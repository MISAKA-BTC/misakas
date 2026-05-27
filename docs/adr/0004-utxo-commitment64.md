# ADR-0004: 64-byte UTXO commitment via a dedicated `UtxoCommitment64` type

Status: Accepted (Phase 1 freeze; PR-7.6 introduced the type + finalize path)
Date: 2026-05-28
Supersedes: —

## Phase 7 PR-7.6 delivery

PR-7.6 (commit on the kaspa-pq branch) introduced the type, the finalize
path, and the RPC wire form, but **kept the header field at 32 bytes**.
The header switch is the last mechanical step and is split out so it can
land on its own once any consumer that reads the header field is ready
for the type change.

What landed in PR-7.6:

| Location | Item |
|---|---|
| [consensus/core/src/utxo_commitment.rs](../../consensus/core/src/utxo_commitment.rs) | `UtxoCommitment64` newtype, `UTXO_COMMITMENT_64_BYTES = 64`. No `From`/`Into<Hash>` conversion (type discipline). |
| [crypto/muhash/src/lib.rs](../../crypto/muhash/src/lib.rs) | `MuHash::finalize_64() -> [u8; 64]` (BLAKE2b-512 of the 2048-byte LtHash state); `EMPTY_MUHASH_64` constant. |
| [rpc/core/src/model/kaspa_pq.rs](../../rpc/core/src/model/kaspa_pq.rs) | `RpcUtxoCommitment64` newtype, Borsh + serde JSON hex encoding, `From`/`Into<UtxoCommitment64>` conversions. |
| `EMPTY_MUHASH_64` | `2938eeb12521e01d…b8eb820d6b` (LtHash empty-state BLAKE2b-512 digest, asserted by `test_empty_hash_64`). |

What is deferred to a follow-up "header-switch PR":

- `Header::utxo_commitment` field switch from `Hash` to
  `UtxoCommitment64`. This touches ~30 files (genesis, header hashing,
  RPC header conversion, p2p convert, bridge, testing/integration,
  WASM optional_header). The type, finalize path, and RPC wire form
  already exist in the branch, so the switch is mechanical.
- Genesis hash recompute (4 networks) once the header field changes.

## Context

LtHash16_1024 (see [ADR-0003](0003-lthash-utxo-accumulator.md))
materializes as a 2048-byte state. For storage inside the block
header we must finalize that state into a fixed-size digest. The two
realistic choices are:

- **32-byte finalize.** Cheap. Compatible with reusing
  `kaspa_hashes::Hash` (which is 32 bytes everywhere in the
  codebase). However, 32 bytes (256 bits) cannot honestly claim the
  ≥200-bit security level of the underlying LtHash16_1024 state once
  you account for collision security ≈ half-output.
- **64-byte finalize.** Preserves the full security margin of the
  underlying state. Requires a dedicated type because everything
  else in the header uses 32-byte hashes.

A blanket widening of `kaspa_hashes::Hash` to 64 bytes is a
non-starter for this PoC. It would touch:

- every database key in `database/`,
- every RPC message and gRPC proto in `rpc/grpc/*` and `rpc/wrpc/*`,
- every WASM binding in `wasm/`, `crypto/txscript/src/wasm/`,
- every wallet on-disk format,
- every merkle-tree node, every txid, every block hash.

That diff is the size of a separate project.

## Decision

Introduce a dedicated newtype:

```rust
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default)]
pub struct UtxoCommitment64([u8; 64]);
```

placed in `consensus/core/src/header.rs` (or a new
`consensus/core/src/utxo_commitment.rs`, decided in Phase 3/4
implementation).

- The block header's UTXO-commitment field switches to
  `UtxoCommitment64` (production target).
- For the PoC, a 32-byte commitment may be used to keep the diff
  small. If used, the spec line in `kaspa-pq-spec.md` §3.3 must
  flag this as a "PoC shortcut, not the production security claim".
- All other hash-typed fields (`txid`, block hash, merkle root,
  accepted-id merkle root, RPC `Hash`) remain `kaspa_hashes::Hash`
  (32 bytes).
- The finalize function is `BLAKE2b-512(state_bytes)` or
  `BLAKE3-XOF(state_bytes, 64)`. The final choice is locked in
  alongside [ADR-0003](0003-lthash-utxo-accumulator.md).

### Type discipline

- `UtxoCommitment64` must not have a `From<Hash>` impl.
- `Hash` must not have a `From<UtxoCommitment64>` impl.
- RPC serializers must encode `UtxoCommitment64` as a 64-byte
  hex/byte field whose JSON / proto name is distinguishable from
  any 32-byte hash field.
- The wallet, indexer, and explorer should display
  `UtxoCommitment64` as 128 hex chars; a 64-hex display would
  silently misrepresent a truncated value.

## Consequences

### Positive

- Honest security claim. The block-header commitment matches the
  underlying accumulator's claimed security level.
- Bounded blast radius. Only the header, the accumulator, the
  validator, and the RPC encoding for that one field have to learn
  about the 64-byte type.
- Optionality at PoC time. We can ship a 32-byte commitment PoC and
  flip the type later without a second consensus-level rewrite,
  because the rest of the system already knows about
  `UtxoCommitment64` from Phase 1.

### Negative

- Two hash widths in the codebase. Reviewers must be alert to
  `Hash` ↔ `UtxoCommitment64` confusion. Mitigation: no `From`
  conversions, distinct field names, distinct hex display widths.
- Header byte size grows by 32 bytes per block when we move from
  PoC (32) to production (64). At 10 BPS this is ~28 MB/year. Not
  a concern given block-data dominance.

### Neutral

- WASM and TypeScript SDKs grow a new `UtxoCommitment64` type. They
  do not break the existing `Hash` type.

## Alternatives considered

1. **Widen `kaspa_hashes::Hash` to 64 bytes globally.** Rejected:
   diff size, see §Context.
2. **Stuff 64 bytes into two `Hash` fields.** Rejected: footgun,
   downstream tooling will assume each field is independently
   meaningful.
3. **Keep 32-byte commitment in production too.** Rejected for the
   production spec, allowed only as a PoC shortcut. We would not
   be able to claim the LtHash16_1024 security level honestly.

## Implementation notes for Phase 3/4

- The type is introduced in Phase 3 alongside the accumulator,
  even if Phase 3 still emits a 32-byte commitment in the PoC,
  so that downstream code can already depend on
  `UtxoCommitment64`'s existence.
- A `to_be_truncated_hash(&self) -> Hash` helper is **not**
  provided. If a downstream component needs a 32-byte view, it must
  define and name its own truncation explicitly.

## Acceptance criteria

- `UtxoCommitment64` exists as a distinct Rust type, with no
  cross-conversion to `Hash`.
- Block header serializes the UTXO-commitment field with the
  width selected by the active build (32 PoC / 64 production).
- RPC and CLI display widths match the active build, and the
  documentation flags PoC mode whenever the 32-byte variant is
  in use.
- The empty-state finalize value is committed as a fixture and
  matches the genesis-block UTXO-commitment field.

## References

- [ADR-0003 — LtHash16_1024](0003-lthash-utxo-accumulator.md).
- BLAKE2b-512 (RFC 7693). BLAKE3 specification.
