//! Build the exact v3 dual-root commitment from raw-width row-major bytes, with bounded buffers.
//! This is producer/verifier preparation, not a new commitment suite or consensus admission.
//! One pass reads each element once. Matrix column hash states retain one 4096-row tile while
//! row leaves are finalized; column leaf hashes are placed in the canonical (line,tile) order.
//! No full tensor or i128 expansion is allocated. The caller must authenticate the resulting
//! commitment against its registered param root before using the source as verification material.

use crate::hash::{Digest, finish};
use crate::merkle::{AXIS_COL, AXIS_ROW};
use crate::merkle3::{LayoutV3, TENSOR_NODE_DOMAIN_V3, TILE_V3, commit_v3, leaf_state_v3, node_hash_v3, tree_root};
use misaka_palw_tir::DType;

/// Bound on this algorithm's payload buffers, hash states and leaf folding, excluding the
/// caller's reader/cache, program metadata and allocator/process overhead. It is not measured RSS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamedCommitmentShapeV3 {
    pub tensor_bytes: u64,
    pub workspace_bytes: u64,
    pub row_leaves: u64,
    pub col_leaves: u64,
}

pub fn streamed_commitment_shape_v3(dtype: DType, shape: &[usize]) -> Result<StreamedCommitmentShapeV3, String> {
    if shape.len() > 4 {
        return Err("v3 streamed producer expects a TIR rank at most four".into());
    }
    let l = LayoutV3::try_of(shape).ok_or("overflowing tensor shape")?;
    let checked = || -> Option<_> {
        let tensor_bytes = l.len.checked_mul(dtype.width() as u64)?;
        // Twice all leaf hashes covers both leaf arrays and temporary levels during folding.
        // Hash states are required only for a nonempty nonflat tensor, one per column in a batch.
        let state_count = if shape.len() > 1 && l.len > 0 { l.n } else { 0 };
        let workspace_bytes = l
            .leaves(AXIS_ROW)
            .checked_add(l.leaves(AXIS_COL))?
            .checked_mul(128)?
            .checked_add(state_count.checked_mul(std::mem::size_of::<blake2b_simd::State>() as u64)?)?
            .checked_add(TILE_V3.checked_mul(dtype.width() as u64)?)?
            .checked_add(4096)?;
        Some(StreamedCommitmentShapeV3 {
            tensor_bytes,
            workspace_bytes,
            row_leaves: l.leaves(AXIS_ROW),
            col_leaves: l.leaves(AXIS_COL),
        })
    };
    checked().ok_or_else(|| "streamed commitment workspace or byte count overflows".into())
}

fn reserve<T>(n: u64) -> Result<Vec<T>, String> {
    let n = usize::try_from(n).map_err(|_| "buffer exceeds address space")?;
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|e| format!("streamed commitment allocation refused: {e}"))?;
    Ok(v)
}

/// `read(offset, buffer)` must fill exactly that tensor-relative byte range or return an error.
/// The workspace limit is checked before allocating or reading any tensor payload. Every dtype
/// bit pattern is a valid narrow integer, and the shared leaf prefix matches decoded hashing.
pub fn tensor_commitment_streamed_v3(
    dtype: DType,
    shape: &[usize],
    workspace_limit: u64,
    read: &mut dyn FnMut(u64, &mut [u8]) -> Result<(), String>,
) -> Result<(Digest, StreamedCommitmentShapeV3), String> {
    let bound = streamed_commitment_shape_v3(dtype, shape)?;
    if bound.workspace_bytes > workspace_limit {
        return Err(format!("streamed commitment workspace {} > limit {workspace_limit}", bound.workspace_bytes));
    }
    let l = LayoutV3::try_of(shape).ok_or("overflowing tensor shape")?;
    let mut rows: Vec<Digest> = reserve(bound.row_leaves)?;
    let mut cols: Vec<Digest> = reserve(bound.col_leaves)?;
    cols.resize(bound.col_leaves as usize, [0; 64]);
    let width = dtype.width();
    let mut buf: Vec<u8> = reserve(TILE_V3 * width as u64)?;
    buf.resize(TILE_V3 as usize * width, 0);
    if shape.len() <= 1 {
        for tile in 0..l.row_tiles {
            let start = tile * TILE_V3;
            let count = (l.len - start).min(TILE_V3);
            let bytes = &mut buf[..count as usize * width];
            read(start * width as u64, bytes)?;
            let mut row = leaf_state_v3(dtype, AXIS_ROW, 0, tile, count);
            row.update(bytes);
            rows.push(finish(row));
            let mut col = leaf_state_v3(dtype, AXIS_COL, 0, tile, count);
            col.update(bytes);
            cols[tile as usize] = finish(col);
        }
    } else if l.len > 0 {
        let mut states: Vec<blake2b_simd::State> = reserve(l.n)?;
        let batches = l.len / (l.m * l.n);
        for batch in 0..batches {
            for tile in 0..l.col_tiles {
                let start = tile * TILE_V3;
                let count = (l.m - start).min(TILE_V3);
                states.clear();
                for j in 0..l.n {
                    states.push(leaf_state_v3(dtype, AXIS_COL, batch * l.n + j, tile, count));
                }
                for i in start..start + count {
                    let row_line = batch * l.m + i;
                    for rt in 0..l.row_tiles {
                        let first = rt * TILE_V3;
                        let count = (l.n - first).min(TILE_V3);
                        let bytes = &mut buf[..count as usize * width];
                        read((row_line * l.n + first) * width as u64, bytes)?;
                        let mut row = leaf_state_v3(dtype, AXIS_ROW, row_line, rt, count);
                        row.update(bytes);
                        rows.push(finish(row));
                        for (j, value) in bytes.chunks_exact(width).enumerate() {
                            states[first as usize + j].update(value);
                        }
                    }
                }
                for (j, state) in states.drain(..).enumerate() {
                    cols[((batch * l.n + j as u64) * l.col_tiles + tile) as usize] = finish(state);
                }
            }
        }
    }
    let row = tree_root(rows, node_hash_v3, TENSOR_NODE_DOMAIN_V3);
    let col = tree_root(cols, node_hash_v3, TENSOR_NODE_DOMAIN_V3);
    Ok((commit_v3(dtype, shape, &row, &col), bound))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle3::{LeafOpeningV3, TreesV3, tensor_commitment_v3};
    use misaka_palw_tir::Tensor;

    #[test]
    fn streamed_raw_commitments_match_decoded_roots_and_authenticate_existing_openings() {
        for dtype in DType::ALL {
            // Partial tiles, odd leaf counts, flat and batched columns, and both axes >4096.
            for shape in [
                vec![],
                vec![0],
                vec![1],
                vec![9],
                vec![4097],
                vec![2, 7],
                vec![7, 2],
                vec![2, 3, 5],
                vec![2, 1, 3, 7],
                vec![4101, 3],
                vec![3, 4101],
            ] {
                let len = shape.iter().product::<usize>();
                let data = (0..len)
                    .map(|i| match i % 7 {
                        0 => dtype.min_value(),
                        1 => dtype.max_value(),
                        _ => ((i * 17) % 127) as i128,
                    })
                    .collect();
                let t = Tensor::new(dtype, shape.clone(), data).unwrap();
                let bytes = t.to_le_bytes();
                let mut read_bytes = 0;
                let bound = streamed_commitment_shape_v3(dtype, &shape).unwrap();
                let (root, actual) = tensor_commitment_streamed_v3(dtype, &shape, bound.workspace_bytes, &mut |offset, out| {
                    let start = offset as usize;
                    out.copy_from_slice(&bytes[start..start + out.len()]);
                    read_bytes += out.len();
                    Ok(())
                })
                .unwrap();
                assert_eq!(root, tensor_commitment_v3(&t), "{dtype:?} {shape:?}");
                assert_eq!(actual, bound);
                assert_eq!(read_bytes, bytes.len(), "every payload byte is read exactly once");
                let trees = TreesV3::of(&t);
                for axis in [AXIS_ROW, AXIS_COL] {
                    let (lines, tiles) = if axis == AXIS_ROW {
                        (trees.layout.rows, trees.layout.row_tiles)
                    } else {
                        (trees.layout.cols, trees.layout.col_tiles)
                    };
                    if lines > 0 && tiles > 0 {
                        assert!(LeafOpeningV3::of_trees(&t, &trees, axis, lines - 1, tiles - 1).unwrap().authenticates(&root));
                    }
                }
            }
        }
    }

    #[test]
    fn bounds_overflow_and_reader_failure_are_refusals_before_large_allocation_or_partial_success() {
        let mut calls = 0;
        for shape in [vec![usize::MAX, 2], vec![1, 1, 1, 1, 1], vec![151_936, 1536]] {
            assert!(
                tensor_commitment_streamed_v3(DType::I16, &shape, 1, &mut |_, _| {
                    calls += 1;
                    Ok(())
                })
                .is_err()
            );
        }
        assert_eq!(calls, 0);
        assert!(streamed_commitment_shape_v3(DType::I128, &[usize::MAX]).is_err());
        let error = tensor_commitment_streamed_v3(DType::I8, &[7], 1 << 20, &mut |_, _| Err("truncated source".into())).unwrap_err();
        assert_eq!(error, "truncated source");
        let bytes = vec![17; 4101];
        let (first, _) = tensor_commitment_streamed_v3(DType::I8, &[4101], 1 << 20, &mut |o, b| {
            b.copy_from_slice(&bytes[o as usize..o as usize + b.len()]);
            Ok(())
        })
        .unwrap();
        let mut other = bytes;
        other[4100] ^= 1;
        let (last, _) = tensor_commitment_streamed_v3(DType::I8, &[4101], 1 << 20, &mut |o, b| {
            b.copy_from_slice(&other[o as usize..o as usize + b.len()]);
            Ok(())
        })
        .unwrap();
        assert_ne!(first, last, "a changed last partial leaf cannot keep the root");
    }
    #[test]
    fn simultaneous_row_and_column_tile_boundaries_keep_the_canonical_leaf_order() {
        let shape = vec![4097, 4097];
        let len = 4097 * 4097;
        let t = Tensor::new(DType::I8, shape.clone(), (0..len).map(|i| ((i * 17 + 39) % 256) as i128 - 128).collect()).unwrap();
        let bytes = t.to_le_bytes();
        let mut read = 0;
        let (root, bound) = tensor_commitment_streamed_v3(DType::I8, &shape, 8 << 20, &mut |o, out| {
            out.copy_from_slice(&bytes[o as usize..o as usize + out.len()]);
            read += out.len();
            Ok(())
        })
        .unwrap();
        assert_eq!(read, bytes.len());
        assert!(bound.workspace_bytes < 8 << 20);
        assert_eq!(root, tensor_commitment_v3(&t));
    }
}
