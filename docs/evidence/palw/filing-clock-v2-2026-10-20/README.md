# Proposed sequential filing clock checks — 2026-10-20

Branch `codex/rfc14-x3-reorg-clock`, based on `4f41081ee`. Correctness/regression checks only: no throughput, network-latency, CPU or memory measurements. All build commands ran through `/Users/wata/.cache/misaka-heavy-lock.sh` in this worktree. No production value or pinned identity was changed.

- [Final native plan checks](native-release.log): 2 passed, 0 failed, 0 ignored. V1 bytes and rederived plan remain exact; high synthetic byte throughput does not erase V2 serial waits; both collection-window dominance and deadline propagation are checked; the unchanged door cap refuses the longer requirement.
- [Identity regressions](identity-regressions-release.log): 29 passed, 0 failed, 0 ignored (G14 gates 12 including t12 identity, activation pool 5, dormant transport 2, unchanged release 9, explicit clock-version discrimination 1).
- [Consensus compile](consensus-release-check.log): release check succeeded; existing warnings remain.
- [Initial native run](native-before-collection-fixture-fix.log): 1 passed, 1 failed. The plan test wrongly assumed V2 must always extend collect. Its mixed fixture's existing collection window already covers both allowances. The correction explicitly checks this case, then compares V1/V2 under the same existing synthetic small-window configuration. No production cap or fixture constant was raised to pass.

Commands (without the shared-lock wrapper):

```sh
cargo test --release -p kaspa-consensus-core --lib sequential_filing_v2
cargo test --release -p kaspa-consensus-core --test palw_tir_filing_clock_v2 --test palw_court_transport_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates
cargo check --release -p kaspa-consensus
```

See [ADR-0193](../../../adr/0193-sequential-witness-filing-clock.md) and the [implementation record](../../../design/palw/integration/codex-filing-clock-progress-2026-10-20.md). The unadopted V2 has no pinned cost record and no shipped preset. The actual door, reserved physical supply, fair inclusion/duty rules and all-owner outsider route remain unproven. No arming, integration or deployment occurred.

Final selected total: **31 passed / 0 failed / 0 ignored**. This is not a G14 completion count. X3 document validation: PASS with 0 new issues, 43 known baseline references; not a clean strict audit.

## Raw log SHA-256

- `consensus-release-check.log`: `f74259f44b22dea45f5d93f118cbd3dfe5d0f58d95ef1aeaa546ed25c83675c2`
- `identity-regressions-release.log`: `675e567c408eb63bc1b821d08c6675f73c9c8fbd1f29ec6647e554ada330d737`
- `native-before-collection-fixture-fix.log`: `2f29a992b24e1f0f360b294bc2e33cfd9085101b1044df59b70dafca1ea0ba3d`
- `native-release.log`: `4a2683157af6c4a3cbf4d74657738fe550419f6f72da2e39b732c95bd7ebc428`
