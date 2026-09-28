//! **RFC-0002 Phase F (F3): the TIR inventory — the artifact layout an IR class's `artifact_root`
//! commits to** (`docs/design/palw/tir/phase-f-integration.md` §2.10).
//!
//! An IR class's weights are the params of its `TirProgramV1`, so the inventory is a function of
//! the program's declarations and nothing else — no profile, no per-lineage table:
//!
//! * leaves in DECLARATION order: for each param `j`, for each instance (`None` for a global; for a
//!   per-layer param every layer whose scheduled block references it, ascending), for each row;
//! * a row is one axis-0 slice of the tensor (`prod(shape[1..]) · width(dtype)` bytes) when the rank
//!   is at least 2, and the whole tensor when the rank is 0 or 1 — a per-channel vector (a narrowing's
//!   `m`, `s` or `z`) is opened whole, as the legacy per-channel tables are (`(0, 9·channels)`);
//! * a row longer than [`PALW_TIR_ROW_PIECE_BYTES_V1`] (32 KiB) is split into consecutive pieces of
//!   that size, the last one shorter;
//! * each leaf is the existing artifact leaf ([`artifact_leaf_parts_v1`]) over
//!   `PalwArtifactOperandV1 { tensor_name: ParamDecl.name, layer, row_start: byte offset of the piece
//!   in the tensor, bytes }`, and the root is the existing tree ([`artifact_root_v1`], odd nodes
//!   promoted) — so every opening an IR court checks is an ordinary artifact opening.
//!
//! Lowerers store matmul weights output-major (`[N, K]`), so a vocabulary or output tile opens
//! exactly its `tile_len` rows. Every leaf's position is a closed form of the declarations
//! ([`palw_tir_leaf_index_v1`]), which is what lets a court name the leaf a cone's operand lives
//! in without the artifact, and lets "every byte is covered exactly once, in one order" be a property
//! of the program rather than of the producer.

use crate::Hash64;
use crate::palw_artifact::{
    PalwArtifactMerkleFrontierV1, PalwArtifactOpeningV1, PalwArtifactOperandV1, artifact_leaf_parts_v1, artifact_leaf_v1,
    artifact_node_v1,
};
use misaka_palw_tir::{Ref, TirProgramV1};
use std::borrow::Cow;
use std::collections::BTreeSet;

/// **The largest leaf: a row longer than this is split into pieces of this size.**
///
/// 32 KiB, so every leaf of an IR inventory can be opened by a readiness-V2 possession proof, whose
/// largest openable leaf is `PALW_READINESS_V2_LEAF_MAX_BYTES_V1` (40 KiB): a leaf above that can
/// never be proved, and a class holding one could never seat. (It was 64 KiB before F3 shipped: a
/// per-channel vector over a 151,936-token vocabulary would have been such a leaf.) A power of two
/// under the ceiling, with the ceiling's slack left for the frame.
pub const PALW_TIR_ROW_PIECE_BYTES_V1: u64 = 32_768;
const _: () = assert!(PALW_TIR_ROW_PIECE_BYTES_V1 as usize <= crate::palw_model_registry_v1::PALW_READINESS_V2_LEAF_MAX_BYTES_V1);

/// Key of [`palw_tir_graph_ir_root_v1`] (design §2.3).
pub const PALW_TIR_GRAPH_IR_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/tir/graph-ir-root/v1";

/// **`graph_ir_root = H64(key "misaka-palw/tir/graph-ir-root/v1", program)`** over the program's
/// canonical bytes (design §2.3) — what a container's embedded program is checked against, and what
/// the class id commits to.
pub fn palw_tir_graph_ir_root_v1(program_bytes: &[u8]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_TIR_GRAPH_IR_ROOT_DOMAIN_V1).to_state();
    state.update(program_bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Why a TIR inventory could not be laid out or built.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwTirInventoryError {
    #[error("the program declares no param, so its inventory has no leaf and no root")]
    Empty,
    #[error("param `{name}` is {bytes} bytes: a row offset is a u32, so a tensor must stay under 4 GiB")]
    TensorTooLarge { name: String, bytes: u64 },
    #[error("the inventory has {0} leaves, more than a u32 leaf index can address")]
    TooManyLeaves(u64),
    #[error("param `{name}` (layer {layer:?}): no tensor supplied")]
    Missing { name: String, layer: Option<u16> },
    #[error("param `{name}` (layer {layer:?}): {got} bytes supplied, the declaration needs {want}")]
    Length { name: String, layer: Option<u16>, got: u64, want: u64 },
    #[error("leaf {index} is outside an inventory of {count}")]
    IndexOutOfRange { index: u32, count: u32 },
}

/// One leaf's coordinates: which instance of which param, and which bytes of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirInventoryRowV1 {
    pub param: u16,
    pub layer: Option<u16>,
    /// Byte offset of the piece within the tensor.
    pub row_start: u32,
    pub len: u32,
}

/// The instances of every param: `[None]` for a global; for a per-layer param every layer whose
/// scheduled block references it, ascending. (A per-layer param that no scheduled block reads has no
/// instance; normal form NF-11 makes that impossible for a validated program.)
pub fn palw_tir_param_instances_v1(p: &TirProgramV1) -> Vec<Vec<Option<u16>>> {
    let mut readers: Vec<BTreeSet<u8>> = vec![BTreeSet::new(); p.params.len()];
    for (bi, b) in p.blocks.iter().enumerate() {
        for n in &b.nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = r
                    && let Some(set) = readers.get_mut(*j as usize)
                {
                    set.insert(bi as u8);
                }
            }
        }
    }
    p.params
        .iter()
        .enumerate()
        .map(|(j, d)| {
            if d.per_layer {
                p.schedule.layers.iter().enumerate().filter(|(_, k)| readers[j].contains(k)).map(|(l, _)| Some(l as u16)).collect()
            } else {
                vec![None]
            }
        })
        .collect()
}

/// Bytes of one instance of param `j`.
pub fn palw_tir_tensor_bytes_v1(p: &TirProgramV1, j: u16) -> u64 {
    let d = &p.params[j as usize];
    d.shape.iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
}

/// Bytes of one row of param `j` (an axis-0 slice at rank ≥ 2, the whole tensor below).
fn row_bytes(p: &TirProgramV1, j: u16) -> u64 {
    let d = &p.params[j as usize];
    if d.shape.len() >= 2 {
        d.shape[1..].iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
    } else {
        palw_tir_tensor_bytes_v1(p, j)
    }
}

/// Leaves of one instance of param `j`.
fn leaves_per_instance(p: &TirProgramV1, j: u16) -> u64 {
    let (t, r) = (palw_tir_tensor_bytes_v1(p, j), row_bytes(p, j));
    if t == 0 || r == 0 {
        return 0;
    }
    (t / r) * r.div_ceil(PALW_TIR_ROW_PIECE_BYTES_V1)
}

/// The number of leaves, in closed form.
pub fn palw_tir_inventory_leaf_count_v1(p: &TirProgramV1) -> Result<u32, PalwTirInventoryError> {
    let instances = palw_tir_param_instances_v1(p);
    let mut total: u64 = 0;
    for (j, inst) in instances.iter().enumerate() {
        let bytes = palw_tir_tensor_bytes_v1(p, j as u16);
        if bytes > u32::MAX as u64 {
            return Err(PalwTirInventoryError::TensorTooLarge { name: p.params[j].name.clone(), bytes });
        }
        total += leaves_per_instance(p, j as u16) * inst.len() as u64;
    }
    if total == 0 {
        return Err(PalwTirInventoryError::Empty);
    }
    u32::try_from(total).map_err(|_| PalwTirInventoryError::TooManyLeaves(total))
}

/// **Where a byte of a tensor instance is committed**: the index of the leaf holding byte
/// `byte_offset` of `(param, layer)`, or `None` if that instance or byte does not exist. A closed
/// form of the declarations, so a court needs no artifact to name the leaf a cone operand lives in.
pub fn palw_tir_leaf_index_v1(p: &TirProgramV1, param: u16, layer: Option<u16>, byte_offset: u64) -> Option<u32> {
    let instances = palw_tir_param_instances_v1(p);
    let inst = instances.get(param as usize)?;
    let k = inst.iter().position(|l| *l == layer)? as u64;
    if byte_offset >= palw_tir_tensor_bytes_v1(p, param) {
        return None;
    }
    let before: u64 = (0..param as usize).map(|j| leaves_per_instance(p, j as u16) * instances[j].len() as u64).sum();
    let r = row_bytes(p, param);
    let pieces = r.div_ceil(PALW_TIR_ROW_PIECE_BYTES_V1);
    let (row, within) = (byte_offset / r, byte_offset % r);
    let index = before + k * leaves_per_instance(p, param) + row * pieces + within / PALW_TIR_ROW_PIECE_BYTES_V1;
    u32::try_from(index).ok()
}

/// Visit every leaf's coordinates in inventory order.
pub fn palw_tir_visit_inventory_rows_v1(
    p: &TirProgramV1,
    visit: &mut dyn FnMut(PalwTirInventoryRowV1),
) -> Result<(), PalwTirInventoryError> {
    palw_tir_inventory_leaf_count_v1(p)?;
    for (j, inst) in palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let j = j as u16;
        let (t, r) = (palw_tir_tensor_bytes_v1(p, j), row_bytes(p, j));
        for layer in inst {
            let mut row = 0u64;
            while row < t {
                let mut at = 0u64;
                while at < r {
                    let len = (r - at).min(PALW_TIR_ROW_PIECE_BYTES_V1);
                    visit(PalwTirInventoryRowV1 { param: j, layer, row_start: (row + at) as u32, len: len as u32 });
                    at += len;
                }
                row += r;
            }
        }
    }
    Ok(())
}

/// Supplies the bytes of each tensor instance (little-endian, the declared dtype's width).
pub trait PalwTirTensorSourceV1 {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>>;
}

/// The bytes of one instance, checked against its declaration.
fn instance<'s>(
    p: &TirProgramV1,
    src: &'s dyn PalwTirTensorSourceV1,
    j: u16,
    layer: Option<u16>,
) -> Result<Cow<'s, [u8]>, PalwTirInventoryError> {
    let name = || p.params[j as usize].name.clone();
    let bytes = src.tensor_bytes(j, layer).ok_or_else(|| PalwTirInventoryError::Missing { name: name(), layer })?;
    let want = palw_tir_tensor_bytes_v1(p, j);
    if bytes.len() as u64 != want {
        return Err(PalwTirInventoryError::Length { name: name(), layer, got: bytes.len() as u64, want });
    }
    Ok(bytes)
}

/// **The inventory root, streamed** — one tensor instance resident at a time, every leaf pushed
/// into the Merkle frontier as it is hashed (`palw_artifact`'s streamed root, pinned there against
/// the materialised one). Returns `(root, leaf_count)`.
pub fn palw_tir_inventory_root_v1(p: &TirProgramV1, src: &dyn PalwTirTensorSourceV1) -> Result<(Hash64, u32), PalwTirInventoryError> {
    let count = palw_tir_inventory_leaf_count_v1(p)?;
    let mut frontier = PalwArtifactMerkleFrontierV1::new();
    for (j, inst) in palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let j = j as u16;
        let name = &p.params[j as usize].name;
        let r = row_bytes(p, j) as usize;
        for layer in inst {
            let bytes = instance(p, src, j, layer)?;
            for (ri, row) in bytes.chunks(r).enumerate() {
                for (pi, piece) in row.chunks(PALW_TIR_ROW_PIECE_BYTES_V1 as usize).enumerate() {
                    let start = ri * r + pi * PALW_TIR_ROW_PIECE_BYTES_V1 as usize;
                    frontier.push(artifact_leaf_parts_v1(name, layer, start as u32, piece));
                }
            }
        }
    }
    debug_assert_eq!(frontier.leaf_count(), count as u64);
    Ok((frontier.root().ok_or(PalwTirInventoryError::Empty)?, count))
}

/// Every leaf's operand, materialised (small artifacts and tests; a real artifact streams).
pub fn palw_tir_inventory_operands_v1(
    p: &TirProgramV1,
    src: &dyn PalwTirTensorSourceV1,
) -> Result<Vec<PalwArtifactOperandV1>, PalwTirInventoryError> {
    let mut out = Vec::new();
    for (j, inst) in palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let j = j as u16;
        let r = row_bytes(p, j) as usize;
        for layer in inst {
            let bytes = instance(p, src, j, layer)?;
            for (ri, row) in bytes.chunks(r).enumerate() {
                for (pi, piece) in row.chunks(PALW_TIR_ROW_PIECE_BYTES_V1 as usize).enumerate() {
                    out.push(PalwArtifactOperandV1 {
                        tensor_name: p.params[j as usize].name.clone(),
                        layer,
                        row_start: (ri * r + pi * PALW_TIR_ROW_PIECE_BYTES_V1 as usize) as u32,
                        bytes: piece.to_vec(),
                    });
                }
            }
        }
    }
    Ok(out)
}

/// **An opening of one leaf, streamed**: the leaf's operand and its sibling path, built in one walk
/// that keeps the Merkle frontier's peaks and the path's partial folds — never the leaf vector. The
/// result verifies with `palw_artifact::verify_artifact_opening_v1` against the root.
pub fn palw_tir_open_leaf_v1(
    p: &TirProgramV1,
    src: &dyn PalwTirTensorSourceV1,
    index: u32,
) -> Result<PalwArtifactOpeningV1, PalwTirInventoryError> {
    let count = palw_tir_inventory_leaf_count_v1(p)?;
    if index >= count {
        return Err(PalwTirInventoryError::IndexOutOfRange { index, count });
    }
    // Pass 1: every leaf hash (64 bytes a leaf), and the opened operand.
    let mut leaves: Vec<Hash64> = Vec::with_capacity(count as usize);
    let mut operand = None;
    for (j, inst) in palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let j = j as u16;
        let name = &p.params[j as usize].name;
        let r = row_bytes(p, j) as usize;
        for layer in inst {
            let bytes = instance(p, src, j, layer)?;
            for (ri, row) in bytes.chunks(r).enumerate() {
                for (pi, piece) in row.chunks(PALW_TIR_ROW_PIECE_BYTES_V1 as usize).enumerate() {
                    let start = (ri * r + pi * PALW_TIR_ROW_PIECE_BYTES_V1 as usize) as u32;
                    if leaves.len() as u32 == index {
                        let op = PalwArtifactOperandV1 { tensor_name: name.clone(), layer, row_start: start, bytes: piece.to_vec() };
                        leaves.push(artifact_leaf_v1(&op));
                        operand = Some(op);
                    } else {
                        leaves.push(artifact_leaf_parts_v1(name, layer, start, piece));
                    }
                }
            }
        }
    }
    // Pass 2: the sibling path, promotion mirrored exactly as `open_artifact_leaf_v1` does.
    let mut level = leaves;
    let mut at = index as usize;
    let mut path = Vec::new();
    while level.len() > 1 {
        let promoted = at == level.len() - 1 && level.len() % 2 == 1;
        if !promoted {
            path.push(if at.is_multiple_of(2) { level[at + 1] } else { level[at - 1] });
        }
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        let mut i = 0;
        while i + 1 < level.len() {
            next.push(artifact_node_v1(&level[i], &level[i + 1]));
            i += 2;
        }
        if i < level.len() {
            next.push(level[i]);
        }
        level = next;
        at /= 2;
    }
    Ok(PalwArtifactOpeningV1 { operand: operand.expect("index < count"), leaf_index: index, leaf_count: count, path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_artifact::{artifact_root_v1, open_artifact_leaf_v1, verify_artifact_opening_v1};
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{DType, TensorType};
    use std::collections::BTreeMap;

    /// A two-layer program whose params cover every row rule: a matrix with rows past 32 KiB, a
    /// per-layer matrix, a per-layer vector, a scalar, and a param only the second layer kind reads.
    fn program() -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(40_000, HISTORY_BOUND_V1_SMALL);
        let table = pb.param("embed.table", DType::I8, &[40_000, 3], false);
        let big = pb.param("big.w", DType::I32, &[3, 20_000], false);
        let w = pb.param("blk.w", DType::I8, &[4, 3], true);
        let m = pb.param("blk.m", DType::I64, &[4], true);
        let only_b = pb.param("blk.b_only", DType::I16, &[3], true);
        let s = pb.param("s", DType::I64, &[], false);
        let carry = vec![TensorType::fixed(DType::I32, &[3])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let row = b.gather(table, Ref::Input(0), 0, 0);
            let row = b.cast(row, DType::I32);
            b.finish(&[row])
        };
        let mk = |pb: &mut ProgramBuilder, name: &str, extra: bool| {
            let mut b = pb.block(name, carry.clone());
            let x = b.reshape_fixed(Ref::CarryIn(0), &[3, 1]);
            let acc = b.matmul(w, x, DType::I64);
            let acc = b.reshape_fixed(acc, &[4]);
            let acc = b.mul(acc, m, DType::I128);
            let y = b.slice(acc, 0, 0, 3);
            let y = if extra { b.add(y, only_b, DType::I128) } else { y };
            let y = b.clamp(y, i32::MIN as i64, i32::MAX as i64, DType::I32);
            b.finish(&[y])
        };
        let la = mk(&mut pb, "a", false);
        let lb = mk(&mut pb, "b", true);
        let post = {
            let mut b = pb.block("post", carry.clone());
            let x = b.reshape_fixed(Ref::CarryIn(0), &[3, 1]);
            let bg = b.slice(big, 1, 0, 3);
            let l = b.matmul(bg, x, DType::I64);
            let l = b.mul(l, s, DType::I128);
            let l = b.reshape_fixed(l, &[3]);
            let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
            b.commit(l);
            b.finish(&[])
        };
        let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
        pb.finish(pre, vec![la, lb, la], post, logits)
    }

    struct Src(BTreeMap<(u16, Option<u16>), Vec<u8>>);
    impl PalwTirTensorSourceV1 for Src {
        fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
            self.0.get(&(param, layer)).map(|v| Cow::Borrowed(v.as_slice()))
        }
    }

    fn source(p: &TirProgramV1) -> Src {
        let mut out = BTreeMap::new();
        for (j, inst) in palw_tir_param_instances_v1(p).into_iter().enumerate() {
            for l in inst {
                let n = palw_tir_tensor_bytes_v1(p, j as u16) as usize;
                let seed = (j as u64 + 1) * 1_000_003 + l.map_or(0, |l| l as u64 + 7);
                out.insert((j as u16, l), (0..n).map(|i| ((seed.wrapping_mul(i as u64 + 13)) >> 3) as u8).collect());
            }
        }
        Src(out)
    }

    #[test]
    fn instances_follow_the_schedule() {
        let p = program();
        misaka_palw_tir::validate::validate(&p).expect("valid");
        let inst = palw_tir_param_instances_v1(&p);
        assert_eq!(inst[0], vec![None]);
        assert_eq!(inst[2], vec![Some(0), Some(1), Some(2)]);
        // Read only by block `b`, which runs at layer 1.
        assert_eq!(inst[4], vec![Some(1)]);
    }

    #[test]
    fn rows_pieces_and_the_closed_forms_agree() {
        let p = program();
        let mut rows = Vec::new();
        palw_tir_visit_inventory_rows_v1(&p, &mut |r| rows.push(r)).expect("rows");
        assert_eq!(rows.len() as u32, palw_tir_inventory_leaf_count_v1(&p).expect("count"));
        // big.w rows are 80,000 bytes: three pieces, 32,768, 32,768 then 14,464.
        let big: Vec<_> = rows.iter().filter(|r| r.param == 1).collect();
        assert_eq!(big.len(), 9);
        assert_eq!(
            [(big[0].row_start, big[0].len), (big[1].row_start, big[1].len), (big[2].row_start, big[2].len)],
            [(0, 32_768), (32_768, 32_768), (65_536, 14_464)]
        );
        assert_eq!(big[3].row_start, 80_000, "the second row starts where the first ends");
        // Vectors and scalars are one leaf per instance.
        assert_eq!(rows.iter().filter(|r| r.param == 3).count(), 3);
        assert_eq!(rows.iter().filter(|r| r.param == 5).count(), 1);
        // Every leaf's closed-form index is its position, at its first and its last byte.
        for (i, r) in rows.iter().enumerate() {
            assert_eq!(palw_tir_leaf_index_v1(&p, r.param, r.layer, r.row_start as u64), Some(i as u32), "{r:?}");
            assert_eq!(palw_tir_leaf_index_v1(&p, r.param, r.layer, (r.row_start + r.len - 1) as u64), Some(i as u32));
        }
        assert_eq!(palw_tir_leaf_index_v1(&p, 4, Some(0), 0), None, "no such instance");
        assert_eq!(palw_tir_leaf_index_v1(&p, 5, None, 8), None, "past the tensor");
    }

    #[test]
    fn the_streamed_root_is_the_materialised_root_and_openings_verify() {
        let p = program();
        let src = source(&p);
        let (root, count) = palw_tir_inventory_root_v1(&p, &src).expect("root");
        let ops = palw_tir_inventory_operands_v1(&p, &src).expect("operands");
        assert_eq!(ops.len() as u32, count);
        let leaves: Vec<Hash64> = ops.iter().map(artifact_leaf_v1).collect();
        assert_eq!(artifact_root_v1(&leaves), Some(root));
        for index in [0, 1, count / 2, count - 2, count - 1] {
            let o = palw_tir_open_leaf_v1(&p, &src, index).expect("opening");
            assert_eq!(o, open_artifact_leaf_v1(&ops, index).expect("reference opening"));
            verify_artifact_opening_v1(&o, root).expect("verifies");
        }
        // A byte changed anywhere moves the root.
        let mut bad = source(&p);
        bad.0.get_mut(&(3, Some(2))).expect("blk.m@2")[5] ^= 1;
        assert_ne!(palw_tir_inventory_root_v1(&p, &bad).expect("root").0, root);
    }

    #[test]
    fn a_tensor_of_the_wrong_length_or_a_missing_one_is_refused() {
        let p = program();
        let mut src = source(&p);
        src.0.get_mut(&(3, Some(1))).expect("blk.m@1").pop();
        assert!(matches!(palw_tir_inventory_root_v1(&p, &src), Err(PalwTirInventoryError::Length { .. })));
        let mut src = source(&p);
        src.0.remove(&(4, Some(1)));
        assert!(matches!(palw_tir_inventory_root_v1(&p, &src), Err(PalwTirInventoryError::Missing { .. })));
    }
}
