//! TFO3 carries a literal existing funded original, the exact public prompt
//! text and a fixed PublicOriginal companion. Version 2 also carries the
//! manifest and bounded HF descriptor. This codec checks framing and resource
//! shape. The accepting fold authenticates the actual carrier and its material.

use crate::{
    palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1},
    palw_tir_funded_original_envelope_v1::{
        PALW_TIR_FUNDED_ORIGINAL_MAGIC_V1 as TFO1, PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1, PalwTirFundedOriginalEnvelopeV1,
    },
    palw_tir_funded_original_envelope_v2::{MAGIC as TFO2, PalwTirFundedOriginalEnvelopeV2},
};
use misaka_palw_positive_contract::{
    ContractError,
    public_one_honest::{HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1, ManifestV1, SignedPublicOriginalV1},
};

pub const MAGIC: &[u8; 4] = b"TFO3";
const VERSION_V1: u16 = 1;
const VERSION_WITH_MANIFEST_V2: u16 = 2;
const HEADER_BYTES: usize = MAGIC.len() + 2 + 4 + 4;
const COMPANION_BYTES: usize = SignedPublicOriginalV1::WIRE_BYTES;
const MANIFEST_BYTES: usize = ManifestV1::WIRE_BYTES as usize;
const DESCRIPTOR_LEN_BYTES: usize = std::mem::size_of::<u16>();

#[derive(Clone, Copy, Debug)]
struct ManifestMaterialV2<'a> {
    manifest: &'a [u8; MANIFEST_BYTES],
    hf_descriptor: &'a [u8],
}

/// The existing transport ceiling applies to the entire wrapper.
pub const MAX_BYTES: usize = PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1;

/// Cheap transport recognition without scanning the untrusted body.
pub fn recognizes_v3(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC) || (!bytes.is_empty() && MAGIC.starts_with(bytes))
}

/// The borrowed slices are the exact bytes included in the transaction.
/// Neither decoding this view nor reading its resource quote pays or reserves
/// anything, and it does not verify either signature.
#[derive(Clone, Copy, Debug)]
pub struct PalwTirFundedOriginalEnvelopeV3<'a> {
    funded: &'a [u8],
    prompt: &'a str,
    companion: &'a [u8; COMPANION_BYTES],
    manifest_material: Option<ManifestMaterialV2<'a>>,
    inner_shape: NativeWireShapeV1,
}

impl<'a> PalwTirFundedOriginalEnvelopeV3<'a> {
    /// Recognized framing, including a truncated magic, stays on this path.
    /// A caller must propagate a present error instead of trying an old codec.
    pub fn classify_v3(bytes: &'a [u8]) -> Option<Result<Self, String>> {
        if recognizes_v3(bytes) { Some(Self::decode_v3(bytes)) } else { None }
    }

    pub fn decode_v3(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES || !bytes.starts_with(MAGIC) {
            return Err("TFO3 exceeds its total transport bound or has wrong magic".into());
        }
        let version = read_field::<2>(bytes, MAGIC.len()).map(u16::from_le_bytes)?;
        if !matches!(version, VERSION_V1 | VERSION_WITH_MANIFEST_V2) {
            return Err("unknown TFO3 version".into());
        }
        let inner_bytes = read_field::<4>(bytes, MAGIC.len() + 2).map(u32::from_le_bytes)? as usize;
        let prompt_bytes = read_field::<4>(bytes, MAGIC.len() + 2 + 4).map(u32::from_le_bytes)? as usize;
        let prompt_offset = HEADER_BYTES.checked_add(inner_bytes).ok_or("TFO3 inner length overflow")?;
        let companion_offset = prompt_offset.checked_add(prompt_bytes).ok_or("TFO3 prompt length overflow")?;
        let companion_end = companion_offset.checked_add(COMPANION_BYTES).ok_or("TFO3 companion end overflow")?;
        let companion =
            bytes.get(companion_offset..companion_end).and_then(|part| part.try_into().ok()).ok_or("TFO3 companion width")?;
        let funded = &bytes[HEADER_BYTES..prompt_offset];
        let prompt = &bytes[prompt_offset..companion_offset];
        let suffix = &bytes[companion_end..];
        if version == VERSION_V1 {
            if !suffix.is_empty() {
                return Err("TFO3 version 1 lengths and fixed companion do not reach exact EOF".into());
            }
            return Self::from_parts_v3(funded, prompt, companion);
        }
        let manifest = suffix.get(..MANIFEST_BYTES).and_then(|part| part.try_into().ok()).ok_or("TFO3 manifest width")?;
        let descriptor_len = u16::from_le_bytes(read_field::<DESCRIPTOR_LEN_BYTES>(suffix, MANIFEST_BYTES)?) as usize;
        let descriptor = &suffix[MANIFEST_BYTES + DESCRIPTOR_LEN_BYTES..];
        if descriptor.len() != descriptor_len {
            return Err("TFO3 HF descriptor does not reach exact EOF".into());
        }
        Self::from_parts_with_manifest_v2(funded, prompt, companion, manifest, descriptor)
    }

    pub fn from_parts_v3(funded: &'a [u8], prompt: &'a [u8], companion: &'a [u8; COMPANION_BYTES]) -> Result<Self, String> {
        Self::from_parts(funded, prompt, companion, None)
    }

    pub fn from_parts_with_manifest_v2(
        funded: &'a [u8],
        prompt: &'a [u8],
        companion: &'a [u8; COMPANION_BYTES],
        manifest: &'a [u8; MANIFEST_BYTES],
        hf_descriptor: &'a [u8],
    ) -> Result<Self, String> {
        Self::from_parts(funded, prompt, companion, Some(ManifestMaterialV2 { manifest, hf_descriptor }))
    }

    fn from_parts(
        funded: &'a [u8],
        prompt: &'a [u8],
        companion: &'a [u8; COMPANION_BYTES],
        manifest_material: Option<ManifestMaterialV2<'a>>,
    ) -> Result<Self, String> {
        encoded_len_v3(funded.len(), prompt.len(), manifest_material)?;
        let prompt = std::str::from_utf8(prompt).map_err(|_| "TFO3 prompt is not UTF-8")?;
        let inner_shape = scan_funded_v3(funded)?;
        Ok(Self { funded, prompt, companion, manifest_material, inner_shape })
    }

    pub fn encode_v3(&self) -> Result<Vec<u8>, String> {
        let total = encoded_len_v3(self.funded.len(), self.prompt.len(), self.manifest_material)?;
        let inner_bytes = u32::try_from(self.funded.len()).map_err(|_| "TFO3 inner length exceeds u32")?;
        let prompt_bytes = u32::try_from(self.prompt.len()).map_err(|_| "TFO3 prompt length exceeds u32")?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(total).map_err(|_| "TFO3 output allocation failed")?;
        bytes.extend_from_slice(MAGIC);
        let version = if self.manifest_material.is_some() { VERSION_WITH_MANIFEST_V2 } else { VERSION_V1 };
        bytes.extend_from_slice(&version.to_le_bytes());
        bytes.extend_from_slice(&inner_bytes.to_le_bytes());
        bytes.extend_from_slice(&prompt_bytes.to_le_bytes());
        bytes.extend_from_slice(self.funded);
        bytes.extend_from_slice(self.prompt.as_bytes());
        bytes.extend_from_slice(self.companion);
        if let Some(material) = self.manifest_material {
            let descriptor_len = u16::try_from(material.hf_descriptor.len()).map_err(|_| "TFO3 HF descriptor length exceeds u16")?;
            bytes.extend_from_slice(material.manifest);
            bytes.extend_from_slice(&descriptor_len.to_le_bytes());
            bytes.extend_from_slice(material.hf_descriptor);
        }
        Ok(bytes)
    }

    pub fn funded_original_v3(&self) -> &'a [u8] {
        self.funded
    }

    pub fn prompt_text_v3(&self) -> &'a str {
        self.prompt
    }

    pub fn prompt_bytes_v3(&self) -> &'a [u8] {
        self.prompt.as_bytes()
    }

    pub fn companion_bytes_v3(&self) -> &'a [u8; COMPANION_BYTES] {
        self.companion
    }

    pub fn companion_v3(&self) -> Result<SignedPublicOriginalV1, ContractError> {
        SignedPublicOriginalV1::decode(self.companion)
    }

    pub fn manifest_bytes_v2(&self) -> Option<&'a [u8; MANIFEST_BYTES]> {
        self.manifest_material.map(|material| material.manifest)
    }

    pub fn hf_descriptor_bytes_v2(&self) -> Option<&'a [u8]> {
        self.manifest_material.map(|material| material.hf_descriptor)
    }

    /// No allocation for this borrowed wrapper. These requests cover the
    /// existing bounded TFO2 and nested TFO1 shape scans, for a caller that
    /// will subsequently decode those bodies. The remaining material is
    /// borrowed. Its decoded or copied ownership is separate.
    pub fn inner_native_shape_v3(&self) -> NativeWireShapeV1 {
        self.inner_shape
    }
}

fn read_field<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], String> {
    bytes.get(offset..offset + N).and_then(|field| field.try_into().ok()).ok_or_else(|| "truncated funded original framing".into())
}

fn encoded_len_v3(funded: usize, prompt: usize, manifest_material: Option<ManifestMaterialV2<'_>>) -> Result<usize, String> {
    if prompt == 0 {
        return Err("TFO3 public prompt is empty".into());
    }
    let suffix_bytes = if let Some(material) = manifest_material {
        let descriptor_len = material.hf_descriptor.len();
        if !(1..=HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1.min(u16::MAX as usize)).contains(&descriptor_len) {
            return Err("TFO3 HF descriptor length outside transport bound".into());
        }
        MANIFEST_BYTES + DESCRIPTOR_LEN_BYTES + descriptor_len
    } else {
        0
    };
    HEADER_BYTES
        .checked_add(funded)
        .and_then(|bytes| bytes.checked_add(prompt))
        .and_then(|bytes| bytes.checked_add(COMPANION_BYTES))
        .and_then(|bytes| bytes.checked_add(suffix_bytes))
        .filter(|bytes| *bytes <= MAX_BYTES)
        .ok_or_else(|| "TFO3 exceeds the existing funded original transport bound".into())
}

fn scan_funded_v3(funded: &[u8]) -> Result<NativeWireShapeV1, String> {
    if let Some(body) = funded.strip_prefix(TFO1) {
        return scan_exact::<PalwTirFundedOriginalEnvelopeV1>(body);
    }
    let body = funded.strip_prefix(TFO2).ok_or("TFO3 must contain a literal TFO1 or TFO2")?;
    let family_shape = scan_exact::<PalwTirFundedOriginalEnvelopeV2>(body)?;
    // The TFO2 preflight intentionally treats its embedded byte vector as
    // opaque. Validate that exact slice with the native TFO1 scan as well,
    // before any TFO2 material is decoded or allocated by a caller.
    let inner_len = read_field::<4>(body, 2).map(u32::from_le_bytes)? as usize;
    let inner_end = 6usize.checked_add(inner_len).ok_or("TFO2 literal TFO1 length overflow")?;
    let original_body =
        body.get(6..inner_end).and_then(|bytes| bytes.strip_prefix(TFO1)).ok_or("TFO3 nested TFO2 must carry a literal TFO1")?;
    let base_shape = scan_exact::<PalwTirFundedOriginalEnvelopeV1>(original_body)?;
    family_shape.merge(base_shape).map_err(|why| why.to_string())
}

fn scan_exact<T: NativeWireScanV1>(bytes: &[u8]) -> Result<NativeWireShapeV1, String> {
    let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
    let shape = T::scan_native_wire_v1(&mut reader).map_err(|why| why.to_string())?;
    if reader.remaining() != 0 {
        return Err("TFO3 inner codec has trailing bytes".into());
    }
    Ok(shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Hash64,
        mldsa87_primitives::MLDSA87_SIGNATURE_LEN,
        palw_claim_local_v1::prefix_v1::{PalwPrefixDependencyEnvelopeV1, PalwPrefixExportIntentV1, PalwSignedPrefixExportV1},
        palw_court_family_supplier_v1::{FamilySupplierRoleV1, FamilySupplierTermsV1, SignedFamilySupplierV1},
        palw_freeprompt_v3::PalwFpPrefixStateV1,
        palw_funded_original_coordinator_v1::palw_funded_original_pairing_payload_v1,
        palw_state_v2::PalwBondKeyV2,
        palw_tir_funded_original_envelope_v1::funded_original_transport_fixture_v1,
        tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
    };

    fn family(base: &PalwTirFundedOriginalEnvelopeV1) -> PalwTirFundedOriginalEnvelopeV2 {
        let h = Hash64::from_u64_word;
        PalwTirFundedOriginalEnvelopeV2 {
            version: 2,
            funded_original: base.encode_v1().unwrap(),
            suppliers: vec![SignedFamilySupplierV1 {
                terms: FamilySupplierTermsV1 {
                    version: 1,
                    network: h(1),
                    ruleset: h(2),
                    claim: base.funding.terms.claim,
                    source: base.funding.terms.source_id,
                    relative_graph: h(5),
                    role: FamilySupplierRoleV1::ParameterAndPrompt,
                    inventory: h(6),
                    supplier: PalwBondKeyV2(TransactionOutpoint::new(h(7), 0)),
                    sponsor: PalwBondKeyV2(TransactionOutpoint::new(h(8), 0)),
                    max_material_bytes: 1024,
                    accept_from_daa: 10,
                    accept_until_daa: 20,
                    respond_until_daa: 30,
                    material_until_daa: 40,
                    fee_profile: h(9),
                    capital_profile: h(10),
                    retention_profile: h(11),
                    settlement_policy: h(12),
                    fee_sompi: 17,
                    collateral_sompi: 50,
                    nonce: h(13),
                },
                supplier_signature: vec![1; MLDSA87_SIGNATURE_LEN],
                sponsor_signature: vec![1; MLDSA87_SIGNATURE_LEN],
            }],
            semantic_table: None,
        }
    }

    // Construct malformed transports without letting the production encoder
    // reject them first. No synthetic signatures count as authority.
    const PROMPT: &str = "Studio の prompt\n";

    fn unchecked(inner: &[u8]) -> Vec<u8> {
        unchecked_prompt(inner, PROMPT.as_bytes())
    }

    fn unchecked_prompt(inner: &[u8], prompt: &[u8]) -> Vec<u8> {
        [
            MAGIC.as_slice(),
            &VERSION_V1.to_le_bytes(),
            &(inner.len() as u32).to_le_bytes(),
            &(prompt.len() as u32).to_le_bytes(),
            inner,
            prompt,
            &[0; COMPANION_BYTES],
        ]
        .concat()
    }

    #[test]
    fn tfo3_preserves_literal_tfo1_tfo2_and_companion_bytes() {
        let base = funded_original_transport_fixture_v1();
        let old_claim = base.original_v1().unwrap().original_fp_v1().unwrap().claim_id();
        let family = family(&base);
        for inner in
            [base.encode_v1().unwrap(), family.encode_v2().unwrap(), family.with_signature_aliases_v3().unwrap().encode_v2().unwrap()]
        {
            let companion = [0; COMPANION_BYTES];
            let wire =
                PalwTirFundedOriginalEnvelopeV3::from_parts_v3(&inner, PROMPT.as_bytes(), &companion).unwrap().encode_v3().unwrap();
            assert_eq!(wire, unchecked(&inner), "version 1 wire bytes are frozen");
            let view = PalwTirFundedOriginalEnvelopeV3::classify_v3(&wire).unwrap().unwrap();
            assert!(view.manifest_bytes_v2().is_none());
            assert!(view.hf_descriptor_bytes_v2().is_none());
            assert_eq!(view.funded_original_v3(), inner.as_slice());
            assert_eq!(view.funded_original_v3().as_ptr(), wire[HEADER_BYTES..].as_ptr());
            assert_eq!(view.companion_bytes_v3(), &companion);
            assert_eq!(view.prompt_text_v3(), PROMPT);
            assert_eq!(view.prompt_bytes_v3().as_ptr(), wire[HEADER_BYTES + inner.len()..].as_ptr());
            assert_eq!(view.encode_v3().unwrap(), wire);
            let decoded =
                crate::palw_tir_funded_original_envelope_v2::decode_funded_original_base_v2(view.funded_original_v3()).unwrap();
            assert_eq!(decoded, base);
            assert_eq!(decoded.original_v1().unwrap().original_fp_v1().unwrap().claim_id(), old_claim);
            assert!(view.inner_native_shape_v3().heap > 0);
            let companion = view.companion_v3().unwrap();
            assert!(companion.original.authenticate(&companion.key, &companion.signature).is_err());
            assert!(PalwTirFundedOriginalEnvelopeV3::classify_v3(&inner).is_none());
        }
    }

    #[test]
    fn tfo3_refuses_malformed_without_falling_through() {
        let base = funded_original_transport_fixture_v1().encode_v1().unwrap();
        let wire = unchecked(&base);
        for cut in [1, 2, 3, 4, 6, HEADER_BYTES - 1, HEADER_BYTES, wire.len() - COMPANION_BYTES, wire.len() - 1] {
            assert!(PalwTirFundedOriginalEnvelopeV3::classify_v3(&wire[..cut]).unwrap().is_err());
        }
        let mut wrong_version = wire.clone();
        wrong_version[4] = 3;
        assert!(PalwTirFundedOriginalEnvelopeV3::classify_v3(&wrong_version).unwrap().is_err());
        let mut wrong_length = wire.clone();
        wrong_length[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV3::classify_v3(&wrong_length).unwrap().is_err());
        let mut wrong_prompt_length = wire.clone();
        wrong_prompt_length[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV3::classify_v3(&wrong_prompt_length).unwrap().is_err());
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&unchecked_prompt(&base, &[])).is_err());
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&unchecked_prompt(&base, &[0xF0, 0x28, 0x8C, 0xBC])).is_err());
        let mut trailing = wire.clone();
        trailing.push(0);
        assert!(PalwTirFundedOriginalEnvelopeV3::classify_v3(&trailing).unwrap().is_err());

        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&unchecked(&wire)).is_err());
        let mut nested_family = family(&funded_original_transport_fixture_v1());
        nested_family.funded_original = wire;
        let mut nested = TFO2.to_vec();
        nested.extend(borsh::to_vec(&nested_family).unwrap());
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&unchecked(&nested)).is_err());
        let mut wrong_family = family(&funded_original_transport_fixture_v1()).encode_v2().unwrap();
        wrong_family[4] = 4;
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&unchecked(&wrong_family)).is_err());
        let mut trailing_inner = base;
        trailing_inner.push(0);
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&unchecked(&trailing_inner)).is_err());
    }

    #[test]
    fn tfo3_v2_preserves_borrowed_material_and_checks_suffix_framing() {
        let inner = funded_original_transport_fixture_v1().encode_v1().unwrap();
        let companion = [21; COMPANION_BYTES];
        let manifest = [22; MANIFEST_BYTES];
        // These opaque bytes exercise framing only, before any semantic
        // manifest/descriptor decoding or signature verification.
        let descriptor = [23; HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1];
        let wire = PalwTirFundedOriginalEnvelopeV3::from_parts_with_manifest_v2(
            &inner,
            PROMPT.as_bytes(),
            &companion,
            &manifest,
            &descriptor,
        )
        .unwrap()
        .encode_v3()
        .unwrap();
        let companion_end = HEADER_BYTES + inner.len() + PROMPT.len() + COMPANION_BYTES;
        let descriptor_len_offset = companion_end + MANIFEST_BYTES;
        let descriptor_offset = descriptor_len_offset + DESCRIPTOR_LEN_BYTES;
        let view = PalwTirFundedOriginalEnvelopeV3::decode_v3(&wire).unwrap();
        assert_eq!(wire[4..6], VERSION_WITH_MANIFEST_V2.to_le_bytes());
        assert_eq!(view.funded_original_v3(), inner.as_slice());
        assert_eq!(view.prompt_text_v3(), PROMPT);
        assert_eq!(view.companion_bytes_v3(), &companion);
        assert_eq!(view.manifest_bytes_v2(), Some(&manifest));
        assert_eq!(view.hf_descriptor_bytes_v2(), Some(descriptor.as_slice()));
        assert_eq!(view.manifest_bytes_v2().unwrap().as_ptr(), wire[companion_end..].as_ptr());
        assert_eq!(view.hf_descriptor_bytes_v2().unwrap().as_ptr(), wire[descriptor_offset..].as_ptr());
        assert_eq!(view.encode_v3().unwrap(), wire);

        for cut in [companion_end, descriptor_len_offset, descriptor_offset - 1, descriptor_offset, wire.len() - 1] {
            assert!(PalwTirFundedOriginalEnvelopeV3::classify_v3(&wire[..cut]).unwrap().is_err());
        }
        let mut malformed = wire.clone();
        malformed.push(0);
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&malformed).is_err());
        malformed[descriptor_len_offset..descriptor_offset].copy_from_slice(&((descriptor.len() + 1) as u16).to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&malformed).is_err(), "oversize descriptor with exact EOF");
        malformed.truncate(descriptor_offset);
        malformed[descriptor_len_offset..descriptor_offset].copy_from_slice(&0u16.to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&malformed).is_err(), "empty descriptor with exact EOF");
        let mut downgraded = wire;
        downgraded[4..6].copy_from_slice(&VERSION_V1.to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&downgraded).is_err());
        assert!(
            PalwTirFundedOriginalEnvelopeV3::from_parts_with_manifest_v2(&inner, PROMPT.as_bytes(), &companion, &manifest, &[])
                .is_err()
        );
    }

    #[test]
    fn tfo3_v2_material_uses_the_existing_total_transport_ceiling() {
        let mut family = family(&funded_original_transport_fixture_v1());
        family.semantic_table = Some(vec![]);
        let without_body = family.encode_v2().unwrap().len();
        let companion = [0; COMPANION_BYTES];
        let manifest = [0; MANIFEST_BYTES];
        let descriptor = [1; 1];
        let overhead = HEADER_BYTES + PROMPT.len() + COMPANION_BYTES + MANIFEST_BYTES + DESCRIPTOR_LEN_BYTES + descriptor.len();
        family.semantic_table = Some(vec![1; MAX_BYTES - without_body - overhead]);
        let inner = family.encode_v2().unwrap();
        let wire = PalwTirFundedOriginalEnvelopeV3::from_parts_with_manifest_v2(
            &inner,
            PROMPT.as_bytes(),
            &companion,
            &manifest,
            &descriptor,
        )
        .unwrap()
        .encode_v3()
        .unwrap();
        assert_eq!(wire.len(), MAX_BYTES);
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&wire).is_ok());

        // A structurally bounded descriptor still fails if its extra byte
        // pushes the complete version 2 carrier over the existing ceiling.
        let mut oversized = wire;
        let len_offset = oversized.len() - descriptor.len() - DESCRIPTOR_LEN_BYTES;
        oversized[len_offset..len_offset + DESCRIPTOR_LEN_BYTES].copy_from_slice(&2u16.to_le_bytes());
        oversized.push(1);
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&oversized).is_err());
        assert!(
            PalwTirFundedOriginalEnvelopeV3::from_parts_with_manifest_v2(&inner, PROMPT.as_bytes(), &companion, &manifest, &[1; 2])
                .is_err()
        );
    }

    #[test]
    fn tfo3_total_stays_within_existing_funded_original_ceiling() {
        let mut family = family(&funded_original_transport_fixture_v1());
        family.semantic_table = Some(vec![]);
        let without_body = family.encode_v2().unwrap().len();
        let body_len = MAX_BYTES - HEADER_BYTES - COMPANION_BYTES - PROMPT.len() - without_body;
        family.semantic_table = Some(vec![1; body_len]);
        let at_bound = family.encode_v2().unwrap();
        let wire = unchecked(&at_bound);
        assert_eq!(wire.len(), MAX_BYTES);
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&wire).is_ok());

        family.semantic_table.as_mut().unwrap().push(1);
        let over_bound = family.encode_v2().unwrap();
        assert_eq!(unchecked(&over_bound).len(), MAX_BYTES + 1);
        assert!(PalwTirFundedOriginalEnvelopeV3::from_parts_v3(&over_bound, PROMPT.as_bytes(), &[0; COMPANION_BYTES]).is_err());
        assert!(PalwTirFundedOriginalEnvelopeV3::decode_v3(&unchecked(&over_bound)).is_err());
    }

    #[test]
    fn tfo3_pairing_pins_the_actual_transaction_and_companion_for_each_outer_view() {
        let h = Hash64::from_u64_word;
        let base = funded_original_transport_fixture_v1();
        let literal = base.encode_v1().unwrap();
        let companion = [0; COMPANION_BYTES];
        let manifest = [19; MANIFEST_BYTES];
        let descriptor = [21; 3];
        let with_metadata = PalwTirFundedOriginalEnvelopeV3::from_parts_with_manifest_v2(
            &literal,
            PROMPT.as_bytes(),
            &companion,
            &manifest,
            &descriptor,
        )
        .unwrap()
        .encode_v3()
        .unwrap();
        let wrapped = |wire: &[u8]| {
            let answer = crate::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_encode_v1(wire, &[1, 2]);
            let prefix = PalwPrefixDependencyEnvelopeV1 {
                inner: answer.clone(),
                exports: vec![PalwSignedPrefixExportV1 {
                    intent: PalwPrefixExportIntentV1 {
                        version: 1,
                        network: h(1),
                        source_namespace: 1,
                        source_nonce: h(2),
                        base_commitment: h(3),
                        producer_key_id: h(4),
                        ordinal: 0,
                        prefix: PalwFpPrefixStateV1 { state_root: h(5), prefix_tokens: 8, class_id: h(6) },
                        liability_until: 100,
                        material_until: 120,
                    },
                    signature: vec![0; MLDSA87_SIGNATURE_LEN],
                }],
                consumed: None,
            }
            .encode_v1()
            .unwrap();
            [wire.to_vec(), answer, prefix]
        };

        // These are real transaction identities and exact structural output
        // checks over a synthetic codec fixture, without payment/admission.
        let mut ids = std::collections::BTreeSet::new();
        for (wire, has_metadata) in [(unchecked(&literal), false), (with_metadata, true)] {
            for payload in wrapped(&wire) {
                let tx = Transaction::new(
                    0,
                    vec![TransactionInput::new(TransactionOutpoint::new(h(99), 0), vec![], 0, 0)],
                    vec![
                        TransactionOutput::new(1, ScriptPublicKey::default()),
                        TransactionOutput::new(
                            base.funding.terms.funding_amount_sompi,
                            crate::mldsa87_primitives::p2pkh_mldsa87_spk(
                                &kaspa_hashes::blake2b_512_address_payload(&base.payer_pubkey).as_bytes(),
                            ),
                        ),
                    ],
                    0,
                    crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
                    0,
                    payload,
                );
                assert!(ids.insert(tx.id()));
                let paired = palw_funded_original_pairing_payload_v1(&tx, &wire).unwrap();
                assert_eq!(paired.carrier(), &tx);
                assert_eq!(paired.envelope(), &base);
                assert_eq!(paired.outpoint(), TransactionOutpoint::new(tx.id(), base.funding.terms.funding_output_index));
                assert_eq!(paired.public_companion_v3(), Some(companion.as_slice()));
                assert_eq!(paired.public_prompt_bytes_v3(), Some(PROMPT.as_bytes()));
                assert_eq!(paired.public_manifest_bytes_v2(), has_metadata.then_some(manifest.as_slice()));
                assert_eq!(paired.public_hf_descriptor_bytes_v2(), has_metadata.then_some(descriptor.as_slice()));
                if has_metadata {
                    let actual = crate::palw_tir_funded_original_envelope_v1::actual_funded_payload_v1(paired.carrier()).unwrap();
                    let view = PalwTirFundedOriginalEnvelopeV3::decode_v3(actual).unwrap();
                    let manifest_start = HEADER_BYTES + literal.len() + PROMPT.len() + COMPANION_BYTES;
                    assert_eq!(paired.public_manifest_bytes_v2().unwrap().as_ptr(), view.manifest_bytes_v2().unwrap().as_ptr());
                    assert_eq!(
                        paired.public_hf_descriptor_bytes_v2().unwrap().as_ptr(),
                        view.hf_descriptor_bytes_v2().unwrap().as_ptr()
                    );
                    let mut substituted = wire.clone();
                    substituted[manifest_start] ^= 1;
                    assert!(palw_funded_original_pairing_payload_v1(&tx, &substituted).is_err());
                }
                let mut substituted = wire.clone();
                *substituted.last_mut().unwrap() ^= 1;
                assert!(palw_funded_original_pairing_payload_v1(&tx, &substituted).is_err());
                let wrong_prompt = unchecked_prompt(&literal, b"another literal prompt");
                assert!(palw_funded_original_pairing_payload_v1(&tx, &wrong_prompt).is_err());
                assert!(palw_funded_original_pairing_payload_v1(&tx, &literal).is_err());
            }
        }
    }
}
