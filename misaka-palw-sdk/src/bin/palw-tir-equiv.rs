//! **`palw-tir-equiv` — drill D-F1, offline** (RFC-0002 Phase F, `phase-f-integration.md` §4): a
//! legacy dense row and its PALW-TIR program, same weights, same logits, same rows.
//!
//! ```text
//! palw-tir-equiv --network <id> [--model-id <row>]
//!                (--artifact <file.palwart> [--respan] | --derive [--derive-seed N])
//!                [--tir <file.palwtir>] [--prompts N] [--seed N] [--max-prefill N] [--json]
//! ```
//!
//! The row defaults to testnet-12's dense `Qwen/Qwen2.5-1.5B/graph-v7@8192`. The legacy side runs
//! the A16 engine over the plan compiled from the row's registered profile; the IR side runs the
//! A16 mirror program (which commits every one of the row's node rows) on the typed backend, over
//! the F3 conversion of the same artifact — in memory, or read from `--tir` (a `PALWTIR1` file
//! written by `palw-a16-to-tir`, whose program must be this row's mirror). For the canonical job
//! (the backend's prompt for the zero anchor at the row's canonical length) and `--prompts` random
//! prompts (32 by default), every logits row and every legacy node row must be equal, byte for
//! byte, at every position.
//!
//! `--artifact` names the legacy artifact. Its rotary table is a function of the shape, so an
//! artifact converted at a narrower width can be widened to the row's with `--respan` (the table
//! regenerated, nothing else touched); the tool says whether the artifact then pairs with the
//! root the network registered for the row at genesis — i.e. whether it IS the registered
//! artifact. `--derive` uses the deterministic artifact at the row's exact shape instead (no file:
//! the same function, synthetic weights). `--max-prefill` caps prompt lengths for a quick run.
//!
//! Exit status: 0 equal everywhere, 2 a difference (the first ones printed), 1 usage or I/O.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use misaka_palw_base0::artifact::{Base0ArtifactV1, decode_artifact_file_mapped_v1};
use misaka_palw_base0::classes::{A16_GRAPH_V7_8K_MODEL_ID, CanonicalClassV1, canonical_classes_v1};
use misaka_palw_base0::engine_a16::derived_a16_store;
use misaka_palw_base0::mmap::ReadOnlyMap;
use misaka_palw_base0::rope::RopeTableV1;
use misaka_palw_base0::tir_a16::{a16_mirror_program_with_rows, a16_tir_tensor_bytes};
use misaka_palw_sdk::tir_equiv::{EquivReportV1, equiv_jobs_v1, run_equiv_v1};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

const USAGE: &str = "usage: palw-tir-equiv --network <id> [--model-id <row>] (--artifact <file.palwart> [--respan] | --derive [--derive-seed N]) [--tir <file.palwtir>] [--prompts N] [--seed N] [--max-prefill N] [--json]";

fn die(message: String) -> ! {
    eprintln!("palw-tir-equiv: {message}");
    std::process::exit(1)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn number<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    match flag(args, name) {
        None => default,
        Some(v) => v.parse().unwrap_or_else(|_| die(format!("{name} {v}: not a number"))),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let network = flag(&args, "--network").unwrap_or_else(|| die(USAGE.into()));
    let model_id = flag(&args, "--model-id").unwrap_or(A16_GRAPH_V7_8K_MODEL_ID);
    let prompts: usize = number(&args, "--prompts", 32);
    let seed: u64 = number(&args, "--seed", 0x5EED);
    let json = has("--json");

    // The network: its court (the class table's input) and the roots it registered at genesis.
    let network_id: NetworkId = network.parse().unwrap_or_else(|e| die(format!("--network {network}: {e}")));
    let params: Params = network_id.into();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        die(format!("{network_id} has no PALW V2 bundle"));
    };
    let row: CanonicalClassV1 = canonical_classes_v1(&bundle.court)
        .into_iter()
        .find(|c| c.model_id == model_id)
        .unwrap_or_else(|| die(format!("no class row `{model_id}` in this build's table")));
    let registered: Option<Hash64> = bundle.genesis_objects.iter().find_map(|o| match o {
        PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } if *class_id == row.class_id() => Some(*artifact_root),
        _ => None,
    });

    // The legacy artifact.
    let artifact: Base0ArtifactV1 = if has("--derive") {
        let s = row.artifact_shape;
        Base0ArtifactV1::derive_deterministic(s, number(&args, "--derive-seed", 0x5A16))
            .and_then(|a| a.with_a16_params(derived_a16_store(&s)))
            .unwrap_or_else(|e| die(format!("deriving the artifact at the row's shape: {e:?}")))
    } else {
        let path = PathBuf::from(flag(&args, "--artifact").unwrap_or_else(|| die(USAGE.into())));
        let map = Arc::new(ReadOnlyMap::open(&path).unwrap_or_else(|e| die(format!("{}: {e}", path.display()))));
        let mut a = decode_artifact_file_mapped_v1(map).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
        if a.shape.max_position != row.artifact_shape.max_position && has("--respan") {
            let s = &mut a.shape;
            eprintln!("respan: the rotary table regenerated from {} to {} positions", s.max_position, row.artifact_shape.max_position);
            s.max_position = row.artifact_shape.max_position;
            a.rope = RopeTableV1::generate(s.d_head, s.max_position, s.ln_theta_gen_q)
                .unwrap_or_else(|e| die(format!("regenerating the rotary table: {e:?}")));
        }
        a
    };
    if artifact.shape != row.artifact_shape {
        die(format!(
            "the artifact's shape {:?} is not the row's {:?}{}",
            artifact.shape,
            row.artifact_shape,
            if artifact.shape.max_position != row.artifact_shape.max_position { " (--respan widens the rotary table)" } else { "" }
        ));
    }
    let root = row.artifact_root(&artifact).unwrap_or_else(|e| die(format!("the row's inventory over this artifact: {e:?}")));
    let pairing = match registered {
        Some(r) if r == root => format!("PAIRS with the root {network_id} registered for the row at genesis ({root})"),
        Some(r) => format!("does NOT pair: {network_id} registered {r}, this artifact gives {root}"),
        None => format!("{network_id} registers no class `{model_id}` at genesis; this artifact's root is {root}"),
    };

    // The IR program and its params.
    let (program, rows) = a16_mirror_program_with_rows(&artifact.shape, HISTORY_BOUND_V1_SMALL).unwrap_or_else(|e| die(e));
    let tensors: BTreeMap<(u16, Option<u16>), Vec<u8>> = match flag(&args, "--tir") {
        Some(p) => {
            let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(std::path::Path::new(p))
                .unwrap_or_else(|e| die(format!("{p}: {e}")));
            // A declared class commits its logits under a scheme (`palw-class declare-layout` sets it;
            // the converter leaves it unset), which no value depends on: compare the programs with it
            // taken from the container.
            let mut mirror = program.clone();
            mirror.logits_scheme_id = c.program.logits_scheme_id;
            if c.program != mirror {
                die(format!("{p} carries another program than this row's mirror (convert the artifact with palw-a16-to-tir)"));
            }
            c.header
                .tensors
                .iter()
                .map(|e| {
                    let b = c.read_tensor_bytes(e.param, e.layer).unwrap_or_else(|x| die(format!("{p}: {x}")));
                    ((e.param, e.layer), b)
                })
                .collect()
        }
        None => a16_tir_tensor_bytes(&artifact, &program).unwrap_or_else(|e| die(format!("converting the artifact: {e}"))),
    };

    let (prefill, decode) = (row.canonical_job.0 as usize, row.canonical_job.1 as usize);
    let prefill = prefill.min(number(&args, "--max-prefill", usize::MAX)).max(1);
    let jobs = equiv_jobs_v1(prefill, decode, artifact.shape.vocab, Hash64::default(), prompts, seed);
    eprintln!(
        "{model_id} on {network_id}: {} layers, vocab {}, n_ctx {}; {} jobs ({} positions); artifact {pairing}",
        artifact.shape.n_layers,
        artifact.shape.vocab,
        artifact.shape.max_position,
        jobs.len(),
        jobs.iter().map(|j| j.prompt.len() + j.decode).sum::<usize>()
    );
    let started = std::time::Instant::now();
    let report = run_equiv_v1(&artifact, &row.profile, &program, &rows, &tensors, &jobs, &mut |ji, r: &EquivReportV1| {
        eprintln!(
            "[{:7.1}s] job {}/{} {}: {} positions so far, logits {}/{}, rows {}/{}; legacy {:.1} ms/pos, IR {:.1} ms/pos",
            started.elapsed().as_secs_f64(),
            ji + 1,
            jobs.len(),
            jobs[ji].label,
            r.positions,
            r.logit_rows_equal,
            r.logit_rows,
            r.node_rows_equal,
            r.node_rows,
            1e3 * r.legacy_seconds / r.positions.max(1) as f64,
            1e3 * r.ir_seconds / r.positions.max(1) as f64
        );
    })
    .unwrap_or_else(|e| die(e));
    let verdict = if report.is_equal() { "EQUAL" } else { "DIFFERENT" };
    if json {
        let v = serde_json::json!({
            "network": network_id.to_string(), "row": model_id, "class_id": row.class_id().to_string(),
            "artifact_root": root.to_string(), "registered_root": registered.map(|r| r.to_string()),
            "pairs": registered == Some(root), "verdict": verdict, "jobs": report.jobs, "positions": report.positions,
            "logit_rows": report.logit_rows, "logit_rows_equal": report.logit_rows_equal,
            "node_rows": report.node_rows, "node_rows_equal": report.node_rows_equal,
            "ir_commits_without_legacy_row_per_position": report.commits_without_row,
            "legacy_ms_per_position": 1e3 * report.legacy_seconds / report.positions.max(1) as f64,
            "ir_ms_per_position": 1e3 * report.ir_seconds / report.positions.max(1) as f64,
            "first_mismatches": report.first_mismatches, "seed": seed, "prompts": prompts, "prefill": prefill, "decode": decode,
        });
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        println!("{verdict}: {model_id} on {network_id} — {} jobs, {} positions", report.jobs, report.positions);
        println!("  logits rows equal: {}/{}", report.logit_rows_equal, report.logit_rows);
        println!(
            "  legacy node rows equal to their IR commit points: {}/{} ({} IR commit points a position have no legacy row: the fused site's internals)",
            report.node_rows_equal, report.node_rows, report.commits_without_row
        );
        println!("  artifact {pairing}");
        println!(
            "  legacy {:.1} ms/position, IR (typed backend) {:.1} ms/position",
            1e3 * report.legacy_seconds / report.positions.max(1) as f64,
            1e3 * report.ir_seconds / report.positions.max(1) as f64
        );
        for m in &report.first_mismatches {
            println!("  mismatch: {m}");
        }
    }
    std::process::exit(if report.is_equal() { 0 } else { 2 });
}
