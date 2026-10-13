//! Paid pre-proof discovery in fixed native rows, explicitly versioned v13.
//!
//! Native authenticates all row roots, original coins, permanent ticket/nonce
//! absence and the actual class/claim. Returned rows must be journaled together.
//! Discovery and legacy proof seals share the SAME v11 global/actor/job counters
//! and 21-axis aggregates. No independent pool or refund creates extra capacity.
//! This bounded entitlement is not a theorem that a chosen request count can
//! locate every fault, a live transport implementation, or an activation fence.
use super::*;
use crate::flat_execution11::{ExecutionClassContextV11, ExecutionJobStateV11, ExecutionTerminalV11};

pub const DISCOVERY_VERSION_V13: u16 = 13;
pub const DISCOVERY_REQUESTS_MAX_V13: u32 = 1 << 20;
// Fixed admission/retirement envelopes include the global, actor, subject and
// discovery rows, their original roots and fixed action framing. No map scan.
const DISCOVERY_JOURNAL_BYTES_V13: u64 = 16 << 10;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DiscoveryPolicyV13 {
    pub version: u16,
    pub quota_policy_root: Digest,
    pub requests_per_session: u32,
    pub ordinary_lifetime_requests: u32,
}
impl DiscoveryPolicyV13 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/discovery-policy/v13", self)
    }
    pub fn check(&self, quota: &QuotaPolicyV11) -> Result<(), String> {
        quota.check()?;
        if self.version != DISCOVERY_VERSION_V13
            || self.quota_policy_root != quota.root()
            || self.requests_per_session == 0
            || self.requests_per_session > DISCOVERY_REQUESTS_MAX_V13
        {
            return Err("unknown, rebound or unbounded discovery policy".into());
        }
        // Prepay producer answers at Fund, including all four shared retained
        // slots. Ordinary exhaustion cannot steal a paid discoverer's delivery.
        let required = self
            .requests_per_session
            .checked_mul(PROTECTED_SEALS_PER_JOB_V11)
            .and_then(|n| n.checked_add(PROTECTED_DA_REQUESTS_PER_JOB_MAX_V11))
            .and_then(|n| n.checked_add(self.ordinary_lifetime_requests))
            .ok_or("discovery producer delivery count overflow")?;
        if quota.future_attempts[2] < required {
            return Err("producer lifetime answers lack discovery and protected delivery reserve".into());
        }
        Ok(())
    }
}

pub struct DiscoveryPlanV13 {
    policy_root: Digest,
    quota_plan_root: Digest,
    requests: u32,
    admission: QuotaQuantitiesV11,
    request: QuotaQuantitiesV11,
    transition: QuotaQuantitiesV11,
    proof: ProtectedSealPlanV11,
    guarantee: QuotaQuantitiesV11,
}
impl DiscoveryPlanV13 {
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/discovery-plan/v13",
            &(
                DISCOVERY_VERSION_V13,
                self.policy_root,
                self.quota_plan_root,
                self.requests,
                &self.admission,
                &self.request,
                &self.transition,
                self.proof.root(),
                &self.guarantee,
            ),
        )
    }
    pub fn requests(&self) -> u32 {
        self.requests
    }
    pub fn guarantee(&self) -> &QuotaQuantitiesV11 {
        &self.guarantee
    }
    pub fn admission(&self) -> &QuotaQuantitiesV11 {
        &self.admission
    }
}
pub fn derive_discovery_plan_v13(
    policy: &DiscoveryPolicyV13,
    quota: &QuotaPolicyV11,
    job: &CheckedJobQuotaPlanV11,
) -> Result<DiscoveryPlanV13, String> {
    policy.check(quota)?;
    let proof = derive_protected_seal_plan_v11(job)?;
    let row = action_envelope(DISCOVERY_JOURNAL_BYTES_V13 as u128 * 64, DISCOVERY_JOURNAL_BYTES_V13, None)?;
    // This frozen request quote covers the selected ticket/journal plus the
    // frozen11 request image. It DOES NOT cover native class/post restoration;
    // native must obtain a separately versioned caller-owned supplemental debit
    // before any such restore or callback. This component cannot authorize Final.
    let request = job.action(QuotaActionKindV11::DARequest, &[])?.quantities().checked_add(&row)?;
    let guarantee = request
        .checked_mul(policy.requests_per_session)?
        .checked_add(&row)?
        .checked_add(&proof.admission)?
        .checked_add(&proof.guarantee)?;
    row.checked_add(&guarantee)?.within(&quota.plan)?;
    Ok(DiscoveryPlanV13 {
        policy_root: policy.root(),
        quota_plan_root: job.record().root(),
        requests: policy.requests_per_session,
        admission: row.clone(),
        request,
        transition: row,
        proof,
        guarantee,
    })
}

/// Non-wire view of an original paid native discovery record. No proof root or
/// coordinate is needed. Native consumes a permanent owner nonce/coin receipt.
#[derive(Clone, Debug)]
pub struct NativePaidDiscoveryV13 {
    pub job_id: Digest,
    pub funded_job_root: Digest,
    pub book_root: Digest,
    pub claim_id: Digest,
    pub vector_id: Digest,
    pub actor: Digest,
    pub actor_operator: Digest,
    pub producer_operator: Digest,
    pub nonce: Digest,
    pub posted_daa: u64,
    pub matures_daa: u64,
    pub horizon_daa: u64,
    pub debit: u64,
    pub debit_receipt: Digest,
    pub original_root: Digest,
}
impl NativePaidDiscoveryV13 {
    pub fn record_root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/native-paid-discovery/v13",
            &(
                (self.job_id, self.funded_job_root, self.book_root, self.claim_id, self.vector_id),
                (self.actor, self.actor_operator, self.producer_operator, self.nonce),
                (self.posted_daa, self.matures_daa, self.horizon_daa, self.debit, self.debit_receipt),
            ),
        )
    }
    pub fn key(&self) -> Digest {
        object_id(b"misaka-palw/kernel/discovery-ticket/v13", &(self.job_id, self.actor, self.nonce))
    }
}
pub struct CheckedDiscoveryAdmissionV13 {
    native: NativePaidDiscoveryV13,
    binding: SubjectBindingV11,
}
pub fn check_native_paid_discovery_v13<C: ExecutionClassContextV11>(
    class: &C,
    book: &ExecutionJobStateV11,
    epoch: &TrustedEpochV11,
    native: &NativePaidDiscoveryV13,
) -> Result<CheckedDiscoveryAdmissionV13, String> {
    let job = book.job();
    let claim = book.claim().ok_or("discovery requires an original admitted claim")?;
    if book.terminal() != ExecutionTerminalV11::Challengeable
        || book.settlement().is_some()
        || native.job_id != job.id()
        || native.funded_job_root != job.root()
        || native.book_root != book.root()
        || native.claim_id != claim.id()
        || native.vector_id != claim.vector
        || native.actor == job.producer
        || native.actor_operator == native.producer_operator
        || native.posted_daa != epoch.block_daa
        || native.posted_daa < claim.committed_daa
        || native.posted_daa > claim.challenge_end
        || native.matures_daa
            != native.posted_daa.checked_add(class.record().horizons.seal_maturity).ok_or("discovery maturity overflow")?
        || native.horizon_daa != claim.challenge_end
        || native.matures_daa > native.horizon_daa
        || native.horizon_daa > job.deadline_daa
        || epoch.network_interval != job.admitted_interval
        || native.debit == 0
        || native.debit_receipt == [0; 64]
        || native.original_root != native.record_root()
    {
        return Err("discovery lacks original paid independent native claim/clock/nonce".into());
    }
    book.check_restored(class, job.root())?;
    Ok(CheckedDiscoveryAdmissionV13 { native: native.clone(), binding: job.quota_subject(class.record())? })
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DiscoveryRowV13 {
    pub version: u16,
    pub key: Digest,
    pub subject_root: Digest,
    pub plan_root: Digest,
    pub original_paid_root: Digest,
    pub vector: Digest,
    pub actor: Digest,
    pub actor_operator: Digest,
    pub producer_operator: Digest,
    pub original_debit: u64,
    pub original_debit_receipt: Digest,
    pub matures_daa: u64,
    pub horizon_daa: u64,
    pub admitted_interval: u64,
    pub attempts_used: u32,
    pub consumed: QuotaQuantitiesV11,
    pub guarantee_remaining: QuotaQuantitiesV11,
    pub committed: QuotaQuantitiesV11,
}
impl DiscoveryRowV13 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/discovery-row/v13", self)
    }
    /// Bound before Borsh allocation. This grammar has no variable collections.
    pub fn from_rooted_bytes(bytes: &[u8], expected: &Digest) -> Result<Self, String> {
        if bytes.len() != 2 + 9 * 64 + 4 * 8 + 4 + 3 * QUOTA_AXES_V11 * 16 {
            return Err("discovery row has wrong fixed extent".into());
        }
        let row: Self = borsh::from_slice(bytes).map_err(|_| "malformed fixed discovery row")?;
        if row.version != DISCOVERY_VERSION_V13
            || row.root() != *expected
            || row.consumed.checked_add(&row.guarantee_remaining)? != row.committed
        {
            return Err("discovery row differs from original authenticated root/conservation".into());
        }
        Ok(row)
    }
}

/// Private fixed-row projection. Expected roots/absence are native table facts,
/// never values copied from a carrier. All legacy reservations remain included.
pub struct DiscoveryShardV13 {
    global: QuotaGlobalRowV11,
    actor: QuotaActorRowV11,
    subject: QuotaSubjectSummaryV11,
    row: Option<DiscoveryRowV13>,
    selected_key: Digest,
    policy_root: Digest,
}
impl DiscoveryShardV13 {
    pub fn from_native_rows(
        policy: &DiscoveryPolicyV13,
        quota: &QuotaPolicyV11,
        plan: &DiscoveryPlanV13,
        global: &QuotaGlobalRowV11,
        global_root: &Digest,
        actor: Digest,
        actor_row: Option<(&QuotaActorRowV11, &Digest)>,
        subject: (&QuotaSubjectSummaryV11, &Digest),
        selected_key: Digest,
        row: Option<(&DiscoveryRowV13, &Digest)>,
    ) -> Result<Self, String> {
        policy.check(quota)?;
        if plan.policy_root != policy.root() || plan.quota_plan_root != subject.0.binding.quota_plan_root {
            return Err("discovery plan differs from original policy/job".into());
        }
        let checked = CheckedQuotaShardV11::from_native_rows(
            quota,
            global,
            global_root,
            actor,
            subject.0.binding.job_id,
            None,
            actor_row,
            Some(subject),
            None,
        )?;
        let actor = checked.actor_row();
        let subject = checked.subject_row().ok_or("discovery lost selected original subject")?.clone();
        if let Some((row, expected)) = row {
            if row.root() != *expected
                || row.version != DISCOVERY_VERSION_V13
                || row.key != selected_key
                || row.actor != actor.actor
                || row.subject_root != subject.binding.root()
                || row.plan_root != plan.root()
                || row.actor_operator == row.producer_operator
                || row.actor == subject.binding.producer
                || row.original_paid_root == [0; 64]
                || row.original_debit == 0
                || row.original_debit_receipt == [0; 64]
                || row.admitted_interval != subject.binding.admitted_interval
                || row.attempts_used > plan.requests
                || row.committed != plan.admission.checked_add(&plan.guarantee)?
                || row.consumed != plan.admission.checked_add(&plan.request.checked_mul(row.attempts_used)?)?
                || row.guarantee_remaining != plan.guarantee.checked_sub(&plan.request.checked_mul(row.attempts_used)?)?
                || row.horizon_daa > subject.binding.deadline_daa
                || row.matures_daa > row.horizon_daa
                || row.consumed.checked_add(&row.guarantee_remaining)? != row.committed
                || global.protected_seal_count == 0
                || actor.retained_protected_seals == 0
                || subject.retained_protected_seals == 0
            {
                return Err("discovery original selected row identity/count/conservation mismatch".into());
            }
            row.committed.within(&quota.plan)?;
            if row.admitted_interval == global.epoch.network_interval {
                row.guarantee_remaining.within(&actor.interval.reserved)?;
                row.guarantee_remaining.checked_add(&subject.guarantee_remaining)?.within(&global.network.reserved)?;
            }
        }
        Ok(Self { global: global.clone(), actor, subject, row: row.map(|x| x.0.clone()), selected_key, policy_root: policy.root() })
    }
    pub fn global_row(&self) -> &QuotaGlobalRowV11 {
        &self.global
    }
    pub fn actor_row(&self) -> &QuotaActorRowV11 {
        &self.actor
    }
    pub fn subject_row(&self) -> &QuotaSubjectSummaryV11 {
        &self.subject
    }
    pub fn discovery_row(&self) -> Option<&DiscoveryRowV13> {
        self.row.as_ref()
    }
    fn context(&self, quota: &QuotaPolicyV11, epoch: &TrustedEpochV11, plan: &DiscoveryPlanV13) -> Result<(), String> {
        quota.check()?;
        epoch.check(quota)?;
        let old = &self.global.epoch;
        if self.global.policy_root != quota.root()
            || self.policy_root != plan.policy_root
            || self.subject.binding.quota_plan_root != plan.quota_plan_root
            || epoch.block_daa < old.block_daa
            || epoch.network_interval != old.network_interval
            || epoch.block_hash == old.block_hash && epoch != old
            || epoch.network_interval != self.subject.binding.admitted_interval
        {
            return Err("discovery context rewinds, rebinds or crosses its original quota interval".into());
        }
        Ok(())
    }
    fn current_block(&self, epoch: &TrustedEpochV11) -> QuotaIntervalSummaryV11 {
        if epoch.block_hash == self.global.epoch.block_hash {
            self.global.block.clone()
        } else {
            QuotaIntervalSummaryV11 { consumed: QuotaQuantitiesV11::default(), reserved: self.global.block.reserved.clone() }
        }
    }
    pub fn admit(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &DiscoveryPlanV13,
        paid: &CheckedDiscoveryAdmissionV13,
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let n = &paid.native;
        if self.row.is_some()
            || self.selected_key != n.key()
            || n.actor != self.actor.actor
            || paid.binding != self.subject.binding
            || self.subject.closed
            || n.posted_daa != epoch.block_daa
            || self.global.protected_seal_count >= PROTECTED_SEALS_MAX_V11
            || self.actor.retained_protected_seals >= PROTECTED_SEALS_PER_ACTOR_V11
            || self.subject.retained_protected_seals >= PROTECTED_SEALS_PER_JOB_V11
            || !self.actor.active && self.global.actor_count >= quota.max_actors
            || n.horizon_daa >= epoch.interval_end(quota)?
        {
            return Err("discovery paid owner/slot/original subject is unavailable".into());
        }
        let committed = plan.admission.checked_add(&plan.guarantee)?;
        let block = self.current_block(epoch).admit(&plan.admission, &plan.guarantee, &quota.block)?;
        let network = self.global.network.admit(&plan.admission, &plan.guarantee, &quota.network)?;
        let actor = self.actor.interval.admit(&plan.admission, &plan.guarantee, &quota.actor)?;
        let row = DiscoveryRowV13 {
            version: DISCOVERY_VERSION_V13,
            key: self.selected_key,
            subject_root: self.subject.binding.root(),
            plan_root: plan.root(),
            original_paid_root: n.original_root,
            vector: n.vector_id,
            actor: n.actor,
            actor_operator: n.actor_operator,
            producer_operator: n.producer_operator,
            original_debit: n.debit,
            original_debit_receipt: n.debit_receipt,
            matures_daa: n.matures_daa,
            horizon_daa: n.horizon_daa,
            admitted_interval: epoch.network_interval,
            attempts_used: 0,
            consumed: plan.admission.clone(),
            guarantee_remaining: plan.guarantee.clone(),
            committed,
        };
        if !self.actor.active {
            self.global.actor_count += 1;
        }
        self.actor.active = true;
        self.actor.interval = actor;
        self.actor.retained_protected_seals += 1;
        self.subject.retained_protected_seals += 1;
        self.global.protected_seal_count += 1;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.row = Some(row);
        Ok(())
    }
    fn spend(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        delta: &QuotaQuantitiesV11,
        mut row: DiscoveryRowV13,
    ) -> Result<(), String> {
        row.consumed = row.consumed.checked_add(delta)?;
        row.guarantee_remaining = row.guarantee_remaining.checked_sub(delta)?;
        let block = self.current_block(epoch).consume(delta, &quota.block)?;
        let network = self.global.network.consume(delta, &quota.network)?;
        let actor = self.actor.interval.consume(delta, &quota.actor)?;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.actor.interval = actor;
        self.row = Some(row);
        Ok(())
    }
    /// Persist this debit BEFORE any body decoding, localization or provider
    /// callback. Invalid expensive attempts retain both their slot and cost.
    pub fn debit_request(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &DiscoveryPlanV13,
        job: &CheckedJobQuotaPlanV11,
        actor: Digest,
        carrier: &[u8],
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let mut row = self.row.clone().ok_or("no paid discovery ticket")?;
        if self.subject.closed
            || actor != row.actor
            || job.record().root() != plan.quota_plan_root
            || epoch.block_daa < row.matures_daa
            || epoch.block_daa > row.horizon_daa
            || row.attempts_used >= plan.requests
        {
            return Err("discovery owner/maturity/horizon/prepaid attempts differ".into());
        }
        job.action(QuotaActionKindV11::DARequest, carrier)?;
        row.attempts_used += 1;
        self.spend(quota, epoch, &plan.request, row)
    }
    /// Transfer the same retained slot to an exact own proof with NEW maturity.
    /// Native authenticates the signed paid seal, its original deposit receipt,
    /// and permanent proof-row absence. No second debit or baseline is minted.
    /// Unused discovery allowance stays spent, never becomes ordinary capacity.
    pub fn convert<C: ExecutionClassContextV11>(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &DiscoveryPlanV13,
        class: &C,
        book: &ExecutionJobStateV11,
        native_proof: &NativePaidSealAnchorV11,
    ) -> Result<ProtectedSealSummaryV11, String> {
        self.context(quota, epoch, plan)?;
        let row = self.row.clone().ok_or("no discovery ticket to convert")?;
        let checked_proof = check_native_paid_seal_v11(class, book, epoch, native_proof)?;
        if self.subject.closed
            || *checked_proof.binding() != self.subject.binding
            || native_proof.seal_actor != row.actor
            || native_proof.vector_id != row.vector
            || native_proof.seal_operator_id != row.actor_operator
            || native_proof.producer_operator_id != row.producer_operator
            || native_proof.original_debit != row.original_debit
            || native_proof.original_debit_receipt != row.original_debit_receipt
            || native_proof.posted_daa != epoch.block_daa
            || epoch.block_daa < row.matures_daa
            || epoch.block_daa > row.horizon_daa
            || native_proof.proof_root == [0; 64]
            || native_proof.shared_live_seals != self.global.protected_seal_count
            || native_proof.shared_actor_live_seals != self.actor.retained_protected_seals
            || checked_proof.key().proof_root != native_proof.proof_root
            || checked_proof.key().seal_actor != row.actor
        {
            return Err("discovery conversion rebinds its owner, claim, funding, proof or clock".into());
        }
        let consumed_delta = row.guarantee_remaining.checked_sub(&plan.proof.guarantee)?;
        // All fallible arithmetic precedes mutation; reserve handoff is atomic.
        let proof = ProtectedSealSummaryV11 {
            key: checked_proof.key().clone(),
            subject_binding_root: row.subject_root,
            protected_plan_root: plan.proof.root(),
            original_paid_seal_root: native_proof.original_paid_seal_root,
            seal_operator_id: row.actor_operator,
            producer_operator_id: row.producer_operator,
            admitted_interval: row.admitted_interval,
            matures_daa: native_proof.matures_daa,
            assembly_cap: plan.proof.assembly_cap,
            fragment_count_cap: plan.proof.fragment_count_cap,
            committed: plan.proof.admission.checked_add(&plan.proof.guarantee)?,
            consumed: plan.proof.admission.clone(),
            guarantee_remaining: plan.proof.guarantee.clone(),
            action_attempts_used: [0; 3],
            fragments: FragmentProgressV11::default(),
        };
        self.spend(quota, epoch, &consumed_delta, row)?;
        self.row = None;
        Ok(proof)
    }
    /// After an independently authenticated native interval rollover, old
    /// reservations are expired. Pay a NEW fixed ordinary journal operation;
    /// never subtract an old guarantee from current reservations or credits.
    pub fn retire_expired(&mut self, quota: &QuotaPolicyV11, epoch: &TrustedEpochV11, plan: &DiscoveryPlanV13) -> Result<(), String> {
        quota.check()?;
        epoch.check(quota)?;
        let row = self.row.as_ref().ok_or("no expired discovery ticket")?;
        if self.global.policy_root != quota.root()
            || self.policy_root != plan.policy_root
            || plan.quota_plan_root != self.subject.binding.quota_plan_root
            || epoch.network_interval != self.global.epoch.network_interval
            || epoch.network_interval <= row.admitted_interval
            || epoch.block_daa < self.global.epoch.block_daa
            || epoch.block_hash == self.global.epoch.block_hash && epoch != &self.global.epoch
            || epoch.block_daa <= row.horizon_daa
            || !self.actor.active && self.global.actor_count >= quota.max_actors
        {
            return Err("expired discovery requires current native rollover and paid collection".into());
        }
        let zero = QuotaQuantitiesV11::default();
        let block = self.current_block(epoch).admit(&plan.transition, &zero, &quota.block)?;
        let network = self.global.network.admit(&plan.transition, &zero, &quota.network)?;
        let actor = self.actor.interval.admit(&plan.transition, &zero, &quota.actor)?;
        if !self.actor.active {
            self.global.actor_count += 1;
        }
        self.actor.active = true;
        self.actor.interval = actor;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.global.protected_seal_count -= 1;
        self.actor.retained_protected_seals -= 1;
        self.subject.retained_protected_seals -= 1;
        self.row = None;
        Ok(())
    }
    /// Retire an unused ticket only after its discovery window. Its entire
    /// reservation becomes consumption in this interval; freeing the retained
    /// catalog slot cannot undo that cost or reuse the original native debit.
    pub fn retire(&mut self, quota: &QuotaPolicyV11, epoch: &TrustedEpochV11, plan: &DiscoveryPlanV13) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let row = self.row.clone().ok_or("no discovery ticket to retire")?;
        if epoch.block_daa <= row.horizon_daa {
            return Err("discovery window has not closed".into());
        }
        self.spend(quota, epoch, &row.guarantee_remaining.clone(), row)?;
        self.global.protected_seal_count -= 1;
        self.actor.retained_protected_seals -= 1;
        self.subject.retained_protected_seals -= 1;
        self.row = None;
        Ok(())
    }
}

/// A fixed, paid fifth-row projection for a third-party expiration. Native must
/// journal every returned row and the exact original ticket deletion together.
/// The original nonce/debit remains a separate durable tombstone.
pub struct DiscoveryPublicRetirementV13 {
    pub global: QuotaGlobalRowV11,
    pub payer: QuotaActorRowV11,
    pub owner: QuotaActorRowV11,
    pub subject: QuotaSubjectSummaryV11,
    pub original_ticket: Digest,
    pub original_paid_root: Digest,
}
impl DiscoveryShardV13 {
    pub fn retire_expired_for(
        &self,
        caller: &CheckedQuotaShardV11,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &DiscoveryPlanV13,
        terminal: &CheckedQuotaTerminalV11,
    ) -> Result<DiscoveryPublicRetirementV13, String> {
        quota.check()?;
        epoch.check(quota)?;
        let row = self.row.as_ref().ok_or("public discovery retirement original row absent")?;
        let original_global = caller.global_row();
        let original_payer = caller.actor_row();
        let original_subject = caller.subject_row().ok_or("public discovery retirement selected subject absent")?;
        if original_global.root() != self.global.root()
            || original_subject.row_root() != self.subject.row_root()
            || !self.subject.closed
            || terminal.binding != self.subject.binding
            || original_payer.actor == row.actor
            || self.actor.actor != row.actor
            || self.policy_root != plan.policy_root
            || plan.quota_plan_root != row_plan_quota(&self.subject)
            || row.plan_root != plan.root()
            || row.subject_root != self.subject.binding.root()
            || epoch.network_interval <= row.admitted_interval
            || epoch.block_daa <= row.horizon_daa
            || epoch.block_daa <= self.subject.binding.deadline_daa
            || row.admitted_interval != self.subject.binding.admitted_interval
            || row.consumed.checked_add(&row.guarantee_remaining)? != row.committed
            || self.global.protected_seal_count == 0
            || self.actor.retained_protected_seals == 0
            || self.subject.retained_protected_seals == 0
        {
            return Err("public discovery retirement lacks exact expired closed original owner/counters".into());
        }
        let bytes = QUOTA_GLOBAL_ROW_BYTES_V11
            + 2 * QUOTA_SELECTED_ACTOR_ROW_BYTES_V11
            + QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11
            + 2048
            + DISCOVERY_JOURNAL_BYTES_V13;
        let quantities = action_envelope(bytes as u128 * 64, bytes, None)?.checked_add(&plan.transition)?;
        if caller.seal_row().is_some() {
            return Err("public discovery payer must select no proof seal".into());
        }
        let mut paid = CheckedQuotaShardV11::from_native_rows(
            quota,
            &original_global,
            &original_global.root(),
            original_payer.actor,
            self.subject.binding.job_id,
            None,
            Some((&original_payer, &original_payer.root())),
            Some((original_subject, &original_subject.row_root())),
            None,
        )?;
        paid.debit_ordinary(quota, epoch, &quantities)?;
        let mut global = paid.global_row();
        let payer = paid.actor_row();
        let mut owner = self.actor.clone();
        let mut subject = self.subject.clone();
        global.protected_seal_count -= 1;
        owner.retained_protected_seals -= 1;
        subject.retained_protected_seals -= 1;
        Ok(DiscoveryPublicRetirementV13 {
            global,
            payer,
            owner,
            subject,
            original_ticket: row.key,
            original_paid_root: row.original_paid_root,
        })
    }
}
fn row_plan_quota(subject: &QuotaSubjectSummaryV11) -> Digest {
    subject.binding.quota_plan_root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_execution11::CheckedExecutionClassV11;

    struct Fixture {
        class: CheckedExecutionClassV11,
        job: CheckedJobQuotaPlanV11,
        quota: QuotaPolicyV11,
        policy: DiscoveryPolicyV13,
        epoch: TrustedEpochV11,
        book: ExecutionJobStateV11,
        proof: NativePaidSealAnchorV11,
        native: NativePaidDiscoveryV13,
        plan: DiscoveryPlanV13,
        shard: DiscoveryShardV13,
    }
    fn fixture() -> Fixture {
        let (class, job, quota, epoch, book, proof, state) =
            super::super::protected_tests::fixture_with_config_and_answers(257, None, None, Some(48));
        let policy = DiscoveryPolicyV13 {
            version: 13,
            quota_policy_root: quota.root(),
            requests_per_session: 2,
            ordinary_lifetime_requests: 32,
        };
        let plan = derive_discovery_plan_v13(&policy, &quota, &job).unwrap();
        let mut native = NativePaidDiscoveryV13 {
            job_id: book.job().id(),
            funded_job_root: book.job().root(),
            book_root: book.root(),
            claim_id: book.claim().unwrap().id(),
            vector_id: proof.vector_id,
            actor: proof.seal_actor,
            actor_operator: proof.seal_operator_id,
            producer_operator: proof.producer_operator_id,
            nonce: [219; 64],
            posted_daa: 10,
            matures_daa: 13,
            horizon_daa: book.claim().unwrap().challenge_end,
            debit: proof.original_debit,
            debit_receipt: proof.original_debit_receipt,
            original_root: [0; 64],
        };
        native.original_root = native.record_root();
        let global = QuotaGlobalRowV11 {
            policy_root: state.policy_root,
            epoch: state.epoch.clone(),
            block: state.block.clone(),
            network: state.network.clone(),
            actor_count: state.actors.len() as u32,
            subject_count: state.subjects.len() as u32,
            protected_seal_count: 0,
        };
        let subject = state.subjects.get(&book.job().id()).unwrap();
        let shard = DiscoveryShardV13::from_native_rows(
            &policy,
            &quota,
            &plan,
            &global,
            &global.root(),
            native.actor,
            None,
            (subject, &subject.row_root()),
            native.key(),
            None,
        )
        .unwrap();
        Fixture { class, job, quota, policy, epoch, book, proof, native, plan, shard }
    }
    fn admit(f: &mut Fixture) {
        let paid = check_native_paid_discovery_v13(&f.class, &f.book, &f.epoch, &f.native).unwrap();
        f.shard.admit(&f.quota, &f.epoch, &f.plan, &paid).unwrap();
    }
    fn now(f: &Fixture, daa: u64) -> TrustedEpochV11 {
        TrustedEpochV11::from_chain(&f.quota, [daa as u8; 64], daa).unwrap()
    }
    fn snapshot(f: &Fixture) -> (Digest, Digest, Digest, Option<Digest>) {
        (
            f.shard.global_row().root(),
            f.shard.actor_row().root(),
            f.shard.subject_row().row_root(),
            f.shard.discovery_row().map(DiscoveryRowV13::root),
        )
    }
    fn restore(f: &Fixture) -> Result<DiscoveryShardV13, String> {
        let g = f.shard.global_row();
        let a = f.shard.actor_row();
        let s = f.shard.subject_row();
        let r = f.shard.discovery_row();
        DiscoveryShardV13::from_native_rows(
            &f.policy,
            &f.quota,
            &f.plan,
            g,
            &g.root(),
            a.actor,
            Some((a, &a.root())),
            (s, &s.row_root()),
            f.native.key(),
            r.map(|r| (r, &r.original_paid_root)),
        )
    }
    #[test]
    fn delivery_reserves_cover_ordinary_all_discovery_and_post_proof_slots() {
        let f = fixture();
        f.policy.check(&f.quota).unwrap();
        assert_eq!(f.plan.requests(), 2);
        for field in 0..5 {
            let mut p = f.policy.clone();
            match field {
                0 => p.version = 11,
                1 => p.quota_policy_root[0] ^= 1,
                2 => p.requests_per_session = 0,
                3 => p.requests_per_session = DISCOVERY_REQUESTS_MAX_V13 + 1,
                _ => p.ordinary_lifetime_requests = u32::MAX,
            }
            assert!(p.check(&f.quota).is_err());
        }
        let mut quota = f.quota.clone();
        quota.future_attempts[2] = 47;
        let p = DiscoveryPolicyV13 { quota_policy_root: quota.root(), ..f.policy.clone() };
        assert!(p.check(&quota).is_err(), "four sessions * two + eight proof demands + 32 ordinary = 48 answers");
    }
    #[test]
    fn paid_discovery_needs_no_proof_coordinate_but_binds_native_original_money_and_subject() {
        let f = fixture();
        check_native_paid_discovery_v13(&f.class, &f.book, &f.epoch, &f.native).unwrap();
        for field in 0..15 {
            let mut n = f.native.clone();
            match field {
                0 => n.job_id[0] ^= 1,
                1 => n.funded_job_root[0] ^= 1,
                2 => n.book_root[0] ^= 1,
                3 => n.claim_id[0] ^= 1,
                4 => n.vector_id[0] ^= 1,
                5 => n.actor = f.book.job().producer,
                6 => n.actor_operator = n.producer_operator,
                7 => n.posted_daa += 1,
                8 => n.matures_daa += 1,
                9 => n.horizon_daa += 1,
                10 => n.debit = 0,
                11 => n.debit_receipt = [0; 64],
                12 => n.original_root = [0; 64],
                13 => n.nonce[0] ^= 1,
                _ => n.producer_operator[0] ^= 1,
            }
            assert!(check_native_paid_discovery_v13(&f.class, &f.book, &f.epoch, &n).is_err(), "field {field}");
        }
    }
    #[test]
    fn fixed_paid_row_roundtrip_binds_all_axes_and_refuses_hostile_frames() {
        let mut f = fixture();
        admit(&mut f);
        let row = f.shard.discovery_row().unwrap();
        let bytes = borsh::to_vec(row).unwrap();
        assert_eq!(DiscoveryRowV13::from_rooted_bytes(&bytes, &row.root()).unwrap(), *row);
        for n in 0..bytes.len() {
            assert!(DiscoveryRowV13::from_rooted_bytes(&bytes[..n], &row.root()).is_err());
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(DiscoveryRowV13::from_rooted_bytes(&extra, &row.root()).is_err());
        let mut rebound = row.clone();
        rebound.guarantee_remaining.0[0] += 1;
        assert!(DiscoveryRowV13::from_rooted_bytes(&borsh::to_vec(&rebound).unwrap(), &rebound.root()).is_err());
        assert!(DiscoveryRowV13::from_rooted_bytes(&bytes, &[0; 64]).is_err());
    }
    #[test]
    fn reserved_search_survives_ordinary_saturation_and_failed_attempts_stay_spent() {
        let mut f = fixture();
        admit(&mut f);
        let before = snapshot(&f);
        let immature = now(&f, 12);
        assert!(f.shard.debit_request(&f.quota, &immature, &f.plan, &f.job, f.native.actor, &[]).is_err());
        assert_eq!(snapshot(&f), before);
        // Consume all unreserved aggregate capacity with unrelated traffic.
        // Reservation remains intact. No boolean success or proof is supplied.
        for scope in [&mut f.shard.global.network, &mut f.shard.global.block, &mut f.shard.actor.interval] {
            scope.consumed = f.quota.network.checked_sub(&scope.reserved).unwrap();
        }
        let mature = now(&f, 13);
        for _ in 0..2 {
            f.shard.debit_request(&f.quota, &mature, &f.plan, &f.job, f.native.actor, &[255]).unwrap();
        }
        let before = snapshot(&f);
        assert!(f.shard.debit_request(&f.quota, &mature, &f.plan, &f.job, f.native.actor, &[]).is_err());
        assert_eq!(snapshot(&f), before);
        assert_eq!(f.shard.discovery_row().unwrap().attempts_used, 2);
        assert_eq!(f.shard.global.network.total().unwrap(), f.quota.network);
    }
    #[test]
    fn actor_role_clock_job_and_oversized_carrier_cannot_spend_reserved_search() {
        let mut f = fixture();
        admit(&mut f);
        let before = snapshot(&f);
        let mature = now(&f, 13);
        assert!(f.shard.debit_request(&f.quota, &mature, &f.plan, &f.job, f.book.job().producer, &[]).is_err());
        assert!(f.shard.debit_request(&f.quota, &now(&f, 9), &f.plan, &f.job, f.native.actor, &[]).is_err());
        assert!(f.shard.debit_request(&f.quota, &now(&f, f.native.horizon_daa + 1), &f.plan, &f.job, f.native.actor, &[]).is_err());
        let huge = vec![0; MAX_DA_REQUEST_BYTES_V11 as usize + 1];
        assert!(f.shard.debit_request(&f.quota, &mature, &f.plan, &f.job, f.native.actor, &huge).is_err());
        assert_eq!(snapshot(&f), before);
    }
    #[test]
    fn conversion_transfers_one_original_slot_new_maturity_and_no_unused_credit() {
        let mut f = fixture();
        admit(&mut f);
        let mature = now(&f, 13);
        f.shard.debit_request(&f.quota, &mature, &f.plan, &f.job, f.native.actor, &[]).unwrap();
        let before = f.shard.global.network.total().unwrap();
        let mut proof = f.proof.clone();
        proof.posted_daa = 13;
        proof.matures_daa = 16;
        proof.horizon_daa = 116;
        proof.original_paid_seal_root = native_paid_seal_record_root_v11(&proof);
        let row = f.shard.convert(&f.quota, &mature, &f.plan, &f.class, &f.book, &proof).unwrap();
        assert_eq!(row.matures_daa, 16, "proof cannot borrow discovery maturity");
        assert_eq!(f.shard.global.network.total().unwrap(), before);
        assert_eq!(
            f.shard.global.network.reserved.checked_sub(&f.shard.subject.guarantee_remaining).unwrap(),
            row.guarantee_remaining
        );
        assert_eq!(f.shard.global.protected_seal_count, 1);
        assert_eq!(f.shard.actor.retained_protected_seals, 1);
        assert_eq!(f.shard.subject.retained_protected_seals, 1);
        assert!(f.shard.discovery_row().is_none());
        let before = snapshot(&f);
        assert!(f.shard.convert(&f.quota, &mature, &f.plan, &f.class, &f.book, &proof).is_err());
        assert!(f.shard.debit_request(&f.quota, &mature, &f.plan, &f.job, f.native.actor, &[]).is_err());
        assert_eq!(snapshot(&f), before);
        // Native handoff restores the generated row under its original root.
        let g = f.shard.global_row();
        let a = f.shard.actor_row();
        let s = f.shard.subject_row();
        let mut next = CheckedQuotaShardV11::from_native_rows(
            &f.quota,
            g,
            &g.root(),
            a.actor,
            s.binding.job_id,
            Some(row.key.root()),
            Some((a, &a.root())),
            Some((s, &s.row_root())),
            Some((&row, &row.row_root())),
        )
        .unwrap();
        let before = next.global_row();
        assert!(next.debit_protected(&f.quota, &mature, &f.job, ProtectedActionKindV11::DARequest, &[]).is_err());
        assert_eq!(next.global_row(), before);
    }
    #[test]
    fn conversion_refuses_foreign_money_claim_operator_or_fresh_proof_without_mutation() {
        let mut f = fixture();
        admit(&mut f);
        let mature = now(&f, 13);
        let before = snapshot(&f);
        for field in 0..7 {
            let mut proof = f.proof.clone();
            proof.posted_daa = 13;
            proof.matures_daa = 16;
            proof.horizon_daa = 116;
            match field {
                0 => proof.original_debit += 1,
                1 => proof.original_debit_receipt[0] ^= 1,
                2 => proof.vector_id[0] ^= 1,
                3 => proof.seal_actor[0] ^= 1,
                4 => proof.seal_operator_id[0] ^= 1,
                5 => proof.proof_root = [0; 64],
                _ => proof.shared_live_seals += 1,
            }
            proof.original_paid_seal_root = native_paid_seal_record_root_v11(&proof);
            assert!(f.shard.convert(&f.quota, &mature, &f.plan, &f.class, &f.book, &proof).is_err(), "field {field}");
            assert_eq!(snapshot(&f), before);
        }
    }
    #[test]
    fn retire_consumes_unused_reserve_once_and_does_not_refund_same_interval_capacity() {
        let mut f = fixture();
        admit(&mut f);
        let total = f.shard.global.network.total().unwrap();
        let before = snapshot(&f);
        assert!(f.shard.retire(&f.quota, &now(&f, f.native.horizon_daa), &f.plan).is_err());
        assert_eq!(snapshot(&f), before);
        f.shard.retire(&f.quota, &now(&f, f.native.horizon_daa + 1), &f.plan).unwrap();
        assert_eq!(f.shard.global.network.total().unwrap(), total);
        assert_eq!(f.shard.global.protected_seal_count, 0);
        assert_eq!(f.shard.actor.retained_protected_seals, 0);
        assert_eq!(f.shard.subject.retained_protected_seals, 0);
        assert_eq!(f.shard.global.network.reserved, f.shard.subject.guarantee_remaining);
        let before = snapshot(&f);
        assert!(f.shard.retire(&f.quota, &now(&f, f.native.horizon_daa + 2), &f.plan).is_err());
        assert_eq!(snapshot(&f), before);
    }
    #[test]
    fn shared_catalog_limits_and_each_axis_cap_refuse_atomically() {
        let mut f = fixture();
        let paid = check_native_paid_discovery_v13(&f.class, &f.book, &f.epoch, &f.native).unwrap();
        for scope in 0..3 {
            match scope {
                0 => f.shard.global.protected_seal_count = PROTECTED_SEALS_MAX_V11,
                1 => f.shard.actor.retained_protected_seals = PROTECTED_SEALS_PER_ACTOR_V11,
                _ => f.shard.subject.retained_protected_seals = PROTECTED_SEALS_PER_JOB_V11,
            }
            let before = snapshot(&f);
            assert!(f.shard.admit(&f.quota, &f.epoch, &f.plan, &paid).is_err());
            assert_eq!(snapshot(&f), before);
            f.shard.global.protected_seal_count = 0;
            f.shard.actor.retained_protected_seals = 0;
            f.shard.subject.retained_protected_seals = 0;
        }
        let original = f.shard.global.network.clone();
        for axis in ResourceAxisV10::ALL {
            if f.plan.admission.checked_add(&f.plan.guarantee).unwrap().get(axis) == 0 {
                continue;
            }
            f.shard.global.network.consumed.0[axis as usize] = f.quota.network.0[axis as usize];
            let before = snapshot(&f);
            assert!(f.shard.admit(&f.quota, &f.epoch, &f.plan, &paid).is_err(), "axis {axis:?}");
            assert_eq!(snapshot(&f), before);
            f.shard.global.network = original.clone();
        }
    }
    #[test]
    fn expired_collection_pays_current_rows_without_restoring_old_credits_or_guarantees() {
        let mut f = fixture();
        admit(&mut f);
        let current = now(&f, f.quota.interval_daa);
        // Native performed and rooted its interval rollover. Old retained rows
        // still exist, but all old interval resource reservations have expired.
        let mut g = f.shard.global_row().clone();
        g.epoch = current.clone();
        g.block = QuotaIntervalSummaryV11::default();
        g.network = QuotaIntervalSummaryV11::default();
        g.actor_count = 0;
        let a = f.shard.actor_row();
        let subject = f.shard.subject_row();
        let row = f.shard.discovery_row().unwrap();
        let mut next = DiscoveryShardV13::from_native_rows(
            &f.policy,
            &f.quota,
            &f.plan,
            &g,
            &g.root(),
            a.actor,
            Some((a, &a.root())),
            (subject, &subject.row_root()),
            row.key,
            Some((row, &row.root())),
        )
        .unwrap();
        assert!(next.retire(&f.quota, &current, &f.plan).is_err(), "expired reserve cannot be spent in a new interval");
        next.retire_expired(&f.quota, &current, &f.plan).unwrap();
        assert_eq!(next.global.network.consumed, f.plan.transition);
        assert_eq!(next.global.network.reserved, QuotaQuantitiesV11::default());
        assert_eq!(next.actor.interval.consumed, f.plan.transition);
        assert_eq!(next.global.protected_seal_count, 0);
        assert_eq!(next.global.actor_count, 1);
        assert!(next.retire_expired(&f.quota, &current, &f.plan).is_err());
        // Collection is not free even when the previous interval was paid.
        let mut full = g.clone();
        full.network.consumed = f.quota.network.clone();
        let mut next = DiscoveryShardV13::from_native_rows(
            &f.policy,
            &f.quota,
            &f.plan,
            &full,
            &full.root(),
            a.actor,
            Some((a, &a.root())),
            (subject, &subject.row_root()),
            row.key,
            Some((row, &row.root())),
        )
        .unwrap();
        let before = next.global_row().clone();
        assert!(next.retire_expired(&f.quota, &current, &f.plan).is_err());
        assert_eq!(*next.global_row(), before);
        assert_eq!(next.discovery_row(), Some(row));
    }
    #[test]
    fn restoration_rechecks_exact_attempt_price_and_live_terminal_guard() {
        let mut f = fixture();
        admit(&mut f);
        let g = f.shard.global_row();
        let a = f.shard.actor_row();
        let subject = f.shard.subject_row();
        let mut row = f.shard.discovery_row().unwrap().clone();
        row.attempts_used = 1;
        assert!(
            DiscoveryShardV13::from_native_rows(
                &f.policy,
                &f.quota,
                &f.plan,
                g,
                &g.root(),
                a.actor,
                Some((a, &a.root())),
                (subject, &subject.row_root()),
                row.key,
                Some((&row, &row.root()))
            )
            .is_err(),
            "a rooted count must pay its exact original per-attempt envelope"
        );
        f.shard.subject.closed = true;
        let before = snapshot(&f);
        assert!(f.shard.debit_request(&f.quota, &now(&f, 13), &f.plan, &f.job, f.native.actor, &[]).is_err());
        assert_eq!(snapshot(&f), before);
    }
    #[test]
    fn block_identity_cannot_rebind_but_distinct_dag_blocks_can_share_a_daa_score() {
        let mut f = fixture();
        admit(&mut f);
        let mature = now(&f, 13);
        f.shard.debit_request(&f.quota, &mature, &f.plan, &f.job, f.native.actor, &[]).unwrap();
        let before = snapshot(&f);
        let rebound = TrustedEpochV11::from_chain(&f.quota, mature.block_hash, 14).unwrap();
        assert!(f.shard.debit_request(&f.quota, &rebound, &f.plan, &f.job, f.native.actor, &[]).is_err());
        assert_eq!(snapshot(&f), before);
        let another = TrustedEpochV11::from_chain(&f.quota, [220; 64], 13).unwrap();
        f.shard.debit_request(&f.quota, &another, &f.plan, &f.job, f.native.actor, &[]).unwrap();
        assert_eq!(f.shard.discovery_row().unwrap().attempts_used, 2);
        assert_eq!(f.shard.global.epoch, another);
    }
    #[test]
    fn selected_root_rebinding_cannot_restore_original_discovery() {
        let mut f = fixture();
        admit(&mut f);
        assert!(restore(&f).is_err(), "a paid-record root is not the authenticated selected-row root");
        let g = f.shard.global_row();
        let a = f.shard.actor_row();
        let s = f.shard.subject_row();
        let r = f.shard.discovery_row().unwrap();
        let next = DiscoveryShardV13::from_native_rows(
            &f.policy,
            &f.quota,
            &f.plan,
            g,
            &g.root(),
            a.actor,
            Some((a, &a.root())),
            (s, &s.row_root()),
            r.key,
            Some((r, &r.root())),
        )
        .unwrap();
        assert_eq!(next.discovery_row(), Some(r));
        assert!(
            DiscoveryShardV13::from_native_rows(
                &f.policy,
                &f.quota,
                &f.plan,
                g,
                &g.root(),
                a.actor,
                Some((a, &a.root())),
                (s, &s.row_root()),
                [0; 64],
                Some((r, &r.root()))
            )
            .is_err()
        );
    }
}
