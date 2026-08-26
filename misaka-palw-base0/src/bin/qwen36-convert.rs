//! Convert a Qwen3.6 GGUF checkpoint into a `PALW-QWEN36` integer artifact, and report what it
//! produced.
//!
//! ```text
//! qwen36-convert <file.gguf> [--layers N] [--out artifact.palwq36]
//! ```
//!
//! `--layers N` converts the first `N` blocks. The layer alternation is preserved, so `N = 4`
//! still gets three GatedDeltaNet arms and one gated-attention arm — the whole graph, at a size
//! that fits a machine which cannot hold 35 B parameters.
//!
//! # What this does and does not claim
//!
//! It reads real weights, splits the fused tensors, quantizes per output channel, and runs the
//! engine over the result. The A16 triples are derived from each site's fan-in rather than
//! measured from activations, so **the output is not expected to be faithful** — what is being
//! exercised is the plumbing, the shapes, and the cost. Fidelity needs a float reference of the
//! hybrid graph to calibrate against, and that is the next piece.

use misaka_palw_base0::gguf::{GgufDirectory, GgufTensor, dequantize, parse_directory};
use misaka_palw_base0::qwen36::{Qwen36ArtifactV1, Qwen36Cache, Qwen36Engine, Qwen36LayerKind, Qwen36ShapeV1};
use misaka_palw_base0::rope::RopeTableV1;
use std::io::{Read, Seek, SeekFrom};

use kaspa_consensus_core::palw_base0_a16::A16QuantParams;

fn die(message: String) -> ! {
    eprintln!("qwen36-convert: {message}");
    std::process::exit(1)
}

/// Read one tensor's bytes out of the file and dequantize. A checkpoint is bigger than memory, so
/// nothing is held that is not being converted right now.
fn read_tensor(file: &mut std::fs::File, t: &GgufTensor) -> Vec<f32> {
    let mut buf = vec![0u8; t.bytes];
    file.seek(SeekFrom::Start(t.offset)).unwrap_or_else(|e| die(format!("seek to {}: {e}", t.offset)));
    file.read_exact(&mut buf).unwrap_or_else(|e| die(format!("read {}: {e}", t.name)));
    dequantize(t, &buf).unwrap_or_else(|e| die(format!("{}: {e}", t.name)))
}

fn tensor<'a>(dir: &'a GgufDirectory, name: &str) -> &'a GgufTensor {
    dir.tensors.get(name).unwrap_or_else(|| die(format!("the checkpoint has no tensor {name}")))
}

/// Per-output-channel symmetric int8 quantization. Returns the codes and each row's scale.
fn quantize_rows(values: &[f32], out_dim: usize) -> (Vec<i8>, Vec<f64>) {
    let n = values.len() / out_dim.max(1);
    let mut codes = Vec::with_capacity(values.len());
    let mut scales = Vec::with_capacity(out_dim);
    for c in 0..out_dim {
        let row = &values[c * n..(c + 1) * n];
        let absmax = row.iter().fold(0f32, |a, v| a.max(v.abs())) as f64;
        let scale = if absmax > 0.0 { absmax / 127.0 } else { 1.0 };
        scales.push(scale);
        for v in row {
            codes.push(((*v as f64 / scale).round()).clamp(-127.0, 127.0) as i8);
        }
    }
    (codes, scales)
}

/// A projection's narrowing, derived from its fan-in. The placeholder until a float reference
/// measures the real ranges — and it is a placeholder that is at least the right SHAPE, since a
/// random dot product grows like the square root of its length.
fn projection(fan_in: usize) -> A16QuantParams {
    let bits = usize::BITS - fan_in.max(1).leading_zeros();
    A16QuantParams { multiplier: 1, shift: (8 + bits / 2) as u8, zero: 0 }
}

const UNITY: A16QuantParams = A16QuantParams { multiplier: 1, shift: 0, zero: 0 };

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).unwrap_or_else(|| die("usage: qwen36-convert <file.gguf> [--layers N] [--out FILE]".into()));
    let layer_cap: Option<usize> =
        args.iter().position(|a| a == "--layers").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok());

    // The directory is in the first few tens of megabytes; reading it does not read the file.
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
    let mut head = vec![0u8; 64 << 20];
    let read = file.read(&mut head).unwrap_or_else(|e| die(format!("{path}: {e}")));
    head.truncate(read);
    let dir = parse_directory(&head).unwrap_or_else(|e| die(format!("{path}: {e}")));

    let meta = |k: &str| -> u64 { dir.metadata.get(k).and_then(|v| v.as_u64()).unwrap_or_else(|| die(format!("no metadata {k}"))) };
    let arch = dir.metadata.get("general.architecture").and_then(|v| v.as_str()).unwrap_or("");
    if arch != "qwen35moe" {
        die(format!("this converter reads qwen35moe checkpoints, not {arch:?}"));
    }
    let declared_layers = meta("qwen35moe.block_count") as usize;
    let layers = layer_cap.unwrap_or(declared_layers).min(declared_layers);
    let interval = meta("qwen35moe.full_attention_interval") as usize;
    let d_model = meta("qwen35moe.embedding_length") as usize;
    let head_dim = meta("qwen35moe.attention.key_length") as usize;
    let n_heads = meta("qwen35moe.attention.head_count") as usize;
    let rotary_dim = meta("qwen35moe.rope.dimension_count") as usize;
    let linear_head_dim = meta("qwen35moe.ssm.state_size") as usize;
    let linear_k_heads = meta("qwen35moe.ssm.group_count") as usize;
    let linear_v_dim = meta("qwen35moe.ssm.inner_size") as usize;
    let linear_v_heads = meta("qwen35moe.ssm.time_step_rank") as usize;
    let conv_kernel = meta("qwen35moe.ssm.conv_kernel") as usize;
    let n_experts = meta("qwen35moe.expert_count") as usize;
    let experts_per_token = meta("qwen35moe.expert_used_count") as usize;
    let moe_dim = meta("qwen35moe.expert_feed_forward_length") as usize;
    let shared_dim = meta("qwen35moe.expert_shared_feed_forward_length") as usize;
    let vocab = tensor(&dir, "token_embd.weight").dims[1] as usize;
    // `attn_k` is [d_model, kv_dim], so the kv head count comes from the tensor rather than from
    // the per-layer metadata array this reader skips.
    let kv_dim = tensor(&dir, &format!("blk.{}.attn_k.weight", interval - 1)).dims[1] as usize;

    let shape = Qwen36ShapeV1 {
        layer_types: (0..layers)
            .map(|i| if (i + 1).is_multiple_of(interval) { Qwen36LayerKind::FullAttention } else { Qwen36LayerKind::LinearAttention })
            .collect(),
        d_model,
        n_heads,
        n_kv_heads: kv_dim / head_dim,
        head_dim,
        rotary_dim,
        linear_k_heads,
        linear_v_heads,
        linear_head_dim,
        conv_kernel,
        n_experts,
        experts_per_token,
        moe_dim,
        shared_dim,
        vocab,
        // The rotary table is generated for this many positions; a conversion smoke test does not
        // need 262,144 of them and generating them costs more than it proves.
        max_position: 512,
        eps_q: 1,
        router_up_bits: 20,
    };
    println!(
        "qwen3.6: {layers}/{declared_layers} layers ({} linear, {} full), d {d_model}, heads {n_heads}/{} kv @ {head_dim} (rot {rotary_dim})",
        shape.layer_types.iter().filter(|k| **k == Qwen36LayerKind::LinearAttention).count(),
        shape.layer_types.iter().filter(|k| **k == Qwen36LayerKind::FullAttention).count(),
        shape.n_kv_heads
    );
    println!(
        "         gdn {linear_k_heads}k/{linear_v_heads}v @ {linear_head_dim} (inner {linear_v_dim}, conv {conv_kernel}), moe {experts_per_token}/{n_experts} @ {moe_dim} + shared {shared_dim}, vocab {vocab}"
    );

    let rope_base = dir.metadata.get("qwen35moe.rope.freq_base").and_then(|v| v.as_f64()).unwrap_or(10_000.0);
    let ln_theta = ((rope_base.ln()) * (1u128 << 50) as f64) as i128;
    let started = std::time::Instant::now();
    let rope = RopeTableV1::generate(head_dim, shape.max_position, ln_theta).unwrap_or_else(|e| die(format!("rotary table: {e:?}")));
    let mut artifact = Qwen36ArtifactV1::new(shape.clone(), rope).unwrap_or_else(|e| die(format!("shape: {e:?}")));
    println!("rotary table in {:?} (theta {rope_base})", started.elapsed());

    let started = std::time::Instant::now();
    let mut int8_bytes = 0usize;
    let add = |artifact: Qwen36ArtifactV1, name: String, values: &[f32], out_dim: usize, bytes: &mut usize| -> Qwen36ArtifactV1 {
        let (codes, _) = quantize_rows(values, out_dim);
        *bytes += codes.len();
        artifact.with_tensor(name, codes)
    };

    // The embedding and the output head.
    let embed = read_tensor(&mut file, tensor(&dir, "token_embd.weight"));
    artifact = add(artifact, "token_embd.weight".into(), &embed, vocab, &mut int8_bytes);
    drop(embed);
    let output = read_tensor(&mut file, tensor(&dir, "output.weight"));
    artifact = add(artifact, "output.weight".into(), &output, vocab, &mut int8_bytes);
    drop(output);
    artifact = artifact
        .with_params("embed_lift.a16", &[UNITY])
        .with_params("final_norm.a16", &[UNITY])
        .with_params("output.weight.a16", &[projection(d_model)]);

    for li in 0..layers {
        let g = |suffix: &str| format!("blk.{li}.{suffix}");
        let n = |suffix: &str| format!("blk.{li}.{suffix}");
        for row in ["attn_norm.a16", "attn_align.a16", "attn_residual.a16", "ffn_norm.a16", "ffn_align.a16", "ffn_residual.a16"] {
            artifact = artifact.with_params(n(row), &[UNITY]);
        }
        match shape.layer_types[li] {
            Qwen36LayerKind::LinearAttention => {
                let (dk, dv) = (shape.linear_k_dim(), shape.linear_v_dim());
                // The GGUF fuses q, k and v into one [d_model, 2·dk + dv] tensor; the engine reads
                // three, because a court opening addresses a tensor and a fused one would need an
                // offset convention on top of the name.
                let qkv = read_tensor(&mut file, tensor(&dir, &g("attn_qkv.weight")));
                let per = qkv.len() / (2 * dk + dv);
                artifact = add(artifact, n("linear_q.weight"), &qkv[..dk * per], dk, &mut int8_bytes);
                artifact = add(artifact, n("linear_k.weight"), &qkv[dk * per..2 * dk * per], dk, &mut int8_bytes);
                artifact = add(artifact, n("linear_v.weight"), &qkv[2 * dk * per..], dv, &mut int8_bytes);
                drop(qkv);
                let z = read_tensor(&mut file, tensor(&dir, &g("attn_gate.weight")));
                artifact = add(artifact, n("linear_z.weight"), &z, dv, &mut int8_bytes);
                drop(z);
                let conv = read_tensor(&mut file, tensor(&dir, &g("ssm_conv1d.weight")));
                artifact = add(artifact, n("linear_conv.weight"), &conv, conv.len() / conv_kernel, &mut int8_bytes);
                let dt = read_tensor(&mut file, tensor(&dir, &g("ssm_alpha.weight")));
                artifact = add(artifact, n("linear_dt.weight"), &dt, linear_v_heads, &mut int8_bytes);
                let beta = read_tensor(&mut file, tensor(&dir, &g("ssm_beta.weight")));
                artifact = add(artifact, n("linear_beta.weight"), &beta, linear_v_heads, &mut int8_bytes);
                let out = read_tensor(&mut file, tensor(&dir, &g("ssm_out.weight")));
                artifact = add(artifact, n("linear_o.weight"), &out, d_model, &mut int8_bytes);
                drop(out);
                // `ssm_a` is the per-head A_log; `decay = sigmoid(-dt)^exp(A_log)`.
                let a_log = read_tensor(&mut file, tensor(&dir, &g("ssm_a")));
                let one = kaspa_consensus_core::palw_base0::ONE;
                let decay_c: Vec<A16QuantParams> = a_log
                    .iter()
                    .map(|v| A16QuantParams { multiplier: 1, shift: 0, zero: ((v.exp() as f64) * one as f64) as i64 })
                    .collect();
                artifact = artifact
                    .with_params(n("linear_q.weight.a16"), &[projection(d_model)])
                    .with_params(n("linear_k.weight.a16"), &[projection(d_model)])
                    .with_params(n("linear_v.weight.a16"), &[projection(d_model)])
                    .with_params(n("linear_z.weight.a16"), &[projection(d_model)])
                    .with_params(n("linear_conv.a16"), &[A16QuantParams { multiplier: 1, shift: 16, zero: 0 }])
                    .with_params(n("linear_conv_act.a16"), &[UNITY])
                    .with_params(n("linear_dt.weight.a16"), &[projection(d_model)])
                    .with_params(n("linear_beta.weight.a16"), &[projection(d_model)])
                    .with_params(n("linear_decay_c.a16"), &decay_c)
                    .with_params(n("linear_read.a16"), &[A16QuantParams { multiplier: 1, shift: 23, zero: 0 }])
                    .with_params(n("linear_write.a16"), &[A16QuantParams { multiplier: 1, shift: 7, zero: 0 }])
                    .with_params(n("linear_out.a16"), &[A16QuantParams { multiplier: 1, shift: 23, zero: 0 }])
                    .with_params(n("linear_norm.a16"), &[UNITY])
                    .with_params(n("linear_gate.a16"), &[UNITY])
                    .with_params(n("linear_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                    .with_params(n("linear_o.weight.a16"), &[projection(dv)]);
            }
            Qwen36LayerKind::FullAttention => {
                let q_dim = n_heads * head_dim;
                // `attn_q` is double width: the first half is the query, the second the gate.
                let q = read_tensor(&mut file, tensor(&dir, &g("attn_q.weight")));
                let per = q.len() / (2 * q_dim);
                artifact = add(artifact, n("attn_q.weight"), &q[..q_dim * per], q_dim, &mut int8_bytes);
                artifact = add(artifact, n("attn_gate.weight"), &q[q_dim * per..], q_dim, &mut int8_bytes);
                drop(q);
                for (from, to, out_dim) in [
                    ("attn_k.weight", "attn_k.weight", kv_dim),
                    ("attn_v.weight", "attn_v.weight", kv_dim),
                    ("attn_output.weight", "attn_o.weight", d_model),
                ] {
                    let values = read_tensor(&mut file, tensor(&dir, &g(from)));
                    artifact = add(artifact, n(to), &values, out_dim, &mut int8_bytes);
                }
                artifact = artifact
                    .with_params(n("attn_q.weight.a16"), &[projection(d_model)])
                    .with_params(n("attn_gate.weight.a16"), &[projection(d_model)])
                    .with_params(n("attn_k.weight.a16"), &[projection(d_model)])
                    .with_params(n("attn_v.weight.a16"), &[projection(d_model)])
                    .with_params(n("attn_q_norm.a16"), &[UNITY])
                    .with_params(n("attn_k_norm.a16"), &[UNITY])
                    .with_params(n("attn_rope.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
                    .with_params(n("attn_logits.a16"), &[projection(head_dim)])
                    .with_params(n("attn_softmax_up.a16"), &[A16QuantParams { multiplier: 1, shift: 0, zero: 16 }])
                    .with_params(n("attn_probs.a16"), &[A16QuantParams { multiplier: 1, shift: 9, zero: 0 }])
                    .with_params(n("attn_values.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                    .with_params(n("attn_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
                    .with_params(n("attn_o.weight.a16"), &[projection(q_dim)]);
            }
        }

        // The mixture. The expert tensors are one block of 256 in the checkpoint and 256 rows in
        // the store, because the engine reads the eight the router chose and nothing else — which
        // is what makes the MoE the part a memory map serves best.
        let router = read_tensor(&mut file, tensor(&dir, &g("ffn_gate_inp.weight")));
        artifact = add(artifact, n("ffn_router.weight"), &router, n_experts, &mut int8_bytes);
        let shared_gate = read_tensor(&mut file, tensor(&dir, &g("ffn_gate_inp_shexp.weight")));
        artifact = add(artifact, n("ffn_shared_gate.weight"), &shared_gate, 1, &mut int8_bytes);
        artifact = artifact
            .with_params(n("ffn_router.weight.a16"), &[projection(d_model)])
            .with_params(n("ffn_router.a16"), &[UNITY])
            .with_params(n("ffn_combine.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
            .with_params(n("ffn_shared_gate.weight.a16"), &[projection(d_model)])
            .with_params(n("ffn_shared_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 24, zero: 0 }])
            .with_params(n("ffn_moe_out.a16"), &[UNITY]);

        for (gguf, base, mid, experts) in [
            ("ffn_gate_exps.weight", "_gate.weight", moe_dim, n_experts),
            ("ffn_up_exps.weight", "_up.weight", moe_dim, n_experts),
            ("ffn_down_exps.weight", "_down.weight", d_model, n_experts),
            ("ffn_gate_shexp.weight", "_gate.weight", shared_dim, 1),
            ("ffn_up_shexp.weight", "_up.weight", shared_dim, 1),
            ("ffn_down_shexp.weight", "_down.weight", d_model, 1),
        ] {
            let values = read_tensor(&mut file, tensor(&dir, &g(gguf)));
            let per_expert = values.len() / experts;
            for e in 0..experts {
                let name =
                    if experts == 1 { format!("blk.{li}.ffn_shared_expert{base}") } else { format!("blk.{li}.ffn_expert.{e}{base}") };
                artifact = add(artifact, name, &values[e * per_expert..(e + 1) * per_expert], mid, &mut int8_bytes);
            }
        }
        for e in 0..n_experts {
            let b = format!("blk.{li}.ffn_expert.{e}");
            artifact = artifact
                .with_params(format!("{b}_gate.weight.a16"), &[projection(d_model)])
                .with_params(format!("{b}_up.weight.a16"), &[projection(d_model)])
                .with_params(format!("{b}_silu.a16"), &[UNITY])
                .with_params(format!("{b}_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
                .with_params(format!("{b}_down.weight.a16"), &[projection(moe_dim)]);
        }
        let b = format!("blk.{li}.ffn_shared_expert");
        artifact = artifact
            .with_params(format!("{b}_gate.weight.a16"), &[projection(d_model)])
            .with_params(format!("{b}_up.weight.a16"), &[projection(d_model)])
            .with_params(format!("{b}_silu.a16"), &[UNITY])
            .with_params(format!("{b}_gated.a16"), &[A16QuantParams { multiplier: 1, shift: 15, zero: 0 }])
            .with_params(format!("{b}_down.weight.a16"), &[projection(shared_dim)]);
        println!("  layer {li} ({:?}) converted, {} MiB so far", shape.layer_types[li], int8_bytes >> 20);
    }
    println!("converted in {:?} ({} MiB of int8 weights)", started.elapsed(), int8_bytes >> 20);

    // Run it. The scales are not calibrated, so this checks that the graph executes on real
    // weights and how long it takes — not that it says anything sensible.
    let engine = Qwen36Engine::new(&artifact);
    let mut cache = Qwen36Cache::new(&artifact.shape);
    let started = std::time::Instant::now();
    let tokens = [9707usize, 11, 1879, 0, 3555, 374];
    let mut last = Vec::new();
    for (position, token) in tokens.iter().enumerate() {
        last = engine.forward_token(&mut cache, *token, position).unwrap_or_else(|e| die(format!("forward at {position}: {e}")));
    }
    let elapsed = started.elapsed();
    let nonzero = last.iter().filter(|v| **v != 0).count();
    let absmax = last.iter().map(|v| v.abs()).max().unwrap_or(0);
    let filled = cache.gdn.iter().flatten().filter(|s| s.s.iter().any(|v| *v != 0)).count();
    println!("forward {} tokens in {elapsed:?} ({:.1} ms/token)", tokens.len(), elapsed.as_secs_f64() * 1e3 / tokens.len() as f64);
    println!("  logits nonzero  {nonzero}/{}", last.len());
    println!("  logits absmax   {absmax}");
    println!("  gdn heads with state  {filled}");
    println!("  argmax          {}", misaka_palw_base0::engine::argmax_lowest(&last));
}
