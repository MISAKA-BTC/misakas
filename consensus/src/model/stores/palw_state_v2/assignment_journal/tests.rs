use super::*;
use kaspa_consensus_core::{
    BlockHash,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
};
use kaspa_database::{create_temp_db, prelude::*};
use misaka_palw_positive_contract::{
    assignment::{consents::*, rows::*, *},
    identity::*,
    messages::AggregateBudget,
    native::ruleset_id,
    rows::*,
};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn bond(n: u8) -> NativeOutpoint {
    NativeOutpoint { transaction: NativeHash64([n; 64]), index: n as u32 }
}
fn bundle() -> AssignmentAdmissionBundleV2 {
    let terms = AssignmentTermsV2 {
        version: 2,
        revision: 1,
        network: d(1),
        ruleset: d(2),
        branch_anchor: NativeHash64([3; 64]),
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        registered_class: d(4),
        poster: bond(5),
        worker: bond(6),
        prover: bond(7),
        payer: PrincipalId(d(8)),
        refund_payload: d(9),
        poster_nonce: 0,
        derivation_policy: d(10),
        source_policy: d(11),
        reserve_schedule: d(12),
        worker_reward: 100,
        max_assignment_fee: 20,
        max_assignment_capital: 30,
        max_assignment_locked: 200,
        accept_before_daa: 100,
        result_before_daa: 200,
        reserve_until_daa: 500,
    };
    let assignment = terms.id();
    AssignmentAdmissionBundleV2 {
        terms,
        job_context: d(13),
        quote: d(14),
        source_catalog: d(15),
        funding: AssignmentFundingAuthorizationV2 {
            assignment,
            quote: d(14),
            payer: terms.payer,
            funding_output_index: 0,
            refund_payload: terms.refund_payload,
            max_fee: 20,
            max_capital: 30,
            max_reward: 100,
            max_locked: 200,
            admission_expiry: 100,
        },
        future_case: FutureCaseFundingBoundsV2 {
            assignment,
            payer: terms.payer,
            refund_payload: terms.refund_payload,
            worker: terms.worker,
            max_case_fee: 40,
            max_case_capital: 60,
            max_case_locked: 100,
            case_admission_before_daa: 200,
            service_before_daa: 450,
        },
        source_consent: d(16),
        prover_consent: d(17),
        cancellation_policy: d(18),
    }
}

fn origin(n: u32) -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([21; 64]),
        carrier_block: NativeHash64([22; 64]),
        carrier_transaction: NativeHash64([n as u8; 64]),
        accepted_tx_ordinal: n,
        event_ordinal: 0,
        accepted_daa: 10,
        accepted_blue_score: 11,
    }
}
fn material(n: u8) -> AcceptedMaterial {
    AcceptedMaterial { origin: origin(1), digest: d(n) }
}
fn assignment() -> AssignmentRow {
    let mut bundle = bundle();
    bundle.terms.ruleset = ruleset_id(d(31), d(32));
    bundle.funding.assignment = bundle.terms.id();
    bundle.future_case.assignment = bundle.terms.id();
    let source = AssignmentSourceConsentV2 {
        version: 2,
        revision: 1,
        assignment: bundle.terms.id(),
        worker: bundle.terms.worker,
        source_catalog: bundle.source_catalog,
        source_policy: bundle.terms.source_policy,
        budget: AggregateBudget {
            rounds: 10,
            unique_source_bytes: 10,
            transmitted_bytes: 20,
            additional_work: 10,
            retained_bytes: 10,
            retention_until: 500,
            fee: 20,
            witness_attempts: 2,
        },
        service_before_daa: 450,
    };
    let prover = AssignmentProverConsentV2 {
        version: 2,
        revision: 1,
        assignment: bundle.terms.id(),
        prover: bundle.terms.prover,
        positive_policy: d(32),
        max_attempts: 2,
        max_build_work: 10,
        max_submitted_bytes: 10,
        max_retained_bytes: 10,
        retention_until_daa: 500,
        build_before_daa: 400,
        max_service_fee: 20,
        cancellation_policy: bundle.cancellation_policy,
    };
    bundle.source_consent = source.id();
    bundle.prover_consent = prover.id();
    AssignmentRow {
        bundle,
        source_consent: source,
        prover_consent: prover,
        origin: origin(1),
        worker_key: [0; PUBKEY_BYTES],
        positive_policy: d(32),
        assignment_policy: d(33),
        fired_rules: d(31),
        job: d(34),
        input: d(35),
        context: bundle.job_context,
        registered_metadata: d(36),
        status: AssignmentStatusV2::Open,
        derivation: material(40),
        dependencies: AcceptedMaterial { origin: origin(1), digest: bundle.terms.empty_dependencies_id() },
        reservation: material(41),
        liability: material(42),
        last_event: origin(1),
        admission_material: AcceptedMaterial { origin: origin(1), digest: d(84) },
    }
}

fn limits() -> AssignmentJournalStoreLimitsV2 {
    AssignmentJournalStoreLimitsV2 {
        max_records: 4,
        max_delta_entries: 32,
        max_record_bytes: 4096,
        max_total_raw_bytes: 1 << 20,
        max_work: 1 << 24,
        max_retained_bytes: 1 << 24,
        max_batch_bytes: 1 << 20,
    }
}
fn row(body: NativeRowBody) -> (Vec<u8>, Vec<u8>) {
    let r = NativeRowValue::new(body).unwrap();
    (r.body.key().encode(), r.encode().unwrap())
}
fn entry(body: NativeRowBody) -> PalwDeltaEntryV2 {
    let (key, value) = row(body);
    PalwDeltaEntryV2::KernelRouteRow { table: KERNEL_AUX_TABLE, key, old: None, new: Some(value) }
}
fn fixture() -> (AssignmentJournalFrameRow, Vec<u8>, PalwStateDeltaV2) {
    let a = assignment();
    let frame = assignment_frame_v2(a.id(), 0, d(0), AssignmentJournalEventKindV2::Open, a.origin, &open_payload(), 4096).unwrap();
    let head = frame.append_to(None, 500).unwrap();
    let raw = encode_assignment_journal_record_v2(frame.header, &open_payload(), 4096).unwrap();
    let delta = PalwStateDeltaV2 {
        point: PalwBlockContextV2 { block: BlockHash::from_u64_word(1), daa_score: 10, blue_score: 11, subsidy: 0 },
        entries: vec![
            entry(NativeRowBody::Assignment(a)),
            entry(NativeRowBody::AssignmentJournalFrame(frame)),
            entry(NativeRowBody::AssignmentJournalHead(head)),
        ],
    };
    (frame, raw, delta)
}
#[test]
fn assignment_material_survives_actual_database_close_reopen_and_raw_tamper_fails() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let path = db.path().to_path_buf();
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (frame, raw, delta) = fixture();
    let mut batch = WriteBatch::default();
    batch.put(b"same-native-root", b"committed");
    let (quote, ticket) = store.stage_positive_assignment_journal_batch_v2(&mut batch, &delta, &[&raw], limits()).unwrap();
    assert_eq!(quote.records, 1);
    assert_eq!(quote.total_batch_bytes as usize, batch.size_in_bytes());
    assert!(store.with_positive_assignment_journal_record_v2(frame.header.id(), 4096, |_| ()).is_err());
    db.write(batch).unwrap();
    drop(ticket);
    drop(store);
    drop(db);
    let db = ConnBuilder::default().with_db_path(path).with_files_limit(10).build().unwrap();
    let mut reopened = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    assert!(reopened.with_positive_assignment_journal_record_v2(frame.header.id(), 4096, |bytes| bytes == raw).unwrap());
    assert_eq!(db.get(b"same-native-root").unwrap().unwrap(), b"committed");
    // Selected state can be rolled back separately; retained bytes have no
    // authority to create an assignment from an empty current executed view.
    let read_limits = AssignmentJournalReadLimitsV2 {
        max_frames: 4,
        max_record_bytes: 4096,
        max_total_raw_bytes: 1 << 20,
        max_work: 1 << 24,
        max_heap_bytes: 1 << 20,
    };
    assert!(matches!(
        reopened
            .visit_positive_assignment_journal_v2(&PalwChainStateV2::genesis(), frame.header.assignment, read_limits, |_, _| Ok(())),
        Err(StoreError::LocalResourceUnavailable(_))
    ));
    let mut corrupt = WriteBatch::default();
    corrupt.put(key(frame.header.id()), b"corrupt");
    db.write(corrupt).unwrap();
    let mut batch = WriteBatch::default();
    batch.put(b"before", b"unchanged");
    let before = batch.data().to_vec();
    assert!(reopened.stage_positive_assignment_journal_batch_v2(&mut batch, &delta, &[&raw], limits()).is_err());
    assert_eq!(before, batch.data());
}
#[test]
fn missing_literal_or_delta_and_capacity_errors_preserve_existing_batch() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db, CachePolicy::Count(4));
    let (_, raw, delta) = fixture();
    let mut missing = delta.clone();
    missing.entries.pop();
    let mut tiny = limits();
    tiny.max_work = 1;
    for (delta, records, limits) in [
        (missing, vec![raw.as_slice()], limits()),
        (delta.clone(), vec![], limits()),
        (delta.clone(), vec![raw.as_slice(), raw.as_slice()], limits()),
        (delta, vec![raw.as_slice()], tiny),
    ] {
        let mut batch = WriteBatch::default();
        batch.put(b"before", b"unchanged");
        let before = batch.data().to_vec();
        assert!(store.stage_positive_assignment_journal_batch_v2(&mut batch, &delta, &records, limits).is_err());
        assert_eq!(before, batch.data());
    }
}

#[test]
fn typed_empty_material_outcome_cannot_publish_new_assignment_delta() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (frame, _raw, delta) = fixture();
    let empty = kaspa_consensus_core::palw_state_v2::PositiveAssignmentMaterialOutcomeV2::default();
    let mut batch = WriteBatch::default();
    batch.put(b"utxo-atomic-material-no-drop", b"pending");
    let before = batch.data().to_vec();
    assert!(store.stage_positive_assignment_material_outcome_v2(&mut batch, &delta, &empty).is_err());
    assert_eq!(batch.data(), before);
    drop(batch);
    assert!(db.get(b"utxo-atomic-material-no-drop").unwrap().is_none());
    assert!(store.with_positive_assignment_journal_record_v2(frame.header.id(), 4096, |_| ()).is_err());
}
#[test]
fn staged_assignment_journal_and_utxo_batch_abort_together_without_write() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(4));
    let (frame, raw, delta) = fixture();
    let mut batch = WriteBatch::default();
    batch.put(b"utxo-atomic-material-abort", b"pending");
    batch.put(b"state-root-atomic-material-abort", b"pending");
    let (_quote, ticket) = store.stage_positive_assignment_journal_batch_v2(&mut batch, &delta, &[&raw], limits()).unwrap();
    // Actual RocksDB staging does not publish journal or sibling native keys.
    assert!(store.with_positive_assignment_journal_record_v2(frame.header.id(), 4096, |_| ()).is_err());
    drop(batch);
    drop(ticket);
    assert!(db.get(b"utxo-atomic-material-abort").unwrap().is_none());
    assert!(db.get(b"state-root-atomic-material-abort").unwrap().is_none());
    assert!(store.with_positive_assignment_journal_record_v2(frame.header.id(), 4096, |_| ()).is_err());
}

fn open_payload() -> Vec<u8> {
    use misaka_palw_positive_contract::assignment::open_material::*;
    encode_assignment_open_v2(&[1], &[2, 0, 1, 0, 8], [&[2], &[3]], AssignmentOpenLimitsV2::within_payload(1024)).unwrap()
}
