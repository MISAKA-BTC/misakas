# Receive admission: required ownership cut, not a runtime safety patch

This is an implementation plan and two bounded counterexamples. No runtime
receive admission, wire policy, consensus rule, or peer rejection is changed by
this commit. The production source at 21c remains frozen separately. The
320 MiB P2P ceiling and 256 MiB EVM snapshot application ceiling stay unchanged.

`BudgetedProstCodec::decoder` in `utils/tower/src/transport.rs` still returns the
ordinary tonic-prost decoder and `Decode = U`. In `vendor/tonic/src/codec/decode.rs`,
`StreamingInner::decode_chunk` reserves the length announced by a five-byte
header before receiving its payload. `poll_frame` copies HTTP data into `buf`;
`compression::decompress` grows a second buffer before prost constructs decoded
Vec/String/Box storage. Completed/terminal scratch trimming does not admit any
of those allocations or retain a decoded-value ticket.

The existing `WirePreflight` cannot yet be activated for every valid message.
It refuses repeated singular-message/message-oneof fragments, whereas prost
merges them. The existing 256-by-three fixtures reproduce independent fragment
capacity sums below the actual merged Vec capacity. Reporting this unsupported
case as malformed, silently bypassing its admission, or imposing a small fixed
wire cap would not close the gap compatibly.

The following changes form one ownership cut. A codec-only or queue-only subset
must not be advertised as receive admission for these protocols.

1. Finish compatible schema preflight before selecting it at runtime.

   `utils/tower/build_wire_preflight.rs` and `src/wire_preflight.rs` need persistent
   logical-message field state across singular fragments. Same-tag oneof message
   fragments merge; switching variants replaces a value, including any overlap
   while the new value is being constructed. Repeated message elements have
   separate states; Box allocations and Vec/String capacity growth must retain
   their correct identity. Resetting state per fragment or multiplying each
   recursive child estimate is not an adequate merge model.

   If state cannot remain allocation-free, first count actual schema field/
   message occurrences using a checked, allocation-free wire pass, then reserve
   bounded inspector scratch before constructing its state arena. Do not size
   that arena from a giant multiplier on raw wire length: a legitimate single
   256 MiB bytes field needs almost no schema metadata. Required differential
   fixtures include singular/oneof 256-by-three merges, variant switching and
   switching back, recursive and repeated merges, packed/unpacked fields,
   unknown fields/groups, invalid UTF-8, depth and overflow. Successful prost
   decodes must not be refused as unsupported merely by enabling admission.

2. Add pre-allocation admission at every tonic-owned buffer boundary.

   The core-agnostic tonic `Decoder`/streaming seam needs opaque RAII owners, not
   a post-allocation size report. The process/connection adapter supplies bank
   reservations. Check before initial scratch allocation, length-header reserve,
   HTTP-frame copy, each decompression growth and prost decode. Account old and
   new capacities during reallocations, and simultaneous HTTP input, wire,
   decompressed and decoded storage. A bounded writer or explicitly admitted
   capacity replacement is needed where `BufMut::writer` currently grows output.

   An HTTP frame is already allocated by hyper/h2 before `poll_frame` sees it.
   Account that existing owner while reserving its copy; do not claim this alone
   prevents hyper/h2 ingress allocation. Connection creation/window/aggregate
   admission remains a separate bounded owner. Preserve unread next frames and
   partial frames. Budget failure must be immediate local pressure, not an await
   while holding buffers/locks that need to be released for progress. Test
   simultaneous receivers: filling all wire queues must not make every admitted
   decoder wait forever for an unreserved peak. Reserve progress headroom or
   abort/retry with demonstrable release; do not merely move the deadlock.

3. Keep buffer tickets with actual shared backing and decoded message owners.

   A `Streaming`-scoped or last-poll-scoped guard is too short. `DecodeBuf` can
   return `Bytes` aliases; clones/slices can retain a whole large backing after
   scratch detaches. Use a shared allocation owner analogous to encoded
   `RetainedFrame`, releasing bytes before its ticket. Sharing a guard is valid
   for aliases of one allocation; a physical protobuf clone/copy needs a new
   reservation. Decompression and wire backing have distinct tickets.

   The adapter needs a non-Clone `Inbound<U>` with private owned value/resources,
   and ownership-preserving extraction/mapping methods. Prost construction uses
   a pending schema-peak lease; after construction retain the actual owned
   capacity and release only scratch that has actually died. Cancellation,
   decoder errors and rejected routes must drop physical buffers before tickets.
   Do not expose an unrestricted `into_inner` escape that drops ownership.

4. Migrate generated services, routes and payload extraction together.

   `utils/tower/build_transport.rs` currently rewrites only outbound service
   types. Both generated clients' response streams and servers' request streams
   must carry the inbound owner, without changing protobuf field tags/types.
   P2P `core/connection_handler.rs`, `Router` maps, `IncomingRoute`, and
   `SharedIncomingRoute` currently carry plain `KaspadMessage` values. The
   production `unwrap_message!` / `unwrap_message_with_request_id!` macros in
   `protocol/p2p/src/common.rs` move out a bare payload. They must return an
   ownership-preserving payload and preserve request IDs. Keeping a lease only
   on the outer queue item fails at that exact extraction boundary.

   RPC has the same boundary at server `connection.rs` routing, handler queues,
   `request_handler/handler.rs`/method calls, and client resolver oneshot replies
   and notifications. Inbound request copies captured by blocking work must
   capture their owners too; cancellation of the awaiting future cannot release
   them. Public Rust response values that escape the resolver are another owner,
   not something covered merely by charging the receive queue.

5. Transfer into converted and accumulated native state, not just until dequeue.

   `protocol/p2p/src/convert/messages.rs` converts `BlockHeadersMessage` into
   `Vec<Arc<Header>>` and `PruningPointProofMessage` into per-level vectors plus a
   header-dedup HashMap (currently preallocating 4000 entries). `convert/header.rs`
   allocates native parent vectors while consuming protobuf parents. Admission
   needs typed count/capacity bounds before these conversions, accounting the
   original protobuf and the new allocations simultaneously. An ownership-aware
   conversion must retain old owners until their allocations die, and return
   owners for the constructed native data.

   `protocol/flows/src/ibd/flow.rs` retains converted proofs/chunks across receives
   and captures them in blocking validation/import work. Header Arcs can then
   enter storage caches and outlive the flow. The final cache/native owner must
   adopt or separately admit those allocations before the inbound ticket returns;
   retaining only an outer proof wrapper until `apply_pruning_proof` returns is
   insufficient. Arc aliases share a physical owner; constructing another Header
   or parent vector does not. This boundary requires the relevant consensus/cache
   owner API or an existing demonstrably equivalent bank; it cannot be closed by
   a tonic-only change. EVM snapshot bytes likewise must remain charged while
   native snapshot decoding/import materializes its independent state.

6. Preserve typed local failure through the real consumers.

   A local receive reservation failure should carry a local Rust error marker
   (not a peer-controlled string), map to `ResourceExhausted` for RPC and
   `LocalTransportCapacityUnavailable` for P2P/IBD, and release all partial owners.
   `Router` currently logs a streaming Status and closes; the waiting flow sees
   `ConnectionClosed`. That does not prove a bad chain, but it loses local/source
   fault distinction. Preserve the local close cause through incoming routes so
   candidate/source policy does not penalize a peer for local RAM pressure.
   Only actual malformed input is a protocol verdict. Re-test the real IBD
   candidate path: no InvalidProof/source quarantine/IP ban on local pressure,
   no critical event drop, and progress after cancellation/lease return.

A completed integration needs actual plain/gzip 256 MiB and near-320 MiB cases,
unknown/duplicate protobuf compatibility, many receivers/slow consumers,
large-small-idle/shared backing, pending phases, route saturation, queue drain,
conversion overlap, blocking-work cancellation, final cache ownership, and lease
return/progress. A small successful queue wrapper test is not that evidence.

## Standalone counterexamples

These opt-in tests intentionally fail the desired safety assertion. They use the
production codec/bank/generated messages. The extraction test compiles the actual
production `unwrap_message!` macro via the build script; tiny test types instantiate
only its unused error arms, without claiming to test protocol error classification.
The queue-only owner is an explicitly unsafe proposed implementation, not a
currently deployed wrapper.

```sh
MISAKA_MEMORY_BUDGET_BYTES=1048576 CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/tmp/misaka-transport-target cargo test --locked --manifest-path testing/transport-memory/Cargo.toml --lib inbound_gap_tests::exhausted_process_budget_must_refuse_plain_and_gzip_before_decoded_heap -- --ignored --exact --test-threads=1 --nocapture
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/tmp/misaka-transport-target cargo test --locked --manifest-path testing/transport-memory/Cargo.toml --lib inbound_gap_tests::queue_only_ticket_must_outlive_production_payload_extraction -- --ignored --exact --test-threads=1 --nocapture
```

The first case fills the process bank at 1 MiB, then observes each canonical
4097-header protobuf decoding to 2,555,904 heap bytes (schema peak 3,833,856).
Plain wire is 8203 bytes; gzip wire is 55 bytes. Both decoded payloads survive in
an accumulator after decoder/queue drop while the bank stays at 1,048,576 bytes.
No consensus block is submitted or judged. The second case observes a queue-only
80,608-byte ticket return to zero while the extracted payload still retains
80,184 heap bytes. Neither case deliberately exhausts host RAM. Evidence is in
`evidence/inbound-admission-gap-2026-11-03.log` and
`evidence/inbound-extraction-gap-2026-11-03.log`.
