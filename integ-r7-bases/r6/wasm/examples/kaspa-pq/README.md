# kaspa-pq Phase 7 (PR-7.5) — TypeScript / HTML SDK examples

Two minimal examples that drive the kaspa-pq WASM bindings (PR-7.4):

| File | What it does |
|---|---|
| [`derivation.ts`](derivation.ts) | Node / Bun / Deno-runnable TypeScript: BIP39 mnemonic → kaspa-pq keypair → sign + verify roundtrip. Mirrors the Rust example at [`rpc/wrpc/examples/kaspa_pq_send`](../../../rpc/wrpc/examples/kaspa_pq_send/). |
| [`derivation.html`](derivation.html) | Static HTML page that loads the WASM bundle and derives an address from user-entered mnemonic + path. Useful as a paper-wallet UI prototype. |

## Build the WASM bundle

```bash
wasm-pack build wasm --features wasm32-sdk --target web --out-dir kaspa-pq-pkg
```

The HTML file expects the result at `../kaspa-pq-pkg/kaspa_pq.js`; adjust
the import path at the top of [`derivation.html`](derivation.html) if you
publish to a different location or to npm.

## Run

```bash
# Node / ts-node
cd wasm/examples/kaspa-pq
npm install kaspa-pq
ts-node derivation.ts

# Browser
python3 -m http.server   # then visit http://localhost:8000/derivation.html
```

## Submitting transactions

Both examples stop at the cryptographic boundary. Going further — UTXO
selection, fee estimation, and `submitTransaction` over wRPC against a
live kaspa-pq node — is the Phase 5' follow-up (see ADR-0006 §1 +
ADR-0006 §"Out of scope for Phase 7").
