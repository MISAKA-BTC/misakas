# lane SEED: Panel=1 seat draw and sample seed after the commit (G14 independent review MAJOR-1 / MINOR-1)

2026-10-26. branch `claude/g14-seed` (base `2119c943b` + review `1653fc09f`).

## Changes (Panel=1 fence `palw_kernel_route_panel_one_v1` only; unarmed on every network)
- `consensus/core/src/palw_kernel_route_v1.rs`: added `palw_kernel_panel_one_seed_v1` and `palw_kernel_panel_one_challenge_v1`. They compute `H(domain; tag ‖ claim ‖ anchor.span ‖ anchor.block ‖ anchor.execution_key)` from the existing round seed anchor (ADR-0130/0170, the same kind of anchor Gate C draws from). Also added `palw_kernel_panel_one_anchor_after_commit_v1`, which is true when `anchor.span × span_daa > committed_daa`. An anchor recorded under a longer span length always reads as older, so the check fails closed. No new randomness source.
- `consensus/core/src/palw_kernel_route_fold_v1.rs`: past the fence, nothing is drawn at commit time. `draw_panel_one_seats_v1` runs in the kernel tick. It draws only once, for claims that meet all of these conditions:
  - Checking, Panel-licensed and Program;
  - committed at or past the fence;
  - no assignment or receipt yet;
  - not a paid V2 base claim.

  The draw waits for an anchor that is later than the commit. A claim that no anchor reaches before its check deadline gets no seat, so it ends TimedOut and never becomes Final.
- `state_params.rs`: added `kernel_route_panel_one_governs_v1`.
- Below the fence the code path, the seeds and the rows are unchanged. No new table, wire tag or ID; the assignment row (table 33) keeps its existing type.

## Tests
- `cargo test -p kaspa-consensus-core --lib lane_seed`: exit 0, 1 passed. This covers (a) in pure functions:
  - one claim id gives different seeds under different anchors;
  - 64 anchors seat 4 or more distinct operators;
  - the seeds differ from the interim seeds;
  - the freshness boundary holds.
- `cargo test -p kaspa-consensus --lib lane_seed`: exit 0, 2 passed. These are e2e tests on the real processor, with Panel=1 and Gate C armed without their validation (TEST-ONLY certification policy):
  1. No seat exists at commit. The seat, challenge anchor and sample seed appear only after a post-commit anchor and are equal to that anchor's derivation. The Sybil seat covers the lie, the producer gets reward 0 and the claim is not SecurityCertified. One outsider convicts, the producer's real bond is slashed, and replay gives the same result.
  2. (c)/(d) below.

## (c) Result: Gate C fails
NotCertified returns the held reward in full to the poster. **The lying producer is not slashed** (`slashed(0)==0`). Its loss is only the admission fee plus the reward it never earned. Slashing happens only through a public conviction (FileProof) inside the liability horizon. Residual risk: an unprosecuted lie costs only the admission fee.

## (d) Result
A pre-Gate-C Final:
- pays the producer 0, because the reward is held;
- has `finals_read_v1` event `None`, so it is never an RFC07 beacon source;
- is not SecurityCertified.

Kernel claims reserve B=F=0, and frontier, settlement, Rule E and anchor read V2 claims. This report cites code for that point. The test does not check each of those readers one by one.

## Withholding / residual grinding
The anchor is the one recorded in the first span after the commit. Its block's attempt `execution_key` depends on pre_pow (parents and payload) and the nonce bucket. Each extra option the producer wants therefore costs one admitted attempt (inference plus a PoW block), and withholding a block forfeits its reward. Under RFC-0018 sibling attribution the anchor is the minimum within the span, so the producer can only add attempts or withhold them. It cannot choose the value freely. This is the same exposure Gate C already has. The cost per option rises from about 0 to one attempt.

## Remaining
- Liveness depends on anchor cadence relative to `check_window_daa`. On t12 the span is 1 DAA past 7,300. A drill is required.
- Above the fence, the e2e harness may also use span length 5. The test accepts either 1 or 5.
- The slash gap in (c) is a design decision for the user or the lead.
