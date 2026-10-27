//! Versioned native trace restoration supplements. Frozen DARequest11 and
//! DAAnswer11 remain separate tokens; their 16-KiB tariff is never redefined.
//! Producer consent reserves only producer answers. Paid independent callers
//! reserve their own request restoration, and ordinary callers spend their own
//! current allowance. All failed expensive attempts remain spent.
use super::*;
use crate::flat_execution11::{CheckedNativeExecutionClassV11, ExecutionClassContextV11, ExecutionJobStateV11, ExecutionTerminalV11};

pub const TRACE_ROW_BYTES_V17: u64 = 2 + 1 + 7 * 64 + 2 * 8 + 2 * 4 + 1 + 3 * 21 * 16;
const JOURNAL_BYTES: u64 = 16 << 10;
#[derive(Clone, Debug)]
pub struct TracePlanV17 {
    class: Digest,
    quota: Digest,
    job_plan: Digest,
    request: QuotaQuantitiesV11,
    answer: QuotaQuantitiesV11,
    admission: QuotaQuantitiesV11,
    producer_attempts: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_restore_is_caller_owned_prepaid_and_failed_attempts_do_not_recredit() {
        let (class, job, quota, epoch, book, native, mut state) =
            super::super::protected_tests::fixture_with_config_and_answers(257, None, None, Some(48));
        let paid = check_native_paid_seal_v11(&class, &book, &epoch, &native).unwrap();
        state.admit_protected_seal(&quota, &epoch, native.seal_actor, &paid, &job).unwrap();
        let plan = trace_plan_from_native_record_v17(class.record().id(), &job.record(), &job.record().root(), &quota).unwrap();
        let checked = check_trace_seal_v17(&class, &book, &epoch, &plan, &native).unwrap();
        let g = state.global_row();
        let a = state.actor_row(&native.seal_actor).unwrap();
        let s = state.subject(&book.job().id()).unwrap();
        let mut shard = TraceShardV17::from_native_rows(
            &quota,
            &plan,
            &g,
            &g.root(),
            native.seal_actor,
            Some((&a, &a.root())),
            (s, &s.row_root()),
            None,
        )
        .unwrap();
        let producer_before = state.actor_row(&book.job().producer).unwrap();
        shard.admit(&quota, &epoch, &plan, &checked).unwrap();
        assert_eq!(state.actor_row(&book.job().producer).unwrap(), producer_before);
        assert_eq!(shard.actor_row().retained_protected_seals, a.retained_protected_seals);
        let total = shard.global_row().network.total().unwrap();
        let key = paid.key().root();
        shard.debit_reserved(&quota, &epoch, &plan, &key, 1).unwrap();
        assert_eq!(shard.trace_row().unwrap().attempts_used, 1);
        assert_eq!(shard.global_row().network.total().unwrap(), total);
        // Native may dismiss the alleged coordinate after this receipt; the
        // receipt has already consumed exactly one original paid attempt.
        let wrong = [9; 64];
        let before = shard.trace_row().unwrap().root();
        assert!(shard.debit_reserved(&quota, &epoch, &plan, &wrong, 1).is_err());
        assert_eq!(shard.trace_row().unwrap().root(), before);
        shard.debit_reserved(&quota, &epoch, &plan, &key, 1).unwrap();
        assert!(shard.debit_reserved(&quota, &epoch, &plan, &key, 1).is_err());
        assert_eq!(shard.trace_row().unwrap().remaining, QuotaQuantitiesV11::default());
        assert_eq!(shard.global_row().network.total().unwrap(), total);
        let g = shard.global_row();
        let a = shard.actor_row();
        let row = shard.trace_row().unwrap();
        assert!(
            TraceShardV17::from_native_rows(
                &quota,
                &plan,
                g,
                &g.root(),
                book.job().producer,
                Some((&producer_before, &producer_before.root())),
                (s, &s.row_root()),
                Some((row, &row.root()))
            )
            .is_err()
        );
        let mut changed = row.clone();
        changed.attempts_used = 0;
        assert!(
            TraceShardV17::from_native_rows(
                &quota,
                &plan,
                g,
                &g.root(),
                a.actor,
                Some((a, &a.root())),
                (s, &s.row_root()),
                Some((&changed, &changed.root()))
            )
            .is_err()
        );
    }
    #[test]
    fn ordinary_restore_cannot_consume_another_owners_guarantee() {
        let (class, job, quota, epoch, book, _, state) =
            super::super::protected_tests::fixture_with_config_and_answers(257, None, None, Some(48));
        let plan = trace_plan_from_native_record_v17(class.record().id(), &job.record(), &job.record().root(), &quota).unwrap();
        let g = state.global_row();
        let s = state.subject(&book.job().id()).unwrap();
        let mut public =
            TraceShardV17::from_native_rows(&quota, &plan, &g, &g.root(), [99; 64], None, (s, &s.row_root()), None).unwrap();
        let reserved = public.global_row().network.reserved.clone();
        public.debit_ordinary(&quota, &epoch, &plan).unwrap();
        assert_eq!(public.global_row().network.reserved, reserved);
        assert_eq!(public.actor_row().interval.reserved, QuotaQuantitiesV11::default());
        assert_eq!(public.actor_row().interval.consumed, *plan.request());
        assert!(plan.request().get(ResourceAxisV10::MetadataWork) > job.native_restore_work() as u128);
        assert!(plan.request().get(ResourceAxisV10::OwnedPeakBytes) > job.native_restore_work() as u128);
    }
}
impl TracePlanV17 {
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/native-trace-restore-plan/v17",
            &(17u16, self.class, self.quota, self.job_plan, &self.request, &self.answer, &self.admission, self.producer_attempts),
        )
    }
    pub fn request(&self) -> &QuotaQuantitiesV11 {
        &self.request
    }
    pub fn answer(&self) -> &QuotaQuantitiesV11 {
        &self.answer
    }
}
pub fn derive_trace_plan_v17(
    class: &CheckedNativeExecutionClassV11,
    job: &CheckedJobQuotaPlanV11,
    quota: &QuotaPolicyV11,
) -> Result<TracePlanV17, String> {
    derive_trace_plan_base_v17(
        class.record().id(),
        class.record().program_root,
        class.record().parameter_root,
        class.record().class_scope9_root,
        job,
        quota,
    )
}
/// Reuse the fixed original job quote read from authenticated native state,
/// before restoring a large class or post. This grants no admission authority;
/// paid caller/producer factories still independently check the original book.
pub fn trace_plan_from_native_record_v17(
    class: Digest,
    record: &QuotaPlanRecordV11,
    expected: &Digest,
    quota: &QuotaPolicyV11,
) -> Result<TracePlanV17, String> {
    let checked = CheckedJobQuotaPlanV11::restore(record, expected)?;
    derive_trace_plan_base_v17(class, record.program_root, record.parameter_root, record.class_scope_root, &checked, quota)
}
/// Only checked stage adapters may supply their separately authenticated
/// original source/program/scope roots. There is no public cheap-quote factory.
pub(super) fn derive_trace_plan_base_v17(
    class: Digest,
    program: Digest,
    parameter: Digest,
    scope: Digest,
    job: &CheckedJobQuotaPlanV11,
    quota: &QuotaPolicyV11,
) -> Result<TracePlanV17, String> {
    quota.check()?;
    let r = job.record();
    if r.program_root != program || r.parameter_root != parameter || r.class_scope_root != scope || r.native_restore_work == 0 {
        return Err("trace quote changes original checked class/job scope".into());
    }
    // The original restore tariff bounds program/PC/scope traversal. Conservatively
    // treat every such work unit as one owned byte too, rather than understating
    // a decoded tree's allocator/peak/copy axes. No complete weights are restored.
    let bytes = r
        .native_restore_work
        .checked_add(r.native_post_carrier_cap.checked_mul(4).ok_or("trace post extent overflow")?)
        .and_then(|n| n.checked_add(JOURNAL_BYTES))
        .ok_or("trace restore extent overflow")?;
    let metadata = native_action_metadata_v11(r.native_restore_work, r.native_post_carrier_cap, JOURNAL_BYTES, false)?
        .checked_add(bytes as u128 * 64)
        .ok_or("trace restore work overflow")?;
    let request = action_envelope(metadata, bytes, None)?;
    let answer = request.clone();
    let admission = action_envelope(JOURNAL_BYTES as u128 * 64, JOURNAL_BYTES, None)?;
    admission.checked_add(&answer.checked_mul(quota.future_attempts[2])?)?.within(&quota.plan)?;
    Ok(TracePlanV17 {
        class,
        quota: quota.root(),
        job_plan: r.root(),
        request,
        answer,
        admission,
        producer_attempts: quota.future_attempts[2],
    })
}
pub struct CheckedTraceAdmissionV17 {
    binding: SubjectBindingV11,
    key: Digest,
    actor: Digest,
    original: Digest,
    role: u8,
    cap: u32,
    horizon: u64,
    epoch: TrustedEpochV11,
}
// Only Stage14 can construct this non-wire checked original paid proof token.
pub(super) fn from_checked_stage_proof_v14(
    checked: stage14::CheckedStageTraceProofGrantV14,
    plan: &TracePlanV17,
) -> Result<CheckedTraceAdmissionV17, String> {
    let (binding, key, actor, original, trace, horizon, epoch) = checked.into_parts();
    if trace != plan.root()
        || binding.class_id != plan.class
        || binding.quota_plan_root != plan.job_plan
        || binding.policy_root != plan.quota
        || binding.admitted_interval != epoch.network_interval
        || actor == binding.producer
        || original == [0; 64]
        || key == [0; 64]
        || horizon > binding.deadline_daa
    {
        return Err("stage paid seal trace adapter lacks original checked quote/actor/epoch/horizon".into());
    }
    Ok(CheckedTraceAdmissionV17 { binding, key, actor, original, role: 1, cap: 2, horizon, epoch })
}
// Only stage14's private checked token reaches this bridge; raw wire roots and
// caller flags cannot manufacture a producer-owned reservation.
pub(super) fn from_checked_stage_admission_v17(
    checked: stage14::CheckedStageTraceGrantV14,
    plan: &TracePlanV17,
) -> Result<CheckedTraceAdmissionV17, String> {
    if checked.trace_plan != plan.root()
        || checked.binding.class_id != plan.class
        || checked.binding.quota_plan_root != plan.job_plan
        || checked.binding.policy_root != plan.quota
        || checked.binding.admitted_interval != checked.epoch.network_interval
        || checked.consent == [0; 64]
    {
        return Err("stage trace bridge lacks exact checked original consent/quote/epoch".into());
    }
    Ok(CheckedTraceAdmissionV17 {
        key: checked.binding.job_id,
        actor: checked.binding.producer,
        horizon: checked.binding.deadline_daa,
        binding: checked.binding,
        original: checked.consent,
        role: 0,
        cap: plan.producer_attempts,
        epoch: checked.epoch,
    })
}
pub fn check_trace_producer_v17(
    class: &CheckedNativeExecutionClassV11,
    book: &ExecutionJobStateV11,
    epoch: &TrustedEpochV11,
    plan: &TracePlanV17,
    consent: &source16::NativeSourceConsentV16,
) -> Result<CheckedTraceAdmissionV17, String> {
    let j = book.job();
    if !class.can_admit()
        || class.native_admission_epoch() != Some((epoch.block_hash, epoch.block_daa))
        || book.terminal() != ExecutionTerminalV11::Funded
        || plan.class != class.record().id()
        || plan.class != j.class_id
        || plan.job_plan != j.quota_plan_root
        || plan.quota != class.record().quota_policy_root
        || consent.job_id != j.id()
        || consent.funded_job_root != j.root()
        || consent.book_root != book.root()
        || consent.producer != j.producer
        || consent.producer_operator == [0; 64]
        || consent.original_consent_root == [0; 64]
        || consent.original_trace_plan_root != plan.root()
        || consent.accepted_daa > epoch.block_daa
        || consent.expires_daa < epoch.block_daa
        || epoch.block_daa != j.admitted_daa
        || epoch.network_interval != j.admitted_interval
    {
        return Err("trace producer supplement lacks exact original signed native funding".into());
    }
    Ok(CheckedTraceAdmissionV17 {
        binding: j.quota_subject(class.record())?,
        key: j.id(),
        actor: j.producer,
        original: consent.original_consent_root,
        role: 0,
        cap: plan.producer_attempts,
        horizon: j.deadline_daa,
        epoch: epoch.clone(),
    })
}
pub fn check_trace_seal_v17<C: ExecutionClassContextV11>(
    class: &C,
    book: &ExecutionJobStateV11,
    epoch: &TrustedEpochV11,
    plan: &TracePlanV17,
    native: &NativePaidSealAnchorV11,
) -> Result<CheckedTraceAdmissionV17, String> {
    let checked = check_native_paid_seal_v11(class, book, epoch, native)?;
    if plan.class != class.record().id()
        || plan.job_plan != book.job().quota_plan_root
        || plan.quota != class.record().quota_policy_root
    {
        return Err("trace seal supplement changes original plan".into());
    }
    Ok(CheckedTraceAdmissionV17 {
        binding: checked.binding().clone(),
        key: checked.key().root(),
        actor: native.seal_actor,
        original: native.original_paid_seal_root,
        role: 1,
        cap: 2,
        horizon: native.horizon_daa,
        epoch: epoch.clone(),
    })
}
pub fn check_trace_discovery_v17<C: ExecutionClassContextV11>(
    class: &C,
    book: &ExecutionJobStateV11,
    epoch: &TrustedEpochV11,
    plan: &TracePlanV17,
    policy: &discovery13::DiscoveryPolicyV13,
    native: &discovery13::NativePaidDiscoveryV13,
) -> Result<CheckedTraceAdmissionV17, String> {
    discovery13::check_native_paid_discovery_v13(class, book, epoch, native)?;
    if plan.class != class.record().id()
        || plan.job_plan != book.job().quota_plan_root
        || plan.quota != class.record().quota_policy_root
        || policy.quota_policy_root != plan.quota
    {
        return Err("trace discovery supplement changes original plan".into());
    }
    Ok(CheckedTraceAdmissionV17 {
        binding: book.job().quota_subject(class.record())?,
        key: native.key(),
        actor: native.actor,
        original: native.original_root,
        role: 2,
        cap: policy.requests_per_session.checked_add(2).ok_or("trace discovery request/converted-proof bound overflow")?,
        horizon: native.horizon_daa,
        epoch: epoch.clone(),
    })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TraceSupplementV17 {
    pub version: u16,
    pub role: u8,
    pub key: Digest,
    pub job: Digest,
    pub subject_root: Digest,
    pub plan_root: Digest,
    pub actor: Digest,
    pub original_paid_root: Digest,
    pub quota_plan_root: Digest,
    pub interval: u64,
    pub horizon: u64,
    pub attempts_cap: u32,
    pub attempts_used: u32,
    pub retired: bool,
    pub committed: QuotaQuantitiesV11,
    pub consumed: QuotaQuantitiesV11,
    pub remaining: QuotaQuantitiesV11,
}
impl TraceSupplementV17 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/native-trace-restore-row/v17", self)
    }
}
pub struct TraceShardV17 {
    global: QuotaGlobalRowV11,
    actor: QuotaActorRowV11,
    subject: QuotaSubjectSummaryV11,
    row: Option<TraceSupplementV17>,
}
impl TraceShardV17 {
    pub fn from_native_rows(
        quota: &QuotaPolicyV11,
        plan: &TracePlanV17,
        global: &QuotaGlobalRowV11,
        global_root: &Digest,
        actor: Digest,
        actor_row: Option<(&QuotaActorRowV11, &Digest)>,
        subject: (&QuotaSubjectSummaryV11, &Digest),
        row: Option<(&TraceSupplementV17, &Digest)>,
    ) -> Result<Self, String> {
        if plan.quota != quota.root() || subject.0.binding.quota_plan_root != plan.job_plan {
            return Err("trace shard original quota/plan mismatch".into());
        }
        let base = CheckedQuotaShardV11::from_native_rows(
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
        let actor = base.actor_row();
        let subject = base.subject_row().ok_or("trace subject absent")?.clone();
        if let Some((r, root)) = row {
            let unit = if r.role == 0 { &plan.answer } else { &plan.request };
            let spent = unit.checked_mul(r.attempts_used)?;
            if r.root() != *root
                || r.version != 17
                || r.role > 2
                || r.actor != actor.actor
                || r.job != subject.binding.job_id
                || r.subject_root != subject.binding.root()
                || r.plan_root != plan.root()
                || r.original_paid_root == [0; 64]
                || r.quota_plan_root != plan.job_plan
                || r.interval != subject.binding.admitted_interval
                || r.horizon > subject.binding.deadline_daa
                || r.attempts_cap == 0
                || r.attempts_used > r.attempts_cap
                || r.committed != plan.admission.checked_add(&unit.checked_mul(r.attempts_cap)?)?
                || r.consumed.checked_add(&r.remaining)? != r.committed
                || (!r.retired
                    && (r.consumed != plan.admission.checked_add(&spent)?
                        || r.remaining != unit.checked_mul(r.attempts_cap - r.attempts_used)?))
                || (r.retired && (r.remaining != QuotaQuantitiesV11::default() || r.consumed != r.committed))
            {
                return Err("trace supplement original identity/attempt/conservation mismatch".into());
            }
            r.committed.within(&quota.plan)?;
            if r.interval == global.epoch.network_interval {
                r.remaining.within(&actor.interval.reserved)?;
                r.remaining.within(&global.network.reserved)?;
            }
        }
        Ok(Self { global: global.clone(), actor, subject, row: row.map(|(r, _)| r.clone()) })
    }
    pub fn global_row(&self) -> &QuotaGlobalRowV11 {
        &self.global
    }
    pub fn actor_row(&self) -> &QuotaActorRowV11 {
        &self.actor
    }
    pub fn trace_row(&self) -> Option<&TraceSupplementV17> {
        self.row.as_ref()
    }
    fn context(&self, quota: &QuotaPolicyV11, epoch: &TrustedEpochV11, plan: &TracePlanV17) -> Result<(), String> {
        epoch.check(quota)?;
        let old = &self.global.epoch;
        if self.global.policy_root != plan.quota
            || plan.quota != quota.root()
            || epoch.block_daa < old.block_daa
            || epoch.network_interval != old.network_interval
            || epoch.network_interval != self.subject.binding.admitted_interval
            || epoch.block_hash == old.block_hash && epoch != old
        {
            return Err("trace context rewinds/crosses original interval".into());
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
        plan: &TracePlanV17,
        checked: &CheckedTraceAdmissionV17,
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        if self.row.is_some()
            || self.subject.closed
            || checked.binding != self.subject.binding
            || checked.actor != self.actor.actor
            || checked.epoch != *epoch
            || checked.cap == 0
            || checked.horizon > self.subject.binding.deadline_daa
            || !self.actor.active && self.global.actor_count >= quota.max_actors
        {
            return Err("trace supplement original paid admission/owner differs".into());
        }
        let unit = if checked.role == 0 { &plan.answer } else { &plan.request };
        let guarantee = unit.checked_mul(checked.cap)?;
        let committed = plan.admission.checked_add(&guarantee)?;
        committed.within(&quota.plan)?;
        let block = self.block(epoch).admit(&plan.admission, &guarantee, &quota.block)?;
        let network = self.global.network.admit(&plan.admission, &guarantee, &quota.network)?;
        let actor = self.actor.interval.admit(&plan.admission, &guarantee, &quota.actor)?;
        if !self.actor.active {
            self.global.actor_count += 1;
        }
        self.actor.active = true;
        self.actor.interval = actor;
        self.actor.network_interval = epoch.network_interval;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.row = Some(TraceSupplementV17 {
            version: 17,
            role: checked.role,
            key: checked.key,
            job: self.subject.binding.job_id,
            subject_root: self.subject.binding.root(),
            plan_root: plan.root(),
            actor: checked.actor,
            original_paid_root: checked.original,
            quota_plan_root: plan.job_plan,
            interval: epoch.network_interval,
            horizon: checked.horizon,
            attempts_cap: checked.cap,
            attempts_used: 0,
            retired: false,
            committed,
            consumed: plan.admission.clone(),
            remaining: guarantee,
        });
        Ok(())
    }
    pub fn debit_reserved(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &TracePlanV17,
        key: &Digest,
        role: u8,
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let mut r = self.row.clone().ok_or("trace paid supplement absent")?;
        if self.subject.closed
            || r.retired
            || r.key != *key
            || r.role != role
            || epoch.block_daa > r.horizon
            || r.attempts_used >= r.attempts_cap
        {
            return Err("trace supplement original paid attempt unavailable".into());
        }
        let unit = if role == 0 { &plan.answer } else { &plan.request };
        let block = self.block(epoch).consume(unit, &quota.block)?;
        let network = self.global.network.consume(unit, &quota.network)?;
        let actor = self.actor.interval.consume(unit, &quota.actor)?;
        r.attempts_used += 1;
        r.consumed = r.consumed.checked_add(unit)?;
        r.remaining = r.remaining.checked_sub(unit)?;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.actor.interval = actor;
        self.row = Some(r);
        Ok(())
    }
    /// Current ordinary requester pays its own supplement; no producer or
    /// existing protected owner's reservation is borrowed by a public caller.
    pub fn debit_ordinary(&mut self, quota: &QuotaPolicyV11, epoch: &TrustedEpochV11, plan: &TracePlanV17) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        if self.subject.closed || self.row.is_some() || !self.actor.active && self.global.actor_count >= quota.max_actors {
            return Err("trace ordinary requester unavailable".into());
        }
        let zero = QuotaQuantitiesV11::default();
        let block = self.block(epoch).admit(&plan.request, &zero, &quota.block)?;
        let network = self.global.network.admit(&plan.request, &zero, &quota.network)?;
        let actor = self.actor.interval.admit(&plan.request, &zero, &quota.actor)?;
        if !self.actor.active {
            self.global.actor_count += 1;
        }
        self.actor.active = true;
        self.actor.network_interval = epoch.network_interval;
        self.actor.interval = actor;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        Ok(())
    }
    /// **G14 lane closure G-N1:** retire a lapsed seal's unused restoration supplement in its own interval. The remaining
    /// guarantee is consumed (spent, never refunded); a supplement that already restored a request keeps no different rule — it is
    /// retired the same way, its spent attempts stay spent.
    pub fn retire_lapsed_v41(&mut self, quota: &QuotaPolicyV11, epoch: &TrustedEpochV11, plan: &TracePlanV17, key: &Digest) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let mut r = self.row.clone().ok_or("lapsed trace supplement absent")?;
        if r.retired || r.key != *key || r.role != 1 {
            return Err("lapsed trace supplement differs from its seal".into());
        }
        let spent = r.remaining.clone();
        let block = self.block(epoch).consume(&spent, &quota.block)?;
        let network = self.global.network.consume(&spent, &quota.network)?;
        let actor = self.actor.interval.consume(&spent, &quota.actor)?;
        r.retired = true;
        r.consumed = r.committed.clone();
        r.remaining = QuotaQuantitiesV11::default();
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.actor.interval = actor;
        self.row = Some(r);
        Ok(())
    }
    /// Conversion preserves original paid capacity and spent attempts.
    pub fn handoff_discovery(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &TracePlanV17,
        original: &Digest,
        checked: &CheckedTraceAdmissionV17,
    ) -> Result<(), String> {
        self.context(quota, epoch, plan)?;
        let r = self.row.as_mut().ok_or("trace discovery supplement absent")?;
        if r.role != 2
            || r.key != *original
            || r.retired
            || checked.role != 1
            || checked.actor != r.actor
            || checked.binding != self.subject.binding
            || checked.epoch != *epoch
            || checked.horizon > self.subject.binding.deadline_daa
            || epoch.block_daa > r.horizon
        {
            return Err("trace conversion differs from original paid session/owner/horizon".into());
        }
        r.role = 1;
        r.key = checked.key;
        r.horizon = checked.horizon;
        Ok(())
    }
}

/// Public retirement after the complete ORIGINAL job interval has expired.
/// Native authenticates the selected entitlement/owner index before passing
/// this rooted row; terminal membership alone cannot select a paid owner row.
/// Only the actual caller spends current fixed-row work. Old reserves are
/// documentary and never become current credit, slots or fresh attempts.
pub fn retire_expired_trace_v17(
    caller: &mut CheckedQuotaShardV11,
    quota: &QuotaPolicyV11,
    epoch: &TrustedEpochV11,
    plan: &TracePlanV17,
    row: &TraceSupplementV17,
    expected: &Digest,
    terminal: &CheckedQuotaTerminalV11,
) -> Result<TraceSupplementV17, String> {
    epoch.check(quota)?;
    let subject = caller.subject_row().ok_or("expired trace original subject absent")?;
    let unit = if row.role == 0 { &plan.answer } else { &plan.request };
    let spent = unit.checked_mul(row.attempts_used)?;
    if row.root() != *expected
        || row.version != 17
        || row.role > 2
        || row.retired
        || row.key == [0; 64]
        || row.actor == [0; 64]
        || row.original_paid_root == [0; 64]
        || !subject.closed
        || terminal.binding != subject.binding
        || row.job != subject.binding.job_id
        || row.subject_root != subject.binding.root()
        || row.plan_root != plan.root()
        || plan.class != subject.binding.class_id
        || plan.quota != quota.root()
        || row.quota_plan_root != subject.binding.quota_plan_root
        || plan.job_plan != subject.binding.quota_plan_root
        || row.interval != subject.binding.admitted_interval
        || epoch.network_interval <= row.interval
        || epoch.block_daa <= subject.binding.deadline_daa
        || row.horizon > subject.binding.deadline_daa
        || row.attempts_cap == 0
        || row.attempts_cap > 1 << 20
        || row.attempts_used > row.attempts_cap
        || row.role == 0 && (row.key != row.job || row.actor != subject.binding.producer || row.attempts_cap != plan.producer_attempts)
        || row.committed != plan.admission.checked_add(&unit.checked_mul(row.attempts_cap)?)?
        || row.consumed != plan.admission.checked_add(&spent)?
        || row.remaining != unit.checked_mul(row.attempts_cap - row.attempts_used)?
    {
        return Err("expired trace collection lacks exact original entitlement/closed terminal/conservation".into());
    }
    let bytes = QUOTA_GLOBAL_ROW_BYTES_V11
        + QUOTA_SELECTED_ACTOR_ROW_BYTES_V11
        + QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11
        + TRACE_ROW_BYTES_V17
        + JOURNAL_BYTES;
    let cost = DerivedOrdinaryMetadataV11 { quantities: action_envelope(bytes as u128 * 64, bytes, None)? };
    caller.debit_ordinary(quota, epoch, &cost)?;
    let mut retired = row.clone();
    retired.retired = true;
    retired.consumed = retired.committed.clone();
    retired.remaining = QuotaQuantitiesV11::default();
    Ok(retired)
}
