//! **The cross-lane G14 gates** (`kaspa_consensus_core::palw_g14_gates_v1`; audit `docs/audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md`
//! §4.1 / §5.2): T-N17 (G-10), T-N16 (G-18), T-N9 (G-02 / N4), T-N8 and T-N10 (G-01 option (b) / G-05), T-N15 (G-08 / G-15), the X3
//! candidate (G-09), Kimi (G-16), "G14 from genesis" (the test-only g14-genesis ruleset) and testnet-12's pinned identity.
//!
//! The fold fixture is testnet-12's own (`dos_l5_common`): the real `apply_palw_transition_v7` on the shipped bundle, with each dormant
//! fence's MIRROR set on the bundle's state params exactly as `Params::sync_*` writes it (validation would refuse the arming — that is
//! what T-N17 / T-N9 assert — so a fixture arms the mirror the fold reads). Heights are compressed onto the fixture's own timeline
//! (`FENCE` stands for the release's DAA 14,000); the Params-level tests use 14,000 itself.
//!
//! Run: cargo test -p kaspa-consensus-core --test g14_close_gates -j 4 -- --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, Params, devnet_shipped_params, palw_rc_shipped_params, palw_t12_launch_params_v1,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V4_VERSION, PalwFpJobTailV1,
    PalwFpStopReasonV3, PalwFreePromptCommitmentV3, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_g14_gates_v1::*;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_ruling_fences_v1::{PalwClaimHardDeadlineFenceV1, palw_x3_unmeasured_terms_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwCertifiedFamilyStateV2, PalwCertifiedLaneV1, PalwChainStateV2,
    PalwClaimPhaseV2, PalwClaimStateV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwStateCarriageV2, PalwStateParamsV2,
    PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v7, palw_claim_final_floor_v1, palw_class_admits_claim_v1,
};

/// The fixture's stand-in for the release's DAA 14,000 (the fixture's chain lives around DAA 1,000).
const FENCE: u64 = 1_001;
/// The release height the Params-level tests use.
const RELEASE: u64 = 14_000;

// ---- fixture -------------------------------------------------------------------------------------------------------------------

fn sp_of(p: &Params) -> PalwStateParamsV2 {
    bundle(p).state
}

/// `dos_repro_1`'s FP extras: the processor's t12 resolution of the derived-work and DA-pin fences.
fn fp_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    e.fp_derived_work_daa = p.palw_fp_derived_work_fence().map(|f| f.daa_score());
    e.fp_da_pins_active = p.palw_fp_da_pins_fence().is_some_and(|f| f.is_active(daa));
    e
}

/// One block through the real fold under `sp` (a mirror-armed copy of the bundle's state params).
fn fold_sp(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let f = flags(p, ctx.daa_score);
    apply_palw_transition_v7(
        parent,
        sp,
        None,
        ctx,
        objects,
        work,
        &[],
        exec_key,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &fp_extras(p, ctx.daa_score),
    )
    .map(|(s, _, _)| s)
}

fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

fn floor_work() -> kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1 {
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives")
}

/// `dos_repro_1`'s prerequisites: the pinned FreePrompt family and the floor's registry row (real state objects).
fn seed_prereqs(p: &Params, s: &PalwChainStateV2) -> PalwChainStateV2 {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, palw_lifecycle_profile_v1,
    };
    let (floor, _l, target, _sv) = genesis_classes(p)[0];
    let fam = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_fp_certified_families_v1()
        .into_iter()
        .find(|f| f.drilled_class_id == floor_profile().shape_profile_id())
        .expect("BASE-0 fp family is pinned");
    let work = floor_work();
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = bundle(p).panel.seat_count();
    let expected_q32 = kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
    let row = PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Active,
        work,
        profile: palw_lifecycle_profile_v1(&work, expected_q32, &globals, false),
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    };
    let mut c = PalwStateCarriageV2::from_state(s);
    c.fp_certified_families.insert(fam.digest(), PalwCertifiedFamilyStateV2 { family: fam, certified_daa: 0 });
    c.model_lifecycles.insert(floor, row);
    c.into_state_v3(&bundle(p).state, None, false, p.palw_canonical_work_daa()).expect("carriage rebuilds")
}

fn job(privacy_mode: u8, version: u16) -> PalwFreePromptJobV3 {
    PalwFreePromptJobV3 {
        version,
        network_domain: h(NET),
        class_id: h(0xC1),
        executor_bond: bond_key(1).0,
        executor_pubkey: pubkey_of(1),
        operator_id: h(0xE0),
        anchor_block: h(0xA0),
        anchor_daa: 1_000,
        job_nonce: [0x11; 32],
        tokenizer_id: h(0x70),
        prompt_token_ids_hash: h(0x9012),
        prompt_tokens: 96,
        decode_token_limit: 128,
        max_context_tokens: 4096,
        privacy_mode,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    }
}

fn commitment(job: PalwFreePromptJobV3) -> PalwFreePromptCommitmentV3 {
    PalwFreePromptCommitmentV3 {
        job,
        trace_root: h(1),
        output_root: h(2),
        schedule_root: h(3),
        execution_root: h(4),
        decode_tokens_executed: 8,
        stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
        work_leaves: 64,
        trace_manifest_root: h(5),
        trace_chunk_count: 1,
        trace_retention_daa: 10_000,
    }
}

fn public_v4() -> PalwFreePromptCommitmentV3 {
    commitment(job(PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V4_VERSION))
}
fn panel_da() -> PalwFreePromptCommitmentV3 {
    commitment(job(PALW_FP_PRIVACY_PANEL_DA, PALW_FP_V4_VERSION))
}
fn v5_image() -> PalwFreePromptCommitmentV3 {
    let mut j = job(PALW_FP_PRIVACY_PUBLIC_DA, kaspa_consensus_core::palw_fp_job_v5::PALW_FP_V5_VERSION);
    j.tail = Some(PalwFpJobTailV1::V5(kaspa_consensus_core::palw_fp_job_v5::PalwFpV5TailV1 {
        images: vec![kaspa_consensus_core::palw_gen_class_v1::PalwGenImageInputRefV1 { input_root: h(9), h: 4, w: 4 }],
        source: None,
    }));
    commitment(j)
}
fn constraint_v6() -> PalwFreePromptCommitmentV3 {
    commitment(job(PALW_FP_PRIVACY_PUBLIC_DA, kaspa_consensus_core::palw_fp_constraint_job_v1::PALW_FP_CONSTRAINT_VERSION))
}
fn prefix_v11() -> PalwFreePromptCommitmentV3 {
    commitment(job(PALW_FP_PRIVACY_PUBLIC_DA, kaspa_consensus_core::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION))
}

/// The floor class's FP commitment (dos_repro_1's largest floor claim) naming `public_job`.
#[allow(clippy::too_many_arguments)]
fn fp_commit(
    s: &PalwChainStateV2,
    class_id: Hash64,
    bond: PalwBondKeyV2,
    pk: Vec<u8>,
    claim: Hash64,
    public_job: Option<PalwFreePromptCommitmentV3>,
) -> PalwConsensusObjectV2 {
    let profile = floor_profile();
    let ladder = s.class_step_ladder_v1(&class_id, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    let (pt, dc) = (512u32, 256u32);
    let prompt: Vec<u32> = (0..pt).map(|i| i + (claim.as_bytes()[0] as u32)).collect();
    let work_leaves =
        kaspa_consensus_core::palw_step::step_leaf_count_of_tokens_capped_v1(&profile, pt, dc, ladder).expect("floor leaves derive");
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: Hash64::default(),
        public_job: public_job.map(Box::new),
        eval: None,
        claim,
        class_id,
        bond,
        executor_pubkey: pk,
        work_leaves,
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&prompt),
        prompt_tokens: pt,
        prompt_token_ids: prompt,
        decode_tokens_executed: dc,
        trace_root: h(0x71_0001),
        output_root: h(0x72_0001),
        execution_root: h(0x73_0000 ^ claim.as_bytes()[0] as u64),
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(class_id),
    }
}

/// testnet-12 at DAA 1,002: the attacker bond registered and the floor's FP profile published (dos_repro_1, steps up to the commit).
struct Rig {
    p: Params,
    s: PalwChainStateV2,
    floor: Hash64,
    bond: PalwBondKeyV2,
    pk: Vec<u8>,
}

fn rig(p: Params) -> Rig {
    let sp = sp_of(&p);
    let (floor, ..) = genesis_classes(&p)[0];
    let attacker = 1u64;
    let g = genesis_state(&p);
    let s = fold_sp(
        &p,
        &sp,
        &g,
        &ctx(1, 1_000, 1, 0),
        &[bond_obj(attacker, at_least_the_floor(&p, 100_000_000_000))],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the bond registers");
    let s = seed_prereqs(&p, &s);
    let publish =
        PalwConsensusObjectV2::ClassLaneCertified { class_id: floor, lane: PalwCertifiedLaneV1::FreePrompt, profile: floor_profile() };
    let s =
        fold_sp(&p, &sp, &s, &ctx(2, 1_000, 2, 0), &[publish], PalwBlockWorkV3::None, Hash64::default()).expect("profile published");
    Rig { p, s, floor, bond: bond_key(attacker), pk: pubkey_of(attacker) }
}

fn refusal_code(e: &PalwStateV2Error) -> Option<&'static str> {
    match e {
        PalwStateV2Error::ClassNotRewardable { code, .. } => Some(code),
        _ => None,
    }
}

/// An armed record for `fence` on `p`'s genesis at `at`, every digest distinct and non-zero.
fn record(fence: PalwG14GatedFenceV1, p: &Params, at: u64) -> PalwG14ArmingRecordV1 {
    let evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [0x10 + i as u8; 32]).collect();
    PalwG14ArmingRecordV1 {
        version: PALW_G14_ARMING_RECORD_VERSION_V1,
        fence,
        genesis: p.genesis.hash,
        activation_daa: at,
        source_commit: [0xC0; 20],
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: [0xEE; 32],
    }
}

fn set(p: &mut Params, fence: PalwG14GatedFenceV1, at: Option<ForkActivation>) {
    match fence {
        PalwG14GatedFenceV1::StagedPublicCourtV1 => p.palw_staged_public_court_v1 = at,
        PalwG14GatedFenceV1::StagedTirRouteV1 => p.palw_staged_tir_route_v1 = at,
        PalwG14GatedFenceV1::StagedFloatConstraintV1 => p.palw_staged_float_constraint_v1 = at,
        PalwG14GatedFenceV1::TirPublicConvictionV1 => p.palw_tir_public_conviction_v1 = at,
        PalwG14GatedFenceV1::PublicEvidenceRewardV1 => {
            p.palw_public_evidence_reward_v1 = at;
            p.sync_palw_2026_10_12_ruling_fences_v1();
        }
        PalwG14GatedFenceV1::G14RouteRequiredV1 => p.palw_g14_route_required_v1 = at,
        PalwG14GatedFenceV1::RewardLiabilityHorizonV1 => p.palw_reward_liability_horizon_v1 = at,
        // Lane g14-close-legacy: LG14-A / LG14-B ride the same record gate (LG14-B with LG14-A at or below it).
        PalwG14GatedFenceV1::LegacyPublicFilerV1 => {
            p.palw_legacy_public_filer_v1 = at;
            p.sync_palw_legacy_public_filer_v1();
        }
        PalwG14GatedFenceV1::LegacyHeldDaV2 => {
            p.palw_legacy_public_filer_v1 = at;
            p.sync_palw_legacy_public_filer_v1();
            p.palw_legacy_held_da_v2 = at;
        }
        // Lane g14-court-capacity: the court-capacity fence (with LG14-A at or below it; the placeholder policy, which a real network
        // is refused even with a record).
        PalwG14GatedFenceV1::CourtCapacityV1 => {
            p.palw_legacy_public_filer_v1 = at;
            p.sync_palw_legacy_public_filer_v1();
            p.palw_court_capacity_v1 = at.map(kaspa_consensus_core::palw_court_capacity_v1::PalwCourtCapacityFenceV1::placeholder_at);
            p.sync_palw_court_capacity_v1();
        }
        // The ruling fences D2 and X3 (lane d2-x3-finish, 2026-10-18): record-gated like the others. D2 needs LG14-B at or below it.
        PalwG14GatedFenceV1::CourtModelScopeV1 => {
            p.palw_legacy_held_da_v2 = at;
            p.palw_court_model_scope_v1 = at;
            p.sync_palw_2026_10_12_ruling_fences_v1();
        }
        PalwG14GatedFenceV1::ClaimHardDeadlineV1 => {
            p.palw_claim_hard_deadline_v1 =
                at.map(kaspa_consensus_core::palw_ruling_fences_v1::PalwClaimHardDeadlineFenceV1::unapproved_probe_at);
            p.sync_palw_2026_10_12_ruling_fences_v1();
        }
    }
    p.sync_palw_g14_gates_v1();
}

// ---- T-N17 (G-10) and the record mechanics -------------------------------------------------------------------------------------

/// **T-N17**: every armable G14 fence — and N4, option (b) and the liability horizon — is refused when armed at a real height on a
/// real network until a record for exactly that network and height is pinned; this binary pins none. `None` and `never()` pass.
#[test]
fn armable_g14_fences_refuse_without_a_measured_record() {
    assert!(
        PALW_G14_ARMING_RECORDS_V1
            .iter()
            .all(|r| r.activation_daa == 17_400 && r.genesis == kaspa_consensus_core::config::genesis::PALW_T12_GENESIS.hash),
        "no record is pinned in this binary"
    );
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_g14_gates_v1().expect("testnet-12 as shipped arms none of them");
    for fence in PalwG14GatedFenceV1::ALL {
        let mut armed = t12.clone();
        set(&mut armed, fence, Some(ForkActivation::new(RELEASE)));
        let why = armed.validate_palw_g14_fence_record_v1(fence).expect_err("refused without a pinned record");
        let PalwModeV2Error::Invalid(why) = why else { panic!("{fence:?}: {why:?}") };
        assert!(why.contains(fence.name()) && why.contains("cannot be armed"), "{fence:?}: {why}");
        assert!(armed.validate_palw_v2().is_err(), "{fence:?}: the whole validation refuses it too");
        let mut never = t12.clone();
        set(&mut never, fence, Some(ForkActivation::never()));
        never.validate_palw_g14_fence_record_v1(fence).expect("never() is dormant");
        // The four G-10 fences need V1–V7; option (b), N4 and the liability horizon only remove or delay reward.
        // X3 (lane d2-x3-finish, 2026-10-18) is the one ruling fence whose policy terms are measurements; D2 needs tests and drills only.
        if fence == PalwG14GatedFenceV1::ClaimHardDeadlineV1 {
            assert!(fence.needs_measured_value(), "X3's policy terms are measurements");
            continue;
        }
        assert_eq!(fence.needs_measured_value(), fence.requirements().len() == PALW_G14_VALIDATION_STEPS_V1.len(), "{fence:?}");
    }
    assert!(!PalwG14GatedFenceV1::G14RouteRequiredV1.needs_measured_value());
    assert!(!PalwG14GatedFenceV1::PublicEvidenceRewardV1.needs_measured_value());
    assert!(!PalwG14GatedFenceV1::RewardLiabilityHorizonV1.needs_measured_value());
    for fence in [
        PalwG14GatedFenceV1::StagedPublicCourtV1,
        PalwG14GatedFenceV1::StagedTirRouteV1,
        PalwG14GatedFenceV1::StagedFloatConstraintV1,
        PalwG14GatedFenceV1::TirPublicConvictionV1,
    ] {
        assert!(fence.needs_measured_value(), "{fence:?}");
    }
}

/// The record a reviewed change would pin: exact (fence, network, height), one distinct non-zero evidence digest per requirement, a
/// review and a source commit. Anything else is refused by name.
#[test]
fn an_arming_record_admits_exactly_its_network_and_height() {
    let t12 = palw_t12_shipped_params();
    let fence = PalwG14GatedFenceV1::G14RouteRequiredV1;
    let good = record(fence, &t12, RELEASE);
    assert_eq!(palw_g14_arming_record_admits_v1(fence, t12.genesis.hash, RELEASE, &[good]), Ok(()));
    assert_eq!(
        palw_g14_arming_record_admits_v1(fence, t12.genesis.hash, RELEASE + 1, &[good]),
        Err(PalwG14ArmingRefusalV1::NoRecord),
        "another height"
    );
    assert_eq!(
        palw_g14_arming_record_admits_v1(fence, devnet_shipped_params().genesis.hash, RELEASE, &[good]),
        Err(PalwG14ArmingRefusalV1::NoRecord),
        "another network"
    );
    assert_eq!(
        palw_g14_arming_record_admits_v1(PalwG14GatedFenceV1::PublicEvidenceRewardV1, t12.genesis.hash, RELEASE, &[good]),
        Err(PalwG14ArmingRefusalV1::NoRecord),
        "another fence"
    );
    assert_eq!(palw_g14_arming_record_form_v1(&PalwG14ArmingRecordV1 { version: 2, ..good }), Err(PalwG14ArmingRefusalV1::Version));
    assert_eq!(
        palw_g14_arming_record_form_v1(&PalwG14ArmingRecordV1 { source_commit: [0; 20], ..good }),
        Err(PalwG14ArmingRefusalV1::SourceCommit)
    );
    assert_eq!(
        palw_g14_arming_record_form_v1(&PalwG14ArmingRecordV1 { review: [0; 32], ..good }),
        Err(PalwG14ArmingRefusalV1::Review)
    );
    assert!(matches!(
        palw_g14_arming_record_form_v1(&PalwG14ArmingRecordV1 { evidence: &[[1; 32]], ..good }),
        Err(PalwG14ArmingRefusalV1::EvidenceCount { .. })
    ));
    let mut twice = good.evidence.to_vec();
    twice[1] = twice[0];
    assert_eq!(
        palw_g14_arming_record_form_v1(&PalwG14ArmingRecordV1 { evidence: Box::leak(twice.into_boxed_slice()), ..good }),
        Err(PalwG14ArmingRefusalV1::Evidence(1))
    );
    let mut zero = good.evidence.to_vec();
    zero[2] = [0; 32];
    assert_eq!(
        palw_g14_arming_record_form_v1(&PalwG14ArmingRecordV1 { evidence: Box::leak(zero.into_boxed_slice()), ..good }),
        Err(PalwG14ArmingRefusalV1::Evidence(2))
    );
    let mut as_review = good.evidence.to_vec();
    as_review[0] = good.review;
    assert_eq!(
        palw_g14_arming_record_form_v1(&PalwG14ArmingRecordV1 { evidence: Box::leak(as_review.into_boxed_slice()), ..good }),
        Err(PalwG14ArmingRefusalV1::Evidence(0))
    );
    // The record that would arm option (b) at DAA 14,000 names its own tests, the node E2E and the crossing drill.
    assert_eq!(fence.requirements(), PALW_G14_ROUTE_REQUIRED_ARMING_REQUIREMENTS_V1);
    assert!(fence.requirements()[0].contains("T-N8") && fence.requirements()[1].contains("T-N10"));
}

// ---- T-N16 (G-18) ----------------------------------------------------------------------------------------------------------------

/// **T-N16**: the staged public court needs the legacy public filer in force at or below it (G-18: a staged seed and BundleOpen each
/// need the filer's own reservation, tag 154).
#[test]
fn staged_court_validation_requires_the_legacy_filer_fence() {
    let mut p = palw_t12_shipped_params();
    p.palw_staged_public_court_v1 = Some(ForkActivation::new(RELEASE));
    let why = palw_g14_staged_court_needs_legacy_filer_v1(&p).expect_err("no filer");
    assert!(format!("{why:?}").contains("palw_legacy_public_filer_v1"), "{why:?}");
    // The whole validation names the prerequisite (asked before the record gate).
    let whole = p.validate_palw_v2().expect_err("refused");
    assert!(format!("{whole:?}").contains("needs palw_legacy_public_filer_v1 in force at or below it"), "{whole:?}");
    // A filer above the court is no prerequisite.
    p.palw_legacy_public_filer_v1 = Some(ForkActivation::new(RELEASE + 1));
    assert!(palw_g14_staged_court_needs_legacy_filer_v1(&p).is_err());
    p.palw_legacy_public_filer_v1 = Some(ForkActivation::never());
    assert!(palw_g14_staged_court_needs_legacy_filer_v1(&p).is_err());
    // At or below: the prerequisite holds (each fence is still refused by its own gate until its record is pinned).
    for at in [RELEASE, RELEASE - 1, 0] {
        p.palw_legacy_public_filer_v1 = Some(ForkActivation::new(at));
        p.sync_palw_legacy_public_filer_v1();
        palw_g14_staged_court_needs_legacy_filer_v1(&p).unwrap_or_else(|e| panic!("filer at {at}: {e:?}"));
        assert!(p.validate_palw_v2().is_err(), "the filer and the court each still need their own release");
    }
    // Dormant court: nothing required.
    let mut dormant = palw_t12_shipped_params();
    dormant.palw_staged_public_court_v1 = Some(ForkActivation::never());
    palw_g14_staged_court_needs_legacy_filer_v1(&dormant).expect("never() requires nothing");
}

// ---- T-N9 (G-02, N4) ---------------------------------------------------------------------------------------------------------------

/// **T-N9 (fold)**: past `palw_public_evidence_reward_v1` the claim gate takes no FP Job V5 image / source-id job and no PanelDa job —
/// no claim, so no reward or weight (the acceptance walk drops the object; the block stands); a public V4 job is taken exactly as
/// before; below the fence the same jobs are taken (no retroactivity). The arm-readiness record is defined and, unpinned, refused.
#[test]
fn n4_v5_image_and_panel_da_claims_take_no_new_reward() {
    let r = rig(palw_t12_launch_params_v1());
    let armed = sp_of(&r.p).with_public_evidence_reward_from_daa(Some(FENCE));
    for (n, job) in [(0xA1u64, v5_image()), (0xA2, panel_da())] {
        let claim = h(n);
        let commit = fp_commit(&r.s, r.floor, r.bond, r.pk.clone(), claim, Some(job.clone()));
        let refused =
            fold_sp(&r.p, &armed, &r.s, &ctx(3, FENCE + 1, 3, 0), &[commit.clone()], PalwBlockWorkV3::None, Hash64::default())
                .expect_err("past N4: refused at the claim gate");
        assert_eq!(refusal_code(&refused), Some("N4_NONPUBLIC_EVIDENCE"), "{refused:?}");
        // Below the fence (a claim accepted before it): the old ruleset takes it.
        let below = sp_of(&r.p).with_public_evidence_reward_from_daa(Some(FENCE + 10));
        let s = fold_sp(&r.p, &below, &r.s, &ctx(3, FENCE + 1, 3, 0), &[commit], PalwBlockWorkV3::None, Hash64::default())
            .expect("below N4 the old ruleset takes it");
        assert!(s.claim(&claim).is_some(), "the old-rule claim exists");
    }
    // A public V4 job is unaffected by N4.
    let claim = h(0xA3);
    let commit = fp_commit(&r.s, r.floor, r.bond, r.pk.clone(), claim, Some(public_v4()));
    let s = fold_sp(&r.p, &armed, &r.s, &ctx(3, FENCE + 1, 3, 0), &[commit], PalwBlockWorkV3::None, Hash64::default())
        .expect("a public-evidence job passes N4");
    let taken = s.claim(&claim).expect("taken");
    assert!(kaspa_consensus_core::palw_ruling_fences_v1::palw_claim_earns_reward_or_weight_v1(taken), "and earns as before");
    // The arm-readiness record N4 needs at DAA 14,000 on testnet-12: T-N9 (fold + node E2E) and the D-17 crossing drill.
    let mut p = palw_t12_shipped_params();
    p.palw_public_evidence_reward_v1 = Some(ForkActivation::new(RELEASE));
    p.sync_palw_2026_10_12_ruling_fences_v1();
    let why = p.validate_palw_2026_10_12_ruling_fences_v1().expect_err("unpinned");
    assert!(format!("{why:?}").contains("no arm-readiness record"), "{why:?}");
    let n4 = PalwG14GatedFenceV1::PublicEvidenceRewardV1;
    assert_eq!(n4.requirements(), PALW_N4_ARMING_REQUIREMENTS_V1);
    assert_eq!(palw_g14_arming_record_admits_v1(n4, p.genesis.hash, RELEASE, &[record(n4, &p, RELEASE)]), Ok(()));
}

// ---- T-N8 and T-N10 (G-01 option (b), G-05) ---------------------------------------------------------------------------------------

/// The route mirror as `sync_palw_g14_gates_v1` writes it, with the given route fences in force from `routes`.
fn route_sp(p: &Params, from: u64, routes: PalwG14RouteMirrorV1) -> PalwStateParamsV2 {
    sp_of(p).with_g14_route_required_v1(Some(PalwG14RouteMirrorV1 { from_daa: from, ..routes }))
}

/// **T-N8**: past `palw_g14_route_required_v1` a newly admitted legacy-route claim with no armed G14 route earns nothing — a V2 Attempt
/// is refused at the class gate (the producer's pre-check answers it before an inference; an own attempt that skipped would be paid its
/// carve unverified, so the gate fails it as G14-for-rewards does), a plain FP commitment is refused at the claim gate (dropped by the
/// acceptance walk; its carrier pays the ordinary fee). A claim admitted before the fence settles under the old rules, and a family
/// regains reward the moment its route is armed (the rule reads the fence state).
#[test]
fn legacy_route_claim_without_a_g14_path_earns_nothing_past_14000() {
    let p = palw_t12_launch_params_v1();
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let none = PalwG14RouteMirrorV1::default();
    let armed = route_sp(&p, FENCE, none);
    // A rich probe bond (admitted_per_attempt's fixture), then one attempt.
    const PROBE: u64 = 0xAD_0009;
    let base = sp_of(&p);
    let g = genesis_state(&p);
    let s = fold_sp(
        &p,
        &base,
        &g,
        &ctx(0xAD00_0001, 1_000, 1, 0),
        &[bond_obj(PROBE, 1_000_000_000_000_000)],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the probe bond registers");
    let (env, key, id) =
        junk_attempt(floor, bond_key(PROBE), pubkey_of(PROBE), &operator_pubkey_of(PROBE), pwu, 0xAD_5EED, 0xAD_0000_5EED);
    let attempt_at = |sp: &PalwStateParamsV2, daa: u64| {
        fold_sp(&p, sp, &s, &ctx(0xAD00_0002, daa, 2, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
    };
    // Past the fence, no route: refused by name, no claim, no escrow, no weight.
    let refused = attempt_at(&armed, FENCE).expect_err("a route-less attempt past the fence");
    assert_eq!(refusal_code(&refused), Some(PALW_G14_ROUTE_ABSENT_CODE_V1), "{refused:?}");
    let pre = palw_class_admits_claim_v1(&s, &armed, &extras(&p, FENCE), &floor, FENCE).expect_err("the producer's pre-check");
    assert_eq!(refusal_code(&pre), Some(PALW_G14_ROUTE_ABSENT_CODE_V1));
    // Admitted before the fence: the old rules, unchanged.
    let old = attempt_at(&armed, FENCE - 1).expect("an attempt admitted before the fence");
    let claim = old.claim(&id).expect("the old-rule claim exists");
    assert!(claim.escrowed_reward > 0 || claim.pwu > 0, "and carries its old reward / weight");
    // Only LG14-A armed: still no complete route.
    let half = route_sp(&p, FENCE, PalwG14RouteMirrorV1 { legacy_filer_from: Some(0), ..none });
    assert!(attempt_at(&half, FENCE).is_err(), "LG14-A without LG14-B is not the legacy route's G14 path");
    // LG14-A + LG14-B armed (and, for a class with fused-attention leaves, the court-capacity fence — lane g14-court-capacity): the
    // family regains reward with no other change.
    let routed = route_sp(
        &p,
        FENCE,
        PalwG14RouteMirrorV1 {
            legacy_filer_from: Some(0),
            legacy_held_da_from: Some(FENCE),
            court_capacity_from: Some(FENCE),
            ..none
        },
    );
    let s2 = attempt_at(&routed, FENCE).expect("the armed route restores admission");
    assert!(s2.claim(&id).is_some());
    palw_class_admits_claim_v1(&s, &routed, &extras(&p, FENCE), &floor, FENCE).expect("and the pre-check");
    // The same refusal for the free-prompt lane: a plain public V4 commitment, unframed, past the fence.
    let r = rig(p.clone());
    let fp_armed = route_sp(&r.p, FENCE, none);
    let commit = fp_commit(&r.s, r.floor, r.bond, r.pk.clone(), h(0xB1), Some(public_v4()));
    let refused = fold_sp(&r.p, &fp_armed, &r.s, &ctx(3, FENCE, 3, 0), &[commit.clone()], PalwBlockWorkV3::None, Hash64::default())
        .expect_err("a route-less FP commitment past the fence");
    assert_eq!(refusal_code(&refused), Some(PALW_G14_ROUTE_ABSENT_CODE_V1), "{refused:?}");
    // Lane FPTOK, through the fold (objects.rs): every route armed, the floor class registered no raw tokenizer material, so the
    // claim gate names FpTokenizer among the missing routes (G14_ROUTE_ABSENT). With every route armed except the tokenizer binding
    // the same family is named; the class's material is read from its class semantics, which a floor class does not have.
    let every = PalwG14RouteMirrorV1 {
        legacy_filer_from: Some(0),
        staged_court_from: Some(0),
        tir_conviction_from: Some(0),
        tokenizer_match_from: Some(0),
        ..none
    };
    assert!(r.s.class_semantics_v1(&r.floor).is_none(), "the floor class registers no tokenizer material");
    let no_material = fold_sp(
        &r.p,
        &route_sp(&r.p, FENCE, every),
        &r.s,
        &ctx(3, FENCE, 3, 0),
        &[commit.clone()],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect_err("no tokenizer material: refused through the fold");
    assert_eq!(refusal_code(&no_material), Some(PALW_G14_ROUTE_ABSENT_CODE_V1), "{no_material:?}");
    assert!(format!("{no_material:?}").contains("FpTokenizer"), "{no_material:?}");
    let s3 = fold_sp(&r.p, &fp_armed, &r.s, &ctx(3, FENCE - 1, 3, 0), &[commit], PalwBlockWorkV3::None, Hash64::default())
        .expect("admitted before the fence");
    assert!(s3.claim(&h(0xB1)).is_some());
    // Below every fence (testnet-12 as shipped): byte-identical — the mirror is `None`, the gate answers nothing.
    assert_eq!(sp_of(&palw_t12_shipped_params()).g14_route_required_v1(), None);
    assert_eq!(palw_g14_route_attempt_refusal_v1(&sp_of(&p), u64::MAX), None);
    // Params-level: the fence at DAA 14,000 mirrors 14,000 and reads the route fences' resolved heights.
    let mut t12 = palw_t12_shipped_params();
    t12.palw_g14_route_required_v1 = Some(ForkActivation::new(RELEASE));
    t12.sync_palw_g14_gates_v1();
    let mirror = sp_of(&t12).g14_route_required_v1().expect("mirrored");
    // Lane FPTOK: testnet-12's own tokenizer binding (if it has one) is mirrored, but it alone is no route.
    let tokenizer_match_from = t12.palw_fp_tokenizer_match_fence().map(|f| f.daa_score());
    assert_eq!(mirror, PalwG14RouteMirrorV1 { from_daa: RELEASE, tokenizer_match_from, ..none }, "testnet-12 arms no G14 route");
    assert!(palw_g14_route_attempt_refusal_v1(&sp_of(&t12), RELEASE).is_some());
    assert!(palw_g14_route_attempt_refusal_v1(&sp_of(&t12), RELEASE - 1).is_none());
}

/// **T-N10**: past the fence the G-05 job versions — constraint (6) and prefix (11 / 12), and every FP job's selection and tokenizer
/// — are refused at admission until their route is armed: constraint / prefix / tokenizer by the staged court with its legacy filer,
/// selection by the TIR public frame on the commitment itself. Non-public evidence (V5, PanelDa) has no route at all.
#[test]
fn legacy_fp_selection_and_constraint_jobs_are_refused_until_their_route_is_armed() {
    let p = palw_t12_launch_params_v1();
    let at = FENCE;
    let none = PalwG14RouteMirrorV1::default();
    let check = |routes: PalwG14RouteMirrorV1, c: &PalwFreePromptCommitmentV3, framed: bool| {
        palw_g14_route_fp_refusal_v1(&route_sp(&p, at, routes), at, Some(c), framed, true).map(|(code, _)| code)
    };
    // No route armed: every legacy FP job version is refused; constraint and prefix by the G-05 code.
    assert_eq!(check(none, &constraint_v6(), false), Some(PALW_G14_JOB_UNJUDGEABLE_CODE_V1));
    assert_eq!(check(none, &prefix_v11(), false), Some(PALW_G14_JOB_UNJUDGEABLE_CODE_V1));
    assert_eq!(check(none, &public_v4(), false), Some(PALW_G14_ROUTE_ABSENT_CODE_V1), "selection + tokenizer");
    assert_eq!(check(none, &v5_image(), true), Some(PALW_G14_ROUTE_ABSENT_CODE_V1), "non-public: no route can exist");
    assert_eq!(check(none, &panel_da(), true), Some(PALW_G14_ROUTE_ABSENT_CODE_V1));
    // The staged court with its legacy filer: constraint / prefix / tokenizer are judged, selection is not (the legacy FP lane's
    // selection proofs are refused on FP claims) — so still refused, now for selection alone.
    let staged = PalwG14RouteMirrorV1 { legacy_filer_from: Some(0), staged_court_from: Some(at), ..none };
    assert_eq!(check(staged, &constraint_v6(), false), Some(PALW_G14_ROUTE_ABSENT_CODE_V1));
    assert_eq!(check(staged, &prefix_v11(), true), Some(PALW_G14_ROUTE_ABSENT_CODE_V1), "framed, but no TIR route armed");
    // The staged court without its filer is not a route (G-18).
    let no_filer = PalwG14RouteMirrorV1 { staged_court_from: Some(at), tir_conviction_from: Some(at), ..none };
    assert_eq!(check(no_filer, &constraint_v6(), true), Some(PALW_G14_JOB_UNJUDGEABLE_CODE_V1));
    // Every route armed: a FRAMED job is admitted; the same job unframed is refused (the frame is the selection route).
    // Lane FPTOK: without the tokenizer binding the tokenizer family has no route, so every FP job stays refused.
    let unbound = PalwG14RouteMirrorV1 { tir_conviction_from: Some(at), ..staged };
    for c in [public_v4(), constraint_v6(), prefix_v11()] {
        assert!(check(unbound, &c, true).is_some(), "version {}: framed, tokenizer unbound", c.job.version);
    }
    let all = PalwG14RouteMirrorV1 { tokenizer_match_from: Some(at), ..unbound };
    for c in [public_v4(), constraint_v6(), prefix_v11()] {
        assert_eq!(check(all, &c, true), None, "version {}: framed, every route armed", c.job.version);
        assert!(check(all, &c, false).is_some(), "version {}: unframed", c.job.version);
    }
    assert!(check(all, &v5_image(), true).is_some() && check(all, &panel_da(), true).is_some(), "non-public never");
    // A route armed AFTER the claim's acceptance does not reach back: the route is read at acceptance.
    let late = PalwG14RouteMirrorV1 { tir_conviction_from: Some(at + 1), ..staged };
    assert!(check(late, &public_v4(), true).is_some());
    // Below the fence: nothing refused. No commitment shown: refused (fail closed).
    assert_eq!(palw_g14_route_fp_refusal_v1(&route_sp(&p, at + 1, none), at, Some(&constraint_v6()), false, true), None);
    assert!(palw_g14_route_fp_refusal_v1(&route_sp(&p, at, none), at, None, true, true).is_some());
    // Lane FPTOK: a class without registered raw tokenizer material has no tokenizer route. Every route armed: a registered class is
    // admitted, an unregistered one refused with G14_ROUTE_ABSENT.
    let tok = |routes: PalwG14RouteMirrorV1, from: u64, c: &PalwFreePromptCommitmentV3, has: bool| {
        palw_g14_route_fp_refusal_v1(&route_sp(&p, from, routes), at, Some(c), true, has).map(|(code, _)| code)
    };
    for c in [public_v4(), constraint_v6(), prefix_v11()] {
        assert_eq!(tok(all, at, &c, true), None, "version {}: registered tokenizer material", c.job.version);
        assert_eq!(tok(all, at, &c, false), Some(PALW_G14_ROUTE_ABSENT_CODE_V1), "version {}: none registered", c.job.version);
        // Below the fence, and while the tokenizer route is unarmed, the flag changes nothing.
        assert_eq!(tok(all, at + 1, &c, false), None, "version {}: below the fence", c.job.version);
        assert_eq!(tok(all, at + 1, &c, false), tok(all, at + 1, &c, true));
        assert_eq!(tok(unbound, at, &c, false), tok(unbound, at, &c, true), "version {}: tokenizer route unarmed", c.job.version);
        assert_eq!(tok(none, at, &c, false), tok(none, at, &c, true), "version {}: no route armed", c.job.version);
    }
    // The fold refuses the constraint commitment at the claim gate with the G-05 code, before any constraint-specific check.
    let r = rig(p);
    let commit = fp_commit(&r.s, r.floor, r.bond, r.pk.clone(), h(0xC6), Some(constraint_v6()));
    let refused =
        fold_sp(&r.p, &route_sp(&r.p, FENCE, none), &r.s, &ctx(3, FENCE, 3, 0), &[commit], PalwBlockWorkV3::None, Hash64::default())
            .expect_err("a G-05 job version past the fence");
    assert_eq!(refusal_code(&refused), Some(PALW_G14_JOB_UNJUDGEABLE_CODE_V1), "{refused:?}");
    // The families each job version needs (the table in `palw_g14_gates_v1`).
    use PalwG14RouteFamilyV1 as F;
    assert_eq!(palw_g14_fp_route_families_v1(&public_v4().job), vec![F::FpSelection, F::FpTokenizer]);
    assert_eq!(palw_g14_fp_route_families_v1(&constraint_v6().job), vec![F::FpSelection, F::FpConstraint, F::FpTokenizer]);
    assert_eq!(palw_g14_fp_route_families_v1(&prefix_v11().job), vec![F::FpSelection, F::FpPrefix, F::FpTokenizer]);
    assert_eq!(palw_g14_fp_route_families_v1(&v5_image().job), vec![F::NonPublicEvidence]);
    assert_eq!(palw_g14_fp_route_families_v1(&panel_da().job), vec![F::NonPublicEvidence]);
}

// ---- T-N15 (G-08, G-15) --------------------------------------------------------------------------------------------------------------

/// Commit, bind, license (Valid quorum) the floor FP claim under `sp`; returns the state and the licence DAA.
fn licensed_fp(r: &Rig, sp: &PalwStateParamsV2, claim: Hash64) -> (PalwChainStateV2, u64) {
    let commit = fp_commit(&r.s, r.floor, r.bond, r.pk.clone(), claim, Some(public_v4()));
    let s = fold_sp(&r.p, sp, &r.s, &ctx(3, 1_002, 3, 0), &[commit], PalwBlockWorkV3::None, Hash64::default()).expect("committed");
    let bonds = genesis_bonds(&r.p);
    let seats: Vec<PalwPanelSeatV2> = bonds[0..5].iter().map(|(k, o, _)| PalwPanelSeatV2 { bond: *k, operator_id: *o }).collect();
    let s = fold_sp(
        &r.p,
        sp,
        &s,
        &ctx(4, 1_003, 4, 0),
        &[PalwConsensusObjectV2::PanelBound { claim, anchor: h(0xA1), seats }],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("bound");
    let valid: Vec<PalwBondKeyV2> = bonds[0..3].iter().map(|(k, _, _)| *k).collect();
    let mut receipts = valid_receipts(claim, &valid);
    for r in &mut receipts {
        r.signed_daa = 1_004;
    }
    let s = fold_sp(
        &r.p,
        sp,
        &s,
        &ctx(5, 1_004, 5, 0),
        &[PalwConsensusObjectV2::ReceiptLicensed { claim, receipts }],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("licensed");
    (s, 1_004)
}

fn is_final(s: &PalwChainStateV2, claim: &Hash64) -> bool {
    matches!(s.claim(claim).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. }))
}

/// **T-N15**: past `palw_reward_liability_horizon_v1` a legacy FP claim stays licensed (voidable, no receipt spend, no coinbase leg)
/// until its liability horizon `licence + window_challenge + window_court`, then reaches Final; below the fence it Finals at the old
/// floor. A Gate C certified reward is held until `final + window_court` and named only by the sweep, once, if still certified.
#[test]
fn certified_and_final_rewards_are_held_until_the_liability_horizon() {
    let r = rig(palw_t12_launch_params_v1());
    let base = sp_of(&r.p);
    let wc = base.window_challenge_at(1_004);
    let court = base.window_court();
    eprintln!("[T-N15] testnet-12 launch: window_challenge_at(1004) = {wc}, window_court = {court}");
    // Below the fence: the old floor (the one producer, `palw_claim_final_floor_v1`).
    let claim = h(0xD1);
    let (s, l) = licensed_fp(&r, &base, claim);
    let c = s.claim(&claim).expect("live").clone();
    let old_final = palw_claim_final_floor_v1(&s, &base, &claim, &c, l).expect("floor");
    assert!(old_final >= l + wc);
    let swept = fold_sp(&r.p, &base, &s, &ctx(6, old_final + 1, 6, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("sweep");
    assert!(is_final(&swept, &claim), "the old rule Finals at its floor");
    // Past the fence: not Final at the old floor, no receipt spend there, Final only past the liability horizon.
    let held = base.clone().with_reward_liability_horizon_from_daa(Some(1_000));
    let (s, l) = licensed_fp(&r, &held, claim);
    let c = s.claim(&claim).expect("live").clone();
    let liability = l + wc + court;
    let new_final = palw_claim_final_floor_v1(&s, &held, &claim, &c, l).expect("floor");
    assert_eq!(new_final, old_final.max(liability), "the one floor reaches the liability horizon");
    assert!(new_final > old_final, "and moves it");
    eprintln!("[T-N15] licence {l}: old Final floor {old_final}, liability horizon floor {new_final}");
    let horizon = new_final;
    assert_eq!(palw_legacy_fp_liability_floor_v1(&held, &c, false, l), Some(liability));
    assert_eq!(palw_legacy_fp_liability_floor_v1(&held, &c, true, l), None, "an evaluation claim is user-paid, not held");
    assert_eq!(palw_legacy_fp_liability_floor_v1(&base, &c, false, l), None, "below the fence");
    let at_old = fold_sp(&r.p, &held, &s, &ctx(6, old_final + 1, 6, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("sweep");
    assert!(!is_final(&at_old, &claim), "past the fence the old floor no longer Finals it");
    let spend = kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendUnsignedV3 {
        version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        network_domain: h(0x0D05_0012),
        challenge: kaspa_consensus_core::palw_freeprompt_v3::spend_challenge_v3(
            h(0x0D05_0012),
            h(0xB0),
            1_700,
            7,
            claim,
            0,
            &r.bond.0,
        ),
        claim_id: claim,
        quantum_index: 0,
        beacon_block: h(0xBEAC),
        producer_bond: r.bond.0,
        producer_pubkey: r.pk.clone(),
    };
    assert!(
        fold_sp(&r.p, &held, &at_old, &ctx(7, old_final + 2, 7, 0), &[], PalwBlockWorkV3::ReceiptSpend(&spend), Hash64::default())
            .is_err(),
        "no receipt spend (no coinbase leg) before the liability horizon"
    );
    let before = fold_sp(&r.p, &held, &s, &ctx(6, horizon - 1, 6, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("sweep");
    assert!(!is_final(&before, &claim), "not Final the DAA before the horizon");
    let after = fold_sp(&r.p, &held, &s, &ctx(6, horizon + 1, 6, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("sweep");
    assert!(is_final(&after, &claim), "Final once the horizon has passed");
    // An attempt is not held by G-08 (its escrow vests to final + window_court already).
    let mut attempt = c.clone();
    attempt.source = kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::Attempt;
    assert_eq!(palw_legacy_fp_liability_floor_v1(&held, &attempt, false, l), None);
    // G-15: the certified reward's hold (Gate C cannot be armed in this binary, so its fold is asserted at the source and the rule here).
    let mut fin: PalwClaimStateV2 = c.clone();
    fin.phase = PalwClaimPhaseV2::Final { final_daa: horizon };
    assert_eq!(palw_certified_reward_held_until_v1(&held, &fin), Some(horizon + court));
    assert_eq!(palw_certified_reward_held_until_v1(&base, &fin), None, "below the fence: named at certification as before");
    assert_eq!(palw_certified_reward_held_until_v1(&held, &c), None, "not Final: nothing to hold yet");
    let fold = include_str!("../src/palw_security_certification_fold_v1.rs");
    assert!(fold.contains("palw_certified_reward_held_until_v1(builder.params, claim).is_some()"), "credit defers a governed reward");
    assert!(fold.contains("is_some_and(|until| now > until)"), "the sweep names it strictly past the horizon");
    assert!(fold.contains("row.held_reward = 0;"), "once");
    assert!(fold.contains("palw_independently_work_certified_v1("), "only while still certified");
    assert!(fold.contains("pub(super) fn release_deferred_certified_reward_at_retirement_v1("), "or at retirement, if earlier");
    let deadlines = include_str!("../src/palw_state_v2/deadlines.rs");
    assert!(
        deadlines.contains("release_deferred_certified_reward_at_retirement_v1(")
            && deadlines.find("release_deferred_certified_reward_at_retirement_v1(")
                < deadlines.find("builder.retire_claim(claim_id, &claim)?;"),
        "the deadline sweep names a deferred reward before the claim retires"
    );
}

// ---- G-09 (X3), G-16 (Kimi) ------------------------------------------------------------------------------------------------------

/// **G-09: the X3 candidate** derived from testnet-12's horizons — printed for the report, structurally sound, its unmeasured terms
/// named, consistent with G-08's floor, and still refused when armed (an unarmed placeholder).
#[test]
fn x3_candidate_is_derived_from_existing_horizons_and_stays_refused() {
    let t12 = palw_t12_shipped_params();
    let sp = sp_of(&t12);
    let f = PalwClaimHardDeadlineFenceV1::candidate_from_horizons_v1(ForkActivation::new(RELEASE), &sp);
    let pol = f.policy;
    let (bind, receipt, wc, court) = (sp.window_bind(), sp.window_receipt(), sp.window_challenge_at(RELEASE), sp.window_court());
    eprintln!(
        "[X3] testnet-12 at {RELEASE}: bind {bind}, receipt {receipt}, challenge {wc} (unshortened {}), court {court} -> T_prepare {}, \
         T_observe {}, T_resolve {}, hard - d0 = {}",
        sp.window_challenge(),
        pol.prepare_daa,
        pol.observe_daa,
        pol.resolve_daa,
        pol.prepare_daa + pol.observe_daa + pol.resolve_daa
    );
    pol.validate().expect("structurally sound");
    // B_disclose is the network's DA disclose window (`palw_da_disclose_window_daa_v1` = the unshortened `window_challenge`).
    let disclose = kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&sp);
    assert_eq!((pol.prepare_daa, pol.observe_daa, pol.resolve_daa), (bind + receipt, wc, disclose + court));
    assert_eq!(
        palw_x3_unmeasured_terms_v1(&pol),
        vec!["B_cold_fetch", "B_check", "B_localize", "B_carrier", "B_reorg_slack", "c_retain"]
    );
    let hard = pol.prepare_daa + pol.observe_daa + pol.resolve_daa;
    // (i) a pursuit at the open cutoff finishes at hard with the unmeasured terms at 0 (B_remaining = disclose + court).
    let mut x3 = sp.clone();
    x3 = x3.with_claim_hard_deadline_v1(Some(kaspa_consensus_core::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1 {
        from_daa: RELEASE,
        policy: pol,
    }));
    assert_eq!(kaspa_consensus_core::palw_ruling_fences_v1::palw_claim_pursuit_budget_v1(&x3, &pol), disclose + court);
    assert_eq!(pol.prepare_daa + pol.observe_daa + disclose + court, hard);
    // (ii) the extension beyond the producer's existing retention (REQ-X3-6, priced at the UNMEASURED c_retain).
    let retention = kaspa_consensus_core::palw_producer_v2::palw_min_trace_retention_daa_v1(&sp);
    eprintln!("[X3] existing retention {retention}, hard - d0 {hard}, extension {}", hard.saturating_sub(retention));
    assert!(hard >= retention);
    // (iii) never before G-08's liability floor for any licence inside the receipt window: L + wc + court <= d0 + bind + receipt + wc + court.
    assert!(hard >= bind + receipt + wc + court);
    let mut armed = t12.clone();
    armed.palw_claim_hard_deadline_v1 = Some(f);
    armed.sync_palw_2026_10_12_ruling_fences_v1();
    assert!(armed.validate_palw_2026_10_12_ruling_fences_v1().is_err(), "the candidate is a placeholder: arming stays refused");
}

/// **G-16: Kimi is out of scope in both places** — round 4 and consumer v2 both refuse a Kimi router at admission.
#[test]
fn kimi_is_out_of_scope_in_both_coverage_dispositions() {
    use kaspa_consensus_core::palw_cell_coverage_v1::{
        PalwCellDispositionV1, PalwCellUncoveredKindV1 as K, palw_cell_uncovered_disposition_v1, palw_cell_uncovered_disposition_v2,
    };
    assert_eq!(palw_cell_uncovered_disposition_v1(K::KimiRouter), PalwCellDispositionV1::RefusedAtAdmission);
    assert_eq!(palw_cell_uncovered_disposition_v2(K::KimiRouter), PalwCellDispositionV1::RefusedAtAdmission);
    assert!(matches!(palw_cell_uncovered_disposition_v2(K::MultiRouterTable), PalwCellDispositionV1::Covered(_)), "q36 unchanged");
}

// ---- G14 from genesis -------------------------------------------------------------------------------------------------------------

/// **g14-genesis validates, and only on its isolated genesis**: the same fences on any shipped preset's genesis are refused.
#[test]
fn g14_genesis_ruleset_validates_and_its_exception_reaches_no_shipped_preset() {
    let g = palw_g14_genesis_test_params_v1();
    assert!(g.has_isolated_g14_genesis_v1());
    g.validate_palw_v2().unwrap_or_else(|e| panic!("g14-genesis validates: {e:?}"));
    for (name, p) in [
        ("testnet-12", palw_t12_shipped_params()),
        ("testnet-12 launch", palw_t12_launch_params_v1()),
        ("testnet-11 rc", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("MAINNET_PARAMS", MAINNET_PARAMS),
    ] {
        assert!(!p.has_isolated_g14_genesis_v1(), "{name}: no shipped genesis carries the marker");
    }
    // The g14-genesis fences on testnet-12's own genesis: refused (no record pinned), the filer by its own gate first.
    let mut same_on_t12 = g.clone();
    same_on_t12.genesis = palw_t12_shipped_params().genesis;
    assert!(!same_on_t12.has_isolated_g14_genesis_v1());
    assert!(same_on_t12.validate_palw_v2().is_err());
    // A tampered isolated genesis (payload changed, hash kept) is not isolated.
    let mut forged = g.clone();
    forged.genesis.nonce ^= 1;
    assert!(!forged.has_isolated_g14_genesis_v1(), "the header must hash to the genesis");
    // Every G14 fence the ruleset carries is at block 0, and the mirrors agree. The two ruling fences (D2, X3) are not part of G14-from-genesis.
    for fence in PalwG14GatedFenceV1::ALL
        .into_iter()
        .filter(|f| !matches!(f, PalwG14GatedFenceV1::CourtModelScopeV1 | PalwG14GatedFenceV1::ClaimHardDeadlineV1))
    {
        assert_eq!(fence.armed(&g), Some(ForkActivation::new(0)), "{fence:?}");
    }
    assert_eq!(g.palw_legacy_public_filer_v1, Some(ForkActivation::new(0)));
    // Lane g14-close-legacy: LG14-B is armable behind its record now, so G14-from-genesis arms it too (the legacy Panel route's G14
    // route is complete from block 0).
    assert_eq!(g.palw_legacy_held_da_v2, Some(ForkActivation::new(0)), "LG14-B from block 0");
    assert_eq!(g.palw_claim_hard_deadline_v1, None, "X3 has no approved value");
    let mirror = sp_of(&g).g14_route_required_v1().expect("mirrored");
    assert_eq!(mirror.from_daa, 0);
    eprintln!("[g14-genesis] route mirror {mirror:?}; TIR conviction resolves at {:?}", g.palw_tir_public_conviction_v1_fence());
    assert_eq!(sp_of(&g).reward_liability_horizon_from_daa(), Some(0));
    assert_eq!(sp_of(&g).public_evidence_reward_from_daa(), Some(0));
}

/// **From block 0 on g14-genesis**: the legacy Panel route pays from block 0 because its G14 route (LG14-A + LG14-B, lane
/// g14-close-legacy) is armed there; non-public material never pays (V5 image, PanelDa), an unframed FP job never pays, and only a
/// family whose G14 route is armed (a framed FP job once the TIR route has resolved) is admitted. Nothing reads pre-fence history: at
/// fence 0 every claim is governed.
#[test]
fn g14_genesis_from_block_zero_only_families_with_an_armed_route_earn() {
    let g = palw_g14_genesis_test_params_v1();
    let sp = sp_of(&g);
    let mirror = sp.g14_route_required_v1().expect("armed");
    assert!(mirror.governs(0), "fence 0: no claim is pre-fence");
    assert_eq!((mirror.legacy_filer_from, mirror.legacy_held_da_from), (Some(0), Some(0)), "the legacy route's G14 path from block 0");
    // The legacy Panel route pays: its family has an armed route at every height.
    for daa in [0, 1, 1_000, u64::MAX / 2] {
        assert!(palw_g14_route_attempt_refusal_v1(&sp, daa).is_none(), "an attempt at {daa}");
    }
    let s = genesis_state(&g);
    let (floor, ..) = genesis_classes(&g)[0];
    palw_class_admits_claim_v1(&s, &sp, &extras(&g, 1), &floor, 1).expect("the producer's pre-check at DAA 1");
    // The same ruleset WITHOUT LG14-B (its gates-lane form): the legacy Panel route never pays.
    let mut half = g.clone();
    half.palw_legacy_held_da_v2 = None;
    half.sync_palw_g14_gates_v1();
    half.validate_palw_v2().unwrap_or_else(|e| panic!("g14-genesis without LG14-B validates: {e:?}"));
    let half_sp = sp_of(&half);
    for daa in [0, 1, 1_000, u64::MAX / 2] {
        assert!(palw_g14_route_attempt_refusal_v1(&half_sp, daa).is_some(), "an attempt at {daa} without LG14-B");
    }
    let pre = palw_class_admits_claim_v1(&s, &half_sp, &extras(&half, 1), &floor, 1).expect_err("the producer's pre-check at DAA 1");
    assert_eq!(refusal_code(&pre), Some(PALW_G14_ROUTE_ABSENT_CODE_V1), "{pre:?}");
    // Non-public material never pays; an unframed public job never pays.
    for c in [v5_image(), panel_da(), public_v4(), constraint_v6(), prefix_v11()] {
        for daa in [0, 1, 1_000] {
            assert!(
                palw_g14_route_fp_refusal_v1(&sp, daa, Some(&c), false, true).is_some(),
                "version {} unframed at {daa}",
                c.job.version
            );
        }
        assert!(
            palw_g14_route_fp_refusal_v1(&sp, 0, Some(&c), true, true).is_some() || mirror.tir_conviction_from == Some(0),
            "framed at 0 only if the TIR route resolved at 0"
        );
    }
    for c in [v5_image(), panel_da()] {
        assert!(palw_g14_route_fp_refusal_v1(&sp, u64::MAX / 2, Some(&c), true, true).is_some(), "non-public never, framed or not");
    }
    // Only families with an armed route earn: a framed public FP job once every route it needs is in force.
    let tir = mirror.tir_conviction_from.expect("the TIR route resolves on g14-genesis");
    // Lane FPTOK: the tokenizer family also needs the job ↔ class tokenizer binding in force.
    let tok = mirror.tokenizer_match_from.expect("the tokenizer binding resolves on g14-genesis");
    if tok > tir {
        assert!(
            palw_g14_route_fp_refusal_v1(&sp, tok - 1, Some(&public_v4()), true, true).is_some(),
            "framed, tokenizer still unbound"
        );
    }
    let tir = tir.max(tok);
    for c in [public_v4(), constraint_v6(), prefix_v11()] {
        assert_eq!(palw_g14_route_fp_refusal_v1(&sp, tir, Some(&c), true, true), None, "version {} framed at {tir}", c.job.version);
    }
    // The liability horizon governs every claim from block 0.
    assert!(sp.reward_liability_horizon_governs_v1(0));
}

// ---- testnet-12's identity -------------------------------------------------------------------------------------------------------

/// The ruleset testnet-12 LAUNCHED on (`post_launch_fence_arming_tests::T12_LAUNCH_IDS`, the lib's pin): adding the two dormant fences
/// (Some-only in every hash) must leave it — and every shipped preset's validation — exactly as it was.
#[test]
fn testnet12_identity_did_not_move() {
    let launch = palw_t12_launch_params_v1();
    assert_eq!(
        (
            launch.consensus_params_id().to_string(),
            launch.consensus_identity_id().to_string(),
            launch.consensus_schedule_id().to_string()
        ),
        (
            "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f".to_string(),
            "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5".to_string(),
            "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd".to_string(),
        )
    );
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    assert_eq!(t12.palw_g14_route_required_v1, None);
    assert_eq!(t12.palw_reward_liability_horizon_v1, None);
    let PalwConsensusMode::ConsensusV2(b) = &t12.palw_consensus_mode else { panic!("V2") };
    assert_eq!(b.state.g14_route_required_v1(), None);
    assert_eq!(b.state.reward_liability_horizon_from_daa(), None);
    eprintln!(
        "[t12 ids] params {} identity {} schedule {}",
        t12.consensus_params_id(),
        t12.consensus_identity_id(),
        t12.consensus_schedule_id()
    );
}
