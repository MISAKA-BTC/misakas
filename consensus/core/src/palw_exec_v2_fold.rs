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
//! * **It verifies only through the kernel route** (amendment 1, spec §10.1; [`sync_slice_verification_v2`]): a slice names a
//!   kernel-route claim that binds it, and its stage follows that claim every block after the route's closing tick — `Final`
//!   plus selected verifier approval verifies it (fixing its leg cap), a conviction proves it false, a default or timeout defaults it, and either failure voids the
//!   suffix and the root ([`void_suffix_v2`]). The test-only writer [`mark_slice_verified_for_tests`] remains for the arithmetic
//!   tests of chains with no kernel route; it is `#[cfg(test)]` and reachable by no node.

use super::*;
use crate::palw_exec_v2::PalwWorkRangeV1;
use crate::palw_exec_v2_verify::{
    PalwExecV2PrefixInputsV1, PalwSliceVerificationV1, palw_exec_v2_claim_outcome_v1, palw_exec_v2_kernel_key_v1,
    palw_exec_v2_leg_cap_v1, palw_exec_v2_prefix_binding_v1, palw_exec_v2_real_job_v1, palw_exec_v2_retention_terms_v1,
    palw_exec_v2_root_slice_job_nonce_v2, palw_exec_v2_route_reward_v1, palw_exec_v2_selected_approval_v1,
    palw_exec_v2_verification_binding_v2,
};
use crate::palw_work_slice_v2::{
    PALW_EXEC_V2_MAX_EXTRA_EXECUTORS, PALW_EXEC_V2_MAX_OPEN_ROOTS, PALW_EXEC_V2_MAX_OPEN_ROOTS_PER_BOND,
    PALW_EXEC_V2_MAX_PENDING_DEPTH, PALW_EXEC_V2_MAX_PENDING_PER_BOND, PALW_EXEC_V2_MAX_ROOT_LIFETIME_DAA,
    PALW_EXEC_V2_MAX_SLICES_PER_BLOCK, PalwExecV2CoveredSliceV1, PalwExecV2StateV1, PalwSliceAdmissionV2, PalwSliceRefusalV2,
    PalwWorkPrefixStageV2, PalwWorkRootDeclarationV2, PalwWorkRootPhaseV2, PalwWorkRootRefusalV2, PalwWorkRootV2, PalwWorkSliceRowV2,
    PalwWorkSliceStageV2, palw_work_plan_range_v2, settle_root_v2,
};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::ExecV2Refused(why.into())
}

// ---------------------------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------------------------

impl PalwChainStateV2 {
    /// Cost policy is frozen by the immutable class's registration rules, not the slice's height.
    fn exec_v2_work_inputs_v1(
        &self,
        params: &PalwStateParamsV2,
        class: &Hash64,
        job: &Hash64,
    ) -> Result<(misaka_palw_tir::TirProgramV1, crate::palw_exec_work_plan_v1::PalwExecWorkCostV1, u32, u32, u32), String> {
        let tir = self.tir_class_v1(class).ok_or("the REAL job cannot be derived from registered IR")?;
        let registered = self.classes.get(class).ok_or("the registered class is missing")?;
        let program = misaka_palw_tir::TirProgramV1::decode_canonical(&tir.program).map_err(|e| e.to_string())?;
        let canonical = crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_of_v1(tir.facts.max_context)
            .ok_or("the REAL job cannot be derived")?;
        let context = crate::palw_tir_attempt_v1::palw_tir_attempt_context_v1(&tir.facts, job, canonical, Hash64::default());
        let cost = if params.tir_fence2_active_at(registered.registered_daa) {
            crate::palw_exec_work_plan_v1::PalwExecWorkCostV1::MinSelectArmsV2
        } else {
            crate::palw_exec_work_plan_v1::PalwExecWorkCostV1::AllNodesV1
        };
        Ok((program, cost, tir.facts.max_context, canonical.0, context.exact_decode_tokens))
    }

    /// The declaration's authenticated token plan, reconstructed entirely from chain data.
    pub fn exec_v2_canonical_work_plan_v1(
        &self,
        params: &PalwStateParamsV2,
        class: &Hash64,
        job: &Hash64,
        boundaries: &[u64],
    ) -> Result<crate::palw_exec_work_plan_v1::PalwExecWorkPlanV1, String> {
        let (program, cost, max, prompt, prefix) = self.exec_v2_work_inputs_v1(params, class, job)?;
        crate::palw_exec_work_plan_v1::palw_exec_work_plan_from_boundaries_v1(&program, cost, max, prompt, prefix, boundaries)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn exec_v2_check_kernel_work_v1(
        &self,
        params: &PalwStateParamsV2,
        root: &PalwWorkRootV2,
        index: u32,
        row: &misaka_palw_kernel::ledger::ClaimRowV1,
        job: &misaka_palw_kernel::job::KernelJobV1,
    ) -> Result<(), String> {
        let (program, cost, max, prompt, prefix) = self.exec_v2_work_inputs_v1(params, &root.class_id, &root.canonical_job_id)?;
        let misaka_palw_kernel::ledger::ClaimBodyV1::Program { claim, .. } = &row.body else {
            return Err("not a program claim".into());
        };
        if job.max_new_tokens as usize != claim.generated.len() {
            return Err("the maximum and actual decode differ".into());
        }
        let range = palw_work_plan_range_v2(&root.boundaries, index).ok_or("no such execution cell")?;
        crate::palw_exec_work_plan_v1::palw_exec_work_slice_scope_v1(
            &program,
            cost,
            max,
            prompt,
            prefix,
            [range.start, range.end],
            job.prompt.len(),
            claim.generated.len(),
        )
        .map_err(|e| e.to_string())
    }

    /// **The EXEC root a kernel claim is bound to, if any** (X8R round 3; ADR-0176 D2's hook H-3a for lane BUDGET): a root's prefix claim
    /// or an accepted slice's verification claim. BUDGET's engine reads it at a kernel claim's `Final` to draw the route reward from
    /// the ROOT's one reservation instead of a fresh one on the executor's bond. Bounded by the open-root and per-root slice bounds.
    pub fn exec_v2_root_of_kernel_claim_v1(&self, kernel_claim: &Hash64) -> Option<Hash64> {
        self.exec_v2
            .roots
            .iter()
            .find(|(_, root)| root.prefix != PalwWorkPrefixStageV2::Unbound && root.prefix_claim == *kernel_claim)
            .map(|(id, _)| *id)
            .or_else(|| self.exec_v2.slices.iter().find(|(_, row)| row.evidence_root == *kernel_claim).map(|((root, _), _)| *root))
    }

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
            matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete)
                && (!root.ready_for_final() || !self.exec_v2_selected_approvals_v1(claim_id, root))
        })
    }

    fn exec_v2_selected_approvals_v1(&self, claim_id: &Hash64, root: &PalwWorkRootV2) -> bool {
        let Some(route) = self.kernel_route.as_ref() else { return true };
        palw_exec_v2_selected_approval_v1(route, &root.prefix_claim)
            && self
                .exec_v2
                .slices
                .range((*claim_id, 0)..=(*claim_id, u32::MAX))
                .all(|(_, slice)| palw_exec_v2_selected_approval_v1(route, &slice.evidence_root))
    }

    /// An allow-list is not the listed workers' consent. An extra executor becomes
    /// a participant only through an accepted slice carrying its own signature.
    fn exec_v2_participants_v1(&self, id: &Hash64, root: &PalwWorkRootV2) -> BTreeSet<PalwBondKeyV2> {
        self.exec_v2
            .slices
            .range((*id, 0)..=(*id, u32::MAX))
            .map(|(_, row)| row.executor)
            .filter(|bond| root.extra_executors.binary_search(bond).is_ok())
            .collect()
    }

    /// Exactly one reservation per consenting participant of each live root.
    /// Slice rows are also the authoritative consent record on replay/restoration.
    pub(super) fn exec_v2_exposure_rows(&self) -> impl Iterator<Item = (PalwBondKeyV2, u128)> + '_ {
        self.exec_v2
            .roots
            .iter()
            .filter(|(_, root)| matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete))
            .flat_map(move |(id, root)| {
                self.exec_v2_participants_v1(id, root).into_iter().map(move |bond| (bond, root.executor_exposure))
            })
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
        // With no receipt of the required second-clock depth, use its conservative
        // upper bound. The canonical fold always supplies its actual depth/None.
        self.exec_v2_admit_slice_at_depth_v2(params, daa_score, covered, folded_in_block, Some(u64::MAX))
    }

    /// Canonical fold admission uses its own raw second-clock depth, matching the
    /// other Work gates. Public preflight may pass the authenticated tip's depth.
    pub fn exec_v2_admit_slice_at_depth_v2(
        &self,
        params: &PalwStateParamsV2,
        daa_score: u64,
        covered: &PalwExecV2CoveredSliceV1,
        folded_in_block: usize,
        raw_depth: Option<u64>,
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
        // ---- 5 (amendment 1, spec §10.1): the verification binding — where the kernel route is in force (every armable ruleset: the
        //      route's fence is a prerequisite of this one), the kernel claim the slice names is a claim of the root class's kernel
        //      class, by this executor, of this slice's job, over this slice's token states and evidence, and has not failed ----
        let verified_at = match self.kernel_route.as_ref() {
            Some(kernel) => {
                let binding = kernel.kernel_binding_v1(&root.class_id).ok_or(R::NotKernelBound)?;
                let row = kernel
                    .rows
                    .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&slice.evidence_root)))
                    .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(bytes).ok())
                    .ok_or(R::VerificationClaimMissing)?;
                if row.rewarded {
                    return Err(R::VerificationAlreadyPaid);
                }
                if row.reserved == 0
                    || (matches!(row.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
                        && row.liability_until.is_none_or(|until| daa_score > until))
                {
                    return Err(R::VerificationClaimFailed);
                }
                if let Some(bytes) = kernel
                    .rows
                    .get(&(misaka_palw_kernel::rows::TABLE_EXEC_REWARD_HOLDS_V1, palw_exec_v2_kernel_key_v1(&slice.evidence_root)))
                {
                    let hold: misaka_palw_kernel::exec_reward::ExecRewardHoldV1 =
                        borsh::from_slice(bytes).map_err(|_| R::VerificationRewardBound)?;
                    if hold.owner_root != slice.root_claim_id.as_bytes() {
                        return Err(R::VerificationRewardBound);
                    }
                }
                let job = kernel
                    .rows
                    .get(&(misaka_palw_kernel::rows::TABLE_JOBS_V1, borsh::to_vec(&row.job_id).expect("a digest serializes")))
                    .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::job::KernelJobV1>(bytes).ok());
                let terms = palw_exec_v2_retention_terms_v1(
                    root.expiry_daa,
                    root.executor_exposure,
                    params.window_court(),
                    &kernel.header.policy,
                )
                .map_err(|_| R::Overflow)?;
                palw_exec_v2_verification_binding_v2(
                    slice,
                    &binding,
                    &row,
                    job.as_ref(),
                    &crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&slice.executor_bond),
                    daa_score,
                    &terms,
                )?;
                self.exec_v2_check_kernel_work_v1(
                    params,
                    root,
                    slice.slice_index,
                    &row,
                    job.as_ref().ok_or(R::ExecutionScopeMismatch)?,
                )
                .map_err(|_| R::ExecutionScopeMismatch)?;
                super::palw_kernel_route_fold_v1::require_exec_provider_horizon_v1(
                    self,
                    &slice.evidence_root,
                    &row,
                    daa_score,
                    terms.liability_until,
                )
                .map_err(|_| R::VerificationProviderHorizon)?;
                match dependency_outcome(self, &slice.evidence_root, &row, daa_score) {
                    PalwSliceVerificationV1::Verified { final_daa } => Some((
                        final_daa,
                        palw_exec_v2_leg_cap_v1(&row, kernel.header.policy.claim_reward),
                        palw_exec_v2_route_reward_v1(&row, kernel.header.policy.claim_reward),
                    )),
                    _ => None,
                }
            }
            None => None,
        };
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
        if slice.executor_bond != root.root_bond
            && !self.exec_v2_participants_v1(&slice.root_claim_id, root).contains(&slice.executor_bond)
            && palw_rcore_gate_room_v1(self, params, &slice.executor_bond, daa_score, raw_depth, PalwRcoreGateV1::Work)
                < root.executor_exposure
        {
            return Err(R::ExecutorUnfunded);
        }
        let work = planned.work().ok_or(R::Overflow)?;
        Ok(PalwSliceAdmissionV2 {
            work,
            verified_at: verified_at.map(|(final_daa, _, _)| final_daa),
            leg_cap: verified_at.map(|(_, cap, _)| cap).unwrap_or(0),
            route_reward: verified_at.map(|(_, _, reward)| reward).unwrap_or(0),
        })
    }

    /// **The consistency of the work-slice ledgers** against each other and against the claims, checked with the rest of
    /// `assert_internal_consistency`. Empty tables are consistent by construction.
    pub(super) fn assert_exec_v2_consistency_v1(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        self.assert_exec_origins()?;
        super::palw_exec_v2_rewards_v1::consistent(self, params)?;
        let bad = |why: String| PalwStateV2Error::CarriageInconsistent(format!("exec v2: {why}"));
        if let Some(kernel) = &self.kernel_route {
            use misaka_palw_kernel::{exec_reward::ExecRewardHoldV1, rows::TABLE_EXEC_REWARD_HOLDS_V1};
            let mut expected = BTreeMap::new();
            for (id, root) in &self.exec_v2.roots {
                if matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete)
                    && root.prefix != PalwWorkPrefixStageV2::Unbound
                {
                    let hold = ExecRewardHoldV1 { owner_root: id.as_bytes(), expires_daa: root.expiry_daa };
                    if expected.insert(root.prefix_claim.as_bytes(), hold).is_some() {
                        return Err(bad("a prefix reward is bound more than once".into()));
                    }
                    for (_, slice) in self.exec_v2.slices.range((*id, 0)..=(*id, u32::MAX)) {
                        if expected.insert(slice.evidence_root.as_bytes(), hold).is_some() {
                            return Err(bad("a kernel reward is bound more than once".into()));
                        }
                    }
                }
            }
            for (claim, hold) in &expected {
                let row: misaka_palw_kernel::ledger::ClaimRowV1 = kernel
                    .rows
                    .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, borsh::to_vec(claim).expect("a digest serializes")))
                    .and_then(|bytes| borsh::from_slice(bytes).ok())
                    .ok_or_else(|| bad("a held dependency is missing".into()))?;
                let floor = super::palw_kernel_route_fold_v1::exec_liability_floor_v1(
                    hold.expires_daa,
                    params.window_court(),
                    &kernel.header.policy,
                )?;
                if row.liability_until.is_none_or(|until| until < floor) {
                    return Err(bad("a dependency's liability cannot cover its root window".into()));
                }
            }
            let mut actual = BTreeMap::new();
            for ((table, key), value) in &kernel.rows {
                if *table == TABLE_EXEC_REWARD_HOLDS_V1 {
                    let claim: misaka_palw_kernel::hash::Digest =
                        borsh::from_slice(key).map_err(|_| bad("bad reward hold key".into()))?;
                    let hold: ExecRewardHoldV1 = borsh::from_slice(value).map_err(|_| bad("bad reward hold row".into()))?;
                    actual.insert(claim, hold);
                }
            }
            if expected != actual {
                return Err(bad("EXEC roots and kernel reward holds differ".into()));
            }
        }
        if self.exec_v2.is_empty() {
            self.assert_exec_job_uses()?;
            return Ok(());
        }
        for (claim_id, root) in &self.exec_v2.roots {
            if !matches!(root.phase, PalwWorkRootPhaseV2::Voided { .. }) {
                super::palw_exec_v2_rewards_v1::require_paid_exec_material_window_v1(self, params, claim_id, root.expiry_daa)?;
            }
            if !matches!(root.phase, PalwWorkRootPhaseV2::Voided { .. })
                && let Some(kernel) = &self.kernel_route
            {
                let dependencies = std::iter::once(root.prefix_claim)
                    .chain(self.exec_v2.slices.range((*claim_id, 0)..=(*claim_id, u32::MAX)).map(|(_, row)| row.evidence_root));
                for id in dependencies {
                    if let Some(row) = kernel
                        .rows
                        .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&id)))
                        .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(bytes).ok())
                        && super::palw_kernel_route_fold_v1::exec_provider_lapsed_v1(self, &id, &row)
                    {
                        return Err(bad("a provider lapse was not propagated to its EXEC root".into()));
                    }
                }
            }
            let claim = self.claims.get(claim_id).ok_or_else(|| bad(format!("root {claim_id} outlives its claim")))?;
            if root.extra_executors.len() > PALW_EXEC_V2_MAX_EXTRA_EXECUTORS
                || root.extra_executors.windows(2).any(|pair| pair[0] >= pair[1])
                || root.extra_executors.contains(&root.root_bond)
            {
                return Err(bad(format!("root {claim_id}'s executor allow-list is not canonical")));
            }
            if matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete)
                && root.executor_exposure != claim.reserved
            {
                return Err(bad(format!("root {claim_id}'s executor reservation differs from its native claim")));
            }
            if claim.class_id != root.class_id
                || claim.job_identity != root.canonical_job_id
                || claim.bond != root.root_bond
                || claim.accepted_block != root.anchor
            {
                return Err(bad(format!("root {claim_id}'s REAL binding differs from its claim")));
            }
            if self.exec_v2.jobs.get(&root.job_work_id) != Some(claim_id) {
                return Err(bad(format!("root {claim_id} does not hold its job work identity")));
            }
            crate::palw_work_slice_v2::palw_work_plan_validate_v2(&root.boundaries, root.total_work)
                .map_err(|e| bad(format!("root {claim_id}'s plan: {e}")))?;
            let work_plan = if self.kernel_route.is_some() && root.prefix != PalwWorkPrefixStageV2::Unbound {
                let plan = self
                    .exec_v2_canonical_work_plan_v1(params, &root.class_id, &root.canonical_job_id, &root.boundaries)
                    .map_err(|e| bad(format!("root {claim_id}'s registered execution cost: {e}")))?;
                let facts = &self.tir_class_v1(&root.class_id).expect("the work plan requires registered IR").facts;
                let canonical = crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_of_v1(facts.max_context)
                    .ok_or_else(|| bad(format!("root {claim_id} has no canonical prompt")))?;
                let prompt = crate::palw_attempt_rules_v1::palw_attempt_prompt_ids_v1(
                    &root.canonical_job_id,
                    u64::from(facts.token_bound),
                    canonical.0,
                );
                let input = crate::palw_exec_v2_verify::palw_exec_v2_token_state_root_v1(&[&prompt]);
                if crate::palw_work_slice_v2::palw_exec_job_work_id_v1(&root.class_id, &root.canonical_job_id, &input)
                    != root.job_work_id
                {
                    return Err(bad(format!("root {claim_id}'s useful-work key differs from its REAL inputs")));
                }
                Some(plan)
            } else {
                None
            };
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
                if !root.authorises(&row.executor) {
                    return Err(bad(format!("root {claim_id}'s slice {index} has an unauthorized executor")));
                }
                if *index as usize != position {
                    return Err(bad(format!("root {claim_id}'s slice rows skip an index")));
                }
                if palw_work_plan_range_v2(&root.boundaries, *index) != Some(row.range) {
                    return Err(bad(format!("root {claim_id}'s slice {index} is not the plan's range")));
                }
                // Reuse the one reconstructed plan for all rows, rather than reparsing IR per
                // slice. Pruned dependency retention remains WINDOW's separate obligation.
                if let (Some(plan), Some(kernel)) = (&work_plan, &self.kernel_route)
                    && let Some(claim_bytes) =
                        kernel.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&row.evidence_root)))
                {
                    let kernel_claim: misaka_palw_kernel::ledger::ClaimRowV1 = borsh::from_slice(claim_bytes)
                        .map_err(|e| bad(format!("root {claim_id}'s slice {index} kernel claim: {e}")))?;
                    if let Some(job_bytes) = kernel.rows.get(&(
                        misaka_palw_kernel::rows::TABLE_JOBS_V1,
                        borsh::to_vec(&kernel_claim.job_id).expect("a digest serializes"),
                    )) {
                        let job: misaka_palw_kernel::job::KernelJobV1 = borsh::from_slice(job_bytes)
                            .map_err(|e| bad(format!("root {claim_id}'s slice {index} kernel job: {e}")))?;
                        let misaka_palw_kernel::ledger::ClaimBodyV1::Program { claim, .. } = &kernel_claim.body else {
                            return Err(bad(format!("root {claim_id}'s slice {index} is not a program claim")));
                        };
                        let terms = palw_exec_v2_retention_terms_v1(
                            root.expiry_daa,
                            root.executor_exposure,
                            params.window_court(),
                            &kernel.header.policy,
                        )
                        .map_err(|why| bad(why.into()))?;
                        let nonce =
                            palw_exec_v2_root_slice_job_nonce_v2(claim_id, *index, root, &terms).map_err(|why| bad(why.into()))?;
                        if job.nonce != nonce.as_bytes() {
                            return Err(bad(format!("root {claim_id}'s slice {index} did not authorize its retention terms")));
                        }
                        if job.max_new_tokens as usize != claim.generated.len()
                            || plan.check_slice_job(*index as usize, job.prompt.len(), claim.generated.len()).is_err()
                        {
                            return Err(bad(format!("root {claim_id}'s slice {index} has a different execution scope")));
                        }
                    }
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
                    PalwWorkSliceStageV2::Voided { .. }
                    | PalwWorkSliceStageV2::ProvenFalse { .. }
                    | PalwWorkSliceStageV2::Defaulted { .. } => {}
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
        self.assert_exec_job_uses()?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Writers: the one journaled door to the three tables
// ---------------------------------------------------------------------------------------------

impl TransitionBuilder<'_> {
    pub(super) fn write_exec_v2_row<K, V>(
        &mut self,
        table: u8,
        key: K,
        new: Option<V>,
        map: fn(&mut PalwChainStateV2) -> &mut BTreeMap<K, V>,
    ) -> Result<Option<V>, PalwStateV2Error>
    where
        K: Ord + Clone + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeHeapV1,
        V: Clone + PartialEq + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeHeapV1,
    {
        // RFC-0006: the typed map clone, key clone, absent membership and the three canonical journal buffers.
        self.reserve_encoded_row_v1(&key, &new, map, "native exec v2 row writer")?;
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
        Ok(old)
    }

    /// The one writer of `RootWorkBudget` rows.
    pub(super) fn write_exec_root(&mut self, key: Hash64, new: Option<PalwWorkRootV2>) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the previous-row clone and the two clones `root_changed` re-reads, plus its one job-use deadline node.
        if self.native_writers_tracking_v1() {
            let mut q = super::native_writers::NativeWriteQuoteV1::new("native exec root clones and job-use deadline index");
            if let Some(previous) = self.state.exec_v2.roots.get(&key) {
                q.copy(previous)?;
            }
            q.copy(&new)?.member::<(u64, Hash64), ()>(1)?;
            self.admit_native_write_v1(q)?;
        }
        let previous = self.state.exec_v2.roots.get(&key).cloned();
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_ROOTS_V1, key, new, |s| &mut s.exec_v2.roots)?;
        if let Some(root) = previous {
            super::palw_exec_v2_history_v1::root_changed(&mut self.state, &root, false);
        }
        if let Some(root) = self.state.exec_v2.roots.get(&key).cloned() {
            super::palw_exec_v2_history_v1::root_changed(&mut self.state, &root, true);
        }
        Ok(())
    }

    /// The one writer of `WorkSliceUse` rows.
    pub(super) fn write_exec_slice(&mut self, key: (Hash64, u32), new: Option<PalwWorkSliceRowV2>) -> Result<(), PalwStateV2Error> {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_SLICES_V1, key, new, |s| &mut s.exec_v2.slices).map(drop)
    }

    /// The one writer of `JobWorkUse` rows.
    pub(super) fn write_exec_job(&mut self, key: Hash64, new: Option<Hash64>) -> Result<(), PalwStateV2Error> {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_JOBS_V1, key, new, |s| &mut s.exec_v2.jobs).map(drop)
    }

    /// The one writer of the anchored-block set.
    pub(super) fn write_exec_anchored(&mut self, key: Hash64, new: Option<u64>) -> Result<(), PalwStateV2Error> {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_ANCHORED_V1, key, new, |s| &mut s.exec_v2.anchored).map(drop)
    }

    pub(super) fn write_exec_reward(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_work_slice_v2::PalwExecV2RewardRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the old-row clone, the removed-payee scratch set and the payee index nodes, from the actual legs.
        if self.native_writers_tracking_v1() {
            let mut q = super::native_writers::NativeWriteQuoteV1::new("native exec reward clone, scratch and payee index");
            let old_legs = self.state.exec_v2_rewards.get(&key).map_or(0, |row| row.legs.len() as u64);
            if let Some(previous) = self.state.exec_v2_rewards.get(&key) {
                q.copy(previous)?;
            }
            let new_legs = new.as_ref().map_or(0, |row| row.legs.len() as u64);
            q.member::<PalwBondKeyV2, ()>(old_legs)?;
            q.member::<PalwBondKeyV2, BTreeSet<Hash64>>(new_legs)?.member::<Hash64, ()>(new_legs)?;
            self.admit_native_write_v1(q)?;
        }
        let old = self.state.exec_v2_rewards.get(&key).cloned();
        if let Some(previous) = &old {
            let removed: BTreeSet<_> = previous
                .legs
                .keys()
                .map(|(bond, _)| *bond)
                .filter(|bond| new.as_ref().is_none_or(|row| !row.legs.keys().any(|(who, _)| who == bond)))
                .collect();
            for bond in removed {
                if self.state.exec_v2_reward_holds.contains_key(&(key, bond)) {
                    self.write_exec_reward_hold((key, bond), None)?;
                }
            }
        }
        super::palw_exec_v2_rewards_v1::index_replace(&mut self.state, key, old.as_ref(), new.as_ref());
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_REWARDS_V1, key, new, |s| &mut s.exec_v2_rewards)?;
        Ok(())
    }

    pub(super) fn write_exec_reward_hold(&mut self, key: (Hash64, PalwBondKeyV2), new: Option<u64>) -> Result<(), PalwStateV2Error> {
        self.write_exec_v2_row(PALW_EXEC_V2_TABLE_REWARD_HOLDS_V1, key, new, |s| &mut s.exec_v2_reward_holds).map(drop)
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
        PALW_EXEC_V2_TABLE_ROOTS_V1 => {
            let id: Hash64 = borsh::from_slice(key).map_err(|_| PalwStateV2Error::DeltaMismatch("EXEC root key does not decode"))?;
            let previous = state.exec_v2.roots.get(&id).cloned();
            swap(&mut state.exec_v2.roots, key, old, new, revert)?;
            if let Some(root) = previous {
                super::palw_exec_v2_history_v1::root_changed(state, &root, false);
            }
            if let Some(root) = state.exec_v2.roots.get(&id).cloned() {
                super::palw_exec_v2_history_v1::root_changed(state, &root, true);
            }
            Ok(())
        }
        PALW_EXEC_V2_TABLE_SLICES_V1 => swap(&mut state.exec_v2.slices, key, old, new, revert),
        PALW_EXEC_V2_TABLE_JOBS_V1 => swap(&mut state.exec_v2.jobs, key, old, new, revert),
        PALW_EXEC_V2_TABLE_ANCHORED_V1 => swap(&mut state.exec_v2.anchored, key, old, new, revert),
        PALW_EXEC_V2_TABLE_REWARDS_V1 => {
            let id: Hash64 =
                borsh::from_slice(key).map_err(|_| PalwStateV2Error::DeltaMismatch("an EXEC reward key does not decode"))?;
            let previous = state.exec_v2_rewards.get(&id).cloned();
            swap(&mut state.exec_v2_rewards, key, old, new, revert)?;
            let installed = state.exec_v2_rewards.get(&id).cloned();
            super::palw_exec_v2_rewards_v1::index_replace(state, id, previous.as_ref(), installed.as_ref());
            Ok(())
        }
        PALW_EXEC_V2_TABLE_REWARD_HOLDS_V1 => swap(&mut state.exec_v2_reward_holds, key, old, new, revert),
        PALW_EXEC_V2_TABLE_ORIGINS_V1 => swap(&mut state.exec_v2_origins, key, old, new, revert),
        PALW_EXEC_V2_TABLE_JOB_USES_V1 => {
            let id: Hash64 =
                borsh::from_slice(key).map_err(|_| PalwStateV2Error::DeltaMismatch("EXEC job-use key does not decode"))?;
            let previous = state.exec_v2_job_uses.get(&id).cloned();
            swap(&mut state.exec_v2_job_uses, key, old, new, revert)?;
            let installed = state.exec_v2_job_uses.get(&id).cloned();
            super::palw_exec_v2_history_v1::index_replace(state, id, previous.as_ref(), installed.as_ref());
            Ok(())
        }
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
    // Amendment 1 (spec §10.1): where the kernel route is in force, a session opens only on a G14-complete class — bound to a kernel
    // class — and its plan root is that class's verification plan, so every slice has a route an outside bond can prosecute.
    if let Some(kernel) = builder.state.kernel_route.as_ref() {
        let binding = kernel.kernel_binding_v1(&claim.class_id).ok_or_else(|| no(R::ClassNotKernelBound))?;
        if declaration.plan_root != binding.plan_root {
            return Err(no(R::PlanNotKernels));
        }
    }
    // The job is the claim's own: a declaration cannot name another job than the execution this claim answered.
    if claim.job_identity == Hash64::default() || claim.job_identity != declaration.canonical_job_id {
        return Err(no(R::ClaimBindingMismatch));
    }
    // REAL lottery weight remains the claim's chain-position credit. Execution cells use IR cost.
    let real_weight = builder
        .state
        .palw_claim_canonical_weight_v1(&claim, builder.extras.canonical_work_daa)
        .ok_or_else(|| no(R::ClaimWorkNotDerivable))?;
    let admitted = if builder.state.kernel_route.is_some() {
        let plan = builder
            .state
            .exec_v2_canonical_work_plan_v1(builder.params, &claim.class_id, &claim.job_identity, &declaration.boundaries)
            .map_err(|_| no(R::ExecutionWorkNotDerivable))?;
        plan.check_declared_work(declaration.total_work, &declaration.boundaries).map_err(|_| no(R::ExecutionWorkNotDerivable))?;
        plan.boundaries()[0]
    } else {
        // Arithmetic-only test histories have no kernel route and are never armable rulesets.
        u64::try_from(real_weight).map_err(|_| no(R::Overflow))?
    };
    if declaration.boundaries.first().copied() != Some(admitted) {
        return Err(no(R::PrefixMismatch { declared: declaration.boundaries.first().copied().unwrap_or(0), admitted }));
    }
    // **GAP-62 (X8R round 3): the initial boundary is the REAL claim's own output, carried by the root bond's prefix claim on the kernel
    // route** — derived from chain data, never the root bond's to choose. The prefix claim's later conviction, default or post-Final
    // forfeit voids the whole session (`sync_slice_verification_v2`); a pending one holds the root's `Final`.
    let prefix = match builder.state.kernel_route.as_ref() {
        None => PalwWorkPrefixStageV2::Unbound,
        Some(kernel) => {
            let binding = kernel.kernel_binding_v1(&claim.class_id).ok_or_else(|| no(R::ClassNotKernelBound))?;
            let tir = builder.state.tir_class_v1(&claim.class_id).ok_or_else(|| no(R::PrefixNotDerivable))?;
            let (prompt, context) = palw_exec_v2_real_job_v1(&tir.facts, &claim.job_identity, builder.extras.prompt_ids_form_v1())
                .ok_or_else(|| no(R::PrefixNotDerivable))?;
            if declaration.input_root != crate::palw_exec_v2_verify::palw_exec_v2_token_state_root_v1(&[&prompt]) {
                return Err(no(R::ClaimBindingMismatch));
            }
            let row = kernel
                .rows
                .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&declaration.prefix_claim)))
                .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(bytes).ok())
                .ok_or_else(|| no(R::PrefixClaimMissing))?;
            let job = kernel
                .rows
                .get(&(misaka_palw_kernel::rows::TABLE_JOBS_V1, borsh::to_vec(&row.job_id).expect("a digest serializes")))
                .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::job::KernelJobV1>(bytes).ok());
            if row.rewarded {
                return Err(no(R::PrefixAlreadyPaid));
            }
            if row.reserved == 0
                || (matches!(row.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. })
                    && row.liability_until.is_none_or(|until| daa > until))
            {
                return Err(no(R::PrefixClaimFailed));
            }
            let inputs = PalwExecV2PrefixInputsV1 {
                root_claim_id: claim_id,
                prefix_work: admitted,
                canonical_job_id: declaration.canonical_job_id,
                plan_root: declaration.plan_root,
                initial_state_root: declaration.initial_state_root,
                real_prompt: &prompt,
                real_context: &context,
                real_output_root: claim.output_root,
            };
            palw_exec_v2_prefix_binding_v1(
                &inputs,
                &binding,
                &row,
                job.as_ref(),
                &crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&claim.bond),
                daa,
            )
            .map_err(no)?;
            match dependency_outcome(&builder.state, &declaration.prefix_claim, &row, daa) {
                PalwSliceVerificationV1::Verified { .. } => PalwWorkPrefixStageV2::Verified {
                    verified_daa: daa,
                    route_reward: palw_exec_v2_route_reward_v1(&row, kernel.header.policy.claim_reward),
                },
                _ => PalwWorkPrefixStageV2::Pending,
            }
        }
    };
    if declaration.expiry_daa <= daa || declaration.expiry_daa > daa.saturating_add(PALW_EXEC_V2_MAX_ROOT_LIFETIME_DAA) {
        return Err(no(R::BadExpiry));
    }
    let job_work_id = declaration.job_work_id(&claim.class_id);
    if builder.state.exec_v2.jobs.contains_key(&job_work_id)
        || builder.state.exec_v2_job_uses.get(&job_work_id).is_some_and(|row| daa < row.expires_daa)
    {
        return Err(no(R::JobWorkAlreadyUsed));
    }
    let job_use = super::palw_exec_v2_history_v1::authorize(&builder.state, declaration, &claim, daa)?;
    if builder.state.exec_v2_job_uses.len() >= crate::palw_work_slice_v2::PALW_EXEC_V2_MAX_JOB_USES_V1 {
        return Err(no(R::JobHistoryQuota));
    }
    // An executor stands behind the root exactly as its producer does: the claim's own reservation.
    let exposure = claim.reserved;
    if exposure == 0 {
        return Err(no(R::ClaimUnreserved));
    }
    if builder.state.exec_v2_retained_slots_v1() >= PALW_EXEC_V2_MAX_OPEN_ROOTS
        || builder.state.exec_v2.open_roots_of(&claim.bond) >= PALW_EXEC_V2_MAX_OPEN_ROOTS_PER_BOND
    {
        return Err(no(R::RootQuota));
    }
    // This is the root owner's allow-list, not the listed workers' consent.
    // Funding is checked only when each worker's first signed slice is accepted.
    for bond in &declaration.extra_executors {
        if *bond == claim.bond {
            return Err(no(R::ExecutorsNotCanonical));
        }
        let active = builder.state.bonds.get(bond).is_some_and(|record| matches!(record.status, PalwBondStatusV2::Active));
        if !active {
            return Err(no(R::ExecutorUnfunded));
        }
    }
    super::palw_exec_v2_rewards_v1::require_paid_exec_material_window_v1(
        &builder.state,
        builder.params,
        &claim_id,
        declaration.expiry_daa,
    )?;
    builder.state.require_monetary_material_enrollment_v1(builder.params, claim_id, daa, None)?;
    if let Some(consent) = builder.state.monetary_material_row_v1(claim_id) {
        if consent.terms.exec_plan_root != Some(declaration.plan_root)
            || !consent.terms.dependencies.contains(&declaration.prefix_claim)
        {
            return Err(refused("monetary provider did not sign this root's complete execution plan/prefix"));
        }
    }
    // ---- writes ----
    if builder.state.kernel_route.is_some() {
        super::palw_kernel_route_fold_v1::bind_exec_reward_v1(
            builder,
            claim_id,
            declaration.prefix_claim,
            declaration.expiry_daa,
            daa,
        )?;
    }
    builder.write_exec_job(job_work_id, Some(claim_id))?;
    builder.write_exec_job_use(job_work_id, Some(job_use))?;
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
            prefix_claim: declaration.prefix_claim,
            prefix,
        }),
    )?;
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
) -> Result<Vec<(BlockHash, Result<(), PalwSliceRefusalV2>)>, PalwStateV2Error> {
    let mut verdicts = Vec::with_capacity(covered.len());
    let mut folded = 0usize;
    for carrier in covered {
        let verdict = match builder.state.exec_v2_admit_slice_at_depth_v2(
            builder.params,
            ctx.daa_score,
            carrier,
            folded,
            palw_settled_anchor_depth_v1(builder.extras),
        ) {
            Ok(admission) => {
                let checkpoint = builder.try_checkpoint()?;
                match builder.hold_exec_recipient_v1(
                    carrier.slice.root_claim_id,
                    carrier.slice.executor_bond,
                    carrier.slice.evidence_root,
                    admission.work,
                    ctx.daa_score,
                ) {
                    Ok(()) => {
                        accept_slice_v2(builder, ctx, carrier, admission)?;
                        folded += 1;
                        Ok(())
                    }
                    Err(PalwStateV2Error::BondBudgetExhausted { .. } | PalwStateV2Error::BondBudgetRefused(_)) => {
                        builder.restore(checkpoint);
                        Err(PalwSliceRefusalV2::RecipientRewardBudget)
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(refusal) => Err(refusal),
        };
        // **The refusal record** (the X8R review: the verdicts were dropped): one note per covered carrier, in the fold's order, in the
        // block's delta — node-durable, branch-local, read by RPC op 240, and inert on apply and revert (no state moves).
        builder.entries.push(PalwDeltaEntryV2::ExecV2Verdict {
            carrier: carrier.carrier,
            code: verdict.as_ref().err().map(PalwSliceRefusalV2::code).unwrap_or(0),
        });
        verdicts.push((carrier.carrier, verdict));
    }
    Ok(verdicts)
}

/// The writes of one admitted slice (checked arithmetic cannot fail here: the admission derived `work` from the plan and the root's
/// counters are bounded by the plan, which `palw_work_plan_validate_v2` bounds below `u64::MAX`).
fn accept_slice_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    carrier: &PalwExecV2CoveredSliceV1,
    admission: PalwSliceAdmissionV2,
) -> Result<(), PalwStateV2Error> {
    let slice = &carrier.slice;
    let mut root = builder.state.exec_v2.roots.get(&slice.root_claim_id).expect("admission found the root").clone();
    if builder.state.kernel_route.is_some() {
        super::palw_kernel_route_fold_v1::bind_exec_reward_v1(
            builder,
            slice.root_claim_id,
            slice.evidence_root,
            root.expiry_daa,
            ctx.daa_score,
        )?;
    }
    if slice.executor_bond != root.root_bond
        && !builder.state.exec_v2_participants_v1(&slice.root_claim_id, &root).contains(&slice.executor_bond)
    {
        let held = builder.state.reserved_exposure.get(&slice.executor_bond).copied().unwrap_or(0);
        let next = held.checked_add(root.executor_exposure).ok_or(PalwStateV2Error::Overflow("exec v2 executor exposure"))?;
        builder.write_exposure(slice.executor_bond, Some(next))?;
    }
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
            stage: match admission.verified_at {
                Some(_) => PalwWorkSliceStageV2::Verified {
                    verified_daa: ctx.daa_score,
                    leg_cap: admission.leg_cap,
                    route_reward: admission.route_reward,
                },
                None => PalwWorkSliceStageV2::Pending,
            },
        }),
    )?;
    root.next_index += 1;
    root.last_state_root = slice.result_state_root;
    root.accepted_work = root.accepted_work.saturating_add(admission.work);
    // Amendment 1: a slice whose kernel claim is already Final is verified at once (it never counts against the pending depth).
    match admission.verified_at {
        Some(_) => root.verified_work = root.verified_work.saturating_add(admission.work),
        None => root.pending += 1,
    }
    if root.next_index == root.slice_count() {
        root.phase = PalwWorkRootPhaseV2::Complete;
    }
    let ready = root.ready_for_final();
    builder.write_exec_root(slice.root_claim_id, Some(root))?;
    if ready {
        // The last slice arrived verified: the claim's `Final` hold is released now, as a verification in a later block would.
        release_final_hold_v2(builder, slice.root_claim_id, ctx.daa_score)?;
    }
    Ok(())
}

/// **Record what one anchoring block covered.** After the permits and the slices (steps 7 and 7b), at a fixed place. Drops the
/// entries the window has passed, refuses a block covered twice (the closure never offers one: this is the ledger's own guard, as
/// the permit's is) or one anchored outside the window, and writes the covered blocks. Bounded by the closure's own leaf bound.
pub(super) fn record_exec_anchor_v2(
    builder: &mut TransitionBuilder<'_>,
    fold: &crate::palw_exec_v2_anchor::PalwExecV2AnchorFoldV1,
) -> Result<(), PalwStateV2Error> {
    if builder.params.exec_v2_from_daa().is_none() {
        return Err(refused("an anchor before palw_exec_payload_v2 is in force"));
    }
    let stale: Vec<Hash64> = builder
        .state
        .exec_v2
        .anchored
        .iter()
        .filter(|(_, span)| span.checked_add(1).is_none_or(|next| next < fold.span_now))
        .map(|(block, _)| *block)
        .collect();
    for block in stale {
        builder.write_exec_anchored(block, None)?;
    }
    for (block, span) in &fold.members {
        if !crate::palw_exec_v2_anchor::palw_exec_v2_window_ok(*span, fold.span_now) {
            return Err(refused(format!("{block} is anchored in span {span}, outside the window of span {}", fold.span_now)));
        }
        if builder.state.exec_v2.anchored.contains_key(block) {
            return Err(refused(format!("{block} was covered by an earlier anchor")));
        }
        builder.write_exec_anchored(*block, Some(*span))?;
    }
    Ok(())
}

impl PalwChainStateV2 {
    /// **Has an anchor already covered this EXEC block?** The closure's boundary: a covered block is neither covered again nor
    /// walked past.
    pub fn exec_v2_anchored_v1(&self, block: &Hash64) -> bool {
        self.exec_v2.anchored.contains_key(block)
    }
}

// ---------------------------------------------------------------------------------------------
// Settlement at Final, void with the claim, retirement with the claim, expiry
// ---------------------------------------------------------------------------------------------

/// The root's one settlement. Bound kernel fees stay escrowed until this step and replace
/// native issuance within the executor's share. All debits, refunds and the Settled marker
/// belong to the same journal; a duplicate Final pays nothing.
pub(super) fn settle_at_final_v2(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    amount: u64,
    final_daa: u64,
) -> Result<(u64, Vec<(PalwBondKeyV2, u64)>), PalwStateV2Error> {
    let Some(mut root) = builder.state.exec_v2.roots.get(claim_id).cloned() else { return Ok((amount, Vec::new())) };
    match root.phase {
        PalwWorkRootPhaseV2::Settled { .. } | PalwWorkRootPhaseV2::Voided { .. } => return Ok((0, Vec::new())),
        PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete => {}
    }
    if !root.ready_for_final() || !builder.state.exec_v2_selected_approvals_v1(claim_id, &root) || final_daa >= root.expiry_daa {
        return Err(PalwStateV2Error::ExecV2Refused("root Final requires complete verification before expiry".into()));
    }
    let rows: Vec<((Hash64, u32), PalwWorkSliceRowV2)> =
        builder.state.exec_v2.slices.range((*claim_id, 0)..=(*claim_id, u32::MAX)).map(|(key, row)| (*key, row.clone())).collect();
    let credited: Vec<_> = rows
        .iter()
        .map(|(_, row)| row.range.work().map(|work| (row.executor, work)).ok_or(PalwStateV2Error::Overflow("exec v2 slice work")))
        .collect::<Result<_, _>>()?;
    let mut settlement = settle_root_v2(amount, root.total_work, root.prefix_work(), &credited, root.root_bond)
        .ok_or(PalwStateV2Error::Overflow("exec v2 settlement"))?;
    super::palw_exec_v2_rewards_v1::start(builder, *claim_id, amount, final_daa)?;
    if builder.state.kernel_route.is_some() {
        let mut held: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
        for (_, row) in &rows {
            let cap = match row.stage {
                PalwWorkSliceStageV2::Verified { leg_cap, .. } => leg_cap,
                _ => 0,
            };
            let entry = held.entry(row.executor).or_default();
            *entry = entry.checked_add(cap).ok_or(PalwStateV2Error::Overflow("exec v2 leg cap"))?;
        }
        let mut excess = 0u64;
        for (bond, leg) in &mut settlement.slice_legs {
            let cap = held.get(bond).copied().unwrap_or(0);
            excess = excess.checked_add(leg.saturating_sub(cap)).ok_or(PalwStateV2Error::Overflow("exec v2 leg cap"))?;
            *leg = (*leg).min(cap);
        }
        settlement.root_leg = settlement.root_leg.checked_add(excess).ok_or(PalwStateV2Error::Overflow("exec v2 leg cap"))?;
        let mut caps: BTreeMap<_, _> = settlement.slice_legs.iter().copied().collect();
        caps.insert(root.root_bond, settlement.root_leg);
        let mut claims = vec![(root.prefix_claim, root.root_bond)];
        claims.extend(rows.iter().map(|(_, row)| (row.evidence_root, row.executor)));
        let paid = super::palw_kernel_route_fold_v1::finish_exec_rewards_v1(builder, *claim_id, &claims, &caps, final_daa)?;
        let mut fees: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
        for ((_, bond), fee) in claims.iter().zip(&paid) {
            let entry = fees.entry(*bond).or_default();
            *entry = entry.checked_add(*fee).ok_or(PalwStateV2Error::Overflow("exec v2 fee total"))?;
        }
        settlement.root_leg -= fees.get(&root.root_bond).copied().unwrap_or(0);
        for (bond, leg) in &mut settlement.slice_legs {
            *leg -= fees.get(bond).copied().unwrap_or(0);
        }
        if let PalwWorkPrefixStageV2::Verified { route_reward, .. } = &mut root.prefix {
            *route_reward = paid[0];
        }
        for ((key, mut row), fee) in rows.into_iter().zip(paid.into_iter().skip(1)) {
            if let PalwWorkSliceStageV2::Verified { route_reward, .. } = &mut row.stage {
                *route_reward = fee;
            }
            builder.write_exec_slice(key, Some(row))?;
        }
    }
    settlement.slice_legs.retain(|(_, leg)| *leg > 0);
    for bond in builder.state.exec_v2_participants_v1(claim_id, &root) {
        if bond == root.root_bond {
            continue;
        }
        let paid_later = settlement.slice_legs.iter().any(|(payee, _)| *payee == bond)
            || builder
                .state
                .exec_v2_rewards
                .get(claim_id)
                .is_some_and(|receipt| receipt.legs.iter().any(|((payee, _), payout)| *payee == bond && payout.amount > 0));
        if !paid_later {
            builder.cancel_native_recipient_v1(*claim_id, bond, 2)?;
        }
    }
    release_executor_exposure_v2(builder, claim_id, &root)?;
    root.phase = PalwWorkRootPhaseV2::Settled { settled_daa: final_daa };
    builder.write_exec_root(*claim_id, Some(root))?;
    Ok((settlement.root_leg, settlement.slice_legs))
}

/// Return exactly one exposure for each worker who accepted a slice of this root.
fn release_executor_exposure_v2(
    builder: &mut TransitionBuilder<'_>,
    id: &Hash64,
    root: &PalwWorkRootV2,
) -> Result<(), PalwStateV2Error> {
    for bond in builder.state.exec_v2_participants_v1(id, root) {
        let held = builder.state.reserved_exposure.get(&bond).copied().unwrap_or(0);
        let next =
            held.checked_sub(root.executor_exposure).ok_or(PalwStateV2Error::Overflow("exec v2 executor exposure underflow"))?;
        builder.write_exposure(bond, if next == 0 { None } else { Some(next) })?;
    }
    Ok(())
}

/// **The claim was voided** (by any route): the root and its pending slices go void with it, the extra executors' exposure
/// returns, nobody is paid and nobody is charged here — a slashing is a conviction's, proven where it is proven. The rows stay
/// until the claim retires. The separate job-use history survives until authorization closes. A settled root has already returned exposure.
pub(super) fn on_claim_voided_v2(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    voided_daa: u64,
) -> Result<(), PalwStateV2Error> {
    let Some(root) = builder.state.exec_v2.roots.get(claim_id).cloned() else { return Ok(()) };
    for bond in builder.state.exec_v2_participants_v1(claim_id, &root) {
        builder.cancel_native_recipient_v1(*claim_id, bond, 2)?;
    }
    super::palw_kernel_route_fold_v1::cancel_exec_rewards_v1(builder, *claim_id, voided_daa)?;
    if matches!(root.phase, PalwWorkRootPhaseV2::Voided { .. }) {
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
        builder.write_exec_slice(key, Some(row))?;
    }
    if matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete) {
        release_executor_exposure_v2(builder, claim_id, &root)?;
    } else {
        super::palw_exec_v2_rewards_v1::burn(builder, *claim_id)?;
    }
    let mut voided = root;
    voided.phase = PalwWorkRootPhaseV2::Voided { from_index, voided_daa };
    builder.write_exec_root(*claim_id, Some(voided))?;
    Ok(())
}

/// **The claim retired**: remove its active root, slices and job index. The authenticated job-use history
/// remains independently until the original REAL header's finite authorization closes; retirement never renews it.
/// A no-op for a claim with no root.
pub(super) fn on_claim_retired_v2(builder: &mut TransitionBuilder<'_>, claim_id: &Hash64) -> Result<(), PalwStateV2Error> {
    let Some(root) = builder.state.exec_v2.roots.get(claim_id).cloned() else { return Ok(()) };
    let keys: Vec<(Hash64, u32)> =
        builder.state.exec_v2.slices.range((*claim_id, 0)..=(*claim_id, u32::MAX)).map(|(key, _)| *key).collect();
    for key in keys {
        builder.write_exec_slice(key, None)?;
    }
    builder.write_exec_job(root.job_work_id, None)?;
    builder.write_exec_root(*claim_id, None)?;
    Ok(())
}

/// **Every open root expires, including a ready root whose REAL Final is delayed.**
/// Run before objects; release job escrows without reward or debit. This bounds reservation retention
/// independently of court progress. Bounded by [`PALW_EXEC_V2_MAX_OPEN_ROOTS`].
pub(super) fn sweep_expired_roots_v2(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    super::palw_exec_v2_history_v1::sweep(builder, ctx.daa_score)?;
    if builder.state.exec_v2.roots.is_empty() {
        return Ok(());
    }
    let expired: Vec<Hash64> = builder
        .state
        .exec_v2
        .roots
        .iter()
        .filter(|(_, root)| {
            matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete) && ctx.daa_score >= root.expiry_daa
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
    if builder
        .state
        .exec_v2
        .roots
        .get(&claim_id)
        .is_some_and(|root| root.ready_for_final() && builder.state.exec_v2_selected_approvals_v1(&claim_id, root))
    {
        builder.rearm_claim_deadline_dl1_v1(claim_id, now_daa)?;
    }
    Ok(())
}

fn dependency_outcome(
    state: &PalwChainStateV2,
    id: &Hash64,
    row: &misaka_palw_kernel::ledger::ClaimRowV1,
    daa: u64,
) -> PalwSliceVerificationV1 {
    let outcome = palw_exec_v2_claim_outcome_v1(row, daa);
    if outcome != PalwSliceVerificationV1::ProvenFalse && super::palw_kernel_route_fold_v1::exec_provider_lapsed_v1(state, id, row) {
        PalwSliceVerificationV1::Defaulted
    } else if matches!(outcome, PalwSliceVerificationV1::Verified { .. })
        && state.kernel_route.as_ref().is_none_or(|route| !palw_exec_v2_selected_approval_v1(route, id))
    {
        PalwSliceVerificationV1::Pending
    } else {
        outcome
    }
}

/// **Amendment 1 (spec §10.1–10.2): every slice follows its kernel claim.**
/// Run each block after the kernel route's closing tick. Read kernel claim decisions and transferred-provider subject changes in this
/// block's journal. Only dependencies named by those entries are reconsidered;
/// the journal also captures a post-Final provider lapse with no claim-row change.
///
/// A Final pending dependency with native selected approval becomes Verified
/// and can release the root's Final hold. An unapproved optimistic Final stays
/// Pending. Conviction remains ProvenFalse; unavailable/forfeited dependencies and
/// transferred provider lapses become Defaulted, including after root settlement.
/// The root is voided and unreleased funds are burned once. Provider failure never
/// charges an executor: the provider court settles its signed reservation.
///
/// A no-op on a chain with no root or no kernel route (below the fence). This
/// journal selection alone does not prove a worst-case per-block resource bound.
pub(super) fn sync_slice_verification_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    if builder.state.exec_v2.roots.is_empty() || builder.state.kernel_route.is_none() {
        return Ok(());
    }
    let written: std::collections::BTreeSet<Vec<u8>> = builder
        .entries
        .iter()
        .filter_map(|entry| match entry {
            PalwDeltaEntryV2::KernelRouteRow { table, key, .. } if *table == misaka_palw_kernel::rows::TABLE_CLAIMS_V1 => {
                Some(key.clone())
            }
            PalwDeltaEntryV2::KernelRouteRow { table, new: Some(bytes), .. }
                if *table == crate::palw_claim_challenge_v2::PAID_CLAIM_FINALS_V2 =>
            {
                let row: crate::palw_claim_challenge_v2::PalwPaidClaimFinalV2 = borsh::from_slice(bytes).ok()?;
                Some(palw_exec_v2_kernel_key_v1(&Hash64::from_bytes(row.base_claim)))
            }
            PalwDeltaEntryV2::KernelRouteRow { table, key, .. }
                if *table == crate::palw_provider_court_v1::PALW_PROVIDER_COURT_TABLE_SUBJECTS_V1 =>
            {
                match borsh::from_slice::<crate::palw_provider_court_v1::ProviderSubjectV1>(key).ok()? {
                    crate::palw_provider_court_v1::ProviderSubjectV1::KernelClaim { claim } => {
                        Some(palw_exec_v2_kernel_key_v1(&claim))
                    }
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    if written.is_empty() {
        return Ok(());
    }
    // **GAP-62: each bound prefix follows its claim first** — a prefix that fails voids the whole session, so its slices are not read
    // after it in this block.
    let prefixes: Vec<(Hash64, PalwWorkRootV2)> = builder
        .state
        .exec_v2
        .roots
        .iter()
        .filter(|(_, root)| {
            matches!(root.phase, PalwWorkRootPhaseV2::Open | PalwWorkRootPhaseV2::Complete | PalwWorkRootPhaseV2::Settled { .. })
        })
        .filter(|(_, root)| matches!(root.prefix, PalwWorkPrefixStageV2::Pending | PalwWorkPrefixStageV2::Verified { .. }))
        .filter(|(_, root)| written.contains(&palw_exec_v2_kernel_key_v1(&root.prefix_claim)))
        .map(|(id, root)| (*id, root.clone()))
        .collect();
    for (root_id, root) in prefixes {
        let (claim_row, claim_reward) = match builder.state.kernel_route.as_ref() {
            Some(kernel) => (
                kernel
                    .rows
                    .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&root.prefix_claim)))
                    .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(bytes).ok()),
                kernel.header.policy.claim_reward,
            ),
            None => (None, 0),
        };
        let pending = matches!(root.prefix, PalwWorkPrefixStageV2::Pending);
        let outcome = match &claim_row {
            Some(claim) => dependency_outcome(&builder.state, &root.prefix_claim, claim, ctx.daa_score),
            None if pending => PalwSliceVerificationV1::Defaulted,
            None => continue,
        };
        match outcome {
            PalwSliceVerificationV1::Verified { .. } if pending => {
                let route_reward = claim_row.as_ref().map(|claim| palw_exec_v2_route_reward_v1(claim, claim_reward)).unwrap_or(0);
                let mut next = root;
                next.prefix = PalwWorkPrefixStageV2::Verified { verified_daa: ctx.daa_score, route_reward };
                builder.write_exec_root(root_id, Some(next))?;
                release_final_hold_v2(builder, root_id, ctx.daa_score)?;
            }
            PalwSliceVerificationV1::ProvenFalse => {
                void_prefix_v2(builder, root_id, PalwWorkPrefixStageV2::ProvenFalse { daa: ctx.daa_score }, ctx)?;
            }
            PalwSliceVerificationV1::Defaulted => {
                void_prefix_v2(builder, root_id, PalwWorkPrefixStageV2::Defaulted { daa: ctx.daa_score }, ctx)?;
            }
            _ => {}
        }
    }
    let touched: Vec<((Hash64, u32), PalwWorkSliceRowV2)> = builder
        .state
        .exec_v2
        .slices
        .iter()
        .filter(|(_, row)| matches!(row.stage, PalwWorkSliceStageV2::Pending | PalwWorkSliceStageV2::Verified { .. }))
        .filter(|(_, row)| written.contains(&palw_exec_v2_kernel_key_v1(&row.evidence_root)))
        .map(|(key, row)| (*key, row.clone()))
        .collect();
    for ((root_id, index), row) in touched {
        let Some(root) = builder.state.exec_v2.roots.get(&root_id).cloned() else { continue };
        if matches!(root.phase, PalwWorkRootPhaseV2::Voided { .. }) {
            continue; // voided earlier in this loop
        }
        let (claim_row, claim_reward) = match builder.state.kernel_route.as_ref() {
            Some(kernel) => (
                kernel
                    .rows
                    .get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, palw_exec_v2_kernel_key_v1(&row.evidence_root)))
                    .and_then(|bytes| borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(bytes).ok()),
                kernel.header.policy.claim_reward,
            ),
            None => (None, 0),
        };
        let pending = matches!(row.stage, PalwWorkSliceStageV2::Pending);
        // A claim the route no longer holds cannot verify a pending slice (a default); a verified slice whose claim the route has
        // retired keeps its stage and its cap.
        let outcome = match &claim_row {
            Some(claim) => dependency_outcome(&builder.state, &row.evidence_root, claim, ctx.daa_score),
            None if pending => PalwSliceVerificationV1::Defaulted,
            None => continue,
        };
        match outcome {
            PalwSliceVerificationV1::Verified { .. } if pending => {
                let work = row.range.work().ok_or(PalwStateV2Error::Overflow("exec v2 slice work"))?;
                let leg_cap = claim_row.as_ref().map(|claim| palw_exec_v2_leg_cap_v1(claim, claim_reward)).unwrap_or(0);
                let route_reward = claim_row.as_ref().map(|claim| palw_exec_v2_route_reward_v1(claim, claim_reward)).unwrap_or(0);
                let mut verified = row;
                verified.stage = PalwWorkSliceStageV2::Verified { verified_daa: ctx.daa_score, leg_cap, route_reward };
                builder.write_exec_slice((root_id, index), Some(verified))?;
                let mut next = root;
                next.pending = next.pending.checked_sub(1).ok_or(PalwStateV2Error::Overflow("exec v2 pending"))?;
                next.verified_work = next.verified_work.checked_add(work).ok_or(PalwStateV2Error::Overflow("exec v2 verified"))?;
                builder.write_exec_root(root_id, Some(next))?;
                release_final_hold_v2(builder, root_id, ctx.daa_score)?;
            }
            PalwSliceVerificationV1::ProvenFalse => {
                void_suffix_v2(builder, root_id, index, PalwWorkSliceStageV2::ProvenFalse { daa: ctx.daa_score }, ctx)?;
            }
            // A pending slice's claim defaulted or timed out — or a verified slice's claim forfeited its reservation after Final
            // (a demand it left unserved inside its liability horizon, the route's `PostFinalDefault`): the material is withheld, so
            // the slice is defaulted while its root has not settled, exactly as a conviction after Final proves it false.
            PalwSliceVerificationV1::Defaulted => {
                void_suffix_v2(builder, root_id, index, PalwWorkSliceStageV2::Defaulted { daa: ctx.daa_score }, ctx)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// **The suffix void** (amendment 1, §10.2): slice `from_index` of `root_id` takes `cause` (`ProvenFalse` or `Defaulted`), every later
/// accepted slice is `Voided` (each chains from its predecessor's result), the root becomes `Voided { from_index }` with its extra
/// executors' exposure returned, and the root claim is voided by the matching reason — uncharged here: the evidence-bound charge is the
/// kernel route's. No prefix or partial payment (spec §6). Earlier slices keep their stage; the rows stay until the claim retires.
pub(super) fn void_suffix_v2(
    builder: &mut TransitionBuilder<'_>,
    root_id: Hash64,
    from_index: u32,
    cause: PalwWorkSliceStageV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    let Some(root) = builder.state.exec_v2.roots.get(&root_id).cloned() else { return Ok(()) };
    super::palw_kernel_route_fold_v1::cancel_exec_rewards_v1(builder, root_id, daa)?;
    if matches!(root.phase, PalwWorkRootPhaseV2::Voided { .. }) {
        return Ok(());
    }
    let settled = matches!(root.phase, PalwWorkRootPhaseV2::Settled { .. });
    let suffix: Vec<((Hash64, u32), PalwWorkSliceRowV2)> = builder
        .state
        .exec_v2
        .slices
        .range((root_id, from_index)..=(root_id, u32::MAX))
        .map(|(key, row)| (*key, row.clone()))
        .collect();
    for ((_, index), mut row) in suffix {
        row.stage = if index == from_index { cause } else { PalwWorkSliceStageV2::Voided { voided_daa: daa } };
        builder.write_exec_slice((root_id, index), Some(row))?;
    }
    if !settled {
        release_executor_exposure_v2(builder, &root_id, &root)?;
    }
    let mut voided = root;
    voided.phase = PalwWorkRootPhaseV2::Voided { from_index, voided_daa: daa };
    builder.write_exec_root(root_id, Some(voided))?;
    let reason = match cause {
        PalwWorkSliceStageV2::ProvenFalse { .. } => PalwVoidReasonV2::WorkSliceProvenFalse,
        _ => PalwVoidReasonV2::WorkSliceDefaulted,
    };
    // The root is already `Voided`, so the void hook (`on_claim_voided_v2`) finds nothing open and leaves it as written here.
    if settled {
        let dependency = builder.state.exec_v2.slices[&(root_id, from_index)].evidence_root;
        super::palw_exec_v2_rewards_v1::invalidate_final(builder, root_id, dependency, ctx, reason)?;
    } else if let Some(claim) = builder.state.claims.get(&root_id).cloned()
        && !claim.phase.is_terminal()
    {
        builder.void_claim(root_id, &claim, daa, reason)?;
    }
    Ok(())
}

/// **The prefix void** (GAP-62): the root's own prefix claim failed — `cause` is `ProvenFalse` (convicted, before or after its `Final`)
/// or `Defaulted` (withheld material, a timeout, a forfeit after `Final`). The REAL claim's run is not the session's honest start, so
/// every accepted slice is `Voided` (each chains from it), the root becomes `Voided { from_index: 0 }` with its extra executors'
/// exposure returned, and the REAL claim is voided `WorkSliceProvenFalse` / `WorkSliceDefaulted` (the prefix is the session's first
/// verified piece) — uncharged here: the evidence-bound charge is the route's, on the root bond's kernel reservation.
pub(super) fn void_prefix_v2(
    builder: &mut TransitionBuilder<'_>,
    root_id: Hash64,
    cause: PalwWorkPrefixStageV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    let Some(root) = builder.state.exec_v2.roots.get(&root_id).cloned() else { return Ok(()) };
    super::palw_kernel_route_fold_v1::cancel_exec_rewards_v1(builder, root_id, daa)?;
    if matches!(root.phase, PalwWorkRootPhaseV2::Voided { .. }) {
        return Ok(());
    }
    let settled = matches!(root.phase, PalwWorkRootPhaseV2::Settled { .. });
    let dependency = root.prefix_claim;
    let slices: Vec<((Hash64, u32), PalwWorkSliceRowV2)> =
        builder.state.exec_v2.slices.range((root_id, 0)..=(root_id, u32::MAX)).map(|(key, row)| (*key, row.clone())).collect();
    for (key, mut row) in slices {
        row.stage = PalwWorkSliceStageV2::Voided { voided_daa: daa };
        builder.write_exec_slice(key, Some(row))?;
    }
    if !settled {
        release_executor_exposure_v2(builder, &root_id, &root)?;
    }
    let mut voided = root;
    voided.prefix = cause;
    voided.phase = PalwWorkRootPhaseV2::Voided { from_index: 0, voided_daa: daa };
    builder.write_exec_root(root_id, Some(voided))?;
    let reason = match cause {
        PalwWorkPrefixStageV2::ProvenFalse { .. } => PalwVoidReasonV2::WorkSliceProvenFalse,
        _ => PalwVoidReasonV2::WorkSliceDefaulted,
    };
    if settled {
        super::palw_exec_v2_rewards_v1::invalidate_final(builder, root_id, dependency, ctx, reason)?;
    } else if let Some(claim) = builder.state.claims.get(&root_id).cloned()
        && !claim.phase.is_terminal()
    {
        builder.void_claim(root_id, &claim, daa, reason)?;
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
    // No kernel claim backs a test-door verification: the cap is unbounded, and settlement caps only where a kernel route exists.
    row.stage = PalwWorkSliceStageV2::Verified { verified_daa, leg_cap: u64::MAX, route_reward: 0 };
    builder.write_exec_slice((claim_id, index), Some(row))?;
    let mut root = builder.state.exec_v2.roots.get(&claim_id).cloned().ok_or_else(|| refused("no such root"))?;
    root.pending = root.pending.checked_sub(1).ok_or(PalwStateV2Error::Overflow("exec v2 pending"))?;
    root.verified_work = root.verified_work.checked_add(work).ok_or(PalwStateV2Error::Overflow("exec v2 verified"))?;
    builder.write_exec_root(claim_id, Some(root))?;
    release_final_hold_v2(builder, claim_id, verified_daa)
}
