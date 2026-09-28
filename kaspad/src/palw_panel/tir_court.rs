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
use kaspa_core::{info, warn};
use misaka_palw_sdk::lineages::tir::{TirBackendV1, TirCaptureV1};

use super::PALW_PANEL;

/// A close a party may file, built or refused, under the label the log names it by.
pub(crate) type PalwTirCloseCandidateV1 = (&'static str, Result<PalwCourtVerdictProofV2, String>);

/// The IR closes a party may file at leaf `index`, in the order it tries them, each built or refused.
pub(crate) fn palw_tir_close_candidates_v1(
    tir: &TirBackendV1,
    accused: &[u8],
    own: Option<&[u8]>,
    index: u64,
    rules: &PalwTirCourtRulesV1,
    challenger: bool,
) -> Vec<PalwTirCloseCandidateV1> {
    let mut out = vec![("cone", tir.cone_close(accused, index, rules))];
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
    out.push(("logits", tir.logits_close(accused, index)));
    let Some(row) = (leaf.position + 1).checked_sub(ctx.declared_prefill_tokens) else { return out };
    let own_token = own.and_then(|own| TirCaptureV1::decode(own).ok()).and_then(|mine| mine.generated.get(row as usize).copied());
    let committed = capture.generated.get(row as usize).copied();
    if let Some(beat) = own_token.filter(|t| Some(*t) != committed) {
        out.push(("decode token", tir.decode_token_close(accused, row, beat)));
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
        let door = tir.decode_token_close(accused, r, o.generated[r as usize]);
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
            candidates.push(("logits", tir.logits_close(accused, tile.index)));
        }
    }
    if leaf.is_none() && row.is_none() && candidates.is_empty() {
        return Ok(None);
    }
    Ok(Some(PalwTirOneMoveCaseV1 { leaf, row, candidates }))
}

/// **The verdict an IR close proof supports against a claim, as far as a node derives it without
/// the chain's state** — the one-move gate's own derivation (`palw_tir_one_move_verdict_v1` →
/// `adjudicate_close_proof_v2`'s IR arm) with the state's two reads supplied by the claim's facts
/// the node already holds (its class and that class's artifact root, from the disputable-claim
/// view): the proof within the court's byte ceiling, its binding naming the claim's class, artifact
/// root and roots, then the IR court at the chain's ladder, prompt form and work limits. `None`
/// where the proof does not adjudicate. The gate re-derives it and refuses a mismatch, so a wrong
/// answer here costs a refused carrier, never a wrong verdict.
pub(crate) fn palw_tir_one_move_verdict_stateless_v1(
    proof: &PalwCourtVerdictProofV2,
    target: &PalwDisputableClaimV2,
    court: &PalwCourtParamsV2,
    step_ladder: u64,
    prompt_form: PalwPromptIdsFormV1,
) -> Option<PalwCourtVerdictV2> {
    use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
    use kaspa_consensus_core::palw_tir_court_v1 as tir;
    kaspa_consensus_core::palw_court_v2::check_close_cost_v2(proof, court).ok()?;
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
    accuser: PalwBondKeyV2,
    court: &PalwCourtParamsV2,
    step_ladder: u64,
    prompt_form: PalwPromptIdsFormV1,
) -> Option<(&'static str, PalwTirOneMoveAccusationV1)> {
    candidates.into_iter().find_map(|(label, built)| {
        let proof = built.ok()?;
        (palw_tir_one_move_verdict_stateless_v1(&proof, target, court, step_ladder, prompt_form)
            == Some(PalwCourtVerdictV2::ExecutorGuilty))
        .then(|| {
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

/// What the IR one-move pass reads and writes of the panel loop's bookkeeping.
pub(super) struct PalwTirOneMoveBooksV1<'a> {
    /// Claims this node has judged and reproduced.
    pub challenged: &'a mut HashSet<Hash64>,
    /// Claims this node has accused in one move (or found no accusation for): filed once.
    pub accused: &'a mut HashSet<Hash64>,
    pub court_pending: &'a mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    pub court_due: &'a mut HashMap<(Hash64, u32, bool), u64>,
}

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
        books: PalwTirOneMoveBooksV1<'_>,
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
                let case = palw_tir_one_move_case_v1(&tir, &accused, &own, &rules)?;
                Ok::<_, String>(case.and_then(|case| {
                    let (leaf, row) = (case.leaf, case.row);
                    palw_tir_one_move_accusation_to_file_v1(case.candidates, &facts, bond_key, &court, ladder, form)
                        .map(|(label, accusation)| (leaf, row, label, accusation))
                }))
            })
            .await
            else {
                continue;
            };
            // Tried once: a claim no IR close convicts is recorded, not filed.
            books.accused.insert(target.claim_id);
            let (leaf, row, label, mut accusation) = match found {
                Ok(Some(found)) => found,
                Ok(None) => {
                    warn!(
                        "[{PALW_PANEL}] IR claim {}: no IR close convicts where this node's execution parts from it; recorded, not filed",
                        target.claim_id
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
            let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                self.consensus_config.params.net.to_string().as_bytes(),
                Some(self.consensus_config.genesis.hash),
            );
            let session_id = palw_tir_one_move_session_id_v1(domain.as_byte_slice(), &accusation);
            let Some(signature) = self.sign(session_id.as_byte_slice(), PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1) else {
                continue;
            };
            accusation.signature = signature;
            let object = PalwConsensusObjectV2::TirShardCourtAccused { accusation: Box::new(accusation) };
            if let Err(why) = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object) {
                warn!(
                    "[{PALW_PANEL}] IR claim {}: the accusation cannot ride a carrier ({why}); recorded, not filed",
                    target.claim_id
                );
                continue;
            }
            if books.court_pending.iter().any(|(sid, _, _, _)| *sid == session_id) {
                continue;
            }
            info!(
                "[{PALW_PANEL}] IR claim {}: filing its one-move accusation ({label} close; first divergent leaf {leaf:?}, token row {row:?})",
                target.claim_id
            );
            books.court_due.insert((session_id, 0, false), due);
            books.court_pending.push((session_id, 0, false, object));
        }
    }
}
