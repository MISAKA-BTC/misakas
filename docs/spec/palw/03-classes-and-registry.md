# PALW spec — 03. Classes and registry

> **Normative.** This chapter states the rules as they are on each network today. Reasoning:
> [design/palw/registry.md](../../design/palw/registry.md). The code is the truth, and disagreements
> are listed in [divergences.md](divergences.md).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (from genesis; `palw_registry_resilience`
from DAA 750)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P7 throughout; P4 (identity and work are derived, never declared); P2 (the
chain never judges a model's use).

A **class** is a model the chain can verify and pay for. A class is **chain data**: anyone registers
one, without changing `main`, by a consensus object. Registering is **listing**. A listing is
permissionless, asynchronous and priced in MSK, and it earns nothing until the chain has seen
independent seats ready to verify it. Eligibility then grows through a lifecycle driven by verified
events. It changes *how much* a class may contribute, never what a unit of its work is worth.

## 3.1 Classes are chain data

- **PALW-CL-1 (the catalogue is state).** The class catalogue MUST be chain state: genesis rows plus
  registered rows. A compiled table is only genesis bootstrap and cache. A class executes from its
  registered profile (04). Only kernels are part of the build.
- **PALW-CL-2 (identity is derived).** A class id MUST be derived from canonical data: the graph IR
  root, the artifact root, the canonical job, the quantization and the runtime version. A name or a
  registrant's declaration never sets it. Model classes have their canonical job fixed at
  `(n_ctx/8 − 1, 2)` under `palw_offence_attribution` (09 PALW-CT-8).
- **PALW-CL-3 (the row).** A class row (`PalwClassRowV2`, `PalwClassStateV2`) records its status
  (`Registered`, `Active`, `Frozen`), its registry row, and its held-regime ladder when it is held
  (`class_step_ladders`).

**Sources:** ADR-0067 D1–D3, ADR-0100 D6, ADR-0135 D1, ADR-0145 §3, ADR-0152 J-5. **Code:**
`core/palw_class_identity_v1.rs`, `core/palw_state_v2.rs` (`PalwClassRowV2`, `PalwClassStatusV2`).

## 3.2 Registration is a listing

- **PALW-CL-4 (permissionless).** Anyone MAY register a class with `ClassRegistered` or
  `ClassManifestV2` (tag 48). The registration carries the manifest: graph root, artifact root and
  bytes, canonical job, quantization, runtime version, bond, and the public source of the artifact
  (`palw_public_model_source_required`). No allow-list exists. Admission is arithmetic, and only
  arithmetic.
- **PALW-CL-5 (derived, never believed).** From the manifest, every node MUST derive the same work
  (`PalwModelWorkV1`) and profile against one set of registry globals (`PalwRegistryGlobalsV1`). The
  profile covers the verification window, prefetch, in-flight cap and required ready seats.
  Admission recomputes and never trusts a rate a registrant states (`palw_measured_model_v1.rs`,
  `palw-class measure`/`verify`). Nothing a registrant writes may multiply reward (05).
- **PALW-CL-6 (the court bounds admission).** A class MUST be admitted only if the court can try it
  (09 PALW-CT-14, PALW-CT-15). Under `palw_offence_attribution` it must also have a logits head, reach
  no Kimi-K3 kernel, and use the `MerkleV1` prompt-ids form above 4,096 ids (09 PALW-CT-8).
- **PALW-CL-7 (priced in MSK, not in time).**
  - Registration exposure is priced in bonded collateral, and duplicates are priced, not policed.
  - Registration pays the certification rent (`palw_certification_rent`).
  - A listing has no deadline before its first panel. Synchronous deadlines start only once a court
    starts. *(The operator's decision of 2026-09-25; divergences.md row 1.)*
- **PALW-CL-8 (reclamation).** A class that produces nothing for its reclamation period gives its
  capacity back (`apply_class_reclamation`). Past `palw_activation_pool` (testnet-12), silence never
  reclaims a class that cannot produce (`Candidate`, `Registered`, `Prefetching` or `Held`), nor a
  genesis row.
- **PALW-CL-9 (tools admit what the chain admits).** The CLI and the node MUST offer every row the chain
  admits. They add no tool-side narrowing *(operator decision T12-030, 2026-09-26)*.

**Sources:** ADR-0056 D1–D3, D5–D7; ADR-0049 H; ADR-0099 D6–D7; ADR-0100 D3; ADR-0135 D1–D3, D7;
ADR-0144 P7. **Code:** `core/palw_model_registration_v1.rs`, `core/palw_model_registry_v1.rs`,
`core/palw_class_admission_v2.rs`, `core/palw_public_model_source_v1.rs`.

## 3.3 Lifecycle: registered is not eligible

- **PALW-CL-10 (states).** A class's registry row walks
  `Candidate → Prefetching → Probation → ActiveLimited → Active`, with `Held` as the state that stops
  only its own new claims (`PalwModelLifecycleV1`, stepped by `palw_lifecycle_step_v1`):
  - Past `palw_admission_independence` a registration starts at `Candidate`, which means it exists and
    is inspectable but earns nothing.
  - Below that fence a registration starts at `Registered`.
  - The order is `palw_lifecycle_step_v1`'s, not the enum's tag order.
- **PALW-CL-11 (budget, not price).** A state sets how much the class may contribute
  (`admission_permille`): 50 ‰ in `Probation`, 100 ‰ in `ActiveLimited`, 1,000 ‰ in `Active`, and 0
  otherwise. It never changes the value of a unit of work (P7).
- **PALW-CL-12 (transitions come from verified events).**
  - **Candidate → Prefetching:** a ready seat not held by the registrant is observed, and the drawn
    admission jury audits the class.
  - **Probation → ActiveLimited:** after `probation_claims` (10) completed probe claims with enough
    ready seats.
  - **Into `Held`:** the class's panel cannot be drawn, its utilization passes the cap, or its
    verification window does not fit its receipt deadline (08 PALW-VF-29).
- **PALW-CL-13 (independence is drawn).** Admission independence MUST come from the draw, never from a
  declaration (`palw_admission_independence`):
  - an **admission jury** drawn per audit from bonds at the panel floor (`palw_admission_jury_v1`;
    audits every `palw_admission_audit_period_daa`, staggered per class);
  - an **outsider seat** on every claim of a bought class (08 PALW-VF-5);
  - a population fixed before its randomness.

  The jury is not stake-weighted (08 §8.2).
- **PALW-CL-14 (resilience, from DAA 750).** Past `palw_registry_resilience`:
  - a class held for readiness keeps its probation progress;
  - probation resets only on failed probes from two or more distinct producer bonds.

  Both are judged by the first DAA of the span a probe is counted in.
- **PALW-CL-15 (the weight gate).** Only a class certified end to end carries weight in fork choice. The
  certified set is genesis ∪ chain (`family_certified_for_weight_v2`). An uncertified family's blocks
  weigh nothing.

**Sources:** ADR-0135 D4–D6; ADR-0145 §7; ADR-0147 §2; ADR-0069 D1–D7 as amended by ADR-0075; ADR-0133
§11.3; ADR-0154 (D1). **Code:** `core/palw_model_registry_v1.rs` (`PalwModelLifecycleV1`,
`palw_lifecycle_step_v1`, `admission_permille`), `core/palw_panel_v2.rs` (`palw_admission_jury_v1`),
`core/palw_class_admission_v2.rs`.

## 3.4 Certification objects

- **PALW-CL-16.** Certification MUST be a consensus object, never a build fact:
  - `FamilyCertified` (tag 13) and `ClassLaneCertified` (tag 14) are carried by ordinary transactions,
    and the transition re-runs the shipped grader.
  - Anyone may submit one.
  - A certificate is scoped to the lane it was drilled on and bound to the network identity. It has no
    expiry by time and no revocation object; a misbehaving class is handled by the court and the
    lifecycle.
  - A family's coverage grows by posting a second certificate.

**Sources:** ADR-0075 D1–D14, ADR-0069 D2–D3. **Code:** `core/palw_e2e_adjudicability.rs`,
`core/palw_state_v2.rs` (the certified sets).

## 3.5 Artifact roots and lines

- **PALW-CL-17 (one owner).** Past `palw_artifact_root_ownership`, an artifact root MUST have exactly one
  owner, stored in rooted state and reserved atomically at registration. Every entrance refuses a
  duplicate root. Competing weights stay permissionless under their own roots. Legacy duplicate rows
  stay as history, and nothing settled before the fence is recomputed.
- **PALW-CL-18 (roots in force).** A class's roots in force are its founding root, its line's current
  version, previews, and a superseded version inside its grace period. An attempt whose artifact root is
  none of these is refused (15 §15.1).

**Sources:** ADR-0143 D1–D9, ADR-0088 D3. **Code:** `core/palw_state_v2.rs` (the root-ownership index),
`core/palw_model_lines_v1.rs`.

## 3.6 The Activation Pool and the verification deadline

- **PALW-CL-19 (the Activation Pool).** Past `palw_activation_pool` (testnet-12, genesis only):
  - Anyone MAY fund a class's pool (`ActivationPoolFunded`, tag 58).
  - The pool pays operators who objectively prove they prepared the model: readiness credits during
    probation, and a bonus per `Final` capped by the claim's seat pay (`palw_activation_bonus_cap_v1`).
    It never pays for the verdict they vote.
  - Each row keeps `funded == prep + bonus + scheduled + paid + withheld`.
  - Payee lists are capped at 256.
  - The same fence arms R1 (silence never reclaims a class that cannot produce), R2 (listings are
    audited at their own staggered span), and a jury drawn from panel-floor bonds.
- **PALW-CL-20 (the verification deadline).** Past `palw_class_verify_deadline`, a claim's
  compute-bearing verification deadline `D(c)` is derived from its class in reference spans of 5 DAA
  (`PALW_CLASS_VERIFY_REF_SPAN_DAA_V1`), unless a measured row applies (`palw_class_verify_rows`, empty
  at testnet-12 genesis). The response windows (court turn, `window_court`, `window_receipt`) stay
  global. `Final` is floored by `palw_claim_final_floor_v1`. The 2M row is closed until ADR-0153's
  flag day installs its measured row.
- **PALW-CL-21 (C7).** Classes with a verification window of at least 1,000 spans, plus
  `palw_rcore_conservative_classes`, are held to Final and to their static cap (10 PALW-CO-15).

**Sources:** ADR-0152 (the Activation Pool; DL; §4-quater), ADR-0147 §6. **Code:**
`core/palw_activation_pool_v1.rs`, `core/palw_class_verify_deadline_v1.rs`.

## 3.7 Held classes and context limits

- **PALW-CL-22 (held classes).** The chain MUST record which classes are held (`class_step_ladders`).
  A held class:
  - is walked, priced and prosecuted at the held ladder, `2^40` leaves (`PALW_HELD_STEP_LADDER_V1`);
  - carries its own prompt-ids form (the tiled Merkle root);
  - is admitted and counted at that ladder.

  Every other class keeps the network's ladder.
- **PALW-CL-23 (context limits).** A context limit MUST be armed only by a release whose evidence anyone
  can reproduce. A vector is a name, a seed and a geometry, and everything else is derived and checked
  by the network's own pipeline. A wider context is a release, and the release cites its vectors.

**Sources:** ADR-0118 D1–D7, ADR-0119 D1–D7, ADR-0110 D1–D7. **Code:** `core/palw_held_context_v1.rs`,
`core/palw_context_ladder.rs`; the vector tools under `misaka-palw-*`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis: registry, lifecycle, independence, certification, artifact-root ownership, Activation Pool, class verify deadline, held regime. DAA 750: `palw_registry_resilience`. The genesis classes and held rows are in 16 PALW-NP-9 |
