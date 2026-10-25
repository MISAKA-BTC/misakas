# RFC-0012: native EVM block readers preserve store errors

Status: coordinated no-run compilation passed; the three actual native DB reader tests passed.

The production `ConsensusApi` implementation for `get_evm_block_by_number`, `get_evm_block_by_rpc_hash` and their shared `get_evm_block_by_l1_hash` reader previously unwrapped store failures. The number reader also turned `is_chain_block` errors into successful absence. These three functions now map actual store errors to the existing `ConsensusError::GeneralOwned` and propagate canonicality errors through the existing `ConsensusResult`.

Genuine absent indexes/headers retain `None`. Missing immutable receipts still yield no transaction hashes; missing immutable raw transactions still contribute zero encoded bytes. Header-number mismatch and genuinely noncanonical number-index targets retain absence; hash lookup retains historical side-branch access. No native marker, clock, gate, EVM header authority or policy changes. Encoded-size overflow behavior is outside this change.

Three tests in `consensus/src/consensus/evm_reader_fault_tests.rs` use a real native node and its actual RocksDB-backed production API. Fresh cold keys receive malformed serialized number, RPC-hash, header, receipt and raw-transaction rows; the API must return a typed error. Other controls cover genuine absence, positive exact targets/size, optional immutable rows, unknown native block error, actual accepted sibling noncanonicality, historical hash access and a number/header mismatch. Reader-only typed EVM rows are deliberately written as store fixtures; these are not evidence of EVM execution or certified heads. Package/filter: `kaspa-consensus evm_native_reader_`.

## Remaining limits

The native `is_chain_block` call still reaches `validate_block_exists`'s status-store `unwrap`, and `ReachabilityService::is_chain_ancestor_of` still unwraps its result-returning implementation. This bounded correction propagates errors actually returned by those APIs; it does not repair their internal panic paths. Other EVM readers, allocator bounds, corruption recovery, historical-signature replay, real EVM campaign and pruned IBD remain outside this test. UNKNOWN acceptance gates stay UNKNOWN.

## Observed verification

The coordinated Core/Consensus/Node EVM plus private no-run job passed in **9m39s**, logged in `/tmp/misaka-rfc-shared-ids-native-evm-build-retry1-20261025.log`; all nine captured referenced Rust source hashes remained unchanged. The completed native Consensus binary was pinned as `/tmp/misaka-rfc-native-evm-consensus-retry1-20261025`, SHA-256 `bd32ff7ace6be756af2c84a541c47509c05b40bb02a7ae2382f7f037b4153769`. Its `evm_native_reader_` filter passed **3/3 in 0.09s**, recorded in `/tmp/misaka-rfc-native-evm-reader-tests-retry1-20261025.log`. These tests executed the actual cold-DB codec failure, genuine absence, positive and native side-branch controls described above; they establish this reader mechanism, not an EVM execution/certification campaign.
