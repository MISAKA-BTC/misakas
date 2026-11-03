//! All-Panel aggregate, bounded source delivery prototype. This is a local
//! reservation ledger, NOT a native coin hold, payment event or slash authority.
//! Source chunks authenticate DATA IDENTITY only, not inference correctness.
use super::{ErrorV2, digest};
use crate::{
    Hash64,
    palw_step_leg::{PalwStepOpeningV1, step_opening_root_capped_v1},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;

pub const CHUNK_BYTES: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ServicePolicyV2 {
    pub version: u16,
    pub case: Hash64,
    pub source: Hash64,
    pub chunks: u64,
    pub max_rounds: u64,
    pub max_unique_bytes: u64,
    pub max_transmitted_bytes: u64,
    pub work_per_chunk: u64,
    pub max_work: u64,
    pub retention_until: u64,
    pub deadline: u64,
    pub price_per_chunk: u64,
    pub hold: u64,
}
impl ServicePolicyV2 {
    pub fn id(&self) -> Hash64 {
        digest(b"misaka/positive-service-policy/v2", self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkRequestV2 {
    Source { first: u64, count: u64 },
    BuildWitness,
    ReconstructTrace,
    ComputeProjection,
    ReplayCheckpoint,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestEffectV2 {
    Opened,
    Coalesced,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServiceMeterV2 {
    pub rounds: u64,
    pub unique_bytes: u64,
    pub transmitted_bytes: u64,
    pub work: u64,
    pub paid: u64,
    pub refunded: u64,
    pub remaining: u64,
}
pub fn chunk_hash_v2(case: &Hash64, index: u64, bytes: &[u8]) -> Hash64 {
    digest(b"misaka/positive-source-chunk/v2", &(case, index, bytes))
}
/// Deliberately no deserialize/Panel acknowledgement/default flag. Completion
/// is recorded only after this Node authenticates the actual supplied bytes.
/// Missing service is not independently sufficient to attribute Miner fraud.
pub struct ServiceLedgerV2 {
    policy: ServicePolicyV2,
    chunks: BTreeMap<u64, bool>,
    meter: ServiceMeterV2,
    closed: bool,
}
impl ServiceLedgerV2 {
    /// Caller must match `policy.id()` to the SIGNED MinerResult.service_policy.
    pub fn new(policy: ServicePolicyV2, signed_policy: Hash64, now: u64) -> Result<Self, ErrorV2> {
        if policy.version != 2 || policy.id() != signed_policy {
            return Err(ErrorV2::Binding);
        }
        if policy.chunks == 0
            || policy.chunks > policy.max_unique_bytes / CHUNK_BYTES as u64
            || policy.retention_until < policy.deadline
            || now >= policy.deadline
            || policy.work_per_chunk == 0
        {
            return Err(ErrorV2::Budget);
        }
        let remaining = policy.hold;
        Ok(Self { policy, chunks: BTreeMap::new(), meter: ServiceMeterV2 { remaining, ..Default::default() }, closed: false })
    }
    pub fn meter(&self) -> ServiceMeterV2 {
        self.meter
    }
    pub fn data_complete(&self) -> bool {
        !self.chunks.is_empty() && self.chunks.values().all(|x| *x)
    }
    pub fn conserved(&self) -> bool {
        self.meter.paid.checked_add(self.meter.refunded).and_then(|x| x.checked_add(self.meter.remaining)) == Some(self.policy.hold)
    }
    pub fn request(&mut self, request: WorkRequestV2, now: u64) -> Result<RequestEffectV2, ErrorV2> {
        if self.closed || now >= self.policy.deadline {
            return Err(ErrorV2::Closed);
        }
        let WorkRequestV2::Source { first, count } = request else {
            // Optional replay needs a distinct, pre-signed consent profile. This
            // profile never creates it by accepting an arbitrary request tag.
            return Err(ErrorV2::UnsupportedProfile);
        };
        let end = first.checked_add(count).filter(|e| count > 0 && *e <= self.policy.chunks).ok_or(ErrorV2::Order)?;
        // Bound the loop and the map even for a malicious u64-sized request.
        let bytes = count.checked_mul(CHUNK_BYTES as u64).ok_or(ErrorV2::Budget)?;
        if bytes > self.policy.max_unique_bytes {
            return Err(ErrorV2::Budget);
        }
        let old = self.chunks.range(first..end).count() as u64;
        let fresh = count - old;
        if fresh == 0 {
            return Ok(RequestEffectV2::Coalesced);
        }
        let unique = self.meter.unique_bytes.checked_add(fresh * CHUNK_BYTES as u64).ok_or(ErrorV2::Budget)?;
        let rounds = self.meter.rounds.checked_add(1).ok_or(ErrorV2::Budget)?;
        let reserved_fee = (self.chunks.len() as u64)
            .checked_add(fresh)
            .and_then(|x| x.checked_mul(self.policy.price_per_chunk))
            .ok_or(ErrorV2::Budget)?;
        if unique > self.policy.max_unique_bytes || rounds > self.policy.max_rounds || reserved_fee > self.policy.hold {
            return Err(ErrorV2::Budget);
        }
        // All checks precede mutation. Panel identity, relay and nonce do not
        // exist in this canonical union, so none can reset its quotas.
        for index in first..end {
            self.chunks.entry(index).or_insert(false);
        }
        self.meter.unique_bytes = unique;
        self.meter.rounds = rounds;
        Ok(RequestEffectV2::Opened)
    }
    /// Direct Node receipt: Panel withholding an ACK has no effect here.
    /// Every attempt, including an invalid or duplicate response, consumes the
    /// aggregate transmission/work allowance; a completed unit is paid once.
    pub fn respond(&mut self, index: u64, bytes: &[u8], opening: &PalwStepOpeningV1, now: u64) -> Result<(), ErrorV2> {
        if self.closed || now >= self.policy.deadline {
            return Err(ErrorV2::Closed);
        }
        let complete = *self.chunks.get(&index).ok_or(ErrorV2::Order)?;
        if bytes.len() != CHUNK_BYTES {
            return Err(ErrorV2::Wire);
        }
        if opening.siblings.len() > crate::palw_step_leg::step_leg_max_opening_siblings_v1(self.policy.chunks) {
            return Err(ErrorV2::Budget);
        }
        // Include the complete opening and its length prefix in communication,
        // not just the raw data payload.
        let wire_bytes = (bytes.len() as u64)
            .checked_add(8 + 64 + 4)
            .and_then(|x| x.checked_add(64 * opening.siblings.len() as u64))
            .ok_or(ErrorV2::Budget)?;
        let transmitted = self.meter.transmitted_bytes.checked_add(wire_bytes).ok_or(ErrorV2::Budget)?;
        let work = self.meter.work.checked_add(self.policy.work_per_chunk).ok_or(ErrorV2::Budget)?;
        if transmitted > self.policy.max_transmitted_bytes || work > self.policy.max_work {
            return Err(ErrorV2::Budget);
        }
        self.meter.transmitted_bytes = transmitted;
        self.meter.work = work;
        if opening.leaf_index != index
            || opening.leaf_hash != chunk_hash_v2(&self.policy.case, index, bytes)
            || step_opening_root_capped_v1(self.policy.chunks, opening, self.policy.chunks).map_err(|_| ErrorV2::Wire)?
                != self.policy.source
        {
            return Err(ErrorV2::Binding);
        }
        if !complete {
            let remaining = self.meter.remaining.checked_sub(self.policy.price_per_chunk).ok_or(ErrorV2::Budget)?;
            self.meter.paid += self.policy.price_per_chunk;
            self.meter.remaining = remaining;
            self.chunks.insert(index, true);
        }
        debug_assert!(self.conserved());
        Ok(())
    }
    /// An idempotent refund of an in-memory reservation, not a native transfer.
    pub fn close(&mut self) {
        if !self.closed {
            self.meter.refunded += self.meter.remaining;
            self.meter.remaining = 0;
            self.closed = true;
        }
        debug_assert!(self.conserved());
    }
}
