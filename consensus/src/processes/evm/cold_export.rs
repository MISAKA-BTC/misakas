//! Exporting finalized EVM history out of RocksDB into cold segment files.
//!
//! The format lives in `kaspa_consensus_core::evm::cold_segment`; this is the
//! side that touches the database and the filesystem.
//!
//! Order matters more than it looks. A segment must be written, flushed and
//! VERIFIED BY RE-READING before the rows it came from are deleted, and the
//! manifest entry must land before the delete too. Any other order has a window
//! in which a crash loses history permanently: unlike an index, a receipt or a
//! payload cannot be regenerated from anything the node still holds.
//!
//! That is also why the export is opt-in and never runs on its own. Moving data
//! to a volume the operator has not chosen — and then deleting the original —
//! is not a decision a background pass should make.

use kaspa_consensus_core::evm::{ColdRecord, ColdSegment, ColdSegmentError, ColdSegmentKind, EvmColdSegmentManifest};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ColdExportError {
    #[error("cold segment: {0}")]
    Segment(#[from] ColdSegmentError),
    #[error("cold segment I/O at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("verification failed after writing {0}: the file does not read back as written")]
    VerifyFailed(PathBuf),
    #[error("refusing to overwrite an existing segment at {0}")]
    Exists(PathBuf),
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> ColdExportError + '_ {
    move |source| ColdExportError::Io { path: path.to_path_buf(), source }
}

/// Write a segment, then read it back and check it decodes to the same records.
///
/// The read-back is not paranoia: this is the step after which the hot rows get
/// deleted, and a partial write that still checksums (because the checksum lives
/// in the same partially-written file) would otherwise be discovered only when
/// someone queries that range, long after the source is gone.
pub fn write_segment(dir: &Path, segment: &ColdSegment) -> Result<PathBuf, ColdExportError> {
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let path = dir.join(segment.header.file_name());
    // Never silently replace: a same-named file means the range was already
    // exported, and overwriting it would destroy the copy the manifest points at.
    if path.exists() {
        return Err(ColdExportError::Exists(path));
    }

    let encoded = borsh::to_vec(segment).expect("ColdSegment is infallibly borsh-serializable");
    // Write to a temporary name and rename: a reader must never observe a
    // half-written segment under its final name.
    let tmp = path.with_extension("mseg.partial");
    {
        let mut file = std::fs::File::create(&tmp).map_err(io(&tmp))?;
        file.write_all(&encoded).map_err(io(&tmp))?;
        // fsync before the rename, or the rename can be durable while the
        // contents are not.
        file.sync_all().map_err(io(&tmp))?;
    }
    std::fs::rename(&tmp, &path).map_err(io(&path))?;

    let verified = read_segment(&path)?;
    if verified.header != segment.header || verified.records()? != segment.records()? {
        return Err(ColdExportError::VerifyFailed(path));
    }
    Ok(path)
}

pub fn read_segment(path: &Path) -> Result<ColdSegment, ColdExportError> {
    let bytes = std::fs::read(path).map_err(io(path))?;
    let segment: ColdSegment = borsh::from_slice(&bytes).map_err(|e| ColdSegmentError::Decode(format!("{}: {e}", path.display())))?;
    // Force the checksum and bounded-inflate checks now, so a corrupt file is
    // rejected at open rather than at the first query against it.
    segment.records()?;
    Ok(segment)
}

/// Export one range and record it, WITHOUT deleting anything.
///
/// Deletion is the caller's separate step, gated on this returning `Ok`. Keeping
/// them apart is what makes the export safe to run on a live node and safe to
/// abandon halfway: the worst outcome is a segment file that duplicates data the
/// database still has.
pub fn export_range(
    dir: &Path,
    kind: ColdSegmentKind,
    records: &[ColdRecord],
    manifest: &mut EvmColdSegmentManifest,
) -> Result<PathBuf, ColdExportError> {
    let segment = ColdSegment::build(kind, records)?;
    let path = write_segment(dir, &segment)?;
    manifest.insert(segment.header)?;
    Ok(path)
}

/// Export a transaction segment, BOUND to consensus and verified at build.
///
/// The records are raw EIP-2718 bytes in `(evm_number, tx_index)` order. For each
/// block the builder recomputes `transactions_root` from its records and requires
/// it to equal the committed root — so a builder bug produces a build error here,
/// not an archived-and-trusted lie. The verification is the exact one §5.5.2
/// specifies: `transactions_root`, which is bound to consensus via the block's
/// `evm_commitment_root`, NOT KIP-15's `accepted_id_merkle_root` (design #3).
///
/// `committed_root_of` returns each block's committed `transactions_root` (from
/// its `EvmExecutionHeader`). Every block appearing in `records` must have one, or
/// the export refuses — an unbound block is a record set nothing vouches for.
#[cfg(feature = "evm")]
pub fn export_transaction_segment(
    dir: &Path,
    records: &[ColdRecord],
    committed_root_of: impl Fn(kaspa_hashes::Hash64) -> Option<kaspa_hashes::EvmH256>,
    manifest: &mut EvmColdSegmentManifest,
) -> Result<PathBuf, ColdExportError> {
    use kaspa_consensus_core::evm::BlockConsensusBinding;
    use std::collections::BTreeSet;

    // One binding per distinct block, in appearance order.
    let mut seen = BTreeSet::new();
    let mut bindings = Vec::new();
    for r in records {
        if seen.insert(r.block) {
            let committed = committed_root_of(r.block)
                .ok_or_else(|| ColdSegmentError::Binding(format!("no committed transactions_root for block {}", r.block)))?;
            bindings.push(BlockConsensusBinding { evm_number: r.evm_number, block: r.block, committed_root: committed });
        }
    }

    let segment = ColdSegment::build_with_bindings(ColdSegmentKind::RawTransactions, records, bindings)?;
    // Verify BEFORE writing: recompute transactions_root from each block's raw txs.
    segment.verify_consensus_binding(true, |block_records| {
        let raws: Vec<Vec<u8>> = block_records.iter().map(|r| r.value.clone()).collect();
        Ok::<_, std::convert::Infallible>(kaspa_evm::roots::transactions_root(&raws))
    })?;
    let path = write_segment(dir, &segment)?;
    manifest.insert(segment.header)?;
    Ok(path)
}

/// §5.1 verification-mode metric: receipt segments REFUSED because their records
/// did not recompute to the committed `receipts_root`. Nonzero means a builder
/// produced receipts consensus did not commit; that segment was NOT written (the
/// point of verify-mode — never silently archive a divergent segment). kaspad
/// surfaces this; a persistently climbing value is a builder bug, not an
/// operational hiccup.
#[cfg(feature = "evm")]
static RECEIPT_BINDING_REJECTIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Read the §5.1 receipt-binding rejection counter (for the kaspad metric).
#[cfg(feature = "evm")]
pub fn receipt_binding_rejections() -> u64 {
    RECEIPT_BINDING_REJECTIONS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Per-block data to bind a receipt segment to consensus (§5.1).
///
/// A receipts root is FENCE-SENSITIVE where a transactions root is not: at/above
/// the `evm_typed_receipt_root` fence it is the Ethereum EIP-2718 typed root,
/// which needs each receipt's tx type — read from `executed_raws`, parallel to
/// the block's receipts in `(tx_index)` order; below the fence it is the v1
/// borsh-MPT root over the receipts alone. So the binding carries, per block: the
/// committed root, which side of the fence the block is on, and (only meaningful
/// for v2) the executed raws. Build `executed_raws` in the SAME order the receipt
/// records appear for the block, or the v2 zip binds the wrong types.
#[cfg(feature = "evm")]
pub struct ReceiptBlockBinding {
    pub committed_root: kaspa_hashes::EvmH256,
    /// `daa_score >= evm_typed_receipt_root_activation_daa_score` for this block.
    pub typed_v2: bool,
    /// Accepted-and-executed raw txs, tx_index-parallel to the block's receipts.
    /// Ignored below the fence; may be empty there.
    pub executed_raws: Vec<Vec<u8>>,
}

/// Export a RECEIPT segment, BOUND to consensus and verified at build (§5.1).
///
/// The records are borsh-encoded `EvmReceipt`s in `(evm_number, tx_index)` order
/// — the exact preimage the block committed: accepted-and-executed txs only (a
/// deterministically-skipped tx contributes no receipt, so the vector is dense),
/// and system ops are NOT here (they commit under a separate `system_ops_root`).
/// For each block the builder decodes its receipts, recomputes the FENCE-CORRECT
/// `receipts_root` via [`kaspa_evm::roots::receipts_root_for_fence`] — the same
/// selector the executor commits through — and requires it to equal the committed
/// root. A mismatch fails the build (nothing is written) and bumps
/// `receipt_binding_rejections`: verify-mode refuses to archive receipts that
/// consensus did not commit, rather than silently trusting the builder.
///
/// `block_of` returns each block's binding (committed root, fence side, executed
/// raws). Every block appearing in `records` must have one, or the export refuses
/// — an unbound block is a record set nothing vouches for.
#[cfg(feature = "evm")]
pub fn export_receipt_segment(
    dir: &Path,
    records: &[ColdRecord],
    block_of: impl Fn(kaspa_hashes::Hash64) -> Option<ReceiptBlockBinding>,
    manifest: &mut EvmColdSegmentManifest,
) -> Result<PathBuf, ColdExportError> {
    use kaspa_consensus_core::evm::{BlockConsensusBinding, EvmReceipt};
    use std::collections::{BTreeSet, HashMap};

    // One binding per distinct block, in appearance order; keep the fence side +
    // raws aside for the recompute (which only sees a block's records).
    let mut seen = BTreeSet::new();
    let mut bindings = Vec::new();
    let mut aux: HashMap<kaspa_hashes::Hash64, ReceiptBlockBinding> = HashMap::new();
    for r in records {
        if seen.insert(r.block) {
            let b = block_of(r.block)
                .ok_or_else(|| ColdSegmentError::Binding(format!("no committed receipts_root for block {}", r.block)))?;
            bindings.push(BlockConsensusBinding { evm_number: r.evm_number, block: r.block, committed_root: b.committed_root });
            aux.insert(r.block, b);
        }
    }

    let segment = ColdSegment::build_with_bindings(ColdSegmentKind::Receipts, records, bindings)?;
    // Verify BEFORE writing: decode each block's receipts and recompute the
    // fence-correct root. A rejection is counted so it is distinguishable from an
    // I/O failure — the operator needs to know a builder diverged, not just that
    // an export did not advance.
    let verified = segment.verify_consensus_binding(true, |block_records| {
        let block = block_records[0].block;
        let a = aux.get(&block).expect("every bound block was inserted into aux");
        let receipts: Vec<EvmReceipt> = block_records
            .iter()
            .map(|r| borsh::from_slice::<EvmReceipt>(&r.value))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("undecodable receipt in block {block}: {e}"))?;
        // v2 zips receipts with executed_raws by index; a length mismatch would
        // bind against a silently-wrong type set. Below the fence the raws are
        // ignored, so no such requirement applies.
        if a.typed_v2 && a.executed_raws.len() != receipts.len() {
            return Err(format!(
                "block {block}: {} receipts but {} executed raws — cannot bind the v2 typed receipts root",
                receipts.len(),
                a.executed_raws.len()
            ));
        }
        Ok::<_, String>(kaspa_evm::roots::receipts_root_for_fence(a.typed_v2, &receipts, &a.executed_raws))
    });
    if verified.is_err() {
        RECEIPT_BINDING_REJECTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    verified?;
    let path = write_segment(dir, &segment)?;
    manifest.insert(segment.header)?;
    Ok(path)
}

/// Resolve one record from cold storage.
///
/// Returns `Ok(None)` when no segment covers the number — "this node does not
/// have it", which the caller reports as pruned rather than as empty.
pub fn lookup(
    dir: &Path,
    manifest: &EvmColdSegmentManifest,
    kind: ColdSegmentKind,
    evm_number: u64,
) -> Result<Option<ColdRecord>, ColdExportError> {
    let Some(header) = manifest.segment_for(kind, evm_number) else { return Ok(None) };
    let segment = read_segment(&dir.join(header.file_name()))?;
    Ok(segment.records()?.into_iter().find(|r| r.evm_number == evm_number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash64;

    fn record(n: u64) -> ColdRecord {
        ColdRecord { evm_number: n, block: Hash64::from_bytes([n as u8; 64]), value: vec![7u8; 256] }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("misaka-cold-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn export_then_look_up_a_record() {
        let dir = temp_dir("roundtrip");
        let mut manifest = EvmColdSegmentManifest::new();
        let records: Vec<_> = (10..20).map(record).collect();

        let path = export_range(&dir, ColdSegmentKind::Receipts, &records, &mut manifest).unwrap();
        assert!(path.exists());
        assert_eq!(manifest.contiguous_range(ColdSegmentKind::Receipts), Some((10, 19)));

        assert_eq!(lookup(&dir, &manifest, ColdSegmentKind::Receipts, 15).unwrap(), Some(record(15)));
        // Outside every segment: "not here", which the caller reports as pruned
        // rather than as an empty result.
        assert_eq!(lookup(&dir, &manifest, ColdSegmentKind::Receipts, 99).unwrap(), None);
        // A different kind is a different file, even over the same range.
        assert_eq!(lookup(&dir, &manifest, ColdSegmentKind::Headers, 15).unwrap(), None);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_existing_segment_is_never_overwritten() {
        // A same-named file means the range was already exported; replacing it
        // would destroy the copy the manifest points at.
        let dir = temp_dir("no-overwrite");
        let mut manifest = EvmColdSegmentManifest::new();
        let records: Vec<_> = (0..5).map(record).collect();
        export_range(&dir, ColdSegmentKind::Payloads, &records, &mut manifest).unwrap();

        let mut second = EvmColdSegmentManifest::new();
        assert!(matches!(export_range(&dir, ColdSegmentKind::Payloads, &records, &mut second), Err(ColdExportError::Exists(_))));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_partial_file_is_left_under_the_final_name() {
        // The rename is what guarantees a reader never sees a half-written
        // segment; the `.partial` must not survive a successful export.
        let dir = temp_dir("atomic");
        let mut manifest = EvmColdSegmentManifest::new();
        export_range(&dir, ColdSegmentKind::Headers, &(0..3).map(record).collect::<Vec<_>>(), &mut manifest).unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("partial"))
            .collect();
        assert!(leftovers.is_empty(), "a partial file survived a successful export: {leftovers:?}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(feature = "evm")]
    fn tx_record(evm_number: u64, block: u8, raw: &[u8]) -> ColdRecord {
        ColdRecord { evm_number, block: Hash64::from_bytes([block; 64]), value: raw.to_vec() }
    }

    #[test]
    #[cfg(feature = "evm")]
    fn a_transaction_segment_verifies_its_transactions_root_at_build() {
        // Two blocks, real transactions_root over their raw bytes. The committed
        // root is what the block actually committed; export must reproduce it.
        let dir = temp_dir("txbind-ok");
        let b1: Vec<Vec<u8>> = vec![vec![1, 2, 3], vec![4, 5]];
        let b2: Vec<Vec<u8>> = vec![vec![9, 9, 9, 9]];
        let records = vec![tx_record(1, 1, &b1[0]), tx_record(1, 1, &b1[1]), tx_record(2, 2, &b2[0])];
        let root1 = kaspa_evm::roots::transactions_root(&b1);
        let root2 = kaspa_evm::roots::transactions_root(&b2);
        let committed = move |block: Hash64| {
            if block == Hash64::from_bytes([1; 64]) {
                Some(root1)
            } else if block == Hash64::from_bytes([2; 64]) {
                Some(root2)
            } else {
                None
            }
        };

        let mut manifest = EvmColdSegmentManifest::new();
        let path = export_transaction_segment(&dir, &records, committed, &mut manifest).unwrap();
        assert!(path.exists(), "a correctly-bound segment is written");

        // A builder that miscommits — a committed root that the records do not
        // reproduce — fails at BUILD, before anything is archived.
        let dir2 = temp_dir("txbind-bad");
        let wrong = |_: Hash64| Some(kaspa_hashes::EvmH256::from_bytes([0xEE; 32]));
        let mut m2 = EvmColdSegmentManifest::new();
        let err = export_transaction_segment(&dir2, &records, wrong, &mut m2).unwrap_err();
        assert!(matches!(err, ColdExportError::Segment(ColdSegmentError::Binding(_))), "{err:?}");
        assert!(
            !dir2.join("raw-transactions-000000000001-000000000002.mseg").exists(),
            "a segment failing its binding must not be written"
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&dir2).ok();
    }

    // --- §5.1 receipts_root binding (fence-aware, verify-mode) ---

    #[cfg(feature = "evm")]
    fn receipt_record(evm_number: u64, block: u8, r: &kaspa_consensus_core::evm::EvmReceipt) -> ColdRecord {
        ColdRecord { evm_number, block: Hash64::from_bytes([block; 64]), value: borsh::to_vec(r).unwrap() }
    }

    #[cfg(feature = "evm")]
    fn a_receipt(succeeded: bool, gas: u64) -> kaspa_consensus_core::evm::EvmReceipt {
        kaspa_consensus_core::evm::EvmReceipt { succeeded, cumulative_gas_used: gas, gas_used: gas, logs: vec![] }
    }

    #[test]
    #[cfg(feature = "evm")]
    fn a_receipt_segment_binds_the_committed_root_and_rejects_a_mismatch() {
        use kaspa_evm::roots::receipts_root_for_fence;
        // Two v1 (below-fence) blocks, real receipts, the roots the blocks committed.
        let b1 = [a_receipt(true, 21_000), a_receipt(false, 42_000)];
        let b2 = [a_receipt(true, 30_000)];
        let records = vec![receipt_record(1, 1, &b1[0]), receipt_record(1, 1, &b1[1]), receipt_record(2, 2, &b2[0])];
        let root1 = receipts_root_for_fence(false, &b1, &[]);
        let root2 = receipts_root_for_fence(false, &b2, &[]);
        let bind_ok = move |block: Hash64| {
            let root = if block == Hash64::from_bytes([1; 64]) { root1 } else { root2 };
            Some(ReceiptBlockBinding { committed_root: root, typed_v2: false, executed_raws: vec![] })
        };

        let dir = temp_dir("receipt-ok");
        let mut m = EvmColdSegmentManifest::new();
        let before = receipt_binding_rejections();
        let path = export_receipt_segment(&dir, &records, bind_ok, &mut m).unwrap();
        assert!(path.exists(), "a correctly-bound receipt segment is written");
        assert_eq!(receipt_binding_rejections(), before, "no rejection on the happy path");

        // A builder whose committed root the records do NOT reproduce: rejected at
        // BUILD (nothing archived), and the §5.1 metric records it.
        let dir2 = temp_dir("receipt-bad");
        let wrong = |_: Hash64| {
            Some(ReceiptBlockBinding {
                committed_root: kaspa_hashes::EvmH256::from_bytes([0xEE; 32]),
                typed_v2: false,
                executed_raws: vec![],
            })
        };
        let mut m2 = EvmColdSegmentManifest::new();
        let err = export_receipt_segment(&dir2, &records, wrong, &mut m2).unwrap_err();
        assert!(matches!(err, ColdExportError::Segment(ColdSegmentError::Binding(_))), "{err:?}");
        assert_eq!(receipt_binding_rejections(), before + 1, "a mismatch bumps the rejection metric");
        assert!(!dir2.join("receipts-000000000001-000000000002.mseg").exists(), "a mismatching segment is never written");

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&dir2).ok();
    }

    #[test]
    #[cfg(feature = "evm")]
    fn the_receipt_binding_is_fence_correct() {
        // The activation boundary is part of the canonicalization: a block above
        // the fence must bind with its v2 root, and binding it with the v1 root
        // (the wrong side) is rejected. A type-2 tx makes v1 != v2.
        use kaspa_evm::roots::{receipts_root, receipts_root_for_fence};
        let r = [a_receipt(true, 21_000)];
        let raws = vec![vec![0x02u8, 0xde, 0xad]]; // EIP-1559 → v2 differs from v1
        let records = vec![receipt_record(7, 7, &r[0])];
        let v2_root = receipts_root_for_fence(true, &r, &raws);
        assert_ne!(v2_root, receipts_root(&r), "precondition: this receipt's v2 root differs from v1");

        // Bound correctly as v2 (with the executed raws): writes.
        let dir = temp_dir("receipt-v2-ok");
        let mut m = EvmColdSegmentManifest::new();
        let raws_c = raws.clone();
        let ok = move |_: Hash64| Some(ReceiptBlockBinding { committed_root: v2_root, typed_v2: true, executed_raws: raws_c.clone() });
        assert!(export_receipt_segment(&dir, &records, ok, &mut m).unwrap().exists());

        // Same block, same committed v2 root, but bound as v1 (wrong fence side):
        // the v1 recompute cannot reproduce the v2 root ⇒ rejected.
        let dir2 = temp_dir("receipt-v2-wrongside");
        let mut m2 = EvmColdSegmentManifest::new();
        let wrong_side = move |_: Hash64| Some(ReceiptBlockBinding { committed_root: v2_root, typed_v2: false, executed_raws: vec![] });
        assert!(matches!(
            export_receipt_segment(&dir2, &records, wrong_side, &mut m2).unwrap_err(),
            ColdExportError::Segment(ColdSegmentError::Binding(_))
        ));

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&dir2).ok();
    }

    #[test]
    #[cfg(feature = "evm")]
    fn system_ops_and_skips_are_outside_the_receipts_preimage() {
        // The committed receipts_root is over the ACCEPTED-AND-EXECUTED receipts
        // ONLY: system ops commit under a separate root, and a skipped tx leaves no
        // receipt (the vector is dense, no placeholder). So the exact dense record
        // set binds, and any extra record — a stand-in for a system-op receipt, or
        // a placeholder a builder wrongly inserted for a skip — fails to reproduce
        // the committed root.
        use kaspa_evm::roots::receipts_root_for_fence;
        let executed = [a_receipt(true, 21_000), a_receipt(true, 45_000)]; // txs 0 and 2 executed; tx 1 skipped
        let committed = receipts_root_for_fence(false, &executed, &[]);
        let bind = move |_: Hash64| Some(ReceiptBlockBinding { committed_root: committed, typed_v2: false, executed_raws: vec![] });

        // Dense, correct preimage: binds.
        let dense = vec![receipt_record(5, 5, &executed[0]), receipt_record(5, 5, &executed[1])];
        let dir = temp_dir("receipt-dense");
        let mut m = EvmColdSegmentManifest::new();
        assert!(export_receipt_segment(&dir, &dense, bind, &mut m).unwrap().exists());

        // One extra receipt (a phantom system-op receipt / skip placeholder):
        // recomputes to a different root ⇒ rejected, never archived.
        let padded =
            vec![receipt_record(5, 5, &executed[0]), receipt_record(5, 5, &a_receipt(true, 1)), receipt_record(5, 5, &executed[1])];
        let dir2 = temp_dir("receipt-padded");
        let mut m2 = EvmColdSegmentManifest::new();
        let bind2 = move |_: Hash64| Some(ReceiptBlockBinding { committed_root: committed, typed_v2: false, executed_raws: vec![] });
        assert!(matches!(
            export_receipt_segment(&dir2, &padded, bind2, &mut m2).unwrap_err(),
            ColdExportError::Segment(ColdSegmentError::Binding(_))
        ));

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&dir2).ok();
    }

    #[test]
    fn a_corrupt_file_is_rejected_at_open_not_at_query_time() {
        let dir = temp_dir("corrupt");
        let mut manifest = EvmColdSegmentManifest::new();
        let path = export_range(&dir, ColdSegmentKind::StateDiffs, &(0..4).map(record).collect::<Vec<_>>(), &mut manifest).unwrap();

        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, bytes).unwrap();

        assert!(read_segment(&path).is_err(), "corruption must surface when the file is opened");
        assert!(lookup(&dir, &manifest, ColdSegmentKind::StateDiffs, 1).is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

// ---------------------------------------------------------------------------
// §5.8 — pruning-time state-history export. Archives the finalized EVM history a
// pruning pass is about to reclaim into cold segments BEFORE the rows are
// deleted, and returns how far the export advanced so the interlock floor can
// hold anything not yet covered.
// ---------------------------------------------------------------------------

#[cfg(feature = "evm")]
use kaspa_consensus_core::evm::{EvmExecutionHeader, EvmStateDiffV2};

/// One EVM-active block's rows, for a state-era export.
#[cfg(feature = "evm")]
pub struct StateHistoryRow {
    pub evm_number: u64,
    pub block: kaspa_hashes::Hash64,
    pub header: EvmExecutionHeader,
    /// The forward state diff, if this node kept one (recent/archive history).
    pub diff: Option<EvmStateDiffV2>,
}

/// Export a contiguous EVM-number range `[from, to)` of state history to cold
/// segments and record them in the manifest.
///
/// Two segments: a HEADERS segment (each `EvmExecutionHeader` carries the
/// `transactions_root`/`receipts_root`/`state_root` that consensus committed via
/// `evm_commitment_root`, so the segment is self-binding) and a DIFFS segment
/// (the material to replay state forward from an anchor). Returns the new export
/// cursor = `to`.
///
/// Fails closed: on any I/O or build error nothing is recorded and the cursor
/// does not advance, so the interlock keeps holding the un-exported rows rather
/// than letting the pruner delete them.
#[cfg(feature = "evm")]
pub fn export_state_history_range(
    dir: &Path,
    rows: &[StateHistoryRow],
    manifest: &mut EvmColdSegmentManifest,
) -> Result<u64, ColdExportError> {
    if rows.is_empty() {
        return Err(ColdSegmentError::Empty.into());
    }
    // Records must be sorted by evm_number (ColdSegment::build enforces it too).
    debug_assert!(rows.windows(2).all(|w| w[0].evm_number <= w[1].evm_number));

    let header_records: Vec<ColdRecord> = rows
        .iter()
        .map(|r| ColdRecord {
            evm_number: r.evm_number,
            block: r.block,
            value: borsh::to_vec(&r.header).expect("EvmExecutionHeader is infallibly borsh-serializable"),
        })
        .collect();
    export_range(dir, ColdSegmentKind::Headers, &header_records, manifest)?;

    // Diffs are present only in recent/archive history; export whatever the node kept.
    let diff_records: Vec<ColdRecord> = rows
        .iter()
        .filter_map(|r| {
            r.diff.as_ref().map(|d| ColdRecord {
                evm_number: r.evm_number,
                block: r.block,
                value: borsh::to_vec(d).expect("EvmStateDiffV2 is infallibly borsh-serializable"),
            })
        })
        .collect();
    if !diff_records.is_empty() {
        export_range(dir, ColdSegmentKind::StateDiffs, &diff_records, manifest)?;
    }

    Ok(rows.last().expect("non-empty").evm_number + 1)
}
