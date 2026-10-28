# Pruned-IBD PALW state transport: chunked, back-pressured (audit 2026-10-04, B-F2)

Status: the original lane-PA design history is retained below. Protocol 106 implements bounded spill/reassembly,
stream identity, sequential backpressure and cross-peer resume. Genuine multiprocess pruning/IBD and native 100k
resource acceptance remain unproven. The experimental protocol107 profile separates compressed-wire and canonical
byte caps; its module tests pass, with full node/100k evidence still required. Node/p2p only: no consensus rule, no params id.

## Problem

`RequestPruningPointPalwState` is answered with ONE `PruningPointPalwState` message holding the whole borsh `PalwStateCarriageV2` plus every
class declaration. The requester refused anything over 64 MiB (`MAX_PALW_STATE_BYTES`, `protocol/flows/src/ibd/flow.rs`), the server had
no cap and served whatever it materialised, and the live carriage is already ≈ 57 MB and grows with every bond, class and claim. The day
it crosses 64 MiB every pruned IBD fails after a full transfer, from every peer, deterministically.

## Interim change (this branch)

* One cap for both ends: `PALW_PRUNING_STATE_MAX_BYTES = 384 MiB` (`protocol/flows/src/palw_gossip.rs`), a sixth of the transport's 1 GiB
  message limit, ≈ 6.7x today's state. It covers the carriage AND the class declarations (their total was unbounded).
* The server answers `found: false` (what an unprepared peer answers, so the requester moves to another peer at once) instead of sending
  a blob the requester would drop, and logs a warning with the size once the state passes half of the cap.
* The existing sidecar serve budget (`reserve_sidecar_serve` / `charge_sidecar_serve`) is the back-pressure on the serving side: one
  maximal serve per peer per window.

The single-message transport and the 384 MiB cap are a stopgap, not the design. Peak memory on the requester is ≈ 3x the message (protobuf
buffer, the `Vec<u8>` handed to borsh, the decoded carriage).

## Design: chunked transport

New p2p messages (protocol version bump, gated like `RequestPruningPointPalwState` is: sent only on a ConsensusV2 network):

* `RequestPruningPointPalwStateChunk { pruning_point_hash, state_root, index }` and
  `PruningPointPalwStateChunk { found, state_root, index, count, total_bytes, bytes (<= 4 MiB), chunk_hash }`.
* The first reply fixes `state_root`, `count` and `total_bytes`; every later request names the same `state_root`, so the server serves one
  materialised snapshot (cached for the IBD's lifetime, keyed by pruning point and root, bounded to one per peer and a short TTL) and a
  reorg between chunks cannot mix two states. A changed root answers `found: false` and the requester restarts from another peer.
* Back-pressure is the request/response itself: the requester asks for chunk `i + 1` only after it has written chunk `i` into a spill file
  under the datadir (not memory); the server holds nothing per chunk. Per-peer serve budget is charged per chunk. A chunk that does not
  hash to `chunk_hash` is the peer's fault (disconnect).
* After the last chunk the requester verifies `total_bytes`, reads the spill file with a streaming borsh reader into
  `PalwStateCarriageV2`, and verifies the carriage against the pruning point's own header exactly as `import_pruning_point_palw_state`
  does now. Peak memory is the decoded carriage plus one chunk. Resume after a dropped connection: `index` of the last written chunk,
  same `state_root`.
* Class declarations ride the same stream as trailing chunks (one entry per registered class, as now), so the "one per registered class"
  bound and the total cap apply to the stream.
* Old peers: a node that sees no chunk message registered answers nothing; the requester falls back to the single message while the state is
  under the interim cap (negotiated by protocol version, never by timeout).

Historical lane-PA outstanding work: this needs a two-node drill (serve while a reorg moves the pruning point, drop mid-transfer, resume), which lane PA may
not run. Implementation order: proto + server flow, requester spill + streaming decode, then the drill.

## Current byte accounting (2026-10-13)

Both negotiated modes retain the same 384 MiB total policy. The protocol-106 canonical stream costs
14 bytes for version/state-frame length/declaration count, plus carriage bytes and 68 bytes per declaration
for class id/byte length, plus each declaration's bytes. Legacy protobuf carries the metadata separately;
its sender/requester previously omitted those costs. A state accepted by that old metric could therefore
be deterministically refused by the chunked receiver at the same policy boundary.

The shared `snapshot_payload_size` now checks that canonical total on both modes. Unknown/duplicate/empty
declarations still consume wire headroom before the existing filtering rules. Legacy serving preflights
the complete size with a bounded streaming writer on the blocking pool before allocating its encoded state;
oversized state returns not-found. Valid legacy state bytes and native consensus-root verification are unchanged.

Chunking reduces live encoded buffers, but does not remove total state or decoded-native memory limits.
The engine's 256 MiB import bound is not a whole-carriage budget: native registry keys, other tables and class
declarations must fit beside every retained population. The actual funded 100k/four-seal native test must
measure that carriage and the genuine network transfer. Neither an accounting correction nor successful
small codec tests certify the 100k/pruned-P2P acceptance condition.

## Experimental protocol107 compressed profile

The [native capacity analysis](../execution/rfc06-native-snapshot-capacity-2026-10-13.md) proves that 100k one-capability bonds
and four 100k retained candidate populations need at least 424,900,000 canonical bytes, already over 384 MiB.
Protocol107 carries exactly the same canonical format-1 envelope through one zlib stream (codec1). It retains
the 384 MiB wire cap and sets a separate experimental 512 MiB canonical encoded cap, 111,970,912 bytes above that
lower bound. This is a profile for testing that shape; arbitrary native state, all registered model/decl shapes,
decoded native RSS and production operating costs still require their own limits and measured acceptance.

The requester chooses a nonzero 32-byte nonce. New stream/chunk hash domains bind the point, nonce, codec and
canonical byte count, plus the previous wire root/size/count/index fields. A new peer may resume only an
identically reconstructed stream under the same nonce/profile; a negotiated 106/107 mode change starts afresh.
Protocol106 and legacy peers retain their raw payloads, hash domains and total cap. There is no timeout-driven
downgrade. Unknown/changed codec metadata is refused before acquisition/continuation.

Sender serialization streams into the compressor and its private spill. The receiver streams from that
spill through a bounded inflater into the existing framed Borsh reader; no whole expanded Vec or second spill
is created. The existing two-session semaphore and original 600-second lifetime cover both versions. The
inflater enforces the declared byte count, requires exact EOF/CRC and rejects trailing compressed bytes or a
second zlib stream. Native adoption still validates the unchanged consensus root. Serving charges the larger
canonical traversal/wire total, so compressed data cannot make a large reconstruction cheap to repeat.

The [protocol107 record](../execution/rfc06-snapshot-protocol107-2026-10-13.md) retains scoped tests and hashes. The
actual high-entropy native 100k carriage must be captured and measured by `scripts/drills/rfc06_snapshot_wire_probe.rs`
with its declarations. A fixture whose public keys/outpoints repeat cannot approve this wire cap.
