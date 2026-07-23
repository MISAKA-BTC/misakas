//! §12.3 archive anchors, v2 — the sparse, compressed, code-free checkpoint.
//!
//! [`EvmStateCheckpointV1`](super::EvmStateCheckpointV1) has a field named
//! `compressed_snapshot` that is a plain `borsh::to_vec` of the full
//! [`EvmStateSnapshot`], bytecode inlined, written every
//! `EVM_CHECKPOINT_INTERVAL` (2048) EVM blocks. Three separate multipliers on a
//! store that is meant to be a sparse anchor:
//!
//! 1. **Not compressed.** A borsh-encoded state is mostly 32-byte words with
//!    long zero runs; a name is not a codec.
//! 2. **Bytecode inlined.** The same contract's code is repeated in every
//!    checkpoint, while prefix 222 already stores it once, content-addressed.
//! 3. **Frequent.** 2048 EVM blocks is minutes on a 10 BPS chain, so a
//!    "periodic anchor" reproduced the whole state several hundred times a day.
//!
//! V2 fixes all three. Accounts carry `code_hash` only and the payload is
//! compressed under a declared codec, so a checkpoint is a sparse anchor that
//! bounds reconstruction distance rather than a second full-state history.
//!
//! The codec is a stored enum rather than an assumption, so a future algorithm
//! is a new variant and not a format break — which is exactly the escape hatch
//! V1's `compressed_snapshot` comment claimed and never used.

use super::{EvmAddress, EvmStateSnapshot, EvmU256};
use crate::evm::state_diff::{StateDiffError, checkpoint_checksum};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{EvmH256, Hash64};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Format version stamped into every v2 checkpoint. Bumped when the PAYLOAD
/// layout changes in a way a v2 reader cannot handle; the codec is versioned
/// separately, so adding a compressor does not touch this.
pub const EVM_CHECKPOINT_FORMAT_V2: u16 = 2;

/// Compression applied to a checkpoint payload.
///
/// Stored rather than assumed: a reader must be able to decode a checkpoint
/// written by a node configured differently, and an operator must be able to
/// turn compression off to isolate a suspected codec bug without a resync.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum CheckpointCodec {
    /// Raw borsh. The v1 behaviour, kept for diagnostics and tiny states where
    /// framing overhead would exceed the saving.
    None = 0,
    /// DEFLATE (zlib). Chosen over zstd because it is already a vetted
    /// dependency of this workspace and pure-Rust on wasm; the codec enum exists
    /// so a stronger algorithm can be added without a format break.
    #[default]
    Deflate = 1,
}

impl CheckpointCodec {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "deflate" => Some(Self::Deflate),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Deflate => "deflate",
        }
    }
}

/// One account inside a v2 checkpoint payload.
///
/// The difference from [`EvmAccountSnapshot`](super::EvmAccountSnapshot) is the
/// whole point: `code_hash` instead of `code`. Bytecode lives once in the
/// content-addressed code store (prefix 222) and is rehydrated on decode, so a
/// contract deployed once stops being copied into every subsequent checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmCheckpointAccount {
    pub address: EvmAddress,
    pub nonce: u64,
    pub balance: EvmU256,
    pub code_hash: EvmH256,
    /// Non-zero storage slots, sorted by slot key (same invariant as v1).
    pub storage: Vec<(EvmU256, EvmU256)>,
}

/// The decoded payload of a v2 checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmCheckpointPayload {
    pub accounts: Vec<EvmCheckpointAccount>,
}

/// A sparse §12.3 reconstruction anchor.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmStateCheckpointV2 {
    pub block: Hash64,
    pub evm_number: u64,
    pub state_root: EvmH256,
    pub codec: CheckpointCodec,
    /// Length of the payload BEFORE compression. Carried so a decoder can size
    /// its buffer, and so `compressed < uncompressed` is checkable from the row
    /// itself — the property v1's field name asserted without evidence.
    pub uncompressed_len: u64,
    pub payload: Vec<u8>,
    pub checksum: [u8; 32],
    pub format_version: u16,
}

impl kaspa_utils::mem_size::MemSizeEstimator for EvmStateCheckpointV2 {
    // Implemented (not the panicking default) so the store is safe under any
    // cache policy — the value carries a heap-allocated payload.
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>() + self.payload.capacity()
    }
}

impl kaspa_utils::mem_size::MemSizeEstimator for EvmCheckpointMeta {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>() + self.retained.capacity() * size_of::<Hash64>()
    }
}

/// Rehydrating a code-free checkpoint needs the content-addressed code store.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointCodeError {
    #[error("checkpoint references code_hash {0:?} which is absent from the code store")]
    MissingCode(EvmH256),
    #[error("code store read failed: {0}")]
    Store(String),
}

impl EvmStateCheckpointV2 {
    /// Build an anchor from a full snapshot. Bytecode is DROPPED — the caller is
    /// responsible for the code being in prefix 222, which it already is: the
    /// same commit that produces a state diff writes that block's new code.
    pub fn build(block: Hash64, evm_number: u64, state_root: EvmH256, snapshot: &EvmStateSnapshot, codec: CheckpointCodec) -> Self {
        let payload = EvmCheckpointPayload {
            accounts: snapshot
                .accounts
                .iter()
                .map(|a| EvmCheckpointAccount {
                    address: a.address,
                    nonce: a.nonce,
                    balance: a.balance,
                    code_hash: a.code_hash,
                    storage: a.storage.clone(),
                })
                .collect(),
        };
        let raw = borsh::to_vec(&payload).expect("EvmCheckpointPayload is infallibly borsh-serializable");
        let uncompressed_len = raw.len() as u64;
        let encoded = compress(&raw, codec);
        // The checksum covers the STORED bytes, so corruption is caught before
        // the decompressor is handed an attacker- or bitrot-shaped input.
        let checksum = checkpoint_checksum(&encoded);
        Self {
            block,
            evm_number,
            state_root,
            codec,
            uncompressed_len,
            payload: encoded,
            checksum,
            format_version: EVM_CHECKPOINT_FORMAT_V2,
        }
    }

    /// Decode back to a full snapshot, resolving each account's bytecode through
    /// `resolve_code`.
    ///
    /// `resolve_code` returns `Ok(None)` for "not in the store". That is an ERROR
    /// here, not an empty account: silently substituting empty code would produce
    /// a snapshot whose state root does not match the committed one, and the
    /// caller would discover it as an unexplained root mismatch far from here.
    /// The empty-code hash is handled without a store lookup.
    pub fn decode_snapshot<E: std::fmt::Display>(
        &self,
        mut resolve_code: impl FnMut(EvmH256) -> Result<Option<Vec<u8>>, E>,
    ) -> Result<EvmStateSnapshot, StateDiffError> {
        let payload = self.decode_payload()?;
        let mut accounts = Vec::with_capacity(payload.accounts.len());
        for a in payload.accounts {
            let code = if a.code_hash == super::state_diff::EVM_EMPTY_CODE_HASH {
                Vec::new()
            } else {
                match resolve_code(a.code_hash) {
                    Ok(Some(code)) => code,
                    Ok(None) => {
                        return Err(StateDiffError::Inconsistent(format!(
                            "checkpoint {} references code_hash {:?} absent from the code store",
                            self.block, a.code_hash
                        )));
                    }
                    Err(e) => {
                        return Err(StateDiffError::Inconsistent(format!("checkpoint {} code lookup failed: {e}", self.block)));
                    }
                }
            };
            accounts.push(super::EvmAccountSnapshot {
                address: a.address,
                nonce: a.nonce,
                balance: a.balance,
                code_hash: a.code_hash,
                code,
                storage: a.storage,
            });
        }
        Ok(EvmStateSnapshot { accounts })
    }

    /// The code hashes this checkpoint depends on — the mark roots a code-store
    /// GC must treat as live for as long as this anchor is retained.
    pub fn referenced_code_hashes(&self) -> Result<Vec<EvmH256>, StateDiffError> {
        Ok(self
            .decode_payload()?
            .accounts
            .into_iter()
            .map(|a| a.code_hash)
            .filter(|h| *h != super::state_diff::EVM_EMPTY_CODE_HASH)
            .collect())
    }

    fn decode_payload(&self) -> Result<EvmCheckpointPayload, StateDiffError> {
        if self.format_version != EVM_CHECKPOINT_FORMAT_V2 {
            return Err(StateDiffError::Inconsistent(format!(
                "checkpoint {} has unsupported format version {}",
                self.block, self.format_version
            )));
        }
        if checkpoint_checksum(&self.payload) != self.checksum {
            return Err(StateDiffError::Inconsistent(format!("checkpoint {} checksum mismatch", self.block)));
        }
        let raw = decompress(&self.payload, self.codec, self.uncompressed_len)
            .map_err(|e| StateDiffError::Inconsistent(format!("checkpoint {} payload decode: {e}", self.block)))?;
        borsh::from_slice(&raw).map_err(|e| StateDiffError::Inconsistent(format!("checkpoint {} payload decode: {e}", self.block)))
    }

    /// Stored bytes. What `--db-stats` and the checkpoint metrics report.
    pub fn stored_len(&self) -> u64 {
        self.payload.len() as u64
    }
}

fn compress(raw: &[u8], codec: CheckpointCodec) -> Vec<u8> {
    match codec {
        CheckpointCodec::None => raw.to_vec(),
        CheckpointCodec::Deflate => {
            let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            // Writing to a Vec cannot fail, and neither can finishing it.
            encoder.write_all(raw).expect("in-memory zlib write");
            encoder.finish().expect("in-memory zlib finish")
        }
    }
}

/// `uncompressed_len` bounds the output so a corrupt or hostile row cannot make
/// a decoder allocate without limit — a decompression bomb is a denial of
/// service even when the checksum is intact, because the checksum only proves
/// the bytes are the ones that were written.
fn decompress(stored: &[u8], codec: CheckpointCodec, uncompressed_len: u64) -> Result<Vec<u8>, String> {
    match codec {
        CheckpointCodec::None => {
            if stored.len() as u64 != uncompressed_len {
                return Err(format!("uncompressed length {} != declared {uncompressed_len}", stored.len()));
            }
            Ok(stored.to_vec())
        }
        CheckpointCodec::Deflate => {
            let mut out = Vec::with_capacity(uncompressed_len.min(1 << 26) as usize);
            let mut decoder = flate2::read::ZlibDecoder::new(stored).take(uncompressed_len);
            decoder.read_to_end(&mut out).map_err(|e| e.to_string())?;
            if out.len() as u64 != uncompressed_len {
                return Err(format!("inflated length {} != declared {uncompressed_len}", out.len()));
            }
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evm::{EvmAccountSnapshot, state_diff::EVM_EMPTY_CODE_HASH};

    fn h256(b: u8) -> EvmH256 {
        EvmH256::from_bytes([b; 32])
    }

    fn contract(addr: u8, code: Vec<u8>, code_hash: EvmH256, slots: usize) -> EvmAccountSnapshot {
        EvmAccountSnapshot {
            address: EvmAddress::from_bytes([addr; 20]),
            nonce: 1,
            balance: EvmU256::from_u128(1000),
            code_hash,
            code,
            storage: (0..slots as u64).map(|i| (EvmU256::from_u128(i as u128), EvmU256::from_u128(i as u128 + 1))).collect(),
        }
    }

    fn eoa(addr: u8) -> EvmAccountSnapshot {
        EvmAccountSnapshot {
            address: EvmAddress::from_bytes([addr; 20]),
            nonce: 7,
            balance: EvmU256::from_u128(42),
            code_hash: EVM_EMPTY_CODE_HASH,
            code: Vec::new(),
            storage: Vec::new(),
        }
    }

    #[test]
    fn roundtrip_restores_the_snapshot_byte_for_byte() {
        let code = vec![0x60, 0x80, 0x60, 0x40];
        let snap = EvmStateSnapshot { accounts: vec![eoa(1), contract(2, code.clone(), h256(0xAA), 4)] };
        let cp = EvmStateCheckpointV2::build(Hash64::from_bytes([9; 64]), 100, h256(0x55), &snap, CheckpointCodec::Deflate);

        let decoded =
            cp.decode_snapshot(|hash| -> Result<Option<Vec<u8>>, String> { Ok((hash == h256(0xAA)).then(|| code.clone())) }).unwrap();
        assert_eq!(decoded, snap, "a checkpoint must reconstruct the exact snapshot it anchored");
    }

    #[test]
    fn bytecode_is_not_stored_in_the_payload() {
        // The saving that matters: the same contract anchored twice must not
        // carry its code twice.
        let code = vec![0xAB; 24_000];
        let snap = EvmStateSnapshot { accounts: vec![contract(2, code.clone(), h256(0xAA), 1)] };
        let cp = EvmStateCheckpointV2::build(Hash64::from_bytes([9; 64]), 1, h256(0x55), &snap, CheckpointCodec::None);
        assert!(
            (cp.stored_len() as usize) < code.len() / 4,
            "payload {} should not contain the {}-byte code",
            cp.stored_len(),
            code.len()
        );
        // And the code hash is still recoverable as a GC mark root.
        assert_eq!(cp.referenced_code_hashes().unwrap(), vec![h256(0xAA)]);
    }

    #[test]
    fn deflate_actually_shrinks_a_realistic_state() {
        // EVM state is mostly 32-byte words with long zero runs; if the codec did
        // not shrink this, "compressed" would again be a claim rather than a fact.
        let snap = EvmStateSnapshot { accounts: (0..64).map(|i| contract(i as u8, Vec::new(), h256(0xAA), 64)).collect() };
        let raw = EvmStateCheckpointV2::build(Hash64::from_bytes([9; 64]), 1, h256(0x55), &snap, CheckpointCodec::None);
        let zipped = EvmStateCheckpointV2::build(Hash64::from_bytes([9; 64]), 1, h256(0x55), &snap, CheckpointCodec::Deflate);

        assert_eq!(raw.uncompressed_len, zipped.uncompressed_len, "both declare the same pre-compression size");
        assert!(zipped.stored_len() < raw.stored_len() / 2, "deflate {} vs raw {}", zipped.stored_len(), raw.stored_len());
        assert!(zipped.stored_len() < zipped.uncompressed_len, "stored payload must be smaller than the declared raw length");
    }

    #[test]
    fn empty_code_hash_needs_no_store_lookup() {
        let snap = EvmStateSnapshot { accounts: vec![eoa(1)] };
        let cp = EvmStateCheckpointV2::build(Hash64::from_bytes([9; 64]), 1, h256(0x55), &snap, CheckpointCodec::Deflate);
        // The resolver panics if called: an EOA must never reach the code store.
        let decoded =
            cp.decode_snapshot(|_| -> Result<Option<Vec<u8>>, String> { panic!("EOA must not hit the code store") }).unwrap();
        assert_eq!(decoded, snap);
    }

    #[test]
    fn missing_code_fails_closed_instead_of_substituting_empty() {
        let snap = EvmStateSnapshot { accounts: vec![contract(2, vec![1, 2, 3], h256(0xAA), 1)] };
        let cp = EvmStateCheckpointV2::build(Hash64::from_bytes([9; 64]), 1, h256(0x55), &snap, CheckpointCodec::Deflate);
        let err = cp.decode_snapshot(|_| -> Result<Option<Vec<u8>>, String> { Ok(None) }).unwrap_err();
        assert!(format!("{err}").contains("absent from the code store"), "{err}");
    }

    #[test]
    fn tampered_payload_and_wrong_version_fail_closed() {
        let snap = EvmStateSnapshot { accounts: vec![eoa(1)] };
        let cp = EvmStateCheckpointV2::build(Hash64::from_bytes([9; 64]), 1, h256(0x55), &snap, CheckpointCodec::Deflate);

        let mut tampered = cp.clone();
        tampered.payload[0] ^= 0xff;
        assert!(tampered.decode_snapshot(|_| -> Result<Option<Vec<u8>>, String> { Ok(None) }).is_err());

        let mut wrong_version = cp.clone();
        wrong_version.format_version = 99;
        assert!(wrong_version.decode_snapshot(|_| -> Result<Option<Vec<u8>>, String> { Ok(None) }).is_err());
    }

    #[test]
    fn a_lying_uncompressed_len_cannot_make_the_decoder_allocate_without_bound() {
        // The checksum proves the bytes are the ones written; it does not prove
        // they are safe to inflate. The declared length is the bound.
        let snap = EvmStateSnapshot { accounts: (0..16).map(|i| contract(i as u8, Vec::new(), h256(0xAA), 16)).collect() };
        let mut cp = EvmStateCheckpointV2::build(Hash64::from_bytes([9; 64]), 1, h256(0x55), &snap, CheckpointCodec::Deflate);
        cp.uncompressed_len = 8; // truncates the inflate well below the real payload
        cp.checksum = checkpoint_checksum(&cp.payload);
        let err = cp.decode_snapshot(|_| -> Result<Option<Vec<u8>>, String> { Ok(None) }).unwrap_err();
        assert!(format!("{err}").contains("decode"), "{err}");
    }
}

// ---------------------------------------------------------------------------
// Cadence: when to write an anchor at all.
// ---------------------------------------------------------------------------

/// Bookkeeping behind the checkpoint cadence (prefix 224).
///
/// V1 needed no state because its rule was `evm_number % 2048 == 0` — which is
/// also why it could not express "every few hours" or "keep only the last N".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmCheckpointMeta {
    /// EVM number of the most recent anchor. `None` (0 with `has_any = false`)
    /// before the first one.
    pub last_evm_number: u64,
    /// Wall-clock of the most recent anchor, from the block header's timestamp
    /// rather than the node's clock, so replaying the same chain on two nodes
    /// makes the same decisions.
    pub last_timestamp_ms: u64,
    pub has_any: bool,
    /// Retained anchors, oldest first. Bounded by the retention policy; evicting
    /// from the front is what keeps the store sparse rather than merely slower
    /// growing.
    pub retained: Vec<Hash64>,
}

/// When to write an anchor, and how many to keep.
///
/// Replaces `EVM_CHECKPOINT_INTERVAL`. The block gap is a CAP rather than the
/// schedule: it bounds worst-case reconstruction distance on a chain that is
/// producing blocks faster than the time trigger fires, while the time trigger
/// does the actual pacing. Expressing the schedule in time is what makes the
/// setting portable across BPS changes — 2048 blocks means 34 minutes at 1 BPS
/// and 3.4 minutes at 10 BPS, and nobody re-derives that when the BPS moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmCheckpointPolicy {
    pub codec: CheckpointCodec,
    /// Minimum wall-clock between anchors.
    pub min_interval_ms: u64,
    /// Hard cap on the EVM-block distance between anchors, so reconstruction
    /// never has to replay an unbounded diff chain.
    pub max_block_gap: u64,
    /// How many anchors to keep. `None` = unbounded (archive).
    pub max_retained: Option<usize>,
}

impl Default for EvmCheckpointPolicy {
    fn default() -> Self {
        Self {
            codec: CheckpointCodec::Deflate,
            // Six hours: long enough that anchors are sparse, short enough that a
            // reconstruction replays hours of diffs rather than days.
            min_interval_ms: 6 * 60 * 60 * 1000,
            // At 10 BPS this is reached only if the chain outruns the time
            // trigger; it exists so the distance is bounded either way.
            max_block_gap: 200_000,
            max_retained: Some(4),
        }
    }
}

impl EvmCheckpointPolicy {
    /// The archive posture: same cadence, but nothing is evicted.
    pub fn archive() -> Self {
        Self { max_retained: None, ..Self::default() }
    }

    /// Whether an anchor is due at `(evm_number, timestamp_ms)`.
    ///
    /// `pruning_anchor` forces one regardless of cadence: a pruning-point advance
    /// is about to make everything below it unreconstructable, so that is exactly
    /// the moment an anchor has to exist.
    pub fn is_due(&self, meta: &EvmCheckpointMeta, evm_number: u64, timestamp_ms: u64, pruning_anchor: bool) -> bool {
        if pruning_anchor {
            return true;
        }
        if !meta.has_any {
            return true;
        }
        if evm_number <= meta.last_evm_number {
            // A reorg can move the sink back; never write an anchor for a number
            // already anchored (the row is keyed by block, so this would grow the
            // store without bounding anything).
            return false;
        }
        evm_number - meta.last_evm_number >= self.max_block_gap
            || timestamp_ms.saturating_sub(meta.last_timestamp_ms) >= self.min_interval_ms
    }

    /// Record a new anchor and return the anchors to evict, oldest first.
    pub fn record(&self, meta: &mut EvmCheckpointMeta, block: Hash64, evm_number: u64, timestamp_ms: u64) -> Vec<Hash64> {
        meta.last_evm_number = evm_number;
        meta.last_timestamp_ms = timestamp_ms;
        meta.has_any = true;
        meta.retained.push(block);
        match self.max_retained {
            Some(max) if meta.retained.len() > max => meta.retained.drain(..meta.retained.len() - max).collect(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod cadence_tests {
    use super::*;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    #[test]
    fn the_first_anchor_is_always_due() {
        let policy = EvmCheckpointPolicy::default();
        assert!(policy.is_due(&EvmCheckpointMeta::default(), 1, 0, false));
    }

    #[test]
    fn time_paces_the_cadence_and_the_block_gap_only_caps_it() {
        let policy = EvmCheckpointPolicy { min_interval_ms: 1000, max_block_gap: 100, ..Default::default() };
        let mut meta = EvmCheckpointMeta::default();
        policy.record(&mut meta, h(1), 10, 5_000);

        // Neither trigger: no anchor. This is the whole saving — v1 wrote one
        // every 2048 blocks here regardless.
        assert!(!policy.is_due(&meta, 50, 5_500, false));
        // Time trigger.
        assert!(policy.is_due(&meta, 50, 6_000, false));
        // Block-gap cap, without enough time having passed.
        assert!(policy.is_due(&meta, 110, 5_500, false));
    }

    #[test]
    fn a_pruning_advance_forces_an_anchor() {
        // Below the pruning point the diff chain is about to disappear, so the
        // cadence does not get a vote.
        let policy = EvmCheckpointPolicy { min_interval_ms: u64::MAX, max_block_gap: u64::MAX, ..Default::default() };
        let mut meta = EvmCheckpointMeta::default();
        policy.record(&mut meta, h(1), 10, 5_000);
        assert!(!policy.is_due(&meta, 11, 5_001, false));
        assert!(policy.is_due(&meta, 11, 5_001, true));
    }

    #[test]
    fn a_reorg_never_re_anchors_a_number_already_anchored() {
        let policy = EvmCheckpointPolicy { min_interval_ms: 0, max_block_gap: 1, ..Default::default() };
        let mut meta = EvmCheckpointMeta::default();
        policy.record(&mut meta, h(1), 100, 5_000);
        assert!(!policy.is_due(&meta, 100, 9_999, false));
        assert!(!policy.is_due(&meta, 99, 9_999, false));
        assert!(policy.is_due(&meta, 101, 9_999, false));
    }

    #[test]
    fn retention_evicts_oldest_first_and_archive_keeps_everything() {
        let policy = EvmCheckpointPolicy { max_retained: Some(2), ..Default::default() };
        let mut meta = EvmCheckpointMeta::default();
        assert!(policy.record(&mut meta, h(1), 1, 0).is_empty());
        assert!(policy.record(&mut meta, h(2), 2, 1).is_empty());
        assert_eq!(policy.record(&mut meta, h(3), 3, 2), vec![h(1)], "the oldest anchor is the one that leaves");
        assert_eq!(meta.retained, vec![h(2), h(3)]);

        let archive = EvmCheckpointPolicy::archive();
        let mut meta = EvmCheckpointMeta::default();
        for i in 1..10u8 {
            assert!(archive.record(&mut meta, h(i), i as u64, i as u64).is_empty());
        }
        assert_eq!(meta.retained.len(), 9);
    }
}
