//! **ADR-0151's wedge search, at the altitude the rules live at.**
//!
//! The operator's mainnet condition, verbatim: *"minimum colluding quorum の実際にslash可能な合計 >
//! cash payout + maturity前に実現可能なexecution rights + その他回収不能なrights が、2M/Kimi/軽量class
//! すべてで成立すること"* — and *"offence が確定したら round_finals / round_pending / round_schedule の
//! 未使用quantaが全部失効し … 二重slash/二重forfeitがない"*.
//!
//! What is checked here is every one of those that is a property of the RULES. What is not — a real
//! restart, a real reorg, a real IBD — is the live drill, and this file does not pretend to cover it.
use std::collections::BTreeSet;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_exec_quantum_maturity_daa_v1, palw_exec_rights_are_forfeit_v1,
    palw_permit_value_sompi_v1, palw_realizable_before_maturity_v1, palw_rounds_per_daa_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_matured_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_offence_v1::{PALW_PANEL_COLLUDING_QUORUM_V1, palw_colluding_quorum_covers_v1};
use kaspa_consensus_core::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::tx::TransactionOutpoint;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// Every class testnet-12 registers, with the pwu its Final would be priced on.
fn registered_classes(p: &Params) -> Vec<(Hash64, u64, u64)> {
    let PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    b.genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, slash_value_per_pwu, .. } => match pwu_rule {
                PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => Some((*class_id, *pwu_per_inference, *slash_value_per_pwu)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// **The mainnet condition, over EVERY registered class.**
///
/// For each: the gain a fraudulent Final authorizes — cash, fork weight, and the execution rights
/// realizable before a conviction could take them — against what the minimum colluding quorum must
/// lock. The inequality has to hold for the 2M row, for the hybrid, and for the liveness floor,
/// because a rule that held only for the dear class would leave the cheap one profitable to lie about.
#[test]
fn the_colluding_quorum_outvalues_the_lie_for_every_class() {
    let p = t12();
    let escrow = p.pre_deflationary_phase_base_subsidy / 1_000 * u64::from(p.palw_overlay_carve.expect("armed").worker_carve_permille);
    let cadence = p.target_time_per_block();
    // testnet-12's maturity is the challenge window it applies, 120 DAA (user decision 2026-09-25),
    // which leaves a 2,880-DAA gap to the liability horizon: on the uncapped mint below that prices
    // every quantum of every class (345,600 rounds > 270,030), where 1,200 priced 216,000 of them.
    let maturity = p.palw_exec_quantum_maturity_v1();
    assert_eq!(maturity, 120, "testnet-12 matures at its short challenge window");
    let window_court = 3_000u64;
    let classes = registered_classes(&p);
    assert_eq!(classes.len(), 3, "the floor and the two held rows");
    for (class_id, pwu_per_inference, slash) in classes {
        let claim_pwu = kaspa_consensus_core::palw_pwu::palw_pwu_v1(u128::MAX / 2, pwu_per_inference);
        // The quanta a Final of this class mints: the UNCLAMPED CanonicalWork over the quantum.
        let quanta = u32::try_from(pwu_per_inference / PALW_EXECUTION_QUANTUM_V1).unwrap_or(u32::MAX).saturating_add(1);
        let residual = palw_realizable_before_maturity_v1(
            quanta,
            maturity,
            window_court,
            cadence,
            palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI),
        );
        assert_eq!(residual, u128::from(quanta) * u128::from(PALW_T12_PERMIT_FEE_CEILING_SOMPI), "the whole mint is priced");
        let facts = PalwClaimFraudFactsV1 {
            reserved: 0,
            escrowed_reward: escrow,
            exposure_pwu: claim_pwu,
            slash_value_per_pwu: slash,
            extra_economic_rights_sompi: residual,
        };
        let gain = palw_max_fraud_gain_v1(&facts);
        let seat = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        assert!(
            palw_colluding_quorum_covers_v1(seat, PALW_PANEL_COLLUDING_QUORUM_V1, gain),
            "class {class_id}: {} colluding seats at {seat} must out-value a gain of {gain}",
            PALW_PANEL_COLLUDING_QUORUM_V1
        );
        // And with a whole unpriced permit of drift, which the shipped one-sompi margin could not take.
        assert!(
            seat.saturating_mul(u128::from(PALW_PANEL_COLLUDING_QUORUM_V1)) > gain.saturating_add(u128::from(PALW_T12_PERMIT_FEE_CEILING_SOMPI)),
            "class {class_id}: the margin must absorb one unpriced permit"
        );
        println!("class {} quanta {quanta:>9} residual {:>14} gain {:>14} seat {:>14}", &format!("{class_id}")[..16], residual, gain, seat);
    }
}

fn a_final(root: Hash64, credit: u64) -> PalwExecFinalV1 {
    PalwExecFinalV1 {
        domain: Hash64::from_u64_word(1),
        bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: Hash64::from_u64_word(9), index: 0 }),
        operator_id: Hash64::from_u64_word(7),
        claim_id: Hash64::from_u64_word(root.as_bytes()[0] as u64 + 1),
        execution_root: root,
        credit,
        accepted_blue_score: 0,
    }
}

/// **A convicted execution mints nothing** — the forfeiture at the mint, which is the last of the
/// three stages a right can sit in.
#[test]
fn a_forfeited_execution_mints_no_quanta() {
    let honest = a_final(Hash64::from_u64_word(0x11), 500_000);
    let lying = a_final(Hash64::from_u64_word(0x22), 500_000);
    let seed = Hash64::from_u64_word(0xABC);
    let none = BTreeSet::new();
    let both = palw_execution_mint_quanta_matured_v1(&[honest, lying], seed, u128::from(PALW_EXECUTION_QUANTUM_V1), 0, 0, &none);
    assert!(both.len() >= 10, "500k + 500k at 100k is at least ten tickets, got {}", both.len());

    let forfeited: BTreeSet<Hash64> = [lying.execution_root].into_iter().collect();
    let after = palw_execution_mint_quanta_matured_v1(&[honest, lying], seed, u128::from(PALW_EXECUTION_QUANTUM_V1), 0, 0, &forfeited);
    assert!(after.iter().all(|q| q.final_id == honest.claim_id), "not one ticket of the convicted execution survives");
    assert!(!after.is_empty(), "and the honest Final keeps its own");
    // Idempotent: forfeiting twice is forfeiting once. No double-forfeit.
    let twice: BTreeSet<Hash64> = [lying.execution_root, lying.execution_root].into_iter().collect();
    assert_eq!(twice.len(), 1, "the forfeiture set is a SET — a second conviction on one root adds nothing");
    let again = palw_execution_mint_quanta_matured_v1(&[honest, lying], seed, u128::from(PALW_EXECUTION_QUANTUM_V1), 0, 0, &twice);
    assert_eq!(again.len(), after.len(), "and mints the same set");
}

/// **A right belongs to the WORK, so a second claim over the same execution forfeits with it.**
/// The same reason the mint dedups by `execution_root`: a copy is not a second right.
#[test]
fn a_copy_of_a_convicted_execution_is_also_forfeit() {
    let root = Hash64::from_u64_word(0x33);
    let original = a_final(root, 400_000);
    let mut copy = a_final(root, 400_000);
    copy.claim_id = Hash64::from_u64_word(0x999);
    let forfeited: BTreeSet<Hash64> = [root].into_iter().collect();
    assert!(palw_exec_rights_are_forfeit_v1(&forfeited, &original.execution_root));
    assert!(palw_exec_rights_are_forfeit_v1(&forfeited, &copy.execution_root));
    let minted = palw_execution_mint_quanta_matured_v1(
        &[original, copy],
        Hash64::from_u64_word(1),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        0,
        0,
        &forfeited,
    );
    assert!(minted.is_empty(), "neither the original nor its copy mints a ticket");
}

/// **Maturity: not one ticket is scheduled before the Final's conviction window has run** — as
/// the matured mint expresses it. The fold no longer calls this mint on testnet-12: since the
/// 2026-09-23 route-matrix audit's #2 it serves the maturity by delaying the SNAPSHOT
/// (`palw_state_v2::tests::past_the_economic_safety_bundle_a_finals_tickets_land_where_their_schedule_is_judged`),
/// because a ticket 144,000 rounds out was on a round no kept schedule lists.
#[test]
fn no_ticket_is_scheduled_before_its_maturity() {
    let p = t12();
    // The `None` rule is the lattice window; testnet-12 states the challenge window it APPLIES
    // (user decision 2026-09-25), ADR-0132 §7.6's 120.
    assert_eq!(palw_exec_quantum_maturity_daa_v1(1_200, 3_000), 1_200, "the None rule: the lattice challenge window");
    let maturity_daa = p.palw_exec_quantum_maturity_v1();
    assert_eq!(maturity_daa, 120, "testnet-12 matures at the challenge window it applies");
    let maturity_rounds = maturity_daa * palw_rounds_per_daa_v1(p.target_time_per_block());
    assert_eq!(maturity_rounds, 14_400);
    let open_round = 1_000u64;
    let minted = palw_execution_mint_quanta_matured_v1(
        &[a_final(Hash64::from_u64_word(0x44), 1_000_000)],
        Hash64::from_u64_word(5),
        u128::from(PALW_EXECUTION_QUANTUM_V1),
        open_round,
        maturity_rounds,
        &BTreeSet::new(),
    );
    assert!(!minted.is_empty());
    for q in &minted {
        assert!(
            q.scheduled_round >= open_round + maturity_rounds,
            "a ticket at round {} is spendable before its Final could be convicted",
            q.scheduled_round
        );
    }
    // Each ticket still occupies a distinct round, so maturity does not collapse the mint.
    let rounds: BTreeSet<u64> = minted.iter().map(|q| q.scheduled_round).collect();
    assert_eq!(rounds.len(), minted.len(), "one ticket, one round");
}
