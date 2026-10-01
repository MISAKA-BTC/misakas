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

fn ref2_dtype(d: misaka_palw_tir::DType) -> misaka_palw_tir_ref2::DType {
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
            let d = c.program.params.get(e.param as usize).ok_or_else(|| format!("a tensor for param {}, which the program does not declare", e.param))?;
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
pub fn run(a: &LoadedArtifact, jobs: &[ConformanceJob], impls: ImplSet, progress: &dyn Fn(usize)) -> Result<Vec<ConformanceVector>, String> {
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
            let c1: Vec<Commit> = o1.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())).collect();
            if let (Some(p2), Some(params2), Some(st)) = (&p2, &params2, st2.as_mut()) {
                let (o2, next) = misaka_palw_tir_ref2::eval::step(p2, params2, &*st, tok as u64).map_err(|e| format!("{}: ref2 at {pos}: {e:?}", job.label))?;
                *st = next;
                let c2: Vec<Commit> = o2.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
                if o1.logits.data != o2.logits.data {
                    return Err(format!("{}: position {pos}: the logits differ between the reference evaluator and the independent implementation", job.label));
                }
                if c1 != c2 {
                    return Err(format!("{}: position {pos}: the reference evaluator and the independent implementation commit differently ({} vs {} commits)", job.label, c1.len(), c2.len()));
                }
            }
            if let Some(ex) = exec.as_mut() {
                let mut sink = Collect(Vec::new());
                ex.step(tok as u32, &mut sink).map_err(|e| format!("{}: exec at {pos}: {e}", job.label))?;
                let (_, xl) = ex.logits();
                let mut c3 = sink.0;
                c3.sort_by_key(|c| c.0);
                if o1.logits.data != xl.to_i128s() {
                    return Err(format!("{}: position {pos}: the logits differ between the reference evaluator and the typed backend", job.label));
                }
                if c1 != c3 {
                    return Err(format!("{}: position {pos}: the reference evaluator and the typed backend commit differently ({} vs {} commits)", job.label, c1.len(), c3.len()));
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
