//! **RFC-0008 v2 in the fold: the root declaration, the six-rule slice admission, the one settlement, the void and the expiry** —
//! dormant behind `Params::palw_exec_payload_v2`. A child module of `palw_state_v2`, as the vertex and held-close folds are, so it
//! reads the builder, the bond registry, the claims and the exposure ledger directly and writes the three work-slice tables only
//! through their one journaled writer. The wire types are [`crate::palw_exec_v2`]'s; the ledgers and pure rules are
//! [`crate::palw_work_slice_v2`]'s; `docs/design/palw/rfc-0008-implementation-spec.md` is normative.
//!
//! # What this module does and what it deliberately cannot do
//!
//! * **It opens a session** ([`apply_root_declared_v2`]) for an **already accepted** REAL attempt claim: the claim's own admitted
//!   canonical work is the plan's prefix, the job identity is the claim's, and each extra executor's exposure is reserved on its
//!   bond in the one committed ledger. A declaration earns no credit, no weight and no clock term.
//! * **It admits slices** ([`apply_covered_slices_v2`]) from the accepting block's covered set, in the processor's canonical order,
//!   judging each against the fold's *own* running state — so slice `i + 1` sees slice `i` accepted in the same block — by the
//!   spec's rules 1–6, in that order, naming the first that fails. A refused carrier writes **nothing**.
//! * **It holds the root claim's `Final`** while the root is not ready ([`final_is_held_v2`]): the claim owes no `Final` deadline
//!   (the same pause a DA session or an unaudited credit takes), so a complete-but-unverified root waits. A root that is not ready
//!   by its expiry voids its claim ([`sweep_expired_roots_v2`]) uncharged: silence and timeout are never convictions.
//! * **It settles once** ([`settle_at_final_v2`]): at the claim's `Final` the root executor's reward leg is split by work share —
//!   `sum(paid) == the allocation`, nothing minted — and the root is marked `Settled`. No slice has its own `Final`, subsidy or
//!   escrow.
//! * **It does not verify.** Nothing in this crate can mark a slice `Verified`: the production door (the RFC-0007 Part VI
//!   `WORK_SLICE` challenge, public-bond prosecution, the DA court) is not wired, so a real chain's root never becomes ready and
//!   never settles. The test-only writer [`mark_slice_verified_for_tests`] exists so the settlement arithmetic and the hold's
//!   release can be exercised; it is `#[cfg(test)]` and reachable by no node.

use super::*;
use crate::palw_exec_v2::PalwWorkRangeV1;
use crate::palw_work_slice_v2::{
    PALW_EXEC_V2_MAX_OPEN_ROOTS, PALW_EXEC_V2_MAX_OPEN_ROOTS_PER_BOND, PALW_EXEC_V2_MAX_PENDING_DEPTH,
    PALW_EXEC_V2_MAX_PENDING_PER_BOND, PALW_EXEC_V2_MAX_ROOT_LIFETIME_DAA, PALW_EXEC_V2_MAX_SLICES_PER_BLOCK,
    PalwExecV2CoveredSliceV1, PalwExecV2StateV1, PalwSliceAdmissionV2, PalwSliceRefusalV2, PalwWorkRootDeclarationV2,
    PalwWorkRootPhaseV2, PalwWorkRootRefusalV2, PalwWorkRootV2, PalwWorkSliceRowV2, PalwWorkSliceStageV2, palw_work_plan_range_v2,
    settle_root_partial_v2, settle_root_v2,
};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::ExecV2Refused(why.into())
}

// ---------------------------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------------------------

impl PalwChainStateV2 {
    /// The root claim's session, if one is open or retained.
    pub fn exec_v2_root_v1(&self, root_claim_id: &Hash64) -> Option<&PalwWorkRootV2> {
        self.exec_v2.roots.get(root_claim_id)
    }

    /// One accepted slice (`WorkSliceUse`).
    pub fn exec_v2_slice_v1(&self, root_claim_id: &Hash64, index: u32) -> Option<&PalwWorkSliceRowV2> {
        self.exec_v2.slices.get(&(*root_claim_id, index))
    }

    /// Which root holds a job's work identity (`JobWorkUse`).
    pub fn exec_v2_job_holder_v1(&self, job_work_id: &Hash64) -> Option<&Hash64> {
        self.exec_v2.jobs.get(job_work_id)
    }

    /// `(roots, slices, jobs)` row counts, for telemetry and the status the kit reads.
    pub fn exec_v2_counts_v1(&self) -> (usize, usize, usize) {
        (self.exec_v2.roots.len(), self.exec_v2.slices.len(), self.exec_v2.jobs.len())
    }

    /// The whole table, read-only (the RPC and the drill read it; nothing mutates through it).
    pub fn exec_v2_state_v1(&self) -> &PalwExecV2StateV1 {
        &self.exec_v2
    }

    /// **Does this root claim's session hold its `Final`?** A licensed claim whose root exists and is not ready for `Final` owes no
    /// `Final` deadline. A claim with no root, or a ready one, is unaffected — so on every chain below the fence (no root row)
    /// this is `false` and costs one map lookup on an empty map.
    pub fn exec_v2_holds_final_v1(&self, claim_id: &Hash64) -> bool {
        self.exec_v2.roots.get(claim_id).is_some_and(|root| {
            matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete) && !root.ready_for_final()
        })
    }

    /// **The exposure the live roots hold on their extra executors' bonds**, re-derived from the rows — what the consistency check
    /// adds to the claims' own commitments so `reserved_exposure` is still exactly the sum of its sources.
    pub(super) fn exec_v2_exposure_rows(&self) -> impl Iterator<Item = (PalwBondKeyV2, u128)> + '_ {
        self.exec_v2
            .roots
            .values()
            .filter(|root| matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete))
            .flat_map(|root| root.extra_executors.iter().map(move |bond| (*bond, root.executor_exposure)))
    }

    /// **The six admission rules, in the spec's order, against THIS state** (§3). Pure: it reads and returns; a refusal names the
    /// first rule that failed and nothing is written. `folded_in_block` is how many slices the accepting block has already folded
    /// (the per-block quota is state-based and deterministic under every arrival order because the covered set is canonical).
    pub fn exec_v2_admit_slice_v1(
        &self,
        params: &PalwStateParamsV2,
        daa_score: u64,
        covered: &PalwExecV2CoveredSliceV1,
        folded_in_block: usize,
    ) -> Result<PalwSliceAdmissionV2, PalwSliceRefusalV2> {
        use PalwSliceRefusalV2 as R;
        if !params.exec_v2_active_at(daa_score) {
            return Err(R::Dormant);
        }
        let slice = &covered.slice;
        // ---- 1: the root is accepted, active and anchored on this history; not complete, Final, expired or void ----
        let root = self.exec_v2.roots.get(&slice.root_claim_id).ok_or(R::NoRoot)?;
        match root.phase {
            PalwWorkRootPhaseV2::Open => {}
            PalwWorkRootPhaseV2::Complete | PalwWorkRootPhaseV2::Voided { .. } | PalwWorkRootPhaseV2::Settled { .. } => {
                return Err(R::RootNotOpen);
            }
        }
        if daa_score >= root.expiry_daa {
            return Err(R::RootExpired);
        }
        let claim_live = self.claims.get(&slice.root_claim_id).is_some_and(|claim| !claim.phase.is_terminal());
        if !claim_live {
            return Err(R::RootClaimNotLive);
        }
        // ---- 2: the bond is eligible and authorised; the carried key is its registered key ----
        // (The signature and the payload commitment were verified by the header stage that produced this covered set.)
        if !root.authorises(&slice.executor_bond) {
            return Err(R::ExecutorNotAuthorized);
        }
        let bond = self.bonds.get(&slice.executor_bond).filter(|bond| matches!(bond.status, PalwBondStatusV2::Active));
        let Some(bond) = bond else { return Err(R::ExecutorNotActive) };
        if bond.pubkey != covered.pubkey {
            return Err(R::ExecutorKeyMismatch);
        }
        // ---- 3: the index and the range are the next canonical ones, unused, overlapping nothing ----
        if slice.slice_index >= root.slice_count() {
            return Err(R::IndexPastPlan);
        }
        if slice.slice_index < root.next_index || self.exec_v2.slices.contains_key(&(slice.root_claim_id, slice.slice_index)) {
            return Err(R::IndexUsed);
        }
        if slice.slice_index > root.next_index {
            return Err(R::SkippedSlice);
        }
        let planned = palw_work_plan_range_v2(&root.boundaries, slice.slice_index).ok_or(R::IndexPastPlan)?;
        if slice.canonical_range != planned {
            let prefix = PalwWorkRangeV1 { start: 0, end: root.prefix_work() };
            let overlaps_accepted = (0..root.next_index).any(|accepted| {
                palw_work_plan_range_v2(&root.boundaries, accepted).is_some_and(|range| range.overlaps(&slice.canonical_range))
            });
            return Err(if prefix.overlaps(&slice.canonical_range) || overlaps_accepted { R::RangeOverlap } else { R::RangeNotPlan });
        }
        // ---- 4: the predecessor is the committed boundary ----
        if slice.predecessor_state_root != root.last_state_root {
            return Err(R::PredecessorMismatch);
        }
        // ---- 5: the evidence and DA commitments are set (shape) and the class, job, kernel and plan are the root's ----
        // (Availability of the committed material is the DA court's, a gate that is not wired; see the implementation record.)
        if slice.class_id != root.class_id {
            return Err(R::ClassMismatch);
        }
        if slice.canonical_job_id != root.canonical_job_id {
            return Err(R::JobMismatch);
        }
        if slice.kernel_version != root.kernel_version {
            return Err(R::KernelMismatch);
        }
        if slice.plan_root != root.plan_root {
            return Err(R::PlanMismatch);
        }
        // ---- 6: per-root, per-bond and per-block depth and quota ----
        if root.pending >= PALW_EXEC_V2_MAX_PENDING_DEPTH {
            return Err(R::RootPendingDepth);
        }
        if self.exec_v2.pending_of(&slice.executor_bond) >= PALW_EXEC_V2_MAX_PENDING_PER_BOND {
            return Err(R::BondPendingDepth);
        }
        if folded_in_block >= PALW_EXEC_V2_MAX_SLICES_PER_BLOCK {
            return Err(R::BlockQuota);
        }
        let work = planned.work().ok_or(R::Overflow)?;
        Ok(PalwSliceAdmissionV2 { work })
    }

    /// **The consistency of the work-slice ledgers** against each other and against the claims, checked with the rest of
    /// `assert_internal_consistency`. Empty tables are consistent by construction.
    pub(super) fn assert_exec_v2_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        let bad = |why: String| PalwStateV2Error::CarriageInconsistent(format!("exec v2: {why}"));
        if self.exec_v2.is_empty() {
            return Ok(());
        }
        for (claim_id, root) in &self.exec_v2.roots {
            if !self.claims.contains_key(claim_id) {
                return Err(bad(format!("root {claim_id} outlives its claim")));
            }
            if self.exec_v2.jobs.get(&root.job_work_id) != Some(claim_id) {
                return Err(bad(format!("root {claim_id} does not hold its job work identity")));
            }
            crate::palw_work_slice_v2::palw_work_plan_validate_v2(&root.boundaries, root.total_work)
                .map_err(|e| bad(format!("root {claim_id}'s plan: {e}")))?;
            if root.next_index > root.slice_count() {
                return Err(bad(format!("root {claim_id} accepted past its plan")));
            }
            let rows: Vec<(&(Hash64, u32), &PalwWorkSliceRowV2)> =
                self.exec_v2.slices.range((*claim_id, 0)..=(*claim_id, u32::MAX)).collect();
            if rows.len() as u32 != root.next_index {
                return Err(bad(format!("root {claim_id} counts {} slices but holds {} rows", root.next_index, rows.len())));
            }
            let mut accepted = 0u64;
            let mut verified = 0u64;
            let mut pending = 0u32;
            for (position, ((_, index), row)) in rows.iter().enumerate() {
                if *index as usize != position {
                    return Err(bad(format!("root {claim_id}'s slice rows skip an index")));
                }
                if palw_work_plan_range_v2(&root.boundaries, *index) != Some(row.range) {
                    return Err(bad(format!("root {claim_id}'s slice {index} is not the plan's range")));
                }
                let work = row.range.work().ok_or_else(|| bad("an empty slice range".into()))?;
                match row.stage {
                    PalwWorkSliceStageV2::Pending => {
                        pending += 1;
                        accepted = accepted.checked_add(work).ok_or(PalwStateV2Error::Overflow("exec v2 accepted work"))?;
                    }
                    PalwWorkSliceStageV2::Verified { .. } => {
                        verified = verified.checked_add(work).ok_or(PalwStateV2Error::Overflow("exec v2 verified work"))?;
                        accepted = accepted.checked_add(work).ok_or(PalwStateV2Error::Overflow("exec v2 accepted work"))?;
                    }
                    PalwWorkSliceStageV2::Voided { .. } => {}
                }
            }
            let voided = matches!(root.phase, PalwWorkRootPhaseV2::Voided { .. });
            if !voided {
                if root.accepted_work != accepted || root.verified_work != verified || root.pending != pending {
                    return Err(bad(format!("root {claim_id}'s work counters differ from its slice rows")));
                }
                if root.prefix_work().checked_add(root.accepted_work).is_none_or(|sum| sum > root.total_work) {
                    return Err(bad(format!("root {claim_id} credits more than its total work")));
                }
                let complete = root.next_index == root.slice_count();
                match root.phase {
                    PalwWorkRootPhaseV2::Open if complete => return Err(bad(format!("root {claim_id} is complete but open"))),
                    PalwWorkRootPhaseV2::Complete | PalwWorkRootPhaseV2::Settled { .. } if !complete => {
                        return Err(bad(format!("root {claim_id} is {:?} but not complete", root.phase)));
                    }
                    _ => {}
                }
                if let Some((_, last)) = rows.last() {
                    if last.result_state_root != root.last_state_root {
                        return Err(bad(format!("root {claim_id}'s boundary is not its last slice's result")));
                    }
                } else if root.last_state_root != root.initial_state_root {
                    return Err(bad(format!("root {claim_id}'s boundary moved with no slice")));
                }
            }
        }
        for (key, row) in &self.exec_v2.slices {
            if !self.exec_v2.roots.contains_key(&key.0) {
                return Err(bad(format!("slice {}/{} has no root", key.0, key.1)));
            }
            let _ = row;
        }
        for (job, holder) in &self.exec_v2.jobs {
            if self.exec_v2.roots.get(holder).map(|root| root.job_work_id) != Some(*job) {
                return Err(bad(format!("job {job} names a root that does not hold it")));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Writers: the one journaled door to the three tables
// ---------------------------------------------------------------------------------------------

impl TransitionBuilder<'_> {
    fn write_exec_v2_row<K, V>(
        &mut self,
        table: u8,
        key: K,
        new: Option<V>,
        map: fn(&mut PalwChainStateV2) -> &mut BTreeMap<K, V>,
    ) -> Option<V>
    where
        K: Ord + Clone + borsh::BorshSerialize,
        V: Clone + PartialEq + borsh::BorshSerialize,
    {
        let rows = map(&mut self.state);
        let old = match new.clone() {
            Some(row) => rows.insert(key.clone(), row),
            None => rows.remove(&key),
        };
        if old != new {
            let encode = |row: &V| borsh::to_vec(row).expect("an exec v2 row is borsh-serializable");
            self.entries.push(PalwDeltaEntryV2::ExecV2Row {
                table,
                key: borsh::to_vec(&key).expect("an exec v2 key is borsh-serializable"),
                old: old.as_ref().map(encode),
                new: new.as_ref().map(encode),
            });
        }
        old
    }

    /// The one writer of `RootWorkBudget` rows.
    pub(super) fn write_exec_root(&mut self, key: Hash64, new: Option<PalwWorkRootV2>) {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_ROOTS_V1, key, new, |s| &mut s.exec_v2.roots);
    }

    /// The one writer of `WorkSliceUse` rows.
    pub(super) fn write_exec_slice(&mut self, key: (Hash64, u32), new: Option<PalwWorkSliceRowV2>) {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_SLICES_V1, key, new, |s| &mut s.exec_v2.slices);
    }

    /// The one writer of `JobWorkUse` rows.
    pub(super) fn write_exec_job(&mut self, key: Hash64, new: Option<Hash64>) {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_JOBS_V1, key, new, |s| &mut s.exec_v2.jobs);
    }
}

// ---------------------------------------------------------------------------------------------
// Delta application (apply and revert)
// ---------------------------------------------------------------------------------------------

/// One [`PalwDeltaEntryV2::ExecV2Row`] applied or reverted: decode the key and the rows as the table's types, check the expected
/// row, install the other. A mismatch is `DeltaMismatch`, never a silent overwrite.
pub(super) fn apply_exec_v2_row_v1(
    state: &mut PalwChainStateV2,
    table: u8,
    key: &[u8],
    old: &Option<Vec<u8>>,
    new: &Option<Vec<u8>>,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    fn swap<K, V>(
        map: &mut BTreeMap<K, V>,
        key: &[u8],
        old: &Option<Vec<u8>>,
        new: &Option<Vec<u8>>,
        revert: bool,
    ) -> Result<(), PalwStateV2Error>
    where
        K: Ord + borsh::BorshDeserialize,
        V: PartialEq + borsh::BorshDeserialize,
    {
        let bad = |_| PalwStateV2Error::DeltaMismatch("an exec v2 row's bytes do not decode");
        let key: K = borsh::from_slice(key).map_err(bad)?;
        let (expected, install) = if revert { (new, old) } else { (old, new) };
        let expected: Option<V> = expected.as_deref().map(borsh::from_slice).transpose().map_err(bad)?;
        if map.get(&key) != expected.as_ref() {
            return Err(PalwStateV2Error::DeltaMismatch("an exec v2 row does not match the delta's expectation"));
        }
        match install.as_deref() {
            Some(bytes) => {
                map.insert(key, borsh::from_slice(bytes).map_err(bad)?);
            }
            None => {
                map.remove(&key);
            }
        }
        Ok(())
    }
    match table {
        PALW_EXEC_V2_TABLE_ROOTS_V1 => swap(&mut state.exec_v2.roots, key, old, new, revert),
        PALW_EXEC_V2_TABLE_SLICES_V1 => swap(&mut state.exec_v2.slices, key, old, new, revert),
        PALW_EXEC_V2_TABLE_JOBS_V1 => swap(&mut state.exec_v2.jobs, key, old, new, revert),
        _ => Err(PalwStateV2Error::DeltaMismatch("an exec v2 row names no table")),
    }
}

// ---------------------------------------------------------------------------------------------
// The root declaration
// ---------------------------------------------------------------------------------------------

/// **`ExecWorkRootOpenedV2`** (object tag 130): open a work session on an accepted REAL claim. Every refusal comes before the first
/// write. The declaration's *signature* is the acceptance layer's (as every object's is); the fold checks what only state can: the
/// claim, its phase, its prefix, the job's one-use history, the quotas and the executors' funded room.
pub(super) fn apply_root_declared_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    declaration: &PalwWorkRootDeclarationV2,
) -> Result<(), PalwStateV2Error> {
    use PalwWorkRootRefusalV2 as R;
    let no = |why: R| refused(why.to_string());
    let daa = ctx.daa_score;
    if !builder.params.exec_v2_active_at(daa) {
        return Err(no(R::Dormant));
    }
    declaration.validate_shape().map_err(no)?;
    let claim_id = declaration.root_claim_id;
    let claim = builder.state.claims.get(&claim_id).ok_or_else(|| no(R::NoClaim))?.clone();
    // The REAL claim must be a live attempt that no verdict has yet licensed: the plan is fixed before the panel's receipts.
    if !matches!(claim.source, PalwClaimSourceV2::Attempt)
        || !matches!(claim.phase, PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. })
    {
        return Err(no(R::ClaimNotOpenable));
    }
    if builder.state.exec_v2.roots.contains_key(&claim_id) {
        return Err(no(R::RootExists));
    }
    // The job is the claim's own: a declaration cannot name another job than the execution this claim answered.
    if claim.job_identity == Hash64::default() || claim.job_identity != declaration.canonical_job_id {
        return Err(no(R::ClaimBindingMismatch));
    }
    // The prefix is the work the chain admitted for the claim, derived and never declared.
    let admitted = builder
        .state
        .palw_claim_canonical_weight_v1(&claim, builder.extras.canonical_work_daa)
        .ok_or_else(|| no(R::ClaimWorkNotDerivable))?;
    let admitted = u64::try_from(admitted).map_err(|_| no(R::Overflow))?;
    if declaration.boundaries.first().copied() != Some(admitted) {
        return Err(no(R::PrefixMismatch { declared: declaration.boundaries.first().copied().unwrap_or(0), admitted }));
    }
    if declaration.expiry_daa <= daa || declaration.expiry_daa > daa.saturating_add(PALW_EXEC_V2_MAX_ROOT_LIFETIME_DAA) {
        return Err(no(R::BadExpiry));
    }
    let job_work_id = declaration.job_work_id(&claim.class_id);
    if builder.state.exec_v2.jobs.contains_key(&job_work_id) {
        return Err(no(R::JobWorkAlreadyUsed));
    }
    // An executor stands behind the root exactly as its producer does: the claim's own reservation.
    let exposure = claim.reserved;
    if exposure == 0 {
        return Err(no(R::ClaimUnreserved));
    }
    if builder.state.exec_v2.roots.len() >= PALW_EXEC_V2_MAX_OPEN_ROOTS
        || builder.state.exec_v2.open_roots_of(&claim.bond) >= PALW_EXEC_V2_MAX_OPEN_ROOTS_PER_BOND
    {
        return Err(no(R::RootQuota));
    }
    // Extra executors: distinct from the root bond, active, and with room for the claim's reservation on their own collateral.
    for bond in &declaration.extra_executors {
        if *bond == claim.bond {
            return Err(no(R::ExecutorsNotCanonical));
        }
        let active = builder.state.bonds.get(bond).is_some_and(|record| matches!(record.status, PalwBondStatusV2::Active));
        if !active || builder.gate_room(bond, daa, PalwRcoreGateV1::Work) < exposure {
            return Err(no(R::ExecutorUnfunded));
        }
    }
    // ---- writes ----
    for bond in &declaration.extra_executors {
        let held = builder.state.reserved_exposure.get(bond).copied().unwrap_or(0);
        let next = held.checked_add(exposure).ok_or(PalwStateV2Error::Overflow("exec v2 executor exposure"))?;
        builder.write_exposure(*bond, Some(next));
    }
    builder.write_exec_job(job_work_id, Some(claim_id));
    builder.write_exec_root(
        claim_id,
        Some(PalwWorkRootV2 {
            class_id: claim.class_id,
            canonical_job_id: declaration.canonical_job_id,
            job_work_id,
            kernel_version: declaration.kernel_version,
            plan_root: declaration.plan_root,
            total_work: declaration.total_work,
            boundaries: declaration.boundaries.clone(),
            initial_state_root: declaration.initial_state_root,
            evidence_policy_root: declaration.evidence_policy_root,
            root_bond: claim.bond,
            extra_executors: declaration.extra_executors.clone(),
            executor_exposure: exposure,
            anchor: claim.accepted_block,
            opened_daa: daa,
            expiry_daa: declaration.expiry_daa,
            next_index: 0,
            last_state_root: declaration.initial_state_root,
            accepted_work: 0,
            verified_work: 0,
            pending: 0,
            phase: PalwWorkRootPhaseV2::Open,
        }),
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Slice acceptance
// ---------------------------------------------------------------------------------------------

/// **Fold the accepting block's covered slices**, in the order given (the processor's canonical order: root, then index, then
/// carrier). Each is judged by [`PalwChainStateV2::exec_v2_admit_slice_v1`] against the running state; an admitted slice writes its
/// row and moves its root, a refused one writes nothing and is reported. Returns each carrier's verdict, in order, for telemetry —
/// a refusal is a lane verdict about a carrier, never an error of the block.
pub(super) fn apply_covered_slices_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    covered: &[PalwExecV2CoveredSliceV1],
) -> Vec<(BlockHash, Result<(), PalwSliceRefusalV2>)> {
    let mut verdicts = Vec::with_capacity(covered.len());
    let mut folded = 0usize;
    for carrier in covered {
        match builder.state.exec_v2_admit_slice_v1(builder.params, ctx.daa_score, carrier, folded) {
            Ok(admission) => {
                accept_slice_v2(builder, ctx, carrier, admission);
                folded += 1;
                verdicts.push((carrier.carrier, Ok(())));
            }
            Err(refusal) => verdicts.push((carrier.carrier, Err(refusal))),
        }
    }
    verdicts
}

/// The writes of one admitted slice (checked arithmetic cannot fail here: the admission derived `work` from the plan and the root's
/// counters are bounded by the plan, which `palw_work_plan_validate_v2` bounds below `u64::MAX`).
fn accept_slice_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    carrier: &PalwExecV2CoveredSliceV1,
    admission: PalwSliceAdmissionV2,
) {
    let slice = &carrier.slice;
    let mut root = builder.state.exec_v2.roots.get(&slice.root_claim_id).expect("admission found the root").clone();
    builder.write_exec_slice(
        (slice.root_claim_id, slice.slice_index),
        Some(PalwWorkSliceRowV2 {
            slice_id: slice.slice_id(),
            range: slice.canonical_range,
            executor: slice.executor_bond,
            carrier: carrier.carrier,
            accepted_daa: ctx.daa_score,
            predecessor_state_root: slice.predecessor_state_root,
            result_state_root: slice.result_state_root,
            evidence_root: slice.evidence_root,
            da_root: slice.da_root,
            stage: PalwWorkSliceStageV2::Pending,
        }),
    );
    root.next_index += 1;
    root.last_state_root = slice.result_state_root;
    root.accepted_work = root.accepted_work.saturating_add(admission.work);
    root.pending += 1;
    if root.next_index == root.slice_count() {
        root.phase = PalwWorkRootPhaseV2::Complete;
    }
    builder.write_exec_root(slice.root_claim_id, Some(root));
}

// ---------------------------------------------------------------------------------------------
// Settlement at Final, void with the claim, retirement with the claim, expiry
// ---------------------------------------------------------------------------------------------

/// **The root's one settlement, at the claim's `Final`.** `amount` is the root executor's reward leg (the producer's, after the
/// panel's share); returns what the root executor keeps and the other executors' legs. A claim without a root keeps all of
/// `amount`. A root that is `Settled` already pays nothing a second time. A root that is not ready — unreachable, the claim holds
/// its `Final` — pays only the prefix and the verified slices (the rest is never named), never wedging the chain on a broken
/// invariant. On a ready root the root becomes `Settled` in the same journaled step, so a restart or a reorg that replays the
/// block finds the marker and the payments together.
pub(super) fn settle_at_final_v2(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    amount: u64,
    final_daa: u64,
) -> Result<(u64, Vec<(PalwBondKeyV2, u64)>), PalwStateV2Error> {
    let Some(root) = builder.state.exec_v2.roots.get(claim_id).cloned() else { return Ok((amount, Vec::new())) };
    match root.phase {
        PalwWorkRootPhaseV2::Settled { .. } => return Ok((0, Vec::new())),
        PalwWorkRootPhaseV2::Voided { .. } => {
            // A voided root never settles slices; the claim cannot reach Final through a void, so this is unreachable. Pay the prefix.
            let prefix = settle_root_partial_v2(amount, root.total_work, root.prefix_work(), &[], root.root_bond)
                .ok_or(PalwStateV2Error::Overflow("exec v2 prefix settlement"))?;
            return Ok((prefix.root_leg, Vec::new()));
        }
        PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete => {}
    }
    let rows: Vec<&PalwWorkSliceRowV2> =
        builder.state.exec_v2.slices.range((*claim_id, 0)..=(*claim_id, u32::MAX)).map(|(_, row)| row).collect();
    let work_of = |row: &PalwWorkSliceRowV2| row.range.work().ok_or(PalwStateV2Error::Overflow("exec v2 slice work"));
    let (settlement, settled) = if root.ready_for_final() {
        let credited: Vec<(PalwBondKeyV2, u64)> =
            rows.iter().map(|row| work_of(row).map(|work| (row.executor, work))).collect::<Result<_, _>>()?;
        (settle_root_v2(amount, root.total_work, root.prefix_work(), &credited, root.root_bond), true)
    } else {
        let verified: Vec<(PalwBondKeyV2, u64)> = rows
            .iter()
            .filter(|row| matches!(row.stage, PalwWorkSliceStageV2::Verified { .. }))
            .map(|row| work_of(row).map(|work| (row.executor, work)))
            .collect::<Result<_, _>>()?;
        (settle_root_partial_v2(amount, root.total_work, root.prefix_work(), &verified, root.root_bond), false)
    };
    let settlement = settlement.ok_or(PalwStateV2Error::Overflow("exec v2 settlement"))?;
    if !settled {
        // Unreachable by construction; recorded loudly in debug builds and degraded safely in release.
        debug_assert!(false, "a root reached Final without being ready");
    }
    // Release the extra executors' exposure and mark the root settled: the marker and the payments are one journaled step.
    release_executor_exposure_v2(builder, &root)?;
    let mut done = root;
    done.phase = PalwWorkRootPhaseV2::Settled { settled_daa: final_daa };
    builder.write_exec_root(*claim_id, Some(done));
    Ok((settlement.root_leg, settlement.slice_legs))
}

/// Return the exposure a root held on each extra executor's bond (exactly what the open took).
fn release_executor_exposure_v2(builder: &mut TransitionBuilder<'_>, root: &PalwWorkRootV2) -> Result<(), PalwStateV2Error> {
    for bond in &root.extra_executors {
        let held = builder.state.reserved_exposure.get(bond).copied().unwrap_or(0);
        let next =
            held.checked_sub(root.executor_exposure).ok_or(PalwStateV2Error::Overflow("exec v2 executor exposure underflow"))?;
        builder.write_exposure(*bond, if next == 0 { None } else { Some(next) });
    }
    Ok(())
}

/// **The claim was voided** (by any route): the root and its pending slices go void with it, the extra executors' exposure
/// returns, nobody is paid and nobody is charged here — a slashing is a conviction's, proven where it is proven. The rows stay
/// (the job's one-use tombstone with them) until the claim retires. A no-op for a claim with no root or a root already terminal.
pub(super) fn on_claim_voided_v2(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    voided_daa: u64,
) -> Result<(), PalwStateV2Error> {
    let Some(root) = builder.state.exec_v2.roots.get(claim_id).cloned() else { return Ok(()) };
    if !matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete) {
        return Ok(());
    }
    let from_index = builder
        .state
        .exec_v2
        .slices
        .range((*claim_id, 0)..=(*claim_id, u32::MAX))
        .find(|(_, row)| !matches!(row.stage, PalwWorkSliceStageV2::Verified { .. }))
        .map(|((_, index), _)| *index)
        .unwrap_or(root.next_index);
    let pending: Vec<((Hash64, u32), PalwWorkSliceRowV2)> = builder
        .state
        .exec_v2
        .slices
        .range((*claim_id, 0)..=(*claim_id, u32::MAX))
        .filter(|(_, row)| matches!(row.stage, PalwWorkSliceStageV2::Pending))
        .map(|(key, row)| (*key, row.clone()))
        .collect();
    for (key, mut row) in pending {
        row.stage = PalwWorkSliceStageV2::Voided { voided_daa };
        builder.write_exec_slice(key, Some(row));
    }
    release_executor_exposure_v2(builder, &root)?;
    let mut voided = root;
    voided.phase = PalwWorkRootPhaseV2::Voided { from_index, voided_daa };
    builder.write_exec_root(*claim_id, Some(voided));
    Ok(())
}

/// **The claim retired**: its root, its slice rows and its job's one-use tombstone leave with it — the ledgers live exactly as long
/// as the claim they hang from, so they are bounded by the claim table's own retirement. (Replay after retirement is closed by the
/// job identity being the REAL claim's own execution anchor, which a new claim cannot reproduce; recorded as a gate in the
/// implementation record.) A no-op for a claim with no root.
pub(super) fn on_claim_retired_v2(builder: &mut TransitionBuilder<'_>, claim_id: &Hash64) {
    let Some(root) = builder.state.exec_v2.roots.get(claim_id).cloned() else { return };
    let keys: Vec<(Hash64, u32)> =
        builder.state.exec_v2.slices.range((*claim_id, 0)..=(*claim_id, u32::MAX)).map(|(key, _)| *key).collect();
    for key in keys {
        builder.write_exec_slice(key, None);
    }
    builder.write_exec_job(root.job_work_id, None);
    builder.write_exec_root(*claim_id, None);
}

/// **Roots past their expiry that are not ready void their claims** (uncharged: a timeout is not a conviction). Run at the start
/// of every block's fold, before its objects. A root whose claim is already terminal was handled by the void hook; a ready root is
/// finalizing and is left alone. Bounded by [`PALW_EXEC_V2_MAX_OPEN_ROOTS`].
pub(super) fn sweep_expired_roots_v2(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if builder.state.exec_v2.roots.is_empty() {
        return Ok(());
    }
    let expired: Vec<Hash64> = builder
        .state
        .exec_v2
        .roots
        .iter()
        .filter(|(_, root)| {
            matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete)
                && ctx.daa_score >= root.expiry_daa
                && !root.ready_for_final()
        })
        .map(|(id, _)| *id)
        .collect();
    for claim_id in expired {
        let Some(claim) = builder.state.claims.get(&claim_id).cloned() else { continue };
        if claim.phase.is_terminal() {
            continue;
        }
        builder.void_claim(claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::WorkRootExpired)?;
    }
    Ok(())
}

/// A ready root releases its claim's `Final`: re-derive the claim's deadline now that the hold is gone.
pub(super) fn release_final_hold_v2(
    builder: &mut TransitionBuilder<'_>,
    claim_id: Hash64,
    now_daa: u64,
) -> Result<(), PalwStateV2Error> {
    if builder.state.exec_v2.roots.get(&claim_id).is_some_and(|root| root.ready_for_final()) {
        builder.rearm_claim_deadline_dl1_v1(claim_id, now_daa)?;
    }
    Ok(())
}

/// **TEST ONLY — the door a verification route will one day own.** Marks slice `(claim_id, index)` positively verified and, if that
/// completes the root, releases the claim's `Final` hold. There is no production caller: the `WORK_SLICE` challenge, public-bond
/// prosecution and the DA court are the gates (spec section 9) that must supply the evidence, and a harness-signed receipt is not
/// a substitute. Compiled out of every build that is not a test.
#[cfg(test)]
pub(crate) fn mark_slice_verified_for_tests(
    builder: &mut TransitionBuilder<'_>,
    claim_id: Hash64,
    index: u32,
    verified_daa: u64,
) -> Result<(), PalwStateV2Error> {
    let mut row = builder.state.exec_v2.slices.get(&(claim_id, index)).cloned().ok_or_else(|| refused("no such slice"))?;
    if !matches!(row.stage, PalwWorkSliceStageV2::Pending) {
        return Err(refused("slice is not pending"));
    }
    let work = row.range.work().ok_or_else(|| refused("empty range"))?;
    row.stage = PalwWorkSliceStageV2::Verified { verified_daa };
    builder.write_exec_slice((claim_id, index), Some(row));
    let mut root = builder.state.exec_v2.roots.get(&claim_id).cloned().ok_or_else(|| refused("no such root"))?;
    root.pending = root.pending.checked_sub(1).ok_or(PalwStateV2Error::Overflow("exec v2 pending"))?;
    root.verified_work = root.verified_work.checked_add(work).ok_or(PalwStateV2Error::Overflow("exec v2 verified"))?;
    builder.write_exec_root(claim_id, Some(root));
    release_final_hold_v2(builder, claim_id, verified_daa)
}
