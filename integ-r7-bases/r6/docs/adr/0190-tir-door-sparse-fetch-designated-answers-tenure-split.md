# ADR-0190 — TIR conviction door: sparse fetch, designated answers, tenure split (Proposed)

- **Decision:** Proposed (2026-10-18). Nothing adopted in code, no fence, no pin, no rule changed. Number taken: 0190 (next free in
  `docs/maintenance/number-registry.json`; 0188 is taken on `codex-free/d2-x3`, 0189 is the options ADR on this lane).
- **Scope:** Huihui-Qwen3.5-2B 32K class; the conviction door of the public court (`palw_tir_frame_cost_v1`).
- **Evidence:** [door-breakdown-1018](../evidence/palw/g14-measure-2026-10-18/door-breakdown-1018.md),
  [read-set-1018](../evidence/palw/g14-measure-2026-10-18/read-set-1018.md), [nonconone-door-options-1018](../evidence/palw/g14-measure-2026-10-18/nonconone-door-options-1018.md),
  and the structural window rule of `release/t12-16000` @ 484e8fa67 (`derived-upper-1018`, read only, not in this branch).
- **Related:** [ADR-0189](0189-tir-court-read-set-door-options.md), [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md).

## Decided by the user (2026-10-18), constraints of this ADR

1. Keep the 3,120 DAA bound. Do not exclude the model (Huihui). Arming only after every gate passes.
2. Separate the evidence-retention period from the slot-occupancy period (today idle grace = `max(reorg_retention, inclusion_margin)`).
   Never treat an honest owner waiting for an answer as idle; never substitute 0 for the wait.
3. Designated custodians answer each request; reuse only authenticated bodies of the same claim/root/coordinates; never punish only for
   not being designated; a silent designated set reaches DA default and refusal within a finite deadline; signers of a false Valid stay
   liable.

## Derivation of the door: per claim, not per class (answer to the open question)

The door is computed **per claim**. `PalwTirPublicSourceV1` is built from the claim's own header and job at
`consensus/core/src/palw_tir_public_source_v1.rs:226`:
`let cone_runs = verified.space.cone_runs_bound_v1(&job, header.step_leaf_count, RUN)`. The door reads it through
`source.cone_runs_v1()` in `tir_public_localization_turns_v1` (`palw_tir_frame_cost_v1.rs:351`) and `tir_public_localization_path_v1`
(`:416`). The class maximum enters only the clock/price bounds (`maximum_custodians`, `maximum_leaf_bytes`).

Consequences:
- The measured door 1,062,231 is the door of the **measured short job** (prefill 4, decode 2, 114,664 leaves, cone 448 = the job's own cap).
- The refusal `DoorBeyondLiabilityHorizon` is checked per claim, so a claim with a long job is refused at a larger door. At 32,768
  positions the shipped `cone_runs_bound_v1` is 95,309 runs (analyser, `derived-upper-1018`), i.e. about 2.2e8 DAA on the serial structure;
  the per-leaf sound derived upper is 116,166 runs. **Under the current serial turn structure a full-context claim cannot pass 3,120.**
- The same door must therefore be evaluated for both: the measured short job (448 cap) and the 32K maximum (95,309 cap / 116,166 derived).

## Re-derivation of the non-cone terms (the 3,132 claim is not an impossibility proof)

`tir_public_localization_path_v1` (`palw_tir_frame_cost_v1.rs:424-427`) prices the output / stop-rule check and the reorg restart at
`widest = path.2`, the largest span on the path, which is the cone-run span 2,346. The output check has no request of its own in the code:
it is one turn with no body defined, so 2,346 is a placeholder reuse, not a measured or derived cost. Its real request (the trace tile,
69 DAA, or a leaf) would cost about 69 DAA. The reorg restart is priced as one turn at the widest span; whether a reorg replays one turn
or the whole path is not decided in the code and must be stated before the restart term is changed. These two terms are therefore the
trace of the 3,132 figure and are open, not proven.

Re-derived spans (script `door/sens.py`, reproduces the 1,062,231 path and the 12,537 option C value):
- one-round sparse span, body = L x 8,192 B: L = 97 gives 907 DAA; L = 256 gives 2,313 DAA.
- one-round span at the observed wire 1,169,221 B: 1,314 DAA (= option C).

## Residual terms (not assumed zero)

- **Uncovered:** the gather / model-parameter operand reads are not bounded by the structural analyser (`derived-upper-1018`). They are an
  explicit uncovered term; no value is assumed.
- **Uncovered:** the derived read-set bound is per leaf over the window rule `q <= p <= q+C+h+1`; the tile-contiguity figure (20,850) holds
  only on fixtures and is not a bound for the class.
- **Auth paths, signatures, control carriers, retention, re-fetch, queue:** each is priced in the span (control 14,320 B per carrier,
  133 controls per 2 MB turn). Signature and retention costs of designated answers are not yet measured.
- **Tenure split (point 2):** the owner queue term (1,536 DAA) changes when the wait is separated from retention; the new value is not
  derived yet.

## Option comparison on the same inputs (ceilings unless stated)

The table in `nonconone-door-options-1018` remains the reference: with the cone at zero the door is 11,223; the non-cone terms must fall by
8,103 DAA. No combination reaches 3,120 with the cone at its one-round floor, because the output check and the trace tiles still add
about 4,200 DAA of serial terms unless they are batched too.

## Decision for the next step (not adopted here)

The design that can satisfy both the 3,120 bound and the 32K maximum must make the **number of serial rounds independent of the run
count**: bounded rounds of batched requests, bytes per round bounded by carrier capacity, designated answers, and the output check and
reorg restart priced at their own request spans. This ADR does not select the final design; it records that the per-claim door at 32K
cannot pass under the current structure.

## Unmet conditions

- No implementation. No dormant fence exists for sparse fetch, designated answers or the tenure split.
- No test yet reproduces the failure on the real class in this lane beyond the existing `door_breakdown_on_a_real_class` (asserts the
  refusal at 3,120; baseline run was started, see final report).
- The 32K maximum door is not measured; only derived from the analyser lane.
- Gather/model-parameter reads are uncovered (above).
- Whole collusion plus one external verifier, default on missing answers, and restart/reorg/IBD identity are not tested.

## Chosen design (Proposed; not implemented)

- **Bounded rounds.** The disputed step's read set is fetched in R = 2 serial rounds, independent of the run count. Round 1: one batched
  request for the whole derived operand closure of the disputed leaf. Round 2: only the value-dependent follow-ups that the window rule
  cannot pre-compute (Gather index, Select branch, TopK selection). Bytes per round are bounded by carrier capacity; a round that exceeds it
  is split into parts inside the same round, not into more rounds.
- **Designated answers.** Each request has a designated custodian recorded in consensus state with obligation, deadline, handover order and
  default. Authenticated bodies are reused only for the same claim, root and coordinates. Signers of a false Valid stay liable even if they
  did not carry the body. Silence of every designated custodian gives DA default and reward/credit refusal at a finite deadline.
- **Tenure split.** Evidence retention and slot occupancy are separate periods. An honest owner waiting for an answer is not idle. Handover
  keeps pursuit state, evidence and charges.
- **Output/stop and reorg.** Priced at their own request spans (trace tile 69 DAA or a leaf), never at the widest span of the path.

## Door formula (every term)

Door = prep + acq + margin_round(R) + transport(round 1 bytes, shared once per authenticated body) + transport(round 2 bytes)
     + output_check_span + reorg_span + owner_queue + descent + rows/tiles + IDs + filing
     + **uncovered: gather/param operand bytes (not bounded, explicit)**
     + auth paths + signatures + control carriers + retention + re-fetch

Only the terms with a measured or derived value can be filled. The remaining ones are named and not assumed zero.

## Numbers from arithmetic (not from tests)

| case | read-set body | transport (shared once) | transport (56 chains) |
|---|---:|---:|---:|
| measured short job, observed max 97 leaves | 794,624 B | 7 DAA | 356 DAA |
| 32K maximum, derived window 4,915,476 leaves | 40,267,579,392 B | 322,141 DAA | 18,039,876 DAA |

Threshold: 3,120 DAA at 125,000 B/DAA allows 390,000,000 B shared once, which is 47,607 leaves. The derived 32K window is about 103 times
that. The dominant term at 32K is the transport of the read-set bytes, not the rounds.

## Concrete unmet condition

The 32K maximum door is not under 3,120 under the current analyser bound. The condition to meet is a demand-driven operand-closure byte bound
for the disputed step of at most 390 MB (shared once) at 32K, or the bound must be shown to be at most 47,607 leaves. The analyser gives only
the window superset, and gather/param reads are not bounded. Until that is derived, 3,120 cannot be shown at the 32K maximum.

## What was not done in this lane

- No implementation of the rounds, designation state, tenure split, or dormant fence.
- The baseline test `door_breakdown_on_a_real_class` (run `b99g16ncm`) had not finished under the shared lock when this was written.
