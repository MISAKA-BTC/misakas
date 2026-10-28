//! Explicit bounded node/hash/selected-value acquisition for mixed court7.
use crate::{Hash64, palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::Digest;
use std::io::{Cursor, Error, ErrorKind};

pub const FLAT_VECTOR_DEMAND_TABLE_V7: u8 = 73;
pub const FLAT_VECTOR_SLOT_TABLE_V7: u8 = 74;
pub const FLAT_VECTOR_SERVED_TABLE_V7: u8 = 75;
pub const FLAT_VECTOR_OPEN_COUNT_TABLE_V7: u8 = 76;
pub const FLAT_VECTOR_UNAVAILABLE_TABLE_V7: u8 = 77;
pub const FLAT_VECTOR_ANSWER_MAX_BYTES_V7: usize = 16 << 10;
pub const FLAT_VECTOR_OPEN_DEMANDS_MAX_V7: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum FlatVectorUnitKindV7 {
    Node = 0,
    Hash(u64) = 1,
    Value(u64) = 2,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorUnitV7 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub kind: FlatVectorUnitKindV7,
}
impl FlatVectorUnitV7 {
    pub fn key(&self, vector: &Hash64) -> Vec<u8> {
        borsh::to_vec(&(vector, self)).unwrap()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorDemandV7 {
    pub vector: Hash64,
    pub unit: FlatVectorUnitV7,
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
    pub dtype: u8,
    pub rank: u8,
    pub shape: [u64; 4],
    pub settled: bool,
}
impl FlatVectorDemandV7 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
            b"misaka-palw/flat-model-vector/demand/v7",
            &(self.vector, &self.unit, self.requester, self.slot, self.filed_daa),
        ))
    }
}
/// The v7 wrapper names either frozen hash-only reply3 or a selected value7.
/// All dynamic paths/leaves are decoded by the existing bounded inner decoder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatVectorAnswerV7 {
    Hash(crate::palw_flat_model_vector_da_v3::FlatVectorAnswerV3),
    Value(misaka_palw_kernel::flat_detect7::FlatValueV7),
}
impl FlatVectorAnswerV7 {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let (tag, b) = match self {
            Self::Hash(a) => (0, borsh::to_vec(a)?),
            Self::Value(a) => (1, a.to_bytes()?),
        };
        if b.len() + 3 > FLAT_VECTOR_ANSWER_MAX_BYTES_V7 {
            return Err(invalid());
        }
        let mut out = 7u16.to_le_bytes().to_vec();
        out.push(tag);
        out.extend(b);
        Ok(out)
    }
    pub fn authenticates(&self, d: &FlatVectorDemandV7) -> bool {
        let u = &d.unit;
        match (u.kind, self) {
            (FlatVectorUnitKindV7::Node | FlatVectorUnitKindV7::Hash(_), Self::Hash(a)) => {
                let leaf = if let FlatVectorUnitKindV7::Hash(i) = u.kind { Some(i) } else { None };
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
            (FlatVectorUnitKindV7::Value(index), Self::Value(a)) => {
                let Some(n) = a.value.node.as_ref() else { return false };
                if a.version != 7
                    || d.dtype != misaka_palw_tir::DType::Idx.tag()
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
            _ => false,
        }
    }
}
fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "invalid bounded mixed-court answer7")
}
pub fn decode_flat_vector_answer_v7(bytes: &[u8]) -> std::io::Result<FlatVectorAnswerV7> {
    if bytes.len() < 3 || bytes.len() > FLAT_VECTOR_ANSWER_MAX_BYTES_V7 || bytes[..2] != 7u16.to_le_bytes() {
        return Err(invalid());
    }
    match bytes[2] {
        0 => crate::palw_flat_model_vector_da_v3::decode_flat_vector_answer_v3(&bytes[3..]).map(FlatVectorAnswerV7::Hash),
        1 => misaka_palw_kernel::flat_detect7::FlatValueV7::from_bytes(&bytes[3..]).map(FlatVectorAnswerV7::Value),
        _ => Err(invalid()),
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorServedHeaderV7 {
    pub answered_daa: u64,
    pub answer_root: Digest,
}
#[derive(BorshSerialize)]
pub struct FlatVectorServedRowV7 {
    pub header: FlatVectorServedHeaderV7,
    pub answer: Vec<u8>,
}
pub fn flat_vector_answer_root_v7(bytes: &[u8]) -> Digest {
    misaka_palw_kernel::hash::object_id(b"misaka-palw/flat-model-vector/answer/v7", &bytes)
}
impl PalwKernelRouteStateV1 {
    pub fn flat_vector_unavailable_v7(&self, vector: &Hash64) -> bool {
        self.aux_row(FLAT_VECTOR_UNAVAILABLE_TABLE_V7, &borsh::to_vec(vector).unwrap()).unwrap_or(false)
    }
    pub fn flat_vector_open_demands_v7(&self, vector: &Hash64) -> u32 {
        self.aux_row(FLAT_VECTOR_OPEN_COUNT_TABLE_V7, &borsh::to_vec(vector).unwrap()).unwrap_or(0)
    }
    pub fn flat_vector_demand_v7(&self, id: &Hash64) -> Option<FlatVectorDemandV7> {
        let d: FlatVectorDemandV7 = self.aux_row(FLAT_VECTOR_DEMAND_TABLE_V7, &borsh::to_vec(id).ok()?)?;
        (d.id() == *id).then_some(d)
    }
    pub fn flat_vector_slot_v7(&self, bond: &PalwBondKeyV2, slot: u8) -> Option<Hash64> {
        if slot >= crate::palw_model_vector_da_v2::MODEL_VECTOR_DEMAND_SLOTS_V2 {
            return None;
        }
        self.aux_row(FLAT_VECTOR_SLOT_TABLE_V7, &borsh::to_vec(&(bond, slot)).ok()?)
    }
    pub fn flat_vector_served_v7(&self, vector: &Hash64, unit: &FlatVectorUnitV7) -> Option<FlatVectorServedHeaderV7> {
        let bytes = self.aux.get(&(FLAT_VECTOR_SERVED_TABLE_V7, unit.key(vector)))?;
        FlatVectorServedHeaderV7::deserialize_reader(&mut Cursor::new(bytes)).ok()
    }
    /// Public read API: bounded authenticated answer bytes, independently usable
    /// after the requester's slot is recycled. No producer/material test hook.
    pub fn flat_vector_public_answer_v7(&self, vector: &Hash64, unit: &FlatVectorUnitV7) -> Option<Vec<u8>> {
        let bytes = self.aux.get(&(FLAT_VECTOR_SERVED_TABLE_V7, unit.key(vector)))?;
        let mut r = Cursor::new(bytes.as_slice());
        let h = FlatVectorServedHeaderV7::deserialize_reader(&mut r).ok()?;
        let count = u32::deserialize_reader(&mut r).ok()? as usize;
        let start = r.position() as usize;
        if count > FLAT_VECTOR_ANSWER_MAX_BYTES_V7 || start.checked_add(count)? != bytes.len() {
            return None;
        }
        let answer = &bytes[start..];
        (flat_vector_answer_root_v7(answer) == h.answer_root).then(|| answer.to_vec())
    }
    pub fn flat_vector_demand_reserved_v7(&self, bond: &PalwBondKeyV2) -> u128 {
        (0..crate::palw_model_vector_da_v2::MODEL_VECTOR_DEMAND_SLOTS_V2)
            .filter_map(|slot| self.flat_vector_slot_v7(bond, slot))
            .filter_map(|id| self.flat_vector_demand_v7(&id))
            .filter(|d| !d.settled && d.requester == *bond)
            .filter(|d| {
                self.flat_model_vector_header_v3(&d.vector).is_some_and(|h| !h.refuted) && !self.flat_vector_unavailable_v7(&d.vector)
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
        flat_detect7::FlatValueV7,
        merkle5::{FlatLeafValueV5, FlatTreeV5},
        seg::SegmentedCommitmentsV1,
    };
    use misaka_palw_tir::{DType, Tensor};
    fn fixture(n: usize, element: u64) -> (FlatVectorDemandV7, FlatVectorAnswerV7) {
        let t = Tensor::new(DType::Idx, vec![n], (0..n).map(|x| x as i128).collect()).unwrap();
        let tree = FlatTreeV5::of(&t).unwrap();
        let c = SegmentedCommitmentsV1::new(vec![vec![vec![tree.commitment()]]]);
        let b = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0));
        let d = FlatVectorDemandV7 {
            vector: Hash64::from_bytes([1; 64]),
            unit: FlatVectorUnitV7 { position: 0, occurrence: 0, node: 0, kind: FlatVectorUnitKindV7::Value(element) },
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
            dtype: DType::Idx.tag(),
            rank: 1,
            shape: [n as u64, 0, 0, 0],
            settled: false,
        };
        let a = FlatVectorAnswerV7::Value(FlatValueV7 {
            version: 7,
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
    fn selected_da7_authenticates_requested_value_shape_node_and_exact_namespace() {
        let (merged, answer) = fixture(66, 65);
        assert!(answer.authenticates(&merged), "short final tail is merged into the preceding leaf");
        let (d, a) = fixture(130, 129);
        assert!(a.authenticates(&d));
        let b = a.to_bytes().unwrap();
        assert_eq!(decode_flat_vector_answer_v7(&b).unwrap(), a);
        assert!(crate::palw_flat_model_vector_da_v3::decode_flat_vector_answer_v3(&b).is_err());
        for field in 0..6 {
            let mut bad = d.clone();
            match field {
                0 => bad.unit.kind = FlatVectorUnitKindV7::Value(0),
                1 => bad.unit.node = 1,
                2 => bad.segment_root[0] ^= 1,
                3 => bad.shape[0] = 129,
                4 => bad.dtype = DType::I32.tag(),
                _ => bad.unit.kind = FlatVectorUnitKindV7::Node,
            };
            assert!(!a.authenticates(&bad));
        }
        if let FlatVectorAnswerV7::Value(v) = a {
            let mut bad = v;
            bad.value.flat[0].bytes[0] ^= 1;
            assert!(!FlatVectorAnswerV7::Value(bad).authenticates(&d));
        }
    }
    #[test]
    fn selected_da7_codec_refuses_trailing_truncated_oversize_unknown_and_nested_counts() {
        let (_, a) = fixture(130, 129);
        let b = a.to_bytes().unwrap();
        for n in 0..b.len() {
            assert!(decode_flat_vector_answer_v7(&b[..n]).is_err());
        }
        let mut bad = b.clone();
        bad.push(0);
        assert!(decode_flat_vector_answer_v7(&bad).is_err());
        let mut bad = b.clone();
        bad[2] = 2;
        assert!(decode_flat_vector_answer_v7(&bad).is_err());
        let mut bad = b.clone();
        bad[0] = 6;
        assert!(decode_flat_vector_answer_v7(&bad).is_err());
        assert!(decode_flat_vector_answer_v7(&vec![0; FLAT_VECTOR_ANSWER_MAX_BYTES_V7 + 1]).is_err());
        // Node siblings count follows wrapper3, value-version2, node-option1,
        // fixed node coordinates8 and commitment64. Refuse before allocating.
        let mut bad = b;
        bad[78..82].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_flat_vector_answer_v7(&bad).is_err());
    }
}
