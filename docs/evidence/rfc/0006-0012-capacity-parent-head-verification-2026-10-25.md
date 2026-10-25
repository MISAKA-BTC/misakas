# RFC-0006/0012 capacity sweep, immutable parent and head verification

The actual court-capacity sweep admits caller scratch before occupancy billing or collection. The native EVM parent reader uses the immutable selected parent, checks number/state-root continuity and refuses missing/corrupt referenced history. Actual block RPC and head-notification consumers propagate those failures; heads are collected completely and retried before publication. See [capacity](0006-court-capacity-sweep-workspace-2026-10-25.md), [native parent](0012-native-exact-selected-parent-2026-10-25.md) and [checked heads](0012-checked-block-parent-and-head-retry-2026-10-25.md).

## Build and fixture correction

Initial four-package compilation passed in 298.13s with 18 matching source/config identities. Seven capacity/head parents passed, but only three of four native-parent parents passed. The raw GhostdagCompact fixture omitted the level-zero namespace byte. Retry corrects this fixture and strengthens assertions: each intended corrupt store must actually return DeserializationError, and semantic controls begin with a valid selected-parent read and require the exact mismatch/missing-data error. Initial results/logs remain archived under `/tmp/misaka-rfc-capacity-parent-head-*-initial-20261025`; failed results do not count as PASS.

Final retry1 four-package `cargo test --lib --no-run` (Core, Consensus, Node, EthRPC; Consensus/Node EVM and Node private-integration-config) **PASS 154.06s**, **18 source/config identities matching**. Source record: `/tmp/misaka-rfc-capacity-parent-head-wave-source-retry1-20261025/record.json`; build log: `/tmp/misaka-rfc-capacity-parent-head-wave-build-retry1-20261025.log`. Rust remained frozen through compilation and executable pinning; identities were rechecked before this record.

## Fixed executable identities

- `kaspa_consensus`: `/tmp/misaka-rfc-capacity-parent-head-kaspa_consensus-retry1-20261025`, SHA-256 `be866023c46ac3b5ddbf00e52b3e5d179ce7fd697061ea25c2daed53396dc589`.
- `kaspa_consensus_core`: `/tmp/misaka-rfc-capacity-parent-head-kaspa_consensus_core-retry1-20261025`, SHA-256 `902bea0538c2571edaf7099883cf633018bd6ef293adfbaf8541e39acd360354`.
- `kaspa_eth_rpc`: `/tmp/misaka-rfc-capacity-parent-head-kaspa_eth_rpc-retry1-20261025`, SHA-256 `02605164e34f2790da93dbd798d2cc2a0b0234cbdb720a07d945678e12497d71`.
- `kaspad_lib`: `/tmp/misaka-rfc-capacity-parent-head-kaspad_lib-retry1-20261025`, SHA-256 `2072fae4ea6f1a2f7bd5a0c8e26fd959f0fc176f0a632e8e4420b818002133e9`.

## Runtime results

| Selection | Passing parents | Ignored | Harness seconds |
|---|---:|---:|---:|
| capacity | 3 | 0 | 0.03 |
| native-parent | 4 | 0 | 0.09 |
| head | 4 | 0 | 0.01 |
| node-ethrpc | 36 | 0 | 0.03 |
| ethrpc-all | 31 | 0 | 0.27 |
| native-readers | 3 | 0 | 0.03 |
| native-canonicality | 3 | 0 | 0.03 |
| native-logs | 5 | 0 | 0.16 |
| core-native-writers | 9 | 0 | 0.01 |
| core-court-deadline | 2 | 0 | 0.01 |
| core-court-backstop | 6 | 0 | 0.09 |

All **11 dedicated parents** and **8 related groups** pass, **102 distinct passing parents** after exact executable/test-name deduplication. The four head controls occur again in the Node group and count once. No ignored tests count as PASS. Subprocess children are inclusive. Exact filters/counts/logs: `/tmp/misaka-rfc-capacity-parent-head-focused-results-20261025.json`, `/tmp/misaka-rfc-capacity-parent-head-regression-results-20261025.json`; pins: `/tmp/misaka-rfc-capacity-parent-head-pins-20261025.json`.

## Limits and remaining work

Native parent controls use real accepted native branches with explicitly stored EVM headers, not newly EVM-executed/certified claims. Head controls inject ConsensusApi errors into actual renderer/retry/broadcast helpers, not a live bridge/reorg campaign. The reservation does not complete all court/admission/aux consumers or establish all-model physical allocator/cost bounds. The old unbounded notification backlog and client lag/backfill remain.

Dropping an awaited Tokio blocking task does not stop its worker. The common owned-session helper's guard lifetime on cancellation requires a separate actual-lock regression; this checkpoint does not claim cancellation-safe pruning exclusion. Normal awaited attempts release sessions before retry delay.

Activation remains None; normal selected-source/V6 bootstrap refuses; unapproved economics stays dormant. Model tiers and claim-selected verifier approval are unchanged. K3 inputs remain uncreated; native39-to-RFC12 settlement mapping remains unformulated; all 15 real campaign gates remain UNKNOWN. No whole-RFC completion or deployment is claimed.

Documentation check: baseline43 unchanged, no new issues. `git diff --check` passes.
