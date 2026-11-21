use super::{MAX_BYTES, Result, random};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use zeroize::Zeroizing;

const HEADER: &[u8; 8] = b"ENZVAULT";
const VERSION: u8 = 1;
const PREFIX: usize = 8 + 1 + 16 + 24;
const KDF_MEMORY_KIB: u32 = 65_536;
const KDF_TIME: u32 = 3;
pub(super) type Key = Zeroizing<[u8; 32]>;

pub(super) fn derive(password: &str, salt: &[u8; 16]) -> Result<Key> {
    if password.len() > 4096 {
        return Err("The vault password is too long.".into());
    }
    let params = Params::new(KDF_MEMORY_KIB, KDF_TIME, 1, Some(32))
        .map_err(|_| "Vault cryptography is unavailable.".to_owned())?;
    let mut memory = Zeroizing::new(vec![argon2::Block::default(); params.block_count()]);
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into_with_memory(password.as_bytes(), salt, &mut *key, &mut memory)
        .map_err(|_| "The vault could not be unlocked.".to_owned())?;
    Ok(key)
}

pub(super) fn seal(key: &Key, salt: &[u8; 16], plaintext: &[u8]) -> Result<Vec<u8>> {
    if plaintext.len() > MAX_BYTES - PREFIX - 16 {
        return Err("The vault is too large.".into());
    }
    let nonce = random::<24>()?;
    let mut output = Vec::with_capacity(PREFIX + plaintext.len() + 16);
    output.extend_from_slice(HEADER);
    output.push(VERSION);
    output.extend_from_slice(salt);
    output.extend_from_slice(&nonce);
    let cipher = XChaCha20Poly1305::new((&**key).into());
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &output,
            },
        )
        .map_err(|_| "The vault could not be encrypted.".to_owned())?;
    output.extend_from_slice(&encrypted);
    Ok(output)
}

pub(super) fn open(password: &str, bytes: &[u8]) -> Result<(Key, [u8; 16], Zeroizing<Vec<u8>>)> {
    let salt = header_salt(bytes)?;
    let key = derive(password, &salt)?;
    let plaintext = open_key(&key, bytes)?;
    Ok((key, salt, plaintext))
}

pub(super) fn header_salt(bytes: &[u8]) -> Result<[u8; 16]> {
    if bytes.len() < PREFIX + 16
        || bytes.len() > MAX_BYTES
        || &bytes[..8] != HEADER
        || bytes[8] != VERSION
    {
        return Err("This is not a supported enzim encrypted vault.".into());
    }
    bytes[9..25]
        .try_into()
        .map_err(|_| "Invalid vault header.".to_owned())
}

pub(super) fn open_key(key: &Key, bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    header_salt(bytes)?;
    let cipher = XChaCha20Poly1305::new((&**key).into());
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&bytes[25..PREFIX]),
            Payload {
                msg: &bytes[PREFIX..],
                aad: &bytes[..PREFIX],
            },
        )
        .map_err(|_| "Wrong vault password or damaged vault. Nothing was changed.".to_owned())?;
    Ok(Zeroizing::new(plaintext))
}

pub(super) fn new_password(password: &str) -> Result<()> {
    if password.chars().count() < 12 || password.len() > 4096 {
        return Err("Use a unique vault password of at least 12 characters.".into());
    }
    Ok(())
}
