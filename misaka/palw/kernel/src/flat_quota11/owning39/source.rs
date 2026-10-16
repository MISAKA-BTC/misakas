//! Own39 producer obligations over the SAME authenticated shared21 aggregate.
//! Native authenticates all selected keys/roots and original fresh pending units.
//! Neither a raw plan nor an ordinary caller can reserve another actor's work.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceObligationV39 {
    pub version: u16,
    pub job: Digest,
    pub subject: Digest,
    pub plan: Digest,
    pub producer: Digest,
    pub producer_operator: Digest,
    pub consent: Digest,
    pub interval: u64,
    pub deadline: u64,
    pub units: u32,
    pub attempts: u32,
    pub retired: bool,
    pub committed: QuotaQuantitiesV11,
    pub consumed: QuotaQuantitiesV11,
    pub remaining: QuotaQuantitiesV11,
}
impl SourceObligationV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/original-producer-source-obligation/v39", self)
    }
}
fn admission(plan: &CheckedQuotaV39) -> Result<QuotaQuantitiesV11, String> {
    let bytes = super::super::sharded::QUOTA_GLOBAL_ROW_BYTES_V11
        + super::super::sharded::QUOTA_SELECTED_ACTOR_ROW_BYTES_V11
        + super::super::sharded::QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11
        + 16384;
    action_envelope(bytes as u128 * 128, bytes, None)
}
pub struct SourceShardV39 {
    global: QuotaGlobalRowV11,
    producer: QuotaActorRowV11,
    subject: QuotaSubjectSummaryV11,
    row: Option<SourceObligationV39>,
}
impl SourceShardV39 {
    pub fn from_native_rows(
        policy: &QuotaPolicyV11,
        plan: &CheckedQuotaV39,
        global: &QuotaGlobalRowV11,
        global_root: &Digest,
        producer: (&QuotaActorRowV11, &Digest),
        subject: (&QuotaSubjectSummaryV11, &Digest),
        row: Option<(&SourceObligationV39, &Digest)>,
    ) -> Result<Self, String> {
        let selected = CheckedQuotaShardV11::from_native_rows(
            policy,
            global,
            global_root,
            producer.0.actor,
            subject.0.binding.job_id,
            None,
            Some(producer),
            Some(subject),
            None,
        )?;
        let s = selected.subject_row().ok_or("source39 original subject absent")?.clone();
        let a = selected.actor_row();
        if s.binding.producer != a.actor
            || s.binding.class_id != plan.class
            || s.binding.quota_plan_root != plan.plan.record().root()
            || s.binding.job_input_root != plan.maximum
        {
            return Err("source39 selected original class/max/producer differs".into());
        }
        if let Some((r, root)) = row {
            let spent = plan.source_plan.answer.checked_mul(r.attempts)?;
            let original = admission(plan)?.checked_add(&plan.source_plan.total)?;
            if r.root() != *root
                || r.version != 39
                || r.job != s.binding.job_id
                || r.subject != s.binding.root()
                || r.plan != plan.source_plan.root()
                || r.producer != a.actor
                || r.producer_operator == [0; 64]
                || r.consent == [0; 64]
                || r.interval != s.binding.admitted_interval
                || r.deadline != s.binding.deadline_daa
                || r.units > plan.source_plan.requests
                || r.attempts > r.units.checked_mul(2).ok_or("source39 attempts overflow")?
                || r.committed != original
                || r.consumed.checked_add(&r.remaining)? != original
                || !r.retired
                    && (r.consumed != admission(plan)?.checked_add(&spent)?
                        || r.remaining != plan.source_plan.total.checked_sub(&spent)?)
                || r.retired && (r.remaining != QuotaQuantitiesV11::default() || r.consumed != original)
            {
                return Err("source39 original attempt/conservation/identity differs".into());
            }
            r.committed.within(&policy.plan)?;
            if r.interval == global.epoch.network_interval {
                r.remaining.checked_add(&s.guarantee_remaining)?.within(&global.network.reserved)?;
                r.remaining.checked_add(&s.guarantee_remaining)?.within(&a.interval.reserved)?;
            }
        }
        Ok(Self { global: global.clone(), producer: a, subject: s, row: row.map(|(r, _)| r.clone()) })
    }
    pub fn global_row(&self) -> &QuotaGlobalRowV11 {
        &self.global
    }
    pub fn producer_row(&self) -> &QuotaActorRowV11 {
        &self.producer
    }
    pub fn source_row(&self) -> Option<&SourceObligationV39> {
        self.row.as_ref()
    }
    fn context(&self, p: &QuotaPolicyV11, e: &TrustedEpochV11) -> Result<(), String> {
        if !self.context_at_or_after(p, e)? {
            return Err("source39 original interval/clock/context differs".into());
        }
        Ok(())
    }
    /// The shared context check, also admitting a LATER interval than the subject's admitted one. `Ok(true)`: the subject's own
    /// admitted interval is current.
    fn context_at_or_after(&self, p: &QuotaPolicyV11, e: &TrustedEpochV11) -> Result<bool, String> {
        p.check()?;
        e.check(p)?;
        let old = &self.global.epoch;
        if self.global.policy_root != p.root()
            || e.block_daa < old.block_daa
            || e.network_interval != old.network_interval
            || e.network_interval < self.subject.binding.admitted_interval
            || e.block_hash == old.block_hash && e != old
        {
            return Err("source39 original interval/clock/context differs".into());
        }
        Ok(e.network_interval == self.subject.binding.admitted_interval)
    }
    fn block(&self, e: &TrustedEpochV11) -> QuotaIntervalSummaryV11 {
        if e.block_hash == self.global.epoch.block_hash {
            self.global.block.clone()
        } else {
            QuotaIntervalSummaryV11 { consumed: QuotaQuantitiesV11::default(), reserved: self.global.block.reserved.clone() }
        }
    }
    pub fn admit(
        &mut self,
        p: &QuotaPolicyV11,
        e: &TrustedEpochV11,
        plan: &CheckedQuotaV39,
        grant: &CheckedSourceAdmissionV39,
    ) -> Result<(), String> {
        self.context(p, e)?;
        let (binding, operator, consent, epoch, source_root) = grant.parts();
        if self.row.is_some()
            || self.subject.closed
            || *binding != self.subject.binding
            || *epoch != *e
            || *source_root != plan.source_plan.root()
            || !self.producer.active
            || self.producer.network_interval != e.network_interval
        {
            return Err("source39 original fresh producer admission differs".into());
        }
        let base = admission(plan)?;
        let guarantee = &plan.source_plan.total;
        let block = self.block(e).admit(&base, guarantee, &p.block)?;
        let network = self.global.network.admit(&base, guarantee, &p.network)?;
        let actor = self.producer.interval.admit(&base, guarantee, &p.actor)?;
        self.row = Some(SourceObligationV39 {
            version: 39,
            job: binding.job_id,
            subject: binding.root(),
            plan: plan.source_plan.root(),
            producer: self.producer.actor,
            producer_operator: *operator,
            consent: *consent,
            interval: e.network_interval,
            deadline: binding.deadline_daa,
            units: 0,
            attempts: 0,
            retired: false,
            committed: base.checked_add(guarantee)?,
            consumed: base,
            remaining: guarantee.clone(),
        });
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = e.clone();
        self.producer.interval = actor;
        Ok(())
    }
    /// Called only after native authenticated a fresh coordinate and the caller's
    /// original paid request. Invalid or duplicated coordinates cannot steal units.
    pub fn admit_valid_unit(
        &mut self,
        p: &QuotaPolicyV11,
        e: &TrustedEpochV11,
        plan: &CheckedQuotaV39,
        job: Digest,
    ) -> Result<(), String> {
        self.context(p, e)?;
        let r = self.row.as_mut().ok_or("source39 obligation absent")?;
        if self.subject.closed || r.retired || r.job != job || e.block_daa > r.deadline || r.units >= plan.source_plan.requests {
            return Err("source39 original unit capacity unavailable".into());
        }
        r.units += 1;
        Ok(())
    }
    /// The native caller MUST persist this result before decoding/allocating an
    /// answer, and cap two attempts per original immutable unit independently.
    pub fn debit_answer(
        &mut self,
        p: &QuotaPolicyV11,
        e: &TrustedEpochV11,
        plan: &CheckedQuotaV39,
        producer: Digest,
        bytes: u64,
    ) -> Result<(), String> {
        self.context(p, e)?;
        let mut r = self.row.clone().ok_or("source39 obligation absent")?;
        if self.subject.closed
            || r.retired
            || r.producer != producer
            || e.block_daa > r.deadline
            || bytes > plan.source_plan.answer_bytes as u64
            || r.attempts >= r.units.checked_mul(2).ok_or("source39 attempt overflow")?
        {
            return Err("source39 original pending producer attempt unavailable".into());
        }
        let delta = &plan.source_plan.answer;
        let block = self.block(e).consume(delta, &p.block)?;
        let network = self.global.network.consume(delta, &p.network)?;
        let actor = self.producer.interval.consume(delta, &p.actor)?;
        r.attempts += 1;
        r.consumed = r.consumed.checked_add(delta)?;
        r.remaining = r.remaining.checked_sub(delta)?;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = e.clone();
        self.producer.interval = actor;
        self.row = Some(r);
        Ok(())
    }
    /// Final closes unanswered capacity only once every admitted native unit has
    /// actually settled. It burns unused credits, never returning interval room.
    ///
    /// Collection53 calls this only past the subject's admitted interval (the shared v11 `collect_closed_expired` requires an
    /// EARLIER, expired interval), so it must accept a later interval: there the unused obligation's reservation expired with its
    /// own interval's counters, nothing current is consumed or returned, and the row is retired exactly once. In the admitted
    /// interval itself the remaining quantities are burned from the current counters as before. (Requiring the admitted interval
    /// here made every collection of an unretired obligation unreachable: same interval refused by the v11 collection, a later
    /// one by this check.)
    pub fn burn_closed(&mut self, p: &QuotaPolicyV11, e: &TrustedEpochV11, terminal: &CheckedQuotaTerminalV11) -> Result<(), String> {
        let current = self.context_at_or_after(p, e)?;
        let mut r = self.row.clone().ok_or("source39 obligation absent")?;
        if r.retired || terminal.binding != self.subject.binding || !self.subject.closed || r.interval != self.subject.binding.admitted_interval
        {
            return Err("source39 requires genuine original terminal closure".into());
        }
        if current {
            let d = r.remaining.clone();
            let block = self.block(e).consume(&d, &p.block)?;
            let network = self.global.network.consume(&d, &p.network)?;
            let actor = self.producer.interval.consume(&d, &p.actor)?;
            self.global.block = block;
            self.global.network = network;
            self.global.epoch = e.clone();
            self.producer.interval = actor;
        }
        r.retired = true;
        r.consumed = r.committed.clone();
        r.remaining = QuotaQuantitiesV11::default();
        self.row = Some(r);
        Ok(())
    }
}
/// Fixed native-cache answer path. Expected plan/root/subject membership comes
/// from the original Fund header, never this answer carrier. It consumes only an
/// ALREADY admitted39 obligation; it cannot create any grant or reservation.
pub fn debit_native_cached_answer_v39(
    policy: &QuotaPolicyV11,
    epoch: &TrustedEpochV11,
    record: &SourcePlanRecordV39,
    expected_plan: &Digest,
    global: &QuotaGlobalRowV11,
    global_root: &Digest,
    producer: (&QuotaActorRowV11, &Digest),
    subject: (&QuotaSubjectSummaryV11, &Digest),
    obligation: (&SourceObligationV39, &Digest),
    carrier_bytes: u64,
) -> Result<SourceShardV39, String> {
    policy.check()?;
    epoch.check(policy)?;
    let selected = CheckedQuotaShardV11::from_native_rows(
        policy,
        global,
        global_root,
        producer.0.actor,
        subject.0.binding.job_id,
        None,
        Some(producer),
        Some(subject),
        None,
    )?;
    let s = selected.subject_row().ok_or("cached source39 original subject absent")?.clone();
    let mut a = selected.actor_row();
    let mut g = global.clone();
    let mut row = obligation.0.clone();
    let stored = record.answer.checked_mul(row.attempts)?;
    if record.version != 39
        || record.root() != *expected_plan
        || record.quota != s.binding.quota_plan_root
        || record.class != s.binding.class_id
        || record.maximum != s.binding.job_input_root
        || record.parameter != s.binding.parameter_root
        || record.requests == 0
        || record.answer_attempts != 2
        || record.answer_parts != 4
        || record.answer_bytes != 128 << 10
        || row.root() != *obligation.1
        || row.version != 39
        || row.job != s.binding.job_id
        || row.subject != s.binding.root()
        || row.plan != *expected_plan
        || row.producer != a.actor
        || row.producer != s.binding.producer
        || row.consent == [0; 64]
        || row.producer_operator == [0; 64]
        || row.interval != s.binding.admitted_interval
        || row.deadline != s.binding.deadline_daa
        || row.interval != epoch.network_interval
        || g.epoch.network_interval != epoch.network_interval
        || a.network_interval != epoch.network_interval
        || !a.active
        || s.closed
        || row.retired
        || epoch.block_daa > row.deadline
        || epoch.block_daa < g.epoch.block_daa
        || epoch.block_hash == g.epoch.block_hash && *epoch != g.epoch
        || carrier_bytes > record.answer_bytes as u64
        || row.units > record.requests
        || row.attempts >= row.units.checked_mul(2).ok_or("cached source39 attempts overflow")?
        || row.remaining != record.total.checked_sub(&stored)?
        || row.consumed.checked_add(&row.remaining)? != row.committed
    {
        return Err("cached source39 lacks original admitted native plan/producer/unit/conservation".into());
    }
    row.remaining.checked_add(&s.guarantee_remaining)?.within(&a.interval.reserved)?;
    row.remaining.checked_add(&s.guarantee_remaining)?.within(&g.network.reserved)?;
    if g.epoch.block_hash != epoch.block_hash {
        g.block.consumed = QuotaQuantitiesV11::default();
    }
    g.block = g.block.consume(&record.answer, &policy.block)?;
    g.network = g.network.consume(&record.answer, &policy.network)?;
    a.interval = a.interval.consume(&record.answer, &policy.actor)?;
    g.epoch = epoch.clone();
    row.attempts += 1;
    row.consumed = row.consumed.checked_add(&record.answer)?;
    row.remaining = row.remaining.checked_sub(&record.answer)?;
    Ok(SourceShardV39 { global: g, producer: a, subject: s, row: Some(row) })
}
