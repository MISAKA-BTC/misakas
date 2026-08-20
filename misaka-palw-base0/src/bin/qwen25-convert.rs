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
use misaka_palw_base0::convert::{Qwen25ConvertPlan, activation_scale_of, biased_channel_count, convert_qwen25, measure_depth_health};

fn die(message: String) -> ! {
    eprintln!("qwen25-convert: {message}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).unwrap_or_else(|| die("usage: qwen25-convert <dir> [--layers N]".into()));
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
        // own; this is the tool's default, printed below so it is never a silent choice.
        max_position: 512,
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
    println!("class id      {}", artifact.execution_class_id());
    println!("tokenizer     {commitment}");
    println!("int8 weights  {} MiB", weight_bytes / (1 << 20));
    println!("biased chans  {} (zero would mean the biases rounded away)", biased_channel_count(&artifact));
    println!("act scale     {}", activation_scale_of(&artifact.norm_requant));
    println!("max_position  {} (the tool's default, not the model's context length)", artifact.shape.max_position);

    let prompt = [9707usize, 11, 1879, 0];
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
}
