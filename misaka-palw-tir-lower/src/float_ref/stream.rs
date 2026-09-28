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

use super::{ParamStore, Session, SiteStat, check_shape};
use crate::error::{LowerError, Result};
use crate::hl::HlProgram;
use crate::weights::{Binding, Resolver, TensorSource, eval_src};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Supplies the params one occurrence reads (block `bi` at `layer`), globals included.
pub trait OccParams: Sync {
    fn load(&self, bi: usize, layer: Option<usize>) -> Result<Arc<ParamStore>>;
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
}

impl OccParams for Streamed<'_> {
    fn load(&self, bi: usize, layer: Option<usize>) -> Result<Arc<ParamStore>> {
        let r = Resolver::new(self.source, &self.binding.aliases).with_ignored(&self.binding.ignored_prefixes);
        let none = BTreeMap::new();
        let mut st = ParamStore::default();
        for pi in self.prog.block_params(bi) {
            let d = &self.prog.params[pi as usize];
            if d.per_layer {
                let l = layer.ok_or_else(|| LowerError::eval(format!("per-layer param `{}` outside a layer", d.name)))?;
                let t = eval_src(&self.binding.srcs[pi as usize], &r, Some(l), &none)
                    .map_err(|e| LowerError::weights(format!("param `{}` layer {l}: {e}", d.name)))?;
                check_shape(&d.name, &t, &d.shape)?;
                st.layered.insert((pi, l), t);
            } else {
                let t = eval_src(&self.binding.srcs[pi as usize], &r, None, &none)
                    .map_err(|e| LowerError::weights(format!("param `{}`: {e}", d.name)))?;
                check_shape(&d.name, &t, &d.shape)?;
                st.global.insert(pi, t);
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
        let results: Vec<Result<(Vec<Vec<Vec<f32>>>, Option<BTreeMap<String, SiteStat>>)>> = seqs
            .par_iter()
            .zip(carries.par_iter())
            .map(|(toks, cin)| {
                let mut sess = Session::new(prog, &store);
                if want_stats {
                    sess = sess.with_site_stats();
                }
                let mut outs = Vec::with_capacity(toks.len());
                for (p, t) in toks.iter().enumerate() {
                    outs.push(sess.eval_occurrence(bi, layer, &cin[p], *t, p)?);
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
