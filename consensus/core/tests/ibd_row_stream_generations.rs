//! G14 storage, IBD lane — end-to-end: a PALW row stream verified incrementally against the committed root, spilled
//! into a capped spill file, whole-state checked, and only then promoted into a new storage generation through the
//! atomic switch journal. Scaled-down caps: the logic, not the bytes, is tested.
//!
//! Design: docs/design/palw/storage/ibd-generations.md. RFC-0037 / ADR-0204 (Proposed).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_row_stream_v1::*;
use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, palw_collection_root_of_entries_v1, palw_state_root_of_preimage_v1};
use kaspa_database::storage_generations::{
    CrashPoint, GenerationError, GenerationStore, VerifiedMarker,
    reserve::{FixedBudgetReserver, StorageReserver, UnboundedReserver},
    spill::{SpillArea, SpillFile},
};
use std::io::Read;
use std::sync::Arc;

type Rows = Vec<(Vec<u8>, Vec<u8>)>;

/// A synthetic committed state in the exact flat form (ADR-0043 §2 shape): version, then each collection's root,
/// with scalar bytes between them.
struct Committed {
    preimage: Vec<u8>,
    root: Hash64,
    collections: Vec<(Vec<u8>, Rows, usize)>,
}

fn rows(label: &str, n: usize, width: usize) -> Rows {
    (0..n).map(|i| (format!("{label}-{i:06}").into_bytes(), vec![(i % 251) as u8; width])).collect()
}

fn committed(spec: &[(&str, usize, usize)]) -> Committed {
    let mut preimage = 5u16.to_le_bytes().to_vec();
    let mut collections = Vec::new();
    for (i, (label, n, width)) in spec.iter().enumerate() {
        preimage.extend_from_slice(&[i as u8; 3]); // a scalar block between roots
        let r = rows(label, *n, *width);
        let root = palw_collection_root_of_entries_v1(label.as_bytes(), r.len(), r.iter().cloned());
        let offset = preimage.len();
        preimage.extend_from_slice(root.as_byte_slice());
        collections.push((label.as_bytes().to_vec(), r, offset));
    }
    let root = palw_state_root_of_preimage_v1(&preimage);
    Committed { preimage, root, collections }
}

fn three() -> Committed {
    committed(&[("bonds", 40, 100), ("classes", 25, 60), ("claims", 70, 80)])
}

/// Stream every collection of `c` into the verifier, applying `tamper` to the (collection, row) it names.
fn stream(
    c: &Committed,
    sink: SpillFile,
    budget: u64,
    tamper: Option<(usize, usize)>,
) -> Result<PalwRowStreamVerifiedV1<SpillFile>, (PalwRowStreamErrorV1, usize)> {
    let mut v = PalwRowStreamVerifierV1::new(c.root, c.preimage.clone(), sink, budget).map_err(|e| (e, 0))?;
    for (ci, (label, rows, offset)) in c.collections.iter().enumerate() {
        v.begin_collection(label, rows.len() as u64, *offset).map_err(|e| (e, ci))?;
        for (ri, (k, val)) in rows.iter().enumerate() {
            let mut val = val.clone();
            if tamper == Some((ci, ri)) {
                val[0] ^= 1;
            }
            v.row(k, &val).map_err(|e| (e, ci))?;
        }
        v.end_collection().map_err(|e| (e, ci))?;
    }
    v.finish().map_err(|e| (e, usize::MAX))
}

/// The whole-state step used by these tests: read the spill back and check a global invariant across collections
/// (every claim names a class index that exists) — standing in for `restore_native_carriage_v1`'s full root
/// recomputation and `assert_internal_consistency*`, which need a real typed state.
fn whole_state(spill: &mut SpillFile, preimage: &[u8], root: Hash64, require: impl Fn(&[(Vec<u8>, Rows)]) -> bool) -> Result<(), String> {
    if palw_state_root_of_preimage_v1(preimage) != root {
        return Err("preimage".into());
    }
    let mut r = spill.reader().map_err(|e| e.to_string())?;
    let collections = palw_read_row_spill_v1(&mut r).map_err(|e| e.to_string())?;
    if require(&collections) { Ok(()) } else { Err("global invariant broken".into()) }
}

fn marker_of<W>(e: &PalwWholeStateVerifiedV1<W>) -> VerifiedMarker {
    VerifiedMarker { subject: "palw-pruning-point test".into(), root_hex: e.root().to_string(), whole_state_checked: true }
}

fn setup(cap: u64, budget: u64) -> (tempfile::TempDir, GenerationStore, SpillArea, Arc<FixedBudgetReserver>) {
    let dir = tempfile::tempdir().unwrap();
    let reserver = Arc::new(FixedBudgetReserver::new(budget));
    let r: Arc<dyn StorageReserver> = reserver.clone();
    let (mut store, _) = GenerationStore::open(dir.path().join("generations"), r.clone()).unwrap();
    // A verified base generation is already active (the node's current state).
    let base = store.begin_staging(0).unwrap();
    std::fs::write(base.path.join("state"), b"base").unwrap();
    store.mark_verified(&base, &VerifiedMarker { subject: "base".into(), root_hex: "00".into(), whole_state_checked: true }).unwrap();
    store.switch_to(base).unwrap();
    let spill = SpillArea::open(dir.path().join("spill"), cap, 2, r).unwrap();
    (dir, store, spill, reserver)
}

#[test]
fn the_streamed_collection_root_is_byte_identical_to_the_committed_one() {
    for (label, n, w) in [("bonds", 0usize, 0usize), ("classes", 1, 1), ("claims", 300, 17), ("x", 5, 4096)] {
        let r = rows(label, n, w);
        let mut s = PalwCollectionRootStreamV1::new(label.as_bytes(), r.len() as u64);
        for (k, v) in &r {
            s.row(k, v).unwrap();
        }
        assert_eq!(s.finish().unwrap(), palw_collection_root_of_entries_v1(label.as_bytes(), r.len(), r.iter().cloned()));
    }
    // Declared count is bound both ways.
    let mut s = PalwCollectionRootStreamV1::new(b"bonds", 1);
    s.row(b"k", b"v").unwrap();
    assert!(matches!(s.clone().row(b"k2", b"v"), Err(PalwRowStreamErrorV1::TooManyRows { .. })));
    assert!(matches!(PalwCollectionRootStreamV1::new(b"bonds", 2).finish(), Err(PalwRowStreamErrorV1::RowCountMismatch { .. })));
}

/// The real PALW genesis state: its own preimage verifies, and its (empty) bond/class/claim tables stream against it.
#[test]
fn the_real_genesis_preimage_and_its_tables_verify() {
    let state = PalwChainStateV2::genesis();
    let preimage = state.state_root_preimage();
    let mut v = PalwRowStreamVerifierV1::new(state.state_root(), preimage.clone(), Vec::<u8>::new(), 1 << 20).unwrap();
    for label in [&b"bonds"[..], b"classes", b"claims"] {
        let root = palw_collection_root_of_entries_v1(label, 0, std::iter::empty());
        let offset = preimage.windows(64).position(|w| w == root.as_byte_slice()).expect("committed");
        v.begin_collection(label, 0, offset).unwrap();
        v.end_collection().unwrap();
    }
    let done = v.finish().unwrap();
    assert_eq!(done.collections().len(), 3);
    // Another state's root refuses the same preimage before any row.
    assert!(matches!(
        PalwRowStreamVerifierV1::new(Hash64::from_bytes([7; 64]), preimage, Vec::<u8>::new(), 1 << 20),
        Err(PalwRowStreamErrorV1::PreimageDoesNotMatchRoot(_))
    ));
}

/// Happy path: stream → spill (peak within the scaled cap) → whole-state check → marker → atomic switch.
#[test]
fn a_verified_stream_is_promoted_through_the_switch_journal_within_the_spill_cap() {
    let c = three();
    let cap = 32 << 10; // scaled-down 384 MiB
    let (dir, mut store, area, reserver) = setup(cap, cap * 2 + 4096);
    let staging = store.begin_staging(4096).unwrap();
    let verified = stream(&c, area.acquire().unwrap(), cap, None).unwrap();
    assert_eq!(verified.collections().iter().map(|(_, n)| *n).sum::<u64>(), 135);
    let mut evidence = verified
        .whole_state_checked(|s, p, r| whole_state(s, p, r, |cols| cols.len() == 3 && cols[2].1.len() == 70))
        .expect("whole-state check passes");
    // Install from the verified spill into the staging generation, then mark and switch.
    let mut bytes = Vec::new();
    evidence.sink().reader().unwrap().read_to_end(&mut bytes).unwrap();
    std::fs::write(staging.path.join("state"), &bytes).unwrap();
    store.mark_verified(&staging, &marker_of(&evidence)).unwrap();
    let active = store.switch_to(staging).unwrap();
    assert_eq!(std::fs::read(active.path.join("state")).unwrap(), bytes);
    drop(evidence);
    let acc = area.accounting();
    assert!(acc.high_water > 0 && acc.high_water <= area.max_bytes(), "peak spill {} within {}", acc.high_water, area.max_bytes());
    assert_eq!(acc.bytes, 0, "spill released after install");
    assert!(reserver.high_water() <= reserver.budget());
    assert_eq!(std::fs::read_dir(dir.path().join("spill")).unwrap().count(), 0);
}

/// A forged row in the 2nd of 3 collections: refused at the END OF THAT COLLECTION (the 3rd is never read), the
/// verifier is poisoned, the spill is released, and the staging generation is never marked or switched in.
#[test]
fn a_mismatch_mid_stream_aborts_at_that_collection_and_promotes_nothing() {
    let c = three();
    let cap = 32 << 10;
    let (dir, mut store, area, _) = setup(cap, cap * 2 + 4096);
    let active_before = store.active().unwrap();
    let staging = store.begin_staging(4096).unwrap();
    let staging_no = staging.number;
    let err = stream(&c, area.acquire().unwrap(), cap, Some((1, 7))).err().expect("tampered stream must fail");
    assert_eq!(err, (PalwRowStreamErrorV1::CollectionMismatch(b"classes".to_vec()), 1));
    assert_eq!(area.accounting().bytes, 0, "the spill is dropped with the failed verifier");
    assert!(!store.is_verified(staging_no));
    assert!(matches!(store.switch_to(staging), Err(GenerationError::NotVerified(_))));
    // Even without an explicit abandon, a restart deletes the never-verified staging generation.
    drop(store);
    let (store, report) = GenerationStore::open(dir.path().join("generations"), Arc::new(UnboundedReserver)).unwrap();
    assert_eq!(report.active, Some(active_before.number));
    assert!(report.removed.contains(&staging_no));
    assert_eq!(std::fs::read(store.active().unwrap().path.join("state")).unwrap(), b"base");
}

/// The poisoned verifier refuses everything after the first failure, including a row that would have been honest.
#[test]
fn a_failed_stream_stays_failed() {
    let c = three();
    let mut v = PalwRowStreamVerifierV1::new(c.root, c.preimage.clone(), Vec::<u8>::new(), 1 << 20).unwrap();
    let (label, rows, offset) = &c.collections[0];
    v.begin_collection(label, rows.len() as u64, *offset).unwrap();
    assert!(matches!(v.begin_collection(label, 1, *offset), Err(PalwRowStreamErrorV1::CollectionAlreadyOpen)));
    assert_eq!(v.row(&rows[0].0, &rows[0].1), Err(PalwRowStreamErrorV1::Poisoned));
    assert!(v.finish().is_err());
    // Offsets: out of range, overlapping, and a duplicate label are refused before any row.
    let mut v = PalwRowStreamVerifierV1::new(c.root, c.preimage.clone(), Vec::<u8>::new(), 1 << 20).unwrap();
    assert!(matches!(v.begin_collection(b"bonds", 0, c.preimage.len() - 10), Err(PalwRowStreamErrorV1::BadOffset { .. })));
    let mut v = PalwRowStreamVerifierV1::new(c.root, c.preimage.clone(), Vec::<u8>::new(), 1 << 20).unwrap();
    v.begin_collection(b"bonds", 40, c.collections[0].2).unwrap();
    for (k, val) in &c.collections[0].1 {
        v.row(k, val).unwrap();
    }
    v.end_collection().unwrap();
    assert!(matches!(v.begin_collection(b"classes", 25, c.collections[0].2 + 10), Err(PalwRowStreamErrorV1::BadOffset { .. })));
    let mut v = PalwRowStreamVerifierV1::new(c.root, c.preimage.clone(), Vec::<u8>::new(), 1 << 20).unwrap();
    v.begin_collection(b"bonds", 0, c.collections[1].2).unwrap();
    // An honest collection presented at another collection's slot does not verify.
    assert!(matches!(v.end_collection(), Err(PalwRowStreamErrorV1::CollectionMismatch(_))));
}

/// The stream exceeds the scaled-down spill cap: a local, clean abort (sink error), nothing promoted, nothing left.
#[test]
fn a_stream_larger_than_the_spill_cap_aborts_cleanly() {
    let c = committed(&[("bonds", 200, 200)]); // ≈ 42 KiB of rows
    let cap = 16 << 10;
    let (dir, mut store, area, _) = setup(cap, cap * 2 + 4096);
    let staging = store.begin_staging(4096).unwrap();
    let err = stream(&c, area.acquire().unwrap(), u64::MAX, None).err().unwrap();
    assert!(matches!(err.0, PalwRowStreamErrorV1::Sink(_)), "{err:?}");
    assert!(area.accounting().high_water <= cap, "never past one file's cap");
    // The verifier's own budget refuses first when it is the tighter one.
    let err = stream(&c, area.acquire().unwrap(), 8 << 10, None).err().unwrap();
    assert!(matches!(err.0, PalwRowStreamErrorV1::ByteBudget(_)));
    store.abandon_staging(staging).unwrap();
    assert_eq!(store.generations_on_disk().unwrap().len(), 1);
    assert_eq!(std::fs::read_dir(dir.path().join("spill")).unwrap().count(), 0);
}

/// Every collection matches the committed root, but the whole-state check refuses the assembled state (the global
/// consistency checks `into_state` runs today): no evidence, no marker, no switch.
#[test]
fn a_root_matching_stream_that_fails_the_whole_state_check_is_not_verified() {
    let c = three();
    let cap = 32 << 10;
    let (_dir, mut store, area, _) = setup(cap, cap * 2 + 4096);
    let staging = store.begin_staging(4096).unwrap();
    let verified = stream(&c, area.acquire().unwrap(), cap, None).unwrap();
    let refused = verified.whole_state_checked(|s, p, r| whole_state(s, p, r, |_| false));
    assert!(matches!(refused, Err(PalwRowStreamErrorV1::WholeState(_))));
    // And the generation store will not take a root-only marker in its place.
    let root_only = VerifiedMarker { subject: "root only".into(), root_hex: c.root.to_string(), whole_state_checked: false };
    assert!(matches!(store.mark_verified(&staging, &root_only), Err(GenerationError::RootAloneIsNotVerified(_))));
    assert!(matches!(store.switch_to(staging), Err(GenerationError::NotVerified(_))));
}

/// Crash after the marker but before the switch (and before / after the journal rename): restart keeps the old
/// verified generation unless the rename happened, in which case the new verified one is active.
#[test]
fn a_crash_between_verification_and_switch_recovers_to_a_verified_generation() {
    for (point, switched) in [(CrashPoint::VerifyAfterMarker, false), (CrashPoint::SwitchAfterJournalTmp, false), (CrashPoint::SwitchAfterJournal, true)]
    {
        let c = three();
        let cap = 32 << 10;
        let (dir, mut store, area, _) = setup(cap, cap * 2 + 4096);
        let old = store.active().unwrap().number;
        let staging = store.begin_staging(4096).unwrap();
        let new = staging.number;
        let mut evidence = stream(&c, area.acquire().unwrap(), cap, None).unwrap().whole_state_checked(|_, _, _| Ok(())).unwrap();
        let mut bytes = Vec::new();
        evidence.sink().reader().unwrap().read_to_end(&mut bytes).unwrap();
        std::fs::write(staging.path.join("state"), &bytes).unwrap();
        store.arm_crash_point(point);
        let r = store.mark_verified(&staging, &marker_of(&evidence)).and_then(|_| store.switch_to(staging).map(|_| ()));
        assert!(matches!(r, Err(GenerationError::SimulatedCrash(p)) if p == point));
        drop((store, evidence, area));
        let (store, report) = GenerationStore::open(dir.path().join("generations"), Arc::new(UnboundedReserver)).unwrap();
        let active = report.active.unwrap();
        assert!(store.is_verified(active));
        assert_eq!(active, if switched { new } else { old }, "{point:?}");
    }
}

/// A collection rooted over a projection of its records (the `gen_classes` / `tir_classes` shape): the full record
/// travels and is spilled, the root is over the projection, and a record that does not decode is refused.
#[test]
fn a_projected_collection_verifies_over_its_rooted_bytes_and_keeps_the_full_record() {
    // Full record = rooted bytes ‖ 0xEE ‖ a tail the root does not cover directly.
    let rooted = rows("gen_classes", 12, 30);
    let full: Rows = rooted.iter().map(|(k, v)| (k.clone(), [v.as_slice(), &[0xEEu8][..], &b"tail"[..]].concat())).collect();
    let mut preimage = vec![1, 2, 3];
    let offset = preimage.len();
    preimage.extend_from_slice(palw_collection_root_of_entries_v1(b"gen_classes", 12, rooted.iter().cloned()).as_byte_slice());
    let root = palw_state_root_of_preimage_v1(&preimage);
    let project = || -> PalwRowProjectionV1 {
        Box::new(|full: &[u8]| -> Result<Vec<u8>, String> {
            let at = full.iter().rposition(|b| *b == 0xEE).ok_or("no projection marker")?;
            Ok(full[..at].to_vec())
        })
    };
    let mut v = PalwRowStreamVerifierV1::new(root, preimage.clone(), Vec::<u8>::new(), 1 << 20).unwrap();
    v.begin_projected_collection(b"gen_classes", 12, offset, project()).unwrap();
    for (k, val) in &full {
        v.row(k, val).unwrap();
    }
    v.end_collection().unwrap();
    let spill = v.finish().unwrap().into_sink();
    let back = palw_read_row_spill_v1(spill.as_slice()).unwrap();
    assert_eq!(back[0].1, full, "the spill keeps the full records for assembly");
    // Hashing the full record as if it were the rooted form does not verify.
    let mut v = PalwRowStreamVerifierV1::new(root, preimage.clone(), Vec::<u8>::new(), 1 << 20).unwrap();
    v.begin_collection(b"gen_classes", 12, offset).unwrap();
    for (k, val) in &full {
        v.row(k, val).unwrap();
    }
    assert!(matches!(v.end_collection(), Err(PalwRowStreamErrorV1::CollectionMismatch(_))));
    // An undecodable record is refused at that row.
    let mut v = PalwRowStreamVerifierV1::new(root, preimage, Vec::<u8>::new(), 1 << 20).unwrap();
    v.begin_projected_collection(b"gen_classes", 12, offset, project()).unwrap();
    assert!(matches!(v.row(b"k", b"no marker"), Err(PalwRowStreamErrorV1::Projection(..))));
}
