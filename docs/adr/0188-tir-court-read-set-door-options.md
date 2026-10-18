# ADR-0188 — TIR court read set and the conviction door: options (Proposed)

- **Decision:** Proposed (2026-10-18). Nothing adopted, nothing implemented, no fence, no pin, no rule changed.
- **Scope:** the Huihui-Qwen3.5-2B 32K class, the cone term of the conviction door (`palw_tir_frame_cost_v1`, `cone_runs_bound_v1`).
- **Evidence:** [read-set-1018](../evidence/palw/g14-measure-2026-10-18/read-set-1018.md) (partial), [door-breakdown-1018](../evidence/palw/g14-measure-2026-10-18/door-breakdown-1018.md), [cone-runs-analysis-1018](../evidence/palw/g14-measure-2026-10-18/cone-runs-analysis-1018.md).
- **Related:** [ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md), [ADR-0181](0181-design-freeze-and-controlled-change.md), PRINCIPLES §11 (a structural change needs an RFC and an ADR and explicit adoption).

## Context

The shipped door for this class is 1,062,231 DAA against a limit of 3,120. The cone term is 448 serial turns (`cone_runs_bound_v1`,
a superset bound that saturates at `ceil(114,664 / 256)`) times 2,346 DAA per turn. The measured read set of one disputed step is
much smaller (decode 2: at most 8 runs, 97 leaves, 1.17 MB wire). That number is observed over 44 samples; it is not a bound.

## Options (variables and assumptions are explicit; door numbers are from `read-set-1018.md`)

1. **Sparse leaf fetch in one round.** Fetch only the operand leaves, not 256-leaf runs, in one serial turn. Door 11,296 to 12,537
   (decode-2 observed inputs). Needs: a derived bound on the read set for every legal job (not yet shown), and sibling-path
   accounting.
2. **Request batching of the same claim's independent turns.** Remove the per-run serial margin by issuing all runs of one claim in
   one round. Door 26,197 (parallel, bound body). Needs: the same claim/root/coordinates only.
3. **Material sharing across custodians and owners (same claim/root/coordinates only).** Door 17,359 serial, 11,658 with option 1.
   Not to merge different owners' requests, and not to merge each signer's liability or default.
4. **Non-cone terms.** Trace tiles 4,209, output/reorg 2,346 each, owner queue 1,536 (total 11,120). Any option that reaches
   3,120 must change these too. None is analysed here.

## Result of the options as measured

No option reaches 3,120. The zero-cone floor is 11,223, so the non-cone terms dominate once the cone term is reduced.

## Unmet conditions (why this is Proposed only)

- The read set is observed on decode 2 (6 positions) only. The decode-30 run reached 28 runs at 33 positions. A derived bound covering
  every legal job, every dispute kind and every checkpoint/history boundary is not shown.
- State and HistTile kinds are not measured (checkpoint 164, h_tile 32).
- No params-free structural analyser; the real court's i128 params still do not fit in 32 GB.
- Non-cone terms are not re-derived per option.
- No adversarial test of: whole collusion plus one external verifier, default on missing answers, restart and reorg recovery.

Adopting any option is a user decision. The minimal change that reaches 3,120 is not yet identified.
