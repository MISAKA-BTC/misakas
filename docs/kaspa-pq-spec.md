# kaspa-pq Specification (v0.3, draft)

Status: Draft. Frozen values listed here are the contract every phase must
respect. Any change must go through an ADR update under `docs/adr/`.

ADR-0007 (Layered PoW) and ADR-0008 (Hash64 consensus identity) widen
the original "signatures + UTXO accumulator" target to "full 64-byte
consensus identity + 512-bit PoW domain". ADR-0009 (DNS Probabilistic
Finality Overlay) adds a Phase 10 post-launch confirmation layer that
binds deep-reorg safety to both `WorkScore` and `StakeScore`. Earlier
Phase 1 non-goals that contradicted these ADRs have been removed; see
the revision history below.

## 0. Scope and non-goals

This document specifies a quantum-resistant Kaspa-based network ("kaspa-pq")
forked from rusty-kaspa. It is **not** a compatibility layer with the
mainline Kaspa network.

### In scope

1. Signature scheme replacement: ML-DSA-65 (FIPS 204) P2PKH only. Address
   payload is the 64-byte BLAKE2b-512 hash of the public key
   (ADR-0008 §"Address payload width").
2. UTXO accumulator replacement: LtHash16_1024. Final commitment is the
   64-byte BLAKE2b-512 of the 2048-byte LtHash state
   (`UtxoCommitmentHash64`, ADR-0008).
3. Network-level isolation (NetworkId, genesis, address prefix, ports).
4. **Layered PoW** (ADR-0007): Layer 0 is the consensus-critical
   BLAKE2b-512 finalizer over a 512-bit comparison domain; Layer 1 is the
   `algo_id`-identified ASIC-resistance tag (`algo_id = 1` =
   kHeavyHash-compatible at Phase 1; ASIC-hard variants are Phase
   2+ separate hard-fork ADRs). `BlueWorkType = Uint576` in Phase 1.
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
| `MLDSA65_PK_LEN`  | `1952`  | ML-DSA-65 public key length (bytes) |
| `MLDSA65_SIG_LEN` | `3309`  | ML-DSA-65 signature length (bytes) |
| `MLDSA65_SIG_ITEM_MAX_LEN` | `3310` | signature item incl. 1-byte sighash type |
| `LTHASH_LANES`    | `1024`  | Number of 16-bit lanes in LtHash state |
| `LTHASH_LANE_BYTES` | `2`   | Bytes per lane |
| `LTHASH_STATE_BYTES` | `2048` | Serialized accumulator state size |
| `UTXO_COMMITMENT_BYTES` (production) | `64` | Header UTXO commitment field |
| `UTXO_COMMITMENT_BYTES` (PoC, optional) | `32` | PoC-only shortcut, must be flagged |
| `MAX_SCRIPT_ELEMENT_SIZE` (kaspa-pq) | `4096` | up from upstream `520` |
| `MAX_SCRIPTS_SIZE` (kaspa-pq) | `10_000` | initial value, unchanged from upstream |
| `MAX_STACK_SIZE` (kaspa-pq) | `244` | initial value, unchanged from upstream |
| Signature context (default) | `"kaspa-pq-v1/tx/mldsa65"` | ML-DSA `ctx` parameter |
| Wallet keygen domain | `"kaspa-pq-wallet-v1/mldsa65/keygen"` | XOF domain separator |

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

- Algorithm: LtHash16_1024 (Meta).
- State: 1024 lanes × 16 bits = 2048 bytes.
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

- New `Version::PubKeyHashMlDsa65 = 2`.
- Payload: `BLAKE2b-256(public_key)` = 32 bytes.
- The existing `PAYLOAD_VECTOR_SIZE = 36` SmallVec accommodates 32-byte
  payloads without resizing.
- The `payload` field of an address is **never** a raw ML-DSA-65 public key.

### 5.2 scriptPubKey (output)

```
OP_DUP
OP_BLAKE2B_256
OP_DATA32 <BLAKE2b-256(public_key)>
OP_EQUALVERIFY
OP_CHECKSIG_MLDSA65
```

Approximately 36–37 bytes per output.

### 5.3 signatureScript (input)

```
PUSH <signature || sighash_type>     ; 3309 + 1 = 3310 bytes payload
PUSH <ML-DSA-65 public key>          ; 1952 bytes payload
```

Approximately 5267 bytes including push opcodes. The full input
(outpoint + length + script + sequence) is approximately 5319 bytes.

### 5.4 sighash

`calc_mldsa65_signature_hash` is added as a new function alongside
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
| `max_signature_script_len` | `10_000` | unchanged, fits one ML-DSA P2PKH input |
| `max_script_public_key_len` | `10_000` | unchanged |
| `MAX_SCRIPT_ELEMENT_SIZE` | `4096` | widened from `520` to accommodate 3310-byte signature item |

## 7. SigCache shape

ML-DSA-65 public keys and signatures are far too large to keep verbatim
in a hot signature-verification cache. The cache key shape is:

```
struct Mldsa65SigCacheKey {
    sig_alg: SigAlg,           // tag = ML-DSA-65
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
        "kaspa-pq-wallet-v1/mldsa65/keygen" ||
        network_id ||
        account ||
        change ||
        index ||
        master_seed
    )[0..32]
keypair = MLDSA65.KeyGen(keygen_seed)
```

The PoC PRF is BLAKE3 XOF; the spec admits BLAKE2b-512 as an
alternative. Once chosen, the choice is normative.

## 9. Compatibility and migration

There is **no** migration path between mainline Kaspa and kaspa-pq.
This is by design: the address format, accumulator, and signature scheme
are all different. A separate one-shot migration tool is out of scope
for the PoC.

## 10. Phase plan (revised: 9-phase ordering)

ADR-0007 (Layered PoW) and ADR-0008 (Hash64 consensus identity) expanded
the original 7-phase plan. The current ordering, with status as of the
last commit to this branch:

| # | Title | Status |
|---|---|---|
| 1 | Spec freeze (this document, ADRs 0001–0005) | ✅ landed |
| 2 | Network isolation (`kaspapq*` prefix, ports, genesis, DNS seeds) | ✅ landed |
| 3 | LtHash16_1024 UTXO accumulator (PoC, 32-byte commitment) | ✅ landed |
| 4 | ML-DSA-65 P2PKH script | ✅ landed |
| 5 | Wallet key derivation + minimal CLI | ✅ landed |
| 5'| `kaspa-pq-cli` standalone binary + encrypted seed + wRPC info | ✅ landed |
| 6 | Mass policy benchmark + reinforcement (`mass_per_sig_op = 6000`) | ✅ landed |
| 7 | RPC / WASM / SDK (PR-7.1 – PR-7.6, incl. UtxoCommitment64) | ✅ landed |
| 8 | Layered PoW foundation (Layer 0; PR-8.1 – PR-8.3) | ✅ landed |
| 9 | Hash64 consensus identity (PR-9.1 – PR-9.4 landed; PR-9.5 cascade deferred) | 🚧 partial |
| 10 | DNS Probabilistic Finality Overlay (PR-10.1 ADR landed; PR-10.2 – PR-10.9 deferred) | 🚧 design-freeze only |

Deferred PRs:

- **PR-8.4** Header.pow_algo_id field + genesis recompute — folded into
  PR-9.5 (the Header struct changes anyway as part of the Hash64
  cascade).
- **PR-8.5** `BlueWorkType: Uint192 → Uint576` — independent of Hash64;
  still applies as its own cascade.
- **PR-8.6** Layer 0 finalizer wired into consensus PoW validation — uses
  the Hash64 pre_pow_hash from PR-9.3; runs as part of the Phase 9
  validator pass.
- **PR-9.5** Consensus identity cascade — `Header`, `Transaction`,
  `TransactionOutpoint`, merkle, GHOSTDAG, pruning, RPC, P2P, database,
  wallet, SDK call sites migrate from `Hash` to the typed `Hash64`
  aliases. Recompute genesis hashes (the new field layout invalidates
  the current values). Multi-PR / multi-session.
- **PR-10.2 – PR-10.9** DNS Finality Overlay implementation — type
  stubs (PR-10.3), `subnetwork_id`-based stake transaction kinds
  (PR-10.4), deterministic `StakeScore` aggregation (PR-10.5), PoC
  hard-checkpoint gate (PR-10.6), mainnet two-dimensional dominance
  rule (PR-10.7), validator sortition (PR-10.8), `DnsConfirmation`
  RPC surface (PR-10.9). All of these are gated on the Phase 1–9
  baseline being live and stable; the overlay does **not** engage at
  network launch (see ADR-0009 §"Three-stage rollout").

## 11. Test plan summary

Full test plan lives in §7 of the project plan; this spec carries the
mandatory acceptance criteria for each phase:

- **Phase 2** simnet launches with kaspa-pq genesis; existing Kaspa
  mainnet/testnet nodes are rejected at handshake.
- **Phase 3** add-then-remove on LtHash returns the empty-state
  commitment; serialized state is exactly 2048 bytes; invalid-block
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

## 11. ADR index

- [ADR-0001 — Network isolation](adr/0001-network-isolation.md)
- [ADR-0002 — ML-DSA-65 P2PKH as the only standard script](adr/0002-mldsa65-p2pkh.md)
- [ADR-0003 — LtHash16_1024 UTXO accumulator](adr/0003-lthash-utxo-accumulator.md)
- [ADR-0004 — 64-byte UTXO commitment](adr/0004-utxo-commitment64.md)
- [ADR-0005 — Mass / DoS policy](adr/0005-mass-policy.md)
- [ADR-0006 — RPC / WASM / SDK types](adr/0006-rpc-wasm-sdk-types.md) (Phase 7 scope freeze)
- [ADR-0007 — Layered PoW](adr/0007-layered-pow.md) (Layer 0 BLAKE2b-512 finalizer + Layer 1 algo_id; Phase 1 = quantum-resistant PoW domain, Phase 2+ = ASIC-hard Layer 1)
- [ADR-0008 — Full Hash64 consensus identity](adr/0008-hash64-consensus-identity.md) (Phase 9 — block hash / txid / merkle root / pruning point / parent references / UTXO commitment / address payload all move to 64 bytes via keyed BLAKE2b-512; 256-bit quantum preimage margin, **not** 256-bit quantum collision)
- [ADR-0009 — DNS Probabilistic Finality Overlay](adr/0009-dns-probabilistic-finality.md) (Phase 10 — PoW/GHOSTDAG keeps block production; PoS adds two-dimensional `WorkScore × StakeScore` reorg gate over selected-chain anchors; partial certificate / shard scheme to bound block mass; three-stage rollout; long-range bound U ≥ R + E)

## 12. Revision history

| Version | Date | Change |
|---|---|---|
| 0.1 | 2026-05-28 | Initial draft. |
| 0.2 | 2026-05-28 | ADR-0007 + ADR-0008 incorporated. Removed the "do not widen Hash past 32 bytes" non-goal (it directly contradicts ADR-0008); added the full 64-byte consensus identity goal; added the Phase 8 / Phase 9 entries to the phase plan; codified the public-claim discipline section. Revised non-goal removal: previously `PQ-strengthening the PoW hash, block hash, txid, or merkle root` was listed as out-of-scope; this is now the explicit Phase 8 + Phase 9 in-scope work. |
| 0.3 | 2026-05-28 | ADR-0009 incorporated. Added in-scope item 6 (DNS Probabilistic Finality Overlay) and Phase 10 row in the phase plan. Codified the DNS-specific public-claim discipline section (binding) — explicitly rejecting "hard finality", "reorg-probability product", and "2^k post-quantum finality" framings. Added Phase 10 acceptance criteria to §11. |
