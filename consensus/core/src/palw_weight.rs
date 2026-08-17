//! ADR-0038 Decision B: the receipt-licensed weight ramp — a block's PALW work matures with
//! evidence, and fabricated work never outweighs the spam-hash backbone.
//!
//! ```text
//! weight(B) = spam_backbone_work(B) + pwu(B) × ramp(B)
//!
//! ramp: Provisional (admission)            → 0
//!       ReceiptLicensed (≥ k receipts)     → ρ_r        (permille, ≈ 1000)
//!       Final (unrefuted W_challenge)      → 1
//!       Voided (convicted before final)    → 0, forever
//! ```
//!
//! Three ADR-0038 invariants are theorems of this module, each pinned by a test:
//!
//! * **W3** — [`ramp_stage_v1`] and [`effective_weight_v1`] are pure functions of
//!   DAG-derivable facts ([`PalwWeightFactsV1`]): equal DAGs give equal facts give equal
//!   weights on every node. No store, no clock, no configuration enters.
//! * **W4** — a Provisional or Voided block's weight IS the spam backbone: unverified pwu
//!   contributes exactly zero to fork choice, so a private fork full of fabricated
//!   commitments weighs its (deliberately tiny) hash work and nothing else.
//! * **W5** — `Final` is absorbing: a conviction fact arriving after finality does not
//!   change the stage (finality means finality — the window is sized so a live watcher
//!   always convicts first, ADR-0038 A1), and a conviction before finality voids exactly
//!   this block's pwu (the backbone survives; other blocks are other facts).
//!
//! The facts themselves (receipt counts via [`crate::palw_receipt`], window passage via
//! [`crate::palw_job_state::PalwDualDeadlineV3`], convictions via the court) are assembled
//! by the caller from its own chain view — the same caller-assembles-facts shape as
//! [`crate::palw_credit`]. Consensus-inert until the ADR-0038 change set wires and
//! activates together.

use thiserror::Error;

/// ρ_r denominator: ramp fractions are permille, integer math only.
pub const PALW_WEIGHT_RAMP_PERMILLE_DENOMINATOR: u64 = 1000;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwWeightError {
    #[error("rho_r is {got}‰, above the denominator {max}‰")]
    RhoOutOfRange { got: u16, max: u64 },
    #[error("receipt quorum k must be nonzero (k = 0 would license fabricated work at admission)")]
    ZeroReceiptQuorum,
}

/// The maturity of one block's PALW work. Stages are ordered by evidence, not by time:
/// `Final` is `ReceiptLicensed` plus a closed window, never a shortcut past it (see
/// [`ramp_stage_v1`] — the "window passed unrefuted" shortcut finalized private forks), and
/// `Voided` is reachable only before `Final` (W5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwWorkRampStageV1 {
    /// Admitted, no evidence yet: pwu counts for nothing (W4).
    Provisional,
    /// ≥ k distinct assigned receipts landed: pwu counts at ρ_r permille.
    ReceiptLicensed,
    /// The challenge window closed with no surviving conviction: pwu counts in full, forever.
    Final,
    /// Convicted before finality: pwu counts for nothing, forever.
    Voided,
}

/// The DAG-derivable facts about one block's PALW work, assembled by the caller from its own
/// chain view at its own virtual point. Every field is recomputable from DAG data alone (W3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWeightFactsV1 {
    /// Distinct assigned `Match` receipts covering this block's commitment
    /// ([`crate::palw_receipt::count_distinct_receipt_verifiers_v1`]).
    pub distinct_receipts: u32,
    /// The challenge window's dual deadline has passed (DAA ∧ MTP).
    pub challenge_window_closed: bool,
    /// A court conviction against this block's work exists, and the conviction's carriage was
    /// accepted BEFORE the challenge window closed. A conviction observed after close is a
    /// protocol-failure telemetry event, never a weight fact (W5).
    pub convicted_before_close: bool,
    /// A dispute over this block's work is open, or terminated `Unadjudicable`, as of this
    /// evaluation.
    ///
    /// Without this field the two states were **unrepresentable**, so both took the `Final`
    /// path at full weight (re-audit 2026-08-17). That inverts the court: ADR-0038 Decision C
    /// and [`crate::palw_dispute`]'s own settlement table make `Unadjudicable` non-creditable
    /// and freeze the class — precisely because it means "this class's catalog cannot decide
    /// the question", which is the forger's hole. Maturing such a block at full weight rewards
    /// exactly the gap the freeze exists to contain. An open dispute is likewise not an
    /// absence of refutation; it is a refutation still being answered.
    pub dispute_open_or_unadjudicable: bool,
}

/// The ramp parameters a network registers (soak outputs; ADR-0038 "does not decide").
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWeightParamsV1 {
    /// k — distinct assigned receipts required for `ReceiptLicensed`.
    pub receipt_quorum: u32,
    /// ρ_r in permille of full pwu (≤ 1000).
    pub rho_r_permille: u16,
}

impl PalwWeightParamsV1 {
    pub fn validate(&self) -> Result<(), PalwWeightError> {
        if self.receipt_quorum == 0 {
            return Err(PalwWeightError::ZeroReceiptQuorum);
        }
        if self.rho_r_permille as u64 > PALW_WEIGHT_RAMP_PERMILLE_DENOMINATOR {
            return Err(PalwWeightError::RhoOutOfRange { got: self.rho_r_permille, max: PALW_WEIGHT_RAMP_PERMILLE_DENOMINATOR });
        }
        Ok(())
    }
}

/// The one stage function. Precedence is exactly ADR-0038 Decision B's:
/// conviction-before-close voids (checked first — a convicted block must not finalize in the
/// same evaluation that observed the conviction); an unresolved court state cannot mature; a
/// closed window over OBSERVED work finalizes; a receipt quorum licenses; otherwise the work
/// is provisional.
///
/// # Why `Final` requires the receipt quorum
///
/// The first version finalized on `challenge_window_closed` alone, documenting it as a "slow
/// path — no quorum landed, the window simply passed unrefuted". That reading of ADR-0027's
/// "PASS is absence of refutation" drops its unstated premise: **absence of refutation is
/// evidence only about work somebody could have refuted.**
///
/// A private fork satisfies it trivially. Mine fabricated PALW blocks in private, hold them for
/// `W_challenge`, publish: nobody refuted them, because nobody could see them. Both facts read
/// true, the stage is `Final`, and the whole fork matures at full pwu — the fabrication the
/// ramp exists to stop, arriving through the ramp's own slow path (re-audit 2026-08-17).
///
/// Receipts are the only proof-of-publication this design has, so they are what makes "no
/// refutation" mean anything. Requiring them is not an extra check bolted onto finality; it is
/// finality's missing premise.
///
/// **Known consequence, deliberately taken here**: assigned verifiers can now withhold receipts
/// and hold an honest block at `Provisional` forever — a censorship veto over *maturity*. That
/// is the strictly safer failure (delayed finality beats a matured forgery), and it is bounded
/// only once unlicensed-but-published work carries some live fork-choice weight, so the chain
/// progresses while maturity waits. That split — live weight for tip selection, safe weight
/// (this function) for finality and IBD — is the next change on this branch and is a
/// precondition of activating the ramp. Until it lands, this module is safe and not yet live.
pub fn ramp_stage_v1(facts: &PalwWeightFactsV1, params: &PalwWeightParamsV1) -> PalwWorkRampStageV1 {
    if facts.convicted_before_close {
        return PalwWorkRampStageV1::Voided;
    }
    // An open or unadjudicable dispute is not an absence of refutation. Checked before the
    // window so a dispute that outlives the window cannot mature by expiry.
    if facts.dispute_open_or_unadjudicable {
        return PalwWorkRampStageV1::Provisional;
    }
    let licensed = facts.distinct_receipts >= params.receipt_quorum;
    if facts.challenge_window_closed && licensed {
        return PalwWorkRampStageV1::Final;
    }
    if licensed {
        return PalwWorkRampStageV1::ReceiptLicensed;
    }
    PalwWorkRampStageV1::Provisional
}

/// `weight(B)`: the spam-hash backbone plus the ramped pwu, in one u128 so no parameter
/// combination can overflow (`u64 × 1000` fits with room to spare).
pub fn effective_weight_v1(
    spam_backbone_work: u64,
    pwu: u64,
    stage: PalwWorkRampStageV1,
    params: &PalwWeightParamsV1,
) -> u128 {
    let ramped: u128 = match stage {
        PalwWorkRampStageV1::Provisional | PalwWorkRampStageV1::Voided => 0,
        PalwWorkRampStageV1::ReceiptLicensed => {
            (pwu as u128) * (params.rho_r_permille as u128) / (PALW_WEIGHT_RAMP_PERMILLE_DENOMINATOR as u128)
        }
        PalwWorkRampStageV1::Final => pwu as u128,
    };
    (spam_backbone_work as u128) + ramped
}

#[cfg(test)]
mod tests {
    use super::*;
    use PalwWorkRampStageV1 as S;

    const PARAMS: PalwWeightParamsV1 = PalwWeightParamsV1 { receipt_quorum: 3, rho_r_permille: 900 };

    fn facts(receipts: u32, closed: bool, convicted: bool) -> PalwWeightFactsV1 {
        PalwWeightFactsV1 {
            distinct_receipts: receipts,
            challenge_window_closed: closed,
            convicted_before_close: convicted,
            dispute_open_or_unadjudicable: false,
        }
    }

    /// `facts` with an open / unadjudicable dispute.
    fn disputed(receipts: u32, closed: bool) -> PalwWeightFactsV1 {
        PalwWeightFactsV1 { dispute_open_or_unadjudicable: true, ..facts(receipts, closed, false) }
    }

    /// Params validation: k = 0 would make fabricated work licensed at admission; ρ_r > 1000
    /// would let receipt-licensed work outweigh final work.
    #[test]
    fn params_validation_is_closed() {
        assert!(PARAMS.validate().is_ok());
        assert_eq!(
            PalwWeightParamsV1 { receipt_quorum: 0, rho_r_permille: 900 }.validate(),
            Err(PalwWeightError::ZeroReceiptQuorum)
        );
        assert_eq!(
            PalwWeightParamsV1 { receipt_quorum: 3, rho_r_permille: 1001 }.validate(),
            Err(PalwWeightError::RhoOutOfRange { got: 1001, max: 1000 })
        );
    }

    /// The stage table, exhaustively: every fact combination maps to exactly the ADR's stage.
    #[test]
    fn stage_table_is_the_adr_table() {
        assert_eq!(ramp_stage_v1(&facts(0, false, false), &PARAMS), S::Provisional);
        assert_eq!(ramp_stage_v1(&facts(2, false, false), &PARAMS), S::Provisional); // below k
        assert_eq!(ramp_stage_v1(&facts(3, false, false), &PARAMS), S::ReceiptLicensed);
        assert_eq!(ramp_stage_v1(&facts(9, false, false), &PARAMS), S::ReceiptLicensed);
        // The removed slow path: a closed window with NO quorum is unobserved work, not
        // finalized work — this is the private-fork hole, now closed.
        assert_eq!(ramp_stage_v1(&facts(0, true, false), &PARAMS), S::Provisional);
        assert_eq!(ramp_stage_v1(&facts(2, true, false), &PARAMS), S::Provisional); // below k, window irrelevant
        assert_eq!(ramp_stage_v1(&facts(9, true, false), &PARAMS), S::Final);
        assert_eq!(ramp_stage_v1(&facts(0, false, true), &PARAMS), S::Voided);
        assert_eq!(ramp_stage_v1(&facts(9, false, true), &PARAMS), S::Voided); // receipts don't outvote the court
    }

    /// Re-audit 2026-08-17: **a private fork cannot self-finalize.**
    ///
    /// The attack the old slow path allowed: mine fabricated PALW blocks in private, hold them
    /// for the whole challenge window, publish. `challenge_window_closed` is true and
    /// `convicted_before_close` is false — nobody refuted work nobody could see — so the old
    /// table returned `Final` and the fork matured at full pwu.
    ///
    /// Absence of refutation is evidence only about work somebody could have refuted, and
    /// receipts are this design's only proof that somebody could. This pins the premise.
    #[test]
    fn a_private_fork_cannot_self_finalize() {
        // The attacker's fact pattern, exactly: window closed, never refuted, never observed.
        let unobserved = facts(0, true, false);
        assert_eq!(ramp_stage_v1(&unobserved, &PARAMS), S::Provisional, "unobserved work must not mature");
        assert_eq!(effective_weight_v1(7, u64::MAX, ramp_stage_v1(&unobserved, &PARAMS), &PARAMS), 7, "and it weighs only its spam backbone");

        // One receipt short is still short — the quorum is the premise, not a hint.
        assert_eq!(ramp_stage_v1(&facts(PARAMS.receipt_quorum - 1, true, false), &PARAMS), S::Provisional);
        // Published and observed work still finalizes exactly as before.
        assert_eq!(ramp_stage_v1(&facts(PARAMS.receipt_quorum, true, false), &PARAMS), S::Final);
    }

    /// An open or `Unadjudicable` dispute cannot mature — the states that were previously
    /// unrepresentable in the facts and therefore took the `Final` path.
    ///
    /// `Unadjudicable` means the class's catalog cannot decide the question and the class
    /// freezes ([`crate::palw_dispute`] I10). Maturing that block at full weight would pay the
    /// forger for finding exactly the hole the freeze exists to contain.
    #[test]
    fn an_unresolved_dispute_cannot_mature() {
        for closed in [false, true] {
            for receipts in [0, PARAMS.receipt_quorum, PARAMS.receipt_quorum * 3] {
                assert_eq!(
                    ramp_stage_v1(&disputed(receipts, closed), &PARAMS),
                    S::Provisional,
                    "receipts={receipts} closed={closed}: an unresolved dispute is not an absence of refutation"
                );
            }
        }
        // A conviction still outranks it — Voided is stronger than "unresolved".
        let convicted_and_disputed = PalwWeightFactsV1 { dispute_open_or_unadjudicable: true, ..facts(9, true, true) };
        assert_eq!(ramp_stage_v1(&convicted_and_disputed, &PARAMS), S::Voided);
    }

    /// W5's precedence edge: a conviction accepted before close voids even when evaluated
    /// after the window closed — and a conviction NOT before close never voids a final block
    /// (the fact encoding makes late convictions unrepresentable as weight facts).
    #[test]
    fn w5_conviction_before_close_beats_finality_and_late_conviction_cannot() {
        assert_eq!(ramp_stage_v1(&facts(9, true, true), &PARAMS), S::Voided);
        assert_eq!(ramp_stage_v1(&facts(9, true, false), &PARAMS), S::Final);
    }

    /// W4: Provisional and Voided weight IS the backbone — fabricated pwu contributes zero.
    #[test]
    fn w4_unlicensed_pwu_is_weightless() {
        for stage in [S::Provisional, S::Voided] {
            assert_eq!(effective_weight_v1(7, u64::MAX, stage, &PARAMS), 7);
        }
    }

    /// The ramp is monotone in evidence: backbone ≤ provisional < receipt-licensed ≤ final,
    /// with ρ_r = 1000 collapsing the last step to equality.
    #[test]
    fn weight_is_monotone_in_stage() {
        let (backbone, pwu) = (5u64, 1_000_000u64);
        let provisional = effective_weight_v1(backbone, pwu, S::Provisional, &PARAMS);
        let licensed = effective_weight_v1(backbone, pwu, S::ReceiptLicensed, &PARAMS);
        let final_ = effective_weight_v1(backbone, pwu, S::Final, &PARAMS);
        assert!(provisional < licensed && licensed <= final_);
        assert_eq!(licensed, 5 + 900_000);
        assert_eq!(final_, 5 + 1_000_000);
        let full = PalwWeightParamsV1 { receipt_quorum: 3, rho_r_permille: 1000 };
        assert_eq!(effective_weight_v1(backbone, pwu, S::ReceiptLicensed, &full), effective_weight_v1(backbone, pwu, S::Final, &full));
    }

    /// W3 (determinism) and no-overflow: the extreme corner is exactly representable.
    #[test]
    fn w3_pure_and_overflow_free_at_the_corner() {
        let a = effective_weight_v1(u64::MAX, u64::MAX, S::Final, &PARAMS);
        let b = effective_weight_v1(u64::MAX, u64::MAX, S::Final, &PARAMS);
        assert_eq!(a, b);
        assert_eq!(a, (u64::MAX as u128) * 2);
        // ReceiptLicensed at the corner: (2^64-1) × 1000 fits u128 with ~54 bits to spare.
        let licensed = effective_weight_v1(u64::MAX, u64::MAX, S::ReceiptLicensed, &PARAMS);
        assert_eq!(licensed, (u64::MAX as u128) + (u64::MAX as u128) * 900 / 1000);
    }

    /// Borsh roundtrip of facts, params and stage (they ride diagnostic/state records).
    #[test]
    fn types_roundtrip_borsh() {
        let f = facts(3, true, false);
        assert_eq!(f, borsh::from_slice::<PalwWeightFactsV1>(&borsh::to_vec(&f).unwrap()).unwrap());
        assert_eq!(PARAMS, borsh::from_slice::<PalwWeightParamsV1>(&borsh::to_vec(&PARAMS).unwrap()).unwrap());
        let s = S::ReceiptLicensed;
        assert_eq!(s, borsh::from_slice::<PalwWorkRampStageV1>(&borsh::to_vec(&s).unwrap()).unwrap());
    }
}
