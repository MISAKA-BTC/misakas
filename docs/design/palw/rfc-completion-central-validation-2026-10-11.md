# RFC06/07/08/12 central validation — 2026-10-11

Implementation and acceptance evidence are separate. All public presets retain dormant fences; no activation or production deployment is performed. The full completion goal remains open.

## User decisions

Fee-only Rounds consume a separate execution/fee Round cap. B_max counts reward-bearing blocks. Market fee plus issuance consume the actual recipient's R_max; signed collectible fee ceilings are reserved before a fill.

## Verified checkpoint: 3c731dd6a

Command (Rust 1.93, macOS aarch64, jobs 2, debug 0, incremental 0):

```sh
RUSTC_WRAPPER=/tmp/rfc7-strip-test-link.py CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked -p misaka-palw-sdk -p kaspa-consensus-core -p kaspa-wallet-core --features kaspa-consensus-core/evm --lib -- kernel_rows palw_bond_budget_v1 bond_budget_fold_v1 rfc0012_ --test-threads=1
```

Log: `/tmp/rfc07-rfc08-rfc12-recipient-wallet-v7.log`. Build 6m17s.

| Scope | Result | Evidence boundary |
| --- | --- | --- |
| Consensus core | 77 passed, 0 failed; 0.39s | Native issuer/Panel/EXEC recipient R holds, standalone kernel entitlement transfer, sparse unpaid collateral holds, budget fold/journal/replay/carriage, native readiness/settlement. These are core tests, not actual node or multiprocess evidence. |
| Wallet | 3 passed, 0 failed; 0.04s | Processor-owned DNS authority, actual maturity/iterator/generator refusal after loss of authority, delayed old DNS reply, unavailable observation clearing. Actual scan response generation binding is a further pending fix. |
| SDK | 4 passed, 0 failed; 0.00s | Complete bounded op-211 pagination and reconstruction of a real rooted route, cursor/completeness/resource and substitution refusals. Supplied RPC roots still need an authenticated chain view. |

The previous wallet run had two passing tests and one fixture failure (`GeneratorFeesInSweepTransaction`). The sweep fixture now uses the generator's existing `Fees::None` contract; selection rules were not weakened. The fixed test exercises the same actual generator.

Earlier independently recorded evidence remains scoped to its own head: 10 chunked snapshot protocol tests on 07eb36f78, three segment-room plus four complete-population integration tests on 1717a68cd, 43 core and four SDK cases on dcd2b2552, and 11 RPC wire cases on 15c7a1c7e. These do not establish 100k real-node load, decoded RSS, genuine pruned P2P or release approval.

## Integrated after that checkpoint: 740f7ca28

The partial court metadata authenticator now checks actual carried prompt, token/trace shape and trace-root recomputation before partial verification. It prevents matching root fields from authenticating substituted content. Its node-feature tests are pending central execution.

The feature-gated private node construction helper uses an explicitly salted test genesis, owned marked DB and loopback services; ordinary kaspad refuses its marked DB. It preserves public Args and activation refusals. Its build, guard tests and genuine process scenario are pending. The helper's D/W/concentration values are mechanics fixtures, not approved security/economic policy.

The actual EVM-enabled node Final/reorg, Round collectible fee, durable import session, staged-table corruption, generation and OS crash suite is running on 740f7ca28. No PASS is assigned before terminal results. Source fee logical vesting-to-native-queue R transfer and an additional delayed wallet scan response fix are pending integration; the current native batch predates those changes.

## Required continuation

Connect and verify every recipient income/export path, actual source queue drain/payment-plus-W, paid Panel/claim adapters, restart/reorg/pruned provenance and multiprocess drills. Close whole-model constraint/boundary/memory/routing coverage, measured worst public court/DA/carrier/capital costs, 100k population/resource/retention and genuine pruned P2P. Source independence/participation/censorship, cumulative retry soundness, 128-bit justification and external review remain open. Dormant fences are not evidence that these requirements passed.
