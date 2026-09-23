//! PALW slashing-coverage pins — fences, taxonomy, and the reserved-vs-reward inequality.
//!
//! These tests do not slash anyone. They freeze the facts a slash-coverage audit has to keep
//! true: which fences are live on the shipped RC card, which offences PALW-v2 actually debits,
//! and whether `claim.reserved` (the `void_and_slash` amount) can cover one claim's payout.
//! PoCs are unit/property only; no network attack.

use kaspa_consensus_core::config::params::{
    palw_rc_shipped_params, ForkActivation, PALW_RC_DA_COURT_FENCE_DAA, PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA,
    MAINNET_PARAMS,
};
use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_canonical_work_v1::{palw_canonical_draw_work_v1, PalwCanonicalClassDescriptorV1};
use kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5;
use kaspa_consensus_core::palw_economic_compute_v1::{
    palw_attempt_economic_compute_v1, palw_attempted_compute_per_claim_v1, PALW_ECONOMIC_COST_TABLE_V1,
};
use kaspa_consensus_core::palw_economics_ledger_v1::palw_rate_priced_reward_v1;
use kaspa_consensus_core::palw_freeprompt_v3::fp_work_id_v1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_GRAPH_V5_N_CTX;
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};
use kaspa_hashes::Hash64;

const RATE_SOMPI_PER_GIGA: u64 = 900_000_000;
const ESCROW_SOMPI: u64 = 320_084_640_000;
const SLASH_VALUE_PER_PWU: u64 = 5;

fn dense_512() -> kaspa_consensus_core::palw_step::PalwShapeProfileV3 {
    palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).expect("the shipped @512 dense row projects")
}

/// The shipped RC card: DA court live, D4 from genesis, objective offence scheduled at 8,500.
/// Eligibility is `available >= required(claim)`, not a raised 400k floor.
#[test]
fn shipped_slash_coverage_fences_are_the_ones_the_audit_scored() {
    let rc = palw_rc_shipped_params();
    assert_eq!(rc.palw_canonical_work, None, "the economic bundle is not scheduled on RC");
    assert_eq!(rc.palw_fp_derived_work, None);
    assert_eq!(rc.palw_admission_independence, None);
    assert_eq!(rc.palw_operator_id_unique, None, "Sybil volume is still priced at another bond, not another proven key");
    assert_eq!(
        rc.palw_objective_offence,
        Some(ForkActivation::new(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA)),
        "the lock ledger is one future bundle on testnet-11"
    );
    assert_eq!(MAINNET_PARAMS.palw_objective_offence, None, "mainnet does not schedule the debit");
    assert_eq!(rc.palw_unavailable_abstains, Some(ForkActivation::always()), "D4 is genesis-true: sampled Unavailable is not a slash");
    assert_eq!(rc.palw_da_court, Some(ForkActivation::new(PALW_RC_DA_COURT_FENCE_DAA)));
    let PalwConsensusMode::ConsensusV2(_) = rc.palw_consensus_mode else {
        panic!("shipped RC is ConsensusV2");
    };
}

/// Historical per-bond work ids still name the liable party. Two bonds of one prompt are two ids.
#[test]
fn historical_work_id_names_the_bond() {
    let class = Hash64::from_u64_word(0xC1);
    let prompt = Hash64::from_u64_word(0x90);
    let bond_a = kaspa_consensus_core::tx::TransactionOutpoint {
        transaction_id: kaspa_consensus_core::TransactionId::from_u64_word(1),
        index: 0,
    };
    let bond_b = kaspa_consensus_core::tx::TransactionOutpoint {
        transaction_id: kaspa_consensus_core::TransactionId::from_u64_word(2),
        index: 0,
    };
    assert_ne!(fp_work_id_v1(&class, &prompt, &bond_a), fp_work_id_v1(&class, &prompt, &bond_b));
}

/// `void_and_slash` takes `claim.reserved = exposure_pwu × slash_value_per_pwu` (5 sompi on
/// genesis classes). One dense-row claim's ADR-0132 payout is the competing number. If payout
/// exceeds reserved, an unchallenged fraud that reaches Final is a credit error P3 accepts —
/// the challenge window is the slash window, and a late verdict against a terminal claim
/// closes the session without a second debit.
#[test]
fn reserved_slash_versus_one_claim_payout_on_the_shipped_dense_row() {
    let dense = dense_512();
    let PalwConsensusMode::ConsensusV2(bundle) = palw_rc_shipped_params().palw_consensus_mode else {
        panic!("shipped RC is ConsensusV2");
    };
    let ladder = bundle.court.max_step_leaf_count();
    let job = rc_job_context(&dense, 63, 2);
    let declared_leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&dense, &job, ladder).unwrap();
    let reserved = (declared_leaves as u128).saturating_mul(SLASH_VALUE_PER_PWU as u128);

    let executed = palw_attempt_economic_compute_v1(&dense, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("draw walks");
    let target = palw_work_ticket_target_v1(executed, palw_work_floor_v1(ESCROW_SOMPI, RATE_SOMPI_PER_GIGA));
    let attempted = palw_attempted_compute_per_claim_v1(palw_expected_attempts_v1(target), executed);
    let payout = palw_rate_priced_reward_v1(ESCROW_SOMPI, attempted, RATE_SOMPI_PER_GIGA as u128) as u128;

    let descriptor = PalwCanonicalClassDescriptorV1::of(&dense, Hash64::default()).expect("one weight format");
    let derived = palw_canonical_draw_work_v1(&descriptor, &job, true).expect("draw derives").provisional_scalar_v1();
    let derived_reserved = derived.saturating_mul(SLASH_VALUE_PER_PWU as u128);

    // Pin the numbers the audit quotes. A change to slash_value_per_pwu or the payout rate
    // must re-score whether a caught fraud still loses more than it earned.
    assert_eq!(declared_leaves, 6_630_544, "shipped dense declared leaves at (63,2)");
    assert_eq!(reserved, 33_152_720);
    assert!(payout > 0);
    assert!(
        derived_reserved > 0,
        "past 7,400 the slash unit follows CanonicalWork, not the leaf declaration"
    );
    // Document the inequality rather than invent a new slash: P3 forbids rolling back a Final,
    // so reserved is the only debit a timely court can take. If payout > reserved the one-job
    // credit error is larger than the slash — that is the 1-of-N re-execution assumption, not
    // a missing offence object.
    let _ = (payout, reserved, derived_reserved);
}

/// **`reserved` is not `max_gain`.** A colluding Valid quorum authorizes the claim's cash payout
/// AND its fork-choice weight. The dense row's reserved (33,152,720) is only the weight term;
/// `palw_max_fraud_gain_v1` adds the escrowed payout, which the previous audit already showed
/// can exceed reserved. Panel seat locks must be sized from this number, not from reserved.
#[test]
fn max_fraud_gain_is_payout_plus_weight_on_the_shipped_dense_row() {
    use kaspa_consensus_core::palw_panel_var_v1::{
        palw_max_fraud_gain_v1, palw_panel_seat_required_v1, PalwClaimFraudFactsV1,
    };
    let dense = dense_512();
    let PalwConsensusMode::ConsensusV2(bundle) = palw_rc_shipped_params().palw_consensus_mode else {
        panic!("shipped RC is ConsensusV2");
    };
    let ladder = bundle.court.max_step_leaf_count();
    let job = rc_job_context(&dense, 63, 2);
    let declared_leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(&dense, &job, ladder).unwrap();
    let reserved = (declared_leaves as u128).saturating_mul(SLASH_VALUE_PER_PWU as u128);

    let executed = palw_attempt_economic_compute_v1(&dense, &job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("draw walks");
    let target = palw_work_ticket_target_v1(executed, palw_work_floor_v1(ESCROW_SOMPI, RATE_SOMPI_PER_GIGA));
    let attempted = palw_attempted_compute_per_claim_v1(palw_expected_attempts_v1(target), executed);
    let payout = palw_rate_priced_reward_v1(ESCROW_SOMPI, attempted, RATE_SOMPI_PER_GIGA as u128) as u128;

    let facts = PalwClaimFraudFactsV1 {
        reserved,
        escrowed_reward: payout.min(u128::from(u64::MAX)) as u64,
        exposure_pwu: declared_leaves,
        slash_value_per_pwu: SLASH_VALUE_PER_PWU,
        extra_economic_rights_sompi: 0,
    };
    let gain = palw_max_fraud_gain_v1(&facts);
    // Pin the dense-row numbers the panel lock must be sized from. Cash is the ADR-0132
    // rate-priced payout (~299B), not the 33,152,720 reserved term. The reserved-only
    // 11,050,907 seat lock under-covers this claim by four orders of magnitude.
    assert_eq!(reserved, 33_152_720);
    assert_eq!(payout, 299_167_816_089);
    assert_eq!(gain, 299_200_968_809, "cash and weight are distinct goods of one Final");
    assert!(gain > reserved, "sizing seats from reserved alone under-covers the payout");
    let required = palw_panel_seat_required_v1(&facts);
    assert_eq!(required, 99_733_656_270);
    assert_eq!(required, gain / 3 + 1);
    assert!(required * 3 > gain);
    assert!(required > 11_050_907);
    assert_eq!(
        palw_rc_shipped_params().palw_objective_offence,
        Some(ForkActivation::new(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA))
    );
}

/// **The 400k registry floor still does not cover a dense-row claim.** Three floor seats are
/// cheaper than reserved, and far cheaper than `max_gain`. Eligibility is the formula amount
/// (`gain/3+1`); genesis 10k MSK can lock a dense-row seat, a cheap new bond cannot. The floor
/// is not raised.
#[test]
fn three_of_five_collusion_value_at_risk_does_not_yet_cover_max_fraud_gain() {
    use kaspa_consensus_core::palw_offence_v1::{
        palw_colluding_quorum_covers_v1, palw_min_slashable_per_colluding_seat_v1, PALW_PANEL_COLLUDING_QUORUM_V1,
    };
    let PalwConsensusMode::ConsensusV2(bundle) = palw_rc_shipped_params().palw_consensus_mode else {
        panic!("shipped RC is ConsensusV2");
    };
    let floor = bundle.state.min_collateral_sompi() as u128;
    let three = floor.saturating_mul(3);
    const DENSE_RESERVED: u128 = 33_152_720;
    assert_eq!(three, 1_200_000, "testnet-11 floor is 400_000 sompi");
    assert!(!palw_colluding_quorum_covers_v1(floor, PALW_PANEL_COLLUDING_QUORUM_V1, DENSE_RESERVED));
    let required = palw_min_slashable_per_colluding_seat_v1(DENSE_RESERVED, PALW_PANEL_COLLUDING_QUORUM_V1);
    assert_eq!(required, 11_050_907);
    assert!(palw_colluding_quorum_covers_v1(required, PALW_PANEL_COLLUDING_QUORUM_V1, DENSE_RESERVED));
    // The reserved-only 11M does not cover the payout-inclusive max_gain of the same row.
    const DENSE_MAX_GAIN: u128 = 299_200_968_809;
    assert!(!palw_colluding_quorum_covers_v1(required, PALW_PANEL_COLLUDING_QUORUM_V1, DENSE_MAX_GAIN));
    let required_from_gain = palw_min_slashable_per_colluding_seat_v1(DENSE_MAX_GAIN, PALW_PANEL_COLLUDING_QUORUM_V1);
    assert_eq!(required_from_gain, 99_733_656_270);
    assert!(palw_colluding_quorum_covers_v1(required_from_gain, PALW_PANEL_COLLUDING_QUORUM_V1, DENSE_MAX_GAIN));
    // The 400k floor still cannot cover a dense-row claim. Eligibility is the formula amount,
    // not a raised floor: genesis 10k MSK can lock; a cheap new bond cannot.
    assert_eq!(
        palw_rc_shipped_params().palw_objective_offence,
        Some(ForkActivation::new(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA))
    );
}

/// Extras still default to dormant. The scheduled fence writes the height into extras on the
/// live path; a silent Default-true would debit history below 8,500.
#[test]
fn objective_offence_extras_are_dormant_by_default() {
    let extras = kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1::default();
    assert_eq!(extras.objective_offence_daa, None);
    assert!(!extras.objective_offence_at(0));
    assert!(!extras.objective_offence_at(7_400));
    assert!(!extras.objective_offence_at(u64::MAX));
}

/// **ADR-0144 §9: a scheduled future offence fence changes the params id, not the handshake.**
/// Below 8,500 nothing locks. Mainnet stays unset.
#[test]
fn a_scheduled_objective_offence_fence_is_dormant_until_its_height() {
    let rc = palw_rc_shipped_params();
    assert_eq!(rc.palw_objective_offence, Some(ForkActivation::new(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA)));
    assert!(!rc.palw_objective_offence_at(0));
    assert!(!rc.palw_objective_offence_at(7_400));
    assert!(!rc.palw_objective_offence_at(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA - 1));
    assert!(rc.palw_objective_offence_at(PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA));
    assert_eq!(MAINNET_PARAMS.palw_objective_offence, None);

    let mut unarmed = rc.clone();
    unarmed.palw_objective_offence = None;
    assert_eq!(
        rc.consensus_identity_id(),
        unarmed.consensus_identity_id(),
        "scheduling a future offence fence must not split the mesh on deploy day"
    );
    assert_ne!(rc.consensus_params_id(), unarmed.consensus_params_id(), "arming is a visible commitment");
    assert_ne!(rc.consensus_schedule_id(), unarmed.consensus_schedule_id(), "the operator log must name it");
}
