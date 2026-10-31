//! LOCAL integration of actual TIR checking, G14 data and A2's source ledger.
//! Registered class/job, accepted prefix, rates and roles are pinned test data,
//! not a production native admission, qualified payment or credit capability.
use super::*;
use kaspa_consensus_core::{
    palw_positive_common_source_replay_v2::*,
    palw_positive_source_geometry_v2::{GeometryLimitsV2, TirSourceGeometryV2},
    palw_positive_source_v2::{
        NativeSourceContextV2, SourceErrorV2, SourceEventCostV2, SourceLimitsV2, SourceRequestEventV2, SourceResponseEventV2,
        SourceServiceFeeV2, history::NativeSourceHistoryV2,
    },
    palw_tir_admission_v1::palw_tir_class_record_v1,
};
use misaka_palw_positive_contract::rows::AggregateUsage;

fn geometry(s: &Scenario) -> TirSourceGeometryV2 {
    let binding = &s.f.binding;
    let (registered, _) = palw_tir_class_record_v1(&binding.class, &binding.artifact_root).unwrap();
    TirSourceGeometryV2::derive(
        &binding.class,
        &registered,
        binding.artifact_root,
        &binding.job_context,
        &s.result,
        PalwPromptIdsFormV1::Flat,
        GeometryLimitsV2 { max_program_bytes: 1 << 20, max_parameter_instances: 64, max_inventory_leaves: 64, max_units_per_range: 8 },
    )
    .unwrap()
}
fn source_limits() -> SourceLimitsV2 {
    SourceLimitsV2 {
        max_requests: 8,
        max_responses: 8,
        max_history_events: 16,
        max_history_bytes: 1 << 20,
        max_intervals: 8,
        max_units_per_request: 8,
        max_native_body_bytes: 8192,
    }
}
fn source_material(record: CaseJournalRecordView<'_>) -> AcceptedMaterial {
    let origin = record.header().origin;
    AcceptedMaterial { origin, digest: Digest(origin.carrier_transaction.0) }
}
struct SourceFixture {
    scenario: Scenario,
    bodies: Vec<Vec<u8>>,
    requests: [SourceRequest; 2],
    raw: [Vec<Vec<u8>>; 2],
    funding_scope: FundingScopeId,
}
impl SourceFixture {
    fn new() -> Self {
        let scenario = Scenario::with_source_policy(false, true);
        let g = geometry(&scenario);
        assert_eq!(g.source_units(SourceKind::ArtifactInventory), 4);
        assert_eq!(g.source_units(SourceKind::Input), 1);
        // Reconstruct the actual tiny fixture's parameter inventory through the
        // real node inventory builder, then obtain its existing G14 opening.
        let program = scenario.f.binding.class.decode_program().unwrap();
        let mut map = MapParams::default();
        map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 1], vec![0, 1, 2, 3]).unwrap());
        let plan = TirPlan::compile(&program).unwrap();
        let params = TirParams::from_map(&plan, &map).unwrap();
        let source = TirParamsSourceV1(&params);
        let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
        assert_eq!(inventory.tree.root(), scenario.f.binding.artifact_root);
        let raw = [
            (0..inventory.tree.leaf_count())
                .map(|unit| borsh::to_vec(&inventory.tree.open(&program, &source, unit).unwrap()).unwrap())
                .collect(),
            vec![borsh::to_vec(&vec![0u32, 1]).unwrap()],
        ];
        let requests = [SourceKind::ArtifactInventory, SourceKind::Input].map(|kind| SourceRequest {
            version: 2,
            revision: 1,
            case: scenario.result.case_id(),
            contract: scenario.service.id(),
            kind,
            source: g.source_root(kind),
            first: 0,
            end: g.source_units(kind),
        });
        let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
        let requester = principal(keypair.verification_key.as_ref());
        let mut source_bodies: [Vec<Vec<u8>>; 2] = [vec![], vec![]];
        for ((request, responses), bodies) in requests.iter().zip(&raw).zip(&mut source_bodies) {
            let signature = *libcrux_ml_dsa::ml_dsa_87::sign(
                &keypair.signing_key,
                &request.signing_id(requester).0,
                REQUEST_SIGN_CONTEXT,
                [0; 32],
            )
            .unwrap()
            .as_ref();
            bodies.push(native_body(PositiveNativeOperationV2::SourceRequest { request: *request, requester, signature }));
            for response in responses {
                bodies.push(native_body(PositiveNativeOperationV2::SourceResponse {
                    case: scenario.result.case_id(),
                    request: request.id(),
                    response: response.clone(),
                }));
            }
        }
        // Meaningful source events occur BETWEEN real checked relations.
        let mut bodies = scenario.bodies[..3].to_vec();
        bodies.extend(source_bodies[0].iter().cloned());
        bodies.push(scenario.bodies[3].clone());
        bodies.extend(source_bodies[1].iter().cloned());
        bodies.extend(scenario.bodies[4..].iter().cloned());
        let initial = kaspa_consensus_core::palw_positive_court_v2::coordinator::native_body::decode_native_body_v2(
            &scenario.bodies[0],
            body_limits(),
        )
        .unwrap();
        let PositiveNativeOperationV2::ResultAdmission { consent, .. } = initial.operation else { unreachable!() };
        Self { scenario, bodies, requests, raw, funding_scope: consent.scope }
    }
    fn journal(&self) -> Journal {
        self.scenario.journal(&self.bodies)
    }
}

#[allow(dead_code)] // Diagnostic fixture errors retain their underlying cause.
#[derive(Debug)]
enum Error {
    Native(&'static str),
    Source(CommonSourceReplayErrorV2),
}
struct ToyUsagePolicy<'a> {
    active: &'a Cell<Option<u64>>,
    fail_at: Option<u64>,
}
impl NativeCommonUsagePolicyV2 for ToyUsagePolicy<'_> {
    fn map_usage_delta(
        &self,
        record: CaseJournalRecordView<'_>,
        _: Option<&PositiveNativeBodyV2>,
        court: CourtUsageDeltaV2,
        source: AggregateUsage,
    ) -> Result<NativeUsageChargeV2, CommonSourceReplayErrorV2> {
        if self.active.get() != Some(record.header().sequence) {
            return Err(CommonSourceReplayErrorV2::NativePolicy);
        }
        if self.fail_at == Some(record.header().sequence) {
            // Test-only fault AFTER the real source ledger applied the first
            // response. The callback changes no native store or live balance.
            assert_eq!(source.fee, 3);
            return Err(CommonSourceReplayErrorV2::NativePolicy);
        }
        // Arbitrary TEST-ONLY E+T work and signed-wire retention accounting.
        // A0/A4 have adopted no native conversion rate or tariff from this.
        Ok(NativeUsageChargeV2 {
            extra_transmitted_bytes: 0,
            additional_work: court.elements.checked_add(court.terms).ok_or(CommonSourceReplayErrorV2::Budget)?,
            retained_bytes: court.signed_wire_bytes,
        })
    }
}
struct SourceEnvironment<'a> {
    base: Environment<'a>,
    fixture: &'a SourceFixture,
    missing_history: bool,
    corrupt_basis: bool,
    corrupt_origin: bool,
    fee_quotes: Cell<u64>,
    last_usage: Cell<Option<AggregateUsage>>,
    poisoned: Cell<bool>,
    fee_timeline: std::cell::RefCell<Vec<(u8, u64)>>,
    fail_policy_at: Option<u64>,
}
impl<'a> SourceEnvironment<'a> {
    fn new(fixture: &'a SourceFixture, journal: &'a Journal) -> Self {
        Self {
            base: Environment::new(&fixture.scenario, journal),
            fixture,
            missing_history: false,
            corrupt_basis: false,
            corrupt_origin: false,
            fee_quotes: Cell::new(0),
            last_usage: Cell::new(None),
            poisoned: Cell::new(false),
            fee_timeline: std::cell::RefCell::new(vec![]),
            fail_policy_at: None,
        }
    }
    fn current_record(&self) -> Result<CaseJournalRecordView<'_>, SourceErrorV2> {
        let sequence = self.base.active.get().ok_or(SourceErrorV2::History)?;
        let bytes = self.base.accepted.records.get(sequence as usize - 1).ok_or(SourceErrorV2::History)?;
        CaseJournalRecordView::decode(bytes, journal_limits()).map_err(SourceErrorV2::Contract)
    }
    fn bind(&self, result: &MinerResultCommit, service: &MinerServiceContract) -> Result<(), SourceErrorV2> {
        // Readonly fixture authorization is always under the historical guard.
        self.current_record()?;
        if result != &self.fixture.scenario.result || service != &self.fixture.scenario.service {
            return Err(SourceErrorV2::Binding);
        }
        Ok(())
    }
}
impl NativeSourceContextV2 for SourceEnvironment<'_> {
    fn history_complete(
        &self,
        result: &MinerResultCommit,
        history: &NativeSourceHistoryV2,
        next: &AcceptedMaterial,
    ) -> Result<(), SourceErrorV2> {
        if self.missing_history || result != &self.fixture.scenario.result {
            return Err(SourceErrorV2::History);
        }
        let current = self.current_record()?;
        if source_material(current) != *next {
            return Err(SourceErrorV2::Binding);
        }
        let expected = self.base.accepted.records[..current.header().sequence as usize - 1].iter().filter_map(|bytes| {
            let record = CaseJournalRecordView::decode(bytes, journal_limits()).unwrap();
            (record.header().kind == CaseJournalEventKind::NativeBody && matches!(record.payload().get(4), Some(3 | 4)))
                .then(|| source_material(record))
        });
        if !history.entries().iter().map(|e| e.material).eq(expected) {
            return Err(SourceErrorV2::History);
        }
        Ok(())
    }
    fn request(
        &self,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        event: &SourceRequestEventV2<'_>,
    ) -> Result<SourceEventCostV2, SourceErrorV2> {
        self.bind(result, service)?;
        let record = self.current_record()?;
        if record.payload() != event.native_body
            || event.material != source_material(record)
            || *event.key != panel_key()
            || event.requester != principal(event.key)
        {
            return Err(SourceErrorV2::Binding);
        }
        // Explicit toy test tariff, not an adopted native work/outer-byte rate.
        Ok(SourceEventCostV2 { additional_work: 2, outer_carrier_bytes: 7, cutoff: 100, service_fee: 0 })
    }
    fn response(
        &self,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        event: &SourceResponseEventV2<'_>,
    ) -> Result<SourceEventCostV2, SourceErrorV2> {
        self.bind(result, service)?;
        let record = self.current_record()?;
        if record.payload() != event.native_body
            || event.material != source_material(record)
            || event.provider != result.miner
            || event.g14_data_range != (137, event.native_body.len() as u64)
        {
            return Err(SourceErrorV2::Binding);
        }
        // Explicit toy test tariff; fee is a due, never payment authority.
        Ok(SourceEventCostV2 { additional_work: 2, outer_carrier_bytes: 7, cutoff: 100, service_fee: 3 })
    }
    fn service_fee(
        &self,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        kind: SourceKind,
        source: Digest,
        unit: u64,
        data_bytes: u64,
    ) -> Result<SourceServiceFeeV2, SourceErrorV2> {
        self.bind(result, service)?;
        let (expected, units) = match kind {
            SourceKind::ArtifactInventory => ((result.artifact, 1), self.fixture.requests[0].end),
            SourceKind::Input => ((result.input, 8), self.fixture.requests[1].end),
        };
        if (source, data_bytes) != expected || unit >= units {
            return Err(SourceErrorV2::Binding);
        }
        self.fee_quotes.set(self.fee_quotes.get() + 1);
        // LOCAL three-unit test price, not an adopted native service schedule.
        Ok(SourceServiceFeeV2 { amount: 3, schedule: d(64), funding_scope: self.fixture.funding_scope })
    }
}

impl NativeReplayEnvironmentV2 for SourceEnvironment<'_> {
    type Error = Error;
    type CarrierError = &'static str;
    type ExternalState = CommonSourceReplayStateV2;
    type Verifier<'a>
        = StepVerifier<'a>
    where
        Self: 'a;
    type HistoricalStepGuard<'a>
        = StepGuard<'a>
    where
        Self: 'a;
    fn quote_record(&self, pin: &CaseJournalPinV2, record: CaseJournalRecordView<'_>) -> Result<NativeReplayStepQuoteV2, Error> {
        self.base.quote_record(pin, record).map_err(Error::Native)
    }
    fn activate_historical_step<'a>(
        &'a self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
    ) -> Result<Self::HistoricalStepGuard<'a>, Error> {
        self.base.activate_historical_step(pin, record).map_err(Error::Native)
    }
    fn validate_native_record(&self, pin: &CaseJournalPinV2, record: CaseJournalRecordView<'_>) -> Result<(), Error> {
        self.base.validate_native_record(pin, record).map_err(Error::Native)
    }
    fn with_expected_aux<T>(
        &self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        visit: impl FnOnce(&[AcceptedMaterial]) -> T,
    ) -> Result<T, Error> {
        self.base.with_expected_aux(pin, record, visit).map_err(Error::Native)
    }
    fn validate_canonical_preimages(&self, pin: &CaseJournalPinV2, record: CaseJournalRecordView<'_>) -> Result<(), Error> {
        self.base.validate_canonical_preimages(pin, record).map_err(Error::Native)
    }
    fn admission_inputs<'a>(
        &'a self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        body: &PositiveNativeBodyV2,
    ) -> Result<RootedReplayAdmissionV2<'a>, Error> {
        self.base.admission_inputs(pin, record, body).map_err(Error::Native)
    }
    fn panel_key(
        &self,
        pin: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        witness: WitnessId,
    ) -> Result<[u8; PUBKEY_BYTES], Error> {
        self.base.panel_key(pin, record, witness).map_err(Error::Native)
    }
    fn make_verifier(&self, pin: &CaseJournalPinV2) -> Result<Self::Verifier<'_>, Error> {
        self.base.make_verifier(pin).map_err(Error::Native)
    }
    fn new_external_state(&self, _: &CaseJournalPinV2) -> Result<Self::ExternalState, Error> {
        self.base.external_state_creations.set(self.base.external_state_creations.get() + 1);
        let s = &self.fixture.scenario;
        CommonSourceReplayStateV2::new(
            s.result.clone(),
            s.service,
            geometry(s),
            source_limits(),
            FreshSourceCaseBasisV2 {
                coverage: s.descriptor,
                reservation: s.aux.material,
                liability: s.aux.material,
                maturity_daa: 200,
                origin: accepted_origin(1),
            },
        )
        .map_err(Error::Source)
    }
    fn apply_external_event(
        &self,
        _: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        body: Option<&PositiveNativeBodyV2>,
        mut before: Option<LocalCourtSnapshotV2>,
        after: LocalCourtSnapshotV2,
        _: &LocalCourtOutcomeV2,
        _: &[LocalCheckedCourtFactV2],
        state: &mut Self::ExternalState,
    ) -> Result<ExternalReplayEffectV2, Error> {
        self.base.guard(record).map_err(Error::Native)?;
        self.base.external_calls.set(self.base.external_calls.get() + 1);
        let mut material = source_material(record);
        if self.corrupt_origin {
            material.origin.accepted_tx_ordinal += 1;
        }
        let key = panel_key();
        let event = match body.map(|b| &b.operation) {
            Some(PositiveNativeOperationV2::SourceRequest { request, requester, signature }) => {
                CommonSourceEventV2::Request(SourceRequestEventV2 {
                    request: *request,
                    requester: *requester,
                    key: &key,
                    signature,
                    material,
                    native_body: record.payload(),
                })
            }
            Some(PositiveNativeOperationV2::SourceResponse { request, .. }) => CommonSourceEventV2::Response(SourceResponseEventV2 {
                request: *request,
                provider: self.fixture.scenario.result.miner,
                material,
                native_body: record.payload(),
                g14_data_range: (137, record.payload().len() as u64),
            }),
            _ => CommonSourceEventV2::None,
        };
        if self.corrupt_basis && record.payload().get(4) == Some(&3) {
            before.as_mut().unwrap().charged_wire_bytes = 0;
        }
        let policy = ToyUsagePolicy { active: &self.base.active, fail_at: self.fail_policy_at };
        let effect = match state.apply_record(record, body, before, after, self, &policy, event) {
            Ok(effect) => effect,
            Err(error) => {
                let request = &self.fixture.requests[0];
                self.poisoned.set(
                    matches!(state.case_row(), Err(CommonSourceReplayErrorV2::Poisoned))
                        && matches!(state.usage(), Err(CommonSourceReplayErrorV2::Poisoned))
                        && matches!(state.history(), Err(CommonSourceReplayErrorV2::Poisoned))
                        && matches!(state.geometry(), Err(CommonSourceReplayErrorV2::Poisoned))
                        && matches!(state.result_id(), Err(CommonSourceReplayErrorV2::Poisoned))
                        && matches!(state.source_union(request.kind, request.source), Err(CommonSourceReplayErrorV2::Poisoned))
                        && matches!(state.receipt(request.kind, request.source, 0), Err(CommonSourceReplayErrorV2::Poisoned))
                        && matches!(state.fee_intent(request.kind, request.source, 0), Err(CommonSourceReplayErrorV2::Poisoned)),
                );
                return Err(Error::Source(error));
            }
        };
        let usage = state.usage().map_err(Error::Source)?;
        self.last_usage.set(Some(usage));
        self.fee_timeline.borrow_mut().push((record.payload()[4], usage.fee));
        Ok(effect)
    }
    fn validate_committed_effect(
        &self,
        _: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        _: Option<LocalCourtSnapshotV2>,
        after: LocalCourtSnapshotV2,
        _: &LocalCourtOutcomeV2,
        _: ExternalReplayEffectV2,
        state: &Self::ExternalState,
    ) -> Result<(), Error> {
        self.base.guard(record).map_err(Error::Native)?;
        let case = state.case_row().map_err(Error::Source)?;
        if case.last_event != record.header().origin || case.usage.transmitted_bytes < after.charged_wire_bytes {
            return Err(Error::Native("global Case differs from the actual accepted step"));
        }
        Ok(())
    }
    fn current_daa(&self, pin: &CaseJournalPinV2) -> Result<u64, Error> {
        self.base.current_daa(pin).map_err(Error::Native)
    }
    fn validate_final(
        &self,
        pin: &CaseJournalPinV2,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        historical: LocalCourtSnapshotV2,
        _: LocalCourtSnapshotV2,
        _: u64,
        _: &[LocalCheckedCourtFactV2],
        state: &Self::ExternalState,
    ) -> Result<(), Error> {
        self.base.final_calls.set(self.base.final_calls.get() + 1);
        let case = state.case_row().map_err(Error::Source)?;
        if pin != &self.base.accepted.pin
            || result != &self.fixture.scenario.result
            || service != &self.fixture.scenario.service
            || case.status != historical.status
            || case.usage.transmitted_bytes < historical.charged_wire_bytes
        {
            return Err(Error::Native("native fixture final Case/basis is stale"));
        }
        Ok(())
    }
}

#[test]
fn interleaved_real_g14_and_tir_replay_reconstructs_receipts_union_and_global_usage_exactly_once() {
    let f = SourceFixture::new();
    let j = f.journal();
    let env = SourceEnvironment::new(&f, &j);
    let fresh = env.new_external_state(&j.pin).unwrap();
    assert_eq!(fresh.usage().unwrap(), AggregateUsage::default());
    assert_eq!(fresh.case_row().unwrap().status, ClaimStatus::ResultCommitted);
    assert_eq!(fresh.history().unwrap().entries().len(), 0);
    let replay = replay_common_case_v2(&j, &j.links(), &env, replay_limits()).unwrap();
    assert_eq!(replay.facts()[0].terminal_kind(), TerminalKindV2::PositivePassed);
    let state = replay.external();
    let usage = state.usage().unwrap();
    let court = replay.historical_court();
    let source_wire: u64 = f.bodies.iter().filter(|b| matches!(b.get(4), Some(3 | 4))).map(|b| b.len() as u64).sum();
    let source_events = 2 + f.raw.iter().map(Vec::len).sum::<usize>() as u64;
    let receipt_count = f.raw.iter().map(Vec::len).sum::<usize>() as u64;
    assert_eq!(usage.rounds, 2);
    assert_eq!(usage.unique_source_bytes, 12);
    assert_eq!(usage.transmitted_bytes, court.charged_wire_bytes + source_wire + source_events * 7);
    assert_eq!(usage.additional_work, court.charged_elements + court.charged_terms + source_events * 2);
    assert_eq!(usage.retained_bytes, court.charged_wire_bytes + source_wire);
    assert_eq!(usage.fee, receipt_count * 3); // source due, never a native paid flag.
    assert_eq!(usage.witness_attempts, 1);
    assert_eq!(state.case_row().unwrap().usage, usage);
    assert_eq!(state.case_row().unwrap().miner_data, MinerDataStatus::Complete);
    assert_eq!(state.history().unwrap().entries().len() as u64, source_events);
    let mut first_receipt_fees = 0;
    for &(operation, fee) in env.fee_timeline.borrow().iter() {
        if operation == 4 {
            first_receipt_fees += 3;
        }
        assert_eq!(fee, first_receipt_fees, "Court and request events never add source fees");
    }
    for (index, request) in f.requests.iter().enumerate() {
        for (unit, raw) in f.raw[index].iter().enumerate() {
            let receipt = state.receipt(request.kind, request.source, unit as u64).unwrap().unwrap();
            assert_eq!(receipt.unit, unit as u64);
            assert_eq!(receipt.bytes, [1, 8][index]);
            assert_eq!(receipt.additional_work, 2);
            assert_eq!(receipt.provider, f.scenario.result.miner);
            assert_eq!(receipt.result, f.scenario.result.id());
            assert_eq!(receipt.settlement, None);
            let duty_index = if index == 0 { 3 } else { 5 + f.raw[0].len() };
            let duty = CaseJournalRecordView::decode(&j.records[duty_index], journal_limits()).unwrap();
            let response = CaseJournalRecordView::decode(&j.records[duty_index + 1 + unit], journal_limits()).unwrap();
            assert_eq!(receipt.duty, source_material(duty));
            assert_eq!(receipt.response, source_material(response));
            assert_ne!(receipt.response.digest, response.history().digest);
            let due = state.fee_intent(request.kind, request.source, unit as u64).unwrap().unwrap();
            assert_eq!(due.amount, 3);
            assert_eq!(due.funding_scope, f.funding_scope);
            assert_eq!(due.schedule, d(64));
            assert_eq!(due.accepted_response, receipt.response);
            let g = geometry(&f.scenario);
            assert_eq!(g.decode_g14_response_v2(request, raw).unwrap().0, unit as u64);
        }
        let union = state.source_union(request.kind, request.source).unwrap().unwrap();
        assert_eq!((union.source_units, union.requested_units, union.completed_units), (request.end, request.end, request.end));
    }
    assert_eq!(env.base.live_commits.get(), 0);
}

#[test]
fn copied_source_request_or_completed_response_cannot_append_or_charge_a_second_fee() {
    let f = SourceFixture::new();
    for response_copy in [false, true] {
        let mut bodies = f.bodies.clone();
        let original = if response_copy { 4 } else { 3 };
        bodies.insert(original + 1, bodies[original].clone());
        let j = f.scenario.journal(&bodies);
        let env = SourceEnvironment::new(&f, &j);
        assert!(matches!(
            replay_common_case_v2(&j, &j.links(), &env, replay_limits()),
            Err(CommonReplayErrorV2::Environment(Error::Source(_)))
        ));
        let prior = env.last_usage.get().unwrap();
        assert_eq!(prior.rounds, 1);
        assert_eq!(prior.fee, if response_copy { 3 } else { 0 });
        assert_eq!(env.base.final_calls.get(), 0);
        assert_eq!(env.base.live_commits.get(), 0);
        assert!(env.poisoned.get());
    }
}

#[test]
fn missing_duty_or_canonical_history_and_partial_global_basis_or_origin_substitution_are_rejected() {
    let f = SourceFixture::new();
    for variant in 0..4 {
        let mut bodies = f.bodies.clone();
        if variant == 0 {
            bodies.remove(3);
        }
        let j = f.scenario.journal(&bodies);
        let mut env = SourceEnvironment::new(&f, &j);
        env.missing_history = variant == 1;
        env.corrupt_basis = variant == 2;
        env.corrupt_origin = variant == 3;
        assert!(
            matches!(
                replay_common_case_v2(&j, &j.links(), &env, replay_limits()),
                Err(CommonReplayErrorV2::Environment(Error::Source(_)))
            ),
            "source binding variant {variant}"
        );
        assert_eq!(env.last_usage.get().unwrap().fee, 0);
        assert_eq!(env.base.final_calls.get(), 0);
        assert_eq!(env.base.live_commits.get(), 0);
        assert!(env.poisoned.get());
    }
}

#[test]
fn local_source_due_never_supplies_the_missing_qualified_native_settlement_port() {
    let f = SourceFixture::new();
    let mut bodies: Vec<_> = f.bodies.iter().map(|b| (CaseJournalEventKind::NativeBody, b.clone())).collect();
    bodies.push((CaseJournalEventKind::NativeTransaction, b"requires-actual-qualified-settlement".to_vec()));
    let j = Journal::new(&f.scenario, bodies, true);
    let env = SourceEnvironment::new(&f, &j);
    assert!(matches!(
        replay_common_case_v2(&j, &j.links(), &env, replay_limits()),
        Err(CommonReplayErrorV2::Environment(Error::Source(CommonSourceReplayErrorV2::MissingSettlementPort)))
    ));
    assert_eq!(env.last_usage.get().unwrap().fee, 15);
    assert_eq!(env.base.final_calls.get(), 0);
    assert_eq!(env.base.live_commits.get(), 0);
    assert!(env.poisoned.get());
}

#[test]
fn bad_current_panel_frame_after_source_completion_preserves_global_cost_receipts_and_first_fees() {
    let f = SourceFixture::new();
    let full = f.journal();
    let baseline_env = SourceEnvironment::new(&f, &full);
    let baseline = replay_common_case_v2(&full, &full.links(), &baseline_env, replay_limits()).unwrap();
    // Source duties and all five literal responses finish before relation2.
    let prefix = 3 + (1 + f.raw[0].len()) + 1 + (1 + f.raw[1].len());
    let bad = frame_bytes(&f.scenario.witness, f.scenario.descriptor, 2, &[0]);
    let mut bodies = f.bodies[..prefix].to_vec();
    bodies.push(Scenario::frame_body(&bad));
    let j = f.scenario.journal(&bodies);
    let env = SourceEnvironment::new(&f, &j);
    let replay = replay_common_case_v2(&j, &j.links(), &env, replay_limits()).unwrap();
    let court = replay.historical_court();
    assert_eq!(court.status, ClaimStatus::Pending);
    assert_eq!(court.last_attempt.unwrap().status, WitnessStatus::Rejected);
    assert_eq!(court.cursor, 2);
    assert_eq!(
        court.charged_wire_bytes,
        signed_bytes(&f.scenario.frames[0]) + signed_bytes(&f.scenario.frames[1]) + signed_bytes(&bad)
    );
    assert!(replay.facts().is_empty());
    assert!(matches!(replay.events().last().unwrap().court, LocalCourtOutcomeV2::SemanticError(LocalCourtErrorV2::Check(_))));
    let state = replay.external();
    let usage = state.usage().unwrap();
    let source_wire: u64 = f.bodies.iter().filter(|b| matches!(b.get(4), Some(3 | 4))).map(|b| b.len() as u64).sum();
    assert_eq!(usage.transmitted_bytes, court.charged_wire_bytes + source_wire + 7 * 7);
    assert_eq!(usage.additional_work, court.charged_elements + court.charged_terms + 7 * 2);
    assert_eq!(usage.unique_source_bytes, 12);
    assert_eq!(usage.fee, 15);
    assert_eq!(state.case_row().unwrap().miner_data, MinerDataStatus::Complete);
    for request in &f.requests {
        for unit in 0..request.end {
            assert_eq!(
                state.receipt(request.kind, request.source, unit).unwrap(),
                baseline.external().receipt(request.kind, request.source, unit).unwrap()
            );
            assert_eq!(
                state.fee_intent(request.kind, request.source, unit).unwrap(),
                baseline.external().fee_intent(request.kind, request.source, unit).unwrap()
            );
        }
    }
    assert_eq!(env.base.live_commits.get(), 0);
}

#[test]
fn policy_failure_after_real_source_apply_poisons_every_getter_and_a_fresh_replay_is_clean() {
    let f = SourceFixture::new();
    let j = f.journal();
    let mut env = SourceEnvironment::new(&f, &j);
    env.fail_policy_at = Some(5); // The first real artifact response.
    assert!(matches!(
        replay_common_case_v2(&j, &j.links(), &env, replay_limits()),
        Err(CommonReplayErrorV2::Environment(Error::Source(CommonSourceReplayErrorV2::NativePolicy)))
    ));
    assert!(env.poisoned.get(), "case/usage/history/geometry/result/union/receipt/due must all refuse the partial state");
    assert_eq!(env.last_usage.get().unwrap().fee, 0);
    assert_eq!(env.base.final_calls.get(), 0);
    assert_eq!(env.base.live_commits.get(), 0);
    let fresh_env = SourceEnvironment::new(&f, &j);
    let fresh = replay_common_case_v2(&j, &j.links(), &fresh_env, replay_limits()).unwrap();
    assert_eq!(fresh.facts()[0].terminal_kind(), TerminalKindV2::PositivePassed);
    assert_eq!(fresh.external().usage().unwrap().fee, 15);
    assert_eq!(fresh.external().history().unwrap().entries().len(), 7);
    assert_eq!(fresh.external().case_row().unwrap().miner_data, MinerDataStatus::Complete);
}
