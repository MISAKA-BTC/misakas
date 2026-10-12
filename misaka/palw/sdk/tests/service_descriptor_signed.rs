//! ADR-0101 Decision 3 — a service descriptor signed with a real ML-DSA-87 key verifies under the
//! verifier every binary ships, and a descriptor with one field changed after signing does not.

use kaspa_consensus_core::palw_model_benefits_v1::grant;
use kaspa_consensus_core::palw_service_descriptor_v1::{
    PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT, PalwLineServiceFactsV1, PalwServiceDescriptorError, PalwServiceDescriptorV1,
    PalwServiceProviderKindV1, palw_service_descriptor_check_v1, palw_service_descriptor_id_v1,
};
use kaspa_hashes::Hash64;

fn verify(pk: &[u8], msg: &[u8], sig: &[u8], ctx: &[u8]) -> bool {
    kaspa_txscript::verify_mldsa87_with_context(pk, msg, sig, ctx).unwrap_or(false)
}

#[test]
fn a_real_signature_verifies_and_a_changed_field_does_not() {
    let domain = Hash64::from_u64_word(0x11);
    let provider = kaspa_pq_validator_core::ValidatorKey::from_seed([7u8; 32]);
    let owner = kaspa_pq_validator_core::ValidatorKey::from_seed([9u8; 32]);
    let facts = PalwLineServiceFactsV1 {
        line_id: Hash64::from_u64_word(7),
        declared_grants: grant::PRIORITY_INFERENCE | grant::INFERENCE_QUOTA | grant::SUPPORT,
        roots: vec![Hash64::from_u64_word(100)],
        origin_pubkeys: vec![owner.public_key().to_vec()],
        now_daa: 50,
    };
    let sign = |key: &kaspa_pq_validator_core::ValidatorKey, grants: u32| {
        let mut d = PalwServiceDescriptorV1 {
            version: 1,
            line_id: facts.line_id,
            grants,
            roots: vec![Hash64::from_u64_word(100)],
            endpoints: vec!["https://provider.example/v1".into()],
            valid_from_daa: 10,
            expires_daa: 100,
            provider_pubkey: key.public_key().to_vec(),
            signature: Vec::new(),
        };
        let id = palw_service_descriptor_id_v1(domain, &d);
        d.signature = key.sign_with_context(id.as_byte_slice(), PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT).to_vec();
        d
    };
    let open = sign(&provider, grant::PRIORITY_INFERENCE | grant::INFERENCE_QUOTA);
    assert_eq!(palw_service_descriptor_check_v1(domain, &open, &facts, verify), Ok(PalwServiceProviderKindV1::Open));
    let origin = sign(&owner, grant::PRIORITY_INFERENCE | grant::SUPPORT);
    assert_eq!(palw_service_descriptor_check_v1(domain, &origin, &facts, verify), Ok(PalwServiceProviderKindV1::Origin));
    // A stranger's key cannot claim the origin's grant, signed or not.
    let stranger_support = sign(&provider, grant::SUPPORT);
    assert!(matches!(
        palw_service_descriptor_check_v1(domain, &stranger_support, &facts, verify),
        Err(PalwServiceDescriptorError::OriginGrantByAStranger(_))
    ));
    // One field changed after signing: the id moves and the signature no longer covers it.
    let widened = PalwServiceDescriptorV1 { expires_daa: 1_000, ..open.clone() };
    assert_eq!(palw_service_descriptor_check_v1(domain, &widened, &facts, verify), Err(PalwServiceDescriptorError::Signature));
    // Another network: the same bytes are not a statement there.
    assert_eq!(
        palw_service_descriptor_check_v1(Hash64::from_u64_word(0x12), &open, &facts, verify),
        Err(PalwServiceDescriptorError::Signature)
    );
}
