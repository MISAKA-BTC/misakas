//! `palw-tir-check --config config.json [--weights PATH | --weights-index INDEX] [--tir-out FILE]
//! [--long-history] [--tile-len N] [--h-chunk N] [--json]`
//!
//! Reads a Hugging Face `config.json` and prints the normalised ArchSpec, the HL program's
//! blocks and layer schedule, per-position MAC/state estimates and, when weights are given, a
//! shape check of every HL param against the checkpoint. Then it lowers the HL program to a
//! PALW-TIR program (Gate 2a), prints its summary, and ADMITS it: `tir_admit_v1` (spec 04b §10.3)
//! at tir/core's starting ceilings with `--tile-len` values per step leaf and an `--h-chunk`
//! history chunk (both 64 unless given; a class layout declares its own) — the per-position costs,
//! the court cones, the checkpoint interval, or the refusal by limit and number. `--tir-out` writes
//! the program's canonical encoding. Exit status: 0 lowered and admitted, 2 NOT_LOWERABLE, a
//! weights mismatch or an admission refusal, 1 usage or I/O error. Non-consensus tooling.

use clap::Parser;
use misaka_palw_tir_lower::lower::{self, LowerOpts};
use misaka_palw_tir_lower::report::{self, WeightsArg};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "palw-tir-check", about = "Describe a Hugging Face decoder for PALW-TIR lowering (RFC-0002 Gate 1)")]
struct Args {
    /// The model's config.json.
    #[arg(long)]
    config: PathBuf,
    /// A .safetensors file, a checkpoint directory, or a model.safetensors.index.json whose
    /// shards are present: every HL param is shape-checked against it.
    #[arg(long, conflicts_with = "weights_index")]
    weights: Option<PathBuf>,
    /// A model.safetensors.index.json; checks tensor names only (the shards may be absent).
    #[arg(long)]
    weights_index: Option<PathBuf>,
    /// Write the lowered PALW-TIR program (its canonical encoding) to this file.
    #[arg(long)]
    tir_out: Option<PathBuf>,
    /// Lower with the long history bound (2^21) instead of 2^18.
    #[arg(long)]
    long_history: bool,
    /// Values per step leaf admission tiles every commit point with (a layout's `commit_tiles`).
    #[arg(long, default_value_t = 64)]
    tile_len: u32,
    /// Positions per canonical history chunk (a layout's `h_tile`; a power of two).
    #[arg(long, default_value_t = 64)]
    h_chunk: u32,
    /// Print the ArchSpec, HL program and costs as JSON instead of text.
    #[arg(long)]
    json: bool,
}

/// The TIR half of the report: lowered or refused, summary, admission, digest.
fn tir_section(c: &report::Checked, a: &Args) -> (serde_json::Value, String, bool) {
    let opts = LowerOpts {
        history_bound: if a.long_history {
            misaka_palw_tir::program::HISTORY_BOUND_V1_HELD
        } else {
            misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL
        },
    };
    match lower::lower(&c.program, &opts) {
        Ok(lw) => {
            let p = &lw.program;
            let bytes = p.encode();
            let inputs = misaka_palw_tir::admit::TirAdmitInputsV1 {
                tile_len: a.tile_len,
                h_chunk: a.h_chunk,
                ..misaka_palw_tir_lower::admission::default_inputs()
            };
            let verdict = misaka_palw_tir_lower::admission::admit(p, &inputs);
            let digest = misaka_palw_tir_lower::artifact::program_digest(p);
            let mut written = None;
            if let Some(path) = &a.tir_out {
                match std::fs::write(path, &bytes) {
                    Ok(()) => written = Some(path.display().to_string()),
                    Err(e) => {
                        eprintln!("{}: {e}", path.display());
                        std::process::exit(1);
                    }
                }
            }
            let mut text = String::from("\nPALW-TIR (Gate 2a lowering)\n");
            text.push_str(&lw.summary());
            text.push_str(&misaka_palw_tir_lower::admission::render(p, &inputs, &verdict));
            text.push_str(&format!("  program digest (BLAKE2b-512 of the encoding): {digest}\n"));
            if let Some(w) = &written {
                text.push_str(&format!("  written: {w} ({} bytes)\n", bytes.len()));
            }
            let ok = verdict.is_ok();
            (
                serde_json::json!({
                    "lowered": true,
                    "bytes": bytes.len(),
                    "blocks": p.blocks.len(),
                    "nodes": p.blocks.iter().map(|b| b.nodes.len()).sum::<usize>(),
                    "params": p.params.len(),
                    "states": p.states.len(),
                    "admission": misaka_palw_tir_lower::admission::to_json(p, &inputs, &verdict),
                    "digest": digest,
                    "written": written,
                }),
                text,
                ok,
            )
        }
        Err(e) => {
            (serde_json::json!({ "lowered": false, "refusal": e.to_string() }), format!("\nPALW-TIR (Gate 2a lowering): {e}\n"), false)
        }
    }
}

fn main() {
    let a = Args::parse();
    let text = match std::fs::read_to_string(&a.config) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}: {e}", a.config.display());
            std::process::exit(1);
        }
    };
    let w = match (&a.weights, &a.weights_index) {
        (Some(p), _) => Some(WeightsArg::Files(p)),
        (None, Some(i)) => {
            // Shards present next to the index → full shape check; otherwise names only.
            let dir = i.parent().map(|d| d.to_path_buf()).unwrap_or_default();
            let shards_present = std::fs::read_to_string(i)
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| {
                    v.get("weight_map")
                        .and_then(|m| m.as_object())
                        .map(|m| m.values().filter_map(|x| x.as_str()).all(|s| dir.join(s).exists()))
                })
                .unwrap_or(false);
            Some(if shards_present { WeightsArg::Files(i) } else { WeightsArg::IndexNamesOnly(i) })
        }
        (None, None) => None,
    };
    match report::check(&text, w) {
        Ok(c) => {
            let (tir_json, tir_text, tir_ok) = tir_section(&c, &a);
            if a.json {
                let v = serde_json::json!({
                    "verdict": report::verdict(&c),
                    "spec": c.spec,
                    "program": c.program,
                    "cost": c.cost,
                    "weights": c.weights.as_ref().map(|w| serde_json::json!({"bound": w.bound, "errors": w.errors, "unused": w.unused})),
                    "tir": tir_json,
                });
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            } else {
                print!("{}", report::render(&c));
                print!("{tir_text}");
            }
            let bad = c.weights.as_ref().map(|w| !w.errors.is_empty()).unwrap_or(false);
            std::process::exit(if bad || !tir_ok { 2 } else { 0 });
        }
        Err(e) => {
            println!("{}", report::refusal(&e));
            std::process::exit(if matches!(e, misaka_palw_tir_lower::LowerError::NotLowerable(_)) { 2 } else { 1 });
        }
    }
}
