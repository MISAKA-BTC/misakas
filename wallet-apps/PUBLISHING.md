# Auto-publishing the MISAKA wallets (CI/CD)

Goal: `git tag … && git push --tags` → the store. The pipeline (build + API upload) is committed;
the only thing the AI/CI cannot do is the **one-time** OAuth/account registration — that needs you
to log into your Google/Apple account once. After that it is fully automated.

Division of labor:
- **Done (in this repo):** build, zip, GitHub Actions workflow, `npm run publish` tooling.
- **You, once:** create the OAuth client, mint a refresh token, add 4 GitHub Secrets. (You — not the
  AI — because it requires logging into your Google account / 2FA, which the AI cannot do.)

---

## 1. Chrome Web Store — ready now

### 1a. One-time OAuth setup (≈10 min, in YOUR Google account)
1. **Create the item once, manually** so it gets an ID: open
   https://chrome.google.com/webstore/devconsole/8facc853-6c8e-4ec8-940a-941ca5963929 → **New item**
   → upload `wallet-apps/dist/misaka-wallet-extension-v0.1.0.zip` → fill the listing (copy in
   [chrome-extension/STORE-LISTING.md](chrome-extension/STORE-LISTING.md)) + privacy-policy URL →
   Save. Copy the **32-char Item ID** from the item's URL. (The UUID in the console URL is your
   *publisher* id, not the item id.)
2. **Google Cloud** (https://console.cloud.google.com): create/select a project → **APIs & Services**
   → enable **“Chrome Web Store API”**.
3. **OAuth consent screen**: User type *External*; add your Google account under *Test users*.
4. **Credentials → Create credentials → OAuth client ID → Desktop app**. Save the **Client ID** and
   **Client secret**.
5. **Mint a refresh token** (interactive, opens a browser — you log in, it prints the token):
   ```bash
   npx --yes chrome-webstore-upload-keys
   # paste the Client ID + secret when prompted; approve in the browser; copy the refresh token
   ```

### 1b. Add 4 GitHub repo Secrets
Repo → **Settings → Secrets and variables → Actions → New repository secret**:
| Secret | Value |
|---|---|
| `CWS_EXTENSION_ID` | the 32-char item id from step 1a.1 |
| `CWS_CLIENT_ID` | OAuth client id |
| `CWS_CLIENT_SECRET` | OAuth client secret |
| `CWS_REFRESH_TOKEN` | refresh token from step 1a.5 |

These are write-only to the AI/CI; they are never printed. The AI never sees or handles them.

### 1c. Release
- **By tag:** `git tag wallet-ext-v0.1.0 && git push origin wallet-ext-v0.1.0` → the
  **Publish Chrome extension** workflow builds the zip and submits it for review (`--auto-publish`).
- **Manually:** Actions → *Publish Chrome extension* → *Run workflow* → choose `upload` (draft) or
  `publish`.
- **Locally:** `cd wallet-apps/chrome-extension && npm i && \
   EXTENSION_ID=… CLIENT_ID=… CLIENT_SECRET=… REFRESH_TOKEN=… npm run publish`.

> Note: `chrome-webstore-upload` **updates an existing** item — that's why step 1a.1 (create once)
> is required. Listing copy, screenshots and the privacy-policy URL are also set in the console the
> first time; CI thereafter pushes new versions. Bump `manifest.json` `version` each release.

---

## 2. Android — Google Play (template; enable once the Capacitor app builds)
One-time: Play Console (one-time $25) → create the app → **Setup → API access** → link a Google
Cloud **service account** with *Release manager* → download its **JSON key** → add as secret
`PLAY_SERVICE_ACCOUNT_JSON`. Then a workflow on tag `wallet-android-v*` builds the AAB
(`npx cap sync android` → Gradle `bundleRelease`, signed with an upload key in secrets) and uploads
with [`r0adkll/upload-google-play`](https://github.com/r0adkll/upload-google-play) to the
`internal` track. Promotion to Production is one click (or `status: completed`). No interactive
login after setup.

## 3. iOS — App Store / TestFlight (template; needs a macOS runner)
One-time: App Store Connect → **Users and Access → Integrations → App Store Connect API** → create a
key → add secrets `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8` (the .p8 contents). A `macos-latest`
workflow on tag `wallet-ios-v*` runs `npx cap sync ios` → `fastlane gym` (archive, signed via
match/automatic signing) → `fastlane pilot upload` to **TestFlight**. Upload is automated; Apple's
review is manual (unavoidable).

> Android/iOS workflows are intentionally NOT committed yet — the Capacitor apps in
> [`mobile/`](mobile/) are still a scaffold. Add the workflow files when the apps build so a green
> CI reflects a real artifact. The auth setup above is the only manual, one-time part.

---

## Why the AI stops at "build, not submit"
Submitting needs an account session (Google/Apple login + 2FA). The AI cannot log into your
accounts or complete OAuth consent, so it builds everything and wires the API; you do the one-time
auth and add the secrets. From then on it is `git push`-to-store, exactly like GitHub.
