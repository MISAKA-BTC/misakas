//! Descriptor-scoped model artifact statements (RFC02 §14.14), independent of legacy 104/105.
//! Rows and the bounded per-bond index are rooted aux tables; maturity grants only matching
//! candidate metadata admission. Neither this statement nor its signer grants execution rights.
use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1};
use crate::palw_kernel_route_v1::PalwKernelRouteStateV1;
use crate::palw_onboarding_v1::{ArtifactBindingStateV1, ArtifactMismatchProofV1, ArtifactTileOpeningV3};
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

// RFC02 corrected allocation 2026-10-11: 41–42 chunk lane, 43–45 provider court.
// The dormant first implementation reused 43–45 incorrectly; no shipping network could arm it.
pub const PALW_MODEL_ARTIFACT_BINDINGS_TABLE_V2: u8 = 46;
pub const PALW_MODEL_ARTIFACT_BOND_INDEX_TABLE_V2: u8 = 47;
pub const PALW_MODEL_ARTIFACT_CANDIDATE_TABLE_V2: u8 = 48;
pub const PALW_MODEL_ARTIFACT_BINDINGS_PER_BOND_V2: usize = 8;
pub const PALW_MODEL_ARTIFACT_PROOF_OVERHEAD_V2: usize = 128 << 10;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelArtifactBindingRowV2 {
    pub descriptor: Digest,
    pub program_root: Digest,
    pub program_bytes_len: u32,
    pub model_inventory_root: Hash64,
    pub kernel_param_root: Hash64,
    pub pc_instances: u32,
    pub max_proof_bytes: u32,
    pub binder: PalwBondKeyV2,
    pub bound_daa: u64,
    pub matures_daa: u64,
    pub final_daa: u64,
    pub reserved: u64,
    pub refuted: bool,
    pub program_bytes: Vec<u8>,
}

/// Fixed-size prefix: collateral, deadline and court pricing reads never copy the stored program.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelArtifactBindingHeaderV2 {
    pub descriptor: Digest,
    pub program_root: Digest,
    pub program_bytes_len: u32,
    pub model_inventory_root: Hash64,
    pub kernel_param_root: Hash64,
    pub pc_instances: u32,
    pub max_proof_bytes: u32,
    pub binder: PalwBondKeyV2,
    pub bound_daa: u64,
    pub matures_daa: u64,
    pub final_daa: u64,
    pub reserved: u64,
    pub refuted: bool,
}
impl ModelArtifactBindingHeaderV2 {
    pub fn id(&self) -> Hash64 {
        model_artifact_binding_id_v2(self.descriptor, self.program_root, self.model_inventory_root, self.kernel_param_root)
    }
    pub fn state_at(&self, daa: u64) -> ArtifactBindingStateV1 {
        if self.refuted {
            ArtifactBindingStateV1::Refuted
        } else if daa < self.matures_daa {
            ArtifactBindingStateV1::Pending
        } else if daa < self.final_daa {
            ArtifactBindingStateV1::Matured
        } else {
            ArtifactBindingStateV1::Final
        }
    }
    pub fn reserved_at(&self, daa: u64) -> u64 {
        if self.refuted || daa >= self.final_daa { 0 } else { self.reserved }
    }
}
impl ModelArtifactBindingRowV2 {
    pub fn id(&self) -> Hash64 {
        model_artifact_binding_id_v2(self.descriptor, self.program_root, self.model_inventory_root, self.kernel_param_root)
    }
    pub fn state_at(&self, daa: u64) -> ArtifactBindingStateV1 {
        if self.refuted {
            ArtifactBindingStateV1::Refuted
        } else if daa < self.matures_daa {
            ArtifactBindingStateV1::Pending
        } else if daa < self.final_daa {
            ArtifactBindingStateV1::Matured
        } else {
            ArtifactBindingStateV1::Final
        }
    }
    pub fn reserved_at(&self, daa: u64) -> u64 {
        if self.refuted || daa >= self.final_daa { 0 } else { self.reserved }
    }
}

pub fn model_artifact_binding_id_v2(descriptor: Digest, program: Digest, model_root: Hash64, kernel_root: Hash64) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
        b"misaka-palw/model-artifact-binding/v2",
        &(descriptor, program, model_root, kernel_root),
    ))
}

/// Conservative structural tariff, charged before canonical-program decode/validation.
/// The quadratic envelope covers declaration/reference/liveness cross checks and instance scans;
/// it is metadata work, not PWU or economic credit. Large statements must fit the same network
/// admission budget; a host's model memory is not consulted.
pub fn model_artifact_work_v2(program_bytes: usize, filing_bytes: usize) -> Option<u64> {
    let p = u64::try_from(program_bytes).ok()?;
    p.checked_mul(p)?.checked_add(p.checked_mul(64)?)?.checked_add(u64::try_from(filing_bytes).ok()?.checked_mul(64)?)
}

impl PalwKernelRouteStateV1 {
    pub fn model_artifact_binding_header_v2(&self, id: &Hash64) -> Option<ModelArtifactBindingHeaderV2> {
        let bytes = self.aux.get(&(PALW_MODEL_ARTIFACT_BINDINGS_TABLE_V2, borsh::to_vec(id).ok()?))?;
        let header = ModelArtifactBindingHeaderV2::deserialize_reader(&mut Cursor::new(bytes)).ok()?;
        (header.id() == *id).then_some(header)
    }
    pub fn model_artifact_binding_v2(&self, id: &Hash64) -> Option<ModelArtifactBindingRowV2> {
        let row: ModelArtifactBindingRowV2 = self.aux_row(PALW_MODEL_ARTIFACT_BINDINGS_TABLE_V2, &borsh::to_vec(id).ok()?)?;
        (row.id() == *id).then_some(row)
    }
    pub fn model_artifact_bindings_of_v2(&self, bond: &PalwBondKeyV2) -> Vec<Hash64> {
        let ids: Vec<Hash64> =
            self.aux_row(PALW_MODEL_ARTIFACT_BOND_INDEX_TABLE_V2, &borsh::to_vec(bond).unwrap()).unwrap_or_default();
        // Only internally written rows exist; a corrupted served index must never induce an unbounded walk.
        if ids.len() > PALW_MODEL_ARTIFACT_BINDINGS_PER_BOND_V2 {
            return Vec::new();
        }
        ids
    }
    pub fn model_artifact_reserved_at_v2(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.model_artifact_bindings_of_v2(bond)
            .iter()
            .filter_map(|id| self.model_artifact_binding_header_v2(id))
            .filter(|r| r.binder == *bond)
            .map(|r| r.reserved_at(daa) as u128)
            .sum()
    }
    pub fn model_artifact_candidate_binding_v2(&self, class: &Hash64) -> Option<Hash64> {
        self.aux_row(PALW_MODEL_ARTIFACT_CANDIDATE_TABLE_V2, &borsh::to_vec(class).ok()?)
    }
}

fn invalid() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "model artifact proof exceeds its bounds")
}
fn bytes<R: Read>(r: &mut R, max: usize) -> std::io::Result<Vec<u8>> {
    let n = u32::deserialize_reader(r)? as usize;
    if n > max {
        return Err(invalid());
    }
    let mut v = vec![0; n];
    r.read_exact(&mut v)?;
    Ok(v)
}

/// Decode only instance proofs and bounded tiles; all attacker-controlled counts are checked
/// before allocation. The exact PC cardinality is recorded by the signed statement. Legacy
/// Row's tag is rejected before its unbounded tensor-row decoder is reached.
pub fn decode_model_artifact_proof_v2(data: &[u8], instances: u32, max_bytes: u32) -> std::io::Result<ArtifactMismatchProofV1> {
    if data.len() > max_bytes as usize {
        return Err(invalid());
    }
    let mut r = Cursor::new(data);
    let tag = u8::deserialize_reader(&mut r)?;
    if !matches!(tag, 0 | 2) {
        return Err(invalid());
    }
    let count = u32::deserialize_reader(&mut r)?;
    if count != instances || count as usize > data.len() / 67 {
        return Err(invalid());
    }
    let mut by_instance = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        let key = <(u16, Option<u16>)>::deserialize_reader(&mut r)?;
        if previous.is_some_and(|p| p >= key) {
            return Err(invalid());
        }
        previous = Some(key);
        by_instance.insert(key, Digest::deserialize_reader(&mut r)?);
    }
    let commitments = ParamCommitmentsV1 { by_instance };
    let proof = if tag == 0 {
        ArtifactMismatchProofV1::Instances { commitments }
    } else {
        let param = u16::deserialize_reader(&mut r)?;
        let layer = Option::<u16>::deserialize_reader(&mut r)?;
        let kernel_tile = ArtifactTileOpeningV3::deserialize_reader(&mut r)?;
        let tensor_name = String::from_utf8(bytes(&mut r, misaka_palw_tir::program::MAX_NAME_BYTES)?).map_err(|_| invalid())?;
        let operand_layer = Option::<u16>::deserialize_reader(&mut r)?;
        let row_start = u32::deserialize_reader(&mut r)?;
        let operand_bytes = bytes(&mut r, crate::palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1 as usize)?;
        let leaf_index = u32::deserialize_reader(&mut r)?;
        let leaf_count = u32::deserialize_reader(&mut r)?;
        let n = u32::deserialize_reader(&mut r)?;
        if n > 32 {
            return Err(invalid());
        }
        let mut path = Vec::with_capacity(n as usize);
        for _ in 0..n {
            path.push(Hash64::deserialize_reader(&mut r)?);
        }
        ArtifactMismatchProofV1::TileV3 {
            commitments,
            param,
            layer,
            kernel_tile,
            v2_opening: PalwArtifactOpeningV1 {
                operand: PalwArtifactOperandV1 { tensor_name, layer: operand_layer, row_start, bytes: operand_bytes },
                leaf_index,
                leaf_count,
                path,
            },
        }
    };
    if r.position() as usize != data.len() {
        return Err(invalid());
    }
    Ok(proof)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_model_decoder_preserves_instance_wire_and_refuses_counts_and_legacy_rows() {
        let pc = ParamCommitmentsV1 { by_instance: [((0, None), [1; 64]), ((1, Some(3)), [2; 64])].into() };
        let p = ArtifactMismatchProofV1::Instances { commitments: pc };
        let wire = borsh::to_vec(&p).unwrap();
        assert_eq!(decode_model_artifact_proof_v2(&wire, 2, 4096).unwrap(), p);
        assert!(decode_model_artifact_proof_v2(&wire, 3, 4096).is_err());
        assert!(decode_model_artifact_proof_v2(&wire, 2, 1).is_err());
        assert!(decode_model_artifact_proof_v2(&[1], 0, 4096).is_err());
        assert!(decode_model_artifact_proof_v2(&[2, 255, 255, 255, 255], u32::MAX, 4096).is_err());
        let mut tail = wire.clone();
        tail.push(0);
        assert!(decode_model_artifact_proof_v2(&tail, 2, 4096).is_err());
        // Use a same-width map for duplicate/order mutation below.
        let p = ArtifactMismatchProofV1::Instances {
            commitments: ParamCommitmentsV1 { by_instance: [((0, None), [1; 64]), ((1, None), [2; 64])].into() },
        };
        let mut wire = borsh::to_vec(&p).unwrap();
        let first = wire[5..72].to_vec();
        wire[72..139].copy_from_slice(&first);
        assert!(decode_model_artifact_proof_v2(&wire, 2, 4096).is_err());
    }
    #[test]
    fn model_binding_identity_binds_every_scope_coordinate_and_reservation_uses_the_horizon() {
        let bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0));
        let mut row = ModelArtifactBindingRowV2 {
            descriptor: [1; 64],
            program_root: [2; 64],
            program_bytes_len: 0,
            program_bytes: Vec::new(),
            model_inventory_root: Hash64::from_bytes([3; 64]),
            kernel_param_root: Hash64::from_bytes([4; 64]),
            pc_instances: 0,
            max_proof_bytes: 1,
            binder: bond,
            bound_daa: 5,
            matures_daa: 10,
            final_daa: 20,
            reserved: 30,
            refuted: false,
        };
        let mut encoded = borsh::to_vec(&row).unwrap();
        encoded.truncate(encoded.len() - 4); // omit even the empty program Vec's length prefix
        let header = ModelArtifactBindingHeaderV2::deserialize_reader(&mut Cursor::new(&encoded)).unwrap();
        assert_eq!(header.id(), row.id());
        assert_eq!(header.reserved_at(19), 30);
        assert!(borsh::from_slice::<ModelArtifactBindingRowV2>(&encoded).is_err(), "a header read does not require the program body");
        let id = row.id();
        for which in 0..4 {
            let mut altered = row.clone();
            match which {
                0 => altered.descriptor[0] ^= 1,
                1 => altered.program_root[0] ^= 1,
                2 => altered.model_inventory_root = Hash64::from_bytes([7; 64]),
                _ => altered.kernel_param_root = Hash64::from_bytes([8; 64]),
            }
            assert_ne!(id, altered.id());
        }
        assert_eq!(row.state_at(9), ArtifactBindingStateV1::Pending);
        assert_eq!(row.state_at(10), ArtifactBindingStateV1::Matured);
        assert_eq!(row.reserved_at(19), 30);
        assert_eq!(row.reserved_at(20), 0);
        assert_eq!(row.state_at(20), ArtifactBindingStateV1::Final);
        row.refuted = true;
        assert_eq!(row.reserved_at(5), 0);
        assert_eq!(row.state_at(30), ArtifactBindingStateV1::Refuted);
        assert!(model_artifact_work_v2(usize::MAX, usize::MAX).is_none());
    }
}

#[cfg(test)]
mod tile_decoder_tests {
    use super::*;
    #[test]
    fn bounded_model_tile_decoder_matches_wire_and_rejects_inventory_lengths_before_reading_body() {
        let t = misaka_palw_tir::Tensor::new(misaka_palw_tir::DType::I16, vec![1], vec![3]).unwrap();
        let tile = ArtifactTileOpeningV3::new(misaka_palw_kernel::merkle3::LeafOpeningV3::of(&t, 0, 0, 0).unwrap()).unwrap();
        let pc = ParamCommitmentsV1 { by_instance: [((0, None), misaka_palw_kernel::merkle3::tensor_commitment_v3(&t))].into() };
        let p = ArtifactMismatchProofV1::TileV3 {
            commitments: pc.clone(),
            param: 0,
            layer: None,
            kernel_tile: tile.clone(),
            v2_opening: PalwArtifactOpeningV1 {
                operand: PalwArtifactOperandV1 { tensor_name: "w".into(), layer: None, row_start: 0, bytes: vec![3, 0] },
                leaf_index: 0,
                leaf_count: 1,
                path: Vec::new(),
            },
        };
        let wire = borsh::to_vec(&p).unwrap();
        assert_eq!(decode_model_artifact_proof_v2(&wire, 1, 4096).unwrap(), p);
        let mut prefix = vec![2];
        borsh::to_writer(&mut prefix, &pc).unwrap();
        borsh::to_writer(&mut prefix, &0u16).unwrap();
        borsh::to_writer(&mut prefix, &None::<u16>).unwrap();
        borsh::to_writer(&mut prefix, &tile).unwrap();
        let mut name = prefix.clone();
        name.extend_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(decode_model_artifact_proof_v2(&name, 1, 4096).unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        borsh::to_writer(&mut prefix, &"w").unwrap();
        borsh::to_writer(&mut prefix, &None::<u16>).unwrap();
        borsh::to_writer(&mut prefix, &0u32).unwrap();
        prefix.extend_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(decode_model_artifact_proof_v2(&prefix, 1, 4096).unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }
}
