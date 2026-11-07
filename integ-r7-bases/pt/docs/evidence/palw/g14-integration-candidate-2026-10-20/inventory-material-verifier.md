# Single-root parameter material verifier candidate

Parent: `87b97e565`. Scope: independent read-only inventory authentication primitive; not ADR-0199 adoption or a shared material store.

`TirInventoryOpeningVerifierV1` borrows a program and retains the existing inventory index with one artifact root. The caller must supply the validated registered program and actual original's root; the constructor does not authenticate that authority. Index construction allocates and still needs native resource admission before a future production caller. The type has no peer decoder, serialization, state writer or fallback store.

A check first requires the exact inventory leaf count, then validates the leaf's tensor name, layer, row start and exact piece length against the program-derived inventory. Only then does it call the existing borrowed Merkle opening verifier. It clones no opening payload, path or program. The native multiproof checker uses the same extracted canonical-piece predicate, preserving its existing checks and errors. This change does not add composite artifact acceptance or activate deferred model improvement/media paths. Membership authenticates bytes and geometry, never model computation correctness, custody or availability.

The tests compare every single opening of the unchanged tiny-execution inventory with existing native multiproof authentication. Hostile inventories alter tensor name, layer, row start, byte length or inventory count and recompute their own valid Merkle roots: generic root membership succeeds, while registered-program geometry refuses. Other-program, other-root, invalid-index and trailing-path cases also refuse. These are synthetic fixtures and predicates, not funded actual-node admission or whole-model dependency coverage.

Still required: actual native program/root authority, bounded decoding and allocation tickets, parameter/prompt source material, no-fallback adapter, all-branch material lifetime/funding/reservation, nonblocking persistence, F02/F03/T05, restart/reorg and measured costs/real-model door. No proposed policy selection, original acceptance, fees, deadlines, old tags, production schema, pre integration, push, arming or deployment follows from this predicate.

## Verified functional checkpoint

The exact fresh unit executable passes 11 `palw_tir_court_v1::` tests (including two new inventory-material tests), followed by 42 `palw_tir_public_fold_v1::` tests. They are disjoint namespaces. The earlier compiler failure was an incorrect test-only `Hash64::ZERO` name; its log is preserved separately, then corrected to the existing constructor. No native limits or fixture commitments were altered to pass. Integration/type-check results are recorded separately in the artifact after completion.

Fresh integration checks pass: t12 identity 9 and activation 5, for 67 unique selected tests total (11 court +42 public fold +14 integration). Full core lib/test and non-test consensus type-check exits are recorded in the artifact; checks are not counted as executed tests. Documentation remains at the existing imported five missing references plus 43 baseline-known occurrences; no authored missing link is added. This is still an unused read-only primitive awaiting native authority/resource and material-store integration, not G14 completion.
