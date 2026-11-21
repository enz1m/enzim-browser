use super::{Credential, MAX_BYTES, MAX_ENTRIES, Result, crypto};
use enzui_core::state_authority::{CustodySnapshot, PublishCustody, StateAuthority};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Document {
    pub(super) entries: Vec<Credential>,
}
impl Document {
    pub(super) fn validate(&self) -> Result<()> {
        if self.entries.len() > MAX_ENTRIES {
            return Err("The vault supports at most 10,000 logins.".into());
        }
        let mut ids = BTreeSet::new();
        for entry in &self.entries {
            entry.validate()?;
            if !ids.insert(&entry.id) {
                return Err("The vault contains duplicate identities.".into());
            }
        }
        Ok(())
    }
}

pub(crate) struct Vault {
    authority: StateAuthority,
    key: crypto::Key,
    salt: [u8; 16],
    document: Document,
}
impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Vault([redacted])")
    }
}
impl Vault {
    pub(crate) fn default_path() -> PathBuf {
        crate::browser_profile::paths()
            .data
            .join("credentials/vault.enzim")
    }
    pub(crate) fn create(path: &Path, password: &str) -> Result<Self> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        crypto::new_password(password)?;
        super::protect_process()?;
        let parent = path.parent().ok_or("The vault path has no parent.")?;
        enzui_security::secure_dir::ensure_private_dir(parent)
            .map_err(|_| "The private vault folder could not be created safely.".to_owned())?;
        let metadata = std::fs::symlink_metadata(parent)
            .map_err(|_| "The vault folder is unavailable.".to_owned())?;
        if !metadata.is_dir()
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.uid() != rustix::process::geteuid().as_raw()
        {
            return Err("The vault folder must be an owner-only directory.".into());
        }
        let authority = StateAuthority::create_new(path).map_err(|_| {
            "A vault already exists or its private path is unsafe. Unlock the existing vault."
                .to_owned()
        })?;
        let salt = super::random::<16>()?;
        let key = crypto::derive(password, &salt)?;
        let mut vault = Self {
            authority,
            key,
            salt,
            document: Document::default(),
        };
        vault.commit(Document::default())?;
        Ok(vault)
    }
    pub(crate) fn unlock(path: &Path, password: &str) -> Result<Self> {
        Self::open_with(path, |bytes| crypto::open(password, bytes))
    }
    pub(crate) fn desktop_secret(&self) -> enzui_security::secret::Secret {
        use base64::Engine;
        let mut bytes = Zeroizing::new(Vec::with_capacity(56));
        bytes.extend_from_slice(b"ENZKEY01");
        bytes.extend_from_slice(&self.salt);
        bytes.extend_from_slice(&*self.key);
        enzui_security::secret::Secret::new(
            base64::engine::general_purpose::STANDARD
                .encode(&*bytes)
                .into_bytes(),
        )
    }
    pub(crate) fn unlock_desktop(path: &Path, secret: &[u8]) -> Result<Self> {
        use base64::Engine;
        if secret.len() != 76 {
            return Err(
                "The remembered vault key is invalid. Unlock with your vault password.".into(),
            );
        }
        let bytes = Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .decode(secret)
                .map_err(|_| "The remembered vault key is invalid.".to_owned())?,
        );
        if bytes.len() != 56 || &bytes[..8] != b"ENZKEY01" {
            return Err("The remembered vault key is invalid.".into());
        }
        Self::open_with(path, |ciphertext| {
            let salt = crypto::header_salt(ciphertext)?;
            if bytes[8..24] != salt {
                return Err(
                    "The remembered key is outdated. Unlock with your vault password.".into(),
                );
            }
            let key = Zeroizing::new(
                bytes[24..56]
                    .try_into()
                    .map_err(|_| "Invalid remembered key.")?,
            );
            let plaintext = crypto::open_key(&key, ciphertext)?;
            Ok((key, salt, plaintext))
        })
    }
    fn open_with(
        path: &Path,
        unlock: impl FnOnce(&[u8]) -> Result<(crypto::Key, [u8; 16], Zeroizing<Vec<u8>>)>,
    ) -> Result<Self> {
        super::protect_process()?;
        let authority = StateAuthority::open_existing(path).map_err(|_| {
            "The private vault is unavailable or has unsafe permissions.".to_owned()
        })?;
        let CustodySnapshot::Current {
            bytes,
            overflow: false,
            ..
        } = authority
            .read_bounded(MAX_BYTES)
            .map_err(|_| "The vault changed while being read.".to_owned())?
        else {
            return Err("The vault is unavailable or too large.".into());
        };
        let (key, salt, plaintext) = unlock(&bytes)?;
        let document: Document = serde_json::from_slice(&plaintext)
            .map_err(|_| "The encrypted vault data is invalid.".to_owned())?;
        document.validate()?;
        Ok(Self {
            authority,
            key,
            salt,
            document,
        })
    }
    pub(crate) fn identity(&self) -> Option<enzui_core::state_authority::StateIdentity> {
        self.authority.identity()
    }
    pub(crate) fn current(&self) -> bool {
        StateAuthority::open_existing(self.authority.path())
            .is_ok_and(|current| current.identity() == self.authority.identity())
    }
    pub(crate) fn entries(&self) -> &[Credential] {
        &self.document.entries
    }
    pub(crate) fn upsert(&mut self, mut entry: Credential) -> Result<()> {
        entry.validate()?;
        entry.modified = chrono::Utc::now().timestamp();
        let mut next = self.document.clone();
        if let Some(existing) = next.entries.iter_mut().find(|e| e.id == entry.id) {
            *existing = entry;
        } else {
            next.entries.push(entry);
        }
        self.commit(next)
    }
    pub(crate) fn delete(&mut self, id: &str) -> Result<()> {
        let mut next = self.document.clone();
        next.entries.retain(|e| e.id != id);
        self.commit(next)
    }
    fn commit(&mut self, next: Document) -> Result<()> {
        next.validate()?;
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&next).map_err(|_| "The vault could not be saved.".to_owned())?,
        );
        let bytes = crypto::seal(&self.key, &self.salt, &plaintext)?;
        match self.authority.publish(&bytes) {
            Ok(_) => {
                self.document = next;
                Ok(())
            }
            Err(error) => {
                if error.custody() == PublishCustody::Rotated {
                    self.document = next;
                }
                Err("The save could not be confirmed. Lock and reopen the vault before continuing; another window or the filesystem may have changed it.".into())
            }
        }
    }

    pub(crate) fn rekey(&mut self, current_password: &str, new_password: &str) -> Result<()> {
        crypto::new_password(new_password)?;
        self.verify_password(current_password)?;
        let salt = super::random::<16>()?;
        let key = crypto::derive(new_password, &salt)?;
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&self.document)
                .map_err(|_| "The vault could not be saved.".to_owned())?,
        );
        let bytes = crypto::seal(&key, &salt, &plaintext)?;
        match self.authority.publish(&bytes) {
            Ok(_) => {
                self.key = key;
                self.salt = salt;
                Ok(())
            }
            Err(error) => {
                if error.custody() == PublishCustody::Rotated {
                    self.key = key;
                    self.salt = salt;
                }
                Err("The password change could not be confirmed. Lock the vault, then try the new password or the previous one. Keep your backup.".into())
            }
        }
    }

    pub(crate) fn verify_password(&self, password: &str) -> Result<()> {
        let key = crypto::derive(password, &self.salt)?;
        use chacha20poly1305::{
            XChaCha20Poly1305, XNonce,
            aead::{Aead, KeyInit},
        };
        let nonce = super::random::<24>()?;
        let proof = XChaCha20Poly1305::new((&*self.key).into())
            .encrypt(
                XNonce::from_slice(&nonce),
                b"enzim reauthentication".as_ref(),
            )
            .map_err(|_| "Vault verification failed.".to_owned())?;
        XChaCha20Poly1305::new((&*key).into())
            .decrypt(XNonce::from_slice(&nonce), proof.as_ref())
            .map_err(|_| "The vault password is incorrect.".to_owned())?;
        Ok(())
    }
    pub(crate) fn encrypted_export(&self, current: &str, export_password: &str) -> Result<Vec<u8>> {
        self.verify_password(current)?;
        crypto::new_password(export_password)?;
        let salt = super::random::<16>()?;
        let key = crypto::derive(export_password, &salt)?;
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&self.document)
                .map_err(|_| "The backup could not be created.".to_owned())?,
        );
        crypto::seal(&key, &salt, &plaintext)
    }
    pub(crate) fn import(&mut self, entries: Vec<Credential>) -> Result<usize> {
        if entries.len() > MAX_ENTRIES {
            return Err("The import is too large.".into());
        }
        let mut next = self.document.clone();
        let mut added = 0;
        for mut entry in entries {
            entry.validate()?;
            if next.entries.iter().any(|old| {
                old.site == entry.site
                    && old.username == entry.username
                    && old.password == entry.password
                    && old.totp == entry.totp
            }) {
                continue;
            }
            entry.id = Credential::new(entry.site.clone())?.id;
            next.entries.push(entry);
            added += 1;
        }
        self.commit(next)?;
        Ok(added)
    }
    pub(crate) fn read_backup(password: &str, bytes: &[u8]) -> Result<Vec<Credential>> {
        let (_, _, plaintext) = crypto::open(password, bytes)?;
        let document: Document = serde_json::from_slice(&plaintext)
            .map_err(|_| "The encrypted backup is invalid.".to_owned())?;
        document.validate()?;
        Ok(document.entries)
    }
}
