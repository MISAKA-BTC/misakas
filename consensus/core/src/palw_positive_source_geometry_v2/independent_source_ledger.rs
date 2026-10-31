//! Independent data/finite-ledger checks. The fixtures below are not native
//! admission, selected-history, role, funding or reservation capabilities.
use super::*;
use kaspa_consensus_core::palw_positive_source_v2::{
    NativeSourceContextV2, NativeSourceLedgerV2, SourceEffectV2, SourceErrorV2, SourceEventCostV2, SourceLimitsV2,
    SourceMissingPortV2, SourceRequestEventV2, SourceResponseEventV2, SourceServiceFeeV2, history::NativeSourceHistoryV2,
};
use misaka_palw_positive_contract::{
    identity::{FundingScopeId, PUBKEY_BYTES, PlanId, REQUEST_SIGN_CONTEXT, RequestId, ResultId, SIGNATURE_BYTES, principal},
    messages::{AggregateBudget, CheckProfile, ClaimStatus, CoverageDescriptor, MinerDataStatus, MinerServiceContract, WitnessStatus},
    rows::{AcceptedMaterial, AcceptedOrigin, AggregateUsage, CaseRow, NativeHash64},
};

fn real_artifact_fixture() -> (Fixture, Vec<PalwArtifactOperandV1>) {
    let values = operands();
    let artifact = artifact_root_v1(&values.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
    (Fixture::new(4, 65, PalwPromptIdsFormV1::MerkleV1, false, artifact), values)
}

#[test]
fn independent_all_actual_g14_bodies_replay_with_raw_and_wire_dimensions_separate() {
    let (f, values) = real_artifact_fixture();
    let geometry = f.geometry(limits()).unwrap();
    let request = f.request(SourceKind::ArtifactInventory, 0, 4);
    let mut raw_total = 0;
    let mut wire_total = 0;
    for index in 0..4 {
        let opening = open_artifact_leaf_v1(&values, index).unwrap();
        let bytes = borsh::to_vec(&opening).unwrap();
        assert_eq!(bytes.len(), 166);
        let (decoded_index, unit) = geometry.decode_g14_response_v2(&request, &bytes).unwrap();
        assert_eq!(decoded_index, u64::from(index));
        assert_eq!(unit, StoredUnitV2::Artifact(opening));
        let (raw, wire) = unit.sizes().unwrap();
        assert_eq!((raw, wire), (4, 166));
        raw_total += raw;
        wire_total += wire;
    }
    assert_eq!((raw_total, wire_total), (16, 664));
    let request = f.request(SourceKind::Input, 0, 3);
    for (index, position) in [(0, 0), (1, 32), (2, 64)] {
        let opening = prompt_ids_opening_v1(&f.prompt, position).unwrap();
        let bytes = borsh::to_vec(&opening).unwrap();
        let (decoded_index, unit) = geometry.decode_g14_response_v2(&request, &bytes).unwrap();
        assert_eq!(decoded_index, index);
        assert_eq!(unit, StoredUnitV2::Input(opening));
    }
}

#[test]
fn independent_opaque_g14_lengths_and_trailing_material_cannot_allocate_or_authenticate() {
    let (f, values) = real_artifact_fixture();
    let geometry = f.geometry(limits()).unwrap();
    let request = f.request(SourceKind::ArtifactInventory, 0, 1);
    let valid = borsh::to_vec(&open_artifact_leaf_v1(&values, 0).unwrap()).unwrap();
    let mut attacks = vec![u32::MAX.to_le_bytes().to_vec()];
    // Borsh artifact: name length/name, None layer, row_start, data length,
    // data, leaf_index, leaf_count, path length, and literal siblings.
    for offset in [0, 18, 34] {
        let mut bad = valid.clone();
        bad[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        attacks.push(bad);
    }
    let mut bad = valid.clone();
    bad[13] = 2; // invalid Option tag
    attacks.push(bad);
    let mut bad = valid.clone();
    bad[22] ^= 1; // authentic data/root disagreement
    attacks.push(bad);
    let mut bad = valid.clone();
    bad[26..30].copy_from_slice(&1u32.to_le_bytes()); // authentic unit outside request
    attacks.push(bad);
    let mut bad = valid.clone();
    bad.push(0); // complete valid opening plus opaque suffix
    attacks.push(bad);
    attacks.push(valid[..valid.len() - 1].to_vec());
    for bytes in attacks {
        let outcome = std::panic::catch_unwind(|| geometry.decode_g14_response_v2(&request, &bytes));
        assert!(outcome.is_ok());
        assert!(outcome.unwrap().is_err());
    }
    let request = f.request(SourceKind::Input, 0, 3);
    let opening = prompt_ids_opening_v1(&f.prompt, 0).unwrap();
    let mut bytes = borsh::to_vec(&opening).unwrap();
    bytes[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(geometry.decode_g14_response_v2(&request, &bytes).is_err());
}

#[test]
fn independent_real_opening_does_not_transfer_between_cases_sources_or_ranges() {
    let (f, values) = real_artifact_fixture();
    let geometry = f.geometry(limits()).unwrap();
    let request = f.request(SourceKind::ArtifactInventory, 0, 1);
    let body = borsh::to_vec(&open_artifact_leaf_v1(&values, 0).unwrap()).unwrap();
    for attack in 0..5 {
        let mut substituted = request;
        match attack {
            0 => substituted.case.0 = d(80),
            1 => substituted.contract = d(81),
            2 => substituted.source = d(82),
            3 => {
                substituted.first = 1;
                substituted.end = 2;
            }
            _ => substituted.kind = SourceKind::Input,
        }
        assert!(geometry.decode_g14_response_v2(&substituted, &body).is_err(), "substitution {attack}");
    }
}

#[test]
fn independent_flat_g14_is_one_whole_prompt_and_cannot_cross_merkle_or_held_forms() {
    let (merkle, _) = real_artifact_fixture();
    let flat = Fixture::new(4, 65, PalwPromptIdsFormV1::Flat, false, merkle.artifact);
    let geometry = flat.geometry(limits()).unwrap();
    let request = flat.request(SourceKind::Input, 0, 1);
    let bytes = borsh::to_vec(&flat.prompt).unwrap();
    let (index, decoded) = geometry.decode_g14_response_v2(&request, &bytes).unwrap();
    assert_eq!(index, 0);
    assert_eq!(decoded, StoredUnitV2::FlatInput(flat.prompt.clone()));
    assert_eq!(decoded.sizes().unwrap(), (260, 264));
    let mut forged = bytes.clone();
    forged[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(geometry.decode_g14_response_v2(&request, &forged).is_err());
    let merkle_geometry = merkle.geometry(limits()).unwrap();
    assert!(merkle_geometry.decode_g14_response_v2(&merkle.request(SourceKind::Input, 0, 3), &bytes).is_err());
    let tile = borsh::to_vec(&prompt_ids_opening_v1(&merkle.prompt, 0).unwrap()).unwrap();
    assert!(geometry.decode_g14_response_v2(&request, &tile).is_err());
    let held = Fixture::new(4, 65, PalwPromptIdsFormV1::Flat, true, merkle.artifact);
    let held_geometry = held.geometry(limits()).unwrap();
    assert_eq!(held_geometry.input_form(), PalwPromptIdsFormV1::MerkleV1);
    assert!(held_geometry.decode_g14_response_v2(&held.request(SourceKind::Input, 0, 3), &bytes).is_err());
}

fn source_limits() -> SourceLimitsV2 {
    SourceLimitsV2 {
        max_requests: 16,
        max_responses: 16,
        max_history_events: 32,
        max_history_bytes: 1 << 20,
        max_intervals: 16,
        max_units_per_request: 8,
        max_native_body_bytes: 8192,
    }
}
fn service_budget() -> AggregateBudget {
    AggregateBudget {
        rounds: 16,
        unique_source_bytes: 1024,
        transmitted_bytes: 1 << 20,
        additional_work: 1024,
        retained_bytes: 1 << 20,
        retention_until: 100,
        fee: 1024,
        witness_attempts: 1,
    }
}
fn accepted_material(sequence: usize) -> AcceptedMaterial {
    let transaction = NativeHash64([40 + sequence as u8; 64]);
    AcceptedMaterial {
        origin: AcceptedOrigin {
            accepting_block: NativeHash64([20 + sequence as u8; 64]),
            carrier_block: NativeHash64([100 + sequence as u8; 64]),
            carrier_transaction: transaction,
            accepted_tx_ordinal: sequence as u32,
            event_ordinal: 0,
            accepted_daa: 10 + sequence as u64,
            accepted_blue_score: 20 + sequence as u64,
        },
        digest: Digest(transaction.0),
    }
}
struct LiteralRequest {
    request: SourceRequest,
    requester: PrincipalId,
    key: [u8; PUBKEY_BYTES],
    signature: [u8; SIGNATURE_BYTES],
    material: AcceptedMaterial,
    body: Vec<u8>,
}
impl LiteralRequest {
    fn new(request: SourceRequest, sequence: usize, panel: u8) -> Self {
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([panel; 32]);
        let key = *pair.verification_key.as_ref();
        let requester = principal(&key);
        let signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &request.signing_id(requester).0, REQUEST_SIGN_CONTEXT, [0; 32])
                .unwrap()
                .as_ref();
        let mut body = vec![2, 0, 1, 0, 3];
        body.extend_from_slice(&borsh::to_vec(&request).unwrap());
        body.extend_from_slice(&borsh::to_vec(&requester).unwrap());
        body.extend_from_slice(&signature);
        assert_eq!(body.len(), 4909);
        Self { request, requester, key, signature, material: accepted_material(sequence), body }
    }
    fn event(&self) -> SourceRequestEventV2<'_> {
        SourceRequestEventV2 {
            request: self.request,
            requester: self.requester,
            key: &self.key,
            signature: &self.signature,
            material: self.material,
            native_body: &self.body,
        }
    }
}
struct LiteralResponse {
    request: RequestId,
    provider: PrincipalId,
    material: AcceptedMaterial,
    body: Vec<u8>,
}
impl LiteralResponse {
    fn new(request: SourceRequest, provider: PrincipalId, raw: &[u8], sequence: usize) -> Self {
        let mut body = vec![2, 0, 1, 0, 4];
        body.extend_from_slice(&request.case.0.0);
        body.extend_from_slice(&request.id().0.0);
        body.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        assert_eq!(body.len(), 137);
        body.extend_from_slice(raw);
        Self { request: request.id(), provider, material: accepted_material(sequence), body }
    }
    fn event(&self) -> SourceResponseEventV2<'_> {
        SourceResponseEventV2 {
            request: self.request,
            provider: self.provider,
            material: self.material,
            native_body: &self.body,
            g14_data_range: (137, self.body.len() as u64),
        }
    }
}
/// Exact preloaded test history, deliberately not an implementation of native
/// acceptance. It cannot expose a source duty or install consensus rows.
struct TestLiteralHistory {
    result: ResultId,
    service: Digest,
    ordered: Vec<AcceptedMaterial>,
    requests: Vec<LiteralRequest>,
    responses: Vec<LiteralResponse>,
    missing: Option<SourceMissingPortV2>,
    cutoff: u64,
    fee: SourceServiceFeeV2,
}
impl TestLiteralHistory {
    fn check(&self, result: &MinerResultCommit, service: &MinerServiceContract) -> Result<SourceEventCostV2, SourceErrorV2> {
        if let Some(port) = self.missing {
            return Err(SourceErrorV2::MissingPort(port));
        }
        if result.id() != self.result || service.id() != self.service {
            return Err(SourceErrorV2::Binding);
        }
        Ok(SourceEventCostV2 { additional_work: 2, outer_carrier_bytes: 7, cutoff: self.cutoff, service_fee: 0 })
    }
    fn add_request(&mut self, request: SourceRequest, panel: u8) {
        let literal = LiteralRequest::new(request, self.ordered.len(), panel);
        self.ordered.push(literal.material);
        self.requests.push(literal);
    }
    fn add_response(&mut self, request: SourceRequest, provider: PrincipalId, raw: &[u8]) {
        let literal = LiteralResponse::new(request, provider, raw, self.ordered.len());
        self.ordered.push(literal.material);
        self.responses.push(literal);
    }
}
impl NativeSourceContextV2 for TestLiteralHistory {
    fn history_complete(
        &self,
        result: &MinerResultCommit,
        history: &NativeSourceHistoryV2,
        next: &AcceptedMaterial,
    ) -> Result<(), SourceErrorV2> {
        if let Some(port) = self.missing {
            return Err(SourceErrorV2::MissingPort(port));
        }
        if result.id() != self.result {
            return Err(SourceErrorV2::Binding);
        }
        let index = self.ordered.iter().position(|m| m == next).ok_or(SourceErrorV2::History)?;
        let observed: Vec<_> = history.entries().iter().map(|e| e.material).collect();
        if observed != self.ordered[..index] && observed != self.ordered[..=index] {
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
        let cost = self.check(result, service)?;
        let literal = self.requests.iter().find(|r| r.material == event.material).ok_or(SourceErrorV2::History)?;
        if literal.request != event.request
            || literal.requester != event.requester
            || &literal.key != event.key
            || &literal.signature != event.signature
            || literal.body != event.native_body
        {
            return Err(SourceErrorV2::Binding);
        }
        Ok(cost)
    }
    fn response(
        &self,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        event: &SourceResponseEventV2<'_>,
    ) -> Result<SourceEventCostV2, SourceErrorV2> {
        let mut cost = self.check(result, service)?;
        let literal = self.responses.iter().find(|r| r.material == event.material).ok_or(SourceErrorV2::History)?;
        if literal.request != event.request
            || literal.provider != event.provider
            || literal.body != event.native_body
            || event.g14_data_range != (137, literal.body.len() as u64)
        {
            return Err(SourceErrorV2::Binding);
        }
        cost.service_fee = self.fee.amount;
        Ok(cost)
    }
    fn service_fee(
        &self,
        result: &MinerResultCommit,
        service: &MinerServiceContract,
        kind: SourceKind,
        source: Digest,
        _unit: u64,
        data_bytes: u64,
    ) -> Result<SourceServiceFeeV2, SourceErrorV2> {
        self.check(result, service)?;
        let expected = match kind {
            SourceKind::ArtifactInventory => result.artifact,
            SourceKind::Input => result.input,
        };
        if source != expected || data_bytes == 0 {
            return Err(SourceErrorV2::Binding);
        }
        Ok(self.fee)
    }
}
fn ledger_fixture(
    budget: AggregateBudget,
) -> (Fixture, Vec<PalwArtifactOperandV1>, TirSourceGeometryV2, NativeSourceLedgerV2, TestLiteralHistory) {
    let (mut f, values) = real_artifact_fixture();
    let service = MinerServiceContract {
        version: CONTRACT_VERSION,
        revision: CONTRACT_REVISION,
        case: f.result.case_id(),
        source_catalog: d(31),
        provider_delegations: d(32),
        budget,
        funding_consent: d(33),
        deadline: 100,
    };
    f.result.service_contract = service.id();
    let geometry = f.geometry(limits()).unwrap();
    let ledger = NativeSourceLedgerV2::new(f.result.clone(), service, AggregateUsage::default(), source_limits()).unwrap();
    let context = TestLiteralHistory {
        result: f.result.id(),
        service: service.id(),
        ordered: vec![],
        requests: vec![],
        responses: vec![],
        missing: None,
        cutoff: 90,
        fee: SourceServiceFeeV2 { amount: 3, schedule: d(210), funding_scope: FundingScopeId(d(211)) },
    };
    (f, values, geometry, ledger, context)
}
fn usage_tuple(ledger: &NativeSourceLedgerV2) -> (u64, u64, u64, u64, u64, u64, u32) {
    let u = ledger.usage();
    (u.rounds, u.unique_source_bytes, u.transmitted_bytes, u.additional_work, u.retained_bytes, u.fee, u.witness_attempts)
}

#[test]
fn independent_overlapping_panels_share_units_and_literal_prior_request_receipts() {
    let (f, values, geometry, mut ledger, mut context) = ledger_fixture(service_budget());
    let a = f.request(SourceKind::ArtifactInventory, 0, 3);
    let b = f.request(SourceKind::ArtifactInventory, 1, 4);
    context.add_request(a, 71);
    let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    ledger.apply(update).unwrap();
    assert_eq!(ledger.usage().unique_source_bytes, 12);
    context.add_request(b, 72);
    let update = ledger.prepare_request(&geometry, &context, context.requests[1].event()).unwrap();
    ledger.apply(update).unwrap();
    assert_eq!(ledger.usage().unique_source_bytes, 16);
    assert_ne!(context.requests[0].requester, context.requests[1].requester);
    assert_eq!(ledger.status(a.kind, a.source, 1), MinerDataStatus::Pending);
    let raw = borsh::to_vec(&open_artifact_leaf_v1(&values, 1).unwrap()).unwrap();
    context.add_response(a, f.result.miner, &raw);
    let update = ledger.prepare_response(&geometry, &context, context.responses[0].event()).unwrap();
    ledger.apply(update).unwrap();
    let receipt = ledger.receipt(a.kind, a.source, 1).unwrap();
    assert_eq!(receipt.duty, context.requests[0].material);
    assert_eq!(receipt.response, context.responses[0].material);
    assert_eq!(receipt.result, f.result.id());
    assert_eq!(receipt.provider, f.result.miner);
    assert_eq!(receipt.bytes, 4);
    assert_eq!(receipt.settlement, None);
    // The completed unit is scoped to the source/case, so B reuses the
    // literally retained A response despite having a different RequestId.
    assert_ne!(a.id(), b.id());
    assert_eq!(ledger.status(b.kind, b.source, 1), MinerDataStatus::Complete);
    assert_eq!(ledger.status(b.kind, b.source, 3), MinerDataStatus::Pending);
    assert_eq!(ledger.usage().unique_source_bytes, 16);
    assert_eq!(ledger.usage().transmitted_bytes, 2 * 4909 + 137 + 166 + 3 * 7);
    assert_eq!(ledger.usage().retained_bytes, 2 * 4909 + 137 + 166);
    assert_eq!(ledger.usage().fee, 3, "the first literal receipt records its once-only due");

    assert_eq!(ledger.history_events().len(), 3);
    for literal in &context.requests {
        assert_eq!(ledger.history().resolve(&literal.material).unwrap(), literal.body.as_slice());
    }
    assert_eq!(ledger.history().resolve(&context.responses[0].material).unwrap(), context.responses[0].body.as_slice());
    let mut forged_material = context.responses[0].material;
    forged_material.digest = d(230);
    assert!(ledger.history().resolve(&forged_material).is_err());

    let (_, _, replay_geometry, mut replay, _) = ledger_fixture(service_budget());
    for literal in &context.requests {
        let update = replay.prepare_request(&replay_geometry, &context, literal.event()).unwrap();
        replay.apply(update).unwrap();
    }
    let update = replay.prepare_response(&replay_geometry, &context, context.responses[0].event()).unwrap();
    replay.apply(update).unwrap();
    assert_eq!(usage_tuple(&replay), usage_tuple(&ledger));
    assert_eq!(replay.receipt(a.kind, a.source, 1), ledger.receipt(a.kind, a.source, 1));
}

#[test]
fn independent_every_locally_owned_budget_dimension_rejects_without_partial_request_state() {
    for dimension in 0..5 {
        let mut budget = service_budget();
        match dimension {
            0 => budget.rounds = 0,
            1 => budget.unique_source_bytes = 11,
            2 => budget.transmitted_bytes = 4908,
            3 => budget.additional_work = 1,
            _ => budget.retained_bytes = 4908,
        }
        let (f, _, geometry, ledger, mut context) = ledger_fixture(budget);
        let request = f.request(SourceKind::ArtifactInventory, 0, 3);
        context.add_request(request, 71);
        let before = usage_tuple(&ledger);
        assert!(ledger.prepare_request(&geometry, &context, context.requests[0].event()).is_err(), "dimension {dimension}");
        assert_eq!(usage_tuple(&ledger), before);
        assert!(ledger.request(request.id()).is_none());
        assert!(ledger.receipt(request.kind, request.source, 0).is_none());
        assert_eq!(ledger.status(request.kind, request.source, 0), MinerDataStatus::NotRequested);
    }
}

#[test]
fn independent_prepared_updates_cannot_cross_cache_instances_or_older_revisions() {
    let (f, _, geometry, mut ledger, mut context) = ledger_fixture(service_budget());
    context.add_request(f.request(SourceKind::ArtifactInventory, 0, 2), 71);
    let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    let (_, _, _, mut identical_cache, _) = ledger_fixture(service_budget());
    let untouched = usage_tuple(&identical_cache);
    assert!(identical_cache.apply(update).is_err(), "equal DTO/revision must not authorize a cross-cache apply");
    assert_eq!(usage_tuple(&identical_cache), untouched);
    assert_eq!(usage_tuple(&ledger), untouched);
    let first = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    let stale = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    ledger.apply(first).unwrap();
    let before = usage_tuple(&ledger);
    assert!(ledger.apply(stale).is_err());
    assert_eq!(usage_tuple(&ledger), before);
}

#[test]
fn independent_missing_native_ports_do_not_become_successful_prepared_obligations() {
    for port in [
        SourceMissingPortV2::AcceptedHistory,
        SourceMissingPortV2::CanonicalHistory,
        SourceMissingPortV2::DrawnRole,
        SourceMissingPortV2::LiveFunding,
        SourceMissingPortV2::Reservation,
        SourceMissingPortV2::FeeSchedule,
        SourceMissingPortV2::NativeApply,
    ] {
        let (f, _, geometry, ledger, mut context) = ledger_fixture(service_budget());
        context.add_request(f.request(SourceKind::ArtifactInventory, 0, 2), 71);
        context.missing = Some(port);
        let before = usage_tuple(&ledger);
        let error = ledger.prepare_request(&geometry, &context, context.requests[0].event()).err().unwrap();
        assert_eq!(error, SourceErrorV2::MissingPort(port));
        assert_eq!(usage_tuple(&ledger), before);
    }
}

#[test]
fn independent_malformed_accepted_opaque_responses_do_not_publish_receipts_or_costs() {
    for attack in 0..7 {
        let (f, values, geometry, mut ledger, mut context) = ledger_fixture(service_budget());
        let request = f.request(SourceKind::ArtifactInventory, 0, 1);
        context.add_request(request, 71);
        let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
        ledger.apply(update).unwrap();
        let mut raw = borsh::to_vec(&open_artifact_leaf_v1(&values, 0).unwrap()).unwrap();
        match attack {
            0 => raw[..4].copy_from_slice(&u32::MAX.to_le_bytes()),
            1 => raw[18..22].copy_from_slice(&u32::MAX.to_le_bytes()),
            2 => raw[34..38].copy_from_slice(&u32::MAX.to_le_bytes()),
            3 => raw[22] ^= 1,
            4 => raw = borsh::to_vec(&open_artifact_leaf_v1(&values, 1).unwrap()).unwrap(),
            5 => raw.push(0),
            _ => {
                raw.pop();
            }
        }
        context.add_response(request, f.result.miner, &raw);
        let before = usage_tuple(&ledger);
        let result = ledger.prepare_response(&geometry, &context, context.responses[0].event());
        assert!(result.is_err(), "malformed response {attack}");
        assert_eq!(usage_tuple(&ledger), before);
        assert!(ledger.receipt(request.kind, request.source, 0).is_none());
        assert_eq!(ledger.status(request.kind, request.source, 0), MinerDataStatus::Pending);
    }
}

#[test]
fn independent_canonical_history_omission_and_exclusive_clock_are_refused_atomically() {
    let (f, _, geometry, ledger, mut context) = ledger_fixture(service_budget());
    context.add_request(f.request(SourceKind::ArtifactInventory, 0, 1), 71);
    context.add_request(f.request(SourceKind::ArtifactInventory, 1, 2), 72);
    let before = usage_tuple(&ledger);
    assert!(ledger.prepare_request(&geometry, &context, context.requests[1].event()).is_err(), "omitted earlier canonical op3");
    assert_eq!(usage_tuple(&ledger), before);
    for cutoff in [9, 10, 101] {
        context.cutoff = cutoff;
        assert!(ledger.prepare_request(&geometry, &context, context.requests[0].event()).is_err(), "invalid cutoff {cutoff}");
        assert_eq!(usage_tuple(&ledger), before);
    }
    context.cutoff = 90;
    // Even an accepted material-shaped object must carry the source-specific
    // literal carrier TxId, rather than a substituted request ID or digest.
    context.requests[0].material.digest = d(201);
    context.ordered[0] = context.requests[0].material;
    assert!(ledger.prepare_request(&geometry, &context, context.requests[0].event()).is_err());
    assert_eq!(usage_tuple(&ledger), before);
}

#[test]
fn independent_identical_ranges_from_other_panels_preserve_first_duty_and_once_only_receipt() {
    let mut budget = service_budget();
    budget.rounds = 1;
    budget.unique_source_bytes = 4;
    let (f, values, geometry, mut ledger, mut context) = ledger_fixture(budget);
    let request = f.request(SourceKind::ArtifactInventory, 0, 1);
    for panel in [71, 72] {
        context.add_request(request, panel);
        let update = ledger.prepare_request(&geometry, &context, context.requests.last().unwrap().event()).unwrap();
        ledger.apply(update).unwrap();
    }
    assert_eq!(ledger.usage().rounds, 1);
    assert_eq!(ledger.usage().unique_source_bytes, 4);
    assert_eq!(ledger.request(request.id()).unwrap().material, context.requests[0].material);
    let raw = borsh::to_vec(&open_artifact_leaf_v1(&values, 0).unwrap()).unwrap();
    context.add_response(request, f.result.miner, &raw);
    let update = ledger.prepare_response(&geometry, &context, context.responses[0].event()).unwrap();
    ledger.apply(update).unwrap();
    let first = *ledger.receipt(request.kind, request.source, 0).unwrap();
    assert_eq!(first.duty, context.requests[0].material);
    assert_eq!(first.settlement, None);
    let before = usage_tuple(&ledger);
    // Exact accepted event replay is inert and does not charge transmission.
    let update = ledger.prepare_response(&geometry, &context, context.responses[0].event()).unwrap();
    ledger.apply(update).unwrap();
    assert_eq!(usage_tuple(&ledger), before);
    // A new accepted op4 is retained/metered, but cannot replace the original
    // literal receipt or manufacture a second completion/settlement.
    context.add_response(request, f.result.miner, &raw);
    let update = ledger.prepare_response(&geometry, &context, context.responses[1].event()).unwrap();
    ledger.apply(update).unwrap();
    assert_eq!(ledger.receipt(request.kind, request.source, 0), Some(&first));
    assert_eq!(ledger.source_union(request.kind, request.source).unwrap().completed_units, 1);
    assert_eq!(ledger.usage().unique_source_bytes, 4);
    assert_eq!(ledger.usage().rounds, 1);
    assert_eq!(ledger.usage().transmitted_bytes - before.2, 137 + 166 + 7);
}

#[test]
fn independent_exact_budget_boundaries_pass_but_source_summaries_cannot_restore_discounts() {
    let mut budget = service_budget();
    budget.rounds = 1;
    budget.unique_source_bytes = 12;
    budget.transmitted_bytes = 4909 + 7;
    budget.additional_work = 2;
    budget.retained_bytes = 4909;
    let (f, _, geometry, mut ledger, mut context) = ledger_fixture(budget);
    context.add_request(f.request(SourceKind::ArtifactInventory, 0, 3), 71);
    let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    ledger.apply(update).unwrap();
    assert_eq!(usage_tuple(&ledger), (1, 12, 4916, 2, 4909, 0, 0));
    let service = *ledger.service();
    let usage = ledger.usage();
    let error = NativeSourceLedgerV2::new(f.result.clone(), service, usage, source_limits()).err().unwrap();
    assert_eq!(error, SourceErrorV2::MissingPort(SourceMissingPortV2::CanonicalHistory));
    let empty = NativeSourceLedgerV2::new(f.result.clone(), service, AggregateUsage::default(), source_limits()).unwrap();
    assert_eq!(empty.status(SourceKind::ArtifactInventory, f.result.artifact, 0), MinerDataStatus::NotRequested);
    assert!(empty.receipt(SourceKind::ArtifactInventory, f.result.artifact, 0).is_none());
}

#[test]
fn independent_fresh_fee_reservation_is_atomic_and_first_receipt_creates_only_one_due() {
    let mut too_small = service_budget();
    too_small.fee = 2;
    let (f, _, geometry, ledger, mut context) = ledger_fixture(too_small);
    context.add_request(f.request(SourceKind::ArtifactInventory, 0, 1), 71);
    assert!(ledger.prepare_request(&geometry, &context, context.requests[0].event()).is_err());
    assert_eq!(usage_tuple(&ledger), (0, 0, 0, 0, 0, 0, 0));
    assert!(ledger.source_union(SourceKind::ArtifactInventory, f.result.artifact).is_none());

    let mut exact = service_budget();
    exact.fee = 3;
    let (f, values, geometry, mut ledger, mut context) = ledger_fixture(exact);
    let request = f.request(SourceKind::ArtifactInventory, 0, 1);
    context.add_request(request, 71);
    let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    assert!(update.fee_intent().is_none(), "a reservation is not a fee due/payment");
    ledger.apply(update).unwrap();
    assert_eq!(ledger.usage().fee, 0);
    let raw = borsh::to_vec(&open_artifact_leaf_v1(&values, 0).unwrap()).unwrap();
    for index in 0..2 {
        context.add_response(request, f.result.miner, &raw);
        let update = ledger.prepare_response(&geometry, &context, context.responses[index].event()).unwrap();
        assert_eq!(update.fee_intent().is_some(), index == 0);
        if let Some(intent) = update.fee_intent() {
            assert_eq!(intent.result, f.result.id());
            assert_eq!(intent.provider, f.result.miner);
            assert_eq!(intent.amount, 3);
            assert_eq!(intent.schedule, context.fee.schedule);
            assert_eq!(intent.funding_scope, context.fee.funding_scope);
            assert_eq!(intent.accepted_response, context.responses[index].material);
            assert_eq!(
                intent.key,
                misaka_palw_positive_contract::native::NativeRowKey::ServiceReceipt {
                    case: request.case,
                    kind: request.kind,
                    source: request.source,
                    unit: 0,
                }
            );
        }
        ledger.apply(update).unwrap();
        assert_eq!(ledger.usage().fee, 3);
        assert_eq!(ledger.receipt(request.kind, request.source, 0).unwrap().settlement, None);
    }
    context.add_request(f.request(SourceKind::ArtifactInventory, 1, 2), 72);
    let before = usage_tuple(&ledger);
    assert!(
        ledger.prepare_request(&geometry, &context, context.requests[1].event()).is_err(),
        "next unit exceeds reserved/due fee cap"
    );
    assert_eq!(usage_tuple(&ledger), before);
}

#[test]
fn independent_response_cannot_replace_a_reserved_fee_amount_schedule_or_funding_scope() {
    for attack in 0..3 {
        let (f, values, geometry, mut ledger, mut context) = ledger_fixture(service_budget());
        let request = f.request(SourceKind::ArtifactInventory, 0, 1);
        context.add_request(request, 71);
        let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
        ledger.apply(update).unwrap();
        match attack {
            0 => context.fee.amount += 1,
            1 => context.fee.schedule = d(212),
            _ => context.fee.funding_scope = FundingScopeId(d(213)),
        }
        let raw = borsh::to_vec(&open_artifact_leaf_v1(&values, 0).unwrap()).unwrap();
        context.add_response(request, f.result.miner, &raw);
        let before = usage_tuple(&ledger);
        assert!(ledger.prepare_response(&geometry, &context, context.responses[0].event()).is_err(), "fee substitution {attack}");
        assert_eq!(usage_tuple(&ledger), before);
        assert!(ledger.receipt(request.kind, request.source, 0).is_none());
    }
}

fn current_case_fixture(ledger: &NativeSourceLedgerV2) -> CaseRow {
    let result = ledger.result();
    CaseRow {
        case: result.case_id(),
        result: result.id(),
        service: *ledger.service(),
        coverage: CoverageDescriptor {
            scope: result.scope,
            result: result.id(),
            plan: PlanId(d(220)),
            profile: CheckProfile::ExactStreamV1,
            mandatory_relations: 1,
            reserved_elements: 4,
            reserved_terms: 4,
            max_part_bytes: 1024,
        },
        status: ClaimStatus::Pending,
        miner_data: ledger.miner_data_status(),
        witness_status: WitnessStatus::Building,
        positive_witness: None,
        usage: ledger.usage(),
        reservation: accepted_material(30),
        liability: accepted_material(31),
        maturity_daa: 100,
        unresolved_fraud: 0,
        last_event: accepted_material(0).origin,
    }
}

#[test]
fn independent_case_refresh_preserves_shared_usage_and_invalidates_prepared_rows() {
    let (f, _, geometry, mut ledger, mut context) = ledger_fixture(service_budget());
    let original = current_case_fixture(&ledger);
    ledger.bind_case(&original).unwrap();
    context.add_request(f.request(SourceKind::ArtifactInventory, 0, 1), 71);
    let stale = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    assert_eq!(stale.expected_case(), Some(&original));
    assert_eq!(stale.case_row().unwrap().miner_data, MinerDataStatus::Pending);
    let mut external = original;
    external.usage.additional_work = 20;
    external.usage.witness_attempts = 1;
    external.witness_status = WitnessStatus::Rejected;
    external.last_event = accepted_material(32).origin;
    ledger.refresh_case_usage(&external).unwrap();
    let refreshed = usage_tuple(&ledger);
    assert!(ledger.validate_prepared(&stale).is_err());
    assert!(ledger.apply(stale).is_err());
    assert_eq!(usage_tuple(&ledger), refreshed);
    let fresh = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    assert_eq!(fresh.expected_case(), Some(&external));
    let proposed = *fresh.case_row().unwrap();
    assert_eq!(proposed.usage.additional_work, 22);
    assert_eq!(proposed.usage.fee, 0);
    assert_eq!(proposed.usage.witness_attempts, 1);
    assert_eq!(proposed.witness_status, WitnessStatus::Rejected);
    assert_eq!(proposed.last_event, context.requests[0].material.origin);
    ledger.apply(fresh).unwrap();
    assert_eq!(ledger.usage(), proposed.usage);
    // A later witness failure cannot erase the separately pending source duty.
    assert_eq!(ledger.miner_data_status(), MinerDataStatus::Pending);
    let mut rollback = proposed;
    rollback.usage.unique_source_bytes = 0;
    assert!(ledger.refresh_case_usage(&rollback).is_err());
    assert_eq!(ledger.usage(), proposed.usage);
    let mut other = proposed;
    other.result = ResultId(d(222));
    assert!(ledger.refresh_case_usage(&other).is_err());
    let mut other = proposed;
    other.coverage.plan = PlanId(d(223));
    assert!(ledger.refresh_case_usage(&other).is_err());
}

#[test]
fn independent_case_refresh_rejects_fee_usage_that_overbooks_a_pending_unit_reservation() {
    let mut budget = service_budget();
    budget.fee = 3;
    let (f, _, geometry, mut ledger, mut context) = ledger_fixture(budget);
    let initial = current_case_fixture(&ledger);
    ledger.bind_case(&initial).unwrap();
    context.add_request(f.request(SourceKind::ArtifactInventory, 0, 1), 71);
    let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    let mut actual = *update.case_row().unwrap();
    ledger.apply(update).unwrap();
    let before = usage_tuple(&ledger);
    actual.usage.fee = 1; // leaves only 2 although the pending unit reserved 3
    assert!(ledger.refresh_case_usage(&actual).is_err(), "refresh overbooked an already-admitted unit fee");
    assert_eq!(usage_tuple(&ledger), before);
}

#[test]
fn independent_native_source_history_refuses_noncanonical_origins_even_with_matching_test_context() {
    for attack in 0..5 {
        let (f, _, geometry, mut ledger, mut context) = ledger_fixture(service_budget());
        context.add_request(f.request(SourceKind::ArtifactInventory, 0, 1), 71);
        let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
        ledger.apply(update).unwrap();
        context.add_request(f.request(SourceKind::ArtifactInventory, 1, 2), 72);
        let first = context.requests[0].material.origin;
        let origin = &mut context.requests[1].material.origin;
        match attack {
            0 => origin.accepting_block = first.accepting_block, // same block with inconsistent clocks
            1 => origin.accepted_blue_score = first.accepted_blue_score, // later block must advance blue score
            2 => origin.accepted_daa = first.accepted_daa - 1,
            3 => {
                // same clocks/block, non-increasing accepted ordinal
                origin.accepting_block = first.accepting_block;
                origin.accepted_daa = first.accepted_daa;
                origin.accepted_blue_score = first.accepted_blue_score;
                origin.accepted_tx_ordinal = first.accepted_tx_ordinal;
            }
            _ => origin.event_ordinal = 1, // existing source op3/op4 have one operation
        }
        context.ordered[1] = context.requests[1].material;
        let before = usage_tuple(&ledger);
        assert!(ledger.prepare_request(&geometry, &context, context.requests[1].event()).is_err(), "origin attack {attack}");
        assert_eq!(usage_tuple(&ledger), before);
        assert_eq!(ledger.history_events().len(), 1);
    }
}

#[test]
fn independent_first_receipt_names_the_op4_request_while_unit_completion_and_fee_remain_shared() {
    let (f, values, geometry, mut ledger, mut context) = ledger_fixture(service_budget());
    let a = f.request(SourceKind::ArtifactInventory, 0, 3);
    let b = f.request(SourceKind::ArtifactInventory, 1, 4);
    for (request, panel) in [(a, 71), (b, 72)] {
        context.add_request(request, panel);
        let update = ledger.prepare_request(&geometry, &context, context.requests.last().unwrap().event()).unwrap();
        ledger.apply(update).unwrap();
    }
    let raw = borsh::to_vec(&open_artifact_leaf_v1(&values, 1).unwrap()).unwrap();
    // A reserved this unit first, but the literal op4 references B. The row's
    // duty must resolve to B's accepted request, rather than A's reservation.
    context.add_response(b, f.result.miner, &raw);
    let update = ledger.prepare_response(&geometry, &context, context.responses[0].event()).unwrap();
    assert_eq!(update.receipt().unwrap().duty, context.requests[1].material);
    assert_eq!(update.receipt().unwrap().response, context.responses[0].material);
    assert!(update.fee_intent().is_some());
    ledger.apply(update).unwrap();
    let receipt = *ledger.receipt(b.kind, b.source, 1).unwrap();
    assert_eq!(ledger.status(a.kind, a.source, 1), MinerDataStatus::Complete);
    assert_eq!(ledger.usage().fee, 3);
    assert_eq!(ledger.usage().unique_source_bytes, 16);
    context.add_response(a, f.result.miner, &raw);
    let update = ledger.prepare_response(&geometry, &context, context.responses[1].event()).unwrap();
    assert!(update.receipt().is_none());
    assert!(update.fee_intent().is_none());
    ledger.apply(update).unwrap();
    assert_eq!(ledger.receipt(a.kind, a.source, 1), Some(&receipt));
    assert_eq!(ledger.usage().fee, 3);
    assert_eq!(ledger.source_union(a.kind, a.source).unwrap().completed_units, 1);

    let (_, _, replay_geometry, mut replay, _) = ledger_fixture(service_budget());
    for literal in &context.requests {
        let update = replay.prepare_request(&replay_geometry, &context, literal.event()).unwrap();
        replay.apply(update).unwrap();
    }
    for literal in &context.responses {
        let update = replay.prepare_response(&replay_geometry, &context, literal.event()).unwrap();
        replay.apply(update).unwrap();
    }
    assert_eq!(usage_tuple(&replay), usage_tuple(&ledger));
    assert_eq!(replay.receipt(a.kind, a.source, 1), Some(&receipt));
}

#[test]
fn independent_no_output_updates_still_identify_every_native_row_that_requires_cas() {
    let (f, values, geometry, mut ledger, mut context) = ledger_fixture(service_budget());
    let request = f.request(SourceKind::ArtifactInventory, 0, 1);
    let source = (request.kind, request.source);
    let unit = (request.kind, request.source, 0);
    context.add_request(request, 71);
    let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    assert_eq!(update.touched_source(), source);
    assert_eq!(update.touched_unit(), None);
    ledger.apply(update).unwrap();
    let update = ledger.prepare_request(&geometry, &context, context.requests[0].event()).unwrap();
    assert_eq!(update.effect(), SourceEffectV2::AlreadyApplied);
    assert!(update.union().is_none());
    assert!(update.receipt().is_none());
    assert_eq!(update.touched_source(), source);
    assert_eq!(update.touched_unit(), None);
    ledger.apply(update).unwrap();
    let raw = borsh::to_vec(&open_artifact_leaf_v1(&values, 0).unwrap()).unwrap();
    context.add_response(request, f.result.miner, &raw);
    let update = ledger.prepare_response(&geometry, &context, context.responses[0].event()).unwrap();
    assert_eq!(update.touched_source(), source);
    assert_eq!(update.touched_unit(), Some(unit));
    ledger.apply(update).unwrap();
    let before = usage_tuple(&ledger);
    let update = ledger.prepare_response(&geometry, &context, context.responses[0].event()).unwrap();
    assert_eq!(update.effect(), SourceEffectV2::AlreadyApplied);
    assert!(update.union().is_none());
    assert!(update.receipt().is_none());
    assert_eq!(update.touched_source(), source);
    assert_eq!(update.touched_unit(), Some(unit));
    ledger.apply(update).unwrap();
    assert_eq!(usage_tuple(&ledger), before);
    context.add_response(request, f.result.miner, &raw);
    let update = ledger.prepare_response(&geometry, &context, context.responses[1].event()).unwrap();
    assert_eq!(update.effect(), SourceEffectV2::AlreadyComplete);
    assert!(update.receipt().is_none());
    assert!(update.fee_intent().is_none());
    assert_eq!(update.touched_source(), source);
    assert_eq!(update.touched_unit(), Some(unit));
    ledger.apply(update).unwrap();
    assert_eq!(ledger.usage().fee, 3);
    assert_eq!(ledger.source_union(request.kind, request.source).unwrap().completed_units, 1);
}
