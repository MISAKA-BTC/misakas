use super::*;
use crate::prelude::ConnBuilder;
fn limits() -> RecoveryStoreLimits {
    RecoveryStoreLimits {
        max_archive_bytes: 1 << 20,
        max_related_rows: 8,
        max_key_bytes: 256,
        max_value_bytes: 4096,
        max_transaction_bytes: 4 << 20,
    }
}
fn db(path: &std::path::Path) -> Arc<DB> {
    ConnBuilder::default().with_db_path(path.to_owned()).with_files_limit(32).build().unwrap()
}
fn store(db: Arc<DB>) -> PositiveRecoveryStore {
    PositiveRecoveryStore::new(db, b"a6-recovery/".to_vec(), limits()).unwrap()
}
fn commit(n: u8) -> RecoveryCommit {
    RecoveryCommit {
        case: [1; 64],
        binding: RecoveryBinding { branch: [2; 64], ruleset: [3; 64], result: [4; 64] },
        expected_head: if n == 0 { None } else { Some(RecoveryHead { records: n as u64 - 1, digest: [n - 1; 64] }) },
        head: RecoveryHead { records: n as u64, digest: [n; 64] },
        transaction: [n; 64],
        observed_daa: n as u64 + 10,
        archive: vec![n; 37],
        related: vec![RelatedRow {
            key: b"native-row".to_vec(),
            expected: if n == 0 { None } else { Some(vec![n - 1]) },
            value: Some(vec![n]),
        }],
    }
}
#[test]
fn durable_head_archive_and_related_rows_rollback_together_across_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("db");
    {
        let s = store(db(&path));
        assert_eq!(s.commit(commit(0)).unwrap(), CommitEffect::Applied);
        assert_eq!(s.commit(commit(0)).unwrap(), CommitEffect::AlreadyApplied);
        assert_eq!(s.commit(commit(1)).unwrap(), CommitEffect::Applied);
    }
    {
        let s = store(db(&path));
        let loaded = s.load(&[1; 64]).unwrap().unwrap();
        assert_eq!(loaded.head, commit(1).head);
        assert_eq!(loaded.archive, vec![1; 37]);
        assert_eq!(s.related_value(b"native-row").unwrap(), Some(vec![1]));
        assert_eq!(s.rollback(&[1; 64], &[1; 64]).unwrap(), RollbackEffect::RolledBack);
        assert_eq!(s.rollback(&[1; 64], &[1; 64]).unwrap(), RollbackEffect::AlreadyRolledBack);
        assert_eq!(s.load(&[1; 64]).unwrap().unwrap().head, commit(0).head);
        assert_eq!(s.related_value(b"native-row").unwrap(), Some(vec![0]));
        assert_eq!(s.commit(commit(1)).unwrap(), CommitEffect::Applied);
        assert_eq!(s.related_value(b"native-row").unwrap(), Some(vec![1]));
        assert_eq!(s.rollback(&[1; 64], &[1; 64]).unwrap(), RollbackEffect::RolledBack);
        assert_eq!(s.rollback(&[1; 64], &[0; 64]).unwrap(), RollbackEffect::RolledBack);
    }
    let s = store(db(&path));
    assert!(s.load(&[1; 64]).unwrap().is_none());
    assert_eq!(s.related_value(b"native-row").unwrap(), None);
}
#[test]
fn rejection_and_failed_ack_do_not_split_native_rows_from_history() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("db");
    let s = store(db(&path));
    assert!(matches!(s.commit_with_fault(commit(0), CommitFault::BeforeWrite), Err(RecoveryStoreError::SimulatedFailure)));
    assert!(s.load(&[1; 64]).unwrap().is_none());
    assert_eq!(s.related_value(b"native-row").unwrap(), None);
    assert!(matches!(s.commit_with_fault(commit(0), CommitFault::AfterWrite), Err(RecoveryStoreError::SimulatedFailure)));
    assert_eq!(s.commit(commit(0)).unwrap(), CommitEffect::AlreadyApplied);
    assert_eq!(s.related_value(b"native-row").unwrap(), Some(vec![0]));
    let mut bad = commit(1);
    bad.related[0].expected = Some(vec![9]);
    assert!(matches!(s.commit(bad), Err(RecoveryStoreError::Conflict)));
    assert_eq!(s.load(&[1; 64]).unwrap().unwrap().head, commit(0).head);
    let mut bad = commit(0);
    bad.related[0].value = Some(vec![9]);
    assert!(matches!(s.commit(bad), Err(RecoveryStoreError::Conflict)));
}
#[test]
fn concurrent_separate_namespaces_cannot_both_spend_same_before_image() {
    let tmp = tempfile::tempdir().unwrap();
    let db = db(&tmp.path().join("db"));
    let a = store(db.clone());
    let b = PositiveRecoveryStore::new(db, b"another-recovery/".to_vec(), limits()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let b1 = barrier.clone();
    let b2 = barrier.clone();
    let first = std::thread::spawn(move || {
        b1.wait();
        a.commit(commit(0))
    });
    let second = std::thread::spawn(move || {
        let mut c = commit(0);
        c.transaction = [88; 64];
        c.head.digest = [88; 64];
        c.archive = vec![88];
        c.related[0].value = Some(vec![88]);
        b2.wait();
        b.commit(c)
    });
    barrier.wait();
    let results = [first.join().unwrap(), second.join().unwrap()];
    assert_eq!(results.iter().filter(|r| matches!(r, Ok(CommitEffect::Applied))).count(), 1);
    assert_eq!(results.iter().filter(|r| matches!(r, Err(RecoveryStoreError::Conflict))).count(), 1);
}
#[test]
fn unrelated_case_mutation_prevents_destructive_rollback() {
    let tmp = tempfile::tempdir().unwrap();
    let s = store(db(&tmp.path().join("db")));
    s.commit(commit(0)).unwrap();
    let mut other = commit(0);
    other.case = [11; 64];
    other.transaction = [12; 64];
    other.related[0].expected = Some(vec![0]);
    other.related[0].value = Some(vec![12]);
    s.commit(other).unwrap();
    assert!(matches!(s.rollback(&[1; 64], &[0; 64]), Err(RecoveryStoreError::Conflict)));
    assert_eq!(s.related_value(b"native-row").unwrap(), Some(vec![12]));
    s.rollback(&[11; 64], &[12; 64]).unwrap();
    s.rollback(&[1; 64], &[0; 64]).unwrap();
    assert_eq!(s.related_value(b"native-row").unwrap(), None);
}
#[test]
fn all_write_bytes_and_codec_lengths_are_bounded_before_commit_or_allocation() {
    let tmp = tempfile::tempdir().unwrap();
    let d = db(&tmp.path().join("db"));
    let s = store(d.clone());
    let mut duplicate = commit(0);
    duplicate.related.push(duplicate.related[0].clone());
    assert!(matches!(s.commit(duplicate), Err(RecoveryStoreError::Conflict)));
    let mut reserved = commit(0);
    reserved.related[0].key = s.namespace.clone();
    reserved.related[0].key.extend_from_slice(b"override");
    assert!(matches!(s.commit(reserved), Err(RecoveryStoreError::Conflict)));
    let small =
        PositiveRecoveryStore::new(d, b"a6-recovery/".to_vec(), RecoveryStoreLimits { max_transaction_bytes: 500, ..limits() })
            .unwrap();
    assert!(matches!(small.commit(commit(0)), Err(RecoveryStoreError::Budget)));
    assert!(s.load(&[1; 64]).unwrap().is_none());
    let encoded = encode_state(&StoredRecovery {
        binding: commit(0).binding,
        head: commit(0).head,
        transaction: [0; 64],
        observed_daa: 10,
        archive: vec![0],
    })
    .unwrap();
    for len in 0..encoded.len() {
        assert!(decode_state(&encoded[..len], limits()).is_err());
    }
    let mut bad = encoded.clone();
    let length_at = 5 + 5 * 64 + 16;
    bad[length_at..length_at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(matches!(decode_state(&bad, limits()), Err(RecoveryStoreError::Budget)));
    let mut suffix = encoded;
    suffix.push(0);
    assert!(decode_state(&suffix, limits()).is_err());
}
// The subprocess exits WITHOUT dropping the DB. sync-WAL durability must be
// sufficient to reopen exactly the old or complete new transaction.
#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("MISAKA_A6_DB_CRASH_PATH") else { return };
    let mode = std::env::var("MISAKA_A6_DB_CRASH_MODE").unwrap();
    let s = store(db(std::path::Path::new(&path)));
    match mode.as_str() {
        "before" => {
            assert!(s.commit_with_fault(commit(0), CommitFault::BeforeWrite).is_err());
        }
        "after" => {
            s.commit(commit(0)).unwrap();
        }
        "rollback" => {
            s.rollback(&[1; 64], &[0; 64]).unwrap();
        }
        _ => panic!("unknown crash mode"),
    }
    std::process::exit(91);
}
#[test]
fn real_process_exit_reopens_atomic_before_after_and_undo_images() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("db");
    for mode in ["before", "after", "rollback"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "palw_positive_recovery::tests::crash_child", "--nocapture"])
            .env("MISAKA_A6_DB_CRASH_PATH", &path)
            .env("MISAKA_A6_DB_CRASH_MODE", mode)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(91), "{:?}", String::from_utf8_lossy(&output.stderr));
        let s = store(db(&path));
        if mode == "after" {
            assert_eq!(s.load(&[1; 64]).unwrap().unwrap().head, commit(0).head);
            assert_eq!(s.related_value(b"native-row").unwrap(), Some(vec![0]));
        } else {
            assert!(s.load(&[1; 64]).unwrap().is_none());
            assert_eq!(s.related_value(b"native-row").unwrap(), None);
        }
    }
}

#[test]
fn clock_only_expiry_projection_is_atomic_and_undo_restores_the_clock() {
    let tmp = tempfile::tempdir().unwrap();
    let s = store(db(&tmp.path().join("db")));
    s.commit(commit(0)).unwrap();
    let mut clock = commit(0);
    clock.expected_head = Some(clock.head);
    clock.transaction = [90; 64];
    clock.observed_daa = 11;
    clock.related[0].expected = Some(vec![0]);
    clock.related[0].value = Some(vec![90]);
    assert_eq!(s.commit(clock.clone()).unwrap(), CommitEffect::Applied);
    assert_eq!(s.load(&[1; 64]).unwrap().unwrap().observed_daa, 11);
    let mut stale = clock.clone();
    stale.transaction = [91; 64];
    stale.related[0].expected = Some(vec![90]);
    assert!(matches!(s.commit(stale), Err(RecoveryStoreError::Conflict)));
    s.rollback(&[1; 64], &[90; 64]).unwrap();
    assert_eq!(s.load(&[1; 64]).unwrap().unwrap().observed_daa, 10);
    assert_eq!(s.related_value(b"native-row").unwrap(), Some(vec![0]));
    assert_eq!(s.commit(clock).unwrap(), CommitEffect::Applied);
}

#[test]
fn bounded_before_image_purge_keeps_current_archive_and_other_case_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let s = store(db(&tmp.path().join("db")));
    s.commit(commit(0)).unwrap();
    s.commit(commit(1)).unwrap();
    s.commit(commit(2)).unwrap();
    s.rollback(&[1; 64], &[2; 64]).unwrap();
    let mut other = commit(0);
    other.case = [33; 64];
    other.related[0].key = b"other-native".to_vec();
    s.commit(other).unwrap();
    let before = s.load(&[1; 64]).unwrap().unwrap();
    assert!(matches!(s.purge_before_images(&[1; 64], commit(0).head, 11, 8, 1 << 20), Err(RecoveryStoreError::Conflict)));
    assert!(matches!(s.purge_before_images(&[1; 64], before.head, 10, 8, 1 << 20), Err(RecoveryStoreError::Conflict)));
    for (rows, bytes) in [(0, 1024), (1, 0), (1, 1)] {
        assert!(matches!(s.purge_before_images(&[1; 64], before.head, 11, rows, bytes), Err(RecoveryStoreError::Budget)));
    }
    let first = s.purge_before_images(&[1; 64], before.head, 11, 1, 1 << 20).unwrap();
    assert_eq!(first.deleted_records, 1);
    assert!(!first.complete);
    assert!(first.deleted_bytes > 0);
    assert_eq!(s.load(&[1; 64]).unwrap().unwrap(), before);
    assert_eq!(s.related_value(b"native-row").unwrap(), Some(vec![1]));
    let done = s.purge_before_images(&[1; 64], before.head, 11, 8, 1 << 20).unwrap();
    assert_eq!(done.deleted_records, 2);
    assert!(done.complete);
    assert!(s.rollback(&[1; 64], &[1; 64]).is_err());
    assert!(s.rollback(&[1; 64], &[2; 64]).is_err());
    assert_eq!(s.load(&[1; 64]).unwrap().unwrap(), before);
    assert_eq!(s.purge_before_images(&[1; 64], before.head, 11, 8, 1 << 20).unwrap().deleted_records, 0);
    s.rollback(&[33; 64], &[0; 64]).unwrap();
    assert_eq!(s.related_value(b"other-native").unwrap(), None);
}

#[test]
fn length_delimited_namespaces_keep_foreign_zero_case_state_out_of_purge() {
    let tmp = tempfile::tempdir().unwrap();
    let db = db(&tmp.path().join("db"));
    let a = PositiveRecoveryStore::new(db.clone(), b"a".to_vec(), limits()).unwrap();
    let b = PositiveRecoveryStore::new(db.clone(), b"a\x01".to_vec(), limits()).unwrap();
    let mut ca = commit(0);
    ca.case = [0; 64];
    ca.related[0].key = b"native-a".to_vec();
    let mut cb = commit(0);
    cb.case = [0; 64];
    cb.transaction = [7; 64];
    cb.head.digest = [7; 64];
    cb.archive = vec![7; 37];
    cb.related[0].key = b"native-b".to_vec();
    a.commit(ca.clone()).unwrap();
    b.commit(cb.clone()).unwrap();
    let before_b = b.load(&[0; 64]).unwrap().unwrap();
    let purge = a.purge_before_images(&[0; 64], ca.head, ca.observed_daa, 8, 1 << 20).unwrap();
    assert_eq!(purge.deleted_records, 1);
    assert!(purge.complete);
    assert_eq!(a.load(&[0; 64]).unwrap().unwrap().head, ca.head);
    assert_eq!(b.load(&[0; 64]).unwrap().unwrap(), before_b);
    assert_eq!(b.related_value(b"native-b").unwrap(), Some(vec![0]));
    assert_eq!(b.rollback(&[0; 64], &[7; 64]).unwrap(), RollbackEffect::RolledBack);
    assert_eq!(b.related_value(b"native-b").unwrap(), None);
    // Noncanonical short keys under the private prefix are corruption, not
    // another transaction which a broad prefix scan may silently delete.
    db.put(a.key(1, &[0; 64], None), b"noncanonical").unwrap();
    assert!(matches!(a.purge_before_images(&[0; 64], ca.head, ca.observed_daa, 8, 1 << 20), Err(RecoveryStoreError::Malformed)));
    assert_eq!(a.load(&[0; 64]).unwrap().unwrap().head, ca.head);
}
