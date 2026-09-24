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
//! * **a challenger** (this node's bond opened the session) reads the accused's held root claims
//!   back off the chain and keeps the first that passes the fold's own checks
//!   ([`palw_held_filing_of_duty_v1`]: a decoy the fold refused never poisons it, and a filing N2
//!   failed on is evicted), builds N2 from it and ONE honest replay of its own
//!   ([`palw_held_challenger_evidence_v1`], the entry a replay-mismatch hook calls with the claim's
//!   leaf), and files each round's choice and the bottom's close;
//! * **4-ter.3 step 6**: where the challenger's own sub-root of the disputed layer's slice is not the
//!   one the accused filed, the bottom cannot be built from the filing — the node demands the
//!   disputed tile's chunk through the held DA court (`DefaultAccusedHeld`, `StateChunk`), and once
//!   the producer discloses it either bottoms on the disclosed path (the bytes are the honest ones)
//!   or files `CheckpointAccused` on a disclosed row that is not the row the accused's step tree
//!   committed (C3 admits it while the session is open). The producer's silence is DA-7's default.
//!
//! **Off the tick** (the deadline design's U-D10). A held build is a forward to the anchor — about
//! 15–35 min and 1.5–2 GB at the peak at 8,191 positions of the 1.5B row (F8; T-A5 measures it) — so
//! it runs as a detached blocking task under the one memory ledger. Builds run in PARALLEL as far as
//! the ledger admits (at most [`PALW_HELD_BUILDS_MAX_V1`]), the responder's first — its silence is a
//! default, a challenger's only its own loss — then the soonest deadline; one party's evidence is
//! keyed by `(claim, narrowed leaf, role)`, so sessions at one leaf share it. A build is reserved as a
//! full seat for its life (`"held-dissection"`) and the finished evidence for its own
//! (`"held-evidence"`); evidence the ledger will not hold is dropped and rebuilt later, never kept
//! unreserved. Evidence is kept for [`PALW_HELD_EVIDENCE_GRACE_DAA_V1`] after the last duty named
//! it, so a transiently empty duty set or a reorg does not discard a build.
//!
//! **Deadlines are the chain's.** A move is due by the deadline the session carries
//! (`PalwCourtDutyV2::rung_deadline_daa`, the fold's own `court_turn_and_rung_deadline_v2`: the
//! opening rung's for the root claim, the phase's clock after it; the session's backstop for a
//! close). Nothing here spells a turn length: the held compute turn (the review's F5,
//! `palw_held_move_turn_daa_v1` = max(the court's turn, `⌈2 ×` the class's reference replay `/ 120 s⌉`),
//! 58 DAA on the 8k row) is stamped by the fold on the moves whose builder re-executes the job — the
//! responder's root claim, and through the first disclosure the challenger's first choice — and every
//! response keeps the court's 42; the node reads whichever the session carries. Every item this
//! module queues carries its due DAA, which the priority lane orders by (`palw_court_queue_edf_v1`).
//!
//! **Carried like every court move**: queued on `court_pending` under `(session, round, role)`,
//! re-planned after `COURT_MOVE_REPLAN_DAA`, sent on the priority lane of the one carrier scheduler
//! (P2-6, `PalwCarrierSlotsV1`). The node's side is behind [`PalwHeldHostV1`], so the whole tick —
//! want, start, poll, move, queue — runs against a fixture in the tests. Below the fence nothing
//! here runs, and the dense route is byte for byte what it was.

use super::*;
use kaspa_consensus_core::palw_attn_court_v1::{
    PalwAttnChunkOpeningV1, PalwAttnDissectBottomV1, PalwAttnRowOpeningV1, PalwAttnTileEvidenceV1,
};
use kaspa_consensus_core::palw_attn_responder_v1::{PalwAttnHeldEvidenceV1, PalwAttnHeldFilingV1};
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_v2::{PalwAttnDisputeSiteV2, PalwCourtVerdictProofV2};
use kaspa_consensus_core::palw_producer_v2::PalwCourtDutyV2;
use kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1;
use kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2;

/// **One party's evidence is a function of the claim, the narrowed leaf and the role alone** — the
/// key a build is kept under, so sessions at one leaf share one build.
pub(crate) type PalwHeldEvidenceKeyV1 = (Hash64, u64, bool);

/// A `court_pending` key: `(session or claim, round, role)`.
pub(crate) type PalwCourtQueueKeyV1 = (Hash64, u32, bool);

/// **How many held builds run at once, at most** — the memory ledger is the bound that matters; this
/// keeps a host with a large ledger from starting more whole-context forwards than it has cores.
pub(crate) const PALW_HELD_BUILDS_MAX_V1: usize = 4;

/// **How long evidence outlives the last duty that named it** (the review's LOW): a transiently empty
/// duty set, a reorg that unwinds a session's opening and replays it — neither throws a 15–35 minute
/// build away. Long past any held session's backstop, so nothing stale is answered from.
pub(crate) const PALW_HELD_EVIDENCE_GRACE_DAA_V1: u64 = 200;

/// A chain read that found nothing is repeated after this many DAA (the dense route's throttle).
const HELD_CHAIN_RELOOK_DAA: u64 = 25;

/// **A step-6 demand's answer is looked for every this many DAA**, from the demand's own DAA on (a
/// short walk): once disclosed, the claim's clock runs again, and the accusation must land inside it.
const HELD_DISCLOSURE_RELOOK_DAA: u64 = 2;

/// **Step 6's queue rounds** (4-ter.3): the demand of the disputed tile's chunk (one per kind, `| 0`
/// K and `| 1` V) and the checkpoint accusation, keyed under the session beside its moves — below
/// the dissection's rounds (`1 << 30`, `1 << 31`), so the priority lane's EDF, which keeps one
/// session's items in round order, never puts a demand behind its own session's close.
pub(crate) const PALW_HELD_STEP6_DEMAND_ROUND_V1: u32 = 1 << 29;
pub(crate) const PALW_HELD_STEP6_ACCUSE_ROUND_V1: u32 = (1 << 29) | 0x100;

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

/// **The duty as it reads once `choice` lands, if that makes the phase terminal** — the fold's own
/// `apply_choice` on a copy of the phase: the tile the bottom is, known the moment the challenger
/// names it (the node's step 6 must not wait to see `Terminal`: the other party may close first).
pub(crate) fn palw_held_terminal_duty_after_v1(
    duty: &PalwCourtDutyV2,
    choice: &kaspa_consensus_core::palw_attn_court_v1::PalwAttnDissectChoiceV1,
) -> Option<PalwCourtDutyV2> {
    let mut phase = duty.dissection.clone()?;
    phase.apply_choice(choice, duty.rung_deadline_daa, 1).ok()?;
    (phase.turn() == PalwBisectTurnV1::Terminal).then(|| PalwCourtDutyV2 {
        turn: PalwBisectTurnV1::Terminal,
        dissection: Some(phase),
        ..duty.clone()
    })
}

/// **Can this party's evidence be built yet?** The responder's from the session's opening — its
/// root claim is the first move; a challenger's once the accused's filing is on chain, which is
/// exactly when a phase is open.
pub(crate) fn palw_held_evidence_buildable_v1(duty: &PalwCourtDutyV2) -> bool {
    duty.i_am_responder || duty.dissection.is_some()
}

/// The evidence key of `duty`: its claim, the leaf its ladder narrowed to, and this node's role.
pub(crate) fn palw_held_evidence_key_v1(duty: &PalwCourtDutyV2) -> Option<PalwHeldEvidenceKeyV1> {
    duty.terminal_index.map(|leaf| (duty.claim_id, leaf, duty.i_am_responder))
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

/// A held root claim's node-local identity (the eviction set's key): a digest of its encoding.
pub(crate) fn palw_held_object_digest_v1(object: &PalwConsensusObjectV2) -> Hash64 {
    let bytes = borsh::to_vec(object).unwrap_or_default();
    let digest = blake2b_simd::Params::new().hash_length(64).key(b"misaka-node/held-filing/v1").hash(&bytes);
    Hash64::from_bytes(digest.as_bytes().try_into().expect("64 bytes"))
}

/// **The accused's filing that stands for `duty`** (the feat/t12-aheld-node review, HIGH): every held
/// root claim the chain holds for the session, OLDEST first, each checked the way the fold checked
/// it (`PalwAttnHeldFilingV1::from_object_checked_v1`: the binding the claim's execution, re-derived;
/// the anchor the site's checkpoint; the sub-roots folding to the anchor's state; the tile opening
/// at the narrowed leaf), less any this node's N2 already failed on (`rejected`). The chain walk
/// returns objects on accepted carriers that the fold refused, so a decoy filed first — the genuine
/// object with one sub-root swapped — is skipped, not cached. Returns the filing and its digest.
pub(crate) fn palw_held_filing_of_duty_v1(
    candidates: &[PalwConsensusObjectV2],
    duty: &PalwCourtDutyV2,
    opening_cap: u64,
    rejected: &HashSet<Hash64>,
) -> Option<(Hash64, PalwAttnHeldFilingV1)> {
    let narrowed = duty.terminal_index?;
    candidates.iter().find_map(|object| {
        let digest = palw_held_object_digest_v1(object);
        if rejected.contains(&digest) {
            return None;
        }
        PalwAttnHeldFilingV1::from_object_checked_v1(
            object,
            &duty.session_id,
            duty.execution_root,
            duty.class_id,
            duty.artifact_root,
            narrowed,
            opening_cap,
        )
        .ok()
        .map(|filing| (digest, filing))
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
/// the bottom from the filing — that is 4-ter.3 step 6 ([`palw_held_step6_units_v1`]), never a
/// bottom here.
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
            palw_held_close_object_v1(evidence, duty, bottom, verdict_of)
        }
    }
}

/// **A bottom, as the close this party files** — the chain's dry run first; filed only when the
/// verdict is this party's (the responder's acquittal, the challenger's conviction).
fn palw_held_close_object_v1(
    evidence: &PalwAttnHeldEvidenceV1,
    duty: &PalwCourtDutyV2,
    bottom: PalwAttnDissectBottomV1,
    verdict_of: impl FnOnce(&PalwCourtVerdictProofV2) -> Option<PalwCourtVerdictV2>,
) -> Result<PalwHeldMoveOutcomeV1, String> {
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

/// **The bytes a held evidence keeps resident while its session lives** — the anchor's chunks and
/// their leaf hashes, the site's K and V series and query slice, and the filed sub-roots: at the 8k
/// row's 8,192 positions ≈ 470 MB of state (28 layers × K and V × 8,192 × 1 KiB) plus ≈ 17 MB of
/// series; at its attempt's 1,025 positions ≈ 59 MB. Reserved on the one ledger for the evidence's
/// life (`"held-evidence"`), once the build's own reservation is released.
pub(crate) fn palw_held_evidence_resident_bytes_v1(evidence: &PalwAttnHeldEvidenceV1) -> u64 {
    let anchor = evidence.evidence.anchor.as_ref().map_or(0, |anchor| {
        let chunks: u64 = anchor.chunks.iter().map(|chunk| chunk.len() as u64).sum();
        chunks.saturating_add(anchor.chunk_hashes.len() as u64 * 64)
    });
    let inputs = &evidence.evidence.inputs;
    let series = (inputs.qh.len() + inputs.k_series.len() + inputs.v_series.len()) as u64 * 4;
    anchor.saturating_add(series).saturating_add(evidence.slice_sub_roots.len() as u64 * 64)
}

// ---- ADR-0152 §4-ter.3 step 6: the StateChunk fallback --------------------------------------------

/// **One chunk of the disputed tile the step-6 path demands**: the checkpoint the filed anchor is
/// (its leaf's index), the chunk's flat index in that checkpoint's held layout, and its slice's kind
/// and layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwHeldStep6UnitV1 {
    pub kind: PalwStateChunkKindV1,
    pub layer: u16,
    pub checkpoint: u32,
    pub chunk_index: u32,
}

impl std::hash::Hash for PalwHeldStep6UnitV1 {
    fn hash<S: std::hash::Hasher>(&self, state: &mut S) {
        (self.kind_code(), self.layer, self.checkpoint, self.chunk_index).hash(state);
    }
}

impl PalwHeldStep6UnitV1 {
    fn kind_code(&self) -> u8 {
        match self.kind {
            PalwStateChunkKindV1::Key => 0,
            PalwStateChunkKindV1::Value => 1,
        }
    }

    /// The demand's queue key: under the session, one per kind.
    pub(crate) fn demand_key_v1(&self, session_id: Hash64) -> PalwCourtQueueKeyV1 {
        (session_id, PALW_HELD_STEP6_DEMAND_ROUND_V1 | u32::from(self.kind_code()), false)
    }
}

/// **Does the bottom need step 6, and for which chunks?** At the terminal tile of `phase`, the
/// chunks of layer ℓ's K and V the bottom reads (the evidence's checkpoint route: one chunk a kind)
/// whose slice's sub-root THIS node computes is not the one the accused filed (4-ter.3 step 6). Empty
/// when the bottom can be built from the filing. `anchored` is the site derived with the anchor.
pub(crate) fn palw_held_step6_units_v1(
    evidence: &PalwAttnHeldEvidenceV1,
    anchored: &PalwAttnDisputeSiteV2,
    phase: &kaspa_consensus_core::palw_attn_court_v1::PalwAttnDissectPhaseV1,
) -> Result<Vec<PalwHeldStep6UnitV1>, String> {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    if evidence.fallback_v1(anchored).map_err(|e| format!("the filed sub-roots: {e}"))?.is_none() {
        return Ok(Vec::new());
    }
    let anchor = evidence.evidence.anchor.as_ref().ok_or("a held site's evidence carries its anchor")?;
    let layout = map::palw_state_layout_v4(&evidence.evidence.binding.shape_profile, anchored.site.anchor_positions)
        .map_err(|e| format!("the anchor's held layout: {e:?}"))?;
    let own =
        map::palw_state_slice_sub_roots_v4(&layout, &anchor.chunk_hashes).map_err(|e| format!("the state's sub-roots: {e:?}"))?;
    // The plain bottom names the chunk a kind the tile reads (the held one only re-paths it).
    let bottom = evidence.evidence.bottom_v1(anchored, phase).map_err(|e| format!("the bottom's chunks: {e}"))?;
    let mut units = Vec::new();
    for (tile, kind) in [(&bottom.k, PalwStateChunkKindV1::Key), (&bottom.v, PalwStateChunkKindV1::Value)] {
        let PalwAttnTileEvidenceV1::Checkpoint { chunk, .. } = tile else {
            return Err("a held bottom reads its history out of the anchor".to_string());
        };
        let slice = layout.address(u64::from(chunk.chunk_index)).ok_or("the chunk's address")?.slice as usize;
        if own.get(slice) != evidence.slice_sub_roots.get(slice) {
            units.push(PalwHeldStep6UnitV1 {
                kind,
                layer: anchored.site.attn_layer,
                checkpoint: anchor.anchor.leaf.checkpoint_index,
                chunk_index: chunk.chunk_index,
            });
        }
    }
    Ok(units)
}

/// **The demand of one chunk** (4-ter.3 step 6, F15): `DefaultAccusedHeld` naming
/// `StateChunk { checkpoint, chunk }` of the accused's `binding`, signed by this node's bond over the
/// network domain (`palw_held_da_accusation_message_v1`). The producer must put the chunk and its
/// path on chain within the disclose window, or DA-7 defaults it (S1) and the void closes the
/// dissection neutrally.
pub(crate) fn palw_held_step6_demand_v1(
    duty: &PalwCourtDutyV2,
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    unit: &PalwHeldStep6UnitV1,
    accuser: PalwBondKeyV2,
    network_domain: Hash64,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    use kaspa_consensus_core::palw_held_da_v1::{
        PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT, PALW_HELD_DA_VERSION_V1, PalwHeldAccusationV1, PalwHeldMissingV1,
        palw_held_da_accusation_message_v1,
    };
    let mut demand = PalwHeldAccusationV1 {
        version: PALW_HELD_DA_VERSION_V1,
        claim: duty.claim_id,
        missing: PalwHeldMissingV1::StateChunk { checkpoint: unit.checkpoint, chunk: unit.chunk_index },
        accuser,
        binding: binding.clone(),
        signature: Vec::new(),
    };
    let message = palw_held_da_accusation_message_v1(network_domain.as_byte_slice(), &demand);
    demand.signature = sign(message.as_byte_slice(), PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT).ok_or("no signing key for a held demand")?;
    Ok(PalwConsensusObjectV2::DefaultAccusedHeld { accusation: Box::new(demand) })
}

/// **The producer's answer to a step-6 demand, found on chain and checked** — R-core+'s
/// `MaterialDisclosedV2` or the v1 court's `MaterialDisclosedHeld` for `unit` of `duty`'s claim,
/// whose anchor is the filed one and whose chunk, at the unit's flat index, folds through its own
/// path to the filed anchor's committed state root under the class's map. An object the fold refused
/// rides an accepted carrier too, so nothing is taken on its word.
pub(crate) fn palw_held_step6_disclosed_v1(
    objects: &[PalwConsensusObjectV2],
    duty: &PalwCourtDutyV2,
    filing: &PalwAttnHeldFilingV1,
    unit: &PalwHeldStep6UnitV1,
    anchor_positions: u32,
) -> Option<PalwAttnChunkOpeningV1> {
    use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
    use kaspa_consensus_core::palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1};
    let named = PalwHeldMissingV1::StateChunk { checkpoint: unit.checkpoint, chunk: unit.chunk_index };
    let profile = &filing.binding.shape_profile;
    objects.iter().find_map(|object| {
        let carriage = match object {
            PalwConsensusObjectV2::MaterialDisclosedV2 {
                claim,
                unit: PalwDaUnitV1::Held(missing),
                answer: PalwDaAnswerV1::Held(carriage),
                ..
            } if *claim == duty.claim_id && *missing == named => carriage,
            PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure }
                if disclosure.claim == duty.claim_id && disclosure.missing == named =>
            {
                disclosure
            }
            _ => return None,
        };
        let PalwHeldDisclosureV1::StateChunk { anchor, chunk } = &carriage.disclosure else { return None };
        if anchor.leaf != filing.anchor.leaf || chunk.chunk_index != unit.chunk_index {
            return None;
        }
        let leaf = kaspa_consensus_core::palw_state_chunk_map::palw_state_chunk_leaf_for_map_v1(
            profile,
            anchor_positions,
            chunk.chunk_index,
            &chunk.chunk_bytes,
        )?;
        let folded = kaspa_consensus_core::palw_state_chunk_map::palw_state_chunk_membership_root_v1(
            profile,
            anchor_positions,
            chunk.chunk_index,
            &leaf,
            &chunk.siblings,
        )
        .ok()?;
        (folded == filing.anchor.leaf.state_chunks_root).then(|| chunk.clone())
    })
}

/// **What the disclosed chunks decide** (4-ter.3 step 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwHeldStep6NextV1 {
    /// Not every demanded chunk is on chain yet.
    Pending,
    /// Every disclosed chunk holds this node's honest bytes: the bottom, each such chunk re-pathed
    /// through the producer's own disclosed path — which reaches the anchor the forger committed.
    Bottom(PalwAttnDissectBottomV1),
    /// A disclosed row is not this node's honest row: the checkpoint court tries it against the
    /// row the accused's step tree committed at `position`.
    Accuse { unit: PalwHeldStep6UnitV1, position: u32, chunk: PalwAttnChunkOpeningV1 },
}

/// **What to do with the disclosures** — for each demanded unit its disclosed chunk (checked,
/// [`palw_held_step6_disclosed_v1`]) against this node's own bytes of that chunk in `evidence`'s
/// anchor state: the first differing row (position-major) is an accusation; all equal, the held
/// bottom with the disclosed paths in place of the filed top path.
pub(crate) fn palw_held_step6_next_v1(
    evidence: &PalwAttnHeldEvidenceV1,
    anchored: &PalwAttnDisputeSiteV2,
    phase: &kaspa_consensus_core::palw_attn_court_v1::PalwAttnDissectPhaseV1,
    units: &[PalwHeldStep6UnitV1],
    disclosed: &HashMap<PalwHeldStep6UnitV1, PalwAttnChunkOpeningV1>,
) -> Result<PalwHeldStep6NextV1, String> {
    use kaspa_consensus_core::palw_state_chunk_map as map;
    let anchor = evidence.evidence.anchor.as_ref().ok_or("a held site's evidence carries its anchor")?;
    let layout = map::palw_state_layout_v4(&evidence.evidence.binding.shape_profile, anchored.site.anchor_positions)
        .map_err(|e| format!("the anchor's held layout: {e:?}"))?;
    let mut all = true;
    for unit in units {
        let Some(chunk) = disclosed.get(unit) else {
            all = false;
            continue;
        };
        let own = anchor.chunks.get(unit.chunk_index as usize).ok_or("the chunk outside this node's anchor state")?;
        if chunk.chunk_bytes == *own {
            continue;
        }
        let entry = map::integer_kv_state_chunk_entry_v1(&layout.attn, u64::from(unit.chunk_index)).ok_or("the chunk's entry")?;
        for position in entry.position_start..entry.position_start + entry.position_count {
            let disclosed_row = map::integer_kv_state_row_v1(&entry, &chunk.chunk_bytes, position);
            if disclosed_row != map::integer_kv_state_row_v1(&entry, own, position) {
                return Ok(PalwHeldStep6NextV1::Accuse { unit: *unit, position, chunk: chunk.clone() });
            }
        }
        return Err("the disclosed chunk differs from this node's and no row of it does".to_string());
    }
    if !all {
        return Ok(PalwHeldStep6NextV1::Pending);
    }
    let mut bottom = evidence.bottom_v1(anchored, phase).map_err(|e| format!("the held bottom: {e}"))?;
    for tile in [&mut bottom.k, &mut bottom.v] {
        if let PalwAttnTileEvidenceV1::Checkpoint { chunk, .. } = tile
            && let Some(opening) = disclosed.iter().find(|(unit, _)| unit.chunk_index == chunk.chunk_index).map(|(_, c)| c)
        {
            chunk.siblings = opening.siblings.clone();
        }
    }
    Ok(PalwHeldStep6NextV1::Bottom(bottom))
}

/// **The checkpoint accusation** (ADR-0103 Decision 1, under C3 while the session is open): the
/// disclosed `chunk` of the filed anchor, the accused `position` of `unit`'s series, and `rows` — the
/// accused's committed cache-write row there ([`PalwExecutionBackendV1::attn_held_cache_write_rows_v1`]),
/// dry-run through the court's own verdict at the class's ladder first (`ExecutorGuilty`, or nothing
/// is filed) and signed over its session id under the checkpoint court's context.
#[allow(clippy::too_many_arguments)]
pub(crate) fn palw_held_step6_accusation_v1(
    duty: &PalwCourtDutyV2,
    filing: &PalwAttnHeldFilingV1,
    unit: &PalwHeldStep6UnitV1,
    position: u32,
    chunk: PalwAttnChunkOpeningV1,
    rows: Vec<PalwAttnRowOpeningV1>,
    accuser: PalwBondKeyV2,
    network_domain: Hash64,
    ladder: u64,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    use kaspa_consensus_core::palw_checkpoint_court_v1::{
        PALW_CHECKPOINT_COURT_MLDSA87_ACCUSE_CONTEXT, PALW_CHECKPOINT_COURT_VERSION_V1, PalwCheckpointAccusationV1,
        PalwCheckpointCourtVerdictV1, palw_checkpoint_court_session_id_v1, palw_checkpoint_court_verdict_v1,
    };
    let mut accusation = PalwCheckpointAccusationV1 {
        version: PALW_CHECKPOINT_COURT_VERSION_V1,
        claim: duty.claim_id,
        execution_root: duty.execution_root,
        trace_root: duty.trace_root,
        executor_bond: duty.executor_bond,
        accuser_bond: accuser,
        binding: filing.binding.clone(),
        anchor: filing.anchor.clone(),
        chunk,
        kind: unit.kind_code(),
        attn_layer: unit.layer,
        position,
        rows,
        signature: Vec::new(),
    };
    match palw_checkpoint_court_verdict_v1(&accusation, duty.class_id, ladder) {
        Ok(PalwCheckpointCourtVerdictV1::ExecutorGuilty) => {}
        Ok(PalwCheckpointCourtVerdictV1::FalseAccusation) => {
            return Err("the checkpoint court reads the disclosed row as the committed one".to_string());
        }
        Err(e) => return Err(format!("the checkpoint accusation does not adjudicate: {e}")),
    }
    let session = palw_checkpoint_court_session_id_v1(network_domain.as_byte_slice(), &accusation);
    accusation.signature =
        sign(session.as_byte_slice(), PALW_CHECKPOINT_COURT_MLDSA87_ACCUSE_CONTEXT).ok_or("no signing key for an accusation")?;
    Ok(PalwConsensusObjectV2::CheckpointAccused { accusation: Box::new(accusation) })
}

// ---- the node's side, and the tick ----------------------------------------------------------------

/// **What the held route needs of the node** — its chain facts at the tick, its key, its ledger and
/// its backends. The panel implements it over its session ([`PalwPanelHeldHostV1`]); a test over a
/// fixture chain, so the whole tick runs without a node.
pub(crate) trait PalwHeldHostV1 {
    /// `palw_offence_attribution` at `daa`.
    fn offence_attribution_active(&self, daa: u64) -> bool;
    /// The class's held-ness off the chain's class table (`PalwClassRowV2::held`); `None` for a class
    /// the chain does not name.
    fn class_is_held(&self, class_id: &Hash64) -> Option<bool>;
    /// The cap the court opens a site's rows under at `daa` (`palw_attn_opening_cap_v1`).
    fn opening_cap(&self, class_id: &Hash64, daa: u64) -> u64;
    /// The dissection arity the acceptance layer derives at `daa`.
    fn arity(&self, daa: u64) -> Option<u8>;
    /// The held DA court's disclose window (`W_disclose`).
    fn disclose_window_daa(&self) -> u64;
    /// The court window a session's backstop is its opening plus.
    fn window_court(&self) -> u64;
    fn network_domain(&self) -> Hash64;
    /// This node's bond — the accuser of a step-6 demand or accusation.
    fn bond(&self) -> PalwBondKeyV2;
    /// The chain's dry run of a close (`palw_court_close_verdict_v2`).
    fn verdict_of(&self, session_id: &Hash64, proof: &PalwCourtVerdictProofV2) -> Option<PalwCourtVerdictV2>;
    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>>;
    /// Keep `claim`'s material while its session lives.
    fn pin(&self, claim: Hash64);
    /// The one memory ledger.
    fn ledger(&self) -> Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>;
    /// A fresh backend for the duty's class, through the node's one resolve door.
    fn backend(&self, duty: &PalwCourtDutyV2) -> Result<Box<dyn PalwExecutionBackendV1>, String>;
    /// What a build of the duty's class reserves: a full seat's figure.
    fn build_need_bytes(&self, backend: &dyn PalwExecutionBackendV1, duty: &PalwCourtDutyV2) -> u64;
    /// The responder's material for P2-7's loader: the claim's facts and every copy it may hold.
    fn responder_material(&self, duty: &PalwCourtDutyV2, backend: &dyn PalwExecutionBackendV1) -> Result<PalwHeldMaterialV1, String>;
    /// The claim's prompt for a free-prompt claim (the user's, out of the job this node holds);
    /// `None` for an attempt, whose prompt is re-derived from its anchor.
    fn carried_prompt(&self, duty: &PalwCourtDutyV2, backend: &dyn PalwExecutionBackendV1) -> Result<Option<Vec<u32>>, String>;
    /// **Whether `claim_id` can still be convicted, and until when** — `Some(daa)`: the claim is not
    /// final or voided, and its current phase ends by itself at `daa` (the licence's window, a DA
    /// session's backstop — the claim row's `deadline_daa`); `None`: it is final, voided, retired, or
    /// not a claim this node's bond seats. What a step-6 pursuit reads once its session is gone.
    fn claim_open_until_v1(&self, claim_id: &Hash64) -> Option<u64>;
}

/// **The responder's material, as P2-7's loader takes it**: the claim's facts, the pool's copies,
/// the retained files (read in the build task, off the tick) and where a re-made capture is kept.
pub(crate) struct PalwHeldMaterialV1 {
    pub facts: PalwDaClaimFactsV1,
    pub pooled: Vec<Vec<u8>>,
    pub paths: Vec<PathBuf>,
    pub keep_dir: Option<PathBuf>,
}

/// A detached blocking task, its result, or its refusal.
enum PalwHeldTaskV1<T> {
    Running {
        task: tokio::task::JoinHandle<Result<T, String>>,
        since_daa: u64,
    },
    Ready(T),
    /// Refused at `at_daa`: tried again a re-plan later (`COURT_MOVE_REPLAN_DAA`), never every tick —
    /// a material that did not verify, or a replay that did not reach the filed root, will not on
    /// the next tick either.
    Failed {
        at_daa: u64,
    },
}

impl<T> PalwHeldTaskV1<T> {
    fn finished(&self) -> bool {
        matches!(self, Self::Running { task, .. } if task.is_finished())
    }

    fn running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// Collect a finished task: `Ready`, or `Failed` with the reason said.
    async fn collect(self, what: &str, current_daa: u64) -> (Self, Option<String>) {
        match self {
            Self::Running { task, since_daa } => match task.await {
                Ok(Ok(done)) => {
                    trace!("[{PALW_PANEL}] {what}: done {} DAA after it started", current_daa.saturating_sub(since_daa));
                    (Self::Ready(done), None)
                }
                Ok(Err(why)) => (Self::Failed { at_daa: current_daa }, Some(why)),
                Err(e) => (Self::Failed { at_daa: current_daa }, Some(format!("the task did not finish: {e}"))),
            },
            other => (other, None),
        }
    }
}

/// A built held evidence, and the ledger's hold on its resident bytes — the two live and die together.
struct PalwHeldBuiltV1 {
    evidence: Arc<PalwAttnHeldEvidenceV1>,
    remade: bool,
    _resident: crate::palw_memory_ledger::PalwMemoryReservationV1,
}

/// One party's evidence for one `(claim, leaf)`: its build, the filing a challenger's was built from
/// (evicted if N2 fails on it), and the DAA a duty last named it.
struct PalwHeldEntryV1 {
    build: PalwHeldTaskV1<PalwHeldBuiltV1>,
    filing: Option<(Hash64, PalwAttnHeldFilingV1)>,
    named_daa: u64,
}

/// A checkpoint accusation's committed row, wanted (`task: None`) or being opened (step 6).
struct PalwHeldRowsV1 {
    unit: PalwHeldStep6UnitV1,
    position: u32,
    task: Option<PalwHeldTaskV1<Vec<PalwAttnRowOpeningV1>>>,
}

/// What the route keeps per session: the chain objects it read for it (the held root claims, the
/// producer's disclosures), when it last looked, step 6's demands and accusation row.
struct PalwHeldSessionV1 {
    named_daa: u64,
    looked_daa: Option<u64>,
    objects: Vec<PalwConsensusObjectV2>,
    demanded: HashMap<PalwHeldStep6UnitV1, u64>,
    /// Every demanded chunk is disclosed on chain: no more disclosure reads.
    disclosed_all: bool,
    rows: Option<PalwHeldRowsV1>,
}

/// **Step 6 that outlives its session** (the forger's race): a consistent forger's own node files
/// its acquittal the block the bottom is reached — its bottom, built from its own anchor, IS an
/// acquittal — and the session closes before the seat's demand is answered. The claim is still
/// licensed, and the DA court and the checkpoint court try it whatever court is (or is not) open, so
/// the challenger's node pursues the claim: the demand, the disclosure, `CheckpointAccused` — until
/// the claim is final or voided. Recorded when the challenger's last choice makes the phase
/// terminal and step 6 is needed there (the duty as it would read at `Terminal`), or at the
/// `Terminal` duty itself.
#[derive(Clone)]
struct PalwHeldPursuitV1 {
    duty: PalwCourtDutyV2,
    units: Vec<PalwHeldStep6UnitV1>,
}

/// A chain read the route asks the node for: every lifecycle object accepted since the session's
/// opening that names it or its claim (`attn_held_objects_from_chain_v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwHeldChainReadV1 {
    pub session_id: Hash64,
    pub claim_id: Hash64,
    pub not_before_daa: u64,
}

/// An item the route queues: its `court_pending` key, the DAA it is due by, and the object.
#[derive(Clone, Debug)]
pub(crate) struct PalwHeldQueuedV1 {
    pub key: PalwCourtQueueKeyV1,
    pub due: u64,
    pub object: PalwConsensusObjectV2,
}

/// One tick's moves: what to queue, and why the rest wait (the panel's stall lines).
#[derive(Default)]
pub(crate) struct PalwHeldTickV1 {
    pub queued: Vec<PalwHeldQueuedV1>,
    pub stalls: Vec<&'static str>,
}

/// **The held route's state across ticks** (the worker's, like `attn_evidence`).
#[derive(Default)]
pub(crate) struct PalwHeldCourtV1 {
    evidence: HashMap<PalwHeldEvidenceKeyV1, PalwHeldEntryV1>,
    sessions: HashMap<Hash64, PalwHeldSessionV1>,
    /// Per claim: the held root claims this node's N2 failed on — never read again.
    rejected: HashMap<Hash64, HashSet<Hash64>>,
    held_classes: HashMap<Hash64, bool>,
    /// Step 6's pursuits, by session — run from the claim's own facts once the session is gone.
    pursuits: HashMap<Hash64, PalwHeldPursuitV1>,
    /// This tick's duties whose evidence is wanted, and the sessions whose accusation row is.
    wanted: Vec<PalwCourtDutyV2>,
    rows_wanted: Vec<PalwCourtDutyV2>,
}

impl PalwHeldCourtV1 {
    fn session_mut(&mut self, session_id: Hash64, current_daa: u64) -> &mut PalwHeldSessionV1 {
        let entry = self.sessions.entry(session_id).or_insert_with(|| PalwHeldSessionV1 {
            named_daa: current_daa,
            looked_daa: None,
            objects: Vec::new(),
            demanded: HashMap::new(),
            disclosed_all: false,
            rows: None,
        });
        entry.named_daa = entry.named_daa.max(current_daa);
        entry
    }

    /// **Does the tick send `duty` down the held route?** [`palw_held_route_v1`] at the tick's DAA,
    /// the class's held-ness read off the chain once per class.
    pub(crate) fn routes_v1<H: PalwHeldHostV1>(&mut self, host: &H, duty: &PalwCourtDutyV2, current_daa: u64) -> bool {
        let fenced = host.offence_attribution_active(current_daa);
        if !fenced || !duty.fused_class || duty.terminal_index.is_none() {
            return false;
        }
        let class_is_held = match self.held_classes.get(&duty.class_id) {
            Some(known) => *known,
            None => match host.class_is_held(&duty.class_id) {
                Some(held) => *self.held_classes.entry(duty.class_id).or_insert(held),
                None => false,
            },
        };
        palw_held_route_v1(fenced, class_is_held, duty)
    }

    /// **The tick begins** (the review's LOW: kept by `(claim, leaf, role)` with a grace window, not
    /// dropped the first tick a duty set comes back empty): every entry a duty names is marked, what
    /// no duty has named for [`PALW_HELD_EVIDENCE_GRACE_DAA_V1`] is dropped (a running task is
    /// detached — it still returns its reservations when it ends), every finished task collected —
    /// a challenger build that failed evicts the filing it was built from — and last tick's wants
    /// are cleared.
    pub(crate) async fn begin_tick_v1(&mut self, court_duties: &[PalwCourtDutyV2], current_daa: u64) {
        for duty in court_duties {
            if let Some(key) = palw_held_evidence_key_v1(duty)
                && let Some(entry) = self.evidence.get_mut(&key)
            {
                entry.named_daa = current_daa;
            }
            if let Some(session) = self.sessions.get_mut(&duty.session_id) {
                session.named_daa = current_daa;
            }
        }
        let alive = |named: u64| named.saturating_add(PALW_HELD_EVIDENCE_GRACE_DAA_V1) >= current_daa;
        self.evidence.retain(|_, entry| alive(entry.named_daa));
        self.sessions.retain(|_, session| alive(session.named_daa));
        let claims: HashSet<Hash64> = self.evidence.keys().map(|(claim, _, _)| *claim).collect();
        self.rejected.retain(|claim, _| claims.contains(claim));
        self.wanted.clear();
        self.rows_wanted.clear();
        let finished: Vec<PalwHeldEvidenceKeyV1> =
            self.evidence.iter().filter(|(_, entry)| entry.build.finished()).map(|(key, _)| *key).collect();
        for key in finished {
            let Some(entry) = self.evidence.get_mut(&key) else { continue };
            let role = if key.2 { "responder (N1)" } else { "challenger (N2)" };
            let build = std::mem::replace(&mut entry.build, PalwHeldTaskV1::Failed { at_daa: current_daa });
            let (collected, refused) = build.collect("a held evidence build", current_daa).await;
            if let PalwHeldTaskV1::Ready(built) = &collected {
                info!(
                    "[{PALW_PANEL}] claim {}: the held evidence ({role}{}) at leaf {} is built — {} bytes resident (ADR-0152 §4-ter N3)",
                    key.0,
                    if built.remade { ", from a re-made capture" } else { "" },
                    key.1,
                    palw_held_evidence_resident_bytes_v1(&built.evidence)
                );
            }
            if let Some(why) = refused {
                warn!("[{PALW_PANEL}] claim {}: the held evidence ({role}) at leaf {} does not build: {why}", key.0, key.1);
                // The review's HIGH: N2 failed on this filing — never build from it again.
                if !key.2
                    && let Some((digest, _)) = entry.filing.take()
                {
                    self.rejected.entry(key.0).or_default().insert(digest);
                }
            }
            entry.build = collected;
        }
        for (session_id, session) in self.sessions.iter_mut() {
            let Some(rows) = session.rows.as_mut() else { continue };
            if !rows.task.as_ref().is_some_and(PalwHeldTaskV1::finished) {
                continue;
            }
            let task = rows.task.take().expect("a finished task");
            let (collected, refused) = task.collect("a checkpoint accusation's committed row", current_daa).await;
            if let Some(why) = refused {
                warn!("[{PALW_PANEL}] session {session_id}: the accused's committed cache-write row does not open: {why}");
            }
            rows.task = Some(collected);
        }
    }

    /// **The chain reads the route needs this tick**: for a challenger with its phase open, the
    /// session's held root claims while no unrejected one stands; at step 6, the producer's
    /// disclosures while a demanded chunk is not on chain — each at most every
    /// `HELD_CHAIN_RELOOK_DAA`.
    pub(crate) fn chain_reads_v1<H: PalwHeldHostV1>(
        &self,
        host: &H,
        duties: &[PalwCourtDutyV2],
        current_daa: u64,
    ) -> Vec<PalwHeldChainReadV1> {
        let mut reads = Vec::new();
        for duty in duties.iter().filter(|duty| !duty.i_am_responder && duty.dissection.is_some()) {
            let session = self.sessions.get(&duty.session_id);
            let rejected = self.rejected.get(&duty.claim_id).cloned().unwrap_or_default();
            let objects = session.map(|s| s.objects.as_slice()).unwrap_or(&[]);
            if palw_held_filing_of_duty_v1(objects, duty, host.opening_cap(&duty.class_id, current_daa), &rejected).is_none() {
                if session.and_then(|s| s.looked_daa).is_none_or(|at| current_daa >= at.saturating_add(HELD_CHAIN_RELOOK_DAA)) {
                    reads.push(PalwHeldChainReadV1 {
                        session_id: duty.session_id,
                        claim_id: duty.claim_id,
                        not_before_daa: duty.session_deadline_daa.saturating_sub(host.window_court()),
                    });
                }
                continue;
            }
            reads.extend(self.disclosure_read_v1(duty.session_id, duty.claim_id, current_daa));
        }
        // A pursuit whose session is gone reads its claim's disclosures the same way.
        for (session_id, pursuit) in &self.pursuits {
            if !duties.iter().any(|duty| duty.session_id == *session_id) {
                reads.extend(self.disclosure_read_v1(*session_id, pursuit.duty.claim_id, current_daa));
            }
        }
        reads
    }

    /// Step 6's read: the producer's disclosures since the earliest demand, while one is unanswered.
    fn disclosure_read_v1(&self, session_id: Hash64, claim_id: Hash64, current_daa: u64) -> Option<PalwHeldChainReadV1> {
        let session = self.sessions.get(&session_id)?;
        if session.disclosed_all || session.looked_daa.is_some_and(|at| current_daa < at.saturating_add(HELD_DISCLOSURE_RELOOK_DAA)) {
            return None;
        }
        let since = *session.demanded.values().min()?;
        Some(PalwHeldChainReadV1 { session_id, claim_id, not_before_daa: since })
    }

    /// The node's answer to a chain read: every object it found, oldest first — added to what the
    /// session already holds (a disclosure read starts at the demand, past the root claims).
    pub(crate) fn note_chain_v1(&mut self, session_id: Hash64, objects: Vec<PalwConsensusObjectV2>, current_daa: u64) {
        let session = self.session_mut(session_id, current_daa);
        let known: HashSet<Hash64> = session.objects.iter().map(palw_held_object_digest_v1).collect();
        session.objects.extend(objects.into_iter().filter(|object| !known.contains(&palw_held_object_digest_v1(object))));
        session.looked_daa = Some(current_daa);
    }

    /// Note `duty` as wanting its evidence built, unless it is building, built, or failed inside a
    /// re-plan.
    fn want(&mut self, duty: &PalwCourtDutyV2, current_daa: u64) {
        let Some(key) = palw_held_evidence_key_v1(duty) else { return };
        let fresh = match self.evidence.get(&key).map(|entry| &entry.build) {
            None => true,
            Some(PalwHeldTaskV1::Failed { at_daa }) => current_daa >= at_daa.saturating_add(COURT_MOVE_REPLAN_DAA),
            Some(_) => false,
        };
        if fresh && palw_held_evidence_buildable_v1(duty) && !self.wanted.iter().any(|w| palw_held_evidence_key_v1(w) == Some(key)) {
            self.wanted.push(duty.clone());
        }
    }

    /// The claims step 6 pursues past a session — the tick reads their rows before the moves.
    pub(crate) fn pursued_claims_v1(&self) -> HashSet<Hash64> {
        self.pursuits.values().map(|pursuit| pursuit.duty.claim_id).collect()
    }

    /// Tests: whether the route holds a BUILT evidence under `key`.
    #[cfg(test)]
    pub(crate) fn holds_built_evidence_v1(&self, key: &PalwHeldEvidenceKeyV1) -> bool {
        self.evidence.get(key).is_some_and(|entry| matches!(entry.build, PalwHeldTaskV1::Ready(_)))
    }

    /// Whether any task of the route is running (tests settle on it).
    #[cfg(test)]
    pub(crate) fn any_running_v1(&self) -> bool {
        self.evidence.values().any(|entry| entry.build.running())
            || self.sessions.values().any(|s| s.rows.as_ref().is_some_and(|r| r.task.as_ref().is_some_and(PalwHeldTaskV1::running)))
    }

    /// **Tests: wait until every running task has finished** — a fixture build is milliseconds; the
    /// next `begin_tick_v1` collects them, as the next real tick would.
    #[cfg(test)]
    pub(crate) async fn settle_v1(&self) {
        let unfinished = |this: &Self| {
            this.evidence.values().any(|entry| entry.build.running() && !entry.build.finished())
                || this
                    .sessions
                    .values()
                    .any(|s| s.rows.as_ref().is_some_and(|r| r.task.as_ref().is_some_and(|t| t.running() && !t.finished())))
        };
        while unfinished(self) {
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    }
}

/// **The held route's moves this tick** — for each held duty (routed by [`PalwHeldCourtV1::routes_v1`]),
/// the move the session asks of this node, built off its evidence by the court's kernels and signed,
/// due by the deadline the session carries; evidence not built yet is wanted for
/// [`palw_held_start_builds_v1`]; a challenger's close that needs step 6 runs it. `busy(key)` is the
/// panel's own dedupe — the item is queued, or was sent less than a re-plan ago.
pub(crate) fn palw_held_moves_v1<H: PalwHeldHostV1>(
    host: &H,
    held: &mut PalwHeldCourtV1,
    duties: &[PalwCourtDutyV2],
    current_daa: u64,
    busy: impl Fn(&PalwCourtQueueKeyV1) -> bool,
) -> PalwHeldTickV1 {
    let mut tick = PalwHeldTickV1::default();
    for duty in duties {
        // The claim's material is this route's to keep while the session lives, from the first tick
        // (the P2-7 review's LOW: the end-of-tick pins are gone after a restart).
        host.pin(duty.claim_id);
        let Some(key) = palw_held_evidence_key_v1(duty) else { continue };
        held.session_mut(duty.session_id, current_daa);
        if let Some(entry) = held.evidence.get_mut(&key) {
            entry.named_daa = current_daa;
        }
        let Some(mv) = palw_held_move_of_duty_v1(duty) else {
            // The other party's turn: this party's evidence is built meanwhile (`want` is a no-op once
            // it is building or built) — the responder's next round reads it, and a challenger's N2
            // starts the moment the filing is on chain, not when its first choice falls due.
            held.want(duty, current_daa);
            tick.stalls.push("waiting — the held dissection's move is the other party's");
            continue;
        };
        let deadline = palw_held_move_deadline_v1(duty, mv);
        if current_daa > deadline {
            tick.stalls.push("a held move past the deadline its session carries — the sweep decides it");
            continue;
        }
        let evidence = match held.evidence.get(&key).map(|entry| &entry.build) {
            Some(PalwHeldTaskV1::Ready(built)) => built.evidence.clone(),
            Some(PalwHeldTaskV1::Running { .. }) => {
                tick.stalls.push("building the held evidence (N1/N2) off the tick");
                continue;
            }
            _ => {
                held.want(duty, current_daa);
                tick.stalls.push(if !duty.i_am_responder && duty.dissection.is_none() {
                    "waiting for the accused's held root claim"
                } else {
                    "the held evidence waits for a build"
                });
                continue;
            }
        };
        let Some(arity) = host.arity(current_daa) else {
            tick.stalls.push("this ruleset's court has no shape for a dissection");
            continue;
        };
        let ctx =
            PalwHeldMoveCtxV1 { artifact_root: duty.artifact_root, opening_cap: host.opening_cap(&duty.class_id, current_daa), arity };
        let move_key = (duty.session_id, super::court_move_round_v1(duty), duty.i_am_responder);
        // 4-ter.3 step 6: the challenger's bottom, where its own sub-root of the disputed layer is
        // not the filed one.
        if mv == PalwHeldMoveV1::Close
            && !duty.i_am_responder
            && let Some(phase) = duty.dissection.as_ref()
        {
            match evidence
                .site_v1(ctx.artifact_root, true, ctx.opening_cap)
                .map_err(|e| e.to_string())
                .and_then(|anchored| palw_held_step6_units_v1(&evidence, &anchored, phase).map(|units| (anchored, units)))
            {
                Ok((anchored, units)) if !units.is_empty() => {
                    held.pursuits
                        .entry(duty.session_id)
                        .or_insert_with(|| PalwHeldPursuitV1 { duty: duty.clone(), units: units.clone() });
                    let backstop = (true, duty.session_deadline_daa);
                    palw_held_step6_v1(
                        host,
                        held,
                        duty,
                        &evidence,
                        &anchored,
                        phase,
                        &units,
                        move_key,
                        backstop,
                        current_daa,
                        &busy,
                        &mut tick,
                    );
                    continue;
                }
                Ok(_) => {}
                Err(why) => {
                    warn!("[{PALW_PANEL}] session {}: the held bottom's chunks: {why}", duty.session_id);
                    tick.stalls.push("a held move does not compute from the evidence");
                    continue;
                }
            }
        }
        if busy(&move_key) {
            continue;
        }
        match palw_held_move_object_v1(
            &evidence,
            duty,
            mv,
            &ctx,
            |proof| host.verdict_of(&duty.session_id, proof),
            |message, context| host.sign(message, context),
        ) {
            Ok(PalwHeldMoveOutcomeV1::File(object)) => {
                info!(
                    "[{PALW_PANEL}] session {}: filing the held dissection's {mv:?} for claim {} as {} — due by DAA {deadline} (now \
                     {current_daa}; ADR-0152 §4-ter N3)",
                    duty.session_id,
                    duty.claim_id,
                    if duty.i_am_responder { "responder" } else { "challenger" }
                );
                // The last choice: if the bottom it reaches needs step 6, the pursuit starts now —
                // the forger's acquittal may close the session before this node sees `Terminal`.
                if let PalwConsensusObjectV2::CourtAttnChildChosen { choice, .. } = &object
                    && let Some(terminal) = palw_held_terminal_duty_after_v1(duty, choice)
                    && let Some(phase) = terminal.dissection.clone()
                {
                    match evidence
                        .site_v1(ctx.artifact_root, true, ctx.opening_cap)
                        .map_err(|e| e.to_string())
                        .and_then(|anchored| palw_held_step6_units_v1(&evidence, &anchored, &phase))
                    {
                        Ok(units) if !units.is_empty() => {
                            info!(
                                "[{PALW_PANEL}] session {}: the bottom this choice reaches reads a slice whose filed sub-root is not this \
                                 node's — step 6 is pursued from here, session or not (ADR-0152 §4-ter.3)",
                                duty.session_id
                            );
                            held.pursuits.entry(duty.session_id).or_insert(PalwHeldPursuitV1 { duty: terminal, units });
                        }
                        Ok(_) => {}
                        Err(why) => warn!("[{PALW_PANEL}] session {}: the bottom's chunks after the choice: {why}", duty.session_id),
                    }
                }
                tick.queued.push(PalwHeldQueuedV1 { key: move_key, due: deadline, object });
            }
            Ok(PalwHeldMoveOutcomeV1::Nothing(why)) => tick.stalls.push(why),
            Err(why) => {
                warn!("[{PALW_PANEL}] session {}: the held dissection's {mv:?} does not build: {why}", duty.session_id);
                tick.stalls.push("a held move does not compute from the evidence");
            }
        }
    }
    palw_held_pursue_v1(host, held, duties, current_daa, &busy, &mut tick);
    tick
}

/// **Step 6's pursuits whose session is gone** ([`PalwHeldPursuitV1`]): each from the duty it
/// recorded and the evidence it keeps alive, due by the claim's own phase end, until the claim is
/// final or voided — the demand (again, if the first did not land), the disclosure, the committed row,
/// `CheckpointAccused`. A pursuit whose session is still open runs on the duty instead.
fn palw_held_pursue_v1<H: PalwHeldHostV1>(
    host: &H,
    held: &mut PalwHeldCourtV1,
    duties: &[PalwCourtDutyV2],
    current_daa: u64,
    busy: &impl Fn(&PalwCourtQueueKeyV1) -> bool,
    tick: &mut PalwHeldTickV1,
) {
    let closed: Vec<Hash64> =
        held.pursuits.keys().filter(|session_id| !duties.iter().any(|duty| duty.session_id == **session_id)).copied().collect();
    for session_id in closed {
        let Some(pursuit) = held.pursuits.get(&session_id).cloned() else { continue };
        let duty = &pursuit.duty;
        let Some(open_until) = host.claim_open_until_v1(&duty.claim_id) else {
            info!("[{PALW_PANEL}] claim {}: final or voided — step 6's pursuit of session {session_id} ends", duty.claim_id);
            held.pursuits.remove(&session_id);
            continue;
        };
        host.pin(duty.claim_id);
        held.session_mut(session_id, current_daa);
        let evidence = match palw_held_evidence_key_v1(duty).and_then(|key| held.evidence.get_mut(&key)) {
            Some(entry) => {
                entry.named_daa = current_daa;
                match &entry.build {
                    PalwHeldTaskV1::Ready(built) => built.evidence.clone(),
                    _ => {
                        held.pursuits.remove(&session_id);
                        continue;
                    }
                }
            }
            None => {
                held.pursuits.remove(&session_id);
                continue;
            }
        };
        let Some(phase) = duty.dissection.as_ref() else {
            held.pursuits.remove(&session_id);
            continue;
        };
        let anchored = match evidence.site_v1(duty.artifact_root, true, host.opening_cap(&duty.class_id, current_daa)) {
            Ok(anchored) => anchored,
            Err(why) => {
                warn!("[{PALW_PANEL}] session {session_id}: step 6's pursuit: the anchored site: {why}");
                held.pursuits.remove(&session_id);
                continue;
            }
        };
        let close_key = (session_id, super::court_move_round_v1(duty), false);
        let settled = palw_held_step6_v1(
            host,
            held,
            duty,
            &evidence,
            &anchored,
            phase,
            &pursuit.units,
            close_key,
            (false, open_until),
            current_daa,
            busy,
            tick,
        );
        if settled {
            held.pursuits.remove(&session_id);
        }
    }
}

/// **4-ter.3 step 6 for one session at its bottom**: demand every chunk of the disputed tile whose
/// slice's sub-root this node computes differently from the filing (due a disclose window before
/// `backstop`, so the producer's silence defaults it first), and once every one is disclosed on
/// chain, either the bottom through the disclosed paths or — a disclosed row not this node's — the
/// checkpoint accusation (its committed row opened off the tick; due by `backstop`). `backstop` is
/// the session's while it is `open`; once it is gone (a pursuit) the claim's own phase end, and
/// there is no bottom to file. Returns whether a pursuit is settled: its session gone and every
/// disclosed chunk this node's own bytes — nothing left to try.
#[allow(clippy::too_many_arguments)]
fn palw_held_step6_v1<H: PalwHeldHostV1>(
    host: &H,
    held: &mut PalwHeldCourtV1,
    duty: &PalwCourtDutyV2,
    evidence: &PalwAttnHeldEvidenceV1,
    anchored: &PalwAttnDisputeSiteV2,
    phase: &kaspa_consensus_core::palw_attn_court_v1::PalwAttnDissectPhaseV1,
    units: &[PalwHeldStep6UnitV1],
    close_key: PalwCourtQueueKeyV1,
    (open, backstop): (bool, u64),
    current_daa: u64,
    busy: &impl Fn(&PalwCourtQueueKeyV1) -> bool,
    tick: &mut PalwHeldTickV1,
) -> bool {
    let Some(key) = palw_held_evidence_key_v1(duty) else { return false };
    let Some((_, filing)) = held.evidence.get(&key).and_then(|entry| entry.filing.clone()) else {
        tick.stalls.push("step 6 needs the filing the challenger's evidence was built from");
        return false;
    };
    let window = host.disclose_window_daa();
    let session = held.session_mut(duty.session_id, current_daa);
    let disclosed: HashMap<PalwHeldStep6UnitV1, PalwAttnChunkOpeningV1> = units
        .iter()
        .filter_map(|unit| {
            palw_held_step6_disclosed_v1(&session.objects, duty, &filing, unit, anchored.site.anchor_positions).map(|c| (*unit, c))
        })
        .collect();
    session.disclosed_all = disclosed.len() == units.len();
    for unit in units.iter().filter(|unit| !disclosed.contains_key(unit)) {
        if session.demanded.get(unit).is_some_and(|at| current_daa < at.saturating_add(window).saturating_add(COURT_MOVE_REPLAN_DAA)) {
            tick.stalls.push("step 6: waiting for the producer to disclose the demanded chunk");
            continue;
        }
        let demand_key = unit.demand_key_v1(duty.session_id);
        if busy(&demand_key) {
            continue;
        }
        match palw_held_step6_demand_v1(duty, &filing.binding, unit, host.bond(), host.network_domain(), |m, c| host.sign(m, c)) {
            Ok(object) => {
                info!(
                    "[{PALW_PANEL}] session {}: this node's sub-root of the disputed layer's {:?} slice is not the filed one — \
                     demanding chunk {} of checkpoint {} (ADR-0152 §4-ter.3 step 6)",
                    duty.session_id, unit.kind, unit.chunk_index, unit.checkpoint
                );
                session.demanded.insert(*unit, current_daa);
                tick.queued.push(PalwHeldQueuedV1 { key: demand_key, due: backstop.saturating_sub(window).max(current_daa), object });
            }
            Err(why) => {
                warn!("[{PALW_PANEL}] session {}: the step-6 demand does not build: {why}", duty.session_id);
                tick.stalls.push("a step-6 demand does not build");
            }
        }
    }
    let mut want_rows = false;
    match palw_held_step6_next_v1(evidence, anchored, phase, units, &disclosed) {
        Ok(PalwHeldStep6NextV1::Pending) => {}
        Ok(PalwHeldStep6NextV1::Bottom(_)) if !open => {
            info!(
                "[{PALW_PANEL}] claim {}: every chunk its producer disclosed is this node's own — its session is closed, and step 6 \
                 has nothing to accuse (ADR-0152 §4-ter.3)",
                duty.claim_id
            );
            return true;
        }
        Ok(PalwHeldStep6NextV1::Bottom(bottom)) => {
            if busy(&close_key) {
                return false;
            }
            match palw_held_close_object_v1(evidence, duty, bottom, |proof| host.verdict_of(&duty.session_id, proof)) {
                Ok(PalwHeldMoveOutcomeV1::File(object)) => {
                    info!(
                        "[{PALW_PANEL}] session {}: bottoming on the producer's disclosed path (ADR-0152 §4-ter.3 step 6)",
                        duty.session_id
                    );
                    tick.queued.push(PalwHeldQueuedV1 { key: close_key, due: backstop, object });
                }
                Ok(PalwHeldMoveOutcomeV1::Nothing(why)) => tick.stalls.push(why),
                Err(why) => {
                    warn!("[{PALW_PANEL}] session {}: the step-6 bottom does not build: {why}", duty.session_id);
                    tick.stalls.push("a step-6 bottom does not build");
                }
            }
        }
        Ok(PalwHeldStep6NextV1::Accuse { unit, position, chunk }) => {
            let accuse_key = (duty.session_id, PALW_HELD_STEP6_ACCUSE_ROUND_V1, false);
            let same = |rows: &PalwHeldRowsV1| rows.unit == unit && rows.position == position;
            match session.rows.as_ref().filter(|rows| same(rows)).and_then(|rows| rows.task.as_ref()) {
                Some(PalwHeldTaskV1::Ready(rows)) => {
                    if busy(&accuse_key) {
                        return false;
                    }
                    match palw_held_step6_accusation_v1(
                        duty,
                        &filing,
                        &unit,
                        position,
                        chunk,
                        rows.clone(),
                        host.bond(),
                        host.network_domain(),
                        host.opening_cap(&duty.class_id, current_daa),
                        |m, c| host.sign(m, c),
                    ) {
                        Ok(object) => {
                            info!(
                                "[{PALW_PANEL}] session {}{}: the producer's disclosed {:?} row at position {position} is not the row \
                                 its step tree committed — filing CheckpointAccused on claim {} (ADR-0152 §4-ter.3 step 6, C3)",
                                duty.session_id,
                                if open { "" } else { " (closed)" },
                                unit.kind,
                                duty.claim_id
                            );
                            tick.queued.push(PalwHeldQueuedV1 { key: accuse_key, due: backstop, object });
                        }
                        Err(why) => {
                            warn!("[{PALW_PANEL}] session {}: the checkpoint accusation: {why}", duty.session_id);
                            tick.stalls.push("a step-6 accusation does not adjudicate");
                        }
                    }
                }
                Some(PalwHeldTaskV1::Running { .. }) => tick.stalls.push("step 6: opening the accused's committed row off the tick"),
                Some(PalwHeldTaskV1::Failed { at_daa }) if current_daa < at_daa.saturating_add(COURT_MOVE_REPLAN_DAA) => {
                    tick.stalls.push("step 6: the committed row did not open — tried again a re-plan later");
                }
                _ => {
                    session.rows = Some(PalwHeldRowsV1 { unit, position, task: None });
                    want_rows = true;
                    tick.stalls.push("step 6: the accused's committed row waits for a build");
                }
            }
        }
        Err(why) => {
            warn!("[{PALW_PANEL}] session {}: the step-6 disclosures: {why}", duty.session_id);
            tick.stalls.push("step 6: the disclosures do not decide");
        }
    }
    if want_rows {
        held.rows_wanted.push(duty.clone());
    }
    false
}

/// **Start the held builds the ledger admits** — the responder's first (its silence is a default),
/// then the soonest deadline; up to [`PALW_HELD_BUILDS_MAX_V1`] running at once. Each is a fresh
/// backend through the node's resolve door, reserved on the one ledger as a full seat for the
/// build's life (`"held-dissection"`); a finished evidence releases that and reserves its own
/// resident bytes (`"held-evidence"`) for its life — refused, it is dropped and rebuilt a re-plan
/// later, never held unreserved (the review's LOW). A ledger refusal leaves the want for the next
/// tick and tries the next candidate; any other refusal is retried a re-plan later.
pub(crate) fn palw_held_start_builds_v1<H: PalwHeldHostV1>(host: &H, held: &mut PalwHeldCourtV1, current_daa: u64) {
    let rank = |duty: &PalwCourtDutyV2| (!duty.i_am_responder, duty.rung_deadline_daa.min(duty.session_deadline_daa), duty.session_id);
    let mut candidates: Vec<(bool, PalwCourtDutyV2)> = std::mem::take(&mut held.wanted).into_iter().map(|d| (false, d)).collect();
    candidates.extend(std::mem::take(&mut held.rows_wanted).into_iter().map(|d| (true, d)));
    candidates.sort_by_key(|(rows, duty)| (rank(duty), *rows));
    let running = |held: &PalwHeldCourtV1| {
        held.evidence.values().filter(|entry| entry.build.running()).count()
            + held
                .sessions
                .values()
                .filter(|s| s.rows.as_ref().is_some_and(|r| r.task.as_ref().is_some_and(PalwHeldTaskV1::running)))
                .count()
    };
    for (rows, duty) in candidates {
        if running(held) >= PALW_HELD_BUILDS_MAX_V1 {
            return;
        }
        let Some(key) = palw_held_evidence_key_v1(&duty) else { continue };
        let Some(narrowed) = duty.terminal_index else { continue };
        let fail = |held: &mut PalwHeldCourtV1| {
            if rows {
                if let Some(r) = held.sessions.get_mut(&duty.session_id).and_then(|s| s.rows.as_mut()) {
                    r.task = Some(PalwHeldTaskV1::Failed { at_daa: current_daa });
                }
            } else {
                held.evidence.insert(
                    key,
                    PalwHeldEntryV1 { build: PalwHeldTaskV1::Failed { at_daa: current_daa }, filing: None, named_daa: current_daa },
                );
            }
        };
        // A challenger's filing: the first held root claim on chain that stands (the review's HIGH).
        let filing = if duty.i_am_responder {
            None
        } else if rows {
            held.evidence.get(&key).and_then(|entry| entry.filing.clone())
        } else {
            let objects = held.sessions.get(&duty.session_id).map(|s| s.objects.clone()).unwrap_or_default();
            let rejected = held.rejected.get(&duty.claim_id).cloned().unwrap_or_default();
            palw_held_filing_of_duty_v1(&objects, &duty, host.opening_cap(&duty.class_id, current_daa), &rejected)
        };
        if !duty.i_am_responder && filing.is_none() {
            trace!("[{PALW_PANEL}] session {}: no standing held root claim of the accused's on chain yet", duty.session_id);
            continue;
        }
        // A fresh instance: the build task owns it, and its walk and state go when the build ends.
        let backend = match host.backend(&duty) {
            Ok(backend) => backend,
            Err(why) => {
                warn!("[{PALW_PANEL}] session {}: no backend for the held class: {why}", duty.session_id);
                fail(held);
                continue;
            }
        };
        let ledger = host.ledger();
        let need = host.build_need_bytes(backend.as_ref(), &duty);
        let reserved = match ledger.reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                role: "held-dissection",
                class_id: duty.class_id,
                job: duty.claim_id,
            },
            need,
        ) {
            Ok(reserved) => reserved,
            Err(refusal) => {
                crate::palw_backends::note_throttled_v1("panel-held-build-ledger", || {
                    format!("[{PALW_PANEL}] session {}: a held build waits for the memory ledger — {refusal}", duty.session_id)
                });
                // Wanted again next tick; a smaller build of another class may still fit now.
                continue;
            }
        };
        let (class_id, claim_id) = (duty.class_id, duty.claim_id);
        if rows {
            let Some(r) = held.sessions.get_mut(&duty.session_id).and_then(|s| s.rows.as_mut()) else { continue };
            let Some((_, filing)) = filing else { continue };
            let carried = match host.carried_prompt(&duty, backend.as_ref()) {
                Ok(carried) => carried,
                Err(why) => {
                    warn!("[{PALW_PANEL}] session {}: step 6 holds no prompt of the claim: {why}", duty.session_id);
                    r.task = Some(PalwHeldTaskV1::Failed { at_daa: current_daa });
                    continue;
                }
            };
            let (kind, layer, position) = (r.unit.kind, r.unit.layer, r.position);
            let task = tokio::task::spawn_blocking(move || {
                let _held_for_the_rows = reserved;
                backend.attn_held_cache_write_rows_v1(&filing, narrowed, carried.as_deref(), kind, layer, position)
            });
            r.task = Some(PalwHeldTaskV1::Running { task, since_daa: current_daa });
            continue;
        }
        let task = if duty.i_am_responder {
            let material = match host.responder_material(&duty, backend.as_ref()) {
                Ok(material) => material,
                Err(why) => {
                    warn!("[{PALW_PANEL}] session {}: the held responder's material: {why}", duty.session_id);
                    fail(held);
                    continue;
                }
            };
            tokio::task::spawn_blocking(move || {
                let PalwHeldMaterialV1 { facts, pooled, paths, keep_dir } = material;
                let kept = pooled.into_iter().chain(paths.iter().filter_map(|path| std::fs::read(path).ok()));
                let keep = |bytes: &[u8]| {
                    if let Some(dir) = keep_dir.as_deref() {
                        palw_da_keep_remade_v1(dir, &claim_id, bytes);
                    }
                };
                let built = palw_held_responder_evidence_v1(backend.as_ref(), &facts, kept, keep, narrowed);
                drop(reserved);
                let (evidence, remade) = built?;
                palw_held_resident_v1(&ledger, evidence, remade, class_id, claim_id)
            })
        } else {
            let carried = match host.carried_prompt(&duty, backend.as_ref()) {
                Ok(carried) => carried,
                Err(why) => {
                    warn!("[{PALW_PANEL}] session {}: the held challenger holds no prompt of the claim: {why}", duty.session_id);
                    fail(held);
                    continue;
                }
            };
            let (_, filed) = filing.clone().expect("checked above");
            tokio::task::spawn_blocking(move || {
                let built = palw_held_challenger_evidence_v1(backend.as_ref(), &filed, narrowed, carried.as_deref());
                drop(reserved);
                palw_held_resident_v1(&ledger, built?, false, class_id, claim_id)
            })
        };
        info!(
            "[{PALW_PANEL}] session {}: building the held evidence as {} off the tick — its next move is due by DAA {} \
             (ADR-0152 §4-ter N3)",
            duty.session_id,
            if duty.i_am_responder { "responder (N1)" } else { "challenger (N2)" },
            duty.rung_deadline_daa
        );
        held.evidence.insert(
            key,
            PalwHeldEntryV1 { build: PalwHeldTaskV1::Running { task, since_daa: current_daa }, filing, named_daa: current_daa },
        );
    }
}

/// A finished evidence under its own reservation — or dropped, never held unreserved.
fn palw_held_resident_v1(
    ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
    evidence: PalwAttnHeldEvidenceV1,
    remade: bool,
    class_id: Hash64,
    claim_id: Hash64,
) -> Result<PalwHeldBuiltV1, String> {
    let bytes = palw_held_evidence_resident_bytes_v1(&evidence);
    let resident = ledger
        .reserve(crate::palw_memory_ledger::PalwMemoryReservationKeyV1 { role: "held-evidence", class_id, job: claim_id }, bytes)
        .map_err(|refusal| {
            format!("the ledger will not hold the evidence's {bytes} resident bytes — dropped, rebuilt later: {refusal}")
        })?;
    Ok(PalwHeldBuiltV1 { evidence: Arc::new(evidence), remade, _resident: resident })
}

/// **The panel as the held route's host** — its session at the tick, its bond, the network domain
/// and the tick's material pool.
pub(crate) struct PalwPanelHeldHostV1<'a> {
    pub panel: &'a PalwPanelService,
    pub session: &'a kaspa_consensusmanager::ConsensusProxy,
    pub network_domain: Hash64,
    pub bond: PalwBondKeyV2,
    pub materials: &'a HashMap<Hash64, Vec<Vec<u8>>>,
    /// The pursued claims still open this tick ([`palw_held_open_claims_v1`]).
    pub open_claims: &'a HashMap<Hash64, u64>,
}

impl PalwPanelHeldHostV1<'_> {
    fn bundle(&self) -> Option<&kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2> {
        match &self.panel.consensus_config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle),
            _ => None,
        }
    }
}

impl PalwHeldHostV1 for PalwPanelHeldHostV1<'_> {
    fn offence_attribution_active(&self, daa: u64) -> bool {
        self.panel.consensus_config.params.palw_offence_attribution_active_at(daa)
    }

    fn class_is_held(&self, class_id: &Hash64) -> Option<bool> {
        self.session.palw_v2_class_table().into_iter().find(|row| row.class_id == *class_id).map(|row| row.held)
    }

    /// The court opens the site's rows at the claim's ladder under the held regime
    /// (`palw_attn_opening_cap_v1`), the structural `2^22` before it.
    fn opening_cap(&self, class_id: &Hash64, daa: u64) -> u64 {
        if self.panel.consensus_config.params.palw_held_context_active_at(daa) {
            self.panel.class_step_ladder(*class_id)
        } else {
            kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES
        }
    }

    /// The arity the ACCEPTANCE layer derives (the processor's `palw_court_params_at`: the held
    /// derivation where the held regime is in force), which the root claim must declare.
    fn arity(&self, daa: u64) -> Option<u8> {
        let params = &self.panel.consensus_config.params;
        kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2(
            self.bundle()?,
            params.palw_kary_court_active_at(daa),
            params.palw_held_context_active_at(daa),
        )
        .ok()
        .map(|court| court.dissection_arity())
    }

    fn disclose_window_daa(&self) -> u64 {
        self.bundle().map_or(0, |bundle| kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&bundle.state))
    }

    fn window_court(&self) -> u64 {
        self.bundle().map_or(0, |bundle| bundle.state.window_court())
    }

    fn network_domain(&self) -> Hash64 {
        self.network_domain
    }

    fn bond(&self) -> PalwBondKeyV2 {
        self.bond
    }

    fn verdict_of(&self, session_id: &Hash64, proof: &PalwCourtVerdictProofV2) -> Option<PalwCourtVerdictV2> {
        self.session.palw_court_close_verdict_v2(session_id, proof)
    }

    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>> {
        self.panel.sign(message, context)
    }

    fn pin(&self, claim: Hash64) {
        self.panel.foreign_pinned.lock().unwrap().insert(claim);
    }

    fn ledger(&self) -> Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1> {
        crate::palw_memory_ledger::host_ledger_v1()
    }

    fn backend(&self, duty: &PalwCourtDutyV2) -> Result<Box<dyn PalwExecutionBackendV1>, String> {
        self.panel.resolve_backend(self.session, duty.class_id, duty.artifact_root)
    }

    fn build_need_bytes(&self, backend: &dyn PalwExecutionBackendV1, duty: &PalwCourtDutyV2) -> u64 {
        self.panel
            .backends()
            .role_memory_need_for_backend_or_chain_v1(
                backend,
                duty.class_id,
                duty.artifact_root,
                None,
                kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::FullSeat,
                |id| self.panel.chain_carriage_v1(self.session, id),
            )
            .total_bytes()
    }

    fn responder_material(&self, duty: &PalwCourtDutyV2, backend: &dyn PalwExecutionBackendV1) -> Result<PalwHeldMaterialV1, String> {
        let (_, _, work_leaves) = self.session.palw_claim_roots_v2(duty.claim_id).ok_or("the chain holds no such claim")?;
        let panel = self.panel;
        let lane = if duty.free_prompt {
            PalwDaLaneV1::FreePrompt { panel_da_admissible: panel.consensus_config.params.palw_panel_da_admissible() }
        } else {
            let bond = &duty.executor_bond;
            PalwDaLaneV1::Attempt {
                anchor: panel
                    .job_anchor_for_claim(self.session, backend, self.network_domain, duty.accepted_block, duty.class_id, bond)
                    .unwrap_or_default(),
                attempt_draw: panel.attempt_draw_for_claim(self.session, duty.accepted_block),
                job: panel.attempt_job_for_claim(self.session, backend, self.network_domain, duty.accepted_block, duty.class_id, bond),
            }
        };
        let facts = PalwDaClaimFactsV1 {
            claim_id: duty.claim_id,
            class_id: duty.class_id,
            executor_bond: duty.executor_bond,
            execution_root: duty.execution_root,
            trace_root: duty.trace_root,
            work_leaves,
            form: panel.class_prompt_ids_form(duty.class_id),
            lane,
            // `None`, as every court re-make reads it (`PalwCourtDutyV2` carries no identity): the
            // claim's execution root commits its job's context, so no other job's capture reproduces
            // it (`verify_material`).
            job_pin: None,
        };
        // Every copy this node may hold: the pool's first, then its own retention and `foreign/`.
        let dir = &panel.config.retention_dir;
        let paths: Vec<PathBuf> = if duty.free_prompt {
            panel.fp_retained_payload_paths(&duty.claim_id).to_vec()
        } else {
            vec![
                crate::palw_producer::palw_retained_material_path(dir, &duty.claim_id),
                dir.join("foreign").join(format!("{}.material", duty.claim_id)),
            ]
        };
        Ok(PalwHeldMaterialV1 {
            facts,
            pooled: self.materials.get(&duty.claim_id).cloned().unwrap_or_default(),
            paths,
            keep_dir: Some(dir.join("foreign")),
        })
    }

    fn carried_prompt(&self, duty: &PalwCourtDutyV2, backend: &dyn PalwExecutionBackendV1) -> Result<Option<Vec<u32>>, String> {
        if !duty.free_prompt {
            return Ok(None);
        }
        // A free-prompt claim's prompt is the user's, out of the job material this node holds.
        let pooled = self.materials.get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[]);
        self.panel
            .fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, pooled)
            .and_then(|job| PalwPanelService::fp_prompt_for_job(backend, &job, self.panel.class_prompt_ids_form(duty.class_id)))
            .map(Some)
            .ok_or_else(|| "no job material of the claim is held here".to_string())
    }

    /// The tick's read of the pursued claims ([`palw_held_open_claims_v1`] over this bond's seat rows).
    fn claim_open_until_v1(&self, claim_id: &Hash64) -> Option<u64> {
        self.open_claims.get(claim_id).copied()
    }
}

/// **The pursued claims still open, and until when** — out of the claim rows this bond seats
/// (`palw_claim_rows_v1`, the panel loop's per-bond read, taken off the tick only while a step-6
/// pursuit exists, which is rare): each non-terminal claim's current phase end. A challenger that
/// is no seat of the claim's panel finds no row, and its pursuit ends with the session.
pub(crate) fn palw_held_open_claims_v1(
    rows: &[kaspa_consensus_core::palw_producer_v2::PalwClaimRowV1],
    pursued: &HashSet<Hash64>,
) -> HashMap<Hash64, u64> {
    rows.iter()
        .filter(|row| pursued.contains(&row.claim_id) && !row.phase.is_terminal())
        .map(|row| (row.claim_id, row.deadline_daa.unwrap_or(u64::MAX)))
        .collect()
}

#[cfg(test)]
mod tests {
    //! **ADR-0152 §4-ter T-A10 (the node): the held route is the one a held dissection takes past the
    //! fence, it files C2, it reads every deadline off the session, and the priority lane carries the
    //! soonest due first.** The route's runs against the fold — T-A9, the tick, step 6 — are in
    //! `held_court_e2e`.
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
    /// regime), on testnet-12's own ruleset — a root claim declaring any other is refused. The panel's
    /// DENSE root claim declares it the same way (the review's MEDIUM: it derived it with the plain
    /// `palw_court_params_at_v2`, which finds none on testnet-12;
    /// `consensus/core/tests/review_heldnode_arity.rs` pins the numbers on every preset).
    #[test]
    fn the_held_and_dense_routes_declare_the_arity_the_acceptance_layer_derives() {
        let t12 = palw_t12_shipped_params();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {
            panic!("testnet-12 is a V2 network")
        };
        assert!(t12.palw_offence_attribution_active_at(0) && t12.palw_held_context_active_at(0), "the held route's network");
        let court = kaspa_consensus_core::palw_court_v2::palw_court_params_held_at_v2(bundle, t12.palw_kary_court_active_at(0), true)
            .expect("testnet-12 derives an arity");
        assert_eq!(court.dissection_arity(), 4, "testnet-12's held court: arity 4 (4-ter F2)");
        let held = "palw_court_params_held_at_v2(\n            self.bundle()?,\n            params.palw_kary_court_active_at(daa),\n            params.palw_held_context_active_at(daa),";
        let source = include_str!("held_court.rs");
        assert!(
            source[source.find("    fn arity(&self, daa: u64) -> Option<u8> {\n        let params").expect("the host's arity")..]
                .contains(held)
        );
        let panel = include_str!("../palw_panel.rs");
        let production = &panel[..panel.find("#[cfg(test)]\nmod tests {").expect("the unit tests follow the code")];
        let root = &production[production.find("AttnMove::Root => {").expect("the dense root arm")..];
        let root = &root[..root.find("AttnMove::Round => {").expect("its end")];
        assert!(!root.contains("palw_court_params_at_v2("), "never the plain derivation");
        assert!(root.contains(
            "palw_court_params_held_at_v2(\n                                bundle,\n                                params.palw_kary_court_active_at(current_daa),\n                                params.palw_held_context_active_at(current_daa),"
        ));
        let _ = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    }

    fn item(key: (u64, u32, bool)) -> (Hash64, u32, bool, PalwConsensusObjectV2) {
        (h(key.0), key.1, key.2, PalwConsensusObjectV2::ReceiptLicensed { claim: h(key.0), receipts: Vec::new() })
    }

    fn keys(queue: &[(Hash64, u32, bool, PalwConsensusObjectV2)]) -> Vec<(Hash64, u32, bool)> {
        queue.iter().map(|(a, b, c, _)| (*a, *b, *c)).collect()
    }

    /// **The priority lane in earliest-deadline order, by the integration owner's rules** (the
    /// review's MEDIUM): dated items first, soonest due first; every item of one key at the key's
    /// earliest due and then by round (one session's moves never swap, however their own dates
    /// fall); undated items after every dated one, in their own (age) order; and the sort is stable
    /// (a re-sort of a sorted queue changes nothing).
    #[test]
    fn the_priority_lane_carries_the_soonest_due_first_and_never_swaps_one_sessions_moves() {
        let mut queue = vec![
            item((1, 0, false)),        // undated, oldest
            item((2, 7, true)),         // session 2's later round, due 90
            item((3, 1, true)),         // due 50
            item((2, 5, true)),         // session 2's earlier round, due 120 — the session sorts at 90
            item((4, u32::MAX, false)), // undated, younger
            item((5, 0, false)),        // due 20
        ];
        let due: HashMap<(Hash64, u32, bool), u64> =
            [((h(2), 7, true), 90), ((h(3), 1, true), 50), ((h(2), 5, true), 120), ((h(5), 0, false), 20)].into_iter().collect();
        palw_court_queue_edf_v1(&mut queue, &due);
        assert_eq!(
            keys(&queue),
            vec![(h(5), 0, false), (h(3), 1, true), (h(2), 5, true), (h(2), 7, true), (h(1), 0, false), (h(4), u32::MAX, false)],
            "soonest first; session 2 at its earliest (90), by round; the undated after, oldest first"
        );
        let again = keys(&queue);
        palw_court_queue_edf_v1(&mut queue, &due);
        assert_eq!(keys(&queue), again, "stable");
        let mut undated = vec![item((9, 0, false)), item((8, 0, false)), item((7, 0, false))];
        palw_court_queue_edf_v1(&mut undated, &HashMap::new());
        assert_eq!(keys(&undated), vec![(h(9), 0, false), (h(8), 0, false), (h(7), 0, false)], "with no dates, the queue's own order");
    }

    /// **Each kind's due date**: a dense move its rung inside the backstop, a close (Terminal) the
    /// backstop; a P2-7 answer the session's deadline for the producer, and for a covering signer the
    /// turn of the signer ranked after it (`palw_disclosure_due_v1`'s stagger) — testnet-12's
    /// `W_disclose` 1,200 on a deadline of 2,200: rank 0 by 1,675 (rank 1's turn), rank 3 by 1,900,
    /// rank 40 by 1,900 (the quarter-window floor), never past the deadline.
    #[test]
    fn every_queued_kind_states_its_due_date() {
        use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
        use kaspa_consensus_core::palw_producer_v2::{PalwDisclosureDutyV1, PalwDisclosureRoleV1};
        let rung = duty(true, PalwBisectTurnV1::AwaitDisclosure);
        assert_eq!(palw_court_move_due_v1(&rung), 1_058);
        assert_eq!(
            palw_court_move_due_v1(&PalwCourtDutyV2 { rung_deadline_daa: 9_000, ..rung.clone() }),
            4_000,
            "inside the backstop"
        );
        assert_eq!(palw_court_move_due_v1(&duty(false, PalwBisectTurnV1::Terminal)), 4_000, "a close by the backstop");
        let answer = |role, signer_rank| PalwDisclosureDutyV1 {
            claim_id: h(1),
            accepted_block: h(2),
            class_id: h(3),
            artifact_root: h(4),
            executor_bond: bond(9),
            discloser: bond(9),
            role,
            signer_rank,
            unit: PalwDaUnitV1::Event { row: 0, tile: 0 },
            deadline_daa: 2_200,
            disclose_window_daa: 1_200,
            in_run_rows: 1,
            trace_root: h(5),
            execution_root: h(6),
            free_prompt: false,
            job_identity: h(7),
        };
        assert_eq!(palw_disclosure_answer_due_v1(&answer(PalwDisclosureRoleV1::Producer, 0)), 2_200);
        for (rank, due) in [(0u16, 1_675u64), (1, 1_750), (3, 1_900), (40, 1_900)] {
            let signer = answer(PalwDisclosureRoleV1::CoveringSigner, rank);
            assert_eq!(palw_disclosure_answer_due_v1(&signer), due, "rank {rank}");
            assert!(palw_disclosure_due_v1(&signer, due), "rank {rank}: its answer is due once its turn has come");
        }
    }

    /// **T-A10, the panel half: the panel files C2, through the held route, before the dense
    /// builder.** In the panel's production code the tick routes a held dissection to this module
    /// BEFORE the dense builder is reached, and after the loop reads the chain the route asks for,
    /// queues its moves with their due dates and starts the builds the ledger admits; this module's
    /// route is fenced on `palw_offence_attribution`, builds its evidence through the windowed verb
    /// alone (N1 with no filing, N2 with a filing that stands the fold's own checks), files move 1 as
    /// the held form the builder makes (`root_claim_held_v1`, tag 57, signed under the responder's
    /// context over the root), answers the responder's material through P2-7's loader, and holds every
    /// build and every evidence under the one ledger. The priority lane sorts EDF inside
    /// `carry_priority_v1`. Red if any of it is re-spelled around.
    #[test]
    fn t_a10_the_panel_routes_a_held_dissection_to_the_windowed_builders_and_files_c2() {
        let panel = include_str!("../palw_panel.rs");
        let production = &panel[..panel.find("#[cfg(test)]\nmod tests {").expect("the unit tests follow the code")];
        let begin =
            production.find("held_court.begin_tick_v1(&court_duties, current_daa).await;").expect("the tick collects held builds");
        let route = production.find("if held_court.routes_v1(&held_host, duty, current_daa) {").expect("the held route");
        let pushed = production[route..].find("held_duties.push(duty.clone());").expect("routed") + route;
        let dense_resolve = production[route..]
            .find("let mut backend = match self.resolve_backend(&session, duty.class_id, duty.artifact_root)")
            .expect("the dense route's backend")
            + route;
        let dense = production.find("b.attn_site_evidence(").expect("the dense builder");
        let reads = production.find("held_court.chain_reads_v1(&held_host, &held_duties, current_daa)").expect("the chain reads");
        let moves = production
            .find("held_court::palw_held_moves_v1(&held_host, &mut held_court, &held_duties, current_daa, busy)")
            .expect("the moves");
        let dated = production[moves..].find("court_due.insert(queued.key, queued.due);").expect("each with its due date") + moves;
        let started =
            production.find("held_court::palw_held_start_builds_v1(&held_host, &mut held_court, current_daa);").expect("builds");
        assert!(begin < route && route < pushed && pushed < dense_resolve && dense_resolve < dense, "routed before the dense builder");
        assert!(production[pushed..pushed + 120].contains("continue;"), "the held route ends the duty's iteration");
        assert!(dense < reads && reads < moves && moves < dated && dated < started, "read, move, date, build — after the loop");
        let reader = production.find("fn attn_held_objects_from_chain_v1(").expect("the held reader");
        assert!(reader > production.find("fn attn_root_filings_from_chain_v1(").expect("the anchored reader"), "beside it");
        assert!(production.contains("PalwAttnHeldFilingV1::from_object_v1(object)"), "the sub-roots read off the object");
        let carry = &production[production.find("    async fn carry_priority_v1(").expect("the priority lane")..];
        let carry = &carry[..carry.find("\n    }\n").expect("its end")];
        let sorted = carry.find("palw_court_queue_edf_v1(court_pending, court_due);").expect("EDF");
        assert!(
            sorted
                < carry.find("for (session_id, round, mine_is_responder, object) in std::mem::take(court_pending)").expect("the loop")
        );

        let module = include_str!("held_court.rs");
        let module = &module[..module.find("#[cfg(test)]\nmod tests {").expect("this module")];
        assert!(module.contains("offence_attribution_active && class_is_held && duty.fused_class && duty.terminal_index.is_some()"));
        assert!(
            module.contains("self.panel.consensus_config.params.palw_offence_attribution_active_at(daa)"),
            "fenced at the tick's DAA"
        );
        assert!(!module.contains(".attn_site_evidence(") && !module.contains(".attn_site_evidence_from_filing("), "no dense builder");
        assert!(module.contains(".attn_site_evidence_held_v1(capture, narrowed, carried, None)"), "N1: the responder, no filing");
        assert!(
            module.contains(".attn_site_evidence_held_v1(&[], narrowed, carried, Some(filing))"),
            "N2: the challenger, the filing"
        );
        assert!(module.contains("PalwAttnHeldFilingV1::from_object_checked_v1("), "a filing stands the fold's own checks");
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
            top_of(module, "pub(crate) fn palw_held_responder_evidence_v1(")
                .contains("palw_da_material_v1(backend, facts, kept, keep)?")
        );
        assert!(module.contains("role: \"held-dissection\""), "a build under the one ledger");
        assert!(module.contains("role: \"held-evidence\""), "and its evidence");
        assert!(module.contains("tokio::task::spawn_blocking(move ||"), "off the tick");
    }

    fn top_of<'a>(source: &'a str, head: &str) -> &'a str {
        let at = source.find(head).unwrap_or_else(|| panic!("{head}"));
        &source[at..at + source[at..].find("\n}\n").expect("its end")]
    }
}
