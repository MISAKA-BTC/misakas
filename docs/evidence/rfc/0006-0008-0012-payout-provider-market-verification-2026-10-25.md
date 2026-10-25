# RFC-0006/0008/0012 payout, provider and production market verification

This checkpoint preserves activation `None`, normal selected-source/V6 paid bootstrap refusal, dormant unapproved economics, frozen model tiers and claim-selected approval requirements. K3 inputs remain uncreated, the formal native39 settlement mapping remains unformulated, and all 15 real campaign gates remain UNKNOWN.

## Implemented consumers

- Vesting maturity and activation-pool flushing reserve their actual workspace before allocation and retain its owner through the consuming transition. The pool source-row count is independent of output drain width. See [payout ownership](0006-0008-vesting-payout-workspace-2026-10-25.md).
- Raw/routed provider publication marks an uncertain rename/directory-fsync outcome unavailable until full reopen. The real GC caller refuses metadata retirement and ACK work after that fault. A reopened janitor preserves its durable deletion intent when raw bytes are already gone but the current witness refuses. See [publication and recovery](0008-provider-publication-uncertain-reopen-2026-10-25.md).
- Actual EVM producer and validator use the exact retained parent and complete settlement input. Local resource/store/history failures remain typed local errors. Signed withdrawal is paid once across another block and an in-process Consensus reopen. See [production market connection](0012-production-market-parent-ownership-2026-10-25.md).

The broad Remote run exposed two stale test fixtures: a drifting miner supplied empty required capture material, and a transport test rejected already-defined purpose tag 5. Only those tests changed: the miner retains its opaque Stage-A capture fixture, while the transport test pins tags 0–5, rejects 6–255, and checks signed-purpose substitution refusal. Production capture and signature checks were not weakened. This opaque capture fixture is not backend capture authentication evidence.

## Build and failure history

Five-package `cargo test --lib --no-run` with Consensus/Node EVM and private-integration-config passed on retry4 in **138.75s**. All **33 source/config SHA-256 identities** matched before and after compilation and again before recording. Original source manifest: `/tmp/misaka-rfc-payout-provider-market-wave-source-retry4-20261025/record.json`. Build log: `/tmp/misaka-rfc-payout-provider-market-wave-build-retry4-20261025.log`.

Initial compilation failed E0277 (84.66s), then E0382 in a test diagnostic (112.92s). Retry2 compiled (146.78s), but one new provider fixture incorrectly required a nonempty native authority inventory; it now compares the actual empty kernel-fixture inventory. Retry3 compiled (158.27s), and all 14 new parents passed. Its broad Remote run had 173 passes, two stale-fixture failures and one ignored test. Those failures and original logs remain preserved under retry2/retry3 names; none counts as a pass. Retry4 incorporates the two test-only repairs.

Private-node binary `cargo check` passed in **75.32s**, and non-EVM Consensus library check passed in **56.95s**, with the original 31 identities matching. These production checks precede only the two Remote cfg(test) repairs; no production source changed afterwards. Check controller: `/tmp/misaka-rfc-payout-provider-market-checks-20261025.json`.

## Immutable executable identities

- `kaspa_consensus`: `/tmp/misaka-rfc-payout-provider-market-kaspa_consensus-retry4-20261025`, SHA-256 `9df50188cbc33e1c49570cd50641d4e842d88983ab0b740b6f19e64ab7ae3de1`.
- `kaspa_consensus_core`: `/tmp/misaka-rfc-payout-provider-market-kaspa_consensus_core-retry4-20261025`, SHA-256 `aaa7b055ead002922e7f3d21ee5d041c6dff5e00d9e1f3f0a4b16ddec92a7593`.
- `kaspa_evm`: `/tmp/misaka-rfc-payout-provider-market-kaspa_evm-retry4-20261025`, SHA-256 `ed3750056029ab456c843d0564d648057a0d6dcb855d30b539f79d71f5431b73`.
- `kaspad_lib`: `/tmp/misaka-rfc-payout-provider-market-kaspad_lib-retry4-20261025`, SHA-256 `3569b634e427675dc88836ff88d06d507903f47f5616b6ac2b3df66edc35c402`.
- `misaka_palw_remote`: `/tmp/misaka-rfc-payout-provider-market-misaka_palw_remote-retry4-20261025`, SHA-256 `6369c0d73391659f12f604fdabb339624a21253a6a1070f453da0ea29db41913`.

## Actual runtime results

| Group | Passed parents | Ignored | Harness seconds |
|---|---:|---:|---:|
| payout | 4 | 0 | 0.03 |
| provider | 4 | 0 | 1.14 |
| market-native | 3 | 0 | 0.81 |
| market-executor | 3 | 0 | 0.05 |
| remote-all | 175 | 1 | 6.91 |
| evm-all | 100 | 3 | 1.72 |
| vesting | 25 | 0 | 0.32 |
| activation-r1 | 6 | 0 | 0.14 |
| activation-r2 | 3 | 0 | 0.01 |
| activation-v1 | 25 | 0 | 0.11 |
| settlement-once | 21 | 0 | 0.03 |
| producer | 2 | 0 | 0.03 |
| deposit-once | 1 | 0 | 0.01 |
| y9 | 2 | 0 | 0.21 |
| market-native-history | 4 | 0 | 0.53 |
| replay-native | 3 | 0 | 0.57 |
| terminal-native | 1 | 0 | 9.00 |

All 14 dedicated parents and all 13 related groups passed. Deduplicating test identities across overlapping filters yields **375 distinct passing parents**; subprocess children are inclusive. Four ignored benchmark/generator parents are excluded from passes. Exact filters, counts, timings and log paths are recorded in `/tmp/misaka-rfc-payout-provider-market-focused-results-20261025.json` and `/tmp/misaka-rfc-payout-provider-market-regression-results-20261025.json`; pins are in `/tmp/misaka-rfc-payout-provider-market-pins-20261025.json`.

The terminal-native regression uses actual signed native replacement/Release/GC fixtures separately from the new small kernel-storage crash controls. SIGKILL demonstrates process interruption, not OS/power-loss durability. Consensus reopen uses the same outer database handle, not a full process/physical database restart. Orders in the native pipeline receive MARKET_MISSING refusals; no filled market, approved model, Safe/Finalized campaign, native39 mapping, K3/100k work, all-model memory bound or full RFC completion is claimed.

Documentation check: 1215 Markdown files, no new regressions, existing baseline 43 unchanged. This is not a strict clean-reference audit. `git diff --check` passed.
