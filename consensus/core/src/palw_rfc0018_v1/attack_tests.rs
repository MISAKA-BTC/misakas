//! **RFC-0018 §10 T-0 — the mandatory top-level acceptance test: the "300M closed-model / self-Panel" case** (ユーザー方針
//! 2026-10-15). Pass criterion: **no illegitimate consensus finality** — an adversary holding 300M BILI, running a model nobody else
//! can obtain and staffing its own Panels, gets ZERO safe-frontier movement, safe weight, settlement depth, rule-E participation share,
//! settled anchors and issuance-funded reward outflow from its uncertified claims, past the fences; and the honest certified side
//! decides fork choice whichever side is the incumbent, on the relay path and the IBD path alike.
//!
//! Each quantity is asserted SEPARATELY (the report the user asked for). The cases: the ~23,076-minimum-bond split (300M / 13,000), voided
//! attempts, the Free-Prompt-only Final, and both directions of the comparison plus the IBD claim-set path (the reorg and IBD halves at
//! the rule level). The real-process halves — reorg / restart / pruning / IBD on owned nodes — are `scripts/drills/rfc0018_dag_drill.py
//! attack`.
//!
//! **Unverified source (2026-10-15): written, not built and not run — no result here is a PASS.** The adversarial verifier-stake share
//! is an explicit input (`ADVERSARY_DRAWN_SHARE`): the residual assumption, reported, never hidden.

use super::*;
use crate::config::params::palw_t12_shipped_params;
use crate::palw_fork_authority_v2::PalwDeepReorgV2;
use crate::palw_fork_choice_rule_e_v1::{
    palw_rule_e_decide_v2, palw_rule_e_sides_above_fork_v1, palw_rule_e_sides_by_claim_set_v1,
};
use crate::palw_security_certification_v1::*;
use crate::palw_state_v2::{
    PalwBondStateV2, PalwBondStatusV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwVoidReasonV2, palw_claim_template_v1,
};
use crate::tx::TransactionOutpoint;

const SOMPI: u64 = 100_000_000;
/// The minimum bond the split uses (13,000 BILI) and the adversary's capital (300M BILI): 23,076 bonds.
const MIN_BOND: u64 = 13_000 * SOMPI;
const ADVERSARY_CAPITAL: u64 = 300_000_000 * SOMPI;
const SPLIT_BONDS: u64 = ADVERSARY_CAPITAL / MIN_BOND;
/// The honest side: 90 bonds of 10M BILI (900M BILI) — an INPUT, as the residual assumption is.
const HONEST_BONDS: u64 = 90;
const HONEST_BOND: u64 = 10_000_000 * SOMPI;
/// The adversary's share of the DRAWN verifier stake (300M of 1,200M): the explicit residual assumption.
const ADVERSARY_DRAWN_SHARE: f64 = 0.25;
const FENCE: u64 = 1_000;
const FORK_BLUE: u64 = 2_000;

fn key(tag: u64, i: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(tag), i as u32))
}

fn bond(collateral: u64, operator: u64) -> PalwBondStateV2 {
    PalwBondStateV2 {
        pubkey: vec![1; 4],
        operator_id: Hash64::from_u64_word(operator),
        collateral,
        slashed: 0,
        status: PalwBondStatusV2::Active,
        registered_daa: 1,
        payout_payload: Hash64::from_u64_word(0x9A00 + operator),
        capable_classes: Default::default(),
    }
}

fn armed_params() -> PalwStateParamsV2 {
    let state = match palw_t12_shipped_params().palw_consensus_mode {
        crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state,
        _ => panic!("testnet-12 is ConsensusV2"),
    };
    state.with_rfc0018_v1(Some(PalwRfc0018MirrorV1 {
        sibling_attribution_from_daa: Some(FENCE),
        panel_anchor_permissionless_from_daa: Some(FENCE),
        rule_e_participation_v2_from_daa: Some(FENCE),
        gate_c_from_daa: Some(FENCE),
        gate_c_rule: PalwGateCRuleV1::SecurityCertifiedV1,
        certification: PalwSecurityCertificationPolicyV1::design_unapproved_v1(720),
        fp_receipt_tail_daa: 0,
        attestor_liability_from_daa: None,
    }))
}

fn row(claim: &Hash64, outcome: Option<PalwCertificationOutcomeV1>, held: u64) -> Vec<u8> {
    borsh::to_vec(&PalwClaimCertificationRowV1 {
        claim: claim.as_bytes(),
        class_id: [0xC1; 64],
        producer_bond: [1; 64],
        producer_principal: [2; 64],
        attempt: true,
        final_daa: FENCE + 10,
        committed_daa: FENCE + 10,
        committed_span: 0,
        anchor: None,
        draws: Vec::new(),
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: Vec::new(),
        held_reward: held,
        payout_payload: [3; 64],
        outcome: outcome.map(|o| (o, FENCE + 900)),
    })
    .unwrap()
}

/// The common past: every honest and every split adversary bond registered.
fn fork_state() -> PalwChainStateV2 {
    let mut s = PalwChainStateV2::genesis();
    for i in 0..HONEST_BONDS {
        s.set_bond_for_tests(key(0x40, i), Some(bond(HONEST_BOND, 0x4000 + i)));
    }
    for i in 0..SPLIT_BONDS {
        s.set_bond_for_tests(key(0xAD, i), Some(bond(MIN_BOND, 0xAD_0000 + i)));
    }
    s
}

fn claim(bond: PalwBondKeyV2, n: u64, phase: PalwClaimPhaseV2) -> (Hash64, PalwClaimStateV2) {
    let mut c = palw_claim_template_v1(Hash64::from_u64_word(0xC1A5), bond, FENCE + 5, 0, 1_000);
    c.pwu = 1_000;
    c.immature_contribution = 300;
    c.accepted_blue_score = FORK_BLUE + 1 + n % 50;
    c.phase = phase;
    (Hash64::from_u64_word(0xC1A1_0000 + n), c)
}

struct Sides {
    adversary: PalwChainStateV2,
    honest: PalwChainStateV2,
    adversary_claims: Vec<(Hash64, PalwClaimStateV2)>,
}

/// The adversary's branch: every split bond's attempt reaches `Final` through its own Panels (a closed model nobody else can check), so
/// its certification is undecided or failed; a share of attempts is voided; one Free-Prompt claim is `Final` and "certified" — the
/// Free-Prompt-only Final case. The honest branch: a few honest attempts, `Final` AND SecurityCertified.
fn sides() -> Sides {
    let mut adversary = fork_state();
    let mut honest = fork_state();
    let mut adversary_claims = Vec::new();
    let fin = PalwClaimPhaseV2::Final { final_daa: FENCE + 60 };
    for i in 0..SPLIT_BONDS {
        let phase = if i % 7 == 0 {
            PalwClaimPhaseV2::Voided { voided_daa: FENCE + 30, reason: PalwVoidReasonV2::BindTimeout }
        } else {
            fin.clone()
        };
        let (id, c) = claim(key(0xAD, i), i, phase);
        adversary.set_false_valid_rows_for_tests(id, Some(c.clone()), None, None, 0);
        // A closed model: undecided, vetoed by honest "Unavailable" positions, or expired — never certified.
        let outcome = match i % 3 {
            0 => None,
            1 => Some(PalwCertificationOutcomeV1::Vetoed),
            _ => Some(PalwCertificationOutcomeV1::Expired),
        };
        adversary.set_kernel_aux_row_for_tests(
            PALW_SECURITY_CERTIFICATION_TABLE_V1,
            certification_claim_key_v1(&palw_certification_id_v1(&id)),
            Some(row(&id, outcome, 1_000)),
        );
        adversary_claims.push((id, c));
    }
    // The Free-Prompt-only Final: even a certified Free-Prompt Final moves no frontier and no settlement depth.
    let (fp_id, mut fp) = claim(key(0xAD, 0), 9_999_999, fin.clone());
    fp.source = PalwClaimSourceV2::FreePrompt { quanta: 8, spent: Default::default() };
    adversary.set_false_valid_rows_for_tests(fp_id, Some(fp.clone()), None, None, 0);
    adversary.set_kernel_aux_row_for_tests(
        PALW_SECURITY_CERTIFICATION_TABLE_V1,
        certification_claim_key_v1(&palw_certification_id_v1(&fp_id)),
        Some(row(&fp_id, Some(PalwCertificationOutcomeV1::Certified), 0)),
    );
    adversary_claims.push((fp_id, fp));
    for i in 0..12 {
        let (id, c) = claim(key(0x40, i), 50_000_000 + i, fin.clone());
        honest.set_false_valid_rows_for_tests(id, Some(c), None, None, 0);
        honest.set_kernel_aux_row_for_tests(
            PALW_SECURITY_CERTIFICATION_TABLE_V1,
            certification_claim_key_v1(&palw_certification_id_v1(&id)),
            Some(row(&id, Some(PalwCertificationOutcomeV1::Certified), 0)),
        );
    }
    Sides { adversary, honest, adversary_claims }
}

#[test]
fn rfc18_t0_the_split_is_the_users_figure() {
    assert_eq!(SPLIT_BONDS, 23_076, "300M BILI over 13,000-BILI minimum bonds");
}

/// (1) safe-frontier movement, (2) safe weight, (5) settled anchors, (6) reward outflow — per claim, at the fold's own gate sites.
#[test]
fn rfc18_t0_no_frontier_no_weight_no_settled_anchor_no_reward_from_uncertified_claims() {
    let p = armed_params();
    let s = sides();
    let mut frontier_moves = 0u64;
    let mut safe_weight = 0u128;
    let mut provisional = 0u128;
    let mut settled_anchor_ticks = 0u64;
    for (id, c) in &s.adversary_claims {
        frontier_moves += palw_gate_c_advances_frontier_v1(&p, &s.adversary, id, c) as u64;
        safe_weight += palw_gate_c_weight_v1(&p, &s.adversary, id, c, c.pwu as u128);
        provisional += palw_claim_provisional_weight_v1(&p, c);
        settled_anchor_ticks += palw_gate_c_settles_anchor_v1(&p, c) as u64;
    }
    assert_eq!(frontier_moves, 0, "safe_frontier: no adversary claim may stand as the frontier");
    // The only claim that certifies is the Free-Prompt Final (weight only through its spends — none here).
    assert_eq!(safe_weight, s.adversary_claims.last().map(|(_, c)| c.pwu as u128).unwrap(), "safe_weight: only the certified FP record");
    assert_eq!(provisional, 0, "bounded_immature / live_total: nothing provisional past the fence");
    assert_eq!(settled_anchor_ticks, 0, "settled anchors: no licence or Final site ticks for a governed claim");
    // Reward outflow: a held issuance-funded reward is named only by a certifying outcome; none of the adversary's attempts has one.
    let named: u64 = s
        .adversary_claims
        .iter()
        .filter(|(_, c)| matches!(c.source, PalwClaimSourceV2::Attempt))
        .filter(|(id, _)| palw_claim_security_certified_v1(&s.adversary, id))
        .count() as u64
        * 1_000;
    assert_eq!(named, 0, "reward outflow: no held reward is ever named for an uncertified attempt");
}

/// (3) settlement depth — the RPC's own read: no PALW settlement finality on a chain whose attempts are all uncertified.
#[test]
fn rfc18_t0_no_settlement_depth_and_honest_reporting() {
    let p = armed_params();
    let s = sides();
    let read = crate::palw_settlement_v1::palw_settlement_v1(&s.adversary, &p, None, FENCE).expect("a frontier-less state answers");
    assert!(!read.settled, "settled: false");
    assert_eq!(read.depth, 0, "settlement depth: 0");
    assert!(read.uncertified_final > 0, "the uncertified Finals are reported, not hidden: {}", read.uncertified_final);
}

/// (4) rule-E participation share, and the fork choice: the honest certified side wins whichever side is the incumbent, on the relay
/// path (both directions — the reorg half) and on the IBD claim-set path.
#[test]
fn rfc18_t0_rule_e_participation_share_is_zero_and_the_honest_side_wins_every_path() {
    let p = armed_params();
    let s = sides();
    let fork = fork_state();
    let pair = palw_rule_e_sides_above_fork_v1(&s.adversary, &s.honest, &fork, FORK_BLUE, &p, false, false, None).unwrap();
    let (adv, hon) = (pair.a, pair.b);
    assert_eq!(adv.participation_capital_v2, 0, "rule-E participation share: the adversary's uncertified/voided attempts count nothing");
    assert_eq!(adv.participation, 0, "v1 participation reads the same Gate C mapping (no uncertified attempt is an attempt)");
    assert!(hon.participation_capital_v2 > 0);
    assert_eq!((adv.economic().0, adv.economic().2 - adv.economic().1), (0, 0), "no frontier and no live weight in the adversary's exclusive past (its only safe weight is the certified FP record's spends: none)");
    for (incumbent, challenger, expect) in [(&adv, &hon, PalwDeepReorgV2::Allow), (&hon, &adv, PalwDeepReorgV2::Refuse)] {
        let decision = palw_rule_e_decide_v2(incumbent, challenger, true, &pair, true, || false, || false);
        assert_eq!(decision, expect);
    }
    // The IBD path (no shared DAG): the same verdict from the claim-set difference.
    let ibd = palw_rule_e_sides_by_claim_set_v1(&s.adversary, FENCE + 100, &s.honest, FENCE + 100, &p, false, false, None).unwrap();
    assert_eq!(ibd.a.participation_capital_v2, 0);
    assert_eq!(
        palw_rule_e_decide_v2(&ibd.a, &ibd.b, true, &ibd, true, || false, || false),
        PalwDeepReorgV2::Allow,
        "IBD commits the honest certified side over an adversary incumbent"
    );
}

/// Split resistance: had the adversary's model been open and its claims honestly certified, the 23,076 pieces would weigh exactly the
/// one 300M bond — never more.
#[test]
fn rfc18_t0_the_split_buys_no_participation() {
    let mut split = PalwRuleEParticipationV2Acc::default();
    for i in 0..SPLIT_BONDS {
        split.push(key(0xAD, i), true, true, Some(MIN_BOND));
    }
    let mut whole = PalwRuleEParticipationV2Acc::default();
    whole.push(key(0xAE, 0), true, true, Some(SPLIT_BONDS * MIN_BOND));
    assert_eq!(split.capital(), whole.capital());
}

/// The residual assumption, reported: the per-claim probability that a closed model's claim certifies at the adversary's drawn-stake
/// share, at the design values; and the pruning-progress bound with undecided certifications.
#[test]
fn rfc18_t0_residual_assumption_and_pruning_progress_are_reported() {
    let policy = PalwSecurityCertificationPolicyV1::design_unapproved_v1(720);
    let p = palw_certification_false_pass_f64(ADVERSARY_DRAWN_SHARE, policy.m as u32, policy.k as u32, policy.veto as u32);
    eprintln!("[rfc18-t0] closed-model per-claim certification probability at drawn share {ADVERSARY_DRAWN_SHARE}: {p:.3e}");
    // The premise-failure impact (reported, never a design condition): f = 1/3, f = 0.6, worst case f = 1.
    for f in [1.0 / 3.0, 0.6, 1.0] {
        let q = palw_certification_false_pass_f64(f, policy.m as u32, policy.k as u32, policy.veto as u32);
        eprintln!("[rfc18-t0] premise failure impact: f = {f:.3} -> per-claim certification {q:.3e}");
    }
    assert!(p > 0.0 && p < 0.05, "at the design values the residual is small and NOT zero: it is the stated premise ({p})");
    // Pruning: undecided adversary certifications hold history only until they are decided (bounded by `lifetime_daa`).
    let s = sides();
    let ceiling = palw_pruning_progress_ceiling_v1(&s.adversary);
    assert!(ceiling.is_some_and(|c| c < FORK_BLUE + 1), "an undecided certification's claim history is not pruned");
}
