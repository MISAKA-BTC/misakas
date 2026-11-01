//! Clean DB close/reopen -> actual shared-journal reader -> fresh real Court
//! and Source caches. Class/job/roles/rates remain trusted LOCAL fixture data.
//! This is not a Node-process restart, IBD, native admission/root authority,
//! physical-lease/RSS validation, qualified payment or a native reorg success.
use super::*;
use kaspa_consensus::model::stores::palw_state_v2::{DbPalwStateV2Store, positive_journal::PositiveJournalStoreLimitsV2};
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwDeltaEntryV2, PalwStateDeltaV2};
use kaspa_database::{
    prelude::{CachePolicy, ConnBuilder, DB},
    registry::DatabaseStorePrefixes,
};
use misaka_palw_positive_contract::{
    journal::CASE_JOURNAL_HEADER_BYTES,
    native::{KERNEL_AUX_TABLE, NativeRowKey},
    rows::{CaseRow, NativeRowBody, NativeRowValue, ServiceReceiptRow, SourceUnionRow},
};
use rocksdb::WriteBatch;
use std::{path::Path, sync::Arc};

type FactIdentity = (ChainScope, CaseId, ResultId, WitnessId, PlanId, TerminalKindV2, CoverageDescriptor);
#[derive(Debug, PartialEq, Eq)]
struct Projection {
    case: CaseRow,
    usage: AggregateUsage,
    historical: LocalCourtSnapshotV2,
    current: LocalCourtSnapshotV2,
    observed: u64,
    receipts: Vec<ServiceReceiptRow>,
    unions: Vec<SourceUnionRow>,
    dues: Vec<kaspa_consensus_core::palw_positive_source_v2::SourceFeeIntentV2>,
    facts: Vec<FactIdentity>,
}
fn projection(replay: &LocalReplayedCaseV2<CommonSourceReplayStateV2>, fixture: &SourceFixture) -> Projection {
    let state = replay.external();
    let mut receipts = vec![];
    let mut unions = vec![];
    let mut dues = vec![];
    for request in &fixture.requests {
        if let Some(union) = state.source_union(request.kind, request.source).unwrap() {
            unions.push(union);
        }
        for unit in 0..request.end {
            if let Some(receipt) = state.receipt(request.kind, request.source, unit).unwrap() {
                receipts.push(receipt);
            }
            if let Some(due) = state.fee_intent(request.kind, request.source, unit).unwrap() {
                dues.push(due);
            }
        }
    }
    Projection {
        case: state.case_row().unwrap(),
        usage: state.usage().unwrap(),
        historical: replay.historical_court(),
        current: replay.current_court(),
        observed: replay.observed_daa(),
        receipts,
        unions,
        dues,
        facts: replay
            .facts()
            .iter()
            .map(|fact| {
                (
                    fact.scope(),
                    fact.case(),
                    fact.result(),
                    fact.witness(),
                    fact.plan(),
                    fact.terminal_kind(),
                    fact.terminal().descriptor(),
                )
            })
            .collect(),
    }
}
fn memory_baseline(fixture: &SourceFixture, journal: &Journal) -> Projection {
    let env = SourceEnvironment::new(fixture, journal);
    let replay = replay_common_case_v2(journal, &journal.links(), &env, replay_limits()).unwrap();
    let expected = projection(&replay, fixture);
    assert_eq!(env.base.live_commits.get(), 0);
    // Return small expected DTOs only. The baseline Court/source cache and its
    // historical environment are dropped before the database is reopened.
    expected
}
fn open(path: &Path, create: bool) -> Arc<DB> {
    ConnBuilder::default().with_db_path(path.to_path_buf()).with_create_if_missing(create).with_files_limit(10).build().unwrap()
}
fn clean_close(store: DbPalwStateV2Store, db: Arc<DB>) {
    drop(store);
    assert_eq!(Arc::strong_count(&db), 1, "all store/cache DB owners must be gone before reopen");
    let weak = Arc::downgrade(&db);
    drop(db);
    assert!(weak.upgrade().is_none(), "this is a real handle close, not a second handle over the same store");
}
fn store_limits() -> PositiveJournalStoreLimitsV2 {
    PositiveJournalStoreLimitsV2 {
        max_records: 128,
        max_delta_entries: 8,
        record_limits: journal_limits(),
        max_total_raw_bytes: 64 << 20,
        max_hash_bytes: 128 << 20,
        max_index_bytes: 4 << 20,
        max_batch_bytes: 16 << 20,
        max_retained_bytes: 128 << 20,
    }
}
fn row(body: NativeRowBody) -> Vec<u8> {
    NativeRowValue::new(body).unwrap().encode().unwrap()
}
fn caller_delta(case: CaseRow, pin: CaseJournalPinV2, old: Option<(CaseRow, CaseJournalPinV2)>) -> PalwStateDeltaV2 {
    let origin = pin.head.history.origin;
    PalwStateDeltaV2 {
        point: PalwBlockContextV2 {
            block: Hash64::from_bytes(origin.accepting_block.0),
            daa_score: origin.accepted_daa,
            blue_score: origin.accepted_blue_score,
            subsidy: 0,
        },
        entries: vec![
            PalwDeltaEntryV2::KernelRouteRow {
                table: KERNEL_AUX_TABLE,
                key: NativeRowKey::Case(case.case).encode(),
                old: old.map(|(case, _)| row(NativeRowBody::Case(case))),
                new: Some(row(NativeRowBody::Case(case))),
            },
            PalwDeltaEntryV2::KernelRouteRow {
                table: KERNEL_AUX_TABLE,
                key: NativeRowKey::JournalHead(case.case).encode(),
                old: old.map(|(_, pin)| row(NativeRowBody::JournalHead(pin.head))),
                new: Some(row(NativeRowBody::JournalHead(pin.head))),
            },
        ],
    }
}
fn persist(
    store: &mut DbPalwStateV2Store,
    db: &Arc<DB>,
    journal: &Journal,
    case: CaseRow,
    old: Option<(CaseRow, CaseJournalPinV2)>,
) -> (Hash64, Hash64) {
    let delta = caller_delta(case, journal.pin, old);
    // An explicit test caller DATA identifier, never an authenticated native
    // current state root or a manufactured admission/capital/lease capability.
    let caller_data_root = Hash64::from_u64_word(90_000 + journal.pin.head.sequence);
    let mut batch = WriteBatch::default();
    let delta_ticket = store.insert_delta_batch_tracked_v1(&mut batch, delta.point.block, caller_data_root, &delta).unwrap();
    let prior_bytes = batch.size_in_bytes();
    let start = old.map_or(0, |(_, pin)| pin.head.sequence as usize);
    let records: Vec<_> = journal.records[start..].iter().map(Vec::as_slice).collect();
    let (quote, raw_ticket) = store.stage_positive_case_journal_batch_v2(&mut batch, &delta, &records, store_limits()).unwrap();
    assert_eq!(quote.total_batch_bytes as usize, batch.size_in_bytes());
    assert!(quote.total_batch_bytes as usize > prior_bytes);
    assert!(store.delta_of_retained_v1(delta.point.block).is_err());
    assert!(store.with_positive_journal_record_v2(journal.pin.head.head, journal_limits().max_record_bytes, |_| ()).is_err());
    let mut options = rocksdb::WriteOptions::default();
    options.set_sync(true);
    let written = db.write_opt(batch, &options);
    // BOTH typed-delta and raw-journal tickets cover the actual write result.
    drop(raw_ticket);
    drop(delta_ticket);
    written.unwrap();
    (delta.point.block, caller_data_root)
}
fn data_pin(delta: &PalwStateDeltaV2) -> (CaseRow, CaseJournalPinV2) {
    let mut case = None;
    let mut head = None;
    for entry in &delta.entries {
        if let PalwDeltaEntryV2::KernelRouteRow { table, key, new: Some(bytes), .. } = entry {
            assert_eq!(*table, KERNEL_AUX_TABLE);
            match NativeRowValue::decode(key, bytes).unwrap().body {
                NativeRowBody::Case(row) => {
                    assert!(case.replace(row).is_none());
                }
                NativeRowBody::JournalHead(row) => {
                    assert!(head.replace(row).is_none());
                }
                _ => panic!("unexpected fixture delta row"),
            }
        }
    }
    let case = case.unwrap();
    let head = head.unwrap();
    assert_eq!(head.case, case.case);
    assert_eq!(head.history.origin, case.last_event);
    (case, CaseJournalPinV2 { scope: case.coverage.scope, case: case.case, result: case.result, head })
}

#[test]
fn clean_db_reopen_replays_the_same_actual_court_source_and_once_only_due_from_persisted_delta_pin() {
    let fixture = SourceFixture::new();
    let journal = fixture.journal();
    let expected = memory_baseline(&fixture, &journal);
    let directory = tempfile::tempdir().unwrap();
    let db = open(directory.path(), true);
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (point, expected_data_root) = persist(&mut store, &db, &journal, expected.case, None);
    clean_close(store, db);

    let db = open(directory.path(), false);
    let store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (data_root, retained_delta) = store.delta_of_retained_v1(point).unwrap();
    assert_eq!(data_root, expected_data_root);
    let (saved_case, pin) = data_pin(&retained_delta);
    assert_eq!(saved_case, expected.case);
    // The input pin is reconstructed from the reopened typed delta, never
    // substituted by journal.pin or a head selected from retained raw bytes.
    for _ in 0..2 {
        let links = check_case_journal_links_v2(&store, pin, chain_limits()).unwrap();
        let env = SourceEnvironment::new(&fixture, &journal);
        let replay = replay_common_case_v2(&store, &links, &env, replay_limits()).unwrap();
        let actual = projection(&replay, &fixture);
        assert_eq!(actual, expected);
        assert_eq!(actual.usage.fee, 15);
        assert_eq!(actual.receipts.len(), 5);
        assert_eq!(actual.dues.len(), 5);
        assert_eq!(actual.unions.len(), 2);
        assert_eq!(env.base.live_commits.get(), 0);
    }
    // Keep the retained typed owner for the entire read/replay interval.
    drop(retained_delta);
    clean_close(store, db);
}

/// Existing production key, used ONLY to delete/corrupt persisted evidence in
/// adversarial tests. All successful raw writes go through the real stage API.
fn corruptible_raw_key(head: Digest) -> [u8; 66] {
    let mut key = [0; 66];
    key[0] = DatabaseStorePrefixes::PalwStateV2Tip.into();
    key[1] = 0x4a;
    key[2..].copy_from_slice(&head.0);
    key
}
#[test]
fn reopened_missing_middle_or_tail_and_corrupted_tail_never_fall_back_to_fixture_records() {
    let fixture = SourceFixture::new();
    let journal = fixture.journal();
    let expected = memory_baseline(&fixture, &journal);
    for damage in 0..3 {
        let directory = tempfile::tempdir().unwrap();
        let db = open(directory.path(), true);
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
        let (point, _) = persist(&mut store, &db, &journal, expected.case, None);
        clean_close(store, db);

        let db = open(directory.path(), false);
        let store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
        let (_, retained_delta) = store.delta_of_retained_v1(point).unwrap();
        let (saved_case, pin) = data_pin(&retained_delta);
        assert_eq!(saved_case, expected.case);
        let links = check_case_journal_links_v2(&store, pin, chain_limits()).unwrap();
        let victim = if damage == 0 { journal.records.len() / 2 } else { journal.records.len() - 1 };
        let raw = &journal.records[victim];
        let head = CaseJournalRecordView::decode(raw, journal_limits()).unwrap().head();
        let key = corruptible_raw_key(head);
        if damage < 2 {
            db.delete(key).unwrap();
        } else {
            let mut changed = raw.clone();
            changed[CASE_JOURNAL_HEADER_BYTES + 4 + 5] ^= 1; // Shape-valid payload mutation, wrong pinned identity.
            CaseJournalRecordView::decode(&changed, journal_limits()).unwrap();
            db.put(key, changed).unwrap();
        }
        assert!(journal.store.contains_key(&head), "the fixture still owns the original bytes; using them as fallback is forbidden");
        assert!(check_case_journal_links_v2(&store, pin, chain_limits()).is_err());
        // An older checked-link token also cannot publish a partial replay
        // after its underlying DB source is damaged. Readonly prefix checks
        // may happen before the later missing record is reached.
        let env = SourceEnvironment::new(&fixture, &journal);
        assert!(replay_common_case_v2(&store, &links, &env, replay_limits()).is_err());
        assert_eq!(env.base.external_state_creations.get(), 0);
        assert_eq!(env.base.external_calls.get(), 0);
        assert_eq!(env.base.final_calls.get(), 0);
        assert_eq!(env.base.live_commits.get(), 0);
        assert_eq!(env.base.active.get(), None);
        drop(retained_delta);
        clean_close(store, db);
    }
}

#[test]
fn an_older_persisted_caller_pin_replays_only_its_prefix_while_future_raw_tail_is_retained() {
    let fixture = SourceFixture::new();
    let full = fixture.journal();
    let prefix = fixture.scenario.journal(&fixture.bodies[..4]); // First source duty, no response yet.
    let expected_prefix = memory_baseline(&fixture, &prefix);
    let expected_full = memory_baseline(&fixture, &full);
    let directory = tempfile::tempdir().unwrap();
    let db = open(directory.path(), true);
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (prefix_point, _) = persist(&mut store, &db, &prefix, expected_prefix.case, None);
    let (full_point, _) = persist(&mut store, &db, &full, expected_full.case, Some((expected_prefix.case, prefix.pin)));
    clean_close(store, db);

    let db = open(directory.path(), false);
    let store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (_, prefix_delta) = store.delta_of_retained_v1(prefix_point).unwrap();
    let (_, full_delta) = store.delta_of_retained_v1(full_point).unwrap();
    let (_, old_pin) = data_pin(&prefix_delta);
    let (_, full_pin) = data_pin(&full_delta);
    assert!(old_pin.head.sequence < full_pin.head.sequence);
    store.with_positive_journal_record_v2(full_pin.head.head, journal_limits().max_record_bytes, |_| ()).unwrap();
    {
        let links = check_case_journal_links_v2(&store, old_pin, chain_limits()).unwrap();
        let env = SourceEnvironment::new(&fixture, &prefix);
        let replay = replay_common_case_v2(&store, &links, &env, replay_limits()).unwrap();
        let actual = projection(&replay, &fixture);
        assert_eq!(actual, expected_prefix);
        assert_eq!(replay.quote().events, old_pin.head.sequence);
        assert_eq!(actual.current.status, ClaimStatus::Pending);
        assert_eq!(actual.case.miner_data, MinerDataStatus::Pending);
        assert_eq!(actual.usage.rounds, 1);
        assert_eq!(actual.usage.unique_source_bytes, 4);
        assert_eq!(actual.usage.fee, 0);
        assert!(actual.facts.is_empty() && actual.receipts.is_empty() && actual.dues.is_empty());
        assert_eq!(actual.unions.len(), 1);
        assert_eq!((actual.unions[0].requested_units, actual.unions[0].completed_units), (4, 0));
        // Historical caller DATA pins confer no current native Case/head rollback
        // authority. The native recovery factory remains independently unavailable.
    } // All replay/environment/link borrows end before closing the DB owners.
    drop(prefix_delta);
    drop(full_delta);
    clean_close(store, db);
}
