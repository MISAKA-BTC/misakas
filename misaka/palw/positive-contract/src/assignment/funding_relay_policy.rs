//! Explicit finite RFC-0046 preimages; no network defaults and no native lease.
use super::{
    funding_relay::{FUNDING_RELAY_MAX_BODY_BYTES_V2, FundingRelayTermsV2},
    funding_relay_rows::FUNDING_RELAY_CHARGE_BYTES_V2,
};
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

/// Per-operation cap for scanning rooted relay occupancy. A point probe
/// counts even when absent. A range visit counts the key and any value it
/// inspects. This meters encoded bytes, separately from signature and
/// historical-ancestry work.
pub struct FundingRelayAuxScanBudgetV2 {
    remaining_reads: u64,
    remaining_work: u64,
}

impl FundingRelayAuxScanBudgetV2 {
    pub fn new(physical: &FundingRelayPhysicalProfileV2) -> Result<Self, ContractError> {
        physical.validate()?;
        Ok(Self { remaining_reads: physical.max_replay_reads, remaining_work: physical.max_replay_work })
    }

    pub fn inspect(&mut self, key_bytes: usize, value_bytes: usize) -> Result<(), ContractError> {
        let bytes = (key_bytes as u64).checked_add(value_bytes as u64).ok_or(ContractError::Budget)?;
        let remaining_reads = self.remaining_reads.checked_sub(1).ok_or(ContractError::Budget)?;
        let remaining_work = self.remaining_work.checked_sub(bytes).ok_or(ContractError::Budget)?;
        self.remaining_reads = remaining_reads;
        self.remaining_work = remaining_work;
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
    fn storage_price(
        &self,
        signatures: u32,
        frame_bytes: u64,
        retained_bytes: u64,
        database_bytes: u64,
        retention_daa: u64,
    ) -> Result<u64, ContractError> {
        sum([
            self.admission,
            mul(u64::from(signatures), self.per_signature)?,
            mul(frame_bytes, self.per_frame_byte)?,
            mul(mul(retained_bytes, retention_daa)?, self.per_retained_byte_daa)?,
            mul(database_bytes, self.per_database_byte)?,
            self.retirement,
        ])
    }

    /// Cost already implied by an op14 and its two pending rows. This covers
    /// the literal lifecycle payload, canonical aux images, both delta
    /// entries and their marginal batch framing. Other accepted history and
    /// replay work still need the actual storage and allocation owners.
    pub fn minimum_registration_fee(
        &self,
        policy: &FundingRelayPolicyV2,
        physical: &FundingRelayPhysicalProfileV2,
        terms: &FundingRelayTermsV2,
    ) -> Result<u64, ContractError> {
        self.validate()?;
        physical.validate()?;
        if self.id() != policy.metadata_fee_schedule || physical.id() != policy.physical_custody_profile {
            return Err(ContractError::Identity);
        }
        let retention_daa = policy.retention_quote(terms)?;
        let storage = FundingRelayStorageExtentV2::for_terms(terms)?;
        if storage.body_bytes > physical.max_frame_bytes
            || storage.retained_bytes > physical.max_retained_bytes
            || storage.database_bytes > physical.max_database_bytes
            || storage.batch_bytes > physical.max_batch_bytes
        {
            return Err(ContractError::Budget);
        }
        self.storage_price(2, storage.body_bytes, storage.retained_bytes, storage.database_bytes, retention_daa)
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
        FundingRelayStorageExtentV2::for_terms(terms)?.check_usage(&usage)?;
        let retention_daa = policy.retention_quote(terms)?;
        let fee = sum([
            self.storage_price(usage.signatures, usage.frame_bytes, usage.retained_bytes, usage.database_bytes, retention_daa)?,
            mul(usage.replay_reads, self.per_replay_read)?,
            mul(usage.replay_work, self.per_replay_work)?,
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
pub const FUNDING_RELAY_MATERIAL_KEY_BYTES_V2: usize = 66;

/// Minimum native extents for a new registration with both rows absent.
/// Charge the same extents even when the local DB already holds the payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundingRelayStorageExtentV2 {
    pub body_bytes: u64,
    pub retained_bytes: u64,
    pub database_bytes: u64,
    pub batch_bytes: u64,
    pub material_put_bytes: u64,
    pub charge_put_bytes: u64,
}

impl FundingRelayStorageExtentV2 {
    const LIFECYCLE_PREFIX_BYTES: u64 = 3;

    pub fn for_terms(terms: &FundingRelayTermsV2) -> Result<Self, ContractError> {
        const NATIVE_AUX_KEY_BYTES: u64 = 74;
        const AUTHORIZATION_BYTES_WITHOUT_TERMS: u64 = 8956;
        const UNBOUND_CUSTODY_BYTES: u64 = 5334;
        // Borsh KernelRouteRow insertion: kind, table, two Vec length
        // prefixes, and the absent-old/present-new Option tags.
        const DELTA_ENTRY_FRAMING_BYTES: u64 = 1 + 1 + 4 + 4 + 1 + 1;

        let terms_bytes = borsh::object_length(terms).map_err(|_| ContractError::NonCanonical)? as u64;
        let body_bytes = sum([5, terms_bytes, mul(2, crate::identity::SIGNATURE_BYTES as u64)?])?;
        let payload_bytes = sum([Self::LIFECYCLE_PREFIX_BYTES, body_bytes])?;
        let aux_bytes =
            sum([NATIVE_AUX_KEY_BYTES, AUTHORIZATION_BYTES_WITHOUT_TERMS, terms_bytes, NATIVE_AUX_KEY_BYTES, UNBOUND_CUSTODY_BYTES])?;
        let delta_bytes = sum([aux_bytes, 2 * DELTA_ENTRY_FRAMING_BYTES])?;
        Self::from_encoded(payload_bytes, aux_bytes, delta_bytes)
    }

    /// Build the same extent from the actual carrier and two canonical delta
    /// entries. Their common block delta wrapper exists without this relay.
    pub fn from_encoded(payload_bytes: u64, aux_bytes: u64, delta_bytes: u64) -> Result<Self, ContractError> {
        let body_bytes = payload_bytes.checked_sub(Self::LIFECYCLE_PREFIX_BYTES).ok_or(ContractError::Budget)?;
        // Every admissible op14 has a two-byte RocksDB value-length varint.
        if !(128..16_384).contains(&payload_bytes) || body_bytes > FUNDING_RELAY_MAX_BODY_BYTES_V2 as u64 || delta_bytes < aux_bytes {
            return Err(ContractError::Budget);
        }
        let charge_bytes = FUNDING_RELAY_CHARGE_BYTES_V2 as u64;
        let retained_bytes = sum([payload_bytes, aux_bytes, charge_bytes])?;
        let raw_extent = sum([payload_bytes, FUNDING_RELAY_MATERIAL_KEY_BYTES_V2 as u64])?;
        let charge_extent = sum([charge_bytes, FUNDING_RELAY_MATERIAL_KEY_BYTES_V2 as u64])?;
        let database_bytes = sum([raw_extent, charge_extent, delta_bytes])?;
        // Put type, one-byte key length, and the respective value lengths.
        let material_put_bytes = sum([raw_extent, 4])?;
        let charge_put_bytes = sum([charge_extent, 3])?;
        // The existing block delta Put's value-length varint can grow by up to
        // four bytes as its u32 value size increases.
        let batch_bytes = sum([database_bytes, 4, 3, 4])?;
        Ok(Self { body_bytes, retained_bytes, database_bytes, batch_bytes, material_put_bytes, charge_put_bytes })
    }

    pub fn check_usage(self, usage: &FundingRelayUsageV2) -> Result<(), ContractError> {
        if usage.frame_bytes < self.body_bytes
            || usage.retained_bytes < self.retained_bytes
            || usage.database_bytes < self.database_bytes
            || usage.batch_bytes < self.batch_bytes
        {
            return Err(ContractError::Budget);
        }
        Ok(())
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
