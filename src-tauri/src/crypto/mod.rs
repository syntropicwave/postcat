//! End-to-end encryption for sync (client side).
//!
//! Envelope design: content is encrypted with a random `data_key` that never
//! changes. That `data_key` is wrapped (encrypted) twice — once with a key
//! derived from the user's password, once with a key derived from a recovery
//! code. So the server only ever holds ciphertext and two wrapped copies of
//! the data key; the password (and thus the plaintext) never leaves the
//! device. Changing the password re-wraps the data key without touching any
//! content; a lost password is recoverable only via the recovery code.

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroize;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const SALT_LEN: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("key derivation failed")]
    Kdf,
    #[error("wrong password or corrupted data")]
    Decrypt,
    #[error("malformed ciphertext")]
    Malformed,
    #[error("invalid recovery code")]
    RecoveryCode,
}

/// The unwrapped content key. Held only in memory while signed in.
#[derive(Clone, Zeroize)]
#[zeroize(drop)]
pub struct DataKey([u8; KEY_LEN]);

/// What the server stores for an account. None of it reveals the password or
/// the content; `auth_verifier` proves knowledge of the password at login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBlob {
    /// Salt for the password KDF (not secret).
    pub salt: String,
    /// Salt for the recovery-code KDF.
    pub recovery_salt: String,
    /// HKDF("auth") of the password root — the server stores its SHA-256.
    pub auth_verifier_hash: String,
    /// data_key sealed with the password-derived key.
    pub wrapped_by_password: String,
    /// data_key sealed with the recovery-derived key.
    pub wrapped_by_recovery: String,
}

/// Result of creating an account: the blob to upload, the recovery code to
/// show the user exactly once, and the live data key.
pub struct Registration {
    pub blob: AccountBlob,
    pub recovery_code: String,
    pub data_key: DataKey,
    /// Sent to the server at login to authenticate (its hash is in the blob).
    pub auth_verifier: String,
}

fn random(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    buf
}

fn argon2_key(secret: &[u8], salt: &[u8]) -> Result<[u8; KEY_LEN], CryptoError> {
    let mut out = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(secret, salt, &mut out)
        .map_err(|_| CryptoError::Kdf)?;
    Ok(out)
}

fn subkey(root: &[u8; KEY_LEN], info: &[u8]) -> [u8; KEY_LEN] {
    let hk = Hkdf::<Sha256>::new(None, root);
    let mut okm = [0u8; KEY_LEN];
    // expand only errors when the output length exceeds 255*HashLen; a single
    // 32-byte key never hits that, so a failure would be a logic bug — leave
    // okm zeroed rather than panic (login would then simply fail to decrypt).
    let _ = hk.expand(info, &mut okm);
    okm
}

fn seal(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce_bytes = random(NONCE_LEN);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| CryptoError::Decrypt)?;
    let mut out = nonce_bytes;
    out.extend_from_slice(&ct);
    Ok(out)
}

fn open(key: &[u8; KEY_LEN], blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < NONCE_LEN {
        return Err(CryptoError::Malformed);
    }
    let (nonce_bytes, ct) = blob.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(key.into());
    cipher
        .decrypt(XNonce::from_slice(nonce_bytes), ct)
        .map_err(|_| CryptoError::Decrypt)
}

fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(s: &str) -> Result<Vec<u8>, CryptoError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| CryptoError::Malformed)
}

/// Human-typable recovery code: 20 random bytes as 8 groups of 5 hex chars.
fn make_recovery_code() -> String {
    let bytes = random(20);
    let hex = hex::encode(bytes);
    hex.as_bytes()
        .chunks(5)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

fn recovery_secret(code: &str) -> Vec<u8> {
    // Normalize: drop separators/case so the user can retype loosely.
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect::<String>()
        .into_bytes()
}

/// Create a new account from a password. The returned recovery code is the
/// only way back in if the password is lost — show it once, never store it.
pub fn register(password: &str) -> Result<Registration, CryptoError> {
    let salt = random(SALT_LEN);
    let recovery_salt = random(SALT_LEN);

    let root = argon2_key(password.as_bytes(), &salt)?;
    let enc_key = subkey(&root, b"postcat-enc-v1");
    let auth = subkey(&root, b"postcat-auth-v1");

    let mut data_key_bytes = random(KEY_LEN);
    let mut data_key = [0u8; KEY_LEN];
    data_key.copy_from_slice(&data_key_bytes);
    data_key_bytes.zeroize();

    let recovery_code = make_recovery_code();
    let recovery_key = argon2_key(&recovery_secret(&recovery_code), &recovery_salt)?;

    let wrapped_by_password = seal(&enc_key, &data_key)?;
    let wrapped_by_recovery = seal(&recovery_key, &data_key)?;

    let auth_verifier = b64(&auth);
    let auth_verifier_hash = hex::encode(sha2::Sha256::digest_bytes(&auth));

    Ok(Registration {
        blob: AccountBlob {
            salt: b64(&salt),
            recovery_salt: b64(&recovery_salt),
            auth_verifier_hash,
            wrapped_by_password: b64(&wrapped_by_password),
            wrapped_by_recovery: b64(&wrapped_by_recovery),
        },
        recovery_code,
        data_key: DataKey(data_key),
        auth_verifier,
    })
}

/// Sign in: derive the keys, verify the password locally against the blob and
/// unwrap the data key. Returns the data key and the auth verifier the client
/// sends to the server to prove it knows the password.
pub fn login(password: &str, blob: &AccountBlob) -> Result<(DataKey, String), CryptoError> {
    let salt = unb64(&blob.salt)?;
    let root = argon2_key(password.as_bytes(), &salt)?;
    let enc_key = subkey(&root, b"postcat-enc-v1");
    let auth = subkey(&root, b"postcat-auth-v1");

    let wrapped = unb64(&blob.wrapped_by_password)?;
    let data_key = open(&enc_key, &wrapped)?; // wrong password fails here
    Ok((data_key_from(&data_key)?, b64(&auth)))
}

/// Derive just the auth verifier from a password and the (server-provided)
/// salt. Needed for login, where the client must present the verifier to the
/// server *before* it receives the wrapped key.
pub fn login_auth_verifier(password: &str, salt_b64: &str) -> Result<String, CryptoError> {
    let salt = unb64(salt_b64)?;
    let root = argon2_key(password.as_bytes(), &salt)?;
    Ok(b64(&subkey(&root, b"postcat-auth-v1")))
}

/// Recover access with the recovery code (e.g. after forgetting the password).
pub fn recover(recovery_code: &str, blob: &AccountBlob) -> Result<DataKey, CryptoError> {
    let salt = unb64(&blob.recovery_salt)?;
    let recovery_key = argon2_key(&recovery_secret(recovery_code), &salt)
        .map_err(|_| CryptoError::RecoveryCode)?;
    let wrapped = unb64(&blob.wrapped_by_recovery)?;
    let data_key = open(&recovery_key, &wrapped).map_err(|_| CryptoError::RecoveryCode)?;
    data_key_from(&data_key)
}

/// Change the password: re-wrap the existing data key with a key derived from
/// the new password. Content is untouched. Returns the updated blob fields.
pub fn change_password(
    data_key: &DataKey,
    new_password: &str,
) -> Result<ChangedPassword, CryptoError> {
    let salt = random(SALT_LEN);
    let root = argon2_key(new_password.as_bytes(), &salt)?;
    let enc_key = subkey(&root, b"postcat-enc-v1");
    let auth = subkey(&root, b"postcat-auth-v1");
    let wrapped = seal(&enc_key, &data_key.0)?;
    Ok(ChangedPassword {
        salt: b64(&salt),
        wrapped_by_password: b64(&wrapped),
        auth_verifier: b64(&auth),
        auth_verifier_hash: hex::encode(sha2::Sha256::digest_bytes(&auth)),
    })
}

pub struct ChangedPassword {
    pub salt: String,
    pub wrapped_by_password: String,
    pub auth_verifier: String,
    pub auth_verifier_hash: String,
}

impl DataKey {
    /// Encrypt content for upload.
    pub fn seal_str(&self, plaintext: &str) -> Result<String, CryptoError> {
        Ok(b64(&seal(&self.0, plaintext.as_bytes())?))
    }

    /// Decrypt content pulled from the server.
    pub fn open_str(&self, blob_b64: &str) -> Result<String, CryptoError> {
        let bytes = open(&self.0, &unb64(blob_b64)?)?;
        String::from_utf8(bytes).map_err(|_| CryptoError::Malformed)
    }
}

fn data_key_from(bytes: &[u8]) -> Result<DataKey, CryptoError> {
    if bytes.len() != KEY_LEN {
        return Err(CryptoError::Malformed);
    }
    let mut k = [0u8; KEY_LEN];
    k.copy_from_slice(bytes);
    Ok(DataKey(k))
}

/// The server verifies login by comparing SHA-256(auth_verifier) to the
/// stored hash. Shared so the server crate can reuse the exact check.
pub fn verify_auth(auth_verifier_b64: &str, stored_hash_hex: &str) -> bool {
    let Ok(bytes) = unb64(auth_verifier_b64) else {
        return false;
    };
    hex::encode(sha2::Sha256::digest_bytes(&bytes)) == stored_hash_hex
}

// Small helper so we don't pull sha2's Digest trait names everywhere.
trait DigestBytes {
    fn digest_bytes(data: &[u8]) -> Vec<u8>;
}
impl DigestBytes for Sha256 {
    fn digest_bytes(data: &[u8]) -> Vec<u8> {
        use sha2::Digest;
        Sha256::digest(data).to_vec()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn register_login_roundtrip() {
        let reg = register("correct horse battery staple").unwrap();
        // Content sealed with the live data key round-trips.
        let sealed = reg.data_key.seal_str(r#"{"secret":"value"}"#).unwrap();
        assert_ne!(sealed, r#"{"secret":"value"}"#);

        let (key, verifier) = login("correct horse battery staple", &reg.blob).unwrap();
        assert_eq!(key.open_str(&sealed).unwrap(), r#"{"secret":"value"}"#);
        // The verifier the client presents matches the stored hash.
        assert!(verify_auth(&verifier, &reg.blob.auth_verifier_hash));
        assert_eq!(verifier, reg.auth_verifier);
    }

    #[test]
    fn wrong_password_fails() {
        let reg = register("hunter2").unwrap();
        assert!(matches!(
            login("hunter3", &reg.blob),
            Err(CryptoError::Decrypt)
        ));
    }

    #[test]
    fn recovery_code_recovers_the_same_data_key() {
        let reg = register("passphrase").unwrap();
        let sealed = reg.data_key.seal_str("payload").unwrap();

        let recovered = recover(&reg.recovery_code, &reg.blob).unwrap();
        assert_eq!(recovered.open_str(&sealed).unwrap(), "payload");

        // Loosely retyped (no dashes, upper case) still works.
        let messy = reg.recovery_code.replace('-', "").to_uppercase();
        assert!(recover(&messy, &reg.blob).is_ok());

        // A wrong code does not.
        assert!(recover("0000-0000-0000-0000", &reg.blob).is_err());
    }

    #[test]
    fn change_password_keeps_content_and_recovery() {
        let reg = register("old-pass").unwrap();
        let sealed = reg.data_key.seal_str("data").unwrap();

        let changed = change_password(&reg.data_key, "new-pass").unwrap();
        // Rebuild the blob as the server would store it.
        let blob = AccountBlob {
            salt: changed.salt,
            wrapped_by_password: changed.wrapped_by_password,
            auth_verifier_hash: changed.auth_verifier_hash,
            ..reg.blob.clone()
        };

        // Old password no longer works; new one does; content still decrypts.
        assert!(login("old-pass", &blob).is_err());
        let (key, _) = login("new-pass", &blob).unwrap();
        assert_eq!(key.open_str(&sealed).unwrap(), "data");
        // Recovery code is unchanged and still recovers the data key.
        assert!(recover(&reg.recovery_code, &blob).is_ok());
    }
}
