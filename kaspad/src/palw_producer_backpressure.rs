//! **Producer backpressure: stop adding claims while this bond's own claims are not getting licensed**
//! — node policy, never validity (the 2026-10-01 panel backlog,
//! `docs/design/palw/t12-panel-backlog-1001.md`, F3).
//!
//! # Why
//!
//! Nothing between a producer and the seats connects the rate claims are issued at to the rate seats
//! can license them. On testnet-12 issuance rose ten-fold (0.6 → 5.3 claims a DAA) while the receipt
//! supply peaked at ~600 an hour and then fell to ~170; the claims waiting for their quorum
//! (`PanelBound`) went from ~17 to 261, their median wait from 2–6 DAA to 61, and every producer
//! kept producing, because a producer reads its bond's exposure ceiling and the chain's room and
//! neither says "the seats are behind". A claim that outlives its receipt window redraws, and its
//! second timeout voids the claim and slashes its producer — so the producer that keeps going is the
//! one that pays for it.
//!
//! # What it does
//!
//! Once every [`PALW_PRODUCER_DEBT_READ_SECS_V1`] the producer reads its OWN bond's open claims and
//! counts the ones that have been `PanelBound` longer than [`PALW_PRODUCER_DEBT_AGE_DAA_V1`] DAA — five
//! times the longest healthy wait. At [`PALW_PRODUCER_DEBT_HOLD_V1`] such claims it holds its attempt
//! lane, and it resumes only when no more than [`PALW_PRODUCER_DEBT_RELEASE_V1`] are left (hysteresis:
//! the hold does not flap on a count that sits at the line). The same seats serve every bond's
//! panels, so a bond whose claims wait is a bond that is adding to a queue the seats are behind on.
//!
//! **Anchor duty is never held** (`facts.binder_due`): an operator's attempt is the anchor that binds
//! other bonds' claims (`palw_operator_anchor`), and a fleet that stopped anchoring would void the
//! claims it was trying not to add to. `binder_due` is true for an operator only while a claim is due
//! at the candidate, so a non-operator producer is held whole and an operator is held between binds.
//!
//! The receipt lane (a certified free-prompt claim's quantum) is not held: it opens no claim.
//!
//! **What this does not do**: it bounds the producers that run it. A bond that does not (another
//! operator's node, an older binary) keeps issuing, so the network-wide answer is consensus's — the
//! network room (`palw_capacity_network_room`) deriving its level from the verification capacity the
//! seats actually have (int-11, in the design doc).

use kaspa_consensus_core::palw_producer_v2::PalwClaimRowV1;
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;

/// A claim has waited too long for its quorum once it has been `PanelBound` this many DAA. The
/// healthy bind → licence wait on testnet-12 was 2–6 DAA (median, per 100-DAA cohort, 400–2,900) and
/// never over ~12; 30 DAA (about 80 minutes) is the line where waiting is a fact about the seats and
/// not the draw.
pub const PALW_PRODUCER_DEBT_AGE_DAA_V1: u64 = 30;

/// The attempt lane holds at this many such claims …
pub const PALW_PRODUCER_DEBT_HOLD_V1: usize = 8;

/// … and resumes when no more than this many are left.
pub const PALW_PRODUCER_DEBT_RELEASE_V1: usize = 2;

/// How often the producer reads its bond's open claims (an RPC-sized read of at most 500 rows).
pub const PALW_PRODUCER_DEBT_READ_SECS_V1: u64 = 10;

/// What the producer counts of its own bond's open claims.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwProducerDebtV1 {
    /// Open claims of this bond waiting in `Provisional` (for their bind).
    pub provisional: usize,
    /// Open claims of this bond in `PanelBound` (waiting for their quorum).
    pub panel_bound: usize,
    /// Of those, the ones that have waited [`PALW_PRODUCER_DEBT_AGE_DAA_V1`] DAA or more.
    pub aged: usize,
    /// How long the longest `PanelBound` wait is, in DAA.
    pub oldest_wait_daa: u64,
}

/// **Count `rows` (this bond's executor claims) at `now_daa`.**
pub fn palw_producer_debt_v1(rows: &[PalwClaimRowV1], now_daa: u64) -> PalwProducerDebtV1 {
    let mut debt = PalwProducerDebtV1::default();
    for row in rows {
        match &row.phase {
            PalwClaimPhaseV2::Provisional => debt.provisional += 1,
            PalwClaimPhaseV2::PanelBound { bound_daa } => {
                let waited = now_daa.saturating_sub(*bound_daa);
                debt.panel_bound += 1;
                debt.oldest_wait_daa = debt.oldest_wait_daa.max(waited);
                if waited >= PALW_PRODUCER_DEBT_AGE_DAA_V1 {
                    debt.aged += 1;
                }
            }
            _ => {}
        }
    }
    debt
}

/// The gate: held or not, with the hysteresis between [`PALW_PRODUCER_DEBT_HOLD_V1`] and
/// [`PALW_PRODUCER_DEBT_RELEASE_V1`].
#[derive(Clone, Copy, Debug, Default)]
pub struct PalwProducerDebtGateV1 {
    held: bool,
}

impl PalwProducerDebtGateV1 {
    pub fn is_held(&self) -> bool {
        self.held
    }

    /// **Judge one reading.** `Some(detail)` while the attempt lane is held — the sentence the producer
    /// logs and `getPalwNodeStatus` serves as the hold's reason.
    pub fn judge(&mut self, debt: &PalwProducerDebtV1) -> Option<String> {
        if self.held {
            if debt.aged <= PALW_PRODUCER_DEBT_RELEASE_V1 {
                self.held = false;
            }
        } else if debt.aged >= PALW_PRODUCER_DEBT_HOLD_V1 {
            self.held = true;
        }
        self.held.then(|| {
            format!(
                "verification backpressure: {} of this bond's claims have been waiting for their panel's quorum for {} DAA or more \
                 (the longest {} DAA; healthy is 2-6) and {} more wait to bind — the seats are behind, and another claim would only \
                 deepen the queue (hold at {}, release at {} or fewer)",
                debt.aged,
                PALW_PRODUCER_DEBT_AGE_DAA_V1,
                debt.oldest_wait_daa,
                debt.provisional,
                PALW_PRODUCER_DEBT_HOLD_V1,
                PALW_PRODUCER_DEBT_RELEASE_V1
            )
        })
    }

    /// The gate's reading as the `verification` status string's producer half.
    pub fn status(&self, debt: &PalwProducerDebtV1) -> String {
        format!(
            "own_provisional={} own_panel_bound={} own_panel_bound_aged={} own_oldest_wait_daa={} debt_hold={}",
            debt.provisional, debt.panel_bound, debt.aged, debt.oldest_wait_daa, self.held
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_producer_v2::PalwClaimRowV1;
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64;

    fn row(phase: PalwClaimPhaseV2) -> PalwClaimRowV1 {
        PalwClaimRowV1 {
            claim_id: Hash64::default(),
            free_prompt: false,
            quanta: 0,
            quanta_spent: 0,
            class_id: Hash64::default(),
            executor_bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_bytes([1; 64]), 0)),
            phase,
            accepted_daa: 0,
            accepted_block: Default::default(),
            rebound_daa: None,
            seats: Vec::new(),
            bound_daa: None,
            deadline_daa: None,
            reserved: 0,
            committed: None,
            escrowed_reward: 0,
            payout_pending: None,
            work_leaves: 0,
            open_courts: 0,
            exec_lane: None,
        }
    }

    fn bound(daa: u64) -> PalwClaimRowV1 {
        row(PalwClaimPhaseV2::PanelBound { bound_daa: daa })
    }

    /// **The count**: only `PanelBound` claims that have waited the age bound are aged; the others are
    /// counted but never move the gate.
    #[test]
    fn only_claims_that_have_waited_the_age_bound_count_as_debt() {
        let now = 1_000;
        let rows = vec![
            bound(now - PALW_PRODUCER_DEBT_AGE_DAA_V1),     // exactly at the bound: aged
            bound(now - PALW_PRODUCER_DEBT_AGE_DAA_V1 + 1), // one short: not
            bound(now - 5),
            bound(now - 61),
            row(PalwClaimPhaseV2::Provisional),
            row(PalwClaimPhaseV2::Final { final_daa: 900 }),
        ];
        let debt = palw_producer_debt_v1(&rows, now);
        assert_eq!((debt.provisional, debt.panel_bound, debt.aged, debt.oldest_wait_daa), (1, 4, 2, 61));
    }

    /// **Hysteresis**: the gate closes at the hold level and opens only at the release level — a count
    /// that sits between them leaves it as it was, so it cannot flap once a tick.
    #[test]
    fn the_gate_holds_at_the_line_and_releases_only_well_under_it() {
        let mut gate = PalwProducerDebtGateV1::default();
        let at = |aged: usize| PalwProducerDebtV1 { aged, panel_bound: aged, ..Default::default() };
        assert!(gate.judge(&at(PALW_PRODUCER_DEBT_HOLD_V1 - 1)).is_none(), "one under the line: producing");
        let detail = gate.judge(&at(PALW_PRODUCER_DEBT_HOLD_V1)).expect("at the line: held");
        assert!(detail.contains("verification backpressure") && detail.contains("hold at 8, release at 2"), "{detail}");
        for between in (PALW_PRODUCER_DEBT_RELEASE_V1 + 1)..PALW_PRODUCER_DEBT_HOLD_V1 {
            assert!(gate.judge(&at(between)).is_some(), "{between} aged: still held — no flapping");
        }
        assert!(gate.judge(&at(PALW_PRODUCER_DEBT_RELEASE_V1 + 1)).is_some());
        assert!(gate.judge(&at(PALW_PRODUCER_DEBT_RELEASE_V1)).is_none(), "released at the release level");
        assert!(!gate.is_held());
        // And back: below the hold level it stays released.
        assert!(gate.judge(&at(PALW_PRODUCER_DEBT_HOLD_V1 - 1)).is_none());
        assert!(gate.judge(&at(PALW_PRODUCER_DEBT_HOLD_V1)).is_some());
    }

    /// **The 2026-10-01 numbers**: 9f76 held 55 provisional + 71 panel-bound; e136 held 67 + 65: both
    /// far over the line, so a producer running this would have held; the operators' b6 (18 panel-bound,
    /// most of them young) would not have.
    #[test]
    fn the_live_bonds_of_the_backlog_would_have_held_and_a_healthy_one_would_not() {
        let now = 3_089;
        // A backlog bond: 71 panel-bound, bound between 19 and 171 DAA ago (the measured spread: median 61).
        let rows: Vec<_> = (0..71u64).map(|i| bound(now - (19 + i * 152 / 70))).collect();
        let debt = palw_producer_debt_v1(&rows, now);
        assert!(debt.aged >= PALW_PRODUCER_DEBT_HOLD_V1, "{debt:?}");
        assert!(PalwProducerDebtGateV1::default().judge(&debt).is_some());
        // A healthy bond: claims bound 1-6 DAA ago.
        let healthy: Vec<_> = (0..40u64).map(|i| bound(now - 1 - i % 6)).collect();
        let debt = palw_producer_debt_v1(&healthy, now);
        assert_eq!(debt.aged, 0);
        assert!(PalwProducerDebtGateV1::default().judge(&debt).is_none());
        // The status string a fleet check reads.
        let mut gate = PalwProducerDebtGateV1::default();
        gate.judge(&palw_producer_debt_v1(&rows, now));
        assert!(gate.status(&palw_producer_debt_v1(&rows, now)).contains("debt_hold=true"));
    }
}
