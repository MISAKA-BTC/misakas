//! Readonly input-boundary tests, not native admission tests. AcceptedOrigin
//! values below are the existing A0 row-codec fixture data, used ONLY as signed
//! DTO fields. No test treats them as executed origins, current locks, native
//! registration, replay or P approval; the production bridge always reports
//! these missing owners. There is no new native-state setter.
use super::*;
use kaspa_consensus_core::{
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_route_policy_v1},
    palw_state_v2::{PalwStateCarriageV2, PalwStateParamsV2},
};
use misaka_palw_kernel::rows::LedgerScalarsV1;
use misaka_palw_positive_accounting::credit::ALL_CREDIT_USES;
use misaka_palw_positive_contract::{identity::*, messages::*, native::KERNEL_AUX_TABLE, rows::*};
use std::collections::BTreeMap;
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn origin() -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64([3; 64]),
        accepted_tx_ordinal: 4,
        event_ordinal: 5,
        accepted_daa: 6,
        accepted_blue_score: 7,
    }
}
fn material() -> AcceptedMaterial {
    AcceptedMaterial { origin: origin(), digest: d(8) }
}
fn signed_bundle() -> (ClaimIndexRow, ResultRow, CaseRow, FundingRow, WitnessRow, CoverageRow) {
    use libcrux_ml_dsa::ml_dsa_87::*;
    use misaka_palw_positive_contract::funding::*;
    let mut r = MinerResultCommit {
        version: 2,
        revision: 1,
        scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
        job: d(4),
        execution: d(5),
        miner: PrincipalId(d(6)),
        registered_class: d(7),
        artifact: d(8),
        program: d(9),
        input: d(10),
        tokenizer: d(11),
        execution_rules: d(12),
        selector: d(13),
        generation_seed: [14; 32],
        temperature_q24: 0,
        prefill: 2,
        decode: 3,
        max_context: 8192,
        output: d(15),
        dependencies: d(16),
        service_contract: d(0),
    };
    let miner = generate_key_pair([1; 32]);
    let payer = generate_key_pair([2; 32]);
    let panel = generate_key_pair([3; 32]);
    let policy = d(31);
    let fired_fork = d(32);
    r.scope.ruleset = misaka_palw_positive_contract::native::ruleset_id(fired_fork, policy);
    r.miner = principal(miner.verification_key.as_ref());
    r.dependencies = material().digest;
    let mut service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: r.case_id(),
        source_catalog: d(33),
        provider_delegations: d(34),
        budget: AggregateBudget {
            rounds: 2,
            unique_source_bytes: 4096,
            transmitted_bytes: 8192,
            additional_work: 8,
            retained_bytes: 4096,
            retention_until: 60,
            fee: 5,
            witness_attempts: 2,
        },
        funding_consent: d(0),
        deadline: 50,
    };
    let basis = FundingPlanBasis {
        profile: CheckProfile::ExactStreamV1,
        derivation: d(35),
        mandatory_relations: 10,
        source_catalog: service.source_catalog,
        resource_quote: d(36),
        policy,
        schedule: d(37),
        required_fee: 10,
        required_capital: 20,
    };
    let consent = FundingConsent {
        version: 2,
        revision: 1,
        case: r.case_id(),
        scope: funding_scope(&r, &service, &basis),
        payer: principal(payer.verification_key.as_ref()),
        max_fees: 12,
        max_locked: 100,
        funding_output_index: 1,
        refund_payload: d(38),
        admission_expiry: 40,
    };
    service.funding_consent = consent.id().0;
    r.service_contract = service.id();
    let result = ResultRow {
        miner_signature: *sign(&miner.signing_key, &r.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref(),
        miner_key: *miner.verification_key.as_ref(),
        origin: origin(),
        policy,
        fired_fork,
        funding_scope: consent.scope,
        dependencies: material(),
        role_assignment: material(),
        result: r.clone(),
    };
    let point = NativeOutpoint { transaction: origin().carrier_transaction, index: 1 };
    let leg = |amount| BalanceLeg { initial: amount, paid: 0, refunded: 0, burned: 0, remaining: amount };
    let funding = FundingRow {
        payer_signature: *sign(&payer.signing_key, &consent.id().0.0, FUNDING_SIGN_CONTEXT, [0; 32]).unwrap().as_ref(),
        payer_key: *payer.verification_key.as_ref(),
        consent,
        basis,
        funding_origin: origin(),
        original_outpoint: point,
        current_outpoint: Some(point),
        initial_locked: 35,
        fee: leg(10),
        capital: leg(20),
        surplus: leg(5),
        liability: material(),
        last_settlement: Some(material()),
    };
    let plan = coverage_plan_id(&r, basis.profile, basis.derivation, 10).unwrap();
    let w = PanelWitnessCommit {
        version: 2,
        revision: 1,
        scope: r.scope,
        case: r.case_id(),
        result: r.id(),
        author: principal(panel.verification_key.as_ref()),
        attempt: 0,
        profile: basis.profile,
        plan,
        execution_record: d(40),
        provenance: d(41),
        candidate_output: r.output,
    };
    let witness = WitnessRow {
        panel_signature: *sign(&panel.signing_key, &w.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref(),
        panel_key: *panel.verification_key.as_ref(),
        witness: w.clone(),
        origin: origin(),
        record_header: material(),
        generated_tokens: material(),
        role_assignment: material(),
    };
    let case = CaseRow {
        case: r.case_id(),
        result: r.id(),
        service,
        coverage: CoverageDescriptor {
            scope: r.scope,
            result: r.id(),
            plan,
            profile: basis.profile,
            mandatory_relations: 10,
            reserved_elements: 100,
            reserved_terms: 200,
            max_part_bytes: 10000,
        },
        status: ClaimStatus::PositivePassed,
        miner_data: MinerDataStatus::Pending,
        witness_status: WitnessStatus::Complete,
        positive_witness: Some(w.id()),
        usage: AggregateUsage::default(),
        reservation: material(),
        liability: material(),
        maturity_daa: 100,
        unresolved_fraud: 0,
        last_event: origin(),
    };
    let coverage = CoverageRow {
        witness: w.id(),
        case: r.case_id(),
        result: r.id(),
        plan,
        mandatory_relations: 10,
        next_relation: 10,
        frame_bytes: 1000,
        signatures: 10,
        elements: 100,
        terms: 200,
        state: CoverageState::Matched,
        last_event: origin(),
    };
    (ClaimIndexRow { claim: r.claim_id(), result: r.id(), case: r.case_id() }, result, case, funding, witness, coverage)
}

// Local, already-owned DTO carriage import exercises the existing public state
// read surface. It is NOT a peer import or native acceptance of these rows.
fn dto_state(entries: BTreeMap<(u8, Vec<u8>), Vec<u8>>) -> PalwChainStateV2 {
    let params = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0).unwrap();
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    let mut route = PalwKernelRouteStateV1::new(
        palw_kernel_route_policy_v1(Hash64::default(), Hash64::default()),
        None,
        LedgerScalarsV1::default(),
    );
    route.aux = entries;
    carriage.kernel_route = Some(route);
    carriage.into_state(&params, None).unwrap()
}

fn encoded_rows(bodies: impl IntoIterator<Item = NativeRowBody>) -> BTreeMap<(u8, Vec<u8>), Vec<u8>> {
    bodies
        .into_iter()
        .map(|body| {
            let row = NativeRowValue::new(body).unwrap();
            ((KERNEL_AUX_TABLE, row.body.key().encode()), row.encode().unwrap())
        })
        .collect()
}

fn denied_all(state: &PalwChainStateV2, claim: ClaimId, error: NativeRowCreditDenial) {
    let before = state.state_root();
    let gate = NativeRowCreditGate::new(state);
    for usage in ALL_CREDIT_USES {
        assert_eq!(gate.authorize(&claim, usage), Err(error), "{usage:?}");
    }
    assert_eq!(state.state_root(), before);
}

#[test]
fn actual_public_genesis_and_empty_route_reads_keep_missing_route_and_row_distinct() {
    let claim = ClaimId(d(90));
    let absent_route = PalwChainStateV2::genesis();
    assert_eq!(
        PositiveCreditRows::new(&absent_route).read(NativeRowKey::ClaimIndex(claim)),
        Err(CreditRowFailure::MissingKernelRoute)
    );
    denied_all(&absent_route, claim, NativeRowCreditDenial::Rows(CreditRowFailure::MissingKernelRoute));

    let empty_route = dto_state(BTreeMap::new());
    assert_eq!(PositiveCreditRows::new(&empty_route).read(NativeRowKey::ClaimIndex(claim)), Ok(None));
    denied_all(&empty_route, claim, NativeRowCreditDenial::Rows(CreditRowFailure::MissingRow(NativeRowKey::ClaimIndex(claim))));
}

#[test]
fn actual_claim_index_missing_result_never_falls_back_to_an_old_subject() {
    let index = ClaimIndexRow { claim: ClaimId(d(1)), result: ResultId(d(2)), case: CaseId(d(3)) };
    let state = dto_state(encoded_rows([NativeRowBody::ClaimIndex(index)]));
    denied_all(&state, index.claim, NativeRowCreditDenial::Rows(CreditRowFailure::MissingRow(NativeRowKey::Result(index.result))));
}

#[test]
fn malformed_version_and_wrong_key_body_are_not_reported_as_absent_rows() {
    let index = ClaimIndexRow { claim: ClaimId(d(1)), result: ResultId(d(2)), case: CaseId(d(3)) };
    let row = NativeRowValue::new(NativeRowBody::ClaimIndex(index)).unwrap();
    let key = NativeRowKey::ClaimIndex(index.claim);
    let mut malformed = row.encode().unwrap();
    malformed.pop();
    let state = dto_state(BTreeMap::from([((KERNEL_AUX_TABLE, key.encode()), malformed)]));
    denied_all(
        &state,
        index.claim,
        NativeRowCreditDenial::Rows(CreditRowFailure::MalformedRow { key, cause: ContractError::NonCanonical }),
    );

    let mut bad_version = row.encode().unwrap();
    bad_version[0] = 1;
    let state = dto_state(BTreeMap::from([((KERNEL_AUX_TABLE, key.encode()), bad_version)]));
    denied_all(
        &state,
        index.claim,
        NativeRowCreditDenial::Rows(CreditRowFailure::MalformedRow { key, cause: ContractError::Version }),
    );

    let other = ClaimIndexRow { claim: ClaimId(d(99)), ..index };
    let bytes = NativeRowValue::new(NativeRowBody::ClaimIndex(other)).unwrap().encode().unwrap();
    let state = dto_state(BTreeMap::from([((KERNEL_AUX_TABLE, key.encode()), bytes)]));
    denied_all(
        &state,
        index.claim,
        NativeRowCreditDenial::Rows(CreditRowFailure::MalformedRow { key, cause: ContractError::Identity }),
    );
}

#[test]
fn actual_core_signed_bundle_is_data_and_leaves_every_native_owner_missing() {
    let (index, result, case, funding, witness, coverage) = signed_bundle();
    let state = dto_state(encoded_rows([
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Result(result),
        NativeRowBody::Case(case),
        NativeRowBody::Funding(funding),
        NativeRowBody::Witness(witness),
        NativeRowBody::Coverage(coverage),
    ]));
    let reader = PositiveCreditRows::new(&state);
    let inputs = reader.resolve(index.claim).unwrap();
    assert_eq!(inputs.bundle().index, index);
    assert_eq!(inputs.bundle().case.miner_data, MinerDataStatus::Pending);
    assert_eq!(inputs.witness_and_coverage().unwrap().1.state, CoverageState::Matched);
    assert_eq!(inputs.source_state_root(), state.state_root());
    assert_eq!(inputs.missing_native_owners(), MISSING_NATIVE_OWNERS);
    denied_all(&state, index.claim, NativeRowCreditDenial::MissingOwner(MissingNativeOwner::AcceptedOrigin));
}

#[test]
fn core_bundle_signature_failure_and_cross_row_link_failure_are_distinct() {
    let (index, result, case, funding, _, _) = signed_bundle();
    let mut bad_signature = result.clone();
    bad_signature.miner_signature[0] ^= 1;
    let state = dto_state(encoded_rows([
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Result(bad_signature),
        NativeRowBody::Case(case),
        NativeRowBody::Funding(funding.clone()),
    ]));
    denied_all(
        &state,
        index.claim,
        NativeRowCreditDenial::Rows(CreditRowFailure::Authentication {
            stage: AuthenticationStage::ResultAndFunding,
            cause: ContractError::Signature,
        }),
    );

    let mut wrong_plan = case;
    wrong_plan.coverage.plan = PlanId(d(99));
    let state = dto_state(encoded_rows([
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Result(result),
        NativeRowBody::Case(wrong_plan),
        NativeRowBody::Funding(funding),
    ]));
    denied_all(&state, index.claim, NativeRowCreditDenial::Rows(CreditRowFailure::BundleLinks(ContractError::Identity)));
}

#[test]
fn missing_positive_witness_or_coverage_is_not_a_completed_local_check() {
    let (index, result, case, funding, witness, _) = signed_bundle();
    let bodies = [
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Result(result.clone()),
        NativeRowBody::Case(case),
        NativeRowBody::Funding(funding.clone()),
    ];
    let state = dto_state(encoded_rows(bodies));
    denied_all(
        &state,
        index.claim,
        NativeRowCreditDenial::Rows(CreditRowFailure::MissingRow(NativeRowKey::Witness(witness.witness.id()))),
    );
    let id = witness.witness.id();
    let state = dto_state(encoded_rows([
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Result(result),
        NativeRowBody::Case(case),
        NativeRowBody::Funding(funding),
        NativeRowBody::Witness(witness),
    ]));
    denied_all(&state, index.claim, NativeRowCreditDenial::Rows(CreditRowFailure::MissingRow(NativeRowKey::Coverage(id))));
}

#[test]
fn witness_authentication_and_coverage_binding_are_checked_by_shared_validator() {
    let (index, result, case, funding, witness, coverage) = signed_bundle();
    let mut bad_signature = witness.clone();
    bad_signature.panel_signature[0] ^= 1;
    let state = dto_state(encoded_rows([
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Result(result.clone()),
        NativeRowBody::Case(case),
        NativeRowBody::Funding(funding.clone()),
        NativeRowBody::Witness(bad_signature),
        NativeRowBody::Coverage(coverage),
    ]));
    denied_all(
        &state,
        index.claim,
        NativeRowCreditDenial::Rows(CreditRowFailure::Authentication {
            stage: AuthenticationStage::Witness,
            cause: ContractError::Signature,
        }),
    );
    let mut wrong_plan = coverage;
    wrong_plan.plan = PlanId(d(99));
    let state = dto_state(encoded_rows([
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Result(result),
        NativeRowBody::Case(case),
        NativeRowBody::Funding(funding),
        NativeRowBody::Witness(witness),
        NativeRowBody::Coverage(wrong_plan),
    ]));
    denied_all(&state, index.claim, NativeRowCreditDenial::Rows(CreditRowFailure::BundleLinks(ContractError::Identity)));
}
