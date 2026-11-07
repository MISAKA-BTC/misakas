# What is different from Kaspa

> 旧ルート README（2026-10-12 まで）から移した手順です。例に testnet-11 や古い値が含まれる場合があります。現行ネットワーク（testnet-12）の値は [README](../../README.md) と [docs/README](../README.md) を正とし、`--help` と chain RPC で確認してください。

## What's different from Kaspa

| Area | misakas (PQ-only) |
|---|---|
| Tx signature | **ML-DSA-87** (pk 2592 B / sig 4627 B); secp256k1/Schnorr/ECDSA disabled at consensus |
| Tx signature context | `kaspa-pq-v2/tx/mldsa87` |
| Sighash | `calc_mldsa87_signature_hash` → 64-byte `Hash64` (domain `kaspa-pq-v2/sighash/mldsa87`) |
| Address | `PubKeyHashMlDsa87` only; payload = **keyed** BLAKE2b-512(`kaspa-pq-v2/address/mldsa87`, vk), 64 B |
| Standard script | ML-DSA-87 P2PKH only (`OP_DUP OP_BLAKE2B_512 OP_DATA64 <64B> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`); P2SH disabled |
| Consensus identity | 64-byte BLAKE2b-512 (`Hash64`): block hash / txid / merkle roots / UTXO commitment / parents |
| secp256k1 | feature-gated out of both `kaspa-consensus` and the `kaspad` node binary (default `pq-only`) |
| Script caps | `MAX_SCRIPT_ELEMENT_SIZE` = 8192, `MAX_SCRIPTS_SIZE` / `max_signature_script_len` = 16_384 |
| Genesis / tokenomics | new genesis; **28B BILI cap = 13B premine** (40 vaults × 0.1B + 1 main × 9B, ML-DSA-87 P2PKH) **+ 15B network emission** over 20 yr, 5%/yr exponential decay (`coinbase::SUBSIDY_BY_MONTH_TABLE`). The premine constants live in `consensus/core/src/config/premine.rs`; a change there moves every network's genesis hash and is refused at startup until re-pinned (audit M-07) |
| Block cadence | **120 s** on PALW networks (`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`), frozen — refused at parameter construction, because a block interval shorter than the inference it certifies certifies nothing |

Authoritative design & spec live under [`docs/`](..):

- [ADR-0019 — ML-DSA-87 migration](../adr/0019-mldsa87-migration.md) (rev 1.2 is the current governing record)
- [Design doc — `docs/kaspa-pq-design-mldsa87.md`](../design/pq/kaspa-pq-design-mldsa87.md)
- [Spec — `docs/kaspa-pq-spec.md`](../spec/pq/kaspa-pq-spec.md)
- [Verification runbook — `docs/kaspa-pq-mldsa87-verification-runbook.md`](../testing/pq/kaspa-pq-mldsa87-verification-runbook.md)
- [Validator runbook — `docs/validator-runbook.md`](../history/operations/dns-validator/validator-runbook.md)
- [PALW provenance map — `docs/palw-registry-map.md`](../reference/palw/palw-registry-map.md) — **where the model registry, the artifact hash, the runtime registry, the receipt, the output root, the verification, the metering and the capability profile already are**, field by field, plus the layers that are refused and why (ADR-0079 §7 / R-09). Read it before proposing a provenance layer.
- [ADR index — `docs/adr/README.md`](../adr/README.md) (what governs, and what was reversed)

**Scope of PQ claims** (per the design doc): "tx authorization uses ML-DSA-87", "secp256k1 signing disabled in PQ consensus mode", "64-byte BLAKE2b-512 consensus identity". Transport-layer (network) traffic is **not** PQ unless an ML-KEM hybrid is enabled.
