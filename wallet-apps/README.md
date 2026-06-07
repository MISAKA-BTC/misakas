# MISAKA Wallet apps (post-quantum, ML-DSA-87)

Multi-platform self-custody wallets for **misakas** (kaspa-pq) — a **Chrome/Chromium
browser extension** and **iOS / Android** apps — that work on both the **testnet**
(`testnet-10`, `misakatest:` addresses) and **mainnet** (`misaka:`) parameter sets.

All three share **one** post-quantum signing core: the audited Rust → WebAssembly SDK
(`kaspa-wasm`) that already powers `wallet.misakascan.com`. Transaction authorization is
**ML-DSA-87** (FIPS 204); there is no secp256k1 path anywhere in the stack.

```
                         ┌─────────────────────────────┐
                         │  kaspa-wasm SDK  (Rust→WASM) │
                         │  Mnemonic · KaspaPqKeyPair   │
                         │  createTransaction           │
                         │  signTransactionMlDsa87      │  ← the only place keys are used
                         │  RpcClient (wRPC-JSON)       │
                         └──────────────┬──────────────┘
              ┌─────────────────────────┼─────────────────────────┐
       ┌──────┴───────┐          ┌──────┴───────┐           ┌──────┴───────┐
       │ chrome-      │          │ mobile/      │           │ (existing)   │
       │ extension/   │          │ Capacitor    │           │ web wallet   │
       │ MV3          │          │ iOS+Android  │           │ misakascan   │
       └──────────────┘          └──────────────┘           └──────────────┘
        encrypted vault           OS Keychain /              (reference)
        + auto-lock               Keystore + biometric
```

## Why one core
ML-DSA-87 signing, address derivation, mass-aware tx building and the consensus
serialization are subtle and security-critical. Keeping them in **one** Rust→WASM
module (not reimplemented per platform) means every wallet signs identically to the node
and there is a single place to audit. The platform layers only add **UI** and
**key-at-rest protection**.

## What's here
| Path | Status | Notes |
|---|---|---|
| [`chrome-extension/`](chrome-extension/) | **working v1** | MV3 extension; encrypted vault; testnet+mainnet; send/receive/balance via the WASM SDK |
| [`SECURITY.md`](SECURITY.md) | spec | the high-security key-management model for every platform (read this first) |
| [`mobile/`](mobile/) | **scaffold + plan** | Capacitor project wrapping the same web/WASM core with native Keychain/Keystore + biometric |

## Security posture (summary — see [SECURITY.md](SECURITY.md))
- The seed phrase / private key is **never stored in plaintext** (the legacy web wallet kept
  it in `localStorage` — these apps do not). It lives encrypted with **AES-256-GCM** under a
  key derived from the user's password (**PBKDF2-HMAC-SHA-512, 600k iterations**; Argon2id where a
  WASM KDF is bundled).
- Decrypted key material exists **only in volatile memory**, only while unlocked, and is wiped on
  **auto-lock** (idle timeout) / lock / close.
- **Mobile**: the vault-encryption key is wrapped by the **hardware** keystore (iOS Secure
  Enclave-backed Keychain, Android Keystore/StrongBox) and released only after **biometric**
  (Face ID / Touch ID / fingerprint) or device-passcode auth.
- Every send shows a **decoded confirmation** (recipient, amount, fee) before signing; the key is
  used for exactly one signature and never leaves the signing context.
- Optional advanced custody: point a validator/power user at the standalone
  [`kaspa-pq-signer`](../kaspa-pq-signer) HSM daemon instead of an in-app key.

## Build / load
- Extension: see [`chrome-extension/README.md`](chrome-extension/README.md) (load unpacked, or zip for the Web Store).
- Mobile: see [`mobile/README.md`](mobile/README.md) (Capacitor → Xcode / Android Studio).
