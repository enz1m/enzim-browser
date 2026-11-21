use enzui_core::packaging::AppPackage;

pub(crate) const APP_ID: &str = "org.enzim.Browser";
pub(crate) const MIME_TYPES: &[&str] = &[
    "text/html",
    "application/xhtml+xml",
    "application/xml",
    "text/xml",
    "application/rdf+xml",
    "application/rss+xml",
    "application/atom+xml",
    "application/json",
    "text/plain",
    "application/pdf",
    "image/apng",
    "image/avif",
    "image/bmp",
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/svg+xml",
    "image/webp",
    "image/vnd.microsoft.icon",
    "image/x-icon",
    "audio/flac",
    "audio/mpeg",
    "audio/ogg",
    "audio/wav",
    "audio/x-wav",
    "audio/webm",
    "video/mp4",
    "video/ogg",
    "video/webm",
    "x-scheme-handler/http",
    "x-scheme-handler/https",
];

#[allow(
    clippy::expect_used,
    reason = "The package identity contains only valid compile-time constants."
)]
pub(crate) fn package() -> AppPackage {
    AppPackage::new(
        APP_ID,
        "enzim Browser",
        "Explore the web with enzUI",
        "enzim-browser-launch",
    )
    .expect("static browser package identity")
    .categories(["Network", "WebBrowser"])
    .keywords(["browser", "web", "internet"])
    .mime_types(MIME_TYPES.iter().copied())
}
pub(crate) fn desktop_entry() -> String {
    format!(
        "{}TryExec=enzim-browser-launch\n",
        package().desktop_entry()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_open_with_entry_matches_framework_identity_and_uri_contract() {
        assert_eq!(
            include_str!("../resources/org.enzim.Browser.desktop"),
            desktop_entry()
        );
        assert!(desktop_entry().contains("Exec=enzim-browser-launch %U\n"));
        assert!(desktop_entry().contains(&format!("StartupWMClass={APP_ID}\n")));
        assert!(MIME_TYPES.contains(&"application/pdf"));
        assert!(MIME_TYPES.contains(&"text/html"));
        let unique = MIME_TYPES.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), MIME_TYPES.len());
    }
}
