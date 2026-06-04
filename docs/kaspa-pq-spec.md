# kaspa-pq Specification (v0.8, draft)

Status: Draft. Frozen values listed here are the contract every phase must
respect. Any change must go through an ADR update under `docs/adr/`.

> **md2 alignment (2026-06-01):** the signature scheme is now **ML-DSA-87** (sizes/version/context in the normative table below updated) per [ADR-0019 rev 1.2](adr/0019-mldsa87-migration.md) + [the design doc](kaspa-pq-design-mldsa87.md). Tx/sighash contexts are `kaspa-pq-v2/{tx,sighash}/mldsa87`; the address payload is a **keyed** BLAKE2b-512 under `kaspa-pq-v2/address/mldsa87`; `MAX_SCRIPTS_SIZE` / `max_signature_script_len` are `16_384` and `MAX_SCRIPT_ELEMENT_SIZE` is `8192`. Any remaining `*65*` / `kaspa-pq-v1/{tx,sighash}` / `4096` mentions in the prose below are historical (ML-DSA-65 draft era).

> **Finality model (audit H-02 — precise wording).** kaspa-pq does **not** claim a "double Nakamoto confirmation". The DNS *confirmed anchor* is a **stake-confirmed canonical lagged anchor**: in `PRODUCTION_DNS_PARAMS`, `required_work_depth = 0`, so the confirmation predicate advances on `StakeScore` over the canonical (blue_score-coordinated, header-committed) anchor — it does **not** independently require a PoW depth. The two-dimensional `WorkScore × StakeScore` dominance is enforced separately, at the **reorg gate** (`TwoDimensionalDominance`) on candidate reorgs, **not** in the confirmation predicate. Deep-reorg safety therefore comes from PoW block production + GHOSTDAG + the reorg gate; DNS finality adds a stake-confirmed canonical anchor on top. (To make confirmation itself PoW-gated — option A — set `required_work_depth > 0` and define work depth as `blue_work(tip) − blue_work(confirmed_anchor)`; deferred. Where the prose below says DNS confirmation "binds … to both WorkScore and StakeScore", read it as the reorg-gate property.)

ADR-0007 (Layered PoW) and ADR-0008 (Hash64 consensus identity) widen
the original "signatures + UTXO accumulator" target to "full 64-byte
consensus identity + 512-bit PoW domain". ADR-0009 (DNS Probabilistic
Finality Overlay) adds a Phase 10 post-launch confirmation layer that
binds deep-reorg safety to both `WorkScore` and `StakeScore`.
ADR-0010 (Validator Node Architecture) supplements ADR-0009 with the
in-process validator architecture — node-role separation, CLI flags,
subsystem file layout, validator-service runtime, block-template
policy, and the 8-step operator runbook. ADR-0011 (Validator
Single-Host Deployment + Equivocation-Safety Operating Model)
extends ADR-0010 with the production-recommended sidecar deployment
shape, the validator-local equivocation guard
(`SignedEpochRecord` + `check_signed_epoch_record`), the 9-variant
`ValidatorStatus` enum, binding policy for key separation
(validator key on the host, owner key **not** on the host), and the
slashing-scope binding (equivocation slashed, downtime not).
ADR-0012 (Mainnet Validator Sortition via On-Chain Commit-Reveal)
closes the ADR-0009 §"Sortition" mainnet TBD pointer with a
three-phase commit-reveal pipeline (commit at `E−2`, reveal at
`E−1`, sortition use at `E`), Byzantine-fault-tolerant
≥ 2/3 fallback rule, stake-weighted top-K committee selection, and
on-chain slashing for commit-without-reveal. ADR-0013 (Validator
Reward Distribution) closes the two reward / slashing loose ends
inherited from ADR-0009 and ADR-0012: per-attestation flat reward
funded by a new inflation track (tx fees stay 100% with PoW
miners, validator rewards land at the **owner** address per
ADR-0011 key separation), inflation cap with a defensive refund,
and the binding `reporter / burned` split for both equivocation
and unreveal slashing (mainnet recommendation 1000 bps = 10% to
reporter, remainder burned). ADR-0014 (Coordinated-Failover
Protocol) resolves the ADR-0011 "one key, one host" future-ADR
pointer with a node-local, signature-bound `TakeoverToken` that
explicitly transfers signing authority from a primary host to a
standby at a specific future epoch; the protocol is
honest-operator-oriented, with the consensus-side
`SlashingEvidencePayload` remaining the malicious-operator
safety net. ADR-0015 (Remote-Signer / HSM Protocol) resolves
the ADR-0010 "key management on a hot node" Negative by
specifying a Unix-domain-socket protocol (length-prefixed Borsh)
between a validator client and a separate `kaspa-pq-signer`
process; covers every ML-DSA-65 use site uniformly (transaction,
attestation, takeover-token); supports three policy modes
(`Permissive` / `AuditOnly` / `Strict`), with `Strict`
relocating the equivocation guard from the validator client to
the signer; HSM backends pluggable behind an internal
`SignerBackend` trait; tamper-evident BLAKE2b-512-chained audit
log. Earlier Phase 1 non-goals that contradicted any of these
ADRs have been removed; see the revision history below.

## 0. Scope and non-goals

This document specifies a quantum-resistant Kaspa-based network ("kaspa-pq")
forked from rusty-kaspa. It is **not** a compatibility layer with the
mainline Kaspa network.

### In scope

1. Signature scheme replacement: ML-DSA-65 (FIPS 204) P2PKH only. Address
   payload is the 64-byte BLAKE2b-512 hash of the public key
   (ADR-0008 §"Address payload width").
2. UTXO accumulator replacement: LtHash32_1024. Final commitment is the
   64-byte BLAKE2b-512 of the 4096-byte LtHash state
   (`UtxoCommitmentHash64`, ADR-0008).
3. Network-level isolation (NetworkId, genesis, address prefix, ports).
4. **Layered PoW** (ADR-0007): Layer 0 is the consensus-critical
   BLAKE2b-512 finalizer over a 512-bit comparison domain; Layer 1 is the
   `algo_id`-identified ASIC-resistance tag (`algo_id = 1` =
   kHeavyHash-compatible at Phase 1; ASIC-hard variants are Phase
   2+ separate hard-fork ADRs). `BlueWorkType = Uint576` in Phase 1.
   **Phase-1 wire constraint (audit M-05):** `pow_algo_id` is consensus-fixed
   to `POW_ALGO_ID_KHEAVYHASH = 1` and is **NOT** carried on the P2P
   `BlockHeader` proto (the field is absent); on deserialization from a P2P
   message the `Header.pow_algo_id` is always initialized to `1`
   (`protocol/p2p/src/convert/header.rs`), and consensus rejects any other
   value (`check_algo_id_phase1`). Adding `algo_id ≥ 2` is a Phase-2 change
   that must add the proto field + a hard-fork activation together (ADR-0007).
5. **64-byte consensus identity** end-to-end (ADR-0008). Block hash,
   transaction id, transaction hash, merkle root, accepted-id merkle
   root, UTXO commitment, pruning point, parent references all move
   from 32-byte `Hash` to 64-byte `Hash64`. The 32-byte type remains as
   `Hash32` for incidental internal use (cache keys, debug
   fingerprints, the Layer 1 kHeavyHash internals).
6. **DNS Probabilistic Finality Overlay** (ADR-0009) as a Phase 10
   post-launch consensus layer. PoW/GHOSTDAG keeps block production and
   tip selection unchanged; PoS validators issue ML-DSA-65 attestations
   over selected-chain anchors, those attestations are committed
   on-chain as partial certificates (8–16 per block), and a
   deterministic `StakeScore` is aggregated from the on-chain shards.
   Mainnet reorgs that exit a DNS-confirmed prefix require **both**
   `WorkScore` dominance and `StakeScore` dominance — no hard finality
   checkpoint.

### Out of scope

- Mainline Kaspa interoperability (wallet, RPC, P2P, address). kaspa-pq
  is a separate network (ADR-0001), not a soft-/hard-fork of mainline.
- ML-DSA multisig, script-hash composite scripts, smart contracts. The
  P2PKH ML-DSA-65 template is the only standard send.
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
- Vendoring commit recorded at the repository root as the initial git commit
  `vendor: rusty-kaspa v1.1.0 base`.
- The vendored snapshot is treated as a hard pin. Upstream merges must be
  reviewed against this specification before being accepted.

## 2. Frozen constants

These constants are normative. Implementations must use exactly these values
unless a follow-up ADR amends them.

| Constant | Value | Where it appears |
|---|---|---|
| `MLDSA87_PK_LEN`  | `2592`  | ML-DSA-87 public key length (bytes) |
| `MLDSA87_SIG_LEN` | `4627`  | ML-DSA-87 signature length (bytes) |
| `MLDSA87_SIG_ITEM_MAX_LEN` | `4628` | signature item incl. 1-byte sighash type |
| `LTHASH_LANES`    | `1024`  | Number of 32-bit lanes in LtHash state |
| `LTHASH_LANE_BYTES` | `4`   | Bytes per lane |
| `LTHASH_STATE_BYTES` | `4096` | Serialized accumulator state size |
| `UTXO_COMMITMENT_BYTES` (production) | `64` | Header UTXO commitment field |
| `UTXO_COMMITMENT_BYTES` (PoC, optional) | `32` | PoC-only shortcut, must be flagged |
| `MAX_SCRIPT_ELEMENT_SIZE` (kaspa-pq) | `8192` | fits 4628 sig + 2592 pk (up from upstream `520`) |
| `MAX_SCRIPTS_SIZE` / `max_signature_script_len` | `16_384` | md2; `max_script_public_key_len` stays `10_000` |
| `MAX_STACK_SIZE` (kaspa-pq) | `244` | initial value, unchanged from upstream |
| Signature context (tx) | `"kaspa-pq-v2/tx/mldsa87"` | ML-DSA `ctx` parameter |
| Sighash domain | `"kaspa-pq-v2/sighash/mldsa87"` | sighash transcript domain tag |
| Address payload | keyed BLAKE2b-512(`"kaspa-pq-v2/address/mldsa87"`, vk) → 64 B | P2PKH-ML-DSA-87 |
| Wallet keygen domain | `"kaspa-pq-wallet-v1/mldsa87/keygen"` | XOF domain separator |

These are pre-implementation freezes — they are the spec, not derived from
running code.

## 3. Cryptographic decisions

### 3.1 Signature

- Algorithm: ML-DSA-65 (FIPS 204), pure mode, with a fixed `ctx` value
  (see §2 and [ADR-0002](adr/0002-mldsa65-p2pkh.md)).
- Library: `libcrux-ml-dsa = "=0.0.9"` (exact pin).
- Verify-time pre-checks: signature length and public-key length must be
  validated **before** calling into `verify`.
- See [ADR-0002](adr/0002-mldsa65-p2pkh.md).

### 3.2 UTXO accumulator

- Algorithm: LtHash32_1024 (Meta).
- State: 1024 lanes × 32 bits = 4096 bytes.
- Element serialization includes the spending outpoint to defeat the
  2^16 duplication wrap-around.
- See [ADR-0003](adr/0003-lthash-utxo-accumulator.md).

### 3.3 UTXO commitment

- Production: BLAKE2b-512 (or BLAKE3 XOF) of LtHash state → 64-byte
  `UtxoCommitment64` type.
- PoC: 32-byte commitment is permitted but must not be claimed as
  the 200-bit-security finalization of LtHash.
- See [ADR-0004](adr/0004-utxo-commitment64.md).

### 3.4 Hashes that stay 32 bytes

- `txid`, block hash, merkle root, accepted-id merkle root, RPC `Hash`:
  unchanged.
- Only the UTXO commitment is widened, via a dedicated `UtxoCommitment64`
  type. The general-purpose `kaspa_hashes::Hash` remains 32 bytes.

## 4. Network identity

- New `NetworkId`, genesis block, address prefix (`kaspapq`), P2P port,
  RPC port, DNS seed list, protocol handshake magic, initial UTXO commitment.
- See [ADR-0001](adr/0001-network-isolation.md).

## 5. Standard transaction format

### 5.1 Address

- New `Version::PubKeyHashMlDsa87 = 2`.
- Payload: `BLAKE2b-256(public_key)` = 32 bytes.
- The existing `PAYLOAD_VECTOR_SIZE = 36` SmallVec accommodates 32-byte
  payloads without resizing.
- The `payload` field of an address is **never** a raw ML-DSA-65 public key.

### 5.2 scriptPubKey (output)

```
OP_DUP
OP_BLAKE2B_512
OP_DATA64 <keyed BLAKE2b-512(public_key) — 64-byte address payload>
OP_EQUALVERIFY
OP_CHECKSIG_MLDSA87
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

`calc_mldsa87_signature_hash` is added as a new function alongside
the existing `calc_schnorr_signature_hash` / `calc_ecdsa_signature_hash`.
The ML-DSA `ctx` parameter binds the signature to the network and scheme
(see §2).

## 6. Mass / DoS policy (initial)

These values are placeholders for PoC. They will be replaced by
benchmarked values in Phase 6 — see [ADR-0005](adr/0005-mass-policy.md).

| Parameter | PoC value | Notes |
|---|---|---|
| `mass_per_tx_byte` | `1` | unchanged |
| `mass_per_script_pub_key_byte` | `10` | unchanged |
| `mass_per_sig_op` | TBD (Phase 6) | scale from upstream `1000` by measured ML-DSA verify cost × safety factor ≥ 1.5 |
| `max_block_mass` | `500_000` | unchanged, may be tightened in Phase 6 |
| `MAX_SCRIPTS_SIZE` / `max_signature_script_len` | `16_384` | fits one ML-DSA-87 P2PKH input (md2; widened from the 10_000 draft) |
| `max_script_public_key_len` | `10_000` | unchanged |
| `MAX_SCRIPT_ELEMENT_SIZE` | `8192` | widened to accommodate the 4628-byte signature item (md2) |

## 7. SigCache shape

ML-DSA-87 public keys and signatures are far too large to keep verbatim
in a hot signature-verification cache. The cache key shape is:

```
struct Mldsa87SigCacheKey {
    sig_alg: SigAlg,           // tag = ML-DSA-87
    pubkey_hash: [u8; 32],     // BLAKE2b-256 of public key bytes
    signature_hash: [u8; 32],  // BLAKE2b-256 of signature bytes
    message_hash: [u8; 32],    // sighash digest
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
    XOF(
        "kaspa-pq-wallet-v1/mldsa87/keygen" ||
        network_id ||
        account ||
        change ||
        index ||
        master_seed
    )[0..32]
keypair = MLDSA87.KeyGen(keygen_seed)
```

The PoC PRF is BLAKE3 XOF; the spec admits BLAKE2b-512 as an
alternative. Once chosen, the choice is normative.

## 9. Compatibility and migration

There is **no** migration path between mainline Kaspa and kaspa-pq.
This is by design: the address format, accumulator, and signature scheme
are all different. A separate one-shot migration tool is out of scope
for the PoC.

## 10. Phase plan (revised: 13-phase ordering)

ADR-0007 (Layered PoW), ADR-0008 (Hash64 consensus identity),
ADR-0009 (DNS overlay), ADR-0010 (Validator node architecture),
ADR-0011 (Validator single-host deployment + equivocation-safety)
and ADR-0012 (Mainnet validator sortition) expanded the original
7-phase plan to 13. The current ordering, with status as of the
last commit to this branch:

| # | Title | Status |
|---|---|---|
| 1 | Spec freeze (this document, ADRs 0001–0005) | ✅ landed |
| 2 | Network isolation (`kaspapq*` prefix, ports, genesis, DNS seeds) | ✅ landed |
| 3 | LtHash32_1024 UTXO accumulator (PoC, 32-byte commitment) | ✅ landed |
| 4 | ML-DSA-65 P2PKH script | ✅ landed |
| 5 | Wallet key derivation + minimal CLI | ✅ landed |
| 5'| `kaspa-pq-cli` standalone binary + encrypted seed + wRPC info | ✅ landed |
| 6 | Mass policy benchmark + reinforcement (`mass_per_sig_op = 6000`) | ✅ landed |
| 7 | RPC / WASM / SDK (PR-7.1 – PR-7.6, incl. UtxoCommitment64) | ✅ landed |
| 8 | Layered PoW foundation (Layer 0; PR-8.1 – PR-8.3, PR-8.5, PR-8.6) | ✅ landed |
| 9 | Hash64 consensus identity (PR-9.1 – PR-9.4 landed; PR-9.5 cascade deferred) | 🚧 partial |
| 10 | DNS Probabilistic Finality Overlay (PR-10.1 ADR + PR-10.2 spec + PR-10.3 type stubs landed; PR-10.4 – PR-10.14 deferred) | 🚧 design-freeze only |
| 11 | Validator node architecture (PR-11.1 ADR + PR-11.2 types + PR-11.3 spec landed; implementation merged into the Phase 10 PR-10.4 – PR-10.14 slots) | ✅ design-freeze landed |
| 12 | Validator single-host deployment + equivocation-safety (PR-12.1 ADR + PR-12.2 types + PR-12.3 spec landed; implementation slots PR-10.6′/10.6″/10.6‴/10.13′/10.14′ layer onto the Phase 10 entries) | ✅ design-freeze landed |
| 13 | Mainnet completeness — sortition / rewards / failover / HSM (PR-13.1–13.3 ADR-0012 sortition + PR-13.4–13.6 ADR-0013 rewards + PR-13.7–13.8 ADR-0014 coordinated failover + PR-13.9–13.10 ADR-0015 remote-signer/HSM + PR-13.11 spec close) | ✅ design-freeze landed |

Phases 11 and 12 are **operational-design** phases: neither
introduces new consensus surface beyond what ADR-0009 already
specified. Phase 11 freezes the in-process operator-facing
contract (single-binary `--enable-validator` mode). Phase 12
freezes the production-recommended sidecar deployment shape, the
validator-local equivocation guard, the 9-variant
`ValidatorStatus` enum, the key-separation policy (validator key
on the host; owner key **not** on the host), and the
slashing-scope binding (equivocation slashed, downtime **not**).
The implementation work for both phases stays inside the Phase 10
slot range (with the Phase 12 work layering onto the matching
Phase 10 entries as `'`-suffixed sub-slots).

Phase 13 is the **mainnet-completeness** phase, design-frozen
across four ADRs:
- **ADR-0012** — Mainnet validator sortition via on-chain
  commit-reveal (consensus-input; pins per-epoch validator-set
  selection);
- **ADR-0013** — Validator reward distribution (consensus-
  input; pins the per-attestation reward, the slashing
  distribution, and the bond ROI economics);
- **ADR-0014** — Coordinated-failover protocol (operational;
  resolves the ADR-0011 "one key, one host" future-ADR pointer);
- **ADR-0015** — Remote-signer / HSM protocol (operational;
  resolves the ADR-0010 hot-key Negative).

Implementation slots remain deferred — they layer onto the
existing Phase 10 PR-10.4 – PR-10.14 entries with `'` suffixes
(plus four `′′…` sub-slots for the Phase 13 operational
additions). The on-chain consensus rules (sortition + rewards)
ship in PR-10.9 and PR-10.5′/PR-10.12′; the operational
binaries (sidecar, signer, failover CLI) ship in PR-10.6′
through PR-10.6′′′′a; the simnet acceptance run that exercises
the full Phase 13 surface end-to-end is PR-10.14′ /
PR-10.14′′.

Refined Phase 10 PR plan (per ADR-0010 §"Phase 10 PR plan" with
Phase 12 sub-slot refinements from ADR-0011 §"Phase 12 PR plan"):

| PR | Title | Status |
|---|---|---|
| 10.1 | ADR-0009 (DNS overlay) | ✅ landed |
| 10.2 | Spec update (DNS scope + Phase 10) | ✅ landed |
| 10.3 | `consensus/core/src/dns_finality.rs` type stubs | ✅ landed |
| 11.1 | ADR-0010 (validator node architecture) | ✅ landed |
| 11.2 | `dns_finality.rs` Hash64 IDs + registry / snapshot / state types + helpers | ✅ landed |
| 11.3 | Spec update (ADR-0010 + Phase 11 row + this 14-slot table) | ✅ landed |
| 12.1 | ADR-0011 (validator single-host deployment + equivocation-safety) | ✅ landed |
| 12.2 | `dns_finality.rs` `ValidatorStatus` + `SignedEpochRecord` + `check_signed_epoch_record` helper + tests | ✅ landed |
| 12.3 | Spec update (ADR-0011 + Phase 12 row + Phase 12 acceptance criteria + v0.5) | ✅ landed |
| 13.1 | ADR-0012 (mainnet validator sortition via on-chain commit-reveal) | ✅ landed |
| 13.2 | `dns_finality.rs` `SortitionMode` + commit/reveal/unreveal payloads + `DnsParams` sortition fields + helpers (`compute_commit`, `derive_epoch_seed_*`, `compute_validator_priority`, `select_committee`) | ✅ landed |
| 13.3 | Spec update (ADR-0012 + Phase 13 row 1/4 + Phase 13 acceptance criteria 1/4 + v0.6) | ✅ landed |
| 13.4 | ADR-0013 (validator reward distribution) | ✅ landed |
| 13.5 | `dns_finality.rs` `RewardParams` + `compute_attestation_reward_payouts` + `compute_slashing_distribution` + `apply_unreveal_reporter_min_cap` helpers + tests | ✅ landed |
| 13.6 | Spec update (ADR-0013 + Phase 13 row 2/4 + Phase 13 acceptance criteria 2/4 + v0.7) | ✅ landed |
| 13.7 | ADR-0014 (coordinated-failover protocol) | ✅ landed |
| 13.8 | `dns_finality.rs` `HostId` alias + `TakeoverToken` + `takeover_token_message` helper + `ValidatorStatus::AwaitingTakeoverToken` extension + tests | ✅ landed |
| 13.9 | ADR-0015 (remote-signer / HSM protocol) | ✅ landed |
| 13.10 | `dns_finality.rs` `SIGNER_PROTOCOL_VERSION` + capability bitflags + `SigningPurpose` / `SignerPolicy` / `SignerError` / `SignerMetadata` enums + `SignerHello{,Ack}` / `SignerRequest` / `SignerResponse` / `SignerAuditRecord` / `SignerOutcome` + `compute_signer_audit_chain_entry` helper + tests | ✅ landed |
| 13.11 | Spec update (ADR-0014 + ADR-0015 + Phase 13 row 4/4 + Phase 13 acceptance criteria 3/4 + 4/4 + v0.8 — closes Phase 13 design-freeze) | ✅ landed (this PR) |
| 10.4 | Stake transaction kinds (`subnetwork_id` route) + tx validation | ⏳ deferred |
| 10.5 | `stake_registry` / `stake_score` consensus processes + stores | ⏳ deferred |
| 10.5′ | Coinbase fan-out for validator attestation rewards in `consensus/src/processes/coinbase.rs`; consumes `RewardParams::per_attestation_reward_sompi` (ADR-0013) | ⏳ deferred |
| 10.6 | `validator_service` in-process loop + `--enable-validator` flag | ⏳ deferred |
| 10.6′ | `kaspa-pq-validator` sidecar binary + 127.0.0.1 wRPC client (ADR-0011) | ⏳ deferred |
| 10.6″ | `signed_epoch` store + `check_signed_epoch_record` integration (ADR-0011) | ⏳ deferred |
| 10.6‴ | `--dry-run` flag wiring + per-epoch eligibility log emitter (ADR-0011) | ⏳ deferred |
| 10.7 | PoC hard-checkpoint reorg gate (`--dns-mode hard-checkpoint`) | ⏳ deferred |
| 10.8 | Mainnet two-dimensional dominance rule + property tests | ⏳ deferred |
| 10.9 | Validator sortition consensus rules (deterministic + commit-reveal per ADR-0012; `consensus/src/processes/validator_sortition.rs`; subnetwork-id tx routing for commit / reveal / unreveal-evidence payloads; on-chain commit-reveal slashing pipeline) | ⏳ deferred |
| 10.10 | P2P `StakeAttestation` gossip + flow integration | ⏳ deferred |
| 10.11 | Miner block-template policy reservation for shards | ⏳ deferred |
| 10.12 | `slashing.rs` evidence pipeline + bond burn + reporter reward | ⏳ deferred |
| 10.12′ | Slashing distribution in `consensus/src/processes/slashing.rs` using `compute_slashing_distribution` for both equivocation and unreveal cases; `apply_unreveal_reporter_min_cap` on the unreveal path (ADR-0013) | ⏳ deferred |
| 10.13 | `wallet/staking.rs` + `kaspa-pq-cli` stake/validator commands | ⏳ deferred |
| 10.13′ | `kaspa-pq-cli validator keygen --out` + `kaspa-pq-cli validator status` (9-variant enum) (ADR-0011) | ⏳ deferred |
| 10.14 | `DnsConfirmation` RPC type + wRPC/WASM bindings + 8-step runbook smoke test on simnet | ⏳ deferred |
| 10.14′ | `getValidatorStatus` RPC + sidecar-mode smoke test on simnet (ADR-0011) | ⏳ deferred |
| 10.6‴′ | `kaspa-pq-cli validator host-id init` + `handoff` + `accept-takeover` + `emergency-takeover --acknowledge-slashing-risk` CLI commands; local `takeover-tokens/` DB layout (ADR-0014) | ⏳ deferred |
| 10.6′′′′ | `kaspa-pq-signer` binary, default `SoftwareKey` backend, `--policy {permissive,auditonly,strict}` flag (ADR-0015) | ⏳ deferred |
| 10.6′′′′a | `--signer-socket <path>` flag on `kaspa-pq-validator`; SIGNER_PROTOCOL_VERSION handshake; sign-request fan-out (ADR-0015) | ⏳ deferred |
| 10.12′′ | Strict-mode signer-side equivocation guard via `check_signed_epoch_record` integration in `kaspa-pq-signer` (ADR-0015) | ⏳ deferred |
| 10.12′′a | `Pkcs11Adapter` build feature for the signer (per-vendor configuration documented per-deployment) (ADR-0015) | ⏳ deferred |
| 10.14′′ | TakeoverToken-driven two-host failover smoke test on simnet (planned + emergency paths) (ADR-0014) | ⏳ deferred |

All deferred slots are gated on the Phase 1–9 baseline being live
and stable; the overlay does **not** engage at network launch (see
ADR-0009 §"Three-stage rollout"). Each slot is small enough to be a
self-contained PR. The `'`-suffixed Phase 12 sub-slots layer onto
the matching Phase 10 entries; they are implementation refinements
named so reviewers can see at a glance which ADR-0011 surface a
given PR is wiring up.

Other deferred work (outside the Phase 10 slot range):

- **PR-8.4** `Header.pow_algo_id` field + genesis recompute — folded
  into PR-9.5 (the `Header` struct changes anyway as part of the
  Hash64 cascade).
- **PR-9.5** Consensus identity cascade — `Header`, `Transaction`,
  `TransactionOutpoint`, merkle, GHOSTDAG, pruning, RPC, P2P,
  database, wallet, SDK call sites migrate from `Hash` to the typed
  `Hash64` aliases. Recompute genesis hashes (the new field layout
  invalidates the current values). Multi-PR / multi-session.

## 11. Test plan summary

Full test plan lives in §7 of the project plan; this spec carries the
mandatory acceptance criteria for each phase:

- **Phase 2** simnet launches with kaspa-pq genesis; existing Kaspa
  mainnet/testnet nodes are rejected at handshake.
- **Phase 3** add-then-remove on LtHash returns the empty-state
  commitment; serialized state is exactly 4096 bytes; invalid-block
  rollback leaves the accumulator consistent with a slow recompute.
- **Phase 4** a well-formed ML-DSA-65 P2PKH spend is accepted; any
  length/context/hash mismatch is rejected before `verify` is called.
- **Phase 5** wallet round-trip on simnet: create, receive coinbase,
  spend.
- **Phase 6** `mass_per_sig_op` set from measured median verify cost
  × safety factor ≥ 1.5; mempool survives a malformed-signature flood.
- **Phase 8** Layer 0 finalizer is deterministic, all input fields
  influence the digest, the length-prefixed `l1_tag` defeats the
  canonical-concat collision attack, and the difficulty-lift identity
  holds at the consensus-core boundary.
- **Phase 9** every 64-byte hash round-trips through hex (128 chars)
  and Borsh (64 raw bytes); each of the 9 keyed BLAKE2b-512 hashers
  produces a digest of the right width and is pairwise-separating from
  the others on the same input; the algo_id = 1 kHeavyHash seed
  derivation is deterministic, per-byte sensitive, and key-separated
  from every other BLAKE2b-256 hasher in the crate.
- **Phase 10** `StakeAttestationShardPayload` mass per block stays
  within the per-block reservation; a candidate fork that exits the
  latest DNS-confirmed anchor is rejected unless it beats the
  canonical chain on both `WorkScore` and `StakeScore` (mainnet); a
  validator that signs two incompatible attestations at the same
  `(bond_outpoint, validator_id, epoch)` is slashable for the full
  evidence window; new nodes can recover a deterministic
  `StakeScore` for any block from the on-chain shards alone.
- **Phase 11** (operational acceptance) the 8-step operator runbook
  from ADR-0010 §"Operator runbook" runs end-to-end on simnet:
  `kaspa-pq-node` boots as a full node, `kaspa-pq-cli validator
  keygen` produces an ML-DSA-65 key, `kaspa-pq-cli stake bond`
  commits a bond, the bond transitions from `Pending` to `Active`
  at `activation_daa_score`, restarting with
  `--enable-validator --validator-key … --stake-bond …` starts the
  in-process validator service, and
  `kaspa-pq-cli get-dns-confirmation <block_hash>` returns a
  populated `DnsConfirmation` (`pow_confirmed`, `dns_confirmed`,
  `work_depth`, `stake_depth`, the three risk-bound strings).
  `validator_set_commitment` is byte-identical across two
  independently-recomputing nodes on the same block (this property
  is unit-tested in `consensus/core/src/dns_finality.rs` today; the
  Phase 11 acceptance is the end-to-end CI run, which is the gate
  on PR-10.14 landing).
- **Phase 12** (operational acceptance, sidecar variant) the
  ADR-0010 runbook runs end-to-end **also** in the sidecar shape:
  `systemctl start kaspa-pq-node` followed by
  `systemctl start kaspa-pq-validator` (the validator service
  starts **before** the bond is active and waits in `BondPending`),
  the bond transitions through the `ValidatorStatus` state machine
  (`NodeNotSynced` → `BondPending` → `ActiveIdle` → `ActiveEligible`
  → `SignedThisEpoch`), and `getValidatorStatus` returns the
  matching 9-variant enum at each transition.
  `check_signed_epoch_record` correctly handles all three outcomes
  (Allow, AllowRebroadcast, Block) across a simulated
  node-restart-during-gossip — the existing six-test decision
  matrix in `consensus/core/src/dns_finality.rs` covers the unit
  level; the Phase 12 acceptance is the end-to-end CI run on
  simnet (gate on PR-10.14′ landing). The `--dry-run` mode signs
  *zero* attestations on a 100-epoch simnet sweep while still
  emitting per-epoch eligibility logs.
- **Phase 13 (1/4 — sortition, ADR-0012)** sortition is
  byte-deterministic across two independently-running nodes for
  the same `(epoch_seed_E, active validator set)` input;
  `compute_commit` re-derivation at tx-validation time accepts
  every honest reveal and rejects every malformed one; the
  exact ≥ 2/3 reveal-threshold boundary fires the primary seed
  derivation (off-by-one drift trips this); a 1024-seed sweep
  with a 10×-stake validator wins the single-slot committee
  > 50% of the time (the unit-tested statistical surface is
  already in `consensus/core/src/dns_finality.rs`); a validator
  that committed but did not reveal within the reveal window
  is slashed by `commit_without_reveal_slash_sompi` upon
  submission of a valid `UnrevealSlashingEvidencePayload`; the
  fallback-rule chain bottoms out at the all-zero `Hash64` for
  `target_epoch == 0`.
- **Phase 13 (2/4 — rewards, ADR-0013)** a block with `N`
  included attestations emits `N + 1` coinbase outputs (1 miner,
  `N` validator), each validator output landing at the
  bond owner address (ADR-0011 cold key, not the validator
  signing key); `compute_attestation_reward_payouts` saturates
  to the cap (not the raw multiplication) when
  `per_attestation_reward × count` exceeds
  `max_validator_inflation_per_block_sompi`, with the refund
  surfaced and accounted; `compute_slashing_distribution`
  satisfies the strict `reporter + burned == slashed`
  invariant across a 30-case matrix (5 slashed amounts × 6
  bps values, including the 0% / 100% boundaries and the
  `u64::MAX × 10000` no-overflow case); the mainnet 1000-bps
  recommendation maps to 10% reporter / 90% burned;
  `apply_unreveal_reporter_min_cap` clamps the reporter-side
  share to the
  `DnsParams::unreveal_reporter_reward_sompi` floor for the
  unreveal-slash case, with the surplus diverted to the burn
  sink (invariant survives the clamp).
- **Phase 13 (3/4 — coordinated failover, ADR-0014)** a
  TakeoverToken signed by the validator key successfully
  transfers signing authority between two same-host validator
  hosts at the planned `valid_from_epoch` without any chain-
  level equivocation; an attempted handoff where the yielding
  host has already signed `valid_from_epoch` is rejected at
  the local `yielded-at` sentinel before token emission; the
  receiving host refuses a replayed token (different
  `taking_over_host_id`) at handshake step 3.b; the
  takeover-token signing context
  (`b"kaspa-pq-v1/takeover/mldsa65"`) is pairwise distinct
  from the transaction and attestation contexts (unit-tested
  in `consensus/core/src/dns_finality.rs`); the
  `compute_host_id` helper is anti-spoofing — a rebuilt
  host with a fresh `host_boot_nonce` gets a new `host_id`;
  the `--acknowledge-slashing-risk` emergency-takeover path
  exists and is required for any handoff without a token (the
  flag is a barrier-of-entry, not a consensus override). The
  PR-10.14′′ simnet smoke test exercises planned + emergency
  paths end-to-end.
- **Phase 13 (4/4 — remote-signer / HSM, ADR-0015)** the
  `SIGNER_PROTOCOL_VERSION` handshake correctly rejects
  mismatched versions with a single
  `SignerError::ProtocolVersionMismatch` frame; the six
  capability bitflags compose under bitwise OR without
  overlap (unit-tested); `SignerRequest` /
  `SignerResponse` round-trip through Borsh in both
  `Result::Ok` and `Result::Err` branches; under
  `SignerPolicy::Strict`, the signer refuses a second
  Attestation request for the same `(validator_id, epoch)`
  with a differing `target_hash | target_daa_score` (the
  `check_signed_epoch_record` decision matrix from ADR-0011
  applies at the signer); the BLAKE2b-512-chained
  `SignerAuditRecord` log detects post-hoc record insertion
  (a record inserted between r1 and r2 shifts the chain hash
  at r2 — cryptographic tamper-detection property unit-
  tested); multiple validator clients pointing at one signer
  under `Strict` cannot collectively double-sign (no test
  vector pinned yet — runtime acceptance via PR-10.6′′′′a
  fan-out test).

## 12. ADR index

- [ADR-0001 — Network isolation](adr/0001-network-isolation.md)
- [ADR-0002 — ML-DSA-65 P2PKH as the only standard script](adr/0002-mldsa65-p2pkh.md)
- [ADR-0003 — LtHash32_1024 UTXO accumulator](adr/0003-lthash-utxo-accumulator.md)
- [ADR-0004 — 64-byte UTXO commitment](adr/0004-utxo-commitment64.md)
- [ADR-0005 — Mass / DoS policy](adr/0005-mass-policy.md)
- [ADR-0006 — RPC / WASM / SDK types](adr/0006-rpc-wasm-sdk-types.md) (Phase 7 scope freeze)
- [ADR-0007 — Layered PoW](adr/0007-layered-pow.md) (Layer 0 BLAKE2b-512 finalizer + Layer 1 algo_id; Phase 1 = quantum-resistant PoW domain, Phase 2+ = ASIC-hard Layer 1)
- [ADR-0008 — Full Hash64 consensus identity](adr/0008-hash64-consensus-identity.md) (Phase 9 — block hash / txid / merkle root / pruning point / parent references / UTXO commitment / address payload all move to 64 bytes via keyed BLAKE2b-512; 256-bit quantum preimage margin, **not** 256-bit quantum collision)
- [ADR-0009 — DNS Probabilistic Finality Overlay](adr/0009-dns-probabilistic-finality.md) (Phase 10 — PoW/GHOSTDAG keeps block production; PoS adds two-dimensional `WorkScore × StakeScore` reorg gate over selected-chain anchors; partial certificate / shard scheme to bound block mass; three-stage rollout; long-range bound U ≥ R + E)
- [ADR-0010 — Validator Node Architecture](adr/0010-validator-node-architecture.md) (Phase 11 — operational supplement to ADR-0009: one binary with three roles via `--enable-mining` / `--enable-validator` flags; subsystem file layout for the Phase 10 implementation PRs; in-process async validator service; on-chain vs P2P gossip split; miner block-template policy reserves mass for attestation shards; 8-step operator runbook; `validator_set_commitment` derivation = BLAKE2b-512 keyed `b"kaspa-pq-validator-set-v1"`)
- [ADR-0011 — Validator Single-Host Deployment + Equivocation-Safety](adr/0011-validator-deployment-and-equivocation-safety.md) (Phase 12 — operational supplement to ADR-0010: sidecar shape `kaspa-pq-node` + `kaspa-pq-validator` connected via 127.0.0.1 wRPC as the production-recommended deployment; 9-variant `ValidatorStatus` enum; `SignedEpochRecord` + `check_signed_epoch_record` honest-operator equivocation guard with Allow / AllowRebroadcast / Block outcomes; key-separation policy — validator key on the host, owner key **not** on the host; slashing-scope binding — equivocation slashed, downtime **not**; `--dry-run` validator mode; auto-wait-for-bond-activation startup; reference systemd units; hardware sizing; "one key, one host" invariant)
- [ADR-0012 — Mainnet Validator Sortition via On-Chain Commit-Reveal](adr/0012-mainnet-validator-sortition-commit-reveal.md) (Phase 13 — closes the ADR-0009 §"Sortition" mainnet TBD pointer: two sortition modes (`Deterministic` for simnet/devnet/testnet-initial; `CommitReveal` for mainnet from genesis); three-phase pipeline (commit at `E−2`, reveal at `E−1`, sortition at `E`); BLAKE2b-512 keyed commitment binding `r || target_epoch || validator_id`; Byzantine-fault-tolerant ≥ 2/3 fallback rule preserving liveness under reveal sabotage; stake-weighted top-K committee selection via `priority_v = first_u128(BLAKE2b-512(SORTITION_PRIORITY_KEY, seed || vid)) / stake`; on-chain slashing for commit-without-reveal via `UnrevealSlashingEvidencePayload`; five new `-v1` domain keys all pairwise distinct; **NOT** an unbiased random oracle — residual bias is O(K · 2⁻¹²⁸) per epoch from selective reveal withholding)
- [ADR-0013 — Validator Reward Distribution](adr/0013-validator-reward-distribution.md) (Phase 13 — closes the ADR-0009 "reporter reward + burn" loose end and the ADR-0012 equivocation-side unspecified split: per-attestation FLAT reward from a new inflation track separate from the miner subsidy; tx fees stay 100% with PoW miners; rewards land at the bond OWNER address per ADR-0011 cold-key separation; coinbase fan-out emits `N + 1` outputs for a block with `N` included attestations; defensive `max_validator_inflation_per_block_sompi` cap with refund accounting; slashing distribution `reporter = S × bps / 10000`, `burned = S − reporter` (mainnet 1000 bps = 10% to reporter); `apply_unreveal_reporter_min_cap` clamps the unreveal-slash reporter to the `unreveal_reporter_reward_sompi` floor with surplus burned; uniform expected APY per staked sompi regardless of validator size; **NOT** earning from tx fees, **NOT** fixed-forever reward rate, **NOT** guaranteed rewards)
- [ADR-0014 — Coordinated-Failover Protocol for Validator Hosts](adr/0014-coordinated-failover-protocol.md) (Phase 13 — resolves the ADR-0011 "one key, one host" future-ADR pointer: two-host hot/standby topology with a node-local, signature-bound `TakeoverToken` (ML-DSA-65 by the validator key) transferring signing authority at a specific future `valid_from_epoch`; honest-operator oriented (ADR-0009 SlashingEvidencePayload remains the malicious-operator safety net); `HostId = BLAKE2b-256(HOST_ID_KEY, hostname || host_boot_nonce)`; signing context `b"kaspa-pq-v1/takeover/mldsa65"` distinct from tx and attestation contexts; slashing-acknowledged emergency-takeover path for crashed-primary cases; new `ValidatorStatus::AwaitingTakeoverToken = 9` appended; **NOT** active/active, **NOT** crashed-primary-safe without acknowledgment, **NOT** malicious-secondary-safe)
- [ADR-0015 — Remote-Signer / HSM Protocol for Validator Signing](adr/0015-remote-signer-hsm-protocol.md) (Phase 13 — resolves the ADR-0010 hot-key Negative: length-prefixed Borsh wire protocol over a Unix domain socket between a validator client and a separate `kaspa-pq-signer` process; covers ALL ML-DSA-65 use sites uniformly (Transaction / Attestation / TakeoverToken); three policy modes (`Permissive` / `AuditOnly` / `Strict`) with `Strict` relocating the equivocation guard from the validator client to the signer; multiple validator clients → one signer under `Strict` cannot collectively double-sign; HSM backends pluggable via internal `SignerBackend` trait (default `SoftwareKey` with Argon2id + ChaCha20-Poly1305 at-rest, opt-in `Pkcs11Adapter` via build feature); BLAKE2b-512-chained `SignerAuditRecord` log with cryptographic tamper-detection via `AUDIT_LOG_CHAIN_KEY = b"kaspa-pq-signer-audit-v1"`; six capability bitflags compose under bitwise OR without overlap; **NOT** network-distributed (v1 same-host only), **NOT** zero-config HSM, **NOT** malicious-client-proof)

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
