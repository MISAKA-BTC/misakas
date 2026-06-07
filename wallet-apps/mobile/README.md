# MISAKA Wallet — iOS & Android (Capacitor)

The mobile apps reuse the **same** post-quantum web/WASM core as the extension and the web wallet
(`kaspa-wasm` → ML-DSA-87 signing) wrapped in a native shell via **[Capacitor](https://capacitorjs.com/)**,
adding **hardware-backed** key protection and **biometric** unlock. One codebase → iOS + Android +
(the same `www/` even runs as a PWA).

> Status: **scaffold + plan**. The web UI + WASM core already exist (share the extension's `popup.js`
> logic as a full-page web app, or reuse `wallet.misakascan.com`'s `app.js`). What remains is the
> Capacitor wrapper + the native secure-storage bridge below + store builds in Xcode / Android Studio.

## Architecture
```
www/                      ← web app (HTML/JS) loading kaspa-wasm (ML-DSA-87) — shared with the extension/web wallet
 └─ vault.js              ← AES-256-GCM vault (same format as the extension)
capacitor secure-storage  ← stores the vault-encryption key in the OS hardware keystore,
 plugin                      released only after Face ID / Touch ID / fingerprint / passcode
iOS (Xcode)  /  Android (Android Studio)  ← store packaging + signing
```

The difference from the extension: instead of (or in addition to) a user password, the
vault-encryption key is held by the **OS secure element** and gated by **biometrics** — so the app
unlocks with Face ID / fingerprint and the key never exists in JS until the OS releases it.

## High-security key storage (the point of the mobile build)
Use a native secure-storage plugin (e.g. `@aparajita/capacitor-biometric-auth` +
`@aparajita/capacitor-secure-storage`, or `capacitor-secure-storage-plugin`) configured for:

- **iOS** — Keychain with
  `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` and a `SecAccessControl` of
  `.biometryCurrentSet` + `.devicePasscode` (Secure-Enclave-backed). `ThisDeviceOnly` ⇒ the key is
  **never** synced to iCloud Keychain and never leaves the device. `.biometryCurrentSet` ⇒ adding a
  new fingerprint/face invalidates the key (anti-coercion).
- **Android** — `KeyStore` (use **StrongBox** when `PackageManager.FEATURE_STRONGBOX_KEYSTORE`)
  AES key created with `setUserAuthenticationRequired(true)` and unlocked via `BiometricPrompt`.

Flow: on first run, generate a random 32-byte **wrapping key** → store it in the secure element
(biometric-gated). The BIP39 mnemonic is encrypted (`vault.js`, AES-256-GCM) under a key derived
from `PBKDF2(wrapping-key)`; ciphertext goes in app-private storage. Unlock = biometric → secure
element releases the wrapping key → decrypt the vault in memory → derive the ML-DSA-87 keypair via
WASM. Lock / background → wipe the in-memory key.

Also enable: `FLAG_SECURE` (Android, blocks screenshots/screen-record on wallet screens), iOS
app-switcher privacy overlay, and clipboard auto-clear after copying a seed/address.

## Bring-up steps
```bash
cd wallet-apps/mobile
npm install
# put the shared web app + WASM in www/  (symlink/copy the extension's popup.* + src/ + kaspa/,
# or build the web-wallet bundle into www/)
npx cap add ios
npx cap add android
npx cap sync
npx cap open ios       # → Xcode: set team/bundle id, enable Face ID usage string, archive
npx cap open android   # → Android Studio: set applicationId, sign, build AAB
```

iOS `Info.plist`: add `NSFaceIDUsageDescription`. Android: `USE_BIOMETRIC` permission +
`FLAG_SECURE` on the wallet activity.

## Networks
Same as the extension: a Testnet/Mainnet switch backed by `src/networks.js` (testnet
`misakatest:` via `wss://misakascan.com/kaspa`; mainnet `misaka:` via a user-set node). Switching
re-derives the address for that network.

## Why not native Rust (uniffi) instead of WASM?
A pure-native path (compile the `kaspa-pq` signing crate to a `.framework`/`.aar` via `uniffi` +
`cargo-ndk`/`cargo-lipo`) is viable and removes the WASM runtime. It is **more** work and a second
signing surface to audit. We default to the shared WASM core for parity with the extension/web
wallet; uniffi is a documented future option if WASM startup/size on mobile becomes a concern.
