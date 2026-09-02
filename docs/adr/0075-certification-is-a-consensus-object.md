# ADR-0075: Certification is a consensus object

**Status:** PROPOSED (2026-09-02), implemented on branch `palw-adr0073-fp-weight` for the
Relaunch 5e re-genesis (the state version and the genesis free-prompt set move, so the fingerprint
moves). Written against the goal "the public testnet accepts free-prompt claims on the real
models, and a model's certification no longer needs a code change".
**Builds on:** ADR-0069 (end-to-end adjudicability: weight is the price of a certified family),
ADR-0074 Decision 6 (the free-prompt lane bears weight only on a certified class), ADR-0054
(permissionless admission), ADR-0042 Decisions 7/8 (lifecycle objects ride transactions).
**Amends:** ADR-0069 Decisions 2 and 5 (the certified set is no longer only the build's), ADR-0069
Decision 6 (a weightless entrant is seated by an object, not by a re-genesis), ADR-0074 Decision 6
(the free-prompt-certified set has a chain half).

> **Landed (index reconciliation, 2026-09-02).** "PROPOSED … implemented on branch" is the
> drafting state; the implementation is on `main` as the Relaunch 5e build (`ff943fe1` the objects,
> `49e5e1da` the mainnet rules of §7, `1b49fcaa` Decision 14's chunked carriage, `654b57f1` the
> re-seed on seating from [ADR-0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md)
> Decision 4), state version 16, fingerprints re-pinned for 5e. On mainnet's genesis card: §7 names
> the model-root constants because the assembly *can* pin model tiers; the decided route
> (Decision 8, ADR-0076 §4) is floor-only at genesis with every model arriving by registration →
> drill → binding, and mainnet ships `PalwConsensusMode::Disabled` until the card is set. Map:
> [`README.md`](README.md).

> **Security amendment appended (2026-09-02)** — see the last section: the permissionless lane's griefing budget — chunk groups carry a deposit, grading is priced before it is performed, the card's keys are the operators' own, and the tooling reads genesis ∪ chain.

## 1. The problem: a certificate lived in the binary

ADR-0069 made weight the price of adjudicability: a class holds a share only if some family this
build has drilled end to end covers every kernel the class reaches, and the set of such families
is pinned by `court_e2e_root` at genesis. ADR-0074 Decision 6 did the same for the free-prompt
lane with a genesis-frozen set of class ids. Both sets are compile-time values. Adding a model's
certification — or the free-prompt lane of a model the network already runs — therefore meant a
code change, a fingerprint move and a rolling re-genesis of every host. On 5d the free-prompt set
holds the floor alone: `execute_free_prompt` runs on the registered A16 graph and on QWEN36 once
this branch lands its path, and the transition refuses both with `FreePromptLaneUncertified`
because nothing on the chain can say otherwise.

The court itself never needed the binary's word. `certify_e2e_family_v1` and
`certify_e2e_free_prompt_lane_v1` grade recorded fault vectors with no backend in scope (ADR-0069
Decision 5 made the evidence exportable for exactly this reason). What was missing was a place on
the chain for the evidence to be graded, and a rule for what the grade unlocks.

## 2. Decisions

**Decision 1 — Two lifecycle objects, carried by ordinary transactions.**
`FamilyCertified { evidence }` carries a family's drill (`PalwCertificationEvidenceV1`: the
attempt-lane `PalwE2eDrillEvidenceV1` or the free-prompt-lane
`PalwE2eFreePromptDrillEvidenceV1`). `ClassLaneCertified { class_id, lane, profile }` binds a
registered class to a lane. Both ride the lifecycle subnetwork (0x4b) like a court move; both are
permissionless and unsigned. The evidence is objective and the class binding is checked against
the class's own profile hash, so a signature would authenticate nothing; the transaction fee is
the rent, and a drill's evidence (hundreds of kilobytes at the RC fixtures' size) rides one
standard-mass transaction.

**Decision 2 — The court grades; nothing else vouches.** The transition re-runs the shipped
grader on the carried vectors and records only the family the grader returns. A drill whose
planted faults the court cannot convict, whose honest run it convicts, whose vectors are about
another graph, or whose free-prompt questions are not its own, is refused
(`CertificationRefused`) and records nothing. A refused object is a dropped carrier: the block
stands (ADR-0042 Decision 5's "reads as nothing" is logged, not silent). The grading work per
object is bounded by the court's own step ceilings and by `PALW_CERTIFICATION_MAX_VECTORS` (32).

**Decision 3 — The chain's certified sets are state.** `PalwChainStateV2` gains
`certified_families` (attempt lane) and `fp_certified_families` (free-prompt lane), keyed by
family digest, and `fp_certified_classes`, keyed by class id. All three are in the state root and
the carriage, written through delta entries that revert (`PALW_STATE_V2_VERSION` 15 → 16). A
family already recorded for a lane is refused a second time (`FamilyAlreadyCertified`).

**Decision 4 — Every gate reads genesis ∪ chain.** `family_certified_for_weight_v2(root, genesis,
chain, reachable)` checks the root against the genesis set only — the chain's families are chain
history, each replayed from the evidence that produced it — and searches both. The registration
share rule in the processor, `verify_class_admission_v3` (v2 is the same gate with the chain set
empty) and the free-prompt arm (`params.fp_certified_classes ∪ state.fp_certified_classes`) all
read the union. `court_e2e_root` keeps its meaning: the set the network committed to at birth.

**Decision 5 — A class is bound by kernel coverage, in both lanes.** `ClassLaneCertified` is
accepted when the class is Active, `profile.shape_profile_id() == class_id` (a class id IS its
profile's id, as `verify_class_admission_v2` already requires), and `reachable_kernels_v1(profile)
⊆ kernel_ids` of some family the chain certified for that lane. Free-prompt lane: the class is
recorded in `fp_certified_classes` and its commitments are admitted from then on. Attempt lane: a
class holding no share is seated at `min_grantable_share_permille` through the same share table a
registration uses (largest-remainder donation, the base reserve, the pending-share ceiling) —
ADR-0069 Decision 6's "earns cadence once some build certifies a backend for it" as an object. A
class already holding a share is refused (`ClassAlreadyWeighted`); a class no chain family covers
is refused (`NoCertifiedFamilyCovers`). The transition holds no profiles, which is why the object
carries one and why the free-prompt record is per class.

**Decision 6 — The genesis free-prompt set is derived by the same rule.** The RC free-prompt
families are the floor, PALW-QWEN36 and PALW-QWEN25-A16, each drilled on the fixture graph its
attempt-lane certificate was drilled on with a caller's prompt instead of the anchor's
(`rc_free_prompt_evidence_v1`). `palw_rc_fp_certified_class_ids_v1()` is every RC catalog class
some such family covers — the floor, the QWEN36 graph-v3 class and the A16 graph-v2 class — so the
5e genesis admits free-prompt claims on the real models without an on-chain object, and every
later class takes the on-chain route.

**Decision 7 — The route has tooling.** `palw-certify drill --family --lane --out` writes a
`FamilyCertified` object from this build's drills (graded locally first, so a drill the chain
would refuse never leaves the machine); `palw-certify bind --model-id --lane --out` writes a
`ClassLaneCertified` for a catalog class; `misaka-cli palw submit-object --object --yes` funds
and broadcasts either, sizing the fee from the carrier's own compute mass.

## 3. What does not change

The attempt-lane genesis set and `court_e2e_root`; registration (signature, admission carriage,
exposure); the court and its cost ceilings; carriage codecs; the SDK's registration preflight,
which still prices against the build's set — a registrant whose family is certified only on chain
must read the chain (the processor requires exactly the share the union implies).

## 4. Consequences, and what is deliberately not here

* A family, once certified, stays. There is no revocation object: a class that misbehaves is
  frozen by contradiction (`ClassFrozen`), as before, and the family's certificate says only that
  the court CAN convict it — which remains true.
* Grading in the transition costs every node CPU per object; the vector cap, the standard-mass
  ceiling and the fee bound it. Spam is a paid way to make every node run a drill grader once.
* A fixture drill certifies kernels, not weights (ADR-0069 Decision 2). This is unchanged, and it
  is why the on-chain object for a new model tier is a fixture-sized file rather than a model.
* The SDK preflight and `misaka-palw-gateway` do not read the chain's sets yet; the processor
  does, so a mismatch surfaces as a refused registration with the share the chain wanted named in
  the error.

## 5. Invariants the tests hold

* `certification_objects_carry_a_family_onto_the_chain_and_seat_its_class` (base0): on real
  drills, a family enters through evidence and only through evidence that grades; tampered
  evidence is refused; a class binds by its own profile; a free-prompt commitment refused before
  the binding is admitted after it; a weightless class is seated at the floor by the attempt-lane
  binding; every delta reverts.
* `the_rc_free_prompt_set_is_the_one_this_build_drilled` and
  `the_rc_free_prompt_classes_are_the_covered_ones` (base0): the genesis set is the drills, field
  for field, and the class set is the covered catalog.
* `a_chain_certified_class_takes_fp_commitments_under_the_genesis_gate`,
  `a_class_is_bound_to_a_chain_family_by_its_own_profile_and_kernel_coverage`,
  `evidence_the_court_refuses_certifies_nothing` (consensus-core): the gates, the refusals, the
  state root, the carriage and the revert.

## 6. The mainnet route for a model this build never pinned (Decision 8)

Mainnet ships PALW off today (`PalwConsensusMode::Disabled`); the bundle it will activate is
built by the same `palw_v2_params_on_base` every RC network uses, so nothing here is a testnet
special case. For a stranger with a new model, the route is three ordinary transactions and no
code change:

1. **Register weightless.** `kaspad --palw-register-class <model id>` (or the SDK) builds the
   class from its registered graph and registers it at 0‰ when no family the build or the chain
   certified covers its kernels. The registration message is signed over the share the OBJECT
   carries — a weightless registration used to be unsignable because the panel signed the floor
   unconditionally; that is fixed with this ADR.
2. **Post the covering family's drill.** `palw-certify drill --model-id <model id> --lane attempt
   --out f.obj` picks the RC family whose kernel set contains every kernel the model's graph
   reaches (`covering_rc_family_v1`), runs that family's fixture drill, grades it locally, and
   writes a `FamilyCertified`; `misaka-cli palw submit-object --object f.obj --yes` carries it.
   The test `every_catalog_class_on_a_registered_graph_is_covered_by_a_drillable_family` holds
   that every class the catalog can express on a registered graph has such a family on both
   lanes. A model whose graph reaches a kernel no shipped family drills is a new architecture,
   and needs a build whose court serves it — that boundary is ADR-0069 Decision 2's, not this
   ADR's.
3. **Bind the class.** `palw-certify bind --model-id <model id> --lane attempt --out b.obj` and
   submit it: the class is seated at the floor share and is weight-bearing from the next epoch
   boundary. `--lane fp` the same way admits its free-prompt claims.

If the family was certified on chain BEFORE the registration, the processor requires the floor
share at registration (ADR-0049 Decision H); `PalwRegistrationTermsV2` now carries the chain's
certified families, so the SDK and the panel price that registration correctly without an RPC of
their own. `a_model_this_build_never_pinned_is_seated_through_the_chain_alone` runs the route on
the Qwen3.5-2B graph-v3 class, which no RC set names.

## 7. Mainnet: the rules of operation (Decisions 9–13)

Mainnet's PALW is born the way every RC is born: the pinned genesis card
(`PALW_MAINNET_GENESIS_BONDS`, `PALW_MAINNET_GENESIS_ARTIFACT_ROOT`,
`PALW_MAINNET_QWEN36_ARTIFACT_ROOT`, `PALW_MAINNET_QWEN25_A16_ARTIFACT_ROOT`) assembled by the same
`palw_v2_params_on_base` the RC networks use, so the certification rules are the ruleset's, not a
mainnet special case (ADR-0042 Decision 11: the last RC and mainnet share one ruleset id, built
from one git tag). The card is empty until real operator keys exist; while it is, mainnet is the
hash-only network it is today, byte for byte. This section decides how the objects operate once
it is not.

**Decision 9 — Who may submit, and how much.** Anyone. A `FamilyCertified` or
`ClassLaneCertified` is an ordinary transaction under the standard mass ceiling, paying the
standard relay fee; no bond, no signature, no allow-list — the court grades the evidence and the
class's own profile hash grades the binding, so a gatekeeper would add nothing but a gatekeeper.
Two bounds hold the cost: at most `PALW_CERTIFICATION_MAX_VECTORS` (32) fault vectors per object,
and at most `PALW_CERTIFICATION_MAX_PER_BLOCK` (2) `FamilyCertified` objects graded per block,
counted in transaction order by the acceptance walk — the third is dropped and the block stands,
so a stranger's evidence can make a block slower to validate by two drills, never invalid.

**Decision 10 — Revocation.** There is no revocation object, on purpose. A class that misbehaves
is frozen by a contradiction certificate (`ClassFrozen`), which removes its weight and refuses its
claims; its certification record stays as history and does nothing. A *family* certificate can
only be wrong if the court that graded it is wrong, and a court defect is a ruleset change: under
ADR-0042 Decision 11 that is a new network identity, born with an empty chain set, on which every
family re-certifies through the same objects. Nothing on the old identity is edited.

**Decision 11 — Upgrade.** Within one identity, a family's coverage grows by posting a second
drill whose kernel set is a superset (a different digest, a second record; the class binding
finds whichever covers it). Across identities, everything re-certifies — the genesis set of the
new identity is derived by the coverage rule from the drills the new court grades
(`palw_rc_fp_certified_class_ids_v1`), and the chain set starts empty. A class's own profile never
changes (its id IS its profile), so a class never needs re-binding on one identity.

**Decision 12 — Expiry.** None by time. A certification is bound to the network identity that
recorded it and lives with the class record: a class reclaimed to Dormant and re-registered under
the same id keeps its free-prompt certification (same profile, same kernels) and takes its share
from the registration rules, which read genesis ∪ chain. What DOES lapse is everything, on a
ruleset change — the wipe policy below.

**Decision 13 — State version 16 and the migration that is not one.** Mainnet has no PALW state
today; a carded mainnet is born at version 16. Testnet-11 moves 15 → 16 by the Relaunch 5e
re-genesis, as every ruleset change has: a node that starts this build over a datadir written by
another version refuses at boot with the reason and the remedy (wipe, resync from peers announcing
this build's fingerprint), because `PalwStateCarriageV2::into_state` accepts only its own version
and the P2P handshake already refuses the old fingerprint. No in-place migration exists and none
is planned: a chain-certified set is chain history of one identity.

**Deployment and verification.** `docs/mainnet-palw-certification-runbook.md`: the card, the
build from the final RC tag, the simultaneous validator swap (every validator on one build —
a mixed fleet forks at the first certification object), and the verification ladder (devnet
rehearsal of the three transactions, the announce fingerprint, the first `FamilyCertified` on the
live chain read back from every validator's `[palw-lifecycle]` line).

**Decision 14 — An object too large for one carrier rides in chunks, reassembled in state.**
Measured on the RC drills: the A16 fixture's attempt drill is 82 KB, the QWEN36 fixture's 248 KB
(183 KB of it the profile repeated in every refutation's binding), the floor's 310 KB. A block
carries at most `max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR` ≈ 125 KB of transaction bytes,
and a standard transaction is bounded the same way, so most drills do not fit one carrier and a
single-carrier rule would have made "permissionless" mean "only small graphs". `ObjectChunk {
group, index, count, bytes }` is a lifecycle object: `group` is the digest of the whole object's
bytes (`palw_object_chunk_group_id_v1`), each part is at most `PALW_OBJECT_CHUNK_MAX_BYTES`
(100,000), a group spans at most `PALW_OBJECT_CHUNK_MAX_COUNT` (8) parts. The chain keeps the
parts IN STATE (`pending_chunks`, in the state root, carried and reverted like every collection)
and, in the block that completes a group, hashes the assembly, decodes it, requires a
`FamilyCertified`, and applies it through the same arm a directly carried one goes through —
which is why the per-block grading cap counts a completing chunk as a certification. The table
holds at most `PALW_OBJECT_CHUNK_MAX_GROUPS` (8) half-assembled groups; a group nobody completes
within `PALW_OBJECT_CHUNK_TTL_DAA` (4,000) is evicted, in key order, when the room is needed. A
part that duplicates, a part under another count, an assembly that does not hash to its group,
or a chunked object of any other kind is refused and the block stands. `palw-certify drill`
writes the chunks beside the whole object and `misaka-cli palw submit-object` carries them as one
chained burst, each carrier funded from the previous one's change.

## Security amendment (2026-09-02) — the permissionless lane's griefing budget

**SA-1 — Chunk groups are deposited, not merely fee-priced.** `ObjectChunk` groups are unsigned and
permissionless; eight pending groups × 800,000 bytes live in the state root for up to 4,000 DAA. A
griefer can hold all eight with junk for ~5.5 days at the price of carriage and block every honest
drill. Rule: a chunk group's carrier locks a deposit proportional to its bytes (an output the
transition recognises, as it does a bond's collateral), refunded when the group completes into an
accepted object and forfeited at TTL or on refusal. Junk pays; honesty is refunded.

**SA-2 — Grading is priced before it is performed.** Two `FamilyCertified` are graded per block
whether or not they pass; a refused one still costs every validator up to 32 court re-executions.
The minimum fee for a `FamilyCertified` is derived from its vector count (compute mass, as `misaka
palw submit-object` already sizes it) and a block that grades one paying less is invalid — so the
CPU every validator spends per block is bounded by fees the griefer actually paid.

**SA-3 — The card's keys are the operators' own**, generated on their hosts; only public rows enter
`params.rs` (the testnet-11 practice); a card with two rows sharing an operator id is refused by the
genesis gate.

**SA-4 — The SDK preflight and the gateway read genesis ∪ chain** (the open item above), because a
tool that reports a chain-certified class as uncertified will be "fixed" by an operator with a flag,
and flags that override chain facts are how a rolling re-genesis starts.
