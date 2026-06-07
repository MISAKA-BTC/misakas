# MISAKA Wallet — key-management & security model

Self-custody post-quantum wallet. The threat we optimize against: **theft of key material**
(seed phrase / ML-DSA-87 private key) from a stolen device, a compromised page/XSS, a malicious
dApp, or local malware. This document is normative for the extension and mobile apps.

## 1. Key lifecycle

```
mnemonic (BIP39, 24 words)
   │  KaspaPqKeyPair.fromMnemonic(mnemonic, "", networkId, 0,0,0)   [WASM, in memory only]
   ▼
ML-DSA-87 keypair  ──sign──►  signTransactionMlDsa87(tx, kp, randomness32())
```

- The mnemonic is the **only** secret persisted, and only as ciphertext (the keypair is always
  re-derived in memory; it is never written to disk).
- Decrypted secrets (`mnemonic`, `KaspaPqKeyPair`) live in **volatile memory only**, exist only
  between unlock and lock, and are best-effort zeroized on lock.

## 2. Vault — encryption at rest

| Parameter | Value |
|---|---|
| KDF | **PBKDF2-HMAC-SHA-512**, **600,000** iterations (WebCrypto-native, no extra deps). Argon2id (`m=64MiB,t=3,p=1`) when a WASM KDF is bundled — preferred where available. |
| Salt | 16 random bytes per vault (`crypto.getRandomValues`) |
| Cipher | **AES-256-GCM**, 96-bit random IV per write, 128-bit tag |
| AAD | a version/þnetwork tag, so ciphertext can't be replayed across vault versions |
| Plaintext | `{ mnemonic, createdAt }` only |

The password is never stored, never logged, and never leaves the device. A wrong password fails
GCM authentication (no oracle). See [`chrome-extension/src/vault.js`](chrome-extension/src/vault.js).

## 3. Where keys live, per platform

### Browser extension (MV3)
- **Vault ciphertext** → `chrome.storage.local` (origin-isolated; not page-readable).
- **Decryption + WASM signing** happen in the extension's own context (popup / offscreen
  document), **never** in a web page and **never** exposed to `window`.
- A page/dApp talks to the wallet only through a **content-script bridge** to the background
  service worker; it can *request* a signature (with a user-confirmation prompt) but can never
  read the key.
- **Auto-lock**: decrypted seed is held in the service worker's RAM (never storage) with an idle
  timer (default **5 min**); on timeout / browser close / explicit lock it is dropped.
- Minimal permissions: `storage`, the node's wRPC host, and `alarms` (for lock). No `tabs`, no
  broad host access.

### iOS / Android (Capacitor)
- Same vault format, but the **vault-encryption key is wrapped by the OS hardware keystore** and
  released only after a biometric / passcode gate:
  - **iOS**: Keychain item with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` +
    `SecAccessControl(.biometryCurrentSet | .devicePasscode)`, Secure-Enclave-backed where the
    device supports it. `ThisDeviceOnly` ⇒ never synced to iCloud / never leaves the device.
  - **Android**: `KeyStore` (StrongBox when present) AES key with
    `setUserAuthenticationRequired(true)` gated by `BiometricPrompt`.
- The mnemonic ciphertext lives in app-private storage; the wrapping key never leaves the secure
  element. A screen-capture/backup of app storage alone is useless without the device + biometric.
- `FLAG_SECURE` (Android) / no-screenshot on sensitive screens; clipboard auto-clear after copying
  a seed/address.

## 4. Transaction safety
- Every send renders a **decoded confirmation**: recipient address, amount, fee, network — signed
  only on explicit user approval.
- **Network is explicit and visible** (testnet `misakatest:` vs mainnet `misaka:`); sending to an
  address whose prefix doesn't match the selected network is rejected before signing.
- Signing randomness is fresh per signature (`crypto.getRandomValues(32)`); the deterministic
  ML-DSA path is also acceptable but we default to hedged randomness.

## 5. Anti-phishing / supply chain
- The extension ships the WASM SDK **in-package** (no remote code load); CSP forbids remote
  scripts (`script-src 'self' 'wasm-unsafe-eval'`).
- Releases are reproducible from the source snapshot and published with `SHA256SUMS`.
- The wallet never asks for the seed except on the explicit backup/restore screens.

## 6. Advanced: external signer (optional)
Power users / validators can keep the ML-DSA-87 key outside the wallet entirely and sign via the
standalone [`kaspa-pq-signer`](../kaspa-pq-signer) daemon (policy engine + anti-equivocation +
audit log) over its local socket. The wallet then holds **no** key material.

## 7. Non-goals (explicit)
- Transport-layer PQ (network traffic) is out of scope unless an ML-KEM hybrid is enabled at the
  node — wallet ↔ node uses TLS/WSS as configured by the RPC endpoint.
- Hardware-wallet (Ledger-style) ML-DSA signing is future work; the `kaspa-pq-signer` socket is
  the current "key outside the app" option.
