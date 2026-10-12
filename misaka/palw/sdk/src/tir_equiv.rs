//! **D-F1 offline: a legacy dense row and its PALW-TIR program — same weights, same logits, same
//! rows** (`palw-tir-equiv`; RFC-0002 Phase F, drill D-F1, `phase-f-integration.md` §4).
//!
//! The legacy side is the dense A16 tier as a producer runs it: [`A16Engine::forward_token_planned`]
//! over the plan compiled from the row's registered profile (testnet-12's `graph-v7@8192`: two
//! pre rows, twenty-four a layer with the fused attention site, three post rows). The IR side is
//! the A16 mirror program (`misaka_palw_base0::tir_a16`), which commits every one of those rows,
//! evaluated by the typed backend (`misaka-palw-tir-exec`, byte-identical to the reference
//! evaluator) over the F3 conversion of the same artifact.
//!
//! For every job — the canonical job and a set of random prompts — and every position, the check
//! is exact: the IR logits row equals the legacy logits row byte for byte, and every IR commit
//! point that is a legacy node row equals that row. Decoding picks the consensus decode token
//! (`base0_decode_token_select_v1`) of the legacy row, so both sides read the same tokens.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1;
use misaka_palw_base0::artifact::Base0ArtifactV1;
use misaka_palw_base0::engine_a16::{A16Cache, A16Engine, A16TraceV1};
use misaka_palw_base0::qwen25_a16_backend::qwen25_a16_prompt_for_anchor;
use misaka_palw_base0::tir_a16::{A16_MIRROR_LAYER_BLOCK, A16_MIRROR_POST_BLOCK, A16_MIRROR_PRE_BLOCK, A16MirrorRowsV1};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_exec::{NodeValue, ParamData, Slice, StepSink, TirExecutor, TirParams, TirPlan};
use rand::{Rng, SeedableRng};
use std::collections::BTreeMap;
use std::time::Instant;

/// One job: a prompt, then `decode` generated tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquivJobV1 {
    pub label: String,
    pub prompt: Vec<usize>,
    pub decode: usize,
}

/// **The jobs D-F1 names**: the canonical job — `prefill` tokens of the prompt the backend derives
/// for `anchor`, then `decode` — and `prompts` random prompts (seeded; lengths uniform in
/// `1..=prefill`, tokens uniform over the vocabulary), each decoding `decode` tokens.
pub fn equiv_jobs_v1(prefill: usize, decode: usize, vocab: usize, anchor: Hash64, prompts: usize, seed: u64) -> Vec<EquivJobV1> {
    let mut jobs = vec![EquivJobV1 {
        label: format!("canonical ({prefill}+{decode}, anchor {})", &anchor.to_string()[..16]),
        prompt: qwen25_a16_prompt_for_anchor(anchor, vocab, prefill as u32),
        decode,
    }];
    let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(seed);
    for i in 0..prompts {
        let len = rng.gen_range(1..=prefill.max(1));
        let prompt = (0..len).map(|_| rng.gen_range(0..vocab)).collect();
        jobs.push(EquivJobV1 { label: format!("random {i} ({len}+{decode})"), prompt, decode });
    }
    jobs
}

/// What a run found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EquivReportV1 {
    pub jobs: usize,
    pub positions: u64,
    /// Logits rows compared (one a position), and how many were equal.
    pub logit_rows: u64,
    pub logit_rows_equal: u64,
    /// Legacy node rows compared against their IR commit point, and how many were equal.
    pub node_rows: u64,
    pub node_rows_equal: u64,
    /// IR commit points a position that are not a legacy row (the fused site's logits and
    /// probability codes, which the legacy row keeps inside its kernel).
    pub commits_without_row: u64,
    /// The first mismatches, described.
    pub first_mismatches: Vec<String>,
    pub legacy_seconds: f64,
    pub ir_seconds: f64,
}

impl EquivReportV1 {
    pub fn is_equal(&self) -> bool {
        self.positions > 0 && self.logit_rows == self.logit_rows_equal && self.node_rows == self.node_rows_equal
    }
}

const KEEP_MISMATCHES: usize = 8;

/// The first index where an IR value and a legacy row differ (or their lengths do).
fn first_diff(ir: &Slice<'_>, row: &[i32]) -> Option<(usize, String, i32)> {
    fn cmp<T: Copy + Into<i128> + std::fmt::Display>(a: &[T], row: &[i32]) -> Option<(usize, String, i32)> {
        if a.len() != row.len() {
            return Some((a.len().min(row.len()), format!("length {}", a.len()), row.len() as i32));
        }
        a.iter().zip(row).position(|(x, y)| (*x).into() != *y as i128).map(|i| (i, a[i].to_string(), row[i]))
    }
    match ir {
        Slice::I8(a) => cmp(a, row),
        Slice::I16(a) => cmp(a, row),
        Slice::I32(a) => cmp(a, row),
        Slice::I64(a) => cmp(a, row),
        Slice::I128(a) => cmp(a, row),
        Slice::Idx(a) => cmp(a, row),
    }
}

/// Receives the IR step's commit points and compares each legacy row against the trace.
struct RowSink<'t> {
    trace: &'t A16TraceV1,
    pre: &'t BTreeMap<u16, usize>,
    layer: &'t BTreeMap<u16, usize>,
    post: &'t BTreeMap<u16, usize>,
    compared: u64,
    equal: u64,
    without_row: u64,
    bad: Vec<String>,
}

impl StepSink for RowSink<'_> {
    fn node(&mut self, v: &NodeValue<'_>) {
        if !v.commit {
            return;
        }
        let (table, rows, what): (&BTreeMap<u16, usize>, Option<&Vec<Vec<i32>>>, String) = match v.block {
            A16_MIRROR_PRE_BLOCK => (self.pre, Some(&self.trace.pre), "pre".into()),
            A16_MIRROR_LAYER_BLOCK => {
                let li = v.layer.unwrap_or(0) as usize;
                (self.layer, self.trace.attn.get(li), format!("layer {li}"))
            }
            A16_MIRROR_POST_BLOCK => (self.post, Some(&self.trace.post), "post".into()),
            _ => {
                self.without_row += 1;
                return;
            }
        };
        let Some(&i) = table.get(&v.node) else {
            self.without_row += 1;
            return;
        };
        self.compared += 1;
        match rows.and_then(|r| r.get(i)) {
            None => self.bad.push(format!("{what} row {i}: the legacy trace has no such row")),
            Some(row) => match first_diff(&v.data, row) {
                None => self.equal += 1,
                Some((at, ir, legacy)) => {
                    self.bad.push(format!("{what} row {i} (IR node {}): lane {at}: IR {ir}, legacy {legacy}", v.node))
                }
            },
        }
    }
}

/// **Run D-F1 over `jobs`.** `tensors` are the program's params (`a16_tir_tensor_bytes` of the
/// same artifact, or a PALWTIR1 container's). `progress(job, report_so_far)` is called after
/// every job.
#[allow(clippy::too_many_arguments)]
pub fn run_equiv_v1(
    artifact: &Base0ArtifactV1,
    profile: &PalwShapeProfileV3,
    program: &TirProgramV1,
    rows: &A16MirrorRowsV1,
    tensors: &BTreeMap<(u16, Option<u16>), Vec<u8>>,
    jobs: &[EquivJobV1],
    progress: &mut dyn FnMut(usize, &EquivReportV1),
) -> Result<EquivReportV1, String> {
    let engine = A16Engine::new(artifact).map_err(|e| format!("the legacy engine: {e:?}"))?;
    let plan = engine.plan_from_profile(profile).map_err(|e| format!("the row's profile is not servable here: {e:?}"))?;
    let tir_plan = TirPlan::compile(program).map_err(|e| format!("the IR program: {e}"))?;
    let mut params = TirParams::new(&tir_plan);
    for ((j, layer), bytes) in tensors {
        let d =
            program.params.get(*j as usize).ok_or_else(|| format!("a tensor for param {j}, which the program does not declare"))?;
        let data = ParamData::from_le_bytes(d.dtype, bytes).map_err(|e| format!("param `{}`: {e}", d.name))?;
        params.insert(&tir_plan, *j, *layer, data).map_err(|e| format!("param `{}`: {e}", d.name))?;
    }
    params.check_complete(&tir_plan).map_err(|e| format!("the IR params: {e}"))?;
    let index = |v: &[u16]| v.iter().enumerate().map(|(i, n)| (*n, i)).collect::<BTreeMap<u16, usize>>();
    let (pre, layer, post) = (index(&rows.pre), index(&rows.layer), index(&rows.post));
    let mut report = EquivReportV1::default();
    for (ji, job) in jobs.iter().enumerate() {
        let mut cache = A16Cache::new(artifact.shape.n_layers);
        let mut exec = TirExecutor::new(&tir_plan, &params).map_err(|e| format!("the IR executor: {e}"))?;
        let total = job.prompt.len() + job.decode;
        let mut token = *job.prompt.first().ok_or("an empty prompt")?;
        for pos in 0..total {
            let t = Instant::now();
            let (logits, trace) = engine
                .forward_token_planned(&plan, &mut cache, token, pos)
                .map_err(|e| format!("{}: legacy at {pos}: {e:?}", job.label))?;
            report.legacy_seconds += t.elapsed().as_secs_f64();
            let mut sink = RowSink {
                trace: &trace,
                pre: &pre,
                layer: &layer,
                post: &post,
                compared: 0,
                equal: 0,
                without_row: 0,
                bad: Vec::new(),
            };
            let t = Instant::now();
            exec.step(token as u32, &mut sink).map_err(|e| format!("{}: IR at {pos}: {e}", job.label))?;
            report.ir_seconds += t.elapsed().as_secs_f64();
            let expected = trace.pre.len() + trace.attn.iter().map(|l| l.len()).sum::<usize>() + trace.post.len();
            report.positions += 1;
            report.node_rows += expected as u64;
            report.node_rows_equal += sink.equal;
            report.commits_without_row = sink.without_row;
            if sink.compared != expected as u64 {
                sink.bad.push(format!("{} legacy rows, {} IR commit points matched to one", expected, sink.compared));
            }
            report.logit_rows += 1;
            let (_, ir_logits) = exec.logits();
            match first_diff(&ir_logits, &logits) {
                None => report.logit_rows_equal += 1,
                Some((at, ir, legacy)) => sink.bad.push(format!("logits lane {at}: IR {ir}, legacy {legacy}")),
            }
            for b in sink.bad {
                if report.first_mismatches.len() < KEEP_MISMATCHES {
                    report.first_mismatches.push(format!("{} position {pos}: {b}", job.label));
                }
            }
            // The next token: the prompt's, then the consensus decode token of this row.
            token = if pos + 1 < job.prompt.len() { job.prompt[pos + 1] } else { base0_decode_token_select_v1(&logits) };
        }
        report.jobs += 1;
        progress(ji, &report);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v7};
    use misaka_palw_base0::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::derived_a16_store;
    use misaka_palw_base0::tir_a16::{a16_mirror_program_with_rows, a16_tir_tensor_bytes};
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;

    fn small() -> (Base0ArtifactV1, PalwShapeProfileV3) {
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 48,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let a = Base0ArtifactV1::derive_deterministic(shape, 0x5A16)
            .expect("shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("store");
        let g = PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 32,
            ffn_dim: 48,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 8,
            vocab_size: 64,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        (a, qwen25_a16_profile_v7(g).expect("the v7 profile"))
    }

    #[test]
    fn the_mirror_commits_every_graph_v7_row_and_equals_the_legacy_engine_row_for_row() {
        let (a, profile) = small();
        let (program, rows) = a16_mirror_program_with_rows(&a.shape, HISTORY_BOUND_V1_SMALL).expect("program");
        assert_eq!(
            (rows.pre.len(), rows.layer.len(), rows.post.len()),
            (profile.pre_nodes.len(), profile.attn_nodes.len(), profile.post_nodes.len())
        );
        let tensors = a16_tir_tensor_bytes(&a, &program).expect("converted");
        let jobs = equiv_jobs_v1(13, 2, a.shape.vocab, Hash64::default(), 3, 7);
        let r = run_equiv_v1(&a, &profile, &program, &rows, &tensors, &jobs, &mut |_, _| {}).expect("ran");
        assert!(r.is_equal(), "{:#?}", r.first_mismatches);
        assert_eq!(r.jobs, 4);
        assert_eq!(r.positions, jobs.iter().map(|j| (j.prompt.len() + j.decode) as u64).sum::<u64>());
        assert_eq!(r.node_rows, r.positions * (2 + 2 * 24 + 3));
        // The fused site's logits and probability codes, per layer, are the IR's extra commits.
        assert_eq!(r.commits_without_row, 2 * 2);
    }

    #[test]
    fn a_changed_param_is_found_at_its_row() {
        let (a, profile) = small();
        let (program, rows) = a16_mirror_program_with_rows(&a.shape, HISTORY_BOUND_V1_SMALL).expect("program");
        let mut tensors = a16_tir_tensor_bytes(&a, &program).expect("converted");
        // Layer 1's `ffn_up` multiplier, lane 0: the up projection row (layer row 15) moves first.
        let j = program.param_index("blk.ffn_up.weight.a16.m").expect("the up multiplier");
        let m = tensors.get_mut(&(j, Some(1))).expect("layer 1");
        m[3] ^= 0x10;
        let jobs = equiv_jobs_v1(6, 1, a.shape.vocab, Hash64::default(), 0, 1);
        let r = run_equiv_v1(&a, &profile, &program, &rows, &tensors, &jobs, &mut |_, _| {}).expect("ran");
        assert!(!r.is_equal());
        assert!(r.first_mismatches[0].contains("layer 1 row 15"), "{:#?}", r.first_mismatches);
    }
}
