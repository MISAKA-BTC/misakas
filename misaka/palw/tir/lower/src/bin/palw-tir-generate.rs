//! `palw-tir-generate` — greedy generation with a `PALWTIR1` artifact (a lowered artifact or a
//! declared class) on the typed backend (`misaka-palw-tir-exec`): each prompt's ids are fed one
//! position at a time, then the argmax of the logits (ties to the lowest id, the decode rule's
//! `base0_decode_token_select_v1`) is fed back until `--new-tokens` ids or the EOS id.
//!
//! Prompts are JSON `{"records": [{"input_ids": [...], ...}, ...]}` (e.g. an HF reference file
//! written by `hf_greedy.py`); with `--compare`, each record's `generated` is the expected output.
//! Prints each prompt's ids, and the prefill and decode throughput of this host.

use clap::Parser;
use misaka_palw_tir_artifact::PalwTirContainerV1;
use misaka_palw_tir_exec::{NoSink, ParamData, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_lower::lower::IntTensor;
use std::borrow::Cow;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "palw-tir-generate", about = "Greedy generation with a PALWTIR1 artifact on the typed backend")]
struct Args {
    /// A `PALWTIR1` container (`palw-tir-fidelity --artifact-out` or `palw-class declare-layout --out`).
    artifact: PathBuf,
    /// Prompt records (JSON `{"records": [{"input_ids": [...]}, ...]}`).
    #[arg(long)]
    prompts: Option<PathBuf>,
    /// Ids to generate per prompt at most.
    #[arg(long, default_value_t = 24)]
    new_tokens: usize,
    /// Stop after this id (it is kept as the last generated id).
    #[arg(long)]
    eos: Option<u32>,
    /// Compare with each record's `generated`.
    #[arg(long)]
    compare: bool,
    /// Write the results (JSON).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Teacher-forced instead: run these token sequences (JSON `{"sequences": [[id, …], …]}`) and
    /// write every position's logits, dequantized by the artifact's `logits_scale`, to
    /// `--logits-out` as little-endian `f32` `[positions, vocab]`.
    #[arg(long, requires = "logits_out")]
    sequences: Option<PathBuf>,
    #[arg(long)]
    logits_out: Option<PathBuf>,
}

fn argmax(v: &[i128]) -> u32 {
    let mut best = 0usize;
    for (i, x) in v.iter().enumerate() {
        if *x > v[best] {
            best = i;
        }
    }
    best as u32
}

fn run(a: &Args) -> Result<serde_json::Value, String> {
    let t0 = Instant::now();
    let c = PalwTirContainerV1::open(&a.artifact).map_err(|e| e.to_string())?;
    let program = c.program.clone();
    let plan = TirPlan::compile(&program).map_err(|e| e.to_string())?;
    let mut tensors: Vec<(u16, Option<u16>, IntTensor)> = Vec::new();
    for e in &c.header.tensors {
        let d = &program.params[e.param as usize];
        let bytes = c.read_tensor_bytes(e.param, e.layer).map_err(|x| x.to_string())?;
        let t = IntTensor::from_le_bytes(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), &bytes).map_err(|x| x.to_string())?;
        tensors.push((e.param, e.layer, t));
    }
    let mut xp = TirParams::new(&plan);
    for (j, layer, t) in &tensors {
        use misaka_palw_tir_lower::lower::IntData;
        let data = match &t.data {
            IntData::I8(v) => ParamData::I8(Cow::Borrowed(v)),
            IntData::I16(v) => ParamData::I16(Cow::Borrowed(v)),
            IntData::I32(v) => ParamData::I32(Cow::Borrowed(v)),
            IntData::I64(v) => ParamData::I64(Cow::Borrowed(v)),
            IntData::Idx(v) => ParamData::Idx(Cow::Borrowed(v)),
        };
        xp.insert(&plan, *j, *layer, data).map_err(|e| e.to_string())?;
    }
    eprintln!("[{:>6.1}s] loaded {} tensors", t0.elapsed().as_secs_f64(), tensors.len());
    if let (Some(sp), Some(lp)) = (&a.sequences, &a.logits_out) {
        let meta: serde_json::Value = serde_json::from_str(&c.header.meta).unwrap_or(serde_json::Value::Null);
        let scale = meta["logits_scale"].as_f64().ok_or("the artifact's provenance carries no logits_scale")?;
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(sp).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let seqs = v["sequences"].as_array().ok_or("sequences: no `sequences`")?;
        let mut bytes: Vec<u8> = Vec::new();
        let (mut n, t1) = (0usize, Instant::now());
        for s in seqs {
            let mut exec = TirExecutor::new(&plan, &xp).map_err(|e| e.to_string())?;
            for t in s.as_array().ok_or("a sequence is not an array")? {
                exec.step(t.as_u64().unwrap_or(0) as u32, &mut NoSink).map_err(|e| e.to_string())?;
                let (_, l) = exec.logits();
                for x in l.to_i128s() {
                    bytes.extend_from_slice(&((x as f64 * scale) as f32).to_le_bytes());
                }
                n += 1;
            }
        }
        std::fs::write(lp, &bytes).map_err(|e| e.to_string())?;
        eprintln!("[{:>6.1}s] {n} positions teacher-forced ({:.2}/s) -> {}", t0.elapsed().as_secs_f64(), n as f64 / t1.elapsed().as_secs_f64(), lp.display());
        return Ok(serde_json::json!({ "positions": n, "logits_scale": scale }));
    }
    let prompts = a.prompts.as_ref().ok_or("--prompts (or --sequences with --logits-out)")?;
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(prompts).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let recs = v["records"].as_array().ok_or("prompts: no `records`")?;
    let (mut prefill_n, mut prefill_s, mut decode_n, mut decode_s) = (0usize, 0f64, 0usize, 0f64);
    let mut out = Vec::new();
    let (mut same, mut total) = (0usize, 0usize);
    for r in recs {
        let ids: Vec<u32> = r["input_ids"].as_array().ok_or("a record without input_ids")?.iter().map(|t| t.as_u64().unwrap_or(0) as u32).collect();
        let mut exec = TirExecutor::new(&plan, &xp).map_err(|e| e.to_string())?;
        let tp = Instant::now();
        for t in &ids {
            exec.step(*t, &mut NoSink).map_err(|e| e.to_string())?;
        }
        prefill_n += ids.len();
        prefill_s += tp.elapsed().as_secs_f64();
        let td = Instant::now();
        let mut out_ids = Vec::new();
        loop {
            let (_, l) = exec.logits();
            let id = argmax(&l.to_i128s());
            out_ids.push(id);
            if out_ids.len() >= a.new_tokens || Some(id) == a.eos {
                break;
            }
            exec.step(id, &mut NoSink).map_err(|e| e.to_string())?;
            decode_n += 1;
        }
        decode_s += td.elapsed().as_secs_f64();
        let mut line = format!("{} prompt ids -> {out_ids:?}", ids.len());
        let mut rec = serde_json::json!({ "input_ids": ids, "generated": out_ids });
        if a.compare {
            let want: Vec<u32> = r["generated"].as_array().map(|g| g.iter().map(|t| t.as_u64().unwrap_or(0) as u32).collect()).unwrap_or_default();
            let prefix = out_ids.iter().zip(&want).take_while(|(x, y)| x == y).count();
            same += prefix;
            total += want.len();
            line += &format!(" vs HF {want:?}: {prefix}/{} identical", want.len());
            rec["hf"] = serde_json::json!(want);
            rec["identical_prefix"] = serde_json::json!(prefix);
        }
        eprintln!("[{:>6.1}s] {line}", t0.elapsed().as_secs_f64());
        out.push(rec);
    }
    let pf = prefill_n as f64 / prefill_s.max(1e-9);
    let dc = decode_n as f64 / decode_s.max(1e-9);
    eprintln!("prefill {prefill_n} positions at {pf:.2}/s, decode {decode_n} at {dc:.2}/s");
    Ok(serde_json::json!({
        "artifact": a.artifact.display().to_string(),
        "records": out,
        "identical": same,
        "total": total,
        "prefill_positions_per_s": pf,
        "decode_positions_per_s": dc,
    }))
}

fn main() {
    let a = Args::parse();
    match run(&a) {
        Ok(v) => {
            if let Some(p) = &a.out {
                if let Err(e) = std::fs::write(p, serde_json::to_vec_pretty(&v).unwrap_or_default()) {
                    eprintln!("palw-tir-generate: {}: {e}", p.display());
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("palw-tir-generate: {e}");
            std::process::exit(1);
        }
    }
}
