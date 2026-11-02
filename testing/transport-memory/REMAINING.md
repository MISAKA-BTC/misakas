Transport memory coverage and remaining owners (2026-11-02)

Implemented:

- P2P output count ceiling 1024, RPC client request count ceiling 256, existing
  RPC server output count ceiling 1024. Generated capacity traversal charges
  retained Vec/String/Box storage, not just wire bytes.
- Process and connection admission includes the simultaneous encoder peak at
  queue acceptance. An accepted queue item needs no further byte admission to
  encode. Errors are immediate; no consensus lock is held awaiting admission.
- Legacy PALW snapshot serializations reserve before construction, transfer the
  same ticket without a second global raw charge, and keep native/session/disk
  ownership through the final HTTP Bytes owner, including cloned/sliced frames.
  Serialization now uses an exact-capacity bounded writer; declaration clones
  check their canonical remaining cap first, and class-count-derived metadata is
  admitted before collecting them. Blocking work captures both generation and
  session/native guards, including cancellation of its awaiting flow.
- Oversized receive/decompression scratch returns at completed-message boundaries
  while preserving unread next frames and separately shared decoded Bytes.
- RPC pending maps have a 4096 count ceiling. Legacy request clones reserve their
  own retained bytes before copying. Admission/try_send failures undo pending
  registration. Sent cancellations preserve the existing matching position until
  response/timeout; they do not remove it and misassign a late same-op response.
- Local transport pressure is a retryable local-resource condition in the IBD
  candidate path. It does not mark a valid chain InvalidProof.
- With the retained UTXO service API, gRPC get_utxos_by_addresses keeps Rust and
  protobuf allocations separately charged during conversion, then transfers the
  same proto ticket into the queue/HTTP owner. The production quote is compared
  with actual generated-message capacity at 0/1/4097 entries and long scripts.

P1 incomplete ownership boundaries:

1. `vendor/tonic/src/codec/decode.rs`: receive growth, decompression growth, and
   decoded protobuf heap expansion still need schema-aware admission before
   allocating. The current decoder capacity fix releases completed large scratch;
   it does not limit simultaneous receive/decode allocations across connections.
   Generated `WirePreflight` now inspects the fixed schemas before allocation and
   has independent decode/capacity fixtures, but runtime reservation and transfer
   into an inbound owner have not been connected. It bounds logical capacity;
   allocator metadata, fragmented resident pages, stack, input and decompress
   backing remain separate owners/costs.
   Duplicate singular/message-oneof wire fragments currently return explicit
   unsupported errors: prost merges their nested repeated fields, and summing
   independent fragment capacities can undercount the merged allocation.
   Runtime integration must preserve that protobuf compatibility or explicitly
   negotiate/document a narrower encoding; no such rejection is deployed here.
   Budgeted encoded frames discard even sub-1-MiB scratch aliases before their
   tickets return; baseline codec buffers/connection metadata remain outside the
   audited message owners and need aggregate admission at connection creation.
2. P2P incoming routes and RPC incoming handler queues still own plain protobuf
   values. Their decoded allocations and flow accumulators need tickets that
   travel into the final state owner; holding a ticket only until dequeue fails
   for IBD accumulators that survive multiple receives.
3. `rpc/service/src/service.rs`: reply construction for get_blocks,
   get_mempool_entries, get_headers and related methods
   occurs before output admission. Large RPC construction must reserve from
   schema/request-derived bounds or use negotiated streaming, before materializing.
   EVM/overlay snapshot serving likewise constructs before the generic output
   ticket; the explicit generation ticket currently covers legacy PALW only.
   PALW source record reads/cache insertion and declaration validation scratch
   remain separate owners. Chunked/compressed snapshot declaration collection
   still uses the older getter and needs the same construction-ticket seam.
   get_utxos_by_addresses is covered through the retained gRPC path after the
   service/gRPC integration; its legacy wRPC/direct Rust serializer returns a
   plain response and still lacks the end-to-end serializer/transport ticket.
4. `notify/src/notifier.rs` notification channel, `notify/src/broadcaster.rs`
   control channel, `notify/src/subscriber.rs` mutation channel, and the RPC client
   notification channel remain unbounded. Critical events need durable cursor/
   replay or a proven bounded backpressure path. Simply dropping on overflow or
   awaiting a bounded sender under a consensus lock is not a safe repair.

P1 existing RPC compatibility issue outside this memory patch:

The legacy QueueResolver matches many replies only by operation. Its existing
timeout removal can let a much later reply match a subsequent same-op call. A
proper repair requires request/connection generation management or closing and
rebuilding the legacy stream; this patch does not claim to resolve that issue.
Its new pre-send cleanup is disarmed immediately after successful queue send.

Evidence scope:

`evidence/large-frames-2026-11-02.log` records real generated P2P EVM messages and
tonic 0.14.5 round trips at 256 MiB and near 320 MiB, plain/gzip. Accounted
simultaneous envelopes were 1,073,939,029 and 1,342,370,389 bytes; final tickets
returned to zero. These are admission accounting observations, not RSS samples.
The near-320 MiB payload leaves 1024 bytes for protobuf framing under the unchanged
320 MiB transport limit. The 256 MiB EVM payload itself is unchanged.

The small harness and private tonic tests cover slow ownership, cancellation,
preallocated snapshot purpose/pending phases, queue saturation, encoding progress,
large/small/idle scratch capacity, shared Bytes, and unread next frames. They do
not establish whole-Node OOM safety, Court deadlines under load, IBD/reorg combined
operation, native maximum-context execution, or multiple deployed processes.


Receive follow-up (2026-11-03): the exact implementation cut and bounded known-
failing reproductions for items1/2 are recorded in `INBOUND-INTEGRATION.md`.
Production codec/route/peer policy is unchanged. Connecting the current inspector
would reject valid duplicate message/oneof encodings; wrapping only an incoming
queue item returns its ticket before the production unwrap macro's payload dies.
