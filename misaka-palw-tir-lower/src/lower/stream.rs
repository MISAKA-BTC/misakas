//! **Streaming materialisation** (RFC-0002 Part II, the streaming loader): the artifact of a
//! lowered program produced one tensor — and, for the big ones, one block of rows — at a time,
//! each handed to a [`TensorSink`] and dropped. [`crate::lower::materialise`] holds every integer
//! param of the artifact until the last is made; this holds one block.
//!
//! Three things bound what is resident:
//!
//! * **The occurrence.** As before, only the block occurrence being filled has its float params
//!   loaded (`pre`, layer 0, …, `post`).
//! * **The row block.** A param that is the row-wise codes of one checkpoint tensor
//!   ([`Lowered::row_params`]: the W8 codes of a projection or of an expert stack, the `i16` codes
//!   of an embedding table) and is at least [`StreamOpts::defer_min_elems`] elements is *deferred*:
//!   it is not loaded with the occurrence. Its codes are made [`StreamOpts::block_elems`] elements
//!   at a time from `rows a..b` of the checkpoint ([`RowSource`]) by the very fill closure the
//!   whole-tensor path runs — handed a context whose param is just the block — and its per-row
//!   scales (which the narrowing params need) by a pass over the blocks that keeps no codes. Codes
//!   are per row, so the block-wise tensor is the whole one, byte for byte.
//! * **The chunk.** The sink cuts what it receives into canonical chunks as it arrives
//!   (`misaka_palw_tir_artifact::chunks`), so what it holds is a chunk.
//!
//! A deferred param that some fill reads WHOLE (a split-outlier projection, a transposed copy, a
//! quantised tensor) has no row-wise form: the fill fails with [`DEFERRED_MARK`], the occurrence is
//! loaded with that param resident and run again, and the report counts it ([`StreamStats::whole`]).
//! That is slower, never wrong. The result does not depend on any of the options: the same tensors
//! come out of every block size, chunk size, thread count and deferral threshold
//! (`tests/streaming_convert.rs`).

use super::fill::{DEFERRED_MARK, FillCtx, IntTensor, deferred_param_of, params_of_blocks, residual_scale};
use super::{Lowered, RowParam, occurrences};
use crate::error::{LowerError, Result};
use crate::float_ref::stream::OccParams;
use crate::float_ref::{ParamStore, SiteStat};
use crate::hl::HlProgram;
use crate::quant::QuantPolicy;
use crate::weights::stream::row_blocks;
use misaka_palw_tir_artifact::chunks::{ChunkStore, ChunkedArtifactV1, InstanceWriter};
use std::collections::{BTreeMap, BTreeSet};

/// How a streaming conversion cuts the work. None of it changes a byte of the result.
#[derive(Clone, Debug)]
pub struct StreamOpts {
    /// A row-wise param of at least this many elements is produced by row blocks (`0`: every
    /// row-wise param; `usize::MAX`: none).
    pub defer_min_elems: usize,
    /// Elements per block of rows (the `f32` block is four bytes each; its codes one or two more).
    pub block_elems: usize,
}

impl Default for StreamOpts {
    fn default() -> Self {
        Self { defer_min_elems: 4 << 20, block_elems: 4 << 20 }
    }
}

/// Where a streaming conversion puts the tensors it makes: one instance at a time, in pieces.
pub trait TensorSink {
    /// Start tensor instance `(param, layer)`, `bytes` long when complete. Anything left of an
    /// instance that was not ended is dropped.
    fn begin(&mut self, param: u16, layer: Option<u16>, bytes: u64) -> Result<()>;
    /// Append bytes to the instance in progress.
    fn push(&mut self, bytes: &[u8]) -> Result<()>;
    /// The instance is complete.
    fn end(&mut self) -> Result<()>;
}

/// A [`TensorSink`] into a content-addressed chunk store: every instance becomes canonical chunks
/// as it arrives, and [`ChunkSink::artifact`] is the recipe a container is assembled from.
pub struct ChunkSink<'s> {
    store: &'s ChunkStore,
    pub artifact: ChunkedArtifactV1,
    cur: Option<(InstanceWriter<'s>, u64, u64)>,
    /// The most any instance writer ever buffered (instrumentation).
    pub peak_buffer: usize,
}

impl<'s> ChunkSink<'s> {
    pub fn new(store: &'s ChunkStore) -> Self {
        Self { store, artifact: ChunkedArtifactV1::default(), cur: None, peak_buffer: 0 }
    }
}

impl TensorSink for ChunkSink<'_> {
    fn begin(&mut self, param: u16, layer: Option<u16>, bytes: u64) -> Result<()> {
        self.cur = Some((InstanceWriter::new(self.store, param, layer), bytes, 0));
        Ok(())
    }
    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        let (w, _, got) = self.cur.as_mut().ok_or_else(|| LowerError::eval("internal: a push outside an instance"))?;
        w.push(bytes).map_err(|e| LowerError::Io(e.to_string()))?;
        *got += bytes.len() as u64;
        self.peak_buffer = self.peak_buffer.max(w.peak_buffer);
        Ok(())
    }
    fn end(&mut self) -> Result<()> {
        let (w, want, got) = self.cur.take().ok_or_else(|| LowerError::eval("internal: an end outside an instance"))?;
        if got != want {
            return Err(LowerError::eval(format!("internal: an instance of {got} bytes, declared {want}")));
        }
        self.artifact.insert(w.finish().map_err(|e| LowerError::Io(e.to_string()))?);
        Ok(())
    }
}

/// What a streaming conversion did (instrumentation, and the proof that it streamed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamStats {
    /// Tensor instances produced.
    pub instances: usize,
    /// Bytes of them.
    pub bytes: u64,
    /// Of the instances, those produced by row blocks (the rest whole, with their occurrence).
    pub by_blocks: usize,
    /// Blocks of rows read and encoded.
    pub blocks: usize,
    /// The largest block's `f32` bytes (the most a deferred param held at once).
    pub max_block_f32_bytes: usize,
    /// HL params an occurrence had to load whole after deferring them (a fill read them whole).
    pub whole: Vec<String>,
}

/// The result of [`materialise_stream`] besides the tensors, which went to the sink.
#[derive(Clone, Debug)]
pub struct StreamMaterialised {
    /// Float value of one unit of the residual stream.
    pub resid_scale: f64,
    /// Float value of one unit of the logits.
    pub logits_scale: f64,
    /// Pre-quantised projections' per-group scales that are not exact at their row's unit.
    pub quant_inexact: usize,
    pub stats: StreamStats,
}

/// Produce every param of `lw` for every occurrence, handing each tensor to `sink` — the streaming
/// twin of [`crate::lower::materialise`], with the same tensors. `progress(done, total)` after each
/// occurrence.
pub fn materialise_stream(
    lw: &Lowered,
    hl: &HlProgram,
    loader: &dyn OccParams,
    calib: &BTreeMap<String, SiteStat>,
    policy: &QuantPolicy,
    opts: &StreamOpts,
    sink: &mut dyn TensorSink,
    progress: &dyn Fn(usize, usize),
) -> Result<StreamMaterialised> {
    let resid = residual_scale(lw, calib, policy)?;
    let used = params_of_blocks(&lw.program);
    let mut done: BTreeSet<(u16, Option<u16>)> = BTreeSet::new();
    let mut st = StreamStats::default();
    let (mut logits_scale, mut quant_inexact) = (None, 0usize);
    let occs = occurrences(hl);
    let total = occs.len();
    for (oi, (hbk, layer, prefix)) in occs.into_iter().enumerate() {
        let tb = lw.block_map[hbk] as usize;
        // The HL params this occurrence reads as row-wise codes and is willing to read by blocks.
        let mut defer: BTreeSet<u32> = BTreeSet::new();
        if let Some(rs) = loader.row_source() {
            for &pi in &used[tb] {
                let d = &lw.program.params[pi as usize];
                if let Some(rp) = lw.row_params.get(&d.name)
                    && let Some((rows, cols)) = rs.row_space(rp.hl, layer)?
                    && rows * cols >= opts.defer_min_elems
                {
                    defer.insert(rp.hl);
                }
            }
        }
        loop {
            let store = if defer.is_empty() { loader.load(hbk, layer)? } else { loader.load_deferring(hbk, layer, &defer)? };
            let ctx = FillCtx::streaming(hl, &store, layer, &prefix, calib, resid, policy, &defer, loader.row_source(), opts.block_elems);
            let attempt = (|| -> Result<()> {
                for &pi in &used[tb] {
                    let d = &lw.program.params[pi as usize];
                    let key = (pi, if d.per_layer { layer.map(|l| l as u16) } else { None });
                    if done.contains(&key) {
                        continue;
                    }
                    let bytes = d.shape.iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64;
                    let by_rows = lw.row_params.get(&d.name).filter(|rp| defer.contains(&rp.hl)).copied();
                    match by_rows {
                        Some(rp) => fill_by_blocks(lw, hl, loader, calib, policy, opts, resid, (&prefix, layer), pi, rp, key, bytes, sink, &mut st)?,
                        None => {
                            let t = (lw.fills[pi as usize])(&ctx).map_err(|e| match deferred_param_of(&e) {
                                Some(_) => e,
                                None => LowerError::eval(format!("filling `{}` at {prefix}: {e}", d.name)),
                            })?;
                            let want: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
                            if t.dtype != d.dtype || t.shape != want || t.len() != want.iter().product::<usize>() {
                                return Err(LowerError::eval(format!(
                                    "internal: fill of `{}` gave {} {:?}, declared {} {want:?}",
                                    d.name,
                                    t.dtype.name(),
                                    t.shape,
                                    d.dtype.name()
                                )));
                            }
                            sink.begin(key.0, key.1, bytes)?;
                            sink.push(&t.le_bytes())?;
                            sink.end()?;
                        }
                    }
                    st.instances += 1;
                    st.bytes += bytes;
                    done.insert(key);
                }
                if hbk == hl.post {
                    logits_scale = Some(ctx.scale(&lw.logits_key)?);
                }
                Ok(())
            })();
            match attempt {
                Ok(()) => {
                    quant_inexact += ctx.quant_inexact();
                    break;
                }
                // A fill read a deferred param whole: load it for this occurrence and run again.
                Err(e) => match deferred_param_of(&e) {
                    Some(p) if defer.remove(&p) => st.whole.push(hl.params[p as usize].name.clone()),
                    _ => return Err(e),
                },
            }
        }
        progress(oi + 1, total);
    }
    Ok(StreamMaterialised { resid_scale: resid, logits_scale: logits_scale.expect("post runs"), quant_inexact, stats: st })
}

/// One row-wise param instance, a block of rows at a time.
#[allow(clippy::too_many_arguments)]
fn fill_by_blocks(
    lw: &Lowered,
    hl: &HlProgram,
    loader: &dyn OccParams,
    calib: &BTreeMap<String, SiteStat>,
    policy: &QuantPolicy,
    opts: &StreamOpts,
    resid: f64,
    (prefix, layer): (&str, Option<usize>),
    pi: u16,
    rp: RowParam,
    key: (u16, Option<u16>),
    bytes: u64,
    sink: &mut dyn TensorSink,
    st: &mut StreamStats,
) -> Result<()> {
    let d = &lw.program.params[pi as usize];
    let rs = loader.row_source().ok_or_else(|| LowerError::eval("internal: a deferred param without a row source"))?;
    let not_rows = |why: String| LowerError::eval(format!("{DEFERRED_MARK}{}: {why}", rp.hl));
    let (rows, cols) = rs.row_space(rp.hl, layer)?.ok_or_else(|| not_rows("not readable by rows".into()))?;
    let (drows, dcols) = (d.shape[..d.shape.len() - 1].iter().map(|x| *x as usize).product::<usize>(), *d.shape.last().expect("a param has a shape") as usize);
    if (rows, cols) != (drows, dcols) {
        return Err(not_rows(format!("the checkpoint's rows are {rows}×{cols}, the param is {drows}×{dcols}")));
    }
    sink.begin(key.0, key.1, bytes)?;
    let step = (opts.block_elems / cols.max(1)).max(1);
    for blk in row_blocks(rows, step) {
        let t = rs.rows(rp.hl, layer, blk.clone())?;
        st.max_block_f32_bytes = st.max_block_f32_bytes.max(t.numel() * 4);
        let mut bstore = ParamStore::default();
        bstore.insert(rp.hl, None, t, None);
        let bctx = FillCtx::for_scales(hl, &bstore, layer, prefix, calib, resid, policy);
        let it: IntTensor = (lw.fills[pi as usize])(&bctx).map_err(|e| LowerError::eval(format!("filling `{}` at {prefix}, rows {blk:?}: {e}", d.name)))?;
        if it.dtype != d.dtype || it.len() != blk.len() * cols {
            return Err(LowerError::eval(format!(
                "internal: the block of `{}` gave {} × {}, wanted {} × {cols}",
                d.name,
                it.dtype.name(),
                it.len(),
                blk.len()
            )));
        }
        sink.push(&it.le_bytes())?;
        st.blocks += 1;
    }
    sink.end()?;
    st.by_blocks += 1;
    Ok(())
}
