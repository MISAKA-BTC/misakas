//! **FR-09 / DSA on the registration path** (COV-P1P2, 2026-10-08): the two DSA fixtures (`deepseek_v32`, `glm_moe_dsa`; tiny,
//! `index_topk` 4) lowered, then asked (1) the IR registration gate of testnet-12 at DAA 7,000 through the SDK's layout search — the
//! token indexer's history cone (the counting-threshold selection: ≤ 16 reductions over `H`, `τ'` a commit point) dissected under the
//! k-ary court, its masked rows adjudicated as every other attention's — and (2) the kernel route's reference VerificationPlan
//! (K2-TIR-v1, armed hypothetically): every node of the selection has a relation of an implemented family.
//!
//! These are tiny models: an admitted fixture shows the FEATURE is registrable under the shipped rules, not that a released DSA
//! model is (every public DSA checkpoint is ≥ 321 B parameters, far past the per-position ceilings — a resource route, not FR-09).
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::tir_layout::{TirLayoutChoiceV1, tir_choose_layout_judged_v1, tir_program_with_scheme_v1};
use std::path::PathBuf;

const ROOT: Hash64 = Hash64::from_bytes([0xD5; 64]);

fn fixture(name: &str) -> String {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures/fr09").join(name).join("config.json");
    std::fs::read_to_string(p).expect("the fixture's config")
}

fn gate(
    params: &kaspa_consensus_core::config::params::Params,
    bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    class: &PalwTirClassV1,
    daa: u64,
) -> Result<(), String> {
    let program = class.decode_program().map_err(|e| e.to_string())?;
    let canonical = palw_tir_attempt_canonical_v1(class).ok_or("too narrow")?;
    let facts = PalwTirJobFactsV1::of(class, &program, class.class_id(&ROOT));
    let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
        0,
    ));
    let object = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_post_genesis_registration_v1(
        class.clone(),
        palw_tir_job_context_v1(&facts, canonical),
        ROOT,
        0,
        u128::MAX,
        1,
        0,
        bond,
        Vec::new(),
        bundle.court.max_step_leaf_count(),
    )
    .map_err(|e| format!("{} ({e})", e.code()))?;
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_registration_preflight_at_v1(params, bundle, &object, daa, &[])
        .map(|_| ())
        .map_err(|e| format!("{} ({e})", e.code()))
}

#[test]
fn the_dsa_fixtures_register_under_the_shipped_rules_and_have_a_complete_k2_plan() {
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("a V2 network") };
    for name in ["deepseek_v32", "glm_moe_dsa"] {
        let prep = misaka_palw_tir_lower::fidelity::prepare(&fixture(name), &misaka_palw_tir_lower::lower::LowerOpts::default())
            .expect("lowered");
        let program = tir_program_with_scheme_v1(&prep.lowered.program, None).expect("tiled");
        // The selection's cones: every dissected commit point reduces over `H` at most 16 times (spec 04b §9.5.1).
        let dissected = kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(&program);
        let worst = dissected
            .iter()
            .map(|(b, n)| {
                kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_cone_reductions_v1(&program.blocks[*b as usize], *n).len()
            })
            .max()
            .unwrap_or(0);
        assert!(!dissected.is_empty() && worst <= 16, "{name}: {} dissected points, at most {worst} reductions", dissected.len());
        // (1) The IR gate at DAA 7,000, through the SDK's layout search.
        let ctx = 128;
        let choice = TirLayoutChoiceV1 { max_context: Some(ctx), ..Default::default() };
        let judged = |c: &PalwTirClassV1| gate(&params, bundle, c, 7_000);
        let chosen =
            tir_choose_layout_judged_v1(&params, bundle, &program, Hash64::from_bytes([0x11; 64]), 64, &choice, true, &judged)
                .expect("a layout search");
        eprintln!(
            "[dsa] {name} @{ctx}: {} dissected commit points (≤ {worst} reductions each), logits tile {:?}, h_tile {} -> {:?}",
            dissected.len(),
            chosen.layout.commit_tiles.last(),
            chosen.layout.h_tile,
            chosen.admission
        );
        assert!(chosen.admission.is_ok(), "{name}: {:?}", chosen.admission);
        // (2) The kernel route: K2-TIR-v1's reference plan, armed hypothetically.
        let route = misaka_palw_sdk::preflight::kernel::kernel_route_of(&program, ctx, 0);
        eprintln!("[dsa] {name}: {} shipped {} hypothetical {} — {}", route.kernel, route.shipped, route.hypothetical, route.detail);
        assert_eq!(route.hypothetical, "ELIGIBLE_AT", "{name}: {}", route.detail);
        assert_eq!(route.shipped, "KERNEL_NOT_ACTIVE");
    }
}

/// **`WEIGHT_ROTATION_HADAMARD_V1` stays inside the shipped primitives and K2 families**: the tiny rotated Qwen3.5 GGUF
/// (`misaka-palw-tir-lower/tests/fixtures/gguf/prism_rotated/rotated`) lowered with its rotation ops (`Op::BlockLinear` → a batched
/// `MatMul` of `i8` rows) is admitted by the IR gate at DAA 7,000 and its K2-TIR-v1 plan is ELIGIBLE_AT — no new primitive, no new
/// family.
#[test]
fn the_rotation_op_is_admitted_and_planned_with_existing_families() {
    let params = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("a V2 network") };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../misaka-palw-tir-lower/tests/fixtures/gguf/prism_rotated/rotated/model.gguf");
    let model = misaka_palw_tir_lower::gguf::GgufModel::open(&path).expect("the rotated fixture");
    let prep = model.prepare(&misaka_palw_tir_lower::lower::LowerOpts::default()).expect("prepared");
    let rotations = prep
        .hl
        .blocks
        .iter()
        .flat_map(|b| b.nodes.iter())
        .filter(|n| matches!(n.op, misaka_palw_tir_lower::hl::Op::BlockLinear { .. }))
        .count();
    assert!(rotations > 0);
    let program = tir_program_with_scheme_v1(&prep.lowered.program, None).expect("tiled");
    let ctx = 128;
    let choice = TirLayoutChoiceV1 { max_context: Some(ctx), ..Default::default() };
    let judged = |c: &PalwTirClassV1| gate(&params, bundle, c, 7_000);
    let chosen = tir_choose_layout_judged_v1(&params, bundle, &program, Hash64::from_bytes([0x22; 64]), 64, &choice, true, &judged)
        .expect("search");
    eprintln!("[rotation] {rotations} rotation ops; @{ctx}: {:?}", chosen.admission);
    assert!(chosen.admission.is_ok(), "{:?}", chosen.admission);
    let route = misaka_palw_sdk::preflight::kernel::kernel_route_of(&program, ctx, 0);
    eprintln!("[rotation] {} hypothetical {} — {}", route.kernel, route.hypothetical, route.detail);
    assert_eq!(route.hypothetical, "ELIGIBLE_AT", "{}", route.detail);
}
