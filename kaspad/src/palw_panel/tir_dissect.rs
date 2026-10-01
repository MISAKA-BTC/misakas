//! **RFC-0002 F7's node side: an IR class's history dissection, played** (spec 04b §9.5; design
//! §2.8).
//!
//! A committed tile whose cone reduces over the history is never closed in one move under the held
//! regime: a cone accusation there opens a session at `Terminal` on the leaf, and the dissection is
//! the court (`palw_tir_dissect_v1`). This module builds each party's move of it from the chain's own
//! view of the session (`PalwCourtDutyV2::tir_dissection`, the turn the fold clocks):
//!
//! * **the root claim** (the responder, move 1 — `CourtTirRootClaimed`, tag 64): every reduction
//!   over `H`'s honest totals and the finalize's carriage, from the responder's own capture;
//! * **each round** (the responder — `CourtTirDissected`, tag 65): the children of the disputed
//!   range, from the same capture;
//! * **each choice** (the challenger — `CourtTirChildChosen`, tag 66): the first child whose claimed
//!   partials are not the ones the challenger's OWN execution computes over that child, against the
//!   same root ([`misaka_palw_sdk::lineages::tir::tir_dissect_choice_v1`]);
//! * **the bottom** (either party — `CourtClosed` with `TirDissection`): one history tile, built from
//!   the ACCUSED capture (the court opens the accused's commitments), filed only by the party whose
//!   side it wins, as every close is.
//!
//! The responder speaks from its own capture (a fold is re-made by re-execution, so a class of any
//! size answers); the challenger's choices from its own execution; the bottom and the named-leaf
//! accusation that opens the session need the accused's leaves — a dense capture, or a fold this node
//! reproduces. Every object rides as filed: its binding's program emptied (RFC-0002 Phase F,
//! decision 2), signed under the ADR-0082 contexts over the IR messages. The heavy half
//! ([`palw_tir_dissect_build_v1`]) is pure and runs off the tick; the signature and the close's dry
//! run ([`palw_tir_dissect_object_v1`]) are the node's.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_v2::{
    PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT, PalwCourtVerdictProofV2,
};
use kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwCourtVerdictV2};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1;
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectRoundV1, PalwTirRootClaimV1, palw_tir_choice_message_v1,
    palw_tir_root_claim_message_v1, palw_tir_round_message_v1,
};
use misaka_palw_sdk::lineages::tir::{TirBackendV1, TirCaptureV1, tir_dissect_choice_v1};

use super::tir_court::{palw_tir_close_is_mine_v1, palw_tir_leaf_is_dissected_v1};

/// **A move of an IR history dissection.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwTirDissectMoveV1 {
    /// The responder's move 1: its root claim at the narrowed dissected leaf.
    Root,
    /// The responder's children of the disputed range.
    Round,
    /// The challenger's child.
    Choice,
    /// The bottom's close, filed by the party it wins for.
    Close,
}

/// **Which IR dissection move `duty` asks of this node now** — the turn the chain's duty view reports
/// (the fold's own helper, `court_session_turn_and_rung_deadline_v2`), never re-derived here. `None`:
/// the other party's move, a legacy (attention) phase, or no dissection at all. Move 1 is owed at a
/// fused IR class's terminal before any phase is open; whether the narrowed leaf is DISSECTED (a root
/// claim) or not (the executor's declared close) is the builder's to check against the capture's job.
pub(crate) fn palw_tir_dissect_move_of_duty_v1(duty: &PalwCourtDutyV2) -> Option<PalwTirDissectMoveV1> {
    if duty.dissection.is_some() {
        return None;
    }
    match (duty.tir_dissection.as_ref(), duty.i_am_responder, duty.turn) {
        (None, true, PalwBisectTurnV1::AwaitDisclosure) if duty.fused_class && duty.terminal_index.is_some() => {
            Some(PalwTirDissectMoveV1::Root)
        }
        (Some(_), true, PalwBisectTurnV1::AwaitDisclosure) => Some(PalwTirDissectMoveV1::Round),
        (Some(_), false, PalwBisectTurnV1::AwaitVerdict) => Some(PalwTirDissectMoveV1::Choice),
        (Some(_), _, PalwBisectTurnV1::Terminal) => Some(PalwTirDissectMoveV1::Close),
        _ => None,
    }
}

/// **A move, built and not yet signed** — its payload as it rides (the program stripped).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwTirDissectBuiltV1 {
    Root(Box<PalwTirRootClaimV1>),
    Round(PalwTirDissectRoundV1),
    Choice(PalwTirDissectChoiceV1),
    Close(PalwCourtVerdictProofV2),
}

/// **Build the move `mv` of `duty`'s dissection** — the heavy half, off the tick. `own` is the
/// party's own capture: the responder's is the claim's; a challenger's its own execution of the same
/// job. `accused` is the claim's capture (the bottom's source; the responder's own serves as it) and
/// `accused_root` the accused's root claim as it rode on chain — the bottom's source for a challenger
/// that holds no accused capture (RFC-0002's evidence transport, option D: the disputed leaf is the one
/// the root claim carries, every other leaf the challenger's own).
/// `Err` names what could not be built — the move is then not filed, and the clock decides.
pub(crate) fn palw_tir_dissect_build_v1(
    tir: &TirBackendV1,
    duty: &PalwCourtDutyV2,
    mv: PalwTirDissectMoveV1,
    own: &[u8],
    accused: Option<&[u8]>,
    accused_root: Option<&PalwTirRootClaimV1>,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwTirDissectBuiltV1, String> {
    let phase = || duty.tir_dissection.as_deref().ok_or_else(|| "no IR dissection phase is open on the session".to_string());
    match mv {
        PalwTirDissectMoveV1::Root => {
            let narrowed = duty.terminal_index.ok_or("the ladder names no leaf")?;
            let ctx = TirCaptureV1::decode(own)?.binding.job_context;
            if !palw_tir_leaf_is_dissected_v1(tir, &ctx, narrowed) {
                return Err(format!(
                    "leaf {narrowed} is not a dissected leaf: the executor's move there is a declared close, which this build does \
                     not file"
                ));
            }
            let mut root = tir.root_claim(own, narrowed, rules)?;
            // The acceptance layer's finalize, run here first (`check_tir_root_claim_v1`, the court's
            // limits): a root claim the chain would refuse is not paid for. An honest capture always
            // finalizes to its own tile; a capture whose committed leaf is not its evaluation does not.
            kaspa_consensus_core::palw_tir_court_v1::check_tir_root_claim_v1(&root, narrowed, rules)
                .map_err(|why| format!("the root claim does not finalize to the committed tile ({why})"))?;
            kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut root.finalize.binding);
            Ok(PalwTirDissectBuiltV1::Root(Box::new(root)))
        }
        PalwTirDissectMoveV1::Round => Ok(PalwTirDissectBuiltV1::Round(tir.dissect_round(own, phase()?, rules)?)),
        PalwTirDissectMoveV1::Choice => {
            let phase = phase()?;
            let honest = tir.dissect_round(own, phase, rules)?;
            let child = tir_dissect_choice_v1(phase, &honest)
                .ok_or("every child the responder filed is this node's own computation: there is no child to name")?;
            Ok(PalwTirDissectBuiltV1::Choice(PalwTirDissectChoiceV1 {
                version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                session_id: duty.session_id,
                round: phase.round(),
                child,
            }))
        }
        PalwTirDissectMoveV1::Close => {
            let phase = phase()?;
            let bottom = match (accused, accused_root) {
                (Some(accused), _) => tir.dissect_bottom(accused, phase, rules)?,
                (None, Some(root)) => {
                    // The root claim rode with its program empty and the chain's walk hands back what
                    // the fold refused too: it is read only as the claim's, at the phase's leaf.
                    let finalize = &root.finalize;
                    if finalize.binding.committed_execution_root != duty.execution_root
                        || finalize.binding.full_logits_trace_root != duty.trace_root
                        || finalize.output_opening.leaf_index != phase.leaf_index()
                    {
                        return Err("the root claim on chain is not this claim's at the phase's leaf".into());
                    }
                    let mut binding = finalize.binding.clone();
                    binding.class.program = tir.class().program.clone();
                    tir.dissect_bottom_from_root_claim(own, root, &binding, phase, rules)?
                }
                (None, None) => {
                    return Err(
                        "the bottom is built from the ACCUSED capture or its root claim on chain, and this node holds neither".into(),
                    );
                }
            };
            let mut proof = PalwCourtVerdictProofV2::TirDissection { bottom: Box::new(bottom) };
            proof.tir_strip_program_v1();
            Ok(PalwTirDissectBuiltV1::Close(proof))
        }
    }
}

/// **The move's object**: signed by the party that owes it — the claim's bond under the ADR-0082
/// responder context over the IR root and round messages, the session's challenger under the
/// challenger context over the choice — and a close only where the chain's dry run of it
/// (`verdict_of`) wins this party's side (`Ok(None)` otherwise: the other outcome needs no fee).
pub(crate) fn palw_tir_dissect_object_v1(
    built: PalwTirDissectBuiltV1,
    duty: &PalwCourtDutyV2,
    arity: u8,
    sign: &dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>>,
    verdict_of: &dyn Fn(&Hash64, &PalwCourtVerdictProofV2) -> Option<PalwCourtVerdictV2>,
) -> Result<Option<PalwConsensusObjectV2>, String> {
    let session_id = duty.session_id;
    let unsigned = || "no signing key for an IR dissection move".to_string();
    match built {
        PalwTirDissectBuiltV1::Root(root) => {
            let message = palw_tir_root_claim_message_v1(&session_id, &root);
            let signature = sign(&message, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT).ok_or_else(unsigned)?;
            Ok(Some(PalwConsensusObjectV2::CourtTirRootClaimed { session_id, root, arity, signature }))
        }
        PalwTirDissectBuiltV1::Round(round) => {
            let phase = duty.tir_dissection.as_deref().ok_or("no IR dissection phase is open on the session")?;
            let message = palw_tir_round_message_v1(&session_id, phase.round(), &round);
            let signature = sign(&message, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT).ok_or_else(unsigned)?;
            Ok(Some(PalwConsensusObjectV2::CourtTirDissected { session_id, round, signature }))
        }
        PalwTirDissectBuiltV1::Choice(choice) => {
            let signature =
                sign(&palw_tir_choice_message_v1(&choice), PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT).ok_or_else(unsigned)?;
            Ok(Some(PalwConsensusObjectV2::CourtTirChildChosen { session_id, choice, signature }))
        }
        PalwTirDissectBuiltV1::Close(proof) => {
            let verdict = verdict_of(&session_id, &proof).ok_or("the bottom does not adjudicate on this chain")?;
            Ok(palw_tir_close_is_mine_v1(verdict, duty.i_am_responder).then_some(PalwConsensusObjectV2::CourtClosed {
                session_id,
                verdict,
                proof,
            }))
        }
    }
}

/// The log's name of a move.
pub(crate) fn palw_tir_dissect_move_name_v1(mv: PalwTirDissectMoveV1) -> &'static str {
    match mv {
        PalwTirDissectMoveV1::Root => "root claim",
        PalwTirDissectMoveV1::Round => "round",
        PalwTirDissectMoveV1::Choice => "child choice",
        PalwTirDissectMoveV1::Close => "bottom close",
    }
}
