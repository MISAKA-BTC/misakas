# RFC-0012 latest/pending market view bound to exact executed head — verified dedicated and related tests

The preceding historical reader/retained simulation work is committed separately. This change closes the remaining node caller that combined a canonical EVM head with a different PALW tip. It neither defines native39 settlement mapping nor supplies K3, model examination, selected-verifier approval or campaign evidence.

## Production defect and bounded correction

`kaspad/src/eth_rpc.rs::head_snapshot_and_env` previously selected canonical `latest_head()` H and read H's EVM state, execution header and L1 DAA, but obtained its market view from `palw_evm_view_v1()`. That old API loads the current PALW tip T and resolves rows/fences at T. H and T are not guaranteed equal: after native retirement, canonical heads use the native settlement snapshot's latest pointer rather than unconditionally selecting the current sink. Combining H's EVM accounts/environment with T's market rows and fences can therefore produce an answer for no single block.

The node now calls a small production `canonical_head_snapshot_and_env_v2` builder in the existing guarded blocking session. The selected H goes to all state/header/DAA readers and to the already verified `palw_evm_view_at_owned_v2(H)` API. The resulting environment carries its retained owner; the raw view field is None. Latest and pending both continue to share this head environment builder. Executor/trace and other raw API callers are unchanged.

The existing typed exact-target RPC helper handles the view result. Only genuine inactive None creates an inert environment; Resource, CapacityUnavailable, History, Store, Context and Unavailable stop environment construction and downstream simulation. There is no attempt to use the raw current-tip API or choose a different head on failure. Missing/error canonical heads stop all subsequent reads. Missing actual execution state/header continues to fail rather than inventing an empty result. F003 still compares the selected H's actual L1 DAA with its existing fence.

This reuses the historical API's pruning/import-generation and retained view/setup contracts. It changes no Core API, policy, fence, wire format or consensus acceptance rule. Existing EVM snapshot export, CacheDB/interpreter/ABI allocations remain outside the retained PALW accounting claim.

## Dedicated controls

New Node filter `canonical_head_env_` has three parent tests using the actual production builder:

- A controlled canonical H differs from the reported PALW sink T. The raw-tip API panics if called. Exact H is observed at every production read, and the environment retains its owner after the reader and original environment drop. Real revm `chainDaa()` returns H's retained-view value; an explicitly controlled newer raw-tip view returns a distinct value. This pointer/view contrast is an adapter control, not an accepted maturity or settlement fact.
- All six typed view refusal classes stop the builder/simulation continuation. True inactive None alone succeeds with no market handlers.
- Error/None/empty canonical heads make no downstream state or view calls. Missing execution snapshot/header never reaches the view reader and cannot fall back to current tip.

Existing native accepted P→A exact-target API→actual snapshot→revm tests remain the appropriate native reader regression. Those do not create a positive nonBASE canonical-head or Safe/Finalized settlement result, and this node adapter fixture does not substitute one. Native39's formal settlement mapping and K3 remain unavailable.

Before the coordinated build this worker ran leaf formatting, Rust parse-to-stdout and diff checks. The parent's dedicated and related runtime results are recorded below; no Cargo or independent runtime was started by this worker.


## Actual verification

The coordinated four-package no-run build passed in 4m48s. All 20 frozen Rust/config identities matched `/tmp/misaka-rfc-head-harvest-gdn-wave-source-20261025/record.json`. The immutable Node binary is `/tmp/misaka-rfc-head-harvest-gdn-node-20261025`, SHA-256 `8e09f1f565a73ca37190764e6992092c138cb76526e017dabbb9b42ed53652e1`.

`canonical_head_env_` passed 3/3 in 0.01s harness / 4.04s wall; log `/tmp/misaka-rfc-head-harvest-gdn-head-tests-20261025.log`. The full EthRPC-related group passed 14/14 in 0.00s; those 14 include the three new tests and must not be added to them as separate tests. Its log is `/tmp/misaka-rfc-head-harvest-gdn-eth-rpc-all-regression-20261025.log`.

The existing actual native API regression passed 4/4 in 0.61s on Consensus SHA-256 `d7e5855bd8b30d8832366b7c6fbcf212edd63c0d48ca8ac2f44ce816e380c82e`. Its quota child (0.28s) is included in the parent count. `/tmp/misaka-rfc-head-harvest-gdn-native-api-regression-20261025.log` records that native accepted-target/read-precompile regression. No Rust changed for this evidence update. A separate harvest fixture-only retry is unrelated to these verified immutable results.
