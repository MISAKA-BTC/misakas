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
use crate::tx::TransactionOutpoint;
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
    ///
    /// The slice's ORDER is not trusted. Anything order-sensitive here sorts it into the canonical
    /// order first — see [`canonical_carriage_order_v1`].
    pub carriage: &'a [(u8, u64, Vec<u8>)],
    /// The network identity the receipt signing digest is bound to
    /// ([`crate::palw_receipt::palw_receipt_message_v1`]).
    ///
    /// Load-bearing rather than bookkeeping: without it a receipt signed for devnet would verify on
    /// mainnet, which is the same cross-network replay ADR-0027's slash certificates had to close.
    pub network_id: &'a [u8],
    /// Bonds as they stand at the chain point being evaluated.
    ///
    /// A view, not a store, for blocker 6(b)'s reason: a store answers about this node's virtual
    /// tip, and a weight fact that depends on where the tip happens to point is not a fact about
    /// the chain being weighed. This is what turns a receipt's `verifier_bond_outpoint` into a
    /// public key and an activity window.
    pub bonds: &'a crate::dns_finality::ActiveBondView,
    /// The oracle a step conviction is re-executed against.
    ///
    /// A node that holds no weights answers `None` and every step conviction lands
    /// `Unadjudicable`, which is the correct direction: a node that cannot check a refutation has
    /// not established that the step is wrong, so it must not let the claim void the block's
    /// weight.
    pub step_weights: &'a dyn crate::palw_step_refute::PalwWeightOracleV1,
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
pub fn resolve_block_facts_v1<F>(input: &PalwResolverInputV1<'_>, verify_signature: F) -> PalwResolvedBlockFactsV1
where
    F: Fn(&[u8], &kaspa_hashes::Hash, &[u8]) -> bool,
{
    use crate::palw_carriage::{
        PALW_CARRIAGE_KIND_EQUIVOCATION, PALW_CARRIAGE_KIND_RECEIPT, PALW_CARRIAGE_KIND_STEP_CONVICTION, PalwCarriageV1,
        adjudicate_equivocation_carriage_v1, adjudicate_step_conviction_carriage_v1, decode_palw_stage1_body,
    };

    let mut receipts = Vec::new();
    let mut convicted_before_close = false;
    let window_close = input.accepted_daa.saturating_add(input.w_challenge);

    for (kind, accepted_daa, body) in input.carriage {
        match *kind {
            PALW_CARRIAGE_KIND_RECEIPT => {
                if let Ok(PalwCarriageV1::Receipt(r)) = decode_palw_stage1_body(*kind, body)
                    && receipt_is_authentic_v1(&r.receipt, input, &verify_signature)
                {
                    receipts.push(r.receipt);
                }
            }
            // A conviction counts only if it was ADJUDICATED and was accepted BEFORE the window
            // closed. A later one is a protocol-failure telemetry event, never a weight fact
            // (ADR-0038 W5), and the comparison is against chain DAA rather than a clock so every
            // node agrees.
            //
            // Adjudication, not shape. Matching the trace root was the whole test before, so a
            // well-formed carriage naming this commitment — no signature, no proof — voided the
            // block's PALW weight permanently, which is B9's shape (re-audit §3.1). Both kinds now
            // run the same adjudicator a slash does, against the accused's own bond and key, and
            // the adjudicators refuse rather than convict when they cannot decide.
            PALW_CARRIAGE_KIND_STEP_CONVICTION => {
                if *accepted_daa <= window_close
                    && let Ok(PalwCarriageV1::StepConviction(c)) = decode_palw_stage1_body(*kind, body)
                    && c.refutation.binding.full_logits_trace_root == input.commitment_root
                    && let Some(accused) = input.bonds.active_bond_at(&c.accused_bond_outpoint, input.pov_daa)
                    && adjudicate_step_conviction_carriage_v1(&c, accused, input.pov_daa, input.step_weights, &verify_signature)
                        .is_ok()
                {
                    convicted_before_close = true;
                }
            }
            PALW_CARRIAGE_KIND_EQUIVOCATION => {
                if *accepted_daa <= window_close
                    && let Ok(PalwCarriageV1::Equivocation(e)) = decode_palw_stage1_body(*kind, body)
                    && (e.certificate.attestation_a.full_logits_trace_root == input.commitment_root
                        || e.certificate.attestation_b.full_logits_trace_root == input.commitment_root)
                    && let Some(accused) = input.bonds.active_bond_at(&e.accused_bond_outpoint, input.pov_daa)
                    && adjudicate_equivocation_carriage_v1(&e, accused, input.pov_daa, &verify_signature).is_ok()
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

/// Is this receipt a bonded panel member's authenticated claim, or merely bytes shaped like one?
///
/// [`assigned_receipt_count_v1`] answers "was this filer DRAWN"; this answers "did this filer
/// actually file it". Both are required and neither implies the other: panel membership without a
/// signature lets anyone file in a drawn verifier's name, and a signature without membership counts
/// an opinion nobody was assigned to give.
///
/// Four conditions, each of which a forged receipt fails:
///
/// 1. **shape** — arity, ordering and signature length ([`PalwVerificationReceiptV1::validate_shape`]);
/// 2. **class** — the verifier replayed under the class the block claims. A cross-class replay is a
///    different computation, so its roots are telemetry and never evidence (ADR-0037 I11);
/// 3. **bond** — `verifier_bond_outpoint` resolves to a bond ACTIVE at the evaluating point of view.
///    An unbonded or already-slashed filer has nothing at stake and is not accountable;
/// 4. **signature** — ML-DSA-87 over the receipt's own digest, under the resolved bond's key.
///
/// Before this existed the resolver pushed every decodable receipt, so quorum was a count of
/// whoever chose to speak — fabricate `k` receipts naming `k` drawn seats and a fabricated block
/// matured to full weight without any of those verifiers doing anything (re-audit §3.1).
fn receipt_is_authentic_v1<F>(receipt: &PalwVerificationReceiptV1, input: &PalwResolverInputV1<'_>, verify_signature: &F) -> bool
where
    F: Fn(&[u8], &kaspa_hashes::Hash, &[u8]) -> bool,
{
    if receipt.validate_shape().is_err() {
        return false;
    }
    if receipt.execution_class_id != input.execution_class_id {
        return false;
    }
    let Some(bond) = input.bonds.active_bond_at(&receipt.verifier_bond_outpoint, input.pov_daa) else {
        return false;
    };
    verify_signature(&bond.validator_pubkey, &receipt.message(input.network_id), &receipt.signature)
}

/// The order carriage records are interpreted in, as a function of the records themselves.
///
/// `(accepted_daa, kind, body_bytes)`, ascending. Two nodes with the same DAG therefore replay the
/// same sequence regardless of how each one's walk happened to collect it — which is the property
/// [`dispute_is_open_v1`] needs and previously only *documented*: it replayed the caller's slice
/// order, so a forward walk could reach `Terminal` where a backward walk reached `AwaitVerdict`,
/// i.e. the same DAG yielding `Final` on one node and `Provisional` on another (re-audit §3.2).
///
/// The tiebreak is the body bytes because they are canonical Borsh and totally ordered; the
/// neighbouring precedent (`compute_palw_credit_outputs`' `(accepted_daa, tx_id)`) has a
/// transaction id available and this input does not.
pub fn canonical_carriage_order_v1(carriage: &[(u8, u64, Vec<u8>)]) -> Vec<&(u8, u64, Vec<u8>)> {
    let mut ordered: Vec<&(u8, u64, Vec<u8>)> = carriage.iter().collect();
    ordered.sort_by(|a, b| (a.1, a.0, &a.2).cmp(&(b.1, b.0, &b.2)));
    ordered
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

    // Canonical order, not the caller's slice order — see `canonical_carriage_order_v1`. The
    // challenger bond rides alongside because an Open only counts from an accountable party.
    let moves: Vec<(u64, TransactionOutpoint, PalwBisectMoveBodyV1)> = canonical_carriage_order_v1(input.carriage)
        .into_iter()
        .filter(|(kind, _, _)| *kind == PALW_CARRIAGE_KIND_BISECT_MOVE)
        .filter_map(|(kind, daa, body)| match decode_palw_stage1_body(*kind, body) {
            Ok(PalwCarriageV1::BisectMove(m)) => Some((*daa, m.challenger_bond_outpoint, m.body)),
            _ => None,
        })
        .collect();

    for (open_daa, challenger_bond, body) in &moves {
        let PalwBisectMoveBodyV1::Open { job_context_hash, committed_root, challenger_id, responder_id, space, space_size } = body
        else {
            continue;
        };
        if *committed_root != input.commitment_root {
            continue;
        }
        // An Open from a bond that is not ACTIVE here opens nothing. Without this, one unbonded
        // record pinned any block at `Provisional` for as long as it stayed in the horizon — a
        // griefing veto over maturity at zero cost and with nobody to charge (re-audit §3.1). The
        // bond is what makes a baseless dispute chargeable; `PalwBisectMoveCarriageV1` carries the
        // outpoint precisely so this check has something to resolve.
        if input.bonds.active_bond_at(challenger_bond, input.pov_daa).is_none() {
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
        // Replay this session's later moves in the canonical order established above.
        for (daa, _, later) in &moves {
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
            // Someone owes a move. Whether that still clouds the block depends on WHO went silent,
            // so ask the ladder's own rule (`declare_no_show`, the same machine both parties play
            // on) rather than re-deriving a deadline here.
            //
            // `Err` — the rung deadline has not passed: a live dispute, genuinely open.
            //
            // `Ok(Responder)` — the accused executor abandoned its own defence. Silence past a
            // deadline is the objective offence `M-O3` and it decides the dispute AGAINST the block,
            // so this must keep the block out of `Final`. Reporting it as decided-and-harmless was
            // the first shape of this code and it was backwards: it matured a block whose executor
            // walked away from a challenge. The conviction itself arrives as its own carriage; what
            // this flag carries is that the accusation is unresolved, which is exactly what the
            // field is named for.
            //
            // `Ok(Challenger)` — the challenger walked away. THIS is the case the deadline test
            // exists for: with it, a baseless Open stops freezing maturity once its author goes
            // quiet, and together with the bond requirement above a griefing veto now costs a bond
            // and expires.
            PalwBisectTurnV1::AwaitDisclosure | PalwBisectTurnV1::AwaitVerdict => {
                match ladder.declare_no_show(input.pov_daa) {
                    Err(_) => return true,
                    Ok(no_show) => match no_show.silent_party {
                        crate::palw_bisect::PalwBisectPartyV1::Responder => return true,
                        crate::palw_bisect::PalwBisectPartyV1::Challenger => {}
                    },
                }
            }
        }
    }
    false
}

/// The block's weight contribution, resolved end to end, or the fact that stopped it.
pub fn resolve_block_weight_v1<F>(
    input: &PalwResolverInputV1<'_>,
    ramp: &crate::palw_weight::PalwWeightParamsV1,
    verify_signature: F,
) -> Result<crate::palw_chain_weight::PalwBlockWeightV1, PalwFactsError>
where
    F: Fn(&[u8], &kaspa_hashes::Hash, &[u8]) -> bool,
{
    let resolved = resolve_block_facts_v1(input, verify_signature);
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

    const NETWORK: &[u8] = b"testnet-11";

    /// A node with no step weights: every step conviction lands `Unadjudicable`, which is what a
    /// real node without the oracle does.
    struct NoStepWeights;
    impl crate::palw_step_refute::PalwWeightOracleV1 for NoStepWeights {
        fn weight_row(&self, _t: &str, _l: Option<u16>, _r: u32, _e: u32) -> Option<Vec<u8>> {
            None
        }
    }

    /// A bond whose public key is `[seed; 32]`, active from DAA 0.
    fn bond(seed: u8) -> crate::dns_finality::StakeBondRecord {
        crate::dns_finality::StakeBondRecord {
            version: 1,
            bond_outpoint: op(seed),
            owner_pubkey_hash: h(seed as u64),
            validator_pubkey_hash: h(seed as u64),
            validator_pubkey: vec![seed; 32],
            amount: 20_000,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbonding_period_blocks: 100,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: crate::dns_finality::BondStatus::Active,
        }
    }

    /// Bonds for the seats the fixtures draw, the challenger bond `bisect_row` names, and the
    /// ACCUSED bond an equivocation certificate points at.
    ///
    /// The accused one needs a matching `validator_pubkey_hash`: the adjudicator refuses a
    /// certificate whose signer is not the bond it accuses, which is what stops a certificate from
    /// naming an innocent bond as the slash target.
    fn bonds() -> crate::dns_finality::ActiveBondView {
        let mut accused = bond(0xB1);
        accused.validator_pubkey_hash = h(0xE1);
        crate::dns_finality::ActiveBondView::from_records(
            [1u8, 2, 3, 0xC9].into_iter().map(|s| (op(s), bond(s))).chain([(op(0xB1), accused)]),
        )
    }

    /// The same bonds, minus the accused one — a node that cannot resolve the bond a conviction
    /// accuses.
    fn bonds_without_the_accused() -> crate::dns_finality::ActiveBondView {
        crate::dns_finality::ActiveBondView::from_records([1u8, 2, 3, 0xC9].into_iter().map(|s| (op(s), bond(s))))
    }

    /// Accepts a signature iff it is the fixture's own: `[0x5A; SIG_LEN]` under a known key. Stands
    /// in for ML-DSA-87, which lives outside consensus-core — the point under test is that the
    /// resolver ASKS, not what the curve answers.
    fn accept_fixture_signature(key: &[u8], _digest: &kaspa_hashes::Hash, signature: &[u8]) -> bool {
        !key.is_empty() && signature == vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN].as_slice()
    }

    /// Rejects everything — a node that cannot verify must count nothing.
    fn reject_every_signature(_key: &[u8], _digest: &kaspa_hashes::Hash, _signature: &[u8]) -> bool {
        false
    }

    fn input<'a>(
        carriage: &'a [(u8, u64, Vec<u8>)],
        panel: &'a [PalwPanelSeatV3],
        pov: u64,
        bonds: &'a crate::dns_finality::ActiveBondView,
        weights: &'a dyn crate::palw_step_refute::PalwWeightOracleV1,
    ) -> PalwResolverInputV1<'a> {
        PalwResolverInputV1 {
            carriage,
            network_id: NETWORK,
            bonds,
            step_weights: weights,
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
        let resolved = resolve_block_facts_v1(&input(&carriage, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature);
        assert_eq!(resolved.accepted_daa, Some(1_000));
        assert_eq!(resolved.pov_daa, Some(1_100));
        assert_eq!(resolved.assigned_receipts, Some(2));
        assert_eq!(resolved.convicted_before_close, Some(false));
        assert_eq!(resolved.dispute_open_or_unadjudicable, Some(false));
        // ...and therefore a weight, end to end.
        let weight = resolve_block_weight_v1(&input(&carriage, &panel, 1_100, &bonds(), &NoStepWeights), &RAMP, accept_fixture_signature).unwrap();
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
        assert_eq!(resolve_block_facts_v1(&input(&just_opened, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature).dispute_open_or_unadjudicable, Some(true));
        // An open dispute cannot mature, whatever the receipts say.
        let weight = resolve_block_weight_v1(&input(&just_opened, &panel, 9_000, &bonds(), &NoStepWeights), &RAMP, accept_fixture_signature).unwrap();
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
            resolve_block_facts_v1(&input(&rows, &panel, 1_200, &bonds(), &NoStepWeights), accept_fixture_signature).dispute_open_or_unadjudicable,
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
        assert_eq!(resolve_block_facts_v1(&input(&other, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature).dispute_open_or_unadjudicable, Some(false));
    }

    /// **§3.1: a receipt only counts if the bonded verifier actually signed it.**
    ///
    /// The resolver used to push every decodable receipt, so quorum counted whoever chose to speak:
    /// forge `k` receipts naming `k` drawn seats and a fabricated block matured to full weight
    /// without any of those verifiers doing anything. Each of the four gates is exercised
    /// separately, because any one of them left open restores the whole forgery.
    #[test]
    fn a_receipt_counts_only_when_its_bonded_verifier_signed_it() {
        let carriage = vec![receipt_row(1, 1_050), receipt_row(2, 1_060)];
        let panel = vec![seat(1), seat(2), seat(3)];

        // Baseline: both receipts verify, so both count.
        let ok = resolve_block_facts_v1(&input(&carriage, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature);
        assert_eq!(ok.assigned_receipts, Some(2));

        // (4) signature — a node that verifies nothing counts nothing. This is the gate whose
        // absence WAS the bug: the receipts are unchanged and still panel-drawn.
        let unsigned = resolve_block_facts_v1(&input(&carriage, &panel, 1_100, &bonds(), &NoStepWeights), reject_every_signature);
        assert_eq!(unsigned.assigned_receipts, Some(0), "an unverifiable receipt is not a licence");
        // ...and therefore the block does not reach the receipt-licensed stage.
        let weight =
            resolve_block_weight_v1(&input(&carriage, &panel, 1_100, &bonds(), &NoStepWeights), &RAMP, reject_every_signature)
                .unwrap();
        assert_eq!(weight.stage, PalwWorkRampStageV1::Provisional);

        // (3) bond — a verifier whose bond this chain point does not hold is not accountable.
        let no_bonds = crate::dns_finality::ActiveBondView::default();
        let unbonded =
            resolve_block_facts_v1(&input(&carriage, &panel, 1_100, &no_bonds, &NoStepWeights), accept_fixture_signature);
        assert_eq!(unbonded.assigned_receipts, Some(0), "an unbonded filer is not a licence");

        // (3b) bond ACTIVITY — a slashed bond resolves but is not active, so it cannot license.
        let slashed = crate::dns_finality::ActiveBondView::from_records([1u8, 2, 3].into_iter().map(|s| {
            let mut b = bond(s);
            b.slashed_at_daa_score = Some(500);
            b.status = crate::dns_finality::BondStatus::Slashed;
            (op(s), b)
        }));
        let after_slash =
            resolve_block_facts_v1(&input(&carriage, &panel, 1_100, &slashed, &NoStepWeights), accept_fixture_signature);
        assert_eq!(after_slash.assigned_receipts, Some(0), "a slashed verifier's receipt is not a licence");

        // (2) class — a replay under another class is a different computation (ADR-0037 I11).
        let held = bonds();
        let mut cross_class = input(&carriage, &panel, 1_100, &held, &NoStepWeights);
        cross_class.execution_class_id = h(0xDEAD);
        assert_eq!(
            resolve_block_facts_v1(&cross_class, accept_fixture_signature).assigned_receipts,
            Some(0),
            "a cross-class receipt is telemetry, never evidence"
        );

        // The network the digest is bound to is part of the question: the same bytes under another
        // network identity must not verify. (Checked through the digest the closure is handed.)
        let mut other_net = input(&carriage, &panel, 1_100, &held, &NoStepWeights);
        other_net.network_id = b"mainnet";
        let digest_here = carriage_receipt(&carriage[0]).message(NETWORK);
        let digest_there = carriage_receipt(&carriage[0]).message(b"mainnet");
        assert_ne!(digest_here, digest_there, "the signing digest must bind the network identity");
        // With a closure that accepts anything the count is unchanged; the binding lives in the
        // digest, which the assertion above pins.
        assert_eq!(resolve_block_facts_v1(&other_net, accept_fixture_signature).assigned_receipts, Some(2));
    }

    /// Pull the receipt back out of a carriage row, to inspect the digest it commits to.
    fn carriage_receipt(row: &(u8, u64, Vec<u8>)) -> crate::palw_receipt::PalwVerificationReceiptV1 {
        match crate::palw_carriage::decode_palw_stage1_body(row.0, &row.2) {
            Ok(PalwCarriageV1::Receipt(r)) => r.receipt,
            other => panic!("not a receipt row: {other:?}"),
        }
    }

    /// **§3.1: a conviction must be ADJUDICATED, not merely shaped.**
    ///
    /// Matching the trace root was the whole test before, so a well-formed carriage naming this
    /// commitment — no signature, no proof — voided the block's PALW weight permanently (B9's
    /// shape). Two ways adjudication can fail, and neither may convict.
    #[test]
    fn a_conviction_that_cannot_be_adjudicated_does_not_void_the_block() {
        use crate::palw_carriage::{PALW_CARRIAGE_KIND_EQUIVOCATION, PalwEquivocationCarriageV1};
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
                attestation_a: att(h(0xC0)),
                attestation_b: att(h(0x02)),
                job_context: ctx,
            },
        };
        let row = (PALW_CARRIAGE_KIND_EQUIVOCATION, 1_400u64, body(&PalwCarriageV1::Equivocation(e)));
        let carriage = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), row];
        let panel = vec![seat(1), seat(2)];

        // Adjudicable: it convicts (this is the same certificate the window test uses).
        assert_eq!(
            resolve_block_facts_v1(&input(&carriage, &panel, 9_000, &bonds(), &NoStepWeights), accept_fixture_signature)
                .convicted_before_close,
            Some(true)
        );

        // The accused's signatures do not verify -> not proven -> no conviction. The receipts stop
        // counting too under the same closure, so assert the conviction flag specifically.
        assert_eq!(
            resolve_block_facts_v1(&input(&carriage, &panel, 9_000, &bonds(), &NoStepWeights), reject_every_signature)
                .convicted_before_close,
            Some(false),
            "an unproven certificate must not void a block's weight"
        );

        // This node cannot resolve the accused bond -> it has no key to check against -> no
        // conviction. A node that cannot adjudicate has not established the offence.
        assert_eq!(
            resolve_block_facts_v1(
                &input(&carriage, &panel, 9_000, &bonds_without_the_accused(), &NoStepWeights),
                accept_fixture_signature
            )
            .convicted_before_close,
            Some(false),
            "an unresolvable accused bond must not void a block's weight"
        );
    }

    /// **§3.1: an Open from an unbonded challenger opens nothing.**
    ///
    /// One record with no bond behind it used to pin any block at `Provisional` for as long as it
    /// stayed in the horizon — a free, unattributable veto over maturity.
    #[test]
    fn an_unbonded_open_cannot_freeze_maturity() {
        let open = PalwBisectMoveBodyV1::Open {
            job_context_hash: h(0x11),
            committed_root: h(0xC0),
            challenger_id: h(0x33),
            responder_id: h(0x44),
            space: PalwBisectSpaceV1::StepLeaves,
            space_size: 4,
        };
        let panel = vec![seat(1), seat(2)];
        let rows = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), bisect_row(open, 1_070)];

        // `bisect_row` names bond 0xC9, which `bonds()` holds: a real dispute.
        assert_eq!(
            resolve_block_facts_v1(&input(&rows, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature)
                .dispute_open_or_unadjudicable,
            Some(true)
        );

        // The same record with that bond absent from this chain point: nothing is opened.
        let without_challenger =
            crate::dns_finality::ActiveBondView::from_records([1u8, 2, 3].into_iter().map(|s| (op(s), bond(s))));
        assert_eq!(
            resolve_block_facts_v1(&input(&rows, &panel, 1_100, &without_challenger, &NoStepWeights), accept_fixture_signature)
                .dispute_open_or_unadjudicable,
            Some(false),
            "an unbonded Open is not a dispute"
        );
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
        assert_eq!(resolve_block_facts_v1(&input(&before, &panel, 9_000, &bonds(), &NoStepWeights), accept_fixture_signature).convicted_before_close, Some(true));
        assert_eq!(resolve_block_weight_v1(&input(&before, &panel, 9_000, &bonds(), &NoStepWeights), &RAMP, accept_fixture_signature).unwrap().stage, PalwWorkRampStageV1::Voided);

        let after = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), equivocation(1_600)];
        assert_eq!(resolve_block_facts_v1(&input(&after, &panel, 9_000, &bonds(), &NoStepWeights), accept_fixture_signature).convicted_before_close, Some(false));
        assert_eq!(
            resolve_block_weight_v1(&input(&after, &panel, 9_000, &bonds(), &NoStepWeights), &RAMP, accept_fixture_signature).unwrap().stage,
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
        let resolved = resolve_block_facts_v1(&input(&carriage, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature);
        assert_eq!(resolved.assigned_receipts, Some(2), "the good receipts still count");
        assert_eq!(resolved.dispute_open_or_unadjudicable, Some(false), "junk cannot open a dispute");
    }

    /// The resolve is order-free: the same records in any walk order give the same facts.
    #[test]
    fn resolution_does_not_depend_on_walk_order() {
        let panel = vec![seat(1), seat(2), seat(3)];

        // Receipts alone are the WEAK half of this property: they land in a `BTreeSet`, so they are
        // order-free by construction and permuting them proves nothing about the resolver.
        let receipts_only = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), receipt_row(3, 1_070)];
        let expected =
            resolve_block_facts_v1(&input(&receipts_only, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature);
        let mut reversed = receipts_only.clone();
        reversed.reverse();
        assert_eq!(
            resolve_block_facts_v1(&input(&reversed, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature),
            expected
        );

        // The BISECTION LADDER is the order-sensitive part, and the only one. A ladder refuses a
        // move that is not its turn, so replaying a played-out session in the wrong order drops the
        // moves it cannot accept: forward reaches `Terminal` (dispute decided), while backward
        // applies the round-0 verdict first — refused — and stalls at `AwaitDisclosure`, i.e. the
        // SAME DAG yielding `Final` on one node and `Provisional` on another. That is the §3.2
        // defect, and permuting a multi-move session is what exercises it.
        let open = PalwBisectMoveBodyV1::Open {
            job_context_hash: h(0x11),
            committed_root: h(0xC0),
            challenger_id: h(0x33),
            responder_id: h(0x44),
            space: PalwBisectSpaceV1::StepLeaves,
            space_size: 4,
        };
        let session = bisect_session_id_v1(&h(0x11), &h(0xC0), &h(0x33), &h(0x44), PalwBisectSpaceV1::StepLeaves, 4);
        let mut played = vec![receipt_row(1, 1_050), bisect_row(open, 1_070)];
        let mut daa = 1_080;
        for round in 0..2u32 {
            let mid = if round == 0 { 2 } else { 1 };
            played.push(bisect_row(
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
            played.push(bisect_row(
                PalwBisectMoveBodyV1::Verdict(PalwBisectVerdictV1 { version: 1, session_id: session, round, agree: false }),
                daa,
            ));
            daa += 5;
        }

        // The point of view MUST sit inside the rung window (moves land at 1_080..1_095 and
        // `W_ROUND` is 30, so the reversed replay's deadline is 1_110). Past it, the deadline rule
        // in `dispute_is_open_v1` reports a stalled reversed ladder as challenger-abandoned and
        // both orders answer `false` — the ordering defect is real but MASKED. A pov of 1_200 is
        // exactly what hid it from the first version of this test.
        let pov = 1_100;
        let forward = resolve_block_facts_v1(&input(&played, &panel, pov, &bonds(), &NoStepWeights), accept_fixture_signature);
        assert_eq!(forward.dispute_open_or_unadjudicable, Some(false), "played to terminal: decided");

        // Every permutation must agree with it — the canonical order is recovered from the records,
        // not inherited from the caller's walk.
        let mut backward = played.clone();
        backward.reverse();
        assert_eq!(
            resolve_block_facts_v1(&input(&backward, &panel, pov, &bonds(), &NoStepWeights), accept_fixture_signature),
            forward,
            "a backward walk must resolve identically"
        );
        let rotated: Vec<_> = played[2..].iter().chain(played[..2].iter()).cloned().collect();
        assert_eq!(
            resolve_block_facts_v1(&input(&rotated, &panel, pov, &bonds(), &NoStepWeights), accept_fixture_signature),
            forward,
            "a rotated walk must resolve identically"
        );
        // And the moves interleaved with the receipt at the end, so the kind tiebreak is exercised.
        let mut shuffled = played[1..].to_vec();
        shuffled.push(played[0].clone());
        assert_eq!(
            resolve_block_facts_v1(&input(&shuffled, &panel, pov, &bonds(), &NoStepWeights), accept_fixture_signature),
            forward,
            "interleaving kinds must resolve identically"
        );
    }
}
