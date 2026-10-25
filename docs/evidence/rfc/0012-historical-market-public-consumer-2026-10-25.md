# RFC-0012 historical market public consumer — dedicated tests verified

This is the next connection after phase 1, committed as `ba6009c25`. Phase 1's four Core and six Consensus tests remain their own verified scope. The new public API/session/node/simulation connection passed its ten dedicated parent tests in the corrected retry; the related legacy regressions also passed on the same immutable source generation. It does not supply the undefined native39 settlement mapping, K3 inputs, selected-verifier approvals or any campaign gate.

## Concrete missing consumer and resulting behavior

`kaspad/src/eth_rpc.rs::snapshot_and_env_at` previously resolved the exact historical L1 block, its EVM header and its EVM state, then deliberately supplied no PALW view and inert market fences. Consequently a historical market read looked dormant even where the historical window was active. The new `ConsensusApi::palw_evm_view_at_owned_v2(target)` and manager-session forwarding retain the exact post-target projection into the historical `EthCallEnv`. `eth_call` and `eth_estimateGas` share that environment builder. Latest/pending continue through the existing raw head-view path; executor and trace retain their existing raw constructor. The EVM snapshot hot-read failure → validated reconstruction path is unchanged.

The default new API refuses as Unavailable rather than returning a false inactive result. Off ConsensusV2 returns None. On ConsensusV2 it reads the exact target header, resolves the existing fences from that header's actual DAA, and returns None only when the window is inactive. Active missing history, Store, Context, CapacityUnavailable and Resource are distinct until the RPC boundary; Resource preserves NativeCoverage rather than becoming a semantic invalid-block result. The node helper maps failure to an RPC server error and never invokes environment/simulation continuation on failure. Neither latest nor DAA zero substitutes for a historical failure.

## Locking, target semantics and import generation

The public adapter owns an actual reader-first pruning session across header, retained state walk and projection. A is after-A, matching the historical EVM snapshot. Its own header commits its parent state, so A.header's PALW root is not misused to authenticate the after-A projection. The real state walk still verifies its own retained/delta contracts and exact target point; the adapter also checks target block and DAA against the header.

A pruning reader does not prevent `begin_import` from changing epoch. The first compile-only version checked generation inside the state walk, before projection, and lacked the public adapter's final check. Review caught that incomplete boundary before dedicated runtime verification. The current adapter obtains the typed import status before the header, requires stable/no-active-import for an active window, then rereads the status after the projection and refuses any changed epoch, unfinished import or typed read fault. A completed view is dropped on this refusal. A single-use thread-local cfg-test callback invokes the real `begin_pruning_point_import` specifically after projection; it is not an instance-local production authority hook.

Dormant windows deliberately return None without an O(state) walk or projection, even if the preliminary import-status result is unavailable. Their immutable target header must still load. There is no active market consumer to authorize in that case. Active windows cannot take this shortcut.

## Simultaneous ownership and allocation boundary

The phase-1 input state and output view retain their existing admitted peak: input owner plus persistent projection plus construction scratch. Gate flags come from the same Params predicates as the old resolver, without allocating a full transition Extras or registry genesis-work copy. The output is `Arc<NativeRetainedV1<PalwEvmViewV1>>`; no bare Arc<View> with a detached sidecar ticket is introduced.

`EthCallEnv` has an explicit retained field alongside the existing raw field. Both supplied simultaneously is a setup refusal even below the fence. The simulator constructs one retained handler setup before allocating its setup storage, and `estimateGas` repeats simulation with the same retained view while each prior call's setup drops. The handler's private view-owner enum keeps the retained owner in every cloned revm callback.

A retained setup owns its allocation ticket and payload together in one Arc. Its persistent request covers the Arc/control envelope, fixed setup fields, exact-capacity facade tuples and the existing 128-action account-counter capacity. Facades sort in place without heap scratch. Descending original ordinal preserves std HashMap collect's last-in-view winner on address collision. There is no guessed std HashMap allocation ceiling and no unused legacy empty-map Arc allocation in the retained path. Legacy setup still uses the old HashMaps.

The retained constructor is crate-private and used by simulation with fresh counters, not by the block executor's committed-log resets. The existing intercept checks `queued >= MAX_MARKET_ACTIONS_PER_EVM_BLOCK` before recording an action; repeated/reverted subcalls still increment the same running count, so at most 128 successful records and 128 distinct account entries can coexist in this actual consumer. No new account-row cap or changed refusal policy is introduced. The existing `set_committed` counter semantics also remain available and are compared directly for both layouts; arbitrary externally supplied oversized action slices are not a newly claimed admission boundary.

This accounts the new PALW state/view/setup owners. It does not establish physical RSS bounds or full RPC allocation admission: existing EVM snapshot export/copy, CacheDB seeding, interpreter/ABI return buffers and unrelated raw latest/executor consumers remain outside this new accounting claim.

## Registered dedicated controls

Consensus `historical_market_native_api_` registers four parents: accepted BASE P→A with actual snapshots and public API→real revm `chainDaa()` difference plus raw-view parity; actual import generation change after projection; true missing versus cold malformed primary header; and an isolated active Resource versus dormant zero-state-work control. The private fixture explicitly declares its existing market/EVM fences and must pass full `validate_palw_v2`. It does not relabel BASE or insert mature-work facts. This is an exact target reader/simulation test, not a positive Safe/Finalized/native39 settlement campaign.

EVM `historical_market_retained_` registers four parents: real simulation/estimateGas parity with raw view and both-input refusal; direct 128-counter and committed-reset parity without capacity growth; artificial collision tuples passed through the production normalization helper against std HashMap's last-winner behavior; and child-process Q−1/Q/cloned-handler lifetime controls retaining both the view and setup tickets. Direct counter/reset controls are not claimed as a full reverted-contract/subcall campaign.

Node `historical_market_rpc_` registers two parents: the shared actual environment-continuation helper refuses all five typed failure classes and default Unavailable without downstream invocation; exact target identity/owner and genuine inactive None proceed. These adapter controls supplement the real native/revm path, not replace it. Child subprocesses are included in their parents, not additional acceptance counts.

The initial 11-file production vertical compiled in the parent's coordinated build (4m37s, 23 referenced identities unchanged); that compile preceded the additional outer generation check and registration of these ten dedicated tests. It is not runtime evidence for the corrected source. The next coordinated four-package build includes the corrected source and dedicated tests. This worker has run only leaf formatting, parse-to-stdout and diff checks; no Cargo or independent runtime was launched.

The first dedicated combined build (parent session 93072) exited 101. Its log is preserved at `/tmp/misaka-rfc-historical-wire-wave-build-20261025.log`. The historical fixture had one E0432: the existing registry address constant was imported from `evm` instead of its actual `evm::model_market` module. The test-only correction uses that existing constant and removes an unused HeaderStoreReader import. Seven other diagnostics belong to the separate wire-import group. No address, production fence or acceptance rule changed. Dedicated runtime remains pending.

## Dedicated runtime verified

The corrected four-package retry passed its no-run build in 2m16s. All 26 frozen referenced Rust identities matched `/tmp/misaka-rfc-historical-wire-wave-source-retry1-20261025/record.json`. The initial failed build's eight import/interface diagnostics (one historical fixture import and seven separate wire-group diagnostics) remain preserved in the initial log; they were compilation failures, not failed or successful runtime observations.

All ten new parent tests passed on the corrected immutable binaries:

- Consensus `historical_market_native_api_`: 4/4, 0.53s harness / 4.11s wall. Log: `/tmp/misaka-rfc-historical-wire-historical_market_native_api-tests-retry1-20261025.log`. Binary SHA-256: `c4ea75106a51d579a872fd025f0d97b28c2d30a11b03d6abc3a410e659058632`.
- EVM `historical_market_retained_`: 4/4, 0.02s harness / 0.59s wall. Log: `/tmp/misaka-rfc-historical-wire-historical_market_retained-tests-retry1-20261025.log`. Binary SHA-256: `678996bd2979f577e5b60ed71cae64de64ab52ef2de8f97b5966a79c4de67ea6`.
- Node `historical_market_rpc_`: 2/2, 0.00s harness / 3.84s wall. Log: `/tmp/misaka-rfc-historical-wire-historical_market_rpc-tests-retry1-20261025.log`. Binary SHA-256: `8e9ab013f585a4917f3792e139abd4ac9239bf02b39e2bb8a44c00845d5694fe`.

Subprocess quota controls are included within their parents. These actual results verify the exact BASE-backed historical reader → real snapshot → revm read-precompile difference, post-projection import refusal, typed source-fault behavior, retained setup admission/lifetime, raw parity and shared RPC continuation boundary described above. They do not establish nonBASE canonical head maturity, Safe/Finalized settlement, K3 or campaign approval. Appropriate legacy-related regression tests also passed on the same retry1 pins, as recorded below. No Rust changed after the frozen source capture.


## Related regressions verified

The same retry1 immutable binaries passed 23 related parent tests: existing `model_market::tests` 11/11 (0.00s), `sim::tests` 1/1 (0.03s), `evm_asof_` 6/6 (0.58s), Core `historical_evm_view_` 4/4 (0.03s), and factory orphan control 1/1 (0.00s). Logs are `/tmp/misaka-rfc-historical-wire-market-legacy-regression-retry1-20261025.log`, `/tmp/misaka-rfc-historical-wire-sim-legacy-regression-retry1-20261025.log`, `/tmp/misaka-rfc-historical-wire-asof-regression-retry1-20261025.log`, `/tmp/misaka-rfc-historical-wire-view-regression-retry1-20261025.log` and `/tmp/misaka-rfc-historical-wire-factory-orphan-regression-retry1-20261025.log`. The asof child (0.15s) and Core view child (0.00s) are included in their respective parent counts. These are related regression results, not additional dedicated vertical controls or campaign evidence.
