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
    case_row_entry(old.map(case), case(new))
}
fn case_row_entry(old: Option<CaseRow>, new: CaseRow) -> PalwDeltaEntryV2 {
    PalwDeltaEntryV2::KernelRouteRow {
        table: KERNEL_AUX_TABLE,
        key: NativeRowKey::Case(new.case).encode(),
        old: old.map(|old| row(NativeRowBody::Case(old))),
        new: Some(row(NativeRowBody::Case(new))),
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
fn assigned_op9_journal_survives_reopen_and_allows_append() {
    use kaspa_consensus_core::{
        palw_positive_court_v2::coordinator::assignment_body::{AssignedResultPayloadV2, PositiveAssignmentNativeOperationV2},
        tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
    };
    use misaka_palw_positive_contract::{
        assignment::AssignmentId,
        funding::{FundingConsent, FundingPlanBasis},
        identity::{FundingScopeId, PrincipalId, SIGNATURE_BYTES},
        messages::MinerResultCommit,
    };
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let path = db.path().to_path_buf();
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let mut policy = limits();
    policy.record_limits.max_record_bytes = 32_768;
    policy.record_limits.max_payload_bytes = 32_768;
    policy.record_limits.max_total_preimage_bytes = 32_768;
    policy.record_limits.max_logical_heap_bytes = 65_536;
    policy.max_total_raw_bytes = 131_072;

    let mut first_case = case(1);
    let body = PositiveAssignmentNativeBodyV2 {
        version: 2,
        revision: 1,
        operation: PositiveAssignmentNativeOperationV2::AdmitAssignedResult {
            assignment: AssignmentId(d(20)),
            payload: AssignedResultPayloadV2 {
                result: MinerResultCommit {
                    version: 2,
                    revision: 1,
                    scope: scope(),
                    job: d(21),
                    execution: d(22),
                    miner: PrincipalId(d(23)),
                    registered_class: d(24),
                    artifact: d(25),
                    program: d(26),
                    input: d(27),
                    tokenizer: d(28),
                    execution_rules: d(29),
                    selector: d(30),
                    generation_seed: [31; 32],
                    temperature_q24: 0,
                    prefill: 1,
                    decode: 1,
                    max_context: 2,
                    output: d(32),
                    dependencies: d(33),
                    service_contract: first_case.service.id(),
                },
                miner_signature: [0; SIGNATURE_BYTES],
                service: first_case.service,
                basis: FundingPlanBasis {
                    profile: CheckProfile::ExactStreamV1,
                    derivation: d(34),
                    mandatory_relations: 1,
                    source_catalog: first_case.service.source_catalog,
                    resource_quote: d(35),
                    policy: d(36),
                    schedule: d(37),
                    required_fee: 1,
                    required_capital: 1,
                },
                consent: FundingConsent {
                    version: 2,
                    revision: 1,
                    case: first_case.case,
                    scope: FundingScopeId(d(38)),
                    payer: PrincipalId(d(39)),
                    max_fees: 1,
                    max_locked: 10,
                    funding_output_index: 0,
                    refund_payload: d(40),
                    admission_expiry: 10,
                },
                payer_signature: [0; SIGNATURE_BYTES],
            },
        },
    };
    let mut payload = vec![1, 0, 176];
    payload.extend(borsh::to_vec(&body).unwrap());
    let tx = Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint::new(BlockHash::from_u64_word(50), 0), vec![1, 2], 0, 1)],
        vec![TransactionOutput::new(10, ScriptPublicKey::from_vec(0, vec![3, 4, 5]))],
        0,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    first_case.last_event.carrier_transaction = NativeHash64(tx.id().as_bytes());
    first_case.last_event.event_ordinal = 0;
    let first = CaseJournalRecord {
        header: CaseJournalHeader {
            version: 2,
            revision: 1,
            scope: scope(),
            case: first_case.case,
            result: first_case.result,
            sequence: 1,
            previous: d(0),
            origin: first_case.last_event,
            kind: CaseJournalEventKind::NativeTransaction,
        },
        payload: borsh::to_vec(&tx).unwrap(),
        aux: vec![],
    };
    let record_head = |record: &CaseJournalRecord| {
        let raw = record.encode(policy.record_limits).unwrap();
        let view = CaseJournalRecordView::decode(&raw, policy.record_limits).unwrap();
        let header = record.header;
        let head = JournalHeadRow {
            case: header.case,
            sequence: header.sequence,
            previous: header.previous,
            head: view.head(),
            history: view.history(),
        };
        (raw, head)
    };
    let mut corrupt = first.clone();
    corrupt.payload.push(0);
    let (corrupt_raw, corrupt_head) = record_head(&corrupt);
    let mut batch = WriteBatch::default();
    assert!(
        store
            .stage_positive_case_journal_batch_v2(
                &mut batch,
                &delta(vec![case_row_entry(None, first_case), head_entry(None, corrupt_head)]),
                &[&corrupt_raw],
                policy,
            )
            .is_err()
    );
    assert_eq!(batch.len(), 0);

    let (first_raw, first_head) = record_head(&first);
    let initial = delta(vec![case_row_entry(None, first_case), head_entry(None, first_head)]);
    let (_, ticket) = store.stage_positive_case_journal_batch_v2(&mut batch, &initial, &[&first_raw], policy).unwrap();
    db.write(batch).unwrap();
    drop(ticket);
    drop(store);
    drop(db);

    let db = ConnBuilder::default().with_db_path(path).with_files_limit(10).build().unwrap();
    let mut reopened = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let next = CaseJournalRecord {
        header: CaseJournalHeader {
            sequence: 2,
            previous: first_head.head,
            origin: origin(2),
            kind: CaseJournalEventKind::NativeBody,
            ..first.header
        },
        payload: vec![2, 0, 1, 0, 5],
        aux: vec![],
    };
    let (next_raw, next_head) = record_head(&next);
    let append = delta(vec![case_row_entry(Some(first_case), case(2)), head_entry(Some(first_head), next_head)]);
    let mut batch = WriteBatch::default();
    let (_, ticket) = reopened.stage_positive_case_journal_batch_v2(&mut batch, &append, &[&next_raw], policy).unwrap();
    db.write(batch).unwrap();
    drop(ticket);
    let chain_limits = CaseJournalChainLimitsV2 { record_limits: policy.record_limits, ..chain_limits() };
    let linked = check_case_journal_links_v2(&reopened, pin(next_head), chain_limits).unwrap();
    let mut records = [first_raw, next_raw].into_iter();
    linked
        .visit_forward(&reopened, |record| {
            assert_eq!(record.raw_bytes(), records.next().unwrap());
            Ok::<_, ()>(())
        })
        .unwrap();
    assert!(records.next().is_none());
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

// These fixtures distinguish cases, results, branches and carrier origins.
// They exercise storage/framing only and create no accepted native Case.
fn distinct_origin(tag: u8, sequence: u64) -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([tag; 64]),
        carrier_block: NativeHash64([tag + 1; 64]),
        carrier_transaction: NativeHash64([tag + sequence as u8; 64]),
        accepted_tx_ordinal: sequence as u32,
        ..origin(sequence)
    }
}
fn distinct_case(tag: u8, sequence: u64) -> CaseRow {
    let mut row = case(sequence);
    row.case = CaseId(d(tag));
    row.result = ResultId(d(tag + 1));
    row.service.case = row.case;
    row.coverage.scope.branch = d(tag + 2);
    row.coverage.result = row.result;
    row.last_event = distinct_origin(tag, sequence);
    row
}
fn distinct_case_entry(tag: u8, old: Option<u64>, new: u64) -> PalwDeltaEntryV2 {
    let next = distinct_case(tag, new);
    PalwDeltaEntryV2::KernelRouteRow {
        table: KERNEL_AUX_TABLE,
        key: NativeRowKey::Case(next.case).encode(),
        old: old.map(|sequence| row(NativeRowBody::Case(distinct_case(tag, sequence)))),
        new: Some(row(NativeRowBody::Case(next))),
    }
}
fn distinct_head_entry(old: Option<JournalHeadRow>, new: JournalHeadRow) -> PalwDeltaEntryV2 {
    PalwDeltaEntryV2::KernelRouteRow {
        table: KERNEL_AUX_TABLE,
        key: NativeRowKey::JournalHead(new.case).encode(),
        old: old.map(|head| row(NativeRowBody::JournalHead(head))),
        new: Some(row(NativeRowBody::JournalHead(new))),
    }
}
fn distinct_fixture(tag: u8, count: u64) -> (Vec<Vec<u8>>, Vec<JournalHeadRow>) {
    let case = distinct_case(tag, 1);
    let mut previous = d(0);
    let mut records = Vec::new();
    let mut heads = Vec::new();
    for sequence in 1..=count {
        let header = CaseJournalHeader {
            version: 2,
            revision: 1,
            scope: case.coverage.scope,
            case: case.case,
            result: case.result,
            sequence,
            previous,
            origin: distinct_origin(tag, sequence),
            kind: CaseJournalEventKind::NativeBody,
        };
        let raw = CaseJournalRecord { header, payload: vec![2, 0, 1, 0, if sequence == 1 { 0 } else { 5 }], aux: vec![] }
            .encode(limits().record_limits)
            .unwrap();
        let view = CaseJournalRecordView::decode(&raw, limits().record_limits).unwrap();
        let head = JournalHeadRow { case: case.case, sequence, previous, head: view.head(), history: view.history() };
        previous = head.head;
        records.push(raw);
        heads.push(head);
    }
    (records, heads)
}
fn distinct_pin(tag: u8, head: JournalHeadRow) -> CaseJournalPinV2 {
    let case = distinct_case(tag, head.sequence);
    CaseJournalPinV2 { scope: case.coverage.scope, case: case.case, result: case.result, head }
}

#[test]
fn multi_case_coalesced_and_intermediate_tail_reopen_forward() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let path = db.path().to_path_buf();
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (a, ah) = distinct_fixture(40, 3);
    let (b, bh) = distinct_fixture(50, 3);
    let (c, ch) = distinct_fixture(60, 1);
    let initial = delta(vec![
        distinct_case_entry(50, None, 1),
        distinct_head_entry(None, ch[0]),
        distinct_case_entry(40, None, 1),
        distinct_head_entry(None, bh[0]),
        distinct_case_entry(60, None, 1),
        distinct_head_entry(None, ah[0]),
    ]);
    let mut seed = WriteBatch::default();
    seed.put([223, 0x49], b"multi-case-parent");
    let (_, seed_ticket) = store.stage_positive_case_journal_batch_v2(&mut seed, &initial, &[&c[0], &b[0], &a[0]], limits()).unwrap();
    let mut options = rocksdb::WriteOptions::default();
    options.set_sync(true);
    db.write_opt(seed, &options).unwrap();
    drop(seed_ticket);

    // A advances through a coalesced endpoint, while B supplies intermediate
    // old/new rows. Delta entries and literal records deliberately interleave.
    let tail = delta(vec![
        distinct_case_entry(40, Some(1), 3),
        distinct_head_entry(Some(bh[0]), bh[1]),
        distinct_case_entry(50, Some(1), 2),
        distinct_head_entry(Some(ah[0]), ah[2]),
        distinct_case_entry(50, Some(2), 3),
        distinct_head_entry(Some(bh[1]), bh[2]),
    ]);
    let mut batch = WriteBatch::default();
    batch.put([223, 0x49], b"multi-case-tail");
    let (quote, ticket) =
        store.stage_positive_case_journal_batch_v2(&mut batch, &tail, &[&b[2], &a[2], &b[1], &a[1]], limits()).unwrap();
    assert_eq!(quote.records, 4);
    assert_eq!(quote.predecessor_record_bytes, (a[0].len() + b[0].len()) as u64);
    assert_eq!(quote.raw_identity_hash_bytes, 2 * (quote.total_raw_bytes + quote.predecessor_record_bytes));
    assert_eq!(quote.total_batch_bytes as usize, batch.size_in_bytes());
    assert!(store.with_positive_journal_record_v2(ah[2].head, 4096, |_| ()).is_err());
    assert!(store.with_positive_journal_record_v2(bh[2].head, 4096, |_| ()).is_err());
    db.write_opt(batch, &options).unwrap();
    drop(ticket);
    drop(store);
    drop(db);

    let db = ConnBuilder::default().with_db_path(path).with_files_limit(10).build().unwrap();
    let reopened = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    assert_eq!(db.get([223, 0x49]).unwrap().unwrap(), b"multi-case-tail");
    for (pin, expected) in [(distinct_pin(40, ah[2]), &a), (distinct_pin(50, bh[2]), &b), (distinct_pin(60, ch[0]), &c)] {
        let checked = check_case_journal_links_v2(&reopened, pin, chain_limits()).unwrap();
        let mut visited = Vec::new();
        checked
            .visit_forward(&reopened, |view| {
                assert_eq!(view.header().case, pin.case);
                assert_eq!(view.header().scope, pin.scope);
                assert_eq!(view.header().result, pin.result);
                visited.push(view.raw_bytes().to_vec());
                Ok::<_, ()>(())
            })
            .unwrap();
        assert_eq!(&visited, expected);
    }
    assert!(reopened.with_positive_journal_record_v2(ch[0].head, 4096, |raw| raw == c[0]).unwrap());
}

#[test]
fn multi_case_missing_or_corrupt_predecessor_rejects_the_whole_batch() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (a, ah) = distinct_fixture(40, 2);
    let (b, bh) = distinct_fixture(50, 2);
    let (c, ch) = distinct_fixture(60, 1);
    let initial = delta(vec![
        distinct_case_entry(40, None, 1),
        distinct_head_entry(None, ah[0]),
        distinct_case_entry(50, None, 1),
        distinct_head_entry(None, bh[0]),
        distinct_case_entry(60, None, 1),
        distinct_head_entry(None, ch[0]),
    ]);
    let mut seed = WriteBatch::default();
    seed.put([223, 0x49], b"multi-case-parent");
    let (_, ticket) = store.stage_positive_case_journal_batch_v2(&mut seed, &initial, &[&a[0], &b[0], &c[0]], limits()).unwrap();
    db.write(seed).unwrap();
    drop(ticket);
    let tail = delta(vec![
        distinct_head_entry(Some(bh[0]), bh[1]),
        distinct_case_entry(40, Some(1), 2),
        distinct_case_entry(50, Some(1), 2),
        distinct_head_entry(Some(ah[0]), ah[1]),
    ]);
    // Missing, truncated, then a valid foreign-case record under B's head key.
    for bad in [None, Some(&b[0][..10]), Some(a[0].as_slice())] {
        let mut damage = WriteBatch::default();
        if let Some(raw) = bad {
            damage.put(journal_key(bh[0].head), raw);
        } else {
            damage.delete(journal_key(bh[0].head));
        }
        db.write(damage).unwrap();
        let mut batch = WriteBatch::default();
        batch.put([223, 0x49], b"uncommitted-tail");
        batch.put(b"caller", b"unchanged");
        let before = batch.data().to_vec();
        assert!(matches!(
            store.stage_positive_case_journal_batch_v2(&mut batch, &tail, &[&b[1], &a[1]], limits()),
            Err(StoreError::DataInconsistency(_))
        ));
        assert_eq!(batch.data(), before);
        let cloned = store.clone();
        let fresh = DbPalwStateV2Store::new(db.clone(), CachePolicy::Empty);
        for reader in [&store, &cloned, &fresh] {
            assert!(matches!(reader.with_positive_journal_record_v2(ah[1].head, 4096, |_| ()), Err(StoreError::KeyNotFound(_))));
            assert!(matches!(reader.with_positive_journal_record_v2(bh[1].head, 4096, |_| ()), Err(StoreError::KeyNotFound(_))));
            assert!(reader.with_positive_journal_record_v2(ah[0].head, 4096, |raw| raw == a[0]).unwrap());
            assert!(reader.with_positive_journal_record_v2(ch[0].head, 4096, |raw| raw == c[0]).unwrap());
        }
        assert_eq!(db.get([223, 0x49]).unwrap().unwrap(), b"multi-case-parent");
        assert!(db.get(b"caller").unwrap().is_none());
    }
}

#[test]
fn multi_case_abandoned_batch_keeps_clone_and_fresh_readers_unpublished() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (a, ah) = distinct_fixture(40, 1);
    let (b, bh) = distinct_fixture(50, 1);
    let initial = delta(vec![
        distinct_case_entry(40, None, 1),
        distinct_head_entry(None, ah[0]),
        distinct_case_entry(50, None, 1),
        distinct_head_entry(None, bh[0]),
    ]);
    let mut abandoned = WriteBatch::default();
    abandoned.put([223, 0x49], b"never-committed");
    let (_, ticket) = store.stage_positive_case_journal_batch_v2(&mut abandoned, &initial, &[&b[0], &a[0]], limits()).unwrap();
    let cloned = store.clone();
    let fresh = DbPalwStateV2Store::new(db.clone(), CachePolicy::Empty);
    let require_unpublished = || {
        for reader in [&store, &cloned, &fresh] {
            for head in [ah[0].head, bh[0].head] {
                assert!(matches!(reader.with_positive_journal_record_v2(head, 4096, |_| ()), Err(StoreError::KeyNotFound(_))));
            }
        }
        assert!(db.get([223, 0x49]).unwrap().is_none());
    };
    require_unpublished();
    drop(abandoned);
    drop(ticket);
    require_unpublished();
}
