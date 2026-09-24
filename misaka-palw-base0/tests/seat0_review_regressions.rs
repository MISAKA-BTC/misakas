//! **SEAT-0's review, kept as regressions.** Each test was a probe that FAILED against the first
//! SEAT-0 diff, or pins a liveness fact the review relied on:
//!
//! * `legacy_*` — the rows-only legacy decode of the A16 / Qwen3.6 `verify_material` re-derived its
//!   roots from SERVED rows with no execution behind them, so on a court-capable backend every new
//!   set of rows was a new root the seat licensed. It is the ledger-compiled class's alone now.
//! * `fold_attempt_*` — an attempt served as a fold: SEAT-0's head rule reads dense leaves only, so
//!   the fold's selecting row was free and every bend a fresh root. On a class whose attempts keep
//!   their tiles no producer serves one; refused. On this line a HELD class's attempt folds, so its
//!   fold stays licensable by material and its selecting row is a residual there; the seat's replay
//!   (the only full-seat route under SEAT-R) reproduces none of the bent roots (`held_fold_attempt_*`).
//! * `crafted_profile_*` — a stranger's profile whose node count overflows `u32` panicked inside the
//!   head rule (`overflow-checks = true`, and the node's panic hook exits). The floor now checks
//!   the class profile id as the other tiers do, and the rules count the profile checked first.
//! * `head_geometry_*`, `liveness_*` — every live head is one SEAT-0 knows, and the graph versions
//!   `seat_material_binds_the_claim` does not run keep their honest verdicts.

use std::sync::Arc;

use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step::{PALW_STEP_MAX_LEAVES, PalwShapeProfileV3};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_MAX_LEAVES, PalwStepBindingV2, checkpoint_leg_root_v2, execution_commitment_root_v2, step_leg_root_v1,
    verify_binding_v1,
};
use kaspa_consensus_core::palw_step_refute::{base0_decode_token_select_v1, base0_logits_trace_root_v1, tiled_logits_trace_root_v1};
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::backend::Base0Backend;
use misaka_palw_base0::produce::{
    Base0FpMaterialV2, Base0RetainedMaterialV1, PALW_BASE0_FP_MATERIAL_MAGIC_V2, base0_activation_leg_root_v1,
    base0_dense_step_root_capped_v1, base0_fp_material_decode_v2, base0_fp_material_encode_v2, base0_logits_head_v1,
    base0_material_decode_v1,
};
use misaka_palw_base0::qwen25_a16_backend::{
    Qwen25A16Backend, Qwen25A16RunV1, a16_court_capable_v1, a16_execute_free_prompt_streaming_v1, qwen25_a16_material_encode_v1,
    qwen25_a16_roots_v1,
};
use misaka_palw_base0::qwen36_backend::{Qwen36Backend, Qwen36RunV1, qwen36_material_encode_v1, qwen36_roots_v1};

const NETWORK: &[u8] = b"misaka-palw-rc";

fn rebind(b: &mut PalwStepBindingV2) {
    let ctx_hash = b.job_context.context_hash();
    let profile_hash = b.shape_profile.shape_profile_id();
    let decode_calls = b.job_context.exact_decode_tokens.saturating_sub(1);
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, b.step_leaf_count, &b.step_merkle_root);
    let checkpoint_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &b.checkpoint_profile.profile_hash(),
        &b.state_chunk_map_id,
        decode_calls,
        b.checkpoint_count,
        &b.checkpoint_merkle_root,
    );
    b.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &b.full_logits_trace_root, &b.activation_leg_root, &checkpoint_root, &step_root);
    assert!(verify_binding_v1(b).is_ok());
}

fn a16_geometry(vocab: u32) -> kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
    kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 32,
        ffn_dim: 64,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 8,
        vocab_size: vocab,
        n_ctx: 128,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    }
}

fn a16_artifact(vocab: u32) -> Arc<Base0ArtifactV1> {
    let g = a16_geometry(vocab);
    let shape = Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: g.rms_eps_q,
    };
    Arc::new(
        Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(misaka_palw_base0::engine_a16::derived_a16_store(&shape))
            .expect("sorted and unique"),
    )
}

fn a16_held_backend(artifact: &Arc<Base0ArtifactV1>) -> (Qwen25A16Backend, PalwShapeProfileV3) {
    use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_held_canonical_v1, qwen25_a16_profile_v7};
    let profile = qwen25_a16_profile_v7(a16_geometry(artifact.shape.vocab as u32)).expect("held v7");
    let backend =
        Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), qwen25_a16_held_canonical_v1(profile.n_ctx))
            .expect("servable")
            .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
            .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1);
    (backend, profile)
}

/// Free rows: no execution, just numbers. `seed` picks them.
fn free_rows(prefill: u32, decode: u32, vocab: usize, seed: u64) -> (Vec<Vec<i32>>, Vec<u32>) {
    let mut rows: Vec<Vec<i32>> = (0..prefill.saturating_sub(1)).map(|_| Vec::new()).collect();
    let mut ids = Vec::new();
    for r in 0..decode as u64 {
        let row: Vec<i32> = (0..vocab as u64)
            .map(|i| ((i.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed.wrapping_add(r).wrapping_mul(0xBF58_476D_1CE4_E5B9)) >> 40) as i32)
            .collect();
        ids.push(base0_decode_token_select_v1(&row) as u32);
        rows.push(row);
    }
    (rows, ids)
}

// ------------------------------------------------------------------------------------------------
// F-A: the legacy rows-only decode is a SEAT-0 bypass on court-capable model classes
// ------------------------------------------------------------------------------------------------

#[test]
fn legacy_rows_only_material_is_not_licensed_on_a_court_capable_a16_class() {
    let artifact = a16_artifact(8_292);
    let (backend, profile) = a16_held_backend(&artifact);
    assert!(a16_court_capable_v1(&profile), "the held v7 row is court-capable: its honest producer writes binding material");
    let anchor = Hash64::from_u64_word(0x1E6A_C7);
    let mut licensed_roots = Vec::new();
    for draw in [true, false] {
        let (job, _) = backend.job_for_anchor(anchor).expect("job");
        let job = palw_attempt_job_v1(job, draw);
        for seed in [1u64, 2, 3] {
            let (logits_rows, generated) = free_rows(job.declared_prefill_tokens, job.exact_decode_tokens, 8_292, seed);
            let run = Qwen25A16RunV1 { logits_rows, generated };
            let (trace_root, _, execution_root, _) = qwen25_a16_roots_v1(&job, artifact.artifact_digest(), &run).expect("roots");
            let bytes = qwen25_a16_material_encode_v1(&run);
            let verdict =
                backend.verify_material(&bytes, PalwClaimRootsV1 { execution_root, trace_root, anchor, attempt_draw: Some(draw) });
            eprintln!("A16 held v7 legacy draw={draw} seed={seed}: {} bytes, verdict {verdict:?}", bytes.len());
            if verdict == PalwMaterialVerdictV1::Matches {
                licensed_roots.push(execution_root);
            }
        }
    }
    licensed_roots.dedup();
    assert!(
        licensed_roots.is_empty(),
        "{} distinct execution roots over FREE rows (no execution) were licensed on a court-capable class — the grind SEAT-0 closes is open here",
        licensed_roots.len()
    );
}

#[test]
fn legacy_rows_only_material_is_not_licensed_on_a_registered_qwen36_class() {
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v7};
    let geometry = PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: 32,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: 2,
        gdn_v_heads: 4,
        gdn_head_dim: 8,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: 8,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    };
    let artifact = Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8));
    let profile = qwen36_profile_v7(geometry).expect("held v7");
    let backend = Qwen36Backend::from_registered_profile(artifact, NETWORK.to_vec(), profile, (3, 4))
        .expect("servable")
        .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1);
    assert!(backend.supports_court(), "a registered Qwen3.6 class commits binding material");
    let anchor = Hash64::from_u64_word(0x36_1E6A);
    let mut licensed = 0;
    for draw in [true, false] {
        let (job, _) = backend.job_for_anchor(anchor).expect("job");
        let job = palw_attempt_job_v1(job, draw);
        for seed in [7u64, 8] {
            let (logits_rows, generated) = free_rows(job.declared_prefill_tokens, job.exact_decode_tokens, 64, seed);
            let run = Qwen36RunV1 { logits_rows, generated };
            let (trace_root, _, execution_root, _) = qwen36_roots_v1(&job, backend.shape_id(), &run).expect("roots");
            let bytes = qwen36_material_encode_v1(&run);
            let verdict =
                backend.verify_material(&bytes, PalwClaimRootsV1 { execution_root, trace_root, anchor, attempt_draw: Some(draw) });
            eprintln!("Qwen3.6 held v7 legacy draw={draw} seed={seed}: verdict {verdict:?}");
            licensed += usize::from(verdict == PalwMaterialVerdictV1::Matches);
        }
    }
    assert_eq!(licensed, 0, "free-row legacy material licensed on a registered (court-capable) Qwen3.6 class");
}

// ------------------------------------------------------------------------------------------------
// F-B: an attempt served as a fold — rule 5 is dense-only, so the selecting row is free
// ------------------------------------------------------------------------------------------------

/// What [`bend_a_fold_attempt`] saw: the honest fold's verdict, every bent `(execution, trace)` root
/// pair with the material verdict it got, and the roots the seat's own replay (`execute_for_verdict`,
/// the route SEAT-R leaves a full seat) reproduces for the same job.
struct BentFoldAttempt {
    honest: PalwMaterialVerdictV1,
    honest_roots: (Hash64, Hash64),
    bent: Vec<((Hash64, Hash64), PalwMaterialVerdictV1)>,
    replay: (Hash64, Hash64),
}

impl BentFoldAttempt {
    fn licensed_by_material(&self) -> Vec<Hash64> {
        self.bent.iter().filter(|(_, v)| *v == PalwMaterialVerdictV1::Matches).map(|((root, _), _)| *root).collect()
    }
}

/// One honest execution of an attempt job through the fold sink, served as a fold under the
/// attempt claim, then its selecting row bent four ways (token moved, trace root and binding
/// re-derived), and the same job replayed as a seat replays it.
fn bend_a_fold_attempt(profile: PalwShapeProfileV3) -> BentFoldAttempt {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1;
    let artifact = a16_artifact(8_292);
    let backend =
        Qwen25A16Backend::new(artifact.clone(), NETWORK.to_vec(), profile.clone(), qwen25_a16_held_canonical_v1(profile.n_ctx))
            .expect("servable")
            .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
            .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1);
    let plan = misaka_palw_base0::engine_a16::A16Engine::new(&artifact).expect("a16").plan_from_profile(&profile).expect("plan");
    let anchor = Hash64::from_u64_word(0xF01D_BE17);
    let draw = true; // the live rule past the prefill-draw fence: D = 1, the only row is the selecting row
    let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
    let job = palw_attempt_job_v1(job, draw);
    let run =
        a16_execute_free_prompt_streaming_v1(&artifact, &profile, Some(&plan), &job, &prompt, backend.step_ladder_cap(), &mut |_| {})
            .expect("one honest execution");
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let honest = base0_fp_material_decode_v2(&base0_fp_material_encode_v2(&run, &ids).expect("fold")).expect("decodes");
    let honest_verdict = backend.verify_material(
        &base0_fp_material_encode_v2(&run, &ids).unwrap(),
        PalwClaimRootsV1 { execution_root: run.execution_root, trace_root: run.trace_root, anchor, attempt_draw: Some(draw) },
    );
    let replay = backend.execute_for_verdict(&job, &prompt).expect("the seat's replay runs");
    let mut bent = Vec::new();
    for bend in 1..=4usize {
        let mut m: Base0FpMaterialV2 = honest.clone();
        let last = m.logits_rows.len() - 1;
        let row = &mut m.logits_rows[last];
        let a = base0_decode_token_select_v1(row);
        let k = (a + bend * 97) % row.len();
        row[k] = row[a] + 1;
        m.generated_token_ids[last] = k as u32;
        m.binding.full_logits_trace_root =
            tiled_logits_trace_root_v1(&m.binding.job_context, &m.logits_rows, &m.generated_token_ids).expect("tiled");
        rebind(&mut m.binding);
        let mut bytes = PALW_BASE0_FP_MATERIAL_MAGIC_V2.to_vec();
        bytes.extend_from_slice(&borsh::to_vec(&m).unwrap());
        let roots = PalwClaimRootsV1 {
            execution_root: m.binding.committed_execution_root,
            trace_root: m.binding.full_logits_trace_root,
            anchor,
            attempt_draw: Some(draw),
        };
        let verdict = backend.verify_material(&bytes, roots);
        eprintln!(
            "fold attempt bend #{bend}: token {a} -> {k}, root {}…, verdict {verdict:?}",
            &roots.execution_root.to_string()[..16]
        );
        bent.push(((roots.execution_root, roots.trace_root), verdict));
    }
    BentFoldAttempt {
        honest: honest_verdict,
        honest_roots: (run.execution_root, run.trace_root),
        bent,
        replay: (replay.execution_root, replay.trace_root),
    }
}

/// A class whose attempts keep their tiles (`palw_attempt_capture_folds_v1` false): no producer
/// serves its attempt as a fold, so the seat refuses one — the honest fold included — and no bend
/// of its selecting row is licensed.
#[test]
fn fold_attempt_bent_row_is_not_licensed() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2;
    let profile = qwen25_a16_profile_v2(a16_geometry(8_292)).expect("v2");
    assert!(!kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1(&profile));
    let seen = bend_a_fold_attempt(profile);
    assert_eq!(seen.honest, PalwMaterialVerdictV1::Mismatch, "no producer serves this class's attempt as a fold");
    let licensed = seen.licensed_by_material();
    assert!(
        licensed.is_empty(),
        "{} distinct execution roots from ONE execution were licensed by re-bending the selecting row of a fold attempt",
        licensed.len()
    );
}

/// **The launch line's residual, and what closes it.** A HELD class's attempt folds here, so its
/// fold is the honest producer's material and the seat licenses it — and SEAT-0's head rule reads
/// dense leaves only, so the selecting row of that fold is not tied to its step tree: through the
/// MATERIAL route one execution still licenses a fresh execution root per bend (asserted here, so a
/// change to it is seen). What closes it is SEAT-R (the peer's F4, in the same binary as F2's
/// `palw_offence_attribution`): a full seat signs a full-mask `Valid` only from its replay, and
/// every door needs that seat (full 1 + partial 4, one holder a segment). The replay reproduces the
/// honest roots and none of the bent ones — `replay_licenses_v1` compares both roots, and the
/// execution root commits the logits trace root — so no bend is licensed there.
#[test]
fn held_fold_attempt_bent_rows_are_licensed_by_material_and_refused_by_the_replay() {
    use kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7;
    let profile = qwen25_a16_profile_v7(a16_geometry(8_292)).expect("v7");
    assert!(kaspa_consensus_core::palw_resource_profile_v1::palw_attempt_capture_folds_v1(&profile));
    let seen = bend_a_fold_attempt(profile);
    assert_eq!(seen.honest, PalwMaterialVerdictV1::Matches, "the honest held fold attempt is licensed");
    assert_eq!(seen.licensed_by_material().len(), seen.bent.len(), "the material route's residual: every bend is licensed");
    assert_eq!(seen.replay, seen.honest_roots, "the seat's replay reproduces the honest roots");
    for ((execution_root, trace_root), _) in &seen.bent {
        assert_ne!(*execution_root, seen.replay.0, "a bent execution root is not the replay's");
        assert_ne!(*trace_root, seen.replay.1, "a bent trace root is not the replay's");
    }
}

// ------------------------------------------------------------------------------------------------
// A stranger's profile — overflow-checks = true in [profile.release]
// ------------------------------------------------------------------------------------------------

fn floor_backend() -> Base0Backend {
    use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
    let court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 4, 2).expect("court");
    let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("floor");
    let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("root");
    Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("resolves"))
        .with_step_ladder_cap(court.max_step_leaf_count())
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
}

#[test]
fn crafted_profile_is_refused_not_a_panic_on_the_floor_fp_route() {
    let backend = floor_backend();
    let anchor = Hash64::from_u64_word(0xC0FF_EE);
    let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
    let job = palw_attempt_job_v1(job, false);
    let out = backend.execute(&job, &prompt).expect("runs");
    let (mut binding, tiles, rows, ids, chunks): Base0RetainedMaterialV1 = base0_material_decode_v1(&out.material).expect("dense");
    // The stranger's profile: every layer attention, 65,535 layers × 65,538 attention nodes ⇒
    // `global_node_count` passes u32::MAX. The post table (the head) is the floor's own.
    let mut node = binding.shape_profile.attn_nodes[0].clone();
    node.weight_name.clear();
    let profile = &mut binding.shape_profile;
    profile.layer_count = u16::MAX;
    profile.full_attention_interval = 1;
    profile.attn_nodes = vec![node; 65_538];
    binding.job_context.shape_profile_id = binding.shape_profile.shape_profile_id();
    binding.activation_leg_root = base0_activation_leg_root_v1(&binding.job_context);
    binding.step_merkle_root = base0_dense_step_root_capped_v1(&binding, &tiles, PALW_STEP_MAX_LEAVES).expect("root");
    binding.full_logits_trace_root = base0_logits_trace_root_v1(&binding.job_context, &rows, &ids);
    rebind(&mut binding);
    let bytes = borsh::to_vec(&(&binding, &tiles, &rows, &ids, &chunks)).unwrap();
    eprintln!("crafted floor material: {} bytes (gossip cap 16 MiB)", bytes.len());
    // An FP-lane claim on the floor: the seat checks only `job_id == anchor`, and the claim's roots
    // are the filer's own (the chain relates an FP execution root to nothing).
    let roots = PalwClaimRootsV1 {
        execution_root: binding.committed_execution_root,
        trace_root: binding.full_logits_trace_root,
        anchor: binding.job_context.job_id,
        attempt_draw: None,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| backend.verify_material(&bytes, roots)));
    eprintln!("verify_material on the crafted profile: {result:?}");
    assert_eq!(
        result.expect("SEAT-0 panics inside verify_material on a stranger's profile (a dead panel task)"),
        PalwMaterialVerdictV1::Unverifiable,
        "a capture for another profile is not the floor's to vouch for"
    );
    // The backstop, for a caller that reaches the rules without the profile gate: rule 0 counts the
    // profile checked before anything sums it.
    let rules = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        misaka_palw_base0::produce::base0_seat_rules_v1(
            &binding,
            &rows,
            &ids,
            None,
            misaka_palw_base0::produce::Base0SeatFamilyV1::IntegerKv,
        )
    }));
    assert_eq!(
        rules.expect("the rules panic on a stranger's profile"),
        Err(misaka_palw_base0::produce::Base0SeatRefusalV1::NodeCountOverflows)
    );
    assert_eq!(misaka_palw_base0::produce::base0_global_node_count_checked_v1(&binding.shape_profile), None);
    assert_eq!(
        misaka_palw_base0::produce::base0_global_node_count_checked_v1(backend.profile()),
        Some(backend.profile().global_node_count()),
        "the checked count is the profile's own count wherever that one does not overflow"
    );
}

// ------------------------------------------------------------------------------------------------
// Coverage print: head tile raggedness of the live classes
// ------------------------------------------------------------------------------------------------

#[test]
fn head_geometry_of_the_live_classes() {
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
    use kaspa_consensus_core::palw_qwen36_profile::{
        PalwQwen36GeometryV1, QWEN36_35B_A3B, qwen36_geometry_artifact_eps, qwen36_profile_v7,
    };
    let floor = floor_backend().profile().clone();
    let mut rows = vec![("floor".to_string(), floor)];
    for n_ctx in [8_192u32, 2_097_152] {
        rows.push((
            format!("A16 held v7 @{n_ctx}"),
            qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }).unwrap(),
        ));
    }
    rows.push((
        "Qwen3.6 held v7 @512".to_string(),
        qwen36_profile_v7(qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 { n_ctx: 512, ..QWEN36_35B_A3B })).unwrap(),
    ));
    for (label, p) in rows {
        let head = base0_logits_head_v1(&p).expect("known head");
        eprintln!(
            "{label}: vocab {} head tile_len {} tiles {} ragged_last_tile {} cadence {:?}",
            p.vocab_size,
            head.tile_len,
            p.vocab_size.div_ceil(head.tile_len),
            p.vocab_size % head.tile_len != 0,
            kaspa_consensus_core::palw_context_ladder::palw_checkpoint_cadence_v1(&p)
        );
    }
}

// ------------------------------------------------------------------------------------------------
// Liveness: graph versions the new test file does not run (Qwen3.6 v5/v6, A16 v5)
// ------------------------------------------------------------------------------------------------

fn seat_rules_on(
    material: &[u8],
    family: misaka_palw_base0::produce::Base0SeatFamilyV1,
) -> Result<(), misaka_palw_base0::produce::Base0SeatRefusalV1> {
    use misaka_palw_base0::produce::{base0_dense_step_leaves_capped_v1, base0_seat_rules_v1};
    if let Ok(m) = base0_fp_material_decode_v2(material) {
        return base0_seat_rules_v1(&m.binding, &m.logits_rows, &m.generated_token_ids, None, family);
    }
    let t = base0_material_decode_v1(material).expect("a binding retention");
    let leaves = base0_dense_step_leaves_capped_v1(&t.0, &t.1, PALW_STEP_LEG_MAX_LEAVES);
    base0_seat_rules_v1(&t.0, &t.2, &t.3, leaves.as_deref(), family)
}

#[test]
fn liveness_other_graph_versions() {
    use kaspa_consensus_core::palw_context_ladder::{PalwCheckpointCadenceV1, palw_checkpoint_cadence_v1};
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3, fp_job_id_v3,
    };
    use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v5, qwen36_profile_v6};
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use misaka_palw_base0::produce::Base0SeatFamilyV1;
    let q36_geometry = PalwQwen36GeometryV1 {
        layer_count: 4,
        full_attention_interval: 4,
        hidden_dim: 32,
        attn_heads: 4,
        attn_kv_heads: 2,
        attn_head_dim: 16,
        rope_dims: 4,
        rope_freq_base_bits: 0x4B18_9680,
        gdn_k_heads: 2,
        gdn_v_heads: 4,
        gdn_head_dim: 8,
        gdn_conv_kernel: 4,
        n_experts: 8,
        experts_per_token: 4,
        moe_dim: 16,
        shared_dim: 16,
        attn_output_gate: 1,
        vocab_size: 64,
        n_ctx: 8,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 512,
    };
    let q36_artifact = Arc::new(misaka_palw_base0::qwen36::qwen36_dev_fixture(4, 8));
    let a16 = a16_artifact(8_292);
    let mut backends: Vec<(String, Box<dyn PalwExecutionBackendV1>, Base0SeatFamilyV1, PalwShapeProfileV3)> = Vec::new();
    for (label, profile) in [("Q36 v5", qwen36_profile_v5(q36_geometry)), ("Q36 v6", qwen36_profile_v6(q36_geometry))] {
        let Ok(profile) = profile else {
            eprintln!("{label}: profile does not project at the fixture geometry — skipped");
            continue;
        };
        match Qwen36Backend::from_registered_profile(q36_artifact.clone(), NETWORK.to_vec(), profile.clone(), (3, 4)) {
            Ok(b) => backends.push((
                label.to_string(),
                Box::new(b.with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES).with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)),
                Base0SeatFamilyV1::Qwen36,
                profile,
            )),
            Err(e) => eprintln!("{label}: not servable at the fixture ({e}) — skipped"),
        }
    }
    {
        let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v5(a16_geometry(8_292)).expect("v5");
        match Qwen25A16Backend::new(a16.clone(), NETWORK.to_vec(), profile.clone(), (15, 2)) {
            Ok(b) => backends.push((
                "A16 v5".to_string(),
                Box::new(b.with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES).with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)),
                Base0SeatFamilyV1::IntegerKv,
                profile,
            )),
            Err(e) => eprintln!("A16 v5: not servable ({e}) — skipped"),
        }
    }
    assert!(!backends.is_empty());
    for (label, backend, family, profile) in &backends {
        let cadence = palw_checkpoint_cadence_v1(profile);
        for draw in [true, false] {
            let anchor = Hash64::from_u64_word(0x7E57_0000 + u64::from(draw));
            let (job, prompt) = backend.job_for_anchor(anchor).expect("job");
            let job = palw_attempt_job_v1(job, draw);
            let out = backend.execute(&job, &prompt).expect("honest attempt");
            let verdict = backend.verify_material(
                &out.material,
                PalwClaimRootsV1 { execution_root: out.execution_root, trace_root: out.trace_root, anchor, attempt_draw: Some(draw) },
            );
            let rules = seat_rules_on(&out.material, *family);
            eprintln!("{label} attempt draw={draw} ({cadence:?}): verdict {verdict:?}, SEAT-0 rules {rules:?}");
            assert_eq!(rules, Ok(()), "{label}: SEAT-0 refuses an honest attempt");
            if cadence == PalwCheckpointCadenceV1::PerDecodeCall {
                assert_eq!(verdict, PalwMaterialVerdictV1::Matches, "{label}: an honest per-call dense attempt");
            }
        }
        // A free prompt of this class, through the capture envelope's inner bytes.
        let form = PalwPromptIdsFormV1::MerkleV1;
        let ids: Vec<u32> = vec![3, 1, 4, 1, 5];
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::from_u64_word(0xD0),
            class_id: profile.shape_profile_id(),
            executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
            executor_pubkey: vec![0x11; 32],
            operator_id: Hash64::from_u64_word(0x0B),
            anchor_block: Hash64::from_u64_word(0xA0),
            anchor_daa: 4242,
            job_nonce: [0x5A; 32],
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(form, &ids).unwrap(),
            prompt_tokens: ids.len() as u32,
            decode_token_limit: 2,
            max_context_tokens: profile.n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
            sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        };
        let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
        match backend.execute_free_prompt(&job, &prompt) {
            Ok(run) => {
                let verdict = backend.verify_material(
                    &run.outcome.material,
                    PalwClaimRootsV1 {
                        execution_root: run.outcome.execution_root,
                        trace_root: run.outcome.trace_root,
                        anchor: fp_job_id_v3(&job),
                        attempt_draw: None,
                    },
                );
                let rules = seat_rules_on(&run.outcome.material, *family);
                eprintln!("{label} FP 5+2: verdict {verdict:?}, SEAT-0 rules {rules:?}");
                assert_eq!(verdict, PalwMaterialVerdictV1::Matches, "{label}: an honest free prompt");
            }
            Err(e) => eprintln!("{label} FP: the producer refuses ({e}) — nothing to seat"),
        }
    }
}
