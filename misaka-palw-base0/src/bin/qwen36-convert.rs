//! Convert a Qwen3.6 GGUF checkpoint into a `PALW-QWEN36` integer artifact.
//!
//! ```text
//! qwen36-convert --gguf <file.gguf> --out <artifact.palwq36> [--layers N]
//! qwen36-convert --url <https://…/model.gguf> --header <first-64MB> --out <artifact> [--layers N]
//! ```
//!
//! # Streaming, and why it is not an optimisation
//!
//! The checkpoint is 24 GiB and the artifact it produces is 33 GiB. A machine that has to hold
//! both has to have 57 GiB of disk free, and one that builds the artifact in memory has to have
//! 33 GiB of RAM. Neither is a reasonable requirement for producing a block.
//!
//! So the artifact's directory is computed from the SHAPE — every tensor's length is arithmetic —
//! written first, and the weights are appended as they are produced. And `--url` reads each source
//! tensor by HTTP range, so the checkpoint never lands on disk at all: peak disk is the artifact,
//! peak memory is the largest single tensor.
//!
//! # What the scales are
//!
//! Derived from each site's fan-in, not measured. That is the right SHAPE — a random dot product
//! grows like the square root of its length — and it is not a calibration. This produces an
//! artifact that RUNS; a float reference of the hybrid graph is what will make it faithful.

use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
use misaka_palw_base0::gguf::{GgufDirectory, GgufTensor, dequantize, parse_directory};
use misaka_palw_base0::qwen36::{Qwen36LayerKind, Qwen36ShapeV1, Qwen36Writer};
use misaka_palw_base0::rope::RopeTableV1;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

fn die(message: String) -> ! {
    eprintln!("qwen36-convert: {message}");
    std::process::exit(1)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(|s| s.as_str())
}

/// Where the checkpoint's bytes come from.
enum Source {
    Local(std::fs::File),
    /// `curl` by range. A subprocess rather than an HTTP crate: this is an offline tool, the
    /// surface used is one ranged GET, and a workspace that audits its dependencies should not
    /// gain a TLS stack for it.
    Url(String),
}

impl Source {
    fn read(&mut self, t: &GgufTensor) -> Vec<u8> {
        match self {
            Self::Local(file) => {
                let mut buf = vec![0u8; t.bytes];
                file.seek(SeekFrom::Start(t.offset)).unwrap_or_else(|e| die(format!("seek: {e}")));
                file.read_exact(&mut buf).unwrap_or_else(|e| die(format!("read {}: {e}", t.name)));
                buf
            }
            Self::Url(url) => {
                let (a, b) = t.range();
                let out = std::process::Command::new("curl")
                    .args(["-sS", "-L", "--max-time", "1800", "-r", &format!("{a}-{}", b - 1), url])
                    .output()
                    .unwrap_or_else(|e| die(format!("curl: {e}")));
                if !out.status.success() || out.stdout.len() != t.bytes {
                    die(format!("fetching {} returned {} bytes, wanted {}", t.name, out.stdout.len(), t.bytes));
                }
                out.stdout
            }
        }
    }
}

/// Per-output-channel symmetric int8 quantization.
fn quantize_rows(values: &[f32], out_dim: usize) -> Vec<i8> {
    let n = values.len() / out_dim.max(1);
    let mut codes = Vec::with_capacity(values.len());
    for c in 0..out_dim {
        let row = &values[c * n..(c + 1) * n];
        let absmax = row.iter().fold(0f32, |a, v| a.max(v.abs())) as f64;
        let scale = if absmax > 0.0 { absmax / 127.0 } else { 1.0 };
        for v in row {
            codes.push((*v as f64 / scale).round().clamp(-127.0, 127.0) as i8);
        }
    }
    codes
}

fn projection(fan_in: usize) -> A16QuantParams {
    let bits = usize::BITS - fan_in.max(1).leading_zeros();
    A16QuantParams { multiplier: 1, shift: (8 + bits / 2) as u8, zero: 0 }
}

const UNITY: A16QuantParams = A16QuantParams { multiplier: 1, shift: 0, zero: 0 };

fn wire(rows: &[A16QuantParams]) -> Vec<u8> {
    rows.iter().flat_map(|p| p.to_wire()).collect()
}

/// One source tensor and the artifact tensors it produces: `(name, out_dim, element range)`.
struct Group {
    source: String,
    parts: Vec<(String, usize, std::ops::Range<usize>)>,
}

fn tensor<'a>(dir: &'a GgufDirectory, name: &str) -> &'a GgufTensor {
    dir.tensors.get(name).unwrap_or_else(|| die(format!("the checkpoint has no tensor {name}")))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_path = flag(&args, "--out")
        .unwrap_or_else(|| die("usage: qwen36-convert (--gguf FILE | --url URL --header FILE) --out FILE [--layers N]".into()));
    let layer_cap: Option<usize> = flag(&args, "--layers").and_then(|v| v.parse().ok());

    // The directory lives in the first few tens of megabytes.
    let (mut source, header) = match (flag(&args, "--gguf"), flag(&args, "--url")) {
        (Some(path), _) => {
            let mut file = std::fs::File::open(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
            let mut head = vec![0u8; 64 << 20];
            let n = file.read(&mut head).unwrap_or_else(|e| die(format!("{path}: {e}")));
            head.truncate(n);
            (Source::Local(file), head)
        }
        (None, Some(url)) => {
            let header = flag(&args, "--header").unwrap_or_else(|| die("--url needs --header <first-64MB-of-the-file>".into()));
            let head = std::fs::read(header).unwrap_or_else(|e| die(format!("{header}: {e}")));
            (Source::Url(url.to_string()), head)
        }
        _ => die("one of --gguf or --url is required".into()),
    };
    let dir = parse_directory(&header).unwrap_or_else(|e| die(format!("header: {e}")));

    let meta = |k: &str| -> u64 { dir.metadata.get(k).and_then(|v| v.as_u64()).unwrap_or_else(|| die(format!("no metadata {k}"))) };
    if dir.metadata.get("general.architecture").and_then(|v| v.as_str()) != Some("qwen35moe") {
        die("this converter reads qwen35moe checkpoints".into());
    }
    let declared = meta("qwen35moe.block_count") as usize;
    let layers = layer_cap.unwrap_or(declared).min(declared);
    let interval = meta("qwen35moe.full_attention_interval") as usize;
    let d = meta("qwen35moe.embedding_length") as usize;
    let head_dim = meta("qwen35moe.attention.key_length") as usize;
    let n_heads = meta("qwen35moe.attention.head_count") as usize;
    let rotary_dim = meta("qwen35moe.rope.dimension_count") as usize;
    let linear_head_dim = meta("qwen35moe.ssm.state_size") as usize;
    let linear_k_heads = meta("qwen35moe.ssm.group_count") as usize;
    let linear_v_heads = meta("qwen35moe.ssm.time_step_rank") as usize;
    let conv_kernel = meta("qwen35moe.ssm.conv_kernel") as usize;
    let n_experts = meta("qwen35moe.expert_count") as usize;
    let experts_per_token = meta("qwen35moe.expert_used_count") as usize;
    let moe_dim = meta("qwen35moe.expert_feed_forward_length") as usize;
    let shared_dim = meta("qwen35moe.expert_shared_feed_forward_length") as usize;
    let vocab = tensor(&dir, "token_embd.weight").dims[1] as usize;
    let kv_dim = tensor(&dir, &format!("blk.{}.attn_k.weight", interval - 1)).dims[1] as usize;
    let max_position: usize = flag(&args, "--context").and_then(|v| v.parse().ok()).unwrap_or(512);

    let shape = Qwen36ShapeV1 {
        layer_types: (0..layers)
            .map(|i| if (i + 1).is_multiple_of(interval) { Qwen36LayerKind::FullAttention } else { Qwen36LayerKind::LinearAttention })
            .collect(),
        d_model: d,
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
        max_position,
        eps_q: 1,
        router_up_bits: 20,
    };
    let (dk, dv, q_dim) = (shape.linear_k_dim(), shape.linear_v_dim(), n_heads * head_dim);
    println!(
        "qwen3.6: {layers}/{declared} layers ({} linear, {} full), d {d}, heads {n_heads}/{} kv @ {head_dim} (rot {rotary_dim})",
        shape.layer_types.iter().filter(|k| **k == Qwen36LayerKind::LinearAttention).count(),
        shape.layer_types.iter().filter(|k| **k == Qwen36LayerKind::FullAttention).count(),
        shape.n_kv_heads
    );
    println!(
        "         gdn {linear_k_heads}k/{linear_v_heads}v @ {linear_head_dim} (inner {dv}), moe {experts_per_token}/{n_experts} @ {moe_dim} + shared {shared_dim}, vocab {vocab}, ctx {max_position}"
    );

    // ---- the plan: every artifact tensor, in the order it will be written ------------------
    let mut groups: Vec<Group> = Vec::new();
    let mut params: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let whole = |n: usize| 0..n;

    groups.push(Group { source: "token_embd.weight".into(), parts: vec![("token_embd.weight".into(), vocab, whole(vocab * d))] });
    groups.push(Group { source: "output.weight".into(), parts: vec![("output.weight".into(), vocab, whole(vocab * d))] });
    params.insert("embed_lift.a16".into(), wire(&[UNITY]));
    params.insert("final_norm.a16".into(), wire(&[UNITY]));
    params.insert("output.weight.a16".into(), wire(&[projection(d)]));

    for li in 0..layers {
        let g = |s: &str| format!("blk.{li}.{s}");
        for row in ["attn_norm.a16", "attn_align.a16", "attn_residual.a16", "ffn_norm.a16", "ffn_align.a16", "ffn_residual.a16"] {
            params.insert(g(row), wire(&[UNITY]));
        }
        match shape.layer_types[li] {
            Qwen36LayerKind::LinearAttention => {
                groups.push(Group {
                    source: g("attn_qkv.weight"),
                    parts: vec![
                        (g("linear_q.weight"), dk, 0..dk * d),
                        (g("linear_k.weight"), dk, dk * d..2 * dk * d),
                        (g("linear_v.weight"), dv, 2 * dk * d..(2 * dk + dv) * d),
                    ],
                });
                groups.push(Group { source: g("attn_gate.weight"), parts: vec![(g("linear_z.weight"), dv, whole(dv * d))] });
                groups.push(Group {
                    source: g("ssm_conv1d.weight"),
                    parts: vec![(g("linear_conv.weight"), 2 * dk + dv, whole(conv_kernel * (2 * dk + dv)))],
                });
                groups.push(Group {
                    source: g("ssm_alpha.weight"),
                    parts: vec![(g("linear_dt.weight"), linear_v_heads, whole(linear_v_heads * d))],
                });
                groups.push(Group {
                    source: g("ssm_beta.weight"),
                    parts: vec![(g("linear_beta.weight"), linear_v_heads, whole(linear_v_heads * d))],
                });
                groups.push(Group { source: g("ssm_out.weight"), parts: vec![(g("linear_o.weight"), d, whole(d * dv))] });
                for (name, p) in [
                    ("linear_q.weight.a16", projection(d)),
                    ("linear_k.weight.a16", projection(d)),
                    ("linear_v.weight.a16", projection(d)),
                    ("linear_z.weight.a16", projection(d)),
                    ("linear_conv.a16", A16QuantParams { multiplier: 1, shift: 16, zero: 0 }),
                    ("linear_conv_act.a16", UNITY),
                    ("linear_dt.weight.a16", projection(d)),
                    ("linear_beta.weight.a16", projection(d)),
                    ("linear_read.a16", A16QuantParams { multiplier: 1, shift: 23, zero: 0 }),
                    ("linear_write.a16", A16QuantParams { multiplier: 1, shift: 7, zero: 0 }),
                    ("linear_out.a16", A16QuantParams { multiplier: 1, shift: 23, zero: 0 }),
                    ("linear_norm.a16", UNITY),
                    ("linear_gate.a16", UNITY),
                    ("linear_gated.a16", A16QuantParams { multiplier: 1, shift: 15, zero: 0 }),
                    ("linear_o.weight.a16", projection(dv)),
                ] {
                    params.insert(g(name), wire(&[p]));
                }
            }
            Qwen36LayerKind::FullAttention => {
                groups.push(Group {
                    source: g("attn_q.weight"),
                    parts: vec![(g("attn_q.weight"), q_dim, 0..q_dim * d), (g("attn_gate.weight"), q_dim, q_dim * d..2 * q_dim * d)],
                });
                groups.push(Group { source: g("attn_k.weight"), parts: vec![(g("attn_k.weight"), kv_dim, whole(kv_dim * d))] });
                groups.push(Group { source: g("attn_v.weight"), parts: vec![(g("attn_v.weight"), kv_dim, whole(kv_dim * d))] });
                groups.push(Group { source: g("attn_output.weight"), parts: vec![(g("attn_o.weight"), d, whole(d * q_dim))] });
                for (name, p) in [
                    ("attn_q.weight.a16", projection(d)),
                    ("attn_gate.weight.a16", projection(d)),
                    ("attn_k.weight.a16", projection(d)),
                    ("attn_v.weight.a16", projection(d)),
                    ("attn_q_norm.a16", UNITY),
                    ("attn_k_norm.a16", UNITY),
                    ("attn_rope.a16", A16QuantParams { multiplier: 1, shift: 24, zero: 0 }),
                    ("attn_logits.a16", projection(head_dim)),
                    ("attn_softmax_up.a16", A16QuantParams { multiplier: 1, shift: 0, zero: 16 }),
                    ("attn_probs.a16", A16QuantParams { multiplier: 1, shift: 9, zero: 0 }),
                    ("attn_values.a16", A16QuantParams { multiplier: 1, shift: 15, zero: 0 }),
                    ("attn_gated.a16", A16QuantParams { multiplier: 1, shift: 24, zero: 0 }),
                    ("attn_o.weight.a16", projection(q_dim)),
                ] {
                    params.insert(g(name), wire(&[p]));
                }
            }
        }
        groups
            .push(Group { source: g("ffn_gate_inp.weight"), parts: vec![(g("ffn_router.weight"), n_experts, whole(n_experts * d))] });
        groups.push(Group { source: g("ffn_gate_inp_shexp.weight"), parts: vec![(g("ffn_shared_gate.weight"), 1, whole(d))] });
        for (gguf, suffix, mid) in [
            ("ffn_gate_exps.weight", "_gate.weight", moe_dim),
            ("ffn_up_exps.weight", "_up.weight", moe_dim),
            ("ffn_down_exps.weight", "_down.weight", d),
        ] {
            let per = mid * if suffix == "_down.weight" { moe_dim } else { d };
            groups.push(Group {
                source: g(gguf),
                parts: (0..n_experts).map(|e| (format!("blk.{li}.ffn_expert.{e}{suffix}"), mid, e * per..(e + 1) * per)).collect(),
            });
        }
        for (gguf, suffix, mid, fan) in [
            ("ffn_gate_shexp.weight", "_gate.weight", shared_dim, d),
            ("ffn_up_shexp.weight", "_up.weight", shared_dim, d),
            ("ffn_down_shexp.weight", "_down.weight", d, shared_dim),
        ] {
            groups
                .push(Group { source: g(gguf), parts: vec![(format!("blk.{li}.ffn_shared_expert{suffix}"), mid, whole(mid * fan))] });
        }
        for (name, p) in [
            ("ffn_router.weight.a16", projection(d)),
            ("ffn_router.a16", UNITY),
            ("ffn_combine.a16", A16QuantParams { multiplier: 1, shift: 24, zero: 0 }),
            ("ffn_shared_gate.weight.a16", projection(d)),
            ("ffn_shared_gated.a16", A16QuantParams { multiplier: 1, shift: 24, zero: 0 }),
            ("ffn_moe_out.a16", UNITY),
        ] {
            params.insert(g(name), wire(&[p]));
        }
        for e in 0..n_experts {
            let b = format!("blk.{li}.ffn_expert.{e}");
            for (name, p) in [
                ("_gate.weight.a16", projection(d)),
                ("_up.weight.a16", projection(d)),
                ("_silu.a16", UNITY),
                ("_gated.a16", A16QuantParams { multiplier: 1, shift: 15, zero: 0 }),
                ("_down.weight.a16", projection(moe_dim)),
            ] {
                params.insert(format!("{b}{name}"), wire(&[p]));
            }
        }
        let b = format!("blk.{li}.ffn_shared_expert");
        for (name, p) in [
            ("_gate.weight.a16", projection(d)),
            ("_up.weight.a16", projection(d)),
            ("_silu.a16", UNITY),
            ("_gated.a16", A16QuantParams { multiplier: 1, shift: 15, zero: 0 }),
            ("_down.weight.a16", projection(shared_dim)),
        ] {
            params.insert(format!("{b}{name}"), wire(&[p]));
        }
    }

    // `ssm_a` is the per-head A_log; it is a PARAMETER, not a weight, so it is read now.
    for li in 0..layers {
        if shape.layer_types[li] != Qwen36LayerKind::LinearAttention {
            continue;
        }
        let t = tensor(&dir, &format!("blk.{li}.ssm_a")).clone();
        let values = dequantize(&t, &source.read(&t)).unwrap_or_else(|e| die(format!("ssm_a: {e}")));
        let one = kaspa_consensus_core::palw_base0::ONE;
        let rows: Vec<A16QuantParams> =
            values.iter().map(|v| A16QuantParams { multiplier: 1, shift: 0, zero: ((v.exp() as f64) * one as f64) as i64 }).collect();
        params.insert(format!("blk.{li}.linear_decay_c.a16"), wire(&rows));
    }

    let plan: Vec<(String, usize)> =
        groups.iter().flat_map(|g| g.parts.iter().map(|(name, _, range)| (name.clone(), range.len()))).collect();
    let total: usize = plan.iter().map(|(_, n)| n).sum();
    println!("plan: {} tensors, {:.2} GiB of int8 weights", plan.len(), total as f64 / (1u64 << 30) as f64);

    let rope_base = dir.metadata.get("qwen35moe.rope.freq_base").and_then(|v| v.as_f64()).unwrap_or(10_000.0);
    let ln_theta = (rope_base.ln() * (1u128 << 50) as f64) as i128;
    let rope = RopeTableV1::generate(head_dim, max_position, ln_theta).unwrap_or_else(|e| die(format!("rotary table: {e:?}")));

    let started = std::time::Instant::now();
    let mut writer = Qwen36Writer::create(std::path::Path::new(out_path), &shape, &rope, &params, plan)
        .unwrap_or_else(|e| die(format!("{out_path}: {e}")));
    let mut done = 0usize;
    for group in &groups {
        let t = tensor(&dir, &group.source).clone();
        let bytes = source.read(&t);
        let values = dequantize(&t, &bytes).unwrap_or_else(|e| die(format!("{}: {e}", group.source)));
        drop(bytes);
        for (name, out_dim, range) in &group.parts {
            if range.end > values.len() {
                die(format!("{} is {} values and {name} wants {:?}", group.source, values.len(), range));
            }
            let codes = quantize_rows(&values[range.clone()], *out_dim);
            writer.push(name, &codes).unwrap_or_else(|e| die(format!("writing {name}: {e}")));
            done += codes.len();
        }
        if group.source.ends_with("ffn_down_exps.weight") || group.parts.len() == 1 && group.source.contains("token_embd") {
            println!("  {:.1}%  {}", done as f64 / total as f64 * 100.0, group.source);
        }
    }
    let written = writer.finish().unwrap_or_else(|e| die(format!("closing {out_path}: {e}")));
    println!("wrote {out_path}: {:.2} GiB in {:?}", written as f64 / (1u64 << 30) as f64, started.elapsed());
}
