//! Actual journal/book IO and admission controls; findings are opaque Output
//! fixtures, not authenticated claim input or signer-duty completion evidence.
use super::super::PalwTirPublicFilerBookV1;
use super::*;
use std::{fs, path::Path, sync::Arc};
fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}
fn entry(n: u64, ids: usize) -> PalwTirPublicPendingFilingV1 {
    PalwTirPublicPendingFilingV1 {
        claim: h(n),
        evidence: PalwTirPublicConvictionEvidenceV1::Output { rows_root: h(20), ids: vec![3; ids] },
        found_daa: 100,
        sends: 0,
        staged_offset: 0,
    }
}
fn wire(entries: &[PalwTirPublicPendingFilingV1]) -> Vec<u8> {
    let mut bytes = PALW_TIR_PUBLIC_FILER_MAGIC_V1.to_vec();
    bytes.extend(borsh::to_vec(entries).unwrap());
    bytes
}
fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("palw-tir-public-filer.v1")
}
#[test]
fn tir_filer_journal_staged_large_roundtrip_has_exact_wire_and_payload_release() {
    let dir = tempfile::tempdir().unwrap();
    let original = entry(1, 400_000);
    let expected = wire(std::slice::from_ref(&original));
    assert!(
        expected.len() > 16 * (90_000 + 1024),
        "legitimate staged fixture exceeds the removed single-control-derived journal ceiling"
    );
    let mut book = PalwTirPublicFilerBookV1::load(dir.path());
    assert!(book.queue_finding_v1(original.claim, original.evidence, 100));
    assert_eq!(fs::read(path(dir.path())).unwrap(), expected, "same magic/count/fields/IDs, no journal wire change");
    drop(book);
    let mut loaded = PalwTirPublicFilerBookV1::load(dir.path());
    assert!(!loaded.loading);
    assert_eq!(loaded.len(), 1);
    let PalwTirPublicConvictionEvidenceV1::Output { ids, .. } = &loaded.filings[&h(1)].evidence else { panic!("Output") };
    assert_eq!(ids.len(), 400_000);
    let payload = Arc::downgrade(&loaded.loaded_filings[0].1);
    assert!(payload.upgrade().is_some());
    loaded.drop_v1(&h(1), "last loaded finding retired");
    assert!(payload.upgrade().is_none(), "deep payload quota releases independently of the still-live book");
    assert!(loaded.loaded_filings.capacity() > 0);
    assert!(loaded.owned_membership.is_some(), "actual retained metadata capacity keeps its own ticket");
}
const TEST_JOURNAL_LOGICAL_BYTES: u64 = 64 << 20;
fn headroom() -> u64 {
    let (mut low, mut high) = (0, TEST_JOURNAL_LOGICAL_BYTES);
    while low < high {
        let middle = low + (high - low + 1) / 2;
        match reserve_native_requests_v1(middle) {
            Ok(ticket) => {
                assert!(ticket.is_some());
                drop(ticket);
                low = middle;
            }
            Err(_) => high = middle - 1,
        }
    }
    low
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tir_filer_journal_quota_load_and_persist_hold_old_bytes_then_real_tick_retries() {
    const CHILD: &str = "MISAKA_TIR_JOURNAL_QUOTA_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let full = format!("{}::tir_filer_journal_quota_load_and_persist_hold_old_bytes_then_real_tick_retries", module_path!());
        let name = full.split_once("::").unwrap().1;
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let bytes = wire(&[entry(1, 400_000)]);
    let file = path(dir.path());
    fs::write(&file, &bytes).unwrap();
    kaspa_consensus_core::palw_native_resource_v1::arm_native_logical_limits_v1(Some(
        kaspa_consensus_core::palw_native_resource_v1::NativeSnapshotResourceLimitsV1 {
            memory_bytes: TEST_JOURNAL_LOGICAL_BYTES,
            database_bytes: 0,
        },
    ));
    let blocker = reserve_native_requests_v1(TEST_JOURNAL_LOGICAL_BYTES).unwrap().unwrap();
    let mut book = PalwTirPublicFilerBookV1::load(dir.path());
    assert!(book.loading);
    assert_eq!(book.len(), 0);
    assert!(!book.queue_finding_v1(h(2), entry(2, 2).evidence, 101));
    book.dirty = true;
    book.persist();
    assert!(matches!(book.advance_journal_v2().await, Err(JournalFaultV2::Resource(_))));
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(!file.with_extension("v1.unreadable").exists());
    drop(blocker);
    book.advance_journal_v2().await.unwrap();
    assert!(!book.loading);
    assert_eq!(book.len(), 1);
    let payload = Arc::downgrade(&book.loaded_filings[0].1);
    let loaded_free = headroom();
    book.drop_v1(&h(1), "release loaded payload");
    assert!(payload.upgrade().is_none());
    let empty_free = headroom();
    assert!(empty_free > loaded_free);
    assert!(empty_free < TEST_JOURNAL_LOGICAL_BYTES, "retained Vec/BTree metadata is independent");
    let blocker = reserve_native_requests_v1(empty_free).unwrap().unwrap();
    assert!(matches!(book.advance_journal_v2().await, Err(JournalFaultV2::Resource(_))));
    assert!(book.dirty);
    assert_eq!(fs::read(&file).unwrap(), bytes);
    drop(blocker);
    book.advance_journal_v2().await.unwrap();
    assert!(!book.dirty);
    assert_eq!(fs::read(&file).unwrap(), wire(&[]));
    drop(book);
    assert_eq!(headroom(), TEST_JOURNAL_LOGICAL_BYTES);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tir_filer_journal_io_loading_and_dirty_write_retry_without_new_finding() {
    let dir = tempfile::tempdir().unwrap();
    let file = path(dir.path());
    fs::create_dir(&file).unwrap();
    let mut book = PalwTirPublicFilerBookV1::load(dir.path());
    assert!(book.loading);
    book.dirty = true;
    book.persist();
    assert!(file.is_dir());
    assert!(matches!(book.advance_journal_v2().await, Err(JournalFaultV2::Io(_))));
    fs::remove_dir(&file).unwrap();
    let old = wire(&[entry(1, 2)]);
    fs::write(&file, &old).unwrap();
    book.advance_journal_v2().await.unwrap();
    assert!(!book.loading);
    assert_eq!(book.len(), 1);
    let temporary = file.with_extension("v1.tmp");
    fs::create_dir(&temporary).unwrap();
    assert!(book.queue_finding_v1(h(2), entry(2, 2).evidence, 101));
    assert!(book.dirty);
    assert_eq!(fs::read(&file).unwrap(), old);
    assert!(matches!(book.advance_journal_v2().await, Err(JournalFaultV2::Io(_))));
    fs::remove_dir(&temporary).unwrap();
    // Same maintenance helper used before actual owner/filer dispatch: no new
    // finding or accepted chain event is needed to retry a dirty journal.
    book.advance_journal_v2().await.unwrap();
    assert!(!book.dirty);
    let reloaded = PalwTirPublicFilerBookV1::load(dir.path());
    assert_eq!(reloaded.len(), 2);
}
#[test]
fn tir_filer_journal_grow_truncate_replace_during_read_are_retry_not_quarantine() {
    for mode in 0..3 {
        let dir = tempfile::tempdir().unwrap();
        let file = path(dir.path());
        let bytes = wire(&[entry(1, 2)]);
        fs::write(&file, &bytes).unwrap();
        let result = read_reserved_with_v2(&file, || match mode {
            0 => {
                use std::io::Write;
                fs::OpenOptions::new().append(true).open(&file).unwrap().write_all(&[0]).unwrap();
            }
            1 => {
                fs::OpenOptions::new().write(true).open(&file).unwrap().set_len(1).unwrap();
            }
            _ => {
                fs::rename(&file, file.with_extension("moved")).unwrap();
                fs::write(&file, &bytes).unwrap();
            }
        });
        assert!(matches!(result, Err(JournalFaultV2::Changed)));
        assert!(file.exists());
        assert!(!file.with_extension("v1.unreadable").exists());
    }
}
#[test]
fn tir_filer_journal_stable_malformed_count_and_duplicate_are_set_aside() {
    let mut count = PALW_TIR_PUBLIC_FILER_MAGIC_V1.to_vec();
    count.extend_from_slice(&17u32.to_le_bytes());
    for bytes in [b"invalid journal".to_vec(), count, wire(&[entry(1, 2), entry(1, 2)])] {
        let dir = tempfile::tempdir().unwrap();
        let file = path(dir.path());
        fs::write(&file, &bytes).unwrap();
        let book = PalwTirPublicFilerBookV1::load(dir.path());
        assert!(!book.loading);
        assert_eq!(book.len(), 0);
        assert!(!file.exists());
        assert_eq!(fs::read(file.with_extension("v1.unreadable")).unwrap(), bytes);
    }
}

#[tokio::test]
async fn tir_filer_journal_cancelled_tick_reuses_actual_read_and_completed_result() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let file = path(dir.path());
    fs::create_dir(&file).unwrap();
    let mut book = PalwTirPublicFilerBookV1::load(dir.path());
    assert!(book.loading);
    fs::remove_dir(&file).unwrap();
    fs::write(&file, wire(&[entry(1, 2)])).unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    book.start_load_worker_v2(move |file| {
        counted.fetch_add(1, Ordering::SeqCst);
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        load_attempt_v2(&file)
    });
    let original_id = book.load_task.as_ref().unwrap().id();
    let book = Arc::new(tokio::sync::Mutex::new(book));
    let awaiting_book = Arc::clone(&book);
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let awaiting = tokio::spawn(async move {
        let mut book = awaiting_book.lock().await;
        entered_tx.send(()).unwrap();
        book.advance_journal_v2().await
    });
    started_rx.await.unwrap();
    entered_rx.await.unwrap();
    awaiting.abort();
    assert!(matches!(awaiting.await, Err(why) if why.is_cancelled()));
    {
        let book = book.lock().await;
        assert!(book.loading);
        assert_eq!(book.load_task.as_ref().unwrap().id(), original_id);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if book.lock().await.load_task.as_ref().unwrap().is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut book = book.lock().await;
    assert_eq!(book.load_task.as_ref().unwrap().id(), original_id);
    book.advance_journal_v2().await.unwrap();
    assert!(!book.loading);
    assert!(book.load_task.is_none());
    assert_eq!(book.len(), 1);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
