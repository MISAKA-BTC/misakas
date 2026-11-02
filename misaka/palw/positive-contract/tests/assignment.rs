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

fn finite_policy() -> policy::AssignmentPolicyV2 {
    policy::AssignmentPolicyV2 {
        version: 2,
        revision: 1,
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        positive_policy: d(40),
        derivation_policy: policy::canonical_assignment_derivation_id(),
        source_policy: d(11),
        assignment_fee_schedule: d(41),
        case_fee_schedule: d(42),
        reserve_schedule: d(12),
        physical_service_profile: d(43),
        cancellation_policy: d(18),
        live: policy::AssignmentLiveCapsV2 { global: 10, per_poster: 2, per_worker: 2, per_prover: 2, per_class: 4 },
        max_frame_bytes: 32_000,
        max_program_bytes: 65_536,
        max_layout_entries: 1000,
        max_source_catalog_entries: 1000,
        max_role_candidates: 5,
        max_prover_joins: 2,
        max_history_frames: 100,
        max_history_bytes: 100_000,
        max_retained_bytes: 200_000,
        max_replay_bytes: 300_000,
        max_replay_work: 1_000_000,
        max_heap_bytes: 1_000_000,
        accept_window_daa: 100,
        result_window_daa: 100,
        build_window_daa: 100,
        check_window_daa: 100,
        fraud_tail_daa: 100,
        refund_window_daa: 100,
        retention_window_daa: 1000,
    }
}

#[test]
fn assignment_policy_is_finite_and_commits_every_bound() {
    let p = finite_policy();
    p.validate_shape().unwrap();
    let mut t = bundle().terms;
    t.derivation_policy = p.derivation_policy;
    p.check_terms(&t, 0).unwrap();
    assert!(p.check_terms(&t, 100).is_err());
    for mutate in [
        |p: &mut policy::AssignmentPolicyV2| p.live.global = 0,
        |p: &mut policy::AssignmentPolicyV2| p.live.per_poster = 11,
        |p: &mut policy::AssignmentPolicyV2| p.max_role_candidates = 2,
        |p: &mut policy::AssignmentPolicyV2| p.max_prover_joins = u32::MAX,
        |p: &mut policy::AssignmentPolicyV2| p.max_frame_bytes = u64::MAX,
        |p: &mut policy::AssignmentPolicyV2| p.max_history_bytes = p.max_retained_bytes + 1,
        |p: &mut policy::AssignmentPolicyV2| p.max_replay_work = 0,
        |p: &mut policy::AssignmentPolicyV2| p.refund_window_daa = u64::MAX - 1,
        |p: &mut policy::AssignmentPolicyV2| p.retention_window_daa = 599,
        |p: &mut policy::AssignmentPolicyV2| p.physical_service_profile = d(0),
        |p: &mut policy::AssignmentPolicyV2| p.derivation_policy = d(10),
    ] {
        let mut changed = p;
        mutate(&mut changed);
        assert_ne!(changed.id(), p.id());
        assert!(changed.validate_shape().is_err());
    }
}

#[test]
fn source_and_prover_declarations_share_signed_bounds_and_do_not_create_pre_case_services() {
    use consents::*;
    use misaka_palw_positive_contract::messages::AggregateBudget;
    let mut b = bundle();
    let source = AssignmentSourceConsentV2 {
        version: 2,
        revision: 1,
        assignment: b.terms.id(),
        worker: b.terms.worker,
        source_catalog: b.source_catalog,
        source_policy: b.terms.source_policy,
        budget: AggregateBudget {
            rounds: 10,
            unique_source_bytes: 1000,
            transmitted_bytes: 2000,
            additional_work: 1000,
            retained_bytes: 1000,
            retention_until: 500,
            fee: 10,
            witness_attempts: 2,
        },
        service_before_daa: 400,
    };
    let prover = AssignmentProverConsentV2 {
        version: 2,
        revision: 1,
        assignment: b.terms.id(),
        prover: b.terms.prover,
        positive_policy: d(40),
        max_attempts: 2,
        max_build_work: 1000,
        max_submitted_bytes: 1000,
        max_retained_bytes: 1000,
        retention_until_daa: 500,
        build_before_daa: 400,
        max_service_fee: 10,
        cancellation_policy: b.cancellation_policy,
    };
    b.source_consent = source.id();
    b.prover_consent = prover.id();
    source.check(&b).unwrap();
    prover.check(&b, d(40)).unwrap();
    for mutate in [
        |s: &mut AssignmentSourceConsentV2| s.worker = bond(99),
        |s: &mut AssignmentSourceConsentV2| s.source_catalog = d(99),
        |s: &mut AssignmentSourceConsentV2| s.budget.rounds = u64::MAX,
        |s: &mut AssignmentSourceConsentV2| s.budget.unique_source_bytes = 2001,
        |s: &mut AssignmentSourceConsentV2| s.budget.retention_until = 399,
        |s: &mut AssignmentSourceConsentV2| s.budget.fee = 41,
        |s: &mut AssignmentSourceConsentV2| s.service_before_daa = 451,
    ] {
        let mut changed = source;
        mutate(&mut changed);
        let mut changed_bundle = b;
        changed_bundle.source_consent = changed.id();
        assert!(changed.check(&changed_bundle).is_err());
    }
    for mutate in [
        |p: &mut AssignmentProverConsentV2| p.prover = bond(99),
        |p: &mut AssignmentProverConsentV2| p.positive_policy = d(99),
        |p: &mut AssignmentProverConsentV2| p.max_build_work = 0,
        |p: &mut AssignmentProverConsentV2| p.max_attempts = 0,
        |p: &mut AssignmentProverConsentV2| p.retention_until_daa = 399,
        |p: &mut AssignmentProverConsentV2| p.build_before_daa = 451,
        |p: &mut AssignmentProverConsentV2| p.max_service_fee = 41,
        |p: &mut AssignmentProverConsentV2| p.cancellation_policy = d(99),
    ] {
        let mut changed = prover;
        mutate(&mut changed);
        let mut changed_bundle = b;
        changed_bundle.prover_consent = changed.id();
        assert!(changed.check(&changed_bundle, d(40)).is_err());
    }
    let changed_bundle = AssignmentAdmissionBundleV2 { source_consent: d(99), ..b };
    assert_ne!(changed_bundle.id(), b.id());
    assert!(source.check(&changed_bundle).is_err());
}
