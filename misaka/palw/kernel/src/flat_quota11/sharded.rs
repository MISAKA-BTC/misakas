//! Fixed selected-row transitions over an authenticated native aggregate.
//!
//! Native authenticates table keys and expected row roots from its journal,
//! prepays these fixed row reads/codecs/hash/copies, and atomically persists all
//! returned rows (including failed expensive attempts). Expected roots supplied
//! by a carrier are NOT authentication. These helpers cannot authenticate chain
//! membership, debit coins, issue rewards, or verify proof-body callbacks.
use super::*;

pub const QUOTA_GLOBAL_ROW_BYTES_V11: u64 = QUOTA_STATE_FIXED_BYTES_V11;
pub const QUOTA_SELECTED_ACTOR_ROW_BYTES_V11: u64 = 64 + 8 + 1 + 2 * 21 * 16 + 4;
pub const QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11: u64 = QUOTA_SUBJECT_ROW_BYTES_V11 - 64;
pub const QUOTA_SELECTED_SEAL_ROW_BYTES_V11: u64 = PROTECTED_SEAL_ROW_BYTES_V11 - 64;
/// No map/Vec and no caller-provided pricing vector exists in this projection.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuotaGlobalRowV11 {
    pub policy_root: Digest,
    pub epoch: TrustedEpochV11,
    pub block: QuotaIntervalSummaryV11,
    pub network: QuotaIntervalSummaryV11,
    pub actor_count: u32,
    pub subject_count: u32,
    pub protected_seal_count: u32,
}
impl QuotaGlobalRowV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/quota-global-row/v11", self)
    }
    pub fn from_rooted_bytes(bytes: &[u8], expected: &Digest) -> Result<Self, String> {
        let row: Self = read_fixed_row(bytes, QUOTA_GLOBAL_ROW_BYTES_V11)?;
        if row.root() != *expected {
            return Err("quota global row differs from authenticated native root".into());
        }
        Ok(row)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuotaActorRowV11 {
    pub actor: Digest,
    pub network_interval: u64,
    pub active: bool,
    pub interval: QuotaIntervalSummaryV11,
    /// Retained seals across ALL intervals; expiry alone never recreates slots.
    pub retained_protected_seals: u32,
}
impl QuotaActorRowV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/quota-actor-row/v11", self)
    }
    pub fn from_rooted_bytes(bytes: &[u8], expected: &Digest) -> Result<Self, String> {
        let row: Self = read_fixed_row(bytes, QUOTA_SELECTED_ACTOR_ROW_BYTES_V11)?;
        if row.root() != *expected {
            return Err("quota actor row differs from authenticated native root".into());
        }
        Ok(row)
    }
}
impl QuotaSubjectSummaryV11 {
    pub fn row_root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/quota-subject-row/v11", self)
    }
    pub fn from_rooted_row_bytes(bytes: &[u8], expected: &Digest) -> Result<Self, String> {
        let row: Self = read_fixed_row(bytes, QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11)?;
        if row.row_root() != *expected {
            return Err("quota subject row differs from authenticated native root".into());
        }
        Ok(row)
    }
}
impl ProtectedSealSummaryV11 {
    pub fn row_root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/quota-seal-row/v11", self)
    }
    pub fn from_rooted_row_bytes(bytes: &[u8], expected: &Digest) -> Result<Self, String> {
        let row: Self = read_fixed_row(bytes, QUOTA_SELECTED_SEAL_ROW_BYTES_V11)?;
        if row.row_root() != *expected {
            return Err("quota seal row differs from authenticated native root".into());
        }
        Ok(row)
    }
}
fn read_fixed_row<T: BorshDeserialize>(bytes: &[u8], width: u64) -> Result<T, String> {
    if bytes.len() as u128 != width as u128 {
        return Err("quota selected row has wrong fixed extent".into());
    }
    borsh::from_slice(bytes).map_err(|e| format!("malformed fixed quota row: {e}"))
}
/// A fixed projection used only with authenticated native row membership.
/// The partial scratch maps are private and never exposed as a whole quota
/// state/root or passed to full-book conservation checking.
pub struct CheckedQuotaShardV11 {
    state: QuotaStateV11,
    actor: Digest,
    selected_job: Digest,
    selected_seal: Option<Digest>,
    actor_was_current: bool,
    retained_actor_seals: u32,
    global_actor_count: u32,
    global_subject_count: u32,
    global_seal_count: u32,
    subject_key: Option<Digest>,
    seal_key: Option<Digest>,
}
impl CheckedQuotaShardV11 {
    /// All expected roots and absent-row decisions come solely from actual
    /// authenticated native tables/journal. Native must also authenticate the
    /// selected keys and original class/job/plan inputs before using this API.
    pub fn from_native_rows(
        policy: &QuotaPolicyV11,
        global: &QuotaGlobalRowV11,
        expected_global: &Digest,
        actor: Digest,
        selected_job: Digest,
        selected_seal: Option<Digest>,
        actor_row: Option<(&QuotaActorRowV11, &Digest)>,
        subject: Option<(&QuotaSubjectSummaryV11, &Digest)>,
        seal: Option<(&ProtectedSealSummaryV11, &Digest)>,
    ) -> Result<Self, String> {
        policy.check()?;
        global.epoch.check(policy)?;
        if global.root() != *expected_global
            || global.policy_root != policy.root()
            || global.actor_count > policy.max_actors
            || global.subject_count > policy.max_subjects
            || global.protected_seal_count > PROTECTED_SEALS_MAX_V11
        {
            return Err("quota shard global identity/count mismatch".into());
        }
        global.block.total()?.within(&policy.block)?;
        global.network.total()?.within(&policy.network)?;
        global.block.consumed.within(&global.network.consumed)?;
        if global.block.reserved != global.network.reserved {
            return Err("quota shard aggregate guarantee differs".into());
        }
        let mut actors = BTreeMap::new();
        let mut actor_was_current = false;
        let mut retained_actor_seals = 0;
        if let Some((row, expected)) = actor_row {
            if row.root() != *expected
                || row.actor != actor
                || row.network_interval > global.epoch.network_interval
                || row.retained_protected_seals > PROTECTED_SEALS_PER_ACTOR_V11
                || row.retained_protected_seals > global.protected_seal_count
                || !row.active && row.interval != QuotaIntervalSummaryV11::default()
            {
                return Err("quota shard actor identity/count mismatch".into());
            }
            retained_actor_seals = row.retained_protected_seals;
            actor_was_current = row.active && row.network_interval == global.epoch.network_interval;
            if actor_was_current {
                if global.actor_count == 0 {
                    return Err("quota shard lost its active actor count".into());
                }
                row.interval.total()?.within(&policy.actor)?;
                row.interval.consumed.within(&global.network.consumed)?;
                row.interval.reserved.within(&global.network.reserved)?;
                actors.insert(actor, row.interval.clone());
            }
        }
        let mut subjects = BTreeMap::new();
        let mut subject_key = None;
        if let Some((row, expected)) = subject {
            if row.row_root() != *expected
                || row.binding.job_id != selected_job
                || row.binding.version != 11
                || row.binding.policy_root != global.policy_root
                || row.binding.admitted_interval > global.epoch.network_interval
                || row.retained_protected_seals > PROTECTED_SEALS_PER_JOB_V11
                || row.retained_protected_seals > global.protected_seal_count
                || global.subject_count == 0
                || row.consumed.checked_add(&row.guarantee_remaining)? != row.committed
            {
                return Err("quota shard original subject identity/conservation mismatch".into());
            }
            row.committed.within(&policy.plan)?;
            if row.future_attempts_used.iter().zip(policy.future_attempts).any(|(used, cap)| *used > cap) {
                return Err("quota shard original attempts exceeded".into());
            }
            if row.binding.admitted_interval == global.epoch.network_interval {
                if row.binding.deadline_daa >= global.epoch.interval_end(policy)? {
                    return Err("quota shard original liability crosses interval".into());
                }
                row.guarantee_remaining.within(&global.network.reserved)?;
            }
            subject_key = Some(row.binding.job_id);
            subjects.insert(row.binding.job_id, row.clone());
        }
        let mut protected_seals = BTreeMap::new();
        let mut seal_key = None;
        if let Some((row, expected)) = seal {
            let subject = subjects.get(&row.key.job_id).ok_or("quota shard seal lacks original selected subject")?;
            if row.row_root() != *expected
                || selected_seal != Some(row.key.root())
                || row.key.seal_actor != actor
                || global.protected_seal_count == 0
                || retained_actor_seals == 0
                || subject.retained_protected_seals == 0
                || row.subject_binding_root != subject.binding.root()
                || row.admitted_interval != subject.binding.admitted_interval
                || row.key.seal_actor == subject.binding.producer
                || row.seal_operator_id == row.producer_operator_id
                || row.original_paid_seal_root == [0; 64]
                || row.key.horizon_daa < row.matures_daa
                || row.key.horizon_daa > subject.binding.deadline_daa
                || row.assembly_cap == 0
                || row.assembly_cap > (1 << 20) + ACTION_OUTER_ALLOWANCE_V11
                || row.fragment_count_cap != row.assembly_cap.div_ceil(PROTECTED_FRAGMENT_BYTES_V11) as u32
                || row.fragment_count_cap > 64
                || row.action_attempts_used.iter().zip([2, 1, 2]).any(|(n, cap)| *n > cap)
                || row.consumed.checked_add(&row.guarantee_remaining)? != row.committed
            {
                return Err("quota shard paid seal identity/conservation mismatch".into());
            }
            row.committed.within(&policy.plan)?;
            row.fragments.check(row.fragment_count_cap, row.assembly_cap)?;
            if row.admitted_interval == global.epoch.network_interval {
                actors.get(&actor).ok_or("quota shard current seal lost actor")?.reserved.checked_sub(&row.guarantee_remaining)?;
            }
            seal_key = Some(row.key.root());
            protected_seals.insert(row.key.root(), row.clone());
        }
        // Producer and distinct paid seal owner have separate reservations.
        // Authenticated rows remain mandatory; this also rejects a corrupt
        // selected pair whose individual quantities fit but combined do not.
        let mut selected_remaining = QuotaQuantitiesV11::default();
        for row in subjects.values() {
            if row.binding.admitted_interval == global.epoch.network_interval {
                selected_remaining = selected_remaining.checked_add(&row.guarantee_remaining)?;
            }
        }
        for row in protected_seals.values() {
            if row.admitted_interval == global.epoch.network_interval {
                selected_remaining = selected_remaining.checked_add(&row.guarantee_remaining)?;
            }
        }
        selected_remaining.within(&global.network.reserved)?;
        Ok(Self {
            state: QuotaStateV11 {
                policy_root: global.policy_root,
                epoch: global.epoch.clone(),
                block: global.block.clone(),
                network: global.network.clone(),
                actors,
                subjects,
                protected_seals,
            },
            actor,
            selected_job,
            selected_seal,
            actor_was_current,
            retained_actor_seals,
            global_actor_count: global.actor_count,
            global_subject_count: global.subject_count,
            global_seal_count: global.protected_seal_count,
            subject_key,
            seal_key,
        })
    }
    /// Fixed readonly rows to persist in one native journal transition. These
    /// include ALL aggregate counters, not sums of the private partial maps.
    pub fn global_row(&self) -> QuotaGlobalRowV11 {
        QuotaGlobalRowV11 {
            policy_root: self.state.policy_root,
            epoch: self.state.epoch.clone(),
            block: self.state.block.clone(),
            network: self.state.network.clone(),
            actor_count: self.global_actor_count,
            subject_count: self.global_subject_count,
            protected_seal_count: self.global_seal_count,
        }
    }
    pub fn actor_row(&self) -> QuotaActorRowV11 {
        QuotaActorRowV11 {
            actor: self.actor,
            network_interval: self.state.epoch.network_interval,
            active: self.state.actors.contains_key(&self.actor),
            interval: self.state.actors.get(&self.actor).cloned().unwrap_or_default(),
            retained_protected_seals: self.retained_actor_seals,
        }
    }
    pub fn subject_row(&self) -> Option<&QuotaSubjectSummaryV11> {
        self.subject_key.and_then(|k| self.state.subjects.get(&k))
    }
    pub fn seal_row(&self) -> Option<&ProtectedSealSummaryV11> {
        self.seal_key.and_then(|k| self.state.protected_seals.get(&k))
    }
    fn check_new_actor(&self, policy: &QuotaPolicyV11) -> Result<(), String> {
        if !self.actor_was_current && self.global_actor_count >= policy.max_actors {
            return Err("quota shard global actor count exhausted".into());
        }
        Ok(())
    }
    fn account_new_actor(&mut self) {
        if !self.actor_was_current {
            self.global_actor_count += 1;
            self.actor_was_current = true;
        }
    }
    /// Same private native baseline token and same full-book algebra; only
    /// selected fixed rows are copied. Authenticated absent subject is required.
    pub fn admit_baseline(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        admission: &CheckedQuotaAdmissionV11,
        plan: &CheckedJobQuotaPlanV11,
    ) -> Result<QuotaReceiptV11, String> {
        self.state.check_context(policy, epoch)?;
        if admission.binding.job_id != self.selected_job
            || self.subject_key.is_some()
            || self.global_subject_count >= policy.max_subjects
        {
            return Err("quota shard subject already exists/catalog full".into());
        }
        let new_interval = epoch.network_interval != self.state.epoch.network_interval;
        if !new_interval {
            self.check_new_actor(policy)?;
        }
        let receipt = self.state.admit_baseline(policy, epoch, self.actor, admission, plan)?;
        if new_interval {
            self.global_actor_count = 0;
            self.actor_was_current = false;
        }
        self.account_new_actor();
        self.global_subject_count += 1;
        self.subject_key = Some(admission.binding.job_id);
        Ok(receipt)
    }
    /// Producer future actions share the exact full-book ownership/attempt
    /// algebra. Neither a selected absence nor a different job may be borrowed.
    pub fn debit_reserved(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
        action: &DerivedQuotaActionV11,
    ) -> Result<QuotaReceiptV11, String> {
        if self.selected_job != binding.job_id || self.subject_key != Some(binding.job_id) {
            return Err("quota shard reserved action selects a different original subject".into());
        }
        self.state.debit(policy, epoch, self.actor, binding, plan, action)
    }
    pub fn debit_terminal(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
        carrier: &[u8],
    ) -> Result<TerminalDebitV11, String> {
        if self.selected_job != binding.job_id || self.subject_key != Some(binding.job_id) {
            return Err("quota shard terminal selects a different original subject".into());
        }
        self.state.debit_terminal(policy, epoch, self.actor, binding, plan, carrier)
    }
    pub fn mark_terminal_closed(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        terminal: &CheckedQuotaTerminalV11,
        paid: &TerminalDebitV11,
    ) -> Result<(), String> {
        if self.selected_job != terminal.binding.job_id {
            return Err("quota shard terminal native anchor selects a different job".into());
        }
        self.state.mark_terminal_closed(policy, epoch, terminal, paid)
    }
    fn preflight_unreserved_actor(&self, policy: &QuotaPolicyV11, epoch: &TrustedEpochV11) -> Result<bool, String> {
        let (_, new_interval) = self.state.check_context(policy, epoch)?;
        if !new_interval {
            self.check_new_actor(policy)?;
        }
        Ok(new_interval)
    }
    fn account_unreserved_actor(&mut self, new_interval: bool) {
        if new_interval {
            self.global_actor_count = 0;
            self.actor_was_current = false;
        }
        self.account_new_actor();
    }
    pub fn debit_ordinary(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        action: &DerivedOrdinaryMetadataV11,
    ) -> Result<(), String> {
        let new_interval = self.preflight_unreserved_actor(policy, epoch)?;
        self.state.debit_ordinary(policy, epoch, self.actor, action)?;
        self.account_unreserved_actor(new_interval);
        Ok(())
    }
    pub fn debit_public_action(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
        action: &DerivedQuotaActionV11,
    ) -> Result<(), String> {
        if self.selected_job != binding.job_id || self.subject_key != Some(binding.job_id) {
            return Err("quota public shard selects a different job".into());
        }
        let new_interval = self.preflight_unreserved_actor(policy, epoch)?;
        self.state.debit_public_action(policy, epoch, self.actor, binding, plan, action)?;
        self.account_unreserved_actor(new_interval);
        Ok(())
    }
    pub fn debit_expired_terminal(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
    ) -> Result<ExpiredTerminalDebitV11, String> {
        if self.selected_job != binding.job_id {
            return Err("quota expired shard selects a different original job".into());
        }
        let new_interval = self.preflight_unreserved_actor(policy, epoch)?;
        let paid = self.state.debit_expired_terminal(policy, epoch, self.actor, binding, plan)?;
        self.account_unreserved_actor(new_interval);
        Ok(paid)
    }
    pub fn mark_expired_terminal_closed<C: crate::flat_execution11::ExecutionClassContextV11>(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        binding: &SubjectBindingV11,
        class: &C,
        terminal: &crate::flat_execution11::ExecutionJobStateV11,
        paid: &ExpiredTerminalDebitV11,
    ) -> Result<(), String> {
        if self.selected_job != binding.job_id {
            return Err("quota expired mark selects a different original job".into());
        }
        self.state.mark_expired_terminal_closed(policy, epoch, binding, class, terminal, paid)
    }
    pub fn admit_protected_seal(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        seal: &CheckedPaidSealV11,
        plan: &CheckedJobQuotaPlanV11,
    ) -> Result<ProtectedQuotaReceiptV11, String> {
        self.state.check_context(policy, epoch)?;
        self.check_new_actor(policy)?;
        if seal.binding().job_id != self.selected_job
            || self.selected_seal != Some(seal.key().root())
            || self.seal_key.is_some()
            || self.global_seal_count >= PROTECTED_SEALS_MAX_V11
            || self.retained_actor_seals >= PROTECTED_SEALS_PER_ACTOR_V11
        {
            return Err("quota shard protected retained catalog exhausted/duplicate".into());
        }
        let receipt = self.state.admit_protected_seal(policy, epoch, self.actor, seal, plan)?;
        self.account_new_actor();
        self.global_seal_count += 1;
        self.retained_actor_seals += 1;
        self.seal_key = Some(seal.key().root());
        Ok(receipt)
    }
    pub fn debit_protected(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &CheckedJobQuotaPlanV11,
        kind: ProtectedActionKindV11,
        carrier: &[u8],
    ) -> Result<ProtectedQuotaReceiptV11, String> {
        let key = self.seal_key.ok_or("quota shard selected paid seal absent")?;
        self.state.debit_protected(policy, epoch, self.actor, &key, plan, kind, carrier)
    }
    pub fn debit_protected_fragment(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &CheckedJobQuotaPlanV11,
        index: u32,
        count: u32,
        bytes: u64,
        outer: u64,
    ) -> Result<ProtectedFragmentDebitV11, String> {
        let key = self.seal_key.ok_or("quota shard selected paid seal absent")?;
        self.state.debit_protected_fragment(policy, epoch, self.actor, &key, plan, index, count, bytes, outer)
    }
    pub fn accept_protected_fragment(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &CheckedJobQuotaPlanV11,
        paid: &ProtectedFragmentDebitV11,
    ) -> Result<(), String> {
        self.state.accept_protected_fragment(policy, epoch, self.actor, plan, paid)
    }
}
impl QuotaStateV11 {
    /// Reference/full-book projections. Native journals these individual fixed
    /// records directly and must not compute this whole-map projection per proof.
    pub fn global_row(&self) -> QuotaGlobalRowV11 {
        QuotaGlobalRowV11 {
            policy_root: self.policy_root,
            epoch: self.epoch.clone(),
            block: self.block.clone(),
            network: self.network.clone(),
            actor_count: self.actors.len() as u32,
            subject_count: self.subjects.len() as u32,
            protected_seal_count: self.protected_seals.len() as u32,
        }
    }
    pub fn actor_row(&self, actor: &Digest) -> Option<QuotaActorRowV11> {
        let count = self.protected_seals.values().filter(|s| s.key.seal_actor == *actor).count() as u32;
        if !self.actors.contains_key(actor) && count == 0 {
            return None;
        }
        Some(QuotaActorRowV11 {
            actor: *actor,
            network_interval: self.epoch.network_interval,
            active: self.actors.contains_key(actor),
            interval: self.actors.get(actor).cloned().unwrap_or_default(),
            retained_protected_seals: count,
        })
    }
}
