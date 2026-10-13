//! Additional bound-PC delivery guarantees over the same quota11 aggregates.
//! Frozen subject11 reservations and attempt meanings are never enlarged.
//! Native authenticates original membership, producer consent and valid pending
//! units, prepays fixed row restoration, and persists every expensive failed
//! answer debit before decoding. External R absence is never a producer default.
//! This component alone supplies neither public PC delivery nor Final authority.
pub mod opening;
use super::*;
use crate::flat_execution11::{CheckedNativeExecutionClassV11, ExecutionJobStateV11, ExecutionTerminalV11};
use crate::merkle::{AXIS_COL, AXIS_ROW};
use crate::merkle3::LayoutV3;

pub const SOURCE_VERSION_V16: u16 = 16;
pub const SOURCE_ANSWER_MAX_BYTES_V16: u64 = 128 << 10;
pub const SOURCE_JOURNAL_BYTES_V16: u64 = 16 << 10;
pub const SOURCE_ROW_BYTES_V16: u64 = 2 + 10 * 64 + 2 * 8 + 3 * 4 + 1 + 3 * 21 * 16;
const ANSWER_ATTEMPTS_PER_UNIT_V16: u32 = 2;

#[derive(Clone, Debug)]
pub struct SourcePlanV16 {
    class: Digest,
    parameter: Digest,
    inventory: Digest,
    quota: Digest,
    job_plan: Digest,
    requests: u32,
    admission: QuotaQuantitiesV11,
    answer: QuotaQuantitiesV11,
    guarantee: QuotaQuantitiesV11,
}
/// Raw fixed documentary record. It grants no capacity; its expected root must
/// come from original native consent18/obligation membership, never a carrier.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourcePlanRecordV16 {
    pub version:u16, pub class:Digest, pub parameter:Digest, pub inventory:Digest,
    pub quota:Digest, pub job_plan:Digest, pub requests:u32,
    pub admission:QuotaQuantitiesV11, pub answer:QuotaQuantitiesV11, pub guarantee:QuotaQuantitiesV11,
}
pub const SOURCE_PLAN_RECORD_BYTES_V16:u64 = 2+5*64+4+3*21*16;
impl SourcePlanRecordV16 {
    pub fn root(&self)->Digest { object_id(b"misaka-palw/kernel/bound-pc-source-plan/v16",self) }
    pub fn from_rooted_bytes(bytes:&[u8],expected:&Digest)->Result<Self,String>{
        let r:Self=sharded::read_fixed_row(bytes,SOURCE_PLAN_RECORD_BYTES_V16)?;
        if r.root()!=*expected{return Err("source quote differs from native original consent root".into());}
        Ok(r)
    }
}
impl SourcePlanV16 {
    pub fn record(&self)->SourcePlanRecordV16 {
        SourcePlanRecordV16 {version:SOURCE_VERSION_V16,class:self.class,parameter:self.parameter,inventory:self.inventory,
            quota:self.quota,job_plan:self.job_plan,requests:self.requests,admission:self.admission.clone(),answer:self.answer.clone(),guarantee:self.guarantee.clone()}
    }
    /// Fixed native cache restore only. Admission independently derives and
    /// compares this root before producer signs consent18 and any coins move.
    pub fn restore(record:&SourcePlanRecordV16,expected:&Digest,quota:&QuotaPolicyV11,job_plan:&Digest)->Result<Self,String>{
        quota.check()?;
        if record.version!=SOURCE_VERSION_V16 || record.root()!=*expected || record.quota!=quota.root()
            || record.job_plan!=*job_plan || record.requests==0 || record.requests>1<<20
            || record.class==[0;64] || record.parameter==[0;64] || record.inventory==[0;64]
            || record.guarantee!=record.answer.checked_mul(record.requests.checked_mul(ANSWER_ATTEMPTS_PER_UNIT_V16).ok_or("source cached attempt overflow")?)?
        {return Err("source cached quote differs from authenticated original contract".into());}
        record.admission.checked_add(&record.guarantee)?.within(&quota.plan)?;
        Ok(Self {class:record.class,parameter:record.parameter,inventory:record.inventory,quota:record.quota,job_plan:record.job_plan,
            requests:record.requests,admission:record.admission.clone(),answer:record.answer.clone(),guarantee:record.guarantee.clone()})
    }
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/bound-pc-source-plan/v16",
            &(
                SOURCE_VERSION_V16,
                self.class,
                self.parameter,
                self.inventory,
                self.quota,
                self.job_plan,
                self.requests,
                &self.admission,
                &self.answer,
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
    pub fn answer(&self) -> &QuotaQuantitiesV11 {
        &self.answer
    }
    pub fn admission(&self) -> &QuotaQuantitiesV11 {
        &self.admission
    }
}
fn ceil_log2(n: u64) -> u32 {
    if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() }
}
/// Each protected owner can identify a mismatching PC tensor from the public
/// instance map, acquire an authenticated header, bisect row/column leaf hashes,
/// and fetch its exact original v3 leaf. Operand acquisitions are separately
/// bounded by the complete original scope, not guessed as one reply round.
pub fn derive_source_plan_v16(
    class: &CheckedNativeExecutionClassV11,
    job: &CheckedJobQuotaPlanV11,
    quota: &QuotaPolicyV11,
    discovery: &discovery13::DiscoveryPolicyV13,
) -> Result<SourcePlanV16, String> {
    discovery.check(quota)?;
    let record = job.record();
    let c = class.record();
    if c.program_root != record.program_root
        || c.parameter_root != record.parameter_root
        || c.class_scope9_root != record.class_scope_root
        || c.quota_policy_root != quota.root()
        || record.positions == 0
        || record.positions > c.max_positions
    {
        return Err("source quote changes original class/job/scope/parameters".into());
    }
    let mut max_leaves = 1u64;
    for p in &class.view().program.params {
        let shape: Vec<_> = p.shape.iter().map(|d| *d as usize).collect();
        let layout = LayoutV3::try_of(&shape).ok_or("source parameter layout overflow")?;
        max_leaves = max_leaves.max(layout.leaves(AXIS_ROW)).max(layout.leaves(AXIS_COL));
    }
    let operand_leaves = class.view().scope.relations.iter().try_fold(0u64, |n, r| {
        let count = r.operand_leaves.iter().try_fold(0u64, |n, v| n.checked_add(*v).ok_or("source operand count overflow"))?;
        Ok::<_, String>(n.max(count))
    })?;
    let each = u64::from(ceil_log2(max_leaves)).checked_add(3).ok_or("source locator count overflow")?.max(operand_leaves);
    let requests = each
        .checked_mul(PROTECTED_SEALS_PER_JOB_V11 as u64)
        .and_then(|n| n.checked_add(discovery.ordinary_lifetime_requests as u64))
        .filter(|n| *n > 0 && *n <= 1 << 20)
        .ok_or("source delivery count exceeds bounded policy")? as u32;
    derive_source_plan_base_v16(c.id(), c.parameter_root, c.inventory_root, class.parameters(), job, quota, requests)
}
// Only checked versioned adapters inside this module's parent may use this
// constructor. Stage14 must authenticate real PC/source separately from inputs.
pub(super) fn derive_source_plan_base_v16(
    class: Digest,
    parameter: Digest,
    inventory: Digest,
    pc: &ParamCommitmentsV1,
    job: &CheckedJobQuotaPlanV11,
    quota: &QuotaPolicyV11,
    requests: u32,
) -> Result<SourcePlanV16, String> {
    quota.check()?;
    if requests == 0 || requests > 1 << 20 || pc.root() != parameter {
        return Err("source delivery request bound".into());
    }
    let record = job.record();
    let admission = action_envelope(SOURCE_JOURNAL_BYTES_V16 as u128 * 64, SOURCE_JOURNAL_BYTES_V16, None)?;
    // The original complete native restore tariff is retained even though
    // the new PC leaf wire allowance is larger than frozen16-KiB DAAnswer11.
    let work =
        native_action_metadata_v11(record.native_restore_work, record.native_post_carrier_cap, SOURCE_ANSWER_MAX_BYTES_V16, false)?;
    let mut answer = action_envelope(work, SOURCE_ANSWER_MAX_BYTES_V16, None)?.checked_add(&admission)?;
    // Independently bound native metadata residency/copies as well as abstract
    // derivation work. A work-only MetadataWork entry is not a RAM allowance.
    // Conservatively one owned byte per original checked restore work unit,
    // plus four bounded post images; this intentionally may refuse capacity.
    let restore_bytes = record.native_restore_work
        .checked_add(record.native_post_carrier_cap.checked_mul(4).ok_or("source restore post overflow")?)
        .and_then(|n| n.checked_add(SOURCE_JOURNAL_BYTES_V16)).ok_or("source restore bytes overflow")?;
    answer = answer.checked_add(&action_envelope(restore_bytes as u128 * 64, restore_bytes, None)?)?;
    include_native_post_io_v11(&mut answer, record.native_post_carrier_cap)?;
    include_source_court_io_v11(&mut answer, pc)?;
    // A complete 128-KiB answer can use four original 32-KiB fragments.
    // Charge ALL bounded fragment codecs/copies/journals in addition to final
    // whole assembly, even when the native sender uses fewer or is invalid.
    let fragment_bytes = PROTECTED_FRAGMENT_BYTES_V11 + (1<<10) + SOURCE_JOURNAL_BYTES_V16;
    let fragment = action_envelope(fragment_bytes as u128 * 64, fragment_bytes, None)?;
    answer = answer.checked_add(&fragment.checked_mul(4)?)?;
    let attempts = requests.checked_mul(ANSWER_ATTEMPTS_PER_UNIT_V16).ok_or("source answer attempts overflow")?;
    let guarantee = answer.checked_mul(attempts)?;
    admission.checked_add(&guarantee)?.within(&quota.plan)?;
    Ok(SourcePlanV16 {
        class,
        parameter,
        inventory,
        quota: quota.root(),
        job_plan: record.root(),
        requests,
        admission,
        answer,
        guarantee,
    })
}

/// Non-wire original producer-signed acceptance, authenticated by native from
/// its immutable consent journal before any requester may reserve the producer.
pub struct NativeSourceConsentV16 {
    pub job_id: Digest,
    pub funded_job_root: Digest,
    pub book_root: Digest,
    pub producer: Digest,
    pub producer_operator: Digest,
    pub original_consent_root: Digest,
    pub original_source_plan_root: Digest,
    pub original_trace_plan_root: Digest,
    pub accepted_daa: u64,
    pub expires_daa: u64,
}
pub struct CheckedSourceAdmissionV16 {
    binding: SubjectBindingV11,
    consent: Digest,
    producer_operator: Digest,
    epoch: TrustedEpochV11,
}
// A stage-only bridge accepts a private checked token from its versioned
// adapter. No public constructor accepts raw roots, counts or wire flags.
pub(super) fn from_checked_stage_admission_v16(
    checked: stage14::CheckedStageSourceGrantV14,
    plan: &SourcePlanV16,
) -> Result<CheckedSourceAdmissionV16, String> {
    if checked.source_plan != plan.root()
        || checked.binding.class_id != plan.class
        || checked.binding.quota_plan_root != plan.job_plan
        || checked.binding.policy_root != plan.quota
        || checked.consent == [0; 64]
        || checked.producer_operator == [0; 64]
        || checked.binding.admitted_interval != checked.epoch.network_interval
    {
        return Err("stage source bridge lacks original checked plan/consent/epoch".into());
    }
    Ok(CheckedSourceAdmissionV16 { binding: checked.binding, consent: checked.consent,
        producer_operator: checked.producer_operator, epoch: checked.epoch })
}
pub fn check_native_source_admission_v16(
    class: &CheckedNativeExecutionClassV11,
    book: &ExecutionJobStateV11,
    epoch: &TrustedEpochV11,
    plan: &SourcePlanV16,
    native: &NativeSourceConsentV16,
) -> Result<CheckedSourceAdmissionV16, String> {
    let j = book.job();
    if book.terminal() != ExecutionTerminalV11::Funded
        || book.settlement().is_some()
        || plan.class != j.class_id
        || plan.class != class.record().id()
        || plan.parameter != class.record().parameter_root
        || plan.inventory != class.record().inventory_root
        || plan.job_plan != j.quota_plan_root
        || plan.quota != class.record().quota_policy_root
        || native.job_id != j.id()
        || native.funded_job_root != j.root()
        || native.book_root != book.root()
        || native.producer != j.producer
        || native.producer_operator == [0; 64]
        || native.original_consent_root == [0; 64]
        || native.original_source_plan_root != plan.root()
        || native.original_trace_plan_root == [0;64]
        || native.accepted_daa > epoch.block_daa
        || native.expires_daa < epoch.block_daa
        || epoch.block_daa != j.admitted_daa
        || epoch.network_interval != j.admitted_interval
    {
        return Err("source obligation lacks exact original funded job and signed producer consent".into());
    }
    Ok(CheckedSourceAdmissionV16 {
        binding: j.quota_subject(class.record())?,
        consent: native.original_consent_root,
        producer_operator: native.producer_operator,
        epoch: epoch.clone(),
    })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceObligationV16 {
    pub version: u16,
    pub job_id: Digest,
    pub subject_root: Digest,
    pub plan_root: Digest,
    pub class: Digest,
    pub parameter: Digest,
    pub inventory: Digest,
    pub producer: Digest,
    pub producer_operator: Digest,
    pub original_consent_root: Digest,
    pub quota_plan_root: Digest,
    pub admitted_interval: u64,
    pub deadline_daa: u64,
    pub requests_cap: u32,
    pub requests_admitted: u32,
    pub answers_used: u32,
    pub retired: bool,
    pub committed: QuotaQuantitiesV11,
    pub consumed: QuotaQuantitiesV11,
    pub guarantee_remaining: QuotaQuantitiesV11,
}
impl SourceObligationV16 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/bound-pc-source-obligation/v16", self)
    }
    pub fn from_rooted_bytes(bytes: &[u8], expected: &Digest) -> Result<Self, String> {
        let row: Self = sharded::read_fixed_row(bytes, SOURCE_ROW_BYTES_V16)?;
        if row.root() != *expected {
            return Err("source obligation differs from native authenticated root".into());
        }
        Ok(row)
    }
}
pub struct SourceShardV16 {
    global: QuotaGlobalRowV11,
    producer: QuotaActorRowV11,
    subject: QuotaSubjectSummaryV11,
    row: Option<SourceObligationV16>,
}
impl SourceShardV16 {
    pub fn from_native_rows(
        quota: &QuotaPolicyV11,
        plan: &SourcePlanV16,
        global: &QuotaGlobalRowV11,
        global_root: &Digest,
        producer: (&QuotaActorRowV11, &Digest),
        subject: (&QuotaSubjectSummaryV11, &Digest),
        row: Option<(&SourceObligationV16, &Digest)>,
    ) -> Result<Self, String> {
        if plan.quota != quota.root()
            || subject.0.binding.quota_plan_root != plan.job_plan
            || subject.0.binding.producer != producer.0.actor
        {
            return Err("source shard original plan/producer mismatch".into());
        }
        let shard = CheckedQuotaShardV11::from_native_rows(
            quota,
            global,
            global_root,
            producer.0.actor,
            subject.0.binding.job_id,
            None,
            Some(producer),
            Some(subject),
            None,
        )?;
        let producer = shard.actor_row();
        let subject = shard.subject_row().ok_or("source subject absent")?.clone();
        if let Some((r, root)) = row {
            let spent = plan.answer.checked_mul(r.answers_used)?;
            if r.root() != *root
                || r.version != SOURCE_VERSION_V16
                || r.job_id != subject.binding.job_id
                || r.subject_root != subject.binding.root()
                || r.plan_root != plan.root()
                || r.class != plan.class
                || r.parameter != plan.parameter
                || r.inventory != plan.inventory
                || r.producer != producer.actor
                || r.producer_operator == [0; 64]
                || r.original_consent_root == [0; 64]
                || r.quota_plan_root != plan.job_plan
                || r.admitted_interval != subject.binding.admitted_interval
                || r.deadline_daa != subject.binding.deadline_daa
                || r.requests_cap != plan.requests
                || r.requests_admitted > r.requests_cap
                || r.answers_used
                    > r.requests_admitted.checked_mul(ANSWER_ATTEMPTS_PER_UNIT_V16).ok_or("source attempt count overflow")?
                || r.committed != plan.admission.checked_add(&plan.guarantee)?
                || r.consumed.checked_add(&r.guarantee_remaining)? != r.committed
                || (!r.retired
                    && (r.consumed != plan.admission.checked_add(&spent)?
                        || r.guarantee_remaining != plan.guarantee.checked_sub(&spent)?))
                || (r.retired && (r.guarantee_remaining != QuotaQuantitiesV11::default() || r.consumed != r.committed))
            {
                return Err("source original row identity/attempt/conservation mismatch".into());
            }
            r.committed.within(&quota.plan)?;
            if r.admitted_interval == global.epoch.network_interval {
                r.guarantee_remaining.checked_add(&subject.guarantee_remaining)?.within(&producer.interval.reserved)?;
                r.guarantee_remaining.checked_add(&subject.guarantee_remaining)?.within(&global.network.reserved)?;
            }
        }
        Ok(Self { global: global.clone(), producer, subject, row: row.map(|(r, _)| r.clone()) })
    }
    pub fn global_row(&self) -> &QuotaGlobalRowV11 {
        &self.global
    }
    pub fn producer_row(&self) -> &QuotaActorRowV11 {
        &self.producer
    }
    pub fn source_row(&self) -> Option<&SourceObligationV16> {
        self.row.as_ref()
    }
    fn context(&self, quota: &QuotaPolicyV11, epoch: &TrustedEpochV11, plan: &SourcePlanV16) -> Result<(), String> {
        quota.check()?;
        epoch.check(quota)?;
        let old = &self.global.epoch;
        if self.global.policy_root != plan.quota
            || plan.quota != quota.root()
            || plan.job_plan != self.subject.binding.quota_plan_root
            || epoch.block_daa < old.block_daa
            || epoch.network_interval != old.network_interval
            || epoch.network_interval != self.subject.binding.admitted_interval
            || epoch.block_hash == old.block_hash && epoch != old
        {
            return Err("source context rebinds/rewinds/crosses original interval".into());
        }
        Ok(())
    }
    fn block(&self, epoch: &TrustedEpochV11) -> QuotaIntervalSummaryV11 {
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
        plan: &SourcePlanV16,
        checked: &CheckedSourceAdmissionV16,
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        if self.row.is_some()
            || self.subject.closed
            || checked.binding != self.subject.binding
            || *epoch != checked.epoch
            || !self.producer.active
            || self.producer.network_interval != epoch.network_interval
        {
            return Err("source original funded admission/producer unavailable".into());
        }
        let block = self.block(epoch).admit(&plan.admission, &plan.guarantee, &quota.block)?;
        let network = self.global.network.admit(&plan.admission, &plan.guarantee, &quota.network)?;
        let actor = self.producer.interval.admit(&plan.admission, &plan.guarantee, &quota.actor)?;
        let row = SourceObligationV16 {
            version: SOURCE_VERSION_V16,
            job_id: self.subject.binding.job_id,
            subject_root: self.subject.binding.root(),
            plan_root: plan.root(),
            class: plan.class,
            parameter: plan.parameter,
            inventory: plan.inventory,
            producer: self.producer.actor,
            producer_operator: checked.producer_operator,
            original_consent_root: checked.consent,
            quota_plan_root: plan.job_plan,
            admitted_interval: epoch.network_interval,
            deadline_daa: self.subject.binding.deadline_daa,
            requests_cap: plan.requests,
            requests_admitted: 0,
            answers_used: 0,
            retired: false,
            committed: plan.admission.checked_add(&plan.guarantee)?,
            consumed: plan.admission.clone(),
            guarantee_remaining: plan.guarantee.clone(),
        };
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.producer.interval = actor;
        self.row = Some(row);
        Ok(())
    }
    /// Native first authenticates a fresh valid original PC unit and its paid
    /// requester/ticket debit. Invalid, duplicate or substituted requests cannot
    /// call this transition or steal the producer's answer reservation.
    pub fn admit_valid_request(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &SourcePlanV16,
        original_job: &Digest,
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let row = self.row.as_mut().ok_or("source obligation absent")?;
        if self.subject.closed
            || row.retired
            || row.job_id != *original_job
            || epoch.block_daa > row.deadline_daa
            || row.requests_admitted >= plan.requests
        {
            return Err("source valid request exceeds original producer obligation".into());
        }
        row.requests_admitted += 1;
        Ok(())
    }
    /// Debit BEFORE assembling/decoding/verifying an original producer answer.
    /// Native also limits attempts per immutable pending unit to two, persists
    /// failed attempts, and authenticates producer signature/consent and payload.
    pub fn debit_answer(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &SourcePlanV16,
        producer: &Digest,
        carrier_bytes: u64,
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let mut row = self.row.clone().ok_or("source obligation absent")?;
        if self.subject.closed
            || row.retired
            || row.producer != *producer
            || epoch.block_daa > row.deadline_daa
            || carrier_bytes > SOURCE_ANSWER_MAX_BYTES_V16
            || row.answers_used >= row.requests_admitted.checked_mul(ANSWER_ATTEMPTS_PER_UNIT_V16).ok_or("source attempts overflow")?
        {
            return Err("source answer lacks original owner/pending prepaid attempt".into());
        }
        let block = self.block(epoch).consume(&plan.answer, &quota.block)?;
        let network = self.global.network.consume(&plan.answer, &quota.network)?;
        let actor = self.producer.interval.consume(&plan.answer, &quota.actor)?;
        row.answers_used += 1;
        row.consumed = row.consumed.checked_add(&plan.answer)?;
        row.guarantee_remaining = row.guarantee_remaining.checked_sub(&plan.answer)?;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.producer.interval = actor;
        self.row = Some(row);
        Ok(())
    }
    /// After genuine terminal close and ORIGINAL deadline, burn the unused
    /// guarantee into consumption. No slots, attempts or ordinary credit revive.
    /// Native must authenticate terminal membership and persist this row before
    /// allowing collection of the original subject/job.
    pub fn retire(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &SourcePlanV16,
        terminal: &CheckedQuotaTerminalV11,
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let mut row = self.row.clone().ok_or("source retirement absent")?;
        if !self.subject.closed || row.retired || terminal.binding != self.subject.binding || epoch.block_daa <= row.deadline_daa {
            return Err("source retirement lacks genuine original expired terminal".into());
        }
        let delta = row.guarantee_remaining.clone();
        let block = self.block(epoch).consume(&delta, &quota.block)?;
        let network = self.global.network.consume(&delta, &quota.network)?;
        let actor = self.producer.interval.consume(&delta, &quota.actor)?;
        row.retired = true;
        row.consumed = row.committed.clone();
        row.guarantee_remaining = QuotaQuantitiesV11::default();
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.producer.interval = actor;
        self.row = Some(row);
        Ok(())
    }
}

/// Public retirement of one old-interval obligation, authenticated from native
/// ORIGINAL source/terminal rows. Its guarantees have already expired; only a
/// new current ordinary fixed-row collection is charged to the actual caller.
/// Native persists the returned tombstone before allowing original job GC.
pub fn retire_expired_source_v16(
    caller:&mut CheckedQuotaShardV11, quota:&QuotaPolicyV11, epoch:&TrustedEpochV11,
    plan:&SourcePlanV16, row:&SourceObligationV16, expected_row:&Digest,
    terminal:&CheckedQuotaTerminalV11,
)->Result<SourceObligationV16,String>{
    quota.check()?;epoch.check(quota)?;
    let subject=caller.subject_row().ok_or("expired source collection lost original subject")?;
    if row.root()!=*expected_row || row.version!=SOURCE_VERSION_V16 || row.retired
        || !subject.closed || terminal.binding!=subject.binding || row.subject_root!=subject.binding.root()
        || row.job_id!=subject.binding.job_id || row.producer!=subject.binding.producer
        || row.plan_root!=plan.root() || row.class!=plan.class || row.parameter!=plan.parameter || row.inventory!=plan.inventory
        || row.quota_plan_root!=subject.binding.quota_plan_root || plan.job_plan!=subject.binding.quota_plan_root
        || plan.quota!=quota.root() || row.admitted_interval!=subject.binding.admitted_interval
        || row.deadline_daa!=subject.binding.deadline_daa || epoch.network_interval<=row.admitted_interval
        || epoch.block_daa<=row.deadline_daa || row.original_consent_root==[0;64] || row.producer_operator==[0;64]
        || row.requests_cap!=plan.requests || row.requests_admitted>row.requests_cap
        || row.answers_used>row.requests_admitted.checked_mul(ANSWER_ATTEMPTS_PER_UNIT_V16).ok_or("expired source attempts overflow")?
        || row.committed!=plan.admission.checked_add(&plan.guarantee)?
        || row.consumed!=plan.admission.checked_add(&plan.answer.checked_mul(row.answers_used)?)?
        || row.guarantee_remaining!=plan.guarantee.checked_sub(&plan.answer.checked_mul(row.answers_used)?)?
    {return Err("expired source collection lacks exact original consent/subject/terminal/attempts".into());}
    let bytes=QUOTA_GLOBAL_ROW_BYTES_V11+QUOTA_SELECTED_ACTOR_ROW_BYTES_V11+QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11
        +SOURCE_ROW_BYTES_V16+SOURCE_PLAN_RECORD_BYTES_V16+SOURCE_JOURNAL_BYTES_V16;
    let action=DerivedOrdinaryMetadataV11{quantities:action_envelope(bytes as u128*64,bytes,None)?};
    caller.debit_ordinary(quota,epoch,&action)?;
    let mut retired=row.clone();
    // Closing the documentary allowance does not add the old quantity to this
    // interval's physical consumption or subtract it from current reservations.
    retired.retired=true;retired.consumed=retired.committed.clone();retired.guarantee_remaining=QuotaQuantitiesV11::default();
    Ok(retired)
}
