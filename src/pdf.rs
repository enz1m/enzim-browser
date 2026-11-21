use std::path::{Path, PathBuf};
#[derive(Debug, Clone)]
pub(crate) struct Page {
    pub(crate) path: PathBuf,
    pub(crate) number: u32,
    pub(crate) count: u32,
    pub(crate) image: iced::widget::image::Handle,
    pub(crate) points: (f32, f32),
    pub(crate) raster: u32,
    pub(crate) view: ViewState,
    stamp: Stamp,
    bytes: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Fit {
    Width,
    #[default]
    Page,
    Custom,
}
#[derive(Debug, Clone)]
pub(crate) struct ViewState {
    pub(crate) fit: Fit,
    pub(crate) percent: i32,
    pub(crate) turns: u8,
    pub(crate) page_input: String,
    pub(crate) error: String,
    pub(crate) scroll: iced::Vector,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            fit: Fit::Page,
            percent: 100,
            turns: 0,
            page_input: "1".into(),
            error: String::new(),
            scroll: iced::Vector::ZERO,
        }
    }
}
impl Page {
    pub(crate) fn scale(&self, size: iced::Size) -> f32 {
        let (w, h) = if self.view.turns % 2 == 1 {
            (self.points.1, self.points.0)
        } else {
            self.points
        };
        match self.view.fit {
            Fit::Custom => self.view.percent as f32 / 100.,
            Fit::Width => (size.width - 32.).max(24.) / (w * 96. / 72.),
            Fit::Page => ((size.width - 32.).max(24.) / (w * 96. / 72.))
                .min((size.height - 32.).max(24.) / (h * 96. / 72.)),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl Stamp {
    fn read(path: &Path) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(path).map_err(|e| e.to_string())?;
        Ok(Self {
            dev: m.dev(),
            ino: m.ino(),
            len: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        })
    }
}
#[derive(Default)]
pub(crate) struct Cache(std::sync::Mutex<std::collections::VecDeque<Page>>);
impl Cache {
    fn get(&self, path: &Path, number: u32, raster: u32, stamp: &Stamp) -> Option<Page> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .rev()
            .find(|p| {
                p.path == path && p.number == number && p.raster >= raster && &p.stamp == stamp
            })
            .cloned()
    }
    fn remember(&self, page: &Page) {
        const BUDGET: usize = 64 * 1024 * 1024;
        if page.bytes > BUDGET {
            return;
        }
        let mut pages = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        pages.retain(|p| p.path != page.path || (p.stamp == page.stamp && p.number != page.number));
        while pages.len() >= 8 || pages.iter().map(|p| p.bytes).sum::<usize>() + page.bytes > BUDGET
        {
            pages.pop_front();
        }
        pages.push_back(page.clone());
    }
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn parser(
    program: &str,
    path: &Path,
    cache: &Path,
    args: &[String],
) -> Result<std::process::Output, String> {
    std::process::Command::new("bwrap")
        .args([
            "--die-with-parent",
            "--unshare-all",
            "--new-session",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib",
            "/lib64",
            "--ro-bind",
            "/etc/fonts",
            "/etc/fonts",
            "--ro-bind",
            "/etc/ld.so.cache",
            "/etc/ld.so.cache",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--ro-bind",
        ])
        .arg(path)
        .arg("/document.pdf")
        .arg("--bind")
        .arg(cache)
        .arg("/output")
        .args([
            "--setenv",
            "HOME",
            "/tmp",
            "--setenv",
            "XDG_CACHE_HOME",
            "/tmp",
            "--unsetenv",
            "LD_LIBRARY_PATH",
            "--unsetenv",
            "LD_PRELOAD",
            "--",
            "/usr/bin/prlimit",
            "--as=1610612736",
            "--cpu=20",
            "--fsize=67108864",
            "--",
            program,
        ])
        .args(args)
        .output()
        .map_err(|e| format!("PDF sandbox: {e}"))
}
fn parser_error(output: &std::process::Output) -> String {
    if String::from_utf8_lossy(&output.stderr)
        .to_ascii_lowercase()
        .contains("password")
    {
        "This PDF is password protected. Open it in a document reader to enter its password.".into()
    } else {
        "This PDF could not be opened. It may be damaged or incomplete. Try downloading it again."
            .into()
    }
}
#[cfg(test)]
pub(crate) fn render(path: PathBuf, number: u32) -> Result<Page, String> {
    render_pending(
        path,
        number,
        1800,
        &std::sync::atomic::AtomicBool::new(true),
    )
}
#[cfg(test)]
pub(crate) fn render_pending(
    path: PathBuf,
    number: u32,
    raster: u32,
    alive: &std::sync::atomic::AtomicBool,
) -> Result<Page, String> {
    render_cached(path, number, raster, alive, &Cache::default())
}
pub(crate) fn render_cached(
    path: PathBuf,
    number: u32,
    raster: u32,
    alive: &std::sync::atomic::AtomicBool,
    cache: &Cache,
) -> Result<Page, String> {
    if !alive.load(std::sync::atomic::Ordering::Acquire) {
        return Err("Document request superseded".into());
    }
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let stamp = Stamp::read(&path)?;
    let raster = raster.clamp(1200, 4096);
    if let Some(page) = cache.get(&path, number, raster, &stamp) {
        return Ok(page);
    }
    // Poppler jobs share a parser lane so rapid navigation cannot multiply the native memory ceiling.
    static PARSER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = PARSER
        .lock()
        .map_err(|_| "The document renderer stopped unexpectedly")?;
    if !alive.load(std::sync::atomic::Ordering::Acquire) {
        return Err("Document request superseded".into());
    }
    if let Some(page) = cache.get(&path, number, raster, &stamp) {
        return Ok(page);
    }
    use std::io::Read;
    let mut header = [0; 1024];
    let length = std::fs::File::open(&path)
        .and_then(|mut f| f.read(&mut header))
        .map_err(|_| "This PDF could not be read. Check that the file is still available.")?;
    if !header[..length].windows(5).any(|word| word == b"%PDF-") {
        return Err("This file is not a PDF document. Choose a file ending in .pdf.".into());
    }
    let cache_path = std::env::temp_dir().join(format!(
        "enzim-browser-pdf-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir(&cache_path).map_err(|e| e.to_string())?;
    let scratch = Scratch(cache_path);
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&scratch.0, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    let requested = number.max(1);
    let mut info = parser(
        "/usr/bin/pdfinfo",
        &path,
        &scratch.0,
        &[
            "-f".into(),
            requested.to_string(),
            "-l".into(),
            requested.to_string(),
            "/document.pdf".into(),
        ],
    )?;
    if !info.status.success() && requested > 1 {
        info = parser(
            "/usr/bin/pdfinfo",
            &path,
            &scratch.0,
            &["/document.pdf".into()],
        )?;
    }
    if !info.status.success() {
        return Err(parser_error(&info));
    }
    let count = String::from_utf8_lossy(&info.stdout)
        .lines()
        .find_map(|l| {
            l.strip_prefix("Pages:")
                .and_then(|n| n.trim().parse::<u32>().ok())
        })
        .filter(|n| *n > 0)
        .ok_or("PDF page count missing")?;
    let number = requested.min(count);
    let geometry = if requested > count && number > 1 {
        parser(
            "/usr/bin/pdfinfo",
            &path,
            &scratch.0,
            &[
                "-f".into(),
                number.to_string(),
                "-l".into(),
                number.to_string(),
                "/document.pdf".into(),
            ],
        )?
    } else {
        info
    };
    let mut points = String::from_utf8_lossy(&geometry.stdout)
        .lines()
        .find_map(|line| {
            line.split_once("size:").and_then(|(_, rest)| {
                let values: Vec<_> = rest.split_whitespace().collect();
                Some((
                    values.first()?.parse::<f32>().ok()?,
                    values.get(2)?.parse::<f32>().ok()?,
                ))
            })
        })
        .filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0. && *h > 0.)
        .unwrap_or((595., 842.));
    let result = parser(
        "/usr/bin/pdftoppm",
        &path,
        &scratch.0,
        &[
            "-f".into(),
            number.to_string(),
            "-singlefile".into(),
            "-scale-to".into(),
            raster.to_string(),
            "/document.pdf".into(),
            "/output/page".into(),
        ],
    )?;
    if !result.status.success() {
        return Err(parser_error(&result));
    }
    let data = std::fs::read(scratch.0.join("page.ppm")).map_err(|e| e.to_string())?;
    let (width, height, pixels) = decode_ppm(&data, raster)?;
    let bytes = pixels.len();
    let image = iced::widget::image::Handle::from_rgba(width, height, pixels);
    let width = width as f32;
    let height = height as f32;
    if (points.0 / points.1 - width / height).abs() > (points.1 / points.0 - width / height).abs() {
        points = (points.1, points.0);
    }
    let page = Page {
        stamp,
        bytes,
        points,
        raster,
        view: ViewState {
            page_input: number.to_string(),
            ..Default::default()
        },
        path,
        number,
        count,
        image,
    };
    if alive.load(std::sync::atomic::Ordering::Acquire) {
        cache.remember(&page);
    }
    Ok(page)
}
fn decode_ppm(data: &[u8], limit: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let mut at = 0;
    fn token<'a>(data: &'a [u8], at: &mut usize) -> Option<&'a [u8]> {
        loop {
            while data.get(*at).is_some_and(u8::is_ascii_whitespace) {
                *at += 1;
            }
            if data.get(*at) != Some(&b'#') {
                break;
            }
            while data.get(*at).is_some_and(|b| *b != b'\n') {
                *at += 1;
            }
        }
        let start = *at;
        while data.get(*at).is_some_and(|b| !b.is_ascii_whitespace()) {
            *at += 1;
        }
        (start < *at).then_some(&data[start..*at])
    }
    if token(data, &mut at) != Some(b"P6") {
        return Err("The PDF renderer returned an invalid image".into());
    }
    let mut number = || {
        token(data, &mut at)
            .and_then(|v| std::str::from_utf8(v).ok())
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or("Invalid PDF image dimensions")
    };
    let width = number()?;
    let height = number()?;
    let maximum = number()?;
    if width == 0 || height == 0 || width > limit || height > limit || maximum != 255 {
        return Err("The PDF image exceeds its rendering bounds".into());
    }
    if data.get(at) == Some(&b'\r') && data.get(at + 1) == Some(&b'\n') {
        at += 2;
    } else if data.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    } else {
        return Err("Invalid PDF image separator".into());
    }
    let size = width as usize * height as usize;
    if data.len().saturating_sub(at) != size * 3 {
        return Err("Incomplete PDF image".into());
    }
    let mut rgba = Vec::with_capacity(size * 4);
    for rgb in data[at..].chunks_exact(3) {
        rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    Ok((width, height, rgba))
}
#[cfg(feature = "benchmark")]
pub(crate) fn benchmark(path: PathBuf) -> Result<serde_json::Value, String> {
    let cache = Cache::default();
    let alive = std::sync::atomic::AtomicBool::new(true);
    let mut results = Vec::new();
    for number in [1, 2, 1, 2] {
        let now = std::time::Instant::now();
        let page = render_cached(path.clone(), number, 1800, &alive, &cache)?;
        results
            .push(serde_json::json!({"page":page.number,"ms":now.elapsed().as_secs_f64()*1000.}));
    }
    Ok(serde_json::json!(results))
}
#[cfg(test)]
mod tests {
    #[test]
    fn browser_tests_pdf_raw_image_bounds_and_binary_separator() {
        let mut image = b"P6\n# Poppler\n1 1\n255\n".to_vec();
        image.extend_from_slice(&[10, 32, 255]);
        let (width, height, rgba) = super::decode_ppm(&image, 4096).unwrap();
        assert_eq!((width, height), (1, 1));
        assert_eq!(rgba, [10, 32, 255, 255]);
        assert!(super::decode_ppm(b"P6\n4097 1\n255\n", 4096).is_err());
        assert!(super::decode_ppm(&image[..image.len() - 1], 4096).is_err());
        assert!(super::decode_ppm(b"P6\n1 1\n65535\n123456", 4096).is_err());
    }
    #[test]
    fn browser_tests_pdf_cache_reuses_pixels_and_invalidates_replaced_documents() {
        let source =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-pages.pdf");
        let dir = std::env::temp_dir().join(format!("enzim-pdf-cache-test-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let cleanup = super::Scratch(dir.clone());
        let path = dir.join("original.pdf");
        std::fs::copy(source, &path).unwrap();
        let cache = super::Cache::default();
        let alive = std::sync::atomic::AtomicBool::new(true);
        let first = super::render_cached(path.clone(), 1, 1200, &alive, &cache).unwrap();
        let warm = super::render_cached(path.clone(), 1, 1200, &alive, &cache).unwrap();
        assert_eq!(first.image.id(), warm.image.id());
        assert!(cache.get(&first.path, 1, 4096, &first.stamp).is_none());
        let replacement = dir.join("replacement.pdf");
        std::fs::write(&replacement, b"not a pdf").unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert!(
            super::render_cached(path, 1, 1200, &alive, &cache)
                .unwrap_err()
                .contains("not a PDF")
        );
        drop(cleanup);
    }

    #[test]
    fn browser_tests_pdf_rejects_non_pdf_with_readable_error() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/browser.html");
        assert_eq!(
            super::render(path, 1).unwrap_err(),
            "This file is not a PDF document. Choose a file ending in .pdf."
        );
    }
    #[test]
    fn browser_tests_pdf_renders_and_navigates_real_document() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/two-pages.pdf");
        let first = super::render(path.clone(), 1).expect("render first PDF page");
        assert_eq!((first.number, first.count), (1, 2));
        let scale = first.scale(iced::Size::new(400., 128.));
        assert!(
            first.points.1 * 96. / 72. * scale + 32. <= 128.1,
            "Fit page must include its padding in a compact viewport"
        );
        let second = super::render(path, 2).expect("render second PDF page");
        assert_eq!((second.number, second.count), (2, 2));
        assert_ne!(first.image.id(), second.image.id());
    }
}
#[cfg(test)]
mod request_tests {
    #[test]
    fn browser_tests_superseded_pdf_does_not_enter_the_parser() {
        let result = super::render_pending(
            "/missing/cancelled.pdf".into(),
            1,
            4096,
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert_eq!(result.unwrap_err(), "Document request superseded");
    }
}
