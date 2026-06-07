# MISAKA Wallet — Chrome / Chromium extension (MV3)

Self-custody post-quantum (ML-DSA-87) wallet for misakas, for **testnet** (`misakatest:`) and
**mainnet** (`misaka:`). The signing core is the in-package `kaspa-wasm` SDK (no remote code).

## Load it (development)
1. `chrome://extensions` → enable **Developer mode**.
2. **Load unpacked** → select this `chrome-extension/` folder.
3. Pin the extension and open it. First run: **Create a new wallet** (back up the 24 words) or
   **Import a recovery phrase**, then set a password.

## Networks
The network switcher (top-right) toggles **Testnet** / **Mainnet**. Switching networks re-locks the
wallet (different address space) and re-derives your address for that network from the same seed.
The testnet talks to `wss://misakascan.com/kaspa` by default; set a custom node URL in **Settings**
(required for mainnet, which is not launched yet, or to use your own node).

## Files
| File | Role |
|---|---|
| `manifest.json` | MV3 manifest — permissions `storage`, `alarms`; CSP forbids remote scripts |
| `popup.{html,css,js}` | the wallet UI **and** the WASM signing (keys live only in this popup's memory while unlocked) |
| `background.js` | tiny service worker — the auto-lock alarm only; holds **no** key material |
| `src/vault.js` | AES-256-GCM + PBKDF2-SHA-512 (600k) encrypted vault |
| `src/networks.js` | testnet / mainnet presets (id, address prefix, RPC, explorer) |
| `kaspa/` | the Rust→WASM SDK (`kaspa.js` + `kaspa_bg.wasm`) — ML-DSA-87 keygen/sign, tx building, wRPC |

## Security (see [../SECURITY.md](../SECURITY.md))
- Recovery phrase is stored **encrypted** (AES-256-GCM); never in plaintext.
- Decrypted keys exist only in volatile memory while unlocked; **auto-lock** after the idle timeout
  (Settings, default 5 min) drops them. The unlocked seed is held in `chrome.storage.session`
  (in-memory, extension-only, cleared on browser close).
- A wrong password fails GCM authentication — no decryption oracle.
- The WASM SDK is bundled (no remote fetch); CSP `script-src 'self' 'wasm-unsafe-eval'`.

## Packaging for the Web Store
A ready-to-upload package is built at `../dist/misaka-wallet-extension-v0.1.0.zip` (manifest at the
zip root, icons + `PRIVACY.md` included). Rebuild with:
`cd chrome-extension && zip -r ../dist/misaka-wallet-extension-v0.1.0.zip . -x '*.DS_Store'`.
See **[STORE-LISTING.md](STORE-LISTING.md)** for the full submission walkthrough (manual + API),
the listing copy, the privacy-policy URL, and permission justifications.

## Updating the WASM core
`kaspa/` is copied from the same `kaspa-wasm` build that powers `wallet.misakascan.com`. Rebuild it
from the repo (`wasm-pack build` of the `kaspa-wasm` target) and replace `kaspa/kaspa.js` +
`kaspa/kaspa_bg.wasm` to ship signing/protocol updates.

## Known limitations (v1)
- Sends use a single transaction (input-capped for ML-DSA mass). A wallet with **many** small UTXOs
  may need a "send max to self" consolidation first; the multi-tx auto-consolidation from the web
  wallet (`app.js`) can be ported here.
- No QR code on the receive screen yet (address + copy only).
- Store submission still needs 1–4 screenshots (1280×800) and a hosted privacy-policy URL (see STORE-LISTING.md).
