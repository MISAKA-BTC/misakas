# RFC-0026 native response subgraph — 2026-10-20

Status: implementation/verification candidate derived from the existing X3 native route. No new protocol adoption, service capacity or default authority.

## Source queries differ from native logical requests

`PalwTirPublicSourceV1::check_request_v1` permits each canonical ID chunk (`offset = 0, 1024, ...`). `request_body_bound_v1` in the native owned route instead permits only `Ids { offset: 0 }`, concatenating ALL original ID chunks into its one response body. Native `request_bound` uses that function for demand admission, clocks and Begin ceilings. Pricing the first source chunk alone would understate this native body's bytes. Counting every source ID chunk as a separately legal owned Demand would also be wrong.

`TirSharedRequestSetV1` retains its source-query meaning and identities. `TirNativeDemandSetV1` represents existing native logical requests: the first six request families are unchanged; source ID chunks collapse into one all-ID logical request, where supported. It derives the native response body/RAM bounds with the existing `request_body_bound_v1`, including concatenation. No new source/model restriction is introduced; existing availability constraints are not a claim that all G14 routes have been covered.

Logical identity uses the original claim, allocation instance, native rules, proposed shared profile, source and request, under `misaka-palw/shared-tir-native-demand/v1`. It is distinct from the existing source-query domain. Neither identity is a funded service packet, an owner-scoped native demand ID, a signer authorization or a computation-validity argument. The actual native demand ID still binds owner, reserved DAA, turn and accepting clock.

## Response subgraph and all declared-length branches

For each native logical request, `TirNativeResponseGraphV1` provides the actual native maximum body and resident RAM. A declared length `b` is permitted by the shape layer exactly when `1 <= b <= maximum_body_bytes`. It determines a branch:

1. Begin declares `b` and the hash of the actual body.
2. Each Part uses the existing 16,384-byte bound; every non-final part is full, and the last carries the remaining positive bytes.
3. Finish follows the last Part.

There are `ceil(b / 16,384) + 2` RESPONSE stages. `predecessor` expresses the preceding response-stage edge; it does not describe every intervening native work/control transition. Integer arithmetic is checked. Ordinals outside the branch, zero/oversized declarations and body slices of a different total length are refused. Accessing a stage does not allocate an entire graph. Building an action copies only that Part (Begin hashes the actual full body, Finish carries none).

`action_at` constructs existing native `PalwTirPublicOwnedActionV1` values; it does not create wire tags or change clocks, body formats or resource caps. Actual native signature verification, correct accepting DAA/turn, the accepted paid allocation, custody, chunk lookup, body authentication and resource/mempool/block limits still apply. Any separately submitted work debit remains subject to its native phase/actor/nonce/lane bounds. A structurally declared branch may contain invalid bytes: only the existing Finish authenticates the response. The stage count is not a complete move budget or a measured serial inclusion time.

## Native connection and verification scope

The existing tiny-frame native conviction fixtures' producer-response helper now obtains the native logical request's graph and builds its actual response controls using the branch. Those controls pass through the existing native dispatcher, rooted chunk rows and Finish authentication. This exercises the graph in the prior honest-response, fraud, held evidence and owner-collusion fixture flows. Fixture custody/control-verification caveats remain; this is not new node T05/T08 or production service admission.

Additional checks compare the logical serialized request set with the existing owned validator over the complete source-query universe of the tiny native fixture, verify native body/RAM bounds and distinct scope namespaces, and check ordinal/body boundaries. Synthetic large ID geometries check that chunk counts collapse to one logical all-ID request. Synthetic declared lengths exercise every size 1..32,770 across two part boundaries, plus maximum-u64 arithmetic. A multi-part synthetic byte body checks exact offsets, body hash and reconstruction; it is not authenticated model evidence, memory/CPU benchmarking or calibrated pricing.

## Still required for the full duty graph

Preparation/Ready, warm work and restart permits, Demand/Reacquire/Collect, all legal adaptive turns and branches, signer-specific eligibility, witness assembly, recovery and end effects are not erased by this response subgraph. Transport signature/framing, work, IO and concurrency costs still need their actual native derivation. An owner-independent logical identity does not make the existing owner-scoped carriage independent of its queue. No admission/supply caller or unadopted physical rate is added here. Service default stays denial-only; full F02/F03, T01–T15, R-D2-NODE and real door/capacity evidence remain open.

## Work-debit reachability audit

In the current X3 native dispatcher, `WorkDebit` is a separate action. Begin/Part/Finish do not require a positive new warm-work receipt before every stage: they check the source, literal signer, Ready state, original response clock and move budget, and Finish authenticates the rooted body. The existing producer-response fixture sends response controls directly. This does not make the actual CPU work free, remove its paid allocation, or authorize inventing a zero cost. It means a full protocol graph must include the legal branch with no intervening WorkDebit as well as admissible debit/restart interleavings. A backend's usual emission sequence must not be mistaken for a mandatory consensus predecessor. This work preserves the existing protocol; it does not add a new receipt prerequisite to force a desired graph.
