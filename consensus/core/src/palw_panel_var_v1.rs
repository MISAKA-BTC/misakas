//! **ADR-0144 §9: slashable panel VAR that cannot be counted twice.**
//!
//! One claim's 3-of-5 inequality is not the mainnet gate. A seat that posts `required` once
//! and signs `Valid` on N live claims has authorized N × `max_gain` of fraud against one
//! posted amount. This module is the accounting that forbids that, as pure functions of
//! consensus facts. Nothing here is consulted on a dormant chain
//! (`Params::palw_objective_offence` is `None` on mainnet; testnet-11 schedules the
//! lock ledger at [`crate::config::params::PALW_RC_OBJECTIVE_OFFENCE_FENCE_DAA`]). The bundle
//! fence arms this ledger together with PanelFalseValid.
//!
//! Registry floor stays the producer floor. Panel eligibility is a dynamic predicate on
//! *available* slashable collateral, not a raised `min_collateral_sompi`. (Past
//! `palw_audit_2026_09_23` a `BondRegistered` must post the PANEL floor —
//! `palw_state_v2::palw_bond_registration_floor_v1`, 2026-09-24 DoS audit #12 (c) — to price the
//! permanent registry row; `min_collateral_sompi` itself, and every exposure priced off it, is
//! unchanged.)

use crate::palw_offence_v1::{PALW_PANEL_COLLUDING_QUORUM_V1, palw_min_slashable_per_colluding_seat_v1};
use crate::palw_state_v2::{PalwBondKeyV2, PalwClaimStateV2, PalwVoidReasonV2};
use crate::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use std::collections::BTreeMap;

/// Consensus-visible rights a colluding Valid quorum authorizes for one claim.
///
/// `reserved` is what a timely court takes from the *producer*. It is not the panel's
/// max fraud gain. Cash (`escrowed_reward`) and fork-choice weight are different goods
/// from the same Final; a previous audit showed payout can exceed reserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClaimFraudFactsV1 {
    pub reserved: u128,
    pub escrowed_reward: u64,
    /// **The claim's work IN THE UNIT ITS RESERVATION WAS WRITTEN IN** — not `claim.pwu`.
    ///
    /// `slash_value_per_pwu` is a price per *collateral* exposure_pwu: every production site that spends
    /// it multiplies it by a normalised quantity (`palw_exposure_pwu_v3` on the attempt lane,
    /// `mul_div(pwu, base_declared, base_canonical)` on the compute-priced free-prompt lane,
    /// the leaves themselves on the leaves-era one). This field carries that same quantity, so
    /// the weight term and the reservation cannot be denominated differently.
    ///
    /// It used to be `claim.pwu` — the RAW derived MAC-eq — against that collateral-unit price.
    /// The reservation side of exactly this mistake was closed on 2026-09-19 (see the comment at
    /// `palw_state_v2.rs`'s `reserved` write: "2,809× on the floor, 12,533× on the dense row");
    /// the weight side was left reading the retired unit, which put the 2M row's seat lock at
    /// 119.19× the collateral any genesis bond posts and made that class unadjudicable.
    pub exposure_pwu: u64,
    pub slash_value_per_pwu: u64,
    /// Permits, extra eligibility, or any other sompi-denominated right the claim mints.
    /// `0` when the claim carries none beyond payout and weight.
    pub extra_economic_rights_sompi: u128,
}

impl PalwClaimFraudFactsV1 {
    pub fn from_claim(claim: &PalwClaimStateV2, slash_value_per_pwu: u64) -> Self {
        Self {
            reserved: claim.reserved,
            escrowed_reward: claim.escrowed_reward,
            exposure_pwu: palw_claim_exposure_pwu_v1(claim, slash_value_per_pwu),
            slash_value_per_pwu,
            extra_economic_rights_sompi: palw_claim_extra_economic_rights_v1(claim),
        }
    }

    /// **The rule below `Params::palw_audit_2026_09_23`, byte for byte**: the weight term carried
    /// the claim's RAW `pwu` — the derived MAC-eq statistical work — against the collateral-unit
    /// price. Kept so a chain point below the fence prices exactly what every lock already written
    /// under it was priced at; a fold that re-derives those locks must reproduce them.
    pub fn from_claim_pre_2026_09_23(claim: &PalwClaimStateV2, slash_value_per_pwu: u64) -> Self {
        Self {
            reserved: claim.reserved,
            escrowed_reward: claim.escrowed_reward,
            exposure_pwu: claim.pwu,
            slash_value_per_pwu,
            extra_economic_rights_sompi: palw_claim_extra_economic_rights_v1(claim),
        }
    }
}

/// Sompi-denominated rights a Valid Final mints besides cash and fork-choice weight.
///
/// Today this is `0`. Execution-lane permits are a scheduling predicate (a class with a Final
/// may hold a permit), not a second mint of this claim's escrow. Registry eligibility is a
/// floor check, not a transfer. If a later fence mints a priced right off a claim, fold it
/// here so [`palw_max_fraud_gain_v1`] cannot silently omit it.
pub fn palw_claim_extra_economic_rights_v1(_claim: &PalwClaimStateV2) -> u128 {
    0
}

/// **The claim's work in the unit its own reservation was written in.**
///
/// Recovered from the reservation rather than recomputed: `reserved` is `exposure × price` at
/// every production site that writes it, so dividing it back by the price returns exactly the
/// quantity that site normalised — with no basis to re-derive and no DAA at which to re-derive it.
/// Re-deriving would reintroduce the failure this closes, because the basis a claim was reserved
/// under is a fact about its accepting block, not about the block asking the question later.
///
/// `slash_value_per_pwu == 0` is the caller's "no class" sentinel; the weight term is zero under
/// either reading then, so the raw pwu is returned unchanged and nothing downstream moves.
///
/// **Known gap, deliberately not papered over**: the compute-priced free-prompt lane reserves at
/// the BASE class's price (`palw_fp_compute_reserved_v1`) while this function is called with the
/// CLAIM's class price. They agree on testnet-12 (every class ships `slash_value_per_pwu = 5`)
/// and disagree on any network that prices classes apart — where this must take the price from
/// the same place the reservation did.
pub fn palw_claim_exposure_pwu_v1(claim: &PalwClaimStateV2, slash_value_per_pwu: u64) -> u64 {
    if slash_value_per_pwu == 0 {
        return claim.pwu;
    }
    (claim.reserved / slash_value_per_pwu as u128).min(u64::MAX as u128) as u64
}

/// Fork-choice weight of the claim, priced in the same sompi unit the producer slash uses.
///
/// **`exposure_pwu` must be a COLLATERAL-unit quantity** — [`PalwClaimFraudFactsV1::exposure_pwu`],
/// not `claim.pwu`. Passing the raw derived work here prices it at a rate calibrated for another
/// unit, which is the 2,810× this argument's name now refuses to accept silently.
pub fn palw_fork_weight_sompi_v1(exposure_pwu: u64, slash_value_per_pwu: u64) -> u128 {
    (exposure_pwu as u128).saturating_mul(slash_value_per_pwu as u128)
}

/// **Maximum economic gain a Valid quorum authorizes for one claim**, from consensus facts.
///
/// Cash and fork-choice weight are added, not `max`'d: a fraudulent Final both names the
/// escrow as payable and inserts `pwu` into `safe_weight`. Cash is `escrowed_reward`, the
/// accepting block's carve — ADR-0132 may pay `min(escrow, attempted × rate)`, which cannot
/// exceed this. `reserved` is the producer-slash number and is not added on top (it would
/// double-count the weight term whenever `reserved == pwu × slash_value`). Extra rights
/// (permits, eligibility) are added last; today they are `0`.
pub fn palw_max_fraud_gain_v1(facts: &PalwClaimFraudFactsV1) -> u128 {
    let cash = facts.escrowed_reward as u128;
    let weight = palw_fork_weight_sompi_v1(facts.exposure_pwu, facts.slash_value_per_pwu);
    cash.saturating_add(weight).saturating_add(facts.extra_economic_rights_sompi)
}

/// Sum of [`palw_max_fraud_gain_v1`] across every claim a colluding set authorized.
pub fn palw_aggregate_max_fraud_gain_v1(authorized: &[PalwClaimFraudFactsV1]) -> u128 {
    authorized.iter().map(palw_max_fraud_gain_v1).fold(0u128, u128::saturating_add)
}

/// Mainnet inequality on one active claim set: unencumbered, non-reused live locks of the
/// colluding quorum strictly exceed the aggregate fraud those locks authorized.
pub fn palw_unencumbered_var_covers_aggregate_v1(colluding_live_locks: u128, authorized: &[PalwClaimFraudFactsV1]) -> bool {
    colluding_live_locks > palw_aggregate_max_fraud_gain_v1(authorized)
}

/// What one Valid signer must lock on this claim so three such locks exceed [`palw_max_fraud_gain_v1`].
pub fn palw_panel_seat_required_v1(facts: &PalwClaimFraudFactsV1) -> u128 {
    palw_min_slashable_per_colluding_seat_v1(palw_max_fraud_gain_v1(facts), PALW_PANEL_COLLUDING_QUORUM_V1)
}

/// Posted minus still-live locks. Expired locks are not collateral.
pub fn palw_available_slashable_v1(posted: u128, locked_live: u128) -> u128 {
    posted.saturating_sub(locked_live)
}

/// Dynamic panel eligibility: available slashable collateral covers one seat of the colluding quorum.
pub fn palw_panel_seat_is_eligible_v1(available: u128, facts: &PalwClaimFraudFactsV1) -> bool {
    available >= palw_panel_seat_required_v1(facts)
}

/// Sum of the `quorum` smallest available exposures among the drawn seats.
pub fn palw_smallest_colluding_var_v1(available_of_drawn_seats: &[u128], quorum: usize) -> u128 {
    if available_of_drawn_seats.len() < quorum {
        return 0;
    }
    let mut v = available_of_drawn_seats.to_vec();
    v.sort_unstable();
    v.iter().take(quorum).copied().fold(0u128, u128::saturating_add)
}

/// Drawn panel is admissible only if the cheapest colluding subset out-values the claim.
pub fn palw_drawn_panel_covers_v1(available_of_drawn_seats: &[u128], facts: &PalwClaimFraudFactsV1) -> bool {
    palw_smallest_colluding_var_v1(available_of_drawn_seats, PALW_PANEL_COLLUDING_QUORUM_V1 as usize) > palw_max_fraud_gain_v1(facts)
}

/// One live lock of slashable collateral against one claim, held until `expiry_daa`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSlashableLockV1 {
    pub claim: Hash64,
    pub amount: u128,
    pub expiry_daa: u64,
    /// **The chain's settled-anchor count when this liability began** (2026-09-23 audit, the two
    /// clocks). `PalwChainStateV2::settled_attempt_finals` at the block that wrote the lock's
    /// expiry — the `Final` that made the seat liable. Past `Params::palw_settled_anchor_depth`
    /// the lock stays live until BOTH the DAA clock has run its window AND that many further
    /// anchors have settled; below it the field is carried and never read.
    pub settled_at_final: u64,
}

impl PalwSlashableLockV1 {
    /// The DAA clock alone — the rule every network below `palw_audit_2026_09_23` runs.
    pub fn is_live(&self, now_daa: u64) -> bool {
        now_daa < self.expiry_daa
    }

    /// **Both clocks** (the 2026-09-23 audit's heartbeat finding). A heartbeat block advances the
    /// DAA at 2^24 hashes and no bond, so a seat that signed a false `Final` could sit out
    /// `window_court` of heartbeat-only history and walk away with its liability expired. Past
    /// the fence the liability also needs `depth` anchors — `Final` attempt claims, each a won
    /// draw plus a licensed panel — to have settled since the seat became liable. `None` is the
    /// DAA-only rule, byte for byte.
    pub fn is_live_v2(&self, now_daa: u64, settled_now: u64, depth: Option<u64>) -> bool {
        match depth {
            None => self.is_live(now_daa),
            Some(depth) => self.is_live(now_daa) || settled_now.saturating_sub(self.settled_at_final) < depth,
        }
    }

    /// [`Self::is_live_v2`] with the second clock bounded per obligation
    /// ([`palw_second_clock_holds_v1`]): past its DAA expiry the lock is held by the anchor count
    /// for at most `2 × window_court` more. `None` is the DAA-only rule, byte for byte.
    pub fn is_live_v3(&self, now_daa: u64, settled_now: u64, depth: Option<u64>, window_court: u64) -> bool {
        self.is_live(now_daa)
            || palw_second_clock_holds_v1(depth, settled_now, self.settled_at_final, self.expiry_daa, now_daa, window_court)
    }
}

/// **Does the second clock still hold an obligation whose DAA clock released it at `daa_release`?**
/// (2026-09-24 DoS audit review of fix #3.)
///
/// The second clock keeps a lock, a liability or a retirement until `depth` anchors have settled
/// since it began, and its liveness escape (`palw_second_clock_depth_v1`) waives it only after a
/// stretch of `2 × window_court` with no licence ANYWHERE on the chain. Measured from the chain's
/// last licence and not from the obligation, that left the count unbounded in DAA: one honest
/// licence every `2 × window_court − 1` DAA — thirty attempts in all on testnet-12 — kept the escape
/// from ever firing while the count crept up by one per licence, and froze every retiring bond on
/// the network for `depth × 2 × window_court` DAA (~250 days on t12 instead of ~10;
/// `review_economic_trickle_freeze`). Once the escape had let a withdrawal through, one later
/// licence froze it again.
///
/// The bound: the second clock may extend an obligation by at most the escape's own stretch,
/// `2 × window_court`, beyond the DAA clock that governs it. It is the same price the escape
/// already concedes — a stretch of `2 × window_court` DAA an attacker drives with heartbeats
/// releases the escape, and it releases this — so a heartbeat attacker gains nothing he did not
/// have; what it removes is the trickle's multiplier and the re-freeze past the bound. `None`
/// (below the fence, or escaped) holds nothing, as before.
pub fn palw_second_clock_holds_v1(
    depth: Option<u64>,
    settled_now: u64,
    settled_at: u64,
    daa_release: u64,
    now_daa: u64,
    window_court: u64,
) -> bool {
    match depth {
        None => false,
        Some(depth) => {
            settled_now.saturating_sub(settled_at) < depth && now_daa < daa_release.saturating_add(window_court.saturating_mul(2))
        }
    }
}

/// **Final does not erase liability.** A small record the chain can keep after the claim row
/// retires: who signed Valid, on which work, until when the lock holds. Claim bytes are not
/// retained; withdraw is refused while any lock on the bond is live.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPanelLiabilityRecordV1 {
    pub claim_id: Hash64,
    pub work_id: Hash64,
    pub class_id: Hash64,
    pub execution_root: Hash64,
    pub output_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    /// Set when the claim voided for a named producer/court fault. `None` after an honest Final.
    pub voided_daa: Option<u64>,
    pub void_reason: Option<PalwVoidReasonV2>,
    pub valid_signers: Vec<(TransactionOutpoint, Hash64)>,
    pub locked_sompi: u128,
    pub expiry_daa: u64,
    /// The settled-anchor count when the liability began — see [`PalwSlashableLockV1::settled_at_final`].
    pub settled_at_final: u64,
}

/// Evidence window after Final: locks (and PanelFalseValid bind) last until this DAA.
pub fn palw_panel_liability_expiry_v1(final_daa: u64, evidence_window_daa: u64) -> u64 {
    final_daa.saturating_add(evidence_window_daa)
}

pub fn palw_liability_still_locks_v1(record: &PalwPanelLiabilityRecordV1, now_daa: u64) -> bool {
    now_daa < record.expiry_daa
}

/// [`palw_liability_still_locks_v1`] on both clocks — see [`PalwSlashableLockV1::is_live_v2`].
pub fn palw_liability_still_locks_v2(record: &PalwPanelLiabilityRecordV1, now_daa: u64, settled_now: u64, depth: Option<u64>) -> bool {
    match depth {
        None => palw_liability_still_locks_v1(record, now_daa),
        Some(depth) => palw_liability_still_locks_v1(record, now_daa) || settled_now.saturating_sub(record.settled_at_final) < depth,
    }
}

/// [`palw_liability_still_locks_v2`] with the second clock bounded per obligation, exactly as
/// [`PalwSlashableLockV1::is_live_v3`] bounds a lock ([`palw_second_clock_holds_v1`]). `depth` is
/// the ESCAPED depth (`palw_second_clock_depth_v1`); `None` is the DAA-only rule, byte for byte.
pub fn palw_liability_still_locks_v3(
    record: &PalwPanelLiabilityRecordV1,
    now_daa: u64,
    settled_now: u64,
    depth: Option<u64>,
    window_court: u64,
) -> bool {
    palw_liability_still_locks_v1(record, now_daa)
        || palw_second_clock_holds_v1(depth, settled_now, record.settled_at_final, record.expiry_daa, now_daa, window_court)
}

/// **Has a lock or a liability row passed its evidence horizon?** (2026-09-24 DoS audit #12, the
/// user's decision (a).) A row may be pruned once it is dead on BOTH clocks — its DAA expiry has
/// passed and the second clock ([`palw_second_clock_holds_v1`], read at the escaped `depth`) no
/// longer holds it — AND `window_court` more DAA have run past that expiry.
///
/// Why the extra `window_court`: "dead" answers "does this row still lock collateral", and a row
/// that locks nothing was still the only thing a late `PanelFalseValid` could bind to — the
/// liability is the claim's residue after the claim row retires, and the lock is what a
/// conviction slashes. Keeping both for one further court window past the moment they stop
/// locking gives a contradiction discovered at the edge of the liability the same window any
/// other court move gets, and then the chain forgets the obligation for good: past this horizon a
/// conviction of that seat is REFUSED (`consume_objective_offence`), never priced at the seat's
/// whole collateral.
///
/// `expiry_daa` and `settled_at_final` are the row's own; both kinds of row carry them.
pub fn palw_panel_obligation_prunable_v1(
    expiry_daa: u64,
    settled_at_final: u64,
    now_daa: u64,
    settled_now: u64,
    depth: Option<u64>,
    window_court: u64,
) -> bool {
    now_daa >= expiry_daa.saturating_add(window_court)
        && !palw_second_clock_holds_v1(depth, settled_now, settled_at_final, expiry_daa, now_daa, window_court)
}

/// Producer-withholding / court-fraud bind after the claim row retires.
pub fn palw_liability_matches_void_v1(record: &PalwPanelLiabilityRecordV1, reason: PalwVoidReasonV2, voided_daa: u64) -> bool {
    record.voided_daa == Some(voided_daa) && record.void_reason == Some(reason)
}

/// Bond-level lock table the future fold persists behind the bundle fence.
/// Borsh so IBD / restart reconstruct the same available amounts as the writer.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSlashableExposureLedgerV1 {
    pub posted: BTreeMap<PalwBondKeyV2, u128>,
    /// `(bond, claim) → lock`. One lock per (seat, claim); a second Valid on the same pair is a no-op.
    pub locks: BTreeMap<(PalwBondKeyV2, Hash64), PalwSlashableLockV1>,
}

impl PalwSlashableExposureLedgerV1 {
    pub fn live_locked(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        self.locks
            .iter()
            .filter(|((b, _), lock)| b == bond && lock.is_live(now_daa))
            .map(|(_, lock)| lock.amount)
            .fold(0u128, u128::saturating_add)
    }

    pub fn available(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        palw_available_slashable_v1(self.posted.get(bond).copied().unwrap_or(0), self.live_locked(bond, now_daa))
    }

    /// Lock `required` against `claim` if the seat has the headroom. False = not eligible.
    pub fn try_lock_valid(&mut self, bond: PalwBondKeyV2, claim: Hash64, required: u128, expiry_daa: u64, now_daa: u64) -> bool {
        if self.locks.contains_key(&(bond, claim)) {
            return true;
        }
        if self.available(&bond, now_daa) < required {
            return false;
        }
        self.locks.insert((bond, claim), PalwSlashableLockV1 { claim, amount: required, expiry_daa, settled_at_final: 0 });
        true
    }

    pub fn total_live_var(&self, bonds: &[PalwBondKeyV2], now_daa: u64) -> u128 {
        bonds.iter().map(|b| self.live_locked(b, now_daa)).fold(0u128, u128::saturating_add)
    }

    pub fn withdraw_allowed(&self, bond: &PalwBondKeyV2, now_daa: u64) -> bool {
        self.live_locked(bond, now_daa) == 0
    }

    /// Posted collateral is never counted twice: live locks on a bond cannot exceed what it posted.
    pub fn posted_is_never_counted_twice(&self, now_daa: u64) -> bool {
        self.posted.iter().all(|(bond, posted)| self.live_locked(bond, now_daa) <= *posted)
    }
}

/// How a `Valid` receipt can fail to be true, for the PanelFalseValid coverage audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwValidFalsityKindV1 {
    /// Two signed execution roots for the same job.
    ExecutorEquivocation,
    /// One-step arithmetic refutation of the committed trace.
    StepArithmeticRefutation,
    /// Structural / legs refutation of the committed program.
    LegsStructuralRefutation,
    /// Output root forged against the committed execution.
    ForgedOutputRoot,
    /// Seat signed Valid while the producer did not serve the data.
    DataWithholdingWhileSigningValid,
    /// Work identity already spent; the claim should not have been accepted.
    DuplicateSpentWork,
    /// Licence assembled without the outsider's Valid (ADR-0147). Acceptance refuses it.
    LicenceWithoutOutsiderValid,
    /// V3 coverage: some segment of the cut lacks the required Valid attestations.
    IncompleteSegmentCoverage,
    /// Conflicting permit use on the same economic right.
    ConflictingPermitUse,
    /// A court (or consumed executor offence) already convicted this claim's executor.
    CourtExecutorGuiltyOnClaim,
    /// Quality, usefulness, speedup, cache — ADR-0144 §9 forbids slashing these.
    QualityUsefulnessSpeedupCache,
    /// The work is actually Valid.
    HonestCorrectWork,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwValidFalsityClassV1 {
    StructurallyImpossible,
    ObjectiveOffenceProofExists,
    ObjectiveProofExistsButNotPanelFalseValidPayload,
    NonSlashable,
    NotAnOffence,
}

/// Every way a Valid can be false is classified. A kind that stays
/// `ObjectiveProofExistsButNotPanelFalseValidPayload` is a remaining hole. Step / legs /
/// forged output now have independent PanelFalseValid payloads, so that class is empty.
pub fn palw_classify_valid_falsity_v1(kind: PalwValidFalsityKindV1) -> PalwValidFalsityClassV1 {
    use PalwValidFalsityClassV1::*;
    use PalwValidFalsityKindV1::*;
    match kind {
        ExecutorEquivocation => ObjectiveOffenceProofExists,
        CourtExecutorGuiltyOnClaim => ObjectiveOffenceProofExists,
        DataWithholdingWhileSigningValid => ObjectiveOffenceProofExists,
        ConflictingPermitUse => ObjectiveOffenceProofExists,
        StepArithmeticRefutation => ObjectiveOffenceProofExists,
        LegsStructuralRefutation => ObjectiveOffenceProofExists,
        ForgedOutputRoot => ObjectiveOffenceProofExists,
        DuplicateSpentWork | LicenceWithoutOutsiderValid | IncompleteSegmentCoverage => StructurallyImpossible,
        QualityUsefulnessSpeedupCache => NonSlashable,
        HonestCorrectWork => NotAnOffence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionId;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
    }

    fn claim(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn facts(reserved: u128, escrow: u64, pwu: u64, slash: u64) -> PalwClaimFraudFactsV1 {
        PalwClaimFraudFactsV1 { reserved, escrowed_reward: escrow, exposure_pwu: pwu, slash_value_per_pwu: slash, extra_economic_rights_sompi: 0 }
    }

    #[test]
    fn max_gain_is_cash_plus_weight_not_reserved_alone() {
        let f = facts(33_152_720, 100_000_000, 6_630_544, 5);
        let gain = palw_max_fraud_gain_v1(&f);
        assert_eq!(palw_fork_weight_sompi_v1(6_630_544, 5), 33_152_720);
        assert_eq!(gain, 100_000_000 + 33_152_720);
        assert!(gain > f.reserved, "payout-above-reserved must raise max_gain");
        assert!(gain > f.escrowed_reward as u128, "weight is a second good");
    }

    #[test]
    fn required_seat_lock_is_sized_from_max_gain_not_reserved() {
        let f = facts(33_152_720, 100_000_000, 6_630_544, 5);
        let gain = palw_max_fraud_gain_v1(&f);
        let required = palw_panel_seat_required_v1(&f);
        assert_eq!(required, gain / 3 + 1);
        assert!(required * 3 > gain);
        assert!(required > 11_050_907, "the reserved-only 11M under-sizes a claim whose payout exceeds reserved");
    }

    #[test]
    fn uniform_required_locks_make_the_smallest_three_cover() {
        let f = facts(1_000, 0, 200, 5);
        let required = palw_panel_seat_required_v1(&f);
        let five = [required; 5];
        assert!(palw_drawn_panel_covers_v1(&five, &f));
        let mut short = [required; 5];
        short[0] = required - 1;
        short[1] = required - 1;
        short[2] = required - 1;
        assert!(!palw_drawn_panel_covers_v1(&short, &f), "the three cheapest seats must still cover");
    }

    #[test]
    fn available_excludes_live_locks_and_ignores_expired_ones() {
        let mut ledger = PalwSlashableExposureLedgerV1::default();
        ledger.posted.insert(bond(1), 30);
        assert!(ledger.try_lock_valid(bond(1), claim(1), 10, 100, 0));
        assert_eq!(ledger.available(&bond(1), 50), 20);
        assert_eq!(ledger.available(&bond(1), 100), 30, "expiry is exclusive: at expiry_daa the lock is gone");
        assert!(!ledger.withdraw_allowed(&bond(1), 50));
        assert!(ledger.withdraw_allowed(&bond(1), 100));
    }

    #[test]
    fn the_same_posted_amount_cannot_authorize_n_claims() {
        let f = facts(0, 9, 0, 0);
        assert_eq!(palw_max_fraud_gain_v1(&f), 9);
        let required = palw_panel_seat_required_v1(&f);
        assert_eq!(required, 4);
        let seats = [bond(2), bond(3), bond(4)];
        let mut ledger = PalwSlashableExposureLedgerV1::default();
        for s in &seats {
            ledger.posted.insert(*s, required);
        }
        let mut authorized = 0u32;
        for n in 1u64..=10 {
            let ok = seats.iter().all(|s| ledger.try_lock_valid(*s, claim(n), required, 1_000, 0));
            if ok {
                authorized += 1;
            }
        }
        assert_eq!(authorized, 1, "one posted required-amount buys one Valid, not ten");
        let gain = (authorized as u128).saturating_mul(palw_max_fraud_gain_v1(&f));
        let var = ledger.total_live_var(&seats, 0);
        assert_eq!(var, required * 3);
        assert!(var > gain);
    }

    #[test]
    fn two_required_posted_buys_two_claims_and_the_var_still_covers() {
        let f = facts(0, 9, 0, 0);
        let required = palw_panel_seat_required_v1(&f);
        let seats = [bond(2), bond(3), bond(4)];
        let mut ledger = PalwSlashableExposureLedgerV1::default();
        for s in &seats {
            ledger.posted.insert(*s, required.saturating_mul(2));
        }
        let mut authorized = 0u32;
        for n in 1u64..=10 {
            if seats.iter().all(|s| ledger.try_lock_valid(*s, claim(n), required, 1_000, 0)) {
                authorized += 1;
            }
        }
        assert_eq!(authorized, 2);
        let gain = (authorized as u128) * palw_max_fraud_gain_v1(&f);
        let var = ledger.total_live_var(&seats, 0);
        assert_eq!(var, required * 2 * 3);
        assert!(var > gain, "k claims × max_gain < 3 seats × k × required");
        assert!(!seats.iter().all(|s| ledger.try_lock_valid(*s, claim(99), required, 1_000, 0)));
    }

    #[test]
    fn honest_unavailable_does_not_lock_and_false_signers_are_the_var() {
        let f = facts(0, 9, 0, 0);
        let required = palw_panel_seat_required_v1(&f);
        let mut ledger = PalwSlashableExposureLedgerV1::default();
        for n in 1u64..=5 {
            ledger.posted.insert(bond(n), required);
        }
        assert!(ledger.try_lock_valid(bond(1), claim(1), required, 50, 0));
        assert!(ledger.try_lock_valid(bond(2), claim(1), required, 50, 0));
        assert!(ledger.try_lock_valid(bond(3), claim(1), required, 50, 0));
        assert_eq!(ledger.live_locked(&bond(4), 0), 0, "Unavailable / dissent does not lock");
        assert_eq!(ledger.live_locked(&bond(5), 0), 0);
        assert!(ledger.try_lock_valid(bond(4), claim(2), required, 50, 0), "an honest seat is free for another claim");
        assert!(!ledger.try_lock_valid(bond(1), claim(2), required, 50, 0), "a false-Valid signer is not");
    }

    #[test]
    fn final_does_not_unlock_and_retire_does_not_erase_a_live_liability() {
        let expiry = palw_panel_liability_expiry_v1(124, 20);
        assert_eq!(expiry, 144);
        let record = PalwPanelLiabilityRecordV1 {
            claim_id: claim(1),
            work_id: claim(1),
            class_id: claim(1),
            execution_root: claim(1),
            output_root: claim(1),
            executor_bond: bond(1),
            voided_daa: None,
            void_reason: None,
            valid_signers: vec![(bond(2).0, Hash64::from_u64_word(0xAA))],
            locked_sompi: 4,
            expiry_daa: expiry,
            settled_at_final: 0,
        };
        assert!(palw_liability_still_locks_v1(&record, 124), "Final is not immunity");
        assert!(palw_liability_still_locks_v1(&record, 143));
        assert!(!palw_liability_still_locks_v1(&record, 144));
        let mut ledger = PalwSlashableExposureLedgerV1::default();
        ledger.posted.insert(bond(2), 4);
        assert!(ledger.try_lock_valid(bond(2), claim(1), 4, expiry, 124));
        assert!(!ledger.withdraw_allowed(&bond(2), 130), "retire ≠ liability erasure");
        assert!(ledger.withdraw_allowed(&bond(2), expiry));
    }

    #[test]
    fn every_valid_falsity_is_classified() {
        use PalwValidFalsityClassV1::*;
        use PalwValidFalsityKindV1::*;
        let rows = [
            (ExecutorEquivocation, ObjectiveOffenceProofExists),
            (StepArithmeticRefutation, ObjectiveOffenceProofExists),
            (LegsStructuralRefutation, ObjectiveOffenceProofExists),
            (ForgedOutputRoot, ObjectiveOffenceProofExists),
            (DataWithholdingWhileSigningValid, ObjectiveOffenceProofExists),
            (DuplicateSpentWork, StructurallyImpossible),
            (LicenceWithoutOutsiderValid, StructurallyImpossible),
            (IncompleteSegmentCoverage, StructurallyImpossible),
            (ConflictingPermitUse, ObjectiveOffenceProofExists),
            (CourtExecutorGuiltyOnClaim, ObjectiveOffenceProofExists),
            (QualityUsefulnessSpeedupCache, NonSlashable),
            (HonestCorrectWork, NotAnOffence),
        ];
        for (kind, class) in rows {
            assert_eq!(palw_classify_valid_falsity_v1(kind), class, "{kind:?}");
        }
        let holes: Vec<_> =
            rows.iter().filter(|(_, c)| *c == ObjectiveProofExistsButNotPanelFalseValidPayload).map(|(k, _)| *k).collect();
        assert!(holes.is_empty(), "every objective Invalid is structural-reject or a PanelFalseValid payload; leftover {holes:?}");
        use crate::palw_offence_v1::PalwPanelContradictionV1 as Contradiction;
        let disc = |c: Contradiction| borsh::to_vec(&c).expect("contradiction serializes")[0];
        assert_eq!(disc(Contradiction::ProducerWithholding { voided_daa: 1 }), 2, "DA-while-Valid");
        assert_eq!(disc(Contradiction::ConflictingPermit { span: 0, round: 0, permit_index: 0 }), 3);
        assert_eq!(disc(Contradiction::CourtFraud { voided_daa: 1 }), 4);
    }

    #[test]
    fn registry_floor_is_not_the_panel_eligibility_gate() {
        let f = facts(33_152_720, 0, 6_630_544, 5);
        let required = palw_panel_seat_required_v1(&f);
        assert!(!palw_panel_seat_is_eligible_v1(400_000, &f));
        assert!(palw_panel_seat_is_eligible_v1(required, &f));
        assert!(required > 400_000);
        assert_eq!(palw_claim_extra_economic_rights_v1(&sample_claim()), 0);
        let derived = PalwClaimFraudFactsV1::from_claim(&sample_claim(), 5);
        assert_eq!(derived.extra_economic_rights_sompi, 0);
        assert_eq!(palw_max_fraud_gain_v1(&derived), derived.escrowed_reward as u128 + palw_fork_weight_sompi_v1(derived.exposure_pwu, 5));
    }

    fn sample_claim() -> crate::palw_state_v2::PalwClaimStateV2 {
        use crate::palw_state_v2::{PalwClaimPhaseV2, PalwClaimSourceV2};
        crate::palw_state_v2::PalwClaimStateV2 {
            source: PalwClaimSourceV2::Attempt,
            class_id: Hash64::from_u64_word(1),
            bond: bond(1),
            pwu: 6_630_544,
            accepted_daa: 10,
            rebound_daa: None,
            accepted_blue_score: 10,
            accepted_block: Hash64::from_u64_word(0xB1),
            trace_root: Hash64::from_u64_word(0x71),
            output_root: Hash64::from_u64_word(0x72),
            execution_root: Hash64::from_u64_word(0x73),
            trace_chunk_count: 4,
            trace_retention_daa: 700,
            reserved: 33_152_720,
            immature_contribution: 0,
            escrowed_reward: 12,
            work_leaves: 0,
            work_id: None,
            phase: PalwClaimPhaseV2::Provisional,
        }
    }

    fn sybil_three() -> [PalwBondKeyV2; 3] {
        [bond(10), bond(11), bond(12)]
    }

    fn lock_wave(
        ledger: &mut PalwSlashableExposureLedgerV1,
        seats: &[PalwBondKeyV2],
        n_claims: u64,
        required: u128,
        expiry_daa: u64,
        now_daa: u64,
    ) -> Vec<PalwClaimFraudFactsV1> {
        let f = facts(0, 9, 0, 0);
        let mut authorized = Vec::new();
        for n in 1..=n_claims {
            if seats.iter().all(|s| ledger.try_lock_valid(*s, claim(n), required, expiry_daa, now_daa)) {
                authorized.push(f);
            }
        }
        authorized
    }

    #[test]
    fn three_sybil_seats_cannot_reuse_one_lock_across_ten_or_a_hundred_claims() {
        let f = facts(0, 9, 0, 0);
        let required = palw_panel_seat_required_v1(&f);
        let seats = sybil_three();
        for n_claims in [10u64, 100] {
            let mut ledger = PalwSlashableExposureLedgerV1::default();
            for s in &seats {
                ledger.posted.insert(*s, required);
            }
            let authorized = lock_wave(&mut ledger, &seats, n_claims, required, 1_000, 0);
            assert_eq!(authorized.len(), 1, "posted required once authorizes one of {n_claims} claims");
            assert!(ledger.posted_is_never_counted_twice(0));
            let var = ledger.total_live_var(&seats, 0);
            assert!(palw_unencumbered_var_covers_aggregate_v1(var, &authorized));
            assert_eq!(var, required * 3);
        }
    }

    #[test]
    fn ten_posted_requireds_authorize_ten_of_a_hundred_and_the_var_still_covers() {
        let f = facts(0, 9, 0, 0);
        let required = palw_panel_seat_required_v1(&f);
        let seats = sybil_three();
        let mut ledger = PalwSlashableExposureLedgerV1::default();
        for s in &seats {
            ledger.posted.insert(*s, required.saturating_mul(10));
        }
        let authorized = lock_wave(&mut ledger, &seats, 100, required, 1_000, 0);
        assert_eq!(authorized.len(), 10);
        assert!(ledger.posted_is_never_counted_twice(0));
        let var = ledger.total_live_var(&seats, 0);
        assert!(palw_unencumbered_var_covers_aggregate_v1(var, &authorized));
        // Strict inequality: VAR equal to aggregate gain is not coverage. Slack from
        // `required = gain/3+1` must not be mistaken for a free extra claim.
        assert!(!palw_unencumbered_var_covers_aggregate_v1(var, &[facts(0, var as u64, 0, 0)]));
    }

    #[test]
    fn final_retire_withdraw_reorg_restart_and_ibd_keep_the_same_unencumbered_var() {
        let f = facts(0, 9, 0, 0);
        let required = palw_panel_seat_required_v1(&f);
        let seats = sybil_three();
        let mut ledger = PalwSlashableExposureLedgerV1::default();
        for s in &seats {
            ledger.posted.insert(*s, required);
        }
        let expiry = palw_panel_liability_expiry_v1(50, 20);
        assert!(seats.iter().all(|s| ledger.try_lock_valid(*s, claim(1), required, expiry, 40)));
        // Final (DAA 50) does not unlock.
        assert!(!seats.iter().any(|s| ledger.withdraw_allowed(s, 50)));
        assert!(ledger.posted_is_never_counted_twice(50));
        // Retire of the claim row does not drop the liability record's lock.
        assert!(!seats.iter().any(|s| ledger.withdraw_allowed(s, 60)));
        // Withdraw after Final, before expiry, is refused.
        assert!(!seats.iter().any(|s| ledger.withdraw_allowed(s, 69)));
        let var_before_expiry = ledger.total_live_var(&seats, 69);
        assert!(palw_unencumbered_var_covers_aggregate_v1(var_before_expiry, &[f]));

        // Restart: a cloned ledger is the same available set.
        let restarted = ledger.clone();
        assert_eq!(restarted, ledger);
        assert_eq!(restarted.available(&seats[0], 69), 0);

        // IBD: borsh round-trip reconstructs the same locks.
        let bytes = borsh::to_vec(&ledger).expect("ledger encodes");
        let recovered: PalwSlashableExposureLedgerV1 = borsh::from_slice(&bytes).expect("IBD decodes");
        assert_eq!(recovered, ledger);
        assert_eq!(recovered.total_live_var(&seats, 69), var_before_expiry);

        // Reorg of the Valid-lock block: restore the parent snapshot; available returns.
        let parent = {
            let mut p = PalwSlashableExposureLedgerV1::default();
            for s in &seats {
                p.posted.insert(*s, required);
            }
            p
        };
        assert_eq!(parent.available(&seats[0], 40), required);
        assert!(parent.withdraw_allowed(&seats[0], 40));

        // A parallel branch from parent locking a different claim does not share this branch's lock,
        // and each branch still cannot double-count its own posted amount.
        let mut other_branch = parent.clone();
        assert!(seats.iter().all(|s| other_branch.try_lock_valid(*s, claim(2), required, expiry, 40)));
        assert!(other_branch.posted_is_never_counted_twice(40));
        assert_eq!(other_branch.total_live_var(&seats, 40), required * 3);
        assert_eq!(ledger.total_live_var(&seats, 40), required * 3);
        // Crossing the two branches' locks would count the same posted amount twice — the
        // inequality is per active claim set (one chain state), not a sum across forks.
        let crossed = ledger.total_live_var(&seats, 40) + other_branch.total_live_var(&seats, 40);
        assert_eq!(crossed, required * 6);
        assert!(!palw_unencumbered_var_covers_aggregate_v1(required * 3, &[f, f]));

        // After liability expiry, withdraw is allowed and the VAR is gone.
        assert!(seats.iter().all(|s| ledger.withdraw_allowed(s, expiry)));
        assert_eq!(ledger.total_live_var(&seats, expiry), 0);
        assert!(ledger.posted_is_never_counted_twice(expiry));
    }

    /// **2026-09-24 DoS audit review of fix #3: the second clock holds a lock at most
    /// `2 × window_court` past its DAA expiry**, whatever the anchor count — so a trickle of one
    /// licence per `2 × window_court − 1` DAA cannot keep a lock live for `depth × 2 × window_court`.
    /// Inside the bound the anchor count still holds it; with the clock off (`None`) it is the DAA
    /// rule byte for byte.
    ///
    /// Fails without the fix: `is_live_v2` (the rule before it) is still live at the bound.
    #[test]
    fn the_second_clock_holds_a_lock_for_at_most_two_court_windows_past_its_expiry() {
        let (window_court, depth) = (3_000u64, 30u64);
        let lock = PalwSlashableLockV1 { claim: Hash64::from_u64_word(1), amount: 7, expiry_daa: 10_000, settled_at_final: 5 };
        let bound = lock.expiry_daa + 2 * window_court;
        // Fewer than `depth` anchors since the liability began, throughout.
        let settled = 5 + depth - 1;
        assert!(lock.is_live_v3(lock.expiry_daa - 1, settled, Some(depth), window_court), "the DAA clock");
        assert!(lock.is_live_v3(bound - 1, settled, Some(depth), window_court), "the anchor count holds it inside the bound");
        assert!(!lock.is_live_v3(bound, settled, Some(depth), window_court), "and not one DAA past it");
        assert!(lock.is_live_v2(bound, settled, Some(depth)), "PRE-FENCE DEFECT RECORD: v2 holds it on the count alone");
        assert!(!lock.is_live_v3(lock.expiry_daa, 5 + depth, Some(depth), window_court), "enough anchors release it at expiry");
        assert!(!lock.is_live_v3(lock.expiry_daa, settled, None, window_court), "no second clock: the DAA rule");
        assert_eq!(lock.is_live_v3(lock.expiry_daa - 1, 0, None, window_court), lock.is_live(lock.expiry_daa - 1));
    }

    /// **2026-09-24 DoS audit #12 (a): an obligation is prunable once it is dead on both clocks AND
    /// `window_court` past its expiry — never while either clock could still hold it.** The horizon
    /// is exact on the DAA side (`expiry + window_court − 1` keeps it, `expiry + window_court`
    /// drops it); a second clock short of `depth` keeps it up to its `2 × window_court` bound; an
    /// escaped second clock (`None`) leaves the DAA horizon alone. The liability predicate
    /// (`palw_liability_still_locks_v3`) and the lock's (`is_live_v3`) agree on every point, and
    /// nothing live is ever prunable.
    #[test]
    fn an_obligation_is_prunable_only_past_its_evidence_horizon_on_both_clocks() {
        let (wc, depth, expiry, settled_at) = (3_000u64, 30u64, 10_000u64, 5u64);
        let (short, full) = (settled_at + depth - 1, settled_at + depth);
        let prunable = |now: u64, settled_now: u64, depth: Option<u64>| {
            palw_panel_obligation_prunable_v1(expiry, settled_at, now, settled_now, depth, wc)
        };
        assert!(!prunable(expiry + wc - 1, full, Some(depth)), "dead on both clocks but inside the horizon");
        assert!(prunable(expiry + wc, full, Some(depth)), "the horizon, both clocks run out");
        assert!(prunable(expiry + wc, short, None), "an escaped second clock leaves the DAA horizon");
        assert!(!prunable(expiry + 2 * wc - 1, short, Some(depth)), "the second clock still holds it");
        assert!(prunable(expiry + 2 * wc, short, Some(depth)), "and not past its 2 x window_court bound");
        let lock =
            PalwSlashableLockV1 { claim: Hash64::from_u64_word(1), amount: 7, expiry_daa: expiry, settled_at_final: settled_at };
        let record = PalwPanelLiabilityRecordV1 {
            claim_id: Hash64::from_u64_word(1),
            work_id: Hash64::from_u64_word(2),
            class_id: Hash64::from_u64_word(3),
            execution_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            executor_bond: bond(6),
            voided_daa: None,
            void_reason: None,
            valid_signers: Vec::new(),
            locked_sompi: 7,
            expiry_daa: expiry,
            settled_at_final: settled_at,
        };
        for now in [expiry - 1, expiry, expiry + wc - 1, expiry + wc, expiry + 2 * wc - 1, expiry + 2 * wc, expiry + 3 * wc] {
            for settled_now in [short, full] {
                for d in [Some(depth), None] {
                    let live = lock.is_live_v3(now, settled_now, d, wc);
                    assert_eq!(live, palw_liability_still_locks_v3(&record, now, settled_now, d, wc), "one rule for both rows");
                    assert!(!(live && prunable(now, settled_now, d)), "nothing live is ever prunable (now {now})");
                }
            }
        }
    }
}
