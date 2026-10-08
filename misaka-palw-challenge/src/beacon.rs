//! **`PalwWorkBeaconV1`** (RFC-0007 §§VI.3–VI.4): `k` distinct future qualifying useful works, in canonical order, mixed into
//! one accumulator — derived by every node from its own canonical PALW history, never issued or locked by a committee.
//!
//! * **Eligibility** ([`eligibility_v1`]): REAL useful work only — heartbeat, BASE-0 fallback, EXEC_TX, EXEC work slices,
//!   receipt-only material, provisional attempts and bare Panel receipts are refused by kind. The source profile must be in the
//!   set frozen from the subject's commitment state as Active **and** G14-complete (the consumer derives both from
//!   authenticated state and the code-derived gate; never a registrant flag), must not be the candidate under test or depend on
//!   its semantics, and the work must be Final, DA-satisfied and valid independently of the challenge consuming it.
//! * **Freshness**: the work's commitment is accepted at or after `S = commitment position + anchor delay`; reaching Final
//!   after `S` is not enough. Its settlement falls in `[S, S + window)`.
//! * **Order**: `(settlement_position, occurrence_index, canonical_work_id)`; the first occurrence of a work identity counts,
//!   re-inclusions/reattachments never enlarge the set. Arrival order, RPC order and producer-selected lists are irrelevant:
//!   [`collect_work_beacon_v1`] sorts what it is given.
//! * **Lock**: `k` sources and the `k`-th settled at depth `D` on this branch — a branch-relative state, not finality and not a
//!   reorg veto. A reorg is a recomputation over the new branch's events; dependents of a lost lock roll back with it.
//! * **Unavailable**: the window closed with fewer than `k`. That is `BEACON_UNAVAILABLE` — never fraud, never a pass, and
//!   never a fallback to heartbeat/BASE-0/EXEC/block hashes, signatures or local randomness.
//!
//! Mixing more works is not, by itself, a proof of unbiased randomness: an adaptive last contributor, withholding, reordering
//! and reorgs can bias a simple accumulator. Bounding that bias is an external review gate (RFC-0007 §VI.8 item 2).

use std::collections::BTreeSet;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::hash::{DOMAIN_CHALLENGE_ANCHOR, DOMAIN_WORK_BEACON, DOMAIN_WORK_BEACON_ITEM, DOMAIN_WORK_BEACON_MIX, Digest, object_id};
use crate::policy::{PolicyRefusalV1, PostCommitChallengePolicyV1};
use crate::subject::SubjectKindV1;

/// What a Final event on the chain is. Only `RealUsefulWork` can be a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum WorkSourceKindV1 {
    RealUsefulWork = 0,
    Heartbeat = 1,
    Base0Fallback = 2,
    ExecTx = 3,
    ExecWorkSlice = 4,
    ReceiptOnly = 5,
    ProvisionalAttempt = 6,
    PanelReceipt = 7,
}

/// One settlement event of the canonical history, with the facts the consumer derived for it from authenticated state.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WorkFinalEventV1 {
    pub kind: WorkSourceKindV1,
    pub source_profile_id: Digest,
    /// Stable under header/carrier reattachment (never a block hash, signature or rewrappable claim id).
    pub canonical_work_id: Digest,
    pub execution_commitment: Digest,
    /// The EARLIEST chain position at which this canonical work identity's commitment was accepted on this branch, through any
    /// carrier: a work committed before `S` and reattached later is still not fresh.
    pub accepted_position: u64,
    /// The chain position of its claim Final / PALW-native settlement.
    pub settlement_position: u64,
    pub occurrence_index: u32,
    pub claim_final: bool,
    pub da_satisfied: bool,
    /// Its validity does not depend on any challenge consuming this beacon.
    pub validity_independent: bool,
    /// Profiles whose semantics this work depends on (pipeline stages, adapters): a candidate here makes it ineligible.
    pub depends_on_profiles: Vec<Digest>,
    /// How the work reached Final: through a Panel licence (and which Panel draw), or with no Panel in its path.
    pub final_path: FinalPathV1,
}

/// **How a source work reached Final** — the fact that decides whether it may seed a Panel draw.
///
/// A Panel-licensed Final is a function of a Panel assignment (its seats can include, delay or void the work), so feeding it into
/// the entropy of a Panel assignment closes the loop `work validity → Panel assignment → Final → beacon → Panel assignment`.
/// [`SubjectKindV1::PanelAssignment`] therefore accepts only [`FinalPathV1::PanelIndependent`] sources; until a Panel-independent
/// Final exists (RFC-0014 public window lapse / RFC-0015), a Panel-assignment beacon stays `BEACON_UNAVAILABLE` and RFC-0010 stays
/// dormant. Other subjects may use Panel-licensed sources; the veto/delay bias a captured Panel has over them is part of the
/// reviewed bias budget (RFC-0007 §VI.8 item 2), not a property this code proves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum FinalPathV1 {
    /// Licensed by a Panel quorum or by parts; the licensing Panel was drawn from `panel_seed_id` at `panel_epoch`.
    PanelLicensed { panel_seed_id: Digest, panel_epoch: u64 },
    /// Final with no Panel licence anywhere in its path.
    PanelIndependent,
}

/// The subject side of a beacon: fixed when the subject's commitment is accepted.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconContextV1 {
    pub chain_genesis: Digest,
    pub ruleset_id: Digest,
    pub policy: PostCommitChallengePolicyV1,
    pub subject_kind: SubjectKindV1,
    pub commitment_root: Digest,
    /// The accepted chain position of the subject's commitment.
    pub commitment_position: u64,
    pub challenge_epoch: u64,
    /// Source profiles that were Active AND G14-complete in the subject's commitment state (frozen then).
    pub eligible_profiles: BTreeSet<Digest>,
    /// The candidate under test and every profile depending on its proposed semantics.
    pub excluded_profiles: BTreeSet<Digest>,
}

impl BeaconContextV1 {
    /// `S`: the first position a source's commitment may be accepted at.
    pub fn start(&self) -> u64 {
        self.commitment_position.saturating_add(self.policy.anchor_delay_slots)
    }

    /// The end (exclusive) of the settlement window.
    pub fn window_end(&self) -> u64 {
        self.start().saturating_add(self.policy.beacon_window_slots)
    }
}

/// Why an event is not a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IneligibleV1 {
    #[error("not REAL useful work ({0:?})")]
    NotUsefulWork(WorkSourceKindV1),
    #[error("the source profile was not Active and G14-complete at the subject's commitment")]
    ProfileNotEligible,
    #[error("the candidate under test, or work depending on its semantics (no self-beacon)")]
    SelfOrDependent,
    #[error("the work's commitment was accepted before the start S (not fresh)")]
    NotFresh,
    #[error("settled outside the beacon window")]
    OutsideWindow,
    #[error("not Final")]
    NotFinal,
    #[error("its public DA obligation is not satisfied")]
    DaUnsatisfied,
    #[error("its validity depends on the challenge consuming it")]
    NotIndependent,
    #[error("a Panel-licensed Final cannot seed a Panel assignment (circular: Panel → Final → beacon → Panel)")]
    PanelDependentFinal,
}

pub fn eligibility_v1(ctx: &BeaconContextV1, ev: &WorkFinalEventV1) -> Result<(), IneligibleV1> {
    use IneligibleV1 as I;
    if ev.kind != WorkSourceKindV1::RealUsefulWork {
        return Err(I::NotUsefulWork(ev.kind));
    }
    if ctx.excluded_profiles.contains(&ev.source_profile_id)
        || ev.depends_on_profiles.iter().any(|p| ctx.excluded_profiles.contains(p))
    {
        return Err(I::SelfOrDependent);
    }
    if !ctx.eligible_profiles.contains(&ev.source_profile_id) {
        return Err(I::ProfileNotEligible);
    }
    if ev.accepted_position < ctx.start() {
        return Err(I::NotFresh);
    }
    if ev.settlement_position < ctx.start() || ev.settlement_position >= ctx.window_end() {
        return Err(I::OutsideWindow);
    }
    if !ev.claim_final {
        return Err(I::NotFinal);
    }
    if !ev.da_satisfied {
        return Err(I::DaUnsatisfied);
    }
    if !ev.validity_independent {
        return Err(I::NotIndependent);
    }
    if ctx.subject_kind == SubjectKindV1::PanelAssignment && ev.final_path != FinalPathV1::PanelIndependent {
        return Err(I::PanelDependentFinal);
    }
    Ok(())
}

/// One mixed source, as the public evidence records it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconSourceV1 {
    pub source_profile_id: Digest,
    pub canonical_work_id: Digest,
    pub execution_commitment: Digest,
    pub accepted_position: u64,
    pub settlement_position: u64,
    pub occurrence_index: u32,
}

/// A locked beacon: its sources in canonical order, every accumulator and the output.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct WorkBeaconV1 {
    pub sources: Vec<BeaconSourceV1>,
    /// `acc_0 … acc_k`.
    pub accumulators: Vec<Digest>,
    pub output: Digest,
    /// The normalized window/epoch and ordered work identities (not a block hash).
    pub challenge_anchor: Digest,
    /// The position from which it is locked on this branch (`k`-th settlement + D).
    pub lock_position: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkBeaconStateV1 {
    /// The window is open and fewer than `k` sources settled.
    Collecting {
        have: u32,
        need: u32,
    },
    /// `k` sources, the `k`-th not yet at depth `D`.
    Candidate {
        have: u32,
        lock_position: u64,
    },
    Locked(WorkBeaconV1),
    /// `BEACON_UNAVAILABLE`: the window closed with fewer than `k`. Not fraud, not a pass.
    Unavailable {
        have: u32,
        need: u32,
    },
}

fn item_v1(s: &BeaconSourceV1) -> Digest {
    object_id(DOMAIN_WORK_BEACON_ITEM, &(s.source_profile_id, s.canonical_work_id, s.execution_commitment))
}

/// `acc_0`.
pub fn initial_accumulator_v1(ctx: &BeaconContextV1) -> Digest {
    object_id(DOMAIN_WORK_BEACON, &(ctx.chain_genesis, ctx.ruleset_id, ctx.policy.id(), ctx.commitment_root, ctx.challenge_epoch))
}

/// `acc_i` from `acc_(i-1)`, the 1-based index `i` and source `i`.
pub fn mix_v1(prev: &Digest, i: u32, source: &BeaconSourceV1) -> Digest {
    object_id(DOMAIN_WORK_BEACON_MIX, &(*prev, i, item_v1(source)))
}

/// The challenge anchor: policy, epoch, window and the ordered work identities.
pub fn challenge_anchor_v1(ctx: &BeaconContextV1, sources: &[BeaconSourceV1]) -> Digest {
    let ids: Vec<Digest> = sources.iter().map(|s| s.canonical_work_id).collect();
    object_id(DOMAIN_CHALLENGE_ANCHOR, &(ctx.policy.id(), ctx.challenge_epoch, ctx.start(), ctx.policy.beacon_window_slots, ids))
}

/// The canonical, eligible, de-duplicated source list from `events` (any order).
pub fn canonical_sources_v1(ctx: &BeaconContextV1, events: &[WorkFinalEventV1]) -> Vec<BeaconSourceV1> {
    let mut sorted: Vec<&WorkFinalEventV1> = events.iter().collect();
    sorted.sort_by(|a, b| {
        (a.settlement_position, a.occurrence_index, a.canonical_work_id).cmp(&(
            b.settlement_position,
            b.occurrence_index,
            b.canonical_work_id,
        ))
    });
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for ev in sorted {
        // A work identity counts once, at its first canonical occurrence, eligible or not: an ineligible first occurrence is not
        // rescued by a later reattachment.
        if !seen.insert(ev.canonical_work_id) {
            continue;
        }
        if eligibility_v1(ctx, ev).is_ok() {
            out.push(BeaconSourceV1 {
                source_profile_id: ev.source_profile_id,
                canonical_work_id: ev.canonical_work_id,
                execution_commitment: ev.execution_commitment,
                accepted_position: ev.accepted_position,
                settlement_position: ev.settlement_position,
                occurrence_index: ev.occurrence_index,
            });
        }
    }
    out
}

/// **The beacon at `tip_position`** of the branch whose settlement events are `events`.
pub fn collect_work_beacon_v1(
    ctx: &BeaconContextV1,
    events: &[WorkFinalEventV1],
    tip_position: u64,
) -> Result<WorkBeaconStateV1, PolicyRefusalV1> {
    ctx.policy.validate()?;
    let need = ctx.policy.work_count_k;
    // Only what has settled by the tip exists on this branch.
    let settled: Vec<WorkFinalEventV1> = events.iter().filter(|e| e.settlement_position <= tip_position).cloned().collect();
    let mut sources = canonical_sources_v1(ctx, &settled);
    sources.truncate(need as usize);
    let have = sources.len() as u32;
    if have < need {
        return Ok(if tip_position >= ctx.window_end() {
            WorkBeaconStateV1::Unavailable { have, need }
        } else {
            WorkBeaconStateV1::Collecting { have, need }
        });
    }
    let lock_position = sources.last().expect("k ≥ 1").settlement_position.saturating_add(ctx.policy.settlement_depth_d);
    if tip_position < lock_position {
        return Ok(WorkBeaconStateV1::Candidate { have, lock_position });
    }
    let mut accumulators = vec![initial_accumulator_v1(ctx)];
    for (i, s) in sources.iter().enumerate() {
        let next = mix_v1(accumulators.last().expect("acc_0"), (i + 1) as u32, s);
        accumulators.push(next);
    }
    let output = *accumulators.last().expect("acc_k");
    let challenge_anchor = challenge_anchor_v1(ctx, &sources);
    Ok(WorkBeaconStateV1::Locked(WorkBeaconV1 { sources, accumulators, output, challenge_anchor, lock_position }))
}

/// Why a presented beacon is not the one this node derives.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BeaconEvidenceRefusalV1 {
    #[error("the policy: {0}")]
    Policy(PolicyRefusalV1),
    #[error("this branch has no locked beacon for the subject ({0:?})")]
    NotLocked(WorkBeaconStateV1),
    #[error("the presented beacon differs from the one derived from canonical history at source {at}")]
    Mismatch { at: usize },
    #[error("the presented accumulator chain, output or anchor is not the derivation of its sources")]
    Derivation,
}

/// **A fresh node checks a presented beacon** against its own canonical events: reordered, non-canonical, duplicated,
/// ineligible or substituted sources, a forged accumulator or anchor — all refused. The presented object is a claim to
/// recompute, never an input.
pub fn verify_work_beacon_v1(
    ctx: &BeaconContextV1,
    presented: &WorkBeaconV1,
    events: &[WorkFinalEventV1],
    tip_position: u64,
) -> Result<(), BeaconEvidenceRefusalV1> {
    let derived = match collect_work_beacon_v1(ctx, events, tip_position).map_err(BeaconEvidenceRefusalV1::Policy)? {
        WorkBeaconStateV1::Locked(b) => b,
        other => return Err(BeaconEvidenceRefusalV1::NotLocked(other)),
    };
    let n = derived.sources.len().max(presented.sources.len());
    if let Some(at) = (0..n).find(|i| derived.sources.get(*i) != presented.sources.get(*i)) {
        return Err(BeaconEvidenceRefusalV1::Mismatch { at });
    }
    if derived != *presented {
        return Err(BeaconEvidenceRefusalV1::Derivation);
    }
    Ok(())
}
