//! The `palw-class pack` command line: `build`, `verify`, `show`.

use super::build::{BuildOpts, DeclareOpts, build};
use super::conformance::ImplSet;
use super::manifest::{PACK_FILE, RuntimePackV1};
use super::verify::{VerifyOpts, verify};
use crate::tir_layout::TirLayoutChoiceV1;
use misaka_palw_tir_lower::convert::{CalibInput, ConvertRequest};
use misaka_palw_tir_lower::detmath::MathMode;
use std::path::PathBuf;
use std::time::Instant;

pub const PACK_USAGE: &str = "palw-class pack — runtime packs: build, verify and show (misaka.palw.runtime-pack.v1)

USAGE:
    palw-class pack build  --model <hf dir | .gguf> --out <artifact.palwtir> --pack <dir>
                           (--calib <tokens.json> [--calib-seqs N] [--positions N] | --stats-in <stats.json>)
                           [--name <name>] [--repo <id>] [--revision <rev>] [--context N]
                           [--headroom16 F] [--headroom32 F] [--headroom-resid F] [--max-window N]
                           [--math libm-v1|std] [--quant-format <file>]... [--adapter <file>] [--tokenizer <file>]
                           [--hf-reference <audit dir | logits.json | hf-reference.json>]
                           [--slope-min F] [--slope-max F] [--corr-min F] [--top1-min F] [--kl-max F] [--allow-out-of-tolerance]
                           [--prompts N] [--prefill N] [--decode N] [--seed N] [--no-ref2] [--no-exec] [--stream|--no-stream]
                           [--declare <network>[:max-context=N][:tile=N][:h-chunk=N]]...
                           [--chunk-store <dir>] [--keep-chunks] [--block-mib N] [--defer-min-mib N]
    palw-class pack verify <pack dir> [--model <dir | .gguf>] [--artifact <file>] [--rebuild]
                           [--no-ref2] [--no-exec] [--no-declared] [--stream|--no-stream] [--strict] [--json]
    palw-class pack show   <pack dir> [--json]

`build` converts the model (the streaming converter, libm-v1 math by default), writes the artifact to --out and
the pack to --pack: the manifest `pack.json` and the sidecars it pins by hash (the calibration statistics, the
calibration tokens, the Hugging Face reference, a supplied adapter and quant descriptors). It runs the
conformance vectors on the reference evaluator, the typed backend and the independent implementation, holds the
program to the reference (units, order, KL) within the tolerance, and — per --declare — declares the class on a
network. `verify` checks every claim it can from what is at hand and says PASS, FAIL or SKIPPED for each; with
--model and --rebuild it rebuilds the artifact from the public source and the pack's profile and compares roots;
with --artifact it checks that file. VERIFIED only when nothing failed and nothing was skipped. Exit 0 verified
(or, without --strict, nothing failed), 2 failed, 1 an error.";

fn take_flag(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    if i + 1 >= args.len() {
        return None;
    }
    args.remove(i);
    Some(args.remove(i))
}

fn take_all(args: &mut Vec<String>, flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(v) = take_flag(args, flag) {
        out.push(v);
    }
    out
}

fn take_bool(args: &mut Vec<String>, flag: &str) -> bool {
    let had = args.iter().any(|a| a == flag);
    args.retain(|a| a != flag);
    had
}

fn num<T: std::str::FromStr>(args: &mut Vec<String>, flag: &str) -> Result<Option<T>, String>
where
    T::Err: std::fmt::Display,
{
    match take_flag(args, flag) {
        Some(v) => v.parse::<T>().map(Some).map_err(|e| format!("{flag} {v}: {e}")),
        None => Ok(None),
    }
}

/// `network[:max-context=N][:tile=N][:h-chunk=N]`.
fn parse_declare(s: &str) -> Result<DeclareOpts, String> {
    let mut parts = s.split(':');
    let network = parts.next().unwrap_or_default().to_string();
    let mut choice = TirLayoutChoiceV1::default();
    for p in parts {
        let (k, v) = p.split_once('=').ok_or_else(|| format!("--declare {s}: `{p}` is key=value"))?;
        let n: u32 = v.parse().map_err(|e| format!("--declare {s}: {k}={v}: {e}"))?;
        match k {
            "max-context" => choice.max_context = Some(n),
            "tile" => choice.tile_len = n,
            "h-chunk" => choice.h_chunk = n,
            "logits-tile" => choice.logits_tile = Some(n),
            other => return Err(format!("--declare {s}: unknown key `{other}` (max-context, tile, h-chunk, logits-tile)")),
        }
    }
    Ok(DeclareOpts { network, choice })
}

/// Run `palw-class pack …`; the exit code the process should end with.
pub fn run(args: &[String]) -> Result<i32, String> {
    let mut args = args.to_vec();
    let sub = if args.is_empty() { String::new() } else { args.remove(0) };
    let t0 = Instant::now();
    let log = |m: String| eprintln!("[{:>7.1}s] {m}", t0.elapsed().as_secs_f64());
    match sub.as_str() {
        "build" => {
            let model = PathBuf::from(take_flag(&mut args, "--model").ok_or(PACK_USAGE)?);
            let out = PathBuf::from(take_flag(&mut args, "--out").ok_or(PACK_USAGE)?);
            let pack_dir = PathBuf::from(take_flag(&mut args, "--pack").ok_or(PACK_USAGE)?);
            let mut req = ConvertRequest::new(&model, &out);
            req.stats_in = take_flag(&mut args, "--stats-in").map(PathBuf::from);
            if let Some(p) = take_flag(&mut args, "--calib") {
                req.calib = Some(CalibInput::from_file(std::path::Path::new(&p)).map_err(|e| e.to_string())?);
            }
            req.calib_seqs = num(&mut args, "--calib-seqs")?;
            req.positions = num(&mut args, "--positions")?;
            req.context = num(&mut args, "--context")?;
            if let Some(v) = num::<f64>(&mut args, "--headroom16")? {
                req.policy.headroom16 = v;
            }
            if let Some(v) = num::<f64>(&mut args, "--headroom32")? {
                req.policy.headroom32 = v;
            }
            if let Some(v) = num::<f64>(&mut args, "--headroom-resid")? {
                req.policy.headroom_resid = v;
            }
            req.max_window = num(&mut args, "--max-window")?;
            if let Some(m) = take_flag(&mut args, "--math") {
                req.math = MathMode::parse(&m).ok_or_else(|| format!("--math {m}: libm-v1 or std"))?;
            }
            req.quant_formats = take_all(&mut args, "--quant-format").into_iter().map(PathBuf::from).collect();
            req.adapter = take_flag(&mut args, "--adapter").map(PathBuf::from);
            req.tokenizer = take_flag(&mut args, "--tokenizer").map(PathBuf::from);
            req.chunk_store = take_flag(&mut args, "--chunk-store").map(PathBuf::from);
            req.keep_chunks = take_bool(&mut args, "--keep-chunks");
            if let Some(v) = num(&mut args, "--block-mib")? {
                req.block_mib = v;
            }
            if let Some(v) = num(&mut args, "--defer-min-mib")? {
                req.defer_min_mib = v;
            }
            let name = take_flag(&mut args, "--name").unwrap_or_else(|| model.file_name().and_then(|n| n.to_str()).unwrap_or("model").to_string());
            let mut o = BuildOpts::new(req, pack_dir, name);
            o.repo = take_flag(&mut args, "--repo");
            o.revision = take_flag(&mut args, "--revision");
            o.hf_reference = take_flag(&mut args, "--hf-reference").map(PathBuf::from);
            if let Some(v) = num(&mut args, "--slope-min")? {
                o.tolerance.slope_min = v;
            }
            if let Some(v) = num(&mut args, "--slope-max")? {
                o.tolerance.slope_max = v;
            }
            if let Some(v) = num(&mut args, "--corr-min")? {
                o.tolerance.corr_min = v;
            }
            if let Some(v) = num(&mut args, "--top1-min")? {
                o.tolerance.top1_min = v;
            }
            if let Some(v) = num(&mut args, "--kl-max")? {
                o.tolerance.kl_max = v;
            }
            o.allow_out_of_tolerance = take_bool(&mut args, "--allow-out-of-tolerance");
            if let Some(v) = num(&mut args, "--prompts")? {
                o.prompts = v;
            }
            if let Some(v) = num(&mut args, "--prefill")? {
                o.prefill = v;
            }
            if let Some(v) = num(&mut args, "--decode")? {
                o.decode = v;
            }
            if let Some(v) = num(&mut args, "--seed")? {
                o.seed = v;
            }
            o.impls = ImplSet { exec: !take_bool(&mut args, "--no-exec"), ref2: !take_bool(&mut args, "--no-ref2") };
            o.streamed = if take_bool(&mut args, "--stream") { Some(true) } else if take_bool(&mut args, "--no-stream") { Some(false) } else { None };
            for d in take_all(&mut args, "--declare") {
                o.declare.push(parse_declare(&d)?);
            }
            if let Some(extra) = args.first() {
                return Err(format!("unexpected argument `{extra}`\n{PACK_USAGE}"));
            }
            let b = build(&o, &log)?;
            println!("{}", b.digest);
            Ok(0)
        }
        "verify" => {
            let json = take_bool(&mut args, "--json");
            let strict = take_bool(&mut args, "--strict");
            let mut o = VerifyOpts::new(PathBuf::new());
            o.model = take_flag(&mut args, "--model").map(PathBuf::from);
            o.artifact = take_flag(&mut args, "--artifact").map(PathBuf::from);
            o.rebuild = take_bool(&mut args, "--rebuild");
            o.impls = ImplSet { exec: !take_bool(&mut args, "--no-exec"), ref2: !take_bool(&mut args, "--no-ref2") };
            o.streamed = if take_bool(&mut args, "--stream") { Some(true) } else if take_bool(&mut args, "--no-stream") { Some(false) } else { None };
            o.declared = !take_bool(&mut args, "--no-declared");
            o.pack_dir = PathBuf::from(args.first().ok_or(PACK_USAGE)?);
            let r = verify(&o, &log)?;
            if json {
                let checks: Vec<_> = r
                    .checks
                    .iter()
                    .map(|c| serde_json::json!({ "check": c.name, "status": format!("{:?}", c.status).to_uppercase(), "detail": c.detail }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "pack": r.pack_digest, "verified": r.verified(), "ok": r.ok(), "checks": checks })).unwrap_or_default());
            } else {
                print!("{}", r.render());
            }
            Ok(if !r.ok() || (strict && !r.verified()) { 2 } else { 0 })
        }
        "show" => {
            let json = take_bool(&mut args, "--json");
            let dir = PathBuf::from(args.first().ok_or(PACK_USAGE)?);
            let text = std::fs::read_to_string(dir.join(PACK_FILE)).map_err(|e| format!("{}: {e}", dir.join(PACK_FILE).display()))?;
            let p = RuntimePackV1::parse(&text)?;
            if json {
                print!("{}", p.to_pretty());
            } else {
                print!("{}", show(&p));
            }
            Ok(0)
        }
        _ => Err(PACK_USAGE.to_string()),
    }
}

/// The human summary of a pack.
pub fn show(p: &RuntimePackV1) -> String {
    let mut o = String::new();
    let s = |x: &str| x.chars().take(16).collect::<String>();
    o.push_str(&format!("pack         {} ({})\n", p.name, p.digest()));
    o.push_str(&format!("model        {} [{}], {} source file(s), {}\n", p.model.label, p.model.format, p.model.files.len(), p.model.architectures.join(", ")));
    o.push_str(&format!(
        "frontend     level {}, adapter {}{}, spec {}\n",
        p.frontend.level,
        p.frontend.adapter.kind,
        p.frontend.adapter.id.as_ref().map(|i| format!(" `{i}`")).unwrap_or_default(),
        s(&p.frontend.spec_digest)
    ));
    let scope = &p.features.scope;
    let left: Vec<String> = scope["excluded"].as_array().map(|a| a.iter().filter_map(|e| e["what"].as_str()).map(str::to_string).collect()).unwrap_or_default();
    o.push_str(&format!(
        "scope        {} → {}{}\n",
        scope["input"].as_str().unwrap_or("?"),
        scope["output"].as_str().unwrap_or("?"),
        if left.is_empty() { "; nothing left out".to_string() } else { format!("; NOT computed: {}", left.join(", ")) }
    ));
    o.push_str(&format!("features     {} in use\n", p.features.used.len()));
    o.push_str(&format!(
        "quant        {}\n",
        if p.quant.descriptors.is_empty() { "float checkpoint".to_string() } else { p.quant.descriptors.iter().map(|d| format!("{} {}", d.name, s(&d.digest))).collect::<Vec<_>>().join(", ") }
    ));
    o.push_str(&format!(
        "profile      headroom {}/{}/{}, calibration {} ({} sites), math {}{}\n",
        p.profile.policy.headroom16,
        p.profile.policy.headroom32,
        p.profile.policy.headroom_resid,
        s(&p.profile.calibration.stats_digest),
        p.profile.calibration.sites,
        p.converter.math.mode,
        p.converter.math.platform.as_ref().map(|x| format!(" ({x})")).unwrap_or_default()
    ));
    o.push_str(&format!("converter    {} {} / {}\n", p.converter.name, p.converter.crate_version, p.converter.lowering));
    o.push_str(&format!("executors    {}\n", p.executor.implementations.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join(", ")));
    o.push_str(&format!("logits       {} (scale {:e}); tolerance slope [{}, {}], corr ≥ {}, top-1 ≥ {}, KL ≤ {}\n", p.logits.convention, p.logits.scale, p.logits.tolerance.slope_min, p.logits.tolerance.slope_max, p.logits.tolerance.corr_min, p.logits.tolerance.top1_min, p.logits.tolerance.kl_max));
    o.push_str(&format!("artifact     {} bytes, file {}, inventory root {}, graph root {}\n", p.result.artifact_bytes, s(&p.result.artifact_digest), s(&p.result.inventory_root), s(&p.result.graph_ir_root)));
    o.push_str(&format!("conformance  {} vector(s), {} positions\n", p.conformance.vectors.len(), p.conformance.vectors.iter().map(|v| v.positions).sum::<usize>()));
    match &p.hf_reference {
        Some(h) => o.push_str(&format!("reference    {} positions over {} sequence(s): slope {:.4}, corr {:.5}, top-1 {:.3}, KL {:.5}\n", h.positions, h.sequences, h.measured.slope, h.measured.corr, h.measured.top1, h.measured.kl_mean)),
        None => o.push_str("reference    none\n"),
    }
    for d in &p.declared {
        o.push_str(&format!("declared     {}: class {} (context {}, interval {})\n", d.network, s(&d.class_id), d.max_context, d.checkpoint_interval));
    }
    o
}
