//! **ADR-0152 §4-ter N3 (A-held): the panel's held route — a held dissection's moves, built by the
//! windowed builders and answered by the node itself.**
//!
//! Past `palw_offence_attribution` a held class's fused leaf is disputed by a held dissection (the
//! fold's `open_held_dissection_v1`), and the fold clocks every move of it: an answerable held
//! class's silence is a default (C1). The dense route the panel runs for every other fused class
//! (`attn_site_evidence`) re-executes the job and holds every tile — refused past the
//! materialization cap, which the 8k row's canonical job is past — so a held class takes this route:
//!
//! * **the responder** (this node's bond PRODUCED the claim) builds N1
//!   (`attn_site_evidence_held_v1` with no filing) out of the claim's material — kept and verified,
//!   or re-made by replaying the claim's job, through P2-7's loader ([`palw_da_material_v1`]) — and
//!   files the root claim as `CourtAttnRootClaimedHeld` (tag 57, C2: with the anchor's slice
//!   sub-roots), then every round, then the close of its acquittal;
//! * **a challenger** (this node's bond opened the session) reads the accused's held filing off the
//!   chain ([`super::attn_held_filings_from_chain_v1`]), builds N2 from it and ONE honest replay of
//!   its own ([`palw_held_challenger_evidence_v1`], the entry a replay-mismatch hook calls with the
//!   claim's leaf), and files each round's choice and the bottom's close.
//!
//! **Off the tick** (the deadline design's U-D10). A held build is a forward to the anchor — about
//! 15–35 min and 1.5–2 GB at the peak at 8,191 positions of the 1.5B row (F8; T-A5 measures it) — so
//! it runs as a detached blocking task under the one memory ledger. The tick starts at most one at a
//! time, the soonest deadline first, polls it, and files the kernel moves (microseconds) off the
//! finished evidence; nothing else in the panel waits on it. A build that fails is retried a
//! re-plan later; one the ledger cannot cover now waits for the next tick.
//!
//! **Deadlines are the chain's.** A move is due by the deadline the session carries
//! (`PalwCourtDutyV2::rung_deadline_daa`, the fold's own `court_turn_and_rung_deadline_v2`: the
//! opening rung's for the root claim, the phase's clock after it; the session's backstop for a
//! close). Nothing here spells a turn length: the held compute turn (the review's F5,
//! `palw_held_move_turn_daa_v1` = max(the court's turn, `⌈2 ×` the class's reference replay `/ 120 s⌉`),
//! 58 DAA on the 8k row) is stamped by the fold on the moves whose builder re-executes the job — the
//! responder's root claim, and through the first disclosure the challenger's first choice — and every
//! response keeps the court's 42; the node reads whichever the session carries.
//!
//! **Carried like every court move**: queued on `court_pending` under `(session, round, role)`,
//! re-planned after `COURT_MOVE_REPLAN_DAA`, sent on the priority lane of the one carrier scheduler
//! (P2-6, `PalwCarrierSlotsV1`). Below the fence nothing here runs, and the dense route is byte for
//! byte what it was.

use super::*;
use kaspa_consensus_core::palw_attn_responder_v1::{PalwAttnHeldEvidenceV1, PalwAttnHeldFilingV1};
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2;
use kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2;

/// **A held dissection's move** (ADR-0093 as built, on the held route).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwHeldMoveV1 {
    /// The responder's move 1: `CourtAttnRootClaimedHeld` (tag 57).
    Root,
    /// The responder's disclosure of one round's children.
    Round,
    /// The challenger's choice of the child its own recompute does not reproduce.
    Choice,
    /// The bottom's close, filed by the party it acquits or convicts in favour of.
    Close,
}

/// **Is `duty` a held dissection's?** Past `palw_offence_attribution`, a session over a class the
/// chain records as held (`class_is_held_v1`), at a fused leaf the ladder already names — what
/// `open_held_dissection_v1` opens. Every other court duty keeps the route it had.
pub(crate) fn palw_held_route_v1(offence_attribution_active: bool, class_is_held: bool, duty: &PalwCourtDutyV2) -> bool {
    offence_attribution_active && class_is_held && duty.fused_class && duty.terminal_index.is_some()
}

/// **Which move `duty` asks of this node now** — the turn the chain's duty view reports (the fold's
/// own helper), never re-derived here. `None`: the other party's move.
pub(crate) fn palw_held_move_of_duty_v1(duty: &PalwCourtDutyV2) -> Option<PalwHeldMoveV1> {
    match (duty.dissection.as_ref(), duty.i_am_responder, duty.turn) {
        (None, true, PalwBisectTurnV1::AwaitDisclosure) => Some(PalwHeldMoveV1::Root),
        (Some(_), true, PalwBisectTurnV1::AwaitDisclosure) => Some(PalwHeldMoveV1::Round),
        (Some(_), false, PalwBisectTurnV1::AwaitVerdict) => Some(PalwHeldMoveV1::Choice),
        (Some(_), _, PalwBisectTurnV1::Terminal) => Some(PalwHeldMoveV1::Close),
        _ => None,
    }
}

/// **The DAA a move must land by, read off the session.** A clocked move's is the rung the session
/// carries (`rung_deadline_daa`); a close's is the session's backstop (`Terminal` is not clocked).
pub(crate) fn palw_held_move_deadline_v1(duty: &PalwCourtDutyV2, mv: PalwHeldMoveV1) -> u64 {
    match mv {
        PalwHeldMoveV1::Close => duty.session_deadline_daa,
        PalwHeldMoveV1::Root | PalwHeldMoveV1::Round | PalwHeldMoveV1::Choice => duty.rung_deadline_daa,
    }
}

/// **Can this party's evidence be built yet?** The responder's from the session's opening — its
/// root claim is the first move; a challenger's once the accused's filing is on chain, which is
/// exactly when a phase is open.
pub(crate) fn palw_held_evidence_buildable_v1(duty: &PalwCourtDutyV2) -> bool {
    duty.i_am_responder || duty.dissection.is_some()
}

/// **N1, on the node: the RESPONDER's evidence out of the claim's own material** — P2-7's loader
/// ([`palw_da_material_v1`]: a kept copy that reproduces the claim's roots, or one re-made by
/// replaying the claim's job and handed to `keep`), then the windowed responder
/// (`attn_site_evidence_held_v1`, no filing) on it with the prompt the material carries (a
/// free-prompt claim's ids; an attempt's is re-derived from its anchor). Returns the evidence and
/// whether the material was re-made. What the root claim, every round and the acquittal's close are
/// computed from.
pub(crate) fn palw_held_responder_evidence_v1(
    backend: &dyn PalwExecutionBackendV1,
    facts: &PalwDaClaimFactsV1,
    kept: impl IntoIterator<Item = Vec<u8>>,
    keep: impl FnOnce(&[u8]),
    narrowed: u64,
) -> Result<(PalwAttnHeldEvidenceV1, bool), String> {
    let (material, remade) = palw_da_material_v1(backend, facts, kept, keep)?;
    let (capture, carried): (&[u8], Option<&[u32]>) = match &material {
        PalwDaCaptureV1::FreePrompt(payload) => (&payload.capture, Some(&payload.material.prompt_token_ids)),
        PalwDaCaptureV1::Attempt(capture) => (capture, None),
    };
    backend
        .attn_site_evidence_held_v1(capture, narrowed, carried, None)
        .map(|evidence| (evidence, remade))
        .map_err(|e| format!("the windowed responder (N1): {e}"))
}

/// **N2, on the node: a CHALLENGER's evidence at `narrowed`** — the accused's held filing and ONE
/// honest replay of this node's own (`attn_site_evidence_held_v1` with the filing): the query row
/// streamed against the accused's step root, the state at the anchor this node's own. `carried` is
/// the claim's prompt for a free-prompt claim (`None` for an attempt, re-derived from the anchor).
///
/// **The entry a replay-mismatch hook calls** (Phase 2 P2-8e): once a seat's replay finds the first
/// divergent leaf of a held claim at a fused site and its accusation opens the held dissection, this
/// is the challenger's side for that claim and leaf — the panel's held route calls it the same way.
pub(crate) fn palw_held_challenger_evidence_v1(
    backend: &dyn PalwExecutionBackendV1,
    filing: &PalwAttnHeldFilingV1,
    narrowed: u64,
    carried: Option<&[u32]>,
) -> Result<PalwAttnHeldEvidenceV1, String> {
    backend.attn_site_evidence_held_v1(&[], narrowed, carried, Some(filing)).map_err(|e| format!("the windowed challenger (N2): {e}"))
}

/// **The accused's filing that stands for `duty`**: the first (oldest) held root claim whose binding
/// is the claim's execution and whose tile, at the narrowed leaf, proves against its step root at the
/// class's ladder — so a refused or foreign object is never the filing. `network_ladder` is the
/// ruleset's `max_step_leaf_count`, raised to the held ladder for a held profile.
pub(crate) fn palw_held_filing_of_duty_v1(
    filings: Vec<PalwAttnHeldFilingV1>,
    duty: &PalwCourtDutyV2,
    network_ladder: u64,
) -> Option<PalwAttnHeldFilingV1> {
    let narrowed = duty.terminal_index?;
    filings.into_iter().find(|filing| {
        filing.binding.committed_execution_root == duty.execution_root
            && filing.out_tile.opening.leaf_index == narrowed
            && kaspa_consensus_core::palw_step_leg::step_opening_root_capped_v1(
                filing.binding.step_leaf_count,
                &filing.out_tile.opening,
                kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(network_ladder, &filing.binding.shape_profile),
            )
            .is_ok_and(|root| root == filing.binding.step_merkle_root)
    })
}

/// **What a held move is built under, read off the node's params at the tick's DAA.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwHeldMoveCtxV1 {
    /// The class's artifact root, which the site's operand openings prove against.
    pub artifact_root: Hash64,
    /// The cap the court opens the site's rows under (`palw_attn_opening_cap_v1`): the claim's
    /// ladder under the held regime.
    pub opening_cap: u64,
    /// The dissection arity the acceptance layer derives (`palw_court_params_held_at_v2`); the root
    /// claim must declare exactly it.
    pub arity: u8,
}

/// **What building a move yielded.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwHeldMoveOutcomeV1 {
    /// The signed object to queue.
    File(PalwConsensusObjectV2),
    /// Nothing to file, and why: an honest disclosure (every child reproduces — silence lets the rung
    /// clock end an accusation the dissection does not support), or a bottom whose verdict is the
    /// other party's (a party closes only the case it wins).
    Nothing(&'static str),
}

/// **A held dissection's move, built from this party's evidence by the court's own kernels** and
/// signed under the move's context (`sign(message, context)`). `verdict_of` is the chain's dry run
/// of a close (`palw_court_close_verdict_v2`); a close is filed only when its verdict is this
/// party's. A challenger whose own sub-root of the site's layer is not the filed one cannot build
/// the bottom from the filing (4-ter.3 step 6) — that is an `Err` naming the slice, never a bottom.
pub(crate) fn palw_held_move_object_v1(
    evidence: &PalwAttnHeldEvidenceV1,
    duty: &PalwCourtDutyV2,
    mv: PalwHeldMoveV1,
    ctx: &PalwHeldMoveCtxV1,
    verdict_of: impl FnOnce(&PalwCourtVerdictProofV2) -> Option<PalwCourtVerdictV2>,
    mut sign: impl FnMut(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwHeldMoveOutcomeV1, String> {
    use kaspa_consensus_core::palw_attn_court_v1::{PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnDissectChoiceV1};
    use kaspa_consensus_core::palw_court_v2::{
        PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT, palw_attn_root_claim_message_v1,
        palw_attn_round_message_v1,
    };
    let site = evidence.site_v1(ctx.artifact_root, false, ctx.opening_cap).map_err(|e| format!("the held site: {e}"))?;
    let phase = || duty.dissection.as_ref().ok_or_else(|| "no dissection phase is open".to_string());
    let signed = |signature: Option<Vec<u8>>| signature.ok_or_else(|| "no signing key for a held move".to_string());
    match mv {
        PalwHeldMoveV1::Root => {
            let mut object =
                evidence.root_claim_held_v1(&site, duty.session_id, ctx.arity).map_err(|e| format!("the held root claim: {e}"))?;
            let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { root, signature, .. } = &mut object else {
                return Err("the held builder filed another form".to_string());
            };
            *signature =
                signed(sign(&palw_attn_root_claim_message_v1(&duty.session_id, root), PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT))?;
            Ok(PalwHeldMoveOutcomeV1::File(object))
        }
        PalwHeldMoveV1::Round => {
            let phase = phase()?;
            let round = evidence.round_v1(&site, phase).map_err(|e| format!("the held round: {e}"))?;
            let signature = signed(sign(
                &palw_attn_round_message_v1(&duty.session_id, phase.round(), &round),
                PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT,
            ))?;
            Ok(PalwHeldMoveOutcomeV1::File(PalwConsensusObjectV2::CourtAttnDissected {
                session_id: duty.session_id,
                round,
                signature,
            }))
        }
        PalwHeldMoveV1::Choice => {
            let phase = phase()?;
            let Some(child) = evidence.divergent_child_v1(&site, phase).map_err(|e| format!("the held children: {e}"))? else {
                return Ok(PalwHeldMoveOutcomeV1::Nothing("the responder's held disclosure reproduces — nothing to name"));
            };
            let choice = PalwAttnDissectChoiceV1 {
                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id: duty.session_id,
                round: phase.round(),
                child,
            };
            let message = borsh::to_vec(&choice).expect("a dissection choice is borsh-serializable");
            let signature = signed(sign(&message, PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT))?;
            Ok(PalwHeldMoveOutcomeV1::File(PalwConsensusObjectV2::CourtAttnChildChosen {
                session_id: duty.session_id,
                choice,
                signature,
            }))
        }
        PalwHeldMoveV1::Close => {
            let phase = phase()?;
            let anchored =
                evidence.site_v1(ctx.artifact_root, true, ctx.opening_cap).map_err(|e| format!("the anchored held site: {e}"))?;
            if !duty.i_am_responder
                && let Some(fallback) = evidence.fallback_v1(&anchored).map_err(|e| format!("the filed sub-roots: {e}"))?
            {
                return Err(format!(
                    "this node's sub-root of slice {} ({:?}, layer {}) is {} and the accused filed {}: its checkpoint disagrees with rows \
                     before its lie, so the bottom is not built from the filing (ADR-0152 §4-ter.3 step 6 — the held-DA StateChunk \
                     demand, then CheckpointAccused)",
                    fallback.slice, fallback.kind, fallback.layer, fallback.own, fallback.filed
                ));
            }
            let bottom = evidence.bottom_v1(&anchored, phase).map_err(|e| format!("the held bottom: {e}"))?;
            let proof = PalwCourtVerdictProofV2::AttnDissection {
                binding: Box::new(evidence.evidence.binding.clone()),
                bottom: Box::new(bottom),
                operand_openings: evidence.evidence.operand_openings.clone(),
            };
            let Some(verdict) = verdict_of(&proof) else {
                return Ok(PalwHeldMoveOutcomeV1::Nothing("the chain reads no verdict from this held bottom"));
            };
            let mine = if duty.i_am_responder {
                verdict == PalwCourtVerdictV2::ChallengerDefeated
            } else {
                verdict == PalwCourtVerdictV2::ExecutorGuilty
            };
            if !mine {
                return Ok(PalwHeldMoveOutcomeV1::Nothing("the held bottom's verdict is the other party's"));
            }
            Ok(PalwHeldMoveOutcomeV1::File(PalwConsensusObjectV2::CourtClosed { session_id: duty.session_id, verdict, proof }))
        }
    }
}

/// **The bytes a held evidence keeps resident while its session lives** — the anchor's chunks and
/// their leaf hashes, the site's K and V series and query slice, and the filed sub-roots: at the 8k
/// row's 8,192 positions ≈ 470 MB of state (28 layers × K and V × 8,192 × 1 KiB) plus ≈ 17 MB of
/// series; at its attempt's 1,025 positions ≈ 59 MB. Reserved on the one ledger for the evidence's
/// life (`"held-evidence"`), after the build's own reservation.
pub(crate) fn palw_held_evidence_resident_bytes_v1(evidence: &PalwAttnHeldEvidenceV1) -> u64 {
    let anchor = evidence.evidence.anchor.as_ref().map_or(0, |anchor| {
        let chunks: u64 = anchor.chunks.iter().map(|chunk| chunk.len() as u64).sum();
        chunks.saturating_add(anchor.chunk_hashes.len() as u64 * 64)
    });
    let inputs = &evidence.evidence.inputs;
    let series = (inputs.qh.len() + inputs.k_series.len() + inputs.v_series.len()) as u64 * 4;
    anchor.saturating_add(series).saturating_add(evidence.slice_sub_roots.len() as u64 * 64)
}

/// A built held evidence and what it cost.
struct PalwHeldBuiltV1 {
    evidence: PalwAttnHeldEvidenceV1,
    remade: bool,
    resident: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}

/// One party's held evidence for one session, as the tick sees it.
enum PalwHeldBuildV1 {
    /// Building off the tick since `since_daa`.
    Running { task: tokio::task::JoinHandle<Result<PalwHeldBuiltV1, String>>, since_daa: u64 },
    /// Built: every later move of the session reads it. The reservation lives as long as it does.
    Ready { evidence: Arc<PalwAttnHeldEvidenceV1>, _resident: Option<crate::palw_memory_ledger::PalwMemoryReservationV1> },
    /// Refused at `at_daa`: tried again a re-plan later (`COURT_MOVE_REPLAN_DAA`), never every tick —
    /// a material that did not verify, or a replay that did not reach the filed root, will not on
    /// the next tick either.
    Failed { at_daa: u64 },
}

/// **The held route's state across ticks** (the worker's, like `attn_evidence`): each party's
/// evidence per session, the accused filings read off the chain (with the DAA of the last look, so a
/// miss is retried on a throttle), the classes known held, and this tick's duties whose evidence is
/// wanted — for the one build the tick may start.
#[derive(Default)]
pub(crate) struct PalwHeldCourtV1 {
    builds: HashMap<(Hash64, bool), PalwHeldBuildV1>,
    filings: HashMap<Hash64, (u64, Option<PalwAttnHeldFilingV1>)>,
    held_classes: HashMap<Hash64, bool>,
    wanted: Vec<PalwCourtDutyV2>,
}

/// A missing filing is looked for again after this many DAA (the dense route's throttle).
const HELD_FILING_RELOOK_DAA: u64 = 25;

impl PalwHeldCourtV1 {
    /// **The tick begins**: what no open session names any more is dropped (a running build is
    /// detached — its task still returns its reservations when it ends), every finished build is
    /// collected, and last tick's wants are cleared.
    pub(crate) async fn begin_tick_v1(&mut self, court_duties: &[PalwCourtDutyV2], current_daa: u64) {
        self.builds.retain(|(session_id, _), _| court_duties.iter().any(|d| d.session_id == *session_id));
        self.filings.retain(|session_id, _| court_duties.iter().any(|d| d.session_id == *session_id));
        self.wanted.clear();
        let finished: Vec<(Hash64, bool)> = self
            .builds
            .iter()
            .filter(|(_, build)| matches!(build, PalwHeldBuildV1::Running { task, .. } if task.is_finished()))
            .map(|(key, _)| *key)
            .collect();
        for key in finished {
            let Some(PalwHeldBuildV1::Running { task, since_daa }) = self.builds.remove(&key) else { continue };
            let role = if key.1 { "responder" } else { "challenger" };
            let collected = match task.await {
                Ok(Ok(built)) => {
                    info!(
                        "[{PALW_PANEL}] session {}: the held evidence ({role}{}) is built — {} bytes resident, {} DAA after it started \
                         (ADR-0152 §4-ter N3)",
                        key.0,
                        if built.remade { ", from a re-made capture" } else { "" },
                        palw_held_evidence_resident_bytes_v1(&built.evidence),
                        current_daa.saturating_sub(since_daa)
                    );
                    PalwHeldBuildV1::Ready { evidence: Arc::new(built.evidence), _resident: built.resident }
                }
                Ok(Err(why)) => {
                    warn!("[{PALW_PANEL}] session {}: the held evidence ({role}) does not build: {why}", key.0);
                    PalwHeldBuildV1::Failed { at_daa: current_daa }
                }
                Err(e) => {
                    warn!("[{PALW_PANEL}] session {}: the held evidence task ({role}) did not finish: {e}", key.0);
                    PalwHeldBuildV1::Failed { at_daa: current_daa }
                }
            };
            self.builds.insert(key, collected);
        }
    }

    fn running(&self) -> bool {
        self.builds.values().any(|build| matches!(build, PalwHeldBuildV1::Running { .. }))
    }

    /// Note `duty` as wanting its evidence built, unless it is building, built, or failed inside a
    /// re-plan.
    fn want(&mut self, duty: &PalwCourtDutyV2, current_daa: u64) {
        let fresh = match self.builds.get(&(duty.session_id, duty.i_am_responder)) {
            None => true,
            Some(PalwHeldBuildV1::Failed { at_daa }) => current_daa >= at_daa.saturating_add(COURT_MOVE_REPLAN_DAA),
            Some(_) => false,
        };
        if fresh && palw_held_evidence_buildable_v1(duty) {
            self.wanted.push(duty.clone());
        }
    }
}

impl PalwPanelService {
    /// **Does the tick send `duty` down the held route?** [`palw_held_route_v1`] at the tick's DAA,
    /// with the class's held-ness read off the chain's class table (`PalwClassRowV2::held`, the
    /// fold's `class_is_held_v1`) once per class.
    pub(super) fn held_route_of_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &PalwCourtDutyV2,
        current_daa: u64,
        held: &mut PalwHeldCourtV1,
    ) -> bool {
        let fenced = self.consensus_config.params.palw_offence_attribution_active_at(current_daa);
        if !fenced || !duty.fused_class || duty.terminal_index.is_none() {
            return false;
        }
        let class_is_held = match held.held_classes.get(&duty.class_id) {
            Some(known) => *known,
            None => match session.palw_v2_class_table().into_iter().find(|row| row.class_id == duty.class_id) {
                Some(row) => *held.held_classes.entry(duty.class_id).or_insert(row.held),
                None => false,
            },
        };
        palw_held_route_v1(fenced, class_is_held, duty)
    }

    /// **The held route's move for `duty` this tick**: the signed object to queue, or the reason it
    /// waits (a stall line). Evidence not built yet is noted for [`Self::held_start_build_v1`]; a
    /// move past its on-chain deadline is not built (the sweep decides it).
    pub(super) fn held_move_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        duty: &PalwCourtDutyV2,
        current_daa: u64,
        held: &mut PalwHeldCourtV1,
    ) -> Result<PalwConsensusObjectV2, &'static str> {
        // The claim's material is this route's to keep while the session lives, from the first tick
        // (the P2-7 review's LOW: the end-of-tick pins are gone after a restart).
        self.foreign_pinned.lock().unwrap().insert(duty.claim_id);
        let evidence = match held.builds.get(&(duty.session_id, duty.i_am_responder)) {
            Some(PalwHeldBuildV1::Ready { evidence, .. }) => Some(evidence.clone()),
            Some(PalwHeldBuildV1::Running { .. }) => None,
            _ => {
                held.want(duty, current_daa);
                None
            }
        };
        let Some(mv) = palw_held_move_of_duty_v1(duty) else {
            return Err("waiting — the held dissection's move is the other party's");
        };
        let deadline = palw_held_move_deadline_v1(duty, mv);
        if current_daa > deadline {
            return Err("a held move past the deadline its session carries — the sweep decides it");
        }
        let Some(evidence) = evidence else {
            return Err(match held.builds.get(&(duty.session_id, duty.i_am_responder)) {
                Some(PalwHeldBuildV1::Running { .. }) => "building the held evidence (N1/N2) off the tick",
                Some(PalwHeldBuildV1::Failed { .. }) => "the held evidence did not build — tried again a re-plan later",
                _ if !duty.i_am_responder && duty.dissection.is_none() => "waiting for the accused's held root claim",
                _ => "the held evidence waits for the one held build",
            });
        };
        let params = &self.consensus_config.params;
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            return Err("a held dissection on a network with no V2 bundle");
        };
        // The arity the ACCEPTANCE layer derives (the processor's `palw_court_params_at`: the held
        // derivation where the held regime is in force), which the root claim must declare.
        let Ok(court) = kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2(
            bundle,
            params.palw_kary_court_active_at(current_daa),
            params.palw_held_context_active_at(current_daa),
        ) else {
            return Err("this ruleset's court has no shape for a dissection");
        };
        // The court opens the site's rows at the claim's ladder under the held regime
        // (`palw_attn_opening_cap_v1`), the structural `2^22` before it.
        let opening_cap = if params.palw_held_context_active_at(current_daa) {
            self.class_step_ladder(duty.class_id)
        } else {
            kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES
        };
        let ctx = PalwHeldMoveCtxV1 { artifact_root: duty.artifact_root, opening_cap, arity: court.dissection_arity() };
        match palw_held_move_object_v1(
            &evidence,
            duty,
            mv,
            &ctx,
            |proof| session.palw_court_close_verdict_v2(&duty.session_id, proof),
            |message, context| self.sign(message, context),
        ) {
            Ok(PalwHeldMoveOutcomeV1::File(object)) => {
                info!(
                    "[{PALW_PANEL}] session {}: filing the held dissection's {mv:?} for claim {} as {} — due by DAA {deadline} (now {current_daa}; \
                     ADR-0152 §4-ter N3)",
                    duty.session_id,
                    duty.claim_id,
                    if duty.i_am_responder { "responder" } else { "challenger" }
                );
                Ok(object)
            }
            Ok(PalwHeldMoveOutcomeV1::Nothing(why)) => Err(why),
            Err(why) => {
                warn!("[{PALW_PANEL}] session {}: the held dissection's {mv:?} does not build: {why}", duty.session_id);
                Err("a held move does not compute from the evidence")
            }
        }
    }

    /// **Start the tick's one held build** — the wanted duty whose deadline comes soonest, if no
    /// build is running: its party's material (the responder's kept or re-made capture, P2-7's
    /// loader) or filing (the challenger's, off the chain) gathered here, then the windowed builder
    /// run as a detached blocking task under the one memory ledger (`"held-dissection"`, a full
    /// seat's figure for the class, held for the build; `"held-evidence"`, the evidence's resident
    /// bytes, held for its life). A ledger refusal waits for the next tick; any other refusal is
    /// retried a re-plan later.
    pub(super) async fn held_start_build_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        network_domain: Hash64,
        current_daa: u64,
        held: &mut PalwHeldCourtV1,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
    ) {
        if held.running() {
            return;
        }
        // Soonest deadline first; a candidate that cannot start now (its filing not on chain yet, the
        // ledger full, a refusal) yields to the next, so no one session holds the others back.
        let mut wanted = std::mem::take(&mut held.wanted);
        wanted.sort_by_key(|duty| (duty.rung_deadline_daa, duty.session_id, duty.i_am_responder));
        wanted.dedup_by_key(|duty| (duty.session_id, duty.i_am_responder));
        for duty in wanted {
            if self.held_try_start_build_v1(session, network_domain, current_daa, held, materials, duty).await {
                return;
            }
        }
    }

    /// [`Self::held_start_build_v1`] for one wanted duty: `true` when its build is started.
    async fn held_try_start_build_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        network_domain: Hash64,
        current_daa: u64,
        held: &mut PalwHeldCourtV1,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        duty: PalwCourtDutyV2,
    ) -> bool {
        let key = (duty.session_id, duty.i_am_responder);
        let Some(narrowed) = duty.terminal_index else { return false };
        let failed = |held: &mut PalwHeldCourtV1| {
            held.builds.insert(key, PalwHeldBuildV1::Failed { at_daa: current_daa });
            false
        };
        // A fresh instance through the one resolve door: the build task owns it, and its walk and
        // state go when the build ends.
        let backend = match self.resolve_backend(session, duty.class_id, duty.artifact_root) {
            Ok(backend) => backend,
            Err(why) => {
                warn!("[{PALW_PANEL}] session {}: no backend for the held class: {why}", duty.session_id);
                return failed(held);
            }
        };
        let need = self.backends().role_memory_need_for_backend_or_chain_v1(
            backend.as_ref(),
            duty.class_id,
            duty.artifact_root,
            None,
            kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::FullSeat,
            |id| self.chain_carriage_v1(session, id),
        );
        let (class_id, session_id) = (duty.class_id, duty.session_id);
        let reserve_resident = move |evidence: &PalwAttnHeldEvidenceV1| {
            let bytes = palw_held_evidence_resident_bytes_v1(evidence);
            match crate::palw_memory_ledger::host_ledger_v1().reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 { role: "held-evidence", class_id, job: session_id },
                bytes,
            ) {
                Ok(reservation) => Some(reservation),
                Err(refusal) => {
                    // Already allocated: refusing to keep it would only make this party silent at a
                    // clocked move. Said, and kept unreserved.
                    warn!(
                        "[{PALW_PANEL}] session {session_id}: the held evidence's {bytes} resident bytes are kept unreserved: {refusal}"
                    );
                    None
                }
            }
        };
        let task = if duty.i_am_responder {
            let Some((_, _, work_leaves)) = session.palw_claim_roots_v2(duty.claim_id) else {
                warn!("[{PALW_PANEL}] session {}: the chain holds no claim {}", duty.session_id, duty.claim_id);
                return failed(held);
            };
            let lane = if duty.free_prompt {
                PalwDaLaneV1::FreePrompt { panel_da_admissible: self.consensus_config.params.palw_panel_da_admissible() }
            } else {
                let bond = &duty.executor_bond;
                PalwDaLaneV1::Attempt {
                    anchor: self
                        .job_anchor_for_claim(session, backend.as_ref(), network_domain, duty.accepted_block, duty.class_id, bond)
                        .unwrap_or_default(),
                    attempt_draw: self.attempt_draw_for_claim(session, duty.accepted_block),
                    job: self.attempt_job_for_claim(
                        session,
                        backend.as_ref(),
                        network_domain,
                        duty.accepted_block,
                        duty.class_id,
                        bond,
                    ),
                }
            };
            let facts = PalwDaClaimFactsV1 {
                claim_id: duty.claim_id,
                class_id: duty.class_id,
                executor_bond: duty.executor_bond,
                execution_root: duty.execution_root,
                trace_root: duty.trace_root,
                work_leaves,
                form: self.class_prompt_ids_form(duty.class_id),
                lane,
                // `None`, as every court re-make reads it (`PalwCourtDutyV2` carries no identity): the
                // claim's execution root commits its job's context, so no other job's capture
                // reproduces it (`verify_material`).
                job_pin: None,
            };
            let reserved = match self.reserve_replay_v1("held-dissection", &need, duty.class_id, duty.claim_id) {
                Ok(reserved) => reserved,
                Err(why) => {
                    crate::palw_backends::note_throttled_v1("panel-held-build-ledger", || {
                        format!("[{PALW_PANEL}] session {}: the held responder's build waits — {why}", duty.session_id)
                    });
                    return false;
                }
            };
            // Every copy this node may hold, the pool's first, then its own retention and `foreign/`.
            let dir = &self.config.retention_dir;
            let paths: Vec<PathBuf> = if duty.free_prompt {
                self.fp_retained_payload_paths(&duty.claim_id).to_vec()
            } else {
                vec![
                    crate::palw_producer::palw_retained_material_path(dir, &duty.claim_id),
                    dir.join("foreign").join(format!("{}.material", duty.claim_id)),
                ]
            };
            let pooled: Vec<Vec<u8>> = materials.get(&duty.claim_id).cloned().unwrap_or_default();
            let foreign = dir.join("foreign");
            let claim = duty.claim_id;
            tokio::task::spawn_blocking(move || {
                let _held_for_the_build = reserved;
                let kept = pooled.into_iter().chain(paths.iter().filter_map(|path| std::fs::read(path).ok()));
                let (evidence, remade) = palw_held_responder_evidence_v1(
                    backend.as_ref(),
                    &facts,
                    kept,
                    |bytes| palw_da_keep_remade_v1(&foreign, &claim, bytes),
                    narrowed,
                )?;
                let resident = reserve_resident(&evidence);
                Ok(PalwHeldBuiltV1 { evidence, remade, resident })
            })
        } else {
            let filing = match held.filings.get(&duty.session_id) {
                Some((_, Some(filing))) => Some(filing.clone()),
                Some((looked, None)) if current_daa < looked.saturating_add(HELD_FILING_RELOOK_DAA) => None,
                _ => {
                    let params = &self.consensus_config.params;
                    let (window, cap) = match &params.palw_consensus_mode {
                        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                            (bundle.state.window_court(), bundle.court.max_step_leaf_count())
                        }
                        _ => (0, 0),
                    };
                    let not_before = duty.session_deadline_daa.saturating_sub(window);
                    let span = current_daa.saturating_sub(not_before).saturating_add(64).min(1 << 16) as usize;
                    let sid = duty.session_id;
                    let filed = session
                        .clone()
                        .spawn_blocking(move |c| super::attn_held_filings_from_chain_v1(c, sid, not_before, span))
                        .await;
                    let filing = palw_held_filing_of_duty_v1(filed, &duty, cap);
                    held.filings.insert(duty.session_id, (current_daa, filing.clone()));
                    filing
                }
            };
            let Some(filing) = filing else {
                trace!("[{PALW_PANEL}] session {}: no held root claim of the accused's on chain yet", duty.session_id);
                return false;
            };
            // A free-prompt claim's prompt is the user's, out of the job material this seat holds.
            let carried: Option<Vec<u32>> = if duty.free_prompt {
                let pooled = materials.get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
                let prompt = self
                    .fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, pooled)
                    .and_then(|job| Self::fp_prompt_for_job(backend.as_ref(), &job, self.class_prompt_ids_form(duty.class_id)));
                match prompt {
                    Some(ids) => Some(ids),
                    None => {
                        warn!("[{PALW_PANEL}] session {}: the held challenger holds no job material of the claim", duty.session_id);
                        return failed(held);
                    }
                }
            } else {
                None
            };
            let reserved = match self.reserve_replay_v1("held-dissection", &need, duty.class_id, duty.claim_id) {
                Ok(reserved) => reserved,
                Err(why) => {
                    crate::palw_backends::note_throttled_v1("panel-held-build-ledger", || {
                        format!("[{PALW_PANEL}] session {}: the held challenger's build waits — {why}", duty.session_id)
                    });
                    return false;
                }
            };
            tokio::task::spawn_blocking(move || {
                let _held_for_the_build = reserved;
                let evidence = palw_held_challenger_evidence_v1(backend.as_ref(), &filing, narrowed, carried.as_deref())?;
                let resident = reserve_resident(&evidence);
                Ok(PalwHeldBuiltV1 { evidence, remade: false, resident })
            })
        };
        info!(
            "[{PALW_PANEL}] session {}: building the held evidence as {} off the tick — the next move is due by DAA {} \
             (ADR-0152 §4-ter N3)",
            duty.session_id,
            if duty.i_am_responder { "responder (N1)" } else { "challenger (N2)" },
            duty.rung_deadline_daa
        );
        held.builds.insert(key, PalwHeldBuildV1::Running { task, since_daa: current_daa });
        true
    }
}

#[cfg(test)]
mod tests {
    //! **ADR-0152 §4-ter T-A10 (the node): the held route is the one a held dissection takes past the
    //! fence, it files C2, and it reads every deadline off the session.** The route's end-to-end run
    //! against the fold is T-A9 (`held_court_e2e`).
    use super::*;
    use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
    use kaspa_consensus_core::network::{NetworkId, NetworkType};

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(n), 0))
    }

    fn duty(i_am_responder: bool, turn: PalwBisectTurnV1) -> PalwCourtDutyV2 {
        PalwCourtDutyV2 {
            accepted_block: h(1),
            session_id: h(2),
            claim_id: h(3),
            class_id: h(4),
            artifact_root: h(5),
            executor_bond: bond(6),
            challenger_bond: bond(7),
            i_am_responder,
            round: 0,
            interval: (40, 41),
            midpoint: None,
            terminal_index: Some(40),
            last_disclosure: None,
            turn,
            rung_deadline_daa: 1_058,
            session_deadline_daa: 4_000,
            trace_root: h(8),
            execution_root: h(9),
            free_prompt: false,
            fused_class: true,
            dissection: None,
            panel_seat_count: 2,
        }
    }

    /// **The route: past the fence, a held class, a fused leaf the ladder names — and nothing else.**
    #[test]
    fn a_held_dissection_takes_the_held_route_only_past_the_fence() {
        let d = duty(true, PalwBisectTurnV1::AwaitDisclosure);
        assert!(palw_held_route_v1(true, true, &d));
        assert!(!palw_held_route_v1(false, true, &d), "below palw_offence_attribution the dense route is untouched");
        assert!(!palw_held_route_v1(true, false, &d), "a class the chain does not record held keeps the dense route");
        assert!(!palw_held_route_v1(true, true, &PalwCourtDutyV2 { fused_class: false, ..d.clone() }), "no fused site, no dissection");
        assert!(!palw_held_route_v1(true, true, &PalwCourtDutyV2 { terminal_index: None, ..d }), "a ladder still walking");
    }

    /// **Whose move, and by when — off the duty the chain reports.** The responder's root claim before
    /// a phase, its rounds after; the challenger's choices; either party's close at `Terminal`; nothing
    /// on the other party's turn. The due DAA is the session's own field: a duty whose fold stamped the
    /// 8k row's 58-DAA compute turn (the review's F5) is due at 58, not at a 42 spelled here; a close is
    /// due by the backstop.
    #[test]
    fn the_move_and_its_deadline_are_read_off_the_session() {
        use kaspa_consensus_core::palw_attn_court_v1::PalwAttnDissectPhaseV1;
        let responder = duty(true, PalwBisectTurnV1::AwaitDisclosure);
        let challenger = duty(false, PalwBisectTurnV1::AwaitDisclosure);
        assert_eq!(palw_held_move_of_duty_v1(&responder), Some(PalwHeldMoveV1::Root));
        assert_eq!(palw_held_move_of_duty_v1(&challenger), None, "the root claim is the responder's");
        assert!(palw_held_evidence_buildable_v1(&responder) && !palw_held_evidence_buildable_v1(&challenger));
        // A phase, as the fold opens it (any valid one: only its presence is read here).
        let values = kaspa_consensus_core::palw_base0_a16::A16QuantParams { multiplier: 1, shift: 22, zero: -5 };
        let root = kaspa_consensus_core::palw_attn_dissect::PalwAttnRootClaimV1 {
            version: kaspa_consensus_core::palw_attn_dissect::PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            head: 0,
            lane_first: 0,
            lane_count: 1,
            history_positions: 8,
            claim: kaspa_consensus_core::palw_attn_dissect::PalwAttnRangeClaimV1 { max: 0, exp_sum: 20_000_000, v_acc: vec![0] },
        };
        let committed = kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(&root.claim.v_acc, values);
        let phase = PalwAttnDissectPhaseV1::open_with_arity(h(2), &root, (0, 0, 1), 8, &committed, values, 2, 4, 1_000, 58, true)
            .expect("a phase");
        let with_phase = |d: &PalwCourtDutyV2, turn| PalwCourtDutyV2 { dissection: Some(phase.clone()), turn, ..d.clone() };
        assert_eq!(palw_held_move_of_duty_v1(&with_phase(&responder, PalwBisectTurnV1::AwaitDisclosure)), Some(PalwHeldMoveV1::Round));
        assert_eq!(palw_held_move_of_duty_v1(&with_phase(&challenger, PalwBisectTurnV1::AwaitVerdict)), Some(PalwHeldMoveV1::Choice));
        assert_eq!(palw_held_move_of_duty_v1(&with_phase(&responder, PalwBisectTurnV1::AwaitVerdict)), None);
        assert_eq!(palw_held_move_of_duty_v1(&with_phase(&challenger, PalwBisectTurnV1::AwaitDisclosure)), None);
        for d in [&responder, &challenger] {
            assert_eq!(palw_held_move_of_duty_v1(&with_phase(d, PalwBisectTurnV1::Terminal)), Some(PalwHeldMoveV1::Close));
        }
        assert!(
            palw_held_evidence_buildable_v1(&with_phase(&challenger, PalwBisectTurnV1::AwaitDisclosure)),
            "once the filing is on chain"
        );
        assert_eq!(palw_held_move_deadline_v1(&responder, PalwHeldMoveV1::Root), 1_058, "the session's rung, whatever its length");
        assert_eq!(palw_held_move_deadline_v1(&responder, PalwHeldMoveV1::Round), 1_058);
        assert_eq!(palw_held_move_deadline_v1(&challenger, PalwHeldMoveV1::Choice), 1_058);
        assert_eq!(palw_held_move_deadline_v1(&responder, PalwHeldMoveV1::Close), 4_000, "a close by the backstop");
    }

    /// **The arity the held route declares is the acceptance layer's** (the processor's
    /// `palw_court_params_at`: `palw_court_params_held_at_v2` at the block's DAA, with the held
    /// regime), on testnet-12's own ruleset — a root claim declaring any other is refused.
    #[test]
    fn the_held_route_declares_the_arity_the_acceptance_layer_derives() {
        let t12 = palw_t12_shipped_params();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {
            panic!("testnet-12 is a V2 network")
        };
        assert!(t12.palw_offence_attribution_active_at(0) && t12.palw_held_context_active_at(0), "the held route's network");
        let court = kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2(bundle, t12.palw_kary_court_active_at(0), true)
            .expect("testnet-12 derives an arity");
        assert_eq!(court.dissection_arity(), 4, "testnet-12's held court: arity 4 (4-ter F2)");
        let source = include_str!("held_court.rs");
        let body = &source[source.find("    pub(super) fn held_move_v1(").expect("the tick's move")..];
        assert!(body.contains("palw_court_params_held_at_v2(\n            bundle,\n            params.palw_kary_court_active_at(current_daa),\n            params.palw_held_context_active_at(current_daa),"));
        let _ = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    }

    /// **T-A10, the panel half: the panel files C2.** In the panel's production code the tick
    /// routes a held dissection to this module BEFORE the dense builder is reached (and starts the
    /// tick's one held build after the loop); this module's route is fenced on
    /// `palw_offence_attribution`, builds its evidence through the windowed verb alone
    /// (`attn_site_evidence_held_v1`: N1 with no filing, N2 with the filing), files move 1 as the
    /// held form the builder makes (`root_claim_held_v1`, tag 57, signed under the responder's
    /// context over the root), reads the accused's filing — its slice sub-roots — off the chain
    /// beside `attn_root_filings_from_chain_v1`, and answers the responder's material through P2-7's
    /// loader. Red if any of it is re-spelled around.
    #[test]
    fn t_a10_the_panel_routes_a_held_dissection_to_the_windowed_builders_and_files_c2() {
        let panel = include_str!("../palw_panel.rs");
        let production = &panel[..panel.find("#[cfg(test)]\nmod tests {").expect("the unit tests follow the code")];
        let begin =
            production.find("held_court.begin_tick_v1(&court_duties, current_daa).await;").expect("the tick collects held builds");
        let route =
            production.find("if self.held_route_of_v1(&session, duty, current_daa, &mut held_court) {").expect("the held route");
        let moved =
            production[route..].find("self.held_move_v1(&session, duty, current_daa, &mut held_court)").expect("its move") + route;
        let queued = production[moved..]
            .find("Ok(object) => court_pending.push((duty.session_id, move_round, duty.i_am_responder, object)),")
            .expect("queued like every court move")
            + moved;
        let dense_resolve = production[route..]
            .find("let mut backend = match self.resolve_backend(&session, duty.class_id, duty.artifact_root)")
            .expect("the dense route's backend")
            + route;
        let dense = production.find("b.attn_site_evidence(").expect("the dense builder");
        let started = production
            .find("self.held_start_build_v1(&session, network_domain, current_daa, &mut held_court, &materials).await;")
            .expect("the one build");
        assert!(begin < route && route < moved && moved < queued, "collected, routed, built, queued");
        assert!(queued < dense_resolve && dense_resolve < dense, "a held dissection never reaches the dense builder");
        assert!(dense < started, "the tick's held build starts after the loop");
        assert!(production[queued..queued + 300].contains("continue;"), "the held route ends the duty's iteration");
        // The chain reader, beside the anchored form's.
        let dense_reader = production.find("fn attn_root_filings_from_chain_v1(").expect("the anchored reader");
        let held_reader = production.find("fn attn_held_filings_from_chain_v1(").expect("the held reader");
        assert!(held_reader > dense_reader, "beside it");
        assert!(
            production[held_reader..].contains("PalwAttnHeldFilingV1::from_object_v1(&object)"),
            "the sub-roots read off the object"
        );

        let module = include_str!("held_court.rs");
        let module = &module[..module.find("#[cfg(test)]\nmod tests {").expect("this module")];
        assert!(module.contains("offence_attribution_active && class_is_held && duty.fused_class && duty.terminal_index.is_some()"));
        assert!(
            module.contains("self.consensus_config.params.palw_offence_attribution_active_at(current_daa)"),
            "fenced at the tick's DAA"
        );
        assert!(!module.contains(".attn_site_evidence(") && !module.contains(".attn_site_evidence_from_filing("), "no dense builder");
        assert!(module.contains(".attn_site_evidence_held_v1(capture, narrowed, carried, None)"), "N1: the responder, no filing");
        assert!(
            module.contains(".attn_site_evidence_held_v1(&[], narrowed, carried, Some(filing))"),
            "N2: the challenger, the filing"
        );
        let root = &module[module.find("PalwHeldMoveV1::Root => {").expect("the root arm")..];
        let root = &root[..root.find("PalwHeldMoveV1::Round => {").expect("its end")];
        assert!(
            root.contains("evidence.root_claim_held_v1(&site, duty.session_id, ctx.arity)"),
            "C2: the held form the builder makes"
        );
        assert!(
            root.contains("palw_attn_root_claim_message_v1(&duty.session_id, root), PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT"),
            "signed as every root claim is"
        );
        assert!(
            !module.contains("CourtAttnRootClaimedAnchored {") && !module.contains("CourtAttnRootClaimed {"),
            "never the other forms"
        );
        assert!(
            module.contains("palw_held_responder_evidence_v1(\n                    backend.as_ref(),\n                    &facts,"),
            "P2-7's loader"
        );
        assert!(
            top_of(module, "pub(crate) fn palw_held_responder_evidence_v1(")
                .contains("palw_da_material_v1(backend, facts, kept, keep)?")
        );
        assert!(
            module.contains("super::attn_held_filings_from_chain_v1(c, sid, not_before, span)"),
            "the challenger's filing, off the chain"
        );
        assert!(
            module.contains("self.reserve_replay_v1(\"held-dissection\", &need, duty.class_id, duty.claim_id)"),
            "under the one ledger"
        );
        assert!(module.contains("tokio::task::spawn_blocking(move ||"), "off the tick");
    }

    fn top_of<'a>(source: &'a str, head: &str) -> &'a str {
        let at = source.find(head).unwrap_or_else(|| panic!("{head}"));
        &source[at..at + source[at..].find("\n}\n").expect("its end")]
    }
}
