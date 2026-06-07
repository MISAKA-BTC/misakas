// MISAKA Wallet — encrypted vault.
//
// The seed phrase is the only persisted secret, and only ever as ciphertext.
// KDF: PBKDF2-HMAC-SHA-512, 600k iterations (WebCrypto-native). Cipher: AES-256-GCM.
// A wrong password fails GCM authentication — there is no decryption oracle.
//
// Format (JSON, stored in chrome.storage.local under VAULT_KEY):
//   { v, kdf:"pbkdf2-sha512", iters, salt(b64), iv(b64), ct(b64) }
//
// SECURITY: this module never persists plaintext and never logs secrets. Callers
// must keep the decrypted string in volatile memory only and wipe it on lock.

export const VAULT_VERSION = 1;
const PBKDF2_ITERS = 600_000;
const SALT_BYTES = 16;
const IV_BYTES = 12;

const enc = new TextEncoder();
const dec = new TextDecoder();

const b64 = (buf) => btoa(String.fromCharCode(...new Uint8Array(buf)));
const unb64 = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));

async function deriveKey(password, salt, iters) {
  const baseKey = await crypto.subtle.importKey("raw", enc.encode(password), "PBKDF2", false, ["deriveKey"]);
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", salt, iterations: iters, hash: "SHA-512" },
    baseKey,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

// AAD binds the ciphertext to its version so it can't be replayed across formats.
function aad() {
  return enc.encode(`misaka-wallet-vault-v${VAULT_VERSION}`);
}

/** Encrypt `plaintext` (the mnemonic) under `password`. Returns a serializable vault object. */
export async function createVault(password, plaintext) {
  if (!password || password.length < 8) throw new Error("password must be at least 8 characters");
  const salt = crypto.getRandomValues(new Uint8Array(SALT_BYTES));
  const iv = crypto.getRandomValues(new Uint8Array(IV_BYTES));
  const key = await deriveKey(password, salt, PBKDF2_ITERS);
  const ct = await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: aad() }, key, enc.encode(plaintext));
  return {
    v: VAULT_VERSION,
    kdf: "pbkdf2-sha512",
    iters: PBKDF2_ITERS,
    salt: b64(salt),
    iv: b64(iv),
    ct: b64(ct),
  };
}

/** Decrypt a vault object with `password`. Throws on wrong password (GCM auth failure). */
export async function openVault(password, vault) {
  if (!vault || vault.kdf !== "pbkdf2-sha512") throw new Error("unsupported vault format");
  const salt = unb64(vault.salt);
  const iv = unb64(vault.iv);
  const key = await deriveKey(password, salt, vault.iters || PBKDF2_ITERS);
  let pt;
  try {
    pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv, additionalData: aad() }, key, unb64(vault.ct));
  } catch {
    throw new Error("incorrect password");
  }
  return dec.decode(pt);
}

/** Re-encrypt with a new password (change-password without exposing plaintext to storage). */
export async function rekeyVault(oldPassword, newPassword, vault) {
  const plaintext = await openVault(oldPassword, vault);
  const next = await createVault(newPassword, plaintext);
  return next;
}
