# RFC-0012 exact blockHash log consumer — runtime pending

This bounded change implements an existing log-query identity contract. It does not supply a bridge payment, market fill, mature useful-work fact, native39 settlement mapping, K3 input, historical signature replay, or campaign approval. Public activation and bootstrap fences, selected-verifier claim authentication, and the frozen model tiers remain unchanged.

## Concrete defect and authoritative contract

The actual `rpc/eth` dispatcher previously resolved `blockHash` with `block_by_hash`, took its EVM number, and called the canonical numeric-range reader at that number. A retained side branch could therefore return another block's canonical logs, and a reorg between the two reads could substitute identity. Explicit null/non-string `blockHash` also selected the numeric/default path. Ambiguous hash-plus-range filters were not rejected.

[EIP-234](https://eips.ethereum.org/EIPS/eip-234) requires the given hash's logs even for a retained fork, an error for an unknown block, and rejection of combined hash/range fields. The existing native `ConsensusApi::get_evm_block_logs(l1)` contract already reads immutable receipts without canonical filtering for detached-log delivery. Numeric range queries keep their separate canonical-only contract. This change follows those existing contracts; it does not grant a side branch consensus finality or credit.

## Production connection

An additive `ConsensusApi::get_evm_block_logs_by_rpc_hash` defaults to explicit unavailable error. Its actual implementation checks the RPC-hash index and exact execution header, verifies the mapped full L1 hash has the requested RPC prefix, and reads that block's immutable receipts. It never reads the mutable number index, computes unrelated block/raw-transaction metadata, or replaces an error with a different block. Genuine unknown/non-EVM identity remains None; a known empty EVM block is Some(empty). Immutable-header, receipt and hash-index StoreError reach the existing `ConsensusError::GeneralOwned` boundary.

`NodeEthProvider::get_logs_by_block_hash` calls this API through the actual session. Its shared production helper verifies all returned L1 prefixes before applying filters, so mismatched logs cannot be hidden as successful filtered emptiness. It preserves address OR, positional topic OR/wildcards, transaction and global log coordinates, and the existing 10,000-result error rather than truncation. The adapter's new provider method also defaults to explicit unavailable. JSON dispatch refuses ambiguous/wrong-type selectors, calls the exact provider method once, checks returned hash identity, returns an error for None, and renders a known empty result as an empty array. No hash-to-number fallback remains in this route.

Actual canonical range queries now use checked index-floor and posting-iterator siblings. A genuine absent completeness floor still permits the old full scan. A malformed floor, undecodable set member, malformed posting payload, number/header/receipt read error, or canonical status/reachability error cannot become a successful empty result or a full-scan success fallback. Genuine unknown HeaderNotFound and noncanonical false keep the prior skip behavior. Both indexed and scan paths map StoreError to the existing GeneralOwned contract. An indexed posting also rechecks the actual log address, preventing a stale/wrong-address posting from bypassing the address filter. Historical lossy store helpers remain for their compatibility callers; the actual query no longer uses them.

## Registered controls and status

Twelve dedicated parent tests are registered; runtime is PENDING and no worker Cargo/runtime was started.

- Consensus `evm_logs_native_` — five parents. Actual pipeline-accepted L1 ancestor and sibling at the same EVM number prove exact side-hash identity versus canonical scan/index selection; duplicate addresses and wrong-address postings are controlled. Cold malformed hash index/header/receipts plus a valid alternate number-row result cannot mask an error; true unknown and known-empty remain distinct. Scan/index header/receipt failures, cold number error, malformed completeness floor, serialized-member envelope error, malformed posting payload, and status/missing-reachability failures exercise the actual production readers. Receipt/header/index rows are explicitly storage fixtures on native L1 contexts, not evidence that those EVM logs executed.
- Node `exact_hash_logs_node_` — three parents. Actual production helper controls cover API/default unavailable, None/known empty, mismatched sibling identity before filtering, exact address/topic/coordinate handling, and 10,000/10,001 result behavior after filtering. Wrong numeric-range use panics in the mock rather than passing.
- `kaspa-eth-rpc` `exact_hash_logs_dispatch_` — four parents. Actual JSON dispatcher controls check exact hash/filters and no number/block lookup, fault/None/mismatched result refusal versus known-empty success, pre-provider ambiguous/type/size rejection, and unchanged explicit numeric range dispatch.

Static leaf formatting, production parse-to-stdout, and diff checks passed before the source freeze. Coordinated parent build, pinned binaries, focused and related regression results will be recorded after execution. These twelve parents are not a real EVM campaign or authenticated-work test.

## Explicit remaining boundaries

The broader subscription/reorg log pump still has its previous empty-on-read-error/retry behavior; this change does not certify reliable removed-log delivery. Immutable decoding/output collections and RPC formatting are not newly covered by full allocation admission or retained-owner lifetime accounting. Validly decoded but incomplete/corrupted content is not cryptographically authenticated by this reader, and read failure does not authorize a new repair/fallback policy. Numeric query multi-read consistency and all mutable head reads are not promoted to a new atomic snapshot guarantee. EIP-234 unknown errors use the existing server-error envelope, without adding a new optional JSON error-data field. Safe/Finalized maturity, actual bridge/market execution, formal native39 settlement mapping and campaign gates remain open.

## Final coordinated verification

Exact-log twelve dedicated parents, whole Node EthRPC28 and whole kaspa-eth-rpc31 PASS. Six-package retry1 no-run PASS in 186.13 seconds with 40 identities matched. Initial missing test import failure is preserved. Earlier PENDING statements describe the preparation checkpoint and are superseded by this terminal result. [Pinned executables, exact counts, logs and scope](0006-0012-source-cone-replay-logs-verification-2026-10-25.md). No campaign status is promoted.
