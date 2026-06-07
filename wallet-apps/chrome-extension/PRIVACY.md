# MISAKA Wallet — Privacy Policy

_Last updated: 2026-06-07_

MISAKA Wallet is a **self-custody, client-side** browser extension for the misakas (kaspa-pq)
network. It is designed to collect as little as possible.

## What we collect
**Nothing.** The developer operates **no server** for this extension and receives **no** personal
data, analytics, telemetry, or usage statistics from it.

## What stays on your device
- Your recovery phrase / private key, stored **encrypted** (AES-256-GCM under a key derived from
  your password with PBKDF2-HMAC-SHA-512) in the browser's extension-local storage. It is **never**
  transmitted anywhere and is never stored in plaintext.
- Your settings (selected network, auto-lock timeout, optional custom node URL).

The decrypted key exists only in volatile memory while the wallet is unlocked and is cleared on
lock, idle auto-lock, or browser close.

## Network connections
To show balances and broadcast transactions, the extension connects directly from your browser to a
misakas node over its public RPC endpoint (by default `wss://misakascan.com/kaspa`, or a custom node
URL you set). These requests contain only the blockchain data needed (e.g. your public address to
query its UTXOs, and signed transactions you choose to send). Your private key is **never** sent.
The node operator may, like any blockchain node, observe the public addresses/transactions queried
through it; choose your own node URL in Settings if you prefer.

## Permissions
- `storage` — to save your encrypted vault and settings locally.
- `alarms` — to enforce the idle auto-lock timer.
- host access to the node RPC endpoint(s) — to query balances and submit transactions.

The extension loads **no remote code** (the signing core is bundled WebAssembly; CSP forbids
remote scripts).

## Contact
Open an issue at https://github.com/MISAKA-BTC/misakas
