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

## First native run: two passed, positive-money control failed

The EVM/private-integration consensus build succeeded in 16m23s. The three-case native selection then terminated
after 2495.04 seconds: the funded-Final/DB-reopen/public-conviction control and the actual reorg/reapplication control
passed; the positive-fund control failed. [The complete failed command log](evidence/rfc08-native-final-2026-10-13-failed.log)
is preserved. No full positive settlement or post-payment material PASS is assigned to this run.

The positive case reached its two native Finals and passed the new unknown-payment material/Consensus API checks.
It later expected 55,965,885,913 sompi on a shared queue while the first observed row contained 12,803,386,003.
The test had compared the full Panel-plus-EXEC-slice aggregate with the first intermediate queue row. Production
`release_plan` waits for the native vesting row to leave before releasing EXEC legs, so these two funds can enter and
physically drain from the same payload key in separate ticks. The corrected control allows an intermediate consumed
amount below the predeclared aggregate, rejects excess consumption, and records completion only when all expected
funds have entered that receipt and physically drained. Final exact aggregate R, exact actual coinbase cash, deadline
plus W, all material/DB-reopen and replay assertions remain required. The correction's fresh run is pending.

A five-second sample of the still-running test showed `Net::beat_to`/heartbeat block templates and background
overlay-window/PALW folding, not the new material oracle. The long silent period was actual clock advancement.
The corrected native control prints its frozen expiry targets and progress every 64 actual heartbeat blocks, and
uses finite heartbeat limits to diagnose missing DAA progress instead of waiting indefinitely. It still advances
only by actual blocks, with no DAA/Final setter or synthetic maturity. These fixture diagnostic limits are not
production throughput/resource approval.

## Paid material admission mismatch repair

`require_paid_exec_material_window_v1` now checks the native paid contract's already accepted `material_until` against
the existing signed root-expiry-derived EXEC retention horizon. Declaration admission calls it before kernel fee/R
transfer and root/work-use writes. Restore/replay applies it to retained non-void roots. The existing provider lease
check still requires its whole EXEC horizon; a contract cannot ask it to serve through a later unauthenticated term.
No-contract histories return without new reads/writes, and no wire field, public preset or clock is changed.

The new core `exec_paid_material_contract_refuses_short_signed_horizon_before_any_fee_or_provider_write` obtains native
contract terms through admission, keeps actual provider lease/transfer fold terms fixed, and checks one-DAA-short
refusal, exact-bound admission, parent carriage unchanged on refusal and the existing delta/revert/carriage checks on
success. Its header-signature seam is explicitly retained. The new native
`x8_g14_native_paid_material_rejects_one_daa_beyond_signed_exec_and_accepts_exact_provider_terms` uses actual ML-DSA
provider leases/DA transfer, signed prefix and root carriers, checks no ledger/money/provider mutation on refusal,
then re-signs a sufficient expiry and requires admission without rewriting any provider term. Both controls await
fresh compilation/execution. They do not claim to bound future physical payment or renew a finite provider contract.

The first core command compiled successfully in 9m05s but its new test failed before reaching the material guard:
the private fixture retained the numeric probe's B rate of one while increasing its capital unit to 100 million.
Ten units of that bond therefore had only ten block units, below one whole block's required one million. Native
attempt admission correctly refused the underfunded fixture and the subsequent Panel bind found no claim.
[The failed core log](evidence/rfc08-paid-material-core-fixture-failure-2026-10-13.log) is retained. The fixture now fixes
its B rate and open-claim ceiling to the same adequate pre-genesis mechanics values as the native control. The
production material guard is unchanged. Fresh execution remains required; the failed fixture is not a guard PASS.

The corrected command terminated successfully: fresh build 7m53s, one focused test passed in 0.06s. It used the existing
node target, jobs2, debug0, `CARGO_INCREMENTAL=1` and `evm,private-integration-config,legacy-secp256k1`, with `--lib`.
[The passing core log](evidence/rfc08-paid-material-core-pass-2026-10-13.log) retains the exact test and executable.
That same fresh executable then passed [90 EXEC-fold cases](evidence/rfc08-paid-material-exec-regressions-2026-10-13.log)
with one existing ignored resource case, and [64 budget cases](evidence/rfc08-paid-material-budget-regressions-2026-10-13.log)
with none ignored. The focused case is included in those 90, not an additional independent case. These runs cover the
new paid-material admission/restore guard and existing no-contract accounting/recovery controls; the actual ML-DSA
provider/native carrier control still requires the root's later consensus build/run.

## Remaining acceptance

Signed provider TTL and collectible collateral must cover each admitted whole path. The current finite native
session guard refuses paths beyond the shortest signed dependency horizon; it does not authorize an extension.
Actual physical payment can remain delayed without a known clock, so root-exact retention does not alone prove
provider consent or a funded finite storage service. Pruned dependency transport, independent process restart/reorg,
worst population resources, all consumer paths and workers' minimum net compensation remain required. The new
positive-money control does not close those requirements.
