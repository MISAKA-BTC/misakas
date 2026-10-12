//! Dormant per-attempt source opt-in. Ordinary job-race seals do not reserve this deposit.
//! The consumer must supply its own activation fence and authenticated, frozen target contexts.
//! These hooks do not approve a beacon policy, fund source fees, or connect a node's beacon collector.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::{BeaconContextV1, RootV1, SubjectKindV1};

use crate::hash::{Digest, object_id};
use crate::ledger::{KernelLedgerV1, LedgerEventV1, settle};
use crate::lifecycle::ClaimStateV1;
use crate::settle::SettlementKindV1;

/// Binary resource ceiling, not a deployed economic policy or lifetime soundness budget.
pub const MAX_LIVE_BEACON_SOURCE_SLOTS_V1: u32 = 65_536;

/// All money and timing terms are frozen before opt-in and signed by their digest in inner tag 40.
/// `hold_until` includes the target's entire source liability/material/court horizon, as derived by the consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconSourceTermsV1 {
    pub context: Digest,
    pub seal_start: u64,
    pub seal_end: u64,
    pub reveal_end: u64,
    pub final_by: u64,
    pub hold_until: u64,
    pub deposit: u64,
    pub max_per_producer: u32,
    pub max_total: u32,
}

impl BeaconSourceTermsV1 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/kernel/beacon-source-terms/v1", self)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(self.seal_start < self.seal_end
            && self.seal_end < self.reveal_end
            && self.reveal_end <= self.final_by
            && self.final_by <= self.hold_until)
            || self.hold_until == u64::MAX
            || self.deposit == 0
            || self.max_per_producer == 0
            || self.max_per_producer > self.max_total
            || self.max_total > MAX_LIVE_BEACON_SOURCE_SLOTS_V1
        {
            return Err("invalid source clocks, deposit or concurrency caps".into());
        }
        Ok(())
    }
}

/// A target is trusted input, like Panel coverage: no transaction can create one.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconSourceTargetV1 {
    pub context: BeaconContextV1,
    pub terms: BeaconSourceTermsV1,
}

impl BeaconSourceTargetV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.context.policy.validate().map_err(|e| e.to_string())?;
        self.terms.validate()?;
        if matches!(self.context.subject_kind, SubjectKindV1::ModelConformance | SubjectKindV1::KernelConformance)
            && self.context.candidate_profile_id == RootV1::Absent
        {
            return Err("a source conformance context must bind its candidate".into());
        }
        if !self.context.policy.is_sealed_source() && !self.context.policy.is_opt_in_source() {
            return Err("source target needs a sealed-source context".into());
        }
        let start =
            self.context.commitment_position.checked_add(self.context.policy.anchor_delay_slots).ok_or("source start overflow")?;
        let seal_end = start.checked_add(self.context.policy.beacon_window_slots).ok_or("source seal window overflow")?;
        let reveal_end = seal_end.checked_add(self.context.policy.beacon_window_slots).ok_or("source reveal window overflow")?;
        if self.terms.context != self.context.id()
            || (self.terms.seal_start, self.terms.seal_end, self.terms.reveal_end) != (start, seal_end, reveal_end)
        {
            return Err("source terms do not bind the frozen context and windows".into());
        }
        Ok(())
    }
}

/// Ephemeral, absent on genesis and row restore. Never copied from object bytes.
#[derive(Clone, Debug, Default)]
pub struct BeaconSourceInputsV1 {
    pub activation_daa: Option<u64>,
    pub targets: BTreeMap<Digest, BeaconSourceTargetV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BeaconSourceOutcomeV1 {
    Forfeited { daa: u64 },
    Refunded { daa: u64, final_daa: u64 },
}

/// `(context, producer)` is consumed permanently, even after failure/refund. Never replace a mixed seal.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconSourceRowV1 {
    pub producer: Digest,
    pub job: Digest,
    pub profile: Digest,
    pub poster: Digest,
    pub seal: Digest,
    pub sealed_daa: u64,
    pub opted_in_daa: u64,
    pub terms: BeaconSourceTermsV1,
    pub held: u64,
    pub revealed: Option<(Digest, u64)>,
    pub outcome: Option<BeaconSourceOutcomeV1>,
}

impl KernelLedgerV1 {
    /// Called anew by the consumer after every load, using rooted target state. Validation is transactional.
    pub fn set_beacon_source_inputs(&mut self, inputs: BeaconSourceInputsV1) -> Result<(), String> {
        if inputs.targets.len() > MAX_LIVE_BEACON_SOURCE_SLOTS_V1 as usize {
            return Err("too many source targets".into());
        }
        for (id, target) in &inputs.targets {
            target.validate()?;
            if *id != target.terms.context {
                return Err("source target key mismatch".into());
            }
        }
        self.beacon_source_inputs = inputs;
        Ok(())
    }

    pub(crate) fn opt_in_beacon_source(
        &mut self,
        producer: Digest,
        job: Digest,
        seal: Digest,
        context: Digest,
        terms_id: Digest,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), String> {
        if self.beacon_source_inputs.targets.get(&context).is_some_and(|t| t.context.policy.is_paid_opt_in_source()) {
            return Err("this source policy requires a signed prepaid fee promise".into());
        }
        self.admit_beacon_source(producer, job, seal, context, terms_id, out)
    }

    /// Internal shared admission; only the prepaid route may call this for v5.
    pub(crate) fn admit_beacon_source(
        &mut self,
        producer: Digest,
        job: Digest,
        seal: Digest,
        context: Digest,
        terms_id: Digest,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), String> {
        let inputs = &self.beacon_source_inputs;
        if inputs.activation_daa.is_none_or(|at| self.daa < at) || !self.optimistic_allowed() {
            return Err("the dedicated beacon source fence is not reached".into());
        }
        let target = inputs.targets.get(&context).ok_or("no authenticated source target")?;
        // Validate here as well: consumers can hold this input struct directly.
        target.validate()?;
        let terms = target.terms;
        let at = inputs.activation_daa.expect("gate checked");
        if target.context.commitment_position < at {
            return Err("a pre-fence target cannot acquire the new source contract".into());
        }
        if terms.id() != terms_id || self.beacon_sources.contains_key(&(context, producer)) {
            return Err("source terms mismatch or this producer already opted in to this target".into());
        }
        let ordinary = self.seals.get(&(job, producer)).ok_or("source needs a live ordinary seal")?;
        if ordinary.seal != seal
            || ordinary.daa < at
            || ordinary.daa < terms.seal_start
            || ordinary.daa >= terms.seal_end
            || self.daa < ordinary.daa
            || self.daa >= terms.seal_end
            || ordinary.daa.checked_add(self.policy.seal_ttl_daa).is_none_or(|end| end < terms.reveal_end - 1)
        {
            return Err("source seal is outside its target window, expired, or does not match".into());
        }
        // These are the two source job forms currently exported by the node. Other forms require their own adapter.
        let profile = self
            .jobs
            .get(&job)
            .map(|j| j.class_binding_id)
            .or_else(|| self.pipeline_jobs.get(&job).map(|j| j.class_binding_id))
            .ok_or("unsupported source job form")?;
        if !self.opv.classes.contains(&profile)
            || !target.context.eligible_profiles.contains(&profile)
            || target.context.excluded_profiles.contains(&profile)
            || target.context.candidate_profile_id == RootV1::Present(profile)
        {
            return Err("source profile is not independent eligible OPV work".into());
        }
        let poster = self.job_poster(&job).ok_or("source needs an authenticated job payer")?;
        let bond = self.bonds.get(&producer).ok_or("source producer bond is missing")?;
        if bond.exit_requested.is_some() || bond.free() < terms.deposit {
            return Err("source producer is exiting or lacks free source collateral".into());
        }
        let mut total = 0usize;
        let mut producer_count = 0usize;
        let mut total_cap = terms.max_total;
        let mut producer_cap = terms.max_per_producer;
        let mut reveal_start = terms.seal_end;
        let mut reveal_end = terms.reveal_end;
        for row in self.beacon_sources.values().filter(|r| self.daa <= r.terms.hold_until) {
            // Failure/refund does not free a slot ahead of its signed horizon.
            total += 1;
            total_cap = total_cap.min(row.terms.max_total);
            if row.producer == producer {
                producer_count += 1;
                producer_cap = producer_cap.min(row.terms.max_per_producer);
                if row.job == job && row.outcome.is_none() && row.revealed.is_none() {
                    reveal_start = reveal_start.max(row.terms.seal_end);
                    reveal_end = reveal_end.min(row.terms.reveal_end);
                }
            }
        }
        if total >= total_cap as usize || producer_count >= producer_cap as usize {
            return Err("source concurrency cap is spent".into());
        }
        if reveal_start >= reveal_end {
            return Err("source targets have no common reveal window".into());
        }
        self.bonds.get_mut(&producer).expect("checked").reserved += terms.deposit;
        self.beacon_sources.insert(
            (context, producer),
            BeaconSourceRowV1 {
                producer,
                job,
                profile,
                poster,
                seal,
                sealed_daa: ordinary.daa,
                opted_in_daa: self.daa,
                terms,
                held: terms.deposit,
                revealed: None,
                outcome: None,
            },
        );
        // These existing kinds represent the same C/R effects; the source row identifies the separate reservation.
        settle(out, producer, terms.deposit, SettlementKindV1::ReserveSealDeposit, None);
        Ok(())
    }

    pub(crate) fn beacon_source_reveal_ready(&self, job: Digest, producer: Digest, salted: bool) -> Result<(), String> {
        for row in self.beacon_sources.values().filter(|r| r.job == job && r.producer == producer && r.outcome.is_none()) {
            if !salted || self.daa < row.terms.seal_end || self.daa >= row.terms.reveal_end {
                return Err("source reveal must be salted and in every opted-in target's reveal window".into());
            }
        }
        Ok(())
    }

    pub(crate) fn beacon_source_liability_floor(&self, job: Digest, producer: Digest) -> Option<u64> {
        self.beacon_sources
            .values()
            .filter(|r| r.job == job && r.producer == producer && r.outcome.is_none())
            .map(|r| r.terms.hold_until)
            .max()
    }

    pub(crate) fn beacon_source_revealed(&mut self, job: Digest, producer: Digest, claim: Digest) {
        for row in self.beacon_sources.values_mut().filter(|r| r.job == job && r.producer == producer && r.outcome.is_none()) {
            row.revealed = Some((claim, self.daa));
        }
    }

    pub(crate) fn beacon_source_seal_forfeited(
        &mut self,
        job: Digest,
        producer: Digest,
        seal: Digest,
        sealed_daa: u64,
        out: &mut Vec<LedgerEventV1>,
    ) {
        let keys: Vec<_> = self
            .beacon_sources
            .iter()
            .filter(|(_, r)| {
                r.job == job && r.producer == producer && r.seal == seal && r.sealed_daa == sealed_daa && r.outcome.is_none()
            })
            .map(|(k, _)| *k)
            .collect();
        for key in keys {
            self.settle_beacon_source(key, true, out);
        }
    }

    fn settle_beacon_source(&mut self, key: (Digest, Digest), forfeit: bool, out: &mut Vec<LedgerEventV1>) {
        let row = self.beacon_sources.get_mut(&key).expect("listed");
        if row.outcome.is_some() {
            return;
        }
        let amount = std::mem::take(&mut row.held);
        row.outcome = Some(if forfeit {
            BeaconSourceOutcomeV1::Forfeited { daa: self.daa }
        } else {
            let id = row.revealed.expect("only a standing Final refunds").0;
            let ClaimStateV1::Final { final_daa } = self.claims[&id].life.state else { unreachable!("checked by tick") };
            BeaconSourceOutcomeV1::Refunded { daa: self.daa, final_daa }
        });
        let claim = row.revealed.map(|r| r.0);
        let producer = row.producer;
        let bond = self.bonds.get_mut(&producer).expect("held source collateral has a bond");
        // A source deposit is separately reserved: claim slashes cannot consume it.
        assert!(bond.reserved >= amount && bond.collateral >= amount);
        bond.reserved -= amount;
        if forfeit {
            bond.collateral -= amount;
            self.burned += amount;
            settle(out, producer, amount, SettlementKindV1::ForfeitSealDeposit, claim);
            settle(out, producer, amount, SettlementKindV1::Burn, claim);
        } else {
            settle(out, producer, amount, SettlementKindV1::ReleaseSealDeposit, claim);
        }
    }

    pub(crate) fn tick_beacon_sources(&mut self, out: &mut Vec<LedgerEventV1>) {
        let mut due = Vec::new();
        for (key, row) in &self.beacon_sources {
            if row.outcome.is_some() {
                continue;
            }
            let (failed, standing_final) = match row.revealed {
                None => (self.daa >= row.terms.reveal_end, false),
                Some((id, _)) => match self.claims.get(&id) {
                    None => (true, false),
                    Some(c) => {
                        let final_on_time =
                            matches!(c.life.state, ClaimStateV1::Final { final_daa } if final_daa <= row.terms.final_by);
                        let failed = c.convicted
                            || matches!(
                                c.life.state,
                                ClaimStateV1::Convicted { .. } | ClaimStateV1::Unavailable { .. } | ClaimStateV1::TimedOut { .. }
                            )
                            || (self.daa > row.terms.final_by && !final_on_time);
                        (failed, final_on_time)
                    }
                },
            };
            if failed {
                due.push((*key, true));
            } else if standing_final && self.daa > row.terms.hold_until {
                due.push((*key, false));
            }
        }
        for (key, forfeit) in due {
            self.settle_beacon_source(key, forfeit, out);
        }
    }

    /// Restoring a row never restores permission to accept new opt-ins. Existing holds still settle by their signed terms.
    pub fn validate_beacon_sources(&self) -> Result<(), String> {
        let mut held_by_bond = BTreeMap::<Digest, u64>::new();
        let mut active_by_bond = BTreeMap::<Digest, (usize, u32)>::new();
        let mut active_total = 0usize;
        let mut total_cap = MAX_LIVE_BEACON_SOURCE_SLOTS_V1;
        for ((context, producer), row) in &self.beacon_sources {
            row.terms.validate()?;
            if *context != row.terms.context
                || *producer != row.producer
                || row.sealed_daa < row.terms.seal_start
                || row.opted_in_daa < row.sealed_daa
                || row.opted_in_daa >= row.terms.seal_end
                || row.opted_in_daa > self.daa
                || self.job_poster(&row.job) != Some(row.poster)
                || (row.outcome.is_none() && row.held != row.terms.deposit)
                || (row.outcome.is_some() && row.held != 0)
            {
                return Err("corrupt source binding, clocks or reservation".into());
            }
            let profile = self
                .jobs
                .get(&row.job)
                .map(|j| j.class_binding_id)
                .or_else(|| self.pipeline_jobs.get(&row.job).map(|j| j.class_binding_id));
            if profile != Some(row.profile) {
                return Err("source job profile mismatch".into());
            }
            if let Some((claim, revealed_daa)) = row.revealed {
                let c = self.claims.get(&claim).ok_or("source reveal has no claim")?;
                if c.job_id != row.job
                    || c.class_binding_id != row.profile
                    || c.producer != row.producer
                    || c.sealed_daa != row.sealed_daa
                    || c.committed_daa != revealed_daa
                    || revealed_daa < row.terms.seal_end
                    || revealed_daa >= row.terms.reveal_end
                    || revealed_daa > self.daa
                    || self.claim_beacon_salts.get(&claim).is_none_or(|salt| crate::route::claim_seal_v2(&claim, salt) != row.seal)
                    || (row.held > 0 && c.liability_until.is_none_or(|until| until < row.terms.hold_until))
                {
                    return Err("source reveal or liability binding mismatch".into());
                }
            } else if row.outcome.is_none()
                && self.seals.get(&(row.job, row.producer)).is_none_or(|s| s.seal != row.seal || s.daa != row.sealed_daa)
            {
                return Err("live source has no matching seal".into());
            }
            match row.outcome {
                Some(BeaconSourceOutcomeV1::Forfeited { daa }) if daa < row.opted_in_daa || daa > self.daa => {
                    return Err("source forfeit clock mismatch".into());
                }
                Some(BeaconSourceOutcomeV1::Refunded { daa, final_daa }) => {
                    let (claim, revealed_daa) = row.revealed.ok_or("refunded source has no reveal")?;
                    if daa <= row.terms.hold_until
                        || daa > self.daa
                        || final_daa < revealed_daa
                        || final_daa > row.terms.final_by
                        || final_daa > daa
                    {
                        return Err("source refund clock mismatch".into());
                    }
                    // Another source contract may extend the same claim's liability past this refund.
                    // A later conviction/default is valid; an earlier one cannot authorize a refund.
                    match self.claims[&claim].life.state {
                        ClaimStateV1::Final { final_daa: actual } if actual == final_daa => {}
                        ClaimStateV1::Convicted { daa: failed } | ClaimStateV1::Unavailable { daa: failed, .. } if failed > daa => {}
                        _ => return Err("source refund has no standing or subsequently failed Final".into()),
                    }
                }
                _ => {}
            }
            if self.daa <= row.terms.hold_until {
                active_total += 1;
                total_cap = total_cap.min(row.terms.max_total);
                let (count, cap) = active_by_bond.entry(row.producer).or_insert((0, row.terms.max_per_producer));
                *count += 1;
                *cap = (*cap).min(row.terms.max_per_producer);
            }
            let sum = held_by_bond.entry(row.producer).or_default();
            *sum = sum.checked_add(row.held).ok_or("source reservation overflow")?;
        }
        if active_total > total_cap as usize || active_by_bond.values().any(|(count, cap)| *count > *cap as usize) {
            return Err("restored source concurrency exceeds a frozen cap".into());
        }
        for (producer, amount) in held_by_bond {
            if amount > 0 && self.bonds.get(&producer).is_none_or(|b| amount > b.reserved || b.reserved > b.collateral) {
                return Err("source reservation is not held by the bond".into());
            }
        }
        Ok(())
    }
}
