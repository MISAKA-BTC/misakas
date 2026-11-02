//! Explicit finite RFC-0046 preimages; no network defaults and no native lease.
use super::funding_relay::{FUNDING_RELAY_MAX_BODY_BYTES_V2, FundingRelayTermsV2};
use crate::{
    ContractError,
    identity::{Digest, hash},
};
use borsh::{BorshDeserialize, BorshSerialize};

fn finite(values: &[u64]) -> bool {
    values.iter().all(|n| *n > 0 && *n < u64::MAX)
}
fn sum(values: impl IntoIterator<Item = u64>) -> Result<u64, ContractError> {
    values.into_iter().try_fold(0u64, |a, b| a.checked_add(b)).ok_or(ContractError::Budget)
}
fn mul(a: u64, b: u64) -> Result<u64, ContractError> {
    a.checked_mul(b).ok_or(ContractError::Budget)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayPolicyV2 {
    pub version: u16,
    pub revision: u16,
    pub max_pending_global: u32,
    pub max_pending_per_poster: u32,
    pub max_pending_per_prover: u32,
    pub max_account_candidates: u32,
    pub max_account_continuations: u32,
    pub max_stored_rows: u32,
    pub max_declared_amount: u64,
    pub register_window_daa: u64,
    pub funding_window_daa: u64,
    pub custody_window_daa: u64,
    pub rollback_margin_daa: u64,
    pub metadata_fee_schedule: Digest,
    pub physical_custody_profile: Digest,
}
impl FundingRelayPolicyV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/funding-relay-policy/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.version != 2 || self.revision != 1 {
            return Err(ContractError::Version);
        }
        if [
            self.max_pending_global,
            self.max_pending_per_poster,
            self.max_pending_per_prover,
            self.max_account_candidates,
            self.max_account_continuations,
            self.max_stored_rows,
        ]
        .into_iter()
        .any(|n| n == 0 || n == u32::MAX)
            || !finite(&[
                self.max_declared_amount,
                self.register_window_daa,
                self.funding_window_daa,
                self.custody_window_daa,
                self.rollback_margin_daa,
            ])
            || self.max_pending_per_poster > self.max_pending_global
            || self.max_pending_per_prover > self.max_pending_global
            || self.max_account_candidates > self.max_pending_global
            || self.max_stored_rows < 2
            || self.metadata_fee_schedule == Digest([0; 64])
            || self.physical_custody_profile == Digest([0; 64])
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
    pub fn check_terms(&self, terms: &FundingRelayTermsV2, now: u64) -> Result<(), ContractError> {
        self.validate()?;
        terms.validate_shape()?;
        if terms.relay_policy != self.id()
            || terms.metadata_fee_schedule != self.metadata_fee_schedule
            || terms.physical_custody_profile != self.physical_custody_profile
        {
            return Err(ContractError::Identity);
        }
        if terms.expected_amount > self.max_declared_amount
            || now >= terms.register_before_daa
            || terms.register_before_daa.checked_sub(now).is_none_or(|n| n > self.register_window_daa)
            || terms.funding_admission_before_daa.checked_sub(now).is_none_or(|n| n > self.funding_window_daa)
            || terms.custody_until_daa.checked_sub(now).is_none_or(|n| n > self.custody_window_daa)
            || terms.funding_admission_before_daa.checked_add(self.rollback_margin_daa).is_none_or(|n| n > terms.custody_until_daa)
        {
            return Err(ContractError::Range);
        }
        Ok(())
    }
    /// Immutable maximum retention interval over ALL eligible inclusion DAAs.
    /// It is a signed reservation quote, never an earned service receipt.
    pub fn retention_quote(&self, terms: &FundingRelayTermsV2) -> Result<u64, ContractError> {
        self.validate()?;
        terms.validate_shape()?;
        let lower = |deadline: u64, window: u64| deadline.checked_sub(window).unwrap_or_default();
        let earliest = lower(terms.register_before_daa, self.register_window_daa)
            .max(lower(terms.funding_admission_before_daa, self.funding_window_daa))
            .max(lower(terms.custody_until_daa, self.custody_window_daa));
        if earliest >= terms.register_before_daa {
            return Err(ContractError::Range);
        }
        terms.custody_until_daa.checked_sub(earliest).ok_or(ContractError::Range)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayPhysicalProfileV2 {
    pub version: u16,
    pub revision: u16,
    pub max_frame_bytes: u64,
    pub max_decoded_bytes: u64,
    pub max_heap_bytes: u64,
    pub max_retained_bytes: u64,
    pub max_database_bytes: u64,
    pub max_batch_bytes: u64,
    pub max_replay_reads: u64,
    pub max_replay_work: u64,
    pub max_signatures: u32,
}
impl FundingRelayPhysicalProfileV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/funding-relay-physical/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.version != 2 || self.revision != 1 {
            return Err(ContractError::Version);
        }
        if !finite(&[
            self.max_frame_bytes,
            self.max_decoded_bytes,
            self.max_heap_bytes,
            self.max_retained_bytes,
            self.max_database_bytes,
            self.max_batch_bytes,
            self.max_replay_reads,
            self.max_replay_work,
        ]) || self.max_signatures != 2
            || self.max_frame_bytes < FUNDING_RELAY_MAX_BODY_BYTES_V2 as u64
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
    pub fn check(&self, usage: &FundingRelayUsageV2) -> Result<(), ContractError> {
        self.validate()?;
        if usage.signatures != 2 {
            return Err(ContractError::Budget);
        }
        for (used, limit) in [
            (usage.frame_bytes, self.max_frame_bytes),
            (usage.decoded_bytes, self.max_decoded_bytes),
            (usage.heap_bytes, self.max_heap_bytes),
            (usage.retained_bytes, self.max_retained_bytes),
            (usage.database_bytes, self.max_database_bytes),
            (usage.batch_bytes, self.max_batch_bytes),
            (usage.replay_reads, self.max_replay_reads),
            (usage.replay_work, self.max_replay_work),
        ] {
            if used == 0 || used > limit {
                return Err(ContractError::Budget);
            }
        }
        Ok(())
    }
}

/// Counts from concrete native typed construction/staging, not remote caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayUsageV2 {
    pub frame_bytes: u64,
    pub decoded_bytes: u64,
    pub heap_bytes: u64,
    pub retained_bytes: u64,
    pub database_bytes: u64,
    pub batch_bytes: u64,
    pub replay_reads: u64,
    pub replay_work: u64,
    pub signatures: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayFeeScheduleV2 {
    pub version: u16,
    pub revision: u16,
    pub admission: u64,
    pub per_signature: u64,
    pub per_frame_byte: u64,
    pub per_retained_byte_daa: u64,
    pub per_database_byte: u64,
    pub per_replay_read: u64,
    pub per_replay_work: u64,
    pub retirement: u64,
}
impl FundingRelayFeeScheduleV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/funding-relay-fees/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.version != 2 || self.revision != 1 {
            return Err(ContractError::Version);
        }
        if !finite(&[
            self.admission,
            self.per_signature,
            self.per_frame_byte,
            self.per_retained_byte_daa,
            self.per_database_byte,
            self.per_replay_read,
            self.per_replay_work,
            self.retirement,
        ]) {
            return Err(ContractError::Budget);
        }
        Ok(())
    }
    pub fn quote(
        &self,
        policy: &FundingRelayPolicyV2,
        physical: &FundingRelayPhysicalProfileV2,
        terms: &FundingRelayTermsV2,
        usage: FundingRelayUsageV2,
        now: u64,
    ) -> Result<FundingRelayMoneyQuoteV2, ContractError> {
        policy.check_terms(terms, now)?;
        self.validate()?;
        physical.check(&usage)?;
        if self.id() != policy.metadata_fee_schedule || physical.id() != policy.physical_custody_profile {
            return Err(ContractError::Identity);
        }
        let retention_daa = policy.retention_quote(terms)?;
        let fee = sum([
            self.admission,
            mul(u64::from(usage.signatures), self.per_signature)?,
            mul(usage.frame_bytes, self.per_frame_byte)?,
            mul(mul(usage.retained_bytes, retention_daa)?, self.per_retained_byte_daa)?,
            mul(usage.database_bytes, self.per_database_byte)?,
            mul(usage.replay_reads, self.per_replay_read)?,
            mul(usage.replay_work, self.per_replay_work)?,
            self.retirement,
        ])?;
        if fee > terms.max_metadata_fee {
            return Err(ContractError::Budget);
        }
        Ok(FundingRelayMoneyQuoteV2 {
            terms: terms.id(),
            policy: policy.id(),
            fee_schedule: self.id(),
            physical_profile: physical.id(),
            usage,
            retention_daa,
            fee,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingRelayMoneyQuoteV2 {
    pub terms: Digest,
    pub policy: Digest,
    pub fee_schedule: Digest,
    pub physical_profile: Digest,
    pub usage: FundingRelayUsageV2,
    pub retention_daa: u64,
    pub fee: u64,
}
impl FundingRelayMoneyQuoteV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/funding-relay-quote/v2/r1", self)
    }
}
