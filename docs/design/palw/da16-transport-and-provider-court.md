# DA16 — the claim-material provider court, the court scope, and the (off-chain) public-material transport

Lane DA16 (`da16/transport-provider-court`). Re-scoped on 2026-10-10 by [ADR-0177](../../adr/0177-model-bond-allocation-without-availability-consensus.md)
("the chain does not interfere with model acquisition"). The lane now owns three things:

1. **The claim-material provider court** (RFC-0009 §4.2): moving a kernel claim's DA responsibility to bonded providers. Dormant behind
   `palw_provider_court_v1`.
2. **The court scope** (ADR-0177 D2, RFC-0014 §16.4): what any court may demand, the cumulative bound, and how a verifier's own model
   operand is authenticated. This is G14C's GAP-06; GAP-05 (availability out of consensus) and GAP-52 (snapshot leaf well-formedness) close
   here too. See §7.
3. **The public-material transport** (`misaka-palw-remote::public_material`, `palw-evidence artifact-*`): optional, non-consensus
   tooling for the artifact half. The claim half serves the court.

Amounts are BILI (1 BILI = 10^8 sompi). Status words are the user's three levels (implemented / verified / armable); nothing here is
armable, and nothing ran against a live node or network.

## 0. Decisions in one table

| Question | Decision | Why |
|---|---|---|
| Model availability in consensus | **Withdrawn.** The `Artifact` lease subject, the READY/LAPSED state of an artifact pair, the tag-104 gate on it, `AVAILABILITY_REQUIRED` from a lapse, and every charge for not serving model bytes are gone. | ADR-0177 D1: a model not being served may never void, hold, slash or delay anything. |
| The `Artifact` variant's bytes | **Kept decodable; refused past the fence at every height.** Discriminant 0 stays reserved. | A-2: below the fence the bytes ride unjudged exactly as before, so no mixed-release split. Past the fence the fold refuses before reading any row, bond or class. |
| What the court may compel | **Claim-specific units only** (input, trace, state, output, witness hashes). Model bytes come only from a verifier's own copy, authenticated against the REGISTERED root. | ADR-0177 D2. One predicate, `palw_court_scope_v1`, consulted by every demand path the lane can reach. |
| Cumulative bound | **Bytes:** model-byte disclosure is 0 per claim and per model, for any number of demands. **Count:** each unit at most once per claim, and ≤ 16 distinct units per (claim, requester operator). | No count cap is shared between requesters, so nothing can starve an honest prosecutor (G14). |
| A claim charge's reporter share | **ADR-0032's 49% before Final** (it was 500‰), for the 151 challenger and the provider-liable demanders alike; burned whole after Final. | It is the same rate as the route's accuser and demander shares (G14-R4's 490‰ constant), so a self-lapse leaves the coalition the same ≥ 51% floor as a self-reported conviction; see §2.3. |
| Snapshot leaf well-formedness (GAP-52) | **Make the court total** instead of attesting at registration. A retrieval claim's own entries become a claim-specific demand unit. | A junk leaf has no preimage, so no registration attestation is refutable. See §7.6. |
| Transport crate | Extend `misaka-palw-remote` (unchanged); the artifact half is **non-consensus**. | Model distribution is off-chain and voluntary (ADR-0177 D1, D2). |

## 1. The public-material transport (non-consensus for artifacts)

`consensus/core/src/palw_public_material_v1.rs` and `misaka-palw-remote/src/public_material.rs` are kept as they were. Each now carries a
module-level NON-CONSENSUS label for the artifact half. It covers:
- the units and their single verification function (artifact leaf, kernel commitments, row-tree node run, kernel row, claim position);
- the manifests (checked against the chain before any byte);
- the read-back publish, per-provider availability (a local observation), repair and the retention monitor;
- the CLI `palw-evidence artifact-fetch | -verify | -status | -repair | -hook`.

A verifier uses it to obtain a model and check it against the REGISTERED roots. No fold arm reads its result, no lease backs it, and a
provider that serves no model bytes is never charged. The claim half (`ClaimPosition`, V2 held units) is what the provider court and the
kernel's demand serve.

## 2. The provider court (claim material only)

### 2.1 Fence

`Params::palw_provider_court_v1: Option<ForkActivation>` follows the usual dormant pattern:
- `None` on every preset, Some-only hashed, `never()` collapsed;
- listed in `palw_fences_v1()`, with a fork-id probe arm;
- `validate_palw_provider_court_v1` refuses every armed height.

Where in force (harness only), the kernel route's fence must be in force too. Below it, tags 150–153 are dropped by name before any slot,
rent or budget (A-2 uniform), and the fold refuses them as the second lock. The same fence also gates the court scope (§7.3) on the kernel
route.

### 2.2 Objects (tags 150–153, ML-DSA-87 by a V2 bond, context `misaka-palw/provider-court/object/v1`)

| Tag | Object | Signer | Effect |
|---|---|---|---|
| 150 | `ProviderLeaseV1 { subject: KernelClaim { claim }, reserved, serve_until_daa }` | the provider | A bonded promise to serve every committed position of the claim. Reserves `reserved ≥ 100 BILI` of FREE collateral, mirrored into V2's committed collateral and both withdrawal gates. One lease per (claim, provider); never the producer's operator. |
| 151 | `ProviderChallengeV1 { subject, provider, unit: ClaimPosition, valid_until_daa }` | another operator's bond | Challenges one committed position of one lease. Reserves a 10 BILI bond; deadline `now + 20`; signed expiry plus a tombstone, so a replay never re-opens it; ≤ 8 open per challenger. **The unit must pass the scope predicate:** an artifact unit is refused as model bytes. |
| 152 | `ProviderAnswerV1 { …, answer: ClaimPosition { bytes } }` | the provider | Classified by the kernel's own `classify_served_position_v1`. Valid by the deadline ⇒ cleared, and the challenger pays a 1 BILI fee. A wrong answer neither clears nor defaults. |
| 153 | `DaTransferV1 { claim }` | the claim's producer | Moves the claim's DA responsibility to its leases (§2.3). |

`ProviderSubjectV1::Artifact { … }` (discriminant 0) is **withdrawn**: 150, 151 and 152 naming it are refused first, by `claim_subject`.

### 2.3 Rules (RFC-0009 §4.2)

* **Transfer.** Accepted only when all of these hold:
  - the claim was committed at or after the fence, is not terminal, has no open demand and no default so far;
  - at least 2 live, uncharged leases from distinct operators (none the producer's) serve through the claim's horizon bound;
  - Σ lease reservations ≥ the claim's reservation.

  The transfer is irreversible.
* **An unanswered 151 charges THAT lease, once per (claim, provider).** The reservation is slashed; the challenger gets its bond back and
  is paid **the PALW reporter share (49%, ADR-0032) before Final**; the rest is burned (all of it after Final). The provider's other
  challenges on the claim settle moot.
  - *Why 49% (G14):* a coalition can void its own claim by charging its own leases through its own Sybil challengers. At 49% it nets a
    loss of ≥ 51% of Σ leases ≥ 51% of the claim's reservation. That is exactly the floor a self-reported conviction leaves at G14-R4's
    490‰ accuser share, so voiding is never the cheaper escape.
  - The former 500‰, beside a 10% accuser share, was a discount. The transfer rule Σ leases ≥ the reservation is what makes the two
    floors meet.
* **The kernel's `FileDemand` stays THE material demand.** On a transferred claim, an unanswered demand is `ProviderLiableDefault`:
  - every live lease is charged; the producer pays nothing;
  - before Final, the demanders share 49% of `min(pool, default_penalty)` (the producer path's rate at G14-R4's merge) and the rest is
    burned; after Final, everything is burned. The coalition's floor is Σ leases − 0.49 × penalty ≥ 0.51 × penalty;
  - the claim is void.
* **Common-mode outage voids, never convicts.** With no live lease left, the claim lapses: `Unavailable { producer_defaulted: false }` (no
  reward; the producer's reservation released, never slashed), or the OPV fact is withdrawn after Final.
* **A false root or a false computation stays the miner's.** Bytes the providers serve still convict through the kernel's unchanged
  `FileProof`.
* **Never a slash:**
  - a Panel's local timeout or a local fetch failure;
  - a lease alone, or a wrong answer before the deadline;
  - a position the claim does not commit;
  - **anything about model bytes.**
* **Old vs new:** below the fence, and for every claim never transferred, the producer's path is byte for byte today's.

### 2.4 Artifact availability — WITHDRAWN (ADR-0177 D1)

The former §2.4 is removed from consensus:
- tag 104 needing ≥ 2 live leases;
- a lapsed pair attesting nothing;
- `AVAILABILITY_REQUIRED` on a lapse.

Lane D's onboarding is back to its own behaviour: `palw_onboarding_{v1,fold_v1}.rs` are restored byte for byte from before DA16. A
binding is refutable by any verifier that holds the bytes (tag 105), with no availability guarantee. See §7.7 for the residual this
re-opens.

### 2.5 Rows (kernel route aux tables 43–45)

| Table | Key | Row |
|---|---|---|
| **43** leases | `(subject, provider)` | `ProviderLeaseRowV1 { reserved, filed_daa, serve_until_daa, charged }` |
| **44** challenges | `(subject, provider, unit)` | `ProviderChallengeRowV1 { challenger, bond, filed_daa, deadline_daa, answered }` |
| **45** claim subjects | `KernelClaim { claim }` | `ProviderSubjectRowV1 { transferred_daa, lapsed_daa, charged, requested }` |

`requested` is the court scope's per-requester tally (§7.3). The closing tick, the `ProviderLiableDefault` settlement and the typed read
`provider_court_read_v1` are unchanged.

### 2.6 INTERIM numbers (drill values, not security values)

- lease reservation floor: 100 BILI;
- challenge bond: 10 BILI; cleared-challenge fee: 1 BILI;
- response window: 20 DAA; minimum providers: 2; ≤ 8 open challenges per challenger;
- reporter share: 49% before Final (`palw_provider_reporter_share_v1`, ADR-0032), burned after Final;
- court scope: ≤ 16 distinct units per (claim, requester operator).

Provider economics and Sybil-independence are EXTERNAL_GATE.

## 3. G14 — the DA half

The setting: the producer and ALL Panel seats collude.

- **Withholding.** One public bonded verifier outside the Panel reaches the correct objective DA default for a withheld claim-specific
  unit: a kernel `FileDemand`, or the retrieval entry demand of §7.6. Any bond may file it; seats have no power in this court.
- **Starvation-freedom.** The per-requester tally never touches another requester's allowance, so it cannot be exhausted against the
  prosecutor.
- **No re-assignment of failures.** A transfer is refused while a demand is in flight.
- **No discounted self-lapse.** A self-lapse leaves the coalition the same ≥ 51% floor as a self-reported conviction (§2.3).
- **Conviction and dismissal.** Fraud is convicted from served material or from the verifier's own model copy. An honest claim is
  dismissed, and reaches Final even when nobody serves the model.

The E2E tests are listed in §6.1.

## 4. Held 8k material

V2 held claims are transport-only. Their default path is the V2 held DA court; that court is ARMED and its `StepLeaf` is in §7.5. K2S
segmented positions reach this court's hook generically.

## 5. Coordination

* **A2U.** Tags 150–153 stay on `palw_provider_court_v1` in the central kind→fence table. Their bytes (including the withdrawn `Artifact`
  discriminant 0) still ride unjudged below the fence. A2U should add one row: "tag 150–152 with subject discriminant 0 → refused past the
  fence at every height (not judged)".
* **K2S.** The call sites are in §7.8.
* **R4X.**
  - Snapshot slices and the registered `M0` pre-state are refused past the fence (§7.3).
  - Retrieval entries are a new demand stage, `0xC0 + s` (§7.6).
  - `r4x_retrieval_…_a_withheld_slice_defaults` asserts behaviour ADR-0177 withdraws once the court scope is in force. It still passes
    because its harness does not arm `palw_provider_court_v1`.
* **G14-R4.** The demand-bond fate (a burn when a served position's claim is not convicted) is the economic half of the relational bound
  (§7.4).

## 6. Implementation status (DA16b, 2026-10-10)

| Piece | Where | Status |
|---|---|---|
| Court re-scope: `Artifact` refused, artifact paths out of the fold, reporter share 49% (challenger and demanders), burned after Final | `consensus/core/src/palw_provider_court_{v1,fold_v1}.rs` | implemented |
| §2.4 removed: onboarding restored | `consensus/core/src/palw_onboarding_{v1,fold_v1}.rs` (= `eae67be30^`) | implemented |
| Court scope: inventory, rule, per-requester tally, node masks, exposure, verifier-operand authentication | `consensus/core/src/palw_court_scope_v1.rs` | implemented, unit tests |
| Kernel-route enforcement (FileDemand admission + tally past the fence) | `consensus/core/src/palw_kernel_route_fold_v1.rs` → `court_scope_admit_demand_v1` | implemented |
| GAP-52: court totality over malformed leaves; entry demands | `misaka-palw-kernel/src/spec/{retrieval,ledger_impl,mod}.rs` | implemented, unit test |
| Transport artifact half labelled non-consensus | `misaka-palw-remote`, `palw_public_material_v1` | implemented (docs only) |

### 6.1 E2E (harness node, fences test-armed without their validation)

* `da16_model_availability_never_gates_a_binding_and_the_artifact_subject_is_refused`:
  - 104 binds with no provider;
  - an `Artifact` lease, challenge and answer are each refused: no row in 43–45, nothing reserved;
  - the transport confirms the binding off chain;
  - every provider directory is deleted, and the binding still matures and attests.
* `da16_a_false_binding_is_refuted_from_the_verifiers_own_bytes_and_the_binders_published_tree`: tag 105 from the verifier's own copy and
  the binder's voluntary publication. The binder is slashed; no court row exists.
* Claim side, unchanged in substance:
  - transfer rules;
  - a provider-liable default (it now also asserts the scope tally and the demander's 49% with conservation);
  - **common-mode outage**: charged once (one challenge moot), **49% to each challenger and the rest burned (conservation)**, an artifact
    unit refused, bonds back, no fee;
  - reorg; false computation convicts the miner (it now asserts the 1 BILI fee); below-the-fence A-2.
* `r4x_typed_roots_e2e::da16_scope` (R4X's harness with the court fence armed):
  - `…a_snapshot_is_never_demanded_and_a_retrieval_claims_own_entries_reach_every_terminal`: a slice demand is refused and an honest claim
    reaches Final; a wrong item is convicted from the verifier's own copy; an entry is demanded and served, or withheld ⇒ the producer's
    default;
  - `…the_registered_memory_is_never_demanded_and_a_carried_pre_state_is`.

### 6.2 Residuals

* **Pre-Final self-lapse.** A coalition can still void its own claim before Final, at the ≥ 51% floor (§2.3). The honest demander whose
  demand was open is refunded but not paid. Whether a lapse with open demands should pay them first is an ECON/G14-R4 decision.
* **Shared adjudication budget.** A 152 spends the route's per-block budget (EXTERNAL_GATE, MEAS).
* **V2 held claims.** Transport only (§4). K2S multi-part positions are handled by K2S's own call sites (§7.8).

## 7. The court scope (ADR-0177 D2, RFC-0014 §16.4) — G14C GAP-06, GAP-52

### 7.1 Inventory

The executable form is `palw_court_unit_scope_v1`; its test pins the lists below.
- **Material:** C = claim-specific (input / trace / state / output / witness hashes); M = model bytes (weights / derived copies / file
  ranges).
- **Supplier:** D = compelled under a demand; P = the producer's opening move; S = a seat's condition; V = the verifier's own copy.

| Route (fence on t12) | Unit | Material | Supplier | Under this rule |
|---|---|---|---|---|
| Kernel route (dormant) | `FileDemand` position (K2 v3; K2S v4 parts) | C trace* | D | allowed; *the owed set must exclude model-bytes nodes (§7.2, K2S) |
| | stage inputs; v4 part 0 (position root, path) | C input / witness | D | allowed |
| | v4 `PostPromptTile` | C input | public | allowed |
| | row-tiled opening of a committed value in a filing | C trace | V | allowed |
| | param row / column / tile opening (`FileProof`, `InstanceRecompute`, `ElementRecompute`) | M weights | V | allowed (verifier's copy, §7.5) |
| | R4X snapshot slice (`0x80 + s`) | M file range | D | **refused** past the fence |
| | R4X registered `M0` pre-state (`0x40`, `pre_source = None`) | M file range | D | **refused** |
| | R4X carried pre-state (`pre_source = Some`) | C state | D | allowed (already public) |
| | R4X retrieval item opening (`WrongItem`, `MissedBetter`) | M file range | V | allowed |
| | **retrieval entry (`0xC0 + s`, new)** | C output | D | allowed (GAP-52) |
| Onboarding (dormant) | 105 `Instances` (commitments) | C witness | V | allowed |
| | 105 `Row` (V2 leaf + bound row) | M | V (+ the binder's own tree) | allowed |
| | 109 `Post` (outcome digests, no leaves) | C witness | D | allowed |
| | 109 `LeafDecode` | M file range | V | allowed |
| | 109 `VectorTokens` | C output | public | allowed |
| Provider court (dormant) | `ClaimPosition` | C trace | D | allowed |
| | `ArtifactLeaf` / `KernelRow` (and the hash units `KernelCommitments` / `KernelRowNodes`) | M (C) | D | **refused** (subject withdrawn) |
| R-core / held DA (ARMED, DAA 0) | `Event` (logits) | C output | D | unchanged |
| | `PromptIdsTile`, `StateChunk`, `StepRange` | C | D | unchanged |
| | **`StepLeaf`: its answer carries `artifact_openings` (weight rows) from the producer** | **M** | **D** | **armed: unchanged; §7.5** |
| TIR / pipeline courts (ARMED, 3,600 / 5,300) | step leaf / node / row node / run | C trace | D | unchanged |
| Dissections (ARMED) | **attention root-claim `operand_openings`; IR / generative root-claim `params`** | **M** | **P** | **armed: unchanged; §7.5** |
| | accuser openings (shard, TIR shard, held close, `ObjectiveOffence`, court close) | M | V | unchanged (allowed) |
| Readiness (ARMED) | **~16 plaintext artifact leaves per (class, bond, span)** | **M** | **S** | **armed: unchanged; §7.5** |

Per-claim caps on the armed side: R-core DA-8 allows 1 named + 3 drawn units per session; non-seat bonds get 3 open and 16 ever; seats
get 4. Weight bytes have **no cumulative cap**. Spread over a class's claims, `StepLeaf` demands can enumerate every weight row its leaves
read, and readiness proofs cover the inventory over time.

### 7.2 The rule (allowed units)

`palw_court_demand_allowed_v1(kind)`: a unit a party can be compelled to supply (D / P / S) must be claim-specific. Model bytes reach a
court only as V.

At the node level, `palw_court_node_materials_v1(program, model_params)` classifies every committed value `[occurrence][node]`, aligned
with `derived_mask_v1`:
- `Public`: consts only;
- `Claim`: model-independent;
- `ModelDependent`: an activation;
- `ModelCopy`: copies of weight elements selected by claim data, e.g. a `Gather` of an embedding row;
- `ModelOnly`: a function of the weights alone, e.g. a transposed or dequantised weight.

States and carries are solved to a fixpoint. `model_params` separates real weights from a TIR v2 stage's lifted inputs.

- **Level 1** (`palw_court_model_bytes_mask_v1`, the rule): `ModelCopy` and `ModelOnly` are never owed in the clear.
- **Level 2** (`palw_court_model_dependent_mask_v1`, a user decision, §7.4): every model-dependent value is owed as commitment hashes
  only.

### 7.3 The cumulative bound, and the kernel-route enforcement

* **Bytes.** Model-byte disclosure is 0 per claim and per model, for any sequence of demands. Each unit's material is fixed by its kind,
  and model bytes are never compellable. Repeated demands can never return a weight, a file range or a copy of either.
* **Per claim.** Every claim-specific unit at most once (served ⇒ public ⇒ refused again). The claim's disclosure ⊆ its own committed
  material.
* **Per (claim, requester operator).** ≤ `PALW_COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 = 16` distinct units (INTERIM, as R-core DA-8's
  "16 ever"). Kept in table 45's `requested`. This counts against nobody else, so it is G14-safe. One prosecution needs ≤ 2 positions +
  ≤ 10 root probes (K2S §2, §11.2), or one entry.
* **Per model.** The union over the model's claims. It grows only with claims producers commit, never with demands.
* **Enforced** in `apply_kernel_route_object_v1` past the fence, by `court_scope_admit_demand_v1`:
  - the demanded `(claim, stage, position)` is classified by `palw_kernel_demand_unit_v1`;
  - a model-content unit is dropped;
  - the tally is recorded only once the kernel accepts the demand.
* **Enforced** in tag 151 by `claim_unit_in_scope`.

### 7.4 The relational residual (Level 1) — a decision for the user

A clear activation is claim-specific, but enough of them reveal a model relation:
- `y = x ⊙ g` reveals `g` from one position;
- `y = W x` reveals `W` from `K` positions, where `K` is the contracted dimension.

`palw_court_model_exposure_v1(program)` reports both per program. No count cap can stop a Sybil extractor without also being a cap a
producer can exhaust against an honest prosecutor. Two non-griefable options:

1. **Economic (keeps Level 1).** G14-R4's demand-bond burn on a served, non-convicted claim prices reconstruction at
   `≥ K × demand_bond` per dense relation, and `≥ demand_bond` per elementwise relation. An honest prosecutor of a fraud is refunded.
2. **Level 2.** The court owes model-dependent values only as commitment hashes. The court terminal becomes:
   - hash bisection by the verifier, from its own re-execution;
   - then recompute-and-compare of one element against the producer's authenticated leaf hash.

   Reachability is preserved for a model holder (ADR-0177 D7). Mid-claim spot checks under withholding are lost (K2S Route A); Route B,
   re-execution, remains. This needs element-granular leaf hashes in K2S's v3 tree, and is the only option that makes "never rebuild"
   literal.

### 7.5 The verifier's own model operand, and the armed t12 routes

**Authentication** is `palw_verify_verifier_model_operand_v1(roots, operand)`, against the class's REGISTERED roots (immutable, ADR-0175):
- a row or column of `(param, layer)`: `commitments.root() == registered kernel_param_root`, then
  `opening.authenticates(commitments.by_instance[(param, layer)])`. The commitments map is on chain in the kernel class registration, so
  the verifier copies it and needs nothing from the producer;
- a V2 inventory leaf: `verify_artifact_opening_v1(opening, registered artifact_root)` at the registered leaf count.

Root equality is not proof of computation; the court still evaluates the relation.

**Armed t12 routes that compel model bytes (not changed).** These are:
- the held `StepLeaf` answer's `artifact_openings`;
- the attention / IR / generative dissection root claims' parameter openings;
- seat readiness leaves.

**Proposed new dormant fence (the Lead allocates the name, e.g. `palw_court_model_scope_v1`):**
1. a `StepLeaf` answer owes only its claim part (the refutation's step openings, the prompt-ids opening); the accuser supplies
   `artifact_openings` from its own copy in the one-move verdict filing;
2. a dissection's root claim names parameter coordinates only, and the challenger supplies the openings;
3. readiness stops publishing plaintext leaves (a user decision: it is a Panel seating condition, PoR-like under ADR-0177 D1).

**Reachability:**
- (1) the one-move verdict needs the openings, and any holder of the model supplies them, so withholding the claim part is still the
  producer's default;
- (2) a dissection's terminal reads the same openings from either side;
- (3) is not a court.

### 7.6 GAP-52 — snapshot leaf well-formedness, by making the court total

A registration-time "every leaf is well-formed" attestation cannot be refuted: a leaf with no preimage has nothing anyone can open. So
the court is made total instead (`misaka-palw-kernel/src/spec/retrieval.rs`):
- the item at an id is authenticated by its leaf alone;
- a malformed one is no item the rule retrieves: `WrongItem` convicts a claim that names it, and `MissedBetter` dismisses it;
- a retrieval claim's own entries are a demand stage, `0xC0 + s` (position = entry index). `classify_entry_response_v1` serves the item
  at the entry's id (any shape) or nothing.

Reachability, with the producer and every seat colluding:
- a junk leaf can be opened by no one, so an entry naming it is the producer's **default**;
- a malformed leaf is served, then **convicted** by `WrongItem`;
- a wrong item is **convicted** from the verifier's copy;
- an honest claim is **dismissed** and reaches Final with nobody serving the snapshot.

The entry is the claim's own stated output (C), never the snapshot (M). This also replaces the withdrawn slice DA.

### 7.7 The identity binding without the provider court

Tag 105's `Row` refutation needs a row of the BOUND tensor. Before the re-scope, the provider court forced it out; now it can come only
from the binder's voluntary publication, unless the instance sets differ (`Instances`).

**Proposal (allocation needed):**
- a binder-scoped demand of HASH units only (`KernelRowNodes` runs of the bound tree; no bytes), answered by the binder, with default ⇒
  binding refuted;
- a tag-105 variant `RowLeaf { path, bound_leaf_hash }` that convicts when `H(true row from the verifier's own V2 bytes) ≠` the bound
  tree's authenticated leaf hash.

No model bytes are compelled, and a false binding becomes refutable without the binder's cooperation. **Decision:** whether failing to
open one's own commitment's hashes may slash the binder (ADR-0177 D1 forbids penalties only for not serving the model).

### 7.8 Where K2S must call the predicate (K2 v4/v5; demand types untouched)

1. `seg_da::{declared_values, position_parts_v1, position_material_v1}`: drop `palw_court_model_bytes_mask_v1(program, real)[occ][node]`
   values from a position's owed parts. Under Level 2, drop `palw_court_model_dependent_mask_v1`. A dropped value is owed as its
   row-tree leaf hashes (commitment structure).
2. `seg_da::{classify_part_v1, assemble_position_v1}`: classify against that layout.
3. `element::{operand_sources, element_value_v1}`:
   - a masked operand comes from the filer's recomputation; its leaves authenticate against the committed node commitment when the value
     is right;
   - a WRONG masked value is convicted by recomputing one leaf from verifier-supplied param leaves, compared with the producer's
     authenticated leaf hash;
   - param leaves are authenticated by `palw_verify_verifier_model_operand_v1` (the registered v3 commitments).
4. The base kernel: `KernelLedgerV1::derived_mask` (`ledger.rs`) ORs the Level-1 mask. `verify.rs` resolves a masked node from verifier
   openings, the same way as a derived one.
5. `consensus-core palw_court_scope_v1::palw_kernel_demand_unit_v1`: add `ClaimBodyV1::Segmented` ⇒ `KernelPosition`, so the fold's
   admission and tally cover segmented demands too. Today an unknown body returns `None`, which means kernel-decided and untallied.
6. `seg_ledger::settle_served_demand_bonds_v4`: G14-R4's burn is §7.4's economic bound.
