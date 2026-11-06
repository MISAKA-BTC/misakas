//! Real protected-store refusal ordering, not a simulated native acceptance.
use super::*;
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    identity::{ChainScope, Digest, principal},
    messages::{AggregateBudget, ClaimForm},
};
use misaka_palw_remote::miner::positive_v2::{rocksdb::RocksAnchorDbLimitsV2, sign_result_v2};

struct LocalResult {
    admission: AdmissionContext,
    result: MinerResultCommit,
    service: MinerServiceContract,
    key: [u8; PUBKEY_BYTES],
    signature: [u8; SIGNATURE_BYTES],
}
impl LocalResult {
    fn new(output: u8) -> Self {
        let d = |n| Digest([n; 64]);
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
        let key = *pair.verification_key.as_ref();
        let mut result = MinerResultCommit {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: ChainScope { network: d(1), branch: d(2), ruleset: d(3) },
            job: d(4),
            execution: d(5),
            miner: principal(&key),
            registered_class: d(6),
            artifact: d(7),
            program: d(8),
            input: d(9),
            tokenizer: d(10),
            execution_rules: d(11),
            selector: d(12),
            generation_seed: [13; 32],
            temperature_q24: 0,
            prefill: 1,
            decode: 1,
            max_context: 8,
            output: d(output),
            dependencies: d(14),
            service_contract: d(15),
        };
        let service = MinerServiceContract {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: result.case_id(),
            source_catalog: d(16),
            provider_delegations: d(17),
            budget: AggregateBudget {
                rounds: 4,
                unique_source_bytes: 1024,
                transmitted_bytes: 4096,
                additional_work: 1000,
                retained_bytes: 4096,
                retention_until: 100,
                fee: 50,
                witness_attempts: 2,
            },
            funding_consent: d(18),
            deadline: 90,
        };
        result.service_contract = service.id();
        let admission = AdmissionContext {
            scope: result.scope,
            expected_job: result.job,
            expected_execution: result.execution,
            expected_miner: result.miner,
            expected_input: result.input,
            artifact: result.artifact,
            program: result.program,
            tokenizer: result.tokenizer,
            execution_rules: result.execution_rules,
            selector: result.selector,
            registered_class: result.registered_class,
            required_form: ClaimForm::PositiveResultV2,
        };
        let (result, signature) = sign_result_v2(&admission, result, &service, 1, &key, |id, context| {
            Ok(*libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, id, context, [0; 32]).unwrap().as_ref())
        })
        .unwrap();
        Self { admission, result, service, key, signature }
    }
    fn signed(&self) -> SignedSourceCaseV2<'_> {
        SignedSourceCaseV2 {
            admission: &self.admission,
            result: self.result.clone(),
            service: self.service,
            key: &self.key,
            signature: &self.signature,
            now: 1,
        }
    }
}
fn process_limits() -> ProcessLimitsV2 {
    ProcessLimitsV2 {
        max_units_per_request: 8,
        max_tracked_units: 16,
        max_provider_reads: 32,
        max_status_checks: 1024,
        max_prepared_bytes: 1 << 20,
    }
}

#[test]
fn native_refusal_cannot_pin_a_result_or_journal_even_across_cold_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("protected-local-io");
    let journal = dir.path().join("not-admitted.journal");
    let limits = RocksAnchorDbLimitsV2 {
        max_cases: 1,
        max_sequence: 1024,
        max_logical_bytes: 1 << 20,
        write_buffer_bytes: 1 << 20,
        block_cache_bytes: 1 << 20,
        max_open_files: 32,
    };
    let refused = LocalResult::new(30);
    let alternate = LocalResult::new(31);
    assert_eq!(refused.result.case_id(), alternate.result.case_id());
    assert_ne!(refused.result.id(), alternate.result.id());
    let database = RocksAnchorDbV2::create(&path, limits).unwrap();
    for mode in [SourceJournalOpenV2::Create, SourceJournalOpenV2::Resume, SourceJournalOpenV2::DiscardUnanchoredTail] {
        let opened = NativeMinerSourceV2::open_resolved(
            refused.signed(),
            &[],
            process_limits(),
            SourceRecoveryV2 {
                database: &database,
                journal: &journal,
                limits: JournalLimitsV2 { max_bytes: 1 << 20, max_records: 1024 },
                mode,
            },
            |result| {
                assert_eq!(result.id(), refused.result.id());
                Err(MinerErrorV2::Unavailable)
            },
        );
        assert!(matches!(opened, Err(MinerErrorV2::Unavailable)));
        assert!(!journal.exists(), "native refusal must precede journal creation or recovery");
        drop(opened);
    }
    drop(database);

    // A real RocksDB reopen distinguishes an absent row from a released lock
    // on an already poisoned immutable binding. This second signed Result is
    // local test data, not a native-admitted success or a service authority.
    let database = RocksAnchorDbV2::open(&path, limits).unwrap();
    let responder = MinerResponderV2::<NativeSourceDutyV2>::new(
        &alternate.admission,
        alternate.result.clone(),
        alternate.service,
        1,
        &alternate.key,
        &alternate.signature,
        &[],
        process_limits(),
    )
    .unwrap();
    let anchor = database.bind(&responder).expect("refused native Result consumed no immutable case slot");
    drop(anchor);
    drop(database);
}
