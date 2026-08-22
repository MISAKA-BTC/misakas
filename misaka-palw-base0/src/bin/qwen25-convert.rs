//! Convert a real Qwen2.5 checkpoint to a PALW integer artifact, and report what it produced.
//!
//! ```text
//! qwen25-convert <dir>          # dir holds model.safetensors, config.json, tokenizer.json
//! qwen25-convert <dir> --layers N   # convert only the first N layers (a depth sweep)
//! ```
//!
//! Prints the class id, the artifact's size, the bias coverage and a forward pass's depth
//! health. Nothing here is on the block-validation path: a verifier re-runs this and compares the
//! class id, which is why the conversion has to be bit-reproducible.

use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use misaka_palw_base0::convert::{
    Qwen25ConvertPlan, activation_scale_of, biased_channel_count, calibrate_layer_residuals, convert_qwen25,
    measure_depth_health,
};

fn die(message: String) -> ! {
    eprintln!("qwen25-convert: {message}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args
        .get(1)
        .unwrap_or_else(|| die("usage: qwen25-convert <dir> [--layers N] [--max-position N] [--out FILE]".into()));
    let flag = |name: &str| -> Option<String> {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
    };
    let out_path = flag("--out");
    let layer_cap: Option<usize> = args.iter().position(|a| a == "--layers").and_then(|i| args.get(i + 1)).map(|v| {
        v.parse().unwrap_or_else(|_| die(format!("--layers wants a number, got {v}")))
    });

    let cfg_bytes = std::fs::read(format!("{dir}/config.json")).unwrap_or_else(|e| die(format!("config.json: {e}")));
    let cfg: serde_json::Value = serde_json::from_slice(&cfg_bytes).unwrap_or_else(|e| die(format!("config.json: {e}")));
    let num = |k: &str| -> usize {
        cfg.get(k).and_then(|v| v.as_u64()).unwrap_or_else(|| die(format!("config.json has no {k}"))) as usize
    };
    let hidden = num("hidden_size");
    let heads = num("num_attention_heads");
    let declared_layers = num("num_hidden_layers");
    let shape = Base0ShapeV1 {
        n_layers: layer_cap.unwrap_or(declared_layers).min(declared_layers),
        n_heads: heads,
        n_kv_heads: num("num_key_value_heads"),
        d_head: hidden / heads,
        d_ff: num("intermediate_size"),
        vocab: num("vocab_size"),
        // The rotary table is generated for this many positions, and generating 32k of them for a
        // conversion smoke test costs more than it proves. The class a network registers picks its
        // own — `--max-position` — and this stays the tool's default, printed below so it is never
        // a silent choice. It is inside the artifact digest, so a class that registers one context
        // length and runs another is a different class.
        max_position: flag("--max-position")
            .map(|v| v.parse().unwrap_or_else(|_| die(format!("--max-position wants a number, got {v}"))))
            .unwrap_or(512),
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1 << 8,
    };
    println!("config: hidden {hidden}, heads {heads}/{} kv, d_head {}, ffn {}, layers {}/{declared_layers}, vocab {}",
        shape.n_kv_heads, shape.d_head, shape.d_ff, shape.n_layers, shape.vocab);

    let tokenizer = std::fs::read(format!("{dir}/tokenizer.json")).unwrap_or_else(|e| die(format!("tokenizer.json: {e}")));
    let commitment = Base0ArtifactV1::tokenizer_commitment_of(&tokenizer);

    let started = std::time::Instant::now();
    let blob = std::fs::read(format!("{dir}/model.safetensors")).unwrap_or_else(|e| die(format!("model.safetensors: {e}")));
    println!("read {} MiB in {:?}", blob.len() / (1 << 20), started.elapsed());

    let plan = Qwen25ConvertPlan { shape, rms_norm_eps_bits: 1e-6f32.to_bits() };
    let started = std::time::Instant::now();
    let artifact = convert_qwen25(&blob, &plan).unwrap_or_else(|e| die(format!("conversion failed: {e}")));
    let artifact = artifact.with_tokenizer_commitment(commitment);
    println!("converted in {:?}", started.elapsed());
    drop(blob);

    let weight_bytes: usize = artifact.embed.len()
        + artifact.unembed.len()
        + artifact.layers.iter().map(|l| l.wq.len() + l.wk.len() + l.wv.len() + l.wo.len() + l.w_gate.len() + l.w_up.len() + l.w_down.len()).sum::<usize>();
    println!("class id      {}", artifact.artifact_digest());
    println!("tokenizer     {commitment}");
    println!("int8 weights  {} MiB", weight_bytes / (1 << 20));
    println!("biased chans  {} (zero would mean the biases rounded away)", biased_channel_count(&artifact));
    println!("act scale     {}", activation_scale_of(&artifact.norm_requant));
    println!("max_position  {} (the tool's default, not the model's context length)", artifact.shape.max_position);

    let prompt = [9707usize, 11, 1879, 0];

    // Phase 3's contingency: one global residual shift is not enough at depth, so each layer's is
    // re-derived from the peak that layer actually produces. Calibration is part of the class
    // identity — `artifact_root` covers the table — so the class id below is the CALIBRATED one.
    let artifact = if args.iter().any(|a| a == "--no-calibrate") {
        artifact
    } else {
        let started = std::time::Instant::now();
        let calibrated =
            calibrate_layer_residuals(&artifact, &prompt, 4).unwrap_or_else(|e| die(format!("calibration failed: {e:?}")));
        println!("calibrated in {:?}", started.elapsed());
        if let Some(table) = &calibrated.layer_residual_requant {
            let shifts: Vec<u8> = table.iter().map(|p| p[1].shift).collect();
            println!("  ffn residual shifts {shifts:?}");
        }
        println!("  class id (calibrated) {}", calibrated.artifact_digest());
        calibrated
    };
    let started = std::time::Instant::now();
    let health = measure_depth_health(&artifact, &prompt).unwrap_or_else(|e| die(format!("forward failed: {e:?}")));
    println!("forward {} tokens in {:?}", prompt.len(), started.elapsed());
    println!("  alive          {}", health.is_alive());
    println!("  gate asym      {}", health.gate_is_asymmetric());
    println!("  residual peak  min {} max {}", health.residual_peak.iter().min().unwrap(), health.residual_peak.iter().max().unwrap());
    println!("  gate peak      min {} max {}", health.gate_peak_decay().iter().min().unwrap(), health.gate_peak_decay().iter().max().unwrap());
    println!("  attn spread    {} (0 = a head selected nothing)", health.min_attention_spread);
    println!("  railed layers  {}/{}", health.saturated_residual.0, health.saturated_residual.1);
    println!("  argmax         {:?}", health.argmax);

    // Determinism, on the real thing: the property the whole class rests on.
    let again = measure_depth_health(&artifact, &prompt).unwrap();
    println!("  reproducible   {}", again == health);

    // **The artifact has to leave this process to be worth anything.** The floor's is DERIVED, so
    // every node mints it from a seed; a converted class cannot be, so until it could be written
    // the only class the tree could actually run was the one it could re-derive. The file is
    // digest-checked on load, so a truncated copy is refused rather than run as some other class.
    // **The two values a registration needs, and they are NOT the same value.** ADR-0049 Decision
    // G: `execution_class_id` is the SHAPE PROFILE id — a class is its graph — while
    // `artifact_root` is the Merkle root over the canonical operand inventory, which is what an
    // opening proves against. The line printed as "class id" above is neither: it is
    // `artifact_digest()`, the artifact's own content hash. Printing all three with their real
    // names, because registering the wrong one is a class nobody can adjudicate.
    if let Some(tile) = flag("--tile-len") {
        let tile_len: u32 = tile.parse().unwrap_or_else(|_| die(format!("--tile-len wants a number, got {tile}")));
        // BASE-0's geometry type carries no kv-head count, and it does not need to: the inventory
        // is built from the ARTIFACT's shape (which has `n_kv_heads`) and the geometry is only
        // cross-checked and read for `tile_len`. `check_geometry` compares the six fields both
        // types share, so a GQA class passes with its query-head count.
        let geometry = kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1 {
            layer_count: artifact.shape.n_layers as u16,
            hidden_dim: artifact.shape.d_model() as u32,
            ffn_dim: artifact.shape.d_ff as u32,
            attn_heads: artifact.shape.n_heads as u16,
            attn_head_dim: artifact.shape.d_head as u32,
            vocab_size: artifact.shape.vocab as u32,
            n_ctx: artifact.shape.max_position as u32,
            n_threads: 1,
            rms_eps_q: artifact.shape.eps_q,
            tile_len,
        };
        match misaka_palw_base0::inventory::base0_inventory_v1(&artifact, geometry) {
            Ok(inv) => {
                println!("artifact_root {}   <- what a registration pins, and what openings prove against", inv.root());
                println!("  inventory rows {}", inv.operands().len());
            }
            Err(e) => println!("artifact_root  UNAVAILABLE: {e:?}"),
        }
        let qgeo = kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
            layer_count: artifact.shape.n_layers as u16,
            hidden_dim: artifact.shape.d_model() as u32,
            ffn_dim: artifact.shape.d_ff as u32,
            attn_heads: artifact.shape.n_heads as u16,
            attn_kv_heads: artifact.shape.n_kv_heads as u16,
            attn_head_dim: artifact.shape.d_head as u32,
            vocab_size: artifact.shape.vocab as u32,
            n_ctx: artifact.shape.max_position as u32,
            n_threads: 1,
            rms_eps_q: artifact.shape.eps_q,
            tile_len,
        };
        match kaspa_consensus_core::palw_qwen25_profile::qwen25_profile_v1(qgeo) {
            Ok(profile) => println!("execution_class_id {}   <- the shape profile id: the class's identity", profile.shape_profile_id()),
            Err(e) => println!("execution_class_id UNAVAILABLE: {e:?}"),
        }
    }

    if let Some(path) = out_path {
        let started = std::time::Instant::now();
        let bytes = misaka_palw_base0::artifact::encode_artifact_file_v1(&artifact);
        std::fs::write(&path, &bytes).unwrap_or_else(|e| die(format!("writing {path}: {e}")));
        println!("wrote {} ({} MiB) in {:?}", path, bytes.len() / (1 << 20), started.elapsed());
        // Read it back HERE rather than trusting the writer: the class id a node will compute
        // from this file is the one the chain must have registered, and finding a mismatch after
        // it is on four hosts is finding it in the worst place.
        match misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes) {
            Ok(back) => println!("  reload class id {} ({})", back.artifact_digest(),
                if back.artifact_digest() == artifact.artifact_digest() { "matches" } else { "MISMATCH" }),
            Err(e) => die(format!("the file this tool just wrote does not load: {e}")),
        }
    }
}
