//! Real native output/current-entry qualification and checked money rules.
//! These cases do not claim native admission, script validity or earned credit.
use kaspa_consensus_core::{
    Hash64,
    mldsa87_primitives::p2pkh_mldsa87_spk,
    palw_positive_assignment_funds_v2::*,
    tx::{Transaction, TransactionOutput, UtxoEntry},
};
use misaka_palw_positive_contract::{
    assignment::{consents::*, role_actions::*, rows::*},
    funding::*,
    identity::*,
    messages::*,
    rows::*,
};
#[path = "../../../misaka/palw/positive-contract/tests/support/assignment.rs"]
mod support;

fn funded() -> (misaka_palw_positive_contract::assignment::AssignmentAdmissionBundleV2, Transaction, UtxoEntry, [u8; PUBKEY_BYTES]) {
    let mut bundle = support::bundle();
    let key = [17; PUBKEY_BYTES];
    let payload = kaspa_hashes::blake2b_512_address_payload(&key);
    bundle.terms.payer = principal(&key);
    bundle.terms.refund_payload = Digest(payload.as_bytes());
    bundle.funding.assignment = bundle.terms.id();
    bundle.funding.payer = bundle.terms.payer;
    bundle.funding.refund_payload = bundle.terms.refund_payload;
    bundle.future_case.assignment = bundle.terms.id();
    bundle.future_case.payer = bundle.terms.payer;
    bundle.future_case.refund_payload = bundle.terms.refund_payload;
    let script = p2pkh_mldsa87_spk(&payload.as_bytes());
    let transaction = Transaction::new(
        0,
        vec![],
        vec![TransactionOutput::new(175, script.clone())],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
        0,
        vec![7],
    );
    let entry = UtxoEntry::new(175, script, 50, false);
    (bundle, transaction, entry, key)
}

#[test]
fn actual_own_output_is_split_without_creating_money() {
    let (bundle, transaction, entry, key) = funded();
    let qualified = qualify_assignment_funding_output_v2(&bundle, &transaction, &key, Some(&entry), 20, 30, 50).unwrap();
    assert_eq!(qualified.outpoint().transaction.0, transaction.id().as_bytes());
    assert_eq!(qualified.outpoint().index, 0);
    assert_eq!(qualified.amount(), 175);
    assert_eq!(qualified.initial_legs().map(|l| l.remaining), [100, 20, 30, 25]);
    assert_eq!(qualified.initial_legs().iter().map(|l| l.initial).sum::<u64>(), entry.amount);
}

#[test]
fn spent_changed_or_other_payer_output_never_qualifies() {
    let (bundle, transaction, entry, key) = funded();
    assert_eq!(
        qualify_assignment_funding_output_v2(&bundle, &transaction, &key, None, 20, 30, 50),
        Err(AssignmentFundsErrorV2::CurrentUtxo)
    );
    let mut changed = entry.clone();
    changed.amount += 1;
    assert_eq!(
        qualify_assignment_funding_output_v2(&bundle, &transaction, &key, Some(&changed), 20, 30, 50),
        Err(AssignmentFundsErrorV2::CurrentUtxo)
    );
    let mut changed = entry.clone();
    changed.script_public_key = p2pkh_mldsa87_spk(&Hash64::from_u64_word(9).as_bytes());
    assert_eq!(
        qualify_assignment_funding_output_v2(&bundle, &transaction, &key, Some(&changed), 20, 30, 50),
        Err(AssignmentFundsErrorV2::CurrentUtxo)
    );
    assert_eq!(
        qualify_assignment_funding_output_v2(&bundle, &transaction, &[18; PUBKEY_BYTES], Some(&entry), 20, 30, 50),
        Err(AssignmentFundsErrorV2::Payer)
    );
}

#[test]
fn expiry_signed_caps_and_amount_shortfall_are_enforced() {
    let (bundle, transaction, entry, key) = funded();
    assert_eq!(
        qualify_assignment_funding_output_v2(&bundle, &transaction, &key, Some(&entry), 20, 30, 100),
        Err(AssignmentFundsErrorV2::Expired)
    );
    assert!(qualify_assignment_funding_output_v2(&bundle, &transaction, &key, Some(&entry), 21, 30, 50).is_err());
    assert!(qualify_assignment_funding_output_v2(&bundle, &transaction, &key, Some(&entry), 20, u64::MAX, 50).is_err());
    let mut short_tx = transaction.clone();
    short_tx.outputs[0].value = 149;
    short_tx.finalize();
    let mut short_entry = entry;
    short_entry.amount = 149;
    assert!(qualify_assignment_funding_output_v2(&bundle, &short_tx, &key, Some(&short_entry), 20, 30, 50).is_err());
}

#[test]
fn protected_earned_dues_and_capital_cannot_be_refunded_or_overflowed() {
    let leg = BalanceLeg { initial: 100, paid: 10, refunded: 5, burned: 5, remaining: 80 };
    let debit = AssignmentLegDebitV2 { paid: 20, refunded: 10, burned: 5 };
    let after = debit_assignment_leg_v2(leg, debit, 45).unwrap();
    assert_eq!(after, BalanceLeg { initial: 100, paid: 30, refunded: 15, burned: 10, remaining: 45 });
    assert_eq!(debit_assignment_leg_v2(leg, debit, 46), Err(AssignmentFundsErrorV2::ProtectedFloor));
    assert!(debit_assignment_leg_v2(leg, AssignmentLegDebitV2 { paid: u64::MAX, burned: 1, refunded: 0 }, 0).is_err());
    assert!(debit_assignment_leg_v2(leg, AssignmentLegDebitV2 { refunded: 81, ..Default::default() }, 0).is_err());
}

#[test]
fn actual_join_output_has_separate_payer_capital_and_no_worker_credit() {
    let (bundle, mut transaction, _, _) = funded();
    let key = [29; PUBKEY_BYTES];
    let payload = kaspa_hashes::blake2b_512_address_payload(&key);
    let script = p2pkh_mldsa87_spk(&payload.as_bytes());
    transaction.outputs[0] = TransactionOutput::new(70, script.clone());
    transaction.finalize();
    let entry = UtxoEntry::new(70, script, 60, false);
    let join = AssignmentProverJoinV2 {
        version: 2,
        revision: 1,
        assignment: bundle.terms.id(),
        original_bundle: bundle.id(),
        case: None,
        prover: support::bond(29),
        payer: principal(&key),
        funding_output_index: 0,
        max_fee: 20,
        max_capital: 30,
        max_locked: 70,
        max_attempts: 2,
        max_build_work: 100,
        max_submitted_bytes: 100,
        max_retained_bytes: 100,
        admission_before_daa: 180,
        service_before_daa: 400,
        retention_until_daa: 500,
        cancellation_policy: bundle.cancellation_policy,
    };
    let output = qualify_assignment_prover_funding_output_v2(&bundle, &join, &key, &transaction, Some(&entry), 20, 30, 60).unwrap();
    assert_eq!(output.initial_legs().map(|l| l.initial), [0, 20, 30, 20]);
    assert!(qualify_assignment_prover_funding_output_v2(&bundle, &join, &key, &transaction, None, 20, 30, 60).is_err());
    assert!(
        qualify_assignment_prover_funding_output_v2(&bundle, &join, &[30; PUBKEY_BYTES], &transaction, Some(&entry), 20, 30, 60)
            .is_err()
    );
    assert!(qualify_assignment_prover_funding_output_v2(&bundle, &join, &key, &transaction, Some(&entry), 21, 30, 60).is_err());
    assert!(qualify_assignment_prover_funding_output_v2(&bundle, &join, &key, &transaction, Some(&entry), 20, 30, 180).is_err());
}

fn budget() -> AggregateBudget {
    AggregateBudget {
        rounds: 10,
        unique_source_bytes: 100,
        transmitted_bytes: 200,
        additional_work: 100,
        retained_bytes: 100,
        retention_until: 500,
        fee: 10,
        witness_attempts: 2,
    }
}
fn assigned() -> (AssignmentRow, AssignmentFundingRow, [u8; PUBKEY_BYTES], Transaction, UtxoEntry) {
    let (mut bundle, transaction, entry, key) = funded();
    bundle.terms.ruleset = misaka_palw_positive_contract::native::ruleset_id(support::d(31), support::d(32));
    bundle.funding.assignment = bundle.terms.id();
    bundle.future_case.assignment = bundle.terms.id();
    let source = AssignmentSourceConsentV2 {
        version: 2,
        revision: 1,
        assignment: bundle.terms.id(),
        worker: bundle.terms.worker,
        source_catalog: bundle.source_catalog,
        source_policy: bundle.terms.source_policy,
        budget: budget(),
        service_before_daa: 400,
    };
    let prover = AssignmentProverConsentV2 {
        version: 2,
        revision: 1,
        assignment: bundle.terms.id(),
        prover: bundle.terms.prover,
        positive_policy: support::d(32),
        max_attempts: 2,
        max_build_work: 100,
        max_submitted_bytes: 100,
        max_retained_bytes: 100,
        retention_until_daa: 500,
        build_before_daa: 400,
        max_service_fee: 10,
        cancellation_policy: bundle.cancellation_policy,
    };
    bundle.source_consent = source.id();
    bundle.prover_consent = prover.id();
    let origin = AcceptedOrigin {
        accepting_block: NativeHash64([1; 64]),
        carrier_block: NativeHash64([2; 64]),
        carrier_transaction: NativeHash64(transaction.id().as_bytes()),
        accepted_tx_ordinal: 1,
        event_ordinal: 0,
        accepted_daa: 50,
        accepted_blue_score: 50,
    };
    let material = AcceptedMaterial { origin, digest: support::d(33) };
    let assignment = AssignmentRow {
        bundle,
        source_consent: source,
        prover_consent: prover,
        origin,
        worker_key: [19; PUBKEY_BYTES],
        positive_policy: support::d(32),
        assignment_policy: support::d(34),
        fired_rules: support::d(31),
        job: support::d(35),
        input: support::d(36),
        context: bundle.job_context,
        registered_metadata: support::d(37),
        status: AssignmentStatusV2::Open,
        admission_material: material,
        derivation: material,
        dependencies: AcceptedMaterial { origin, digest: bundle.terms.empty_dependencies_id() },
        reservation: material,
        liability: material,
        last_event: origin,
    };
    let qualified = qualify_assignment_funding_output_v2(&bundle, &transaction, &key, Some(&entry), 20, 30, 50).unwrap();
    let [worker, fee, capital, surplus] = qualified.initial_legs();
    let funding = AssignmentFundingRow {
        assignment: assignment.id(),
        authorization: bundle.funding,
        payer_key: key,
        payer_signature: [0; SIGNATURE_BYTES],
        funding_origin: origin,
        original_outpoint: qualified.outpoint(),
        current_outpoint: Some(qualified.outpoint()),
        initial_locked: qualified.amount(),
        worker,
        fee,
        capital,
        surplus,
        worker_reserve: 100,
        earned_service_due: 0,
        liability_floor: 30,
        reservation: material,
        liability: material,
        last_settlement: None,
    };
    assignment.validate_shape().unwrap();
    funding.validate_shape().unwrap();
    (assignment, funding, key, transaction, entry)
}
fn case_bundle(assignment: &AssignmentRow) -> (MinerResultCommit, MinerServiceContract, FundingPlanBasis, FundingConsent) {
    let result = MinerResultCommit {
        version: 2,
        revision: 1,
        scope: ChainScope {
            network: assignment.bundle.terms.network,
            ruleset: assignment.bundle.terms.ruleset,
            branch: Digest(assignment.bundle.terms.branch_anchor.0),
        },
        job: assignment.job,
        execution: assignment.context,
        miner: principal(&assignment.worker_key),
        registered_class: assignment.bundle.terms.registered_class,
        artifact: support::d(40),
        program: support::d(41),
        input: assignment.input,
        tokenizer: support::d(42),
        execution_rules: support::d(43),
        selector: support::d(44),
        generation_seed: [1; 32],
        temperature_q24: 0,
        prefill: 1,
        decode: 2,
        max_context: 16,
        output: support::d(45),
        dependencies: assignment.bundle.terms.empty_dependencies_id(),
        service_contract: Digest([0; 64]),
    };
    let service = MinerServiceContract {
        version: 2,
        revision: 1,
        case: result.case_id(),
        source_catalog: assignment.bundle.source_catalog,
        provider_delegations: support::d(46),
        budget: budget(),
        funding_consent: Digest([0; 64]),
        deadline: 400,
    };
    let basis = FundingPlanBasis {
        profile: CheckProfile::ExactStreamV1,
        derivation: support::d(47),
        mandatory_relations: 3,
        source_catalog: assignment.bundle.source_catalog,
        resource_quote: support::d(48),
        policy: assignment.positive_policy,
        schedule: support::d(49),
        required_fee: 30,
        required_capital: 40,
    };
    let consent = FundingConsent {
        version: 2,
        revision: 1,
        case: result.case_id(),
        scope: funding_scope(&result, &service, &basis),
        payer: assignment.bundle.terms.payer,
        max_fees: 40,
        max_locked: 100,
        funding_output_index: 0,
        refund_payload: assignment.bundle.terms.refund_payload,
        admission_expiry: 200,
    };
    linked(result, service, basis, consent)
}
fn linked(
    mut result: MinerResultCommit,
    mut service: MinerServiceContract,
    basis: FundingPlanBasis,
    mut consent: FundingConsent,
) -> (MinerResultCommit, MinerServiceContract, FundingPlanBasis, FundingConsent) {
    consent.scope = funding_scope(&result, &service, &basis);
    service.funding_consent = consent.id().0;
    result.service_contract = service.id();
    (result, service, basis, consent)
}

#[test]
fn case_uses_its_own_signed_bounds_and_full_output_projection() {
    let (assignment, funding, key, mut transaction, _) = assigned();
    transaction.outputs[0].value = 90;
    transaction.payload = vec![9];
    transaction.finalize();
    let entry = UtxoEntry::new(90, transaction.outputs[0].script_public_key.clone(), 120, false);
    let (result, service, basis, consent) = case_bundle(&assignment);
    // C fee30 exceeds A fee20, but satisfies independently signed C bound40.
    let qualified = qualify_assigned_case_funding_output_v2(
        &assignment,
        &funding,
        &result,
        &service,
        &basis,
        &consent,
        &key,
        &transaction,
        Some(&entry),
        120,
    )
    .unwrap();
    assert_eq!(qualified.initial_legs().map(|l| l.initial), [0, 30, 40, 20]);
    let mut changed = result.clone();
    changed.output = support::d(50);
    assert!(
        qualify_assigned_case_funding_output_v2(
            &assignment,
            &funding,
            &changed,
            &service,
            &basis,
            &consent,
            &key,
            &transaction,
            Some(&entry),
            120
        )
        .is_err()
    );
    let mut over = consent.clone();
    over.max_fees = 41;
    let (r, s, b, c) = linked(result.clone(), service, basis, over);
    assert!(
        qualify_assigned_case_funding_output_v2(&assignment, &funding, &r, &s, &b, &c, &key, &transaction, Some(&entry), 120).is_err()
    );
    let mut other = consent.clone();
    other.payer = principal(&[30; PUBKEY_BYTES]);
    let (r, s, b, c) = linked(result, service, basis, other);
    assert!(
        qualify_assigned_case_funding_output_v2(&assignment, &funding, &r, &s, &b, &c, &key, &transaction, Some(&entry), 120).is_err()
    );
}

#[test]
fn initial_relay_matches_only_the_signed_assignment_and_exact_funding_output() {
    use misaka_palw_positive_contract::{
        ContractError,
        assignment::funding_relay::{FundingRelayTermsV2, RelayFundingPurposeV2, RelayTargetV2, RelayTemplateV2},
    };
    let bundle = support::bundle();
    let funding = NativeOutpoint { transaction: NativeHash64([90; 64]), index: bundle.funding.funding_output_index };
    let terms = FundingRelayTermsV2 {
        version: 2,
        revision: 1,
        contract_network: bundle.terms.network,
        contract_ruleset: bundle.terms.ruleset,
        assignment: bundle.terms.id(),
        contract_anchor: bundle.terms.branch_anchor,
        relay_ruleset: Digest([91; 64]),
        relay_policy: Digest([92; 64]),
        payer_bond: bundle.terms.poster,
        payer: bundle.terms.payer,
        refund_payload: bundle.terms.refund_payload,
        funding_purpose: RelayFundingPurposeV2::PosterA,
        target: RelayTargetV2::InitialAssignment { bundle: bundle.id(), poster_nonce: bundle.terms.poster_nonce },
        funding_outpoint: funding,
        expected_amount: 175,
        template: RelayTemplateV2::NativeOp12NoCaseNoReceiptsV1,
        register_before_daa: 10,
        funding_admission_before_daa: 40,
        custody_until_daa: 80,
        metadata_fee_schedule: Digest([93; 64]),
        physical_custody_profile: Digest([94; 64]),
        max_metadata_fee: 1_000_000,
    };
    let check = |r: &FundingRelayTermsV2| check_initial_assignment_relay_target_v2(r, &bundle, funding, 175, 20);
    assert_eq!(check(&terms), Ok(()));
    for substituted in [
        FundingRelayTermsV2 { contract_network: Digest([2; 64]), ..terms },
        FundingRelayTermsV2 { contract_ruleset: Digest([3; 64]), ..terms },
        FundingRelayTermsV2 { contract_anchor: NativeHash64([4; 64]), ..terms },
        FundingRelayTermsV2 { refund_payload: Digest([5; 64]), ..terms },
        FundingRelayTermsV2 { payer_bond: bundle.terms.worker, ..terms },
        FundingRelayTermsV2 { assignment: misaka_palw_positive_contract::assignment::AssignmentId(Digest([6; 64])), ..terms },
        FundingRelayTermsV2 {
            target: RelayTargetV2::InitialAssignment { bundle: Digest([7; 64]), poster_nonce: bundle.terms.poster_nonce },
            ..terms
        },
        FundingRelayTermsV2 {
            target: RelayTargetV2::InitialAssignment { bundle: bundle.id(), poster_nonce: bundle.terms.poster_nonce + 1 },
            ..terms
        },
        FundingRelayTermsV2 { funding_outpoint: NativeOutpoint { index: funding.index + 1, ..funding }, ..terms },
    ] {
        assert_eq!(check(&substituted), Err(ContractError::Identity));
    }
    let other_f = NativeOutpoint { transaction: NativeHash64([95; 64]), ..funding };
    assert_eq!(check_initial_assignment_relay_target_v2(&terms, &bundle, other_f, 175, 20), Err(ContractError::Identity));
    assert_eq!(check_initial_assignment_relay_target_v2(&terms, &bundle, funding, 174, 20), Err(ContractError::Identity));
    assert_eq!(check_initial_assignment_relay_target_v2(&terms, &bundle, funding, 175, 40), Err(ContractError::Range));
}

#[test]
fn initial_case_relay_binds_the_exact_result_consent_and_case_coin() {
    use misaka_palw_positive_contract::{
        ContractError,
        assignment::funding_relay::{FundingRelayTermsV2, RelayFundingPurposeV2, RelayTargetV2, RelayTemplateV2},
    };
    let (assignment, _, _, _, _) = assigned();
    let bundle = &assignment.bundle;
    let (result, _, _, consent) = case_bundle(&assignment);
    let funding = NativeOutpoint { transaction: NativeHash64([90; 64]), index: consent.funding_output_index };
    let terms = FundingRelayTermsV2 {
        version: 2,
        revision: 1,
        contract_network: bundle.terms.network,
        contract_ruleset: bundle.terms.ruleset,
        assignment: assignment.id(),
        contract_anchor: bundle.terms.branch_anchor,
        relay_ruleset: Digest([91; 64]),
        relay_policy: Digest([92; 64]),
        payer_bond: bundle.terms.poster,
        payer: consent.payer,
        refund_payload: consent.refund_payload,
        funding_purpose: RelayFundingPurposeV2::CaseC,
        target: RelayTargetV2::InitialCase { result: result.id(), funding_scope: consent.scope, consent: consent.id() },
        funding_outpoint: funding,
        expected_amount: 90,
        template: RelayTemplateV2::NativeOp12NoCaseNoReceiptsV1,
        register_before_daa: 100,
        funding_admission_before_daa: 150,
        custody_until_daa: 800,
        metadata_fee_schedule: Digest([93; 64]),
        physical_custody_profile: Digest([94; 64]),
        max_metadata_fee: 1_000_000,
    };
    let registration =
        AcceptedOrigin { accepting_block: NativeHash64([97; 64]), accepted_daa: 90, accepted_blue_score: 90, ..assignment.origin };
    assert_eq!(check_case_relay_registration_assignment_v2(&terms, &assignment, registration), Ok(()));
    let substituted_scope = FundingRelayTermsV2 { contract_ruleset: terms.relay_ruleset, ..terms };
    assert_eq!(
        check_case_relay_registration_assignment_v2(&substituted_scope, &assignment, registration),
        Err(ContractError::Identity)
    );
    let mut closed = assignment.clone();
    closed.status = AssignmentStatusV2::Cancelled;
    assert_eq!(check_case_relay_registration_assignment_v2(&terms, &closed, registration), Err(ContractError::Range));
    assert_eq!(check_case_relay_registration_assignment_v2(&terms, &assignment, assignment.origin), Err(ContractError::Range));
    let late_bound = FundingRelayTermsV2 { funding_admission_before_daa: 201, ..terms };
    assert_eq!(check_case_relay_registration_assignment_v2(&late_bound, &assignment, registration), Err(ContractError::Range));
    let check = |terms, result, consent, funding, amount, now| {
        check_initial_case_relay_target_v2(terms, bundle, result, consent, funding, amount, now)
    };
    assert_eq!(check(&terms, result.id(), &consent, funding, 90, 120), Ok(()));
    let wrong_result = ResultId(Digest([95; 64]));
    let mut wrong_scope = consent.clone();
    wrong_scope.scope = FundingScopeId(Digest([96; 64]));
    let mut wrong_consent = consent.clone();
    wrong_consent.max_fees -= 1;
    let wrong_funding = NativeOutpoint { index: funding.index + 1, ..funding };
    for outcome in [
        check(&terms, wrong_result, &consent, funding, 90, 120),
        check(&terms, result.id(), &wrong_scope, funding, 90, 120),
        check(&terms, result.id(), &wrong_consent, funding, 90, 120),
        check(&terms, result.id(), &consent, wrong_funding, 90, 120),
        check(&terms, result.id(), &consent, funding, 89, 120),
    ] {
        assert_eq!(outcome, Err(ContractError::Identity));
    }
    assert_eq!(check(&terms, result.id(), &consent, funding, 90, 150), Err(ContractError::Range));
    let over_cap = FundingRelayTermsV2 { expected_amount: consent.max_locked + 1, ..terms };
    assert_eq!(check(&over_cap, result.id(), &consent, funding, over_cap.expected_amount, 120), Err(ContractError::Budget));
}

#[test]
fn case_relay_continuation_registration_uses_the_actual_predecessor() {
    use misaka_palw_positive_contract::{
        ContractError,
        assignment::funding_relay::{FundingRelayTermsV2, RelayFundingPurposeV2, RelayTargetV2, RelayTemplateV2},
    };
    let (mut assignment, _, key, _, _) = assigned();
    let (result, _, basis, consent) = case_bundle(&assignment);
    assignment.status = AssignmentStatusV2::Consumed { case: result.case_id(), result: result.id() };
    let original = NativeOutpoint { transaction: NativeHash64([90; 64]), index: consent.funding_output_index };
    let predecessor = NativeOutpoint { transaction: NativeHash64([91; 64]), index: 1 };
    let successor = NativeOutpoint { transaction: NativeHash64([92; 64]), index: 2 };
    let funding_origin = AcceptedOrigin {
        accepting_block: NativeHash64([93; 64]),
        carrier_transaction: original.transaction,
        accepted_daa: 100,
        accepted_blue_score: 100,
        ..assignment.origin
    };
    let previous = AcceptedOrigin {
        accepting_block: NativeHash64([94; 64]),
        carrier_transaction: predecessor.transaction,
        accepted_daa: 120,
        accepted_blue_score: 120,
        ..funding_origin
    };
    let registration = AcceptedOrigin { accepted_tx_ordinal: previous.accepted_tx_ordinal + 1, ..previous };
    let account = FundingRow {
        consent,
        basis,
        payer_key: key,
        payer_signature: [0; SIGNATURE_BYTES],
        funding_origin,
        original_outpoint: original,
        current_outpoint: Some(predecessor),
        initial_locked: 90,
        fee: BalanceLeg { initial: 30, remaining: 20, paid: 10, refunded: 0, burned: 0 },
        capital: BalanceLeg { initial: 40, remaining: 40, paid: 0, refunded: 0, burned: 0 },
        surplus: BalanceLeg { initial: 20, remaining: 20, paid: 0, refunded: 0, burned: 0 },
        liability: AcceptedMaterial { origin: funding_origin, digest: support::d(95) },
        last_settlement: Some(AcceptedMaterial { origin: previous, digest: support::d(96) }),
    };
    let signed = &assignment.bundle.terms;
    let terms = FundingRelayTermsV2 {
        version: 2,
        revision: 1,
        contract_network: signed.network,
        contract_ruleset: signed.ruleset,
        assignment: assignment.id(),
        contract_anchor: signed.branch_anchor,
        relay_ruleset: support::d(91),
        relay_policy: support::d(92),
        payer_bond: signed.poster,
        payer: account.consent.payer,
        refund_payload: account.consent.refund_payload,
        funding_purpose: RelayFundingPurposeV2::CaseC,
        target: RelayTargetV2::Continuation {
            original_funding: original,
            predecessor,
            parent_spend: successor.transaction,
            original_contract: account.consent.id().0,
        },
        funding_outpoint: successor,
        expected_amount: 75,
        template: RelayTemplateV2::NativeOp12NoCaseNoReceiptsV1,
        register_before_daa: 150,
        funding_admission_before_daa: 200,
        custody_until_daa: 800,
        metadata_fee_schedule: support::d(93),
        physical_custody_profile: support::d(94),
        max_metadata_fee: 1_000_000,
    };
    let check = |terms: &FundingRelayTermsV2, account: &FundingRow, at| {
        check_case_relay_registration_continuation_v2(terms, &assignment, account, at)
    };
    assert_eq!(check(&terms, &account, registration), Ok(()));
    assert_eq!(check(&terms, &account, previous), Err(ContractError::Range));
    let stale = FundingRow { current_outpoint: Some(original), ..account.clone() };
    assert_eq!(check(&terms, &stale, registration), Err(ContractError::Identity));
    let overdraw = FundingRelayTermsV2 { expected_amount: 81, ..terms };
    assert_eq!(check(&overdraw, &account, registration), Err(ContractError::Budget));
    let other = FundingRelayTermsV2 { payer_bond: signed.prover, ..terms };
    assert_eq!(check(&other, &account, registration), Err(ContractError::Identity));
}
