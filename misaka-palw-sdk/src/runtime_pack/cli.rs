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
                           [--declare <network>[:max-context=N][:tile=N][:h-chunk=N][:logits-tile=N]]...
                           [--chunk-store <dir>] [--keep-chunks] [--block-mib N] [--defer-min-mib N]
    palw-class pack verify <pack dir> [--model <dir | .gguf>] [--artifact <file>] [--rebuild]
                           [--no-ref2] [--no-exec] [--no-declared] [--stream|--no-stream] [--strict] [--json]
    palw-class pack show   <pack dir> [--json]
    palw-class pack build-frontend --model <safetensors dir | GGUF file/set/index> --frontend-pack <file> --out <artifact> --pack <dir>
                           --vectors <tokens.json> [--decode N] [--revision <rev>] [--block-bytes N]
    palw-class pack verify-frontend --model <safetensors dir | GGUF file/set/index> --artifact <file> --pack <dir> --rebuild-out <file>
                           [--block-bytes N]
    palw-class pack attach-frontend-fidelity --pack <existing dir> --artifact <base artifact>
                           --hf-reference <reference> --fidelity-policy <policy.json> --out <new dir>
    palw-class pack bind-class --pack <existing dir> --artifact <declared.palwtir> --network <network> --out <new dir>
    palw-class pack index  --artifact <file.palwtir> [--out <file>] [--root <hex128>]
    palw-class pack commit-conformance --pack <dir> --artifact <declared class file> --state <dir> --network <net>
                           --chain-genesis <hex128|label:x> --ruleset-id <hex128|label:x> [--class-id <prefix>] [--candidate-id <hex128>]
                           [--k N] [--delay N] [--window N] [--depth N] [--repetitions N] [--security-bits N] [--retry-limit N]
                           [--vectors N] [--prompt-len N] [--decode N] [--leaves N] [--vector-fault-ppm N] [--leaf-fault-ppm N]
                           [--no-require-independent] [--no-require-backend] [--plan-positions N]
    palw-class pack run-conformance --pack <dir> --artifact <file> --state <dir> --commitment <statement root prefix> --facts <facts.json>
                           [--no-ref2] [--no-exec] [--max-checks N]
    palw-class pack verify-conformance --pack <dir> --artifact <file> --state <dir> --commitment <prefix> --facts <facts.json>
                           --evidence <evidence.borsh> [--no-rerun]
    palw-class pack synthetic-facts --state <dir> --commitment <prefix> --out <facts.json> [--position N] [--works N] [--tip N] [--label L]
    palw-class pack conformance-status --state <dir>

`build` converts the model (the streaming converter, libm-v1 math by default), writes the artifact to --out and
the pack to --pack: the manifest `pack.json` and the sidecars it pins by hash (the calibration statistics, the
calibration tokens, the Hugging Face reference, a supplied adapter and quant descriptors). It runs the
conformance vectors on the reference evaluator, the typed backend and the independent implementation, holds the
program to the reference (units, order, KL) within the tolerance, and — per --declare — declares the class on a
network. `verify` checks every claim it can from what is at hand and says PASS, FAIL or SKIPPED for each; with
--model and --rebuild it rebuilds the artifact from the public source and the pack's profile and compares roots;
with --artifact it checks that file. VERIFIED only when nothing failed and nothing was skipped. Exit 0 verified
(or, without --strict, nothing failed), 2 failed, 1 an error.
`attach-frontend-fidelity` accepts an independent frontend companion, its base artifact, public HF reference
sidecars and a strict versioned fidelity policy. The policy's revision, task, context, logit units and thresholds
are validated BEFORE measurement; failed coverage or tolerance writes no output pack. The new companion pins
policy/reference bytes and portable libm-v1 metrics; `verify-frontend` independently repeats the fit before
publishing its rebuild. This is a reference-logit gate, not a source-equivalence certificate, routing/state
fidelity, task quality or live Final. The reference provider's label is informational.
`bind-class` supports either manifest format and pins an existing declared artifact's exact layout into a NEW pack without recalibration or admission search.
It changes the pack digest, not the class/artifact identity. It does not certify admission, readiness or mining;
run `pack verify` and live `misaka model preflight` separately. Old packs remain untouched.
`index` reads the artifact once and stores the Merkle index (one 64-byte hash per 32 KiB leaf) beside it, `<artifact>.merkleidx` (RFC-0013
§7.2); with --root it refuses to write an index that does not fold to that root. It is a cache, never an authority: `run-conformance`
uses a sidecar index only when it folds to the committed artifact root (opening the drawn leaves then reads those leaves only, not the
artifact) and otherwise makes the streamed pass it always made.

Beacon conformance (RFC-0013 §9; the policy is an UNAPPROVED test policy; nothing here is consensus):
`commit-conformance` binds the pack into a ConformanceCommitmentV1 (artifact/program/tokenizer/exact-layout roots re-derived from the
artifact, the VerificationPlan root of the kernel that would run it — judged hypothetically armed —, the policy, the calibration
(typed absence for independent integer-import recipes), the implementation set and the sampled-check scope) and refuses before any randomness if static admission fails or the scope cannot
meet the policy. `run-conformance` takes v1 file facts (misaka.palw.beacon-facts.v1; this runner has no RPC adapter and is separate
from the chain's attributed/sealed-source policies — `synthetic-facts` writes SYNTHETIC ones), derives the beacon and the challenge with the
shared contract only, runs the selected checks and writes BeaconConformanceEvidenceV1. `verify-conformance` recomputes everything in
a fresh process and re-executes the checks. Exit: run 0 passed, 2 evidence written but not a pass, 3 pending (WaitingRandomness /
BEACON_UNAVAILABLE / interrupted — resume with the same command), 1 refused; verify 0 PASS, 2 FAIL or not a pass, 3 pending or
results not re-executed. A sampled pass is never full-scope fidelity, never semantic admission, never a claim about an unbiased beacon.";

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
        "build-frontend" => {
            let model = PathBuf::from(take_flag(&mut args, "--model").ok_or(PACK_USAGE)?);
            let frontend = PathBuf::from(take_flag(&mut args, "--frontend-pack").ok_or(PACK_USAGE)?);
            let out = PathBuf::from(take_flag(&mut args, "--out").ok_or(PACK_USAGE)?);
            let dir = PathBuf::from(take_flag(&mut args, "--pack").ok_or(PACK_USAGE)?);
            let vectors = PathBuf::from(take_flag(&mut args, "--vectors").ok_or(PACK_USAGE)?);
            let decode = num(&mut args, "--decode")?.unwrap_or(4);
            let block = num(&mut args, "--block-bytes")?.unwrap_or(1 << 20);
            let revision = take_flag(&mut args, "--revision");
            if let Some(extra) = args.first() {
                return Err(format!("unexpected argument `{extra}`"));
            }
            let jobs = super::primitive::read_jobs(&vectors, decode)?;
            let pack = super::primitive::build(&model, &frontend, &out, &dir, &jobs, revision, block)?;
            println!(
                "{}",
                serde_json::json!({"pack_digest":pack.digest()?,"integer_conformance":"PASS","source_equivalence":pack.source_equivalence,
                "full_task":"UNVERIFIED","live_final":"UNVERIFIED"})
            );
            Ok(0)
        }
        "verify-frontend" => {
            let model = PathBuf::from(take_flag(&mut args, "--model").ok_or(PACK_USAGE)?);
            let artifact = PathBuf::from(take_flag(&mut args, "--artifact").ok_or(PACK_USAGE)?);
            let dir = PathBuf::from(take_flag(&mut args, "--pack").ok_or(PACK_USAGE)?);
            let rebuilt = PathBuf::from(take_flag(&mut args, "--rebuild-out").ok_or(PACK_USAGE)?);
            let block = num(&mut args, "--block-bytes")?.unwrap_or(1 << 20);
            if let Some(extra) = args.first() {
                return Err(format!("unexpected argument `{extra}`"));
            }
            let pack = super::primitive::verify(&dir, &model, &artifact, &rebuilt, block)?;
            println!("{}", pack.report()?);
            Ok(0)
        }
        "attach-frontend-fidelity" => {
            let dir = PathBuf::from(take_flag(&mut args, "--pack").ok_or(PACK_USAGE)?);
            let artifact = PathBuf::from(take_flag(&mut args, "--artifact").ok_or(PACK_USAGE)?);
            let reference = PathBuf::from(take_flag(&mut args, "--hf-reference").ok_or(PACK_USAGE)?);
            let policy = PathBuf::from(take_flag(&mut args, "--fidelity-policy").ok_or(PACK_USAGE)?);
            let out = PathBuf::from(take_flag(&mut args, "--out").ok_or(PACK_USAGE)?);
            if let Some(extra) = args.first() {
                return Err(format!("unexpected argument `{extra}`"));
            }
            let pack = super::frontend_fidelity::attach(&dir, &artifact, &reference, &policy, &out)?;
            println!(
                "{}",
                serde_json::json!({"pack_digest":pack.digest()?, "reference_logits":"WITHIN_PREDECLARED_TOLERANCE",
                "source_equivalence":pack.source_equivalence,"routing_fidelity":"UNVERIFIED","runtime_state_saturation":"UNVERIFIED",
                "task_quality":"UNVERIFIED","full_task":"UNVERIFIED","live_final":"UNVERIFIED"})
            );
            Ok(0)
        }
        "commit-conformance" => conformance_cli::commit(&mut args, &log),
        "run-conformance" => conformance_cli::run_cmd(&mut args, &log),
        "verify-conformance" => conformance_cli::verify_cmd(&mut args, &log),
        "synthetic-facts" => conformance_cli::synthetic(&mut args),
        "conformance-status" => conformance_cli::status(&mut args),
        "index" => {
            // RFC-0013 §7.2: the stored Merkle index of an artifact — one pass now, so that opening the leaves a beacon draw names (and
            // authenticating a row tile) never needs another.
            let artifact = PathBuf::from(take_flag(&mut args, "--artifact").ok_or(PACK_USAGE)?);
            let out =
                take_flag(&mut args, "--out").map(PathBuf::from).unwrap_or_else(|| super::beacon_run::merkle_index_path(&artifact));
            let want = take_flag(&mut args, "--root");
            if let Some(extra) = args.first() {
                return Err(format!("unexpected argument `{extra}`"));
            }
            let c =
                misaka_palw_tir_artifact::PalwTirContainerV1::open(&artifact).map_err(|e| format!("{}: {e}", artifact.display()))?;
            let ranges = crate::tir_stream::ContainerRanges::open(&c)?;
            let index =
                crate::tir_merkle_index::PalwTirMerkleIndexV1::build_streamed(&c.program, &ranges).map_err(|e| e.to_string())?;
            if let Some(want) = want {
                let root = kaspa_hashes::Hash64::from_bytes(super::commit::unhex64(&want)?);
                index.verify_root(root).map_err(|e| format!("not writing {}: {e}", out.display()))?;
            }
            index.write(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            log(format!("{} leaves, root {}; wrote {}", index.leaf_count(), index.root(), out.display()));
            println!("{}", index.root());
            Ok(0)
        }
        "bind-class" => {
            let dir = PathBuf::from(take_flag(&mut args, "--pack").ok_or(PACK_USAGE)?);
            let artifact = PathBuf::from(take_flag(&mut args, "--artifact").ok_or(PACK_USAGE)?);
            let network = take_flag(&mut args, "--network").ok_or(PACK_USAGE)?;
            let out = PathBuf::from(take_flag(&mut args, "--out").ok_or(PACK_USAGE)?);
            if let Some(extra) = args.first() {
                return Err(format!("unexpected argument `{extra}`"));
            }
            let legacy = dir.join(PACK_FILE).exists();
            let frontend = dir.join(super::primitive::PACK_FILE).exists();
            if legacy == frontend {
                return Err("exactly one supported pack manifest is required".into());
            }
            let digest = if frontend {
                super::bind::bind_frontend_class(&dir, &artifact, &network, &out)?.digest()?
            } else {
                super::bind::bind_class(&dir, &artifact, &network, &out)?.digest()
            };
            println!("{digest}");
            log("bound exact identity only; verify this pack and check live admission before registering".into());
            Ok(0)
        }
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
            let name = take_flag(&mut args, "--name")
                .unwrap_or_else(|| model.file_name().and_then(|n| n.to_str()).unwrap_or("model").to_string());
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
            o.streamed = if take_bool(&mut args, "--stream") {
                Some(true)
            } else if take_bool(&mut args, "--no-stream") {
                Some(false)
            } else {
                None
            };
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
            o.streamed = if take_bool(&mut args, "--stream") {
                Some(true)
            } else if take_bool(&mut args, "--no-stream") {
                Some(false)
            } else {
                None
            };
            o.declared = !take_bool(&mut args, "--no-declared");
            o.pack_dir = PathBuf::from(args.first().ok_or(PACK_USAGE)?);
            let r = verify(&o, &log)?;
            if json {
                let checks: Vec<_> = r
                    .checks
                    .iter()
                    .map(|c| serde_json::json!({ "check": c.name, "status": format!("{:?}", c.status).to_uppercase(), "detail": c.detail }))
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({ "pack": r.pack_digest, "verified": r.verified(), "ok": r.ok(), "checks": checks })
                    )
                    .unwrap_or_default()
                );
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
    o.push_str(&format!(
        "model        {} [{}], {} source file(s), {}\n",
        p.model.label,
        p.model.format,
        p.model.files.len(),
        p.model.architectures.join(", ")
    ));
    o.push_str(&format!(
        "frontend     level {}, adapter {}{}, spec {}\n",
        p.frontend.level,
        p.frontend.adapter.kind,
        p.frontend.adapter.id.as_ref().map(|i| format!(" `{i}`")).unwrap_or_default(),
        s(&p.frontend.spec_digest)
    ));
    let scope = &p.features.scope;
    let left: Vec<String> = scope["excluded"]
        .as_array()
        .map(|a| a.iter().filter_map(|e| e["what"].as_str()).map(str::to_string).collect())
        .unwrap_or_default();
    o.push_str(&format!(
        "scope        {} → {}{}\n",
        scope["input"].as_str().unwrap_or("?"),
        scope["output"].as_str().unwrap_or("?"),
        if left.is_empty() { "; nothing left out".to_string() } else { format!("; NOT computed: {}", left.join(", ")) }
    ));
    o.push_str(&format!("features     {} in use\n", p.features.used.len()));
    o.push_str(&format!(
        "quant        {}\n",
        if p.quant.descriptors.is_empty() {
            "float checkpoint".to_string()
        } else {
            p.quant.descriptors.iter().map(|d| format!("{} {}", d.name, s(&d.digest))).collect::<Vec<_>>().join(", ")
        }
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
    o.push_str(&format!(
        "executors    {}\n",
        p.executor.implementations.iter().map(|i| i.name.as_str()).collect::<Vec<_>>().join(", ")
    ));
    o.push_str(&format!(
        "logits       {} (scale {:e}); tolerance slope [{}, {}], corr ≥ {}, top-1 ≥ {}, KL ≤ {}\n",
        p.logits.convention,
        p.logits.scale,
        p.logits.tolerance.slope_min,
        p.logits.tolerance.slope_max,
        p.logits.tolerance.corr_min,
        p.logits.tolerance.top1_min,
        p.logits.tolerance.kl_max
    ));
    o.push_str(&format!(
        "artifact     {} bytes, file {}, inventory root {}, graph root {}\n",
        p.result.artifact_bytes,
        s(&p.result.artifact_digest),
        s(&p.result.inventory_root),
        s(&p.result.graph_ir_root)
    ));
    o.push_str(&format!(
        "conformance  {} vector(s), {} positions\n",
        p.conformance.vectors.len(),
        p.conformance.vectors.iter().map(|v| v.positions).sum::<usize>()
    ));
    match &p.hf_reference {
        Some(h) => o.push_str(&format!(
            "reference    {} positions over {} sequence(s): slope {:.4}, corr {:.5}, top-1 {:.3}, KL {:.5}\n",
            h.positions, h.sequences, h.measured.slope, h.measured.corr, h.measured.top1, h.measured.kl_mean
        )),
        None => o.push_str("reference    none\n"),
    }
    for d in &p.declared {
        o.push_str(&format!(
            "declared     {}: class {} (context {}, interval {})\n",
            d.network,
            s(&d.class_id),
            d.max_context,
            d.checkpoint_interval
        ));
    }
    o
}

/// The beacon-conformance subcommands.
mod conformance_cli {
    use super::{PACK_USAGE, num, take_bool, take_flag};
    use crate::runtime_pack::beacon_run::*;
    use crate::runtime_pack::commit::{CommitParamsV1, ConformanceScopeV1, hex, unhex64};
    use crate::runtime_pack::conformance::ImplSet;
    use crate::runtime_pack::facts::{FileFactSource, facts_to_json};
    use misaka_palw_challenge::hash::{Digest, named_id};
    use misaka_palw_challenge::reference_policy_v1;
    use std::path::PathBuf;

    fn req(args: &mut Vec<String>, flag: &str) -> Result<String, String> {
        take_flag(args, flag).ok_or_else(|| format!("{flag} is required\n{PACK_USAGE}"))
    }

    fn digest_arg(s: &str) -> Result<Digest, String> {
        match s.strip_prefix("label:") {
            Some(l) => Ok(named_id(&format!("tool-label/{l}"))),
            None => unhex64(s),
        }
    }

    fn no_extra(args: &[String]) -> Result<(), String> {
        match args.first() {
            Some(extra) => Err(format!("unexpected argument `{extra}`\n{PACK_USAGE}")),
            None => Ok(()),
        }
    }

    pub fn commit(args: &mut Vec<String>, log: &dyn Fn(String)) -> Result<i32, String> {
        let pack = PathBuf::from(req(args, "--pack")?);
        let artifact = PathBuf::from(req(args, "--artifact")?);
        let state = PathBuf::from(req(args, "--state")?);
        let network = req(args, "--network")?;
        let genesis = digest_arg(&req(args, "--chain-genesis")?)?;
        let ruleset = digest_arg(&req(args, "--ruleset-id")?)?;
        let k = num::<u32>(args, "--k")?.unwrap_or(3);
        let delay = num::<u64>(args, "--delay")?.unwrap_or(2);
        let window = num::<u64>(args, "--window")?.unwrap_or(40);
        let depth = num::<u64>(args, "--depth")?.unwrap_or(5);
        let reps = num::<u32>(args, "--repetitions")?.unwrap_or(4);
        let mut policy = reference_policy_v1(k, delay, window, depth, reps);
        if let Some(b) = num::<u16>(args, "--security-bits")? {
            policy.security_bits = b;
        }
        if let Some(r) = num::<u32>(args, "--retry-limit")? {
            policy.retry_limit = r;
        }
        let mut scope = ConformanceScopeV1::new(
            num(args, "--vectors")?.unwrap_or(14),
            num(args, "--prompt-len")?.unwrap_or(4),
            num(args, "--decode")?.unwrap_or(2),
            num(args, "--leaves")?.unwrap_or(512),
        );
        if let Some(p) = num(args, "--vector-fault-ppm")? {
            scope.vector_fault_ppm = p;
        }
        if let Some(p) = num(args, "--leaf-fault-ppm")? {
            scope.leaf_fault_ppm = p;
        }
        scope.require_independent = !take_bool(args, "--no-require-independent");
        scope.require_backend = !take_bool(args, "--no-require-backend");
        let mut params = CommitParamsV1::new(network, genesis, ruleset, policy, scope);
        params.class_id_prefix = take_flag(args, "--class-id");
        params.candidate_id = take_flag(args, "--candidate-id").map(|s| digest_arg(&s)).transpose()?;
        params.plan_positions = num(args, "--plan-positions")?;
        no_extra(args)?;
        let (b, dir) = commit_conformance(&pack, &artifact, &state, &params, log).map_err(|e| e.to_string())?;
        eprintln!("commitment   {}", dir.display());
        eprintln!("policy       {}", params.policy_label);
        eprintln!(
            "admission    {} (hypothetically armed; shipped schedule: {}), plan {} at {} positions",
            b.admission.kernel,
            b.admission.shipped_outcome,
            &hex(&b.admission.plan_root)[..16],
            b.admission.positions
        );
        eprintln!("scope        {}", params.scope.statement(params.policy.repetition_count));
        println!("{}", hex(&b.commitment.statement_root()));
        Ok(0)
    }

    fn impls(args: &mut Vec<String>) -> ImplSet {
        ImplSet { exec: !take_bool(args, "--no-exec"), ref2: !take_bool(args, "--no-ref2") }
    }

    pub fn run_cmd(args: &mut Vec<String>, log: &dyn Fn(String)) -> Result<i32, String> {
        let pack = PathBuf::from(req(args, "--pack")?);
        let artifact = PathBuf::from(req(args, "--artifact")?);
        let state = PathBuf::from(req(args, "--state")?);
        let commitment = req(args, "--commitment")?;
        let facts = PathBuf::from(req(args, "--facts")?);
        let max_checks = num::<usize>(args, "--max-checks")?;
        let im = impls(args);
        no_extra(args)?;
        let src = FileFactSource(facts);
        let out = run_conformance(
            &RunInput {
                pack_dir: &pack,
                artifact: &artifact,
                state_dir: &state,
                commitment: &commitment,
                source: &src,
                impls: im,
                max_checks,
                fault: None,
            },
            log,
        )
        .map_err(|e| e.to_string())?;
        match out {
            RunOutcome::Waiting { have, need, lock_position, tip } => {
                println!(
                    "WAITING_RANDOMNESS: {have} of {need} qualifying works{} at tip {tip}; nothing was run, no fallback randomness is used. Resume with the same command when the canonical history has advanced.",
                    lock_position.map(|l| format!(", the beacon locks at position {l}")).unwrap_or_default()
                );
                Ok(3)
            }
            RunOutcome::Unavailable { have, need, tip, retries, retry_limit } => {
                println!(
                    "BEACON_UNAVAILABLE: the window closed with {have} of {need} qualifying works at tip {tip} (window {retries} of {} allowed). Not fraud, not a pass; a retry is a NEW commitment and window.",
                    retry_limit + 1
                );
                Ok(3)
            }
            RunOutcome::Interrupted { done, total, seed } => {
                println!(
                    "INTERRUPTED: {done} of {total} checks recorded under seed {}; resume with the same command.",
                    &hex(&seed)[..16]
                );
                Ok(3)
            }
            RunOutcome::Evidence { evidence, dir, seed, local, measures, provenance } => {
                println!("evidence      {}", dir.join("evidence.borsh").display());
                println!("evidence id   {}", hex(&evidence.id()));
                println!("challenge seed {}", hex(&seed));
                println!("facts         {}", provenance.label());
                println!(
                    "status        {} — {} of {} required checks run, {} failed, {} missing; derived -log2(eps) >= {}",
                    status_code(evidence.status),
                    evidence.checks_run,
                    evidence.checks_required,
                    evidence.checks_failed,
                    evidence.missing_checks.len(),
                    evidence.derived_epsilon_bits
                );
                for f in &evidence.failures {
                    println!("  FAILED   {f}");
                }
                for m in &evidence.missing_checks {
                    println!("  MISSING  {m}");
                }
                println!("measured      {}", measures.to_json());
                match local {
                    Ok(()) => {
                        println!(
                            "local judgement: the contract accepts this evidence for the beacon this process derived (a fresh `verify-conformance` is what counts)"
                        );
                        Ok(0)
                    }
                    Err(e) => {
                        println!("NOT A PASS: {e}");
                        Ok(2)
                    }
                }
            }
        }
    }

    pub fn verify_cmd(args: &mut Vec<String>, log: &dyn Fn(String)) -> Result<i32, String> {
        let pack = PathBuf::from(req(args, "--pack")?);
        let artifact = PathBuf::from(req(args, "--artifact")?);
        let state = PathBuf::from(req(args, "--state")?);
        let commitment = req(args, "--commitment")?;
        let facts = PathBuf::from(req(args, "--facts")?);
        let evidence = PathBuf::from(req(args, "--evidence")?);
        let rerun = !take_bool(args, "--no-rerun");
        no_extra(args)?;
        let src = FileFactSource(facts);
        let v = verify_conformance(
            &VerifyInput {
                pack_dir: &pack,
                artifact: &artifact,
                state_dir: &state,
                commitment: &commitment,
                evidence: &evidence,
                source: &src,
                rerun,
                impls: ImplSet::default(),
            },
            log,
        )
        .map_err(|e| e.to_string())?;
        match v {
            Verdict::Pass { evidence_id, measures, provenance } => {
                println!(
                    "PASS: every derivable field recomputed from public inputs, every selected check re-executed, the evidence reproduced exactly"
                );
                println!("evidence id   {}", hex(&evidence_id));
                println!(
                    "facts         {}{}",
                    provenance.label(),
                    if provenance.is_synthetic() {
                        "  (SYNTHETIC: this exercises the pipeline; it says nothing about any chain's history)"
                    } else {
                        ""
                    }
                );
                println!(
                    "scope         a SAMPLED conditional check — not full-scope fidelity, not semantic admission, not a statement that the beacon is unbiased"
                );
                println!("measured      {}", measures.to_json());
                Ok(0)
            }
            Verdict::NotReproduced { provenance } => {
                println!(
                    "NOT A PASS (results not re-executed): the challenge, selection and evidence are consistent with the canonical facts ({}), but --no-rerun leaves the result roots unverified",
                    provenance.label()
                );
                Ok(3)
            }
            Verdict::Pending { why } => {
                println!("NOT A PASS (pending): {why}");
                Ok(3)
            }
            Verdict::NotPass { status, detail } => {
                println!("NOT A PASS: the evidence says {} — {detail}", status_code(status));
                Ok(2)
            }
            Verdict::Fail { code, detail } => {
                println!("FAIL {code}: {detail}");
                Ok(2)
            }
        }
    }

    pub fn synthetic(args: &mut Vec<String>) -> Result<i32, String> {
        let state = PathBuf::from(req(args, "--state")?);
        let commitment = req(args, "--commitment")?;
        let out = PathBuf::from(req(args, "--out")?);
        let position = num::<u64>(args, "--position")?.unwrap_or(1000);
        let works = num::<u32>(args, "--works")?;
        let tip = num::<u64>(args, "--tip")?;
        let label = take_flag(args, "--label").unwrap_or_else(|| "synthetic".into());
        no_extra(args)?;
        let l =
            load_commitment(&resolve_commitment_dir(&state, &commitment).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let mut f = synthetic_facts(&l.commitment, &l.policy, position, &label);
        if let Some(w) = works {
            f.events.truncate(w as usize);
        }
        if let Some(t) = tip {
            f.tip_position = t;
        }
        write_atomic(&out, serde_json::to_string_pretty(&facts_to_json(&f, &l.policy.id())).unwrap_or_default().as_bytes())?;
        eprintln!(
            "wrote SYNTHETIC beacon facts ({} works, tip {}) — not a canonical history of any chain",
            f.events.len(),
            f.tip_position
        );
        Ok(0)
    }

    pub fn status(args: &mut Vec<String>) -> Result<i32, String> {
        let state = PathBuf::from(req(args, "--state")?);
        no_extra(args)?;
        let text = std::fs::read_to_string(ledger_path(&state)).map_err(|e| format!("{}: {e}", ledger_path(&state).display()))?;
        print!("{text}");
        Ok(0)
    }
}
