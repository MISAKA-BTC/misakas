# ADR-0081: Long context — the input is a state chain

**Status: REFUTED IN PART, 2026-09-03, before any of it was implemented**, together with ADR-0080,
whose mechanism it builds on — see that ADR's status block for the three findings. Two things here
survive and one is this ADR's own error, recorded because the error is more instructive than the
proposal:

* **§1.2 stands and is the most useful thing in this document.** `prompt_token_ids_hash_v2` really
  is a flat digest (`palw_v2.rs:521`), a flat digest really cannot be opened, and the `n_ctx × 4`
  term really does ride every node. Decision 3 — make it a Merkle root — is worth doing on its own
  merits, independently of segmentation, because it lowers the close cost of EVERY long-context
  design including the ones that replace this one.
* **§1.3 IS WRONG, and it was this author's error, stated confidently.** It claims the state chain
  "already exists and is already enforced" and cites `prev_checkpoint_leaf_hash` and
  `palw_step_leg.rs:1215`. The chain exists — INSIDE ONE CLAIM. `checkpoint_genesis_prev_v2`
  derives the chain's genesis link from `job_context_hash` ALONE (`palw_step_leg.rs:627`), and
  `verify_kv_anchor` is intra-claim in every one of its checks: it requires
  `covered_decode_call == disputed_call − 1` exactly and compares roots keyed to THIS claim's
  context (`palw_step_refute.rs:3390`, `:3409`). So the machinery is built to bind a chain to one
  claim and to forbid grafting it across claims — the precise opposite of what Decision 2 needed
  from it, which made Decision 2 look far cheaper than it is.

The second bullet is the mirror of the mistake §1.3 was written to prevent. This document warns, in
that same section, that "a thing recorded as missing that the code already holds sends the next
reader to rebuild it" — and then made the opposite error, recording as held a thing that holds
something narrower. Both directions cost the same, and the check that catches both is the same one:
read what the mechanism BINDS, not just that it exists.

**Do not implement §3 as written.** Decision 3 (Merkle prompt ids) may be lifted out and pursued
alone; everything that depends on a cross-claim chain waits for a successor to ADR-0080.

**Original status:** PROPOSED (2026-09-03). ADR-0080 separated the length of an ANSWER from the width of a
verified unit: a long generation became a chain of short decode segments, each binding its
predecessor's committed state. This ADR does the symmetric thing for the PROMPT, and it is not a
copy of ADR-0080 with the arrow reversed — prefill is not decode, and the difference is where the
work is. Two things this ADR found in the tree change what it has to decide: the state chain
ADR-0080 Decision 2 asks for **already exists and is already enforced** (§1.3), and the term that
actually forbids a long prompt is **not the KV history but the prompt ids**, which are flat-hashed
and therefore carried whole (§1.2). The second is the one consensus change this ADR is really
about.
**Builds on:** ADR-0080 (the answer is long, the verified unit is short; R2's three numbers),
ADR-0077 (R1, and Decision 8's sampled interval), ADR-0030 §3 (the checkpoint leg, its state chunk
map and its `prev_checkpoint_leaf_hash` chain), ADR-0026 (borrow the architecture, refuse the proof
model), ADR-0028 (sampling decides nothing; only the court convicts), ADR-0072 (the ticket is the
execution), ADR-0074 Decision 5 (weight is leaves).
**Amends:** nothing yet. It states a change to `prompt_token_ids_hash` (Decision 3) that moves every
job id and therefore every claim id, and that is a ruleset move it does not itself arm.

## 1. What was measured

### 1.1 Prefill is not decode, and the asymmetry is the whole problem

ADR-0080's decode segmentation works because a decode step reads one position and a bounded state.
Prefill does not have that shape: at position `p` the attention layers read every prior position, so
a thirty-token "prefill segment" at position 10,000 is not thirty positions of work against a
bounded state — it is thirty positions each attending to up to 10,000 cached rows. Cutting the
prompt into segments does not make that smaller. It relocates it.

So the naive symmetry — "input segments like output segments" — does not close the budget on its
own, and this ADR says so before proposing anything, because the symmetry is the intuitive move and
it is the one that fails silently: every clause passes except the byte count.

**What DOES bound it is that the court disputes one LEAF, not one segment.** A close carries what a
single disputed step needs. `derive_court_cost_v1` prices an attention step's KV references as one
range run per position — linear in the context — and graph-v4 (`palw-adr0077-tiled-attn2`) is what
makes that per-leaf cost tile-sized instead: `PALW_ATTN_HISTORY_TILE_V4 = 16` with
`tiled_kv_state_chunk_map_id_v3` / `tiled_kv_state_geometry_v3` enumerate the cache a history tile at
a time, so an attention leaf opens the tile it reduces over rather than the history it sits after.
As in ADR-0080 Decision 5, **graph-v4 is a precondition here and not an optimisation** — more
load-bearing than there, because for a long prompt the attention arm is the whole cost.

One architectural fact worth recording because it decides which class goes first: on the hybrid
(`Qwen3.6-35B-A3B`, `full_attention_interval = 4`) three layers in four are `GatedDeltaNet`, whose
replay state is one `k_dim × v_dim` matrix per head — **the same size at position 8 and at position
8,192**. Only the fourth layer carries a growing KV cache. So the hybrid is structurally the better
long-context candidate, and the dense family is the harder one, which is the reverse of the order
every previous rung was attempted in.

### 1.2 The term that forbids a long prompt is the prompt ids, and it is flat-hashed

`derive_court_cost_v1` adds, on EVERY node of the graph:

```rust
// The prompt ids ride every refutation that addresses a gather, and a challenger may carry them
// on any close: they are checked against `prompt_token_ids_hash` before one is read, so they cost
// bytes rather than trust.
evidence = evidence.checked_add(n_ctx.checked_mul(4)?)?;
```

and the reason they are carried WHOLE is one line in `palw_v2.rs:521`:

```rust
pub fn prompt_token_ids_hash_v2(prompt_token_ids: &[u32]) -> Hash64 {
    let mut w = CanonicalWriter::new();
    w.put_u32_seq(prompt_token_ids);
    w.keyed64(PALW_V2_DOMAIN_PROMPT_TOKEN_IDS)
}
```

It is a FLAT digest over the sequence. A flat digest cannot be opened: to prove that position `i`
held id `x`, a close must carry every id, because that is the only way to recompute the hash. So the
`n_ctx × 4` term is not an accident of pricing — it is the honest price of the commitment the job
already makes, and `check_execution_step_refutation_v1` enforces exactly that
(`prompt_token_ids_hash_v2(&refutation.prompt_token_ids) != binding.job_context.prompt_token_ids_hash`).

**This is why input segmentation alone changes nothing.** Cut a 32,000-token prompt into 1,067
segments and every close still carries 128 KB of ids, because every close still has to reproduce one
flat hash. The KV history has a fix (tiles). The ids do not, until the commitment changes shape.

### 1.3 The state chain ADR-0080 asks for already exists, and is already enforced

`PalwCheckpointLeafV2` carries `prev_checkpoint_leaf_hash` (`palw_step_leg.rs:602`), it is inside the
leaf's own hash (`:620`), and the court already refuses a chain that does not link:
`palw_step_leg.rs:1215` compares `later_preimage.prev_checkpoint_leaf_hash != earlier_opening.leaf_hash`.
So ADR-0080's Decision 2 and this ADR's transition verification are **extensions of a mechanism that
is built, tested and shipped**, not new machinery. What is missing is only that the chain covers
decode calls and not prefill positions, and that nothing yet requires a JOB's segments to be
consecutive links of it.

Recording this because the opposite error has been made five times in two days in this repository:
a thing recorded as missing that the code already holds sends the next reader to rebuild it.

## 2. The requirement

> **R3 — a long prompt is a chain of bounded state transitions, and nothing a court opens grows
> with the prompt's length.** Not "the prompt is split": the model still sees one context, and the
> answer must be identical to the answer an unsegmented run would give. What is segmented is the
> VERIFICATION — the unit a seat replays and a court closes over — and the bound must hold for every
> term of the close, including the ids, or the segmentation has moved the cost rather than removed
> it.

```text
              a 32,000-token prompt                      the model sees ONE context
                        │
        ┌───────────────┴───────────────┐
        ▼                               ▼
 prefill state chain              the answer
 I0 → I1 → … → I1066                    │
   each: prev_state                     ▼
       + input tile              decode state chain (ADR-0080)
       + runtime/model            S0 → S1 → … → SN
       → next_state                     │
        └───────────────┬───────────────┘
                        ▼
                 final state root
                        │
                        ▼
              one job, one reward,
              one ticket, one weight
```

## 3. Decisions

**Decision 1 — the prompt is a chain of prefill segments, and the model's view is unchanged.** A job
declares `prefill_segment_width` and a segment count. Segment `i` consumes the input tile at its
positions and the state segment `i − 1` committed, and produces the state segment `i + 1` consumes.
This is *prefill state continuation*, not context splitting: the ids, their order and the attention
they receive are exactly those of an unsegmented run, and the answer is byte-identical. A design in
which the model saw thirty tokens at a time would be a different model, and is refused here by name.

**Decision 2 — the transition is what is committed and what is verified.** Segment `i`'s commitment
is over `(job_id, segment_index, prev_state, input_tile_root, runtime_id, model_id, next_state)`, and
a seat verifies it by recomputing `prev_state + input_tile → next_state` with the class's own
kernels and comparing EXACTLY. A `next_state` alone would let a producer splice a fabricated state
into the middle of the chain; binding the transition is what makes each link answerable. The
mechanism is the checkpoint leg's existing `prev_checkpoint_leaf_hash` chain (§1.3) extended to
prefill positions, not a new object.

**Decision 3 — `prompt_token_ids_hash` becomes a Merkle root over the ids.** This is the one change
without which nothing else in this ADR helps (§1.2). A flat digest forces every close to carry every
id; a root lets a close open the ids the disputed step actually gathers — one tile plus
`log₂(n_ctx)` path elements — and the `n_ctx × 4` term becomes logarithmic. Consequences, stated
because they are large: the hash is a field of `PalwFreePromptJobV3`, so it is inside `fp_job_id_v3`
and therefore inside every claim id; the refutation's `prompt_token_ids` becomes an opening rather
than a list; and `PalwFpMaterialV1` / the DA payload keep carrying the ids whole, because a seat
replaying a segment needs them and the DA obligation is not a close. It is a ruleset move and it
moves every id in the lane.

**Decision 4 — segment width is derived, never chosen, and the two widths are separate.**
`prefill_segment_width` and ADR-0080's decode `verification_segment_width` are two quantities: the
work per position differs (a prefill position writes a KV row every later position reads; a decode
position does not), so their carrier arithmetic differs. Both are derived by `derive_court_cost_v1`
over the segment and refused if they do not fit, exactly as ADR-0080 Decision 6 requires. The
measured decode width today is 30; the prefill width is UNKNOWN until U-02 measures it and this ADR
does not guess it.

**Decision 5 — graph-v4's tiles are an addressing primitive, not a verification protocol.** They
make a leaf's KV opening tile-sized (§1.1). They do not decide which segments are verified, they do
not bound the ids (Decision 3 does), and they do not by themselves make any context length
admissible. Naming the boundary because "we have tiles now" is exactly the sentence that would let
someone conclude the problem is solved.

**Decision 6 — no reward multiplication, on either axis.** ADR-0080 Decision 3's invariance extends
to prefill: reward, ticket, weight and quanta are priced in LEAVES and do not change when a job is
cut, on the input side or the output side. A prompt of 32,000 tokens earns what its leaves earn,
whether it is verified as one segment or a thousand. Anything per-segment would let a producer cut
one inference into a thousand lottery entries, which is ADR-0072's rule broken by arithmetic rather
than by intent.

**Decision 7 — the input has one canonical root, and the segments are not on the chain.** The
canonical job carries `input_root`, `segment_count`, `segment_width` and the final state commitment.
The segments themselves are the executor's, served on request like a capture opening (ADR-0077
Decision 8), never carried. ADR-0080's cost note applies unchanged: `N` roots is bytes, and bytes
are the thing this lineage will pay; the ids and the tiles are not.

**Decision 8 — coverage is a protocol rule, not a hope.** A seat verifying segment 537 has said
nothing about segments 0–536. Verifying one link of a chain is weaker than verifying one interval of
a single execution, because a chain has more places to hide. So coverage must be stated as a rule
with a number: seats draw their segments from the beacon and their seat index (the ADR-0077
Decision 8 draw, one level up), the draw is a pure function of chain facts so nobody can predict it
at commit time, and the ADR must name what fraction of a job's segments the panel collectively
covers per claim and what a producer's expected cost is of a single forged link. **This ADR does not
yet name that number** — U-03 measures the detection probability and the number follows from it,
because a coverage rule chosen before the measurement is a guess with a decimal point on it.

**Decision 9 — "GPT-like" is two ADRs, and neither alone.** ADR-0080 makes a long ANSWER verifiable.
This ADR makes a long PROMPT verifiable. Only both together separate an LLM's input and output
lengths from the consensus verification width, and any claim about practical length that cites one
of them is half a claim.

## 4. What this costs

* **Chain bytes.** As ADR-0080: roots, not content. `input_root` plus the per-segment roots the job
  declares.
* **Close bytes.** The point of the exercise. With Decision 3 the id term goes from `n_ctx × 4` to a
  tile plus `log₂(n_ctx)` path elements; with graph-v4 the KV term goes from `n_ctx` range runs to
  one tile. Both must land or neither helps.
* **Identity.** Decision 3 moves every job id and claim id in the free-prompt lane. A ruleset move,
  larger than ADR-0080's.
* **Executor time.** One inference, unchanged. Committing per-segment transitions is hashing.
* **Latency.** UNKNOWN, and 1,067 serially bound prefill segments is a longer chain than ADR-0080's
  134. U-02 measures it before anything is built on it.
* **Seat time.** Bounded by the draw, not by the prompt — provided Decision 8's coverage number is
  set from a measurement rather than from taste.

## 5. Invariants the tests must hold

```
Y0   Answer equivalence: a segmented prefill and an unsegmented one produce byte-identical output
     ids, roots and execution root for the same prompt. The model's view is not what was split.
Y1   A close's byte count does not grow with prompt length: measured at 1k, 4k and 32k prompts,
     max_close_bytes is within a constant factor, with the id term logarithmic (Decision 3) and the
     KV term tile-sized (graph-v4).
Y2   A prefill segment whose opened prev_state is not its predecessor's committed next_state is
     refused BY NAME, and the refusal names both states.
Y3   A forged link cannot be hidden by segment count: the panel's expected detection probability
     for one forged transition is at or above Decision 8's stated number, at every segment count.
Y4   Economic invariance on BOTH axes: a job's weight, quanta, pwu and ticket inputs are identical
     for every (prefill segment count, decode segment count) pair over the same leaves.
Y5   Both widths are DERIVED from the carrier and a class declaring a wider one is refused; no
     preset carries a chosen value.
Y6   ADR-0044 F4-F6 hold verbatim; neither segment count enters the beacon, the panel or the ticket.
Y7   The DA payload still carries the prompt ids whole (Decision 3's carve-out): a seat replaying a
     segment can obtain them, and a class where it cannot is refused rather than replayed blind.
```

## 6. Order of work

Nothing here is built before the measurement above it returns.

| unit | content | done when |
|---|---|---|
| U-00 | graph-v4 merged and its tiled attention leaf measured at position 1k / 4k / 32k | a disputed attention leaf's close is tile-sized at every position |
| U-01 | **ADR-0080's measurement first** — 134 serially bound DECODE segments: latency, bytes, invariance | ADR-0080 U-01's three numbers exist |
| U-02 | **Long-context prefill drill, at the width that is already safe.** 1,000 tokens as 30 × ~34 prefill segments on the real Qwen runtime; measure carrier bytes, latency, memory, ticket, reward, state commitments, one random segment verification, and Y0 answer equivalence | the eight numbers exist and Y0 is green |
| U-03 | Decision 8's coverage number, from measurement: detection probability for one forged link vs. segment count and draw size | the number is derived and written into the ADR |
| U-04 | Decision 3 — `prompt_token_ids_hash` as a Merkle root; the refutation carries an opening | Y1's id half green; every moved id re-pinned and listed |
| U-05 | Decisions 1, 2, 4 — the prefill chain, its transition commitment and its derived width | Y2, Y5 green |
| U-06 | Decision 6 — economic invariance on both axes, as a test written BEFORE the feature | Y4 green |
| U-07 | a long-context row registered through ADR-0075's route and drilled on devnet | the drill reaches a receipt block on a job with a long prompt and a long answer |

**Done when** a person gives a registered class a prompt of thousands of tokens, receives an answer
of thousands of tokens, and the chain holds one job id whose prefill and decode chains were verified
by seats that fetched a bounded number of bytes each — with the reward identical to what the same
leaves would have earned unsegmented, and with the answer byte-identical to the unsegmented run.

## 7. Supersession

| what | disposition |
|---|---|
| ADR-0080 §8 "long INPUT is not solved here" | this ADR is what that line points to |
| ADR-0080 Decision 2 — segments bind their predecessor's state | extended to prefill; and §1.3 records that the mechanism already exists and is enforced at `palw_step_leg.rs:1215` |
| ADR-0080 Decision 5 — graph-v4 is a precondition | restated and strengthened: for a long prompt the attention arm is the whole cost, so it is load-bearing rather than enabling |
| ADR-0080 Decision 3 — economic invariance | extended to the input axis (Decision 6) |
| `prompt_token_ids_hash_v2` — a flat digest over the sequence | replaced by a Merkle root (Decision 3). The flat form is why the id term is linear and it cannot be priced away |
| ADR-0077 Decision 8 — the seat's sampled interval | its draw is the model for Decision 8's segment draw; the within-segment behaviour is unchanged |
| ADR-0026 — borrow the architecture, refuse the proof model | honoured, and Decision 8 is where it bites: sampling a CHAIN is weaker than sampling an execution, so the coverage rule is required to be a number rather than a habit |

## 8. What is deliberately not decided

* **The coverage number** (Decision 8). It follows from U-03 and is not guessed here.
* **The prefill segment width.** Derived (Decision 4). The decode width is measured at 30; prefill
  is different work and its number is U-02's.
* **KV continuation across JOBS.** ADR-0077 §8's exclusion stands: this ADR chains segments inside
  one job, not jobs to each other.
* **Whether a long prompt should cost more per leaf than a short one.** Decision 6 makes it
  neutral. Whether the economics SHOULD be neutral is a separate question this ADR does not open.
* **Which family goes first.** §1.1 observes the hybrid is structurally the better candidate
  (three layers in four carry O(1) recurrent state); it does not decide the order, because the
  hybrid is also the tier with a 33 GiB artifact and minutes-per-answer decode.

## 9. Number hygiene

This is ADR-0081; ADR-0080 is the last on this branch. A concurrent claimant renumbers the later
writer, per ADR-0036 Decision 5.
