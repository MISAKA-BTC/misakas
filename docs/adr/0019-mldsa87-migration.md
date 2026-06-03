# ADR-0019: Migrate Signature Scheme from ML-DSA-65 to ML-DSA-87

## Status
Accepted (2026-05-30); **Revised 1.1 — 2026-05-31** (scope expanded to PQ-only completion); **Revised 1.2 — 2026-06-01** (md2 alignment: identifier rename, v2 contexts, keyed address payload, 16_384 caps).

Supersedes the signature-level choice in [ADR-0002](0002-mldsa65-p2pkh.md). Forces a new genesis (see [ADR-0008](0008-pq-genesis-premine.md)); **not compatible** with any prior kaspa-pq chain state, UTXO set, or address.

---

## Revision 1.2 — md2 (PQ-only design v2.0) alignment (2026-06-01)

Revision 1.1 deferred the identifier rename and used v1 signing contexts, an *unkeyed* address payload, and 10_000 caps. The **md2** design pass closed these. The current code governs; where 1.1 / 1.0 conflict with the items below, **1.2 governs**.

- **Naming — DONE (S10).** The deferred `MlDsa65`→`MlDsa87` rename has landed: `MlDsa87` / `MLDSA87_*` / `mldsa87` across all crates, the address version `PubKeyHashMlDsa87`, opcode idents, and the WASM `signTransactionMlDsa87` binding. (Only ADR-0002's *filename* keeps `mldsa65`.) The 1.0/1.1 "keep `MlDsa65` identifiers" policy is **withdrawn**.
- **Signing contexts v1→v2 — DONE (S11b).** Tx context = `b"kaspa-pq-v2/tx/mldsa87"`, sighash domain = `b"kaspa-pq-v2/sighash/mldsa87"`. Signer + verifier move in lock-step; no genesis change.
- **Address payload now KEYED — DONE (S11c).** The 64-byte ML-DSA-87 P2PKH payload is a **keyed** BLAKE2b-512 of the verification key under `b"kaspa-pq-v2/address/mldsa87"` (was *unkeyed*). The `OP_BLAKE2B_512` opcode recomputes this keyed hash at spend time; the opcode, the wallet/premine/validator derivations all share `kaspa_hashes::blake2b_512_address_payload` so a P2PKH stays spendable. This **re-genesised** the chain (new premine payload + `utxo_commitment` + all 5 genesis block hashes; `hash_merkle_root`s unchanged since the coinbase is unchanged).
- **Script caps 10_000→16_384 — DONE (S11a).** `MAX_SCRIPTS_SIZE`, the mempool `MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE`, and the 4 `Params` presets' `max_signature_script_len` are now **16_384**. `MAX_SCRIPT_ELEMENT_SIZE` stays 8192; `max_script_public_key_len` stays 10_000.
- **Validator-attestation crash fix (`67dfbb0`).** The DNS-overlay `rewarded_epochs_store` (per-block `(bond, epoch)` reward keys) reused the `tracked_bytes` utxo_diffs cache policy, but its `Vec` value implements only `estimate_mem_units` (not `_bytes`) → the first reward-bearing coinbase panicked the `virtual-processor` (`not implemented` @ `utils/src/mem_size.rs:40`). Fixed to an untracked `Count` policy (+ regression test).

The normative table in *Revision 1.1* below is updated in place to these values.

---

## Revision 1.1 — PQ-only completion (2026-05-31)

The original 1.0 decision was a minimal value/variant swap (ML-DSA-65→87) that *kept* legacy paths. Per the team design doc **`docs/kaspa-pq-design-mldsa87.md`**, the scope is now a full **PQ-only** completion: legacy secp256k1/Schnorr/ECDSA, legacy addresses, and P2SH are made **unrepresentable at the consensus, mempool, and wallet layers** — not merely "ML-DSA added". This revises several 1.0 decisions.

### Locked decisions (this revision)
1. **Address payload = 64-byte BLAKE2b-512** of the ML-DSA-87 verification key (was 32-byte). Needs an `OP_BLAKE2B_512` / `OpData64` opcode. `scriptPubKey = OP_DUP OP_BLAKE2B_512 OP_DATA64 <payload64> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`.
2. **Premine → single-key ML-DSA-87 P2PKH** (was 2-of-3 multisig P2SH). P2SH/multisig is out of launch scope, so the premine cannot be a P2SH UTXO.
3. **P2SH disabled in launch scope.** Re-enabling ML-DSA multisig is a separate ADR + redeem-script static-analysis class.

### Revised from 1.0
- **Naming:** full rename to `*87*` including the tx context string `b"kaspa-pq-v1/tx/mldsa87"`. The 1.0 "keep `MlDsa65` identifiers" minimal-churn policy is **withdrawn**.
- **Script caps:** P2PKH-only → `MAX_SCRIPT_ELEMENT_SIZE = 8192`, `MAX_SCRIPTS_SIZE = 10_000`, `max_signature_script_len = 10_000`. (Revises the 1.0 multisig-sized 16384/32768.)

### Net-new consensus/wallet work (8 phases; design doc §17, §22 order)
All eight phases have landed on branch `pr-19-…` (not yet pushed to `main`):

- **P1 (done):** `PqEnforcementMode {Disabled, PolicyOnly, Consensus}` + `pq_activation_daa_score` + `Params::is_pq_active(daa)` in `params.rs`; all kaspa-pq nets = `Consensus` @ genesis. (Inert until consumed in P2.)
- **P2 (done):** `ScriptPolicy` in `TxScriptEngine`; legacy signature opcodes → consensus error; P2SH disabled; PQ-only script-class reject at mempool **and** consensus (output + input).
- **P3 (done):** `calc_mldsa87_signature_hash` → `Hash64` + `Mldsa87SigHashReusedValues`; context `mldsa87`; ML-DSA verify/sign in lockstep on the 64-byte digest.
- **P4 (done):** 64-byte address + premine P2PKH + caps; genesis regenerated once.
- **P5 (done):** wallet/API PQ-only (legacy address reject, PQ change address); native ML-DSA wallet-core.
- **P6 (done):** `utxo_commitment` → `Hash64`.
- **P7 (done):** mass/DoS calibration (`mass_per_sig_op` = 10000 from the measured ML-DSA-87/Schnorr verify ratio).
- **P8 (secp256k1 feature-gate + SigCache PQ key + docs done — S8a `b278817`, S8b `a1d93f5`, S8c):**
  `secp256k1` is now an **optional** dependency of `kaspa-txscript(-errors)`, `kaspa-consensus-core`, and `kaspa-consensus`, gated behind a `legacy-secp256k1` feature (default = `pq-only`). The SigCache key is secp-free — `SigAlg {MlDsa87, #[cfg(legacy)] Schnorr/Ecdsa}` + three `[u8; 64]` BLAKE2b digests (design §10), dropping the S3b `secp256k1::Message` fold. `cargo tree -p kaspa-consensus -e normal` now links **zero** secp256k1, enforced as a hard gate by `scripts/pq-ci-guard.sh` (`HARD_SECP_GATE=1` default), wired into the CI `lints` job. **S9 extension (RPC/SDK → node binary):** secp256k1 is also gated out of the **kaspad node binary** — the RPC/SDK path (`kaspa-rpc-core → kaspa-consensus-wasm → kaspa-consensus-client`) now makes secp optional and gates its legacy schnorr signer (`sign_with_multiple_v3`) + error variants behind `legacy-secp256k1`. consensus-client's `wasm32-sdk` profile pulls `legacy-secp256k1` in (the WASM SDK still ships the legacy signer), but the native node never enables `wasm32-sdk`, so `cargo tree -p kaspad -e normal` links **zero** secp256k1. The CI guard now asserts **both** `kaspa-consensus` and `kaspad` are secp-free. **Deferred (non-blocking):** the cosmetic `MlDsa65`→`87` identifier rename. (The wallet-stack secp256k1 gating deferred here is now **done** — see P10.)
- **P10 (wallet-stack secp256k1 feature-gate — audit QL-1):** the `legacy-secp256k1` fence now extends through the entire wallet stack. `secp256k1` is an **optional** dependency of `kaspa-bip32`, `kaspa-wallet-keys`, `kaspa-wallet-pskt`, and `kaspa-wallet-core` (all `default = ["pq-only"]`). The classical key infrastructure — BIP32 extended-key (xprv/xpub) derivation, the secp `PrivateKey`/`PublicKey`/`Keypair`/`XPrv`/`XPub` types, the classical account variants (`bip32`/`multisig`/`legacy`/`watchonly`/`bip32watch`/`keypair`/`resident`), HD + deterministic address derivation, secp message signing, and the whole PSKT/PSKB crate — is gated behind `legacy-secp256k1`, leaving only the ML-DSA-87 (`mldsa`) account, BIP39, and the shared wallet/UTXO/storage infra in the PQ-only build. The single-key ML-DSA account reuses the existing address-*set* UTXO scan path (not the HD `AddressManager`), and the generator's `Signer` routes through the native ML-DSA path (`try_pq_keypair` → `sign_transaction_inputs_mldsa87`); the secp key-map signing path is gated. `kaspa-cli`/`kaspa-daemon`/`kaspa-wallet` default to pq-only and opt into the classical wallet via `legacy-secp256k1`; the WASM SDK keeps the classical signer via `wasm32-sdk → legacy-secp256k1` (the consensus-client precedent). **Net result:** `cargo tree -e normal -i secp256k1` links **zero** secp256k1 for **every** production binary — `kaspad`, `kaspa-pq-cli`, `kaspa-wallet`, `kaspa-cli`, `kaspa-daemon`, `misaminer`, `kaspa-pq-miner`, `kaspa-pq-validator` — with only the `simpa`/`rothschild` sim/load-test tools opting back in. The classical wallet (`--features legacy-secp256k1`) still builds and passes its full suite (wallet-core 49/0/3, wallet-keys 21/0/7, bip32 6/0, pskt 5/0); the PQ-only build is warning-free and passes its subset (wallet-core 30/0, wallet-keys 18/0, bip32 5/0).

### Security note (libcrux advisories)
Local `advisory-db` lists `RUSTSEC-2026-0076` / `-0077` for `libcrux-ml-dsa`, both `patched >= 0.0.8`; we pin `=0.0.9` → **clear**. The design doc's cited `RUSTSEC-2026-0125/0126` are not present in the local advisory-db snapshot. `cargo audit` / `cargo deny check advisories` are wired into the P1 CI guard (`scripts/pq-ci-guard.sh`); as of P8 the secp256k1-tree check is a **hard** gate (`HARD_SECP_GATE=1` default), run by the CI `lints` job.

### Normative launch-scope values (design doc §16.2)
| Item | Value |
|---|---|
| Signature | ML-DSA-87 (FIPS 204, NIST cat 5) |
| Tx signature context | `kaspa-pq-v2/tx/mldsa87` |
| Address version | `PubKeyHashMlDsa87` only |
| Address payload | keyed BLAKE2b-512 (`kaspa-pq-v2/address/mldsa87`), 64 bytes |
| Standard script | ML-DSA-87 P2PKH only |
| P2SH | disabled |
| Legacy secp256k1 opcodes | consensus-disabled |
| ML-DSA sighash | `calc_mldsa87_signature_hash`, 64 bytes (domain `kaspa-pq-v2/sighash/mldsa87`) |
| UTXO commitment | `Hash64` (64 bytes) |
| `MAX_SCRIPT_ELEMENT_SIZE` | 8192 |
| `MAX_SCRIPTS_SIZE` / `max_signature_script_len` | 16_384 |
| `mass_per_sig_op` | 10000 |

The 1.0 body below is retained for history; where it conflicts with this revision (naming, caps, P2SH/premine, address width), **this revision governs**.

---

## Context

Kaspa-pq adopted **ML-DSA-65** (FIPS 204, NIST security category 3) as its sole signature scheme (ADR-0002). Category 3 targets roughly AES-192-equivalent post-quantum security.

We migrate to **ML-DSA-87** (FIPS 204, NIST security category 5) — the highest ML-DSA parameter set, targeting roughly AES-256-equivalent security — to maximize the long-term quantum-resistance margin of the chain before mainnet and before significant value or tooling accretes on the category-3 parameters.

Size impact:

| | ML-DSA-65 (cat 3) | ML-DSA-87 (cat 5) | Δ |
|---|---|---|---|
| Public key | 1952 B | 2592 B | +640 |
| Signature | 3309 B | 4627 B | +1318 |
| Secret key | 4032 B | 4896 B | (derived from a 32-byte seed; never stored) |

The address layer is unaffected in **format** (addresses store a 32-byte BLAKE2b-256 hash of the pubkey — ADR-0003), but every address **value** changes because the hashed pubkey is different.

There is no central switch for the ML-DSA parameter set: the variant is selected by `use libcrux_ml_dsa::ml_dsa_65` repeated across ~9 crates, and the byte sizes are independently hardcoded as named constants in three crates (txscript, consensus-core, rpc-core). This is therefore a **scattered migration**, not a one-line config change.

## Decision

### 1. Switch the ML-DSA parameter set to 87
Change every `libcrux_ml_dsa::ml_dsa_65` import/call site to `ml_dsa_87`: `crypto/txscript` (`lib.rs`, `standard/multisig.rs`, `benches/bench.rs`), `wallet/keys` (`kaspa_pq.rs`, `kaspa_pq_wasm.rs`), `kaspa-pq-validator-core`, `consensus` virtual-processor tests, `wallet/pq-cli`, `rpc/wrpc/examples/kaspa_pq_send`.

### 2. Update size constants (three independent copies)
- `crypto/txscript/src/lib.rs`: `MLDSA65_PK_LEN` 1952→**2592**, `MLDSA65_SIG_LEN` 3309→**4627**
- `consensus/core/src/dns_finality.rs`: `STAKE_VALIDATOR_PUBKEY_LEN` 1952→**2592**, `STAKE_ATTESTATION_SIG_LEN` 3309→**4627**
- `rpc/core/src/model/kaspa_pq.rs`: `RPC_MLDSA65_PK_LEN` 1952→**2592**, `RPC_MLDSA65_SIG_LEN` 3309→**4627**

Existing parity asserts (rpc↔txscript, dns_finality↔txscript) cross-check these and will catch drift at test time. Bare literals (test fixtures, WASM length-check error strings, derivation comments) are updated to match.

### 3. Raise size-derived caps and recalibrate mass
- Raise `MAX_SCRIPT_ELEMENT_SIZE` / `MAX_SCRIPTS_SIZE` (txscript) and `MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE` (mining) to admit the larger 87 pushes. A 2-of-3 ML-DSA-87 multisig redeem script (~7.8 KB) and its unlock script (~16.5 KB) exceed the current 16 384-byte caps.
- Recalibrate `mass_per_sig_op` (`consensus/core/src/config/params.rs`, currently 6000, calibrated to ML-DSA-65 verify cost). ML-DSA-87 verification is slower; re-benchmark and raise so sigops mass still reflects real verify cost.

### 4. New genesis (forced)
The 15 B premine is locked to a 2-of-3 ML-DSA multisig whose redeem-script hash is baked into `MISAKA_PREMINE_P2SH_SCRIPT`, which feeds the genesis `utxo_commitment` and thence all genesis block hashes. ML-DSA-87 keys produce a different redeem script, so we regenerate, in order:
1. multisig keys + address (`gen_misaka_devnet_multisig`) → `misaka-devnet-multisig-keys.json`
2. the premine P2SH constant `MISAKA_PREMINE_P2SH_SCRIPT` (`consensus/core/src/config/premine.rs`)
3. the premine commitment (`print_premine_commitment`)
4. all genesis block hashes (`gen_kaspa_pq_genesis_hashes`, `consensus/core/src/config/genesis.rs`)

### 5. Naming policy (minimal-churn for this pass)
Identifiers keep their existing `MlDsa65` / `MLDSA65_*` spelling for this migration; only **values and behavior** become ML-DSA-87. Each changed size constant carries a clarifying comment. A full identifier rename to `*MlDsa87*` (key types, the WASM `signTransactionMlDsa65` binding, opcode idents, the address-version variant name) is a tracked **cosmetic follow-up**, sequenced separately to keep the behavioral migration reviewable and low-risk.

Unchanged on purpose (variant-agnostic / opaque): opcode byte tags `0xa6` (`OP_CHECKSIGMLDSA65`) and `0xa7` (`OP_CHECKMULTISIGMLDSA65`), the address version byte (2), the 32-byte validator/wallet seed length, and the signing domain-separation context strings (`b"kaspa-pq-v1/tx/mldsa65"`, `.../att/mldsa65`, `.../takeover/mldsa65`) — kept identical so signer and verifier stay in lock-step.

## Consequences

### Positive
- Maximum NIST category-5 post-quantum security margin for the chain.

### Negative / operational
- **New chain.** No compatibility with any existing kaspa-pq genesis, UTXO set, or address. A full from-genesis redeploy of all devnet hosts plus a WASM wallet rebuild is required.
- All keypairs/addresses are functionally new (format identical, values different).
- External consumers (misakascan, misaka-api, wallet.misakascan) must regenerate addresses and update the WASM SDK.
- Larger transactions and higher mass for every PQ spend (+640 B pubkey, +1318 B signature revealed at spend time).
- `MlDsa65`-spelled identifiers temporarily denote ML-DSA-87 until the rename follow-up lands.

## Alternatives Considered
- **Stay on ML-DSA-65.** Rejected per the migration directive; category 5 chosen for long-term margin while the ecosystem is still small.
- **Full identifier rename in this same pass.** Deferred: high churn across 9 crates and it would break the external WASM JS API name mid-migration. Sequenced as a separate cosmetic pass.
- **Parameterize the variant behind a feature/const (single switch).** Rejected for now: a larger refactor than the migration itself; the scattered `ml_dsa_NN` imports plus per-crate size constants would all need to route through one module. Revisit if a third parameter set is ever contemplated.

## References
- FIPS 204 (ML-DSA)
- Supersedes [ADR-0002](0002-mldsa65-p2pkh.md); forces new genesis per [ADR-0008](0008-pq-genesis-premine.md); address format unchanged per [ADR-0003](0003-pq-address-format.md); mass policy per [ADR-0005](0005-mass-policy.md)
- `libcrux-ml-dsa` crate (`ml_dsa_87` submodule)
