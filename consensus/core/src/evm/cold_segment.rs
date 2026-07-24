//! Immutable cold segments for finalized EVM history.
//!
//! Everything else in this effort bounds what RocksDB holds. This is the escape
//! valve for the data an archive node genuinely must keep: history that is
//! finalized and will never change again, moved out of the hot database into
//! compressed, checksummed, append-only files.
//!
//! The distinction that makes it worth doing is not size, it is MUTABILITY.
//! RocksDB pays for the ability to overwrite and delete any key — compaction,
//! tombstones, bloom filters, write amplification — and finalized history needs
//! none of it. A segment file is written once, verified by checksum, and
//! thereafter only read or deleted whole. That means it can live on a slower or
//! cheaper volume, be copied between nodes, be verified offline, and be dropped
//! by an operator with `rm` rather than by a background pass that has to race
//! block production for the same disk.
//!
//! This module is the FORMAT and the manifest — the pure, offline-testable part.
//! Export and import live in the node layer, because they touch the stores.
//!
//! A segment is not a backup. It carries no signature and proves nothing about
//! the chain; the checksum only proves the bytes are the ones the exporter
//! wrote. A node importing a segment from elsewhere must still verify the
//! records against its own headers.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::{EvmH256, Hash64, blake2b_256_keyed};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Domain-separated key for segment checksums. Distinct from every other keyed
/// BLAKE2b context in MISAKA, so a segment digest can never be confused with a
/// checkpoint digest.
pub const EVM_COLD_SEGMENT_CHECKSUM_CONTEXT: &[u8] = b"MISAKA/evm-cold-segment/v1";

pub const EVM_COLD_SEGMENT_FORMAT: u16 = 1;

/// Which history a segment holds.
///
/// One kind per file rather than one file per block range holding everything:
/// an operator who wants receipts but not traces should be able to keep one and
/// delete the other, and a reader should not have to decompress traces to find a
/// receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ColdSegmentKind {
    Headers = 0,
    Payloads = 1,
    Receipts = 2,
    RawTransactions = 3,
    StateDiffs = 4,
    LogPostings = 5,
}

impl ColdSegmentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Headers => "headers",
            Self::Payloads => "payloads",
            Self::Receipts => "receipts",
            Self::RawTransactions => "raw-transactions",
            Self::StateDiffs => "state-diffs",
            Self::LogPostings => "log-postings",
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        [Self::Headers, Self::Payloads, Self::Receipts, Self::RawTransactions, Self::StateDiffs, Self::LogPostings]
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}

/// One record inside a segment: an EVM number, the L1 block it belongs to, and
/// the opaque encoded value.
///
/// The value stays opaque so the segment format does not have to change when a
/// store's value type does — a cold file written today must still be readable
/// after a value gains a field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColdRecord {
    pub evm_number: u64,
    pub block: Hash64,
    pub value: Vec<u8>,
}

/// A segment's header. Small enough to read without touching the payload, so a
/// node can decide whether a file is relevant before decompressing it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColdSegmentHeader {
    pub format_version: u16,
    pub kind: ColdSegmentKind,
    /// Inclusive EVM-number range.
    pub from_evm_number: u64,
    pub to_evm_number: u64,
    pub record_count: u64,
    pub uncompressed_len: u64,
    /// Keyed BLAKE2b-256 over the COMPRESSED payload.
    pub checksum: [u8; 32],
}

impl ColdSegmentHeader {
    pub fn covers(&self, evm_number: u64) -> bool {
        (self.from_evm_number..=self.to_evm_number).contains(&evm_number)
    }

    /// A stable file name. Zero-padded so lexicographic order is numeric order —
    /// which matters when the only tool available is `ls`.
    pub fn file_name(&self) -> String {
        format!("{}-{:012}-{:012}.mseg", self.kind.as_str(), self.from_evm_number, self.to_evm_number)
    }
}

/// One block's CONSENSUS binding inside a segment.
///
/// The checksum proves the bytes are the ones the builder wrote; it proves
/// nothing about whether the builder wrote the RIGHT bytes. A binding closes that
/// gap: the block's records must recompute to `committed_root` (a
/// `transactions_root` for a transaction segment), which is itself bound to
/// consensus via the block's `evm_commitment_root` and the L1 header chain. This
/// is what stops a builder bug from being archived as truth — the review's
/// precondition for treating any segment as authoritative.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockConsensusBinding {
    pub evm_number: u64,
    pub block: Hash64,
    pub committed_root: EvmH256,
}

/// A complete segment: header, compressed records, and per-block consensus
/// bindings. Empty `bindings` = an unbound segment (e.g. a headers archive that
/// carries its own roots); a transaction segment must be bound.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColdSegment {
    pub header: ColdSegmentHeader,
    pub payload: Vec<u8>,
    pub bindings: Vec<BlockConsensusBinding>,
}

#[derive(Debug, thiserror::Error)]
pub enum ColdSegmentError {
    #[error("segment is empty")]
    Empty,
    #[error("records are not sorted by evm_number ({0} after {1})")]
    Unsorted(u64, u64),
    #[error("unsupported segment format version {0}")]
    UnsupportedVersion(u16),
    #[error("segment checksum mismatch")]
    Checksum,
    #[error("segment payload decode: {0}")]
    Decode(String),
    #[error("consensus binding: {0}")]
    Binding(String),
}

fn segment_checksum(bytes: &[u8]) -> [u8; 32] {
    blake2b_256_keyed(EVM_COLD_SEGMENT_CHECKSUM_CONTEXT, bytes)
}

impl ColdSegment {
    /// Build a segment from records. They must be sorted and non-empty.
    ///
    /// Sortedness is enforced rather than fixed up: a segment claims a contiguous
    /// EVM-number range in its header, and silently reordering records would let a
    /// caller's bug become a file whose header lies about what is inside it.
    pub fn build(kind: ColdSegmentKind, records: &[ColdRecord]) -> Result<Self, ColdSegmentError> {
        let Some(first) = records.first() else { return Err(ColdSegmentError::Empty) };
        for pair in records.windows(2) {
            if pair[1].evm_number < pair[0].evm_number {
                return Err(ColdSegmentError::Unsorted(pair[1].evm_number, pair[0].evm_number));
            }
        }
        let last = records.last().expect("non-empty");

        let raw = borsh::to_vec(&records.to_vec()).expect("ColdRecord is infallibly borsh-serializable");
        let uncompressed_len = raw.len() as u64;
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&raw).expect("in-memory zlib write");
        let payload = encoder.finish().expect("in-memory zlib finish");

        Ok(Self {
            header: ColdSegmentHeader {
                format_version: EVM_COLD_SEGMENT_FORMAT,
                kind,
                from_evm_number: first.evm_number,
                to_evm_number: last.evm_number,
                record_count: records.len() as u64,
                uncompressed_len,
                checksum: segment_checksum(&payload),
            },
            payload,
            bindings: Vec::new(),
        })
    }

    /// Build a segment carrying per-block consensus bindings.
    pub fn build_with_bindings(
        kind: ColdSegmentKind,
        records: &[ColdRecord],
        bindings: Vec<BlockConsensusBinding>,
    ) -> Result<Self, ColdSegmentError> {
        let mut seg = Self::build(kind, records)?;
        seg.bindings = bindings;
        Ok(seg)
    }

    /// Verify every block's records recompute to its committed root.
    ///
    /// `recompute` receives one block's records in `(evm_number, tx_index)` order —
    /// the order the builder concatenated them, which is the order the root is
    /// defined over — and returns the root they produce. The check fails closed on
    /// any mismatch, any bound block with no records, or (when `require_all`) any
    /// record block with no binding, so a builder cannot ship a segment whose
    /// records disagree with what consensus committed.
    pub fn verify_consensus_binding<E: std::fmt::Display>(
        &self,
        require_all: bool,
        recompute: impl Fn(&[ColdRecord]) -> Result<EvmH256, E>,
    ) -> Result<(), ColdSegmentError> {
        use std::collections::BTreeMap;
        let records = self.records()?;
        let mut by_block: BTreeMap<Hash64, Vec<ColdRecord>> = BTreeMap::new();
        for r in records {
            by_block.entry(r.block).or_default().push(r);
        }
        let bound: std::collections::HashSet<Hash64> = self.bindings.iter().map(|b| b.block).collect();

        for binding in &self.bindings {
            let Some(block_records) = by_block.get(&binding.block) else {
                return Err(ColdSegmentError::Binding(format!("bound block {} has no records in the segment", binding.block)));
            };
            let got = recompute(block_records).map_err(|e| ColdSegmentError::Binding(format!("{}: {e}", binding.block)))?;
            if got != binding.committed_root {
                return Err(ColdSegmentError::Binding(format!(
                    "block {} records recompute to {:?}, committed {:?}",
                    binding.block, got, binding.committed_root
                )));
            }
        }
        if require_all {
            for block in by_block.keys() {
                if !bound.contains(block) {
                    return Err(ColdSegmentError::Binding(format!("block {block} has records but no consensus binding")));
                }
            }
        }
        Ok(())
    }

    /// Decode the records, verifying the checksum first.
    ///
    /// `uncompressed_len` bounds the inflate: the checksum proves the bytes are
    /// the ones written, not that they are safe to expand, and a cold file may
    /// have come from another machine.
    pub fn records(&self) -> Result<Vec<ColdRecord>, ColdSegmentError> {
        if self.header.format_version != EVM_COLD_SEGMENT_FORMAT {
            return Err(ColdSegmentError::UnsupportedVersion(self.header.format_version));
        }
        if segment_checksum(&self.payload) != self.header.checksum {
            return Err(ColdSegmentError::Checksum);
        }
        let mut raw = Vec::with_capacity(self.header.uncompressed_len.min(1 << 28) as usize);
        flate2::read::ZlibDecoder::new(&self.payload[..])
            .take(self.header.uncompressed_len)
            .read_to_end(&mut raw)
            .map_err(|e| ColdSegmentError::Decode(e.to_string()))?;
        if raw.len() as u64 != self.header.uncompressed_len {
            return Err(ColdSegmentError::Decode(format!("inflated {} != declared {}", raw.len(), self.header.uncompressed_len)));
        }
        let records: Vec<ColdRecord> = borsh::from_slice(&raw).map_err(|e| ColdSegmentError::Decode(e.to_string()))?;
        if records.len() as u64 != self.header.record_count {
            return Err(ColdSegmentError::Decode(format!("{} records != declared {}", records.len(), self.header.record_count)));
        }
        Ok(records)
    }

    /// Bytes as stored, for the manifest and for size reporting.
    pub fn stored_len(&self) -> u64 {
        self.payload.len() as u64
    }
}

/// What a node knows about its cold segments (prefix 229).
///
/// Kept in the database rather than derived by scanning a directory: a node must
/// be able to say what history it can serve WITHOUT touching a volume that may
/// be slow, remote, or temporarily unmounted. A missing file is then a detectable
/// inconsistency instead of silently narrowing what the node claims to have.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmColdSegmentManifest {
    pub segments: Vec<ColdSegmentHeader>,
    pub format_version: u16,
}

impl EvmColdSegmentManifest {
    pub fn new() -> Self {
        Self { segments: Vec::new(), format_version: EVM_COLD_SEGMENT_FORMAT }
    }

    /// Record a segment, keeping the list sorted and rejecting an overlap.
    ///
    /// Overlaps are refused rather than merged: two segments claiming the same
    /// EVM number would make "which file answers this query" ambiguous, and the
    /// answer would depend on iteration order.
    pub fn insert(&mut self, header: ColdSegmentHeader) -> Result<(), ColdSegmentError> {
        if self
            .segments
            .iter()
            .any(|s| s.kind == header.kind && s.from_evm_number <= header.to_evm_number && header.from_evm_number <= s.to_evm_number)
        {
            return Err(ColdSegmentError::Unsorted(header.from_evm_number, header.to_evm_number));
        }
        self.segments.push(header);
        self.segments.sort_by_key(|s| (s.kind, s.from_evm_number));
        self.format_version = EVM_COLD_SEGMENT_FORMAT;
        Ok(())
    }

    pub fn segment_for(&self, kind: ColdSegmentKind, evm_number: u64) -> Option<&ColdSegmentHeader> {
        self.segments.iter().find(|s| s.kind == kind && s.covers(evm_number))
    }

    /// The contiguous range this node can serve for `kind`, counting up from the
    /// oldest segment. A gap ends the range: history a node can only partly serve
    /// is history it should not claim.
    pub fn contiguous_range(&self, kind: ColdSegmentKind) -> Option<(u64, u64)> {
        let mut iter = self.segments.iter().filter(|s| s.kind == kind);
        let first = iter.next()?;
        let (from, mut to) = (first.from_evm_number, first.to_evm_number);
        for s in iter {
            if s.from_evm_number > to + 1 {
                break;
            }
            to = to.max(s.to_evm_number);
        }
        Some((from, to))
    }

    pub fn total_stored_bytes(&self) -> u64 {
        // Header-declared sizes, so this answers without reading the files.
        self.segments.iter().map(|s| s.uncompressed_len).sum()
    }
}

impl kaspa_utils::mem_size::MemSizeEstimator for EvmColdSegmentManifest {
    fn estimate_mem_bytes(&self) -> usize {
        size_of::<Self>() + self.segments.capacity() * size_of::<ColdSegmentHeader>()
    }
}

#[cfg(test)]
impl ColdSegment {
    fn format_version_bump(&mut self) {
        self.header.format_version = EVM_COLD_SEGMENT_FORMAT + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(n: u64) -> ColdRecord {
        ColdRecord { evm_number: n, block: Hash64::from_bytes([n as u8; 64]), value: vec![n as u8; 512] }
    }

    fn segment(kind: ColdSegmentKind, from: u64, to: u64) -> ColdSegment {
        ColdSegment::build(kind, &(from..=to).map(record).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn a_segment_round_trips_and_compresses() {
        let records: Vec<_> = (100..200).map(record).collect();
        let seg = ColdSegment::build(ColdSegmentKind::Receipts, &records).unwrap();

        assert_eq!(seg.header.from_evm_number, 100);
        assert_eq!(seg.header.to_evm_number, 199);
        assert_eq!(seg.header.record_count, 100);
        // Cold storage that did not shrink would be RocksDB with extra steps.
        assert!(seg.stored_len() < seg.header.uncompressed_len / 4, "{} vs {}", seg.stored_len(), seg.header.uncompressed_len);
        assert_eq!(seg.records().unwrap(), records);
    }

    #[test]
    fn unsorted_records_are_refused_rather_than_reordered() {
        // The header claims a contiguous range; silently sorting would turn a
        // caller's bug into a file whose header lies about its contents.
        let records = vec![record(5), record(3)];
        assert!(matches!(ColdSegment::build(ColdSegmentKind::Headers, &records), Err(ColdSegmentError::Unsorted(3, 5))));
        assert!(matches!(ColdSegment::build(ColdSegmentKind::Headers, &[]), Err(ColdSegmentError::Empty)));
    }

    #[test]
    fn corruption_and_version_skew_fail_closed() {
        let mut seg = segment(ColdSegmentKind::Payloads, 1, 10);
        let good = seg.clone();

        seg.payload[0] ^= 0xff;
        assert!(matches!(seg.records(), Err(ColdSegmentError::Checksum)));

        let mut wrong_version = good.clone();
        wrong_version.format_version_bump();
        assert!(matches!(wrong_version.records(), Err(ColdSegmentError::UnsupportedVersion(_))));

        // A header that disagrees with its payload is corruption too — the count
        // is part of what the file promises.
        let mut lying_count = good;
        lying_count.header.record_count = 99;
        assert!(matches!(lying_count.records(), Err(ColdSegmentError::Decode(_))));
    }

    #[test]
    fn a_lying_uncompressed_len_cannot_make_the_decoder_allocate_without_bound() {
        // Cold files may arrive from another machine; the checksum proves the
        // bytes are what was written, not that expanding them is safe.
        let mut seg = segment(ColdSegmentKind::StateDiffs, 1, 50);
        seg.header.uncompressed_len = 16;
        seg.header.checksum = segment_checksum(&seg.payload);
        assert!(matches!(seg.records(), Err(ColdSegmentError::Decode(_))));
    }

    #[test]
    fn the_manifest_refuses_overlapping_segments() {
        // Two files claiming the same number would make "which one answers" depend
        // on iteration order.
        let mut m = EvmColdSegmentManifest::new();
        m.insert(segment(ColdSegmentKind::Receipts, 0, 99).header).unwrap();
        assert!(m.insert(segment(ColdSegmentKind::Receipts, 50, 149).header).is_err());
        // A different kind over the same range is fine — they are separate files.
        m.insert(segment(ColdSegmentKind::Headers, 0, 99).header).unwrap();
        assert_eq!(m.segments.len(), 2);
    }

    #[test]
    fn lookup_and_contiguous_range_stop_at_a_gap() {
        let mut m = EvmColdSegmentManifest::new();
        m.insert(segment(ColdSegmentKind::Receipts, 0, 99).header).unwrap();
        m.insert(segment(ColdSegmentKind::Receipts, 100, 199).header).unwrap();
        // Deliberate gap at 200..299.
        m.insert(segment(ColdSegmentKind::Receipts, 300, 399).header).unwrap();

        assert!(m.segment_for(ColdSegmentKind::Receipts, 150).is_some());
        assert!(m.segment_for(ColdSegmentKind::Receipts, 250).is_none());
        // History a node can only partly serve is history it must not claim.
        assert_eq!(m.contiguous_range(ColdSegmentKind::Receipts), Some((0, 199)));
        assert_eq!(m.contiguous_range(ColdSegmentKind::Payloads), None);
    }

    fn root(b: u8) -> EvmH256 {
        EvmH256::from_bytes([b; 32])
    }

    #[test]
    fn a_binding_that_matches_the_recompute_passes_and_a_mismatch_fails_closed() {
        // The recompute stand-in: a block's root is its evm_number as a byte. A
        // real builder uses transactions_root; the grouping and compare are what
        // this exercises.
        let records = vec![record(1), record(1), record(2)];
        let bindings = vec![
            BlockConsensusBinding { evm_number: 1, block: records[0].block, committed_root: root(1) },
            BlockConsensusBinding { evm_number: 2, block: records[2].block, committed_root: root(2) },
        ];
        let seg = ColdSegment::build_with_bindings(ColdSegmentKind::RawTransactions, &records, bindings).unwrap();

        // Correct recompute (root = first record's evm_number) verifies.
        seg.verify_consensus_binding::<String>(true, |recs| Ok(root(recs[0].evm_number as u8))).unwrap();

        // A builder bug — records that do not produce the committed root — is
        // caught rather than archived.
        let err = seg.verify_consensus_binding::<String>(true, |_| Ok(root(0xFF))).unwrap_err();
        assert!(matches!(err, ColdSegmentError::Binding(_)), "{err:?}");
    }

    #[test]
    fn require_all_rejects_a_block_with_records_but_no_binding() {
        // A transaction segment must bind EVERY block; an unbound block is a hole a
        // builder could smuggle unverified records through.
        let records = vec![record(1), record(2)];
        let bindings = vec![BlockConsensusBinding { evm_number: 1, block: records[0].block, committed_root: root(1) }];
        let seg = ColdSegment::build_with_bindings(ColdSegmentKind::RawTransactions, &records, bindings).unwrap();
        let err = seg.verify_consensus_binding::<String>(true, |recs| Ok(root(recs[0].evm_number as u8))).unwrap_err();
        assert!(matches!(err, ColdSegmentError::Binding(_)), "{err:?}");
        // Without require_all the partially-bound segment is accepted (bound blocks
        // still checked).
        seg.verify_consensus_binding::<String>(false, |recs| Ok(root(recs[0].evm_number as u8))).unwrap();
    }

    #[test]
    fn a_binding_for_a_block_with_no_records_fails() {
        let records = vec![record(1)];
        let bindings = vec![BlockConsensusBinding { evm_number: 9, block: Hash64::from_bytes([9; 64]), committed_root: root(9) }];
        let seg = ColdSegment::build_with_bindings(ColdSegmentKind::RawTransactions, &records, bindings).unwrap();
        assert!(seg.verify_consensus_binding::<String>(false, |_| Ok(root(9))).is_err());
    }

    #[test]
    fn the_delete_floor_is_the_pruning_point_when_export_is_off() {
        // Inert by default: pruning reclaims EVM rows up to the pruning point,
        // exactly as before the interlock existed.
        assert_eq!(evm_row_delete_floor(EvmSegmentExport::Off, 1_000, 200), 1_000);
        // The export cursor is ignored when off.
        assert_eq!(evm_row_delete_floor(EvmSegmentExport::Off, 1_000, 0), 1_000);
    }

    #[test]
    fn the_delete_floor_holds_rows_the_export_has_not_covered() {
        // With export on, the pruner never reclaims past what the export archived.
        assert_eq!(evm_row_delete_floor(EvmSegmentExport::Async, 1_000, 600), 600);
        // A caught-up export lets pruning proceed to the pruning point.
        assert_eq!(evm_row_delete_floor(EvmSegmentExport::Async, 1_000, 1_000), 1_000);
        // A stalled export (cursor 0) holds ALL EVM rows rather than losing them —
        // the safe direction: history is retained, disk grows, and the lag metric
        // fires. It never deletes an unexported row.
        assert_eq!(evm_row_delete_floor(EvmSegmentExport::Async, 1_000, 0), 0);
    }

    #[test]
    fn export_lag_is_the_uncovered_distance() {
        assert_eq!(evm_export_lag_blocks(1_000, 600), 400);
        assert_eq!(evm_export_lag_blocks(1_000, 1_000), 0);
        // Cursor ahead of the pruning point (shouldn't happen) saturates to 0.
        assert_eq!(evm_export_lag_blocks(1_000, 1_200), 0);
    }

    #[test]
    fn file_names_sort_numerically() {
        // Zero padding matters when the only available tool is `ls`.
        let a = segment(ColdSegmentKind::Receipts, 9, 9).header.file_name();
        let b = segment(ColdSegmentKind::Receipts, 100, 100).header.file_name();
        assert!(a < b, "{a} should sort before {b}");
        assert!(a.ends_with(".mseg"));
    }
}

/// §5.8: whether finalized EVM history is exported to cold segments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvmSegmentExport {
    /// No export. The EVM-row delete floor is the pruning point (today's behaviour).
    #[default]
    Off,
    /// Export the finalized range to cold segments at pruning advance, and hold any
    /// EVM row the export has not yet covered.
    Async,
}

impl EvmSegmentExport {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "async" => Some(Self::Async),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Async => "async",
        }
    }
    pub fn is_active(self) -> bool {
        matches!(self, Self::Async)
    }
}

/// The EVM-row deletion floor for a pruning pass (§5.8 interlock).
///
/// Below the returned EVM number, a pruned block's EVM history rows may be
/// deleted; at or above it they are HELD. With export off the floor is the
/// pruning point, so pruning behaves exactly as before. With export on the floor
/// is `min(pruning_point, export_cursor)` — the pruner never reclaims a row the
/// export has not archived, so a lagging or failed export holds history rather
/// than losing it. L1 pruning (the pruning-point advance, UTXO deletion) is NOT
/// gated by this; only the EVM-row deletes are.
pub fn evm_row_delete_floor(export: EvmSegmentExport, pruning_point_evm_number: u64, export_cursor: u64) -> u64 {
    if export.is_active() {
        pruning_point_evm_number.min(export_cursor)
    } else {
        pruning_point_evm_number
    }
}

/// How far behind the pruning point the export is, in EVM blocks (§5.8 metric
/// `export_lag_blocks`). A rising lag under an active export is the signal that
/// the builder cannot keep up and history is being held rather than reclaimed.
pub fn evm_export_lag_blocks(pruning_point_evm_number: u64, export_cursor: u64) -> u64 {
    pruning_point_evm_number.saturating_sub(export_cursor)
}
