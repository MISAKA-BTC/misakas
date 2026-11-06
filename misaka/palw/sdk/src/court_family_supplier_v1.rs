//! RFC-0042 explicit service consent and literal TFO2 transport builders. Quotes
//! must come from native admission; local signatures/encodings do not reserve funds.
//! Sign UTXO inputs after wrapping: their sighash commits the complete TFO2 carrier.
pub use kaspa_consensus_core::palw_court_family_material_v1::{
    FamilyDraftQuoteV2, FamilyMaterialActionV1, FamilyMaterialTargetV1, SignedFamilyMaterialV1, sign_family_material_v1,
};
pub use kaspa_consensus_core::palw_court_family_supplier_v1::{
    FamilySupplierRoleV1, FamilySupplierTermsV1, SignedFamilySupplierV1, sign_family_supplier_v1,
};
pub use kaspa_consensus_core::palw_state_v2::{
    PalwFamilyOriginalNativeQuoteV2, palw_family_material_begin_quote_v2, palw_family_original_native_quote_v2,
    palw_family_supplier_terms_quote_v2,
};
/// Active source-derived generated semantic purpose; V2 exports remain literal.
pub use kaspa_consensus_core::palw_state_v2::{
    PalwFamilyOriginalNativeQuoteV3, family_supplier_profiles_v3, palw_family_original_native_quote_v3,
    palw_family_supplier_terms_quote_v3,
};
use kaspa_consensus_core::palw_tir_funded_original_envelope_v2::{MAX_SUPPLIERS, PalwTirFundedOriginalEnvelopeV2};
use kaspa_consensus_core::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
};

/// Keep the exact old original/funding bytes; only the independent service
/// consents occupy the new wrapper. Native admission still verifies all roles,
/// actual inventories, authorized parties, prices and the real funding outputs.
pub fn wrap_funded_original_v2(literal_tfo1: Vec<u8>, suppliers: Vec<SignedFamilySupplierV1>) -> Result<Vec<u8>, String> {
    wrap_funded_original_with_table_v2(literal_tfo1, suppliers, None)
}
/// Exact canonical table witness, authenticated before native inventory derivation.
pub fn wrap_funded_original_with_table_v2(
    literal_tfo1: Vec<u8>,
    mut suppliers: Vec<SignedFamilySupplierV1>,
    semantic_table: Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    suppliers.sort_by_key(|s| s.terms.role);
    PalwTirFundedOriginalEnvelopeV2 { version: 2, funded_original: literal_tfo1, suppliers, semantic_table }.encode_v2()
}
/// Every supplier and sponsor signs their own exact native quote. A legacy
/// source pledge cannot be reused to sign these independent service obligations.
pub fn sign_funded_original_v2(
    literal_tfo1: Vec<u8>,
    terms: Vec<FamilySupplierTermsV1>,
    sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    sign_funded_original_with_table_v2(literal_tfo1, terms, None, sign)
}
pub fn sign_funded_original_with_table_v2(
    literal_tfo1: Vec<u8>,
    mut terms: Vec<FamilySupplierTermsV1>,
    semantic_table: Option<Vec<u8>>,
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let base = kaspa_consensus_core::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1::decode_v1(&literal_tfo1)?;
    if terms.is_empty() || terms.len() > MAX_SUPPLIERS {
        return Err("family consent native role count".into());
    }
    terms.sort_by_key(|s| s.role);
    if terms.windows(2).any(|w| w[0].role == w[1].role) {
        return Err("family consent repeats a role".into());
    }
    for t in &terms {
        t.validate_shape_v1().map_err(str::to_owned)?;
        if t.claim != base.funding.terms.claim || t.source != base.funding.terms.source_id {
            return Err("family consent quote belongs to another literal funded original".into());
        }
    }
    let suppliers = terms
        .into_iter()
        .map(|t| {
            sign_family_supplier_v1(t, |who, message, context| sign(who, message, context))
                .ok_or_else(|| "family supplier or sponsor signing key unavailable".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    wrap_funded_original_with_table_v2(literal_tfo1, suppliers, semantic_table)
}
/// Actual uploader-signed controls for one complete literal body. The supplied
/// native bound is a preflight convenience; consensus rederives it independently.
/// TokenizerRaw bodies use `borsh::to_vec(&raw)`; semantic tables use PALWTTB1.
pub fn family_material_upload_v1(
    network: Hash64,
    contract: Hash64,
    target: FamilyMaterialTargetV1,
    submitter: PalwBondKeyV2,
    native_bound: u64,
    body: &[u8],
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<Vec<SignedFamilyMaterialV1>, String> {
    use kaspa_consensus_core::palw_court_family_material_v1::{FAMILY_MATERIAL_PART_BYTES_V1, family_material_draft_id_v1};
    if body.is_empty() || body.len() as u64 > native_bound {
        return Err("family material exceeds quoted native target bound".into());
    }
    let hash = kaspa_consensus_core::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(body);
    let draft = family_material_draft_id_v1(contract, target, submitter, body.len() as u64, hash)?;
    let count = body.len().div_ceil(FAMILY_MATERIAL_PART_BYTES_V1).checked_add(2).ok_or("family material packet count overflow")?;
    let mut controls = Vec::new();
    controls.try_reserve_exact(count).map_err(|e| e.to_string())?;
    let mut push = |action| -> Result<(), String> {
        let control = SignedFamilyMaterialV1 { version: 1, contract, target, submitter, action, signature: vec![] };
        let signed = sign_family_material_v1(control, network, |who, message, context| sign(who, message, context))
            .ok_or("family uploader signing key unavailable")?;
        controls.push(signed);
        Ok(())
    };
    push(FamilyMaterialActionV1::Begin { total_bytes: body.len() as u64, body_hash: hash })?;
    for (n, part) in body.chunks(FAMILY_MATERIAL_PART_BYTES_V1).enumerate() {
        let offset = (n as u64).checked_mul(FAMILY_MATERIAL_PART_BYTES_V1 as u64).ok_or("family material offset overflow")?;
        push(FamilyMaterialActionV1::Part { draft, offset, bytes: part.to_vec() })?;
    }
    push(FamilyMaterialActionV1::Finish { draft })?;
    Ok(controls)
}
/// Additional optional draft: sign the exact native quote against the current
/// prefix state and acceptance DAA. If the quote changes, regenerate Begin,
/// Part and Finish: the draft identity binds the exact signed quote.
pub fn family_material_upload_v2(
    network: Hash64,
    contract: Hash64,
    target: FamilyMaterialTargetV1,
    submitter: PalwBondKeyV2,
    native_bound: u64,
    body: &[u8],
    quote: FamilyDraftQuoteV2,
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<Vec<SignedFamilyMaterialV1>, String> {
    use kaspa_consensus_core::palw_court_family_material_v1::{FAMILY_MATERIAL_PART_BYTES_V1, family_material_draft_id_v2};
    if body.is_empty() || body.len() as u64 > native_bound || quote.version != 2 || quote.fee_sompi == 0 || quote.collateral_sompi == 0
    {
        return Err("independent draft native bound/quote absent".into());
    }
    let hash = kaspa_consensus_core::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(body);
    let draft = family_material_draft_id_v2(contract, target, submitter, body.len() as u64, hash, &quote)?;
    let count = body.len().div_ceil(FAMILY_MATERIAL_PART_BYTES_V1).checked_add(2).ok_or("independent draft control count overflow")?;
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|e| e.to_string())?;
    let mut push = |version, action| -> Result<(), String> {
        out.push(
            sign_family_material_v1(
                SignedFamilyMaterialV1 { version, contract, target, submitter, action, signature: vec![] },
                network,
                |who, message, context| sign(who, message, context),
            )
            .ok_or("independent draft signing key unavailable")?,
        );
        Ok(())
    };
    push(2, FamilyMaterialActionV1::BeginReserved { total_bytes: body.len() as u64, body_hash: hash, quote })?;
    for (n, bytes) in body.chunks(FAMILY_MATERIAL_PART_BYTES_V1).enumerate() {
        push(
            1,
            FamilyMaterialActionV1::Part {
                draft,
                offset: (n as u64).checked_mul(FAMILY_MATERIAL_PART_BYTES_V1 as u64).ok_or("independent draft offset overflow")?,
                bytes: bytes.to_vec(),
            },
        )?;
    }
    push(1, FamilyMaterialActionV1::Finish { draft })?;
    Ok(out)
}
pub fn family_material_object_v1(control: SignedFamilyMaterialV1) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::CourtFamilyMaterialV1 { control: Box::new(control) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sdk_family_material_upload_signs_every_exact_chunk_and_finish() {
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([101; 32]);
        let network = Hash64::from_u64_word(1);
        let contract = Hash64::from_u64_word(2);
        let uploader = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(3), 0));
        let part = kaspa_consensus_core::palw_court_family_material_v1::FAMILY_MATERIAL_PART_BYTES_V1;
        let body = vec![0x5a; part + 7];
        let controls = family_material_upload_v1(
            network,
            contract,
            FamilyMaterialTargetV1::SemanticTable,
            uploader,
            body.len() as u64,
            &body,
            |who, message, context| {
                assert_eq!(*who, uploader);
                Some(libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [102; 32]).unwrap().as_ref().to_vec())
            },
        )
        .unwrap();
        assert_eq!(controls.len(), 4);
        let mut restored = Vec::new();
        let mut draft = None;
        for control in &controls {
            control.verify_v1(network, key.verification_key.as_ref()).unwrap();
            match &control.action {
                FamilyMaterialActionV1::Begin { total_bytes, .. } => assert_eq!(*total_bytes, body.len() as u64),
                FamilyMaterialActionV1::Part { draft: id, offset, bytes } => {
                    assert_eq!(*offset, restored.len() as u64);
                    assert!(draft.is_none_or(|old| old == *id));
                    draft = Some(*id);
                    restored.extend_from_slice(bytes);
                }
                FamilyMaterialActionV1::Finish { draft: id } => assert_eq!(draft, Some(*id)),
                FamilyMaterialActionV1::BeginReserved { .. } => panic!("legacy builder must preserve literal V1 Begin"),
            }
        }
        assert_eq!(restored, body);
        assert!(
            family_material_upload_v1(network, contract, FamilyMaterialTargetV1::SemanticTable, uploader, 1, &body, |_, _, _| None)
                .is_err()
        );
    }
    #[test]
    fn sdk_family_tfo2_refuses_invalid_base_before_signing() {
        let mut called = false;
        let result = sign_funded_original_v2(b"TFO1".to_vec(), vec![], |_, _, _| {
            called = true;
            None
        });
        assert!(result.is_err());
        assert!(!called);
    }
}

/// Explicit new wire framing with lossless aliases for identical literal
/// supplier/sponsor signatures. Logical signatures and both verifications stay
/// independent. Existing V2 wrappers keep their original bytes.
pub fn wrap_funded_original_with_table_v3(
    literal_tfo1: Vec<u8>,
    mut suppliers: Vec<SignedFamilySupplierV1>,
    semantic_table: Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    suppliers.sort_by_key(|s| s.terms.role);
    PalwTirFundedOriginalEnvelopeV2 { version: 3, funded_original: literal_tfo1, suppliers, semantic_table }.encode_v2()
}
