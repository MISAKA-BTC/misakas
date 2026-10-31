use super::*;
use kaspa_consensus_core::{
    BlockHash,
    palw_positive_journal_chain_v2::{CaseJournalChainLimitsV2, CaseJournalPinV2, check_case_journal_links_v2},
    palw_state_v2::PalwBlockContextV2,
};
use kaspa_database::{create_temp_db, prelude::*};
use misaka_palw_positive_contract::{
    identity::{ChainScope, PlanId, ResultId},
    journal::CaseJournalRecord,
    messages::{AggregateBudget, CheckProfile, ClaimStatus, CoverageDescriptor, MinerDataStatus, MinerServiceContract, WitnessStatus},
    rows::{AcceptedOrigin, AggregateUsage, NativeHash64},
};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn scope() -> ChainScope {
    ChainScope { network: d(1), ruleset: d(2), branch: d(3) }
}
fn limits() -> PositiveJournalStoreLimitsV2 {
    PositiveJournalStoreLimitsV2 {
        max_records: 16,
        max_delta_entries: 32,
        record_limits: CaseJournalLimits {
            max_record_bytes: 4096,
            max_payload_bytes: 1024,
            max_aux_entries: 4,
            max_aux_preimage_bytes: 1024,
            max_total_preimage_bytes: 2048,
            max_logical_heap_bytes: 4096,
        },
        max_total_raw_bytes: 65_536,
        max_hash_bytes: 1 << 20,
        max_index_bytes: 1 << 20,
        max_batch_bytes: 1 << 20,
        max_retained_bytes: 4 << 20,
    }
}
fn origin(sequence: u64) -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([6; 64]),
        carrier_block: NativeHash64([7; 64]),
        carrier_transaction: NativeHash64([sequence as u8; 64]),
        accepted_tx_ordinal: 0,
        event_ordinal: sequence as u32,
        accepted_daa: sequence,
        accepted_blue_score: sequence,
    }
}
fn case(sequence: u64) -> CaseRow {
    CaseRow {
        case: CaseId(d(4)),
        result: ResultId(d(5)),
        service: MinerServiceContract {
            version: 2,
            revision: 1,
            case: CaseId(d(4)),
            source_catalog: d(10),
            provider_delegations: d(11),
            budget: AggregateBudget {
                rounds: 10,
                unique_source_bytes: 10,
                transmitted_bytes: 10,
                additional_work: 10,
                retained_bytes: 10,
                retention_until: 20,
                fee: 10,
                witness_attempts: 1,
            },
            funding_consent: d(12),
            deadline: 10,
        },
        coverage: CoverageDescriptor {
            scope: scope(),
            result: ResultId(d(5)),
            plan: PlanId(d(13)),
            profile: CheckProfile::ExactStreamV1,
            mandatory_relations: 1,
            reserved_elements: 1,
            reserved_terms: 1,
            max_part_bytes: 10,
        },
        status: ClaimStatus::ResultCommitted,
        miner_data: MinerDataStatus::NotRequested,
        witness_status: WitnessStatus::Building,
        positive_witness: None,
        usage: AggregateUsage::default(),
        reservation: AcceptedMaterial { origin: origin(1), digest: d(14) },
        liability: AcceptedMaterial { origin: origin(1), digest: d(15) },
        maturity_daa: 100,
        unresolved_fraud: 0,
        last_event: origin(sequence),
    }
}
fn row(body: NativeRowBody) -> Vec<u8> {
    NativeRowValue::new(body).unwrap().encode().unwrap()
}
fn case_entry(old: Option<u64>, new: u64) -> PalwDeltaEntryV2 {
    PalwDeltaEntryV2::KernelRouteRow {
        table: KERNEL_AUX_TABLE,
        key: NativeRowKey::Case(CaseId(d(4))).encode(),
        old: old.map(|s| row(NativeRowBody::Case(case(s)))),
        new: Some(row(NativeRowBody::Case(case(new)))),
    }
}
fn head_entry(old: Option<JournalHeadRow>, new: JournalHeadRow) -> PalwDeltaEntryV2 {
    PalwDeltaEntryV2::KernelRouteRow {
        table: KERNEL_AUX_TABLE,
        key: NativeRowKey::JournalHead(CaseId(d(4))).encode(),
        old: old.map(|h| row(NativeRowBody::JournalHead(h))),
        new: Some(row(NativeRowBody::JournalHead(new))),
    }
}
fn delta(entries: Vec<PalwDeltaEntryV2>) -> PalwStateDeltaV2 {
    PalwStateDeltaV2 {
        point: PalwBlockContextV2 { block: BlockHash::from_u64_word(1), daa_score: 1, blue_score: 1, subsidy: 0 },
        entries,
    }
}
fn fixture() -> (Vec<Vec<u8>>, Vec<JournalHeadRow>) {
    let mut records = vec![];
    let mut heads = vec![];
    let mut previous = d(0);
    for sequence in 1..=2 {
        let header = CaseJournalHeader {
            version: 2,
            revision: 1,
            scope: scope(),
            case: CaseId(d(4)),
            result: ResultId(d(5)),
            sequence,
            previous,
            origin: origin(sequence),
            kind: CaseJournalEventKind::NativeBody,
        };
        let raw = CaseJournalRecord { header, payload: vec![2, 0, 1, 0, if sequence == 1 { 0 } else { 5 }], aux: vec![] }
            .encode(limits().record_limits)
            .unwrap();
        let view = CaseJournalRecordView::decode(&raw, limits().record_limits).unwrap();
        let head = JournalHeadRow { case: header.case, sequence, previous, head: view.head(), history: view.history() };
        previous = head.head;
        records.push(raw);
        heads.push(head);
    }
    (records, heads)
}
fn coalesced(head: JournalHeadRow) -> PalwStateDeltaV2 {
    delta(vec![case_entry(None, head.sequence), head_entry(None, head)])
}
fn pin(head: JournalHeadRow) -> CaseJournalPinV2 {
    CaseJournalPinV2 { scope: scope(), case: CaseId(d(4)), result: ResultId(d(5)), head }
}
fn chain_limits() -> CaseJournalChainLimitsV2 {
    CaseJournalChainLimitsV2 {
        record_limits: limits().record_limits,
        max_events: 16,
        max_total_record_bytes: 65_536,
        max_hash_bytes: 131_072,
        max_index_bytes: 1024,
    }
}

#[test]
fn real_db_batch_atomicity_reopen_and_reindex_retain_inactive_raw_records() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let path = db.path().to_path_buf();
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (records, heads) = fixture();
    let refs: Vec<_> = records.iter().map(Vec::as_slice).collect();
    let mut batch = WriteBatch::default();
    // An atomic native-identity marker, not a manufactured accepted Case row.
    batch.put([223, 0x49], b"native-selected-root");
    let (quote, ticket) = store.stage_positive_case_journal_batch_v2(&mut batch, &coalesced(heads[1]), &refs, limits()).unwrap();
    assert_eq!(quote.records, 2);
    assert_eq!(quote.total_batch_bytes as usize, batch.size_in_bytes());
    assert!(store.with_positive_journal_record_v2(heads[1].head, 4096, |_| ()).is_err());
    assert!(db.get([223, 0x49]).unwrap().is_none());
    let mut options = rocksdb::WriteOptions::default();
    options.set_sync(true);
    db.write_opt(batch, &options).unwrap();
    drop(ticket);
    assert_eq!(db.get([223, 0x49]).unwrap().unwrap(), b"native-selected-root");
    check_case_journal_links_v2(&store, pin(heads[1]), chain_limits()).unwrap();
    drop(store);
    drop(db);
    let db = ConnBuilder::default().with_db_path(path).with_files_limit(10).build().unwrap();
    let mut restarted = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let checked = check_case_journal_links_v2(&restarted, pin(heads[1]), chain_limits()).unwrap();
    checked.visit_forward(&restarted, |_| Ok::<_, ()>(())).unwrap();
    // Reverting/removing selected native identity never deletes raw evidence.
    let mut rollback = WriteBatch::default();
    rollback.delete([223, 0x49]);
    db.write(rollback).unwrap();
    assert_eq!(restarted.with_positive_journal_record_v2(heads[1].head, 4096, |raw| raw == records[1]).unwrap(), true);
    restarted.reindex_if_stale().unwrap();
    assert!(db.get([223, 0x49]).unwrap().is_none());
    assert!(restarted.with_positive_journal_record_v2(heads[0].head, 4096, |raw| raw == records[0]).unwrap());
}

#[test]
fn coalesced_and_intermediate_rows_and_existing_old_head_are_supported() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (records, heads) = fixture();
    let intermediate =
        delta(vec![case_entry(None, 1), head_entry(None, heads[0]), case_entry(Some(1), 2), head_entry(Some(heads[0]), heads[1])]);
    let mut batch = WriteBatch::default();
    let (_, ticket) =
        store.stage_positive_case_journal_batch_v2(&mut batch, &intermediate, &[&records[1], &records[0]], limits()).unwrap();
    db.write(batch).unwrap();
    drop(ticket);
    // Exact content append is idempotent even with a restarted native writer.
    let tail = delta(vec![case_entry(Some(1), 2), head_entry(Some(heads[0]), heads[1])]);
    let mut batch = WriteBatch::default();
    let (_, ticket) = store.stage_positive_case_journal_batch_v2(&mut batch, &tail, &[&records[1]], limits()).unwrap();
    db.write(batch).unwrap();
    drop(ticket);
}

#[test]
fn every_stage_error_preserves_existing_caller_batch_bytes() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (records, heads) = fixture();
    let mut wrong_head = heads[1];
    wrong_head.previous = d(99);
    let mut wrong_case = case(2);
    wrong_case.last_event = origin(1);
    let wrong_case_delta = delta(vec![
        PalwDeltaEntryV2::KernelRouteRow {
            table: KERNEL_AUX_TABLE,
            key: NativeRowKey::Case(wrong_case.case).encode(),
            old: None,
            new: Some(row(NativeRowBody::Case(wrong_case))),
        },
        head_entry(None, heads[1]),
    ]);
    let candidates = [
        (coalesced(wrong_head), vec![records[0].as_slice(), records[1].as_slice()]),
        (wrong_case_delta, vec![records[0].as_slice(), records[1].as_slice()]),
        (coalesced(heads[1]), vec![records[1].as_slice()]),
        (coalesced(heads[0]), vec![records[0].as_slice(), records[1].as_slice()]),
        (delta(vec![]), vec![records[0].as_slice()]),
        (coalesced(heads[0]), vec![]),
    ];
    for (delta, records) in candidates {
        let mut batch = WriteBatch::default();
        batch.put(b"caller", b"unchanged");
        let before = batch.data().to_vec();
        assert!(store.stage_positive_case_journal_batch_v2(&mut batch, &delta, &records, limits()).is_err());
        assert_eq!(batch.data(), before);
    }
    let mut corrupt_batch = WriteBatch::default();
    corrupt_batch.put(journal_key(heads[0].head), b"corrupt");
    db.write(corrupt_batch).unwrap();
    let mut batch = WriteBatch::default();
    batch.put(b"caller", b"unchanged");
    let before = batch.data().to_vec();
    assert!(matches!(
        store.stage_positive_case_journal_batch_v2(&mut batch, &coalesced(heads[0]), &[&records[0]], limits()),
        Err(StoreError::DataInconsistency(_))
    ));
    assert_eq!(batch.data(), before);
}

#[test]
fn explicit_capacity_refusal_is_separate_and_never_mutates_batch() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (records, heads) = fixture();
    for policy in [
        PositiveJournalStoreLimitsV2 { max_records: 1, ..limits() },
        PositiveJournalStoreLimitsV2 { max_index_bytes: 1, ..limits() },
        PositiveJournalStoreLimitsV2 { max_hash_bytes: 1, ..limits() },
        PositiveJournalStoreLimitsV2 { max_retained_bytes: 1, ..limits() },
        PositiveJournalStoreLimitsV2 { max_batch_bytes: 1, ..limits() },
    ] {
        let mut batch = WriteBatch::default();
        batch.put(b"caller", b"unchanged");
        let before = batch.data().to_vec();
        assert!(matches!(
            store.stage_positive_case_journal_batch_v2(&mut batch, &coalesced(heads[1]), &[&records[0], &records[1]], policy),
            Err(StoreError::LocalResourceUnavailable(_))
        ));
        assert_eq!(batch.data(), before);
    }
}

#[test]
fn raw_pinned_reader_is_bounded_but_chain_owns_corruption_and_identity_checks() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (records, heads) = fixture();
    assert!(matches!(store.with_positive_journal_record_v2(heads[0].head, 4096, |_| ()), Err(StoreError::KeyNotFound(_))));
    let mut batch = WriteBatch::default();
    batch.put(journal_key(heads[0].head), &records[1]);
    db.write(batch).unwrap();
    let mut called = false;
    assert!(matches!(
        store.with_positive_journal_record_v2(heads[0].head, 1, |_| called = true),
        Err(StoreError::LocalResourceUnavailable(_))
    ));
    assert!(!called);
    assert!(check_case_journal_links_v2(&store, pin(heads[0]), chain_limits()).is_err());
    let mut batch = WriteBatch::default();
    batch.put(journal_key(heads[0].head), &records[0][..10]);
    db.write(batch).unwrap();
    assert!(check_case_journal_links_v2(&store, pin(heads[0]), chain_limits()).is_err());
}

#[test]
fn append_requires_literal_selected_predecessor_and_keeps_batch_on_failure() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (records, heads) = fixture();
    let tail = delta(vec![case_entry(Some(1), 2), head_entry(Some(heads[0]), heads[1])]);
    let mut batch = WriteBatch::default();
    batch.put(b"caller", b"unchanged");
    let before = batch.data().to_vec();
    assert!(matches!(
        store.stage_positive_case_journal_batch_v2(&mut batch, &tail, &[&records[1]], limits()),
        Err(StoreError::DataInconsistency(_))
    ));
    assert_eq!(batch.data(), before);
    // The native old-head DTO cannot nominate a different literal predecessor.
    let mut corruption = WriteBatch::default();
    corruption.put(journal_key(heads[0].head), &records[1]);
    db.write(corruption).unwrap();
    assert!(matches!(
        store.stage_positive_case_journal_batch_v2(&mut batch, &tail, &[&records[1]], limits()),
        Err(StoreError::DataInconsistency(_))
    ));
    assert_eq!(batch.data(), before);
}
