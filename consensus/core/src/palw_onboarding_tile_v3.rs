//! Bounded wire opening for the dormant artifact-binding tile court. The existing
//! kernel leaf codec remains unchanged; this filing rejects lengths before allocation.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::merkle3::{LeafOpeningV3, TILE_V3};
use misaka_palw_tir::{DType, types::MAX_RANK};
use std::io::{Error, ErrorKind, Read, Write};

/// Same bytes as a kernel v3 leaf, with at most four dimensions, 4,096 values
/// and 64 siblings. Private storage makes the bound hold for locally built proofs too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactTileOpeningV3(LeafOpeningV3);

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "artifact tile opening exceeds its bounded wire domain")
}

impl ArtifactTileOpeningV3 {
    pub fn new(leaf: LeafOpeningV3) -> std::io::Result<Self> {
        if leaf.shape.len() > MAX_RANK || leaf.values.len() > TILE_V3 as usize || leaf.siblings.len() > 64 {
            return Err(invalid());
        }
        let dtype = leaf.dtype().ok_or_else(invalid)?;
        if leaf.values.iter().any(|v| !dtype.contains(*v)) {
            return Err(invalid());
        }
        Ok(Self(leaf))
    }

    pub fn leaf(&self) -> &LeafOpeningV3 {
        &self.0
    }
}

impl BorshSerialize for ArtifactTileOpeningV3 {
    fn serialize<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.0.serialize(w)
    }
}

fn bounded_count<R: Read>(r: &mut R, max: usize) -> std::io::Result<usize> {
    let n = u32::deserialize_reader(r)? as usize;
    if n > max { Err(invalid()) } else { Ok(n) }
}

impl BorshDeserialize for ArtifactTileOpeningV3 {
    fn deserialize_reader<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let tag = u8::deserialize_reader(r)?;
        let dtype = DType::ALL.into_iter().find(|d| d.tag() == tag).ok_or_else(invalid)?;
        let rank = bounded_count(r, MAX_RANK)?;
        let mut shape = Vec::with_capacity(rank);
        for _ in 0..rank {
            shape.push(u64::deserialize_reader(r)?);
        }
        let axis = u8::deserialize_reader(r)?;
        let line = u64::deserialize_reader(r)?;
        let tile = u64::deserialize_reader(r)?;
        let count = bounded_count(r, TILE_V3 as usize)?;
        let mut values = Vec::with_capacity(count);
        let mut buf = [0u8; 16];
        for _ in 0..count {
            r.read_exact(&mut buf[..dtype.width()])?;
            values.push(dtype.decode_le(&buf[..dtype.width()]));
        }
        let count = bounded_count(r, 64)?;
        let mut siblings = Vec::with_capacity(count);
        for _ in 0..count {
            siblings.push(<[u8; 64]>::deserialize_reader(r)?);
        }
        let other_root = <[u8; 64]>::deserialize_reader(r)?;
        Ok(Self(LeafOpeningV3 { dtype: tag, shape, axis, line, tile, values, siblings, other_root }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::merkle::AXIS_ROW;
    use misaka_palw_tir::Tensor;

    #[test]
    fn bounded_tile_wire_matches_kernel_leaf_at_all_dtype_widths() {
        for dtype in DType::ALL {
            let t = Tensor::new(dtype, vec![4097], vec![1; 4097]).unwrap();
            for tile in 0..2 {
                let leaf = LeafOpeningV3::of(&t, AXIS_ROW, 0, tile).unwrap();
                let wire = borsh::to_vec(&leaf).unwrap();
                let bounded = ArtifactTileOpeningV3::new(leaf.clone()).unwrap();
                assert_eq!(borsh::to_vec(&bounded).unwrap(), wire);
                assert_eq!(ArtifactTileOpeningV3::try_from_slice(&wire).unwrap().leaf(), &leaf);
            }
        }
    }

    #[test]
    fn attacker_lengths_are_refused_before_any_field_body_is_read() {
        let t = Tensor::new(DType::I8, vec![1], vec![1]).unwrap();
        let leaf = LeafOpeningV3::of(&t, AXIS_ROW, 0, 0).unwrap();
        let wire = borsh::to_vec(&leaf).unwrap();
        // Supply just the header and the excessive count. InvalidData rather than
        // UnexpectedEof proves rejection precedes reading/allocating the field body.
        for (at, count) in [(1, 5u32), (30, 4097), (35, 65)] {
            let mut attack = wire[..at].to_vec();
            attack.extend_from_slice(&count.to_le_bytes());
            let err = ArtifactTileOpeningV3::try_from_slice(&attack).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidData, "count at {at}");
        }
        let mut huge = leaf;
        huge.values.resize(4097, 1);
        assert!(ArtifactTileOpeningV3::new(huge).is_err());
    }

    fn court_fixture(
        shape: &[usize],
        dtype: DType,
        line: u64,
        tile: u64,
        lie: bool,
    ) -> (misaka_palw_tir::TirProgramV1, crate::Hash64, crate::Hash64, crate::palw_onboarding_v1::ArtifactMismatchProofV1) {
        court_fixture_axis(shape, dtype, AXIS_ROW, line, tile, lie)
    }

    fn court_fixture_axis(
        shape: &[usize],
        dtype: DType,
        axis: u8,
        line: u64,
        tile: u64,
        lie: bool,
    ) -> (misaka_palw_tir::TirProgramV1, crate::Hash64, crate::Hash64, crate::palw_onboarding_v1::ArtifactMismatchProofV1) {
        use crate::palw_artifact::{artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1};
        use crate::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_operands_v1, palw_tir_leaf_index_v1};
        use misaka_palw_kernel::{merkle3::tensor_commitment_v3, trace::ParamCommitmentsV1};
        struct Source(std::collections::BTreeMap<(u16, Option<u16>), Vec<u8>>);
        impl PalwTirTensorSourceV1 for Source {
            fn tensor_bytes(&self, j: u16, l: Option<u16>) -> Option<std::borrow::Cow<'_, [u8]>> {
                self.0.get(&(j, l)).map(|b| std::borrow::Cow::Borrowed(b.as_slice()))
            }
        }
        let mut f = misaka_palw_tir_sketch::fixture::wide128_v1(83);
        let (&(param, layer), _) = f.params.tensors.iter().next().unwrap();
        let len = shape.iter().product();
        let truth = Tensor::new(dtype, shape.to_vec(), vec![0; len]).unwrap();
        f.program.params[param as usize].dtype = dtype;
        f.program.params[param as usize].shape = shape.iter().map(|v| *v as u32).collect();
        f.params.tensors.insert((param, layer), truth.clone());
        let source = Source(f.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect());
        let operands = palw_tir_inventory_operands_v1(&f.program, &source).unwrap();
        let root = artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
        let mut wrong = truth;
        let layout = misaka_palw_kernel::merkle3::LayoutV3::of(shape);
        let start = layout.leaf_elements(axis, line, tile).unwrap()[0];
        if lie {
            wrong.data[start as usize] = 1;
        }
        let mut pc = ParamCommitmentsV1::of_v3(&f.params);
        pc.by_instance.insert((param, layer), tensor_commitment_v3(&wrong));
        let bound = crate::Hash64::from_bytes(pc.root());
        let index = palw_tir_leaf_index_v1(&f.program, param, layer, start * dtype.width() as u64).unwrap();
        let proof = crate::palw_onboarding_v1::ArtifactMismatchProofV1::TileV3 {
            commitments: pc,
            param,
            layer,
            kernel_tile: ArtifactTileOpeningV3::new(LeafOpeningV3::of(&wrong, axis, line, tile).unwrap()).unwrap(),
            v2_opening: open_artifact_leaf_v1(&operands, index).unwrap(),
        };
        (f.program, root, bound, proof)
    }

    #[test]
    fn tile_court_compares_exact_byte_coordinates_across_ranks_widths_and_piece_boundaries() {
        use crate::palw_onboarding_v1::verify_artifact_mismatch_v1;
        for dtype in DType::ALL {
            for (shape, line, tile) in
                [(vec![9001], 0, 2), (vec![2, 9001], 1, 1), (vec![2, 3, 5000], 4, 1), (vec![2, 2, 2, 5000], 6, 1)]
            {
                let (p, root, bound, proof) = court_fixture(&shape, dtype, line, tile, true);
                assert_eq!(verify_artifact_mismatch_v1(&p, root, bound, &proof), Ok(()), "{dtype:?} {shape:?}");
                let (p, root, bound, proof) = court_fixture(&shape, dtype, line, tile, false);
                assert_eq!(
                    verify_artifact_mismatch_v1(&p, root, bound, &proof),
                    Err("the two openings agree on every byte they share")
                );
            }
        }
    }

    #[test]
    fn tile_court_authenticates_both_roots_and_refuses_copied_or_disjoint_coordinates() {
        use crate::palw_onboarding_v1::{ArtifactMismatchProofV1 as P, verify_artifact_mismatch_v1 as judge};
        let (program, root, bound, proof) = court_fixture(&[2, 9001], DType::I32, 1, 1, true);
        assert!(judge(&program, crate::Hash64::from_bytes([0; 64]), bound, &proof).is_err());
        assert!(judge(&program, root, crate::Hash64::from_bytes([0; 64]), &proof).is_err());
        for attack in 0..5 {
            let mut copied = proof.clone();
            let P::TileV3 { param, kernel_tile, v2_opening, .. } = &mut copied else { unreachable!() };
            match attack {
                0 => *param = u16::MAX,
                1 => v2_opening.operand.bytes[0] ^= 1,
                2 => v2_opening.leaf_index ^= 1,
                3 => {
                    let mut leaf = kernel_tile.leaf().clone();
                    leaf.tile = 0;
                    *kernel_tile = ArtifactTileOpeningV3::new(leaf).unwrap();
                }
                _ => {
                    let mut leaf = kernel_tile.leaf().clone();
                    leaf.shape = vec![u64::MAX, 2];
                    *kernel_tile = ArtifactTileOpeningV3::new(leaf).unwrap();
                }
            }
            assert!(judge(&program, root, bound, &copied).is_err(), "attack {attack}");
        }
        // Both openings authenticate, but an inventory leaf from another row has no overlap.
        let (_, _, _, other) = court_fixture(&[2, 9001], DType::I32, 0, 0, false);
        let P::TileV3 { v2_opening: disjoint, .. } = other else { unreachable!() };
        let mut copied = proof;
        let P::TileV3 { v2_opening, .. } = &mut copied else { unreachable!() };
        *v2_opening = disjoint;
        assert_eq!(judge(&program, root, bound, &copied), Err("the two openings cover no common byte"));
    }

    #[test]
    fn authenticated_wrong_metadata_is_a_binding_fault_and_legacy_wire_is_unchanged() {
        use crate::palw_onboarding_v1::{ArtifactMismatchProofV1 as P, verify_artifact_mismatch_v1 as judge};
        use misaka_palw_kernel::{merkle::TensorOpeningV1, merkle3::tensor_commitment_v3, trace::ParamCommitmentsV1};
        let (program, root, _, proof) = court_fixture(&[2, 5000], DType::I32, 0, 0, false);
        for (shape, dtype) in [(vec![1, 10000], DType::I32), (vec![2, 5000], DType::I8)] {
            let mut fault = proof.clone();
            let P::TileV3 { commitments, param, layer, kernel_tile, .. } = &mut fault else { unreachable!() };
            let wrong = Tensor::new(dtype, shape, vec![0; 10000]).unwrap();
            commitments.by_instance.insert((*param, *layer), tensor_commitment_v3(&wrong));
            *kernel_tile = ArtifactTileOpeningV3::new(LeafOpeningV3::of(&wrong, AXIS_ROW, 0, 0).unwrap()).unwrap();
            assert_eq!(judge(&program, root, crate::Hash64::from_bytes(commitments.root()), &fault), Ok(()));
        }
        let empty = ParamCommitmentsV1::default();
        let mut old = vec![0];
        old.extend(borsh::to_vec(&empty).unwrap());
        assert_eq!(borsh::to_vec(&P::Instances { commitments: empty.clone() }).unwrap(), old);
        let P::TileV3 { v2_opening, .. } = proof else { unreachable!() };
        let row = TensorOpeningV1::row(&Tensor::new(DType::I32, vec![1], vec![0]).unwrap(), 0).unwrap();
        let mut old = vec![1];
        old.extend(borsh::to_vec(&(empty.clone(), 0u16, None::<u16>, row.clone(), v2_opening.clone())).unwrap());
        assert_eq!(borsh::to_vec(&P::Row { commitments: empty, param: 0, layer: None, kernel_row: row, v2_opening }).unwrap(), old);
    }
    #[test]
    fn strided_column_tiles_use_exact_indices_and_convict_a_forged_column_root() {
        use crate::palw_onboarding_v1::{ArtifactMismatchProofV1 as P, verify_artifact_mismatch_v1 as judge};
        use misaka_palw_kernel::merkle::AXIS_COL;
        for dtype in DType::ALL {
            for (shape, line, tile) in
                [(vec![9001], 0, 1), (vec![9001, 2], 1, 1), (vec![2, 9001, 3], 4, 1), (vec![2, 2, 9001, 3], 7, 1)]
            {
                let (p, root, bound, proof) = court_fixture_axis(&shape, dtype, AXIS_COL, line, tile, true);
                assert_eq!(judge(&p, root, bound, &proof), Ok(()), "column {dtype:?} {shape:?}");
                let (p, root, bound, proof) = court_fixture_axis(&shape, dtype, AXIS_COL, line, tile, false);
                assert_eq!(judge(&p, root, bound, &proof), Err("the two openings agree on every byte they share"));
            }
        }
        let (p, root, _, mut forged) = court_fixture_axis(&[9001, 2], DType::I32, AXIS_COL, 0, 0, true);
        let (_, _, _, honest) = court_fixture_axis(&[9001, 2], DType::I32, AXIS_COL, 0, 0, false);
        let P::TileV3 { kernel_tile: correct, .. } = honest else { unreachable!() };
        let P::TileV3 { commitments, param, layer, kernel_tile, .. } = &mut forged else { unreachable!() };
        let mut leaf = kernel_tile.leaf().clone();
        // Keep the true row root, and forge ONLY the column root. Honest row
        // comparisons cannot convict this; the column court must do so.
        leaf.other_root = correct.leaf().other_root;
        let commitment = leaf.recomputed_commitment().unwrap();
        commitments.by_instance.insert((*param, *layer), commitment);
        let bound = crate::Hash64::from_bytes(commitments.root());
        *kernel_tile = ArtifactTileOpeningV3::new(leaf).unwrap();
        assert_eq!(judge(&p, root, bound, &forged), Ok(()));
        assert_eq!(borsh::to_vec(&forged).unwrap()[0], 2, "the new variant is appended");
    }
}
