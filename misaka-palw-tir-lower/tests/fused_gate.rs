//! **RFC-0002 §7 F-4: the gate the fused kernels ship with** (`misaka_palw_tir_exec::fused`).
//!
//! A fused kernel ships only with a differential gate that fires. At every position this file runs
//! a program on four implementations:
//!
//! * the reference evaluator (`misaka-palw-tir`),
//! * the independent second implementation (`misaka-palw-tir-ref2`, fed the canonical bytes),
//! * the typed backend on its generic primitive kernels,
//! * the same backend with the fused kernels on.
//!
//! It requires the logits and every commit point to be equal, byte for byte, across all four, and
//! the run state (the `Fixed` states a fused kernel writes included) to be equal across the
//! reference and the two backends. The programs are:
//!
//! * **per kernel, a program whose one layer is the kernel's template**, fed from param tables by
//!   the token. It runs on random operands, on range-extreme ones (every dtype's minimum and
//!   maximum, shifts past both ends of their clamp, zero and full-scale gates), and at more than one
//!   layer and position for the recurrence. It checks that the kernel actually ran;
//! * **every HF tiny-fixture program the lowerer produces**, with its calibrated params — the
//!   domain the kernels run in on a node — reporting which kernels matched where;
//! * **the deliberately broken variant** (`TirExecutor::set_fused_fault`) of each kernel on its
//!   program, which moves one output lane by one: the gate must refuse it, or it gates nothing.

use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN, TirProgramV1};
use misaka_palw_tir::{DType, Interpreter, MapParams, ParamSource, Ref, RunState, Tensor, TensorType};
use misaka_palw_tir_exec::fused::kernel_index;
use misaka_palw_tir_exec::{NodeValue, ParamData, StepSink, TirExecutor, TirParams, TirPlan};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::path::Path;

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

fn ref2_dtype(d: DType) -> misaka_palw_tir_ref2::DType {
    use misaka_palw_tir_ref2::DType as B;
    match d {
        DType::I8 => B::I8,
        DType::I16 => B::I16,
        DType::I32 => B::I32,
        DType::I64 => B::I64,
        DType::I128 => B::I128,
        DType::Idx => B::Idx,
    }
}

/// What a run on the four implementations saw.
#[derive(Debug, Default)]
struct Seen {
    positions: usize,
    /// `(kernel, regions)` the fused backend ran per position.
    fused: Vec<(&'static str, usize)>,
}

/// **The four-way comparison** of `p` with `params` over each sequence of `seqs` — `Err` at the first
/// position where anything differs. With `fault`, the fused backend runs that kernel's deliberately
/// broken variant.
fn four_way(p: &TirProgramV1, params: &dyn ParamSource, seqs: &[Vec<u32>], fault: Option<usize>) -> Result<Seen, String> {
    // The params, once per implementation's own form.
    let plan = TirPlan::compile(p).map_err(|e| format!("exec plan: {e}"))?;
    let mut owned: Vec<((u16, Option<u16>), Vec<u8>)> = Vec::new();
    for &(j, layer) in &plan.param_instances {
        let t = params.param(j, layer).ok_or_else(|| format!("param {j} at {layer:?} is not bound"))?;
        owned.push(((j, layer), t.to_le_bytes()));
    }
    let bytes = p.encode();
    let p2 = misaka_palw_tir_ref2::codec::decode_canonical(&bytes).map_err(|e| format!("ref2 refuses the program: {e:?}"))?;
    let mut params2 = misaka_palw_tir_ref2::eval::Params::new();
    let mut xparams = TirParams::new(&plan);
    for ((j, layer), b) in &owned {
        let d = &p2.params[*j as usize];
        assert_eq!(d.dtype, ref2_dtype(p.params[*j as usize].dtype));
        let shape = d.shape.iter().map(|x| *x as u64).collect();
        let t2 = misaka_palw_tir_ref2::Tensor::from_le_bytes(d.dtype, shape, b).map_err(|e| format!("ref2 tensor: {e:?}"))?;
        params2.insert((*j, layer.map(u32::from)), t2);
        let data = ParamData::from_le_bytes(p.params[*j as usize].dtype, b).map_err(|e| e.to_string())?;
        xparams.insert(&plan, *j, *layer, data).map_err(|e| e.to_string())?;
    }
    let interp = Interpreter::new(p).map_err(|e| e.to_string())?;
    let mut seen = Seen::default();
    for seq in seqs {
        let mut st1 = RunState::default();
        let mut st2 = misaka_palw_tir_ref2::eval::initial_state(&p2);
        let mut generic = TirExecutor::new(&plan, &xparams).map_err(|e| e.to_string())?;
        let mut fused = TirExecutor::new(&plan, &xparams).map_err(|e| e.to_string())?;
        fused.set_fused(true);
        fused.set_fused_fault(fault);
        seen.fused = fused.fused_summary();
        for (pos, tok) in seq.iter().enumerate() {
            let o1 = interp.step(params, &mut st1, *tok).map_err(|e| format!("reference at {pos}: {e}"))?;
            let (o2, next) =
                misaka_palw_tir_ref2::eval::step(&p2, &params2, &st2, *tok as u64).map_err(|e| format!("ref2 at {pos}: {e:?}"))?;
            st2 = next;
            let mut runs = Vec::new();
            for exec in [&mut generic, &mut fused] {
                let mut sink = Collect(Vec::new());
                exec.step(*tok, &mut sink).map_err(|e| format!("exec at {pos}: {e}"))?;
                let mut commits = sink.0;
                commits.sort_by_key(|c| c.0);
                runs.push((exec.logits().1.to_i128s(), commits, exec.export_state()));
            }
            let c1: Vec<Commit> =
                o1.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(u32::from), c.node, c.value.data.clone())).collect();
            let c2: Vec<Commit> = o2.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.data.clone())).collect();
            if o1.logits.data != o2.logits.data || c1 != c2 {
                return Err(format!("position {pos}: the reference and ref2 differ"));
            }
            for (who, (logits, commits, state)) in ["generic", "fused"].iter().zip(&runs) {
                if *logits != o1.logits.data {
                    return Err(format!("position {pos}: the {who} backend's logits differ"));
                }
                if let Some((a, b)) = c1.iter().zip(commits).find(|(a, b)| a != b) {
                    return Err(format!(
                        "position {pos}: the {who} backend commits slot {} as {:?}, the reference {:?}",
                        b.0, b.4, a.4
                    ));
                }
                if c1.len() != commits.len() {
                    return Err(format!(
                        "position {pos}: the {who} backend commits {} values, the reference {}",
                        commits.len(),
                        c1.len()
                    ));
                }
                if *state != st1 {
                    return Err(format!("position {pos}: the {who} backend's run state differs"));
                }
            }
            seen.positions += 1;
        }
    }
    Ok(seen)
}

// ---- the kernels' own programs -----------------------------------------------------------------

/// Params drawn by `draw(param, layer, element)`.
fn params_for(p: &TirProgramV1, layers: u16, draw: &mut dyn FnMut(&str, usize) -> i128) -> MapParams {
    let mut m = MapParams { tensors: Default::default() };
    for (j, d) in p.params.iter().enumerate() {
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        let insts: Vec<Option<u16>> = if d.per_layer { (0..layers).map(Some).collect() } else { vec![None] };
        for l in insts {
            let data: Vec<i128> = (0..n).map(|i| draw(&d.name, i).clamp(d.dtype.min_value(), d.dtype.max_value())).collect();
            let shape = d.shape.iter().map(|x| *x as usize).collect();
            m.tensors.insert((j as u16, l), Tensor::new(d.dtype, shape, data).expect("in range"));
        }
    }
    m
}

/// A value of `dtype`: uniform, or (with probability `extreme`) one of the type's edges.
fn value(rng: &mut ChaCha20Rng, dtype: DType, extreme: f64) -> i128 {
    let (lo, hi) = (dtype.min_value().max(i64::MIN as i128), dtype.max_value().min(i64::MAX as i128));
    if rng.gen_bool(extreme) { [lo, lo + 1, -1, 0, 1, hi - 1, hi][rng.gen_range(0..7)] } else { rng.gen_range(lo..=hi) }
}

fn sequences(rng: &mut ChaCha20Rng, vocab: u32, count: usize, len: usize) -> Vec<Vec<u32>> {
    (0..count).map(|_| (0..len).map(|_| rng.gen_range(0..vocab)).collect()).collect()
}

const VOCAB: u32 = 16;

/// `gdn_step_q36` as every layer: the gated delta rule over a per-layer `Fixed` state, its operands
/// by token (the value row is the carry, so each layer reads the one before it).
fn gdn_program(h: u32, dv: u32, dk: u32, layers: usize) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(VOCAB, HISTORY_BOUND_V1_SMALL);
    let kt = pb.param("k.table", DType::I16, &[VOCAB, h * dk], false);
    let qt = pb.param("q.table", DType::I16, &[VOCAB, h * dk], false);
    let vt = pb.param("v.table", DType::I32, &[VOCAB, h * dv], false);
    let dect = pb.param("decay.table", DType::I32, &[VOCAB, h], false);
    let bett = pb.param("beta.table", DType::I32, &[VOCAB, h], false);
    let triple = |pb: &mut ProgramBuilder, name: &str| {
        (
            pb.param(&format!("{name}.m"), DType::I64, &[h], true),
            pb.param(&format!("{name}.s"), DType::I8, &[h], true),
            pb.param(&format!("{name}.z"), DType::I64, &[h], true),
        )
    };
    let read = triple(&mut pb, "read");
    let delta = triple(&mut pb, "delta");
    let out = triple(&mut pb, "out");
    let ws = pb.param("write_shift", DType::I32, &[h], true);
    let smax = i32::MAX as i64;
    let state = pb.fixed_state("S", DType::I32, &[h, dv, dk], -smax, smax, true);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let v = b.gather(vt, Ref::Input(INPUT_TOKEN), 0, 0);
        b.finish(&[v])
    };
    let layer = {
        let mut b = pb.block("layer", vec![TensorType::fixed(DType::I32, &[h * dv])]);
        let tok = Ref::Input(INPUT_TOKEN);
        let k = b.gather(kt, tok, 0, 0);
        let k = b.reshape_fixed(k, &[h, dk]);
        let q = b.gather(qt, tok, 0, 0);
        let q = b.reshape_fixed(q, &[h, dk]);
        let v = b.reshape_fixed(Ref::CarryIn(0), &[h, dv]);
        let decay = b.gather(dect, tok, 0, 0);
        let beta = b.gather(bett, tok, 0, 0);
        let narrowing = |b: &mut misaka_palw_tir::builder::BlockBuilder<'_>, (m, s, z): (Ref, Ref, Ref)| (m, b.pow2_of(s), z);
        let (r, d, o) = (narrowing(&mut b, read), narrowing(&mut b, delta), narrowing(&mut b, out));
        let y = b.gdn_step_q36(state, k, v, q, decay, beta, r, d, ws, o);
        let y = b.reshape_fixed(y, &[h * dv]);
        let y = b.commit(y);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[h * dv])]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[h * dv]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![layer; layers], post, 0)
}

/// Draw params the way a kernel's domain is: `plausible` magnitudes, or (`extreme`) the edges.
fn draw(rng: &mut ChaCha20Rng, extreme: f64) -> impl FnMut(&str, usize) -> i128 + '_ {
    move |name, _| {
        let edge = rng.gen_bool(extreme);
        match name.rsplit('.').next().unwrap_or(name) {
            // A narrowing's multiplier, shift and zero: plausible scales, or anything at all.
            "ez" if !edge => rng.gen_range(0i128..=1 << 20),
            "es" if !edge => rng.gen_range(0i128..=40),
            "ez" => value(rng, DType::I64, 1.0),
            "es" => value(rng, DType::I32, 1.0),
            "m" if !edge => rng.gen_range(1i128..=1 << 16),
            "s" if !edge => rng.gen_range(8i128..=30),
            "z" if !edge => rng.gen_range(-64i128..=64),
            "m" | "z" => value(rng, DType::I64, 1.0),
            "s" => value(rng, DType::I8, 1.0),
            // The recurrence's gates in [0, ONE], or past both ends.
            "table" if name.starts_with("decay") || name.starts_with("beta") => {
                if edge {
                    value(rng, DType::I32, 1.0)
                } else {
                    rng.gen_range(0i128..=1 << 24)
                }
            }
            // The router's kept weights in [0, 2^25].
            "table" if name.starts_with("w.") => rng.gen_range(0i128..=1 << 25),
            // Wide activations the kernel fuses over: sums of their squares stay in a machine word (the plan decides).
            "table" if name.starts_with("x32") => rng.gen_range(-(1i128 << 26)..=1 << 26),
            "write_shift" => rng.gen_range(-40i128..=40),
            _ => i128::MIN, // resolved by dtype below
        }
    }
}

/// A kernel's program over `cases` seeds, half of them range-extreme, and its broken variant.
fn gate(kernel: &str, p: &TirProgramV1, layers: u16, positions: usize) {
    let k = kernel_index(kernel).expect("a kernel of this build");
    let dtypes: std::collections::BTreeMap<String, DType> = p.params.iter().map(|d| (d.name.clone(), d.dtype)).collect();
    for seed in 0..8u64 {
        let extreme = if seed % 2 == 0 { 0.0 } else { 0.3 };
        let mut rng = ChaCha20Rng::seed_from_u64(0xF05E + seed);
        let mut raw_rng = ChaCha20Rng::seed_from_u64(0xD4A3 + seed);
        let mut raw = draw(&mut raw_rng, extreme);
        let params = params_for(p, layers, &mut |name, i| {
            let v = raw(name, i);
            if v == i128::MIN { value(&mut rng, dtypes[name], extreme) } else { v }
        });
        let seqs = sequences(&mut ChaCha20Rng::seed_from_u64(0x5E0 + seed), VOCAB, 2, positions);
        let seen = four_way(p, &params, &seqs, None).unwrap_or_else(|e| panic!("{kernel}, seed {seed}: {e}"));
        let ran = seen.fused.iter().find(|(n, _)| *n == kernel).map_or(0, |(_, c)| *c);
        assert!(ran > 0, "{kernel}, seed {seed}: the kernel never ran ({:?}), so this gated nothing", seen.fused);
        // The gate fires: the broken variant is refused on the same operands.
        let caught = four_way(p, &params, &seqs, Some(k)).expect_err("the broken variant must not pass the gate");
        assert!(caught.contains("fused"), "{kernel}, seed {seed}: refused, but not by the fused backend: {caught}");
    }
}

#[test]
fn gdn_step_is_the_reference_on_random_and_extreme_operands_and_its_broken_variant_is_caught() {
    gate("gdn_step_q36", &gdn_program(4, 8, 16, 2), 2, 6);
    gate("gdn_step_q36", &gdn_program(2, 4, 4, 1), 1, 6);
}

/// A one-layer program whose layer normalises the row its token selects: `l2_unit_q15` over `i16` codes, or `rms_unit_q24` over
/// `i16`/`i32` codes with an `i64` `eps` param. `rows × n` is the row shape; the table's rows are the operands.
fn rowop_program(l2: bool, dtype: DType, rows: u32, n: u32, layers: usize) -> TirProgramV1 {
    rowop_program_kind(if l2 { 0 } else { 1 }, dtype, rows, n, layers)
}

/// `kind`: 0 `l2_unit_q15`, 1 `rms_unit_q24`, 2 `rms_norm_wide_q36`, 3 `rms_norm_wide_q36_exact`.
fn rowop_program_kind(kind: u8, dtype: DType, rows: u32, n: u32, layers: usize) -> TirProgramV1 {
    let l2 = kind == 0;
    let mut pb = ProgramBuilder::new(VOCAB, HISTORY_BOUND_V1_SMALL);
    let table = pb.param(if dtype == DType::I32 { "x32.table" } else { "x.table" }, dtype, &[VOCAB, rows * n], false);
    let eps = (kind == 1).then(|| pb.param("eps", DType::I64, &[1], false));
    let wide = (kind >= 2).then(|| (pb.param("ez", DType::I64, &[1], false), pb.param("es", DType::I32, &[1], false)));
    let out_dtype = if l2 { DType::I16 } else { DType::I32 };
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let v = b.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
        let v = b.cast(v, out_dtype);
        b.finish(&[v])
    };
    let layer = {
        let mut b = pb.block("layer", vec![TensorType::fixed(out_dtype, &[rows * n])]);
        let x = b.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.reshape_fixed(x, &[rows, n]);
        let y = match kind {
            0 => b.l2_unit_q15(x),
            1 => b.rms_unit_q24(x, eps.expect("rms has an eps")),
            2 => {
                let (ez, es) = wide.expect("wide eps");
                b.rms_norm_wide_q36(x, ez, es)
            }
            _ => {
                let (ez, es) = wide.expect("wide eps");
                b.rms_norm_wide_q36_exact(x, ez, es)
            }
        };
        let y = b.reshape_fixed(y, &[rows * n]);
        let y = b.commit(y);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::fixed(out_dtype, &[rows * n])]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[rows * n]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![layer; layers], post, 0)
}

#[test]
fn the_unit_row_kernels_are_the_reference_on_random_and_extreme_operands_and_their_broken_variants_are_caught() {
    gate("l2_unit_q15", &rowop_program(true, DType::I16, 3, 8, 2), 2, 6);
    gate("l2_unit_q15", &rowop_program(true, DType::I16, 1, 16, 1), 1, 6);
    gate("rms_unit_q24", &rowop_program(false, DType::I16, 3, 8, 2), 2, 6);
    gate("rms_unit_q24", &rowop_program(false, DType::I32, 2, 16, 1), 1, 6);
    for kind in [2u8, 3] {
        gate("rms_norm_wide_q36", &rowop_program_kind(kind, DType::I16, 3, 8, 2), 2, 6);
        gate("rms_norm_wide_q36", &rowop_program_kind(kind, DType::I32, 2, 16, 1), 1, 6);
    }
}

// ---- every HF tiny-fixture program -------------------------------------------------------------

/// The lowered program of fixture `name` with its calibrated params, as `three_way.rs` prepares it.
fn lowered(name: &str) -> Result<(TirProgramV1, misaka_palw_tir_lower::lower::IntParams, usize, usize), String> {
    use misaka_palw_tir_lower::fidelity;
    use misaka_palw_tir_lower::float_ref::ParamStore;
    use misaka_palw_tir_lower::float_ref::stream::Resident;
    use misaka_palw_tir_lower::lower::{LowerOpts, materialise};
    use misaka_palw_tir_lower::quant::QuantPolicy;
    use misaka_palw_tir_lower::weights::Checkpoint;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name);
    let cfg = std::fs::read_to_string(dir.join("config.json")).map_err(|e| e.to_string())?;
    let prep = fidelity::prepare(&cfg, &LowerOpts::default()).map_err(|e| format!("prepare: {e}"))?;
    let ck = Checkpoint::open(&dir).map_err(|e| e.to_string())?;
    let (params, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(|e| e.to_string())?;
    let loader = Resident(std::sync::Arc::new(params));
    let max_len = prep.spec.embedding.positions.as_ref().map_or(usize::MAX, |p| p.rows - p.offset);
    let calib = fidelity::random_sequences(prep.hl.vocab, 4, 24.min(max_len), 11);
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, &calib, &quiet).map_err(|e| format!("calibrate: {e}"))?;
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet)
        .map_err(|e| format!("materialise: {e}"))?;
    Ok((prep.lowered.program.clone(), mat.params, prep.hl.vocab, max_len))
}

#[test]
fn every_hf_tiny_fixture_program_is_the_same_with_the_fused_kernels_on() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut names: Vec<String> =
        std::fs::read_dir(&root).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    let mut failed = Vec::new();
    let mut matched: std::collections::BTreeMap<&'static str, Vec<String>> = Default::default();
    for n in &names {
        if !root.join(n).join("model.safetensors").exists() {
            continue;
        }
        let (p, params, vocab, max_len) = match lowered(n) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("{n:>22}: not prepared ({e})");
                continue;
            }
        };
        let seqs: Vec<Vec<u32>> = misaka_palw_tir_lower::fidelity::random_sequences(vocab, 2, 12.min(max_len), 97)
            .into_iter()
            .map(|s| s.into_iter().map(|t| t as u32).collect())
            .collect();
        match four_way(&p, &params, &seqs, None) {
            Ok(seen) => {
                let ran: Vec<String> = seen.fused.iter().filter(|(_, c)| *c > 0).map(|(k, c)| format!("{k}×{c}")).collect();
                for (k, c) in &seen.fused {
                    if *c > 0 {
                        matched.entry(k).or_default().push(n.clone());
                    }
                }
                eprintln!(
                    "{n:>22}: {} positions equal on all four; fused: {}",
                    seen.positions,
                    if ran.is_empty() { "none".into() } else { ran.join(", ") }
                );
            }
            Err(e) => failed.push(format!("{n}: {e}")),
        }
    }
    for (k, programs) in &matched {
        eprintln!("{k}: matched in {} program(s): {}", programs.len(), programs.join(", "));
    }
    assert!(failed.is_empty(), "fixtures where the four implementations differ:\n{}", failed.join("\n"));
    assert!(
        matched.get("gdn_step_q36").is_some_and(|v| !v.is_empty()),
        "gdn_step_q36 matched no lowered program: the lowerer's use of it went unmatched"
    );
}

/// **The tiny hybrids, timed** (RFC-0002 Phase G's benchmark table): the lowered Qwen3.5-MoE and
/// Qwen3-Next fixture programs (gated delta + MoE + attention layers), with their calibrated params,
/// stepped on the generic backend and with the fused kernels on — every logit compared — and the
/// median time per position printed. A measurement, not a check: run it in release,
/// `cargo test --release -p misaka-palw-tir-lower --test fused_gate -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench_the_tiny_hybrids_generic_and_fused() {
    use std::time::Instant;
    for name in ["qwen3_5_moe", "qwen3_next"] {
        let (p, params, vocab, max_len) = lowered(name).expect("the fixture lowers");
        let plan = TirPlan::compile(&p).expect("the plan");
        let owned: Vec<((u16, Option<u16>), Vec<u8>)> =
            plan.param_instances.iter().map(|&(j, l)| ((j, l), params.param(j, l).expect("bound").to_le_bytes())).collect();
        let mut xparams = TirParams::new(&plan);
        for ((j, layer), b) in &owned {
            let data = ParamData::from_le_bytes(p.params[*j as usize].dtype, b).expect("whole elements");
            xparams.insert(&plan, *j, *layer, data).expect("a param of its declaration");
        }
        let seq: Vec<u32> =
            misaka_palw_tir_lower::fidelity::random_sequences(vocab, 1, 64.min(max_len), 5)[0].iter().map(|t| *t as u32).collect();
        let mut logits: Vec<Vec<Vec<i128>>> = Vec::new();
        let mut line = format!("{name:>12}: {} positions;", seq.len());
        for fused in [false, true] {
            let mut exec = TirExecutor::new(&plan, &xparams).expect("an executor");
            exec.set_fused(fused);
            let mut times = Vec::with_capacity(seq.len());
            let mut out = Vec::with_capacity(seq.len());
            for tok in &seq {
                let s = Instant::now();
                exec.step(*tok, &mut misaka_palw_tir_exec::NoSink).expect("a step");
                times.push(s.elapsed().as_secs_f64());
                out.push(exec.logits().1.to_i128s());
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            line += &format!(
                " {} {:.1} µs/position (min {:.1}){}",
                if fused { "fused" } else { "generic" },
                times[times.len() / 2] * 1e6,
                times[0] * 1e6,
                if fused { format!(" {:?}", exec.fused_summary()) } else { ";".into() }
            );
            logits.push(out);
        }
        assert_eq!(logits[0], logits[1], "{name}: the fused backend's logits differ");
        eprintln!("{line}");
    }
}

/// **The measurement behind `ADMISSIBLE_GENERIC`'s coefficient** (`misaka-palw-sdk` `GENERIC_WIDE_PASS_FACTOR_V1`): the unit-row programs at
/// serving-like shapes, stepped on the generic backend and with the fused kernels on, every logit compared, the median time per position
/// printed with the ratio. A measurement, not a check: `cargo test --release -p misaka-palw-tir-lower --test fused_gate measure_ -- --ignored --nocapture`.
#[test]
#[ignore]
fn measure_the_generic_over_fused_ratio_of_the_wide_patterns() {
    use std::time::Instant;
    let cases: [(&str, u8, DType, u32, u32); 6] = [
        ("l2_unit_q15 32x128", 0, DType::I16, 32, 128),
        ("l2_unit_q15 8x2048", 0, DType::I16, 8, 2048),
        ("rms_unit_q24 1x4096", 1, DType::I16, 1, 4096),
        ("rms_unit_q24 16x512", 1, DType::I16, 16, 512),
        ("rms_norm_wide_q36 1x4096", 2, DType::I16, 1, 4096),
        ("rms_norm_wide_q36 8x1024", 2, DType::I16, 8, 1024),
    ];
    for (label, kind, dtype, rows, n) in cases {
        let p = rowop_program_kind(kind, dtype, rows, n, 8);
        let mut rng = ChaCha20Rng::seed_from_u64(77);
        let mut raw_rng = ChaCha20Rng::seed_from_u64(78);
        let mut raw = draw(&mut raw_rng, 0.0);
        let dtypes: std::collections::BTreeMap<String, DType> = p.params.iter().map(|d| (d.name.clone(), d.dtype)).collect();
        let params = params_for(&p, 8, &mut |name, i| {
            let v = raw(name, i);
            if v == i128::MIN { value(&mut rng, dtypes[name], 0.0) } else { v }
        });
        let plan = TirPlan::compile(&p).expect("plan");
        let owned: Vec<((u16, Option<u16>), Vec<u8>)> =
            plan.param_instances.iter().map(|&(j, l)| ((j, l), params.param(j, l).expect("bound").to_le_bytes())).collect();
        let mut xparams = TirParams::new(&plan);
        for ((j, layer), b) in &owned {
            let data = ParamData::from_le_bytes(p.params[*j as usize].dtype, b).expect("whole elements");
            xparams.insert(&plan, *j, *layer, data).expect("a param");
        }
        let seq: Vec<u32> = (0..64).map(|i| (i * 5 + 1) % VOCAB).collect();
        let mut med = [0f64; 2];
        let mut logits: Vec<Vec<Vec<i128>>> = Vec::new();
        for (k, fused) in [false, true].into_iter().enumerate() {
            let mut exec = TirExecutor::new(&plan, &xparams).expect("an executor");
            exec.set_fused(fused);
            let mut times = Vec::new();
            let mut out = Vec::new();
            for tok in &seq {
                let s = Instant::now();
                exec.step(*tok, &mut misaka_palw_tir_exec::NoSink).expect("a step");
                times.push(s.elapsed().as_secs_f64());
                out.push(exec.logits().1.to_i128s());
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            med[k] = times[times.len() / 2];
            logits.push(out);
        }
        assert_eq!(logits[0], logits[1], "{label}: the fused logits differ");
        eprintln!("{label:>28}: generic {:>9.1} us  fused {:>9.1} us  ratio {:.2}x", med[0] * 1e6, med[1] * 1e6, med[0] / med[1]);
    }
}
