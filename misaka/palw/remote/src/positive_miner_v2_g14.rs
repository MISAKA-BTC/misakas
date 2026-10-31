//! Outbound existing G14 material bytes for native op4. This module allocates
//! no wire discriminants and grants no duty, receipt, payment or credit.
use super::*;

/// Encode ONE already prepared source unit. The native owner resolves the
/// accepted RequestId and input form before decoding these raw existing G14
/// bytes. Native wrapper/carrier fees and admission are accounted separately.
/// `max_bytes` bounds this body before serialization, hashing or allocation.
pub fn encode_g14_unit_v2(
    result: &MinerResultCommit,
    request: &SourceRequest,
    source_units: u64,
    prompt_form: PalwPromptIdsFormV1,
    index: u64,
    unit: &StoredUnitV2,
    max_bytes: u64,
) -> Result<Vec<u8>, MinerErrorV2> {
    request.validate(source_units)?;
    if request.case != result.case_id() || request.contract != result.service_contract || index < request.first || index >= request.end
    {
        return Err(MinerErrorV2::Binding);
    }
    if max_bytes == 0 || !unit.bounded(max_bytes) || unit.sizes()?.1 > max_bytes {
        return Err(MinerErrorV2::Budget);
    }
    let form_matches = matches!(
        (unit, prompt_form),
        (StoredUnitV2::Artifact(_), _)
            | (StoredUnitV2::Input(_), PalwPromptIdsFormV1::MerkleV1)
            | (StoredUnitV2::FlatInput(_), PalwPromptIdsFormV1::Flat)
    );
    if !form_matches || !unit.authenticates(result, request, index, source_units) {
        return Err(MinerErrorV2::Binding);
    }
    // Preserve literal G14 codec: no StoredUnitV2 enum tag, second tree or
    // privately selected unit identity is included in this body.
    match unit {
        StoredUnitV2::Artifact(value) => borsh::to_vec(value),
        StoredUnitV2::Input(value) => borsh::to_vec(value),
        StoredUnitV2::FlatInput(value) => borsh::to_vec(value),
    }
    .map_err(|_| MinerErrorV2::Binding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::miner::positive_v2::tests::Fixture;

    #[test]
    fn existing_g14_opening_codec_is_preserved_and_independently_authenticated() {
        let fixture = Fixture::new();
        for kind in [SourceKind::ArtifactInventory, SourceKind::Input] {
            let request = fixture.request(kind, 0, 3);
            let unit = fixture.provider.unit(kind, request.source, 2).unwrap();
            let bytes = encode_g14_unit_v2(&fixture.result, &request, 3, PalwPromptIdsFormV1::MerkleV1, 2, unit, 4096).unwrap();
            match kind {
                SourceKind::ArtifactInventory => {
                    let opening: PalwArtifactOpeningV1 = borsh::from_slice(&bytes).unwrap();
                    assert_eq!(opening.leaf_index, 2);
                    verify_artifact_opening_v1(&opening, Hash64::from_bytes(fixture.result.artifact.0)).unwrap();
                    assert_eq!(&StoredUnitV2::Artifact(opening), unit);
                }
                SourceKind::Input => {
                    let opening: PalwPromptIdsOpeningV1 = borsh::from_slice(&bytes).unwrap();
                    assert_eq!(opening.tile_index, 2);
                    verify_prompt_ids_opening_v1(&Hash64::from_bytes(fixture.result.input.0), fixture.result.prefill, &opening)
                        .unwrap();
                    assert_eq!(&StoredUnitV2::Input(opening), unit);
                }
            }
        }
    }

    #[test]
    fn native_binding_form_range_and_wire_cap_are_not_supplied_by_unit() {
        let fixture = Fixture::new();
        let mut request = fixture.request(SourceKind::Input, 0, 2);
        let unit = fixture.provider.unit(request.kind, request.source, 1).unwrap();
        let encode = |request: &SourceRequest, units, form, index, max| {
            encode_g14_unit_v2(&fixture.result, request, units, form, index, unit, max)
        };
        assert_eq!(encode(&request, 3, PalwPromptIdsFormV1::Flat, 1, 4096), Err(MinerErrorV2::Binding));
        assert_eq!(encode(&request, 3, PalwPromptIdsFormV1::MerkleV1, 0, 4096), Err(MinerErrorV2::Binding));
        assert!(encode(&request, 1, PalwPromptIdsFormV1::MerkleV1, 1, 4096).is_err());
        assert_eq!(encode(&request, 3, PalwPromptIdsFormV1::MerkleV1, 2, 4096), Err(MinerErrorV2::Binding));
        let size = unit.sizes().unwrap().1;
        assert_eq!(encode(&request, 3, PalwPromptIdsFormV1::MerkleV1, 1, size - 1), Err(MinerErrorV2::Budget));
        assert_eq!(encode(&request, 3, PalwPromptIdsFormV1::MerkleV1, 1, size).unwrap().len() as u64, size);
        request.contract.0[0] ^= 1;
        assert_eq!(encode(&request, 3, PalwPromptIdsFormV1::MerkleV1, 1, 4096), Err(MinerErrorV2::Binding));
    }

    #[test]
    fn flat_input_is_existing_whole_vec_and_cannot_be_merkle_form() {
        let mut fixture = Fixture::new();
        let ids: Vec<u32> = (0..fixture.result.prefill).collect();
        fixture.result.input = Digest(
            kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, &ids)
                .unwrap()
                .as_bytes(),
        );
        let request = fixture.request(SourceKind::Input, 0, 1);
        let unit = StoredUnitV2::FlatInput(ids.clone());
        let bytes = encode_g14_unit_v2(&fixture.result, &request, 1, PalwPromptIdsFormV1::Flat, 0, &unit, 4096).unwrap();
        assert_eq!(borsh::from_slice::<Vec<u32>>(&bytes).unwrap(), ids);
        assert_eq!(
            encode_g14_unit_v2(&fixture.result, &request, 1, PalwPromptIdsFormV1::MerkleV1, 0, &unit, 4096),
            Err(MinerErrorV2::Binding)
        );
    }
}
