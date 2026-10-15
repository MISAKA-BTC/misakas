//! **Layer-major runs of the float reference** — what calibration and the fidelity harness use on
//! a real checkpoint.
//!
//! A step-by-step [`Session`](super::Session) needs every layer's weights at every position, so a
//! 1.5B model would sit in memory as 6 GB of f32. Here the loop is turned around: every position
//! of every sequence goes through one block occurrence (`pre`, layer 0, layer 1, …, `post`) before
//! the next one is loaded, so only one layer's weights are resident at a time ([`Streamed`]). The
//! carries between occurrences are `positions × hidden` floats. Sequences run in parallel (rayon);
//! each keeps its own histories and recurrent states for the occurrence in flight, which is
//! exactly what a step-by-step run would hold for that layer — so the result is the step-by-step
//! result, value for value.

use super::{LazyParam, ParamStore, Session, SiteStat, bind_one, check_shape};
use crate::error::{LowerError, Result};
use crate::hl::HlProgram;
use crate::weights::stream::{eval_src_rows, src_row_space};
use crate::weights::{Binding, Resolver, Tensor, TensorSource};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::{Arc, OnceLock};

/// Supplies the params one occurrence reads (block `bi` at `layer`), globals included.
pub trait OccParams: Sync {
    fn load(&self, bi: usize, layer: Option<usize>) -> Result<Arc<ParamStore>>;

    /// [`load`](Self::load) without the HL params in `defer`: the caller reads those by blocks of
    /// rows through [`row_source`](Self::row_source), so a vocabulary-sized table or a stack of
    /// experts is never resident whole. A loader that cannot serve row ranges ignores `defer`.
    fn load_deferring(&self, bi: usize, layer: Option<usize>, defer: &BTreeSet<u32>) -> Result<Arc<ParamStore>> {
        let _ = defer;
        self.load(bi, layer)
    }

    /// Row-range access to the params (see [`RowSource`]), when this loader has it.
    fn row_source(&self) -> Option<&dyn RowSource> {
        None
    }
}

/// HL params read by blocks of rows — `rows a..b` of the param's value, every axis but the last
/// flattened ([`crate::weights::stream`]).
pub trait RowSource: Sync {
    /// `(rows, cols)` of HL param `p` at occurrence `layer`, when it can be read by row ranges.
    fn row_space(&self, p: u32, layer: Option<usize>) -> Result<Option<(usize, usize)>>;
    /// Rows `rows` of the param, a `[rows.len(), cols]` tensor.
    fn rows(&self, p: u32, layer: Option<usize>, rows: Range<usize>) -> Result<Tensor>;
}

/// Every param already in memory (small models, tests).
pub struct Resident(pub Arc<ParamStore>);

impl OccParams for Resident {
    fn load(&self, _bi: usize, _layer: Option<usize>) -> Result<Arc<ParamStore>> {
        Ok(self.0.clone())
    }
}

/// Params read from a checkpoint per occurrence: only what the block references, for this layer.
pub struct Streamed<'a> {
    pub prog: &'a HlProgram,
    pub binding: &'a Binding,
    pub source: &'a (dyn TensorSource + Sync),
    /// The checkpoint's tensor names, listed once (a resolver per row block would list them again).
    names: OnceLock<Arc<BTreeSet<String>>>,
    /// **L1 (row-streamed calibration)**: a param of at least this many elements (and a source that serves row ranges) is bound LAZILY —
    /// read when an op asks, never resident with its layer. `0`: off (every param of the occurrence is loaded whole, as before).
    lazy_min_elems: usize,
    /// What the lazy params may keep resident between asks, in bytes (see [`ParamStore::set_lazy_cache_bytes`]).
    lazy_cache_bytes: usize,
}

impl<'a> Streamed<'a> {
    pub fn new(prog: &'a HlProgram, binding: &'a Binding, source: &'a (dyn TensorSource + Sync)) -> Self {
        Streamed { prog, binding, source, names: OnceLock::new(), lazy_min_elems: 0, lazy_cache_bytes: 0 }
    }

    /// **Bind the large params lazily** (RFC-0002 Part II §II.9 L1): a param of `min_elems` elements or more is read from the checkpoint when
    /// an op asks for it (the whole tensor, or — for a stack of experts — the rows of the experts a router selected), and kept between
    /// asks only within `cache_bytes`. The float reference then needs the largest single tensor (or one expert) resident, not a layer.
    /// The stores this loader returns borrow the checkpoint, the program and the binding, so they live only inside the run that asked for
    /// them (`run_layer_major` drops each occurrence's store before the next).
    pub fn with_lazy(mut self, min_elems: usize, cache_bytes: usize) -> Self {
        self.lazy_min_elems = min_elems;
        self.lazy_cache_bytes = cache_bytes;
        self
    }

    fn resolver(&self) -> Resolver<'a> {
        let names = self.names.get_or_init(|| Arc::new(self.source.names().into_iter().collect())).clone();
        Resolver::shared(self.source, &self.binding.aliases, names).with_ignored(&self.binding.ignored_prefixes)
    }

    /// The model layer an HL param's source expression is evaluated at (`None` for a global).
    fn model_layer_of(&self, p: u32, layer: Option<usize>) -> Result<Option<usize>> {
        let d = &self.prog.params[p as usize];
        if d.per_layer {
            let l = layer.ok_or_else(|| LowerError::eval(format!("per-layer param `{}` outside a layer", d.name)))?;
            Ok(Some(self.prog.model_layer(l)))
        } else {
            Ok(None)
        }
    }
}

impl RowSource for Streamed<'_> {
    fn row_space(&self, p: u32, layer: Option<usize>) -> Result<Option<(usize, usize)>> {
        if !self.source.serves_row_ranges() {
            return Ok(None);
        }
        let ml = self.model_layer_of(p, layer)?;
        let r = self.resolver();
        src_row_space(&self.binding.srcs[p as usize], &r, ml, &BTreeMap::new())
    }

    fn rows(&self, p: u32, layer: Option<usize>, rows: Range<usize>) -> Result<Tensor> {
        let ml = self.model_layer_of(p, layer)?;
        let r = self.resolver();
        eval_src_rows(&self.binding.srcs[p as usize], &r, ml, &BTreeMap::new(), rows)
    }
}

impl OccParams for Streamed<'_> {
    fn row_source(&self) -> Option<&dyn RowSource> {
        Some(self)
    }

    fn load_deferring(&self, bi: usize, layer: Option<usize>, defer: &BTreeSet<u32>) -> Result<Arc<ParamStore>> {
        self.load_except(bi, layer, defer)
    }

    fn load(&self, bi: usize, layer: Option<usize>) -> Result<Arc<ParamStore>> {
        self.load_except(bi, layer, &BTreeSet::new())
    }
}

/// A param read from the checkpoint when asked (L1). It holds the loader's borrows with their lifetime extended: see the SAFETY note on
/// [`Streamed::lazy_param`].
struct StreamedLazy {
    prog: &'static HlProgram,
    binding: &'static Binding,
    source: &'static (dyn TensorSource + Sync),
    names: Arc<BTreeSet<String>>,
    p: u32,
    ml: Option<usize>,
    shape: Vec<usize>,
}

impl StreamedLazy {
    fn resolver(&self) -> Resolver<'static> {
        Resolver::shared(self.source, &self.binding.aliases, self.names.clone()).with_ignored(&self.binding.ignored_prefixes)
    }
}

impl LazyParam for StreamedLazy {
    fn shape(&self) -> &[usize] {
        &self.shape
    }
    fn load(&self) -> Result<Tensor> {
        let r = self.resolver();
        let d = &self.prog.params[self.p as usize];
        let (t, _) = bind_one(&self.binding.srcs[self.p as usize], &r, self.ml).map_err(|e| {
            LowerError::weights(format!("param `{}`{}: {e}", d.name, self.ml.map(|l| format!(" layer {l}")).unwrap_or_default()))
        })?;
        check_shape(&d.name, &t, &d.shape)?;
        Ok(t)
    }
    fn rows(&self, rows: Range<usize>) -> Result<Tensor> {
        let r = self.resolver();
        eval_src_rows(&self.binding.srcs[self.p as usize], &r, self.ml, &BTreeMap::new(), rows)
    }
}

impl Streamed<'_> {
    /// The lazy form of HL param `pi` at occurrence `layer`, when this loader binds it lazily: the option is on, the param is large enough, its
    /// source serves row ranges and is not pre-quantised (a quantised param's integers are read whole with it).
    ///
    /// SAFETY of the lifetime extension: the returned value borrows `self.prog`, `self.binding` and `self.source`, which the caller of
    /// [`OccParams::load`] holds for as long as it uses the store it gets back — every caller in this crate (`run_layer_major`,
    /// `run_to_post`, `calibrate`, the fidelity harness) drops an occurrence's store before the loader it came from, and none keeps one.
    /// Nothing here is reachable from the store after the loader is gone because nothing outside those runs holds the store.
    fn lazy_param(&self, pi: u32, layer: Option<usize>) -> Result<Option<Arc<dyn LazyParam>>> {
        if self.lazy_min_elems == 0 || !self.source.serves_row_ranges() || self.binding.srcs[pi as usize].is_quant() {
            return Ok(None);
        }
        let ml = self.model_layer_of(pi, layer)?;
        let r = self.resolver();
        let Some((rows, cols)) = src_row_space(&self.binding.srcs[pi as usize], &r, ml, &BTreeMap::new())? else { return Ok(None) };
        if rows.saturating_mul(cols) < self.lazy_min_elems {
            return Ok(None);
        }
        let names = self.names.get_or_init(|| Arc::new(self.source.names().into_iter().collect())).clone();
        // SAFETY: see above.
        let (prog, binding, source) = unsafe {
            (
                std::mem::transmute::<&HlProgram, &'static HlProgram>(self.prog),
                std::mem::transmute::<&Binding, &'static Binding>(self.binding),
                std::mem::transmute::<&(dyn TensorSource + Sync), &'static (dyn TensorSource + Sync)>(self.source),
            )
        };
        let shape = self.prog.params[pi as usize].shape.clone();
        Ok(Some(Arc::new(StreamedLazy { prog, binding, source, names, p: pi, ml, shape })))
    }

    fn load_except(&self, bi: usize, layer: Option<usize>, skip: &BTreeSet<u32>) -> Result<Arc<ParamStore>> {
        let r = self.resolver();
        let mut st = ParamStore::default();
        st.set_lazy_cache_bytes(self.lazy_cache_bytes);
        for pi in self.prog.block_params(bi) {
            if skip.contains(&pi) {
                continue;
            }
            let d = &self.prog.params[pi as usize];
            if let Some(lazy) = self.lazy_param(pi, if d.per_layer { layer } else { None })? {
                st.insert_lazy(pi, if d.per_layer { layer } else { None }, lazy);
                continue;
            }
            if d.per_layer {
                let l = layer.ok_or_else(|| LowerError::eval(format!("per-layer param `{}` outside a layer", d.name)))?;
                // Keyed by the occurrence, read at the model layer it belongs to.
                let ml = self.prog.model_layer(l);
                let (t, q) = bind_one(&self.binding.srcs[pi as usize], &r, Some(ml))
                    .map_err(|e| LowerError::weights(format!("param `{}` layer {ml}: {e}", d.name)))?;
                check_shape(&d.name, &t, &d.shape)?;
                st.insert(pi, Some(l), t, q);
            } else {
                let (t, q) = bind_one(&self.binding.srcs[pi as usize], &r, None)
                    .map_err(|e| LowerError::weights(format!("param `{}`: {e}", d.name)))?;
                check_shape(&d.name, &t, &d.shape)?;
                st.insert(pi, None, t, q);
            }
        }
        Ok(Arc::new(st))
    }
}

/// What a layer-major run returns.
#[derive(Default)]
pub struct LayerMajorRun {
    /// Site statistics over every position of every sequence (when asked for).
    pub stats: BTreeMap<String, SiteStat>,
    /// Logits per sequence and position (when asked for): the positions [`KeepLogits`] keeps, in order.
    pub logits: Vec<Vec<Vec<f32>>>,
    /// The position of each kept row of `logits`.
    pub logit_positions: Vec<Vec<usize>>,
}

/// Which logits a layer-major run keeps. A long sequence's logits are `positions × vocabulary` floats (32,768 × 248,320 is
/// 32 GB); a run that wants a few positions keeps those and drops the rest as they are made.
#[derive(Clone, Debug, Default)]
pub enum KeepLogits {
    #[default]
    None,
    All,
    At(BTreeSet<usize>),
}

impl KeepLogits {
    fn keeps(&self, p: usize) -> bool {
        match self {
            KeepLogits::None => false,
            KeepLogits::All => true,
            KeepLogits::At(set) => set.contains(&p),
        }
    }
}

/// Positions a session evaluates node by node in one run ([`Session::eval_occurrence_run`]): each linear's matrix is read once
/// per run instead of once per position. The result does not depend on it, bit for bit (1 is position by position);
/// `PALW_FLOAT_POSITION_BLOCK` overrides it.
pub const DEFAULT_POSITION_BLOCK: usize = 32;

/// The run length in force: `PALW_FLOAT_POSITION_BLOCK` when it is a positive number, else [`DEFAULT_POSITION_BLOCK`].
pub fn position_block() -> usize {
    std::env::var("PALW_FLOAT_POSITION_BLOCK")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(DEFAULT_POSITION_BLOCK)
}

/// **Carry spill** (RFC-0020 §6, opt-in): where a sequence's carries go when one occurrence's outputs for the whole sequence
/// would exceed `budget_bytes` — a file of rows in `dir`, written in position order and read back by runs of positions in the
/// next occurrence. The rows are the same `f32` bits either way (little-endian on disk), so a spilled run is the in-RAM run bit
/// for bit; it trades RAM for disk I/O.
#[derive(Clone, Debug)]
pub struct SpillCfg {
    pub dir: std::path::PathBuf,
    /// Spill an occurrence's outputs when (first row's bytes × positions) exceeds this (0: always).
    pub budget_bytes: u64,
}

static SPILL: std::sync::Mutex<Option<SpillCfg>> = std::sync::Mutex::new(None);

/// Set (or clear) the carry spill every layer-major run of this process uses ([`run_layer_major`], [`run_to_post`]).
pub fn set_carry_spill(cfg: Option<SpillCfg>) {
    *SPILL.lock().unwrap_or_else(|e| e.into_inner()) = cfg;
}

fn carry_spill() -> Option<SpillCfg> {
    SPILL.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// One sequence's carries between two occurrences: in RAM, or spilled to a file.
pub(crate) enum CarryRows {
    Ram(Vec<Vec<Vec<f32>>>),
    Disk(DiskRows),
}

impl CarryRows {
    fn len(&self) -> usize {
        match self {
            CarryRows::Ram(v) => v.len(),
            CarryRows::Disk(d) => d.index.len(),
        }
    }
    /// Rows `s .. e`, moved out (a RAM row is left empty: the consumed input is freed as the output is made).
    fn take(&mut self, s: usize, e: usize) -> Result<Vec<Vec<Vec<f32>>>> {
        match self {
            CarryRows::Ram(v) => Ok(v[s..e].iter_mut().map(std::mem::take).collect()),
            CarryRows::Disk(d) => d.read(s, e),
        }
    }
    fn ram_bytes(&self) -> u64 {
        match self {
            CarryRows::Ram(v) => v.iter().flatten().map(|r| r.len() as u64 * 4).sum(),
            CarryRows::Disk(_) => 0,
        }
    }
    fn disk_bytes(&self) -> u64 {
        match self {
            CarryRows::Ram(_) => 0,
            CarryRows::Disk(d) => d.bytes,
        }
    }
    /// Every row, in RAM (the end of a run that hands its carries back).
    fn into_ram(self) -> Result<Vec<Vec<Vec<f32>>>> {
        match self {
            CarryRows::Ram(v) => Ok(v),
            CarryRows::Disk(d) => {
                let n = d.index.len();
                d.read(0, n)
            }
        }
    }
}

/// A spilled sequence of rows: `[u32 carries][u32 len][len × f32 LE]…` per position; `index[p]` is position `p`'s offset.
pub(crate) struct DiskRows {
    path: std::path::PathBuf,
    index: Vec<u64>,
    bytes: u64,
}

impl Drop for DiskRows {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn io_err(path: &std::path::Path, e: std::io::Error) -> LowerError {
    LowerError::Io(format!("carry spill {}: {e}", path.display()))
}

impl DiskRows {
    fn read(&self, s: usize, e: usize) -> Result<Vec<Vec<Vec<f32>>>> {
        use std::io::{Read, Seek, SeekFrom};
        if s >= e {
            return Ok(Vec::new());
        }
        let mut f = std::fs::File::open(&self.path).map_err(|x| io_err(&self.path, x))?;
        f.seek(SeekFrom::Start(self.index[s])).map_err(|x| io_err(&self.path, x))?;
        let mut r = std::io::BufReader::with_capacity(1 << 20, f);
        let mut word = [0u8; 4];
        let mut u32_of = |r: &mut std::io::BufReader<std::fs::File>| -> Result<u32> {
            r.read_exact(&mut word).map_err(|x| io_err(&self.path, x))?;
            Ok(u32::from_le_bytes(word))
        };
        let mut rows: Vec<Vec<Vec<f32>>> = Vec::with_capacity(e - s);
        for _ in s..e {
            let k = u32_of(&mut r)? as usize;
            let mut row: Vec<Vec<f32>> = Vec::with_capacity(k);
            for _ in 0..k {
                let n = u32_of(&mut r)? as usize;
                let mut raw = vec![0u8; n * 4];
                r.read_exact(&mut raw).map_err(|x| io_err(&self.path, x))?;
                row.push(raw.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect());
            }
            rows.push(row);
        }
        Ok(rows)
    }
}

/// Where an occurrence's outputs go: RAM, until the first row says the whole sequence would pass the spill budget.
struct CarryWriter {
    ram: Vec<Vec<Vec<f32>>>,
    disk: Option<(std::io::BufWriter<std::fs::File>, DiskRows)>,
    spill: Option<SpillCfg>,
    positions: usize,
    tag: String,
}

static SPILL_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl CarryWriter {
    fn new(spill: Option<SpillCfg>, positions: usize, tag: String) -> Self {
        CarryWriter { ram: Vec::with_capacity(if spill.is_some() { 1 } else { positions }), disk: None, spill, positions, tag }
    }
    fn write_row(w: &mut std::io::BufWriter<std::fs::File>, d: &mut DiskRows, row: &[Vec<f32>]) -> Result<()> {
        use std::io::Write;
        d.index.push(d.bytes);
        let mut buf = Vec::with_capacity(4 + row.iter().map(|v| 4 + v.len() * 4).sum::<usize>());
        buf.extend_from_slice(&(row.len() as u32).to_le_bytes());
        for v in row {
            buf.extend_from_slice(&(v.len() as u32).to_le_bytes());
            for x in v {
                buf.extend_from_slice(&x.to_le_bytes());
            }
        }
        w.write_all(&buf).map_err(|x| io_err(&d.path, x))?;
        d.bytes += buf.len() as u64;
        Ok(())
    }
    fn push(&mut self, row: Vec<Vec<f32>>) -> Result<()> {
        if self.disk.is_none()
            && self.ram.is_empty()
            && let Some(cfg) = &self.spill
            && row.iter().map(|v| v.len() as u64 * 4).sum::<u64>() * self.positions as u64 > cfg.budget_bytes
        {
            let n = SPILL_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = cfg.dir.join(format!("carries-{}-{}-{n}.rows", std::process::id(), self.tag));
            std::fs::create_dir_all(&cfg.dir).map_err(|x| io_err(&cfg.dir, x))?;
            let f = std::fs::File::create(&path).map_err(|x| io_err(&path, x))?;
            self.disk = Some((std::io::BufWriter::with_capacity(8 << 20, f), DiskRows { path, index: Vec::new(), bytes: 0 }));
        }
        match &mut self.disk {
            Some((w, d)) => Self::write_row(w, d, &row),
            None => {
                self.ram.push(row);
                Ok(())
            }
        }
    }
    fn finish(self) -> Result<CarryRows> {
        use std::io::Write;
        match self.disk {
            Some((mut w, d)) => {
                w.flush().map_err(|x| io_err(&d.path, x))?;
                Ok(CarryRows::Disk(d))
            }
            None => Ok(CarryRows::Ram(self.ram)),
        }
    }
}

/// Every position of `toks` through occurrence `(bi, layer)`, in runs of `block` positions, consuming the incoming rows as they
/// are read: each run's input rows are moved out of `rows` (a RAM row is freed, not kept beside its output) and each position's
/// outputs go through `out(position, outputs)` — what it returns becomes the next occurrence's carry row (the post block
/// returns nothing and keeps the logits it wants). The session, the absolute positions, the evaluation order and the order
/// the sites observe are those of a run that held both sequences (RFC-0020 §3). Returns the next occurrence's rows.
#[allow(clippy::too_many_arguments)]
fn run_occurrence(
    sess: &mut Session<'_>,
    bi: usize,
    layer: Option<usize>,
    rows: &mut CarryRows,
    toks: &[usize],
    block: usize,
    mut writer: CarryWriter,
    out: &mut dyn FnMut(usize, Vec<Vec<f32>>) -> Vec<Vec<f32>>,
) -> Result<CarryRows> {
    if rows.len() != toks.len() {
        return Err(LowerError::eval(format!("internal: {} carry rows for {} positions", rows.len(), toks.len())));
    }
    let mut s = 0;
    while s < toks.len() {
        let e = (s + block.max(1)).min(toks.len());
        let cin = rows.take(s, e)?;
        for (j, o) in sess.eval_occurrence_run(bi, layer, &cin, &toks[s..e], s)?.into_iter().enumerate() {
            writer.push(out(s + j, o))?;
        }
        s = e;
    }
    writer.finish()
}

/// Run `seqs` through the program, one occurrence at a time. `progress` is called after each
/// occurrence with its index and the total.
/// **The float reference up to the `post` block**: the post block's inputs for every sequence and
/// position (the final residual stream, `hidden` floats a position). The post block writes no
/// state (NF-19), so a caller evaluates it one position at a time ([`post_logits`]) — a long
/// evaluation never holds `positions × vocabulary` logits.
pub fn run_to_post(
    prog: &HlProgram,
    loader: &dyn OccParams,
    seqs: &[Vec<usize>],
    progress: &dyn Fn(usize, usize),
) -> Result<Vec<Vec<Vec<Vec<f32>>>>> {
    let mut occs: Vec<(usize, Option<usize>)> = vec![(prog.pre, None)];
    occs.extend(prog.schedule.iter().enumerate().map(|(l, k)| (*k as usize, Some(l))));
    let total = occs.len() + 1;
    let block = position_block();
    let spill = carry_spill();
    let mut carries: Vec<CarryRows> = seqs.iter().map(|s| CarryRows::Ram(vec![Vec::new(); s.len()])).collect();
    for (oi, (bi, layer)) in occs.into_iter().enumerate() {
        let store = loader.load(bi, layer)?;
        crate::ram_account::set(crate::ram_account::Kind::Weights, store.resident_bytes());
        let results: Vec<Result<(CarryRows, u64, u64)>> = seqs
            .par_iter()
            .zip(carries.par_iter_mut())
            .enumerate()
            .map(|(si, (toks, rows))| {
                let mut sess = Session::new(prog, &store);
                let writer = CarryWriter::new(spill.clone(), toks.len(), format!("s{si}o{oi}"));
                let next = run_occurrence(&mut sess, bi, layer, rows, toks, block, writer, &mut |_, o| o)?;
                Ok((next, sess.history_bytes(), sess.state_bytes()))
            })
            .collect();
        let (mut kv, mut state) = (0u64, 0u64);
        for (si, r) in results.into_iter().enumerate() {
            let (next, h, f) = r?;
            carries[si] = next;
            kv += h;
            state += f;
        }
        account_occurrence(oi, total, &carries, kv, state);
        progress(oi + 1, total);
    }
    carries.into_iter().map(CarryRows::into_ram).collect()
}

/// RAM accounting after an occurrence (RFC-0020 §2; a no-op unless enabled): the carries held in RAM and on disk, the largest
/// history and recurrent state the occurrence's sessions reached, and a snapshot.
fn account_occurrence(oi: usize, total: usize, carries: &[CarryRows], kv: u64, state: u64) {
    use crate::ram_account::{Kind, set, snapshot};
    if !crate::ram_account::enabled() {
        return;
    }
    set(Kind::Carries, carries.iter().map(CarryRows::ram_bytes).sum());
    set(Kind::Spilled, carries.iter().map(CarryRows::disk_bytes).sum());
    set(Kind::KvHistory, kv);
    set(Kind::RecurrentState, state);
    snapshot(&format!("occurrence {}/{total}", oi + 1));
}

/// The float logits of one position from its `post` inputs ([`run_to_post`]); `store` is
/// `loader.load(prog.post, None)`.
pub fn post_logits(prog: &HlProgram, store: &ParamStore, post_in: &[Vec<f32>], token: usize, pos: usize) -> Result<Vec<f32>> {
    let mut sess = Session::new(prog, store);
    let mut outs = sess.eval_occurrence(prog.post, None, post_in, token, pos)?;
    if outs.is_empty() {
        return Err(LowerError::eval("the post block produced no logits"));
    }
    Ok(outs.swap_remove(0))
}

pub fn run_layer_major(
    prog: &HlProgram,
    loader: &dyn OccParams,
    seqs: &[Vec<usize>],
    want_stats: bool,
    want_logits: bool,
    progress: &dyn Fn(usize, usize),
) -> Result<LayerMajorRun> {
    let keep = if want_logits { KeepLogits::All } else { KeepLogits::None };
    run_layer_major_keep(prog, loader, seqs, want_stats, &keep, position_block(), progress)
}

/// [`run_layer_major`] keeping the logits `logits` names, with runs of `block` positions (the result does not depend on it).
pub fn run_layer_major_keep(
    prog: &HlProgram,
    loader: &dyn OccParams,
    seqs: &[Vec<usize>],
    want_stats: bool,
    logits: &KeepLogits,
    block: usize,
    progress: &dyn Fn(usize, usize),
) -> Result<LayerMajorRun> {
    let mut occs: Vec<(usize, Option<usize>)> = vec![(prog.pre, None)];
    occs.extend(prog.schedule.iter().enumerate().map(|(l, k)| (*k as usize, Some(l))));
    occs.push((prog.post, None));
    let total = occs.len();
    let spill = carry_spill();
    // carries[seq][pos] = the block inputs of the occurrence about to run; each row is consumed as it is read.
    let mut carries: Vec<CarryRows> = seqs.iter().map(|s| CarryRows::Ram(vec![Vec::new(); s.len()])).collect();
    let mut out =
        LayerMajorRun { stats: BTreeMap::new(), logits: vec![Vec::new(); seqs.len()], logit_positions: vec![Vec::new(); seqs.len()] };
    for (oi, (bi, layer)) in occs.into_iter().enumerate() {
        let store = loader.load(bi, layer)?;
        crate::ram_account::set(crate::ram_account::Kind::Weights, store.resident_bytes());
        // The post block's outputs are the logits: the ones nobody wants (all of them in a calibration) are dropped as they
        // are made — a 4,096-token sequence of a 248k vocabulary would otherwise hold 4 GB of rows only to discard them.
        let post = bi == prog.post;
        type SeqOut = (CarryRows, Vec<Vec<f32>>, Vec<usize>, Option<BTreeMap<String, SiteStat>>, u64, u64);
        let results: Vec<Result<SeqOut>> = seqs
            .par_iter()
            .zip(carries.par_iter_mut())
            .enumerate()
            .map(|(si, (toks, rows))| {
                let mut sess = Session::new(prog, &store);
                if want_stats {
                    sess = sess.with_site_stats();
                }
                let (mut kept_rows, mut kept) = (Vec::new(), Vec::new());
                // A layer's outputs are the next occurrence's carries; the post block's are logits: the ones asked for are kept,
                // the row is retired either way (nothing goes to the next occurrence).
                let writer = CarryWriter::new(if post { None } else { spill.clone() }, toks.len(), format!("s{si}o{oi}"));
                let next = run_occurrence(&mut sess, bi, layer, rows, toks, block, writer, &mut |p, mut o| {
                    if !post {
                        return o;
                    }
                    if logits.keeps(p) {
                        kept_rows.push(o.swap_remove(0));
                        kept.push(p);
                    }
                    Vec::new()
                })?;
                let (h, f) = (sess.history_bytes(), sess.state_bytes());
                Ok((next, kept_rows, kept, sess.sites.take(), h, f))
            })
            .collect();
        let (mut kv, mut state) = (0u64, 0u64);
        for (si, r) in results.into_iter().enumerate() {
            let (next, kept_rows, kept, st, h, f) = r?;
            if post {
                out.logits[si] = kept_rows;
                out.logit_positions[si] = kept;
                carries[si] = CarryRows::Ram(Vec::new());
            } else {
                carries[si] = next;
            }
            kv += h;
            state += f;
            if let Some(st) = st {
                for (k, v) in st {
                    out.stats.entry(k).or_default().merge(&v);
                }
            }
        }
        if crate::ram_account::enabled() {
            use crate::ram_account::{Kind, set};
            set(Kind::Stats, out.stats.iter().map(|(k, v)| (k.len() + 96 + v.chan_absmax.len() * 4) as u64).sum());
            set(Kind::Logits, out.logits.iter().flatten().map(|r| r.len() as u64 * 4).sum());
        }
        account_occurrence(oi, total, &carries, kv, state);
        progress(oi + 1, total);
    }
    Ok(out)
}
