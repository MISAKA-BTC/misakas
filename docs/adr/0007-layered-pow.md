# ADR-0007: Layered PoW (Layer 0 quantum-resistant finalizer + Layer 1 ASIC-hard tag)

Status: Accepted (Phase 1 design freeze; PR-8.1 — PR-8.3 land the foundations)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0001](0001-network-isolation.md), [ADR-0003](0003-lthash-utxo-accumulator.md)

## Context

The upstream Kaspa PoW reduces to a single Keccak-family hash
(`cSHAKE256("ProofOfWorkHash")` and `cSHAKE256("HeavyHash")` —
[crypto/hashes/src/hashers.rs](../../crypto/hashes/src/hashers.rs)),
with the resulting 32-byte digest compared against a 256-bit target
derived from the header's compact `bits` field. Block work is a
192-bit accumulator (`kaspa-math::Uint192`).

That design conflates three concerns that kaspa-pq wants to keep
separable:

1. **The PoW comparison domain.** The size of the integer the miner
   solves against. Upstream uses 256 bits, which is "safe enough"
   under current symmetric-hash assumptions but does not give a
   comfortable post-quantum margin against future Grover-style
   speedups on the underlying hash.
2. **The ASIC-resistance function.** The actual heavy computation
   the miner runs. Upstream uses Keccak; kaspa-pq Phase 1 inherits
   that exactly, and a future hard-fork is expected to swap it for
   an ASIC-hard variant (Argon2d, Argon2id, RandomX-like,
   Cuckoo-like, …).
3. **The work accounting width.** The size of the integer that
   accumulates block work into `BlueWorkType` and feeds GHOSTDAG /
   DNS WorkScore. Upstream's `Uint192` is convenient but does not
   leave headroom for the 512-bit target.

Conflating these makes either side of the swap dangerous: changing
ASIC resistance silently re-derives the work comparison domain,
and any change to the work comparison domain requires re-deriving
the entire ASIC stack. kaspa-pq splits them at the spec level so
each layer can move on its own hard-fork schedule.

## Decision

kaspa-pq PoW is a **two-layer** construction:

```
Layer 1  : L1_tag = AsicHardFn_v{algo_id}(pre_pow_hash, timestamp, bits, nonce)
                    ↓ length-prefixed
Layer 0  : pow_512 = BLAKE2b-512(
                       key   = "kaspa-pq-pow-v1",
                       input = network_id || algo_id ||
                               pre_pow_hash || timestamp || bits || nonce ||
                               len(L1_tag) || L1_tag
                     )
Accept   : Uint512::from_le_bytes(pow_512)
             <= Uint512::from_compact_target_bits_512(bits)
```

Constants:

- `POW_FINALIZER_DOMAIN = b"kaspa-pq-pow-v1"`
- `POW_FINALIZER_BYTES = 64`
- `POW_ALGO_ID_KHEAVYHASH = 1`   (kaspa-pq Phase 1 only)
- `BlueWorkType = Uint576`        (Phase 1)
- DAA internal arithmetic = `Uint640`
- `PowTargetType = Uint512`
- `PowWorkType = Uint512`

### Layer 0 — quantum-resistant finalizer (consensus-critical)

Frozen in Phase 1; never changes without an additional hard-fork
ADR. Properties:

- **Family:** BLAKE2b (HAIFA-like construction). Deliberately
  different from the Keccak family used by Layer 1, so a structural
  weakness in one family does not propagate through both halves of
  the same PoW.
- **Output width:** 64 bytes = 512 bits. Compared against a 512-bit
  target.
- **Keyed:** `b"kaspa-pq-pow-v1"` as the BLAKE2b key. Matches the
  existing `crypto/hashes/src/hashers.rs` BLAKE2b family that
  already uses `.key($domain_sep)` for kaspa-style domain
  separation (e.g. `MuHashFinalizeHash` keyed with
  `b"MuHashFinalize"`).
- **Self-delimiting:** the input embeds `network_id` plus an
  explicit `len(L1_tag)` byte-length prefix so adding a new
  `algo_id` variant cannot collide with a previous variant's
  encoding.

### Layer 1 — ASIC-resistance tag (`algo_id`-driven)

Identified by an 8-bit `algo_id` carried in the header (added in
PR-8.4). Phase 1 ships `algo_id = 1 = POW_ALGO_ID_KHEAVYHASH`, a
direct re-export of the upstream `cSHAKE256("HeavyHash")` 32-byte
tag. **No claim of ASIC-resistance change at Phase 1.**

Future `algo_id` values (`= 2`, `= 3`, …) are introduced by their
own hard-fork ADRs and may choose Argon2d, Argon2id, RandomX-like,
Cuckoo-like, etc. Switching is a **hard cut-off**: two `algo_id`
values are never simultaneously valid at the same `daa_score`, and
mixed-algo difficulty arithmetic is not part of this design.

### Width chain

```
header.bits  (compact 32-bit)
       ↓ from_compact_target_bits_512
target_512  (Uint512)
       ↓ pow finalize / verify
pow_512     (Uint512)
       ↓ work = floor(2^512 / (target_512 + 1))
work_576    (Uint576)
       ↓ DAA window aggregation
daa_acc_640 (Uint640)
```

The work accumulator is one machine word wider than the target so
that a window of 2^64 maximum-work blocks still fits without
overflow (a deliberately impossible upper bound, but cheap to keep
on the safe side).

### Difficulty lift from upstream's 256-bit world

For any historical 256-bit target `target_256` derived from the
upstream `bits` field:

```
target_512 = target_256 << 256
```

Under the ideal uniform-hash model, this preserves block-finding
probability exactly:

```
Pr[X_512 ≤ target_256 << 256]
  = (target_256 << 256) / 2^512
  = target_256 / 2^256
  = Pr[X_256 ≤ target_256]
```

So if a fork-activation rule is ever needed to lift from upstream
Kaspa difficulty to kaspa-pq Phase 1 difficulty, this is the
preserving map.

### `BlueWorkType` width choice

Phase 1 picks `Uint576` as a **safe ceiling**. The minimal-safe
width is governed by the smallest target the consensus rules will
accept. If a future ADR adds `min_target ≥ 2^256` as a consensus
rule, single-block work is bounded above by `2^256` and
`BlueWorkType = Uint384` becomes safe. Until that ADR lands,
`Uint576` is the chosen width and is not optimised.

## Public claim discipline

External messaging about kaspa-pq Phase 1 PoW must be precise to
avoid over-promising ASIC resistance. Templates:

**Acceptable:**

> kaspa-pq introduces a Layered PoW. Phase 1 establishes the
> quantum-resistant PoW domain — a 512-bit target, a BLAKE2b-512
> finalizer, and `Uint512`/`Uint576` work accounting. Layer 1
> remains the upstream kHeavyHash-compatible function
> (`algo_id = 1`), so ASIC resistance is maintained at the current
> level. ASIC-hard Layer 1 variants are scheduled for Phase 2 and
> beyond via separate hard-fork ADRs.

**Unacceptable:**

> Layered PoW makes kaspa-pq ASIC-resistant.

The second sentence is wrong at Phase 1 — it conflates Layer 0 and
Layer 1. Repeat: Phase 1 ships `algo_id = 1` only, and that
function has the same ASIC profile as the upstream Keccak-based
kHeavyHash.

## Consequences

### Positive

- Quantum-resistance and ASIC-hardness become independent
  hard-fork knobs. Phase 1 can ship without committing to a
  specific ASIC-hard algorithm choice.
- Two different hash families on either side of the PoW reduces
  the blast radius of any single-family structural break.
- DNS WorkScore (the PoS-PoW two-axis finality overlay) cleanly
  binds to Layer 0's `BlueWorkType`. The ASIC-resistance dial
  moves under Layer 1 without affecting WorkScore.
- A future `algo_id` switch (e.g. to Argon2id) only re-derives the
  tag → finalizer input section; the rest of the validator,
  RPC, mempool, and storage stack is unchanged.

### Negative

- One extra hash per block (BLAKE2b-512 over a small input). The
  cost is negligible against the cost of the Layer 1 function.
- Block-header width grows by 1 byte (`pow_algo_id`).
- `BlueWorkType` becomes 72 bytes (Uint576) rather than 24 bytes
  (Uint192). RocksDB rows that store work values grow accordingly.
- DAA arithmetic moves from 256/192-bit ladders to 512/576/640-bit
  ladders. The math is straight-line but does need
  `Uint512` / `Uint576` / `Uint640` types added to `kaspa-math`.

### Neutral

- The minimum target floor is left unset in Phase 1. If a future
  ADR adds `min_target ≥ 2^256`, `BlueWorkType` can be tightened
  to `Uint384`.

## Implementation order (Phase 8 PR sequence)

1. **PR-8.1: ADR-0007 (this).** No code, just the design freeze.
2. **PR-8.2: kaspa-math.** Add `Uint512`, `Uint576`, `Uint640`,
   each via the existing `construct_uint!` macro. Add
   `compact_target_bits_512` helper symmetric with the existing
   `compact_target_bits`.
3. **PR-8.3: consensus/core/src/pow_layer0.rs.** Self-contained
   module exposing `POW_FINALIZER_DOMAIN`, `POW_FINALIZER_BYTES`,
   `POW_ALGO_ID_KHEAVYHASH`, `pow_finalizer_blake2b_512`,
   `lift_target_256_to_512`, `calc_work_512`. Unit tests for the
   difficulty-lift identity and the BLAKE2b-512 finalizer
   determinism.
4. **PR-8.4 (deferred): Header field.** Add `pow_algo_id: u8` to
   `Header`. Recompute genesis hashes (4 networks). Consensus-
   breaking — handled in its own PR so the change is reviewable in
   isolation.
5. **PR-8.5 (deferred): BlueWorkType cascade.** Swap the type
   alias `BlueWorkType = Uint192` → `Uint576`. Cascades through
   header serialization, RPC types, GHOSTDAG data, DAA, and
   downstream consumers (~50 files).
6. **PR-8.6 (deferred): Validation wiring.** Connect
   `pow_finalizer_blake2b_512` into the consensus PoW check; route
   the L1 tag through `pow_algo_id`-driven dispatch.

PRs 8.4 – 8.6 are intentionally separate from PR-8.1 – 8.3 so the
Layer 0 design + math + module can land first and the
consensus-breaking changes can be reviewed and rolled out on a
known good base.

## Phase 2 (deferred): `pow_algo_id` wire support (audit H-04)

**Status: planned; NOT required for the Phase-1 single-algo launch.**

### Why it is safe to defer (the Phase-1 posture)

At Phase 1 only `POW_ALGO_ID_KHEAVYHASH = 1` is admitted, and the rule is already
enforced on the **live** path: `validate_header_in_isolation` calls
`check_algo_id_phase1(header.pow_algo_id)` (→ `RuleError::UnknownPowAlgoId`) for
every ordinary and trusted-IBD header, and pruning-proof import enforces the same
rule (`PruningProofUnknownPowAlgoId`). The field is **internal-only**: it is not
carried on the P2P or RPC wire, so every node reconstructs `pow_algo_id = 1` from
`POW_ALGO_ID_KHEAVYHASH` when decoding a header (`protocol/p2p/src/convert/header.rs`,
the `rpc-core` / `rpc-grpc-core` header models) — there is no `new_finalized`-bypassing
borsh/serde header decode on any network receive path — and binds the locally-recomputed
identity hash to it. So there is no wire field through which a peer can inject a deviating
value, and no mixed-`algo_id` regime: **no Phase-1 consensus-split risk** (audit H-04:
validation present + on the live path; the only residual is forward-compatibility, below).

### Phase-2 work (release-blocker for the `algo_id ≥ 2` hard fork)

Introducing an ASIC-hard Layer-1 variant (`algo_id = 2, …`) is a **hard fork** and MUST
land all of the following together, gated on a new `pow_algo_id_phase2_activation_daa_score`:

1. **P2P wire.** Add `uint32 powAlgoId` to `BlockHeader` in `protocol/p2p/proto/p2p.proto`;
   regenerate the protowire. Plumb it through `From<(HeaderFormat,&Header)>` (send) and
   `Header::try_from` (read — drop the hardcoded `POW_ALGO_ID_KHEAVYHASH` default) in
   `protocol/p2p/src/convert/header.rs`.
2. **RPC wire.** Add the field to `RpcBlockHeader` in `rpc/grpc/core/proto/rpc.proto`
   (regenerate) and to the `rpc-core` header models; carry it through
   `rpc/grpc/core/src/convert/header.rs` and the `rpc-core` `TryFrom`s (replace the
   hardcoded default). Keep `submit_block` / block-template round-tripping the real value.
3. **Consensus rule.** Replace `check_algo_id_phase1` with a height-aware check: below
   `pow_algo_id_phase2_activation_daa_score` admit only `1`; at/above admit `{1, 2}`. Apply
   the same rule in pruning-proof validation.
4. **PoW dispatch.** Route `header.pow_algo_id` to the L1 algorithm in `consensus/pow` (the
   `StateLayer0` finalizer + the L1 tag are already `pow_algo_id`-aware; wire the dispatch to
   the new variant's verifier).
5. **No re-genesis required** — the field already exists on `Header` and is in the
   identity-hash preimage with value `1`; only the wire transport and the rule's admitted set
   change. Existing `algo_id = 1` headers stay valid.
6. **Tests.** A header carrying `pow_algo_id = 2` below activation is rejected; at/above
   activation it validates and dispatches to the L1 verifier; a P2P + RPC round-trip preserves
   the field (no longer defaulted).

Until this lands, `pow_algo_id` MUST remain `1` everywhere and the `check_algo_id_phase1`
gate MUST stay enforced.

## References

- [ADR-0001 — Network isolation](0001-network-isolation.md)
  (kaspa-pq is a fresh chain; the difficulty-lift identity is a
  documentation aid, not a migration path from a live mainline).
- [ADR-0003 — LtHash16_1024](0003-lthash-utxo-accumulator.md)
  (the LtHash empty-state finalize uses the same keyed-BLAKE2b
  family that the Layer 0 finalizer uses, modulo output width).
- RFC 7693 (BLAKE2). FIPS 202 (SHA-3 family — alternative
  considered for the Layer 0 finalizer; rejected in favour of
  BLAKE2b-512 for implementation symmetry with the existing
  kaspa-pq hash stack).
