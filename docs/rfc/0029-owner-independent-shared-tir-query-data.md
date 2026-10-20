# RFC-0029 — owner-independent shared TIR query data

- Status: **Draft / Proposed / Review Required** (2026-10-21).
- Companion: [ADR-0196](../adr/0196-owner-independent-shared-tir-query-data.md).
- Governing contracts: [RFC-0026](0026-court-policy-master-contract.md), [ADR-0192 detailed contract](../design/palw/integration/rfc26-service-shared-frontier-contract-2026-10-20.md).
- Scope: isolated candidate source-query primitive and signed request intent. No production lifecycle, rooted frontier, paid admission, supply, default or activation is implemented by this draft.

## 1. Closed query scope

The existing `TirSharedRequestSetV1` derives all legal source queries from the original frame/source/native rules/shared profile: every permitted interior level, leaf, overlapping run, row, tile and canonical ID chunk. Existing query IDs stay unchanged. A separate `shared-tir-query-universe/v1` identity binds claim/allocation/rules/profile/source and complete cardinality. Owner, requester, relay, queue order and first arrival are not inputs.

The in-memory frontier is keyed by canonical ordinal in that immutable universe. Repeated requests coalesce and different legal choices coexist. There is no owner reservation credential or requester-specific exclusive slot in this primitive. This is not a proof that a native node admits an outsider while all owner seats are occupied: that requires original funded admission, native ingress and node T05 below.

## 2. Signed request intent

`SignedSharedRequestV1` field order is version `u16`, universe `Hash64`, ordinal `u64`, query ID `Hash64`, literal signer bond, signature bytes. Its message binds the native caller's network domain and every non-signature field under `shared-tir-request-intent/v1`; ML-DSA-87 uses the distinct `court-shared-request/v1/mldsa87` context. A requester's own bond key is resolved from state. A requester or relay does not become the owner of the query or its response data.

The signature Vec decoder validates the exact existing ML-DSA-87 length before allocation. The other fields are fixed-size. Unknown payload version, wrong universe/ordinal/query, wrong network/key/signature or absent bond changes no frontier state. A valid duplicate is coalesced, not converted into another shared obligation. This does not waive the real carrier's fee or refund its funding automatically.

The signature proves identity of this intent, not current original/profile authorization, funding, entitlement to slash or correctness of model computation. The candidate query set remains a caller-supplied native-source view; a future native coordinator MUST reconstruct it from the accepted original and adopted profile. No producer-supplied scope hash is an admission certificate.

## 3. Authenticated public data

Only a `PalwTirPublicAuthenticatedUnitV1` produced by the existing original-root source authenticator may fill an ordinary query. The unit's source and exact request must match. No hash-only response, declared sender, caller timestamp, fabricated completion flag or decodable response DTO substitutes for that capability. A previously authenticated alternative opening is not a new fault or a new reward.

Canonical ID chunks are different: they are provisional until the complete ordered ID sequence authenticates against the original outer trace root. The frontier accepts the resulting `PalwTirPublicAuthenticatedIdsV1` capability and serves borrowed canonical slices; it does not independently mark a raw ID chunk authenticated. Rows/tile authentication still needs the existing authenticated rows/IDs anchor. No body is reused across a different original source scope.

These capability values are intentionally not deserializable. The in-memory frontier is not serialized into consensus state. Future snapshots must retain raw bounded data and reauthenticate it against the current native original context before constructing capabilities; a serialized “authenticated” bool is forbidden.

A public data opening is not a particular custodian's signed response. Sharing data does not discharge that signer's duty, release bonds, certify credit, decide a semantic court, or promise Panel revenue. The full protocol still needs fresh adaptive response signatures and existing qualified custody rules. Keys for funds are not delegated.

## 4. Source-data upper components

`source_data_upper` covers every source query without allocating the query universe:

- Step/rows/tree nodes: for every level, multiply its width by the maximal first-node bound. The last ragged node has no larger frontier; all intermediate levels remain included.
- Leaves: all positions at the existing worst-leaf unit bound.
- Runs: every count from 1 through the existing legal run limit and every valid start, with that count's unit bound. Overlaps are retained.
- Tile nodes: all rows and levels; trace tiles and ID chunks separate full chunks from the last partial chunk.

All sums/products use checked arithmetic. Overflow returns failure without partially modifying the accumulator. The derived query count must equal the closed-set cardinality. Profile namespace changes cannot lower identical source body bounds.

Outputs are query count and sums of the existing maximum body bytes, parser steps, hash input bytes and resident-unit RAM. They are **partial derived uppers**, not measured runtime CPU or total physical service capacity. They omit request/response signatures, actual carrier/fragments, IO, frontier/indices/journal/retention, reorg copies, full-ID EOF/dependency work, preparation/work/restart/witness/semantic court costs and actual delivery. These omissions are open work, not zeroes. Even the resident-unit sum is not a funded storage/heap admission certificate for the new frontier.

Overlapping data may be shared only with authenticated transformations and their full costs. This conservative source-unit sum is sufficient for its stated source component, not a necessary lower bound proving all optimized designs impossible.

## 5. Required native protocol

Before activation, the candidate must reconstruct the accepted original/source/rules/profile, preserve original `a + 3,120` notice and `a + 10,000` hard bounds, fix notice equality in the boundary contract, and bind all legal adaptive queries, dependencies and late branches at original acceptance. A request arriving after a claim's permitted opening cannot reset its clock. A timely initial claim notice and later adaptive steps need separate native state; the timeless prototype does not invent that state.

The native coordinator must atomically reserve the complete graph, collect actual fees/funding, establish current literal court signers and dependency horizons, and install holds. Failure leaves none of these partially installed. It must reserve actual carriage/CPU/IO/memory/retention under the existing caps, derive first-block supply, and retain no-default recovery when supply/assumptions fail. The preceding body upper alone cannot enable this path.

Responses exceeding one existing control/part envelope need bounded streaming with scoped Begin/Part/Finish, signed literal responder, deterministic EOF authentication and rollback. No new larger ceiling is assigned here. Data sharing and obligation completion require separate effects; outsider requests cannot create unrestricted unpriced liabilities for custodians.

A future rooted namespace/codec and lifecycle allocation need explicit review. This draft allocates none and never reinterprets tag 230, old tag 245, X3 container, service v2 or RFC-0028 clock rows. Existing dormant fences and real-network empty arming records remain. Old claims keep old rules; the in-memory candidate is not wired into them.

## 6. Verification and limits

Selected tests cover actual native source-unit bounds versus a complete tiny-source oracle, ragged tree maxima, checked sum overflow, actual ML-DSA-87 request signatures, duplicate/different choices, profile/network/key/coordinate substitution, original-root data authentication, provisional/sealed IDs, malformed/truncated signature lengths and no mutation on refusal. Tiny execution and injected bond registration are fixtures, not actual node registration, payment, network capacity, signer availability or model measurements.

Full T05 still requires an unfinished-evidence outsider under all owner coalition occupation/refilling, every legal choice and latest response, duplicate roots, real carrier/fee/UTXO admission, shared authenticated data through deterministic adjudication, same-build supply and T01–T15, restart/reorg/pruned IBD and t12 identity. No adopted numeric capacity/fee, automatic bond release, model exclusion or cap increase is introduced.

Selected result: **67 passed / 0 failed / 0 ignored** (query/frontier 11, fresh identity/gate regressions 26, same fresh core executable native replay 30), non-test consensus check and existing generator idempotence succeeded. [Raw logs, initial failures, hashes, exact scopes and source patches](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#owner-independent-shared-source-data-candidate). This does not complete the native protocol in §5 or node T05.
