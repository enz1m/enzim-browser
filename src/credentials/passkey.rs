//! Passkeys: the authenticator behind the page's `navigator.credentials`.
//!
//! The engine is built without WebAuthn, so the page-world script in
//! `passkey_page.js` implements the API and relays each call here. Everything
//! a relying party relies on is decided on this side: the origin comes from
//! the engine's own record of the calling frame, never from the page; the
//! relying party ID is checked against it; the client data is written here;
//! and the private key never leaves the vault it is stored in except to sign.
//!
//! Credentials are ES256 (P-256, SHA-256) with `none` attestation, in the
//! shape Bitwarden stores them, so one saved in a Bitwarden account works in
//! Bitwarden's own clients and the other way round.

use super::{Result, Secret};
use base64::Engine;
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The page-world relay's handler and isolated world.
pub(crate) const HANDLER: &str = "enzimPasskeys";
pub(crate) const WORLD: &str = "enzim-passkeys";
pub(crate) const PAGE_SOURCE: &str = include_str!("../passkey_page.js");
pub(crate) const BRIDGE_SOURCE: &str = include_str!("../passkey_bridge.js");

/// COSE algorithm identifier of ES256, the one algorithm offered.
const ES256: i64 = -7;
const FLAG_USER_PRESENT: u8 = 0x01;
const FLAG_USER_VERIFIED: u8 = 0x04;
const FLAG_BACKUP_ELIGIBLE: u8 = 0x08;
const FLAG_BACKED_UP: u8 = 0x10;
const FLAG_ATTESTED: u8 = 0x40;
/// Bitwarden's authenticator identifier, used for credentials kept in a
/// Bitwarden account, and this browser's own for its vault and the keyring.
const BITWARDEN_AAGUID: [u8; 16] = [
    0xd5, 0x48, 0x82, 0x6e, 0x79, 0xb4, 0xdb, 0x40, 0xa3, 0xd8, 0x11, 0x11, 0x6f, 0x7e, 0x83, 0x49,
];
const BROWSER_AAGUID: [u8; 16] = [
    0x8f, 0x2a, 0x6b, 0x1c, 0x4d, 0x3e, 0x4a, 0x59, 0xb7, 0xc0, 0x65, 0x6e, 0x7a, 0x69, 0x6d, 0x42,
];
const MAX_MESSAGE: usize = 64 * 1024;
const MAX_CHALLENGE: usize = 16 * 1024;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
/// Decodes base64url, and the standard alphabet and padding other clients
/// sometimes write.
fn unb64(text: &str) -> Option<Vec<u8>> {
    let text: String = text
        .trim_end_matches('=')
        .chars()
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            other => other,
        })
        .collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text)
        .ok()
}

/// A passkey kept with a login. Byte strings are base64url without padding.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Passkey {
    pub(crate) credential_id: String,
    pub(crate) rp_id: String,
    #[serde(default)]
    pub(crate) rp_name: String,
    pub(crate) user_handle: String,
    #[serde(default)]
    pub(crate) user_name: String,
    #[serde(default)]
    pub(crate) user_display_name: String,
    /// The PKCS#8 private key.
    pub(crate) key: Secret,
    #[serde(default)]
    pub(crate) counter: u32,
    #[serde(default)]
    pub(crate) created: i64,
}
impl std::fmt::Debug for Passkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Passkey([redacted])")
    }
}
impl Passkey {
    pub(crate) fn validate(&self) -> Result<()> {
        let id = unb64(&self.credential_id).unwrap_or_default();
        let user = unb64(&self.user_handle);
        if id.is_empty()
            || id.len() > 1023
            || !valid_rp_syntax(&self.rp_id)
            || user.is_none_or(|user| user.len() > 64)
            || self.rp_name.len() > 512
            || self.user_name.len() > 512
            || self.user_display_name.len() > 512
            || self.signing_key().is_err()
        {
            return Err("The login's passkey is invalid.".into());
        }
        Ok(())
    }
    fn signing_key(&self) -> Result<SigningKey> {
        let der = zeroize::Zeroizing::new(
            unb64(self.key.expose()).ok_or("The passkey's private key is unreadable.")?,
        );
        SigningKey::from_pkcs8_der(&der)
            .map_err(|_| "The passkey's private key is unreadable.".to_owned())
    }

    /// Whether `signature` (ASN.1) over `signed` verifies with this
    /// passkey's public key.
    #[cfg(test)]
    pub(crate) fn verifies(&self, signed: &[u8], signature: &[u8]) -> bool {
        use p256::ecdsa::signature::Verifier;
        self.signing_key().is_ok_and(|key| {
            Signature::from_der(signature)
                .is_ok_and(|signature| key.verifying_key().verify(signed, &signature).is_ok())
        })
    }

    /// The credential as Bitwarden's item JSON carries it.
    pub(crate) fn to_bitwarden(&self) -> serde_json::Value {
        let id = unb64(&self.credential_id).unwrap_or_default();
        let credential_id = match <[u8; 16]>::try_from(id.as_slice()) {
            Ok(raw) => guid(&raw),
            Err(_) => format!("b64.{}", self.credential_id),
        };
        serde_json::json!({
            "credentialId": credential_id,
            "keyType": "public-key",
            "keyAlgorithm": "ECDSA",
            "keyCurve": "P-256",
            "keyValue": self.key.expose(),
            "rpId": self.rp_id,
            "userHandle": self.user_handle,
            "userName": self.user_name,
            "counter": self.counter.to_string(),
            "rpName": self.rp_name,
            "userDisplayName": self.user_display_name,
            "discoverable": "true",
            "creationDate": chrono::DateTime::from_timestamp(self.created, 0)
                .unwrap_or_default()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        })
    }
    /// Reads one of a Bitwarden item's `fido2Credentials`; `None` for a key
    /// type this authenticator does not sign with or a malformed entry.
    pub(crate) fn from_bitwarden(value: &serde_json::Value) -> Option<Self> {
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_owned();
        if value["keyType"] != "public-key"
            || value["keyAlgorithm"] != "ECDSA"
            || value["keyCurve"] != "P-256"
        {
            return None;
        }
        let id = value["credentialId"].as_str()?;
        let credential_id = match id.strip_prefix("b64.") {
            Some(encoded) => b64(&unb64(encoded)?),
            None => b64(&unguid(id)?),
        };
        // Counters arrive as text from the CLI and as numbers from exports.
        let counter = value["counter"]
            .as_str()
            .and_then(|counter| counter.parse().ok())
            .or_else(|| value["counter"].as_u64().and_then(|n| u32::try_from(n).ok()))
            .unwrap_or(0);
        let passkey = Self {
            credential_id,
            rp_id: text("rpId"),
            rp_name: text("rpName"),
            user_handle: b64(&unb64(value["userHandle"].as_str().unwrap_or_default())?),
            user_name: text("userName"),
            user_display_name: text("userDisplayName"),
            key: Secret::new(b64(&unb64(value["keyValue"].as_str()?)?)),
            counter,
            created: value["creationDate"]
                .as_str()
                .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok())
                .map_or(0, |date| date.timestamp()),
        };
        passkey.validate().ok().map(|()| passkey)
    }
}

fn guid(raw: &[u8; 16]) -> String {
    let hex = |bytes: &[u8]| data_encoding::HEXLOWER.encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        hex(&raw[..4]),
        hex(&raw[4..6]),
        hex(&raw[6..8]),
        hex(&raw[8..10]),
        hex(&raw[10..])
    )
}
fn unguid(text: &str) -> Option<[u8; 16]> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if text.len() != 36 || hex.len() != 32 {
        return None;
    }
    data_encoding::HEXLOWER_PERMISSIVE
        .decode(hex.as_bytes())
        .ok()?
        .try_into()
        .ok()
}

/// Why a request is refused, as the page sees it: a DOMException name and a
/// message that says nothing about what is stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) name: &'static str,
    pub(crate) message: &'static str,
}
impl Failure {
    pub(crate) const NOT_ALLOWED: Self = Self {
        name: "NotAllowedError",
        message: "The request is not allowed by the user agent or the platform in the current context.",
    };
    const SECURITY: Self = Self {
        name: "SecurityError",
        message: "The relying party ID is not a registrable domain suffix of, nor equal to, the current domain.",
    };
    const NOT_SUPPORTED: Self = Self {
        name: "NotSupportedError",
        message: "No supported public key algorithm was requested.",
    };
    const INVALID: Self = Self {
        name: "TypeError",
        message: "The credential options are invalid.",
    };
    pub(crate) const EXCLUDED: Self = Self {
        name: "InvalidStateError",
        message: "The authenticator already holds a credential for this account.",
    };
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Message {
    Create { id: u64, options: CreateOptions },
    Get { id: u64, options: GetOptions },
    Cancel { id: u64 },
}
#[derive(Deserialize)]
struct Entity {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "displayName")]
    display_name: String,
}
#[derive(Deserialize)]
struct Parameter {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    alg: i64,
}
#[derive(Deserialize)]
struct Descriptor {
    #[serde(default)]
    id: String,
}
#[derive(Deserialize)]
struct CreateOptions {
    rp: Entity,
    user: Entity,
    challenge: String,
    #[serde(default, rename = "pubKeyCredParams")]
    parameters: Vec<Parameter>,
    #[serde(default, rename = "excludeCredentials")]
    exclude: Vec<Descriptor>,
}
#[derive(Deserialize)]
struct GetOptions {
    #[serde(default, rename = "rpId")]
    rp_id: Option<String>,
    challenge: String,
    #[serde(default, rename = "allowCredentials")]
    allow: Vec<Descriptor>,
}

/// What the page asked for, bound to the origin the engine recorded for the
/// calling frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    /// The page's own number for the call, echoed in the reply.
    pub(crate) id: u64,
    pub(crate) origin: String,
    pub(crate) rp_id: String,
    challenge: String,
    pub(crate) operation: Operation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Create {
        rp_name: String,
        user_handle: String,
        user_name: String,
        user_display_name: String,
        /// Credential IDs the relying party already has for this account.
        exclude: Vec<String>,
    },
    Get {
        /// The credential IDs the relying party accepts; empty for any.
        allow: Vec<String>,
    },
}
/// A parsed message from the page.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Incoming {
    Request(Box<Request>),
    Cancel(u64),
    /// A request refused before any prompt, with the page's number for it.
    Refused(u64, Failure),
}

fn valid_rp_syntax(rp_id: &str) -> bool {
    !rp_id.is_empty()
        && rp_id.len() <= 253
        && !rp_id.starts_with('.')
        && !rp_id.ends_with('.')
        && rp_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.'))
}

/// Whether `rp_id` may be claimed from `host`: the host itself, or a
/// registrable domain the host is under. A public suffix is never enough, and
/// an address literal is only ever itself.
fn rp_allowed(host: &str, rp_id: &str) -> bool {
    if !valid_rp_syntax(rp_id) {
        return false;
    }
    if host == rp_id {
        return true;
    }
    host.parse::<std::net::IpAddr>().is_err()
        && host
            .strip_suffix(rp_id)
            .is_some_and(|prefix| prefix.ends_with('.'))
        && psl::domain(rp_id.as_bytes()).is_some()
}

impl Incoming {
    /// Parses a message posted by the page's relay. `origin` and `top_origin`
    /// are the engine's; a frame of another origin than its top document is
    /// refused, as is anything but HTTPS.
    pub(crate) fn parse(json: &str, origin: &str, top_origin: &str, main_frame: bool) -> Option<Self> {
        if json.len() > MAX_MESSAGE {
            return None;
        }
        let message: Message = match serde_json::from_str(json) {
            Ok(message) => message,
            Err(error) => {
                super::credential_trace(format_args!(
                    "passkey message not understood: {}",
                    error.to_string().chars().take(160).collect::<String>()
                ));
                // A request that names itself still gets an answer.
                let id = serde_json::from_str::<serde_json::Value>(json).ok()?["id"].as_u64()?;
                return Some(Self::Refused(id, Failure::INVALID));
            }
        };
        let (id, rp, challenge, operation) = match message {
            Message::Cancel { id } => return Some(Self::Cancel(id)),
            Message::Create { id, options } => {
                let supported = options
                    .parameters
                    .iter()
                    .any(|parameter| parameter.kind == "public-key" && parameter.alg == ES256);
                if !supported {
                    return Some(Self::Refused(id, Failure::NOT_SUPPORTED));
                }
                let user = unb64(options.user.id.as_deref().unwrap_or_default());
                if user.as_ref().is_none_or(|user| user.is_empty() || user.len() > 64)
                    || options.exclude.len() > 64
                    || options.rp.name.len() > 512
                    || options.user.name.len() > 512
                    || options.user.display_name.len() > 512
                {
                    return Some(Self::Refused(id, Failure::INVALID));
                }
                (
                    id,
                    options.rp.id,
                    options.challenge,
                    Operation::Create {
                        rp_name: options.rp.name,
                        user_handle: b64(&user.unwrap_or_default()),
                        user_name: options.user.name,
                        user_display_name: options.user.display_name,
                        exclude: options.exclude.into_iter().map(|entry| entry.id).collect(),
                    },
                )
            }
            Message::Get { id, options } => {
                if options.allow.len() > 64 {
                    return Some(Self::Refused(id, Failure::INVALID));
                }
                (
                    id,
                    options.rp_id,
                    options.challenge,
                    Operation::Get {
                        allow: options.allow.into_iter().map(|entry| entry.id).collect(),
                    },
                )
            }
        };
        let named_rp = rp.clone();
        let refused = |failure: Failure| {
            super::credential_trace(format_args!(
                "passkey refused in validation: {failure:?} origin={origin} main_frame={main_frame} rp={named_rp:?}"
            ));
            Some(Self::Refused(id, failure))
        };
        let Ok(url) = url::Url::parse(origin) else {
            return refused(Failure::NOT_ALLOWED);
        };
        let Some(host) = url.host_str().filter(|_| url.scheme() == "https") else {
            return refused(Failure::NOT_ALLOWED);
        };
        if !main_frame && origin != top_origin {
            return refused(Failure::NOT_ALLOWED);
        }
        // WebAuthn sets no upper bound on a challenge, and Google's run past
        // a kilobyte; the bound only keeps the message within MAX_MESSAGE.
        if unb64(&challenge).is_none_or(|challenge| challenge.is_empty() || challenge.len() > MAX_CHALLENGE) {
            return refused(Failure::INVALID);
        }
        let rp_id = rp.unwrap_or_else(|| host.to_owned());
        if !rp_allowed(host, &rp_id) {
            return refused(Failure::SECURITY);
        }
        Some(Self::Request(Box::new(Request {
            id,
            origin: url.origin().ascii_serialization(),
            rp_id,
            challenge,
            operation,
        })))
    }
}

/// Where a new credential is kept, which decides how it describes itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keeper {
    /// The browser's vault or the system keyring: on this device only.
    Local,
    /// A Bitwarden account: synchronized.
    Bitwarden,
}

impl Request {
    fn client_data(&self, kind: &str) -> Result<Vec<u8>> {
        // The member order is the one the specification's serialization gives.
        let text = |value: &str| {
            serde_json::to_string(value).map_err(|_| "The client data is invalid.".to_owned())
        };
        Ok(format!(
            "{{\"type\":{},\"challenge\":{},\"origin\":{},\"crossOrigin\":false}}",
            text(kind)?,
            text(&self.challenge)?,
            text(&self.origin)?
        )
        .into_bytes())
    }
    fn authenticator_data(&self, flags: u8, counter: u32, attested: &[u8]) -> Vec<u8> {
        let mut data = Sha256::digest(self.rp_id.as_bytes()).to_vec();
        data.push(flags);
        data.extend_from_slice(&counter.to_be_bytes());
        data.extend_from_slice(attested);
        data
    }
    fn flags(keeper: Keeper) -> u8 {
        // The user unlocked the store and confirmed this request in the
        // browser's own prompt, which is the verification this authenticator
        // has to offer.
        FLAG_USER_PRESENT
            | FLAG_USER_VERIFIED
            | match keeper {
                Keeper::Local => 0,
                Keeper::Bitwarden => FLAG_BACKUP_ELIGIBLE | FLAG_BACKED_UP,
            }
    }

    /// Whether `passkey` answers this request.
    pub(crate) fn accepts(&self, passkey: &Passkey) -> bool {
        passkey.rp_id == self.rp_id
            && match &self.operation {
                Operation::Get { allow } => {
                    allow.is_empty() || allow.iter().any(|id| unb64(id) == unb64(&passkey.credential_id))
                }
                Operation::Create { .. } => false,
            }
    }
    /// Makes a new credential for a registration and the reply for the page.
    pub(crate) fn create(&self, keeper: Keeper) -> Result<(Passkey, serde_json::Value)> {
        let Operation::Create {
            rp_name,
            user_handle,
            user_name,
            user_display_name,
            ..
        } = &self.operation
        else {
            return Err("This request does not register a credential.".into());
        };
        let key = loop {
            let seed = zeroize::Zeroizing::new(super::random::<32>()?);
            if let Ok(key) = SigningKey::from_slice(&seed[..]) {
                break key;
            }
        };
        let private = key
            .to_pkcs8_der()
            .map_err(|_| "The new key could not be encoded.".to_owned())?;
        let credential_id = super::random::<16>()?;
        let passkey = Passkey {
            credential_id: b64(&credential_id),
            rp_id: self.rp_id.clone(),
            rp_name: rp_name.clone(),
            user_handle: user_handle.clone(),
            user_name: user_name.clone(),
            user_display_name: user_display_name.clone(),
            key: Secret::new(b64(private.as_bytes())),
            counter: 0,
            created: chrono::Utc::now().timestamp(),
        };
        let point = key.verifying_key().to_sec1_point(false);
        let (x, y) = point
            .x()
            .zip(point.y())
            .ok_or("The new key has no public point.")?;
        // COSE_Key: {1: 2 (EC2), 3: -7 (ES256), -1: 1 (P-256), -2: x, -3: y}.
        let mut cose = vec![0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20];
        cose.extend_from_slice(x);
        cose.extend_from_slice(&[0x22, 0x58, 0x20]);
        cose.extend_from_slice(y);
        let mut attested = match keeper {
            Keeper::Local => BROWSER_AAGUID,
            Keeper::Bitwarden => BITWARDEN_AAGUID,
        }
        .to_vec();
        attested.extend_from_slice(&(credential_id.len() as u16).to_be_bytes());
        attested.extend_from_slice(&credential_id);
        attested.extend_from_slice(&cose);
        let data = self.authenticator_data(Self::flags(keeper) | FLAG_ATTESTED, 0, &attested);
        // {"fmt": "none", "attStmt": {}, "authData": bytes}, keys in canonical order.
        let mut attestation = vec![0xa3, 0x63];
        attestation.extend_from_slice(b"fmt");
        attestation.push(0x64);
        attestation.extend_from_slice(b"none");
        attestation.push(0x67);
        attestation.extend_from_slice(b"attStmt");
        attestation.push(0xa0);
        attestation.push(0x68);
        attestation.extend_from_slice(b"authData");
        attestation.push(0x59);
        attestation.extend_from_slice(&(data.len() as u16).to_be_bytes());
        attestation.extend_from_slice(&data);
        let public = key
            .verifying_key()
            .to_public_key_der()
            .map_err(|_| "The new public key could not be encoded.".to_owned())?;
        let reply = serde_json::json!({
            "id": passkey.credential_id,
            "clientDataJSON": b64(&self.client_data("webauthn.create")?),
            "attestationObject": b64(&attestation),
            "authenticatorData": b64(&data),
            "publicKey": b64(public.as_bytes()),
            "publicKeyAlgorithm": ES256,
            "transports": ["internal"],
        });
        Ok((passkey, reply))
    }

    /// Signs this request's challenge with `passkey` and returns the reply
    /// for the page.
    pub(crate) fn assert(&self, passkey: &Passkey, keeper: Keeper) -> Result<serde_json::Value> {
        if !self.accepts(passkey) {
            return Err("This passkey does not belong to the requesting website.".into());
        }
        let client_data = self.client_data("webauthn.get")?;
        let data = self.authenticator_data(Self::flags(keeper), passkey.counter, &[]);
        let mut signed = data.clone();
        signed.extend_from_slice(&Sha256::digest(&client_data));
        let signature: Signature = passkey.signing_key()?.sign(&signed);
        Ok(serde_json::json!({
            "id": passkey.credential_id,
            "clientDataJSON": b64(&client_data),
            "authenticatorData": b64(&data),
            "signature": b64(signature.to_der().as_bytes()),
            "userHandle": passkey.user_handle,
        }))
    }
}

/// The script that hands `result` for the page's call `id` to its relay.
pub(crate) fn reply_script(
    id: u64,
    result: std::result::Result<serde_json::Value, Failure>,
) -> enzui_security::secret::SecretText {
    let message = match result {
        Ok(mut value) => {
            value["requestId"] = id.into();
            value["ok"] = true.into();
            value
        }
        Err(failure) => serde_json::json!({
            "requestId": id,
            "ok": false,
            "name": failure.name,
            "message": failure.message,
        }),
    };
    enzui_security::secret::SecretText::new(format!(
        "globalThis.__enzimPasskeys?.reply({message}) === true"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{VerifyingKey, signature::Verifier};
    use p256::pkcs8::DecodePublicKey;

    const ORIGIN: &str = "https://login.example.com";

    fn registration(rp: &str) -> String {
        format!(
            r#"{{"kind":"create","id":7,"options":{{"rp":{{"id":{rp},"name":"Example"}},
            "user":{{"id":"dXNlci0x","name":"alex@example.com","displayName":"Alex"}},
            "challenge":"Y2hhbGxlbmdlLWZvci1jcmVhdGU","pubKeyCredParams":[{{"type":"public-key","alg":-257}},{{"type":"public-key","alg":-7}}]}}}}"#
        )
    }
    fn request(json: &str, origin: &str) -> Request {
        match Incoming::parse(json, origin, origin, true) {
            Some(Incoming::Request(request)) => *request,
            other => panic!("not a request: {other:?}"),
        }
    }
    fn sign_in(allow: &str) -> String {
        format!(
            r#"{{"kind":"get","id":9,"options":{{"rpId":"example.com","challenge":"Y2hhbGxlbmdlLWZvci1nZXQ","allowCredentials":[{allow}]}}}}"#
        )
    }

    #[test]
    fn a_challenge_longer_than_a_kilobyte_is_accepted() {
        // Google signs in with challenges of more than a kilobyte.
        let long = b64(&[7u8; 2048]);
        let json = sign_in("").replace("Y2hhbGxlbmdlLWZvci1nZXQ", &long);
        assert_eq!(request(&json, ORIGIN).challenge, long);
        let too_long = sign_in("").replace("Y2hhbGxlbmdlLWZvci1nZXQ", &b64(&vec![7u8; MAX_CHALLENGE + 1]));
        assert!(matches!(
            Incoming::parse(&too_long, ORIGIN, ORIGIN, true),
            Some(Incoming::Refused(9, _))
        ));
    }

    #[test]
    fn a_relying_party_is_its_host_or_a_registrable_domain_above_it() {
        for (host, rp, allowed) in [
            ("login.example.com", "login.example.com", true),
            ("login.example.com", "example.com", true),
            ("example.com", "login.example.com", false),
            ("login.example.com", "com", false),
            ("login.example.co.uk", "co.uk", false),
            ("login.example.co.uk", "example.co.uk", true),
            ("notexample.com", "example.com", false),
            ("login.example.com", "evil.com", false),
            ("login.example.com", "Example.com", false),
            ("login.example.com", ".example.com", false),
            ("192.168.1.10", "168.1.10", false),
            ("192.168.1.10", "192.168.1.10", true),
            ("user.github.io", "github.io", false),
        ] {
            assert_eq!(rp_allowed(host, rp), allowed, "{rp} from {host}");
        }
    }

    #[test]
    fn a_request_is_bound_to_the_engines_origin_and_refused_outside_it() {
        let bound = request(&registration("\"example.com\""), ORIGIN);
        assert_eq!((bound.id, bound.rp_id.as_str()), (7, "example.com"));
        assert_eq!(bound.origin, ORIGIN);
        // No relying party ID: the calling host.
        assert_eq!(request(&registration("null"), ORIGIN).rp_id, "login.example.com");
        let refused = |json: &str, origin: &str, top: &str, main| match Incoming::parse(json, origin, top, main) {
            Some(Incoming::Refused(_, failure)) => failure.name,
            other => panic!("not refused: {other:?}"),
        };
        assert_eq!(
            refused(&registration("\"other.com\""), ORIGIN, ORIGIN, true),
            "SecurityError",
            "another site's relying party ID"
        );
        assert_eq!(
            refused(&registration("\"example.com\""), "http://login.example.com", ORIGIN, true),
            "NotAllowedError",
            "a page without HTTPS"
        );
        assert_eq!(
            refused(&registration("\"example.com\""), ORIGIN, "https://embedder.test", false),
            "NotAllowedError",
            "a frame inside another site"
        );
        assert_eq!(
            refused(
                &registration("\"example.com\"").replace("\"alg\":-7", "\"alg\":-8"),
                ORIGIN,
                ORIGIN,
                true
            ),
            "NotSupportedError",
            "no ES256 among the algorithms"
        );
        assert_eq!(refused(r#"{"kind":"get","id":3}"#, ORIGIN, ORIGIN, true), "TypeError");
        assert_eq!(
            Incoming::parse(r#"{"kind":"cancel","id":4}"#, ORIGIN, ORIGIN, true),
            Some(Incoming::Cancel(4))
        );
        assert!(Incoming::parse("not json", ORIGIN, ORIGIN, true).is_none());
    }

    #[test]
    fn a_registration_and_a_sign_in_verify_as_a_relying_party_would_check_them() {
        let registration = request(&registration("\"example.com\""), ORIGIN);
        let (passkey, created) = registration.create(Keeper::Local).expect("credential");
        passkey.validate().expect("a valid stored passkey");
        let field = |value: &serde_json::Value, key: &str| unb64(value[key].as_str().expect(key)).expect(key);

        // Registration: client data, authenticator data and attestation.
        let client: serde_json::Value =
            serde_json::from_slice(&field(&created, "clientDataJSON")).expect("client data");
        assert_eq!(client["type"], "webauthn.create");
        assert_eq!(client["origin"], ORIGIN);
        assert_eq!(client["challenge"], "Y2hhbGxlbmdlLWZvci1jcmVhdGU");
        assert_eq!(client["crossOrigin"], false);
        let data = field(&created, "authenticatorData");
        assert_eq!(&data[..32], Sha256::digest(b"example.com").as_slice());
        assert_eq!(data[32], 0x45, "present, verified, attested data; not backed up");
        assert_eq!(&data[33..37], &[0; 4]);
        assert_eq!(&data[37..53], &BROWSER_AAGUID);
        assert_eq!(&data[53..55], &[0, 16]);
        assert_eq!(b64(&data[55..71]), passkey.credential_id);
        assert_eq!(data.len(), 71 + 77);
        let attestation = field(&created, "attestationObject");
        assert!(attestation.ends_with(&data));
        assert_eq!(&attestation[..2], &[0xa3, 0x63]);
        // The public key in the COSE structure is the one handed to the page.
        let public = VerifyingKey::from_public_key_der(&field(&created, "publicKey")).expect("public key");
        let point = public.to_sec1_point(false);
        assert_eq!(&data[81..113], point.x().expect("x").as_slice());
        assert_eq!(&data[116..148], point.y().expect("y").as_slice());

        // Sign-in from a sibling host of the same relying party.
        let sibling = "https://www.example.com";
        let sign_in_request = request(&sign_in(""), sibling);
        let asserted = sign_in_request.assert(&passkey, Keeper::Local).expect("assertion");
        let client_data = field(&asserted, "clientDataJSON");
        let client: serde_json::Value = serde_json::from_slice(&client_data).expect("client data");
        assert_eq!(client["type"], "webauthn.get");
        assert_eq!(client["origin"], sibling);
        let data = field(&asserted, "authenticatorData");
        assert_eq!(data.len(), 37);
        assert_eq!(data[32], 0x05);
        let mut signed = data;
        signed.extend_from_slice(&Sha256::digest(&client_data));
        let signature = Signature::from_der(&field(&asserted, "signature")).expect("signature");
        public.verify(&signed, &signature).expect("the signature verifies");
        assert_eq!(asserted["userHandle"], "dXNlci0x");

        // Another relying party, and a list that does not name the credential.
        let foreign = request(&sign_in("").replace("example.com", "example.org"), "https://example.org");
        assert!(foreign.assert(&passkey, Keeper::Local).is_err());
        let narrowed = request(&sign_in(r#"{"type":"public-key","id":"AAAA"}"#), sibling);
        assert!(!narrowed.accepts(&passkey));
        let named = request(
            &sign_in(&format!(r#"{{"type":"public-key","id":"{}"}}"#, passkey.credential_id)),
            sibling,
        );
        assert!(named.accepts(&passkey));
    }

    #[test]
    fn a_passkey_keeps_its_identity_through_bitwardens_item_format() {
        let (passkey, _) = request(&registration("\"example.com\""), ORIGIN)
            .create(Keeper::Bitwarden)
            .expect("credential");
        let item = passkey.to_bitwarden();
        assert_eq!(item["credentialId"].as_str().expect("id").len(), 36, "a GUID");
        assert_eq!(item["counter"], "0");
        assert_eq!(item["discoverable"], "true");
        assert_eq!(Passkey::from_bitwarden(&item), Some(passkey.clone()));
        // A credential ID that is no GUID, as other clients write it.
        let mut other = item.clone();
        other["credentialId"] = format!("b64.{}", b64(&[7; 32])).into();
        other["counter"] = 3.into();
        let read = Passkey::from_bitwarden(&other).expect("non-GUID credential");
        assert_eq!(read.credential_id, b64(&[7; 32]));
        assert_eq!(read.counter, 3);
        assert_eq!(read.to_bitwarden()["credentialId"], other["credentialId"]);
        // A key this authenticator cannot sign with is not offered.
        let mut unsupported = item;
        unsupported["keyCurve"] = "Ed25519".into();
        assert!(Passkey::from_bitwarden(&unsupported).is_none());
        // Bitwarden's flags say the credential is synchronized.
        let request = request(&sign_in(""), ORIGIN);
        let asserted = request.assert(&passkey, Keeper::Bitwarden).expect("assertion");
        assert_eq!(unb64(asserted["authenticatorData"].as_str().expect("data")).expect("data")[32], 0x1d);
    }

    #[test]
    fn the_reply_script_carries_the_pages_request_number_and_no_stored_detail_on_refusal() {
        let refused = reply_script(12, Err(Failure::NOT_ALLOWED));
        let text = refused.expose_secret();
        assert!(text.starts_with("globalThis.__enzimPasskeys?.reply({"));
        assert!(text.contains("\"requestId\":12") && text.contains("\"ok\":false"));
        assert!(text.contains("NotAllowedError"));
        let accepted = reply_script(13, Ok(serde_json::json!({"id": "abc"})));
        assert!(accepted.expose_secret().contains("\"ok\":true"));
    }
}
