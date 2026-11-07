//! Explicit bounded node/hash/selected-value acquisition for mixed court9.
use crate::{Hash64, palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::Digest;
use std::io::{Cursor, Error, ErrorKind};

pub const FLAT_VECTOR_DEMAND_TABLE_V9: u8 = 82;
pub const FLAT_VECTOR_SLOT_TABLE_V9: u8 = 83;
pub const FLAT_VECTOR_SERVED_TABLE_V9: u8 = 84;
pub const FLAT_VECTOR_OPEN_COUNT_TABLE_V9: u8 = 85;
pub const FLAT_VECTOR_UNAVAILABLE_TABLE_V9: u8 = 86;
pub const FLAT_VECTOR_ANSWER_MAX_BYTES_V9: usize = 16 << 10;
pub const FLAT_VECTOR_OPEN_DEMANDS_MAX_V9: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum FlatVectorUnitKindV9 {
    Node = 0,
    Hash(u64) = 1,
    Value(u64) = 2,
    Header = 3,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorUnitV9 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub kind: FlatVectorUnitKindV9,
}
impl FlatVectorUnitV9 {
    pub fn key(&self, vector: &Hash64) -> Vec<u8> {
        borsh::to_vec(&(vector, self)).unwrap()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorDemandV9 {
    pub vector: Hash64,
    pub unit: FlatVectorUnitV9,
    pub requester: PalwBondKeyV2,
    pub requester_payout: Hash64,
    pub poster: PalwBondKeyV2,
    pub poster_payout: Hash64,
    pub slot: u8,
    pub filed_daa: u64,
    pub deadline_daa: u64,
    pub proof_until: u64,
    pub bond: u64,
    pub node_index: u64,
    pub node_count: u64,
    pub positions: u32,
    pub segment_root: Digest,
    pub scope_root: Digest,
    pub dtype: u8,
    pub rank: u8,
    pub shape: [u64; 4],
    pub settled: bool,
}
impl FlatVectorDemandV9 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
            b"misaka-palw/flat-model-vector/demand/v9",
            &(self.vector, &self.unit, self.requester, self.slot, self.filed_daa),
        ))
    }
}
/// The v9 wrapper names either frozen hash-only reply3 or a typed value9.
/// All dynamic paths/leaves are decoded by the existing bounded inner decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatVectorAnswerV9 {
    Hash(crate::palw_flat_model_vector_da_v3::FlatVectorAnswerV3),
    Value(misaka_palw_kernel::flat_detect9::FlatValueV9),
    Header {
        node: crate::palw_flat_model_vector_da_v3::FlatVectorAnswerV3,
        value: misaka_palw_kernel::flat_detect9::FlatHeaderValueV9,
    },
}
impl FlatVectorAnswerV9 {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let (tag, b) = match self {
            Self::Hash(a) => (0, borsh::to_vec(a)?),
            Self::Value(a) => (1, a.to_bytes()?),
            Self::Header { node, value } => {
                let node_bytes = borsh::to_vec(node)?;
                let mut b = (node_bytes.len() as u32).to_le_bytes().to_vec();
                b.extend(node_bytes);
                b.extend(value.to_bytes()?);
                (2, b)
            }
        };
        if b.len() + 3 > FLAT_VECTOR_ANSWER_MAX_BYTES_V9 {
            return Err(invalid());
        }
        let mut out = 9u16.to_le_bytes().to_vec();
        out.push(tag);
        out.extend(b);
        Ok(out)
    }
    pub fn authenticates(&self, d: &FlatVectorDemandV9) -> bool {
        let u = &d.unit;
        match (u.kind, self) {
            (FlatVectorUnitKindV9::Node | FlatVectorUnitKindV9::Hash(_), Self::Hash(a)) => {
                let leaf = if let FlatVectorUnitKindV9::Hash(i) = u.kind { Some(i) } else { None };
                a.authenticates(&crate::palw_flat_model_vector_da_v3::FlatVectorDemandV3 {
                    vector: d.vector,
                    unit: crate::palw_flat_model_vector_da_v3::FlatVectorUnitV3 {
                        position: u.position,
                        occurrence: u.occurrence,
                        node: u.node,
                        leaf,
                    },
                    requester: d.requester,
                    requester_payout: d.requester_payout,
                    poster: d.poster,
                    poster_payout: d.poster_payout,
                    slot: d.slot,
                    filed_daa: d.filed_daa,
                    deadline_daa: d.deadline_daa,
                    proof_until: d.proof_until,
                    bond: d.bond,
                    node_index: d.node_index,
                    node_count: d.node_count,
                    positions: d.positions,
                    segment_root: d.segment_root,
                    dtype: d.dtype,
                    rank: d.rank,
                    shape: d.shape,
                    settled: d.settled,
                })
            }
            (FlatVectorUnitKindV9::Value(index), Self::Value(a)) => {
                let Some(n) = a.value.node.as_ref() else { return false };
                if a.version != 9
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
                        && l.elements().is_some_and(|xs| xs.iter().any(|(i, _)| *i == index))
                } else if let Some(l) = a.value.legacy.first() {
                    a.value.legacy.len() == 1
                        && a.value.flat.is_empty()
                        && l.dtype == d.dtype
                        && l.shape == d.shape[..d.rank as usize]
                        && l.axis == misaka_palw_kernel::merkle::AXIS_ROW
                        && l.authenticates(&n.commitment)
                        && l.elements().is_some_and(|xs| xs.iter().any(|(i, _)| *i == index))
                } else {
                    false
                }
            }
            (FlatVectorUnitKindV9::Header, Self::Header { node, value }) => {
                let n = &node.opening;
                if value.version != 9
                    || value.scope != d.scope_root
                    || node.version != 3
                    || node.leaf.is_some()
                    || d.rank > 4
                    || (n.position, n.occurrence, n.node) != (u.position, u.occurrence, u.node)
                    || !n.authenticates_segment_v1(&d.segment_root, d.positions, d.node_index, d.node_count)
                {
                    return false;
                }
                value.authenticates(&n.commitment)
            }
            _ => false,
        }
    }
}
fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "invalid bounded mixed-court answer9")
}
pub fn decode_flat_vector_answer_v9(bytes: &[u8]) -> std::io::Result<FlatVectorAnswerV9> {
    if bytes.len() < 3 || bytes.len() > FLAT_VECTOR_ANSWER_MAX_BYTES_V9 || bytes[..2] != 9u16.to_le_bytes() {
        return Err(invalid());
    }
    match bytes[2] {
        0 => crate::palw_flat_model_vector_da_v3::decode_flat_vector_answer_v3(&bytes[3..]).map(FlatVectorAnswerV9::Hash),
        1 => misaka_palw_kernel::flat_detect9::FlatValueV9::from_bytes(&bytes[3..]).map(FlatVectorAnswerV9::Value),
        2 => {
            let count = bytes.get(3..7).ok_or_else(invalid)?;
            let n = u32::from_le_bytes(count.try_into().unwrap()) as usize;
            if n > crate::palw_flat_model_vector_da_v3::FLAT_VECTOR_ANSWER_MAX_BYTES_V3 || n > bytes.len().saturating_sub(7) {
                return Err(invalid());
            }
            let node = crate::palw_flat_model_vector_da_v3::decode_flat_vector_answer_v3(&bytes[7..7 + n])?;
            let value = misaka_palw_kernel::flat_detect9::FlatHeaderValueV9::from_bytes(&bytes[7 + n..])?;
            Ok(FlatVectorAnswerV9::Header { node, value })
        }
        _ => Err(invalid()),
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorServedHeaderV9 {
    pub answered_daa: u64,
    pub answer_root: Digest,
}
#[derive(BorshSerialize)]
pub struct FlatVectorServedRowV9 {
    pub header: FlatVectorServedHeaderV9,
    pub answer: Vec<u8>,
}
pub fn flat_vector_answer_root_v9(bytes: &[u8]) -> Digest {
    misaka_palw_kernel::hash::object_id(b"misaka-palw/flat-model-vector/answer/v9", &bytes)
}
impl PalwKernelRouteStateV1 {
    pub fn flat_vector_unavailable_v9(&self, vector: &Hash64) -> bool {
        self.aux_row(FLAT_VECTOR_UNAVAILABLE_TABLE_V9, &borsh::to_vec(vector).unwrap()).unwrap_or(false)
    }
    pub fn flat_vector_open_demands_v9(&self, vector: &Hash64) -> u32 {
        self.aux_row(FLAT_VECTOR_OPEN_COUNT_TABLE_V9, &borsh::to_vec(vector).unwrap()).unwrap_or(0)
    }
    pub fn flat_vector_demand_v9(&self, id: &Hash64) -> Option<FlatVectorDemandV9> {
        let d: FlatVectorDemandV9 = self.aux_row(FLAT_VECTOR_DEMAND_TABLE_V9, &borsh::to_vec(id).ok()?)?;
        (d.id() == *id).then_some(d)
    }
    pub fn flat_vector_slot_v9(&self, bond: &PalwBondKeyV2, slot: u8) -> Option<Hash64> {
        if slot >= crate::palw_model_vector_da_v2::MODEL_VECTOR_DEMAND_SLOTS_V2 {
            return None;
        }
        self.aux_row(FLAT_VECTOR_SLOT_TABLE_V9, &borsh::to_vec(&(bond, slot)).ok()?)
    }
    pub fn flat_vector_served_v9(&self, vector: &Hash64, unit: &FlatVectorUnitV9) -> Option<FlatVectorServedHeaderV9> {
        let bytes = self.aux.get(&(FLAT_VECTOR_SERVED_TABLE_V9, unit.key(vector)))?;
        FlatVectorServedHeaderV9::deserialize_reader(&mut Cursor::new(bytes)).ok()
    }
    /// Public read API: bounded authenticated answer bytes, independently usable
    /// after the requester's slot is recycled. No producer/material test hook.
    pub fn flat_vector_public_answer_v9(&self, vector: &Hash64, unit: &FlatVectorUnitV9) -> Option<Vec<u8>> {
        let bytes = self.aux.get(&(FLAT_VECTOR_SERVED_TABLE_V9, unit.key(vector)))?;
        let mut r = Cursor::new(bytes.as_slice());
        let h = FlatVectorServedHeaderV9::deserialize_reader(&mut r).ok()?;
        let count = u32::deserialize_reader(&mut r).ok()? as usize;
        let start = r.position() as usize;
        if count > FLAT_VECTOR_ANSWER_MAX_BYTES_V9 || start.checked_add(count)? != bytes.len() {
            return None;
        }
        let answer = &bytes[start..];
        (flat_vector_answer_root_v9(answer) == h.answer_root).then(|| answer.to_vec())
    }
    pub fn flat_vector_demand_reserved_v9(&self, bond: &PalwBondKeyV2) -> u128 {
        (0..crate::palw_model_vector_da_v2::MODEL_VECTOR_DEMAND_SLOTS_V2)
            .filter_map(|slot| self.flat_vector_slot_v9(bond, slot))
            .filter_map(|id| self.flat_vector_demand_v9(&id))
            .filter(|d| !d.settled && d.requester == *bond)
            .filter(|d| {
                self.flat_model_vector_header_v3(&d.vector).is_some_and(|h| !h.refuted) && !self.flat_vector_unavailable_v9(&d.vector)
            })
            .map(|d| d.bond as u128)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::{
        flat_court5::FlatOperandV5,
        flat_detect9::FlatValueV9,
        merkle5::{FlatLeafValueV5, FlatTreeV5},
        seg::SegmentedCommitmentsV1,
    };
    use misaka_palw_tir::{DType, Tensor};
    fn fixture(n: usize, element: u64) -> (FlatVectorDemandV9, FlatVectorAnswerV9) {
        let t = Tensor::new(DType::Idx, vec![n], (0..n).map(|x| x as i128).collect()).unwrap();
        let tree = FlatTreeV5::of(&t).unwrap();
        let c = SegmentedCommitmentsV1::new(vec![vec![vec![tree.commitment()]]]);
        let b = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0));
        let d = FlatVectorDemandV9 {
            vector: Hash64::from_bytes([1; 64]),
            unit: FlatVectorUnitV9 { position: 0, occurrence: 0, node: 0, kind: FlatVectorUnitKindV9::Value(element) },
            requester: b,
            requester_payout: Hash64::from_bytes([2; 64]),
            poster: b,
            poster_payout: Hash64::from_bytes([3; 64]),
            slot: 0,
            filed_daa: 1,
            deadline_daa: 21,
            proof_until: 31,
            bond: 100,
            node_index: 0,
            node_count: 1,
            positions: 1,
            segment_root: c.segment_roots()[0],
            scope_root: [1; 64],
            dtype: DType::Idx.tag(),
            rank: 1,
            shape: [n as u64, 0, 0, 0],
            settled: false,
        };
        let a = FlatVectorAnswerV9::Value(FlatValueV9 {
            version: 9,
            scope: [1; 64],
            value: FlatOperandV5 {
                node: c.node_opening(0, 0, 0),
                // Respect merged 64..127-element tails in frozen flat5.
                flat: vec![FlatLeafValueV5::of(&t, &tree, tree.layout().leaf_of(element).unwrap()).unwrap()],
                legacy: vec![],
            },
        });
        (d, a)
    }
    #[test]
    fn typed_da9_authenticates_requested_value_shape_node_and_exact_namespace() {
        let (merged, answer) = fixture(66, 65);
        assert!(answer.authenticates(&merged), "short final tail is merged into the preceding leaf");
        let (d, a) = fixture(130, 129);
        assert!(a.authenticates(&d));
        let b = a.to_bytes().unwrap();
        assert_eq!(decode_flat_vector_answer_v9(&b).unwrap(), a);
        assert!(crate::palw_flat_model_vector_da_v3::decode_flat_vector_answer_v3(&b).is_err());
        for field in 0..6 {
            let mut bad = d.clone();
            match field {
                0 => bad.unit.kind = FlatVectorUnitKindV9::Value(0),
                1 => bad.unit.node = 1,
                2 => bad.segment_root[0] ^= 1,
                3 => bad.shape[0] = 129,
                4 => bad.dtype = DType::I32.tag(),
                _ => bad.unit.kind = FlatVectorUnitKindV9::Node,
            };
            assert!(!a.authenticates(&bad));
        }
        if let FlatVectorAnswerV9::Value(v) = a {
            let mut bad = v;
            bad.value.flat[0].bytes[0] ^= 1;
            assert!(!FlatVectorAnswerV9::Value(bad).authenticates(&d));
        }
    }
    #[test]
    fn typed_da9_codec_refuses_trailing_truncated_oversize_unknown_and_nested_counts() {
        let (_, a) = fixture(130, 129);
        let b = a.to_bytes().unwrap();
        for n in 0..b.len() {
            assert!(decode_flat_vector_answer_v9(&b[..n]).is_err());
        }
        let mut bad = b.clone();
        bad.push(0);
        assert!(decode_flat_vector_answer_v9(&bad).is_err());
        let mut bad = b.clone();
        bad[2] = 2;
        assert!(decode_flat_vector_answer_v9(&bad).is_err());
        let mut bad = b.clone();
        bad[0] = 6;
        assert!(decode_flat_vector_answer_v9(&bad).is_err());
        assert!(decode_flat_vector_answer_v9(&vec![0; FLAT_VECTOR_ANSWER_MAX_BYTES_V9 + 1]).is_err());
        // Node siblings count follows wrapper3, value-version2, node-option1,
        // scope64, fixed node coordinates8 and commitment64. Refuse before allocating.
        let mut bad = b;
        bad[142..146].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_flat_vector_answer_v9(&bad).is_err());
    }
    #[test]
    fn typed_da9_all_six_values_headers_and_borrowed_scope_refusals() {
        use misaka_palw_kernel::{
            flat_court5::FlatHeaderV5,
            flat_detect9::FlatHeaderValueV9,
            merkle3::{LeafOpeningV3, TreesV3},
        };
        for dtype in DType::ALL {
            for n in [1usize, 63, 64, 127, 129] {
                let t = Tensor::new(dtype, vec![n], (0..n).map(|x| (x % 17) as i128).collect()).unwrap();
                let commitment = misaka_palw_kernel::flat_court5::flat_profile_commitment_v5(&t).unwrap();
                let c = SegmentedCommitmentsV1::new(vec![vec![vec![commitment]]]);
                let (mut d, _) = fixture(130, 129);
                d.unit.kind = FlatVectorUnitKindV9::Value((n - 1) as u64);
                d.dtype = dtype.tag();
                d.shape = [n as u64, 0, 0, 0];
                d.segment_root = c.segment_roots()[0];
                let (flat, legacy, header) = if n >= 64 {
                    let tree = FlatTreeV5::of(&t).unwrap();
                    (
                        vec![FlatLeafValueV5::of(&t, &tree, tree.layout().leaf_of((n - 1) as u64).unwrap()).unwrap()],
                        vec![],
                        FlatHeaderV5::Flat {
                            dtype: dtype.tag(),
                            shape: t.shape.iter().map(|x| *x as u64).collect(),
                            root: tree.root(),
                        },
                    )
                } else {
                    let tree = TreesV3::of(&t);
                    (
                        vec![],
                        vec![LeafOpeningV3::of(&t, 0, 0, 0).unwrap()],
                        FlatHeaderV5::Legacy {
                            dtype: dtype.tag(),
                            shape: t.shape.iter().map(|x| *x as u64).collect(),
                            row_root: tree.row_root(),
                            col_root: tree.col_root(),
                        },
                    )
                };
                let mut value = FlatValueV9 {
                    version: 9,
                    scope: d.scope_root,
                    value: FlatOperandV5 { node: c.node_opening(0, 0, 0), flat, legacy },
                };
                let a = FlatVectorAnswerV9::Value(value.clone());
                assert!(a.authenticates(&d), "{dtype:?}/{n}");
                assert_eq!(decode_flat_vector_answer_v9(&a.to_bytes().unwrap()).unwrap(), a);
                value.scope[0] ^= 1;
                assert!(!FlatVectorAnswerV9::Value(value).authenticates(&d));
                d.unit.kind = FlatVectorUnitKindV9::Header;
                let node = crate::palw_flat_model_vector_da_v3::FlatVectorAnswerV3 {
                    version: 3,
                    opening: c.node_opening(0, 0, 0).unwrap(),
                    leaf: None,
                };
                let mut header = FlatHeaderValueV9 { version: 9, scope: d.scope_root, header };
                let a = FlatVectorAnswerV9::Header { node: node.clone(), value: header.clone() };
                assert!(a.authenticates(&d));
                assert_eq!(decode_flat_vector_answer_v9(&a.to_bytes().unwrap()).unwrap(), a);
                header.scope[0] ^= 1;
                assert!(!FlatVectorAnswerV9::Header { node, value: header }.authenticates(&d));
            }
        }
    }
}
