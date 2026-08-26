# ADR-0053: One execution family — Family M is withdrawn, and the court is not optional

Status: **Accepted.** Supersedes ADR-0051. Removes the Metal/GGUF execution family, the family
concept, the per-class panel, the runtime pins and the `misaka-palw-metal` crate. Moves
`PALW_STATE_V2_VERSION` (7 → 8) and the ruleset id, so testnet-11's consensus fingerprint moves and
every network re-mints.

Date: 2026-08-26

Relates to: ADR-0051 (**superseded**), ADR-0026 (whose thesis 0051 walked back for one family and
which is hereby restored in full), ADR-0038/0039 (PALW is the consensus work; no weight without a
complete catalog), ADR-0040 (the integer family that is now the only one), ADR-0045 (the share
table — a row of which 0051 reserved for this family), ADR-0049 (the adjudication contract the
withdrawn family deliberately never entered), ADR-0052 (`PALW-QWEN36`, the measurement that
removed 0051's motive), ADR-0034 (routing — its `Metal` family becomes reserved here).

---

## Context — the motive expired, and then the mechanisms turned out not to exist

ADR-0051 was an honest response to a real measurement. On 2026-08-22, making Qwen2.5-1.5B a
MISAKA-arithmetic class cost a canonical IR projection, an arithmetic canonicalization, a per-model
converter and a 1.7 GiB re-quantized artifact — and the result still produced a degraded argmax
(`[11,11,11,11]`), because static PTQ into BASE-0's int8 stream lost the model. The same checkpoint
in native GGUF ran at full quality at ~40 tok/s. From that, 0051 concluded that the lane whose job
is UX must ride on an existing runtime, commit to what the inference *said*, and verify by tolerant
replay — accepting, by name and capped at half the economy, a proof model that can never convict.

**That premise is dead.** ADR-0052 and the work that landed with it put Qwen3.6-35B-A3B's forty
layers — thirty GatedDeltaNet arms, ten gated-attention arms, a 256-expert mixture — through the
integer runtime, generating real text, with a kernel catalog covering **100 %** of what the graph
can reach and an adjudicator whose every arm calls the same function the engine calls. The model
the black box existed to serve is adjudicable without the black box. Whatever Family M was worth,
it is not worth it against a court-adjudicable class that runs the same checkpoint.

That alone would justify a re-scoping. What justifies a **withdrawal** is what an audit of the
shipped code found underneath ADR-0051's three safety claims.

### The 500‰ cap was never written

Decision 1's whole safety argument is that Family M's non-convictable work is bounded at half the
share table, "so the half that can convict a liar stays in charge of the tie".

`PalwClassAdmissionError::FamilyShareCap` exists as an error variant. **It has zero construction
sites.** Nothing in admission, in the genesis loader, in the share table or in the class DAA ever
computes a per-family sum or compares one to a cap. A Family-M class registered at any permille the
share rules allowed, and the bound that made the family acceptable was a doc comment.

This is the failure mode this repository has now recorded four times: *a gate that never fires is
more dangerous than no gate*, because the design reasons as though it fired.

### The per-class panel was checked and then ignored

Decision 6 called per-class panel parameters "the one structural consensus change this ADR
requires": a class could register `(seats, quorum)` thinner than the network's, floored by
`min_class_panel` in the bundle, so a Metal class could license with two seats instead of six.

Two things were true of the implementation:

1. **It was not scoped to the family.** `verify_class_admission_v2` calls `terms.panel_params(…)`
   at the top of the function, *before* any family dispatch. `min_class_panel: (2, 2)` shipped in
   `palw_fp_devnet_bundle_v3`, which every preset's params are built from. So on every shipped
   network, a **deterministic** class could also register a two-seat panel. A field added to let a
   black-box family exist quietly lowered the licensing bar for the family that can convict.
2. **It changed nothing anyway.** The panel a claim is actually bound with comes from
   `derive_panel_v2(state, panel_params, …)` in the virtual processor, and `panel_params` there is
   the node's bundle-global `PalwPanelParamsV2`. The class's registered `(seats, quorum)` never
   reaches it. A class admitted at 2-of-2 was still bound a 5-seat panel.

A ruleset field that weakened a gate and altered no behaviour is not a feature with a bug in it. It
is a schema cost paid for nothing, twice: `PALW_STATE_V2_VERSION` went 5 → 6 → 7 in a single day to
carry `terms` and then `runtime_pins`, for a family that never produced a block.

### The admission arm skipped every check that makes a class prosecutable

This is the strongest line in the case. `verify_class_admission_v2` dispatched a non-adjudicable
class to `palw_metal_class_admission_v1`, which:

* ran `validate_geometry` **instead of** `validate_shape`;
* skipped the **ADR-0038 A4 coverage gate** entirely — the gate whose whole purpose is refusing a
  class whose disputes would come back `Unadjudicable`, "rejected but unslashed, the hole a forger
  farms";
* skipped the **ladder-depth** check (`DeeperThanTheLadder`) and the **ADR-0049 Decision C court
  cost ceilings** (`CourtCostExceedsCeiling`);
* and wrote a `PalwClassCatalogEntryV2` whose `reachable_kernels` was `Default::default()` — the
  **empty set**. The class catalog root, which is inside the ruleset id, committed to a class the
  court knows nothing about.

Every one of those checks was written to refuse exactly the object this arm constructed. The
justification was internally coherent — "running them would refuse every Family-M class for failing
to be something it never claimed" — and that is precisely the argument that should have ended the
proposal rather than opened an arm around it. A class that cannot be prosecuted is not a class with
a different verification scheme. It is the thing the gates are for.

### And it could not have run — for a reason no operator could fix

The obvious version of this is **wrong**, and an earlier draft of this ADR published it: "the fleet
holds no Apple Silicon, so a family whose seats replay on Metal has nobody to seat." What
testnet-11 actually registered is not a Metal class. `682756bc…` is the **Linux CPU** build of the
pinned llama.cpp — commit `28f1f623` records the measurement ("the Linux CPU class is live at
682756bc…"), and the deployed worker reports `accelerate:false, avx2:true`. The family's identity
is its pins, not its silicon, so a CPU build qualifies; four fleet hosts carry that exact worker.
Replay capacity existed.

It still never produced a block on any chain, and the reason is a **fixed point in the class
identity** rather than a hardware shortage:

1. every Family-M execution goes through `MetalBackend::run_worker`, which invokes the worker at
   `--mode v2-legs-job`;
2. the deployed worker (built 2026-08-14) does not implement that mode. It refuses — measured still
   refusing, about twice a second, on 2026-08-26;
3. the repo's worker DOES implement it, and that is exactly why it cannot be deployed:
   `worker_binary_sha256` is inside the preimage of `PalwRuntimeManifestV2::manifest_hash()`, so a
   rebuild is a different runtime and `check_runtime_identity` refuses it for a class pinning the
   old manifest;
4. and the class id is the shape profile id, which does not move when the binary does — so
   registering the repaired worker collides with the class already there:
   `PalwStateV2Error::DuplicateClass`.

No configuration, rebuild or restart closes that loop. Only a re-mint does. A family whose sole
deployed class cannot be repaired without re-minting the network is not a family having an outage;
it is one whose identity scheme has no repair path — a structural argument for withdrawal, where
"we own no Macs" was a contingent one that a reader could answer by buying a Mac.

Two claims from the earlier draft are corrected rather than carried:

* **the panel arithmetic.** Quorum is **3 of 5**, not 5 of 5 (`PALW_V2_PANEL_SEATS = 5`,
  `PALW_V2_PANEL_QUORUM = 3`), and a drawable panel needs `SEATS + 1` distinct operators because a
  panel excludes the executor. Both are true of every class on the network, not of this family, so
  neither carries weight here;
* **the budget observation.** A class holding share while producing nothing was measured lying
  about its epoch budget for 33 hours — but the mid-epoch budget fix was **reverted on main**
  (`b57adc83`, "the mid-epoch budget fix rewrites history, so it cannot ship to a running chain"),
  and the class now reports `share=1 budget=1` and still produces nothing. It stands as a
  historical observation, not as a present-tense claim.

The economic point survives both corrections, in its narrow form: a share granted to a class that
produces nothing is not idle, because per-class DAA and epoch budgets reason about expected
production.

## Decision 1 — There is one execution family, and it is not a value

A class is verified one way: pinned integer arithmetic, a graph projected from a canonical IR,
disputes ending in the ADR-0049 court. This is **not** recorded as a single-variant enum. The type
`PalwExecutionFamilyV1` (the verification scheme, `consensus/core/src/palw_backend.rs` — distinct
from ADR-0034's routing family of the same name) and its `is_court_adjudicable()` are deleted.

Every registered class is court-adjudicable **by construction**, which is a stronger statement than
a flag that says so: a flag has an arm, and the arm the withdrawn family needed was the one that
skipped the coverage gate. A future second family is a new ADR that re-derives the whole question,
not a variant somebody adds to a live enum.

## Decision 2 — `PalwClassTermsV2` is deleted, and the class record shrinks

Gone from the chain: `terms.family`, `terms.runtime_pins` (`PalwRuntimePinsV2`), and
`terms.panel_seats` / `terms.panel_quorum`. Gone from the bundle: `min_class_panel` and the
`min_panel_seats` / `min_panel_quorum` accessors; gone from `PalwRegistrationTermsV2`, the two
fields a registrant read them through.

`PALW_STATE_V2_VERSION` goes **7 → 8**. A shrinking record forks a chain exactly as loudly as a
growing one, and ADR-0043 §2's change rule applies unchanged: the preimage moves, so the version
moves, and `the_version_8_state_root_golden_vectors` is where that is declared. The spec-side
second implementation of the state root moves with it — the correspondence is a round trip or it is
nothing.

## Decision 1a — What "one gate" does and does not claim, at genesis

Decision 1 is a statement about `verify_class_admission_v2`, the POST-GENESIS path: there is one
arm, and every registration that reaches it runs the whole of `validate_shape`, the ADR-0038 A4
coverage walk, the ladder-depth check and the ADR-0049 Decision C court-cost ceilings. Removing the
family removed the second arm; it did not make the genesis path identical to that one, and this
section says so rather than letting a reader assume it.

A **genesis** registration carries `admission: None`, because the ruleset id already commits to a
catalog describing the class. `verify_palw_genesis_v2` therefore checks the registration against
`PalwClassCatalogV2` rather than against a carried profile, and `verify_against_catalog` gates:

* the catalog is the one the ruleset root commits to;
* BASE-0 is present (the liveness floor is registered);
* **A4 coverage, for every entry** — so a genesis class whose graph reaches an uncatalogued kernel
  is refused, exactly as a post-genesis one is;
* **ladder depth** — `court.max_step_leaf_count() >= catalog.max_step_leaf_count()`.

It does **not** check the court-cost ceilings. `derive_court_cost_v1` appears nowhere in the
genesis path, so `max_opening_bytes` / `max_terminal_macs` / `max_operand_count` bound a
post-genesis entrant and not a class minted into a genesis. A class can therefore be seated at
block zero whose cheapest prosecutable step costs more than the ruleset allows — coverage-clean,
ladder-deep enough, and unpolicable, which is the exact condition ADR-0049 Decision C named.

Two further facts about the genesis path, stated so nobody has to rediscover them:

* the catalog's numbers (`reachable_kernels`, the two leaf counts) are **minted, not re-derived**.
  With no carried profile there is no graph to walk at load, so coverage is checked against the set
  the catalog asserts. Whoever mints a genesis is trusted to have derived it from the profile —
  which is why the mint path and the admission path must build entries with the same function, and
  why `verify_class_admission_v2` returns "what a genesis catalog would have held for this class";
* this is a **pre-existing** hole, not one this ADR opens. It is recorded here because Decision 1's
  "there is no arm that skips it" is true of the gate it names and would be false as a claim about
  every path a class can enter by. The fix — carrying the derived cost in the catalog entry so the
  bundle can check it the way it already checks depth — belongs to the class-catalog work, not to
  withdrawing a family.

## Decision 3 — One panel: the network's

Panel seats and quorum are bundle-global again. A class does not get to be licensed by fewer
operators than the network decided, and the network's decision is inside its ruleset id, where a
registrant cannot reach it.

## Decision 4 — `--palw-register-class` survives, pointed at a class the court can judge

ADR-0049 Decision H's post-genesis registration path is kept, because the thing it carries was
never family-specific: a profile, a canonical job, a bond's signature. What is removed is the only
builder it had — `family_m_post_genesis_registration_v1`, which could construct exactly the one
black-box class its own crate pinned.

Its replacement, `palw_post_genesis_registration_v1` (in `palw_class_admission_v2.rs`, beside the
gate that reads what it builds), is a generalization rather than a port: a deterministic class's
identity is its graph, which the caller already holds, so it builds a registration for **any**
profile. `kaspad --palw-register-class` now registers the class of the converted artifact the node
loaded, matched by shape against the build's own class registry. An operator who put an artifact on
disk has already said which class this node is for; a second declaration is a second place for them
to disagree.

Three things stay out of the caller's hands, for the reasons the withdrawn builder already gave:
the **share** (an entrant joins at the ruleset's minimum grantable permille), the **class id** (it
is the profile's id — a class IS its graph), and **`pwu_per_inference`** (counted from the canonical
job, so a declared value can only fail).

## Decision 5 — ADR-0034's `Metal` routing family becomes reserved

The routing family (`palw_routing.rs`, hardware) is a different type from the withdrawn
verification family, and ADR-0034 is not otherwise touched. But `Metal` was admissible there for
one reason — the withdrawn family was going to route to it, and a tolerant verifier does not need
Apple floats to be reproducible. Nothing routes there now.

A Metal binding today would have to be a deterministic-integer class executing on Apple GPUs. There
is not one line of GPU kernel in this tree, and no vendor guarantees the float reproducibility such
a class would have to be adjudicated against. `family_is_reserved_v1` therefore covers `Metal`
alongside `Cuda` and `Rocm`: it returns the way they do, through the ADR-0027 falsifiable
conformance campaign, not through a code edit. The discriminant stays `1` because it is on the wire.

## Decision 6 — What is deleted, in full

| | |
|---|---|
| `misaka-palw-metal` (crate, 1,538 lines) | `MetalBackend`, `CAT-M-0001`, the GGUF catalog, the pins, the workspace + `kaspad` dependency |
| `PalwExecutionFamilyV1` / `is_court_adjudicable()` | `consensus/core/src/palw_backend.rs`; `fn family()` leaves the backend trait |
| `PalwClassTermsV2`, `PalwRuntimePinsV2` | `consensus/core/src/palw_state_v2.rs`, and `terms` from `PalwClassStateV2` and `ClassRegistered` |
| `PalwClassAdmissionError::{FamilyShareCap, PanelTerms}` | the dead cap and the panel check |
| `palw_metal_class_admission_v1` | the arm that skipped `validate_shape`, A4, the ladder and the ceilings |
| `min_class_panel` + accessors, `PalwRegistrationTermsV2::min_panel_{seats,quorum}` | the ruleset field and its readers |
| `PalwProducerFactsV2::terms`, `PalwSeatDutyV2::terms`, `PalwCourtDutyV2::terms`, `PalwDisputableClaimV2::terms` | the family a producer, a seat and a challenger each had to dispatch on |
| `--palw-metal-worker`, `PalwBackendRegistry`'s family match | the CLI flag and the node-side dispatch |

`misaka-palw-worker` (the pinned llama.cpp build) is **kept**. It is not Family M; it is the runtime
the ADR-0034 capability probe interrogates, and no crate depends on it. Its build-time message and
the README no longer claim it serves a registered class, because it no longer does.

## Consequences

* **Every network re-mints.** The ruleset id moves (`min_class_panel` left the bundle) and the state
  root moves (version 8), so testnet-11's consensus fingerprint moves:
  `a708dc4a…` → `a1284a00…`. This is a coordinated upgrade — the fingerprint is what peers compare
  at the handshake, so old and new builds will not agree. Testnet-11 is the only preset that moves,
  being the only one carrying a PALW V2 bundle.
* **ADR-0026 is restored in full.** Its thesis — take Ambient's architecture, refuse its proof
  model, build exact-within-pinned-class — no longer has an exception carved into it.
* **The UX question is re-opened honestly.** ADR-0051 existed because a usable model in the
  deterministic family looked unaffordable. It is now affordable but not free: ADR-0052's class runs
  and is adjudicable, and its fidelity and speed are engineering work, not a proof-model problem.
  If a future measurement says a tolerant family is needed after all, it needs a new ADR that
  answers what 0051 did not: who enforces the cap, what the seats' hardware distribution actually
  is, and which gate refuses a class whose disputes cannot terminate.

## What this does NOT walk back

| | |
|---|---|
| Canonical IR / ADR-0049 | Untouched. The machinery of every class. |
| BASE-0 floor | Untouched. Liveness anchor, derived artifact, court intact. |
| ADR-0034 routing/bands | Kept, minus one admissible family (Decision 5). |
| ADR-0044 free-prompt lane | Untouched. The UX carrier is orthogonal to which family serves it. |
| ADR-0049 Decision H post-genesis registration | Kept, with a better builder (Decision 4). |
| ADR-0045 share table | Kept. What is removed is a cap that was never implemented, not the table. |

## Reading order for a reviewer

1. ADR-0051 — the proposal this withdraws; read Decisions 1, 4 and 6 against the Context above.
2. `consensus/core/src/palw_class_admission_v2.rs` — one gate, no second arm.
3. ADR-0052 — the class that removed 0051's motive.
4. `consensus/core/src/palw_backend.rs` — the seam that survived losing the thing it was built for.
