# RFC-0007 exact provider-unit failure provenance — 2026-10-25

Status: coordinated build, SDK3/3 and node2/2 focused parents PASS; see [final verification](0006-0007-0008-0012-court-aux-provider-round-verification-2026-10-25.md). Old APIs, public presets, normal startup rules, parameters, wire enums and selected-verifier requirements are unchanged.

## Concrete missing consumer branch

The private paid checker recorded an exact failed query only when its authenticated socket callback returned an error. The socket verifies the response signature, request echo and byte bounds, then returns the raw unit. The SDK subsequently performs the unit's own decode and native material authentication. A successful signed transport carrying malformed raw unit bytes therefore returned SDK `InvalidMaterial` while the checker's failed-query slot remained empty. The checker withheld its receipt, but also skipped its existing explicitly funded material Challenge path. Repeated malformed responses could leave that path pending forever.

This is a prosecution/availability liveness gap. It was not a false Clean result, an approval bypass, or an existing native conviction. The existing SDK malformed-unit control already exercises `vec![0]` returning `InvalidMaterial`. No new pre-fix runtime result is claimed; the pre-fix native control flow was inspected directly.

## Change and ownership

The existing classified error enum and the classified/String SDK APIs retain their signatures and meanings. The additive `verify_using_provider_located_v4` returns a private-field `ProviderReaderFailureV4`. It carries the existing cause plus an optional fixed `(stage, position)` from the raw unit's own failed decode/authentication boundary. Its type has no deserializer or public constructor for attaching a coordinate. An ordinary error converts only to an unlocated failure.

The provider reader records this coordinate inside the exact position/snapshot/segmented-unit decode/authentication path and separately inside each parameter row/tile acquisition. Optional reader failures stay sticky. LocalCapacity, Unknown and unavailable transport do not acquire invalid-unit provenance. The old classified API returns precisely the original cause.

Full parameter reconstruction checks and segmented full-position assembly occur outside those unit-local boundaries. Their cross-unit inconsistency cannot accuse the last fetched row/part. No coordinate is guessed from a successful last request. The additional metadata is fixed-size and allocates no material buffer; the existing raw/decoded/parameter/acquisition owners and their lease lifetimes are preserved.

The actual node calls the located reader. The SDK's `provider_material_challenge_unit_v4` selects only a located InvalidMaterial. The node then binds that coordinate to the same own base claim, dependency, serving provider and current consent generation. Existing transport-error queries follow the same binder; a different root/dependency/provider/generation/version refuses selection. Missing generation or unlocated error remains pending. This selection grants neither duty authority nor guilt.

The result enters the existing accepted-challenge deduplication, explicit configured deadline/funding, fresh own-native query authority, signed material Challenge, rehearsal and durable outbox flow. A changed generation is checked again by native authority before signing. No new proof type, penalty, fee, receipt, source or activation authority is introduced.

## Prepared controls and exact scope

SDK selector: `provider_reader_v4::memory_tests::located_tests` (three public parents).

- Real two-step Memory producer/provider reader, with successful acquisition callbacks and malformed bytes at the actual entry-state, ordinary position and immutable parameter unit. Each failure selects exactly that unit through the same SDK action selector the node uses, cannot complete a receipt, and leaves the fixture ledger root unchanged. An unchanged valid retry completes its local check.
- Typed local quota, opaque error and transport failures preserve causes and sticky read ordering. The actual-frame workspace refusal stops after its separately retained raw response, before unit authentication/typed decoding, and cannot select a duty.
- All rows individually authenticate under an intentionally mixed row/column commitment in an isolated parameter reconstruction fixture. The full tensor cannot reproduce that commitment. The actual reconstruction failure has no unit metadata and selects no last-row obligation. This modified registry fixture is not represented as a newly admitted native class.

Node selector: `kernel_paid_checker_v4::failed_query::tests` (two public parents).

- Positive SDK-unit coordinate mapping and the existing transport-query path preserve the exact base/dependency/provider/generation.
- Root, dependency, generation, provider and version substitutions, and absent generation/coordinate, cannot select a query.

These are local consumer controls. They do not establish signed malformed-response handling across separate processes, native Challenge acceptance/finality, all four adapter campaigns, monetary sufficiency, source independence, real K3 work, or any RFC-0012 campaign gate. K3 formal inputs remain uncreated. All 15 campaign gates stay UNKNOWN. Normal selected-source and V6 paid bootstrap activation refusal remains intact. A local direct check does not replace randomly selected verifier approval.

## Initial verification failure retained

The first coordinated no-run build passed (249.97 s, frozen source identities matched). The initial three-parent located-reader run passed two controls and failed the new capacity control at its fixture's `no fetch after own budget refusal` panic. Log: `/tmp/misaka-rfc-court-aux-provider-round-provider-located-20261025.log`. The actual reader obtains a separately retained raw response before quoting/checking its frame and codec workspace. The new test incorrectly assumed that a one-byte SDK workspace limit forbids the preceding fetch. This was a test expectation error, not evidence of an introduced acceptance bypass.

Only that test's injection was corrected: the callback now returns its genuine retained unit, the actual workspace check refuses after exactly one acquisition, and assertions require typed LocalCapacity, absent unit metadata, no challenge selection and unchanged fixture ledger root. Production behavior was not changed for this repair. The corrected final runtime passed all three SDK parents; all14 reader regressions and all19 node checker controls also passed. The initial failure remains recorded rather than relabeled PASS. The separate node failed-query scope controls passed 2/2 on the initial frozen artifact.
