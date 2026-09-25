//! MSK-26A-PALW-10 -- `palw_public_model_source_required` is armed at DAA 0 on testnet-12 and
//! hashed into `consensus_params_id`, but `palw_public_model_source_v1` is never compiled (lib.rs
//! has no `mod` line), so nothing enforces a public-source commitment on class registrations.
//!
//! Audit commit: 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate:        kaspa-consensus-core (consensus/core)
//! Command:
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-PALW-10.rs \
//!      consensus/core/tests/audit_poc_msk_26a_palw_10.rs && \
//!   cargo test -p kaspa-consensus-core --test audit_poc_msk_26a_palw_10 -- --nocapture ; \
//!   rm consensus/core/tests/audit_poc_msk_26a_palw_10.rs
//!
//! PASS = the vulnerable behaviour is present:
//!   (1) `palw_t12_shipped_params()` arms `palw_public_model_source_required` at DAA 0 (mainnet: None)
//!       and the fence moves `consensus_params_id` / `consensus_identity_id` (it is fingerprinted);
//!   (2) `consensus/core/src/lib.rs` declares no `palw_public_model_source_v1` module although the
//!       file exists, and no non-test source file in the workspace reads the fence outside
//!       params.rs / fork_id_v1.rs / the extension's ruleset-candidate parser / the orphan file;
//!   (3) the registration object (`ClassRegistered` + `PalwClassAdmissionCarriageV2`) has no field
//!       that could carry a source (the exhaustive destructures below stop compiling once one is
//!       added), and such a registration -- with no source at all -- folds on testnet-12's params at
//!       DAA 2, i.e. past the fence.
//! Once the rule is wired (module compiled, a source field on the registration, a reader of the
//! fence in the admission path) this test fails to compile or its assertions fail.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{
    ForkActivation, Params, PalwPublicModelSourceRuleV1, mainnet_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1, palw_genesis_model_works_v1, palw_rc_typed_class_works_v1,
};
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClassAdmissionCarriageV2, PalwConsensusObjectV2,
    PalwPwuRuleV2, PalwTransitionExtrasV1, apply_palw_transition_v7,
};
use kaspa_consensus_core::palw_step::step_leaf_count_capped_v1;
use kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::path::{Path, PathBuf};

const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
const REGISTRANT: u64 = 0x5_0C_E;

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("t12 is a ConsensusV2 network"),
    }
}

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn ctx(block: u64, daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: daa, subsidy: T12_BLOCK_SUBSIDY_SOMPI }
}

// ---------------------------------------------------------------------------------------------
// (1) The fence is armed at genesis on testnet-12 and is part of the network's fingerprint.
// ---------------------------------------------------------------------------------------------
#[test]
fn msk_26a_palw_10_fence_is_armed_and_fingerprinted() {
    let t12 = palw_t12_shipped_params();
    let main = mainnet_shipped_params();
    println!("t12     palw_public_model_source_required = {:?}", t12.palw_public_model_source_required);
    println!("mainnet palw_public_model_source_required = {:?}", main.palw_public_model_source_required);
    assert_eq!(
        t12.palw_public_model_source_required,
        Some(PalwPublicModelSourceRuleV1 { activation: ForkActivation::new(0) }),
        "testnet-12 arms the public-source rule at DAA 0"
    );
    assert_eq!(main.palw_public_model_source_required, None, "mainnet preset leaves it dormant");

    let mut cleared = t12.clone();
    cleared.palw_public_model_source_required = None;
    println!(
        "consensus_params_id t12 = {:?}\nconsensus_params_id t12 w/o fence = {:?}",
        t12.consensus_params_id(),
        cleared.consensus_params_id()
    );
    assert_ne!(t12.consensus_params_id(), cleared.consensus_params_id(), "the fence is hashed into consensus_params_id");
    assert_ne!(t12.consensus_identity_id(), cleared.consensus_identity_id(), "and into the network identity");
}

// ---------------------------------------------------------------------------------------------
// (2) The module that would enforce it is not compiled, and nothing else reads the fence.
// ---------------------------------------------------------------------------------------------
fn walk_src(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if matches!(name.as_str(), "target" | ".git" | "docs" | "tests" | "node_modules") {
                continue;
            }
            walk_src(&p, out);
        } else if name.ends_with(".rs") && !name.starts_with("audit_poc_") {
            out.push(p);
        }
    }
}

#[test]
fn msk_26a_palw_10_enforcement_module_is_dead_code() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = core.join("..").join("..");
    let lib_rs = std::fs::read_to_string(core.join("src/lib.rs")).expect("lib.rs");
    let orphan = std::fs::read_to_string(core.join("src/palw_public_model_source_v1.rs")).expect("the module file exists");
    assert!(orphan.contains("pub fn palw_public_model_source_admit_v1("), "the enforcement function is written");
    assert!(
        orphan.contains("Consensus guarantees that an economically active registration past"),
        "and documented as a consensus guarantee"
    );
    let declared = lib_rs.lines().any(|l| {
        let t = l.trim_start();
        !t.starts_with("//") && t.contains("mod palw_public_model_source_v1")
    });
    println!("lib.rs declares `mod palw_public_model_source_v1`: {declared}");
    assert!(!declared, "lib.rs never declares the module, so it is not compiled");

    // Every non-test .rs file in the workspace that names the fence field.
    let mut files = Vec::new();
    walk_src(&root, &mut files);
    let mut readers: Vec<String> = files
        .iter()
        .filter(|p| std::fs::read_to_string(p).map(|s| s.contains("palw_public_model_source_required")).unwrap_or(false))
        .map(|p| p.strip_prefix(&root).unwrap_or(p).to_string_lossy().replace('\\', "/"))
        .collect();
    readers.sort();
    println!("files naming palw_public_model_source_required (of {} scanned): {readers:#?}", files.len());
    let allowed = [
        "consensus/core/src/config/params.rs",            // definition, normalisation, fence list, fingerprint, t12 arming
        "consensus/core/src/fork_id_v1.rs",               // fork-id fence sweep: sets the field
        "consensus/core/src/palw_public_model_source_v1.rs", // the orphan (doc comment only)
        "misaka-palw-extension/src/kinds/ruleset_candidate.rs", // candidate parser: sets the field
    ];
    for r in &readers {
        assert!(allowed.contains(&r.as_str()), "an unexpected reader of the fence exists: {r}");
    }
    // The consensus crate (block/virtual processing) never reads it.
    assert!(!readers.iter().any(|r| r.starts_with("consensus/src/")), "the validation pipeline does not read the fence");
    let params_rs = std::fs::read_to_string(core.join("src/config/params.rs")).unwrap();
    assert!(!params_rs.contains("fn palw_public_model_source_required_at"), "no accessor exists for the fence");
}

// ---------------------------------------------------------------------------------------------
// (3) A post-fence registration carries no source and folds on testnet-12.
// ---------------------------------------------------------------------------------------------
fn t12_extras(p: &Params, b: &PalwConsensusParamsV2) -> PalwTransitionExtrasV1 {
    let lane = p.palw_execution_lane.expect("t12 arms the execution lane");
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = b.panel.seat_count();
    let mut works = palw_genesis_model_works_v1(&b.genesis_objects);
    for (id, work) in palw_rc_typed_class_works_v1() {
        works.entry(id).or_insert(work);
    }
    let activation = p.palw_model_registry.map(|f| f.daa_score()).unwrap_or(0);
    let fold = PalwModelRegistryFoldV1 {
        globals,
        span_daa: lane.schedule_span_daa,
        genesis_works: works.clone(),
        grace_until_daa: PalwModelRegistryFoldV1::grace_until_v1(activation, lane.schedule_span_daa, &globals),
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(0),
    };
    PalwTransitionExtrasV1 {
        model_lines_active: true,
        model_benefits_active: true,
        evm_market_active: true,
        model_leg_v2_active: true,
        model_seed_v2_active: true,
        court_responder_coverage_active: true,
        fp_da_pins_active: true,
        share_growth_final_active: true,
        epoch_budget_release_active: true,
        panel_economy_active: true,
        work_priced_reward_active: true,
        panel_reward_multiple_permille: p.palw_panel_exposure_floor.map(|f| f.reward_multiple_permille).unwrap_or(0),
        economic_payout: p.palw_economic_payout.map(|f| f.fold_v1(0)),
        work_target: Some(PalwWorkTargetFoldV1 {
            rate_sompi_per_giga: 1,
            block_bits: 0,
            max_factor: b.state.class_daa_max_factor(),
            works: fold.genesis_works.clone(),
        }),
        work_target_active: true,
        artifact_root_ownership_active: true,
        operator_id_unique_active: true,
        canonical_work_daa: p.palw_canonical_work_daa(),
        admission_independence_daa: p.palw_admission_independence.map(|f| f.daa_score()),
        fp_derived_work_daa: p.palw_fp_derived_work.map(|f| f.daa_score()),
        single_lottery_active: true,
        verification_v2_active: true,
        verification_s3_active: true,
        verification_s2_active: true,
        readiness_v2_active: true,
        attn_anchored_root_active: true,
        audit_2026_09_11_active: true,
        audit_2026_09_11_deep_active: true,
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(0),
        settled_anchor_depth: if p.palw_audit_2026_09_23_active_at(0) { p.palw_settled_anchor_depth } else { None },
        prompt_ids_merkle: true,
        objective_offence_daa: p.palw_objective_offence.map(|f| f.daa_score()),
        seat_gate_possession_daa: p.palw_seat_gate_possession.map(|f| f.daa_score()),
        escrow_carve: Some(PalwRewardParamsV2::new(p.palw_overlay_carve.expect("carve").worker_carve_permille).expect("carve")),
        model_registry: Some(fold),
        ..Default::default()
    }
}

#[test]
fn msk_26a_palw_10_post_fence_registration_without_source_folds() {
    let p = palw_t12_shipped_params();
    let b = bundle_of(&p);
    let fence = p.palw_public_model_source_required.expect("armed").activation.daa_score();
    let extras = t12_extras(&p, &b);

    // Genesis plus a registrant bond (as `dos_l2_registration.rs` builds it).
    let mut genesis = b.genesis_objects.clone();
    genesis.push(PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(REGISTRANT),
        pubkey: vec![0xA7; 8],
        operator_pubkey: vec![0xA7; 16],
        collateral: 51_642_979_663_480,
        payout_payload: h(REGISTRANT),
        capable_classes: Default::default(),
        signature: Vec::new(),
    });
    let (s0, _, _) = apply_palw_transition_v7(
        &PalwChainStateV2::genesis(),
        &b.state,
        None,
        &ctx(1, 0),
        &genesis,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("genesis folds");

    // A new class derived from the floor class (distinct id, distinct root).
    let floor =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor");
    let fjob = kaspa_consensus_core::palw_base0_profile::rc_job_context(
        &floor,
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.0,
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1,
    );
    let mut prof = floor.clone();
    prof.n_threads = 7777;
    let mut job = fjob.clone();
    job.shape_profile_id = prof.shape_profile_id();
    let leaves = step_leaf_count_capped_v1(&prof, &job, b.court.max_step_leaf_count()).unwrap();
    let new_class = prof.shape_profile_id();
    assert!(s0.class(&new_class).is_none(), "not registered at genesis");

    let object = PalwConsensusObjectV2::ClassRegistered {
        class_id: new_class,
        artifact_root: h(0xFEED),
        slash_value_per_pwu: s0.class(&b.base_class_id).unwrap().slash_value_per_pwu,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
        initial_target: s0.class_target(&b.base_class_id).unwrap().target,
        share_permille: 0,
        activation_daa: 0,
        admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
            profile: prof,
            canonical: job,
            registrant_bond: bond_key(REGISTRANT),
            signature: vec![0; 4627],
        })),
    };

    // Exhaustive destructures (no `..`): these are EVERY field a post-genesis registration and its
    // carriage have. Neither has a URI, a revision or a `PalwPublicModelSourceV1`; adding one
    // makes this stop compiling.
    let PalwConsensusObjectV2::ClassRegistered {
        class_id: _,
        artifact_root: _,
        slash_value_per_pwu: _,
        pwu_rule: _,
        initial_target: _,
        share_permille: _,
        activation_daa: _,
        admission,
    } = &object
    else {
        unreachable!()
    };
    let PalwClassAdmissionCarriageV2 { profile: _, canonical: _, registrant_bond: _, signature: _ } =
        admission.as_deref().expect("a post-genesis registration carries its admission");

    let daa = 2u64;
    assert!(daa >= fence, "the block is past the public-source fence (DAA {fence})");
    let (s1, _, _) = apply_palw_transition_v7(
        &s0,
        &b.state,
        None,
        &ctx(2, daa),
        &[object],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("a registration with NO public source commitment folds past the fence");
    let row = s1.class(&new_class);
    println!(
        "t12 fence at DAA {fence}; registration at DAA {daa} with no public source folded; class row present = {}",
        row.is_some()
    );
    assert!(row.is_some(), "the class is registered without any public source");
}
