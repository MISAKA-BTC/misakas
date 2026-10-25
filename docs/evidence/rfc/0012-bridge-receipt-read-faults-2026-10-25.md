# RFC-0012 bridge-visible receipt read faults — dedicated and related tests verified

This change concerns canonical receipt availability and identity, not execution of a new bridge transaction or market fill. No native39 settlement mapping, mature-work facts, K3 input, selected-verifier approval, model examination or campaign result is supplied by these fixtures.

## Existing contract and concrete consumer defect

The EVM design v0.4 §9.3 places a successful withdrawal's synthetic output in the accepting block's UTXO diff; §16 returns the receipt for an accepted/executed transaction and null for included-but-unaccepted or skipped transactions. The existing executor control at `misaka/evm/core/src/executor.rs` also establishes that the F002 withdrawal log is part of its committed receipt. A local reader failure is not evidence that such a transaction was skipped or absent.

`Consensus::get_evm_tx_receipt` previously unwrapped transaction-index, receipt and execution-header store reads. It also converted `is_chain_block` failure to false and searched an older index occurrence. The main `NodeEthProvider::transaction_receipt` then converted the public API's Err to None using `.ok().flatten()`. Consequently a canonicality fault could select an older receipt or appear as unknown/null, while malformed local rows could panic rather than report an unavailable read.

The native reader now maps actual index/receipt/header StoreError to the existing `ConsensusError::GeneralOwned` and propagates canonicality faults. Genuine unknown `HeaderNotFound`, genuine noncanonical false and defensive index/receipt mismatch retain their prior skip behavior. The transaction-index store's true missing row remains an empty index; a true missing receipt row remains absent. Existing true missing execution-header → legacy number-zero compatibility is unchanged; this bounded change does not certify that legacy fallback.

The main receipt RPC uses `with_canonical_receipt_v2`: API Err becomes an RPC server error before optional enrichment/rendering, true None becomes null without unused enrichment, and Some passes the exact receipt view. The existing optional base-fee lookup, raw transaction decode/metadata fallbacks and related native unwraps are not changed. Other receipt status/trace callers that still collapse Err, and the broader reorg log pump's empty-on-read-error/retry contract, remain separate debt. This is not a claim that all bridge/RPC error boundaries are complete.

## Dedicated controls

Consensus filter `evm_receipt_native_` registers four parent tests:

- Cold malformed transaction index is a read fault, distinguished from genuine missing/included-only indexes.
- Cold malformed receipts and execution header reach the actual production reader on an accepted canonical ancestor distinct from the current sink. An otherwise valid older indexed receipt cannot turn either failure into success.
- Cold malformed status, missing reachability and malformed reachability under the actual Separator-suffixed key cannot be treated as branch denial or silently select an older valid receipt. The extra status entries are explicit storage fault inputs, not accepted claims/blocks.
- Actual pipeline-inserted ancestor and sibling distinguish canonical from noncanonical. Serialized receipt/index rows exercise the production projection's exact accepting block, number, index, cumulative log offset and gas/status; unknown header, noncanonical side branch, index/hash mismatch and genuinely absent receipts preserve existing behavior. These are receipt storage fixtures on real L1 branch contexts, not evidence that their supplied EVM receipts executed on-chain.

Node filter `receipt_rpc_` registers three parent tests at the actual production continuation helper: API read fault invokes no enrichment/rendering continuation; genuine absence stays null without enrichment; a positive view retains its exact accepting identity, logs and metadata. There are no child-process counts in these seven dedicated parents.

The source checkpoint passed leaf formatting, production parse-to-stdout and diff checks. The parent subsequently ran the coordinated build and immutable runtime controls below. This worker started no Cargo or independent runtime.


## Actual build and runtime result

The coordinated build passed in 4m39s with all 15 frozen identities matching `/tmp/misaka-rfc-bridge-receipt-wave-source-20261025/record.json`. The receipt runtime used immutable Consensus `/tmp/misaka-rfc-bridge-receipt-kaspa_consensus-20261025` (SHA-256 `6d8842a6d54f9c7bdc64b3b7f010bfd3d9bb8a516d7e0b67c62322c3d19ef5c1`) and Node `/tmp/misaka-rfc-bridge-receipt-kaspad_lib-20261025` (SHA-256 `710591c618c93d729a3316769b8bec6ef9eb46544dfd41bd59317dcf331bda22`).

All seven dedicated parents passed: native receipt reader 4/4 (0.07s), RPC receipt helper 3/3 (0.00s). Related canonicality 3/3 (0.03s), native EVM readers 3/3 (0.03s), and all EthRPC tests 17/17 (0.01s) also passed. The EthRPC 17 include the three new RPC tests: there are 27 distinct passing parents overall, not 30. Logs are `/tmp/misaka-rfc-bridge-receipt-native-20261025.log`, `/tmp/misaka-rfc-bridge-receipt-rpc-20261025.log`, `/tmp/misaka-rfc-bridge-receipt-canonicality-20261025.log`, `/tmp/misaka-rfc-bridge-receipt-reader-20261025.log` and `/tmp/misaka-rfc-bridge-receipt-eth-rpc-20261025.log`.

After the build terminated and binaries were pinned, the two preexisting activation-metadata doc-comment lines were moved from the newly inserted receipt helper back to `with_executed_l1_daa`. The recorded source/binary identity retains the earlier comment placement; this subsequent change moves comments only and is semantically identical. No production behavior or tests changed after pinning. These passes verify the bounded read/error contract, not new bridge execution, market fill, historical signature replay, Safe/Finalized maturity or campaign approval.
