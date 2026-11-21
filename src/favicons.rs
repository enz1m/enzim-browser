use base64::Engine;
use iced::widget::image::Handle;
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashMap, path::PathBuf};

pub(crate) struct Cache {
    entries: RefCell<HashMap<String, Option<Handle>>>,
    directory: Option<PathBuf>,
    persist: bool,
}
impl Cache {
    pub(crate) fn new(private: bool) -> Self {
        Self {
            entries: RefCell::new(HashMap::new()),
            directory: (!cfg!(test))
                .then(|| crate::browser_profile::paths().cache.join("favicons")),
            persist: !private,
        }
    }
    pub(crate) fn origin(address: &str) -> Option<String> {
        let url = url::Url::parse(address).ok()?;
        matches!(url.scheme(), "https" | "http").then(|| url.origin().ascii_serialization())
    }
    fn file(&self, origin: &str) -> Option<PathBuf> {
        self.directory
            .as_ref()
            .map(|dir| dir.join(format!("{:x}.png", Sha256::digest(origin.as_bytes()))))
    }
    fn decode(bytes: &[u8]) -> Option<Handle> {
        if bytes.len() > 64 * 1024 {
            return None;
        }
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let (w, h) = reader.into_dimensions().ok()?;
        if w == 0 || h == 0 || w > 64 || h > 64 {
            return None;
        }
        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .ok()?
            .into_rgba8();
        Some(Handle::from_rgba(
            image.width(),
            image.height(),
            image.into_raw(),
        ))
    }
    fn remember(&self, origin: String, handle: Option<Handle>) {
        let mut entries = self.entries.borrow_mut();
        if entries.len() >= 2048 {
            entries.clear();
        }
        entries.insert(origin, handle);
    }
    pub(crate) fn get(&self, address: &str) -> Option<Handle> {
        let origin = Self::origin(address)?;
        if let Some(value) = self.entries.borrow().get(&origin) {
            return value.clone();
        }
        let value = self.file(&origin).and_then(|file| {
            let metadata = std::fs::symlink_metadata(&file).ok()?;
            if !metadata.is_file() || metadata.len() > 64 * 1024 {
                return None;
            }
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(file)
                .ok()?
                .take(64 * 1024 + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            Self::decode(&bytes)
        });
        self.remember(origin, value.clone());
        value
    }
    pub(crate) fn receive(&self, address: &str, json: &str) -> Option<Handle> {
        if json.len() > 100_000 {
            return None;
        }
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let origin = Self::origin(address)?;
        if Self::origin(value["url"].as_str()?)? != origin {
            return None;
        }
        let encoded = value["data"]
            .as_str()?
            .strip_prefix("data:image/png;base64,")?;
        if encoded.len() > 90_000 {
            return None;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .ok()?;
        self.store(&origin, &bytes)
    }
    fn store(&self, origin: &str, bytes: &[u8]) -> Option<Handle> {
        let handle = Self::decode(bytes)?;

        self.remember(origin.to_owned(), Some(handle.clone()));
        if self.persist
            && let Some(file) = self.file(origin)
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            if let Some(dir) = file.parent()
                && std::fs::create_dir_all(dir).is_ok()
            {
                let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
                let temporary = file.with_extension(format!("{}.tmp", std::process::id()));
                if let Ok(mut output) = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&temporary)
                {
                    use std::io::Write;
                    if output.write_all(bytes).is_ok() {
                        let _ = std::fs::rename(&temporary, &file);
                    }
                    let _ = std::fs::remove_file(temporary);
                }
                if let Ok(files) = std::fs::read_dir(dir) {
                    let mut files: Vec<_> = files
                        .filter_map(Result::ok)
                        .filter(|entry| entry.path().extension().is_some_and(|e| e == "png"))
                        .collect();
                    if files.len() > 2048 {
                        files.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());
                        let excess = files.len() - 2048;
                        for old in files.into_iter().take(excess) {
                            let _ = std::fs::remove_file(old.path());
                        }
                    }
                }
            }
        }
        Some(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn favicon_cache_reopens_without_tabs_and_private_icons_stay_memory_only() {
        let directory = std::env::temp_dir().join(format!(
            "enzim-icons-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let cache = |private| {
            let mut cache = Cache::new(private);
            cache.directory = Some(directory.clone());
            cache
        };
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(32, 32, image::Rgba([12, 48, 170, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("PNG");
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes.into_inner());
        let message = |url: &str| {
            serde_json::json!({"url":url,"data":format!("data:image/png;base64,{encoded}")})
                .to_string()
        };
        let regular = cache(false);
        assert!(
            regular
                .receive("https://example.org/", &message("https://example.com/"))
                .is_none()
        );
        assert!(
            regular
                .receive("https://example.com/a", &message("https://example.com/b"))
                .is_some()
        );
        drop(regular);
        assert!(cache(false).get("https://example.com/history").is_some());
        let private = cache(true);
        assert!(
            private
                .receive(
                    "https://private.example/",
                    &message("https://private.example/")
                )
                .is_some()
        );
        assert!(private.get("https://private.example/").is_some());
        drop(private);
        assert!(cache(false).get("https://private.example/").is_none());
        assert!(cache(false).get("https://example.com:8443/").is_none());
        std::fs::remove_dir_all(directory).expect("cleanup");
    }
}
