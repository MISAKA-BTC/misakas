//! Protected paid-seal reservations in the SAME persisted quota book.
//!
//! Native authenticates original paid seal/chain rows and coin transitions.
//! This component does not authenticate caller-provided roots, create reward
//! rights, guarantee arbitrary proofs from two DA requests, or count RPC retries.
use super::*;
use crate::flat_execution11::{ExecutionJobStateV11, ExecutionTerminalV11};

pub const PROTECTED_SEALS_MAX_V11: u32 = 1024;
pub const PROTECTED_SEALS_PER_ACTOR_V11: u32 = 4;
pub const PROTECTED_FRAGMENT_BYTES_V11: u64 = 32 << 10;
pub const PROTECTED_FRAGMENT_ATTEMPTS_PER_PART_V11: u32 = 2;
const FRAGMENT_OUTER_BYTES_V11: u64 = 1024;
/// Map key + full key + five digest bindings + clocks + three 21-axis vectors
/// + three action counters + fixed count/bitmap/attempt/byte progress.
pub const PROTECTED_SEAL_ROW_BYTES_V11: u64 = 64 + 4 * 64 + 8 + 5 * 64 + 2 * 8 + 3 * 21 * 16 + 3 * 4 + 4 + 8 + 64 + 8 + 8 + 4;
pub const PROTECTED_SEALS_PER_JOB_V11: u32 = 4;
pub const PROTECTED_DA_REQUESTS_PER_JOB_MAX_V11: u32 = 2 * PROTECTED_SEALS_PER_JOB_V11;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProtectedSealKeyV11 {
    pub job_id: Digest,
    pub vector_id: Digest,
    pub seal_actor: Digest,
    pub proof_root: Digest,
    pub horizon_daa: u64,
}
impl ProtectedSealKeyV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/quota-protected-seal-key/v11", self)
    }
}
/// A price record derived only from an independently checked job/class plan.
/// Its root is stored on admission; a wire declaration is never authoritative.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProtectedSealPlanV11 {
    pub version: u16,
    pub quota_plan_root: Digest,
    pub assembly_cap: u64,
    pub fragment_count_cap: u32,
    pub fragment_attempts_per_part: u32,
    pub admission: QuotaQuantitiesV11,
    pub fragment: QuotaQuantitiesV11,
    pub court: QuotaQuantitiesV11,
    pub whole_chunk: QuotaQuantitiesV11,
    pub da_request: QuotaQuantitiesV11,
    pub guarantee: QuotaQuantitiesV11,
}
impl ProtectedSealPlanV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/quota-protected-seal-plan/v11", self)
    }
}
pub fn derive_protected_seal_plan_v11(plan: &CheckedJobQuotaPlanV11) -> Result<ProtectedSealPlanV11, String> {
    let assembly_cap = plan.action_carrier_cap(QuotaActionKindV11::Chunk)?;
    let count = assembly_cap.div_ceil(PROTECTED_FRAGMENT_BYTES_V11);
    if count == 0 || count > 64 {
        return Err("protected assembly exceeds fixed fragment bitmap".into());
    }
    let fragment_count_cap = count as u32;
    let bytes = PROTECTED_FRAGMENT_BYTES_V11 + FRAGMENT_OUTER_BYTES_V11;
    let fragment = action_envelope((3 * PROTECTED_FRAGMENT_BYTES_V11 + FRAGMENT_OUTER_BYTES_V11) as u128 * 64, bytes, None)?;
    let court = plan.action(QuotaActionKindV11::Court, &[])?.quantities().clone();
    let whole_chunk = plan.action(QuotaActionKindV11::Chunk, &[])?.quantities().clone();
    let da_request = plan.action(QuotaActionKindV11::DARequest, &[])?.quantities().clone();
    let guarantee = court.checked_mul(2)?.checked_add(&whole_chunk)?.checked_add(&da_request.checked_mul(2)?)?.checked_add(
        &fragment.checked_mul(
            fragment_count_cap.checked_mul(PROTECTED_FRAGMENT_ATTEMPTS_PER_PART_V11).ok_or("protected fragment attempts overflow")?,
        )?,
    )?;
    // Bounded full-book implementation scans at most 1024 fixed actor keys.
    // Native must separately preprice its actual rooted row/aggregate restore;
    // whole-state serialization/hash/clone is NOT concealed by this quotation.
    let admission = action_envelope(
        (16 << 10) + PROTECTED_SEALS_MAX_V11 as u128 * 64,
        PROTECTED_SEAL_ROW_BYTES_V11 + QUOTA_SUBJECT_ROW_BYTES_V11 + QUOTA_ACTOR_ROW_BYTES_V11 + QUOTA_STATE_FIXED_BYTES_V11,
        None,
    )?;
    Ok(ProtectedSealPlanV11 {
        version: 11,
        quota_plan_root: plan.record().root(),
        assembly_cap,
        fragment_count_cap,
        fragment_attempts_per_part: PROTECTED_FRAGMENT_ATTEMPTS_PER_PART_V11,
        admission,
        fragment,
        court,
        whole_chunk,
        da_request,
        guarantee,
    })
}

/// Trusted non-wire view constructed from the native ORIGINAL paid seal and
/// authenticated book/claim. Counts include the proposed insertion and all
/// live legacy6/7/9/new11 seals. Operator IDs come from actual registered actors.
/// No action carrier may manufacture this view from flags, hashes or a deposit.
#[derive(Clone, Debug)]
pub struct NativePaidSealAnchorV11 {
    pub job_id: Digest,
    pub funded_job_root: Digest,
    pub book_root: Digest,
    pub claim_id: Digest,
    pub vector_id: Digest,
    pub seal_actor: Digest,
    pub seal_operator_id: Digest,
    pub producer_operator_id: Digest,
    pub proof_root: Digest,
    pub posted_daa: u64,
    pub matures_daa: u64,
    pub horizon_daa: u64,
    pub original_paid_seal_root: Digest,
    pub original_debit: u64,
    pub original_debit_receipt: Digest,
    pub shared_live_seals: u32,
    pub shared_actor_live_seals: u32,
}
/// Root of the fixed original paid-seal projection stored by native11. Shared
/// live counts are mutable catalog state and are deliberately outside this root.
/// Hash equality checks local consistency; native authenticates the stored root.
pub fn native_paid_seal_record_root_v11(native: &NativePaidSealAnchorV11) -> Digest {
    object_id(
        b"misaka-palw/kernel/native-paid-seal-record/v11",
        &(
            (native.job_id, native.funded_job_root, native.book_root, native.claim_id, native.vector_id),
            (native.seal_actor, native.seal_operator_id, native.producer_operator_id, native.proof_root),
            (native.posted_daa, native.matures_daa, native.horizon_daa, native.original_debit, native.original_debit_receipt),
        ),
    )
}
pub struct CheckedPaidSealV11 {
    binding: SubjectBindingV11,
    key: ProtectedSealKeyV11,
    original_paid_seal_root: Digest,
    seal_operator_id: Digest,
    producer_operator_id: Digest,
    matures_daa: u64,
    posted_daa: u64,
}
impl CheckedPaidSealV11 {
    pub fn key(&self) -> &ProtectedSealKeyV11 {
        &self.key
    }
    pub fn binding(&self) -> &SubjectBindingV11 {
        &self.binding
    }
}
pub fn check_native_paid_seal_v11<C: crate::flat_execution11::ExecutionClassContextV11>(
    class: &C,
    book: &ExecutionJobStateV11,
    epoch: &TrustedEpochV11,
    native: &NativePaidSealAnchorV11,
) -> Result<CheckedPaidSealV11, String> {
    let job = book.job();
    let claim = book.claim().ok_or("protected seal requires the original admitted claim")?;
    if book.terminal() != ExecutionTerminalV11::Challengeable
        || book.settlement().is_some()
        || native.job_id != job.id()
        || native.funded_job_root != job.root()
        || native.book_root != book.root()
        || native.claim_id != claim.id()
        || native.vector_id != claim.vector
        || native.seal_actor == job.producer
        || native.seal_operator_id == native.producer_operator_id
        || native.posted_daa != epoch.block_daa
        || native.posted_daa < claim.committed_daa
        || native.posted_daa > claim.challenge_end
        || native.matures_daa
            != native.posted_daa.checked_add(class.record().horizons.seal_maturity).ok_or("seal maturity overflow")?
        || native.horizon_daa
            != native
                .posted_daa
                .checked_add(class.record().horizons.seal_ttl)
                .and_then(|n| n.checked_add(class.record().horizons.proof_grace))
                .ok_or("seal horizon overflow")?
        || native.horizon_daa > job.deadline_daa
        || epoch.network_interval != job.admitted_interval
        || native.original_paid_seal_root != native_paid_seal_record_root_v11(native)
        || native.original_debit == 0
        || native.original_debit_receipt == [0; 64]
        || native.shared_live_seals == 0
        || native.shared_live_seals > PROTECTED_SEALS_MAX_V11
        || native.shared_actor_live_seals == 0
        || native.shared_actor_live_seals > PROTECTED_SEALS_PER_ACTOR_V11
    {
        return Err("protected reserve lacks exact original paid independent native seal/claim/slots/horizon".into());
    }
    book.check_restored(class, native.funded_job_root)?;
    Ok(CheckedPaidSealV11 {
        binding: job.quota_subject(class.record())?,
        key: ProtectedSealKeyV11 {
            job_id: job.id(),
            vector_id: claim.vector,
            seal_actor: native.seal_actor,
            proof_root: native.proof_root,
            horizon_daa: native.horizon_daa,
        },
        original_paid_seal_root: native.original_paid_seal_root,
        seal_operator_id: native.seal_operator_id,
        producer_operator_id: native.producer_operator_id,
        matures_daa: native.matures_daa,
        posted_daa: native.posted_daa,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FragmentProgressV11 {
    pub declared_count: u32,
    pub accepted_bitmap: u64,
    pub attempts_by_index: [u8; 64],
    pub accepted_bytes: u64,
}
impl Default for FragmentProgressV11 {
    fn default() -> Self {
        Self { declared_count: 0, accepted_bitmap: 0, attempts_by_index: [0; 64], accepted_bytes: 0 }
    }
}
impl FragmentProgressV11 {
    pub(super) fn check(&self, count_cap: u32, assembly_cap: u64) -> Result<(), String> {
        if self.declared_count > count_cap
            || self.declared_count > 64
            || self.accepted_bytes > assembly_cap
            || self.attempts_by_index.iter().any(|n| *n > PROTECTED_FRAGMENT_ATTEMPTS_PER_PART_V11 as u8)
        {
            return Err("protected fragment restored count/attempt/byte refusal".into());
        }
        let mask = self.mask();
        if self.accepted_bitmap & !mask != 0
            || self.attempts_by_index[self.declared_count as usize..].iter().any(|n| *n != 0)
            || (self.accepted_bitmap == 0) != (self.accepted_bytes == 0)
            || self.accepted_bitmap.count_ones() as u64 > self.accepted_bytes
            || self.accepted_bytes > self.accepted_bitmap.count_ones() as u64 * PROTECTED_FRAGMENT_BYTES_V11
            || (0..self.declared_count).any(|i| self.accepted_bitmap & (1u64 << i) != 0 && self.attempts_by_index[i as usize] == 0)
        {
            return Err("protected fragment restored bitmap/accepted bytes mismatch".into());
        }
        Ok(())
    }
    fn mask(&self) -> u64 {
        if self.declared_count == 64 { u64::MAX } else { (1u64 << self.declared_count) - 1 }
    }
    pub fn complete(&self) -> bool {
        self.declared_count != 0 && self.declared_count <= 64 && self.accepted_bitmap == self.mask()
    }
    /// All raw payload/count/sum checks run before a callback or Vec allocation.
    fn preflight(&self, plan: &ProtectedSealPlanV11, index: u32, count: u32, bytes: u64) -> Result<(), String> {
        if count == 0
            || count > plan.fragment_count_cap
            || count > 64
            || index >= count
            || bytes == 0
            || bytes > PROTECTED_FRAGMENT_BYTES_V11
            || self.declared_count != 0 && self.declared_count != count
            || self.accepted_bitmap & (1u64 << index) != 0
            || self.attempts_by_index[index as usize] >= PROTECTED_FRAGMENT_ATTEMPTS_PER_PART_V11 as u8
            || self.accepted_bytes.checked_add(bytes).is_none_or(|n| n > plan.assembly_cap)
        {
            return Err("protected fragment raw count/index/bytes/duplicate/attempt/aggregate refusal".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProtectedSealSummaryV11 {
    pub key: ProtectedSealKeyV11,
    pub subject_binding_root: Digest,
    pub protected_plan_root: Digest,
    pub original_paid_seal_root: Digest,
    pub seal_operator_id: Digest,
    pub producer_operator_id: Digest,
    pub admitted_interval: u64,
    pub matures_daa: u64,
    pub assembly_cap: u64,
    pub fragment_count_cap: u32,
    pub committed: QuotaQuantitiesV11,
    pub consumed: QuotaQuantitiesV11,
    pub guarantee_remaining: QuotaQuantitiesV11,
    /// Court, whole assembled chunk, protected DA request, respectively.
    pub action_attempts_used: [u32; 3],
    pub fragments: FragmentProgressV11,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtectedActionKindV11 {
    Court,
    WholeChunk,
    DARequest,
}
impl ProtectedActionKindV11 {
    fn index(self) -> usize {
        match self {
            Self::Court => 0,
            Self::WholeChunk => 1,
            Self::DARequest => 2,
        }
    }
    fn cap(self) -> u32 {
        match self {
            Self::Court => 2,
            Self::WholeChunk => 1,
            Self::DARequest => 2,
        }
    }
    fn quota_kind(self) -> QuotaActionKindV11 {
        match self {
            Self::Court => QuotaActionKindV11::Court,
            Self::WholeChunk => QuotaActionKindV11::Chunk,
            Self::DARequest => QuotaActionKindV11::DARequest,
        }
    }
    fn quantities(self, plan: &ProtectedSealPlanV11) -> &QuotaQuantitiesV11 {
        match self {
            Self::Court => &plan.court,
            Self::WholeChunk => &plan.whole_chunk,
            Self::DARequest => &plan.da_request,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct ProtectedQuotaReceiptV11 {
    pub epoch: TrustedEpochV11,
    pub seal_key: Digest,
    pub consumed: QuotaQuantitiesV11,
    pub added_guarantee: QuotaQuantitiesV11,
    pub remaining_guarantee: QuotaQuantitiesV11,
}
/// Private token for exactly one prepaid fragment callback. Failed callbacks
/// retain attempt/quantity. A later retry invalidates this old acceptance token.
pub struct ProtectedFragmentDebitV11 {
    receipt: ProtectedQuotaReceiptV11,
    index: u32,
    count: u32,
    bytes: u64,
    attempt: u8,
    after_seal_consumed: QuotaQuantitiesV11,
}
impl ProtectedFragmentDebitV11 {
    pub fn receipt(&self) -> &ProtectedQuotaReceiptV11 {
        &self.receipt
    }
}

impl QuotaStateV11 {
    pub(super) fn check_protected_restored(
        &self,
        policy: &QuotaPolicyV11,
        reserved: &mut QuotaQuantitiesV11,
        actor_reserved: &mut BTreeMap<Digest, QuotaQuantitiesV11>,
        actor_subject_consumed: &mut BTreeMap<Digest, QuotaQuantitiesV11>,
    ) -> Result<(), String> {
        let mut counts: BTreeMap<Digest, u32> = BTreeMap::new();
        let mut job_counts: BTreeMap<Digest, u32> = BTreeMap::new();
        for (key, seal) in &self.protected_seals {
            let subject = self.subjects.get(&seal.key.job_id).ok_or("restored protected seal lost its original subject")?;
            let count = counts.entry(seal.key.seal_actor).or_default();
            *count = count.checked_add(1).ok_or("protected restored actor count overflow")?;
            let job_count = job_counts.entry(seal.key.job_id).or_default();
            *job_count = job_count.checked_add(1).ok_or("protected restored job count overflow")?;
            if *key != seal.key.root()
                || *count > PROTECTED_SEALS_PER_ACTOR_V11
                || *job_count > PROTECTED_SEALS_PER_JOB_V11
                || seal.subject_binding_root != subject.binding.root()
                || seal.admitted_interval != subject.binding.admitted_interval
                || seal.key.seal_actor == subject.binding.producer
                || seal.seal_operator_id == seal.producer_operator_id
                || seal.original_paid_seal_root == [0; 64]
                || seal.key.horizon_daa < seal.matures_daa
                || seal.key.horizon_daa > subject.binding.deadline_daa
                || seal.assembly_cap == 0
                || seal.assembly_cap > (1 << 20) + ACTION_OUTER_ALLOWANCE_V11
                || seal.fragment_count_cap != seal.assembly_cap.div_ceil(PROTECTED_FRAGMENT_BYTES_V11) as u32
                || seal.fragment_count_cap > 64
                || seal.action_attempts_used.iter().zip([2, 1, 2]).any(|(n, cap)| *n > cap)
                || seal.consumed.checked_add(&seal.guarantee_remaining)? != seal.committed
            {
                return Err("restored protected identity/count/operator/horizon/conservation mismatch".into());
            }
            seal.committed.within(&policy.plan)?;
            seal.fragments.check(seal.fragment_count_cap, seal.assembly_cap)?;
            if seal.admitted_interval == self.epoch.network_interval {
                if seal.key.horizon_daa >= self.epoch.interval_end(policy)? {
                    return Err("restored protected liability crosses interval".into());
                }
                *reserved = reserved.checked_add(&seal.guarantee_remaining)?;
                let old = actor_reserved.get(&seal.key.seal_actor).cloned().unwrap_or_default();
                actor_reserved.insert(seal.key.seal_actor, old.checked_add(&seal.guarantee_remaining)?);
                let old = actor_subject_consumed.get(&seal.key.seal_actor).cloned().unwrap_or_default();
                actor_subject_consumed.insert(seal.key.seal_actor, old.checked_add(&seal.consumed)?);
            }
        }
        for (key, subject) in &self.subjects {
            if subject.retained_protected_seals != job_counts.remove(key).unwrap_or_default() {
                return Err("restored protected original job retained count differs".into());
            }
        }
        Ok(())
    }
    pub fn protected_seal(&self, key: &Digest) -> Option<&ProtectedSealSummaryV11> {
        self.protected_seals.get(key)
    }
    pub fn protected_seal_count(&self) -> usize {
        self.protected_seals.len()
    }
    /// The only reservation path uses a private token made from an original
    /// native PAID seal. Reserve before maturity; do not repeat reference MACs.
    pub fn admit_protected_seal(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        checked: &CheckedPaidSealV11,
        job_plan: &CheckedJobQuotaPlanV11,
    ) -> Result<ProtectedQuotaReceiptV11, String> {
        let (new_block, new_interval) = self.check_context(policy, epoch)?;
        let binding = &checked.binding;
        if actor != checked.key.seal_actor
            || checked.posted_daa != epoch.block_daa
            || binding.admitted_interval != epoch.network_interval
            || new_interval
            || checked.key.horizon_daa >= epoch.interval_end(policy)?
        {
            return Err("protected admission differs from paid owner/original interval/clock".into());
        }
        let subject = self.subjects.get(&binding.job_id).ok_or("protected admission requires the original producer baseline")?;
        if subject.binding != *binding
            || subject.closed
            || binding.quota_plan_root != job_plan.record().root()
            || subject.retained_protected_seals >= PROTECTED_SEALS_PER_JOB_V11
        {
            return Err("protected paid subject/plan is not admitted or already terminal".into());
        }
        let key = checked.key.root();
        if self.protected_seals.contains_key(&key)
            || self.protected_seals.len() >= PROTECTED_SEALS_MAX_V11 as usize
            || self.protected_seals.values().filter(|s| s.key.seal_actor == actor).count() >= PROTECTED_SEALS_PER_ACTOR_V11 as usize
        {
            return Err("protected retained global/actor seal count exhausted or duplicate".into());
        }
        let plan = derive_protected_seal_plan_v11(job_plan)?;
        let committed = plan.admission.checked_add(&plan.guarantee)?;
        committed.within(&policy.plan)?;
        let actor_old = self.actors.get(&actor);
        if actor_old.is_none() && self.actors.len() >= policy.max_actors as usize {
            return Err("protected actor count exhausted".into());
        }
        let block_old = if new_block {
            QuotaIntervalSummaryV11 { consumed: QuotaQuantitiesV11::default(), reserved: self.block.reserved.clone() }
        } else {
            self.block.clone()
        };
        let block = block_old.admit(&plan.admission, &plan.guarantee, &policy.block)?;
        let network = self.network.admit(&plan.admission, &plan.guarantee, &policy.network)?;
        let actor_next = actor_old.cloned().unwrap_or_default().admit(&plan.admission, &plan.guarantee, &policy.actor)?;
        let summary = ProtectedSealSummaryV11 {
            key: checked.key.clone(),
            subject_binding_root: binding.root(),
            protected_plan_root: plan.root(),
            original_paid_seal_root: checked.original_paid_seal_root,
            seal_operator_id: checked.seal_operator_id,
            producer_operator_id: checked.producer_operator_id,
            admitted_interval: binding.admitted_interval,
            matures_daa: checked.matures_daa,
            assembly_cap: plan.assembly_cap,
            fragment_count_cap: plan.fragment_count_cap,
            committed,
            consumed: plan.admission.clone(),
            guarantee_remaining: plan.guarantee.clone(),
            action_attempts_used: [0; 3],
            fragments: FragmentProgressV11::default(),
        };
        let receipt = ProtectedQuotaReceiptV11 {
            epoch: epoch.clone(),
            seal_key: key,
            consumed: plan.admission,
            added_guarantee: plan.guarantee.clone(),
            remaining_guarantee: plan.guarantee,
        };
        self.block = block;
        self.network = network;
        self.actors.insert(actor, actor_next);
        self.protected_seals.insert(key, summary);
        self.subjects.get_mut(&binding.job_id).expect("original subject checked").retained_protected_seals += 1;
        self.epoch = epoch.clone();
        Ok(receipt)
    }
    fn check_protected(
        &self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        key: &Digest,
        job_plan: &CheckedJobQuotaPlanV11,
    ) -> Result<ProtectedSealPlanV11, String> {
        self.check_context(policy, epoch)?;
        let seal = self.protected_seals.get(key).ok_or("protected paid seal is absent")?;
        let subject = self.subjects.get(&seal.key.job_id).ok_or("protected paid seal lost its original subject")?;
        let plan = derive_protected_seal_plan_v11(job_plan)?;
        if seal.key.seal_actor != actor
            || seal.subject_binding_root != subject.binding.root()
            || subject.binding.quota_plan_root != job_plan.record().root()
            || seal.protected_plan_root != plan.root()
            || seal.assembly_cap != plan.assembly_cap
            || seal.fragment_count_cap != plan.fragment_count_cap
            || seal.admitted_interval != epoch.network_interval
            || epoch.block_daa < seal.matures_daa
            || epoch.block_daa > seal.key.horizon_daa
            || seal.key.horizon_daa >= epoch.interval_end(policy)?
        {
            return Err("protected action lacks exact mature own paid subject/plan/horizon".into());
        }
        Ok(plan)
    }
    fn consume_protected(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        key: &Digest,
        quantities: &QuotaQuantitiesV11,
        mut seal: ProtectedSealSummaryV11,
    ) -> Result<ProtectedQuotaReceiptV11, String> {
        let (new_block, new_interval) = self.check_context(policy, epoch)?;
        if new_interval {
            return Err("protected action cannot restore an expired interval".into());
        }
        seal.consumed = seal.consumed.checked_add(quantities)?;
        seal.guarantee_remaining = seal.guarantee_remaining.checked_sub(quantities)?;
        if seal.consumed.checked_add(&seal.guarantee_remaining)? != seal.committed {
            return Err("protected conservation mismatch".into());
        }
        let block_old = if new_block {
            QuotaIntervalSummaryV11 { consumed: QuotaQuantitiesV11::default(), reserved: self.block.reserved.clone() }
        } else {
            self.block.clone()
        };
        let block = block_old.consume(quantities, &policy.block)?;
        let network = self.network.consume(quantities, &policy.network)?;
        let actor =
            self.actors.get(&seal.key.seal_actor).ok_or("protected owner aggregate is absent")?.consume(quantities, &policy.actor)?;
        let receipt = ProtectedQuotaReceiptV11 {
            epoch: epoch.clone(),
            seal_key: *key,
            consumed: quantities.clone(),
            added_guarantee: QuotaQuantitiesV11::default(),
            remaining_guarantee: seal.guarantee_remaining.clone(),
        };
        self.block = block;
        self.network = network;
        self.actors.insert(seal.key.seal_actor, actor);
        self.protected_seals.insert(*key, seal);
        self.epoch = epoch.clone();
        Ok(receipt)
    }
    /// Native selects the protected two-slot DA lane before variable decoding;
    /// ordinary 32-slot requests must not occupy it. Actor alone spends its own
    /// reserved allowance. An invalid expensive body retains debit and slot.
    pub fn debit_protected(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        key: &Digest,
        job_plan: &CheckedJobQuotaPlanV11,
        kind: ProtectedActionKindV11,
        carrier: &[u8],
    ) -> Result<ProtectedQuotaReceiptV11, String> {
        let plan = self.check_protected(policy, epoch, actor, key, job_plan)?;
        job_plan.action(kind.quota_kind(), carrier)?;
        let mut seal = self.protected_seals.get(key).expect("checked seal").clone();
        let index = kind.index();
        if seal.action_attempts_used[index] >= kind.cap() {
            return Err("protected action prepaid attempt exhausted".into());
        }
        if kind == ProtectedActionKindV11::WholeChunk && !seal.fragments.complete() {
            return Err("protected assembled finish lacks all admitted fragments".into());
        }
        seal.action_attempts_used[index] += 1;
        self.consume_protected(policy, epoch, key, kind.quantities(&plan), seal)
    }
    /// Check fixed raw header/body/count/aggregate BEFORE allocating any part
    /// group, copying payload, or running a hash/callback. Then persist debit.
    pub fn debit_protected_fragment(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        key: &Digest,
        job_plan: &CheckedJobQuotaPlanV11,
        index: u32,
        count: u32,
        raw_bytes: u64,
        outer_bytes: u64,
    ) -> Result<ProtectedFragmentDebitV11, String> {
        let plan = self.check_protected(policy, epoch, actor, key, job_plan)?;
        if outer_bytes > FRAGMENT_OUTER_BYTES_V11 {
            return Err("protected fragment outer header exceeds fixed cap".into());
        }
        let mut seal = self.protected_seals.get(key).expect("checked seal").clone();
        seal.fragments.preflight(&plan, index, count, raw_bytes)?;
        seal.fragments.declared_count = count;
        seal.fragments.attempts_by_index[index as usize] += 1;
        let attempt = seal.fragments.attempts_by_index[index as usize];
        let receipt = self.consume_protected(policy, epoch, key, &plan.fragment, seal)?;
        let after_seal_consumed = self.protected_seals[key].consumed.clone();
        Ok(ProtectedFragmentDebitV11 { receipt, index, count, bytes: raw_bytes, attempt, after_seal_consumed })
    }
    /// Native calls only after actual authenticated fragment verification and
    /// bounded storage succeed atomically. The private debit is not a wire proof.
    pub fn accept_protected_fragment(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        job_plan: &CheckedJobQuotaPlanV11,
        paid: &ProtectedFragmentDebitV11,
    ) -> Result<(), String> {
        let key = paid.receipt.seal_key;
        let plan = self.check_protected(policy, epoch, actor, &key, job_plan)?;
        let seal = self.protected_seals.get(&key).expect("checked seal");
        if paid.receipt.epoch != *epoch
            || self.epoch != *epoch
            || paid.after_seal_consumed != seal.consumed
            || paid.count != seal.fragments.declared_count
            || paid.attempt != seal.fragments.attempts_by_index[paid.index as usize]
            || seal.fragments.accepted_bitmap & (1u64 << paid.index) != 0
            || seal.fragments.accepted_bytes.checked_add(paid.bytes).is_none_or(|n| n > plan.assembly_cap)
        {
            return Err("protected fragment lacks its exact private prepaid verified callback".into());
        }
        let progress = &mut self.protected_seals.get_mut(&key).expect("checked seal").fragments;
        progress.accepted_bitmap |= 1u64 << paid.index;
        progress.accepted_bytes += paid.bytes;
        Ok(())
    }
    /// One expired earlier-interval seal row, after authenticating the genuine
    /// original terminal book/counter. No current or original allowance refund.
    /// A job close alone cannot collect a still-live protected horizon.
    pub fn collect_expired_protected_seal(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        key: &Digest,
        terminal: &CheckedQuotaTerminalV11,
    ) -> Result<ProtectedQuotaReceiptV11, String> {
        self.check_context(policy, epoch)?;
        let seal = self.protected_seals.get(key).ok_or("protected collection row absent")?;
        if seal.key.job_id != terminal.binding.job_id
            || seal.subject_binding_root != terminal.binding.root()
            || seal.admitted_interval >= epoch.network_interval
            || epoch.block_daa <= seal.key.horizon_daa
        {
            return Err("protected collection lacks exact expired original terminal subject".into());
        }
        let quantities = action_envelope(16 << 10, PROTECTED_SEAL_ROW_BYTES_V11 + 7 * 64, None)?;
        let subject = self.subjects.get(&terminal.binding.job_id).ok_or("protected collection lost its original subject")?;
        if subject.retained_protected_seals == 0 {
            return Err("protected collection original count already absent".into());
        }
        self.debit_unreserved(policy, epoch, actor, &quantities)?;
        self.protected_seals.remove(key);
        self.subjects.get_mut(&terminal.binding.job_id).expect("original subject checked").retained_protected_seals -= 1;
        Ok(ProtectedQuotaReceiptV11 {
            epoch: epoch.clone(),
            seal_key: *key,
            consumed: quantities,
            added_guarantee: QuotaQuantitiesV11::default(),
            remaining_guarantee: QuotaQuantitiesV11::default(),
        })
    }
}
