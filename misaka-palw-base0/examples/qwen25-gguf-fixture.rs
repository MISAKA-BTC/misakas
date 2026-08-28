//! Write the Qwen2.5-shaped dev checkpoint as a GGUF file — the LM Studio lane without the
//! download.
//!
//! ```text
//! cargo run -p misaka-palw-base0 --example qwen25-gguf-fixture -- <out.gguf> [bf16|f32|q8_0]
//! ```
//!
//! The file drives `qwen25-convert <out.gguf> --a16` end to end at a size any machine holds, so
//! the whole lane — GGUF parse, checkpoint synthesis, calibration, static PTQ, `--out`, the
//! node-side decode — can be smoke-tested before a 1.5B download is on disk. The weights are the
//! dev fixture's; nothing derived from this file is ever a registrable class.

use misaka_palw_base0::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::lmstudio::{DevFixtureCarrierV1, qwen25_gguf_dev_fixture};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(out) = args.get(1) else {
        eprintln!("usage: qwen25-gguf-fixture <out.gguf> [bf16|f32|q8_0]");
        std::process::exit(1);
    };
    let carrier = match args.get(2).map(String::as_str) {
        None | Some("q8_0") => DevFixtureCarrierV1::Q8_0,
        Some("bf16") => DevFixtureCarrierV1::Bf16,
        Some("f32") => DevFixtureCarrierV1::F32,
        Some(other) => {
            eprintln!("unknown carrier {other:?} (bf16, f32 and q8_0 exist)");
            std::process::exit(1);
        }
    };
    // The same shape the block-production test converts: Qwen2.5's structure — grouped-query
    // attention with a real group, biases, both norms — at a size a smoke test can calibrate.
    let shape = Base0ShapeV1 {
        n_layers: 2,
        n_heads: 4,
        n_kv_heads: 2,
        d_head: 8,
        d_ff: 64,
        vocab: 32,
        max_position: 32,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1 << 8,
    };
    let bytes = qwen25_gguf_dev_fixture(&shape, carrier);
    if let Err(e) = std::fs::write(out, &bytes) {
        eprintln!("{out}: {e}");
        std::process::exit(1);
    }
    println!("wrote {out} ({} bytes, carrier {carrier:?})", bytes.len());
}
