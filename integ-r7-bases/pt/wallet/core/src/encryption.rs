//!
//! Wallet data encryption module.
//!

use crate::imports::*;
use crate::result::Result;
use argon2::Argon2;
use chacha20poly1305::{
    Key, XChaCha20Poly1305,
    aead::{AeadCore, AeadInPlace, KeyInit, OsRng, rand_core::RngCore},
};
use sha2::{Digest, Sha256};
use std::ops::{Deref, DerefMut};
use zeroize::Zeroize;

/// Encryption algorithms supported by the Wallet framework.
#[derive(Default, Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum EncryptionKind {
    #[default]
    XChaCha20Poly1305,
}

/// Abstract data container that can contain either plain or encrypted data and
/// transform the data between the two states.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "encryptable", content = "payload")]
pub enum Encryptable<T> {
    #[serde(rename = "plain")]
    Plain(T),
    #[serde(rename = "xchacha20poly1305")]
    XChaCha20Poly1305(Encrypted),
}

impl<T> Zeroize for Encryptable<T>
where
    T: Zeroize,
{
    fn zeroize(&mut self) {
        match self {
            Self::Plain(t) => t.zeroize(),
            Self::XChaCha20Poly1305(e) => e.zeroize(),
        }
    }
}

impl<T> Encryptable<T>
where
    T: Clone + Zeroize + BorshDeserialize + BorshSerialize,
{
    pub fn is_encrypted(&self) -> bool {
        !matches!(self, Self::Plain(_))
    }

    pub fn decrypt(&self, secret: Option<&Secret>) -> Result<Decrypted<T>> {
        match self {
            Self::Plain(v) => Ok(Decrypted::new(v.clone())),
            Self::XChaCha20Poly1305(v) => {
                if let Some(secret) = secret {
                    Ok(v.decrypt(secret)?)
                } else {
                    Err("Decryption secret is 'None' when the data is encrypted!".into())
                }
            }
        }
    }

    pub fn encrypt(&self, secret: &Secret, encryption_kind: EncryptionKind) -> Result<Encrypted> {
        match self {
            Self::Plain(v) => Ok(Decrypted::new(v.clone()).encrypt(secret, encryption_kind)?),
            Self::XChaCha20Poly1305(v) => match encryption_kind {
                EncryptionKind::XChaCha20Poly1305 => Ok(v.clone()),
            },
        }
    }

    pub fn into_encrypted(&self, secret: &Secret, encryption_kind: EncryptionKind) -> Result<Self> {
        match self {
            Self::Plain(v) => Ok(Self::XChaCha20Poly1305(Decrypted::new(v.clone()).encrypt(secret, encryption_kind)?)),
            Self::XChaCha20Poly1305(v) => Ok(Self::XChaCha20Poly1305(v.clone())),
        }
    }

    pub fn into_decrypted(self, secret: &Secret) -> Result<Self> {
        match self {
            Self::Plain(v) => Ok(Self::Plain(v)),
            Self::XChaCha20Poly1305(v) => Ok(Self::Plain(v.decrypt::<T>(secret)?.unwrap())),
        }
    }
}

impl<T> From<T> for Encryptable<T> {
    fn from(value: T) -> Self {
        Encryptable::Plain(value)
    }
}

/// Abstract decrypted data container.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Decrypted<T>(pub(crate) T)
where
    T: BorshSerialize + BorshDeserialize;

impl<T> AsRef<T> for Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> AsMut<T> for Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> Decrypted<T>
where
    T: BorshSerialize + BorshDeserialize,
{
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn encrypt(&self, secret: &Secret, encryption_kind: EncryptionKind) -> Result<Encrypted> {
        let bytes = borsh::to_vec(&self.0)?;
        let encrypted = match encryption_kind {
            EncryptionKind::XChaCha20Poly1305 => encrypt_xchacha20poly1305(bytes.as_slice(), secret)?,
        };
        Ok(Encrypted::new(encryption_kind, encrypted))
    }

    pub fn unwrap(self) -> T {
        self.0
    }
}

/// Encrypted data container (wraps an encrypted payload)
#[derive(Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Encrypted {
    encryption_kind: EncryptionKind,
    payload: Vec<u8>,
}

impl Zeroize for Encrypted {
    fn zeroize(&mut self) {
        self.payload.zeroize();
    }
}

impl std::fmt::Debug for Encrypted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encrypted").field("encryption_kind", &self.encryption_kind).field("payload", &self.payload.to_hex()).finish()
    }
}

impl Encrypted {
    pub fn new(encryption_kind: EncryptionKind, payload: Vec<u8>) -> Self {
        Encrypted { encryption_kind, payload }
    }

    pub fn replace(&mut self, from: Encrypted) {
        self.payload = from.payload;
    }

    pub fn kind(&self) -> EncryptionKind {
        self.encryption_kind
    }

    pub fn decrypt<T>(&self, secret: &Secret) -> Result<Decrypted<T>>
    where
        T: BorshSerialize + BorshDeserialize,
    {
        match self.encryption_kind {
            EncryptionKind::XChaCha20Poly1305 => {
                let decrypted = decrypt_xchacha20poly1305(&self.payload, secret)?;
                Ok(Decrypted(T::try_from_slice(decrypted.as_ref())?))
            }
        }
    }
}

/// Produces `SHA256` hash of the given data.
#[inline]
pub fn sha256_hash(data: &[u8]) -> Secret {
    let mut sha256 = Sha256::default();
    sha256.update(data);
    Secret::new(sha256.finalize().to_vec())
}

/// Produces `SHA256d` hash of the given data.
#[inline]
pub fn sha256d_hash(data: &[u8]) -> Secret {
    let mut sha256 = Sha256::default();
    sha256.update(data);
    sha256_hash(sha256.finalize().as_slice())
}

/// Produces an `argon2sha256iv` hash of the given data, using a **deterministic** salt derived from
/// the data itself (`SHA256(data)`).
///
/// NOTE: the deterministic salt is a legacy property — the same password always yields the same KDF
/// key, which weakens offline-cracking resistance. New encryptions use a random per-message salt (see
/// [`encrypt_xchacha20poly1305`]); this function is retained for decrypting pre-existing (v0) wallet
/// blobs and is otherwise not used.
pub fn argon2_sha256iv_hash(data: &[u8], byte_length: usize) -> Result<Secret> {
    let salt = sha256_hash(data);
    argon2_hash_with_salt(data, salt.as_ref(), byte_length)
}

/// Derive a key from `data` and an explicit `salt` via Argon2.
fn argon2_hash_with_salt(data: &[u8], salt: &[u8], byte_length: usize) -> Result<Secret> {
    let mut key = vec![0u8; byte_length];
    Argon2::default().hash_password_into(data, salt, &mut key)?;
    Ok(key.into())
}

/// Versioned-envelope constants for [`encrypt_xchacha20poly1305`]. Layout:
/// `MAGIC(4) || salt(16) || nonce(24) || ciphertext+tag`. The magic is also bound as AEAD associated
/// data so the version cannot be stripped/forged. Blobs without the magic are treated as legacy v0
/// (deterministic salt, `nonce(24) || ciphertext`), so existing wallets keep decrypting.
const ENC_ENVELOPE_MAGIC: &[u8; 4] = b"KWE1";
const ENC_SALT_LEN: usize = 16;
const ENC_NONCE_LEN: usize = 24; // XChaCha20Poly1305 nonce

/// Encrypts the given data using the `XChaCha20Poly1305` algorithm with a random per-message Argon2
/// salt, wrapped in a versioned envelope (`MAGIC || salt || nonce || ciphertext`).
pub fn encrypt_xchacha20poly1305(data: &[u8], secret: &Secret) -> Result<Vec<u8>> {
    let mut salt = [0u8; ENC_SALT_LEN];
    OsRng.fill_bytes(&mut salt); // random per-message salt
    let private_key_bytes = argon2_hash_with_salt(secret.as_ref(), &salt, 32)?;
    let key = Key::from_slice(private_key_bytes.as_ref());
    let cipher = XChaCha20Poly1305::new(key);
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng); // 192-bit; unique per message
    let mut buffer = data.to_vec();
    buffer.reserve(16);
    // Bind the version magic as AEAD associated data.
    cipher.encrypt_in_place(&nonce, ENC_ENVELOPE_MAGIC, &mut buffer)?;
    let mut out = Vec::with_capacity(ENC_ENVELOPE_MAGIC.len() + ENC_SALT_LEN + ENC_NONCE_LEN + buffer.len());
    out.extend_from_slice(ENC_ENVELOPE_MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(nonce.as_slice());
    out.extend_from_slice(&buffer);
    Ok(out)
}

/// Decrypts data produced by [`encrypt_xchacha20poly1305`]. Auto-detects the versioned envelope
/// (random salt) and falls back to the legacy v0 format (deterministic salt) for older blobs. Returns
/// an error (never panics) on a truncated payload.
pub fn decrypt_xchacha20poly1305(data: &[u8], secret: &Secret) -> Result<Secret> {
    if data.starts_with(ENC_ENVELOPE_MAGIC) {
        // Versioned envelope: MAGIC || salt(16) || nonce(24) || ciphertext+tag.
        let header = ENC_ENVELOPE_MAGIC.len();
        let salt_end = header + ENC_SALT_LEN;
        let nonce_end = salt_end + ENC_NONCE_LEN;
        if data.len() < nonce_end {
            return Err(Error::Custom("encrypted payload is truncated".to_string()));
        }
        let salt = &data[header..salt_end];
        let nonce = &data[salt_end..nonce_end];
        let private_key_bytes = argon2_hash_with_salt(secret.as_ref(), salt, 32)?;
        let key = Key::from_slice(private_key_bytes.as_ref());
        let cipher = XChaCha20Poly1305::new(key);
        let mut buffer = data[nonce_end..].to_vec();
        cipher.decrypt_in_place(nonce.into(), ENC_ENVELOPE_MAGIC, &mut buffer)?;
        Ok(Secret::new(buffer))
    } else {
        // Legacy v0: deterministic salt (SHA256(password)), layout nonce(24) || ciphertext+tag.
        if data.len() < ENC_NONCE_LEN {
            return Err(Error::Custom("encrypted payload is truncated".to_string()));
        }
        let private_key_bytes = argon2_sha256iv_hash(secret.as_ref(), 32)?;
        let key = Key::from_slice(private_key_bytes.as_ref());
        let cipher = XChaCha20Poly1305::new(key);
        let nonce = &data[0..ENC_NONCE_LEN];
        let mut buffer = data[ENC_NONCE_LEN..].to_vec();
        cipher.decrypt_in_place(nonce.into(), &[], &mut buffer)?;
        Ok(Secret::new(buffer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_argon2() {
        println!("testing argon2 hash");
        let password = b"user_password";
        let hash = argon2_sha256iv_hash(password, 32).unwrap();
        let hash_hex = hash.as_ref().to_hex();
        // println!("argon2hash: {:?}", hash_hex);
        assert_eq!(hash_hex, "a79b661f0defd1960a4770889e19da0ce2fde1e98ca040f84ab9b2519ca46234");
    }

    #[test]
    fn test_wallet_encrypt_decrypt() -> Result<()> {
        println!("testing encrypt/decrypt");

        let password = b"password";
        let original = b"hello world".to_vec();
        // println!("original: {}", original.to_hex());
        let password = Secret::new(password.to_vec());
        let encrypted = encrypt_xchacha20poly1305(&original, &password).unwrap();
        // println!("encrypted: {}", encrypted.to_hex());
        let decrypted = decrypt_xchacha20poly1305(&encrypted, &password).unwrap();
        // println!("decrypted: {}", decrypted.to_hex());
        assert_eq!(decrypted.as_ref(), original);

        // The new envelope carries the version magic and a 16-byte random salt before the nonce.
        assert!(encrypted.starts_with(ENC_ENVELOPE_MAGIC), "new format is a versioned envelope");

        Ok(())
    }

    #[test]
    fn test_wallet_decrypt_legacy_v0_blob() -> Result<()> {
        // A blob written by the pre-fix code (deterministic salt, layout nonce(24) || ciphertext)
        // must still decrypt via the legacy fallback path (backward compatibility).
        let password = Secret::new(b"password".to_vec());
        let original = b"legacy payload".to_vec();

        let private_key_bytes = argon2_sha256iv_hash(password.as_ref(), 32)?;
        let key = Key::from_slice(private_key_bytes.as_ref());
        let cipher = XChaCha20Poly1305::new(key);
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let mut buffer = original.clone();
        buffer.reserve(16);
        cipher.encrypt_in_place(&nonce, &[], &mut buffer)?;
        let mut legacy_blob = nonce.to_vec();
        legacy_blob.extend_from_slice(&buffer);

        let decrypted = decrypt_xchacha20poly1305(&legacy_blob, &password)?;
        assert_eq!(decrypted.as_ref(), original);
        Ok(())
    }

    #[test]
    fn test_wallet_decrypt_short_payload_errors_without_panicking() {
        // audit F10: a truncated ciphertext must return an error, not panic on an out-of-bounds slice.
        let password = Secret::new(b"password".to_vec());
        assert!(decrypt_xchacha20poly1305(&[], &password).is_err());
        assert!(decrypt_xchacha20poly1305(&[0u8; 4], &password).is_err());
        assert!(decrypt_xchacha20poly1305(&[0u8; 23], &password).is_err()); // < nonce length
        assert!(decrypt_xchacha20poly1305(ENC_ENVELOPE_MAGIC, &password).is_err()); // magic, no salt/nonce
    }
}
