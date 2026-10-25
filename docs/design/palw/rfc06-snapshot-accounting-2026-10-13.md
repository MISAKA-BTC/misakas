# RFC06: pruning snapshot accounting — 2026-10-13

Legacy pruning-state sender/requester counted only state bytes plus declaration
bytes. Protocol 106 counted the canonical envelope: 14 bytes of format/state
length/count and 68 bytes per class id/declaration length, plus those data bytes.
Thus a legacy-accepted snapshot near the 384 MiB total cap could deterministically
exceed the chunked cap. The legacy sender also allocated its whole encoded state
before discovering an oversized response, and serialized it on the async flow.

All three consumers now share `snapshot_payload_size`, with checked arithmetic and
Borsh declaration length/count checks. The legacy server preflights with a bounded
streaming size writer inside its existing blocking snapshot operation and only
encodes an accepted state there. Oversized state returns not-found before allocating
its encoded Vec. After encoding, the actual sender checks/charges the same total
that the requester checks before decoding. Unknown/duplicate/empty declarations
consume their header space before existing filtering. Legacy state bytes remain
unchanged; native adoption still authenticates the header's consensus state root.
The 384 MiB total cap has not been increased.

## Validation

The actual `palw_snapshot_chunks.rs` module was compiled directly with `rustc --test`
against existing node-target core/hashes/Borsh/Tokio/UUID rlibs. All 11 module tests
passed (9 existing, 2 new), including framing, hash/EOF, ordered chunks, interrupted
cross-peer resume, expiry, cancellation, unlinked spill and hostile Borsh lengths.
The new actual canonical fixture measures 544 bytes under the old field sum versus
762 canonical bytes: an omitted 218 bytes for three declarations. Shared accounting
matches actual serialization. Numerical controls cover the exact cap and one-byte
excess, extra empty declarations, 1,024 empty headers, zero state, integer overflow
and impossible Borsh lengths without allocating a near-cap buffer.

[Raw results](evidence/rfc06-snapshot-accounting-2026-10-13-tests.log) and
[source/executable/dependency hashes](evidence/rfc06-snapshot-accounting-2026-10-13.json)
are retained. Full sender/requester flow type checking awaits the integrated node
build; these module tests are not a genuine multiprocess pruned-network result.

## Capacity and model-input audit

The engine's 256 MiB limit bounds only Panel state. Native bond public keys, other
state tables and class declarations accompany the retained population. Protocol 106
still has a 384 MiB whole-payload cap; chunking does not prove that the actual funded
100k registry with four 100k seals fits or can join. The existing ignored native
`rfc06_complete_population_native_100k_four_retained_seals_and_fifth_refusal` measures
`full_carriage_bytes` and `legacy_384mib_fits`. This test and actual pruned transfer
remain required alongside the exact-population load/refusal test. No synthetic
candidate encoding is used to approve whole native carriage or decoded RSS.

Production TIR budget inputs were separately inspected: `TirBackendV1::new` obtains
the class from the artifact and constructs its actual `PalwTirStepSpaceV1`; its
runner uses that space and artifact plan/parameters. Actual replay evidence derives
the checked cost vector from every scheduled occurrence and the actual block's
worst legal history window. Replay-limit/cost tool overrides occur in tests, with
no daemon caller found. No fixture geometry substitution was found in those
production constructors. The existing executable corpus uses fixture shapes;
its resource numbers remain scoped controls, not worst registered model/time/RSS/
DA/court market costs or fee approval. All full RFC06 acceptance gates still apply.
