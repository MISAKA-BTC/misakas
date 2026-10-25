# RFC-0012: native EVM selector API errors stay errors

Status: the node EVM test binary compiled and its four focused API-boundary tests passed. The coordinated multi-package no-run job and separate native DB reader tests were still pending at this observation.

`NodeEthProvider::account_at` previously discarded `ConsensusResult` errors from number, earliest and RPC-hash lookups with `.ok().flatten()`. Its unresolved historical selector returns no account, so balance/nonce/code/storage consumers could report successful zero/empty state after a native API failure. The historical `snapshot_and_env_at` consumer independently discarded the same errors and reported an unknown selector. Both also treated a failed `is_chain_block` read as a successful noncanonical decision.

Both actual consumers now enter one private `with_resolved_state_selector` continuation boundary. Number/earliest/hash API failures are server errors and stop the account/snapshot continuation. With `requireCanonical`, only `Ok(false)` produces the existing invalid-params noncanonical refusal; `Err` remains a server read error. Opting out retains the existing no-canonical-read behavior. Genuine `Ok(None)`, exact resolved L1 targets, earliest/number selection and existing canonical-head tag resolution retain their meanings. Latest/pending simulation still uses its existing separate head fast path.

The new `kaspad/src/eth_rpc_state_selector_tests.rs` leaf drives the production continuation boundary with a typed `ConsensusApi` test implementation. Four tests cover API failures and zero downstream continuation calls; real absence versus exact present target; canonical denial versus read error and opt-out; and exact latest/pending/safe/finalized/earliest source calls. The test filter is `eth_rpc::state_selector_tests`, package `kaspad`, feature `evm`. These are native-provider API-boundary controls, not a real EVM chain or campaign.

## Remaining limits

This change does **not** alter `consensus/src/consensus/mod.rs`. Its current `get_evm_block_by_number` and `get_evm_block_by_rpc_hash` implementations still contain store `unwrap` calls, and the number reader still maps a canonicality error to false. Failures hidden or panicking inside those readers cannot be repaired by this caller boundary. Other read-error fallbacks, historical snapshot reconstruction, simulation DAA-zero fallback and generic RPC argument parsing are outside this bounded change. No physical DB-fault campaign, historical-signature replay, model verification, approved policy or full RFC-0012 acceptance is claimed; UNKNOWN campaign gates remain UNKNOWN.

## Observed verification

The completed node binary was immutably pinned while the coordinated Cargo job continued other packages: `/tmp/misaka-rfc-native-evm-node-retry1-20261025`, SHA-256 `3687957a277006c7fc4fabf322cccf4425d01971e84c3ef9d5853bcc9e155a77`. Filter `eth_rpc::state_selector_tests` passed **4/4 in 0.00s**, recorded in `/tmp/misaka-rfc-native-evm-node-selector-tests-retry1-20261025.log`. Root verified all nine captured referenced Rust source hashes unchanged. This is the node API-boundary result only; it is not a completed integrated no-run result, execution of the separate native cold-DB tests, or an EVM campaign.

The coordinated multi-package no-run job subsequently completed **PASS in 9m39s**, logged in `/tmp/misaka-rfc-shared-ids-native-evm-build-retry1-20261025.log`, with all nine captured referenced Rust source hashes unchanged. The distinct actual native DB reader group also passed 3/3; its binary, scope and results are recorded in [the native reader evidence](0012-native-evm-reader-store-errors-2026-10-25.md). The earlier node four-test result is not counted again as additional tests.
