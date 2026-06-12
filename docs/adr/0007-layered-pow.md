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

## Phase 2 — Argon2id (`algo_id = 2`): LANDED, then SUPERSEDED

Phase 2 shipped the memory-hard **Argon2id 16 MiB** Layer-1 (`POW_ALGO_ID_ARGON2ID = 2`,
`pow_layer0::argon2id_l1_tag_v1`) on testnet/mainnet to compress the GPU↔ASIC gap. It worked,
but exposed a structural cost: a memory-hard PoW is **symmetric** — the verifier pays (a large
fraction of) the miner's per-hash cost. Measured on the weakest mesh host, one Argon2id-16 MiB
header verification is ~48 ms (~20 H/s/core), which makes **PoW verification the IBD / catch-up
bottleneck**: a node syncing the header chain spends the majority of its CPU re-running Argon2id.

The `algo_id = 2` code (constant, `argon2id_l1_tag_v1`, the `StateLayer0` dispatch arm) is kept
present-but-dormant so historical pruning proofs spanning the Phase-2 era still verify
(`check_algo_id_known` admits `{1, 2, 3}`); no live network selects it.

## Phase 3 — compute-only BLAKE2b-512 ∥ SHA3-512 (`algo_id = 3`): ACTIVE

To remove the catch-up bottleneck, Phase 3 replaces Argon2id with a **compute-only** Layer-1 tag
(`POW_ALGO_ID_BLAKE2B_SHA3 = 3`, `pow_layer0::blake2b_sha3_l1_tag_v1`):

```text
half_b = BLAKE2b-512(key = "kaspa-pq-l1-blake2b-sha3-v1", netid_len||netid || pre_pow_hash64 || nonce_le)   // 64 B
half_s = SHA3-512(        "kaspa-pq-l1-blake2b-sha3-v1" || netid_len||netid || pre_pow_hash64 || nonce_le)   // 64 B
l1_tag = half_b || half_s                                                                                    // 128 B
```

The 128-byte tag (≤ `POW_L1_TAG_MAX_BYTES = 256`) feeds the **unchanged** Layer-0 BLAKE2b-512
finalizer with `algo_id = 3`, which mixes a second BLAKE2b-512 over the whole preimage — so the
accepted digest depends on every tag byte and a miner cannot skip the SHA3 half. Per-nonce work is
`2×BLAKE2b-512 + 1×SHA3-512` (all compute-only), giving ~10^4× faster verification (~µs/header).

### The trade-off (explicit and accepted)

A compute-only PoW is **not** memory-hard: GPU/FPGA/ASIC acceleration is possible — Phase 3 gives up
the kHeavyHash/Argon2id-era ASIC-resistance goal. This is acceptable in this fork because PoW is no
longer the sole security pillar: the **two-dimensional (PoW × stake) DNS finality overlay**
(ADR-0009) gates reorgs on stake-confirmed anchors, so a pure-PoW majority cannot rewrite confirmed
history. The decision optimizes for sync/verification cost (a real operational pain) over PoW
egalitarianism (already softened by the overlay). The two hash families are concatenated as a
cryptographic hedge: a break in one of BLAKE2b-512 / SHA3-512 still leaves the Layer-0 finalizer's
comparison intact.

### Activation & re-genesis

- Gated on `Params::pow_blake2b_sha3_activation` (renamed from `pow_argon2id_activation`):
  `always()` on testnet/mainnet (`algo_id = 3` from genesis), `never()` on devnet/simnet (stay
  kHeavyHash). `required_algo_id(active)` → `3` when active, else `1`; `check_algo_id` enforces the
  exact id per header DAA score (single-algo invariant, no mixed-`algo_id` DAG).
- **Re-genesis with a clean genesis-hash break.** Genesis is PoW-exempt and still declares
  `algo_id = 1`, so the algo switch *alone* would leave the genesis hash byte-identical and only
  invalidate *post-genesis* Argon2id blocks. That is unsafe: a node restarted **without wiping its
  DB** would silently resume the old Argon2id chain (`factory.rs` reopens an existing consensus with
  `process_genesis=false`) and graft `algo_id = 3` blocks onto an `algo_id = 2` history, splitting
  from freshly-genesised nodes. So the re-genesis bumps each live net's coinbase with a Phase-3
  relaunch marker (`"-bs3"`, mirroring TN11's `11,1`), which **changes the genesis hash** of
  `GENESIS` (mainnet) and `TESTNET_GENESIS` (testnet-10). Devnet/simnet stay kHeavyHash and are
  untouched; `TESTNET11_GENESIS` is vestigial (no active params) and untouched.
- **Startup genesis-mismatch guard.** `Consensus::new` now asserts, when opening an existing DB
  (`process_genesis=false`), that the DB's recorded genesis (`past_pruning_points_store[0]` — written
  at genesis processing, never pruned, the proof anchor) equals the configured `genesis.hash`. With
  the marker bump an un-wiped node's stored genesis ≠ the new configured genesis, so it **refuses to
  start** with a "wipe to re-genesis" message instead of silently resuming the old chain. (This
  mirrors the invariant the pruning processor already asserts at runtime.)
- **Operational re-genesis runbook.** Stop ALL nodes; delete every consensus data dir AND the
  explorer DB; rebuild from this commit; relaunch. The guard makes a forgotten wipe a loud refusal,
  not a silent split.
- **Difficulty**: the inherited genesis `bits` are Argon2id-era and ~10^4× too easy for the fast
  hash; they are kept as the easy launch floor (self-correcting — the DAA ramps difficulty to the
  hash-rate equilibrium `D ≈ aggregate-H/s ÷ BPS` within the first `MIN_DIFFICULTY_WINDOW_SIZE`
  blocks under **un-throttled** mining, and never stalls). Re-bench with `pq-miner --bench-secs`
  (now measures BLAKE2b-SHA3) on launch hardware and pre-set `bits` near equilibrium to skip the
  initial instamine ramp (re-genesises the `hash` constant — recompute via `gen_kaspa_pq_genesis_hashes`).
- The P2P/RPC wire already carries `pow_algo_id` as a generic `u8`/`u32` (Phase-2 wire support
  landed), so `algo_id = 3` round-trips with no transport change.

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
