//! Layer 2: assembling the weight facts, as pure functions.
//!
//! [`crate::palw_weight::ramp_stage_v1`] and [`crate::palw_chain_weight`] decide what a block's
//! work is worth. This module is where the inputs they consume come from — the step the
//! 2026-08-17 re-audit called the consumer layer, and the one that has to be right before any
//! fence opens.
//!
//! # Absence is an error, never a zero
//!
//! The old credit walk read acceptance data that returns an EMPTY vector when pruned, so a node
//! validating just above its own pruning point saw no commitments and computed a different answer
//! than an archival node — with pruning points differing per node, the answer was per-node
//! (blocker 6). Every function here is total in the same direction as
//! [`crate::palw_chain_weight::chain_weights_v1`]: what cannot be resolved is reported as
//! unresolved, and the caller must decide, rather than being handed a plausible zero.
//!
//! # A receipt only counts if it was ASSIGNED
//!
//! [`crate::palw_receipt::count_distinct_receipt_verifiers_v1`] counts distinct bond outpoints,
//! which is necessary and not sufficient: k bonds under one owner are still k distinct outpoints,
//! so counting them licenses fabricated work at the cost of k bonds the fabricator already owns
//! (re-audit). [`assigned_receipt_count_v1`] intersects with the drawn panel, so a receipt from a
//! verifier nobody assigned is telemetry rather than a licence.

use crate::palw_job_panel::PalwPanelSeatV3;
use crate::palw_receipt::{PalwReceiptVerdictV1, PalwVerificationReceiptV1};
use crate::palw_weight::PalwWeightFactsV1;
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwFactsError {
    #[error("this node cannot resolve {what} for the block under evaluation — an unresolved fact is an error, never a permissive zero")]
    Unresolved { what: &'static str },
    #[error("the challenge window is zero — every block would finalize at admission")]
    ZeroChallengeWindow,
}

/// Distinct ASSIGNED verifiers who filed a `Match` receipt for this block's commitment.
///
/// Three filters, each load-bearing:
///
/// * the receipt must target this block AND this commitment root — a receipt for the same block
///   under a different root is about a different claim;
/// * the verdict must be `Match` — a `Mismatch` routes into dispute and licenses nothing;
/// * the filer's bond must be a **drawn seat**. Without this the quorum counts anyone who chose
///   to file, so a fabricator with `k` of its own bonds licenses its own work. Panel membership
///   is what makes a receipt a duty discharged rather than an opinion offered.
///
/// Dedup is by bond outpoint, which is unique; a validator key hash is not
/// (`dns_finality` states so itself), and counting by it is how one identity becomes several.
pub fn assigned_receipt_count_v1(
    receipts: &[PalwVerificationReceiptV1],
    panel: &[PalwPanelSeatV3],
    target_block_hash: &Hash64,
    target_commitment_root: &Hash64,
) -> u32 {
    let seats: BTreeSet<_> = panel.iter().map(|s| (s.bond_outpoint.transaction_id, s.bond_outpoint.index)).collect();
    receipts
        .iter()
        .filter(|r| {
            r.target_block_hash == *target_block_hash
                && r.target_commitment_root == *target_commitment_root
                && r.verdict == PalwReceiptVerdictV1::Match
        })
        .map(|r| (r.verifier_bond_outpoint.transaction_id, r.verifier_bond_outpoint.index))
        .filter(|id| seats.contains(id))
        .collect::<BTreeSet<_>>()
        .len() as u32
}

/// What a caller must have resolved from chain state before weight facts can be assembled.
///
/// Every field is `Option` on purpose: `None` means "this node could not resolve it", which is a
/// different statement from any value it might otherwise have defaulted to. The conversion below
/// refuses rather than choosing.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwResolvedBlockFactsV1 {
    /// DAA at which this block's commitment was accepted on the chain being evaluated.
    pub accepted_daa: Option<u64>,
    /// The evaluating chain's own DAA — the point of view the window is measured against.
    pub pov_daa: Option<u64>,
    /// Distinct assigned `Match` receipts ([`assigned_receipt_count_v1`]).
    pub assigned_receipts: Option<u32>,
    /// A conviction against this block's work was accepted BEFORE the window closed.
    pub convicted_before_close: Option<bool>,
    /// A dispute over this block is open, or terminated `Unadjudicable`.
    pub dispute_open_or_unadjudicable: Option<bool>,
}

/// Turn resolved facts into the ramp's input, or say which one is missing.
///
/// `w_challenge` is the class's registered window. The window-closed test is
/// `pov_daa − accepted_daa > w_challenge`, evaluated on the chain being weighed rather than
/// against a clock: two nodes with the same DAG agree, which is the whole determinism obligation
/// ([`crate::palw_chain_weight`]'s module docs).
pub fn weight_facts_v1(resolved: &PalwResolvedBlockFactsV1, w_challenge: u64) -> Result<PalwWeightFactsV1, PalwFactsError> {
    if w_challenge == 0 {
        return Err(PalwFactsError::ZeroChallengeWindow);
    }
    let accepted = resolved.accepted_daa.ok_or(PalwFactsError::Unresolved { what: "the commitment's acceptance DAA" })?;
    let pov = resolved.pov_daa.ok_or(PalwFactsError::Unresolved { what: "the evaluating chain's DAA" })?;
    let receipts = resolved.assigned_receipts.ok_or(PalwFactsError::Unresolved { what: "the assigned receipt count" })?;
    let convicted = resolved.convicted_before_close.ok_or(PalwFactsError::Unresolved { what: "the conviction state" })?;
    let disputed =
        resolved.dispute_open_or_unadjudicable.ok_or(PalwFactsError::Unresolved { what: "the dispute state" })?;
    Ok(PalwWeightFactsV1 {
        distinct_receipts: receipts,
        // Saturating: a point of view BEHIND the acceptance is not a closed window, and it is the
        // ordinary state during a reorg rather than an error.
        challenge_window_closed: pov.saturating_sub(accepted) > w_challenge,
        convicted_before_close: convicted,
        dispute_open_or_unadjudicable: disputed,
    })
}

/// The pwu a block's class earns it: [`crate::palw_pwu::palw_pwu_v1`] over the class's own DAA
/// target and its registered per-inference cost.
///
/// Both come from chain state and neither is a miner input, which is what
/// [`crate::palw_pwu::check_pwu_claim_v1`] enforces against the commitment's claim. This is the
/// same derivation, for the consumer that needs the value rather than a verdict on a claim.
pub fn block_pwu_v1(class_target: Option<u128>, pwu_per_inference: Option<u64>) -> Result<u64, PalwFactsError> {
    let target = class_target.ok_or(PalwFactsError::Unresolved { what: "the class's DAA target" })?;
    let cost = pwu_per_inference.ok_or(PalwFactsError::Unresolved { what: "the class's per-inference cost" })?;
    Ok(crate::palw_pwu::palw_pwu_v1(target, cost))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_weight::{PalwWeightParamsV1, PalwWorkRampStageV1, ramp_stage_v1};
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }
    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_bytes([seed; 64]), index: 0 }
    }
    fn seat(seed: u8) -> PalwPanelSeatV3 {
        PalwPanelSeatV3 { validator_id: h(seed as u64), bond_outpoint: op(seed) }
    }
    fn receipt(bond: u8, block: u64, root: u64, verdict: PalwReceiptVerdictV1) -> PalwVerificationReceiptV1 {
        PalwVerificationReceiptV1 {
            version: crate::palw_receipt::PALW_RECEIPT_VERSION_V1,
            target_block_hash: h(block),
            target_commitment_root: h(root),
            execution_class_id: h(0xC1),
            sample_coordinates: vec![crate::palw_receipt::PalwSampleCoordinateV1 {
                token_index: 0,
                layer_index: 0,
                node_slot: 0,
                unit_index: 0,
            }],
            observed_roots: vec![h(0x74)],
            verdict,
            verifier_bond_outpoint: op(bond),
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// **A receipt from a verifier nobody assigned licenses nothing.**
    ///
    /// Counting distinct bond outpoints is necessary and not sufficient: k bonds under one owner
    /// are k distinct outpoints, so a fabricator with its own k bonds could license its own work
    /// at no cost beyond bonds it already holds (re-audit). Intersecting with the drawn panel is
    /// what makes a receipt a duty discharged rather than an opinion offered.
    #[test]
    fn only_assigned_verifiers_license_work() {
        let panel = vec![seat(1), seat(2), seat(3)];
        // Three self-owned bonds, none of them drawn.
        let squatters: Vec<_> = [7u8, 8, 9].iter().map(|b| receipt(*b, 100, 200, PalwReceiptVerdictV1::Match)).collect();
        assert_eq!(assigned_receipt_count_v1(&squatters, &panel, &h(100), &h(200)), 0, "unassigned filers license nothing");
        // The naive count would have accepted all three — this is the gap being closed.
        assert_eq!(crate::palw_receipt::count_distinct_receipt_verifiers_v1(&squatters, &h(100), &h(200)), 3);

        // Two assigned, one not: two.
        let mixed = vec![
            receipt(1, 100, 200, PalwReceiptVerdictV1::Match),
            receipt(2, 100, 200, PalwReceiptVerdictV1::Match),
            receipt(9, 100, 200, PalwReceiptVerdictV1::Match),
        ];
        assert_eq!(assigned_receipt_count_v1(&mixed, &panel, &h(100), &h(200)), 2);
    }

    /// The other two filters: a receipt about a different claim, and a `Mismatch`, both count for
    /// nothing.
    #[test]
    fn a_receipt_must_match_the_claim_and_agree_with_it() {
        let panel = vec![seat(1), seat(2)];
        let wrong_root = vec![receipt(1, 100, 999, PalwReceiptVerdictV1::Match)];
        assert_eq!(assigned_receipt_count_v1(&wrong_root, &panel, &h(100), &h(200)), 0, "a different root is a different claim");
        let wrong_block = vec![receipt(1, 999, 200, PalwReceiptVerdictV1::Match)];
        assert_eq!(assigned_receipt_count_v1(&wrong_block, &panel, &h(100), &h(200)), 0);
        let mismatch = vec![receipt(1, 100, 200, PalwReceiptVerdictV1::Mismatch)];
        assert_eq!(assigned_receipt_count_v1(&mismatch, &panel, &h(100), &h(200)), 0, "a mismatch routes to dispute");
        // One identity filing twice is one voice.
        let twice = vec![receipt(1, 100, 200, PalwReceiptVerdictV1::Match), receipt(1, 100, 200, PalwReceiptVerdictV1::Match)];
        assert_eq!(assigned_receipt_count_v1(&twice, &panel, &h(100), &h(200)), 1);
    }

    /// **Every unresolved fact is an error, and the error says which one.**
    ///
    /// This is blocker 6's root cause made unrepresentable: the old walk turned pruned data into
    /// an empty vector and computed a plausible answer from it, which two nodes with different
    /// pruning points computed differently.
    #[test]
    fn an_unresolved_fact_refuses_and_names_itself() {
        let full = PalwResolvedBlockFactsV1 {
            accepted_daa: Some(1_000),
            pov_daa: Some(2_000),
            assigned_receipts: Some(3),
            convicted_before_close: Some(false),
            dispute_open_or_unadjudicable: Some(false),
        };
        assert!(weight_facts_v1(&full, 500).is_ok());

        for (name, mutate) in [
            ("the commitment's acceptance DAA", (|f: &mut PalwResolvedBlockFactsV1| f.accepted_daa = None) as fn(&mut _)),
            ("the evaluating chain's DAA", |f: &mut PalwResolvedBlockFactsV1| f.pov_daa = None),
            ("the assigned receipt count", |f: &mut PalwResolvedBlockFactsV1| f.assigned_receipts = None),
            ("the conviction state", |f: &mut PalwResolvedBlockFactsV1| f.convicted_before_close = None),
            ("the dispute state", |f: &mut PalwResolvedBlockFactsV1| f.dispute_open_or_unadjudicable = None),
        ] {
            let mut broken = full.clone();
            mutate(&mut broken);
            assert_eq!(weight_facts_v1(&broken, 500), Err(PalwFactsError::Unresolved { what: name }));
        }
        // A zero window would finalize everything at admission.
        assert_eq!(weight_facts_v1(&full, 0), Err(PalwFactsError::ZeroChallengeWindow));
    }

    /// The window test is chain-relative, and a point of view BEHIND the acceptance — the
    /// ordinary state during a reorg — is simply "not closed" rather than an underflow.
    #[test]
    fn the_window_is_measured_on_the_chain_being_weighed() {
        let at = |pov: u64| PalwResolvedBlockFactsV1 {
            accepted_daa: Some(1_000),
            pov_daa: Some(pov),
            assigned_receipts: Some(3),
            convicted_before_close: Some(false),
            dispute_open_or_unadjudicable: Some(false),
        };
        assert!(!weight_facts_v1(&at(1_500), 500).unwrap().challenge_window_closed, "exactly at the window is not past it");
        assert!(weight_facts_v1(&at(1_501), 500).unwrap().challenge_window_closed);
        assert!(!weight_facts_v1(&at(900), 500).unwrap().challenge_window_closed, "a reorged-behind view is not closed");
    }

    /// The assembled facts drive the ramp exactly as the ramp's own table says — the layers meet
    /// where they are supposed to.
    #[test]
    fn assembled_facts_drive_the_ramp() {
        const PARAMS: PalwWeightParamsV1 = PalwWeightParamsV1 { receipt_quorum: 3, rho_r_permille: 900 };
        let build = |receipts: u32, pov: u64, convicted: bool, disputed: bool| {
            weight_facts_v1(
                &PalwResolvedBlockFactsV1 {
                    accepted_daa: Some(1_000),
                    pov_daa: Some(pov),
                    assigned_receipts: Some(receipts),
                    convicted_before_close: Some(convicted),
                    dispute_open_or_unadjudicable: Some(disputed),
                },
                500,
            )
            .unwrap()
        };
        assert_eq!(ramp_stage_v1(&build(0, 1_100, false, false), &PARAMS), PalwWorkRampStageV1::Provisional);
        assert_eq!(ramp_stage_v1(&build(3, 1_100, false, false), &PARAMS), PalwWorkRampStageV1::ReceiptLicensed);
        assert_eq!(ramp_stage_v1(&build(3, 2_000, false, false), &PalwWeightParamsV1 { ..PARAMS }), PalwWorkRampStageV1::Final);
        // The private fork: window closed, no assigned receipts, never matures.
        assert_eq!(ramp_stage_v1(&build(0, 2_000, false, false), &PARAMS), PalwWorkRampStageV1::Provisional);
        assert_eq!(ramp_stage_v1(&build(9, 1_100, true, false), &PARAMS), PalwWorkRampStageV1::Voided);
    }

    /// pwu comes from chain state on this side too, and an unresolved class is an error rather
    /// than a zero-weight block.
    #[test]
    fn pwu_is_resolved_or_refused() {
        let target = u128::MAX >> 10; // 1_024 expected attempts
        assert_eq!(block_pwu_v1(Some(target), Some(100)), Ok(102_400));
        assert_eq!(block_pwu_v1(None, Some(100)), Err(PalwFactsError::Unresolved { what: "the class's DAA target" }));
        assert_eq!(
            block_pwu_v1(Some(target), None),
            Err(PalwFactsError::Unresolved { what: "the class's per-inference cost" })
        );
    }
}
