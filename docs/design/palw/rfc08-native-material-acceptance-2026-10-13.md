# RFC08 native settlement and material acceptance — 2026-10-13

This record retains the full BUDGET/WINDOW/RECOVERY scope. No shipped activation changes.

## Production connections already present

The older requirement index's standalone and pre-draw entries are not evidence that their hooks are absent:

- `palw_kernel_route_fold_v1` reserves the shared Q/R claim budget before accepted claim, fee or bond writes;
  `plan_kernel_claim_budget_v1` grants no B or F. `hold_kernel_income_v1` reclassifies the same R into the
  actual recipient hold. EXEC attachment transfers that funded right rather than adding another standalone payment.
- `bond_budget_round_draw_v1` supplies each bond's remaining rights to the candidate draw, reserves the allocated
  rights in the same window, and atomically clears the whole draw if any reservation fails.
- `native_final.rs` contains native root Final, actual poster debit, independently held reward legs, physical DB
  reopen, a selected-chain reorg, public conviction and positive coinbase settlement controls. Its REAL trace
  admission and lottery seam is explicitly retained; these controls do not prove genuine model execution.

These connections need composed acceptance and resource/economic evidence; naming a hook is not a gate PASS.

## Additional consumer control

The positive-fund test now arms reporter schema V2 and material schema V3 together with the existing private
pre-genesis budget. It requires every native root and kernel dependency to retain material while physical payment
is unknown, then requires their material release to equal the last actual positive fund payment plus W. It compares
material receipts after physical DB reopen, checks the inclusive pre-release boundary, and checks that the exact
root's material releases while another root of the same recipients remains separately funded. Existing coinbase,
one-debit, duplicate-licence and replay assertions remain in the same test.

The native test must be compiled and executed before this additional control is recorded as PASS. Its private
D1/W436 and counterfactual whole-escrow pricing are mechanics values, not approved production security or economics.

## Fresh baseline evidence

The existing post-genesis-repair core test executable
`/tmp/misaka-rfc67812-node-target/debug/deps/kaspa_consensus_core-e9cf524de6d5c171` was executed directly, without a new
compile. These results cover its compiled source, not the newly edited native consumer test:

| Selection | Terminal result | Log |
| --- | --- | --- |
| `palw_bond_budget` | 64 passed, zero failed/ignored | `/tmp/misaka-rfc67812-rfc08-budget-baseline.log` |
| `palw_state_v2::tests::exec_v2_fold_v1` | 89 passed, zero failed, one existing ignored resource case | `/tmp/misaka-rfc67812-rfc08-exec-baseline.log` |

The budget cases include sparse physical payment clocks, partial-fund cancellation, exact journal reorg, material
carriage/rebuild, capped draw rollback and no double R charge. These leaf/fold results do not replace native-process
acceptance. The edited native test passed `rustfmt --edition 2024 --check` and `git diff --check`; execution is pending.

## GC read failure repair

The retention janitor and foreign-copy pruning both read `palw_material_income_hold_until_v1` afresh before deletion.
They already pin unknown physical-payment clocks and holds beyond the original trace/provider term. However, the
Consensus API previously converted a reader error or missing snapshot into `None` using `.ok().flatten()?`.
That result means no material obligation to the GC consumer. The general reader may also return a readable old tip
when walking to the committed sink fails; deleting from that other branch's absence is unsafe.

The repaired API pins with `Some(u64::MAX)` when the active material profile cannot supply the committed generation,
the sink changes during the read, or pruning import is active, unstable, unreadable or changes epoch during the read.
A healthy complete snapshot still returns its exact deadline or genuine absence. Dormant profiles keep returning
`None`. No signed provider term is changed by this conservative local file-deletion rule.

The production decision helper was compiled directly with `rustc --test` through a module wrapper, without a new
Cargo target. Two controls passed. Replacing only the helper's decision with the previous `.ok().flatten()` behavior
in a temporary copy made both controls fail: unavailable state discharged the hold and an unstable import executed
its otherwise readable release decision. Logs: `/tmp/misaka-rfc67812-material-hold-before.log` and
`/tmp/misaka-rfc67812-material-hold-after.log`. This reproduces the decision defect; it is not a simulated physical
node crash or a full Consensus build. The native positive-fund control additionally checks the actual Consensus API
at unknown payment, payment plus W, physical DB reopen, pre-release and release boundaries. That integration compile
and run remains pending. Formatting and diff checks passed.

## Remaining acceptance

Signed provider TTL and collectible collateral must cover each admitted whole path. The current finite native
session guard refuses paths beyond the shortest signed dependency horizon; it does not authorize an extension.
Actual physical payment can remain delayed without a known clock, so root-exact retention does not alone prove
provider consent or a funded finite storage service. Pruned dependency transport, independent process restart/reorg,
worst population resources, all consumer paths and workers' minimum net compensation remain required. The new
positive-money control does not close those requirements.
