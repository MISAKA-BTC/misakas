# ADR-0008: Full Hash64 consensus identity

Status: Accepted (Phase 9 design freeze; PR-9.1 — PR-9.3 land the foundations)
Date: 2026-05-28
Supersedes: in part, [ADR-0004 §"Scope"](0004-utxo-commitment64.md)
            (UtxoCommitment64 was carved out as the only 64-byte
            field; this ADR widens the scope to all consensus
            identity).
Extends: [ADR-0007](0007-layered-pow.md) (the Layer 0 finalizer is
         the prototypical Hash64 producer; this ADR generalises that
         choice across the identity surface).

## Context

The kaspa-pq plan up to Phase 8 kept `kaspa_hashes::Hash` at its
upstream 32-byte width for everything except the UTXO commitment.
The reasoning at the time was diff-size: widening every consensus
hash would touch ~hundreds of files across header, txid, merkle,
RPC, P2P, database, and wallet code.

That stance leaves a quantum-margin asymmetry. Even with the Layer 0
PoW finalizer producing 512 bits (ADR-0007) and the UTXO commitment
widened to 64 bytes (ADR-0004), the block hash, transaction id,
merkle root, accepted-id merkle root, pruning point, and parent
references stay 256 bits. Grover-style preimage attacks therefore
still impose only a `2^128` cost on those identities — the chain
identity does not actually carry the post-quantum margin we set out
to give it.

For a **new network** (which kaspa-pq is, per ADR-0001), the case
for paying the diff cost up-front is much stronger than for a
mainline upgrade: there is no live chain to migrate; the storage
and wire-format cost is paid once, not retrofitted; and external
tooling that integrates against kaspa-pq pays for kaspa-pq's
semantics, not upstream's.

## Decision

kaspa-pq's consensus-visible hash identity is **64 bytes / 512
bits** end-to-end. Concretely, the following move from `Hash` (32B)
to `Hash64` (64B):

| Identity | Type | Producer |
|---|---|---|
| Block hash | `BlockHash` = `Hash64` | `BlockHash64` keyed BLAKE2b-512 over header preimage |
| Pre-PoW hash | `Hash64` | `BlockPrePowHash64` keyed BLAKE2b-512 |
| Transaction id | `TransactionId` = `Hash64` | `TransactionID64` keyed BLAKE2b-512 |
| Transaction hash | `TransactionHash` = `Hash64` | `TransactionHash64` keyed BLAKE2b-512 |
| Transaction sighash | `Hash64` | `TransactionSigningHash64` keyed BLAKE2b-512 |
| Merkle branch hash | `Hash64` | `MerkleBranchHash64` keyed BLAKE2b-512 |
| `hash_merkle_root` | `MerkleRoot` = `Hash64` | derived from MerkleBranchHash64 over txid64 leaves |
| `accepted_id_merkle_root` | `Hash64` | derived from MerkleBranchHash64 over accepted-id64 leaves |
| UTXO commitment | `UtxoCommitment` = `Hash64` | `UtxoCommitmentHash64` over LtHash 2048-byte state |
| Pruning point | `BlockHash` = `Hash64` | inherits BlockHash64 |
| Parent references | `Vec<BlockHash>` = `Vec<Hash64>` | inherits BlockHash64 |
| PoW final output | `PowHash512` = `Hash64` | `PowFinalHash64` (ADR-0007 §Decision) |
| `pubkey_hash` (P2PKH ML-DSA-65) | `[u8; 64]` payload | `BLAKE2b-512(public_key)` |
| Script-hash (P2SH) payload | `[u8; 64]` payload | `BLAKE2b-512(redeem_script)` |

The following stay at 32 bytes deliberately (no quantum-margin
requirement; the use is incidental or short-lived):

- Network id hash (used only for log-line identification, not
  consensus).
- Short cache keys / debug fingerprints inside the validator
  hot path.
- `kaspa-pq Layer 1 algo_id = 1` (`kHeavyHash v1`) internal
  hashes — Layer 1 is the ASIC-resistance dial and is explicitly
  swappable; its internal hashes are not consensus identity.
- ML-DSA `SigCacheKey` digests (ADR-0002 §7 — these are
  intentionally short to keep the cache memory budget small).
- BLAKE2b-256 over the 1952-byte ML-DSA-65 public key when used
  *as a cache key* (not as the address payload).

### Type layout

```rust
// crypto/hashes/src/lib.rs
pub struct Hash([u8; 32]);                  // existing upstream type
pub type   Hash32 = Hash;                   // documentation alias
pub struct Hash64([u8; 64]);                // new

// consensus/core re-exports
pub type BlockHash             = Hash64;
pub type TransactionId         = Hash64;
pub type TransactionHash       = Hash64;
pub type MerkleRoot            = Hash64;
pub type AcceptedIdMerkleRoot  = Hash64;
pub type UtxoCommitment        = Hash64;
pub type PowHash512            = Hash64;
```

Phase 9 ships `Hash32`, `Hash64`, and the 8 keyed BLAKE2b-512
hashers in `crypto/hashes`. Downstream crates migrate their use of
`Hash` to one of the type aliases above piece by piece. This is
the same pattern Phase 7 used to migrate the 32-byte UTXO
commitment to a separate `UtxoCommitment64` newtype before flipping
the header field.

### Hash function choice

All Hash64 producers use **BLAKE2b-512 keyed with a
domain-separator string**, mirroring the upstream `crypto/hashes/src/hashers.rs`
BLAKE2b-256 family pattern (`MuHashFinalizeHash` keyed with
`b"MuHashFinalize"`, etc.). BLAKE2b is RFC 7693, supports 1–64
byte outputs, and `blake2b_simd` (already in the workspace) covers
both widths.

The Layer 1 algo_id = 1 Phase 1 path remains in the Keccak family
(`cSHAKE256("HeavyHash")`), so the kaspa-pq PoW retains the
hash-family diversity property from ADR-0007: a structural break
in BLAKE2 does not invalidate Layer 1, and vice versa.

SHA3-512 was considered as an alternative. It has the "FIPS-202
standardised" advantage; it has the "shares its family with Layer
1" disadvantage. For Phase 9 we choose BLAKE2b-512 for
implementation symmetry with the existing kaspa-pq hash stack and
to preserve Layer 0 / Layer 1 family diversity.

### algo_id = 1 (kHeavyHash) seed derivation

The upstream kHeavyHash signature takes a 32-byte seed. With
`pre_pow_hash` widened to 64 bytes, the algo_id = 1 path
**derives** a 32-byte seed from the 64-byte pre-PoW hash:

```text
l1_seed32 = BLAKE2b-256(
    key   = b"kaspa-pq-l1-kheavyhash-v1-seed",
    input = pre_pow_hash64,
)
L1_tag32 = kHeavyHash_v1(l1_seed32, timestamp, nonce)
```

The kaspa-pq Layer 0 finalizer then consumes `L1_tag32` length-
prefixed (the ADR-0007 self-delimiting layout) so a future
`algo_id = 2` whose tag is a different length cannot collide with
the algo_id = 1 encoding. The seed derivation is domain-separated
on its own keyed BLAKE2b instance so that the 32-byte seed and
the 64-byte pre-PoW hash cannot be substituted for each other in
any other context.

### Address payload width

The two payload-carrying address versions widen:

| Version | Old payload | New payload |
|---|---|---|
| `PubKeyHashMlDsa65` | 32 bytes (`BLAKE2b-256` of public key) | **64 bytes** (`BLAKE2b-512` of public key) |
| `ScriptHash` | 32 bytes (`BLAKE2b-256` of redeem script) | **64 bytes** (`BLAKE2b-512` of redeem script) |

`PAYLOAD_VECTOR_SIZE` in `crypto/addresses/src/lib.rs` grows from
the current 36 to **at least 68** (64-byte payload + 4-byte slack
for future use). The legacy 32-byte `Version::PubKey` /
`Version::PubKeyECDSA` payloads remain representable for parser
completeness but are non-standard (ADR-0002).

A new consensus opcode `OP_BLAKE2B_512` is added so the P2PKH
template can hash the public-key push to a 64-byte value before
the equality check:

```text
OP_DUP
OP_BLAKE2B_512
OP_DATA64 <BLAKE2b-512(public_key)>
OP_EQUALVERIFY
OP_CHECKSIG_MLDSA65
```

`OP_DATA64` is the natural `OpData{N}` extension covering the
length 64 (the existing macro family stops at `OpData75` upstream,
so 64 fits inside the same opcode-number band). `MAX_SCRIPT_ELEMENT_SIZE`
already covers it (4096, set in Phase 4).

### Security framing — what we claim and what we don't

Per the user-provided security framing (faithfully reproduced):

> 64-byte hash commitments give a search space of **2^256** against
> Grover-style preimage attacks on a quantum adversary. This is the
> "256-bit quantum preimage margin" we claim.

> Quantum collision resistance under the BHT bound is approximately
> `2^(512/3) ≈ 2^170`, **not** `2^256`. We **do not** claim
> "256-bit quantum collision resistance".

The wording we use in user-facing material is therefore:

- ✅ "512-bit commitment domain"
- ✅ "256-bit quantum preimage margin"
- ✅ "high-margin quantum collision resistance"
- ❌ "256-bit quantum collision"
- ❌ "256-bit post-quantum security across the board"

This precision matters: an external claim of "256-bit post-quantum
security" would be wrong, and getting it wrong publicly is more
damaging than the actual security margin we have.

## Consequences

### Positive

- The chain's identity layer carries the same quantum-preimage
  margin (256-bit) as the PoW finalize. There is no asymmetry where
  the PoW is hardened but the txids are not.
- Single hash-construction choice (keyed BLAKE2b-N) across the
  consensus surface — easier to audit and easier for downstream
  language bindings to implement consistently.
- The "two hash families on the PoW" property (BLAKE2 for Layer 0,
  Keccak for Layer 1) is preserved.

### Negative

- ~2x growth on every RocksDB row that's keyed by or contains a
  hash. Approximate scaling:
  - Block hash: 32 → 64 bytes.
  - Transaction id: 32 → 64 bytes (every UTXO row's outpoint).
  - GHOSTDAG data: many `Hash` fields × 2.
- Wire: every `TransactionInput.outpoint.transaction_id` grows by
  32 bytes. ML-DSA-65 inputs are ~5 KB; the +32-byte outpoint is
  noise.
- RPC payload: every hex-encoded hash field doubles (64 → 128
  hex characters). Block explorer UI / wallets need to handle the
  wider display.
- Diff size: this is a several-thousand-line cascade across
  consensus, RPC, P2P, database, wallet, and SDK.

### Neutral

- Address strings grow by ~50% (32-byte → 64-byte payload, bech32
  is roughly 1.6 chars per byte after the prefix). A
  `kaspapq:` mainnet address goes from ~62 characters to ~100
  characters. Users will copy-paste either way; the change is
  cosmetic.

## Implementation order (revised 9-phase plan)

Phases 1–8 already shipped in this branch (commits `02eb0b9` →
`3d7eb82`) and serve as the kaspa-pq baseline. Phase 9 builds on
that baseline.

| Phase | Title | Status |
|---|---|---|
| 1 | Spec freeze | ✅ landed |
| 2 | Network isolation | ✅ landed |
| 3 | LtHash16_1024 UTXO accumulator (PoC) | ✅ landed |
| 4 | ML-DSA-65 P2PKH script | ✅ landed |
| 5 | Wallet key derivation + minimal CLI | ✅ landed |
| 6 | Mass policy benchmark | ✅ landed |
| 7 | RPC / WASM / SDK | ✅ landed (PR-7.1 – 7.6) |
| 8 | Layered PoW foundation (Layer 0) | ✅ landed (PR-8.1 – 8.3); PR-8.4 / 8.5 / 8.6 deferred |
| **9** | **Hash64 consensus identity** | **this ADR + PR-9.1 – PR-9.5** |

Phase 9 PR sequence:

- **PR-9.1: This ADR.** Design freeze; no code.
- **PR-9.2: crypto/hashes Hash64.** New `Hash64` struct, new
  `blake2b_512_hasher!` macro, 8 domain-separated keyed
  BLAKE2b-512 hashers. `Hash32 = Hash` documentation alias.
  Tests: hex roundtrip, Borsh, serde, fixed-length validation,
  hasher determinism.
- **PR-9.3: pow_layer0 Hash64 + l1_seed32.** Switch
  `pow_finalizer_blake2b_512` to take `Hash64` for `pre_pow_hash`.
  Add the `l1_seed32_for_kheavyhash_v1` helper. Tests: seed
  derivation is deterministic and key-separated from any other
  BLAKE2b-256 use.
- **PR-9.4: Spec update.** Remove the §1.2 "we do not widen Hash
  past 32 bytes" non-goal. Add the new goal. Reorder phases.
  Document the security framing word-for-word.
- **PR-9.5 (deferred): Consensus identity cascade.** Migrate
  `Header`, `Transaction`, `TransactionOutpoint`, merkle, GHOSTDAG,
  pruning, RPC, P2P, database, wallet, SDK call sites from `Hash`
  to the typed `Hash64` aliases. Recompute genesis hashes.
  This is the multi-PR, multi-session cascade — handled separately
  from PR-9.1 – 9.4 so the foundations can land first.

### Relationship to the previously-deferred PR-8.4 / PR-8.5 / PR-8.6

- **PR-8.4 (Header.pow_algo_id)** — still applies. Folded into
  PR-9.5 since the same Header struct changes anyway.
- **PR-8.5 (BlueWorkType: Uint192 → Uint576)** — independent of
  Hash64; still applies in its own PR.
- **PR-8.6 (PoW validator wiring)** — applies. Uses the Hash64
  `pre_pow_hash` produced by `BlockPrePowHash64` (introduced in
  PR-9.2). Implementation lands as part of the Phase 9 validator
  pass.

## References

- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md) (payload
  width discussion; this ADR widens the payload).
- [ADR-0003 — LtHash16_1024](0003-lthash-utxo-accumulator.md)
  (UTXO accumulator state; UtxoCommitmentHash64 finalises it).
- [ADR-0004 — UtxoCommitment64](0004-utxo-commitment64.md)
  (precursor of the same widening, now generalised).
- [ADR-0007 — Layered PoW](0007-layered-pow.md) (Layer 0 finalize
  is the prototypical Hash64 producer).
- RFC 7693 (BLAKE2). FIPS 202 (SHA-3 — alternative considered
  and rejected for hash-family-diversity reasons).
