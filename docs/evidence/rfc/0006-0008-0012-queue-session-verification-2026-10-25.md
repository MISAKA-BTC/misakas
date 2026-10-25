# RFC-0006/0008/0012 consumed queues and blocking-session verification

## Implemented actual consumers

The shared pre-object phase now admits payout-prefix and EVM-settlement key buffers before collection. Each actual row writer still reserves its state/journal separately, with the existing payout/ACK/settlement order retained. Actual block fold/rehearsal and old-collector byte/root parity are checked. Existing private fee/refund economics controls now call this shared phase instead of a hand-copied payout loop. See [queue reservation](0006-0008-pre-object-queue-workspace-2026-10-25.md).

The actual owned consensus blocking helper now captures the whole session through its receiver method, including the pruning read guard. A cancelled awaiter cannot release that guard while its worker is still reading. Normal completion, cancellation and worker unwind use the same original tests. See [actual-lock reproduction and repair](0012-blocking-consensus-session-guard-lifetime-2026-10-25.md).

## Confirmed RED and unchanged tests

Before production repair, a manager-only no-run build passed in **63.09s** with frozen source identities matching. The actual helper/lock regression ran all three parents: normal return and panic PASS, cancellation FAIL. The worker had entered and was blocked on a real synchronous latch; caller abort was awaited to completion; polling the actual writer returned Ready before worker release. This establishes premature guard release without a timed-delay inference. Cleanup released the worker before the failing assertion.

The immutable RED executable is `/tmp/misaka-rfc-queue-session-kaspa_consensusmanager-red-20261025`, SHA-256 `1d0e4317b97170f857f5fae5a7acea17b0d7ed466cf0ac97a5dfaa2a1fb42959`. Result/log: `/tmp/misaka-rfc-queue-session-red-results-20261025.json`, `/tmp/misaka-rfc-queue-session-session-red-runtime-20261025.log`. It remains preserved and is not a passing result. The three tests were unchanged for GREEN.

## GREEN build and executable identity

Final five-package `cargo test --lib --no-run` (Core, Consensus, Node, EthRPC, ConsensusManager; Consensus/Node EVM and Node private-integration-config) **PASS 250.58s**, **17 source/config identities matching**. Rust remained frozen through build and pinning; source and executable hashes were rechecked before this record. Manifest: `/tmp/misaka-rfc-queue-session-wave-source-green-20261025/record.json`; log: `/tmp/misaka-rfc-queue-session-wave-build-green-20261025.log`.

- `kaspa_consensus`: `/tmp/misaka-rfc-queue-session-kaspa_consensus-green-20261025`, SHA-256 `d99985ee062866454f25801172c944515f123acb1fa58472e7e74f59e63b52c5`.
- `kaspa_consensus_core`: `/tmp/misaka-rfc-queue-session-kaspa_consensus_core-green-20261025`, SHA-256 `d9eb716b8e8ed4aa116fbd3e22ef5ebcbb6fb3c25bd01b65955b99e50b436f9d`.
- `kaspa_consensusmanager`: `/tmp/misaka-rfc-queue-session-kaspa_consensusmanager-green-20261025`, SHA-256 `2e6b398217452a44e42e7a63fe65bc3171abf6bce950ec9624c22172e04d8dcf`.
- `kaspa_eth_rpc`: `/tmp/misaka-rfc-queue-session-kaspa_eth_rpc-green-20261025`, SHA-256 `3a4090fa59e2e72202f237d934a871723a5f77f30fe4243ccf888a1342d10bb3`.
- `kaspad_lib`: `/tmp/misaka-rfc-queue-session-kaspad_lib-green-20261025`, SHA-256 `aef478aef38fe1b3d19f5b88e495397b3e1e0a78e765af81c79c09b52144f08a`.

## Actual runtime

| Selection | Passing parents | Ignored | Harness seconds |
|---|---:|---:|---:|
| queue | 3 | 0 | 0.02 |
| session | 3 | 0 | 0.00 |
| node-ethrpc | 36 | 0 | 0.03 |
| ethrpc-all | 31 | 0 | 0.27 |
| native-readers | 3 | 0 | 0.04 |
| native-canonicality | 3 | 0 | 0.02 |
| native-logs | 5 | 0 | 0.13 |
| core-native-writers | 9 | 0 | 0.02 |
| core-court-deadline | 2 | 0 | 0.01 |
| core-capacity | 3 | 0 | 0.01 |
| core-payout | 31 | 0 | 0.32 |
| core-pre-object | 2 | 0 | 0.01 |
| core-source-economics | 9 | 0 | 0.42 |
| native-parent | 4 | 0 | 0.07 |

All **6 new parents** and **12 related groups** pass. Deduplicating exact executable/test names gives **143 distinct passing parents**; no ignored tests count as passes. Subprocess children are inclusive. Exact filters/counts/logs: `/tmp/misaka-rfc-queue-session-focused-results-20261025.json`, `/tmp/misaka-rfc-queue-session-regression-results-20261025.json`; immutable pins: `/tmp/misaka-rfc-queue-session-pins-20261025.json`.

The economics group includes existing private principal refund and settled-fee queue ACK controls now connected to the actual common drain. These are explicit private fixture inputs, not approved public economics or live provider/coinbase acceptance. New queue parents themselves use dormant ACK and stored obligations/refused EVM rows; these scopes are distinct.

## Limits and full-goal residuals

The queue quote closes only its key arrays, not all payout/court/aux construction, physical allocator/RSS bounds or all-model costs. Session tests establish actual lock ownership under deterministic cancellation/unwind; they do not establish an OS crash/restart/pruned-IBD campaign or enforce an upper wall time on blocking work. A running worker still continues after its awaiter is cancelled and holds the guard until it exits.

Real work across multiple processes, K3 formal inputs, 100k scale, full wire IBD/pruning/reorg, OS crash, EVM Safe/Finalized mature-work, bridge/market fill and historical signatures remain separate acceptance. K3 inputs remain uncreated and native39-to-RFC12 settlement mapping unformulated. All 15 real campaign gates remain UNKNOWN. Public activation stays None, normal selected-source/V6 paid bootstrap refuses, unapproved economics remains dormant, frozen model tiers and claim-selected validator approval are unchanged. No full RFC completion or deployment is claimed.

Documentation check: existing baseline43 unchanged, new issues0; `git diff --check` passes. This is no-new-regressions rather than a strict clean-reference audit.
