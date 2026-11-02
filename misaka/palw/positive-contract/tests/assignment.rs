use misaka_palw_positive_contract::{assignment::*, identity::*, rows::*};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn bond(n: u8) -> NativeOutpoint {
    NativeOutpoint { transaction: NativeHash64([n; 64]), index: n as u32 }
}
fn bundle() -> AssignmentAdmissionBundleV2 {
    let terms = AssignmentTermsV2 {
        version: 2,
        revision: 1,
        network: d(1),
        ruleset: d(2),
        branch_anchor: NativeHash64([3; 64]),
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        registered_class: d(4),
        poster: bond(5),
        worker: bond(6),
        prover: bond(7),
        payer: PrincipalId(d(8)),
        refund_payload: d(9),
        poster_nonce: 0,
        derivation_policy: d(10),
        source_policy: d(11),
        reserve_schedule: d(12),
        worker_reward: 100,
        max_assignment_fee: 20,
        max_assignment_capital: 30,
        max_assignment_locked: 200,
        accept_before_daa: 100,
        result_before_daa: 200,
        reserve_until_daa: 500,
    };
    let assignment = terms.id();
    AssignmentAdmissionBundleV2 {
        terms,
        job_context: d(13),
        quote: d(14),
        source_catalog: d(15),
        funding: AssignmentFundingAuthorizationV2 {
            assignment,
            quote: d(14),
            payer: terms.payer,
            funding_output_index: 0,
            refund_payload: terms.refund_payload,
            max_fee: 20,
            max_capital: 30,
            max_reward: 100,
            max_locked: 200,
            admission_expiry: 100,
        },
        future_case: FutureCaseFundingBoundsV2 {
            assignment,
            payer: terms.payer,
            refund_payload: terms.refund_payload,
            worker: terms.worker,
            max_case_fee: 40,
            max_case_capital: 60,
            max_case_locked: 100,
            case_admission_before_daa: 200,
            service_before_daa: 450,
        },
        source_consent: d(16),
        prover_consent: d(17),
        cancellation_policy: d(18),
    }
}

#[test]
fn terms_identity_precedes_derived_bundle_and_literal_consents() {
    let base = bundle();
    base.validate_shape().unwrap();
    let id = base.terms.id();
    let mut changed = base;
    changed.job_context = d(42);
    assert_eq!(changed.terms.id(), id);
    assert_ne!(changed.id(), base.id());
    // A/C caps and source/prover declarations are co-signed in the bundle,
    // without feeding their derived identities back into the result-free ID.
    for mutate in [
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.max_case_fee -= 1,
        |b: &mut AssignmentAdmissionBundleV2| b.source_consent = d(44),
        |b: &mut AssignmentAdmissionBundleV2| b.prover_consent = d(45),
        |b: &mut AssignmentAdmissionBundleV2| b.funding.funding_output_index = 1,
        |b: &mut AssignmentAdmissionBundleV2| b.cancellation_policy = d(46),
    ] {
        let mut changed = base;
        mutate(&mut changed);
        assert_eq!(changed.terms.id(), id);
        assert_ne!(changed.id(), base.id());
    }
    for mutate in [
        |t: &mut AssignmentTermsV2| t.network = d(41),
        |t: &mut AssignmentTermsV2| t.ruleset = d(42),
        |t: &mut AssignmentTermsV2| t.branch_anchor = NativeHash64([43; 64]),
        |t: &mut AssignmentTermsV2| t.registered_class = d(44),
        |t: &mut AssignmentTermsV2| t.worker = bond(45),
        |t: &mut AssignmentTermsV2| t.poster_nonce = 1,
        |t: &mut AssignmentTermsV2| t.worker_reward += 1,
        |t: &mut AssignmentTermsV2| t.result_before_daa += 1,
    ] {
        let mut terms = base.terms;
        mutate(&mut terms);
        assert_ne!(terms.id(), id);
    }
    let encoded = borsh::to_vec(&base).unwrap();
    assert_eq!(borsh::from_slice::<AssignmentAdmissionBundleV2>(&encoded).unwrap(), base);
}

#[test]
fn a_and_future_c_identity_caps_and_expiries_are_separate() {
    let base = bundle();
    for mutate in [
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.payer = PrincipalId(d(90)),
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.refund_payload = d(91),
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.worker = bond(92),
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.assignment = AssignmentId(d(93)),
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.max_case_capital = 61,
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.service_before_daa = 501,
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.case_admission_before_daa = 201,
        |b: &mut AssignmentAdmissionBundleV2| b.future_case.max_case_locked = u64::MAX,
        |b: &mut AssignmentAdmissionBundleV2| b.funding.max_reward += 1,
        |b: &mut AssignmentAdmissionBundleV2| b.funding.max_fee += 1,
        |b: &mut AssignmentAdmissionBundleV2| b.funding.max_capital += 1,
        |b: &mut AssignmentAdmissionBundleV2| b.funding.max_locked += 1,
        |b: &mut AssignmentAdmissionBundleV2| b.funding.admission_expiry += 1,
        |b: &mut AssignmentAdmissionBundleV2| b.funding.assignment = AssignmentId(d(94)),
        |b: &mut AssignmentAdmissionBundleV2| b.funding.quote = d(95),
    ] {
        let mut changed = base;
        mutate(&mut changed);
        assert!(changed.validate_shape().is_err());
    }
    assert_eq!(base.check_quoted_amounts(20, 30, 150), Ok(()));
    assert!(base.check_quoted_amounts(21, 30, 151).is_err());
    assert!(base.check_quoted_amounts(20, 31, 151).is_err());
    assert!(base.check_quoted_amounts(20, 30, 149).is_err());
    assert!(base.check_quoted_amounts(20, 30, 201).is_err());
    assert!(base.check_quoted_amounts(u64::MAX, 1, 200).is_err());
}

#[test]
fn permanent_nonce_has_no_retirement_reset_and_exhaustion_never_wraps() {
    let first = AssignmentNonceV2::default();
    let second = first.after_accepting(0).unwrap();
    assert_eq!(second.next, 1);
    assert!(second.after_accepting(0).is_err());
    assert!(second.after_accepting(2).is_err());
    let final_nonce = AssignmentNonceV2 { next: u64::MAX, exhausted: false }.after_accepting(u64::MAX).unwrap();
    assert_eq!(final_nonce.next, u64::MAX);
    assert!(final_nonce.exhausted);
    assert!(final_nonce.after_accepting(u64::MAX).is_err());
    assert!(final_nonce.after_accepting(0).is_err());
}

#[test]
fn a_role_signature_cannot_be_relabelled_or_moved_to_another_bundle() {
    use libcrux_ml_dsa::ml_dsa_87::portable;
    let key = portable::generate_key_pair([21; 32]);
    let verification: &[u8; PUBKEY_BYTES] = key.verification_key.as_ref();
    let mut b = bundle();
    b.terms.payer = principal(verification);
    b.funding.payer = b.terms.payer;
    b.future_case.payer = b.terms.payer;
    b.funding.assignment = b.terms.id();
    b.future_case.assignment = b.terms.id();
    for role in [AssignmentRoleV2::Poster, AssignmentRoleV2::Worker, AssignmentRoleV2::Prover] {
        let signature = portable::sign(&key.signing_key, &b.id().0, role.signing_context(), [22; 32]).unwrap();
        b.authenticate_role(role, verification, signature.as_ref()).unwrap();
        for other in [AssignmentRoleV2::Poster, AssignmentRoleV2::Worker, AssignmentRoleV2::Prover] {
            if other != role {
                assert!(b.authenticate_role(other, verification, signature.as_ref()).is_err());
            }
        }
        let mut other = b;
        other.source_consent = d(99);
        assert!(other.authenticate_role(role, verification, signature.as_ref()).is_err());
    }
}
