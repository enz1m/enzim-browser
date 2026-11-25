use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

// Local documents retain opaque origins, including when they load sibling assets.
const HEADERS: &str = "Cache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads; object-src 'none'\r\nAccept-Ranges: bytes\r\nConnection: close\r\n";

pub(crate) fn file_path(uri: &str) -> Option<PathBuf> {
    let parsed = url::Url::parse(uri).ok()?;
    if parsed.scheme() != "file" || parsed.query().is_some() {
        return None;
    }
    parsed.to_file_path().ok()
}
pub(crate) fn file_uri(path: &Path) -> Option<String> {
    url::Url::from_file_path(path).ok().map(Into::into)
}
pub(crate) fn mime(path: &Path) -> Option<&'static str> {
    Some(
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "htm" | "html" => "text/html",
            "xhtml" | "xht" => "application/xhtml+xml",
            "xml" => "application/xml",
            "rdf" => "application/rdf+xml",
            "rss" => "application/rss+xml",
            "atom" => "application/atom+xml",
            "json" | "map" => "application/json",
            "txt" | "text" | "log" | "md" | "csv" => "text/plain",
            "pdf" => "application/pdf",
            "apng" => "image/apng",
            "avif" => "image/avif",
            "bmp" => "image/bmp",
            "gif" => "image/gif",
            "jpg" | "jpeg" | "jpe" => "image/jpeg",
            "png" => "image/png",
            "svg" => "image/svg+xml",
            "webp" => "image/webp",
            "ico" => "image/vnd.microsoft.icon",
            "flac" => "audio/flac",
            "mp3" => "audio/mpeg",
            "ogg" | "oga" | "opus" => "audio/ogg",
            "wav" => "audio/wav",
            "weba" => "audio/webm",
            "webm" => "video/webm",
            "mp4" | "m4v" => "video/mp4",
            "ogv" => "video/ogg",
            "css" => "text/css",
            "js" | "mjs" => "text/javascript",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "ttf" => "font/ttf",
            "otf" => "font/otf",
            _ => return None,
        },
    )
}
fn content_type(file: &mut File, path: &Path) -> std::io::Result<&'static str> {
    let detected = tree_magic_mini::from_file(file);
    file.seek(SeekFrom::Start(0))?;
    Ok(if detected == Some("application/pdf") {
        "application/pdf"
    } else {
        mime(path)
            .or(detected)
            .unwrap_or("application/octet-stream")
    })
}

#[derive(Default)]
pub(crate) struct Sources {
    servers: HashMap<PathBuf, Server>,
}
pub(crate) enum Opened {
    Pdf(PathBuf),
    Web { uri: String, file_uri: String },
}
impl Sources {
    pub(crate) fn open(&mut self, uri: &str) -> Result<Opened, String> {
        let path = file_path(uri)
            .ok_or("Open a local file URI.")?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.is_file() {
            return Err("The selected document is not a regular file.".into());
        }
        let mut selected = File::open(&path).map_err(|error| error.to_string())?;
        let kind = content_type(&mut selected, &path).map_err(|error| error.to_string())?;
        if !kind.starts_with("text/") && !crate::desktop::MIME_TYPES.contains(&kind) {
            return Err("This file format is not supported by the browser.".into());
        }
        if kind == "application/pdf" {
            return Ok(Opened::Pdf(path));
        }
        let parent = path
            .parent()
            .ok_or("The selected file has no parent directory.")?
            .to_path_buf();
        if !self.servers.contains_key(&parent) {
            if self.servers.len() >= 64 {
                return Err("Close some local document tabs before opening another folder.".into());
            }
            self.servers.insert(parent.clone(), Server::start(&parent)?);
        }
        let server = &self.servers[&parent];
        let mut target = server.url(&path)?;
        if kind.starts_with("audio/") || kind.starts_with("video/") {
            target.push_str("?enzim-media");
        }
        Ok(Opened::Web {
            uri: target,
            file_uri: file_uri(&path).ok_or("Invalid document path.")?,
        })
    }
    pub(crate) fn display_uri(&self, uri: &str) -> Option<String> {
        self.servers
            .values()
            .find_map(|server| server.display_uri(uri))
    }
    /// The origins serving `uri`, a local document's `file:` address.
    pub(crate) fn origins_of(&self, uri: &str) -> Vec<String> {
        self.servers
            .iter()
            .filter(|(root, _)| {
                url::Url::from_directory_path(root)
                    .is_ok_and(|prefix| uri.starts_with(prefix.as_str()))
            })
            .map(|(_, server)| server.origin())
            .collect()
    }
    pub(crate) fn retain<'a>(&mut self, uris: impl Iterator<Item = &'a str>) {
        let uris: Vec<_> = uris.collect();
        self.servers.retain(|root, server| {
            let prefix = url::Url::from_directory_path(root).ok().map(String::from);
            uris.iter().any(|uri| {
                prefix
                    .as_ref()
                    .is_some_and(|prefix| uri.starts_with(prefix))
                    || uri.starts_with(&server.origin())
            })
        });
    }
}
struct State {
    root: File,
    path: PathBuf,
    address: SocketAddr,
    token: String,
    cancelled: AtomicBool,
    clients: AtomicUsize,
}
struct Server {
    state: Arc<State>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.state.cancelled.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.state.address, Duration::from_millis(20));
    }
}
impl Server {
    fn start(path: &Path) -> Result<Self, String> {
        let root = File::open(path).map_err(|error| error.to_string())?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| error.to_string())?;
        let mut random = [0; 32];
        getrandom::fill(&mut random).map_err(|error| error.to_string())?;
        let token = random
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>();
        let state = Arc::new(State {
            root,
            path: path.into(),
            address: listener.local_addr().map_err(|error| error.to_string())?,
            token,
            cancelled: AtomicBool::new(false),
            clients: AtomicUsize::new(0),
        });
        let owner = Arc::clone(&state);
        std::thread::Builder::new()
            .name("local-documents".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                while !owner.cancelled.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if owner.cancelled.load(Ordering::Acquire) {
                                break;
                            }
                            if owner.clients.fetch_add(1, Ordering::AcqRel) >= 8 {
                                owner.clients.fetch_sub(1, Ordering::AcqRel);
                                continue;
                            }
                            let client = Arc::clone(&owner);
                            if std::thread::Builder::new()
                                .name("local-document-read".into())
                                .stack_size(256 * 1024)
                                .spawn(move || {
                                    let _ = serve(stream, &client);
                                    client.clients.fetch_sub(1, Ordering::AcqRel);
                                })
                                .is_err()
                            {
                                owner.clients.fetch_sub(1, Ordering::AcqRel);
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { state })
    }
    fn origin(&self) -> String {
        format!("http://{}", self.state.address)
    }
    fn url(&self, path: &Path) -> Result<String, String> {
        let relative = path
            .strip_prefix(&self.state.path)
            .map_err(|error| error.to_string())?;
        let mut uri = url::Url::parse(&format!("{}/{}/", self.origin(), self.state.token))
            .map_err(|error| error.to_string())?;
        let mut parts = uri
            .path_segments_mut()
            .map_err(|()| "Invalid document origin.")?;
        parts.pop_if_empty();
        for component in relative.components() {
            parts.push(
                component
                    .as_os_str()
                    .to_str()
                    .ok_or("The document path is not UTF-8.")?,
            );
        }
        drop(parts);
        Ok(uri.into())
    }
    fn display_uri(&self, uri: &str) -> Option<String> {
        let parsed = url::Url::parse(uri).ok()?;
        if parsed.origin().ascii_serialization() != self.origin() {
            return None;
        }
        let relative = request_path(parsed.path(), &self.state.token)?;
        let mut display = url::Url::from_file_path(self.state.path.join(relative)).ok()?;
        display.set_fragment(parsed.fragment());
        Some(display.into())
    }
}
fn request_path(raw: &str, token: &str) -> Option<PathBuf> {
    let uri = url::Url::parse(&format!("file://{raw}")).ok()?;
    let decoded = uri.to_file_path().ok()?;
    let relative = decoded.strip_prefix(Path::new("/").join(token)).ok()?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    Some(relative.into())
}
fn confined_file(state: &State, path: &Path) -> std::io::Result<File> {
    // The kernel checks every path component under the pinned directory descriptor.
    let fd = rustix::fs::openat2(
        &state.root,
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOCTTY,
        rustix::fs::Mode::empty(),
        rustix::fs::ResolveFlags::BENEATH
            | rustix::fs::ResolveFlags::NO_SYMLINKS
            | rustix::fs::ResolveFlags::NO_MAGICLINKS,
    )?;
    let file = File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("Not a regular file."));
    }
    Ok(file)
}
fn error_response(stream: &mut TcpStream, code: u16) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code} Refused\r\n{HEADERS}Content-Length: 0\r\n\r\n"
    )
}
fn byte_range(value: Option<&str>, size: u64) -> Option<(u64, u64)> {
    let Some(value) = value else {
        return Some((0, size));
    };
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    if start.is_empty() {
        let count = end.parse::<u64>().ok()?.min(size);
        return (count > 0).then_some((size - count, count));
    }
    let start = start.parse::<u64>().ok()?;
    let end = if end.is_empty() {
        size.checked_sub(1)?
    } else {
        end.parse::<u64>().ok()?.min(size.checked_sub(1)?)
    };
    (start <= end && start < size).then_some((start, end - start + 1))
}
fn media_document(path: &str, kind: &str) -> String {
    let source = path
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let tag = if kind.starts_with("video/") {
        "video"
    } else {
        "audio"
    };
    format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>Media</title><style>:root{{color-scheme:light dark}}body{{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;background:Canvas}}video{{max-width:100vw;max-height:100vh}}audio{{width:min(480px,90vw)}}</style><{tag} controls preload=metadata src=\"{source}\"></{tag}>"
    )
}

fn serve(mut stream: TcpStream, state: &State) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut input = Vec::with_capacity(1024);
    let mut buffer = [0; 1024];
    while !input.windows(4).any(|window| window == b"\r\n\r\n") {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Ok(());
        }
        input.extend_from_slice(&buffer[..count]);
        if input.len() > 16 * 1024 {
            return error_response(&mut stream, 431);
        }
    }
    let Ok(request) = std::str::from_utf8(&input) else {
        return error_response(&mut stream, 400);
    };
    let mut lines = request.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split_whitespace();
    let method = first.next().unwrap_or_default();
    let target = first.next().unwrap_or_default();
    if !matches!(method, "GET" | "HEAD") {
        return error_response(&mut stream, 405);
    }
    let mut host = None;
    let mut range = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("host") {
                if host.is_some() {
                    return error_response(&mut stream, 400);
                }
                host = Some(value.trim());
            }
            if key.eq_ignore_ascii_case("range") {
                if range.is_some() {
                    return error_response(&mut stream, 416);
                }
                range = Some(value.trim());
            }
        }
    }
    if host != Some(state.address.to_string().as_str()) || state.cancelled.load(Ordering::Acquire) {
        return error_response(&mut stream, 403);
    }
    let Some(path) = request_path(target.split('?').next().unwrap_or_default(), &state.token)
    else {
        return error_response(&mut stream, 403);
    };
    let Ok(mut file) = confined_file(state, &path) else {
        return error_response(&mut stream, 404);
    };
    let size = file.metadata()?.len();
    let mime = content_type(&mut file, &path)?;
    // A media document avoids WebKit's preliminary standalone-media load failure.
    // Its element streams the original resource, including normal byte ranges.
    if target.ends_with("?enzim-media")
        && (mime.starts_with("audio/") || mime.starts_with("video/"))
    {
        let document = media_document(target.split('?').next().unwrap_or_default(), mime);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\n{HEADERS}Content-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
            document.len()
        )?;
        if method != "HEAD" {
            stream.write_all(document.as_bytes())?;
        }
        return Ok(());
    }
    let Some((start, count)) = byte_range(range, size) else {
        return error_response(&mut stream, 416);
    };
    write!(
        stream,
        "HTTP/1.1 {} OK\r\n{HEADERS}Content-Type: {mime}\r\nContent-Length: {count}\r\n",
        if range.is_some() { 206 } else { 200 }
    )?;
    if range.is_some() {
        write!(
            stream,
            "Content-Range: bytes {}-{}/{size}\r\n",
            start,
            start + count - 1
        )?;
    }
    stream.write_all(b"\r\n")?;
    if method == "HEAD" {
        return Ok(());
    }
    file.seek(SeekFrom::Start(start))?;
    let mut reader = file.take(count);
    let mut chunk = vec![0; 64 * 1024];
    while !state.cancelled.load(Ordering::Acquire) {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        stream.write_all(&chunk[..count])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let mut nonce = [0; 16];
            getrandom::fill(&mut nonce).expect("fixture randomness");
            let root = std::env::temp_dir()
                .join(format!("enzim-open-with-{:x}", u128::from_ne_bytes(nonce)));
            std::fs::create_dir(&root).expect("fixture directory");
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn request(uri: &str, headers: &str) -> Vec<u8> {
        let uri = url::Url::parse(uri).expect("fixture URL");
        let host = format!(
            "{}:{}",
            uri.host_str().expect("host"),
            uri.port().expect("port")
        );
        let mut socket = TcpStream::connect(&host).expect("document server");
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("read deadline");
        let target = format!(
            "{}{}",
            uri.path(),
            uri.query()
                .map_or_else(String::new, |query| format!("?{query}"))
        );
        write!(
            socket,
            "GET {target} HTTP/1.1\r\nHost: {host}\r\n{headers}\r\n"
        )
        .expect("request");
        let mut response = Vec::new();
        socket.read_to_end(&mut response).expect("response");
        response
    }
    #[test]
    fn desktop_open_with_files_keep_relative_assets_stream_ranges_and_confine_reads() {
        let fixture = Fixture::new();
        let root = fixture.0.join("site");
        std::fs::create_dir(&root).expect("site directory");
        let page = root.join("report #é.html");
        std::fs::write(
            &page,
            "<!doctype html><title>Selected report</title><link rel=stylesheet href=theme.css>",
        )
        .expect("HTML");
        std::fs::write(root.join("theme.css"), "body{color:green}").expect("CSS");
        std::fs::write(root.join("media.webm"), b"0123456789").expect("range fixture");
        let secret = fixture.0.join("outside-secret.txt");
        std::fs::write(&secret, "OUTSIDE-CANARY").expect("outside file");
        std::os::unix::fs::symlink(&secret, root.join("escape.txt")).expect("escape symlink");
        let mut sources = Sources::default();
        let Opened::Web {
            uri,
            file_uri: display,
            ..
        } = sources
            .open(&file_uri(&page).expect("file URI"))
            .expect("open document")
        else {
            panic!("HTML must use the web renderer");
        };
        assert_eq!(sources.display_uri(&uri), Some(display));
        let response = request(&uri, "");
        let text = String::from_utf8_lossy(&response);
        assert!(text.starts_with("HTTP/1.1 200"));
        assert!(text.contains("Selected report"));
        assert!(text.contains("Content-Security-Policy: sandbox allow-scripts"));
        assert!(!text.contains("allow-same-origin"));
        assert!(!text.contains("Access-Control-Allow-Origin:"));
        assert!(text.contains("Referrer-Policy: no-referrer"));
        let base = url::Url::parse(&uri).expect("URL");
        let css = request(base.join("theme.css").expect("asset URL").as_str(), "");
        assert!(String::from_utf8_lossy(&css).contains("Content-Type: text/css\r\n"));
        assert!(css.ends_with(b"body{color:green}"));
        let part = request(
            base.join("media.webm").expect("media URL").as_str(),
            "Range: bytes=2-5\r\n",
        );
        assert!(part.starts_with(b"HTTP/1.1 206"));
        assert!(part.ends_with(b"2345"));
        let Opened::Web { uri: media, .. } = sources
            .open(&file_uri(&root.join("media.webm")).expect("media URI"))
            .expect("open media")
        else {
            panic!("media document")
        };
        assert!(media.ends_with("?enzim-media"));
        let document = String::from_utf8(request(&media, "")).expect("media HTML");
        assert!(document.contains("Content-Type: text/html; charset=utf-8"));
        assert!(document.contains("<video controls preload=metadata src=\"/"));
        assert!(!document.contains("autoplay"));
        assert!(
            !document.contains("?enzim-media\""),
            "the media element must stream the original resource"
        );
        for relative in [
            "escape.txt",
            "../outside-secret.txt",
            "%2e%2e/%2e%2e/outside-secret.txt",
        ] {
            let response = request(base.join(relative).expect("negative URL").as_str(), "");
            assert!(!response.starts_with(b"HTTP/1.1 200"), "refuse {relative}");
            assert!(!String::from_utf8_lossy(&response).contains("OUTSIDE-CANARY"));
        }
        let wrong = request(
            &uri.replace(
                base.path().split('/').nth(1).expect("token"),
                "wrong-capability",
            ),
            "",
        );
        assert!(wrong.starts_with(b"HTTP/1.1 403"));
        sources.retain(std::iter::once("https://example.test/"));
        assert!(
            sources.servers.is_empty(),
            "leaving local documents revokes the folder capability"
        );
    }
    #[test]
    fn desktop_open_with_pdf_and_plain_documents_use_their_existing_renderers() {
        let fixture = Fixture::new();
        let pdf = fixture.0.join("document.pdf");
        std::fs::write(&pdf, b"%PDF-1.4\n").expect("PDF");
        let mut sources = Sources::default();
        assert!(matches!(
            sources.open(&file_uri(&pdf).expect("URI")),
            Ok(Opened::Pdf(_))
        ));
        assert!(sources.servers.is_empty());
        let unnamed_pdf = fixture.0.join("extensionless-document");
        std::fs::write(&unnamed_pdf, b"%PDF-1.4\n").expect("extensionless PDF");
        assert!(matches!(
            sources.open(&file_uri(&unnamed_pdf).expect("URI")),
            Ok(Opened::Pdf(_))
        ));
        let archive = fixture.0.join("archive.zip");
        std::fs::write(&archive, b"PK\x03\x04\0\0\0\0\0\0\0\0\0\0\0\0")
            .expect("unsupported archive");
        assert!(sources.open(&file_uri(&archive).expect("URI")).is_err());
        let plain = fixture.0.join("README");
        std::fs::write(&plain, "Text without an extension").expect("text");
        let Opened::Web { uri, .. } = sources
            .open(&file_uri(&plain).expect("URI"))
            .expect("plain file")
        else {
            panic!("plain document");
        };
        assert!(String::from_utf8_lossy(&request(&uri, "")).contains("Content-Type: text/plain"));
        let css = fixture.0.join("theme.css");
        std::fs::write(&css, "body{color:green}").expect("text subtype");
        assert!(
            matches!(
                sources.open(&file_uri(&css).expect("URI")),
                Ok(Opened::Web { .. })
            ),
            "text/plain desktop associations include text subtypes"
        );
        assert!(sources.open("file://remote.invalid/document.html").is_err());
        assert!(
            sources
                .open(&file_uri(&fixture.0).expect("directory URI"))
                .is_err()
        );
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    #[test]
    fn desktop_open_with_batch_keeps_background_operands_lazy_and_uncontained() {
        let mut app = crate::browser::browser_tests::app();
        app.browser.private = true;
        app.browser.saved.tabs[0].container = Some(app.browser.saved.containers[0].id);
        let urls = vec![
            "https://example.test/first".to_owned(),
            "https://example.test/second".to_owned(),
            "https://example.test/last".to_owned(),
        ];
        drop(app.open_external_urls(urls.clone()));
        assert_eq!(app.browser.saved.tabs.len(), 4);
        assert_eq!(app.browser.saved.active, 3);
        for (offset, url) in urls.iter().enumerate() {
            let index = offset + 1;
            assert_eq!(&app.browser.saved.tabs[index].url, url);
            assert!(app.browser.saved.tabs[index].container.is_none());
            if index < 3 {
                assert!(
                    app.browser.sleeping[index].is_none(),
                    "background operands must not allocate or release native pages"
                );
            }
        }
        assert!(
            matches!(&app.pending_navigation, Some(crate::webview_native::BrowserAction::Navigate(url)) if url == &urls[2])
        );
    }

    #[test]
    fn desktop_open_with_routes_selected_files_without_global_file_or_loopback_grants() {
        let root = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .expect("isolated test cache")
            .join("desktop-routing");
        std::fs::create_dir_all(&root).expect("fixture directory");
        let path = root.join("selected.html");
        std::fs::write(&path, "<title>Local</title>").expect("fixture");
        let mut app = crate::browser::browser_tests::app();
        app.browser.private = true;
        let uri = file_uri(&path).expect("file URI");
        app.submit_browser_action(crate::webview_native::BrowserAction::Navigate(uri.clone()));
        assert_eq!(app.address, uri);
        assert_eq!(app.browser.saved.tabs[0].url, uri);
        assert!(
            app.browser.local_grants.is_empty(),
            "file opening must not mint a profile-wide loopback grant"
        );
        let Some(crate::webview_native::BrowserAction::Navigate(transport)) =
            &app.pending_navigation
        else {
            panic!("web document is queued for the native renderer");
        };
        assert!(transport.starts_with("http://127.0.0.1:"));
        assert_eq!(app.local_files.display_uri(transport), Some(uri));
        let pdf = root.join("selected.pdf");
        std::fs::write(&pdf, b"%PDF-1.4\n").expect("PDF fixture");
        let uri = file_uri(&pdf).expect("PDF URI");
        app.submit_browser_action(crate::webview_native::BrowserAction::Navigate(uri.clone()));
        assert_eq!(
            app.browser.saved.tabs.len(),
            1,
            "PDF uses the requested tab"
        );
        assert_eq!(app.address, uri);
        assert!(app.pending_navigation.is_none());
        assert_eq!(
            app.browser.pdf_requests[&app.browser.saved.tabs[0].key].path,
            pdf
        );
    }
}
