# Native pruner / virtual PALW-status lock order

Status: production correction compiled and the deterministic real-store regression passed. Run 6 advanced through the former deadlock, physically deleted the target body/delta, then terminated FAILED on current-point PALW snapshot availability. After its startup was verified, root intentionally stopped the deadlocked Run-5 writer; its database, fixed binary, source and logs remain preserved.

## Actual stopped path

Run 5 reached its first real pruning-point movement after the 72k checkpoint (DAA 37520, PP DAA 9), with the Release body and delta still present. Root observed live writer PID 32288 at CPU zero and no progress for about 16 minutes. This is not recorded as a mere observation timeout or physical-pruning PASS.

The two-second/90-sample artifact `/tmp/misaka-rfc08-pruning-run5-first-pp-sample-20261025.txt` shows virtual `commit_utxo_state` waiting for the statuses write lock in `StatusesStoreBatchExtensions::set_batch`; the pruner waits for another exclusive lock in `PruningProcessor::prune`.

The fixed Run-5 binary was inspected **as a file only**, using LLDB `target create` and `disassemble`, without attaching to or running the process. Sample return PC `0x104cf55c8` at prune `+12844`, load base `0x104354000`, maps to file PC `0x1009a15c8`. The successful lock branch immediately calls `DbPalwStateV2Store::clear_legacy_answer_history`, which is the beginning of `delete_delta_batch`. Disassembly is saved in `/tmp/misaka-rfc08-run5-prune-lock-disassembly-20261025.txt`. This identifies the actual pruner wait as PALW-store exclusion, rather than inferring it from CPU zero alone.

The source cycle is concrete:

- Virtual holds `palw_archive_guard` through the uncommitted archive/delta union, then waits for statuses to publish the same block batch.
- Pruner holds statuses for its row batch, then attempts a separate PALW writer for delta deletion (and another reader for archive GC).

The fixture's test thread is parked awaiting the native heartbeat future. Its per-iteration PALW reader is a statement-scoped guard, not held across that await.

## Correction

The actual pruning-row path now obtains one private four-store guard group in the order **PALW → relations → reachability-relations → statuses**. It uses that same PALW writer for both delta deletion and prefix-history GC. All guards survive the unchanged `WriteBatch`, reachability staging/upgrade, durable DB write and existing rewarded/quality publication; PALW is explicitly dropped only afterward, before the next reachability upgradable-read acquisition.

The surrounding pruning-session and reachability-upgradable ordering remains unchanged. The header processor still takes relations before statuses and never requests PALW, so the correction does not introduce the forbidden PALW→statuses→relations order. Virtual archive exclusion is not dropped early. No pruning depth, retention boundary, point selection, native activation or state rule changes.

## Deterministic regression scope

The public filter `pruning_row_lock_real_stores_complete_virtual_batch_before_pruner` runs an owned bounded child. The production wrapper and test share the same acquisition function over the real `ConsensusStorage` PALW, relations, reachability-relations and statuses guards. The test's instance-local channels pause that function immediately before PALW; no process-global hooks or substitute mutex model are used.

A virtual-style writer holds actual PALW exclusion and must finish an actual statuses/delta DB batch while the pruner waits. The test also holds actual relations after releasing PALW and requires statuses to remain available while the pruner owns PALW and waits for relations. These controls reject both the original statuses→PALW cycle and the alternative statuses-before-relations cycle. After all four pruning guards are acquired, the actual delta-delete, prefix-history GC and status update are staged. Before the DB write the test requires all four guards still held and the durable delta/status unchanged. Only after a flush gate can the batch become durable and guards release. Fresh empty-cache store readers verify the deleted delta/archive body/expiry, retained virtual delta and exact statuses.

The parent bounds the child at 30 seconds and reaps that owned child on failure, so a lock-order regression cannot strand a blocked thread in the main test process. This child controller is a regression harness, not an OS-crash campaign. The prefix-history movement is a store fixture, not an authenticated claim or court transition.

This verifies the shared production lock order and atomic store mechanism; it does not establish full production-pruner traversal, allocator bounds, K3, pruned IBD or the 15-gate campaign. The unmodified-depth long path is running separately. Run-5 artifacts remain preserved independently.

## Executed verification and next long run

The first no-run build failed E0432 because the new test imported the action enum from the court module instead of its existing carriage module. Only the test import was corrected; the original diagnostic remains in `/tmp/misaka-rfc-pruner-row-lock-build-20261025.log`. The corrected consensus `--lib --features evm` no-run build passed in **5m47s**, log `/tmp/misaka-rfc-pruner-row-lock-build-retry1-20261025.log`. Five scoped source identities were rechecked before pinning, in `/tmp/misaka-rfc-pruner-row-lock-source-retry1-20261025/record.json`. They do not represent whole-tree exercised coverage.

Immutable binary: `/tmp/misaka-rfc-pruner-row-lock-consensus-retry1-20261025`, SHA-256 `7c9f9823288b1100d39e13c344de8b77168c04a116adc04abf7cf17a7a911c69`. The exact real-store test passed **1/1, 0.18s**, including its one child **0.04s**; the child is not an additional independent test. Runtime log: `/tmp/misaka-rfc-pruner-row-lock-tests-retry1-pinned-20261025.log`. A preceding root pinning script failed before test execution due to a Python path-expression typo; `/tmp/misaka-rfc-pruner-row-lock-tests-retry1-20261025.log` only contains the resulting missing-executable error and is not a runtime test failure or PASS.

The same immutable binary started the actual long Run 6 with writer **PID44103**, new DB `/private/tmp/rusty-kaspa/.tmpZiTc06`. Its start record confirms depth/lattice74920, finality20, nonarchival, retentionNone, proof m1000, simulated clock and max159840. Progress: `/tmp/misaka-rfc08-typed-pruning-progress-run6-lock-order-20261025.log`; runtime: `/tmp/misaka-rfc08-typed-pruning-native-tests-run6-lock-order-20261025.log`; scoped 36-file identity snapshot: `/tmp/misaka-rfc08-typed-pruning-run6-lock-order-source-20261025/`. Some node identities are not compiled by the consensus target.

The existing harness has no long-loop resume entrypoint: its writer creates a new temporary DB and reconstructs signed material, funding and Final. Acquisition-only cold readers do not mine, and their earlier expected-tip input is not a full wallet/clock/job checkpoint. Run 6 therefore starts fresh; Run-5 data are not rewritten to simulate a resume.

Root verified the old exact writer command and last 72k/PP9 record, then sent SIGTERM only to owned writer PID32288 after actual Run-6 startup. Controller record: `/tmp/misaka-rfc08-typed-pruning-run5-controller-stop-20261025.json`; old parent handle16600 exited101 due to this controlled interruption. This is an intentional interruption for confirmed deadlock, not a natural failure, physical-pruning PASS or OS-crash campaign. Run 6 must independently reach actual body/delta deletion, cold receipt and all-generation GC; these results remain PENDING.


## Run 6 terminal scope

Run 6 reached beats72713 / DAA37876 / PP DAA409 with both target body and delta absent, then failed at the current-point snapshot loader returning None. Its public parent exited101 after2541.51s. In-process initial/replacement receipts passed preceding assertions; cold receipt and all-generation GC were not reached. The old lock cycle did not recur at the first PP movement. This does not make the full long filter PASS. The temporary Run-6 DB was naturally removed during failure unwinding; its exact None cause remains unproven. Confirmed capture-before-delete and recovery gaps, and the distinction from possible point-publication lag, are recorded in [the terminal pruning evidence](0008-typed-native-pruning-2026-10-25.md#run-6-terminal-failure-after-actual-deletion). Earlier PENDING statements in this document describe historical checkpoints.
