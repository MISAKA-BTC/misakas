# PALW spec — 03. Classes and registry

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions.

**Purpose.** A *class* is a model the chain can verify and pay for. This chapter covers how a class
is identified, how anyone registers one without changing `main`, how its registration is priced, and
how it earns eligibility through verified use. Registering a model is **listing** it. Listing is
permissionless and cheap to leave standing. Eligibility is scarce and earned (P7). This chapter also
covers the Activation Pool that orders classes into verification, and the per-class verification
deadline.

**Principles served:** P7 (throughout), P4 (identity is derived, not declared), P2 (the protocol never
judges the model's use).

## 3.1 Class identity

- [ ] A class id is derived from canonical data (graph, arithmetic tier, artifact root and profile),
  never from a name or a registrant's declaration. *Sources:* 0145 §3, 0067 D1, 0100. *Code:*
  `core/palw_class_identity_v1.rs`.
- [ ] Classes are chain data. The chain holds the class catalogue, and only kernels are part of the
  build (chapter 04). *Sources:* 0067 D1–D3, 0053 (re-scoped).
- [ ] The class row and its status (`Registered`, `Active`, `Frozen`). *Code:* `core/palw_state_v2.rs`
  `PalwClassRowV2`, `PalwClassStateV2`, `PalwClassStatusV2`.

## 3.2 Registration is a listing

- [ ] Anyone MAY register a compatible class by a chain object. The chain keeps no list of allowed
  models. *Sources:* 0056 D1, 0049 Decision H, 0135, 0144 P7. *Code:*
  `core/palw_model_registration_v1.rs`, `core/palw_model_registry_v1.rs`.
- [ ] The registrant measures and the chain recomputes. The profile is derived, and nothing a
  registrant writes may multiply reward. *Sources:* 0099 D1–D4, 0135, 0145. *Code:*
  `core/palw_measured_model_v1.rs`, `core/palw_resource_profile_v1.rs`.
- [ ] A registration is asynchronous and long-lived. There is no deadline before a panel exists.
  Synchronous deadlines start only once a court starts. Spam is priced in MSK, not limited by time.
  *Sources:* **the operator's decision of 2026-09-25, not yet in an ADR** (a divergence entry until
  one exists). Registration exposure, rent and duplicates: 0056 D3/D6, fence
  `palw_certification_rent`.
- [ ] Every row the chain admits can be registered from the CLI and the node, with no tool-side
  narrowing (operator decision T12-030, 2026-09-26). The Spec states the chain's admission and
  nothing narrower.
- [ ] Public model source: a registration names a public source for its artifact (fence
  `palw_public_model_source_required`). *Code:* `core/palw_public_model_source_v1.rs`.
- [ ] Reclamation of dead classes. *Sources:* 0056 D5.

## 3.3 Lifecycle: registered is not eligible

- [ ] The lifecycle is `Candidate → Prefetching → Probation → ActiveLimited → Active`, with `Held` as
  the state that stops a class's own new claims. Past `palw_admission_independence` a row starts at
  `Candidate`, which means it exists but is not eligible. Below that fence it starts at `Registered`.
  Each transition requires verified protocol events: completed claims, and a ready seat the
  registrant does not hold. The lifecycle order is defined by `palw_lifecycle_step_v1`, not by the
  enum's tag order (its borsh discriminants are chain bytes). *Sources:* 0135, 0145 §7, 0144 P7.
  *Code:* `PalwModelLifecycleV1`.
- [ ] **A state changes how much a class may contribute, never what a unit is worth.**
  `admission_permille()` is 50 ‰ in Probation, 100 ‰ in ActiveLimited and 1,000 ‰ in Active. A
  class is promoted after `probation_claims` completed claims. *Sources:* 0144 §2, 0135. *Code:*
  `PalwModelLifecycleV1::admission_permille`.
- [ ] **Independence is drawn, not declared.** The admission jury is drawn, and the audit period is
  fenced. *Sources:* 0147. *Code:* fences `palw_admission_independence` and
  `palw_admission_audit_period_daa`; `core/palw_class_admission_v2.rs`, `core/palw_admission_v2.rs`.
- [ ] From DAA 750 on testnet-12 (`palw_registry_resilience`), a class returns to Probation only when
  probes from **two or more distinct bonds** fail. The claim-side half of this fence is in chapter 07.
  *Sources:* the post-launch audit (V03/V05), the launch note §00. *Code:* `core/palw_model_registry_v1.rs`.
- [ ] The weight gate: only classes certified end to end carry weight. The certified set is genesis ∪
  chain. *Sources:* 0069 as amended by 0075, 0070.

## 3.4 Certification objects

- [ ] Certification is a consensus object (for example `ClassLaneCertified`), scoped to the lane it
  was drilled on. A weightless entrant is seated by an object, not by a regenesis. *Sources:* 0075
  D1–D8, 0073 D6 / 0074 D6 (superseded parts).

## 3.5 Artifact roots

- [ ] An artifact root has one owner on the chain, by an index and not by map order. Competing weights
  stay permissionless. *Sources:* 0143. *Code:* fence `palw_artifact_root_ownership`.
- [ ] The roots in force for a line: the founding root, the current version, previews, and a
  superseded version inside its grace period. *Sources:* 0088 D3 (amends 0056 D6).

## 3.6 The Activation Pool and the verification deadline

- [ ] The Activation Pool: how registered classes queue for, and enter, verification capacity.
  *Sources:* 0152 (the Activation Pool section). *Code:* `core/palw_activation_pool_v1.rs`, fence
  `palw_activation_pool`.
- [ ] The class verification deadline `D(c)`: derived by default, or from a measured row after
  ADR-0153's flag day (reserved, not written). *Sources:* 0152 DL-1 and §4-quater. *Code:*
  `core/palw_class_verify_deadline_v1.rs`, fence `palw_class_verify_deadline`,
  `Params::palw_class_verify_rows`.
- [ ] Conservative classes (testnet-12: the 2M row) and what "conservative" restricts. *Sources:*
  0152 T-2(b). *Code:* `palw_rcore_conservative_classes`, `PALW_T12_RCORE_CONSERVATIVE_CLASSES`.

## 3.7 Held classes and context limits

- [ ] The chain records which classes are held. A held class carries its own prompt form and is
  walked at the regime's ladder. *Sources:* 0118, 0119. Chapter 04 has the execution side.
- [ ] A context limit is activated only from reproducible public vectors. *Sources:* 0110.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis, except `palw_registry_resilience` (DAA 750). Genesis classes and held rows: chapter 16 |

**Design:** `design/palw/registry.md`.
