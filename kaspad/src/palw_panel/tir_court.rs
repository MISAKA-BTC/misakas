//! **An IR class's court close** (RFC-0002 Phase F, F6's node half).
//!
//! Once the ladder narrows a session to one step leaf, a legacy class closes with an arithmetic
//! refutation of that step (`refutation_for_index`, `operand_openings_for`). An IR class closes with
//! an IR proof instead (F5, appended to `PalwCourtVerdictProofV2`), built by the IR backend from the
//! ACCUSED capture — the same object whichever party builds it:
//!
//! * `TirCone` — the leaf's cone, demand-evaluated by the court over the units the close carries
//!   (either party: the court acquits an honest leaf and convicts a lying one, or convicts by
//!   PALW-TIR-33 an operand outside its proven interval);
//! * at a logits tile, for a challenger: `TirLogits` — the step tile against the same row's lanes in
//!   the committed trace, which convicts an executor that committed two different rows — and the
//!   decode-token door (`TirDecodeTokenTiled` / `TirDecodeToken`), which convicts a generated token
//!   that is not the greedy selection over its row, the challenger's own token naming the lane that
//!   beats it.
//!
//! A party files only the close that wins its side (the dissection's rule): the challenger a
//! conviction, the responder an acquittal. The other outcome needs no fee — the session's backstop
//! ends an unproven accusation on the challenger's side anyway.
//!
//! **Where the court plays no bisection** (the held regime, ADR-0103 Decision 1; testnet-12) an IR
//! claim is accused in one move instead (`TirShardCourtAccused`, tag 62): the challenger replays
//! the job the claim's block asked for, finds where its execution parts from the accused capture
//! ([`palw_tir_one_move_case_v1`]), and files the first IR close the court convicts on there
//! ([`palw_tir_one_move_accusation_to_file_v1`]), signed over its session id
//! (`PalwPanelService::tir_one_move_pass_v1`).

use std::collections::{HashMap, HashSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwCourtVerdictV2};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1;
use kaspa_consensus_core::palw_tir_one_move_v1::{
    PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1, PALW_TIR_ONE_MOVE_VERSION_V1, PalwTirOneMoveAccusationV1,
    palw_tir_one_move_session_id_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_core::{info, warn};
use misaka_palw_sdk::lineages::tir::{TirBackendV1, TirCaptureV1};

use super::PALW_PANEL;

/// A close a party may file, built or refused, under the label the log names it by.
pub(crate) type PalwTirCloseCandidateV1 = (&'static str, Result<PalwCourtVerdictProofV2, String>);

/// **A close as it rides**: its binding's program emptied — the chain holds the registered class's
/// program in its `tir_classes` row, puts it back before it reads the binding, and refuses a carried
/// one (RFC-0002 Phase F, decision 2).
fn as_filed(built: Result<PalwCourtVerdictProofV2, String>) -> Result<PalwCourtVerdictProofV2, String> {
    built.map(|mut proof| {
        proof.tir_strip_program_v1();
        proof
    })
}

/// The IR closes a party may file at leaf `index`, in the order it tries them, each built (as it
/// rides: its program stripped) or refused.
pub(crate) fn palw_tir_close_candidates_v1(
    tir: &TirBackendV1,
    accused: &[u8],
    own: Option<&[u8]>,
    index: u64,
    rules: &PalwTirCourtRulesV1,
    challenger: bool,
) -> Vec<PalwTirCloseCandidateV1> {
    let mut out = vec![("cone", as_filed(tir.cone_close(accused, index, rules)))];
    if !challenger {
        return out;
    }
    // At a logits tile the challenger also holds the two other doors. Its own execution names the
    // token it selected at that row — the lane that beats a wrongly committed one.
    let Ok(capture) = TirCaptureV1::decode(accused) else { return out };
    let ctx = &capture.binding.job_context;
    let space = tir.space();
    let Some(leaf) = space.leaf_at(ctx, index) else { return out };
    let post = (space.occurrences().len() - 1) as u32;
    let at_logits =
        matches!(leaf.kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if occurrence == post && node == space.program.logits);
    if !at_logits {
        return out;
    }
    out.push(("logits", as_filed(tir.logits_close(accused, index))));
    let Some(row) = (leaf.position + 1).checked_sub(ctx.declared_prefill_tokens) else { return out };
    let own_token = own.and_then(|own| TirCaptureV1::decode(own).ok()).and_then(|mine| mine.generated.get(row as usize).copied());
    let committed = capture.generated.get(row as usize).copied();
    if let Some(beat) = own_token.filter(|t| Some(*t) != committed) {
        out.push(("decode token", as_filed(tir.decode_token_close(accused, row, beat))));
    }
    out
}

/// Is `verdict` the side `i_am_responder` wins?
pub(crate) fn palw_tir_close_is_mine_v1(verdict: PalwCourtVerdictV2, i_am_responder: bool) -> bool {
    if i_am_responder { verdict == PalwCourtVerdictV2::ChallengerDefeated } else { verdict == PalwCourtVerdictV2::ExecutorGuilty }
}

/// The session a close is filed in, for the log.
pub(crate) fn palw_tir_close_note_v1(session_id: &Hash64, label: &str, verdict: PalwCourtVerdictV2, index: u64) -> String {
    format!("session {session_id} closes as {verdict:?} on IR step {index} ({label} close)")
}

/// Is `leaf` a tile of the committed logits (the post block's logits node)?
fn is_logits_leaf(tir: &TirBackendV1, kind: &PalwTirLeafKindV1) -> bool {
    let space = tir.space();
    let post = (space.occurrences().len() - 1) as u32;
    matches!(kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if *occurrence == post && *node == space.program.logits)
}

/// **A seat's one-move case against an IR claim** (the input of an IR one-move accusation, on
/// chains whose court plays no bisection): where the accused capture parts from this seat's own
/// execution of the same job, and the IR closes that may convict there, in the order a challenger
/// tries them:
///
/// * at the first step leaf the two part at — its cone, and at a logits tile the logits and
///   decode-token doors ([`palw_tir_close_candidates_v1`]);
/// * at the first decode row whose selected token differs — the decode-token door with this seat's
///   own token as the lane that beats the committed one. A lie in the SELECTION leaves every step
///   leaf of its row honest (the court's cone at the next position's input acquits it: that input
///   is the accused's own token), so without this door a wrong answer over honest arithmetic had no
///   court;
/// * where neither parts, at the first lane of the committed trace that differs — the logits door
///   over the step tile holding that lane (a trace that is not the one the steps computed).
///
/// `Ok(None)` when the two captures agree everywhere.
pub(crate) struct PalwTirOneMoveCaseV1 {
    /// The first step leaf at which the two executions part, if any does.
    pub leaf: Option<u64>,
    /// The first decode row whose selected token differs, if any does.
    pub row: Option<u32>,
    pub candidates: Vec<PalwTirCloseCandidateV1>,
}

pub(crate) fn palw_tir_one_move_case_v1(
    tir: &TirBackendV1,
    accused: &[u8],
    own: &[u8],
    rules: &PalwTirCourtRulesV1,
) -> Result<Option<PalwTirOneMoveCaseV1>, String> {
    let leaf = tir.first_divergent_leaf(accused, own)?;
    let (a, o) = (tir.decode_capture(accused)?, tir.decode_capture(own)?);
    let row = a.generated.iter().zip(&o.generated).position(|(x, y)| x != y).map(|r| r as u32);
    let mut candidates = match leaf {
        Some(leaf) => palw_tir_close_candidates_v1(tir, accused, Some(own), leaf, rules, true),
        None => Vec::new(),
    };
    if let Some(r) = row {
        let door = as_filed(tir.decode_token_close(accused, r, o.generated[r as usize]));
        let offered = door.as_ref().is_ok_and(|d| candidates.iter().any(|(_, built)| built.as_ref().ok() == Some(d)));
        if !offered {
            candidates.push(("decode token", door));
        }
    }
    if leaf.is_none() && row.is_none() {
        let lane = a
            .logits_rows
            .iter()
            .zip(&o.logits_rows)
            .enumerate()
            .find_map(|(r, (x, y))| x.iter().zip(y).position(|(p, q)| p != q).map(|lane| (r as u32, lane as u64)));
        let ctx = &a.binding.job_context;
        let tile = lane.and_then(|(r, lane)| {
            let position = (ctx.declared_prefill_tokens + r).checked_sub(1)?;
            tir.space().leaves_of_position(ctx, position).into_iter().find(|l| {
                is_logits_leaf(tir, &l.kind)
                    && matches!(l.kind, PalwTirLeafKindV1::Commit { first_element, .. }
                        if (first_element..first_element + u64::from(l.value_count)).contains(&lane))
            })
        });
        if let Some(tile) = tile {
            candidates.push(("logits", as_filed(tir.logits_close(accused, tile.index))));
        }
    }
    if leaf.is_none() && row.is_none() && candidates.is_empty() {
        return Ok(None);
    }
    Ok(Some(PalwTirOneMoveCaseV1 { leaf, row, candidates }))
}

/// **Is step leaf `index` of the job `ctx` a DISSECTED leaf of the class** — a tile of a commit point
/// whose cone reduces over the history (RFC-0002 F7, `palw_tir_dissected_commit_points_v1`), which the
/// held regime never tries in one move: a cone accusation there opens a dissection instead.
pub(crate) fn palw_tir_leaf_is_dissected_v1(tir: &TirBackendV1, ctx: &PalwJobContextV2, index: u64) -> bool {
    let space = tir.space();
    let Some(leaf) = space.leaf_at(ctx, index) else { return false };
    let PalwTirLeafKindV1::Commit { block, node, .. } = leaf.kind else { return false };
    kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(&space.program).contains(&(block, node))
}

/// The label a one-move case gives its NAMED-LEAF proof (a dissected leaf's challenge).
pub(crate) const PALW_TIR_NAMED_LEAF_LABEL_V1: &str = "named leaf";

/// **A one-move case at a DISSECTED leaf** (RFC-0002 F7): under the held regime a cone accusation at a
/// leaf whose cone reduces over the history is not adjudicated — the chain opens a dissection there
/// with the accuser as its challenger (`palw_tir_one_move_outcome_v1` → `NeedsDissection`). So the
/// case's cone candidate becomes the NAMED-LEAF proof ([`TirBackendV1::named_leaf_refutation`]: the
/// accused's leaf and its opening, nothing else), filed as the challenge, and this node plays the
/// challenger's side of the dissection it opens (`palw_panel::tir_dissect`). The proof is built from the
/// accused's capture — a dense one, or a fold this node reproduces — and the bottom will be too, so a
/// case this node could not finish is never opened. The other doors (decode token, logits) are
/// adjudicated in one move and stay behind it. `Some(leaf)` beside the case when the first divergent
/// leaf is dissected.
pub(crate) fn palw_tir_one_move_case_at_dissected_leaf_v1(
    tir: &TirBackendV1,
    accused: &[u8],
    mut case: PalwTirOneMoveCaseV1,
    rules: &PalwTirCourtRulesV1,
) -> (PalwTirOneMoveCaseV1, Option<u64>) {
    let (Some(leaf), Ok(capture)) = (case.leaf, TirCaptureV1::decode(accused)) else { return (case, None) };
    if !palw_tir_leaf_is_dissected_v1(tir, &capture.binding.job_context, leaf) {
        return (case, None);
    }
    case.candidates.retain(|(label, _)| *label != "cone");
    let named = as_filed(
        tir.named_leaf_refutation(accused, leaf, rules)
            .map(|refutation| PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) }),
    );
    case.candidates.insert(0, (PALW_TIR_NAMED_LEAF_LABEL_V1, named));
    (case, Some(leaf))
}

/// **Does `proof` name a DISSECTED leaf of `target`'s claim?** — the chain's first gate on a named-leaf
/// accusation (`palw_tir_one_move_dissected_leaf_v1`), derived as the node holds it: the proof within
/// the court's byte ceiling and riding without its program, the node's program put back, the binding
/// naming the claim's class, artifact root and roots, and the leaf dissected and not convicted on its
/// face (`palw_tir_named_dissected_leaf_v1`). Such an accusation declares `ExecutorGuilty` and opens a
/// dissection; nothing else about it is adjudicated in one move.
pub(crate) fn palw_tir_one_move_names_a_dissected_leaf_v1(
    proof: &PalwCourtVerdictProofV2,
    target: &PalwDisputableClaimV2,
    program: &[u8],
    court: &PalwCourtParamsV2,
) -> bool {
    if kaspa_consensus_core::palw_court_v2::check_close_cost_v2(proof, court).is_err() {
        return false;
    }
    let PalwCourtVerdictProofV2::TirCone { refutation } = proof else { return false };
    if !refutation.binding.class.program.is_empty() {
        return false;
    }
    let mut filled = refutation.as_ref().clone();
    filled.binding.class.program = program.to_vec();
    let binding = &filled.binding;
    if binding.class.class_id(&binding.artifact_root) != target.class_id
        || binding.artifact_root != target.artifact_root
        || binding.committed_execution_root != target.execution_root
        || binding.full_logits_trace_root != target.trace_root
    {
        return false;
    }
    matches!(kaspa_consensus_core::palw_tir_court_v1::palw_tir_named_dissected_leaf_v1(&filled), Ok(Some(_)))
}

/// **The verdict an IR close proof supports against a claim, as far as a node derives it without
/// the chain's state** — the one-move gate's own derivation (`palw_tir_one_move_verdict_v1` →
/// `adjudicate_close_proof_v2`'s IR arm) with the state's three reads supplied by what the node
/// already holds (the claim's class and that class's artifact root, from the disputable-claim view,
/// and the class's `program`, from its own artifact, put back into the proof as the chain puts back
/// the registered one): the proof within the court's byte ceiling, its binding naming the claim's class, artifact
/// root and roots, then the IR court at the chain's ladder, prompt form and work limits. `None`
/// where the proof does not adjudicate. The gate re-derives it and refuses a mismatch, so a wrong
/// answer here costs a refused carrier, never a wrong verdict.
pub(crate) fn palw_tir_one_move_verdict_stateless_v1(
    proof: &PalwCourtVerdictProofV2,
    target: &PalwDisputableClaimV2,
    program: &[u8],
    court: &PalwCourtParamsV2,
    step_ladder: u64,
    prompt_form: PalwPromptIdsFormV1,
) -> Option<PalwCourtVerdictV2> {
    use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
    use kaspa_consensus_core::palw_tir_court_v1 as tir;
    kaspa_consensus_core::palw_court_v2::check_close_cost_v2(proof, court).ok()?;
    // The chain puts the registered program back into a close that rides without it, and refuses one
    // that carries it (decision 2); the node puts back its own copy — the class id the binding names
    // commits to it, so a wrong copy is a binding that does not name the claim's class.
    if !proof.tir_binding_v1()?.class.program.is_empty() {
        return None;
    }
    let mut filled = proof.clone();
    filled.tir_binding_mut_v1()?.class.program = program.to_vec();
    let proof = &filled;
    let binding = proof.tir_binding_v1()?;
    if binding.class.class_id(&binding.artifact_root) != target.class_id
        || binding.artifact_root != target.artifact_root
        || binding.committed_execution_root != target.execution_root
        || binding.full_logits_trace_root != target.trace_root
    {
        return None;
    }
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: step_ladder,
        prompt_form,
        limits: kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(court),
    };
    let outcome = match proof {
        PalwCourtVerdictProofV2::TirCone { refutation } => tir::check_tir_cone_refutation_v1(refutation, &rules),
        PalwCourtVerdictProofV2::TirLogits { accusation } => tir::check_tir_logits_consistency_v1(accusation, &rules),
        PalwCourtVerdictProofV2::TirDecodeTokenTiled { binding, pin } => tir::check_tir_decode_token_tiled_v1(binding, pin, &rules),
        PalwCourtVerdictProofV2::TirDecodeToken { binding, pin, position } => {
            tir::check_tir_decode_token_flat_v1(binding, pin, *position)
        }
        _ => return None,
    };
    match outcome {
        Ok(_) => Some(PalwCourtVerdictV2::ExecutorGuilty),
        Err(PalwStepRefuteError::NoFaultFound) => Some(PalwCourtVerdictV2::ChallengerDefeated),
        Err(_) => None,
    }
}

/// **The IR one-move accusation a challenger files against `target`**: the first candidate of its
/// one-move case the court convicts on, as `accuser` — unsigned (the caller signs
/// `palw_tir_one_move_session_id_v1` under `PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1`). `None`
/// when no candidate convicts: nothing is filed, because a one-move accusation that does not convict
/// charges its accuser.
pub(crate) fn palw_tir_one_move_accusation_to_file_v1(
    candidates: Vec<PalwTirCloseCandidateV1>,
    target: &PalwDisputableClaimV2,
    program: &[u8],
    accuser: PalwBondKeyV2,
    court: &PalwCourtParamsV2,
    step_ladder: u64,
    prompt_form: PalwPromptIdsFormV1,
) -> Option<(&'static str, PalwTirOneMoveAccusationV1)> {
    candidates.into_iter().find_map(|(label, built)| {
        let proof = built.ok()?;
        // A named leaf is the challenge of a dissection, not a verdict: the chain opens the session.
        let files = if label == PALW_TIR_NAMED_LEAF_LABEL_V1 {
            palw_tir_one_move_names_a_dissected_leaf_v1(&proof, target, program, court)
        } else {
            palw_tir_one_move_verdict_stateless_v1(&proof, target, program, court, step_ladder, prompt_form)
                == Some(PalwCourtVerdictV2::ExecutorGuilty)
        };
        files.then(|| {
            (
                label,
                PalwTirOneMoveAccusationV1 {
                    version: PALW_TIR_ONE_MOVE_VERSION_V1,
                    claim: target.claim_id,
                    execution_root: target.execution_root,
                    trace_root: target.trace_root,
                    executor_bond: target.executor_bond,
                    accuser_bond: accuser,
                    verdict: PalwCourtVerdictV2::ExecutorGuilty,
                    proof,
                    signature: Vec::new(),
                },
            )
        })
    })
}

/// **When an IR one-move accusation of a claim licensed at `licensed_daa` is due**: before the
/// claim can reach `Final` (its challenge window from the licence), less the landing margin every
/// automatic accusation keeps — so the priority lane carries it ahead of undated items, and it folds
/// while the claim can still be convicted.
pub(crate) fn palw_tir_one_move_due_v1(
    params: &kaspa_consensus_core::config::params::Params,
    licensed_daa: u64,
    current_daa: u64,
) -> u64 {
    let window = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state.window_challenge_at(licensed_daa),
        _ => 0,
    };
    super::palw_seat_court_filing_due_v1(licensed_daa.saturating_add(window), None, current_daa)
}

// ---------------------------------------------------------------------------------------------
// RFC-0002's evidence transport, option B: a seat's pursuit of a claim whose capture it never holds
// ---------------------------------------------------------------------------------------------

/// **The interval-lane request index an IR annex of `leaf` is asked under** — the leaf-evidence kind
/// (ADR-0111 Decision 2, bit 29) with the leaf's low bits as its "interval", so every leaf of one
/// descent is its own `(claim, index)` slot on the lane and in the seat's pool. The leaf itself rides
/// the request's signed field; the index only keys the answer.
pub(crate) fn palw_tir_annex_request_index_v1(leaf: u64) -> u32 {
    let low = (leaf & ((kaspa_consensus_core::palw_leaf_evidence_v1::PALW_LEAF_EVIDENCE_REQUEST_BIT_V1 as u64) - 1)) as u32;
    kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_request_index_v1(low).expect("below the bit")
}

/// How many DAA a seat waits for an annex before asking again, and how many asks of one leaf it makes
/// before it leaves the claim to the on-chain demand (option C) — an executor that does not serve is
/// the liar C exists for.
pub(crate) const PALW_TIR_ANNEX_REASK_DAA_V1: u64 = 3;
pub(crate) const PALW_TIR_ANNEX_ASKS_V1: u32 = 5;

/// **A seat's pursuit of one claim through served annexes.** The own execution and its tree are the
/// seat's re-run of the claim's job (the process-wide memo keeps the run itself); the rest is where
/// the descent stands.
#[derive(Clone)]
pub(crate) struct PalwTirAnnexPursuitV1 {
    pub own: std::sync::Arc<misaka_palw_sdk::lineages::tir::TirRetainedJobV1>,
    pub own_tree: std::sync::Arc<misaka_palw_sdk::lineages::tir::TirStepTreeV1>,
    /// The leaf whose annex is asked, and the level the search is bounded below.
    pub leaf: u64,
    pub below: Option<usize>,
    /// `Some((row, token))` once the steps agree and the ids part: the annex asked is the logits leaf
    /// whose tile holds the seat's own `token` at decode row `row` — the decode-token door's pin.
    pub token: Option<(u32, u32)>,
    pub asked_daa: u64,
    pub asks: u32,
    pub rounds: u32,
}

impl PalwTirAnnexPursuitV1 {
    pub fn new(own: std::sync::Arc<misaka_palw_sdk::lineages::tir::TirRetainedJobV1>, current_daa: u64) -> Self {
        let own_tree = std::sync::Arc::new(misaka_palw_sdk::lineages::tir::TirStepTreeV1::full(&own.leaf_hashes));
        Self { own, own_tree, leaf: 0, below: None, token: None, asked_daa: current_daa, asks: 1, rounds: 0 }
    }
}

/// **What one verified annex tells a pursuit to do next.**
pub(crate) enum PalwTirAnnexStepV1 {
    /// Ask this leaf's annex next (the descent's next round, or the decode-token door's logits leaf).
    Ask { leaf: u64, below: Option<usize>, token: Option<(u32, u32)> },
    /// The accusations the seat may file, in the order it tries them (each built, or refused).
    Accuse { leaf: Option<u64>, row: Option<u32>, candidates: Vec<PalwTirCloseCandidateV1> },
    /// Nothing the annexes can reach differs (a lie in the trace's lanes alone), or the annex is not
    /// of this job: the claim is left to the on-chain demand.
    Stop(&'static str),
}

/// **The pursuit's step on one annex** — pure, over the seat's own execution and one annex verified
/// against the claim (`binding` is its filled binding).
pub(crate) fn palw_tir_annex_step_v1(
    tir: &TirBackendV1,
    pursuit: &PalwTirAnnexPursuitV1,
    annex: &misaka_palw_sdk::lineages::tir::PalwTirLeafAnnexV1,
    binding: &kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1,
    rules: &PalwTirCourtRulesV1,
) -> PalwTirAnnexStepV1 {
    use misaka_palw_sdk::lineages::tir::{TirDivergenceV1, tir_first_divergence_from_opening_v1};
    let own = pursuit.own.as_ref();
    if annex.leaf() != pursuit.leaf {
        return PalwTirAnnexStepV1::Stop("the annex opens another leaf than the one asked");
    }
    // The decode-token door's round: the pin of the row, re-aimed at the seat's own token.
    if let Some((row, token)) = pursuit.token {
        let door = as_filed(tir.annex_decode_token_close(binding, annex, row, token));
        return PalwTirAnnexStepV1::Accuse { leaf: None, row: Some(row), candidates: vec![("decode token", door)] };
    }
    match tir_first_divergence_from_opening_v1(&pursuit.own_tree, &annex.opening, pursuit.below) {
        None => PalwTirAnnexStepV1::Stop("the annex's opening is not of this job's step tree"),
        Some(TirDivergenceV1::Within { level, first }) => PalwTirAnnexStepV1::Ask { leaf: first, below: Some(level), token: None },
        Some(TirDivergenceV1::At(leaf)) => {
            let ctx = &binding.job_context;
            let mut candidates = Vec::new();
            if palw_tir_leaf_is_dissected_v1(tir, ctx, leaf) {
                candidates.push((PALW_TIR_NAMED_LEAF_LABEL_V1, as_filed(tir.annex_named_leaf(binding, annex, own))));
            } else {
                candidates.push(("cone", as_filed(tir.annex_cone_close(binding, annex, own, rules))));
            }
            if tir.space().leaf_at(ctx, leaf).is_some_and(|l| is_logits_leaf(tir, &l.kind)) {
                candidates.push(("logits", as_filed(tir.annex_logits_close(binding, annex, own))));
            }
            PalwTirAnnexStepV1::Accuse { leaf: Some(leaf), row: None, candidates }
        }
        Some(TirDivergenceV1::Agrees) => {
            // Every step leaf is the seat's own: a lie, if any, is in the trace. The ids name a token
            // row; the annex of the logits leaf whose tile holds the seat's own token there carries
            // the decode-token door's pin.
            let Some(row) = annex.generated().iter().zip(&own.generated).position(|(a, b)| a != b) else {
                return PalwTirAnnexStepV1::Stop(
                    "every step leaf and every id agrees: a lie in the trace's lanes alone, which no annex round reaches",
                );
            };
            let token = own.generated[row];
            let ctx = &binding.job_context;
            let Some(position) = (ctx.declared_prefill_tokens + row as u32).checked_sub(1) else {
                return PalwTirAnnexStepV1::Stop("a decode row before the first selecting position");
            };
            let space = tir.space();
            let leaf = space.leaves_of_position(ctx, position).into_iter().find(|l| {
                is_logits_leaf(tir, &l.kind)
                    && matches!(l.kind, PalwTirLeafKindV1::Commit { first_element, .. }
                        if (first_element..first_element + u64::from(l.value_count)).contains(&u64::from(token)))
            });
            match leaf {
                Some(l) => PalwTirAnnexStepV1::Ask { leaf: l.index, below: None, token: Some((row as u32, token)) },
                None => PalwTirAnnexStepV1::Stop("no logits leaf of that row holds the seat's own token"),
            }
        }
    }
}

/// What the IR one-move pass reads and writes of the panel loop's bookkeeping.
pub(super) struct PalwTirOneMoveBooksV1<'a> {
    /// Claims this node has judged and reproduced.
    pub challenged: &'a mut HashSet<Hash64>,
    /// Claims this node has accused in one move (or found no accusation for): filed once.
    pub accused: &'a mut HashSet<Hash64>,
    pub court_pending: &'a mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    pub court_due: &'a mut HashMap<(Hash64, u32, bool), u64>,
    /// Option B: the claims pursued through served annexes (their capture is not held).
    pub pursuits: &'a mut HashMap<Hash64, PalwTirAnnexPursuitV1>,
    /// The interval lane's served payloads, where the annexes asked for arrive.
    pub openings: &'a HashMap<(Hash64, u32), Vec<Vec<u8>>>,
}

/// **How many claims a seat pursues through annexes at once** — each holds the seat's own run and its
/// step tree (≈ 0.3 GB for a 1.5B class at 512 positions); the next waits for the first to end.
pub(crate) const PALW_TIR_ANNEX_PURSUITS_V1: usize = 1;

/// **The claim a seat duty names, as the one-move pass reads a target** — for a claim this seat's
/// replay refuted before any licence: its block, class, roots and executor are the duty's.
pub(crate) fn palw_tir_duty_target_v1(duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2) -> PalwDisputableClaimV2 {
    PalwDisputableClaimV2 {
        accepted_block: duty.accepted_block,
        claim_id: duty.claim_id,
        class_id: duty.class_id,
        artifact_root: duty.artifact_root,
        executor_bond: duty.executor_bond,
        trace_root: duty.trace_root,
        execution_root: duty.execution_root,
        licensed_daa: duty.bound_daa,
        free_prompt: duty.free_prompt,
    }
}

impl super::PalwPanelService {
    /// **The rules and arity an IR dissection move is built under at `current_daa`** (RFC-0002 F7) —
    /// what the processor admits it by: the network's court limits, the claim's step ladder (an IR
    /// class records none, so the network's), the network's prompt form, and the arity the ruleset
    /// derives there (`palw_court_params_held_at_v2`, held-aware — the one a root claim must declare).
    /// `None` on a network with no V2 bundle.
    pub(super) fn tir_dissection_rules_v1(
        &self,
        tir: &TirBackendV1,
        class_id: Hash64,
        current_daa: u64,
    ) -> Option<(PalwTirCourtRulesV1, u8)> {
        let params = &self.consensus_config.params;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return None;
        };
        let arity = kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2(
            bundle,
            params.palw_kary_court_active_at(current_daa),
            params.palw_held_context_active_at(current_daa),
        )
        .ok()?
        .dissection_arity();
        let court = self.config.court;
        let network_ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
            &court,
            params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
        );
        let mut rules = tir.court_rules(&court);
        rules.max_step_leaf_count = self.seat_refutation_ladder_v1(class_id, network_ladder, current_daa);
        rules.prompt_form = self.config.prompt_ids_form;
        Some((rules, arity))
    }

    /// **RFC-0002 Phase F (F6): the IR one-move pass — an IR claim's court where the held regime
    /// plays no bisection.** Its targets:
    ///
    /// * every licensed claim of an IR class this node may dispute — each with `--palw-challenge`,
    ///   otherwise the ones its seat faulted (ADR-0085 Decision 4) — dated before the claim's `Final`;
    /// * **and at once, licensed or not, every IR claim this seat's replay refuted** (ADR-0098
    ///   Decision 2, ADR-0099 Decision 5: a seat that found a lie files it). Seats that replay refuse
    ///   the lie its licence, so a lie a seat found would otherwise meet no court at all — it would
    ///   only fail to license. Always on: the seat's duty, not a flag. Dated before the receipt
    ///   deadline, as the legacy capture arm's accusation is.
    ///
    /// For each: the capture the claim's producer served that answers for the claim's roots, this
    /// node's own execution of the job the claim's block asked for, and — where they part — the first
    /// IR close the court convicts on, checked as the gate derives it, signed over its session id and
    /// queued on the court's carrier path. Once per claim: a reproduced claim is judged, an accused
    /// one (or one no close convicts) is not tried again.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn tir_one_move_pass_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        seat_faulted: &HashSet<Hash64>,
        replay_refuted: &HashSet<Hash64>,
        mut books: PalwTirOneMoveBooksV1<'_>,
    ) {
        let mut targets: Vec<(PalwDisputableClaimV2, u64)> = session
            .palw_disputable_claims_v2(vec![bond_key])
            .into_iter()
            .filter(|t| self.config.challenge || seat_faulted.contains(&t.claim_id))
            .map(|t| {
                let due = palw_tir_one_move_due_v1(&self.consensus_config.params, t.licensed_daa, current_daa);
                (t, due)
            })
            .collect();
        if !replay_refuted.is_empty() {
            for duty in session.palw_seat_duties_v2(vec![bond_key]) {
                if !replay_refuted.contains(&duty.claim_id) || targets.iter().any(|(t, _)| t.claim_id == duty.claim_id) {
                    continue;
                }
                let earliest_final = super::palw_seat_claim_earliest_final_v1(&self.consensus_config.params, duty.bound_daa);
                let due = super::palw_seat_court_filing_due_v1(duty.receipt_deadline, earliest_final, current_daa);
                targets.push((palw_tir_duty_target_v1(&duty), due));
            }
        }
        // Option B: a pursuit whose claim is no longer a target (Final, voided, accused) ends.
        books.pursuits.retain(|claim, _| targets.iter().any(|(t, _)| t.claim_id == *claim) && !books.accused.contains(claim));
        for (target, due) in targets {
            if books.challenged.contains(&target.claim_id) || books.accused.contains(&target.claim_id) {
                continue;
            }
            // Only an IR class is tried here: a legacy claim's held court is its seat's capture arm.
            let tir = match self.backends().resolve_tir_v1(target.class_id, target.artifact_root) {
                None => continue,
                Some(Ok(tir)) => tir,
                Some(Err(why)) => {
                    crate::palw_backends::note_throttled_v1("tir-one-move-backend", || {
                        format!("[{PALW_PANEL}] claim {}: the IR backend does not build for its class ({why})", target.claim_id)
                    });
                    continue;
                }
            };
            // IR v1 serves attempts; a free-prompt IR job has no capture this pass could read.
            if target.free_prompt {
                continue;
            }
            // The accused capture: a served payload that answers for the claim's own roots — the
            // close is an assertion about the ACCUSED's steps (audit3 S-01).
            let Some(accused) = materials
                .get(&target.claim_id)
                .and_then(|pool| {
                    pool.iter().find(|m| {
                        tir.decode_capture(m).is_ok_and(|c| {
                            c.binding.committed_execution_root == target.execution_root
                                && c.binding.full_logits_trace_root == target.trace_root
                        })
                    })
                })
                .cloned()
            else {
                // **Option B: no capture reaches this seat** (a large class's never does) — the claim
                // is pursued through its executor's served annexes instead.
                self.tir_annex_pursuit_tick_v1(session, bond_key, network_domain, current_daa, &target, due, tir, &mut books).await;
                continue;
            };
            let Ok(backend) = self.resolve_backend(session, target.class_id, target.artifact_root) else { continue };
            // The anchor comes from the claim's block, never from the capture (see the bisection
            // half above for why).
            let Some((job, prompt)) = self.attempt_job_for_claim(
                session,
                backend.as_ref(),
                network_domain,
                target.accepted_block,
                target.class_id,
                &target.executor_bond,
            ) else {
                continue;
            };
            let Ok((_backend, run)) = super::offload(backend, move |b| b.execute(&job, &prompt)).await else { continue };
            let Ok(run) = run else { continue };
            if run.execution_root == target.execution_root && run.trace_root == target.trace_root {
                books.challenged.insert(target.claim_id);
                continue;
            }
            warn!(
                "[{PALW_PANEL}] IR claim {} committed an execution this node does not reproduce — building its one-move accusation",
                target.claim_id
            );
            // The ladder, prompt form and court the chain adjudicates the accusation at.
            let court = self.config.court;
            let network_ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                &court,
                self.consensus_config.params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
            );
            let ladder = self.seat_refutation_ladder_v1(target.class_id, network_ladder, current_daa);
            let form = self.config.prompt_ids_form;
            let mut rules = tir.court_rules(&court);
            rules.max_step_leaf_count = ladder;
            let (own, facts) = (run.material, target.clone());
            let Ok(found) = tokio::task::spawn_blocking(move || {
                let Some(case) = palw_tir_one_move_case_v1(&tir, &accused, &own, &rules)? else {
                    return Ok::<_, String>((None, None));
                };
                // RFC-0002 F7: at a dissected leaf the accusation is the named-leaf challenge.
                let (case, dissected) = palw_tir_one_move_case_at_dissected_leaf_v1(&tir, &accused, case, &rules);
                let program = tir.class().program.clone();
                let (leaf, row) = (case.leaf, case.row);
                let found = palw_tir_one_move_accusation_to_file_v1(case.candidates, &facts, &program, bond_key, &court, ladder, form)
                    .map(|(label, accusation)| (leaf, row, label, accusation));
                Ok((dissected, found))
            })
            .await
            else {
                continue;
            };
            // Tried once: a claim no IR close convicts is recorded, not filed.
            books.accused.insert(target.claim_id);
            let (leaf, row, label, accusation) = match found {
                Ok((_, Some(found))) => found,
                Ok((dissected, None)) => {
                    warn!(
                        "[{PALW_PANEL}] IR claim {}: {}; recorded, not filed",
                        target.claim_id,
                        match dissected {
                            Some(leaf) => format!(
                                "its first divergent leaf {leaf} is dissected and its named-leaf challenge does not build from the \
                                 accused capture held here (a fold this node does not reproduce opens no leaf)"
                            ),
                            None => "no IR close convicts where this node's execution parts from it".to_string(),
                        }
                    );
                    continue;
                }
                Err(why) => {
                    warn!(
                        "[{PALW_PANEL}] IR claim {}: its one-move case does not build ({why}); recorded, not filed",
                        target.claim_id
                    );
                    continue;
                }
            };
            self.file_tir_one_move_v1(&target, due, label, leaf, row, accusation, &mut books);
        }
    }

    /// **Sign and queue an IR one-move accusation** — the session id over the network domain, the
    /// accuser's signature, the carrier check, and the court's priority lane at `due`.
    fn file_tir_one_move_v1(
        &self,
        target: &PalwDisputableClaimV2,
        due: u64,
        label: &str,
        leaf: Option<u64>,
        row: Option<u32>,
        mut accusation: PalwTirOneMoveAccusationV1,
        books: &mut PalwTirOneMoveBooksV1<'_>,
    ) {
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.consensus_config.params.net.to_string().as_bytes(),
            Some(self.consensus_config.genesis.hash),
        );
        let session_id = palw_tir_one_move_session_id_v1(domain.as_byte_slice(), &accusation);
        let Some(signature) = self.sign(session_id.as_byte_slice(), PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1) else {
            return;
        };
        accusation.signature = signature;
        let object = PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) };
        if let Err(why) = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object) {
            warn!("[{PALW_PANEL}] IR claim {}: the accusation cannot ride a carrier ({why}); recorded, not filed", target.claim_id);
            return;
        }
        if books.court_pending.iter().any(|(sid, _, _, _)| *sid == session_id) {
            return;
        }
        info!(
            "[{PALW_PANEL}] IR claim {}: filing its one-move accusation ({label} close; first divergent leaf {leaf:?}, token row {row:?})",
            target.claim_id
        );
        books.court_due.insert((session_id, 0, false), due);
        books.court_pending.push((session_id, 0, false, object));
    }

    /// **Option B: one tick of the annex pursuit of `target`** — the seat's own run first (once: if it
    /// reproduces the claim, the claim is judged and nothing is asked), then the annex of the leaf the
    /// descent stands at: asked, re-asked on [`PALW_TIR_ANNEX_REASK_DAA_V1`], given up after
    /// [`PALW_TIR_ANNEX_ASKS_V1`] (the executor does not serve: option C's demand on chain is the path),
    /// and — once one verifies against the claim — stepped ([`palw_tir_annex_step_v1`]) to the next
    /// leaf or to the accusation, which is filed as the capture path files it.
    #[allow(clippy::too_many_arguments)]
    async fn tir_annex_pursuit_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        target: &PalwDisputableClaimV2,
        due: u64,
        tir: TirBackendV1,
        books: &mut PalwTirOneMoveBooksV1<'_>,
    ) {
        let claim = target.claim_id;
        let court = self.config.court;
        let network_ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
            &court,
            self.consensus_config.params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
        );
        let ladder = self.seat_refutation_ladder_v1(target.class_id, network_ladder, current_daa);
        let form = self.config.prompt_ids_form;
        let mut rules = tir.court_rules(&court);
        rules.max_step_leaf_count = ladder;
        let program = tir.class().program.clone();
        // 1. The seat's own run of the claim's job, once — and the first ask.
        if !books.pursuits.contains_key(&claim) {
            if books.pursuits.len() >= PALW_TIR_ANNEX_PURSUITS_V1 {
                return;
            }
            let Ok(backend) = self.resolve_backend(session, target.class_id, target.artifact_root) else { return };
            let Some((job, prompt)) = self.attempt_job_for_claim(
                session,
                backend.as_ref(),
                network_domain,
                target.accepted_block,
                target.class_id,
                &target.executor_bond,
            ) else {
                return;
            };
            drop(backend);
            let Ok(prompt) = prompt.into_iter().map(u32::try_from).collect::<Result<Vec<u32>, _>>() else { return };
            let Ok(Ok(own)) = tokio::task::spawn_blocking(move || tir.retain_memo(&job, &prompt)).await else { return };
            if own.binding.committed_execution_root == target.execution_root && own.binding.full_logits_trace_root == target.trace_root
            {
                books.challenged.insert(claim);
                return;
            }
            warn!(
                "[{PALW_PANEL}] IR claim {claim} committed an execution this node does not reproduce, and no capture of it reaches \
                 this seat — asking its executor for served annexes (RFC-0002 evidence transport B)"
            );
            books.pursuits.insert(claim, PalwTirAnnexPursuitV1::new(own, current_daa));
            self.request_leaf_evidence_v1(network_domain, claim, palw_tir_annex_request_index_v1(0), 0, current_daa).await;
            return;
        }
        let Some(pursuit) = books.pursuits.get(&claim).cloned() else { return };
        // 2. The annex of the leaf asked, verified against the claim.
        let index = palw_tir_annex_request_index_v1(pursuit.leaf);
        let held = books
            .openings
            .get(&(claim, index))
            .into_iter()
            .flatten()
            .filter_map(|bytes| misaka_palw_sdk::lineages::tir::PalwTirLeafAnnexV1::decode(bytes).ok())
            .filter(|annex| annex.leaf() == pursuit.leaf)
            .find_map(|annex| {
                misaka_palw_sdk::lineages::tir::palw_tir_leaf_annex_verify_v1(
                    &annex,
                    &program,
                    target.execution_root,
                    target.trace_root,
                    ladder,
                )
                .ok()
                .map(|binding| (annex, binding))
            });
        let Some((annex, binding)) = held else {
            if current_daa < pursuit.asked_daa.saturating_add(PALW_TIR_ANNEX_REASK_DAA_V1) {
                return;
            }
            if pursuit.asks >= PALW_TIR_ANNEX_ASKS_V1 {
                warn!(
                    "[{PALW_PANEL}] IR claim {claim}: its executor served no annex of leaf {} in {} asks — left to the on-chain \
                     demand (option C); this seat's replay withholds the licence",
                    pursuit.leaf, pursuit.asks
                );
                books.pursuits.remove(&claim);
                books.accused.insert(claim);
                return;
            }
            if let Some(p) = books.pursuits.get_mut(&claim) {
                p.asks += 1;
                p.asked_daa = current_daa;
            }
            self.request_leaf_evidence_v1(network_domain, claim, index, pursuit.leaf, current_daa).await;
            return;
        };
        // 3. The step, off the tick.
        let task = pursuit.clone();
        let Ok((tir, step)) = tokio::task::spawn_blocking(move || {
            let step = palw_tir_annex_step_v1(&tir, &task, &annex, &binding, &rules);
            (tir, step)
        })
        .await
        else {
            return;
        };
        drop(tir);
        match step {
            PalwTirAnnexStepV1::Ask { leaf, below, token } => {
                if let Some(p) = books.pursuits.get_mut(&claim) {
                    (p.leaf, p.below, p.token) = (leaf, below, token);
                    (p.asks, p.asked_daa, p.rounds) = (1, current_daa, p.rounds + 1);
                }
                self.request_leaf_evidence_v1(network_domain, claim, palw_tir_annex_request_index_v1(leaf), leaf, current_daa).await;
            }
            PalwTirAnnexStepV1::Stop(why) => {
                info!("[{PALW_PANEL}] IR claim {claim}: the annex pursuit ends without an accusation — {why}");
                books.pursuits.remove(&claim);
                books.accused.insert(claim);
            }
            PalwTirAnnexStepV1::Accuse { leaf, row, candidates } => {
                books.pursuits.remove(&claim);
                books.accused.insert(claim);
                let found = palw_tir_one_move_accusation_to_file_v1(candidates, target, &program, bond_key, &court, ladder, form);
                match found {
                    Some((label, accusation)) => {
                        info!(
                            "[{PALW_PANEL}] IR claim {claim}: the annexes name leaf {leaf:?} / token row {row:?} after {} round(s)",
                            pursuit.rounds + 1
                        );
                        self.file_tir_one_move_v1(target, due, label, leaf, row, accusation, books);
                    }
                    None => warn!(
                        "[{PALW_PANEL}] IR claim {claim}: no accusation built from the annexes convicts (leaf {leaf:?}, row {row:?}); \
                         recorded, not filed"
                    ),
                }
            }
        }
    }
}
