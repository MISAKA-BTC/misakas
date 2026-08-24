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

use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1};
use misaka_palw_base0::convert::{
    Qwen25ConvertPlan, activation_scale_of, biased_channel_count, calibrate_layer_residuals, convert_qwen25, measure_depth_health,
};

fn die(message: String) -> ! {
    eprintln!("qwen25-convert: {message}");
    std::process::exit(1)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).unwrap_or_else(|| die("usage: qwen25-convert <dir> [--layers N] [--max-position N] [--out FILE]".into()));
    let flag = |name: &str| -> Option<String> { args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned() };
    let out_path = flag("--out");
    let layer_cap: Option<usize> =
        flag("--layers").map(|v| v.parse().unwrap_or_else(|_| die(format!("--layers wants a number, got {v}"))));

    // **The class registry is the only arithmetic there is.** This tool used to carry its own copy
    // of the shape — `max_position: 512` and `eps_q: 1 << 8`, the latter inherited from the floor —
    // while `QWEN25_1_5B` declares `rms_eps_q: 1`. Two arithmetic specifications under one model
    // id, and not a cosmetic split: the engine norms with the ARTIFACT's epsilon and the court
    // re-norms with the CLASS's, so an artifact built at 256 under a class registered at 1 has
    // every honest execution convicted — a bug already in this repo's history.
    //
    // So the shape is LOOKED UP, and `config.json` becomes something to check the checkpoint
    // against rather than the thing that decides the class.
    let model_id = flag("--model-id").unwrap_or_else(|| {
        die("--model-id is required (e.g. Qwen/Qwen2.5-1.5B): a class's arithmetic comes from the registry, not from config.json"
            .into())
    });
    let court =
        kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .unwrap_or_else(|e| die(format!("the shipped court parameters do not build: {e:?}")));
    let class = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&court, &model_id)
        .unwrap_or_else(|| die(format!("{model_id} is not a class this build knows")));

    let cfg_bytes = std::fs::read(format!("{dir}/config.json")).unwrap_or_else(|e| die(format!("config.json: {e}")));
    let cfg: serde_json::Value = serde_json::from_slice(&cfg_bytes).unwrap_or_else(|e| die(format!("config.json: {e}")));
    let num =
        |k: &str| -> usize { cfg.get(k).and_then(|v| v.as_u64()).unwrap_or_else(|| die(format!("config.json has no {k}"))) as usize };
    let declared_layers = num("num_hidden_layers");
    let want = class.artifact_shape;

    // The checkpoint must BE the model the class is defined for. A mismatch means this directory
    // holds a different checkpoint than `--model-id` claims, and converting it anyway would mint
    // an artifact under a class id that describes something else.
    for (field, got, expect) in [
        ("hidden_size", num("hidden_size"), want.n_heads * want.d_head),
        ("num_attention_heads", num("num_attention_heads"), want.n_heads),
        ("num_key_value_heads", num("num_key_value_heads"), want.n_kv_heads),
        ("intermediate_size", num("intermediate_size"), want.d_ff),
        ("vocab_size", num("vocab_size"), want.vocab),
    ] {
        if got != expect {
            die(format!("config.json says {field}={got} and {model_id} is defined at {expect} — wrong checkpoint"));
        }
    }
    if layer_cap.is_none() && declared_layers != want.n_layers {
        die(format!("config.json says {declared_layers} layers and {model_id} is defined at {} — wrong checkpoint", want.n_layers));
    }

    // Every field from the registry. `--layers` still truncates for a smoke test, and says so:
    // the artifact it produces matches no registered class, by construction.
    let shape = Base0ShapeV1 { n_layers: layer_cap.unwrap_or(want.n_layers).min(declared_layers), ..want };
    if shape.n_layers != want.n_layers {
        println!("WARNING: --layers {} truncates the class; this artifact matches no registered class", shape.n_layers);
    }
    println!("class {model_id}");
    println!("  canonical id  {}", class.class_id());
    println!(
        "  geometry      layers {} hidden {} heads {}/{} kv, d_head {}, ffn {}, vocab {}",
        shape.n_layers,
        shape.n_heads * shape.d_head,
        shape.n_heads,
        shape.n_kv_heads,
        shape.d_head,
        shape.d_ff,
        shape.vocab
    );
    println!("  arithmetic    eps_q {} (registry, not config.json), max_position {}", shape.eps_q, shape.max_position);

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
        + artifact
            .layers
            .iter()
            .map(|l| l.wq.len() + l.wk.len() + l.wv.len() + l.wo.len() + l.w_gate.len() + l.w_up.len() + l.w_down.len())
            .sum::<usize>();
    println!("class id      {}", artifact.artifact_digest());
    println!("tokenizer     {commitment}");
    println!("int8 weights  {} MiB", weight_bytes / (1 << 20));
    println!("biased chans  {} (zero would mean the biases rounded away)", biased_channel_count(&artifact));
    println!("act scale     {}", activation_scale_of(&artifact.norm_requant));
    println!("max_position  {} (the class's context, from the registry)", artifact.shape.max_position);

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
    // **The two values a registration needs, and they are NOT the same value.** ADR-0049 G:
    // `execution_class_id` is the SHAPE PROFILE id — a class IS its graph — while `artifact_root`
    // is the Merkle root over the canonical operand inventory, which is what an opening proves
    // against. The "class id" this tool has always printed is neither: it is `artifact_digest()`,
    // the artifact's own content hash. Both come from the registry entry, at the tile length the
    // class is defined at, so neither can be a local choice.
    match class.artifact_root(&artifact) {
        Ok(root) => {
            println!("artifact_root      {root}");
            println!("execution_class_id {}", class.class_id());
        }
        Err(e) => println!("artifact_root      UNAVAILABLE: {e:?}"),
    }

    // **The producer's own path, before anything is deployed.** `measure_depth_health` is a
    // forward pass; a block needs the STEP CAPTURE too — every node output tiled and Merkleised
    // into the commitment an attempt carries. That is the expensive half and the one that decides
    // whether this class can produce at all, so it is measured here rather than discovered on a
    // fleet host.
    // **Engine width vs profile width, node by node.** ADR-0049 Decision F: the engine, the
    // profile, the adjudicator and the inventory are four hand-written descriptions of one
    // computation, and a capture that will not become a leg is those descriptions disagreeing.
    // The leg error names a table-LOCAL index and no width, which is not enough to act on, so
    // this prints both widths for every captured row.
    if args.iter().any(|a| a == "--check-capture") {
        use kaspa_consensus_core::palw_step::PalwStepOutLenV1;
        let engine = misaka_palw_base0::engine::Base0Engine::new(&artifact);
        let mut cache = misaka_palw_base0::engine::KvCache::new(&artifact);
        let (_, probe) =
            engine.forward_token_probed(&mut cache, 1, 0).unwrap_or_else(|e| die(format!("one probed token did not run: {e:?}")));
        let rows = misaka_palw_base0::legs::base0_captured_rows_v1(&probe);
        println!("capture check: {} rows at position 0", rows.len());
        let mut bad = 0usize;
        for r in &rows {
            let Some(global) = class.profile.global_node_slot(r.table, r.layer, r.index) else {
                println!(
                    "  MISSING SLOT {:?} layer {} index {} (engine produced a row the profile has no node for)",
                    r.table, r.layer, r.index
                );
                bad += 1;
                continue;
            };
            let (node, _) = class.profile.resolve_node_slot(global).expect("resolvable");
            // kv_len at position 0 is 1: prefill position p sees p+1 keys.
            let declared = match node.out_len {
                PalwStepOutLenV1::Fixed { elements } => elements as usize,
                PalwStepOutLenV1::KvScaled { multiplier } => multiplier as usize,
            };
            if r.row.len() != declared {
                println!(
                    "  WIDTH  {:?} layer {} index {} slot {global} {:?}: engine {} vs profile {} ({} tiles vs {})",
                    r.table,
                    r.layer,
                    r.index,
                    node.op_kind,
                    r.row.len(),
                    declared,
                    r.row.len().div_ceil(node.tile_len as usize),
                    declared.div_ceil(node.tile_len as usize)
                );
                bad += 1;
            }
        }
        println!("capture check: {bad} disagreement(s)");
    }

    if args.iter().any(|a| a == "--execute") {
        use kaspa_consensus_core::palw_base0_profile::rc_job_context;
        let anchor = kaspa_hashes::Hash64::from_u64_word(0x9E4D_1234);
        let (prefill, decode) = class.canonical_job;
        let mut ctx = rc_job_context(&class.profile, prefill, decode);
        ctx.job_id = anchor;
        ctx.execution_seed = anchor.as_byte_slice()[..32].try_into().expect("64 bytes has 32");
        let job_prompt: Vec<usize> = (0..prefill as usize).map(|i| (i * 7919) % artifact.shape.vocab).collect();
        ctx.prompt_token_ids_hash =
            kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&job_prompt.iter().map(|t| *t as u32).collect::<Vec<_>>());
        let leaves = kaspa_consensus_core::palw_step::step_leaf_count(&class.profile, &ctx)
            .unwrap_or_else(|e| die(format!("the canonical job has no step space: {e:?}")));
        println!("execute: canonical job {prefill} prefill / {decode} decode, {leaves} step leaves");
        let started = std::time::Instant::now();
        match misaka_palw_base0::produce::base0_execute_for_attempt_v1(&artifact, &class.profile, &ctx, &job_prompt) {
            Ok(run) => {
                println!("  ran in        {:?}", started.elapsed());
                println!("  trace_root    {}", run.trace_root);
                println!("  output_root   {}", run.output_root);
                println!("  execution_root {}", run.execution_root);
                println!("  step leaves   {} tiles captured", run.tiles.tiles.len());
                println!("  generated     {:?}", run.generated_token_ids);
            }
            Err(e) => die(format!("the canonical job did not execute: {e}")),
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
            Ok(back) => println!(
                "  reload class id {} ({})",
                back.artifact_digest(),
                if back.artifact_digest() == artifact.artifact_digest() { "matches" } else { "MISMATCH" }
            ),
            Err(e) => die(format!("the file this tool just wrote does not load: {e}")),
        }
    }
}
