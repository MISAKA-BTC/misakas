# RFC06/07/08/12 court, auxiliary sweeps, provider provenance and round gas verification

## Actual changes

- Active-capacity court ingress admits dispute copies, pool/scheduler scratch and the fee's input copy before allocation. Session-row handoff is separately admitted; existing-row checks borrow. See [court admission](0006-court-admission-workspace-2026-10-25.md).
- Four actual pre-object auxiliary sweeps admit expired prompt keys, panel grouping/retirement buffers, due reporter reward copies and reporter commitment keys. Existing gates/collectors/award/order remain. See [auxiliary sweeps](0006-0008-auxiliary-sweep-workspace-2026-10-25.md).
- An additive SDK located failure carries exact unit-local decode/authentication coordinates to the actual private checker. It binds the current native root/provider/generation before the existing funded Challenge path; capacity/opaque/cross-unit failures invent no coordinate. See [provider provenance](0007-provider-unit-failure-challenge-provenance-2026-10-25.md).
- Actual native round gas is retained independently of market activation, checked before speculative-worker reuse, and committed by the empty fast path just as by full execution. See [round gas reproduction and repair](0012-evm-round-gas-production-parity-2026-10-25.md).

## Failed results preserved

Initial five-package no-run passed in249.97s. Court3/3, auxiliary5/5 and node-query2/2 passed; SDK located2/3 passed. Its new capacity test incorrectly expected no fetch before actual-frame workspace admission. Only that test was corrected to return its separately retained actual response, then require exactly one acquisition, LocalCapacity, no coordinate/action and unchanged ledger root. Production SDK behavior was not changed for this fixture repair. Initial log: `/tmp/misaka-rfc-court-aux-provider-round-provider-located-20261025.log`.

The unchanged-production actual native round test FAILED0/1 in2.15s after real drawn-seat signatures, Final/schedule/ticket, accepted signed round and positive distinct permit count. Its merging block reached StatusUTXOValid but stored30M gas instead of33M. The initial parent-reader/worker-context repair built in146.83s and passed all13 court/aux/provider parents, yet that native test still FAILED in2.27s. Both failure logs and source snapshots remain under the red/green names in `/tmp/misaka-rfc-court-aux-provider-round-*`.

The empty-execution optimization separately omitted the full executor's user_gas_cap clamp. Before changing it, the extended existing full/fast equivalence test FAILED at cap33M (0.01s harness) after a77.87s EVM-only no-run build. It covers four caps, two receipt-fence states and genesis/stateful parents (16 combinations within one parent), plus snapshot entry equivalence. This RED binary remains `/tmp/misaka-rfc-court-aux-provider-round-kaspa_evm-fast-path-red-20261025`, SHA-256 `58e36fefa344ee33bff41738119f575b9b0594806c36d942cb97ceefa16ec1b5`.

No failed result is counted as PASS. Neither the extended equivalence test nor the actual native test was weakened for the final repair.

## Final build and immutable executables

Six-package `cargo test --lib --no-run` (Core, Consensus, Node, EthRPC, SDK, EVM; Consensus/Node EVM and Node private-integration-config) **PASS 179.37s**, **31 source/config identities matching**. All referenced Rust was frozen through build and pinning and rechecked before this record. Manifest: `/tmp/misaka-rfc-court-aux-provider-round-wave-source-retry1-20261025/record.json`; build log: `/tmp/misaka-rfc-court-aux-provider-round-wave-build-retry1-20261025.log`.

- `kaspa_consensus`: `/tmp/misaka-rfc-court-aux-provider-round-kaspa_consensus-retry1-20261025`, SHA-256 `e68736cb9d1284e194c51461df0b2a86c6e55d8b9ca44237630d4dd2f5f9a6fd`.
- `kaspa_consensus_core`: `/tmp/misaka-rfc-court-aux-provider-round-kaspa_consensus_core-retry1-20261025`, SHA-256 `3c39c5d3670f00d0ab560e1184e4fd821d6c0f3457b95317ca3df46db6dced59`.
- `kaspa_eth_rpc`: `/tmp/misaka-rfc-court-aux-provider-round-kaspa_eth_rpc-retry1-20261025`, SHA-256 `ef30441547771b91f03f3ac9a044e3368cc1a49f5ec6a137384063ae4fd71629`.
- `kaspa_evm`: `/tmp/misaka-rfc-court-aux-provider-round-kaspa_evm-retry1-20261025`, SHA-256 `34073779db2639384cc8a18ae5d7a32079044ce3af9619d5bb4bf22b5fa783b2`.
- `kaspad_lib`: `/tmp/misaka-rfc-court-aux-provider-round-kaspad_lib-retry1-20261025`, SHA-256 `ef2dc12e5d1443589e917af1c6e32a8bc8ff7078ecdf6fe1feb86d5ec72be434`.
- `misaka_palw_sdk`: `/tmp/misaka-rfc-court-aux-provider-round-misaka_palw_sdk-retry1-20261025`, SHA-256 `3a65604c38d16e9eb0813008fd1c2928b2202f3e4c8c2c084858a87dfe54ffff`.

## Actual runtime

| Selection | Passing parents | Ignored | Harness seconds |
|---|---:|---:|---:|
| court-admission | 3 | 0 | 0.03 |
| aux-sweeps | 5 | 0 | 0.01 |
| provider-located | 3 | 0 | 0.19 |
| checker-failed-query | 2 | 0 | 0.00 |
| empty-fast-full | 1 | 0 | 0.01 |
| actual-native-round-gas | 1 | 0 | 2.14 |
| core-capacity | 4 | 0 | 0.01 |
| core-ingress | 2 | 0 | 0.00 |
| core-reporter | 15 | 0 | 0.02 |
| core-panel-clocks | 16 | 0 | 0.01 |
| core-prompt-retention | 12 | 0 | 0.05 |
| core-settlement-once | 21 | 0 | 0.03 |
| sdk-reader | 14 | 0 | 1.47 |
| sdk-actions | 5 | 0 | 0.10 |
| node-checker | 19 | 0 | 0.70 |
| native-market | 3 | 0 | 0.71 |
| native-round-lane | 8 | 0 | 60.11 |
| evm-executor | 31 | 0 | 1.29 |

All **14 new parents**, **one extended existing parent**, and **12 related groups** pass. Exact executable/test-name deduplication yields **158 distinct passing parents**, zero ignored. Subprocess children are inclusive. The actual native round test is also inside the round-lane group; the extended empty test is also inside the executor group; SDK/node overlaps count once. Exact controllers/logs: `/tmp/misaka-rfc-court-aux-provider-round-focused-results-retry1-20261025.json`, `/tmp/misaka-rfc-court-aux-provider-round-round-results-retry1-20261025.json`, `/tmp/misaka-rfc-court-aux-provider-round-regression-results-20261025.json`. Pins: `/tmp/misaka-rfc-court-aux-provider-round-pins-retry1-20261025.json`.

The actual round regression now accepts the untouched producer candidate, stores the exact33M cap for its actual one permitted round, and checks the execution commitment. The broader native group also passes the earlier market-active actual settlement/withdrawal-once/cold-reopen consumers. These are bounded native mechanisms over explicit harness keys, fixture trace roots and real selected-seat signatures, not real K3 inference or an independent deployment.

## Consensus compatibility and remaining full scope

Round gas repair is consensus-visible. An older binary actually accepted30M in the RED fixture. The current evidence does not establish that public history contains or lacks such blocks, or that historical replay is interchangeable across versions. Public-history compatibility and release/upgrade review remain necessary before deployment. No public binary replacement, new activation height or policy approval is performed here.

Court controls use an actual folded registry/attempt with explicit stored capacity-book/private fee inputs; they do not complete every legacy ingress/court/aux consumer. Auxiliary tests use explicit stored rows and independently target phase quotas; they do not authenticate those rows' whole admission history. Provider tests exercise local reader/action/binder consumers, not separate-process signed malformed-response/native Challenge acceptance. Logical typed reservations are not physical allocator/RSS or all-model cost measurements.

All-stage/four-adapter native acceptance, source independence, all reward-to-coinbase lifecycle, remaining material/process/wire/pruning/reorg/OS-crash and historical-signature campaigns remain outstanding. K3 formal inputs remain uncreated, native39-to-RFC12 settlement mapping unformulated. Public activation remains None; normal selected-source/V6 paid bootstrap refuses, unapproved economics stays dormant, frozen model tiers and selected-verifier approval remain unchanged. The 15 real campaign gates remain UNKNOWN. Whole RFC completion is not claimed.

Documentation check: 1236 Markdown files, existing baseline43 unchanged, new issues0; `git diff --check` passed. This is no-new-regressions, not a strict clean-reference audit.
