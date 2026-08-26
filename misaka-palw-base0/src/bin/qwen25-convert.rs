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

use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_1000000_GEN_Q};
use misaka_palw_base0::convert::{
    Qwen25ConvertPlan, activation_scale_of, biased_channel_count, calibrate_layer_residuals, convert_qwen25, measure_depth_health,
};
use misaka_palw_base0::engine::{Base0Engine, KvCache};
use misaka_palw_base0::reference::{RefConfigV1, reference_forward_full, score_fidelity};

fn die(message: String) -> ! {
    eprintln!("qwen25-convert: {message}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).unwrap_or_else(|| die("usage: qwen25-convert <dir> [--layers N]".into()));
    let layer_cap: Option<usize> = args
        .iter()
        .position(|a| a == "--layers")
        .and_then(|i| args.get(i + 1))
        .map(|v| v.parse().unwrap_or_else(|_| die(format!("--layers wants a number, got {v}"))));

    let cfg_bytes = std::fs::read(format!("{dir}/config.json")).unwrap_or_else(|e| die(format!("config.json: {e}")));
    let cfg: serde_json::Value = serde_json::from_slice(&cfg_bytes).unwrap_or_else(|e| die(format!("config.json: {e}")));
    let num =
        |k: &str| -> usize { cfg.get(k).and_then(|v| v.as_u64()).unwrap_or_else(|| die(format!("config.json has no {k}"))) as usize };
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
        // Qwen2.5's base, NOT the conventional 10,000 — the config says rope_theta 1e6, and the
        // wrong base is a silently-wrong model (measured: layer-0 attention delta at cosine 0.73
        // against the reference, from the rotation alone).
        ln_theta_gen_q: LN_THETA_1000000_GEN_Q,
        eps_q: 1 << 8,
    };
    println!(
        "config: hidden {hidden}, heads {heads}/{} kv, d_head {}, ffn {}, layers {}/{declared_layers}, vocab {}",
        shape.n_kv_heads, shape.d_head, shape.d_ff, shape.n_layers, shape.vocab
    );

    let tokenizer = std::fs::read(format!("{dir}/tokenizer.json")).unwrap_or_else(|e| die(format!("tokenizer.json: {e}")));
    let commitment = Base0ArtifactV1::tokenizer_commitment_of(&tokenizer);

    let started = std::time::Instant::now();
    let blob = std::fs::read(format!("{dir}/model.safetensors")).unwrap_or_else(|e| die(format!("model.safetensors: {e}")));
    println!("read {} MiB in {:?}", blob.len() / (1 << 20), started.elapsed());

    let plan = Qwen25ConvertPlan { shape, rms_norm_eps_bits: 1e-6f32.to_bits() };

    // 57 tokens of real prose (the runbook's own words, tokenized by the model's tokenizer.json).
    // A 4-token prompt gave 3 generic positions — too few for activation ranges, and any
    // staticness question is unmeasurable at n=3.
    let prompt: Vec<usize> = vec![
        785, 8781, 69180, 24821, 825, 3019, 315, 279, 11320, 25, 264, 88737, 37201, 628, 288, 279, 10023, 504, 11163, 54510, 323,
        42465, 4078, 17966, 13, 1416, 807, 28295, 11, 279, 5473, 15885, 279, 54510, 323, 279, 34784, 27627, 892, 3108, 46153, 13,
        4440, 5256, 374, 68903, 11, 773, 279, 1973, 315, 45735, 4157, 2297, 279, 1102, 13,
    ];
    let ref_cfg = RefConfigV1 {
        n_layers: shape.n_layers,
        n_heads: shape.n_heads,
        n_kv_heads: shape.n_kv_heads,
        d_head: shape.d_head,
        d_ff: shape.d_ff,
        vocab: shape.vocab,
        rms_eps: cfg.get("rms_norm_eps").and_then(|v| v.as_f64()).unwrap_or(1e-6) as f32,
        rope_theta: cfg.get("rope_theta").and_then(|v| v.as_f64()).unwrap_or(1e6) as f32,
    };

    // The float reference runs FIRST: it is both the calibration measurement and, at the end,
    // the quality bar. One pass serves both.
    let started = std::time::Instant::now();
    let (reference, ref_probe, stats) =
        reference_forward_full(&blob, &ref_cfg, &prompt).unwrap_or_else(|e| die(format!("reference failed: {e}")));
    let ref_streams = &ref_probe.streams;
    println!("reference forward in {:?}", started.elapsed());
    let ref_argmax: Vec<usize> = reference
        .iter()
        .map(|row| {
            let mut best = 0usize;
            for (i, v) in row.iter().enumerate() {
                if *v > row[best] {
                    best = i;
                }
            }
            best
        })
        .collect();
    println!("  reference argmax {ref_argmax:?}");
    if args.iter().any(|a| a == "--dump-stats") {
        for (li, st) in stats.per_layer.iter().enumerate() {
            println!(
                "  L{li:02} n1 {:.2} n2 {:.2} | q {:.1} k {:.1} v {:.1} attn {:.2} | d0 {:.1} h0 {:.1} d1 {:.1} h1 {:.1} | gate {:.1} silu {:.1} up {:.1} gu {:.1}",
                st.norm_absmax[0],
                st.norm_absmax[1],
                st.q_absmax,
                st.k_absmax,
                st.v_absmax,
                st.attn_absmax,
                st.delta_absmax[0],
                st.h_absmax[0],
                st.delta_absmax[1],
                st.h_absmax[1],
                st.gate_absmax,
                st.silu_absmax,
                st.up_absmax,
                st.gated_absmax
            );
        }
        println!("  final norm absmax {:.2}", stats.final_norm_absmax);
        // The geometry of the outliers: per position, and per channel at the worst layer. If the
        // massive values ride ONE position they are a sink token; if they ride every position at
        // fixed channels they are a stream bias. The two need different arithmetic.
        for li in [1usize, 2, 13, 26].into_iter().filter(|l| *l < shape.n_layers) {
            let per_pos: Vec<String> =
                ref_streams[li].iter().map(|row| format!("{:.0}", row.iter().fold(0f32, |a, v| a.max(v.abs())))).collect();
            println!("  L{li:02} per-position |h| {}", per_pos.join(" "));
        }
        // Staticness of the heavy stream channels: if a heavy channel's value is near-constant
        // across positions, it is a BIAS wearing a channel — subtractable by a requant zero — and
        // the stream's dynamic range shrinks by its magnitude. If it varies, it is signal and
        // int8 must carry it.
        for li in [5usize, 13, 21].into_iter().filter(|l| *l < shape.n_layers) {
            let rows: Vec<&Vec<f32>> = ref_probe.streams[li].iter().skip(1).collect();
            let d = rows[0].len();
            let n = rows.len() as f32;
            let mut mean = vec![0f32; d];
            for r in &rows {
                for (c, v) in r.iter().enumerate() {
                    mean[c] += v / n;
                }
            }
            let mut var = vec![0f32; d];
            for r in &rows {
                for (c, v) in r.iter().enumerate() {
                    var[c] += (v - mean[c]) * (v - mean[c]) / n;
                }
            }
            let mut idx: Vec<usize> = (0..d).collect();
            idx.sort_by(|a, b| mean[*b].abs().partial_cmp(&mean[*a].abs()).unwrap());
            let line: Vec<String> = idx.iter().take(8).map(|c| format!("[{c}] {:.1}±{:.1}", mean[*c], var[*c].sqrt())).collect();
            println!("  L{li:02} heavy channels (mean±std over {} generic positions): {}", rows.len(), line.join("  "));
        }
        // Worst channels at layer 2, last position: which channels are massive, and how many.
        let row = &ref_streams[2][prompt.len() - 1];
        let mut idx: Vec<usize> = (0..row.len()).collect();
        idx.sort_by(|a, b| row[*b].abs().partial_cmp(&row[*a].abs()).unwrap());
        let top: Vec<String> = idx.iter().take(8).map(|i| format!("[{i}]={:.0}", row[*i])).collect();
        println!("  L02 top channels {}", top.join(" "));
        let big = row.iter().filter(|v| v.abs() > 100.0).count();
        println!("  L02 channels past |100| {big}/{}", row.len());
    }

    if args.iter().any(|a| a == "--a16") {
        // The W8A16 path: measure fidelity and stop — this is the activation width the
        // quantization-regime ladder said the architecture needs.
        let started = std::time::Instant::now();
        let artifact = misaka_palw_base0::convert::convert_qwen25_a16(&blob, &plan, &stats)
            .unwrap_or_else(|e| die(format!("a16 conversion failed: {e}")));
        println!("a16 converted in {:?}", started.elapsed());
        println!("a16 artifact  {}", artifact.artifact_digest());
        // **`--out` is what turns a measurement into a runtime.** Until the artifact could be
        // written, every use of this class re-read a 3 GiB checkpoint and re-quantized it, which
        // is a minute per run and is why nothing downstream of conversion had been built.
        if let Some(i) = args.iter().position(|a| a == "--out") {
            let path = args.get(i + 1).unwrap_or_else(|| die("--out wants a path".into()));
            let bytes = misaka_palw_base0::artifact::encode_artifact_file_v1(&artifact);
            std::fs::write(path, &bytes).unwrap_or_else(|e| die(format!("{path}: {e}")));
            // Read it straight back: the container verifies the digest on decode, so a write that
            // cannot be read is caught here rather than by the runtime that loads it tomorrow.
            let back = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes)
                .unwrap_or_else(|e| die(format!("the artifact just written does not decode: {e}")));
            assert_eq!(back.artifact_digest(), artifact.artifact_digest());
            println!("a16 written   {} ({} MiB)", path, bytes.len() >> 20);
        }
        let blob2 = blob;
        let engine = misaka_palw_base0::engine_a16::A16Engine::new(&artifact)
            .unwrap_or_else(|e| die(format!("a16 engine refused the artifact: {e:?}")));
        let mut cache = misaka_palw_base0::engine_a16::A16Cache::new(shape.n_layers);
        let started = std::time::Instant::now();
        let mut logits: Vec<Vec<i32>> = Vec::new();
        let mut a16_streams: Vec<Vec<Vec<i32>>> = Vec::new();
        for (position, token) in prompt.iter().enumerate() {
            let (l, st) =
                engine.forward_token_probed(&mut cache, *token, position).unwrap_or_else(|e| die(format!("a16 forward: {e:?}")));
            logits.push(l);
            a16_streams.push(st);
        }
        println!("a16 forward {} tokens in {:?}", prompt.len(), started.elapsed());
        // The rows ARE the committed i16 logit codes (in i32 lanes) — scored as-is, because the
        // class output is DEFINED over them.
        let fidelity = score_fidelity(&reference, &logits);
        println!("  a16 top-1 agree      {}/{}", fidelity.top1_agree, fidelity.positions);
        println!("  a16 top-5 contains   {}/{}", fidelity.top5_contains, fidelity.positions);
        println!("  a16 rank corr (100)  {:.4}", fidelity.top100_rank_correlation);
        println!("  a16 FAITHFUL         {}", fidelity.is_faithful());
        // Determinism on the real thing, and the held-out check: the score above is on the
        // CALIBRATION prompt (in-sample). A class whose fidelity only holds on the prompt its
        // scales were measured from is a curve fit, so `--held-out` scores a second prompt the
        // calibration never saw, against its own reference.
        if let Some(pos) = args.iter().position(|a| a == "--held-out") {
            let held: Vec<usize> = args
                .get(pos + 1)
                .unwrap_or_else(|| die("--held-out wants a comma-separated token list".into()))
                .split(',')
                .map(|v| v.parse().unwrap_or_else(|_| die(format!("bad held-out token: {v}"))))
                .collect();
            let (held_ref, _, _) =
                reference_forward_full(&blob2, &ref_cfg, &held).unwrap_or_else(|e| die(format!("held-out reference failed: {e}")));
            let mut cache = misaka_palw_base0::engine_a16::A16Cache::new(shape.n_layers);
            let held_logits: Vec<Vec<i32>> = held
                .iter()
                .enumerate()
                .map(|(position, token)| {
                    engine.forward_token(&mut cache, *token, position).unwrap_or_else(|e| die(format!("a16 held-out forward: {e:?}")))
                })
                .collect();
            let held_fid = score_fidelity(&held_ref, &held_logits);
            println!("  a16 HELD-OUT top-1   {}/{}", held_fid.top1_agree, held_fid.positions);
            println!("  a16 HELD-OUT top-5   {}/{}", held_fid.top5_contains, held_fid.positions);
            println!("  a16 HELD-OUT corr    {:.4}", held_fid.top100_rank_correlation);
            println!("  a16 HELD-OUT FAITHFUL {}", held_fid.is_faithful());
        }
        let last = prompt.len() - 1;
        let line: Vec<String> = (0..shape.n_layers)
            .map(|li| {
                let ints: Vec<f32> = a16_streams[last][li].iter().map(|c| *c as f32).collect();
                let (mut dot, mut na, mut nb) = (0f64, 0f64, 0f64);
                for (a, b) in ref_probe.streams[li][last].iter().zip(&ints) {
                    dot += *a as f64 * *b as f64;
                    na += *a as f64 * *a as f64;
                    nb += *b as f64 * *b as f64;
                }
                format!("{:.2}", if na > 0.0 && nb > 0.0 { dot / (na.sqrt() * nb.sqrt()) } else { 0.0 })
            })
            .collect();
        println!("  a16 stream cosine {}", line.join(" "));
        // **`--replay` / `--dispute-replay` are not wired on this line.** They drive
        // `misaka_palw_base0::replay`, which reads the court's A16 dispatch — the half of the A16
        // work that still lives on `palw-mainnet-rc-integration` and whose reconcile touches
        // `palw_step_refute` and `palw_qwen25_profile`, both of which moved on both branches.
        // Conversion does not need it: what is being built here is the artifact, and the court
        // replays the artifact rather than the converter.
        if args.iter().any(|a| a == "--replay" || a == "--dispute-replay") {
            println!("note: --replay needs the court-side A16 reconcile; not available on this branch");
        }
        return;
    }

    let started = std::time::Instant::now();
    // The static-calibrated float converter retired with the A16 tier (its whole apparatus —
    // smoothing, per-channel folds, sink lanes — was the ceiling A16 removed); the float lane
    // converts plainly and calibrates per-layer residual shifts below.
    let artifact = convert_qwen25(&blob, &plan).unwrap_or_else(|e| die(format!("conversion failed: {e}")));
    let artifact = artifact.with_tokenizer_commitment(commitment);
    println!("converted in {:?}", started.elapsed());
    drop(blob);

    let weight_bytes: usize = artifact.embed.len()
        + artifact.unembed.len()
        + artifact
            .layers
            .iter()
            .map(|l| l.wq.len() + l.wk.len() + l.wv.len() + l.wo.len() + l.w_gate.len() + l.w_up.len() + l.w_down.len())
            .sum::<usize>();
    println!("class id      {}", artifact.artifact_digest());
    println!("tokenizer     {commitment}");
    println!("int8 weights  {} MiB", weight_bytes / (1 << 20));
    println!("biased chans  {} (zero would mean the biases rounded away)", biased_channel_count(&artifact));
    println!("act scale     {} (the legacy global; calibrated sites carry their own)", activation_scale_of(&artifact.norm_requant));
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
    println!(
        "  gate peak      min {} max {}",
        health.gate_peak_decay().iter().min().unwrap(),
        health.gate_peak_decay().iter().max().unwrap()
    );
    println!("  attn spread    {} (0 = a head selected nothing)", health.min_attention_spread);
    println!("  railed layers  {}/{}", health.saturated_residual.0, health.saturated_residual.1);
    println!("  argmax         {:?}", health.argmax);

    // Determinism, on the real thing: the property the whole class rests on.
    let again = measure_depth_health(&artifact, &prompt).unwrap();
    println!("  reproducible   {}", again == health);

    let engine = Base0Engine::new(&artifact);
    let mut cache = KvCache::new(&artifact);
    let mut integer: Vec<Vec<i32>> = Vec::new();
    for (position, token) in prompt.iter().enumerate() {
        let (logits, probe) =
            engine.forward_token_probed(&mut cache, *token, position).unwrap_or_else(|e| die(format!("engine: {e:?}")));
        integer.push(logits);
        let _ = probe;
    }

    if args.iter().any(|a| a == "--dump-stats") {
        // The last pass again, probed, for the per-layer internals the health summary collapses.
        let mut cache = KvCache::new(&artifact);
        let mut last = None;
        for (position, token) in prompt.iter().enumerate() {
            last = Some(engine.forward_token_probed(&mut cache, *token, position).unwrap());
        }
        let (_, probe) = last.unwrap();
        let heads = shape.n_heads;
        for li in 0..shape.n_layers {
            let spreads = &probe.attention_spread[li * heads..(li + 1) * heads];
            let uniform = spreads.iter().filter(|s| **s == 0).count();
            println!(
                "  L{li:02} attn spread min {:>9} max {:>9} uniform-heads {uniform}/{heads}  residual peak {}",
                spreads.iter().min().unwrap(),
                spreads.iter().max().unwrap(),
                probe.residual_peak[li]
            );
        }
    }
    let fidelity = score_fidelity(&reference, &integer);
    println!("  top-1 agree      {}/{}", fidelity.top1_agree, fidelity.positions);
    println!("  top-5 contains   {}/{}", fidelity.top5_contains, fidelity.positions);
    println!("  rank corr (100)  {:.4}", fidelity.top100_rank_correlation);
    println!("  FAITHFUL         {}", fidelity.is_faithful());
}
