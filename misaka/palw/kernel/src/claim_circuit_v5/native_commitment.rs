//! Existing native tensor commitments lowered into the actual hash circuit.
//! Both dual roots bind the same input wires. Tiled/hiding modes use the exact
//! existing leaf ordering and ragged-tail rules, not a new root convention.
use super::blake2b::{DigestWiresV5, compression_blocks_v5, graph_cost_v5, keyed_512_v5};
use super::*;
use misaka_palw_tir::DType;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeTensorCommitmentV5 {
    DualRootV2,
    TiledDualRootV3,
    HidingFlatV1,
}
fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or("native hash circuit count overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or("native hash circuit product overflow".into())
}
fn constant_byte(v: u8) -> ByteV5 {
    std::array::from_fn(|i| u32::from((v >> i) & 1 != 0))
}
struct Layout {
    elements: u64,
    rows: u64,
    row_len: u64,
    cols: u64,
    col_len: u64,
    m: u64,
    n: u64,
    flat: bool,
}
fn layout(shape: &[usize]) -> Result<Layout, String> {
    if shape.len() > u8::MAX as usize {
        return Err("native tensor rank is not representable".into());
    }
    let elements = if shape.contains(&0) {
        0
    } else {
        shape.iter().try_fold(1u64, |n, d| n.checked_mul(u64::try_from(*d).ok()?)).ok_or("native tensor shape product overflow")?
    };
    if shape.len() <= 1 {
        return Ok(Layout { elements, rows: 1, row_len: elements, cols: 1, col_len: elements, m: 1, n: elements, flat: true });
    }
    let m = shape[shape.len() - 2] as u64;
    let n = shape[shape.len() - 1] as u64;
    let batch = if elements == 0 { 0 } else { elements / (m * n) };
    Ok(Layout {
        elements,
        rows: if n == 0 { 0 } else { elements / n },
        row_len: n,
        cols: mul(batch, n)?,
        col_len: m,
        m,
        n,
        flat: false,
    })
}
fn domains(suite: NativeTensorCommitmentV5) -> (&'static [u8], &'static [u8], &'static [u8]) {
    match suite {
        NativeTensorCommitmentV5::DualRootV2 => {
            (crate::merkle::TENSOR_LEAF_DOMAIN_V2, crate::merkle::TENSOR_NODE_DOMAIN_V2, crate::merkle::TENSOR_COMMITMENT_DOMAIN_V2)
        }
        NativeTensorCommitmentV5::TiledDualRootV3 => {
            (crate::merkle3::TENSOR_LEAF_DOMAIN_V3, crate::merkle3::TENSOR_NODE_DOMAIN_V3, crate::merkle3::TENSOR_COMMITMENT_DOMAIN_V3)
        }
        NativeTensorCommitmentV5::HidingFlatV1 => (
            crate::hide::TENSOR_HIDE_LEAF_DOMAIN_V1,
            crate::hide::TENSOR_HIDE_NODE_DOMAIN_V1,
            crate::hide::TENSOR_HIDE_COMMITMENT_DOMAIN_V1,
        ),
    }
}
fn tiled_tree_blocks(lines: u64, line_len: u64, tile: u64, prefix: u64, width: u64, leaf_domain: &[u8]) -> Result<u64, String> {
    let tiles = line_len.div_ceil(tile);
    let leaves = mul(lines, tiles)?;
    if leaves == 0 {
        return Ok(1);
    }
    let full = compression_blocks_v5(add(prefix, mul(tile, width)?)?, leaf_domain.len())?;
    let tail = line_len - mul(tiles - 1, tile)?;
    let last = compression_blocks_v5(add(prefix, mul(tail, width)?)?, leaf_domain.len())?;
    add(mul(lines, add(mul(tiles - 1, full)?, last)?)?, mul(leaves - 1, 2)?)
}
/// Arithmetic preflight from shape, dtype and the exact native layout. No
/// tensor-sized vector, circuit gate or witness is allocated while counting.
pub fn native_tensor_cost_v5(suite: NativeTensorCommitmentV5, dtype: DType, shape: &[usize]) -> Result<CircuitCostV5, String> {
    let l = layout(shape)?;
    let width = dtype.width() as u64;
    let (leaf, node, commit) = domains(suite);
    let _ = node;
    let tree = match suite {
        NativeTensorCommitmentV5::DualRootV2 => {
            let one = |leaves: u64, length: u64| -> Result<u64, String> {
                if leaves == 0 {
                    Ok(1)
                } else {
                    add(mul(leaves, compression_blocks_v5(add(18, mul(length, width)?)?, leaf.len())?)?, mul(leaves - 1, 2)?)
                }
            };
            add(one(l.rows, l.row_len)?, one(l.cols, l.col_len)?)?
        }
        NativeTensorCommitmentV5::TiledDualRootV3 => add(
            tiled_tree_blocks(l.rows, l.row_len, crate::merkle3::TILE_V3, 26, width, leaf)?,
            tiled_tree_blocks(l.cols, l.col_len, crate::merkle3::TILE_V3, 26, width, leaf)?,
        )?,
        NativeTensorCommitmentV5::HidingFlatV1 => {
            let tiles = crate::hide::hide_tiles_v1(l.elements);
            if tiles == 0 {
                1
            } else {
                let tail = l.elements - mul(tiles - 1, crate::hide::HIDE_TILE_V1)?;
                let full = compression_blocks_v5(add(17, mul(crate::hide::HIDE_TILE_V1, width)?)?, leaf.len())?;
                let last = compression_blocks_v5(add(17, mul(tail, width)?)?, leaf.len())?;
                add(add(mul(tiles - 1, full)?, last)?, mul(tiles - 1, 2)?)?
            }
        }
    };
    let roots = if suite == NativeTensorCommitmentV5::HidingFlatV1 { 64 } else { 128 };
    let commit_bytes = add(add(2, mul(shape.len() as u64, 8)?)?, roots)?;
    let blocks = add(tree, compression_blocks_v5(commit_bytes, commit.len())?)?;
    graph_cost_v5(mul(mul(l.elements, width)?, 8)?, blocks, 512)
}
struct Leaf<'a> {
    data: &'a dyn CircuitByteStreamV5,
    layout: &'a Layout,
    suite: NativeTensorCommitmentV5,
    dtype: DType,
    axis: u8,
    line: u64,
    tile: u64,
    first: u64,
    count: u64,
}
impl Leaf<'_> {
    fn prefix(&self) -> usize {
        match self.suite {
            NativeTensorCommitmentV5::DualRootV2 => 18,
            NativeTensorCommitmentV5::TiledDualRootV3 => 26,
            NativeTensorCommitmentV5::HidingFlatV1 => 17,
        }
    }
    fn preimage_header(&self) -> [u8; 26] {
        let mut p = [0; 26];
        p[0] = self.dtype.tag();
        match self.suite {
            NativeTensorCommitmentV5::DualRootV2 => {
                p[1] = self.axis;
                p[2..10].copy_from_slice(&self.line.to_le_bytes());
                p[10..18].copy_from_slice(&self.count.to_le_bytes());
            }
            NativeTensorCommitmentV5::TiledDualRootV3 => {
                p[1] = self.axis;
                p[2..10].copy_from_slice(&self.line.to_le_bytes());
                p[10..18].copy_from_slice(&self.tile.to_le_bytes());
                p[18..26].copy_from_slice(&self.count.to_le_bytes());
            }
            NativeTensorCommitmentV5::HidingFlatV1 => {
                p[1..9].copy_from_slice(&self.tile.to_le_bytes());
                p[9..17].copy_from_slice(&self.count.to_le_bytes());
            }
        }
        p
    }
}
impl CircuitByteStreamV5 for Leaf<'_> {
    fn len(&self) -> usize {
        self.prefix() + self.count as usize * self.dtype.width()
    }
    fn byte(&self, index: usize) -> Result<ByteV5, String> {
        if index < self.prefix() {
            return Ok(constant_byte(self.preimage_header()[index]));
        }
        if index >= self.len() {
            return Err("native tensor leaf byte index".into());
        }
        let offset = index - self.prefix();
        let relative = offset / self.dtype.width();
        let element = match self.suite {
            NativeTensorCommitmentV5::HidingFlatV1 => add(self.first, relative as u64)?,
            _ if self.axis == crate::merkle::AXIS_ROW || self.layout.flat => add(
                mul(self.line, if self.axis == crate::merkle::AXIS_ROW { self.layout.row_len } else { self.layout.col_len })?,
                add(self.first, relative as u64)?,
            )?,
            _ => {
                let batch = self.line / self.layout.n;
                let column = self.line % self.layout.n;
                add(
                    add(mul(mul(batch, self.layout.m)?, self.layout.n)?, mul(add(self.first, relative as u64)?, self.layout.n)?)?,
                    column,
                )?
            }
        };
        let at = add(mul(element, self.dtype.width() as u64)?, (offset % self.dtype.width()) as u64)?;
        self.data.byte(usize::try_from(at).map_err(|_| "native leaf offset not addressable")?)
    }
}
struct Pair<'a>(&'a DigestWiresV5, &'a DigestWiresV5);
impl CircuitByteStreamV5 for Pair<'_> {
    fn len(&self) -> usize {
        128
    }
    fn byte(&self, i: usize) -> Result<ByteV5, String> {
        if i < 64 {
            Ok(self.0[i])
        } else if i < 128 {
            Ok(self.1[i - 64])
        } else {
            Err("native Merkle node index".into())
        }
    }
}
struct MerkleStack {
    levels: [Option<DigestWiresV5>; 64],
    leaves: u64,
}
impl MerkleStack {
    fn new() -> Self {
        Self { levels: [None; 64], leaves: 0 }
    }
    fn push(&mut self, b: &mut CircuitBuilderV5, domain: &[u8], mut value: DigestWiresV5) -> Result<(), String> {
        self.leaves = self.leaves.checked_add(1).ok_or("native Merkle leaf count overflow")?;
        for level in &mut self.levels {
            if let Some(left) = level.take() {
                value = keyed_512_v5(b, domain, &Pair(&left, &value))?;
            } else {
                *level = Some(value);
                return Ok(());
            }
        }
        Err("native Merkle stack depth exceeded".into())
    }
    fn finish(&mut self, b: &mut CircuitBuilderV5, domain: &[u8]) -> Result<DigestWiresV5, String> {
        let mut result = None;
        for left in self.levels.iter_mut().filter_map(Option::take) {
            result = Some(if let Some(right) = result { keyed_512_v5(b, domain, &Pair(&left, &right))? } else { left });
        }
        match result {
            Some(root) => Ok(root),
            None => keyed_512_v5(b, domain, &ConstantByteStreamV5(&[])),
        }
    }
}
struct RootPreimage<'a> {
    dtype: DType,
    shape: &'a [usize],
    row: &'a DigestWiresV5,
    col: Option<&'a DigestWiresV5>,
}
impl CircuitByteStreamV5 for RootPreimage<'_> {
    fn len(&self) -> usize {
        2 + self.shape.len() * 8 + if self.col.is_some() { 128 } else { 64 }
    }
    fn byte(&self, index: usize) -> Result<ByteV5, String> {
        if index == 0 {
            return Ok(constant_byte(self.dtype.tag()));
        }
        if index == 1 {
            return Ok(constant_byte(self.shape.len() as u8));
        }
        let dims = self.shape.len() * 8;
        if index < 2 + dims {
            let offset = index - 2;
            return Ok(constant_byte((self.shape[offset / 8] as u64).to_le_bytes()[offset % 8]));
        }
        let i = index - 2 - dims;
        if i < 64 {
            Ok(self.row[i])
        } else if i < 128 {
            self.col.map(|c| c[i - 64]).ok_or("native single-root preimage overflow".into())
        } else {
            Err("native tensor root preimage index".into())
        }
    }
}
pub fn native_tensor_root_v5(
    b: &mut CircuitBuilderV5,
    suite: NativeTensorCommitmentV5,
    dtype: DType,
    shape: &[usize],
    data: &dyn CircuitByteStreamV5,
) -> Result<DigestWiresV5, String> {
    let l = layout(shape)?;
    let bytes = mul(l.elements, dtype.width() as u64)?;
    if u64::try_from(data.len()).ok() != Some(bytes) {
        return Err("native tensor circuit data length differs from shape/dtype".into());
    }
    let (leaf_domain, node_domain, commit_domain) = domains(suite);
    let mut root = |axis: u8| -> Result<DigestWiresV5, String> {
        let mut stack = MerkleStack::new();
        if suite == NativeTensorCommitmentV5::HidingFlatV1 {
            for tile in 0..crate::hide::hide_tiles_v1(l.elements) {
                let (first, end) = crate::hide::hide_tile_range_v1(l.elements, tile).ok_or("native hiding tile range")?;
                let leaf = Leaf { data, layout: &l, suite, dtype, axis: 0, line: 0, tile, first, count: end - first };
                let hash = keyed_512_v5(b, leaf_domain, &leaf)?;
                stack.push(b, node_domain, hash)?;
            }
        } else {
            let (lines, length) = if axis == crate::merkle::AXIS_ROW { (l.rows, l.row_len) } else { (l.cols, l.col_len) };
            let tile_width = if suite == NativeTensorCommitmentV5::TiledDualRootV3 { crate::merkle3::TILE_V3 } else { length.max(1) };
            let tiles = if suite == NativeTensorCommitmentV5::DualRootV2 { 1 } else { length.div_ceil(tile_width) };
            for line in 0..lines {
                for tile in 0..tiles {
                    let first = mul(tile, tile_width)?;
                    let count = length.saturating_sub(first).min(tile_width);
                    let leaf = Leaf { data, layout: &l, suite, dtype, axis, line, tile, first, count };
                    let hash = keyed_512_v5(b, leaf_domain, &leaf)?;
                    stack.push(b, node_domain, hash)?;
                }
            }
        }
        stack.finish(b, node_domain)
    };
    let row = root(crate::merkle::AXIS_ROW)?;
    let col = if suite == NativeTensorCommitmentV5::HidingFlatV1 { None } else { Some(root(crate::merkle::AXIS_COL)?) };
    keyed_512_v5(b, commit_domain, &RootPreimage { dtype, shape, row: &row, col: col.as_ref() })
}
/// Maximum simultaneously live typed Merkle accumulator. The hash compressor
/// has fixed-size stack arrays and does not allocate a heap per block.
pub fn native_tensor_workspace_bytes_v5() -> u64 {
    std::mem::size_of::<MerkleStack>() as u64
}
/// Endpoint compilation reserves the full typed graph before emission. The
/// stack frame and witness have independent retained allocation authorities.
pub fn compile_native_tensor_v5(
    suite: NativeTensorCommitmentV5,
    dtype: DType,
    shape: &[usize],
    expected: Digest,
    limits: CircuitLimitsV5,
    memory: CircuitMemoryBudgetV5,
) -> Result<ReservedCircuitV5, String> {
    let plan = native_tensor_cost_v5(suite, dtype, shape)?;
    let scratch = native_tensor_workspace_bytes_v5();
    let _scratch = memory.reserve(scratch)?;
    let mut builder = CircuitBuilderV5::new_planned(limits, plan, memory)?;
    let bytes = usize::try_from(plan.input_bits / 8).map_err(|_| "native circuit input bytes not addressable")?;
    let input = builder.input_byte_span(bytes)?;
    let root = native_tensor_root_v5(&mut builder, suite, dtype, shape, &input)?;
    builder.assert_bytes(&root, &expected)?;
    builder.finish_reserved()
}
