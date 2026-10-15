# RFC06: experimental snapshot transport protocol107 — 2026-10-13

Protocol107 addresses the [proven native carriage capacity failure](rfc06-native-snapshot-capacity-2026-10-13.md)
by streaming the existing canonical format-1 snapshot envelope through one zlib
stream. It changes neither PalwStateCarriageV2 Borsh nor consensus root/activation.
New additive protobuf fields carry codec1, canonical encoded length and a
requester-chosen nonzero 32-byte nonce. The handshake advertises 107 and explicitly
negotiates 106 for older peers; those peers keep the existing raw/legacy payload,
hash domain and 384 MiB cap. Timeout does not choose another codec.

| Resource | Bound | Scope |
| --- | ---: | --- |
| Compressed wire total | 384 MiB | Same total byte allowance as existing snapshots. |
| Canonical encoded total | 512 MiB | Experimental 100k one-capability/four-population profile. |
| One wire chunk | 4 MiB | Ordered request/reply backpressure. |
| Process sessions | 2 | Shared sender/receiver/global resume-cache lease. |
| Session age | 600 seconds | Original lease, never renewed by resume. |
| zlib window | Fixed codec window | Streaming inflater; no whole expanded Vec or additional spill. |

The canonical allowance adds 111,970,912 bytes above the 424,900,000-byte lower
bound. It is not an all-model/all-native-state bound, decoded-native RAM bound,
production fleet approval or price. The full intended native carriage, registered
model declarations, actual decoder RSS, CPU/latency and genuine P2P/pruning still
need acceptance evidence.

Stream/chunk v2 hashes bind point, nonce, codec, canonical count and the prior
wire root/count/size/index fields. Changed profiles cannot resume another stream.
A compatible new peer reconstructs the same immutable compressed stream under the
same nonce. Negotiated raw/compressed mode changes discard the interrupted stream
and start a fresh acquisition. Unknown codec, malformed nonce and unnegotiated
metadata are refused. Serving charges max(canonical traversal, wire size), so a
small compressed reply does not permit cheap repeated large reconstructions.

The sender writes through compressor/bounded hash writer/private spill; the receiver
validates the complete wire hash and streams a bounded inflater into the existing
native framed reader. It checks exact canonical count, EOF, CRC and complete
consumption of a single compressed stream. Trailing junk/another stream, truncated
checksums, expansion past the declaration and policy excess return no native
snapshot. Unlinked spill/RAII session cleanup remain shared with protocol106.
Native consensus adoption still checks the pruning header's unchanged state root.

## Validation and remaining acceptance

The actual snapshot module compiled directly against existing node-target
core/hashes/Borsh/Tokio/UUID/flate2 libraries. All 16 tests passed, with no ignored
cases (2.46 s): the existing raw/frame/hostile-length/session/cleanup suite plus
compressed canonical equality/filtering, negotiation/caps, nonce/point/profile
binding, CRC/truncate/bomb/trailing-stream/exact-length refusals, and a genuine
multi-chunk compressed resume on an independently reconstructed identical peer.
A changed nonce refuses that resume. These are codec/consumer unit controls, not
actual multi-process network acceptance. [Raw results](evidence/rfc06-snapshot-protocol107-2026-10-13-tests.log)
and [source/executable/library hashes](evidence/rfc06-snapshot-protocol107-2026-10-13.json)
are retained. Full requester/server/proto flow type checking awaits the matching
node build.

`scripts/drills/rfc06_snapshot_wire_probe.rs` consumes the actual native four-seal
carriage dump and optional class-declaration Borsh file through these same sender,
chunk and receiver APIs. It reports exact wire/canonical counts, chunks, timings,
declarations and canonical equality. Build it directly against matching node
libraries. It installs no state and reports realP2pAccepted=false. Missing declarations
measure state-only transport, not the whole required declaration set. The actual
100k valid high-entropy public keys/operators/ordinary transaction hashes must be
measured; repeating keys or integer-word outpoints cannot approve the cap. That
measurement is still pending, so protocol107 has not yet closed native100k or
pruned-P2P/RAM/economic gates.
