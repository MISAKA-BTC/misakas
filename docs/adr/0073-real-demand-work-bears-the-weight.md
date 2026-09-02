# ADR-0073: Real-demand work bears the weight

**Status:** PROPOSED (2026-09-02). Decision 2 is DECIDED; its activation is gated by Decisions 1 and 3.
**Builds on:** ADR-0044 (the free-prompt lane), ADR-0066 (heartbeat out of `bits`), ADR-0068 (the LLM-primary
economy), ADR-0069 (adjudicability is the price of weight), ADR-0070 (the model tiers' step spaces),
ADR-0071/0072 (the attempt lane's price and ticket).
**Amends:** ADR-0044 Decision 4's "the receipt lane is weightless" (after Decisions 1 and 3 it is not);
ADR-0069's certification scope (a family is certified for the lane it was drilled on).

> **Progress and amendments (index reconciliation, 2026-09-02).** Phase ① (a free-prompt claim
> is tried in the same court) and Phase ③ (one unit — step leaves — and the attempt drawn by the
> chain's beacon, [ADR-0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md)) are landed and
> ship from Relaunch 5d. Phase ② (the receipt lane bears weight) is DECIDED and still gated;
> Phase ④ (share / chain position for the lane) is open. Decision 6 is amended by
> [ADR-0075](0075-certification-is-a-consensus-object.md): a weightless entrant is seated by a
> `ClassLaneCertified` object, and the certified free-prompt set is chain state (genesis ∪
> chain), not the first entry in the build's `palw_rc_fp_certified_families_v1`. Map: [`README.md`](README.md).

> **Security amendment appended (2026-09-02)** — see the last section: preconditions on Phase ④ — the single-block beacon's withholding bias is bounded (`k ≥ 3` attempt blocks), 4b's supply metric counts executors not claims, and receipt weight ramps like attempts.

## 1. The finding

MISAKA's purpose is a user running a local LLM on a prompt of their own. The chain has two lanes
that carry LLM work, and today the wrong one carries the chain:

* The **attempt lane** (algo 6) runs a *canonical* job nobody asked for — the job is a function of
  the block template and a nonce bucket (`palw_job_anchor_v1`). Every unit of chain weight
  (`safe_weight`, blue work through `bits`, the class share table, the epoch budgets) is earned
  here. It is priced in **step leaves**: `pwu_per_inference` is the class's
  `canonical_step_leaf_count` by rule (`verify_palw_genesis_v2` refuses anything else), and a block
  claims `expected_draws × leaves` by equality (`DerivedV1`).
* The **free-prompt lane** (a 0x4a commitment transaction, a seat panel, a beacon-drawn receipt
  block on algo 7) runs the *user's* job. It exists and it works end to end on the BASE-0 floor.
  And it is, today, a side channel:
  1. **Weightless as a block.** `algo_id_carries_no_chain_position(7)` is `true`; a receipt block
     buys neither level nor blue work, and the PoW arm returns `true` without a target because of
     that (`the_receipt_lanes_freedom_from_bits_rests_on_its_weightlessness`). A spent quantum adds
     `PWU_PER_QUANTUM = 10` to `safe_weight`, and that is all.
  2. **Priced in a foreign unit.** `cu = prompt × 1 + decode × 64`, `quanta = min(⌊cu/100⌋, 64)`,
     `pwu = quanta × 10`. Tokens, not leaves. A whole certified BASE-0 free-prompt job is worth
     70 pwu; one BASE-0 attempt is worth thousands. Nothing in the tree relates CU to a step leaf.
  3. **Verified by re-execution.** The FP-R6 seat arm runs `execute_free_prompt` on every pooled
     payload (up to 4, plus 2 on-disk) — a full inference per seat per payload, five seats per
     panel: up to 30 inferences to certify one. The attempt lane's seats hash (`verify_material`);
     they never execute.
  4. **Not adjudicable.** The state machine is ready — `CourtOpened` has no source filter,
     `CourtClosed { ExecutorGuilty }` slashes an FP claim's `reserved` through `void_and_slash`
     — and no party can assemble a close, for four concrete reasons (§4). The shipped challenger
     re-executes the *anchor* job for an FP claim and therefore opens a court against every honest
     one it sees.
  5. **Uncertified.** ADR-0069's certificate is minted from a drill over `job_for_anchor` +
     `execute`; `execute_free_prompt` appears in no drill. The lane the certificate is about is
     the lane nobody asked for.

The consequence is that 100 % of the chain's weight is synthetic work, and the work the network
exists to do is paid an ordinary coinbase for a block that weighs nothing. ADR-0068 named the LLM
the primary economy; this ADR names *real demand* the primary lane of that economy, and lays out
the order in which that can be made true without re-opening any of the holes the attempt lane has
already closed.

**The order is not negotiable.** Weight without adjudicability is ADR-0069's original sin
(97.8 % of weight once went to a class the code said could never be convicted). Adjudicability
without a shared price unit gives the free-prompt lane weight the attempt lane's retarget cannot
read. So: court first, price second, weight third, share last.

## 2. Decisions

**Decision 1 — Phase ①: a free-prompt claim is disputed in the same court as an attempt, and a
seat's duty is to check, not to re-run.**

1a. *One capture codec.* An FP run already computes the court-usable capture — the family
material tuple `(binding, tiles, logits_rows, generated, checkpoint_chunks)` — into
`outcome.material`, and drops it. It is persisted and broadcast under the same data-availability
obligation as an attempt's (`trace_manifest_root` / `trace_chunk_count` / `trace_retention_daa`
already sit on the FP commitment for exactly this). The job-description material `FPM1`
(`PalwFpMaterialV1 { job, prompt_token_ids }`) stays as what it is: the question, not the answer.

1b. *The FP claim's anchor is its job id.* For an attempt the panel derives the job from the
header (`palw_job_anchor_v1` over template, class, bond, nonce bucket) so the accused cannot set
the question. For an FP claim the question is set by the *user* and fixed on chain before any
verifier looks: `fp_job_id_v3(job)` is what the binding's `job_context.job_id` carries, and the
0x4a payload carries the prompt ids hash-bound to it. So `PalwClaimRootsV1.anchor` for an FP claim
is the job id, and `verify_material`'s `job_id == anchor` equality holds for the right capture
and fails for every other, exactly as on the attempt lane. `PalwCourtDutyV2` learns the lane
(`free_prompt`, as `PalwSeatDutyV2` already does) so the panel's court arm can choose.

1c. *The prompt reaches the refutation.* `PalwExecutionStepRefutationV1::prompt_token_ids` is
"irreducible" (its own doc) and the court refuses a non-empty list that does not hash to the
binding's `prompt_token_ids_hash`. Today every backend derives the list from the anchor: A16 and
Qwen3.6 fall back to empty (prefill `Embed` leaves are `Unadjudicable`), BASE-0 passes the wrong
list (every close is `InputSetNotCanonical`). The prover takes the ids as an input for an FP
claim — from `FPM1` or the accepted 0x4a payload, both hash-bound — and the attempt path is
untouched.

1d. *The challenger disputes the job that was claimed.* For an FP claim the challenger's own
execution is `execute_free_prompt(job, prompt)` from the hash-bound ids, never `job_for_anchor`.
A challenger that cannot execute the FP path (Qwen3.6 today) opens nothing.

1e. *Seats check; challengers replay.* FP-R6 becomes what the attempt lane's seat arm is:
`verify_material` over the served capture (hashing), plus **k sampled leaves** — the seat picks k
leaf indices with its own randomness, asks the prover for each (`refutation_for_index` over the
served capture, which resumes from the served checkpoint leg and replays at most one checkpoint
interval), and runs the court's own `check_execution_step_refutation_v1` locally. `NoFaultFound`
on every sample and a price that re-derives (1f) is `Valid`. A skipped inference is wrong at every
leaf and is caught by any sample; a single corrupted leaf is the court's question, and the court
is cheap (bisect to one leaf, one arithmetic refutation — ADR-0070). Full re-execution is what a
*challenger* chooses to spend to open a court, never a seat's obligation.

1f. *Certification covers the lane.* ADR-0069's certificate gains an FP drill: a family is
FP-certified only when an `execute_free_prompt` run's every leaf adjudicates and a tampered leaf
convicts under the shipped court. `palw_rc_certified_families_v1` records the lane a family is
certified for; Decision 2 reads the FP bit, and nothing else does.

**Decision 2 — Phase ②: real-demand work bears weight. DECIDED.**

The question was whether free-prompt work should carry chain weight at all. It should: it is the
work the network exists to do, and after Decision 1 it is *more* accountable than an attempt, not
less — its prompt is on chain, its capture is served, its leaves adjudicate in the same court, and
its ticket is drawn by a beacon that does not exist when the claim is fixed (nothing to grind,
cf. the ADR-0072 review).

What "self-dealing" means here, stated so it stops being an objection: an executor who submits
prompts to itself has run an inference the chain can convict, priced by the leaves it executed.
That is a canonical job by another name, and it is exactly as good as one. The only self-dealing
that matters is *shape-crafting* — choosing prompts that price above their cost — and Decision 3
removes the surface by pricing the leaves themselves.

Activation is gated, not dated: a class's FP lane bears weight when (i) Decision 1 has landed and
the class is FP-certified (1f), and (ii) Decision 3 has landed so the weight is in the unit the
retarget and the share table already read. Until both hold, a spent quantum keeps adding exactly
what it adds today.

**Decision 3 — Phase ③: one unit, the step leaf, on both lanes.**

3a. An FP claim's `pwu` is the job's **step-leaf count** — a pure function of the class profile
and `(prompt_tokens, decode_tokens_executed)`, derived the way `canonical_step_leaf_count` is for
the canonical job — and it is checked by *equality* at commitment admission, the `DerivedV1`
discipline. The binding's leaf count is the same number, so a capture whose leaves do not match
the claim is refused structurally.

3b. A quantum is a leaf count, not a CU count. *As landed (ADR-0074 Decision 5):* not a
network-wide `QUANTUM_LEAVES` but a fraction of the class's own canonical job —
`max(1, canonical_leaves / 8)` — so every class's job is eight draws and `pwu` stays in leaves. `PalwFpCuWeightsV3`, `QUANTUM_CU`, `PWU_PER_QUANTUM` are withdrawn
from the bundle (their text stays in ADR-0044 as the record). The "no shape prices above the
pure-decode reference" invariant becomes a theorem instead of a calibration: leaves are the work.

3c. The lottery discipline, stated once for both lanes: **a draw is a paid execution, drawn from a
value the executor cannot vary after paying.** The receipt lane's beacon draw satisfies it by
construction. The attempt lane's ADR-0072 draw satisfies it only when every field inside the
priced bytes is an equality against chain state, a function of the execution the panel replays,
or the position — the review of ADR-0072 found `trace_retention_daa` (and the DA trio generally)
pinned by nothing, which made one inference worth 2^64 draws. That is ADR-0072's to close; this
ADR records the invariant as the bar both lanes are held to, and notes that a beacon-drawn
attempt lane would satisfy it structurally rather than field by field.

**Decision 4 — Phase ④: the weight and the share move to the lane that does the work.**

4a. *A receipt block has a chain position.* `algo_id_carries_no_chain_position(7)` flips to
`false` at activation. Its blue work is `calc_work(bits)` like every other block's, with
`bits = compact(receipt_target[class])` — the class's receipt-lane target, which the stateful
admission already compares the quantum ticket against — so the header's declared work is checked
against state at admission (the way an attempt's `pwu` is), and the work has the same statistical
meaning as the attempt lane's: expected paid executions per block. The PoW arm stops skipping the
target comparison; the test that pinned "freedom from `bits` rests on weightlessness" is replaced
by "chain position rests on the lottery being consensus-checked at admission". ADR-0066's
predicate is honoured: a lane has weight iff its blocks are priced.

4b. *The share follows supply.* `fp_attempt_share_permille` leaves 900 on a per-class schedule
toward a receipt-lane majority, bounded by what the lane actually produces: the silent-lane
renormalisation (`apply_class_retargets`) stays as the liveness floor, so a class with no FP
supply retargets its attempt lane alone, and the attempt lane becomes what ADR-0068's floor
doctrine already says it is — the fallback that keeps the DAA moving when nobody is asking.

4c. *Payment.* A receipt block's coinbase remains the executor's payment; the worker carve stays
attempt-only. What Decision 4 changes is what the block *weighs*, not what it pays.

**Decision 5 — Rollout.** Phase ① moves the fingerprint once, through the certificate root (1f);
its node-side parts (1a–1e) ship ahead of it and are inert on an uncertified lane. Phase ③ and
Phase ④ are each one fingerprint move; ② is a bit inside ③'s. No re-genesis at any step.

## 3. What this costs, stated before it is measured

* **Seat cost per FP claim** falls from ≤ 30 inferences to hashing plus `k` checkpoint-interval
  replays. With the RC checkpoint interval and `k = 4`, a Qwen-class claim verifies in well under
  one inference per seat; the floor in milliseconds. This is what makes a receipt-lane majority
  affordable at all — at 30 inferences per certified inference the lane can never carry more than
  1/30 of the network's compute as weight.
* **Detection.** A claim with no execution behind it fails every sampled leaf; a claim with one
  corrupted leaf survives sampling with probability `1 − k/n` and is the challenger's to catch. The
  executor gains nothing from a single corrupted leaf (the work was still done) and loses
  `reserved` if caught, so the sampling rate prices the *skip*, which is the only profitable lie.
* **Court cost** is ADR-0070's: `log₂(leaves)` rungs plus one arithmetic refutation, bounded by
  the 80 KiB close ceiling — unchanged by this ADR, now reachable from the FP lane.
* **Weight.** After ④a a receipt block weighs `1/receipt_target` expected executions, like an
  attempt weighs `1/(P_class × P_bits)`. The chain's weight becomes a function of paid executions
  regardless of who asked for them — which is the sentence ADR-0072 ended on, made true of the
  lane it excluded.

## 4. Phase ① work items (the four gaps, with their closures)

| # | Gap (as found 2026-09-02) | Closure |
|---|---|---|
| G1 | The prover derives the prompt from the anchor: A16/Qwen3.6 → empty list → prefill `Embed` leaves `Unadjudicable`; BASE-0 → foreign list → `InputSetNotCanonical` on every close | `refutation_for_index` takes the FP prompt ids as an input on the FP path (hash-checked against the binding); attempt path unchanged |
| G2 | The court capture (family tuple) is computed by `execute_free_prompt` into `outcome.material` and never persisted or broadcast; only `FPM1` travels | The FP worker retains and gossips the tuple under the claim id, beside `FPM1`; the pool/retention readers accept both by magic |
| G3 | `PalwCourtDutyV2` has no lane; the court arm derives an attempt anchor and `verify_material` refuses on `job_id != anchor` | `free_prompt` on the court duty; anchor := `fp_job_id_v3(job)` from `FPM1` for an FP claim |
| G4 | The challenger re-executes the anchor job for an FP claim → opens a court against every honest FP claim, staking `reserved` | Challenger executes the FP job from the hash-bound ids; opens nothing when the backend has no FP path |
| G5 | FP-R6 seats re-execute (≤ 30 inferences per claim) | `verify_material` + `k` sampled leaves through the court's own refutation check; re-execution remains as the challenger's spend |
| G6 | No drill certifies the FP path | `certify_e2e_family_v1` accepts an FP-drill evidence set; the derived set records the lane |

Known and not in scope here: the registered A16 row refuses `execute_free_prompt` until its graph
is reconciled (ADR-0049 Decision F gap), and Qwen3.6 has no FP path; both are prerequisites for
those classes' FP lanes to be *certified*, not for the court to exist. Two stale comments to
retire when the files are touched: `palw_state_v2.rs` (the "FP worker captures no legs" note above
the `UnadjudicableCommitment` refusal) and `palw_fp_devnet_v3.rs` ("court 2400"; the value is 3000).

## 5. Invariants the tests must hold

* An FP claim built by the shipped worker on the floor class is convicted by the shipped court
  when one leaf of its capture is tampered, and every leaf of an honest one adjudicates
  `NoFaultFound` — the FP twin of ADR-0069's drill.
* A seat's FP verdict never calls `execute_free_prompt`.
* A challenger never opens a court against a claim whose capture reproduces from the claim's own
  job.
* `pwu` on an FP claim equals the capture's leaf count, and admission refuses one unit off in
  either direction (Decision 3).
* A receipt block's `bits` equals `compact(receipt_target[class])` at admission, and its blue work
  is `calc_work(bits)` (Decision 4) — with the PoW arm comparing the digest to that target.
* A class with no FP supply retargets exactly as it does today (Decision 4b, the liveness floor).

## 6. Supersession

| Decision | Status after this ADR |
|---|---|
| ADR-0044 Decision 4 — receipts are weightless, `algo_id_carries_no_chain_position(7)` | stands through Phases ①–③; flips at Phase ④ activation (Decision 4a) |
| ADR-0044 Decision 7 — CU weights price the lane | withdrawn at Phase ③ (Decision 3b); the text stays as the record |
| ADR-0066 — a lane has weight iff its blocks are priced | honoured: receipt blocks become priced before they weigh |
| ADR-0068 — the LLM-primary economy, the floor's minimum | refined: real demand is the primary lane; the attempt lane is the floor doctrine's fallback |
| ADR-0069 — a certified family may bear weight | extended: certification names the lane; the FP bit gates Decision 2 |
| ADR-0072 §3 "does not make real-demand work the primary lane" | this is the ADR it deferred to |

## Security amendment (2026-09-02) — preconditions on Phase ④ (Decision 4) and on 4b's supply metric

**SA-1 — The beacon's single-block bias is bounded before receipt blocks gain chain position.**
Today the beacon is the first attempt-class chain block at or after the slot
(`derive_beacon_fact_v3`); re-rolling costs one inference, but *withholding* costs only that
block's subsidy: a producer whose block would be the beacon can drop it when the draw is
unfavourable to its own pending claims. The quantized ticket (ADR-0044 Decision 5) bounds the gain
per bit; Decision 4 multiplies the stake on that bit by giving receipt blocks position and share.
Precondition: the beacon becomes the fold of the first `k ≥ 3` attempt blocks at or after the slot,
so a producer with attempt share `p` controls the draw with probability `pᵏ` instead of `p`; the
bound is stated here with `p` read from the live share table and pinned by a test on the fork-choice
simulator. Receipt blocks remain non-beacons (F15).

**SA-2 — 4b's supply metric counts executors, not claims.** "Bounded by what the lane produces"
must not be movable by one operator flooding `Canonical` self-claims — honest work, but self-dealt.
Count distinct executor bonds with a certified claim in the span, and cap one bond's contribution
at `1/seat_count` of the metric; otherwise the share walks toward the lane its largest operator
dominates.

**SA-3 — A receipt block's chain position is priced by its class's receipt target under the class
retarget's `max_factor` (4 per epoch), and receipt weight ramps exactly as attempts do**
(Provisional → ReceiptLicensed → Final); a receipt block whose claim voids is weightless
retroactively only inside the ramp window and never after `Final` (ADR-0039 3e).
