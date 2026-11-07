# Dormant court transport verification — 2026-10-20

Branch: `codex/rfc14-x3-reorg-clock`, based on `e27771b96`. These are release correctness/regression checks, not throughput, CPU or memory measurements and not arming evidence. The coefficients in the synthetic probe are not production values.

- [Native transport tests](native-transport-release.log): 4 passed, including exact codec bytes, checked overflow, a real signed native Part/Finish budget refusal and old-original/dormant compatibility. One of the four is the existing legacy chunk carrier test.
- [Consensus compile](consensus-release-check.log): `cargo check --release -p kaspa-consensus` succeeded; warnings remain.
- [Final identity regressions](identities-release.log): all 28 passed (12 G14 gates including pinned t12 identity, 5 activation-pool checks, 2 transport checks, 9 unchanged-release checks). Explicit `Never` now preserves params, identity and schedule IDs. No golden value was changed.
- [Initial identity regression](identities-before-never-fix.log): existing G14/t12 identity tests passed, but the new explicit-`Never` schedule-ID test failed (1 passed, 1 failed in the new target). The fix excludes an already-`Never` transport fence from schedule visitation; it does not repin any expected identity.

Commands ran in this worktree under `/Users/wata/.cache/misaka-heavy-lock.sh`:

```sh
cargo test --release -p kaspa-consensus-core --lib transport_
cargo test --release -p kaspa-consensus-core --test palw_court_transport_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates
cargo check --release -p kaspa-consensus
```

The native log is a clean replay under the same lock of the release lib-test executable `target/release/deps/kaspa_consensus_core-cd85ed321316b157 transport_`. The build attempt's output overlapped a previous failed compile's log, so that mixed file is not presented as evidence. The clean replay passed all four tests.

For scope and remaining requirements see the [implementation record](../../../design/palw/integration/codex-court-transport-progress-2026-10-20.md). A real processor-level capacity exhaustion test, pre-authentication work limits, physical reserved supply, service attribution, shared pursuit and production calibration remain open. No fence was armed and no live network was changed.

## Raw log SHA-256

- `consensus-release-check.log`: `883db23ffb50675e1da69b38c8d09e5a7f646744cf56b1762ed705b11c6f2fd0`
- `identities-before-never-fix.log`: `cb99c67528e203beef4053f8c4b37b8d2bfb1b47c707428e69da1a1b7f1f8485`
- `identities-release.log`: `441afc6c9f7a4980e899ce20a328d2ead6e23be5032387fc6d81fbb07056cbfb`
- `native-transport-release.log`: `a2362a6bd49d5a1f0b27d4fb9d66d18fae7204bf41cfb733ae995664383cf0ef`
