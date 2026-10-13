//! Bounded new29 paid fragment ledger over the SAME frozen21-axis global and
//! actor intervals. New records own their u16/512 semantics; old11 seals,
//! counters, prices, roots and decoders are untouched.
use crate::palw_native_paged_proof_v29::*;
use misaka_palw_kernel::{
    flat_quota11::{
        PROTECTED_SEALS_MAX_V11, PROTECTED_SEALS_PER_ACTOR_V11, QuotaActorRowV11, QuotaGlobalRowV11, QuotaIntervalSummaryV11,
        QuotaPolicyV11, QuotaQuantitiesV11, TrustedEpochV11,
    },
    hash::Digest,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePagedDebitV29 {
    Part(u16),
    Assembly,
    Court,
}
/// Private current native token. Text29/Stage28 adapters construct it only from
/// their authenticated funded book, original maximum and actual paid own seal.
pub struct CheckedNativePagedAdmissionV29 {
    status: NativePagedJobStatusV29,
    seal: NativePagedSealV29,
    epoch: TrustedEpochV11,
}
impl CheckedNativePagedAdmissionV29 {
    pub(crate) fn from_current_native(
        status: NativePagedJobStatusV29,
        seal: NativePagedSealV29,
        plan: &CheckedNativePagedProofPlanV29,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
    ) -> Result<Self, String> {
        epoch.check(quota)?;
        seal.check(plan)?;
        if status.version != 29
            || !status.active
            || status.owners_admitted >= 4
            || status.namespace != seal.namespace
            || status.job != seal.job
            || status.funding != seal.original_funding
            || status.claim != seal.claim
            || status.maximum != seal.maximum_contract
            || status.paid_plan != seal.original_plan
            || status.parameter != seal.real_parameter
            || status.inventory != seal.inventory
            || status.paid_owners.contains(&seal.id())
            || !status.paid_owners.contains(&[0; 64])
            || status.admitted_interval != epoch.network_interval
            || seal.admitted_interval != epoch.network_interval
            || seal.posted != epoch.block_daa
            || seal.admitted_block != epoch.block_hash
            || seal.horizon >= status.deadline
            || seal.horizon >= epoch.interval_end(quota)?
            || seal.posted > status.challenge_end
            || seal.part_attempts.iter().any(|n| *n != 0)
            || seal.accepted != [0; 64]
            || seal.assembly_attempts != 0
            || seal.court_attempts != 0
            || seal.retired
            || seal.spent
        {
            return Err("paged29 current genuine paid book/owner/interval/original maximum differs".into());
        }
        Ok(Self { status, seal, epoch: epoch.clone() })
    }
}
/// Expiry certifies elapsed original liability, not execution correctness.
/// A private native adapter authenticates this book root and owner-specific
/// absence of outstanding demands before constructing it.
pub struct CheckedNativePagedExpiryV29 {
    job: Digest,
    book: Digest,
    funding: Digest,
    claim: Digest,
    seal: Digest,
    now: u64,
}
impl CheckedNativePagedExpiryV29 {
    pub(crate) fn from_current_native(
        status: &NativePagedJobStatusV29,
        seal: &NativePagedSealV29,
        genuine_book: Digest,
        now: u64,
        outstanding_original_units: u32,
    ) -> Result<Self, String> {
        if status.job != seal.job
            || status.funding != seal.original_funding
            || status.claim != seal.claim
            || !status.paid_owners.contains(&seal.id())
            || genuine_book == [0; 64]
            || now <= seal.horizon
            || outstanding_original_units != 0
            || seal.retired
        {
            return Err("paged29 expiry lacks genuine original owner/book/complete pending closure".into());
        }
        Ok(Self { job: status.job, book: genuine_book, funding: status.funding, claim: status.claim, seal: seal.root(), now })
    }
}
pub struct NativePagedQuotaProjectionV29 {
    global: QuotaGlobalRowV11,
    actor: QuotaActorRowV11,
    status: NativePagedJobStatusV29,
    seal: Option<NativePagedSealV29>,
}
fn admit(
    summary: &QuotaIntervalSummaryV11,
    baseline: &QuotaQuantitiesV11,
    held: &QuotaQuantitiesV11,
    cap: &QuotaQuantitiesV11,
) -> Result<QuotaIntervalSummaryV11, String> {
    let s =
        QuotaIntervalSummaryV11 { consumed: summary.consumed.checked_add(baseline)?, reserved: summary.reserved.checked_add(held)? };
    s.consumed.checked_add(&s.reserved)?.within(cap)?;
    Ok(s)
}
fn consume(
    summary: &QuotaIntervalSummaryV11,
    delta: &QuotaQuantitiesV11,
    cap: &QuotaQuantitiesV11,
) -> Result<QuotaIntervalSummaryV11, String> {
    let s = QuotaIntervalSummaryV11 { consumed: summary.consumed.checked_add(delta)?, reserved: summary.reserved.checked_sub(delta)? };
    s.consumed.checked_add(&s.reserved)?.within(cap)?;
    Ok(s)
}
impl NativePagedQuotaProjectionV29 {
    pub fn global(&self) -> &QuotaGlobalRowV11 {
        &self.global
    }
    pub fn actor(&self) -> &QuotaActorRowV11 {
        &self.actor
    }
    pub fn status(&self) -> &NativePagedJobStatusV29 {
        &self.status
    }
    pub fn seal(&self) -> Option<&NativePagedSealV29> {
        self.seal.as_ref()
    }
    pub fn from_native_rows(
        quota: &QuotaPolicyV11,
        plan: &CheckedNativePagedProofPlanV29,
        global: &QuotaGlobalRowV11,
        global_root: Digest,
        actor: &QuotaActorRowV11,
        actor_root: Digest,
        status: &NativePagedJobStatusV29,
        status_root: Digest,
        seal: Option<(&NativePagedSealV29, Digest)>,
    ) -> Result<Self, String> {
        quota.check()?;
        global.epoch.check(quota)?;
        if global.root() != global_root
            || actor.root() != actor_root
            || status.root() != status_root
            || status.version != 29
            || status.owners_admitted > 4
            || status.paid_owners.iter().filter(|id| **id != [0; 64]).count() > status.owners_admitted as usize
            || status.paid_plan != plan.record().root()
            || global.policy_root != quota.root()
            || global.actor_count > quota.max_actors
            || global.protected_seal_count > PROTECTED_SEALS_MAX_V11
            || actor.retained_protected_seals > PROTECTED_SEALS_PER_ACTOR_V11
            || status.paid_owners.iter().filter(|k| **k != [0; 64]).count() > 4
            || status.paid_owners.iter().filter(|k| **k != [0; 64]).any(|k| status.paid_owners.iter().filter(|j| *j == k).count() != 1)
            || global.network.consumed.checked_add(&global.network.reserved)?.within(&quota.network).is_err()
            || global.block.consumed.checked_add(&global.block.reserved)?.within(&quota.block).is_err()
        {
            return Err("paged29 fixed original roots/shared aggregate counters differ".into());
        }
        if actor.network_interval == global.epoch.network_interval {
            actor.interval.consumed.checked_add(&actor.interval.reserved)?.within(&quota.actor)?;
        }
        if let Some((s, root)) = seal {
            s.check(plan)?;
            if s.root() != root
                || s.actor != actor.actor
                || s.job != status.job
                || s.original_funding != status.funding
                || s.claim != status.claim
                || s.maximum_contract != status.maximum
                || s.real_parameter != status.parameter
                || s.inventory != status.inventory
                || s.admitted_interval != status.admitted_interval
                || s.horizon >= status.deadline
                || !status.paid_owners.contains(&s.id())
                || global.protected_seal_count == 0
                || actor.retained_protected_seals == 0
            {
                return Err("paged29 original selected paid owner/status differs".into());
            }
            if s.admitted_interval == global.epoch.network_interval {
                s.remaining.within(&actor.interval.reserved)?;
                s.remaining.within(&global.network.reserved)?;
            }
        }
        Ok(Self { global: global.clone(), actor: actor.clone(), status: status.clone(), seal: seal.map(|(s, _)| s.clone()) })
    }
    fn block(&self, epoch: &TrustedEpochV11) -> QuotaIntervalSummaryV11 {
        if epoch.block_hash == self.global.epoch.block_hash {
            self.global.block.clone()
        } else {
            QuotaIntervalSummaryV11 { consumed: QuotaQuantitiesV11::default(), reserved: self.global.block.reserved.clone() }
        }
    }
    fn current(&self, quota: &QuotaPolicyV11, epoch: &TrustedEpochV11) -> Result<(), String> {
        epoch.check(quota)?;
        if self.global.policy_root != quota.root()
            || epoch.network_interval != self.global.epoch.network_interval
            || epoch.network_interval != self.status.admitted_interval
            || epoch.block_daa < self.global.epoch.block_daa
            || epoch.block_hash == self.global.epoch.block_hash && *epoch != self.global.epoch
        {
            return Err("paged29 current chain epoch rewinds or crosses original reservation".into());
        }
        Ok(())
    }
    /// Legacy counts cover pre11 flat6/7/9 rows only. Native11/13/14/28/29
    /// are already conserved in global/actor21 and must not be counted twice.
    pub fn admit(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &CheckedNativePagedProofPlanV29,
        token: &CheckedNativePagedAdmissionV29,
        legacy_shared_live: u32,
        legacy_actor_live: u32,
    ) -> Result<(), String> {
        self.current(quota, epoch)?;
        if self.seal.is_some()
            || self.status != token.status
            || token.epoch != *epoch
            || token.seal.actor != self.actor.actor
            || self.global.protected_seal_count.checked_add(legacy_shared_live).is_none_or(|n| n >= PROTECTED_SEALS_MAX_V11)
            || self.actor.retained_protected_seals.checked_add(legacy_actor_live).is_none_or(|n| n >= PROTECTED_SEALS_PER_ACTOR_V11)
            || (self.actor.network_interval != epoch.network_interval || !self.actor.active)
                && self.global.actor_count >= quota.max_actors
        {
            return Err("paged29 original paid shared owner/actor/job slots exhausted".into());
        }
        let p = plan.record();
        let block = admit(&self.block(epoch), &p.admission, &p.guarantee, &quota.block)?;
        let network = admit(&self.global.network, &p.admission, &p.guarantee, &quota.network)?;
        let current_actor = if self.actor.network_interval == epoch.network_interval {
            self.actor.interval.clone()
        } else {
            QuotaIntervalSummaryV11::default()
        };
        let actor = admit(&current_actor, &p.admission, &p.guarantee, &quota.actor)?;
        let slot = self.status.paid_owners.iter().position(|k| *k == [0; 64]).ok_or("paged29 original job owners exhausted")?;
        self.status.paid_owners[slot] = token.seal.id();
        self.status.owners_admitted = self.status.owners_admitted.checked_add(1).filter(|n| *n <= 4).ok_or("paged29 original producer lifetime owner guarantee exhausted")?;
        self.status.latest_paid_horizon = self.status.latest_paid_horizon.max(token.seal.horizon);
        if self.actor.network_interval != epoch.network_interval || !self.actor.active {
            self.global.actor_count += 1;
        }
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.global.protected_seal_count += 1;
        self.actor.active = true;
        self.actor.network_interval = epoch.network_interval;
        self.actor.interval = actor;
        self.actor.retained_protected_seals += 1;
        self.seal = Some(token.seal.clone());
        Ok(())
    }
    /// Persist this fixed-row receipt before nested body decoding, any part
    /// clone, assembly allocation, source/class restoration or court callback.
    pub fn debit(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &CheckedNativePagedProofPlanV29,
        actor: Digest,
        kind: NativePagedDebitV29,
    ) -> Result<(), String> {
        self.current(quota, epoch)?;
        let mut seal = self.seal.clone().ok_or("paged29 original paid seal absent")?;
        if actor != seal.actor
            || seal.retired
            || seal.spent
            || seal.discovery
            || epoch.block_daa < seal.matures
            || epoch.block_daa >= seal.reveal_until
            || !self.status.active
        {
            return Err("paged29 original own maturity/window/actor differs".into());
        }
        let p = plan.record();
        let delta = match kind {
            NativePagedDebitV29::Part(index) => {
                if index >= seal.count || seal.part_attempts[index as usize] >= 2 || seal.accepted(index) {
                    return Err("paged29 original part attempts already spent".into());
                }
                seal.part_attempts[index as usize] += 1;
                p.part.clone()
            }
            NativePagedDebitV29::Assembly => {
                if seal.assembly_attempts >= 2 || (0..seal.count).any(|i| !seal.accepted(i)) {
                    return Err("paged29 assembly requires all original paid parts".into());
                }
                seal.assembly_attempts += 1;
                p.assembly.clone()
            }
            NativePagedDebitV29::Court => {
                if seal.court_attempts >= 2 || seal.assembly_attempts == 0 {
                    return Err("paged29 original court attempts exhausted/unassembled".into());
                }
                seal.court_attempts += 1;
                p.court.clone()
            }
        };
        let block = consume(&self.block(epoch), &delta, &quota.block)?;
        let network = consume(&self.global.network, &delta, &quota.network)?;
        let actor = consume(&self.actor.interval, &delta, &quota.actor)?;
        seal.consumed = seal.consumed.checked_add(&delta)?;
        seal.remaining = seal.remaining.checked_sub(&delta)?;
        seal.check(plan)?;
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.actor.interval = actor;
        self.seal = Some(seal);
        Ok(())
    }
    /// Exact private native ticket conversion, never a caller-supplied flag.
    /// It preserves original debt, every source attempt and finite stream clock.
    pub(crate) fn convert_discovery(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &CheckedNativePagedProofPlanV29,
        actor: Digest,
        operator: Digest,
        proof: Digest,
        seal_root: Digest,
        count: u16,
        total: u32,
        new_matures: u64,
    ) -> Result<(), String> {
        self.current(quota, epoch)?;
        let mut seal = self.seal.clone().ok_or("paged29 original discovery row absent")?;
        if !seal.discovery
            || seal.retired
            || seal.spent
            || actor != seal.actor
            || operator != seal.operator
            || epoch.block_daa < seal.matures
            || epoch.block_daa >= seal.reveal_until
            || new_matures <= epoch.block_daa
            || new_matures >= seal.reveal_until
            || count == 0
            || count > plan.record().maximum_parts
            || total == 0
            || total > plan.record().maximum_bytes
            || count as u64 != (total as u64).div_ceil(32 << 10)
            || proof == [0; 64]
            || seal_root == [0; 64]
            || seal.conversion_attempts != 0
        {
            return Err("paged29 conversion rebinds original paid owner/maximum/window or is immature".into());
        }
        let old = seal.id();
        let slot = self.status.paid_owners.iter().position(|k| *k == old).ok_or("paged29 original discovery owner slot absent")?;
        let delta = &plan.record().conversion;
        let block = consume(&self.block(epoch), delta, &quota.block)?;
        let network = consume(&self.global.network, delta, &quota.network)?;
        let owner = consume(&self.actor.interval, delta, &quota.actor)?;
        seal.discovery = false;
        seal.proof_root = proof;
        seal.seal_root = seal_root;
        seal.count = count;
        seal.total_bytes = total;
        seal.matures = new_matures;
        seal.conversion_attempts = 1;
        seal.consumed = seal.consumed.checked_add(delta)?;
        seal.remaining = seal.remaining.checked_sub(delta)?;
        seal.check(plan)?;
        if self.status.paid_owners.contains(&seal.id()) {
            return Err("paged29 converted owner already retained".into());
        }
        self.status.paid_owners[slot] = seal.id();
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.actor.interval = owner;
        self.seal = Some(seal);
        Ok(())
    }
    pub fn expire_owner(
        &mut self,
        quota: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        plan: &CheckedNativePagedProofPlanV29,
        token: &CheckedNativePagedExpiryV29,
    ) -> Result<(), String> {
        epoch.check(quota)?;
        let mut seal = self.seal.clone().ok_or("paged29 expiry original paid seal absent")?;
        seal.check(plan)?;
        if token.job != self.status.job
            || token.funding != self.status.funding
            || token.claim != self.status.claim
            || token.seal != seal.root()
            || token.now != epoch.block_daa
            || token.book == [0; 64]
            || epoch.block_daa <= seal.horizon
            || seal.retired
            || epoch.network_interval < seal.admitted_interval
            || epoch.network_interval != self.global.epoch.network_interval
            || epoch.block_daa < self.global.epoch.block_daa
            || self.global.protected_seal_count == 0
            || self.actor.retained_protected_seals == 0
        {
            return Err("paged29 native expiry rewinds/rebinds original owner liability".into());
        }
        let slot = self.status.paid_owners.iter().position(|k| *k == seal.id()).ok_or("paged29 expiry original owner slot absent")?;
        let (block, network, actor) = if epoch.network_interval == seal.admitted_interval {
            (
                consume(&self.block(epoch), &seal.remaining, &quota.block)?,
                consume(&self.global.network, &seal.remaining, &quota.network)?,
                Some(consume(&self.actor.interval, &seal.remaining, &quota.actor)?),
            )
        } else {
            (self.block(epoch), self.global.network.clone(), None)
        };
        // No reservation is subtracted from a later interval; earlier debt is
        // documentary consumption. Current caller pays fixed+actual cache GC.
        seal.consumed = seal.committed.clone();
        seal.remaining = QuotaQuantitiesV11::default();
        seal.retired = true;
        seal.check(plan)?;
        self.status.paid_owners[slot] = [0; 64];
        self.global.block = block;
        self.global.network = network;
        self.global.epoch = epoch.clone();
        self.global.protected_seal_count -= 1;
        if let Some(a) = actor {
            self.actor.interval = a;
        }
        self.actor.retained_protected_seals -= 1;
        self.seal = Some(seal);
        Ok(())
    }
    /// Body shape is checked AFTER the part debit; malformed nested counts keep
    /// their attempts and cost. Only exact original lengths obtain a cache slot.
    pub fn accept_part(&mut self, plan: &CheckedNativePagedProofPlanV29, index: u16, bytes: usize) -> Result<(), String> {
        let seal = self.seal.as_mut().ok_or("paged29 original seal absent")?;
        if index >= seal.count || seal.part_attempts[index as usize] == 0 || seal.accepted(index) {
            return Err("paged29 original part not paid/duplicate".into());
        }
        let expected = if index + 1 == seal.count { seal.total_bytes as usize - index as usize * (32 << 10) } else { 32 << 10 };
        if bytes != expected {
            return Err("paged29 original exact part length differs".into());
        }
        seal.accepted[index as usize / 8] |= 1 << (index as usize % 8);
        seal.check(plan)
    }
}
