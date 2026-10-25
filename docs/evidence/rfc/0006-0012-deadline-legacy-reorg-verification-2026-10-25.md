# RFC-0006/0012 court deadlines, legacy originals and reorg log verification

## Implemented consumers and scope

- Native court timeout sweep preflights the due sessions, associated claims and existing TIR phases before its first caller clone. Its transition construction owner retains the borrowed quote through verdict/rearm and downstream writers. New controls use a real folded disclosure transcript, Q−1/Q/retry and flags-off journal/root replay/revert parity. They do not establish the TIR Some-phase or service-bound archive campaign. See [court deadline caller workspace](0006-court-deadline-caller-workspace-2026-10-25.md).
- The two legacy TIR Source getters dispatch native callers to an exact scanned cold original-commitment decoder. Its retained owner covers the original during Source construction and checks. Length-prefixed claim identity streams canonical bytes. New cold-reader and identity controls do not claim an old ClaimBundle Preparation/Acquisition end-to-end, nor ownership of their derived legacy work plans. See [legacy original ownership](0006-tir-legacy-original-retained-reader-2026-10-25.md).
- The actual node log pump preserves a reorg notification on checked read errors, collects the whole immutable-hash batch before sending, retries with a delay/session release, and responds to shutdown/subscriber departure. Fault-injected ConsensusApi helper controls are not a newly accepted DAG/bridge reorg campaign. See [notification retry](0012-reorg-log-notification-retry-2026-10-25.md).

Public activation remains None, selected-source/V6 bootstrap refuses normally, unapproved economics stays dormant, frozen model tiers and selected claim-verifier approval are unchanged. K3 remains uncreated, native39 settlement mapping unformulated, and the 15 real campaign gates UNKNOWN. No all-model memory/cost or full RFC completion is inferred.

## Build and initial failed regression

The first four-package no-run passed in 283.15s with 19 source/config identities matching. All ten new parents passed. The first related run passed ten groups, but the broad deadline selection had 72/73 passes. Its old readiness-disclosure fixture expected RootNotCommitted at DAA3141 while its demand expired at1000. The same exact failure was reproduced on the preceding immutable Core binary before these changes: `/tmp/misaka-rfc-readiness-demand-expiry-prior-reproduction-20261025.log`.

The test alone now preserves DemandExpired for the old expired demand and checks RootNotCommitted with a separate demand valid through3141 at3141. Production expiry precedence, signatures, root standing and model policies are unchanged. This repairs an existing test input, not a new model review or approval rule. Initial focused/regression results and immutable pins are archived under `/tmp/misaka-rfc-deadline-legacy-reorg-*-initial-20261025.json`; initial failed logs remain preserved. No failed result counts as PASS.

Final coordinated retry1 `cargo test --lib --no-run` for kaspa-consensus-core, kaspa-consensus, kaspad and kaspa-eth-rpc with Consensus/Node EVM and private-integration-config **PASS 276.53s**, all **20 source/config identities matching**. Source manifest: `/tmp/misaka-rfc-deadline-legacy-reorg-wave-source-retry1-20261025/record.json`; log: `/tmp/misaka-rfc-deadline-legacy-reorg-wave-build-retry1-20261025.log`. Rust remained frozen through compilation and pinning, and hashes were rechecked for this record.

## Immutable executable identities

- `kaspa_consensus`: `/tmp/misaka-rfc-deadline-legacy-reorg-kaspa_consensus-retry1-20261025`, SHA-256 `c4f1ef6e90120afae6d7e5cb903d0c4bff2d6e1d5d9e2c28bf2227add66b9c43`.
- `kaspa_consensus_core`: `/tmp/misaka-rfc-deadline-legacy-reorg-kaspa_consensus_core-retry1-20261025`, SHA-256 `7e1310d6926e6f6a26e5f785ec3cb37ec6144062da37333012f0cdf10127be1c`.
- `kaspa_eth_rpc`: `/tmp/misaka-rfc-deadline-legacy-reorg-kaspa_eth_rpc-retry1-20261025`, SHA-256 `5c9391775bf95fd80be41cc645768dfc3bc03e56f8b83edee2121df04c9daa15`.
- `kaspad_lib`: `/tmp/misaka-rfc-deadline-legacy-reorg-kaspad_lib-retry1-20261025`, SHA-256 `875e1b0fe87bb3cb73ceb4a8e2b24c3ae55a7f5248580d940ec17ffa40fa54ce`.

## Actual runtime verification

| Selection | Passed parents | Ignored | Harness seconds |
|---|---:|---:|---:|
| deadline | 2 | 0 | 0.03 |
| legacy | 4 | 0 | 0.21 |
| reorg | 4 | 0 | 0.01 |
| node-ethrpc | 2 | 0 | 0.00 |
| node-exact-logs | 3 | 0 | 0.01 |
| ethrpc-all | 31 | 0 | 0.28 |
| native-log-faults | 5 | 0 | 0.16 |
| source-state | 5 | 0 | 0.17 |
| source-three | 4 | 0 | 1.80 |
| source-continuation | 3 | 0 | 1.40 |
| source-validation | 2 | 0 | 0.38 |
| court-deadline | 73 | 0 | 8.73 |
| court-backstop | 6 | 0 | 0.08 |
| court-silence | 11 | 0 | 0.12 |

All ten dedicated parents and all eleven related groups passed. Deduplication across overlapping filters gives **153 distinct passing parents**. Subprocess children are inclusive. The related deadline filter includes the two new court parents; they are not added twice. Exact test names were deduplicated across every selection. Exact filter/count/time/log controllers are `/tmp/misaka-rfc-deadline-legacy-reorg-focused-results-20261025.json` and `/tmp/misaka-rfc-deadline-legacy-reorg-regression-results-20261025.json`; immutable pins are `/tmp/misaka-rfc-deadline-legacy-reorg-pins-20261025.json`.

The node's preexisting unbounded notification backlog, WebSocket lag/backfill and best-effort newHeads reads/parent identity remain debt. A persistent log read fault can delay later events. Cancelling an awaited blocking task does not forcibly stop the underlying closure. Source legacy derived plans, other court/aux consumers, physical allocator bounds, OS crash, K3/100k work and live campaign evidence remain outside this checkpoint.

Documentation check: 1219 Markdown files, existing baseline43 unchanged, new issues0; this is no-new-regressions, not a strict clean-reference audit. `git diff --check` passed.
