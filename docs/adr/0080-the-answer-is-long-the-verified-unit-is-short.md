# ADR-0080: The answer is long; the verified unit is short

**Status: SUPERSEDED IN PART by [ADR-0082](0082-the-close-is-flat-in-the-context.md), 2026-09-03,
later the same day.** §3 — one job as N verification segments — is withdrawn in full and is not to
be implemented; the refutation below is the reason and it stands. Three things here survive and are
what ADR-0082 is built on: §1's measurement (the 512 residue is the context's WIDTH, not the
history's length), Decision 3's economic invariant (reward, ticket, weight and quanta are functions
of a job's leaves and of nothing a producer can restructure — executable as
`palw_economic_locus_v1`, which narrowed it to "inside the quanta band"), and "design A" as landed
on `palw-testnet-5f` (the close ceiling as a chunk-group count; its transport half — a court close
riding its OWN chunk-group table at `(session_id, side)` — is ADR-0080 W5, which ADR-0082 depends
on and does not re-decide). The mechanism that replaces §3 is ADR-0082 Decisions 1–3: attention as
one fused node refuted by a k-ary dissection over the history, so nothing context-wide is committed
or carried.

**Status: REFUTED IN PART, 2026-09-03, before any of it was implemented.** §1's measurement stands
and is the best statement of the 512 wall this project has; Decision 3's economic invariant stands
and is worth keeping whatever replaces this. The MECHANISM — one answer as N claims — does not
survive three findings, each verified against the code by a second reader and then by this ADR's
own author:

* **A derivation cannot name a multi-claim answer, and that is fatal to the acceptance condition.**
  `PalwDerivedArtifactV1` carries a singular `claim_id` and a singular `output_root`, whose doc
  comment is "MUST equal the claim's committed `output_root` — a cross-check, not a second source"
  (`palw_derived_v1.rs:107-124`), and the transition enforces it. And `output_commitment_v2`
  (`palw_v2.rs:890`) is a FLAT keyed hash — `job_context_hash ‖ ids ‖ rendered_hash` under one
  domain key — so no id can be opened from it and no concatenation relation between two claims'
  output roots is checkable, because each is keyed to a DIFFERENT `job_context_hash`. "The answer
  is the concatenation of N segments" is therefore not a statement this chain can verify, and
  ADR-0078's leg does not survive segmentation as drafted.
* **`fp_work_id_v1` refuses segmented claims outright.** It is keyed on `(class_id,
  prompt_token_ids_hash, decode_tokens_executed, executor_bond)` with no state and no parent
  (`palw_freeprompt_v3.rs:286-299`), so the second segment of one prompt is `DuplicateWork`.
* **The per-CLAIM costs make it more expensive than the carrier it exists to respect.** The panel is
  five seats and three receipts per claim, the epoch budget is per block, `PALW_DERIVED_MAX_PER_CLAIM`
  is four per claim. Measured rather than estimated (`palw_economic_locus_v1`): `PalwSeatReceiptV2`
  is **4,772** bytes on the wire and a `ReceiptLicensed` carrying three is **14,385**, so a
  37-segment answer is **513,597 bytes of signatures** (513 kB decimal, 501 KiB — this ADR's
  original "about 514 KB" was decimal-right and KiB-wrong) inside **532,245 bytes** of consensus
  objects, more than six times `DEFAULT_MAX_CLOSE_BYTES`. And that is a FLOOR: it counts receipt
  carriage only, excluding the N commitment transactions, N `PanelBound` objects, N full-size court
  closes, N exposure reservations and 740 seat interval replays against 20 for the same answer as
  one claim.
* **Decision 3's invariance holds only inside a BAND, and both edges are reachable at shipped
  constants.** This is the correction that matters most, because the ADR asserted the invariance
  without qualification. `fp_quanta_v3` FLOORS — a segment under one quantum prices at zero and
  `apply_free_prompt` refuses it by name (`ZeroQuanta`) — and SATURATES at
  `MAX_QUANTA_PER_RECEIPT = 64`. Measured: **51,200 leaves as ONE claim weighs 12,800 pwu; the same
  leaves as FOUR claims weigh 51,200** — a 4× weight multiplier, at the shipped cap rather than at
  a pathological parameter. That is precisely the free money Decision 3 was written to forbid,
  produced by the very restructuring it was written to license.
* **And the exposure sentence above was imprecise in a way worth naming.** "Reserves a flat
  `pwu_per_inference` per claim" is exact for the ATTEMPT lane (`palw_exposure_pwu_v1` takes no leaf
  argument), and FALSE for the FREE-PROMPT lane — the lane a long answer actually travels — where
  `apply_free_prompt` reserves `quanta × quantum × slash_value`, which DOES scale with the work,
  until the cap saturates it and the flat reading becomes true again.
* **A per-claim queue drained per block, sized by the premise a segmenting design negates.**
  `PALW_V2_MAX_PAYOUTS_PER_BLOCK = 8` is justified in the source verbatim, twice: "Eight against at
  most one new claim per block: a backlog drains eight times faster than it can be created, so this
  bounds latency, not throughput." `apply_attempt` relies on the same premise. N claims per answer
  enqueue N rows against a fixed 8 per block, dividing the safety factor by N; at N ≥ 8 the stated
  property inverts.
* **The cost of the parent field is a re-mint, not a fence.** A `parent` in `PalwJobContextV2`
  changes `context_hash`'s preimage, which carries `PALW_TRACE_COMMITMENT_VERSION_V2`, so every
  class id, every trace root and every checkpoint leaf moves. That is a new trace-commitment
  version.

**What this ADR is now for:** §1 is the motivation any successor needs, and Decision 3's invariant
is the requirement any successor must satisfy. Do not implement §3 as written. The measurement this
ADR should have asked for FIRST is not latency — it is whether a derivation can name a multi-claim
answer at all, because if it cannot, segmentation buys long output at the price of the artifact leg,
which is half of what it was for.

**Original status:** PROPOSED (2026-09-03). Written against a measurement that closed a door, and against the
observation that the door was the wrong one to be pushing on. ADR-0077 Decision 13 planned a context
ladder of 512 → 2,048 → 8,192 positions per family, each rung a registered class. The first rung is
not admissible and no compression reaches it (§1). This ADR does not make 512 fit. It removes the
assumption that made 512 the question: that **one inference is one verification unit**. A long
answer becomes a chain of short verified segments, and the three numbers that were one number —
what the user gets, what one court close covers, and how much input a class admits — become three.
**Builds on:** ADR-0077 (R0: one inference is one answer and one claim; R1: the artifact to the
user, the receipt to the chain; Decision 8: a seat verifies one interval, sampled from the beacon),
ADR-0026 (borrow Ambient's architecture, refuse its tolerant proof model), ADR-0028 (sampling
decides nothing; only the court convicts), ADR-0030 §3 (the checkpoint leg and its state chunk map),
ADR-0044 F4–F6 (the beacon, the panel and the ticket), ADR-0072 (the ticket is the execution),
ADR-0074 Decision 5 (a quantum is a fraction of the canonical job's LEAVES), ADR-0075 (a class is
seated by `ClassLaneCertified`), ADR-0078 (what was made from the answer is committed, never
carried).
**Amends:** ADR-0077 Decision 13 — the ladder's rungs are re-interpreted (§7); the widths it names
were sized against a unit this ADR dissolves.

## 1. What was measured, and why it is not a compression problem

The carrier is `DEFAULT_MAX_CLOSE_BYTES = 80 KiB`, and it is not a preference: a court close is a
transaction, so the bound is the mempool's standard-transaction mass mirrored (ADR-0049 Decision C,
`palw_class_admission_v2::PALW_RC_COURT_MAX_CLOSE_BYTES`). Against it, measured on the dense class
`Qwen/Qwen2.5-1.5B/graph-v2`:

| `n_ctx` | worst close | verdict |
|---|---|---|
| 30 (fence armed) | inside | the widest row the carrier admits |
| 21 (fence not armed) | inside | the widest row without Decision 11 + 12 |
| 31 | 83,901 | over |
| 512 | 1,154,673 | 14× over |

and on the hybrid `Qwen3.6-35B-A3B/graph-v3` at 512: `attn[15]` ATTN_VALUES 2,240,241 and
`attn[12]` ATTN_SCORES 2,193,249 — 27× over.

**The decisive measurement is not the size; it is where the residue lives.** With every anchor byte
priced at ZERO and the history collapsed to ONE position, the dense 512 row still closes at
**82,080 bytes**, and the binding node is `attn[23] ffn_down` — a node whose only input reference is
`Step(22)` and which reads no history at all. `palw_context_ladder::tests::what_still_refuses_the_hybrid_512_row`
asserts the same thing permanently for the hybrid, in its own words: *"the residue is the CONTEXT's
width rather than the interval's length"*.

So the 512 close is made of two things, and neither is compressible:

* one node's **fixed operand width** — the SwiGLU down projection reduces over `ffn_dim` (8,960
  lanes on this family), a property of the MODEL, not of how many tokens the job covers;
* the **prompt-id term, `n_ctx × 4`, which rides EVERY node** (`derive_court_cost_v1`: the ids are
  checked against `prompt_token_ids_hash` before one is read, so a challenger may carry them on any
  close).

Only the second is linear in the context. That is the whole finding: a better map, a finer tile, a
cheaper anchor — every mitigation this project has built or contemplated attacks the first term or
the history, and the row is refused by a node that touches neither. **512 was never going to fit,
and the reason is arithmetic rather than engineering.**

The complement is what makes this ADR possible: the linear term is linear, so it goes away when the
unit is short. At `n_ctx` 30 the prompt-id term is 120 bytes per node against 2,048 at 512, and the
same binding node closes inside the carrier. The carrier already admits the unit this ADR wants.

## 2. The requirement

> **R2 — the length of the answer, the width of one verified unit, and the amount of input a class
> admits are THREE numbers, and the ruleset names them separately.** A person asking for four
> thousand tokens gets four thousand tokens. A court close covers one segment of about thirty
> positions. What a class will accept as input is its own bound. Today all three are `n_ctx`, which
> is why raising the third to make the first usable broke the second.

The shape, and the asymmetry it buys — generation costs what it costs, verification does not:

```text
                 the person
                      │  "write me a piece of music"
                      ▼
             ┌──────────────────┐
             │   the runtime    │   ONE inference, 4,000 tokens
             └────────┬─────────┘
                      │
        ┌─────────────┴─────────────┐
        ▼                           ▼
   the answer                one canonical job
   (4,000 tokens,            ├── segment 0    ≤ w
    kept by the user,        ├── segment 1    ≤ w      each binds the previous
    never on the chain)      ├── …            ≤ w      segment's committed state
                             └── segment N    ≤ w
                                      │
                                      ▼
                            one reward, one ticket,
                            one weight — priced in
                            LEAVES, not in segments
```

This is Ambient's asymmetry (generation is whole, verification is sampled) with the proof model this
lineage refuses still refused, exactly as ADR-0026 requires and ADR-0077 Decision 8 already applies
one level down: comparison is **exact** inside a pinned integer class, a seat's verdict **convicts
nobody**, and guilt is reached only by the court's bisection to one leaf.

## 3. Decisions

**Decision 1 — a canonical job is a CHAIN of verification segments.** The job stays the unit a
person asks for, a claim is made of, and the chain pays; the SEGMENT becomes the unit a close covers
and a seat replays. A job declares its segment width `w` and its segment count `N`; the executor
runs one inference and commits `N` segment roots plus the job root.

**Decision 2 — a segment binds its predecessor's committed state, or it is not part of the job.**
Segment `k + 1` opens the checkpoint chunk that segment `k` committed as its end state. This is not
new machinery: the checkpoint leg's state chunks ARE that state (`palw_state_chunk_map`,
`palw_step_leg::PalwCheckpointLeafV2`), and `verify_kv_anchor` already authenticates a carried
checkpoint against the binding. What is new is the REQUIREMENT that consecutive segments agree.
Without it the segments of one job can be computed in parallel from fabricated intermediate states,
and "one inference" is `N` unrelated inferences wearing one job id — which is the same class of
forgery `job_for_anchor` refuses on the attempt lane, restated in time instead of in input.

**Decision 3 — reward, ticket, weight and quanta are invariant under segmentation.** They are priced
in LEAVES (ADR-0074 Decision 5: a quantum is `pwu_per_inference / 8` leaves), and a job's leaves do
not change when it is cut into segments. So splitting is weight-neutral by construction. Any
quantity that were per-CLAIM or per-SEGMENT would make splitting free money and would break
ADR-0072's "the ticket is the execution": a producer would cut a job into a thousand pieces and
enter the lottery a thousand times for one inference. **This is the invariant to write before a line
is implemented**, because it is the one a well-meaning optimisation quietly violates.

**Decision 4 — a seat samples across segments as well as within one.** ADR-0077 Decision 8 draws `k`
checkpoint intervals from the beacon and the seat index. This extends the same draw one level up:
the seat draws segments too, so the bytes it fetches are bounded independent of `N` as well as of
the job's length. The draw stays a pure function of chain facts (`palw_fp_interval_draw_v1`'s shape,
with the segment count taken from CHAIN data — the job's declared `N` on the accepted payload — never
from a count the executor reports at request time, for the reason that module already states: an
executor that could shrink the count could predict the draw).

**Decision 5 — the anchor must be tile-addressed, or segmentation defers the linear term instead of
removing it.** This is the condition on which the whole design turns, and it is stated as a decision
rather than an assumption because it is currently FALSE on the shipped path.
`verify_kv_anchor` requires every chunk of the map to be carried
(`geometry.chunk_count() == ops.chunks.len()`), and `integer_kv_positions_at_v1` sizes the state at
`declared_prefill + covered_decode_call` — so the anchor at segment `k` is the WHOLE history up to
`k`, and a chain of short segments would carry a growing anchor: 1,050,624 bytes on the hybrid at
position 512, charged once per history-reading reference. The fix exists and is not yet merged:
graph-v4 (`palw-adr0077-tiled-attn2`) introduces `PALW_ATTN_HISTORY_TILE_V4 = 16` and
`tiled_kv_state_chunk_map_id_v3` / `tiled_kv_state_geometry_v3`, which enumerate the cache a history
TILE at a time so an opening addresses the rows a step actually reads rather than the whole cache.
**ADR-0080 depends on that map.** A class registered under a whole-history map may not declare a
segment chain, and admission refuses it by name — because a chain whose anchors grow is a chain that
fails at exactly the length it was built for.

**Decision 6 — the three limits are separate ruleset quantities.**
`verification_segment_width` is the court's, derived from the carrier the way every other ceiling in
this lineage is derived (`derive_court_cost_v1` over the segment, refused if it does not fit — never
a chosen number); `max_input_context` is the class's, and it is what the model's rotary table and
the anchor's cost jointly allow; `max_output_tokens` is the application's and reaches consensus only
through the leaves it produced. A class states all three, and `verify_class_admission_v2` checks the
first against the carrier rather than trusting it.

**Decision 7 — the door left open, named.** Nothing here decides KV continuation ACROSS jobs
(ADR-0077 §8 keeps it out and this ADR does not take it back), the value of `w` (Decision 6 derives
it; §5 V3 pins that it is derived), or what a segment chain costs in latency. §6's first unit is the
measurement, not the implementation.

## 4. What this costs, stated before it is measured

* **Chain bytes.** `N` segment roots instead of one job root. At `w = 30` a 4,000-token answer is
  ~134 segments; at 64 bytes a root that is ~8.5 KiB inside a standard transaction, which is the
  one place this design is more expensive than the one it replaces, and it is bounded and small.
* **Seat bytes.** UNCHANGED from ADR-0077 Decision 8 and independent of `N`: `k` draws, each an
  interval opening, each `O(interval × row + log₂ leaves)`. That is the point of Decision 4.
* **Court time.** Unchanged. A dispute still bisects to one leaf inside one segment, and the segment
  is smaller than the jobs the ladder was sized for, so the ladder is not tighter than it was.
* **Executor time.** The inference is one inference; the capture is the capture ADR-0077 Decision 8
  already retains. Committing `N` roots is hashing, not re-running.
* **Latency.** UNKNOWN and it is the first thing to measure (§6 U-01). A chain of 134 serially bound
  segments has 134 state bindings on the critical path, and whether that is microseconds or
  something worse is a measurement nobody in this repository has taken.
* **Identity.** A ruleset move: `verification_segment_width` is a `PalwConsensusParamsV2` quantity,
  so it is inside `palw_ruleset_id_v2`, so it lands with a re-genesis like every rung before it.
  Mainnet ships PALW off and is untouched.

## 5. Invariants the tests must hold

```
V0   Segmentation is economically neutral: a job of L leaves cut into N segments and the same job
     as one segment produce byte-identical weight, quanta, pwu and ticket inputs, for every N.
V1   A segment whose opened start state is not its predecessor's committed end state is refused BY
     NAME, and the refusal names the two states.
V2   The bytes a seat fetches for one claim are bounded independent of the job's token length AND
     of N AND of the segment's position in the chain (this is the half Decision 5 is about: an
     anchor that grows with position violates it while every other clause passes).
V3   verification_segment_width is DERIVED from the carrier by derive_court_cost_v1 and a class
     declaring a wider one is refused; no preset carries a chosen value.
V4   ADR-0044 F4-F6 hold verbatim: the beacon, the panel and the ticket are derived exactly as
     before, pinned by the existing golden vectors. A job's segment count enters none of them.
V5   The segments of one job cannot be produced in parallel from fabricated intermediate states:
     a chain that skips or forges a binding fails V1 before any of its work is priced.
V6   A class registered under a whole-history state chunk map cannot declare a segment chain
     (Decision 5), and the refusal says which map it declared.
```

## 6. Order of work

| unit | content | done when |
|---|---|---|
| U-01 | **Measure first.** `w × N` serially bound, on the dense class: latency per binding, total wall-clock for a 4,000-token answer, and the per-claim byte totals at N = 1, 8, 134 | the three numbers exist; if latency is prohibitive this ADR is re-opened rather than implemented |
| U-02 | Decision 5's dependency: graph-v4's tile-addressed map merged, and V2 measured against it | V2 green at position 30, 512 and 4,096 |
| U-03 | Decision 3's invariant, as a test, BEFORE the feature | V0 green for N ∈ {1, 2, 8, 134} |
| U-04 | Decision 2 — the chain binding and its named refusal | V1 and V5 green |
| U-05 | Decision 6 — the three limits as separate ruleset quantities; admission derives `w` | V3 and V6 green |
| U-06 | Decision 4 — the seat's cross-segment draw | V2 green with the draw, on a job with N ≫ k |
| U-07 | a segment-chain row registered through ADR-0075's route, drilled on devnet | the drill (`scripts/misaka-palw-fp-devnet-e2e.sh`) reaches a receipt block on a chained job |

**Done when** a person asks a registered class for four thousand tokens, keeps the answer, and the
chain holds one job id whose segments were verified by seats that fetched a bounded number of bytes
each — with the reward, the ticket and the weight identical to what the same leaves would have
earned as one segment. Until U-01's latency number exists, nothing below it is worth building.

## 7. Supersession

| what | disposition |
|---|---|
| ADR-0077 Decision 13 — rows at 512, then 2,048 and 8,192 | re-interpreted: those were OUTPUT lengths described as contexts. 512 is not admissible as a context (§1) and does not need to be; it is reachable as a job length over ~17 segments of 30 |
| ADR-0077 Decision 12 — `COURT_MAX_STEP_LEAVES = 2^32` | honoured and now cheap: a short segment's step space is far under the ladder, so the ladder stops being the binding constraint it was sized to be |
| ADR-0077 Decision 12's arithmetic — "(32 + 2) × 60 = 2,040" | **wrong in the text and already right in the code**: `worst_case_duration_daa` counts MOVES (`(2 × 32 + 2) × 60 = 3,960`), and `palw_context_ladder::palw_court_turn_deadline_v1` derives 45 under the fence rather than using 60. The ADR sentence needs correcting; no constant does |
| ADR-0077 Decision 8 — a seat verifies one interval, sampled | extended by Decision 4 to sample segments as well; the within-segment draw is unchanged |
| ADR-0074 Decision 5 — the quantum is a fraction of the canonical job's leaves | load-bearing here: it is WHY Decision 3's invariance holds by construction rather than by care |
| ADR-0026 — borrow the architecture, refuse the proof model | honoured: the asymmetry is Ambient's, the exactness and the court are not |
| ADR-0078 — the artifact never rides | unchanged, and clarified: this ADR segments the INFERENCE, not the artifact. A scene's commitment tree is not needed and is not proposed |

## 8. What is deliberately not decided

* **The value of `w`.** Derived (Decision 6). The measurement says the dense carrier admits 30
  today; that is a measurement of one class under one court, not a constant to write down.
* **Long INPUT.** This ADR makes long OUTPUT verifiable. A 128k-token prompt is a different problem
  with the same shape and it is not solved here: the prompt-id term and the anchor both scale with
  the input, and Decision 5's tile addressing is necessary but not obviously sufficient. Stated
  rather than implied, because "GPT-like" would otherwise be read as covering it.
* **KV continuation across jobs** — ADR-0077 §8's list stands.
* **Whether a segment chain earns its own weight.** It earns the job's weight, undivided
  (Decision 3). Whether a very long job SHOULD earn more per leaf than a short one is an economic
  question this ADR deliberately does not open.
* **Parallel generation.** Decision 2 makes segments serially bound, which forbids computing them
  in parallel. That is a cost, taken deliberately: the alternative is a job whose pieces nobody can
  show came from one inference.

## 9. Number hygiene

This is ADR-0080; ADR-0079 is the last committed. A concurrent claimant renumbers the later writer,
per ADR-0036 Decision 5.
