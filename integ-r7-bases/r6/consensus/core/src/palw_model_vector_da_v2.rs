//! Point-unit DA for candidate traces. Model-derived bytes remain verifier-supplied.
//! The responder reads only a fixed admitted request and at most one bounded tile.
use crate::{Hash64, palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{hash::Digest, seg::NodeOpeningV1};
use std::io::{Cursor, Error, ErrorKind};

pub const MODEL_VECTOR_DEMAND_TABLE_V2: u8 = 53;
pub const MODEL_VECTOR_DEMAND_SLOT_TABLE_V2: u8 = 54;
pub const MODEL_VECTOR_SERVED_TABLE_V2: u8 = 55;
pub const MODEL_VECTOR_DEMAND_COUNT_TABLE_V2: u8 = 56;
pub const MODEL_VECTOR_DEMAND_SLOTS_V2: u8 = 4;
pub const MODEL_VECTOR_ANSWER_MAX_BYTES_V2: usize = 80 << 10;
pub const MODEL_VECTOR_NODE_AXIS_V2: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelVectorUnitV2 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    /// 0 row, 1 column, 2 node commitment only (line/tile must be zero).
    pub axis: u8,
    pub line: u64,
    pub tile: u64,
}
impl ModelVectorUnitV2 {
    pub fn key(&self, vector: &Hash64) -> Vec<u8> {
        borsh::to_vec(&(vector, self)).unwrap()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelVectorDemandV2 {
    pub vector: Hash64,
    pub unit: ModelVectorUnitV2,
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
    pub settled: bool,
}
impl ModelVectorDemandV2 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
            b"misaka-palw/model-vector/demand/v2",
            &(self.vector, &self.unit, self.requester, self.slot, self.filed_daa),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelVectorServedV2 {
    pub answered_daa: u64,
    pub answer_root: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct ModelVectorAnswerV2 {
    pub opening: NodeOpeningV1,
    pub tile: Option<crate::palw_onboarding_v1::ArtifactTileOpeningV3>,
}
impl ModelVectorAnswerV2 {
    pub fn authenticates(&self, d: &ModelVectorDemandV2) -> bool {
        let u = &d.unit;
        if (self.opening.position, self.opening.occurrence, self.opening.node) != (u.position, u.occurrence, u.node)
            || !self.opening.authenticates_segment_v1(&d.segment_root, d.positions, d.node_index, d.node_count)
        {
            return false;
        }
        match (u.axis, &self.tile) {
            (MODEL_VECTOR_NODE_AXIS_V2, None) => true,
            (0 | 1, Some(t)) => {
                let l = t.leaf();
                (l.axis, l.line, l.tile) == (u.axis, u.line, u.tile) && l.authenticates(&self.opening.commitment)
            }
            _ => false,
        }
    }
}

pub fn decode_model_vector_answer_v2(data: &[u8]) -> std::io::Result<ModelVectorAnswerV2> {
    let invalid = || Error::new(ErrorKind::InvalidData, "vector answer exceeds its bounded unit grammar");
    if data.len() > MODEL_VECTOR_ANSWER_MAX_BYTES_V2 {
        return Err(invalid());
    }
    let mut r = Cursor::new(data);
    let position = u32::deserialize_reader(&mut r)?;
    let occurrence = u16::deserialize_reader(&mut r)?;
    let node = u16::deserialize_reader(&mut r)?;
    let commitment = Digest::deserialize_reader(&mut r)?;
    fn path(r: &mut Cursor<&[u8]>, max: usize) -> std::io::Result<Vec<Digest>> {
        let n = u32::deserialize_reader(r)? as usize;
        if n > max || n > r.get_ref().len().saturating_sub(r.position() as usize) / 64 {
            return Err(Error::new(ErrorKind::InvalidData, "vector answer path exceeds its bounded wire"));
        }
        (0..n).map(|_| Digest::deserialize_reader(r)).collect()
    }
    let node_siblings = path(&mut r, 64)?;
    let position_root = Digest::deserialize_reader(&mut r)?;
    let position_siblings = path(&mut r, 10)?;
    let tile = match u8::deserialize_reader(&mut r)? {
        0 => None,
        1 => Some(crate::palw_onboarding_v1::ArtifactTileOpeningV3::deserialize_reader(&mut r)?),
        _ => return Err(invalid()),
    };
    if r.position() as usize != data.len() {
        return Err(invalid());
    }
    Ok(ModelVectorAnswerV2 {
        opening: NodeOpeningV1 { position, occurrence, node, commitment, node_siblings, position_root, position_siblings },
        tile,
    })
}

impl PalwKernelRouteStateV1 {
    pub fn model_vector_open_demands_v2(&self, vector: &Hash64) -> u32 {
        self.aux_row(MODEL_VECTOR_DEMAND_COUNT_TABLE_V2, &borsh::to_vec(vector).unwrap()).unwrap_or(0)
    }
    pub fn model_vector_demand_v2(&self, id: &Hash64) -> Option<ModelVectorDemandV2> {
        let d: ModelVectorDemandV2 = self.aux_row(MODEL_VECTOR_DEMAND_TABLE_V2, &borsh::to_vec(id).ok()?)?;
        (d.id() == *id).then_some(d)
    }
    pub fn model_vector_demand_slot_v2(&self, bond: &PalwBondKeyV2, slot: u8) -> Option<Hash64> {
        if slot >= MODEL_VECTOR_DEMAND_SLOTS_V2 {
            return None;
        }
        self.aux_row(MODEL_VECTOR_DEMAND_SLOT_TABLE_V2, &borsh::to_vec(&(bond, slot)).ok()?)
    }
    pub fn model_vector_served_v2(&self, vector: &Hash64, unit: &ModelVectorUnitV2) -> Option<ModelVectorServedV2> {
        self.aux_row(MODEL_VECTOR_SERVED_TABLE_V2, &unit.key(vector))
    }
    pub fn model_vector_demand_reserved_v2(&self, bond: &PalwBondKeyV2) -> u128 {
        (0..MODEL_VECTOR_DEMAND_SLOTS_V2)
            .filter_map(|s| self.model_vector_demand_slot_v2(bond, s))
            .filter_map(|id| self.model_vector_demand_v2(&id))
            .filter(|d| !d.settled && d.requester == *bond)
            .filter(|d| self.model_vector_header_v2(&d.vector).is_some_and(|h| !h.refuted && !h.unavailable))
            .map(|d| d.bond as u128)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::seg::position_node_opening_v1;
    use misaka_palw_tir::{DType, Tensor};
    fn fixture() -> (ModelVectorDemandV2, ModelVectorAnswerV2) {
        let t = Tensor::new(DType::I8, vec![8193], vec![7; 8193]).unwrap();
        let trace = misaka_palw_kernel::trace::TraceV1 { values: vec![vec![vec![t.clone()]]; 2], inputs: Vec::new() };
        // Candidate trace v2: every value committed v3 (RFC02-T merge; the frozen helper now takes the program for its modes).
        let c = misaka_palw_kernel::seg::SegmentedCommitmentsV1::new(
            trace.values.iter().map(|p| misaka_palw_kernel::seg_scope::seg_position_commitments_v1(&[], p)).collect(),
        );
        let opening = position_node_opening_v1(0, &c.commitments[0], c.position_path(0).1, 0, 0).unwrap();
        assert_eq!(
            opening.authenticates(&c.segment_roots(), 2, 0, 1),
            opening.authenticates_segment_v1(&c.segment_roots()[0], 2, 0, 1)
        );
        let later = misaka_palw_kernel::seg::SegmentedCommitmentsV1::new(vec![c.commitments[0].clone(); 1030]);
        let later_opening = position_node_opening_v1(1027, &later.commitments[1027], later.position_path(1027).1, 0, 0).unwrap();
        assert!(later_opening.authenticates(&later.segment_roots(), 1030, 0, 1));
        assert!(later_opening.authenticates_segment_v1(&later.segment_roots()[1], 1030, 0, 1));
        assert!(!later_opening.authenticates_segment_v1(&later.segment_roots()[0], 1030, 0, 1));
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(8), 0));
        let d = ModelVectorDemandV2 {
            vector: Hash64::from_bytes([1; 64]),
            unit: ModelVectorUnitV2 { position: 0, occurrence: 0, node: 0, axis: 0, line: 0, tile: 1 },
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
            positions: 2,
            segment_root: c.segment_roots()[0],
            settled: false,
        };
        let a = ModelVectorAnswerV2 {
            opening,
            tile: Some(
                crate::palw_onboarding_v1::ArtifactTileOpeningV3::new(
                    misaka_palw_kernel::merkle3::LeafOpeningV3::of(&t, 0, 0, 1).unwrap(),
                )
                .unwrap(),
            ),
        };
        (d, a)
    }
    #[test]
    fn vector_da_answer_roundtrips_and_authenticates_exact_unit_and_segment() {
        let (d, a) = fixture();
        let bytes = borsh::to_vec(&a).unwrap();
        assert_eq!(decode_model_vector_answer_v2(&bytes).unwrap(), a);
        assert!(a.authenticates(&d));
        let mut bad = d.clone();
        bad.unit.tile = 0;
        assert!(!a.authenticates(&bad));
        assert_ne!(d.id(), bad.id());
        bad = d.clone();
        bad.segment_root[0] ^= 1;
        assert!(!a.authenticates(&bad));
        bad = d.clone();
        bad.unit.position = 1;
        assert!(!a.authenticates(&bad));
        bad = d.clone();
        bad.unit.axis = MODEL_VECTOR_NODE_AXIS_V2;
        bad.unit.tile = 0;
        assert!(!a.authenticates(&bad));
        assert!(ModelVectorAnswerV2 { tile: None, ..a }.authenticates(&bad));
        assert_ne!(d.id(), ModelVectorDemandV2 { filed_daa: 2, ..d.clone() }.id());
    }
    #[test]
    fn vector_da_answer_refuses_counts_and_trailing_bytes_before_large_allocations() {
        let (_, a) = fixture();
        let mut b = borsh::to_vec(&a).unwrap();
        b.push(0);
        assert_eq!(decode_model_vector_answer_v2(&b).unwrap_err().kind(), ErrorKind::InvalidData);
        let mut b = borsh::to_vec(&a.opening).unwrap();
        b.truncate(72);
        b.extend(65u32.to_le_bytes());
        assert_eq!(decode_model_vector_answer_v2(&b).unwrap_err().kind(), ErrorKind::InvalidData);
        let mut b = borsh::to_vec(&a.opening).unwrap();
        b.extend([1, DType::I8.tag()]);
        b.extend(5u32.to_le_bytes());
        assert_eq!(decode_model_vector_answer_v2(&b).unwrap_err().kind(), ErrorKind::InvalidData);
        assert!(decode_model_vector_answer_v2(&vec![0; MODEL_VECTOR_ANSWER_MAX_BYTES_V2 + 1]).is_err());
    }
}
