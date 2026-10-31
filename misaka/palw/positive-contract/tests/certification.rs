use misaka_palw_positive_contract::{ContractError, certification::*, identity::*, messages::*, native::certification_subject};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn result() -> MinerResultCommit {
    MinerResultCommit {
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
        service_contract: d(17),
    }
}
fn expected(result: &MinerResultCommit) -> PositiveAttestationExpectationsV2 {
    PositiveAttestationExpectationsV2 {
        scope: result.scope,
        policy: d(31),
        draw_anchor: d(32),
        witness: WitnessId(d(33)),
        verifier_bond: d(34),
        verifier: PrincipalId(d(22)),
        checked_material: d(35),
    }
}
fn attestation(result: &MinerResultCommit) -> PositiveCourtAttestationV2 {
    PositiveCourtAttestationV2 {
        version: 2,
        revision: 1,
        scope: result.scope,
        claim: result.claim_id(),
        result: result.id(),
        case: result.case_id(),
        subject: certification_subject(result),
        policy: d(31),
        draw_anchor: d(32),
        witness: WitnessId(d(33)),
        verifier_bond: d(34),
        verifier: PrincipalId(d(22)),
        verdict: PositiveCourtVerdictV2::PositiveChecked,
        checked_material: d(35),
    }
}

#[test]
fn fixed_body_matches_independent_field_order_and_id_hash() {
    let r = result();
    let a = attestation(&r);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    for digest in [
        a.scope.network,
        a.scope.ruleset,
        a.scope.branch,
        a.claim.0,
        a.result.0,
        a.case.0,
        a.subject,
        a.policy,
        a.draw_anchor,
        a.witness.0,
        a.verifier_bond,
        a.verifier.0,
    ] {
        bytes.extend_from_slice(&digest.0);
    }
    bytes.push(0); // PositiveChecked, appended only verdict schema.
    bytes.extend_from_slice(&a.checked_material.0);
    assert_eq!(bytes.len(), 837);
    assert_eq!(a.encode().unwrap().as_slice(), bytes);
    assert_eq!(PositiveCourtAttestationV2::decode(&bytes), Ok(a));
    let hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka/positive/attestation-id/v2/r1").hash(&bytes);
    assert_eq!(a.id().0.as_slice(), hash.as_bytes());
    assert_eq!(POSITIVE_ATTESTATION_BYTES_V2 + SIGNATURE_BYTES, 5464);
}

#[test]
fn every_fixed_field_is_in_the_statement_id() {
    let a = attestation(&result());
    let bytes = a.encode().unwrap();
    for i in 4..bytes.len() {
        let mut changed = bytes;
        // The only enum byte is constrained to its known alternatives.
        changed[i] ^= 1;
        if let Ok(other) = PositiveCourtAttestationV2::decode(&changed) {
            assert_ne!(a.id(), other.id(), "field byte {i}");
        }
    }
}

#[test]
fn native_expectations_pin_all_identity_draw_and_material_fields() {
    let r = result();
    let a = attestation(&r);
    let expected = expected(&r);
    assert_eq!(a.validate_binding(&r, &expected), Ok(()));
    let mut changes = Vec::new();
    macro_rules! change {
        ($field:ident, $value:expr) => {{
            let mut b = a;
            b.$field = $value;
            changes.push(b);
        }};
    }
    change!(scope, ChainScope { network: d(99), ..a.scope });
    change!(scope, ChainScope { ruleset: d(99), ..a.scope });
    change!(scope, ChainScope { branch: d(99), ..a.scope });
    change!(claim, ClaimId(d(99)));
    change!(result, ResultId(d(99)));
    change!(case, CaseId(d(99)));
    change!(subject, d(99));
    change!(policy, d(99));
    change!(draw_anchor, d(99));
    change!(witness, WitnessId(d(99)));
    change!(verifier_bond, d(99));
    change!(verifier, PrincipalId(d(99)));
    change!(checked_material, d(99));
    for changed in changes {
        assert_eq!(changed.validate_binding(&r, &expected), Err(ContractError::Scope));
    }
    let mut foreign_context = expected;
    foreign_context.scope.branch = d(99);
    assert_eq!(a.validate_binding(&r, &foreign_context), Err(ContractError::Scope));
    // Same raw beacon could produce two native draw records with another roster,
    // policy or accepted origin. The opaque native draw ID must still match.
    let mut another_draw_with_same_beacon = expected;
    another_draw_with_same_beacon.draw_anchor = d(98);
    assert_eq!(a.validate_binding(&r, &another_draw_with_same_beacon), Err(ContractError::Scope));
}

#[test]
fn full_result_output_and_conditions_cannot_borrow_another_subject_vote() {
    let r = result();
    let a = attestation(&r);
    let e = expected(&r);
    let mut other = r.clone();
    other.output = d(99);
    assert_eq!(r.case_id(), other.case_id(), "output change keeps aggregate case identity");
    assert_ne!(certification_subject(&r), certification_subject(&other));
    assert_eq!(a.validate_binding(&other, &e), Err(ContractError::Scope));
    other = r.clone();
    other.input = d(99);
    assert_eq!(a.validate_binding(&other, &e), Err(ContractError::Scope));
    other = r.clone();
    other.scope.branch = d(99);
    assert_eq!(a.validate_binding(&other, &e), Err(ContractError::Scope));
}

#[test]
fn actor_and_new_role_signature_are_required_for_each_literal_verdict() {
    use libcrux_ml_dsa::ml_dsa_87::*;
    let key = generate_key_pair([81; 32]);
    let other = generate_key_pair([82; 32]);
    let public_key: &[u8; PUBKEY_BYTES] = key.verification_key.as_ref();
    let r = result();
    let mut a = attestation(&r);
    let mut e = expected(&r);
    e.verifier = principal(public_key);
    a.verifier = e.verifier;
    for verdict in
        [PositiveCourtVerdictV2::PositiveChecked, PositiveCourtVerdictV2::Unavailable, PositiveCourtVerdictV2::NotReproducible]
    {
        a.verdict = verdict;
        let signature = *sign(&key.signing_key, &a.id().0, POSITIVE_ATTESTATION_SIGN_CONTEXT_V2, [83; 32]).unwrap().as_ref();
        assert_eq!(a.authenticate(&r, &e, public_key, &signature), Ok(()));
        assert_eq!(a.authenticate(&r, &e, other.verification_key.as_ref(), &signature), Err(ContractError::Identity));
        let mut forged = signature;
        forged[0] ^= 1;
        assert_eq!(a.authenticate(&r, &e, public_key, &forged), Err(ContractError::Signature));
        for old_context in [RESULT_SIGN_CONTEXT, WITNESS_SIGN_CONTEXT, b"misaka-palw/model-eligibility/attestation/mldsa87/v1"] {
            let old_signature = *sign(&key.signing_key, &a.id().0, old_context, [84; 32]).unwrap().as_ref();
            assert_eq!(a.authenticate(&r, &e, public_key, &old_signature), Err(ContractError::Signature));
        }
        let mut other_material = a;
        other_material.checked_material = d(99);
        assert_eq!(other_material.authenticate(&r, &e, public_key, &signature), Err(ContractError::Scope));
    }
    // There is no approval, conviction, draw-selection or native-credit return
    // value. Authentication returns only the statement's actor/binding result.
    assert_eq!(misaka_palw_positive_contract::require_native_activation(), Err(ContractError::NativeAdapterUnassigned));
}

#[test]
fn fixed_codec_rejects_versions_unknown_verdicts_cuts_and_trailing_bytes() {
    let a = attestation(&result());
    let bytes = a.encode().unwrap();
    for n in 0..bytes.len() {
        assert_eq!(PositiveCourtAttestationV2::decode(&bytes[..n]), Err(ContractError::NonCanonical));
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert_eq!(PositiveCourtAttestationV2::decode(&trailing), Err(ContractError::NonCanonical));
    let mut invalid = bytes;
    invalid[POSITIVE_ATTESTATION_BYTES_V2 - 65] = 3;
    assert_eq!(PositiveCourtAttestationV2::decode(&invalid), Err(ContractError::NonCanonical));
    for offset in [0, 2] {
        let mut invalid = bytes;
        invalid[offset] ^= 1;
        assert_eq!(PositiveCourtAttestationV2::decode(&invalid), Err(ContractError::Version));
    }
    let mut invalid = a;
    invalid.revision = 2;
    assert_eq!(invalid.encode(), Err(ContractError::Version));
}
