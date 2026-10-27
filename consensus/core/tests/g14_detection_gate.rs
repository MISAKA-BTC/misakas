//! **G14 detection policy at the claim gate** (`palw_detection_policy_v1`; `docs/design/palw/g14-detection-policy-2026-10-17.md` §10).
//!
//! The fold fixture is testnet-12's own (`dos_l5_common`, as `g14_close_gates`): the real `apply_palw_transition_v7` on the shipped
//! bundle, with the dormant fence's MIRROR set on the bundle's state params exactly as `Params::sync_palw_detection_policy_v1` writes
//! it (validation refuses arming, so a fixture arms the mirror the fold reads). `FENCE` stands for the release's DAA 16,000.
//!
//! Past the fence a claim — attempt or free-prompt — is refused exactly as G-01 option (b) refuses (`ClassNotRewardable`, no claim,
//! no reward, weight or credit) when its class has no policy, when its collectable collateral is below the policy's deterrence bound,
//! or when the policy cannot check it within its resources; a claim the policy admits is taken as before. Below the fence the fold is
//! byte-identical with or without the mirror. Restart and reorg: the gate is a pure function of the parent state, the block and the
//! mirror, so re-folding gives the same state and the same refusal on every branch.

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_launch_params_v1, palw_t12_shipped_params};
use kaspa_consensus_core::palw_detection_policy_v1::*;
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwCertifiedFamilyStateV2, PalwCertifiedLaneV1, PalwChainStateV2,
    PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v7,
};

const FENCE: u64 = 1_001;
const PROBE: u64 = 0xDE_0009;

fn sp_of(p: &Params) -> PalwStateParamsV2 {
    bundle(p).state
}

fn fp_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    e.fp_derived_work_daa = p.palw_fp_derived_work_fence().map(|f| f.daa_score());
    e.fp_da_pins_active = p.palw_fp_da_pins_fence().is_some_and(|f| f.is_active(daa));
    e
}

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

fn code(e: &PalwStateV2Error) -> Option<&'static str> {
    match e {
        PalwStateV2Error::ClassNotRewardable { code, .. } => Some(code),
        _ => None,
    }
}

fn carriage(s: &PalwChainStateV2) -> Vec<u8> {
    borsh::to_vec(&PalwStateCarriageV2::from_state(s)).expect("carriage encodes")
}

/// The floor class's policy, with the attempt lane's N and a cost per interval scaled to `reserved` (the claim's collateral).
fn floor_policy(class: Hash64, cost_per_interval: u64) -> PalwDetectionPolicyV1 {
    let mut p =
        palw_detection_genesis_design_v1(class, PalwDetectionRouteV1::Base0A16, PalwIntervalCountRuleV1::HeldPositions { width: 16 });
    p.attempt_intervals = 64;
    p.economics.cost_per_interval_sompi = cost_per_interval;
    p.economics.collateral_per_interval_sompi = cost_per_interval.saturating_mul(2);
    p
}

fn armed(p: &Params, policies: Vec<PalwDetectionPolicyV1>) -> PalwStateParamsV2 {
    sp_of(p).with_detection_policy_v1(Some(PalwDetectionPolicyFenceV1 { activation: ForkActivation::new(FENCE), policies }))
}

struct AttemptRig {
    p: Params,
    s: PalwChainStateV2,
    floor: Hash64,
    env: kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2,
    key: Hash64,
    id: Hash64,
    /// The claim's collateral under the old rules (read from a pre-fence admission).
    reserved: u128,
}

fn attempt_rig() -> AttemptRig {
    let p = palw_t12_launch_params_v1();
    let (floor, leaves, target, _) = genesis_classes(&p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    let g = genesis_state(&p);
    let s = fold_sp(
        &p,
        &sp_of(&p),
        &g,
        &ctx(0xDE00_0001, 1_000, 1, 0),
        &[bond_obj(PROBE, 1_000_000_000_000_000)],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the probe bond registers");
    let (env, key, id) =
        junk_attempt(floor, bond_key(PROBE), pubkey_of(PROBE), &operator_pubkey_of(PROBE), pwu, 0xDE_5EED, 0xDE_0000_5EED);
    let old =
        fold_sp(&p, &sp_of(&p), &s, &ctx(0xDE00_0002, FENCE, 2, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .expect("the attempt is admitted without the fence");
    let reserved = old.claim(&id).expect("claim").reserved;
    assert!(reserved > 0, "the fixture's claim reserves collateral");
    AttemptRig { p, s, floor, env, key, id, reserved }
}

impl AttemptRig {
    fn attempt(&self, sp: &PalwStateParamsV2, daa: u64, block: u64) -> Result<PalwChainStateV2, PalwStateV2Error> {
        fold_sp(&self.p, sp, &self.s, &ctx(block, daa, 2, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&self.env), self.key)
    }
}

/// **Attempt lane, past the fence**: no policy → refused; collateral below the bound → refused; a k the resources cannot carry →
/// refused; an adequate policy → admitted exactly as without the fence. Every refusal is option (b)'s: no claim is written.
#[test]
fn an_attempt_past_the_fence_is_admitted_only_under_a_policy_its_collateral_and_resources_satisfy() {
    let r = attempt_rig();
    // N = 64 attempt intervals; S_required = 1.1 · 64 · c (q = 1, shadow 1%: q_eff = 1).
    let affordable = u64::try_from(r.reserved / (64 * 2)).unwrap().max(1);
    let unaffordable = u64::try_from(r.reserved).unwrap();
    // No policy for the class.
    let other = armed(&r.p, vec![floor_policy(h(0xC0_FFEE), affordable)]);
    let e = r.attempt(&other, FENCE, 0xDE00_0002).expect_err("no policy for the class");
    assert_eq!(code(&e), Some("DETECTION_NO_POLICY_FOR_CLASS"), "{e:?}");
    // The deterrence bound on the producer's actual collateral.
    let poor = armed(&r.p, vec![floor_policy(r.floor, unaffordable)]);
    let e = r.attempt(&poor, FENCE, 0xDE00_0002).expect_err("collateral below the bound");
    assert_eq!(code(&e), Some("DETECTION_COLLATERAL_BELOW_DETERRENCE_BOUND"), "{e:?}");
    // Resources: the legacy 512-pair pool cannot hold k · 32 claims.
    let mut tight = floor_policy(r.floor, affordable);
    tight.resources.retention_pairs_cap = 512;
    let e = r.attempt(&armed(&r.p, vec![tight]), FENCE, 0xDE00_0002).expect_err("retention below the draw");
    assert_eq!(code(&e), Some("DETECTION_RETENTION_BELOW_DRAW"), "{e:?}");
    // A class policy with no attempt N refuses the attempt lane.
    let mut no_attempt = floor_policy(r.floor, affordable);
    no_attempt.attempt_intervals = 0;
    let e = r.attempt(&armed(&r.p, vec![no_attempt]), FENCE, 0xDE00_0002).expect_err("no attempt N");
    assert_eq!(code(&e), Some("DETECTION_NO_INTERVALS"), "{e:?}");
    // Adequate: admitted, and the claim is the one the old rules write.
    let ok = r.attempt(&armed(&r.p, vec![floor_policy(r.floor, affordable)]), FENCE, 0xDE00_0002).expect("admitted under its policy");
    let none = r.attempt(&sp_of(&r.p), FENCE, 0xDE00_0002).expect("admitted without the fence");
    assert_eq!(ok.claim(&r.id), none.claim(&r.id));
    assert_eq!(carriage(&ok), carriage(&none), "an admitted claim folds byte-identically");
}

/// **Below the fence the fold is byte-identical** with or without the mirror (and with a mirror that would refuse everything).
#[test]
fn below_the_fence_the_fold_is_byte_identical() {
    let r = attempt_rig();
    let refuse_all = armed(&r.p, vec![]);
    let with = r.attempt(&refuse_all, FENCE - 1, 0xDE00_0002).expect("below the fence");
    let without = r.attempt(&sp_of(&r.p), FENCE - 1, 0xDE00_0002).expect("no mirror");
    assert_eq!(with, without);
    assert_eq!(carriage(&with), carriage(&without));
    // A never() fence collapses to None at the Params level, so the shipped bundle never carries a mirror.
    let mut t12 = palw_t12_shipped_params();
    assert!(sp_of(&t12).detection_policy_v1().is_none());
    t12.palw_detection_policy_v1 = Some(PalwDetectionPolicyFenceV1::at(ForkActivation::never()));
    t12.sync_palw_detection_policy_v1();
    assert!(sp_of(&t12).detection_policy_v1().is_none());
    assert!(t12.validate_palw_detection_policy_v1().is_ok());
    // An armed height mirrors, and an un-synced bundle is refused by name.
    let mut armed_p = palw_t12_shipped_params();
    armed_p.palw_detection_policy_v1 = Some(PalwDetectionPolicyFenceV1::at(ForkActivation::new(16_000)));
    assert!(armed_p.validate_palw_detection_policy_v1().unwrap_err().to_string().contains("mirror"));
    armed_p.sync_palw_detection_policy_v1();
    assert_eq!(sp_of(&armed_p).detection_policy_v1().map(|f| f.activation), Some(ForkActivation::new(16_000)));
    assert!(armed_p.validate_palw_detection_policy_v1().unwrap_err().to_string().contains("cannot be armed"));
}

/// **Restart and reorg**: the gate reads only the parent state, the block and the mirror. Re-folding the same block gives the same
/// state (restart); folding a sibling block and then the original again gives the original's state and the original's refusal
/// (reorg) — on every branch, the same verdict.
#[test]
fn the_gate_is_deterministic_across_restart_and_reorg() {
    let r = attempt_rig();
    let affordable = u64::try_from(r.reserved / (64 * 2)).unwrap().max(1);
    let good = armed(&r.p, vec![floor_policy(r.floor, affordable)]);
    let poor = armed(&r.p, vec![floor_policy(r.floor, u64::try_from(r.reserved).unwrap())]);
    let a = r.attempt(&good, FENCE, 0xDE00_00A1).expect("branch A admits");
    let a_again = r.attempt(&good, FENCE, 0xDE00_00A1).expect("restart re-folds");
    assert_eq!(carriage(&a), carriage(&a_again));
    // A sibling block on the same parent (another block id and DAA), then the original again.
    let b = r.attempt(&good, FENCE + 5, 0xDE00_00B2).expect("branch B admits");
    assert!(b.claim(&r.id).is_some());
    let back = r.attempt(&good, FENCE, 0xDE00_00A1).expect("reorg back to A");
    assert_eq!(carriage(&back), carriage(&a));
    // The refusal is the same on both branches.
    let ea = r.attempt(&poor, FENCE, 0xDE00_00A1).expect_err("refused on A");
    let eb = r.attempt(&poor, FENCE + 5, 0xDE00_00B2).expect_err("refused on B");
    assert_eq!(code(&ea), code(&eb));
    assert_eq!(format!("{ea:?}"), format!("{eb:?}"));
}

// ---- the free-prompt lane -------------------------------------------------------------------------------------------------------

fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

fn seed_prereqs(p: &Params, s: &PalwChainStateV2) -> PalwChainStateV2 {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, palw_lifecycle_profile_v1,
    };
    let (floor, _l, target, _sv) = genesis_classes(p)[0];
    let fam = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_fp_certified_families_v1()
        .into_iter()
        .find(|f| f.drilled_class_id == floor_profile().shape_profile_id())
        .expect("BASE-0 fp family is pinned");
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    let work = kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives");
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

fn fp_commit(s: &PalwChainStateV2, class_id: Hash64, bond: PalwBondKeyV2, pk: Vec<u8>, claim: Hash64) -> PalwConsensusObjectV2 {
    let profile = floor_profile();
    let ladder = s.class_step_ladder_v1(&class_id, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP);
    let (pt, dc) = (512u32, 256u32);
    let prompt: Vec<u32> = (0..pt).map(|i| i + (claim.as_bytes()[0] as u32)).collect();
    let work_leaves =
        kaspa_consensus_core::palw_step::step_leaf_count_of_tokens_capped_v1(&profile, pt, dc, ladder).expect("floor leaves derive");
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: Hash64::default(),
        public_job: None,
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

/// **Free-prompt lane, past the fence**: N is the held rule over the commitment's own counts (bound to its priced work by the
/// derived-work rule), and the same refusals apply; an adequate policy admits the commitment as before.
#[test]
fn a_free_prompt_commitment_past_the_fence_obeys_the_same_gate() {
    let p = palw_t12_launch_params_v1();
    let (floor, ..) = genesis_classes(&p)[0];
    let attacker = 1u64;
    let g = genesis_state(&p);
    let s = fold_sp(
        &p,
        &sp_of(&p),
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
        fold_sp(&p, &sp_of(&p), &s, &ctx(2, 1_000, 2, 0), &[publish], PalwBlockWorkV3::None, Hash64::default()).expect("published");
    let commit = fp_commit(&s, floor, bond_key(attacker), pubkey_of(attacker), h(0xB1));
    let fold_at = |sp: &PalwStateParamsV2, daa: u64| {
        fold_sp(&p, sp, &s, &ctx(3, daa, 3, 0), &[commit.clone()], PalwBlockWorkV3::None, Hash64::default())
    };
    let old = fold_at(&sp_of(&p), FENCE).expect("admitted without the fence");
    let reserved = old.claim(&h(0xB1)).expect("the FP claim").reserved;
    assert!(reserved > 0);
    // N = ⌈(512 + 255) / 16⌉ = 48 under the held rule; S_required = 1.1 · 48 · c.
    let affordable = u64::try_from(reserved / (48 * 2)).unwrap().max(1);
    let e = fold_at(&armed(&p, vec![]), FENCE).expect_err("no policy");
    assert_eq!(code(&e), Some("DETECTION_NO_POLICY_FOR_CLASS"), "{e:?}");
    let e = fold_at(&armed(&p, vec![floor_policy(floor, u64::try_from(reserved).unwrap())]), FENCE).expect_err("collateral");
    assert_eq!(code(&e), Some("DETECTION_COLLATERAL_BELOW_DETERRENCE_BOUND"), "{e:?}");
    let mut short = floor_policy(floor, affordable);
    short.n_max = 47;
    let e = fold_at(&armed(&p, vec![short]), FENCE).expect_err("N above n_max");
    assert_eq!(code(&e), Some("DETECTION_N_ABOVE_N_MAX"), "{e:?}");
    let ok = fold_at(&armed(&p, vec![floor_policy(floor, affordable)]), FENCE).expect("admitted under its policy");
    assert_eq!(carriage(&ok), carriage(&old));
    // Below the fence: byte-identical even with a refuse-all mirror.
    let below = fold_at(&armed(&p, vec![]), FENCE - 1).expect("below the fence");
    let below_none = fold_at(&sp_of(&p), FENCE - 1).expect("no mirror");
    assert_eq!(carriage(&below), carriage(&below_none));
}

// ---- lane g14-court-capacity-r2: the court's per-claim reservation at the claim gate (detection-policy §10 item 1) ----------------

/// **The claim gate checks the policy's declared court/DA needs against what the court actually reserves for the claim**
/// (`palw_court_claim_budget_v1`, past `palw_court_capacity_v1`): the draw must be compellable on ONE reservation's own budget
/// (34 reserved DA sessions here), and the rounds must finish before the last guaranteed first move (`hard − W` for a one-move
/// class). A policy that declares more than that is admitted below the court fence (its declared values) and refused past it
/// (`DETECTION_COURT_BUDGET_BELOW_DRAW`); a claim whose retention leaves no guaranteed first move is refused
/// (`DETECTION_DEADLINE_UNREACHABLE`). The fold's attempt gate reads the same budget.
#[test]
fn the_claim_gate_checks_the_policys_needs_against_the_courts_per_claim_reservation() {
    use kaspa_consensus_core::palw_court_capacity_v1::*;
    let r = attempt_rig();
    let affordable = u64::try_from(r.reserved / (64 * 2)).unwrap().max(1);
    let court = PalwCourtCapacityMirrorV1 { from_daa: 0, policy: palw_court_capacity_placeholder_policy_v1() };
    let facts = PalwDetectionClaimFactsV1 {
        attempt_lane: true,
        work_leaves: 0,
        context_priced_leaves: None,
        prompt_tokens: 0,
        decode_tokens_executed: 0,
        collateral_sompi: r.reserved,
    };
    // A policy whose draw needs more openings than one reservation compels: a tight ε, a raised k ceiling, and declared resources
    // (court openings, bytes, retention, fetch, replay) large enough that only the court's actual reservation binds.
    let mut wide = floor_policy(r.floor, affordable);
    wide.epsilon = PalwRatioV1 { num: 1, den: 1_000_000 };
    wide.k_max = 64;
    wide.resources.court_openings_per_claim = 64;
    wide.resources.da_bytes_per_claim = u64::MAX / 4;
    wide.resources.retention_pairs_cap = u32::MAX;
    wide.resources.max_claim_fetch_bytes = u64::MAX / 4;
    wide.resources.max_claim_replay_units = u64::MAX / 4;
    let plan = wide.plan_v1(&facts).expect("the declared values admit the wide draw");
    eprintln!("[court budget] wide policy: N {}, k_round {}, rounds {}", plan.n, plan.k_round, plan.rounds);
    let sp = armed(&r.p, vec![wide]);
    let with_court = sp.clone().with_court_capacity_v1(Some(court));
    // The budget: below the court fence none; past it, one reservation's own sessions and the claim's own hard deadline.
    assert_eq!(palw_court_claim_budget_for_retention_v1(&r.s, &sp, &r.floor, FENCE, 999_999), None);
    let budget = palw_court_claim_budget_for_retention_v1(&r.s, &with_court, &r.floor, FENCE, 999_999).expect("a budget");
    let fused = r.s.class(&r.floor).is_some_and(|c| c.fused_attention);
    let w = court.policy.admission_bucket_daa(with_court.turn_deadline_daa(), with_court.window_court());
    let hard = 999_999 - kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&with_court);
    assert_eq!(budget.compellable_openings, 34, "one reservation's own reserved DA sessions");
    assert_eq!(budget.reserved_sessions, u32::from(fused));
    assert_eq!(budget.first_move_by_daa, hard - if fused { 4 * w } else { w });
    let producer_default = palw_court_claim_budget_v1(&r.s, &with_court, &r.floor, FENCE).expect("a budget");
    eprintln!(
        "[court budget] floor class (fused {fused}): {budget:?}; at the producer's minimum retention the last guaranteed first move is \
         DAA {} (accepted {FENCE})",
        producer_default.first_move_by_daa
    );
    let gate = |sp: &PalwStateParamsV2, retention: u64| {
        palw_detection_claim_refusal_with_budget_v1(
            sp,
            &r.floor,
            FENCE,
            &facts,
            palw_court_claim_budget_for_retention_v1(&r.s, sp, &r.floor, FENCE, retention),
        )
        .map(|(code, _)| code)
    };
    if plan.k_round > budget.compellable_openings {
        assert_eq!(gate(&sp, 999_999), None, "below the court fence: the declared values admit it");
        assert_eq!(
            gate(&with_court, 999_999),
            Some("DETECTION_COURT_BUDGET_BELOW_DRAW"),
            "past it: the reservation cannot compel the draw"
        );
        // The fold's attempt gate reads the same budget.
        let e = r.attempt(&with_court, FENCE, 0xDE00_0002).expect_err("the attempt gate refuses it too");
        assert_eq!(code(&e), Some("DETECTION_COURT_BUDGET_BELOW_DRAW"), "{e:?}");
        r.attempt(&sp, FENCE, 0xDE00_0002).expect("admitted on the declared values below the court fence");
    } else {
        panic!("the fixture's wide policy draws only {} per round; it must exceed one reservation's 34 sessions", plan.k_round);
    }
    // A policy within the reservation is admitted past the court fence on a retention that leaves its rounds time …
    let fits = armed(&r.p, vec![floor_policy(r.floor, affordable)]).with_court_capacity_v1(Some(court));
    assert_eq!(gate(&fits, 999_999), None, "within one reservation's budget, before the last guaranteed first move");
    // … and refused when the claim's retention leaves no guaranteed first move after its acceptance.
    let short = FENCE + kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&fits) + w / 2;
    assert_eq!(gate(&fits, short), Some("DETECTION_DEADLINE_UNREACHABLE"), "no time to prosecute: not reward-bearing");
}
