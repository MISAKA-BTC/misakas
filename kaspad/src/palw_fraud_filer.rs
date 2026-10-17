//! **Lane LG14-A (RFC-0014 §6): the common fraud filer, the node's half** (node policy; a child module of `palw_panel`, whose loop
//! holds its one book and calls [`PalwPanelService::fraud_filer_tick_v1`] once a tick).
//!
//! **What it closes** (GAP-80, G14 on the legacy V2 Panel route). A claim every seat of its panel signed `Valid` reaches `Final` unless
//! someone OUTSIDE the panel can take it to an objective outcome. Past `palw_legacy_public_filer_v1` any bond can: this node replays
//! the claim's job on its own model (the attempt's job is derived from the claim's block, never served), and when its execution root
//! differs it runs the ONE engine every role runs (`kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_v1`, also
//! what the real-node suite `lg14a_legacy_filer_e2e` drives):
//!
//! 1. **reserve** — tag 154, asked of the fold first (`palw_legacy_dispute_reservation_check_v1`): the claim's ends are held until its
//!    own hard deadline, so no timeout outruns the pursuit;
//! 2. **read the binding** — a reserved `DefaultAccused` of event `(0, 0)`, whose answer carries the claim's step binding (the fold
//!    authenticates it against the claim's `execution_root`);
//! 3. **localize** — past LG14-B's fence a base0-codec replica compares authenticated tag-158 frontiers against its own tree,
//!    descending to the first divergent leaf. Every other case (below that fence, or any codec) runs lane G14-V's authenticated
//!    `StepRange` subtree descent (`kaspa_consensus_core::palw_auth_descent_v1`): only subtree roots the range answers carried, each
//!    authenticated to the claim's step root, are compared with the verifier's own tree — never a lone midpoint leaf — so a lie at
//!    any position is located in at most `h − 9` reads (`h` the tree height; 33 reserved sessions at 2^40 leaves);
//! 4. **terminal** — the LG14-B replica builds tag 159 from its own registered model and public leaf hashes, or uses a public CKW
//!    to open a fused held court. The subtree descent demands `StepLeaf`. Silence on a required unit defaults (DA-7).
//!    Public binding/job/shape/output faults go through the reporter door as kind 4 before descent. **Past an agreeing step tree**
//!    (G14-V) the logits trace is localized from the binding read's event (flat: no further session; tiled: an authenticated
//!    rows/tiles descent of `Event` reads) and `LogitsNotStepOutput` / `ForgedOutput` filed through the same door; step-tree
//!    agreement never clears a mismatched execution. A checkpoint-leg-only difference is reported, not guessed (open).
//!
//! **Shared progress** (§7.4): a unit the chain already answered — this node's before a restart, or anybody's — is read off the
//! accepted blocks, never demanded again; the unit must be marked answered for descent. A complete direct proof needs only
//! authenticated public bytes and the recorded target, even when the DA arm refused that disclosure.
//!
//! **Restart** (§6.3): the book is a cache, nothing is persisted. The candidates and every case's chain facts come from the tip, the
//! answers from the accepted blocks (walked back to the oldest pursued claim's acceptance after a start), and the honest run from a
//! fresh replay. A restart costs one replay a case and never a wrong filing.
//!
//! **Who** — identity decides, nothing opts in: a node that carries (`--palw-fee-outpoint`) past the fence. A claim on whose current
//! panel this bond sits is the seat's duties' (P2-6, P2-8b/8d), which this filer leaves alone so one bond never files a claim twice;
//! its own claims are never pursued.
//!
//! **Load.** One replay in flight, started only while the seat's own replay slots have room for a light replay; a mismatch's capture is
//! kept for its localization, at most [`PALW_FRAUD_FILER_HELD_RUNS_V1`] at once (the rest wait their turn); one item on the court queue a
//! case, each sent at most [`PALW_FRAUD_FILER_SENDS_V1`] times; the chain read once a DAA. Every item rides the priority lane dated
//! now: a reservation and a demand are what keep the claim's ends held and the producer's answer window running.
//!
//! **Consensus-inert, and dormant below the fence** — a clean no-op there: the book is emptied and nothing is queued, so a node below
//! `palw_legacy_public_filer_v1` (every shipped preset) behaves byte for byte as before.

use super::*;
use std::collections::BTreeSet;
#[path = "palw_staged_bundle_scheduler.rs"]
mod bundle_scheduler;
#[path = "palw_fraud_filer_constraint.rs"]
mod constraint;
#[path = "palw_constraint_scheduler.rs"]
mod constraint_scheduler;
#[path = "palw_fraud_filer_history.rs"]
pub(super) mod history;
#[path = "palw_fraud_filer_input.rs"]
pub(super) mod input;
#[path = "palw_fraud_filer_prefix.rs"]
mod prefix;
#[path = "palw_selected_event_scheduler.rs"]
mod selected_event_scheduler;
#[path = "palw_tir_acquisition_driver.rs"]
mod tir_acquisition_driver;
#[path = "palw_filer_tokenizer.rs"]
mod tokenizer;
#[path = "palw_tokenizer_scheduler.rs"]
mod tokenizer_scheduler;
#[path = "palw_tir_arithmetic_scheduler.rs"]
mod tir_arithmetic_scheduler;
/// G14 R4: the common public-conviction filing of an ORIGINAL TIR frame (build + sign; node policy, dormant below its fence).
#[path = "palw_tir_public_filer.rs"]
pub(super) mod tir_public_filer;
/// G14 R3: an ORIGINAL TIR frame's custodian preparation (the producer and a drawn Valid seat), the prepared capture and Ready
/// (node policy, dormant below its fence).
#[path = "palw_tir_public_custodian.rs"]
pub(super) mod tir_public_custodian;
/// G14 R4: when to file a public conviction against an ORIGINAL TIR frame, its queue lane and its restart journal (node policy,
/// dormant below its fence).
#[path = "palw_tir_public_filer_scheduler.rs"]
pub(super) mod tir_public_filer_scheduler;
/// G14 R4: the outside verifier's acquisition driver and localization — the findings feed of the public filer (node policy,
/// dormant below its fence).
#[path = "palw_tir_public_owner.rs"]
pub(super) mod tir_public_owner;
#[cfg(test)] // the in-process e2e tests read the fp input the filer builds
pub(super) use input::palw_fraud_filer_fp_input_v1;
pub(super) use input::{palw_fraud_filer_fp_metadata_v1, palw_fraud_filer_fp_payload_v1};

use super::reporter_filer::{
    PalwConvictionDoorV1, PalwConvictionFilingV1, PalwFileOutcomeV1, PalwFilingOriginV1, PalwReporterFilerV1,
};
use crate::palw_legacy_held_v2::PalwLegacyReplicaV2;
use kaspa_consensus_core::palw_auth_descent_v1::{
    PalwAuthDescentV1, PalwAuthTreeV1, PalwOwnLeafTreeV1, PalwTraceDescentV1, PalwTraceLocalizeV1, palw_auth_descent_next_v1,
};
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, palw_da_step_leaf_is_fused_v1};
use kaspa_consensus_core::palw_legacy_held_da_v2::{
    PalwCommittedKernelWitnessV2, PalwLegacyDescentStepV2, PalwLegacyFrontierV2, PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2,
    palw_legacy_held_check_answer_v2, palw_legacy_leaf_is_fused_v2,
};
use kaspa_consensus_core::palw_legacy_public_filer_v1::{
    PALW_DISPUTE_RESERVATION_VERSION_V1, PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1,
    PalwFilerActionV1, PalwFilerPhaseV1, PalwFilerRoleV1, PalwLegacyBisectV1, PalwLegacyProbeV1, palw_dispute_reserved_object_v1,
    palw_fraud_filer_demand_object_v1, palw_fraud_filer_facts_v1, palw_fraud_filer_learn_v1, palw_fraud_filer_reservation_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwFraudFilerCandidateV1, PalwLegacyDisputeViewV1};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_core::{debug, error};

/// The court queue's round of this filer's reservation of a claim — a round no other lane keys by (P2-6 `u32::MAX`, the reporter
/// filer `u32::MAX − 3 ..= u32::MAX − 1`, P2-8d `− 6`, lane B `− 7`, the audit duty `− 8`, the rider outbox `− 9`).
pub(super) const PALW_FRAUD_FILER_RESERVE_ROUND_V1: u32 = u32::MAX - 10;
/// The court queue's round of this filer's demand on a claim (one open at a time: DA-1's one session per accuser).
pub(super) const PALW_FRAUD_FILER_DEMAND_ROUND_V1: u32 = u32::MAX - 11;
pub(super) const PALW_FRAUD_FILER_TERMINAL_ROUND_V2: u32 = u32::MAX - 12;
/// An input bootstrap that finds an honest claim relinquishes its held reservation.
pub(super) const PALW_FRAUD_FILER_RELEASE_ROUND_V1: u32 = u32::MAX - 13;
/// One guarded typed staged move per claim; the accepted cursor, not transport, acknowledges it.
pub(super) const PALW_FRAUD_FILER_STAGED_ROUND_V1: u32 = u32::MAX - 14;

/// A TIR claim's natively derived original context (behind `palw_staged_tir_route_v1`): the
/// decode view and the admitted public header, both read from native accepted state.
pub(super) struct FilerTirContextV1 {
    pub view: kaspa_consensus_core::palw_decode_view_v1::PalwDecodeViewV1,
    pub header: Box<kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>,
}

fn palw_fraud_filer_paid_progress_v1(
    progress: &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
    claim: Hash64,
    owner: PalwBondKeyV2,
) -> bool {
    progress.claim == claim
        && progress.reserver == owner
        && progress.seed_id.is_some()
        && (progress.cursor.is_some() || progress.bundle.is_some())
        && progress.terms.accepted_daa <= progress.reserved_daa
        && progress.reserved_daa <= progress.now_daa
        && progress.now_daa
            < progress.bundle.as_ref().map_or(progress.terms.collect_until_daa, |bundle| bundle.plan.terms.collect_until_daa)
}

fn palw_fraud_filer_staged_due_v1(
    object: &PalwConsensusObjectV2,
    terms: &kaspa_consensus_core::palw_rendering_resources_v1::PalwRenderingClaimTermsV1,
) -> u64 {
    use kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1;
    let until = match object {
        PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { .. } | PalwConsensusObjectV2::PublicCourtStreamOpenedNativeV1 { .. } => {
            terms.open_until_daa
        }
        PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement }
            if matches!(movement.action, PalwConstraintStreamActionV1::BundleOpen { .. }) =>
        {
            terms.open_until_daa
        }
        PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement }
            if matches!(movement.action, PalwConstraintStreamActionV1::PrefixPublicCollect { .. }) =>
        {
            terms.collect_until_daa
        }
        _ => terms.stream_until_daa,
    };
    until.saturating_sub(1)
}
/// The host ledger's role for this filer's replays.
pub(super) const PALW_FRAUD_FILER_REPLAY_ROLE_V1: &str = "fraud-filer";
/// Mismatched runs held in memory at once (each keeps its capture for the localizer's own ranges).
pub(super) const PALW_FRAUD_FILER_HELD_RUNS_V1: usize = 2;
/// Replays one claim gets: the run, and one more after a failure that was this host's.
pub(super) const PALW_FRAUD_FILER_RUNS_PER_CLAIM_V1: u8 = 2;
/// Sends of one item (a reservation, one probe's demand): the first, and one more if the first was lost.
pub(super) const PALW_FRAUD_FILER_SENDS_V1: u8 = 2;
/// A queued item not on chain this long after it was sent is taken as lost.
pub(super) const PALW_FRAUD_FILER_RESEND_DAA_V1: u64 = 30;
/// Chain blocks one page reads at most. A partial page resumes from its cursor; DAA difference is not a bound on chain blocks.
pub(super) const PALW_FRAUD_FILER_WALK_BLOCKS_V1: usize = 40_000;
/// DAA a later walk re-reads below the last one's tip, for a reorg.
pub(super) const PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1: u64 = 64;
/// Cases the book keeps at most (settled ones are dropped first once their claim leaves the candidates).
pub(super) const PALW_FRAUD_FILER_MAX_CASES_V1: usize = 4_096;

pub(super) use input::PalwFraudFilerFpBootstrapV1;
#[cfg(test)]
pub(super) use input::PalwFraudFilerFpDescriptorV1;

/// Whether a court-queue entry is this filer's.
pub(super) fn palw_fraud_filer_queued_v1(round: u32, responder: bool) -> bool {
    !responder
        && (round == PALW_FRAUD_FILER_RESERVE_ROUND_V1
            || round == PALW_FRAUD_FILER_DEMAND_ROUND_V1
            || round == PALW_FRAUD_FILER_TERMINAL_ROUND_V2
            || round == PALW_FRAUD_FILER_RELEASE_ROUND_V1
            || round == PALW_FRAUD_FILER_STAGED_ROUND_V1)
}

/// **One honest run of a claim's job on this host** — the filer's own material, never the producer's: the roots its replay
/// reproduced, and its own leaf hashes of any range (opened from its capture by the backend that ran it, which the closure holds with
/// the capture and the prompt). Holds the ledger's reservation for as long as the capture lives.
pub(super) struct PalwFraudFilerRunV1 {
    pub(super) execution_root: Hash64,
    pub(super) trace_root: Hash64,
    pub(super) output_root: Hash64,
    /// This node's own leaf hashes of `[first, first + count)`.
    pub(super) own_range: Box<dyn Fn(u64, u32) -> Result<Vec<Hash64>, String> + Send + Sync>,
    pub(super) legacy: Option<Arc<PalwLegacyReplicaV2>>,
    /// Lane G14-V: the own run's material for the authenticated subtree descent and the localizations past the step tree.
    pub(super) own: Option<Arc<PalwFraudFilerOwnRunV1>>,
    /// Authenticated public FP descriptor retained with a mismatched replay until prosecution.
    pub(super) fp_job: Option<Box<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>>,
    pub(super) own_output_ids: Vec<u32>,
    pub(super) _job_reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
    pub(super) _reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}

/// Public descriptor retained while a local replica is absent. Its byte ticket follows the descriptor,
/// while dropping the run releases the much larger capture ticket and backend closure.
struct PalwFraudFilerDeferredPublicV1 {
    job: Box<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
    _reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
}
struct PalwTirSelectionIdsV1 {
    ids: Vec<u32>,
    _reservation: crate::palw_memory_ledger::PalwMemoryReservationV1,
}

fn palw_fraud_filer_tir_retention_v1(
    ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
    claim: Hash64,
    job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
    decode: u32,
) -> Result<(crate::palw_memory_ledger::PalwMemoryReservationV1, crate::palw_memory_ledger::PalwMemoryReservationV1), String> {
    let job_bytes = (borsh::object_length(job).map_err(|e| e.to_string())? as u64)
        .checked_mul(8)
        .and_then(|n| n.checked_add(16_384))
        .ok_or("TIR public descriptor retention overflow")?;
    let ids_bytes = u64::from(decode).checked_mul(32).and_then(|n| n.checked_add(16_384)).ok_or("TIR own-ID retention overflow")?;
    let reserve = |role, bytes| {
        ledger
            .reserve(crate::palw_memory_ledger::PalwMemoryReservationKeyV1 { role, class_id: job.class_id, job: claim }, bytes)
            .map_err(|e| e.to_string())
    };
    Ok((reserve("public-tir-descriptor", job_bytes)?, reserve("public-tir-own-ids", ids_bytes)?))
}

/// **Lane G14-V: the verifier's own run, as the localizers past the binding read it** — its backend, its capture and the job's
/// prompt (its own material, never the producer's). Its own step tree is built once per run, retained at
/// [`kaspa_consensus_core::palw_auth_descent_v1::PalwOwnLeafTreeV1::retain_level_v1`] and folded below it on demand.
pub(super) struct PalwFraudFilerOwnRunV1 {
    pub(super) backend: Arc<dyn PalwExecutionBackendV1>,
    pub(super) material: Arc<Vec<u8>>,
    pub(super) prompt: Arc<Vec<u32>>,
    pub(super) tree: std::sync::OnceLock<Result<Arc<PalwOwnLeafTreeV1>, String>>,
}

impl PalwFraudFilerOwnRunV1 {
    pub(super) fn new(backend: Arc<dyn PalwExecutionBackendV1>, material: Arc<Vec<u8>>, prompt: Arc<Vec<u32>>) -> Self {
        Self { backend, material, prompt, tree: std::sync::OnceLock::new() }
    }

    /// The own run's committed event `(row, tile)` (its DA responder's disclosure of its own capture).
    pub(super) fn event(
        &self,
        row: u32,
        tile: u8,
    ) -> Result<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1, String> {
        self.backend.disclose_trace_event(&self.material, row, tile)
    }

    /// The own run's binding (an out-of-range event discloses only the binding).
    pub(super) fn binding(&self) -> Result<PalwStepBindingV2, String> {
        self.event(u32::MAX, u8::MAX).map(|d| d.binding().clone())
    }

    /// The own run's leaf hashes of `[first, first + count)`.
    pub(super) fn range(&self, first: u64, count: u32) -> Result<Vec<Hash64>, String> {
        self.backend.held_step_range_answer_v1(&self.material, &self.prompt, first, count).map(|opening| opening.leaf_hashes)
    }

    /// The own run's opening of step leaf `leaf` (authenticates against the claim's step root only where the two trees agree).
    pub(super) fn opening(&self, leaf: u64) -> Result<kaspa_consensus_core::palw_step_leg::PalwStepOpeningV1, String> {
        let range = self.backend.held_step_range_answer_v1(&self.material, &self.prompt, leaf, 1)?;
        let leaf_hash = *range.leaf_hashes.first().ok_or("an empty own range")?;
        Ok(kaspa_consensus_core::palw_step_leg::PalwStepOpeningV1 { leaf_index: leaf, leaf_hash, siblings: range.siblings })
    }

    /// The own step tree over `leaves` leaves, built once.
    pub(super) fn tree(&self, leaves: u64) -> Result<Arc<PalwOwnLeafTreeV1>, String> {
        let tree = self
            .tree
            .get_or_init(|| {
                PalwOwnLeafTreeV1::build(leaves, PalwOwnLeafTreeV1::retain_level_v1(leaves), &|first, count| self.range(first, count))
                    .map(Arc::new)
            })
            .clone()?;
        if tree.leaf_count != leaves {
            return Err("the claim's step count is not this public job's: the job/count terminal is required".into());
        }
        Ok(tree)
    }
}

/// Where this node's judgement of one claim stands.
#[derive(Clone)]
#[allow(dead_code)] // dormant or test-only: kept, not yet wired to a caller in this build
pub(super) enum PalwFraudFilerVerdictV1 {
    /// Not yet replayed (or a replay that failed on this host, to be run once more).
    Pending,
    /// The replay reproduced the claim's execution root: nothing to file.
    Honest,
    /// Arithmetic matched; immutable registered semantics still require authenticated public court material.
    SemanticPending,
    /// Its replica was evicted from the bounded cache; the public pursuit remains live.
    DeferredMismatch,
    /// The replay did not: the engine pursues it.
    Mismatch(Arc<PalwFraudFilerRunV1>),
    /// This host cannot judge it (the class does not resolve, the block is not held, the replay failed twice, a fused leaf).
    Unjudged(String),
    /// The pursuit is over.
    Settled(PalwFilerPhaseV1),
}

/// What this node sent last on a case, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwFraudFilerSentV1 {
    Reserve,
    Demand(PalwLegacyProbeV1),
    Release,
    Reacquire { probe: PalwLegacyProbeV1, session_number: u8 },
    Terminal { leaf: u64 },
}

/// One history walk, anchored to the tip where it started. Its watermark advances only when `next` is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PalwFraudFilerWalkV1 {
    pub(super) floor: u64,
    pub(super) anchor: Hash64,
    pub(super) anchor_daa: u64,
    pub(super) next: Option<Hash64>,
    pub(super) incomplete: bool,
}

/// Only the binding read and the ONE unit a case's localizer currently reads enter the cache (lane G14-V: the subtree descent's
/// range or the trace descent's event — any unit the chain answered for anyone, never every range a stranger demanded). Each is
/// learned into the case's authenticated tree and dropped, so the cache holds one binding and at most one unit per pursued claim.
/// LG14-B separately holds the selected authenticated node/witness answer or one bounded G14-L
/// round; its selected/learned legacy units retain independent host-ledger tickets.
fn palw_fraud_filer_cache_unit_v1(unit: &PalwDaUnitV1, wanted: Option<&PalwDaUnitV1>) -> bool {
    matches!(*unit, PalwDaUnitV1::Event { row: 0, tile: 0 }) || wanted == Some(unit)
}

/// Read/resume one page. A reorg beyond the old anchor restarts at the current tip and the oldest pursued claim, including when
/// a recent incremental floor would otherwise hide the new branch's old answers. An unavailable old suffix retains authenticated
/// answers from the readable prefix, with an incomplete watermark and a restart at the next tip.
#[cfg(test)]
pub(super) fn palw_fraud_filer_read_page_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    oldest: u64,
    floor: u64,
    previous: Option<PalwFraudFilerWalkV1>,
    max_blocks: usize,
    wanted: &BTreeMap<Hash64, Hash64>,
    legacy_wanted: &BTreeMap<Hash64, PalwLegacyHeldUnitV2>,
) -> Result<(PalwFraudFilerWalkV1, bool, Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>), String> {
    palw_fraud_filer_read_page_with_form_v1(
        consensus,
        oldest,
        floor,
        previous,
        max_blocks,
        wanted,
        legacy_wanted,
        &BTreeMap::new(),
        &BTreeMap::new(),
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
    )
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // dormant or test-only: kept, not yet wired to a caller in this build
pub(super) fn palw_fraud_filer_read_page_with_form_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    oldest: u64,
    floor: u64,
    previous: Option<PalwFraudFilerWalkV1>,
    max_blocks: usize,
    wanted: &BTreeMap<Hash64, Hash64>,
    legacy_wanted: &BTreeMap<Hash64, PalwLegacyHeldUnitV2>,
    legacy_rounds: &BTreeMap<Hash64, BTreeSet<PalwLegacyHeldUnitV2>>,
    rcore_wanted: &BTreeMap<Hash64, PalwDaUnitV1>,
    network: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<(PalwFraudFilerWalkV1, bool, Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>), String> {
    palw_fraud_filer_read_page_with_budget_v2(
        consensus,
        oldest,
        floor,
        previous,
        max_blocks,
        wanted,
        legacy_wanted,
        legacy_rounds,
        rcore_wanted,
        &BTreeMap::new(),
        network,
        kaspa_consensus_core::palw_legacy_answer_chunks_v2::PALW_LEGACY_ANSWER_MAX_BYTES_V2,
    )
}

#[allow(clippy::too_many_arguments)]
fn palw_fraud_filer_read_page_with_budget_v2(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    oldest: u64,
    floor: u64,
    previous: Option<PalwFraudFilerWalkV1>,
    max_blocks: usize,
    wanted: &BTreeMap<Hash64, Hash64>,
    legacy_wanted: &BTreeMap<Hash64, PalwLegacyHeldUnitV2>,
    legacy_rounds: &BTreeMap<Hash64, BTreeSet<PalwLegacyHeldUnitV2>>,
    rcore_wanted: &BTreeMap<Hash64, PalwDaUnitV1>,
    event_wanted: &BTreeMap<Hash64, Vec<PalwDaUnitV1>>,
    network: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    maximum: u64,
) -> Result<(PalwFraudFilerWalkV1, bool, Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>), String> {
    let tip = consensus.get_sink();
    let same_branch = previous.is_none_or(|p| p.anchor == tip || consensus.is_chain_ancestor_of(p.anchor, tip).unwrap_or(false));
    let reset = !same_branch;
    let continuing = previous.filter(|p| same_branch && p.next.is_some() && p.next != Some(p.anchor));
    let floor = continuing.map_or(if reset { oldest } else { floor }, |p| p.floor);
    let anchor = continuing.map_or(tip, |p| p.anchor);
    let anchor_daa = continuing.map_or_else(
        || consensus.get_header(anchor).map(|h| h.daa_score).map_err(|e| format!("history anchor {anchor}: {e}")),
        |p| Ok(p.anchor_daa),
    )?;
    let start = continuing.and_then(|p| p.next).unwrap_or(tip);
    let mut answers = BTreeMap::new();
    let mut unavailable = continuing.is_some_and(|walk| walk.incomplete);
    let (next, page_incomplete) = history::walk_available_v1(consensus, start, floor, max_blocks, &mut |block, object| match object {
        PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, answer, .. }
            if wanted.get(&claim).is_some_and(|root| {
                kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(&unit, &answer, root)
            }) && (palw_fraud_filer_cache_unit_v1(&unit, rcore_wanted.get(&claim))
                || event_wanted.get(&claim).is_some_and(|units| units.contains(&unit))) =>
        {
            palw_fraud_filer_cache_answer_v1(&mut answers, (claim, unit), PalwDaBuiltAnswerV1::Rcore(answer));
        }
        PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
            if (legacy_wanted.get(&answer.claim) == Some(&answer.unit)
                || legacy_rounds.get(&answer.claim).is_some_and(|round| round.contains(&answer.unit)))
                && wanted.get(&answer.claim).is_some_and(|root| {
                    kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v3(
                        root,
                        &answer.unit,
                        &answer.binding,
                        &answer.answer,
                        answer.binding.step_leaf_count,
                        network,
                    )
                    .is_ok()
                }) =>
        {
            answers
                .entry((answer.claim, PalwDaUnitV1::LegacyHeldV2(answer.unit)))
                .or_insert(PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((answer.binding, answer.answer))));
        }
        PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk }
            if (legacy_wanted.get(&chunk.claim) == Some(&chunk.unit)
                || legacy_rounds.get(&chunk.claim).is_some_and(|round| round.contains(&chunk.unit)))
                && wanted.contains_key(&chunk.claim)
                && !answers.contains_key(&(chunk.claim, PalwDaUnitV1::LegacyHeldV2(chunk.unit))) =>
        {
            match consensus.palw_legacy_answer_history_v2(block, chunk, maximum) {
                Ok(Some(answer))
                    if wanted.get(&answer.claim).is_some_and(|root| {
                        kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v3(
                            root,
                            &answer.unit,
                            &answer.binding,
                            &answer.answer,
                            answer.binding.step_leaf_count,
                            network,
                        )
                        .is_ok()
                    }) =>
                {
                    answers
                        .entry((answer.claim, PalwDaUnitV1::LegacyHeldV2(answer.unit)))
                        .or_insert(PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((answer.binding, answer.answer))));
                }
                Err(why) => {
                    debug!("[{PALW_PANEL}] multipart public history at {block}: {why}; completion watermark unchanged");
                    unavailable = true;
                }
                _ => {}
            }
        }
        _ => {}
    })
    .unwrap_or_else(|why| {
        debug!("[{PALW_PANEL}] partial public history: {why}; completion watermark unchanged");
        (Some(anchor), true)
    });
    unavailable |= page_incomplete;
    let next = if unavailable && next.is_none() { Some(anchor) } else { next };
    let end = consensus.get_sink();
    if anchor != end && !consensus.is_chain_ancestor_of(anchor, end).unwrap_or(false) {
        // A branch switch during the page invalidates every collected answer and watermark.
        let end_daa = consensus.get_header(end).map_err(|e| format!("history end anchor {end}: {e}"))?.daa_score;
        return Ok((
            PalwFraudFilerWalkV1 { floor: oldest, anchor: end, anchor_daa: end_daa, next: Some(end), incomplete: false },
            true,
            vec![],
        ));
    }
    // `next == anchor` is the unavailable-history sentinel: retain independently authenticated answers from the readable
    // prefix, never mark the floor completed, and restart at the NEW tip on the next tick so a reacquired copy is reachable.
    Ok((PalwFraudFilerWalkV1 { floor, anchor, anchor_daa, next, incomplete: unavailable }, reset, answers.into_iter().collect()))
}

/// Preserve an authenticated binding even when its event bytes are malformed, since the binding can prove a job/shape fault.
/// A valid public event replaces a binding-only carrier, so junk pin bytes cannot hide an older output proof across pages.
pub(super) fn palw_fraud_filer_cache_answer_v1(
    answers: &mut BTreeMap<(Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1>,
    key: (Hash64, PalwDaUnitV1),
    answer: PalwDaBuiltAnswerV1,
) {
    let event_valid = |a: &PalwDaBuiltAnswerV1| match (key.1, a) {
        (PalwDaUnitV1::Event { row, tile }, PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(e))) => {
            let b = e.binding();
            kaspa_consensus_core::palw_step_refute::check_trace_event_disclosure_v1(
                b.full_logits_trace_root,
                b.committed_execution_root,
                row,
                tile,
                e,
                b.step_leaf_count,
            )
            .is_ok()
        }
        _ => false,
    };
    match answers.entry(key) {
        std::collections::btree_map::Entry::Vacant(slot) => {
            slot.insert(answer);
        }
        std::collections::btree_map::Entry::Occupied(mut slot) if event_valid(&answer) && !event_valid(slot.get()) => {
            slot.insert(answer);
        }
        _ => {}
    }
}

/// A typed retrieval bound: node answers cannot grow to the whole terminal ceiling merely
/// because another unit can. The frontier is at most 2^9 hashes and an u64 tree has at most
/// 64 outer siblings. Binding and framing are counted independently of those hashes.
fn palw_fraud_filer_legacy_material_bytes_v1(
    binding: &PalwStepBindingV2,
    unit: PalwLegacyHeldUnitV2,
    maximum: u64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<u64, String> {
    use kaspa_consensus_core::palw_legacy_held_da_v2::*;
    let maximum = maximum.min(kaspa_consensus_core::palw_legacy_answer_chunks_v2::PALW_LEGACY_ANSWER_MAX_BYTES_V2);
    if maximum == 0 {
        return Err("public legacy material has no owned answer envelope".into());
    }
    let binding_bytes = borsh::object_length(binding).map_err(|e| format!("legacy binding length: {e}"))? as u64;
    let body = match unit {
        PalwLegacyHeldUnitV2::StepNode { .. } | PalwLegacyHeldUnitV2::CheckpointNode { .. } => {
            64 * ((1u64 << PALW_LEGACY_HELD_NODE_DEPTH_V2) + 64) + 16
        }
        PalwLegacyHeldUnitV2::PromptIds { chunk } => {
            palw_legacy_prompt_answer_bound_v2(binding, chunk, form).map_err(|e| e.to_string())?
        }
        PalwLegacyHeldUnitV2::FpJob => PALW_LEGACY_FP_JOB_MAX_BYTES_V2 + 8,
        PalwLegacyHeldUnitV2::KernelWitness { .. } => maximum,
    };
    // Even a CKW cannot exceed the owned answer ceiling. The framing allowance also covers
    // vec/map metadata; eight simultaneous copies are conservative for the serial service.
    Ok(8 * binding_bytes.saturating_add(body).min(maximum).saturating_add(16_384))
}

/// **One case** — a candidate claim and this node's pursuit of it.
#[derive(Clone)]
pub(super) struct PalwFraudFilerCaseV1 {
    pub(super) candidate: PalwFraudFilerCandidateV1,
    pub(super) verdict: PalwFraudFilerVerdictV1,
    pub(super) binding: Option<PalwStepBindingV2>,
    _public_context_reservation: Option<Arc<crate::palw_memory_ledger::PalwMemoryReservationV1>>,
    pub(super) bisect: PalwLegacyBisectV1,
    /// Lane G14-V: the claim's step tree as far as authenticated range answers reached it (the subtree descent's state).
    pub(super) step_tree: Option<PalwAuthTreeV1>,
    /// Lane G14-V: the authenticated tiled events a trace localization read past an agreeing step tree.
    pub(super) trace: PalwTraceDescentV1,
    /// Lane G14-V: the one range or event unit this case's localizer reads now (the history reader caches only it).
    pub(super) rcore_wanted: Option<PalwDaUnitV1>,
    pub(super) frontiers: Vec<PalwLegacyFrontierV2>,
    pub(super) witness: Option<Box<PalwCommittedKernelWitnessV2>>,
    pub(super) legacy_wanted: Option<PalwLegacyHeldUnitV2>,
    constraint_plan: Option<constraint::TiledPlanV1>,
    constraint_done: bool,
    prefix_plan: Option<prefix::PrefixPlanV1>,
    prefix_complete: bool,
    prefix_stage_pending: bool,
    rendering_done: bool,
    staged_tokenizer: Option<Arc<std::sync::Mutex<Option<tokenizer_scheduler::TokenizerSchedulerV1>>>>,
    staged_constraint: Option<Arc<std::sync::Mutex<Option<constraint_scheduler::ConstraintSchedulerV1>>>>,
    constraint_stage_pending: bool,
    staged_bundle_pending: bool,
    staged_tokenizer_sent: Option<(Hash64, u64, u8)>,
    prefix_budget: u64,
    _prefix_reservation: Option<Arc<crate::palw_memory_ledger::PalwMemoryReservationV1>>,
    /// Lane G14-L: the current k-ary round's units (the lanes of one reserved session) — the history reader caches their answers.
    pub(super) legacy_round: Vec<PalwLegacyHeldUnitV2>,
    /// Lane G14-L: when each lane of the current round was last queued, and how many times (a lost lane is resent after
    /// [`PALW_FRAUD_FILER_RESEND_DAA_V1`], at most [`PALW_FRAUD_FILER_SENDS_V1`] times).
    pub(super) lanes_sent: BTreeMap<PalwLegacyHeldUnitV2, (u64, u8)>,
    // Tickets survive the history worker and every case clone while authenticated answers or
    // their learned frontiers remain live. No new lane is submitted without its retention budget.
    legacy_material_tickets: BTreeMap<PalwLegacyHeldUnitV2, Arc<crate::palw_memory_ledger::PalwMemoryReservationV1>>,
    pub(super) runs: u8,
    registered_preflight: bool,
    arithmetic_matched: bool,
    tir_selection_ids: Option<Arc<PalwTirSelectionIdsV1>>,
    replay_failures: u8,
    replay_turn: u64,
    deferred_public: Option<Arc<PalwFraudFilerDeferredPublicV1>>,
    input_retry_at: u64,
    pub(super) fp_bootstrap: Option<PalwFraudFilerFpBootstrapV1>,
    /// Authenticated user ids survive replica rotation under their original small ticket.
    fp_replay_input: Option<PalwFraudFilerFpBootstrapV1>,
    /// Binding-only pursuit when the original FP job is unavailable; held through a later replay.
    fp_job_pending: bool,
    fp_expected_pin: Option<Hash64>,
    fp_job_disclosure_enabled: bool,
    fp_descriptor: Option<Arc<input::PalwFraudFilerFpDescriptorV1>>,
    _fp_job_reservation: Option<Arc<crate::palw_memory_ledger::PalwMemoryReservationV1>>,
    /// Keep a completed bootstrap in the book until its on-chain reservation is relinquished.
    release_pending: bool,
    awaiting_public: Option<(PalwLegacyProbeV1, u64)>,
    /// The last item sent, when, and how many times.
    pub(super) sent: Option<(PalwFraudFilerSentV1, u64, u8)>,
    /// **G14 (G-D02b): the court round of this case's last item the priority lane reported carried, and when.** The engine reads the
    /// chain, so if it asks for the very item again after it was carried, the carrying block left the selected chain (a reorg — e.g.
    /// the nodes mining apart across a fork-id switch) and the item is resent at once, without spending its retry budget.
    pub(super) carried: Option<(u32, u64)>,
    /// Reporter-door handoffs of the claim's direct proof, bounded like the replay lane's.
    direct_handed: u8,
    direct_asked_at: Option<u64>,
    /// A TIR required-family claim's native context (behind `palw_staged_tir_route_v1`): set by the
    /// native TIR replay. Such a case never enters the legacy binding/probe pipeline.
    pub(super) tir: Option<Arc<FilerTirContextV1>>,
}

impl PalwFraudFilerCaseV1 {
    pub(super) fn new(candidate: PalwFraudFilerCandidateV1) -> Self {
        Self {
            candidate,
            verdict: PalwFraudFilerVerdictV1::Pending,
            binding: None,
            _public_context_reservation: None,
            bisect: PalwLegacyBisectV1::new(0),
            step_tree: None,
            trace: PalwTraceDescentV1::default(),
            rcore_wanted: None,
            frontiers: Vec::new(),
            witness: None,
            legacy_wanted: None,
            constraint_plan: None,
            constraint_done: false,
            prefix_plan: None,
            prefix_complete: false,
            prefix_stage_pending: false,
            rendering_done: false,
            staged_tokenizer: None,
            staged_constraint: None,
            constraint_stage_pending: false,
            staged_bundle_pending: false,
            staged_tokenizer_sent: None,
            prefix_budget: 0,
            _prefix_reservation: None,
            legacy_round: Vec::new(),
            lanes_sent: BTreeMap::new(),
            legacy_material_tickets: BTreeMap::new(),
            runs: 0,
            registered_preflight: false,
            arithmetic_matched: false,
            tir_selection_ids: None,
            replay_failures: 0,
            replay_turn: 0,
            deferred_public: None,
            input_retry_at: 0,
            fp_bootstrap: None,
            fp_replay_input: None,
            fp_job_pending: false,
            fp_expected_pin: None,
            fp_job_disclosure_enabled: false,
            fp_descriptor: None,
            _fp_job_reservation: None,
            release_pending: false,
            awaiting_public: None,
            sent: None,
            carried: None,
            direct_handed: 0,
            direct_asked_at: None,
            tir: None,
        }
    }

    fn preserve_semantic_refusal(&mut self) {
        if let Some(input) = self.fp_bootstrap.take() {
            self.fp_descriptor = Some(input.payload);
            self._fp_job_reservation = input._reservation;
        }
        self.verdict = PalwFraudFilerVerdictV1::SemanticPending;
        self.arithmetic_matched = false;
        self.fp_job_pending = true;
        self.release_pending = false;
    }

    pub(super) fn pursued(&self) -> bool {
        self.release_pending
            || matches!(
                self.verdict,
                PalwFraudFilerVerdictV1::Mismatch(_)
                    | PalwFraudFilerVerdictV1::DeferredMismatch
                    | PalwFraudFilerVerdictV1::SemanticPending
            )
            || (matches!(self.verdict, PalwFraudFilerVerdictV1::Pending) && (self.fp_bootstrap.is_some() || self.fp_job_pending))
    }

    /// Public metadata survives successful bootstrap cleanup through the held replay.
    fn public_fp_job_v1(&self) -> Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3> {
        self.fp_bootstrap
            .as_ref()
            .map(|input| input.payload.job())
            .or_else(|| self.fp_descriptor.as_ref().map(|input| input.job()))
            .or_else(|| self.deferred_public.as_ref().map(|public| public.job.as_ref()))
            .or_else(|| match &self.verdict {
                PalwFraudFilerVerdictV1::Mismatch(run) => run.fp_job.as_deref(),
                _ => None,
            })
    }

    fn constraint_preflight_pending_v1(&self) -> bool {
        self.constraint_stage_pending
            || (self.binding.is_some() && !self.constraint_done && self.public_fp_job_v1().is_some_and(|job| job.is_constraint()))
    }

    pub(super) fn learn(&mut self, probe: PalwLegacyProbeV1, answer: &PalwDaBuiltAnswerV1) -> Result<(), String> {
        if let (Some(plan), Some(binding), PalwLegacyProbeV1::HeldNode { unit }, PalwDaBuiltAnswerV1::LegacyHeldV2(answer)) =
            (self.prefix_plan.as_mut(), self.binding.as_ref(), probe, answer)
            && plan.next_unit(binding)? == Some(unit)
        {
            if answer.0 != *binding {
                return Err("the prefix witness carries another binding".into());
            }
            let PalwLegacyHeldAnswerV2::KernelWitness(witness) = &answer.1 else {
                return Err("the prefix unit is not a committed kernel witness".into());
            };
            plan.learn(binding, witness)?;
            self.legacy_wanted = None;
            return Ok(());
        }
        if self.fp_bootstrap.is_some() || self.fp_job_pending {
            match (probe, answer) {
                (PalwLegacyProbeV1::Binding { row: 0, tile: 0 }, PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event))) => {
                    if !kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                        &probe.unit(),
                        &PalwDaAnswerV1::Event(event.clone()),
                        &self.candidate.job.execution_root,
                    ) {
                        return Err("bootstrap binding does not authenticate".into());
                    }
                    let binding = event.binding();
                    if binding.full_logits_trace_root != self.candidate.job.trace_root {
                        return Err("bootstrap binding has another trace root".into());
                    }
                    self.bisect = PalwLegacyBisectV1::new(binding.step_leaf_count);
                    self.step_tree = PalwAuthTreeV1::new(binding.step_leaf_count, binding.step_merkle_root);
                    self.binding = Some(binding.clone());
                    return Ok(());
                }
                (PalwLegacyProbeV1::HeldNode { unit }, PalwDaBuiltAnswerV1::LegacyHeldV2(answer)) => {
                    if self.binding.as_ref() != Some(&answer.0) {
                        return Err("bootstrap input carries another binding".into());
                    }
                    if unit == PalwLegacyHeldUnitV2::FpJob {
                        let PalwLegacyHeldAnswerV2::FpJob(job) = &answer.1 else {
                            return Err("job preimage has another answer kind".into());
                        };
                        self.fp_descriptor = Some(Arc::new(input::PalwFraudFilerFpDescriptorV1::from_disclosure(
                            &self.candidate,
                            self.fp_expected_pin.ok_or("no recorded FP job pin")?,
                            &answer.0,
                            job,
                        )?));
                    } else {
                        self.fp_bootstrap
                            .as_mut()
                            .ok_or("bootstrap still has no authenticated FP job/input")?
                            .learn(&answer.0, unit, &answer.1)?;
                    }
                    self.legacy_wanted = None;
                    self.input_retry_at = 0;
                    return Ok(());
                }
                _ => return Err("bootstrap waits for a binding or input unit".into()),
            }
        }
        if self.constraint_plan.is_some_and(|plan| plan.units().contains(&probe.unit()))
            && let PalwDaBuiltAnswerV1::Rcore(answer) = answer
        {
            if !kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                &probe.unit(),
                answer,
                &self.candidate.job.execution_root,
            ) {
                return Err("the selected public tile does not authenticate".into());
            }
            let PalwDaAnswerV1::Event(event) = answer else { return Err("the selected answer is not an event".into()) };
            if self.binding.as_ref() != Some(event.binding()) {
                return Err("the selected tile has another binding".into());
            }
            return Ok(());
        }
        match (probe, answer) {
            (PalwLegacyProbeV1::HeldNode { unit }, PalwDaBuiltAnswerV1::LegacyHeldV2(answer)) => {
                let binding = self.binding.as_ref().ok_or("a legacy node is learned after the binding")?;
                if answer.0 != *binding {
                    return Err("the public legacy answer carries another binding".into());
                }
                palw_legacy_held_check_answer_v2(
                    &self.candidate.job.execution_root,
                    &unit,
                    binding,
                    &answer.1,
                    binding.step_leaf_count,
                )
                .map_err(|e| format!("the public legacy answer does not authenticate: {e}"))?;
                match (unit, &answer.1) {
                    (PalwLegacyHeldUnitV2::StepNode { level, index }, PalwLegacyHeldAnswerV2::Node { frontier, .. }) => {
                        let frontier = PalwLegacyFrontierV2::of_answer(binding.step_leaf_count, level, index, frontier)
                            .ok_or("the public frontier is not the requested node")?;
                        if let Some(prior) = self.frontiers.iter().find(|f| f.level == level && f.index == index) {
                            if *prior != frontier {
                                return Err("conflicting frontiers of one authenticated node".into());
                            }
                        } else {
                            // Lane G14-L: a k-ary round holds up to 64 node lanes; a descent never runs more rounds than the
                            // one-node descent would (each round covers at least the nine levels one node does).
                            let cap =
                                kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_descent_rounds_v2(binding.step_leaf_count)
                                    as usize
                                    * kaspa_consensus_core::palw_legacy_parallel_v1::PALW_LEGACY_LANES_PER_SESSION_V1 as usize;
                            if self.frontiers.len() >= cap {
                                return Err("more frontiers than this descent can consume".into());
                            }
                            self.frontiers.push(frontier);
                        }
                    }
                    (PalwLegacyHeldUnitV2::KernelWitness { .. }, PalwLegacyHeldAnswerV2::KernelWitness(witness)) => {
                        self.witness = Some(witness.clone());
                    }
                    _ => return Err("this filer's step descent does not consume that legacy unit".into()),
                }
                self.legacy_wanted = None;
                Ok(())
            }
            (_, PalwDaBuiltAnswerV1::Rcore(answer)) => {
                let PalwFraudFilerVerdictV1::Mismatch(run) = &self.verdict else { return Err("no mismatched replay is held".into()) };
                match probe {
                    // G14-V: a range is learned into the authenticated subtree descent, never into a linear scan.
                    PalwLegacyProbeV1::Range { .. } => {
                        let binding = self.binding.as_ref().ok_or("a range is learned after the binding")?;
                        let tree = self.step_tree.as_mut().ok_or("no step tree is held for this binding")?;
                        kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_learn_subtree_v1(
                            probe,
                            answer,
                            &self.candidate.job.execution_root,
                            binding,
                            tree,
                        )?;
                        self.rcore_wanted = None;
                        Ok(())
                    }
                    // G14-V: a tiled logits event past the binding, authenticated against the claim's own roots before it is kept.
                    PalwLegacyProbeV1::Binding { row, tile } if (row, tile) != (0, 0) => {
                        let binding = self.binding.as_ref().ok_or("an event is learned after the binding")?;
                        let PalwDaAnswerV1::Event(event) = answer else { return Err("an event is answered by an event".into()) };
                        if event.binding() != binding {
                            return Err("the event carries another binding".into());
                        }
                        self.trace.learn(
                            self.candidate.job.trace_root,
                            self.candidate.job.execution_root,
                            row,
                            tile,
                            event,
                            binding.step_leaf_count,
                        )?;
                        self.rcore_wanted = None;
                        Ok(())
                    }
                    _ => {
                        palw_fraud_filer_learn_v1(
                            probe,
                            answer,
                            &self.candidate.job.execution_root,
                            &mut self.binding,
                            &mut self.bisect,
                            |first, count| (run.own_range)(first, count),
                        )?;
                        if let (PalwLegacyProbeV1::Binding { .. }, Some(binding)) = (probe, self.binding.as_ref()) {
                            self.step_tree = PalwAuthTreeV1::new(binding.step_leaf_count, binding.step_merkle_root);
                        }
                        Ok(())
                    }
                }
            }
            _ => Err("the answer has another carriage than the probe".into()),
        }
    }

    /// Public semantics cannot certify an arithmetic run which the host never completed.
    fn may_finish_semantics_v1(&self, semantic_ready: bool) -> bool {
        semantic_ready
            && self.arithmetic_matched
            && !self.constraint_stage_pending
            && !self.staged_bundle_pending
            && matches!(self.verdict, PalwFraudFilerVerdictV1::SemanticPending)
    }

    /// A neutral stage still requires a completed own selection replay. Missing material
    /// or a refused replay must not freeze this owner's only Constraint stage as honest.
    fn neutral_constraint_plan_v1(&self, authenticated_ids: &[u32]) -> Option<constraint::TiledPlanV1> {
        let selection_replayed =
            self.arithmetic_matched || matches!(&self.verdict,PalwFraudFilerVerdictV1::Mismatch(run) if run.fp_job.is_some());
        selection_replayed.then(|| authenticated_ids.first().copied()).flatten().map(|committed| constraint::TiledPlanV1 {
            position: 0,
            committed,
            witness: None,
            beat_lane: None,
        })
    }

    /// Only all three independently replayed roots establish arithmetic agreement.
    /// Required public semantics still run, with the original own IDs available for nomination.
    fn retain_tir_selection_replay_v1(
        &mut self,
        job: kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        replay: kaspa_consensus_core::palw_backend::PalwTirReplayV1,
        job_ticket: crate::palw_memory_ledger::PalwMemoryReservationV1,
        ids_ticket: crate::palw_memory_ledger::PalwMemoryReservationV1,
    ) {
        self.registered_preflight = true;
        self.release_pending = false;
        self.staged_bundle_pending = true;
        self.arithmetic_matched = (replay.execution_root, replay.full_logits_trace_root, replay.output_root)
            == (self.candidate.job.execution_root, self.candidate.job.trace_root, self.candidate.job.output_root);
        if self.arithmetic_matched {
            self.deferred_public =
                Some(Arc::new(PalwFraudFilerDeferredPublicV1 { job: Box::new(job), _reservation: Some(job_ticket) }));
            self.tir_selection_ids = Some(Arc::new(PalwTirSelectionIdsV1 { ids: replay.generated_ids, _reservation: ids_ticket }));
            self.verdict = PalwFraudFilerVerdictV1::SemanticPending;
            return;
        }
        self.tir_selection_ids = None;
        self.verdict = PalwFraudFilerVerdictV1::Mismatch(Arc::new(PalwFraudFilerRunV1 {
            execution_root: replay.execution_root,
            trace_root: replay.full_logits_trace_root,
            output_root: replay.output_root,
            own_range: Box::new(|_, _| Err("a TIR selection replay does not authenticate a legacy step range".into())),
            legacy: None,
            own: None,
            fp_job: Some(Box::new(job)),
            own_output_ids: replay.generated_ids,
            _job_reservation: Some(job_ticket),
            _reservation: Some(ids_ticket),
        }));
    }

    fn finish_tir_semantics_v1(&mut self) -> bool {
        if self.tir.is_none() || !self.may_finish_semantics_v1(true) {
            return false;
        }
        self.verdict = PalwFraudFilerVerdictV1::Honest;
        self.release_pending = true;
        true
    }

    /// Apply the authenticated worker result, including the input accumulator and its shared memory ticket.
    pub(super) fn accept_learned_v1(&mut self, learned: Self) {
        self.awaiting_public = None;
        self.binding = learned.binding;
        self.bisect = learned.bisect;
        self.step_tree = learned.step_tree;
        self.trace = learned.trace;
        self.rcore_wanted = learned.rcore_wanted;
        self.frontiers = learned.frontiers;
        self.witness = learned.witness;
        self.legacy_wanted = learned.legacy_wanted;
        self.prefix_plan = learned.prefix_plan;
        self.prefix_complete = learned.prefix_complete;
        self.legacy_round = learned.legacy_round;
        self.legacy_material_tickets = learned.legacy_material_tickets;
        self.fp_bootstrap = learned.fp_bootstrap;
        self.fp_descriptor = learned.fp_descriptor;
        self.input_retry_at = learned.input_retry_at;
    }

    /// Reserve the selected answer envelopes before reading or demanding them. Learned node
    /// frontiers keep their old unit tickets; input/witness/job tickets retire when no cache or
    /// selected worker can use them. The per-unit multiplier covers decode, cache, learning and
    /// the bounded service worker copies, including leaf-to-node frontier expansion.
    fn reserve_legacy_material_v1(
        &mut self,
        ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
        maximum: u64,
        form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> Result<(), String> {
        let selected: BTreeSet<_> = self.legacy_round.iter().copied().chain(self.legacy_wanted).collect();
        self.legacy_material_tickets.retain(|unit, _| {
            selected.contains(unit)
                || matches!(unit, PalwLegacyHeldUnitV2::KernelWitness { leaf }
                    if self.witness.as_ref().is_some_and(|w| w.refutation.output_opening.leaf_index == *leaf))
                || unit.node().is_some_and(|(_, level, index)| self.frontiers.iter().any(|f| f.level == level && f.index == index))
        });
        if selected.is_empty() {
            return Ok(());
        }
        let binding = self.binding.as_ref().ok_or("public legacy material waits for its authenticated binding")?;
        let required: Vec<_> = selected
            .iter()
            .map(|unit| palw_fraud_filer_legacy_material_bytes_v1(binding, *unit, maximum, form).map(|bytes| (*unit, bytes)))
            .collect::<Result<_, _>>()?;
        let missing: Vec<_> = required
            .into_iter()
            .filter(|(u, bytes)| self.legacy_material_tickets.get(u).is_none_or(|ticket| ticket.bytes() < *bytes))
            .collect();
        let cap = usize::from(kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1)
            + usize::from(kaspa_consensus_core::palw_legacy_parallel_v1::PALW_LEGACY_EXTRA_LANES_PER_RESERVATION_V1);
        if self.legacy_material_tickets.keys().copied().chain(selected.iter().copied()).collect::<BTreeSet<_>>().len() > cap {
            return Err("public legacy material exceeds this reservation's session/lane envelope".into());
        }
        // Atomic round admission: failed reservations drop every newly acquired guard and leave
        // the already admitted frontier guards intact. Other duties use this same host ledger.
        let mut admitted = Vec::with_capacity(missing.len());
        for (unit, bytes) in missing {
            let ticket = ledger
                .reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "public-legacy-material",
                        class_id: self.candidate.job.class_id,
                        job: self.candidate.claim_id,
                    },
                    bytes,
                )
                .map_err(|why| format!("public legacy material waits for memory: {why}"))?;
            admitted.push((unit, Arc::new(ticket)));
        }
        self.legacy_material_tickets.extend(admitted);
        Ok(())
    }

    /// A pruned round still needs the ordinary priced reacquisition path. Select only bytes
    /// this phase can consume, so an already accumulated input or learned node is never
    /// reacquired repeatedly. This is one unit/session; it does not imply batch feasibility.
    fn missing_legacy_round_v1(
        &self,
        view: &PalwLegacyDisputeViewV1,
        answer_read: impl Fn(&PalwDaUnitV1) -> bool,
    ) -> Option<PalwLegacyProbeV1> {
        self.legacy_round.iter().find_map(|unit| {
            let needed = match unit {
                PalwLegacyHeldUnitV2::PromptIds { chunk } => {
                    self.fp_bootstrap.as_ref().is_some_and(|input| !input.ready() && input.chunk == *chunk)
                }
                PalwLegacyHeldUnitV2::StepNode { level, index } | PalwLegacyHeldUnitV2::CheckpointNode { level, index } => {
                    !self.frontiers.iter().any(|f| f.level == *level && f.index == *index)
                }
                PalwLegacyHeldUnitV2::KernelWitness { leaf } => {
                    self.witness.as_ref().is_none_or(|w| w.refutation.output_opening.leaf_index != *leaf)
                }
                PalwLegacyHeldUnitV2::FpJob => self.public_fp_job_v1().is_none(),
            };
            let key = PalwDaUnitV1::LegacyHeldV2(*unit);
            (needed && view.unit_was_answered_v1(&key) && !answer_read(&key)).then_some(PalwLegacyProbeV1::HeldNode { unit: *unit })
        })
    }

    /// Give the bounded history walk a retry interval, then explicitly reacquire a missing answered unit. A new request
    /// consumes this reservation's next session counter; retries of the same signed request never consume another counter.
    pub(super) fn reacquire_after_v1(
        &mut self,
        view: &PalwLegacyDisputeViewV1,
        me: PalwBondKeyV2,
        probe: PalwLegacyProbeV1,
        now_daa: u64,
    ) -> Option<kaspa_consensus_core::palw_legacy_public_filer_v1::PalwDisputeReacquireV1> {
        if now_daa > view.hard_deadline_daa || view.sessions.iter().any(|(bond, _)| *bond == me) {
            return None;
        }
        let row = view.record.as_ref()?.live.get(&me)?;
        if !view.unit_was_answered_v1(&probe.unit()) {
            return None;
        }
        let since = match self.awaiting_public {
            Some((held, at)) if held == probe => at,
            _ => {
                self.awaiting_public = Some((probe, now_daa));
                return None;
            }
        };
        if now_daa.saturating_sub(since) < PALW_FRAUD_FILER_RESEND_DAA_V1 {
            return None;
        }
        let session_number = row.sessions_opened.checked_add(1)?;
        if session_number > PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1 {
            return None;
        }
        let binding = match probe.unit() {
            PalwDaUnitV1::Event { .. } => None,
            PalwDaUnitV1::Held(_) | PalwDaUnitV1::LegacyHeldV2(_) => Some(self.binding.clone()?),
            _ => return None,
        };
        Some(kaspa_consensus_core::palw_legacy_public_filer_v1::PalwDisputeReacquireV1 {
            version: PALW_DISPUTE_RESERVATION_VERSION_V1,
            claim: self.candidate.claim_id,
            reserver: me,
            reserved_daa: row.reserved_daa,
            session_number,
            valid_until_daa: now_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1).min(view.hard_deadline_daa),
            unit: probe.unit(),
            binding,
        })
    }

    /// May `item` be sent now? A different item always; the same item has a bounded retry interval and count.
    fn may_send(&self, item: PalwFraudFilerSentV1, now_daa: u64) -> Result<bool, ()> {
        if self.reorged_out_v1(item) {
            return Ok(true);
        }
        match self.sent {
            Some((last, at, sends)) if last == item => {
                if now_daa.saturating_sub(at) < PALW_FRAUD_FILER_RESEND_DAA_V1 {
                    Ok(false)
                } else if sends >= PALW_FRAUD_FILER_SENDS_V1 {
                    Err(())
                } else {
                    Ok(true)
                }
            }
            _ => Ok(true),
        }
    }

    /// The item was carried after it was last sent and the engine asks for it again: the carrying block was reorged out.
    fn reorged_out_v1(&self, item: PalwFraudFilerSentV1) -> bool {
        matches!((self.sent, self.carried), (Some((last, sent_at, _)), Some((round, carried_at)))
            if last == item && round == palw_fraud_filer_item_round_v1(item) && carried_at >= sent_at)
    }

    fn note_sent(&mut self, item: PalwFraudFilerSentV1, now_daa: u64) {
        let reorged = self.reorged_out_v1(item);
        let sends = match self.sent {
            _ if reorged => 1,
            Some((last, _, sends)) if last == item => sends.saturating_add(1),
            _ => 1,
        };
        if reorged {
            self.carried = None;
        }
        self.sent = Some((item, now_daa, sends));
    }
}

/// A blocking replay is tied to the exact candidate snapshot that started it. Keep the handle until it terminates,
/// even after invalidation: aborting a started blocking worker cannot stop it or release its memory safely.
struct PalwFraudFilerReplayV1 {
    candidate: PalwFraudFilerCandidateV1,
    handle: tokio::task::JoinHandle<Result<PalwFraudFilerRunV1, String>>,
}

/// **The filer's book** (the module's header): the cases by claim, the replay in flight, and the answers read off the accepted blocks.
#[derive(Default)]
pub(super) struct PalwFraudFilerBookV1 {
    pub(super) cases: BTreeMap<Hash64, PalwFraudFilerCaseV1>,
    replay_turn: u64,
    target_claims: BTreeSet<Hash64>,
    discovery_after: Option<(u64, Hash64)>,
    deferred_candidates: usize,
    read_at: Option<u64>,
    running: Option<PalwFraudFilerReplayV1>,
    /// `(claim, unit) → authenticated answer`, only the binding and consecutive ranges this localizer reads.
    pub(super) answers: BTreeMap<(Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1>,
    /// The DAA the walks have read down to (`None`: never walked since the start).
    walked_from: Option<u64>,
    /// The original tip's DAA at the last COMPLETED walk.
    walked_to: Option<u64>,
    /// The current page's cursor and branch anchor, or the last completed walk's anchor.
    walk: Option<PalwFraudFilerWalkV1>,
    /// This bond's role (an operator's genesis bond, or any other) — read once per refresh.
    role: Option<PalwFilerRoleV1>,
    network_form: Option<kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1>,
    /// **G14 (G-D02a): the step refutation each public legacy terminal carried**, kept until the pursuit ends. When the chain
    /// convicts the producer on it, the same refutation proves every liable `Valid` signer false, and is handed to the false-Valid
    /// filer (`palw_filer_false_valid.rs`) so the kind-3 filings follow without anyone filing by hand.
    pub(super) terminal_proofs: BTreeMap<Hash64, PalwFraudFilerTerminalProofV1>,
    /// **G14 (G-D02c): the one claim per producer this filer drives to conviction first.** One outside bond carries one object a
    /// block, and a producer lying on every draw makes a claim a block, so reserving every claim as it arrives starves the pursuit
    /// that would end them all (ADR-0160: one conviction forfeits every open claim of the bond and takes its collateral). While a
    /// producer's focus is pursued, its other claims are reserved only once their Final deadline comes within
    /// [`PALW_FRAUD_FILER_FOCUS_MARGIN_DAA_V1`] — so none reaches Final unheld, and the carrier goes to the focus's descent first.
    pub(super) focus: BTreeMap<PalwBondKeyV2, Hash64>,
}

/// G-D02c: a non-focus claim of a producer under pursuit is reserved once its Final deadline is this near (DAA). Generous against the
/// pursuit's length so a deferred claim is never left unheld at its floor; a claim with no Final deadline yet (unbound, unlicensed)
/// waits.
pub(super) const PALW_FRAUD_FILER_FOCUS_MARGIN_DAA_V1: u64 = 60;

/// G-D02c: should the filer defer reserving `claim` of `producer` now? Only while another claim of the same producer is the focus and
/// `claim`'s Final deadline (if any) is farther than the margin.
pub(super) fn palw_fraud_filer_defer_reservation_v1(
    focus: &BTreeMap<PalwBondKeyV2, Hash64>,
    producer: &PalwBondKeyV2,
    claim: &Hash64,
    final_deadline_daa: Option<u64>,
    current_daa: u64,
) -> bool {
    focus.get(producer).is_some_and(|f| f != claim)
        && final_deadline_daa.is_none_or(|d| d > current_daa.saturating_add(PALW_FRAUD_FILER_FOCUS_MARGIN_DAA_V1))
}

/// One public terminal's refutation (the producer's committed half, the verifier's own model rows, the prompt carriage).
#[derive(Clone, Debug)]
pub(super) struct PalwFraudFilerTerminalProofV1 {
    pub(super) leaf: u64,
    pub(super) refutation: kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1,
    pub(super) operand_openings: Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>,
    pub(super) prompt_ids_opening: Option<kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsOpeningV1>,
}

impl PalwFraudFilerTerminalProofV1 {
    /// The refutation a public terminal object carries (`None` for any other object).
    pub(super) fn of_terminal_v1(object: &PalwConsensusObjectV2) -> Option<Self> {
        match object {
            PalwConsensusObjectV2::LegacyLeafRecomputedV2 { accusation } => Some(Self {
                leaf: accusation.leaf_index,
                refutation: accusation.refutation.clone(),
                operand_openings: accusation.artifact_openings.clone(),
                prompt_ids_opening: accusation.prompt_ids_opening.clone(),
            }),
            PalwConsensusObjectV2::ShardCourtAccused { accusation } => Some(Self {
                leaf: accusation.leaf_index,
                refutation: accusation.refutation.clone(),
                operand_openings: accusation.artifact_openings.clone(),
                prompt_ids_opening: accusation.prompt_ids_opening.clone(),
            }),
            _ => None,
        }
    }
}

impl PalwFraudFilerBookV1 {
    fn observe_paid_authority_v1(
        &mut self,
        claim: Hash64,
        authority: &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedPaidAuthorityV1,
        now_daa: u64,
    ) {
        if authority.terms.accepted_daa > authority.reserved_daa
            || authority.reserved_daa > now_daa
            || now_daa >= authority.terms.collect_until_daa
        {
            return;
        }
        if let Some(case) = self.cases.get_mut(&claim)
            && !case.prefix_complete
            && !case.rendering_done
            && matches!(
                case.verdict,
                PalwFraudFilerVerdictV1::Pending
                    | PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::DaDefault | PalwFilerPhaseV1::Dismissed)
            )
        {
            case.verdict = PalwFraudFilerVerdictV1::SemanticPending;
            case.release_pending = false;
        }
    }

    fn hold_fp_input_v1(
        &mut self,
        claim: Hash64,
        payload: input::PalwFraudFilerFpDescriptorV1,
        form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        ledger: &Arc<crate::palw_memory_ledger::PalwMemoryLedgerV1>,
        maximum: u64,
    ) -> bool {
        let Some(case) = self.cases.get_mut(&claim) else { return false };
        if case.fp_bootstrap.is_some() {
            return true;
        }
        if case.fp_replay_input.as_ref().is_some_and(|input| input.form == form && input.payload.job() == payload.job()) {
            case.fp_bootstrap = case.fp_replay_input.take();
            return true;
        }
        let was_pursued = case.pursued();
        // Canonical ids are derived under a replay/direct-proof ticket, not this bootstrap.
        let n = if payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL {
            0
        } else {
            u64::from(payload.job().prompt_tokens)
        };
        let bytes = n.saturating_mul(16).saturating_add(n.div_ceil(32).saturating_mul(256)).saturating_add(2 * maximum);
        let Ok(ticket) = ledger.reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                role: "public-fp-input",
                class_id: payload.job().class_id,
                job: claim,
            },
            bytes,
        ) else {
            return false;
        };
        case.fp_bootstrap = Some(input::PalwFraudFilerFpBootstrapV1 {
            payload: Arc::new(payload),
            form,
            ids: Vec::new(),
            chunk: 0,
            _reservation: Some(Arc::new(ticket)),
        });
        if !was_pursued {
            self.walked_from = None;
            self.walked_to = None;
            self.walk = None;
            self.read_at = None;
        }
        true
    }

    /// Forget cases below the fence, but drain any already-started blocking replay before another can start.
    pub(super) fn clear(&mut self) {
        let running = self.running.take();
        self.answers.clear(); // Cached bytes retire before their case-owned guards.
        *self = Self { running, ..Self::default() };
    }

    /// **Take a fresh candidate read**: a new claim opens a case; a case whose claim left the candidates is dropped unless it is
    /// pursued (its outcome is read off the tip). A changed acceptance/job/role restarts that case and invalidates its queued items.
    /// Unrelated settled-case cleanup does not restart a pursued claim's partial backfill. Bounded by [`PALW_FRAUD_FILER_MAX_CASES_V1`].
    pub(super) fn refresh(&mut self, candidates: Vec<PalwFraudFilerCandidateV1>, now_daa: u64) -> BTreeSet<Hash64> {
        let fresh: BTreeSet<Hash64> = candidates.iter().map(|c| c.claim_id).collect();
        let mut invalidated = BTreeSet::new();
        let mut changed = false;
        let retired: BTreeSet<_> =
            self.cases.iter().filter(|(claim, case)| !fresh.contains(claim) && !case.pursued()).map(|(claim, _)| *claim).collect();
        self.answers.retain(|(claim, _), _| !retired.contains(claim));
        self.cases.retain(|claim, case| {
            let keep = fresh.contains(claim) || case.pursued();
            if !keep {
                invalidated.insert(*claim);
            }
            keep
        });
        let mut incoming = Vec::new();
        for candidate in candidates {
            match self.cases.get_mut(&candidate.claim_id) {
                Some(case) if case.candidate != candidate => {
                    changed = true;
                    invalidated.insert(candidate.claim_id);
                    self.answers.retain(|(claim, _), _| *claim != candidate.claim_id);
                    *case = PalwFraudFilerCaseV1::new(candidate);
                }
                Some(_) => {}
                None => incoming.push(candidate),
            }
        }
        incoming.sort_by_key(|candidate| (candidate.accepted_daa, candidate.claim_id));
        let after = self.discovery_after;
        let turn = PALW_FRAUD_FILER_MAX_CASES_V1.saturating_sub(self.cases.len()).max(16);
        let mut admitted = 0;
        let total = incoming.len();
        let later = incoming.iter().filter(|candidate| after.is_none_or(|after| (candidate.accepted_daa, candidate.claim_id) > after));
        let earlier =
            incoming.iter().filter(|candidate| after.is_some_and(|after| (candidate.accepted_daa, candidate.claim_id) <= after));
        for candidate in later.chain(earlier).take(turn) {
            if self.cases.len() >= PALW_FRAUD_FILER_MAX_CASES_V1 {
                let victim = self
                    .cases
                    .values()
                    .filter(|case| {
                        !case.pursued()
                            && !self.target_claims.contains(&case.candidate.claim_id)
                            && self.running.as_ref().is_none_or(|running| running.candidate != case.candidate)
                    })
                    .min_by_key(|case| (case.replay_turn, case.candidate.accepted_daa, case.candidate.claim_id))
                    .map(|case| case.candidate.claim_id);
                let Some(victim) = victim else { break };
                self.answers.retain(|(claim, _), _| *claim != victim);
                self.cases.remove(&victim);
                invalidated.insert(victim);
            }
            self.cases.insert(candidate.claim_id, PalwFraudFilerCaseV1::new(candidate.clone()));
            self.discovery_after = Some((candidate.accepted_daa, candidate.claim_id));
            admitted += 1;
        }
        self.deferred_candidates = total.saturating_sub(admitted);
        if !invalidated.is_empty() {
            self.answers.retain(|(claim, _), _| !invalidated.contains(claim));
        }
        if changed {
            self.walked_from = None;
            self.walked_to = None;
            self.walk = None;
        }
        self.read_at = Some(now_daa);
        invalidated
    }

    /// Whether the chain should be read again at `now_daa`.
    pub(super) fn stale(&self, now_daa: u64) -> bool {
        self.read_at != Some(now_daa)
    }

    /// Mismatched runs held now.
    pub(super) fn held_runs(&self) -> usize {
        self.cases.values().filter(|case| matches!(case.verdict, PalwFraudFilerVerdictV1::Mismatch(_))).count()
    }

    /// The next replay or FP metadata probe. A busy replay lane may probe a pending FP body,
    /// so its binding/direct proof is not held behind captures; it never starts another execution.
    #[cfg(test)]
    pub(super) fn next_replay(&self) -> Option<Hash64> {
        self.next_replay_at(0)
    }

    #[cfg(test)]
    fn next_replay_at(&self, current_daa: u64) -> Option<Hash64> {
        self.next_probe_at(current_daa, true)
    }

    fn next_probe_at(&self, current_daa: u64, replay_room: bool) -> Option<Hash64> {
        let busy = !replay_room || self.running.is_some() || self.held_runs() >= PALW_FRAUD_FILER_HELD_RUNS_V1;
        self.cases
            .values()
            .filter(|case| {
                matches!(case.verdict, PalwFraudFilerVerdictV1::Pending | PalwFraudFilerVerdictV1::DeferredMismatch)
                    && case.fp_bootstrap.as_ref().is_none_or(|input| {
                        input.ready()
                            || input.payload.job().prompt_mode
                                == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL
                    })
                    && self.running.as_ref().is_none_or(|running| running.candidate != case.candidate)
                    && (!busy || (case.candidate.job.free_prompt && case.fp_bootstrap.is_none()))
                    && !case.candidate.seat
                    && current_daa >= case.input_retry_at
            })
            .min_by_key(|case| {
                (
                    !self.target_claims.contains(&case.candidate.claim_id),
                    case.replay_turn,
                    case.candidate.accepted_daa,
                    case.candidate.claim_id,
                )
            })
            .map(|case| case.candidate.claim_id)
    }

    /// Rotate only when another eligible claim needs a replay. Never discard court state or
    /// release an on-chain reservation: a missing local replica is a scheduling state, not a verdict.
    fn make_replay_room(&mut self, current_daa: u64, replay_room: bool) {
        if !replay_room || self.running.is_some() || self.held_runs() < PALW_FRAUD_FILER_HELD_RUNS_V1 {
            return;
        }
        let waiting = self.cases.values().any(|case| {
            matches!(case.verdict, PalwFraudFilerVerdictV1::Pending | PalwFraudFilerVerdictV1::DeferredMismatch)
                && !case.candidate.seat
                && current_daa >= case.input_retry_at
                // Reprobing an unavailable job uses no replica. Its public DA pursuit
                // must not repeatedly evict a different claim's active localization.
                && (!case.fp_job_pending || case.public_fp_job_v1().is_some())
                && case.fp_bootstrap.as_ref().is_none_or(|input| {
                    input.ready()
                        || input.payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL
                })
        });
        if !waiting {
            return;
        }
        self.evict_oldest_capture();
    }

    /// Resource pressure can require the remaining replica too, even below the count cap.
    fn evict_oldest_capture(&mut self) -> bool {
        let victim = self
            .cases
            .values()
            .filter(|case| matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if Arc::strong_count(run) == 1))
            .min_by_key(|case| (case.replay_turn, case.candidate.accepted_daa, case.candidate.claim_id))
            .map(|case| case.candidate.claim_id);
        let Some(victim) = victim else { return false };
        let case = self.cases.get_mut(&victim).expect("selected case");
        let PalwFraudFilerVerdictV1::Mismatch(run) = std::mem::replace(&mut case.verdict, PalwFraudFilerVerdictV1::DeferredMismatch)
        else {
            unreachable!()
        };
        match Arc::try_unwrap(run) {
            Ok(mut run) => {
                case.deferred_public = run
                    .fp_job
                    .take()
                    .map(|job| Arc::new(PalwFraudFilerDeferredPublicV1 { job, _reservation: run._job_reservation.take() }));
                // `run` drops here: only authenticated public metadata survives its replica.
                true
            }
            Err(run) => {
                case.verdict = PalwFraudFilerVerdictV1::Mismatch(run);
                false
            }
        }
    }

    fn started_replay(&mut self, claim: Hash64) {
        if self.replay_turn == u64::MAX {
            // Preserve fair ordering even at counter exhaustion, using the bounded case book.
            let mut order: Vec<_> = self.cases.iter().map(|(claim, case)| (case.replay_turn, *claim)).collect();
            order.sort();
            for (index, (_, claim)) in order.into_iter().enumerate() {
                self.cases.get_mut(&claim).unwrap().replay_turn = index as u64;
            }
            self.replay_turn = self.cases.len() as u64;
        }
        self.replay_turn += 1;
        if let Some(case) = self.cases.get_mut(&claim) {
            case.replay_turn = self.replay_turn;
        }
    }

    /// The claims the engine steps this tick, oldest acceptance first.
    pub(super) fn pursued(&self) -> Vec<Hash64> {
        let mut out: Vec<(u64, Hash64)> = self
            .cases
            .values()
            .filter(|case| case.pursued())
            .map(|case| (case.candidate.accepted_daa, case.candidate.claim_id))
            .collect();
        out.sort();
        out.into_iter().map(|(_, claim)| claim).collect()
    }

    /// A result of an older acceptance/job/role snapshot must neither clear nor settle the current case.
    fn judge_replay(
        &mut self,
        replayed: &PalwFraudFilerCandidateV1,
        result: Result<PalwFraudFilerRunV1, String>,
    ) -> Option<&PalwFraudFilerVerdictV1> {
        if self.cases.get(&replayed.claim_id)?.candidate != *replayed {
            return None;
        }
        self.judge(replayed.claim_id, result)
    }

    /// Judge all three committed roots. An output-only or trace-only mismatch still needs a public proof or DA default.
    pub(super) fn judge(&mut self, claim: Hash64, result: Result<PalwFraudFilerRunV1, String>) -> Option<&PalwFraudFilerVerdictV1> {
        let case = self.cases.get_mut(&claim)?;
        let was_pursued = case.pursued();
        case.runs = case.runs.saturating_add(1);
        if result.is_err() {
            case.replay_failures = case.replay_failures.saturating_add(1);
        }
        let bootstrapped = case.fp_bootstrap.is_some() || case.fp_job_pending;
        if result.is_ok() {
            case.deferred_public = None;
            case.fp_replay_input = case.fp_bootstrap.take().filter(|input| {
                input.ready()
                    && input.payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER
                    && result.as_ref().is_ok_and(|run| {
                        run.execution_root != case.candidate.job.execution_root
                            || run.trace_root != case.candidate.job.trace_root
                            || run.output_root != case.candidate.job.output_root
                    })
            });
            case.fp_descriptor = None;
            case.fp_job_pending = false;
            case._fp_job_reservation = None;
        }
        case.verdict = match result {
            Ok(run)
                if run.execution_root != case.candidate.job.execution_root
                    || run.trace_root != case.candidate.job.trace_root
                    || run.output_root != case.candidate.job.output_root =>
            {
                PalwFraudFilerVerdictV1::Mismatch(Arc::new(run))
            }
            Ok(mut run) if case.registered_preflight => {
                case.deferred_public = run
                    .fp_job
                    .take()
                    .map(|job| Arc::new(PalwFraudFilerDeferredPublicV1 { job, _reservation: run._job_reservation.take() }));
                // Keep the row-zero obligation live without retaining an unnecessary matching replica.
                case.arithmetic_matched = true;
                case.fp_job_pending = true;
                PalwFraudFilerVerdictV1::SemanticPending
            }
            Ok(_) => PalwFraudFilerVerdictV1::Honest,
            Err(_) if case.registered_preflight => {
                // A false named prefix can make the honest backend refuse execution. Its
                // authenticated job/binding court must still convict without a producer cache.
                case.preserve_semantic_refusal();
                PalwFraudFilerVerdictV1::SemanticPending
            }
            Err(_) if case.replay_failures < PALW_FRAUD_FILER_RUNS_PER_CLAIM_V1 => PalwFraudFilerVerdictV1::Pending,
            Err(why) => PalwFraudFilerVerdictV1::Unjudged(format!("the replay failed twice: {why}")),
        };
        case.release_pending =
            bootstrapped && matches!(case.verdict, PalwFraudFilerVerdictV1::Honest | PalwFraudFilerVerdictV1::Unjudged(_));
        if case.release_pending {
            case.fp_bootstrap = None;
            case.fp_descriptor = None;
            case.fp_job_pending = false;
            case._fp_job_reservation = None;
        }
        if case.pursued() && !was_pursued {
            // Earlier walks did not collect answers for this case while it was pending, even if its acceptance is newer than
            // the old floor. Backfill every newly pursued case; do not skip it using another case's completed watermark.
            self.walked_from = None;
            self.walked_to = None;
            self.walk = None;
            self.read_at = None;
        }
        Some(&case.verdict)
    }

    /// The active backfill's floor, else the oldest pursued claim's acceptance before the first completed walk, else the last
    /// completed walk's original tip less a reorg margin. Every new pursuit invalidates the shared completion watermark.
    pub(super) fn walk_floor(&self) -> Option<u64> {
        let oldest = self.cases.values().filter(|case| case.pursued()).map(|case| case.candidate.accepted_daa).min()?;
        if let Some(walk) = self.walk.filter(|w| w.next.is_some()) {
            return Some(walk.floor);
        }
        Some(match (self.walked_from, self.walked_to) {
            (Some(from), Some(to)) if from <= oldest => to.saturating_sub(PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1),
            _ => oldest,
        })
    }

    /// Record only the range actually read. Partial pages retain their cursor and original tip; a branch restart discards the
    /// old branch's watermark/cache. Authenticated answers from a successful page can be used before the backfill completes.
    pub(super) fn walked(
        &mut self,
        walk: PalwFraudFilerWalkV1,
        reset: bool,
        answers: Vec<((Hash64, PalwDaUnitV1), PalwDaBuiltAnswerV1)>,
    ) {
        if reset {
            self.walked_from = None;
            self.walked_to = None;
            self.answers.clear();
            for case in self.cases.values_mut() {
                case.frontiers.clear();
                case.witness = None;
                case.binding = None;
                case.bisect = PalwLegacyBisectV1::new(0);
                case.step_tree = None;
                case.trace = PalwTraceDescentV1::default();
                case.rcore_wanted = None;
                case.legacy_wanted = None;
                case.constraint_plan = None;
                case.constraint_done = false;
                // A rooted semantic completion belongs to the accepted branch. Revalidate
                // both its typed court and any released matching replay after rollback.
                if case.registered_preflight
                    && (case.arithmetic_matched || case.release_pending)
                    && matches!(case.verdict, PalwFraudFilerVerdictV1::SemanticPending | PalwFraudFilerVerdictV1::Honest)
                {
                    case.verdict = PalwFraudFilerVerdictV1::Pending;
                    case.fp_job_pending = case.candidate.job.free_prompt;
                    case.release_pending = false;
                }
                case.arithmetic_matched = false;
                case.tir_selection_ids = None;
                case.prefix_plan = None;
                case.prefix_complete = false;
                case.prefix_stage_pending = false;
                case.rendering_done = false;
                case._prefix_reservation = None;
                case.prefix_budget = 0;
                case.legacy_round.clear();
                case.lanes_sent.clear();
                case.legacy_material_tickets.clear();
                case.staged_tokenizer = None;
                case.staged_constraint = None;
                case.constraint_stage_pending = false;
                case.staged_bundle_pending = false;
                case.staged_tokenizer_sent = None;
                case.sent = None;
                case.awaiting_public = None;
                if let Some(input) = case.fp_bootstrap.as_mut() {
                    input.ids.clear();
                    input.chunk = 0;
                }
                case.fp_replay_input = None;
            }
        }
        if walk.next.is_none() && !walk.incomplete {
            self.walked_from = Some(self.walked_from.map_or(walk.floor, |from| from.min(walk.floor)));
            self.walked_to = Some(walk.anchor_daa);
        }
        self.walk = Some(walk);
        for (key, answer) in answers {
            if self.cases.get(&key.0).is_some_and(|case| {
                case.pursued()
                    && match &answer {
                        PalwDaBuiltAnswerV1::Rcore(answer) => {
                            if matches!(key.1, PalwDaUnitV1::Event { .. })
                                && key.1 != (PalwDaUnitV1::Event { row: 0, tile: 0 })
                                && case.rcore_wanted != Some(key.1)
                                && !case.constraint_plan.is_some_and(|plan| plan.units().contains(&key.1))
                            {
                                return false;
                            }
                            kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
                                &key.1,
                                answer,
                                &case.candidate.job.execution_root,
                            )
                        }
                        PalwDaBuiltAnswerV1::LegacyHeldV2(answer) => {
                            let PalwDaUnitV1::LegacyHeldV2(unit) = key.1 else { return false };
                            (case.legacy_wanted == Some(unit) || case.legacy_round.contains(&unit))
                                && kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v3(
                                    &case.candidate.job.execution_root,
                                    &unit,
                                    &answer.0,
                                    &answer.1,
                                    answer.0.step_leaf_count,
                                    case.fp_bootstrap.as_ref().map_or(
                                        self.network_form
                                            .unwrap_or(kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
                                        |input| input.form,
                                    ),
                                )
                                .is_ok()
                        }
                    }
            }) {
                palw_fraud_filer_cache_answer_v1(&mut self.answers, key, answer);
            }
        }
        // An answer of a claim no longer pursued leaves with its case.
        let pursued: BTreeSet<Hash64> = self.pursued().into_iter().collect();
        self.answers.retain(|(claim, _), _| pursued.contains(claim));
    }

    /// Settle a case: its run (and the capture's reservation) and cached answers are let go.
    pub(super) fn settle(&mut self, claim: &Hash64, verdict: PalwFraudFilerVerdictV1) {
        self.answers.retain(|(held, _), _| held != claim);
        self.focus.retain(|_, focused| focused != claim);
        if let Some(case) = self.cases.get_mut(claim) {
            case.verdict = verdict;
            case.deferred_public = None;
            case.prefix_plan = None;
            case.prefix_complete = false;
            case.prefix_stage_pending = false;
            case._prefix_reservation = None;
            case.release_pending = false;
            case.fp_bootstrap = None;
            case.fp_replay_input = None;
            case.tir_selection_ids = None;
            case.fp_descriptor = None;
            case.fp_job_pending = false;
            case._fp_job_reservation = None;
            case.binding = None;
            case._public_context_reservation = None;
        }
        self.answers.retain(|(held, _), _| held != claim);
        if let Some(case) = self.cases.get_mut(claim) {
            case.frontiers.clear();
            case.witness = None;
            case.legacy_wanted = None;
            case.constraint_plan = None;
            case.legacy_round.clear();
            case.lanes_sent.clear();
            case.legacy_material_tickets.clear();
            case.staged_tokenizer = None;
            case.staged_constraint = None;
            case.constraint_stage_pending = false;
            case.staged_bundle_pending = false;
            case.staged_tokenizer_sent = None;
            case.rcore_wanted = None;
            case.trace = PalwTraceDescentV1::default();
        }
    }

    /// A newer binding-only carrier can be read before an older valid token pin. Do not discard the pursuit on descent's
    /// inability to explain matching step trees until that public backfill completes.
    fn descent_failed(&mut self, claim: &Hash64, why: String) -> bool {
        if self.walk.is_none_or(|walk| walk.next.is_some() || walk.incomplete) {
            return false;
        }
        self.settle(claim, PalwFraudFilerVerdictV1::Unjudged(why));
        true
    }

    /// G14-V: the subtree/trace descent's next unit, read from history like a selected legacy unit (an answer that predates the
    /// selection is backfilled; only this unit and the binding are cached for the claim).
    pub(super) fn want_rcore(&mut self, claim: Hash64, unit: PalwDaUnitV1) {
        let Some(case) = self.cases.get_mut(&claim) else { return };
        if case.rcore_wanted == Some(unit) {
            return;
        }
        case.rcore_wanted = Some(unit);
        self.answers.retain(|(held, held_unit), _| {
            *held != claim || matches!(held_unit, PalwDaUnitV1::Event { row: 0, tile: 0 } | PalwDaUnitV1::LegacyHeldV2(_))
        });
        self.walked_from = None;
        self.walked_to = None;
        self.walk = None;
        self.read_at = None;
    }

    /// Reading a previously answered legacy unit needs its historical bytes, even if a prior walk predated this selection.
    pub(super) fn want_legacy(&mut self, claim: Hash64, unit: PalwLegacyHeldUnitV2) {
        let Some(case) = self.cases.get_mut(&claim) else { return };
        if case.legacy_wanted == Some(unit) {
            return;
        }
        case.legacy_wanted = Some(unit);
        let round = case.legacy_round.clone();
        self.answers.retain(|(held, held_unit), _| {
            *held != claim || !matches!(held_unit, PalwDaUnitV1::LegacyHeldV2(u) if !round.contains(u))
        });
        self.walked_from = None;
        self.walked_to = None;
        self.walk = None;
        self.read_at = None;
    }

    /// **Lane G14-L: the current k-ary round's units** — their historical answers are read (backfilled like a selected legacy unit);
    /// answers of the previous round's other units leave the cache (each learned one lives on in the case's frontiers).
    pub(super) fn want_legacy_round(&mut self, claim: Hash64, units: Vec<PalwLegacyHeldUnitV2>) {
        let Some(case) = self.cases.get_mut(&claim) else { return };
        if case.legacy_round == units {
            return;
        }
        case.lanes_sent.retain(|unit, _| units.contains(unit));
        case.legacy_round = units;
        let (round, single) = (case.legacy_round.clone(), case.legacy_wanted);
        self.answers.retain(|(held, held_unit), _| {
            *held != claim || !matches!(held_unit, PalwDaUnitV1::LegacyHeldV2(u) if !round.contains(u) && single != Some(*u))
        });
        self.walked_from = None;
        self.walked_to = None;
        self.walk = None;
        self.read_at = None;
    }
}

/// Matching arithmetic cannot erase the claim-stamped model/tokenizer/table statement. A
/// refused or missing public binding/job remains pending; a prefix checkpoint needs its KV court.
fn palw_fraud_filer_registered_preflight_v1(
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    rules: kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1,
    answer: &PalwDaBuiltAnswerV1,
    fp_job: Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
    prefix_verified: bool,
    rendering_verified: bool,
) -> Result<bool, String> {
    if target.registered_semantics.is_none() {
        return Ok(true);
    }
    let PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event)) = answer else { return Ok(false) };
    if !kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
        &PalwDaUnitV1::Event { row: 0, tile: 0 },
        &PalwDaAnswerV1::Event(event.clone()),
        &target.execution_root,
    ) {
        return Err("the semantic binding does not authenticate to this claim".into());
    }
    let binding = event.binding();
    kaspa_consensus_core::palw_step_leg::verify_binding_v1(binding).map_err(|e| e.to_string())?;
    if kaspa_consensus_core::palw_offence_attribution_v1::palw_binding_identity_fault_v1(target, binding, rules, true)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Ok(false);
    }
    if target.lane == Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1::FreePrompt) {
        let Some(job) = fp_job else { return Ok(false) };
        if kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context) != target.job_identity {
            return Err("the semantic job pin differs from the recorded claim".into());
        }
        kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_fp_job_check_v2(binding, job).map_err(|e| e.to_string())?;
        if job.is_constraint()
            && kaspa_consensus_core::palw_offence_attribution_v1::palw_registered_table_fault_v1(target, binding, job)
                .map_err(|e| e.to_string())?
        {
            return Ok(false);
        }
        let qualified_rendering = target.registered_semantics.as_ref().is_some_and(|row| {
            row.rendering_rule.is_some()
                && row.token_table_root.is_some()
                && row.raw_tokenizer_bytes.is_some()
                && row.tokenizer_key_events.is_some()
        });
        if (job.is_constraint() || qualified_rendering) && !rendering_verified {
            // Root equality is only the immutable selection check. The whole authenticated
            // tokenizer/table correspondence must also pass before matching arithmetic is Honest.
            return Ok(false);
        }
        if job.is_prefix_state() && !prefix_verified {
            return Ok(false);
        }
    }
    Ok(true)
}

/// **The engine's step for one case**, decided off what the chain says and what this node already read (pure, so the tests drive it):
/// the engine's own action, except that a demand the chain already answered is READ (shared progress, §7.4) — `Learn` — and one it
/// has not answered while this node holds no answer read for it yet is demanded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PalwFraudFilerStepV1 {
    Engine(PalwFilerActionV1),
    Learn(PalwLegacyProbeV1),
    /// The chain marks the unit answered but the walk has not read the answer yet.
    AwaitAnswer(PalwLegacyProbeV1),
    LegacyTerminal {
        leaf: u64,
    },
    /// G14-V: the claim's step tree is the verifier's own and its execution is not — localize past the step tree (the logits
    /// trace, the committed tokens) instead of settling a mismatched execution as honest.
    PastStepTree,
    /// Lane G14-L: demand these lanes of one k-ary round (each joins this bond's reserved session; the first to land opens it);
    /// `round` is the whole round, whose answers the history reader caches.
    DemandLanes {
        round: Vec<PalwLegacyHeldUnitV2>,
        demand: Vec<PalwLegacyProbeV1>,
    },
    /// Lane G14-L: the chain answered lanes of this round that the walk has not read yet.
    AwaitRound(Vec<PalwLegacyHeldUnitV2>),
}

/// **Lane G14-L: the parallel-lane rule's inputs to a step** — `None` below either legacy fence (the one-unit descent, unchanged).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PalwFraudFilerLanesV1 {
    pub(super) now_daa: u64,
    pub(super) form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
}

/// **Lane G14-L: one round's next move**, from the chain and this node's reads alone (a restart re-derives it): a lane the chain
/// answered, this node read and may consume now is learned (one a step); the unanswered ones are demanded as lanes of this bond's
/// reserved session — opened by the first to land, joined by the others while the session's join window is open and it holds fewer
/// than 64 — never a lane already in the session; answered lanes the walk has not read yet are awaited; else wait.
pub(super) fn palw_fraud_filer_round_step_v1(
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    round: Vec<PalwLegacyHeldUnitV2>,
    learnable: &dyn Fn(&PalwLegacyHeldUnitV2) -> bool,
    answer_read: &dyn Fn(&PalwDaUnitV1) -> bool,
    now_daa: u64,
) -> PalwFraudFilerStepV1 {
    use kaspa_consensus_core::palw_legacy_parallel_v1::{PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1, PALW_LEGACY_LANES_PER_SESSION_V1};
    let unit = |u: &PalwLegacyHeldUnitV2| PalwDaUnitV1::LegacyHeldV2(*u);
    let answered = |u: &PalwLegacyHeldUnitV2| view.is_some_and(|v| v.unit_was_answered_v1(&unit(u)));
    if let Some(u) = round.iter().find(|u| answered(u) && answer_read(&unit(u)) && learnable(u)) {
        return PalwFraudFilerStepV1::Learn(PalwLegacyProbeV1::HeldNode { unit: *u });
    }
    let pending = round.iter().filter(|u| !answered(u)).copied();
    let mine = view.and_then(|v| v.sessions.iter().find(|(bond, _)| bond == me)).map(|(_, session)| session);
    let demand: Vec<PalwLegacyProbeV1> = match mine {
        None => pending.take(PALW_LEGACY_LANES_PER_SESSION_V1 as usize).map(|unit| PalwLegacyProbeV1::HeldNode { unit }).collect(),
        Some(session) => {
            let joinable = session.units.iter().all(|u| matches!(u, PalwDaUnitV1::LegacyHeldV2(_)))
                && now_daa <= session.opened_daa.saturating_add(PALW_LEGACY_LANE_JOIN_WINDOW_DAA_V1);
            let room = (PALW_LEGACY_LANES_PER_SESSION_V1 as usize).saturating_sub(session.units.len());
            if joinable {
                pending
                    .filter(|u| !session.units.contains(&unit(u)))
                    .take(room)
                    .map(|unit| PalwLegacyProbeV1::HeldNode { unit })
                    .collect()
            } else {
                Vec::new()
            }
        }
    };
    if !demand.is_empty() {
        return PalwFraudFilerStepV1::DemandLanes { round, demand };
    }
    if round.iter().any(|u| answered(u) && !answer_read(&unit(u)) && learnable(u)) {
        return PalwFraudFilerStepV1::AwaitRound(round);
    }
    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait)
}

/// **Lane G14-L: the bootstrap's parallel round** — after the binding, every input chunk AND the root round's node lanes ride ONE
/// reserved session (the plan's input round: the verifier replays after it, and the root round's frontiers are already on chain).
/// Chunks are learned in order; root nodes are deferred while constraint selection is pending.
/// Falls back to the one-unit bootstrap below the fences, before
/// the binding, or where the claim needs no reservation step.
pub(super) fn palw_fraud_filer_bootstrap_step_v2(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    lanes: Option<PalwFraudFilerLanesV1>,
    read: impl Fn(&PalwDaUnitV1) -> bool,
) -> PalwFraudFilerStepV1 {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let parallel = match (lanes, case.binding.as_ref(), case.fp_bootstrap.as_ref()) {
        (Some(lanes), Some(binding), Some(input))
            if facts.outcome.is_none()
                && !facts.ended
                && facts.reserved
                && facts.accusable
                && !input.ready()
                && input.payload.job().prompt_mode != kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL =>
        {
            Some((lanes, binding, input))
        }
        _ => None,
    };
    let Some((lanes, binding, input)) = parallel else {
        return palw_fraud_filer_bootstrap_step_v1(role, view, me, reservable, case, read);
    };
    use kaspa_consensus_core::palw_legacy_parallel_v1::{
        palw_legacy_input_lanes_v1, palw_legacy_kary_round_units_v1, palw_legacy_kary_width_bits_v1,
    };
    let chunks = palw_legacy_input_lanes_v1(binding, lanes.form);
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let Ok(s) = palw_legacy_kary_width_bits_v1(height, chunks) else {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Unjudged));
    };
    let mut round: Vec<PalwLegacyHeldUnitV2> = (0..chunks).map(|chunk| PalwLegacyHeldUnitV2::PromptIds { chunk }).collect();
    if height > 0
        && !case.constraint_preflight_pending_v1()
        && !(case.public_fp_job_v1().is_some_and(|job| job.is_prefix_state()) && !case.prefix_complete)
    {
        round.extend(palw_legacy_kary_round_units_v1(
            kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyTreeV2::Step,
            binding.step_leaf_count,
            height,
            0,
            s,
        ));
    }
    let next = PalwLegacyHeldUnitV2::PromptIds { chunk: input.chunk };
    palw_fraud_filer_round_step_v1(view, me, round, &|u| *u == next, &read, lanes.now_daa)
}

/// **Lane G14-L: the node's LG14-B step with parallel lanes** — past both legacy fences a reserving public bond with a mismatched
/// base0 replica runs the k-ary descent ([`PalwLegacyReplicaV2::kary`]): one round's node lanes in one reserved session, the fused
/// leaf's witness and the terminal as before. Everything else (no binding yet, a seat, below the fences) is the one-unit step.
#[allow(clippy::too_many_arguments)]
pub(super) fn palw_fraud_filer_legacy_step_v3(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    own_court_open: bool,
    case: &PalwFraudFilerCaseV1,
    lanes: Option<PalwFraudFilerLanesV1>,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> Result<PalwFraudFilerStepV1, String> {
    use kaspa_consensus_core::palw_legacy_parallel_v1::{PalwLegacyKaryStepV1 as K, palw_legacy_input_lanes_v1};
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let applies = lanes.is_some()
        && facts.outcome.is_none()
        && !facts.ended
        && !own_court_open
        && facts.reserved
        && facts.accusable
        && case.binding.is_some()
        && matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if run.legacy.is_some());
    let (Some(lanes), true) = (lanes, applies) else {
        return palw_fraud_filer_legacy_step_v2(role, view, me, reservable, own_court_open, case, answer_read);
    };
    let binding = case.binding.as_ref().expect("checked above");
    let PalwFraudFilerVerdictV1::Mismatch(run) = &case.verdict else { unreachable!("checked above") };
    let replica = run.legacy.as_ref().expect("checked above");
    let step = replica.kary(binding, &case.frontiers, palw_legacy_input_lanes_v1(binding, lanes.form))?;
    let probe = match step {
        K::Round { units, .. } => {
            // A lane whose frontier the case already holds is consumed: never learned again.
            let fresh = |u: &PalwLegacyHeldUnitV2| {
                u.node().is_none_or(|(_, level, index)| !case.frontiers.iter().any(|f| f.level == level && f.index == index))
            };
            return Ok(palw_fraud_filer_round_step_v1(view, me, units, &fresh, &answer_read, lanes.now_daa));
        }
        _ if facts.session_open => return Ok(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait)),
        K::Agrees => return Ok(PalwFraudFilerStepV1::PastStepTree),
        K::FirstDivergentLeaf(leaf) => {
            if !palw_legacy_leaf_is_fused_v2(binding, leaf) || case.witness.is_some() {
                return Ok(PalwFraudFilerStepV1::LegacyTerminal { leaf });
            }
            PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::KernelWitness { leaf } }
        }
        K::OwnTreeInconsistent { .. } | K::OwnTreeUnavailable { .. } | K::Conflict { .. } | K::Refused(_) => {
            return Ok(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Unjudged)));
        }
    };
    let unit = probe.unit();
    Ok(match (view.is_some_and(|view| view.unit_was_answered_v1(&unit)), answer_read(&unit)) {
        (true, true) => PalwFraudFilerStepV1::Learn(probe),
        (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
        _ => PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe)),
    })
}

pub(super) fn palw_fraud_filer_bootstrap_step_v1(
    _role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    read: impl Fn(&PalwDaUnitV1) -> bool,
) -> PalwFraudFilerStepV1 {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    if let Some(outcome) = facts.outcome {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(outcome));
    }
    if facts.ended {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Expired));
    }
    if !facts.reserved {
        return PalwFraudFilerStepV1::Engine(if facts.reservable { PalwFilerActionV1::Reserve } else { PalwFilerActionV1::Wait });
    }
    if facts.session_open || !facts.accusable {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait);
    }
    let probe = if case.binding.is_none() && (case.fp_bootstrap.is_some() || case.fp_job_pending || case.prefix_plan.is_some()) {
        PalwLegacyProbeV1::Binding { row: 0, tile: 0 }
    } else if let (Some(plan), Some(binding)) = (case.prefix_plan.as_ref(), case.binding.as_ref()) {
        match plan.next_unit(binding) {
            Ok(Some(unit)) => PalwLegacyProbeV1::HeldNode { unit },
            _ => return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait),
        }
    } else if let Some(input) = case.fp_bootstrap.as_ref() {
        if input.ready() || input.payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL
        {
            return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait);
        }
        PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::PromptIds { chunk: input.chunk } }
    } else if case.fp_job_pending
        && case.fp_job_disclosure_enabled
        && case.public_fp_job_v1().is_none()
        && case.fp_expected_pin.is_some_and(|pin| pin != Hash64::default())
    {
        PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::FpJob }
    } else {
        return PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait);
    };
    let answered = view.is_some_and(|v| v.unit_was_answered_v1(&probe.unit()));
    match (answered, read(&probe.unit())) {
        (true, true) => PalwFraudFilerStepV1::Learn(probe),
        (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
        _ => PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe)),
    }
}

fn palw_fraud_filer_constraint_step_v1(
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    read: impl Fn(&PalwDaUnitV1) -> bool,
) -> Option<PalwFraudFilerStepV1> {
    let plan = case.constraint_plan.filter(|_| !case.constraint_done)?;
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    if let Some(outcome) = facts.outcome {
        return Some(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(outcome)));
    }
    if facts.ended {
        return Some(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Expired)));
    }
    if !facts.reserved {
        return Some(PalwFraudFilerStepV1::Engine(if facts.reservable {
            PalwFilerActionV1::Reserve
        } else {
            PalwFilerActionV1::Wait
        }));
    }
    if facts.session_open || !facts.accusable {
        return Some(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait));
    }
    for unit in plan.units() {
        if read(&unit) {
            continue;
        }
        let PalwDaUnitV1::Event { row, tile } = unit else { unreachable!() };
        let probe = PalwLegacyProbeV1::Binding { row, tile };
        return Some(if view.is_some_and(|v| v.unit_was_answered_v1(&unit)) {
            PalwFraudFilerStepV1::AwaitAnswer(probe)
        } else {
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe))
        });
    }
    Some(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait))
}

/// The node's LG14-B step: the common reservation/DA/outcome engine, an own replica, and authenticated public frontiers.
pub(super) fn palw_fraud_filer_legacy_step_v2(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    own_court_open: bool,
    case: &PalwFraudFilerCaseV1,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> Result<PalwFraudFilerStepV1, String> {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    if let Some(outcome) = facts.outcome {
        return Ok(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(outcome)));
    }
    if own_court_open && facts.outcome.is_none() {
        return Ok(PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait));
    }
    let descent = match (&case.binding, &case.verdict) {
        (Some(binding), PalwFraudFilerVerdictV1::Mismatch(run))
            if !facts.session_open
                && facts.accusable
                && (facts.reserved || matches!(role, PalwFilerRoleV1::Seat | PalwFilerRoleV1::Watchdog)) =>
        {
            run.legacy.as_ref().ok_or("this replay has no legacy replica")?.descent(binding, &case.frontiers)?
        }
        _ => PalwLegacyDescentStepV2::Agrees,
    };
    let mut action = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_descent_v1(
        role,
        &facts,
        case.pursued(),
        descent,
        |_| false,
    );
    if let PalwFilerActionV1::HeldRoute { leaf } = action {
        let binding = case.binding.as_ref().ok_or("a located leaf needs the authenticated binding")?;
        if !palw_legacy_leaf_is_fused_v2(binding, leaf) || case.witness.is_some() {
            return Ok(PalwFraudFilerStepV1::LegacyTerminal { leaf });
        }
        action = PalwFilerActionV1::Demand(PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::KernelWitness { leaf } });
    }
    if matches!(action, PalwFilerActionV1::Done(PalwFilerPhaseV1::Honest)) && case.pursued() {
        return Ok(PalwFraudFilerStepV1::PastStepTree);
    }
    Ok(match action {
        PalwFilerActionV1::Demand(probe) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.unit_was_answered_v1(&unit)), answer_read(&unit)) {
                (true, true) => PalwFraudFilerStepV1::Learn(probe),
                (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
                _ => PalwFraudFilerStepV1::Engine(action),
            }
        }
        _ => PalwFraudFilerStepV1::Engine(action),
    })
}

/// **G14-V: the node's codec-independent step** — the common engine over the authenticated `StepRange` subtree descent
/// (`palw_fraud_filer_next_subtree_v1`): the claim's committed subtree roots, as authenticated range answers carried them, against
/// the verifier's own tree (built once from its own run). Replaces the linear scan, which refused every claim past 32,768 leaves; an
/// agreeing step tree under a mismatched execution goes past the step tree, never to `Honest`. Blocking: the first call builds the
/// own tree.
pub(super) fn palw_fraud_filer_subtree_step_v1(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> Result<PalwFraudFilerStepV1, String> {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let binding = case.binding.as_ref();
    let descent = match (binding, &case.verdict, &case.step_tree) {
        (Some(binding), PalwFraudFilerVerdictV1::Mismatch(run), Some(tree))
            if !facts.session_open
                && facts.accusable
                && facts.outcome.is_none()
                && (facts.reserved || matches!(role, PalwFilerRoleV1::Seat | PalwFilerRoleV1::Watchdog)) =>
        {
            let own = run.own.as_ref().ok_or("this replay keeps no own run")?;
            let own_tree = own.tree(binding.step_leaf_count)?;
            let range = |first: u64, count: u32| own.range(first, count);
            palw_auth_descent_next_v1(
                tree,
                &|level, index| own_tree.node(level, index, &range),
                kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_BISECT_FINAL_RANGE_V1,
            )
        }
        _ => PalwAuthDescentV1::Agrees,
    };
    let action = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_subtree_v1(
        role,
        &facts,
        case.pursued(),
        binding.map_or(0, |b| b.step_leaf_count),
        descent,
        |leaf| binding.is_some_and(|binding| !palw_da_step_leaf_is_fused_v1(binding, leaf)),
    );
    if matches!(action, PalwFilerActionV1::Done(PalwFilerPhaseV1::Honest)) && case.pursued() {
        return Ok(PalwFraudFilerStepV1::PastStepTree);
    }
    Ok(match action {
        PalwFilerActionV1::Demand(probe) if !matches!(probe, PalwLegacyProbeV1::Terminal { .. }) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.unit_was_answered_v1(&unit)), answer_read(&unit)) {
                (true, true) => PalwFraudFilerStepV1::Learn(probe),
                (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
                (false, _) => PalwFraudFilerStepV1::Engine(action),
            }
        }
        action => PalwFraudFilerStepV1::Engine(action),
    })
}

/// **G14-V: what the public localization past an agreeing step tree does next** — a proof to file, or an event to read.
pub(super) enum PalwFraudFilerPastStepV1 {
    File(PalwConvictionFilingV1),
    Read(PalwLegacyProbeV1),
}

/// **G14-V: past the step tree** (blocking). The claim's step tree is the verifier's own and its execution, trace or output is not.
/// From the binding read (authenticated against the claim's execution root), the public tiled events this case read, and its OWN
/// run: a committed logits lane the agreeing head did not output (`LogitsNotStepOutput`), or a flat committed token that is not its
/// agreeing row's selection (`ForgedOutput`) — each held to the fold's own predicate before it is filed. Any other differing field
/// (job, shape, legs, prompt, output root) is a direct proof's, which the reporter door already ran from the same binding; a
/// differing checkpoint leg with everything else agreeing is reported, not guessed.
pub(super) fn palw_fraud_filer_past_step_v1(
    case: &PalwFraudFilerCaseV1,
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    event0: Option<&PalwDaBuiltAnswerV1>,
    file_by: Option<u64>,
    ladder: u64,
) -> Result<PalwFraudFilerPastStepV1, String> {
    let PalwFraudFilerVerdictV1::Mismatch(run) = &case.verdict else { return Err("no mismatched replay is held".into()) };
    let own = run.own.as_ref().ok_or("this replay keeps no own run")?;
    let binding = case.binding.as_ref().ok_or("past the step tree needs the binding read")?;
    let Some(PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event0))) = event0 else {
        return Err("the binding read's event is not held".into());
    };
    let own_binding = own.binding()?;
    if own_binding.step_merkle_root != binding.step_merkle_root || own_binding.step_leaf_count != binding.step_leaf_count {
        return Err("the step trees differ: the step descent's, not this localization's".into());
    }
    if own_binding.full_logits_trace_root == binding.full_logits_trace_root {
        let differs = [
            ("checkpoint leg", own_binding.checkpoint_merkle_root != binding.checkpoint_merkle_root),
            ("activation leg", own_binding.activation_leg_root != binding.activation_leg_root),
            ("job context", own_binding.job_context != binding.job_context),
            (
                "shape / checkpoint profile",
                own_binding.shape_profile != binding.shape_profile
                    || own_binding.checkpoint_profile != binding.checkpoint_profile
                    || own_binding.state_chunk_map_id != binding.state_chunk_map_id,
            ),
        ];
        let named: Vec<&str> = differs.iter().filter(|(_, d)| *d).map(|(n, _)| *n).collect();
        return Err(format!(
            "the step tree and logits trace are the verifier's own; the execution differs in {named:?} (a direct proof's, or the \
             checkpoint-state localization, which is not wired) — nothing guessed"
        ));
    }
    let next = kaspa_consensus_core::palw_auth_descent_v1::palw_trace_localize_next_v1(
        target,
        binding,
        event0,
        &case.trace,
        &|row, tile| own.event(row, tile),
        &|leaf| own.opening(leaf),
        ladder,
    )?;
    match next {
        PalwTraceLocalizeV1::Read { row, tile } => Ok(PalwFraudFilerPastStepV1::Read(PalwLegacyProbeV1::Binding { row, tile })),
        PalwTraceLocalizeV1::Proof(contradiction) => {
            let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_v1(
                target.claim_id,
                target.executor_bond,
                contradiction,
                None,
            )
            .map_err(|e| e.to_string())?;
            let filing = PalwConvictionFilingV1::of_offence(built.object, target.claim_id, file_by, PalwFilingOriginV1::Replay)
                .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                .ok_or("the reporter's key differs from the proof builder's")?;
            Ok(PalwFraudFilerPastStepV1::File(filing))
        }
    }
}

/// The retired linear-scan step (kept for the engine's unit tests).
#[cfg(test)]
pub(super) fn palw_fraud_filer_step_v1(
    role: PalwFilerRoleV1,
    view: Option<&PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    case: &PalwFraudFilerCaseV1,
    answer_read: impl Fn(&PalwDaUnitV1) -> bool,
) -> PalwFraudFilerStepV1 {
    let facts = palw_fraud_filer_facts_v1(view, me, reservable, case.binding.is_some());
    let binding = case.binding.as_ref();
    let action = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_v1(
        role,
        &facts,
        case.pursued(),
        &case.bisect,
        |leaf| binding.is_some_and(|binding| !palw_da_step_leaf_is_fused_v1(binding, leaf)),
    );
    match action {
        PalwFilerActionV1::Demand(probe) if !matches!(probe, PalwLegacyProbeV1::Terminal { .. }) => {
            let unit = probe.unit();
            match (view.is_some_and(|view| view.unit_was_answered_v1(&unit)), answer_read(&unit)) {
                (true, true) => PalwFraudFilerStepV1::Learn(probe),
                (true, false) => PalwFraudFilerStepV1::AwaitAnswer(probe),
                (false, _) => PalwFraudFilerStepV1::Engine(action),
            }
        }
        action => PalwFraudFilerStepV1::Engine(action),
    }
}

/// A direct proof from one public, authenticated event and the claim facts the tip recorded.
/// No producer capture, own replay, Panel receipt or answered-session flag enters this decision.
#[allow(dead_code)] // dormant or test-only: kept, not yet wired to a caller in this build
pub(super) fn palw_fraud_filer_public_filing_v1(
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    rules: kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1,
    answer: &PalwDaBuiltAnswerV1,
    file_by: Option<u64>,
) -> Result<Option<PalwConvictionFilingV1>, String> {
    palw_fraud_filer_public_filing_with_fp_v2(target, rules, answer, file_by, None)
}

/// The public FP job is authenticated by the core proof's claim/binding checks. No input ids or
/// successful replay is required to prosecute a self-consistent bad canonical count/root.
#[allow(dead_code)] // dormant or test-only: kept, not yet wired to a caller in this build
pub(super) fn palw_fraud_filer_public_filing_with_fp_v2(
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    rules: kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1,
    answer: &PalwDaBuiltAnswerV1,
    file_by: Option<u64>,
    fp_job: Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
) -> Result<Option<PalwConvictionFilingV1>, String> {
    palw_fraud_filer_public_filing_with_budget_v3(
        target,
        rules,
        answer,
        file_by,
        fp_job,
        kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES,
    )
}

pub(super) fn palw_fraud_filer_public_filing_with_budget_v3(
    target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
    rules: kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1,
    answer: &PalwDaBuiltAnswerV1,
    file_by: Option<u64>,
    fp_job: Option<&kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
    maximum: u64,
) -> Result<Option<PalwConvictionFilingV1>, String> {
    use kaspa_consensus_core::palw_offence_attribution_v1::{
        palw_binding_identity_fault_v1, palw_fp_canonical_prompt_admit_v1, palw_output_fault_v1, palw_prompt_not_anchored_admit_v1,
        palw_prompt_not_anchored_fault_v1,
    };
    use kaspa_consensus_core::palw_offence_v1::{PalwPanelContradictionV1, PalwPromptProofV1};
    use kaspa_consensus_core::palw_step_refute::{PalwDecodeTokenPinV1, PalwTiledDecodeTokensV1, PalwTraceEventDisclosureV1};
    let PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event)) = answer else { return Ok(None) };
    let event_bytes = borsh::object_length(event).map_err(|e| e.to_string())? as u64;
    if event_bytes > maximum.min(kaspa_consensus_core::palw_terminal_carriage_v1::PALW_TERMINAL_MAX_BYTES_V1) {
        return Err("the public event exceeds its admitted terminal evidence envelope".into());
    }
    let binding = event.binding();
    if !kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_answer_authenticates_v1(
        &PalwDaUnitV1::Event { row: 0, tile: 0 },
        &PalwDaAnswerV1::Event(event.clone()),
        &target.execution_root,
    ) {
        return Err("the public event does not authenticate to this claim".into());
    }
    let structural = kaspa_consensus_core::palw_step_leg::PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: kaspa_consensus_core::palw_step_leg::PalwStepEvidenceV1::Shape,
    };
    let fp_proof = fp_job.and_then(|job| {
        let prefill = palw_fp_canonical_prompt_admit_v1(target, binding, job, rules).ok()?;
        if binding.job_context.declared_prefill_tokens == prefill
            && u64::from(prefill) > kaspa_consensus_core::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1
        {
            return None;
        }
        Some(PalwPromptProofV1::FpCanonicalWhole { job: Box::new(job.clone()) })
    });
    let contradiction = if palw_binding_identity_fault_v1(target, binding, rules, true).map_err(|e| e.to_string())?.is_some() {
        PalwPanelContradictionV1::IdentityMismatch { binding: binding.clone() }
    } else if let Some(job) = fp_job
        && kaspa_consensus_core::palw_offence_attribution_v1::palw_registered_table_fault_v1(target, binding, job)
            .is_ok_and(|fault| fault)
    {
        PalwPanelContradictionV1::RegisteredTableMismatch { binding: binding.clone(), job: Box::new(job.clone()) }
    } else if kaspa_consensus_core::palw_step_leg::check_step_refutation_capped_v1(&structural, binding.step_leaf_count).is_ok() {
        PalwPanelContradictionV1::StepStructural(structural)
    } else if let Some(proof) = fp_proof
        && palw_prompt_not_anchored_fault_v1(target, binding, &proof, rules).map_err(|e| e.to_string())?
    {
        PalwPanelContradictionV1::PromptNotAnchored { binding: binding.clone(), proof }
    } else if palw_prompt_not_anchored_admit_v1(target, binding, rules)
        .is_ok_and(|prefill| u64::from(prefill) <= kaspa_consensus_core::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1)
        && palw_prompt_not_anchored_fault_v1(target, binding, &PalwPromptProofV1::Whole, rules).map_err(|e| e.to_string())?
    {
        // Derive the anchor's prompt, not the producer's ids. The real gate still charges this Whole proof's heavy budget and rent.
        PalwPanelContradictionV1::PromptNotAnchored { binding: binding.clone(), proof: PalwPromptProofV1::Whole }
    } else {
        let pin = match event {
            PalwTraceEventDisclosureV1::Flat { pin, .. } => PalwDecodeTokenPinV1::Base0V1(pin.clone()),
            PalwTraceEventDisclosureV1::Tiled { generated_token_ids, row_opening, .. } => {
                let rows_root = kaspa_consensus_core::palw_step_leg::step_opening_root_capped_v1(
                    u64::from(binding.job_context.exact_decode_tokens),
                    row_opening,
                    binding.step_leaf_count,
                )
                .map_err(|e| e.to_string())?;
                PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: generated_token_ids.clone() })
            }
            PalwTraceEventDisclosureV1::OutOfRange { .. } => return Ok(None),
        };
        if palw_output_fault_v1(target, binding, &pin).map_err(|e| e.to_string())? {
            PalwPanelContradictionV1::OutputMismatch { binding: binding.clone(), pin }
        } else if let Some(position) = match &pin {
            PalwDecodeTokenPinV1::Base0V1(tokens) => tokens.generated_token_ids.iter(),
            PalwDecodeTokenPinV1::TiledV1(tokens) => tokens.generated_token_ids.iter(),
            PalwDecodeTokenPinV1::FloatV2(_) => return Ok(None),
        }
        .position(|id| *id >= binding.shape_profile.vocab_size)
        {
            // The event already authenticates these IDs. This proof needs neither a host token
            // table nor another logits tile; the core gate applies the block's public-filer fence.
            match pin {
                PalwDecodeTokenPinV1::Base0V1(pin) => {
                    PalwPanelContradictionV1::ForgedOutput { binding: binding.clone(), pin, position: position as u32 }
                }
                PalwDecodeTokenPinV1::TiledV1(tokens) => PalwPanelContradictionV1::ForgedOutputTiled {
                    binding: binding.clone(),
                    proof: kaspa_consensus_core::palw_offence_v1::PalwForgedOutputTiledProofV1::OutOfVocab {
                        position: position as u32,
                        tokens,
                    },
                },
                PalwDecodeTokenPinV1::FloatV2(_) => unreachable!(),
            }
        } else if let (Some(job), PalwTraceEventDisclosureV1::Flat { pin, .. }) = (fp_job.filter(|j| j.is_constraint()), event) {
            let table = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_token_table_for_job_v1(job)
                .ok_or("the public verifier does not hold this job's tokenizer table")?;
            let Some(proof) = constraint::flat_contradiction_with_budget_v2(target, binding, job, pin, table, maximum)? else {
                return Ok(None);
            };
            proof
        } else {
            return Ok(None);
        }
    };
    let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
        target.claim_id,
        target.executor_bond,
        contradiction,
        None,
        maximum,
    )
    .map_err(|e| e.to_string())?;
    let filing = PalwConvictionFilingV1::of_offence(built.object, target.claim_id, file_by, PalwFilingOriginV1::Replay)
        .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
        .ok_or("the reporter's key differs from the proof builder's")?;
    Ok(Some(filing))
}

/// The court round a filer item rides (`court_due` / `court_moved` key).
pub(super) fn palw_fraud_filer_item_round_v1(item: PalwFraudFilerSentV1) -> u32 {
    match item {
        PalwFraudFilerSentV1::Reserve => PALW_FRAUD_FILER_RESERVE_ROUND_V1,
        PalwFraudFilerSentV1::Release => PALW_FRAUD_FILER_RELEASE_ROUND_V1,
        PalwFraudFilerSentV1::Demand(_) | PalwFraudFilerSentV1::Reacquire { .. } => PALW_FRAUD_FILER_DEMAND_ROUND_V1,
        PalwFraudFilerSentV1::Terminal { .. } => PALW_FRAUD_FILER_TERMINAL_ROUND_V2,
    }
}

#[cfg(test)]
mod g_d02b_tests {
    use super::*;

    /// G-D02c: while one claim of a producer is the focus, its other claims are deferred until their Final deadline is near; claims of
    /// other producers, and the focus itself, never are.
    #[test]
    fn a_producers_other_claims_wait_for_the_focus_until_their_deadline_is_near() {
        let p = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Default::default(), 0));
        let q = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Default::default(), 1));
        let (focus_claim, other) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let mut focus = BTreeMap::new();
        assert!(!palw_fraud_filer_defer_reservation_v1(&focus, &p, &other, None, 10), "no focus: reserve");
        focus.insert(p, focus_claim);
        assert!(!palw_fraud_filer_defer_reservation_v1(&focus, &p, &focus_claim, None, 10), "the focus itself");
        assert!(palw_fraud_filer_defer_reservation_v1(&focus, &p, &other, None, 10), "no Final deadline yet: wait");
        assert!(palw_fraud_filer_defer_reservation_v1(&focus, &p, &other, Some(10 + PALW_FRAUD_FILER_FOCUS_MARGIN_DAA_V1 + 1), 10));
        assert!(!palw_fraud_filer_defer_reservation_v1(&focus, &p, &other, Some(10 + PALW_FRAUD_FILER_FOCUS_MARGIN_DAA_V1), 10), "near: reserve");
        assert!(!palw_fraud_filer_defer_reservation_v1(&focus, &q, &other, None, 10), "another producer");
    }

    /// G-D02b: an item carried and then asked for again (its block reorged out) is resent at once and does not spend the retry
    /// budget; an item never carried keeps the normal spacing and budget.
    #[test]
    fn a_reorged_out_item_is_resent_at_once_without_spending_its_budget() {
        let mut case = PalwFraudFilerCaseV1::new(super::tests::candidate_for_input_test());
        let item = PalwFraudFilerSentV1::Reserve;
        assert_eq!(case.may_send(item, 10), Ok(true));
        case.note_sent(item, 10);
        assert_eq!(case.may_send(item, 11), Ok(false), "spacing before any carriage");
        case.carried = Some((palw_fraud_filer_item_round_v1(item), 12));
        assert_eq!(case.may_send(item, 13), Ok(true), "carried, then asked again: reorged out, resend at once");
        case.note_sent(item, 13);
        assert_eq!(case.sent.map(|s| s.2), Some(1), "the reorged resend does not spend the retry budget");
        assert_eq!(case.carried, None);
        assert_eq!(case.may_send(item, 14), Ok(false), "normal spacing again");
        for _ in 0..PALW_FRAUD_FILER_SENDS_V1 {
            case.note_sent(item, 14);
        }
        assert_eq!(case.may_send(item, 14 + PALW_FRAUD_FILER_RESEND_DAA_V1), Err(()), "the budget still bounds plain retries");
        case.carried = Some((PALW_FRAUD_FILER_DEMAND_ROUND_V1, 20));
        assert!(!case.reorged_out_v1(item), "another round's carriage says nothing about this item");
    }
}

pub(super) fn palw_fraud_filer_sign_terminal_v2(
    object: PalwConsensusObjectV2,
    domain: &Hash64,
    ceiling: u64,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    let object = match object {
        PalwConsensusObjectV2::LegacyLeafRecomputedV2 { accusation } => {
            if kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_leaf_recompute_bytes_v2(&accusation) > ceiling {
                return Err("the public leaf recompute exceeds the close ceiling".into());
            }
            crate::palw_legacy_held_v2::palw_legacy_leaf_recompute_object_v2(domain, *accusation, sign)?
        }
        PalwConsensusObjectV2::ShardCourtAccused { mut accusation } => {
            use kaspa_consensus_core::palw_shard_court_v1::{PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, palw_shard_court_session_id_v1};
            if kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_accusation_bytes_v1(&accusation) > ceiling {
                return Err("the public fused accusation exceeds the close ceiling".into());
            }
            let sid = palw_shard_court_session_id_v1(domain.as_byte_slice(), &accusation);
            accusation.signature = sign(sid.as_byte_slice(), PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT)
                .filter(|s| !s.is_empty())
                .ok_or("no signing key for the fused accusation")?;
            PalwConsensusObjectV2::ShardCourtAccused { accusation }
        }
        _ => return Err("not a public legacy terminal".into()),
    };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(|e| e.to_string())?;
    Ok(object)
}

/// Run only the verifier's own backend. FP execution uses the job's controls and determines its executed context itself.
#[allow(clippy::too_many_arguments)]
pub(super) fn palw_fraud_filer_execute_v1(
    backend: Box<dyn PalwExecutionBackendV1>,
    mut ctx: PalwJobContextV2,
    prompt32: Vec<u32>,
    prompt_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    attempt_draw: Option<bool>,
    fp_job: Option<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3>,
    reserved: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
) -> Result<PalwFraudFilerRunV1, String> {
    let prompt: Vec<usize> = prompt32.iter().map(|id| *id as usize).collect();
    let job_reservation = fp_job
        .as_ref()
        .map(|job| {
            let bytes = borsh::object_length(job).map_err(|e| e.to_string())? as u64;
            crate::palw_memory_ledger::host_ledger_v1()
                .reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "public-replay-job",
                        class_id: job.class_id,
                        job: ctx.job_id,
                    },
                    bytes.saturating_add(1024).saturating_mul(8),
                )
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let mut own_output_ids = Vec::new();
    let outcome = if let Some(job) = fp_job.as_ref() {
        // The public replay owns its scope just as the producer and interval verifier do.
        // A constraint's authenticated job tail and this host's matching token table determine
        // the mask; a missing/mismatched table is a host refusal, never a fraud verdict.
        let mask = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_mask_for_host_v1(
            job,
            backend.token_table_for_job_v1(job),
        )?;
        let run = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_with_constraint_scope_v1(mask, || {
            backend.execute_free_prompt(job, &prompt)
        })?;
        ctx = backend
            .fp_job_context_for_executed_v1(job, run.facts.decode_tokens_executed)
            .ok_or("the backend cannot derive its own FP executed context")?;
        if job.is_constraint() {
            own_output_ids = run.output_token_ids;
        }
        run.outcome
    } else {
        backend.execute(&ctx, &prompt)?
    };
    let backend: Arc<dyn PalwExecutionBackendV1> = Arc::from(backend);
    let material = Arc::new(outcome.material);
    let prompt32 = Arc::new(prompt32);
    let own_backend = backend.clone();
    let own_material = material.clone();
    let own_prompt = prompt32.clone();
    let own = Arc::new(PalwFraudFilerOwnRunV1::new(backend.clone(), material.clone(), prompt32.clone()));
    let legacy = misaka_palw_base0::produce::base0_material_decode_any_v1(&material).is_ok().then(|| {
        Arc::new(PalwLegacyReplicaV2 {
            backend,
            capture: material,
            prompt_ids: prompt32,
            form: prompt_form,
            roots: kaspa_consensus_core::palw_backend::PalwClaimRootsV1 {
                execution_root: outcome.execution_root,
                trace_root: outcome.trace_root,
                anchor: ctx.job_id,
                attempt_draw: if fp_job.is_some() { None } else { attempt_draw },
                output_root: fp_job.as_ref().map(|_| outcome.output_root),
                job_pin: fp_job.as_ref().map(|_| kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&ctx)),
            },
        })
    });
    Ok(PalwFraudFilerRunV1 {
        execution_root: outcome.execution_root,
        trace_root: outcome.trace_root,
        output_root: outcome.output_root,
        own_range: Box::new(move |first, count| {
            own_backend.held_step_range_answer_v1(&own_material, &own_prompt, first, count).map(|opening| opening.leaf_hashes)
        }),
        legacy,
        own: Some(own),
        fp_job: fp_job.map(Box::new),
        own_output_ids,
        _job_reservation: job_reservation,
        _reservation: reserved,
    })
}

impl PalwPanelService {
    /// **The filer's half of the tick** (the module's header): read the candidates and the accepted answers once a DAA, poll and judge
    /// the replay in flight, start the next one where the seat's slots have room, and step every pursued case through the engine —
    /// each item asked of the fold or built by the one builder, queued on the court lane dated now.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn fraud_filer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        reporter: &mut PalwReporterFilerV1,
        false_valid: &mut crate::palw_filer_false_valid::PalwFalseValidFilerV1,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        seat_replay_room: bool,
        public_materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        // What the priority lane carried of this filer's items since the last tick, said and forgotten here: the filer keeps its own
        // spacing (`sent`), never `court_moved`'s debounce.
        let mut carried = Vec::new();
        court_moved.retain(|(claim, round, responder), at| {
            if !palw_fraud_filer_queued_v1(*round, *responder) {
                return true;
            }
            info!("[{PALW_PANEL}] claim {claim}: the fraud filer's item (round {round}) was carried at DAA {at} (LG14-A)");
            carried.push((*claim, *round, *at));
            false
        });
        for (claim, round, at) in carried {
            if let Some(case) = book.cases.get_mut(&claim) {
                case.carried = Some((round, at));
            }
        }
        let params = &self.consensus_config.params;
        if !(params.palw_legacy_public_filer_active_at(current_daa) && self.config.fee_outpoint.is_some()) {
            if book.running.as_ref().is_some_and(|replay| replay.handle.is_finished()) {
                let replay = book.running.take().expect("finished above");
                drop(replay.handle.await); // Discard the result, releasing its capture and reservation.
            }
            if !book.cases.is_empty() || book.running.is_some() {
                book.clear();
            }
            court_pending.retain(|(_, round, responder, _)| !palw_fraud_filer_queued_v1(*round, *responder));
            return;
        }
        book.network_form = Some(params.palw_prompt_ids_form_at(current_daa));
        if book.stale(current_daa) {
            let targets = self.config.public_claims.clone();
            book.target_claims = targets.iter().copied().collect();
            let candidates = session
                .clone()
                .spawn_blocking(move |c| {
                    if targets.is_empty() {
                        c.palw_fraud_filer_candidates_v1(bond_key)
                    } else {
                        c.palw_fraud_filer_candidates_for_claims_v1(bond_key, targets)
                    }
                })
                .await;
            let invalidated = book.refresh(candidates, current_daa);
            if book.deferred_candidates != 0 {
                crate::palw_backends::note_throttled_v1("public-filer-discovery-cap", || {
                    format!(
                        "[{PALW_PANEL}] {} public candidates deferred by this host's bounded discovery; select exact claims with --palw-public-claim",
                        book.deferred_candidates
                    )
                });
            }
            court_pending.retain(|(claim, round, responder, _)| {
                !invalidated.contains(claim) || !palw_fraud_filer_queued_v1(*round, *responder)
            });
            court_due.retain(|(claim, round, responder), _| {
                !invalidated.contains(claim) || !palw_fraud_filer_queued_v1(*round, *responder)
            });
            if book.role.is_none() {
                let operator = palw_operator_da::palw_operator_registrations_v1(params).iter().any(|(bond, _)| *bond == bond_key);
                book.role = Some(if operator { PalwFilerRoleV1::Operator } else { PalwFilerRoleV1::PublicBond });
            }
            if let Some(floor) = book.walk_floor() {
                let wanted: BTreeMap<Hash64, Hash64> = book
                    .cases
                    .values()
                    .filter(|case| case.pursued())
                    .map(|case| (case.candidate.claim_id, case.candidate.job.execution_root))
                    .collect();
                let oldest = book.cases.values().filter(|case| case.pursued()).map(|case| case.candidate.accepted_daa).min().unwrap();
                let previous = book.walk;
                let mut legacy_wanted: BTreeMap<Hash64, PalwLegacyHeldUnitV2> =
                    book.cases.values().filter_map(|case| case.legacy_wanted.map(|unit| (case.candidate.claim_id, unit))).collect();
                let event_wanted = book
                    .cases
                    .values()
                    .filter_map(|case| {
                        case.constraint_plan.filter(|_| !case.constraint_done).map(|plan| (case.candidate.claim_id, plan.units()))
                    })
                    .collect();
                let mut legacy_rounds: BTreeMap<Hash64, BTreeSet<PalwLegacyHeldUnitV2>> = book
                    .cases
                    .values()
                    .filter(|case| !case.legacy_round.is_empty())
                    .map(|case| (case.candidate.claim_id, case.legacy_round.iter().copied().collect()))
                    .collect();
                let rcore_wanted: BTreeMap<Hash64, PalwDaUnitV1> =
                    book.cases.values().filter_map(|case| case.rcore_wanted.map(|unit| (case.candidate.claim_id, unit))).collect();
                let history_form = params.palw_prompt_ids_form_at(current_daa);
                let history_maximum = self
                    .config
                    .court
                    .max_close_bytes()
                    .min(kaspa_consensus_core::palw_legacy_answer_chunks_v2::PALW_LEGACY_ANSWER_MAX_BYTES_V2);
                let ledger = crate::palw_memory_ledger::host_ledger_v1();
                let mut retained = Vec::new();
                for (claim, case) in &mut book.cases {
                    if !case.pursued() {
                        continue;
                    }
                    match case.reserve_legacy_material_v1(&ledger, history_maximum, history_form) {
                        Ok(()) => retained.extend(case.legacy_material_tickets.values().cloned()),
                        Err(why) => {
                            // Memory pressure is Pending. Do not read/decode an unfunded round
                            // and do not suppress other claims' admitted public progress.
                            legacy_wanted.remove(claim);
                            legacy_rounds.remove(claim);
                            debug!("[{PALW_PANEL}] claim {claim}: {why}; retained history unchanged");
                        }
                    }
                }
                let scratch = crate::palw_memory_ledger::host_ledger_v1().reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "public-answer-history",
                        class_id: Hash64::default(),
                        job: previous.map_or(Hash64::default(), |w| w.anchor),
                    },
                    8 * history_maximum.saturating_add(16_384),
                );
                let result = match scratch {
                    Ok(scratch) => {
                        session
                            .clone()
                            .spawn_blocking(move |c| {
                                let _scratch = scratch;
                                palw_fraud_filer_read_page_with_budget_v2(
                                    c,
                                    oldest,
                                    floor,
                                    previous,
                                    PALW_FRAUD_FILER_WALK_BLOCKS_V1,
                                    &wanted,
                                    &legacy_wanted,
                                    &legacy_rounds,
                                    &rcore_wanted,
                                    &event_wanted,
                                    history_form,
                                    history_maximum,
                                )
                                .map(|page| (page, retained))
                            })
                            .await
                    }
                    Err(why) => Err(format!("public history waits for memory: {why}")),
                };
                match result {
                    Ok(((walk, reset, answers), _retained)) => {
                        if reset {
                            let restarted: BTreeSet<Hash64> = book.pursued().into_iter().collect();
                            court_pending.retain(|(claim, round, responder, _)| {
                                !restarted.contains(claim) || !palw_fraud_filer_queued_v1(*round, *responder)
                            });
                            court_due.retain(|(claim, round, responder), _| {
                                !restarted.contains(claim) || !palw_fraud_filer_queued_v1(*round, *responder)
                            });
                        }
                        book.walked(walk, reset, answers);
                    }
                    Err(why) => {
                        warn!("[{PALW_PANEL}] the fraud filer's public history page is unavailable: {why}; watermark unchanged")
                    }
                }
            }
        }
        // The replay in flight, polled every tick until it returns.
        if book.running.as_ref().is_some_and(|replay| replay.handle.is_finished()) {
            let replay = book.running.take().expect("held above");
            let claim = replay.candidate.claim_id;
            let result = replay.handle.await.unwrap_or_else(|e| Err(format!("the replay task did not finish: {e}")));
            match book.judge_replay(&replay.candidate, result) {
                Some(PalwFraudFilerVerdictV1::Mismatch(run)) => error!(
                    "[{PALW_PANEL}] claim {claim}: this node's replay of the claim's job does NOT reproduce its execution root (replayed \
                     {}) — every seat of its panel let it pass or none looked; pursuing it as a public bond (LG14-A, RFC-0014 §6)",
                    run.execution_root
                ),
                Some(PalwFraudFilerVerdictV1::Honest) => {
                    #[cfg(test)]
                    {
                        let mut observed = self.fraud_filer_honest_replays.lock().unwrap();
                        if observed.len() >= PALW_FRAUD_FILER_MAX_CASES_V1 {
                            observed.clear();
                        }
                        observed.insert(claim);
                    }
                    debug!("[{PALW_PANEL}] claim {claim}: the fraud filer's replay reproduces the claim — honest, nothing filed")
                }
                Some(PalwFraudFilerVerdictV1::Unjudged(why)) => {
                    warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot judge the claim — {why} (LG14-A)")
                }
                _ => {}
            }
        }
        // A rooted paid semantic row survives a sibling DA outcome. Read accepted authority
        // before scheduling a fresh replay: continuation is not a new legacy reservation.
        let _authority_memory = if params.palw_staged_public_court_v1_active_at(current_daa) {
            let bytes = (book.cases.len() as u64).saturating_mul(1024).saturating_add(16_384);
            match crate::palw_memory_ledger::host_ledger_v1().reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-paid-authority-read",
                    class_id: Hash64::default(),
                    job: Hash64::default(),
                },
                bytes,
            ) {
                Ok(ticket) => Some(ticket),
                Err(why) => {
                    debug!("[{PALW_PANEL}] paid semantic discovery waits for memory: {why}");
                    return;
                }
            }
        } else {
            None
        };
        let staged_authority = if params.palw_staged_public_court_v1_active_at(current_daa) {
            let claims: Vec<_> = book.cases.keys().copied().collect();
            session
                .clone()
                .spawn_blocking(move |c| {
                    claims
                        .into_iter()
                        .map(|claim| (claim, c.palw_staged_paid_authority_v1((claim, bond_key))))
                        .collect::<HashMap<_, _>>()
                })
                .await
        } else {
            HashMap::new()
        };
        for (claim, authority) in &staged_authority {
            if let Ok(Some(authority)) = authority {
                book.observe_paid_authority_v1(*claim, authority, current_daa);
            }
        }
        book.make_replay_room(current_daa, seat_replay_room);
        if let Some(claim) = book.next_probe_at(current_daa, seat_replay_room) {
            self.fraud_filer_start_replay_v1(
                session,
                book,
                claim,
                network_domain,
                current_daa,
                public_materials.get(&claim).map(Vec::as_slice).unwrap_or(&[]),
                seat_replay_room,
            )
            .await;
        }
        // Every pursued case, one engine step each (a step that reads an answer runs on to the next).
        let role = book.role.unwrap_or(PalwFilerRoleV1::PublicBond);
        let form = params.palw_prompt_ids_form_at(current_daa);
        for claim in book.pursued() {
            let (view, reservable, own_court_open, target) = session
                .clone()
                .spawn_blocking(move |c| {
                    let view = c.palw_legacy_dispute_v1(claim);
                    let reservable = view
                        .as_ref()
                        .and_then(|view| c.palw_legacy_dispute_reservation_check_v1(palw_fraud_filer_reservation_v1(view, bond_key)))
                        .is_some_and(|check: Result<u128, String>| check.is_ok());
                    let own_court_open =
                        c.palw_court_duties_v2(vec![bond_key]).iter().any(|duty| duty.claim_id == claim && !duty.i_am_responder);
                    (view, reservable, own_court_open, c.palw_fraud_filer_target_v1(claim))
                })
                .await;
            if let Some(case) = book.cases.get_mut(&claim) {
                case.fp_expected_pin = target.as_ref().map(|target| target.job_identity);
                case.fp_job_disclosure_enabled = params.palw_legacy_held_da_v2_active_at(current_daa);
            }
            if params.palw_staged_public_court_v1_active_at(current_daa)
                && book.cases.get(&claim).is_some_and(|case| {
                    case.tir.is_none()
                        && case.candidate.job.free_prompt
                        && (case.binding.is_none() || case.public_fp_job_v1().is_none())
                })
                && let Err(why) = self.fraud_filer_public_context_v1(session, book, claim, bond_key)
            {
                debug!("[{PALW_PANEL}] claim {claim}: original public header bootstrap remains pending: {why}");
            }
            let facts = palw_fraud_filer_facts_v1(
                view.as_ref(),
                &bond_key,
                reservable,
                book.cases.get(&claim).is_some_and(|case| case.binding.is_some()),
            );
            if let Some(Err(why)) = staged_authority.get(&claim) {
                debug!("[{PALW_PANEL}] claim {claim}: paid semantic authority snapshot unavailable: {why}");
                continue;
            }
            let tir_release = book.cases.get(&claim).is_some_and(|case| {
                case.tir.is_some() && case.release_pending && matches!(case.verdict, PalwFraudFilerVerdictV1::Honest)
            });
            if tir_release && !facts.reserved && !facts.session_open {
                book.settle(&claim, PalwFraudFilerVerdictV1::Honest);
                continue;
            }
            if staged_authority.get(&claim).is_some_and(|p| {
                matches!(p, Ok(Some(p))
                if p.terms.accepted_daa <= p.reserved_daa && p.reserved_daa <= current_daa && current_daa < p.terms.collect_until_daa)
            }) && (!facts.reserved
                || facts.ended
                || facts.outcome.is_some()
                || book.cases.get(&claim).is_some_and(|case| {
                    !case.prefix_complete && !case.rendering_done && (case.binding.is_none() || case.public_fp_job_v1().is_none())
                }))
                && facts.outcome != Some(PalwFilerPhaseV1::Convicted)
                && !(tir_release && facts.reserved && !facts.ended && facts.outcome.is_none())
            {
                court_pending.retain(|(held, round, responder, _)| {
                    *held != claim
                        || !palw_fraud_filer_queued_v1(*round, *responder)
                        || (*round == PALW_FRAUD_FILER_STAGED_ROUND_V1 && !*responder)
                });
                let authority = staged_authority[&claim].as_ref().ok().and_then(Option::as_ref).expect("paid authority above");
                let bytes = match authority.terms.plan.resident_ram_bytes.checked_mul(8).and_then(|n| n.checked_add(16_384)) {
                    Some(bytes) => bytes,
                    None => {
                        debug!("[{PALW_PANEL}] claim {claim}: retained paid snapshot copy bound overflow");
                        continue;
                    }
                };
                let _paid_snapshot_memory = match crate::palw_memory_ledger::host_ledger_v1().reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "public-paid-snapshot-read",
                        class_id: book.cases[&claim].candidate.job.class_id,
                        job: claim,
                    },
                    bytes,
                ) {
                    Ok(ticket) => ticket,
                    Err(why) => {
                        debug!("[{PALW_PANEL}] claim {claim}: retained paid snapshot waits for memory: {why}");
                        continue;
                    }
                };
                let progress = match session.palw_staged_owned_progress_v1((claim, bond_key)) {
                    Ok(Some(progress))
                        if palw_fraud_filer_paid_progress_v1(&progress, claim, bond_key)
                            // Paid authority names the ACTIVE literal stage (its seed digest and terms)
                            // once a bundle stage is open, and the bundle frame before that — exactly
                            // what the owned progress reports. Comparing the frame forever after
                            // activation would read as a perpetual snapshot change.
                            && (progress.reserved_daa, progress.seed_id, progress.terms)
                                == (authority.reserved_daa, Some(authority.seed_id), authority.terms) =>
                    {
                        progress
                    }
                    Ok(Some(progress)) if palw_fraud_filer_paid_progress_v1(&progress, claim, bond_key) => {
                        debug!("[{PALW_PANEL}] claim {claim}: accepted paid authority changed between snapshots; retry unchanged");
                        continue;
                    }
                    Ok(_) => {
                        court_pending
                            .retain(|(held, round, responder, _)| *held != claim || !palw_fraud_filer_queued_v1(*round, *responder));
                        book.settle(&claim, PalwFraudFilerVerdictV1::Settled(facts.outcome.unwrap_or(PalwFilerPhaseV1::Expired)));
                        continue;
                    }
                    Err(why) => {
                        debug!("[{PALW_PANEL}] claim {claim}: retained paid cursor remains unavailable: {why}");
                        continue;
                    }
                };
                match self
                    .fraud_filer_paid_stage_v1(session, book, claim, bond_key, target.as_ref(), progress, court_pending, court_due)
                    .await
                {
                    Ok((Some(filing), _)) => {
                        false_valid.note_executor_refuted_v1(
                            crate::palw_filer_false_valid::palw_false_valid_armed_v1(
                                params,
                                self.config.fee_outpoint.is_some(),
                                current_daa,
                            ),
                            &filing.object,
                            Some(view.as_ref().map_or(book.cases[&claim].candidate.accepted_daa, |v| v.accepted_daa)),
                            current_daa,
                        );
                        let mut door = self.conviction_door_v1(session, reporter, bond_key, network_domain);
                        let case = book.cases.get_mut(&claim).expect("paid pursuit retained");
                        if !door.holds(&filing.offence_key)
                            && case.direct_handed < super::reporter_filer::PALW_FILER_HAND_OFFS_PER_OFFENCE_V1
                            && case.direct_asked_at.is_none_or(|at| current_daa.saturating_sub(at) >= PALW_FRAUD_FILER_RESEND_DAA_V1)
                        {
                            case.direct_asked_at = Some(current_daa);
                            match door.file(filing) {
                                PalwFileOutcomeV1::Queued { .. } | PalwFileOutcomeV1::AlreadyFiled => case.direct_handed += 1,
                                PalwFileOutcomeV1::AlreadyConvicted => {}
                                why => {
                                    warn!("[{PALW_PANEL}] claim {claim}: retained paid semantic proof waits at reporter door: {why:?}")
                                }
                            }
                        }
                    }
                    Ok((None, true)) => {
                        let case = book.cases.get_mut(&claim).expect("paid pursuit retained");
                        if !case.finish_tir_semantics_v1()
                            && facts.reserved
                            && !facts.ended
                            && facts.outcome.is_none()
                            && !case.arithmetic_matched
                        {
                            case.verdict = PalwFraudFilerVerdictV1::Pending;
                        }
                    }
                    Ok((None, _)) => {}
                    Err(why) => debug!("[{PALW_PANEL}] claim {claim}: retained paid semantic continuation remains pending: {why}"),
                }
                // Paid authority never authorizes a new seed, DA demand, reserve or release.
                continue;
            }
            if facts.ended || facts.outcome.is_some() {
                court_pending.retain(|(held, round, responder, _)| *held != claim || !palw_fraud_filer_queued_v1(*round, *responder));
                book.settle(&claim, PalwFraudFilerVerdictV1::Settled(facts.outcome.unwrap_or(PalwFilerPhaseV1::Expired)));
                continue;
            }
            if book.cases.get(&claim).is_some_and(|case| case.release_pending) {
                // Stop unsent bootstrap moves; a handed-off reservation may still land. Observe it
                // for one resend interval before forgetting a currently unreserved case.
                court_pending.retain(|(held, round, responder, _)| {
                    *held != claim || !palw_fraud_filer_queued_v1(*round, *responder) || *round == PALW_FRAUD_FILER_RELEASE_ROUND_V1
                });
                if !facts.reserved {
                    court_pending
                        .retain(|(held, round, responder, _)| *held != claim || !palw_fraud_filer_queued_v1(*round, *responder));
                    let case = book.cases.get_mut(&claim).expect("held above");
                    let reservation_in_flight = matches!(case.sent,
                        Some((PalwFraudFilerSentV1::Reserve, at, _))
                        if current_daa.saturating_sub(at) < PALW_FRAUD_FILER_RESEND_DAA_V1);
                    if !reservation_in_flight {
                        case.release_pending = false;
                        case.sent = None;
                    }
                } else if !facts.session_open
                    && !court_pending
                        .iter()
                        .any(|(held, round, responder, _)| *held == claim && palw_fraud_filer_queued_v1(*round, *responder))
                {
                    self.fraud_filer_queue_v1(
                        book,
                        claim,
                        PalwFraudFilerSentV1::Release,
                        current_daa,
                        court_pending,
                        court_due,
                        || {
                            let mut signed = true;
                            let object = kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_released_object_v1(
                                network_domain,
                                claim,
                                bond_key,
                                |m| {
                                    self.sign(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(|| {
                                        signed = false;
                                        Vec::new()
                                    })
                                },
                            );
                            if signed { Ok(object) } else { Err("no signing key for reservation release".into()) }
                        },
                    );
                }
                continue;
            }
            let mut header_bundle_dispatched = false;
            if params.palw_staged_public_court_v1_active_at(current_daa) && facts.reserved {
                match session.palw_staged_owned_progress_v1((claim, bond_key)) {
                    Ok(Some(progress)) if progress.bundle.is_some() => {
                        header_bundle_dispatched = true;
                        book.cases.get_mut(&claim).expect("pursued").staged_bundle_pending = true;
                        let context = book.cases.get(&claim).and_then(|case| {
                            case.binding.as_ref().zip(case.public_fp_job_v1()).map(|(binding, job)| (Some(binding.clone()), job.clone()))
                        });
                        // A TIR claim has no legacy binding: its context is the natively derived view
                        // (behind `palw_staged_tir_route_v1`; `None` below that fence or for a legacy claim).
                        let tir = match (target.as_ref(), context.is_none()) {
                            (Some(target), true) => session
                                .palw_staged_tir_view_v1(claim, self.class_step_ladder(target.class_id))
                                .ok()
                                .flatten()
                                .map(|(view, header)| FilerTirContextV1 { view, header }),
                            _ => None,
                        };
                        let context = match (context, &tir) {
                            (Some(context), _) => Some(context),
                            (None, Some(_)) => session.palw_staged_original_commitment_v1(claim).ok().flatten().map(|o| (None, o.job)),
                            (None, None) => None,
                        };
                        if let (Some(target), Some((binding, job))) = (target.as_ref(), context) {
                            let maximum = self
                                .config
                                .court
                                .max_close_bytes()
                                .min(kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
                            let file_by = progress.bundle.as_ref().map(|bundle| bundle.plan.terms.collect_until_daa.saturating_sub(2));
                            match self
                                .fraud_filer_bundle_stage_v1(
                                    session,
                                    book,
                                    claim,
                                    target,
                                    binding.as_ref(),
                                    &job,
                                    tir.as_ref(),
                                    progress,
                                    maximum,
                                    file_by,
                                    court_pending,
                                    court_due,
                                )
                                .await
                            {
                                Ok((Some(filing), _)) => {
                                    false_valid.note_executor_refuted_v1(
                                        crate::palw_filer_false_valid::palw_false_valid_armed_v1(
                                            params,
                                            self.config.fee_outpoint.is_some(),
                                            current_daa,
                                        ),
                                        &filing.object,
                                        Some(view.as_ref().map_or(book.cases[&claim].candidate.accepted_daa, |v| v.accepted_daa)),
                                        current_daa,
                                    );
                                    let mut door = self.conviction_door_v1(session, reporter, bond_key, network_domain);
                                    let case = book.cases.get_mut(&claim).expect("pursued");
                                    if !door.holds(&filing.offence_key)
                                        && case.direct_handed < super::reporter_filer::PALW_FILER_HAND_OFFS_PER_OFFENCE_V1
                                        && case
                                            .direct_asked_at
                                            .is_none_or(|at| current_daa.saturating_sub(at) >= PALW_FRAUD_FILER_RESEND_DAA_V1)
                                    {
                                        case.direct_asked_at = Some(current_daa);
                                        if matches!(
                                            door.file(filing),
                                            PalwFileOutcomeV1::Queued { .. } | PalwFileOutcomeV1::AlreadyFiled
                                        ) {
                                            case.direct_handed += 1;
                                        }
                                    }
                                    continue;
                                }
                                Ok((None, true)) => {
                                    book.cases.get_mut(&claim).expect("pursued").finish_tir_semantics_v1();
                                }
                                Ok(_) => {}
                                Err(why) => debug!("[{PALW_PANEL}] claim {claim}: original-header bundle remains pending: {why}"),
                            }
                        }
                    }
                    Err(why) => {
                        header_bundle_dispatched = true;
                        book.cases.get_mut(&claim).expect("pursued").staged_bundle_pending = true;
                        debug!("[{PALW_PANEL}] claim {claim}: required-family snapshot unavailable: {why}");
                    }
                    _ => {}
                }
            }
            // A TIR case (behind `palw_staged_tir_route_v1`): the engine's prelude decides the outcome and the
            // reservation; everything after the reservation is the staged bundle's, dispatched above. It never
            // enters the legacy binding read, probes or descent.
            if book.cases.get(&claim).is_some_and(|case| case.tir.is_some()) {
                if book.cases[&claim].release_pending {
                    continue;
                }
                let mismatch = matches!(
                    book.cases[&claim].verdict,
                    PalwFraudFilerVerdictV1::Mismatch(_)
                        | PalwFraudFilerVerdictV1::DeferredMismatch
                        | PalwFraudFilerVerdictV1::SemanticPending
                );
                match kaspa_consensus_core::palw_legacy_public_filer_v1::palw_fraud_filer_next_v1(
                    role,
                    &kaspa_consensus_core::palw_legacy_public_filer_v1::PalwFilerClaimFactsV1 { binding_known: true, ..facts.clone() },
                    mismatch,
                    &PalwLegacyBisectV1::new(0),
                    |_| false,
                ) {
                    PalwFilerActionV1::Done(phase) if facts.outcome.is_some() || facts.ended || !mismatch => {
                        book.settle(&claim, PalwFraudFilerVerdictV1::Settled(phase));
                    }
                    PalwFilerActionV1::Reserve => {
                        if let Some(view) = view.as_ref() {
                            self.fraud_filer_queue_v1(
                                book,
                                claim,
                                PalwFraudFilerSentV1::Reserve,
                                current_daa,
                                court_pending,
                                court_due,
                                || {
                                    let mut signed = true;
                                    let object = palw_dispute_reserved_object_v1(
                                        network_domain,
                                        palw_fraud_filer_reservation_v1(view, bond_key),
                                        |m| {
                                            self.sign(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(|| {
                                                signed = false;
                                                Vec::new()
                                            })
                                        },
                                    );
                                    if signed { Ok(object) } else { Err("no signing key for this bond".to_string()) }
                                },
                            );
                        }
                    }
                    _ => {}
                }
                continue;
            }
            // Even a disclosure the session arm refused can carry a complete cryptographic proof of a wrong job.
            // Judge the binding against the recorded target BEFORE a mismatched count can stop subtree descent.
            // The reporter door rehearses the actual gate; an open colluder court cannot veto a valid direct proof.
            if params.palw_offence_attribution_active_at(current_daa)
                && let (Some(target), Some(answer), kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle)) = (
                    target.as_ref(),
                    book.answers.get(&(claim, PalwDaUnitV1::Event { row: 0, tile: 0 })).cloned(),
                    &params.palw_consensus_mode,
                )
            {
                let answer = &answer;
                let rules = kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 {
                    prompt_ids_form: params.palw_prompt_ids_form_at(current_daa),
                    base_class_id: bundle.base_class_id,
                    da_signer_liability: params.palw_rcore_plus_active_at(current_daa),
                };
                let file_by = view.as_ref().map(|v| v.hard_deadline_daa.saturating_sub(2));
                let maximum = match session.palw_terminal_progress_v1((claim, bond_key)) {
                    Ok(Some(progress)) => {
                        progress.court_maximum.min(kaspa_consensus_core::palw_terminal_carriage_v1::PALW_TERMINAL_MAX_BYTES_V1)
                    }
                    Ok(None) => kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES,
                    Err(why) => {
                        debug!("[{PALW_PANEL}] claim {claim}: public terminal budget is unavailable: {why}");
                        continue;
                    }
                };
                let fp_job = book.cases.get(&claim).and_then(|c| c.public_fp_job_v1().cloned());
                let mut staged_filing = None;
                let mut staged_pending =
                    header_bundle_dispatched && book.cases.get(&claim).is_some_and(|case| case.staged_bundle_pending);
                let mut public_prefix_stage = false;
                let mut public_bundle_stage = header_bundle_dispatched;
                let bundle_progress =
                    if params.palw_staged_public_court_v1_active_at(current_daa) && facts.reserved && fp_job.is_some() {
                        Some(session.palw_staged_owned_progress_v1((claim, bond_key)))
                    } else {
                        None
                    };
                if let Some(Err(why)) = &bundle_progress {
                    book.cases.get_mut(&claim).expect("pursued").staged_bundle_pending = true;
                    public_bundle_stage = true;
                    staged_pending = true;
                    public_prefix_stage = fp_job.as_ref().is_some_and(|job| job.is_prefix_state());
                    debug!("[{PALW_PANEL}] claim {claim}: unknown accepted required-family progress remains pending: {why}");
                }
                if matches!(&bundle_progress,Some(Ok(Some(progress))) if progress.bundle.is_none()) {
                    book.cases.get_mut(&claim).expect("pursued").staged_bundle_pending = false;
                }
                if params.palw_staged_public_court_v1_active_at(current_daa)
                    && !header_bundle_dispatched
                    && facts.reserved
                    && let (Some(job), PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(initial))) = (fp_job.as_ref(), answer)
                    && let Some(Ok(Some(progress))) = bundle_progress
                    && progress.bundle.is_some()
                {
                    public_bundle_stage = true;
                    public_prefix_stage = job.is_prefix_state();
                    staged_pending = true;
                    match self
                        .fraud_filer_bundle_stage_v1(
                            session,
                            book,
                            claim,
                            target,
                            Some(initial.binding()),
                            job,
                            None,
                            progress,
                            maximum,
                            file_by,
                            court_pending,
                            court_due,
                        )
                        .await
                    {
                        Ok((filing, complete)) => {
                            staged_filing = filing;
                            staged_pending = !complete;
                        }
                        Err(why) => debug!("[{PALW_PANEL}] claim {claim}: required-family bundle remains pending: {why}"),
                    }
                }
                if !public_bundle_stage
                    && params.palw_staged_public_court_v1_active_at(current_daa)
                    && facts.reserved
                    && let (Some(job), PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(initial))) =
                        (fp_job.as_ref().filter(|job| job.is_prefix_state()), answer)
                {
                    match session.palw_staged_owned_progress_v1((claim, bond_key)) {
                        Ok(Some(progress)) => {
                            public_prefix_stage = true;
                            staged_pending = true;
                            match self
                                .fraud_filer_prefix_stage_v1(
                                    session,
                                    book,
                                    claim,
                                    target,
                                    initial.binding(),
                                    job,
                                    progress,
                                    maximum,
                                    file_by,
                                    court_pending,
                                    court_due,
                                )
                                .await
                            {
                                Ok((filing, done)) => {
                                    staged_filing = filing;
                                    if done {
                                        book.cases.get_mut(&claim).expect("pursued").prefix_complete = true;
                                        staged_pending = false;
                                    }
                                }
                                Err(why) => debug!("[{PALW_PANEL}] claim {claim}: staged public prefix work remains pending: {why}"),
                            }
                        }
                        Ok(None) => {}
                        Err(why) => {
                            public_prefix_stage = true;
                            staged_pending = true;
                            debug!("[{PALW_PANEL}] claim {claim}: staged public prefix snapshot unavailable: {why}");
                        }
                    }
                }
                if !public_prefix_stage
                    && fp_job.as_ref().is_some_and(|job| job.is_prefix_state())
                    && book.cases.get(&claim).is_some_and(|case| case._prefix_reservation.is_none() || case.prefix_budget < maximum)
                {
                    // Three learning/proof copies, Vec growth and the terminal builder remain
                    // within this conservative envelope; all bytes stay under the shared ledger.
                    let ticket = crate::palw_memory_ledger::host_ledger_v1().reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-prefix-proof",
                            class_id: target.class_id,
                            job: claim,
                        },
                        maximum.saturating_mul(8),
                    );
                    let Ok(ticket) = ticket else { continue };
                    let case = book.cases.get_mut(&claim).expect("pursued case");
                    case._prefix_reservation = Some(Arc::new(ticket));
                    case.prefix_budget = maximum;
                }
                book.cases.get_mut(&claim).expect("pursued").prefix_stage_pending = public_prefix_stage && staged_pending;
                if !public_bundle_stage
                    && params.palw_staged_public_court_v1_active_at(current_daa)
                    && facts.reserved
                    && !book.cases.get(&claim).is_some_and(|case| case.rendering_done)
                    && target.registered_semantics.as_ref().is_some_and(|r| {
                        r.rendering_rule.is_some()
                            && r.tokenizer_key_events.is_some()
                            && r.raw_tokenizer_bytes.is_some()
                            && r.token_table_root.is_some()
                    })
                    && let (Some(job), PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(initial))) = (fp_job.as_ref(), answer)
                {
                    match session.palw_staged_owned_progress_v1((claim, bond_key)) {
                        Ok(Some(progress)) => {
                            // A qualified prospective contract keeps rendering pending even if
                            // this host cannot currently fund/read its own operand. Local refusal
                            // is neither a producer DA accusation nor a semantic pass.
                            staged_pending = true;
                            match self
                                .fraud_filer_tokenizer_stage_v1(
                                    session,
                                    book,
                                    claim,
                                    target,
                                    initial.binding(),
                                    job,
                                    progress,
                                    maximum,
                                    file_by,
                                    current_daa,
                                    court_pending,
                                    court_due,
                                )
                                .await
                            {
                                Ok((filing, done)) => {
                                    staged_filing = filing;
                                    if done {
                                        book.cases.get_mut(&claim).expect("pursued").rendering_done = true;
                                        staged_pending = false;
                                    }
                                }
                                Err(why) => debug!("[{PALW_PANEL}] claim {claim}: staged own-tokenizer work remains pending: {why}"),
                            }
                        }
                        Ok(None) => {}
                        Err(why) => {
                            staged_pending = true;
                            debug!("[{PALW_PANEL}] claim {claim}: staged snapshot unavailable: {why}");
                        }
                    }
                }
                // The longest canonical prompt needs a whole-root recompute. Reserve its temporary ids/tree and proof buffers,
                // then do all direct-proof hashing off the service loop. Pressure delays this pursuit rather than settling it.
                let prefill = match answer {
                    PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event)) => {
                        kaspa_consensus_core::palw_offence_attribution_v1::palw_prompt_not_anchored_admit_v1(
                            target,
                            event.binding(),
                            rules,
                        )
                        .ok()
                        .or_else(|| {
                            fp_job.as_ref().and_then(|job| {
                                kaspa_consensus_core::palw_offence_attribution_v1::palw_fp_canonical_prompt_admit_v1(
                                    target,
                                    event.binding(),
                                    job,
                                    rules,
                                )
                                .ok()
                                .filter(|n| *n == event.binding().job_context.declared_prefill_tokens)
                            })
                        })
                        .map(u64::from)
                        .unwrap_or(0)
                        .min(kaspa_consensus_core::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1)
                    }
                    _ => 0,
                };
                let tiles = prefill.div_ceil(u64::from(kaspa_consensus_core::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN));
                let bytes = prefill
                    .saturating_mul(8)
                    .saturating_add(tiles.saturating_mul(256))
                    .saturating_add(2 * kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
                let bytes = bytes.saturating_add(if fp_job.as_ref().is_some_and(|job| job.is_constraint()) {
                    // Leaf hashes plus cached Merkle levels, built once for all opened IDs.
                    match answer {
                        PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event)) => {
                            u64::from(event.binding().shape_profile.vocab_size).saturating_add(1).saturating_mul(256)
                        }
                        _ => 0,
                    }
                } else {
                    0
                });
                let reserved = match crate::palw_memory_ledger::host_ledger_v1().reserve(
                    crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                        role: "public-binding-proof",
                        class_id: target.class_id,
                        job: claim,
                    },
                    bytes,
                ) {
                    Ok(reserved) => reserved,
                    Err(why) => {
                        debug!("[{PALW_PANEL}] claim {claim}: the public direct proof waits for memory: {why}");
                        continue;
                    }
                };
                let case = book.cases.get(&claim).expect("pursued above");
                let previous_plan = case.constraint_plan;
                let constraint_done = case.constraint_done;
                let previous_prefix = case.prefix_plan.clone();
                let prefix_complete = case.prefix_complete;
                let previous_rendering_done = case.rendering_done;
                let tokenizer_backends = self.backends();
                let tokenizer_session = session.clone();
                let tokenizer_chain_classes = self.config.chain_classes;
                let own_ids = match &case.verdict {
                    PalwFraudFilerVerdictV1::Mismatch(run) => run.own_output_ids.clone(),
                    _ => Vec::new(),
                };
                let selection_replayed = matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if run.fp_job.is_some());
                let arithmetic_matched = case.arithmetic_matched;
                let events: BTreeMap<PalwDaUnitV1, _> = book
                    .answers
                    .iter()
                    .filter_map(|((held, unit), answer)| {
                        if *held != claim {
                            return None;
                        }
                        let PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event)) = answer else { return None };
                        Some((*unit, event.clone()))
                    })
                    .collect();
                let target = target.clone();
                let answer = answer.clone();
                let proof = tokio::task::spawn_blocking(move || {
                    let _reserved = reserved;
                    let mut prefix_plan = previous_prefix;
                    let mut prefix_complete = prefix_complete;
                    let mut prefix_direct_checked = false;
                    let evaluate_prefix = || -> Result<_, String> {
                        let Some(job) = fp_job.as_ref().filter(|job| job.is_prefix_state()) else { return Ok(None) };
                        let direct =
                            palw_fraud_filer_public_filing_with_budget_v3(&target, rules, &answer, file_by, fp_job.as_ref(), maximum)?;
                        prefix_direct_checked = true;
                        if direct.is_some() {
                            return Ok(direct);
                        }
                        if public_prefix_stage {
                            prefix_plan = None;
                            return Ok(None);
                        }
                        let PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(initial)) = &answer else { return Ok(None) };
                        if prefix_plan.is_none() {
                            prefix_plan = prefix::PrefixPlanV1::for_job(&target, initial.binding(), job, maximum)?;
                        }
                        let Some(plan) = prefix_plan.as_mut() else { return Ok(None) };
                        plan.increase_budget(maximum)?;
                        if plan.next_unit(initial.binding())?.is_some() {
                            return Ok(None);
                        }
                        let contradiction = plan.contradiction(&target, initial.binding(), job)?;
                        prefix_complete = true;
                        contradiction
                            .map(|contradiction| {
                                let built =
                                    kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                                        target.claim_id,
                                        target.executor_bond,
                                        contradiction,
                                        None,
                                        maximum,
                                    )
                                    .map_err(|e| e.to_string())?;
                                PalwConvictionFilingV1::of_offence(built.object, target.claim_id, file_by, PalwFilingOriginV1::Replay)
                                    .filter(|filing| (filing.offence_key, filing.evidence_id) == (built.offence_id, built.evidence_id))
                                    .ok_or_else(|| "the prefix reporter key differs from its proof builder".to_string())
                            })
                            .transpose()
                    };
                    let mut evaluate_prefix = evaluate_prefix;
                    let prefix_filing = evaluate_prefix()?;
                    // The outside verifier owns this D7 model/tokenizer material. Neither the
                    // producer nor an injected verdict supplies this path or its authenticated bytes.
                    let mut tokenizer_check = None;
                    let mut rendering_done = previous_rendering_done;
                    if !rendering_done
                        && !staged_pending
                        && staged_filing.is_none()
                        && target.registered_semantics.as_ref().is_some_and(|row| row.rendering_rule.is_some())
                        && let Some(job) = fp_job.as_ref()
                        && let PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(initial)) = &answer
                    {
                        let check = (|| {
                            let path = tokenizer_backends
                                .held_tokenizer_path_or_chain_v1(target.class_id, target.artifact_root, |id| {
                                    if tokenizer_chain_classes {
                                        tokenizer_session.palw_registered_class_carriage_v1(id)
                                    } else {
                                        None
                                    }
                                })
                                .ok_or("the outside verifier does not hold the registered tokenizer file")?;
                            let table = tokenizer_scheduler::registered_table_v1(&target, initial.binding(), job)?;
                            tokenizer::check_held_tokenizer_v1(&path, &target, initial.binding(), job, &table, maximum as usize)
                        })();
                        match check {
                            Ok(check) => {
                                rendering_done = check.fault.is_none();
                                tokenizer_check = Some(check);
                            }
                            Err(why) => debug!(
                                "[{PALW_PANEL}] claim {}: registered tokenizer verification remains pending: {why}",
                                target.claim_id
                            ),
                        }
                    }
                    let tokenizer_filing = tokenizer_check
                        .as_mut()
                        .and_then(|check| check.fault.take())
                        .map(|contradiction| {
                            let built =
                                kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                                    target.claim_id,
                                    target.executor_bond,
                                    contradiction,
                                    None,
                                    maximum,
                                )
                                .map_err(|e| e.to_string())?;
                            PalwConvictionFilingV1::of_offence(built.object, target.claim_id, file_by, PalwFilingOriginV1::Replay)
                                .filter(|filing| (filing.offence_key, filing.evidence_id) == (built.offence_id, built.evidence_id))
                                .ok_or_else(|| "the tokenizer reporter key differs from its proof builder".to_string())
                        })
                        .transpose()?
                        .or(staged_filing);
                    let semantic_ready = palw_fraud_filer_registered_preflight_v1(
                        &target,
                        rules,
                        &answer,
                        fp_job.as_ref(),
                        prefix_complete,
                        rendering_done,
                    )
                    .unwrap_or(false);
                    let evaluate = || {
                        if tokenizer_filing.is_some() {
                            return Ok((tokenizer_filing, previous_plan, constraint_done));
                        }
                        if prefix_filing.is_some() {
                            return Ok((prefix_filing, previous_plan, constraint_done));
                        }
                        let direct = if prefix_direct_checked
                            || (public_bundle_stage && fp_job.as_ref().is_some_and(|job| job.is_constraint()))
                        {
                            None
                        } else {
                            palw_fraud_filer_public_filing_with_budget_v3(&target, rules, &answer, file_by, fp_job.as_ref(), maximum)?
                        };
                        if direct.is_some() || constraint_done {
                            return Ok((direct, previous_plan, constraint_done));
                        }
                        let Some(job) = fp_job.filter(|job| job.is_constraint()) else { return Ok((None, None, false)) };
                        let PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(initial)) = &answer else {
                            return Ok((None, None, false));
                        };
                        if !matches!(initial, kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Tiled { .. }) {
                            return Ok((
                                None,
                                None,
                                matches!(initial, kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Flat { .. }),
                            ));
                        }
                        // The public bundle stage streams the original table in bounded parts (a
                        // file-backed holding may exceed any resident table); the legacy direct
                        // proof keeps its in-memory table and its own evidence budget.
                        let table = if public_bundle_stage {
                            None
                        } else {
                            Some(
                                kaspa_consensus_core::palw_fp_constraint_job_v1::palw_token_table_for_job_v1(&job)
                                    .ok_or("the public verifier does not hold the job's tokenizer table")?,
                            )
                        };
                        let plan = match previous_plan {
                            Some(plan) => Some(plan),
                            None if public_bundle_stage => {
                                let root = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(&job)
                                    .ok_or("constrained job has no immutable table root")?;
                                constraint_scheduler::nominate_v1(
                                    &target,
                                    &job,
                                    initial,
                                    &own_ids,
                                    crate::palw_public_token_table::public_table_v1(&root, &job.tokenizer_id)
                                        .ok_or("the public verifier does not hold the job's original public table")?,
                                    &crate::palw_memory_ledger::host_ledger_v1(),
                                )?
                            }
                            None => constraint::tiled_plan_v1(
                                &target,
                                &job,
                                initial,
                                &own_ids,
                                table.clone().ok_or("legacy direct proof lost its resident table")?,
                            )?,
                        };
                        let plan = plan.or_else(|| {
                            if public_bundle_stage
                                && (selection_replayed || arithmetic_matched)
                                && let kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Tiled {
                                    generated_token_ids,
                                    ..
                                } = initial
                            {
                                generated_token_ids.first().copied().map(|committed| constraint::TiledPlanV1 {
                                    position: 0,
                                    committed,
                                    witness: None,
                                    beat_lane: None,
                                })
                            } else {
                                None
                            }
                        });
                        let Some(plan) = plan else { return Ok((None, None, selection_replayed)) };
                        if public_bundle_stage {
                            return Ok((None, Some(plan), false));
                        }
                        let selected: Option<Vec<_>> = plan.units().iter().map(|unit| events.get(unit).cloned()).collect();
                        let Some(selected) = selected else { return Ok((None, Some(plan), false)) };
                        let table = table.ok_or("legacy direct proof lost its resident table")?;
                        let contradiction =
                            constraint::tiled_contradiction_with_budget_v2(&target, &job, plan, &selected, table, maximum)?;
                        let filing = contradiction
                            .map(|contradiction| {
                                let built =
                                    kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                                        target.claim_id,
                                        target.executor_bond,
                                        contradiction,
                                        None,
                                        maximum,
                                    )
                                    .map_err(|e| e.to_string())?;
                                PalwConvictionFilingV1::of_offence(built.object, target.claim_id, file_by, PalwFilingOriginV1::Replay)
                                    .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                                    .ok_or_else(|| "the reporter key differs from the public proof builder".to_string())
                            })
                            .transpose()?;
                        // An own-replay nominee whose PUBLIC row does not prove a selector lie
                        // yields to arithmetic localization, never to an unproved conviction.
                        Ok((filing, Some(plan), true))
                    };
                    evaluate().map(|result| (result, semantic_ready, prefix_plan, prefix_complete, rendering_done, tokenizer_check))
                })
                .await
                .unwrap_or_else(|e| Err(format!("the public proof worker failed: {e}")));
                match proof {
                    Ok(((Some(filing), _, _), _, prefix_plan, prefix_complete, rendering_done, _tokenizer_check)) => {
                        let case = book.cases.get_mut(&claim).expect("pursued case");
                        case.prefix_plan = prefix_plan;
                        case.prefix_complete = prefix_complete;
                        case.rendering_done = rendering_done;
                        // Keep the tokenizer scratch/evidence ticket alive until the reporter
                        // has copied the normal filing into its independently budgeted queue.
                        // The public proof refutes the liable Valid signers as well as the
                        // executor. Keep it before settlement removes this case from pursuit;
                        // the existing false-Valid scanner independently checks every receipt.
                        false_valid.note_executor_refuted_v1(
                            crate::palw_filer_false_valid::palw_false_valid_armed_v1(
                                params,
                                self.config.fee_outpoint.is_some(),
                                current_daa,
                            ),
                            &filing.object,
                            view.as_ref().map(|v| v.accepted_daa),
                            current_daa,
                        );
                        // This proof supersedes any unsent localization carrier. Keep the chain's live obligations until conviction.
                        court_pending
                            .retain(|(held, round, responder, _)| *held != claim || !palw_fraud_filer_queued_v1(*round, *responder));
                        let mut door = self.conviction_door_v1(session, reporter, bond_key, network_domain);
                        let case = book.cases.get_mut(&claim).expect("pursued above");
                        if !door.holds(&filing.offence_key)
                            && case.direct_handed < super::reporter_filer::PALW_FILER_HAND_OFFS_PER_OFFENCE_V1
                            && case.direct_asked_at.is_none_or(|at| current_daa.saturating_sub(at) >= PALW_FRAUD_FILER_RESEND_DAA_V1)
                        {
                            case.direct_asked_at = Some(current_daa);
                            match door.file(filing) {
                                PalwFileOutcomeV1::Queued { .. } | PalwFileOutcomeV1::AlreadyFiled => case.direct_handed += 1,
                                PalwFileOutcomeV1::AlreadyConvicted => {}
                                why => {
                                    warn!("[{PALW_PANEL}] claim {claim}: the public direct proof waits at the reporter door: {why:?}")
                                }
                            }
                        }
                        continue;
                    }
                    Ok(((None, plan, complete), semantic_ready, prefix_plan, prefix_complete, rendering_done, _tokenizer_check)) => {
                        let case = book.cases.get_mut(&claim).expect("pursued case");
                        case.prefix_plan = prefix_plan;
                        case.prefix_complete = prefix_complete;
                        case.rendering_done = rendering_done;
                        if case.may_finish_semantics_v1(semantic_ready) {
                            let case = book.cases.get_mut(&claim).expect("pursued above");
                            case.verdict = PalwFraudFilerVerdictV1::Honest;
                            case.deferred_public = None;
                            case.prefix_plan = None;
                            case._prefix_reservation = None;
                            case.prefix_budget = 0;
                            case.fp_job_pending = false;
                            case.release_pending = true;
                            #[cfg(test)]
                            self.fraud_filer_honest_replays.lock().unwrap().insert(claim);
                            // The next tick relinquishes the on-chain reservation before dropping pursuit.
                            continue;
                        }
                        let case = book.cases.get_mut(&claim).expect("pursued above");
                        if case.constraint_plan != plan {
                            // Selected tiles may already be in older authenticated history.
                            case.constraint_plan = plan;
                            book.walked_from = None;
                            book.walked_to = None;
                            book.walk = None;
                            book.read_at = None;
                        }
                        let case = book.cases.get_mut(&claim).expect("held above");
                        case.constraint_done = complete && !case.staged_bundle_pending;
                    }
                    Err(why) => debug!("[{PALW_PANEL}] claim {claim}: this public event yields no direct proof: {why}"),
                }
            }
            // A queued item can have landed on another branch/read, or another public party may have answered its unit.
            // Query the chain before a pending carrier suppresses progression; discard duplicates before paying to resend them.
            let lanes_on = params.palw_legacy_held_da_v2_active_at(current_daa);
            let lanes = lanes_on.then_some(PalwFraudFilerLanesV1 { now_daa: current_daa, form });
            court_pending.retain(|(held, round, responder, object)| {
                if *held != claim || !palw_fraud_filer_queued_v1(*round, *responder) { return true; }
                // Staged owner moves are the staged queue's own: it retires them against the
                // accepted cursor, never against the legacy session/answer facts below.
                if *round == PALW_FRAUD_FILER_STAGED_ROUND_V1 { return true; }
                if *round == PALW_FRAUD_FILER_RESERVE_ROUND_V1 { return !facts.reserved; }
                if *round == PALW_FRAUD_FILER_TERMINAL_ROUND_V2 { return !own_court_open; }
                // Lane G14-L: a queued lane stays until it lands in this bond's session or its unit is answered.
                if lanes_on && let PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand } = object {
                    let unit = PalwDaUnitV1::LegacyHeldV2(demand.unit);
                    return !view.as_ref().is_some_and(|v| {
                        v.unit_was_answered_v1(&unit)
                            || v.sessions.iter().any(|(bond, session)| *bond == bond_key && session.units.contains(&unit))
                    });
                }
                if facts.session_open { return false; }
                if let Some((PalwFraudFilerSentV1::Reacquire { session_number, .. }, _, _)) = book.cases.get(&claim).and_then(|c| c.sent) {
                    return view.as_ref().and_then(|v| v.record.as_ref()).and_then(|r| r.live.get(&bond_key))
                        .is_some_and(|row| row.sessions_opened < session_number);
                }
                !book.cases.get(&claim).is_some_and(|case| matches!(case.sent,
                    Some((PalwFraudFilerSentV1::Demand(probe), _, _)) if view.as_ref().is_some_and(|v| v.unit_was_answered_v1(&probe.unit()))))
            });
            if court_pending.iter().any(|(held, round, responder, _)| *held == claim && palw_fraud_filer_queued_v1(*round, *responder))
            {
                continue;
            }
            // Lane G14-L: a k-ary round's lanes are read one a step; give a round of 64 its reads in one tick.
            let steps = if lanes_on {
                4 + kaspa_consensus_core::palw_legacy_parallel_v1::PALW_LEGACY_LANES_PER_SESSION_V1 as usize
            } else {
                4
            };
            for _ in 0..steps {
                let Some(case) = book.cases.get(&claim) else { break };
                let legacy = params.palw_legacy_held_da_v2_active_at(current_daa)
                    && matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if run.legacy.is_some());
                let step = if case.fp_bootstrap.is_some()
                    || case.fp_job_pending
                    || matches!(case.verdict, PalwFraudFilerVerdictV1::DeferredMismatch)
                    || (case.prefix_plan.is_some() && !case.prefix_complete)
                {
                    palw_fraud_filer_bootstrap_step_v2(role, view.as_ref(), &bond_key, reservable, case, lanes, |unit| {
                        book.answers.contains_key(&(claim, *unit))
                    })
                } else if let Some(step) = palw_fraud_filer_constraint_step_v1(view.as_ref(), &bond_key, reservable, case, |unit| {
                    book.answers.contains_key(&(claim, *unit))
                }) {
                    step
                } else if case.constraint_preflight_pending_v1() || case.prefix_stage_pending {
                    // An arithmetic DA session would block the selector's required public rows.
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait)
                } else if legacy {
                    let owned = case.clone();
                    let view = view.clone();
                    let read: BTreeSet<PalwDaUnitV1> =
                        book.answers.keys().filter(|(held, _)| *held == claim).map(|(_, unit)| *unit).collect();
                    match tokio::task::spawn_blocking(move || {
                        palw_fraud_filer_legacy_step_v3(
                            role,
                            view.as_ref(),
                            &bond_key,
                            reservable,
                            own_court_open,
                            &owned,
                            lanes,
                            |unit| read.contains(unit),
                        )
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("the public descent task did not finish: {e}")))
                    {
                        Ok(step) => step,
                        Err(why) => {
                            warn!("[{PALW_PANEL}] claim {claim}: the public legacy descent cannot proceed: {why}");
                            if !book.descent_failed(&claim, why) {
                                debug!("[{PALW_PANEL}] claim {claim}: waiting for the remaining public proof history before settling");
                            }
                            break;
                        }
                    }
                } else {
                    // G14-V: the codec-independent authenticated subtree descent (its first call builds the own tree: blocking).
                    let owned = case.clone();
                    let view = view.clone();
                    let read: BTreeSet<PalwDaUnitV1> =
                        book.answers.keys().filter(|(held, _)| *held == claim).map(|(_, unit)| *unit).collect();
                    match tokio::task::spawn_blocking(move || {
                        palw_fraud_filer_subtree_step_v1(role, view.as_ref(), &bond_key, reservable, &owned, |unit| {
                            read.contains(unit)
                        })
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("the public subtree descent task did not finish: {e}")))
                    {
                        Ok(step) => step,
                        Err(why) => {
                            warn!("[{PALW_PANEL}] claim {claim}: the public subtree descent cannot proceed: {why}");
                            if !book.descent_failed(&claim, why) {
                                debug!("[{PALW_PANEL}] claim {claim}: waiting for the remaining public proof history before settling");
                            }
                            break;
                        }
                    }
                };
                match step {
                    PalwFraudFilerStepV1::Learn(probe) => {
                        let answer = book.answers.get(&(claim, probe.unit())).cloned().expect("read above");
                        let mut owned = case.clone();
                        let learned = tokio::task::spawn_blocking(move || owned.learn(probe, &answer).map(|()| owned))
                            .await
                            .unwrap_or_else(|e| Err(format!("the read did not finish: {e}")));
                        match learned {
                            Ok(owned) => {
                                let case = book.cases.get_mut(&claim).expect("held above");
                                case.accept_learned_v1(owned);
                                // A learned node, range or trace event lives on in the case's authenticated tree; the bytes go.
                                if matches!(probe, PalwLegacyProbeV1::HeldNode { .. } | PalwLegacyProbeV1::Range { .. })
                                    || matches!(probe, PalwLegacyProbeV1::Binding { row, tile } if (row, tile) != (0, 0))
                                {
                                    book.answers.remove(&(claim, probe.unit()));
                                }
                                debug!("[{PALW_PANEL}] claim {claim}: read the authenticated chain answer to {probe:?}");
                            }
                            Err(why) => {
                                warn!("[{PALW_PANEL}] claim {claim}: the chain's answer to {probe:?} does not read: {why} (LG14-A)");
                                if book.cases.get(&claim).is_some_and(|case| case.prefix_plan.is_some()) {
                                    // An insufficient finite evidence budget is an explicit pending
                                    // scope refusal, never an Honest verdict or reservation release.
                                    break;
                                }
                                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
                                break;
                            }
                        }
                    }
                    PalwFraudFilerStepV1::AwaitAnswer(probe) => {
                        match probe {
                            PalwLegacyProbeV1::HeldNode { unit } => book.want_legacy(claim, unit),
                            PalwLegacyProbeV1::Range { .. } => book.want_rcore(claim, probe.unit()),
                            PalwLegacyProbeV1::Binding { row, tile } if (row, tile) != (0, 0) => book.want_rcore(claim, probe.unit()),
                            _ => {}
                        }
                        if let Some(view) = view.as_ref() {
                            self.fraud_filer_reacquire_v1(
                                book,
                                claim,
                                view,
                                bond_key,
                                probe,
                                current_daa,
                                network_domain,
                                court_pending,
                                court_due,
                            );
                        }
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait) => break,
                    PalwFraudFilerStepV1::AwaitRound(round) => {
                        book.want_legacy_round(claim, round);
                        if let Some(case) = book.cases.get_mut(&claim)
                            && let Err(why) = case.reserve_legacy_material_v1(
                                &crate::palw_memory_ledger::host_ledger_v1(),
                                self.config.court.max_close_bytes(),
                                form,
                            )
                        {
                            debug!("[{PALW_PANEL}] claim {claim}: {why}; the public round remains Pending");
                            break;
                        }
                        if let Some(view) = view.as_ref()
                            && let Some(probe) = book
                                .cases
                                .get(&claim)
                                .and_then(|case| case.missing_legacy_round_v1(view, |unit| book.answers.contains_key(&(claim, *unit))))
                        {
                            self.fraud_filer_reacquire_v1(
                                book,
                                claim,
                                view,
                                bond_key,
                                probe,
                                current_daa,
                                network_domain,
                                court_pending,
                                court_due,
                            );
                        }
                        break;
                    }
                    PalwFraudFilerStepV1::DemandLanes { round, demand } => {
                        book.want_legacy_round(claim, round);
                        if let Some(case) = book.cases.get_mut(&claim)
                            && let Err(why) = case.reserve_legacy_material_v1(
                                &crate::palw_memory_ledger::host_ledger_v1(),
                                self.config.court.max_close_bytes(),
                                form,
                            )
                        {
                            debug!("[{PALW_PANEL}] claim {claim}: {why}; no new lane is queued");
                            break;
                        }
                        let Some(view) = view.as_ref() else { break };
                        self.fraud_filer_queue_lanes_v1(
                            book,
                            claim,
                            &demand,
                            current_daa,
                            court_pending,
                            court_due,
                            |probe, binding| {
                                palw_fraud_filer_demand_object_v1(
                                    &network_domain,
                                    claim,
                                    &view.execution_root,
                                    probe,
                                    binding,
                                    bond_key,
                                    form,
                                    |message, context| self.sign(message, context),
                                )
                            },
                        );
                        break;
                    }
                    PalwFraudFilerStepV1::PastStepTree => {
                        // G14-V: the step tree is the verifier's own and the execution is not — localize the logits trace or
                        // the committed tokens from the binding read, public events and the own run; never settle as honest.
                        let Some(target) = target.clone() else {
                            debug!("[{PALW_PANEL}] claim {claim}: past the step tree, the claim's offence target is not readable yet");
                            break;
                        };
                        let fallback = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                            &self.config.court,
                            params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
                        );
                        let ladder = self.seat_refutation_ladder_v1(target.class_id, fallback, current_daa);
                        let file_by = view.as_ref().map(|v| v.hard_deadline_daa.saturating_sub(2));
                        let owned = case.clone();
                        let event0 = book.answers.get(&(claim, PalwDaUnitV1::Event { row: 0, tile: 0 })).cloned();
                        let past = tokio::task::spawn_blocking(move || {
                            palw_fraud_filer_past_step_v1(&owned, &target, event0.as_ref(), file_by, ladder)
                        })
                        .await
                        .unwrap_or_else(|e| Err(format!("the past-step localization task did not finish: {e}")));
                        match past {
                            Ok(PalwFraudFilerPastStepV1::File(filing)) => {
                                info!("[{PALW_PANEL}] claim {claim}: past an agreeing step tree, a public proof is filed (G14-V)");
                                let mut door = self.conviction_door_v1(session, reporter, bond_key, network_domain);
                                let case = book.cases.get_mut(&claim).expect("pursued above");
                                if !door.holds(&filing.offence_key)
                                    && case.direct_handed < super::reporter_filer::PALW_FILER_HAND_OFFS_PER_OFFENCE_V1
                                    && case
                                        .direct_asked_at
                                        .is_none_or(|at| current_daa.saturating_sub(at) >= PALW_FRAUD_FILER_RESEND_DAA_V1)
                                {
                                    case.direct_asked_at = Some(current_daa);
                                    match door.file(filing) {
                                        PalwFileOutcomeV1::Queued { .. } | PalwFileOutcomeV1::AlreadyFiled => case.direct_handed += 1,
                                        PalwFileOutcomeV1::AlreadyConvicted => {}
                                        why => warn!(
                                            "[{PALW_PANEL}] claim {claim}: the past-step proof waits at the reporter door: {why:?}"
                                        ),
                                    }
                                }
                                break;
                            }
                            Ok(PalwFraudFilerPastStepV1::Read(probe)) => {
                                let unit = probe.unit();
                                book.want_rcore(claim, unit);
                                let answered = view.as_ref().is_some_and(|v| v.unit_was_answered_v1(&unit));
                                if answered && let Some(answer) = book.answers.get(&(claim, unit)).cloned() {
                                    let mut owned = book.cases.get(&claim).expect("pursued above").clone();
                                    let learned = tokio::task::spawn_blocking(move || owned.learn(probe, &answer).map(|()| owned))
                                        .await
                                        .unwrap_or_else(|e| Err(format!("the read did not finish: {e}")));
                                    match learned {
                                        Ok(owned) => {
                                            book.cases.get_mut(&claim).expect("held above").accept_learned_v1(owned);
                                            book.answers.remove(&(claim, unit));
                                            continue;
                                        }
                                        Err(why) => {
                                            warn!("[{PALW_PANEL}] claim {claim}: the chain's event {probe:?} does not read: {why}");
                                            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
                                            break;
                                        }
                                    }
                                }
                                if answered {
                                    break; // The walk reads it next.
                                }
                                let Some(view) = view.as_ref() else { break };
                                let binding = book.cases.get(&claim).and_then(|c| c.binding.clone());
                                self.fraud_filer_queue_v1(
                                    book,
                                    claim,
                                    PalwFraudFilerSentV1::Demand(probe),
                                    current_daa,
                                    court_pending,
                                    court_due,
                                    || {
                                        palw_fraud_filer_demand_object_v1(
                                            &network_domain,
                                            claim,
                                            &view.execution_root,
                                            probe,
                                            binding.as_ref(),
                                            bond_key,
                                            form,
                                            |message, context| self.sign(message, context),
                                        )
                                    },
                                );
                                break;
                            }
                            Err(why) => {
                                warn!("[{PALW_PANEL}] claim {claim}: past the step tree nothing is provable yet: {why}");
                                if !book.descent_failed(&claim, why) {
                                    debug!(
                                        "[{PALW_PANEL}] claim {claim}: waiting for the remaining public proof history before settling"
                                    );
                                }
                                break;
                            }
                        }
                    }
                    PalwFraudFilerStepV1::LegacyTerminal { leaf } => {
                        let item = PalwFraudFilerSentV1::Terminal { leaf };
                        if case.may_send(item, current_daa) != Ok(true) {
                            self.fraud_filer_queue_v1(book, claim, item, current_daa, court_pending, court_due, || {
                                Err("the terminal was not delivered after its retry".into())
                            });
                            break;
                        }
                        let PalwFraudFilerVerdictV1::Mismatch(run) = &case.verdict else { break };
                        let Some(replica) = run.legacy.clone() else { break };
                        let Some(binding) = case.binding.clone() else { break };
                        let Some(artifact_root) = case.candidate.job.artifact_root else { break };
                        let bound_to = kaspa_consensus_core::palw_shard_court_v1::PalwOneMoveClaimV2 {
                            execution_root: case.candidate.job.execution_root,
                            class_id: case.candidate.job.class_id,
                            artifact_root,
                        };
                        let (frontiers, witness, producer, trace_root) =
                            (case.frontiers.clone(), case.witness.clone(), case.candidate.producer, case.candidate.job.trace_root);
                        let fallback = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
                            &self.config.court,
                            params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),
                        );
                        let ladder = self.seat_refutation_ladder_v1(bound_to.class_id, fallback, current_daa);
                        let terminal = tokio::task::spawn_blocking(move || {
                            replica.terminal(
                                &binding,
                                &frontiers,
                                witness.as_deref(),
                                leaf,
                                claim,
                                &bound_to,
                                trace_root,
                                producer,
                                bond_key,
                                ladder,
                            )
                        })
                        .await
                        .unwrap_or_else(|e| Err(format!("the public terminal task did not finish: {e}")));
                        if let Ok(object) = &terminal
                            && let Some(proof) = PalwFraudFilerTerminalProofV1::of_terminal_v1(object)
                        {
                            // G14 (G-D02a): the terminal's refutation is objective evidence against every liable Valid signer
                            // too; the false-Valid filer files only what the chain's own check accepts, so noting it now (and
                            // again at the conviction, see below) never pays for a filing the fold refuses.
                            false_valid.note_step_refutation_v1(
                                crate::palw_filer_false_valid::palw_false_valid_armed_v1(
                                    params,
                                    self.config.fee_outpoint.is_some(),
                                    current_daa,
                                ),
                                claim,
                                &proof.refutation,
                                &proof.operand_openings,
                                proof.prompt_ids_opening.as_ref(),
                                crate::palw_filer_false_valid::PalwFalseValidSourceV1::CourtClose { leaf: proof.leaf },
                                Some(case.candidate.accepted_daa),
                                current_daa,
                            );
                            book.terminal_proofs.insert(claim, proof);
                        }
                        self.fraud_filer_queue_v1(book, claim, item, current_daa, court_pending, court_due, || {
                            palw_fraud_filer_sign_terminal_v2(
                                terminal?,
                                &network_domain,
                                self.config.court.max_close_bytes(),
                                |m, context| self.sign(m, context),
                            )
                        });
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(phase)) => {
                        match phase {
                            PalwFilerPhaseV1::Convicted | PalwFilerPhaseV1::DaDefault => {
                                info!("[{PALW_PANEL}] claim {claim}: the pursuit ended — {phase:?} (LG14-A)")
                            }
                            _ => warn!(
                                "[{PALW_PANEL}] claim {claim}: the pursuit ended without an objective outcome — {phase:?} (LG14-A)"
                            ),
                        }
                        // G14 (G-D02a): a conviction on our own terminal proves every liable Valid signer false as well.
                        if phase == PalwFilerPhaseV1::Convicted
                            && let Some(proof) = book.terminal_proofs.remove(&claim)
                        {
                            let bound = view.as_ref().map(|v| v.accepted_daa);
                            if false_valid.note_step_refutation_v1(
                                crate::palw_filer_false_valid::palw_false_valid_armed_v1(
                                    params,
                                    self.config.fee_outpoint.is_some(),
                                    current_daa,
                                ),
                                claim,
                                &proof.refutation,
                                &proof.operand_openings,
                                proof.prompt_ids_opening.as_ref(),
                                crate::palw_filer_false_valid::PalwFalseValidSourceV1::CourtClose { leaf: proof.leaf },
                                bound,
                                current_daa,
                            ) {
                                info!(
                                    "[{PALW_PANEL}] claim {claim}: the public conviction at leaf {} is handed to the false-Valid filer (G14 G-D02a)",
                                    proof.leaf
                                );
                            }
                        } else if phase != PalwFilerPhaseV1::Convicted {
                            book.terminal_proofs.remove(&claim);
                        }
                        book.settle(&claim, PalwFraudFilerVerdictV1::Settled(phase));
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::HeldRoute { leaf }) => {
                        warn!(
                            "[{PALW_PANEL}] claim {claim}: the first divergent leaf {leaf} is a fused-attention site — its terminal is the \
                             held dissection's (LG14-B), not a unit; the reservation keeps the claim held until its hard deadline"
                        );
                        book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(format!("fused leaf {leaf}: the held dissection's")));
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve) => {
                        let Some(view) = view.as_ref() else { break };
                        let producer = case.candidate.producer;
                        if palw_fraud_filer_defer_reservation_v1(&book.focus, &producer, &claim, view.deadline_daa, current_daa) {
                            crate::palw_backends::note_throttled_v1("fraud-filer-focus", || {
                                format!(
                                    "[{PALW_PANEL}] claim {claim}: reservation deferred — producer's claim {} is pursued first (G14 G-D02c)",
                                    book.focus[&producer]
                                )
                            });
                            break;
                        }
                        book.focus.entry(producer).or_insert(claim);
                        self.fraud_filer_queue_v1(
                            book,
                            claim,
                            PalwFraudFilerSentV1::Reserve,
                            current_daa,
                            court_pending,
                            court_due,
                            || {
                                let mut signed = true;
                                let object = palw_dispute_reserved_object_v1(
                                    network_domain,
                                    palw_fraud_filer_reservation_v1(view, bond_key),
                                    |m| {
                                        self.sign(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(|| {
                                            signed = false;
                                            Vec::new()
                                        })
                                    },
                                );
                                if signed { Ok(object) } else { Err("no signing key for this bond".to_string()) }
                            },
                        );
                        break;
                    }
                    PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe)) => {
                        let Some(view) = view.as_ref() else { break };
                        let binding = case.binding.clone();
                        match probe {
                            PalwLegacyProbeV1::HeldNode { unit } => book.want_legacy(claim, unit),
                            PalwLegacyProbeV1::Range { .. } => book.want_rcore(claim, probe.unit()),
                            PalwLegacyProbeV1::Binding { row, tile } if (row, tile) != (0, 0) => book.want_rcore(claim, probe.unit()),
                            _ => {}
                        }
                        self.fraud_filer_queue_v1(
                            book,
                            claim,
                            PalwFraudFilerSentV1::Demand(probe),
                            current_daa,
                            court_pending,
                            court_due,
                            || {
                                palw_fraud_filer_demand_object_v1(
                                    &network_domain,
                                    claim,
                                    &view.execution_root,
                                    probe,
                                    binding.as_ref(),
                                    bond_key,
                                    form,
                                    |message, context| self.sign(message, context),
                                )
                            },
                        );
                        break;
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_public_context_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        owner: PalwBondKeyV2,
    ) -> Result<(), String> {
        let Some(progress) = session.palw_staged_owned_progress_v1((claim, owner))? else { return Ok(()) };
        let Some(bundle) = &progress.bundle else { return Ok(()) };
        let bytes = bundle
            .plan
            .terms
            .plan
            .resident_ram_bytes
            .checked_mul(8)
            .and_then(|n| n.checked_add(16_384))
            .ok_or("original public context memory overflow")?;
        let ticket = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-original-binding-context",
                    class_id: Hash64::default(),
                    job: claim,
                },
                bytes,
            )
            .map_err(|e| e.to_string())?;
        let Some((binding, job)) = session.palw_staged_resolved_public_context_v1(claim)? else {
            return Err("original public context has no qualified Legacy hydration".into());
        };
        let expected = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context);
        let case = book.cases.get_mut(&claim).ok_or("public context pursuit disappeared")?;
        let descriptor = input::PalwFraudFilerFpDescriptorV1::from_disclosure(&case.candidate, expected, &binding, &job)?;
        if case.fp_expected_pin != Some(expected)
            || (progress.claim, progress.reserver) != (claim, owner)
            || binding.step_leaf_count > self.class_step_ladder(job.class_id)
        {
            return Err("original public context substitutes target/owner/job or registered ladder".into());
        }
        case.bisect = PalwLegacyBisectV1::new(binding.step_leaf_count);
        case.step_tree = PalwAuthTreeV1::new(binding.step_leaf_count, binding.step_merkle_root);
        case.binding = Some(binding);
        case.fp_descriptor = Some(Arc::new(descriptor));
        case._public_context_reservation = Some(Arc::new(ticket));
        case.fp_job_pending = true;
        case.staged_bundle_pending = true;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_constraint_stage_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        selected_events: Option<Vec<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1>>,
        flat: Option<(constraint_scheduler::StreamedPinV1, Hash64, PalwBondKeyV2)>,
        flat_original: Option<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3>,
        original_seed: Option<kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1>,
        progress: kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        maximum: u64,
        file_by: Option<u64>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use constraint_scheduler::{ConstraintScheduleV1, ConstraintSchedulerV1};
        use kaspa_consensus_core::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1;
        let case = book.cases.get_mut(&claim).ok_or("constraint pursuit disappeared")?;
        case.constraint_stage_pending = true;
        let worker = case.staged_constraint.get_or_insert_with(|| Arc::new(std::sync::Mutex::new(None))).clone();
        let plan = case.constraint_plan;
        let public_events = selected_events.as_ref().map(|events| events.iter().collect()).or_else(|| {
            plan.and_then(|p| p.legacy_units_v1()).and_then(|units| {
                units
                    .into_iter()
                    .map(|unit| match book.answers.get(&(claim, unit)) {
                        Some(PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event))) => Some(event),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()
            })
        });
        let copy_bytes = (borsh::object_length(&(binding, job, &original_seed, &public_events, &flat_original))
            .map_err(|e| e.to_string())? as u64)
            .checked_mul(8)
            .and_then(|n| n.checked_add(147_456))
            .ok_or("constraint worker copy bound overflow")?;
        let copies = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-constraint-worker-copies",
                    class_id: target.class_id,
                    job: claim,
                },
                copy_bytes,
            )
            .map_err(|e| e.to_string())?;
        let public_events = public_events.map(|events| events.into_iter().cloned().collect::<Vec<_>>());
        let table_root = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job)
            .ok_or("constraint job has no immutable table root")?;
        let table = crate::palw_public_token_table::public_table_v1(&table_root, &job.tokenizer_id)
            .ok_or("constraint exact public table holding is unavailable")?;
        let binding = binding.clone();
        let job = job.clone();
        let target = target.clone();
        let maximum_leaves = self.class_step_ladder(target.class_id);
        let executor = target.executor_bond;
        let now = progress.now_daa;
        let worker_progress = progress.clone();
        let decision = tokio::task::spawn_blocking(move || {
            let _copies = copies;
            let mut worker = worker.lock().map_err(|_| "constraint worker lock poisoned")?;
            let build = || {
                if let Some(seed) = &original_seed {
                    ConstraintSchedulerV1::reopen_with_flat_v1(
                        &target,
                        &job,
                        &binding,
                        seed,
                        flat.as_ref().map(|(pin, _, _)| pin),
                        flat_original.as_ref(),
                        table.clone(),
                        &worker_progress,
                        maximum_leaves,
                        &crate::palw_memory_ledger::host_ledger_v1(),
                    )
                } else if let Some((pin, source_id, answerer)) = &flat {
                    ConstraintSchedulerV1::open_from_flat_v1(
                        &target,
                        &job,
                        &binding,
                        pin,
                        *source_id,
                        *answerer,
                        plan.ok_or("Flat authenticated nomination is unavailable")?.witness,
                        flat_original.as_ref().ok_or("Flat semantic opening lost native original commitment")?,
                        table.clone(),
                        &worker_progress,
                        &crate::palw_memory_ledger::host_ledger_v1(),
                    )
                } else {
                    ConstraintSchedulerV1::open(
                        &target,
                        &job,
                        plan.ok_or("authenticated constraint nominee is unavailable")?,
                        public_events.as_deref().ok_or("original selected public tile acquisition is incomplete")?,
                        table.clone(),
                        &worker_progress,
                        maximum_leaves,
                        &crate::palw_memory_ledger::host_ledger_v1(),
                    )
                }
            };
            if worker.is_none() {
                *worker = Some(build()?);
            }
            match worker.as_mut().expect("built").poll(&worker_progress) {
                Ok(decision) => Ok(decision),
                Err(why) if why.contains("cursor") || why.contains("branch") => {
                    *worker = None;
                    *worker = Some(build()?);
                    worker.as_mut().expect("rebuilt").poll(&worker_progress)
                }
                Err(why) => Err(why),
            }
        })
        .await
        .map_err(|e| format!("constraint worker did not finish: {e}"))??;
        let object = match decision {
            ConstraintScheduleV1::Finished(reference) => {
                court_pending
                    .retain(|(c, round, responder, _)| (*c, *round, *responder) != (claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false));
                let case = book.cases.get_mut(&claim).ok_or("constraint pursuit disappeared")?;
                case.staged_tokenizer_sent = None;
                case.constraint_stage_pending = false;
                case.constraint_done = true;
                let Some(reference) = reference else {
                    return Ok((None, true));
                };
                let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                    claim,
                    executor,
                    kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StagedCourt { reference },
                    None,
                    maximum,
                )
                .map_err(|e| e.to_string())?;
                let filing = PalwConvictionFilingV1::of_offence(built.object, claim, file_by, PalwFilingOriginV1::Replay)
                    .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                    .ok_or("constraint staged reporter key mismatch")?;
                return Ok((Some(filing), false));
            }
            ConstraintScheduleV1::Seed(mut seed) => {
                seed.signature =
                    self.sign(&seed.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no constraint owner signing key")?;
                seed.check(90_000)?;
                PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed: Box::new(seed) }
            }
            ConstraintScheduleV1::NativeSeed(mut seed) => {
                seed.signature =
                    self.sign(&seed.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no constraint owner signing key")?;
                seed.check(90_000)?;
                PalwConsensusObjectV2::PublicCourtStreamOpenedNativeV1 { seed: Box::new(seed) }
            }
            ConstraintScheduleV1::Leaf(mut leaf) => {
                leaf.signature =
                    self.sign(&leaf.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no constraint owner signing key")?;
                leaf.check()?;
                PalwConsensusObjectV2::PublicCourtStreamLeafV1 { leaf: Box::new(leaf) }
            }
            ConstraintScheduleV1::Move(mut movement) => {
                movement.signature = self
                    .sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1)
                    .ok_or("no constraint owner signing key")?;
                movement.check()?;
                PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }
            }
        };
        Self::fraud_filer_stage_queue_v1(book, claim, object, &progress, now, court_pending, court_due)?;
        Ok((None, false))
    }

    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_flat_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        progress: &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<
        Option<(
            kaspa_consensus_core::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
            Hash64,
            PalwBondKeyV2,
            crate::palw_memory_ledger::PalwMemoryReservationV1,
        )>,
        String,
    > {
        use crate::palw_flat_public_source::FlatSourceScheduleV1;
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{
                PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
            },
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_selected_event_source_v1::PalwSelectedEventBindingV1,
        };
        let bundle = progress.bundle.as_ref().ok_or("Flat source has no prepaid bundle")?;
        let bytes = bundle.plan.terms.plan.resident_ram_bytes.checked_mul(2).ok_or("Flat snapshot memory overflow")?;
        let guard = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-flat-source-read",
                    class_id: target.class_id,
                    job: claim,
                },
                bytes,
            )
            .map_err(|e| e.to_string())?;
        let flat = session.palw_staged_flat_source_v1((claim, progress.reserver))?;
        let mut due = progress.terms.open_until_daa.saturating_sub(1);
        let action = if let Some(flat) = flat {
            // A separate native read may land on a newer accepted tip (blocks arrive between the
            // two reads). That is tolerated: the literal identity and immutable clocks below are
            // what must match, and every decision uses this read's own clock. An older read is not.
            if flat.now_daa < progress.now_daa
                || (flat.session.claim, flat.session.owner, flat.session.reserved_daa, flat.session.bundle_id)
                    != (claim, progress.reserver, progress.reserved_daa, bundle.bundle_id)
                || flat.context.binding.as_ref() != binding
                || flat.context.job.as_ref() != job
                || (flat.session.respond_until_daa, flat.session.collect_until_daa)
                    != (
                        bundle.plan.preseed_source_until_v1(
                            kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint,
                            kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
                        )?,
                        progress.terms.collect_until_daa,
                    )
            {
                return Err("Flat source changes original binding/job/owner/paid window or snapshot".into());
            }
            let decision = crate::palw_flat_public_source::request_v1(&flat.session, flat.now_daa)?;
            if matches!(decision, FlatSourceScheduleV1::Nominate { .. } | FlatSourceScheduleV1::Ready { .. }) {
                let ids = flat.session.authenticated_generated_ids_v1()?;
                let case = book.cases.get(&claim).ok_or("Flat pursuit disappeared")?;
                let own = match &case.verdict {
                    PalwFraudFilerVerdictV1::Mismatch(run) => run.own_output_ids.as_slice(),
                    _ => &[],
                };
                let replayed =
                    case.arithmetic_matched || matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if run.fp_job.is_some());
                let root = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job)
                    .ok_or("Flat Constraint loses immutable table root")?;
                let table = crate::palw_public_token_table::public_table_v1(&root, &job.tokenizer_id)
                    .ok_or("Flat exact public table is unavailable")?;
                // Streaming mask: bounded parts of the original table, never a resident vocabulary.
                let nominee = constraint_scheduler::nominate_authenticated_ids_v1(
                    target,
                    job,
                    binding,
                    ids,
                    own,
                    table,
                    &crate::palw_memory_ledger::host_ledger_v1(),
                )?;
                let nominee = match nominee {
                    Some(plan) => plan,
                    None if replayed => {
                        constraint::TiledPlanV1 { position: ids.len() as u32, committed: 0, witness: None, beat_lane: None }
                    }
                    _ => return Ok(None),
                };
                if flat.session.nomination_v1().is_some_and(|n| n != (nominee.position, nominee.beat_lane)) {
                    return Err("Flat local selector differs from immutable accepted nomination".into());
                }
                book.cases.get_mut(&claim).ok_or("Flat pursuit disappeared")?.constraint_plan = Some(nominee);
            }
            match decision {
                FlatSourceScheduleV1::Wait => return Ok(None),
                FlatSourceScheduleV1::Ready { signer } => {
                    if !flat.session.collected_v1()
                        && flat.session.custodians.iter().any(|signer| flat.session.has_response_duty_v1(signer))
                    {
                        return Ok(None);
                    }
                    let pin = session
                        .palw_staged_flat_pin_v1((claim, progress.reserver), flat.session.source_id, signer)?
                        .ok_or("Flat native EOF pin is unavailable")?;
                    return Ok(Some((pin, flat.session.source_id, signer, guard)));
                }
                FlatSourceScheduleV1::Nominate { source_id } => {
                    due = flat.session.nomination_until_daa.saturating_sub(1);
                    let plan =
                        book.cases.get(&claim).and_then(|case| case.constraint_plan).ok_or("Flat owner nomination is unavailable")?;
                    PalwConstraintStreamActionV1::FlatSourceNominate { source_id, position: plan.position, beat: plan.beat_lane }
                }
                FlatSourceScheduleV1::Collect => {
                    due = flat.session.collect_until_daa.saturating_sub(1);
                    PalwConstraintStreamActionV1::FlatSourceCollect
                }
            }
        } else {
            PalwConstraintStreamActionV1::FlatSourceOpen { binding: PalwSelectedEventBindingV1::from_binding_v1(binding)? }
        };
        let mut movement = PalwConstraintStreamMoveV1 {
            version: 1,
            claim,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            seed_id: bundle.bundle_id,
            phase: PalwConstraintStreamPhaseV1::PrefixCourt,
            action,
            signature: Vec::new(),
        };
        movement.signature =
            self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no Flat source owner signing key")?;
        movement.check()?;
        Self::fraud_filer_stage_queue_v1(
            book,
            claim,
            PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) },
            progress,
            progress.now_daa,
            court_pending,
            court_due,
        )?;
        // The accepted source's nomination clock is earlier than the child opening clock.
        court_due.entry((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)).and_modify(|held| *held = (*held).min(due));
        Ok(None)
    }

    /// **The greedy Float32 source owner** (behind `palw_staged_float_constraint_v1`): opens the
    /// accepted Float32 source, nominates over the authenticated ids from its own replay (the Flat
    /// owner's selector — a replay that diverged names the position and the beating lane), collects
    /// missed custodian clocks, and hands the native authenticated row pin to the Constraint seed.
    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_float_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        progress: &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<
        Option<(
            kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatConstraintPinV1,
            Hash64,
            PalwBondKeyV2,
            crate::palw_memory_ledger::PalwMemoryReservationV1,
        )>,
        String,
    > {
        use crate::palw_flat_public_source::FloatSourceScheduleV1;
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{
                PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
            },
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_selected_event_source_v1::PalwSelectedEventBindingV1,
        };
        if self.consensus_config.params.palw_staged_float_constraint_v1_fence().is_none() {
            return Ok(None);
        }
        let bundle = progress.bundle.as_ref().ok_or("Float32 source has no prepaid bundle")?;
        let bytes = bundle.plan.terms.plan.resident_ram_bytes.checked_mul(2).ok_or("Float32 snapshot memory overflow")?;
        let guard = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-float-source-read",
                    class_id: target.class_id,
                    job: claim,
                },
                bytes,
            )
            .map_err(|e| e.to_string())?;
        let float = session.palw_staged_float_source_v1((claim, progress.reserver))?;
        let mut due = progress.terms.open_until_daa.saturating_sub(1);
        let action = if let Some(float) = float {
            if float.now_daa < progress.now_daa
                || (float.session.claim, float.session.owner, float.session.reserved_daa, float.session.bundle_id)
                    != (claim, progress.reserver, progress.reserved_daa, bundle.bundle_id)
                || float.context.binding.as_ref() != binding
                || float.context.job.as_ref() != job
                || (float.session.respond_until_daa, float.session.collect_until_daa)
                    != (
                        bundle.plan.preseed_source_until_v1(
                            kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint,
                            kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
                        )?,
                        progress.terms.collect_until_daa,
                    )
            {
                return Err("Float32 source changes original binding/job/owner/paid window or snapshot".into());
            }
            let decision = crate::palw_flat_public_source::float_request_v1(&float.session, float.now_daa)?;
            if matches!(decision, FloatSourceScheduleV1::Nominate { .. } | FloatSourceScheduleV1::Ready { .. }) {
                let ids = float.session.authenticated_generated_ids_v1()?;
                let case = book.cases.get(&claim).ok_or("Float32 pursuit disappeared")?;
                let own = match &case.verdict {
                    PalwFraudFilerVerdictV1::Mismatch(run) => run.own_output_ids.as_slice(),
                    _ => &[],
                };
                let replayed =
                    case.arithmetic_matched || matches!(&case.verdict, PalwFraudFilerVerdictV1::Mismatch(run) if run.fp_job.is_some());
                let root = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job)
                    .ok_or("Float32 Constraint loses immutable table root")?;
                let table = crate::palw_public_token_table::public_table_v1(&root, &job.tokenizer_id)
                    .ok_or("Float32 exact public table is unavailable")?;
                let nominee = constraint_scheduler::nominate_authenticated_ids_v1(
                    target,
                    job,
                    binding,
                    ids,
                    own,
                    table,
                    &crate::palw_memory_ledger::host_ledger_v1(),
                )?;
                let nominee = match nominee {
                    Some(plan) => plan,
                    None if replayed => {
                        constraint::TiledPlanV1 { position: ids.len() as u32, committed: 0, witness: None, beat_lane: None }
                    }
                    _ => return Ok(None),
                };
                if float.session.nomination_v1().is_some_and(|n| n != (nominee.position, nominee.beat_lane)) {
                    return Err("Float32 local selector differs from immutable accepted nomination".into());
                }
                book.cases.get_mut(&claim).ok_or("Float32 pursuit disappeared")?.constraint_plan = Some(nominee);
            }
            match decision {
                FloatSourceScheduleV1::Wait => return Ok(None),
                FloatSourceScheduleV1::Ready { signer } => {
                    if !float.session.collected_v1()
                        && float.session.custodians.iter().any(|signer| float.session.has_response_duty_v1(signer))
                    {
                        return Ok(None);
                    }
                    let pin = session
                        .palw_staged_float_pin_v1((claim, progress.reserver), float.session.source_id, signer)?
                        .ok_or("Float32 authenticated row pin is unavailable")?;
                    return Ok(Some((pin, float.session.source_id, signer, guard)));
                }
                FloatSourceScheduleV1::Nominate { source_id } => {
                    due = float.session.nomination_until_daa.saturating_sub(1);
                    let plan =
                        book.cases.get(&claim).and_then(|case| case.constraint_plan).ok_or("Float32 owner nomination is unavailable")?;
                    PalwConstraintStreamActionV1::FloatSourceNominate { source_id, position: plan.position, beat: plan.beat_lane }
                }
                FloatSourceScheduleV1::Collect => {
                    due = float.session.collect_until_daa.saturating_sub(1);
                    PalwConstraintStreamActionV1::FloatSourceCollect
                }
            }
        } else {
            PalwConstraintStreamActionV1::FloatSourceOpen { binding: PalwSelectedEventBindingV1::from_binding_v1(binding)? }
        };
        let mut movement = PalwConstraintStreamMoveV1 {
            version: 1,
            claim,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            seed_id: bundle.bundle_id,
            phase: PalwConstraintStreamPhaseV1::PrefixCourt,
            action,
            signature: Vec::new(),
        };
        movement.signature =
            self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no Float32 source owner signing key")?;
        movement.check()?;
        Self::fraud_filer_stage_queue_v1(
            book,
            claim,
            PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) },
            progress,
            progress.now_daa,
            court_pending,
            court_due,
        )?;
        court_due.entry((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)).and_modify(|held| *held = (*held).min(due));
        Ok(None)
    }

    /// **A TIR claim's Constraint stage** (behind `palw_staged_tir_route_v1`): the native selected
    /// source (no binding on any carrier) until the nominee's tiles are authenticated against the
    /// view the fold derives, then the native seed and the constrained-selection moves.
    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_tir_constraint_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        tir: &FilerTirContextV1,
        progress: kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        maximum: u64,
        file_by: Option<u64>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use constraint_scheduler::{ConstraintScheduleV1, ConstraintSchedulerV1};
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{
                PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
            },
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_selected_event_source_v1::PalwSelectedEventCoordinateV1,
        };
        use selected_event_scheduler::SelectedEventScheduleV1;
        let ladder = self.class_step_ladder(target.class_id);
        let root = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job)
            .ok_or("TIR Constraint job loses its immutable table root")?;
        let table = crate::palw_public_token_table::public_table_v1(&root, &job.tokenizer_id)
            .ok_or("TIR Constraint exact public table is unavailable")?;
        let mut pin = None;
        if progress.cursor.is_none() {
            let selected = session
                .palw_staged_selected_events_v1((claim, progress.reserver))?
                .ok_or("TIR native selected source is unavailable")?;
            let bundle = progress.bundle.as_ref().ok_or("TIR selected source has no prepaid owner bundle")?;
            if selected.now_daa < progress.now_daa
                || (selected.session.claim, selected.session.owner, selected.session.reserved_daa, selected.session.bundle_id)
                    != (claim, progress.reserver, progress.reserved_daa, bundle.bundle_id)
                || selected.context.is_some()
                || selected.native.as_ref().is_some_and(|native| *native.header != *tir.header || native.job.as_ref() != job)
            {
                return Err("TIR selected source changes original owner/bundle, header, job or snapshot".into());
            }
            let bootstrap = PalwSelectedEventCoordinateV1 { row: 0, tile: 0 };
            if book.cases.get(&claim).and_then(|case| case.constraint_plan).is_none()
                && let Some(event) = selected.session.requests.get(&bootstrap).and_then(|request| request.event.as_ref())
            {
                let case = book.cases.get(&claim).ok_or("TIR pursuit disappeared")?;
                // The verifier's own TIR replay (the backend's `replay_tir_generated_ids_v1`, RFC-0002
                // lane) names the beat; without it only an id the constraint forbids is nominated.
                let own = match &case.verdict {
                    PalwFraudFilerVerdictV1::Mismatch(run) => run.own_output_ids.as_slice(),
                    _ => case.tir_selection_ids.as_ref().map(|holding| holding.ids.as_slice()).unwrap_or_default(),
                };
                let nominee = constraint_scheduler::nominate_authenticated_ids_view_v1(
                    target,
                    job,
                    &tir.view,
                    &event.generated_token_ids,
                    own,
                    table.clone(),
                    &crate::palw_memory_ledger::host_ledger_v1(),
                )?
                .or_else(|| case.neutral_constraint_plan_v1(&event.generated_token_ids));
                book.cases.get_mut(&claim).ok_or("TIR pursuit disappeared")?.constraint_plan = nominee;
            }
            let plan = book.cases.get(&claim).and_then(|case| case.constraint_plan);
            let mut coordinates = vec![bootstrap];
            if let Some(plan) = plan {
                for (row, tile) in plan.selected_tiles_v1() {
                    let coordinate = PalwSelectedEventCoordinateV1 { row, tile };
                    if !coordinates.contains(&coordinate) {
                        coordinates.push(coordinate);
                    }
                }
            }
            let action = match selected_event_scheduler::request_v1(&selected.session, &coordinates, selected.now_daa)? {
                SelectedEventScheduleV1::Wait => return Ok((None, false)),
                SelectedEventScheduleV1::Ready => {
                    let Some(plan) = plan else { return Ok((None, false)) };
                    let events = plan
                        .selected_tiles_v1()
                        .into_iter()
                        .map(|(row, tile)| {
                            selected
                                .session
                                .requests
                                .get(&PalwSelectedEventCoordinateV1 { row, tile })
                                .and_then(|request| request.event.clone())
                                .ok_or("TIR selected source lost authenticated accepted material".to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    pin = Some((constraint_scheduler::native_pin_v1(target, job, &tir.view, plan, &events, ladder)?, plan.witness));
                    None
                }
                SelectedEventScheduleV1::Demand(demand) => Some(PalwConstraintStreamActionV1::NativeSelectedDemand { demand }),
                SelectedEventScheduleV1::Collect { coordinate, demand_id } => {
                    Some(PalwConstraintStreamActionV1::SelectedEventCollect { coordinate, demand_id })
                }
            };
            if let Some(action) = action {
                let demand_due = matches!(action, PalwConstraintStreamActionV1::NativeSelectedDemand { .. }).then(|| {
                    selected.session.respond_until_daa.saturating_sub(selected.session.minimum_response_span_daa).saturating_sub(1)
                });
                let mut movement = PalwConstraintStreamMoveV1 {
                    version: 1,
                    claim,
                    reserver: progress.reserver,
                    reserved_daa: progress.reserved_daa,
                    seed_id: selected.session.bundle_id,
                    phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                    action,
                    signature: Vec::new(),
                };
                movement.signature =
                    self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no TIR owner signing key")?;
                movement.check()?;
                Self::fraud_filer_stage_queue_v1(
                    book,
                    claim,
                    PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) },
                    &progress,
                    progress.now_daa,
                    court_pending,
                    court_due,
                )?;
                if let Some(due) = demand_due {
                    court_due.entry((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)).and_modify(|held| *held = (*held).min(due));
                }
                return Ok((None, false));
            }
        }
        let accepted = if progress.cursor.is_some() { session.palw_staged_owned_native_seed_v1((claim, progress.reserver))? } else { None };
        let case = book.cases.get_mut(&claim).ok_or("TIR constraint pursuit disappeared")?;
        case.constraint_stage_pending = true;
        let worker = case.staged_constraint.get_or_insert_with(|| Arc::new(std::sync::Mutex::new(None))).clone();
        let target = target.clone();
        let job = job.clone();
        let view = tir.view.clone();
        let header = tir.header.clone();
        let executor = target.executor_bond;
        let now = progress.now_daa;
        let worker_progress = progress.clone();
        let decision = tokio::task::spawn_blocking(move || {
            let mut worker = worker.lock().map_err(|_| "TIR constraint worker lock poisoned")?;
            let build = || match (&accepted, &pin) {
                (Some(accepted), _) => ConstraintSchedulerV1::reopen_native_v1(
                    &target,
                    &job,
                    &view,
                    accepted,
                    table.clone(),
                    &worker_progress,
                    ladder,
                    &crate::palw_memory_ledger::host_ledger_v1(),
                ),
                (None, Some((pin, witness))) => ConstraintSchedulerV1::open_native_v1(
                    &target,
                    &job,
                    &view,
                    &header,
                    pin.clone(),
                    *witness,
                    table.clone(),
                    &worker_progress,
                    ladder,
                    &crate::palw_memory_ledger::host_ledger_v1(),
                ),
                (None, None) => Err("TIR constraint has neither an accepted native seed nor an authenticated pin".to_string()),
            };
            if worker.is_none() {
                *worker = Some(build()?);
            }
            match worker.as_mut().expect("built").poll(&worker_progress) {
                Ok(decision) => Ok(decision),
                Err(why) if why.contains("cursor") || why.contains("branch") => {
                    *worker = None;
                    *worker = Some(build()?);
                    worker.as_mut().expect("rebuilt").poll(&worker_progress)
                }
                Err(why) => Err(why),
            }
        })
        .await
        .map_err(|e| format!("TIR constraint worker did not finish: {e}"))??;
        let object = match decision {
            ConstraintScheduleV1::Finished(reference) => {
                court_pending
                    .retain(|(c, round, responder, _)| (*c, *round, *responder) != (claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false));
                let case = book.cases.get_mut(&claim).ok_or("TIR constraint pursuit disappeared")?;
                case.constraint_stage_pending = false;
                case.constraint_done = true;
                let Some(reference) = reference else {
                    return Ok((None, true));
                };
                let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                    claim,
                    executor,
                    kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StagedCourt { reference },
                    None,
                    maximum,
                )
                .map_err(|e| e.to_string())?;
                let filing = PalwConvictionFilingV1::of_offence(built.object, claim, file_by, PalwFilingOriginV1::Replay)
                    .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                    .ok_or("TIR constraint staged reporter key mismatch")?;
                return Ok((Some(filing), false));
            }
            ConstraintScheduleV1::NativeSeed(mut seed) => {
                seed.signature =
                    self.sign(&seed.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no TIR owner signing key")?;
                seed.check(90_000)?;
                PalwConsensusObjectV2::PublicCourtStreamOpenedNativeV1 { seed: Box::new(seed) }
            }
            ConstraintScheduleV1::Leaf(mut leaf) => {
                leaf.signature =
                    self.sign(&leaf.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no TIR owner signing key")?;
                leaf.check()?;
                PalwConsensusObjectV2::PublicCourtStreamLeafV1 { leaf: Box::new(leaf) }
            }
            ConstraintScheduleV1::Move(mut movement) => {
                movement.signature =
                    self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no TIR owner signing key")?;
                movement.check()?;
                PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }
            }
            ConstraintScheduleV1::Seed(_) => return Err("a TIR scheduler never emits a legacy seed".into()),
        };
        Self::fraud_filer_stage_queue_v1(book, claim, object, &progress, now, court_pending, court_due)?;
        Ok((None, false))
    }

    /// **A TIR claim's prefix-state stage** (behind `palw_staged_tir_route_v1`): the owner opens
    /// the stage with a native `PrefixState` seed; the claim's literal custody then owes the
    /// boundary leaves (the node's TIR prefix responders answer); after the custody clock the
    /// owner collects; a completed slot that re-derives another state root than the job declared
    /// is filed as a staged-court conviction.
    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_tir_prefix_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        tir: &FilerTirContextV1,
        progress: kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        maximum: u64,
        file_by: Option<u64>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{
                PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
                PalwConstraintStreamNativeSeedV1, PalwNativeSeedFamilyV1,
            },
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_staged_court_state_v1::{PalwStagedCertificateRefV1, PalwStagedOwnedCursorV1},
        };
        if progress.claim != claim || progress.reserver == target.executor_bond {
            return Err("snapshot is not this outside verifier's accepted TIR claim/clock".into());
        }
        let object = match &progress.cursor {
            None => {
                if progress.now_daa >= progress.terms.open_until_daa {
                    return Err("TIR prefix opening door is closed".into());
                }
                let named = kaspa_consensus_core::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| e.to_string())?;
                let mut seed = PalwConstraintStreamNativeSeedV1 {
                    version: 1,
                    claim,
                    reserver: progress.reserver,
                    reserved_daa: progress.reserved_daa,
                    view_identity: kaspa_consensus_core::palw_constraint_stream_carriage_v1::palw_native_view_identity_v1(&tir.header),
                    vocab_size: tir.view.vocab_size,
                    job: None,
                    family: PalwNativeSeedFamilyV1::PrefixState { prefix_tokens: named.prefix_tokens },
                    signature: Vec::new(),
                };
                seed.signature =
                    self.sign(&seed.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no TIR prefix owner signing key")?;
                seed.check(90_000)?;
                PalwConsensusObjectV2::PublicCourtStreamOpenedNativeV1 { seed: Box::new(seed) }
            }
            Some(PalwStagedOwnedCursorV1::TirPrefix { completed: Some(true), .. }) => {
                let reference = PalwStagedCertificateRefV1 {
                    claim,
                    reserver: progress.reserver,
                    reserved_daa: progress.reserved_daa,
                    seed_id: progress.seed_id.ok_or("TIR prefix fault lost its accepted seed")?,
                };
                let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                    claim,
                    target.executor_bond,
                    kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StagedCourt { reference },
                    None,
                    maximum,
                )
                .map_err(|e| e.to_string())?;
                let filing = PalwConvictionFilingV1::of_offence(built.object, claim, file_by, PalwFilingOriginV1::Replay)
                    .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                    .ok_or("TIR prefix staged reporter key mismatch")?;
                return Ok((Some(filing), false));
            }
            Some(PalwStagedOwnedCursorV1::TirPrefix { completed: Some(false), .. }) => return Ok((None, true)),
            Some(PalwStagedOwnedCursorV1::TirPrefix { collected: false, .. })
                if progress.now_daa >= progress.terms.stream_until_daa && progress.now_daa < progress.terms.collect_until_daa =>
            {
                let mut movement = PalwConstraintStreamMoveV1 {
                    version: 1,
                    claim,
                    reserver: progress.reserver,
                    reserved_daa: progress.reserved_daa,
                    seed_id: progress.seed_id.ok_or("TIR prefix collection lost its accepted seed")?,
                    phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                    action: PalwConstraintStreamActionV1::NativeTirPrefixCollect,
                    signature: Vec::new(),
                };
                movement.signature =
                    self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no TIR prefix owner signing key")?;
                movement.check()?;
                PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }
            }
            // The custody is streaming (or was collected without a completed slot): nothing to send.
            Some(PalwStagedOwnedCursorV1::TirPrefix { .. }) => return Ok((None, false)),
            Some(_) => return Err("another immutable family owns this TIR prefix stage".into()),
        };
        let _ = session;
        Self::fraud_filer_stage_queue_v1(book, claim, object, &progress, progress.now_daa, court_pending, court_due)?;
        Ok((None, false))
    }

    /// **A TIR claim's tokenizer stage** (behind `palw_staged_tir_route_v1`): the legacy tokenizer
    /// worker over the natively derived view, signing a native seed.
    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_tir_tokenizer_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        tir: &FilerTirContextV1,
        progress: kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        maximum: u64,
        file_by: Option<u64>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use kaspa_consensus_core::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1;
        use tokenizer_scheduler::{TokenizerScheduleV1, TokenizerSchedulerV1};
        if progress.claim != claim || progress.reserver == target.executor_bond {
            return Err("snapshot is not this outside verifier's accepted TIR claim/clock".into());
        }
        let current_daa = progress.now_daa;
        let deadline = if progress.cursor.is_none() { progress.terms.open_until_daa } else { progress.terms.collect_until_daa };
        if progress.now_daa >= deadline {
            return Err("prospective owned TIR tokenizer court clock is closed".into());
        }
        let worker = book
            .cases
            .get_mut(&claim)
            .ok_or("TIR pursuit disappeared")?
            .staged_tokenizer
            .get_or_insert_with(|| Arc::new(std::sync::Mutex::new(None)))
            .clone();
        let backends = self.backends();
        let chain_classes = self.config.chain_classes;
        let held_session = session.clone();
        let executor_bond = target.executor_bond;
        let target = target.clone();
        let job = job.clone();
        let view = tir.view.clone();
        let header = tir.header.clone();
        let worker_progress = progress.clone();
        let original_seed = if matches!(
            progress.cursor,
            Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedCursorV1::Tokenizer { .. })
        ) {
            session.palw_staged_owned_native_seed_v1((claim, progress.reserver))?.map(|context| *context.seed)
        } else {
            None
        };
        let decision = tokio::task::spawn_blocking(move || -> Result<TokenizerScheduleV1, String> {
            let mut worker = worker.lock().map_err(|_| "TIR tokenizer worker lock poisoned")?;
            let build = || -> Result<TokenizerSchedulerV1, String> {
                let path = backends
                    .held_tokenizer_path_or_chain_v1(target.class_id, target.artifact_root, |id| {
                        if chain_classes { held_session.palw_registered_class_carriage_v1(id) } else { None }
                    })
                    .ok_or("outside verifier does not hold registered tokenizer")?;
                let original = tokenizer_scheduler::TokenizerOriginalV1::Tir { view: &view, header: &header };
                let table = tokenizer_scheduler::registered_table_of_v1(&target, original, &job)?;
                TokenizerSchedulerV1::open_native_v1(
                    &path,
                    &target,
                    &view,
                    &header,
                    &job,
                    table,
                    &worker_progress,
                    &crate::palw_memory_ledger::host_ledger_v1(),
                    original_seed.as_ref(),
                )
            };
            if worker.is_none() {
                *worker = Some(build()?);
            }
            match worker.as_mut().expect("built").poll(&worker_progress) {
                Ok(decision) => Ok(decision),
                Err(why) if why.contains("cursor") || why.contains("branch") => {
                    *worker = None;
                    *worker = Some(build()?);
                    worker.as_mut().expect("rebuilt").poll(&worker_progress)
                }
                Err(why) => Err(why),
            }
        })
        .await
        .map_err(|e| format!("TIR tokenizer blocking worker failed: {e}"))??;
        let object = match decision {
            TokenizerScheduleV1::Finished(reference) => {
                court_pending.retain(|(held, round, responder, _)| {
                    (*held, *round, *responder) != (claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)
                });
                book.cases.get_mut(&claim).expect("pursued").staged_tokenizer_sent = None;
                let Some(reference) = reference else {
                    return Ok((None, true));
                };
                let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                    claim,
                    executor_bond,
                    kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StagedCourt { reference },
                    None,
                    maximum,
                )
                .map_err(|e| e.to_string())?;
                let filing = PalwConvictionFilingV1::of_offence(built.object, claim, file_by, PalwFilingOriginV1::Replay)
                    .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                    .ok_or("TIR staged reporter key mismatch")?;
                return Ok((Some(filing), false));
            }
            TokenizerScheduleV1::NativeSeed(mut seed) => {
                seed.signature = self.sign(&seed.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no staged signing key")?;
                seed.check(90_000)?;
                PalwConsensusObjectV2::PublicCourtStreamOpenedNativeV1 { seed: Box::new(seed) }
            }
            TokenizerScheduleV1::Move(mut movement) => {
                movement.signature =
                    self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no staged signing key")?;
                movement.check()?;
                PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }
            }
            TokenizerScheduleV1::Seed(_) => return Err("a TIR tokenizer scheduler never emits a legacy seed".into()),
        };
        Self::fraud_filer_stage_queue_v1(book, claim, object, &progress, current_daa, court_pending, court_due)?;
        Ok((None, false))
    }

    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_selected_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        progress: &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<
        Option<(
            Vec<kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1>,
            crate::palw_memory_ledger::PalwMemoryReservationV1,
        )>,
        String,
    > {
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{
                PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
            },
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_selected_event_source_v1::{PalwSelectedEventBindingV1, PalwSelectedEventCoordinateV1},
        };
        use selected_event_scheduler::SelectedEventScheduleV1;
        let bytes = progress
            .bundle
            .as_ref()
            .ok_or("selected source has no prepaid owner bundle")?
            .plan
            .terms
            .plan
            .resident_ram_bytes
            .checked_mul(2)
            .ok_or("selected snapshot read memory overflow")?;
        let guard = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-selected-source-read",
                    class_id: target.class_id,
                    job: claim,
                },
                bytes,
            )
            .map_err(|e| e.to_string())?;
        let selected = session
            .palw_staged_selected_events_v1((claim, progress.reserver))?
            .ok_or("prospective original selected source is unavailable")?;
        // Same tolerance as the Flat source: newer accepted tip, identical literal identity/clocks.
        if selected.now_daa < progress.now_daa
            || (selected.session.claim, selected.session.owner, selected.session.reserved_daa, selected.session.bundle_id)
                != (claim, progress.reserver, progress.reserved_daa, progress.bundle.as_ref().expect("checked").bundle_id)
            || (selected.session.respond_until_daa, selected.session.collect_until_daa)
                != (
                    progress.bundle.as_ref().expect("checked").plan.preseed_source_until_v1(
                        kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint,
                        kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
                    )?,
                    progress.terms.collect_until_daa,
                )
        {
            return Err("selected source changes original owner/bundle/child clock or accepted snapshot".into());
        }
        if selected.context.as_ref().is_some_and(|context| context.binding.as_ref() != binding) {
            return Err("selected source context differs from original immutable binding".into());
        }
        if selected.session.minimum_response_span_daa
            != kaspa_consensus_core::palw_selected_event_source_v1::selected_event_response_span_v1(
                selected.session.custodians.len() as u64,
                kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
            )?
        {
            return Err("selected source changes immutable response preparation/inclusion span".into());
        }
        let bootstrap = PalwSelectedEventCoordinateV1 { row: 0, tile: 0 };
        if book.cases.get(&claim).and_then(|case| case.constraint_plan).is_none()
            && let Some(event) = selected.session.requests.get(&bootstrap).and_then(|request| request.event.as_ref())
        {
            let context = selected.context.as_ref().ok_or("selected bootstrap lost original accepted context")?;
            let case = book.cases.get(&claim).ok_or("selected pursuit disappeared")?;
            let own = match &case.verdict {
                PalwFraudFilerVerdictV1::Mismatch(run) => run.own_output_ids.as_slice(),
                _ => &[],
            };
            let replayed =
                case.arithmetic_matched || matches!(&case.verdict,PalwFraudFilerVerdictV1::Mismatch(run) if run.fp_job.is_some());
            let root = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(&context.job)
                .ok_or("selected bootstrap loses immutable Constraint table")?;
            let table = crate::palw_public_token_table::public_table_v1(&root, &context.job.tokenizer_id)
                .ok_or("selected exact table unavailable")?;
            let nominee = constraint_scheduler::nominate_authenticated_ids_v1(
                target,
                &context.job,
                binding,
                &event.generated_token_ids,
                own,
                table,
                &crate::palw_memory_ledger::host_ledger_v1(),
            )?
            .or_else(
                    || {
                        (replayed && !event.generated_token_ids.is_empty()).then(|| constraint::TiledPlanV1 {
                            position: 0,
                            committed: event.generated_token_ids[0],
                            witness: None,
                            beat_lane: None,
                        })
                    },
                );
            book.cases.get_mut(&claim).ok_or("selected pursuit disappeared")?.constraint_plan = nominee;
        }
        let plan = book.cases.get(&claim).and_then(|case| case.constraint_plan);
        let mut coordinates = vec![bootstrap];
        if let Some(plan) = plan {
            for (row, tile) in plan.selected_tiles_v1() {
                let coordinate = PalwSelectedEventCoordinateV1 { row, tile };
                if !coordinates.contains(&coordinate) {
                    coordinates.push(coordinate)
                }
            }
        }
        let action = match selected_event_scheduler::request_v1(&selected.session, &coordinates, selected.now_daa)? {
            SelectedEventScheduleV1::Wait => return Ok(None),
            SelectedEventScheduleV1::Ready => {
                let Some(plan) = plan else { return Ok(None) };
                let context = selected.context.as_ref().ok_or("selected ready source has no accepted original context")?;
                let selected_coordinates = plan
                    .selected_tiles_v1()
                    .into_iter()
                    .map(|(row, tile)| PalwSelectedEventCoordinateV1 { row, tile })
                    .collect::<Vec<_>>();
                let events = selected_coordinates
                    .iter()
                    .map(|coordinate| {
                        selected
                            .session
                            .requests
                            .get(coordinate)
                            .and_then(|request| request.event.clone())
                            .map(|event| event.into_disclosure_v1(*context.binding.clone()))
                            .ok_or("selected source lost authenticated accepted material".to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                return Ok(Some((events, guard)));
            }
            SelectedEventScheduleV1::Demand(demand) => PalwConstraintStreamActionV1::SelectedEventDemand {
                binding: PalwSelectedEventBindingV1::from_binding_v1(binding)?,
                demand,
            },
            SelectedEventScheduleV1::Collect { coordinate, demand_id } => {
                PalwConstraintStreamActionV1::SelectedEventCollect { coordinate, demand_id }
            }
        };
        let demand_due = matches!(action, PalwConstraintStreamActionV1::SelectedEventDemand { .. })
            .then(|| selected.session.respond_until_daa.saturating_sub(selected.session.minimum_response_span_daa).saturating_sub(1));
        let mut movement = PalwConstraintStreamMoveV1 {
            version: 1,
            claim,
            reserver: progress.reserver,
            reserved_daa: progress.reserved_daa,
            seed_id: selected.session.bundle_id,
            phase: PalwConstraintStreamPhaseV1::PrefixCourt,
            action,
            signature: Vec::new(),
        };
        movement.signature =
            self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no selected owner signing key")?;
        movement.check()?;
        Self::fraud_filer_stage_queue_v1(
            book,
            claim,
            PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) },
            progress,
            progress.now_daa,
            court_pending,
            court_due,
        )?;
        if let Some(due) = demand_due {
            court_due.entry((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)).and_modify(|held| *held = (*held).min(due));
        }
        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_bundle_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: Option<&PalwStepBindingV2>,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        tir: Option<&FilerTirContextV1>,
        progress: kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        maximum: u64,
        file_by: Option<u64>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use bundle_scheduler::BundleScheduleV1;
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{
                PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
            },
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_staged_bundle_v1::PalwStagedFamilyV1,
        };
        book.cases.get_mut(&claim).ok_or("bundle pursuit disappeared")?.staged_bundle_pending = true;
        let path = || {
            self.backends()
                .held_tokenizer_path_or_chain_v1(target.class_id, target.artifact_root, |id| {
                    if self.config.chain_classes { session.palw_registered_class_carriage_v1(id) } else { None }
                })
                .ok_or("outside verifier does not hold immutable registered tokenizer operand".to_string())
        };
        let quote = if progress.seed_id.is_none() {
            let required = progress
                .bundle
                .as_ref()
                .ok_or("unopened claim has no required-family contract")?
                .plan
                .stages
                .iter()
                .any(|stage| stage.family == PalwStagedFamilyV1::Tokenizer);
            let cap = if required { Some(std::fs::metadata(path()?).map_err(|e| e.to_string())?.len()) } else { None };
            session.palw_staged_bundle_plan_v1(claim, cap)?
        } else {
            None
        };
        let decision =
            bundle_scheduler::request_v1(&progress, quote.as_ref())?.ok_or("claim has no prospective required-family bundle")?;
        let mut movement = match decision {
            BundleScheduleV1::Complete => {
                let case = book.cases.get_mut(&claim).ok_or("bundle pursuit disappeared")?;
                case.staged_bundle_pending = false;
                for stage in &progress.bundle.as_ref().ok_or("completed bundle disappeared")?.plan.stages {
                    match stage.family {
                        PalwStagedFamilyV1::PublicPrefix => case.prefix_complete = true,
                        PalwStagedFamilyV1::Tokenizer => case.rendering_done = true,
                        PalwStagedFamilyV1::Constraint => case.constraint_done = true,
                    }
                }
                return Ok((None, true));
            }
            BundleScheduleV1::Fault(reference) => {
                let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                    claim,
                    target.executor_bond,
                    kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StagedCourt { reference },
                    None,
                    maximum,
                )
                .map_err(|e| e.to_string())?;
                let filing = PalwConvictionFilingV1::of_offence(built.object, claim, file_by, PalwFilingOriginV1::Replay)
                    .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                    .ok_or("bundle inventory reporter key mismatch")?;
                return Ok((Some(filing), false));
            }
            BundleScheduleV1::Family { family, progress: child } => {
                if let Some(tir) = tir {
                    let result = match family {
                        PalwStagedFamilyV1::Constraint => {
                            self.fraud_filer_tir_constraint_stage_v1(
                                session,
                                book,
                                claim,
                                target,
                                job,
                                tir,
                                child,
                                maximum,
                                file_by,
                                court_pending,
                                court_due,
                            )
                            .await?
                        }
                        PalwStagedFamilyV1::Tokenizer => {
                            self.fraud_filer_tir_tokenizer_stage_v1(
                                session,
                                book,
                                claim,
                                target,
                                job,
                                tir,
                                child,
                                maximum,
                                file_by,
                                court_pending,
                                court_due,
                            )
                            .await?
                        }
                        PalwStagedFamilyV1::PublicPrefix => {
                            self.fraud_filer_tir_prefix_stage_v1(session, book, claim, target, job, tir, child, maximum, file_by, court_pending, court_due)
                                .await?
                        }
                    };
                    return Ok((result.0, false));
                }
                let binding = binding.ok_or("a legacy family stage needs its resolved original binding")?;
                let result = match family {
                    PalwStagedFamilyV1::PublicPrefix => {
                        self.fraud_filer_prefix_stage_v1(
                            session,
                            book,
                            claim,
                            target,
                            binding,
                            job,
                            child,
                            maximum,
                            file_by,
                            court_pending,
                            court_due,
                        )
                        .await?
                    }
                    PalwStagedFamilyV1::Tokenizer => {
                        self.fraud_filer_tokenizer_stage_v1(
                            session,
                            book,
                            claim,
                            target,
                            binding,
                            job,
                            child,
                            maximum,
                            file_by,
                            progress.now_daa,
                            court_pending,
                            court_due,
                        )
                        .await?
                    }
                    PalwStagedFamilyV1::Constraint => {
                        // A Float32 class's source is its v2 event tree, whatever its scheme field says
                        // (greedy only, behind `palw_staged_float_constraint_v1`; the native quote refused the rest).
                        let float_lane = binding.shape_profile.lane == kaspa_consensus_core::palw_step::PalwStepLaneV1::Float32;
                        let flat_scheme = !float_lane
                            && binding.shape_profile.logits_scheme_id
                                == kaspa_consensus_core::palw_step_refute::flat_logits_scheme_id_v1();
                        let float = if child.cursor.is_none() && float_lane {
                            let Some(float) = self
                                .fraud_filer_float_stage_v1(
                                    session,
                                    book,
                                    claim,
                                    target,
                                    binding,
                                    job,
                                    &child,
                                    court_pending,
                                    court_due,
                                )
                                .await?
                            else {
                                return Ok((None, false));
                            };
                            Some(float)
                        } else {
                            None
                        };
                        let flat = if child.cursor.is_none() && flat_scheme {
                            let Some(flat) = self
                                .fraud_filer_flat_stage_v1(
                                    session,
                                    book,
                                    claim,
                                    target,
                                    binding,
                                    job,
                                    &child,
                                    court_pending,
                                    court_due,
                                )
                                .await?
                            else {
                                return Ok((None, false));
                            };
                            Some(flat)
                        } else {
                            None
                        };
                        let selected = if child.cursor.is_none() && !flat_scheme && !float_lane {
                            let Some(selected) = self
                                .fraud_filer_selected_stage_v1(session, book, claim, target, binding, &child, court_pending, court_due)
                                .await?
                            else {
                                return Ok((None, false));
                            };
                            Some(selected)
                        } else {
                            None
                        };
                        let seed =
                            if child.cursor.is_some() { session.palw_staged_owned_seed_v1((claim, progress.reserver))? } else { None };
                        let reopened_flat = match seed.as_ref().and_then(|seed| seed.auxiliary.as_ref()) {
                            Some(kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FlatConstraint {
                                source_id, answerer,
                            }) => Some((
                                constraint_scheduler::StreamedPinV1::Flat(
                                    session.palw_staged_flat_pin_v1((claim, progress.reserver), *source_id, *answerer)?
                                        .ok_or("accepted Flat semantic seed lost its native EOF pin")?,
                                ),
                                *source_id, *answerer,
                            )),
                            Some(kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwAuxiliaryPublicCourtSeedV1::FloatConstraint {
                                source_id, answerer,
                            }) => Some((
                                constraint_scheduler::StreamedPinV1::Float(
                                    session.palw_staged_float_pin_v1((claim, progress.reserver), *source_id, *answerer)?
                                        .ok_or("accepted Float32 semantic seed lost its authenticated row pin")?,
                                ),
                                *source_id, *answerer,
                            )),
                            _ => None,
                        };
                        let flat_original = if flat_scheme || float_lane {
                            Some(
                                session
                                    .palw_staged_original_commitment_v1(claim)?
                                    .ok_or("Flat actor lost its native original commitment")?,
                            )
                        } else {
                            None
                        };
                        self.fraud_filer_constraint_stage_v1(
                            book,
                            claim,
                            target,
                            binding,
                            job,
                            selected.as_ref().map(|(events, _)| events.clone()),
                            flat.as_ref()
                                .map(|(pin, source, signer, _)| (constraint_scheduler::StreamedPinV1::Flat(pin.clone()), *source, *signer))
                                .or(float.as_ref().map(|(pin, source, signer, _)| {
                                    (constraint_scheduler::StreamedPinV1::Float(pin.clone()), *source, *signer)
                                }))
                                .or(reopened_flat),
                            flat_original,
                            seed,
                            child,
                            maximum,
                            file_by,
                            court_pending,
                            court_due,
                        )
                        .await?
                    }
                };
                // A child's honest result is an activation prerequisite, not bundle completion.
                return Ok((result.0, false));
            }
            BundleScheduleV1::Control(movement) => movement,
            BundleScheduleV1::Inventory { offset, maximum: cap } => {
                use std::io::{Read, Seek, SeekFrom};
                let path = path()?;
                let reservation = crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-bundle-inventory",
                            class_id: target.class_id,
                            job: claim,
                        },
                        131_072,
                    )
                    .map_err(|e| e.to_string())?;
                let action = tokio::task::spawn_blocking(move || -> Result<PalwConstraintStreamActionV1, String> {
                    let _reservation = reservation;
                    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                    let length = file.metadata().map_err(|e| e.to_string())?.len();
                    if length > cap || offset > length {
                        return Err("held tokenizer exceeds immutable actual operand cap/offset".into());
                    }
                    if offset == length {
                        return Ok(PalwConstraintStreamActionV1::InventoryFinish);
                    }
                    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
                    let mut bytes = vec![0u8; (length - offset).min(65_536) as usize];
                    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
                    Ok(PalwConstraintStreamActionV1::InventoryPart { offset, bytes })
                })
                .await
                .map_err(|e| format!("bundle inventory worker failed: {e}"))??;
                PalwConstraintStreamMoveV1 {
                    version: 1,
                    claim,
                    reserver: progress.reserver,
                    reserved_daa: progress.reserved_daa,
                    seed_id: progress.bundle.as_ref().ok_or("inventory bundle disappeared")?.bundle_id,
                    phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                    action,
                    signature: Vec::new(),
                }
            }
        };
        movement.signature =
            self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no bundle owner signing key")?;
        movement.check()?;
        Self::fraud_filer_stage_queue_v1(
            book,
            claim,
            PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) },
            &progress,
            progress.now_daa,
            court_pending,
            court_due,
        )?;
        Ok((None, false))
    }

    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_paid_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        owner: PalwBondKeyV2,
        target: Option<&kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1>,
        progress: kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedCursorV1;
        if !palw_fraud_filer_paid_progress_v1(&progress, claim, owner) {
            return Err("retained paid authority lacks original accepted owner/seed/clock".into());
        }
        let target = target.ok_or("paid semantic target is unavailable")?;
        // A TIR claim has no legacy binding: its stages run over the natively derived view
        // (behind `palw_staged_tir_route_v1`; every stage of a TIR claim is bundled).
        if let Some((view, header)) = session.palw_staged_tir_view_v1(claim, self.class_step_ladder(target.class_id))? {
            let original =
                session.palw_staged_original_commitment_v1(claim)?.ok_or("TIR paid stage lost its native original commitment")?;
            let bundle = progress.bundle.as_ref().ok_or("a TIR claim's stages are bundled")?;
            let file_by = Some(bundle.plan.terms.collect_until_daa.saturating_sub(2));
            let maximum = self
                .config
                .court
                .max_close_bytes()
                .min(kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
            let tir = FilerTirContextV1 { view, header };
            return self
                .fraud_filer_bundle_stage_v1(
                    session,
                    book,
                    claim,
                    target,
                    None,
                    &original.job,
                    Some(&tir),
                    progress,
                    maximum,
                    file_by,
                    court_pending,
                    court_due,
                )
                .await;
        }
        let case = book.cases.get(&claim).ok_or("paid pursuit disappeared")?;
        let held = case.binding.clone().zip(case.public_fp_job_v1().cloned());
        let (binding, job) = match held {
            Some(context) => context,
            None => session
                .palw_staged_owned_context_v1((claim, owner))?
                .ok_or("paid semantic binding/job needs authenticated public seed context")?,
        };
        let maximum = self
            .config
            .court
            .max_close_bytes()
            .min(kaspa_consensus_core::palw_offence_attribution_v1::PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES);
        let file_by = Some(progress.terms.collect_until_daa.saturating_sub(2));
        if progress.bundle.is_some() {
            let file_by = Some(progress.bundle.as_ref().expect("present").plan.terms.collect_until_daa.saturating_sub(2));
            return self
                .fraud_filer_bundle_stage_v1(
                    session,
                    book,
                    claim,
                    target,
                    Some(&binding),
                    &job,
                    None,
                    progress,
                    maximum,
                    file_by,
                    court_pending,
                    court_due,
                )
                .await;
        }
        let now = progress.now_daa;
        let prefix =
            matches!(progress.cursor, Some(PalwStagedOwnedCursorV1::PublicPrefix { .. } | PalwStagedOwnedCursorV1::Prefix { .. }));
        let constraint = matches!(progress.cursor, Some(PalwStagedOwnedCursorV1::Constraint { .. }));
        let (filing, honest) = match progress.cursor.as_ref().ok_or("paid stage has no accepted cursor")? {
            // A TIR prefix stage is always bundled and was dispatched through the TIR branch above.
            PalwStagedOwnedCursorV1::TirPrefix { .. } => return Err("a TIR prefix stage runs only inside its TIR bundle".into()),
            PalwStagedOwnedCursorV1::PublicPrefix { .. } | PalwStagedOwnedCursorV1::Prefix { .. } => {
                self.fraud_filer_prefix_stage_v1(
                    session,
                    book,
                    claim,
                    target,
                    &binding,
                    &job,
                    progress,
                    maximum,
                    file_by,
                    court_pending,
                    court_due,
                )
                .await?
            }
            PalwStagedOwnedCursorV1::Tokenizer { .. } => {
                self.fraud_filer_tokenizer_stage_v1(
                    session,
                    book,
                    claim,
                    target,
                    &binding,
                    &job,
                    progress,
                    maximum,
                    file_by,
                    now,
                    court_pending,
                    court_due,
                )
                .await?
            }
            PalwStagedOwnedCursorV1::Constraint { .. } => {
                let seed =
                    session.palw_staged_owned_seed_v1((claim, owner))?.ok_or("paid constraint original signed seed is unavailable")?;
                self.fraud_filer_constraint_stage_v1(
                    book,
                    claim,
                    target,
                    &binding,
                    &job,
                    None,
                    None,
                    None,
                    Some(seed),
                    progress,
                    maximum,
                    file_by,
                    court_pending,
                    court_due,
                )
                .await?
            }
        };
        if honest {
            // One typed honest result does not erase the sibling DA outcome or establish
            // completion of every required family in the immutable active plan.
            debug!("[{PALW_PANEL}] claim {claim}: retained typed court finished honest; other public obligations are unchanged");
            let case = book.cases.get_mut(&claim).ok_or("paid pursuit disappeared")?;
            if prefix {
                case.prefix_complete = true;
            } else if constraint {
                case.constraint_done = true;
            } else {
                case.rendering_done = true;
            }
        }
        Ok((filing, honest))
    }

    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_prefix_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        progress: kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        maximum: u64,
        file_by: Option<u64>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use super::public_prefix::PrefixScheduleV1;
        use kaspa_consensus_core::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1;
        use kaspa_consensus_core::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1;
        use kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedCursorV1;
        let maximum_leaves = self.class_step_ladder(target.class_id);
        let copy_bytes = (borsh::object_length(&(binding, job, &target.registered_semantics)).map_err(|e| e.to_string())? as u64)
            .checked_mul(8)
            .and_then(|n| n.checked_add(131_072))
            .ok_or("public prefix worker copy bound overflow")?;
        let copies = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-prefix-worker-copies",
                    class_id: target.class_id,
                    job: claim,
                },
                copy_bytes,
            )
            .map_err(|e| e.to_string())?;
        let needs_parameters = matches!(
            &progress.cursor,
            Some(PalwStagedOwnedCursorV1::PublicPrefix { cursor: PalwPrefixFamilyCursorV1::HeadParameters { .. }, .. })
        );
        let holding = if needs_parameters {
            let need = self
                .backends()
                .role_memory_need_or_chain_v1(target.class_id, target.artifact_root, super::FULL_SEAT_V1, |id| {
                    self.chain_carriage_v1(session, id)
                })
                .ok_or("public prefix own artifact resource profile unavailable")?;
            let ticket = self.reserve_replay_v1("public-prefix-own-parameters", &need, target.class_id, claim)?;
            Some((self.resolve_backend(session, target.class_id, target.artifact_root)?, ticket))
        } else {
            None
        };
        let executor_bond = target.executor_bond;
        let current_daa = progress.now_daa;
        let worker_progress = progress.clone();
        let page_session = session.clone();
        let history_flow_context = self.flow_context.clone();
        let page_directory = self.config.retention_dir.join("public-prefix-owned");
        let history_directory = page_directory.clone();
        let target = target.clone();
        let binding = binding.clone();
        let job = job.clone();
        let decision = tokio::task::spawn_blocking(move || {
            let _copies = copies;
            let backend = holding.as_ref().map(|(backend, _ticket)| backend.as_ref());
            super::public_prefix::request_with_pages_v1(
                &target,
                &binding,
                &job,
                &worker_progress,
                maximum_leaves,
                backend,
                &page_directory,
                |opening| {
                    page_session.palw_prefix_page_write_v1(
                        worker_progress.snapshot_block,
                        (claim, worker_progress.reserver),
                        worker_progress.seed_id.ok_or("public prefix page write lacks accepted seed")?,
                        opening,
                    )
                },
                || {
                    page_session.with_consensus_api_v1(|consensus| {
                        super::public_prefix::recover_pages_with_public_pull_v1(
                            consensus,
                            &target,
                            &binding,
                            &job,
                            &worker_progress,
                            maximum_leaves,
                            &page_directory,
                            |request| history_flow_context.palw_prefix_history().poll_v1(request),
                        )
                    })
                },
            )
        })
        .await
        .map_err(|e| format!("public prefix blocking worker failed: {e}"))??;
        let object = match decision {
            PrefixScheduleV1::Wait => {
                // Every local, archived and peer source of a rooted link may be gone. Only then
                // is the paid native backwards recovery opened; an already opened accepted
                // session is always continued (restart loses the local hint, not the session).
                if let (Some(seed), Some(_)) = (progress.seed_id, progress.bundle.as_ref()) {
                    let native_needed = super::public_prefix::needs_native_recovery_v1(&history_directory, seed);
                    return self.fraud_filer_history_recovery_v1(
                        session,
                        book,
                        claim,
                        &progress,
                        seed,
                        native_needed,
                        court_pending,
                        court_due,
                    );
                }
                return Ok((None, false));
            }
            PrefixScheduleV1::Finished(reference) => {
                court_pending.retain(|(held, round, responder, _)| {
                    (*held, *round, *responder) != (claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)
                });
                book.cases.get_mut(&claim).ok_or("pursuit disappeared")?.staged_tokenizer_sent = None;
                let Some(reference) = reference else { return Ok((None, true)) };
                let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                    claim,
                    executor_bond,
                    kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StagedCourt { reference },
                    None,
                    maximum,
                )
                .map_err(|e| e.to_string())?;
                let filing = PalwConvictionFilingV1::of_offence(built.object, claim, file_by, PalwFilingOriginV1::Replay)
                    .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                    .ok_or("public prefix staged reporter key mismatch")?;
                return Ok((Some(filing), false));
            }
            PrefixScheduleV1::Seed(mut seed) => {
                seed.signature =
                    self.sign(&seed.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no staged signing key")?;
                seed.check(100_000)?;
                PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed: Box::new(seed) }
            }
            PrefixScheduleV1::Move(mut movement) => {
                movement.signature =
                    self.sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no staged signing key")?;
                movement.check()?;
                PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }
            }
        };
        Self::fraud_filer_stage_queue_v1(book, claim, object, &progress, current_daa, court_pending, court_due)?;
        Ok((None, false))
    }

    /// The outside owner's half of paid public history recovery: open the native backwards
    /// traversal only when the rooted page replay found no local, archived or peer copy, then
    /// follow the accepted session's own Demand/Collect clocks. Recovered records are archived
    /// natively, so the ordinary page replay reads them on a later poll. A native default is
    /// the exact once-only DA charge, never a computation conviction.
    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_history_recovery_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        progress: &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        seed: Hash64,
        native_needed: bool,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use super::history_recovery_actor::{HistoryOwnerDecisionV1, owner_request_v1};
        use kaspa_consensus_core::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1;
        let Some(history) = session.palw_staged_history_recovery_v1((claim, progress.reserver), seed)? else {
            return Ok((None, false));
        };
        // A newer accepted tip than the owner's cursor read is fine: the session's own native
        // clocks decide. An older one is not decided against.
        if history.now_daa < progress.now_daa {
            return Ok((None, false));
        }
        if history.session.is_none() && !native_needed {
            return Ok((None, false));
        }
        match owner_request_v1(&history)? {
            HistoryOwnerDecisionV1::Wait | HistoryOwnerDecisionV1::Recovered => Ok((None, false)),
            HistoryOwnerDecisionV1::Default => {
                debug!(
                    "[{PALW_PANEL}] claim {claim}: native public history default is charged once; this stage's semantic \
                     continuation has no remaining rooted record source"
                );
                Ok((None, false))
            }
            HistoryOwnerDecisionV1::Move { mut movement, until_daa } => {
                movement.signature = self
                    .sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1)
                    .ok_or("no history recovery owner signing key")?;
                movement.check()?;
                Self::fraud_filer_stage_queue_v1(
                    book,
                    claim,
                    PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) },
                    progress,
                    progress.now_daa,
                    court_pending,
                    court_due,
                )?;
                // The accepted demand/collection clock, not the child's later stream horizon.
                court_due
                    .entry((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false))
                    .and_modify(|held| *held = (*held).min(until_daa.saturating_sub(1)));
                Ok((None, false))
            }
        }
    }

    /// A rooted prospective owner court is progressed using only this verifier's holding.
    /// Its own one guarded move uses the existing signed, charged priority carrier queue.
    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_tokenizer_stage_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        target: &kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
        progress: kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        maximum: u64,
        file_by: Option<u64>,
        _current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(Option<PalwConvictionFilingV1>, bool), String> {
        use kaspa_consensus_core::palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1;
        use tokenizer_scheduler::{TokenizerScheduleV1, TokenizerSchedulerV1};
        if progress.claim != claim || progress.reserver == target.executor_bond {
            return Err("snapshot is not this outside verifier's accepted claim/clock".into());
        }
        // Clock refusal precedes expensive file parsing/replay and is never called Honest.
        let current_daa = progress.now_daa;
        let deadline = if progress.cursor.is_none() { progress.terms.open_until_daa } else { progress.terms.collect_until_daa };
        if progress.now_daa >= deadline {
            return Err("prospective owned court clock is closed".into());
        }
        let worker = book
            .cases
            .get_mut(&claim)
            .ok_or("pursuit disappeared")?
            .staged_tokenizer
            .get_or_insert_with(|| Arc::new(std::sync::Mutex::new(None)))
            .clone();
        let copy_bytes = (borsh::object_length(&(binding, job)).map_err(|e| e.to_string())? as u64)
            .checked_mul(8)
            .and_then(|n| n.checked_add(16_384))
            .ok_or("staged public operand copy bound overflow")?;
        let copies = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-tokenizer-worker-copies",
                    class_id: target.class_id,
                    job: claim,
                },
                copy_bytes,
            )
            .map_err(|e| e.to_string())?;
        let backends = self.backends();
        let chain_classes = self.config.chain_classes;
        let held_session = session.clone();
        let executor_bond = target.executor_bond;
        let target = target.clone();
        let binding = binding.clone();
        let job = job.clone();
        let worker_progress = progress.clone();
        let original_seed = if matches!(
            progress.cursor,
            Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedCursorV1::Tokenizer { .. })
        ) {
            session.palw_staged_owned_seed_v1((claim, progress.reserver))?
        } else {
            None
        };
        let decision = tokio::task::spawn_blocking(move || -> Result<TokenizerScheduleV1, String> {
            let _copies = copies;
            let mut worker = worker.lock().map_err(|_| "tokenizer worker lock poisoned")?;
            let build = || -> Result<TokenizerSchedulerV1, String> {
                let path = backends
                    .held_tokenizer_path_or_chain_v1(target.class_id, target.artifact_root, |id| {
                        if chain_classes { held_session.palw_registered_class_carriage_v1(id) } else { None }
                    })
                    .ok_or("outside verifier does not hold registered tokenizer")?;
                let table = tokenizer_scheduler::registered_table_v1(&target, &binding, &job)?;
                TokenizerSchedulerV1::open_with_original_v1(
                    &path,
                    &target,
                    &binding,
                    &job,
                    table,
                    &worker_progress,
                    &crate::palw_memory_ledger::host_ledger_v1(),
                    original_seed.as_ref(),
                )
            };
            if worker.is_none() {
                *worker = Some(build()?);
            }
            match worker.as_mut().expect("built").poll(&worker_progress) {
                Ok(decision) => Ok(decision),
                Err(why) if why.contains("cursor") || why.contains("branch") => {
                    // Discard the old speculative parser and its guarded packet BEFORE reserving
                    // a fresh reconstruction. A deep/unfunded replay remains explicit Pending.
                    *worker = None;
                    *worker = Some(build()?);
                    worker.as_mut().expect("rebuilt").poll(&worker_progress)
                }
                Err(why) => Err(why),
            }
        })
        .await
        .map_err(|e| format!("tokenizer blocking worker failed: {e}"))??;
        match decision {
            TokenizerScheduleV1::Finished(reference) => {
                court_pending.retain(|(held, round, responder, _)| {
                    (*held, *round, *responder) != (claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)
                });
                book.cases.get_mut(&claim).expect("pursued").staged_tokenizer_sent = None;
                let Some(reference) = reference else {
                    return Ok((None, true));
                };
                let built = kaspa_consensus_core::palw_replay_refute_v1::palw_replay_executor_refuted_object_with_budget_v2(
                    claim,
                    executor_bond,
                    kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::StagedCourt { reference },
                    None,
                    maximum,
                )
                .map_err(|e| e.to_string())?;
                let filing = PalwConvictionFilingV1::of_offence(built.object, claim, file_by, PalwFilingOriginV1::Replay)
                    .filter(|f| (f.offence_key, f.evidence_id) == (built.offence_id, built.evidence_id))
                    .ok_or("staged reporter key mismatch")?;
                Ok((Some(filing), false))
            }
            decision => {
                let object = match decision {
                    TokenizerScheduleV1::Seed(mut seed) => {
                        seed.signature =
                            self.sign(&seed.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no staged signing key")?;
                        seed.check(100000)?;
                        PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed: Box::new(seed) }
                    }
                    TokenizerScheduleV1::NativeSeed(mut seed) => {
                        seed.signature =
                            self.sign(&seed.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).ok_or("no staged signing key")?;
                        seed.check(90_000)?;
                        PalwConsensusObjectV2::PublicCourtStreamOpenedNativeV1 { seed: Box::new(seed) }
                    }
                    TokenizerScheduleV1::Move(mut movement) => {
                        movement.signature = self
                            .sign(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1)
                            .ok_or("no staged signing key")?;
                        movement.check()?;
                        PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) }
                    }
                    _ => unreachable!(),
                };
                Self::fraud_filer_stage_queue_v1(book, claim, object, &progress, current_daa, court_pending, court_due)?;
                Ok((None, false))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_stage_queue_v1(
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        object: PalwConsensusObjectV2,
        progress: &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) -> Result<(), String> {
        match &object {
            PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement }
                if (movement.claim, movement.reserver, movement.reserved_daa) == (claim, progress.reserver, progress.reserved_daa)
                    && Self::fraud_filer_bundle_control_v1(progress, movement) => {}
            PalwConsensusObjectV2::PublicCourtStreamOpenedV1 { seed }
                if progress.seed_id.is_none()
                    && progress.cursor.is_none()
                    && (seed.claim, seed.reserver, seed.reserved_daa) == (claim, progress.reserver, progress.reserved_daa) => {}
            PalwConsensusObjectV2::PublicCourtStreamOpenedNativeV1 { seed }
                if progress.seed_id.is_none()
                    && progress.cursor.is_none()
                    && (seed.claim, seed.reserver, seed.reserved_daa) == (claim, progress.reserver, progress.reserved_daa) => {}
            PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement }
                if progress.cursor.is_some()
                    && !matches!(
                        movement.action,
                        kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::BundleOpen { .. }
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::ActivateStage { .. }
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::InventoryPart { .. }
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::InventoryFinish
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::SelectedEventDemand {..}
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::SelectedEventAnswer (_)
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::SelectedEventCollect {..}
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::NativeSelectedDemand {..}
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::FlatSourceOpen {..}
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::FlatSourceAnswer (_)
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::FlatSourceNominate {..}
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::FlatSourceCollect
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::FloatSourceOpen {..}
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::FloatSourceAnswer (_)
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::FloatSourceNominate {..}
                            | kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1::FloatSourceCollect
                    )
                    && progress.seed_id == Some(movement.seed_id)
                    && (movement.claim, movement.reserver, movement.reserved_daa)
                        == (claim, progress.reserver, progress.reserved_daa) => {}
            PalwConsensusObjectV2::PublicCourtStreamLeafV1 { leaf }
                if matches!(
                    progress.cursor,
                    Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedCursorV1::Constraint { .. })
                ) && progress.seed_id == Some(leaf.seed_id)
                    && (leaf.claim, leaf.reserver, leaf.reserved_daa) == (claim, progress.reserver, progress.reserved_daa) => {}
            _ => return Err("staged carrier changes accepted owner/seed or opens a replacement court".into()),
        }
        let encoded = borsh::to_vec(&object).map_err(|e| e.to_string())?;
        let digest = Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"PALW/node/staged-queue/v1")
                .hash(&encoded)
                .as_bytes()
                .try_into()
                .expect("64"),
        );
        let case = book.cases.get_mut(&claim).ok_or("pursuit disappeared")?;
        // Accepted cursors supersede stale unsent carriers; removing a queued packet is not progress.
        court_pending.retain(|(held, round, responder, o)| {
            (*held, *round, *responder) != (claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false) || *o == object
        });
        if court_pending
            .iter()
            .any(|(held, round, responder, _)| (*held, *round, *responder) == (claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false))
        {
            return Ok(());
        }
        let sends = match case.staged_tokenizer_sent {
            Some((id, at, sends)) if id == digest => {
                if current_daa.saturating_sub(at) < PALW_FRAUD_FILER_RESEND_DAA_V1 {
                    return Ok(());
                }
                if sends >= PALW_FRAUD_FILER_SENDS_V1 {
                    return Err("signed staged move did not land within its bounded retry envelope".into());
                }
                sends
            }
            _ => 0,
        };
        let due = match (&object, progress.bundle.as_ref()) {
            (PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement }, Some(bundle)) => {
                use kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1;
                match movement.action {
                    PalwConstraintStreamActionV1::InventoryPart { .. } | PalwConstraintStreamActionV1::InventoryFinish => bundle
                        .plan
                        .terms
                        .open_until_daa
                        .checked_add(
                            bundle
                                .plan
                                .inventory
                                .as_ref()
                                .ok_or("inventory queue has no frozen quoted intake")?
                                .envelope
                                .inclusion_daa,
                        )
                        .ok_or("inventory queue original deadline overflow")?
                        .saturating_sub(1),
                    PalwConstraintStreamActionV1::ActivateStage { .. } => bundle
                        .plan
                        .stages
                        .get(bundle.active_index.map_or(0, |i| i as usize + 1))
                        .ok_or("activation queue has no next required family")?
                        .terms
                        .open_until_daa
                        .saturating_sub(1),
                    PalwConstraintStreamActionV1::SelectedEventDemand { .. }
                    | PalwConstraintStreamActionV1::NativeSelectedDemand { .. }
                    | PalwConstraintStreamActionV1::FlatSourceOpen { .. }
                    | PalwConstraintStreamActionV1::FlatSourceNominate { .. }
                    | PalwConstraintStreamActionV1::FloatSourceOpen { .. }
                    | PalwConstraintStreamActionV1::FloatSourceNominate { .. } => bundle
                        .plan
                        .preseed_source_until_v1(
                            kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint,
                            kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
                        )?
                        .saturating_sub(1),
                    PalwConstraintStreamActionV1::SelectedEventCollect { .. }
                    | PalwConstraintStreamActionV1::FlatSourceCollect
                    | PalwConstraintStreamActionV1::FloatSourceCollect
                    | PalwConstraintStreamActionV1::NativeTirPrefixCollect => {
                        progress.terms.collect_until_daa.saturating_sub(1)
                    }
                    _ => palw_fraud_filer_staged_due_v1(&object, &progress.terms),
                }
            }
            _ => palw_fraud_filer_staged_due_v1(&object, &progress.terms),
        };
        court_due.insert((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false), due);
        court_pending.push((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false, object));
        case.staged_tokenizer_sent = Some((digest, current_daa, sends + 1));
        Ok(())
    }

    fn fraud_filer_bundle_control_v1(
        progress: &kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        movement: &kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1,
    ) -> bool {
        use kaspa_consensus_core::palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1;
        use kaspa_consensus_core::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1;
        if movement.phase != PalwConstraintStreamPhaseV1::PrefixCourt {
            return false;
        }
        match &movement.action {
            PalwConstraintStreamActionV1::BundleOpen { plan } => {
                progress.seed_id.is_none()
                    && progress.cursor.is_none()
                    && progress.bundle.is_some()
                    && movement.seed_id == plan.id_v1(progress.claim)
                    && plan.check_v1().is_ok()
                    && (plan.terms.accepted_daa, plan.terms.open_until_daa)
                        == (progress.terms.accepted_daa, progress.terms.open_until_daa)
            }
            PalwConstraintStreamActionV1::ActivateStage { previous_seed, next_seed } => {
                progress.bundle.as_ref().is_some_and(|bundle| {
                    bundle.pending_seed == Some(*next_seed)
                        && Some(*previous_seed) == progress.seed_id
                        && movement.seed_id == *previous_seed
                        && (bundle.active_index.is_none() || bundle_scheduler::honest_complete_v1(progress.cursor.as_ref()))
                })
            }
            PalwConstraintStreamActionV1::InventoryPart { offset, .. } => progress.bundle.as_ref().is_some_and(|bundle| {
                movement.seed_id == bundle.bundle_id
                    && bundle.active_index.is_none()
                    && bundle.inventory.is_some_and(|(accepted, _, sealed, _)| accepted == *offset && !sealed)
            }),
            PalwConstraintStreamActionV1::InventoryFinish => progress.bundle.as_ref().is_some_and(|bundle| {
                movement.seed_id == bundle.bundle_id
                    && bundle.active_index.is_none()
                    && bundle.inventory.is_some_and(|(_, _, sealed, _)| !sealed)
            }),
            // Owner-signed history Demand/Collect: the bundle frame names the paid owner, the
            // action names the active public stage. Custodian answers are never owner moves.
            PalwConstraintStreamActionV1::HistoryRecovery { stage_seed, action, .. } => {
                action.answerer_v1().is_none()
                    && progress.seed_id == Some(*stage_seed)
                    && progress.bundle.as_ref().is_some_and(|bundle| movement.seed_id == bundle.bundle_id)
            }
            PalwConstraintStreamActionV1::SelectedEventDemand { .. }
            | PalwConstraintStreamActionV1::SelectedEventCollect { .. }
            | PalwConstraintStreamActionV1::NativeSelectedDemand { .. }
            | PalwConstraintStreamActionV1::FlatSourceOpen { .. }
            | PalwConstraintStreamActionV1::FlatSourceNominate { .. }
            | PalwConstraintStreamActionV1::FlatSourceCollect
            | PalwConstraintStreamActionV1::FloatSourceOpen { .. }
            | PalwConstraintStreamActionV1::FloatSourceNominate { .. }
            | PalwConstraintStreamActionV1::FloatSourceCollect => progress.bundle.as_ref().is_some_and(|bundle| {
                movement.seed_id == bundle.bundle_id
                    && progress.cursor.is_none()
                    && bundle.plan.stages.get(bundle.active_index.map_or(0, |i| i as usize + 1)).is_some_and(|stage| {
                        stage.family == kaspa_consensus_core::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint
                            && stage.terms == progress.terms
                    })
            }),
            _ => false,
        }
    }

    /// The same native counter/deadline/standing checks serve single reads and missing lanes.
    /// A round never gets an unpaid second DA demand for a historically answered unit.
    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_reacquire_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        view: &PalwLegacyDisputeViewV1,
        bond_key: PalwBondKeyV2,
        probe: PalwLegacyProbeV1,
        current_daa: u64,
        network_domain: Hash64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
    ) {
        let Some(request) = book.cases.get_mut(&claim).and_then(|c| c.reacquire_after_v1(view, bond_key, probe, current_daa)) else {
            return;
        };
        let item = PalwFraudFilerSentV1::Reacquire { probe, session_number: request.session_number };
        self.fraud_filer_queue_v1(book, claim, item, current_daa, court_pending, court_due, || {
            let mut signed = true;
            let object =
                kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_reacquire_object_v1(network_domain, request, |m| {
                    self.sign(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).unwrap_or_else(|| {
                        signed = false;
                        Vec::new()
                    })
                });
            if signed { Ok(object) } else { Err("no signing key for reacquisition".into()) }
        });
    }

    /// **Queue one item of a case** under the filer's own key, dated now — once per [`PALW_FRAUD_FILER_RESEND_DAA_V1`] and at most
    /// [`PALW_FRAUD_FILER_SENDS_V1`] times; an item whose sends are spent, or that cannot be built, ends this node's pursuit loudly.
    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_queue_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        item: PalwFraudFilerSentV1,
        current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        build: impl FnOnce() -> Result<PalwConsensusObjectV2, String>,
    ) {
        let Some(case) = book.cases.get_mut(&claim) else { return };
        match case.may_send(item, current_daa) {
            Ok(false) => return,
            Ok(true) => {}
            Err(()) => {
                warn!(
                    "[{PALW_PANEL}] claim {claim}: the fraud filer's {item:?} was sent {PALW_FRAUD_FILER_SENDS_V1} times and never landed (LG14-A)"
                );
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(format!("{item:?} never landed")));
                return;
            }
        }
        match build() {
            Ok(object) => {
                let round = palw_fraud_filer_item_round_v1(item);
                if case.reorged_out_v1(item) {
                    warn!(
                        "[{PALW_PANEL}] claim {claim}: the fraud filer's {item:?} was carried but is no longer on the selected chain (reorg) — resending (G14 G-D02b)"
                    );
                }
                info!("[{PALW_PANEL}] claim {claim}: the fraud filer queues {item:?} (LG14-A, RFC-0014 §6–§7)");
                court_due.insert((claim, round, false), current_daa);
                court_pending.push((claim, round, false, object));
                case.note_sent(item, current_daa);
            }
            Err(why) => {
                warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot build {item:?}: {why} (LG14-A)");
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
            }
        }
    }

    /// **Lane G14-L: queue a round's lanes** — each a tag-157 demand under the filer's own key, dated now; a lane is resent only after
    /// [`PALW_FRAUD_FILER_RESEND_DAA_V1`] and at most [`PALW_FRAUD_FILER_SENDS_V1`] times (a lane whose sends are spent, or that cannot
    /// be built, ends this node's pursuit loudly, as one item does).
    #[allow(clippy::too_many_arguments)]
    fn fraud_filer_queue_lanes_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        demand: &[PalwLegacyProbeV1],
        current_daa: u64,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
        build: impl Fn(PalwLegacyProbeV1, Option<&PalwStepBindingV2>) -> Result<PalwConsensusObjectV2, String>,
    ) {
        let Some(case) = book.cases.get_mut(&claim) else { return };
        let binding = case.binding.clone();
        let mut queued = 0usize;
        let mut failed = None;
        for probe in demand {
            let PalwLegacyProbeV1::HeldNode { unit } = *probe else { continue };
            match case.lanes_sent.get(&unit) {
                Some((at, _)) if current_daa.saturating_sub(*at) < PALW_FRAUD_FILER_RESEND_DAA_V1 => continue,
                Some((_, sends)) if *sends >= PALW_FRAUD_FILER_SENDS_V1 => {
                    warn!(
                        "[{PALW_PANEL}] claim {claim}: the fraud filer's lane {unit:?} was sent {PALW_FRAUD_FILER_SENDS_V1} times and never \
                         landed (G14-L)"
                    );
                    failed = Some(format!("lane {unit:?} never landed"));
                    break;
                }
                _ => {}
            }
            match build(*probe, binding.as_ref()) {
                Ok(object) => {
                    court_due.insert((claim, PALW_FRAUD_FILER_DEMAND_ROUND_V1, false), current_daa);
                    court_pending.push((claim, PALW_FRAUD_FILER_DEMAND_ROUND_V1, false, object));
                    let sends = case.lanes_sent.get(&unit).map_or(1, |(_, sends)| sends.saturating_add(1));
                    case.lanes_sent.insert(unit, (current_daa, sends));
                    queued += 1;
                }
                Err(why) => {
                    warn!("[{PALW_PANEL}] claim {claim}: the fraud filer cannot build lane {unit:?}: {why} (G14-L)");
                    failed = Some(why);
                    break;
                }
            }
        }
        if let Some(why) = failed {
            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
            return;
        }
        if queued > 0 {
            info!("[{PALW_PANEL}] claim {claim}: the fraud filer queues {queued} lanes of one round (G14-L, k-ary descent)");
        }
    }

    fn fraud_filer_hold_missing_fp_job_v1(&self, book: &mut PalwFraudFilerBookV1, claim: Hash64) -> bool {
        let Some(case) = book.cases.get_mut(&claim) else { return false };
        if case.fp_job_pending {
            return true;
        }
        let was_pursued = case.pursued();
        let Ok(ticket) = crate::palw_memory_ledger::host_ledger_v1().reserve(
            crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                role: "public-fp-job-binding",
                class_id: case.candidate.job.class_id,
                job: claim,
            },
            4 * self.config.court.max_close_bytes(),
        ) else {
            return false;
        };
        case.fp_job_pending = true;
        case._fp_job_reservation = Some(Arc::new(ticket));
        if !was_pursued {
            book.walked_from = None;
            book.walked_to = None;
            book.walk = None;
            book.read_at = None;
        }
        true
    }

    /// Retain the authenticated public FP job under a small input ticket. In particular, canonical
    /// input proofs must be reachable before a full replay ticket or a backend ceiling context exists.
    fn fraud_filer_hold_fp_input_v1(
        &self,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        payload: input::PalwFraudFilerFpDescriptorV1,
        form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> bool {
        book.hold_fp_input_v1(claim, payload, form, &crate::palw_memory_ledger::host_ledger_v1(), self.config.court.max_close_bytes())
    }

    /// Start an attempt derived from its header, or an FP job/input authenticated against its accepted commitment and recorded pin.
    /// Run the verifier's own model under the full-seat memory reservation, retaining only its own capture. Missing public history
    /// or input waits without spending a run; a backend that cannot execute the job remains explicitly unjudged.
    /// **The native TIR entry** (behind `palw_staged_tir_route_v1`): the claim's accepted public FP
    /// job and its authenticated public input, the verifier's OWN TIR replay
    /// (`replay_tir_generated_ids_v1`) under the claim's admitted header, then the verdict. Equal
    /// tiled logits trace roots do not establish full execution or public-family correctness.
    /// Both results retain their own ids for Constraint nomination. Reservation and the staged bundle follow through
    /// the engine's prelude and the TIR bundle stages. No legacy binding is read.
    #[allow(clippy::too_many_arguments)]
    async fn fraud_filer_tir_replay_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        tir: FilerTirContextV1,
        network_domain: Hash64,
        current_daa: u64,
        public_material: &[Vec<u8>],
        replay_room: bool,
    ) {
        let Some(case) = book.cases.get(&claim) else { return };
        let candidate = case.candidate.clone();
        let retry = |book: &mut PalwFraudFilerBookV1, why: String| {
            if let Some(case) = book.cases.get_mut(&claim) {
                case.input_retry_at = current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
            }
            debug!("[{PALW_PANEL}] claim {claim}: the native TIR replay waits: {why}");
        };
        if !replay_room || book.running.is_some() {
            return retry(book, "no replay room on this host".into());
        }
        let snapshot = candidate.clone();
        let payload = match session.clone().spawn_blocking(move |c| palw_fraud_filer_fp_payload_v1(c, &snapshot, network_domain)).await {
            Ok(payload) => input::PalwFraudFilerFpDescriptorV1::Accepted(payload),
            Err(why) => return retry(book, format!("the accepted public FP job is unavailable: {why}")),
        };
        let job = payload.job().clone();
        let accepted_pin = match &payload {
            input::PalwFraudFilerFpDescriptorV1::Accepted(p) => kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&p.commitment),
            input::PalwFraudFilerFpDescriptorV1::Disclosed { job_identity, .. } => *job_identity,
        };
        if job.network_domain != network_domain
            || job.class_id != tir.view.class_id
            || kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&tir.view.job_context) != accepted_pin
        {
            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged("the accepted FP job is not the TIR header's job".into()));
            return;
        }
        let Some(artifact_root) = candidate.job.artifact_root else {
            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged("its class is gone from the registry".into()));
            return;
        };
        let backend: Arc<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1> =
            match self.resolve_backend(session, job.class_id, artifact_root) {
            Ok(backend) => Arc::from(backend),
            Err(why) => {
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(format!("its TIR class does not resolve on this host ({why})")));
                return;
            }
        };
        let prompt_form = self.class_prompt_ids_form(job.class_id);
        let prompt = match input::palw_fraud_filer_fp_descriptor_input_v1(backend.as_ref(), &payload, prompt_form, public_material) {
            Ok(input) => input.prompt_ids,
            Err(why) => return retry(book, format!("the authenticated public input is not available yet: {why}")),
        };
        let need = match self
            .backends()
            .role_memory_need_or_chain_v1(job.class_id, artifact_root, super::FULL_SEAT_V1, |id| self.chain_carriage_v1(session, id))
        {
            Some(need) => need,
            None => return retry(book, "the TIR class's resource profile is unavailable".into()),
        };
        let reservation = match self.reserve_replay_v1(PALW_FRAUD_FILER_REPLAY_ROLE_V1, &need, job.class_id, claim) {
            Ok(reservation) => reservation,
            Err(why) => return retry(book, why),
        };
        // These smaller tickets are acquired before execution and follow the retained
        // descriptor/IDs after the FullSeat reservation leaves the blocking worker.
        let (job_ticket, ids_ticket) = match palw_fraud_filer_tir_retention_v1(
            &crate::palw_memory_ledger::host_ledger_v1(),
            claim,
            &job,
            tir.view.job_context.exact_decode_tokens,
        ) {
            Ok(tickets) => tickets,
            Err(why) => return retry(book, why),
        };
        let header = tir.header.clone();
        let replay_job = job.clone();
        let replay = tokio::task::spawn_blocking(move || {
            let _reservation = reservation;
            backend.replay_tir_generated_ids_v1(&header, &replay_job, &prompt)
        })
        .await
        .map_err(|e| format!("the TIR replay worker did not finish: {e}"))
        .and_then(|r| r);
        let replay = match replay {
            Ok(replay) => replay,
            Err(why) => {
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(format!("no TIR replay on this host: {why}")));
                return;
            }
        };
        if replay.generated_ids.len() != tir.view.job_context.exact_decode_tokens as usize {
            book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged("the TIR replay changed the exact decode count".into()));
            return;
        }
        let case = book.cases.get_mut(&claim).expect("selected case");
        case.tir = Some(Arc::new(tir));
        let selection_matches = replay.full_logits_trace_root == case.tir.as_ref().expect("set").view.full_logits_trace_root;
        case.retain_tir_selection_replay_v1(job, replay, job_ticket, ids_ticket);
        info!(
            "[{PALW_PANEL}] claim {claim}: native TIR replay finished (matching logits: {selection_matches}); required public families remain pending (G14)"
        );
    }

    async fn fraud_filer_start_replay_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwFraudFilerBookV1,
        claim: Hash64,
        network_domain: Hash64,
        current_daa: u64,
        public_material: &[Vec<u8>],
        replay_room: bool,
    ) {
        let Some(case) = book.cases.get(&claim) else { return };
        let candidate = case.candidate.clone();
        let job = candidate.job.clone();
        let producer = candidate.producer;
        // A TIR required-family claim has no legacy binding: its discovery, replay and verdict are
        // native (the decode view from native state, the verifier's own TIR replay).
        if job.free_prompt
            && let Ok(Some((view, header))) = session.palw_staged_tir_view_v1(claim, self.class_step_ladder(job.class_id))
        {
            self.fraud_filer_tir_replay_v1(
                session,
                book,
                claim,
                FilerTirContextV1 { view, header },
                network_domain,
                current_daa,
                public_material,
                replay_room,
            )
            .await;
            return;
        }
        let unjudged = |book: &mut PalwFraudFilerBookV1, why: String| {
            debug!("[{PALW_PANEL}] claim {claim}: the fraud filer does not judge it — {why}");
            if book.cases.get(&claim).is_some_and(|case| case.registered_preflight) {
                let was_pursued = book.cases[&claim].pursued();
                book.cases.get_mut(&claim).expect("selected case").preserve_semantic_refusal();
                if !was_pursued {
                    book.walked_from = None;
                    book.walked_to = None;
                    book.walk = None;
                    book.read_at = None;
                }
            } else {
                book.settle(&claim, PalwFraudFilerVerdictV1::Unjudged(why));
            }
        };
        let metadata_only = !replay_room || book.running.is_some() || book.held_runs() >= PALW_FRAUD_FILER_HELD_RUNS_V1;
        let fp_payload = if job.free_prompt {
            let snapshot = candidate.clone();
            let payload = if let Some(input) = book.cases.get(&claim).and_then(|c| c.fp_bootstrap.as_ref()) {
                Ok(input.payload.as_ref().clone())
            } else if let Some(job) = book.cases.get(&claim).and_then(|c| c.fp_descriptor.as_ref()) {
                Ok(job.as_ref().clone())
            } else if let Some((public, pin)) = book
                .cases
                .get(&claim)
                .and_then(|case| case.deferred_public.as_ref().zip(case.fp_expected_pin.filter(|pin| *pin != Hash64::default())))
            {
                Ok(input::PalwFraudFilerFpDescriptorV1::Disclosed {
                    job: public.job.as_ref().clone(),
                    execution_root: job.execution_root,
                    job_identity: pin,
                })
            } else {
                session
                    .clone()
                    .spawn_blocking(move |c| {
                        if metadata_only {
                            palw_fraud_filer_fp_metadata_v1(c, &snapshot, network_domain)
                        } else {
                            palw_fraud_filer_fp_payload_v1(c, &snapshot, network_domain)
                        }
                    })
                    .await
                    .map(input::PalwFraudFilerFpDescriptorV1::Accepted)
            };
            match payload {
                Ok(payload) => Some(payload),
                Err(why) => {
                    // Binding/job/shape proofs and the existing binding DA obligation do not
                    // require the accepted FP body, or an executable backend context.
                    self.fraud_filer_hold_missing_fp_job_v1(book, claim);
                    if let Some(case) = book.cases.get_mut(&claim) {
                        case.input_retry_at = current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    }
                    debug!("[{PALW_PANEL}] claim {claim}: public FP job unavailable; pursuing its binding: {why}");
                    return;
                }
            }
        } else {
            None
        };
        let preflight = session
            .clone()
            .spawn_blocking(move |c| c.palw_fraud_filer_target_v1(claim).is_none_or(|target| target.registered_semantics.is_some()))
            .await;
        book.cases.get_mut(&claim).expect("selected case").registered_preflight = preflight;
        if preflight
            && let Some(payload) = fp_payload.as_ref()
            && !self.fraud_filer_hold_fp_input_v1(book, claim, payload.clone(), self.class_prompt_ids_form(job.class_id))
        {
            book.cases.get_mut(&claim).expect("selected case").input_retry_at =
                current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
            return;
        }
        // A probe selected while a replay/capture occupies the lane must not instantiate a
        // backend or reserve/start an extra replay. Missing-body binding pursuits were held above.
        if metadata_only {
            book.cases.get_mut(&claim).expect("held above").input_retry_at =
                current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
            return;
        }
        let Some(artifact_root) = job.artifact_root else { return unjudged(book, "its class is gone from the registry".into()) };
        let mut backend = match self.resolve_backend(session, job.class_id, artifact_root) {
            Ok(backend) => backend,
            Err(why) => {
                if book.cases.get(&claim).is_some_and(|case| case.fp_job_pending) {
                    book.cases.get_mut(&claim).expect("held above").input_retry_at =
                        current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    debug!("[{PALW_PANEL}] claim {claim}: model resolution waits while public binding pursuit remains live: {why}");
                    return;
                }
                return unjudged(book, format!("its class does not resolve on this host ({why})"));
            }
        };
        let prompt_form = self.class_prompt_ids_form(job.class_id);
        let mut prepared_only = false;
        let (ctx, mut prompt, fp_payload) = if let Some(payload) = fp_payload {
            let fp_job = payload.job();
            if fp_job.network_domain != network_domain {
                return unjudged(book, "the disclosed FP job has another network domain".into());
            }
            let canonical = fp_job.prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL;
            let user = fp_job.prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER;
            if ((canonical
                && matches!(
                    fp_job.version,
                    kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION
                        | kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V4_VERSION
                ))
                || user)
                && self.consensus_config.params.palw_legacy_held_da_v2_active_at(current_daa)
            {
                let held = book.cases.get(&claim).is_some_and(|case| case.fp_bootstrap.is_some());
                if !self.fraud_filer_hold_fp_input_v1(book, claim, payload.clone(), prompt_form) {
                    book.cases.get_mut(&claim).expect("held above").input_retry_at =
                        current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    return;
                }
                // Missing user input must reach the public court BEFORE context derivation or
                // a full replay ticket can fail. Authentication holds only the input ticket;
                // the worker hashes one public prefix, never a producer capture, off the loop.
                if user {
                    let input = book.cases.get(&claim).and_then(|case| case.fp_bootstrap.as_ref()).expect("held above");
                    if !input.ready() {
                        match input::palw_fraud_filer_read_user_input_v1(input.payload.clone(), prompt_form, public_material).await {
                            Ok(ids) => {
                                book.cases.get_mut(&claim).expect("held above").fp_bootstrap.as_mut().expect("held above").ids = ids
                            }
                            Err(why) => {
                                book.cases.get_mut(&claim).expect("held above").input_retry_at =
                                    current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                                debug!(
                                    "[{PALW_PANEL}] claim {claim}: the fraud filer's authenticated public FP input waits before replay: {why}"
                                );
                                return;
                            }
                        }
                    }
                    // A fresh complete public input should not open an unnecessary reservation
                    // while the ordinary replay runs. An existing court bootstrap stays held
                    // until its replay verdict releases or prosecutes its live reservation.
                    prepared_only = !held && !book.cases.get(&claim).is_some_and(|case| case.fp_job_pending);
                }
            }
            // Budget the declared ceiling, even if the producer claims an early stop. Only the verifier's run decides its context.
            let Some(ctx) = backend.fp_job_context_v1(payload.job()) else {
                if book.cases.get(&claim).is_some_and(|c| c.fp_bootstrap.is_some()) {
                    book.cases.get_mut(&claim).expect("held above").input_retry_at =
                        current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    return;
                }
                return unjudged(book, "this backend cannot derive the FP job's ceiling context".into());
            };
            (ctx, Vec::new(), Some(payload))
        } else {
            let Some((ctx, prompt)) =
                self.attempt_job_for_claim(session, backend.as_ref(), network_domain, job.accepted_block, job.class_id, &producer)
            else {
                if let Some(case) = book.cases.get_mut(&claim) {
                    case.input_retry_at = current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                }
                return;
            };
            (ctx, prompt, None)
        };
        let backends = self.backends();
        let mut cache_evictions = 0;
        let reserved = loop {
            match self.reserve_replay_at_widest_run_v1(PALW_FRAUD_FILER_REPLAY_ROLE_V1, backend.as_mut(), job.class_id, claim, |b| {
                backends.role_memory_need_for_backend_or_chain_v1(
                    b,
                    job.class_id,
                    artifact_root,
                    Some(&ctx),
                    super::FULL_SEAT_V1,
                    |id| self.chain_carriage_v1(session, id),
                )
            }) {
                Ok((reserved, _)) => break reserved,
                Err(_) if cache_evictions < PALW_FRAUD_FILER_HELD_RUNS_V1 && book.evict_oldest_capture() => {
                    cache_evictions += 1;
                }
                Err(why) => {
                    book.cases.get_mut(&claim).expect("held above").input_retry_at =
                        current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    crate::palw_backends::note_throttled_v1("panel-fraud-filer-ledger", || {
                        format!("[{PALW_PANEL}] claim {claim}: the fraud filer's replay waits: {why} (LG14-A)")
                    });
                    return;
                }
            }
        };
        // Canonical derivation and replay input copies hold the full ticket. Acquired user ids
        // were already authenticated under the small ticket, off the loop, or by the DA learner.
        let fp_job = if let Some(payload) = fp_payload {
            let acquired = book.cases.get(&claim).and_then(|c| c.fp_bootstrap.as_ref()).filter(|input| input.ready());
            let input_result = if let Some(input) = acquired {
                Ok(input::PalwFraudFilerFpInputV1 { job: payload.job().clone(), prompt_ids: input.ids.clone() })
            } else {
                input::palw_fraud_filer_fp_descriptor_input_v1(backend.as_ref(), &payload, prompt_form, public_material)
            };
            let input = match input_result {
                Ok(input) => input,
                Err(why) => {
                    if let Some(case) = book.cases.get_mut(&claim) {
                        case.input_retry_at = current_daa.saturating_add(PALW_FRAUD_FILER_RESEND_DAA_V1);
                    }
                    if ((payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL
                        && matches!(
                            payload.job().version,
                            kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION
                                | kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V4_VERSION
                        ))
                        || payload.job().prompt_mode == kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER)
                        && self.consensus_config.params.palw_legacy_held_da_v2_active_at(current_daa)
                    {
                        self.fraud_filer_hold_fp_input_v1(book, claim, payload, prompt_form);
                    }
                    debug!("[{PALW_PANEL}] claim {claim}: the fraud filer's authenticated public FP input waits: {why}");
                    return;
                }
            };
            prompt = input.prompt_ids.into_iter().map(|id| id as usize).collect();
            Some(input.job)
        } else {
            None
        };
        if prepared_only {
            book.cases.get_mut(&claim).expect("held above").fp_bootstrap = None;
        }
        let Ok(prompt32) = prompt.iter().map(|id| u32::try_from(*id)).collect::<Result<Vec<u32>, _>>() else {
            return unjudged(book, "a prompt id past u32".into());
        };
        info!("[{PALW_PANEL}] claim {claim}: the fraud filer replays the claim's job off the loop (LG14-A, RFC-0014 §6.1)");
        let attempt_draw = self.attempt_draw_for_claim(session, job.accepted_block);
        book.started_replay(claim);
        let handle = tokio::task::spawn_blocking(move || {
            palw_fraud_filer_execute_v1(backend, ctx, prompt32, prompt_form, attempt_draw, fp_job, Some(reserved))
        });
        book.running = Some(PalwFraudFilerReplayV1 { candidate, handle });
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::{PalwDisputeClaimV1, PalwDisputeReservationRowV1};
    use kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1;
    use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(0xF1), n as u32))
    }

    fn candidate(claim: u64, accepted_daa: u64, seat: bool) -> PalwFraudFilerCandidateV1 {
        PalwFraudFilerCandidateV1 {
            claim_id: Hash64::from_u64_word(claim),
            producer: bond(99),
            accepted_daa,
            seat,
            job: PalwOperatorDaJobV1 {
                accepted_block: Hash64::from_u64_word(claim + 1_000),
                class_id: Hash64::from_u64_word(7),
                artifact_root: Some(Hash64::from_u64_word(8)),
                execution_root: Hash64::from_u64_word(0xE0 + claim),
                trace_root: Hash64::from_u64_word(0x70 + claim),
                output_root: Hash64::default(),
                work_leaves: 0,
                free_prompt: false,
                held_to_final: false,
            },
        }
    }

    pub(in crate::palw_panel) fn candidate_for_input_test() -> PalwFraudFilerCandidateV1 {
        candidate(1, 100, false)
    }

    fn view(claim: u64, phase: PalwClaimPhaseV2) -> PalwLegacyDisputeViewV1 {
        PalwLegacyDisputeViewV1 {
            claim_id: Hash64::from_u64_word(claim),
            class_id: Hash64::from_u64_word(7),
            producer: bond(99),
            phase,
            execution_root: Hash64::from_u64_word(0xE0 + claim),
            trace_root: Hash64::from_u64_word(0x70 + claim),
            accepted_daa: 100,
            trace_retention_daa: 5_000,
            job_identity: Hash64::default(),
            public_fp_job: None,
            registered_semantics: None,
            work_leaves: 0,
            deadline_daa: None,
            hard_deadline_daa: 3_800,
            record: None,
            sessions: Vec::new(),
            answered: Vec::new(),
            flat_answered_rows: 0,
            open_courts: 0,
            executor_refuted: false,
            court_convicted: false,
            da_defaulted: false,
        }
    }

    #[test]
    fn full_discovery_cache_admits_later_claims_without_abandoning_live_pursuits() {
        let mut book = PalwFraudFilerBookV1::default();
        let mut candidates: Vec<_> = (1..=PALW_FRAUD_FILER_MAX_CASES_V1 as u64).map(|id| candidate(id, 100, false)).collect();
        book.refresh(candidates.clone(), 200);
        let first = candidates[0].claim_id;
        book.started_replay(first);
        book.judge(first, Ok(run(Hash64::default(), Hash64::default())));
        let late = candidate(9000, 201, false);
        candidates.push(late.clone());
        book.refresh(candidates.clone(), 201);
        assert_eq!(book.cases.len(), PALW_FRAUD_FILER_MAX_CASES_V1);
        assert!(book.cases.contains_key(&late.claim_id), "a full local discovery cache cannot silently discard a later public claim");
        assert!(
            matches!(book.cases[&first].verdict, PalwFraudFilerVerdictV1::Mismatch(_)),
            "an admitted pursuit is never evicted to admit discovery"
        );
        book.target_claims.insert(late.claim_id);
        assert_eq!(book.next_probe_at(201, true), Some(late.claim_id), "the operator's finite target precedes unrelated waiting jobs");
        book.refresh(candidates, 202);
        assert!(book.cases.contains_key(&late.claim_id));
        assert!(book.cases.contains_key(&first));
    }

    #[tokio::test]
    async fn discovery_rotation_preserves_a_running_acceptance_and_exposes_full_live_pressure() {
        let mut book = PalwFraudFilerBookV1::default();
        let mut candidates: Vec<_> = (1..=PALW_FRAUD_FILER_MAX_CASES_V1 as u64).map(|id| candidate(id, 100, false)).collect();
        book.refresh(candidates.clone(), 200);
        let running = candidates[0].clone();
        book.running =
            Some(PalwFraudFilerReplayV1 { candidate: running.clone(), handle: tokio::spawn(async { Err("unused".into()) }) });
        candidates.push(candidate(9000, 201, false));
        book.refresh(candidates.clone(), 201);
        assert!(book.cases.contains_key(&running.claim_id));
        for case in book.cases.values_mut() {
            case.fp_job_pending = true;
        }
        let later = candidate(9001, 202, false);
        candidates.push(later.clone());
        book.refresh(candidates, 202);
        assert_eq!(book.cases.len(), PALW_FRAUD_FILER_MAX_CASES_V1);
        assert!(!book.cases.contains_key(&later.claim_id));
        assert!(book.deferred_candidates > 0, "all-live pressure is explicit rather than an invisible discarded target");
        assert!(book.running.take().unwrap().handle.await.unwrap().is_err());
    }

    #[test]
    fn waiting_public_input_does_not_spend_a_run_or_block_another_pending_claim() {
        let mut book = PalwFraudFilerBookV1::default();
        let first = candidate(1, 100, false);
        let second = candidate(2, 101, false);
        book.refresh(vec![first.clone(), second.clone()], 100);
        book.cases.get_mut(&first.claim_id).unwrap().input_retry_at = 130;
        assert_eq!(book.next_replay_at(100), Some(second.claim_id));
        assert_eq!(book.next_replay_at(129), Some(second.claim_id));
        assert_eq!(book.next_replay_at(130), Some(first.claim_id));
        let case = &book.cases[&first.claim_id];
        assert_eq!(case.runs, 0);
        assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending));
        let mut changed = first.clone();
        changed.job.accepted_block = Hash64::from_u64_word(999);
        book.refresh(vec![changed, second], 110);
        assert_eq!(book.next_replay_at(110), Some(first.claim_id), "a new job clears its predecessor's wait");
    }

    #[test]
    fn waiting_bootstrap_is_pursued_without_holding_a_replay_slot_and_reorg_forgets_input() {
        let mut book = PalwFraudFilerBookV1::default();
        let a = candidate(1, 100, false);
        let b = candidate(2, 101, false);
        book.refresh(vec![a.clone(), b.clone()], 100);
        let payload = Arc::new(input::tests::payload());
        book.cases.get_mut(&a.claim_id).unwrap().fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
            form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(payload.as_ref().clone())),
            ids: vec![],
            chunk: 0,
            _reservation: None,
        });
        assert_eq!(book.pursued(), vec![a.claim_id]);
        assert_eq!(book.held_runs(), 0);
        assert_eq!(book.next_replay_at(100), Some(b.claim_id));
        assert_eq!(book.walk_floor(), Some(a.accepted_daa));
        let input = book.cases.get_mut(&a.claim_id).unwrap().fp_bootstrap.as_mut().unwrap();
        input.ids = payload.prompt_token_ids.clone();
        input.chunk = 1;
        assert_eq!(book.next_replay_at(100), Some(a.claim_id));
        book.walked(
            PalwFraudFilerWalkV1 { floor: 100, anchor: Hash64::from_u64_word(9), anchor_daa: 150, next: None, incomplete: false },
            true,
            vec![],
        );
        let case = &book.cases[&a.claim_id];
        assert_eq!(case.runs, 0);
        assert!(case.fp_bootstrap.as_ref().unwrap().ids.is_empty());
        assert_eq!(case.fp_bootstrap.as_ref().unwrap().chunk, 0);
        assert_eq!(book.next_replay_at(150), Some(b.claim_id));
        let mut changed = a.clone();
        changed.job.accepted_block = Hash64::default();
        book.refresh(vec![changed, b], 151);
        assert!(book.cases[&a.claim_id].fp_bootstrap.is_none());
        assert_eq!(book.next_replay_at(151), Some(a.claim_id));
    }

    #[test]
    fn public_fp_replay_opens_its_own_constraint_mask_and_refuses_unavailable_tables() {
        use kaspa_consensus_core::palw_decode_constraint_v1::*;
        use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
        use kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1;
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let (mut job, ids) = crate::palw_panel::seat_s_tests::floor_fp_job(&backend);
        let constraint = PalwDecodeConstraintV1 {
            version: 1,
            compiler_id: Hash64::from_u64_word(0xC014),
            start_frame: 0,
            frames: vec![PalwConstraintFrameV1 {
                start: 0,
                nodes: vec![PalwConstraintNodeV1 {
                    accepting: true,
                    edges: vec![PalwConstraintEdgeV1 { lo: b'a', hi: b'a', action: PalwConstraintActionV1::Goto(0) }],
                }],
            }],
        };
        let vocab = misaka_palw_base0::rc::palw_rc_base0_shape_v1().vocab;
        let mut table = PalwTokenTableV1 {
            entries: vec![Some(b"b".to_vec()); vocab],
            eog_token_ids: vec![(vocab - 1) as u32],
            tokenizer_id: job.tokenizer_id,
        };
        table.entries[0] = Some(b"a".to_vec());
        let root = palw_token_tables_v1().register(table);
        job.version = PALW_FP_CONSTRAINT_VERSION;
        job.decode = None;
        job.tail = Some(PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 { constraint: constraint.to_bytes(), table_root: root }));
        let mut selection = PalwFraudFilerCaseV1::new(candidate(1, 100, false));
        let mut descriptor = input::tests::payload();
        descriptor.commitment.job = job.clone();
        selection.fp_descriptor = Some(Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(descriptor)));
        assert!(!selection.constraint_preflight_pending_v1(), "binding bootstrap must remain reachable");
        selection.binding = Some(material_budget_binding());
        assert!(selection.constraint_preflight_pending_v1(), "no generic arithmetic session may preempt selection");
        selection.constraint_done = true;
        assert!(!selection.constraint_preflight_pending_v1(), "verified selection yields to arithmetic localization");
        let prompt: Vec<_> = ids.iter().map(|id| *id as usize).collect();
        assert!(backend.execute_free_prompt(&job, &prompt).is_err(), "a constrained run requires its host scope");
        let expected = palw_fp_with_constraint_scope_v1(
            palw_fp_constraint_mask_for_host_v1(&job, backend.token_table_for_job_v1(&job)).unwrap(),
            || backend.execute_free_prompt(&job, &prompt),
        )
        .unwrap();
        assert!(expected.output_token_ids.iter().all(|id| *id == 0), "the constraint permits only lane zero");
        let ctx = backend.fp_job_context_for_executed_v1(&job, expected.facts.decode_tokens_executed).unwrap();
        let replay = |job| {
            palw_fraud_filer_execute_v1(
                Box::new(crate::palw_panel::seat_s_tests::floor_backend()),
                ctx.clone(),
                ids.clone(),
                backend.prompt_ids_form(),
                None,
                Some(job),
                None,
            )
        };
        let actual = replay(job.clone()).expect("the outside verifier holds the public table and its own registered model");
        assert_eq!(actual.execution_root, expected.outcome.execution_root);
        assert_eq!(actual.trace_root, expected.outcome.trace_root);
        assert_eq!(actual.output_root, expected.outcome.output_root);
        assert_eq!(actual.fp_job.as_deref(), Some(&job), "a mismatched run can later build its public job proof");
        assert!(actual._job_reservation.is_some(), "the retained descriptor holds its memory ticket");
        let c = candidate(0xC014, 300, false);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        assert!(matches!(book.judge(c.claim_id, Ok(actual)), Some(PalwFraudFilerVerdictV1::Mismatch(_))));
        assert!(book.cases[&c.claim_id].fp_bootstrap.is_none());
        assert!(book.cases[&c.claim_id].fp_descriptor.is_none());
        assert_eq!(book.cases[&c.claim_id].public_fp_job_v1(), Some(&job), "successful replay cleanup retains the prosecution job");
        assert!(palw_fp_constraint_scope_v1().is_none(), "the replay restores its thread scope");
        let mut wrong_tokenizer = job.clone();
        wrong_tokenizer.tokenizer_id = Hash64::from_u64_word(0xBAD_C015);
        assert!(replay(wrong_tokenizer).is_err(), "a held table from another tokenizer cannot drive a judgement");
        let Some(PalwFpJobTailV1::Constraint(tail)) = job.tail.as_mut() else { unreachable!() };
        tail.table_root = Hash64::from_u64_word(0xBAD_C014);
        assert!(replay(job).is_err(), "an unavailable table is a refusal, never a judgement");
        assert!(palw_fp_constraint_scope_v1().is_none());
    }

    #[test]
    fn matching_roots_hold_registered_semantics_for_authenticated_binding_and_job() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        use kaspa_consensus_core::palw_offence_attribution_v1::{PalwClaimSourceKindV1, PalwIdentityRulesV1, PalwOffenceTargetV1};
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let payload = input::tests::payload();
        let job = payload.commitment.job;
        let prompt: Vec<_> = payload.prompt_token_ids.iter().map(|id| *id as usize).collect();
        let executed = backend.execute_free_prompt(&job, &prompt).unwrap();
        let event = backend.disclose_trace_event(&executed.outcome.material, 0, 0).unwrap();
        let binding = event.binding();
        let mut c = candidate(0xC018, 300, false);
        c.job.free_prompt = true;
        c.job.execution_root = executed.outcome.execution_root;
        c.job.trace_root = executed.outcome.trace_root;
        c.job.output_root = executed.outcome.output_root;
        let target = PalwOffenceTargetV1 {
            claim_id: c.claim_id,
            class_id: binding.shape_profile.shape_profile_id(),
            artifact_root: Hash64::from_u64_word(8),
            executor_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(job.executor_bond),
            execution_root: executed.outcome.execution_root,
            trace_root: executed.outcome.trace_root,
            output_root: executed.outcome.output_root,
            lane: Some(PalwClaimSourceKindV1::FreePrompt),
            segment_count: Some(1),
            phase: None,
            job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context),
            registered_semantics: Some(kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1::of_registration(
                binding.shape_profile.shape_profile_id(),
                Hash64::from_u64_word(8),
                &binding.job_context,
                300,
            )),
        };
        let rules = PalwIdentityRulesV1 {
            prompt_ids_form: backend.prompt_ids_form(),
            base_class_id: target.class_id,
            da_signer_liability: true,
        };
        let answer = PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event.clone()));
        assert!(palw_fraud_filer_registered_preflight_v1(&target, rules, &answer, Some(&job), false, false).unwrap());
        assert!(
            !palw_fraud_filer_registered_preflight_v1(&target, rules, &answer, None, false, false).unwrap(),
            "a matching replay cannot erase its job obligation"
        );
        let mut wrong = target.clone();
        wrong.registered_semantics.as_mut().unwrap().tokenizer_id = Hash64::from_u64_word(0xBAD);
        assert!(
            !palw_fraud_filer_registered_preflight_v1(&wrong, rules, &answer, Some(&job), false, false).unwrap(),
            "matching arithmetic does not clear J8"
        );
        wrong = target.clone();
        wrong.execution_root = Hash64::from_u64_word(0xBAD);
        assert!(
            palw_fraud_filer_registered_preflight_v1(&wrong, rules, &answer, Some(&job), false, false).is_err(),
            "a foreign row cannot clear the semantic obligation"
        );
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(8192), || None);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        book.cases.get_mut(&c.claim_id).unwrap().registered_preflight = true;
        let mut replica = run(c.job.execution_root, c.job.trace_root);
        replica.output_root = c.job.output_root;
        replica.fp_job = Some(Box::new(job.clone()));
        replica._job_reservation = Some(
            ledger
                .reserve(PalwMemoryReservationKeyV1 { role: "matching-public", class_id: c.job.class_id, job: c.claim_id }, 1024)
                .unwrap(),
        );
        replica._reservation = Some(
            ledger
                .reserve(PalwMemoryReservationKeyV1 { role: "matching-capture", class_id: c.job.class_id, job: c.claim_id }, 4096)
                .unwrap(),
        );
        assert!(matches!(book.judge(c.claim_id, Ok(replica)), Some(PalwFraudFilerVerdictV1::SemanticPending)));
        assert_eq!(ledger.reserved_bytes(), 1024, "only the authenticated public job survives a matching replay");
        assert_eq!(book.held_runs(), 0);
        assert!(book.cases[&c.claim_id].pursued());
        assert!(book.cases[&c.claim_id].may_finish_semantics_v1(true));
        assert!(!book.cases[&c.claim_id].may_finish_semantics_v1(false));
        assert!(!book.cases[&c.claim_id].release_pending);
        assert_eq!(book.cases[&c.claim_id].public_fp_job_v1(), Some(&job));
        assert!(book.next_replay_at(300).is_none(), "the semantic court requires a binding, not another arithmetic run");
        assert!(matches!(
            palw_fraud_filer_bootstrap_step_v1(
                PalwFilerRoleV1::PublicBond,
                Some(&view(0xC018, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 300 })),
                &bond(1),
                true,
                &book.cases[&c.claim_id],
                |_| false
            ),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve)
        ));
    }

    #[test]
    fn registered_replay_refusal_keeps_the_public_job_court_without_claiming_arithmetic_honesty() {
        let c = candidate(0xC019, 300, false);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        let payload = input::tests::payload();
        let job = payload.commitment.job.clone();
        let case = book.cases.get_mut(&c.claim_id).unwrap();
        case.registered_preflight = true;
        case.fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
            payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(payload)),
            form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            ids: Vec::new(),
            chunk: 0,
            _reservation: None,
        });
        assert!(matches!(
            book.judge(c.claim_id, Err("named prefix differs from the recomputed checkpoint".into())),
            Some(PalwFraudFilerVerdictV1::SemanticPending)
        ));
        let case = &book.cases[&c.claim_id];
        assert!(case.pursued());
        assert!(!case.release_pending);
        assert!(!case.arithmetic_matched);
        assert!(!case.may_finish_semantics_v1(true), "even complete public semantics cannot certify a refused arithmetic run");
        assert!(case.fp_bootstrap.is_none(), "a refusal keeps only the public descriptor rather than accumulated input ids");
        assert_eq!(case.public_fp_job_v1(), Some(&job));
        assert_eq!(book.next_replay_at(300), None, "a backend refusal pursues its public court without an infinite host retry");
    }

    #[test]
    fn matching_tir_logits_with_another_execution_or_output_root_remain_mismatched() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(1_048_576), || None);
        for (execution_matches, output_matches) in [(false, true), (true, false), (false, false)] {
            let mut case = PalwFraudFilerCaseV1::new(candidate(0xC01A, 300, false));
            let job = input::tests::payload().commitment.job;
            let ids = vec![1, 2];
            let execution_root = if execution_matches { case.candidate.job.execution_root } else { Hash64::from_u64_word(900) };
            let output_root = if output_matches { case.candidate.job.output_root } else { Hash64::from_u64_word(901) };
            let (job_ticket, ids_ticket) = palw_fraud_filer_tir_retention_v1(&ledger, case.candidate.claim_id, &job, 2).unwrap();
            case.retain_tir_selection_replay_v1(
                job.clone(),
                kaspa_consensus_core::palw_backend::PalwTirReplayV1 {
                    generated_ids: ids.clone(),
                    full_logits_trace_root: case.candidate.job.trace_root,
                    execution_root,
                    output_root,
                },
                job_ticket,
                ids_ticket,
            );
            assert!(case.pursued(), "matching logits cannot bypass the registered public stages");
            assert!(!case.release_pending);
            assert!(case.staged_bundle_pending);
            assert!(!case.prefix_complete && !case.constraint_done && !case.rendering_done);
            assert!(!case.arithmetic_matched);
            assert!(!case.may_finish_semantics_v1(true));
            let PalwFraudFilerVerdictV1::Mismatch(run) = &case.verdict else { panic!("real mismatched roots retained") };
            assert_eq!(run.own_output_ids, ids);
            assert_eq!(run.fp_job.as_deref(), Some(&job));
            assert_eq!(run.execution_root, execution_root);
            assert_eq!(run.output_root, output_root);
            assert_eq!(
                case.neutral_constraint_plan_v1(&ids),
                Some(constraint::TiledPlanV1 { position: 0, committed: 1, witness: None, beat_lane: None })
            );
            assert!(ledger.reserved_bytes() > 0);
            drop(case);
            assert_eq!(ledger.reserved_bytes(), 0);
        }
    }

    #[test]
    fn matching_all_tir_roots_keep_budgeted_ids_and_require_public_bundle_completion() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(1_048_576), || None);
        let mut case = PalwFraudFilerCaseV1::new(candidate(0xC01A, 300, false));
        // A local policy fixture only; no synthetic header is submitted as a native credential.
        let binding = material_budget_binding();
        case.tir = Some(Arc::new(FilerTirContextV1 {
            view: kaspa_consensus_core::palw_decode_view_v1::PalwDecodeViewV1::of_verified_binding_v1(&binding),
            header: Box::new(kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1 {
                version: 1,
                job_context: binding.job_context.clone(),
                artifact_root: Hash64::default(),
                full_logits_trace_root: binding.full_logits_trace_root,
                step_leaf_count: binding.step_leaf_count,
                step_merkle_root: binding.step_merkle_root,
                committed_execution_root: binding.committed_execution_root,
            }),
        }));
        let job = input::tests::payload().commitment.job;
        let ids = vec![1, 2];
        let (job_ticket, ids_ticket) = palw_fraud_filer_tir_retention_v1(&ledger, case.candidate.claim_id, &job, 2).unwrap();
        case.retain_tir_selection_replay_v1(
            job.clone(),
            kaspa_consensus_core::palw_backend::PalwTirReplayV1 {
                generated_ids: ids.clone(),
                full_logits_trace_root: case.candidate.job.trace_root,
                execution_root: case.candidate.job.execution_root,
                output_root: case.candidate.job.output_root,
            },
            job_ticket,
            ids_ticket,
        );
        assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::SemanticPending));
        assert!(case.arithmetic_matched);
        assert!(case.pursued());
        assert!(!case.release_pending);
        assert!(case.staged_bundle_pending);
        assert!(!case.prefix_complete && !case.constraint_done && !case.rendering_done);
        assert!(!case.may_finish_semantics_v1(true));
        assert!(!case.finish_tir_semantics_v1());
        assert_eq!(case.tir_selection_ids.as_ref().unwrap().ids, ids);
        assert_eq!(case.public_fp_job_v1(), Some(&job));
        assert!(case.neutral_constraint_plan_v1(&ids).is_some());
        case.staged_bundle_pending = false;
        case.constraint_stage_pending = true;
        assert!(!case.finish_tir_semantics_v1());
        case.constraint_stage_pending = false;
        assert!(case.finish_tir_semantics_v1());
        assert!(case.release_pending);
        assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Honest));
        let cloned = case.clone();
        let retained = ledger.reserved_bytes();
        assert!(retained > 0);
        drop(case);
        assert_eq!(ledger.reserved_bytes(), retained, "case clones retain the original descriptor/ID tickets");
        drop(cloned);
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    #[test]
    fn missing_or_refused_selection_cannot_freeze_a_neutral_constraint_stage() {
        let mut case = PalwFraudFilerCaseV1::new(candidate(0xC01B, 300, false));
        let ids = [1, 2];
        assert!(case.neutral_constraint_plan_v1(&ids).is_none());
        case.verdict = PalwFraudFilerVerdictV1::SemanticPending;
        assert!(case.neutral_constraint_plan_v1(&ids).is_none());
        case.verdict = PalwFraudFilerVerdictV1::Mismatch(Arc::new(run(Hash64::default(), Hash64::default())));
        assert!(case.neutral_constraint_plan_v1(&ids).is_none(), "a non-FP replay supplies no completed selection");
        case.verdict = PalwFraudFilerVerdictV1::SemanticPending;
        case.arithmetic_matched = true;
        assert!(case.neutral_constraint_plan_v1(&ids).is_some());
    }

    #[test]
    fn input_bootstrap_keeps_binding_proofs_reachable_even_when_all_input_is_public() {
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let payload = Arc::new(input::tests::payload());
        let job = &payload.commitment.job;
        let executed =
            backend.execute_free_prompt(job, &payload.prompt_token_ids.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        let binding = misaka_palw_base0::produce::base0_material_decode_any_v1(&executed.outcome.material).unwrap().binding().clone();
        let me = bond(1);
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        case.fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
            payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(payload.as_ref().clone())),
            form: backend.prompt_ids_form(),
            ids: payload.prompt_token_ids.clone(),
            chunk: 0,
            _reservation: None,
        });
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 300 });
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 300, deposit: 1, sessions_opened: 0, lanes_joined: 0 });
        v.record = Some(record);
        let step = |case: &PalwFraudFilerCaseV1| {
            palw_fraud_filer_bootstrap_step_v1(PalwFilerRoleV1::PublicBond, Some(&v), &me, true, case, |_| false)
        };
        assert_eq!(
            step(&case),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(PalwLegacyProbeV1::Binding { row: 0, tile: 0 })),
            "a failed ceiling/context must still be able to obtain a binding for direct job proofs"
        );
        case.binding = Some(binding);
        assert_eq!(
            step(&case),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait),
            "available input must not cause redundant PromptIds obligations"
        );
    }

    #[test]
    fn bootstrap_verdicts_release_only_after_honesty_or_final_host_failure_and_reset_on_job_change() {
        let c = candidate(3, 300, false);
        for fail in [false, true] {
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![c.clone()], 300);
            book.cases.get_mut(&c.claim_id).unwrap().fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
                payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(input::tests::payload())),
                form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                ids: vec![],
                chunk: 0,
                _reservation: None,
            });
            if fail {
                assert!(matches!(book.judge(c.claim_id, Err("host failure".into())), Some(PalwFraudFilerVerdictV1::Pending)));
                assert!(!book.cases[&c.claim_id].release_pending);
                assert!(book.cases[&c.claim_id].fp_bootstrap.is_some());
                book.judge(c.claim_id, Err("host failure again".into()));
                assert!(matches!(book.cases[&c.claim_id].verdict, PalwFraudFilerVerdictV1::Unjudged(_)));
            } else {
                book.judge(c.claim_id, Ok(run(c.job.execution_root, c.job.trace_root)));
                assert!(matches!(book.cases[&c.claim_id].verdict, PalwFraudFilerVerdictV1::Honest));
            }
            assert!(book.cases[&c.claim_id].release_pending);
            assert_eq!(book.pursued(), vec![c.claim_id]);
            assert!(
                book.cases[&c.claim_id].fp_bootstrap.is_none(),
                "the input ticket is relinquished before releasing the chain reservation"
            );
            let mut new = c.clone();
            new.job.accepted_block = Hash64::from_u64_word(999);
            book.refresh(vec![new], 301);
            assert!(!book.cases[&c.claim_id].release_pending, "old replay cannot release a different branch's reservation");
        }
    }

    #[tokio::test]
    async fn a_missing_fp_body_can_be_probed_while_the_local_capture_lane_is_full() {
        let mut book = PalwFraudFilerBookV1::default();
        let first = candidate(1, 100, false);
        let second = candidate(2, 101, false);
        let mut fp = candidate(3, 102, false);
        fp.job.free_prompt = true;
        let attempt = candidate(4, 103, false);
        book.refresh(vec![first.clone(), second.clone(), fp.clone(), attempt.clone()], 100);
        book.judge(first.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        book.judge(second.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        assert_eq!(book.held_runs(), PALW_FRAUD_FILER_HELD_RUNS_V1);
        assert_eq!(book.next_replay_at(100), Some(fp.claim_id), "a public binding proof does not need a capture slot");
        book.cases.get_mut(&fp.claim_id).unwrap().input_retry_at = 130;
        assert_eq!(book.next_replay_at(129), None, "busy probes neither bypass their retry nor execute the pending attempt");
        assert_eq!(book.next_replay_at(130), Some(fp.claim_id));
        assert_eq!(book.cases[&fp.claim_id].runs, 0);
        book.settle(&first.claim_id, PalwFraudFilerVerdictV1::Unjudged("done".into()));
        let mut other_fp = candidate(5, 104, false);
        other_fp.job.free_prompt = true;
        book.refresh(vec![first, second, fp.clone(), attempt, other_fp.clone()], 130);
        book.running =
            Some(PalwFraudFilerReplayV1 { candidate: fp.clone(), handle: tokio::spawn(async { Err("unused worker result".into()) }) });
        assert_eq!(
            book.next_probe_at(130, false),
            Some(other_fp.claim_id),
            "seat pressure cannot stop metadata probing, and the running acceptance is not probed twice"
        );
        assert!(book.running.take().unwrap().handle.await.unwrap().is_err());
    }

    #[test]
    fn full_capture_cache_rotates_fairly_without_forgetting_a_public_pursuit() {
        let mut book = PalwFraudFilerBookV1::default();
        let claims: Vec<_> = (1..=3).map(|id| candidate(id, 100 + id, false)).collect();
        book.refresh(claims.clone(), 200);
        for claim in &claims[..2] {
            book.started_replay(claim.claim_id);
            book.judge(claim.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        }
        let first = claims[0].claim_id;
        book.cases.get_mut(&first).unwrap().sent = Some((PalwFraudFilerSentV1::Reserve, 150, 1));
        book.cases.get_mut(&first).unwrap().bisect = PalwLegacyBisectV1::new(17);
        let weak = match &book.cases[&first].verdict {
            PalwFraudFilerVerdictV1::Mismatch(run) => Arc::downgrade(run),
            _ => unreachable!(),
        };
        assert_eq!(book.next_probe_at(200, true), None);
        book.make_replay_room(200, false);
        assert_eq!(book.held_runs(), PALW_FRAUD_FILER_HELD_RUNS_V1, "seat pressure never starts another replica");
        book.make_replay_room(200, true);
        assert!(weak.upgrade().is_none(), "the evicted closure and capture ticket have actually dropped");
        assert_eq!(book.held_runs(), PALW_FRAUD_FILER_HELD_RUNS_V1 - 1);
        assert!(matches!(book.cases[&first].verdict, PalwFraudFilerVerdictV1::DeferredMismatch));
        assert_eq!(book.cases[&first].sent, Some((PalwFraudFilerSentV1::Reserve, 150, 1)));
        assert!(book.pursued().contains(&first), "eviction cannot relinquish the active prosecution");
        assert_eq!(book.next_probe_at(200, true), Some(claims[2].claim_id), "a never-replayed claim precedes older replicas");
        book.started_replay(claims[2].claim_id);
        book.judge(claims[2].claim_id, Ok(run(Hash64::default(), Hash64::default())));
        book.make_replay_room(200, true);
        assert_eq!(book.next_probe_at(200, true), Some(first), "the deferred claim gets its next fair turn");
        book.started_replay(first);
        book.judge(first, Ok(run(Hash64::default(), Hash64::default())));
        book.make_replay_room(200, true);
        assert_eq!(book.next_probe_at(200, true), Some(claims[1].claim_id));
        assert!(book.held_runs() < PALW_FRAUD_FILER_HELD_RUNS_V1);
        // Successful cache rotations do not consume the bounded host-failure retry allowance.
        assert!(matches!(book.judge(first, Err("transient host error".into())), Some(PalwFraudFilerVerdictV1::Pending)));
        assert!(matches!(book.judge(first, Err("second host error".into())), Some(PalwFraudFilerVerdictV1::Unjudged(_))));
    }

    #[test]
    fn missing_job_reprobes_preserve_localization_until_authenticated_job_arrives() {
        let mut book = PalwFraudFilerBookV1::default();
        let first = candidate(1, 100, false);
        let second = candidate(2, 101, false);
        let mut missing = candidate(3, 102, false);
        missing.job.free_prompt = true;
        book.refresh(vec![first.clone(), second.clone(), missing.clone()], 200);
        for claim in [&first, &second] {
            book.started_replay(claim.claim_id);
            book.judge(claim.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        }
        book.cases.get_mut(&missing.claim_id).unwrap().fp_job_pending = true;
        let held = match &book.cases[&first.claim_id].verdict {
            PalwFraudFilerVerdictV1::Mismatch(run) => Arc::downgrade(run),
            _ => unreachable!(),
        };
        for daa in 200..232 {
            book.make_replay_room(daa, true);
            assert_eq!(book.held_runs(), PALW_FRAUD_FILER_HELD_RUNS_V1);
            assert!(held.upgrade().is_some());
            assert_eq!(book.next_probe_at(daa, true), Some(missing.claim_id));
            assert!(book.pursued().contains(&missing.claim_id), "binding DA remains reachable without a job");
        }
        book.cases.get_mut(&missing.claim_id).unwrap().fp_descriptor =
            Some(Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(input::tests::payload())));
        book.make_replay_room(232, true);
        assert!(held.upgrade().is_none());
        assert_eq!(book.held_runs(), PALW_FRAUD_FILER_HELD_RUNS_V1 - 1);
        assert_eq!(book.next_probe_at(232, true), Some(missing.claim_id));
        assert!(book.pursued().contains(&first.claim_id), "rotation preserves the earlier public court");
    }

    #[test]
    fn rotating_a_pursued_replica_does_not_restart_a_paginated_public_history_walk() {
        let mut book = PalwFraudFilerBookV1::default();
        let claim = candidate(1, 100, false);
        book.refresh(vec![claim.clone()], 200);
        book.judge(claim.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        assert!(book.evict_oldest_capture());
        let walk = PalwFraudFilerWalkV1 {
            anchor: Hash64::from_u64_word(200),
            anchor_daa: 200,
            floor: 100,
            next: Some(Hash64::from_u64_word(150)),
            incomplete: false,
        };
        book.walk = Some(walk);
        book.walked_from = Some(100);
        book.walked_to = Some(140);
        book.read_at = Some(200);
        book.judge(claim.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        assert_eq!(book.walk, Some(walk));
        assert_eq!((book.walked_from, book.walked_to, book.read_at), (Some(100), Some(140), Some(200)));
        let newer = candidate(2, 180, false);
        book.refresh(vec![claim, newer.clone()], 201);
        book.judge(newer.claim_id, Ok(run(Hash64::default(), Hash64::default())));
        assert!(book.walk.is_none(), "a newly pursued claim still requires its own backfill");
    }

    #[test]
    fn public_branch_rollback_invalidates_semantic_completion_before_honesty_or_release() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        for releasing in [false, true] {
            let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(4096), || None);
            let mut c = candidate(3, 300, false);
            c.job.free_prompt = true;
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![c.clone()], 400);
            let case = book.cases.get_mut(&c.claim_id).unwrap();
            case.registered_preflight = true;
            case.arithmetic_matched = true;
            case.prefix_complete = true;
            case.rendering_done = true;
            case.prefix_stage_pending = true;
            case.fp_job_pending = !releasing;
            case.release_pending = releasing;
            case.verdict = if releasing { PalwFraudFilerVerdictV1::Honest } else { PalwFraudFilerVerdictV1::SemanticPending };
            case.prefix_budget = 256;
            case._prefix_reservation = Some(Arc::new(
                ledger
                    .reserve(
                        PalwMemoryReservationKeyV1 { role: "rollback-prefix-test", class_id: c.job.class_id, job: c.claim_id },
                        256,
                    )
                    .unwrap(),
            ));
            assert!(case.pursued());
            let walk = PalwFraudFilerWalkV1 {
                anchor: Hash64::from_u64_word(401),
                anchor_daa: 401,
                floor: 300,
                next: Some(Hash64::from_u64_word(350)),
                incomplete: false,
            };
            book.walked(walk, true, vec![]);
            let case = &book.cases[&c.claim_id];
            assert!(!case.prefix_complete && !case.rendering_done && !case.arithmetic_matched && !case.prefix_stage_pending);
            assert!(!case.release_pending, "a rolled-back completed row cannot release an unverified replacement branch");
            assert!(!case.may_finish_semantics_v1(true), "semantic booleans cannot certify arithmetic absent a fresh own replay");
            assert!(case.pursued(), "rollback never abandons the live public reservation");
            assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending));
            assert_eq!(
                book.next_replay_at(401),
                Some(c.claim_id),
                "matching arithmetic is recomputed instead of stranding SemanticPending"
            );
            assert_eq!(ledger.reserved_bytes(), 0);
            assert_eq!(case.prefix_budget, 0);
        }
    }

    #[test]
    fn rotating_user_input_keeps_its_ticket_and_does_not_restart_public_history() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};
        use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(32_768), || None);
        let mut book = PalwFraudFilerBookV1::default();
        let claim = candidate(1, 100, false);
        book.refresh(vec![claim.clone()], 200);
        let payload = input::tests::payload();
        let descriptor = input::PalwFraudFilerFpDescriptorV1::Accepted(payload.clone());
        assert!(book.hold_fp_input_v1(claim.claim_id, descriptor.clone(), PalwPromptIdsFormV1::Flat, &ledger, 4096));
        assert!(book.walk.is_none(), "first bootstrap backfills answers not collected for a pending claim");
        book.cases.get_mut(&claim.claim_id).unwrap().fp_bootstrap.as_mut().unwrap().ids = payload.prompt_token_ids.clone();
        assert!(book.cases[&claim.claim_id].fp_bootstrap.as_ref().unwrap().ready());
        let reserved = ledger.reserved_bytes();
        let walk = PalwFraudFilerWalkV1 {
            anchor: Hash64::from_u64_word(200),
            anchor_daa: 200,
            floor: 100,
            next: Some(Hash64::from_u64_word(150)),
            incomplete: false,
        };
        book.walk = Some(walk);
        for _ in 0..8 {
            book.judge(claim.claim_id, Ok(run(Hash64::default(), Hash64::default())));
            assert!(book.cases[&claim.claim_id].fp_bootstrap.is_none());
            assert!(book.cases[&claim.claim_id].fp_replay_input.as_ref().unwrap().ready());
            assert!(book.evict_oldest_capture());
            assert!(book.hold_fp_input_v1(claim.claim_id, descriptor.clone(), PalwPromptIdsFormV1::Flat, &ledger, 4096));
            assert!(book.cases[&claim.claim_id].fp_bootstrap.as_ref().unwrap().ready());
            assert_eq!(book.walk, Some(walk), "already pursued input never restarts the paginated scan");
            assert_eq!(ledger.reserved_bytes(), reserved, "rotation neither reallocates nor releases authenticated ids");
        }
        book.settle(&claim.claim_id, PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::Convicted));
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    #[test]
    fn constrained_input_round_cannot_preempt_selector_with_arithmetic_obligations() {
        use kaspa_consensus_core::palw_fp_constraint_job_v1::PALW_FP_CONSTRAINT_VERSION;
        let mut payload = input::tests::payload();
        payload.commitment.job.version = PALW_FP_CONSTRAINT_VERSION;
        let me = bond(1);
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        let binding = material_budget_binding();
        case.binding = Some(binding.clone());
        case.fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
            payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(payload)),
            form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            ids: vec![],
            chunk: 0,
            _reservation: None,
        });
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 300 });
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 300, deposit: 1, sessions_opened: 0, lanes_joined: 0 });
        v.record = Some(record);
        let step = |case: &PalwFraudFilerCaseV1| {
            palw_fraud_filer_bootstrap_step_v2(
                PalwFilerRoleV1::PublicBond,
                Some(&v),
                &me,
                true,
                case,
                Some(PalwFraudFilerLanesV1 { now_daa: 300, form: case.fp_bootstrap.as_ref().unwrap().form }),
                |_| false,
            )
        };
        let PalwFraudFilerStepV1::DemandLanes { round, demand } = step(&case) else { panic!("input is publicly acquirable") };
        assert!(!round.is_empty());
        assert!(round.iter().all(|unit| matches!(unit, PalwLegacyHeldUnitV2::PromptIds { .. })));
        assert!(
            demand.iter().all(|probe| matches!(probe, PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::PromptIds { .. } }))
        );
        case.constraint_done = true;
        let PalwFraudFilerStepV1::DemandLanes { round, .. } = step(&case) else { panic!("ordinary parallel input round") };
        assert!(
            round.iter().any(|unit| matches!(unit, PalwLegacyHeldUnitV2::StepNode { .. })),
            "verified selection permits arithmetic batching"
        );
        let mut descriptor = input::tests::payload();
        descriptor.commitment.job.version = kaspa_consensus_core::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION;
        case.fp_bootstrap.as_mut().unwrap().payload = Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(descriptor));
        let PalwFraudFilerStepV1::DemandLanes { round, .. } = step(&case) else { panic!("public prefix input remains acquirable") };
        assert!(
            round.iter().all(|unit| matches!(unit, PalwLegacyHeldUnitV2::PromptIds { .. })),
            "prefix semantics precede arithmetic obligations too"
        );
        case.prefix_complete = true;
        let PalwFraudFilerStepV1::DemandLanes { round, .. } = step(&case) else { panic!("verified prefix permits arithmetic round") };
        assert!(round.iter().any(|unit| matches!(unit, PalwLegacyHeldUnitV2::StepNode { .. })));
    }

    #[test]
    fn selected_source_queue_preserves_exact_paid_bundle_and_child_collection_clock() {
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1},
            palw_selected_event_source_v1::PalwSelectedEventCoordinateV1,
            palw_staged_bundle_v1::{PalwStagedBundlePlanV1, PalwStagedBundleProgressV1, PalwStagedFamilyV1},
            palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        };
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let resource = PalwRenderingResourcePlanV1::derive(0, 1, config.prices).unwrap();
        let plan = PalwStagedBundlePlanV1::derive_v1(
            300,
            vec![(PalwStagedFamilyV1::Constraint, resource), (PalwStagedFamilyV1::Tokenizer, resource)],
            config,
        )
        .unwrap();
        let c = candidate(3, 300, false);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        let progress = PalwStagedOwnedProgressV1 {
            snapshot_block: kaspa_consensus_core::BlockHash::from_u64_word(300),
            now_daa: plan.stages[0].terms.open_until_daa,
            claim: c.claim_id,
            reserver: bond(1),
            reserved_daa: 300,
            terms: plan.stages[0].terms,
            prefix_custodians: None,
            prefix_page_root: None,
            seed_id: None,
            cursor: None,
            bundle: Some(PalwStagedBundleProgressV1 {
                bundle_id: plan.id_v1(c.claim_id),
                plan: plan.clone(),
                active_index: None,
                pending_seed: None,
                inventory: None,
            }),
        };
        let movement = PalwConstraintStreamMoveV1 {
            version: 1,
            claim: c.claim_id,
            reserver: bond(1),
            reserved_daa: 300,
            seed_id: plan.id_v1(c.claim_id),
            phase: PalwConstraintStreamPhaseV1::PrefixCourt,
            action: PalwConstraintStreamActionV1::SelectedEventCollect {
                coordinate: PalwSelectedEventCoordinateV1 { row: 3, tile: 1023 },
                demand_id: Hash64::from_u64_word(5),
            },
            signature: Vec::new(),
        };
        let mut pending = Vec::new();
        let mut due = HashMap::new();
        PalwPanelService::fraud_filer_stage_queue_v1(
            &mut book,
            c.claim_id,
            PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement.clone()) },
            &progress,
            progress.now_daa,
            &mut pending,
            &mut due,
        )
        .unwrap();
        assert_eq!(due[&(c.claim_id, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)], progress.terms.collect_until_daa - 1);
        assert_ne!(progress.terms.collect_until_daa, plan.terms.collect_until_daa);
        let flat_collect = PalwConstraintStreamMoveV1 { action: PalwConstraintStreamActionV1::FlatSourceCollect, ..movement.clone() };
        PalwPanelService::fraud_filer_stage_queue_v1(
            &mut book,
            c.claim_id,
            PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(flat_collect.clone()) },
            &progress,
            progress.now_daa,
            &mut pending,
            &mut due,
        )
        .unwrap();
        assert_eq!(due[&(c.claim_id, PALW_FRAUD_FILER_STAGED_ROUND_V1, false)], progress.terms.collect_until_daa - 1);
        assert!(!PalwPanelService::fraud_filer_bundle_control_v1(
            &progress,
            &PalwConstraintStreamMoveV1 { seed_id: Hash64::from_u64_word(999), ..flat_collect },
        ));
        for wrong in [
            PalwConstraintStreamMoveV1 { seed_id: Hash64::from_u64_word(999), ..movement.clone() },
            PalwConstraintStreamMoveV1 { reserver: bond(2), ..movement.clone() },
        ] {
            assert!(
                PalwPanelService::fraud_filer_stage_queue_v1(
                    &mut book,
                    c.claim_id,
                    PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(wrong) },
                    &progress,
                    progress.now_daa,
                    &mut pending,
                    &mut due
                )
                .is_err()
            );
        }
        let case = book.cases.get_mut(&c.claim_id).unwrap();
        case.verdict = PalwFraudFilerVerdictV1::SemanticPending;
        case.arithmetic_matched = true;
        case.staged_bundle_pending = true;
        assert!(!case.may_finish_semantics_v1(true));
    }

    #[test]
    fn owner_history_recovery_moves_name_the_bundle_frame_and_the_active_stage_only() {
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_history_custody_v1::PalwHistoryRecoveryActionV1,
            palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1},
            palw_staged_bundle_v1::{PalwStagedBundlePlanV1, PalwStagedBundleProgressV1, PalwStagedFamilyV1},
            palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        };
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let resource = PalwRenderingResourcePlanV1::derive(0, 1, config.prices).unwrap();
        let plan = PalwStagedBundlePlanV1::derive_v1(300, vec![(PalwStagedFamilyV1::PublicPrefix, resource)], config).unwrap();
        let c = candidate(4, 300, false);
        let stage = Hash64::from_u64_word(77);
        let progress = PalwStagedOwnedProgressV1 {
            snapshot_block: kaspa_consensus_core::BlockHash::from_u64_word(301),
            now_daa: plan.stages[0].terms.accepted_daa + 1,
            claim: c.claim_id,
            reserver: bond(1),
            reserved_daa: 300,
            terms: plan.stages[0].terms,
            prefix_custodians: None,
            prefix_page_root: None,
            seed_id: Some(stage),
            cursor: None,
            bundle: Some(PalwStagedBundleProgressV1 {
                bundle_id: plan.id_v1(c.claim_id),
                plan: plan.clone(),
                active_index: Some(0),
                pending_seed: None,
                inventory: None,
            }),
        };
        let demand = PalwConstraintStreamMoveV1 {
            version: 1,
            claim: c.claim_id,
            reserver: bond(1),
            reserved_daa: 300,
            seed_id: plan.id_v1(c.claim_id),
            phase: PalwConstraintStreamPhaseV1::PrefixCourt,
            action: PalwConstraintStreamActionV1::HistoryRecovery {
                stage_seed: stage,
                request_head: Hash64::from_u64_word(5),
                sequence: 3,
                demand_opened_daa: 0,
                action: PalwHistoryRecoveryActionV1::Demand,
            },
            signature: Vec::new(),
        };
        assert!(PalwPanelService::fraud_filer_bundle_control_v1(&progress, &demand));
        let collect = PalwConstraintStreamMoveV1 {
            action: PalwConstraintStreamActionV1::HistoryRecovery {
                stage_seed: stage,
                request_head: Hash64::from_u64_word(5),
                sequence: 3,
                demand_opened_daa: 310,
                action: PalwHistoryRecoveryActionV1::Collect,
            },
            ..demand.clone()
        };
        assert!(PalwPanelService::fraud_filer_bundle_control_v1(&progress, &collect));
        // A custodian's signed answer is never an owner move, another stage is not this cursor,
        // and the stage seed is not the bundle frame.
        let answer = PalwConstraintStreamMoveV1 {
            action: PalwConstraintStreamActionV1::HistoryRecovery {
                stage_seed: stage,
                request_head: Hash64::from_u64_word(5),
                sequence: 3,
                demand_opened_daa: 310,
                action: PalwHistoryRecoveryActionV1::Finish { answerer: bond(2) },
            },
            ..demand.clone()
        };
        assert!(!PalwPanelService::fraud_filer_bundle_control_v1(&progress, &answer));
        let other_stage = PalwConstraintStreamMoveV1 {
            action: PalwConstraintStreamActionV1::HistoryRecovery {
                stage_seed: Hash64::from_u64_word(78),
                request_head: Hash64::from_u64_word(5),
                sequence: 3,
                demand_opened_daa: 0,
                action: PalwHistoryRecoveryActionV1::Demand,
            },
            ..demand.clone()
        };
        assert!(!PalwPanelService::fraud_filer_bundle_control_v1(&progress, &other_stage));
        assert!(!PalwPanelService::fraud_filer_bundle_control_v1(&progress, &PalwConstraintStreamMoveV1 { seed_id: stage, ..demand }));
    }

    #[test]
    fn staged_collection_queue_keeps_the_paid_clock_and_requires_accepted_cursor_progress() {
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
            palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
            palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1},
            palw_staged_court_state_v1::PalwStagedOwnedProgressV1,
        };
        let c = candidate(3, 300, false);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        let prices = PalwStagedPublicCourtConfigV1::network_v1().prices;
        let plan = PalwRenderingResourcePlanV1::derive(0, 1, prices).unwrap();
        let terms = PalwRenderingClaimTermsV1::derive(300, plan, 100, 100, 10).unwrap();
        let progress = PalwStagedOwnedProgressV1 {
            snapshot_block: kaspa_consensus_core::BlockHash::from_u64_word(300),
            now_daa: terms.stream_until_daa,
            claim: c.claim_id,
            reserver: bond(1),
            reserved_daa: 300,
            terms,
            prefix_custodians: None,
            prefix_page_root: None,
            bundle: None,
            seed_id: Some(Hash64::from_u64_word(9)),
            cursor: Some(kaspa_consensus_core::palw_staged_court_state_v1::PalwStagedOwnedCursorV1::Prefix {
                moves_used: 1,
                cursor: kaspa_consensus_core::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1::Complete,
                completed: None,
            }),
        };
        let object = |demand| PalwConsensusObjectV2::PublicCourtStreamMoveV1 {
            movement: Box::new(PalwConstraintStreamMoveV1 {
                version: 1,
                claim: c.claim_id,
                reserver: bond(1),
                reserved_daa: 300,
                seed_id: Hash64::from_u64_word(9),
                phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                action: PalwConstraintStreamActionV1::PrefixPublicCollect {
                    cursor_id: Hash64::from_u64_word(10),
                    demand_id: Hash64::from_u64_word(demand),
                },
                signature: Vec::new(),
            }),
        };
        let mut pending = Vec::new();
        let mut due = HashMap::new();
        let now = progress.now_daa;
        let key = (c.claim_id, PALW_FRAUD_FILER_STAGED_ROUND_V1, false);
        let first = object(11);
        PalwPanelService::fraud_filer_stage_queue_v1(&mut book, c.claim_id, first.clone(), &progress, now, &mut pending, &mut due)
            .unwrap();
        assert_eq!(due[&key], terms.collect_until_daa - 1, "collection is eligible after streaming ends");
        assert_eq!(pending.len(), 1);
        PalwPanelService::fraud_filer_stage_queue_v1(&mut book, c.claim_id, first.clone(), &progress, now, &mut pending, &mut due)
            .unwrap();
        assert_eq!(pending.len(), 1, "one accepted cursor has one queued carrier");
        pending.clear();
        PalwPanelService::fraud_filer_stage_queue_v1(&mut book, c.claim_id, first, &progress, now, &mut pending, &mut due).unwrap();
        assert!(pending.is_empty(), "transport removal is not accepted progress and does not bypass retry spacing");
        let next = object(12);
        PalwPanelService::fraud_filer_stage_queue_v1(&mut book, c.claim_id, next.clone(), &progress, now, &mut pending, &mut due)
            .unwrap();
        assert_eq!(pending[0].3, next, "an actual new desired cursor supersedes old retry bookkeeping");
        let stamp = book.cases[&c.claim_id].staged_tokenizer_sent.unwrap();
        assert_eq!(stamp.2, 1);
        let mut wrong = object(13);
        let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = &mut wrong else { unreachable!() };
        movement.seed_id = Hash64::from_u64_word(99);
        assert!(
            PalwPanelService::fraud_filer_stage_queue_v1(&mut book, c.claim_id, wrong, &progress, now, &mut pending, &mut due)
                .is_err()
        );
        let mut wrong = object(14);
        let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = &mut wrong else { unreachable!() };
        movement.reserver = bond(2);
        assert!(
            PalwPanelService::fraud_filer_stage_queue_v1(&mut book, c.claim_id, wrong, &progress, now, &mut pending, &mut due)
                .is_err()
        );
        assert_eq!(pending.len(), 1, "rejected owner/seed substitutions do not mutate the paid queue");
    }

    #[test]
    fn retained_paid_authority_resumes_after_da_default_without_replay_or_new_legacy_authority() {
        use kaspa_consensus_core::{
            palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1},
            palw_staged_court_state_v1::PalwStagedPaidAuthorityV1,
        };
        let c = candidate(3, 300, false);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        let plan = PalwRenderingResourcePlanV1::derive(0, 1, PalwStagedPublicCourtConfigV1::network_v1().prices).unwrap();
        let terms = PalwRenderingClaimTermsV1::derive(300, plan, 100, 100, 10).unwrap();
        let authority = PalwStagedPaidAuthorityV1 { reserved_daa: 300, seed_id: Hash64::from_u64_word(9), terms };
        let mut closed = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 300 });
        closed.da_defaulted = true;
        closed.record = None;
        let facts = palw_fraud_filer_facts_v1(Some(&closed), &bond(1), false, false);
        assert_eq!(facts.outcome, Some(PalwFilerPhaseV1::DaDefault));
        assert!(!facts.reserved && !facts.reservable);
        book.observe_paid_authority_v1(c.claim_id, &authority, 301);
        assert!(book.pursued().contains(&c.claim_id), "a fresh verifier resumes authenticated paid semantic work");
        assert!(matches!(book.cases[&c.claim_id].verdict, PalwFraudFilerVerdictV1::SemanticPending));
        assert_eq!(book.next_replay(), None, "no input bootstrap or new replay is prerequisite to the paid cursor");
        assert_eq!(
            palw_fraud_filer_facts_v1(Some(&closed), &bond(1), false, false),
            facts,
            "paid continuation never manufactures ordinary live reservation"
        );
        book.settle(&c.claim_id, PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::DaDefault));
        book.observe_paid_authority_v1(c.claim_id, &authority, terms.collect_until_daa);
        assert!(book.pursued().is_empty(), "the original collection horizon is never extended");
        book.observe_paid_authority_v1(c.claim_id, &authority, 299);
        assert!(book.pursued().is_empty(), "a future reservation cannot authorize continuation");
        book.observe_paid_authority_v1(c.claim_id, &authority, 301);
        assert!(book.pursued().contains(&c.claim_id), "an earlier local DA-default settle does not erase accepted paid authority");
    }

    fn material_budget_binding() -> PalwStepBindingV2 {
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let payload = input::tests::payload();
        let job = &payload.commitment.job;
        let executed =
            backend.execute_free_prompt(job, &payload.prompt_token_ids.iter().map(|id| *id as usize).collect::<Vec<_>>()).unwrap();
        misaka_palw_base0::produce::base0_material_decode_any_v1(&executed.outcome.material).unwrap().binding().clone()
    }

    #[test]
    fn a_history_hole_never_publishes_a_completed_book_watermark() {
        let candidate = candidate(3, 300, false);
        let claim = candidate.claim_id;
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate], 400);
        book.judge(claim, Ok(run(Hash64::default(), Hash64::default())));
        let walk =
            PalwFraudFilerWalkV1 { floor: 300, anchor: Hash64::from_u64_word(999), anchor_daa: 999, next: None, incomplete: true };
        book.walked(walk, false, vec![]);
        assert!(book.walked_from.is_none() && book.walked_to.is_none());
        assert!(!book.descent_failed(&claim, "an old unavailable body is not proof that the public court is unreachable".into()));
        assert!(book.pursued().contains(&claim));
    }

    #[test]
    fn pruned_round_selects_only_answered_unread_material_needed_by_this_phase() {
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        let payload = input::tests::payload();
        assert!(payload.commitment.job.prompt_tokens > 0);
        case.fp_bootstrap = Some(PalwFraudFilerFpBootstrapV1 {
            payload: Arc::new(input::PalwFraudFilerFpDescriptorV1::Accepted(payload)),
            form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            ids: Vec::new(),
            chunk: 0,
            _reservation: None,
        });
        let old = PalwLegacyHeldUnitV2::PromptIds { chunk: 1 };
        let wanted = PalwLegacyHeldUnitV2::PromptIds { chunk: 0 };
        case.legacy_round = vec![old, wanted];
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 150 });
        assert!(case.missing_legacy_round_v1(&v, |_| false).is_none(), "unanswered lanes still use ordinary demand/default");
        v.answered = vec![PalwDaUnitV1::LegacyHeldV2(old), PalwDaUnitV1::LegacyHeldV2(wanted)];
        assert_eq!(case.missing_legacy_round_v1(&v, |_| false), Some(PalwLegacyProbeV1::HeldNode { unit: wanted }));
        assert!(case.missing_legacy_round_v1(&v, |unit| *unit == PalwDaUnitV1::LegacyHeldV2(wanted)).is_none());
        let input = case.fp_bootstrap.as_mut().unwrap();
        input.ids.resize(input.payload.job().prompt_tokens as usize, 0);
        assert!(case.missing_legacy_round_v1(&v, |_| false).is_none(), "already accumulated input never spends another session");
        let node = PalwLegacyHeldUnitV2::StepNode { level: 9, index: 0 };
        case.legacy_round = vec![node];
        v.answered.push(PalwDaUnitV1::LegacyHeldV2(node));
        assert!(case.missing_legacy_round_v1(&v, |_| false).is_some());
        case.frontiers.push(PalwLegacyFrontierV2 { level: 9, index: 0, below: 0, first: 0, nodes: vec![], leaf_hashes: None });
        assert!(
            case.missing_legacy_round_v1(&v, |_| false).is_none(),
            "a retained authenticated frontier is consumed without reacquisition"
        );
    }

    #[test]
    fn round_reacquisition_keeps_the_native_counter_wait_and_hard_deadline() {
        let me = bond(1);
        let unit = PalwLegacyHeldUnitV2::StepNode { level: 9, index: 0 };
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        case.binding = Some(material_budget_binding());
        case.legacy_round = vec![unit];
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 150 });
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 151, deposit: 1, sessions_opened: 3, lanes_joined: 5 });
        v.record = Some(record);
        v.answered.push(PalwDaUnitV1::LegacyHeldV2(unit));
        let probe = case.missing_legacy_round_v1(&v, |_| false).unwrap();
        assert!(case.reacquire_after_v1(&v, me, probe, 200).is_none());
        assert!(case.reacquire_after_v1(&v, me, probe, 229).is_none());
        let request = case.reacquire_after_v1(&v, me, probe, 230).unwrap();
        assert_eq!(request.session_number, 4);
        assert_eq!(request.reserved_daa, 151);
        assert_eq!(request.unit, PalwDaUnitV1::LegacyHeldV2(unit));
        assert_eq!(case.reacquire_after_v1(&v, me, probe, 230).unwrap(), request, "a signed retry never advances the native counter");
        v.hard_deadline_daa = 235;
        assert_eq!(case.reacquire_after_v1(&v, me, probe, 230).unwrap().valid_until_daa, 235);
        assert!(case.reacquire_after_v1(&v, me, probe, 236).is_none());
        v.hard_deadline_daa = 3800;
        v.record.as_mut().unwrap().live.get_mut(&me).unwrap().sessions_opened = PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1;
        assert!(case.reacquire_after_v1(&v, me, probe, 230).is_none(), "the bridge cannot waive the lifetime session cap");
    }

    #[test]
    fn a_parallel_round_reserves_every_retained_unit_atomically_before_reading() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};
        let form = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        case.binding = Some(material_budget_binding());
        // This tests the retrieval ledger, not admission or an authenticated 64-node fixture.
        case.legacy_round = (0..64).map(|index| PalwLegacyHeldUnitV2::StepNode { level: 9, index }).collect();
        let maximum = kaspa_consensus_core::palw_legacy_answer_chunks_v2::PALW_LEGACY_ANSWER_MAX_BYTES_V2;
        let per_unit =
            palw_fraud_filer_legacy_material_bytes_v1(case.binding.as_ref().unwrap(), case.legacy_round[0], maximum, form).unwrap();
        assert!(per_unit < 8 * 100_000, "node material uses its typed hash frontier bound, not a whole CKW ceiling");
        let short = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(64 * per_unit - 1), || None);
        assert!(case.reserve_legacy_material_v1(&short, maximum, form).is_err());
        assert_eq!(short.reserved_bytes(), 0, "the 64th refusal releases all new provisional tickets");
        assert!(case.legacy_material_tickets.is_empty());
        assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending), "memory refusal is not a verdict");
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(64 * per_unit), || None);
        case.reserve_legacy_material_v1(&ledger, maximum, form).unwrap();
        assert_eq!(ledger.reserved_bytes(), 64 * per_unit);
        case.reserve_legacy_material_v1(&ledger, maximum, form).unwrap();
        assert_eq!(ledger.reserved_bytes(), 64 * per_unit, "reread/resend never charges a second copy of the same unit");
        let worker = case.clone();
        case.legacy_round.clear();
        case.reserve_legacy_material_v1(&ledger, maximum, form).unwrap();
        assert_eq!(ledger.reserved_bytes(), 64 * per_unit, "a live worker's copies retain the same guards");
        drop(worker);
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    #[test]
    fn learned_frontier_memory_survives_round_rotation_until_settle_and_worker_drain() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1};
        let form = kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        case.binding = Some(material_budget_binding());
        let unit = PalwLegacyHeldUnitV2::StepNode { level: 9, index: 0 };
        case.legacy_round = vec![unit];
        let maximum = kaspa_consensus_core::palw_legacy_answer_chunks_v2::PALW_LEGACY_ANSWER_MAX_BYTES_V2;
        let per_unit = palw_fraud_filer_legacy_material_bytes_v1(case.binding.as_ref().unwrap(), unit, maximum, form).unwrap();
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(per_unit), || None);
        case.reserve_legacy_material_v1(&ledger, maximum, form).unwrap();
        // Lifetime bookkeeping only: native authentication is independently exercised by the
        // descent tests. Allocate the learned hashes only after their ticket has been admitted.
        case.frontiers.push(PalwLegacyFrontierV2 {
            level: 9,
            index: 0,
            below: 0,
            first: 0,
            nodes: vec![Hash64::default(); 512],
            leaf_hashes: None,
        });
        case.legacy_round = vec![PalwLegacyHeldUnitV2::StepNode { level: 9, index: 1 }];
        assert!(case.reserve_legacy_material_v1(&ledger, maximum, form).is_err());
        assert_eq!(ledger.reserved_bytes(), per_unit, "an unfunded new round preserves the admitted old frontier");
        assert!(case.legacy_material_tickets.contains_key(&unit));
        let worker = case.clone();
        let claim = case.candidate.claim_id;
        let mut book = PalwFraudFilerBookV1::default();
        book.cases.insert(claim, case);
        book.settle(&claim, PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::Convicted));
        assert!(book.cases[&claim].frontiers.is_empty());
        assert_eq!(ledger.reserved_bytes(), per_unit, "settling the book cannot release a still-running worker's bytes");
        drop(worker);
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    #[test]
    fn evicted_fp_replica_keeps_authenticated_job_and_its_byte_ticket() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(16_384), || None);
        let mut book = PalwFraudFilerBookV1::default();
        let claims: Vec<_> = (1..=3).map(|id| candidate(id, 100 + id, false)).collect();
        book.refresh(claims.clone(), 200);
        let job = input::tests::payload().commitment.job;
        for claim in &claims[..2] {
            book.started_replay(claim.claim_id);
            let mut replica = run(Hash64::default(), Hash64::default());
            replica.fp_job = Some(Box::new(job.clone()));
            replica._job_reservation = Some(
                ledger
                    .reserve(
                        PalwMemoryReservationKeyV1 { role: "deferred-public-test", class_id: claim.job.class_id, job: claim.claim_id },
                        1024,
                    )
                    .unwrap(),
            );
            replica._reservation = Some(
                ledger
                    .reserve(
                        PalwMemoryReservationKeyV1 {
                            role: "deferred-capture-test",
                            class_id: claim.job.class_id,
                            job: claim.claim_id,
                        },
                        4096,
                    )
                    .unwrap(),
            );
            book.judge(claim.claim_id, Ok(replica));
        }
        assert_eq!(ledger.reserved_bytes(), 10_240);
        book.make_replay_room(200, true);
        assert_eq!(ledger.reserved_bytes(), 6144, "eviction drops only the capture reservation");
        assert_eq!(book.cases[&claims[0].claim_id].public_fp_job_v1(), Some(&job));
        book.settle(&claims[0].claim_id, PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::Convicted));
        assert!(book.cases[&claims[0].claim_id].deferred_public.is_none());
        assert_eq!(ledger.reserved_bytes(), 5120, "settlement releases the retained public descriptor ticket");
        let key = PalwMemoryReservationKeyV1 { role: "new-wider-replay", class_id: claims[2].job.class_id, job: claims[2].claim_id };
        assert!(ledger.reserve(key.clone(), 12_000).is_err(), "one older replica can still block a wider profile below the count cap");
        assert!(book.evict_oldest_capture());
        assert_eq!(ledger.reserved_bytes(), 1024);
        let wider = ledger.reserve(key, 12_000).expect("the wider replay progresses after bounded pressure eviction");
        assert_eq!(book.held_runs(), 0);
        assert!(book.cases[&claims[1].claim_id].pursued());
        drop(wider);
        assert_eq!(ledger.reserved_bytes(), 1024);
    }

    #[test]
    fn binding_only_fp_bootstrap_authenticates_without_a_replay_and_keeps_its_ticket_until_settlement() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        let payload = input::tests::payload();
        let backend = crate::palw_panel::seat_s_tests::floor_backend();
        let executed = backend
            .execute_free_prompt(&payload.commitment.job, &payload.prompt_token_ids.iter().map(|id| *id as usize).collect::<Vec<_>>())
            .unwrap();
        let event = backend.disclose_trace_event(&executed.outcome.material, 0, 0).unwrap();
        let mut c = candidate(3, 300, false);
        c.job.free_prompt = true;
        c.job.class_id = payload.commitment.job.class_id;
        c.producer = PalwBondKeyV2(payload.commitment.job.executor_bond);
        c.job.execution_root = payload.commitment.execution_root;
        c.job.trace_root = payload.commitment.trace_root;
        c.job.output_root = payload.commitment.output_root;
        let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(4096), || None);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 300);
        let case = book.cases.get_mut(&c.claim_id).unwrap();
        case.fp_job_pending = true;
        case._fp_job_reservation = Some(Arc::new(
            ledger
                .reserve(PalwMemoryReservationKeyV1 { role: "binding-test", class_id: c.job.class_id, job: c.claim_id }, 2048)
                .unwrap(),
        ));
        assert!(case.pursued());
        assert!(case.fp_bootstrap.is_none());
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 300 });
        v.execution_root = c.job.execution_root;
        v.trace_root = c.job.trace_root;
        let me = bond(1);
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 300, deposit: 1, sessions_opened: 0, lanes_joined: 0 });
        v.record = Some(record);
        let probe = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
        assert_eq!(
            palw_fraud_filer_bootstrap_step_v1(PalwFilerRoleV1::PublicBond, Some(&v), &me, true, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(probe))
        );
        let mut bad = event.clone();
        match &mut bad {
            kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Flat { binding, .. }
            | kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Tiled { binding, .. }
            | kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::OutOfRange { binding } => {
                binding.job_context.job_id = Hash64::default()
            }
        }
        assert!(case.learn(probe, &PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(bad))).is_err());
        assert!(case.binding.is_none());
        case.learn(probe, &PalwDaBuiltAnswerV1::Rcore(PalwDaAnswerV1::Event(event))).unwrap();
        assert_eq!(case.runs, 0);
        assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending));
        assert_eq!(
            palw_fraud_filer_bootstrap_step_v1(PalwFilerRoleV1::PublicBond, Some(&v), &me, true, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait),
            "a correct binding alone cannot invent the missing full job"
        );
        let binding = case.binding.clone().unwrap();
        case.fp_expected_pin = Some(kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&payload.commitment));
        case.fp_job_disclosure_enabled = true;
        let job_probe = PalwLegacyProbeV1::HeldNode { unit: PalwLegacyHeldUnitV2::FpJob };
        assert_eq!(
            palw_fraud_filer_bootstrap_step_v1(PalwFilerRoleV1::PublicBond, Some(&v), &me, true, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(job_probe))
        );
        let mut wrong = payload.commitment.job.clone();
        wrong.job_nonce[0] ^= 1;
        assert!(
            case.learn(
                job_probe,
                &PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((binding.clone(), PalwLegacyHeldAnswerV2::FpJob(Box::new(wrong)))))
            )
            .is_err()
        );
        assert!(case.fp_descriptor.is_none());
        let mut owned = case.clone();
        owned
            .learn(
                job_probe,
                &PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((
                    binding,
                    PalwLegacyHeldAnswerV2::FpJob(Box::new(payload.commitment.job.clone())),
                ))),
            )
            .unwrap();
        assert!(case.fp_descriptor.is_none(), "the learner retains a separate state copy");
        case.accept_learned_v1(owned);
        assert!(matches!(case.fp_descriptor.as_deref(), Some(input::PalwFraudFilerFpDescriptorV1::Disclosed { .. })));
        assert!(case.fp_bootstrap.is_none(), "input memory is reserved later, before acquiring ids");
        assert_eq!(case.runs, 0);
        let mut own = run(c.job.execution_root, c.job.trace_root);
        own.output_root = c.job.output_root;
        assert!(matches!(book.judge(c.claim_id, Ok(own)), Some(PalwFraudFilerVerdictV1::Honest)));
        assert!(book.cases[&c.claim_id].release_pending, "a job later recovered must still release its earlier binding reservation");
        assert!(!book.cases[&c.claim_id].fp_job_pending);
        assert!(book.cases[&c.claim_id].fp_descriptor.is_none());
        assert_eq!(ledger.reserved_bytes(), 0);
    }

    #[test]
    fn selected_constraint_tiles_use_the_public_reservation_and_answer_engine_before_descent() {
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        case.verdict = PalwFraudFilerVerdictV1::Mismatch(Arc::new(run(Hash64::from_u64_word(1), Hash64::default())));
        let lane = kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES as u32;
        let plan = constraint::TiledPlanV1 { position: 3, committed: 0, witness: None, beat_lane: Some(lane) };
        case.constraint_plan = Some(plan);
        let me = bond(1);
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 300 });
        let step = |v: &PalwLegacyDisputeViewV1, case: &PalwFraudFilerCaseV1, read: &dyn Fn(&PalwDaUnitV1) -> bool| {
            palw_fraud_filer_constraint_step_v1(Some(v), &me, true, case, read).unwrap()
        };
        assert_eq!(step(&v, &case, &|_| false), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve));
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 300, deposit: 1, sessions_opened: 0, lanes_joined: 0 });
        v.record = Some(record);
        assert_eq!(
            step(&v, &case, &|_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(PalwLegacyProbeV1::Binding { row: 3, tile: 0 }))
        );
        let units = plan.units();
        assert_eq!(units.len(), 2);
        assert_eq!(
            step(&v, &case, &|unit| *unit == units[0]),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(PalwLegacyProbeV1::Binding { row: 3, tile: 1 }))
        );
        v.answered.push(units[1]);
        assert_eq!(
            step(&v, &case, &|unit| *unit == units[0]),
            PalwFraudFilerStepV1::AwaitAnswer(PalwLegacyProbeV1::Binding { row: 3, tile: 1 })
        );
        assert_eq!(step(&v, &case, &|_| true), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Wait));
        case.constraint_done = true;
        assert!(
            palw_fraud_filer_constraint_step_v1(Some(&v), &me, true, &case, |_| false).is_none(),
            "a complete public row which does not prove selection yields to arithmetic descent"
        );
    }

    /// A run whose roots are `execution_root` / `trace_root`, opening no range (the tests never read one).
    fn run(execution_root: Hash64, trace_root: Hash64) -> PalwFraudFilerRunV1 {
        PalwFraudFilerRunV1 {
            execution_root,
            trace_root,
            output_root: Hash64::default(),
            own_range: Box::new(|_, _| Err("no range".into())),
            legacy: None,
            own: None,
            fp_job: None,
            own_output_ids: Vec::new(),
            _job_reservation: None,
            _reservation: None,
        }
    }

    #[test]
    fn trace_only_and_output_only_mismatches_are_pursued_and_only_three_matching_roots_are_honest() {
        let c = candidate(3, 300, false);
        let claim = c.claim_id;
        for (trace_diff, output_diff) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![c.clone()], 10);
            let mut replay = run(c.job.execution_root, c.job.trace_root);
            if trace_diff {
                replay.trace_root = Hash64::from_u64_word(88);
            }
            if output_diff {
                replay.output_root = Hash64::from_u64_word(99);
            }
            let verdict = book.judge(claim, Ok(replay)).expect("held");
            if trace_diff || output_diff {
                assert!(matches!(verdict, PalwFraudFilerVerdictV1::Mismatch(_)));
                assert_eq!(book.walk_floor(), Some(300), "public binding backfill remains required");
            } else {
                assert!(matches!(verdict, PalwFraudFilerVerdictV1::Honest));
                assert_eq!(book.walk_floor(), None);
            }
        }
    }

    #[test]
    fn a_partial_history_page_cannot_end_a_pursuit_before_older_direct_evidence_is_read() {
        let c = candidate(3, 300, false);
        let claim = c.claim_id;
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c], 10);
        book.judge(claim, Ok(run(Hash64::default(), Hash64::default())));
        assert!(!book.descent_failed(&claim, "no successful history page yet".into()));
        let page = PalwFraudFilerWalkV1 {
            floor: 300,
            anchor: Hash64::from_u64_word(1000),
            anchor_daa: 1000,
            next: Some(Hash64::from_u64_word(500)),
            incomplete: false,
        };
        book.walked(page, false, vec![]);
        assert!(!book.descent_failed(&claim, "a pin is not read yet".into()));
        assert_eq!(book.held_runs(), 1);
        assert_eq!(book.walk_floor(), Some(300));
        book.walked(PalwFraudFilerWalkV1 { next: None, ..page }, false, vec![]);
        assert!(book.descent_failed(&claim, "complete history cannot supply a proof".into()));
        assert_eq!(book.held_runs(), 0);
        assert!(matches!(book.cases[&claim].verdict, PalwFraudFilerVerdictV1::Unjudged(_)));
    }

    #[test]
    fn another_acceptance_restarts_an_honest_or_mismatched_case_and_rejects_old_results() {
        let old = candidate(3, 300, false);
        let claim = old.claim_id;
        for mismatch in [false, true] {
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![old.clone()], 10);
            let result =
                if mismatch { run(Hash64::default(), old.job.trace_root) } else { run(old.job.execution_root, old.job.trace_root) };
            book.judge_replay(&old, Ok(result)).expect("current job");
            let old_capture = match &book.cases[&claim].verdict {
                PalwFraudFilerVerdictV1::Mismatch(run) => Some(Arc::downgrade(run)),
                _ => None,
            };
            let case = book.cases.get_mut(&claim).unwrap();
            case.frontiers.push(PalwLegacyFrontierV2 {
                level: 1,
                index: 0,
                below: 0,
                first: 0,
                nodes: vec![Hash64::from_u64_word(1)],
                leaf_hashes: None,
            });
            case.bisect = PalwLegacyBisectV1::new(64);
            case.legacy_wanted = Some(PalwLegacyHeldUnitV2::StepNode { level: 1, index: 0 });
            case.note_sent(PalwFraudFilerSentV1::Reserve, 10);
            case.direct_handed = 2;
            case.direct_asked_at = Some(10);
            let page = PalwFraudFilerWalkV1 {
                floor: 300,
                anchor: Hash64::from_u64_word(1000),
                anchor_daa: 1000,
                next: None,
                incomplete: false,
            };
            book.walked(page, false, vec![]);
            assert!(book.refresh(vec![old.clone()], 11).is_empty(), "an unchanged candidate retains its replay");
            assert_eq!(book.cases[&claim].frontiers.len(), 1);
            let mut new = old.clone();
            new.job.accepted_block = Hash64::from_u64_word(2000); // Same claim and all three roots, a different header-derived job.
            assert_eq!(book.refresh(vec![new.clone()], 12), BTreeSet::from([claim]));
            let case = &book.cases[&claim];
            assert!(matches!(case.verdict, PalwFraudFilerVerdictV1::Pending));
            assert!(case.frontiers.is_empty());
            assert_eq!(
                (case.bisect, case.legacy_wanted, case.sent, case.runs, case.direct_handed, case.direct_asked_at),
                (PalwLegacyBisectV1::new(0), None, None, 0, 0, None)
            );
            assert_eq!((book.walk, book.walked_from, book.walked_to), (None, None, None));
            if let Some(capture) = old_capture {
                assert!(capture.upgrade().is_none(), "old own capture is released");
            }
            // Even matching roots in a late old result cannot make the new header's job Honest; an old failure cannot spend a retry.
            assert!(book.judge_replay(&old, Ok(run(old.job.execution_root, old.job.trace_root))).is_none());
            assert!(book.judge_replay(&old, Err("an old worker's failure".into())).is_none());
            assert_eq!(book.cases[&claim].runs, 0);
            assert_eq!(book.next_replay(), Some(claim));
            assert!(matches!(
                book.judge_replay(&new, Ok(run(Hash64::default(), new.job.trace_root))),
                Some(PalwFraudFilerVerdictV1::Mismatch(_))
            ));
            assert_eq!(book.walk_floor(), Some(300), "new job's mismatch backfills from acceptance");
        }
    }

    #[tokio::test]
    async fn an_invalidated_blocking_replay_drains_under_its_reservation_before_the_next_starts() {
        use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
        for clear in [false, true] {
            let old = candidate(3, 300, false);
            let claim = old.claim_id;
            let mut new = old.clone();
            new.job.accepted_block = Hash64::from_u64_word(2000);
            let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(4096), || None);
            let reserved = ledger
                .reserve(
                    PalwMemoryReservationKeyV1 { role: PALW_FRAUD_FILER_REPLAY_ROLE_V1, class_id: old.job.class_id, job: claim },
                    2048,
                )
                .expect("reservation");
            let (started, ready) = tokio::sync::oneshot::channel();
            let (finish, wait) = std::sync::mpsc::channel();
            let job = old.job.clone();
            let handle = tokio::task::spawn_blocking(move || {
                started.send(()).expect("observer");
                wait.recv().expect("continue worker");
                let mut result = run(job.execution_root, job.trace_root);
                result._reservation = Some(reserved);
                Ok(result)
            });
            let mut book = PalwFraudFilerBookV1::default();
            book.refresh(vec![old.clone()], 10);
            book.running = Some(PalwFraudFilerReplayV1 { candidate: old, handle });
            tokio::time::timeout(std::time::Duration::from_secs(5), ready).await.expect("worker starts").expect("started");
            if clear {
                book.clear();
            }
            book.refresh(vec![new], 11);
            assert!(book.running.is_some(), "clear/invalidation must not detach a blocking worker");
            assert_eq!(book.next_replay(), None, "one worker remains in flight");
            assert_eq!(ledger.reserved_bytes(), 2048, "the live worker's memory stays reserved");
            finish.send(()).expect("finish old worker");
            let replay = book.running.take().expect("still observed");
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(5), replay.handle).await.expect("worker drains").expect("joined");
            assert_eq!(ledger.reserved_bytes(), 2048, "the returned capture still holds its reservation");
            assert!(book.judge_replay(&replay.candidate, result).is_none());
            assert_eq!(ledger.reserved_bytes(), 0, "discarding the stale capture releases it");
            assert_eq!(book.next_replay(), Some(claim));
            assert!(matches!(book.cases[&claim].verdict, PalwFraudFilerVerdictV1::Pending));
        }
    }

    #[test]
    fn restarting_public_history_discards_old_localization_and_resend_state_but_keeps_the_same_jobs_replay() {
        let c = candidate(3, 300, false);
        let claim = c.claim_id;
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone()], 10);
        book.judge_replay(&c, Ok(run(Hash64::default(), c.job.trace_root)));
        let case = book.cases.get_mut(&claim).unwrap();
        case.legacy_wanted = Some(PalwLegacyHeldUnitV2::StepNode { level: 1, index: 0 });
        case.note_sent(PalwFraudFilerSentV1::Reserve, 10);
        case.note_sent(PalwFraudFilerSentV1::Reserve, 40);
        assert_eq!(case.may_send(PalwFraudFilerSentV1::Reserve, 70), Err(()));
        let page = PalwFraudFilerWalkV1 {
            floor: 300,
            anchor: Hash64::from_u64_word(2000),
            anchor_daa: 1000,
            next: Some(claim),
            incomplete: false,
        };
        book.walked(page, true, vec![]);
        let case = &book.cases[&claim];
        assert!(case.pursued(), "unchanged job's own replay is still valid");
        assert_eq!((case.legacy_wanted, case.sent), (None, None));
        assert_eq!(case.may_send(PalwFraudFilerSentV1::Reserve, 70), Ok(true), "new branch can reserve afresh");
        assert_eq!(book.walk_floor(), Some(300));
    }

    #[test]
    fn unrelated_settled_case_cleanup_cannot_restart_a_pursuits_history_and_a_new_seat_leaves_the_public_lane() {
        let c = candidate(3, 300, false);
        let claim = c.claim_id;
        let other = candidate(4, 400, false);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![c.clone(), other.clone()], 10);
        book.judge_replay(&c, Ok(run(Hash64::default(), c.job.trace_root)));
        book.judge_replay(&other, Ok(run(other.job.execution_root, other.job.trace_root)));
        let page = PalwFraudFilerWalkV1 {
            floor: 300,
            anchor: Hash64::from_u64_word(1000),
            anchor_daa: 1000,
            next: Some(Hash64::from_u64_word(600)),
            incomplete: false,
        };
        book.walked(page, false, vec![]);
        assert_eq!(book.refresh(vec![c.clone()], 11), BTreeSet::from([other.claim_id]));
        assert_eq!(book.walk, Some(page), "unrelated expired/honest claims cannot continually rewind another pursuit");
        let mut seat = c.clone();
        seat.seat = true;
        assert_eq!(book.refresh(vec![seat], 12), BTreeSet::from([claim]));
        assert!(book.pursued().is_empty());
        assert_eq!(book.next_replay(), None, "the new Panel seat's replay belongs to its seat lane");
        assert!(book.judge_replay(&c, Ok(run(Hash64::default(), c.job.trace_root))).is_none());
    }

    /// **The book replays oldest first, one at a time, never a seat's claim, and holds at most two mismatches.**
    #[test]
    fn the_book_replays_oldest_first_and_bounds_what_it_holds() {
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate(3, 300, false), candidate(1, 100, true), candidate(2, 200, false), candidate(4, 400, false)], 10);
        assert_eq!(book.next_replay(), Some(Hash64::from_u64_word(2)), "the seat's claim is the seat's duties'");
        let honest = candidate(2, 200, false).job;
        book.judge(Hash64::from_u64_word(2), Ok(run(honest.execution_root, honest.trace_root)));
        assert!(matches!(book.cases[&Hash64::from_u64_word(2)].verdict, PalwFraudFilerVerdictV1::Honest));
        assert_eq!(book.next_replay(), Some(Hash64::from_u64_word(3)));
        book.judge(Hash64::from_u64_word(3), Ok(run(Hash64::from_u64_word(1), honest.trace_root)));
        book.judge(Hash64::from_u64_word(4), Ok(run(Hash64::from_u64_word(1), honest.trace_root)));
        assert_eq!(book.held_runs(), 2);
        book.refresh(vec![candidate(5, 50, false)], 11);
        assert_eq!(book.next_replay(), None, "two mismatches held: the next waits");
        assert_eq!(book.pursued(), vec![Hash64::from_u64_word(3), Hash64::from_u64_word(4)], "pursued cases survive the candidates");
        assert!(!book.cases.contains_key(&Hash64::from_u64_word(2)), "a settled case leaves with its candidate");
        // A failure on this host is run once more, then unjudged.
        book.settle(&Hash64::from_u64_word(3), PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::Convicted));
        let five = Hash64::from_u64_word(5);
        assert_eq!(book.next_replay(), Some(five));
        book.judge(five, Err("refused".into()));
        assert!(matches!(book.cases[&five].verdict, PalwFraudFilerVerdictV1::Pending));
        book.judge(five, Err("refused".into()));
        assert!(matches!(book.cases[&five].verdict, PalwFraudFilerVerdictV1::Unjudged(_)));
    }

    /// **Shared progress**: a probe the chain already answered is read (once the walk has the answer), never demanded; a reservation
    /// comes first; the engine's terminal is demanded even when answered (the fold adjudicates it).
    #[test]
    fn a_unit_the_chain_answered_is_read_never_demanded() {
        let me = bond(1);
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate(3, 300, false)], 10);
        let claim = Hash64::from_u64_word(3);
        book.judge(claim, Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        let case = &book.cases[&claim];
        let mut v = view(3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 150 });
        let step = |v: &PalwLegacyDisputeViewV1, reservable, read: bool| {
            palw_fraud_filer_step_v1(PalwFilerRoleV1::PublicBond, Some(v), &me, reservable, case, |_| read)
        };
        assert_eq!(step(&v, true, false), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Reserve));
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(me, PalwDisputeReservationRowV1 { reserved_daa: 151, deposit: 1, sessions_opened: 0, lanes_joined: 0 });
        v.record = Some(record);
        let binding = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
        assert_eq!(step(&v, false, false), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Demand(binding)));
        v.answered.push(binding.unit());
        assert_eq!(step(&v, false, false), PalwFraudFilerStepV1::AwaitAnswer(binding));
        assert_eq!(step(&v, false, true), PalwFraudFilerStepV1::Learn(binding));
        v.answered.clear();
        v.flat_answered_rows = 1;
        assert_eq!(
            step(&v, false, false),
            PalwFraudFilerStepV1::AwaitAnswer(binding),
            "historical Flat coverage also waits for public bytes"
        );
        assert!(!v.unit_was_answered_v1(&PalwDaUnitV1::Event { row: 0, tile: 1 }), "Flat never covers another tile");
        assert!(!v.unit_was_answered_v1(&PalwDaUnitV1::Event { row: 1, tile: 0 }), "Flat never covers a row past its bound");
        let mut fresh = book.cases[&claim].clone();
        assert!(fresh.reacquire_after_v1(&v, me, binding, 200).is_none());
        assert!(fresh.reacquire_after_v1(&v, me, binding, 230).is_some(), "historical Flat coverage uses numbered reacquisition");
        // The outcome ends it whatever else stands.
        v.phase =
            PalwClaimPhaseV2::Voided { voided_daa: 200, reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::CourtFraud };
        assert_eq!(step(&v, false, true), PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Convicted)));
        assert_eq!(
            palw_fraud_filer_step_v1(PalwFilerRoleV1::PublicBond, None, &me, false, case, |_| false),
            PalwFraudFilerStepV1::Engine(PalwFilerActionV1::Done(PalwFilerPhaseV1::Expired)),
            "a retired claim's pursuit expires"
        );
    }

    /// **An item is sent once, re-sent once after it was lost, and never a third time**; a different item is never held back.
    #[test]
    fn an_item_is_sent_at_most_twice() {
        let mut case = PalwFraudFilerCaseV1::new(candidate(3, 300, false));
        let reserve = PalwFraudFilerSentV1::Reserve;
        assert_eq!(case.may_send(reserve, 10), Ok(true));
        case.note_sent(reserve, 10);
        assert_eq!(case.may_send(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1 - 1), Ok(false));
        assert_eq!(case.may_send(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1), Ok(true));
        case.note_sent(reserve, 10 + PALW_FRAUD_FILER_RESEND_DAA_V1);
        assert_eq!(case.may_send(reserve, 10 + 3 * PALW_FRAUD_FILER_RESEND_DAA_V1), Err(()));
        assert_eq!(case.may_send(PalwFraudFilerSentV1::Demand(PalwLegacyProbeV1::Binding { row: 0, tile: 0 }), 11), Ok(true));
    }

    /// **The walk reads from the oldest pursued claim after a start, then only what the tip added (less a reorg margin).**
    #[test]
    fn the_walk_starts_at_the_oldest_pursuit_then_follows_the_tip() {
        let mut book = PalwFraudFilerBookV1::default();
        assert_eq!(book.walk_floor(), None, "nothing pursued: no walk");
        book.refresh(vec![candidate(3, 300, false), candidate(4, 400, false)], 10);
        book.judge(Hash64::from_u64_word(4), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        assert_eq!(book.walk_floor(), Some(400));
        book.walked(
            PalwFraudFilerWalkV1 {
                floor: 400,
                anchor: Hash64::from_u64_word(1_000),
                anchor_daa: 1_000,
                next: None,
                incomplete: false,
            },
            false,
            Vec::new(),
        );
        assert_eq!(book.walk_floor(), Some(1_000 - PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1));
        book.judge(Hash64::from_u64_word(3), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        assert_eq!(book.walk_floor(), Some(300), "an older pursuit walks back to its acceptance");
    }

    #[test]
    fn partial_pages_and_newer_pursuits_do_not_inherit_a_completed_watermark() {
        let mut book = PalwFraudFilerBookV1::default();
        book.refresh(vec![candidate(3, 300, false), candidate(4, 400, false)], 10);
        book.judge(Hash64::from_u64_word(3), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        let anchor = Hash64::from_u64_word(1_000);
        let partial =
            PalwFraudFilerWalkV1 { floor: 300, anchor, anchor_daa: 1_000, next: Some(Hash64::from_u64_word(800)), incomplete: false };
        book.walked(partial, false, Vec::new());
        assert_eq!((book.walked_from, book.walked_to), (None, None));
        assert_eq!(book.walk_floor(), Some(300));
        book.walked(PalwFraudFilerWalkV1 { next: None, ..partial }, false, Vec::new());
        assert_eq!(book.walk_floor(), Some(1_000 - PALW_FRAUD_FILER_WALK_MARGIN_DAA_V1));
        // The newer claim's responses were not collected while its replay was pending, although the walk crossed its acceptance.
        book.judge(Hash64::from_u64_word(4), Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        assert_eq!(book.walk_floor(), Some(300), "a new pursuit forces backfill even when its acceptance is above the old floor");
        assert_eq!((book.walked_from, book.walked_to, book.walk), (None, None, None));
        assert!(book.stale(10), "start backfill on the next tick even if DAA has not advanced");
    }

    #[test]
    fn selecting_an_already_answered_legacy_unit_backfills_instead_of_inheriting_a_recent_watermark() {
        let mut book = PalwFraudFilerBookV1::default();
        let claim = Hash64::from_u64_word(3);
        book.refresh(vec![candidate(3, 300, false)], 10);
        book.judge(claim, Ok(run(Hash64::from_u64_word(1), Hash64::default())));
        let complete = PalwFraudFilerWalkV1 {
            floor: 300,
            anchor: Hash64::from_u64_word(1_000),
            anchor_daa: 1_000,
            next: None,
            incomplete: false,
        };
        book.walked(complete, false, Vec::new());
        assert_eq!(book.walk_floor(), Some(936));
        let root = PalwLegacyHeldUnitV2::StepNode { level: 16, index: 0 };
        book.want_legacy(claim, root);
        assert_eq!(book.walk_floor(), Some(300));
        assert!(book.stale(10), "the selected unit may predate the last walk while it was unselected");
        book.walked(complete, false, Vec::new());
        book.want_legacy(claim, root);
        assert_eq!(book.walk_floor(), Some(936), "waiting for the same unit does not continually restart a partial read");
        book.want_legacy(claim, PalwLegacyHeldUnitV2::StepNode { level: 8, index: 1 });
        assert_eq!(book.walk_floor(), Some(300), "the next selected node also needs its older bytes");
        book.settle(&claim, PalwFraudFilerVerdictV1::Settled(PalwFilerPhaseV1::Convicted));
        assert_eq!(book.cases[&claim].legacy_wanted, None);
    }

    #[test]
    fn the_history_cache_cannot_accumulate_unused_range_units() {
        use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
        let range = |first, count| PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first, count });
        assert!(palw_fraud_filer_cache_unit_v1(&PalwDaUnitV1::Event { row: 0, tile: 0 }, None));
        assert!(!palw_fraud_filer_cache_unit_v1(&PalwDaUnitV1::Event { row: 1, tile: 0 }, None));
        // G14-V: only the ONE unit the case's descent reads now is cached — any position, never a stranger's other demands.
        let wanted = range(1 << 30, 1_024);
        for first in [0u64, 1_024, 1 << 30, (1 << 30) + 1_024] {
            for count in [1, 1_024] {
                assert_eq!(palw_fraud_filer_cache_unit_v1(&range(first, count), Some(&wanted)), range(first, count) == wanted);
            }
        }
        assert!(!palw_fraud_filer_cache_unit_v1(&range(0, 1_024), None));
        let event = PalwDaUnitV1::Event { row: 3, tile: 7 };
        assert!(palw_fraud_filer_cache_unit_v1(&event, Some(&event)));
        assert!(!palw_fraud_filer_cache_unit_v1(&PalwDaUnitV1::Event { row: 3, tile: 8 }, Some(&event)));
    }

    /// **The tick wires the filer behind no flag, after lane B, with the seat's replay room** (a source pin).
    #[test]
    fn the_tick_wires_the_filer_behind_no_flag() {
        let panel = include_str!("palw_panel.rs");
        let tick = panel.find("self.fraud_filer_tick_v1(").expect("the tick calls the filer");
        let operator = panel.find("self.operator_da_tick_v1(").expect("lane B");
        assert!(tick > operator, "after lane B");
        assert!(panel[tick..tick + 400].contains("seat_replays.has_room(false)"), "the replay yields to the seat's slots");
    }
}
