//! TFO2: new family-consented original transport; literal TFO1 bytes and all
//! legacy identities are preserved. No old pledge signs these new obligations.
use crate::palw_court_family_supplier_v1::SignedFamilySupplierV1;
use crate::palw_tir_funded_original_envelope_v1::{PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1, PalwTirFundedOriginalEnvelopeV1};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};
pub const MAGIC: &[u8; 4] = b"TFO2";
pub const MAX_SUPPLIERS: usize = 4;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirFundedOriginalEnvelopeV2 {
    pub version: u16,
    pub funded_original: Vec<u8>,
    pub suppliers: Vec<SignedFamilySupplierV1>,
    /// Canonical table inventory witness; native admission authenticates its immutable root.
    pub semantic_table: Option<Vec<u8>>,
}
impl BorshDeserialize for PalwTirFundedOriginalEnvelopeV2 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let bad = |s: &str| io::Error::new(io::ErrorKind::InvalidData, s.to_owned());
        let version = u16::deserialize_reader(r)?;
        if version != 2 {
            return Err(bad("TFO2 version"));
        }
        let funded_original = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1)?;
        let count = u32::deserialize_reader(r)? as usize;
        if count == 0 || count > MAX_SUPPLIERS {
            return Err(bad("TFO2 supplier count"));
        }
        let mut suppliers = Vec::with_capacity(count);
        for _ in 0..count {
            suppliers.push(SignedFamilySupplierV1::deserialize_reader(r)?);
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
        if self.version != 2
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
        if borsh::object_length(self)
            .map_err(|e| e.to_string())?
            .checked_add(4)
            .is_none_or(|n| n > PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1)
        {
            return Err("TFO2 existing outer ceiling".into());
        }
        Ok(())
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
crate::palw_native_resource_v1::wire::wire_struct!(PalwTirFundedOriginalEnvelopeV2;u16,Vec<u8>,Vec<SignedFamilySupplierV1>,Option<Vec<u8>>);
