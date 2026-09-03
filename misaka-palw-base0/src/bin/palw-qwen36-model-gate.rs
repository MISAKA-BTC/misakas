//! **`palw-qwen36-model-gate` — the same question `palw-model-gate` asks, for the QWEN36 lane.**
//!
//! `palw-model-gate` answered it for the DENSE tier: real Qwen2.5-1.5B, at `n_ctx` 256, emits DSL
//! the shipped transformers accept. Two tiers are registered at genesis and the other one — the
//! qwen36 lineage, served by `palw-qwen36-fp-worker` over `Qwen36Backend` — has never been asked.
//!
//! **Why this is a second binary and not a `--backend` flag on the first.** The two gates share
//! the case list and the reporting; they share nothing else. The artifact container differs
//! (`decode_artifact_file_v1` over a read vs. `open_artifact` over an mmap), the tokenizer's
//! SOURCE differs (a `tokenizer.json` vs. the GGUF header — a `.palwq36` carries no tokenizer),
//! the backend differs, and the wide arm's engine, cache and plan are three unrelated concrete
//! types with no common trait to write the raw decode loop against. A selector would have to
//! duplicate the whole assembly inside two match arms and would be these two files concatenated
//! behind an `if`. The shipped tree already splits for exactly this reason — ADR-0077 Decision 1
//! says what stays in a per-family worker is "how the artifact and the tokenizer are opened, and
//! which catalog row the runtime embodies", which is precisely the delta — and keeping
//! `palw-model-gate.rs` byte-identical preserves a landed result someone may re-run.
//!
//! ```text
//!   MISAKA_PALW_ARTIFACT=... MISAKA_PALW_GGUF=... palw-qwen36-model-gate --n-ctx 256 --out <dir>
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
use kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_base0::fp_worker::{
    FpWorkerFamilyV1, FpWorkerRuntime, MappedArtifactV1, QWEN_EOG_TOKEN_NAMES, prompt_ids_for_input_v1, render_answer_v1,
};
use misaka_palw_base0::gguf::parse_directory;
use misaka_palw_base0::qwen36::open_artifact;
use misaka_palw_base0::qwen36_backend::Qwen36Backend;
use misaka_palw_base0::tokenizer::QwenTokenizer;
use std::io::Read;
use std::path::PathBuf;

/// The row the artifact on this machine belongs to. The genesis tier is
/// `Qwen3.6-35B-A3B/graph-v3`; those weights are not on this machine, and the artifact that IS
/// here is the SAME lane's `Qwen/Qwen3.5-2B/graph-v3` — same graph family, same executor, same
/// tokenizer, a smaller model in it. Overridable so the identical run can be pointed at the 35B
/// artifact on a host that holds it.
const DEFAULT_MODEL_ID: &str = "Qwen/Qwen3.5-2B/graph-v3";

fn die(msg: String) -> ! {
    eprintln!("[palw-qwen36-model-gate] fatal: {msg}");
    std::process::exit(1);
}

/// **The qwen36 ladder row at an arbitrary width, at an arbitrary geometry of the lineage.**
///
/// [`kaspa_consensus_core::palw_context_ladder::palw_qwen36_context_row_profile_v1`] is the
/// shipped ladder function and it hardcodes `QWEN36_35B_A3B` — the genesis tier's geometry, whose
/// weights are not on this machine. This is that function's body with the geometry lifted to a
/// parameter, and `assert_matches_shipped_ladder` below proves the lift is faithful by rebuilding
/// the 35B row through THIS function and comparing it to the shipped one. So the row under test is
/// the shipped ladder's row, evaluated at the geometry the artifact actually has.
fn qwen36_row_profile(geometry: PalwQwen36GeometryV1, n_ctx: u32) -> Result<PalwShapeProfileV3, String> {
    let geometry = kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(PalwQwen36GeometryV1 {
        n_ctx,
        ..geometry
    });
    let mut profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(geometry).map_err(|e| format!("{e:?}"))?;
    profile.state_chunk_map_id = if profile.full_attention_interval == 0 {
        kaspa_consensus_core::palw_state_chunk_map::gdn_state_chunk_map_id_v2()
    } else {
        kaspa_consensus_core::palw_state_chunk_map::hybrid_state_chunk_map_id_v2()
    };
    profile.validate_shape().map_err(|e| format!("{e:?}"))?;
    Ok(profile)
}

/// The lift above is only trustworthy if it reproduces the shipped ladder where the shipped ladder
/// is defined. A divergence here is a failed assertion, not a quietly different graph.
fn assert_matches_shipped_ladder(n_ctx: u32) {
    let shipped = kaspa_consensus_core::palw_context_ladder::palw_qwen36_context_row_profile_v1(n_ctx);
    let mine = qwen36_row_profile(kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B, n_ctx);
    match (shipped, mine) {
        (Ok(a), Ok(b)) => assert_eq!(
            a.shape_profile_id(),
            b.shape_profile_id(),
            "the harness's row builder is not the shipped qwen36 ladder at n_ctx {n_ctx}"
        ),
        (Err(_), Err(_)) => {}
        (a, b) => panic!("the harness's row builder disagrees with the shipped qwen36 ladder at n_ctx {n_ctx}: {a:?} vs {b:?}"),
    }
}

/// The GGUF header, grown until the directory parses — `palw-qwen36-fp-worker::read_gguf_header`,
/// verbatim, because the tokenizer this lane serves is the one in that header and nowhere else.
fn read_gguf_header(path: &str) -> Vec<u8> {
    let mut file = std::fs::File::open(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
    let mut buf = Vec::new();
    let mut want = 1usize << 22;
    loop {
        buf.resize(want, 0);
        let mut read = 0usize;
        while read < want {
            match file.read(&mut buf[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(e) => die(format!("{path}: {e}")),
            }
        }
        buf.truncate(read);
        if parse_directory(&buf).is_ok() || read < want {
            return buf;
        }
        want *= 2;
        if want > (1usize << 30) {
            die(format!("{path}: the header did not parse within a gigabyte"));
        }
        use std::io::Seek;
        file.rewind().unwrap_or_else(|e| die(format!("{path}: {e}")));
    }
}

/// **The Qwen3.5 generation prompt this tree's ChatML assembly does not emit.**
///
/// Measured, not guessed: the GGUF's own `tokenizer.chat_template` ends its `add_generation_prompt`
/// branch with `'<|im_start|>assistant\n'` followed by `'<think>\n'` when `enable_thinking` is
/// true and by `'<think>\n\n</think>\n\n'` otherwise — so under the model's OWN template the think
/// opener is part of the PROMPT in both modes, and the model never generates it.
/// [`misaka_palw_base0::tokenizer::qwen_chat_prompt`] stops at `assistant\n`, which leaves this
/// reasoning model to emit the block itself, into the answer bytes, where
/// `grammar.canonicalize(answer)` sees leading non-JSON and refuses. `--assistant-prefill`
/// supplies it so the two assemblies can be compared on the same model.
const QWEN35_NOTHINK_PREFILL: &str = "<think>\n\n</think>\n\n";

/// **The gateway's ChatML template, as segments** — the same reproduction `palw-model-gate` uses,
/// checked against this tree's other spelling so a divergence in either is a failed assertion and
/// not a silently different prompt. `assistant_prefill` is appended AFTER that check, so the
/// shipped template is verified on every run and the variant is visibly a variant.
fn chatml_user_segments(
    tokenizer: &QwenTokenizer,
    content: &str,
    assistant_prefill: &str,
) -> (Vec<PalwFpPromptSegmentV1>, String) {
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

    if !assistant_prefill.is_empty() {
        segments.push(PalwFpPromptSegmentV1::Text(assistant_prefill.as_bytes().to_vec()));
        displayed.push_str(assistant_prefill);
    }
    (segments, displayed)
}

struct Case {
    name: &'static str,
    transformer: &'static str,
    prompt: String,
}

/// **The dense gate's eight cases, verbatim.** They are not re-tuned for this lane on purpose: the
/// two tiers have to be compared on the same questions or the comparison says nothing.
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
    let mode = flag("--mode").unwrap_or_else(|| "both".to_string());
    // Empty by default: the SHIPPED assembly is what a gate measures unless it is told
    // otherwise. `--assistant-prefill nothink` is the model's own template's branch.
    let assistant_prefill = match flag("--assistant-prefill").as_deref() {
        None | Some("") | Some("shipped") => String::new(),
        Some("nothink") => QWEN35_NOTHINK_PREFILL.to_string(),
        Some(other) => other.to_string(),
    };
    std::fs::create_dir_all(&out).unwrap_or_else(|e| die(format!("{}: {e}", out.display())));

    let model_id = std::env::var("MISAKA_PALW_MODEL_ID").unwrap_or_else(|_| DEFAULT_MODEL_ID.to_string());
    let row = misaka_palw_base0::classes::qwen36_canonical_classes_v1()
        .into_iter()
        .find(|r| r.model_id == model_id)
        .unwrap_or_else(|| die(format!("this build's catalog has no {model_id} row")));
    if row.graph_version < 2 {
        die(format!("{model_id} is a legacy (graph-v1) row whose graph the court cannot adjudicate; use its /graph-v3 row"));
    }

    // -------------------------------------------------------------------------------------
    // 1. The profile, at the width under test. Refusals here ARE the result.
    // -------------------------------------------------------------------------------------
    for probe in [8u32, 16, 64, 256, 512] {
        assert_matches_shipped_ladder(probe);
    }
    eprintln!("[palw-qwen36-model-gate] the harness's row builder reproduces the shipped qwen36 ladder at 8/16/64/256/512");

    let profile = qwen36_row_profile(row.geometry, n_ctx)
        .unwrap_or_else(|e| die(format!("the ladder will not build a {model_id} row at n_ctx {n_ctx}: {e}")));
    eprintln!(
        "[palw-qwen36-model-gate] profile: n_ctx={} vocab={} layers={} full_attention_interval={} pre_nodes={} attn_nodes={} \
         class_id={}",
        profile.n_ctx,
        profile.vocab_size,
        profile.layer_count,
        profile.full_attention_interval,
        profile.pre_nodes.len(),
        profile.attn_nodes.len(),
        faster_hex::hex_string(profile.shape_profile_id().as_byte_slice())
    );

    // For reference: the registered row this class actually serves today.
    let registered = row.profile().unwrap_or_else(|e| die(format!("{model_id}: the row's geometry does not project: {e:?}")));
    eprintln!(
        "[palw-qwen36-model-gate] registered row: model_id={model_id} n_ctx={} canonical_job={:?} class_id={}",
        registered.n_ctx,
        row.canonical_job,
        faster_hex::hex_string(registered.shape_profile_id().as_byte_slice())
    );

    // -------------------------------------------------------------------------------------
    // 2. The artifact and the tokenizer, opened the way the shipped worker opens them:
    //    the `.palwq36` container mapped and verified by reading, and the tokenizer out of the
    //    GGUF header — a `.palwq36` deliberately carries no tokenizer.
    // -------------------------------------------------------------------------------------
    let artifact_path = std::env::var("MISAKA_PALW_ARTIFACT").unwrap_or_else(|_| die("MISAKA_PALW_ARTIFACT is not set".into()));
    let gguf_path =
        std::env::var("MISAKA_PALW_GGUF").unwrap_or_else(|_| die("MISAKA_PALW_GGUF is not set (the tokenizer source)".into()));
    let network_id = std::env::var("MISAKA_PALW_NETWORK_ID").unwrap_or_else(|_| "testnet-11".to_string());

    let started = std::time::Instant::now();
    let header = read_gguf_header(&gguf_path);
    let directory = parse_directory(&header).unwrap_or_else(|e| die(format!("{gguf_path}: {e}")));
    let get = |key: &str| directory.metadata.get(key);
    let tokens = get("tokenizer.ggml.tokens").and_then(|v| v.as_strings()).unwrap_or_else(|| die("no tokenizer.ggml.tokens".into()));
    let merges = get("tokenizer.ggml.merges").and_then(|v| v.as_strings()).unwrap_or_else(|| die("no tokenizer.ggml.merges".into()));
    let types = get("tokenizer.ggml.token_type").and_then(|v| v.as_ints()).unwrap_or(&[]);
    let gguf_arch = get("general.architecture").and_then(|v| v.as_str()).unwrap_or("<none>").to_string();
    let gguf_name = get("general.name").and_then(|v| v.as_str()).unwrap_or("<none>").to_string();
    let gguf_tokens = tokens.len();
    let gguf_merges = merges.len();
    let tokenizer = QwenTokenizer::from_gguf(tokens, merges, types).unwrap_or_else(|e| die(format!("{gguf_path}: {e}")));
    drop(header);

    let guard = MappedArtifactV1::verify_by_reading(std::path::Path::new(&artifact_path)).unwrap_or_else(|e| die(e));
    let artifact = open_artifact(std::path::Path::new(&artifact_path)).unwrap_or_else(|e| die(format!("{artifact_path}: {e}")));
    row.shape_matches(&artifact.shape).unwrap_or_else(|e| die(format!("{artifact_path} is not a {model_id} artifact: {e}")));
    let load_ms = started.elapsed().as_millis() as u64;
    eprintln!(
        "[palw-qwen36-model-gate] artifact: vocab={} n_layers={} max_position={} eps_q={} digest={}",
        artifact.shape.vocab,
        artifact.shape.n_layers(),
        artifact.shape.max_position,
        artifact.shape.eps_q,
        guard.digest_hex()
    );
    // **The tokenizer question, answered by measurement rather than by filename.** A `.palwq36`
    // carries no tokenizer commitment (the job's `tokenizer_id` is zero and nothing on chain checks
    // it), so the only check available is the one that actually matters: the table has to span
    // exactly the vocabulary the artifact's output layer produces. A tokenizer with fewer entries
    // cannot name every id the model can emit, and one with more names ids the model cannot.
    eprintln!(
        "[palw-qwen36-model-gate] tokenizer: gguf arch={gguf_arch:?} name={gguf_name:?} tokens={gguf_tokens} merges={gguf_merges}; \
         artifact vocab={}; profile vocab={}; SPANS_VOCAB={}",
        artifact.shape.vocab,
        profile.vocab_size,
        gguf_tokens == artifact.shape.vocab
    );
    if gguf_tokens != artifact.shape.vocab {
        die(format!(
            "the tokenizer at {gguf_path} has {gguf_tokens} tokens and this artifact's vocabulary is {} — a mismatched \
             tokenizer produces confident garbage, which is worse than no answer",
            artifact.shape.vocab
        ));
    }
    if (n_ctx as usize) > artifact.shape.max_position {
        die(format!("the artifact's rotary table covers {} positions and n_ctx {n_ctx} was asked for", artifact.shape.max_position));
    }

    // -------------------------------------------------------------------------------------
    // 3. The backend, from the WIDER profile. `from_registered_profile` compiles the graph
    //    against this build's kernels — a width this build cannot serve is refused here.
    // -------------------------------------------------------------------------------------
    let net = network_id.clone().into_bytes();
    let net_bytes = net.clone();
    let arc = std::sync::Arc::new(artifact);
    match Qwen36Backend::from_registered_profile(arc.clone(), net.clone(), profile.clone(), row.canonical_job) {
        Ok(_) => eprintln!("[palw-qwen36-model-gate] from_registered_profile: the n_ctx {n_ctx} row compiles against this artifact"),
        Err(e) => eprintln!("[palw-qwen36-model-gate] from_registered_profile REFUSES the n_ctx {n_ctx} row: {e}"),
    }
    match Qwen36Backend::from_registered_profile(arc.clone(), net.clone(), registered.clone(), row.canonical_job) {
        Ok(_) => eprintln!(
            "[palw-qwen36-model-gate] from_registered_profile: the REGISTERED n_ctx {} row compiles against this artifact",
            registered.n_ctx
        ),
        Err(e) => eprintln!(
            "[palw-qwen36-model-gate] from_registered_profile REFUSES the REGISTERED n_ctx {} row too: {e}",
            registered.n_ctx
        ),
    }

    // The shipped worker's own assembly (`palw-qwen36-fp-worker::load`), at the width under test.
    let engine_arc = arc.clone();
    let backend = Qwen36Backend::with_class_profile(arc, model_id.clone(), row.canonical_job, profile.clone(), net.clone());
    let shape_id = backend.shape_id();
    {
        use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
        eprintln!("[palw-qwen36-model-gate] backend built; supports_court={}", backend.supports_court());
        if !backend.supports_court() {
            eprintln!(
                "[palw-qwen36-model-gate] WARNING: this backend serves no registered graph at n_ctx {n_ctx}, so the committed \
                 arm cannot run. The wide arm still measures the model."
            );
        }
    }

    let rt = FpWorkerRuntime::new(
        backend,
        &profile,
        tokenizer,
        FpWorkerFamilyV1 {
            model_id: model_id.clone(),
            runtime_identity: shape_id,
            tokenizer_id: Hash64::default(),
            vocab: profile.vocab_size,
            retention_schema: "misaka.palw.fp-v3-qwen36-retention.v1",
            retention_family: "qwen36",
            eog_token_names: QWEN_EOG_TOKEN_NAMES,
            artifact: Some(guard),
        },
        net,
        load_ms,
    )
    .unwrap_or_else(|e| die(e));
    eprintln!(
        "[palw-qwen36-model-gate] runtime: n_ctx={} vocab={} load_ms={}",
        rt.manifest().n_ctx,
        rt.manifest().vocab,
        rt.load_ms()
    );

    // -------------------------------------------------------------------------------------
    // 4. The template's own cost, measured: the empty-content user turn.
    // -------------------------------------------------------------------------------------
    let (empty_segments, empty_displayed) = chatml_user_segments(rt.tokenizer(), "", &assistant_prefill);
    let empty_ids = prompt_ids_for_input_v1(rt.tokenizer(), rt.manifest(), &PalwFpWorkerInputV3::Segments(empty_segments))
        .unwrap_or_else(|e| die(e));
    eprintln!(
        "[palw-qwen36-model-gate] MEASURED template overhead: {} prompt tokens for zero content ({:?}); ids {:?}",
        empty_ids.len(),
        empty_displayed,
        empty_ids
    );

    eprintln!("[palw-qwen36-model-gate] assistant_prefill: {assistant_prefill:?}");
    let eog: Vec<u32> = rt.manifest().eog_token_ids.clone();
    eprintln!("[palw-qwen36-model-gate] eog ids {eog:?}");

    // The wide arm's plan: the SAME `plan_from_profile` the backend compiled, so the untraced walk
    // and the captured one are the same arithmetic.
    let wide_plan = misaka_palw_base0::qwen36::Qwen36Engine::new(&engine_arc).plan_from_profile(&profile);

    let mut report = Vec::new();
    for case in cases() {
        if let Some(only) = &only {
            if &case.name != only {
                continue;
            }
        }
        let (segments, displayed) = chatml_user_segments(rt.tokenizer(), &case.prompt, &assistant_prefill);
        let prompt_ids = match prompt_ids_for_input_v1(rt.tokenizer(), rt.manifest(), &PalwFpWorkerInputV3::Segments(segments)) {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("[palw-qwen36-model-gate] {}: prompt refused: {e}", case.name);
                continue;
            }
        };
        let prefill = prompt_ids.len() as u32;
        if prefill >= n_ctx {
            eprintln!("[palw-qwen36-model-gate] {}: prefill {prefill} already fills n_ctx {n_ctx} — no decode budget", case.name);
            report.push(serde_json::json!({
                "case": case.name, "prefill_tokens": prefill, "decode_budget": 0,
                "blocked": "prefill >= n_ctx",
            }));
            continue;
        }
        let wanted = n_ctx - prefill;
        let prompt_usize: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();

        // ---------------------------------------------------------------------------------
        // The WIDE arm: the same engine, the same plan, the same selection rule, the same loop
        // order as `qwen36_execute_for_attempt_streaming_v1` — with the step capture removed,
        // which is the only thing the 2^22 ladder bounds. The ids are the ids that path commits.
        // ---------------------------------------------------------------------------------
        if mode == "wide" || mode == "both" {
            use misaka_palw_base0::qwen36::{Qwen36Cache, Qwen36Engine};
            let plan = match &wide_plan {
                Ok(plan) => plan,
                Err(e) => die(format!("the n_ctx {n_ctx} row does not plan against this artifact: {e}")),
            };
            let engine = Qwen36Engine::new(&engine_arc);
            let mut cache = Qwen36Cache::new(&engine_arc.shape);
            let t0 = std::time::Instant::now();
            let mut last = Vec::new();
            for (position, token) in prompt_usize.iter().enumerate() {
                last = engine
                    .forward_token_planned_logits(plan, &mut cache, *token, position)
                    .unwrap_or_else(|e| die(format!("prefill at {position}: {e}")));
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
                if cache_position >= engine_arc.shape.max_position {
                    break;
                }
                let logits = engine
                    .forward_token_planned_logits(plan, &mut cache, next as usize, cache_position)
                    .unwrap_or_else(|e| die(format!("decode at {cache_position}: {e}")));
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
                "[palw-qwen36-model-gate] {} WIDE: prefill={prefill} budget={wanted} generated={} in {ms} ms; eog at {:?}; \
                 answer {} bytes",
                case.name,
                generated.len(),
                stop,
                answer.len()
            );
            eprintln!("---8<--- {} WIDE RAW ---8<---\n{}\n---8<--- end ---8<---", case.name, String::from_utf8_lossy(&raw));
            report.push(serde_json::json!({
                "case": case.name,
                "arm": "wide",
                "model_id": model_id,
                "transformer": case.transformer,
                "n_ctx": n_ctx,
                "assistant_prefill": assistant_prefill,
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
            eprintln!("[palw-qwen36-model-gate] {}: no decode budget fits the step ladder at all", case.name);
            report.push(serde_json::json!({
                "case": case.name, "arm": "ladder", "n_ctx": n_ctx,
                "prefill_tokens": prefill, "wanted_decode": wanted,
                "granted_decode": 0, "step_leaves": serde_json::Value::Null,
            }));
            continue;
        };
        eprintln!(
            "[palw-qwen36-model-gate] {}: MEASURED prefill={prefill} wanted_decode={wanted} granted_decode={decode} \
             step_leaves={leaves}",
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
                    eprintln!("[palw-qwen36-model-gate] {}: execute_free_prompt refused: {e}", case.name);
                    report.push(serde_json::json!({
                        "case": case.name, "arm": "committed", "prefill_tokens": prefill, "decode_budget": decode,
                        "step_leaves": leaves, "blocked": e,
                    }));
                    continue;
                }
            }
        };
        let ms = t0.elapsed().as_millis() as u64;

        let ids = &run.output_token_ids;
        let raw = render_answer_v1(rt.tokenizer(), ids);
        let stop = ids.iter().position(|id| eog.contains(id));
        let answer = render_answer_v1(rt.tokenizer(), &ids[..stop.unwrap_or(ids.len())]);

        let raw_path = out.join(format!("{}.raw", case.name));
        let ans_path = out.join(format!("{}.answer", case.name));
        std::fs::write(&raw_path, &raw).unwrap_or_else(|e| die(format!("{}: {e}", raw_path.display())));
        std::fs::write(&ans_path, &answer).unwrap_or_else(|e| die(format!("{}: {e}", ans_path.display())));

        eprintln!(
            "[palw-qwen36-model-gate] {}: executed {} tokens in {ms} ms; eog at {:?}; answer {} bytes",
            case.name,
            ids.len(),
            stop,
            answer.len()
        );
        eprintln!("---8<--- {} RAW ---8<---\n{}\n---8<--- end ---8<---", case.name, String::from_utf8_lossy(&raw));

        report.push(serde_json::json!({
            "case": case.name,
            "arm": "committed",
            "model_id": model_id,
            "transformer": case.transformer,
            "n_ctx": n_ctx,
            "assistant_prefill": assistant_prefill,
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

    let tag = if assistant_prefill.is_empty() { "shipped".to_string() } else { "nothink".to_string() };
    let report_path = out.join(format!("report-n{n_ctx}-{tag}.json"));
    std::fs::write(&report_path, serde_json::to_vec_pretty(&serde_json::json!(report)).unwrap())
        .unwrap_or_else(|e| die(format!("{}: {e}", report_path.display())));
    eprintln!("[palw-qwen36-model-gate] report: {}", report_path.display());
}
