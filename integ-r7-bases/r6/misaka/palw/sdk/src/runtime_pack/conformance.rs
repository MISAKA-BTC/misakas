//! **Independent conformance**: a lowered program with its integer tensors, run on every executor
//! this build has, must be the same bytes — at every position the logits, and every commit point (slot,
//! block, layer, node, value).
//!
//! * the reference evaluator (`misaka-palw-tir`),
//! * the typed backend nodes run (`misaka-palw-tir-exec`),
//! * the independent second implementation (`misaka-palw-tir-ref2`: written from the specification
//!   alone, decoding the canonical bytes with its own codec).
//!
//! A [`ConformanceJob`] is a prompt and a number of greedily decoded tokens (arg-max of the logits,
//! the lowest index on a tie); the result is a [`ConformanceVector`]: the tokens and two digests — over
//! every position's logits and over every commit — that the pack records and that anyone with the
//! artifact reproduces on any executor.

use super::manifest::{ConformanceVector, ImplRec, hex};
use misaka_palw_tir::{Interpreter, RunState, TirProgramV1};
use misaka_palw_tir_artifact::PalwTirContainerV1;
use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_lower::lower::{IntParams, IntTensor};
use std::path::Path;

/// One job: a prompt, then `decode` greedily decoded tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceJob {
    pub label: String,
    pub prompt: Vec<usize>,
    pub decode: usize,
}

/// Which executors beyond the reference evaluator run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImplSet {
    pub exec: bool,
    pub ref2: bool,
}

impl Default for ImplSet {
    fn default() -> Self {
        ImplSet { exec: true, ref2: true }
    }
}

impl ImplSet {
    /// The implementations a pack records for this set.
    pub fn records(&self) -> Vec<ImplRec> {
        let v = env!("CARGO_PKG_VERSION").to_string();
        let mut out = vec![ImplRec { name: "reference".into(), crate_name: "misaka-palw-tir".into(), crate_version: v.clone() }];
        if self.exec {
            out.push(ImplRec { name: "typed-backend".into(), crate_name: "misaka-palw-tir-exec".into(), crate_version: v.clone() });
        }
        if self.ref2 {
            out.push(ImplRec { name: "independent".into(), crate_name: "misaka-palw-tir-ref2".into(), crate_version: v });
        }
        out
    }
}

/// One commit: `(slot, block, layer, node, values)`.
type Commit = (u64, u8, Option<u32>, u16, Vec<i128>);

struct Collect(Vec<Commit>);
impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.push((v.slot as u64, v.block, v.layer.map(u32::from), v.node, v.data.to_i128s()));
        }
    }
}

pub(crate) fn ref2_dtype(d: misaka_palw_tir::DType) -> misaka_palw_tir_ref2::DType {
    use misaka_palw_tir::DType as A;
    use misaka_palw_tir_ref2::DType as B;
    match d {
        A::I8 => B::I8,
        A::I16 => B::I16,
        A::I32 => B::I32,
        A::I64 => B::I64,
        A::I128 => B::I128,
        A::Idx => B::Idx,
    }
}

/// BLAKE2b-256 (keyed by `domain`) over a sequence of chunks.
fn digest(domain: &[u8], parts: impl Iterator<Item = Vec<u8>>) -> String {
    let mut st = blake2b_simd::Params::new().hash_length(32).key(domain).to_state();
    for p in parts {
        st.update(&p);
    }
    hex(st.finalize().as_bytes())
}

pub const LOGITS_DIGEST_KEY: &[u8] = b"misaka.palw.conformance.logits.v1";
pub const COMMITS_DIGEST_KEY: &[u8] = b"misaka.palw.conformance.commits.v1";

fn logits_chunk(l: &[i128]) -> Vec<u8> {
    let mut b = (l.len() as u32).to_le_bytes().to_vec();
    for x in l {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

fn commit_chunk(c: &Commit) -> Vec<u8> {
    let mut b = c.0.to_le_bytes().to_vec();
    b.push(c.1);
    match c.2 {
        Some(l) => {
            b.push(1);
            b.extend_from_slice(&l.to_le_bytes());
        }
        None => b.push(0),
    }
    b.extend_from_slice(&c.3.to_le_bytes());
    b.extend_from_slice(&logits_chunk(&c.4));
    b
}

/// The lowest index of the largest logit.
pub fn argmax(l: &[i128]) -> usize {
    let mut best = 0;
    for (i, x) in l.iter().enumerate() {
        if *x > l[best] {
            best = i;
        }
    }
    best
}

/// One tensor's little-endian bytes, by `(param, layer)`.
pub type TensorBytes = ((u16, Option<u16>), Vec<u8>);

/// The artifact's integer params in the three executors' forms.
pub struct LoadedArtifact {
    pub program: TirProgramV1,
    params: IntParams,
    bytes: Vec<TensorBytes>,
}

impl LoadedArtifact {
    /// Read every tensor of a `PALWTIR1` container into memory (the program's whole artifact: a pack's
    /// conformance of a large class needs the memory of its artifact, once per executor form).
    pub fn open(path: &Path) -> Result<LoadedArtifact, String> {
        let c = PalwTirContainerV1::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut params = IntParams::default();
        let mut bytes = Vec::new();
        for e in &c.header.tensors {
            let b = c.read_tensor_bytes(e.param, e.layer).map_err(|x| format!("param {}: {x}", e.param))?;
            let d = c
                .program
                .params
                .get(e.param as usize)
                .ok_or_else(|| format!("a tensor for param {}, which the program does not declare", e.param))?;
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            let t = IntTensor::from_le_bytes(d.dtype, shape, &b).map_err(|x| format!("param `{}`: {x}", d.name))?;
            params.tensors.insert((e.param, e.layer), t);
            bytes.push(((e.param, e.layer), b));
        }
        Ok(LoadedArtifact { program: c.program.clone(), params, bytes })
    }

    pub fn params(&self) -> &IntParams {
        &self.params
    }

    /// The tensors' little-endian bytes by `(param, layer)`.
    pub fn bytes_by_param(&self) -> impl Iterator<Item = &TensorBytes> {
        self.bytes.iter()
    }
}

/// Run `jobs` on the reference evaluator and — per `impls` — the typed backend and the independent
/// implementation, every position's logits and commits compared. `progress(job)` after each job.
pub fn run(
    a: &LoadedArtifact,
    jobs: &[ConformanceJob],
    impls: ImplSet,
    progress: &dyn Fn(usize),
) -> Result<Vec<ConformanceVector>, String> {
    let p = &a.program;
    // ref2: its own decoding of the canonical bytes, its own tensors.
    let (p2, params2) = if impls.ref2 {
        let p2 = misaka_palw_tir_ref2::codec::decode_canonical(&p.encode()).map_err(|e| format!("ref2 refuses the program: {e:?}"))?;
        let mut params2 = misaka_palw_tir_ref2::eval::Params::new();
        for ((j, layer), b) in &a.bytes {
            let d = &p2.params[*j as usize];
            let shape = d.shape.iter().map(|x| *x as u64).collect();
            let t2 = misaka_palw_tir_ref2::Tensor::from_le_bytes(d.dtype, shape, b).map_err(|e| format!("ref2 tensor: {e:?}"))?;
            if d.dtype != ref2_dtype(p.params[*j as usize].dtype) {
                return Err(format!("ref2 reads param {j} as another dtype"));
            }
            params2.insert((*j, layer.map(u32::from)), t2);
        }
        (Some(p2), Some(params2))
    } else {
        (None, None)
    };
    // exec: the plan and borrowed little-endian params.
    let (plan, xparams) = if impls.exec {
        let plan = TirPlan::compile(p).map_err(|e| format!("exec plan: {e}"))?;
        let mut xp = TirParams::new(&plan);
        for ((j, layer), b) in &a.bytes {
            let data = ParamData::from_le_bytes(p.params[*j as usize].dtype, b).map_err(|e| e.to_string())?;
            xp.insert(&plan, *j, *layer, data).map_err(|e| e.to_string())?;
        }
        (Some(plan), Some(xp))
    } else {
        (None, None)
    };
    let interp = Interpreter::new(p).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(jobs.len());
    for (ji, job) in jobs.iter().enumerate() {
        let vocab = p.token_bound as usize;
        if job.prompt.is_empty() || job.prompt.iter().any(|t| *t >= vocab) {
            return Err(format!("{}: the prompt is empty or has a token outside the vocabulary of {vocab}", job.label));
        }
        let mut st1 = RunState::default();
        let mut st2 = p2.as_ref().map(misaka_palw_tir_ref2::eval::initial_state);
        let mut exec = match (&plan, &xparams) {
            (Some(pl), Some(xp)) => Some(TirExecutor::new(pl, xp).map_err(|e| e.to_string())?),
            _ => None,
        };
        let total = job.prompt.len() + job.decode;
        let (mut logit_chunks, mut commit_chunks, mut tokens) = (Vec::new(), Vec::new(), Vec::new());
        let mut tok = job.prompt[0];
        for pos in 0..total {
            let o1 = interp.step(a.params(), &mut st1, tok as u32).map_err(|e| format!("{}: reference at {pos}: {e}", job.label))?;
            let c1: Vec<Commit> =
                o1.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())).collect();
            if let (Some(p2), Some(params2), Some(st)) = (&p2, &params2, st2.as_mut()) {
                let (o2, next) = misaka_palw_tir_ref2::eval::step(p2, params2, &*st, tok as u64)
                    .map_err(|e| format!("{}: ref2 at {pos}: {e:?}", job.label))?;
                *st = next;
                let c2: Vec<Commit> = o2.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
                if o1.logits.data != o2.logits.data {
                    return Err(format!(
                        "{}: position {pos}: the logits differ between the reference evaluator and the independent implementation",
                        job.label
                    ));
                }
                if c1 != c2 {
                    return Err(format!(
                        "{}: position {pos}: the reference evaluator and the independent implementation commit differently ({} vs {} commits)",
                        job.label,
                        c1.len(),
                        c2.len()
                    ));
                }
            }
            if let Some(ex) = exec.as_mut() {
                let mut sink = Collect(Vec::new());
                ex.step(tok as u32, &mut sink).map_err(|e| format!("{}: exec at {pos}: {e}", job.label))?;
                let (_, xl) = ex.logits();
                let mut c3 = sink.0;
                c3.sort_by_key(|c| c.0);
                if o1.logits.data != xl.to_i128s() {
                    return Err(format!(
                        "{}: position {pos}: the logits differ between the reference evaluator and the typed backend",
                        job.label
                    ));
                }
                if c1 != c3 {
                    return Err(format!(
                        "{}: position {pos}: the reference evaluator and the typed backend commit differently ({} vs {} commits)",
                        job.label,
                        c1.len(),
                        c3.len()
                    ));
                }
            }
            logit_chunks.push(logits_chunk(&o1.logits.data));
            commit_chunks.extend(c1.iter().map(commit_chunk));
            tok = if pos + 1 < job.prompt.len() {
                job.prompt[pos + 1]
            } else {
                let t = argmax(&o1.logits.data);
                tokens.push(t);
                t
            };
        }
        // The last decoded token is chosen after the last position computed; it is recorded, not run.
        out.push(ConformanceVector {
            label: job.label.clone(),
            prompt: job.prompt.clone(),
            decode: job.decode,
            tokens: tokens.into_iter().take(job.decode).collect(),
            positions: total,
            logits_digest: digest(LOGITS_DIGEST_KEY, logit_chunks.into_iter()),
            commits_digest: digest(COMMITS_DIGEST_KEY, commit_chunks.into_iter()),
        });
        progress(ji);
    }
    Ok(out)
}

/// The jobs a pack's conformance runs: `prompts` seeded random prompts of up to `prefill` tokens, each
/// decoding `decode` tokens (the vocabulary and the seed fix them).
pub fn jobs(vocab: usize, prompts: usize, prefill: usize, decode: usize, seed: u64) -> Vec<ConformanceJob> {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(seed);
    (0..prompts)
        .map(|i| {
            let len = rng.gen_range(1..=prefill.max(1));
            let prompt = (0..len).map(|_| rng.gen_range(0..vocab)).collect();
            ConformanceJob { label: format!("random {i} ({len}+{decode})"), prompt, decode }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The streamed form (RFC-0002 Part II §II.9 L2)
// ---------------------------------------------------------------------------------------------

/// **An artifact over this many bytes is conformed streamed unless told otherwise**: [`run`] holds the artifact whole once per executor
/// form (the reference's tensors are `i128`), which a 33 GiB class cannot afford; [`run_streamed`] reads each tensor for the reference
/// when it is asked and drops it, and runs the typed backend over the mapped file.
pub const STREAM_ABOVE_BYTES_V1: u64 = 2 << 30;

/// Independent decoding from the same authenticated container, without a whole-model parameter map.
/// Only I/O is shared: ref2 decodes the program/tensors and evaluates primitives independently.
struct LazyIndependentParams<'a> {
    container: &'a misaka_palw_tir_artifact::PalwTirContainerV1,
    program: &'a misaka_palw_tir_ref2::program::Program,
    peak: std::cell::Cell<u64>,
}

impl misaka_palw_tir_ref2::eval::ParamSource for LazyIndependentParams<'_> {
    fn tensor(&self, index: u16, layer: Option<u32>) -> misaka_palw_tir_ref2::error::Res<Option<misaka_palw_tir_ref2::Tensor>> {
        use misaka_palw_tir_ref2::error::{Class, TirError};
        let Some(d) = self.program.params.get(index as usize) else { return Ok(None) };
        let layer = layer.map(u16::try_from).transpose().map_err(|e| TirError::new(Class::Index, e.to_string()))?;
        let bytes = self
            .container
            .read_tensor_bytes(index, layer)
            .map_err(|e| TirError::new(Class::Missing, format!("param {index}: {e}")))?;
        let tensor = misaka_palw_tir_ref2::Tensor::from_le_bytes(d.dtype, d.shape.iter().map(|x| *x as u64).collect(), &bytes)?;
        self.peak.set(self.peak.get().max((tensor.data.len() * std::mem::size_of::<i128>()) as u64));
        Ok(Some(tensor))
    }
}

/// What a streamed run did and did not do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamedNote {
    /// The executors that ran (`reference`, `typed-backend`, `independent`).
    pub ran: Vec<String>,
    /// Why the independent implementation did not run, when it was asked and did not.
    pub ref2_skipped: Option<String>,
    /// The largest tensor the reference held at once, in bytes of its `i128` form.
    pub reference_peak_tensor_bytes: u64,
    /// Largest independently decoded parameter, not total process RSS or activation memory. Under tiling this is the most param elements the
    /// independent implementation held at once (a tile, or a param loaded whole) in its `i128` form.
    pub independent_peak_tensor_bytes: u64,
    /// RFC-0013 §5: what the tiled independent implementation did (`None` when it read whole tensors).
    pub independent_tiles: Option<misaka_palw_tir_ref2::tiled::TileReport>,
    /// RFC-0013 §5: the container reads behind those tiles, with the leaves hashed when a stored Merkle index authenticated them.
    pub independent_rows: Option<crate::tir_rows::RowReadStatsV1>,
}

/// **How the independent implementation reads a param larger than a tile** (RFC-0013 §5). `MatMul` and `Gather` consume it in row tiles of at
/// most `max(tile_elems, one row)` elements — never whole — through the container's rows; with `strict`, a param larger than a tile that no
/// tiled primitive reads is refused instead of loaded whole, so the residency bound is a guarantee. With an `index` every tile is authenticated
/// by hashing only the leaves that cover it ([`crate::tir_merkle_index`]); the caller has shown the index folds to the root it holds. The
/// vectors are the same bytes as the untiled run's (the tiled evaluation is bit-identical to the whole-tensor one); only the memory moves.
#[derive(Clone, Copy)]
pub struct StreamTilingV1<'a> {
    pub tile_elems: u64,
    pub strict: bool,
    pub index: Option<&'a crate::tir_merkle_index::PalwTirMerkleIndexV1>,
}

/// **[`run`], streamed**: the artifact at `path` is opened as a node opens it (mapped), the reference evaluator reads each param through a
/// lazy source over the container (one tensor decoded per ask and dropped), the typed backend runs over the mapping, and the independent
/// implementation also decodes parameters on demand, regardless of artifact size. The vectors are the same bytes [`run`] gives (the digests are over the
/// reference's outputs), so a pack built either way verifies either way.
pub fn run_streamed(
    path: &Path,
    jobs: &[ConformanceJob],
    impls: ImplSet,
    progress: &dyn Fn(usize),
) -> Result<(Vec<ConformanceVector>, StreamedNote), String> {
    run_streamed_with_progress(path, jobs, impls, progress, &|_, _| {})
}

/// Notify after all requested executors agree at a position; both indices are zero-based.
/// This reports progress, not a persisted verification receipt or full source fidelity.
pub fn run_streamed_with_progress(
    path: &Path,
    jobs: &[ConformanceJob],
    impls: ImplSet,
    progress: &dyn Fn(usize),
    position_progress: &dyn Fn(usize, usize),
) -> Result<(Vec<ConformanceVector>, StreamedNote), String> {
    run_streamed_tiled_with_progress(path, jobs, impls, None, progress, position_progress)
}

/// [`run_streamed_with_progress`], with the independent implementation reading params in row tiles when `tiling` is given (RFC-0013 §5).
/// Without `impls.ref2` there is no independent implementation and `tiling` has nothing to apply to.
pub fn run_streamed_tiled_with_progress(
    path: &Path,
    jobs: &[ConformanceJob],
    impls: ImplSet,
    tiling: Option<&StreamTilingV1<'_>>,
    progress: &dyn Fn(usize),
    position_progress: &dyn Fn(usize, usize),
) -> Result<(Vec<ConformanceVector>, StreamedNote), String> {
    let artifact = misaka_palw_tir_exec::node::TirArtifactV1::open(path)?;
    let container = artifact.container();
    let p = &container.program;
    let lazy = misaka_palw_tir_exec::node::LazyContainerParams::new(container);
    let interp = Interpreter::new(p).map_err(|e| e.to_string())?;
    let p2 = if impls.ref2 {
        Some(misaka_palw_tir_ref2::codec::decode_canonical(&p.encode()).map_err(|e| format!("ref2 refuses the program: {e:?}"))?)
    } else {
        None
    };
    // The independent implementation's params: whole tensors decoded on demand, or (RFC-0013 §5) row tiles.
    let lazy2 = match (&p2, tiling) {
        (Some(program), None) => Some(LazyIndependentParams { container, program, peak: std::cell::Cell::new(0) }),
        _ => None,
    };
    let row_source = match (&p2, tiling) {
        (Some(program), Some(t)) => Some(crate::tir_rows::ContainerRowSource::new(container, program, t.index)?),
        _ => None,
    };
    let tiled2 = match (&row_source, tiling) {
        (Some(rows), Some(t)) => {
            let tiled = misaka_palw_tir_ref2::tiled::TiledParams::new(rows, t.tile_elems);
            Some(if t.strict { tiled.strict() } else { tiled })
        }
        _ => None,
    };
    let params2: Option<&dyn misaka_palw_tir_ref2::eval::ParamSource> = match (&tiled2, &lazy2) {
        (Some(t), _) => Some(t),
        (None, Some(l)) => Some(l),
        (None, None) => None,
    };
    let mut ran = vec!["reference".to_string()];
    if impls.exec {
        ran.push("typed-backend".into());
    }
    if p2.is_some() {
        ran.push("independent".into());
    }
    let mut out = Vec::with_capacity(jobs.len());
    for (ji, job) in jobs.iter().enumerate() {
        let vocab = p.token_bound as usize;
        if job.prompt.is_empty() || job.prompt.iter().any(|t| *t >= vocab) {
            return Err(format!("{}: the prompt is empty or has a token outside the vocabulary of {vocab}", job.label));
        }
        let mut st1 = RunState::default();
        let mut st2 = p2.as_ref().map(misaka_palw_tir_ref2::eval::initial_state);
        let mut exec =
            if impls.exec { Some(TirExecutor::new(artifact.plan(), artifact.params()).map_err(|e| e.to_string())?) } else { None };
        let total = job.prompt.len() + job.decode;
        let (mut logit_chunks, mut commit_chunks, mut tokens) = (Vec::new(), Vec::new(), Vec::new());
        let mut tok = job.prompt[0];
        for pos in 0..total {
            let o1 = interp.step(&lazy, &mut st1, tok as u32).map_err(|e| format!("{}: reference at {pos}: {e}", job.label))?;
            let c1: Vec<Commit> =
                o1.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())).collect();
            if let (Some(p2), &Some(params2), Some(st)) = (&p2, &params2, st2.as_mut()) {
                let (o2, next) = misaka_palw_tir_ref2::eval::step(p2, params2, &*st, tok as u64)
                    .map_err(|e| format!("{}: ref2 at {pos}: {e:?}", job.label))?;
                *st = next;
                let c2: Vec<Commit> = o2.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
                if o1.logits.data != o2.logits.data {
                    return Err(format!(
                        "{}: position {pos}: the logits differ between the reference evaluator and the independent implementation",
                        job.label
                    ));
                }
                if c1 != c2 {
                    return Err(format!(
                        "{}: position {pos}: the reference evaluator and the independent implementation commit differently ({} vs {} commits)",
                        job.label,
                        c1.len(),
                        c2.len()
                    ));
                }
            }
            if let Some(ex) = exec.as_mut() {
                let mut sink = Collect(Vec::new());
                ex.step(tok as u32, &mut sink).map_err(|e| format!("{}: exec at {pos}: {e}", job.label))?;
                let (_, xl) = ex.logits();
                let mut c3 = sink.0;
                c3.sort_by_key(|c| c.0);
                if o1.logits.data != xl.to_i128s() {
                    return Err(format!(
                        "{}: position {pos}: the logits differ between the reference evaluator and the typed backend",
                        job.label
                    ));
                }
                if c1 != c3 {
                    return Err(format!(
                        "{}: position {pos}: the reference evaluator and the typed backend commit differently ({} vs {} commits)",
                        job.label,
                        c1.len(),
                        c3.len()
                    ));
                }
            }
            logit_chunks.push(logits_chunk(&o1.logits.data));
            commit_chunks.extend(c1.iter().map(commit_chunk));
            tok = if pos + 1 < job.prompt.len() {
                job.prompt[pos + 1]
            } else {
                let t = argmax(&o1.logits.data);
                tokens.push(t);
                t
            };
            position_progress(ji, pos);
        }
        out.push(ConformanceVector {
            label: job.label.clone(),
            prompt: job.prompt.clone(),
            decode: job.decode,
            tokens: tokens.into_iter().take(job.decode).collect(),
            positions: total,
            logits_digest: digest(LOGITS_DIGEST_KEY, logit_chunks.into_iter()),
            commits_digest: digest(COMMITS_DIGEST_KEY, commit_chunks.into_iter()),
        });
        progress(ji);
    }
    Ok((
        out,
        StreamedNote {
            ran,
            ref2_skipped: None,
            reference_peak_tensor_bytes: lazy.peak_tensor_bytes(),
            independent_peak_tensor_bytes: match (&tiled2, &lazy2) {
                (Some(t), _) => t.report().peak_param_elems().saturating_mul(std::mem::size_of::<i128>() as u64),
                (None, Some(l)) => l.peak.get(),
                (None, None) => 0,
            },
            independent_tiles: tiled2.as_ref().map(|t| t.report()),
            independent_rows: row_source.as_ref().map(|r| r.stats()),
        },
    ))
}

// ---------------------------------------------------------------------------------------------------------------------------
// The per-implementation runner (beacon conformance)
// ---------------------------------------------------------------------------------------------------------------------------

/// What one implementation computed for one job: BLAKE2b-256 (keyed, the same keys as the pack's vectors) over ITS OWN logits and ITS
/// OWN commit points, position by position — not a copy of the reference's. Two implementations that agree give equal digests; one that
/// disagrees gives a different digest at the position it disagrees at, which is also where the job stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImplRunV1 {
    pub logits_digest: [u8; 32],
    pub commits_digest: [u8; 32],
    /// The tokens this implementation's arg-max chose after the prompt.
    pub tokens: Vec<u32>,
    pub positions: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImplOutcomeV1 {
    Ran(ImplRunV1),
    /// The implementation refused or failed: the message is part of the record.
    Error(String),
    /// Not asked to run (`ImplSet` switched it off): a skipped check, never a pass.
    NotRun,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TripleResult {
    pub reference: ImplOutcomeV1,
    pub independent: ImplOutcomeV1,
    pub backend: ImplOutcomeV1,
    /// The first position at which two implementations differed (logits or commit points), or the first refusal.
    pub disagreement: Option<String>,
}

struct Acc {
    logits: blake2b_simd::State,
    commits: blake2b_simd::State,
    tokens: Vec<u32>,
    positions: u32,
}

impl Acc {
    fn new() -> Self {
        let key = |k: &[u8]| blake2b_simd::Params::new().hash_length(32).key(k).to_state();
        Acc { logits: key(LOGITS_DIGEST_KEY), commits: key(COMMITS_DIGEST_KEY), tokens: Vec::new(), positions: 0 }
    }

    fn push(&mut self, logits: &[i128], commits: &[Commit]) {
        self.logits.update(&logits_chunk(logits));
        for c in commits {
            self.commits.update(&commit_chunk(c));
        }
        self.positions += 1;
    }

    fn finish(self) -> ImplRunV1 {
        let mut a = [0u8; 32];
        a.copy_from_slice(self.logits.finalize().as_bytes());
        let mut b = [0u8; 32];
        b.copy_from_slice(self.commits.finalize().as_bytes());
        ImplRunV1 { logits_digest: a, commits_digest: b, tokens: self.tokens, positions: self.positions }
    }
}

/// **The three implementations open over one artifact, for many jobs.** The reference reads each tensor when asked (lazily), the typed
/// backend runs over the mapped file, the independent implementation decodes the canonical bytes with its own codec and reads each
/// tensor when asked — no executor holds the artifact whole. Memory is the largest expanded tensor plus activations and the mapping's
/// residency, as RFC-0013 §5 says: not a universal bound.
pub struct TripleRunner {
    artifact: misaka_palw_tir_exec::node::TirArtifactV1,
    p2: Option<misaka_palw_tir_ref2::program::Program>,
    impls: ImplSet,
}

impl TripleRunner {
    pub fn open(path: &Path, impls: ImplSet) -> Result<Self, String> {
        let artifact = misaka_palw_tir_exec::node::TirArtifactV1::open(path)?;
        let p2 = if impls.ref2 {
            Some(
                misaka_palw_tir_ref2::codec::decode_canonical(&artifact.container().program.encode())
                    .map_err(|e| format!("ref2 refuses the program: {e:?}"))?,
            )
        } else {
            None
        };
        Ok(Self { artifact, p2, impls })
    }

    pub fn program(&self) -> &TirProgramV1 {
        &self.artifact.container().program
    }

    /// Run one job on every enabled implementation, comparing logits and every commit point at every position.
    pub fn run_job(&self, job: &ConformanceJob) -> Result<TripleResult, String> {
        let container = self.artifact.container();
        let p = &container.program;
        let vocab = p.token_bound as usize;
        if job.prompt.is_empty() || job.prompt.iter().any(|t| *t >= vocab) {
            return Err(format!("{}: the prompt is empty or has a token outside the vocabulary of {vocab}", job.label));
        }
        let lazy = misaka_palw_tir_exec::node::LazyContainerParams::new(container);
        let interp = Interpreter::new(p).map_err(|e| e.to_string())?;
        let params2 = self.p2.as_ref().map(|program| LazyIndependentParams { container, program, peak: std::cell::Cell::new(0) });
        let mut st1 = RunState::default();
        let mut st2 = self.p2.as_ref().map(misaka_palw_tir_ref2::eval::initial_state);
        let mut exec = if self.impls.exec {
            Some(TirExecutor::new(self.artifact.plan(), self.artifact.params()).map_err(|e| e.to_string())?)
        } else {
            None
        };
        let (mut a1, mut a2, mut a3) = (Acc::new(), Acc::new(), Acc::new());
        let (mut e2, mut e3): (Option<String>, Option<String>) = (None, None);
        let mut disagreement = None;
        let total = job.prompt.len() + job.decode;
        let mut tok = job.prompt[0];
        'positions: for pos in 0..total {
            let o1 = match interp.step(&lazy, &mut st1, tok as u32) {
                Ok(o) => o,
                Err(e) => {
                    return Ok(TripleResult {
                        reference: ImplOutcomeV1::Error(format!("position {pos}: {e}")),
                        independent: if self.impls.ref2 { ImplOutcomeV1::Ran(a2.finish()) } else { ImplOutcomeV1::NotRun },
                        backend: if self.impls.exec { ImplOutcomeV1::Ran(a3.finish()) } else { ImplOutcomeV1::NotRun },
                        disagreement: Some(format!("the reference evaluator refused at position {pos}: {e}")),
                    });
                }
            };
            let c1: Vec<Commit> =
                o1.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())).collect();
            a1.push(&o1.logits.data, &c1);
            let mut stop = false;
            if let (Some(p2), Some(params2), Some(st)) = (self.p2.as_ref(), params2.as_ref(), st2.as_mut()) {
                match misaka_palw_tir_ref2::eval::step(p2, params2, &*st, tok as u64) {
                    Ok((o2, next)) => {
                        *st = next;
                        let c2: Vec<Commit> =
                            o2.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
                        a2.push(&o2.logits.data, &c2);
                        if pos + 1 >= job.prompt.len() {
                            a2.tokens.push(argmax(&o2.logits.data) as u32);
                        }
                        if o1.logits.data != o2.logits.data {
                            disagreement.get_or_insert(format!(
                                "position {pos}: the logits differ between the reference evaluator and the independent implementation"
                            ));
                            stop = true;
                        } else if c1 != c2 {
                            disagreement.get_or_insert(format!(
                                "position {pos}: the reference evaluator and the independent implementation commit differently ({} vs {} commits)",
                                c1.len(),
                                c2.len()
                            ));
                            stop = true;
                        }
                    }
                    Err(e) => {
                        e2 = Some(format!("position {pos}: {e:?}"));
                        disagreement.get_or_insert(format!("the independent implementation refused at position {pos}: {e:?}"));
                        stop = true;
                    }
                }
            }
            if let Some(ex) = exec.as_mut() {
                let mut sink = Collect(Vec::new());
                match ex.step(tok as u32, &mut sink) {
                    Ok(()) => {
                        let (_, xl) = ex.logits();
                        let xl = xl.to_i128s();
                        let mut c3 = sink.0;
                        c3.sort_by_key(|c| c.0);
                        a3.push(&xl, &c3);
                        if pos + 1 >= job.prompt.len() {
                            a3.tokens.push(argmax(&xl) as u32);
                        }
                        if o1.logits.data != xl {
                            disagreement.get_or_insert(format!(
                                "position {pos}: the logits differ between the reference evaluator and the typed backend"
                            ));
                            stop = true;
                        } else if c1 != c3 {
                            disagreement.get_or_insert(format!(
                                "position {pos}: the reference evaluator and the typed backend commit differently ({} vs {} commits)",
                                c1.len(),
                                c3.len()
                            ));
                            stop = true;
                        }
                    }
                    Err(e) => {
                        e3 = Some(format!("position {pos}: {e}"));
                        disagreement.get_or_insert(format!("the typed backend refused at position {pos}: {e}"));
                        stop = true;
                    }
                }
            }
            if pos + 1 >= job.prompt.len() {
                a1.tokens.push(argmax(&o1.logits.data) as u32);
            }
            if stop {
                break 'positions;
            }
            tok = if pos + 1 < job.prompt.len() { job.prompt[pos + 1] } else { argmax(&o1.logits.data) };
        }
        // The last decoded token is chosen after the last position computed; it is recorded, not run.
        for a in [&mut a1, &mut a2, &mut a3] {
            a.tokens.truncate(job.decode);
        }
        let outcome = |enabled: bool, err: Option<String>, a: Acc| match (enabled, err) {
            (false, _) => ImplOutcomeV1::NotRun,
            (true, Some(e)) => ImplOutcomeV1::Error(e),
            (true, None) => ImplOutcomeV1::Ran(a.finish()),
        };
        Ok(TripleResult {
            reference: ImplOutcomeV1::Ran(a1.finish()),
            independent: outcome(self.impls.ref2, e2, a2),
            backend: outcome(self.impls.exec, e3, a3),
            disagreement,
        })
    }
}
