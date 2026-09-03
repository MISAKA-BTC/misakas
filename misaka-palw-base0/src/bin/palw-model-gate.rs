//! **`palw-model-gate` — does the MODEL clear the bar the CARRIER work assumes it clears?**
//!
//! Everything measured about ADR-0078 Decision 2 so far has been about the carrier: the court's
//! 81,920-byte close ceiling, the widest registrable `n_ctx` row, the 8-token chat template, the
//! 38/60-token grammar floors. None of it answers the other half — whether the dense class's
//! model, given a context wide enough to hold an answer, actually EMITS a grammar-valid DSL.
//!
//! This binary is the experiment, and it is deliberately NOT the shipped worker: the shipped
//! `palw-a16-fp-worker` serves at the CLASS's registered `n_ctx` (16), read from the catalog row
//! and never from the artifact, and the width is the thing under test. So the runtime is
//! assembled the same way the worker assembles it — the same artifact decode, the same tokenizer,
//! the same `FpWorkerRuntime`, the same `execute_free_prompt` — over a profile built by
//! `palw_a16_context_row_profile_v1(n)`, the ladder function that exists for exactly this row.
//!
//! ```text
//!   MISAKA_PALW_ARTIFACT=... MISAKA_PALW_TOKENIZER=... palw-model-gate --n-ctx 256 --out <dir>
//! ```
//!
//! Nothing here is consensus code and nothing here is on a producer's path. It writes each raw
//! answer to `<dir>/<case>.raw` (every generated id rendered, EOG included and everything after
//! it) and the display-stopped answer to `<dir>/<case>.answer` — the bytes a consumer would hand
//! `palw-derive derive --answer`.

use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpPromptSegmentV1, PalwFpWorkerInputV3,
    PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_base0::artifact::decode_artifact_file_v1;
use misaka_palw_base0::fp_worker::{
    FpWorkerFamilyV1, FpWorkerRuntime, MappedArtifactV1, QWEN_EOG_TOKEN_NAMES, prompt_ids_for_input_v1, render_answer_v1,
};
use misaka_palw_base0::qwen25_a16_backend::Qwen25A16Backend;
use misaka_palw_base0::tokenizer::QwenTokenizer;
use std::path::PathBuf;

const MODEL_ID: &str = "Qwen/Qwen2.5-1.5B/graph-v2";

fn die(msg: String) -> ! {
    eprintln!("[palw-model-gate] fatal: {msg}");
    std::process::exit(1);
}

/// **The gateway's ChatML template, as segments** — `misaka_palw_gateway::wire::build_prompt`'s
/// user-turn shape. It is reproduced here rather than imported because `misaka-palw-gateway`
/// exposes no library target and `misaka-palw-base0` is below it in the graph; the reproduction
/// is checked against `misaka_palw_base0::tokenizer::qwen_chat_prompt`, this tree's OTHER
/// spelling of the same template, so a divergence in either is a failed assertion and not a
/// silently different prompt.
fn chatml_user_segments(tokenizer: &QwenTokenizer, content: &str) -> (Vec<PalwFpPromptSegmentV1>, String) {
    let start = tokenizer.added_id("<|im_start|>").unwrap_or_else(|| die("this tokenizer has no <|im_start|>".into()));
    let end = tokenizer.added_id("<|im_end|>").unwrap_or_else(|| die("this tokenizer has no <|im_end|>".into()));
    let mut segments = Vec::new();
    let mut displayed = String::new();

    segments.push(PalwFpPromptSegmentV1::Special(start));
    displayed.push_str("<|im_start|>");
    let body = format!("user\n{content}");
    segments.push(PalwFpPromptSegmentV1::Text(body.as_bytes().to_vec()));
    displayed.push_str(&body);
    segments.push(PalwFpPromptSegmentV1::Special(end));
    displayed.push_str("<|im_end|>");
    segments.push(PalwFpPromptSegmentV1::Text(b"\n".to_vec()));
    displayed.push('\n');
    segments.push(PalwFpPromptSegmentV1::Special(start));
    displayed.push_str("<|im_start|>");
    segments.push(PalwFpPromptSegmentV1::Text(b"assistant\n".to_vec()));
    displayed.push_str("assistant\n");

    let shipped = misaka_palw_base0::tokenizer::qwen_chat_prompt(None, &[("user", content)]);
    assert_eq!(displayed, shipped, "the harness's template is not this tree's Qwen chat template");

    // …and against the assembly the GATEWAY now builds prompts with. The reproduction above is
    // kept byte for byte so this file's landed result stays re-runnable; this assertion is what
    // makes "the dense lane did not move" a fact the gate itself checks, on the real tokenizer,
    // rather than a claim about a table copied into a unit test.
    let specials: Vec<(String, u32)> = tokenizer.added_tokens().iter().map(|a| (a.content.clone(), a.id)).collect();
    let production = misaka_palw_base0::chat_template::qwen_chat_prompt_plan_v1(&specials, &[("user", content)])
        .unwrap_or_else(|e| die(format!("the shipped chat template refused this prompt: {e}")))
        .unwrap_or_else(|| die("this tokenizer declares no ChatML markers".into()));
    assert_eq!(
        production.template_id,
        misaka_palw_base0::chat_template::TEMPLATE_ID_CHAT_SEGMENTS_V1,
        "the dense tier must select the transform its SMF and STL evidence was measured under, and it selected {}",
        production.template_id
    );
    assert_eq!(production.segments, segments, "the shipped assembly no longer builds the dense tier's segments");
    assert_eq!(production.displayed, displayed);
    (segments, displayed)
}

struct Case {
    name: &'static str,
    transformer: &'static str,
    prompt: String,
}

fn cases() -> Vec<Case> {
    let music_corpus = r#"{"v":1,"ppq":480,"tempo_us_per_quarter":500000,"time_signature":[4,4],"tracks":[{"name":"lead","channel":0,"program":0,"notes":[{"pitch":60,"velocity":100,"onset":0,"duration":480}]}]}"#;
    let cad_corpus = r#"{"v":1,"frac_bits":2,"sketches":{},"solid":{"op":"box","min":[-6,-6,-6],"max":[6,6,6]}}"#;
    vec![
        Case {
            name: "music-bare",
            transformer: "music/smf/v1",
            prompt: "Emit the minimal music/v1 DSL for a single MIDI note. Output only JSON.".to_string(),
        },
        Case {
            name: "music-schema",
            transformer: "music/smf/v1",
            prompt: "Emit a music/v1 JSON object. Exactly these top-level keys: v (1), ppq (480), \
                     tempo_us_per_quarter (500000), time_signature ([4,4]), tracks. Each track has exactly: \
                     name, channel, program, notes. Each note has exactly: pitch, velocity, onset, duration. \
                     One track, one note. Output only JSON."
                .to_string(),
        },
        Case {
            name: "music-oneshot",
            transformer: "music/smf/v1",
            prompt: format!("Copy this JSON exactly and output nothing else:\n{music_corpus}"),
        },
        Case {
            name: "cad-bare",
            transformer: "cad/stl/v1",
            prompt: "Emit the minimal cad/v1 DSL for a box. Output only JSON.".to_string(),
        },
        Case {
            name: "cad-schema",
            transformer: "cad/stl/v1",
            prompt: "Emit a cad/v1 JSON object. Exactly these top-level keys: v (1), frac_bits (2), \
                     sketches ({}), solid. solid has exactly: op (\"box\"), min ([x,y,z]), max ([x,y,z]). \
                     Output only JSON."
                .to_string(),
        },
        Case {
            name: "cad-oneshot",
            transformer: "cad/stl/v1",
            prompt: format!("Copy this JSON exactly and output nothing else:\n{cad_corpus}"),
        },
        // Two probes beyond the six the brief names, because the six produced ONE failure mode
        // more often than any other — a markdown code fence around otherwise valid JSON — and
        // whether a sentence of prompt removes it decides whether this is a model problem or a
        // prompt problem.
        Case {
            name: "music-nofence",
            transformer: "music/smf/v1",
            prompt: "Emit a music/v1 JSON object. Exactly these top-level keys: v (1), ppq (480), \
                     tempo_us_per_quarter (500000), time_signature ([4,4]), tracks. Each track has exactly: \
                     name, channel, program, notes. Each note has exactly: pitch, velocity, onset, duration. \
                     One track, exactly one note. Reply with the JSON object only: start with { and end with }. \
                     No code fence, no explanation."
                .to_string(),
        },
        Case {
            name: "cad-nofence",
            transformer: "cad/stl/v1",
            prompt: "Emit a cad/v1 JSON object. Exactly these top-level keys: v (1), frac_bits (2), \
                     sketches ({}), solid. solid has exactly: op (\"box\"), min ([x,y,z]), max ([x,y,z]). \
                     Reply with the JSON object only: start with { and end with }. No code fence, no explanation."
                .to_string(),
        },
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let n_ctx: u32 = flag("--n-ctx").unwrap_or_else(|| "256".into()).parse().unwrap_or_else(|e| die(format!("--n-ctx: {e}")));
    let out = PathBuf::from(flag("--out").unwrap_or_else(|| die("--out <dir> is required".into())));
    let only = flag("--only");
    // `committed` runs the shipped `execute_free_prompt` at whatever decode budget the SHIPPED
    // step ladder (2^22 leaves) admits. `wide` runs the identical decode loop through the same
    // engine with no step capture, at the full `n_ctx - prefill` budget — the generation the
    // committed path WOULD produce if the ladder admitted it (ADR-0077 Decision 12 raises the
    // top to 2^32 behind a fence; the executor here still counts against the shipped constant).
    let mode = flag("--mode").unwrap_or_else(|| "both".to_string());
    std::fs::create_dir_all(&out).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));

    // -------------------------------------------------------------------------------------
    // 1. The profile, at the width under test. Refusals here ARE the result: a ladder row that
    //    does not project is the wall, and it is reported as one.
    // -------------------------------------------------------------------------------------
    let profile = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v1(n_ctx)
        .unwrap_or_else(|e| die(format!("the ladder will not build an A16 row at n_ctx {n_ctx}: {e:?}")));
    if let Err(e) = profile.validate_shape() {
        die(format!("the n_ctx {n_ctx} row does not validate: {e:?}"));
    }
    eprintln!(
        "[palw-model-gate] profile: n_ctx={} vocab={} layers={} pre_nodes={} attn_nodes={} class_id={}",
        profile.n_ctx,
        profile.vocab_size,
        profile.layer_count,
        profile.pre_nodes.len(),
        profile.attn_nodes.len(),
        faster_hex::hex_string(profile.shape_profile_id().as_byte_slice())
    );

    // For reference: the registered row this class actually serves today.
    let court =
        kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .unwrap_or_else(|e| die(format!("the shipped court params do not build: {e:?}")));
    let registered = misaka_palw_base0::classes::canonical_class_by_model_id_v1(&court, MODEL_ID)
        .unwrap_or_else(|| die(format!("this build's catalog has no {MODEL_ID} row")));
    eprintln!(
        "[palw-model-gate] registered row: n_ctx={} canonical_job={:?} class_id={}",
        registered.profile.n_ctx,
        registered.canonical_job,
        faster_hex::hex_string(registered.profile.shape_profile_id().as_byte_slice())
    );

    // -------------------------------------------------------------------------------------
    // 2. The artifact and the tokenizer, opened the way the shipped worker opens them.
    // -------------------------------------------------------------------------------------
    let artifact_path = std::env::var("MISAKA_PALW_ARTIFACT").unwrap_or_else(|_| die("MISAKA_PALW_ARTIFACT is not set".into()));
    let tokenizer_path = std::env::var("MISAKA_PALW_TOKENIZER").unwrap_or_else(|_| die("MISAKA_PALW_TOKENIZER is not set".into()));
    let network_id = std::env::var("MISAKA_PALW_NETWORK_ID").unwrap_or_else(|_| "testnet-11".to_string());

    let started = std::time::Instant::now();
    let bytes = std::fs::read(&artifact_path).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let guard = MappedArtifactV1::verify_from_bytes(std::path::Path::new(&artifact_path), &bytes).unwrap_or_else(|e| die(e));
    let artifact = decode_artifact_file_v1(&bytes).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    let digest = artifact.artifact_digest();
    let tokenizer_commitment = artifact.tokenizer_commitment;
    eprintln!(
        "[palw-model-gate] artifact: vocab={} n_layers={} max_position={} digest={}",
        artifact.shape.vocab,
        artifact.shape.n_layers,
        artifact.shape.max_position,
        guard.digest_hex()
    );
    if (n_ctx as usize) > artifact.shape.max_position {
        die(format!("the artifact's rotary table covers {} positions and n_ctx {n_ctx} was asked for", artifact.shape.max_position));
    }
    let tokenizer_bytes = std::fs::read(&tokenizer_path).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));
    // **Is this the tokenizer the artifact was converted with?** The artifact carries a
    // commitment over the tokenizer file's bytes, so the question has an exact answer rather than
    // a filename's worth of confidence. Reported, not enforced: a reformatted-but-equivalent file
    // is a different byte string and still the right table, and the runtime does not enforce it
    // either.
    let observed = misaka_palw_base0::artifact::Base0ArtifactV1::tokenizer_commitment_of(&tokenizer_bytes);
    eprintln!(
        "[palw-model-gate] tokenizer: {} bytes; artifact commitment {}; this file {}; MATCH={}",
        tokenizer_bytes.len(),
        faster_hex::hex_string(tokenizer_commitment.as_byte_slice()),
        faster_hex::hex_string(observed.as_byte_slice()),
        observed == tokenizer_commitment
    );
    let tokenizer = QwenTokenizer::from_json(&tokenizer_bytes).unwrap_or_else(|e| die(format!("{tokenizer_path}: {e}")));
    let load_ms = started.elapsed().as_millis() as u64;

    // -------------------------------------------------------------------------------------
    // 3. The backend, from the WIDER profile. `from_registered_profile` compiles the graph
    //    against this build's kernels — a width this build cannot serve is refused here.
    // -------------------------------------------------------------------------------------
    let net = network_id.clone().into_bytes();
    let net_bytes = net.clone();
    let arc = std::sync::Arc::new(artifact);
    // Reported, because it is a fact about this row and not about the model: the ADR-0067
    // constructor compiles the declared graph against the artifact, and the DENSE ladder row is
    // built from `QWEN25_1_5B`, whose `rms_eps_q` is 1 while the converter wrote 256. The
    // REGISTERED row is built from the same constant and carries the same split — which is why
    // the shipped worker uses `Qwen25A16Backend::new` (no compiled plan, the artifact's epsilon
    // executes) and why this harness does too.
    match Qwen25A16Backend::from_registered_profile(arc.clone(), net.clone(), profile.clone(), registered.canonical_job) {
        Ok(_) => eprintln!("[palw-model-gate] from_registered_profile: the n_ctx {n_ctx} row compiles against this artifact"),
        Err(e) => eprintln!("[palw-model-gate] from_registered_profile REFUSES the n_ctx {n_ctx} row: {e}"),
    }
    match Qwen25A16Backend::from_registered_profile(arc.clone(), net.clone(), registered.profile.clone(), registered.canonical_job) {
        Ok(_) => eprintln!("[palw-model-gate] from_registered_profile: the REGISTERED n_ctx 16 row compiles against this artifact"),
        Err(e) => eprintln!("[palw-model-gate] from_registered_profile REFUSES the REGISTERED n_ctx 16 row too: {e}"),
    }
    // The shipped worker's own assembly (`palw-a16-fp-worker::load`).
    let engine_arc = arc.clone();
    let backend = Qwen25A16Backend::new(arc, net.clone(), profile.clone(), registered.canonical_job);
    eprintln!("[palw-model-gate] backend built; supports_court={}", {
        use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
        backend.supports_court()
    });

    let rt = FpWorkerRuntime::new(
        backend,
        &profile,
        tokenizer,
        FpWorkerFamilyV1 {
            model_id: MODEL_ID.to_string(),
            runtime_identity: digest,
            tokenizer_id: tokenizer_commitment,
            vocab: profile.vocab_size,
            retention_schema: "misaka.palw.fp-v3-a16-retention.v1",
            retention_family: "qwen25-a16",
            eog_token_names: QWEN_EOG_TOKEN_NAMES,
            artifact: Some(guard),
        },
        net,
        load_ms,
    )
    .unwrap_or_else(|e| die(e));
    eprintln!("[palw-model-gate] runtime: n_ctx={} vocab={} load_ms={}", rt.manifest().n_ctx, rt.manifest().vocab, rt.load_ms());

    // -------------------------------------------------------------------------------------
    // 4. The template's own cost, measured: the empty-content user turn.
    // -------------------------------------------------------------------------------------
    let (empty_segments, empty_displayed) = chatml_user_segments(rt.tokenizer(), "");
    let empty_ids = prompt_ids_for_input_v1(rt.tokenizer(), rt.manifest(), &PalwFpWorkerInputV3::Segments(empty_segments))
        .unwrap_or_else(|e| die(e));
    eprintln!(
        "[palw-model-gate] MEASURED template overhead: {} prompt tokens for zero content ({:?}); ids {:?}",
        empty_ids.len(),
        empty_displayed,
        empty_ids
    );

    let eog: Vec<u32> = rt.manifest().eog_token_ids.clone();
    eprintln!("[palw-model-gate] eog ids {eog:?}");

    let mut report = Vec::new();
    for case in cases() {
        if let Some(only) = &only {
            if &case.name != only {
                continue;
            }
        }
        let (segments, displayed) = chatml_user_segments(rt.tokenizer(), &case.prompt);
        let prompt_ids = match prompt_ids_for_input_v1(rt.tokenizer(), rt.manifest(), &PalwFpWorkerInputV3::Segments(segments)) {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("[palw-model-gate] {}: prompt refused: {e}", case.name);
                continue;
            }
        };
        let prefill = prompt_ids.len() as u32;
        if prefill >= n_ctx {
            eprintln!("[palw-model-gate] {}: prefill {prefill} already fills n_ctx {n_ctx} — no decode budget", case.name);
            report.push(serde_json::json!({
                "case": case.name, "prefill_tokens": prefill, "decode_budget": 0,
                "blocked": "prefill >= n_ctx",
            }));
            continue;
        }
        let wanted = n_ctx - prefill;

        let prompt_usize: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();

        // ---------------------------------------------------------------------------------
        // The WIDE arm: the same engine, the same selection rule, the same loop order as
        // `a16_execute_for_attempt_streaming_v1` — with the step capture removed, which is the
        // only thing the 2^22 ladder bounds. The ids are the ids that path would commit.
        // ---------------------------------------------------------------------------------
        if mode == "wide" || mode == "both" {
            use misaka_palw_base0::engine_a16::{A16Cache, A16Engine};
            let engine = A16Engine::new(&engine_arc).unwrap_or_else(|e| die(format!("the artifact is not an A16 class: {e:?}")));
            let mut cache = A16Cache::new(engine_arc.shape.n_layers);
            let t0 = std::time::Instant::now();
            let mut last = Vec::new();
            for (position, token) in prompt_usize.iter().enumerate() {
                last = engine
                    .forward_token(&mut cache, *token, position)
                    .unwrap_or_else(|e| die(format!("prefill at {position}: {e:?}")));
            }
            let mut generated: Vec<u32> = Vec::new();
            let mut next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&last) as u32;
            generated.push(next);
            let mut stopped_at_eog = eog.contains(&next);
            for call in 1..wanted as usize {
                if stopped_at_eog {
                    break;
                }
                let cache_position = prefill as usize + call - 1;
                let logits = engine
                    .forward_token(&mut cache, next as usize, cache_position)
                    .unwrap_or_else(|e| die(format!("decode at {cache_position}: {e:?}")));
                next = kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1(&logits) as u32;
                generated.push(next);
                stopped_at_eog = eog.contains(&next);
            }
            let ms = t0.elapsed().as_millis() as u64;
            let raw = render_answer_v1(rt.tokenizer(), &generated);
            let stop = generated.iter().position(|id| eog.contains(id));
            let answer = render_answer_v1(rt.tokenizer(), &generated[..stop.unwrap_or(generated.len())]);
            let raw_path = out.join(format!("{}.wide.raw", case.name));
            let ans_path = out.join(format!("{}.wide.answer", case.name));
            std::fs::write(&raw_path, &raw).unwrap_or_else(|e| die(format!("{}: {e}", raw_path.display())));
            std::fs::write(&ans_path, &answer).unwrap_or_else(|e| die(format!("{}: {e}", ans_path.display())));
            eprintln!(
                "[palw-model-gate] {} WIDE: prefill={prefill} budget={wanted} generated={} in {ms} ms; eog at {:?}; answer {} bytes",
                case.name,
                generated.len(),
                stop,
                answer.len()
            );
            eprintln!("---8<--- {} WIDE RAW ---8<---\n{}\n---8<--- end ---8<---", case.name, String::from_utf8_lossy(&raw));
            report.push(serde_json::json!({
                "case": case.name,
                "arm": "wide",
                "transformer": case.transformer,
                "n_ctx": n_ctx,
                "prompt": case.prompt,
                "prefill_tokens": prefill,
                "decode_budget": wanted,
                "generated": generated.len(),
                "execute_ms": ms,
                "eog_at": stop,
                "raw_bytes": raw.len(),
                "answer_bytes": answer.len(),
                "answer_path": ans_path.display().to_string(),
                "output_token_ids": generated.clone(),
            }));
        }
        if mode == "wide" {
            continue;
        }

        // The court's ladder is a real ceiling on the decode budget, so it is MEASURED rather
        // than assumed: the largest budget whose step leaf count fits `PALW_STEP_MAX_LEAVES`.
        let job_for = |decode: u32| PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: Hash64::default(),
            class_id: rt.manifest().class_id,
            executor_bond: TransactionOutpoint::default(),
            executor_pubkey: vec![0u8; 32],
            operator_id: Hash64::default(),
            anchor_block: Hash64::default(),
            anchor_daa: 0,
            job_nonce: [0u8; 32],
            tokenizer_id: rt.manifest().tokenizer_id,
            prompt_token_ids_hash: prompt_token_ids_hash_v2(&prompt_ids),
            prompt_tokens: prefill,
            decode_token_limit: decode,
            max_context_tokens: n_ctx,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
            prompt_mode: PALW_FP_PROMPT_MODE_USER,
        };
        let leaves_for = |decode: u32| -> Result<u64, String> {
            use kaspa_consensus_core::palw_fp_execution_v3::{PalwFpClassFactsV3, PalwFpRunFactsV3, palw_fp_job_context_v3};
            use kaspa_consensus_core::palw_freeprompt_v3::PalwFpStopReasonV3;
            let job = job_for(decode);
            let class = PalwFpClassFactsV3 {
                model_profile_id: rt.manifest().model_profile_id,
                runtime_manifest_hash: rt.manifest().runtime_manifest_hash,
                runtime_class_id: rt.manifest().runtime_class_id,
                shape_profile_id: rt.manifest().shape_profile_id,
                cu_ruleset_id: Hash64::default(),
            };
            let facts = PalwFpRunFactsV3 {
                decode_tokens_executed: decode,
                stop_reason: PalwFpStopReasonV3::ExactBudgetReached,
                full_logits_trace_root: Hash64::default(),
                activation_leg_root: Hash64::default(),
                checkpoint_leg_root: Hash64::default(),
                step_leg_root: Hash64::default(),
                step_leaf_count: 0,
            };
            let ctx = palw_fp_job_context_v3(&job, &class, &facts, &net_bytes).map_err(|e| format!("{e:?}"))?;
            kaspa_consensus_core::palw_step::step_leaf_count(&profile, &ctx).map_err(|e| format!("{e:?}"))
        };

        let mut decode = wanted;
        let mut leaves = None;
        while decode > 0 {
            match leaves_for(decode) {
                Ok(n) => {
                    leaves = Some(n);
                    break;
                }
                Err(_) => decode -= 1,
            }
        }
        let Some(leaves) = leaves else {
            eprintln!("[palw-model-gate] {}: no decode budget fits the step ladder at all", case.name);
            report.push(serde_json::json!({
                "case": case.name, "arm": "ladder", "n_ctx": n_ctx,
                "prefill_tokens": prefill, "wanted_decode": wanted,
                "granted_decode": 0, "step_leaves": serde_json::Value::Null,
            }));
            continue;
        };
        eprintln!(
            "[palw-model-gate] {}: MEASURED prefill={prefill} wanted_decode={wanted} granted_decode={decode} step_leaves={leaves}",
            case.name
        );

        if mode == "ladder" {
            report.push(serde_json::json!({
                "case": case.name, "arm": "ladder", "n_ctx": n_ctx,
                "prefill_tokens": prefill, "wanted_decode": wanted,
                "granted_decode": decode, "step_leaves": leaves,
            }));
            continue;
        }

        let job = job_for(decode);
        let t0 = std::time::Instant::now();
        let run = {
            use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
            match rt.backend().execute_free_prompt(&job, &prompt_usize) {
                Ok(run) => run,
                Err(e) => {
                    eprintln!("[palw-model-gate] {}: execute_free_prompt refused: {e}", case.name);
                    report.push(serde_json::json!({
                        "case": case.name, "prefill_tokens": prefill, "decode_budget": decode,
                        "step_leaves": leaves, "blocked": e,
                    }));
                    continue;
                }
            }
        };
        let ms = t0.elapsed().as_millis() as u64;

        let ids = &run.output_token_ids;
        let raw = render_answer_v1(rt.tokenizer(), ids);
        // The gateway's DISPLAY stop: the execution always runs to the budget, but a consumer's
        // answer is the prefix before the first end-of-generation id.
        let stop = ids.iter().position(|id| eog.contains(id));
        let answer = render_answer_v1(rt.tokenizer(), &ids[..stop.unwrap_or(ids.len())]);

        let raw_path = out.join(format!("{}.raw", case.name));
        let ans_path = out.join(format!("{}.answer", case.name));
        std::fs::write(&raw_path, &raw).unwrap_or_else(|e| die(format!("{}: {e}", raw_path.display())));
        std::fs::write(&ans_path, &answer).unwrap_or_else(|e| die(format!("{}: {e}", ans_path.display())));

        eprintln!(
            "[palw-model-gate] {}: executed {} tokens in {ms} ms; eog at {:?}; answer {} bytes",
            case.name,
            ids.len(),
            stop,
            answer.len()
        );
        eprintln!("---8<--- {} RAW ---8<---\n{}\n---8<--- end ---8<---", case.name, String::from_utf8_lossy(&raw));

        report.push(serde_json::json!({
            "case": case.name,
            "arm": "committed",
            "transformer": case.transformer,
            "n_ctx": n_ctx,
            "prompt": case.prompt,
            "displayed_prompt": displayed,
            "prefill_tokens": prefill,
            "wanted_decode": wanted,
            "granted_decode": decode,
            "step_leaves": leaves,
            "execute_ms": ms,
            "eog_at": stop,
            "raw_bytes": raw.len(),
            "answer_bytes": answer.len(),
            "answer_path": ans_path.display().to_string(),
            "raw_path": raw_path.display().to_string(),
            "execution_root": faster_hex::hex_string(run.outcome.execution_root.as_byte_slice()),
            "output_token_ids": ids.clone(),
        }));
    }

    let report_path = out.join(format!("report-n{n_ctx}.json"));
    std::fs::write(&report_path, serde_json::to_vec_pretty(&serde_json::json!(report)).unwrap())
        .unwrap_or_else(|e| die(format!("{}: {e}", report_path.display())));
    eprintln!("[palw-model-gate] report: {}", report_path.display());
}
