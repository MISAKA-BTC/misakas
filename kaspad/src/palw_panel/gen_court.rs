//! **A tensor claim's seat and court on a node** (RFC-0003 §I.4, the carriage's node half) — dormant until the
//! free-prompt lane opens for pipeline classes ([`crate::palw_gen_seat::palw_gen_lane_open_v1`]:
//! `palw_fp_job_v5` over `palw_gen_v1`).
//!
//! A tensor claim's job is the USER's (a `PalwGenJobV1` at free-prompt job version 10), so its material is its
//! producer's capture ([`misaka_palw_base0::gen_tensor_worker::GenTensorCaptureV1`], served as `FPG1`): the job and
//! its ids and images, every leaf the run committed and the claimed canonical output, lies included.
//!
//! * **The seat** ([`PalwPanelService::gen_tensor_seat_pass_v1`]). The claim's own capture is the one whose
//!   rebuilt execution commits the claim's roots (cheap: it hashes the captured leaves, it runs nothing); the
//!   seat replays that capture's inputs over its held class off the loop, in the SEAT-R slots under its
//!   ledger reservation, and compares the replay's execution root, step root, leaf count and output root with the
//!   claim's — `Licensed` when they agree (a `Valid`), `Refuted` when they do not (terminal for `Valid`: the
//!   court's question, and a sampled verdict never slashes). The whole job is replayed: a tensor claim has no
//!   partial seat.
//! * **The court** ([`PalwPanelService::gen_one_move_pass_v1`]) where the held regime plays no bisection. For a
//!   claim this seat's replay refuted (or any licensed claim of a pipeline class with `--palw-challenge`): the
//!   accused capture rebuilt, this node's own run of the same inputs, the first leaf where their commitments
//!   part (or, where the steps agree, the first output tile that is not its own step tile's), the close the court
//!   convicts on there — checked as the chain derives it, from the class's row — signed over its session id and
//!   queued as a `GenShardCourtAccused`. A close that does not fit one carrier is recorded and not filed (the
//!   held leaf challenge, decision 22, files it once its fence is armed).
//!
//! **Logging (ADR-0079 SA-7):** nothing here logs prompt ids or image bytes; a refusal names the rule.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_v2::{PalwCourtVerdictProofV2, palw_gen_close_verdict_for_row_v1, palw_tir_court_limits_v1};
use kaspa_consensus_core::palw_gen_one_move_v1::{
    PALW_GEN_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1, PALW_GEN_ONE_MOVE_VERSION_V1, PalwGenOneMoveAccusationV1, palw_gen_one_move_session_id_v1,
};
use kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1;
use kaspa_consensus_core::palw_producer_v2::{PalwDisputableClaimV2, PalwSeatDutyV2};
use kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1;
use kaspa_consensus_core::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, PalwBondKeyV2, PalwConsensusObjectV2, PalwCourtVerdictV2};
use kaspa_core::{info, warn};
use misaka_palw_base0::gen_tensor_worker::{GenTensorCaptureV1, GenTensorWorkV1, gen_tensor_material_decode_v1};
use misaka_palw_base0::gen_worker::GenCourtMoveV1;
use misaka_palw_sdk::lineages::generative::GenBackendV1;

use super::{
    PALW_PANEL, PalwSeatRDutyV1, PalwSeatReplayPollV1, PalwSeatReplayStepV1, PalwSeatReplaysV1, palw_seat_replay_step_v1,
};

/// **Is this a TENSOR class** — an image or an embedding: its claims are `PalwGenJobV1`s on the free-prompt lane
/// (job version 10) and its material is `FPG1`. A text pipeline class's claims are FP Job V4/V5's, which this
/// module does not touch.
pub(super) fn gen_class_is_tensor_v1(tensor: &GenBackendV1) -> bool {
    let profile = tensor.entry().row.profile;
    profile == PalwGenProfileV1::Image as u8 || profile == PalwGenProfileV1::Embedding as u8
}

/// A payload's identity, for the replay slots' key: a keyed digest of its bytes (two payloads of one job with
/// different leaves are two replays' worth of questions).
fn payload_key_v1(bytes: &[u8]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka-node/tensor-tensor-payload/v1").to_state();
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(bytes);
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **The captures a seat holds for a claim** — each pooled or retained payload that decodes as `FPG1`, speaks about
/// the claim's class and executor, and is distinct from the ones before it: `(key, capture, bytes)`.
fn captures_of_v1(
    duty_class: Hash64,
    duty_executor: &PalwBondKeyV2,
    payloads: impl IntoIterator<Item = Vec<u8>>,
) -> Vec<(Hash64, GenTensorCaptureV1, Vec<u8>)> {
    let mut out: Vec<(Hash64, GenTensorCaptureV1, Vec<u8>)> = Vec::new();
    for bytes in payloads {
        let Some(capture) = gen_tensor_material_decode_v1(&bytes) else { continue };
        if capture.job.envelope.class_id != duty_class || capture.job.envelope.executor_bond != duty_executor.0 {
            continue;
        }
        let key = payload_key_v1(&bytes);
        if out.iter().any(|(k, _, _)| *k == key) {
            continue;
        }
        out.push((key, capture, bytes));
    }
    out
}

/// **Does this capture rebuild to the claim's own roots?** — the rebuilt execution's tensor execution root, step
/// root, leaf count and output root are the claim's. Cheap (it hashes the captured leaves and runs nothing), and
/// the test a payload is the claim's capture and not a stranger's.
fn capture_answers_for_v1(tensor: &GenBackendV1, capture: &GenTensorCaptureV1, execution_root: Hash64, trace_root: Hash64, output_root: Hash64, work_leaves: u64) -> Option<GenTensorWorkV1> {
    let work = tensor.rebuild(capture).ok()?;
    (work.execution_root() == execution_root
        && work.binding.step_root() == trace_root
        && work.binding.output_root == output_root
        && work.binding.step_leaf_count == work_leaves)
        .then_some(work)
}

/// What one tick of a tensor claim's seat pass came to.
pub(super) struct PalwGenSeatPassV1 {
    pub step: PalwSeatReplayStepV1,
    /// The payload a `Licensed` step keeps (the claim's own capture, which a replay just reproduced).
    pub kept: Option<Vec<u8>>,
    /// Whether this seat holds a capture of the claim's class and executor at all (N-5: a seat that was served
    /// something accuses nobody for silence).
    pub served: bool,
}

impl super::PalwPanelService {
    /// **A tensor claim's seat, one tick** (see the module doc). The replay runs off the loop in the SEAT-R slots;
    /// while it runs the seat files nothing.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn gen_tensor_seat_pass_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        tensor: Arc<GenBackendV1>,
        duty: &PalwSeatDutyV2,
        seat_r_duty: PalwSeatRDutyV1,
        current_daa: u64,
        pooled: &[Vec<u8>],
        replays: &mut PalwSeatReplaysV1,
    ) -> PalwGenSeatPassV1 {
        let disk = self.fp_retained_payload_paths(&duty.claim_id).into_iter().filter_map(|path| std::fs::read(path).ok());
        let held = captures_of_v1(duty.class_id, &duty.executor_bond, pooled.iter().cloned().chain(disk));
        let served = !held.is_empty();
        let mut waiting = false;
        for (key_digest, capture, bytes) in held {
            let key = (duty.claim_id, key_digest);
            // Whose it is, asked once per payload.
            let own = match replays.own_job(&key, 1) {
                Some(answer) => answer,
                None => {
                    let own = capture_answers_for_v1(&tensor, &capture, duty.execution_root, duty.trace_root, duty.output_root, duty.work_leaves)
                        .is_some();
                    replays.note_own_job(key, own, 1);
                    own
                }
            };
            if !own {
                continue;
            }
            let mut poll = replays.poll(&key, current_daa).await;
            if let PalwSeatReplayPollV1::Done { result: Err(e), fresh } = &poll
                && replays.retry_refused(&key, current_daa, seat_r_duty.deadline)
            {
                if *fresh {
                    warn!(
                        "[{PALW_PANEL}] tensor replay for claim {} refused: {e} — starting it once more (SEAT-R)",
                        duty.claim_id
                    );
                }
                poll = PalwSeatReplayPollV1::Absent;
            }
            match poll {
                PalwSeatReplayPollV1::Running => waiting = true,
                PalwSeatReplayPollV1::Done { result: Ok(replayed), fresh } => {
                    let step = palw_seat_replay_step_v1(&Ok(replayed.clone()), duty.execution_root, duty.trace_root, duty.work_leaves, duty.output_root);
                    match step {
                        PalwSeatReplayStepV1::Licensed => {
                            if fresh {
                                info!(
                                    "[{PALW_PANEL}] tensor claim {}: licensed by replay — the job reproduces the claim's roots and \
                                     canonical output at its price ({:?} leaves) (SEAT-R)",
                                    duty.claim_id, replayed.work_leaves
                                );
                            }
                            return PalwGenSeatPassV1 { step: PalwSeatReplayStepV1::Licensed, kept: Some(bytes), served };
                        }
                        PalwSeatReplayStepV1::Refuted => {
                            if fresh {
                                warn!(
                                    "[{PALW_PANEL}] tensor claim {}: its own capture's replay does not reproduce the claim's roots \
                                     (execution {} vs {}, trace {} vs {}, output {:?} vs {}, leaves {:?} vs priced {}) — nothing \
                                     licenses the claim here",
                                    duty.claim_id,
                                    replayed.execution_root,
                                    duty.execution_root,
                                    replayed.trace_root,
                                    duty.trace_root,
                                    replayed.output_root,
                                    duty.output_root,
                                    replayed.work_leaves,
                                    duty.work_leaves
                                );
                            }
                            return PalwGenSeatPassV1 { step: PalwSeatReplayStepV1::Refuted, kept: None, served };
                        }
                        PalwSeatReplayStepV1::Waiting | PalwSeatReplayStepV1::NoVerdict => {}
                    }
                }
                PalwSeatReplayPollV1::Done { result: Err(e), fresh } => {
                    if fresh {
                        warn!("[{PALW_PANEL}] tensor replay for claim {} refused: {e} (SEAT-R)", duty.claim_id);
                    }
                }
                PalwSeatReplayPollV1::Absent => {
                    if !replays.fits(&duty.class_id, current_daa, seat_r_duty.deadline) {
                        crate::palw_backends::note_throttled_v1("panel-replay-late", || {
                            format!(
                                "[{PALW_PANEL}] replay of tensor claim {} not started: this host's last replay of its class would return \
                                 after the receipt deadline (DAA {}, SEAT-R)",
                                duty.claim_id, seat_r_duty.deadline
                            )
                        });
                        continue;
                    }
                    waiting = true;
                    if !replays.has_room(seat_r_duty.heavy) {
                        crate::palw_backends::note_throttled_v1("panel-replay-room", || {
                            format!("[{PALW_PANEL}] replay of tensor claim {} waits: every replay slot is taken", duty.claim_id)
                        });
                        continue;
                    }
                    let need = self.backends().role_memory_need_for_backend_or_chain_v1(
                        tensor.as_ref(),
                        duty.class_id,
                        duty.artifact_root,
                        None,
                        PalwResourceRoleV1::FullSeat,
                        |id| self.chain_carriage_v1(session, id),
                    );
                    let reserved = match self.reserve_replay_v1("full-seat", &need, duty.class_id, duty.claim_id) {
                        Ok(reserved) => reserved,
                        Err(why) => {
                            crate::palw_backends::note_throttled_v1("panel-replay-ledger", || {
                                format!("[{PALW_PANEL}] replay of tensor claim {} deferred: {why}", duty.claim_id)
                            });
                            continue;
                        }
                    };
                    info!(
                        "[{PALW_PANEL}] tensor claim {}: replaying its job off the loop for a verdict (deadline DAA {}, SEAT-R)",
                        duty.claim_id, seat_r_duty.deadline
                    );
                    let backend = tensor.clone();
                    replays.start_task(key, duty.class_id, seat_r_duty.heavy, current_daa, Some(reserved), move || {
                        backend.replay_roots(&capture)
                    });
                }
            }
        }
        PalwGenSeatPassV1 { step: if waiting { PalwSeatReplayStepV1::Waiting } else { PalwSeatReplayStepV1::NoVerdict }, kept: None, served }
    }

    /// **The tensor one-move pass — a pipeline claim's court where the held regime plays no bisection** (see the
    /// module doc). Once per claim: a reproduced claim is judged, an accused one (or one no close convicts) is
    /// not tried again.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn gen_one_move_pass_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        bond_key: PalwBondKeyV2,
        network_domain: Hash64,
        current_daa: u64,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        seat_faulted: &HashSet<Hash64>,
        replay_refuted: &HashSet<Hash64>,
        books: PalwGenOneMoveBooksV1<'_>,
    ) {
        let mut targets: Vec<(PalwDisputableClaimV2, u64)> = session
            .palw_disputable_claims_v2(vec![bond_key])
            .into_iter()
            .filter(|t| self.config.challenge || seat_faulted.contains(&t.claim_id))
            .map(|t| {
                let due = super::tir_court::palw_tir_one_move_due_v1(&self.consensus_config.params, t.licensed_daa, current_daa);
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
                targets.push((super::tir_court::palw_tir_duty_target_v1(&duty), due));
            }
        }
        for (target, due) in targets {
            if books.challenged.contains(&target.claim_id) || books.accused.contains(&target.claim_id) {
                continue;
            }
            // Only a tensor claim of a pipeline class this node holds is tried here.
            if !target.free_prompt {
                continue;
            }
            let tensor = match self.backends().resolve_gen_v1(target.class_id, target.artifact_root) {
                None => continue,
                Some(Ok(tensor)) if gen_class_is_tensor_v1(&tensor) => Arc::new(tensor),
                Some(Ok(_)) => continue,
                Some(Err(why)) => {
                    crate::palw_backends::note_throttled_v1("tensor-one-move-backend", || {
                        format!("[{PALW_PANEL}] claim {}: the generative backend does not build for its class ({why})", target.claim_id)
                    });
                    continue;
                }
            };
            // The accused capture: a served payload that rebuilds to the claim's own roots — the close is an
            // assertion about the ACCUSED's leaves (audit3 S-01).
            let pool = materials.get(&target.claim_id).map(|v| v.to_vec()).unwrap_or_default();
            let disk = self.fp_retained_payload_paths(&target.claim_id).into_iter().filter_map(|path| std::fs::read(path).ok());
            let accused_capture = captures_of_v1(target.class_id, &target.executor_bond, pool.into_iter().chain(disk))
                .into_iter()
                .find(|(_, capture, _)| {
                    tensor.rebuild(capture)
                        .is_ok_and(|w| w.execution_root() == target.execution_root && w.binding.step_root() == target.trace_root)
                })
                .map(|(_, capture, _)| capture);
            let Some(capture) = accused_capture else { continue };
            // This node's own run of the same inputs, and where the two executions part — off the loop.
            let court = self.config.court;
            let limits = palw_tir_court_limits_v1(&court);
            let form = tensor.prompt_ids_form();
            let row = tensor.entry().row.clone();
            let facts = target.clone();
            let backend = tensor.clone();
            let need = self.backends().role_memory_need_for_backend_or_chain_v1(
                tensor.as_ref(),
                target.class_id,
                target.artifact_root,
                None,
                PalwResourceRoleV1::FullSeat,
                |id| self.chain_carriage_v1(session, id),
            );
            let reservation = match self.reserve_replay_v1("court", &need, target.class_id, target.claim_id) {
                Ok(reservation) => reservation,
                Err(why) => {
                    crate::palw_backends::note_throttled_v1("tensor-one-move-ledger", || {
                        format!("[{PALW_PANEL}] tensor claim {}: its court replay is deferred: {why}", target.claim_id)
                    });
                    continue;
                }
            };
            let Ok(found) = tokio::task::spawn_blocking(move || {
                let _held_for_the_replay = reservation;
                let accused = backend.rebuild(&capture)?;
                let own = backend.replay(&capture)?;
                if own.execution_root() == accused.execution_root() {
                    return Ok::<_, String>((None, None));
                }
                // The first leaf where the commitments part; where the steps agree, the first output tile that is
                // not its own step tile's.
                let (leaf, label_hint) = match backend.first_divergence(&accused, &own) {
                    Some(leaf) => (leaf, "first divergent leaf"),
                    None => match backend.output_audit(&accused) {
                        Some((_, global, _)) => (global, "output audit"),
                        None => return Ok((None, None)),
                    },
                };
                let candidates = backend.court_candidates(&accused, leaf, true, &limits);
                let mut filed = None;
                for (label, built) in candidates {
                    let Ok(GenCourtMoveV1::Close(proof)) = built else { continue };
                    let verdict = palw_gen_close_verdict_for_row_v1(
                        &row,
                        &facts.class_id,
                        &facts.execution_root,
                        &proof,
                        None,
                        &court,
                        form,
                        false,
                    );
                    if verdict == Ok(PalwCourtVerdictV2::ExecutorGuilty) {
                        filed = Some((label, proof));
                        break;
                    }
                }
                Ok((Some((leaf, label_hint)), filed))
            })
            .await
            else {
                continue;
            };
            // Tried once.
            books.accused.insert(target.claim_id);
            let (found_at, filed) = match found {
                Ok(pair) => pair,
                Err(why) => {
                    warn!("[{PALW_PANEL}] tensor claim {}: its court case does not build ({why}); recorded, not filed", target.claim_id);
                    continue;
                }
            };
            let Some((leaf, hint)) = found_at else {
                // The two executions agree: the claim reproduced.
                books.challenged.insert(target.claim_id);
                books.accused.remove(&target.claim_id);
                continue;
            };
            let Some((label, proof)) = filed else {
                warn!(
                    "[{PALW_PANEL}] tensor claim {}: no close convicts at leaf {leaf} ({hint}); recorded, not filed",
                    target.claim_id
                );
                continue;
            };
            let mut accusation = PalwGenOneMoveAccusationV1 {
                version: PALW_GEN_ONE_MOVE_VERSION_V1,
                claim: target.claim_id,
                execution_root: target.execution_root,
                trace_root: target.trace_root,
                executor_bond: target.executor_bond,
                accuser_bond: bond_key,
                verdict: PalwCourtVerdictV2::ExecutorGuilty,
                proof,
                signature: Vec::new(),
            };
            let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                self.consensus_config.params.net.to_string().as_bytes(),
                Some(self.consensus_config.genesis.hash),
            );
            let session_id = palw_gen_one_move_session_id_v1(domain.as_byte_slice(), &accusation);
            let Some(signature) = self.sign(session_id.as_byte_slice(), PALW_GEN_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1) else { continue };
            accusation.signature = signature;
            let object = PalwConsensusObjectV2::GenShardCourtAccused { accusation: Box::new(accusation) };
            // One carrier: a close past it is the held leaf challenge's (decision 22), filed once its fence is armed.
            let bytes = borsh::to_vec(&object).map(|b| b.len()).unwrap_or(usize::MAX);
            if bytes > PALW_OBJECT_CHUNK_MAX_BYTES {
                warn!(
                    "[{PALW_PANEL}] tensor claim {}: its {label} close is {bytes} bytes, past one carrier ({PALW_OBJECT_CHUNK_MAX_BYTES}); \
                     recorded, not filed — the held leaf challenge (palw_held_close_chunks_v1, decision 22) carries it",
                    target.claim_id
                );
                continue;
            }
            if let Err(why) = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object) {
                warn!("[{PALW_PANEL}] tensor claim {}: the accusation cannot ride a carrier ({why}); recorded, not filed", target.claim_id);
                continue;
            }
            if books.court_pending.iter().any(|(sid, _, _, _)| *sid == session_id) {
                continue;
            }
            info!(
                "[{PALW_PANEL}] tensor claim {}: filing its one-move accusation ({label} close at leaf {leaf}, {bytes} bytes)",
                target.claim_id
            );
            books.court_due.insert((session_id, 0, false), due);
            books.court_pending.push((session_id, 0, false, object));
        }
    }
}

/// What the tensor one-move pass reads and writes of the panel loop's bookkeeping (the IR pass's own).
pub(super) struct PalwGenOneMoveBooksV1<'a> {
    /// Claims this node has judged and reproduced.
    pub challenged: &'a mut HashSet<Hash64>,
    /// Claims this node has accused in one move (or found no accusation for): filed once.
    pub accused: &'a mut HashSet<Hash64>,
    pub court_pending: &'a mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    pub court_due: &'a mut HashMap<(Hash64, u32, bool), u64>,
}

#[allow(dead_code)]
fn proof_kind_v1(proof: &PalwCourtVerdictProofV2) -> &'static str {
    match proof {
        PalwCourtVerdictProofV2::GenCone { .. } => "cone",
        PalwCourtVerdictProofV2::GenOutputTile { .. } => "output",
        PalwCourtVerdictProofV2::GenDecodeToken { .. } => "decode token",
        _ => "other",
    }
}
