# Pruned-IBD PALW state transport: chunked, back-pressured (audit 2026-10-04, B-F2)

Status: design note; the interim change (below) is in lane PA's branch `rcore/post5300-audit`. Node/p2p only: no consensus rule, no params id.

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

Not done in lane PA: this needs a two-node drill (serve while a reorg moves the pruning point, drop mid-transfer, resume), which lane PA may
not run. Implementation order: proto + server flow, requester spill + streaming decode, then the drill.
