//! Small previews of the links the assistant gives: the page's title,
//! description, site and picture.
//!
//! The model chooses these addresses, so a preview is fetched only from a
//! public address: the name is resolved first, every address must be global,
//! and the connection is pinned to the address that was checked. Redirects
//! are followed by hand, three at most, each checked the same way. Nothing is
//! sent but a plain GET with no cookies.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs as _};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub(crate) struct Preview {
    pub(crate) url: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) site: String,
    pub(crate) image: Option<iced::widget::image::Handle>,
}

const PAGE_LIMIT: usize = 384 * 1024;
const IMAGE_LIMIT: usize = 2 * 1024 * 1024;

/// Fetches `url`'s preview; `None` when it is not a public web page.
pub(crate) fn fetch(url: &str) -> Option<Preview> {
    let (final_url, page) = get(url, PAGE_LIMIT, "text/html")?;
    let html = String::from_utf8_lossy(&page);
    let meta = Meta::parse(&html);
    let host = url::Url::parse(&final_url)
        .ok()?
        .host_str()?
        .trim_start_matches("www.")
        .to_owned();
    let title = meta
        .get("og:title")
        .or_else(|| meta.get("twitter:title"))
        .map(str::to_owned)
        .or_else(|| meta.title.clone())
        .unwrap_or_else(|| host.clone());
    let description = meta
        .get("og:description")
        .or_else(|| meta.get("description"))
        .or_else(|| meta.get("twitter:description"))
        .unwrap_or_default()
        .to_owned();
    let image = meta
        .get("og:image")
        .or_else(|| meta.get("twitter:image"))
        .and_then(|src| url::Url::parse(&final_url).ok()?.join(src).ok())
        .and_then(|src| get(src.as_str(), IMAGE_LIMIT, "image/").map(|(_, bytes)| bytes))
        .and_then(|bytes| thumbnail(&bytes));
    Some(Preview {
        url: url.to_owned(),
        title: clip(&decode(&title), 140),
        description: clip(&decode(&description), 220),
        site: meta
            .get("og:site_name")
            .map_or(host, |site| clip(&decode(site), 60)),
        image,
    })
}

/// A public page's readable text, for the assistant's `read_web_page`.
pub(crate) fn page_text(url: &str) -> Result<serde_json::Value, String> {
    let (final_url, page) = get(url, 1536 * 1024, "text/")
        .ok_or("The page could not be read; only public web pages are.")?;
    let html = String::from_utf8_lossy(&page);
    let title = Meta::parse(&html).title.map(|t| decode(&t)).unwrap_or_default();
    let mut text = readable(&html);
    let total = text.chars().count();
    if total > 24_000 {
        text = text.chars().take(24_000).collect();
        text.push_str("\n… (the rest of the page is left out)");
    }
    Ok(serde_json::json!({"url": final_url, "title": clip(&title, 200), "text": text}))
}

/// The text of an HTML page: no scripts, styles or markup, one line per block.
/// Searches the web with DuckDuckGo's plain results page, without a tab,
/// cookies or sign-ins: up to six results, each a title, address and snippet.
pub(crate) fn search(query: &str) -> Result<serde_json::Value, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Give the words to search for.".into());
    }
    let address = format!(
        "https://html.duckduckgo.com/html/?q={}",
        url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>()
    );
    let (_, page) = get(&address, 1024 * 1024, "text/").ok_or("The web search could not be reached.")?;
    let results = search_results(&page);
    if results.is_empty() {
        return Err("The web search found nothing, or refused the request; try other words or read_web_page.".into());
    }
    Ok(serde_json::json!({"query": query, "results": results}))
}

/// The results of a DuckDuckGo HTML page. Advertisements, whose links go
/// through DuckDuckGo's ad redirect, are left out.
fn search_results(page: &[u8]) -> Vec<serde_json::Value> {
    let lower = page.to_ascii_lowercase();
    let strip = |bytes: &[u8]| -> String {
        let mut out = String::new();
        let mut inside = false;
        for c in text(bytes).chars() {
            match c {
                '<' => inside = true,
                '>' => inside = false,
                c if !inside => out.push(c),
                _ => {}
            }
        }
        clip(&decode(&out), 300)
    };
    let mut results = Vec::new();
    let mut at = 0;
    // Where the previous result's link ended: what follows is this result's block.
    let mut previous = 0;
    while let Some(class) = find(&lower, b"class=\"result__a\"", at) {
        at = class + 1;
        let Some(open) = lower.get(..class).and_then(|head| head.iter().rposition(|&b| b == b'<')) else {
            continue;
        };
        let (Some(tag_end), Some(close)) = (find(&lower, b">", class), find(&lower, b"</a>", class)) else {
            break;
        };
        // An advertisement sits in a block marked as one, and its link leads
        // back through DuckDuckGo.
        let advertised = lower.get(previous..class).is_some_and(|block| find(block, b"result--ad", 0).is_some());
        previous = close;
        let Some(href) = page.get(open..tag_end).and_then(|tag| attribute(tag, b"href")) else {
            continue;
        };
        let target = url::Url::parse(&format!("https:{}", decode(&href)))
            .ok()
            .filter(|u| u.path() == "/l/" && !advertised)
            .and_then(|u| u.query_pairs().find(|(k, _)| k == "uddg").map(|(_, v)| v.into_owned()))
            .filter(|t| {
                url::Url::parse(t).is_ok_and(|u| {
                    matches!(u.scheme(), "http" | "https") && !u.host_str().is_some_and(|h| h == "duckduckgo.com" || h.ends_with(".duckduckgo.com"))
                })
            });
        let Some(target) = target else {
            continue;
        };
        let title = strip(page.get(tag_end + 1..close).unwrap_or_default());
        let next = find(&lower, b"class=\"result__a\"", close).unwrap_or(lower.len());
        let snippet = find(&lower, b"class=\"result__snippet\"", close)
            .filter(|&s| s < next)
            .and_then(|s| {
                let open = find(&lower, b">", s)? + 1;
                let end = find(&lower, b"</a>", open)?;
                Some(strip(page.get(open..end)?))
            })
            .unwrap_or_default();
        results.push(serde_json::json!({"title": title, "url": target, "snippet": snippet}));
        if results.len() == 6 {
            break;
        }
    }
    results
}

fn readable(html: &str) -> String {
    let html = html.as_bytes();
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = find(&lower, b"<body", 0).unwrap_or(0);
    while at < html.len() {
        let Some(open) = find(&lower, b"<", at) else {
            out.extend_from_slice(html.get(at..).unwrap_or_default());
            break;
        };
        out.extend_from_slice(html.get(at..open).unwrap_or_default());
        let rest = lower.get(open + 1..).unwrap_or_default();
        let skip = ["script", "style", "noscript", "svg", "template", "head"]
            .into_iter()
            .find(|tag| rest.starts_with(tag.as_bytes()));
        if let Some(tag) = skip {
            let close = format!("</{tag}");
            at = find(&lower, close.as_bytes(), open).map_or(html.len(), |c| c + close.len());
            at = find(&lower, b">", at).map_or(html.len(), |e| e + 1);
            continue;
        }
        let end = find(&lower, b">", open).map_or(html.len(), |e| e + 1);
        let name: Vec<u8> = rest
            .iter()
            .skip_while(|b| **b == b'/')
            .take_while(|b| b.is_ascii_alphanumeric())
            .copied()
            .collect();
        if matches!(
            name.as_slice(),
            b"p" | b"div" | b"br" | b"li" | b"tr" | b"h1" | b"h2" | b"h3" | b"h4" | b"h5" | b"h6"
                | b"section" | b"article" | b"pre" | b"blockquote" | b"table"
        ) {
            out.push(b'\n');
        }
        at = end;
    }
    decode(&text(&out))
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn clip(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        text
    } else {
        format!("{}…", text.chars().take(max).collect::<String>().trim_end())
    }
}

/// The few entities titles use.
fn decode(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn thumbnail(bytes: &[u8]) -> Option<iced::widget::image::Handle> {
    let picture = image::load_from_memory(bytes).ok()?;
    let picture = picture.thumbnail(480, 270).to_rgba8();
    let (width, height) = picture.dimensions();
    (width >= 16 && height >= 16)
        .then(|| iced::widget::image::Handle::from_rgba(width, height, picture.into_raw()))
}

/// Whether an address is on the public internet.
fn global(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19))
                || a >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return global(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
                || first == 0x2001 && v6.segments()[1] == 0x0db8)
        }
    }
}

/// GETs `url` from a public address, following up to three redirects.
fn get(url: &str, limit: usize, kind: &str) -> Option<(String, Vec<u8>)> {
    let mut current = url::Url::parse(url).ok()?;
    for _ in 0..4 {
        if !matches!(current.scheme(), "http" | "https") || !current.username().is_empty() {
            return None;
        }
        let host = current.host_str()?.trim_matches(['[', ']']).to_owned();
        let port = current.port_or_known_default()?;
        let addresses: Vec<SocketAddr> = (host.as_str(), port).to_socket_addrs().ok()?.collect();
        let address = addresses.first().copied()?;
        if addresses.iter().any(|a| !global(a.ip())) {
            return None;
        }
        let mut easy = curl::easy::Easy::new();
        let mut body = Vec::new();
        let mut content_type = String::new();
        let mut location = None;
        let pin = format!(
            "{}:{port}:{}",
            current.host_str()?,
            match address.ip() {
                IpAddr::V6(v6) => format!("[{v6}]"),
                IpAddr::V4(v4) => v4.to_string(),
            }
        );
        let run = |easy: &mut curl::easy::Easy,
                   body: &mut Vec<u8>,
                   content_type: &mut String,
                   location: &mut Option<String>|
         -> Result<(), curl::Error> {
            let mut resolve = curl::easy::List::new();
            resolve.append(&pin)?;
            easy.resolve(resolve)?;
            easy.url(current.as_str())?;
            easy.follow_location(false)?;
            easy.connect_timeout(Duration::from_secs(5))?;
            easy.timeout(Duration::from_secs(8))?;
            easy.useragent(concat!(
                "Mozilla/5.0 (X11; Linux x86_64) enzim-browser/",
                env!("CARGO_PKG_VERSION"),
                " link preview"
            ))?;
            easy.accept_encoding("")?;
            let mut transfer = easy.transfer();
            transfer.header_function(|header| {
                let header = String::from_utf8_lossy(header);
                if let Some((name, value)) = header.split_once(':') {
                    let value = value.trim().to_owned();
                    match name.trim().to_ascii_lowercase().as_str() {
                        "content-type" => *content_type = value.to_ascii_lowercase(),
                        "location" => *location = Some(value),
                        _ => {}
                    }
                }
                true
            })?;
            transfer.write_function(|data| {
                if body.len() >= limit {
                    return Ok(0);
                }
                let room = limit - body.len();
                body.extend_from_slice(&data[..data.len().min(room)]);
                Ok(data.len())
            })?;
            transfer.perform()
        };
        let result = run(&mut easy, &mut body, &mut content_type, &mut location);
        // Stopping at the limit ends the transfer with a write error.
        if let Err(error) = result
            && !(error.is_write_error() && body.len() >= limit)
        {
            return None;
        }
        let code = easy.response_code().ok()?;
        if (300..400).contains(&code) {
            current = current.join(&location?).ok()?;
            continue;
        }
        if code != 200 || !content_type.starts_with(kind) {
            return None;
        }
        return Some((current.to_string(), body));
    }
    None
}

/// The first place of `needle` in `hay` from `from`, by bytes.
fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|at| from + at)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A page's `<title>` and `<meta>` properties, read without a full parser.
/// It works on bytes, so a place found in the lowercased copy is the same
/// place in the page.
struct Meta {
    title: Option<String>,
    properties: Vec<(String, String)>,
}

impl Meta {
    fn parse(html: &str) -> Self {
        let html = html.as_bytes();
        let head = find(&html.to_ascii_lowercase(), b"</head", 0)
            .and_then(|end| html.get(..end))
            .unwrap_or(html);
        let lower = head.to_ascii_lowercase();
        let title = find(&lower, b"<title", 0).and_then(|start| {
            let open = find(&lower, b">", start)? + 1;
            let close = find(&lower, b"</title", open)?;
            Some(text(head.get(open..close)?).trim().to_owned())
        });
        let mut properties = Vec::new();
        let mut at = 0;
        while let Some(start) = find(&lower, b"<meta", at) {
            let Some(end) = find(&lower, b">", start) else {
                break;
            };
            let tag = head.get(start..end).unwrap_or_default();
            let name = attribute(tag, b"property").or_else(|| attribute(tag, b"name"));
            if let (Some(name), Some(content)) = (name, attribute(tag, b"content")) {
                properties.push((name.to_ascii_lowercase(), content));
            }
            at = end;
        }
        Self { title, properties }
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(n, v)| n == name && !v.trim().is_empty())
            .map(|(_, v)| v.as_str())
    }
}

fn attribute(tag: &[u8], name: &[u8]) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut at = 0;
    while let Some(start) = find(&lower, name, at) {
        at = start + name.len();
        if start > 0 && !lower.get(start - 1).is_some_and(u8::is_ascii_whitespace) {
            continue;
        }
        let mut cursor = at;
        while tag.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if tag.get(cursor) != Some(&b'=') {
            continue;
        }
        cursor += 1;
        while tag.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let value = tag.get(cursor..)?;
        let quote = *value.first()?;
        let value = if quote == b'"' || quote == b'\'' {
            let rest = value.get(1..)?;
            rest.split(|b| *b == quote).next()?
        } else {
            value
                .split(|b| b.is_ascii_whitespace() || *b == b'/')
                .next()?
        };
        return Some(text(value));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_results_are_read_without_ads() {
        let page = br#"<div class="result results_links result--ad"><a rel="nofollow" class="result__a" href="//duckduckgo.com/y.js?ad_domain=shop.example&amp;uddg=https%3A%2F%2Fshop.example%2Ftowers">Buy towers</a></div>
<div class="result results_links result--ad"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fshop.example%2Fcourses">Courses</a></div>
<div class="result"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fduckduckgo.com%2Fy.js%3Fad%3D1">Wrapped ad</a></div>
<div class="result"><h2><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FEiffel_Tower&amp;rut=1">Eiffel Tower - <b>Wikipedia</b></a></h2>
<a class="result__snippet" href="x">Completed in <b>1889</b> as the entrance &amp; arch.</a></div>
<div class="result"><a class="result__a" href="//duckduckgo.com/l/?uddg=javascript%3Aalert(1)">Bad</a></div>"#;
        let results = search_results(page);
        assert_eq!(results.len(), 1, "the advertisement and the script link are left out");
        assert_eq!(results[0]["url"], "https://en.wikipedia.org/wiki/Eiffel_Tower");
        assert_eq!(results[0]["title"], "Eiffel Tower - Wikipedia");
        assert_eq!(results[0]["snippet"], "Completed in 1889 as the entrance & arch.");
    }

    #[test]
    fn page_metadata_is_read() {
        let html = r#"<html><head><title>Plain &amp; simple</title>
            <meta property="og:title" content="The Title">
            <meta name='description' content='What it is about.'>
            <META PROPERTY=og:site_name CONTENT=Example>
            </head><body><meta property="og:image" content="/late.png"></body>"#;
        let meta = Meta::parse(html);
        assert_eq!(meta.title.as_deref(), Some("Plain &amp; simple"));
        assert_eq!(meta.get("og:title"), Some("The Title"));
        assert_eq!(meta.get("description"), Some("What it is about."));
        assert_eq!(meta.get("og:site_name"), Some("Example"));
        assert_eq!(meta.get("og:image"), None, "only the head is read");
        assert_eq!(decode("Plain &amp; simple"), "Plain & simple");
    }

    #[test]
    fn page_text_leaves_markup_and_scripts_out() {
        let html = "<html><head><title>T</title><style>p{}</style></head><body><h1>Hello</h1><p>One &amp; <b>two</b></p><script>var x='<p>no</p>';</script><ul><li>A</li><li>B</li></ul></body></html>";
        assert_eq!(readable(html), "Hello\nOne & two\nA\nB");
    }

    #[test]
    fn only_public_addresses_are_fetched() {
        for private in ["127.0.0.1", "10.1.2.3", "192.168.50.9", "169.254.1.1", "100.64.0.1", "::1", "fd00::1", "fe80::1", "::ffff:127.0.0.1", "0.0.0.0"] {
            assert!(!global(private.parse().expect("ip")), "{private}");
        }
        for public in ["93.184.215.14", "2606:4700::1111", "1.1.1.1"] {
            assert!(global(public.parse().expect("ip")), "{public}");
        }
        assert!(get("http://127.0.0.1:9/", 10, "text/html").is_none(), "loopback refused");
        assert!(get("http://localhost:9/", 10, "text/html").is_none(), "a name for loopback refused");
        assert!(get("file:///etc/passwd", 10, "text/html").is_none(), "other schemes refused");
    }
}
