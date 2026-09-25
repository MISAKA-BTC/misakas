//! **ADR-0152 v3.1 Phase 2, P2-8b / P2-8d (and P2-8e's hook): the replay-mismatch contradiction
//! builder** — what a seat whose OWN replay of a claim's job does not reproduce the claim's committed
//! roots turns into a proof, from the claim's SERVED capture and its LOCAL one.
//!
//! **Why it exists** (§3.9's garbage row, SR-8, X10; `phase2-plan.md` F14 and §5.7). Before this, a
//! `StepArithmetic` contradiction existed only in the capture sampler's arm: a seat that replayed a
//! claim and found its roots wrong knew the producer lied and could prove nothing — its replay yields
//! roots, the interval arm a leaf or a block, the segment resume a bool. The garbage strategy (the
//! right job's context, a well-formed garbage trace, DA answered) was refuted only where a capture
//! sampler happened to draw the lying leaf, 4 samples in `n`. After X10 an unrefuted garbage claim
//! costs 0, so P2-8b is REQUIRED (§7.3): every seat that replays must be able to build the proof.
//!
//! **What [`palw_replay_contradiction_v1`] does, in order** — one pure function, which kaspad's filer
//! runs off the panel's loop under its ledger reservation (`kaspad/src/palw_filer_replay.rs`) and the
//! real-claim suite runs on a producer-built claim (T54f), so the proof the node files is the proof
//! the suite folds:
//!
//! 1. **The served capture is the claim's**: the family's own `verify_material` against the roots the
//!    seat arms read (`PalwClaimRootsV1`: the claim's roots, the block's anchor and draw, the recorded
//!    job pin). A capture that fails it but carries the claim's own binding (authenticated: its
//!    committed root is the claim's and its fields rebuild it, `verify_binding_v1`) goes to step 5
//!    alone: a family's seat rule refuses exactly the garbage-logits lie (base0's SEAT-0 head rule:
//!    committed rows that are not the head's step outputs), so that producer's capture never
//!    verifies anywhere. Anything else is not the committed execution and proves nothing about the
//!    producer. [`palw_replay_served_standing_v1`] is the same reading, exported so a node sorts its
//!    gossiped candidates by it before it pays for a replay.
//! 2. **The local capture answers the same job and disagrees**: the served capture is one the family
//!    lays out (`capture_shape`; bytes in a retention it does not read are
//!    [`PalwReplayNothingV1::ServedNotBisectable`]), the local one's
//!    shape (job context, leaf count) is the served one's, and it does NOT reproduce the claim's
//!    roots. A seat whose local run is of another job, or whose run agrees with the claim, has no
//!    mismatch to prove and abstains — the liveness half of "the mismatch must be the seat's replay
//!    against the claim's committed roots, under the claim's job".
//! 3. **Bisect** ([`palw_replay_bisect_v1`]): the family's prefix commitment
//!    (`PalwExecutionBackendV1::bisect_prefix_state` — the verb ADR-0027's ladder is built on) of both
//!    captures at the pinned midpoint (`palw_bisect::bisect_midpoint_v1`, the ladder's own), one
//!    whole-space rung and `⌈log₂ n⌉` halvings: `O(log n)` rungs, each reserved by the caller before it
//!    runs, and a budget no caller can exceed ([`palw_replay_bisect_rungs_v1`]). A stop that is this
//!    host's (a rung the ledger refused, the local prefix unreadable —
//!    [`PalwReplayNothingV1::is_this_hosts`]) is reported as such, with the rungs it spent, so the
//!    caller runs the claim again instead of settling it. It returns the FIRST leaf the two step
//!    trees disagree at: every leaf before it is the honest execution's, so the step there read
//!    honest inputs and wrote a wrong output — which is exactly what a step refutation proves.
//! 4. **At that leaf**:
//!    * a fused-attention leaf (`palw_da_step_leaf_is_fused_v1`, the fold's own predicate) is P2-8e's
//!      held dissection — [`PalwReplayFindingV1::NeedsDissection`], ONE hook, not implemented here
//!      (it waits for the audit's A-held line, §3.9 C1–C5, tag 57);
//!    * otherwise the refutation is opened FROM THE SERVED CAPTURE (the producer's committed tiles,
//!      its operand rows, the prompt tile in the network's carriage — the capture arm's recipe) and
//!      held to the fold's own predicate (`palw_false_valid_convicts_execution_v2`, what kind 4's
//!      adjudicator reads for `StepArithmetic`): convicting → [`PalwReplayFindingV1::Refutes`];
//!    * served material that cannot open the leaf → [`PalwReplayFindingV1::DemandLeaf`] (P2-8d): the
//!      seat demands the leaf's evidence in a data-availability session (DA-3's named `StepLeaf`,
//!      J-6's garbage path), and the producer discloses it or defaults (S1).
//! 5. **Step trees that agree everywhere, or a capture only its binding ties to the claim**, leave
//!    F1c: the committed logits rows are compared with the local ones, and at the first bent lane
//!    `LogitsNotStepOutput = 12` is built (the served event, the head leaf opened from the served
//!    capture) and held to the fold's own predicate (`palw_logits_not_step_output_fault_v1`, which
//!    authenticates the event and the head leaf against the claim's own roots) — "garbage logits on an
//!    honest step tree". No bisection runs and no leaf is demanded on a capture no seat verified.
//!
//! **Liveness: never an honest producer.** Nothing is returned for filing that the fold's own
//! adjudicator predicate does not convict, so an honest producer's claim — whose committed steps and
//! rows ARE the correct function of its committed inputs — yields `Nothing` whatever this seat's
//! replay did (a nondeterministic or mis-built backend included): at worst the seat wasted its own
//! replay. A `DemandLeaf` is returned only when the leaf is LOCATED on the claim's own verified
//! capture and the proof could not be opened from it — never when a proof was built and did not
//! convict.
//!
//! **Node policy, consensus-inert.** Nothing here is read by the fold; it builds objects the fold
//! then judges by its own rules ([`palw_replay_executor_refuted_object_v1`] encodes kind 4's evidence
//! exactly as `palw_check_executor_refuted_v1` decodes it). Read-side pieces in core for the same
//! reason as P2-6's and P2-7's builders (`palw_da_rcore_v1`): the real-claim suite and the node run
//! one builder.

use crate::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use crate::palw_offence_attribution_v1::{
    PALW_EXECUTOR_REFUTED_VERSION_V1, PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES, PalwExecutorRefutedEvidenceV1, PalwOffenceTargetV1,
    palw_executor_refuted_admission_v1, palw_executor_refuted_offence_id_v1, palw_false_valid_convicts_execution_v2,
    palw_logits_not_step_output_fault_v1,
};
use crate::palw_offence_v1::{PalwOffenceKindV1, PalwOffenceVerifyError, PalwPanelContradictionV1, palw_offence_evidence_digest_v1};
use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, PalwPromptIdsOpeningV1};
use crate::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use crate::palw_step_leg::PalwStepBindingV2;
use crate::palw_step_refute::{PalwExecutionStepRefutationV1, PalwTraceEventDisclosureV1};
use kaspa_hashes::Hash64;

/// **The most rungs any bisection takes**: one whole-space rung and one per halving of the widest
/// space a ladder opens (`palw_bisect::PALW_BISECT_MAX_SPACE`, 2^40) — ADR-0027's bound, and the
/// step ladder's (`PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`). A claim's own budget is tighter:
/// [`palw_replay_bisect_rungs_v1`] of its leaf count.
pub const PALW_REPLAY_BISECT_MAX_RUNGS_V1: u32 = 1 + 40;

/// **Rows × tiles of committed logits F1c's scan reads before it gives up** — the scan is linear (the
/// rows carry no prefix commitment to bisect), so it is bounded by a constant: 16 tiled rows of the
/// widest head vocabulary (`PALW_LOGITS_HEAD_MAX_VOCAB_V1 / PALW_LOGITS_TILE_LANES` = 256 tiles), or
/// every row of a flat scheme, whose one disclosure carries them all. The attempt lane decodes one
/// row on testnet-12.
pub const PALW_REPLAY_LOGITS_SCAN_MAX_V1: u32 = 4_096;

/// **The rung budget of one claim's bisection**: `1 + ⌈log₂ n⌉` — the whole-space rung and the
/// halvings of `[0, n)` down to one leaf. Exactly what [`palw_replay_bisect_v1`] can spend; the O(log n)
/// bound the filer reserves against (T54f's bound test holds it on a long trace).
pub fn palw_replay_bisect_rungs_v1(step_leaf_count: u64) -> u32 {
    if step_leaf_count <= 1 { 1 } else { 1 + (u64::BITS - (step_leaf_count - 1).leading_zeros()) }
}

/// Which execution a prefix commitment is read off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwReplaySideV1 {
    /// The claim's committed execution, as its producer served it.
    Served,
    /// This seat's own execution of the claim's job.
    Local,
}

/// Where the bisection ended: the first leaf the two step trees disagree at, and the rungs it took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwReplayBisectV1 {
    pub leaf: u64,
    pub rungs: u32,
}

/// **Why a bisection named no leaf.** Every one is "nothing to prove at a step" — never a finding
/// against the producer.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwReplayBisectStopV1 {
    #[error("the step space is empty")]
    EmptySpace,
    #[error("the step space of {0} leaves is wider than a bisection opens (2^40)")]
    SpaceTooWide(u64),
    /// The whole-space rung agrees: the step trees are one tree (F1c's case, or an output-root
    /// mismatch, which is `OutputMismatch`'s).
    #[error("the two step trees agree at every leaf ({rungs} rungs)")]
    StepTreesAgree { rungs: u32 },
    #[error("the {side:?} execution's prefix state at leaf {index} cannot be read (rung {rung})")]
    Unreadable { side: PalwReplaySideV1, index: u64, rung: u32 },
    #[error("the bisection's budget of {budget} rungs is spent")]
    OverBudget { budget: u32 },
    /// The caller's reservation for the rung was refused: nothing past it ran.
    #[error("rung {rung} was not reserved: {why}")]
    RungRefused { rung: u32, why: String },
}

impl PalwReplayBisectStopV1 {
    /// **The rungs the search spent before it stopped** — what a finding reports (the review's
    /// finding 3: a stop used to report 0 whatever it had read). A refused rung did not run; an
    /// unreadable one was reserved and read; a spent budget is the budget.
    pub fn rungs_spent(&self) -> u32 {
        match self {
            Self::EmptySpace | Self::SpaceTooWide(_) => 0,
            Self::StepTreesAgree { rungs } => *rungs,
            Self::Unreadable { rung, .. } => rung.saturating_add(1),
            Self::OverBudget { budget } => *budget,
            Self::RungRefused { rung, .. } => *rung,
        }
    }

    /// **Is the stop a condition of THIS HOST rather than a fact about the claim?** A rung the
    /// ledger refused (memory pressure from the seat's own replays) or this seat's own execution's
    /// prefix it could not read says nothing of the producer — the caller runs the claim again
    /// (the review's finding 3: it used to settle the case for good). Every other stop is the
    /// claim's: its space, its step trees, its served prefix.
    pub fn is_this_hosts(&self) -> bool {
        matches!(self, Self::RungRefused { .. } | Self::Unreadable { side: PalwReplaySideV1::Local, .. })
    }
}

/// **The bisection itself** — the first leaf of `[0, step_leaf_count)` at which the served and the
/// local executions' prefix commitments part.
///
/// `prefix_state(side, i)` is the side's commitment to its leaves `[0, i)` (the family's
/// `bisect_prefix_state`, a PREFIX commitment: two executions agreeing through `i` agree there, two
/// differing before `i` do not). Invariant: the prefix at `lo` agrees (the empty prefix of one job
/// context), the prefix at `hi` does not; `palw_bisect::bisect_midpoint_v1` halves `[lo, hi)` until it
/// is one leaf wide, and that leaf, `lo`, is the first that differs. `reserve_rung(r)` runs before
/// rung `r` reads a byte and its guard is held until the rung's two reads are done — the caller's
/// memory-ledger reservation, so no rung computes what the ledger did not cover. At most `budget`
/// rungs run; a claim's is [`palw_replay_bisect_rungs_v1`], which the search never exceeds.
pub fn palw_replay_bisect_v1<G>(
    step_leaf_count: u64,
    budget: u32,
    mut prefix_state: impl FnMut(PalwReplaySideV1, u64) -> Option<Hash64>,
    mut reserve_rung: impl FnMut(u32) -> Result<G, String>,
) -> Result<PalwReplayBisectV1, PalwReplayBisectStopV1> {
    if step_leaf_count == 0 {
        return Err(PalwReplayBisectStopV1::EmptySpace);
    }
    if step_leaf_count > crate::palw_bisect::PALW_BISECT_MAX_SPACE {
        return Err(PalwReplayBisectStopV1::SpaceTooWide(step_leaf_count));
    }
    let mut rungs = 0u32;
    let mut agrees = |index: u64| -> Result<bool, PalwReplayBisectStopV1> {
        if rungs >= budget {
            return Err(PalwReplayBisectStopV1::OverBudget { budget });
        }
        let rung = rungs;
        let _held_for_the_rung = reserve_rung(rung).map_err(|why| PalwReplayBisectStopV1::RungRefused { rung, why })?;
        rungs += 1;
        let served = prefix_state(PalwReplaySideV1::Served, index).ok_or(PalwReplayBisectStopV1::Unreadable {
            side: PalwReplaySideV1::Served,
            index,
            rung,
        })?;
        let local = prefix_state(PalwReplaySideV1::Local, index).ok_or(PalwReplayBisectStopV1::Unreadable {
            side: PalwReplaySideV1::Local,
            index,
            rung,
        })?;
        Ok(served == local)
    };
    if agrees(step_leaf_count)? {
        return Err(PalwReplayBisectStopV1::StepTreesAgree { rungs: 1 });
    }
    let (mut lo, mut hi) = (0u64, step_leaf_count);
    while hi - lo > 1 {
        let mid = crate::palw_bisect::bisect_midpoint_v1(lo, hi);
        if agrees(mid)? {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(PalwReplayBisectV1 { leaf: lo, rungs })
}

/// **The claim a replay is held against** — the chain's facts, never the material's.
#[derive(Clone, Copy, Debug)]
pub struct PalwReplayClaimV1<'a> {
    /// The claim as kind 4's adjudicator resolves it (`palw_offence_target_v1`): its execution root,
    /// class and artifact root, executor. A node builds it from its seat duty's copies of the same
    /// record fields.
    pub target: &'a PalwOffenceTargetV1,
    /// What the served capture must reproduce — the roots every seat arm checks material against
    /// (the claim's roots, the block's anchor and draw, the recorded job pin).
    pub roots: PalwClaimRootsV1,
    /// The fold's ladder for the class (`class_step_ladder_v1(class, PALW_FALSE_VALID_NETWORK_LADDER_V1)`),
    /// or a narrower one: a ladder only bounds the walk, so a narrower one refuses more, never less.
    pub ladder: u64,
    /// The class's prompt-id carriage (`palw_prompt_ids_form_of_class_v1`): a step refutation's
    /// prompt rides as ONE tile on a Merkle network (the capture arm's `palw_refutation_prompt_carriage_v1`).
    pub form: PalwPromptIdsFormV1,
    /// The job's prompt ids on the free-prompt lane (the job is the caller's, so its ids are an
    /// input to the prover); `None` on the attempt lane, whose prompt the prover re-derives.
    pub prompt_token_ids: Option<&'a [u32]>,
}

/// Where a proof stands in the committed execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwReplaySiteV1 {
    /// A step leaf whose committed output is not the step of its committed inputs (`StepArithmetic`, 5).
    StepLeaf(u64),
    /// A logits row whose committed lanes are not the head's step output (`LogitsNotStepOutput`, 12).
    LogitsHead { row: u32, head_tile: u32 },
}

/// **Why the builder has nothing to file** — each an honest outcome or a refusal, never a charge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwReplayNothingV1 {
    /// The served capture neither reproduces the claim's roots nor carries its binding: there is no
    /// committed execution in hand.
    ServedNotTheClaims,
    /// The served bytes carry the claim's binding (it authenticates against the claim's execution
    /// root) in a retention this family does not lay out — `capture_shape` refuses it: base0's floor
    /// reads the dense tuple only, so a folded (v2) retention served for a floor claim. Not the
    /// claim's capture as its family holds it (the floor retains dense on both lanes; the model
    /// tiers fold and their `capture_shape` / `bisect_prefix_state` read the fold), so nothing is
    /// located or filed from it — and a node sorts such bytes out before it replays anything
    /// ([`PalwReplayServedStandingV1::NotBisectable`]).
    ServedNotBisectable,
    /// The local capture is not of the served job's shape (another job context, another leaf count,
    /// bytes this family does not read): this seat's replay answers another question — it abstains.
    LocalNotTheJob(String),
    /// The local capture reproduces the claim's roots: there is no mismatch to prove.
    LocalReproduces,
    /// The bisection named no leaf.
    Bisect(PalwReplayBisectStopV1),
    /// A proof was built and the fold's own predicate does not convict: filed never (liveness — this
    /// is what an honest producer's claim yields against a seat whose replay is wrong).
    NotConvicted { site: PalwReplaySiteV1, why: PalwOffenceVerifyError },
    /// The step trees and every logits lane agree: the mismatch is in the output root — `OutputMismatch`
    /// (10), a claim-proving contradiction this builder does not make (P2-8 files it from the pin).
    LogitsAgree,
    /// The class has no provable logits head (`palw_logits_head_v1`), so F1c has no law to stand on.
    NoLogitsHead,
    /// The logits scan spent [`PALW_REPLAY_LOGITS_SCAN_MAX_V1`] reads without finding the bent lane.
    LogitsScanOverBudget,
    /// The served capture discloses no binding (the family has no DA responder).
    NoBinding(String),
}

impl PalwReplayNothingV1 {
    /// **Is this "nothing" this host's condition, not the claim's?** Only a bisection stop that is
    /// ([`PalwReplayBisectStopV1::is_this_hosts`]): the caller treats it as a failed run, which the
    /// claim's second run may redo, never as a finding that settles the claim.
    pub fn is_this_hosts(&self) -> bool {
        matches!(self, Self::Bisect(stop) if stop.is_this_hosts())
    }
}

/// **What the builder concluded.** `rungs` is the bisection's spend (0 where none ran).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwReplayFindingV1 {
    /// File it: kind 4 with this contradiction, which the fold's own predicate convicts.
    Refutes {
        contradiction: PalwPanelContradictionV1,
        prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
        site: PalwReplaySiteV1,
        rungs: u32,
    },
    /// **P2-8d**: the divergent leaf is located on the claim's own capture, and the served material
    /// cannot open its proof. Demand it: a data-availability session naming `StepLeaf { leaf }` under
    /// `binding` (DA-3, J-6) — the producer discloses the leaf's evidence or defaults (S1).
    DemandLeaf {
        leaf: u64,
        binding: Box<PalwStepBindingV2>,
        why: String,
        rungs: u32,
    },
    /// **P2-8e's hook**: the divergent leaf is a fused-attention leaf, whose terminal is a held
    /// dissection (DA-3 refuses it as a unit, `DaUnitNeedsDissection`). Not implemented: it waits for
    /// the audit's A-held line (§3.9 C1–C5, `CourtAttnRootClaimedHeld`, tag 57).
    NeedsDissection {
        leaf: u64,
        binding: Box<PalwStepBindingV2>,
        rungs: u32,
    },
    Nothing {
        why: PalwReplayNothingV1,
        rungs: u32,
    },
}

/// The claim's binding as its served capture commits it: every family with a DA responder opens an
/// out-of-range event as `OutOfRange { binding }` from the binding alone (P2-7's read of a capture's
/// binding, `palw_da_material_v1`).
fn served_binding_v1(backend: &dyn PalwExecutionBackendV1, served: &[u8]) -> Result<PalwStepBindingV2, String> {
    backend.disclose_trace_event(served, u32::MAX, u8::MAX).map(|disclosure| disclosure.binding().clone())
}

/// **Whether a served capture's BINDING is the claim's** — its committed execution root is the
/// claim's and its fields rebuild that root (`palw_step_leg::verify_binding_v1`, the fold's own
/// authentication of a held binding). Nothing about the body: that is `verify_material`'s, or —
/// for a capture no seat rule verifies — the fold's predicate on whatever is opened from it.
fn binding_is_the_claims_v1(binding: &PalwStepBindingV2, execution_root: &Hash64) -> bool {
    binding.committed_execution_root == *execution_root && crate::palw_step_leg::verify_binding_v1(binding).is_ok()
}

/// **Where a served capture stands before anything is paid for it** (the review's findings 1 and 5).
/// A node's pool takes any gossiped bytes, so its filer sorts them first — by the SAME reading the
/// builder's own gate makes of a capture that fails the seat rule ([`palw_replay_contradiction_v1`]
/// step 1), and the family's own shape read (its step 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwReplayServedStandingV1 {
    /// No binding reads off it, or its binding is not the claim's (another execution's root, or
    /// fields that do not rebuild the claim's): bytes anyone could have gossiped. Skipped, never a
    /// reason to stop looking — the claim's own capture may arrive later.
    NotTheClaims,
    /// The claim's own binding, in a retention this family does not lay out
    /// ([`PalwReplayNothingV1::ServedNotBisectable`]). Skipped like `NotTheClaims`: it costs no
    /// replay and stops nothing.
    NotBisectable,
    /// The claim's binding, in a retention the family bisects: worth a replay.
    Bisectable,
}

/// **[`PalwReplayServedStandingV1`] of `served` against the claim's `execution_root`** — a decode
/// and a hash of the binding; no replay, no verification of the body.
pub fn palw_replay_served_standing_v1(
    backend: &dyn PalwExecutionBackendV1,
    served: &[u8],
    execution_root: &Hash64,
) -> PalwReplayServedStandingV1 {
    match served_binding_v1(backend, served) {
        Ok(binding) if binding_is_the_claims_v1(&binding, execution_root) => {
            if backend.capture_shape(served).is_some() {
                PalwReplayServedStandingV1::Bisectable
            } else {
                PalwReplayServedStandingV1::NotBisectable
            }
        }
        _ => PalwReplayServedStandingV1::NotTheClaims,
    }
}

/// A leaf's refutation off `capture`, on the lane the claim runs on.
fn refutation_at_v1(
    backend: &dyn PalwExecutionBackendV1,
    capture: &[u8],
    leaf: u64,
    prompt_token_ids: Option<&[u32]>,
) -> Result<PalwExecutionStepRefutationV1, String> {
    match prompt_token_ids {
        Some(ids) => backend.refutation_for_free_prompt_index(capture, leaf, ids),
        None => backend.refutation_for_index(capture, leaf),
    }
}

/// **P2-8b / P2-8d: the replay-mismatch contradiction** — see the module's header for the steps and
/// the liveness rule. `served` is a capture as its family serves it (never a pool envelope: the
/// caller unwraps an `FPC1` payload); `local` is this seat's own execution of the SAME job, dense.
/// `budget` caps the bisection's rungs (a claim's is [`palw_replay_bisect_rungs_v1`]); `reserve_rung`
/// is the caller's per-rung memory reservation, held for the rung's two reads.
pub fn palw_replay_contradiction_v1<G>(
    backend: &dyn PalwExecutionBackendV1,
    served: &[u8],
    local: &[u8],
    claim: PalwReplayClaimV1<'_>,
    budget: u32,
    reserve_rung: impl FnMut(u32) -> Result<G, String>,
) -> PalwReplayFindingV1 {
    let nothing = |why: PalwReplayNothingV1, rungs: u32| PalwReplayFindingV1::Nothing { why, rungs };
    // 1. The served capture is the claim's committed execution — the seat arms' one rule — or, failing
    // that rule, at least carries the claim's own binding (see `verified` below).
    let verified = backend.verify_material(served, claim.roots) == PalwMaterialVerdictV1::Matches;
    let binding = match served_binding_v1(backend, served) {
        Ok(binding) => binding,
        Err(why) if verified => return nothing(PalwReplayNothingV1::NoBinding(why), 0),
        Err(_) => return nothing(PalwReplayNothingV1::ServedNotTheClaims, 0),
    };
    if !verified && !binding_is_the_claims_v1(&binding, &claim.target.execution_root) {
        return nothing(PalwReplayNothingV1::ServedNotTheClaims, 0);
    }
    // 2. The local capture is the same job, and disagrees with the claim. A served capture the
    // family cannot lay out is the claim's retention form, named apart from this seat's own replay.
    let Some(served_shape) = backend.capture_shape(served) else {
        return nothing(PalwReplayNothingV1::ServedNotBisectable, 0);
    };
    let Some(local_shape) = backend.capture_shape(local) else {
        return nothing(PalwReplayNothingV1::LocalNotTheJob("a capture this family does not read".into()), 0);
    };
    if served_shape.job_context != local_shape.job_context {
        return nothing(PalwReplayNothingV1::LocalNotTheJob("another job context".into()), 0);
    }
    if served_shape.step_leaf_count != local_shape.step_leaf_count {
        return nothing(
            PalwReplayNothingV1::LocalNotTheJob(format!(
                "{} leaves against the claim's {}",
                local_shape.step_leaf_count, served_shape.step_leaf_count
            )),
            0,
        );
    }
    if backend.verify_material(local, claim.roots) == PalwMaterialVerdictV1::Matches {
        return nothing(PalwReplayNothingV1::LocalReproduces, 0);
    }
    // **A capture no seat verifies, under the claim's own binding, is F1c's alone.** A family's seat
    // rule refuses a capture whose committed logits rows are not its head's step outputs (base0's
    // SEAT-0 head rule) — which is exactly the garbage-logits lie, so such a producer's capture never
    // verifies anywhere, and its rows are nevertheless the claim's commitment. `LogitsNotStepOutput`
    // authenticates everything it carries against the claim's roots (the event against the trace
    // root under the claim's binding, the head leaf against the step root), so the fold's predicate is
    // the whole gate: bytes that are not the claim's convict nothing. Nothing else is attempted on
    // them — no bisection, no demand: a leaf located on bytes no seat verified is not the claim's.
    if !verified {
        return logits_not_step_output_v1(backend, served, local, &binding, claim, 0);
    }
    // 3. The first leaf the step trees part at.
    let n = served_shape.step_leaf_count;
    let located = palw_replay_bisect_v1(
        n,
        budget.min(palw_replay_bisect_rungs_v1(n)),
        |side, index| match side {
            PalwReplaySideV1::Served => backend.bisect_prefix_state(served, index),
            PalwReplaySideV1::Local => backend.bisect_prefix_state(local, index),
        },
        reserve_rung,
    );
    let (leaf, rungs) = match located {
        Ok(PalwReplayBisectV1 { leaf, rungs }) => (leaf, rungs),
        // 5. One step tree: F1c's garbage logits, or the output root's.
        Err(PalwReplayBisectStopV1::StepTreesAgree { rungs }) => {
            return logits_not_step_output_v1(backend, served, local, &binding, claim, rungs);
        }
        Err(stop) => {
            let rungs = stop.rungs_spent();
            return nothing(PalwReplayNothingV1::Bisect(stop), rungs);
        }
    };
    // 4. P2-8e's one hook: a fused-attention leaf goes to a held dissection, never to a unit or a
    // one-step proof (the fold's own predicate, DA-3's `DaUnitNeedsDissection`).
    if crate::palw_da_rcore_v1::palw_da_step_leaf_is_fused_v1(&binding, leaf) {
        return PalwReplayFindingV1::NeedsDissection { leaf, binding: Box::new(binding), rungs };
    }
    let demand = |why: String| PalwReplayFindingV1::DemandLeaf { leaf, binding: Box::new(binding.clone()), why, rungs };
    let refutation = match refutation_at_v1(backend, served, leaf, claim.prompt_token_ids) {
        Ok(refutation) => refutation,
        Err(why) => return demand(format!("the served capture does not open leaf {leaf}: {why}")),
    };
    let operand_openings = match backend.operand_openings_for(&refutation) {
        Ok(openings) => openings,
        Err(why) => return demand(format!("leaf {leaf}'s artifact rows do not open: {why}")),
    };
    let (refutation, prompt_ids_opening) = match crate::palw_step_refute::palw_refutation_prompt_carriage_v1(claim.form, refutation) {
        Ok(carried) => carried,
        Err(why) => return demand(format!("leaf {leaf}'s prompt tile does not open: {why}")),
    };
    let contradiction = PalwPanelContradictionV1::StepArithmetic { refutation, operand_openings };
    let site = PalwReplaySiteV1::StepLeaf(leaf);
    match palw_false_valid_convicts_execution_v2(
        &contradiction,
        prompt_ids_opening.as_ref(),
        claim.target.execution_root,
        claim.target.artifact_root,
        claim.ladder,
    ) {
        Ok(()) => PalwReplayFindingV1::Refutes { contradiction, prompt_ids_opening, site, rungs },
        Err(why) => nothing(PalwReplayNothingV1::NotConvicted { site, why }, rungs),
    }
}

/// **F1c on a replay** (§3.10 J-5, addendum §4-bis.6): the step trees agree, so the step outputs are
/// the honest ones — or the capture is tied to the claim only by its binding, and nothing is assumed of
/// its steps; the first committed logits lane that is not the local run's is a candidate lane the
/// head's step did not output. The event is the SERVED capture's disclosure (the committed rows), the
/// head leaf is opened from the served capture, and the fold's own predicate decides — it
/// authenticates both against the claim's roots and holds (`LogitsHold`) a lane that IS the head's
/// output.
fn logits_not_step_output_v1(
    backend: &dyn PalwExecutionBackendV1,
    served: &[u8],
    local: &[u8],
    binding: &PalwStepBindingV2,
    claim: PalwReplayClaimV1<'_>,
    rungs: u32,
) -> PalwReplayFindingV1 {
    use crate::palw_step_refute::PALW_LOGITS_TILE_LANES;
    let nothing = |why: PalwReplayNothingV1| PalwReplayFindingV1::Nothing { why, rungs };
    let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
    let Some(head) = crate::palw_step::palw_logits_head_v1(profile) else { return nothing(PalwReplayNothingV1::NoLogitsHead) };
    let vocab = profile.vocab_size as usize;
    let mut reads = 0u32;
    // The first (row, lane) whose committed value is not the local run's, and the served event
    // that carries the row (flat: every row) or the lane's tile (tiled).
    let mut bent: Option<(u32, usize)> = None;
    'rows: for row in 0..ctx.exact_decode_tokens {
        let tiles = vocab.div_ceil(PALW_LOGITS_TILE_LANES).max(1);
        for tile in 0..tiles {
            reads += 1;
            if reads > PALW_REPLAY_LOGITS_SCAN_MAX_V1 {
                return nothing(PalwReplayNothingV1::LogitsScanOverBudget);
            }
            let Ok(tile_u8) = u8::try_from(tile) else { return nothing(PalwReplayNothingV1::NoLogitsHead) };
            let (Ok(s), Ok(l)) =
                (backend.disclose_trace_event(served, row, tile_u8), backend.disclose_trace_event(local, row, tile_u8))
            else {
                continue 'rows;
            };
            let (base, s_lanes, l_lanes): (usize, &[i32], &[i32]) = match (&s, &l) {
                (PalwTraceEventDisclosureV1::Flat { pin: sp, .. }, PalwTraceEventDisclosureV1::Flat { pin: lp, .. }) => {
                    let (Some(sr), Some(lr)) = (sp.logits_rows.get(row as usize), lp.logits_rows.get(row as usize)) else {
                        continue 'rows;
                    };
                    (0, sr.as_slice(), lr.as_slice())
                }
                (
                    PalwTraceEventDisclosureV1::Tiled { tile_lanes: sl, .. },
                    PalwTraceEventDisclosureV1::Tiled { tile_lanes: ll, .. },
                ) => (tile * PALW_LOGITS_TILE_LANES, sl.as_slice(), ll.as_slice()),
                _ => continue 'rows,
            };
            if let Some(at) = s_lanes.iter().zip(l_lanes.iter()).position(|(a, b)| a != b) {
                bent = Some((row, base + at));
                break 'rows;
            }
            // A flat disclosure carries the whole row: its tiles are one read.
            if matches!(s, PalwTraceEventDisclosureV1::Flat { .. }) {
                continue 'rows;
            }
        }
    }
    let Some((row, lane)) = bent else { return nothing(PalwReplayNothingV1::LogitsAgree) };
    let head_tile = (lane / head.tile_len as usize) as u32;
    let flat = profile.logits_scheme_id == crate::palw_step_refute::flat_logits_scheme_id_v1();
    let logits_tile = if flat { 0 } else { (head_tile as usize * head.tile_len as usize) / PALW_LOGITS_TILE_LANES };
    let site = PalwReplaySiteV1::LogitsHead { row, head_tile };
    let not_convicted = |why: PalwOffenceVerifyError| nothing(PalwReplayNothingV1::NotConvicted { site, why });
    let needs = PalwOffenceVerifyError::PanelFalseValidNeedsContradiction;
    let Ok(logits_tile) = u8::try_from(logits_tile) else { return not_convicted(PalwOffenceVerifyError::HeadUnproven) };
    let Ok(event) = backend.disclose_trace_event(served, row, logits_tile) else { return not_convicted(needs) };
    let Some(coord) = crate::palw_step::palw_logits_head_coordinate_v1(&head, ctx, row, head_tile) else {
        return not_convicted(needs);
    };
    let Some(index) = crate::palw_step::canonical_step_leaf_index(profile, ctx, &coord) else { return not_convicted(needs) };
    let head_opening = match refutation_at_v1(backend, served, index, claim.prompt_token_ids) {
        Ok(refutation) => refutation.output_opening,
        Err(_) => return not_convicted(needs),
    };
    match palw_logits_not_step_output_fault_v1(claim.target, &event, row, head_tile, &head_opening, claim.ladder) {
        Ok(()) => PalwReplayFindingV1::Refutes {
            contradiction: PalwPanelContradictionV1::LogitsNotStepOutput { event, row, head_tile, head_opening },
            prompt_ids_opening: None,
            site,
            rungs,
        },
        Err(why) => not_convicted(why),
    }
}

/// **Kind 4, as the builder files it** — the object, and what a reporter filer needs beside it: the
/// ledger key (one kind-4 offence per claim, `palw_executor_refuted_offence_id_v1`) and the evidence
/// id (`palw_offence_evidence_digest_v1` of the evidence bytes — what R-3's commitment binds, N12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwReplayRefutedObjectV1 {
    pub offence_id: Hash64,
    pub evidence_id: Hash64,
    pub object: PalwConsensusObjectV2,
}

/// Why no kind-4 object was built — each a refusal the gate or the fold would make.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwReplayObjectErrorV1 {
    #[error("not a refutation of an executor: {0}")]
    NotARefutation(PalwOffenceVerifyError),
    #[error("a prompt-id opening rides only beside a StepArithmetic")]
    StrayOpening,
    #[error("the evidence is {bytes} bytes, above the {cap} one carrier holds")]
    TooLarge { bytes: u64, cap: u64 },
    #[error("the object cannot ride a carrier: {0}")]
    CannotRide(&'static str),
}

/// **The `ExecutorRefuted` a replay proves** (J-4, kind 4): `PalwExecutorRefutedEvidenceV1` version 1
/// over `claim_id`, the contradiction and its prompt tile, with the reporter slot EMPTY (R-3 uses
/// objects 53/54, never that slot) — exactly what `palw_check_executor_refuted_v1` decodes. Refused
/// here, by the fold's own admission rule and the one-carrier cap, before a carrier is paid for it.
pub fn palw_replay_executor_refuted_object_v1(
    claim_id: Hash64,
    executor: PalwBondKeyV2,
    contradiction: PalwPanelContradictionV1,
    prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
) -> Result<PalwReplayRefutedObjectV1, PalwReplayObjectErrorV1> {
    palw_executor_refuted_admission_v1(&contradiction).map_err(PalwReplayObjectErrorV1::NotARefutation)?;
    if prompt_ids_opening.is_some() && !matches!(contradiction, PalwPanelContradictionV1::StepArithmetic { .. }) {
        return Err(PalwReplayObjectErrorV1::StrayOpening);
    }
    let evidence = borsh::to_vec(&PalwExecutorRefutedEvidenceV1 {
        version: PALW_EXECUTOR_REFUTED_VERSION_V1,
        claim_id,
        contradiction,
        prompt_ids_opening,
        reporter_reveal: Vec::new(),
    })
    .expect("the evidence serializes");
    if evidence.len() as u64 > PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES {
        return Err(PalwReplayObjectErrorV1::TooLarge { bytes: evidence.len() as u64, cap: PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES });
    }
    let evidence_id = palw_offence_evidence_digest_v1(&evidence);
    let object =
        PalwConsensusObjectV2::ObjectiveOffence { kind: PalwOffenceKindV1::ExecutorRefuted, accused: executor, evidence_id, evidence };
    crate::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(PalwReplayObjectErrorV1::CannotRide)?;
    Ok(PalwReplayRefutedObjectV1 { offence_id: palw_executor_refuted_offence_id_v1(&executor.0, &claim_id), evidence_id, object })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic pair of executions over `n` leaves that part at `diverge` (or never): the prefix
    /// commitment is the count of agreeing leaves in `[0, i)`, which is a prefix commitment exactly
    /// when the leaves before `diverge` agree and the rest do not.
    fn prefix(n: u64, diverge: Option<u64>) -> impl FnMut(PalwReplaySideV1, u64) -> Option<Hash64> {
        move |side, index| {
            let index = index.min(n);
            let word = match (side, diverge) {
                (PalwReplaySideV1::Local, _) | (_, None) => index,
                (PalwReplaySideV1::Served, Some(d)) if index <= d => index,
                (PalwReplaySideV1::Served, Some(_)) => index | (1 << 63),
            };
            Some(Hash64::from_u64_word(word))
        }
    }

    /// **The bisection names the first divergent leaf in at most `1 + ⌈log₂ n⌉` rungs, each reserved
    /// before it runs** — over every position of a small space and at the edges of a 2^40 one.
    #[test]
    fn the_bisection_names_the_first_divergent_leaf_in_log_rungs() {
        for n in 1..=70u64 {
            for d in 0..n {
                let mut reserved = Vec::new();
                let got = palw_replay_bisect_v1(n, palw_replay_bisect_rungs_v1(n), prefix(n, Some(d)), |rung| {
                    reserved.push(rung);
                    Ok::<(), String>(())
                })
                .unwrap_or_else(|e| panic!("n {n} d {d}: {e}"));
                assert_eq!(got.leaf, d, "n {n}");
                assert!(got.rungs <= palw_replay_bisect_rungs_v1(n), "n {n} d {d}: {} rungs", got.rungs);
                assert_eq!(reserved, (0..got.rungs).collect::<Vec<_>>(), "every rung reserved, in order");
            }
        }
        let n = 1u64 << 40;
        for d in [0, 1, n / 2, n - 2, n - 1] {
            let got = palw_replay_bisect_v1(n, PALW_REPLAY_BISECT_MAX_RUNGS_V1, prefix(n, Some(d)), |_| Ok::<(), String>(())).unwrap();
            assert_eq!((got.leaf, got.rungs <= 41), (d, true));
        }
        assert_eq!(palw_replay_bisect_rungs_v1(1 << 40), PALW_REPLAY_BISECT_MAX_RUNGS_V1);
        assert_eq!(palw_replay_bisect_rungs_v1(1), 1);
        assert_eq!(palw_replay_bisect_rungs_v1(2), 2);
        assert_eq!(palw_replay_bisect_rungs_v1(1_000_000), 21);
    }

    /// Agreeing trees, a refused rung, a spent budget and an unreadable side name no leaf.
    #[test]
    fn the_bisection_names_no_leaf_it_did_not_prove() {
        assert_eq!(
            palw_replay_bisect_v1(64, 7, prefix(64, None), |_| Ok::<(), String>(())),
            Err(PalwReplayBisectStopV1::StepTreesAgree { rungs: 1 })
        );
        assert_eq!(
            palw_replay_bisect_v1(64, 7, prefix(64, Some(3)), |rung| if rung < 2 { Ok(()) } else { Err("full".to_string()) }),
            Err(PalwReplayBisectStopV1::RungRefused { rung: 2, why: "full".into() })
        );
        assert_eq!(
            palw_replay_bisect_v1(64, 3, prefix(64, Some(3)), |_| Ok::<(), String>(())),
            Err(PalwReplayBisectStopV1::OverBudget { budget: 3 })
        );
        assert_eq!(
            palw_replay_bisect_v1(
                64,
                7,
                |side, i| (side == PalwReplaySideV1::Served).then_some(Hash64::from_u64_word(i)),
                |_| { Ok::<(), String>(()) }
            ),
            Err(PalwReplayBisectStopV1::Unreadable { side: PalwReplaySideV1::Local, index: 64, rung: 0 })
        );
        assert_eq!(palw_replay_bisect_v1(0, 7, prefix(0, None), |_| Ok::<(), String>(())), Err(PalwReplayBisectStopV1::EmptySpace));
    }

    /// **A stop says what it spent, and whether it is this host's** (the review's finding 3): a rung
    /// the ledger refused and an unreadable LOCAL prefix are this host's conditions — the caller runs
    /// the claim again — and report the rungs actually read; an unreadable SERVED prefix, agreeing
    /// trees and a spent budget are the claim's.
    #[test]
    fn a_stop_reports_its_rungs_and_whether_it_is_this_hosts() {
        let refused =
            palw_replay_bisect_v1(64, 7, prefix(64, Some(3)), |rung| if rung < 2 { Ok(()) } else { Err("full".to_string()) })
                .unwrap_err();
        assert_eq!((refused.rungs_spent(), refused.is_this_hosts()), (2, true));
        assert!(PalwReplayNothingV1::Bisect(refused).is_this_hosts());
        let local = palw_replay_bisect_v1(
            64,
            7,
            |side, i| (side == PalwReplaySideV1::Served || i < 64).then_some(Hash64::from_u64_word(i)),
            |_| Ok::<(), String>(()),
        )
        .unwrap_err();
        assert_eq!((local.rungs_spent(), local.is_this_hosts()), (1, true), "{local}");
        let served = palw_replay_bisect_v1(
            64,
            7,
            |side, i| (side == PalwReplaySideV1::Local).then_some(Hash64::from_u64_word(i)),
            |_| Ok::<(), String>(()),
        )
        .unwrap_err();
        assert_eq!((served.rungs_spent(), served.is_this_hosts()), (1, false), "the claim's prefix: {served}");
        let over = palw_replay_bisect_v1(64, 3, prefix(64, Some(3)), |_| Ok::<(), String>(())).unwrap_err();
        assert_eq!((over.rungs_spent(), over.is_this_hosts()), (3, false));
        let agree = palw_replay_bisect_v1(64, 7, prefix(64, None), |_| Ok::<(), String>(())).unwrap_err();
        assert_eq!((agree.rungs_spent(), agree.is_this_hosts()), (1, false));
        assert!(!PalwReplayNothingV1::ServedNotBisectable.is_this_hosts());
        assert!(!PalwReplayNothingV1::LocalReproduces.is_this_hosts());
    }
}
