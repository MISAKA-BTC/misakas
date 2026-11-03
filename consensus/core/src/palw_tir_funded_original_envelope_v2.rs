//! TFO2: new family-consented original transport; literal TFO1 bytes and all
//! legacy identities are preserved. No old pledge signs these new obligations.
use crate::palw_court_family_supplier_v1::{FamilySupplierTermsV1, SignedFamilySupplierV1};
use crate::palw_tir_funded_original_envelope_v1::{PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1, PalwTirFundedOriginalEnvelopeV1};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read, Write};
pub const MAGIC: &[u8; 4] = b"TFO2";
pub const MAX_SUPPLIERS: usize = 4;
/// Explicit lossless wire revision. Contract messages, both logical signatures,
/// old version-2 bytes and the existing outer ceiling remain unchanged.
pub const SIGNATURE_ALIAS_WIRE_VERSION_V3: u16 = 3;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirFundedOriginalEnvelopeV2 {
    pub version: u16,
    pub funded_original: Vec<u8>,
    pub suppliers: Vec<SignedFamilySupplierV1>,
    /// Canonical table inventory witness; native admission authenticates its immutable root.
    pub semantic_table: Option<Vec<u8>>,
}
fn write_supplier_alias_v3<W: Write>(supplier: &SignedFamilySupplierV1, writer: &mut W) -> io::Result<()> {
    supplier.terms.serialize(writer)?;
    supplier.supplier_signature.serialize(writer)?;
    let alias = supplier.supplier_signature == supplier.sponsor_signature;
    u8::from(!alias).serialize(writer)?;
    if !alias {
        supplier.sponsor_signature.serialize(writer)?;
    }
    Ok(())
}
fn read_supplier_alias_v3<R: Read>(reader: &mut R) -> io::Result<SignedFamilySupplierV1> {
    let terms = FamilySupplierTermsV1::deserialize_reader(reader)?;
    let supplier_signature = crate::palw_public_binding_v1::read_signature_v1(reader)?;
    let sponsor_signature = match u8::deserialize_reader(reader)? {
        0 => {
            let mut signature = Vec::new();
            signature
                .try_reserve_exact(supplier_signature.len())
                .map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e.to_string()))?;
            signature.extend_from_slice(&supplier_signature);
            signature
        }
        1 => {
            let signature = crate::palw_public_binding_v1::read_signature_v1(reader)?;
            if signature == supplier_signature {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "noncanonical duplicated signature in TFO2 wire v3"));
            }
            signature
        }
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "TFO2 wire v3 signature alias")),
    };
    Ok(SignedFamilySupplierV1 { terms, supplier_signature, sponsor_signature })
}
impl BorshSerialize for PalwTirFundedOriginalEnvelopeV2 {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.version.serialize(writer)?;
        self.funded_original.serialize(writer)?;
        if self.version == 2 {
            self.suppliers.serialize(writer)?;
        } else if self.version == SIGNATURE_ALIAS_WIRE_VERSION_V3 {
            u32::try_from(self.suppliers.len()).map_err(io::Error::other)?.serialize(writer)?;
            for supplier in &self.suppliers {
                write_supplier_alias_v3(supplier, writer)?;
            }
        } else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "TFO2 wire version"));
        }
        self.semantic_table.serialize(writer)
    }
}
impl BorshDeserialize for PalwTirFundedOriginalEnvelopeV2 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let bad = |s: &str| io::Error::new(io::ErrorKind::InvalidData, s.to_owned());
        let version = u16::deserialize_reader(r)?;
        if version != 2 && version != SIGNATURE_ALIAS_WIRE_VERSION_V3 {
            return Err(bad("TFO2 version"));
        }
        let funded_original = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1)?;
        let count = u32::deserialize_reader(r)? as usize;
        if count == 0 || count > MAX_SUPPLIERS {
            return Err(bad("TFO2 supplier count"));
        }
        let mut suppliers = Vec::with_capacity(count);
        for _ in 0..count {
            suppliers.push(if version == 2 { SignedFamilySupplierV1::deserialize_reader(r)? } else { read_supplier_alias_v3(r)? });
        }
        let semantic_table = match u8::deserialize_reader(r)? {
            0 => None,
            1 => Some(crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1)?),
            _ => return Err(bad("TFO2 semantic table option")),
        };
        let out = Self { version, funded_original, suppliers, semantic_table };
        out.check_v2().map_err(|e| bad(&e))?;
        Ok(out)
    }
}
impl PalwTirFundedOriginalEnvelopeV2 {
    pub fn check_v2(&self) -> Result<(), String> {
        if !matches!(self.version, 2 | SIGNATURE_ALIAS_WIRE_VERSION_V3)
            || self.suppliers.is_empty()
            || self.suppliers.len() > MAX_SUPPLIERS
            || self.suppliers.windows(2).any(|p| p[0].terms.role >= p[1].terms.role)
        {
            return Err("TFO2 unordered, duplicate or missing supplier roles".into());
        }
        let base = PalwTirFundedOriginalEnvelopeV1::decode_v1(&self.funded_original)?;
        for s in &self.suppliers {
            s.terms.validate_shape_v1().map_err(str::to_owned)?;
            if s.terms.claim != base.funding.terms.claim
                || s.terms.source != base.funding.terms.source_id
                || s.supplier_signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
                || s.sponsor_signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            {
                return Err("TFO2 supplier names another original or lacks joint signatures".into());
            }
        }
        let encoded = borsh::object_length(self).map_err(|e| e.to_string())?.checked_add(4).ok_or("TFO2 encoded length overflows")?;
        if encoded > PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1 {
            return Err(format!(
                "TFO2 existing outer ceiling: encoded={encoded}, limit={}, wire_version={}, base={}, roles={}, table={}",
                PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1,
                self.version,
                self.funded_original.len(),
                self.suppliers.len(),
                self.semantic_table.as_ref().map_or(0, Vec::len)
            ));
        }
        Ok(())
    }
    /// Opt into the new wire representation before UTXO signing. Equal literal
    /// signatures are stored once and reconstructed twice; verification still
    /// checks both parties against their actual current keys and exact terms.
    pub fn with_signature_aliases_v3(mut self) -> Result<Self, String> {
        self.version = SIGNATURE_ALIAS_WIRE_VERSION_V3;
        self.check_v2()?;
        Ok(self)
    }
    pub fn encode_v2(&self) -> Result<Vec<u8>, String> {
        self.check_v2()?;
        let mut bytes = MAGIC.to_vec();
        self.serialize(&mut bytes).map_err(|e| e.to_string())?;
        Ok(bytes)
    }
    pub fn decode_v2(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1 {
            return Err("TFO2 existing outer ceiling".into());
        }
        borsh::from_slice(bytes.strip_prefix(MAGIC).ok_or("TFO2 magic")?).map_err(|e| e.to_string())
    }
}
/// Structural dispatch only. Admission must consume and authenticate suppliers.
pub fn decode_funded_original_base_v2(bytes: &[u8]) -> Result<PalwTirFundedOriginalEnvelopeV1, String> {
    if bytes.starts_with(MAGIC) {
        PalwTirFundedOriginalEnvelopeV1::decode_v1(&PalwTirFundedOriginalEnvelopeV2::decode_v2(bytes)?.funded_original)
    } else {
        PalwTirFundedOriginalEnvelopeV1::decode_v1(bytes)
    }
}
crate::palw_native_resource_v1::native_struct!(PalwTirFundedOriginalEnvelopeV2;version,funded_original,suppliers,semantic_table);
impl crate::palw_native_resource_v1::NativeWireScanV1 for PalwTirFundedOriginalEnvelopeV2 {
    fn scan_native_wire_v1<R: Read>(
        reader: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        use crate::palw_native_resource_v1::NativeWireShapeV1;
        let bad = |s| io::Error::new(io::ErrorKind::InvalidData, s);
        let version = reader.short()?;
        if version != 2 && version != SIGNATURE_ALIAS_WIRE_VERSION_V3 {
            return Err(bad("TFO2 wire version"));
        }
        let mut shape = Vec::<u8>::scan_native_wire_v1(reader)?;
        let count = reader.word()? as usize;
        if count == 0 || count > MAX_SUPPLIERS {
            return Err(bad("TFO2 supplier count"));
        }
        shape = shape.allocation((count * 2) as u64, std::mem::size_of::<SignedFamilySupplierV1>())?;
        shape.collection_entries = shape.collection_entries.checked_add(count as u64).ok_or_else(|| bad("TFO2 shape overflow"))?;
        let signature = |reader: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>| -> io::Result<NativeWireShapeV1> {
            let n = reader.word()? as usize;
            if n != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
                return Err(bad("TFO2 signature width"));
            }
            reader.skip(n as u64)?;
            NativeWireShapeV1 { outer_entries: n as u64, byte_entries: n as u64, ..Default::default() }.allocation((n * 2) as u64, 1)
        };
        for _ in 0..count {
            if version == 2 {
                shape = shape.merge(SignedFamilySupplierV1::scan_native_wire_v1(reader)?)?;
            } else {
                shape = shape.merge(FamilySupplierTermsV1::scan_native_wire_v1(reader)?)?;
                let first = signature(reader)?;
                shape = shape.merge(first)?;
                match reader.byte()? {
                    // Decode creates a second real Vec. Charge that logical
                    // allocation even though its bytes are aliased on wire.
                    0 => shape = shape.merge(first)?,
                    1 => shape = shape.merge(signature(reader)?)?,
                    _ => return Err(bad("TFO2 signature alias")),
                }
            }
        }
        shape.merge(Option::<Vec<u8>>::scan_native_wire_v1(reader)?)
    }
}

#[cfg(test)]
mod signature_alias_tests {
    use super::*;
    use crate::{Hash64, palw_court_family_supplier_v1::*, palw_state_v2::PalwBondKeyV2, tx::TransactionOutpoint};
    fn consent(same_key: bool) -> (SignedFamilySupplierV1, Vec<u8>, Vec<u8>) {
        let a = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81; 32]);
        let b = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([82; 32]);
        let h = |n| Hash64::from_u64_word(n);
        let terms = FamilySupplierTermsV1 {
            version: 1,
            network: h(1),
            ruleset: h(2),
            claim: h(3),
            source: h(4),
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
        };
        let signed = sign_family_supplier_v1(terms.clone(), |who, msg, context| {
            let key = if same_key || *who == terms.supplier { &a.signing_key } else { &b.signing_key };
            Some(libcrux_ml_dsa::ml_dsa_87::sign(key, msg, context, [83; 32]).unwrap().as_ref().to_vec())
        })
        .unwrap();
        (
            signed,
            a.verification_key.as_ref().to_vec(),
            if same_key { a.verification_key.as_ref().to_vec() } else { b.verification_key.as_ref().to_vec() },
        )
    }
    #[test]
    fn tfo2_wire_v3_alias_restores_both_exact_signatures_and_checks_both_authorities() {
        for same in [false, true] {
            let (signed, supplier_key, sponsor_key) = consent(same);
            let mut wire = Vec::new();
            write_supplier_alias_v3(&signed, &mut wire).unwrap();
            let decoded = read_supplier_alias_v3(&mut wire.as_slice()).unwrap();
            assert_eq!(decoded, signed);
            assert_eq!(decoded.id_v1(), signed.id_v1());
            decoded.verify_v1(&signed.terms, 10, &supplier_key, &sponsor_key).unwrap();
            assert!(decoded.verify_v1(&signed.terms, 10, &supplier_key, &[0; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN]).is_err());
            let mut corrupt = decoded.clone();
            corrupt.sponsor_signature[0] ^= 1;
            assert!(corrupt.verify_v1(&signed.terms, 10, &supplier_key, &sponsor_key).is_err());
            let mut substituted = decoded;
            substituted.terms.role = FamilySupplierRoleV1::SemanticTable;
            assert!(substituted.verify_v1(&substituted.terms, 10, &supplier_key, &sponsor_key).is_err());
            assert!(read_supplier_alias_v3(&mut &wire[..wire.len() - 1]).is_err());
            let old = borsh::to_vec(&signed).unwrap();
            assert_eq!(
                old.len() as isize - wire.len() as isize,
                if same { (crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN + 3) as isize } else { -1 }
            );
        }
    }
    #[test]
    fn tfo2_wire_v3_preserves_legacy_bytes_and_reserves_expanded_signature_memory() {
        use crate::palw_native_resource_v1::{NativeHeapV1, NativeWireReaderV1, NativeWireScanV1};
        let (signed, _, _) = consent(true);
        // Codec/shape test only; actual TFO1 and both native admissions are
        // exercised by the full ordinary/Prefix production fixture.
        let mut envelope = PalwTirFundedOriginalEnvelopeV2 {
            version: 2,
            funded_original: vec![42],
            suppliers: vec![signed],
            semantic_table: Some(vec![7, 8]),
        };
        let legacy = borsh::to_vec(&envelope).unwrap();
        assert_eq!(legacy, borsh::to_vec(&(2u16, &envelope.funded_original, &envelope.suppliers, &envelope.semantic_table)).unwrap());
        envelope.version = SIGNATURE_ALIAS_WIRE_VERSION_V3;
        let compact = borsh::to_vec(&envelope).unwrap();
        assert_eq!(legacy.len() - compact.len(), crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN + 3);
        let mut reader = NativeWireReaderV1::new(compact.as_slice(), compact.len() as u64);
        let shape = PalwTirFundedOriginalEnvelopeV2::scan_native_wire_v1(&mut reader).unwrap();
        assert_eq!(reader.remaining(), 0);
        assert!(shape.heap >= envelope.native_heap_v1("expanded signature alias").bytes().unwrap());
        assert!(shape.byte_entries >= (2 * crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN + 3) as u64);
        let mut invalid = compact.clone();
        invalid[0..2].copy_from_slice(&4u16.to_le_bytes());
        let mut reader = NativeWireReaderV1::new(invalid.as_slice(), invalid.len() as u64);
        assert!(PalwTirFundedOriginalEnvelopeV2::scan_native_wire_v1(&mut reader).is_err());
    }
}
