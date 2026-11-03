//! Data/cryptographic-statement fixtures only; no native accepted P success,
//! current session, replay, collateral lease or worker payment is claimed.
use kaspa_consensus_core::{
    TransactionId, palw_kernel_route_v1::palw_kernel_bond_id_v1, palw_native_resource_v1::preflight_native_value_v1,
    palw_positive_certification_v2::*, palw_security_certification_v1::*, palw_state_v2::PalwBondKeyV2, tx::TransactionOutpoint,
};
use misaka_palw_positive_contract::{
    assignment::AssignmentId, certification::*, identity::*, messages::MinerResultCommit, native::certification_subject, rows::*,
};
fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn origin(daa: u64) -> AcceptedOrigin {
    AcceptedOrigin {
        accepting_block: NativeHash64([daa as u8; 64]),
        carrier_block: NativeHash64([daa as u8; 64]),
        carrier_transaction: NativeHash64([daa as u8; 64]),
        accepted_tx_ordinal: 0,
        event_ordinal: 0,
        accepted_daa: daa,
        accepted_blue_score: daa,
    }
}
fn policy(m: u16, k: u16) -> PalwSecurityCertificationPolicyV1 {
    PalwSecurityCertificationPolicyV1 {
        m,
        k,
        veto: 0,
        anchor_delay_spans: 2,
        attest_window_daa: 5,
        prosecution_window_daa: 19,
        certify_by_daa: 7,
        verifier_maturity_daa: 3,
        verifier_min_collateral_sompi: 1,
        draws_per_epoch: 4,
        epoch_daa: 11,
    }
}
fn result() -> MinerResultCommit {
    MinerResultCommit {
        version: 2,
        revision: 1,
        scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
        job: d(4),
        execution: d(5),
        miner: principal(&[25; PUBKEY_BYTES]),
        registered_class: d(7),
        artifact: d(8),
        program: d(9),
        input: d(10),
        tokenizer: d(11),
        execution_rules: d(12),
        selector: misaka_palw_positive_contract::tir::greedy_selector_id(),
        generation_seed: [14; 32],
        temperature_q24: 0,
        prefill: 2,
        decode: 3,
        max_context: 8192,
        output: d(15),
        dependencies: d(16),
        service_contract: d(17),
    }
}
fn row(m: u16, k: u16) -> PositiveCertificationRowV2 {
    let result = result();
    let policy = policy(m, k);
    PositiveCertificationRowV2 {
        fixed: PositiveCertificationFixedV2 {
            header: PositiveCertificationHeaderV2 {
                version: 2,
                revision: 1,
                assignment: AssignmentId(d(18)),
                subject: certification_subject(&result),
                claim: result.claim_id(),
                result: result.id(),
                case: result.case_id(),
                scope: result.scope,
                policy,
                policy_id: positive_certification_policy_id_v2(&policy),
                commit_origin: origin(100),
                committed_span: Some(2),
                worker: NativeOutpoint { transaction: NativeHash64([24; 64]), index: 0 },
                worker_operator: d(24),
                worker_key: [25; PUBKEY_BYTES],
                worker_gain: 100,
                liability: AcceptedMaterial { origin: origin(100), digest: d(26) },
            },
            finality: None,
            outcome: None,
            liability_release: None,
        },
        draw: None,
        votes: Vec::new(),
    }
}
fn drawn(mut row: PositiveCertificationRowV2, positions: Vec<Digest>) -> PositiveCertificationRowV2 {
    let mut draw = PositiveCertificationDrawV2 {
        meta: PositiveCertificationDrawMetaV2 {
            roster: d(30),
            roster_count: if positions.is_empty() { 0 } else { 1 },
            roster_stake: if positions.is_empty() { 0 } else { 1000 },
            seed: Digest(palw_certification_seed_v1(&row.fixed.header.subject.0, 4, &[33; 64])),
            anchor_span: 4,
            anchor_block: NativeHash64([32; 64]),
            execution_key: NativeHash64([33; 64]),
            origin: PositiveCertificationDrawOriginV2 { block: NativeHash64([34; 64]), daa: 107, blue_score: 107 },
            deadline_daa: 112,
            id: d(0),
        },
        positions,
    };
    draw.meta.id = draw.expected_id(&row.fixed.header);
    row.draw = Some(draw);
    row
}
fn vote(mut row: PositiveCertificationRowV2, verdict: PositiveCourtVerdictV2) -> PositiveCertificationRowV2 {
    use libcrux_ml_dsa::ml_dsa_87::portable;
    let key = portable::generate_key_pair([77; 32]);
    let pk: &[u8; PUBKEY_BYTES] = key.verification_key.as_ref();
    let point = NativeOutpoint { transaction: NativeHash64([90; 64]), index: 1 };
    let id = Digest(palw_kernel_bond_id_v1(&PalwBondKeyV2(TransactionOutpoint::new(
        TransactionId::from_bytes(point.transaction.0),
        point.index,
    ))));
    let positions = vec![id; usize::from(row.fixed.header.policy.m)];
    row = drawn(row, positions);
    row.fixed.finality = Some(PositiveCertificationFinalV2 {
        origin: origin(110),
        witness: WitnessId(d(40)),
        material: AcceptedMaterial { origin: origin(110), digest: d(41) },
    });
    let h = &row.fixed.header;
    let statement = PositiveCourtAttestationV2 {
        version: 2,
        revision: 1,
        scope: h.scope,
        claim: h.claim,
        result: h.result,
        case: h.case,
        subject: h.subject,
        policy: h.policy_id,
        draw_anchor: row.draw.as_ref().unwrap().meta.id,
        witness: WitnessId(d(40)),
        verifier_bond: id,
        verifier: principal(pk),
        verdict,
        checked_material: d(41),
    };
    let signature = portable::sign(&key.signing_key, &statement.id().0, POSITIVE_ATTESTATION_SIGN_CONTEXT_V2, [78; 32]).unwrap();
    let liability_amount = if verdict == PositiveCourtVerdictV2::PositiveChecked {
        kaspa_consensus_core::palw_g14_lane_closure_v1::palw_kernel_signer_lock_amount_v1(h.worker_gain, h.policy.k as u8)
    } else {
        0
    };
    row.votes.push(PositiveCertificationVoteV2 {
        bond: point,
        key: *pk,
        statement,
        signature: *signature.as_ref(),
        origin: origin(111),
        liability_amount,
    });
    row
}
#[test]
fn bounded_prefix_p_codec_preserves_old_namespaces_and_refuses_hostile_lengths() {
    let row = row(75, 41);
    row.validate_shape().unwrap();
    let key = positive_certification_key_v2(row.fixed.header.subject);
    let raw = borsh::to_vec(&row).unwrap();
    assert_eq!(PositiveCertificationRowV2::decode(&key, &raw).unwrap(), row);
    for prefix in [b'S', b'K', b'V', b'H'] {
        let mut old = key.clone();
        old[0] = prefix;
        assert!(PositiveCertificationRowV2::decode(&old, &raw).is_err());
    }
    let mut hostile = raw.clone();
    let n = hostile.len();
    hostile[n - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(preflight_native_value_v1::<PositiveCertificationRowV2>(hostile.as_slice(), hostile.len() as u64).is_err());
    let mut oversized = row;
    oversized.fixed.header.policy.m = 76;
    oversized.fixed.header.policy.k = 42;
    oversized.fixed.header.policy_id = positive_certification_policy_id_v2(&oversized.fixed.header.policy);
    assert!(oversized.validate_shape().is_err());
}
#[test]
fn one_bond_votes_once_but_each_with_replacement_position_counts() {
    let row = vote(row(3, 2), PositiveCourtVerdictV2::PositiveChecked);
    row.validate_shape().unwrap();
    row.authenticate_votes(&result()).unwrap();
    assert_eq!(row.votes.len(), 1);
    assert_eq!(row.positive_positions(), 3);
    let mut duplicate = row.clone();
    duplicate.votes.push(duplicate.votes[0].clone());
    assert!(duplicate.validate_shape().is_err());
    let mut changed = row.clone();
    changed.votes[0].statement.verdict = PositiveCourtVerdictV2::Unavailable;
    changed.votes[0].liability_amount = 0;
    assert!(changed.authenticate_votes(&result()).is_err());
    let mut changed_material = row;
    changed_material.votes[0].statement.checked_material = d(99);
    assert!(changed_material.authenticate_votes(&result()).is_err());
}
#[test]
fn actual_policy_failure_clocks_do_not_turn_pending_positive_into_failure() {
    use PositiveCertificationOutcomeV2 as O;
    let base = row(3, 2);
    assert_eq!(positive_certification_failure_v2(&base, 107, true), None);
    assert_eq!(positive_certification_failure_v2(&base, 108, true), Some(O::Expired));
    let empty = drawn(base.clone(), vec![]);
    assert_eq!(positive_certification_failure_v2(&empty, 107, true), Some(O::NoPopulation));
    let withheld = vote(base.clone(), PositiveCourtVerdictV2::Unavailable);
    assert_eq!(positive_certification_failure_v2(&withheld, 112, true), None);
    assert_eq!(positive_certification_failure_v2(&withheld, 113, true), Some(O::NotCertified));
    let mut veto = row(3, 2);
    veto.fixed.header.policy.veto = 1;
    veto.fixed.header.policy_id = positive_certification_policy_id_v2(&veto.fixed.header.policy);
    let veto = vote(veto, PositiveCourtVerdictV2::NotReproducible);
    assert_eq!(positive_certification_failure_v2(&veto, 111, true), Some(O::Vetoed));
    let positive = vote(base.clone(), PositiveCourtVerdictV2::PositiveChecked);
    assert_eq!(
        positive_certification_failure_v2(&positive, u64::MAX, true),
        None,
        "kpositive still needs actual Final/prosecution/current native no-fraud proof"
    );
    assert_eq!(positive_certification_failure_v2(&positive, 111, false), Some(O::Withdrawn));
    let mut withdrawn = base;
    withdrawn.fixed.outcome = Some((O::Withdrawn, 111));
    assert_eq!(positive_certification_failure_v2(&withdrawn, 112, false), None);
}
#[test]
fn delayed_final_and_draw_origin_are_bound_without_public_check_or_old_vote() {
    let mut row = vote(row(3, 2), PositiveCourtVerdictV2::PositiveChecked);
    row.fixed.outcome = Some((PositiveCertificationOutcomeV2::Certified, 128));
    assert!(row.validate_shape().is_err(), "actualFinal110+prosecution19=129");
    row.fixed.outcome = Some((PositiveCertificationOutcomeV2::Certified, 129));
    row.validate_shape().unwrap();
    let old = row.draw.as_ref().unwrap().meta.id;
    row.draw.as_mut().unwrap().meta.anchor_block = NativeHash64([88; 64]);
    assert_ne!(old, row.draw.as_ref().unwrap().expected_id(&row.fixed.header));
    assert!(row.validate_shape().is_err());
    let policy = policy(3, 2);
    assert!(!palw_certification_pool_admits_v1(&policy, 100, &d(1).0, true, 50, 100, &d(1).0));
    assert!(!palw_certification_pool_admits_v1(&policy, 100, &d(1).0, true, 99, 100, &d(2).0));
    assert!(palw_certification_pool_admits_v1(&policy, 100, &d(1).0, true, 97, 100, &d(2).0));
}
