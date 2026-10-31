//! A6 common-journal integration with the actual independent TIR checker.
//! Native provenance, roles and clocks below are explicitly test fixtures.
//! No test establishes production admission, live funds or Gate C authority.
use super::*;
use kaspa_consensus_core::{
    palw_positive_common_replay_v2::*,
    palw_positive_contract_v2::record::RecordLimits,
    palw_positive_court_v2::coordinator::{
        contract::CarrierVerifierV2,
        native_body::{NativeBodyLimitsV2, PositiveNativeBodyV2, PositiveNativeOperationV2, encode_native_body_v2},
    },
    palw_positive_journal_chain_v2::*,
};
use misaka_palw_positive_contract::{
    funding::{FundingConsent, FundingPlanBasis, funding_scope},
    journal::*,
    record::{RecordHeader, encode_record},
    rows::{AcceptedMaterial, AcceptedOrigin, JournalHeadRow, NativeHash64},
};
use std::{cell::Cell, collections::BTreeMap};

fn body_limits() -> NativeBodyLimitsV2 {
    NativeBodyLimitsV2 { max_body_bytes: 2 << 20, max_opaque_bytes: 1 << 20, max_exact_relations: 1, max_signatures: 2 }
}
fn journal_limits() -> CaseJournalLimits {
    CaseJournalLimits {
        max_record_bytes: 3 << 20,
        max_payload_bytes: 2 << 20,
        max_aux_entries: 8,
        max_aux_preimage_bytes: 1 << 20,
        max_total_preimage_bytes: 3 << 20,
        max_logical_heap_bytes: 4 << 20,
    }
}
fn chain_limits() -> CaseJournalChainLimitsV2 {
    CaseJournalChainLimitsV2 {
        record_limits: journal_limits(),
        max_events: 4096,
        max_total_record_bytes: 1 << 32,
        max_hash_bytes: 1 << 34,
        max_index_bytes: 1 << 20,
    }
}
fn replay_limits() -> CommonReplayLimitsV2 {
    CommonReplayLimitsV2 {
        body: body_limits(),
        max_events: 4096,
        max_facts: 2,
        max_total_record_bytes: 1 << 34,
        max_hash_bytes: 1 << 36,
        max_work_units: 1 << 40,
        max_known_buffer_bytes: 1 << 30,
    }
}
fn accepted_origin(sequence: u64) -> AcceptedOrigin {
    let h = |n| NativeHash64(Hash64::from_u64_word(n).as_bytes());
    AcceptedOrigin {
        accepting_block: h(500 + sequence),
        carrier_block: h(1000 + sequence),
        carrier_transaction: h(2000 + sequence),
        accepted_tx_ordinal: 0,
        event_ordinal: 0,
        accepted_daa: 10,
        accepted_blue_score: sequence,
    }
}
fn native_body(operation: PositiveNativeOperationV2) -> Vec<u8> {
    encode_native_body_v2(&PositiveNativeBodyV2 { version: 2, revision: 1, operation }, body_limits()).unwrap()
}

struct Scenario {
    f: Fixture,
    result: MinerResultCommit,
    service: MinerServiceContract,
    witness: PanelWitnessCommit,
    descriptor: CoverageDescriptor,
    frames: Vec<Vec<u8>>,
    bodies: Vec<Vec<u8>>,
    aux: CaseJournalAux,
}
impl Scenario {
    fn new(mismatch: bool) -> Self {
        Self::with_source_policy(mismatch, false)
    }
    fn with_source_policy(mismatch: bool, source_policy: bool) -> Self {
        let f = fixture();
        let mut result = if mismatch { f.result(d(99)) } else { f.correct_result() };
        let mut service = service(&result);
        if source_policy {
            // LOCAL fixture policy only, no newly allocated protocol catalog
            // codec/domain. Pin existing roots and independently counted units
            // before the real payer/Result signatures are constructed below.
            let program = f.binding.class.decode_program().unwrap();
            let units = kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1::new(&program).unwrap().leaf_count();
            let catalog = borsh::to_vec(&(f.binding.artifact_root, f.binding.job_context.prompt_token_ids_hash, units, 1u32)).unwrap();
            service.source_catalog = Digest(blake2b_simd::Params::new().hash_length(64).hash(&catalog).as_bytes().try_into().unwrap());
        }
        let basis = FundingPlanBasis {
            profile: CheckProfile::ExactStreamV1,
            derivation: d(61),
            mandatory_relations: f.binding.step_leaf_count,
            source_catalog: service.source_catalog,
            resource_quote: d(62),
            policy: d(63),
            schedule: d(64),
            required_fee: 10,
            required_capital: 20,
        };
        let payer = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([94; 32]);
        let consent = FundingConsent {
            version: 2,
            revision: 1,
            case: result.case_id(),
            scope: funding_scope(&result, &service, &basis),
            payer: principal(payer.verification_key.as_ref()),
            max_fees: 100,
            max_locked: 200,
            funding_output_index: 1,
            refund_payload: d(65),
            admission_expiry: 90,
        };
        service.funding_consent = consent.id().0;
        result.service_contract = service.id();
        let payer_signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&payer.signing_key, &consent.id().0.0, FUNDING_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
        // Actual payer authentication is a signed-terms check, not a coin lock.
        consent.authenticate(&result, &service, &basis, 10, payer.verification_key.as_ref(), &payer_signature).unwrap();
        let prepared = f.prepare(&result, limits());
        let descriptor = prepared.descriptor();
        let witness = f.witness(&result, descriptor);
        let record_header = RecordHeader {
            version: 2,
            revision: 1,
            result: result.id(),
            artifact: result.artifact,
            full_logits_trace_root: native(f.binding.full_logits_trace_root),
            step_leaf_count: f.binding.step_leaf_count,
            step_merkle_root: native(f.binding.step_merkle_root),
            committed_execution_root: native(f.binding.committed_execution_root),
        };
        let record = encode_record(record_header, &f.generated, result.decode, 1 << 20).unwrap();
        let frames = f.frames(&witness, descriptor);
        let mut bodies = vec![
            native_body(PositiveNativeOperationV2::ResultAdmission {
                result: result.clone(),
                miner_signature: sign_result(&result),
                service,
                basis,
                consent,
                payer_signature,
            }),
            native_body(PositiveNativeOperationV2::WitnessCommit {
                witness: witness.clone(),
                panel_signature: sign_witness(&witness),
                record,
            }),
        ];
        for frame in &frames {
            bodies.push(Self::frame_body(frame));
        }
        bodies.push(native_body(PositiveNativeOperationV2::Finish {
            case: result.case_id(),
            result: result.id(),
            witness: witness.id(),
        }));
        let aux = CaseJournalAux {
            material: AcceptedMaterial { origin: accepted_origin(1), digest: native(f.binding.class.graph_ir_root()) },
            preimage: f.binding.class.program.clone(),
        };
        Self { f, result, service, witness, descriptor, frames, bodies, aux }
    }
    fn frame_body(bytes: &[u8]) -> Vec<u8> {
        let frame = EvidenceFrame::decode(bytes, court_limits().max_part_bytes).unwrap();
        native_body(PositiveNativeOperationV2::ExactFrame {
            frame: bytes.to_vec(),
            panel_signature: carrier_signature(&frame).signature,
        })
    }
    fn journal(&self, bodies: &[Vec<u8>]) -> Journal {
        Journal::new(self, bodies.iter().map(|b| (CaseJournalEventKind::NativeBody, b.clone())).collect(), true)
    }
}

#[path = "positive_court_a6_source.rs"]
mod real_source_replay;

struct Journal {
    pin: CaseJournalPinV2,
    records: Vec<Vec<u8>>,
    store: BTreeMap<Digest, Vec<u8>>,
}
impl Journal {
    fn new(s: &Scenario, bodies: Vec<(CaseJournalEventKind, Vec<u8>)>, with_aux: bool) -> Self {
        let mut records = Vec::new();
        let mut store = BTreeMap::new();
        let mut previous = d(0);
        let mut latest = None;
        for (index, (kind, payload)) in bodies.into_iter().enumerate() {
            let sequence = index as u64 + 1;
            let record = CaseJournalRecord {
                header: CaseJournalHeader {
                    version: 2,
                    revision: 1,
                    scope: s.result.scope,
                    case: s.result.case_id(),
                    result: s.result.id(),
                    sequence,
                    previous,
                    origin: accepted_origin(sequence),
                    kind,
                },
                payload,
                aux: if index == 0 && with_aux { vec![s.aux.clone()] } else { vec![] },
            };
            let bytes = record.encode(journal_limits()).unwrap();
            let view = CaseJournalRecordView::decode(&bytes, journal_limits()).unwrap();
            let head = view.head();
            latest = Some(JournalHeadRow { case: s.result.case_id(), sequence, previous, head, history: view.history() });
            previous = head;
            store.insert(head, bytes.clone());
            records.push(bytes);
        }
        Self {
            pin: CaseJournalPinV2 { scope: s.result.scope, case: s.result.case_id(), result: s.result.id(), head: latest.unwrap() },
            records,
            store,
        }
    }
    fn links(&self) -> CheckedCaseJournalLinksV2 {
        check_case_journal_links_v2(self, self.pin, chain_limits()).unwrap()
    }
}
impl CaseJournalSourceV2 for Journal {
    type Error = &'static str;
    fn with_record<T>(&self, head: Digest, max_bytes: u64, visit: impl FnOnce(&[u8]) -> T) -> Result<T, Self::Error> {
        let bytes = self.store.get(&head).ok_or("missing fixture record")?;
        if bytes.len() as u64 > max_bytes {
            return Err("fixture record exceeds source bound");
        }
        Ok(visit(bytes))
    }
}

#[derive(Default)]
struct Shadow {
    events: u64,
    wire: u64,
}
struct StepGuard<'a>(&'a Cell<Option<u64>>);
impl Drop for StepGuard<'_> {
    fn drop(&mut self) {
        self.0.set(None);
    }
}
struct StepVerifier<'a> {
    env: &'a Environment<'a>,
    actual: ActualVerifier,
}
impl CarrierVerifierV2 for StepVerifier<'_> {
    type Authorization = PanelPacketSignatureV2;
    type Error = &'static str;
    fn authorize_candidate(&self, result: &MinerResultCommit, witness: &PanelWitnessCommit) -> Result<(), Self::Error> {
        self.env.role()?;
        self.actual.authorize_candidate(result, witness).map_err(|_| "actual Panel role verification failed")
    }
    fn max_carrier_overhead(&self) -> u64 {
        self.actual.max_carrier_overhead()
    }
    fn verify(
        &self,
        result: &MinerResultCommit,
        witness: &PanelWitnessCommit,
        frame: &EvidenceFrame<'_>,
        authorization: &Self::Authorization,
    ) -> Result<u64, Self::Error> {
        self.env.role()?;
        self.actual.verify(result, witness, frame, authorization).map_err(|_| "actual Panel carrier verification failed")
    }
}

/// Only the tests' independently generated accepted-prefix fixture. It has no
/// native constructor, writable store, coin account or Gate C implementation.
struct Environment<'a> {
    scenario: &'a Scenario,
    accepted: &'a Journal,
    key: [u8; PUBKEY_BYTES],
    active: Cell<Option<u64>>,
    quote_calls: Cell<u64>,
    native_calls: Cell<u64>,
    external_state_creations: Cell<u64>,
    external_calls: Cell<u64>,
    role_calls: Cell<u64>,
    final_calls: Cell<u64>,
    live_commits: Cell<u64>,
    now: Cell<u64>,
    deny_role_at: Option<u64>,
    fail_final: bool,
    drift_final: bool,
    quote_work: u64,
}
impl<'a> Environment<'a> {
    fn new(scenario: &'a Scenario, accepted: &'a Journal) -> Self {
        Self {
            scenario,
            accepted,
            key: miner_key(),
            active: Cell::new(None),
            quote_calls: Cell::new(0),
            native_calls: Cell::new(0),
            external_state_creations: Cell::new(0),
            external_calls: Cell::new(0),
            role_calls: Cell::new(0),
            final_calls: Cell::new(0),
            live_commits: Cell::new(0),
            now: Cell::new(50),
            deny_role_at: None,
            fail_final: false,
            drift_final: false,
            quote_work: 1000,
        }
    }
    fn guard(&self, record: CaseJournalRecordView<'_>) -> Result<(), &'static str> {
        if self.active.get() != Some(record.header().sequence) {
            return Err("historical step guard absent");
        }
        Ok(())
    }
    fn role(&self) -> Result<(), &'static str> {
        self.role_calls.set(self.role_calls.get() + 1);
        let sequence = self.active.get().ok_or("verifier lost historical step guard")?;
        if self.deny_role_at == Some(sequence) {
            return Err("fixture role unavailable at this accepted step");
        }
        Ok(())
    }
}
impl NativeReplayEnvironmentV2 for Environment<'_> {
    type Error = &'static str;
    type CarrierError = &'static str;
    type ExternalState = Shadow;
    type Verifier<'a>
        = StepVerifier<'a>
    where
        Self: 'a;
    type HistoricalStepGuard<'a>
        = StepGuard<'a>
    where
        Self: 'a;
    fn quote_record(&self, _: &CaseJournalPinV2, _: CaseJournalRecordView<'_>) -> Result<NativeReplayStepQuoteV2, Self::Error> {
        self.quote_calls.set(self.quote_calls.get() + 1);
        Ok(NativeReplayStepQuoteV2 { work_units: self.quote_work, hash_bytes: 1000, peak_native_heap_bytes: 1 << 24 })
    }
    fn activate_historical_step<'a>(
        &'a self,
        _: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
    ) -> Result<Self::HistoricalStepGuard<'a>, Self::Error> {
        if self.active.replace(Some(record.header().sequence)).is_some() {
            return Err("nested fixture historical guard");
        }
        Ok(StepGuard(&self.active))
    }
    fn validate_native_record(&self, pin: &CaseJournalPinV2, record: CaseJournalRecordView<'_>) -> Result<(), Self::Error> {
        self.guard(record)?;
        self.native_calls.set(self.native_calls.get() + 1);
        if pin != &self.accepted.pin {
            return Err("fixture native head changed");
        }
        let expected = self.accepted.records.get((record.header().sequence - 1) as usize).ok_or("fixture acceptance missing")?;
        let expected = CaseJournalRecordView::decode(expected, journal_limits()).unwrap();
        if record.header() != expected.header() || record.payload() != expected.payload() {
            return Err("fixture accepted carrier substitution");
        }
        Ok(())
    }
    fn with_expected_aux<T>(
        &self,
        _: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        visit: impl FnOnce(&[AcceptedMaterial]) -> T,
    ) -> Result<T, Self::Error> {
        self.guard(record)?;
        Ok(visit(if record.header().sequence == 1 { std::slice::from_ref(&self.scenario.aux.material) } else { &[] }))
    }
    fn validate_canonical_preimages(&self, _: &CaseJournalPinV2, record: CaseJournalRecordView<'_>) -> Result<(), Self::Error> {
        self.guard(record)?;
        for aux in record.aux() {
            if aux.material() != self.scenario.aux.material || aux.preimage() != self.scenario.aux.preimage {
                return Err("fixture native material/preimage mismatch");
            }
        }
        Ok(())
    }
    fn admission_inputs<'a>(
        &'a self,
        _: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        _: &PositiveNativeBodyV2,
    ) -> Result<RootedReplayAdmissionV2<'a>, Self::Error> {
        self.guard(record)?;
        Ok(RootedReplayAdmissionV2 {
            context: rooted(&self.scenario.result),
            registered: &self.scenario.f.binding.class,
            artifact: self.scenario.f.binding.artifact_root,
            job: &self.scenario.f.binding.job_context,
            form: PalwPromptIdsFormV1::Flat,
            check_limits: limits(),
            record_limits: RecordLimits { max_record_bytes: 1 << 20, max_generated_heap_bytes: 1 << 20 },
            court_limits: court_limits(),
            miner_key: &self.key,
        })
    }
    fn panel_key(
        &self,
        _: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        witness: WitnessId,
    ) -> Result<[u8; PUBKEY_BYTES], Self::Error> {
        self.guard(record)?;
        if witness != self.scenario.witness.id() {
            return Err("unassigned witness");
        }
        Ok(panel_key())
    }
    fn make_verifier(&self, _: &CaseJournalPinV2) -> Result<Self::Verifier<'_>, Self::Error> {
        Ok(StepVerifier { env: self, actual: verifier_for(&self.scenario.result) })
    }
    fn new_external_state(&self, _: &CaseJournalPinV2) -> Result<Self::ExternalState, Self::Error> {
        self.external_state_creations.set(self.external_state_creations.get() + 1);
        Ok(Shadow::default())
    }
    fn apply_external_event(
        &self,
        _: &CaseJournalPinV2,
        record: CaseJournalRecordView<'_>,
        body: Option<&PositiveNativeBodyV2>,
        _: Option<LocalCourtSnapshotV2>,
        after: LocalCourtSnapshotV2,
        _: &LocalCourtOutcomeV2,
        _: &[LocalCheckedCourtFactV2],
        state: &mut Self::ExternalState,
    ) -> Result<ExternalReplayEffectV2, Self::Error> {
        self.guard(record)?;
        self.external_calls.set(self.external_calls.get() + 1);
        if matches!(
            body.map(|b| &b.operation),
            None | Some(
                PositiveNativeOperationV2::SourceRequest { .. }
                    | PositiveNativeOperationV2::SourceResponse { .. }
                    | PositiveNativeOperationV2::PositiveCourtAttestation { .. }
            )
        ) {
            return Err("source/funding/qualified native transaction port unavailable in this fixture");
        }
        state.events += 1;
        state.wire = after.charged_wire_bytes;
        Ok(ExternalReplayEffectV2::NoOp)
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
    ) -> Result<(), Self::Error> {
        self.guard(record)?;
        if state.events != record.header().sequence || state.wire != after.charged_wire_bytes {
            return Err("fixture shadow differs from recomputed Court");
        }
        Ok(())
    }
    fn current_daa(&self, _: &CaseJournalPinV2) -> Result<u64, Self::Error> {
        Ok(self.now.get())
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
    ) -> Result<(), Self::Error> {
        self.final_calls.set(self.final_calls.get() + 1);
        if self.fail_final
            || pin != &self.accepted.pin
            || result != &self.scenario.result
            || service != &self.scenario.service
            || state.events != pin.head.sequence
            || state.wire != historical.charged_wire_bytes
        {
            return Err("fixture native generation/head/rows are stale");
        }
        if self.drift_final {
            self.now.set(self.now.get() + 1);
        }
        Ok(())
    }
}

#[test]
fn actual_tir_journal_replay_recomputes_pass_and_genuine_miner_mismatch() {
    for mismatch in [false, true] {
        let s = Scenario::new(mismatch);
        let j = s.journal(&s.bodies);
        let env = Environment::new(&s, &j);
        let replay = replay_common_case_v2(&j, &j.links(), &env, replay_limits()).unwrap();
        let expected = if mismatch { TerminalKindV2::ClaimRefuted } else { TerminalKindV2::PositivePassed };
        assert_eq!(replay.facts().len(), 1);
        assert_eq!(replay.facts()[0].terminal_kind(), expected);
        assert_eq!(replay.current_court().cursor, s.descriptor.mandatory_relations);
        assert_eq!(replay.current_court().charged_wire_bytes, s.frames.iter().map(|f| signed_bytes(f)).sum::<u64>());
        assert_eq!(replay.events().len(), s.bodies.len());
        assert!(env.role_calls.get() >= s.frames.len() as u64 + 1);
        assert_eq!(env.active.get(), None);
        assert_eq!(env.live_commits.get(), 0);
    }
}

#[test]
fn actual_checker_panel_failure_keeps_charged_wire_without_miner_refutation() {
    let s = Scenario::new(false);
    let bad = frame_bytes(&s.witness, s.descriptor, 1, &[0]);
    let bodies = vec![s.bodies[0].clone(), s.bodies[1].clone(), s.bodies[2].clone(), Scenario::frame_body(&bad)];
    let j = s.journal(&bodies);
    let env = Environment::new(&s, &j);
    let replay = replay_common_case_v2(&j, &j.links(), &env, replay_limits()).unwrap();
    assert_eq!(replay.historical_court().status, ClaimStatus::Pending);
    assert_eq!(replay.historical_court().cursor, 1);
    assert_eq!(replay.historical_court().last_attempt.unwrap().status, WitnessStatus::Rejected);
    assert_eq!(replay.historical_court().charged_wire_bytes, signed_bytes(&s.frames[0]) + signed_bytes(&bad));
    assert!(matches!(replay.events().last().unwrap().court, LocalCourtOutcomeV2::SemanticError(LocalCourtErrorV2::Check(_))));
    assert!(replay.facts().is_empty());
}

#[test]
fn omitted_record_or_relation_and_future_or_idempotent_court_deliveries_are_not_journal_events() {
    let s = Scenario::new(false);
    for variant in 0..4 {
        let mut bodies = s.bodies.clone();
        match variant {
            0 => {
                bodies.remove(1);
            }
            1 => {
                bodies.remove(2);
            }
            2 => {
                bodies.insert(2, bodies[1].clone());
            }
            _ => {
                bodies.insert(3, bodies[2].clone());
            }
        }
        let j = s.journal(&bodies);
        let env = Environment::new(&s, &j);
        assert!(replay_common_case_v2(&j, &j.links(), &env, replay_limits()).is_err(), "omission/repeat variant {variant}");
        assert_eq!(env.external_calls.get(), [1, 2, 2, 3][variant], "rejected input never reaches the external effect callback");
        assert_eq!(env.live_commits.get(), 0);
        assert_eq!(env.final_calls.get(), 0);
    }
}

#[test]
fn pinned_tail_and_exact_native_material_references_cannot_be_shortened() {
    let s = Scenario::new(false);
    let mut j = s.journal(&s.bodies);
    let links = j.links();
    j.store.remove(&j.pin.head.head);
    let env = Environment::new(&s, &j);
    assert!(replay_common_case_v2(&j, &links, &env, replay_limits()).is_err());
    // Forward visitation permits readonly prefix callbacks before discovering
    // a later missing record. No replay state or effect may escape that error.
    assert_eq!(env.native_calls.get(), j.records.len() as u64 - 1);
    assert_eq!(env.external_state_creations.get(), 0);
    assert_eq!(env.external_calls.get(), 0);
    assert_eq!(env.final_calls.get(), 0);
    assert_eq!(env.live_commits.get(), 0);
    assert_eq!(env.active.get(), None);
    let shortened = s.journal(&s.bodies[..s.bodies.len() - 1]);
    let expected = s.journal(&s.bodies);
    let env = Environment::new(&s, &expected);
    assert!(replay_common_case_v2(&shortened, &shortened.links(), &env, replay_limits()).is_err());
    let missing_aux = Journal::new(&s, s.bodies.iter().map(|b| (CaseJournalEventKind::NativeBody, b.clone())).collect(), false);
    let env = Environment::new(&s, &missing_aux);
    assert!(matches!(
        replay_common_case_v2(&missing_aux, &missing_aux.links(), &env, replay_limits()),
        Err(CommonReplayErrorV2::Contract(_))
    ));
    assert_eq!(env.external_calls.get(), 0);
}

#[test]
fn unsupported_source_and_native_settlement_ports_refuse_without_any_live_commit() {
    let s = Scenario::new(false);
    for kind in [CaseJournalEventKind::NativeBody, CaseJournalEventKind::NativeTransaction] {
        let payload = if kind == CaseJournalEventKind::NativeBody {
            native_body(PositiveNativeOperationV2::SourceResponse {
                case: s.result.case_id(),
                request: RequestId(d(90)),
                response: vec![1],
            })
        } else {
            b"unknown-actual-native-transaction-input".to_vec()
        };
        let j = Journal::new(&s, vec![(CaseJournalEventKind::NativeBody, s.bodies[0].clone()), (kind, payload)], true);
        let env = Environment::new(&s, &j);
        assert!(matches!(replay_common_case_v2(&j, &j.links(), &env, replay_limits()), Err(CommonReplayErrorV2::Environment(_))));
        assert_eq!(env.external_calls.get(), 2);
        assert_eq!(env.live_commits.get(), 0);
        assert_eq!(env.final_calls.get(), 0);
    }
}

#[test]
fn current_clock_expiry_and_final_generation_or_clock_changes_are_rechecked() {
    let s = Scenario::new(false);
    let j = s.journal(&s.bodies[..3]);
    let env = Environment::new(&s, &j);
    env.now.set(100);
    let replay = replay_common_case_v2(&j, &j.links(), &env, replay_limits()).unwrap();
    assert_eq!(replay.historical_court().status, ClaimStatus::Pending);
    assert_eq!(replay.current_court().status, ClaimStatus::EndedUnverified);
    assert!(replay.facts().is_empty());
    assert_eq!(replay.events().len(), 3);
    assert_eq!(replay.observed_daa(), 100);
    assert_eq!(replay.pin(), &j.pin);
    for drift in [false, true] {
        let mut env = Environment::new(&s, &j);
        env.fail_final = !drift;
        env.drift_final = drift;
        let result = replay_common_case_v2(&j, &j.links(), &env, replay_limits());
        assert!(if drift {
            matches!(result, Err(CommonReplayErrorV2::Clock))
        } else {
            matches!(result, Err(CommonReplayErrorV2::Environment(_)))
        });
        assert_eq!(env.final_calls.get(), 1);
        assert_eq!(env.live_commits.get(), 0);
    }
}

#[test]
fn current_historical_frame_role_is_rechecked_under_the_step_guard() {
    let s = Scenario::new(false);
    let j = s.journal(&s.bodies);
    let mut env = Environment::new(&s, &j);
    env.deny_role_at = Some(3);
    assert!(matches!(replay_common_case_v2(&j, &j.links(), &env, replay_limits()), Err(CommonReplayErrorV2::Court(_))));
    assert_eq!(env.external_calls.get(), 2);
    assert!(env.role_calls.get() >= 2);
    assert_eq!(env.active.get(), None);
    assert_eq!(env.live_commits.get(), 0);
}

#[test]
fn bounded_budget_refusal_precedes_native_authentication_and_external_callbacks() {
    let s = Scenario::new(false);
    let j = s.journal(&s.bodies);
    let links = j.links();
    for too_small in [true, false] {
        let mut env = Environment::new(&s, &j);
        let mut bounded = replay_limits();
        if too_small {
            bounded.max_known_buffer_bytes = 1;
        } else {
            env.quote_work = u64::MAX;
        }
        assert!(matches!(replay_common_case_v2(&j, &links, &env, bounded), Err(CommonReplayErrorV2::Budget)));
        assert_eq!(env.native_calls.get(), 0);
        assert_eq!(env.external_calls.get(), 0);
        assert_eq!(env.final_calls.get(), 0);
        assert_eq!(env.live_commits.get(), 0);
    }
}
