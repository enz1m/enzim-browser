use super::Result;
use enzui_security::secret::SecretText;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) enabled: bool,
    pub(crate) provider: String,
    pub(crate) offer_save: bool,
    pub(crate) autofill: bool,
    pub(crate) private_fill: bool,
    pub(crate) lock_minutes: u16,
    pub(crate) desktop_unlock: bool,
    pub(crate) storage: super::accounts::Storage,
    pub(crate) system_store: bool,
    pub(crate) browser_store: bool,
    pub(crate) accounts: Vec<super::accounts::Connection>,
    pub(crate) save_to: String,
    pub(crate) preferred_source: String,
    pub(crate) setup_provider: Option<super::accounts::Kind>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "enzim.builtin".into(),
            offer_save: true,
            autofill: true,
            private_fill: false,
            lock_minutes: 5,
            desktop_unlock: true,
            storage: super::accounts::Storage::Browser,
            system_store: false,
            browser_store: true,
            accounts: Vec::new(),
            save_to: String::new(),
            preferred_source: String::new(),
            setup_provider: None,
        }
    }
}
impl Settings {
    #[cfg(test)]
    pub(crate) fn enabled_defaults() -> Self {
        Self {
            enabled: true,
            provider: "enzim.builtin".into(),
            offer_save: true,
            autofill: true,
            private_fill: false,
            lock_minutes: 5,
            desktop_unlock: false,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Secret(SecretText);
impl Secret {
    pub(crate) fn new(value: String) -> Self {
        Self(SecretText::new(value))
    }
    pub(crate) fn expose(&self) -> &str {
        self.0.expose_secret()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl Default for Secret {
    fn default() -> Self {
        Self::new(String::new())
    }
}
impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.expose())
    }
}
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Site {
    pub(crate) origin: String,
    pub(crate) container: Option<u64>,
    #[serde(skip)]
    pub(crate) url: Option<Secret>,
}
impl PartialEq for Site {
    fn eq(&self, other: &Self) -> bool {
        self.origin == other.origin && self.container == other.container
    }
}
impl Eq for Site {}
impl Site {
    pub(crate) fn parse(input: &str, container: Option<u64>) -> Result<Self> {
        let url = url::Url::parse(input).map_err(|_| "Enter a valid HTTPS website.".to_owned())?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(
                "Passwords are available only for HTTPS websites without URL credentials.".into(),
            );
        }
        Ok(Self {
            origin: url.origin().ascii_serialization(),
            container,
            url: Some(Secret::new(url.into())),
        })
    }
    pub(crate) fn matches(&self, other: &Self) -> bool {
        self == other
    }
    pub(crate) fn matches_page(&self, other: &Self) -> bool {
        self.matches(other)
            && self
                .url
                .as_ref()
                .zip(other.url.as_ref())
                .is_none_or(|(left, right)| left == right)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Credential {
    pub(crate) id: String,
    pub(crate) site: Site,
    pub(crate) name: Secret,
    pub(crate) username: Secret,
    pub(crate) password: Secret,
    pub(crate) totp: Option<super::Totp>,
    pub(crate) notes: Secret,
    pub(crate) modified: i64,
    /// The passkey of this account, if the website gave it one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) passkey: Option<Box<super::passkey::Passkey>>,
    #[serde(skip)]
    pub(crate) remote_uri: Option<RemoteUri>,
}
#[derive(Clone)]
pub(crate) struct RemoteUri {
    pub(crate) original: Secret,
    pub(crate) edited: Secret,
}
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credential([redacted])")
    }
}
impl Credential {
    pub(crate) fn website(&self) -> Secret {
        self.remote_uri.as_ref().map_or_else(
            || {
                self.site
                    .url
                    .clone()
                    .unwrap_or_else(|| Secret::new(self.site.origin.clone()))
            },
            |uri| uri.edited.clone(),
        )
    }
    pub(crate) fn set_website(&mut self, value: &str) -> Result<()> {
        self.site = Site::parse(value, self.site.container)?;
        if let Some(uri) = &mut self.remote_uri {
            uri.edited = Secret::new(value.trim().into());
        }
        Ok(())
    }
    pub(crate) fn update_password(&mut self, submitted: &Self) -> Result<()> {
        if !self.site.matches(&submitted.site) || self.username != submitted.username {
            return Err(
                "The submitted login belongs to another website, account or container.".into(),
            );
        }
        let mut validation = self.clone();
        if validation.id.starts_with("ref:") {
            validation.id = super::router::split_reference(&validation.id)?.1;
        }
        validation.validate()?;
        submitted.validate()?;
        self.password = submitted.password.clone();
        self.modified = chrono::Utc::now().timestamp();
        Ok(())
    }
    pub(crate) fn new(site: Site) -> Result<Self> {
        use base64::Engine;
        Ok(Self {
            id: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(super::random::<16>()?),
            name: Secret::new(site.origin.clone()),
            site,
            username: Secret::default(),
            password: Secret::default(),
            totp: None,
            notes: Secret::default(),
            modified: chrono::Utc::now().timestamp(),
            passkey: None,
            remote_uri: None,
        })
    }
    pub(crate) fn validate(&self) -> Result<()> {
        let site = Site::parse(&self.site.origin, self.site.container)?;
        if site != self.site {
            return Err("The login website is invalid.".into());
        }
        self.validate_contents()?;
        if let Some(uri) = &self.remote_uri
            && Site::parse(uri.edited.expose(), self.site.container)? != self.site
        {
            return Err("The edited website does not match this login.".into());
        }
        Ok(())
    }
    /// Bounds for explicit remote-item reads; saving and automatic fill still require a valid site.
    pub(super) fn validate_contents(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 128
            || self.name.expose().len() > 512
            || self.username.expose().len() > 4096
            || self.password.expose().len() > 4096
            || self.notes.expose().len() > 16_384
        {
            return Err("The login contains invalid or oversized fields.".into());
        }
        if let Some(totp) = &self.totp {
            totp.validate()?;
        }
        if let Some(passkey) = &self.passkey {
            passkey.validate()?;
        }
        if self.remote_uri.as_ref().is_some_and(|uri| {
            uri.original.expose().len() > 8192 || uri.edited.expose().len() > 8192
        }) {
            return Err("The login website is oversized.".into());
        }
        Ok(())
    }
}
