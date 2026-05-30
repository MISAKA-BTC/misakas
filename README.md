# misakas

Post-quantum **Kaspa (MISAKA)** node — a rusty-kaspa fork hardened for the quantum era.

## What's different from upstream Kaspa
- **Pure PQ-PoW** — BLAKE2b-512 Layer-0 proof-of-work (µs verify; not Argon2id).
- **ML-DSA-65 signatures** — post-quantum tx signing + P2PKH (`misaka*` addresses) and
  `OP_CHECKMULTISIGMLDSA65` multisig.
- **Hash64** — 64-byte block/tx ids (BLAKE2b-512).
- **Tokenomics** — 30B cap = 15B genesis premine + 15B over 20 years at 5%/yr decay.
- **DNS finality overlay (ADR-0018, BFT-free PoS)** — quality-gated StakeScore, validator
  **participation rewards** (§E), worker inclusion bounty (§D), fee/subsidy splits (§F,
  Worker 75 / Validator 25 / Node 0), two-dimensional WorkScore×StakeScore reorg dominance,
  and equivocation slashing. Gated per-network: off on mainnet/testnet/simnet, active on the
  experimental devnet.

## Run a node
Use the prebuilt Linux x86_64 binaries from the latest Release, or `cargo build --release`.

## Become a validator (no code changes)
On a network where the overlay is active: `keygen → fund → bond → run` using the shipped
`kaspa-pq-validator` and `pq-miner` CLIs. Full steps: **[docs/validator-runbook.md](docs/validator-runbook.md)**.

## Verification / test plan
See **[docs/test-plan-kaspa-pq.md](docs/test-plan-kaspa-pq.md)** (Hash64 / PQ-PoW / ML-DSA /
UTXO commitment / tokenomics / DNS overlay + DAG integration harness).

---
*Snapshot of the working source (no upstream git history, no build artifacts, no secrets).*
