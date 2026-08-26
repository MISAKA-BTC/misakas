//! Convert a Qwen3.6 GGUF checkpoint into a calibrated `PALW-QWEN36` integer artifact.
//!
//! ```text
//! qwen36-convert --gguf FILE   --out ARTIFACT [--layers N] [--context N] [--prompt ids]
//! qwen36-convert --url URL --header FIRST64MB --out ARTIFACT [--layers N] …
//! ```
//!
//! # One pass, three jobs
//!
//! The checkpoint is 24 GiB and the artifact is 33 GiB, so neither can be read twice for free and
//! neither fits in memory. This reads each tensor once and does everything that needs it while it
//! is in hand:
//!
//! 1. **quantize** it per output channel and append the codes — the directory was written first,
//!    from the shape, so the weights stream;
//! 2. **run the `f32` reference** for every prompt position through that layer, layer-major, which
//!    is why the reference takes borrowed weights rather than fetching its own;
//! 3. **derive that layer's A16 triples** from the weight row scales, the norm gains γ, and the
//!    ranges step 2 just measured.
//!
//! The triples are patched into the header at the end. That is sound because the codes do not
//! depend on them: a code is a per-output-channel property of the weight and nothing else, while a
//! scale is a statement about a range that only running the model can supply.
//!
//! `--url` fetches each tensor by HTTP range, so the checkpoint never lands on disk. Peak disk is
//! the artifact; peak memory is one layer's tensors.

use kaspa_consensus_core::palw_base0::K;
use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
use misaka_palw_base0::gguf::{GgufDirectory, GgufTensor, dequantize, parse_directory};
use misaka_palw_base0::qwen36::{Qwen36LayerKind, Qwen36ShapeV1, Qwen36Writer};
use misaka_palw_base0::qwen36_calibrate as cal;
use misaka_palw_base0::qwen36_reference as reference;
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

enum Source {
    Local(std::fs::File),
    /// `curl` by range. A subprocess rather than an HTTP crate: this is an offline tool, the
    /// surface used is one ranged GET, and a workspace that audits its dependencies should not
    /// gain a TLS stack for it.
    Url(String),
}

impl Source {
    fn bytes(&mut self, t: &GgufTensor) -> Vec<u8> {
        match self {
            Self::Local(file) => {
                let mut buf = vec![0u8; t.bytes];
                file.seek(SeekFrom::Start(t.offset)).unwrap_or_else(|e| die(format!("seek: {e}")));
                file.read_exact(&mut buf).unwrap_or_else(|e| die(format!("read {}: {e}", t.name)));
                buf
            }
            Self::Url(url) => {
                let (a, b) = t.range();
                for attempt in 0..3 {
                    let out = std::process::Command::new("curl")
                        .args(["-sS", "-L", "--max-time", "1800", "-r", &format!("{a}-{}", b - 1), url])
                        .output()
                        .unwrap_or_else(|e| die(format!("curl: {e}")));
                    if out.status.success() && out.stdout.len() == t.bytes {
                        return out.stdout;
                    }
                    eprintln!("  retrying {} (attempt {})", t.name, attempt + 1);
                }
                die(format!("fetching {} failed three times", t.name))
            }
        }
    }
}

struct Reader<'a> {
    dir: &'a GgufDirectory,
    source: Source,
}

impl Reader<'_> {
    fn get(&mut self, name: &str) -> Vec<f32> {
        let t = self.dir.tensors.get(name).unwrap_or_else(|| die(format!("the checkpoint has no tensor {name}"))).clone();
        let bytes = self.source.bytes(&t);
        dequantize(&t, &bytes).unwrap_or_else(|e| die(format!("{name}: {e}")))
    }
}

/// Per-output-channel symmetric int8 quantization: the codes, and each row's scale.
fn quantize_rows(values: &[f32], out_dim: usize) -> (Vec<i8>, Vec<f64>) {
    let n = values.len() / out_dim.max(1);
    let mut codes = Vec::with_capacity(values.len());
    let mut scales = Vec::with_capacity(out_dim);
    for c in 0..out_dim {
        let row = &values[c * n..(c + 1) * n];
        let absmax = row.iter().fold(0f32, |a, v| a.max(v.abs())) as f64;
        // A row of zeros gets a scale of one rather than of zero: the codes are all zero either
        // way, and a zero scale would make the narrowing's gain zero for a channel that might be
        // non-zero in a later revision of the checkpoint.
        let scale = if absmax > 0.0 { absmax / 127.0 } else { 1.0 };
        scales.push(scale);
        for v in row {
            codes.push((*v as f64 / scale).round().clamp(-127.0, 127.0) as i8);
        }
    }
    (codes, scales)
}

fn wire(rows: &[A16QuantParams]) -> Vec<u8> {
    rows.iter().flat_map(|p| p.to_wire()).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_path = flag(&args, "--out")
        .unwrap_or_else(|| die("usage: qwen36-convert (--gguf FILE | --url URL --header FILE) --out FILE [--layers N]".into()));
    let layer_cap: Option<usize> = flag(&args, "--layers").and_then(|v| v.parse().ok());
    let max_position: usize = flag(&args, "--context").and_then(|v| v.parse().ok()).unwrap_or(512);

    let (source, header) = match (flag(&args, "--gguf"), flag(&args, "--url")) {
        (Some(path), _) => {
            let mut file = std::fs::File::open(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
            let mut head = vec![0u8; 64 << 20];
            let n = file.read(&mut head).unwrap_or_else(|e| die(format!("{path}: {e}")));
            head.truncate(n);
            (Source::Local(file), head)
        }
        (None, Some(url)) => {
            let h = flag(&args, "--header").unwrap_or_else(|| die("--url needs --header <first-64MB-of-the-file>".into()));
            (Source::Url(url.to_string()), std::fs::read(h).unwrap_or_else(|e| die(format!("{h}: {e}"))))
        }
        _ => die("one of --gguf or --url is required".into()),
    };
    let dir = parse_directory(&header).unwrap_or_else(|e| die(format!("header: {e}")));
    let mut reader = Reader { dir: &dir, source };

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
    let vocab = dir.tensors.get("token_embd.weight").map(|t| t.dims[1] as usize).unwrap_or_else(|| die("no token_embd".into()));
    let kv_dim = dir
        .tensors
        .get(&format!("blk.{}.attn_k.weight", interval - 1))
        .map(|t| t.dims[1] as usize)
        .unwrap_or_else(|| die("no full-attention layer in the checkpoint".into()));
    let eps = dir.metadata.get("qwen35moe.attention.layer_norm_rms_epsilon").and_then(|v| v.as_f64()).unwrap_or(1e-6) as f32;

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
    let width = 2 * dk + dv;
    println!(
        "qwen3.6: {layers}/{declared} layers ({} linear, {} full), d {d}, heads {n_heads}/{} kv @ {head_dim} (rot {rotary_dim})",
        shape.layer_types.iter().filter(|k| **k == Qwen36LayerKind::LinearAttention).count(),
        shape.layer_types.iter().filter(|k| **k == Qwen36LayerKind::FullAttention).count(),
        shape.n_kv_heads
    );
    println!(
        "         gdn {linear_k_heads}k/{linear_v_heads}v @ {linear_head_dim} (inner {dv}), moe {experts_per_token}/{n_experts} @ {moe_dim} + shared {shared_dim}, vocab {vocab}, ctx {max_position}, eps {eps:e}"
    );

    // The calibration prompt. Short on purpose: what is being measured is each site's magnitude,
    // and a longer prompt costs a linear amount of reference time for a range that stops moving.
    let prompt: Vec<usize> = flag(&args, "--prompt")
        .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![9707, 11, 1879, 0, 3555, 374, 279, 6722]);
    println!("calibration prompt: {} positions", prompt.len());

    // ---- the tensor plan, in the order it will be written ----------------------------------
    let mut plan: Vec<(String, usize)> = vec![("token_embd.weight".into(), vocab * d)];
    for li in 0..layers {
        let g = |s: &str| format!("blk.{li}.{s}");
        match shape.layer_types[li] {
            Qwen36LayerKind::LinearAttention => {
                plan.push((g("linear_q.weight"), dk * d));
                plan.push((g("linear_k.weight"), dk * d));
                plan.push((g("linear_v.weight"), dv * d));
                plan.push((g("linear_z.weight"), dv * d));
                plan.push((g("linear_conv.weight"), conv_kernel * width));
                plan.push((g("linear_dt.weight"), linear_v_heads * d));
                plan.push((g("linear_beta.weight"), linear_v_heads * d));
                plan.push((g("linear_o.weight"), d * dv));
            }
            Qwen36LayerKind::FullAttention => {
                plan.push((g("attn_q.weight"), q_dim * d));
                plan.push((g("attn_gate.weight"), q_dim * d));
                plan.push((g("attn_k.weight"), kv_dim * d));
                plan.push((g("attn_v.weight"), kv_dim * d));
                plan.push((g("attn_o.weight"), d * q_dim));
            }
        }
        plan.push((g("ffn_router.weight"), n_experts * d));
        plan.push((g("ffn_shared_gate.weight"), d));
        for e in 0..n_experts {
            plan.push((format!("blk.{li}.ffn_expert.{e}_gate.weight"), moe_dim * d));
            plan.push((format!("blk.{li}.ffn_expert.{e}_up.weight"), moe_dim * d));
            plan.push((format!("blk.{li}.ffn_expert.{e}_down.weight"), d * moe_dim));
        }
        plan.push((format!("blk.{li}.ffn_shared_expert_gate.weight"), shared_dim * d));
        plan.push((format!("blk.{li}.ffn_shared_expert_up.weight"), shared_dim * d));
        plan.push((format!("blk.{li}.ffn_shared_expert_down.weight"), d * shared_dim));
    }
    // Last, because the reference needs it only after every layer has run.
    plan.push(("output.weight".into(), vocab * d));
    let total: usize = plan.iter().map(|(_, n)| n).sum();
    println!("plan: {} tensors, {:.2} GiB of int8 weights", plan.len(), total as f64 / (1u64 << 30) as f64);

    // ---- placeholder parameters, at their final widths -------------------------------------
    let unity = A16QuantParams { multiplier: 1, shift: 0, zero: 0 };
    let mut params: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let place = |params: &mut BTreeMap<String, Vec<u8>>, name: String, rows: usize| {
        params.insert(name, wire(&vec![unity; rows]));
    };
    place(&mut params, "embed_lift.a16".into(), vocab);
    place(&mut params, "final_norm.a16".into(), d);
    place(&mut params, "output.weight.a16".into(), vocab);
    for li in 0..layers {
        let g = |s: &str| format!("blk.{li}.{s}");
        for (name, rows) in [
            ("attn_norm.a16", d),
            ("attn_align.a16", d),
            ("attn_residual.a16", d),
            ("ffn_norm.a16", d),
            ("ffn_align.a16", d),
            ("ffn_residual.a16", d),
            ("ffn_router.weight.a16", n_experts),
            ("ffn_router.a16", n_experts),
            ("ffn_router_up.a16", 1),
            ("ffn_combine.a16", 1),
            ("ffn_shared_gate.weight.a16", 1),
            ("ffn_shared_gated.a16", 1),
            ("ffn_moe_out.a16", d),
        ] {
            place(&mut params, g(name), rows);
        }
        match shape.layer_types[li] {
            Qwen36LayerKind::LinearAttention => {
                for (name, rows) in [
                    ("linear_q.weight.a16", dk),
                    ("linear_k.weight.a16", dk),
                    ("linear_v.weight.a16", dv),
                    ("linear_z.weight.a16", dv),
                    ("linear_conv.a16", width),
                    ("linear_conv_act.a16", width),
                    ("linear_dt.weight.a16", linear_v_heads),
                    ("linear_beta.weight.a16", linear_v_heads),
                    ("linear_decay_c.a16", linear_v_heads),
                    ("linear_read.a16", linear_v_heads),
                    ("linear_write.a16", linear_v_heads),
                    ("linear_out.a16", linear_v_heads),
                    ("linear_norm.a16", dv),
                    ("linear_gate.a16", dv),
                    ("linear_gated.a16", dv),
                    ("linear_o.weight.a16", d),
                ] {
                    place(&mut params, g(name), rows);
                }
            }
            Qwen36LayerKind::FullAttention => {
                for (name, rows) in [
                    ("attn_q.weight.a16", q_dim),
                    ("attn_gate.weight.a16", q_dim),
                    ("attn_k.weight.a16", kv_dim),
                    ("attn_v.weight.a16", kv_dim),
                    ("attn_q_norm.a16", head_dim),
                    ("attn_k_norm.a16", head_dim),
                    ("attn_rope.a16", 1),
                    ("attn_logits.a16", 1),
                    ("attn_softmax_up.a16", 1),
                    ("attn_probs.a16", 1),
                    ("attn_values.a16", 1),
                    ("attn_gated.a16", 1),
                    ("attn_o.weight.a16", d),
                ] {
                    place(&mut params, g(name), rows);
                }
            }
        }
        for e in 0..n_experts {
            let b = format!("blk.{li}.ffn_expert.{e}");
            for (suffix, rows) in [
                ("_gate.weight.a16", moe_dim),
                ("_up.weight.a16", moe_dim),
                ("_silu.a16", moe_dim),
                ("_gated.a16", moe_dim),
                ("_down.weight.a16", d),
            ] {
                place(&mut params, format!("{b}{suffix}"), rows);
            }
        }
        let b = format!("blk.{li}.ffn_shared_expert");
        for (suffix, rows) in [
            ("_gate.weight.a16", shared_dim),
            ("_up.weight.a16", shared_dim),
            ("_silu.a16", shared_dim),
            ("_gated.a16", shared_dim),
            ("_down.weight.a16", d),
        ] {
            place(&mut params, format!("{b}{suffix}"), rows);
        }
    }

    let rope_base = dir.metadata.get("qwen35moe.rope.freq_base").and_then(|v| v.as_f64()).unwrap_or(10_000.0);
    let ln_theta = (rope_base.ln() * (1u128 << 50) as f64) as i128;
    let rope = RopeTableV1::generate(head_dim, max_position, ln_theta).unwrap_or_else(|e| die(format!("rotary table: {e:?}")));
    // The reference rotates by the pinned table's angles, converted, rather than by the ones a
    // float implementation would have chosen — so the comparison is about quantization and not
    // about two different models.
    let pairs = rotary_dim / 2;
    let rope_f32: Vec<(Vec<f32>, Vec<f32>)> = (0..prompt.len().min(max_position))
        .map(|p| {
            let (c, s) = rope.row(p).unwrap_or_else(|| die("the rotary table is short".into()));
            let q = (1u64 << K) as f32;
            (c[..pairs].iter().map(|v| *v as f32 / q).collect(), s[..pairs].iter().map(|v| *v as f32 / q).collect())
        })
        .collect();

    let started = std::time::Instant::now();
    let mut writer = Qwen36Writer::create(std::path::Path::new(out_path), &shape, &rope, &params, plan)
        .unwrap_or_else(|e| die(format!("{out_path}: {e}")));
    let mut measured: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut calibration = reference::Qwen36CalibrationV1::default();
    let mut state = reference::Qwen36RefState::new(&shape);

    // ---- the embedding, and the stream's first exponent ------------------------------------
    let embed_f = reader.get("token_embd.weight");
    // Per row, which is per token: one scale for the whole table is one scale for its outliers,
    // and the ordinary rows a prompt actually uses then start on a fraction of the int8 range.
    let (embed_codes, embed_scales) = quantize_rows(&embed_f, vocab);
    writer.push("token_embd.weight", &embed_codes).unwrap_or_else(|e| die(format!("token_embd: {e}")));
    drop(embed_codes);
    let mut h: Vec<Vec<f32>> = prompt.iter().map(|t| embed_f[t * d..(t + 1) * d].to_vec()).collect();
    drop(embed_f);
    let e_stream = cal::site_exponent(h.iter().flatten().fold(0f32, |a, v| a.max(v.abs())) as f64);
    measured
        .insert("embed_lift.a16".into(), wire(&embed_scales.iter().map(|s| cal::triple(s * 2f64.powi(e_stream))).collect::<Vec<_>>()));
    let mut e_stream = e_stream;

    let site = |c: &reference::Qwen36CalibrationV1, name: &str| -> f64 { c.sites.get(name).map(|r| r.absmax).unwrap_or(0.0) };

    for li in 0..layers {
        let g = |s: &str| format!("blk.{li}.{s}");
        let attn_gain = reader.get(&g("attn_norm.weight"));
        let post_gain = reader.get(&g("post_attention_norm.weight"));

        // The arm's weights, quantized and written as they arrive, with their row scales kept.
        let mut scales: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let push = |writer: &mut Qwen36Writer, scales: &mut BTreeMap<String, Vec<f64>>, name: &str, values: &[f32], out_dim: usize| {
            let (codes, s) = quantize_rows(values, out_dim);
            writer.push(name, &codes).unwrap_or_else(|e| die(format!("writing {name}: {e}")));
            scales.insert(name.to_string(), s);
        };

        let e_stream_in = e_stream;
        match shape.layer_types[li] {
            Qwen36LayerKind::LinearAttention => {
                let qkv = reader.get(&g("attn_qkv.weight"));
                push(&mut writer, &mut scales, &g("linear_q.weight"), &qkv[..dk * d], dk);
                push(&mut writer, &mut scales, &g("linear_k.weight"), &qkv[dk * d..2 * dk * d], dk);
                push(&mut writer, &mut scales, &g("linear_v.weight"), &qkv[2 * dk * d..], dv);
                let z = reader.get(&g("attn_gate.weight"));
                push(&mut writer, &mut scales, &g("linear_z.weight"), &z, dv);
                let conv = reader.get(&g("ssm_conv1d.weight"));
                push(&mut writer, &mut scales, &g("linear_conv.weight"), &conv, width);
                let alpha = reader.get(&g("ssm_alpha.weight"));
                push(&mut writer, &mut scales, &g("linear_dt.weight"), &alpha, linear_v_heads);
                let beta = reader.get(&g("ssm_beta.weight"));
                push(&mut writer, &mut scales, &g("linear_beta.weight"), &beta, linear_v_heads);
                let out = reader.get(&g("ssm_out.weight"));
                push(&mut writer, &mut scales, &g("linear_o.weight"), &out, d);
                let a_log = reader.get(&g("ssm_a"));
                let ssm_norm = reader.get(&g("ssm_norm.weight"));

                let w = reference::LinearWeights {
                    attn_norm: &attn_gain,
                    post_norm: &post_gain,
                    qkv: &qkv,
                    gate: &z,
                    conv: &conv,
                    alpha: &alpha,
                    beta: &beta,
                    a_log: &a_log,
                    ssm_norm: &ssm_norm,
                    out: &out,
                };
                reference::reference_linear_layer(&shape, &mut state, &mut calibration, li, &w, &mut h, eps);

                // ---- the triples, from what that run measured ------------------------------
                let e_normed = cal::site_exponent(site(&calibration, &g("attn_norm")));
                let e_qkv = cal::site_exponent(site(&calibration, &g("linear_qkv")));
                let e_conv_act = cal::site_exponent(site(&calibration, &g("linear_conv")));
                let e_state_out = cal::site_exponent(site(&calibration, &g("linear_state_out")));
                let e_gated = cal::site_exponent(site(&calibration, &g("linear_gated")));
                // The head-wise norm changes the magnitude, so the value that reaches the multiply
                // has its own exponent — it is not the state output's.
                let e_normed_out = cal::site_exponent(site(&calibration, &g("linear_normed")));
                let e_stream_out = cal::site_exponent(site(&calibration, &g("attn_residual")));
                // **The delta is produced at the POST-residual stream's exponent, not at its own.**
                //
                // The engine aligns the stream to the delta's scale, adds, and renormalizes. If the
                // delta is smaller than the stream — which it is, in a healthy residual network —
                // then an exponent chosen from the delta's own range makes the ALIGNED stream
                // overflow the code rail and saturate, and the residual path stops carrying
                // anything. Choosing the sum's exponent for both costs the delta some resolution
                // and is the only choice that keeps the addition meaningful.
                let e_delta = e_stream_out;
                // `silu(z)` is unbounded above; a probability's exponent saturates it.
                let e_gate = cal::site_exponent(site(&calibration, &g("linear_gate_act")));
                // The state's exponent is what the state REACHED, not what the contraction bound
                // allows: a head at `decay ≈ 1` sends the bound to a million and the grid it
                // implies saturates on the first token.
                // **Per head.** The row that leaves the recurrence is normalized per head, and an
                // RMS norm divides by that head's own magnitude — so a head whose values are small
                // gets few code bits at a shared exponent and the norm amplifies its relative error
                // to O(1). Per head is safe where per lane is not: the norm reduces WITHIN a head,
                // and the only reduction across heads is the output projection, which runs after
                // the row is back on one exponent.
                let state_lanes = calibration.sites.get(&g("linear_state")).map(|r| r.lanes.clone()).unwrap_or_default();
                let out_lanes = calibration.sites.get(&g("linear_state_out")).map(|r| r.lanes.clone()).unwrap_or_default();
                let v_lanes = calibration.sites.get(&g("linear_conv")).map(|r| r.lanes.clone()).unwrap_or_default();
                let head_max = |lanes: &[f64], from: usize, len: usize, fallback: f64| -> f64 {
                    let slice = lanes.get(from..from + len).unwrap_or(&[]);
                    let m = slice.iter().fold(0.0f64, |a, v| a.max(*v));
                    if m > 0.0 { m } else { fallback }
                };
                let hd = linear_head_dim;
                let (mut read_rows, mut write_rows, mut out_rows) = (Vec::new(), Vec::new(), Vec::new());
                for vh in 0..linear_v_heads {
                    // The state's lanes are recorded one head at a time, so what survives is the
                    // last head's; the row peak is the honest fallback for the others.
                    // **The state is `i32`, so its exponent targets the `i32` rail.**
                    // `site_exponent` sizes against the A16 code rail, which is the right rail for
                    // anything that will be an activation and the wrong one for a recurrent state
                    // — the state is never narrowed to a code, and sizing it as if it were left
                    // fifteen of its thirty-one bits unused. That is precision the recurrence
                    // carries forward, and it is what the arm's output was missing.
                    let e_state =
                        cal::site_exponent(head_max(&state_lanes, 0, state_lanes.len(), site(&calibration, &g("linear_state")))) + 15;
                    // `v` is the conv output's third block, `[2·dk, 2·dk + dv)`.
                    let e_v = cal::site_exponent(head_max(&v_lanes, 2 * dk + vh * hd, hd, site(&calibration, &g("linear_conv"))));
                    // Wide out: the recurrence's row is normalized per head and never reduced
                    // across heads, so it is sized against the `i32` rail like the state.
                    let e_o = cal::site_exponent(head_max(&out_lanes, vh * hd, hd, site(&calibration, &g("linear_state_out")))) + 15;
                    let (r, w, o) = cal::gdn_params(e_state, e_v, e_o);
                    read_rows.push(r);
                    write_rows.push(w);
                    out_rows.push(o);
                }
                let _ = (e_conv_act, e_state_out);

                measured.insert(g("attn_norm.a16"), wire(&cal::norm_params(&attn_gain, e_normed)));
                for (name, out_dim) in [("linear_q.weight", dk), ("linear_k.weight", dk), ("linear_v.weight", dv)] {
                    let _ = out_dim;
                    measured.insert(g(&format!("{name}.a16")), wire(&cal::projection_params(&scales[&g(name)], e_normed, e_qkv)));
                }
                measured.insert(g("linear_z.weight.a16"), wire(&cal::to_qk_params(&scales[&g("linear_z.weight")], e_normed)));
                measured.insert(g("linear_conv.a16"), wire(&cal::to_qk_params(&scales[&g("linear_conv.weight")], e_qkv)));
                measured.insert(g("linear_conv_act.a16"), wire(&vec![cal::rescale_params(K as i32, e_conv_act); width]));
                measured.insert(g("linear_dt.weight.a16"), wire(&cal::to_qk_params(&scales[&g("linear_dt.weight")], e_normed)));
                measured.insert(g("linear_beta.weight.a16"), wire(&cal::to_qk_params(&scales[&g("linear_beta.weight")], e_normed)));
                measured.insert(g("linear_decay_c.a16"), wire(&a_log.iter().map(|v| cal::decay_exponent(*v)).collect::<Vec<_>>()));
                measured.insert(g("linear_read.a16"), wire(&read_rows));
                measured.insert(g("linear_write.a16"), wire(&write_rows));
                measured.insert(g("linear_out.a16"), wire(&out_rows));
                // The norm stays WIDE: γ is applied at Q[K], so the gate multiply sees the precision
                // the norm produced rather than a sixteen-bit rounding of it.
                let _ = e_normed_out;
                measured.insert(g("linear_norm.a16"), wire(&cal::norm_params(&ssm_norm.repeat(linear_v_heads), K as i32)));
                // `linear_gate.a16` is no longer read: the gate reaches the multiply in Q[K].
                let _ = e_gate;
                measured.insert(g("linear_gated.a16"), wire(&vec![cal::product_params(K as i32, K as i32, e_gated); dv]));
                measured
                    .insert(g("linear_o.weight.a16"), wire(&cal::projection_params(&scales[&g("linear_o.weight")], e_gated, e_delta)));
                measured.insert(g("attn_align.a16"), wire(&vec![cal::rescale_params(e_stream_in, e_delta); d]));
                measured.insert(g("attn_residual.a16"), wire(&vec![cal::rescale_params(e_delta, e_stream_out); d]));
                e_stream = e_stream_out;
            }
            Qwen36LayerKind::FullAttention => {
                let q = reader.get(&g("attn_q.weight"));
                push(&mut writer, &mut scales, &g("attn_q.weight"), &q[..q_dim * d], q_dim);
                push(&mut writer, &mut scales, &g("attn_gate.weight"), &q[q_dim * d..], q_dim);
                let k = reader.get(&g("attn_k.weight"));
                push(&mut writer, &mut scales, &g("attn_k.weight"), &k, kv_dim);
                let v = reader.get(&g("attn_v.weight"));
                push(&mut writer, &mut scales, &g("attn_v.weight"), &v, kv_dim);
                let o = reader.get(&g("attn_output.weight"));
                push(&mut writer, &mut scales, &g("attn_o.weight"), &o, d);
                let q_norm = reader.get(&g("attn_q_norm.weight"));
                let k_norm = reader.get(&g("attn_k_norm.weight"));

                let w = reference::FullWeights {
                    attn_norm: &attn_gain,
                    post_norm: &post_gain,
                    q: &q,
                    k: &k,
                    v: &v,
                    o: &o,
                    q_norm: &q_norm,
                    k_norm: &k_norm,
                };
                reference::reference_full_layer(&shape, &mut state, &mut calibration, li, &w, &mut h, &rope_f32, eps);

                let e_normed = cal::site_exponent(site(&calibration, &g("attn_norm")));
                let e_q = cal::site_exponent(site(&calibration, &g("attn_q")));
                let e_k = cal::site_exponent(site(&calibration, &g("attn_k_rot")));
                let e_v = cal::site_exponent(site(&calibration, &g("attn_v")));
                let e_qn = cal::site_exponent(site(&calibration, &g("attn_q_rot")));
                let e_logit = cal::site_exponent(site(&calibration, &g("attn_logits")));
                let e_attn = cal::site_exponent(site(&calibration, &g("attn_values")));
                let e_gated = cal::site_exponent(site(&calibration, &g("attn_gated")));
                let e_stream_out = cal::site_exponent(site(&calibration, &g("attn_residual")));
                // **The delta is produced at the POST-residual stream's exponent, not at its own.**
                //
                // The engine aligns the stream to the delta's scale, adds, and renormalizes. If the
                // delta is smaller than the stream — which it is, in a healthy residual network —
                // then an exponent chosen from the delta's own range makes the ALIGNED stream
                // overflow the code rail and saturate, and the residual path stops carrying
                // anything. Choosing the sum's exponent for both costs the delta some resolution
                // and is the only choice that keeps the addition meaningful.
                let e_delta = e_stream_out;

                measured.insert(g("attn_norm.a16"), wire(&cal::norm_params(&attn_gain, e_normed)));
                measured.insert(g("attn_q.weight.a16"), wire(&cal::projection_params(&scales[&g("attn_q.weight")], e_normed, e_q)));
                measured.insert(g("attn_gate.weight.a16"), wire(&cal::to_qk_params(&scales[&g("attn_gate.weight")], e_normed)));
                measured.insert(g("attn_k.weight.a16"), wire(&cal::projection_params(&scales[&g("attn_k.weight")], e_normed, e_k)));
                measured.insert(g("attn_v.weight.a16"), wire(&cal::projection_params(&scales[&g("attn_v.weight")], e_normed, e_v)));
                // The QK-norms take the projection's codes to the rotated code scale.
                measured.insert(g("attn_q_norm.a16"), wire(&cal::norm_params(&q_norm, e_qn)));
                measured.insert(g("attn_k_norm.a16"), wire(&cal::norm_params(&k_norm, e_k)));
                // The rotary table is Q[K] and the rotation preserves the code exponent, so the
                // narrowing is exactly the table's own scale undone.
                measured.insert(g("attn_rope.a16"), wire(&[cal::triple(2f64.powi(-(K as i32)))]));
                measured.insert(g("attn_logits.a16"), wire(&[cal::attn_logit_params(e_qn, e_k, e_logit, head_dim)]));
                measured.insert(
                    g("attn_softmax_up.a16"),
                    wire(&[A16QuantParams { multiplier: 1, shift: 0, zero: cal::softmax_up_bits(e_logit) as i64 }]),
                );
                // Softmax returns Q[K] probabilities; the narrowing puts them on the code grid,
                // where a probability's natural exponent is the one that maps 1.0 to the rail.
                measured.insert(g("attn_probs.a16"), wire(&[cal::rescale_params(K as i32, cal::site_exponent(1.0))]));
                measured.insert(g("attn_values.a16"), wire(&[cal::product_params(cal::site_exponent(1.0), e_v, e_attn)]));
                measured.insert(g("attn_gated.a16"), wire(&[cal::product_params(e_attn, K as i32, e_gated)]));
                measured.insert(g("attn_o.weight.a16"), wire(&cal::projection_params(&scales[&g("attn_o.weight")], e_gated, e_delta)));
                measured.insert(g("attn_align.a16"), wire(&vec![cal::rescale_params(e_stream_in, e_delta); d]));
                measured.insert(g("attn_residual.a16"), wire(&vec![cal::rescale_params(e_delta, e_stream_out); d]));
                e_stream = e_stream_out;
            }
        }

        // ---- the mixture ------------------------------------------------------------------
        let router = reader.get(&g("ffn_gate_inp.weight"));
        push(&mut writer, &mut scales, &g("ffn_router.weight"), &router, n_experts);
        let shared_router = reader.get(&g("ffn_gate_inp_shexp.weight"));
        push(&mut writer, &mut scales, &g("ffn_shared_gate.weight"), &shared_router, 1);
        let gate_exps = reader.get(&g("ffn_gate_exps.weight"));
        let up_exps = reader.get(&g("ffn_up_exps.weight"));
        let down_exps = reader.get(&g("ffn_down_exps.weight"));
        let per_up = moe_dim * d;
        let per_down = d * moe_dim;
        for e in 0..n_experts {
            let b = format!("blk.{li}.ffn_expert.{e}");
            push(&mut writer, &mut scales, &format!("{b}_gate.weight"), &gate_exps[e * per_up..(e + 1) * per_up], moe_dim);
            push(&mut writer, &mut scales, &format!("{b}_up.weight"), &up_exps[e * per_up..(e + 1) * per_up], moe_dim);
            push(&mut writer, &mut scales, &format!("{b}_down.weight"), &down_exps[e * per_down..(e + 1) * per_down], d);
        }
        let shared_gate = reader.get(&g("ffn_gate_shexp.weight"));
        let shared_up = reader.get(&g("ffn_up_shexp.weight"));
        let shared_down = reader.get(&g("ffn_down_shexp.weight"));
        let sb = format!("blk.{li}.ffn_shared_expert");
        push(&mut writer, &mut scales, &format!("{sb}_gate.weight"), &shared_gate, shared_dim);
        push(&mut writer, &mut scales, &format!("{sb}_up.weight"), &shared_up, shared_dim);
        push(&mut writer, &mut scales, &format!("{sb}_down.weight"), &shared_down, d);

        let mw = reference::MoeWeights {
            post_norm: &post_gain,
            router: &router,
            gate_exps: &gate_exps,
            up_exps: &up_exps,
            down_exps: &down_exps,
            shared_gate: &shared_gate,
            shared_up: &shared_up,
            shared_down: &shared_down,
            shared_router: &shared_router,
        };
        let e_stream_in = e_stream;
        reference::reference_moe_layer(&shape, &mut calibration, li, &mw, &mut h, eps);

        let e_ffn_normed = cal::site_exponent(site(&calibration, &g("ffn_norm")));
        let e_expert_gated = cal::site_exponent(site(&calibration, &g("ffn_expert_gated")));
        let e_expert_up = cal::site_exponent(site(&calibration, &g("ffn_expert_up")));
        // The expert's output is wide now, so its exponent is chosen against the i32 rail rather
        // than the code rail — fifteen more bits of resolution for a row the combine cancels.
        let e_expert_out = cal::site_exponent(site(&calibration, &g("ffn_expert_out"))) + 15;
        let e_routed = cal::site_exponent(site(&calibration, &g("ffn_routed")));
        // Same rule as the arms': the mixture's output rides the post-residual stream's exponent,
        // so the aligned stream does not saturate on its way into the addition.
        let e_stream_out_ffn = cal::site_exponent(site(&calibration, &g("ffn_residual")));
        let e_moe_out = e_stream_out_ffn;
        let e_shared_gated = cal::site_exponent(site(&calibration, &g("ffn_shared_gated")));
        let e_shared_out = cal::site_exponent(site(&calibration, &g("ffn_shared_out"))) + 15;
        let e_shared_up = cal::site_exponent(site(&calibration, &g("ffn_shared_up")));

        measured.insert(g("ffn_norm.a16"), wire(&cal::norm_params(&post_gain, e_ffn_normed)));
        let (router_p, router_narrow, up_bits) =
            cal::router_params(&scales[&g("ffn_router.weight")], e_ffn_normed, site(&calibration, &g("ffn_router")));
        measured.insert(g("ffn_router.weight.a16"), wire(&router_p));
        measured.insert(g("ffn_router.a16"), wire(&vec![router_narrow; n_experts]));
        measured.insert(g("ffn_router_up.a16"), wire(&[A16QuantParams { multiplier: 1, shift: 0, zero: up_bits as i64 }]));
        // The routed combine: Q[K] weights times expert codes, into the routed site's exponent.
        measured.insert(g("ffn_combine.a16"), wire(&[cal::product_params(K as i32, e_expert_out, e_routed)]));
        measured
            .insert(g("ffn_shared_gate.weight.a16"), wire(&cal::to_qk_params(&scales[&g("ffn_shared_gate.weight")], e_ffn_normed)));
        measured.insert(g("ffn_shared_gated.a16"), wire(&[cal::product_params(e_shared_out, K as i32, e_routed)]));
        measured.insert(g("ffn_moe_out.a16"), wire(&vec![cal::rescale_params(e_routed, e_moe_out); d]));
        for e in 0..n_experts {
            let b = format!("blk.{li}.ffn_expert.{e}");
            measured
                .insert(format!("{b}_gate.weight.a16"), wire(&cal::to_qk_params(&scales[&format!("{b}_gate.weight")], e_ffn_normed)));
            measured.insert(
                format!("{b}_up.weight.a16"),
                wire(&cal::projection_params(&scales[&format!("{b}_up.weight")], e_ffn_normed, e_expert_up)),
            );
            // Wide: the silu stays at Q[K] and the product narrows once.
            measured.insert(format!("{b}_silu.a16"), wire(&vec![cal::rescale_params(K as i32, K as i32); moe_dim]));
            measured
                .insert(format!("{b}_gated.a16"), wire(&vec![cal::product_params(K as i32, e_expert_up, e_expert_gated); moe_dim]));
            measured.insert(
                format!("{b}_down.weight.a16"),
                wire(&cal::projection_params(&scales[&format!("{b}_down.weight")], e_expert_gated, e_expert_out)),
            );
        }
        measured
            .insert(format!("{sb}_gate.weight.a16"), wire(&cal::to_qk_params(&scales[&format!("{sb}_gate.weight")], e_ffn_normed)));
        measured.insert(
            format!("{sb}_up.weight.a16"),
            wire(&cal::projection_params(&scales[&format!("{sb}_up.weight")], e_ffn_normed, e_shared_up)),
        );
        measured.insert(format!("{sb}_silu.a16"), wire(&vec![cal::rescale_params(K as i32, K as i32); shared_dim]));
        measured
            .insert(format!("{sb}_gated.a16"), wire(&vec![cal::product_params(K as i32, e_shared_up, e_shared_gated); shared_dim]));
        measured.insert(
            format!("{sb}_down.weight.a16"),
            wire(&cal::projection_params(&scales[&format!("{sb}_down.weight")], e_shared_gated, e_shared_out)),
        );
        measured.insert(g("ffn_align.a16"), wire(&vec![cal::rescale_params(e_stream_in, e_moe_out); d]));
        measured.insert(g("ffn_residual.a16"), wire(&vec![cal::rescale_params(e_moe_out, e_stream_out_ffn); d]));
        e_stream = e_stream_out_ffn;

        println!("  layer {li:2} ({:?}) stream e={e_stream}", shape.layer_types[li]);
    }

    // ---- the head --------------------------------------------------------------------------
    let output_norm = reader.get("output_norm.weight");
    let output = reader.get("output.weight");
    let (codes, out_scales) = quantize_rows(&output, vocab);
    writer.push("output.weight", &codes).unwrap_or_else(|e| die(format!("output.weight: {e}")));
    drop(codes);
    reference::reference_head(&shape, &mut calibration, &output_norm, &output, &h, eps);
    drop(output);
    let e_final = cal::site_exponent(site(&calibration, "final_norm"));
    let e_logits = cal::site_exponent(site(&calibration, "logits"));
    measured.insert("final_norm.a16".into(), wire(&cal::norm_params(&output_norm, e_final)));
    measured.insert("output.weight.a16".into(), wire(&cal::projection_params(&out_scales, e_final, e_logits)));

    let written = writer.finish_with_params(&measured).unwrap_or_else(|e| die(format!("closing {out_path}: {e}")));
    println!(
        "wrote {out_path}: {:.2} GiB in {:?} ({} sites calibrated)",
        written as f64 / (1u64 << 30) as f64,
        started.elapsed(),
        measured.len()
    );

    // The reference's measured ranges, for comparing a run's magnitudes against them site by site.
    if let Some(path) = flag(&args, "--dump-sites") {
        let mut out = String::new();
        for (name, range) in &calibration.sites {
            out.push_str(&format!("{name}\t{:.6e}\t{:.6e}\t{}\n", range.absmax, range.rms(), cal::site_exponent(range.absmax)));
        }
        std::fs::write(path, out).unwrap_or_else(|e| die(format!("{path}: {e}")));
        println!("site ranges: {} sites to {path}", calibration.sites.len());
    }

    // The last position's row at every site, for locating a stage whose direction is wrong.
    if let Some(path) = flag(&args, "--dump-rows") {
        let mut out = Vec::new();
        out.extend_from_slice(&(calibration.rows.len() as u64).to_le_bytes());
        for (name, row) in &calibration.rows {
            out.extend_from_slice(&(name.len() as u64).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(row.len() as u64).to_le_bytes());
            for v in row {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        std::fs::write(path, &out).unwrap_or_else(|e| die(format!("{path}: {e}")));
        println!("site rows: {} sites to {path}", calibration.rows.len());
    }

    // The reference's own logits, for a fidelity check against a run of the artifact.
    if let Some(path) = flag(&args, "--reference-logits") {
        let mut out = Vec::with_capacity(calibration.logits.len() * vocab * 4 + 8);
        out.extend_from_slice(&(calibration.logits.len() as u64).to_le_bytes());
        for row in &calibration.logits {
            out.extend_from_slice(&(row.len() as u64).to_le_bytes());
            for v in row {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        std::fs::write(path, &out).unwrap_or_else(|e| die(format!("{path}: {e}")));
        println!("reference logits: {} rows to {path}", calibration.logits.len());
    }
}
