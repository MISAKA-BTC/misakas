//! DATA-only counterexamples. Zero signatures deliberately cannot establish
//! accepted history, a live lock or native authorization; the real owner must
//! supply those independently before it can consume any calculation result.
use super::*;
use kaspa_consensus_core::{
    mldsa87_primitives::p2pkh_mldsa87_spk,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{TransactionInput, TransactionOutput, UtxoEntry},
};
use misaka_palw_positive_contract::{funding::*, identity::*, messages::*, native::ruleset_id, rows::*};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn origin(clock: u64, tx: [u8; 64]) -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([clock as u8; 64]),
        carrier_block: NativeHash64([clock as u8; 64]),
        carrier_transaction: NativeHash64(tx),
        accepted_tx_ordinal: 4,
        event_ordinal: 5,
        accepted_daa: clock,
        accepted_blue_score: clock,
    }
}
fn material(clock: u64, seed: u8) -> AcceptedMaterial {
    AcceptedMaterial { origin: origin(clock, [seed; 64]), digest: d(seed) }
}
fn leg(n: u64) -> BalanceLeg {
    BalanceLeg { initial: n, paid: 0, refunded: 0, burned: 0, remaining: n }
}

struct Fixture {
    result: ResultRow,
    case: CaseRow,
    funding: FundingRow,
    provider: PrincipalId,
    keys: FeeRecipientKeys,
}
impl Fixture {
    fn new(seed: u8) -> Self {
        let key = [0; PUBKEY_BYTES];
        let provider_key = [1; PUBKEY_BYTES];
        let provider = principal(&provider_key);
        let keys = FeeRecipientKeys::new([provider_key.as_slice()], 1).unwrap();
        let mut r = MinerResultCommit {
            version: 2,
            revision: 1,
            scope: ChainScope { network: d(1), ruleset: ruleset_id(d(32), d(31)), branch: d(3) },
            job: d(seed),
            execution: d(5),
            miner: principal(&key),
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
        let mut service = MinerServiceContract {
            version: 2,
            revision: 1,
            case: r.case_id(),
            source_catalog: d(33),
            provider_delegations: d(34),
            budget: AggregateBudget {
                rounds: 10,
                unique_source_bytes: 100,
                transmitted_bytes: 200,
                additional_work: 100,
                retained_bytes: 200,
                retention_until: 200,
                fee: 20,
                witness_attempts: 2,
            },
            funding_consent: d(0),
            deadline: 100,
        };
        let basis = FundingPlanBasis {
            profile: CheckProfile::ExactStreamV1,
            derivation: d(35),
            mandatory_relations: 10,
            source_catalog: service.source_catalog,
            resource_quote: d(36),
            policy: d(31),
            schedule: d(37),
            required_fee: 27,
            required_capital: 20,
        };
        let consent = FundingConsent {
            version: 2,
            revision: 1,
            case: r.case_id(),
            scope: funding_scope(&r, &service, &basis),
            payer: principal(&key),
            max_fees: 30,
            max_locked: 100,
            funding_output_index: 1,
            refund_payload: Digest(kaspa_hashes::blake2b_512_address_payload(&key).as_bytes()),
            admission_expiry: 40,
        };
        service.funding_consent = consent.id().0;
        r.service_contract = service.id();
        let admitted = origin(10, [seed.wrapping_add(70); 64]);
        let result = ResultRow {
            result: r.clone(),
            miner_key: key,
            miner_signature: [0; SIGNATURE_BYTES],
            origin: admitted,
            policy: basis.policy,
            fired_fork: d(32),
            funding_scope: consent.scope,
            dependencies: AcceptedMaterial { origin: admitted, digest: r.dependencies },
            role_assignment: material(10, 18),
        };
        let case = CaseRow {
            case: r.case_id(),
            result: r.id(),
            service,
            coverage: CoverageDescriptor {
                scope: r.scope,
                result: r.id(),
                plan: coverage_plan_id(&r, basis.profile, basis.derivation, basis.mandatory_relations).unwrap(),
                profile: basis.profile,
                mandatory_relations: 10,
                reserved_elements: 100,
                reserved_terms: 200,
                max_part_bytes: 10000,
            },
            status: ClaimStatus::EndedUnverified,
            miner_data: MinerDataStatus::Complete,
            witness_status: WitnessStatus::Incomplete,
            positive_witness: None,
            usage: AggregateUsage { fee: 10, ..AggregateUsage::default() },
            reservation: material(10, 19),
            liability: material(10, 20),
            maturity_daa: 100,
            unresolved_fraud: 0,
            last_event: admitted,
        };
        let point = NativeOutpoint { transaction: admitted.carrier_transaction, index: 1 };
        let funding = FundingRow {
            consent,
            basis,
            payer_key: key,
            payer_signature: [0; SIGNATURE_BYTES],
            funding_origin: admitted,
            original_outpoint: point,
            current_outpoint: Some(point),
            initial_locked: 100,
            fee: leg(27),
            capital: leg(20),
            surplus: leg(53),
            liability: material(10, 21),
            last_settlement: None,
        };
        Self { result, case, funding, provider, keys }
    }
    fn data(&self) -> FundingData<'_> {
        FundingData { result: &self.result, case: &self.case, funding: &self.funding }
    }
    fn locks(&self) -> PositiveLockViewData {
        PositiveLockViewData::new(&[self.data()], &[], &[], 10).unwrap()
    }
    fn point(&self) -> TransactionOutpoint {
        outpoint(self.funding.current_outpoint.unwrap())
    }
    fn pay(&self, n: u64) -> TransactionOutput {
        TransactionOutput::new(n, p2pkh_mldsa87_spk(&self.keys.payload(self.provider).unwrap().as_bytes()))
    }
    fn refund(&self, n: u64) -> TransactionOutput {
        TransactionOutput::new(n, p2pkh_mldsa87_spk(&h(self.funding.consent.refund_payload).as_bytes()))
    }
    fn receipt(&self, unit: u64) -> ServiceReceiptRow {
        ServiceReceiptRow {
            case: self.case.case,
            kind: SourceKind::Input,
            source: self.result.result.input,
            unit,
            result: self.case.result,
            duty: material(10, 50),
            response: material(20, 51),
            provider: self.provider,
            bytes: 10,
            additional_work: 1,
            settlement: None,
        }
    }
    fn due(&self, receipt: &ServiceReceiptRow, amount: u64) -> SourceFeeDeltaIntent {
        SourceFeeDeltaIntent {
            receipt_key: NativeRowKey::ServiceReceipt {
                case: receipt.case,
                kind: receipt.kind,
                source: receipt.source,
                unit: receipt.unit,
            },
            result: self.case.result,
            funding_scope: self.funding.consent.scope,
            schedule: self.funding.basis.schedule,
            provider: receipt.provider,
            amount,
            accepted_response: receipt.response,
        }
    }
    fn whole(&self, kind: CourtWholeFeeKind, amount: u64, seed: u8) -> CourtWholeFeeData {
        CourtWholeFeeData {
            kind,
            result: self.case.result,
            funding_scope: self.funding.consent.scope,
            schedule: self.funding.basis.schedule,
            provider: if kind == CourtWholeFeeKind::ContractualBurn { None } else { Some(self.provider) },
            amount,
            accepted_service: material(20, seed),
        }
    }
    fn tx(&self, outputs: Vec<TransactionOutput>, relay: Option<u64>) -> (Transaction, Vec<UtxoEntry>) {
        let mut inputs = vec![TransactionInput::new(self.point(), vec![], 0, 0)];
        let mut entries = vec![UtxoEntry::new(remaining(&self.funding).unwrap(), self.refund(1).script_public_key, 10, false)];
        if let Some(amount) = relay {
            inputs.push(TransactionInput::new(TransactionOutpoint::new(h(d(99)), 0), vec![], 0, 0));
            entries.push(UtxoEntry::new(amount, self.refund(1).script_public_key, 10, false));
        }
        (Transaction::new(0, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]), entries)
    }
    #[allow(clippy::too_many_arguments)]
    fn calc(
        &self,
        locks: &PositiveLockViewData,
        tx: &Transaction,
        entries: Vec<UtxoEntry>,
        source: &[SourcePaymentData<'_>],
        whole: &[CourtWholeFeeData],
        refunds: RefundData,
        other_burn: OtherBurnData,
        now: u64,
    ) -> Result<PositiveSpendProposalData, PositiveFundsError> {
        let transaction = PopulatedTransaction::new(tx, entries);
        propose_positive_spend(PositiveSpendData {
            current: self.data(),
            locks,
            transaction: &transaction,
            source,
            court_whole: whole,
            recipients: &self.keys,
            refunds,
            other_burn,
            settlement: AcceptedMaterial { origin: origin(now, tx.id().as_bytes()), digest: d(60) },
            now,
            max_dues: 20,
        })
    }
}

#[test]
fn source_payment_preserves_all_three_legs_and_returns_no_native_authority() {
    let f = Fixture::new(1);
    let locks = f.locks();
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let (tx, entries) = f.tx(vec![f.pay(10), f.refund(90)], None);
    let p = f.calc(&locks, &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 30).unwrap();
    assert_eq!((p.after().fee.remaining, p.after().capital.remaining, p.after().surplus.remaining), (17, 20, 53));
    assert_eq!((p.source_fee_paid(), p.court_fee_paid(), p.continued(), p.actual_fee()), (10, 0, 90, 0));
    assert_eq!(p.after().current_outpoint, Some(NativeOutpoint { transaction: NativeHash64(tx.id().as_bytes()), index: 1 }));
    assert_eq!(p.before().original_outpoint, p.after().original_outpoint);
    assert_eq!(p.after().consent, p.before().consent);
    assert_eq!(p.require_native_commit(), Err(MissingNativeFundsOwner::ActualAcceptedMoneyContext));
    assert_eq!(p.check_snapshot(f.data()), Ok(()));
    assert_eq!(p.check_source_snapshot(&[receipt]), Ok(()));
}

#[test]
fn expiry_does_not_unlock_and_same_prefix_provisional_outputs_are_unspendable() {
    let f = Fixture::new(1);
    let point = f.point();
    let locks = f.locks();
    let (tx, _) = f.tx(vec![f.refund(100)], None);
    assert!(matches!(locks.classify_spend(&tx), Ok(LockedInputData::NeedsPositiveQualification { input: 0, .. })));
    let provisional = PositiveLockViewData::new(&[], &[], &[point], 1).unwrap();
    assert_eq!(provisional.classify_spend(&tx), Err(PositiveFundsError::ProvisionalSpend));
    // No clock/deadline argument exists that could waive a live lock.
}

#[test]
fn duplicate_cases_original_points_and_cross_namespace_locks_are_rejected() {
    let f = Fixture::new(1);
    let mut changed = f.funding.clone();
    changed.current_outpoint = Some(NativeOutpoint { transaction: NativeHash64([91; 64]), index: 0 });
    let same_case = FundingData { funding: &changed, ..f.data() };
    assert_eq!(PositiveLockViewData::new(&[f.data(), same_case], &[], &[], 10).err(), Some(PositiveFundsError::DuplicateLock));
    assert_eq!(PositiveLockViewData::new(&[f.data()], &[f.point()], &[], 10).err(), Some(PositiveFundsError::DuplicateLock));
    assert_eq!(PositiveLockViewData::new(&[f.data()], &[], &[f.point()], 10).err(), Some(PositiveFundsError::DuplicateLock));
    let other = Fixture::new(2);
    let mut alias = other.funding.clone();
    alias.current_outpoint = f.funding.current_outpoint;
    let alias = FundingData { funding: &alias, ..other.data() };
    assert_eq!(PositiveLockViewData::new(&[f.data(), alias], &[], &[], 10).err(), Some(PositiveFundsError::DuplicateLock));
}

#[test]
fn a_transaction_cannot_cross_fund_two_locked_inputs_or_place_its_escrow_second() {
    let f = Fixture::new(1);
    let other = Fixture::new(2);
    let locks = PositiveLockViewData::new(&[f.data(), other.data()], &[], &[], 10).unwrap();
    let (mut tx, _) = f.tx(vec![f.refund(100)], Some(100));
    tx.inputs[1].previous_outpoint = other.point();
    tx.finalize();
    assert_eq!(locks.classify_spend(&tx), Err(PositiveFundsError::MultipleLockedInputs));
    let old = TransactionOutpoint::new(h(d(99)), 0);
    let mixed = PositiveLockViewData::new(&[f.data()], &[old], &[], 10).unwrap();
    let (tx, _) = f.tx(vec![f.refund(100)], Some(100));
    assert_eq!(mixed.classify_spend(&tx), Err(PositiveFundsError::MultipleLockedInputs));
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let (mut tx, mut entries) = f.tx(vec![f.pay(10), f.refund(90)], Some(10));
    tx.inputs.swap(0, 1);
    entries.swap(0, 1);
    tx.finalize();
    assert_eq!(
        f.calc(&f.locks(), &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::MissingPositiveLock)
    );
}

#[test]
fn actual_transaction_identity_and_supported_payload_are_required() {
    let f = Fixture::new(1);
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let (mut tx, entries) = f.tx(vec![f.pay(9), f.refund(91)], None);
    tx.outputs[0] = f.pay(10);
    assert_eq!(
        f.calc(&f.locks(), &tx, entries.clone(), &source, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::TransactionIdentity)
    );
    tx.payload = vec![1];
    tx.finalize();
    assert_eq!(
        f.calc(&f.locks(), &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::UnsupportedPayload)
    );
}

#[test]
fn source_pending_dues_are_payable_once_but_duplicates_and_settled_receipts_are_not() {
    let f = Fixture::new(1);
    let mut receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let (tx, entries) = f.tx(vec![f.pay(10), f.pay(10), f.refund(80)], None);
    let duplicate =
        [SourcePaymentData { due: &due, expected_receipt: &receipt }, SourcePaymentData { due: &due, expected_receipt: &receipt }];
    assert_eq!(
        f.calc(&f.locks(), &tx, entries.clone(), &duplicate, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::DuplicateDue)
    );
    receipt.settlement = Some(material(25, 61));
    let paid = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    assert_eq!(
        f.calc(&f.locks(), &tx, entries, &paid, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::AlreadySettled)
    );
}

#[test]
fn source_provider_response_and_schedule_are_bound_to_the_current_receipt() {
    let f = Fixture::new(1);
    let receipt = f.receipt(0);
    let (tx, entries) = f.tx(vec![f.pay(10), f.refund(90)], None);
    for field in 0..4 {
        let mut due = f.due(&receipt, 10);
        match field {
            0 => due.provider = PrincipalId(d(90)),
            1 => due.accepted_response = material(20, 91),
            2 => due.schedule = d(92),
            _ => due.receipt_key = NativeRowKey::Case(f.case.case),
        }
        let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
        assert_eq!(
            f.calc(&f.locks(), &tx, entries.clone(), &source, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
            Some(PositiveFundsError::SourceReceipt)
        );
    }
}

#[test]
fn bond_legacy_and_positive_burn_share_one_actual_fee_allocation() {
    let f = Fixture::new(1);
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let whole = [f.whole(CourtWholeFeeKind::ContractualBurn, 2, 62)];
    for (relay, expected) in [(5, Some(PositiveFundsError::BurnAllocation)), (7, None)] {
        let (tx, entries) = f.tx(vec![f.pay(10), f.refund(88)], Some(relay));
        let result =
            f.calc(&f.locks(), &tx, entries, &source, &whole, RefundData::default(), OtherBurnData { bond: 3, legacy_escrow: 4 }, 30);
        assert_eq!(result.as_ref().err().cloned(), expected);
        if let Ok(plan) = result {
            assert_eq!((plan.positive_burn(), plan.actual_fee()), (2, 9));
        }
    }
}

#[test]
fn wrong_recipient_remainder_or_relay_subsidy_is_rejected() {
    let f = Fixture::new(1);
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    for outputs in [vec![f.refund(10), f.refund(90)], vec![f.pay(10), f.refund(89)]] {
        let (tx, entries) = f.tx(outputs, None);
        assert_eq!(
            f.calc(&f.locks(), &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
            Some(PositiveFundsError::Output)
        );
    }
    let (tx, entries) = f.tx(vec![f.pay(10), f.refund(90), f.pay(1)], None);
    assert_eq!(
        f.calc(&f.locks(), &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::Conservation)
    );
}

#[test]
fn fee_payments_cannot_spend_the_capital_or_surplus_legs() {
    let f = Fixture::new(1);
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let whole = [f.whole(CourtWholeFeeKind::Control, 18, 62)];
    let (tx, entries) = f.tx(vec![f.pay(10), f.pay(18), f.refund(72)], None);
    assert_eq!(
        f.calc(&f.locks(), &tx, entries, &source, &whole, RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::Conservation)
    );
}

#[test]
fn refunds_keep_three_leg_accounting_and_require_a_resolution_reference() {
    let f = Fixture::new(1);
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let whole = [f.whole(CourtWholeFeeKind::ContractualBurn, 2, 62)];
    let (tx, entries) = f.tx(vec![f.pay(10), f.refund(88)], None);
    let mut refunds = RefundData { fee: 15, capital: 20, surplus: 53, resolution: None };
    assert_eq!(
        f.calc(&f.locks(), &tx, entries.clone(), &source, &whole, refunds, OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::ReleaseEvidence)
    );
    refunds.resolution = Some(f.funding.liability);
    let p = f.calc(&f.locks(), &tx, entries, &source, &whole, refunds, OtherBurnData::default(), 30).unwrap();
    assert_eq!((p.after().fee.refunded, p.after().capital.refunded, p.after().surplus.refunded), (15, 20, 53));
    assert_eq!((p.returned(), p.continued(), p.after().current_outpoint), (88, 0, None));
    assert_eq!(p.require_native_commit(), Err(MissingNativeFundsOwner::ActualAcceptedMoneyContext));
}

#[test]
fn snapshot_guards_detect_changed_case_result_funding_and_receipts() {
    let mut f = Fixture::new(1);
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let (tx, entries) = f.tx(vec![f.pay(10), f.refund(90)], None);
    let p = f.calc(&f.locks(), &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 30).unwrap();
    f.case.status = ClaimStatus::Revoked;
    assert_eq!(p.check_snapshot(f.data()), Err(PositiveFundsError::CompareAndSwap));
    f.case.status = ClaimStatus::EndedUnverified;
    f.result.role_assignment = material(10, 63);
    assert_eq!(p.check_snapshot(f.data()), Err(PositiveFundsError::CompareAndSwap));
    let mut changed = receipt;
    changed.settlement = Some(material(30, 64));
    assert_eq!(p.check_source_snapshot(&[changed]), Err(PositiveFundsError::CompareAndSwap));
    assert_eq!(p.check_source_snapshot(&[]), Err(PositiveFundsError::CompareAndSwap));
}

#[test]
fn panel_attempt_and_accepting_origin_changes_cannot_purchase_another_completion_or_carrier_fee() {
    let mut f = Fixture::new(1);
    f.case.status = ClaimStatus::PositivePassed;
    f.case.positive_witness = Some(WitnessId(d(90)));
    f.case.witness_status = WitnessStatus::Complete;
    let locks = f.locks();
    for kind in [CourtWholeFeeKind::PanelCompletion, CourtWholeFeeKind::Carrier] {
        let first = f.whole(kind, 2, 62);
        let mut second = f.whole(kind, 2, 63);
        if kind == CourtWholeFeeKind::Carrier {
            second.accepted_service.origin.carrier_transaction = first.accepted_service.origin.carrier_transaction;
        }
        let (tx, entries) = f.tx(vec![f.pay(2), f.pay(2), f.refund(96)], None);
        assert_eq!(
            f.calc(&locks, &tx, entries, &[], &[first, second], RefundData::default(), OtherBurnData::default(), 30).err(),
            Some(PositiveFundsError::DuplicateDue)
        );
    }
}

#[test]
fn same_block_clocks_are_exact_while_cross_block_clocks_are_monotone() {
    let before = origin(10, [70; 64]);
    let next_block = AcceptedOrigin { accepting_block: NativeHash64([11; 64]), ..before };
    let cases = [
        ("same event", before, true),
        ("later event", AcceptedOrigin { event_ordinal: 6, ..before }, true),
        ("later transaction", AcceptedOrigin { accepted_tx_ordinal: 5, event_ordinal: 0, ..before }, true),
        ("earlier event", AcceptedOrigin { event_ordinal: 4, ..before }, false),
        ("earlier transaction", AcceptedOrigin { accepted_tx_ordinal: 3, event_ordinal: 6, ..before }, false),
        ("same block higher DAA", AcceptedOrigin { accepted_daa: 11, ..before }, false),
        ("same block lower DAA", AcceptedOrigin { accepted_daa: 9, ..before }, false),
        ("same block higher blue score", AcceptedOrigin { accepted_blue_score: 11, ..before }, false),
        ("same block lower blue score", AcceptedOrigin { accepted_blue_score: 9, ..before }, false),
        ("next block equal clocks", next_block, true),
        (
            "next block increasing clocks and reset ordinals",
            AcceptedOrigin { accepted_daa: 11, accepted_blue_score: 11, accepted_tx_ordinal: 0, event_ordinal: 0, ..next_block },
            true,
        ),
        ("next block higher DAA only", AcceptedOrigin { accepted_daa: 11, ..next_block }, true),
        ("next block higher blue score only", AcceptedOrigin { accepted_blue_score: 11, ..next_block }, true),
        ("next block lower DAA", AcceptedOrigin { accepted_daa: 9, accepted_blue_score: 11, ..next_block }, false),
        ("next block lower blue score", AcceptedOrigin { accepted_daa: 11, accepted_blue_score: 9, ..next_block }, false),
    ];
    for (label, after, expected) in cases {
        assert_eq!(not_after(before, after), expected, "{label}");
    }
}

#[test]
fn contradictory_same_block_settlement_clock_is_rejected_by_the_spend_proposal() {
    let f = Fixture::new(1);
    let locks = f.locks();
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let (tx, entries) = f.tx(vec![f.pay(10), f.refund(90)], None);
    let transaction = PopulatedTransaction::new(&tx, entries);
    // DATA-only contradiction: one accepting block cannot have admission at
    // DAA/blue 10 and settlement at 30, even with a later event ordinal. No
    // accepted-money authority is created by this fixture or the calculator.
    let settlement = AcceptedMaterial {
        origin: AcceptedOrigin {
            accepting_block: f.funding.funding_origin.accepting_block,
            event_ordinal: 6,
            ..origin(30, tx.id().as_bytes())
        },
        digest: d(60),
    };
    let result = propose_positive_spend(PositiveSpendData {
        current: f.data(),
        locks: &locks,
        transaction: &transaction,
        source: &source,
        court_whole: &[],
        recipients: &f.keys,
        refunds: RefundData::default(),
        other_burn: OtherBurnData::default(),
        settlement,
        now: 30,
        max_dues: 20,
    });
    assert_eq!(result.err(), Some(PositiveFundsError::SettlementOrigin));
}

#[test]
fn clocks_cannot_rewind_after_a_previous_settlement_or_before_a_response() {
    let mut f = Fixture::new(1);
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 10);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let (tx, entries) = f.tx(vec![f.pay(10), f.refund(90)], None);
    let p = f.calc(&f.locks(), &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 30).unwrap();
    f.funding = p.after().clone();
    f.case.last_event = p.after().last_settlement.unwrap().origin;
    f.case.usage.fee = 15;
    let receipt = f.receipt(1);
    let due = f.due(&receipt, 5);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let (tx, entries) = f.tx(vec![f.pay(5), f.refund(85)], None);
    assert_eq!(
        f.calc(&f.locks(), &tx, entries.clone(), &source, &[], RefundData::default(), OtherBurnData::default(), 25).err(),
        Some(PositiveFundsError::SettlementOrigin)
    );
    assert_eq!(
        f.calc(&f.locks(), &tx, entries.clone(), &source, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::SettlementOrigin)
    );
    assert!(f.calc(&f.locks(), &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 40).is_ok());
}

#[test]
fn free_source_units_and_court_fees_do_not_increase_source_due_usage_at_settlement() {
    let mut f = Fixture::new(1);
    f.case.usage.fee = 0;
    let receipt = f.receipt(0);
    let due = f.due(&receipt, 0);
    let source = [SourcePaymentData { due: &due, expected_receipt: &receipt }];
    let (tx, entries) = f.tx(vec![f.refund(100)], None);
    assert_eq!(
        f.calc(&f.locks(), &tx, entries, &source, &[], RefundData::default(), OtherBurnData::default(), 30).err(),
        Some(PositiveFundsError::NoValueMovement)
    );
    let whole = [f.whole(CourtWholeFeeKind::Control, 3, 62)];
    let (tx, entries) = f.tx(vec![f.pay(3), f.refund(97)], None);
    let p = f.calc(&f.locks(), &tx, entries, &source, &whole, RefundData::default(), OtherBurnData::default(), 30).unwrap();
    assert_eq!((p.source_fee_paid(), p.court_fee_paid(), f.case.usage.fee), (0, 3, 0));
}
