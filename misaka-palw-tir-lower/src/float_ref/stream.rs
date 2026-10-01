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

use super::{ParamStore, Session, SiteStat, bind_one, check_shape};
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
}

impl<'a> Streamed<'a> {
    pub fn new(prog: &'a HlProgram, binding: &'a Binding, source: &'a (dyn TensorSource + Sync)) -> Self {
        Streamed { prog, binding, source, names: OnceLock::new() }
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

impl Streamed<'_> {
    fn load_except(&self, bi: usize, layer: Option<usize>, skip: &BTreeSet<u32>) -> Result<Arc<ParamStore>> {
        let r = self.resolver();
        let mut st = ParamStore::default();
        for pi in self.prog.block_params(bi) {
            if skip.contains(&pi) {
                continue;
            }
            let d = &self.prog.params[pi as usize];
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
    /// Logits per sequence and position (when asked for).
    pub logits: Vec<Vec<Vec<f32>>>,
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
    let mut carries: Vec<Vec<Vec<Vec<f32>>>> = seqs.iter().map(|s| vec![Vec::new(); s.len()]).collect();
    for (oi, (bi, layer)) in occs.into_iter().enumerate() {
        let store = loader.load(bi, layer)?;
        let results: Vec<Result<Vec<Vec<Vec<f32>>>>> = seqs
            .par_iter()
            .zip(carries.par_iter())
            .map(|(toks, cin)| {
                let mut sess = Session::new(prog, &store);
                let mut outs = Vec::with_capacity(toks.len());
                for (p, t) in toks.iter().enumerate() {
                    outs.push(sess.eval_occurrence(bi, layer, &cin[p], *t, p)?);
                }
                Ok(outs)
            })
            .collect();
        for (si, r) in results.into_iter().enumerate() {
            carries[si] = r?;
        }
        progress(oi + 1, total);
    }
    Ok(carries)
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
    let mut occs: Vec<(usize, Option<usize>)> = vec![(prog.pre, None)];
    occs.extend(prog.schedule.iter().enumerate().map(|(l, k)| (*k as usize, Some(l))));
    occs.push((prog.post, None));
    let total = occs.len();
    // carries[seq][pos] = the block inputs of the occurrence about to run.
    let mut carries: Vec<Vec<Vec<Vec<f32>>>> = seqs.iter().map(|s| vec![Vec::new(); s.len()]).collect();
    let mut out = LayerMajorRun { stats: BTreeMap::new(), logits: vec![Vec::new(); seqs.len()] };
    for (oi, (bi, layer)) in occs.into_iter().enumerate() {
        let store = loader.load(bi, layer)?;
        // The post block's outputs are the logits: when nobody wants them (a calibration), each
        // is dropped as it is made — a 4,096-token sequence of a 248k vocabulary would otherwise
        // hold 4 GB of rows only to discard them.
        let keep = bi != prog.post || want_logits;
        let results: Vec<Result<(Vec<Vec<Vec<f32>>>, Option<BTreeMap<String, SiteStat>>)>> = seqs
            .par_iter()
            .zip(carries.par_iter())
            .map(|(toks, cin)| {
                let mut sess = Session::new(prog, &store);
                if want_stats {
                    sess = sess.with_site_stats();
                }
                let mut outs = Vec::with_capacity(if keep { toks.len() } else { 0 });
                for (p, t) in toks.iter().enumerate() {
                    let o = sess.eval_occurrence(bi, layer, &cin[p], *t, p)?;
                    if keep {
                        outs.push(o);
                    }
                }
                Ok((outs, sess.sites.take()))
            })
            .collect();
        for (si, r) in results.into_iter().enumerate() {
            let (outs, st) = r?;
            if bi == prog.post {
                if want_logits {
                    out.logits[si] = outs.into_iter().map(|mut o| o.swap_remove(0)).collect();
                }
                carries[si].clear();
            } else {
                carries[si] = outs;
            }
            if let Some(st) = st {
                for (k, v) in st {
                    out.stats.entry(k).or_default().merge(&v);
                }
            }
        }
        progress(oi + 1, total);
    }
    Ok(out)
}
