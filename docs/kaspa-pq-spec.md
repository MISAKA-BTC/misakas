# kaspa-pq Specification (v0.9, draft)

Status: Draft. Frozen values listed here are the contract every phase must
respect. Any change must go through an ADR update under `docs/adr/`. v0.9
(2026-10-02) re-checked every concrete value below against the code on this
branch; the current network is testnet-12 (`release.json`).

> **Signature scheme:** **ML-DSA-87** per [ADR-0019 rev 1.2](adr/0019-mldsa87-migration.md) (supersedes the ML-DSA-65 scheme of [ADR-0002](adr/0002-mldsa65-p2pkh.md); the P2PKH structure stands). Background and rationale: [the design doc](design/kaspa-pq-design-mldsa87.md) (point-in-time; this spec is normative where they differ). Test coverage: [`test-plan-kaspa-pq.md`](test-plan-kaspa-pq.md).

> **Finality model (audit H-02 — precise wording).** kaspa-pq does **not** claim a "double Nakamoto confirmation" or BFT finality. The DNS confirmation predicate (`is_dns_confirmed`, `consensus/core/src/dns_finality.rs`) advances the confirmed anchor iff `WorkDepth ≥ required_work_depth ∧ StakeDepth ≥ required_stake_depth`, where `WorkDepth` is the blue work accumulated **since** the anchor became the canonical lagged anchor (not cumulative-from-genesis work). `PRODUCTION_DNS_PARAMS.required_work_depth` is a non-zero calibration floor (`1_000_000`); testnet-12 (`PALW_T12_DNS_PARAMS`) runs the production set with testnet's `100`; the devnet/simnet set (`GENESIS_ACTIVE_DNS_PARAMS`) keeps `ZERO` (stake-only). The two-dimensional `WorkScore × StakeScore` dominance is enforced separately, at the **reorg gate** (`TwoDimensionalDominance`) on candidate reorgs.

The PQ-phase ADRs (0001–0015) are indexed in §12; the maintained index,
including which of them were later superseded (ADR-0002 → 0019, ADR-0012 →
0017, the voting-weight half of ADR-0009 → 0024), is
[`docs/adr/README.md`](adr/README.md).

## 0. Scope and non-goals

This document specifies a quantum-resistant Kaspa-based network ("kaspa-pq")
forked from rusty-kaspa. It is **not** a compatibility layer with the
mainline Kaspa network.

### In scope

1. Signature scheme replacement: ML-DSA-87 (FIPS 204) P2PKH only. Address
   payload is the 64-byte **keyed** BLAKE2b-512 of the public key under
   `kaspa-pq-v2/address/mldsa87` (ADR-0008, ADR-0019).
2. UTXO accumulator replacement: LtHash32_1024. Final commitment is the
   64-byte keyed BLAKE2b-512 (domain `UtxoCommitment64`) of the 4096-byte
   LtHash state (`Header.utxo_commitment: Hash64`, ADR-0004/0008).
3. Network-level isolation (NetworkId, genesis, address prefix, ports).
4. **Layered PoW** (ADR-0007): Layer 0 is the consensus-critical
   BLAKE2b-512 finalizer over a 512-bit comparison domain; Layer 1 is the
   `algo_id`-identified work tag. `BlueWorkType = Uint576`.
   `Header.pow_algo_id` is carried on the P2P `BlockHeader` proto
   (`protocol/p2p/src/convert/header.rs`) and consensus enforces the
   algo id required at each DAA score (`check_algo_id_for_mode_accepting`,
   `consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs`).
   The ids are defined in `consensus/core/src/pow_layer0.rs` (`1` =
   kHeavyHash-compatible, plus the later PALW / heartbeat ids). testnet-12 is
   a PALW `ConsensusV2` network that activates no V1 PALW proof-of-work
   (`validate_palw_v2`); PALW is specified by its own ADRs (see the
   maintained index).
5. **64-byte consensus identity** end-to-end (ADR-0008). Block hash,
   transaction id, transaction hash, merkle root, accepted-id merkle
   root, UTXO commitment, pruning point, parent references all move
   from 32-byte `Hash` to 64-byte `Hash64`. The 32-byte type remains as
   `Hash32` for incidental internal use (debug fingerprints, the Layer 1
   kHeavyHash internals).
6. **DNS Probabilistic Finality Overlay** (ADR-0009) as a Phase 10
   post-launch consensus layer. PoW/GHOSTDAG keeps block production and
   tip selection unchanged; every active bond issues ML-DSA-87 attestations
   over selected-chain anchors (all-active, no sortition — ADR-0017), those
   attestations are committed on-chain as shards, and a deterministic
   `StakeScore` is aggregated from them. testnet-12 runs the overlay from
   genesis (`dns_params = Some(PALW_T12_DNS_PARAMS)`).
   Mainnet reorgs that exit a DNS-confirmed prefix require **both**
   `WorkScore` dominance and `StakeScore` dominance — no hard finality
   checkpoint.

### Out of scope

- Mainline Kaspa interoperability (wallet, RPC, P2P, address). kaspa-pq
  is a separate network (ADR-0001), not a soft-/hard-fork of mainline.
- ML-DSA multisig and script-hash (P2SH) composite scripts — P2SH is
  consensus-disabled in PQ-only mode (`ScriptHashDisabledInPqMode`). The
  P2PKH ML-DSA-87 template is the only standard send. (Smart contracts are
  not part of the UTXO script layer; they live in the separate EVM lane,
  ADR-0020.)
- Hardware-wallet support; BIP32-style hierarchical key derivation that
  requires a discrete-log-friendly curve.

### Public-claim discipline (binding)

The kaspa-pq Phase 9 security claim, taken verbatim from ADR-0008 §"Security
framing":

- ✅ "512-bit commitment domain"
- ✅ "256-bit quantum preimage margin" (Grover bound)
- ✅ "high-margin quantum collision resistance"
- ❌ "256-bit quantum collision" — **not claimed**
- ❌ "256-bit post-quantum security" (across the board) — **not claimed**

Quantum collision resistance under the BHT bound is approximately
`2^(512/3) ≈ 2^170`, not `2^256`. External material must use the
phrasings above and **must not** over-claim collision resistance.

The kaspa-pq Phase 10 DNS finality claim, taken verbatim from
ADR-0009 §"Public-claim discipline (binding)":

- ✅ "PoW-ledger + PoS probabilistic finality"
- ✅ "Two-resource confirmed history"
- ✅ "Deep reorg of a DNS-confirmed prefix requires both `WorkScore` and
  `StakeScore` dominance"
- ✅ "Non-substitutability: PoW surplus does not substitute for PoS
  deficit and vice versa"
- ✅ "Liveness depends on both PoW miners and PoS validators while the
  overlay is active"
- ✅ "Weak subjectivity remains: new nodes need a recent peer-supplied
  checkpoint to safely rejoin"
- ❌ "BFT finality" / "hard finality" — **not claimed**. Mainnet DNS is
  probabilistic. The PoC hard-checkpoint mode is a testing convenience.
- ❌ "Reorg probability is the product of PoW and PoS reorg probabilities"
  — **not claimed**. The DNS paper explicitly does not claim joint
  independence; the overlay's value is non-substitutability.
- ❌ "DNS gives 2^k post-quantum finality" — **not claimed** without an
  explicit `cW`, `cS`, `emergency_work_margin`, and
  `emergency_stake_margin` quote for the network in question.

## 1. Base version

- Upstream: `rusty-kaspa` workspace package version `1.1.0`
  (see [Cargo.toml](../Cargo.toml) → `[workspace.package].version`).
- The original vendoring commit (`vendor: rusty-kaspa v1.1.0 base`) is in
  the full git history (not in shallow clones).
- The vendored snapshot is treated as a hard pin. Upstream merges must be
  reviewed against this specification before being accepted.

## 2. Frozen constants

These constants are normative. Implementations must use exactly these values
unless a follow-up ADR amends them.

| Constant | Value | Where it appears |
|---|---|---|
| `MLDSA87_PK_LEN`  | `2592`  | ML-DSA-87 public key length (bytes) |
| `MLDSA87_SIG_LEN` | `4627`  | ML-DSA-87 signature length (bytes) |
| signature item | `4628` | signature + 1-byte sighash type |
| `LTHASH_LANES`    | `1024`  | Number of 32-bit lanes in LtHash state |
| `LTHASH_LANE_BYTES` | `4`   | Bytes per lane |
| `LTHASH_STATE_BYTES` | `4096` | Serialized accumulator state size |
| UTXO commitment | `64` bytes | `Header.utxo_commitment: Hash64` |
| `MAX_SCRIPT_ELEMENT_SIZE` (kaspa-pq) | `8192` | fits 4628 sig + 2592 pk (up from upstream `520`) |
| `MAX_SCRIPTS_SIZE` / `max_signature_script_len` | `16_384` | md2; `max_script_public_key_len` stays `10_000` |
| `MAX_STACK_SIZE` (kaspa-pq) | `244` | initial value, unchanged from upstream |
| Signature context (tx) | `"kaspa-pq-v2/tx/mldsa87"` | ML-DSA `ctx` parameter |
| Sighash domain | `"kaspa-pq-v2/sighash/mldsa87"` | sighash transcript domain tag |
| Address payload | keyed BLAKE2b-512(`"kaspa-pq-v2/address/mldsa87"`, vk) → 64 B | P2PKH-ML-DSA-87 |
| Wallet keygen domain | `"kaspa-pq-wallet-v1/mldsa87/keygen"` | XOF domain separator |

Code homes: `crypto/txscript/src/lib.rs` (sizes, stack, tx context),
`consensus/core/src/hashing/sighash.rs` (`MLDSA87_SIGHASH_DOMAIN`),
`crypto/hashes/src/lib.rs` (`MLDSA87_ADDRESS_CONTEXT`),
`crypto/muhash/src/lib.rs` (LtHash), `wallet/keys/src/kaspa_pq.rs`
(`KASPA_PQ_WALLET_KEYGEN_DOMAIN`).

## 3. Cryptographic decisions

### 3.1 Signature

- Algorithm: ML-DSA-87 (FIPS 204), pure mode, with a fixed `ctx` value
  (see §2 and [ADR-0019](adr/0019-mldsa87-migration.md)).
- Library: `libcrux-ml-dsa = "=0.0.10"` (exact pin, workspace `Cargo.toml`).
- Verify-time pre-checks: signature length and public-key length must be
  validated **before** calling into `verify`.

### 3.2 UTXO accumulator

- Algorithm: LtHash32_1024 (Meta).
- State: 1024 lanes × 32 bits = 4096 bytes.
- Element serialization includes the spending outpoint to defeat the
  2^16 duplication wrap-around.
- See [ADR-0003](adr/0003-lthash-utxo-accumulator.md).

### 3.3 UTXO commitment

- Keyed BLAKE2b-512 (domain `UtxoCommitment64`) of the LtHash state →
  64-byte `Hash64` (`MuHash::finalize`, `crypto/muhash/src/lib.rs`).
- See [ADR-0004](adr/0004-utxo-commitment64.md).

### 3.4 Hash widths

- Every consensus identity — block hash, txid, merkle roots, pruning
  point, parent references, UTXO commitment, address payload — is a 64-byte
  `Hash64` (ADR-0008). `Hash` (= `Hash32`) remains only for the legacy
  32-byte kHeavyHash PoW path and incidental internal use.
- The EVM lane's Ethereum trie roots are 32 bytes and live in the block
  body's `EvmExecutionHeader`; the L1 header carries a single 64-byte
  `evm_commitment_root`.

## 4. Network identity

- New `NetworkId`, genesis block, address prefixes (`misaka` / `misakatest`
  / `misakasim` / `misakadev`, `crypto/addresses/src/lib.rs`), per-network
  ports (`consensus/core/src/network.rs`), DNS seed list, gRPC proto package
  `protowire.kaspapq`, initial UTXO commitment.
- See [ADR-0001](adr/0001-network-isolation.md).

## 5. Standard transaction format

### 5.1 Address

- `Version::PubKeyHashMlDsa87 = 2`.
- Payload: keyed BLAKE2b-512(`kaspa-pq-v2/address/mldsa87`, public_key) =
  64 bytes (`PAYLOAD_VECTOR_SIZE = 64`).
- The `payload` field of an address is **never** a raw ML-DSA public key.

### 5.2 scriptPubKey (output)

```
OP_DUP
OP_BLAKE2B_512                 ; 0xc4
OP_DATA64 <keyed BLAKE2b-512(public_key) — 64-byte address payload>
OP_EQUALVERIFY
OP_CHECKSIG_MLDSA87            ; 0xa6
```

Exactly 69 bytes per output (the address payload is the **keyed** BLAKE2b-512
of the public key under `kaspa-pq-v2/address/mldsa87`; md2 §4.2 / ADR-0019).

### 5.3 signatureScript (input)

```
PUSH <signature || sighash_type>     ; 4627 + 1 = 4628 bytes payload
PUSH <ML-DSA-87 public key>          ; 2592 bytes payload
```

(The ML-DSA-87 signature is 4627 bytes and the public key 2592 bytes;
ADR-0019.)

### 5.4 sighash

`calc_mldsa87_signature_hash` (`consensus/core/src/hashing/sighash.rs`)
returns a 64-byte `Hash64` under the `kaspa-pq-v2/sighash/mldsa87` domain;
signer and verifier must both call it. The ML-DSA `ctx` parameter binds the
signature to the scheme (see §2). (The legacy secp256k1 sighash functions are
compiled only under the `legacy-secp256k1` feature.)

## 6. Mass / DoS policy

Current preset values (`consensus/core/src/config/params.rs`); rationale in
[ADR-0005](adr/0005-mass-policy.md).

| Parameter | Value | Notes |
|---|---|---|
| `mass_per_tx_byte` | `1` | unchanged from upstream |
| `mass_per_script_pub_key_byte` | `10` | unchanged from upstream |
| `mass_per_sig_op` | `10000` | raised from upstream `1000` for ML-DSA verify cost |
| `max_block_mass` | `500_000` | unchanged from upstream |
| `MAX_SCRIPTS_SIZE` / `max_signature_script_len` | `16_384` | fits one ML-DSA-87 P2PKH input |
| `max_script_public_key_len` | `10_000` | unchanged |
| `MAX_SCRIPT_ELEMENT_SIZE` | `8192` | fits the 4628-byte signature item |

## 7. SigCache shape

ML-DSA-87 public keys and signatures are far too large to keep verbatim
in a hot signature-verification cache. The cache key
(`SigCacheKey`, `crypto/txscript/src/lib.rs`) is:

```
struct SigCacheKey {
    sig_alg: SigAlg,                // MlDsa87 (the only consensus-active scheme)
    pub_key_digest: [u8; 64],       // BLAKE2b-512 of public key bytes
    signature_digest: [u8; 64],     // BLAKE2b-512 of signature bytes
    message_digest: [u8; 64],       // the 64-byte ML-DSA-87 sighash
}
```

The signature-verification cache must not hold full public keys or
signatures by value. This is both a memory-DoS mitigation and an
allocation policy decision.

## 8. Wallet key derivation

- BIP39 mnemonic → 64-byte master seed: reused unchanged.
- BIP32-style hierarchical derivation (secp256k1): **not used**.
- Per-account / per-index seed:

```
keygen_seed =
    keyed_BLAKE2b-256(
        key   = "kaspa-pq-wallet-v1/mldsa87/keygen",
        input = len(network_id)_le_u32 || network_id || account_le_u32 ||
                change_le_u32 || index_le_u32 ||
                len(master_seed)_le_u32 || master_seed
    )                                   ; 32 bytes
keypair = MLDSA87.KeyGen(keygen_seed)
```

Normative implementation: `derive_keygen_seed` in
`wallet/keys/src/kaspa_pq.rs`. `network_id` is the `NetworkId` string form,
so one mnemonic yields distinct addresses per network.

## 9. Compatibility and migration

There is **no** migration path between mainline Kaspa and kaspa-pq.
This is by design: the address format, accumulator, and signature scheme
are all different. A separate one-shot migration tool is out of scope
for the PoC.

## 10. Phase history

The PQ build-out ran as 13 phases (spec freeze; network isolation; LtHash;
ML-DSA P2PKH; wallet derivation + CLI; mass policy; RPC/WASM/SDK; Layered
PoW; Hash64 consensus identity; DNS overlay; validator node architecture;
validator deployment + equivocation safety; mainnet completeness). The
per-PR slot tables that tracked them through v0.8 are in git history. The
Hash64 cascade (PR-9.5) and the DNS overlay / validator work are implemented
in the tree (e.g. `consensus/core/src/dns_finality.rs`, the
`kaspa-pq-validator`, `kaspa-pq-validator-core` and `kaspa-pq-signer`
crates, the `getDnsConfirmation` RPC), with the decision changes recorded in
later ADRs (all-active attestation, ADR-0017, replaced ADR-0012's
commit-reveal sortition). Later work (EVM lane, PALW) is governed by its own
ADRs.

## 11. Test plan

The test plan is [`test-plan-kaspa-pq.md`](test-plan-kaspa-pq.md). The
baseline acceptance properties this spec pins:

- Existing Kaspa mainnet/testnet nodes are rejected at handshake.
- Add-then-remove on LtHash returns the empty-state commitment; serialized
  state is exactly 4096 bytes; invalid-block rollback leaves the
  accumulator consistent with a slow recompute.
- A well-formed ML-DSA-87 P2PKH spend is accepted; any
  length/context/hash mismatch is rejected before `verify` is called.
- The Layer 0 finalizer is deterministic, all input fields influence the
  digest, and the length-prefixed `l1_tag` defeats the canonical-concat
  collision attack.
- Every 64-byte hash round-trips through hex (128 chars) and Borsh (64 raw
  bytes); the keyed BLAKE2b-512 hashers are pairwise-separating on the same
  input.

## 12. ADR index

> **The maintained index is [`docs/adr/README.md`](adr/README.md)** — it covers every ADR
> together with the supersede map that says which decisions were later reversed. The list
> below is the PQ-phase snapshot (ADR-0001–0015) and is not extended.

- [ADR-0001 — Network isolation](adr/0001-network-isolation.md)
- [ADR-0002 — ML-DSA-65 P2PKH](adr/0002-mldsa65-p2pkh.md) — scheme superseded by [ADR-0019](adr/0019-mldsa87-migration.md) (ML-DSA-87)
- [ADR-0003 — LtHash32_1024 UTXO accumulator](adr/0003-lthash-utxo-accumulator.md)
- [ADR-0004 — 64-byte UTXO commitment](adr/0004-utxo-commitment64.md)
- [ADR-0005 — Mass / DoS policy](adr/0005-mass-policy.md)
- [ADR-0006 — RPC / WASM / SDK types](adr/0006-rpc-wasm-sdk-types.md)
- [ADR-0007 — Layered PoW](adr/0007-layered-pow.md)
- [ADR-0008 — Full Hash64 consensus identity](adr/0008-hash64-consensus-identity.md) (256-bit quantum preimage margin, **not** 256-bit quantum collision)
- [ADR-0009 — DNS Probabilistic Finality Overlay](adr/0009-dns-probabilistic-finality.md) — voting-weight half superseded by [ADR-0024](adr/0024-verified-llm-token-weighted-bft.md)
- [ADR-0010 — Validator Node Architecture](adr/0010-validator-node-architecture.md)
- [ADR-0011 — Validator Single-Host Deployment + Equivocation-Safety](adr/0011-validator-deployment-and-equivocation-safety.md)
- [ADR-0012 — Mainnet Validator Sortition via On-Chain Commit-Reveal](adr/0012-mainnet-validator-sortition-commit-reveal.md) — **superseded in whole** by [ADR-0017](adr/0017-all-active-staker-attestation.md)
- [ADR-0013 — Validator Reward Distribution](adr/0013-validator-reward-distribution.md)
- [ADR-0014 — Coordinated-Failover Protocol for Validator Hosts](adr/0014-coordinated-failover-protocol.md)
- [ADR-0015 — Remote-Signer / HSM Protocol for Validator Signing](adr/0015-remote-signer-hsm-protocol.md)

## 13. Revision history

| Version | Date | Change |
|---|---|---|
| 0.1 | 2026-05-28 | Initial draft. |
| 0.2 | 2026-05-28 | ADR-0007 + ADR-0008 incorporated. Removed the "do not widen Hash past 32 bytes" non-goal (it directly contradicts ADR-0008); added the full 64-byte consensus identity goal; added the Phase 8 / Phase 9 entries to the phase plan; codified the public-claim discipline section. Revised non-goal removal: previously `PQ-strengthening the PoW hash, block hash, txid, or merkle root` was listed as out-of-scope; this is now the explicit Phase 8 + Phase 9 in-scope work. |
| 0.3 | 2026-05-28 | ADR-0009 incorporated. Added in-scope item 6 (DNS Probabilistic Finality Overlay) and Phase 10 row in the phase plan. Codified the DNS-specific public-claim discipline section (binding) — explicitly rejecting "hard finality", "reorg-probability product", and "2^k post-quantum finality" framings. Added Phase 10 acceptance criteria to §11 (test plan). |
| 0.4 | 2026-05-28 | ADR-0010 incorporated. Added Phase 11 row (operational design, no new consensus surface) to the phase plan, with the refined 14-slot Phase 10 implementation roadmap from ADR-0010 §"Phase 10 PR plan" inlined as a sub-table. Added Phase 11 acceptance criteria (8-step operator runbook + byte-identical `validator_set_commitment` across nodes) to §11 (test plan). Added ADR-0010 to the ADR index (§12). Renumbered §12 → §13 to fix the pre-existing duplicate-§11 mis-numbering; ADR-0006's "§11 (ADR index)" reference updated to "§12 (ADR index)" in the same commit. |
| 0.5 | 2026-05-28 | ADR-0011 incorporated. Added Phase 12 row (operational design, no new consensus surface) to the phase plan; widened the Phase 10 PR sub-table with the `'`-suffixed implementation sub-slots (PR-10.6′ sidecar binary, PR-10.6″ signed-epoch store, PR-10.6‴ `--dry-run`, PR-10.13′ CLI validator commands, PR-10.14′ `getValidatorStatus` RPC + sidecar smoke). Added Phase 12 acceptance criteria (sidecar-shape end-to-end runbook + `check_signed_epoch_record` decision matrix + 100-epoch `--dry-run` sweep emitting zero on-chain attestations) to §11 (test plan). Added ADR-0011 to the ADR index (§12). |
| 0.6 | 2026-05-28 | ADR-0012 incorporated (Phase 13, 1/4). Added Phase 13 row (🚧 1/4 ADRs landed — mainnet completeness phase covering sortition + rewards + failover + HSM); refined the PR sub-table with PR-13.1 / PR-13.2 / PR-13.3 entries and rewrote the PR-10.9 entry to reference ADR-0012 explicitly (was: "PoC deterministic; mainnet commit-reveal in a follow-up ADR"). Added Phase 13 (1/4) acceptance criteria to §11 (test plan) — sortition determinism, commit-reveal cycle, ≥ 2/3 threshold boundary pin, stake-weighted committee bias-test, commit-without-reveal slashing, fallback-chain bottom-out at `Hash64::ZERO` for `epoch == 0`. Added ADR-0012 to the ADR index (§12) with the explicit "NOT an unbiased random oracle" framing per ADR-0012 §"Public-claim discipline". |
| 0.7 | 2026-05-28 | ADR-0013 incorporated (Phase 13, 2/4). Phase 13 row flipped to 🚧 2/4. Refined PR sub-table with PR-13.4 / PR-13.5 / PR-13.6 entries and the two `'`-suffixed implementation sub-slots (PR-10.5′ coinbase fan-out, PR-10.12′ slashing distribution). Added Phase 13 (2/4) acceptance criteria to §11 (test plan) — coinbase fan-out emits `N + 1` outputs landing at the owner address (cold key), `compute_attestation_reward_payouts` cap + refund correctness, `compute_slashing_distribution` 30-case invariant matrix (reporter + burned == slashed across 5 amounts × 6 bps including u64::MAX no-overflow), mainnet 1000-bps recommendation pinned, `apply_unreveal_reporter_min_cap` clamp + surplus-to-burn behaviour. Added ADR-0013 to the ADR index (§12) with the explicit "tx fees stay with miners" / "rewards to owner cold key" / "NOT guaranteed" framing. |
| 0.8 | 2026-05-28 | ADR-0014 + ADR-0015 incorporated (Phase 13, 4/4 — closes the Phase 13 design freeze). Phase 13 row flipped to ✅ design-freeze landed. Refined PR sub-table with PR-13.7 (ADR-0014), PR-13.8 (failover types), PR-13.9 (ADR-0015), PR-13.10 (signer types), PR-13.11 (this PR — spec close), and six new `'`-suffixed implementation sub-slots (PR-10.6‴′ failover CLI, PR-10.6′′′′ signer binary, PR-10.6′′′′a validator handshake, PR-10.12′′ Strict-mode signer-side equivocation guard, PR-10.12′′a Pkcs11Adapter, PR-10.14′′ failover smoke test). Added Phase 13 (3/4 — coordinated failover) and (4/4 — remote-signer / HSM) acceptance criteria to §11 (test plan) — TakeoverToken handoff determinism, replay-rejection, anti-spoofing host_id, slashing-acknowledged emergency path; SIGNER_PROTOCOL_VERSION handshake mismatch handling, capability bitflag composition, SignerRequest/Response Result-arm round-trip, Strict-mode signer-side equivocation guard rejection, BLAKE2b-512-chained audit log tamper-detection. Added ADR-0014 and ADR-0015 to the ADR index (§12), each with the full NOT-claimed framing required by their respective public-claim discipline sections. |
| 0.9 | 2026-10-02 | Re-checked against the code. Normative values moved to ML-DSA-87 throughout (address payload 64-byte keyed BLAKE2b-512, `libcrux-ml-dsa =0.0.10`, `SigCacheKey` 64-byte digests, keyed BLAKE2b-256 keygen seed, `misaka*` prefixes, Hash64 everywhere, `mass_per_sig_op = 10000`); finality banner matches the non-zero `required_work_depth`; `pow_algo_id` is now on the P2P header; §10 phase/PR tables and the Phase 10–13 acceptance criteria replaced by a short history and a pointer to the test plan; ADR index trimmed to one line per ADR with supersede notes. |
