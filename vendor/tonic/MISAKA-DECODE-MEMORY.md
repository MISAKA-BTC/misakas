# Decoder scratch retention follow-up

This is a capacity-retention fix to the vendored tonic 0.14.5 decoder. It is not
an inbound allocation budget or a Node-wide OOM fix. No message-size setting,
wire format, consensus rule, or Core API changes here. The existing 320 MiB P2P
and 256 MiB snapshot settings remain available.

After a completed message, the existing detach operation copies at most the
configured scratch threshold (1 MiB, or a larger decoder buffer setting) of
unread wire suffix. A larger unread next frame stays intact until a later
completed boundary. A `ReadHeader`/HTTP-body `Pending` boundary may also release
oversized scratch while preserving partial next-header bytes. A partially read
message is not treated as idle and is not cleared. In particular, tonic still
reserves the announced message length after its five-byte header; an incomplete
`ReadBody` can retain that capacity while waiting for more input. This change
does not impose a connection-wide or process-wide admission limit on that path.

The decompression buffer contains only the current message, never subsequent
wire frames. After the decoder returns a completed item, oversized decompression
scratch can be dropped even if that decoder deliberately consumed only a prefix.
Any returned `Bytes` clone or slice still owns its original allocation. In
particular, a tiny slice can keep a large allocation resident: clearing scratch
does not prove that all memory for a message was freed.

Body errors, truncated EOF, truncated trailers, and normal terminal responses
now detach both scratch buffers. Errors are returned once and subsequent polls
are exhausted. A complete length header followed by EOF is a truncated message,
including when there are no payload bytes yet; trailers cannot hide a partial
message. Successful trailers and their metadata remain available.

The focused actual-source tests exercise plain and gzip large-to-small-to-idle,
incompressible gzip (both scratch buffers), split next headers/payloads across
`Pending`, large unread subsequent frames, held `Bytes` clones/slices, a prefix
consumer, body errors, malformed gzip, EOF, and trailers. Buffers are inspected
and checked for reclaimable old backing after the last returned alias is dropped.
The separate lightweight Cargo harness pins bytes 1.11.1 and flate2 1.0.34 to the
repository lock versions; the production workspace still needs its own locked
integration test.

Not covered by this fix: reservations before wire buffering/decompression or
protobuf heap expansion; full runtime schema-preflight integration; HTTP-body
and decoded-message owners outside these two scratch buffers; allocator/RSS
return behavior; or the still-unidentified aggregate anonymous heap growth.
