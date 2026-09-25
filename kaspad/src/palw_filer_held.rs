//! **ADR-0152 v3.1 Phase 2, P2-8e — held dissections of fused-attention leaves** (node policy; a
//! child of the replay filer, whose book holds each case, run once a tick from
//! [`PalwPanelService::replay_filer_tick_v1`]).
//!
//! **What it closes** (§3.9's garbage row on a held class, DA-3, J-6, F5; T54g). A seat whose replay
//! bisects a claim's served capture to its first divergent step (P2-8b) and finds that step is a
//! fused-attention leaf cannot prove it in one step: the leaf checker returns `NeedsDissection`, and
//! DA-3 refuses such a leaf as a data-availability unit (`DaUnitNeedsDissection`) — "its filer opens a
//! held dissection (the court) instead". Before this module the replay filer's hook said so and filed
//! nothing, so a garbage attention tile on the 8k row cost its producer nothing once X10 arms. Here:
//!
//! 1. **Off the loop, in the run that found the leaf** ([`palw_held_opening_v1`], called from
//!    `palw_replay_filer_run_v1` under the run's own ledger reservation, the local capture dropped
//!    first):
//!    * the ONE-MOVE evidence at the leaf, out of the served capture the bisection verified, on the
//!      claim's lane (P2-8b's recipe: the attempt's `refutation_for_index`; the free prompt's through
//!      the executor's own builder, `palw_leaf_evidence_from_capture_v1`), stripped of its history as
//!      the bound verdict requires (`for_the_one_move_v2`), and held to the fold's own verdict
//!      (`verdict_at_v2`): `NeedsDissection` is the object the fold defers to a held dissection
//!      (`open_held_dissection_v1`), `ExecutorGuilty` convicts in the one move itself;
//!    * the accused's filing as the served capture implies it — the windowed responder on the served
//!      capture (N1), its held root claim read back through the fold's own checks
//!      (`PalwAttnHeldFilingV1::from_object_checked_v1`); every field of it is pinned to a root the
//!      claim committed, so it is the filing any root claim the producer can land will carry;
//!    * the CHALLENGER's evidence against that filing, by the A-held line's builder
//!      [`palw_held_challenger_evidence_v1`] (N2: this seat's own replay to the anchor, the accused's
//!      query row) — exactly what the held route builds again once the chain holds the filing;
//!    * **the court's kernels decide** ([`palw_held_tile_is_a_lie_v1`]): the honest root over N2's
//!      history finalizes to a tile other than the committed one. Only then is anything opened. A
//!      producer's root claim must finalize to its committed tile (`RootDoesNotFinalize`), so its
//!      root is not N2's and the held route names the first child that is not; the dissection ends at
//!      a bottom that convicts (`CourtHeldVerdict`, reason 8, F3 (B)) or at 4-ter.3 step 6. **An
//!      honest producer is never dissected by an honest seat**: its committed tile IS the kernels'
//!      function of its committed history, so N2 reproduces it — whatever this seat's dense replay
//!      did — and the case settles ([`PalwHeldOpeningV1::Reproduces`]).
//! 2. **On the loop** ([`palw_held_dissections_step_v1`]): the `ShardCourtAccused` at the leaf is
//!    built and signed ([`palw_held_opening_object_v1`], the capture arm's recipe), asked of the fold
//!    before a carrier is paid for (`palw_object_rehearsal_v1`: the acceptance layer and the object's
//!    own arm on the tip — C3's seat rule, C4's reservation, the court's capacity, the claim's phase)
//!    and queued on `court_pending` under the accusation's own key `(accusation id, 0, false)` — the
//!    key the capture arm and the named-leaf pursuit file the same object under, so one accusation is
//!    one queue entry whoever builds it — due by [`palw_court_accusation_due_v1`] (the claim's `Final`
//!    less the landing margin; the audit's MED-4 rule for `ShardCourtAccused`, one helper).
//! 3. **The session is the held route's** (`held_court`): whose turn it is, the moves, their
//!    deadlines — the root claim's rung is the class's compute turn (`palw_held_move_turn_daa_v1`, F5)
//!    which the fold stamps and the node reads off the session, never a number here. This module only
//!    notices that the chain opened it and stands aside.
//!
//! **One dissection per `(claim, leaf)`**: in the book (a case has one step), across filers (the
//! queue key), across restarts ([`PalwHeldDissectionsV1`] is REBUILT from the chain's court duties
//! every tick: a held dissection this bond challenges is known the first tick after a restart, and no
//! replay case is noted for its claim), and by the fold (C3: one session per seat on a claim).
//!
//! **Missing material** ([`PalwHeldOpeningV1::Missing`]): where the served material does not yield
//! the opening's evidence, the case falls back to P2-8d's `StepLeaf` demand of that leaf — the case's
//! ONE step, so never both for one leaf — which asks the fold's own gates and the builder's stateless
//! halves before anything is signed. DA-3 refuses a fused leaf as a unit, so on today's fold that
//! demand is refused before it is built (`NeedsDissection`), and the case settles with the fold's
//! reason: nothing is built that the fold refuses.
//!
//! **What a held class serves, today** (the gap T54g's e2e pins,
//! `t54g_gap_a_fresh_seat_cannot_bisect_a_lying_held_fold`). A held class retains a FOLD, and base0
//! reads a fold below its retained level only by an honest re-execution — which does not reproduce a
//! lying fold's roots. So a fresh seat verifies a lying fold (its roots come off the retained tree)
//! but the bisection's first rung is `Unreadable` on the served side, and neither P2-8b nor this
//! module reaches the leaf: the run settles with nothing filed. Everything here runs as soon as the
//! seat can read the served capture at the lie — a retention it reads whole, or the fold read at its
//! committed leaves (a `StepRange` disclosure of the divergent block, the annex lane's block leaves)
//! and the fused tile disclosed; the e2e plays exactly that, with the leaves proven to root to the
//! fold's committed tree.
//!
//! **Fences.** Dormant below `palw_rcore_plus` (the replay filer's own gate). Below
//! `palw_offence_attribution`, for a class the chain does not record as held, or for one whose held
//! dissection is unanswerable (C5, the 2M row), the run builds no opening
//! ([`PalwPanelService::held_opening_ctx_v1`] is `None`) and the fused finding settles as before.

use super::*;
use kaspa_consensus_core::palw_attn_responder_v1::{PalwAttnHeldEvidenceV1, PalwAttnHeldFilingV1};
use kaspa_consensus_core::palw_producer_v2::{PalwCourtDutyV2, PalwObjectRehearsalV1};
use kaspa_consensus_core::palw_shard_court_v1::{PalwLeafEvidenceV1, PalwOneMoveClaimV2, PalwShardCourtVerdictV1};
use kaspa_consensus_core::palw_state_v2::PalwStateV2Error;

use super::super::held_court::{PalwCourtQueueKeyV1, palw_held_challenger_evidence_v1};

#[cfg(test)]
#[path = "palw_filer_held_e2e.rs"]
mod e2e;

/// Carriers one opening may take: the first, and one more when the first was lost (P2-8b's rule).
pub(super) const PALW_HELD_OPENING_SENDS_V1: u8 = 2;

/// **The court-queue round of an opening** — `(accusation id, 0, false)`, the key the capture arm
/// and the named-leaf pursuit queue a `ShardCourtAccused` under. The accusation's id binds the
/// network, the claim, its roots, both bonds and the leaf and NOT the refutation's bytes
/// (`palw_shard_court_session_id_v1`), so one seat's accusation of one leaf is one key whichever filer
/// built it.
pub(super) const PALW_HELD_OPENING_QUEUE_ROUND_V1: u32 = 0;

/// The dissection arity a pre-opening root claim is read back with. The reader
/// (`from_object_checked_v1`) does not check it; the node's real root claims declare the acceptance
/// layer's (`palw_court_params_held_at_v2`).
const PALW_HELD_REHEARSAL_ARITY_V1: u8 = 2;

/// **The DAA a court accusation of a claim is due by, on the priority lane** (EDF,
/// `palw_court_queue_edf_v1`): the claim's current phase end — its `Final` for a licensed claim —
/// less the landing margin an automatic accusation keeps (`PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1`). The
/// audit's MED-4 gives `ShardCourtAccused` and the named-leaf pursuit this due date; the held
/// dissection's opening is a `ShardCourtAccused`, so it is dated by the same ONE helper and the lane
/// orders the three alike. Saturating: an unknown phase end (`u64::MAX`) is the last dated place.
pub(crate) fn palw_court_accusation_due_v1(phase_end_daa: u64) -> u64 {
    phase_end_daa.saturating_sub(PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1)
}

/// **What a run needs to build a held opening**, resolved on the loop at its start
/// ([`PalwPanelService::held_opening_ctx_v1`]): the class's artifact root, the cap the court opens a
/// held site's rows under (the claim's ladder under the held regime), the ladder the chain adjudicates
/// the one move at, and whether the bound verdict is in force (`palw_audit_2026_09_23`: a fused leaf
/// is carried without its history).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PalwHeldOpeningCtxV1 {
    pub artifact_root: Hash64,
    pub opening_cap: u64,
    pub ladder: u64,
    pub bound: bool,
}

/// **The served capture a fused finding was made on**, handed out of the bisection to the opening:
/// the bytes, the roots they verified against, and the prompt the free-prompt lane carries (the
/// payload's ids; `None` for an attempt, whose prompt every builder re-derives from its anchor).
pub(crate) struct PalwHeldServedV1 {
    pub capture: Vec<u8>,
    pub roots: kaspa_consensus_core::palw_backend::PalwClaimRootsV1,
    pub carried: Option<Vec<u32>>,
}

/// **The one move's evidence at `leaf`, off the served capture, on the claim's lane** — P2-8b's
/// recipe (`refutation_at_v1`: the attempt lane's `refutation_for_index`, the free-prompt lane's with
/// the carried ids — here the executor's own builder, `palw_leaf_evidence_from_capture_v1`, which
/// tries a fold's own path first), the artifact rows it reads, and the prompt tile in the network's
/// carriage.
fn palw_held_leaf_evidence_v1(
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    served: &PalwHeldServedV1,
    leaf: u64,
    work_leaves: u64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<PalwLeafEvidenceV1, String> {
    if let Some(ids) = served.carried.as_deref() {
        // `output_root: None`: a leaf's evidence proves a step leaf against the execution root.
        let roots = kaspa_consensus_core::palw_backend::PalwClaimRootsV1 { output_root: None, ..served.roots };
        return kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
            backend,
            &served.capture,
            ids,
            roots,
            work_leaves,
            leaf,
            form,
        );
    }
    let refutation = backend.refutation_for_index(&served.capture, leaf)?;
    let artifact_openings = backend.operand_openings_for(&refutation)?;
    let (refutation, prompt_ids_opening) =
        kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1(form, refutation)
            .map_err(|e| format!("the prompt tile does not open: {e}"))?;
    Ok(PalwLeafEvidenceV1 { refutation, artifact_openings, prompt_ids_opening })
}

/// **What a run made of a fused finding** ([`palw_held_opening_v1`]).
#[derive(Debug)]
pub(crate) enum PalwHeldOpeningV1 {
    /// Open the held dissection: the one move's evidence at the leaf (stripped where the bound verdict
    /// requires), which the fold defers to the dissection — or convicts on outright. `route` says
    /// where the bottom will be found, for the log.
    Open { evidence: Box<PalwLeafEvidenceV1>, route: &'static str },
    /// The court's kernels reproduce the committed tile from the accused's own filing and this seat's
    /// history: nothing is opened (an honest producer is never dissected).
    Reproduces,
    /// The served material does not yield the opening's evidence: P2-8d's `StepLeaf` demand is the
    /// case's one fallback.
    Missing(String),
    /// This seat cannot play the dissection (its own N2 does not build, or the kernels cannot read the
    /// site): nothing is opened — a challenger that cannot move loses C4's charge.
    Unplayable(String),
}

/// A node-local session id a pre-opening root claim is built and read back under (the reader checks
/// only that the object names it). Keyed, so it is no chain session's id.
fn palw_held_rehearsal_session_v1(claim: &Hash64, leaf: u64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka-node/held-opening/v1").to_state();
    state.update(claim.as_byte_slice());
    state.update(&leaf.to_le_bytes());
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **The accused's held filing, as its served capture implies it** — the windowed responder on the
/// served capture (N1, no filing), its held root claim built and read back through the fold's own
/// checks of a filed one (`from_object_checked_v1`: the binding the claim's, the held map, C-01, the
/// out tile the narrowed leaf, the operands, the anchor the site's, the sub-roots rooting to it).
/// What it returns is the claim's own commitments at the site, which every root claim the producer
/// can land carries too. N1's resident evidence is dropped on return.
fn palw_held_filing_of_served_v1(
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    served: &PalwHeldServedV1,
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    leaf: u64,
    ctx: &PalwHeldOpeningCtxV1,
) -> Result<PalwAttnHeldFilingV1, String> {
    let responder = backend
        .attn_site_evidence_held_v1(&served.capture, leaf, served.carried.as_deref(), None)
        .map_err(|e| format!("the windowed responder on the served capture (N1): {e}"))?;
    let site =
        responder.site_v1(ctx.artifact_root, false, ctx.opening_cap).map_err(|e| format!("the served capture's held site: {e}"))?;
    let session = palw_held_rehearsal_session_v1(&target.claim_id, leaf);
    let object = responder
        .root_claim_held_v1(&site, session, PALW_HELD_REHEARSAL_ARITY_V1)
        .map_err(|e| format!("the served capture's held root claim: {e}"))?;
    PalwAttnHeldFilingV1::from_object_checked_v1(
        &object,
        &session,
        target.execution_root,
        target.class_id,
        ctx.artifact_root,
        leaf,
        ctx.opening_cap,
    )
    .map_err(|e| format!("the served capture's filing does not stand the fold's checks: {e}"))
}

/// **Does the court's own composition say the committed tile is not the attention of its history?**
/// The honest root over `challenger`'s inputs (`root_claim_v1`: `a16_attn_root_claim_v1`, the court's
/// kernel) finalized, against the committed output tile the filing opens
/// (`palw_attn_opened_lanes_v1`, the fold's reading at move 1). `true`: no root claim the accused can
/// land (one that finalizes to its tile, `RootDoesNotFinalize` otherwise) is this recompute's, so the
/// held route has a child to name.
pub(super) fn palw_held_tile_is_a_lie_v1(
    challenger: &PalwAttnHeldEvidenceV1,
    filing: &PalwAttnHeldFilingV1,
    artifact_root: Hash64,
    opening_cap: u64,
) -> Result<bool, String> {
    let site = challenger.site_v1(artifact_root, false, opening_cap).map_err(|e| format!("the challenger's held site: {e}"))?;
    let root = challenger.evidence.root_claim_v1(&site).map_err(|e| format!("the challenger's honest root: {e}"))?;
    let committed = kaspa_consensus_core::palw_attn_court_v1::palw_attn_opened_lanes_v1(
        &filing.out_tile,
        &site.binding,
        site.head_lanes.2 as usize,
    )
    .map_err(|e| format!("the committed tile does not open: {e:?}"))?;
    Ok(kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(&root.claim.v_acc, site.site.params.values) != committed)
}

/// **P2-8e's opening, off the loop** — see the module's header, step 1. `work_leaves` is the served
/// capture's leaf count (the bisection's `n`); `form` the class's prompt-id form.
pub(super) fn palw_held_opening_v1(
    backend: &dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1,
    served: &PalwHeldServedV1,
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    leaf: u64,
    work_leaves: u64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ctx: &PalwHeldOpeningCtxV1,
) -> PalwHeldOpeningV1 {
    // 1. The one move's evidence at the leaf, off the served capture.
    let evidence = match palw_held_leaf_evidence_v1(backend, served, leaf, work_leaves, form) {
        Ok(evidence) => evidence,
        Err(why) => return PalwHeldOpeningV1::Missing(format!("the served capture does not open leaf {leaf}'s evidence: {why}")),
    };
    let evidence = if ctx.bound { evidence.for_the_one_move_v2() } else { evidence };
    let bound_to =
        PalwOneMoveClaimV2 { execution_root: target.execution_root, class_id: target.class_id, artifact_root: ctx.artifact_root };
    match evidence.verdict_at_v2(&bound_to, ctx.ladder, ctx.bound) {
        Ok(PalwShardCourtVerdictV1::NeedsDissection) => {}
        // The one move's own arithmetic convicts (the unbound verdict reads the history): no
        // dissection is needed — the same accusation convicts at the fold.
        Ok(PalwShardCourtVerdictV1::ExecutorGuilty) => {
            return PalwHeldOpeningV1::Open { evidence: Box::new(evidence), route: "the one move convicts outright" };
        }
        Ok(PalwShardCourtVerdictV1::FalseAccusation) => return PalwHeldOpeningV1::Reproduces,
        Err(e) => return PalwHeldOpeningV1::Missing(format!("leaf {leaf}'s evidence does not adjudicate as the fold reads it: {e}")),
    }
    // 2. The accused's filing, as its served capture implies it.
    let filing = match palw_held_filing_of_served_v1(backend, served, target, leaf, ctx) {
        Ok(filing) => filing,
        Err(why) => return PalwHeldOpeningV1::Missing(why),
    };
    // 3. N2 — the challenger's evidence, the A-held line's builder.
    let challenger = match palw_held_challenger_evidence_v1(backend, &filing, leaf, served.carried.as_deref()) {
        Ok(challenger) => challenger,
        Err(why) => return PalwHeldOpeningV1::Unplayable(why),
    };
    // 4. The court's kernels decide.
    match palw_held_tile_is_a_lie_v1(&challenger, &filing, ctx.artifact_root, ctx.opening_cap) {
        Ok(true) => {
            // Where the bottom is: this seat's own sub-root of the site's layer is the filed one (a
            // bottom on the filing), or not (4-ter.3 step 6: the checkpoint disagrees with the rows
            // before the lie — the StateChunk demand, then `CheckpointAccused`).
            let step6 = challenger
                .site_v1(ctx.artifact_root, true, ctx.opening_cap)
                .ok()
                .and_then(|anchored| challenger.fallback_v1(&anchored).ok().flatten())
                .is_some();
            let route = if step6 { "4-ter.3 step 6 (the anchor disagrees with this seat's rows)" } else { "a bottom on the filing" };
            PalwHeldOpeningV1::Open { evidence: Box::new(evidence), route }
        }
        Ok(false) => PalwHeldOpeningV1::Reproduces,
        Err(why) => PalwHeldOpeningV1::Unplayable(why),
    }
}

/// **The held dissection's opening, built and signed on the loop** — the capture arm's recipe: the
/// accusation from the evidence (every content field the evidence's), its shape at the ladder the
/// chain adjudicates the one move at, the accuser's ML-DSA-87 over the accusation id under
/// `PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT`, the ride rule. Returns its court-queue key
/// (`(accusation id, 0, false)`) and the object. Never against this node's own bond.
pub(super) fn palw_held_opening_object_v1(
    evidence: &PalwLeafEvidenceV1,
    duty: &PalwSeatDutyV2,
    accuser: PalwBondKeyV2,
    network_domain: &Hash64,
    ladder: u64,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<(PalwCourtQueueKeyV1, PalwConsensusObjectV2), String> {
    use kaspa_consensus_core::palw_shard_court_v1::{PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, palw_shard_court_session_id_v1};
    if duty.executor_bond == accuser {
        return Err("the claim is this node's own".to_string());
    }
    let mut accusation =
        evidence.clone().into_accusation_v1(duty.claim_id, duty.execution_root, duty.trace_root, duty.executor_bond, accuser);
    accusation.validate_shape(ladder).map_err(|e| format!("the accusation's shape is refused locally: {e}"))?;
    let id = palw_shard_court_session_id_v1(network_domain.as_byte_slice(), &accusation);
    accusation.signature = sign(id.as_byte_slice(), PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT)
        .filter(|signature| !signature.is_empty())
        .ok_or("no signing key for the accusation")?;
    let object = PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
        .map_err(|why| format!("the accusation cannot ride a carrier: {why}"))?;
    Ok(((id, PALW_HELD_OPENING_QUEUE_ROUND_V1, false), object))
}

/// **What the seat does with the fold's answer to its opening** — the fold's own reasons, sorted by
/// whether a wait can change them: C4's room (A-6's free half frees as sessions close), the court's
/// capacity, C3's further-session rule (a non-seat waits for the open session to end; this seat's own
/// session is noticed off the chain) and a duplicate session wait a re-plan; every other refusal —
/// the claim terminal or gone, the roots, the verdict, an unanswerable class, the bond's standing,
/// the acceptance layer's — settles the case.
pub(super) fn palw_held_rehearsal_step_v1(rehearsal: &PalwObjectRehearsalV1) -> PalwSeatAccuseStepV1 {
    match rehearsal {
        PalwObjectRehearsalV1::Accepted => PalwSeatAccuseStepV1::File,
        PalwObjectRehearsalV1::Refused(
            PalwStateV2Error::AccusationExposureCeiling { .. }
            | PalwStateV2Error::CourtSessionsAtCapacity { .. }
            | PalwStateV2Error::HeldDissectionFurtherSessionRefused { .. }
            | PalwStateV2Error::DuplicateSession(_),
        ) => PalwSeatAccuseStepV1::Retry,
        PalwObjectRehearsalV1::Refused(_) | PalwObjectRehearsalV1::NotAccepted(_) => PalwSeatAccuseStepV1::Settle,
    }
}

/// **One held dissection this seat's replay found and is opening** — the replay case's step
/// (`PalwReplayCaseStepV1::Dissect`) from the run's `Open` until the chain opens the session or the
/// fold refuses it for good.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PalwHeldDissectV1 {
    pub leaf: u64,
    /// The one move's evidence at the leaf (stripped where the bound verdict requires).
    pub evidence: Box<PalwLeafEvidenceV1>,
    /// The signed accusation and its court-queue key, once built.
    pub filed: Option<(PalwCourtQueueKeyV1, PalwConsensusObjectV2)>,
    /// Carriers queued ([`PALW_HELD_OPENING_SENDS_V1`] at most), and the DAA of the last.
    pub sends: u8,
    pub queued_at: u64,
    /// The DAA the fold was last asked about the queued object (asked again a re-plan later).
    pub asked_at: Option<u64>,
    /// The DAA the fold last asked the seat to wait.
    pub refused_at: Option<u64>,
}

impl PalwHeldDissectV1 {
    pub(super) fn new(leaf: u64, evidence: Box<PalwLeafEvidenceV1>) -> Self {
        Self { leaf, evidence, filed: None, sends: 0, queued_at: 0, asked_at: None, refused_at: None }
    }
}

/// **The held dissections this bond challenges, as the chain shows them — rebuilt every tick**
/// (the restart rebuild: nothing here is persisted, and the first tick after a restart knows every
/// open one). `open`: `(claim, leaf)` → the court session, read off this bond's court duties (a held
/// fused session it challenges). `done`: a `(claim, leaf)` whose session this node saw and that has
/// since closed — convicted (the claim is gone from every duty anyway), acquitted, or defaulted —
/// kept for a case's window so the standing replay trigger never opens it again.
#[derive(Default)]
pub(super) struct PalwHeldDissectionsV1 {
    open: BTreeMap<(Hash64, u64), Hash64>,
    done: BTreeMap<(Hash64, u64), u64>,
}

impl PalwHeldDissectionsV1 {
    /// **Read the chain's court duties** — every held dissection this bond challenges at a fused leaf
    /// the ladder names (what `open_held_dissection_v1` opens). A session gone since the last read is
    /// `done` from `current_daa`; one seen again is open again (a transiently empty read, a reorg).
    /// Returns the sessions seen for the first time: `(claim, leaf, session, the DAA the producer's
    /// root claim is due by)` — the rung the fold stamped (the class's compute turn, F5).
    pub(super) fn observe_v1(
        &mut self,
        court_duties: &[PalwCourtDutyV2],
        bond: &PalwBondKeyV2,
        current_daa: u64,
    ) -> Vec<(Hash64, u64, Hash64, u64)> {
        let now: BTreeMap<(Hash64, u64), (Hash64, u64)> = court_duties
            .iter()
            .filter(|duty| !duty.i_am_responder && duty.challenger_bond == *bond && duty.fused_class)
            .filter_map(|duty| duty.terminal_index.map(|leaf| ((duty.claim_id, leaf), (duty.session_id, duty.rung_deadline_daa))))
            .collect();
        for key in self.open.keys() {
            if !now.contains_key(key) {
                self.done.insert(*key, current_daa);
            }
        }
        let fresh = now
            .iter()
            .filter(|(key, _)| !self.open.contains_key(key))
            .map(|((claim, leaf), (session, rung))| (*claim, *leaf, *session, *rung))
            .collect();
        for key in now.keys() {
            self.done.remove(key);
        }
        self.open = now.into_iter().map(|(key, (session, _))| (key, session)).collect();
        self.done.retain(|_, at| current_daa <= at.saturating_add(super::PALW_REPLAY_FILER_CASE_DAA_V1));
        fresh
    }

    /// Whether this bond holds, or held within a case's window, a held dissection on `claim`.
    pub(super) fn claim_v1(&self, claim: &Hash64) -> bool {
        self.open.keys().chain(self.done.keys()).any(|(c, _)| c == claim)
    }

    /// Whether this bond holds, or held within a case's window, the held dissection at `(claim, leaf)`.
    pub(super) fn dissected_v1(&self, claim: &Hash64, leaf: u64) -> bool {
        self.open.contains_key(&(*claim, leaf)) || self.done.contains_key(&(*claim, leaf))
    }
}

/// **What the loop's half needs of the node** — the panel over its session
/// ([`PalwPanelHeldFilerHostV1`]); a test over a fixture fold, so the whole step runs without a node.
pub(super) trait PalwHeldFilerHostV1 {
    fn network_domain(&self) -> Hash64;
    /// The ladder the chain adjudicates a one-move accusation of `class_id` at, at `daa`.
    fn accusation_ladder(&self, class_id: Hash64, daa: u64) -> u64;
    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>>;
    /// The fold's answer to `object` in the next block (`palw_object_rehearsal_v1`); `None`: no tip.
    fn rehearse(&self, object: &PalwConsensusObjectV2) -> Option<PalwObjectRehearsalV1>;
    /// The DAA `claim`'s current phase ends by itself — a licensed claim's `Final` — `None` unknown.
    fn phase_end(&self, claim: &Hash64) -> Option<u64>;
}

/// **The loop's half, once a tick** — see the module's header, steps 2 and 3:
///
/// 1. the chain's held dissections, rebuilt ([`PalwHeldDissectionsV1::observe_v1`]); a case whose
///    `(claim, leaf)` the chain holds or held settles — the session is the held route's, and its
///    queued opening (the fold would refuse a second) leaves the queue;
/// 2. each other `Dissect` case: the object built and signed once; a queued one asked again a re-plan
///    after the last ask (refused → out of the queue: waited or settled); an unqueued one — never
///    sent, or its carrier lost (sent [`super::PALW_REPLAY_FILER_REFILE_DAA_V1`] ago, or dropped by
///    the lane) — asked of the fold and queued with its due date while a send is left.
///
/// Returns the openings queued this tick.
#[allow(clippy::too_many_arguments)]
pub(super) fn palw_held_dissections_step_v1<H: PalwHeldFilerHostV1>(
    host: &H,
    filer: &mut PalwReplayFilerV1,
    current_daa: u64,
    bond: PalwBondKeyV2,
    court_duties: &[PalwCourtDutyV2],
    court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    court_due: &mut HashMap<PalwCourtQueueKeyV1, u64>,
    court_moved: &HashMap<PalwCourtQueueKeyV1, u64>,
) -> usize {
    for (claim, leaf, session, rung) in filer.held.observe_v1(court_duties, &bond, current_daa) {
        info!(
            "[{PALW_PANEL}] claim {claim}: this bond's held dissection at fused leaf {leaf} is open (session {session}); the \
             producer's held root claim is due by DAA {rung}, the rung the chain stamped — the held route plays it from here \
             (ADR-0152 §4-ter, F5, P2-8e)"
        );
    }
    let fresh = |at: Option<u64>| at.is_some_and(|at| current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA));
    let dissecting: Vec<Hash64> = filer
        .cases
        .iter()
        .filter(|(_, case)| matches!(case.step, PalwReplayCaseStepV1::Dissect(_)))
        .map(|(claim, _)| *claim)
        .collect();
    let mut queued_now = 0;
    for claim in dissecting {
        let Some(case) = filer.cases.get_mut(&claim) else { continue };
        let PalwReplayCaseStepV1::Dissect(dissect) = &mut case.step else { continue };
        // 1. The chain holds (or held) this dissection: the held route's from here.
        if filer.held.dissected_v1(&claim, dissect.leaf) {
            if let Some((key, _)) = &dissect.filed {
                court_pending.retain(|(a, b, c, _)| (*a, *b, *c) != *key);
            }
            filer.settle_at(&claim, current_daa);
            continue;
        }
        // 2. Built and signed once.
        if dissect.filed.is_none() {
            let ladder = host.accusation_ladder(case.duty.class_id, current_daa);
            match palw_held_opening_object_v1(&dissect.evidence, &case.duty, bond, &host.network_domain(), ladder, |m, c| {
                host.sign(m, c)
            }) {
                Ok(filed) => dissect.filed = Some(filed),
                Err(why) => {
                    warn!(
                        "[{PALW_PANEL}] claim {claim}: the held dissection's opening at leaf {} cannot be built: {why}",
                        dissect.leaf
                    );
                    filer.settle_at(&claim, current_daa);
                    continue;
                }
            }
        }
        let (key, object) = dissect.filed.clone().expect("built above");
        let due = palw_court_accusation_due_v1(host.phase_end(&claim).unwrap_or(u64::MAX));
        let mut settle = false;
        if queued_v1(court_pending, key) {
            // Queued (by this case, or the same accusation by another filer): the fold asked again a
            // re-plan after the last ask, and the lane told its due date.
            court_due.entry(key).and_modify(|at| *at = (*at).min(due)).or_insert(due);
            if fresh(dissect.asked_at) {
                continue;
            }
            dissect.asked_at = Some(current_daa);
            match host.rehearse(&object).map(|r| (palw_held_rehearsal_step_v1(&r), r)) {
                None | Some((PalwSeatAccuseStepV1::File, _)) => {}
                Some((PalwSeatAccuseStepV1::Retry, r)) => {
                    info!("[{PALW_PANEL}] claim {claim}: the held dissection's opening waits — the fold: {r:?} (P2-8e)");
                    court_pending.retain(|(a, b, c, _)| (*a, *b, *c) != key);
                    dissect.sends = dissect.sends.saturating_sub(1);
                    dissect.refused_at = Some(current_daa);
                }
                Some((PalwSeatAccuseStepV1::Settle, r)) => {
                    info!("[{PALW_PANEL}] claim {claim}: the held dissection's opening settles — the fold: {r:?} (P2-8e)");
                    court_pending.retain(|(a, b, c, _)| (*a, *b, *c) != key);
                    settle = true;
                }
            }
        } else {
            let in_flight =
                court_moved.get(&key).is_some_and(|sent| current_daa < sent.saturating_add(super::PALW_REPLAY_FILER_REFILE_DAA_V1));
            // Never sent and not queued: the lane dropped it (the mempool or the carrier builder
            // refused it) — asked again only a re-plan after it was queued.
            let dropped_early = dissect.sends > 0 && !court_moved.contains_key(&key) && fresh(Some(dissect.queued_at));
            if in_flight || dropped_early || dissect.sends >= PALW_HELD_OPENING_SENDS_V1 || fresh(dissect.refused_at) {
                continue;
            }
            dissect.asked_at = Some(current_daa);
            match host.rehearse(&object).map(|r| (palw_held_rehearsal_step_v1(&r), r)) {
                None => {}
                Some((PalwSeatAccuseStepV1::File, _)) => {
                    info!(
                        "[{PALW_PANEL}] claim {claim}: opening the held dissection at fused leaf {} (ShardCourtAccused, accusation \
                         {}) — due by DAA {due} (ADR-0152 DA-3, §4-ter, P2-8e)",
                        dissect.leaf, key.0
                    );
                    court_pending.push((key.0, key.1, key.2, object));
                    court_due.insert(key, due);
                    dissect.sends += 1;
                    dissect.queued_at = current_daa;
                    queued_now += 1;
                }
                Some((PalwSeatAccuseStepV1::Retry, r)) => {
                    crate::palw_backends::note_throttled_v1("panel-held-opening-wait", || {
                        format!("[{PALW_PANEL}] claim {claim}: the held dissection's opening waits — the fold: {r:?} (P2-8e)")
                    });
                    dissect.refused_at = Some(current_daa);
                }
                Some((PalwSeatAccuseStepV1::Settle, r)) => {
                    info!("[{PALW_PANEL}] claim {claim}: the held dissection's opening settles — the fold: {r:?} (P2-8e)");
                    settle = true;
                }
            }
        }
        if settle {
            filer.settle_at(&claim, current_daa);
        }
    }
    queued_now
}

/// **What a finished run's fused finding does to its case** (the pure half of `on_run_v1`'s arm).
/// `on_host_failure` is the book's answer to a failure that may be this host's (the claim's second
/// run, or a settle when none is left): what an unplayable opening gets — the challenger's own replay
/// to the anchor did not build, which a second run may.
pub(super) fn palw_held_next_of_opening_v1(
    held: &PalwHeldDissectionsV1,
    claim: Hash64,
    leaf: u64,
    binding: Box<kaspa_consensus_core::palw_step_leg::PalwStepBindingV2>,
    rungs: u32,
    opening: PalwHeldOpeningV1,
    on_host_failure: PalwReplayNextV1,
) -> PalwReplayNextV1 {
    match opening {
        PalwHeldOpeningV1::Open { .. } if held.dissected_v1(&claim, leaf) => {
            info!(
                "[{PALW_PANEL}] claim {claim}: this bond's held dissection at leaf {leaf} is already on chain — nothing more (P2-8e)"
            );
            PalwReplayNextV1::Settle
        }
        PalwHeldOpeningV1::Open { evidence, route } => {
            info!(
                "[{PALW_PANEL}] claim {claim}: the first divergent step is fused-attention leaf {leaf} ({rungs} bisection rungs), and \
                 the court's kernels on the accused's own filing say its tile is not the attention of its history — opening a held \
                 dissection there; its bottom: {route} (ADR-0152 DA-3, §4-ter, P2-8e)"
            );
            PalwReplayNextV1::Step(PalwReplayCaseStepV1::Dissect(Box::new(PalwHeldDissectV1::new(leaf, evidence))))
        }
        PalwHeldOpeningV1::Reproduces => {
            info!(
                "[{PALW_PANEL}] claim {claim}: this seat's replay parts from the claim at fused-attention leaf {leaf}, but the \
                 court's kernels reproduce the committed tile from the accused's own filing — nothing is opened (P2-8e)"
            );
            PalwReplayNextV1::Settle
        }
        PalwHeldOpeningV1::Missing(why) => {
            info!(
                "[{PALW_PANEL}] claim {claim}: fused-attention leaf {leaf} — {why}; falling back to the StepLeaf demand of that leaf \
                 (P2-8d), which the fold's gates judge before anything is signed (P2-8e)"
            );
            PalwReplayNextV1::Step(PalwReplayCaseStepV1::Demand { leaf, binding, sends: 0, refused_at: None })
        }
        PalwHeldOpeningV1::Unplayable(why) => {
            warn!(
                "[{PALW_PANEL}] claim {claim}: a held dissection at fused-attention leaf {leaf} is not opened — this seat's own \
                 evidence does not build ({why}); the claim's next run may (P2-8e)"
            );
            on_host_failure
        }
    }
}

/// **The panel as the loop half's host** — its session at the tick, the network domain, and the
/// phase ends read for this tick's openings.
struct PalwPanelHeldFilerHostV1<'a> {
    panel: &'a PalwPanelService,
    session: &'a kaspa_consensusmanager::ConsensusProxy,
    network_domain: Hash64,
    phase_ends: HashMap<Hash64, u64>,
}

impl PalwHeldFilerHostV1 for PalwPanelHeldFilerHostV1<'_> {
    fn network_domain(&self) -> Hash64 {
        self.network_domain
    }

    fn accusation_ladder(&self, class_id: Hash64, daa: u64) -> u64 {
        self.panel.held_accusation_ladder_v1(class_id, daa)
    }

    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>> {
        self.panel.sign(message, context)
    }

    fn rehearse(&self, object: &PalwConsensusObjectV2) -> Option<PalwObjectRehearsalV1> {
        self.session.palw_object_rehearsal_v1(object)
    }

    fn phase_end(&self, claim: &Hash64) -> Option<u64> {
        self.phase_ends.get(claim).copied()
    }
}

impl PalwPanelService {
    /// **The ladder the chain adjudicates a one-move accusation of `class_id` at**, as the capture
    /// arm files one: the network's refutation cap at `daa`, raised to the class's own held ladder
    /// past `palw_offence_attribution` (`seat_refutation_ladder_v1`).
    pub(super) fn held_accusation_ladder_v1(&self, class_id: Hash64, daa: u64) -> u64 {
        let cap = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
            &self.config.court,
            self.consensus_config.params.palw_court_ladder.is_some_and(|f| f.is_active(daa)),
        );
        self.seat_refutation_ladder_v1(class_id, cap, daa)
    }

    /// **Whether a run of `class_id` builds a held opening, and under what** — `Some` past
    /// `palw_offence_attribution` with the held regime in force, for a class the chain records as held
    /// whose dissection is answerable (C5: the 2M row's is refused at the fold, so it is not built);
    /// `None` everywhere else, where a fused finding settles as before (the fence-off twin).
    pub(super) fn held_opening_ctx_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        class_id: Hash64,
        artifact_root: Hash64,
        current_daa: u64,
    ) -> Option<PalwHeldOpeningCtxV1> {
        let params = &self.consensus_config.params;
        if !params.palw_offence_attribution_active_at(current_daa) || !params.palw_held_context_active_at(current_daa) {
            return None;
        }
        if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode
            && bundle.state.held_class_is_unanswerable_v1(&class_id)
        {
            return None;
        }
        if !session.palw_v2_class_table().into_iter().any(|row| row.class_id == class_id && row.held) {
            return None;
        }
        Some(PalwHeldOpeningCtxV1 {
            artifact_root,
            opening_cap: self.class_step_ladder(class_id),
            ladder: self.held_accusation_ladder_v1(class_id, current_daa),
            bound: params.palw_audit_2026_09_23_active_at(current_daa),
        })
    }

    /// **P2-8e's loop half, once a tick** ([`palw_held_dissections_step_v1`] over the panel): the
    /// phase ends of the claims with an opening to date read off the chain first (this bond's seat
    /// rows, off the tick — only while such a case exists, which is rare).
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn held_dissections_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: &mut PalwReplayFilerV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        court_duties: &[PalwCourtDutyV2],
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<PalwCourtQueueKeyV1, u64>,
        court_moved: &HashMap<PalwCourtQueueKeyV1, u64>,
    ) {
        let wanted: HashSet<Hash64> = filer
            .cases
            .iter()
            .filter(|(_, case)| matches!(case.step, PalwReplayCaseStepV1::Dissect(_)))
            .map(|(claim, _)| *claim)
            .collect();
        let phase_ends: HashMap<Hash64, u64> = if wanted.is_empty() {
            HashMap::new()
        } else {
            session
                .clone()
                .spawn_blocking(move |c| {
                    c.palw_claim_rows_v1(bond_key, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Seat, false, 4_096)
                })
                .await
                .map(|read| read.rows)
                .unwrap_or_default()
                .into_iter()
                .filter(|row| wanted.contains(&row.claim_id))
                .map(|row| (row.claim_id, row.deadline_daa.unwrap_or(u64::MAX)))
                .collect()
        };
        let host = PalwPanelHeldFilerHostV1 { panel: self, session, network_domain, phase_ends };
        palw_held_dissections_step_v1(&host, filer, current_daa, bond_key, court_duties, court_pending, court_due, court_moved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }

    /// A court duty of `bond` as challenger of a held fused session at `leaf` on `claim`.
    pub(super) fn challenger_duty(claim: Hash64, leaf: u64, session: Hash64, me: PalwBondKeyV2, rung: u64) -> PalwCourtDutyV2 {
        PalwCourtDutyV2 {
            accepted_block: h(0xB0),
            session_id: session,
            claim_id: claim,
            class_id: h(0xC1A55),
            artifact_root: h(0xA7),
            executor_bond: bond(0xE0),
            challenger_bond: me,
            i_am_responder: false,
            round: 0,
            interval: (leaf, leaf + 1),
            midpoint: None,
            terminal_index: Some(leaf),
            last_disclosure: None,
            turn: PalwBisectTurnV1::AwaitDisclosure,
            rung_deadline_daa: rung,
            session_deadline_daa: rung + 3_000,
            trace_root: h(0x7A),
            execution_root: h(0xE7),
            free_prompt: false,
            fused_class: true,
            dissection: None,
            panel_seat_count: 5,
        }
    }

    /// **MED-4's due rule, one helper**: the claim's `Final` less the landing margin; unknown is the
    /// last dated place, never an overflow.
    #[test]
    fn p2_8e_an_opening_is_due_by_the_claims_final_less_the_landing_margin() {
        assert_eq!(palw_court_accusation_due_v1(10_000), 10_000 - PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1);
        assert_eq!(palw_court_accusation_due_v1(30), 0, "saturating");
        assert_eq!(palw_court_accusation_due_v1(u64::MAX), u64::MAX - PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1);
    }

    /// **The fold's reasons, sorted**: room, capacity, C3 and a duplicate wait; every other refusal and
    /// the acceptance layer's settle.
    #[test]
    fn p2_8e_the_folds_answer_to_an_opening_as_a_step() {
        use PalwObjectRehearsalV1 as R;
        let step = |r: R| palw_held_rehearsal_step_v1(&r);
        assert_eq!(step(R::Accepted), PalwSeatAccuseStepV1::File);
        let ceiling = PalwStateV2Error::AccusationExposureCeiling {
            bond: bond(1),
            edge: "held dissection",
            backed: 1,
            accusation: 2,
            ceiling: 3,
        };
        assert_eq!(step(R::Refused(ceiling)), PalwSeatAccuseStepV1::Retry, "C4's room frees as sessions close");
        assert_eq!(
            step(R::Refused(PalwStateV2Error::CourtSessionsAtCapacity { open: 9, max: 9, turn_deadline_daa: 42, per_block: 4 })),
            PalwSeatAccuseStepV1::Retry
        );
        assert_eq!(
            step(R::Refused(PalwStateV2Error::HeldDissectionFurtherSessionRefused { claim: h(1), why: "a non-seat waits" })),
            PalwSeatAccuseStepV1::Retry
        );
        assert_eq!(step(R::Refused(PalwStateV2Error::DuplicateSession(h(2)))), PalwSeatAccuseStepV1::Retry);
        assert_eq!(
            step(R::Refused(PalwStateV2Error::WrongPhase { claim: h(1), edge: "ShardCourtAccused" })),
            PalwSeatAccuseStepV1::Settle,
            "a claim gone terminal"
        );
        assert_eq!(
            step(R::Refused(PalwStateV2Error::ShardCourtHeldSiteUnanswerable { claim: h(1), leaf: 3, class: h(9) })),
            PalwSeatAccuseStepV1::Settle,
            "C5's unanswerable class"
        );
        assert_eq!(step(R::NotAccepted("the signature does not verify".into())), PalwSeatAccuseStepV1::Settle);
    }

    /// **The restart rebuild**: the book is a function of the chain's court duties — a fresh book (a
    /// restarted node) knows every held dissection this bond challenges on its first read, a closed one
    /// is `done` for a case's window, one seen again is open again, and a responder's or another
    /// challenger's session is not this bond's.
    #[test]
    fn p2_8e_the_held_dissections_are_rebuilt_from_the_chain() {
        let me = bond(7);
        let mine = challenger_duty(h(1), 42, h(0x51), me, 160);
        let mut theirs = challenger_duty(h(2), 43, h(0x52), bond(8), 170);
        theirs.challenger_bond = bond(8);
        let mut responder = challenger_duty(h(3), 44, h(0x53), me, 180);
        responder.i_am_responder = true;
        let mut restarted = PalwHeldDissectionsV1::default();
        let fresh = restarted.observe_v1(&[mine.clone(), theirs, responder], &me, 120);
        assert_eq!(fresh, vec![(h(1), 42, h(0x51), 160)], "the rung the chain stamped is read off the duty");
        assert!(restarted.claim_v1(&h(1)) && restarted.dissected_v1(&h(1), 42));
        assert!(!restarted.claim_v1(&h(2)) && !restarted.claim_v1(&h(3)));
        assert!(restarted.observe_v1(&[mine.clone()], &me, 121).is_empty(), "seen before: not fresh");
        // The session closes: done for a case's window, then forgotten.
        assert!(restarted.observe_v1(&[], &me, 200).is_empty());
        assert!(restarted.dissected_v1(&h(1), 42), "done");
        assert!(!restarted.dissected_v1(&h(1), 41), "another leaf is another dissection");
        let _ = restarted.observe_v1(&[], &me, 200 + super::super::PALW_REPLAY_FILER_CASE_DAA_V1);
        assert!(restarted.dissected_v1(&h(1), 42), "inside the window");
        let _ = restarted.observe_v1(&[], &me, 201 + super::super::PALW_REPLAY_FILER_CASE_DAA_V1);
        assert!(!restarted.claim_v1(&h(1)), "past it");
        // Open again after a transiently empty read.
        let _ = restarted.observe_v1(&[mine.clone()], &me, 300);
        let _ = restarted.observe_v1(&[], &me, 301);
        let _ = restarted.observe_v1(&[mine], &me, 302);
        assert!(restarted.dissected_v1(&h(1), 42));
        assert!(restarted.done.is_empty(), "seen again: open, not done");
    }
}
