# ADR-0003: LtHash UTXO accumulator (LtHash16_1024 → LtHash32_1024)

Status: Accepted (Phase 1); REVISED (audit QM-1) — migrated to LtHash32_1024.
Date: 2026-05-28 (rev. 2026-06-03)
Supersedes: —

> **Revision (audit QM-1):** the accumulator was migrated from the 16-bit-lane
> `LtHash16_1024` (~100-bit quantum collision margin — the weakest quantum link
> in the system) to the 32-bit-lane `LtHash32_1024` (4096-byte state, ~170-bit
> quantum collision), the variant §"Alternatives considered" below listed as
> deferred. The serialized state doubled (2048 → 4096 B) and the genesis
> `utxo_commitment` / block hashes were recomputed (re-genesis); the element
> expansion and 64-byte BLAKE2b-512 finalize are otherwise unchanged. The
> 16-bit analysis below is retained for historical context.

## Context

Upstream rusty-kaspa uses a 3072-bit multiplicative MuHash as its UTXO
accumulator. Verified by inspection of [crypto/muhash/src/lib.rs](../../crypto/muhash/src/lib.rs):

```rust
pub const SERIALIZED_MUHASH_SIZE: usize = ELEMENT_BYTE_SIZE; // 384
pub(crate) const ELEMENT_BIT_SIZE: usize = 3072;
pub(crate) const ELEMENT_BYTE_SIZE: usize = ELEMENT_BIT_SIZE / 8;
```

This is **not** ECMH over secp256k1; it is multiplicative MuHash in a
3072-bit RSA-style group. Security rests on a discrete-log-style
assumption in that group, which falls to a sufficiently large quantum
computer running Shor's algorithm.

Replacing it with an additive lattice-style accumulator (LtHash) buys
us:

- A symmetric-key-only security argument (LtHash reduces to SIS
  / lattice problems).
- Cheap component-wise add/remove (the same operation, just with the
  inverse element).
- A simple serialization (a fixed-size byte string of lanes).

The Meta LtHash design uses 1024 lanes of 16 bits each, i.e. a
2048-byte state. The accumulator is a commutative group under
component-wise addition modulo `2^16`. Public references claim ≥200-bit
classical and ≥100-bit quantum security for the 2048-byte instantiation.

A known footgun: with 16-bit lanes, adding the same element 2^16 times
returns to the empty state (each lane wraps). Therefore each element
must be uniquely tagged.

## Decision

kaspa-pq replaces the MuHash UTXO accumulator with **LtHash16_1024**:

- `LTHASH_LANES = 1024`, `LTHASH_LANE_BYTES = 2`,
  `LTHASH_STATE_BYTES = 2048`.
- The accumulator type is `LtHashUtxoAccumulator`, internally
  `lanes: [u16; 1024]`.
- The set operation is component-wise addition mod `2^16`. Remove is
  component-wise subtraction mod `2^16`.
- The element-to-lane-vector map is BLAKE3 (XOF) over the element
  serialization, expanded to `LTHASH_STATE_BYTES`. BLAKE3 is chosen
  for its native XOF interface; BLAKE2b-512 is an acceptable
  fallback and the final choice is locked in at Phase 3.

### Element serialization

```
element_bytes =
    "kaspa-pq-utxo-v1"           ||  // 16-byte literal domain tag
    txid                          ||  // 32 bytes
    output_index                  ||  // 4 bytes, little-endian
    amount                        ||  // 8 bytes, little-endian
    script_public_key_version     ||  // 2 bytes, little-endian
    script_public_key_len         ||  // 4 bytes, little-endian
    script_public_key             ||  // variable
    daa_score                     ||  // 8 bytes, little-endian
    is_coinbase                       // 1 byte (0x00 or 0x01)
```

Every UTXO element therefore embeds its `(txid, output_index)`, which
is unique by definition of a confirmed UTXO set. This rules out the
2^16-duplicate collision.

### Migration path inside crypto/muhash

To keep the diff bounded in Phase 3, we keep the upstream API surface
during the PoC and replace the **implementation**:

- `new()`
- `add_element_byte_hash(...)` / equivalent
- `remove_element_byte_hash(...)`
- `combine(other)`
- `serialize()`
- `deserialize(bytes)`
- `finalize()` → 32-byte digest in PoC, 64-byte `UtxoCommitment64`
  in production (see [ADR-0004](0004-utxo-commitment64.md))

When the API has settled, rename the crate from `kaspa-muhash` to
`kaspa-utxo-accumulator` and the type from `MuHash` to
`UtxoAccumulator`. Old type names remain as deprecated aliases for
exactly one release.

### Header field

The block header's UTXO commitment field changes meaning, not name,
during the PoC. It carries the LtHash final commitment (32-byte PoC
mode or 64-byte production mode). See
[ADR-0004](0004-utxo-commitment64.md) for the 64-byte type.

## Consequences

### Positive

- Quantum-resistant UTXO commitment.
- 2048-byte state is small in absolute terms and trivially
  serializable.
- Add and remove are the same primitive operation, so DAG-merge logic
  ("apply this block's diff") and rollback ("undo this block's diff")
  are symmetric.
- Order independence falls out of commutativity, so DAG ordering
  edge cases that bit upstream MuHash work also won't bite here.

### Negative

- Each block must touch 2048 bytes of state instead of 384. In
  practice this is dominated by RocksDB I/O and is not a hotpath
  concern, but it is a real bytes-on-disk increase per UTXO set
  snapshot.
- A bug in the lane-vector derivation (e.g. forgetting to bind the
  outpoint, or padding the XOF wrong) is silent — it produces a
  valid-looking 2048-byte state that disagrees with everyone else.
  Property tests are therefore non-optional.

### Neutral

- The 16-bit lane wrap is a defended attack surface, not a residual
  risk: as long as elements carry their outpoint, the attacker
  cannot construct 2^16 copies of the same element.

## Alternatives considered

1. **Keep MuHash, swap the underlying group.** Rejected: any
   discrete-log-style group falls to Shor's algorithm.
2. **Use a hash-based Merkle accumulator with per-block delta
   commitments.** Rejected for the PoC: implementation cost is
   higher, and the DAG structure of Kaspa makes per-block delta
   replay harder than the commutative LtHash design.
3. **LtHash16_512 (1024-byte state).** Rejected: the size win is
   small and the security margin is reduced.
4. **LtHash32_1024 (4096-byte state).** Deferred: a future ADR may
   bump to 32-bit lanes if the 2^16 collision class proves to be
   harder to defend than expected.

## Implementation notes for Phase 3

Files expected to change:

- `crypto/muhash/src/lib.rs` — replace `MuHash` implementation with
  `LtHashUtxoAccumulator`. Keep `MuHash` type alias for the PoC.
- `crypto/muhash/src/u3072.rs` — delete or move to an `attic/`
  module; the multiplicative group is not used.
- `crypto/muhash/src/element_hash.rs` — replace with a BLAKE3-XOF
  expansion to 2048 bytes.
- All `add_element` / `remove_element` callers — should not need to
  change because the public surface is preserved.
- Genesis: recompute the initial UTXO commitment as the empty-state
  serialization of `LtHashUtxoAccumulator`. Document the expected
  empty-state hex string in a test fixture.

## Acceptance criteria (Phase 3)

1. `add(e)` then `remove(e)` returns the empty-state commitment.
2. The accumulator is order-independent: for any two permutations
   of the same set, the commitments are equal.
3. `combine(self, other) == add_all(self, other.elements)` whenever
   the two sets are disjoint.
4. Serialized state is exactly `LTHASH_STATE_BYTES = 2048`.
5. Deserialization of 2048-byte state and then re-serialization is
   the identity.
6. The 2^16 duplication attack is **not** possible against unique
   UTXOs (property test: random UTXO sets never collide under the
   set-of-`(txid, index)` partial order).
7. An invalid-block rollback (apply diff D, detect failure, apply
   −D) restores the previous commitment exactly.

## References

- Meta LtHash design paper and OSS implementation.
- Upstream [crypto/muhash/src/lib.rs](../../crypto/muhash/src/lib.rs).
- [ADR-0004 — 64-byte UTXO commitment](0004-utxo-commitment64.md).
