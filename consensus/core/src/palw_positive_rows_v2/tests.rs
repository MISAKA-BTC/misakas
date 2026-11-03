//! Independent aux151 codec, resource preflight and restore counterexamples.
//! Signatures identify the committed DTOs; these tests construct no native
//! admission, computation, coin-lock or credit authority.
use super::*;
use misaka_palw_positive_contract::{ContractError, identity::*, messages::*, native::NativeRowKey, rows::*};
use std::collections::BTreeMap;

pub(super) fn d(n: u8) -> Digest {
    Digest([n; 64])
}
pub(super) fn origin() -> AcceptedOrigin {
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
pub(super) fn origin_at(event_ordinal: u32) -> AcceptedOrigin {
    // All fixture events share one actual accepting block and therefore the
    // same native DAA/blue-score coordinates. Only event order advances.
    AcceptedOrigin { event_ordinal, ..origin() }
}
pub(super) fn material_at(event_ordinal: u32) -> AcceptedMaterial {
    AcceptedMaterial { origin: origin_at(event_ordinal), digest: d(8) }
}
pub(super) fn material() -> AcceptedMaterial {
    material_at(origin().event_ordinal)
}
pub(super) fn coverage() -> CoverageRow {
    CoverageRow {
        witness: WitnessId(d(1)),
        case: CaseId(d(2)),
        result: ResultId(d(3)),
        plan: PlanId(d(4)),
        mandatory_relations: 10,
        next_relation: 0,
        frame_bytes: 0,
        signatures: 0,
        elements: 0,
        terms: 0,
        state: CoverageState::Checking,
        last_event: origin(),
    }
}
pub(crate) fn signed_bundle() -> (ClaimIndexRow, ResultRow, CaseRow, FundingRow, WitnessRow, CoverageRow) {
    signed_bundle_for_relations(10)
}

#[test]
fn p_terminal_case_requires_its_exact_result_scope_and_liability() {
    use crate::{
        palw_positive_certification_v2::{
            PositiveCertificationHeaderV2, check_positive_certification_result_links_v2, positive_certification_case_live_v2,
            positive_certification_policy_id_v2,
        },
        palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
    };

    let (_, result, mut case, _, _, _) = signed_bundle();
    let r = &result.result;
    let policy = PalwSecurityCertificationPolicyV1 {
        m: 1,
        k: 1,
        veto: 0,
        anchor_delay_spans: 1,
        attest_window_daa: 5,
        prosecution_window_daa: 19,
        certify_by_daa: 7,
        verifier_maturity_daa: 3,
        verifier_min_collateral_sompi: 1,
        draws_per_epoch: 4,
        epoch_daa: 11,
    };
    let header = PositiveCertificationHeaderV2 {
        version: 2,
        revision: 1,
        assignment: misaka_palw_positive_contract::assignment::AssignmentId(result.role_assignment.digest),
        subject: misaka_palw_positive_contract::native::certification_subject(r),
        claim: r.claim_id(),
        result: r.id(),
        case: r.case_id(),
        scope: r.scope,
        policy,
        policy_id: positive_certification_policy_id_v2(&policy),
        commit_origin: result.origin,
        committed_span: Some(2),
        worker: NativeOutpoint { transaction: NativeHash64([82; 64]), index: 0 },
        worker_operator: d(83),
        worker_key: result.miner_key,
        worker_gain: 100,
        liability: case.liability,
    };
    {
        use crate::palw_positive_certification_v2::{
            POSITIVE_CERTIFICATION_TABLE_V2, PositiveCertificationFixedV2, PositiveCertificationOutcomeV2, PositiveCertificationRowV2,
        };
        let mut key = vec![b'P'];
        key.extend_from_slice(&header.subject.0);
        let key = (POSITIVE_CERTIFICATION_TABLE_V2, key);
        let mut p = PositiveCertificationRowV2 {
            fixed: PositiveCertificationFixedV2 { header: header.clone(), finality: None, outcome: None, liability_release: None },
            draw: None,
            votes: Vec::new(),
        };
        let pending = borsh::to_vec(&p).unwrap();
        let last = header.commit_origin.accepted_daa + header.policy.certify_by_daa;
        let mut route = empty_route();
        route.update_positive_aux_indexes_v2(&key, Some(&pending)).unwrap();
        route.aux.insert(key.clone(), pending.clone());
        assert_eq!(route.positive_draw_slots_v2(last, 4), 1);
        assert_eq!(route.positive_draw_slots_v2(last + 1, 4), 0);
        let subject = crate::Hash64::from_bytes(header.subject.0);
        assert_eq!(route.due_positive_certification_subjects_v2(last).next(), None);
        assert!(route.due_positive_certification_subjects_v2(last + 1).eq([subject]));
        let bytes = borsh::to_vec(&route).unwrap();
        let mut restored: crate::palw_kernel_route_v1::PalwKernelRouteStateV1 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(restored.positive_draw_slots_v2(last, 4), 1);
        p.fixed.outcome = Some((PositiveCertificationOutcomeV2::Expired, last + 1));
        let expired = borsh::to_vec(&p).unwrap();
        restored.update_positive_aux_indexes_v2(&key, Some(&expired)).unwrap();
        restored.aux.insert(key.clone(), expired);
        assert_eq!(restored.positive_draw_slots_v2(last, 4), 0);
        assert_eq!(restored.due_positive_certification_subjects_v2(last + 1).next(), None);
        restored.update_positive_aux_indexes_v2(&key, Some(&pending)).unwrap();
        restored.aux.insert(key, pending);
        assert_eq!(borsh::to_vec(&restored).unwrap(), bytes);
        assert_eq!(restored.positive_draw_slots_v2(last, 4), 1);
        assert!(restored.due_positive_certification_subjects_v2(last + 1).eq([subject]));
        assert!(restored.undrawn_positive_subjects_v2(last).eq([subject]));
        assert_eq!(restored.undrawn_positive_subjects_v2(last + 1).next(), None);
    }
    {
        use crate::{
            palw_g14_lane_closure_v1::palw_kernel_signer_lock_amount_v1,
            palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
            palw_positive_certification_v2::*,
            palw_security_certification_v1::palw_certification_seed_v1,
            palw_state_v2::PalwBondKeyV2,
            tx::TransactionOutpoint,
        };
        use misaka_palw_positive_contract::certification::{PositiveCourtAttestationV2, PositiveCourtVerdictV2};
        let mut h = header.clone();
        h.policy.m = 3;
        h.policy.k = 2;
        h.policy.veto = 2;
        h.policy_id = positive_certification_policy_id_v2(&h.policy);
        let at = h.commit_origin.accepted_daa;
        let deadline = at + h.policy.attest_window_daa;
        let anchor_span = h.committed_span.unwrap() + h.policy.anchor_delay_spans;
        let verifier_bond = PalwBondKeyV2(TransactionOutpoint::new(crate::Hash64::from_bytes([86; 64]), 0));
        let verifier = Digest(palw_kernel_bond_id_v1(&verifier_bond));
        let execution_key = NativeHash64([88; 64]);
        let mut draw = PositiveCertificationDrawV2 {
            meta: PositiveCertificationDrawMetaV2 {
                roster: d(87),
                roster_count: 1,
                roster_stake: 1,
                seed: Digest(palw_certification_seed_v1(&h.subject.0, anchor_span, &execution_key.0)),
                anchor_span,
                anchor_block: NativeHash64([89; 64]),
                execution_key,
                origin: PositiveCertificationDrawOriginV2 { block: h.commit_origin.accepting_block, daa: at, blue_score: 7 },
                deadline_daa: deadline,
                id: d(0),
            },
            positions: vec![verifier; 3],
        };
        draw.meta.id = draw.expected_id(&h);
        let finality = PositiveCertificationFinalV2 { origin: origin(), witness: WitnessId(d(90)), material: material() };
        let key = (POSITIVE_CERTIFICATION_TABLE_V2, positive_certification_key_v2(h.subject));
        let mut p = PositiveCertificationRowV2 {
            fixed: PositiveCertificationFixedV2 {
                header: h.clone(),
                finality: Some(finality),
                outcome: None,
                liability_release: None,
            },
            draw: Some(draw),
            votes: Vec::new(),
        };
        let install = |route: &mut PalwKernelRouteStateV1, p: &PositiveCertificationRowV2| {
            p.validate_shape().unwrap();
            let bytes = borsh::to_vec(p).unwrap();
            route.update_positive_aux_indexes_v2(&key, Some(&bytes)).unwrap();
            route.aux.insert(key.clone(), bytes);
        };
        let mut route = empty_route();
        let subject = crate::Hash64::from_bytes(h.subject.0);
        install(&mut route, &p);
        assert_eq!(route.positive_draw_slots_v2(at, 4), 0);
        assert_eq!(route.undrawn_positive_subjects_v2(at).next(), None);
        assert_eq!(route.due_positive_certification_subjects_v2(deadline).next(), None);
        let before_vote = borsh::to_vec(&route).unwrap();
        let restored: PalwKernelRouteStateV1 = borsh::from_slice(&before_vote).unwrap();
        assert!(restored.due_positive_certification_subjects_v2(deadline + 1).eq([subject]));
        assert_eq!(positive_certification_failure_v2(&p, deadline + 1, true), Some(PositiveCertificationOutcomeV2::NotCertified));
        let voter_key = [0; PUBKEY_BYTES];
        p.votes.push(PositiveCertificationVoteV2 {
            bond: NativeOutpoint { transaction: NativeHash64([86; 64]), index: 0 },
            key: voter_key,
            statement: PositiveCourtAttestationV2 {
                version: 2,
                revision: 1,
                scope: h.scope,
                claim: h.claim,
                result: h.result,
                case: h.case,
                subject: h.subject,
                policy: h.policy_id,
                draw_anchor: p.draw.as_ref().unwrap().meta.id,
                witness: finality.witness,
                verifier_bond: verifier,
                verifier: principal(&voter_key),
                verdict: PositiveCourtVerdictV2::PositiveChecked,
                checked_material: finality.material.digest,
            },
            signature: [0; SIGNATURE_BYTES],
            origin: origin(),
            liability_amount: palw_kernel_signer_lock_amount_v1(h.worker_gain, h.policy.k as u8),
        });
        install(&mut route, &p);
        assert_eq!(p.positive_positions(), 3);
        assert_eq!(route.due_positive_certification_subjects_v2(deadline + 1).next(), None);
        assert_eq!(positive_certification_failure_v2(&p, deadline + 1, true), None);
        let prosecution = finality.origin.accepted_daa + h.policy.prosecution_window_daa;
        assert_eq!(route.due_positive_quorum_subjects_v2(prosecution - 1).next(), None);
        assert!(route.due_positive_quorum_subjects_v2(prosecution).eq([subject]));
        let restored: PalwKernelRouteStateV1 = borsh::from_slice(&borsh::to_vec(&route).unwrap()).unwrap();
        assert!(restored.due_positive_quorum_subjects_v2(prosecution).eq([subject]));

        p.votes[0].statement.verdict = PositiveCourtVerdictV2::NotReproducible;
        p.votes[0].liability_amount = 0;
        install(&mut route, &p);
        assert_eq!(route.due_positive_certification_subjects_v2(at - 1).next(), None);
        assert!(route.due_positive_certification_subjects_v2(at).eq([subject]));
        assert_eq!(route.due_positive_quorum_subjects_v2(u64::MAX).next(), None);
        assert_eq!(positive_certification_failure_v2(&p, at, true), Some(PositiveCertificationOutcomeV2::Vetoed));
        p.votes.clear();
        install(&mut route, &p);
        assert_eq!(borsh::to_vec(&route).unwrap(), before_vote);
        assert!(route.due_positive_certification_subjects_v2(deadline + 1).eq([subject]));
    }
    assert_eq!(positive_certification_case_live_v2(&header, &case), Ok(true));
    assert_eq!(check_positive_certification_result_links_v2(&header, &result, &case), Ok(()));
    let mut foreign = header.clone();
    foreign.commit_origin.accepted_tx_ordinal += 1;
    assert_eq!(check_positive_certification_result_links_v2(&foreign, &result, &case), Err(ContractError::Identity));
    foreign = header.clone();
    foreign.assignment = misaka_palw_positive_contract::assignment::AssignmentId(d(81));
    assert_eq!(check_positive_certification_result_links_v2(&foreign, &result, &case), Err(ContractError::Identity));
    foreign = header.clone();
    foreign.worker_key[0] ^= 1;
    assert_eq!(check_positive_certification_result_links_v2(&foreign, &result, &case), Err(ContractError::Identity));
    foreign = header.clone();
    foreign.subject = d(84);
    assert_eq!(check_positive_certification_result_links_v2(&foreign, &result, &case), Err(ContractError::Identity));
    for status in [ClaimStatus::ClaimRefuted, ClaimStatus::Revoked, ClaimStatus::EndedUnverified] {
        case.status = status;
        assert_eq!(positive_certification_case_live_v2(&header, &case), Ok(false));
        assert_eq!(check_positive_certification_result_links_v2(&header, &result, &case), Ok(()));
        let mut other_output = r.clone();
        other_output.output = d(85);
        assert_eq!(other_output.case_id(), r.case_id());
        assert_ne!(other_output.id(), r.id());
        let other =
            CaseRow { result: other_output.id(), coverage: CoverageDescriptor { result: other_output.id(), ..case.coverage }, ..case };
        assert_eq!(positive_certification_case_live_v2(&header, &other), Err(ContractError::Identity));
        let other =
            CaseRow { coverage: CoverageDescriptor { scope: ChainScope { branch: d(86), ..r.scope }, ..case.coverage }, ..case };
        assert_eq!(positive_certification_case_live_v2(&header, &other), Err(ContractError::Identity));
        let other = CaseRow { liability: material_at(99), ..case };
        assert_eq!(positive_certification_case_live_v2(&header, &other), Err(ContractError::Identity));
    }
}

/// Real signatures over a chosen literal relation count. Large counts are
/// hostile DTO fixtures, not admitted execution profiles or native quotes.
pub(super) fn signed_bundle_for_relations(mandatory: u64) -> (ClaimIndexRow, ResultRow, CaseRow, FundingRow, WitnessRow, CoverageRow) {
    use libcrux_ml_dsa::ml_dsa_87::*;
    use misaka_palw_positive_contract::funding::*;
    let json: serde_json::Value =
        serde_json::from_str(include_str!("../../../../docs/design/palw/positive-court/contract-vectors-r1.json")).unwrap();
    let hex = json["result_borsh_hex"].as_str().unwrap();
    let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    let mut r: MinerResultCommit = borsh::from_slice(&bytes).unwrap();
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
            unique_source_bytes: 8192,
            transmitted_bytes: 16384,
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
        mandatory_relations: mandatory,
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
    let plan = coverage_plan_id(&r, basis.profile, basis.derivation, mandatory).unwrap();
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
        origin: origin_at(6),
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
            mandatory_relations: mandatory,
            reserved_elements: 100,
            reserved_terms: 200,
            max_part_bytes: 10000,
        },
        status: ClaimStatus::PositivePassed,
        miner_data: MinerDataStatus::Pending,
        witness_status: WitnessStatus::Complete,
        positive_witness: Some(w.id()),
        usage: AggregateUsage {
            unique_source_bytes: 4096,
            transmitted_bytes: 4096,
            additional_work: 1,
            witness_attempts: 1,
            ..AggregateUsage::default()
        },
        reservation: material(),
        liability: material(),
        maturity_daa: 100,
        unresolved_fraud: 0,
        last_event: origin_at(19),
    };
    let coverage = CoverageRow {
        witness: w.id(),
        case: r.case_id(),
        result: r.id(),
        plan,
        mandatory_relations: mandatory,
        next_relation: mandatory,
        frame_bytes: 1373u64.checked_mul(mandatory).unwrap_or(u64::MAX),
        signatures: mandatory,
        elements: 100,
        terms: 200,
        state: CoverageState::Matched,
        last_event: origin_at(16),
    };
    (ClaimIndexRow { claim: r.claim_id(), result: r.id(), case: r.case_id() }, result, case, funding, witness, coverage)
}

pub(super) fn row_kind(body: &NativeRowBody) -> u8 {
    match body {
        NativeRowBody::Result(_) => 0,
        NativeRowBody::ClaimIndex(_) => 1,
        NativeRowBody::Case(_) => 2,
        NativeRowBody::Witness(_) => 3,
        NativeRowBody::Coverage(_) => 4,
        NativeRowBody::SourceUnion(_) => 5,
        NativeRowBody::ServiceReceipt(_) => 6,
        NativeRowBody::Funding(_) => 7,
        NativeRowBody::PacketReceipt(_) => 8,
        NativeRowBody::JournalHead(_) => 9,
        NativeRowBody::Assignment(_) => 10,
        NativeRowBody::AssignmentNonce(_) => 11,
        NativeRowBody::AssignmentUse(_) => 12,
        NativeRowBody::AssignmentFunding(_) => 13,
        NativeRowBody::AssignmentSettlement(_) => 14,
        NativeRowBody::AssignmentRole(_) => 15,
        NativeRowBody::AssignmentJournalHead(_) => 16,
        NativeRowBody::AssignmentJournalFrame(_) => 17,
        NativeRowBody::AssignmentProverFunding(_) => 18,
        NativeRowBody::FundingRelayAuthorization(_) => 19,
        NativeRowBody::FundingRelayCustody(_) => 20,
    }
}

pub(super) fn all_rows() -> Vec<NativeRowValue> {
    let (index, result, case, funding, witness, coverage) = signed_bundle();
    let source = case.service.source_catalog;
    // Ten successful, signed one-relation events, with distinct accepted
    // positions and frame identities. A Matched cache never replaces them.
    let mut bodies = vec![
        NativeRowBody::Result(result.clone()),
        NativeRowBody::ClaimIndex(index),
        NativeRowBody::Case(case),
        NativeRowBody::Witness(witness.clone()),
        NativeRowBody::Coverage(coverage),
        NativeRowBody::SourceUnion(SourceUnionRow {
            case: case.case,
            kind: SourceKind::Input,
            source,
            source_units: 9,
            requested_units: 5,
            completed_units: 1,
            union: material_at(17),
        }),
        NativeRowBody::ServiceReceipt(ServiceReceiptRow {
            case: case.case,
            kind: SourceKind::Input,
            source,
            unit: 4,
            result: result.result.id(),
            duty: material_at(17),
            response: material_at(18),
            provider: PrincipalId(d(66)),
            bytes: 4096,
            additional_work: 1,
            settlement: Some(material_at(18)),
        }),
        NativeRowBody::Funding(funding),
        NativeRowBody::JournalHead(JournalHeadRow {
            case: case.case,
            sequence: 15,
            previous: d(95),
            head: d(96),
            history: material_at(19),
        }),
    ];
    for first_relation in 0..10 {
        bodies.push(NativeRowBody::PacketReceipt(PacketReceiptRow {
            witness: witness.witness.id(),
            first_relation,
            frame: EvidencePartId(d(77 + first_relation as u8)),
            author: witness.witness.author,
            signed_bytes: 6000,
            origin: origin_at(7 + first_relation as u32),
        }));
    }
    bodies.into_iter().map(|body| NativeRowValue::new(body).unwrap()).collect()
}
pub(super) fn encoded_rows(rows: &[NativeRowValue]) -> Vec<(u8, Vec<u8>, Vec<u8>)> {
    rows.iter().map(|row| (151, row.body.key().encode(), row.encode().unwrap())).collect()
}
fn encoded_map(rows: &[NativeRowValue]) -> BTreeMap<(u8, Vec<u8>), Vec<u8>> {
    encoded_rows(rows).into_iter().map(|(table, key, value)| ((table, key), value)).collect()
}
#[test]
fn all_ten_native_row_kinds_roundtrip_with_canonical_typed_keys_and_real_signatures() {
    let rows = all_rows();
    assert_eq!(rows.len(), 19);
    let mut kinds = 0u16;
    for row in &rows {
        let kind = row_kind(&row.body);
        kinds |= 1 << kind;
        let key = row.body.key().encode();
        let value = row.encode().unwrap();
        assert_eq!(key[4], kind);
        assert_eq!(value[4], kind);
        assert_eq!(NativeRowValue::decode(&key, &value), Ok(row.clone()));
        let mut bad_key = key.clone();
        bad_key[0] = 1;
        assert_eq!(NativeRowValue::decode(&bad_key, &value), Err(ContractError::Version));
        let mut unknown = key.clone();
        unknown[4] = 255;
        assert!(NativeRowValue::decode(&unknown, &value).is_err());
        let mut other_id = key.clone();
        other_id[5] ^= 1;
        assert_eq!(NativeRowValue::decode(&other_id, &value), Err(ContractError::Identity));
        let mut other_kind = key.clone();
        other_kind[4] = (kind as u8 + 1) % 10;
        assert!(NativeRowValue::decode(&other_kind, &value).is_err());
        let mut unknown_value = value.clone();
        unknown_value[4] = 255;
        assert!(NativeRowValue::decode(&key, &unknown_value).is_err());
        let mut bad_value_version = value.clone();
        bad_value_version[2] ^= 1;
        assert_eq!(NativeRowValue::decode(&key, &bad_value_version), Err(ContractError::Version));
        let mut tail = value.clone();
        tail.push(0);
        assert_eq!(NativeRowValue::decode(&key, &tail), Err(ContractError::NonCanonical));
        assert!(NativeRowValue::decode(&key, &value[..value.len() - 1]).is_err());
    }
    assert_eq!(kinds, 0b11_1111_1111, "all ten allocated native row kinds remain covered");
    let (index, result, case, funding, witness, coverage) = signed_bundle();
    validate_bundle(&index, &result, &case, &funding).unwrap();
    validate_witness_bundle(&result, &case, &witness, &coverage).unwrap();
}

fn scan_row(key: &[u8], value: &[u8]) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
    use crate::palw_native_resource_v1::NativeWireReaderV1;
    let mut frame = NativeWireReaderV1::new(value, value.len() as u64);
    let shape = crate::palw_native_resource_v1::positive_rows::scan_positive_aux_row_v1(key, &mut frame)?;
    assert_eq!(frame.remaining(), 0);
    Ok(shape)
}
pub(crate) fn empty_route() -> crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    use crate::palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_route_policy_v1};
    PalwKernelRouteStateV1::new(
        palw_kernel_route_policy_v1(crate::Hash64::from_u64_word(61), crate::Hash64::from_u64_word(62)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1::default(),
    )
}
#[test]
fn positive_job_projection_tracks_result_install_remove_and_cold_decode() {
    use crate::Hash64;
    use misaka_palw_positive_contract::native::KERNEL_AUX_TABLE;
    let (_, result, ..) = signed_bundle();
    let job = Hash64::from_bytes(result.result.job.0);
    let value = NativeRowValue::new(NativeRowBody::Result(result)).unwrap();
    let key = (KERNEL_AUX_TABLE, value.body.key().encode());
    let bytes = value.encode().unwrap();
    let mut route = empty_route();
    let original = borsh::to_vec(&route).unwrap();
    assert!(!route.contains_positive_job_v2(job));
    assert!(route.update_positive_aux_indexes_v2(&key, Some(&[0])).is_err());
    assert!(!route.contains_positive_job_v2(job));

    route.update_positive_aux_indexes_v2(&key, Some(&bytes)).unwrap();
    route.aux.insert(key.clone(), bytes.clone());
    assert!(route.contains_positive_job_v2(job));
    assert!(!route.contains_positive_job_v2(Hash64::from_bytes([u8::MAX; 64])));
    let committed = borsh::to_vec(&route).unwrap();
    let mut restored: crate::palw_kernel_route_v1::PalwKernelRouteStateV1 = borsh::from_slice(&committed).unwrap();
    assert!(restored.contains_positive_job_v2(job));
    assert_eq!(restored.aux_root(), route.aux_root());
    restored.update_positive_aux_indexes_v2(&key, None).unwrap();
    restored.aux.remove(&key);
    assert!(!restored.contains_positive_job_v2(job));
    assert_eq!(borsh::to_vec(&restored).unwrap(), original);
}

#[test]
fn fixed_enum_memory_is_charged_per_short_row_and_exact_limits_are_enforced() {
    use crate::palw_native_resource_v1::{
        NativeDemandCheckerV1, NativeHeapV1, NativeSnapshotResourceLimitsV1, preflight_native_value_v1,
    };
    use std::mem::size_of;
    let rows = all_rows();
    let index = rows.iter().find(|row| matches!(&row.body, NativeRowBody::ClaimIndex(_))).unwrap();
    let key = index.body.key().encode();
    let bytes = index.encode().unwrap();
    let retained = size_of::<NativeRowValue>() as u64;
    let workspace =
        (MAX_NATIVE_ROW_BYTES + MAX_NATIVE_KEY_BYTES + 8 * size_of::<NativeRowValue>() + 2 * size_of::<NativeRowKey>()) as u64;
    let attempt_index = size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<([u8; 64], u32), ()>>() as u64;
    let expected = retained + workspace + attempt_index;
    assert!((bytes.len() as u64) < retained, "a small ClaimIndex cannot price a large native enum by encoded length");
    for row in &rows {
        assert_eq!(row.native_heap_v1("aux151 fixed DTO").bytes(), Some(0));
        let encoded = row.encode().unwrap();
        let key = row.body.key().encode();
        let scanned = scan_row(&key, &encoded).unwrap();
        assert_eq!(scanned.heap, expected);
        let standalone = preflight_native_value_v1::<NativeRowValue>(&encoded[..], encoded.len() as u64).unwrap();
        assert_eq!(standalone.logical_requests_v1(), retained);
    }
    let shape = scan_row(&key, &bytes).unwrap();
    let short = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: shape.heap - 1, database_bytes: 0 });
    assert!(short.reserve_requests_v1(shape.heap, 0).is_err());
    assert_eq!(short.outstanding(), (0, 0));
    let exact = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: shape.heap, database_bytes: 0 });
    let lease = exact.reserve_requests_v1(shape.heap, 0).unwrap();
    assert_eq!(exact.outstanding(), (shape.heap, 0));
    assert!(exact.reserve_requests_v1(1, 0).is_err());
    drop(lease);
    assert_eq!(exact.outstanding(), (0, 0));
    let mut route = empty_route();
    route.aux.insert((151, key.clone()), bytes.clone());
    let raw = route.header.native_heap_v1("header").bytes().unwrap()
        + route.rows.native_heap_v1("raw ledger").bytes().unwrap()
        + route.aux.native_heap_v1("raw aux").bytes().unwrap()
        + route.ledger_cache.native_heap_v1("cache").bytes().unwrap();
    assert_eq!(route.native_heap_v1("typed route").bytes(), Some(raw + expected));
    // A second fixed-width row independently consumes another typed slot and
    // decode workspace, even though its byte row remains only a few hundred bytes.
    let second = NativeRowValue::new(NativeRowBody::ClaimIndex(ClaimIndexRow {
        claim: ClaimId(d(201)),
        result: ResultId(d(202)),
        case: CaseId(d(203)),
    }))
    .unwrap();
    route.aux.insert((151, second.body.key().encode()), second.encode().unwrap());
    let raw = route.header.native_heap_v1("header").bytes().unwrap()
        + route.rows.native_heap_v1("raw ledger").bytes().unwrap()
        + route.aux.native_heap_v1("raw aux").bytes().unwrap()
        + route.ledger_cache.native_heap_v1("cache").bytes().unwrap();
    assert_eq!(route.native_heap_v1("typed route").bytes(), Some(raw + 2 * expected));
}
#[test]
fn key_aware_preflight_rejects_hostile_enum_option_lengths_and_every_truncated_prefix() {
    use crate::palw_native_resource_v1::NativeWireReaderV1;
    use std::{cell::Cell, io::Read};
    let rows = all_rows();
    for row in &rows {
        let key = row.body.key().encode();
        let bytes = row.encode().unwrap();
        for len in [0, 1, 4, bytes.len() - 1] {
            assert!(scan_row(&key, &bytes[..len]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(scan_row(&key, &trailing).is_err());
        let mut unknown = bytes.clone();
        unknown[4] = 255;
        assert!(scan_row(&key, &unknown).is_err());
        let mut wrong_key = key.clone();
        wrong_key[5] ^= 1;
        assert!(scan_row(&wrong_key, &bytes).is_err());
        let mut wrong_version = key.clone();
        wrong_version[0] ^= 1;
        assert!(scan_row(&wrong_version, &bytes).is_err());
    }
    let index = rows.iter().find(|row| matches!(&row.body, NativeRowBody::ClaimIndex(_))).unwrap();
    let key = index.body.key().encode();
    let bytes = index.encode().unwrap();
    for len in 0..bytes.len() {
        assert!(scan_row(&key, &bytes[..len]).is_err());
    }
    let mut hostile_key = key.clone();
    hostile_key[4] = 255;
    assert!(scan_row(&hostile_key, &bytes).is_err());
    assert!(scan_row(&vec![0; MAX_NATIVE_KEY_BYTES + 1], &bytes).is_err());
    let funding = rows.iter().find_map(|r| if let NativeRowBody::Funding(f) = &r.body { Some(f) } else { None }).unwrap();
    let funding_row = rows.iter().find(|r| matches!(&r.body, NativeRowBody::Funding(_))).unwrap();
    let mut invalid_option = funding_row.encode().unwrap();
    let current_outpoint_tag = 5
        + borsh::object_length(&funding.consent).unwrap()
        + borsh::object_length(&funding.basis).unwrap()
        + PUBKEY_BYTES
        + SIGNATURE_BYTES
        + borsh::object_length(&funding.funding_origin).unwrap()
        + borsh::object_length(&funding.original_outpoint).unwrap();
    assert_eq!(invalid_option[current_outpoint_tag], 1);
    invalid_option[current_outpoint_tag] = 255;
    assert!(scan_row(&funding_row.body.key().encode(), &invalid_option).is_err());
    struct Probe<'a>(&'a Cell<usize>);
    impl Read for Probe<'_> {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            self.0.set(self.0.get() + 1);
            Ok(0)
        }
    }
    let reads = Cell::new(0);
    let mut oversized = NativeWireReaderV1::new(Probe(&reads), (MAX_NATIVE_ROW_BYTES + 1) as u64);
    assert!(crate::palw_native_resource_v1::positive_rows::scan_positive_aux_row_v1(&key, &mut oversized).is_err());
    assert_eq!(reads.get(), 0, "oversized frame must be rejected before reading or allocating payload");
}
#[test]
fn native_route_scanner_rejects_duplicate_and_unsorted_positive_keys_before_collection() {
    use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
    use crate::palw_native_resource_v1::preflight_native_value_v1;
    let row = all_rows().into_iter().find(|row| matches!(&row.body, NativeRowBody::ClaimIndex(_))).unwrap();
    let first = ((151u8, row.body.key().encode()), row.encode().unwrap());
    let other = NativeRowValue::new(NativeRowBody::ClaimIndex(ClaimIndexRow {
        claim: ClaimId(d(201)),
        result: ResultId(d(202)),
        case: CaseId(d(203)),
    }))
    .unwrap();
    let second = ((151u8, other.body.key().encode()), other.encode().unwrap());
    let mut canonical = vec![first.clone(), second];
    canonical.sort_by(|a, b| a.0.cmp(&b.0));
    let route = empty_route();
    let encode = |aux: Vec<((u8, Vec<u8>), Vec<u8>)>| {
        let mut bytes = borsh::to_vec(&route.header).unwrap();
        bytes.extend(borsh::to_vec(&route.rows).unwrap());
        // Vec<entry> and Borsh map use the same count+entry layout. This lets
        // the fixture carry duplicates that collecting a BTreeMap would erase.
        bytes.extend(borsh::to_vec(&aux).unwrap());
        bytes
    };
    let bytes = encode(canonical.clone());
    preflight_native_value_v1::<PalwKernelRouteStateV1>(&bytes[..], bytes.len() as u64).unwrap();
    let direct: PalwKernelRouteStateV1 = borsh::from_slice(&bytes).unwrap();
    assert_eq!(borsh::to_vec(&direct).unwrap(), bytes, "direct Borsh keeps the canonical field layout");
    assert_eq!(direct.aux.len(), 2);
    let duplicate = encode(vec![first.clone(), first]);
    assert!(preflight_native_value_v1::<PalwKernelRouteStateV1>(&duplicate[..], duplicate.len() as u64).is_err());
    assert!(
        borsh::from_slice::<PalwKernelRouteStateV1>(&duplicate).is_err(),
        "direct typed decoding must not erase duplicate aux151 keys"
    );
    canonical.reverse();
    let reversed = encode(canonical);
    assert!(preflight_native_value_v1::<PalwKernelRouteStateV1>(&reversed[..], reversed.len() as u64).is_err());
    assert!(borsh::from_slice::<PalwKernelRouteStateV1>(&reversed).is_err(), "direct decoder must reject descending Positive keys");
}
#[test]
fn typed_restore_rejects_duplicate_orphan_rows_and_missing_atomic_journal_head() {
    use super::restore::{validate_positive_aux_restore_v2, validate_positive_aux_uncollected_v2};
    let rows = all_rows();
    let map = encoded_map(&rows);
    validate_positive_aux_restore_v2(&map).unwrap();
    let mut duplicate = encoded_rows(&rows);
    duplicate.push(duplicate.first().unwrap().clone());
    assert!(validate_positive_aux_uncollected_v2(&duplicate).is_err());
    let mut route = empty_route();
    route.aux = map.clone();
    let valid_served: Vec<_> = map.iter().map(|((table, key), value)| (*table, key.clone(), value.clone())).collect();
    let valid_restored = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(
        &borsh::to_vec(&route.header).unwrap(),
        valid_served,
        &route.ledger_root(),
        &route.aux_root(),
    )
    .unwrap();
    assert_eq!(valid_restored.aux, map);
    let mut missing_head = route.clone();
    missing_head.aux.retain(|(_, key), _| key[4] != 9);
    let missing_served: Vec<_> = missing_head.aux.iter().map(|((table, key), value)| (*table, key.clone(), value.clone())).collect();
    // The served roots themselves match the missing-head data. Failure must
    // come from the installed Positive consistency check, not root mismatch.
    let refusal = crate::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(
        &borsh::to_vec(&missing_head.header).unwrap(),
        missing_served,
        &missing_head.ledger_root(),
        &missing_head.aux_root(),
    )
    .unwrap_err();
    assert!(refusal.contains("inconsistent Positive aux rows"), "{refusal}");
    assert!(refusal.contains("MissingRequiredRow"), "{refusal}");
    let mut ordered: Vec<_> = map.iter().map(|((table, key), value)| (*table, key.clone(), value.clone())).collect();
    ordered.push(ordered.first().unwrap().clone());
    assert!(
        crate::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(
            &borsh::to_vec(&route.header).unwrap(),
            ordered,
            &route.ledger_root(),
            &route.aux_root()
        )
        .is_err()
    );
    for kind in [0u8, 1, 2, 3, 4, 7, 9] {
        let mut missing = map.clone();
        missing.retain(|(_, key), _| key[4] != kind);
        assert!(validate_positive_aux_restore_v2(&missing).is_err(), "orphan/missing row kind {kind} was silently restored");
    }
    let mut receipt_orphan = map.clone();
    receipt_orphan.retain(|(_, key), _| key[4] != 5);
    assert!(validate_positive_aux_restore_v2(&receipt_orphan).is_err());
    let mut receipt_omitted = map;
    receipt_omitted.retain(|(_, key), _| key[4] != 6);
    assert!(validate_positive_aux_restore_v2(&receipt_omitted).is_err(), "union summary must not invent omitted service receipts");
}
#[test]
fn semantic_corruption_does_not_become_a_missing_row_or_a_smaller_resource_quote() {
    use super::restore::validate_positive_aux_restore_v2;
    let rows = all_rows();
    for kind in [4u8, 5] {
        let mut bad = rows.clone();
        let row = bad.iter_mut().find(|r| r.body.key().encode()[4] == kind).unwrap();
        match &mut row.body {
            NativeRowBody::Coverage(c) => {
                c.next_relation = c.mandatory_relations - 1;
                c.state = CoverageState::Matched;
            }
            NativeRowBody::SourceUnion(u) => {
                u.completed_units = u.requested_units + 1;
            }
            _ => unreachable!(),
        }
        // Serialize the public DTO directly: canonical bytes can still carry
        // an invalid logical row, which every native key-aware path must refuse.
        let key = row.body.key().encode();
        let bytes = borsh::to_vec(row).unwrap();
        assert!(NativeRowValue::decode(&key, &bytes).is_err());
        assert!(scan_row(&key, &bytes).is_err());
        let mut map = encoded_map(&rows);
        map.insert((151, key), bytes);
        assert!(validate_positive_aux_restore_v2(&map).is_err());
    }
    let mut bad_signature = rows.clone();
    if let Some(r) = bad_signature.iter_mut().find_map(|row| match &mut row.body {
        NativeRowBody::Result(result) => Some(result),
        _ => None,
    }) {
        r.miner_signature[0] ^= 1;
    }
    let map = encoded_map(&bad_signature);
    assert!(validate_positive_aux_restore_v2(&map).is_err(), "shape-valid signatures must still be reauthenticated");
    let mut wrong_plan = rows.clone();
    if let Some(c) = wrong_plan.iter_mut().find_map(|row| match &mut row.body {
        NativeRowBody::Coverage(coverage) => Some(coverage),
        _ => None,
    }) {
        c.plan = PlanId(d(199));
    }
    assert!(validate_positive_aux_restore_v2(&encoded_map(&wrong_plan)).is_err());
}
