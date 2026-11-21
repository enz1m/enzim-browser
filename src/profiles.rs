use serde::{Deserialize, Serialize};
use std::io::Read;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Container {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) color: usize,
    #[serde(default)]
    pub(crate) icon: String,
    #[serde(default)]
    pub(crate) privacy: crate::container_privacy::Overrides,
}

impl Container {
    pub(crate) fn create(name: &str, color: usize) -> Result<Self, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err("Use a container name between 1 and 60 characters.".into());
        }
        let mut bytes = [0; 8];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|e| format!("Could not create a container identity: {e}"))?;
        Ok(Self {
            id: u64::from_le_bytes(bytes).max(1),
            name: name.into(),
            color: color % 8,
            icon: default_icon(),
            privacy: Default::default(),
        })
    }

    pub(crate) fn storage_name(&self) -> String {
        format!("container-{:016x}", self.id)
    }
}

pub(crate) fn defaults() -> Vec<Container> {
    ["Personal", "Work", "Banking", "Shopping"]
        .into_iter()
        .enumerate()
        .filter_map(|(color, name)| {
            Container::create(name, color).ok().map(|mut entry| {
                entry.icon = ICONS[color].0.into();
                entry
            })
        })
        .collect()
}

pub(crate) const ICONS: [(&str, &str); 12] = [
    ("user-round", "Personal"),
    ("briefcase-business", "Work"),
    ("landmark", "Banking"),
    ("shopping-bag", "Shopping"),
    ("code", "Development"),
    ("graduation-cap", "Study"),
    ("heart", "Health"),
    ("plane", "Travel"),
    ("gamepad-2", "Games"),
    ("message-circle", "Social"),
    ("flask-conical", "Research"),
    ("shield-check", "Private projects"),
];
fn default_icon() -> String {
    "user-round".into()
}
impl Container {
    pub(crate) fn icon_name(&self) -> &'static str {
        ICONS
            .iter()
            .find(|(name, _)| *name == self.icon)
            .or_else(|| {
                ICONS
                    .iter()
                    .find(|(_, label)| label.eq_ignore_ascii_case(&self.name))
            })
            .map_or("user-round", |(name, _)| *name)
    }
}
