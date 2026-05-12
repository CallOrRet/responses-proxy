//! Symmetric encryption for compact summary content using AES-256-GCM.
//!
//! Each ciphertext is `nonce (12 bytes) || encrypted data`, hex-encoded.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};

/// Encrypt plaintext with AES-256-GCM. Returns hex-encoded ciphertext (nonce + data).
pub fn encrypt(key: &[u8; 32], plaintext: &str) -> Option<String> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;

    use rand::RngExt;
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes()).ok()?;

    let mut result = nonce_bytes.to_vec();
    result.extend(&ciphertext);
    Some(hex::encode(&result))
}

/// Decrypt hex-encoded ciphertext produced by [`encrypt`]. Returns the plaintext.
pub fn decrypt(key: &[u8; 32], hex_ciphertext: &str) -> Option<String> {
    let data = hex::decode(hex_ciphertext).ok()?;
    if data.len() < 12 {
        return None;
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);

    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext).ok()?;

    String::from_utf8(plaintext).ok()
}
