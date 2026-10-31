//! Bounded node-local transfer of a Positive recovery journal. This is NOT a
//! P2P codec or a new state-root collection. The receiving adapter resolves the
//! scope, complete journal head and length from its trusted selected history.
//! A byte/root match only establishes identity: the existing Recovery port must
//! replay signatures, source data, failures and every computation before an
//! import token exists. No persisted Pass flag or row-stream marker is accepted.
use super::coordinator::recovery::{
    JournaledCoordinatorV2, RecoveryAdapterV2, RecoveryContextV2, RecoveryHeadV2, RecoveryLimitsV2, RetentionV2,
};
use super::{ClaimStatusV2, ErrorV2};
use crate::Hash64;
use misaka_palw_positive_contract::ports::Recovery;
use std::io::{self, Write};

/// Identity of the LOCAL private coordinator. These fields are not aliases for
/// A0 wire IDs; a native ingress must explicitly resolve its own result binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryImportScopeV2 {
    pub network: Hash64,
    pub branch: Hash64,
    pub ruleset: Hash64,
    pub case: Hash64,
    pub result: Hash64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryTransferLimitsV2 {
    /// Canonical packet payload length, except the last packet's exact tail.
    pub chunk_bytes: u64,
    pub max_chunks: u64,
    pub max_packets: u64,
    pub max_wire_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecoveryTransferMeterV2 {
    pub packets: u64,
    pub wire_bytes: u64,
    pub accepted_bytes: u64,
    pub accepted_chunks: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryChunkEffectV2 {
    Appended,
    /// Only an exact retransmission of the immediately preceding packet. It
    /// still consumes the cumulative packet/communication allowance.
    Duplicate,
}
#[derive(Debug)]
pub enum RecoveryImportErrorV2 {
    Invalid(ErrorV2),
    Storage(io::Error),
    Poisoned,
}
impl From<ErrorV2> for RecoveryImportErrorV2 {
    fn from(error: ErrorV2) -> Self {
        Self::Invalid(error)
    }
}

// Five scope digests, records+head, packet ordinal and payload length. This is
// conservative private envelope accounting, not an assigned network layout.
const ENVELOPE_BYTES: u64 = 5 * 64 + 8 + 64 + 8 + 8;
fn journal_hasher(encoded_bytes: u64) -> Result<blake2b_simd::State, ErrorV2> {
    let length = u32::try_from(encoded_bytes).map_err(|_| ErrorV2::Budget)?;
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka/private-pc-recovery/v1").to_state();
    // Same Borsh slice envelope as coordinator::recovery's journal identity,
    // streamed without allocating a second encoded journal.
    state.update(&length.to_le_bytes());
    Ok(state)
}
fn packet_id(index: u64, bytes: &[u8]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka/private-pc-chunk/v1").to_state();
    state.update(&index.to_le_bytes());
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(bytes);
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64"))
}

/// Own the same receiver across relay replacement/timeouts. Creating another
/// receiver is a new node-local transfer reservation, never a fresh Miner duty.
/// The sink must be a capped private spill; `SpillArea` supplies that quota and
/// crash cleanup in the native adapter. Errors poison this entire receiver.
pub struct RecoveryChunkImportV2<W: Write> {
    scope: RecoveryImportScopeV2,
    head: RecoveryHeadV2,
    encoded_bytes: u64,
    chunks: u64,
    transfer: RecoveryTransferLimitsV2,
    limits: RecoveryLimitsV2,
    meter: RecoveryTransferMeterV2,
    last_packet: Option<Hash64>,
    stream: blake2b_simd::State,
    sink: W,
    poisoned: bool,
}
impl<W: Write> RecoveryChunkImportV2<W> {
    pub fn new(
        scope: RecoveryImportScopeV2,
        head: RecoveryHeadV2,
        encoded_bytes: u64,
        transfer: RecoveryTransferLimitsV2,
        limits: RecoveryLimitsV2,
        sink: W,
    ) -> Result<Self, ErrorV2> {
        if encoded_bytes == 0
            || encoded_bytes > limits.max_encoded_bytes
            || head.records > limits.max_records
            || transfer.chunk_bytes == 0
            || transfer.max_chunks == 0
            || transfer.max_packets == 0
        {
            return Err(ErrorV2::Budget);
        }
        let chunks = encoded_bytes.div_ceil(transfer.chunk_bytes);
        let minimum_wire = chunks.checked_mul(ENVELOPE_BYTES).and_then(|n| n.checked_add(encoded_bytes)).ok_or(ErrorV2::Budget)?;
        if chunks > transfer.max_chunks || chunks > transfer.max_packets || minimum_wire > transfer.max_wire_bytes {
            return Err(ErrorV2::Budget);
        }
        Ok(Self {
            scope,
            head,
            encoded_bytes,
            chunks,
            transfer,
            limits,
            meter: RecoveryTransferMeterV2::default(),
            last_packet: None,
            stream: journal_hasher(encoded_bytes)?,
            sink,
            poisoned: false,
        })
    }
    pub fn meter(&self) -> RecoveryTransferMeterV2 {
        self.meter
    }
    pub fn next_index(&self) -> u64 {
        self.meter.accepted_chunks
    }
    pub fn complete(&self) -> bool {
        !self.poisoned && self.meter.accepted_chunks == self.chunks
    }

    pub fn accept(
        &mut self,
        scope: RecoveryImportScopeV2,
        head: RecoveryHeadV2,
        index: u64,
        bytes: &[u8],
    ) -> Result<RecoveryChunkEffectV2, RecoveryImportErrorV2> {
        if self.poisoned {
            return Err(RecoveryImportErrorV2::Poisoned);
        }
        let result = self.accept_inner(scope, head, index, bytes);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn accept_inner(
        &mut self,
        scope: RecoveryImportScopeV2,
        head: RecoveryHeadV2,
        index: u64,
        bytes: &[u8],
    ) -> Result<RecoveryChunkEffectV2, RecoveryImportErrorV2> {
        // Bound before hashing or writing a peer-selected payload.
        if bytes.len() as u64 > self.transfer.chunk_bytes {
            return Err(ErrorV2::Budget.into());
        }
        let packets = self.meter.packets.checked_add(1).ok_or(ErrorV2::Budget)?;
        let wire_bytes = self
            .meter
            .wire_bytes
            .checked_add(ENVELOPE_BYTES)
            .and_then(|n| n.checked_add(bytes.len() as u64))
            .ok_or(ErrorV2::Budget)?;
        if packets > self.transfer.max_packets || wire_bytes > self.transfer.max_wire_bytes {
            return Err(ErrorV2::Budget.into());
        }
        self.meter.packets = packets;
        self.meter.wire_bytes = wire_bytes;
        if scope != self.scope || head != self.head {
            return Err(ErrorV2::Binding.into());
        }
        if index >= self.chunks {
            return Err(ErrorV2::Order.into());
        }
        let expected_bytes = self.transfer.chunk_bytes.min(self.encoded_bytes - index * self.transfer.chunk_bytes);
        if bytes.len() as u64 != expected_bytes {
            return Err(ErrorV2::Wire.into());
        }
        let packet = packet_id(index, bytes);
        if self.meter.accepted_chunks > 0 && index == self.meter.accepted_chunks - 1 {
            return if self.last_packet == Some(packet) { Ok(RecoveryChunkEffectV2::Duplicate) } else { Err(ErrorV2::Binding.into()) };
        }
        if index != self.meter.accepted_chunks {
            return Err(ErrorV2::Order.into());
        }
        self.sink.write_all(bytes).map_err(RecoveryImportErrorV2::Storage)?;
        self.stream.update(bytes);
        self.meter.accepted_bytes += bytes.len() as u64;
        self.meter.accepted_chunks += 1;
        self.last_packet = Some(packet);
        Ok(RecoveryChunkEffectV2::Appended)
    }

    /// Read back the private spill under the supplied encoded-byte cap, then
    /// run the actual Recovery port. `readback` belongs to the local storage
    /// adapter and must cap allocation; a bounded `SpillFile::reader` is suitable.
    /// The spill is consumed/dropped on every failure and never published here.
    pub fn finish<C: RecoveryContextV2>(
        mut self,
        driver: &RecoveryAdapterV2<C>,
        anchor: &C::TrustedAnchor,
        readback: impl FnOnce(W, u64) -> io::Result<Vec<u8>>,
    ) -> Result<CheckedRecoveryImportV2, RecoveryImportErrorV2> {
        if self.poisoned {
            return Err(RecoveryImportErrorV2::Poisoned);
        }
        if !self.complete() || self.meter.accepted_bytes != self.encoded_bytes {
            return Err(ErrorV2::Incomplete.into());
        }
        let root = Hash64::from_bytes(self.stream.finalize().as_bytes().try_into().expect("64"));
        if root != self.head.digest {
            return Err(ErrorV2::Binding.into());
        }
        self.sink.flush().map_err(RecoveryImportErrorV2::Storage)?;
        let bytes = readback(self.sink, self.limits.max_encoded_bytes).map_err(RecoveryImportErrorV2::Storage)?;
        if bytes.len() as u64 != self.encoded_bytes {
            return Err(ErrorV2::Wire.into());
        }
        let restored = driver.replay(anchor, bytes)?;
        let actual_head = restored.head().map_err(|_| ErrorV2::Closed)?;
        let result = restored.state().map_err(|_| ErrorV2::Closed)?.result();
        if actual_head != self.head
            || result.network != self.scope.network
            || result.branch != self.scope.branch
            || result.ruleset != self.scope.ruleset
            || result.claim != self.scope.case
            || result.id() != self.scope.result
        {
            return Err(ErrorV2::Binding.into());
        }
        Ok(CheckedRecoveryImportV2 { scope: self.scope, head: self.head, restored })
    }
}

/// Can only be constructed by a complete identity-checked transfer followed by
/// the actual full replay. It carries no Gate C/credit or coin authority. Native
/// publication must atomically select this head with related checked DB rows.
pub struct CheckedRecoveryImportV2 {
    scope: RecoveryImportScopeV2,
    head: RecoveryHeadV2,
    restored: JournaledCoordinatorV2,
}
impl CheckedRecoveryImportV2 {
    pub fn scope(&self) -> RecoveryImportScopeV2 {
        self.scope
    }
    pub fn head(&self) -> RecoveryHeadV2 {
        self.head
    }
    pub fn encoded(&self) -> &[u8] {
        self.restored.encoded().expect("checked immutable recovery state")
    }
    pub fn state(&self) -> &JournaledCoordinatorV2 {
        &self.restored
    }
    /// The authenticated receiving clock used by replay. Publication cannot
    /// substitute a caller-supplied time for this checked state view.
    pub fn observed_daa(&self) -> u64 {
        self.restored.observed_daa().expect("checked immutable recovery clock")
    }
    pub fn into_state(self) -> JournaledCoordinatorV2 {
        self.restored
    }
}

/// Retention decisions are bound to the exact case/scope/head being retired.
/// Rooted horizons are still required. The old archive-availability flag alone
/// cannot permit pruning: a fully rechecked replacement archive is mandatory.
pub struct RecoveryRetentionV2 {
    scope: RecoveryImportScopeV2,
    head: RecoveryHeadV2,
    horizons: RetentionV2,
}
impl RecoveryRetentionV2 {
    pub fn new(scope: RecoveryImportScopeV2, head: RecoveryHeadV2, horizons: RetentionV2) -> Self {
        Self { scope, head, horizons }
    }
    pub fn may_prune(&self, now: u64, replacement: &CheckedRecoveryImportV2) -> bool {
        self.scope == replacement.scope
            && self.head == replacement.head
            && replacement.state().state().is_ok_and(|s| s.completion().claim != ClaimStatusV2::Checking)
            && self.horizons.may_prune_for_self_verifying_import(now)
    }
}

#[cfg(test)]
#[path = "recovery_sync_tests.rs"]
mod tests;
