# PALW-TIR Phase F — consensus integration design (Gate F1)

> **Design record, Gate F1 (design only; nothing here is implemented).** How PALW-TIR v1 classes
> become real on chain behind the dormant fence `palw_tir_v1` (RFC-0002 Phase F), and the
> independent Phase 0(a) fix for the live GDN `k_heads ≠ v_heads` defect. Inputs: RFC-0002
> (`docs/rfc/0002-palw-tensor-ir.md`, branch `rfc/0002-tensor-ir`), spec
> [04b](../../../spec/palw/04b-tensor-ir.md) (Gate 1), [corpus-v1](corpus-v1.md), and the
> `misaka-palw-tir` crate at `b7492d601`. The integration target is `rcore/int-6` (the live
> testnet-12 line); every `file:line` below is read at `tir/phase-f` = int-6 + tir/core Gate 1.
>
> Decisions already taken by the lead and applied here: PALW-TIR-33 (a committed operand outside its
> proven interval is a malformed commitment and the producer loses); the IR artifact stores typed
> param tensors (A16 `m`, `s`, `z` separately); per-position decode semantics; Fixed state committed
> at checkpoints, not per position.

Contents: §0 decisions at a glance · §1 integration map · §2 designs · §3 Phase 0(a) · §4 work plan
· §5 decisions needed · Appendix A object sketches.

---

## 0. Decisions at a glance

| # | Question | Recommendation | Why, in one line |
| --- | --- | --- | --- |
| D1 | What `palw_tir_v1` is | `Option<PalwTirFenceV1 { activation, prim_set_id, court_version, ceilings }>`, Some-only in both fingerprints, whole-option `never()` collapse, activation-only in `for_each_fence` | the `palw_heartbeat` shape (a fence with a companion value) — the only one that keeps every preset byte-identical and still fingerprints the value where armed |
| D2 | "`court_catalog_root` gains `prim_set_id`" | leave the bundle's `court_catalog_root` untouched; the armed fence writes `palw_tir_court_root_v1 = H(court_catalog_root_v1 ‖ prim_set_id ‖ court_version)` into `consensus_params_id` | the bundle root is inside every V2 preset's `palw_ruleset_id_v2`; the IR court is a fenced addition, exactly as `KERNEL_CATALOG_FENCED_V1` is |
| D3 | Program carriage (RFC OQ4) | **inline**, in one lifecycle carrier; network ceiling `max_program_bytes` ≈ 88,000 in v1 (format cap stays 256 KiB); the artifact embeds the same bytes for availability | admission is a consensus check every node must run from chain data; realistic programs are 10–50 KB; no new chunk lane |
| D4 | IR class id | `H(graph_ir_root ‖ H(layout) ‖ artifact_root ‖ tokenizer_id)` under its own domain; `logits_scheme_id` and `prim_set_id` enter through `graph_ir_root` | the RFC's list, plus the commitment layout the step space depends on; nothing node-local exists to commit |
| D5 | Commitment layout | **declared** by the registrant (`PalwTirLayoutV1`: `max_context`, tiles per commit point, `h_tile`, checkpoint interval `C`, state tiles), **verified** by admission, inside the class id | derivation would tie class ids to court ceilings a later fence may move; tiles never price work (PALW-TIR-16) |
| D6 | Admission | `verify_class_admission_v10` = decode_canonical → prim set → `tir_admit_v1` → layout/ladder/close/window checks → class id → weight gate; called from a NEW appended object `ClassRegisteredTirV1` | legacy v9 and `ClassRegistered` stay byte-identical; older builds skip the appended variant (A-2) exactly as the new build drops it before the fence |
| D7 | Step leaves | one tree: commit-point tiles (slots = spec 04b §3.3), **plus Fixed-state checkpoint leaves every `C` positions and Hist tile leaves every `h_tile` positions**, in position order; no separate checkpoint leg for IR classes | the ladder's first-divergent leaf is then always adjudicable from leaves that precede it — checkpoint replay and history anchoring need no second tree and no second court |
| D8 | Court | one appended arm `TirCone`: authenticate → PALW-TIR-33 on every opened committed value → element-demand ("pull") evaluation of the disputed tile's cone → compare | one interpreter of 25 primitives replaces per-kernel arms; demand slicing is what keeps a GDN head or a matmul tile inside 16 Mi MACs |
| D9 | Generic H dissection | root claim = the totals of every H-reduction in the tile's cone; rounds fold by exact sum/max; bottom evaluates the H-local subgraph over one `h_tile` | ADR-0082's protocol stated over PALW-TIR-32, with no attention-specific triple |
| D10 | Work vector | structural classification of nodes (§2.9); normalization = reductions feeding `IntRsqrt`; name heuristics retired for IR classes | G7/PALW-TIR-16; representation-neutral by construction |
| D11 | Node side | SDK `TirLineageV1` + a generic typed evaluator backend (byte-identical to the reference) first; legacy fused engines as fused patterns later (Phase G) | the drill needs a real 1.5B model at usable speed; fused kernels last |
| D12 | Phase 0(a) | the live head mapping is **correct** for the artifact (V heads are tiled at GGUF conversion); the live defect is the state-map conv geometry — fix by profile version 3 + new map names + fence `palw_gdn_key_heads` | §3 |

---

## 1. Integration map

One line per point: what is there, what changes. "Unchanged" means the legacy path stays byte
for byte; IR behaviour is added beside it and reachable only through the fence.

### 1.1 Params, fences, identity

| Where | What | Phase F change |
| --- | --- | --- |
| `consensus/core/src/config/params.rs:961` | `pub struct Params` | add `palw_tir_v1: Option<PalwTirFenceV1>` (and `palw_gdn_key_heads: Option<ForkActivation>` for §3) |
| `params.rs:2806-2817` | `palw_token_lift` — the template (ADR-0102 fenced kernel) | model the doc and every site below on it |
| `params.rs:3726` (genesis gates ≈`5021-5028`) | `validate_palw_v2` | refuse: fence value ≠ this build's `prim_set_id`/`court_version`; ceilings above the format caps; arming without `palw_audit_2026_09_11` declared (A-2 tolerance, §2.2) or without `palw_kary_court` at or below; a genesis IR row unless armed at 0 |
| `params.rs:5556-5566` | `consensus_identity_id` (visitor → normaliser → params id) | unchanged; covered by the two sites below |
| `params.rs:5592` (token-lift arm `5992`) | `normalize_values_a_scheduled_fence_drags_with_it` | collapse the WHOLE `palw_tir_v1` option on `activation == never()` (the D1 rule, as `palw_heartbeat` at `5680`) |
| `params.rs:8374-8383` | `palw_token_lift_fence` / `_active_at` | add `palw_tir_v1_fence()` (ConsensusV2 only) and `palw_tir_v1_active_at(daa)` — the one resolution point |
| `params.rs:8928` (list `≈9301`) | `palw_fences_v1` (exhaustive) | add the entry `"palw_tir_v1"` with the fence's activation — this alone makes it a fork-id gate fence |
| `params.rs:9340`, `9355` (token lift `9527`) | `fence_schedule_v1`, `consensus_schedule_id` | name + height, plus `H(value)` reported (not gated), SA-4 style |
| `params.rs:9881` (token lift `10690`; heartbeat `10184`) | `for_each_fence` (exhaustive) | visit `activation` only, Some-only (`if let Some(f) = palw_tir_v1.as_mut()`) — never the value |
| `params.rs:10980` (token lift `11814`; heartbeat `11595`) | `consensus_params_id` (exhaustive) | Some-only: label `palw_tir_v1/court-v1`, activation, `prim_set_id`, `court_version`, borsh(ceilings), `palw_tir_court_root_v1` |
| `params.rs:≈10960` | `PalwRuleManifestGatesV1` | add `tir_v1` state (RPC manifest only) |
| presets `params.rs:13514,13763,13994,20959` | struct literals | `palw_tir_v1: None` (and `palw_gdn_key_heads: None`) |
| `params.rs:18986-19000` | `PalwPostLaunchFenceV1 { name, set }` | new entries; `set` writes the fence with the t12 v1 value |
| `params.rs:19039, 19388, 19429, 19448` | t12 flag-day lists V1 (750), V2 (1,300), V3 (1,500), `palw_t12_release_v2_params` | `palw_gdn_key_heads` joins the next list after 1,500; `palw_tir_v1` a later list at a height chosen after the Phase F drill (never 750/1,000/1,300/1,500 or any other list's) |
| `consensus/core/src/fork_id_v1.rs:267` | `fork_id_gate_fences_v1` (derived from `palw_fences_v1`) | unchanged — derivation picks the fence up |
| `fork_id_v1.rs:565` (token lift `772`) | `set_fence_for_probe` | add both names (the probe test fails by name otherwise) |
| `consensus/core/src/config/drill.rs:579-615` | `palw_drill_post_launch_fences_*_at_v1` | the list's `--palw-drill-fenceN-at` moves the new entries on a salted drill chain |
| `consensus/src/pipeline/virtual_processor/processor.rs:549-552, 1119, 12360` | processor fence fields and `*_at` accessors | `palw_tir_v1` field from `palw_tir_v1_fence()`; `palw_tir_at(daa)` |

### 1.2 Class admission and registry

| Where | What | Phase F change |
| --- | --- | --- |
| `consensus/core/src/palw_class_admission_v2.rs:2183-2660` | `verify_class_admission_v9` (shape → held gate → id → schemes → fenced kernels `2283-2301` → coverage `2336` → court/geometry → e2e weight `2412` → held walls → ladder `2468-2491` → court cost `2543-2556` → window → canonical count/pwu `2636-2647` → catalog entry `2651`) | unchanged; add `verify_class_admission_v10` (§2.4) in a new module `palw_tir_admission_v1.rs` |
| `palw_class_admission_v2.rs:1126` | `PalwClassAdmissionError` | append `TirNeedsItsFence`, `TirProgram(TirErrorKind)`, `TirPrimSet`, `TirExceeds{limit,value,cap}`, `TirLayout(..)`, `TirClassIdIsNotDerived` |
| `palw_class_admission_v2.rs:293, 1770` | `derive_court_cost_v1`, `reachable_kernels_v1` (profile walks) | IR twins: `tir_court_cost_v1` (from `tir_admit_v1`), `tir_reachable_prims_v1` (§2.7 catalog note) |
| `palw_class_admission_v2.rs:530, 1759` | genesis predicates (`palw_genesis_reaches_fenced_kernel_v1` …) | add `palw_genesis_registers_tir_class_v1` for the genesis gate |
| `consensus/core/src/palw_model_registry_v1.rs:29-37` | `PalwModelManifestV1.graph_ir_root` (placeholder) | becomes the IR class's `graph_ir_root`; `runtime_version` = the court version |
| `palw_model_registry_v1.rs:140-152, 770-798` | `palw_manifest_verdict_v1`; `palw_model_work_from_carriage_v1` with `ops_supported: verification_ccu > 0` (`:792`) | IR: `tir_model_work_v1(program, layout, canonical)`; "unsupported op" is a decode refusal (unknown prim tag → `NEEDS_PRIMITIVE`), never a proxy |
| `consensus/core/src/palw_model_registration_v1.rs:322-362` | processor-same preflight (SDK's gate) | IR twin calling v10 |
| `consensus/core/src/palw_registry.rs` | ADR-0026…0033 class registration object (measured windows) | unchanged |
| `consensus/core/src/palw_activation_pool_v1.rs` | listing rules (R1/R2/P2, pool) | unchanged — class-agnostic; IR classes list, audit and activate like any bought class |
| `consensus/core/src/palw_catalog_coverage.rs:181` | `palw_court_catalog_root_v1` (identity, from `catalogued_kernel_ids_v1`) | unchanged (D2) |
| `palw_catalog_coverage.rs:≈205` | `catalog_covered_kernels_v1` (strips fenced ids) | also strips the TIR primitive namespace (§2.7) |
| `consensus/core/src/palw_mode_v2.rs:1054, 1595-1640` | bundle `court_catalog_root`, `verify_against_catalog` | unchanged; IR genesis rows (Phase H only) verified by the IR gate |
| `consensus/core/src/palw_e2e_adjudicability.rs:174, 625, 693` | `PalwE2eFamilyV1`, `family_certified_for_weight_v2`, `palw_rc_certified_families_v1` | an IR family is a family whose `kernel_ids` are TIR primitive ids; certified on chain (`FamilyCertified`, ADR-0075) so `court_e2e_root` never moves (§5 Q6) |

### 1.3 Registration carriage, objects, state

| Where | What | Phase F change |
| --- | --- | --- |
| `consensus/core/src/palw_state_v2.rs:6237-6281` | `PalwConsensusObjectV2::ClassRegistered { …, admission: Option<Box<PalwClassAdmissionCarriageV2>> }` | unchanged (its borsh is on chain) |
| `palw_state_v2.rs:5689` | `PalwClassAdmissionCarriageV2 { profile, canonical, registrant_bond, signature }` | unchanged; new `PalwTirAdmissionCarriageV1` (Appendix A) |
| end of `PalwConsensusObjectV2` | positional borsh discriminants | append `ClassRegisteredTirV1`, `CourtTirRootClaimed`, `CourtTirDissected`, `CourtTirChildChosen` (§2.8) |
| `palw_state_v2.rs:7394-7410, 7426` | 4 bought registrations per block; 1 MSK burn | the IR variant counts in `palw_class_registration_buyer_v1` and pays the same burn |
| `palw_state_v2.rs:7453-7469, 7667` | ObjectChunk (100 KB × 16, 8 groups; FamilyCertified only) | unchanged (D3 does not use it) |
| `palw_state_v2.rs:4940-4987` | `PalwClassStateV2` (hashed whole in `classes`) | unchanged; IR sets `fused_attention = true` iff some commit point is dissected (reuses the Terminal clock at `26740-26776`) |
| `palw_state_v2.rs:11648` | `state_root` (collections in frozen order; late tables enter once non-empty, ADR-0087 M7) | new `tir_classes` and `tir_dissections` tables, rooted only once written (impossible before the fence) |
| `palw_state_v2.rs:20919-20960` | `open_model_lifecycle` derives registry work from the carriage profile | IR arm derives `tir_model_work_v1` |
| `consensus/core/src/palw_lifecycle_objects_v2.rs:99-248` | `palw_lifecycle_object_may_ride_v2` (stateless) | `ClassRegisteredTirV1 => Ok(())` at every height (a block carrying it must be valid on both builds) |
| `palw_lifecycle_objects_v2.rs:679-716, 747-790` | extraction; `validate_palw_lifecycle_tx` + A-2 tolerance of undecodable payloads | unchanged; A-2 is what makes appending safe (§2.2) |
| `processor.rs:≈9580-9800` (v9 call `9758`) | `ClassRegistered` acceptance arm (share rule, signature, fences, v9, attributability) | new `ClassRegisteredTirV1` arm: before the fence dropped by name with the block standing; past it signature + v10 + IR attributability |
| `consensus/core/src/palw_carriage.rs` | Stage-0/1 commitment carriage (roots only) | unchanged — class-agnostic |

### 1.4 Execution: step legs, checkpoints, logits

| Where | What | Phase F change |
| --- | --- | --- |
| `consensus/core/src/palw_step.rs:409-498` | `PalwShapeProfileV3` (4 tables ≤ 64 nodes, geometry, `n_threads`/`repack_on` inside the id `466,478`) | unchanged; IR classes are `PalwTirClassV1` (Appendix A) |
| `palw_step.rs:36-45` | profile versions V1/V2 (`palw_step_version_supported`) | V3 for §3 only |
| `palw_step.rs:772-872` | global node slots, `resolve_node_slot`, `shape_profile_id` | IR: spec 04b §3.3 slots; `tir_class_id_v1` |
| `palw_step.rs:1247, 1336, 1432, 1659` | worst-case / deepest-job / canonical leaf counts (closed form); `canonical_step_coordinates` (bitwise search over the running total) | IR twins in `palw_tir_step_v1.rs` using the same closed-form technique over commit points + periodic state/hist leaves (§2.5) |
| `consensus/core/src/palw_step_leg.rs:1207-1228` | `PalwStepTileLeafV1 { coord{call,node_slot,position,tile}, value_count, values_le }` and its hash (binds context + profile hash) | reused as is: 4-byte lanes; the "profile hash" slot carries the IR class id |
| `palw_step_leg.rs:2052-2067, 2133, 2157` | `PalwStepBindingV2` (carries the whole profile), `binding_commitment_root_v1`, `verify_binding` | new `PalwTirStepBindingV1` + `palw_tir_execution_root_v1` (no checkpoint leg, §2.6) |
| `palw_step_leg.rs:1670, 1749` | `PalwCheckpointLeafV2`, `execution_commitment_root_v2` | unchanged; not used by IR classes |
| `palw_step_leg.rs:1975-2019` | `PalwStepFaultV1` (0–18) | append `TirValueOutsideProvenInterval { value_index } = 19`, `TirLogitsTraceMismatch { value_index } = 20` |
| `consensus/core/src/palw_state_chunk_map.rs` | legacy state maps (v1…v4 held) | unchanged except §3; IR state lives in step leaves |
| `consensus/core/src/palw_context_ladder.rs:556, 605-620, 751` | checkpoint interval/cadence, anchored interval | unchanged; IR's `C` is the layout's |
| `consensus/core/src/palw_step_refute.rs:2743, 2750` | flat / tiled logits scheme ids | reused; the scheme id sits in the program |

### 1.5 The court

| Where | What | Phase F change |
| --- | --- | --- |
| `palw_step_refute.rs:254-331, 460` | `KDESC_ALL`, `catalogued_kernel_ids_v1` (`304`), `KERNEL_CATALOG_FENCED_V1` (`322`), `KERNEL_CATALOG` (`460`) | unchanged (the IR court is not a catalog entry) |
| `palw_step_refute.rs:958, 1037, 2087` | `kimi_row`, `qwen36_row`, `base0_row` — per-kernel arms | unchanged; the IR arm is one generic function |
| `palw_step_refute.rs:2514-2527, 2569` | `PalwStepRefuteError` (`Unadjudicable`, `InputSetNotCanonical`, `NoFaultFound`), `PalwExecutionStepRefutationV1` | reused errors; new `PalwTirConeRefutationV1` |
| `palw_step_refute.rs:4484, 4761` | `check_execution_step_refutation_opened_capped_v1`, `run_program` | IR twin `check_tir_cone_refutation_v1` (new module `palw_tir_court_v1.rs`) |
| `palw_step_refute.rs:3201-3244` (`kh = vh % k_heads` at `3222`), `3752` | `qwen36_gdn_slice_v1`, `required_positions` | §3 only |
| `consensus/core/src/palw_court_v2.rs:394-468` | `PalwCourtVerdictProofV2` (positional) | append `TirCone`, `TirDissection` |
| `palw_court_v2.rs:781, 864, 1028, 1690, 1835` | profile pin, `adjudicate_court_close_v3`, `adjudicate_close_proof_v2`, `check_close_cost_v2`, `map_refutation_outcome` | new arms; the narrowed-leaf door and cost gate apply unchanged; mapping reused |
| `consensus/core/src/palw_attn_court_v1.rs:456, 1135, 1241, 1262` | the dissection phase, bottom check, window admission | the protocol skeleton and window rule reused by the generic phase (§2.8) |
| `consensus/core/src/palw_attn_dissect.rs:1-80` | range claims (A16 triple), fold, pinned cut, round bound | cut/round arithmetic reused; claims generalised |
| `consensus/core/src/palw_bisect.rs:39-41, 242, 662-760` | binary and k-ary ladders over step leaves (≤ 48 rounds) | unchanged — the IR step space is a step-leaf space |
| `consensus/core/src/palw_checkpoint_court_v1.rs` | ADR-0103 D1 row-vs-chunk court | not used by IR classes (Hist tiles are step leaves, §2.6) |
| `processor.rs:4391, 9141, 9464` | where `adjudicate_court_close_v3` runs (tip preview, split close, one-carrier close) | unchanged call sites; the new arms live inside the court function |
| `palw_state_v2.rs:26733-26776` | Terminal-turn clock (`court_session_class_is_fused_v2`) | also consults `tir_dissections` |

### 1.6 Canonical work and economics

| Where | What | Phase F change |
| --- | --- | --- |
| `consensus/core/src/palw_economic_compute_v1.rs:289-325` | `gdn_state_elements`, `routed_group_count` (`"router"` in a name), `reads_routed_row` (`".routed"` suffix) | untouched for legacy; never reached by IR classes |
| `consensus/core/src/palw_canonical_work_v1.rs:220, 402, 490-525, 594` | descriptor (profile), `PalwCanonicalWorkVectorV1`, private `PalwCanonicalShapeV1`, `palw_canonical_work_v1` | add `tir_canonical_shape_v1(program, layout)` producing the same shape type; `palw_canonical_work_from_shape_v1` reused (§2.9) |

### 1.7 Node side

| Where | What | Phase F change |
| --- | --- | --- |
| `misaka-palw-sdk/src/lineage.rs:128` | `PalwModelLineageV1` (entries carry `profile: PalwShapeProfileV3`) | `PalwClassEntryV1` gains a graph enum with a `Legacy(profile)` and a `Tir(Arc<PalwTirClassV1>)` variant |
| `misaka-palw-sdk/src/sdk.rs:72, 386` | `builtin_lineages_v1` (dense, qwen36), `registration_candidate` | add `TirLineageV1`; candidates may be IR registrations |
| `misaka-palw-sdk/src/conformance.rs:29` | `check_lineage_v1` | runs over the TIR lineage too |
| `misaka-palw-sdk/src/bin/palw-class.rs:30-60` | `ledger/inspect/preflight/measure/verify/manifest` | add `check-architecture` (§2.11) |
| `consensus/core/src/palw_backend.rs:242-520` | `PalwExecutionBackendV1` (execute, verify, bisect state, court evidence, dissection responder, segments) | implemented once, generically, by the TIR backend |
| `misaka-palw-base0/src/backend.rs:465`, `qwen36_backend.rs:1380`, `qwen25_a16_backend.rs:2269` | the three legacy backends | unchanged; the A16 one is the drill's comparison target |
| `kaspad/src/args.rs:274, 1636`; `kaspad/src/palw_panel.rs:3459, 6752`; `daemon.rs:1832` | `--palw-register-class` → SDK `registration_candidate` | accepts a TIR artifact; builds `ClassRegisteredTirV1`; duties unchanged (always on) |

### 1.8 Drills and batteries (reused, not reinvented)

| Where | What | Reuse |
| --- | --- | --- |
| `misaka-palw-base0/src/e2e_drill.rs:208, 336` | family certification drill over `PalwExecutionBackendV1` (planted faults, both directions, covering set) | TIR family certificate (D-F2) |
| `scripts/misaka-palw-t12-rcore-drill.sh`, `consensus/core/src/config/drill.rs` | salted t12 drill chain on the shipping kaspad; `--palw-drill-fenceN-at` | fence-crossing drill (D-F4) |
| `scripts/misaka-palw-model-registry-devnet-drill.sh` (step 1b clock gate) | registry/listing drill | IR registration → Candidate → Active walk |
| red-team harness (2026-09-07, 8 block forgeries; a `misaminer` `redteam` bin never committed) | 8/8 rejection battery | commit it as `misaminer/src/bin/redteam.rs` and rerun against the IR class (D-F3) |
| `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs` | processor-level conviction test shape | IR court end-to-end in the processor |

---

## 2. Designs

### 2.1 The fence `palw_tir_v1`

```rust
pub struct PalwTirFenceV1 {
    pub activation: ForkActivation,
    /// H64(key "misaka-palw/tir/prim-set/v1", prim_set_descriptor_v1()) — the build's, restated.
    pub prim_set_id: Hash64,
    /// Semantics version of the IR court: cone evaluation, the demand rule, PALW-TIR-33, the
    /// dissection claim form and the step-space layout. 1 for v1.
    pub court_version: u16,
    pub ceilings: PalwTirCeilingsV1,
}
pub struct PalwTirCeilingsV1 {       // a network may only tighten the format caps of 04b §5
    pub max_program_bytes: u32,      // t12 v1: 88,000 (one carrier, §2.2); format cap 262,144
    pub max_unrolled_nodes: u32,     // Σ over occurrences — bounds admission CPU and slots
    pub max_context: u32,            // ≤ history_bound; the largest layout.max_context admitted
    pub max_macs_per_position: u64,  // §8 worst case at H = min(W, max_context)
    pub max_state_bytes: u64,        // Fixed + Hist at max_context (a seat's working set)
    pub max_peak_live_bytes: u64,
    pub max_cone_work: u64,          // admission's own work (04b §10.3); t12 v1: 2^16; format cap 2^20
}
```

**Fingerprints.** Exactly the `palw_heartbeat` discipline (`params.rs:1273`, hashed at `11595`,
visited at `10184`, collapsed at `5680`), but Some-only in `for_each_fence` as every post-launch
fence is:

* `consensus_params_id`: `if let Some(f)` → label `b"palw_tir_v1/court-v1"`, activation height,
  `prim_set_id`, `court_version`, `borsh(ceilings)`, and `palw_tir_court_root_v1(prim_set_id,
  court_version)`. Absent → nothing written, so every preset that leaves it `None` is byte-identical
  to a build without the field.
* `for_each_fence`: the activation only; the value is a price/limit set and normalising it to
  `0/u64::MAX` would make two different ceiling sets fingerprint alike.
* normaliser: the whole option collapses on `activation == never()` (a scheduled future height is
  normalised to `never()` by the identity visitor first), so builds that differ only in a future
  height share an identity and peer through the rollout; `always()` survives and separates.
* `consensus_schedule_id`: name, height and `H(value)` — reported, never gated (ADR-0066 SA-4).
* `palw_fences_v1` → the fork-id gate names it automatically (`fork_id_v1.rs:267`): past the
  height an un-upgraded node is refused. The memory rule "a fence at a scheduled height is invisible
  to the fork id" is enforced by choosing an unused height and by the drill mover's refusal.

**What the fence arms.** Past it: `ClassRegisteredTirV1` is admitted by v10; the IR court arms and
the three dissection moves are admissible; `tir_classes`/`tir_dissections` can be written. Before
it every one of those objects is dropped by name with the block standing, and nothing an IR object
could write exists — the property the ADR-0102 fence has for its fenced kernel.

**`court_catalog_root` and `prim_set_id` (D2).** `palw_court_catalog_root_v1()`
(`palw_catalog_coverage.rs:181`) is copied into every V2 bundle at assembly (`params.rs:14165`,
`palw_rc_identity_v2.rs:534`) and is therefore inside `palw_ruleset_id_v2`; appending anything to
it moves every shipped identity. The RFC's sentence is realised as
`palw_tir_court_root_v1 = H64(key "misaka-palw/tir/court-root/v1", court_catalog_root_v1 ‖
prim_set_id ‖ court_version)`, hashed by the armed fence: two builds whose IR courts differ have
different params ids exactly where the fence is armed, and nowhere else. A network minted later
with the IR from genesis (mainnet, Phase H) may carry it in a new bundle version.

**Validation.** `validate_palw_v2` refuses: a fence value whose `prim_set_id` or `court_version`
is not this build's (a build adjudicates only its own primitive set); ceilings above the format
caps; arming on a ruleset that has not declared `palw_audit_2026_09_11` (§2.2 depends on its A-2
tolerance); arming without `palw_kary_court` active at or below it (dissection); a genesis that
registers an IR class unless the fence is armed at 0. Presets: `None` everywhere; on testnet-12 an
entry `PalwPostLaunchFenceV1 { name: "palw_tir_v1", set }` in a flag-day list after the capacity
steps, at a height the user picks once D-F4 passes.

### 2.2 Program carriage — inline, one carrier (RFC open question 4)

**Recommendation: inline in the registration object, one lifecycle carrier, `max_program_bytes`
≈ 88,000 on testnet-12 in v1; the TIR artifact also embeds the program for availability. Not an
artifact leaf.**

1. **Admission is a consensus check.** `verify_class_admission_*` runs on every validating node in
   the acceptance walk (`processor.rs:9758`). With only `graph_ir_root` on chain, block validity
   would depend on fetching an off-chain artifact — the failure mode ADR-0049 Decision H answered by
   carrying the profile ("it has to carry them anyway — nothing else on a running chain can tell the
   court what the class computes", `palw_state_v2.rs:6258-6280`).
2. **A withheld program is an unadjudicable class.** If the registrant can withhold the program,
   every dispute is `Unadjudicable` (A4). Publishing it once on chain makes it available to every
   party forever (IBD replays the carrier; the artifact carries a copy for seats; every court move
   re-carries and re-hashes it, §2.7).
3. **Size is not the obstacle it looks like.** A block carries ≈125 KB of transactions and a
   standard transaction ≈120,000 bytes (`palw_mode_v2.rs` `PALW_STANDARD_TX_BYTES`;
   `palw_state_v2.rs:7453` sizes chunks at 100,000). A lowered dense model is ≈200 nodes per layer
   block at ≈40 bytes a node plus a few KB of declarations — 10–20 KB; a hybrid or MoE with 2–4
   block kinds 30–50 KB (experts and heads are tensor axes, not nodes). The 64 KiB const allowance
   is the only way to approach 100 KB. One carrier (100,000 less ≈5 KB of signature and fields, less
   the layout) is enough for every corpus program; the network ceiling says so explicitly and a
   program above it is `EXCEEDS(program_bytes)`.
4. **No new lane.** The ADR-0075 chunk lane (`pending_chunks`, 8 global content-keyed groups) is
   shared with certification and squattable; a bond-keyed publication lane in the ADR-0080 style
   would be new state machinery. If a real program ever needs more than one carrier, that lane is
   a separate, later fence.

**Why appending an object is safe.** Older builds on a ruleset that declared
`palw_audit_2026_09_11` tolerate an undecodable lifecycle payload at admission and skip it at
extraction (`palw_lifecycle_objects_v2.rs:747-790`, `:686-690`). The new build decodes
`ClassRegisteredTirV1`, lets it ride (stateless may-ride `Ok`), and before the fence drops it in the
acceptance walk by name, folding nothing, with no rent (`palw_object_rent_ceiling_v1`'s default
arm is 0) — the same outcome as the older build. Past the fence the older build is on the other
side of the fork id anyway. `validate_palw_v2` refuses to arm the fence without the audit
declaration, because without A-2 an older build fails the whole block.

### 2.3 The IR class and its identity

```rust
pub struct PalwTirClassV1 {
    pub version: u16,                 // 1
    pub program: Vec<u8>,             // canonical TirProgramV1 bytes (04b §4)
    pub layout: PalwTirLayoutV1,
    pub tokenizer_id: Hash64,
}
pub struct PalwTirLayoutV1 {
    pub version: u16,
    pub max_context: u32,             // positions a job may touch (prefill + decode − 1); ≤ history_bound
    pub checkpoint_interval: u32,     // C: Fixed state leaves every C positions (§2.6)
    pub h_tile: u32,                  // canonical history chunk: dissection bottom and Hist tiles
    pub commit_tiles: Vec<u32>,       // tile_len per committed node, (block, node) order
    pub state_tiles: Vec<u32>,        // per StateDecl: Fixed → lanes per tile; Hist → lanes per sub-row
}
graph_ir_root   = H64(key "misaka-palw/tir/graph-ir-root/v1", program)
tir_class_id_v1 = H64(key "misaka-palw/tir/class-id/v1",
                      graph_ir_root ‖ H64(borsh(layout)) ‖ artifact_root ‖ tokenizer_id)
```

* `logits_scheme_id` and `prim_set_id` are fields of the program, so the class id commits to both
  through `graph_ir_root` (PALW-TIR-7). `n_threads`, `repack_on` and every other node-local knob do
  not exist in an IR class (they are inside the legacy id, `palw_step.rs:466,478`).
* `artifact_root` is the TIR inventory root (§2.10), a function of the program's param declarations
  and the tensors — so "one root, one owner" (ADR-0143) holds by construction.
* The job context's existing `shape_profile_id` field carries the IR class id (it is an opaque
  `Hash64` everywhere it is read), so every step leaf, leg root and execution root binds the class
  exactly as it binds a legacy profile, with no new job-context field. The key is distinct from
  `PALW_STEP_DOMAIN_SHAPE_PROFILE_V3`, so an IR id can never equal a legacy profile id.
* **Layout inside the id (D5).** The step space (and therefore every commitment) depends on the
  tiles, `h_tile` and `C`; a producer and a court must agree on them, and a declared, verified value
  is the legacy discipline (`tile_len` is inside the profile today). Deriving them instead would make
  a class id a function of court ceilings that a later fence may change. Several layouts of one
  program are several classes; each costs a registration burn and earns weight separately
  (ADR-0145 §7), and the work vector ignores the layout (PALW-TIR-16), so re-tiling buys nothing.

### 2.4 Admission v10

`verify_class_admission_v10(bundle, fence: &PalwTirFenceV1, carriage: &PalwTirAdmissionCarriageV1,
registration, certified, chain_certified, court: PalwKaryCourtV1, held: PalwHeldAdmissionV1, ...)
-> Result<(PalwClassCatalogEntryV2, PalwTirClassRecordV1), PalwClassAdmissionError>`, in this order
(cheapest refusal first, as v9):

1. `program.len() ≤ fence.ceilings.max_program_bytes`; `TirProgramV1::decode_canonical` (strict
   Borsh, re-encode identity, NF-1…22) — an unknown primitive tag is `NEEDS_PRIMITIVE`.
2. `program.prim_set_id == fence.prim_set_id`; `logits_scheme_id` ∈ {flat, tiled} (as v9
   `2262-2268`); `history_bound = 2^21` requires `held.armed` (the held regime, ADR-0103).
3. Layout well-formed: `commit_tiles` has one entry per committed node, each in
   `[PALW_STEP_MIN_TILE_LEN, PALW_STEP_MAX_TILE_LEN]`; `max_context ≤ min(history_bound,
   ceilings.max_context)`; `h_tile` a power of two in `[16, 4096]`; `C ≥ 1`; `state_tiles` divide
   their state's lanes.
4. `tir_admit_v1(&program, &TirAdmitInputsV1 { layout facts, terminal ceilings from bundle.court,
   ceilings })` (tir/core) → `TirAdmissionV1` or a refusal named by limit (§2.12). It proves
   PALW-TIR-9 (ranges), PALW-TIR-12 (costs at `H = min(W, max_context)`), PALW-TIR-13 (every commit
   point's tile cone within 16 Mi MACs / operand units / bytes, or dissectable), and derives each
   Fixed state's `C_j`.
5. `layout.checkpoint_interval ≤ min_j C_j`; every non-dissected tile cone plus its worst state
   replay (`C − 1` positions of demand-restricted update cones) within the terminal ceiling.
6. Canonical job: `footprint ≤ max_context` (v9 `2266-2272` in IR units); leaf counts (worst case,
   deepest legal job, canonical) against the class's ladder (`palw_class_step_ladder_v1`);
   `pwu_per_inference == counted` (v9 `2636-2647`).
7. Close bytes: worst `TirCone` close = program + layout + demanded operand units + paths
   ≤ `max_close_bytes`/`max_close_chunks` (`palw_close_fits_court_v1`); dissection moves at the
   court's arity fit one carrier (`palw_attn_dissect_arity_fits_carrier_v1` generalised).
8. Window: `palw_attn_court_admits_row_v1(played, max_context, h_tile, window_court)` (or `_held_`)
   for a class with a dissected commit point, as v9 `2585-2625`.
9. `class_id == tir_class_id_v1(...)` (`TirClassIdIsNotDerived`).
10. Weight: `share_permille > 0` needs a certified family covering the class's reachable primitive
    ids (§2.7, §5 Q6) — registration stays permissionless at 0‰ (ADR-0069 D5).
11. Past `palw_offence_attribution`: the IR twin of `palw_attributable_class_v1` and
    `palw_held_class_is_attributable_v1` (every dissected site answerable within the compute turn).

It returns the catalog entry (`max_step_leaf_count`, `canonical_step_leaf_count`,
`reachable_kernels` = the primitive ids, `court_cost`) and a `PalwTirClassRecordV1 { graph_ir_root,
layout_digest, tokenizer_id, prim_set_id, dissected_slots, step_shape }` for the new `tir_classes`
table (the state-only facts the court clock and the lane need; `step_shape` is per block, ≤ a few KB).

**DoS.** Admission is linear in the unrolled node count, bounded by `max_unrolled_nodes`; its CPU
time is measured with `consensus/core/tests/dos_l2_registration.rs`'s harness; the existing 4 per
block and 1 MSK burn apply.

**As built (F6, `palw_tir_admission_v1.rs`).** Where the implementation settled what the sketch above
left open:

* `tir_admit_v1` takes one `tile_len`; a layout tiles each commit point at its own. Admission runs it
  once per distinct commit tile length (at most 8, else refused), with the per-tile ceilings disabled,
  and checks each commit point's cone at its own length against the court: its tile MACs within
  `max_terminal_macs`, and its evaluation (the tile's MACs, elementwise and transcendental work plus
  `C − 1` positions of the worst `Fixed`-state replay it reads) within `palw_tir_court_limits_v1`. A
  per-commit-point `tile_len` in `TirAdmitInputsV1` would make this one run (a tir/core follow-up).
* A cone that reduces over `H` is adjudicated whole until F7 wires the dissection: its tile at
  `H = W` must fit like any other, else `TirNeedsDissection`.
* **One IR registration a block reaches v10** (`PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`, the
  coordinator's release item of 2026-09-29): sizing costs every node up to seconds, so the acceptance
  walk drops a further `ClassRegisteredTirV1` by name before any rent, slot or fee — exactly as the
  below-fence drop — and the fold refuses a second as its second lock (`TirRegistrationsPerBlockExceeded`).
  One the walk drops before the gate (unsigned, a stale target, refused by the rehearsal) takes no place.
* The close-bytes check (program + frame + the worst tile's opened operand bytes ≤ `max_close_bytes`)
  is a necessary condition only: `tir_admit_v1` reports element-granular demand, and a close carries
  whole step leaves and whole inventory pieces with their paths. A sufficient bound needs per-leaf
  demand from `tir_admit_v1` (the same follow-up).
* The canonical job must BE the attempt formula's yardstick context (`palw_tir_job_context_v1` at
  `(f − 1, 2)`) on every network, and a canonical prompt past J5b's inline bound must be committed in
  the Merkle form — the legacy attributability rule, unconditional for a class type with no legacy.
* **Tokens against positions (ref2, `tir/ref2` 533e7b7fa item 7).** The job context counts a job in
  TOKENS — `prefill + decode ≤ max_context_tokens`, the v2 family's rule (`check_job_context_shape`)
  that every court path runs — and the layout in POSITIONS — `prefill + decode − 1 ≤ max_context`
  (`job_shape`): the last emitted token is produced by the last position's logits and never fed
  back. The yardstick context therefore states `max_context_tokens = max_context + 1`, and the two
  rules agree on every `(prefill, decode)`: the longest job, all `max_context` positions, is admitted
  by both and runs end to end (`a_job_at_exactly_max_context_positions_runs_end_to_end_in_the_canonical_context`).
  The legacy context states `n_ctx` (its stricter reading); that one field is the IR context's only
  difference from the legacy one over the same facts.
* A program at the held history bound is refused in v1: the held regime's accusations and answers
  carry legacy bindings. The class's ladder is the network's.
* The weight check reads `reachable_kernels = { kernel_semantics_id_v1("palw-tir/v1/prim=<Name>") }`.
* On the corpus at a 64-position context (testnet-12's court): the five models are admitted in
  6–50 ms each (debug build); the dense and sliding-window models' attention cones, adjudicated whole
  at `H = W`, open 16.8 MB, just under testnet-12's 16.82 MB close ceiling.

### 2.5 The step space of an IR class

**Positions.** Absolute position `a`: prefill `p` is `a = p` at coordinate `(call 0, position p)`;
decode call `c ≥ 1` is `a = P + c − 1` at `(call c, position 0)` — the legacy coordinates, so the
bisection, the decode-token door and every opening format are reused.

**Per position, in order** (all of it before position `a + 1`):

1. **Commit points of the program.** Occurrences `o_0 … o_(L+1)`; node slots exactly spec 04b §3.3
   (every node counted, so a slot names the same node for producer and court); a leaf per tile of
   each node with `commit = true`, `E(out at H = min(a + 1, W_block))` elements, `tile_len` from the
   layout, last tile ragged. The `post` occurrence runs only at `a ≥ P − 1` (where logits are
   consumed); this needs one normal-form rule from tir/core: **`post` contains no `StateWrite` or
   `HistAppend`**, so skipping it elsewhere is exact (§5 Q4).
2. **Fixed-state checkpoint leaves** when `(a + 1) % C == 0`: for each Fixed state instance (state
   index, layer ascending), the value after this position's `StateWrite`, in `state_tiles[j]`-lane
   tiles, at reserved slots after the program's last slot.
3. **Hist tile leaves** when `(a + 1) % h_tile == 0`: for each Hist state instance and each sub-row
   (`state_tiles[j]` lanes of the row, e.g. one KV head), the `h_tile` rows of positions
   `a + 1 − h_tile … a` concatenated, at the next reserved slots.

Lanes are 4 bytes: `i8`/`i16` sign-extended, `i32` as is, `idx` unsigned; `i64`/`i128` are never
committed (NF-17). The leaf preimage is `PalwStepTileLeafV1` unchanged; its "profile hash" is the
IR class id.

**The invariant that makes this one tree.** *Every leaf is adjudicated from leaves that precede it
in the enumeration* — a cone reads commit points of lower slots in the same position, carry-outs of
earlier occurrences, rows and tiles of earlier positions, and state leaves of an earlier checkpoint;
a state leaf reads the previous checkpoint's state leaves and the rows in between; a Hist tile reads
the `h_tile` rows it concatenates. The ladder narrows to the **first** divergent leaf, whose
predecessors all agree with the honest execution, so the court convicts iff that leaf is wrong. A
forged checkpoint or a forged history tile is itself the first divergent leaf — no second tree, no
checkpoint court, no "trust the anchor" gap. (Tested as a property in step F4.)

**Counting and coordinates.** Per position the leaf count is piecewise affine in `a` (one piece per
distinct window `W` below `max_context`) plus two periodic terms; the worst case, the deepest legal
job and the canonical job are closed forms, and `tir_step_coordinates_v1` finds a leaf by the same
bitwise search over the running total `canonical_step_coordinates` uses (`palw_step.rs:1659-1720`) —
no walk proportional to the context. `canonical H chunks` are `h_tile`-aligned ranges of `[0, H)`
with the pinned cut rule of `palw_attn_dissect` for the dissection's children.

**Logits.** The logits node is a commit point (NF-6) and the class also commits
`full_logits_trace_root` under its scheme (the decode-token arms and the generated-id pin read it).
The layout forces the logits node's `tile_len` to the scheme's tile, and a new structural fault
`TirLogitsTraceMismatch` convicts an executor whose step tile and trace tile differ (two openings,
one move) — the two commitments of one row can never disagree unpunished.

**Execution root.** `palw_tir_execution_root_v1 = H64(key "misaka-palw/tir/execution-root/v1",
ctx_hash ‖ full_logits_trace_root ‖ step_leg_root_v1(ctx_hash, class_id, leaf_count, merkle_root))`
— a new domain, so an IR root can never verify as a legacy one.

### 2.6 Fixed-state checkpoints and replay; history

* The state at the start of position `a` (what `Ref::State(j)` reads) is the latest checkpoint's
  state leaves at `q < a` (or all zeros at genesis, 04b §3.4) advanced by replaying the **update
  cones** of the co-dependent Fixed states over positions `q + 1 … a − 1`, each reading only
  per-position commit points and params. A refutation that needs `State(j)` carries those state
  leaves and those commit points; the court replays with the reference evaluator (saturating
  `StateWrite` included).
* A state leaf at checkpoint `k` is adjudicated by the same replay from checkpoint `k − 1` over `C`
  positions.
* Replay is **demand-restricted** (§2.7): a disputed GDN head replays one head's `S`
  (`dv × dk`), the two key-head slices it reads and its conv channels — the head slicing the legacy
  court hand-writes (`palw_step_refute.rs:2011-2016`). `tir_admit_v1` derives `C_j` from the
  demand closure of state `j`'s update over one position (a fixpoint over the scan, per element
  group) and the terminal ceiling; the layout's `C ≤ min_j C_j`.
* History: a cone that reads a `HistAppend` output at `H` needs the prior rows. Complete, aligned
  `h_tile` blocks are opened as one Hist tile leaf per sub-row; the partial block since the last tile
  as per-position rows (each a `HistAppend` input, a commit point by NF-20). The dissection bottom
  (§2.8) opens exactly one tile.
* **No separate checkpoint leg for IR classes.** Seats resume (ADR-0133) from state leaves and
  tiles, which are openable against the step root. The legacy checkpoint leg, chunk maps and
  ADR-0103 D1 court remain for legacy classes only.
* Cost: a 35B-scale hybrid commits ≈16 M state lanes per checkpoint (≈245 leaves at 64 Ki tiles);
  KV tiles of a 1.5B dense model ≈112 leaves per 64 positions. Both are far below the ladder at the
  contexts the ceilings admit; the held regime's `2^40` ladder applies at `history_bound = 2^21`.

### 2.7 The generic court arm and PALW-TIR-33

**Object.** `PalwCourtVerdictProofV2::TirCone { refutation: Box<PalwTirConeRefutationV1> }`
(appended, tag 5), the refutation carrying every unit it needs: `PalwTirConeRefutationV1 { binding:
PalwTirStepBindingV1, output_opening, output_preimage, operands: PalwStepInputRowV1, params:
Vec<PalwArtifactOpeningV1>, prompt_token_ids, prompt_ids_openings, decode_tokens }` (Appendix A;
as built in F5, `palw_tir_court_v1`). Beside it, appended in the same step: `TirLogits` (6, the
logits-consistency accusation), `TirDecodeTokenTiled` (7) and `TirDecodeToken` (8), the decode-token
door over an IR binding. Below `palw_tir_v1` the acceptance layer drops an IR close by name, and the
fold — through the `#[borsh(skip)]` mirror `PalwStateParamsV2::tir_from_daa`, synced by
`Params::sync_palw_tir_v1` and checked by `validate_palw_tir_v1` — reads an assembled one as bytes
that do not decode, exactly as an older build reads a variant its enum lacks. The court's work
limits for one IR close are `4 × max_terminal_macs` in both counts (`palw_tir_court_limits_v1`);
admission v10 (F6) must refuse a class whose worst cone would not fit them.

**Check order** (in `palw_tir_court_v1::check_tir_cone_refutation_v1`, called from
`adjudicate_court_close_v3`/`adjudicate_close_proof_v2` for a claim whose class is in
`tir_classes`):

1. `check_close_cost_v2` on bytes (before any path is walked); the narrowed-leaf door
   (`opened == narrowed`, `palw_court_v2.rs:916-921`).
2. Binding: `H(program) = graph_ir_root`, `tir_class_id_v1(..) == claim.class_id`,
   `ctx.shape_profile_id == class_id`, the trace root and `palw_tir_execution_root_v1` equal the
   claim's (`check_arithmetic_close_binding`, `check_execution_root_binding`).
3. `decode_canonical` (+ `prim_set_id` = the fence's) and `analyze_ranges` — block-local intervals,
   linear in the program; the court recomputes them rather than trusting a stored copy.
4. Structural checks on the output leaf (coordinate canonical, value count, lane width) — the legacy
   `check_step_refutation_capped_v1` discipline, and the leaf must be a commit-point, state or hist
   leaf of this class.
5. **PALW-TIR-33 on the output tile:** every lane must be inside its node's proven interval (for a
   state leaf, inside `[lo, hi]`; for a Hist tile, inside the row node's interval) → otherwise
   `Ok(TirValueOutsideProvenInterval)`: the executor committed a value no execution can produce.
6. Verify every carried unit — step range openings against the step root, artifact openings against
   the class's `artifact_root`, prompt ids / decode pins against the job — and build a read-only
   leaf oracle from them. **PALW-TIR-33 on every opened committed operand** (including state
   leaves and history rows): a violation convicts the executor, who committed it, whichever leaf the
   challenger disputed. After this step no cone value can leave its interval (soundness of
   `analyze_ranges`), so the evaluation cannot overflow.
7. **Demand evaluation.** The element-pull evaluator (§2.12) computes exactly the tile's elements,
   pulling operand elements through each primitive's index map (elementwise and broadcast → same
   index; `Reshape`/`Transpose`/`Slice`/`Concat` → remapped; `MatMul` → a row of `a` and a column of
   `b`; reductions and `TopK` → the whole axis; `Gather` → the index element first, then the data
   row it names; `HistAppend` → the demanded history rows), memoised per `(node, index)`, replaying
   state where a `State` ref is pulled. Every leaf request is served by the oracle and recorded; a
   request the oracle cannot serve, or a carried unit never requested, is `InputSetNotCanonical`
   (refused, nobody slashed); the canonical order is the sort of the recorded set. Work is counted
   and the evaluation aborts past the terminal MAC ceiling (admission guarantees an honest cone fits).
   Index values that steer demand come only from authenticated leaves.
8. Compare with the committed tile: different → `ComputationMismatch { value_index }` →
   `ExecutorGuilty`; equal → `NoFaultFound` → `ChallengerDefeated` (`map_refutation_outcome`,
   `palw_court_v2.rs:1835`). An evaluator error after step 6 can only be an interpreter defect:
   `Unadjudicable` → the close is refused and nobody is slashed; the golden vectors and the second
   implementation exist to make that set empty.

**Why demand evaluation and not whole-node `eval_cone`.** The LM head's cone is a `[1, D] × [D, V]`
product (≈300 M MACs at `V = 151,936`); a GDN layer's `S` is `v_heads × dv × dk`. Whole nodes would
exceed every ceiling; element demand gives a vocabulary tile (`tile_len × D` MACs) and a single head,
for every architecture, without a per-kernel slice function (`qwen36_gdn_slice_v1`,
`qwen36_lane_slice_v1`, … at `palw_step_refute.rs:3201-3335`).

**Catalog and A4.** The IR court is not a `KERNEL_CATALOG` entry. An IR class's `reachable_kernels`
is `{ kernel_semantics_id_v1("palw-tir/v1/prim=<Name>") }` over the primitives it uses — a namespace
`catalog_covered_kernels_v1` strips (as it strips the fenced tables) so the identity catalog is never
asked about it. Coverage for an IR class is "the program decodes under v1": A4 is proved once, for the
25 primitives, by the golden vectors, the second implementation and the court battery (D-F2).

**Who loses on PALW-TIR-33.** The executor — every committed value is the executor's statement.
A challenger cannot manufacture a violation: the values are opened against the executor's roots.
A dissection claim outside its reduction's interval (§2.8) is refused as a move, so the responder's
rung clock runs on.

### 2.8 Generic H dissection

A commit point is **dissected** when its tile cone contains a reduction over `H` (`ReduceSum`/
`ReduceMax` along `H`, or `MatMul` contracting `H`) and its direct cost at `H = min(W, max_context)`
exceeds the terminal ceiling; admission lists the dissected slots (`PalwTirClassRecordV1`). Every such
reduction reduces an H-local tensor (PALW-TIR-32), and every sum and maximum is exact, so ADR-0082's
argument transfers verbatim:

* **Root claim** (`CourtTirRootClaimed`, responder, the Terminal move on a dissected leaf): for each
  H-reduction node `r_1 … r_n` of the tile's cone (node order), its totals over `[0, H)` restricted
  to the demanded elements, as integers of the node's dtype (`i128` wire form; dispute objects, not
  commitments, so PALW-TIR-5 does not apply). Checked before any round: each value inside its node's
  interval, and the cone evaluated with `r_i` **supplied** from the claim reproduces the committed
  tile (the "finalize" check, i.e. `eval` with supplied nodes — spec 04b §9.2).
* **Rounds** (`CourtTirDissected` / `CourtTirChildChosen`): `k` children over the pinned cut of the
  disputed range at `h_tile` granularity (`palw_attn_dissect` arithmetic); each child claims the
  partials of every `r_i` over its range, computed against the root's H-free values; the fold is
  `Σ` (sum, matmul) or `max` per reduction, exact. A disclosure that does not fold is the responder's
  self-contradiction → conviction; the challenger names a child.
* **Bottom** (`TirDissection` close): one `h_tile` range. The court opens the tile's history (one
  Hist tile leaf per sub-row, or the rows), the H-free leaves of the H-local subgraph, and takes the
  earlier reductions' totals from the root claim where later H-local nodes consume them (the softmax
  maximum inside `IntExp(s − m)`), evaluates the H-local subgraph over the range only
  (`eval_h_range`, §2.12), and compares every partial. Mismatch → `ExecutorGuilty`; match →
  `ChallengerDefeated`.
* **State.** Phases live in a new `tir_dissections` table (root once written); the session clock
  (`palw_state_v2.rs:26740`) reads it; `PalwClassStateV2.fused_attention = true` for a class with a
  dissected slot, so the existing Terminal-turn rule makes the responder owe the root claim or an
  acquitting `TirCone` close, and silence loses on the responder's side. Fence: `palw_tir_v1` (the
  k-ary court is a prerequisite); signatures as the attention moves (`palw_lifecycle_objects_v2.rs:
  139-148`).
* **Bounds.** Rounds `⌈log_k(max_context / h_tile)⌉` within `PALW_BISECT_MAX_ROUNDS`; a child costs
  `Σ_i |demanded(r_i)| × 16` bytes; admission checks that `k` children fit one carrier and the whole
  exchange fits `window_court` (`palw_attn_court_admits_row_v1`).

This is the legacy attention dissection with the triple `(m*, S*, V*)` replaced by "every H-reduction
of the cone"; the A16 attention of the drill class lowers to exactly three reductions (max, exponent
sum, value sum).

### 2.9 The structural work vector

As built (F8, `palw_tir_work_v1`): `palw_tir_work_shape_v1(program) -> PalwTirWorkShapeV1`, its own
type — the legacy `PalwCanonicalShapeV1` is affine in the kv length and cannot hold a window, while an
IR block costs `c₀ + c₁ · min(a + 1, W)` — and `work_v1(facts)` sums the executed positions in
closed form into the same `PalwCanonicalWorkVectorV1`. Per node, spec 04b §8's formulas (affinity in
`H` checked, not assumed), classified by structure only, in the precedence dense/routed matmul >
normalization > recurrence > attention > other; "recurrence" is refined to the nodes on a path from a
`State` read to a `StateWrite` plus the matmuls reading the state (so a projection feeding the update
stays a weight matmul):

| dimension | nodes |
| --- | --- |
| `dense_matmul` | `MatMul` with an operand reaching a `Param` through structural primitives only (no `Gather`) — incl. the LM head |
| `routed_expert_matmul` | `MatMul` whose `Param` operand passes through a `Gather` whose indices trace to a `TopK` — only the gathered (active) experts' MACs, which §8 already counts |
| `attention_prefill` / `_decode` | nodes whose output shape contains `H`, and reductions over `H`; split by position class |
| `recurrence` | the update cones of Fixed states (backward from `StateWrite` within the block), and `MatMul`s with a `State` operand |
| `normalization` | `ReduceSum`s whose output reaches an `IntRsqrt` through scalar ops, and those `IntRsqrt`s (RFC OQ7; alternative in §5) |
| `other_verified_ops` | every other node (elementwise × table `elementwise`, transcendental evaluations × `transcendental`) |
| `weight_traffic_bytes` | bytes of `Param` operands read by `MatMul`s (gathered rows only) |
| `kv_read_bytes` / `kv_write_bytes` | `H × row bytes` read through each `HistAppend` output; the appended row's bytes |

Nothing reads `commit`, `tile_len`, `h_tile`, `C` or node names (PALW-TIR-16, PALW-WK-3; a test
re-tiles and re-commits a program and requires an identical vector). Qwen2.5-1.5B-A16 as an IR
program (tir/lower's `a16_mirror_program`) prices `dense_matmul` and `weight_traffic_bytes` exactly as
the legacy graph-v7 class does; its total arithmetic is 1.3-2.1 % higher, because the IR counts every
integer operation of the requantisation, rotary and norm segments (report in
`misaka-palw-base0/tests/tir_a16_work_vs_legacy.rs`). The registry's
`PalwModelWorkV1` comes from the same walk (`tir_model_work_v1`); `palw_economic_compute_v1.rs:
297-325`'s name heuristics are never reached by an IR class.

**The `Select`-arm credit (past `Params::palw_tir_fence2`; RFC-0004 open question 4).** The walk above
credits both value operands of a `Select`, but a backend may legally skip, per element, the arm the
condition does not choose, and no court reads work that reaches no commit point — so a class could be
paid for work never done (P5). Past the second IR fence the registry records
`palw_tir_model_work_v2(.., true)`: for each `Select` `s`, in ascending node order, and each value
operand `k`, the **arm-only region** `E(s, k)` is the set of nodes that are not sinks (a commit point —
carry-outs and the logits included —, a `StateWrite`, a `HistAppend`), have a use, and whose every use
is `s` at operand `k` or a node of `E(s, k)`. With `w(n)` a node's affine work (§8, per dimension),
`A_s = Σ_{E(s,1)} w − Σ_{Select s' ∈ E(s,1)} δ(s')` and `B_s` likewise, the discount is `δ(s) =
max(A_s, B_s)` coefficient by coefficient, and a block is credited `Σ_n w(n) − Σ_s δ(s)`: each arm pair
at the coefficient-wise minimum of the two, an affine form never above `min(A(H), B(H))` — so never
more than any execution does, and nested `Select`s are credited first. On the corpus it moves the
elementwise work by 1–20 % of the toy programs with `Select`s and the A16 decoders by under 0.02 %;
a program without a `Select` is unchanged (`tests/palw_tir_select_credit.rs`).

### 2.10 The SDK lineage and the node backend

* **Consensus artifact layout (the "TIR inventory").** Leaves in declaration order: for each param
  `j`, each instance (global, or each layer whose block references it, ascending), each axis-0 row
  (split at 32 KiB, so every leaf fits readiness V2's 40 KiB openable leaf):
  `PalwArtifactOperandV1 { tensor_name: ParamDecl.name, layer, row_start (byte offset), bytes }`;
  root by the existing tree (`palw_artifact.rs:40-110, 248`). Lowerers store
  matmul weights output-major (`[N, K]`, consumed through `Transpose`) so a vocabulary tile opens
  `tile_len` rows. The container `PALWTIR1` embeds the program, layout and tokenizer id; the
  `.palwmanifest` sidecar (`misaka-palw-sdk/src/class_manifest.rs`) records the inventory root.
  The tir-lower crate-local format (`PALWTIRA`) is converted, not reused.
* **`TirLineageV1`** (`misaka-palw-sdk/src/lineages/tir.rs`): sniffs `PALWTIR1`; `load` verifies
  `H(program)` and computes the root once (streamed); `classes()` yields the IR classes of the
  artifacts the node holds (IR classes are data, not a built-in table); `pair` checks the tensors
  against the declarations; `resolve` builds the backend. `PalwClassEntryV1`'s graph becomes an
  enum; every consumer already goes through `class_id()`.
* **Backend, generic first** (`misaka-palw-tir-exec`, node software): typed native storage
  (`i8/i16/i32` by dtype; `i64/i128` only where the proven interval needs it), zero-copy params from
  the mapped artifact, row-lazy `Gather`, multithreaded `MatMul` inside order-free regions only,
  per-position evaluation (prefill batching later). It implements `PalwExecutionBackendV1`:
  `execute` (streams leaves in the §2.5 order into the step-leg builder), `verify_material`,
  `bisect_prefix_state`, `disclose_trace_event`, the cone evidence builder (the §2.7 demand run in
  "record" mode), the dissection responder (partials), and resume from state leaves. **Gate (F-4
  applied to generic kernels):** byte-identical to the reference evaluator on every golden vector,
  on fuzzed programs, and on the corpus programs over many positions — and to the second
  implementation when it lands.
* **Legacy fused kernels later.** The A16 and Q36 engines become fused implementations of the IR
  segments tir/core has shown byte-identical (Phase G); nothing in Phase F waits for them.

### 2.11 The tool: `palw-class check-architecture`

```
palw-class check-architecture --network <id> --config config.json [--weights-index …] [--legacy]
palw-class check-architecture --network <id> --tir program.tir [--layout layout.json]
```

* IR mode: tir-lower (non-consensus) turns the config into a program and a default layout
  (`tir_default_layout_v1`: tiles from the cone costs, `C` = min `C_j`); the verdict is
  `tir_admit_v1` — **the function v10 calls** — with the network's `palw_tir_v1` ceilings (the v1
  defaults while the fence is dormant, stated in the output). Verdicts: `ADMISSIBLE`,
  `ADMISSIBLE_GENERIC`, `EXCEEDS(limit, value, cap)`, `NEEDS_PRIMITIVE(name)`, `NOT_LOWERABLE(reason)`,
  and `UNVERIFIED` where a check needs weights not given.
* Legacy mode (`--legacy`, Phase 0(b)): map the config to the nearest shipped lineage geometry (dense
  A16, Qwen3.6 hybrid) and run the processor-same preflight (`palw_model_registration_v1.rs:322-362`
  → `verify_class_admission_v9`) with the network's fences; `NEEDS_KERNEL(descriptor)` where the
  catalog lacks one.
* Both call consensus functions; neither has its own opinion.

### 2.12 What Phase F needs from `misaka-palw-tir` (interface requirements)

| Item | Owner (proposed) | Needed by |
| --- | --- | --- |
| `tir_admit_v1(program, inputs) -> TirAdmissionV1 { intervals per node, §8 cost vector at H_max, per-commit-point worst tile cone cost (MACs, bytes, operand units), dissected commit points and their H-reductions, per-Fixed-state C_j }`, refusals named by limit | tir/core (Gate 2) | F6 |
| element-pull evaluator `eval_demanded(block, layer, target, elements, oracle) -> (values, recorded leaf requests, work)` + a box-demand analysis for admission's worst case | Phase F writes, tir/core reviews; ref2 implements it too | F5 |
| `eval_h_range(block, layer, reductions, range, supplied)` — H-local nodes over `[t0, t1)`, `Iota` over `H` offset, `HistAppend` restricted | Phase F writes, tir/core reviews | F7 |
| demand closure over the scan for Fixed states (per element group), used for `C_j` and by the court's replay | tir/core (inside `tir_admit_v1`) | F5/F6 |
| normal-form rule: `post` has no `StateWrite`/`HistAppend` | tir/core (spec 04b §5) | F4 |
| spec 04b §10.1 wording: Fixed-state checkpoints and Hist tiles are step leaves (D7) | tir/core | F4 |
| keyed hashing stays in consensus-core (the crate exposes bytes) | — (already so) | F1 |

#### 2.12.1 Close sizing — the exact interface admission needs (decision 3, 2026-09-28)

**Why.** A court is clocked at every terminal leaf, so an admitted class whose worst terminal close cannot
ride the fold (at most `min(court.max_close_chunks, PALW_COURT_CLOSE_MAX_CHUNKS = 32)` chunks of
`PALW_COURT_CLOSE_CHUNK_MAX_BYTES`, ≈ 3.2 MB on testnet-12) convicts an HONEST executor. The first v10
measure (the 16 KiB frame plus `ConeV1::terminal_opened_bytes`) is element-granular — committed lanes at 4
bytes, params at their width — and is a lower bound only: a close carries WHOLE units with their openings.
`PalwTirConeRefutationV1::params` is one `PalwArtifactOpeningV1` per inventory leaf, each with a full path.
Measured on the Qwen2.5-1.5B A16 mirror: 959,657 inventory leaves → 20 siblings → 1,280 path bytes per
opening, against a 1,536-byte `output.weight` row; a logits tile of `T` vocab rows carries ≈ `T × 2,860`
bytes, twice the element measure: `T = 2048` is ≈ 5.9 MB (element measure 3.1 MB), `T = 1024` ≈ 2.93 MB.
(3B: 2,048-byte rows, depth 21 → `T = 1024` ≈ 3.5 MB; `T = 512`.)

**The interface (misaka-palw-tir, `admit.rs`).**

```rust
pub struct TirAdmitInputsV1 {
    /// Values per step leaf, per commit point in the program's commit order ((block, node) —
    /// `PalwTirLayoutV1::commit_tiles` verbatim). Replaces the single `tile_len`; each in `1..=2^16`.
    pub commit_tile_len: Vec<u32>,
    /// Elements per `Fixed`-state leaf, per state in declaration order (`PalwTirLayoutV1::state_tiles`).
    pub state_tile_len: Vec<u32>,
    /// Positions per canonical `H` chunk (`PalwTirLayoutV1::h_tile`).
    pub h_chunk: u32,
    pub ceilings: TirCeilingsV1,
}

/// A terminal close's demand in the units it is CARRIED in — never element bytes.
pub struct TirCloseDemandV1 {
    /// Per committed source (commit index): the distinct step leaves read, at that source's own
    /// `commit_tile_len`, and the contiguous runs they form (a run shares one sibling set).
    pub commit_leaves: Vec<(u16, u64 /* leaves */, u64 /* runs */)>,
    /// Per `Fixed` state: the checkpoint leaves the replay reads (at `state_tile_len`) and the
    /// positions replayed (at most `C_j − 1`).
    pub state_leaves: Vec<(u16, u64, u32)>,
    /// Per `Hist` state: the distinct `h_chunk` tiles read.
    pub hist_tiles: Vec<(u16, u64)>,
    /// Per param instance `(param, layer)`: the demanded element ranges `[from, to)`, row-major,
    /// merged and sorted. consensus-core maps them to inventory leaves (rows, 32 KiB pieces).
    pub param_ranges: Vec<(u16, Option<u16>, Vec<(u64, u64)>)>,
    /// Prompt ids read (`pre`'s `Input(0)` inside the cone).
    pub prompt_ids: u64,
}

pub struct ConeV1 {
    // … as today, plus:
    /// The worst tile's demand under the caller's `price`, at `H = W`.
    pub tile_demand: TirCloseDemandV1,
    /// With `h_reductions`: the worst dissection bottom's demand at `H = h_chunk`.
    pub chunk_demand: Option<TirCloseDemandV1>,
}

pub fn tir_admit_v1(
    program_bytes: &[u8],
    inputs: &TirAdmitInputsV1,
    price: &dyn Fn(&TirCloseDemandV1) -> u64,
) -> Result<TirAdmissionV1, TirAdmitError>;
```

**The contract.** (1) *Containment*: for every job the layout admits, every tile of every commit point and
every dissection bottom, the units `eval_demanded` records for it (the court's own close) form a demand
`D` with `price(D) ≤ price(tile_demand)` (resp. `chunk_demand`) — the reported worst is an upper bound
under any monotone `price` (more units never cost less). (2) Deterministic: a consensus value. (3) The
ranking runs inside `max_cone_work`. (4) One run per class: the per-tile ceilings apply at each commit
point's own length (admission's loop over distinct lengths, at most 8, goes away).

**consensus-core's price** (`palw_tir_admission_v1`, the only place bytes are priced): `16,384` (the frame:
the binding with its program referenced, the headers) + the disputed leaf's preimage and opening + Σ commit
leaves × preimage bytes at `commit_tile_len` + Σ runs × `2 · 64 · ⌈log2 L⌉` (a run's two boundary paths;
`L` the deepest job's step-leaf count, capped by the ladder) + the same for state leaves and Hist tiles +
Σ inventory leaves of `param_ranges` × (leaf bytes + 30, the operand header and index) + Σ runs of
consecutive inventory leaves × `2 · 64 · ⌈log2 N_inv⌉` (the parameter openings ride as ONE
`PalwArtifactMultiproofV1`, below) + the prompt as the network's form carries it. v10 refuses `price(terminal) > cap` as `CourtCostExceedsCeiling { what: "IR terminal close
bytes as carried" }` — then exact, not necessary-only. Until this interface lands, admission must bound
the parameter part from the element ranges' row count (every touched row's pieces priced with a full
path), which is exact for vocabulary tiles and an over-count elsewhere.

**Test.** A property test over the corpus: random jobs, random tiles, `build_tir_cone_refutation_v1`'s
borsh length ≤ `price(tile_demand)` and ≤ the cap for every admitted class.

**The box rule is sound, not tight (tir-core, 2026-09-28).** On the 1.5B mirror the attention-scores tile
(block 1, node 103, 128 lanes) box-demands ALL of `W_q` — 2,459,157 element bytes — because the box
over-approximates through `reshape` and `rope_pairs`; the court's own close reads one head's 128 rows. With a
path per leaf that box prices at ≈ 4.4 MB (refused) against a real close of ≈ 0.5 MB. An exact price of the
box therefore refuses the 1.5B class, and an element-granular one (v10 as of F7 34c672bec) is a LOWER bound
that admits uncarriable layouts (the 1.5B at 2,048 logits lanes carries ≈ 5.9 MB with per-leaf paths): an
honest executor of such a class loses its terminal-leaf clock.

**The order (the DAA-2,000 release needs 1–3; 4 is its own later fence, since admission only widens):**

1. *(consensus-core, Phase F)* The IR close carries its parameter openings as ONE `PalwArtifactMultiproofV1`
   (the readiness-V2 format consensus already verifies, `verify_artifact_multiproof_v1`) in place of one
   `PalwArtifactOpeningV1` per leaf: a run of consecutive leaves pays ≤ `2 · depth · 64` bytes of siblings,
   not `depth · 64` per leaf. A logits tile of `T` rows is then ≈ `T × 1,566` bytes (1,024 → ≈ 1.62 MB with
   the frame; 2,048 → ≈ 3.23 MB, over the 3.2 MB cap).
2. *(tir-core)* `tir_admit_v1` reports each cone's worst tile and dissection bottom as `TirCloseDemandV1`
   built from the box rule's own per-source boxes — an upper bound on the court's read set for every tile,
   position, job and value (`Select`: the condition and both operands; `Gather`: the index and one slice per
   index element, location-free).
3. *(tir-core)* v10 prices (2) in carried units under (1) and refuses above the cap. Sound; the 1.5B class is
   admitted at 1,024 logits lanes (the scores tile ≈ 2.53 MB even with all of `W_q`, the logits tile
   ≈ 1.62 MB); the 3B class is refused (its box `W_q` alone is 4.2 MB) until (4).
4. *(later fence)* The tight analysis: per-source unions of at most `N` boxes (exact through `reshape`,
   `rope_pairs`, transpose, slice and broadcast; a deterministic merge above `N`), evaluated per alignment
   class of a commit point's tiles (a layout needing more than a cap of classes is refused: tiles align to the
   head width and `h_tile`), counted in `max_cone_work`; the containment test above plus tightness on the
   corpus (priced ≤ 1.25 × the court's measured close for every commit point).

---

## 3. Phase 0(a): the live GDN `k_heads ≠ v_heads` defect

### 3.1 Where the code assumes `k_heads = v_heads`

`PalwShapeProfileV3` has one `gdn_heads` (`palw_step.rs:459`), which the Qwen3.6 profile sets to the
VALUE head count (`palw_qwen36_profile.rs:1447`, `gdn_heads: g.gdn_v_heads`); the node widths do use
both counts (`:727-735`, `Conv = 2·k_dim + v_dim`). Two different things then read that one field:

* **The head mapping** `kh = vh % k_heads` — engine `misaka-palw-base0/src/qwen36.rs:1239-1242`, plan
  `qwen36_plan.rs:790`, float reference `qwen36_reference.rs:273-277`, court
  `palw_step_refute.rs:3222` (with `k_heads` derived from the ref width there, `:3218`).
* **The state chunk maps' conv geometry** — the v1 name `conv-row=(2*gdn_head_k_dim+gdn_head_v_dim)
  *gdn_heads*4` (`palw_state_chunk_map.rs:1038-1039`) and `gdn_conv_window_bytes_v1`
  (`:1224-1236`); the v2 name's per-head gather `[q:h*k, k:heads*k+h*k, v:2*heads*k+h*v]`
  (`:1109-1111`), used by every composition built on it (hybrid v2/v3/v4, `:212-258`); mirrored by
  the engine's capture `base0_gdn_state_geometry_v1/v2` (`fp_capture.rs:1502, 1784`, row width
  `(2k+v)·heads`) and `conv_head_channels` (`:1743-1747`).

### 3.2 The head order of testnet-12's Qwen3.6 artifact — finding

* testnet-12 has **no** Qwen3.6 class at genesis: the operator removed the held hybrid row on
  2026-09-23 because "every held hybrid attempt fails at prefill" (`params.rs:15256-15265`); its
  genesis rows are two dense Qwen2.5 rows (`:15291`). A hybrid could only arrive by registration
  transaction, and would fail the same way.
* The fleet's Qwen3.6 artifact is `qwen36-convert` output over
  `Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf` (`palw_qwen36_profile.rs:95-110`). The converter
  reads the GGUF and accepts only `general.architecture ∈ {qwen35moe, qwen3moe, qwen35}`
  (`bin/qwen36-convert.rs:236-252`); it reads `ssm.group_count` as `k_heads` and
  `ssm.time_step_rank` as `v_heads` (`:265-268`) and **never permutes heads** (its only permutation is
  the rotary one, `:37-69`).
* llama.cpp's converter writes `qwen35`/`qwen35moe` through `_LinearAttentionVReorderBase`
  (`/Users/wata/Downloads/misaka-palw-runtime/llama.cpp/conversion/qwen.py:438-603`, classes
  `Qwen3_5TextModel`/`Qwen3_5MoeTextModel` `:623-630`), which **reorders the value-head axis from
  HF's grouped order to tiled order** in every value-indexed tensor: the v rows of `in_proj_qkv`,
  the rows of `in_proj_z`, `in_proj_a`, `in_proj_b`, `A_log`, `dt_bias`, the v channels of
  `conv1d`, and the input columns of `out_proj` (`:554-603`; the NVFP4 path `:461-548` likewise).
  This has been so since the architecture existed: it is in the commit that introduced it
  (`fc0fe4004`, 2026-02-11, "models : support qwen3.5 series (#19468)" — "reorder v heads for linear
  attention to avoid expensive interleaved repeat"). llama.cpp's own graphs then use a plain tiled
  `ggml_repeat_4d` (`src/models/qwen35moe.cpp:462-468`, `qwen35.cpp:443-444`), while `qwen3next`,
  whose GGUFs keep grouped order, uses an interleaved repeat (`qwen3next.cpp:519-536`). A `qwen35moe`
  GGUF from any other writer would be garbage in llama.cpp itself, and the pinned file runs there.
* **Therefore the live `vh % k_heads` computes exactly HF's function** (`repeat_interleave`: value
  head `g` reads key head `g // r`) with the value heads relabelled by `t = (g mod r)·k_heads +
  g div r` — for which `t mod k_heads = g div r`. The reference's comment ("the other reading pairs
  every value head with the wrong key", `qwen36_reference.rs:273-276`) is right *for this artifact*.
  The head mapping is **not a live defect**; it is an undocumented convention that a converter from
  HF safetensors would break. The IR makes it explicit: tiling over a V-permuted artifact and grouping
  over HF's weights are two programs, two class ids, one function.
* **The live defect is the conv geometry alone.** At 16/32 with `hd = 128` the engine's window row
  is `2·16·128 + 32·128 = 8,192` lanes; the maps expect `(2·128 + 128)·32 = 12,288`, so capture
  refuses (`ConvIsNotTheGeometrys`, `fp_capture.rs:1872`) — the prefill failure — and were it not
  refused, value head `h ≥ 16` would gather its q from the k region and its k and v from offsets
  `32·128` too far. The per-head slice SIZE `(2·hd_k + hd_v)·4·kernel` (`gdn_conv_head_slice_bytes_v2`,
  `:1141-1151`) is right at any ratio (a head's window is `q_kh, k_kh, v_h`), and the delta half is
  per value head, so pricing and the anchored `S` replay are correct. Qwen3.8-27B (16/48,
  `palw_qwen36_profile.rs:228-252`) is the same case (`fp_recompute.rs:1288-1306` records the refusal).

### 3.3 Minimal live fix

1. **Profile version 3** (`PALW_STEP_OBJECT_VERSION_V3`): V1's layer phase, and "the GDN key-head
   count is `k_heads = width(GatedDeltaNet.ref0) / gdn_head_k_dim`" (the derivation the court already
   uses, `palw_step_refute.rs:3218-3222`, made one function `palw_gdn_key_heads_v1`, which also
   requires `gdn_heads % k_heads == 0` and the conv node's width `= 2·k_heads·hd_k + gdn_heads·hd_v`).
   A version, not a field, for the precedent's reason (`palw_step.rs:38-40`: a field moves every
   profile id). Older builds refuse V3 in `validate_shape` (`palw_step.rs:587`), so before the fence
   an old and a new build agree: nobody admits it.
2. **New map names that spell the key-head count**: `PALW_GDN_STATE_CHUNK_MAP_NAME_V3` =
   `conv-head-row=(2*gdn_head_k_dim+gdn_head_v_dim)*4/conv-head-gather=[q:(h%kh)*k,k:kh*k+(h%kh)*k,
   v:2*kh*k+h*v]/window-row=(2*kh*k+heads*v)*4/kh=width(gdn.ref0)/k` and the held composition
   `palw_hybrid_state_chunk_map_name_v5` (attn v4 + gdn v3), recognised by `palw_map_is_held_v4`'s
   successor; the dispatch functions (`gdn_state_terms_for_map_v1`, the chunk-count and layout
   functions, `palw_state_chunk_count_at_v1` `:989-1016`, `palw_state_layout_v4` `:461-481`) gain the v3/v5 arms. Existing ids keep their meaning.
3. **Engine**: `base0_gdn_state_geometry_v3` and `conv_head_channels_v3` taking `k_heads`
   (`fp_capture.rs`), capture and restore under the v5 map; `palw_qwen36_context_row_profile_v8(n_ctx)`
   = v7 at version 3 with the v5 map. The head mapping, the reference and the court's GdnStep arm are
   unchanged (correct, §3.2); the converter records `v_head_order = tiled (llama.cpp qwen35
   V-reorder)` in the artifact metadata.
4. **Admission gate**: `verify_class_admission_v9` gains `gdn_key_heads: bool`; a V3 profile or a
   v3/v5 map without it is `GdnKeyHeadsNeedsItsFence`; past it (hygiene) a V1/V2 profile with
   `k_heads ≠ gdn_heads` and a recurrence map is refused (`GdnMapAssumesEqualHeads`) — such a class
   can never produce. Genesis: `validate_palw_v2` refuses a genesis V3 row unless armed at 0.
5. **Fence** `palw_gdn_key_heads: Option<ForkActivation>` — a bare fence, Some-only in both ids,
   `never()` collapse, `palw_fences_v1`, probe, processor accessor; on testnet-12 an entry in the first
   flag-day list after DAA 1,500 at an unused height. No kernel is added, so `court_catalog_root` does
   not move.
6. **Fixtures** (`misaka-palw-base0/src/fuzz_qwen36.rs`): tiny hybrids at **16/32 and 16/48** heads
   (head dim 4, hidden 64, four layers at interval 4, tiny experts), plus the existing 2/4 (`qwen36_dev_
   fixture`). **End to end:** (a) capture succeeds at every checkpoint and `resumed == fresh` at every
   covered position (the `fp_recompute.rs:1308` test shape, now green on k≠v); (b) the anchored
   GdnStep refutation convicts a one-lane fault and acquits the honest tile, for a head `< 16` and a
   head `≥ 16`; (c) the SsmConv arm (`qwen36_conv_slice_v1`, geometry-agnostic) likewise; (d) the
   ADR-0103 checkpoint court on the hybrid's attention rows; (e) a processor test: registration below
   the fence refused by name with the block standing, above it admitted, produced, disputed and
   closed; (f) a salted t12 drill crossing the fence on the shipping binary. The fleet's 512 artifact
   then registers `graph-v8@512` by transaction (its inventory root re-derived for the new profile).

---

## 4. Work plan

Estimates are agent-days of one implementation agent, review included; the lead integrates.

| Step | Work | Exit test | Before `tir_admit_v1`? | Est. |
| --- | --- | --- | --- | --- |
| **P0a** | §3.3 in full | fixtures (a)–(e) green; drill (f) crosses on the shipping binary | yes (independent) | 8 |
| **F1** | fence `palw_tir_v1` + value type + all fingerprint sites + validation + probe + drill mover | every shipped preset's params/identity/schedule ids equal the pinned pre-change values; probe and fork-id tests name the fence; each `validate_palw_v2` refusal fires | yes | 2 |
| **F2** | `PalwTirClassV1`/layout/class id; `ClassRegisteredTirV1` appended; may-ride; processor pre-fence drop; registration message and signature | a block carrying it folds byte-identically to a block without it below the fence (state root, fees, skipped list), and the old enum fails to decode it (A-2 path); past the fence it reaches a v10 stub | yes | 3 |
| **F3** | TIR inventory, `PALWTIR1` container, manifest; converter legacy A16 `.palwart` → TIR artifact (triples split into `m`,`s`,`z`) | streamed root = materialised root; openings verify; converted tensors equal the legacy codes and triples | yes | 4 |
| **F4** | step space (§2.5): enumeration, closed forms, coordinates, leg builder, execution root; `post` rule | closed form = walk for the five corpus programs and a sweep of jobs; the first-divergence property (every leaf's operand units precede it) over all leaves of those programs | yes | 5 |
| **F5** | demand evaluator (in the crate) + `TirCone` arm + PALW-TIR-33 + logits consistency + state replay | for the five corpus programs × every commit point × tile × several positions: honest → `ChallengerDefeated`; every single-lane forgery → `ExecutorGuilty`; every out-of-interval forgery → TIR-33; demanded values = `eval_cone` values; hostile objects total (no panic) | yes (intervals exist at Gate 1; costs stubbed) | 8 |
| **F6** | admission v10 wiring, `tir_classes`, registry work, processor arm, preflight | corpus programs admitted; the mutation corpus refused by name; registration CPU within the DoS budget; processor e2e registration past the fence | **no** | 4 |
| **F7** | generic H dissection: objects, `tir_dissections`, clock, fold, bottom | long-`H` attention cones: honest acquitted; a lie in each reduction convicted at the fold or the bottom; silence loses; window admission | partly (needs the dissected set) | 7 |
| **F8** | structural work vector + registry work | re-tiling/re-committing invariance; vectors for the corpus; IR-vs-legacy Qwen2.5-A16 comparison reported | yes | 3 |
| **F9** | `TirLineageV1`, generic backend, evidence builders, responder, resume | `check_lineage_v1` passes; backend = reference on golden vectors, fuzzed programs and corpus runs; `e2e_drill` issues a TIR family certificate on a tiny class | yes | 10 |
| **F10** | `palw-class check-architecture` (IR + legacy modes) | verdicts on tir-lower's 57 HF tiny fixtures; legacy mode equals processor admission on the shipped lineages | legacy mode yes | 3 |
| **F11** | drills D-F1…D-F4 (below) | the RFC Phase F exit gate | no | 8 |

Total ≈ 65 agent-days: with two implementation agents in parallel (A: F2–F5, F7; B: P0a, F3, F8,
F9, F10) and the lead on F1/F6/F11, about **7 calendar weeks**, consistent with the RFC's 6–8. The
critical path is F5 → F7 → F11, and `tir_admit_v1` gates F6.

**Drills (the exit gate).**

* **D-F1 — Qwen2.5-A16 as an IR program, same weights, same logits.** The IR program is written from
  tir/core's A16 segments (legacy-mirror conformance) for the testnet-12 dense row `graph-v7@8192`;
  the artifact is the F3 conversion of the genesis 8k artifact. Offline first (`palw-tir-equiv`: for
  the canonical job and 32 random prompts the IR logits rows equal the legacy engine's byte for byte,
  and each IR commit point that corresponds to a legacy node row is equal); then on a salted t12
  drill chain past the fence: registration → Candidate → readiness → Active → claims → `Final`.
* **D-F2 — court battery.** On a tiny IR class and on the drill class: planted faults at every commit
  point class (carry-outs, `TopK`, `HistAppend` rows, state leaves, Hist tiles, logits, a dissected
  attention output) convicted; honest challenges defeated; out-of-interval commitments convicted by
  TIR-33; the dissection both ways; the TIR family certificate.
* **D-F3 — forged-output red-team 8/8.** Commit the 2026-09-07 harness (8 attacks: raw skeleton,
  nonce grind, algo downgrade, forged commitment, tampered state root, fat coinbase, inflated
  position, insider well-formed envelope) and run it with the producer on the IR class: the same eight
  rejections, by the same gates, as for the legacy class; no node panics.
* **D-F4 — fence crossing on the shipping binary.** The salted t12 drill with the release's list
  moved low (`--palw-drill-fenceN-at`): below the fence an IR registration is dropped by the new
  binary and skipped by the old one with identical tips; blocks validated below and above; the clock
  gate (DAA advances past the height); an old-binary peer refused by the fork id past the height; the
  listing walk and a court close past it. The binary is the one that ships (memory rules).

**Risks.**

1. **A16 attention as IR segments** (RFC OQ2). The Gate 1 conformance list has no attention
   segment (`misaka-palw-tir/tests/legacy_mirror.rs`); if `AttnFused`'s requantised probabilities
   cannot be expressed byte-identically, D-F1's "same logits" fails and must be resolved with
   tir/core before F11.
2. **Generic backend speed.** The Gate 1 evaluator holds every element as `i128` and clones params;
   a 1.5B model needs the typed backend (F9) to run D-F1 at all.
3. **The demand evaluator is consensus-critical.** Mitigations: differential tests against
   `eval_cone`, an implementation in ref2, and the hostile-input totality sweep
   (`rcore/hf-court-total`'s pattern).
4. **Range proofs on real programs** may need lowerer-inserted "clamps that never fire" (04b §7);
   a program that cannot be proved is refused, which is safe but may block D-F1 until the templates
   carry them.
5. **Close size.** Carrying the program costs ≈1 chunk per close on testnet-12's 27-chunk court; on
   devnet's one-carrier court (81,920 bytes) an IR close does not fit — the drills run on the salted
   t12 ruleset. If closes bind later, the program can be stored in `tir_classes` instead (state
   growth ≤ 88 KB per class).
6. **Operand-unit ceiling.** The legacy `max_operand_count = 8` counts tensors; a TIR cone may read
   more (q, K tile, V tile, scales, tables). Lowerers add commit points to cut cones; if that is not
   enough, the ceiling is re-expressed in bytes for IR classes (admission-time, IR-only).
7. **Rulesets without A-2** cannot arm the fence; testnet-12 declares `palw_audit_2026_09_11`, older
   networks would need it first.

---

## 5. Decisions needed

From the lead (design) or the user (network):

1. **(lead)** D3 inline carriage with a one-carrier ceiling (`max_program_bytes` ≈ 88,000 on t12 v1),
   no chunked lane in v1.
2. **(lead)** D5 declared layout inside the class id (vs a canonical derivation).
3. **(lead, tir/core)** D7: Fixed-state checkpoints and Hist tiles as step leaves, no checkpoint leg
   for IR classes — changes spec 04b §10.1's wording.
4. **(lead, tir/core)** the `post`-is-effect-free normal-form rule.
5. **(lead)** who writes `eval_demanded`/`eval_h_range` (proposed: Phase F writes, tir/core reviews,
   ref2 implements independently).
6. **(user)** RFC OQ5 — weight for IR classes: recommended = a TIR family certified once for the
   primitive set (drilled end to end, including a dissection and a state replay; certified on chain
   by `FamilyCertified`, so `court_e2e_root` never moves) + the registry's existing per-class
   readiness (which is what proves responders exist, ADR-0093's gap). No per-program drill.
7. **(user)** RFC OQ6 — generic-backend throughput floor: recommended not a consensus condition in
   v1; D-F1 measures generic vs legacy throughput and the number is published.
8. **(user)** RFC OQ7 — normalization: recommended = reductions feeding `IntRsqrt` (structural);
   alternative = fold into `other_verified_ops`.
9. **(user)** Phase 0(a): profile V3 + new maps + `palw_gdn_key_heads`, in the first flag-day list
   after DAA 1,500; its height.
10. **(user)** the `palw_tir_v1` height on testnet-12 (after D-F4 passes) and whether it shares a
    flag day with other fences (only at an unused height).
11. **(lead)** the t12 v1 ceilings (`max_program_bytes`, `max_unrolled_nodes`, `max_context`,
    `max_macs_per_position`, `max_state_bytes`, `max_peak_live_bytes`) — proposed after F6 measures
    the corpus. **Decided (F6, 2026-09-28)** from the Qwen2.5 A16 decoders as IR programs at the
    small history bound (`tir_a16_admission_measure.rs`; 1.5B: 12,928 B, 7,762 unrolled nodes,
    24.1 G MACs a position at `H = W`, 7.5 GB state, 549 MB peak live, 838 work; 3B: 9,970 nodes,
    41.7 G MACs, 9.7 GB): 88,000 B; `2^16` nodes; context `2^18`; `2^37` MACs; `2^35` state bytes;
    `2^32` peak live bytes; `2^16` admission work (`PALW_T12_TIR_CEILINGS_V1`).
12. **(lead)** the free-prompt lane and ADR-0133 segment claims for IR classes stay closed (refused
    by name) until Phase H; Phase F certifies the attempt lane, panels and the court.

---

## Appendix A — object sketches (borsh; appended variants only)

```rust
// PalwConsensusObjectV2, appended after the last variant:
ClassRegisteredTirV1 {
    class_id: Hash64,                 // tir_class_id_v1(..)
    artifact_root: Hash64,            // TIR inventory root
    slash_value_per_pwu: u64,
    pwu_rule: PalwPwuRuleV2,          // DerivedV1 { pwu_per_inference = counted IR leaves }
    initial_target: u128,
    share_permille: u16,
    activation_daa: u64,
    admission: Box<PalwTirAdmissionCarriageV1>,
},
CourtTirRootClaimed { session_id: Hash64, root: PalwTirRootClaimV1, arity: u8, signature: Vec<u8> },
CourtTirDissected   { session_id: Hash64, round: u8, children: Vec<PalwTirRangeClaimV1>, signature: Vec<u8> },
CourtTirChildChosen { session_id: Hash64, round: u8, child: u8, signature: Vec<u8> },

pub struct PalwTirAdmissionCarriageV1 {
    pub class: PalwTirClassV1,        // program bytes, layout, tokenizer id
    pub canonical: PalwJobContextV2,
    pub registrant_bond: PalwBondKeyV2,
    pub signature: Vec<u8>,           // ML-DSA-87 over palw_tir_class_registration_message_v1
}

// PalwCourtVerdictProofV2, appended (as built in F5; TirDissection with F7):
TirCone { refutation: Box<PalwTirConeRefutationV1> },                                    // 5
TirLogits { accusation: Box<PalwTirLogitsConsistencyV1> },                              // 6
TirDecodeTokenTiled { binding: Box<PalwTirStepBindingV1>, pin: PalwTiledDecodePinV1 },   // 7
TirDecodeToken { binding: Box<PalwTirStepBindingV1>, pin: PalwBase0DecodeTokensV1, position: u32 }, // 8
TirDissection { binding: Box<PalwTirStepBindingV1>, bottom: Box<PalwTirDissectBottomV1>,
                operand_openings: Vec<PalwArtifactOpeningV1> },

pub struct PalwTirStepBindingV1 {
    pub version: u16,
    pub job_context: PalwJobContextV2,          // shape_profile_id = the IR class id
    pub class: PalwTirClassV1,                  // re-hashed to the class id every time
    pub artifact_root: Hash64,
    pub full_logits_trace_root: Hash64,
    pub step_leaf_count: u64,
    pub step_merkle_root: Hash64,
    pub committed_execution_root: Hash64,       // palw_tir_execution_root_v1
}
pub struct PalwTirConeRefutationV1 {
    pub binding: PalwTirStepBindingV1,
    pub output_opening: PalwStepOpeningV1,
    pub output_preimage: PalwStepTileLeafV1,
    pub operands: PalwStepInputRowV1,           // every step leaf read, ascending, range-proved
    pub params: Vec<PalwArtifactOpeningV1>,     // every inventory leaf read, ascending
    pub prompt_token_ids: Vec<u32>,             // flat form: whole, iff a prompt id is read
    pub prompt_ids_openings: Vec<PalwPromptIdsOpeningV1>, // Merkle form: the tiles read
    pub decode_tokens: Option<PalwDecodeTokenPinV1>,      // iff a generated id is read
}
pub struct PalwTirLogitsConsistencyV1 {
    pub binding: PalwTirStepBindingV1,
    pub step_opening: PalwStepOpeningV1,        // a tile of the logits node, a >= P - 1
    pub step_preimage: PalwStepTileLeafV1,
    pub trace: PalwTirTraceLanesV1,             // Flat(pin) | Tiled { ids, row root + opening, tile + opening }
}
pub struct PalwTirRootClaimV1 { pub totals: Vec<Vec<i128>> }        // per H-reduction, demanded elements
pub struct PalwTirRangeClaimV1 { pub partials: Vec<Vec<i128>> }

// PalwStepFaultV1, appended: TirValueOutsideProvenInterval { value_index } = 19,
//                            TirLogitsTraceMismatch { value_index } = 20 (evidence kind 7).
// New state tables (rooted only once written): tir_classes, tir_dissections.
```
