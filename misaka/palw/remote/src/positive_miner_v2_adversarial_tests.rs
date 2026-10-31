//! Independent A2 resource-attack fixtures. These duties are local fixtures,
//! never a production native admission or computation-checking capability.
use super::*;
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    identity::{ChainScope, principal},
};
use std::cell::Cell;

struct MovingViewDuty {
    inner: super::tests::Duty,
    authorizations: Cell<u32>,
    change: u8,
}
impl CanonicalDataDutyV2 for MovingViewDuty {
    fn request(&self) -> &SourceRequest {
        self.inner.request()
    }
    fn authorize(&self, result: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2> {
        let mut bounds = self.inner.authorize(result)?;
        let calls = self.authorizations.get();
        self.authorizations.set(calls + 1);
        if calls > 0 {
            match self.change {
                0 => return Err(MinerErrorV2::Closed), // reorg invalidated the pinned view
                1 => bounds.now = bounds.cutoff,       // exclusive deadline while reading
                2 => bounds.budget.fee += 1,           // substituted reservation
                3 => bounds.now = 0,                   // clock rollback
                _ => unreachable!(),
            }
        }
        Ok(bounds)
    }
    fn status(&self, index: u64) -> Result<MinerDataStatus, MinerErrorV2> {
        self.inner.status(index)
    }
}

#[test]
fn changed_native_view_cannot_release_response_or_refund_local_read_cost() {
    let f = super::tests::Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    for change in 0..4 {
        let duty = MovingViewDuty { inner: f.duty(), authorizations: Cell::new(0), change };
        let mut responder = MinerResponderV2::<MovingViewDuty>::new(
            &f.admission,
            f.result.clone(),
            f.contract,
            1,
            &f.key,
            &f.signature,
            &providers,
            ProcessLimitsV2 {
                max_units_per_request: 8,
                max_tracked_units: 8,
                max_provider_reads: 8,
                max_status_checks: 8,
                max_prepared_bytes: 1 << 20,
            },
        )
        .unwrap();
        assert!(responder.respond(&duty).is_err());
        assert_eq!(duty.authorizations.get(), 2);
        // Work happened before the view change was observed. No response is
        // released, but those irreversible preparation costs stay charged.
        assert_eq!(responder.meter().provider_reads, 1);
        assert_eq!(responder.meter().unique_source_bytes, 16);
        assert_eq!(responder.meter().rounds, 1);
    }
}

fn digest(n: u8) -> Digest {
    Digest([n; 64])
}

struct BudgetDuty {
    request: SourceRequest,
    bounds: DutyBoundsV2,
    status_calls: Cell<u64>,
    complete: bool,
}
impl CanonicalDataDutyV2 for BudgetDuty {
    fn request(&self) -> &SourceRequest {
        &self.request
    }
    fn authorize(&self, _: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2> {
        Ok(self.bounds)
    }
    fn status(&self, _: u64) -> Result<MinerDataStatus, MinerErrorV2> {
        self.status_calls.set(self.status_calls.get() + 1);
        Ok(if self.complete { MinerDataStatus::Complete } else { MinerDataStatus::Pending })
    }
}
struct MustNotRead;
impl StoredProviderV2 for MustNotRead {
    fn unit(&self, _: SourceKind, _: Digest, _: u64) -> Result<&StoredUnitV2, String> {
        panic!("an over-budget request must not reach source I/O")
    }
}

struct SignedCase {
    result: MinerResultCommit,
    contract: MinerServiceContract,
    context: AdmissionContext,
    key: [u8; PUBKEY_BYTES],
    signature: [u8; SIGNATURE_BYTES],
}
impl SignedCase {
    fn new() -> Self {
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([173; 32]);
        let key = *keys.verification_key.as_ref();
        let mut result = MinerResultCommit {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: ChainScope { network: digest(1), ruleset: digest(2), branch: digest(3) },
            job: digest(4),
            execution: digest(5),
            miner: principal(&key),
            registered_class: digest(6),
            artifact: digest(7),
            program: digest(8),
            input: digest(9),
            tokenizer: digest(10),
            execution_rules: digest(11),
            selector: digest(12),
            generation_seed: [13; 32],
            temperature_q24: 0,
            prefill: 1,
            decode: 1,
            max_context: 2,
            output: digest(14),
            dependencies: digest(15),
            service_contract: digest(0),
        };
        let contract = MinerServiceContract {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: result.case_id(),
            source_catalog: digest(16),
            provider_delegations: digest(17),
            funding_consent: digest(18),
            deadline: 90,
            budget: AggregateBudget {
                rounds: 8,
                unique_source_bytes: 128,
                transmitted_bytes: 128,
                additional_work: 1,
                retained_bytes: 128,
                retention_until: 100,
                fee: 0,
                witness_attempts: 1,
            },
        };
        result.service_contract = contract.id();
        let context = AdmissionContext {
            scope: result.scope,
            expected_job: result.job,
            expected_execution: result.execution,
            expected_miner: result.miner,
            expected_input: result.input,
            registered_class: result.registered_class,
            artifact: result.artifact,
            program: result.program,
            tokenizer: result.tokenizer,
            execution_rules: result.execution_rules,
            selector: result.selector,
            required_form: ClaimForm::PositiveResultV2,
        };
        let signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        Self { result, contract, context, key, signature }
    }
    fn duty(&self, complete: bool) -> BudgetDuty {
        BudgetDuty {
            request: SourceRequest {
                version: CONTRACT_VERSION,
                revision: CONTRACT_REVISION,
                case: self.result.case_id(),
                contract: self.contract.id(),
                source: self.result.artifact,
                kind: SourceKind::ArtifactInventory,
                first: 0,
                end: 8,
            },
            bounds: DutyBoundsV2 {
                source_units: 8,
                max_unit_bytes: 128,
                work_per_read: 1,
                budget: self.contract.budget,
                now: 1,
                cutoff: 90,
            },
            status_calls: Cell::new(0),
            complete,
        }
    }
    fn limits(&self) -> ProcessLimitsV2 {
        ProcessLimitsV2 {
            max_units_per_request: 8,
            max_tracked_units: 1,
            max_provider_reads: 8,
            max_prepared_bytes: 128,
            max_status_checks: 16,
        }
    }
    fn responder<'a>(&self, providers: &'a [&'a dyn StoredProviderV2], limits: ProcessLimitsV2) -> MinerResponderV2<'a, BudgetDuty> {
        MinerResponderV2::new(&self.context, self.result.clone(), self.contract, 1, &self.key, &self.signature, providers, limits)
            .unwrap()
    }
}

#[test]
fn pending_union_is_rejected_before_accumulating_beyond_signed_memory_and_work() {
    let case = SignedCase::new();
    let provider = MustNotRead;
    let providers: [&dyn StoredProviderV2; 1] = [&provider];
    let mut miner = case.responder(&providers, case.limits());
    let duty = case.duty(false);
    assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Budget));
    // The old implementation collected all eight units before discovering that
    // even two pending units exceed the signed byte/work/metadata envelope.
    assert!(duty.status_calls.get() <= 2, "visited {} pending units before rejecting", duty.status_calls.get());
    assert_eq!(miner.meter().provider_reads, 0);
    assert_eq!(miner.meter().unique_source_bytes, 0);
    assert_eq!(miner.meter().prepared_bytes, 0);
}

#[test]
fn completed_units_cannot_hide_unbounded_canonical_status_lookup_work() {
    let case = SignedCase::new();
    let provider = MustNotRead;
    let providers: [&dyn StoredProviderV2; 1] = [&provider];
    let mut limits = case.limits();
    limits.max_status_checks = 3;
    let mut miner = case.responder(&providers, limits);
    let duty = case.duty(true);
    assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Budget));
    assert_eq!(duty.status_calls.get(), 3);
    assert_eq!(miner.meter().provider_reads, 0);
    // A nonce/Panel retry uses the SAME session; it may not buy another scan.
    assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Budget));
    assert_eq!(duty.status_calls.get(), 3);
}

#[test]
fn source_budget_arithmetic_overflow_is_refused_before_provider_io() {
    let case = SignedCase::new();
    let provider = MustNotRead;
    let providers: [&dyn StoredProviderV2; 1] = [&provider];
    let mut miner = case.responder(&providers, case.limits());
    let mut duty = case.duty(false);
    duty.bounds.max_unit_bytes = u64::MAX;
    assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Budget));
    assert!(duty.status_calls.get() <= 1);
    assert_eq!(miner.meter().provider_reads, 0);
}
