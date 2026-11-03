//! In-memory A4 duty fixture, never a production native admission capability.
use super::*;
use kaspa_consensus_core::{
    palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1},
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_ids_opening_v1, prompt_token_ids_commitment_v1},
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    identity::{ChainScope, principal},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn budget() -> AggregateBudget {
    AggregateBudget {
        rounds: 4,
        unique_source_bytes: 1 << 18,
        transmitted_bytes: 1 << 20,
        additional_work: 1000,
        retained_bytes: 1 << 18,
        retention_until: 100,
        fee: 50,
        witness_attempts: 2,
    }
}
fn limits() -> ProcessLimitsV2 {
    ProcessLimitsV2 {
        max_units_per_request: 8,
        max_tracked_units: 16,
        max_provider_reads: 32,
        max_status_checks: 1024,
        max_prepared_bytes: 1 << 20,
    }
}
pub(super) struct Fixture {
    pub(super) result: MinerResultCommit,
    pub(super) contract: MinerServiceContract,
    pub(super) admission: AdmissionContext,
    pub(super) key: [u8; PUBKEY_BYTES],
    pub(super) signature: [u8; SIGNATURE_BYTES],
    pub(super) provider: Provider,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let operands: Vec<_> = (0..3)
            .map(|i| PalwArtifactOperandV1 {
                tensor_name: "embedding".into(),
                layer: None,
                row_start: i * 16,
                bytes: vec![i as u8; 16],
            })
            .collect();
        let root = artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
        let prompt = (0..65).collect::<Vec<_>>();
        let input = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &prompt).unwrap();
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
        let key: [u8; PUBKEY_BYTES] = *pair.verification_key.as_ref();
        let mut result = MinerResultCommit {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            scope: ChainScope { network: d(1), branch: d(2), ruleset: d(3) },
            job: d(4),
            execution: d(5),
            miner: principal(&key),
            registered_class: d(6),
            artifact: Digest(root.as_bytes()),
            program: d(7),
            input: Digest(input.as_bytes()),
            tokenizer: d(8),
            execution_rules: d(9),
            selector: d(10),
            generation_seed: [11; 32],
            temperature_q24: 0,
            prefill: 65,
            decode: 2,
            max_context: 128,
            output: d(12),
            dependencies: d(13),
            service_contract: d(14),
        };
        let contract = MinerServiceContract {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: result.case_id(),
            source_catalog: d(20),
            provider_delegations: d(21),
            budget: budget(),
            funding_consent: d(22),
            deadline: 90,
        };
        result.service_contract = contract.id();
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
        let (result, signature) = sign_result_v2(&admission, result, &contract, 1, &key, |id, context| {
            Ok(*libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, id, context, [0; 32]).unwrap().as_ref())
        })
        .unwrap();
        let mut data = BTreeMap::new();
        for i in 0..3 {
            data.insert(
                (SourceKind::ArtifactInventory, result.artifact, i),
                StoredUnitV2::Artifact(open_artifact_leaf_v1(&operands, i as u32).unwrap()),
            );
            data.insert(
                (SourceKind::Input, result.input, i),
                StoredUnitV2::Input(prompt_ids_opening_v1(&prompt, i as u32 * 32).unwrap()),
            );
        }
        Self { result, contract, admission, key, signature, provider: Provider { data, calls: Cell::new(0) } }
    }
    fn set_budget(&mut self, budget: AggregateBudget) {
        self.contract.budget = budget;
        self.result.service_contract = self.contract.id();
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
        self.signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &self.result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    }
    pub(super) fn request(&self, kind: SourceKind, first: u64, end: u64) -> SourceRequest {
        SourceRequest {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: self.result.case_id(),
            contract: self.result.service_contract,
            source: if kind == SourceKind::Input { self.result.input } else { self.result.artifact },
            kind,
            first,
            end,
        }
    }
    pub(super) fn duty(&self) -> Duty {
        Duty {
            result: self.result.id(),
            request: self.request(SourceKind::ArtifactInventory, 0, 1),
            not_requested: false,
            bounds: DutyBoundsV2 { source_units: 3, max_unit_bytes: 4096, work_per_read: 5, budget: budget(), now: 1, cutoff: 90 },
            complete: Rc::new(RefCell::new(BTreeSet::new())),
        }
    }
    pub(super) fn responder<'a>(
        &self,
        providers: &'a [&'a dyn StoredProviderV2],
        limits: ProcessLimitsV2,
    ) -> MinerResponderV2<'a, Duty> {
        MinerResponderV2::new(&self.admission, self.result.clone(), self.contract, 1, &self.key, &self.signature, providers, limits)
            .unwrap()
    }
}
pub(super) struct Provider {
    data: BTreeMap<(SourceKind, Digest, u64), StoredUnitV2>,
    pub(super) calls: Cell<u64>,
}
impl StoredProviderV2 for Provider {
    fn unit(&self, kind: SourceKind, source: Digest, index: u64) -> Result<&StoredUnitV2, String> {
        self.calls.set(self.calls.get() + 1);
        self.data.get(&(kind, source, index)).ok_or_else(|| "not stored".into())
    }
}
#[derive(Clone)]
pub(super) struct Duty {
    request: SourceRequest,
    not_requested: bool,
    result: misaka_palw_positive_contract::identity::ResultId,
    bounds: DutyBoundsV2,
    complete: Rc<RefCell<BTreeSet<(SourceKind, Digest, u64)>>>,
}
impl CanonicalDataDutyV2 for Duty {
    fn request(&self) -> &SourceRequest {
        &self.request
    }
    fn authorize(&self, result: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2> {
        if self.result != result.id() {
            return Err(MinerErrorV2::Binding);
        }
        Ok(self.bounds)
    }
    fn status(&self, index: u64) -> Result<MinerDataStatus, MinerErrorV2> {
        if self.not_requested {
            return Ok(MinerDataStatus::NotRequested);
        }
        let request = &self.request;
        Ok(if self.complete.borrow().contains(&(request.kind, request.source, index)) {
            MinerDataStatus::Complete
        } else {
            MinerDataStatus::Pending
        })
    }
}
impl MinerResponderV2<'_, Duty> {
    fn respond_to(&mut self, duty: &Duty, request: &SourceRequest) -> Result<PreparedDataV2, MinerErrorV2> {
        let scoped = Duty { request: *request, ..duty.clone() };
        self.respond(&scoped)
    }
}
impl Duty {
    // Actual G14 data authentication precedes the local fixture's accepted state.
    fn accept(&self, result: &MinerResultCommit, reply: &PreparedDataV2) {
        for (index, unit) in &reply.units {
            assert!(unit.authenticates(result, &reply.request, *index, self.bounds.source_units));
            self.complete.borrow_mut().insert((reply.request.kind, reply.request.source, *index));
        }
    }
}

#[test]
fn result_signature_binds_output_conditions_and_service_without_witness() {
    let f = Fixture::new();
    f.result.authenticate(&f.key, &f.signature).unwrap();
    for field in 0..8 {
        let mut changed = f.result.clone();
        match field {
            0 => changed.output = d(101),
            1 => changed.artifact = d(102),
            2 => changed.input = d(103),
            3 => changed.service_contract = d(104),
            4 => changed.scope.branch = d(105),
            5 => changed.generation_seed[0] ^= 1,
            6 => changed.dependencies = d(106),
            _ => changed.tokenizer = d(107),
        }
        assert!(changed.authenticate(&f.key, &f.signature).is_err());
    }
    // A provider and a witness API are not arguments to result signing.
    assert_eq!(f.provider.calls.get(), 0);
}

#[test]
fn signing_refuses_wrong_key_scope_and_bad_signatures() {
    let f = Fixture::new();
    let called = Cell::new(false);
    let bad_key = [0; PUBKEY_BYTES];
    assert!(
        sign_result_v2(&f.admission, f.result.clone(), &f.contract, 1, &bad_key, |_, _| {
            called.set(true);
            Ok([0; SIGNATURE_BYTES])
        })
        .is_err()
    );
    assert!(!called.get());
    let mut wrong = f.result.clone();
    wrong.scope.ruleset = d(99);
    assert!(
        sign_result_v2(&f.admission, wrong, &f.contract, 1, &f.key, |_, _| {
            called.set(true);
            Ok([0; SIGNATURE_BYTES])
        })
        .is_err()
    );
    assert!(!called.get());
    assert!(sign_result_v2(&f.admission, f.result.clone(), &f.contract, 1, &f.key, |_, _| Ok([0; SIGNATURE_BYTES])).is_err());
}

#[test]
fn prepared_data_needs_node_intake_and_overlap_has_one_unique_cost() {
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let mut miner = f.responder(&providers, limits());
    let duty = f.duty();
    let first = f.request(SourceKind::ArtifactInventory, 0, 2);
    let reply = miner.respond_to(&duty, &first).unwrap();
    assert_eq!(reply.units.len(), 2);
    assert_eq!(duty.status(0).unwrap(), MinerDataStatus::Pending);
    assert_eq!(miner.meter().unique_source_bytes, 32);
    duty.accept(&f.result, &reply);
    let overlap = f.request(SourceKind::ArtifactInventory, 1, 3);
    let reply = miner.respond_to(&duty, &overlap).unwrap();
    assert_eq!(reply.units.iter().map(|x| x.0).collect::<Vec<_>>(), vec![2]);
    assert_eq!(miner.meter().unique_source_bytes, 48);
    duty.accept(&f.result, &reply);
    let before = miner.meter();
    // Panel change/nonce cannot appear in A0's canonical request identity.
    assert!(miner.respond_to(&duty, &first).unwrap().units.is_empty());
    assert!(miner.respond_to(&duty, &overlap).unwrap().units.is_empty());
    assert_eq!(miner.meter().provider_reads, before.provider_reads);
    assert_eq!(miner.meter().prepared_bytes, before.prepared_bytes);
    assert_eq!(miner.meter().unique_source_bytes, before.unique_source_bytes);
}

#[test]
fn withheld_panel_receipt_and_external_provider_delivery_need_no_miner_resend() {
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let mut miner = f.responder(&providers, limits());
    let duty = f.duty();
    let request = f.request(SourceKind::Input, 0, 3);
    // Provider directly submits existing prompt openings while Miner is offline.
    let units = (0..3).map(|i| (i, f.provider.data[&(SourceKind::Input, f.result.input, i)].clone())).collect();
    duty.accept(&f.result, &PreparedDataV2 { request, units });
    let before = miner.meter();
    assert!(miner.respond_to(&duty, &request).unwrap().units.is_empty());
    assert_eq!(miner.meter().provider_reads, before.provider_reads);
    assert_eq!(miner.meter().prepared_bytes, before.prepared_bytes);
    assert_eq!(miner.meter().unique_source_bytes, before.unique_source_bytes);
    assert_eq!(f.provider.calls.get(), 0);
}

#[test]
fn invalid_provider_falls_back_without_access_to_miner_key() {
    let f = Fixture::new();
    let mut poison = Provider { data: f.provider.data.clone(), calls: Cell::new(0) };
    if let StoredUnitV2::Artifact(unit) = poison.data.get_mut(&(SourceKind::ArtifactInventory, f.result.artifact, 0)).unwrap() {
        unit.operand.bytes[0] ^= 1;
    }
    let providers: [&dyn StoredProviderV2; 2] = [&poison, &f.provider];
    let mut miner = f.responder(&providers, limits());
    let reply = miner.respond_to(&f.duty(), &f.request(SourceKind::ArtifactInventory, 0, 1)).unwrap();
    assert_eq!(reply.units.len(), 1);
    assert_eq!(miner.meter().provider_reads, 2);
    assert_eq!(miner.meter().additional_work, 10);
    assert_eq!(poison.calls.get(), 1);
    assert_eq!(f.provider.calls.get(), 1);
}

#[test]
fn retries_spend_transmission_and_work_but_do_not_reset_unique_union() {
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let mut miner = f.responder(&providers, limits());
    let duty = f.duty();
    let request = f.request(SourceKind::Input, 1, 2);
    let one = miner.respond_to(&duty, &request).unwrap();
    let before = miner.meter();
    let retry = miner.respond_to(&duty, &request).unwrap();
    assert_eq!(one, retry);
    assert_eq!(miner.meter().unique_source_bytes, before.unique_source_bytes);
    assert_eq!(miner.meter().rounds, before.rounds);
    assert_eq!(miner.meter().prepared_bytes, before.prepared_bytes * 2);
    assert_eq!(miner.meter().additional_work, before.additional_work * 2);
}

#[test]
fn malformed_wrong_scope_and_over_budget_requests_do_no_provider_io() {
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let mut miner = f.responder(&providers, limits());
    let duty = f.duty();
    let valid = f.request(SourceKind::ArtifactInventory, 0, 1);
    for which in 0..7 {
        let mut request = valid;
        match which {
            0 => request.first = 1,
            1 => request.end = u64::MAX,
            2 => request.version = 1,
            3 => request.contract = d(99),
            4 => request.source = d(99),
            5 => request.case = misaka_palw_positive_contract::identity::CaseId(d(99)),
            _ => request.revision += 1,
        }
        assert!(miner.respond_to(&duty, &request).is_err());
    }
    let mut closed = f.duty();
    closed.bounds.now = closed.bounds.cutoff;
    assert_eq!(miner.respond_to(&closed, &valid), Err(MinerErrorV2::Closed));
    let mut limited = f.duty();
    limited.bounds.budget.unique_source_bytes = 1;
    assert_eq!(miner.respond_to(&limited, &valid), Err(MinerErrorV2::Budget));
    assert_eq!(f.provider.calls.get(), 0);
    assert_eq!(miner.meter().provider_reads, 0);
    assert_eq!(miner.meter().prepared_bytes, 0);
}

#[test]
fn missing_corrupt_and_huge_provider_units_cannot_create_unbounded_work() {
    let f = Fixture::new();
    let empty = Provider { data: BTreeMap::new(), calls: Cell::new(0) };
    let providers: [&dyn StoredProviderV2; 1] = [&empty];
    let mut local = limits();
    local.max_provider_reads = 2;
    let mut miner = f.responder(&providers, local);
    let duty = f.duty();
    let request = f.request(SourceKind::Input, 0, 1);
    assert_eq!(miner.respond_to(&duty, &request), Err(MinerErrorV2::Unavailable));
    assert_eq!(miner.respond_to(&duty, &request), Err(MinerErrorV2::Unavailable));
    assert_eq!(miner.respond_to(&duty, &request), Err(MinerErrorV2::Budget));
    assert_eq!(empty.calls.get(), 2);
    let mut huge = Provider { data: f.provider.data.clone(), calls: Cell::new(0) };
    if let StoredUnitV2::Artifact(unit) = huge.data.get_mut(&(SourceKind::ArtifactInventory, f.result.artifact, 0)).unwrap() {
        unit.operand.bytes.resize(8192, 0);
    }
    let providers: [&dyn StoredProviderV2; 1] = [&huge];
    let mut miner = f.responder(&providers, limits());
    assert_eq!(miner.respond_to(&duty, &f.request(SourceKind::ArtifactInventory, 0, 1)), Err(MinerErrorV2::Unavailable));
    assert_eq!(miner.meter().prepared_bytes, 0);
}

#[test]
fn work_retention_round_and_process_limits_are_checked_before_read() {
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let request = f.request(SourceKind::Input, 0, 1);
    for which in 0..5 {
        let mut duty = f.duty();
        let mut local = limits();
        match which {
            0 => duty.bounds.budget.rounds = 0,
            1 => duty.bounds.budget.additional_work = 4,
            2 => duty.bounds.budget.retained_bytes = 1,
            3 => local.max_prepared_bytes = 1,
            _ => duty.bounds.budget.transmitted_bytes = 1,
        }
        let mut miner = f.responder(&providers, local);
        assert_eq!(miner.respond_to(&duty, &request), Err(MinerErrorV2::Budget));
    }
    assert_eq!(f.provider.calls.get(), 0);
}

#[test]
fn empty_provider_list_and_unadmitted_duties_fail_closed() {
    let f = Fixture::new();
    let mut miner = f.responder(&[], limits());
    let mut duty = f.duty();
    let request = f.request(SourceKind::Input, 0, 1);
    assert_eq!(miner.respond_to(&duty, &request), Err(MinerErrorV2::Unavailable));
    duty.result = misaka_palw_positive_contract::identity::ResultId(d(99));
    assert_eq!(miner.respond_to(&duty, &request), Err(MinerErrorV2::Binding));
}

#[test]
fn ordinary_inference_needs_no_trace_capture_or_witness_api() {
    use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder};
    let mut p = ProgramBuilder::new(4, 1 << 18);
    let embedding = p.param("embedding", DType::I8, &[4, 4], false);
    let mut pre = p.block("pre", vec![]);
    let x = pre.gather(embedding, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let logits = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(logits);
    let Ref::Node(logits) = logits else { unreachable!() };
    let post = post.finish(&[]);
    let program = p.finish(pre, vec![], post, logits);
    let plan = TirPlan::compile(&program).unwrap();
    let mut map = MapParams::default();
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], (1..=16).collect()).unwrap());
    let params = TirParams::from_map(&plan, &map).unwrap();
    let limits = InferenceLimitsV2 { max_prefill: 4, max_decode: 4, max_positions: 8 };
    assert_eq!(infer_tokens_v2(&plan, &params, &[1, 2], 3, limits).unwrap(), vec![3, 3, 3]);
    assert!(infer_tokens_v2(&plan, &params, &[], 3, limits).is_err());
    assert!(infer_tokens_v2(&plan, &params, &[4], 3, limits).is_err());
    assert!(infer_tokens_v2(&plan, &params, &[1], 0, limits).is_err());
    assert!(infer_tokens_v2(&plan, &params, &[1], 5, limits).is_err());
    assert!(infer_tokens_v2(&plan, &params, &[1], u32::MAX, limits).is_err());
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], vec![7; 16]).unwrap());
    let params = TirParams::from_map(&plan, &map).unwrap();
    assert_eq!(infer_tokens_v2(&plan, &params, &[3], 2, limits).unwrap(), vec![0, 0]);
}

#[test]
fn flat_prompt_is_one_bounded_authenticated_g14_unit() {
    let mut f = Fixture::new();
    let ids = vec![2; 65];
    f.result.input = Digest(prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &ids).unwrap().as_bytes());
    f.admission.expected_input = f.result.input;
    let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
    f.signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &f.result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    f.provider.data.insert((SourceKind::Input, f.result.input, 0), StoredUnitV2::FlatInput(ids.clone()));
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let mut miner = f.responder(&providers, limits());
    let mut duty = f.duty();
    duty.bounds.source_units = 1;
    let reply = miner.respond_to(&duty, &f.request(SourceKind::Input, 0, 1)).unwrap();
    assert_eq!(reply.units, vec![(0, StoredUnitV2::FlatInput(ids))]);
    duty.accept(&f.result, &reply);
    assert!(miner.respond_to(&duty, &reply.request).unwrap().units.is_empty());
}

#[test]
fn request_not_admitted_by_node_cannot_trigger_source_io() {
    let f = Fixture::new();
    let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
    let mut miner = f.responder(&providers, limits());
    let mut duty = f.duty();
    duty.not_requested = true;
    assert_eq!(miner.respond(&duty), Err(MinerErrorV2::Binding));
    assert_eq!(f.provider.calls.get(), 0);
    assert_eq!(miner.meter().provider_reads, 0);
    assert_eq!(miner.meter().prepared_bytes, 0);
}

#[test]
fn duty_or_panel_change_cannot_raise_signed_consent_caps() {
    for work_limited in [false, true] {
        let mut f = Fixture::new();
        let mut consent = budget();
        if work_limited {
            consent.additional_work = 5;
        } else {
            consent.rounds = 1;
        }
        f.set_budget(consent);
        let providers: [&dyn StoredProviderV2; 1] = [&f.provider];
        let mut miner = f.responder(&providers, limits());
        let duty = f.duty(); // Its broad quote cannot enlarge the signed cap.
        miner.respond_to(&duty, &f.request(SourceKind::ArtifactInventory, 0, 1)).unwrap();
        assert_eq!(miner.respond_to(&duty, &f.request(SourceKind::ArtifactInventory, 1, 2)), Err(MinerErrorV2::Budget));
        assert_eq!(f.provider.calls.get(), 1);
    }
}
