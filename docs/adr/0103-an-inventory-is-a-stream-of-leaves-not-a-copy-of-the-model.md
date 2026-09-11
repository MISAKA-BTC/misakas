# ADR-0103 — An inventory is a stream of leaves, not a copy of the model

* Status: PROPOSED 2026-09-11 on `feat/adr-0099-sharded-seat`; **W1–W7 IMPLEMENTED the same day,
  consensus-inert** (§9): the leaf, its preimage, the tree, the layout rules and every opening are
  the ones the court already verifies — what changed is how a node BUILDS them, and every root,
  count, byte total, placement and document id it builds is byte-identical to the materializing
  builder's (§1.3, §1.4). No preset, fingerprint or rule moves. W8 is measured on the 2B and
  extrapolated for the 35B (no 35B artifact on this host); W9 and W10 are stated, not built.
* Builds on: [0049](0049-palw-adjudication-contract.md) Decision G (the canonical inventory and its layout
  rules), [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) (the shard plan, which places inventory rows),
  [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the held measurement), [0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
  Decision 3 (a graph-v6 registration pins the INVENTORY root — so building the inventory became
  part of registering and serving) and §8 (which named this ADR's subject as the K3 prerequisite).
* Amends: ADR-0102 §4 and §6 step 4 (the 33 GiB copy is gone from measurement and registration).
  Supersedes nothing.

## 0. The sentence this ADR is

**An artifact inventory's root is a Merkle root over leaves in canonical order, and nothing in it
needs the rows held at once — so a node builds it as a stream: every row planned, then read in
canonical order a block at a time, checked by the one layout rule, hashed into the tree and
dropped.** Before this ADR every node that measured or registered a hybrid class copied every row
of the model into memory first: 5.8 GB for the 2B, more than 33.5 GiB for the 35B-A3B — more than
the machines that measure. The inventory's FORMAT does not change; only the way it is built.

## 1. What was measured

### 1.1 The copy

`palw-class measure --network testnet-11 qwen35-2b.palwq36` on the tree before this ADR
(`252b7b3b`, warm page cache, this Mac): **max RSS 5,816,369,152 B**, peak footprint 3.33 GB,
18.9 s. The builder held every row as an owned `Vec<u8>` until the root was computed, and made
whole-tensor temporaries on the way (a matmul's codes and exponents copied whole before being
chunked, every parameter table widened to its lane count). Under graph-v6 the registered root IS
this inventory's root (ADR-0102 D3), so for the 35B-A3B (≈ 33.5 GiB of rows) there was no host on
the fleet that could measure, register or resolve the class at all.

### 1.2 The stream

The same command on this ADR's tree: **max RSS 164,331,520 B** (peak footprint 121,946,664 B),
18.3 s. The inventory pass alone (the measurement also computes four context rows' walls and shard
plans): 2.1 s, 134 MB RSS, 94 MB footprint — 829,914 leaves in 1,266 tensors. Of what remains, the
artifact's own parameter store (34.8 MB, loaded at open by the reader, not by the inventory) and
one 16 MiB read block are the largest parts.

### 1.3 Byte-identity on the real artifact

| check | result |
|---|---|
| the Measured Model document (`--out`) | **byte-identical**, 5,592 B; id `045a3322f0c6bbe7…` |
| the printed report | identical but for the `written:` path |
| `palw-class verify --artifact` of the OLD document with the NEW binary | identical report, all 42 fields "recomputed, equal" |
| inventory root | `189c78352da5e4931c53a13e7f9f3bb5…` |
| inventory bytes | 2,484,279,940 |

### 1.4 The fixture record

Fourteen cases — five graphs (v1, v2, v5, v6, the v6 artifact-row profile) over the one-row lift and
over a per-token lift, a fixture with tensor-backed group exponents, a fixture with a wrong-length
exponent table, and the fuzz corpus's tiny class under v5 and v6 — were printed by the builder
**before** the emitter existed (`252b7b3b`) and are pinned: root, leaf count and bytes for the nine
that serve, the refusal *message* for the five that do not. The emitter reproduces every line
(`the_emitter_reproduces_the_materializing_builders_record`).

### 1.5 The 35B, extrapolated (W8)

Not measured: the 33.5 GiB artifact is not on this host. From its geometry (40 layers, 256 routed
experts, `moe_dim` 512, hidden 2048, the 2B's tile of 8 rows): ≈ 13.8 M leaves in ≈ 113 k tensors.
The stream holds the plan (one small entry per tensor), one read block and the frontier's
`⌈log₂ n⌉ = 24` peaks; the materialized digest (one 64-byte leaf plus a coordinate per row) would
hold ≈ 2 GB, and the copy ≥ 33.5 GiB. What will dominate the stream's RSS is the reader's own
parameter store — every per-row triple of every expert, ≈ 0.7 GB, resident since `open_artifact` —
which is the loader's design, not the inventory's. The run that turns this into a measurement is
§6 step 1.

## 2. The requirement

A streamed build is the SAME inventory as the materialized one: the same rows in the same order
under the same checks, so the same root, leaf count, byte total, shard placement, Measured Model
document id and `verify` verdicts. A root that moves between them is a defect, never a migration.
And there is one description of the rows — two hand-written builders would be two descriptions of
one computation, which is the correspondence defect ADR-0049's inventory was built to end.

## 3. Decisions

**Decision 1 (W1) — the leaf over borrowed parts.** `artifact_leaf_parts_v1(name, layer, row_start,
bytes)` is the leaf; `artifact_leaf_v1(&operand)` delegates to it. `PalwArtifactLeafHasherV1` takes
the position and the DECLARED length first — the preimage carries the length before the bytes — and
then the bytes in any chunking, so a row larger than any buffer is still one leaf; `finish` refuses a
stream whose absorbed length is not the declared one, the only way a streamed leaf could differ.

**Decision 2 (W2) — rows without bytes, and the rules spelled once.** `PalwArtifactRowDigestV1`
(coordinate, length, leaf), `PalwArtifactInventoryDigestV1` (the rows under the same checks; its
`opening_v1(index, bytes)` takes the row's own bytes back, checks them against the leaf and returns
the materialized opening byte for byte) and `PalwArtifactInventorySummaryV1 {root, leaf_count,
artifact_bytes}`. The layout rules have ONE spelling, `PalwInventoryLayoutCheckerV1`, which checks a
row given the rows before it; the held inventory's check is that checker run over the slice, so a
stream is refused by the same rule, at the same row, with the same error.

**Decision 3 (W3) — one emitter, sinks for the rest.** `qwen36_visit_inventory_rows_v1` is the only
place a Qwen3.6-family inventory's rows are laid out. It PLANS first — the profile walked slot by
slot, every refusal the layout has raised in slot order before a byte is read, each tensor's rows
described (`Stored`, `Zeros`, `Params`, `Small`, `Rope`) — then READS in canonical order, tensor by
tensor. The old builder's "first pushed coordinate wins" is restated per tensor: where several slots
plan one tensor, an EQUAL plan adds nothing (rows are a function of the plan), and different plans
are merged by offset with the first-planned kept, holding that one tensor. Every hybrid graph plans
exactly one tensor pair twice — a full-attention layer's rotation table and its clamp, named by both
rotations — with equal plans; the buffered merge is pinned on a synthetic plan because no shipped
graph reaches it. The sinks: `qwen36_inventory_v1` (the bytes kept — the court's opening path, still
used), `qwen36_inventory_digest_v1` (leaves kept), `qwen36_inventory_summary_v1` and
`qwen36_inventory_measure_v1` (nothing kept per row). The old builder's body is gone; its record is
the pin of §1.4.

**Decision 4 (W4) — row-sized scratch.** `Qwen36ArtifactV1::tensor_len` (present exactly when
`tensor` would answer) and `read_tensor_range_into` (through the file descriptor on a mapped store,
never the mapping — the reason `artifact_root` gives: faults run at 6 MB/s where reads run at
1.3 GB/s, and a faulted page stays resident). A parameter table is read through the engine's own
rule (a full table verbatim, a singleton to every lane, a head table repeated) and never widened into
a copy. The read size is not part of the result: pinned equal at 1, 7, 64 and 4096 bytes.

**Decision 5 (W5) — the measurement streams.** `palw-class measure`/`verify` hold one entry per
TENSOR, its rows' bytes summed: `palw_artifact_bytes_from_inventory_v1` places a row by its tensor
and layer and adds its length, so a tensor's sum places exactly what its rows place (pinned on every
case against the per-row placement).

**Decision 6 (W6) — the registration root streams.** The SDK's `registered_root_of`, the memoized
graph-v6 root a producer registers and the chain-registered arm resolves, is the summary's root.

**Decision 7 (W7) — the frontier.** `PalwArtifactMerkleFrontierV1` keeps one peak per level and
folds them from the lowest at the end; carrying an unpaired node upward untouched IS promotion, so
its root is `artifact_root_v1`'s at every size (pinned for 1–300 leaves, around powers of two, and
1,100). `PalwArtifactInventoryStreamV1` is the checker, the frontier and a byte count.

## 4. What this does not change, and what it does not reach

* **Consensus: nothing.** The leaf domain and preimage, the promote-odd tree, the layout rules, the
  openings and the court's checks are unchanged; so is every ruleset id and fingerprint.
* **The dense and BASE-0 builders** (`a16_inventory_v1`, `base0_inventory_v1`) still materialize.
  Their classes are 1.5 B-parameter and below, so they are not what blocks a K3-class model; the
  same emitter-and-sinks shape applies when they are next touched.
* **The court's openings** (`operand_openings_for`, `attn_site_evidence` in the hybrid backend) still
  build the materialized inventory to open a handful of rows — for the 35B that is still the
  33.5 GiB copy, on the path a seat takes when it is sampled. That is W9 (§6).

## 5. Invariants the tests hold

1. The parts leaf and the streamed leaf are the operand leaf; a short stream is refused —
   `the_parts_leaf_and_the_streamed_leaf_are_the_operand_leaf`.
2. The frontier's root is `artifact_root_v1`'s at every size —
   `the_frontier_root_is_the_promoting_root_at_every_size`.
3. The digest inventory roots, checks and opens as the materialized one, and a stream of the same
   rows yields its summary or its refusal — `the_digest_inventory_roots_checks_and_opens_as_the_materialized_one`.
4. The emitter reproduces the old builder's fourteen-line record —
   `the_emitter_reproduces_the_materializing_builders_record`.
5. On every serving case: the digest's rows are the materialized rows digested, the summaries are
   equal, one entry per tensor places what the rows place, every read size yields the same rows in
   canonical order, and openings from the digest are the materialized openings —
   `the_digest_is_the_inventory_on_every_case_and_every_read_size`.
6. First-planned-wins per tensor, equal plans once, and the only tensors any shipped graph plans
   twice are a rotation table and its clamp — `a_tensor_planned_twice_keeps_the_first_planned_row_of_each_offset`.
7. A written-and-reopened (mapped) artifact streams the inventory of the in-memory one —
   `a_mapped_store_streams_the_same_inventory`.
8. On this host's real 2B artifact: §1.3 (a recorded run, not a test — the artifact is not in CI).

## 6. Order of work

1. **W8 on a host holding the 35B-A3B:** `palw-class measure` under `/usr/bin/time`, the RSS
   recorded here, the document's root compared with a second run. Needs the operator's host.
2. **W9 — openings from the stream, a separate ADR.** The plan already IS the recipe for any
   tensor's rows (`Q36PlannedTensorV1`), so an opening needs only the root (cached once per holding
   as a digest, ≈ 64 B per leaf, or recomputed) and the one tensor's rows re-read; a `.pidx`
   sidecar is the persisted form of the same recipe. Then `operand_openings_for` and
   `attn_site_evidence` stop copying the model.
3. **W10 — shard seats** measure and serve their shard's tensors: the plan filtered by the shard's
   placement (ADR-0099), the rest never read.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0102 §4, "a graph-v6 root costs one pass that copies the artifact's rows" | a pass that holds one read block (§1.2) |
| ADR-0102 §6 step 4, "a streaming inventory" | done for measurement and registration; openings are W9 |
| the Qwen3.6 builder's body (a slot loop pushing owned rows, a seen-set, a final sort) | one emitter; its record pinned (§1.4) |

## 8. What is deliberately not decided

* **Whether the reader's parameter store should itself stream** (≈ 0.7 GB for the 35B, resident
  from open). It is the loader's design and the engine reads it per token; §1.5's measurement says
  whether it matters.
* **The sidecar's format** (W9), and whether a seat caches a digest or recomputes the root.

## 9. Number hygiene and implementation record

0103 is the next free number after ADR-0102 (whose §9 says so). Claimed on
`feat/adr-0099-sharded-seat` on 2026-09-11. **The next free number is 0104.**

* **2026-09-11** — written and implemented the same day:
  * `consensus/core/src/palw_artifact.rs` — `artifact_leaf_parts_v1`, `PalwArtifactLeafHasherV1`,
    `PalwArtifactMerkleFrontierV1`, `PalwInventoryLayoutCheckerV1` (the held check now runs it),
    `PalwArtifactRowDigestV1`, `PalwArtifactInventoryDigestV1`, `PalwArtifactInventorySummaryV1`,
    `PalwArtifactInventoryStreamV1`, `PalwArtifactInventoryV1::summary`, the streaming tests;
    `palw_shard_plan_v1.rs` — `PalwInventoryRowMetaV1` from a row digest.
  * `misaka-palw-base0/src/qwen36.rs` — `tensor_len`, `read_tensor_range_into`.
  * `misaka-palw-base0/src/inventory.rs` — the plan (`qwen36_inventory_plan_v1`), the canonical
    emission (`qwen36_emit_plan_v1`), `qwen36_visit_inventory_rows_v1`, the four sinks, the
    `adr0103_cases` record and tests.
  * `misaka-palw-sdk` — `palw-class measure`/`verify` through `qwen36_inventory_measure_v1`;
    `registered_root_of` through `qwen36_inventory_summary_v1`.
