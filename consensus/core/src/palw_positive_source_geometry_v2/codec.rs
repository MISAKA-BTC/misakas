//! Raw existing G14 bodies selected by the accepted request kind/input form.
//! No new wire discriminants or commitments are defined here.
use super::*;
use crate::{palw_artifact::PalwArtifactOperandV1, palw_step_leg::PalwStepOpeningV1};
/// This decoder is deliberately explicit: an untrusted Borsh vector length is
/// compared with both the protocol bound and the remaining file before allocation.
struct Reader<'a> {
    rest: &'a [u8],
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], PositiveSourceGeometryErrorV2> {
        if n > self.rest.len() {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        let (bytes, rest) = self.rest.split_at(n);
        self.rest = rest;
        Ok(bytes)
    }
    fn u8(&mut self) -> Result<u8, PositiveSourceGeometryErrorV2> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, PositiveSourceGeometryErrorV2> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, PositiveSourceGeometryErrorV2> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, PositiveSourceGeometryErrorV2> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn hash(&mut self) -> Result<Hash64, PositiveSourceGeometryErrorV2> {
        Ok(Hash64::from_bytes(self.take(64)?.try_into().unwrap()))
    }
    fn bytes(&mut self) -> Result<Vec<u8>, PositiveSourceGeometryErrorV2> {
        let n = self.u32()? as usize;
        let bytes = self.take(n)?;
        let mut out = Vec::new();
        out.try_reserve_exact(n).map_err(|_| PositiveSourceGeometryErrorV2::Budget)?;
        out.extend_from_slice(bytes);
        Ok(out)
    }
    fn hashes(&mut self) -> Result<Vec<Hash64>, PositiveSourceGeometryErrorV2> {
        let n = self.u32()? as usize;
        if n > 32 || n > self.rest.len() / 64 {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        (0..n).map(|_| self.hash()).collect()
    }
    fn ids(&mut self, max: usize) -> Result<Vec<u32>, PositiveSourceGeometryErrorV2> {
        let n = self.u32()? as usize;
        if n > max || n > self.rest.len() / 4 {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        let mut out = Vec::new();
        out.try_reserve_exact(n).map_err(|_| PositiveSourceGeometryErrorV2::Budget)?;
        for _ in 0..n {
            out.push(self.u32()?);
        }
        Ok(out)
    }
    fn opening(&mut self) -> Result<PalwStepOpeningV1, PositiveSourceGeometryErrorV2> {
        Ok(PalwStepOpeningV1 { leaf_index: self.u64()?, leaf_hash: self.hash()?, siblings: self.hashes()? })
    }
}

impl TirSourceGeometryV2 {
    /// Bound the complete body BEFORE vector allocation, decode the existing
    /// opening, then authenticate exact registered coordinates/data identity.
    /// This is not a native receipt: only the actual accepted event fold may
    /// attach its authenticated origin and commit the resulting row.
    pub fn decode_g14_response_v2(
        &self,
        request: &SourceRequest,
        bytes: &[u8],
    ) -> Result<(u64, StoredUnitV2), PositiveSourceGeometryErrorV2> {
        let range = self.request_bound(request)?;
        if bytes.len() as u64 > range.max_unit_bytes {
            return Err(PositiveSourceGeometryErrorV2::Budget);
        }
        let mut r = Reader { rest: bytes };
        let (index, unit) = match request.kind {
            SourceKind::ArtifactInventory => {
                let tensor_name = String::from_utf8(r.bytes()?).map_err(|_| PositiveSourceGeometryErrorV2::Binding)?;
                let layer = match r.u8()? {
                    0 => None,
                    1 => Some(r.u16()?),
                    _ => return Err(PositiveSourceGeometryErrorV2::Binding),
                };
                let row_start = r.u32()?;
                let operand = PalwArtifactOperandV1 { tensor_name, layer, row_start, bytes: r.bytes()? };
                let opening = PalwArtifactOpeningV1 { operand, leaf_index: r.u32()?, leaf_count: r.u32()?, path: r.hashes()? };
                (u64::from(opening.leaf_index), StoredUnitV2::Artifact(opening))
            }
            SourceKind::Input => match self.input_form() {
                PalwPromptIdsFormV1::Flat => (0, StoredUnitV2::FlatInput(r.ids(self.prefill as usize)?)),
                PalwPromptIdsFormV1::MerkleV1 => {
                    let opening = PalwPromptIdsOpeningV1 {
                        prompt_token_count: r.u32()?,
                        tile_index: r.u32()?,
                        tile_ids: r.ids(PALW_PROMPT_IDS_TILE_LEN as usize)?,
                        opening: r.opening()?,
                    };
                    (u64::from(opening.tile_index), StoredUnitV2::Input(opening))
                }
            },
        };
        if !r.rest.is_empty() || !(request.first..request.end).contains(&index) {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        let bound = self.check_unit(request.kind, index, &unit)?;
        let (data, body) = unit.sizes()?;
        if data != bound.max_data_bytes || body != bytes.len() as u64 || body > bound.max_unit_bytes {
            return Err(PositiveSourceGeometryErrorV2::Binding);
        }
        Ok((index, unit))
    }
}
