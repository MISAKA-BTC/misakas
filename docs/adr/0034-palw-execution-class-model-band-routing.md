# ADR-0034: PALW re-verification routing — four execution-class families, five model bands, one deciding binding

Status: **Accepted (architecture; activates nothing).** Devnet / shadow / zero-credit envelope
unchanged. This ADR fixes how re-verification work is **routed** once PALW carries more than one
model and more than one backend: which coarse keys a receipt and a verifier advertise, how the
two sides are matched automatically without a human picking jobs off a list, and — the part that
keeps the rest honest — which key is allowed to *decide* anything. It was circulated in draft
under the number 0028 ("Execution Class / Model Band による PALW 自動再検証ルーティング設計");
that number was already taken by the challenge-sampling protocol, which this ADR builds on
rather than replaces. One element of the draft is **rejected, not adopted** — the
`FINALIZED_WITHOUT_REPLAY` crediting path — see §7, which explains why it cannot coexist with
premises this fork has already accepted and what replaces it.
Date: 2026-08-16
Relates to: ADR-0026 §3/§8 (exact-within-class, per-class signed bundles), ADR-0027 (P1–P3,
one-step adjudication, GPU admission falsifiability), ADR-0028 (credit gate, `select_replay_panel_v1`
assignment, DAA windows, `q` as funded replays, opening-call audits), ADR-0029 (carriage kinds
0x01–0x06, the version-trap rule, mass budgets), ADR-0030 (step space, `PalwShapeProfileV3`,
`PalwStepOpKindV1`), ADR-0031 (transcendental provenance; the Apple-libm admission boundary),
ADR-0032 (fee/bounty mechanics), ADR-0033 (where `credit(C)` is evaluated),
`consensus/core/src/palw_registry.rs` (`PalwClassRegistrationV1` — the object this ADR
generalizes), `consensus/core/src/palw_schedule.rs` (`select_replay_panel_v1`,
`credited_ceiling_tokens_v1`, `PALW_SCHEDULE_REPLAY_KAPPA`), `consensus/core/src/vlt.rs`
(`derive_runtime_class_id`, the class-tag literals), the v2 design §16 (capability declaration).

## Thesis

Routing identity is **three-keyed**, and the keys have strictly ordered authority:

```
ExecutionFamily   (4 values, fixed)     — routes work to hardware that could hold it
ModelBand         (5 values, fixed)     — sizes windows, bonds and capacity; coarse load class
ModelBinding      (exact, unbounded)    — the ONLY key a verdict may reference
```

A family and a band may decide *who gets asked*. Only the exact binding — model × family ×
class version × runtime manifest × quantization, resolving to a registered
`runtime_class_id` lineage — decides *what a replay means*. Two receipts in the same family
and band are not comparable; two receipts under the same binding are exactly comparable, to
the byte (ADR-0026 §3, unmoved). This is what lets model count grow without growing the class
count past four or the band count past five, and without ever weakening the exactness rule.

## Premises carried forward, plus one

P1 (no BFT), P2 (no challenge-randomness dependence), P3 (slash-terminal) from ADR-0027; the
ADR-0028 corollary (**randomness may schedule work; it may never make an unchecked job safe**);
class = conformance to the canonical reference (ADR-0027 §2); class membership is
manifest-hash exact, never a label (ADR-0026 §3). One new premise joins them:

> **A family is a routing index, not a determinism claim.** The measurements that forced the
> class mechanism also refute every coarser identity: Metal ≠ CPU on the same job
> (`ba3b9994…` ≠ `d04672dc…`); arm ≡ x86 only 7/8 seeds (a prefill batch-GEMM near-tie);
> EPYC ≠ Broadwell 4/8 on the registry Q8_0 artifact; fp16-lane attention accumulation and
> repack coverage split aarch64 from x86_64 *structurally* (ADR-0030 Facts 13–14). "CUDA" or
> "CPU" is where a job *could* run, never a set whose members agree. Any rule that compares
> two roots because their families match repeats the golden-gate label bug (`f9ab6ab`) at
> protocol scale.

## Decision

### 1. The three keys, and what each is allowed to touch

```rust
enum PalwExecutionFamilyV1 {          // new; frozen at four — no fifth family
    Metal = 1,
    Cuda  = 2,
    Rocm  = 3,
    Cpu   = 4,
}

enum PalwModelBandV1 {                // new; frozen at five — no B5
    B0 = 0, B1 = 1, B2 = 2, B3 = 3, B4 = 4,
}
```

| key | may influence | may never influence |
| --- | --- | --- |
| family + family version | candidate discovery, capability indexing, coverage accounting, UI grouping | any verdict, any comparison of roots, any slash |
| band | window sizing, bond floors, capacity/admission caps, audit-rate scaling, reward scaling | verifier selection **alone** (§6), any verdict |
| binding | everything: panel eligibility, replay comparison, refutation, adjudication | — |

The family is not new state so much as recognition of structure the class tags already carry:
`misaka-palw-lite-cpu/x86_64/v1` is *family segment / arch segment / version segment*, and
`derive_runtime_class_id` hashes the whole tag. This ADR makes the first and last segments
machine-readable (registry fields, §3) without touching how `runtime_class_id` is derived —
existing class ids do not move. New acceleration hardware must be shown to fit an existing
family before any proposal to add one; the draft's rule stands: **no fifth family, no B5.**

A **family version** (the draft's `class_version`) names a coordinated runtime generation
inside a family — the `/v1` tag segment plus a `PalwExecutionFamilyManifestV1` enumerating
which exact runtime manifests that generation admits (worker binary hash lineage, kernel
bundle, deterministic flags, golden-set root, activation/retirement epochs — the per-class
signed-bundle discipline of ADR-0026 §8, indexed). At most **two versions per family** are
active at once (`current`, `previous`); retirement follows the profile rule — profiles are
never overwritten (ADR-0027 §5).

**The CPU family is not the adjudicator.** The CPU family is for miners/re-executors that run
LLMs on CPUs under a pinned deterministic build (today's `CPU_BUILD_PROFILE`:
`no-blas/no-openmp/single-variant/…` — general BLAS is already excluded by construction, per
the draft's requirement). Full-node one-step adjudication runs in canonical reference
arithmetic (`palw_reference.rs` ruleset v2 + the Berkeley-SoftFloat second implementation in
`misaka-palw-reference2`) — a separate thing from any family, needing no family membership
(ADR-0027 §2).

### 2. Draft vocabulary → this fork's identifiers

The draft named objects the tree does not; this table is normative for implementation so the
ADR and the code cannot drift apart. "new" = to be landed, consensus-inert first, like
everything before it.

| draft name | here |
| --- | --- |
| `execution_class` | `PalwExecutionFamilyV1` (new) — "family", to avoid colliding with `runtime_class_id`, which stays the exact-class identity |
| `class_version` | family version: the tag's `/vN` segment + `PalwExecutionFamilyManifestV1` (new) |
| `ExecutionClassManifestV1` | `PalwExecutionFamilyManifestV1` (new); per-class facts stay in `RuntimeManifestV2` and the registration row |
| `model_id` / `model_root` | `model_profile_id` (already in every v2 envelope/context); `ModelDefinitionV1` (new) gives it a registered preimage — today it is an opaque input |
| `model_binding_id` | `binding_id` = `registration_id()` of a registration row (§3): **a binding IS a registration row**, extended with model/family/band fields |
| `model_band` | `PalwModelBandV1` (new), binding-scoped |
| `PalwReceiptV3` | the Stage-1 successor of carriage kind 0x01 (`PalwCommitmentCarriageV1`) carrying the routing keys — a **new kind/id** per ADR-0029's version-trap rule, never a retrofit |
| `VerifierCapabilityV1` | `PalwVerifierCapabilityV1` (new): the binding-aware successor of the v2 design §16 capability declaration (node-side handle today: `PalwComputeCapability`) |
| Receipt Verifier (role) | not a party: stateless carriage validation (`validate_palw_carriage_v1`) + the registry equality checks of §5 |
| Bonded Re-executor | assigned panel member (`select_replay_panel_v1` lineage) + ADR-0028 §4 attester |
| Challenger | ADR-0027 §1 refuter (re-execute → name the first divergence) |
| `ReplayAttestationV1` | `PalwExecutionAttestationV1` (landed, `palw_slash.rs`) + its carriage (kind 0x02) |
| adjudication primitives (`LOAD_FRAGMENT`, `MUL_INT`, …) | the landed step taxonomy: `PalwStepOpKindV1` (17 kinds, `EmbedLookup … CpyF32F16`) resolved to kernel programs by `kernel_semantics_id` (`resolve_kernel`), executed by `check_execution_step_refutation_v1` |
| `palw_reference.rs` "one primitive" | one **step tile** under ruleset v2 — already the landed unit |

### 3. The registry: definitions, bindings, and the row that already exists

Model identity and execution identity register separately and join in a binding:

```rust
struct ModelDefinitionV1 {            // new
    model_profile_id: Hash64,         // the id every envelope already carries, now with a preimage
    gguf_sha256: [u8; 32],            // artifact identity (the qwen35_pins discipline, generalized)
    gguf_size: u64,
    tokenizer_id: Hash64,             // tokenizer_id_v2_for_gguf lineage — pinned outside the runtime
    architecture_id: Hash64,
    total_parameter_count: u64,
    active_parameter_count: u64,      // MoE honesty: totals alone misclassify (draft §7.1)
    publisher_signature: Vec<u8>,
}
```

A **binding** is `PalwClassRegistrationV1` (landed, consensus-inert, `palw_registry.rs`)
extended with the routing keys — the registration row already co-locates everything a binding
means (class id, manifest hash, model id, shape profile, commitment form, adjudication depth,
replay-cost measurement, credited ceiling, windows). Added fields:

```
class_tag: String                            // the SAME string runtime_class_id hashes —
                                             //   validate() recomputes the id from it and
                                             //   machine-reads family/version out of it
                                             //   (routing_keys_for_class_tag_v1, fail-closed
                                             //   on unrecognized tag shapes), so neither key
                                             //   below is ever self-declared
execution_family: PalwExecutionFamilyV1      // checked against the tag, never believed
family_version: u16                          // checked against the tag, never believed
model_band: PalwModelBandV1                  // derived, §4 — validate() rejects a declared band
                                             //   that does not equal the derivation (the
                                             //   CeilingNotDerived pattern, applied to bands)
quantization_id: Hash64
model_artifact_bytes: u64                    // must equal the signed ModelDefinitionV1.gguf_size
                                             //   at activation (binding_matches_definition_v1) —
                                             //   the one band dimension the artifact digest
                                             //   mechanically pins is not left self-declared
peak_memory_bytes: u64
max_proof_material_bytes: u64
```

The wall-clock replay deadline is an **accessor** (`replay_deadline_secs(block_time_ms)`),
not a stored field: a stored copy is uninterpretable without also storing the block time it
assumed, and a network re-parameterizing its block rate (this branch's own 1 s-block work)
would invalidate every drafted registration. The registration's `version` field moves to
**layout generation 2** with this extension, so any stray generation-1 bytes fail
`UnsupportedVersion` instead of misdecoding at the insertion point.

`binding_id = registration_id()` over the extended preimage. The same model under Metal, CUDA,
ROCm and CPU is four bindings; a model with no CPU binding registered simply has none — the
family count does not move (draft §6.2, adopted verbatim). Re-banding an existing binding is
prohibited: a re-classification is a **new binding id with its own activation epoch**
(draft §21.9 — the same rule the registry already enforces for every identity field: ids move,
edits do not exist).

Today's tree is row 1: `Qwen3.5-2B-Q4_K_M` (`aaf42c8b…`) × Cpu family ×
`misaka-palw-lite-cpu/x86_64/v1` — and row 2, its aarch64-dotprod sibling; the Metal tags are
candidate rows pending their own registration measurements. Nothing about multi-model routing
may break while the registry holds exactly these.

### 4. Model band: derived from the binding, and capped by the pruning horizon

A band is a property of a **binding**, never of a model name (the draft's CUDA-Q4 vs CPU-Q8
example is exactly right), and never self-declared (§21.1). The v1 derivation, adopted from
the draft with its inputs grounded in landed measurement types:

```
S_artifact = model_artifact_bytes / 4 GiB
S_memory   = peak_memory_bytes / 8 GiB
S_work     = max_replay_work_units / BASE_REPLAY_WORK_UNITS
S_proof    = max_proof_material_bytes / 64 MiB
resource_score = max(S_artifact, S_memory, S_work, S_proof)

band: ≤1 → B0   ≤2 → B1   ≤4 → B2   ≤8 → B3   ≤16 → B4   >16 → not registrable in v1
```

`max_replay_work_units` is derived from the binding's own registered replay-cost measurement
at its derived ceiling (`replay_work_ms_v1`) — measurement-derived, never miner-declared; the
active-parameter-based derivation is a future refinement, and no doc may claim it is wired
before it is. `BASE_REPLAY_WORK_UNITS` is a **frozen snapshot** of row 1's measured
full-replay cost at v1-definition time (679 975 ms), so B0 means "costs about what the
reference binding cost when the derivation was defined." A later fleet re-bench does NOT move
the constant — every registered band re-derives against it, so an edit would re-band every
binding at once (re-banding-by-constant); a base change is a new derivation version, never an
edit. Every coefficient above is a **placeholder until devnet measurement fixes it** —
shipping a placeholder as measured is the §15-class violation it always is. One consequence
is stated rather than hidden: the work dimension is window-relative (a class registering
tighter windows both works less and credits less), which the economic simulation gate must
price — the artifact/memory/proof dimensions are the window-independent floor.

What a band does, concretely:

* **Windows scale with the band.** `w_replay ≥ κ · p99_cold_replay(binding, ceiling)` with
  `κ = PALW_SCHEDULE_REPLAY_KAPPA = 3` — a B4 replay deadline is long because its p99 is long
  (draft §22.2, now with the landed inequality enforcing it: `validate()` already rejects
  `ReplayDoesNotFit`).
* **And therefore the pruning horizon caps the band.** ADR-0028 §3's chain of inequalities
  (`W_challenge ≥ W_replay + L_bisect · W_round + margin`, `finality < W_challenge`,
  `W_challenge + slack < pruning horizon` — 30 h / 38 h on the two parameter sets) does not
  bend for a big model. A binding whose measured p99 pushes its window set past the horizon
  **cannot be registered as creditable** at any declared ceiling — the existing
  `credited_ceiling_tokens_v1` / `ReplayDoesNotFit` machinery generalizes per band. Roughly:
  B0–B1 fit today's horizons trivially (fleet p99 37–91 s at D=512); B3–B4 fit **only if**
  the checkpoint interval registered with the binding makes rung responses interval-priced
  rather than full-replay-priced, and possibly not at all without a longer horizon or
  pruning-surviving carriage. That is a parameter decision taken explicitly at registration —
  never a silent stretch, exactly ADR-0028's rule. A binding that cannot credit may still run
  SHADOW (uncredited) for measurement.
* **Bond floors and admission caps index by band** (§8; the parameter table), including the physical cap
  `R_jobs · q ≤ Σ capacity(p99)` from ADR-0028 §4e, now computed per binding.
* **Initial family restriction:** `CPU max active band = B1` (row 1 is B0); every other
  family starts at B0. The cap is a **registered fact** — a `max_active_band` field on the
  family-version manifest (§1), seeded from `initial_family_max_active_band_v1` — so raising
  it is publishing a new manifest record once the independent-re-executor count and measured
  deadlines the draft demands exist, never a code edit two observers could disagree across
  mid-rollout; among a generation's records the most restrictive published cap wins.

The draft's illustrative table (B0 ≈ 1–3B … B4 ≈ 70B-class / large MoE) is kept as
explanation, not as normative input.

### 5. Receipts carry the keys; the registry, not the miner, gives them meaning

The Stage-1 commitment body (kind 0x01's successor, new id) grows exactly three routing
fields — `binding_id`, `execution_family`, `model_band` — beside what it already carries
(envelope, committed form, committed root, legs binding, identity, signature). Acceptance
checks, in the landed vocabulary (the draft's §8.2, made mechanical):

```
stateless:  validate_palw_carriage_v1 lineage — decode, caps, signature (ML-DSA-87 carriage
            context), committed-root recomputation for composite forms (recomputed, never
            trusted — ADR-0029's rule)
registry:   binding_id is ACTIVE (not LOW_COVERAGE-frozen states of §10, not retired)
            envelope.model_profile_id      == binding.model_profile_id
            envelope.runtime_manifest_hash == binding.runtime_manifest_hash
            envelope.runtime_class_id      == binding.runtime_class_id
            envelope.shape_profile_id      == binding.shape_profile.shape_profile_id()
            envelope.trace_scheme_id / cu_ruleset_id == the registered ones
            carried execution_family        == binding.execution_family
            carried model_band              == binding.model_band      ← band forgery = invalid
            exact_decode_tokens ≤ binding.credited_ceiling_tokens (crediting path)
                                 ≤ PALW_V2 format ceilings (always)
dedup:      first-accepted-wins on committed_root (ADR-0029 §2)
```

A receipt whose declared band or family disagrees with the registry row is rejected as
malformed — the miner's claim is checked against `registry[binding_id]`, never believed
(draft §8.2's rule, unchanged). No re-execution happens at acceptance; this layer is cheap by
construction and runs on every node.

### 6. Verifier capability: two layers, and an agent that registers itself

Capability is **two claims with different verification costs**, and both are required
(draft §9.1 — band-only assignment ships jobs to hosts that do not hold the model):

* **Hardware capability** — family, family version, `max_model_band`, memory, concurrency,
  bond account, availability TTL. Cheap to declare, cheap to index.
* **Ready bindings** — the set of `binding_id`s this verifier can replay *now*: artifact held
  (exact digest), runtime boots, **golden set passed** (the landed `v2-selftest` discipline —
  a ready claim without a golden pass is not a claim), replay benchmark measured (the landed
  `v2-replay-bench` path). Committed as `ready_binding_root` (Merkle); duty eligibility and
  any claim carry `binding_id + proof` against it.

```rust
struct PalwVerifierCapabilityV1 {     // new; successor of the v2 design §16 declaration
    verifier_id: Hash64,
    execution_family: PalwExecutionFamilyV1,
    family_version: u16,
    max_model_band: PalwModelBandV1,
    ready_binding_root: Hash64,
    max_concurrency: u16,
    available_slots: u16,
    max_accepted_replay_secs: u32,
    minimum_reward: u64,
    replay_bond_outpoint: TransactionOutpoint,   // the bond-UTXO discipline, ADR-0016 lineage
    available_bond: u64,
    availability_expiry_daa: u64,
    capability_nonce: u64,
    signature: Vec<u8>,
}
```

The **`misaka-palw-reexecutor` agent** (the operational successor of `misaka-palw-agent`
Phase A plus `palw-shadow attest`) automates the draft's §9.3 sequence: detect backend →
resolve family/version → scan local artifacts against active bindings → verify memory fit →
run goldens → run the replay benchmark → emit the ready set → derive `max_model_band` →
submit the capability → heartbeat. No human enumerates models. Operator policy stays local
(allow/deny model globs, auto-download budget, max band, concurrency, minimum profit,
power/schedule caps — the draft's §9.4 TOML shape is adopted as the agent's config surface).
A capability is a signed, TTL'd, nonce-monotonic statement; an expired capability is simply
not eligible — silence is never an offense at this layer.

### 7. What routing may decide — and the draft state that is rejected

**Adopted:** every accepted receipt on the crediting path gets an **assigned, funded panel**,
and assignment is the deterministic ticket lottery of ADR-0028 §2 with its eligibility
predicate extended by the two new layers:

```
eligible(v, C) ⟺ v.execution_family == binding(C).execution_family
              ∧ v.family_version    == binding(C).family_version
              ∧ v.max_model_band    ≥  binding(C).model_band          // routing precondition,
              ∧ ready(v, binding_id(C))   // Merkle proof against ready_binding_root — the
                                          //   part band can never substitute for
              ∧ v.available_slots > 0
              ∧ v.available_bond ≥ required_replay_bond(band)
              ∧ v.availability_expiry_daa > now
              ∧ reputation(v) ≥ floor
              ∧ ¬same_control_domain(v, executor(C))    // self-verification excluded, operator
                                                        //   aggregation counted once (v0.1)
              ∧ ¬frozen(v)

panel(C) = q lowest tickets over eligible(·, C)         // domain-keyed, executor-excluded,
                                                        //   anchor(C) at daa(C)+Δ_bind — the
                                                        //   select_replay_panel_v1 lineage
```

Three panel rules the twins did not need, because their candidates come from one
consensus-registered validator set while routed candidates are **self-published capability
records**: (1) the ticket additionally binds `binding_id` and the escalation round — a
commitment root reused across two bindings, or a re-draw at the same anchor, must not produce
correlated draws; (2) a verifier id appearing more than once in the candidate set is dropped
entirely (the caller failed nonce supersession; choosing between two conflicting records
would make the panel input-order-dependent — fail-closed); (3) **one seat per control
domain** among distinct verifiers, lowest ticket holding the seat — the §10 counts-once rule
applied to seats, or one operator's sibling identities would collapse the funded redundancy
of `q` to a single machine.

Band alone never selects (`Receipt = B4, Verifier max = B4 → assign` remains the named
anti-pattern); the full conjunction is the assignment rule (draft §12.2, adopted).
`q ≥ 2` on the crediting path (ADR-0028 §4 — the draft's `required_independent_replays = 1`
default is corrected to the accepted value; one replay is a liveness single-point-of-failure).

**Escalation replaces monopoly-claiming.** The draft's Round 0/1/2 ladder maps onto the
assignment model rather than a race: Round 0 *is* the panel (a funded duty, not an exclusive
right); if `w_replay` passes without the required on-time attestations, the panel is
**re-drawn wider** at the escalation anchor (original no-shows keep their objective offense),
and a further lapse raises the reward (`ReplayBountyEscalated`). Bonds are an eligibility
floor with a capped influence on selection weight — stake never buys the panel (draft §12.4,
adopted). The refutation window stays permissionless throughout: anyone who replays and
diverges may refute regardless of panel membership (ADR-0028 §4d — rivalry, not altruism).

**Rejected: `FINALIZED_WITHOUT_REPLAY`.** The draft finalizes receipts that lose a
band-thresholded lottery (`selection_hash < replay_threshold[model_band]`) without any
replay. Under P2 that is precisely the dependence this fork refused twice already: the moment
the draw is predictable or grindable, *the unchecked set is the attacker's choice* — and the
ADR-0028 corollary is explicit that no rule of the form "not drawn, therefore creditable
unchecked" may exist. It does not return here. `credit(C)` stays ADR-0033's predicate,
verbatim: window closed ∧ ≥ 1 assigned root-equal attestation ∧ no accepted refutation; zero
attestations ⇒ credit 0, never "shrink the panel and mint." The verification tax is the
`(1 + q·ρ_v)` issuance split, visible in emission — not a cost the lottery quietly deletes.

**What survives of the draft's randomness and risk scoring** (both decide nothing):

* the PRF-selected **opening-call audit** stream (ADR-0028 §5, landed seam) — answerability
  and DA sampling, priced per ADR-0032; its *rate* may scale with band and risk;
* **risk-based escalation of `q` and audit rate** — new miner, new binding, new family
  version, anomalous timing, mismatch history, coverage thinness raise the *redundancy* and
  *audit frequency* purchased for a job (the v0.1 §9.2 dynamic inputs, unchanged meaning).
  Risk raises how much checking is funded; it never selects who is guilty and never waives a
  replay on the crediting path.

### 8. Bonds: two escalating locks, on the rails that already exist

The draft's two-bond structure is adopted and lands on ADR-0032's mechanics — no new covenant
machinery:

* **Replay bond** — the panel-duty lock: claim-hoarding, timeout and false-completion
  pricing. Sized `max(band_min_replay_bond[band], reward × multiplier, externality)`.
  No-show slashes per the amended floor `min(100 · ρ_v · base, bond)` (the B15 finding —
  uncollectible floors are lies).
* **Challenge bond** — the refutation lock, strictly larger; it prices false-slash attempts
  and adjudication DoS. **No vote can confiscate it** — under P1 the only thing that may take
  a challenge bond is losing the deterministic one-step check
  (`check_execution_step_refutation_v1` → `NoFaultFound`), and the only thing that may take a
  miner's bond is losing it (`ComputationMismatch`). `Unadjudicable` (outside the kernel
  catalog / opaque decoder) slashes **nobody** — the landed three-way verdict is load-bearing
  here.

Settlement flows are ADR-0032's: match ⇒ bond back + `ρ_v · base` fee + reputation; timeout ⇒
partial/total replay-bond forfeit; correct challenge ⇒ bond back + bounty
(`min(10 % · slashed, B_cap)` to the refutation carrier's `(tx_id, 0)` slot, remainder burns,
`slash_id`-idempotent); false challenge ⇒ challenge bond split burn / treasury / injured-miner
compensation. DA failure is the miner's offense, never the assignee's
(`DATA_WITHHOLDING` — draft §21.10, already the accepted rule).

### 9. Adjudication is band-independent, bounded, and per-binding in depth

The full node holds no model, no GPU, no family membership (draft §2.8–2.9 — already the
architecture). The hard bounds that make "one disputed primitive" true at any model size are
the **landed constants**, restated as the invariant the draft asked for:

```
one step  = one output tile, recomputed from openings under ruleset v2
            (palw_reference.rs + the SoftFloat-3e second implementation, 12.5 M-case differential)
opening   ≤ 22 siblings (PALW_STEP_LEG_MAX_OPENING_SIBLINGS); tile ≤ 256 KiB
            (PALW_STEP_LEG_MAX_TILE_BYTES); state chunk ≤ 1 MiB
carriage  ≤ 16 openings per call; evidence chunks ≤ 340 000 B × ≤ 4; every measured
            worst-case step opening fits one 480 000-mass standard tx (ADR-0030 §3)
bisection ≤ 48 rounds over a space ≤ 2^40 (PALW_BISECT_MAX_ROUNDS / _MAX_SPACE)
reference ops per check ≈ 10⁵–10⁷ (dot ≤ 2^20, GEMM dim ≤ 4096)
```

Growing the model to 70B / 200B / 500B grows the *number* of steps (and the band), never the
size of one adjudication — these constants do not scale with the band, by rule. The draft's
latency targets are adopted as engineering targets for the one-step check: p50 < 1 s,
p95 < 5 s, p99 < 20 s.

The draft's primitive list (`LOAD_FRAGMENT`, `MUL_INT`, …) is superseded by the landed step
taxonomy: `PalwStepOpKindV1`'s 17 kinds, resolved per node to `kernel_semantics_id` programs
(the transcription catalog: L2Norm, RmsNormFused, Swiglu, sigmoid/softplus over glibc-2.39
programs, GdnCore in both dot structures — growing by transcription + differential
validation, never by edit).

**Adjudication depth is a binding fact, surfaced by routing.** `PalwAdjudicationDepthV1`
already encodes it: `ArithmeticCatalogued` bindings expose the full refutation ladder;
`StructuralOnly` bindings (today: every Apple-libm-dependent class — ADR-0031's honest
admission boundary) expose structural faults, contradiction certificates and DA offenses, but
not arithmetic conviction. Capability listings, coverage accounting and the explorer MUST
display depth per binding; a Stage-2 (slash-bearing) binding requires `ArithmeticCatalogued`
plus the registry's existing `stage2_eligible` gates. CUDA and ROCm are **reserved families
with no code paths today** (the tree's only CUDA reference is the literal `cuda-off`);
admitting the first binding in either is the ADR-0027 falsifiable campaign — deterministic
kernels, pinned reductions, reference conformance — and some hardware is expected to fail it.

### 10. Coverage: a binding nobody can replay does not get to exist quietly

Binding activation requires (draft §16.1, with the accepted thresholds):

```
golden set passed on ≥ the minimum independent re-executors
artifact retrievable under the DA rules            independent ready re-executors:
family version active IN THE MANIFEST SET              devnet ≥ 3, mainnet ≥ 5
binding's runtime manifest ∈ the generation's      (same control domain counts once)
  admitted lineage                                 adjudication depth recorded
declared band ≤ the generation's registered cap    signed ModelDefinition joins the row
windows/ceiling validate() against the network       (model_profile_id + artifact size)
family not reserved (Cuda/Rocm never activate in v1 — their first binding is the
  ADR-0027 conformance campaign, a different act than activation)
```

The generation facts are derived from the manifest records inside
`binding_may_activate_v1` — not accepted as caller booleans — so two observers of one
registry state cannot disagree about what activated.

Live coverage walks the ladder — each transition an on-chain-observable fact, none a
governance vote:

```
ACTIVE → LOW_COVERAGE (ready count < threshold):  replay reward up, audit rate up, warning
       → THROTTLED:   new-receipt admission rate-limited; high-value jobs refused
       → FROZEN:      no new receipts; existing disputes run to completion
DEPRECATED → RETIRED: planned retirement; receipts against retired bindings rejected
CONTRADICTION_FREEZE: ClassContradictionCertificateV1 — two same-class signed differing
                      roots ⇒ freeze, bonds frozen not released, no slash (ADR-0027 §5);
                      registry-side this is to_zero_credit(): the ceiling IS the switch
```

Two orderings are normative: **starvation outranks deprecation** (a deprecated binding still
accepts receipts, so zero ready re-executors — or a zero threshold, which is no threshold —
freezes it regardless of the planned retirement), and the `epochs_below_min` streak is
counted by ONE published rule (`next_epochs_below_min_v1`: reset at-or-above threshold,
saturating increment below), because a privately invented counter would let two observers
classify the same chain history into different coverage states.

"検証者が存在しないモデルを登録したまま、祈りで運用することは認めない" — the draft's sentence is
the rule: a binding without its minimum independent ready re-executors never activates, and
one that loses them stops accepting work *before* the panel machinery would start assigning
duties nobody can serve (an empty eligible set already means credit 0 — never shrink-and-mint).

### 11. Same-binding decides; cross-anything diagnoses

Unchanged, restated once against the new keys: slashing-grade comparison exists **only inside
one binding** (same `binding_id`, hence same exact class). Cross-binding, cross-family,
cross-band comparisons — including CUDA-vs-Metal on the same model — are the diagnostic tier:
drift alarms, golden re-runs, contradiction investigation, freeze inputs. They never slash,
never mint, never gate a block (ADR-0026 §3's carve-out; ADR-0027 made it structural). A
tolerant comparator anywhere in the binding path remains a new-scheme-version event.

### 12. `llm.misakascan.com` is a window, never an oracle

The explorer/market UI shows what the chain and the capability set already say — receipts
with their three keys and depth, candidates with rewards/bonds/deadlines/eligible-count,
verifier dashboards, coverage states. It schedules nothing and adjudicates nothing; every
listed action is reachable agent-side from chain RPC alone, so the site's death changes
liveness not at all (draft §19, adopted; ADR-0029's watcher/reporter is the data path).
Keys and model execution live in the local agent, never the browser.

## Receipt lifecycle (the draft's §17, corrected)

```
SUBMITTED → stateless+registry checks → ACCEPTED (panel derivable at daa(C)+Δ_bind)
  ATTESTED (≥1 on-time assigned root-equal)  → W_challenge close → CREDITED
  no on-time attestation (escalations spent) → W_challenge close → NOT_CREDITED (credit 0)
  MISMATCH found by anyone → CHALLENGE_OPEN → direct refutation
                                            → (DA-degraded: BISECTION, ≤48 rungs)
                           → ONE_STEP_ADJUDICATION
                                → ComputationMismatch  → MINER_FAULT   → miner penalty
                                → NoFaultFound         → CHALLENGER_FAULT → challenge bond slash
                                → Unadjudicable        → nobody slashed; catalog gap recorded
  refutation accepted after close: convicts, does not un-credit (ADR-0033 §3's tail metric)
```

`FINALIZED_WITHOUT_REPLAY` does not appear; there is no path from ACCEPTED to credit that
bypasses an attested replay.

## Stage mapping (the draft's §24 onto the accepted ladder)

| draft stage | lands as | note |
| --- | --- | --- |
| 0 Shadow Capability | ladder Stage 0 (ADR-0028 §6) + capability/matching shadow | family detect, band derivation, ready sets, capability tx, matching *logs* — joins the existing drill (`palw-shadow` submit/attest/watch/report); no value moves |
| 1 Non-slashing Replay Market | **rejected as a network stage** | consensus rewards without slash break §4b (attestation = liability): rubber-stamping would be strictly dominant. Market mechanics soak in the drill namespace (drill `network_id`, operator-side accounting) instead |
| 2 Bonded Replay | ladder Stage 1 → 2 preconditions | replay bonds, timeout offenses, reputation — objective offenses first (Stage 1), credit only at Stage 2 |
| 3 Challenge Game | ladder Stage 2 | refutation slashes WorkBond; requires `ArithmeticCatalogued` + `stage2_eligible` + the §4e leverage remedy |
| 4 Production Slashing | ladder Stage 3 | full economics, coverage enforcement, wider exposure; second reference implementation gate |

## Initial parameters (every number a placeholder; simulation- or registration-gated)

```
families = 4 (frozen)          bands = 5 (frozen)         active versions per family ≤ 2
CPU max active band = B1       q = 2 (crediting path)     κ = 3 (landed)
independent ready re-executors: devnet 3 / mainnet 5
capability TTL = 30 min        heartbeat = 5 min
band thresholds 1/2/4/8/16     BASE_REPLAY_WORK_UNITS = row-1 measured replay
replay/challenge bond floors, F_call, B_cap, multipliers: economic-simulation gate (B15
  discipline; the ADR-0028 §4e leverage remedy MUST be encoded before any Stage-2 credit)
```

## Required tests (the draft's §25, in this vocabulary — plus the routing red team)

The landed adversarial suite (`palw_adversarial.rs`, 10 attacks) gains routing attacks; each
line below is a test obligation, most against pure functions:

```
B4 receipt never panels a max-B3 verifier          family mismatch never panels (Cuda vs Metal)
family-version mismatch refused                    band match without ready proof refused
forged ready-binding Merkle proof refused          executor/self/same-domain excluded
expired capability ineligible                      bond-short capability ineligible
declared band ≠ registry band → receipt invalid    retired/frozen binding → receipt invalid
DA-unavailable ⇒ assignee unpunished, miner's offense
cross-family mismatch never slashes (diagnostic only)
false challenge slashes only after NoFaultFound    Unadjudicable slashes nobody
adjudication needs no model artifact               coverage drop walks ACTIVE→…→FROZEN
band re-derivation from registry fields is exact (no declared-band trust anywhere)
no path credits a receipt with zero attestations (FINALIZED_WITHOUT_REPLAY is untypable)
```

## What this ADR deliberately does not decide

* **Numbers.** Band coefficients, thresholds per band for bonds/rewards/audit rates, fee and
  bounty magnitudes — the economic-simulation gate owns them all.
* **CUDA/ROCm admission.** Reserved families stay empty until a backend passes the ADR-0027
  conformance campaign; per-generation class granularity inside those families is a
  measurement question, not a naming one.
* **RPC surface.** No PALW RPC exists today (the drill consumes generic block/acceptance
  RPC); the draft's `palw_*` method list and event names are the Stage-1 design input, to be
  fixed alongside the Stage-1 carriage release — events as store-derived notifications, not
  new consensus facts.
* **DA-layer artifact distribution** (auto-download of model artifacts to re-executors) —
  agent policy today; a protocol concern only if coverage economics prove it must be.
* **Mainnet.** Separate ADR after Stage 3, as always.

## Consequences

* **New objects to land, consensus-inert first:** `PalwExecutionFamilyV1` / `PalwModelBandV1`
  / `ModelDefinitionV1` / `PalwExecutionFamilyManifestV1` (carrying `max_active_band`) / the
  registration-row extension (`class_tag`-verified keys, band derivation, layout generation 2)
  in `validate()` / `PalwVerifierCapabilityV1` (+ carriage kind for capabilities at Stage 1)
  / the eligibility extension beside `select_replay_panel_v1` (new domain key — the
  assignment twin discipline — with the seat-dedup rules of §7) / the coverage state walk
  and its counting rule / the store set-rules (`binding_rows_coherent_v1`: no two rows share
  a `runtime_class_id`, or a receipt key-validates under one row and credits under the
  other's windows; `binding_matches_definition_v1`) / the `misaka-palw-reexecutor` agent
  surface. Each with goldens and domain-uniqueness tests against every existing PALW family
  and the VLT sortition key, like everything before it.
* **ADR-0028 §2 is amended** by §7: panel eligibility becomes binding-aware (ready proof
  required), and the escalation re-draw is added for panel lapse. Its §4 `q ≥ 2`, funding
  split, no-show pricing and permissionless window are unchanged.
* **ADR-0029's Stage-1 commitment kind** grows the three routing fields as a new body/id
  (version-trap rule); the capability object gets its own kind. Mass estimates must be
  re-run for both.
* **`PalwClassRegistrationV1` becomes the binding row** — the reader ADR-0033 promised gains
  a second consumer (routing) before its first (the credit gate) activates; both read the
  same rows, so the registry stays the single place where "what may this class do" lives.
* **The routing layer inherits the registry's honesty guarantees**: bands derived not
  declared, ceilings derived not declared, depth recorded not implied — the three lies
  `validate()` already refuses, now load-bearing for matchmaking too.
* The draft (Japanese, circulated as "ADR-0028 v0.1") is superseded by this document; its
  section numbers are preserved in the cross-references above where the mapping is not
  obvious.
