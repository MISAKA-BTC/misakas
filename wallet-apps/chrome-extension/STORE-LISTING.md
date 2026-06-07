# Chrome Web Store — submission guide (MISAKA Wallet)

The upload artifact is **`wallet-apps/dist/misaka-wallet-extension-v0.1.0.zip`** (manifest at the
zip root, icons + privacy policy included). Uploading requires a signed-in Google account, so it
must be done by you (or via the Web Store API with your own OAuth credentials) — see both paths
below.

## A. Manual upload (recommended, ~5 min)
1. Open the developer console: https://chrome.google.com/webstore/devconsole/8facc853-6c8e-4ec8-940a-941ca5963929
   (a one-time **$5** developer-registration fee applies if the account hasn't paid it).
2. **New item** → drag in `dist/misaka-wallet-extension-v0.1.0.zip`.
3. Fill the **Store listing**:
   - **Name**: MISAKA Wallet (post-quantum)
   - **Summary** (≤132 chars): `Self-custody post-quantum (ML-DSA-87) wallet for misakas — testnet & mainnet.`
   - **Description**: see the block below.
   - **Category**: Productivity (or "Developer Tools").
   - **Language**: English (add Japanese if desired).
   - **Icon**: 128×128 — already in the package (`icons/icon-128.png`); upload the same PNG.
   - **Screenshots**: at least one **1280×800** (or 640×400). Use the onboarding / home views.
4. **Privacy practices** tab:
   - **Privacy policy URL**: host `PRIVACY.md` and paste its URL, e.g.
     `https://raw.githubusercontent.com/MISAKA-BTC/misakas/main/wallet-apps/chrome-extension/PRIVACY.md`
     (or a page on misakascan.com).
   - **Single purpose**: "A self-custody wallet for the misakas (kaspa-pq) network."
   - **Permission justifications**:
     - `storage` — store the user's encrypted vault and settings locally.
     - `alarms` — enforce the idle auto-lock timer.
     - host access (`*.misakascan.com`, localhost) — connect to the misakas node RPC to read
       balances and submit transactions.
   - **Remote code**: **No** (the WASM signing core is bundled; CSP forbids remote scripts).
   - **Data usage**: declare that no user data is collected or transmitted (self-custody).
5. **Save draft** → **Submit for review**.

## B. Programmatic upload (Web Store API — for CI; uses YOUR OAuth creds)
One-time setup (you do this in your own Google Cloud project — do **not** share the tokens):
enable the *Chrome Web Store API*, create an OAuth client, and mint a refresh token. Then:
```bash
ZIP=wallet-apps/dist/misaka-wallet-extension-v0.1.0.zip
# 1) access token
ACCESS=$(curl -s https://oauth2.googleapis.com/token \
  -d client_id=$CLIENT_ID -d client_secret=$CLIENT_SECRET \
  -d refresh_token=$REFRESH_TOKEN -d grant_type=refresh_token | jq -r .access_token)
# 2) FIRST time: create the item (returns the itemId — save it)
curl -s -H "Authorization: Bearer $ACCESS" -H "x-goog-api-version: 2" \
  -X POST -T "$ZIP" https://www.googleapis.com/upload/chromewebstore/v1.1/items
# 2') SUBSEQUENT updates: replace the existing item
curl -s -H "Authorization: Bearer $ACCESS" -H "x-goog-api-version: 2" \
  -X PUT -T "$ZIP" https://www.googleapis.com/upload/chromewebstore/v1.1/items/$ITEM_ID
# 3) publish
curl -s -H "Authorization: Bearer $ACCESS" -H "x-goog-api-version: 2" -H "Content-Length: 0" \
  -X POST https://www.googleapis.com/chromewebstore/v1.1/items/$ITEM_ID/publish
```
> The UUID in your devconsole URL (`8facc853-…`) is the **publisher/group** id, not the **item**
> id. The 32-char item id is assigned on the first create (step 2) and used thereafter.

## Description (paste into the listing)
```
MISAKA Wallet is a self-custody, post-quantum wallet for the misakas (kaspa-pq) network.

• Post-quantum signatures: transactions are authorized with ML-DSA-87 (FIPS 204, NIST level 5).
  There is no secp256k1 path anywhere.
• Testnet & mainnet: switch networks in one tap; the same recovery phrase derives the right
  address for each (misakatest: / misaka:).
• Security-first key management: your 24-word recovery phrase is encrypted on-device with
  AES-256-GCM (PBKDF2-HMAC-SHA-512, 600k iterations) and is NEVER stored in plaintext. Keys live
  only in memory while unlocked and are wiped on auto-lock. No remote code is loaded.
• Self-custody: no account, no server, no tracking. The signing core is bundled WebAssembly.

Send, receive, and check balances on the misakas network.
```

## Review notes / likely follow-ups
- The package bundles a ~12 MB WASM signing core (`kaspa_bg.wasm`) — large but it is plain
  (non-obfuscated) Rust output; point reviewers at the open source if asked.
- Host permissions are scoped to `*.misakascan.com` + localhost. If you later allow arbitrary
  custom node URLs, request that host at runtime via `optional_host_permissions` rather than
  widening the manifest (keeps review clean).
- Before submitting: add 1–4 screenshots (1280×800) and confirm the privacy-policy URL resolves.
