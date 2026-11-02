# getBlocks generation admission — remaining P1, next-candidate design

Status: investigation only. This design is **not implemented or covered by the first candidate**. First-candidate source is frozen while the parent runs real Node/Consensus test binaries. No new Consensus API, raw bincode projection, RPC adapter or traversal policy is added by this document. The UTXO cancellation repair is a separate completed ownership correction.

## Current ownership and allocation evidence

| Stage | Current source and behavior | Why queue-only admission cannot cover it |
| --- | --- | --- |
| Hash segment | `rpc/service/src/service.rs::get_blocks_call` calls `get_hashes_between`; `consensus/src/processes/sync/mod.rs::antipast_hashes_between` allocates segment vectors, EXEC membership, parents-first output and aggregate hashes. | These allocations precede RPC response/protobuf/transport admission. The configured `max_blocks` works at mergeset granularity and the first EXEC segment is taken whole. It is not a total byte bound. |
| Sink anticone | If the segment reaches sink, `get_blocks_call` calls `get_anticone`, builds a dedup HashSet and appends the filtered anticone. `Consensus::get_anticone` passes `None` for maximum traversal. `traversal_manager.rs::antipast_traversal` holds output Vec, BFS queue and visited HashSet. | Anticone and BFS working sets are additional to the segment bound and can grow before service sees the result. A response budget acquired after this call does not cover those allocations. |
| Block source | `Consensus::get_block_even_if_header_only` reads header, `BlockTransactionsStore::get`, and owned EVM payload, regardless of RPC `include_transactions`. | `BlockTransactionsV3` warm clones share `Arc<Vec<Transaction>>`, but a cold `CachedDbAccess::read` can deserialize the full body before any RPC reservation. Existing cache sizing is not an audited exact admission bound. |
| EVM source | `DbEvmPayloadStore` caches `EvmExecutionPayload` by value. `CachedDbAccess::read` clones it; the result is then wrapped by `Arc::new` in the block getter. | Even a warm EVM lookup can deep-copy system ops, transaction byte vectors and extra data. The Arc wrapping occurs after that copy. |
| Rust header | `RpcHeader` expands `CompressedParents` into `Vec<Vec<BlockHash>>` and copies `palw_commitment`. | A compressed run can represent multiple parent levels (cumulative count is u8). Serialized row length alone is not a justified upper bound for expanded lists. Count actual expanded parent entries/levels. |
| Rust transaction | `ConsensusConverter::get_transaction` collects inputs/outputs and clones payload/signature scripts/script public keys, with verbose hashes/mass/output addresses. | The source body and the independently owned Rust output can coexist. A reservation after `RpcBlock` exists is too late. |
| Rust EVM | `ConsensusConverter::get_block` calls `EvmExecutionPayload::payload_bytes`, producing a Borsh Vec even when UTXO transactions were not requested. | The owned EVM source and its canonical byte representation coexist. This field is actually returned and must be retained by a no-UTXO-body optimization. |
| Metadata | Converter fetches owned external ghostdag data, a children Vec, lane strings/lineage and classified PALW merge vectors. Merge classification reads member headers. | Children count is not the block's direct-parent count. Cold metadata/header reads and their copies also precede the final aggregate. Rust-only serde fields and protobuf fields must be distinguished when quoting copies. |
| Protobuf | `rpc/grpc/core/src/convert/block.rs` creates independent header/transaction/verbose messages and clones `evm_payload`. Header/tx converters format Hash64s and hex strings. | Rust and protobuf coexist. `to_rpc_hex` currently holds a byte Vec and its copied String together. The codec and HTTP owner are later stages, covered only when the preallocated ticket actually reaches A's adapter. |

No assumption here attributes the observed anonymous RSS to one of these owners. They are concrete source allocation boundaries requiring protection.

## The cached transaction ID is actually persisted

This point was checked in source, rather than assumed:

- `Transaction` in `consensus/core/src/tx.rs` derives serde `Serialize`/`Deserialize`. Its cached `id: TransactionId` field is not skipped. It follows `mass`; the source comment says to use `id()` for consumers, not that the field is absent from the codec.
- `BlockBody` in `consensus/src/model/stores/block_transactions.rs` is a serde record around `BlockTransactionsV3`.
- `BlockTransactionsV3` in `consensus/core/src/block/transactions_v3.rs` is serde-transparent over `Arc<Vec<Transaction>>`; its local allocation ticket is skipped. An existing codec test checks this has the same binary representation as the old transaction vector.
- Consequently an ID projection can preserve the **stored cached IDs**, matching the current getter. It must not recompute IDs as an unsolicited output change.

A bounded serde visitor over a pinned binary row can walk transaction records with borrowed payload/signature/script byte slices, skip input/output sequences without collecting them, and retain only IDs. Input/output counts and dynamic byte totals can be accumulated at the same time. It still needs a count-derived reservation before the ID Vec is allocated. The actual pinned-row codec must be tested against normal body decoding, including zero transactions, long payloads/scripts, nested collections, malformed/truncated lengths and Hash64 IDs. This is a feasible next implementation, **not yet a validated decoder**. It must not silently fall back to a full body allocation while claiming projection coverage.

## Minimal next-candidate interface and scope

1. Reuse `RetainedRpcReply<GetBlocksResponse>` with one internal `RpcApi::get_blocks_retained_call` default helper. Keep public response structures, RPC operation identities and wire formats unchanged. A's gRPC handler can retain Rust output while constructing protobuf and transfer the protobuf ticket through `enqueue_preallocated`, as for UTXO. wRPC needs its own serializer/queue ownership integration and remains separately unclosed.
2. Add at most one focused read-only Consensus projection method for transaction IDs / body shape. The production implementation lives in new store/parser helpers; the small trait/delegate additions belong near the existing block getters, outside A's snapshot-getter hunk. Existing fallback/mock implementations can preserve old behavior, but that fallback is not covered source admission.
3. For `include_transactions=false`, use the ID projection plus the existing header/EVM reads. Preserve `verbose.transaction_ids`, header/PALW fields, status, ordering and EVM payload. Avoid materializing UTXO inputs/outputs/payload merely to report IDs. Cold header and EVM source allocation are explicitly still open until their store boundary is admitted.
4. Admit new `RpcBlock` and protobuf copies from actual typed counts/lengths **before conversion**, and reserve aggregate Vec/hash storage before extending it. Keep original source, Rust output and protobuf owner distinct through simultaneous phases. A count/length measured after the source getter may bound the *next conversion allocation*; it does not retroactively cover a cold source allocation.
5. A complete source path subsequently needs pinned schema inventory for cold body/header/EVM rows, pre-copy ghostdag/children admission and source tickets that remain with any cached retained allocation. It must reserve before decode/deep-clone and revalidate the exact admitted shape before reading/building a changed row. A cache Arc clone and a new independently decoded body are different cases. Source/cache tickets cannot be discarded while newly allocated cached bodies remain live.
6. Separately admit hash traversal's queue/visited/output growth at its concrete insertion boundaries. Include first-segment EXEC members, dedup working set and sink anticone. Use the same total byte bank and immediate local-capacity refusal; do not turn a traversal/RPC memory refusal into a consensus invalid-block verdict.

A fixed giant multiplier over DB bytes, an arbitrary small RPC page count, or a 4 MiB message ceiling is not a substitute for these inventories. Preserve the existing legitimate 320 MiB P2P / 256 MiB snapshot compatibility. Any use of consensus block mass/size bounds must be derived for the actual historical block/ruleset and all payload families; current configuration alone does not establish a historical allocation bound.

## Required evidence before claiming coverage

- Exact result/order parity with the existing `getBlocks` response below resource limits, both include flags, header-only blocks, mixed UTXO/PALW/EVM payloads and sink anticone addition.
- ID projection parity with the persisted codec, with payload/script bytes remaining borrowed, no deep source-body clone for the no-transactions path, and explicit malformed-codec errors.
- Actual Rust/protobuf capacity checks against shared production quote functions, including compressed parent expansion, long scripts/signatures/PALW commitments and EVM bytes.
- Low RAM refusal before each protected construction, progress after release, and a small control request remaining admissible. At least one legitimate large transfer must succeed under adequate budget.
- Cold cache vs warm cache, concurrent cache misses, source/cache last-owner lifetime, and cancellation at each blocking/async boundary. Do not repeat the UTXO issue where only the caller future held a blocking job's input ticket.
- Body/header/metadata read failures remain local/store errors. Existing chain state, claim approvals, model tiers, roots, court decisions and block validity are unchanged.
- Traversal/anticone stress and realistic Explorer filler requests alongside large/slow transports. An absence of OOM in a short run does not prove all node allocations are bounded.
