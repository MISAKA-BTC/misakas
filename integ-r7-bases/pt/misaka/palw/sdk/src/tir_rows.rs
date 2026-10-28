//! **A PALWTIR1 container as a row source for the tiled independent evaluator** (RFC-0013 §5).
//!
//! `misaka_palw_tir_ref2::tiled` evaluates `MatMul` and `Gather` over a param in row tiles; it asks a [`RowSource`] for rows `[r0, r0 + n)` of
//! axis 0 and never for the whole tensor. This is that source over a container on disk:
//!
//! * **the bytes of the rows only** are read (`pread` of `rows × row_bytes` bytes at the instance's offset), decoded by ref2's own
//!   `Tensor::from_le_bytes` (the independent implementation keeps its own decoding; only the I/O is shared);
//! * **authenticated when an index is given.** With a [`PalwTirMerkleIndexV1`] every leaf that covers the rows read is hashed and compared with
//!   the stored leaf before the rows are decoded ([`PalwTirMerkleIndexV1::read_authenticated`]), so a tile of a tensor larger than memory is
//!   checked against the artifact root without reading the rest of the artifact. Without one the bytes are the file's, as the lazy whole-tensor
//!   source has always read them. The caller is responsible for having shown the index folds to the root it holds
//!   ([`PalwTirMerkleIndexV1::verify_root`]);
//! * **the declaration is ref2's**: dtype and shape come from the independently decoded program, the container supplies only where the bytes
//!   are; an instance the container does not hold is `None` (the evaluator reports it missing, as for the whole-tensor source).
//!
//! The counters ([`RowReadStatsV1`]) are what a run reports: bytes read, and — when authenticated — the leaves and bytes hashed.

use crate::tir_merkle_index::PalwTirMerkleIndexV1;
use crate::tir_stream::{ContainerRanges, PalwTirRangeSourceV1};
use misaka_palw_tir_artifact::PalwTirContainerV1;
use misaka_palw_tir_ref2::error::{Class, Res, TirError};
use misaka_palw_tir_ref2::program::Program;
use misaka_palw_tir_ref2::tensor::count;
use misaka_palw_tir_ref2::tiled::RowSource;
use misaka_palw_tir_ref2::{DType, Tensor};
use std::cell::Cell;

/// The read-ahead of the row source: a few leaves. Row tiles are read where they are; a 4 MiB window per scattered `Gather` row would read
/// thousands of times what it uses.
const ROW_WINDOW_BYTES: u64 = 256 << 10;

/// What a [`ContainerRowSource`] has done so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowReadStatsV1 {
    /// `rows` calls served.
    pub reads: u64,
    /// Bytes of tensor rows returned.
    pub bytes_read: u64,
    /// Leaves hashed and compared with the stored index (0 without one).
    pub leaves_authenticated: u64,
    /// Bytes those leaves held.
    pub bytes_hashed: u64,
}

/// The container's rows, for [`misaka_palw_tir_ref2::tiled::TiledParams`].
pub struct ContainerRowSource<'a> {
    container: &'a PalwTirContainerV1,
    program: &'a Program,
    ranges: ContainerRanges<'a>,
    index: Option<&'a PalwTirMerkleIndexV1>,
    stats: Cell<RowReadStatsV1>,
}

impl<'a> ContainerRowSource<'a> {
    /// `program` is the independently decoded program of the same container (its declarations are used; the container's bytes locate the
    /// data); `index`, when given, authenticates every read.
    pub fn new(
        container: &'a PalwTirContainerV1,
        program: &'a Program,
        index: Option<&'a PalwTirMerkleIndexV1>,
    ) -> Result<Self, String> {
        Ok(Self {
            container,
            program,
            ranges: ContainerRanges::open_with_window(container, ROW_WINDOW_BYTES)?,
            index,
            stats: Cell::new(RowReadStatsV1::default()),
        })
    }

    pub fn stats(&self) -> RowReadStatsV1 {
        self.stats.get()
    }

    fn layer_of(layer: Option<u32>) -> Res<Option<u16>> {
        layer.map(u16::try_from).transpose().map_err(|e| TirError::new(Class::Index, e.to_string()))
    }
}

impl RowSource for ContainerRowSource<'_> {
    fn shape(&self, index: u16, layer: Option<u32>) -> Res<Option<(DType, Vec<u64>)>> {
        let Some(d) = self.program.params.get(index as usize) else { return Ok(None) };
        let layer = Self::layer_of(layer)?;
        Ok(self.container.locate(index, layer).map(|_| (d.dtype, d.shape.iter().map(|&x| x as u64).collect())))
    }

    fn rows(&self, index: u16, layer: Option<u32>, row_start: u64, rows: u64) -> Res<Tensor> {
        let missing = |why: String| TirError::new(Class::Missing, format!("param {index}: {why}"));
        let operand = |why: String| TirError::new(Class::Operand, format!("param {index}: {why}"));
        let d = self.program.params.get(index as usize).ok_or_else(|| missing("no such param".into()))?;
        let layer = Self::layer_of(layer)?;
        let (_, total) = self.container.locate(index, layer).ok_or_else(|| missing(format!("no instance at layer {layer:?}")))?;
        let shape: Vec<u64> = d.shape.iter().map(|&x| x as u64).collect();
        let width = d.dtype.width() as u64;
        // A rank-0 param has one "row": the scalar. Otherwise the rows are axis 0's, each `rest` elements.
        let (row_bytes, in_shape) = match shape.split_first() {
            None => {
                if (row_start, rows) != (0, 1) {
                    return Err(operand("a scalar param has one row".into()));
                }
                (width, Vec::new())
            }
            Some((&n0, rest)) => {
                if row_start.checked_add(rows).is_none_or(|end| end > n0) {
                    return Err(operand(format!("rows {row_start}+{rows} outside a param of {n0} rows")));
                }
                let mut s = vec![rows];
                s.extend_from_slice(rest);
                (count(rest) * width, s)
            }
        };
        let (lo, hi) = (row_start * row_bytes, (row_start + rows) * row_bytes);
        if hi > total {
            return Err(operand(format!("bytes {lo}..{hi} of an instance of {total}")));
        }
        let mut bytes = vec![0u8; (hi - lo) as usize];
        let mut stats = self.stats.get();
        match self.index {
            Some(index_file) => {
                let r = index_file
                    .read_authenticated(&self.container.program, &self.ranges, index, layer, lo..hi, &mut bytes)
                    .map_err(|e| operand(e.to_string()))?;
                stats.leaves_authenticated += r.leaves as u64;
                stats.bytes_hashed += r.bytes_hashed;
            }
            None => self.ranges.read_range(index, layer, lo..hi, &mut bytes).map_err(missing)?,
        }
        stats.reads += 1;
        stats.bytes_read += bytes.len() as u64;
        self.stats.set(stats);
        Tensor::from_le_bytes(d.dtype, in_shape, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::derived_a16_store;
    use misaka_palw_base0::tir_a16::convert_a16_to_tir;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;

    fn converted(tag: &str) -> std::path::PathBuf {
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 48,
            vocab: 100,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let a = Base0ArtifactV1::derive_deterministic(shape, 0x3F3)
            .expect("shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("store");
        let path = std::env::temp_dir().join(format!("tir-rows-{tag}-{}.palwtir", std::process::id()));
        convert_a16_to_tir(&a, HISTORY_BOUND_V1_SMALL, &path, "{}".into()).expect("converted");
        path
    }

    fn ref2_program(c: &PalwTirContainerV1) -> Program {
        misaka_palw_tir_ref2::codec::decode_canonical(&c.program.encode()).expect("ref2 decodes the program")
    }

    /// Rows read through the source are the rows of the whole tensor, with and without the index; the index hashes exactly the leaves under
    /// the rows asked for; a damaged byte is refused as the leaf it is in; an instance that is not there is `None`.
    #[test]
    fn rows_are_the_whole_tensors_rows_and_the_index_authenticates_only_what_was_read() {
        let path = converted("rows");
        let c = PalwTirContainerV1::open(&path).expect("opens");
        let p2 = ref2_program(&c);
        let index = PalwTirMerkleIndexV1::build_streamed(&c.program, &ContainerRanges::open(&c).unwrap()).expect("index");
        let plain = ContainerRowSource::new(&c, &p2, None).expect("source");
        let checked = ContainerRowSource::new(&c, &p2, Some(&index)).expect("source");
        let mut multi_row = 0;
        for e in &c.header.tensors {
            let d = &p2.params[e.param as usize];
            let layer = e.layer.map(u32::from);
            let whole = c.read_tensor_bytes(e.param, e.layer).unwrap();
            let all = Tensor::from_le_bytes(d.dtype, d.shape.iter().map(|&x| x as u64).collect(), &whole).unwrap();
            let (dtype, shape) = plain.shape(e.param, layer).unwrap().expect("held");
            assert_eq!((dtype, &shape), (d.dtype, &d.shape.iter().map(|&x| x as u64).collect::<Vec<_>>()));
            let n0 = shape.first().copied().unwrap_or(1);
            let rest = if shape.len() <= 1 { 1 } else { count(&shape[1..]) as usize };
            for (r0, n) in [(0, 1), (n0 - 1, 1), (n0 / 2, 1.max((n0 / 3).min(n0 - n0 / 2))), (0, n0)] {
                let (a, b) = if shape.is_empty() { (0, 1) } else { (r0, n) };
                let want_data = &all.data[a as usize * rest..(a + b) as usize * rest];
                for src in [&plain, &checked] {
                    let t = src.rows(e.param, layer, a, b).expect("rows");
                    assert_eq!(t.data, want_data, "param {} layer {layer:?} rows {a}+{b}", e.param);
                    assert_eq!(t.dtype, d.dtype);
                    if !shape.is_empty() {
                        assert_eq!(t.shape[0], b);
                    }
                }
                if b > 1 {
                    multi_row += 1;
                }
            }
        }
        assert!(multi_row > 5);
        let (s_plain, s_checked) = (plain.stats(), checked.stats());
        assert_eq!((s_plain.reads, s_plain.bytes_read), (s_checked.reads, s_checked.bytes_read), "the same rows were read");
        assert_eq!(s_plain.leaves_authenticated, 0);
        assert!(s_checked.leaves_authenticated > 0 && s_checked.bytes_hashed >= s_checked.bytes_read, "{s_checked:?}");
        // One row of one big tensor hashes one leaf's worth, not the tensor.
        let big = c.header.tensors.iter().max_by_key(|e| e.bytes).unwrap();
        let before = checked.stats();
        checked.rows(big.param, big.layer.map(u32::from), 3, 1).unwrap();
        let after = checked.stats();
        assert_eq!(after.leaves_authenticated - before.leaves_authenticated, 1);
        assert!(after.bytes_hashed - before.bytes_hashed <= big.bytes / 4, "a single row is a fraction of the tensor");
        // Requests outside the tensor and outside the container.
        let layer = big.layer.map(u32::from);
        let n0 = p2.params[big.param as usize].shape[0] as u64;
        assert_eq!(checked.rows(big.param, layer, n0, 1).unwrap_err().class, Class::Operand);
        assert_eq!(checked.rows(big.param, layer, u64::MAX, 2).unwrap_err().class, Class::Operand);
        assert_eq!(plain.shape(9_999, None).unwrap(), None);
        assert_eq!(plain.shape(big.param, Some(60_000)).unwrap(), None);

        // A damaged byte: refused with the plain source never noticing, by the authenticated source as the leaf it is in.
        let damaged = std::env::temp_dir().join(format!("tir-rows-damaged-{}.palwtir", std::process::id()));
        let mut bytes = std::fs::read(&path).unwrap();
        let (off, len) = c.locate(big.param, big.layer).unwrap();
        let row_bytes = len / n0;
        bytes[(off + 3 * row_bytes) as usize] ^= 0x10;
        std::fs::write(&damaged, &bytes).unwrap();
        let dc = PalwTirContainerV1::open(&damaged).unwrap();
        let (dplain, dchecked) =
            (ContainerRowSource::new(&dc, &p2, None).unwrap(), ContainerRowSource::new(&dc, &p2, Some(&index)).unwrap());
        assert!(dplain.rows(big.param, layer, 3, 1).is_ok(), "an unauthenticated read cannot tell");
        let e = dchecked.rows(big.param, layer, 3, 1).unwrap_err();
        assert!(e.reason.contains("does not hash to the stored leaf"), "{e:?}");
        assert!(dchecked.rows(big.param, layer, 0, 3).is_ok(), "the rows before the damaged one still verify");
        let _ = std::fs::remove_file(&damaged);
        let _ = std::fs::remove_file(&path);
    }
}
