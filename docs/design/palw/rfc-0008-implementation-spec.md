# RFC-0008 implementation spec v0 — claim-backed work-slice blocks (DORMANT)

Status: implementation spec for lane RF8, written 2026-10-03 by the coordinating session from RFC-0008
(`docs/rfc/0008-palw-claim-backed-consensus-blocks.md`, main 533923289), ADR-0058/0069/0125/0137/0140/0141/0142/
0165/0168 and RFC-0002/0006/0007. Base: `rcore/int-12` (the DAA 5,300 candidate line) because RFC-0008 builds on
int-12's RFC-0002/0006/0007 code; this branch is **never** merged into the 5,300 release.

**Non-negotiable:** every new fence is `None` on every shipped preset (mainnet, testnet-11, testnet-12, devnet,
simnet); no activation height is chosen; no deployment. Params id / schedule id of every shipped preset must not move
(drift 0 — the `never()` collapse and Some-only id writes as for every fence). Arming is refused by `validate_*`
outside a salted drill genesis while any §11 prerequisite is open, with the open items named in the error. Nothing
unsolved is represented by a placeholder check, a `todo!()`, or a comment claiming completion: an unsolved item is
either not built or built and refused at arming, and it is listed in §11 with the reason.

## 1. Three fences (all dormant), four places each

| fence | what it arms | requires |
|---|---|---|
| `palw_work_slice_v1: Option<PalwWorkSliceParamsV1>` | algo 11, session roots, slice ledger, slice claims, weight/reward at Final | `palw_real_clock_tick_v1` armed at or below (int-12 lane RS) is NOT required; ConsensusV2 bundle |
| `palw_ws_clock_v1: Option<ForkActivation>` | algo-11 blocks become clock tick sources | `palw_work_slice_v1` and `palw_real_clock_tick_v1` at or below |
| `palw_merge_admission_v1: Option<PalwMergeAdmissionParamsV1>` | chain blocks may not merge ineligible floors (stage A) / ineligible PALW blocks (stage B) | RS's floor state machine fence at or below |

Four places for each (the repo's fence rule): the `Option` field, `for_each_fence`, Some-only writes in
`consensus_params_id` and `consensus_schedule_id`, and the `never()` collapse. Companion values (all hashed with the
fence, test values only, no shipped value): `slice_target_ccu` (W_slice), `max_slices_per_root`, `max_pending_depth`,
`max_pending_ccu_per_bond`, `max_open_roots_per_bond`, `final_deadline_daa`, `root_window_daa`, `root_fee_sompi`.

## 2. Objects

### 2.1 Session root — `PalwWorkSessionRootV1`, object tag 96 (lifecycle object, tx-carried)
Fields: `version`(1), `class_id`, `job_id` (= H(domain ‖ class_id ‖ input_commitment), derived and checked, never
free), `input_commitment`, `positions_total` (T), `plan_version`(1), `initial_state_commitment` (checked against the
derived initial boundary root where the class lets it be derived), `da_commitment`, executor bond outpoint + pubkey,
ML-DSA signature over the domain-separated root digest. `root_id = blake2b-512("misaka-palw/ws-root/v1" ‖ borsh(root
minus signature))`.

Acceptance (fold, as a lifecycle object; refusal = dropped object, block stands): class registered and admitting and a
certified, weight-bearing family (ADR-0069 D5, `palw_class_bears_weight_v2`) whose TIR profile yields a per-position
cost (§3); `job_id` not already opened on this chain (**job ledger: one root per job per history** — the Sybil/split
guard); bond active, matured, key matches; `positions_total` within the class bound; derived plan length ≤
`max_slices_per_root`; derived total CCU within the bond's exposure ceiling; open roots of the bond <
`max_open_roots_per_bond`; `root_fee_sompi` burned. **The root earns nothing: no claim, no CCU, no PWU, no reward,
no weight.**

### 2.2 Slice envelope — `PalwWorkSliceEnvelopeV1`, carried by an algo-11 block where the attempt envelope rides
Fields: `root_id`, `slice_index: u16`, `range_start: u32`, `range_end: u32`, `pred_boundary_root`,
`result_boundary_root`, `trace_root`, `output_root` (what the verifier checks), `da_root`, `proof_commitment`,
`class_id`, `job_id`, bond outpoint + pubkey, ML-DSA signature over
`H("misaka-palw/ws-slice/v1" ‖ network ‖ pre_pow ‖ timestamp ‖ nonce ‖ root_id ‖ slice_index ‖ result_boundary_root ‖
bond)` — the attempt `challenge_v2` / ADR-0125 D4 pattern, so the slice is bound to its block's parents (pre_pow).
**The slice's CCU is never carried; it is derived** (ADR-0137 §22.4: a declared pwu once bought fork-choice weight).

### 2.3 Equivocation evidence — `PalwWorkSliceEquivocationV1`, object tag 97
Two envelopes, same `(root_id, slice_index)`, same bond, different `pre_pow`, both validly signed, **while neither
block was on the other's selected chain at signing** — define precisely; a re-mine after the first block is no longer
on the chain the producer follows is NOT equivocation (honest reorg). If no rule distinguishes these from block data
alone, do not slash: record the evidence only and list it in §11 (do not invent a heuristic). Tags 98–99 reserved.

**As built (S3): the evidence type is validated by the pure core (`PalwWsEquivocationV1::validate_v1`, canonical id), but no
tag-97 object is carried — the fold neither records nor acts on evidence.** No rule separates equivocation from an honest
re-mine, so a record would be state nobody reads; the tag stays reserved and the gap stays listed (§11 item 1).

## 3. Deterministic slice plan (pure, `consensus/core/src/palw_work_slice_v1.rs`)
`palw_ws_plan_v1(cost, positions_total, slice_target_ccu, align_g) -> Result<Vec<PalwSliceRangeV1{start,end,ccu}>, _>`:
walk positions `0..T`, accumulating the **structural per-position CCU** of RFC-0002 §5.3 (at that position's history
length; PALW-WK-3: independent of commit points and `tile_len`); cut at the first `G`-aligned position where the
accumulator ≥ `slice_target_ccu` (`G = lcm(C, h_tile)`, RFC-0006 §1.2, from the class's admission profile); the last
slice takes the remainder (≥ 1 position). Ranges partition `[0, T)`. Find the existing per-position cost / work vector
in the TIR crates first; only if none exists, write the pure function from RFC-0002 §5.3 with tests. A class whose
profile gives no per-position cost cannot open a root (refusal, named).

## 4. Ledger, conservation, outstanding bounds (fold; delta 107 sessions, 108 job ledger; carriage tail 0xE8)
`PalwWorkSessionV1 { root_id, class_id, job_id, bond, plan (or its hash + n), next_index, last_result_root,
final_ccu, pending: [PalwPendingSliceV1 { index, ccu, block, accepted_daa, deadline_daa, stage: Pending|Licensed }],
status: Open | Complete | Voided{ at_index, reason } | Expired }`.

Slice acceptance (algo-11 block, own or merged, fold order = own first, then merged in acceptance order):
1. root exists, `Open`, inside `[accepted_daa, accepted_daa + root_window_daa]`;
2. envelope `class_id`, `job_id`, bond equal the root's;
3. `slice_index == next_index` — **one credit per `(root_id, slice_index)` per history; no skip, no duplicate**;
4. `[range_start, range_end) == plan[slice_index]` — no overlap, no gap;
5. `pred_boundary_root == last_result_root` (index 0: `initial_state_commitment`);
6. `pending.len() < max_pending_depth` — **outstanding depth bound**;
7. bond's pending CCU over all its sessions + this slice ≤ `max_pending_ccu_per_bond` and within the bond's existing
   exposure ceiling — **outstanding value bound**;
8. slice work id `H("misaka-palw/ws-work/v1" ‖ root_id ‖ slice_index)` unused (its own domain — the attempt rule
   "one inference is one claim across blocks", ST `apply_attempt`/C-1, stays untouched for attempts).
On success: `next_index += 1`, `last_result_root = result_boundary_root`, push pending with
`deadline_daa = accepted_daa + final_deadline_daa`, create the slice's claim (Provisional, §8), bounded immature as a
Provisional claim of that CCU. On failure: own slice → block **disqualified from chain** (ADR-0058: own work
disqualifies); merged slice → skipped (ADR-0058) unless the merge-admission fence (§7) says otherwise.

Verification events (driven by the slice claim's lifecycle, §8):
- Licensed(i): stage = Licensed.
- Final(i): only if every lower index is Final (**in-order finality**); `final_ccu += ccu`; pending removed; reward
  row and `safe_weight` through the SAME helper attempts use (one helper, ADR-0069), exactly once.
- Fraud(i) (court verdict) or deadline lapse of i: status `Voided{at_index: i}`; **i and every later slice of the root
  are void (dependents)**: no reward, no safe weight, immature released; fraud → the bond is slashed through the
  existing offence path; deadline lapse (e.g. panel stopped) → no slash (silence is not slashable, ADR-0064), session
  closed. Root complete when the last index is Final.

Invariants (property-tested, and debug-asserted in the fold): Σ final_ccu of a root ≤ Σ plan ccu =
canonical_job_ccu; ranges partition `[0,T)`; each `(root, index)` Final at most once per history; the root earns 0;
**slice Finals grant no ADR-0125 round permits in this fence** (the execution lane keeps drawing only from attempt
claims, so a slice's CCU is never credited twice through E blocks); a void slice pays nothing and its dependents
pay nothing.

### 4.1 The slice claim, and merged-slice entitlement (RF8 S3; the lead approved both on 2026-10-03)
**The claim** (`PalwClaimSourceV2::WorkSlice { root_id, index, ccu }`, appended last): its id is its work id
`H("misaka-palw/ws-work/v1" ‖ root_id ‖ slice_index)`; its weight `pwu` is the slice's CCU (a root whose plan has a slice above
`u64::MAX` cannot open, so the ledger's `u128` and the claim's `u64` are one number); `reserved = scale(pwu) × slash_value_per_pwu`
in the derived unit (`palw_exposure_pwu_v3` with the slice as the one draw; a root cannot open without the derived basis); the
immature weight and the escrowed carve are an attempt's; `execution_root` is the slice's `result_boundary_root`. At `Final` its
weight is added once, through the attempts' helper (`palw_claim_safe_contribution_v3`), and its escrow is paid work-priced by its
OWN work (`palw_work_priced_reward_v1(escrow, pwu, unit)`: the remainder slice earns the share of the carve its work is).
A slice claim rides the core ledgers (reservation, immature weight, safe weight, escrow, deadlines, panel, court). **It does not ride
the capacity lanes keyed on an attempt claim** (F-E escrow at licence and the E-4 obligation hold, F-L, F-W, F-S, F-Q, F-R, F-N,
F-EM, F-M1 riders, F-K), the model registry's probes and activation pools, the work target, the execution lane's round credit or the
settled-anchor clock, and a `Final` slice is not reversed by an execution-root conviction — a named open item (§11) that keeps arming
refused.

**The lifecycle hooks** are at the three funnels: the licence (`license_claim`: the session stage), the `Final`
(`finalize_claim`: the gate at its top — in order, inside the slice's deadline — and the credit at its tail) and the void
(`void_claim`, every route: the session void from this slice and every dependent's claim voided, `WorkSliceDependent`). A slice
waiting for a lower index is deferred by re-arming its deadline at the block it waited in (DL-1's `max(floor, last)` row, so a reload
re-derives the same index); a slice past its deadline lapses (`WorkSliceLapse`) instead of finalizing. There is no sweep of
sessions: the claim lifecycle's own windows bound liveness, and a session that holds nothing is retired at the event that drains it
(by the next root for an Open root whose window passed with nothing ever accepted). The job ledger keeps its row for ever.

**Merged-slice entitlement.** A chain block's coinbase can read only the PARENT state, and a template is built before the block's own
slice and objects exist. So a merged slice block is entitled — paid its worker share, its carve withheld and escrowed — iff the
ledger accepts it **in acceptance order over the parent's ledger**, each accepted slice applied before the next is asked (a mergeset
carrying slices 3 and 4 of one session entitles both), and **nothing else is seen: not the chain block's own slice, not its objects,
not the sweeps**. The coinbase (`palw_v2_unentitled_blues` → `palw_ws_merged_entitled_v1`) and the fold (the merged-slice view,
`apply_work_slice_v1`) ask the same question of the same view with the same `PalwWsAcceptV1::of_slice_v1`; the fold then also
requires the live ledger and the claim gates, which can only refuse MORE (a withheld, unclaimed carve is burned, never minted). A
claim is therefore never created for a block the coinbase did not pay.

## 5. The block kind — algo 11 `POW_ALGO_ID_PALW_WORK_SLICE_V1`
- `pow_layer0.rs`: new id + docs; new predicate `is_palw_work_slice_algo_id`. Review EVERY predicate that lists
  attempt ids (6/9) and decide per site; algo-10 semantics unchanged (test).
- Header stage (only when `palw_work_slice_v1` is active at the header's DAA; below it algo 11 is refused like any
  unknown id): envelope decode, version, range shape, index bound, signature with the carried key, DA pin shape.
  Stateless only — no state is read at the header stage.
- GHOSTDAG: `LaneColoring::Weighted` like attempts (heartbeats transparent to it; F1 same-chain applies);
  `palw_lane_blue_work_v1` gives algo 11 the attempt constant `2^20` — **never the slice CCU**; selected-parent
  eligible; counted in the DAA set exactly as attempts are. Same constructor params at the four sites (header path,
  virtual, pruning-proof build, pruning-proof validate).
- PoW: as attempts under the single lottery. **No per-slice lottery** — see §11 item 4 (p = 1 question).
- Body: carriage consistency; coinbase/subsidy rule as attempt blocks (rewards come from Final payout rows).
- Virtual: own slice → `palw_v2_check_work_slice` (decode and the stateless checks re-run from the header's own position),
  then the fold's ledger and claim gates; either failing disqualifies the block; merged → the fold on the merged-slice view
  (§4.1), refused = skipped.
- RPC: `blockKind = "WORK_SLICE"`; optional `getPalwWorkSession(root_id)`.

## 6. Clock fence `palw_ws_clock_v1` (ADR-0142 amendment; ADR-0140 §6 amendment)
Extend lane RS's `palw_clock_tick_source_v1` facts so an algo-11 block is a tick source, **header-derived** (ADR-0142
§3: the DAA score is decided at the header stage, so "authenticated slice" cannot gate it; the cursor's one tick per
slot is the safety bound — a forged slice can consume a slot exactly as a heartbeat can, never two). Cursor math
unchanged: one tick per slot; missed slots lost (skip term) — a late slice cannot fill past slots; H5 refuses a step
before its slot; a heartbeat that consumed a slot stays valid forever (no retroactive invalidation); heartbeats remain
an unconditional tick source (ADR-0140 D3). Write the ADR-0140 §6 note: the court is not on this tick's path.

**As built (S4, dormant).** `palw_clock_tick_source_with_slices_v1(facts, slice_facts, attempt_ticks, slice_ticks)` is the tick-source
function with the third source, beside ADR-0165's `PalwClockMergesetFactsV1` and `palw_clock_tick_source_v1`, which do not move (it is that
rule, byte for byte, with `slice_ticks` false, whatever the slice facts hold): an algo-11 block in the mergeset is a source beside the
heartbeat and the attempt, counted from the header's algo id alone (`PalwClockSliceFactsV1 { slices, newest_slice_ms }`), the newest source of
any kind deciding the stamp the cursor's slot is asked of, one tick however many. `PalwClockStepV1.slice_ticks` (read at
the selected parent's score, like the attempt's) reports it, and `lead_capped` reaches an algo-11 header exactly where it is a source. The
cursor math, H5, the floor, heartbeats as an unconditional source, "a heartbeat that consumed a slot stays valid for ever" (its validity reads
its own parents' cursor; no later block moves it) and "a late source cannot fill slots already lost" are untouched; the fence needs
`palw_work_slice_v1` and `palw_real_clock_tick_v1` at or below it (S2's validation). ADR-0142 §10 and ADR-0140 §6 carry the amendment notes
(the court is not on this tick's path). **Tested:** the pure rule (the old rule byte for byte for every facts shape; the newest source of any
kind; one tick however many slices; a three-source clock-safety simulation at all four fence combinations), the lead cap, the lane
classification (a slice never ticks the score by itself), and through the real pipeline on testnet-12 — a child of a slice block steps the
clock only past the fence, three sibling slice blocks in one slot are one tick, an algo-11 header past the lead cap is refused only where it is
a source. **Not tested:** a slice that the ledger CREDITS beside the clock (the two share no state), and the producer's own pacing (S7).

## 7. Merge admission fence `palw_merge_admission_v1` (RFC-0008 §7 item 2 candidate)
Colouring is header-only and stays so. Instead, **a chain block may not merge an ineligible PALW block**; the
canonical chain therefore never colours one, and the decision uses only the merging chain's own fold state.

- **Stage A (floors).** Keep, in rooted state (delta 109, tail 0xE8 entry), a ring of the floor state (RS's
  `palw_floor_step_v1` Idle/Probe/Normal) at the end of each slot for the last `merge_depth + 2` slots. Rule, for
  chain block C with the fence active at C's DAA: every BASE-0 floor F in C's mergeset (selected parent excluded) must
  have `ring[slot(F.daa)] == Idle`; otherwise C is **disqualified from chain**
  (`MergeAdmission::FloorOutsideIdle{floor, slot, state}`). C's own floor: the same at C's selected parent. The
  creation context is F's own DAA slot, so an honest floor made in Idle never becomes unmergeable later (no cascade);
  a floor whose DAA slot is outside the ring is refused. Template mirror in `pick_virtual_parents` (one function, as
  the heartbeat-width rule does: a template never builds what consensus refuses).
- **Stage B (static eligibility of attempts and slices at the creation context)**: bond active+matured at slot
  (prove bond activation/retirement delays ≥ the merge window, else keep versioned status), class admitting and
  target at `span(B.daa)` (keep the previous span's status/target), ticket valid against it, slice root open and
  index ≤ n at that slot. Build only what is creation-context deterministic; otherwise leave it unbuilt and list it.
- Amends ADR-0058 under this fence only (merged ineligible floor: skipped → disqualifies the merger). Rebase on RS's
  final floor-state commit before building stage A (RS freezes 2026-10-04 12:00 JST); stages 1–4 do not wait.

**As built (S5): stage A, dormant. Stage B is NOT built (`stage_b = true` is refused).** Written on lane RS's final floor machine
(`palw_floor_step_v1`, the rooted `floor_state`, delta 104 / tail `0xEA`, merged from `rcore/int-12`):

* **The ring** (`palw_merge_admission_v1.rs`; state `merge_ring`, delta **109** `MergeRing {old, new}`, carriage tail `0xE8`'s third
  component, one Some-only root block). One entry per DAA slot for the last `merge_depth + 2` slots (32 on testnet-12):
  `floor` — the floor state at the END of the slot, the fold running the step last so a REAL attempt accepted in the block is already
  in it, every block of the slot refreshing it — and `due_since`, the DAA at which "a claim is `Provisional` past its anchor slot, with
  the bind-deadlock fence and R-core+ in force" last turned true, **as of the slot's start** (one O(claims) scan a slot, run by the first
  block of the slot on the state it started from — never a scan a block). A block of an older slot cannot occur along a chain.
* **The rule** (`palw_merge_floor_verdict_v1`, run by the chain walk's `palw_v2_check_merge_admission` before anything folds). Every
  BASE-0 floor in C's mergeset (selected parent excluded) and C itself if it is one, judged at ITS OWN slot (`F.daa`) on the selected
  parent's ring: eligible iff the ring says **Idle** there, **or the anchor duty required it** — RS's `palw_floor_anchor_duty_v1`, the
  rule the node's floor producer runs, over the ring's facts: `binder_due` = a claim waited (`due_since` set) and the floor's bond is
  one lane A takes as a binder (lane A not in force at the slot, or the bond is an operator's); `due_slots = F.daa − due_since`;
  `after_slots` = the fence's `duty_after_slots` (30, a salted drill may shorten it); the stagger from the bond's txid. A floor below
  the fence is eligible (made under the rules it was made under); one older than the ring is refused. The slot being OPEN (newer than the
  ring's newest entry) is answered by the same ring step projected on the selected parent's state, so the check and the fold cannot
  disagree. Any refusal disqualifies C from the chain (`PalwMergeRefusalV1`: `FloorOutsideIdle`, `FloorBeyondRing`); the block stays in
  the DAG and is not judged again.
* **The template mirror** is one function (`palw_merge_admission_filter_parents_v1`) in `pick_virtual_parents`, after the parents are
  picked and before the bounded-merge pass: every floor the virtual block would merge is asked the chain walk's own question over the
  selected parent's state at the DAA the virtual would carry; a parent that brings a refused floor leaves the set (the selected parent
  never does), and the question is asked again of what remains. A refused floor stays a tip, unmerged, until it is past the merge
  bound; no honest template names it.
* **Fence value and prerequisites.** `PalwMergeAdmissionFenceV1 { activation, stage_b, duty_after_slots }` (mirrored into the state
  params with `ring_slots = merge_depth + 2` by `Params::sync_palw_work_slice_v1`). Needs `palw_floor_reserve_v1` (the machine),
  `palw_anchor_at_ceiling` and `palw_rcore_plus` (the claims "waiting for an anchor" the duty reads exist only under them: without them
  no floor is ever a binder and stage A would hold every non-operator claim's binder out of the chain) at or below it. The merge
  admission entry left `PALW_WS_UNBUILT_V1` (only the verifier's seat side remains); stage B is refused by `stage_b`.
* **ADR-0058 is amended under the fence only** (a floor the chain refused disqualifies its merger instead of being skipped by it): the
  amendment note is at the end of that ADR.
* **A slice is a REAL event of the floor machine** (the S3 note, re-pointed at lane RS's final machine in the merge): an accepted slice
  claim of any class but the base steps it like an attempt block's attempt — BLUE for a block's own slice and a merged one the mergeset
  does not colour RED.

**What stage A does not do, and what stage B would need.** Colouring is still header-only: a REAL attempt or work-slice block that is
header-claimed and never accepted (no bond that matured, a class that is not admitting, a ticket that does not win, a root that is not
open, an index that is not next) still colours, and a bonded producer can still make such blocks that are statically eligible. Stage B
would refuse them before the chain colours them and needs facts the ring does not hold: (1) the bond's status at the attempt's slot
(active, matured, retiring, frozen — a per-slot bond ring, or a proof that activation and retirement delays are at least the merge
window); (2) the class's admission state and target at the attempt's span, and the ticket checked against it (a per-span class
snapshot; `class_ticket_v3` is computable from header and class facts); (3) for a slice, the session's `next_index` and window at the
slice's slot (a per-slot session ring) — the ledger's own check reads the merger's state, not the slice's creation context; (4) the
attempt's budget, room, share and exposure checks are stateful by nature and cannot move to a creation context at all, so a REAL
attempt the fold skips for those reasons still colours however stage B is built. None of it is built; none is faked.

**Tested (S5)** — pure (`palw_merge_admission_v1/tests.rs`): the ring's step (append, refresh, evict, the scan asked once a slot, an
older slot changes nothing), `due_since` continuing while a claim waits and restarting when none does, the slot lookup (exact, over a
gap, outside), the verdict over every slot state, the duty exception equal to `palw_floor_anchor_duty_v1` over every wait, stagger and
clause with the exact boundary, a RED-only stream refusing floors for at most `probe` of every `probe + cooldown` slots, the load-time
invariants. Fold (`real_work_reserve_v1/merge_ring.rs`): one entry a slot following the machine and evicting beyond the length, a second
block of a slot refreshing the floor only, nothing kept where the fold is not told the fence is in force, `due_since` from real claims
under the fences and not without them, the carriage round trip and the committed root refusing a forged ring, two branches' rings with
deltas applying and reverting exactly (a reorg both ways). Pipeline (`t12_real_share/merge_admission.rs`, testnet-12's own pipeline, lane A
with four operators and ADR-0170's window): floors made while REAL work flows are no chain block and the slow REAL attempt stays BLUE
(`a_floor_made_while_real_work_flows_is_no_chain_block_and_the_slow_real_attempt_stays_blue`, with its control — the fence off: the same floors
are chain blocks and the attempt goes RED); the anchor duty's one binder is a chain block and binds the waiting claim
(`the_anchor_duty_binder_is_a_chain_block_under_stage_a_and_binds_the_waiting_claim`), the exception opening exactly at the wait plus the
stagger (`the_duty_exception_opens_exactly_at_the_wait_plus_the_stagger`); a floor made in Idle stays mergeable after the state leaves Idle
(`a_floor_made_in_idle_stays_mergeable_after_the_state_leaves_idle`); a refused floor lighter than the sink is never named a parent
(`a_refused_floor_lighter_than_the_sink_is_never_named_a_parent`); a second node fed the same blocks, in the order they were mined and in other
orders, walks the same chain and merges none of the refused floors
(`a_second_node_walks_the_same_chain_and_merges_none_of_the_refused_floors_in_any_arrival_order`); a pruned join inside a Normal stretch decides
the floors as the archival node does (`a_pruned_join_inside_a_normal_stretch_decides_the_floors_as_an_archival_node_does`). The slice
counterpart — a slow work-slice block staying BLUE under rogue floors with stage A armed — is `slice_success::a_slow_slice_stays_blue_under_rogue_floors_when_stage_a_keeps_them_out_of_the_chain` (S8).

### 7.1 Alternative on the table — class-aware colouring ("R3", lane RS, coordinator's message of 2026-10-03)

R3: a REAL or work-slice candidate treats floor-lane peers — an attempt-lane header whose carried `class_id` is the base
class — as it treats heartbeats today (invisible to it in the k-cluster count; ADR-0105's F1 same-chain restriction
applies), so floors keep flowing and REAL/slice BLUE no longer depends on floor producers complying with A″ or stage A.
No code for R3 is on this branch; what follows is read off the code that is (`ghostdag/protocol.rs`: `LaneColoring`,
`lane_coloring`, the weighted peer walk) and off the cited ADRs.

1. **The unverified BLUE it grants.** The class is the header's own claim (the carried envelope) and colouring runs before
   stateful admission (ADR-0165 §00.2: the colouring "reads the lane, not the class and not the ledger"). So a header
   that *declares* a non-base class, or is an algo-11 header, would be floor-transparent whether or not its class is
   registered, its bond eligible, its root open or its index the next one: BLUE unless another REAL/slice-claiming header
   sits in its anticone (k = 1). What it gets is what every attempt-lane header already gets — header-stage blue work
   2^20 (never a slice's CCU), selected-parent eligibility, DAA-set membership, the clock tick under
   `palw_real_clock_tick_v1` — and nothing economic (no claim, weight or reward: the fold skips a merged one and
   disqualifies an own one). What R3 changes is the competitor set. Today floors (3.0 a slot on t12, 64 % external; P2 via
   ADR-0165) count against such a header; under R3 only other REAL/slice-claiming headers do, and those are free at the
   header stage (an ML-DSA signature under a carried key and the easy network-constant attempt target; no bond is read
   before colouring). So R3 turns "a floor made my REAL RED" into "a fake REAL made my REAL RED": it removes the
   compliance dependence and does not price the fake-REAL attack. Merge admission stage B (§7) is the only item here
   that refuses unaccepted REAL/slices before the chain colours them, and it is unbuilt (§11 item 2).
2. **Four-site reproducibility.** R3 is header-only: the class is decoded from the candidate's carried envelope and the
   rule is keyed on the candidate's own DAA score and the base-class id from `Params`, as the heartbeat rule is
   (`HeartbeatTransparency`, `lane_coloring`, F1's walk on the GHOSTDAG store). It reproduces at the header path, the
   virtual, the pruning-proof build and the pruning-proof validate if it enters through the same constructor parameter
   (`GhostdagManager::new` takes `heartbeat_transparent` that way so that no site, the pruning proof's two included,
   can forget it); cost, one envelope decode per attempt-lane peer in a mergeset. Merge admission is not a colouring rule: it is a chain-block disqualification decided at the virtual/body
   stage from rooted fold state (the floor-state ring, §7), reproducible across arrival order, IBD and pruning because
   the ring rides the carriage, and it leaves the four colouring constructors untouched.
3. **Anchor supply.** R3 keeps floors merged and accepted, so it does not create the anchor starvation; it removes it
   only if R3 *replaces* the floor exclusion (A″ in the 5,300 release; stage A here) — armed beside either, the
   starvation stays. Stage A as specified in §7 does create it: a floor made outside `Idle` is unmergeable, and P2
   measured that every live seed anchor on testnet-12 is a floor (lane A's anchor is "is, or merges, an operator's
   attempt", `palw_operator_anchor_v1`). See the §11 row "anchor supply under floor exclusion".

They answer different halves and are not exclusive: R3 removes floor-vs-REAL RED by making colouring ignore floors; merge
admission removes it by keeping ineligible blocks out of the canonical chain, and only it can refuse unaccepted
REAL/slices. A combination (R3 for floors, stage B for unaccepted PALW blocks) is not designed or evaluated here.

## 8. Verification path
Each accepted slice creates a slice claim through the existing claim lifecycle (bind → receipts → licence →
challenge → Final), the panel replaying `[range_start, range_end)` from `pred_boundary_root` to
`result_boundary_root`. First find out whether the panel/TIR code can replay a position range from a committed
boundary state (TirStepRun, RFC-0006 cells, DA of the boundary state). If yes, wire it. If not, wire the ledger to
the lifecycle hooks, keep the verifier unbuilt, and make `validate_palw_work_slice_v1` refuse arming with
"no slice verifier" — never mark a slice verified without a replay.

**As built (S6): the replay is possible, and it is a library; the seat side is not built, so arming stays refused.**

*Is a position range replayable from a committed boundary state?* Yes, with what the class runner already has: a `TirResumePointV1`
after a checkpoint position (every `Fixed` instance's value, every history's recent rows, the ids selected so far) is "everything a run
needs to continue exactly as the uninterrupted run would", `TirClassRunnerV1::replay(.., from, until, ..)` resumes from one and stops at
`until`, and the leaves it produces are, at their indices in the job's step space, the uninterrupted run's over those positions
(`a_run_resumed_at_any_checkpoint_continues_leaf_for_leaf`, on every golden program). S6 adds `replay_recording` (the resume points a replay
passes) and, in `misaka-palw-tir-exec` (`node/slice.rs`), the slice layer: `produce_slice_v1` (a producer's commitments for `[start, end)`
resumed from `from`) and `verify_slice_v1` (a verifier's replay of a slice's range from the boundary it holds, its commitments compared, the
predecessor asked BEFORE any replay).

*What a slice's roots commit* (the chain holds them opaque; the pure definitions are `palw_ws_*_root_v1` in `palw_work_slice_v1.rs`, so a
verifier written elsewhere computes the same): `pred_boundary_root` = `palw_ws_initial_boundary_root_v1` (slice 0; also the root's
`initial_state_commitment`) or the previous slice's boundary root; `trace_root` = `palw_ws_trace_root_v1` over the step-tree root of the
slice's own leaves (each leaf hash already binds the job context, the class and its coordinate); `output_root` = `palw_ws_output_root_v1`
over the ids the slice selected; `result_boundary_root` = `palw_ws_boundary_root_v1` over the canonical bytes
(`TirResumePointV1::canonical_bytes_v1`) of the state after `end − 1` — a checkpoint position, because the plan cuts at a multiple of
`G = lcm(C, h_tile)` — or, for the last slice (nothing resumes from a job's end), `palw_ws_final_boundary_root_v1` over every id the job
generated; `da_root` = `palw_ws_da_root_v1` over the same canonical bytes (zero for the last slice). A session's job context
(`palw_ws_job_context_v1`) is derived from the root and the network.

*Verdicts* (`TirSliceVerdictV1`): `Verified`, `PredBoundary` (the boundary in hand is not the committed predecessor — never replayed
from), `Trace`, `Output`, `ResultBoundary`, `DataAvailability`, `Refused` (the replay could not run: no verdict on the slice).

**Tested** (`misaka-palw-tir-exec/tests/slice.rs`, feature `node`, every golden program and corpus model at four layouts — G = 2, 6, 4,
3 — and jobs): slices cut at the alignment and produced by resuming reproduce the uninterrupted job (contiguous leaves summing to the job's,
each leaving the state the uninterrupted run recorded at the same position, the last one's boundary the job's ids); every honest slice
verifies from the state its predecessor committed; each lie is its own verdict, a verifier handed another state is told it is not the
predecessor, a range past the job is `Refused`; the resume state's canonical bytes round-trip and the decoder refuses a wrong magic, a
truncation, a trailing byte and an unholdable length. The pure commitments bind every input and keep six distinct domains
(`palw_work_slice_v1`'s suite).

**Not built, and why arming stays refused (`PALW_WS_UNBUILT_V1`, "the slice verifier's seat side"):** no seat service replays a slice
claim — the panel's duties, material pools, replay steps and receipts (`kaspad/src/palw_panel.rs`) are attempt-shaped; nothing carries a
boundary state and the prompt to a seat (no DA transport for them); and no court path adjudicates a slice (a verdict on a slice is a
statement about bytes in hand, not yet a conviction). So nothing signs a receipt for a slice, and a slice cannot be licensed: a slice's
claim times out. That is the safe direction, and it is why the lane is refused armed, not why it is correct armed.

## 9. Producer (node, dormant; explicit node flag; drill only)
`kaspad/src/palw_ws_producer.rs`: open a root; compute the session in order; at each slice boundary build an algo-11
block whose template's selected parent is the session's previous slice block when it is the virtual chain tip, sign,
submit; on reorg/orphan re-derive `next_index`/`last_result_root` from the virtual state and re-mine only the missing
slice; never sign one `(root, index)` for two parents that are both tips; respect `max_pending_depth` (hold, logged).

**As built (S7): a drill-only node service that opens nothing; the seat side stays unbuilt.** `kaspad/src/palw_ws_producer.rs`
(`PalwWsProducerService`), started by `--palw-drill-ws-session=<file.json>` — `{"artifact": <PALWTIR1 file>, "prompt": [ids],
"positions": N}`, with the node's own `--palw-producer-key`, `-bond` and `-pay-address` — refused by `validate_args` off a salted
drill genesis and again by the daemon on the chain it runs. Where the sketch above differs from what is built:

* **It opens no root.** A root is a tx-carried lifecycle object and its carrier needs a funded input the producer does not hold. It
  writes the signed root (`PalwConsensusObjectV2::WorkSessionRootV1`, borsh) to `<datadir>/palw-ws/ws-root-<id>.borsh` for
  `misaka palw submit-object` (whose dry run now summarises a root instead of dumping its 7 kB of key and signature), and starts
  mining when the chain holds it.
* **The chain's session — not the producer's memory — says which slice is next.** `ConsensusApi::palw_work_session_v1(root_id)` reads
  it from the tip state (`None` unless the fence is armed on this ruleset, the root is not held, or it has retired).
  `palw_ws_producer_step_v1` turns it into Mine / Hold / Over / NoSession, asking the ledger's own `palw_ws_check_accept_v1` about the
  slice it would build, so the producer holds back exactly what the ledger would refuse (an own slice the ledger refuses disqualifies
  its block). A reorg, or a block the chain never took, re-derives from the state. Depth, value and window holds are logged once per
  change.
* **The compute is S6's.** The blocking replay (`produce_slice_v1`) runs on a blocking thread; the end state of each slice is kept in
  memory and as a canonical-bytes file; a restart reads it or — with the files gone — replays the job from its start, and a boundary the
  chain does not hold drops memory and files together and re-derives. Tested through a restart with a real artifact (a PALWTIR1
  container written from the golden dense decoder).
* **The block is the node's own template**, re-declared algo 11, with the envelope bound to that template's pre-PoW hash, stamp and
  nonce (the digest is compared to nothing, so no nonce is ground). Not built: choosing the session's previous slice block as the
  template's selected parent — the slice's predecessor is the ledger's boundary at its selected parent, which the ledger checks.
* **A re-mine needs evidence, never a timer** (S7 amendment; the first S7 had "one block per index per 30 s", which is shorter than a
  slot and could mint a duplicate while the first block was merely unmerged). A duplicate `(root, index)` block earns nothing — one
  credit per history — **but it is an algo-11 block and colours**, so a producer that signs the index again puts a second block of
  the same lane beside the first. `palw_ws_remine_v1` (consensus-core, pure) answers from `PalwWsBlockStandingV1`, the facts
  `ConsensusApi::palw_ws_block_standing_v1` reads from this node's own reachability under the prune lock: it allows a second
  signing when the node does not hold the block; when the block is beyond the merge bound (`sink blue score − its blue score >
  merge_depth + 2`, so no chain block can merge it); when it is in the sink's past while the session still asks for the index (folded
  and not taken); when it is no tip, the virtual's past does not hold it and **two chain blocks have followed the submission**; and
  when its selected parent left the sink's selected chain (a reorg) and it is no tip. **It never allows one while the block is a
  tip** (inside the merge bound), and holds a block the virtual has merged and the chain has not yet folded. The producer keeps a
  record of each submitted block (block, the sink's blue score read after the submission) in a small file per index — a restart does
  not forget a live block — until the slice is **Final**, asks the rule only when the session still asks for that index, re-reads the
  session after an allowance before signing, and logs and counts each duplicate. Whether a re-mine is distinguishable from
  equivocation is still §11 row 1, open (the evidence rule decides when this producer signs; it does not make a producer's
  duplicates punishable). A lost record can cost one duplicate. Tested: the rule state by state (consensus-core), the book's
  restart/forget/damaged-file behaviour and the **duplicate count** over scripted chains with restarts (kaspad: an honest block is
  signed once, a lost block twice, a tip held through the merge depth, an unmerged block once per two chain blocks of evidence, a
  reorg once), and the standing facts through the real pipeline (`t12_work_slice_header`).
* **Not built, and why the fence still cannot arm:** nothing verifies a slice (§8), so a slice this producer mines is claimed, never
  licensed, and for want of a receipt times out; no transport for the boundary state and the prompt; no RPC that reads a session; no
  drill flag for the RFC-0008 fences in `PalwDrillExtraFencesV1` (a multi-lane hotspot, and arming is refused anyway).

## 10. Reporting stages
S1 core pure (plan, ledger, invariants) → S2 algo 11 + fences + header/colouring/blue work → S3 fold integration +
carriage + RPC → S4 clock fence → S5 merge admission (after RS's final commit) → S6 verification path → S7 producer →
S8 test sweep. Commit at each stage; report sha, tests run/passed/failed, §11 table changes.

**As committed** (branch `rfc8/claim-backed-blocks`): S1 `e0185909a`, S2 `6d6766285`, S3 `58d22437b`, S4 `ae88e3b8f`, S6 `d113ef885`, S7 `964347d3e`,
the `rcore/int-12` merge (lane RS's final floor machine, `9c5339f18`) `1ab7d04bb`, the S7 amendment `28717e05a` (re-mine on evidence, never on a timer),
S5 `cf24ce006`, and S8 — the commit that carries this paragraph (the success path through the real pipeline, §12.1, ADR-0169, §11 final).

## 11. RFC-0008 §7 status — final (S8): what is built, what stays open, and why the fences cannot arm

The lane keeps this table true: a row moves only with the commit that moves it. `PALW_WS_OPEN_ITEMS_V1` (consensus-core) is the machine-checked copy of the "stays open" column — its seven rows are the seven rows below — and `PALW_WS_UNBUILT_V1` is the one part that is not built at all.

| §7 item | what this branch builds (and where it is tested) | what stays open → fence cannot arm because |
|---|---|---|
| 1 precomputation & fork reuse | a slice bound to its block's position (a third party's re-sign fails: stateless check and pipeline); one credit per `(root, index)` per history (property-tested over random event sequences, a second node and a restart; through the real pipeline: three blocks signed for one index, one claim, the weight moved once); the slice work-id domain; **a producer signs an index again only on evidence from the consensus's own reachability — never on a timer, never while its block is a tip** (S7 amendment; the duplicates are counted in the tests) | a producer can precompute a deterministic job and re-sign slices for another branch; nothing in block data separates equivocation from an honest re-mine (the evidence type is validated by the pure core, but no tag-97 object is carried and nothing records or slashes on it — the amendment changes when THIS producer signs, not what is punishable); the RFC requires simulation of cheap private forks, header flooding and long-range sync — **none run** |
| 2 header-time eligibility | merge admission **stage A (floors), built (S5) and tested**: a ring of the floor state per slot in rooted state, a chain block that merges a floor the chain refused at the floor's slot is disqualified (the anchor duty's one binder and floors made in Idle stay mergeable; the decision reads only the merging chain's state: tested through IBD, three arrival orders, a pruned join and both ways of a reorg), the template mirrors it in one function; with stage A armed a slow slice stays BLUE under rogue floors | **stage B is NOT built**: header-claimed REAL attempts and work-slice blocks that are never accepted (bond not matured, class not admitting, ticket not winning, root not open, index not next) still colour, and a BONDED producer can still make statically eligible fake attempts/slices (punished only after the fact). Stage B would need a bond-status ring, a per-span class snapshot and a session ring (§7); the budget, room, share and exposure checks are stateful by nature and cannot move to a creation context at all, so an attempt the fold skips for those reasons still colours however stage B is built. Stage A withholds floors and so creates the anchor-supply row below; R3 (§7.1) is the alternative for floors and grants unverified floor-transparent BLUE to header-claimed REAL/slices |
| 3 optimistic finality | depth/value/deadline bounds; in-order finality; dependents void; reward and safe weight only at `Final`; tested pure, in the fold, and through the real pipeline (a claim is bound by the chain's own panel derivation, licensed, finalized after its challenge window and credits its session and weight once; a claim nobody licenses lapses, voids its session and slashes nothing) | challenge/court throughput for slices is unmeasured; the slice verifier is a library (replay from a committed boundary state, §8) with **no seat, no transport of the boundary state and the prompt, and no court path behind it** — so a slice is claimed and never licensed (`PALW_WS_UNBUILT_V1`); `proof_commitment` is None-only. The pipeline `Final` above is the harness signing `Valid` for the seats: the chain accepts such receipts and nothing on a real chain would produce them honestly |
| 4 economics & Sybil | job ledger (one root per job per history); Σ ≤ job CCU; root earns 0; no E-lane permits from slices; open roots per bond bounded; a slice's escrow is work-priced by its own CCU | p = 1 at slice size W (ADR-0137/0141: the lottery stops metering slices) — needs a decision and measurement (M1–M5); a slice is metered by neither the class epoch budget nor the work target; root opening is NOT maturity-gated (as an attempt's bond is not): `max_open_roots_per_bond` and the job ledger are the only Sybil bounds; the job ledger grows by one row per root for ever (priced by the fee) |
| 5 liveness | heartbeats unchanged and unconditional; clock structural (S4: an algo-11 block is a header-derived tick source, one tick a slot, lead-capped, behind `palw_ws_clock_v1`); deadline lapse voids without slash (tested pure, in the fold and through the pipeline) | producer+panel stopped together: slices back-pressure on depth, clock carried by heartbeats — needs the drill (**not run**); the clock rule is tested at the pure rule and through the pipeline, not under a slow-inference drill |
| anchor supply under floor exclusion (coordinator, 2026-10-03) | S5 answers the binding half: the anchor duty's one binder per wait is mergeable (the verdict is `palw_floor_anchor_duty_v1` over the ring's `due_since`), so a non-operator's REAL claim still binds under stage A (tested, §7) | any fence that holds floors out of the chain (A″ in the 5,300 release; stage A in §7) removes the attempt blocks that lane-A panel binding (`palw_operator_anchor_v1`: "is, or merges, an operator's attempt"), the jury seed and the round-lane seeding anchor on; P2 measured every live seed anchor on testnet-12 to be a floor. **Not answered here: the seed half.** `palw_anchor_window_v1` (ADR-0170, W = 24) is on this branch since the int-12 merge and widens where a seed anchor may stand, and is not shown to cover stage A or slice-only operation; what stage A still removes is every other floor made outside Idle, and with it any seed anchor that was one. Needs an admitted operator/attempt anchor whose supply does not depend on floors being merged, with the seed still ungrindable (the reason lane A exists) |
| slice claims outside the attempt-only lanes (RF8 S3, 2026-10-03) | the core ledgers: reservation, immature weight, `Final`'s safe weight, the escrow, deadlines, panel, court — through the attempts' own helpers (§4.1); the coinbase and the fold agree on a slice's escrow (read off the real coinbase, own and merged-BLUE) | the capacity lanes keyed on an attempt (F-E escrow at licence and E-4, F-L, F-W, F-S, F-Q, F-R, F-N, F-EM, F-M1, F-K), the model registry's probes and pools, the work target, the round credit and the settled-anchor clock do not see a slice claim, and a `Final` slice is not reversed by an execution-root conviction; where those fences are armed a slice bypasses them. Needs a per-lane decision (include, or refuse arming beside it) |

### 11.1 Why the fences cannot arm

**Mechanically.** `Params::validate_palw_work_slice_v1` refuses arming `palw_work_slice_v1`, `palw_ws_clock_v1` or `palw_merge_admission_v1` on **every** ruleset, a salted drill included, while `PALW_WS_UNBUILT_V1` is non-empty (today: the slice verifier's seat side), and on **every ruleset that is not a salted drill's** while any of the seven `PALW_WS_OPEN_ITEMS_V1` rows is open, naming each one in the error. The refusal is tested (`palw_work_slice_fences::an_unbuilt_part_refuses_arming_on_every_ruleset_drills_included_and_names_itself`, `…::past_the_unbuilt_parts_a_non_drill_ruleset_is_refused_naming_every_open_prerequisite_and_a_drill_is_not`), and so is the dormancy: every fence is `None` on every shipped preset, in no flag-day list, with no activation height, and the shipped fingerprints do not move (`…::dormant_on_every_ruleset_and_named_by_the_exhaustive_list`, `…::the_shipped_fingerprints_do_not_drift`). The producer and its flag are refused off a salted drill genesis as well. The pipeline tests arm the fences on the config they hold (below the layer that guards deployment) precisely because the node's own validation refuses them.

**Substantively — what arming would do today.**

1. *Nothing can license a slice.* No seat service replays a slice, nothing carries the boundary state and the prompt to a seat, and no court path adjudicates one (§8). A slice claim therefore times out at its bind or receipt deadline and voids its session: producers would spend inference for no credit. And the claim lifecycle does not care *how* a quorum came to sign `Valid` — the pipeline test that walks a slice to `Final` signs for the seats by hand and the chain accepts it — so the first seat that signs without replaying (or a quorum that does) credits a slice nobody verified. Arming before the seat side exists would make a slice's credit depend on seats' honesty with nothing the chain can check.
2. *The floor-vs-model problem is closed for floors only.* Stage A keeps floors that the floor state refused out of the chain; stage B (header-time eligibility of REAL attempts and slices) is not built, so a bonded producer's header-claimed, never-accepted attempt or slice still colours.
3. *Holding floors out removes the anchors lane A and the seed stand on.* Stage A answers the binding half and not the seed half (row above); `palw_anchor_window_v1` is merged but not shown to cover it.
4. *Economics are undecided and unmeasured.* p = 1 at slice size W; a slice is metered by neither the class budget nor the work target; the capacity lanes do not see a slice; the job ledger grows without bound (priced by the fee).
5. *Nothing has been drilled or simulated:* producer and panel stopped together, the challenge/court throughput for slices, cheap private forks, header flooding, long-range sync.

**Not run — plainly.** Stage B (not built). A pipeline run that crosses an activation height (every pipeline test arms its fences at DAA 0 or 1; the `Legacy` verdict is unit-tested only). A live drill (the producer against a live chain and a real artifact beyond its restart test; producer and panel stopped together). The simulations of item 1. A session root accepted through the real pipeline (opening one needs an IR class admitted through the court, which the pipeline harness does not stand up; the root's gate and the fold's open are tested at the gate and in the fold, the session is planted in the pipeline). A mutation check of S1. The reorg arm of `palw_ws_block_standing_v1` against a real competing branch.

## 12. Required tests (each named in the report; "not run" listed explicitly)
duplicate slice; boundary skip / overlap; cross-branch replay (third-party re-sign fails; producer re-mine credited
once per history; ledgers of two branches independent); old floor miner (floors made regardless of state never in a
valid chain's mergeset under stage A; slices/REAL stay BLUE on t12's own GHOSTDAG, as RS's `t12_real_share` suite
does); RED-only spam (cannot keep floors refused beyond probe/cooldown); panel stop (depth back-pressure, deadline
void without slash, clock by heartbeats); slow inference (late slice: no double tick, no retroactive heartbeat
invalidation); producer stop/resume; reorg (deltas exact both ways); pruning (T49-style carriage capture/import
mid-session → identical decisions); IBD (fresh consensus, same blocks, identical ledger); DAA double tick (slice +
heartbeat, two slices, one slot → one tick); reward/work conservation (property test over random event sequences);
algo-10 unchanged; shipped presets drift 0. Existing suites to run: consensus-core lib, kaspa-consensus lib (incl.
virtual_processor tests), kaspad lib, the t12 pipeline suites; report counts.

### 12.1 As tested (S8): every required test, by name, with its result

Generated from the final run's logs (`gen_table` in the lane's scratch; a name that is not found in exactly one line of its suite's log would be listed
as NOT FOUND, so the table cannot omit a failure). Suites: `cl` = `kaspa-consensus-core` lib, `pl` = `kaspa-consensus` lib (pipeline tests under
`pipeline::virtual_processor::tests::`), `kd` = `kaspad` lib, `it` = the consensus-core integration tests (`palw_work_slice_fences`), `tx` = `misaka-palw-tir-exec`'s `slice` test.

**Suite totals on the final binaries:** consensus-core lib: 3414 passed / 0 failed / 11 ignored; kaspa-consensus lib (pipeline): 619 passed / 0 failed / 22 ignored; kaspad lib: 555 passed / 0 failed / 0 ignored; consensus-core integration tests: 25 passed / 0 failed / 0 ignored; misaka-palw-tir-exec slice test: 3 passed / 0 failed / 0 ignored.

| required (§12) | tests (suite, name, result) |
|---|---|
| duplicate slice | `cl` `work_slice_fold_v1::every_own_slice_refusal_is_named` — **pass** (DuplicateSlice among the named refusals, none writes)<br>`cl` `palw_work_slice_v1::tests::every_acceptance_refusal_is_named_in_the_specs_order_and_none_writes` — **pass** (the ledger's rule 3)<br>`pl` `t12_real_share::slice_success::duplicates_of_a_slice_index_earn_nothing_and_the_credit_is_one_per_root_and_index` — **pass** (three blocks signed for index 0: one claim, the weight moved once, the own duplicate no chain block, the merged one skipped; duplicates counted (2)) |
| boundary skip / overlap | `cl` `palw_work_slice_v1::tests::every_acceptance_refusal_is_named_in_the_specs_order_and_none_writes` — **pass** (SkippedSlice, RangeNotPlan, PredBoundaryMismatch)<br>`cl` `palw_work_slice_v1::tests::the_plan_cuts_at_the_first_aligned_position_that_reaches_the_target` — **pass** (the plan: no gap, no overlap)<br>`cl` `palw_work_slice_v1::tests::the_plan_equals_the_walk_on_random_costs` — **pass** (the plan against an independent walk)<br>`pl` `t12_real_share::slice_success::an_own_slice_against_an_open_session_is_a_chain_block_and_claims_once` — **pass** (through the pipeline: a skip-ahead and another bond's slice are no chain block, the next index is)<br>`tx` `slices_reproduce_the_uninterrupted_job_and_verify_by_replay_on_every_program` — **pass** (boundary states chain: slices reproduce the uninterrupted job) |
| cross-branch replay | `cl` `palw_work_slice_v1::tests::two_branches_hold_independent_ledgers_and_a_reaccepted_slice_is_credited_once_in_each` — **pass** (ledgers of two branches independent)<br>`cl` `work_slice_fold_v1::two_branches_credit_one_index_each_and_their_ledgers_are_independent` — **pass** (in the fold)<br>`cl` `palw_work_slice_v1::tests::the_stateless_check_recomputes_the_challenge_from_the_block_position` — **pass** (third-party re-sign / re-mount fails)<br>`cl` `palw_work_slice_v1::tests::a_slice_signature_is_checked_over_its_id_in_its_own_context` — **pass** (third-party re-sign fails)<br>`pl` `t12_work_slice_header::past_its_fence_the_carriage_is_checked_by_name_before_any_pow_or_state` — **pass** (a challenge another position derived is refused at the header)<br>`pl` `t12_real_share::slice_success::duplicates_of_a_slice_index_earn_nothing_and_the_credit_is_one_per_root_and_index` — **pass** (a producer's re-mined/equivocating sibling is credited once per history)<br>`kd` `palw_ws_producer::tests::a_producer_signs_an_index_once_while_its_block_may_still_count_and_the_duplicates_are_counted` — **pass** (producer re-mine: once per index while the block may count) |
| old floor miner (floors never in a valid chain's mergeset under stage A; REAL/slice blocks stay BLUE) | `pl` `t12_real_share::merge_admission::a_floor_made_while_real_work_flows_is_no_chain_block_and_the_slow_real_attempt_stays_blue` — **pass** (rogue floors in Normal are disqualified, never merged; the slow REAL attempt stays BLUE)<br>`pl` `t12_real_share::merge_admission::control_without_the_fence_the_same_floors_are_chain_blocks_and_the_slow_real_attempt_goes_red` — **pass** (the control: fence off, the same floors are chain blocks and the attempt goes RED)<br>`pl` `t12_real_share::slice_success::a_slow_slice_stays_blue_under_rogue_floors_when_stage_a_keeps_them_out_of_the_chain` — **pass** (the same for a slice block)<br>`pl` `t12_real_share::merge_admission::a_refused_floor_lighter_than_the_sink_is_never_named_a_parent` — **pass** (the template mirror)<br>`pl` `t12_real_share::merge_admission::a_floor_made_in_idle_stays_mergeable_after_the_state_leaves_idle` — **pass** (a floor made in Idle is not refused later)<br>`pl` `t12_real_share::merge_admission::the_anchor_duty_binder_is_a_chain_block_under_stage_a_and_binds_the_waiting_claim` — **pass** (a duty floor is merged and binds the waiting claim)<br>`pl` `t12_real_share::merge_admission::the_duty_exception_opens_exactly_at_the_wait_plus_the_stagger` — **pass** (the duty boundary) |
| RED-only spam | `cl` `palw_merge_admission_v1::tests::a_red_only_stream_refuses_floors_for_at_most_the_probe_of_every_probe_plus_cooldown` — **pass** (pure (RS's machine over the ring); not run through the pipeline) |
| panel stop (depth back-pressure; deadline void without slash; clock by heartbeats) | `cl` `palw_work_slice_v1::tests::the_outstanding_depth_bound_holds_back_a_producer_the_panel_does_not_keep_up_with` — **pass** (depth back-pressure)<br>`cl` `palw_work_slice_v1::tests::a_deadline_lapse_voids_without_a_slash_and_a_late_final_is_refused` — **pass** (deadline void without slash)<br>`cl` `work_slice_fold_v1::a_slice_past_its_deadline_lapses_instead_of_finalizing` — **pass** (in the fold)<br>`pl` `t12_real_share::slice_success::a_slice_nobody_licenses_lapses_the_session_voids_with_it_and_nothing_is_slashed` — **pass** (through the pipeline: no panel, the claim voids, the session voids, nothing slashed or reserved, a later slice refused)<br>`pl` `t12_work_slice_clock::a_work_slice_block_is_a_tick_source_only_past_its_clock_fence` — **pass** (clock carried by heartbeats; slices tick only past their fence) |
| slow inference (late slice: no double tick, no retroactive heartbeat invalidation) | `pl` `t12_work_slice_clock::a_slice_in_a_slot_a_heartbeat_consumed_neither_unseats_it_nor_ticks_again` — **pass** (a slice in a slot a heartbeat consumed: neither unseated nor a second tick)<br>`pl` `t12_work_slice_clock::a_work_slice_block_past_the_lead_cap_is_refused_only_where_it_is_a_tick_source` — **pass** (the lead cap)<br>`pl` `t12_real_share::slice_success::a_slow_slice_stays_blue_under_rogue_floors_when_stage_a_keeps_them_out_of_the_chain` — **pass** (a 340 s slice lands BLUE)<br>`pl` `t12_real_share::slice_success::a_merged_slice_is_credited_and_escrows_the_merged_blocks_carve_and_its_merger_is_a_valid_chain_block` — **pass** (a late slice merged RED is still credited) |
| producer stop/resume | `kd` `palw_ws_producer::tests::a_restarted_producer_commits_the_same_slices_from_its_state_files_or_the_jobs_start_and_never_from_a_stale_boundary` — **pass** (real artifact, restart from state files / job start / stale boundary)<br>`kd` `palw_ws_producer::tests::the_record_of_a_submitted_block_survives_a_restart_and_goes_when_its_slice_is_final` — **pass** (the submitted-block record across a restart)<br>`kd` `palw_ws_producer::tests::the_producer_mines_what_the_ledger_takes_and_holds_what_it_would_refuse` — **pass** (the decision, state by state)<br>`cl` `palw_work_slice_v1::tests::a_producer_re_mines_only_on_evidence_never_on_a_timer_and_never_while_the_block_is_a_tip` — **pass** (the re-mine rule)<br>`pl` `t12_work_slice_header::the_standing_a_producer_reads_of_its_block_is_the_consensus_own_reachability` — **pass** (the facts the rule reads, through the real pipeline)<br>`tx` `a_resume_states_canonical_bytes_round_trip_and_the_decoder_is_strict` — **pass** (the resume state round-trips) |
| reorg (deltas exact both ways) | `cl` `work_slice_fold_v1::two_branches_credit_one_index_each_and_their_ledgers_are_independent` — **pass** (every fold test re-applies and reverts each block's delta)<br>`cl` `real_work_reserve_v1::merge_ring::two_branches_hold_different_rings_and_their_deltas_apply_and_revert_exactly` — **pass** (the ring's deltas, both ways)<br>`pl` `t12_real_share::slice_success::duplicates_of_a_slice_index_earn_nothing_and_the_credit_is_one_per_root_and_index` — **pass** (a sibling's hash decides the sink: the winner is either, the ledger follows) |
| pruning (carriage capture/import mid-session -> identical decisions) | `cl` `work_slice_fold_v1::a_session_survives_a_carriage_round_trip_mid_flight_and_a_second_node_folding_the_same_blocks_agrees` — **pass** (carriage round trip mid-session)<br>`cl` `work_slice_fold_v1::a_state_without_sessions_carries_no_tail_and_one_with_a_session_carries_it_last` — **pass** (the tail)<br>`cl` `real_work_reserve_v1::merge_ring::a_kept_ring_rides_the_carriage_and_reloads_under_its_root` — **pass** (the ring in the carriage)<br>`pl` `t12_real_share::merge_admission::a_pruned_join_inside_a_normal_stretch_decides_the_floors_as_an_archival_node_does` — **pass** (a pruned join decides the floors as the archival node does) |
| IBD (fresh consensus, same blocks, identical ledger) | `pl` `t12_real_share::slice_success::a_second_node_fed_the_same_blocks_holds_the_same_ledger` — **pass** (a second node holds the same PALW state root, session and claims)<br>`pl` `t12_real_share::merge_admission::a_second_node_walks_the_same_chain_and_merges_none_of_the_refused_floors_in_any_arrival_order` — **pass** (same chain, state roots and ring in the mined order and two shuffled orders) |
| DAA double tick (slice + heartbeat, two slices, one slot -> one tick) | `pl` `t12_work_slice_clock::a_burst_of_work_slices_in_one_slot_is_one_tick` — **pass** (two slices, one slot)<br>`pl` `t12_work_slice_clock::a_slice_in_a_slot_a_heartbeat_consumed_neither_unseats_it_nor_ticks_again` — **pass** (slice + heartbeat) |
| reward/work conservation (property tests over random event sequences; the coinbase side) | `cl` `palw_work_slice_v1::tests::conservation_holds_over_random_event_sequences` — **pass** (pure ledger, random events)<br>`cl` `palw_work_slice_v1::tests::a_producer_that_is_convicted_at_a_random_slice_keeps_only_what_finalized_before_it` — **pass** (pure, random conviction)<br>`cl` `work_slice_fold_v1::work_is_credited_once_over_random_histories_and_a_second_node_and_a_restart_agree` — **pass** (fold, random histories, second node, restart)<br>`cl` `work_slice_fold_v1::a_slice_and_an_attempt_credit_the_shared_ledgers_additively` — **pass** (slice and attempt share the ledgers additively)<br>`pl` `t12_real_share::slice_success::an_own_slice_against_an_open_session_is_a_chain_block_and_claims_once` — **pass** (the next coinbase pays the miner the worker base less exactly the claim's escrow)<br>`pl` `t12_real_share::slice_success::a_merged_blue_slice_is_paid_its_worker_base_less_exactly_the_escrow_its_claim_records` — **pass** (the same for a merged BLUE slice)<br>`pl` `t12_real_share::slice_success::a_slice_claim_walks_the_lifecycle_to_final_and_credits_its_session_once` — **pass** (Final credits the session and the weight once, however long the chain then runs) |
| algo-10 unchanged | `it` `algo_11_is_the_slice_lane_and_algo_10_and_every_older_id_is_where_it_was` — **pass** |
| shipped presets drift 0 (every fence None, no height, fingerprints pinned) | `it` `the_shipped_fingerprints_do_not_drift` — **pass**<br>`it` `dormant_on_every_ruleset_and_named_by_the_exhaustive_list` — **pass**<br>`it` `an_unbuilt_part_refuses_arming_on_every_ruleset_drills_included_and_names_itself` — **pass** (arming refused: PALW_WS_UNBUILT_V1)<br>`it` `past_the_unbuilt_parts_a_non_drill_ruleset_is_refused_naming_every_open_prerequisite_and_a_drill_is_not` — **pass** (arming refused: PALW_WS_OPEN_ITEMS_V1) |

**NOT RUN — plainly.** (1) **Stage B** — not built (§7): nothing runs because nothing exists. (2) **A pipeline run that crosses an activation height** — every pipeline test arms its fences at DAA 0 or 1, so no test folds a chain across the height at which a fence turns on; the `Legacy` verdict (a floor made before the fence is eligible) and the ring's start are unit-tested only. (3) **A live drill** — the producer against a live chain with a real artifact beyond its restart test, and producer and panel stopped together (RFC §7 item 5); nothing here has run on a network.

Also not run, and why: the cheap-private-fork, header-flooding and long-range-sync simulations the RFC asks for (§7 item 1); a slice's `Final` WITH A VERIFIER — nothing replays a slice (`PALW_WS_UNBUILT_V1`), so the `Final` that `a_slice_claim_walks_the_lifecycle_to_final…` reaches is the harness signing `Valid` for the seats (the chain accepts such receipts; on a real chain nothing would produce them honestly); a session root accepted through the real pipeline (opening one needs an IR class admitted through the court, which the pipeline harness does not stand up — the root's gate and the fold's open are tested at the gate and in the fold, the session is planted in the pipeline); the reorg arm of `palw_ws_block_standing_v1` against a real competing branch (the harness builds none; the rule's reorg arm is unit-tested and the read is one `try_is_chain_ancestor_of`); RED-only spam through the pipeline (pure only); a mutation check of S1.

**Known flakes, not this lane's.** Two pre-existing tests in the kaspa-consensus lib are probabilistic or timing-sensitive and are listed so that a red run is not read as this lane's: `hb_fork_choice_probe::hb_regression_below_the_fence_the_armed_build_is_the_launched_build` (about 7 %, P2's investigation) and `t12_seat_maturity_fence::t12_a_bond_registered_after_launch_is_drawable_below_the_fence_and_waits_its_window_past_it` (its assertion that the released rule seats the newcomer in at least one of a handful of hash-drawn panels fails when none happens to; seen once in five full runs of the merged tree — one of two runs that shared a machine — and 8 of 8 in isolation). Neither failed in the clean final run above.
