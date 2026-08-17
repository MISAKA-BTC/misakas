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

// ---------------------------------------------------------------------------------------------
// The resolver: carriage records + chain-scoped views -> resolved facts
// ---------------------------------------------------------------------------------------------

/// Everything the resolver reads, gathered by the caller from the chain it is evaluating.
///
/// The caller owns the walk; this owns the interpretation. That split is what keeps the
/// interpretation testable without a database and keeps the walk's chain-scoping explicit at its
/// own call site rather than hidden behind a store handle.
pub struct PalwResolverInputV1<'a> {
    /// Carriage records accepted on the chain being evaluated, within the challenge horizon.
    /// Each is `(kind, accepted_daa, body)` exactly as the store holds it.
    pub carriage: &'a [(u8, u64, Vec<u8>)],
    /// The panel drawn for this block's commitment.
    pub panel: &'a [crate::palw_job_panel::PalwPanelSeatV3],
    /// This block's own facts.
    pub block_hash: Hash64,
    pub commitment_root: Hash64,
    pub execution_class_id: Hash64,
    /// DAA at which this block's commitment was accepted, and the evaluating chain's own DAA.
    pub accepted_daa: u64,
    pub pov_daa: u64,
    /// The class's target at this chain point — `None` when the view does not hold the class.
    pub class_target: Option<u128>,
    /// The class's registered per-inference cost.
    pub pwu_per_inference: Option<u64>,
    /// The class's registered challenge window.
    pub w_challenge: u64,
    /// The class's registered rung window — what a bisection deadline is derived from.
    pub w_round: u64,
}

/// Decode the carriage records this block's facts depend on and assemble them.
///
/// Undecodable bodies are SKIPPED rather than failing the resolve: a body that does not decode as
/// its kind never passed admission on this chain, so it is not a fact about this chain — and
/// making one poison the whole resolve would hand any peer a way to make a block unweighable.
/// What is refused is a fact this node genuinely cannot determine, which is
/// [`weight_facts_v1`]'s job.
pub fn resolve_block_facts_v1(input: &PalwResolverInputV1<'_>) -> PalwResolvedBlockFactsV1 {
    use crate::palw_carriage::{
        PALW_CARRIAGE_KIND_EQUIVOCATION, PALW_CARRIAGE_KIND_RECEIPT, PALW_CARRIAGE_KIND_STEP_CONVICTION, PalwCarriageV1,
        decode_palw_stage1_body,
    };

    let mut receipts = Vec::new();
    let mut convicted_before_close = false;
    let window_close = input.accepted_daa.saturating_add(input.w_challenge);

    for (kind, accepted_daa, body) in input.carriage {
        match *kind {
            PALW_CARRIAGE_KIND_RECEIPT => {
                if let Ok(PalwCarriageV1::Receipt(r)) = decode_palw_stage1_body(*kind, body) {
                    receipts.push(r.receipt);
                }
            }
            // A conviction counts only if it was accepted BEFORE the window closed. A later one is
            // a protocol-failure telemetry event, never a weight fact (ADR-0038 W5), and the
            // comparison is against chain DAA rather than a clock so every node agrees.
            PALW_CARRIAGE_KIND_STEP_CONVICTION => {
                if *accepted_daa <= window_close
                    && let Ok(PalwCarriageV1::StepConviction(c)) = decode_palw_stage1_body(*kind, body)
                    && c.refutation.binding.full_logits_trace_root == input.commitment_root
                {
                    convicted_before_close = true;
                }
            }
            PALW_CARRIAGE_KIND_EQUIVOCATION => {
                if *accepted_daa <= window_close
                    && let Ok(PalwCarriageV1::Equivocation(e)) = decode_palw_stage1_body(*kind, body)
                    && (e.certificate.attestation_a.full_logits_trace_root == input.commitment_root
                        || e.certificate.attestation_b.full_logits_trace_root == input.commitment_root)
                {
                    convicted_before_close = true;
                }
            }
            _ => {}
        }
    }

    PalwResolvedBlockFactsV1 {
        accepted_daa: Some(input.accepted_daa),
        pov_daa: Some(input.pov_daa),
        assigned_receipts: Some(assigned_receipt_count_v1(&receipts, input.panel, &input.block_hash, &input.commitment_root)),
        convicted_before_close: Some(convicted_before_close),
        dispute_open_or_unadjudicable: Some(dispute_is_open_v1(input)),
    }
}

/// Whether a bisection ladder over this commitment is still running.
///
/// Derived from the ladder's own moves rather than from a session store: the moves ARE carriage
/// records on the chain being evaluated, so replaying them is chain-scoped by construction and
/// needs no second source that could disagree with the first.
///
/// A session that reached `Terminal` or `Abandoned` is decided and no longer blocks maturity. A
/// session still awaiting a move is an open dispute, and an open dispute is not an absence of
/// refutation — it is a refutation still being answered
/// ([`crate::palw_weight::ramp_stage_v1`]).
///
/// A move that does not decode, or that the ladder refuses as illegal, is skipped: it never
/// advanced the game on this chain, so it is not a fact about the game's state. Skipping is safe
/// in the direction that matters — an unparseable move cannot END a dispute, only fail to
/// advance one.
fn dispute_is_open_v1(input: &PalwResolverInputV1<'_>) -> bool {
    use crate::palw_bisect::{PalwBisectLadderV1, PalwBisectTurnV1};
    use crate::palw_carriage::{PALW_CARRIAGE_KIND_BISECT_MOVE, PalwBisectMoveBodyV1, PalwCarriageV1, decode_palw_stage1_body};

    let moves: Vec<(u64, PalwBisectMoveBodyV1)> = input
        .carriage
        .iter()
        .filter(|(kind, _, _)| *kind == PALW_CARRIAGE_KIND_BISECT_MOVE)
        .filter_map(|(kind, daa, body)| match decode_palw_stage1_body(*kind, body) {
            Ok(PalwCarriageV1::BisectMove(m)) => Some((*daa, m.body)),
            _ => None,
        })
        .collect();

    for (open_daa, body) in &moves {
        let PalwBisectMoveBodyV1::Open { job_context_hash, committed_root, challenger_id, responder_id, space, space_size } = body
        else {
            continue;
        };
        if *committed_root != input.commitment_root {
            continue;
        }
        let Ok(mut ladder) = PalwBisectLadderV1::open(
            job_context_hash,
            committed_root,
            challenger_id,
            responder_id,
            *space,
            *space_size,
            *open_daa,
            open_daa.saturating_add(input.w_round.max(1)),
        ) else {
            continue;
        };
        // Replay this session's later moves in accepted order. `carriage` is already in the
        // caller's walk order; ties are impossible within one session because a ladder refuses a
        // move that is not its turn.
        for (daa, later) in &moves {
            match later {
                PalwBisectMoveBodyV1::Disclosure(d) if d.session_id == ladder.session_id() => {
                    let _ = ladder.apply_disclosure(d, *daa, input.w_round);
                }
                PalwBisectMoveBodyV1::Verdict(v) if v.session_id == ladder.session_id() => {
                    let _ = ladder.apply_verdict(v, *daa, input.w_round);
                }
                _ => {}
            }
        }
        match ladder.turn() {
            // Decided: the dispute no longer blocks maturity.
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => {}
            // Still someone's move: the dispute is open.
            PalwBisectTurnV1::AwaitDisclosure | PalwBisectTurnV1::AwaitVerdict => return true,
        }
    }
    false
}

/// The block's weight contribution, resolved end to end, or the fact that stopped it.
pub fn resolve_block_weight_v1(
    input: &PalwResolverInputV1<'_>,
    ramp: &crate::palw_weight::PalwWeightParamsV1,
) -> Result<crate::palw_chain_weight::PalwBlockWeightV1, PalwFactsError> {
    let resolved = resolve_block_facts_v1(input);
    let facts = weight_facts_v1(&resolved, input.w_challenge)?;
    let pwu = block_pwu_v1(input.class_target, input.pwu_per_inference)?;
    Ok(crate::palw_chain_weight::PalwBlockWeightV1 { pwu, stage: crate::palw_weight::ramp_stage_v1(&facts, ramp) })
}

#[cfg(test)]
mod resolver_tests {
    use super::*;
    use crate::palw_bisect::{PalwBisectDisclosureV1, PalwBisectSpaceV1, PalwBisectVerdictV1, bisect_session_id_v1};
    use crate::palw_carriage::{
        PALW_CARRIAGE_KIND_BISECT_MOVE, PALW_CARRIAGE_KIND_RECEIPT, PALW_CARRIAGE_VERSION_V1, PalwBisectMoveBodyV1,
        PalwBisectMoveCarriageV1, PalwCarriageV1, PalwReceiptCarriageV1, encode_palw_carriage_v1,
    };
    use crate::palw_job_panel::PalwPanelSeatV3;
    use crate::palw_weight::{PalwWeightParamsV1, PalwWorkRampStageV1};
    use crate::tx::{TransactionId, TransactionOutpoint};

    const W_CHALLENGE: u64 = 500;
    const W_ROUND: u64 = 30;
    const RAMP: PalwWeightParamsV1 = PalwWeightParamsV1 { receipt_quorum: 2, rho_r_permille: 900 };

    fn h(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }
    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_bytes([seed; 64]), index: 0 }
    }
    fn seat(seed: u8) -> PalwPanelSeatV3 {
        PalwPanelSeatV3 { validator_id: h(seed as u64), bond_outpoint: op(seed) }
    }

    /// Stage-1 body bytes, exactly as the store holds them.
    fn body(obj: &PalwCarriageV1) -> Vec<u8> {
        encode_palw_carriage_v1(obj)[7..].to_vec()
    }

    fn receipt_row(bond: u8, daa: u64) -> (u8, u64, Vec<u8>) {
        let r = PalwReceiptCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            receipt: crate::palw_receipt::PalwVerificationReceiptV1 {
                version: crate::palw_receipt::PALW_RECEIPT_VERSION_V1,
                target_block_hash: h(0xB0),
                target_commitment_root: h(0xC0),
                execution_class_id: h(0xC1),
                sample_coordinates: vec![crate::palw_receipt::PalwSampleCoordinateV1 {
                    token_index: 0,
                    layer_index: 0,
                    node_slot: 0,
                    unit_index: 0,
                }],
                observed_roots: vec![h(0x74)],
                verdict: crate::palw_receipt::PalwReceiptVerdictV1::Match,
                verifier_bond_outpoint: op(bond),
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            },
        };
        (PALW_CARRIAGE_KIND_RECEIPT, daa, body(&PalwCarriageV1::Receipt(r)))
    }

    fn bisect_row(b: PalwBisectMoveBodyV1, daa: u64) -> (u8, u64, Vec<u8>) {
        let m = PalwBisectMoveCarriageV1 { version: PALW_CARRIAGE_VERSION_V1, challenger_bond_outpoint: op(0xC9), body: b };
        (PALW_CARRIAGE_KIND_BISECT_MOVE, daa, body(&PalwCarriageV1::BisectMove(m)))
    }

    fn input<'a>(carriage: &'a [(u8, u64, Vec<u8>)], panel: &'a [PalwPanelSeatV3], pov: u64) -> PalwResolverInputV1<'a> {
        PalwResolverInputV1 {
            carriage,
            panel,
            block_hash: h(0xB0),
            commitment_root: h(0xC0),
            execution_class_id: h(0xC1),
            accepted_daa: 1_000,
            pov_daa: pov,
            class_target: Some(u128::MAX >> 10), // 1_024 expected attempts
            pwu_per_inference: Some(100),
            w_challenge: W_CHALLENGE,
            w_round: W_ROUND,
        }
    }

    /// **Every fact resolves.** This is what lets the fork-choice fence stop refusing: the
    /// resolver produces a complete `PalwResolvedBlockFactsV1`, so `weight_facts_v1` has nothing
    /// left to be unable to answer.
    #[test]
    fn the_resolver_answers_every_fact() {
        let carriage = vec![receipt_row(1, 1_050), receipt_row(2, 1_060)];
        let panel = vec![seat(1), seat(2), seat(3)];
        let resolved = resolve_block_facts_v1(&input(&carriage, &panel, 1_100));
        assert_eq!(resolved.accepted_daa, Some(1_000));
        assert_eq!(resolved.pov_daa, Some(1_100));
        assert_eq!(resolved.assigned_receipts, Some(2));
        assert_eq!(resolved.convicted_before_close, Some(false));
        assert_eq!(resolved.dispute_open_or_unadjudicable, Some(false));
        // ...and therefore a weight, end to end.
        let weight = resolve_block_weight_v1(&input(&carriage, &panel, 1_100), &RAMP).unwrap();
        assert_eq!(weight.pwu, 102_400);
        assert_eq!(weight.stage, PalwWorkRampStageV1::ReceiptLicensed);
    }

    /// An open ladder blocks maturity; a terminated one does not. Derived from the moves
    /// themselves, so no session store can disagree with the chain.
    #[test]
    fn an_open_ladder_is_an_open_dispute() {
        let open = PalwBisectMoveBodyV1::Open {
            job_context_hash: h(0x11),
            committed_root: h(0xC0),
            challenger_id: h(0x33),
            responder_id: h(0x44),
            space: PalwBisectSpaceV1::StepLeaves,
            space_size: 4,
        };
        let session = bisect_session_id_v1(&h(0x11), &h(0xC0), &h(0x33), &h(0x44), PalwBisectSpaceV1::StepLeaves, 4);
        let panel = vec![seat(1), seat(2)];

        // Opened and awaiting the first disclosure: open.
        let just_opened = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), bisect_row(open.clone(), 1_070)];
        assert_eq!(resolve_block_facts_v1(&input(&just_opened, &panel, 1_100)).dispute_open_or_unadjudicable, Some(true));
        // An open dispute cannot mature, whatever the receipts say.
        let weight = resolve_block_weight_v1(&input(&just_opened, &panel, 9_000), &RAMP).unwrap();
        assert_eq!(weight.stage, PalwWorkRampStageV1::Provisional, "an open dispute is not an absence of refutation");

        // Played to the terminal index: decided, no longer blocking.
        let mut rows = just_opened.clone();
        let mut daa = 1_080;
        for round in 0..2u32 {
            let mid = if round == 0 { 2 } else { 1 };
            rows.push(bisect_row(
                PalwBisectMoveBodyV1::Disclosure(PalwBisectDisclosureV1 {
                    version: 1,
                    session_id: session,
                    round,
                    midpoint: mid,
                    mid_state: h(mid),
                }),
                daa,
            ));
            daa += 5;
            rows.push(bisect_row(
                PalwBisectMoveBodyV1::Verdict(PalwBisectVerdictV1 { version: 1, session_id: session, round, agree: false }),
                daa,
            ));
            daa += 5;
        }
        assert_eq!(
            resolve_block_facts_v1(&input(&rows, &panel, 1_200)).dispute_open_or_unadjudicable,
            Some(false),
            "a ladder played to its terminal index is decided"
        );

        // A ladder over a DIFFERENT commitment is not this block's dispute.
        let elsewhere = PalwBisectMoveBodyV1::Open {
            job_context_hash: h(0x11),
            committed_root: h(0xDEAD),
            challenger_id: h(0x33),
            responder_id: h(0x44),
            space: PalwBisectSpaceV1::StepLeaves,
            space_size: 4,
        };
        let other = vec![receipt_row(1, 1_050), bisect_row(elsewhere, 1_070)];
        assert_eq!(resolve_block_facts_v1(&input(&other, &panel, 1_100)).dispute_open_or_unadjudicable, Some(false));
    }

    /// A conviction accepted before the window closes voids the block; one after it does not.
    /// The comparison is against chain DAA, so every node agrees about which side it fell.
    #[test]
    fn a_conviction_counts_only_before_the_window_closes() {
        use crate::palw_carriage::{PALW_CARRIAGE_KIND_EQUIVOCATION, PalwEquivocationCarriageV1};
        let equivocation = |daa: u64| {
            let ctx = crate::palw_step_refute::tests::skeleton_refutation().binding.job_context;
            let att = |root: Hash64| crate::palw_slash::PalwExecutionAttestationV1 {
                version: crate::palw_slash::PALW_S_OBJECT_VERSION_V1,
                executor_id: h(0xE1),
                job_context_hash: ctx.context_hash(),
                full_logits_trace_root: root,
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            };
            let e = PalwEquivocationCarriageV1 {
                version: PALW_CARRIAGE_VERSION_V1,
                accused_bond_outpoint: op(0xB1),
                certificate: crate::palw_slash::PalwClassContradictionCertificateV1 {
                    version: crate::palw_slash::PALW_S_OBJECT_VERSION_V1,
                    attestation_a: att(h(0xC0)), // this block's commitment root
                    attestation_b: att(h(0x02)),
                    job_context: ctx,
                },
            };
            (PALW_CARRIAGE_KIND_EQUIVOCATION, daa, body(&PalwCarriageV1::Equivocation(e)))
        };
        let panel = vec![seat(1), seat(2)];

        // Window closes at 1_000 + 500 = 1_500.
        let before = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), equivocation(1_400)];
        assert_eq!(resolve_block_facts_v1(&input(&before, &panel, 9_000)).convicted_before_close, Some(true));
        assert_eq!(resolve_block_weight_v1(&input(&before, &panel, 9_000), &RAMP).unwrap().stage, PalwWorkRampStageV1::Voided);

        let after = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), equivocation(1_600)];
        assert_eq!(resolve_block_facts_v1(&input(&after, &panel, 9_000)).convicted_before_close, Some(false));
        assert_eq!(
            resolve_block_weight_v1(&input(&after, &panel, 9_000), &RAMP).unwrap().stage,
            PalwWorkRampStageV1::Final,
            "a late conviction cannot unmake finality (W5)"
        );
    }

    /// An undecodable body is skipped, not fatal: it never passed admission on this chain, so it
    /// is not a fact about it — and making one poison the resolve would hand any peer a way to
    /// make a block unweighable.
    #[test]
    fn junk_carriage_is_skipped_rather_than_fatal() {
        let carriage = vec![
            (PALW_CARRIAGE_KIND_RECEIPT, 1_050, vec![0xFF; 8]),
            receipt_row(1, 1_050),
            (PALW_CARRIAGE_KIND_BISECT_MOVE, 1_060, vec![0x00; 3]),
            receipt_row(2, 1_060),
        ];
        let panel = vec![seat(1), seat(2)];
        let resolved = resolve_block_facts_v1(&input(&carriage, &panel, 1_100));
        assert_eq!(resolved.assigned_receipts, Some(2), "the good receipts still count");
        assert_eq!(resolved.dispute_open_or_unadjudicable, Some(false), "junk cannot open a dispute");
    }

    /// The resolve is order-free: the same records in any walk order give the same facts.
    #[test]
    fn resolution_does_not_depend_on_walk_order() {
        let carriage = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), receipt_row(3, 1_070)];
        let panel = vec![seat(1), seat(2), seat(3)];
        let expected = resolve_block_facts_v1(&input(&carriage, &panel, 1_100));
        let mut reversed = carriage.clone();
        reversed.reverse();
        assert_eq!(resolve_block_facts_v1(&input(&reversed, &panel, 1_100)), expected);
        let rotated = [carriage[2].clone(), carriage[0].clone(), carriage[1].clone()];
        assert_eq!(resolve_block_facts_v1(&input(&rotated, &panel, 1_100)), expected);
    }
}
