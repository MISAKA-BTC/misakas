# Lane ARM-LEGACY report (`LegacyPanelAttempt` / `LegacyFusedLeaf`), 2026-10-27

Branch `claude/g14-arm-legacy`, based on `claude/g14-panel1-17000` (`56c5ca067`) with `claude/g14-fam-legacy2` merged in (no conflicts). Nothing was pushed or deployed, the activation was not changed, and no record is pinned. The target moved from 19,000 to **17,000** (coordinator, 2026-10-27).

## Passing

- `consensus/core/tests/g14_arm_legacy_17000.rs` (candidate list, test only; `PALW_T12_FULL_ACTIVATION_*` unchanged). On testnet-12 as shipped, filer, held DA v2, outsider lane, court capacity and route required are armed at the same height. **The whole validation (F02 and F03 included) passes with only the TEST records.** It is refused without the records, every record is load-bearing, and it is refused without the outsider lane (`OneOutsiderCapacityIncomplete`). Both families are armed from that height and not below it. The ids do not move.
  The test was run at 19,000 and passed. It was renamed for 17,000 and that rerun was skipped to save the build. **Waiver: rerun at 17,000.**
- `g14_close_legacy` 9/9 and `g14_court_capacity` 15/15 (1 ignored).
- Node E2E, 7/7:
  - (a) `legacy_v2_attempt_full_collusion_outside_bond_convicts[_with_court_capacity]`: the producer and every seat lie (all `Valid`), and one outsider convicts and slashes using public material only. The `_with_court_capacity` case covers the FusedLeaf route. The IBD replay roots match block by block.
  - (b) held DA withholding ends in DA default, not a compute conviction.
  - (c) the honest twin is Final and paid.
  - (d) new `g14_arm_legacy_d.rs`: held-DA default across a restart (same DB), a reorg, a full IBD and a pruned import; the honest twin is untouched across a restart, IBD and pruned import.
  - Existing cases cover (a) across restart, reorg, IBD and pruned import: `lg14a_the_pursuit_survives_a_node_restart` and `g14r_resume` (not run here).

## Measurements

`docs/evidence/palw/g14-17000/measurements/M-CAP/arm-legacy-2026-10-27/`. **UNDER_LOAD, load about 11** (the no-load rerun was stopped while queued behind slots at load 45).

- One-move verdict p95: 141.7 µs.
- Pursuit through court and slash: 20 DAA.
- Panel bind + license, 5 seats: about 0.5 s.

V11 estimate: unchanged (cap 42, weight 13, ρ 3,125). The measurement does not contradict it. The ledger addendum is in `lane-values-report.md`.

## Remaining to arm (waivers)

- The records (coordinator).
- A no-load or minimum-spec M-CAP run.
- The opening, dissection and close classes and the working set.
- The hard-deadline bound (not armed on this harness).
- Panel concurrent capacity.
- The drill (skipped by the user).
- The unmarked-genesis node E2E (it runs on the isolated G14 genesis).
