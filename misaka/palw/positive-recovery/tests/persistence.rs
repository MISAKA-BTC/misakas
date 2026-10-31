mod fixture;
use fixture::{TinyExecution, tiny_execution};
use kaspa_consensus_core::{
    Hash64,
    mldsa87_primitives::{MLDSA87_PUBKEY_LEN, mldsa87_key_id},
    palw_positive_court_v2::{
        AdmissionScopeV2, ClaimStatusV2, ErrorV2, ExactPolicyV2, MinerResultV2, PanelWitnessV2, RESULT_CONTEXT, SignatureV2,
        WITNESS_CONTEXT,
        coordinator::{
            CoordinatorLimitsV2, PanelRosterV2, PositiveCoordinatorV2, SOURCE_REQUEST_CONTEXT,
            recovery::{
                EventOriginV2, JournaledCoordinatorV2, RecoveryAdapterV2, RecoveryAdmissionV2, RecoveryContextV2, RecoveryEventV2,
                RecoveryHeadV2, RecoveryLimitsV2, RetentionV2, SavedPartV2,
            },
        },
        gate::ClaimFormV2,
        recovery_sync::{
            CheckedRecoveryImportV2, RecoveryChunkImportV2, RecoveryImportScopeV2, RecoveryRetentionV2, RecoveryTransferLimitsV2,
        },
        service::{CHUNK_BYTES, ServicePolicyV2, WorkRequestV2, chunk_hash_v2},
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_step_leg::{step_merkle_root_v1, step_opening_v1},
    palw_tir_court_v1::{PalwTirCourtRulesV1, build_tir_cone_refutation_v1, tir_logits_event_disclosure_v1},
    palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepSpaceV1},
};
use kaspa_database::{
    palw_positive_recovery::{PositiveRecoveryStore, RecoveryStoreLimits, RelatedRow, RollbackEffect},
    prelude::ConnBuilder,
};
use misaka_palw_positive_recovery::{
    PersistenceErrorV2, commit_initial, commit_prepared, prune_retired, publish_import, restore_and_commit, restore_selected,
};
use std::path::Path;

fn hash(x: u64) -> Hash64 {
    Hash64::from_u64_word(x)
}
fn raw(h: Hash64) -> [u8; 64] {
    *h.as_byte_slice()
}
fn caps() -> RecoveryLimitsV2 {
    RecoveryLimitsV2 { max_records: 128, max_encoded_bytes: 128 << 20, max_decoded_heap: 128 << 20 }
}
fn sign(seed: u8, id: Hash64, domain: &[u8]) -> SignatureV2 {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]);
    SignatureV2(
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, id.as_byte_slice(), domain, [seed.wrapping_add(1); 32])
            .unwrap()
            .as_ref()
            .to_vec(),
    )
}
struct Scenario {
    x: TinyExecution,
    result: MinerResultV2,
    witness: PanelWitnessV2,
    policy: ExactPolicyV2,
    service: ServicePolicyV2,
    bytes: Vec<u8>,
}
impl Scenario {
    fn new() -> Self {
        let x = tiny_execution(None);
        let policy = ExactPolicyV2 {
            max_leaves: 1 << 20,
            max_part_bytes: 1 << 20,
            max_part_heap: 8 << 20,
            max_total_bytes: 64 << 20,
            max_elements_per_leaf: 1_000_000,
            max_terms_per_leaf: 1_000_000,
            max_total_elements: 1 << 40,
            max_total_terms: 1 << 40,
            max_commit_bytes: 1 << 20,
            deadline: 100,
        };
        let bytes = vec![19; CHUNK_BYTES];
        let service = ServicePolicyV2 {
            version: 2,
            case: hash(104),
            source: step_merkle_root_v1(&[chunk_hash_v2(&hash(104), 0, &bytes)]).unwrap(),
            chunks: 1,
            max_rounds: 1,
            max_unique_bytes: CHUNK_BYTES as u64,
            max_transmitted_bytes: CHUNK_BYTES as u64 * 8,
            work_per_chunk: 1,
            max_work: 5,
            retention_until: 120,
            deadline: 100,
            price_per_chunk: 10,
            hold: 20,
        };
        let result = MinerResultV2 {
            version: 2,
            network: hash(101),
            branch: hash(102),
            ruleset: hash(103),
            claim: hash(104),
            job: x.binding.job_context.clone(),
            artifact: x.binding.artifact_root,
            output: kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&x.binding.job_context, &x.generated),
            sampling: kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
            service_policy: service.id(),
            dependencies: hash(105),
        };
        let witness = PanelWitnessV2 {
            version: 2,
            result: result.id(),
            policy: policy.id(),
            attempt: 0,
            binding: x.binding.clone(),
            generated: x.generated.clone(),
        };
        Self { x, result, witness, policy, service, bytes }
    }
    fn fresh(&self) -> PositiveCoordinatorV2 {
        let miner = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([31; 32]);
        let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([32; 32]);
        PositiveCoordinatorV2::admit(
            AdmissionScopeV2 {
                network: self.result.network,
                branch: self.result.branch,
                ruleset: self.result.ruleset,
                registered_class_id: self.result.job.shape_profile_id,
                required_form: ClaimFormV2::MinerResultV2,
            },
            self.result.clone(),
            sign(31, self.result.id(), RESULT_CONTEXT),
            miner.verification_key.as_ref().to_vec(),
            self.x.binding.class.clone(),
            self.policy,
            PalwPromptIdsFormV1::Flat,
            PanelRosterV2::new(vec![panel.verification_key.as_ref().to_vec()], MLDSA87_PUBKEY_LEN as u64).unwrap(),
            self.service.clone(),
            CoordinatorLimitsV2 {
                max_attempts: 2,
                max_reserved_elements: 1 << 45,
                max_reserved_terms: 1 << 45,
                max_reserved_witness_bytes: 256 << 20,
                max_history_bytes: 1 << 20,
                max_batch_parts: 1 << 20,
                max_batch_bytes: 64 << 20,
            },
            1,
        )
        .unwrap()
    }
    fn journal(&self) -> JournaledCoordinatorV2 {
        JournaledCoordinatorV2::new(self.fresh(), 1, caps()).unwrap()
    }
    fn panel(&self) -> Hash64 {
        mldsa87_key_id(libcrux_ml_dsa::ml_dsa_87::generate_key_pair([32; 32]).verification_key.as_ref())
    }
    fn request(&self, j: &JournaledCoordinatorV2) -> RecoveryEventV2 {
        let id = j.state().unwrap().request_id(WorkRequestV2::Source { first: 0, count: 1 }).unwrap();
        RecoveryEventV2::Request { panel: self.panel(), first: 0, count: 1, signature: sign(32, id, SOURCE_REQUEST_CONTEXT).0 }
    }
    fn response(&self) -> RecoveryEventV2 {
        let leaf = chunk_hash_v2(&self.result.claim, 0, &self.bytes);
        RecoveryEventV2::Response { index: 0, bytes: self.bytes.clone(), opening: step_opening_v1(&[leaf], 0).unwrap() }
    }
    fn candidate(&self) -> RecoveryEventV2 {
        RecoveryEventV2::Candidate {
            panel: self.panel(),
            witness: self.witness.clone(),
            signature: sign(32, self.witness.id(), WITNESS_CONTEXT).0,
        }
    }
    fn batch(&self) -> RecoveryEventV2 {
        let space = PalwTirStepSpaceV1::new(&self.x.binding.class).unwrap();
        let rules = PalwTirCourtRulesV1 {
            max_step_leaf_count: self.policy.max_leaves,
            prompt_form: PalwPromptIdsFormV1::Flat,
            limits: misaka_palw_tir::demand::DemandLimits {
                max_elements: self.policy.max_elements_per_leaf,
                max_terms: self.policy.max_terms_per_leaf,
            },
        };
        let parts = (0..self.x.binding.step_leaf_count)
            .map(|i| {
                let cone = build_tir_cone_refutation_v1(&self.x.binding, i, &self.x, &rules).unwrap();
                let leaf = space.leaf_at(&self.x.binding.job_context, i).unwrap();
                let trace = match leaf.kind {
                    PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. }
                        if occurrence as usize == space.occurrences().len() - 1 && node == space.program.logits =>
                    {
                        let row = leaf.position + 1 - self.x.binding.job_context.declared_prefill_tokens;
                        Some(
                            borsh::to_vec(
                                &tir_logits_event_disclosure_v1(
                                    &self.x.binding,
                                    &self.x.rows,
                                    &self.x.generated,
                                    row,
                                    (first_element / 4096) as u8,
                                )
                                .unwrap(),
                            )
                            .unwrap(),
                        )
                    }
                    _ => None,
                };
                SavedPartV2 { cone: borsh::to_vec(&cone).unwrap(), trace }
            })
            .collect();
        RecoveryEventV2::Batch { witness: self.witness.id(), first: 0, parts }
    }
}
fn origin(daa: u64) -> EventOriginV2 {
    EventOriginV2 { carrier: hash(900), daa }
}
fn auth(o: EventOriginV2, _: &RecoveryEventV2) -> bool {
    o.carrier == hash(900)
}
fn store(path: &Path) -> PositiveRecoveryStore {
    let db = ConnBuilder::default().with_db_path(path.to_owned()).with_files_limit(32).build().unwrap();
    PositiveRecoveryStore::new(
        db,
        b"a6-production-test/".to_vec(),
        RecoveryStoreLimits {
            max_archive_bytes: 128 << 20,
            max_related_rows: 8,
            max_key_bytes: 128,
            max_value_bytes: 4096,
            max_transaction_bytes: 512 << 20,
        },
    )
    .unwrap()
}
// These are dedicated test rows recording the actual checked service and
// resource state. They are not a substitute for a native coin funding engine.
fn project(db: &PositiveRecoveryStore, s: &PositiveCoordinatorV2) -> Result<Vec<RelatedRow>, PersistenceErrorV2> {
    let m = s.service_meter();
    let a = s.attempts();
    let rows = [
        (b"test-native/service-fees".as_slice(), borsh::to_vec(&(m.paid, m.refunded, m.remaining)).unwrap()),
        (b"test-native/service-budget".as_slice(), borsh::to_vec(&(m.rounds, m.unique_bytes, m.transmitted_bytes, m.work)).unwrap()),
        (
            b"test-native/resource-hold".as_slice(),
            borsh::to_vec(&(a.attempts, a.reserved_elements, a.reserved_terms, a.reserved_witness_bytes, a.received_witness_bytes))
                .unwrap(),
        ),
    ];
    rows.into_iter()
        .map(|(key, value)| Ok(RelatedRow { key: key.to_vec(), expected: db.related_value(key)?, value: Some(value) }))
        .collect()
}
fn fees(db: &PositiveRecoveryStore) -> (u64, u64, u64) {
    borsh::from_slice(&db.related_value(b"test-native/service-fees").unwrap().unwrap()).unwrap()
}
fn initial(db: &PositiveRecoveryStore, f: &Scenario) -> JournaledCoordinatorV2 {
    commit_initial(db, f.journal(), 1, |daa| daa == 1, |s| project(db, s)).unwrap()
}
fn step(
    db: &PositiveRecoveryStore,
    f: &Scenario,
    j: JournaledCoordinatorV2,
    event: RecoveryEventV2,
    daa: u64,
) -> (JournaledCoordinatorV2, Result<(), ErrorV2>) {
    let update = j.prepare_record(f.fresh(), 1, origin(daa), event, auth).unwrap();
    commit_prepared(db, update, || daa, |s| project(db, s)).unwrap()
}
struct Context<'a> {
    fixture: &'a Scenario,
    head: RecoveryHeadV2,
    now: u64,
}
impl RecoveryContextV2 for Context<'_> {
    type TrustedAnchor = Hash64;
    fn resolve(&self, anchor: &Hash64) -> Result<RecoveryAdmissionV2, ErrorV2> {
        if *anchor != self.fixture.result.branch {
            return Err(ErrorV2::Binding);
        }
        Ok(RecoveryAdmissionV2 { coordinator: self.fixture.fresh(), admitted_at: 1, expected: self.head })
    }
    fn authenticate(&self, _: &Hash64, o: EventOriginV2, e: &RecoveryEventV2) -> bool {
        auth(o, e)
    }
    fn current_daa(&self) -> u64 {
        self.now
    }
}
fn restore(db: &PositiveRecoveryStore, f: &Scenario, head: RecoveryHeadV2, now: u64) -> JournaledCoordinatorV2 {
    restore_selected(db, Context { fixture: f, head, now }, &f.result.branch, caps()).unwrap()
}

#[test]
fn actual_db_head_checked_fees_resources_and_undo_survive_restart_together() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let f = Scenario::new();
    let before_finish;
    let final_head;
    let transaction;
    {
        let db = store(&path);
        let j = initial(&db, &f);
        assert_eq!(fees(&db), (0, 0, 20));
        let req = f.request(&j);
        let (j, r) = step(&db, &f, j, req, 2);
        r.unwrap();
        let (j, r) = step(&db, &f, j, f.response(), 3);
        r.unwrap();
        assert_eq!(fees(&db), (10, 0, 10));
        let (j, r) = step(&db, &f, j, f.candidate(), 4);
        r.unwrap();
        let (j, r) = step(&db, &f, j, f.batch(), 5);
        r.unwrap();
        before_finish = j.head().unwrap();
        let (j, r) = step(&db, &f, j, RecoveryEventV2::Finish, 6);
        r.unwrap();
        final_head = j.head().unwrap();
        assert_eq!(j.state().unwrap().completion().claim, ClaimStatusV2::Passed);
        assert_eq!(fees(&db), (10, 10, 0));
        transaction = db.load(&raw(f.result.claim)).unwrap().unwrap().transaction;
    }
    {
        let db = store(&path);
        let restored = restore(&db, &f, final_head, 7);
        assert_eq!(restored.state().unwrap().completion().claim, ClaimStatusV2::Passed);
        assert_eq!(restored.state().unwrap().service_meter().paid, 10);
        assert_eq!(fees(&db), (10, 10, 0));
        assert_eq!(db.rollback(&raw(f.result.claim), &transaction).unwrap(), RollbackEffect::RolledBack);
        assert_eq!(db.rollback(&raw(f.result.claim), &transaction).unwrap(), RollbackEffect::AlreadyRolledBack);
        let rollback = restore(&db, &f, before_finish, 7);
        assert_eq!(rollback.state().unwrap().completion().claim, ClaimStatusV2::Checking);
        assert_eq!(rollback.state().unwrap().current().unwrap().cursor(), f.x.binding.step_leaf_count);
        assert_eq!(fees(&db), (10, 0, 10));
    }
}
#[test]
fn failed_projection_and_stale_checked_proposal_publish_neither_head_nor_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = store(&dir.path().join("db"));
    let f = Scenario::new();
    let j = initial(&db, &f);
    let head = j.head().unwrap();
    let req = f.request(&j);
    let proposal = j.prepare_record(f.fresh(), 1, origin(2), req, auth).unwrap();
    assert!(commit_prepared(&db, proposal, || 2, |_| Err(PersistenceErrorV2::Binding)).is_err());
    assert_eq!(fees(&db), (0, 0, 20));
    assert_eq!(db.load(&raw(f.result.claim)).unwrap().unwrap().head.digest, raw(head.digest));
    let a = restore(&db, &f, head, 1);
    let b = restore(&db, &f, head, 1);
    let req = f.request(&a);
    let (pa, ra) = step(&db, &f, a, req, 2);
    ra.unwrap();
    let accepted_head = pa.head().unwrap();
    let stale = b.prepare_record(f.fresh(), 1, origin(3), f.candidate(), auth).unwrap();
    assert!(commit_prepared(&db, stale, || 3, |s| project(&db, s)).is_err());
    let state = restore(&db, &f, accepted_head, 4);
    assert_eq!(state.state().unwrap().attempts().attempts, 0);
    assert_eq!(state.state().unwrap().service_meter().rounds, 1);
    assert_eq!(fees(&db), (0, 0, 20));
}
#[test]
fn expired_clock_only_proposal_commits_refund_with_unchanged_archive_head() {
    let dir = tempfile::tempdir().unwrap();
    let db = store(&dir.path().join("db"));
    let f = Scenario::new();
    let j = initial(&db, &f);
    let head = j.head().unwrap();
    let update = j.prepare_clock(100, |daa| daa == 100).unwrap();
    let (j, r) = commit_prepared(&db, update, || 100, |s| project(&db, s)).unwrap();
    r.unwrap();
    assert_eq!(j.head().unwrap(), head);
    assert_eq!(fees(&db), (0, 20, 0));
    let state = restore(&db, &f, head, 101);
    assert_eq!(state.state().unwrap().completion().claim, ClaimStatusV2::EndedUnverified);
    assert!(restore_selected(&db, Context { fixture: &f, head, now: 99 }, &f.result.branch, caps()).is_err());
}
#[test]
fn checked_chunk_import_replays_before_publishing_a_second_actual_store() {
    let dir = tempfile::tempdir().unwrap();
    let first = store(&dir.path().join("first"));
    let second = store(&dir.path().join("second"));
    let f = Scenario::new();
    let j = initial(&first, &f);
    let req = f.request(&j);
    let (j, r) = step(&first, &f, j, req, 2);
    r.unwrap();
    let (j, r) = step(&first, &f, j, f.response(), 3);
    r.unwrap();
    let head = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    let scope = RecoveryImportScopeV2 {
        network: f.result.network,
        branch: f.result.branch,
        ruleset: f.result.ruleset,
        case: f.result.claim,
        result: f.result.id(),
    };
    let mut receiver = RecoveryChunkImportV2::new(
        scope,
        head,
        bytes.len() as u64,
        RecoveryTransferLimitsV2 { chunk_bytes: 1024, max_chunks: 100_000, max_packets: 100_001, max_wire_bytes: 128 << 20 },
        caps(),
        Vec::new(),
    )
    .unwrap();
    for (index, chunk) in bytes.chunks(1024).enumerate() {
        receiver.accept(scope, head, index as u64, chunk).unwrap();
    }
    assert!(second.load(&raw(f.result.claim)).unwrap().is_none());
    let driver = RecoveryAdapterV2::new(Context { fixture: &f, head, now: 4 }, caps());
    let token = receiver.finish(&driver, &f.result.branch, |bytes, _| Ok(bytes)).unwrap();
    let restored = publish_import(&second, token, None, || 4, |s| project(&second, s)).unwrap();
    assert_eq!(restored.head().unwrap(), head);
    assert_eq!(fees(&second), (10, 0, 10));
    let (j, r) = step(&second, &f, restored, f.response(), 5);
    r.unwrap();
    assert_eq!(j.state().unwrap().service_meter().work, 2);
    assert_eq!(fees(&second), (10, 0, 10));
}

#[test]
#[ignore = "worker invoked by actual_process_exit_after_sync_commit_replays_without_double_payment"]
fn durable_storage_child() {
    let path = std::env::var_os("MISAKA_A6_PRODUCTION_CRASH_DB").expect("DB directory");
    let db = store(Path::new(&path));
    let f = Scenario::new();
    let j = initial(&db, &f);
    let req = f.request(&j);
    let (j, r) = step(&db, &f, j, req, 2);
    r.unwrap();
    let (_, r) = step(&db, &f, j, f.response(), 3);
    r.unwrap();
    std::process::exit(86);
}
#[test]
fn actual_process_exit_after_sync_commit_replays_without_double_payment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "durable_storage_child"])
        .env("MISAKA_A6_PRODUCTION_CRASH_DB", &path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(86), "{}", String::from_utf8_lossy(&output.stderr));
    let f = Scenario::new();
    // Resolve an independently replayed expected selected-history head; the DB
    // row's saved flags or its own digest are not used as computation authority.
    let mut expected = f.journal();
    let req = f.request(&expected);
    expected.record(origin(2), req, auth, |_, _| Ok(())).unwrap().unwrap();
    expected.record(origin(3), f.response(), auth, |_, _| Ok(())).unwrap().unwrap();
    let db = store(&path);
    let j = restore(&db, &f, expected.head().unwrap(), 4);
    assert_eq!(fees(&db), (10, 0, 10));
    let (j, r) = step(&db, &f, j, f.response(), 5);
    r.unwrap();
    assert_eq!(j.state().unwrap().service_meter().work, 2);
    assert_eq!(fees(&db), (10, 0, 10));
}

#[test]
fn restore_uses_one_native_clock_snapshot_for_store_check_and_replay() {
    use std::cell::Cell;
    struct Drifting<'a> {
        base: Context<'a>,
        reads: &'a Cell<usize>,
    }
    impl RecoveryContextV2 for Drifting<'_> {
        type TrustedAnchor = Hash64;
        fn resolve(&self, anchor: &Hash64) -> Result<RecoveryAdmissionV2, ErrorV2> {
            self.base.resolve(anchor)
        }
        fn authenticate(&self, anchor: &Hash64, o: EventOriginV2, e: &RecoveryEventV2) -> bool {
            self.base.authenticate(anchor, o, e)
        }
        fn current_daa(&self) -> u64 {
            let n = self.reads.get();
            self.reads.set(n + 1);
            if n == 0 { 100 } else { 80 }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = store(&dir.path().join("db"));
    let f = Scenario::new();
    let j = initial(&db, &f);
    let head = j.head().unwrap();
    let proposal = j.prepare_clock(90, |daa| daa == 90).unwrap();
    let (_, outcome) = commit_prepared(&db, proposal, || 90, |s| project(&db, s)).unwrap();
    outcome.unwrap();
    assert_eq!(db.load(&raw(f.result.claim)).unwrap().unwrap().observed_daa, 90);
    let reads = Cell::new(0);
    let restored =
        restore_selected(&db, Drifting { base: Context { fixture: &f, head, now: 0 }, reads: &reads }, &f.result.branch, caps())
            .unwrap();
    assert_eq!(reads.get(), 1, "store check and replay must share the same clock snapshot");
    assert_eq!(restored.state().unwrap().completion().claim, ClaimStatusV2::EndedUnverified);
    assert_eq!(restored.state().unwrap().service_meter().refunded, 20);
}

#[test]
fn startup_restore_commits_clock_projection_before_returning_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let f = Scenario::new();
    let head;
    {
        let db = store(&path);
        let j = initial(&db, &f);
        head = j.head().unwrap();
        assert_eq!(fees(&db), (0, 0, 20));
    }
    let db = store(&path);
    let restored =
        restore_and_commit(&db, Context { fixture: &f, head, now: 100 }, &f.result.branch, caps(), |s| project(&db, s)).unwrap();
    assert_eq!(restored.state().unwrap().completion().claim, ClaimStatusV2::EndedUnverified);
    assert_eq!(fees(&db), (0, 20, 0));
    let saved = db.load(&raw(f.result.claim)).unwrap().unwrap();
    assert_eq!(saved.observed_daa, 100);
    assert_eq!(saved.head.digest, raw(head.digest));
    let repeated = restore_and_commit(&db, Context { fixture: &f, head, now: 100 }, &f.result.branch, caps(), |_| {
        panic!("same-clock restart must be idempotent")
    })
    .unwrap();
    assert_eq!(repeated.state().unwrap().service_meter().refunded, 20);
    assert_eq!(fees(&db), (0, 20, 0));
}
fn checked_import(f: &Scenario, j: &JournaledCoordinatorV2, now: u64) -> CheckedRecoveryImportV2 {
    let head = j.head().unwrap();
    let bytes = j.encoded().unwrap();
    let scope = RecoveryImportScopeV2 {
        network: f.result.network,
        branch: f.result.branch,
        ruleset: f.result.ruleset,
        case: f.result.claim,
        result: f.result.id(),
    };
    let mut receiver = RecoveryChunkImportV2::new(
        scope,
        head,
        bytes.len() as u64,
        RecoveryTransferLimitsV2 { chunk_bytes: 1024, max_chunks: 100_000, max_packets: 100_000, max_wire_bytes: 128 << 20 },
        caps(),
        Vec::new(),
    )
    .unwrap();
    for (i, chunk) in bytes.chunks(1024).enumerate() {
        receiver.accept(scope, head, i as u64, chunk).unwrap();
    }
    let driver = RecoveryAdapterV2::new(Context { fixture: f, head, now }, caps());
    receiver.finish(&driver, &f.result.branch, |bytes, _| Ok(bytes)).unwrap()
}
#[test]
fn pruning_requires_rechecked_closed_archive_current_clock_and_all_retention_horizons() {
    let dir = tempfile::tempdir().unwrap();
    let db = store(&dir.path().join("db"));
    let f = Scenario::new();
    let j = initial(&db, &f);
    let req = f.request(&j);
    let (j, r) = step(&db, &f, j, req, 2);
    r.unwrap();
    let (j, r) = step(&db, &f, j, f.response(), 3);
    r.unwrap();
    let head = j.head().unwrap();
    let proposal = j.prepare_clock(200, |daa| daa == 200).unwrap();
    let (j, r) = commit_prepared(&db, proposal, || 200, |s| project(&db, s)).unwrap();
    r.unwrap();
    let token = checked_import(&f, &j, 200);
    let scope = token.scope();
    let horizon = RetentionV2 {
        pending: false,
        evidence_until: 120,
        liability_until: 120,
        fee_until: 120,
        reorg_until: 120,
        archive_available: true,
    };
    assert!(
        prune_retired(&db, &token, RecoveryRetentionV2::new(scope, head, RetentionV2 { pending: true, ..horizon }), 100, 128 << 20)
            .is_err()
    );
    assert!(
        prune_retired(
            &db,
            &token,
            RecoveryRetentionV2::new(scope, RecoveryHeadV2 { digest: hash(999), ..head }, horizon),
            100,
            128 << 20
        )
        .is_err()
    );
    assert!(
        prune_retired(&db, &token, RecoveryRetentionV2::new(scope, head, RetentionV2 { reorg_until: 201, ..horizon }), 100, 128 << 20)
            .is_err()
    );
    let update = j.prepare_clock(201, |daa| daa == 201).unwrap();
    let (j, r) = commit_prepared(&db, update, || 201, |s| project(&db, s)).unwrap();
    r.unwrap();
    assert!(
        prune_retired(&db, &token, RecoveryRetentionV2::new(scope, head, horizon), 100, 128 << 20).is_err(),
        "stale rechecked clock cannot authorize pruning"
    );
    let token = checked_import(&f, &j, 201);
    let before = db.load(&raw(f.result.claim)).unwrap().unwrap();
    let first = prune_retired(&db, &token, RecoveryRetentionV2::new(scope, head, horizon), 1, 128 << 20).unwrap();
    assert_eq!(first.deleted_records, 1);
    assert!(!first.complete);
    let rest = prune_retired(&db, &token, RecoveryRetentionV2::new(scope, head, horizon), 100, 128 << 20).unwrap();
    assert!(rest.complete);
    assert!(rest.deleted_records > 0);
    assert_eq!(db.load(&raw(f.result.claim)).unwrap().unwrap(), before);
    assert_eq!(fees(&db), (10, 10, 0));
    let restored = restore(&db, &f, head, 201);
    assert_eq!(restored.state().unwrap().completion().claim, ClaimStatusV2::EndedUnverified);
    assert!(
        db.rollback(&raw(f.result.claim), &before.transaction).is_err(),
        "expired undo is no longer a retained rollback authority"
    );
}

#[test]
fn stale_checked_finish_cannot_publish_after_the_authoritative_clock_moves() {
    let dir = tempfile::tempdir().unwrap();
    let db = store(&dir.path().join("db"));
    let f = Scenario::new();
    let j = initial(&db, &f);
    let req = f.request(&j);
    let (j, r) = step(&db, &f, j, req, 2);
    r.unwrap();
    let (j, r) = step(&db, &f, j, f.response(), 3);
    r.unwrap();
    let (j, r) = step(&db, &f, j, f.candidate(), 4);
    r.unwrap();
    let (j, r) = step(&db, &f, j, f.batch(), 5);
    r.unwrap();
    let head = j.head().unwrap();
    let saved = db.load(&raw(f.result.claim)).unwrap().unwrap();
    let prepared = j.prepare_record(f.fresh(), 1, origin(6), RecoveryEventV2::Finish, auth).unwrap();
    assert_eq!(prepared.checked_state().completion().claim, ClaimStatusV2::Passed);
    let rejected = commit_prepared(&db, prepared, || 100, |_| panic!("stale checked state must not reach native projection"));
    assert!(matches!(rejected, Err(PersistenceErrorV2::Clock)));
    assert_eq!(db.load(&raw(f.result.claim)).unwrap().unwrap(), saved);
    assert_eq!(fees(&db), (10, 0, 10));
    let restored = restore(&db, &f, head, 100);
    assert_eq!(restored.state().unwrap().completion().claim, ClaimStatusV2::EndedUnverified);
}

#[test]
fn stale_checked_import_is_rejected_before_projection_then_current_replay_can_publish() {
    let dir = tempfile::tempdir().unwrap();
    let target = store(&dir.path().join("target"));
    let f = Scenario::new();
    let mut j = f.journal();
    let req = f.request(&j);
    for (daa, event) in [(2, req), (3, f.response()), (4, f.candidate()), (5, f.batch()), (6, RecoveryEventV2::Finish)] {
        j.record(origin(daa), event, auth, |_, _| Ok(())).unwrap().unwrap();
    }
    let token = checked_import(&f, &j, 6);
    assert_eq!(token.state().state().unwrap().completion().claim, ClaimStatusV2::Passed);
    let rejected = publish_import(&target, token, None, || 100, |_| panic!("stale import must not reach native projection"));
    assert!(matches!(rejected, Err(PersistenceErrorV2::Clock)));
    assert!(target.load(&raw(f.result.claim)).unwrap().is_none());
    assert!(target.related_value(b"test-native/service-fees").unwrap().is_none());
    let rechecked = checked_import(&f, &j, 100);
    let restored = publish_import(&target, rechecked, None, || 100, |s| project(&target, s)).unwrap();
    assert_eq!(restored.state().unwrap().completion().claim, ClaimStatusV2::Passed);
    assert_eq!(target.load(&raw(f.result.claim)).unwrap().unwrap().observed_daa, 100);
    assert_eq!(fees(&target), (10, 10, 0));
}

#[test]
fn startup_restore_rechecks_authoritative_clock_after_replay_before_projecting_rows() {
    use std::cell::Cell;
    struct Advancing<'a> {
        base: Context<'a>,
        reads: &'a Cell<usize>,
    }
    impl RecoveryContextV2 for Advancing<'_> {
        type TrustedAnchor = Hash64;
        fn resolve(&self, anchor: &Hash64) -> Result<RecoveryAdmissionV2, ErrorV2> {
            self.base.resolve(anchor)
        }
        fn authenticate(&self, anchor: &Hash64, o: EventOriginV2, e: &RecoveryEventV2) -> bool {
            self.base.authenticate(anchor, o, e)
        }
        fn current_daa(&self) -> u64 {
            let n = self.reads.get();
            self.reads.set(n + 1);
            if n == 0 { 100 } else { 101 }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = store(&dir.path().join("db"));
    let f = Scenario::new();
    let j = initial(&db, &f);
    let head = j.head().unwrap();
    let proposal = j.prepare_clock(90, |daa| daa == 90).unwrap();
    let (_, r) = commit_prepared(&db, proposal, || 90, |s| project(&db, s)).unwrap();
    r.unwrap();
    let saved = db.load(&raw(f.result.claim)).unwrap().unwrap();
    assert_eq!(saved.observed_daa, 90);
    assert_eq!(fees(&db), (0, 0, 20));
    let reads = Cell::new(0);
    let rejected = restore_and_commit(
        &db,
        Advancing { base: Context { fixture: &f, head, now: 0 }, reads: &reads },
        &f.result.branch,
        caps(),
        |_| panic!("clock changed during replay; native rows must not be projected"),
    );
    assert!(matches!(rejected, Err(PersistenceErrorV2::Clock)));
    assert_eq!(reads.get(), 2, "startup must revalidate its pinned clock immediately before commit");
    assert_eq!(db.load(&raw(f.result.claim)).unwrap().unwrap(), saved);
    assert_eq!(fees(&db), (0, 0, 20));
}

#[test]
fn checksum_valid_shorter_or_advanced_cache_history_cannot_replace_the_external_protected_head() {
    use kaspa_consensus_core::palw_positive_court_v2::recovery_sync::RecoveryImportErrorV2;
    fn wire_replay<C: RecoveryContextV2<TrustedAnchor = Hash64>>(
        f: &Scenario,
        journal: &JournaledCoordinatorV2,
        context: C,
    ) -> Result<CheckedRecoveryImportV2, RecoveryImportErrorV2> {
        let head = journal.head().unwrap();
        let bytes = journal.encoded().unwrap();
        let scope = RecoveryImportScopeV2 {
            network: f.result.network,
            branch: f.result.branch,
            ruleset: f.result.ruleset,
            case: f.result.claim,
            result: f.result.id(),
        };
        let mut receiver = RecoveryChunkImportV2::new(
            scope,
            head,
            bytes.len() as u64,
            RecoveryTransferLimitsV2 { chunk_bytes: 1024, max_chunks: 100_000, max_packets: 100_000, max_wire_bytes: 128 << 20 },
            caps(),
            Vec::new(),
        )
        .unwrap();
        for (index, chunk) in bytes.chunks(1024).enumerate() {
            receiver.accept(scope, head, index as u64, chunk).unwrap();
        }
        let driver = RecoveryAdapterV2::new(context, caps());
        receiver.finish(&driver, &f.result.branch, |bytes, _| Ok(bytes))
    }
    struct RejectUnanchoredTail<'a> {
        base: Context<'a>,
    }
    impl RecoveryContextV2 for RejectUnanchoredTail<'_> {
        type TrustedAnchor = Hash64;
        fn resolve(&self, anchor: &Hash64) -> Result<RecoveryAdmissionV2, ErrorV2> {
            self.base.resolve(anchor)
        }
        fn authenticate(&self, anchor: &Hash64, origin: EventOriginV2, event: &RecoveryEventV2) -> bool {
            // The independently selected canonical history contains the request
            // but never accepted the extra source-response carrier/event.
            self.base.authenticate(anchor, origin, event) && origin.daa == 2 && matches!(event, RecoveryEventV2::Request { .. })
        }
        fn current_daa(&self) -> u64 {
            self.base.now
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let protected = store(&dir.path().join("protected"));
    let shorter = store(&dir.path().join("shorter-cache"));
    let target = store(&dir.path().join("unpublished-import"));
    let f = Scenario::new();
    let initial_history = initial(&protected, &f);
    let request = f.request(&initial_history);
    let (protected_history, outcome) = step(&protected, &f, initial_history, request, 2);
    outcome.unwrap();
    let pinned = protected_history.head().unwrap();
    assert_eq!(protected_history.state().unwrap().service_meter().rounds, 1);
    assert_eq!(protected_history.state().unwrap().service_meter().unique_bytes, 4096);

    // Both archives and their checksums are honestly encoded. The cache with a
    // shorter history has zero charged request bytes; it must not discount the
    // independently protected request obligation merely because it parses.
    let short_history = initial(&shorter, &f);
    assert_ne!(short_history.head().unwrap(), pinned);
    assert_eq!(short_history.state().unwrap().service_meter().unique_bytes, 0);
    assert!(matches!(
        restore_selected(&shorter, Context { fixture: &f, head: pinned, now: 3 }, &f.result.branch, caps()),
        Err(PersistenceErrorV2::Binding)
    ));
    assert!(wire_replay(&f, &short_history, Context { fixture: &f, head: pinned, now: 3 }).is_err());

    // Simulate a locally advanced but externally unanchored cache tail using
    // an actual DB transaction. The previous protected anchor stays pinned.
    let (advanced, outcome) = step(&protected, &f, protected_history, f.response(), 3);
    outcome.unwrap();
    let advanced_head = advanced.head().unwrap();
    assert_ne!(advanced_head, pinned);
    assert_eq!(protected.load(&raw(f.result.claim)).unwrap().unwrap().head.digest, raw(advanced_head.digest));
    assert!(matches!(
        restore_selected(&protected, Context { fixture: &f, head: pinned, now: 3 }, &f.result.branch, caps()),
        Err(PersistenceErrorV2::Binding)
    ));
    assert!(
        restore_and_commit(&protected, Context { fixture: &f, head: pinned, now: 3 }, &f.result.branch, caps(), |_| {
            panic!("cache-selected head must not reach native projection")
        })
        .is_err()
    );
    assert!(wire_replay(&f, &advanced, Context { fixture: &f, head: pinned, now: 3 }).is_err());
    assert!(target.load(&raw(f.result.claim)).unwrap().is_none());
    assert!(target.related_value(b"test-native/service-fees").unwrap().is_none());

    let checksum_valid = wire_replay(&f, &advanced, Context { fixture: &f, head: advanced_head, now: 3 }).unwrap();
    assert_eq!(checksum_valid.head(), advanced_head);
    assert_eq!(checksum_valid.state().state().unwrap().service_meter().unique_bytes, 4096);
    // Matching the checksum/head does not bypass origin/canonical-history
    // authentication. An authenticated identity is not an accepted event.
    assert!(matches!(
        restore_selected(
            &protected,
            RejectUnanchoredTail { base: Context { fixture: &f, head: advanced_head, now: 3 } },
            &f.result.branch,
            caps(),
        ),
        Err(PersistenceErrorV2::Core(ErrorV2::Binding))
    ));
    assert!(wire_replay(&f, &advanced, RejectUnanchoredTail { base: Context { fixture: &f, head: advanced_head, now: 3 } }).is_err());
}

#[test]
fn interrupted_actual_request_commit_restores_only_the_pinned_head_and_never_discounts_short_responses() {
    use kaspa_database::palw_positive_recovery::{CommitFault, RecoveryBinding, RecoveryCommit, RecoveryHead};
    for (index, fault) in [CommitFault::BeforeWrite, CommitFault::AfterWrite].into_iter().enumerate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let f = Scenario::new();
        let h0;
        let h1;
        {
            let db = store(&path);
            let journal = initial(&db, &f);
            h0 = journal.head().unwrap();
            let request = f.request(&journal);
            let prepared = journal.prepare_record(f.fresh(), 1, origin(2), request, auth).unwrap();
            let mut pinned_before_write = None;
            // A real prepared coordinator operation supplies the exact archive
            // and head. The hook injects an actual synchronous DB batch fault;
            // no successful persistence stub stands in for this write.
            let interrupted = prepared.commit(|previous, next, archive, state| {
                assert_eq!(previous, h0);
                pinned_before_write = Some(next);
                let meter = state.service_meter();
                assert_eq!((meter.rounds, meter.unique_bytes, meter.transmitted_bytes, meter.work), (1, 4096, 0, 0));
                let related = project(&db, state).map_err(|error| std::io::Error::other(format!("{error:?}")))?;
                db.commit_with_fault(
                    RecoveryCommit {
                        case: raw(f.result.claim),
                        binding: RecoveryBinding {
                            branch: raw(f.result.branch),
                            ruleset: raw(f.result.ruleset),
                            result: raw(f.result.id()),
                        },
                        expected_head: Some(RecoveryHead { records: previous.records, digest: raw(previous.digest) }),
                        head: RecoveryHead { records: next.records, digest: raw(next.digest) },
                        observed_daa: 2,
                        transaction: [71 + index as u8; 64],
                        archive: archive.to_vec(),
                        related,
                    },
                    fault,
                )
                .map(|_| ())
                .map_err(|error| std::io::Error::other(format!("{error:?}")))
            });
            assert!(interrupted.is_err());
            h1 = pinned_before_write.expect("head was pinned before the attempted write");
            assert_ne!(h0, h1);
        }
        // Resolve only the independently pinned pre-write/proposed heads. A
        // peer-selected checksum-valid suffix is never a third recovery choice.
        let db = store(&path);
        let (selected, rejected, initial_rounds, initial_unique) = match fault {
            CommitFault::BeforeWrite => (h0, h1, 0, 0),
            CommitFault::AfterWrite => (h1, h0, 1, 4096),
        };
        assert!(matches!(
            restore_selected(&db, Context { fixture: &f, head: rejected, now: 2 }, &f.result.branch, caps()),
            Err(PersistenceErrorV2::Binding)
        ));
        let journal = restore(&db, &f, selected, 2);
        let meter = journal.state().unwrap().service_meter();
        assert_eq!((meter.rounds, meter.unique_bytes, meter.transmitted_bytes, meter.work), (initial_rounds, initial_unique, 0, 0));
        assert_eq!(fees(&db), (0, 0, 20));
        let request = f.request(&journal);
        let (journal, outcome) = step(&db, &f, journal, request, 3);
        outcome.unwrap();
        let request = f.request(&journal);
        let (journal, outcome) = step(&db, &f, journal, request, 4);
        outcome.unwrap();
        assert_eq!(journal.state().unwrap().service_meter().rounds, 1);
        assert_eq!(journal.state().unwrap().service_meter().unique_bytes, 4096);
        let mut short = f.response();
        if let RecoveryEventV2::Response { bytes, .. } = &mut short {
            bytes.truncate(CHUNK_BYTES - 1);
        }
        let (journal, outcome) = step(&db, &f, journal, short, 5);
        assert_eq!(outcome, Err(ErrorV2::Wire));
        let meter = journal.state().unwrap().service_meter();
        assert_eq!((meter.rounds, meter.unique_bytes, meter.transmitted_bytes, meter.work, meter.paid), (1, 4096, 0, 0, 0));
        assert_eq!(
            journal.state().unwrap().completion().miner_data,
            kaspa_consensus_core::palw_positive_court_v2::coordinator::DataResponseStatusV2::Pending,
        );
        assert_eq!(fees(&db), (0, 0, 20));
        let (journal, outcome) = step(&db, &f, journal, f.response(), 6);
        outcome.unwrap();
        let (journal, outcome) = step(&db, &f, journal, f.response(), 7);
        outcome.unwrap();
        let meter = journal.state().unwrap().service_meter();
        // Each complete source reply includes its 76-byte opening. The short
        // malformed reply did not complete service or reduce either later debit.
        assert_eq!((meter.rounds, meter.unique_bytes, meter.transmitted_bytes, meter.work, meter.paid), (1, 4096, 8344, 2, 10));
        assert_eq!(fees(&db), (10, 0, 10));
        let final_head = journal.head().unwrap();
        drop(journal);
        drop(db);
        let db = store(&path);
        let restored = restore(&db, &f, final_head, 7);
        let meter = restored.state().unwrap().service_meter();
        assert_eq!((meter.rounds, meter.unique_bytes, meter.transmitted_bytes, meter.work, meter.paid), (1, 4096, 8344, 2, 10));
        assert_eq!(fees(&db), (10, 0, 10));
    }
}
