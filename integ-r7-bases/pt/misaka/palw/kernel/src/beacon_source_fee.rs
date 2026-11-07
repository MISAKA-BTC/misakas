//! Prepaid, signed source fees. Existing source contracts/collectors are unchanged.
//! Funding debits the sponsor into a separate escrow: later sponsor slashes cannot consume it.
//! The consumer supplies a dedicated fence and authenticated target/sponsor/minimum; no object supplies those inputs.
use crate::beacon_source::{BeaconSourceOutcomeV1, BeaconSourceTargetV1, BeaconSourceTermsV1, MAX_LIVE_BEACON_SOURCE_SLOTS_V1};
use crate::hash::{Digest, object_id};
use crate::ledger::{KernelLedgerV1, LedgerEventV1, settle};
use crate::settle::SettlementKindV1 as K;
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconSourceFeeTermsV1 {
    pub source: BeaconSourceTermsV1,
    pub sponsor: Digest,
    pub fee_per_source: u64,
    pub max_sources: u32,
}
impl BeaconSourceFeeTermsV1 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/kernel/beacon-source-fee-terms/v1", self)
    }
    pub fn gross(&self) -> Result<u64, String> {
        self.fee_per_source.checked_mul(u64::from(self.max_sources)).ok_or_else(|| "source fee budget overflow".into())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        if self.fee_per_source == 0 || self.max_sources == 0 || self.max_sources > self.source.max_total {
            return Err("invalid source fee or funded capacity".into());
        }
        self.gross()?;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconSourceFeeTargetV1 {
    pub target: BeaconSourceTargetV1,
    pub sponsor: Digest,
    pub minimum_fee: u64,
    pub max_sources: u32,
}
#[derive(Clone, Debug, Default)]
pub struct BeaconSourceFeeInputsV1 {
    pub activation_daa: Option<u64>,
    pub targets: BTreeMap<Digest, BeaconSourceFeeTargetV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconSourceFeeFundingV1 {
    pub target: BeaconSourceTargetV1,
    pub terms: BeaconSourceFeeTermsV1,
    pub funded_daa: u64,
    pub held: u64,
    pub allocated: u32,
    pub paid: u64,
    pub refunded: u64,
    pub unused_refunded: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BeaconSourceFeeOutcomeV1 {
    Paid { daa: u64, claim: Digest, final_daa: u64 },
    Refunded { daa: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BeaconSourceFeePromiseV1 {
    pub terms_id: Digest,
    pub accepted_daa: u64,
    pub fee: u64,
    pub outcome: Option<BeaconSourceFeeOutcomeV1>,
}
impl KernelLedgerV1 {
    pub fn set_beacon_source_fee_inputs(&mut self, inputs: BeaconSourceFeeInputsV1) -> Result<(), String> {
        if inputs.targets.len() > MAX_LIVE_BEACON_SOURCE_SLOTS_V1 as usize {
            return Err("too many fee targets".into());
        }
        for (ctx, t) in &inputs.targets {
            t.target.validate()?;
            if *ctx != t.target.terms.context || t.minimum_fee == 0 || t.max_sources == 0 || t.max_sources > t.target.terms.max_total {
                return Err("invalid authenticated fee target".into());
            }
        }
        self.beacon_source_fee_inputs = inputs;
        Ok(())
    }
    fn fee_gate(&self, context: &Digest) -> Result<&BeaconSourceFeeTargetV1, String> {
        let at = self.beacon_source_fee_inputs.activation_daa.ok_or("source fee fence missing")?;
        if self.daa < at || !self.optimistic_allowed() {
            return Err("source fee fence not reached".into());
        }
        let t = self.beacon_source_fee_inputs.targets.get(context).ok_or("no authenticated source fee target")?;
        if self.beacon_source_inputs.activation_daa.is_none_or(|a| self.daa < a)
            || self.beacon_source_inputs.targets.get(context) != Some(&t.target)
        {
            return Err("source fee needs the independent source fence and the same authenticated target".into());
        }
        if t.target.context.commitment_position < at
            || self.daa < t.target.context.commitment_position
            || self.daa >= t.target.terms.seal_end
        {
            return Err("fee target predates its fence or closed its admission window".into());
        }
        Ok(t)
    }
    pub fn source_fee_escrow_total(&self) -> Result<u64, String> {
        self.beacon_source_fees.values().try_fold(0u64, |n, f| n.checked_add(f.held).ok_or_else(|| "source fee total overflow".into()))
    }
    pub(crate) fn fund_beacon_source_fee(
        &mut self,
        terms: BeaconSourceFeeTermsV1,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), String> {
        terms.validate()?;
        let context = terms.source.context;
        let expected = self.fee_gate(&context)?;
        if terms.source != expected.target.terms
            || terms.sponsor != expected.sponsor
            || terms.fee_per_source < expected.minimum_fee
            || terms.max_sources > expected.max_sources
            || (expected.target.context.policy.is_paid_opt_in_source()
                && terms.max_sources < expected.target.context.policy.work_count_k)
        {
            return Err("fee terms differ from the authenticated sponsor/minimum/cap/target".into());
        }
        if self.beacon_source_fees.contains_key(&context) {
            return Err("source fee contract already funded".into());
        }
        if self.beacon_source_fees.values().filter(|f| f.held > 0).count() >= MAX_LIVE_BEACON_SOURCE_SLOTS_V1 as usize {
            return Err("live source fee funding cap spent".into());
        }
        let amount = terms.gross()?;
        self.source_fee_escrow_total()?.checked_add(amount).ok_or("source fee total overflow")?;
        let b = self.bonds.get(&terms.sponsor).ok_or("fee sponsor bond missing")?;
        if b.exit_requested.is_some() || b.free() < amount {
            return Err("fee sponsor exiting or lacks free collateral".into());
        }
        let target = expected.target.clone();
        self.bonds.get_mut(&terms.sponsor).expect("checked").collateral -= amount;
        self.beacon_source_fees.insert(
            context,
            BeaconSourceFeeFundingV1 {
                target,
                terms,
                funded_daa: self.daa,
                held: amount,
                allocated: 0,
                paid: 0,
                refunded: 0,
                unused_refunded: false,
            },
        );
        // This is an escrow transfer, not a burn and not source income.
        settle(out, terms.sponsor, amount, K::FundSourceFeeEscrow, Some(context));
        Ok(())
    }
    pub(crate) fn opt_in_paid_beacon_source(
        &mut self,
        producer: Digest,
        job: Digest,
        seal: Digest,
        context: Digest,
        source_terms: Digest,
        fee_terms: Digest,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), String> {
        let expected = self.fee_gate(&context)?;
        let funding = self.beacon_source_fees.get(&context).ok_or("source fee not prepaid")?;
        if funding.terms.id() != fee_terms
            || funding.terms.source.id() != source_terms
            || funding.terms.source != expected.target.terms
            || funding.terms.sponsor != expected.sponsor
            || funding.terms.fee_per_source < expected.minimum_fee
            || funding.terms.max_sources > expected.max_sources
            || funding.unused_refunded
            || funding.allocated >= funding.terms.max_sources
            || self.beacon_source_fee_promises.contains_key(&(context, producer))
        {
            return Err("source fee contract substituted, closed, or exhausted".into());
        }
        let fee = funding.terms.fee_per_source;
        let pending = self
            .beacon_source_fee_promises
            .range((context, [0; 64])..=(context, [255; 64]))
            .filter(|(_, p)| p.outcome.is_none())
            .try_fold(fee, |n, (_, p)| n.checked_add(p.fee).ok_or("fee allocation overflow"))?;
        if pending > funding.held {
            return Err("source fee escrow does not back the minimum receipt".into());
        }
        self.admit_beacon_source(producer, job, seal, context, source_terms, out)?;
        self.beacon_source_fees.get_mut(&context).expect("checked").allocated += 1;
        self.beacon_source_fee_promises
            .insert((context, producer), BeaconSourceFeePromiseV1 { terms_id: fee_terms, accepted_daa: self.daa, fee, outcome: None });
        Ok(())
    }
    pub(crate) fn tick_beacon_source_fees(&mut self, out: &mut Vec<LedgerEventV1>) {
        let due: Vec<_> = self
            .beacon_source_fee_promises
            .iter()
            .filter(|(_, p)| p.outcome.is_none())
            .filter_map(|(key, _)| self.beacon_sources.get(key)?.outcome.map(|s| (*key, s)))
            .collect();
        for (key, source_outcome) in due {
            let promise = self.beacon_source_fee_promises.get_mut(&key).expect("listed");
            let f = self.beacon_source_fees.get_mut(&key.0).expect("validated");
            assert!(f.held >= promise.fee);
            f.held -= promise.fee;
            match source_outcome {
                BeaconSourceOutcomeV1::Refunded { final_daa, .. } => {
                    let claim = self.beacon_sources[&key].revealed.expect("standing Final").0;
                    promise.outcome = Some(BeaconSourceFeeOutcomeV1::Paid { daa: self.daa, claim, final_daa });
                    f.paid += promise.fee;
                    settle(out, key.1, promise.fee, K::PaySourceFeeEscrow, Some(key.0));
                }
                BeaconSourceOutcomeV1::Forfeited { .. } => {
                    promise.outcome = Some(BeaconSourceFeeOutcomeV1::Refunded { daa: self.daa });
                    f.refunded += promise.fee;
                    settle(out, f.terms.sponsor, promise.fee, K::RefundSourceFeeEscrow, Some(key.0));
                }
            }
        }
        for (context, f) in &mut self.beacon_source_fees {
            if !f.unused_refunded && self.daa >= f.terms.source.seal_end {
                let unused = u64::from(f.terms.max_sources - f.allocated) * f.terms.fee_per_source;
                assert!(f.held >= unused);
                f.held -= unused;
                f.refunded += unused;
                f.unused_refunded = true;
                if unused > 0 {
                    settle(out, f.terms.sponsor, unused, K::RefundSourceFeeEscrow, Some(*context));
                }
            }
        }
    }
    /// Both books are rooted, and money is conserved independently of the sponsor's current bond balance.
    pub fn validate_beacon_source_fees(&self) -> Result<(), String> {
        self.source_fee_escrow_total()?;
        for (context, f) in &self.beacon_source_fees {
            f.target.validate()?;
            f.terms.validate()?;
            if f.target.context.policy.is_paid_opt_in_source()
                && self
                    .beacon_sources
                    .range((*context, [0; 64])..=(*context, [255; 64]))
                    .any(|(key, _)| !self.beacon_source_fee_promises.contains_key(key))
            {
                return Err("mandatory source fee promise missing".into());
            }
            if *context != f.terms.source.context
                || f.terms.source != f.target.terms
                || f.funded_daa < f.target.context.commitment_position
                || f.funded_daa >= f.terms.source.seal_end
                || f.funded_daa > self.daa
                || f.allocated > f.terms.max_sources
                || (f.paid as u128 + f.refunded as u128 + f.held as u128) != f.terms.gross()? as u128
                || (f.unused_refunded && self.daa < f.terms.source.seal_end)
                || (!f.unused_refunded && self.daa >= f.terms.source.seal_end)
            {
                return Err("corrupt source fee funding, clocks or backing".into());
            }
            let (mut count, mut paid, mut refunded, mut pending) = (0u32, 0u64, 0u64, 0u64);
            for ((_, producer), p) in self.beacon_source_fee_promises.range((*context, [0; 64])..=(*context, [255; 64])) {
                let source = self.beacon_sources.get(&(*context, *producer)).ok_or("fee promise has no source")?;
                if p.terms_id != f.terms.id()
                    || p.fee != f.terms.fee_per_source
                    || source.terms != f.terms.source
                    || p.accepted_daa != source.opted_in_daa
                    || p.accepted_daa < f.funded_daa
                    || p.accepted_daa > self.daa
                {
                    return Err("corrupt source fee promise binding".into());
                }
                count = count.checked_add(1).ok_or("too many source fee promises")?;
                match (p.outcome, source.outcome) {
                    (None, None) => pending = pending.checked_add(p.fee).ok_or("pending source fee overflow")?,
                    (
                        Some(BeaconSourceFeeOutcomeV1::Paid { daa, claim, final_daa }),
                        Some(BeaconSourceOutcomeV1::Refunded { daa: s, final_daa: sf }),
                    ) if daa >= s
                        && daa <= self.daa
                        && daa > f.terms.source.hold_until
                        && final_daa == sf
                        && source.revealed.is_some_and(|r| r.0 == claim) =>
                    {
                        paid = paid.checked_add(p.fee).ok_or("paid source fee overflow")?;
                    }
                    (Some(BeaconSourceFeeOutcomeV1::Refunded { daa }), Some(BeaconSourceOutcomeV1::Forfeited { daa: s }))
                        if daa >= s && daa <= self.daa =>
                    {
                        refunded = refunded.checked_add(p.fee).ok_or("refunded source fee overflow")?;
                    }
                    _ => return Err("fee settlement is not the source's standing Final/forfeit".into()),
                }
            }
            if count > f.terms.max_sources {
                return Err("too many source fee promises".into());
            }
            let unused = u64::from(f.terms.max_sources - count) * f.terms.fee_per_source;
            if count != f.allocated
                || paid != f.paid
                || refunded.checked_add(if f.unused_refunded { unused } else { 0 }) != Some(f.refunded)
                || pending.checked_add(if f.unused_refunded { 0 } else { unused }) != Some(f.held)
            {
                return Err("fee promise inventory differs from prepaid escrow".into());
            }
        }
        for key in self.beacon_source_fee_promises.keys() {
            if !self.beacon_source_fees.contains_key(&key.0) {
                return Err("orphan source fee promise".into());
            }
        }
        Ok(())
    }
}
