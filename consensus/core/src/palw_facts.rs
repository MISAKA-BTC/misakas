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
use crate::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwFactsError {
    #[error(
        "this node cannot resolve {what} for the block under evaluation — an unresolved fact is an error, never a permissive zero"
    )]
    Unresolved { what: &'static str },
    #[error("the challenge window is zero — every block would finalize at admission")]
    ZeroChallengeWindow,
    /// A class-facts view answered about a class other than the one it was asked about.
    #[error("class facts were asked for {asked} and answered for {answered} — a block must be priced by its OWN class")]
    ClassFactsMismatch { asked: Hash64, answered: Hash64 },
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
    let disputed = resolved.dispute_open_or_unadjudicable.ok_or(PalwFactsError::Unresolved { what: "the dispute state" })?;
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
///
/// **Caller obligation (ADR-0039 1a).** `pwu_per_inference` may only be resolved from a
/// registration whose catalog coverage is complete — one for which
/// `PalwClassRegistrationV1::catalog_coverage_certificate_v1` succeeds.
///
/// **And `class_target` has exactly one legal source: a fold over the BLOCK'S OWN selected-parent
/// chain** ([`crate::palw_class_daa::fold_class_target_v1`]). Never a store row: a store keyed by
/// class id holds whatever the virtual chain last wrote, so a weight derived from it depends on where
/// the reading node's tip points. `DbPalwClassStateStore` no longer carries a target at all, which is
/// what makes that structural rather than a rule to remember — see its module header. This signature takes a
/// bare `Option<u64>` on purpose (threading a registration through a seam whose only live callers
/// pass `None` would add a parameter with no consumer, which is the very defect the coverage rule
/// was fixing), so the obligation is stated rather than typed. Today it holds upstream: a node
/// cannot install `palw_fork_choice` without a `palw_credit` registration that
/// `Params::validate_palw_v1` has confirmed is `ArithmeticCatalogued`. When the W4′ weight wiring
/// lands it must read this number from `params.palw_credit.registration` for that reason — never
/// from a free-floating store field.
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
        assert_eq!(block_pwu_v1(Some(target), None), Err(PalwFactsError::Unresolved { what: "the class's per-inference cost" }));
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
/// The two class facts a block's pwu is made of, answered together and stamped with the class
/// they are FOR.
///
/// Stamped because the resolver cross-checks it: a view that answers for a different class than
/// the one asked about is a bug that would otherwise price one class's block with another class's
/// numbers, silently and in the term that carries 90–99 % of fork-choice weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassFactsV1 {
    /// The class these facts describe. Must equal the block's own `execution_class_id`.
    pub execution_class_id: Hash64,
    /// The class's DAA target **for this block**, not the class's target now.
    pub class_target: u128,
    /// The class's registered normative operation count per canonical inference.
    pub pwu_per_inference: u64,
}

/// Where a block's class facts come from.
///
/// A view rather than two bare `Option`s beside the class id, for the reason
/// [`PalwResolverInputV1::bonds`] is one: facts handed in alongside an identity are not bound to
/// it, so nothing stopped a caller pairing class A's id with class B's target and per-inference
/// cost. Making the id the LOOKUP KEY is what binds them (re-audit §3.2).
///
/// The method takes the block's own `accepted_daa`, not the evaluating node's point of view,
/// because the target that prices a block is the one that stood **when that block was accepted** —
/// a fold over the block's own selected-parent chain
/// ([`crate::palw_class_daa::fold_class_target_v1`]). A signature that only offered "now" could be
/// satisfied by reading the class's current target, which is the defect this replaces: every
/// retarget silently rewrote the weight of history that had already matured.
pub trait PalwClassFactsViewV1 {
    /// The facts for `execution_class_id` as they stood at `block_accepted_daa`, or `None` when
    /// this view does not hold that class — which the resolver turns into `Unresolved`, never a
    /// permissive zero.
    fn class_facts_for_block(&self, execution_class_id: &Hash64, block_accepted_daa: u64) -> Option<PalwClassFactsV1>;
}

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
    /// The root the block ANNOUNCED — `PalwCommitmentCarriageV1::committed_root`. For a composite
    /// class that is the composite execution root; for a bare-v2 class it IS the logits trace root.
    ///
    /// Distinct from [`Self::logits_trace_root`], and the two used to be one field. That conflation
    /// was wrong for every composite class, in one direction or the other and unavoidably: the
    /// conviction and equivocation arms compare it against `full_logits_trace_root` while the
    /// bisection arm compares it against a ladder `Open`'s `committed_root`, and for form 1 those are
    /// different values under different domains (`execution_commitment_root_v1` over four inputs
    /// versus the logits leg alone). So one of the two consumers never matched, and which one
    /// depended on what the caller chose to put in the field: either no conviction could ever count
    /// against a composite block, or no dispute could ever open on one. Nothing failed because both
    /// values coincide for bare-v2 — the case every fixture used.
    ///
    /// The crediting walk has always kept the two apart (`compute_palw_credit_outputs` reads
    /// `binding.full_logits_trace_root` when a binding is present and `committed_root` when it is
    /// not); this input now does the same.
    pub commitment_root: Hash64,
    /// The block's execution's LOGITS leg — the value an attestation signs and a step refutation's
    /// binding carries. Equal to [`Self::commitment_root`] for a bare-v2 class.
    pub logits_trace_root: Hash64,
    pub execution_class_id: Hash64,
    /// DAA at which this block's commitment was accepted, and the evaluating chain's own DAA.
    pub accepted_daa: u64,
    pub pov_daa: u64,
    /// Where this block's class facts are looked up, BY its own `execution_class_id`.
    ///
    /// Replaces the pair of bare `Option`s this input used to carry beside the class id. Those
    /// were unbound to it — the resolver never compared them against the block's own class — so
    /// the dominant term of block weight was whatever the caller passed (re-audit §3.2).
    pub classes: &'a dyn PalwClassFactsViewV1,
    /// The class's registered windows, as one object.
    ///
    /// This used to be the two bare fields `w_challenge` and `w_round`, and carrying the whole
    /// schedule instead is what makes [`panel_duty_v1`] possible: the assigned-duty deadline is
    /// `delta_bind + w_replay`, a THIRD relationship among these windows, and a caller passing it
    /// as its own parameter could pass anything. `PalwScheduleParamsV1::validate` is what keeps
    /// `duty_close < w_challenge` true, and it can only enforce that over windows it holds together.
    pub schedule: crate::palw_schedule::PalwScheduleParamsV1,
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
    let window_close = input.accepted_daa.saturating_add(input.schedule.w_challenge);

    for (kind, accepted_daa, body) in input.carriage {
        match *kind {
            // Bounded by the window for the same reason the conviction arms are, and the
            // asymmetry between them was the defect: a receipt accepted AFTER the window closed
            // used to count toward quorum while a conviction accepted after it did not (W5). Late
            // evidence therefore counted FOR a block and never against it, so a block that missed
            // quorum inside its window could still be topped up to `Final` afterwards — an
            // attacker withholding receipts could raise an old branch's weight at a time of its
            // choosing, which is precisely the retroactive reorg a challenge window exists to
            // bound. One boundary, `<= window_close`, shared with the convictions below.
            PALW_CARRIAGE_KIND_RECEIPT => {
                if *accepted_daa <= window_close
                    && let Ok(PalwCarriageV1::Receipt(r)) = decode_palw_stage1_body(*kind, body)
                    && receipt_is_authentic_v1(&r.receipt, *accepted_daa, input, &verify_signature)
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
            //
            // Both arms judge the accused's bond at the DAA the CONVICTION was accepted, and pass
            // that same DAA on — the adjudicator re-checks activity itself, so resolving the record
            // at the filing moment while handing it `pov_daa` left the pov dependency exactly where
            // it was. It runs the opposite way from the receipt defect and is worse for it: a
            // proven conviction VOIDS this block's weight, so reading at pov means the void LIFTS
            // once the accused's bond ages out of Active. The punished block quietly regains weight
            // as time passes, and the accused steers it — get convicted, unbond, be un-convicted.
            // Judged at the moment the accusation was accepted, a conviction that landed stays
            // landed at every later point of view.
            PALW_CARRIAGE_KIND_STEP_CONVICTION => {
                if *accepted_daa <= window_close
                    && let Ok(PalwCarriageV1::StepConviction(c)) = decode_palw_stage1_body(*kind, body)
                    && c.refutation.binding.full_logits_trace_root == input.logits_trace_root
                    && let Some(accused) = input.bonds.active_bond_at(&c.accused_bond_outpoint, *accepted_daa)
                    && adjudicate_step_conviction_carriage_v1(
                        &c,
                        accused,
                        *accepted_daa,
                        input.network_id,
                        input.step_weights,
                        &verify_signature,
                    )
                    .is_ok()
                {
                    convicted_before_close = true;
                }
            }
            PALW_CARRIAGE_KIND_EQUIVOCATION => {
                if *accepted_daa <= window_close
                    && let Ok(PalwCarriageV1::Equivocation(e)) = decode_palw_stage1_body(*kind, body)
                    && (e.certificate.attestation_a.full_logits_trace_root == input.logits_trace_root
                        || e.certificate.attestation_b.full_logits_trace_root == input.logits_trace_root)
                    && let Some(accused) = input.bonds.active_bond_at(&e.accused_bond_outpoint, *accepted_daa)
                    && adjudicate_equivocation_carriage_v1(&e, accused, *accepted_daa, input.network_id, &verify_signature).is_ok()
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
        // BOTH halves of the field's name. The freeze was derived and then not consumed: a class
        // whose catalog cannot decide a step is one whose blocks nothing can be held to, and
        // without this term such a block matured to `Final` anyway — I10 stopping at the function
        // that computes it. `ramp_stage_v1` reads this before it looks at the window, which is what
        // makes a coverage gap keep a block out of `safe(C)` rather than merely annotate it.
        //
        // Both terms are bounded by this block's own challenge window, so adding the second cannot
        // demote a matured block: `Final` requires `pov` past the window close, and a freeze record
        // must be accepted at or before it, so every record that can freeze is already visible at
        // the first point of view where `Final` is reachable at all.
        dispute_open_or_unadjudicable: Some(dispute_is_open_v1(input) || class_frozen_before_close_v1(input, &verify_signature)),
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
/// 3. **bond** — `verifier_bond_outpoint` resolves to a bond that was ACTIVE **when the receipt was
///    accepted**. An unbonded or already-slashed filer has nothing at stake and is not accountable;
///    the moment that is judged at is the moment it acted, NOT the evaluating node's tip.
///
///    This read `input.pov_daa`, and `effective_bond_status` is one-way: once `pov` passes a
///    filer's unbond request it is `Unbonding` forever. So a receipt STOPPED being authentic as its
///    filer left, a matured block dropped back below quorum, and its pwu left `safe(C)` — finality
///    eroding by nothing more than time passing, and steerable: file, let the block mature, then
///    unbond to demote it. The credit path already states the principle this restores — "a
///    refutation accepted after the window still convicts, but does not revoke credit" — later
///    facts punish, they do not revoke;
/// 4. **signature** — ML-DSA-87 over the receipt's own digest, under the resolved bond's key.
///
/// Before this existed the resolver pushed every decodable receipt, so quorum was a count of
/// whoever chose to speak — fabricate `k` receipts naming `k` drawn seats and a fabricated block
/// matured to full weight without any of those verifiers doing anything (re-audit §3.1).
fn receipt_is_authentic_v1<F>(
    receipt: &PalwVerificationReceiptV1,
    filed_daa: u64,
    input: &PalwResolverInputV1<'_>,
    verify_signature: &F,
) -> bool
where
    F: Fn(&[u8], &kaspa_hashes::Hash, &[u8]) -> bool,
{
    if receipt.validate_shape().is_err() {
        return false;
    }
    if receipt.execution_class_id != input.execution_class_id {
        return false;
    }
    let Some(bond) = input.bonds.active_bond_at(&receipt.verifier_bond_outpoint, filed_daa) else {
        return false;
    };
    verify_signature(&bond.validator_pubkey, &receipt.message(input.network_id), &receipt.signature)
}

/// What a drawn seat owed this block, once its window is over.
///
/// ADR-0038 Decision C makes the assigned verifier's duty objective: **attest or no-show**, and a
/// no-show is an offence rather than an abstention. This is the accounting side of that — the fact
/// a consumer needs before any consequence can attach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwPanelDutyV1 {
    /// The window has not closed at this point of view. Who failed to file is NOT yet a fact, and
    /// saying "nobody has filed so everybody defaulted" mid-window is how a slow network becomes a
    /// mass slash. A caller must treat this as "ask again later", never as an empty no-show set.
    Pending,
    /// The window closed. These drawn seats filed nothing within it, in canonical outpoint order.
    Closed { no_shows: Vec<TransactionOutpoint> },
}

/// Which drawn seats defaulted on this block's commitment.
///
/// Derived by a fold over the carriage on the chain being evaluated — no store, for the reason
/// [`class_frozen_before_close_v1`] states.
///
/// Three rules, each of which decides a real case the other way round from the receipt count:
///
/// * **any verdict discharges the duty.** [`assigned_receipt_count_v1`] counts `Match` only,
///   because only agreement licenses. A verifier who replayed and filed `Mismatch` did the work and
///   disagreed — the strongest possible discharge, and the one the network most needs filed. Reusing
///   the quorum filter here would punish exactly the honest dissent Decision C is built to collect.
/// * **the deadline is the DUTY window, not the challenge window.**
///   [`crate::palw_schedule::job_schedule_v1`]'s `replay_deadline_daa` — `commit + delta_bind +
///   w_replay`, the schedule's own "attest or refute within this many DAA of the anchor", which it
///   validates to be strictly shorter than `w_challenge`, and the same deadline the credit path
///   bounds attestations by. The quorum count beside this uses the longer window on
///   purpose: a late receipt is still evidence someone replayed and agreed, so discarding it would
///   cost liveness, while an ASSIGNED seat that files after its deadline has already had the chance
///   to see what the others filed. Same carriage, two deadlines, each the one its own consumer's
///   rule is written against.
/// * **a bond that stopped being active is NOT excused.** It is the losing side of a real trade-off:
///   a slashed seat collects a no-show on top of its slash. The alternative is worse — excusing an
///   inactive bond makes withdrawal an exit from assigned duty, so any seat that dislikes what it is
///   about to find can unbond instead of filing, and the panel silently loses whichever members saw
///   something. Double-counting a punishment is a fairness cost; a purchasable exemption is a
///   soundness one.
///
/// The consequence is deliberately NOT here. This answers "who defaulted"; what that costs is a
/// slash-path decision, and the two are separated so the accounting can be tested against cases no
/// live slash path exists to exercise yet.
pub fn panel_duty_v1<F>(input: &PalwResolverInputV1<'_>, verify_signature: F) -> PalwPanelDutyV1
where
    F: Fn(&[u8], &kaspa_hashes::Hash, &[u8]) -> bool,
{
    use crate::palw_carriage::{PALW_CARRIAGE_KIND_RECEIPT, PalwCarriageV1, decode_palw_stage1_body};

    // The deadline comes from `job_schedule_v1`, which is where `commit + delta_bind + w_replay`
    // is already defined and is what the CREDIT path bounds attestations by
    // (`a.accepted_daa <= schedule.replay_deadline_daa`). Recomputing the sum here would be a
    // second derivation of one deadline, and the two paths would then be free to drift — the
    // through-line defect this tree has closed twice already. An overflowing schedule is
    // `Pending` rather than an answer.
    let Ok(schedule) = crate::palw_schedule::job_schedule_v1(&input.schedule, input.accepted_daa) else {
        return PalwPanelDutyV1::Pending;
    };
    let window_close = schedule.replay_deadline_daa;

    // A zero duty window accuses the whole panel one DAA after acceptance, so it is refused the
    // same way `weight_facts_v1` refuses a zero challenge window. `PalwScheduleParamsV1::validate`
    // already rejects zeroes; this input takes the params unvalidated, and the safe answer to a
    // misconfiguration is "not a fact yet" rather than a mass accusation.
    if window_close <= input.accepted_daa || input.pov_daa <= window_close {
        return PalwPanelDutyV1::Pending;
    }
    let mut filed: BTreeSet<(Hash64, u32)> = BTreeSet::new();
    for (kind, accepted_daa, body) in input.carriage {
        if *kind == PALW_CARRIAGE_KIND_RECEIPT
            && *accepted_daa <= window_close
            && let Ok(PalwCarriageV1::Receipt(r)) = decode_palw_stage1_body(*kind, body)
            && r.receipt.target_block_hash == input.block_hash
            && r.receipt.target_commitment_root == input.commitment_root
            && receipt_is_authentic_v1(&r.receipt, *accepted_daa, input, &verify_signature)
        {
            filed.insert((r.receipt.verifier_bond_outpoint.transaction_id, r.receipt.verifier_bond_outpoint.index));
        }
    }

    let mut no_shows: Vec<TransactionOutpoint> = input
        .panel
        .iter()
        .filter(|s| !filed.contains(&(s.bond_outpoint.transaction_id, s.bond_outpoint.index)))
        .map(|s| s.bond_outpoint)
        .collect();
    no_shows.sort_by_key(|o| (o.transaction_id, o.index));
    no_shows.dedup();
    PalwPanelDutyV1::Closed { no_shows }
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

/// Which adjudication outcome freezes the class — ADR-0038 I10, as a decision rather than a walk.
///
/// Exactly one does. A conviction that LANDS slashes the executor and leaves the class running; a
/// conviction that fails for any other reason is a fact about the challenger's evidence, and
/// freezing on it would be a denial of service against an honest class. Only "this build's catalog
/// cannot decide the step" is a fact about the class's own coverage.
///
/// Split out of the walk so the rule can be tested exhaustively over outcomes, which building a
/// complete refutation for each case cannot do cheaply.
pub fn outcome_freezes_class_v1(outcome: &Result<TransactionOutpoint, crate::palw_carriage::PalwCarriageError>) -> bool {
    matches!(outcome, Err(crate::palw_carriage::PalwCarriageError::StepUnadjudicable))
}

/// Which carriage rows a freeze may be derived FROM, split out of the walk for the same reason
/// [`outcome_freezes_class_v1`] is: no fixture in this crate reaches `Unadjudicable` end to end, so
/// a rule left inline here would be a rule nothing tests.
///
/// Two bounds, and the second is the one that matters.
///
/// * `<= pov_daa` — nothing the evaluating point has not reached is a fact about its chain;
/// * `<= accepted_daa + w_challenge` — **the same window bound every other carriage arm carries**.
///
/// Without the window bound this is a retroactive demotion weapon, and a broader one than the
/// late-`Open` that was fixed as exactly that: a freeze is a fact about the CLASS, so one coverage
/// gap surfacing at any later DAA would pull every matured block of that class back to
/// `Provisional` at once. ADR-0039 §3e is explicit that a conviction "can never rewrite safe
/// weight — it can only prevent work from entering it", and an unbounded freeze rewrites it
/// wholesale. `carriage_accepted_after_the_window_changes_nothing` is the rule; this keeps
/// the freeze inside it before anything wires the freeze to the ramp.
///
/// What the bound leans on is the assumption the whole conviction path already leans on, and
/// ADR-0039 states as residual assumption 1: an honest party reachable within every challenge
/// window. A gap that surfaces inside the window pins those blocks at `Provisional` and they never
/// mature; one that surfaces afterwards stops the class going FORWARD — which is the store-backed
/// [`crate::palw_class_state::PalwClassStateView::is_frozen`]'s job on the panel and mint paths,
/// fail-closed, and not this function's.
fn freeze_record_is_in_scope_v1(record_accepted_daa: u64, input: &PalwResolverInputV1<'_>) -> bool {
    record_accepted_daa <= input.pov_daa && record_accepted_daa <= input.accepted_daa.saturating_add(input.schedule.w_challenge)
}

/// ADR-0038 I10: whether a coverage gap in this class surfaced on the chain being evaluated
/// **before this block's challenge window closed**.
///
/// The bound is in the name because it has to be read before the function is wired: this is not
/// "is the class frozen right now", which is a question about the present and is the store-backed
/// view's ([`freeze_record_is_in_scope_v1`] derives the difference). This one is per-block, and a
/// gap that surfaces after a block matured leaves that block alone.
///
/// A class freezes when a step conviction against it adjudicates `Unadjudicable` — this build's
/// catalog cannot decide the refuted step. That is a fact about the class's coverage rather than
/// about the accused, so it slashes nobody (I10) and stops the class instead: a class whose
/// disputes cannot be decided is one whose blocks nothing can be held to.
///
/// **Derived from the chain, never from a store**, and that is the safe choice rather than the
/// convenient one. The class-state store's own module records why a row is dangerous: it answers
/// about the reading node's virtual tip, so two nodes with different sink histories would disagree
/// about a coinbase — a partition, not a slow node. Its note that "a seed writer must arrive
/// together with per-chain-point scoping, never before it" is satisfied here by having no row at
/// all: the moves ARE carriage on the chain being evaluated, so replaying them is chain-scoped by
/// construction and two nodes with the same DAG cannot disagree.
///
/// Only records [`freeze_record_is_in_scope_v1`] admits count, in canonical order, and a record that fails to
/// decode or to adjudicate is skipped — an unparseable accusation cannot freeze a class, which is
/// the safe direction: freezing on junk would be a denial-of-service against an honest class.
/// **Two functions answer "is this class frozen", and they are not rivals.** `PalwClassStateView::is_frozen`
/// reads the class-state store and governs the PANEL and MINT paths, where it is fail-closed: a
/// class it cannot answer for reads as frozen, so a node that cannot establish a class is running
/// draws no panel and mints nothing. This one derives the freeze from the chain and governs WEIGHT,
/// where the store would be wrong for the reason that module states about itself — a row answers
/// about the reading node's virtual tip, and a weight that depends on where a tip happens to point
/// is not a fact about the chain being weighed.
///
/// The split is deliberate, and the reason it is safe is that the two fail in the same direction:
/// the store refuses to mint when it cannot answer, and this returns `false` when it cannot
/// establish a freeze — which withholds nothing and lets the ordinary refutation paths run. Neither
/// should be swapped for the other without re-deriving that.
///
pub fn class_frozen_before_close_v1<F>(input: &PalwResolverInputV1<'_>, verify_signature: F) -> bool
where
    F: Fn(&[u8], &kaspa_hashes::Hash, &[u8]) -> bool,
{
    use crate::palw_carriage::{
        PALW_CARRIAGE_KIND_STEP_CONVICTION, PalwCarriageV1, adjudicate_step_conviction_carriage_v1, decode_palw_stage1_body,
    };
    canonical_carriage_order_v1(input.carriage).into_iter().any(|(kind, accepted_daa, body)| {
        *kind == PALW_CARRIAGE_KIND_STEP_CONVICTION && freeze_record_is_in_scope_v1(*accepted_daa, input) && {
            let Ok(PalwCarriageV1::StepConviction(c)) = decode_palw_stage1_body(*kind, body) else { return false };
            // At the DAA the conviction was ACCEPTED, like every other accountability question on
            // this walk. Read at `pov_daa` a freeze silently LIFTS once the accused's bond ages
            // out — the emergency stop undoing itself while the coverage gap it fired on is still
            // there, and undoing it later on some nodes than others.
            let Some(accused) = input.bonds.active_bond_at(&c.accused_bond_outpoint, *accepted_daa) else { return false };
            // The ONLY outcome that freezes. A conviction that lands, and one that fails for
            // any other reason, both leave the class running.
            outcome_freezes_class_v1(&adjudicate_step_conviction_carriage_v1(
                &c,
                accused,
                *accepted_daa,
                input.network_id,
                input.step_weights,
                &verify_signature,
            ))
        }
    })
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
        // The Open must land inside the refutation window, for the same reason the conviction and
        // receipt arms must. Unbounded, this was a RETROACTIVE maturity veto: `ramp_stage_v1`
        // returns `Provisional` on an open dispute before it ever looks at the window, so one
        // bonded Open filed against an already-`Final` block demoted it — and `safe(C)`, which
        // governs IBD and the deep-reorg bound, LOST weight it had already accumulated. That is
        // ADR-0038's own "mutable-weight forkchoice" critical, reachable at the price of a single
        // carriage record.
        //
        // Only the Open is bounded. The session's later moves are meant to run past the window —
        // that is what `prosecution_slack` exists for — so the replay below is left alone.
        if *open_daa > input.accepted_daa.saturating_add(input.schedule.w_challenge) {
            continue;
        }
        // An Open from a bond that is not ACTIVE here opens nothing. Without this, one unbonded
        // record pinned any block at `Provisional` for as long as it stayed in the horizon — a
        // griefing veto over maturity at zero cost and with nobody to charge (re-audit §3.1). The
        // bond is what makes a baseless dispute chargeable; `PalwBisectMoveCarriageV1` carries the
        // outpoint precisely so this check has something to resolve.
        if input.bonds.active_bond_at(challenger_bond, *open_daa).is_none() {
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
            open_daa.saturating_add(input.schedule.w_round.max(1)),
        ) else {
            continue;
        };
        // Replay this session's later moves in the canonical order established above.
        for (daa, _, later) in &moves {
            match later {
                PalwBisectMoveBodyV1::Disclosure(d) if d.session_id == ladder.session_id() => {
                    let _ = ladder.apply_disclosure(d, *daa, input.schedule.w_round);
                }
                PalwBisectMoveBodyV1::Verdict(v) if v.session_id == ladder.session_id() => {
                    let _ = ladder.apply_verdict(v, *daa, input.schedule.w_round);
                }
                _ => {}
            }
        }
        match ladder.turn() {
            // Only an ABANDONED ladder is decided. It is also unreachable during replay: the loop
            // above applies Disclosure and Verdict moves only, so the sole route to `Abandoned` is
            // this function's own `declare_no_show` below.
            PalwBisectTurnV1::Abandoned => {}
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
            // `Terminal` belongs HERE, not with the decided case. It means the bisection narrowed to
            // one index and the RESPONDER owes the terminal opening — the ladder says exactly that
            // and charges the Responder for silence at `Terminal` (`palw_bisect`'s `declare_no_show`).
            // Treating it as decided matured a block whose disputed step had never been adjudicated
            // by anyone: the ladder had finished LOCATING the step and nobody had yet CHECKED it.
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::AwaitDisclosure | PalwBisectTurnV1::AwaitVerdict => {
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
    let facts = weight_facts_v1(&resolved, input.schedule.w_challenge)?;
    // Looked up by the block's own class id and its own acceptance point — not read off the
    // input beside them. The stamp is then checked: a view that answers for another class would
    // otherwise price this block with that class's numbers.
    let class_facts = input
        .classes
        .class_facts_for_block(&input.execution_class_id, input.accepted_daa)
        .ok_or(PalwFactsError::Unresolved { what: "the block's execution class" })?;
    if class_facts.execution_class_id != input.execution_class_id {
        return Err(PalwFactsError::ClassFactsMismatch { asked: input.execution_class_id, answered: class_facts.execution_class_id });
    }
    let pwu = block_pwu_v1(Some(class_facts.class_target), Some(class_facts.pwu_per_inference))?;
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
    /// Duty closes at `delta_bind + w_replay` = 120, well inside the 500-DAA challenge window —
    /// the inequality `PalwScheduleParamsV1::validate` enforces, and the gap the duty tests exploit.
    const DUTY_WINDOW: u64 = 120;
    const SCHEDULE: crate::palw_schedule::PalwScheduleParamsV1 = crate::palw_schedule::PalwScheduleParamsV1 {
        version: crate::palw_schedule::PALW_SCHEDULE_PARAMS_VERSION_V1,
        delta_bind: 20,
        w_replay: 100,
        w_answer: 60,
        w_round: W_ROUND,
        w_challenge: W_CHALLENGE,
        prosecution_slack: 100,
        q: 2,
    };
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
        receipt_row_verdict(bond, daa, crate::palw_receipt::PalwReceiptVerdictV1::Match)
    }

    /// The same row with the verdict chosen. `Mismatch` is a discharged duty and never a quorum
    /// vote, so the two consumers must be able to disagree about one row.
    fn receipt_row_verdict(bond: u8, daa: u64, verdict: crate::palw_receipt::PalwReceiptVerdictV1) -> (u8, u64, Vec<u8>) {
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
                verdict,
                verifier_bond_outpoint: op(bond),
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            },
        };
        (PALW_CARRIAGE_KIND_RECEIPT, daa, body(&PalwCarriageV1::Receipt(r)))
    }

    /// A bonded `Open` against the fixture's commitment, accepted at `daa`.
    fn open_row(daa: u64) -> (u8, u64, Vec<u8>) {
        bisect_row(
            PalwBisectMoveBodyV1::Open {
                job_context_hash: h(0x11),
                committed_root: h(0xC0),
                challenger_id: h(0x33),
                responder_id: h(0x44),
                space: PalwBisectSpaceV1::StepLeaves,
                space_size: 4,
            },
            daa,
        )
    }

    fn bisect_row(b: PalwBisectMoveBodyV1, daa: u64) -> (u8, u64, Vec<u8>) {
        let m = PalwBisectMoveCarriageV1 { version: PALW_CARRIAGE_VERSION_V1, challenger_bond_outpoint: op(0xC9), body: b };
        (PALW_CARRIAGE_KIND_BISECT_MOVE, daa, body(&PalwCarriageV1::BisectMove(m)))
    }

    /// A step-conviction carriage against the fixture's accused bond, with the skeleton refutation
    /// the step-refute tests use. Against `NoStepWeights` it adjudicates `Unadjudicable`, which is
    /// the case I10 is about.
    fn conviction_row(daa: u64) -> (u8, u64, Vec<u8>) {
        let refutation = crate::palw_step_refute::tests::skeleton_refutation();
        let logits_root = refutation.binding.full_logits_trace_root;
        let composite_root = refutation.binding.committed_execution_root;
        let c = crate::palw_carriage::PalwStepConvictionCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: op(0xB1),
            attestation: crate::palw_slash::PalwExecutionAttestationV1 {
                version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
                executor_id: h(0xE1),
                job_context_hash: refutation.binding.job_context.context_hash(),
                full_logits_trace_root: logits_root,
                committed_root: composite_root,
                bond_outpoint: op(0xB1),
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            },
            refutation,
        };
        (crate::palw_carriage::PALW_CARRIAGE_KIND_STEP_CONVICTION, daa, body(&PalwCarriageV1::StepConviction(c)))
    }

    /// A step conviction whose refutation passes every structural check and then meets an
    /// uncatalogued kernel — the only carriage that reaches `Unadjudicable`, and therefore the only
    /// way to test what a class coverage gap does to a block.
    ///
    /// Recorded twice as an untested path before the fixture existed. `conviction_row` beside it
    /// cannot get this far: its skeleton refutation fails on its opening path, so it exercises the
    /// walk's filters and never the outcome.
    fn unadjudicable_conviction_row(daa: u64) -> (u8, u64, Vec<u8>) {
        let refutation = crate::palw_step_refute::tests::unadjudicable_refutation();
        let c = crate::palw_carriage::PalwStepConvictionCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: op(0xB1),
            attestation: crate::palw_slash::PalwExecutionAttestationV1 {
                version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
                executor_id: h(0xE1),
                job_context_hash: refutation.binding.job_context.context_hash(),
                full_logits_trace_root: refutation.binding.full_logits_trace_root,
                committed_root: refutation.binding.committed_execution_root,
                bond_outpoint: op(0xB1),
                signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
            },
            refutation,
        };
        (crate::palw_carriage::PALW_CARRIAGE_KIND_STEP_CONVICTION, daa, body(&PalwCarriageV1::StepConviction(c)))
    }

    /// An equivocation certificate against `op(0xB1)` that ADJUDICATES — the fixtures' only route
    /// to a landed conviction, and therefore the only way to test what a landed conviction does
    /// (a step conviction cannot get past its structural checks here, see `conviction_row`).
    fn equivocation_row(daa: u64) -> (u8, u64, Vec<u8>) {
        use crate::palw_carriage::{PALW_CARRIAGE_KIND_EQUIVOCATION, PalwEquivocationCarriageV1};
        let ctx = crate::palw_step_refute::tests::skeleton_refutation().binding.job_context;
        let att = |root: Hash64| crate::palw_slash::PalwExecutionAttestationV1 {
            version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
            executor_id: h(0xE1),
            job_context_hash: ctx.context_hash(),
            full_logits_trace_root: root,
            committed_root: root,
            bond_outpoint: op(0xB1),
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        };
        let e = PalwEquivocationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: op(0xB1),
            certificate: crate::palw_slash::PalwClassContradictionCertificateV1 {
                version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
                // The LOGITS leg of this block's execution, which is what an attestation signs — not the
                // announced composite root. The two are distinct in the fixture on purpose.
                attestation_a: att(h(0x1A)), // this block's commitment root
                attestation_b: att(h(0x02)),
                job_context: ctx,
            },
        };
        (PALW_CARRIAGE_KIND_EQUIVOCATION, daa, body(&PalwCarriageV1::Equivocation(e)))
    }

    /// The chain identity the resolver runs under. It must equal the network the equivocation
    /// fixtures' job context names, because a foreign-network certificate is now refused before any
    /// signature is checked — which is the point of that rule.
    const NETWORK: &[u8] = b"step-refute-test";

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
            // DELIBERATELY DIFFERENT values. Every fixture used to give the announced root and the
            // logits leg the same value — the bare-v2 coincidence — which is exactly why the
            // conflated field looked correct for both consumers. A composite class has two roots and
            // the tests must too.
            commitment_root: h(0xC0),
            logits_trace_root: h(0x1A),
            execution_class_id: h(0xC1),
            accepted_daa: 1_000,
            pov_daa: pov,
            classes: &FIXTURE_CLASSES,
            schedule: SCHEDULE,
        }
    }

    /// A view that holds exactly the fixture class, at the fixture's numbers.
    struct FixtureClasses;
    impl PalwClassFactsViewV1 for FixtureClasses {
        fn class_facts_for_block(&self, execution_class_id: &Hash64, _block_accepted_daa: u64) -> Option<PalwClassFactsV1> {
            (*execution_class_id == h(0xC1)).then(|| PalwClassFactsV1 {
                execution_class_id: h(0xC1),
                class_target: u128::MAX >> 10, // 1_024 expected attempts
                pwu_per_inference: 100,
            })
        }
    }
    static FIXTURE_CLASSES: FixtureClasses = FixtureClasses;

    /// ADR-0039's reorg-equivalence property, in the one direction that can be tested here:
    /// **with the carriage fixed, advancing the point of view never un-matures a block.**
    ///
    /// This is not a style rule. `safe(C)` governs IBD and the deep-reorg bound, so pwu that leaves
    /// it is finality being handed back. Authenticity used to be judged at `pov_daa`, and
    /// `effective_bond_status` is one-way — once `pov` passes a filer's unbond request the bond is
    /// `Unbonding` forever. A validator could therefore file its receipt, let the block mature, and
    /// then unbond to drop the block back below quorum. Nothing about the chain changed; only the
    /// clock did.
    ///
    /// Judged at the moment of filing, the same walk is stable at every later point of view.
    #[test]
    fn a_matured_block_does_not_un_mature_when_its_verifiers_leave() {
        let panel = [seat(1), seat(2)];
        let weights = NoStepWeights;
        let carriage = vec![receipt_row(1, 1_100), receipt_row(2, 1_110)];

        // Seat 1 requests unbonding at 1 800 — after it filed, after the window closed.
        let leaving = crate::dns_finality::ActiveBondView::from_records([1u8, 2, 3, 0xC9].into_iter().map(|s| {
            let mut b = bond(s);
            if s == 1 {
                b.unbond_request_daa_score = Some(1_800);
            }
            (op(s), b)
        }));

        for pov in [1_600, 1_799, 1_800, 5_000] {
            let resolved = resolve_block_facts_v1(&input(&carriage, &panel, pov, &leaving, &weights), accept_fixture_signature);
            let facts = weight_facts_v1(&resolved, W_CHALLENGE).expect("resolved");
            assert_eq!(resolved.assigned_receipts, Some(2), "pov {pov}: a receipt filed while bonded stays filed");
            assert_eq!(
                crate::palw_weight::ramp_stage_v1(&facts, &RAMP),
                PalwWorkRampStageV1::Final,
                "pov {pov}: matured weight must not be handed back"
            );
        }

        // The rule it must NOT become: a filer that was already gone when it filed still counts for
        // nothing. Seat 1 unbonds at 1 050, before its own receipt at 1 100.
        let gone_first = crate::dns_finality::ActiveBondView::from_records([1u8, 2, 3, 0xC9].into_iter().map(|s| {
            let mut b = bond(s);
            if s == 1 {
                b.unbond_request_daa_score = Some(1_050);
            }
            (op(s), b)
        }));
        let resolved = resolve_block_facts_v1(&input(&carriage, &panel, 2_000, &gone_first, &weights), accept_fixture_signature);
        assert_eq!(resolved.assigned_receipts, Some(1), "an unbonded filer is still not accountable");
    }

    /// A matured block cannot be demoted by a dispute opened after its window closed.
    ///
    /// `ramp_stage_v1` returns `Provisional` on an open dispute BEFORE it looks at the window —
    /// correct for a dispute opened inside the window and still running, and catastrophic for one
    /// opened afterwards. Unbounded, a single bonded `Open` against an already-`Final` block took
    /// its pwu back out of `safe(C)`, the weight that governs IBD and the deep-reorg bound. An
    /// accumulated finality weight that can go DOWN is ADR-0038's "mutable-weight forkchoice"
    /// critical, and it cost one carriage record.
    #[test]
    fn a_dispute_opened_after_the_window_cannot_demote_a_matured_block() {
        let bonds = bonds();
        let panel = [seat(1), seat(2)];
        let weights = NoStepWeights;
        let quorum = vec![receipt_row(1, 1_100), receipt_row(2, 1_110)];

        // Matured: quorum inside the window, window closed, nothing against it.
        let matured = resolve_block_facts_v1(&input(&quorum, &panel, 2_000, &bonds, &weights), accept_fixture_signature);
        let facts = weight_facts_v1(&matured, W_CHALLENGE).expect("resolved");
        assert_eq!(crate::palw_weight::ramp_stage_v1(&facts, &RAMP), PalwWorkRampStageV1::Final);

        // The same chain plus one bonded Open, accepted a full window after the close.
        let mut late = quorum.clone();
        late.push(open_row(2_400));
        let after = resolve_block_facts_v1(&input(&late, &panel, 2_500, &bonds, &weights), accept_fixture_signature);
        assert_eq!(after.dispute_open_or_unadjudicable, Some(false), "a late Open is telemetry, not a dispute");
        let facts_after = weight_facts_v1(&after, W_CHALLENGE).expect("resolved");
        assert_eq!(
            crate::palw_weight::ramp_stage_v1(&facts_after, &RAMP),
            PalwWorkRampStageV1::Final,
            "safe weight already accumulated must not be revocable"
        );

        // And the window still works: the SAME Open accepted at the last DAA inside it does open.
        let mut in_time = quorum.clone();
        in_time.push(open_row(1_500));
        let inside = resolve_block_facts_v1(&input(&in_time, &panel, 2_500, &bonds, &weights), accept_fixture_signature);
        assert_eq!(inside.dispute_open_or_unadjudicable, Some(true), "an Open inside the window still disputes");
    }

    /// The window bounds the receipts too, and this is the case that used to go the other way.
    ///
    /// Two receipts land inside the window and a third lands after it. Quorum is 2, so the block is
    /// licensed either way — the point under test is the COUNT, because the count is what a fourth
    /// seat's late arrival used to raise. A block that missed quorum in its window could be topped
    /// up afterwards, at a moment the topper-up chose.
    #[test]
    fn a_receipt_accepted_after_the_window_closed_is_not_counted() {
        let bonds = bonds();
        let panel = [seat(1), seat(2), seat(3)];
        let weights = NoStepWeights;

        let in_window = vec![receipt_row(1, 1_100), receipt_row(2, 1_200)];
        let plus_late = vec![receipt_row(1, 1_100), receipt_row(2, 1_200), receipt_row(3, 1_501)];

        let a = resolve_block_facts_v1(&input(&in_window, &panel, 2_000, &bonds, &weights), accept_fixture_signature);
        let b = resolve_block_facts_v1(&input(&plus_late, &panel, 2_000, &bonds, &weights), accept_fixture_signature);
        assert_eq!(a.assigned_receipts, Some(2));
        assert_eq!(b.assigned_receipts, Some(2), "a receipt accepted at window_close+1 must not raise the count");

        // And the boundary itself is the convictions' boundary: `window_close` exactly still counts.
        let at_close = vec![receipt_row(1, 1_100), receipt_row(2, 1_200), receipt_row(3, 1_500)];
        let c = resolve_block_facts_v1(&input(&at_close, &panel, 2_000, &bonds, &weights), accept_fixture_signature);
        assert_eq!(c.assigned_receipts, Some(3), "the last DAA INSIDE the window must still count");
    }

    /// ADR-0038 Decision C: mid-window, "who defaulted" is not a fact yet.
    ///
    /// The dangerous answer is not a wrong name — it is an empty-looking `Closed` set that a caller
    /// reads as "nobody defaulted", or a full one it reads as "everybody did". A network partition
    /// that delays every receipt would, under the second reading, slash a whole panel for being
    /// slow. `Pending` is a different value, so neither reading is available.
    #[test]
    fn duty_is_pending_until_the_duty_window_closes() {
        let bonds = bonds();
        let panel = [seat(1), seat(2), seat(3)];
        let weights = NoStepWeights;
        let carriage = vec![receipt_row(1, 1_100)];

        for pov in [1_000, 1_100, 1_000 + DUTY_WINDOW] {
            assert_eq!(
                panel_duty_v1(&input(&carriage, &panel, pov, &bonds, &weights), accept_fixture_signature),
                PalwPanelDutyV1::Pending,
                "pov {pov} is inside the duty window"
            );
        }
        assert!(matches!(
            panel_duty_v1(&input(&carriage, &panel, 1_000 + DUTY_WINDOW + 1, &bonds, &weights), accept_fixture_signature),
            PalwPanelDutyV1::Closed { .. }
        ));
    }

    /// A misconfigured zero duty window accuses nobody.
    ///
    /// `PalwScheduleParamsV1::validate` rejects zero windows, but this input takes the params
    /// unvalidated — and the failure mode of getting it wrong is not a missing fact, it is every
    /// seat in default one DAA after acceptance.
    #[test]
    fn a_zero_duty_window_is_pending_rather_than_a_mass_accusation() {
        let bonds = bonds();
        let panel = [seat(1), seat(2)];
        let weights = NoStepWeights;
        let carriage = vec![];
        let mut inp = input(&carriage, &panel, 9_000, &bonds, &weights);
        inp.schedule = crate::palw_schedule::PalwScheduleParamsV1 { delta_bind: 0, w_replay: 0, ..SCHEDULE };
        assert_eq!(panel_duty_v1(&inp, accept_fixture_signature), PalwPanelDutyV1::Pending);
    }

    /// The two deadlines diverge on one row, and the same carriage answers both consumers.
    ///
    /// Seat 1 filed `Match` inside the duty window — discharged, and counts toward quorum.
    /// Seat 2 filed `Mismatch` inside it — also discharged, and this is what separates duty from
    /// quorum: counting a `Mismatch` as a no-show would punish the honest dissent Decision C exists
    /// to collect, and counting it toward quorum would let a disagreement license work.
    /// Seat 3 filed at 1 200 — past its duty deadline (1 120) but inside the challenge window
    /// (1 500). It defaulted AND its receipt counts toward quorum. That pair is the whole point of
    /// two deadlines: one row, two rules, neither borrowed from the other.
    #[test]
    fn a_mismatch_discharges_the_duty_and_the_two_deadlines_differ() {
        let bonds = bonds();
        let panel = [seat(1), seat(2), seat(3)];
        let weights = NoStepWeights;
        let carriage = vec![
            receipt_row(1, 1_100),
            receipt_row_verdict(2, 1_110, crate::palw_receipt::PalwReceiptVerdictV1::Mismatch),
            receipt_row(3, 1_200),
        ];
        let inp = input(&carriage, &panel, 2_000, &bonds, &weights);

        assert_eq!(panel_duty_v1(&inp, accept_fixture_signature), PalwPanelDutyV1::Closed { no_shows: vec![op(3)] });
        assert_eq!(
            resolve_block_facts_v1(&inp, accept_fixture_signature).assigned_receipts,
            Some(2),
            "seat 3's late receipt still licenses; seat 2's Mismatch never does"
        );
    }

    /// An unverifiable signature is not a filing.
    ///
    /// The node that cannot check signatures reports the whole panel in default — which is correct
    /// as an ACCOUNTING answer and catastrophic as a slash. It is the reason the consequence is not
    /// in this function: a caller must satisfy itself it can verify before acting on the set.
    #[test]
    fn a_receipt_this_node_cannot_verify_discharges_nothing() {
        let bonds = bonds();
        let panel = [seat(1), seat(2)];
        let weights = NoStepWeights;
        let carriage = vec![receipt_row(1, 1_100), receipt_row(2, 1_110)];
        let inp = input(&carriage, &panel, 2_000, &bonds, &weights);

        assert_eq!(panel_duty_v1(&inp, accept_fixture_signature), PalwPanelDutyV1::Closed { no_shows: vec![] });
        assert_eq!(
            panel_duty_v1(&inp, reject_every_signature),
            PalwPanelDutyV1::Closed { no_shows: vec![op(1), op(2)] },
            "unverifiable receipts leave every seat looking defaulted — the caller's problem, stated"
        );
    }

    /// ADR-0038 I10, exhaustively: exactly one adjudication outcome freezes the class.
    ///
    /// A conviction that LANDS slashes the executor and leaves the class running. A conviction that
    /// fails for any other reason is a fact about the challenger's evidence, and freezing on it
    /// would be a denial of service against an honest class. Only "this build's catalog cannot
    /// decide the step" is a fact about the class's own coverage.
    #[test]
    fn only_the_unadjudicable_outcome_freezes_the_class() {
        use crate::palw_carriage::PalwCarriageError as E;
        assert!(outcome_freezes_class_v1(&Err(E::StepUnadjudicable)));

        assert!(!outcome_freezes_class_v1(&Ok(op(0xB1))), "a landed conviction slashes; it does not freeze");
        for other in [
            E::StepConvictionNotProven("opening path ended short of the root".into()),
            E::EquivocationNotProven("attestation signature does not verify".into()),
            E::EquivocationBondInactive,
            E::EquivocationBondNotTheSigner,
            E::BindingRootMismatch,
            E::CommittedRootMismatch,
            E::TruncatedEnvelope,
            E::UnknownKind(0xFF),
        ] {
            assert!(!outcome_freezes_class_v1(&Err(other)), "only a coverage gap freezes");
        }
    }

    /// **One rule, three types, and nothing tied the third to the other two.**
    ///
    /// "Only `Unadjudicable` freezes the class" is stated on `PalwJobStatusV3` (the ADR-0037 spine),
    /// mirrored onto `PalwDisputeSummary::freeze_class` — which `palw_dispute` already pins against
    /// the spine — and stated a third time here on the carriage adjudicator's `Result`.
    /// `outcome_freezes_class_v1` was not tied to either.
    ///
    /// Left untied, adding a second freezing terminal to one side is silent: `demands_class_freeze`
    /// is a `matches!`, so a new variant compiles. And the two sides carry the rule to DIFFERENT
    /// consequences — the spine stops new jobs on the class, this one keeps blocks out of `safe(C)`.
    /// A divergence is therefore not "a stale mirror" but two nodes disagreeing about a block's
    /// weight depending on which path they consulted, which is the partition shape.
    ///
    /// The `match` below is exhaustive on purpose: a new terminal will not compile until whoever
    /// adds it says which carriage outcome it corresponds to. That is the whole mechanism — this
    /// tree has closed the same through-line defect twice by replacing two hand-kept lists with one
    /// lookup, and where the types genuinely differ, an exhaustive map is the same trick.
    #[test]
    fn the_spine_and_the_carriage_adjudicator_freeze_on_the_same_terminal() {
        use crate::palw_carriage::PalwCarriageError as E;
        use crate::palw_job_state::PalwJobStatusV3 as J;

        // Every status, mapped to the adjudication outcome that reports the same thing. `None`
        // where the status is not something an adjudicator can return at all — a job mid-flight is
        // not a verdict about a refutation.
        let corresponding = |status: J| -> Option<Result<TransactionOutpoint, E>> {
            match status {
                J::Open
                | J::Committed
                | J::PanelSelected
                | J::Provisional { .. }
                | J::ChallengeWindow { .. }
                | J::Disputed { .. }
                | J::Adjudicating { .. } => None,
                // The court reproduced divergent bits: the conviction lands and slashes.
                J::Convicted => Some(Ok(op(0xB1))),
                // The court reproduced the executor's bits: the challenger was wrong.
                J::NoFaultFound => Some(Err(E::StepConvictionNotProven("no fault found".into()))),
                // The window closed without the court deciding anything about the class.
                J::FinalizedAccepted | J::FinalizedRejected => Some(Err(E::EquivocationNotProven("window closed".into()))),
                // The one that is about the CLASS rather than about either party.
                J::Unadjudicable => Some(Err(E::StepUnadjudicable)),
            }
        };

        let statuses = [
            J::Open,
            J::Committed,
            J::PanelSelected,
            J::Provisional { accepted: true },
            J::Provisional { accepted: false },
            J::ChallengeWindow { provisionally_accepted: true },
            J::Disputed { provisionally_accepted: true },
            J::Adjudicating { provisionally_accepted: true },
            J::FinalizedAccepted,
            J::FinalizedRejected,
            J::Convicted,
            J::NoFaultFound,
            J::Unadjudicable,
        ];
        let mut agreed = 0;
        for status in statuses {
            let Some(outcome) = corresponding(status) else { continue };
            assert_eq!(
                status.demands_class_freeze(),
                outcome_freezes_class_v1(&outcome),
                "{status:?}: the spine and the carriage adjudicator disagree about freezing"
            );
            agreed += 1;
        }
        assert!(agreed >= 5, "the map must actually reach the terminals, got {agreed}");
        assert!(J::Unadjudicable.demands_class_freeze(), "and the shared answer is not 'never freeze'");
    }

    /// The walk's filters: a record of the wrong kind, or one the evaluating point has not reached,
    /// cannot freeze — and neither can an unproven conviction, which is the case a real chain will
    /// see most often.
    ///
    /// The positive case is covered by `only_the_unadjudicable_outcome_freezes_the_class` rather
    /// than here: reaching `Unadjudicable` end-to-end needs a refutation that passes every
    /// structural check and then meets an uncatalogued kernel, which no fixture in this crate
    /// builds. Splitting the rule out is what keeps that gap from being an untested rule.
    #[test]
    fn the_freeze_walk_ignores_the_wrong_kind_the_future_and_the_unproven() {
        let panel = vec![seat(1), seat(2), seat(3)];
        let bonds = bonds();

        let quiet = input(&[], &panel, 9_100, &bonds, &NoStepWeights);
        assert!(!class_frozen_before_close_v1(&quiet, accept_fixture_signature), "an empty chain freezes nothing");

        // Receipts are not convictions.
        let receipts = vec![receipt_row(1, 9_000)];
        let only_receipts = input(&receipts, &panel, 9_100, &bonds, &NoStepWeights);
        assert!(!class_frozen_before_close_v1(&only_receipts, accept_fixture_signature), "the wrong kind freezes nothing");

        // A conviction whose signature does not verify is the challenger's problem.
        let carriage = vec![conviction_row(9_000)];
        let unproven = input(&carriage, &panel, 9_100, &bonds, &NoStepWeights);
        assert!(!class_frozen_before_close_v1(&unproven, reject_every_signature), "an unproven conviction is not a coverage gap");

        // And nothing the evaluating point has not reached counts.
        let before = input(&carriage, &panel, 8_999, &bonds, &NoStepWeights);
        assert!(!class_frozen_before_close_v1(&before, accept_fixture_signature), "not yet on this chain");
    }

    /// The freeze's temporal scope, exhaustively — split out for the reason
    /// `only_the_unadjudicable_outcome_freezes_the_class` is: no fixture here reaches
    /// `Unadjudicable` end to end, so a bound left inline in the walk would be an untested bound on
    /// a rule that is about to be wired.
    ///
    /// The window half is the one that matters. Unbounded, a freeze is a broader retroactive
    /// demotion weapon than the late-`Open` already fixed as exactly that: a freeze is a fact about
    /// the CLASS, so one coverage gap surfacing at any later DAA would pull every matured block of
    /// the class back to `Provisional` at once — ADR-0039 §3e's "can never rewrite safe weight",
    /// rewritten wholesale.
    #[test]
    fn a_coverage_gap_freezes_only_inside_the_blocks_own_window() {
        let panel = [seat(1), seat(2)];
        let bonds = bonds();
        // accepted at 1 000, w_challenge 500 — the window closes at 1 500.
        let at = |pov: u64| input(&[], &panel, pov, &bonds, &NoStepWeights);

        let late_pov = at(9_000);
        for inside in [0, 1, 999, 1_000, 1_400, 1_499, 1_500] {
            assert!(freeze_record_is_in_scope_v1(inside, &late_pov), "{inside} is inside the window");
        }
        for outside in [1_501, 1_600, 9_000, u64::MAX] {
            assert!(!freeze_record_is_in_scope_v1(outside, &late_pov), "{outside} is after the window closed");
        }

        // And the point of view still bounds it, for records the window would otherwise admit:
        // nothing the evaluating point has not reached is a fact about its chain.
        let early_pov = at(1_200);
        assert!(freeze_record_is_in_scope_v1(1_200, &early_pov), "reached");
        assert!(!freeze_record_is_in_scope_v1(1_201, &early_pov), "not yet reached, though inside the window");

        // A schedule whose window would overflow the DAA space saturates rather than wrapping —
        // wrapping would make the bound admit nothing at all, which fails OPEN for the freeze.
        let mut huge = at(u64::MAX);
        huge.accepted_daa = u64::MAX - 1;
        assert!(freeze_record_is_in_scope_v1(u64::MAX, &huge), "saturating, not wrapping");
    }

    /// ADR-0038 I10 END TO END, which nothing could reach before `unadjudicable_refutation`
    /// existed: a class whose catalog cannot decide a refuted step keeps its blocks out of `Final`.
    ///
    /// The rule was implemented as far as `class_frozen_before_close_v1` and then not consumed —
    /// `resolve_block_facts_v1` filled the field named `dispute_open_or_unadjudicable` with the
    /// dispute half alone, so a block with a full receipt quorum and a live coverage gap against it
    /// matured to `Final` and entered `safe(C)`. A block nothing can be held to, counted as
    /// finality.
    ///
    /// And the second half of the rule, which is the one the window bound is for: the SAME
    /// carriage accepted after the window closed leaves a matured block alone. Without that a
    /// freeze is a broader retroactive demotion than the late-`Open` already fixed as one, because
    /// it is a fact about the class rather than about a block.
    #[test]
    fn a_coverage_gap_keeps_the_block_out_of_final() {
        let bonds = bonds();
        let panel = [seat(1), seat(2)];
        let quorum = vec![receipt_row(1, 1_100), receipt_row(2, 1_110)];

        // The control: this quorum matures on its own.
        assert_eq!(
            resolve_block_weight_v1(&input(&quorum, &panel, 9_000, &bonds, &NoStepWeights), &RAMP, accept_fixture_signature)
                .unwrap()
                .stage,
            PalwWorkRampStageV1::Final
        );

        // Window closes at 1 500. A gap surfacing inside it clouds the block, permanently.
        let mut clouded = quorum.clone();
        clouded.push(unadjudicable_conviction_row(1_400));
        let inp = input(&clouded, &panel, 9_000, &bonds, &NoStepWeights);
        assert!(class_frozen_before_close_v1(&inp, accept_fixture_signature), "the fixture must actually reach Unadjudicable");
        assert_eq!(
            resolve_block_facts_v1(&inp, accept_fixture_signature).dispute_open_or_unadjudicable,
            Some(true),
            "the field's name promises this half too"
        );
        assert_eq!(
            resolve_block_weight_v1(&inp, &RAMP, accept_fixture_signature).unwrap().stage,
            PalwWorkRampStageV1::Provisional,
            "a block nothing can be held to must not enter safe(C)"
        );
        // It is a coverage gap, not a conviction: nobody is slashed and the block is not Voided.
        assert_eq!(resolve_block_facts_v1(&inp, accept_fixture_signature).convicted_before_close, Some(false));

        // And the same record after the window leaves the matured block alone.
        let mut late = quorum.clone();
        late.push(unadjudicable_conviction_row(1_501));
        let late_inp = input(&late, &panel, 9_000, &bonds, &NoStepWeights);
        assert!(!class_frozen_before_close_v1(&late_inp, accept_fixture_signature), "past the window");
        assert_eq!(
            resolve_block_weight_v1(&late_inp, &RAMP, accept_fixture_signature).unwrap().stage,
            PalwWorkRampStageV1::Final,
            "a gap surfacing after the window stops the class going forward, it does not rewrite safe weight"
        );
    }

    /// The dominant term of block weight is looked up BY the block's class, not handed in beside
    /// it — so a view that does not hold the block's class cannot be papered over with someone
    /// else's numbers.
    #[test]
    fn a_block_whose_class_the_view_does_not_hold_is_unresolved_not_zero() {
        struct EmptyView;
        impl PalwClassFactsViewV1 for EmptyView {
            fn class_facts_for_block(&self, _: &Hash64, _: u64) -> Option<PalwClassFactsV1> {
                None
            }
        }
        let carriage = vec![receipt_row(1, 1_050), receipt_row(2, 1_060)];
        let panel = vec![seat(1), seat(2), seat(3)];
        let bonds = bonds();
        let mut input = input(&carriage, &panel, 1_100, &bonds, &NoStepWeights);
        input.classes = &EmptyView;
        assert!(matches!(
            resolve_block_weight_v1(&input, &RAMP, accept_fixture_signature),
            Err(PalwFactsError::Unresolved { what: "the block's execution class" })
        ));
    }

    /// A view that answers about a DIFFERENT class is refused rather than believed. Without the
    /// stamp this would price the block with another class's target and per-inference cost — in
    /// the term that carries most of fork-choice weight, and silently.
    #[test]
    fn class_facts_for_the_wrong_class_are_refused() {
        struct WrongClassView;
        impl PalwClassFactsViewV1 for WrongClassView {
            fn class_facts_for_block(&self, _: &Hash64, _: u64) -> Option<PalwClassFactsV1> {
                Some(PalwClassFactsV1 { execution_class_id: h(0xDEAD), class_target: u128::MAX >> 10, pwu_per_inference: 100 })
            }
        }
        let carriage = vec![receipt_row(1, 1_050), receipt_row(2, 1_060)];
        let panel = vec![seat(1), seat(2), seat(3)];
        let bonds = bonds();
        let mut input = input(&carriage, &panel, 1_100, &bonds, &NoStepWeights);
        input.classes = &WrongClassView;
        assert!(matches!(
            resolve_block_weight_v1(&input, &RAMP, accept_fixture_signature),
            Err(PalwFactsError::ClassFactsMismatch { .. })
        ));
    }

    /// The view is asked about the BLOCK's acceptance point, not the evaluating node's. A target
    /// that moved after the block was accepted must not re-price it — the defect was that every
    /// retarget rewrote the weight of history that had already matured.
    #[test]
    fn the_class_target_asked_for_is_the_blocks_own_acceptance_point() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static ASKED_AT: AtomicU64 = AtomicU64::new(u64::MAX);
        struct RecordingView;
        impl PalwClassFactsViewV1 for RecordingView {
            fn class_facts_for_block(&self, id: &Hash64, block_accepted_daa: u64) -> Option<PalwClassFactsV1> {
                ASKED_AT.store(block_accepted_daa, Ordering::SeqCst);
                Some(PalwClassFactsV1 { execution_class_id: *id, class_target: u128::MAX >> 10, pwu_per_inference: 100 })
            }
        }
        let carriage = vec![receipt_row(1, 1_050), receipt_row(2, 1_060)];
        let panel = vec![seat(1), seat(2), seat(3)];
        let bonds = bonds();
        let mut input = input(&carriage, &panel, 1_100, &bonds, &NoStepWeights);
        input.classes = &RecordingView;
        let _ = resolve_block_weight_v1(&input, &RAMP, accept_fixture_signature);
        assert_eq!(ASKED_AT.load(Ordering::SeqCst), input.accepted_daa, "priced at the block's own point, not the observer's");
        assert_ne!(input.accepted_daa, input.pov_daa, "the fixture must keep the two apart or this proves nothing");
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
        let weight =
            resolve_block_weight_v1(&input(&carriage, &panel, 1_100, &bonds(), &NoStepWeights), &RAMP, accept_fixture_signature)
                .unwrap();
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
        assert_eq!(
            resolve_block_facts_v1(&input(&just_opened, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature)
                .dispute_open_or_unadjudicable,
            Some(true)
        );
        // An open dispute cannot mature, whatever the receipts say.
        let weight =
            resolve_block_weight_v1(&input(&just_opened, &panel, 9_000, &bonds(), &NoStepWeights), &RAMP, accept_fixture_signature)
                .unwrap();
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
        // A ladder played to its terminal index is NOT decided. `Terminal` means the bisection has
        // LOCATED the disputed step and the responder owes the terminal opening — nobody has
        // adjudicated it. The ladder itself charges the Responder for silence at `Terminal`, which is
        // the same statement. Reporting it as decided matured a block whose disputed step had never
        // been checked by anyone (re-audit).
        assert_eq!(
            resolve_block_facts_v1(&input(&rows, &panel, 1_200, &bonds(), &NoStepWeights), accept_fixture_signature)
                .dispute_open_or_unadjudicable,
            Some(true),
            "a located-but-unadjudicated step is an open dispute, not a decided one"
        );
        // NOTE the liveness cost this exposes, which is a real remaining gap rather than a choice:
        // the ladder has no vocabulary for the terminal OPENING, so a responder that answers
        // honestly and a challenger that then walks away leaves the ladder at `Terminal` forever and
        // the block permanently Provisional. Closing that needs a terminal-opening move (or a
        // deadline that charges the challenger once the opening has landed). Maturing the block
        // instead — which is what the old code did — trades a liveness cost for a soundness hole,
        // and that is the wrong direction.
        //
        // A terminal-opening move was designed and REJECTED (2026-08-17) on four independent
        // grounds. Two of them were defects in shipped code and are now fixed; two are prerequisites
        // that remain, and adding the move before they are settled converts this fail-CLOSED liveness
        // cost into a fail-OPEN soundness hole. Do not add it first.
        //
        // FIXED: the ladder's window budget charged one window per rung when a rung costs two, so a
        // conviction would have landed past `w_challenge` and been discarded (see
        // `palw_schedule::affordable_ladder_rounds_v1`); and this input's `commitment_root` was read
        // as both the announced root and the logits leg, so the terminal could not have been tied to
        // either (see the field's own doc).
        //
        // REMAINING, and each voids the move on its own:
        //
        // 1. `mid_state` IS NEVER CHECKED, so the rungs bind nothing. `apply_disclosure` pushes it
        //    into `PalwBisectLadderV1::disclosures` and NOTHING in the tree reads it — verified by
        //    grep. The field's own doc says "the terminal check's anchor pair comes from here", and
        //    that check does not exist. Consequence, with a terminal move added: a guilty responder
        //    discloses junk at every rung, the honest challenger's agree-iff-divergence-past-midpoint
        //    strategy disagrees every time, the interval collapses on an index the RESPONDER steered,
        //    and it then opens an honest early leaf. The challenger has nothing to convict on, goes
        //    quiet, and a challenger no-show settles `NoFaultFound` — the fraud is credited and the
        //    honest challenger's bond is forfeited. Closing it needs a definition of "state
        //    commitment at index i" for each `PalwBisectSpaceV1`, verified at every disclosure.
        //
        // 2. THE LADDER'S OUTCOME CANNOT BE A MATURITY TRIGGER while the conviction it defers to is
        //    unfileable. The ladder exists for a miner that WITHHELD, so no execution attestation is
        //    on chain — and `adjudicate_step_conviction_carriage_v1` accepts only that object as its
        //    authorship half. A terminal that charges the challenger for not filing what it
        //    structurally cannot file is fail-open by construction. Either give the adjudicator an
        //    authorship arm that accepts a commitment carriage (whose signature already covers the
        //    composite root and the bond outpoint), or route the ladder into the same one-step check
        //    the direct route uses. Until then the correct terminal is `Unadjudicable` — nobody
        //    slashed, nothing credited — never `Final`.
        //
        // Also unresolved and cheaper: `Open` carries `responder_id` only, and a validator key hash
        // is not unique to a bond, so no ladder outcome can name an executor bond to slash. Adding
        // `responder_bond_outpoint` to the existing `Open` variant is a wire edit that is cheap only
        // while every preset carries `palw_credit: None`.

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
        assert_eq!(
            resolve_block_facts_v1(&input(&other, &panel, 1_100, &bonds(), &NoStepWeights), accept_fixture_signature)
                .dispute_open_or_unadjudicable,
            Some(false)
        );
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
        let unbonded = resolve_block_facts_v1(&input(&carriage, &panel, 1_100, &no_bonds, &NoStepWeights), accept_fixture_signature);
        assert_eq!(unbonded.assigned_receipts, Some(0), "an unbonded filer is not a licence");

        // (3b) bond ACTIVITY — a slashed bond resolves but is not active, so it cannot license.
        let slashed = crate::dns_finality::ActiveBondView::from_records([1u8, 2, 3].into_iter().map(|s| {
            let mut b = bond(s);
            b.slashed_at_daa_score = Some(500);
            b.status = crate::dns_finality::BondStatus::Slashed;
            (op(s), b)
        }));
        let after_slash = resolve_block_facts_v1(&input(&carriage, &panel, 1_100, &slashed, &NoStepWeights), accept_fixture_signature);
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
            version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
            executor_id: h(0xE1),
            job_context_hash: ctx.context_hash(),
            full_logits_trace_root: root,
            committed_root: root,
            bond_outpoint: op(0xB1),
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        };
        let e = PalwEquivocationCarriageV1 {
            version: PALW_CARRIAGE_VERSION_V1,
            accused_bond_outpoint: op(0xB1),
            certificate: crate::palw_slash::PalwClassContradictionCertificateV1 {
                version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
                // The LOGITS leg of this block's execution, which is what an attestation signs — not the
                // announced composite root. The two are distinct in the fixture on purpose.
                attestation_a: att(h(0x1A)),
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
        let without_challenger = crate::dns_finality::ActiveBondView::from_records([1u8, 2, 3].into_iter().map(|s| (op(s), bond(s))));
        assert_eq!(
            resolve_block_facts_v1(&input(&rows, &panel, 1_100, &without_challenger, &NoStepWeights), accept_fixture_signature)
                .dispute_open_or_unadjudicable,
            Some(false),
            "an unbonded Open is not a dispute"
        );
    }

    /// A conviction accepted before the window closes voids the block; one after it does not.
    /// **The announced root and the logits leg are two different facts, and each consumer must read
    /// its own.**
    ///
    /// They were ONE field. The conviction and equivocation arms compare it against an attestation's
    /// `full_logits_trace_root`; the bisection arm compares it against a ladder `Open`'s
    /// `committed_root`, which for a composite class is `execution_commitment_root_v1` — four inputs
    /// under a different domain than the logits leg. So one of the two consumers could never match,
    /// and which one depended on what the caller put in the field: either no conviction could ever
    /// count against a composite block, or no dispute could ever open on one.
    ///
    /// Nothing failed because the two coincide for bare-v2, which is what every fixture used. This
    /// test gives them distinct values and pins that each arm follows the right one.
    #[test]
    fn a_conviction_reads_the_logits_leg_while_a_ladder_reads_the_announced_root() {
        let ctx = crate::palw_step_refute::tests::skeleton_refutation().binding.job_context;
        let att = |root: Hash64| crate::palw_slash::PalwExecutionAttestationV1 {
            version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
            executor_id: h(0xE1),
            job_context_hash: ctx.context_hash(),
            full_logits_trace_root: root,
            committed_root: root,
            bond_outpoint: op(0xB1),
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        };
        let equivocation = |root: Hash64| {
            let e = crate::palw_carriage::PalwEquivocationCarriageV1 {
                version: PALW_CARRIAGE_VERSION_V1,
                accused_bond_outpoint: op(0xB1),
                certificate: crate::palw_slash::PalwClassContradictionCertificateV1 {
                    version: crate::palw_slash::PALW_S_OBJECT_VERSION_V3,
                    attestation_a: att(root),
                    attestation_b: att(h(0x02)),
                    job_context: ctx.clone(),
                },
            };
            (crate::palw_carriage::PALW_CARRIAGE_KIND_EQUIVOCATION, 1_400u64, body(&PalwCarriageV1::Equivocation(e)))
        };
        let panel = vec![seat(1), seat(2)];
        let base = vec![receipt_row(1, 1_050), receipt_row(2, 1_060)];

        // Against the LOGITS leg it convicts…
        let mut with_logits = base.clone();
        with_logits.push(equivocation(h(0x1A)));
        assert_eq!(
            resolve_block_facts_v1(&input(&with_logits, &panel, 9_000, &bonds(), &NoStepWeights), accept_fixture_signature)
                .convicted_before_close,
            Some(true)
        );
        // …and against the ANNOUNCED root it does not. Reading the wrong field here is a conviction
        // that silently never counts, which is the fail-open direction.
        let mut with_announced = base.clone();
        with_announced.push(equivocation(h(0xC0)));
        assert_eq!(
            resolve_block_facts_v1(&input(&with_announced, &panel, 9_000, &bonds(), &NoStepWeights), accept_fixture_signature)
                .convicted_before_close,
            Some(false),
            "an attestation over another execution's root must not convict this block"
        );

        // Symmetrically for the ladder: an `Open` naming the announced root opens the dispute, and
        // one naming the logits leg does not.
        let open = |root: Hash64| {
            let m = crate::palw_carriage::PalwBisectMoveCarriageV1 {
                version: PALW_CARRIAGE_VERSION_V1,
                // A BONDED challenger: an Open from an unbonded record opens nothing, which is a
                // different rule and must not be what this test is measuring.
                challenger_bond_outpoint: op(0xC9),
                body: PalwBisectMoveBodyV1::Open {
                    job_context_hash: ctx.context_hash(),
                    committed_root: root,
                    challenger_id: h(0xC2),
                    responder_id: h(0xE1),
                    space: crate::palw_bisect::PalwBisectSpaceV1::StepLeaves,
                    space_size: 16,
                },
            };
            (crate::palw_carriage::PALW_CARRIAGE_KIND_BISECT_MOVE, 1_100u64, body(&PalwCarriageV1::BisectMove(m)))
        };
        let mut ladder_on_announced = base.clone();
        ladder_on_announced.push(open(h(0xC0)));
        assert!(
            resolve_block_facts_v1(&input(&ladder_on_announced, &panel, 1_200, &bonds(), &NoStepWeights), accept_fixture_signature)
                .dispute_open_or_unadjudicable
                .unwrap_or(false),
            "an Open naming the announced root opens this block's dispute"
        );
        let mut ladder_on_logits = base;
        ladder_on_logits.push(open(h(0x1A)));
        assert!(
            !resolve_block_facts_v1(&input(&ladder_on_logits, &panel, 1_200, &bonds(), &NoStepWeights), accept_fixture_signature)
                .dispute_open_or_unadjudicable
                .unwrap_or(true),
            "an Open naming another root must not pin this block at Provisional"
        );
    }

    /// The comparison is against chain DAA, so every node agrees about which side it fell.
    #[test]
    fn a_conviction_counts_only_before_the_window_closes() {
        let equivocation = equivocation_row;
        let panel = vec![seat(1), seat(2)];

        // Window closes at 1_000 + 500 = 1_500.
        let before = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), equivocation(1_400)];
        assert_eq!(
            resolve_block_facts_v1(&input(&before, &panel, 9_000, &bonds(), &NoStepWeights), accept_fixture_signature)
                .convicted_before_close,
            Some(true)
        );
        assert_eq!(
            resolve_block_weight_v1(&input(&before, &panel, 9_000, &bonds(), &NoStepWeights), &RAMP, accept_fixture_signature)
                .unwrap()
                .stage,
            PalwWorkRampStageV1::Voided
        );

        let after = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), equivocation(1_600)];
        assert_eq!(
            resolve_block_facts_v1(&input(&after, &panel, 9_000, &bonds(), &NoStepWeights), accept_fixture_signature)
                .convicted_before_close,
            Some(false)
        );
        assert_eq!(
            resolve_block_weight_v1(&input(&after, &panel, 9_000, &bonds(), &NoStepWeights), &RAMP, accept_fixture_signature)
                .unwrap()
                .stage,
            PalwWorkRampStageV1::Final,
            "a late conviction cannot unmake finality (W5)"
        );
    }

    /// The other half of ADR-0039's reorg-equivalence property, and the half that runs the
    /// dangerous way: **with the carriage fixed, advancing the point of view never un-VOIDS a
    /// block either.**
    ///
    /// A landed conviction voids this block's PALW weight. The accused's bond used to be re-checked
    /// by the adjudicator at `pov_daa` — resolving the record at the filing moment did not reach
    /// that second check — and `effective_bond_status` is one-way, so once `pov` passed the
    /// accused's unbond request the conviction stopped adjudicating and the void LIFTED. The
    /// punished block regained full weight by nothing but time passing, and the accused steered
    /// it: get convicted, unbond, be un-convicted.
    ///
    /// That direction is worse than the receipt one it mirrors. A receipt that stops counting
    /// costs an honest block its finality; a conviction that stops counting hands weight back to
    /// the block a proof said was wrong.
    #[test]
    fn a_voided_block_does_not_un_void_when_the_accused_leaves() {
        let panel = vec![seat(1), seat(2)];
        // Window closes at 1_000 + 500 = 1_500; the conviction lands inside it.
        let carriage = vec![receipt_row(1, 1_050), receipt_row(2, 1_060), equivocation_row(1_400)];

        // The accused requests unbonding at 2_000 — after it was convicted.
        let leaving = {
            let mut accused = bond(0xB1);
            accused.validator_pubkey_hash = h(0xE1);
            accused.unbond_request_daa_score = Some(2_000);
            crate::dns_finality::ActiveBondView::from_records(
                [1u8, 2, 3, 0xC9].into_iter().map(|s| (op(s), bond(s))).chain([(op(0xB1), accused)]),
            )
        };

        for pov in [1_500, 1_999, 2_000, 9_000] {
            let inp = input(&carriage, &panel, pov, &leaving, &NoStepWeights);
            assert_eq!(
                resolve_block_facts_v1(&inp, accept_fixture_signature).convicted_before_close,
                Some(true),
                "pov {pov}: a conviction that landed stays landed"
            );
            assert_eq!(
                resolve_block_weight_v1(&inp, &RAMP, accept_fixture_signature).unwrap().stage,
                PalwWorkRampStageV1::Voided,
                "pov {pov}: voided weight must not be handed back"
            );
        }

        // The rule it must NOT become: 'the accused's bond is never checked'. One already gone when
        // the accusation was filed has nothing at stake to answer with, so the accusation is not
        // adjudicable against it and the block keeps its weight.
        let already_gone = {
            let mut accused = bond(0xB1);
            accused.validator_pubkey_hash = h(0xE1);
            accused.unbond_request_daa_score = Some(1_200);
            crate::dns_finality::ActiveBondView::from_records(
                [1u8, 2, 3, 0xC9].into_iter().map(|s| (op(s), bond(s))).chain([(op(0xB1), accused)]),
            )
        };
        let inp = input(&carriage, &panel, 9_000, &already_gone, &NoStepWeights);
        assert_eq!(resolve_block_facts_v1(&inp, accept_fixture_signature).convicted_before_close, Some(false));
        assert_eq!(resolve_block_weight_v1(&inp, &RAMP, accept_fixture_signature).unwrap().stage, PalwWorkRampStageV1::Final);
    }

    /// ADR-0039's reorg-equivalence obligation, as a SUITE rather than as anecdotes.
    ///
    /// Each fix above caught one attack, and each was found by asking the same question of one more
    /// call site. The question generalises, so the test should: **a block's classification, once
    /// decided, is permanent and unique.** `Final` and `Voided` are the decisions; every earlier
    /// point of view may move freely between `Provisional` and `ReceiptLicensed`.
    ///
    /// The looser half is not slack, it is ADR-0039 §3e: *"a conviction can never rewrite safe
    /// weight — it can only prevent work from entering it. Retroactive void therefore acts on live
    /// weight alone, inside the challenge window, above the safe frontier."* Below the decision the
    /// stage governs only bounded live weight, and evidence arriving there is supposed to move it.
    /// An earlier draft of this test asserted a monotone rank instead and forbade exactly that — it
    /// failed the moment a real coverage gap became visible mid-window, which is the ADR working.
    ///
    /// Each point of view sees only the carriage it has REACHED. Holding the whole set visible
    /// throughout would model a node that can see its own future, and the transition being tested
    /// — a record becoming visible — would never happen.
    ///
    /// The sweep straddles every boundary the fixture has: the window close at 1 500, the unbond
    /// requests the leaving scenarios place at 1 800 and 2 000, and a point of view far past all of
    /// them, where a one-way `effective_bond_status` has long since flipped.
    ///
    /// `panel_duty_v1` rides the same axis under a stricter rule — it may go `Pending` to exactly
    /// one `Closed` answer and never move again. A drifting duty set is worse than a drifting
    /// stage: it names the seats a slash path would charge, so two nodes that disagree about it
    /// charge different validators for the same block.
    #[test]
    fn once_decided_a_block_stays_decided() {
        /// The two stages that are DECISIONS rather than positions. `Provisional` and
        /// `ReceiptLicensed` are both "not decided yet" — they differ only in how much live weight
        /// a chain may carry meanwhile, which is bounded by β and is not finality.
        fn is_terminal(stage: PalwWorkRampStageV1) -> bool {
            matches!(stage, PalwWorkRampStageV1::Final | PalwWorkRampStageV1::Voided)
        }

        // A bond view in which ONE party asks to unbond, at a DAA of the scenario's choosing.
        let leaving_at = |seed: u8, daa: u64| {
            let mut accused = bond(0xB1);
            accused.validator_pubkey_hash = h(0xE1);
            crate::dns_finality::ActiveBondView::from_records(
                [1u8, 2, 3, 0xC9]
                    .into_iter()
                    .map(|s| {
                        let mut b = bond(s);
                        if s == seed {
                            b.unbond_request_daa_score = Some(daa);
                        }
                        (op(s), b)
                    })
                    .chain([(
                        op(0xB1),
                        if seed == 0xB1 {
                            let mut a = accused.clone();
                            a.unbond_request_daa_score = Some(daa);
                            a
                        } else {
                            accused
                        },
                    )]),
            )
        };

        let quorum = || vec![receipt_row(1, 1_100), receipt_row(2, 1_110)];
        let with = |extra: (u8, u64, Vec<u8>)| {
            let mut c = quorum();
            c.push(extra);
            c
        };

        // Window closes at 1 000 + 500 = 1 500.
        let scenarios: Vec<(&str, Vec<(u8, u64, Vec<u8>)>, crate::dns_finality::ActiveBondView)> = vec![
            ("quorum and nothing against it", quorum(), bonds()),
            ("one receipt short of quorum", vec![receipt_row(1, 1_100)], bonds()),
            ("no carriage at all", vec![], bonds()),
            ("a conviction inside the window", with(equivocation_row(1_400)), bonds()),
            ("a conviction after the window", with(equivocation_row(1_600)), bonds()),
            ("a coverage gap inside the window", with(unadjudicable_conviction_row(1_400)), bonds()),
            ("a coverage gap after the window", with(unadjudicable_conviction_row(1_600)), bonds()),
            ("a dispute opened inside the window", with(open_row(1_200)), bonds()),
            ("a dispute opened after the window", with(open_row(2_400)), bonds()),
            ("a verifier unbonds after filing", quorum(), leaving_at(1, 1_800)),
            ("the accused unbonds after conviction", with(equivocation_row(1_400)), leaving_at(0xB1, 2_000)),
            ("the challenger unbonds after opening", with(open_row(1_200)), leaving_at(0xC9, 1_800)),
        ];

        // Every point of view is >= the block's own acceptance at 1 000.
        const POVS: [u64; 11] = [1_000, 1_200, 1_400, 1_499, 1_500, 1_501, 1_800, 2_000, 2_500, 5_000, 1_000_000];

        let panel = [seat(1), seat(2)];
        let mut ever_final = false;
        let mut ever_voided = false;
        for (name, carriage, bonds) in &scenarios {
            let mut stages: Vec<PalwWorkRampStageV1> = Vec::new();
            let mut duties: Vec<PalwPanelDutyV1> = Vec::new();
            for pov in POVS {
                // The carriage this point of view has actually REACHED. Holding the whole set
                // visible at every pov would model a node that can see its own future, and the
                // transition being tested — a record becoming visible — would never happen.
                let visible: Vec<(u8, u64, Vec<u8>)> = carriage.iter().filter(|(_, daa, _)| *daa <= pov).cloned().collect();
                let inp = input(&visible, &panel, pov, bonds, &NoStepWeights);
                stages.push(resolve_block_weight_v1(&inp, &RAMP, accept_fixture_signature).expect("every fixture resolves").stage);
                duties.push(panel_duty_v1(&inp, accept_fixture_signature));
            }

            // The duty accounting rides the same axis, and a drifting one is worse than a drifting
            // stage: it names the seats a slash path would charge, so two nodes that disagree about
            // it charge different validators for the same block. It may only go `Pending` -> one
            // `Closed` answer, and that answer may never change afterwards.
            let mut closed: Option<&PalwPanelDutyV1> = None;
            for (i, duty) in duties.iter().enumerate() {
                match (duty, closed) {
                    (PalwPanelDutyV1::Pending, None) => {}
                    (PalwPanelDutyV1::Pending, Some(_)) => {
                        panic!("{name}: pov {} went back to Pending after closing", POVS[i])
                    }
                    (d @ PalwPanelDutyV1::Closed { .. }, None) => closed = Some(d),
                    (d @ PalwPanelDutyV1::Closed { .. }, Some(first)) => {
                        assert_eq!(d, first, "{name}: pov {} named a different default set", POVS[i])
                    }
                }
            }
            assert!(closed.is_some(), "{name}: the sweep must outlive the duty window or it proves nothing about it");

            let mut terminal: Option<(usize, PalwWorkRampStageV1)> = None;
            for (i, stage) in stages.iter().enumerate() {
                match terminal {
                    Some((first, settled)) => assert_eq!(
                        *stage, settled,
                        "{name}: settled {settled:?} at pov {} and read {stage:?} at pov {}",
                        POVS[first], POVS[i]
                    ),
                    None if is_terminal(*stage) => terminal = Some((i, *stage)),
                    None => {}
                }
            }
            match terminal {
                Some((_, PalwWorkRampStageV1::Final)) => ever_final = true,
                Some((_, PalwWorkRampStageV1::Voided)) => ever_voided = true,
                _ => {}
            }
        }

        // The sweep must actually exercise both terminals, or a suite that never matures anything
        // would pass it vacuously.
        assert!(ever_final, "no scenario ever reached Final — the invariant would hold trivially");
        assert!(ever_voided, "no scenario ever reached Voided — the constancy rule was never exercised");
    }

    /// The suite's second axis, and the one the point-of-view sweep provably cannot see.
    ///
    /// That sweep holds the carriage fixed and advances the clock. The three window defects fixed
    /// on this branch all move the other way: the attacker holds the clock and **adds a record**.
    /// Checked against the sweep alone, removing the late-`Open` bound leaves every scenario
    /// `Provisional` at every point of view — constant, monotone, and passing. Two axes are needed
    /// because the attacks use two.
    ///
    /// The rule on this axis is flat rather than monotone: **carriage accepted after the challenge
    /// window closed cannot change a block's stage in either direction.** Late evidence is
    /// telemetry (W5). Both directions are load-bearing and each was broken on its own once — a
    /// late receipt used to top a block up to `Final` at a moment of the filer's choosing, and a
    /// late `Open` used to demote one that already was.
    ///
    /// Two bases, because "cannot change" is only meaningful where there is room to move in that
    /// direction: a matured block has room to fall, and one short of quorum has room to rise.
    ///
    /// The duty accounting is checked on the same records. It is not weight, but it is the set a
    /// slash path would charge, and a late receipt that discharged a seat's duty would let a
    /// defaulter buy its way out of the accounting after the fact — at a moment of its choosing,
    /// which is the shape of every defect on this axis.
    #[test]
    fn carriage_accepted_after_the_window_changes_nothing() {
        let bonds = bonds();
        let panel = [seat(1), seat(2), seat(3)];
        // Window closes at 1 000 + 500 = 1 500; every appended record lands well past it.
        const LATE: u64 = 1_600;
        const POV: u64 = 9_000;

        let stage_of = |carriage: &[(u8, u64, Vec<u8>)]| {
            resolve_block_weight_v1(&input(carriage, &panel, POV, &bonds, &NoStepWeights), &RAMP, accept_fixture_signature)
                .expect("every fixture resolves")
                .stage
        };
        // Duty travels with it. A late receipt that discharged a seat's duty would let a defaulter
        // buy its way out of the accounting after the fact, at a moment of its own choosing.
        let duty_of = |carriage: &[(u8, u64, Vec<u8>)]| {
            panel_duty_v1(&input(carriage, &panel, POV, &bonds, &NoStepWeights), accept_fixture_signature)
        };

        let bases: [(&str, Vec<(u8, u64, Vec<u8>)>, PalwWorkRampStageV1); 2] = [
            ("matured", vec![receipt_row(1, 1_100), receipt_row(2, 1_110)], PalwWorkRampStageV1::Final),
            ("one short of quorum", vec![receipt_row(1, 1_100)], PalwWorkRampStageV1::Provisional),
        ];

        for (base_name, base, expected) in &bases {
            assert_eq!(stage_of(base), *expected, "{base_name}: the base itself must be what the case is about");
            let base_duty = duty_of(base);
            assert!(matches!(base_duty, PalwPanelDutyV1::Closed { .. }), "{base_name}: the duty window must be closed by now");

            // One of every kind that carries weight meaning, and every one of them REACHES its
            // outcome: the equivocation lands a conviction, and the step conviction reaches
            // `Unadjudicable`. This arm used to hold `conviction_row`, which fails on its opening
            // path — it stood for the shape of a conviction and tested none of the rule, so the
            // freeze's window bound was covered here in name only.
            for (kind, late) in [
                ("a receipt", receipt_row(3, LATE)),
                ("a landed conviction", equivocation_row(LATE)),
                ("an unadjudicable conviction", unadjudicable_conviction_row(LATE)),
                ("a dispute", open_row(LATE)),
            ] {
                let mut extended = base.clone();
                extended.push(late);
                assert_eq!(
                    stage_of(&extended),
                    *expected,
                    "{base_name}: {kind} accepted after the window changed the stage — late evidence is telemetry (W5)"
                );
                assert_eq!(duty_of(&extended), base_duty, "{base_name}: {kind} accepted after the window changed who defaulted");
            }
        }
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
        // moves it cannot accept — and the two orders then sit at DIFFERENT turns, which the deadline
        // rule reads differently: at `AwaitVerdict` past the deadline the CHALLENGER is the silent
        // party and the dispute closes, while at `AwaitDisclosure` past it the RESPONDER is silent
        // and the dispute stays open. Same DAG, `Final` on one node and `Provisional` on another.
        // That is the §3.2 defect, and it takes a multi-move session plus a point of view past the
        // deadline to see it.
        let open = PalwBisectMoveBodyV1::Open {
            job_context_hash: h(0x11),
            committed_root: h(0xC0),
            challenger_id: h(0x33),
            responder_id: h(0x44),
            space: PalwBisectSpaceV1::StepLeaves,
            space_size: 8,
        };
        let session = bisect_session_id_v1(&h(0x11), &h(0xC0), &h(0x33), &h(0x44), PalwBisectSpaceV1::StepLeaves, 8);
        // TWO moves are the minimum that exposes order: the replay applies every move in the list,
        // so a single disclosure is accepted whatever position it sits in. With a disclosure AND a
        // verdict, forward accepts both and reaches `AwaitDisclosure` at round 1, while backward
        // meets the verdict first — refused, not its turn — then accepts the disclosure and stops at
        // `AwaitVerdict` at round 0. Past the deadline those two turns charge DIFFERENT parties, so
        // the same DAG answers `Provisional` one way and `Final` the other.
        let played = vec![
            receipt_row(1, 1_050),
            bisect_row(open, 1_070),
            bisect_row(
                PalwBisectMoveBodyV1::Disclosure(PalwBisectDisclosureV1 {
                    version: 1,
                    session_id: session,
                    round: 0,
                    midpoint: 4,
                    mid_state: h(4),
                }),
                1_080,
            ),
            bisect_row(
                PalwBisectMoveBodyV1::Verdict(PalwBisectVerdictV1 { version: 1, session_id: session, round: 0, agree: false }),
                1_085,
            ),
        ];

        // A point of view past the rung deadline (1_080 + W_ROUND = 1_110), so the deadline rule
        // fires and the two turns diverge into different answers.
        let pov = 1_200;
        let forward = resolve_block_facts_v1(&input(&played, &panel, pov, &bonds(), &NoStepWeights), accept_fixture_signature);
        assert_eq!(
            forward.dispute_open_or_unadjudicable,
            Some(true),
            "forward: both moves land, the responder owes round 1 and went silent — still open"
        );

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
    }
}
