# PALW spec — 18. Layer-sharded panels

> **Normative.** This chapter specifies RFC-0006 (adopted 2026-10-03, every open question at its recommended
> default): a seat of an IR class verifies a **cell** of a claim — a contiguous range of layers crossed with a
> position segment — from the claim's committed boundary rows and only that range's weights, and a claim of a class
> that declared a **layer-shard plan** licenses **by parts**.
>
> It applies past the dormant fence `palw_tir_shard_v1`. Below the fence nothing here is read, every table is
> empty, every object of §18.0 is dropped by name at acceptance and refused by the fold, and every root and carriage
> is byte-identical to a build without it. An engineer who reads only this file and chapters 04b and 17 must be able to
> build a second implementation of the plan, the draw, the receipt, the recount, the licence by parts, the lock and
> the pay that agrees with the fold on every input.
>
> Status: **Draft for review (lane S)**. §18.0 (tags and ids) is final for v1.

Contents:
- §18.0 tags and ids;
- §18.1 the fence;
- §18.2 the plan;
- §18.3 cells and segments;
- §18.4 the panel;
- §18.5 the receipt;
- §18.6 the recount and the licence by parts;
- §18.7 locks and pay;
- §18.8 the class room;
- §18.9 shard readiness;
- §18.10 data availability of boundary rows (`TirStepRun`);
- §18.11 rules PALW-LSP-1…20;
- §18.12 the node (informative).

---

## 18.0 Tags and ids

Every number below was chosen free on every branch that touches the same space (§17.0's rule; the core lane's table is
the authority):

| Space | Number | Name |
| --- | --- | --- |
| `PalwConsensusObjectV2` | 91 | `TirShardPlanDeclared { class_id, s_l, s_p, signature }` |
| `PalwConsensusObjectV2` | 92 | `TirShardReceiptLicensed { part }` (`PalwTirShardPartV1`) |
| `PalwConsensusObjectV2` | 93 | `TirSeatReadinessProved { bond, class_id, shard, span, proof, signature }` |
| `PalwDaUnitV1` | 7 | `TirStepRun { first: u64, count: u32 }` |
| `PalwDaAnswerV1` | 10 | `TirStepRun(PalwTirStepRunDisclosureV1)` |
| `PalwDeltaEntryV2` | 101 | `TirShardPlan { class, old, new }` |
| `PalwDeltaEntryV2` | 102 | `TirShardClaim { claim, old, new }` |
| `state_root` block | — | `tir-shard/v1`, hashed only when `tir_shard_plans` or `tir_shard_claims` holds a row |
| Carriage tail | `0xE1` | the two tables, encoded only when non-empty |

The three objects carry explicit discriminants (`#[borsh(use_discriminant = true)]`, as tags 83–90 do). Each is declared
below `palw_lifecycle_object_may_ride_v2`'s stateless gate (all three ride at every height), dropped by name below the
fence at acceptance (first, charged nothing: an older build skips an unknown tag), and refused by the fold as the second
lock (`TirShardDormant`). `TirShardReceiptLicensed` is not an H-1 heartbeat carrier.

ML-DSA-87 contexts and message domains (keyed BLAKE2b-512; **not** in testnet-12's committed context set V5 — they are
covered by the Some-only fence, as the batch licence's and the market's are, and live in their own registry
`PALW_TIR_SHARD_V1_ALL_DOMAINS`):

| Name | Value |
| --- | --- |
| plan message domain / context | `misaka-palw/tir-shard/plan/message/v1` / `misaka-palw/tir-shard/plan/mldsa87/v1` |
| receipt V4 message domain / context | `misaka-palw/receipt-v4/message/v1` / `misaka-palw/receipt-v4/mldsa87/v1` |
| shard ready-class domain | `misaka-palw/tir-shard/ready-class/v1` |
| shard seed domain | `misaka-palw/tir-shard/shard-seed/v1` |
| readiness message domain / context | `misaka-palw/tir-shard/readiness/message/v1` / `misaka-palw/tir-shard/readiness/mldsa87/v1` |
| readiness challenge domain | `misaka-palw/tir-shard/readiness/leaves/v1` |

## 18.1 The fence

| Item | Value |
| --- | --- |
| Field | `Params::palw_tir_shard_v1: Option<ForkActivation>` |
| `consensus_params_id` | Some-only: `"palw_tir_shard_v1"`, then the activation (LE u64) |
| `consensus_schedule_id` | Some-only: the same two writes |
| Normaliser | `Some(never())` collapses to `None`; `for_each_fence` visits the activation only |
| Fork id | listed in `palw_fences_v1` as `("palw_tir_shard_v1", activation)` |
| Mirror | `PalwStateParamsV2::tir_shard_from_daa` (borsh-skipped), written by `Params::sync_palw_tir_shard_v1` only |
| Prerequisites | a `ConsensusV2` ruleset (checked first); `palw_tir_v1`, `palw_tir_fence2`, `palw_verification_v2`, `palw_rcore_plus` and `palw_admission_independence` in force at or below the activation, each refused by name (`validate_palw_tir_shard_v1`) |
| Shipped | `None` on every preset, in no flag-day list. One fence, armed after a node-only shadow period (RFC decision 1) |
| Drill | `--palw-drill-tir-shard-at` (`palw_drill_tir_shard_at_v1`, list `PALW_DRILL_TIR_SHARD_FENCES_V1`), armed after `--palw-drill-tir-at` and `--palw-drill-tir2-at` |

## 18.2 The plan

A class's registrant declares **one plan**, once, immutable like the class's graph:
`TirShardPlanDeclared { class_id, s_l, s_p, signature }`, signed by the registrant's bond under the plan context over
`palw_tir_shard_plan_message_v1(network_domain, class_id, s_l, s_p)`.

- **Where.** A registered IR class with a registrant (a genesis class has none and is never sharded:
  `ShardPlanFromGenesisClass`), declared once (`ShardPlanAlreadyDeclared`), at or past the fence.
- **Shape** (`palw_tir_shard_plan_shape_v1`): `S_L ∈ [max(min, 2), max(2·min, 2)]` and at most one shard per layer
  and at most 64; `S_P ∈ {1, s_shard − 1}` = `{1, 2}`. `min` = `palw_tir_shard_min_shards_v1`, the fewest shards whose widest
  shard fits the network's seat budget `PALW_TIR_SHARD_SEAT_BUDGET_BYTES_V1` (3.5 GiB). A registrant may declare up to
  `PALW_TIR_SHARD_OVERSHARD_FACTOR_V1` = 2 times the derived `min`, so a class cannot thin its own panels.
- **The layer partition** (`palw_tir_shard_partition_v1`, RFC §1.1): the contiguous partition of the layers into `S_L`
  shards that minimises the widest shard's weight. A layer's weight is the bytes of every param instance its block reads
  plus the `Hist` and `Fixed` state it holds at `max_context` (4 bytes a value); global params and global state count on
  every shard that holds a layer; `pre` (and its extra params) rides with shard 0, `post` with shard `S_L − 1`. Ties
  break to the lowest first cut. Occurrence indices: `0` = `pre`, `1 + l` = layer `l`, `L + 1` = `post`
  (`palw_tir_shard_occurrences_v1`).
- **The cell table** (`cell_permille`, `S_L × S_P` entries, shard-major, sum exactly 1,000; largest-remainder rounding,
  ties to the lowest index): each cell's structural work (PALW-TIR-16, `work_cell_v1` over the cell's occurrences and
  positions) as permille of the class's canonical job (`palw_tir_shard_canonical_facts_v1`: half prefill, the rest
  decode, over `max_context`). Stored with the plan; derived once, at the declaration.

State: `tir_shard_plans: class → PalwTirShardPlanV1 { s_l, s_p, declared_daa, cell_permille }`.

## 18.3 Cells and segments

A job of `T = prefill + decode − 1` positions is cut into `S_P` segments at multiples of `G = lcm(C, h_tile)` (the
class's checkpoint interval and history tile; `palw_tir_shard_segment_align_v1`): segment `j` is the positions
`[⌊j·T/S_P⌋_G, ⌊(j+1)·T/S_P⌋_G)`, the last ending at `T` (`palw_tir_shard_segment_positions_v1`; empty when the job is shorter than
`G`). A **cell** is `(shard i, segment j)`: the occurrences of shard `i` at the positions of segment `j`.

**Verifying a cell** (spec 04b's step space, PALW-TIR-1…; the node's function is `verify_cell_v1`): the verifier reads
committed leaves only — the previous occurrence's carry-out commit tiles (shard `i > 0`), the job's tokens (shard 0, and
`Input(1)` everywhere), for a segment `j > 0` the checkpoint tiles at `p − 1` and the history tiles that complete the
window — recomputes every leaf of the cell with the shard's own params, and compares each recomputed leaf hash with the
committed one. The last shard also checks each generated id against the greedy selection over its own committed logits
row. The first leaf that differs is the cell's finding.

**The detection lemma (RFC §2.1).** The first divergent leaf of an execution lies in exactly one cell, and that cell's
seat finds it, because every input of the leaf is a committed leaf the cell holds or an output of an earlier leaf of
its own recomputation. A lie at a boundary row that the downstream shard computes honestly from is found by the
**upstream** cell, whose recomputation of that row disagrees. A cell finding is the input of the IR one-move court
(`TirShardCourtAccused`, tag 62), whose cone close is unchanged.

## 18.4 The panel

For a claim of an IR class that declared a plan, bound at or past the fence, the panel is drawn **per shard** and stored
flat, **shard-major**, each shard `[outsider?] ++ s_shard class seats` with `s_shard = 3`
(`palw_tir_panel_stride_v1(outsider)` = 3 or 4 seats; `palw_tir_panel_shard_slice_v1`).

- **Class seats.** For shard `i`, `derive_panel_v2_with_policy_judged_v1` over the shard's own seed
  `H(shard-seed domain ‖ panel seed ‖ claim ‖ LE u16 i)` and the shard's **ready class**
  `H(ready-class domain ‖ class ‖ LE u16 S_L ‖ LE u16 i)`: the draw's candidates are the bonds with a standing readiness row
  under that derived class (§18.9), one seat per operator per shard, by the draw policy in force (the stake race past
  `palw_rcore_plus`). The claim's own executor is excluded by bond, operator and key. **A bond may sit in several shards**
  (its seats are distinct duties with distinct masks, §18.5; its lock accumulates, §18.7).
- **The outsider.** Where the claim is outsider-judged (`palw_claim_is_outsider_judged_v1`: a bought class past
  `palw_admission_independence`), each shard's slice leads with its outsider, drawn from the base-class population exactly as
  ADR-0147 draws the claim's, under the outsider ticket domain with the shard in the ticket seed. One operator holds at
  most one outsider seat of a claim, and a shard's outsider operator is not also a class seat of that shard.
- **A short shard** refuses the whole draw by name (`InsufficientEligibleShardBonds { shard, needed, available }`); a sharded
  class never falls back to a flat panel. A panel bound under a plan whose length is not `S_L × stride` is a flat panel (bound
  before the plan, or of a class that declared none) and writes no shard record.
- **The segments each seat attests** (`palw_tir_shard_assignment_v1`, `palw_tir_shard_seat_mask_v1`): the outsider attests every
  segment of its shard. `S_P = 1`: every class seat attests the whole shard. `S_P = 2`: Verification V2's assignment
  (`palw_segment_assignment_v2`) over the shard's own seed with `k = s_shard − 1 = 2` segments among the three class seats — one
  full-shard seat attesting both segments and one partial seat each.

At bind the fold writes the claim's record `tir_shard_claims[claim] = PalwTirShardClaimV1 { s_l, s_p, outsider, progress,
cell_counts, counted, drawn_permille, unserved_seen }` (the plan frozen at the bind: a later declaration cannot move it;
`drawn_permille[i]` is stored seat `i`'s share of the claim's work, §18.7). `PanelBound` is refused unless its seats are exactly the derived panel.

## 18.5 The receipt

`PalwSeatReceiptV4 { receipt: PalwSeatReceiptV2, shard: u16, segments: PalwSegmentMaskV2 }`, signed under the V4 context
over `palw_receipt_message_v4(network_domain, claim, verdict, signed_daa, shard, segments)` = H(V4 domain ‖
`palw_receipt_message_v2(…)` ‖ LE u16 shard ‖ LE u32 mask), so a relayer cannot move a receipt to another shard or widen its
mask. Verdicts keep their meaning: `Valid` attests every leaf of every cell of the mask; `Unavailable` names a missing input
leaf; `Incapable` is unchanged; `Sampled` counts nothing. The receipt window is the claim's (`bound + window`, DA-5 shifts
included). A V4 receipt's borsh is the V2 receipt's followed by `shard` and `segments`, so no other receipt version decodes as
one and it decodes as none of them.

## 18.6 The recount and the licence by parts

`TirShardReceiptLicensed { part: PalwTirShardPartV1 { claim, shard, receipts: Vec<PalwSeatReceiptV4> } }` — at most
`s_shard + 1` receipts, one carrier at any shard count. `palw_tir_shard_part_verdict_v1` is the one function the acceptance
layer, the assembler and the fold call:

1. each receipt is the claim's, for this shard, from a seat of this shard's slice, `Valid`, a distinct bond, and its mask is
   **exactly** the seat's assigned mask (§18.4);
2. where the claim is outsider-judged, the slice's first seat (the outsider) has answered `Valid`;
3. the class seats' masks cover **every segment** of the shard at least `PALW_TIR_SHARD_ATTESTERS_PER_CELL_V1` = 2 times
   (the outsider is counted beside them in the recount, not in this test);
4. otherwise the part is `Short`.

A part lands once per shard (`ShardAlreadyLicensed`). The part that completes the plan licenses the claim in that block,
with `licence_door = ShardPart { quorum_per_shard: 2 + outsider }` and
`basis_k = min(3, min over cells (i, j) of #distinct counted Valid signers covering (i, j))`
(`palw_tir_shard_basis_k_v1`, over every part's counted signers); `Final` requires `basis_k ≥ 2` (`PALW_RCORE_FINAL_BASIS_K_V1`),
unchanged. For a flat claim `S_L = 1` and the count is `palw_receipt_set_basis_k_v1` exactly. A claim that licenses by parts
**refuses every whole-object door** (`LicensedByParts`): `ReceiptLicensed`, the V1 quorum, the coverage door, the optimistic door
and `ProducerDefaulted`. A shard whose receipts say `Unavailable` sets the licence's `unserved_seen` latch (C1). Supplementary
receipts may raise `basis_k`, never lower it (Q-3).

## 18.7 Locks and pay

- **Lock.** A seat counted by a part locks `lock_{max(basis_k,2)}` scaled by its share:
  `⌊lock × max(share, 125) / 1000⌋` (at least 1), `share` = the stored `drawn_permille` of its place
  (`PALW_TIR_SHARD_SEAT_FLOOR_PERMILLE_V1` = 125, decision 6: a seat on a small cell still locks and is paid at least an
  eighth). A bond counted in several shards **accumulates** one lock per `(bond, shard)` (`segments: 0`, so the lock is
  released and charged as one whole-claim lock), each at its own share. A `Leaf` fault (Q-6) is charged to the receipts whose
  cell covers the leaf's cell.
- **Pay.** At `Final`, the panel's pool `⌊reward × pool_permille / 1000⌋` is divided over the drawn seats by their (floored)
  share; a credited seat is paid its part, the producer the exact rest, and what no seat was credited for, with the division's
  dust, is the reserve — never the producer's (`palw_tir_shard_split_v1`: `producer + Σ paid + reserve == reward`). Per
  `(bond, shard)` leg.

## 18.8 The class room

Past the fence the class's room (the number of claims the registry admits for the class in the window) is the **binding
shard's**: `min over shards i ⌊ready_i × per_seat × window × 1000 / (2 × eccu × permille_i)⌋` (`palw_tir_shard_room_v1`), where
`ready_i` is the shard's `ready_eff` (the effective ready seats of its derived ready class), `permille_i` the shard's share of
the work (the sum of its cells' permille), and the other terms the flat room's. A class with no plan keeps the flat rule.

## 18.9 Shard readiness

`TirSeatReadinessProved { bond, class_id, shard, span, proof, signature }` is a possession proof over **one shard's rows**:

- the multiproof reconstructs the class's registered `artifact_root` and opens at most the readiness-V2 carrier budget;
- the leaves opened are the carrier-bounded prefix of `palw_tir_shard_readiness_leaves_v1(class, S_L, shard, bond, span,
  ranges, PALW_READINESS_V2_CHUNKS_V1)`, drawn from the shard's own inventory rows
  (`palw_tir_shard_inventory_ranges_v1`: the leaves of every param instance the shard's occurrences read) — so the draw
  seats exactly the bonds that hold the shard;
- `span` is the current span or one the landing window admits, and strictly newer than the bond's standing row for the derived
  class;
- signed by the bond under the readiness context over `palw_tir_shard_readiness_message_v1(network_domain, bond, class, S_L,
  shard, span, opened (index, leaf hash))`.

It writes `seat_readiness[(bond, ready_class)]` (the same table, the same row, proof version 2). A bond that proves several
shards stands in several derived classes.

## 18.10 Data availability of boundary rows: `TirStepRun`

A cell reads runs of contiguous leaves (a position's commit tiles, the carry-out tiles of the previous occurrence). The DA unit
`TirStepRun { first, count }` (`count ≤ PALW_TIR_STEP_RUN_MAX_LEAVES_V1` = 256) demands them as **one** unit: the answer
`PalwTirStepRunDisclosureV1 { binding, preimages, range }` carries the preimages and one range opening
(`step_merkle_range_siblings_v1`) that `check_tir_step_run_disclosure_v1` walks to the claim's step root. Past the fence only
(`palw_tir_shard_v1` and `palw_tir_fence2`); a run past the execution is proven out of range by the binding, as a leaf past it is.
The runs a cell reads are listed by `cell_runs_v1` (the node's function): the carry-in tiles lead each run, and a segment
restore's checkpoint and history tiles are runs of their own. The demand and the answer ride the same accusation and disclosure
path as `TirStepLeaf` (`DefaultAccusedTirStep`, `MaterialDisclosedV2`): the executor answers inside `W_disclose` or its claim
defaults (`ProducerWithholding`).

## 18.11 Rules

| Id | Rule |
| --- | --- |
| PALW-LSP-1 | Below `palw_tir_shard_v1` every object, unit and answer of this chapter is dropped by name at acceptance and refused by the fold; no table has a row and no root changes |
| PALW-LSP-2 | The fence has all four writes (field, `for_each_fence`, Some-only id writes, `never()` collapse) and is dormant on every preset |
| PALW-LSP-3 | A plan is declared once, by the class's registrant, for a class that is not a genesis class, with a shape the program and the seat budget accept |
| PALW-LSP-4 | The layer partition and the cell table are pure functions of the program, `max_context`, `S_L` and `S_P` |
| PALW-LSP-5 | A claim bound at or past the fence under a plan draws a panel per shard; the panel is exactly the derived one |
| PALW-LSP-6 | A short shard refuses the draw by name; there is no flat fallback for a planned class |
| PALW-LSP-7 | A bond may sit in several shards; one operator holds at most one outsider seat of a claim |
| PALW-LSP-8 | A receipt's mask is exactly its seat's assigned mask, and its signature covers the shard and the mask |
| PALW-LSP-9 | A part licenses its shard iff its outsider (where outsider-judged) says `Valid` and every segment has at least two distinct class attesters |
| PALW-LSP-10 | A shard lands once; the part that completes the plan licenses the claim and records `basis_k` over cells |
| PALW-LSP-11 | A claim that licenses by parts refuses every whole-object licence door and `ProducerDefaulted` |
| PALW-LSP-12 | A seat's lock is `max(share,125)‰` of the whole lock, accumulated per `(bond, shard)`; its pay is proportional to the same share |
| PALW-LSP-13 | `producer + Σ paid + reserve == reward` for every `Final` claim drawn per shard |
| PALW-LSP-14 | The class room is the binding shard's; a class without a plan keeps the flat room |
| PALW-LSP-15 | A shard readiness proof opens leaves drawn from that shard's own inventory rows, and is strictly newer than the bond's standing row |
| PALW-LSP-16 | A run unit's answer is checked against the claim's step root by one range opening |
| PALW-LSP-17 | The first divergent leaf of an execution is found by exactly one cell, and a consistent boundary lie by its upstream cell |
| PALW-LSP-18 | Producer and verifier share one function for every derivation above (§18.4, §18.6, §18.7) |
| PALW-LSP-19 | Every arithmetic step in the fold is checked; every hostile input is refused by name |
| PALW-LSP-20 | The delta of every block reverts exactly and the carriage reloads under its root (entries 101, 102; tail `0xE1`) |

## 18.12 The node (informative)

- **Seat.** A duty carries its shard place (`PalwSeatDutyV2::tir_shard`: shard, `S_L`, `S_P`, outsider flag, slice index and
  assigned mask), read off the claim's record. The node leaves the flat replay for such a duty, verifies its cells over the
  claim's capture (`tir_verify_capture_cells_v1`, the CPU executor's `step_cell`, from the shard's own params), and files a V4
  receipt. A cell finding builds the same `TirShardCourtAccused` a whole-job replay builds, from the finding's leaf.
- **Device.** Cells run through `KernelBackendV1`; a device is a `TirDeviceV1` (`misaka-palw-tir-exec::cellstep`: it builds the
  stepper of a cell from the cell's own params, no consensus type crosses) that a binary registers (`register_device_v1`) and
  `--palw-tir-shard-gpu` selects (default off). A refusal of the device — a cell of a segment after the first, params it cannot
  hold, a history short of rows — is never a different verdict: the CPU runs the cell (`gpu-integer-backend.md` §8). The GPU
  crate (`misaka-palw-tir-gpu`, an isolated workspace) implements `TirDeviceV1`; its `tests/cell.rs` holds the cell grain equal to
  the CPU executor's. A node binary that carries it needs a plug-in boundary, because wgpu 30 (`js-sys ^0.3.104`) and the consensus
  stack (`js-sys =0.3.77`) have no common lock.
- **The device helper.** A device that cannot share the node's lock runs as a helper process (`palw-tir-gpu-helper`, built in the
  GPU workspace): `--palw-tir-shard-gpu` spawns it and speaks the versioned cell wire of `misaka_palw_tir_exec::cellproc` over its
  stdin and stdout (`Hello`, `Open{program, occ, params}`, `Step`, state reads, `Close`). With `--palw-tir-shard-gpu-mirror` (always
  in shadow) every answer is checked against the CPU executor's byte for byte; a difference, an error or a refusal takes the helper out
  of service and the CPU runs the cell.
- **A seat without the capture** (`--palw-tir-shard-demand-runs`): it demands the job's leaves as `TirStepRun` units (at most one
  run of 256 leaves per session, four sessions), and once the executor has answered every run the leaves make the capture
  (`TirBackendV1::capture_from_runs_v1`: the step root recomputed over them must be the binding's) which it verifies its cells over.
- **A seat without the class** (`--palw-tir-shard-mirror`): it fetches only its shard's inventory rows
  (`palw_tir_shard_inventory_ranges_v1`), each an artifact opening proven by its Merkle path against the registered root
  (`fetch_shard_params_v1`), and verifies its cells over exactly those rows; a lying holder is refused by name. Such a seat
  does not build the accusation of a finding (its cone opens parameters through a backend it does not hold).
- **Shadow.** `--palw-tir-shard-shadow` runs the pass and files nothing.
- **Holding a shard.** `--palw-tir-shard-hold` limits the shards a node proves and answers. The executor runs a shard from the
  params of its own occurrences alone (`TirExecutor::new_cell` refuses a cell whose occurrences read an unbound instance).
- **Registrant.** `--palw-tir-shard-declare=<class>:<S_L>:<S_P>` carries the plan declaration once.
