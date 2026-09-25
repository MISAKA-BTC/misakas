//! **Lane B of the panel-seed stopgap (2026-09-26): what an operator's non-seat data-availability
//! filer reads of the tip** — node policy's read, never a rule.
//!
//! **Why.** The panel seed CRITICAL (`docs/t12-panel-seed-2026-09-25.md`): while audit P0-10 is open
//! a lottery win costs ~279 junk BLAKE2b draws, so whoever holds the anchor can re-roll a claim's
//! panel until its own Sybil seats cover it, and a coverage lie reaches `Final` with no slash. The
//! user's decision of 2026-09-26 01:00 adds, beside fence F1 and the operator-anchor fence, a node
//! duty: **an operator node that is not a seat on a claim's panel files a data-availability
//! accusation on it**, so a junk claim meets an honest accuser even when its panel is captured
//! (ADR-0152 §3.9's "panel-independent conviction", DA-1 / DA-8: any bond may accuse, and a non-seat
//! session neither pauses the claim nor spends a seat's budget).
//!
//! **Which claims.** A claim can reach `Final` only through a licence, and a licence is signed by the
//! claim's `Valid` signers — the seats whose locks the fold writes (L-1). Under the stopgap's operator
//! trust, a claim produced by an operator bond, or licensed by operator signers alone, is vouched for
//! by the operator's own replay. So the claims this read offers are exactly the ones a captured panel
//! could carry to `Final` unverified:
//!
//! * accusable at a licensed stage — `ReceiptLicensed` (DA-2's `Licensed`), or `Final` with its
//!   vesting row still unmatured (`FinalRow`): the fold's own `da_admission_v1` stages past the licence;
//! * produced by a bond outside `operators`;
//! * relied on at least one `Valid` signer outside `operators` — a seat of the claim's panel holding a
//!   slashable lock on it (or, after `Final`, a credited seat of its vesting row);
//! * inside the accuse window: `now + W_disclose ≤ trace_retention_daa`, the fold's retention gate.
//!
//! A claim that never licenses is charged without an accuser at launch (D1: the second failed panel
//! forfeits, S0′), so it is not offered. **When X10 (`palw_rcore_attributed_charging`) arms, that
//! stops being true and this read must offer the `Live` stage too** — X10 is not declared in this
//! build.
//!
//! **What it carries.** Per claim: the stage and the DAA it began, the producer, the current panel's
//! seats (whom the node's rank excludes), the outside signers (why it is offered), the operator bonds
//! that already accuse or accused it (an open session, a refuted entry held, or a seat session opened
//! — the node backs off), DA-8's non-seat counts (3 open, 16 ever), and the last DAA the fold admits
//! an accusation at. Whether a session opens is still asked of the fold per claim
//! (`palw_producer_v2::palw_da_accusation_check_v1`, A-6's room included) before a carrier is paid for.
//!
//! **Consensus-inert.** Nothing here is read by a block rule, the fold or any id; it moves no
//! fingerprint. Empty below `palw_rcore_plus` (the DA court is dormant there).

use crate::Hash64;
use crate::palw_da_rcore_v1::PalwDaStageV1;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwStateParamsV2};
use std::collections::BTreeSet;

/// **One claim an operator's non-seat filer may accuse** ([`palw_operator_da_candidates_v1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwOperatorDaCandidateV1 {
    pub claim_id: Hash64,
    /// The claim's producer (a bond outside the operator set).
    pub producer: PalwBondKeyV2,
    /// `Licensed` or `FinalRow` — the stage a session would open at.
    pub stage: PalwDaStageV1,
    /// The DAA the stage began: `licensed_daa`, or `final_daa`. What the nodes' stagger counts from.
    pub stage_daa: u64,
    /// The seats of the claim's current panel, in seat order: a seat's session is P2-6's, never this
    /// filer's, so the node's rank leaves them out.
    pub seats: Vec<PalwBondKeyV2>,
    /// The claim's `Valid` signers outside the operator set, in bond order — why it is offered.
    pub outside_signers: Vec<PalwBondKeyV2>,
    /// Operator bonds with an open session on the claim, a refuted exposure held on it, or a seat
    /// session opened on it — in bond order. Non-empty means an honest accuser is already there.
    pub operator_accusers: Vec<PalwBondKeyV2>,
    /// DA-8: non-seat sessions open on the claim now (at most 3).
    pub open_non_seat: u8,
    /// DA-8: non-seat sessions ever opened on the claim (at most 16).
    pub opened_non_seat_total: u16,
    /// The last DAA an accusation still folds at: `trace_retention_daa − W_disclose`.
    pub accuse_until_daa: u64,
}

/// **The facts one claim is judged on** — what [`palw_operator_da_candidates_v1`] gathers off the
/// state, split out so the selection rule ([`palw_operator_da_select_v1`]) is one pure function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwOperatorDaClaimFactsV1 {
    pub claim_id: Hash64,
    pub producer: PalwBondKeyV2,
    pub phase: PalwClaimPhaseV2,
    /// `Some(matured)` when the claim has a vesting row: `matured` is its maturity latch.
    pub vesting_row_matured: Option<bool>,
    pub trace_retention_daa: u64,
    /// The current panel's seats (empty with no panel).
    pub seats: Vec<PalwBondKeyV2>,
    /// The seats (and, after `Final`, the vesting row's credited seats) holding a `Valid` signer's
    /// standing on the claim.
    pub signers: Vec<PalwBondKeyV2>,
    /// Every bond with an open session on the claim, a refuted exposure held on it, or a seat
    /// session opened on it.
    pub accusers: Vec<PalwBondKeyV2>,
    pub open_non_seat: u8,
    pub opened_non_seat_total: u16,
}

/// **The selection rule** (the module's "Which claims"): the candidate `facts` make, or `None`.
/// `disclose_window` is `W_disclose` (`palw_da_disclose_window_daa_v1`), `now_daa` the DAA the
/// accusation is expected to fold at.
pub fn palw_operator_da_select_v1(
    facts: &PalwOperatorDaClaimFactsV1,
    operators: &BTreeSet<PalwBondKeyV2>,
    disclose_window: u64,
    now_daa: u64,
) -> Option<PalwOperatorDaCandidateV1> {
    if operators.contains(&facts.producer) {
        return None;
    }
    let (stage, stage_daa) = match facts.phase {
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => (PalwDaStageV1::Licensed, licensed_daa),
        PalwClaimPhaseV2::Final { final_daa } if facts.vesting_row_matured == Some(false) => (PalwDaStageV1::FinalRow, final_daa),
        _ => return None,
    };
    let accuse_until_daa = facts.trace_retention_daa.checked_sub(disclose_window)?;
    if now_daa > accuse_until_daa || facts.seats.is_empty() {
        return None;
    }
    let outside_signers: Vec<PalwBondKeyV2> =
        facts.signers.iter().copied().filter(|b| !operators.contains(b)).collect::<BTreeSet<_>>().into_iter().collect();
    if outside_signers.is_empty() {
        return None;
    }
    let operator_accusers: Vec<PalwBondKeyV2> =
        facts.accusers.iter().copied().filter(|b| operators.contains(b)).collect::<BTreeSet<_>>().into_iter().collect();
    Some(PalwOperatorDaCandidateV1 {
        claim_id: facts.claim_id,
        producer: facts.producer,
        stage,
        stage_daa,
        seats: facts.seats.clone(),
        outside_signers,
        operator_accusers,
        open_non_seat: facts.open_non_seat,
        opened_non_seat_total: facts.opened_non_seat_total,
        accuse_until_daa,
    })
}

/// **The facts of `claim_id` at the tip**, or `None` for a claim the state does not hold.
pub fn palw_operator_da_claim_facts_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> Option<PalwOperatorDaClaimFactsV1> {
    let claim = state.claim(claim_id)?;
    let seats: Vec<PalwBondKeyV2> =
        state.panel(claim_id).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
    let row = state.vesting_row(claim_id);
    let mut signers: BTreeSet<PalwBondKeyV2> =
        seats.iter().copied().filter(|seat| state.slashable_lock(*seat, *claim_id).is_some()).collect();
    if let Some(row) = row {
        signers.extend(row.seats.iter().map(|(bond, _)| *bond));
    }
    let record = state.da_claim(claim_id);
    let mut accusers: BTreeSet<PalwBondKeyV2> = state.da_sessions_of(claim_id).map(|(accuser, _)| *accuser).collect();
    if let Some(record) = record {
        accusers.extend(record.refuted_held.iter().map(|(accuser, _)| *accuser));
        accusers.extend(record.opened_by_seat.keys().copied());
    }
    Some(PalwOperatorDaClaimFactsV1 {
        claim_id: *claim_id,
        producer: claim.bond,
        phase: claim.phase.clone(),
        vesting_row_matured: row.map(|row| row.matured_at.is_some()),
        trace_retention_daa: claim.trace_retention_daa,
        seats,
        signers: signers.into_iter().collect(),
        accusers: accusers.into_iter().collect(),
        open_non_seat: record.map(|r| r.open_other_sessions).unwrap_or(0),
        opened_non_seat_total: record.map(|r| r.opened_non_seat_total).unwrap_or(0),
    })
}

/// **Lane B's read of the tip: every claim an operator's non-seat filer may accuse at `now_daa`**
/// (the module's "Which claims"), oldest stage first. `operators` is the operator's bond set — on
/// testnet-12 the eight genesis bonds, which node policy derives from the bundle's genesis
/// registrations. Empty below `palw_rcore_plus` and for an empty operator set.
pub fn palw_operator_da_candidates_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    operators: &[PalwBondKeyV2],
    now_daa: u64,
) -> Vec<PalwOperatorDaCandidateV1> {
    if !params.rcore_plus_active_at(now_daa) || operators.is_empty() {
        return Vec::new();
    }
    let operators: BTreeSet<PalwBondKeyV2> = operators.iter().copied().collect();
    let disclose_window = crate::palw_state_v2::palw_da_disclose_window_daa_v1(params);
    let mut out: Vec<PalwOperatorDaCandidateV1> = state
        .claims_iter()
        .filter(|(_, claim)| {
            !operators.contains(&claim.bond)
                && matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. } | PalwClaimPhaseV2::Final { .. })
        })
        .filter_map(|(claim_id, _)| palw_operator_da_claim_facts_v1(state, claim_id))
        .filter_map(|facts| palw_operator_da_select_v1(&facts, &operators, disclose_window, now_daa))
        .collect();
    out.sort_by(|a, b| (a.stage_daa, a.claim_id).cmp(&(b.stage_daa, b.claim_id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB0), n as u32))
    }

    fn facts() -> PalwOperatorDaClaimFactsV1 {
        PalwOperatorDaClaimFactsV1 {
            claim_id: Hash64::from_u64_word(0xC1),
            producer: bond(20),
            phase: PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1_500 },
            vesting_row_matured: None,
            trace_retention_daa: 1_500 + 4_400,
            seats: vec![bond(1), bond(2), bond(21), bond(22), bond(3)],
            signers: vec![bond(1), bond(21), bond(22)],
            accusers: vec![],
            open_non_seat: 0,
            opened_non_seat_total: 0,
        }
    }

    fn operators() -> BTreeSet<PalwBondKeyV2> {
        (0..8).map(bond).collect()
    }

    const W: u64 = 1_200;

    /// **The rule, each clause on its own.** An outside producer licensed with outside signers is
    /// offered at `Licensed`; an operator's claim, a claim licensed by operators alone, a claim
    /// before its licence, a `Final` whose row matured (or has none), and a claim past the accuse
    /// window are not; a `Final` with an unmatured row is offered at `FinalRow`.
    #[test]
    fn the_selection_rule_offers_exactly_what_a_captured_panel_could_carry_to_final() {
        let ops = operators();
        let c = palw_operator_da_select_v1(&facts(), &ops, W, 1_501).expect("an outside claim, outside signers");
        assert_eq!((c.stage, c.stage_daa), (PalwDaStageV1::Licensed, 1_500));
        assert_eq!(c.outside_signers, vec![bond(21), bond(22)], "only the outside signers, in bond order");
        assert_eq!(c.accuse_until_daa, 1_500 + 4_400 - W);
        assert!(c.operator_accusers.is_empty());

        let own = PalwOperatorDaClaimFactsV1 { producer: bond(4), ..facts() };
        assert_eq!(palw_operator_da_select_v1(&own, &ops, W, 1_501), None, "an operator's own claim");
        let vouched = PalwOperatorDaClaimFactsV1 { signers: vec![bond(1), bond(3)], ..facts() };
        assert_eq!(palw_operator_da_select_v1(&vouched, &ops, W, 1_501), None, "licensed by operator signers alone");
        let unsigned = PalwOperatorDaClaimFactsV1 { signers: vec![], ..facts() };
        assert_eq!(palw_operator_da_select_v1(&unsigned, &ops, W, 1_501), None, "no signer: nothing relied on");
        for phase in [
            PalwClaimPhaseV2::Provisional,
            PalwClaimPhaseV2::PanelBound { bound_daa: 1_400 },
            PalwClaimPhaseV2::Voided { voided_daa: 1_600, reason: crate::palw_state_v2::PalwVoidReasonV2::ProducerWithholding },
        ] {
            let f = PalwOperatorDaClaimFactsV1 { phase: phase.clone(), ..facts() };
            assert_eq!(palw_operator_da_select_v1(&f, &ops, W, 1_501), None, "{phase:?}");
        }
        for row in [None, Some(true)] {
            let f = PalwOperatorDaClaimFactsV1 {
                phase: PalwClaimPhaseV2::Final { final_daa: 1_621 },
                vesting_row_matured: row,
                ..facts()
            };
            assert_eq!(palw_operator_da_select_v1(&f, &ops, W, 1_700), None, "Final, row {row:?}");
        }
        let final_row = PalwOperatorDaClaimFactsV1 {
            phase: PalwClaimPhaseV2::Final { final_daa: 1_621 },
            vesting_row_matured: Some(false),
            ..facts()
        };
        let c = palw_operator_da_select_v1(&final_row, &ops, W, 1_700).expect("Final with its row unmatured");
        assert_eq!((c.stage, c.stage_daa), (PalwDaStageV1::FinalRow, 1_621));

        let until = 1_500 + 4_400 - W;
        assert!(palw_operator_da_select_v1(&facts(), &ops, W, until).is_some(), "the last DAA the fold admits");
        assert_eq!(palw_operator_da_select_v1(&facts(), &ops, W, until + 1), None, "past it the fold refuses (retention)");
        let short = PalwOperatorDaClaimFactsV1 { trace_retention_daa: W - 1, ..facts() };
        assert_eq!(palw_operator_da_select_v1(&short, &ops, W, 0), None, "a retention shorter than W_disclose: never");
        let no_panel = PalwOperatorDaClaimFactsV1 { seats: vec![], ..facts() };
        assert_eq!(palw_operator_da_select_v1(&no_panel, &ops, W, 1_501), None, "no panel: the fold refuses");
    }

    /// **Who already accuses.** Only operator accusers are carried (an outside accuser may be the
    /// producer's own Sybil, answering at `deadline − 1`, so nobody backs off for it); DA-8's counts
    /// ride along untouched.
    #[test]
    fn only_operator_accusers_are_carried_and_da8_counts_ride_along() {
        let f = PalwOperatorDaClaimFactsV1 {
            accusers: vec![bond(23), bond(5), bond(2)],
            open_non_seat: 2,
            opened_non_seat_total: 9,
            ..facts()
        };
        let c = palw_operator_da_select_v1(&f, &operators(), W, 1_501).unwrap();
        assert_eq!(c.operator_accusers, vec![bond(2), bond(5)]);
        assert_eq!((c.open_non_seat, c.opened_non_seat_total), (2, 9));
    }
}
