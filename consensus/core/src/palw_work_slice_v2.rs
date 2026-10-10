//! RFC-0008 v2 — the **work-slice ledgers and their pure rules**: `WorkSliceUse`, `RootWorkBudget`, `JobWorkUse`, the canonical
//! plan, the six admission checks' refusal names, and the root's one settlement.
//!
//! The wire types are [`crate::palw_exec_v2`]'s; the fold that writes these rows through journaled writers is
//! `palw_exec_v2_fold` (a child of `palw_state_v2`, so it reads the bond registry, the claims and the exposure ledger directly).
//! Everything here is a pure function of its arguments and uses checked integers throughout.
//!
//! # The three ledgers (spec §4)
//!
//! ```text
//! roots[root_claim_id]          -> PalwWorkRootV2     RootWorkBudget: job identity, plan, accepted/verified work, lifecycle
//! slices[(root_claim_id, i)]    -> PalwWorkSliceRowV2 WorkSliceUse: range, roots, carrier, stage, executor
//! jobs[job_work_id]             -> root_claim_id      JobWorkUse: the one-use history rule for a job's computation
//! ```
//!
//! They are separate from the round-permit ledger (`round_permits_used`): a permit's consumption creates no work credit and a
//! slice's acceptance spends no permit.
//!
//! # What is conserved
//!
//! ```text
//! root_prefix_work + sum(credited slice work) <= total_work                 (checked in the plan and at every admission)
//! credited ranges are pairwise disjoint                                     (a slice's range is the plan's range for its index)
//! one accepted use per (root, index) in a history                           (the row's presence)
//! sum(root and slice reward paid) <= the root claim's funded allocation     (settle_root_v2: a floor split, the root keeps the rest)
//! one settlement per root                                                   (phase Settled is terminal)
//! every EXEC chain-position term = 0                                        (nothing here touches fork choice, the DAA or the clock)
//! ```
//!
//! # Verification (amendment 1, spec §10.1)
//!
//! A slice becomes `Verified` only when the G14 kernel route finalizes the kernel claim the slice names
//! ([`crate::palw_exec_v2_verify`]): the route an ordinary outside bond can stop from public material — a conviction makes the slice
//! `ProvenFalse`, a default (withheld material, a timeout) `Defaulted`, and either voids the slice's suffix and the root. Nothing here
//! marks a slice verified from a carrier, a receipt or a harness signature; the test-only writer `mark_slice_verified_for_tests`
//! remains for the fold's arithmetic tests on chains with no kernel route.

use crate::palw_exec_v2::{PALW_EXEC_V2_MAX_SLICES_PER_ROOT, PalwWorkRangeV1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::{BlockHash, Hash64};

/// The most roots the lane holds open at once, chain-wide. Each open root is priced by the REAL claim it hangs from, but the
/// ledger is also bounded by count so a flood of cheap claims cannot grow it without limit.
pub const PALW_EXEC_V2_MAX_OPEN_ROOTS: usize = 256;

/// The most roots one bond may be the *root executor* of at once.
pub const PALW_EXEC_V2_MAX_OPEN_ROOTS_PER_BOND: usize = 4;

/// The most authorised executors a root names besides its own bond.
pub const PALW_EXEC_V2_MAX_EXTRA_EXECUTORS: usize = 7;

/// The most accepted-but-unverified slices one root may hold: after predecessor acceptance the next slice may follow before
/// verification, up to this depth. Pending does not mean verified or Final.
pub const PALW_EXEC_V2_MAX_PENDING_DEPTH: u32 = 4;

/// The most accepted-but-unverified slices one bond may hold across every root, as an executor.
pub const PALW_EXEC_V2_MAX_PENDING_PER_BOND: u32 = 16;

/// The most slices one accepting block folds. The covered set is in canonical order, so the first this-many win and the rest
/// are skipped by name (`BlockQuota`) — deterministic under every arrival order, and independent of the transaction lane's width.
pub const PALW_EXEC_V2_MAX_SLICES_PER_BLOCK: usize = 8;

/// The most DAA a root may stay open from its declaration (a hard cap on the declared expiry).
pub const PALW_EXEC_V2_MAX_ROOT_LIFETIME_DAA: u64 = 100_000;

/// The most canonical work one root may plan (2^48): a bound on the plan, not a price. The reward is the claim's own funded
/// allocation split by work share, so a larger plan only dilutes the share per unit.
pub const PALW_EXEC_V2_MAX_ROOT_WORK: u64 = 1 << 48;

/// The domain of a job's work identity.
pub const PALW_EXEC_V2_JOB_WORK_DOMAIN: &[u8] = b"misaka-palw/exec-v2/job-work/v1";
/// The domain of a root declaration's identity (what the root bond signs).
pub const PALW_EXEC_V2_ROOT_DECL_DOMAIN: &[u8] = b"misaka-palw/exec-v2/root-declaration/v1";
/// The ML-DSA-87 context a root declaration is signed under.
pub const PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/exec-v2/root/mldsa87/v1";

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn update_bond(state: &mut blake2b_simd::State, bond: &PalwBondKeyV2) {
    state.update(bond.0.transaction_id.as_byte_slice());
    state.update(&bond.0.index.to_le_bytes());
}

/// **A root declaration** (`ExecWorkRootOpenedV2`): the tx-carried act that opens a REAL claim's work session. It binds an
/// **already accepted** REAL claim (`root_claim_id`) and earns nothing by itself — the claim's own admitted work is the
/// prefix, and no weight, reward or clock term comes from the declaration.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWorkRootDeclarationV2 {
    /// The accepted REAL claim this session hangs from; its bond is the root executor, and its chain block is the root's anchor.
    pub root_claim_id: Hash64,
    /// The job: what the class's canonical job identity says this session computes (bound to the claim's `job_identity`).
    pub canonical_job_id: Hash64,
    /// The committed job input.
    pub input_root: Hash64,
    /// The kernel version the plan and the checks run under.
    pub kernel_version: u32,
    /// The `VerificationPlan` root.
    pub plan_root: Hash64,
    /// The job's total canonical work, root prefix included.
    pub total_work: u64,
    /// The plan: `boundaries[0]` is the root's own admitted prefix (the claim's canonical work), `boundaries[i]..boundaries[i+1]`
    /// is slice `i`, `boundaries.last()` is `total_work`. Strictly increasing.
    pub boundaries: Vec<u64>,
    /// The state root at the end of the prefix: slice 0's predecessor. **GAP-62 (X8R round 3):** where the kernel route is in force
    /// it is not the root bond's to choose — it must be `token_state(prompt ‖ generated)` of [`Self::prefix_claim`], whose prompt is
    /// the REAL claim's anchored prompt and whose run is the REAL claim's committed output.
    pub initial_state_root: Hash64,
    /// **GAP-62: the prefix claim** — the kernel-route program claim, by the root bond, that carries the REAL claim's own run (the
    /// prefix `[0, boundaries[0])`) as public, adjudicable material (`palw_exec_v2_prefix_binding_v1`). Unread where no kernel route
    /// is in force (the arithmetic chains), where the prefix stays the REAL claim's own route's.
    pub prefix_claim: Hash64,
    /// The DA and evidence policy the slices' commitments are held to.
    pub evidence_policy_root: Hash64,
    /// Authorised executors besides the root bond: strictly ascending, at most [`PALW_EXEC_V2_MAX_EXTRA_EXECUTORS`].
    pub extra_executors: Vec<PalwBondKeyV2>,
    /// The DAA at which an unfinished root expires.
    pub expiry_daa: u64,
    /// The root bond's ML-DSA-87 signature over [`Self::id`] under [`PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT`].
    pub signature: Vec<u8>,
}

impl PalwWorkRootDeclarationV2 {
    /// The declaration's identity: every field but the signature, in order.
    pub fn id(&self) -> Hash64 {
        let mut state = keyed(PALW_EXEC_V2_ROOT_DECL_DOMAIN);
        state.update(self.root_claim_id.as_byte_slice());
        state.update(self.canonical_job_id.as_byte_slice());
        state.update(self.input_root.as_byte_slice());
        state.update(&self.kernel_version.to_le_bytes());
        state.update(self.plan_root.as_byte_slice());
        state.update(&self.total_work.to_le_bytes());
        state.update(&(self.boundaries.len() as u64).to_le_bytes());
        for boundary in &self.boundaries {
            state.update(&boundary.to_le_bytes());
        }
        state.update(self.initial_state_root.as_byte_slice());
        state.update(self.prefix_claim.as_byte_slice());
        state.update(self.evidence_policy_root.as_byte_slice());
        state.update(&(self.extra_executors.len() as u64).to_le_bytes());
        for bond in &self.extra_executors {
            update_bond(&mut state, bond);
        }
        state.update(&self.expiry_daa.to_le_bytes());
        finish(state)
    }

    /// **The job's work identity** — the key of [`PalwExecV2StateV1::jobs`]. It binds the class, the canonical job, its input,
    /// the initial boundary state, the whole plan and the kernel, and **not** the root claim, the executors or the branch, so
    /// copying a job into a new root cannot earn the same work twice in a canonical history. Repeated-job semantics come from
    /// the existing ticket rules: `canonical_job_id` is the REAL claim's own job identity, which differs per attempt.
    pub fn job_work_id(&self, class_id: &Hash64) -> Hash64 {
        let mut state = keyed(PALW_EXEC_V2_JOB_WORK_DOMAIN);
        state.update(class_id.as_byte_slice());
        state.update(self.canonical_job_id.as_byte_slice());
        state.update(self.input_root.as_byte_slice());
        state.update(self.initial_state_root.as_byte_slice());
        state.update(&self.kernel_version.to_le_bytes());
        state.update(self.plan_root.as_byte_slice());
        state.update(&(self.boundaries.len() as u64).to_le_bytes());
        for boundary in &self.boundaries {
            state.update(&boundary.to_le_bytes());
        }
        state.update(&self.total_work.to_le_bytes());
        finish(state)
    }

    /// The shape a stateless reader can check: a valid plan and roots that are set.
    pub fn validate_shape(&self) -> Result<(), PalwWorkRootRefusalV2> {
        palw_work_plan_validate_v2(&self.boundaries, self.total_work)?;
        let roots = [
            ("root_claim_id", &self.root_claim_id),
            ("canonical_job_id", &self.canonical_job_id),
            ("input_root", &self.input_root),
            ("plan_root", &self.plan_root),
            ("initial_state_root", &self.initial_state_root),
            ("evidence_policy_root", &self.evidence_policy_root),
        ];
        for (name, root) in roots {
            if root.as_byte_slice().iter().all(|b| *b == 0) {
                return Err(PalwWorkRootRefusalV2::RootUnset(name));
            }
        }
        if self.extra_executors.len() > PALW_EXEC_V2_MAX_EXTRA_EXECUTORS {
            return Err(PalwWorkRootRefusalV2::TooManyExecutors(self.extra_executors.len()));
        }
        if self.extra_executors.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(PalwWorkRootRefusalV2::ExecutorsNotCanonical);
        }
        Ok(())
    }
}

/// The domain a root declaration's signed message is hashed under (network-bound; the id alone is not).
pub const PALW_EXEC_V2_ROOT_SIGNING_DOMAIN: &[u8] = b"misaka-palw/exec-v2/root-declaration/signing/v1";

impl PalwWorkRootDeclarationV2 {
    /// **What the root bond signs**: the declaration's identity bound to the network, in the root signing domain — so a declaration
    /// signed for one network is no declaration on another.
    pub fn signing_message(&self, network_domain: Hash64) -> Hash64 {
        let mut state = keyed(PALW_EXEC_V2_ROOT_SIGNING_DOMAIN);
        state.update(network_domain.as_byte_slice());
        state.update(self.id().as_byte_slice());
        finish(state)
    }
}

/// **The acceptance layer's check of a root declaration's signature**: the declaration names its claim; the claim's bond is the root
/// executor; its registered key must have signed [`PalwWorkRootDeclarationV2::signing_message`] under
/// [`PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT`]. `verify` is the ML-DSA-87 verifier `(pubkey, message, signature, context)`. State-free of
/// everything but the bond registry and the claim row — the fold re-checks the claim's phase and the rest.
pub fn palw_work_root_verify_signature_v2<V>(
    state: &crate::palw_state_v2::PalwChainStateV2,
    network_domain: Hash64,
    declaration: &PalwWorkRootDeclarationV2,
    verify: V,
) -> Result<(), PalwWorkRootRefusalV2>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let claim = state.claim(&declaration.root_claim_id).ok_or(PalwWorkRootRefusalV2::NoClaim)?;
    let bond = state.bond(&claim.bond).ok_or(PalwWorkRootRefusalV2::NotSigned)?;
    let message = declaration.signing_message(network_domain);
    if verify(&bond.pubkey, message.as_byte_slice(), &declaration.signature, PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT) {
        Ok(())
    } else {
        Err(PalwWorkRootRefusalV2::NotSigned)
    }
}

/// Why a root declaration was refused (each leaves no row).
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwWorkRootRefusalV2 {
    #[error("the work-slice lane is not in force at this height")]
    Dormant,
    #[error("the root's {0} is unset (all zero)")]
    RootUnset(&'static str),
    #[error("the plan is not a partition: {0}")]
    PlanInvalid(&'static str),
    #[error("the plan names {0} slices, above the per-root bound")]
    TooManySlices(usize),
    #[error("the root names {0} extra executors, above the bound")]
    TooManyExecutors(usize),
    #[error("the root's extra executors are not strictly ascending")]
    ExecutorsNotCanonical,
    #[error("the root claim is unknown to this chain")]
    NoClaim,
    #[error("the root claim is not a live REAL attempt in a phase that may open a session")]
    ClaimNotOpenable,
    #[error("the root claim already has a session")]
    RootExists,
    #[error("the root declaration is not signed by the root claim's bond")]
    NotSigned,
    #[error("the root's class or canonical job is not the claim's")]
    ClaimBindingMismatch,
    #[error("the root claim's canonical work cannot be derived, so no prefix can be bound")]
    ClaimWorkNotDerivable,
    #[error("the plan's prefix {declared} is not the root claim's admitted canonical work {admitted}")]
    PrefixMismatch { declared: u64, admitted: u64 },
    #[error("the plan's total work {0} is above the per-root bound")]
    TotalWorkTooLarge(u64),
    #[error("the root's expiry is not within its lifetime bound")]
    BadExpiry,
    #[error("this job's work was already used by another root (its work identity is recorded)")]
    JobWorkAlreadyUsed,
    #[error("the root claim reserves nothing, so an executor cannot stand behind it")]
    ClaimUnreserved,
    #[error("an executor bond is unknown, inactive or cannot fund the root's exposure")]
    ExecutorUnfunded,
    #[error("the lane already holds the most open roots, or this bond the most of its own")]
    RootQuota,
    #[error("checked arithmetic overflowed")]
    Overflow,
    /// Amendment 1 (spec §10.1): where the kernel route is in force a session opens only on a G14-complete class — one bound to a kernel
    /// class (onboarding tag 106), so every slice can be verified, convicted or defaulted through that route.
    #[error("the root claim's class is not bound to a kernel class: its slices would have no verification route")]
    ClassNotKernelBound,
    /// Amendment 1: the session's plan root is the bound kernel class's verification plan.
    #[error("the root's plan root is not its class's kernel verification plan")]
    PlanNotKernels,
    /// **GAP-62 (X8R round 3): the REAL claim's public job cannot be derived** — its class has no IR record (facts), its context is too
    /// narrow for a canonical job, or its job identity is unrecorded — so no prefix claim can be checked against it.
    #[error("the root claim's anchored prompt and job context cannot be derived, so its prefix cannot be bound")]
    PrefixNotDerivable,
    /// GAP-62: the declaration names no kernel claim the route holds.
    #[error("the declaration's prefix claim is not held by the kernel route")]
    PrefixClaimMissing,
    /// GAP-62: the prefix claim is a pipeline claim (pipeline-class sessions await a segment state, as their slices do).
    #[error("the prefix claim is not a single-program kernel claim")]
    PrefixKindUnsupported,
    /// GAP-62: the prefix claim was convicted, defaulted or timed out.
    #[error("the prefix claim has failed (convicted, defaulted or timed out)")]
    PrefixClaimFailed,
    /// GAP-62: the prefix claim does not carry the REAL claim's run — the named field (class, executor, job, job nonce, prompt,
    /// output) differs.
    #[error("the prefix claim does not carry the root claim's run: its {0} differs")]
    PrefixNotBound(&'static str),
    /// GAP-62: the declared initial boundary is not `token_state(prompt ‖ generated)` of the prefix claim.
    #[error("the initial boundary is not the token state of the REAL claim's prompt and committed output")]
    InitialBoundaryNotLinked,
}

/// **Validate a plan**: `boundaries` is a strictly increasing list of at least two values whose first is the (positive) root
/// prefix and whose last is `total_work`; every consecutive pair is one slice's range, so the ranges are disjoint, ordered and
/// cover `[boundaries[0], total_work)` exactly — no gap and no overlap — and the slice count is bounded.
pub fn palw_work_plan_validate_v2(boundaries: &[u64], total_work: u64) -> Result<(), PalwWorkRootRefusalV2> {
    use PalwWorkRootRefusalV2::PlanInvalid;
    if boundaries.len() < 2 {
        return Err(PlanInvalid("a plan needs the prefix boundary and at least one slice"));
    }
    let slices = boundaries.len() - 1;
    if slices > PALW_EXEC_V2_MAX_SLICES_PER_ROOT as usize {
        return Err(PalwWorkRootRefusalV2::TooManySlices(slices));
    }
    if boundaries[0] == 0 {
        return Err(PlanInvalid("the root prefix is zero: a root with no admitted work earns no session"));
    }
    if boundaries.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(PlanInvalid("boundaries are not strictly increasing (an empty or inverted slice)"));
    }
    if *boundaries.last().expect("two boundaries") != total_work {
        return Err(PlanInvalid("the last boundary is not the total work"));
    }
    if total_work > PALW_EXEC_V2_MAX_ROOT_WORK {
        return Err(PalwWorkRootRefusalV2::TotalWorkTooLarge(total_work));
    }
    Ok(())
}

/// The canonical range of slice `index` under `boundaries`, or `None` past the last slice.
pub fn palw_work_plan_range_v2(boundaries: &[u64], index: u32) -> Option<PalwWorkRangeV1> {
    let i = index as usize;
    let start = *boundaries.get(i)?;
    let end = *boundaries.get(i + 1)?;
    Some(PalwWorkRangeV1 { start, end })
}

/// **The lifecycle a root holds** (spec §4; the design states `PositivelyVerified`, `WindowClosed` and `Final` are the *claim's*
/// own — a root only waits for them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwWorkRootPhaseV2 {
    /// Declared; slices may be accepted in canonical order.
    Open,
    /// Every planned slice is accepted (not verified). The claim still waits for verification of each.
    Complete,
    /// A slice was proved false (or the session failed): the suffix from `from_index` is void and the root never settles.
    Voided { from_index: u32, voided_daa: u64 },
    /// The one settlement was made at the claim's `Final`.
    Settled { settled_daa: u64 },
}

/// A root: **`RootWorkBudget`**. Everything a slice is checked against is here, so a repeated field on a slice is verified, never
/// trusted.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWorkRootV2 {
    pub class_id: Hash64,
    pub canonical_job_id: Hash64,
    pub job_work_id: Hash64,
    pub kernel_version: u32,
    pub plan_root: Hash64,
    pub total_work: u64,
    pub boundaries: Vec<u64>,
    pub initial_state_root: Hash64,
    pub evidence_policy_root: Hash64,
    /// The root claim's bond: the root executor, and always authorised.
    pub root_bond: PalwBondKeyV2,
    /// The other authorised executors, ascending.
    pub extra_executors: Vec<PalwBondKeyV2>,
    /// What each extra executor stands behind for this root (the root claim's own reservation), held in `reserved_exposure` for the
    /// root's life. Recorded so the release returns exactly what the open took.
    pub executor_exposure: u128,
    /// The REAL chain anchor: the block that carried the root claim.
    pub anchor: BlockHash,
    pub opened_daa: u64,
    pub expiry_daa: u64,
    /// The next canonical slice index.
    pub next_index: u32,
    /// The state root the next slice must start from: the initial root, then each accepted slice's result.
    pub last_state_root: Hash64,
    /// Credited (accepted) slice work: `sum(range.work())`, root prefix excluded.
    pub accepted_work: u64,
    /// Of it, the work of slices whose verification is positive.
    pub verified_work: u64,
    /// Accepted but unverified slices.
    pub pending: u32,
    pub phase: PalwWorkRootPhaseV2,
    /// **GAP-62: the prefix claim** (the declaration's), and its stage. `Unbound` where no kernel route was in force at the declaration.
    pub prefix_claim: Hash64,
    pub prefix: PalwWorkPrefixStageV2,
}

/// **The stage of a root's own prefix** (GAP-62, X8R round 3): the REAL claim's run, carried by the root bond's prefix claim on the
/// kernel route and followed exactly as a slice follows its claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwWorkPrefixStageV2 {
    /// No kernel route was in force at the declaration: nothing binds the prefix, which stays the REAL claim's own route's (the
    /// arithmetic chains of the fold tests and the pipeline tests below the route).
    Unbound,
    /// The prefix claim is live and undecided: the root's `Final` waits.
    Pending,
    /// The prefix claim reached `Final`; `route_reward` is the route's own `Final` reward on it (netted from the root's allocation at
    /// settlement: ADR-0176 D2, one work, one right).
    Verified { verified_daa: u64, route_reward: u64 },
    /// The prefix claim was convicted: the REAL claim's run is proven false and the whole session is void.
    ProvenFalse { daa: u64 },
    /// The prefix claim defaulted (withheld material, a timeout, a forfeit after `Final`): the whole session is void.
    Defaulted { daa: u64 },
}

impl PalwWorkPrefixStageV2 {
    /// Does the prefix let the root reach its `Final`?
    pub fn admits_final(&self) -> bool {
        matches!(self, Self::Unbound | Self::Verified { .. })
    }

    /// The route reward the prefix's claim was paid (0 unless verified).
    pub fn route_reward(&self) -> u64 {
        match self {
            Self::Verified { route_reward, .. } => *route_reward,
            _ => 0,
        }
    }
}

impl PalwWorkRootV2 {
    /// The root executor's own admitted work: the prefix.
    pub fn prefix_work(&self) -> u64 {
        self.boundaries.first().copied().unwrap_or(0)
    }

    /// The number of planned slices.
    pub fn slice_count(&self) -> u32 {
        self.boundaries.len().saturating_sub(1) as u32
    }

    /// The work the slices have to cover: `total_work - prefix`.
    pub fn slice_work_total(&self) -> u64 {
        self.total_work.saturating_sub(self.prefix_work())
    }

    /// Is `bond` an authorised executor of this root (the root bond, or an extra)?
    pub fn authorises(&self, bond: &PalwBondKeyV2) -> bool {
        *bond == self.root_bond || self.extra_executors.binary_search(bond).is_ok()
    }

    /// **Is the root ready for the claim's `Final`?** Every planned slice accepted and verified, the arithmetic closed, not voided
    /// or already settled. A root that is not ready holds its claim's `Final` (the claim owes no deadline while it waits).
    pub fn ready_for_final(&self) -> bool {
        matches!(self.phase, PalwWorkRootPhaseV2::Complete)
            && self.pending == 0
            && self.next_index == self.slice_count()
            && self.accepted_work == self.slice_work_total()
            && self.verified_work == self.accepted_work
            && self.prefix.admits_final()
    }

    /// Is the root still able to accept slices?
    pub fn is_open(&self) -> bool {
        matches!(self.phase, PalwWorkRootPhaseV2::Open)
    }
}

/// **The stage of one accepted slice.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwWorkSliceStageV2 {
    /// Accepted into the lane; not verified, not Final.
    Pending,
    /// Positively verified: its kernel-route claim reached `Final` (amendment 1, spec §10.1). `leg_cap` is the most its executor may be
    /// paid for it at the root's settlement — the reservation its kernel claim held when it verified, less the route's own `Final`
    /// reward on that claim ([`crate::palw_exec_v2_verify::palw_exec_v2_leg_cap_v1`], spec §10.5 as revised by the X8R round-2
    /// review): fixed when the slice verifies, so no later release of the reservation and no timing of the root's `Final` moves it.
    ///
    /// `route_reward` (X8R round 3, ADR-0176 D2) is the route's `Final` reward paid on that claim, snapshotted with the cap: the
    /// settlement nets it from the slice's share of the root's one allocation, so the work is paid once.
    Verified { verified_daa: u64, leg_cap: u64, route_reward: u64 },
    /// Void: the root failed from this slice back (a false predecessor voids its dependents), or the root expired.
    Voided { voided_daa: u64 },
    /// **Amendment 1: its kernel claim was convicted** — the slice is proven false; it and every later slice of its root are void.
    ProvenFalse { daa: u64 },
    /// **Amendment 1: its kernel claim defaulted** (withheld material, a timeout) — the slice can never be verified; it and every later
    /// slice of its root are void.
    Defaulted { daa: u64 },
}

/// **`WorkSliceUse`**: one accepted slice.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWorkSliceRowV2 {
    pub slice_id: Hash64,
    pub range: PalwWorkRangeV1,
    pub executor: PalwBondKeyV2,
    /// The EXEC block that carried it.
    pub carrier: BlockHash,
    pub accepted_daa: u64,
    pub predecessor_state_root: Hash64,
    pub result_state_root: Hash64,
    pub evidence_root: Hash64,
    pub da_root: Hash64,
    pub stage: PalwWorkSliceStageV2,
}

/// **All of the lane's work-slice rows.** Empty — and then neither rooted nor carried — on every chain below
/// `Params::palw_exec_payload_v2`.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecV2StateV1 {
    /// `RootWorkBudget`, keyed by the root claim id.
    pub roots: std::collections::BTreeMap<Hash64, PalwWorkRootV2>,
    /// `WorkSliceUse`, keyed `(root claim, slice index)`.
    pub slices: std::collections::BTreeMap<(Hash64, u32), PalwWorkSliceRowV2>,
    /// `JobWorkUse`: a job's work identity to the root that holds it.
    pub jobs: std::collections::BTreeMap<Hash64, Hash64>,
    /// **The EXEC blocks an anchor has covered**, by block hash to the span of the block's anchor: what bounds the closure walk
    /// (a covered block is a boundary, covered once and never walked past) and what stops one carrier being accepted twice through
    /// two anchors. Holds only blocks inside the two-span window — an older entry is dropped by the next anchoring block, and a
    /// block that old is outside the window anyway, so it could not be covered again.
    pub anchored: std::collections::BTreeMap<Hash64, u64>,
}

impl PalwExecV2StateV1 {
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty() && self.slices.is_empty() && self.jobs.is_empty() && self.anchored.is_empty()
    }

    /// Open roots whose root executor is `bond`.
    pub fn open_roots_of(&self, bond: &PalwBondKeyV2) -> usize {
        self.roots.values().filter(|root| root.root_bond == *bond && root.is_open()).count()
    }

    /// Accepted-but-unverified slices `bond` holds as an executor, across every root.
    pub fn pending_of(&self, bond: &PalwBondKeyV2) -> u32 {
        self.slices.values().filter(|row| row.executor == *bond && matches!(row.stage, PalwWorkSliceStageV2::Pending)).count() as u32
    }
}

/// **One slice carrier the accepting block's covered set holds, as the fold receives it**: the carrier's block hash, the slice
/// (decoded from a header-validated `PXE2` envelope whose signature the header stage already verified under `pubkey`), and the
/// key that signed. The processor builds the list in canonical order; the fold re-judges each against its own state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecV2CoveredSliceV1 {
    pub carrier: BlockHash,
    pub slice: crate::palw_exec_v2::PalwWorkSliceV1,
    pub pubkey: Vec<u8>,
}

/// What an admitted slice contributes: the work its range credits, and — where its kernel claim is already Final when the slice is
/// admitted (amendment 1) — the Final that verifies it at once, with the leg cap that claim gives it (0 when not verified).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSliceAdmissionV2 {
    pub work: u64,
    pub verified_at: Option<u64>,
    pub leg_cap: u64,
    /// The route's `Final` reward on the claim when it is already Final at admission (0 otherwise).
    pub route_reward: u64,
}

/// **The named refusals of slice admission, in the spec's order** (§3 rules 1–6). A refused carrier writes nothing.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwSliceRefusalV2 {
    #[error("the work-slice lane is not in force at this height")]
    Dormant,
    // ---- 1: the root ----
    #[error("1: no such root on this chain")]
    NoRoot,
    #[error("1: the root is not open (complete, void or settled)")]
    RootNotOpen,
    #[error("1: the root expired")]
    RootExpired,
    #[error("1: the root claim is gone, Final or no longer live")]
    RootClaimNotLive,
    // ---- 2: the bond ----
    #[error("2: the bond is not an authorised executor of this root")]
    ExecutorNotAuthorized,
    #[error("2: the bond is unknown or not active")]
    ExecutorNotActive,
    #[error("2: the carried key is not the bond's registered key")]
    ExecutorKeyMismatch,
    // ---- 3: the index and the range ----
    #[error("3: this (root, index) was already used")]
    IndexUsed,
    #[error("3: the index skips ahead of the next canonical slice")]
    SkippedSlice,
    #[error("3: the index is past the plan")]
    IndexPastPlan,
    #[error("3: the range overlaps the root prefix or an accepted slice")]
    RangeOverlap,
    #[error("3: the range is not the plan's range for this index")]
    RangeNotPlan,
    // ---- 4: the predecessor ----
    #[error("4: the predecessor is not the root's committed boundary")]
    PredecessorMismatch,
    // ---- 5: the bindings ----
    #[error("5: the slice's class is not the root's")]
    ClassMismatch,
    #[error("5: the slice's job is not the root's")]
    JobMismatch,
    #[error("5: the slice's kernel version is not the root's")]
    KernelMismatch,
    #[error("5: the slice's verification plan is not the root's")]
    PlanMismatch,
    // ---- 6: limits ----
    #[error("6: the root holds the most pending slices")]
    RootPendingDepth,
    #[error("6: the bond holds the most pending slices")]
    BondPendingDepth,
    #[error("6: the block already folded the most slices")]
    BlockQuota,
    #[error("checked arithmetic overflowed")]
    Overflow,
    // ---- 5 (amendment 1, spec §10.1): the verification binding — the kernel claim the slice names ----
    #[error("5: the root's class has no kernel binding, so no verification route")]
    NotKernelBound,
    #[error("5: the kernel route holds no claim with the id the slice names")]
    VerificationClaimMissing,
    #[error("5: the named kernel claim is a pipeline claim: pipeline-class slices await a segment state")]
    VerificationKindUnsupported,
    #[error("5: the named kernel claim does not bind the slice ({0})")]
    VerificationClaimNotBound(&'static str),
    #[error("5: the named kernel claim already failed (convicted, unavailable or timed out)")]
    VerificationClaimFailed,
}

impl PalwSliceRefusalV2 {
    /// **The refusal's pinned code** — what the refusal record (`PalwDeltaEntryV2::ExecV2Verdict`) and the RPC carry. `0` is an admitted
    /// slice; codes are never reused or renumbered.
    pub fn code(&self) -> u8 {
        use PalwSliceRefusalV2 as R;
        match self {
            R::Dormant => 1,
            R::NoRoot => 2,
            R::RootNotOpen => 3,
            R::RootExpired => 4,
            R::RootClaimNotLive => 5,
            R::ExecutorNotAuthorized => 6,
            R::ExecutorNotActive => 7,
            R::ExecutorKeyMismatch => 8,
            R::IndexUsed => 9,
            R::SkippedSlice => 10,
            R::IndexPastPlan => 11,
            R::RangeOverlap => 12,
            R::RangeNotPlan => 13,
            R::PredecessorMismatch => 14,
            R::ClassMismatch => 15,
            R::JobMismatch => 16,
            R::KernelMismatch => 17,
            R::PlanMismatch => 18,
            R::RootPendingDepth => 19,
            R::BondPendingDepth => 20,
            R::BlockQuota => 21,
            R::Overflow => 22,
            R::NotKernelBound => 23,
            R::VerificationClaimMissing => 24,
            R::VerificationKindUnsupported => 25,
            R::VerificationClaimNotBound(_) => 26,
            R::VerificationClaimFailed => 27,
        }
    }

    /// The name a code stands for (`admitted` for 0), for a reader of the refusal record.
    pub fn name_of_code(code: u8) -> &'static str {
        match code {
            0 => "admitted",
            1 => "dormant",
            2 => "no_root",
            3 => "root_not_open",
            4 => "root_expired",
            5 => "root_claim_not_live",
            6 => "executor_not_authorized",
            7 => "executor_not_active",
            8 => "executor_key_mismatch",
            9 => "index_used",
            10 => "skipped_slice",
            11 => "index_past_plan",
            12 => "range_overlap",
            13 => "range_not_plan",
            14 => "predecessor_mismatch",
            15 => "class_mismatch",
            16 => "job_mismatch",
            17 => "kernel_mismatch",
            18 => "plan_mismatch",
            19 => "root_pending_depth",
            20 => "bond_pending_depth",
            21 => "block_quota",
            22 => "overflow",
            23 => "not_kernel_bound",
            24 => "verification_claim_missing",
            25 => "verification_kind_unsupported",
            26 => "verification_claim_not_bound",
            27 => "verification_claim_failed",
            _ => "unknown",
        }
    }
}

/// **One root's single settlement** (spec §4): the claim's funded allocation `allocation` (the root executor's leg of the reward)
/// split by work share. Each slice executor is paid `floor(allocation × credited_work / total_work)` — summed per executor — and
/// the root executor keeps the rest, which is its prefix share plus the division's remainder. So:
///
/// * `sum(paid) == allocation` exactly (nothing minted, nothing lost, no dust stranded);
/// * no slice leg exceeds its work's proportion of the allocation;
/// * an executor's leg depends only on the credited ranges, never on the number of slices or carriers.
///
/// `slices` are the credited slices' `(executor, work)`; `None` on overflow or when a slice's work does not add up to the root's
/// plan (the settlement is only for a fully credited root).
pub fn settle_root_v2(
    allocation: u64,
    total_work: u64,
    prefix_work: u64,
    slices: &[(PalwBondKeyV2, u64)],
    root_bond: PalwBondKeyV2,
) -> Option<PalwRootSettlementV2> {
    let credited: u64 = slices.iter().try_fold(0u64, |sum, (_, work)| sum.checked_add(*work))?;
    if prefix_work.checked_add(credited)? != total_work || total_work == 0 {
        return None;
    }
    settle_core_v2(allocation, allocation, total_work, slices, root_bond)
}

/// **A partial settlement** — the defensive door for a root that reaches the claim's `Final` without every slice verified (the
/// claim holds its `Final` while the root is not ready, so this is unreachable by construction; it is here so that if the
/// invariant were ever broken the chain pays only what was verified and never wedges). Only the prefix and the *verified*
/// slices are payable: `payable = floor(allocation × (prefix + verified) / total)`; the rest of the allocation is never named
/// (never minted). Shares are still `floor(allocation × work / total)`, and the root keeps `payable − Σ shares`.
pub fn settle_root_partial_v2(
    allocation: u64,
    total_work: u64,
    prefix_work: u64,
    verified: &[(PalwBondKeyV2, u64)],
    root_bond: PalwBondKeyV2,
) -> Option<PalwRootSettlementV2> {
    let credited: u64 = verified.iter().try_fold(0u64, |sum, (_, work)| sum.checked_add(*work))?;
    let paid_work = prefix_work.checked_add(credited)?;
    if paid_work > total_work || total_work == 0 {
        return None;
    }
    let payable = u64::try_from((allocation as u128).checked_mul(paid_work as u128)? / total_work as u128).ok()?;
    settle_core_v2(allocation, payable, total_work, verified, root_bond)
}

fn settle_core_v2(
    allocation: u64,
    payable: u64,
    total_work: u64,
    slices: &[(PalwBondKeyV2, u64)],
    root_bond: PalwBondKeyV2,
) -> Option<PalwRootSettlementV2> {
    let mut legs: std::collections::BTreeMap<PalwBondKeyV2, u64> = std::collections::BTreeMap::new();
    let mut slice_paid: u64 = 0;
    for (executor, work) in slices {
        let share = u64::try_from((allocation as u128).checked_mul(*work as u128)? / total_work as u128).ok()?;
        slice_paid = slice_paid.checked_add(share)?;
        let leg = legs.entry(*executor).or_insert(0);
        *leg = leg.checked_add(share)?;
    }
    let root_leg = payable.checked_sub(slice_paid)?;
    // The root executor may itself have executed slices: its legs merge into its one payout.
    let merged_root = legs.remove(&root_bond).unwrap_or(0);
    let root_total = root_leg.checked_add(merged_root)?;
    let others: Vec<(PalwBondKeyV2, u64)> = legs.into_iter().filter(|(_, amount)| *amount > 0).collect();
    Some(PalwRootSettlementV2 { root_leg: root_total, slice_legs: others })
}

/// A settlement: what the root executor is paid and what each other executor is paid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwRootSettlementV2 {
    pub root_leg: u64,
    pub slice_legs: Vec<(PalwBondKeyV2, u64)>,
}

impl PalwRootSettlementV2 {
    /// The total paid: the allocation, exactly.
    pub fn total(&self) -> Option<u64> {
        self.slice_legs.iter().try_fold(self.root_leg, |sum, (_, amount)| sum.checked_add(*amount))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(v), 0))
    }

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn decl() -> PalwWorkRootDeclarationV2 {
        PalwWorkRootDeclarationV2 {
            root_claim_id: h(1),
            canonical_job_id: h(2),
            input_root: h(3),
            kernel_version: 1,
            plan_root: h(4),
            total_work: 100,
            boundaries: vec![10, 40, 70, 100],
            initial_state_root: h(5),
            prefix_claim: h(11),
            evidence_policy_root: h(6),
            extra_executors: vec![bond(8), bond(9)],
            expiry_daa: 5_000,
            signature: vec![0; 8],
        }
    }

    #[test]
    fn a_plan_is_a_partition_with_no_gap_and_no_overlap() {
        let d = decl();
        assert_eq!(d.validate_shape(), Ok(()));
        // The ranges are exactly the consecutive boundaries.
        let ranges: Vec<_> = (0..3).map(|i| palw_work_plan_range_v2(&d.boundaries, i).unwrap()).collect();
        assert_eq!(ranges[0], PalwWorkRangeV1 { start: 10, end: 40 });
        assert_eq!(ranges[2], PalwWorkRangeV1 { start: 70, end: 100 });
        assert!(palw_work_plan_range_v2(&d.boundaries, 3).is_none());
        for (i, a) in ranges.iter().enumerate() {
            for b in ranges.iter().skip(i + 1) {
                assert!(!a.overlaps(b));
            }
        }
        let covered: u64 = ranges.iter().map(|r| r.work().unwrap()).sum();
        assert_eq!(10 + covered, d.total_work, "prefix + slices == total");
        // Every way to break it.
        let bad = |boundaries: Vec<u64>, total| palw_work_plan_validate_v2(&boundaries, total);
        assert!(bad(vec![], 0).is_err());
        assert!(bad(vec![10], 10).is_err());
        assert!(bad(vec![0, 10], 10).is_err(), "a zero prefix");
        assert!(bad(vec![10, 10, 20], 20).is_err(), "an empty slice");
        assert!(bad(vec![10, 30, 20], 20).is_err(), "an inverted slice");
        assert!(bad(vec![10, 20, 30], 40).is_err(), "the last boundary is not the total");
        assert!(bad(vec![10, 20, 30], 25).is_err());
        assert!(bad(vec![10, 20], PALW_EXEC_V2_MAX_ROOT_WORK + 1).is_err());
        let many: Vec<u64> = (1..=(PALW_EXEC_V2_MAX_SLICES_PER_ROOT as u64 + 2)).collect();
        let total = *many.last().unwrap();
        assert!(matches!(bad(many, total), Err(PalwWorkRootRefusalV2::TooManySlices(_))));
        let ok: Vec<u64> = (1..=(PALW_EXEC_V2_MAX_SLICES_PER_ROOT as u64 + 1)).collect();
        let total = *ok.last().unwrap();
        assert_eq!(bad(ok, total), Ok(()), "the bound itself is allowed");
    }

    #[test]
    fn a_declaration_has_one_canonical_form_and_an_id_that_moves_with_every_field() {
        let base = decl();
        let id = base.id();
        type Mutation = Box<dyn Fn(&mut PalwWorkRootDeclarationV2)>;
        let mutations: Vec<(&str, Mutation)> = vec![
            ("root", Box::new(|d| d.root_claim_id = h(99))),
            ("job", Box::new(|d| d.canonical_job_id = h(99))),
            ("input", Box::new(|d| d.input_root = h(99))),
            ("kernel", Box::new(|d| d.kernel_version = 2)),
            ("plan", Box::new(|d| d.plan_root = h(99))),
            ("total", Box::new(|d| d.total_work = 101)),
            ("boundaries", Box::new(|d| d.boundaries[1] = 41)),
            ("initial", Box::new(|d| d.initial_state_root = h(99))),
            ("prefix claim", Box::new(|d| d.prefix_claim = h(99))),
            ("policy", Box::new(|d| d.evidence_policy_root = h(99))),
            ("executors", Box::new(|d| d.extra_executors.push(bond(10)))),
            ("expiry", Box::new(|d| d.expiry_daa += 1)),
        ];
        for (name, mutate) in mutations {
            let mut changed = base.clone();
            mutate(&mut changed);
            assert_ne!(changed.id(), id, "{name}");
        }
        // The signature is not part of the id.
        let mut resigned = base.clone();
        resigned.signature = vec![1; 8];
        assert_eq!(resigned.id(), id);
        // Executors must be strictly ascending and bounded.
        let mut unsorted = base.clone();
        unsorted.extra_executors = vec![bond(9), bond(8)];
        assert_eq!(unsorted.validate_shape(), Err(PalwWorkRootRefusalV2::ExecutorsNotCanonical));
        let mut dup = base.clone();
        dup.extra_executors = vec![bond(8), bond(8)];
        assert_eq!(dup.validate_shape(), Err(PalwWorkRootRefusalV2::ExecutorsNotCanonical));
        let mut many = base.clone();
        many.extra_executors = (10..30).map(bond).collect();
        assert!(matches!(many.validate_shape(), Err(PalwWorkRootRefusalV2::TooManyExecutors(_))));
    }

    #[test]
    fn the_job_work_identity_does_not_bind_the_root_the_executors_or_the_branch() {
        let a = decl();
        let mut b = decl();
        b.root_claim_id = h(777);
        b.extra_executors = vec![bond(30)];
        b.expiry_daa = 9;
        b.signature = vec![9; 8];
        assert_eq!(a.job_work_id(&h(50)), b.job_work_id(&h(50)), "copying the job into a new root keeps its work identity");
        // But everything the computation is made of moves it.
        assert_ne!(a.job_work_id(&h(50)), a.job_work_id(&h(51)), "class");
        let mut c = decl();
        c.input_root = h(9);
        assert_ne!(a.job_work_id(&h(50)), c.job_work_id(&h(50)), "input");
        let mut d = decl();
        d.boundaries = vec![10, 50, 70, 100];
        assert_ne!(a.job_work_id(&h(50)), d.job_work_id(&h(50)), "plan");
        let mut e = decl();
        e.canonical_job_id = h(9);
        assert_ne!(a.job_work_id(&h(50)), e.job_work_id(&h(50)), "job");
    }

    fn root_row() -> PalwWorkRootV2 {
        let d = decl();
        PalwWorkRootV2 {
            class_id: h(50),
            canonical_job_id: d.canonical_job_id,
            job_work_id: d.job_work_id(&h(50)),
            kernel_version: d.kernel_version,
            plan_root: d.plan_root,
            total_work: d.total_work,
            boundaries: d.boundaries.clone(),
            initial_state_root: d.initial_state_root,
            evidence_policy_root: d.evidence_policy_root,
            root_bond: bond(7),
            extra_executors: d.extra_executors.clone(),
            executor_exposure: 0,
            anchor: h(60),
            opened_daa: 100,
            expiry_daa: d.expiry_daa,
            next_index: 0,
            last_state_root: d.initial_state_root,
            accepted_work: 0,
            verified_work: 0,
            pending: 0,
            phase: PalwWorkRootPhaseV2::Open,
            prefix_claim: d.prefix_claim,
            prefix: PalwWorkPrefixStageV2::Unbound,
        }
    }

    #[test]
    fn a_root_is_ready_only_when_every_planned_slice_is_accepted_and_verified() {
        let mut root = root_row();
        assert_eq!(root.prefix_work(), 10);
        assert_eq!(root.slice_count(), 3);
        assert_eq!(root.slice_work_total(), 90);
        assert!(root.authorises(&bond(7)) && root.authorises(&bond(8)) && root.authorises(&bond(9)));
        assert!(!root.authorises(&bond(10)));
        assert!(!root.ready_for_final());
        root.next_index = 3;
        root.accepted_work = 90;
        root.phase = PalwWorkRootPhaseV2::Complete;
        root.pending = 3;
        assert!(!root.ready_for_final(), "accepted but unverified is not ready");
        root.pending = 0;
        root.verified_work = 60;
        assert!(!root.ready_for_final(), "partly verified is not ready");
        root.verified_work = 90;
        assert!(root.ready_for_final());
        // GAP-62: a bound prefix must be verified too.
        for (stage, ready) in [
            (PalwWorkPrefixStageV2::Pending, false),
            (PalwWorkPrefixStageV2::ProvenFalse { daa: 4 }, false),
            (PalwWorkPrefixStageV2::Defaulted { daa: 4 }, false),
            (PalwWorkPrefixStageV2::Verified { verified_daa: 4, route_reward: 7 }, true),
            (PalwWorkPrefixStageV2::Unbound, true),
        ] {
            root.prefix = stage;
            assert_eq!(root.ready_for_final(), ready, "{stage:?}");
        }
        root.phase = PalwWorkRootPhaseV2::Voided { from_index: 1, voided_daa: 5 };
        assert!(!root.ready_for_final(), "a void root never settles");
        root.phase = PalwWorkRootPhaseV2::Settled { settled_daa: 5 };
        assert!(!root.ready_for_final(), "a settled root settles once");
    }

    #[test]
    fn settlement_conserves_the_allocation_exactly_and_pays_work_share() {
        let root_bond = bond(7);
        let slices = [(bond(8), 30u64), (bond(9), 30), (bond(8), 30)];
        let s = settle_root_v2(1_000, 100, 10, &slices, root_bond).unwrap();
        assert_eq!(s.total(), Some(1_000), "the allocation, exactly");
        // 1000 * 30 / 100 = 300 each; bond 8 holds two slices.
        assert_eq!(s.slice_legs, vec![(bond(8), 600), (bond(9), 300)]);
        assert_eq!(s.root_leg, 100, "the prefix share");
        // Rounding: the remainder stays with the root.
        let s = settle_root_v2(1_001, 100, 10, &slices, root_bond).unwrap();
        assert_eq!(s.total(), Some(1_001));
        assert_eq!(s.slice_legs, vec![(bond(8), 600), (bond(9), 300)]);
        assert_eq!(s.root_leg, 101);
        // The root executor executing a slice merges into its one leg.
        let own = [(root_bond, 30u64), (bond(9), 60)];
        let s = settle_root_v2(1_000, 100, 10, &own, root_bond).unwrap();
        assert_eq!(s.total(), Some(1_000));
        assert_eq!(s.slice_legs, vec![(bond(9), 600)]);
        assert_eq!(s.root_leg, 400);
        // No allocation, no payment.
        let s = settle_root_v2(0, 100, 10, &slices, root_bond).unwrap();
        assert_eq!(s.total(), Some(0));
        assert!(s.slice_legs.is_empty());
        // A settlement only exists for a fully credited root, and never overflows.
        assert!(settle_root_v2(1_000, 100, 10, &slices[..2], root_bond).is_none(), "work missing");
        assert!(settle_root_v2(1_000, 100, 20, &slices, root_bond).is_none(), "work over");
        assert!(settle_root_v2(u64::MAX, u64::MAX, 1, &[(bond(8), u64::MAX - 1)], root_bond).is_some());
        assert!(settle_root_v2(1, 0, 0, &[], root_bond).is_none());
    }

    /// The reward does not depend on how the work was cut: N slices are not N times the reward.
    #[test]
    fn splitting_a_range_into_more_slices_pays_no_more() {
        let root_bond = bond(7);
        let coarse = settle_root_v2(10_000, 1_000, 100, &[(bond(8), 900)], root_bond).unwrap();
        let fine: Vec<(PalwBondKeyV2, u64)> = (0..90).map(|_| (bond(8), 10u64)).collect();
        let fine = settle_root_v2(10_000, 1_000, 100, &fine, root_bond).unwrap();
        assert_eq!(coarse.total(), Some(10_000));
        assert_eq!(fine.total(), Some(10_000));
        let coarse_leg: u64 = coarse.slice_legs.iter().map(|(_, a)| *a).sum();
        let fine_leg: u64 = fine.slice_legs.iter().map(|(_, a)| *a).sum();
        assert!(fine_leg <= coarse_leg, "floor division per slice can only lose dust, never gain");
        assert_eq!(coarse_leg, 9_000);
        assert!(coarse_leg - fine_leg <= 90, "at most one sompi of dust per slice");
        // The dust goes to the root, not out of the allocation.
        assert_eq!(fine.root_leg + fine_leg, 10_000);
    }

    #[test]
    fn row_types_round_trip_through_borsh_with_stable_stage_bytes() {
        let root = root_row();
        let bytes = borsh::to_vec(&root).unwrap();
        assert_eq!(borsh::from_slice::<PalwWorkRootV2>(&bytes).unwrap(), root);
        let mut state = PalwExecV2StateV1::default();
        assert!(state.is_empty());
        state.roots.insert(h(1), root);
        state.jobs.insert(h(2), h(1));
        assert!(!state.is_empty());
        let bytes = borsh::to_vec(&state).unwrap();
        assert_eq!(borsh::from_slice::<PalwExecV2StateV1>(&bytes).unwrap(), state);
        // Stage and phase discriminants are pinned: they are in the state root.
        assert_eq!(borsh::to_vec(&PalwWorkSliceStageV2::Pending).unwrap()[0], 0);
        assert_eq!(borsh::to_vec(&PalwWorkSliceStageV2::Verified { verified_daa: 1, leg_cap: 0, route_reward: 0 }).unwrap()[0], 1);
        assert_eq!(borsh::to_vec(&PalwWorkSliceStageV2::Voided { voided_daa: 1 }).unwrap()[0], 2);
        assert_eq!(borsh::to_vec(&PalwWorkRootPhaseV2::Open).unwrap()[0], 0);
        assert_eq!(borsh::to_vec(&PalwWorkRootPhaseV2::Complete).unwrap()[0], 1);
        assert_eq!(borsh::to_vec(&PalwWorkRootPhaseV2::Voided { from_index: 0, voided_daa: 0 }).unwrap()[0], 2);
        assert_eq!(borsh::to_vec(&PalwWorkRootPhaseV2::Settled { settled_daa: 0 }).unwrap()[0], 3);
    }
}
