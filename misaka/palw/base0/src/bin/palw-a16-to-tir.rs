//! Convert an A16 `.palwart` artifact into a PALW-TIR artifact (`PALWTIR1`): the A16 engine's
//! mirror program for the artifact's shape, and every tensor converted — int8 slabs and rotary
//! tables verbatim, each `(multiplier, shift, zero)` triple split into typed `m`/`s`/`z` tensors
//! (RFC-0002 Phase F, F3; `misaka_palw_base0::tir_a16`).
//!
//! ```text
//! palw-a16-to-tir --artifact <in.palwart> --out <out.palwtir> [--respan N] [--held] [--check N]
//! ```
//!
//! `--held` lowers with the long history bound (2^21) instead of 2^18. `--respan N` first widens
//! (or narrows) the artifact to `N` positions: the rotary table is a function of the shape, so it
//! is regenerated and nothing else changes (testnet-12's genesis `graph-v7@8192` artifact is the
//! 512-wide genesis artifact respanned to 8,192 — `palw-tir-equiv` checks the pairing). `--check N`
//! runs the first `N` positions of a fixed token row through the A16 engine and through the
//! converted program on the reference evaluator (from the written file) and refuses unless every
//! logit code agrees.

use misaka_palw_base0::artifact::decode_artifact_file_mapped_v1;
use misaka_palw_base0::engine_a16::{A16Cache, A16Engine};
use misaka_palw_base0::mmap::ReadOnlyMap;
use misaka_palw_base0::tir_a16::convert_a16_to_tir;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_HELD, HISTORY_BOUND_V1_SMALL};
use std::path::PathBuf;
use std::sync::Arc;

fn die(message: String) -> ! {
    eprintln!("palw-a16-to-tir: {message}");
    std::process::exit(1)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: palw-a16-to-tir --artifact <in.palwart> --out <out.palwtir> [--respan N] [--held] [--check N]";
    let input = PathBuf::from(flag(&args, "--artifact").unwrap_or_else(|| die(usage.into())));
    let out = PathBuf::from(flag(&args, "--out").unwrap_or_else(|| die(usage.into())));
    let hb = if args.iter().any(|a| a == "--held") { HISTORY_BOUND_V1_HELD } else { HISTORY_BOUND_V1_SMALL };
    let check: usize = flag(&args, "--check").and_then(|v| v.parse().ok()).unwrap_or(0);
    let map = Arc::new(ReadOnlyMap::open(&input).unwrap_or_else(|e| die(format!("{}: {e}", input.display()))));
    let mut artifact = decode_artifact_file_mapped_v1(map).unwrap_or_else(|e| die(format!("{}: {e}", input.display())));
    if let Some(n) = flag(&args, "--respan") {
        let n: usize = n.parse().unwrap_or_else(|_| die(format!("--respan {n}: not a number")));
        let s = &mut artifact.shape;
        println!("respan: the rotary table regenerated from {} to {n} positions", s.max_position);
        s.max_position = n;
        artifact.rope = misaka_palw_base0::rope::RopeTableV1::generate(s.d_head, n, s.ln_theta_gen_q)
            .unwrap_or_else(|e| die(format!("regenerating the rotary table: {e:?}")));
    }
    if artifact.a16_params.is_none() {
        die(format!("{} carries no A16 parameter store (not an A16 class artifact)", input.display()));
    }
    let meta = format!(
        "{{\"converted_from\":\"{}\",\"converter\":\"palw-a16-to-tir\",\"history_bound\":{hb}}}",
        input.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
    );
    let (program, digest) = convert_a16_to_tir(&artifact, hb, &out, meta).unwrap_or_else(|e| die(e.to_string()));
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    println!(
        "{}: {} layers, {} params, program {} bytes; file digest {hex}",
        out.display(),
        artifact.shape.n_layers,
        program.params.len(),
        program.encode().len()
    );
    if check > 0 {
        let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(&out).unwrap_or_else(|e| die(e.to_string()));
        let engine = A16Engine::new(&artifact).unwrap_or_else(|e| die(format!("A16 engine: {e:?}")));
        let mut cache = A16Cache::new(artifact.shape.n_layers);
        let interp = misaka_palw_tir::Interpreter::new(&c.program).unwrap_or_else(|e| die(e.to_string()));
        let mut state = misaka_palw_tir::RunState::default();
        for pos in 0..check.min(artifact.shape.max_position) {
            let token = (pos * 7919 + 13) % artifact.shape.vocab;
            let want = engine.forward_token(&mut cache, token, pos).unwrap_or_else(|e| die(format!("engine at {pos}: {e:?}")));
            let got = interp.step(&c, &mut state, token as u32).unwrap_or_else(|e| die(format!("TIR at {pos}: {e}")));
            let got: Vec<i32> = got.logits.data.iter().map(|v| *v as i32).collect();
            if got != want {
                die(format!("position {pos}: the converted program's logits differ from the engine's"));
            }
        }
        println!("check: {check} positions, every logit code equal to the A16 engine's");
    }
}
