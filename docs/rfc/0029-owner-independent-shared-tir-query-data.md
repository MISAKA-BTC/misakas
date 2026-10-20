# RFC-0029 — owner-independent shared TIR query data

- Status: **Draft / Proposed / Review Required** (2026-10-21).
- Companion: [ADR-0196](../adr/0196-owner-independent-shared-tir-query-data.md).
- Governing contracts: [RFC-0026](0026-court-policy-master-contract.md), [ADR-0192 detailed contract](../design/palw/integration/rfc26-service-shared-frontier-contract-2026-10-20.md).
- Scope: isolated source-query/intent primitive and rooted data adapter candidate (§7). No production installer/lifecycle, complete paid admission, supply, signer-duty completion, default or activation is implemented.

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

These capability values are intentionally not deserializable. The in-memory frontier is not serialized into consensus state. The rooted candidate in §7 retains raw bounded data and reauthenticates it against the current native original context before constructing capabilities; a serialized “authenticated” bool is forbidden.

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

The local rooted candidate namespace/codec in §7 needs explicit public schema review; no lifecycle tag is allocated here. It never reinterprets tag 230, old tag 245, X3 container, service v2 or RFC-0028 clock rows. Existing dormant fences and real-network empty arming records remain. Old claims keep old rules; the in-memory candidate is not wired into them.

## 6. Verification and limits

Selected tests cover actual native source-unit bounds versus a complete tiny-source oracle, ragged tree maxima, checked sum overflow, actual ML-DSA-87 request signatures, duplicate/different choices, profile/network/key/coordinate substitution, original-root data authentication, provisional/sealed IDs, malformed/truncated signature lengths and no mutation on refusal. Tiny execution and injected bond registration are fixtures, not actual node registration, payment, network capacity, signer availability or model measurements.

Full T05 still requires an unfinished-evidence outsider under all owner coalition occupation/refilling, every legal choice and latest response, duplicate roots, real carrier/fee/UTXO admission, shared authenticated data through deterministic adjudication, same-build supply and T01–T15, restart/reorg/pruned IBD and t12 identity. No adopted numeric capacity/fee, automatic bond release, model exclusion or cap increase is introduced.

Checkpoint `b318ea946` selected result: **67 passed / 0 failed / 0 ignored** (query/frontier 11, fresh identity/gate regressions 26, same fresh core executable native replay 30), non-test consensus check and existing generator idempotence succeeded. [Raw logs, initial failures, hashes, exact scopes and source patches](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#owner-independent-shared-source-data-candidate). This does not complete the native protocol in §5 or node T05.


## 7. Rooted native data candidate (after b318ea946)

Local appended key/row tags are: `CourtSharedContext(claim)` 14/20, `CourtSharedQuery(request_id)` 15/21, and `CourtSharedIds(claim)` 16/22. Payload versions are 1. All old enum ordinals/codecs remain unchanged. These tags are candidate schema additions, not public adoption or lifecycle allocation. No production original coordinator installs the context, and no object dispatcher invokes the new data functions. Context presence is NOT a full reservation, timely notice or court-opening certificate.

Context fixes original claim/allocation/source/native rules, a separate shared profile derived from the bound service-v2 profile, canonical universe identity/count, original acceptance/hard/material clocks. It must match the CURRENT rooted original, registered class/layout and unresolved service binding. An intent is resolved against this actual native view, not a caller-supplied query set. The requester's existing Active/above-floor bond and real ML-DSA intent are checked; requester identity is not query ownership. Duplicates coalesce without another write, and distinct queries are separate rows. The opening stores actual accepting block/DAA and historical intent identity. A requester later retiring does not invalidate that accepted fact or privatize public data.

These are data operations inside `[original_acceptance, original_hard)`; they do not amend any existing owner's response deadline or the `+3,120` notice rule, define notice equality, or open a new court after its notice period. Initial notice and original complete all-branch admission remain unimplemented. No fee exemption follows from the private/native primitive: actual carrier funding/fee admission still needs integration.

Query rows retain bounded raw response bytes, digest and actual accepting context. Whole-ID rows retain the raw sequence/rows root and context. Source hash, request coordinate and full-ID outer EOF are reauthenticated on writes, public reads and carriage consistency. A matching storage digest or expected carriage root does not bypass original-root authentication. ID queries derive available data from the full sealed sequence, never from a per-chunk authenticated flag. Valid data availability has no signer-duty, credit, bond, refund or default effect. Fresh signed adaptive responses remain separate required work.

The internal functions use native checkpoints and the existing `StagedCourtSeed` journal/map/index writer. Query opening metadata and completed data/IDs/context are immutable; book/profile removal or substitution cannot strand them. Original material TTL stays unchanged, while an unresolved binding retains needed rows past it. No signed foreign promise is extended. Journal undo/replay must restore the same rows/indices. Raw clone/ID-byte hash workspace uses concrete typed allocation requests before allocation; new ID/unit runtime capabilities keep their leases attached. Rooted wire preflight/heap visitors are regenerated with the schema. Extra source reconstruction, per-row reauthentication, hashes, indices, copies, IO and retention are still costs to price/calibrate. This adapter alone does not establish a complete memory/CPU/physical-service profile.

The selected native fixture fills all eight existing owner slots through the existing verified-half Open path, confirms the outsider cannot Open another owner, then exercises the independent rooted shared request/data path with real requester signatures. It checks coalescing, other choices, invalid body/EOF refusal, read/reload, tampered raw data, journal undo/replay and retention. Original registration/signatures/payment/context installation are fixtures; there is no real lifecycle carrier, full original reservation, adversarial refilling/latest-answer campaign or node T05 acceptance yet.

The selected rooted-data checkpoint verifies **69 passed / 0 failed / 0 ignored**, non-test consensus check and regenerated inventory consistency. [Exact native/source fixture scope, raw logs, hashes and duplicate-count handling](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#rooted-shared-data-native-adapter-candidate). The first honest case was strengthened to a committed arithmetic lie and a valid-codec/digest but wrong-source body; these are data identity tests, not model correctness certification. Native lifecycle/funding/original all-branch admission/supply and node T05 remain incomplete.
