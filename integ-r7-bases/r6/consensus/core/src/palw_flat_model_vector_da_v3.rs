//! Flat-v5 candidate hash-unit acquisition. No response grammar carries values.
use crate::{Hash64, palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{hash::Digest, merkle5::FlatLeafHashV5, seg::NodeOpeningV1};
use std::io::{Cursor, Error, ErrorKind};

pub const FLAT_VECTOR_DEMAND_TABLE_V3: u8 = 59;
pub const FLAT_VECTOR_SLOT_TABLE_V3: u8 = 60;
pub const FLAT_VECTOR_SERVED_TABLE_V3: u8 = 61;
pub const FLAT_VECTOR_OPEN_COUNT_TABLE_V3: u8 = 62;
pub const FLAT_VECTOR_UNAVAILABLE_TABLE_V3: u8 = 63;
pub const FLAT_VECTOR_ANSWER_MAX_BYTES_V3: usize = 10 << 10;
pub const FLAT_VECTOR_OPEN_DEMANDS_MAX_V3: u32 = 32;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorUnitV3 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    /// None: node commitment. Some: flat-v5 hash leaf, never a clear value.
    pub leaf: Option<u64>,
}
impl FlatVectorUnitV3 {
    pub fn key(&self, vector: &Hash64) -> Vec<u8> {
        borsh::to_vec(&(vector, self)).unwrap()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorDemandV3 {
    pub vector: Hash64,
    pub unit: FlatVectorUnitV3,
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
impl FlatVectorDemandV3 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
            b"misaka-palw/flat-model-vector/demand/v3",
            &(self.vector, &self.unit, self.requester, self.slot, self.filed_daa),
        ))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatVectorAnswerV3 {
    pub version: u16,
    pub opening: NodeOpeningV1,
    pub leaf: Option<FlatLeafHashV5>,
}
impl FlatVectorAnswerV3 {
    pub fn authenticates(&self, d: &FlatVectorDemandV3) -> bool {
        let u = &d.unit;
        if self.version != 3
            || (self.opening.position, self.opening.occurrence, self.opening.node) != (u.position, u.occurrence, u.node)
            || !self.opening.authenticates_segment_v1(&d.segment_root, d.positions, d.node_index, d.node_count)
        {
            return false;
        }
        match (u.leaf, &self.leaf) {
            (None, None) => true,
            (Some(index), Some(leaf)) => {
                d.rank <= 4
                    && leaf.index == index
                    && leaf.dtype == d.dtype
                    && leaf.shape == d.shape[..d.rank as usize]
                    && leaf.authenticates(&self.opening.commitment)
            }
            _ => false,
        }
    }
}
fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "invalid bounded flat hash-unit answer")
}
pub fn decode_flat_vector_answer_v3(bytes: &[u8]) -> std::io::Result<FlatVectorAnswerV3> {
    if bytes.len() > FLAT_VECTOR_ANSWER_MAX_BYTES_V3 {
        return Err(invalid());
    }
    let mut r = Cursor::new(bytes);
    let version = u16::deserialize_reader(&mut r)?;
    if version != 3 {
        return Err(invalid());
    }
    let position = u32::deserialize_reader(&mut r)?;
    let occurrence = u16::deserialize_reader(&mut r)?;
    let node = u16::deserialize_reader(&mut r)?;
    let commitment = Digest::deserialize_reader(&mut r)?;
    fn path(r: &mut Cursor<&[u8]>, max: usize) -> std::io::Result<Vec<Digest>> {
        let count = u32::deserialize_reader(r)? as usize;
        if count > max || count > r.get_ref().len().saturating_sub(r.position() as usize) / 64 {
            return Err(invalid());
        }
        (0..count).map(|_| Digest::deserialize_reader(r)).collect()
    }
    let node_siblings = path(&mut r, 64)?;
    let position_root = Digest::deserialize_reader(&mut r)?;
    let position_siblings = path(&mut r, 10)?;
    let leaf = match u8::deserialize_reader(&mut r)? {
        0 => None,
        1 => Some(FlatLeafHashV5::deserialize_reader(&mut r)?),
        _ => return Err(invalid()),
    };
    if r.position() as usize != bytes.len() {
        return Err(invalid());
    }
    Ok(FlatVectorAnswerV3 {
        version,
        opening: NodeOpeningV1 { position, occurrence, node, commitment, node_siblings, position_root, position_siblings },
        leaf,
    })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatVectorServedHeaderV3 {
    pub answered_daa: u64,
    pub answer_root: Digest,
}
#[derive(BorshSerialize)]
pub struct FlatVectorServedRowV3 {
    pub header: FlatVectorServedHeaderV3,
    pub answer: Vec<u8>,
}
pub fn flat_vector_answer_root_v3(bytes: &[u8]) -> Digest {
    misaka_palw_kernel::hash::object_id(b"misaka-palw/flat-model-vector/answer/v3", &bytes)
}
impl PalwKernelRouteStateV1 {
    pub fn flat_vector_unavailable_v3(&self, vector: &Hash64) -> bool {
        self.aux_row(FLAT_VECTOR_UNAVAILABLE_TABLE_V3, &borsh::to_vec(vector).unwrap()).unwrap_or(false)
    }
    pub fn flat_vector_open_demands_v3(&self, vector: &Hash64) -> u32 {
        self.aux_row(FLAT_VECTOR_OPEN_COUNT_TABLE_V3, &borsh::to_vec(vector).unwrap()).unwrap_or(0)
    }
    pub fn flat_vector_demand_v3(&self, id: &Hash64) -> Option<FlatVectorDemandV3> {
        let d: FlatVectorDemandV3 = self.aux_row(FLAT_VECTOR_DEMAND_TABLE_V3, &borsh::to_vec(id).ok()?)?;
        (d.id() == *id).then_some(d)
    }
    pub fn flat_vector_slot_v3(&self, bond: &PalwBondKeyV2, slot: u8) -> Option<Hash64> {
        if slot >= crate::palw_model_vector_da_v2::MODEL_VECTOR_DEMAND_SLOTS_V2 {
            return None;
        }
        self.aux_row(FLAT_VECTOR_SLOT_TABLE_V3, &borsh::to_vec(&(bond, slot)).ok()?)
    }
    pub fn flat_vector_served_v3(&self, vector: &Hash64, unit: &FlatVectorUnitV3) -> Option<FlatVectorServedHeaderV3> {
        let bytes = self.aux.get(&(FLAT_VECTOR_SERVED_TABLE_V3, unit.key(vector)))?;
        FlatVectorServedHeaderV3::deserialize_reader(&mut Cursor::new(bytes)).ok()
    }
    /// Public read API: bounded authenticated answer bytes, independently usable
    /// after the requester's slot is recycled. No producer/material test hook.
    pub fn flat_vector_public_answer_v3(&self, vector: &Hash64, unit: &FlatVectorUnitV3) -> Option<Vec<u8>> {
        let bytes = self.aux.get(&(FLAT_VECTOR_SERVED_TABLE_V3, unit.key(vector)))?;
        let mut r = Cursor::new(bytes.as_slice());
        let h = FlatVectorServedHeaderV3::deserialize_reader(&mut r).ok()?;
        let count = u32::deserialize_reader(&mut r).ok()? as usize;
        let start = r.position() as usize;
        if count > FLAT_VECTOR_ANSWER_MAX_BYTES_V3 || start.checked_add(count)? != bytes.len() {
            return None;
        }
        let answer = &bytes[start..];
        (flat_vector_answer_root_v3(answer) == h.answer_root).then(|| answer.to_vec())
    }
    pub fn flat_vector_demand_reserved_v3(&self, bond: &PalwBondKeyV2) -> u128 {
        (0..crate::palw_model_vector_da_v2::MODEL_VECTOR_DEMAND_SLOTS_V2)
            .filter_map(|slot| self.flat_vector_slot_v3(bond, slot))
            .filter_map(|id| self.flat_vector_demand_v3(&id))
            .filter(|d| !d.settled && d.requester == *bond)
            .filter(|d| {
                self.flat_model_vector_header_v3(&d.vector).is_some_and(|h| !h.refuted) && !self.flat_vector_unavailable_v3(&d.vector)
            })
            .map(|d| d.bond as u128)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::{merkle5::FlatTreeV5, seg::SegmentedCommitmentsV1};
    use misaka_palw_tir::{DType, Tensor};
    fn fixture() -> (FlatVectorDemandV3, FlatVectorAnswerV3) {
        let t = Tensor::new(DType::I128, vec![129], vec![i128::MAX; 129]).unwrap();
        let tree = FlatTreeV5::of(&t).unwrap();
        let c = SegmentedCommitmentsV1::new(vec![vec![vec![tree.commitment()]]]);
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(8), 0));
        let d = FlatVectorDemandV3 {
            vector: Hash64::from_bytes([1; 64]),
            unit: FlatVectorUnitV3 { position: 0, occurrence: 0, node: 0, leaf: Some(1) },
            requester: bond,
            requester_payout: Hash64::from_bytes([2; 64]),
            poster: bond,
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
            dtype: DType::I128.tag(),
            rank: 1,
            shape: [129, 0, 0, 0],
            settled: false,
        };
        let a = FlatVectorAnswerV3 { version: 3, opening: c.node_opening(0, 0, 0).unwrap(), leaf: Some(tree.opening(1).unwrap()) };
        (d, a)
    }
    #[test]
    fn flat_hash_da_v3_exact_unit_shape_segment_and_namespace_are_authenticated() {
        let (d, a) = fixture();
        assert!(a.authenticates(&d));
        for field in 0..9 {
            let mut bad = d.clone();
            match field {
                0 => bad.unit.position += 1,
                1 => bad.unit.node += 1,
                2 => bad.unit.occurrence += 1,
                3 => bad.unit.leaf = Some(0),
                4 => bad.dtype = 0,
                5 => bad.rank = 5,
                6 => bad.shape[0] += 1,
                7 => bad.segment_root[0] ^= 1,
                _ => bad.node_index += 1,
            }
            assert!(!a.authenticates(&bad), "field {field}");
        }
        let mut bad = a.clone();
        bad.version = 2;
        assert!(!bad.authenticates(&d));
        bad = a.clone();
        bad.leaf.as_mut().unwrap().hash[0] ^= 1;
        assert!(!bad.authenticates(&d));
        let mut node = d.clone();
        node.unit.leaf = None;
        assert!(!a.authenticates(&node));
        assert!(FlatVectorAnswerV3 { leaf: None, ..a }.authenticates(&node));
        assert_ne!(d.id(), node.id());
        let tables = [
            53,
            54,
            55,
            56,
            57,
            58,
            FLAT_VECTOR_DEMAND_TABLE_V3,
            FLAT_VECTOR_SLOT_TABLE_V3,
            FLAT_VECTOR_SERVED_TABLE_V3,
            FLAT_VECTOR_OPEN_COUNT_TABLE_V3,
            FLAT_VECTOR_UNAVAILABLE_TABLE_V3,
        ];
        assert_eq!(tables.into_iter().collect::<std::collections::BTreeSet<_>>().len(), tables.len());
    }
    #[test]
    fn flat_hash_da_v3_public_read_bounds_stored_body_and_rechecks_digest() {
        let (d, a) = fixture();
        let bytes = borsh::to_vec(&a).unwrap();
        let row = FlatVectorServedRowV3 {
            header: FlatVectorServedHeaderV3 { answered_daa: 5, answer_root: flat_vector_answer_root_v3(&bytes) },
            answer: bytes.clone(),
        };
        let policy =
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::from_bytes([1; 64]), Hash64::from_bytes([2; 64]));
        let mut route = PalwKernelRouteStateV1::new(policy, None, Default::default());
        let key = (FLAT_VECTOR_SERVED_TABLE_V3, d.unit.key(&d.vector));
        let wire = borsh::to_vec(&row).unwrap();
        route.aux.insert(key.clone(), wire.clone());
        assert_eq!(route.flat_vector_public_answer_v3(&d.vector, &d.unit), Some(bytes));
        for change in 0..4 {
            let mut bad = wire.clone();
            match change {
                0 => bad[72..76].copy_from_slice(&u32::MAX.to_le_bytes()),
                1 => {
                    bad.pop();
                }
                2 => bad.push(0),
                _ => bad[76] ^= 1,
            }
            route.aux.insert(key.clone(), bad);
            assert!(route.flat_vector_public_answer_v3(&d.vector, &d.unit).is_none());
        }
    }
    #[test]
    fn flat_hash_da_v3_wire_refuses_every_truncation_surplus_and_nested_count_before_bodies() {
        let (_, a) = fixture();
        let bytes = borsh::to_vec(&a).unwrap();
        assert_eq!(decode_flat_vector_answer_v3(&bytes).unwrap(), a);
        for cut in 0..bytes.len() {
            assert!(decode_flat_vector_answer_v3(&bytes[..cut]).is_err());
        }
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(decode_flat_vector_answer_v3(&bad).is_err());
        let mut bad = bytes.clone();
        bad[0] = 2;
        assert!(decode_flat_vector_answer_v3(&bad).is_err());
        for at in [74, 142, 148, 232] {
            let mut bad = bytes.clone();
            bad[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(decode_flat_vector_answer_v3(&bad).is_err(), "count offset {at}");
        }
        assert!(decode_flat_vector_answer_v3(&vec![0; FLAT_VECTOR_ANSWER_MAX_BYTES_V3 + 1]).is_err());
        let legacy = crate::palw_model_vector_da_v2::ModelVectorAnswerV2 { opening: a.opening.clone(), tile: None };
        assert!(decode_flat_vector_answer_v3(&borsh::to_vec(&legacy).unwrap()).is_err());
        use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
        for o in [
            K::DemandFlatModelVectorV3 { vector: [1; 64], slot: 3, position: 0, occurrence: 0, node: 0, leaf: Some(1) },
            K::AnswerFlatModelVectorV3 { demand: [2; 64], answer: bytes },
            K::SettleFlatModelVectorDemandV3 { demand: [3; 64] },
        ] {
            assert_eq!(K::decode(&o.encode()).unwrap(), o);
            assert_eq!(
                crate::palw_lifecycle_objects_v2::palw_kernel_route_inner_fence_v1(&o),
                Some(crate::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelFreeV1)
            );
        }
    }
}
