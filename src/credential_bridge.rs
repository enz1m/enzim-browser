use crate::credentials::{Credential, Result, Secret, Site};
use enzui_security::secret::SecretText;
use serde::Deserialize;
use zeroize::Zeroizing;

pub(crate) const WORLD: &str = "enzim-credential-provider";
pub(crate) const HANDLER: &str = "enzimCredentials";
pub(crate) const SOURCE: &str = include_str!("credential_bridge.js");

#[derive(Clone, Debug, serde::Serialize, Deserialize)]
pub(crate) struct Document {
    pub(crate) document: String,
    pub(crate) origin: String,
    pub(crate) target: String,
    #[serde(default)]
    pub(crate) username_only: bool,
}
impl Document {
    pub(crate) fn valid_for(&self, site: &Site) -> bool {
        self.origin == site.origin
            && self.target.len() <= 20
            && self.target.bytes().all(|byte| byte.is_ascii_digit())
            && self.document.len() == 32
            && self.document.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FieldFocus {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) viewport_width: f32,
    pub(crate) viewport_height: f32,
}
impl FieldFocus {
    pub(crate) fn anchor(&self, viewport: iced::Rectangle) -> Option<iced::Rectangle> {
        let values = [
            self.x,
            self.y,
            self.width,
            self.height,
            self.viewport_width,
            self.viewport_height,
        ];
        if values.iter().any(|value| !value.is_finite())
            || self.width < 8.
            || self.height < 8.
            || self.viewport_width < 16.
            || self.viewport_height < 16.
            || self.viewport_width > 65536.
            || self.viewport_height > 65536.
        {
            return None;
        }
        let sx = viewport.width / self.viewport_width;
        let sy = viewport.height / self.viewport_height;
        iced::Rectangle {
            x: viewport.x + self.x * sx,
            y: viewport.y + self.y * sy,
            width: self.width * sx,
            height: self.height * sy,
        }
        .intersection(&viewport)
        .filter(|rect| rect.width >= 8. && rect.height >= 8.)
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub(crate) enum Event {
    #[serde(rename = "ready")]
    Ready {
        document: String,
        origin: String,
        action: String,
        target: String,
        password: bool,
        otp: bool,
        #[serde(default)]
        username_only: bool,
        #[serde(default)]
        focus: Option<FieldFocus>,
    },
    #[serde(rename = "dismiss")]
    Dismiss { document: String, origin: String },
    #[serde(rename = "submitted")]
    Submitted {
        document: String,
        origin: String,
        action: String,
        username: Secret,
        password: Secret,
    },
}
impl Event {
    pub(crate) fn parse(json: &SecretText) -> Result<Self> {
        if json.expose_secret().len() > 32_768 {
            return Err("The credential message is too large.".into());
        }
        serde_json::from_str(json.expose_secret())
            .map_err(|_| "The credential message is invalid.".into())
    }
}

pub(crate) fn fill_script(
    document: &Document,
    site: &Site,
    login: &Credential,
    code: bool,
) -> Result<SecretText> {
    fill_script_inner(document, site, login, code, false)
}

/// Explicit chrome selection may override origin matching, never document or container binding.
pub(crate) fn fill_selected_script(
    document: &Document,
    site: &Site,
    login: &Credential,
    code: bool,
) -> Result<SecretText> {
    fill_script_inner(document, site, login, code, true)
}

fn fill_script_inner(
    document: &Document,
    site: &Site,
    login: &Credential,
    code: bool,
    selected: bool,
) -> Result<SecretText> {
    if !document.valid_for(site)
        || login.site.container != site.container
        || (!selected && !login.site.matches(site))
    {
        return Err("The website or container changed. Choose the login again.".into());
    }
    if document.username_only && !code && login.username.is_empty() {
        return Err("This saved login has no username or email address to fill.".into());
    }
    if !document.username_only && !code && login.password.is_empty() {
        return Err("This saved login has no password to fill.".into());
    }
    let otp = if code {
        Some(
            login
                .totp
                .as_ref()
                .ok_or("This login has no authenticator.")?
                .code(chrono::Utc::now().timestamp().max(0) as u64)?
                .0,
        )
    } else {
        None
    };
    let mut source = Zeroizing::new(String::from("globalThis.__enzimCredentials?.fill("));
    source.push_str(
        &serde_json::to_string(document).map_err(|_| "The form document is invalid.".to_owned())?,
    );
    for value in [
        (!code).then_some(&login.username),
        (!code && !document.username_only).then_some(&login.password),
        otp.as_ref(),
    ] {
        source.push(',');
        let encoded = Zeroizing::new(
            match value {
                Some(secret) => serde_json::to_string(secret),
                None => Ok("null".into()),
            }
            .map_err(|_| "The login could not be prepared for autofill.".to_owned())?,
        );
        source.push_str(&encoded);
    }
    source.push_str(") === true");
    Ok(SecretText::new(std::mem::take(&mut *source)))
}

#[cfg(test)]
mod password_review_tests {
    use super::*;
    #[test]
    fn password_review_manual_selection_overrides_only_origin_not_container_or_document() {
        let target = Site::parse("https://signin.example.test", Some(7)).expect("target");
        let mut login =
            Credential::new(Site::parse("https://accounts.example.test", Some(7)).expect("source"))
                .expect("login");
        login.username = Secret::new("review-user".into());
        login.password = Secret::new("synthetic-review-password".into());
        let mut document = Document {
            document: "0123456789abcdef0123456789abcdef".into(),
            origin: target.origin.clone(),
            target: "1".into(),
            username_only: false,
        };
        assert!(
            fill_script(&document, &target, &login, false).is_err(),
            "automatic matching stays strict"
        );
        assert!(
            fill_selected_script(&document, &target, &login, false).is_ok(),
            "explicit selection supports mismatched saved URLs"
        );
        login.site.container = Some(8);
        assert!(
            fill_selected_script(&document, &target, &login, false).is_err(),
            "container boundary survives manual selection"
        );
        login.site.container = Some(7);
        document.origin = "https://changed.example.test".into();
        assert!(
            fill_selected_script(&document, &target, &login, false).is_err(),
            "stale target cannot receive secrets"
        );
    }
}
