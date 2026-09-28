//! `palw-tir-check --config config.json [--weights PATH | --weights-index INDEX] [--json]`
//!
//! Reads a Hugging Face `config.json` and prints the normalised ArchSpec, the HL program's
//! blocks and layer schedule, per-position MAC/state estimates and, when weights are given, a
//! shape check of every HL param against the checkpoint. Exit status: 0 lowerable, 2
//! NOT_LOWERABLE, 1 usage or I/O error. Non-consensus tooling (RFC-0002 §8, Gate 1).

use clap::Parser;
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
    /// Print the ArchSpec, HL program and costs as JSON instead of text.
    #[arg(long)]
    json: bool,
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
            if a.json {
                let v = serde_json::json!({
                    "verdict": report::verdict(&c),
                    "spec": c.spec,
                    "program": c.program,
                    "cost": c.cost,
                    "weights": c.weights.as_ref().map(|w| serde_json::json!({"bound": w.bound, "errors": w.errors, "unused": w.unused})),
                });
                println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            } else {
                print!("{}", report::render(&c));
            }
            let bad = c.weights.as_ref().map(|w| !w.errors.is_empty()).unwrap_or(false);
            std::process::exit(if bad { 2 } else { 0 });
        }
        Err(e) => {
            println!("{}", report::refusal(&e));
            std::process::exit(if matches!(e, misaka_palw_tir_lower::LowerError::NotLowerable(_)) { 2 } else { 1 });
        }
    }
}
