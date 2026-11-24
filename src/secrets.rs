//! Encryption at rest for stored credentials (Oracle session, WoS API key, Google tokens).
//!
//! Set `OMNICORE_SECRET_KEY` (any long random string) in the host's environment — NOT in
//! the database. Values are then stored as `enc:v1:<base64(nonce ‖ AES-256-GCM ciphertext)>`,
//! so a leaked `bot.sqlite` (or the GitHub backup of it) no longer contains usable secrets.
//!
//! * No key set: values are stored as plain text, exactly as before (a warning is logged once).
//! * Existing plain-text rows keep working; they are re-written encrypted the next time they
//!   are saved.
//! * Losing or changing the key makes encrypted rows unreadable: reconnect those services.

use std::sync::OnceLock;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::Aes256Gcm;
use base64::Engine;
use sha2::{Digest, Sha256};

const PREFIX: &str = "enc:v1:";
const NONCE_LEN: usize = 12;

fn key() -> Option<&'static [u8; 32]> {
    static KEY: OnceLock<Option<[u8; 32]>> = OnceLock::new();
    KEY.get_or_init(|| {
        let raw = std::env::var("OMNICORE_SECRET_KEY").ok()?;
        let raw = raw.trim();
        if raw.is_empty() {
            tracing::warn!(
                "OMNICORE_SECRET_KEY is not set — stored credentials are NOT encrypted at rest"
            );
            return None;
        }
        Some(Sha256::digest(raw.as_bytes()).into())
    })
    .as_ref()
}

/// Encrypt `plain` for storage (a no-op without a key, or if it is already sealed).
pub fn seal(plain: &str) -> String {
    if plain.starts_with(PREFIX) {
        return plain.to_string();
    }
    let Some(key) = key() else {
        return plain.to_string();
    };
    seal_with(key, plain)
}

/// Decrypt a stored value. Plain (legacy) values pass through; a sealed value that cannot be
/// decrypted (wrong/missing key) yields `None`.
pub fn open(stored: &str) -> Option<String> {
    let Some(body) = stored.strip_prefix(PREFIX) else {
        return Some(stored.to_string());
    };
    let Some(key) = key() else {
        tracing::error!("a stored credential is encrypted but OMNICORE_SECRET_KEY is not set");
        return None;
    };
    open_with(key, body)
}

fn seal_with(key: &[u8; 32], plain: &str) -> String {
    let Ok(cipher) = Aes256Gcm::new_from_slice(key) else {
        return String::new();
    };
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    match cipher.encrypt(&nonce, plain.as_bytes()) {
        Ok(mut ciphertext) => {
            let mut blob = nonce.to_vec();
            blob.append(&mut ciphertext);
            format!(
                "{PREFIX}{}",
                base64::engine::general_purpose::STANDARD.encode(blob)
            )
        }
        Err(_) => {
            // Encryption essentially cannot fail; never store a secret we failed to seal.
            tracing::error!("encrypting a credential failed");
            String::new()
        }
    }
}

fn open_with(key: &[u8; 32], body: &str) -> Option<String> {
    let blob = base64::engine::general_purpose::STANDARD
        .decode(body)
        .ok()?;
    if blob.len() <= NONCE_LEN {
        return None;
    }
    let (nonce, ciphertext) = blob.split_at(NONCE_LEN);
    let nonce: [u8; NONCE_LEN] = nonce.try_into().ok()?;
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let plain = cipher.decrypt((&nonce).into(), ciphertext).ok()?;
    String::from_utf8(plain).ok()
}

pub fn seal_opt(value: Option<&str>) -> Option<String> {
    value.map(seal)
}

pub fn open_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| open(&v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tamper() {
        let key: [u8; 32] = Sha256::digest(b"test key").into();
        let sealed = seal_with(&key, "super secret token");
        assert!(sealed.starts_with(PREFIX));
        assert!(!sealed.contains("super secret"));
        assert_eq!(
            open_with(&key, &sealed[PREFIX.len()..]).as_deref(),
            Some("super secret token")
        );
        // Two seals of the same value differ (random nonce).
        assert_ne!(sealed, seal_with(&key, "super secret token"));
        // Wrong key and corrupted data are rejected.
        let other: [u8; 32] = Sha256::digest(b"other").into();
        assert_eq!(open_with(&other, &sealed[PREFIX.len()..]), None);
        assert_eq!(open_with(&key, "AAAA"), None);
    }

    #[test]
    fn legacy_plain_values_pass_through() {
        assert_eq!(open("plain-value").as_deref(), Some("plain-value"));
    }
}
