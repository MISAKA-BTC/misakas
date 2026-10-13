//! New scope25 trace answers. Frozen hash3 framing is reused only for its
//! original authenticated Node/flat-hash openings; typed values/headers carry25.
use crate::{palw_flat_model_vector_da_v3::*, palw_flat_model_vector_da_v9::*};
use misaka_palw_kernel::flat_detect25::{FlatHeaderValueV25, FlatValueV25};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeTraceAnswerV29 {
    Hash(FlatVectorAnswerV3),
    Value(FlatValueV25),
    Header { node: FlatVectorAnswerV3, value: FlatHeaderValueV25 },
}
impl NativeTraceAnswerV29 {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let (tag, body) = match self {
            Self::Hash(a) => (0, borsh::to_vec(a)?),
            Self::Value(a) => (1, a.to_bytes()?),
            Self::Header { node, value } => {
                let n = borsh::to_vec(node)?;
                let mut b = (n.len() as u32).to_le_bytes().to_vec();
                b.extend(n);
                b.extend(value.to_bytes()?);
                (2, b)
            }
        };
        if body.len() + 7 > 128 << 10 {
            return Err(bad());
        }
        let mut raw = 29u16.to_le_bytes().to_vec();
        raw.push(tag);
        raw.extend((body.len() as u32).to_le_bytes());
        raw.extend(body);
        Ok(raw)
    }
    pub fn from_bytes(raw: &[u8]) -> std::io::Result<Self> {
        if raw.len() < 7
            || raw.len() > 128 << 10
            || raw[..2] != 29u16.to_le_bytes()
            || u32::from_le_bytes(raw[3..7].try_into().unwrap()) as usize != raw.len() - 7
        {
            return Err(bad());
        }
        match raw[2] {
            0 => decode_flat_vector_answer_v3(&raw[7..]).map(Self::Hash),
            1 => FlatValueV25::from_bytes(&raw[7..]).map(Self::Value),
            2 => {
                let n = u32::from_le_bytes(raw.get(7..11).ok_or_else(bad)?.try_into().unwrap()) as usize;
                if n > FLAT_VECTOR_ANSWER_MAX_BYTES_V3 || n > raw.len().saturating_sub(11) {
                    return Err(bad());
                }
                Ok(Self::Header {
                    node: decode_flat_vector_answer_v3(&raw[11..11 + n])?,
                    value: FlatHeaderValueV25::from_bytes(&raw[11 + n..])?,
                })
            }
            _ => Err(bad()),
        }
    }
    /// No old court/scope grant is involved. Only the unchanged Merkle opening
    /// predicate is reused on native-selected exact segment/position/node roots.
    pub fn authenticates(&self, d: &FlatVectorDemandV9) -> bool {
        let u = &d.unit;
        match (u.kind, self) {
            (FlatVectorUnitKindV9::Node | FlatVectorUnitKindV9::Hash(_), Self::Hash(a)) => {
                FlatVectorAnswerV9::Hash(a.clone()).authenticates(d)
            }
            (FlatVectorUnitKindV9::Value(index), Self::Value(a)) => {
                let Some(n) = a.value.node.as_ref() else { return false };
                if a.version != 25
                    || a.scope != d.scope_root
                    || d.rank > 4
                    || (n.position, n.occurrence, n.node) != (u.position, u.occurrence, u.node)
                    || !n.authenticates_segment_v1(&d.segment_root, d.positions, d.node_index, d.node_count)
                {
                    return false;
                }
                if let Some(l) = a.value.flat.first() {
                    a.value.flat.len() == 1
                        && a.value.legacy.is_empty()
                        && l.opening.dtype == d.dtype
                        && l.opening.shape == d.shape[..d.rank as usize]
                        && l.authenticates(&n.commitment)
                        && l.elements().is_some_and(|v| v.iter().any(|(i, _)| *i == index))
                } else if let Some(l) = a.value.legacy.first() {
                    a.value.legacy.len() == 1
                        && a.value.flat.is_empty()
                        && l.dtype == d.dtype
                        && l.shape == d.shape[..d.rank as usize]
                        && l.axis == misaka_palw_kernel::merkle::AXIS_ROW
                        && l.authenticates(&n.commitment)
                        && l.elements().is_some_and(|v| v.iter().any(|(i, _)| *i == index))
                } else {
                    false
                }
            }
            (FlatVectorUnitKindV9::Header, Self::Header { node, value }) => {
                let n = &node.opening;
                value.version == 25
                    && value.scope == d.scope_root
                    && node.version == 3
                    && node.leaf.is_none()
                    && (n.position, n.occurrence, n.node) == (u.position, u.occurrence, u.node)
                    && n.authenticates_segment_v1(&d.segment_root, d.positions, d.node_index, d.node_count)
                    && value.authenticates(&n.commitment)
            }
            _ => false,
        }
    }
}
fn bad() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "bounded original native trace answer29")
}
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativePublicUnitV29 {
    Trace(FlatVectorUnitV9),
    Source { param: u16, layer_code: u32, kind: u8, axis: u8, level: u8, index: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePublicRequestV29 {
    pub version: u16,
    pub job: Digest,
    pub paid_owner: Digest,
    pub slot: u8,
    pub unit: NativePublicUnitV29,
}
impl NativePublicRequestV29 {
    pub fn unit_id(&self) -> Digest {
        object_id(b"misaka-palw/native/text-original-public-unit/v29", &(29u16, self.job, &self.unit))
    }
}
