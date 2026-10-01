//! **The model preflight** (RFC-0002 Part II §II.2, task 7): a verdict from headers, before the weights.
//!
//! The fixtures are the lowerer's tiny Hugging Face checkpoints and GGUF files; the truncated and header-only
//! variants are made here from them, so every test that says "headers only" proves it by removing the data.

use misaka_palw_sdk::preflight::{Depth, Options, Report, StageStatus, run};
use std::path::{Path, PathBuf};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures").join(rel)
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("preflight-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch");
    d
}

/// A copy of a fixture directory with a tokenizer beside it (the fixtures carry none), and — when `header_only` — every
/// safetensors file cut after its header.
fn copy_fixture(src: &Path, tag: &str, header_only: bool) -> PathBuf {
    let dst = scratch(tag);
    std::fs::write(dst.join("tokenizer.json"), "{}").expect("tokenizer");
    for e in std::fs::read_dir(src).expect("fixture").flatten() {
        let name = e.file_name();
        let bytes = std::fs::read(e.path()).expect("read");
        let bytes = if header_only && name.to_string_lossy().ends_with(".safetensors") {
            let n = u64::from_le_bytes(bytes[..8].try_into().expect("8")) as usize;
            bytes[..8 + n].to_vec()
        } else {
            bytes
        };
        std::fs::write(dst.join(&name), bytes).expect("write");
    }
    dst
}

fn opts(depth: Depth) -> Options {
    Options { depth, network: Some("testnet-12".into()), ..Options::default() }
}

fn codes(r: &Report) -> Vec<String> {
    r.blockers()
        .iter()
        .map(|b| match &b.arg {
            Some(a) => format!("{}({a})", b.code),
            None => b.code.clone(),
        })
        .collect()
}

#[test]
fn a_directory_is_judged_from_its_headers_and_removing_the_data_changes_nothing() {
    let whole = copy_fixture(&fixture("hf/llama"), "llama-whole", false);
    let full = run(&whole, &opts(Depth::Headers)).expect("preflight");
    let headers = copy_fixture(&fixture("hf/llama"), "llama-headers", true);
    let bare = run(&headers, &opts(Depth::Headers)).expect("preflight");
    assert_eq!(full.verdict.convert.status, StageStatus::Ok, "{}", full.render());
    assert_eq!(bare.verdict.convert.status, StageStatus::Ok, "{}", bare.render());
    assert_eq!(full.tensors.checked, "shapes");
    assert_eq!(bare.tensors.checked, "shapes");
    assert_eq!((full.tensors.bound, full.tensors.missing_total), (bare.tensors.bound, bare.tensors.missing_total));
    assert_eq!(full.model.as_ref().map(|m| m.spec_digest.clone()), bare.model.as_ref().map(|m| m.spec_digest.clone()));
    // What was read is the header: a fraction of the weights.
    let weights = full.source.weight_bytes.expect("declared weight bytes");
    assert!(full.input.bytes_read < weights, "read {} of {weights}", full.input.bytes_read);
    assert!(bare.source.shards.iter().all(|s| !s.complete), "the copy holds headers only");
    assert!(bare.notes.iter().any(|n| n.contains("header")), "{:?}", bare.notes);
    // The same bytes in, the same bytes out.
    assert_eq!(full.to_json(), run(&whole, &opts(Depth::Headers)).expect("again").to_json());
    let _ = std::fs::remove_dir_all(headers);
    let _ = std::fs::remove_dir_all(whole);
}

#[test]
fn the_first_line_says_which_mode_ran_and_a_shallow_depth_says_what_it_could_not() {
    let dir = copy_fixture(&fixture("hf/llama"), "llama-shallow", false);
    let r = run(&dir, &Options { depth: Depth::Shape, network: None, ..Options::default() }).expect("preflight");
    let line = r.mode_line();
    assert!(line.starts_with("preflight: model · Hugging Face directory · depth headers (requested shape)"), "{line}");
    assert_eq!(r.depth.stopped_at.as_deref(), Some("network not given (--network <id>)"));
    // Every verdict that needs the deeper depth is unknown, never ok.
    assert_eq!(r.verdict.register.status, StageStatus::Unknown);
    assert_eq!(r.verdict.mine.status, StageStatus::Unknown);
    assert!(!r.registrable() || r.verdict.convert.status == StageStatus::Ok);
}

/// A GGUF v3 file: metadata (strings and u32), a tensor table, and — only when `with_data` — aligned data.
fn gguf_bytes(meta: &[(&str, Result<&str, u32>)], tensors: &[(&str, Vec<u64>, u32, usize)], with_data: bool) -> Vec<u8> {
    fn st(out: &mut Vec<u8>, s: &str) {
        out.extend((s.len() as u64).to_le_bytes());
        out.extend(s.as_bytes());
    }
    let mut out = b"GGUF".to_vec();
    out.extend(3u32.to_le_bytes());
    out.extend((tensors.len() as u64).to_le_bytes());
    out.extend((meta.len() as u64).to_le_bytes());
    for (k, v) in meta {
        st(&mut out, k);
        match v {
            Ok(text) => {
                out.extend(8u32.to_le_bytes());
                st(&mut out, text);
            }
            Err(n) => {
                out.extend(4u32.to_le_bytes());
                out.extend(n.to_le_bytes());
            }
        }
    }
    let mut off = 0u64;
    for (name, dims, ty, bytes) in tensors {
        st(&mut out, name);
        out.extend((dims.len() as u32).to_le_bytes());
        for d in dims {
            out.extend(d.to_le_bytes());
        }
        out.extend(ty.to_le_bytes());
        out.extend(off.to_le_bytes());
        off += (*bytes as u64).div_ceil(32) * 32;
    }
    while !out.len().is_multiple_of(32) {
        out.push(0);
    }
    if with_data {
        out.extend(std::iter::repeat_n(0u8, off as usize));
    }
    out
}

#[test]
fn a_gguf_of_custom_types_with_an_mmproj_is_refused_by_name_and_the_projector_is_a_note() {
    let dir = scratch("gguf-custom");
    // The shape of the Mitsuba-ComfyUI-27B-GGUF case: an architecture this build has no mapping for, two types no
    // descriptor describes, a vision projector beside it — and a file that holds its header and none of its 27 GB.
    let bytes = gguf_bytes(
        &[
            ("general.architecture", Ok("qwen38")),
            ("general.file_type", Err(901)),
            ("general.name", Ok("Mitsuba-ComfyUI-27B")),
            ("tokenizer.ggml.model", Ok("gpt2")),
        ],
        &[
            ("blk.0.attn_q.weight", vec![256, 64], 9100, 8 << 20),
            ("blk.0.ffn_up.weight", vec![256, 64], 9101, 8 << 20),
            ("blk.0.attn_norm.weight", vec![64], 0, 256),
        ],
        false,
    );
    std::fs::write(dir.join("model.gguf"), &bytes).expect("gguf");
    std::fs::write(dir.join("mmproj-model-f16.gguf"), b"GGUF").expect("mmproj");
    let r = run(&dir.join("model.gguf"), &opts(Depth::Shape)).expect("preflight");
    let c = codes(&r);
    assert!(c.iter().any(|x| x == "QUANT_NO_DESCRIPTOR(ggml/9100)"), "{c:?}\n{}", r.render());
    assert!(c.iter().any(|x| x == "QUANT_NO_DESCRIPTOR(ggml/9101)"), "{c:?}");
    assert!(c.iter().any(|x| x == "ARCH_REFUSED"), "{c:?}");
    assert_eq!(r.verdict.convert.status, StageStatus::Blocked);
    // The refusal names the tensors and the safe paths, not a bare error.
    let b = r.verdict.convert.blockers.iter().find(|b| b.arg.as_deref() == Some("ggml/9100")).expect("blocker");
    assert_eq!(b.evidence, vec!["blk.0.attn_q.weight".to_string()]);
    assert!(b.safe_paths.iter().any(|p| p.contains("--quant-format")), "{:?}", b.safe_paths);
    // The register stage had no program to judge, and says so rather than ok.
    assert_eq!(r.verdict.register.status, StageStatus::Unknown);
    // The file is read for its header alone (to the alignment padding that follows it).
    assert!(bytes.len() as u64 - r.input.bytes_read < 32, "read {} of {} bytes", r.input.bytes_read, bytes.len());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_tiny_hybrid_with_sparse_attention_and_hashed_embeddings_is_read_as_features_not_as_a_qwen4_parser() {
    let dir = copy_fixture(&fixture("hf-q4/qwen4_exp"), "qwen4-exp", true);
    let r = run(&dir, &opts(Depth::Shape)).expect("preflight");
    let m = r.model.as_ref().expect("model");
    // Read by a data adapter into registry features: the report names the combination, whether or not every lowering exists yet.
    let ids: Vec<&str> = m.features.iter().map(|f| f.id.as_str()).collect();
    for want in ["ATTN_SPARSE_BLOCK_V1", "EMBED_NGRAM_PLE_V1", "RESIDUAL_GATED_HC_V1"] {
        assert!(ids.contains(&want), "{want} not among {ids:?}");
    }
    assert!(matches!(m.adapter, misaka_palw_tir_lower::hf_schema::AdapterSource::BuiltIn { ref id, .. } if id == "qwen4-exp"));
    // A feature the lowerer does not lower yet is a named blocker (ARCH_NEEDS_FEATURE with the feature), never a parse failure and
    // never a Qwen4 special case; once lane G lowers them the verdict is ok and the level B.
    for b in r.verdict.convert.blockers.iter() {
        assert_eq!(b.code, "ARCH_NEEDS_FEATURE", "{}", r.render());
        assert!(ids.contains(&b.arg.as_deref().unwrap_or("")));
    }
    assert_eq!(m.level == "C", !r.verdict.convert.blockers.is_empty(), "{}", r.render());
    let _ = std::fs::remove_dir_all(dir);
}

/// A copy of a fixture whose safetensors header is rewritten by `edit` (a header only: the data is not needed to be judged).
fn edited_header_copy(src: &Path, tag: &str, edit: &dyn Fn(&mut serde_json::Map<String, serde_json::Value>)) -> PathBuf {
    let dst = copy_fixture(src, tag, true);
    let path = dst.join("model.safetensors");
    let bytes = std::fs::read(&path).expect("read");
    let n = u64::from_le_bytes(bytes[..8].try_into().expect("8")) as usize;
    let mut header: serde_json::Value = serde_json::from_slice(&bytes[8..8 + n]).expect("header");
    edit(header.as_object_mut().expect("object"));
    let json = serde_json::to_vec(&header).expect("json");
    let mut out = (json.len() as u64).to_le_bytes().to_vec();
    out.extend(json);
    std::fs::write(&path, out).expect("write");
    dst
}

fn with_config(src: &Path, tag: &str, edit: &dyn Fn(&mut serde_json::Value)) -> PathBuf {
    let dst = copy_fixture(src, tag, true);
    let path = dst.join("config.json");
    let mut c: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("config")).expect("json");
    edit(&mut c);
    std::fs::write(&path, c.to_string()).expect("write");
    dst
}

#[test]
fn a_vision_language_model_is_its_text_decoder_and_the_vision_tower_need_not_be_downloaded() {
    let dir = copy_fixture(&fixture("hf-vis/llava"), "llava", true);
    let r = run(&dir, &opts(Depth::Headers)).expect("preflight");
    assert_eq!(r.verdict.convert.status, StageStatus::Ok, "{}", r.render());
    let scope = r.scope.as_ref().expect("scope");
    assert!(scope.text_only);
    let kinds: Vec<&str> = scope.excluded.iter().map(|e| e.kind.as_str()).collect();
    assert!(kinds.contains(&"vision") && kinds.contains(&"projector"), "{kinds:?}");
    let a = r.artifact.as_ref().expect("artifact");
    assert!(a.left_out_bytes > 0, "the vision tower's bytes are said");
    assert!(a.download_bytes_needed.expect("needed") + a.left_out_bytes <= a.download_bytes_total.expect("total"));
    assert!(r.notes.iter().any(|n| n.contains("text stage only")), "{:?}", r.notes);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_described_quantisation_is_read_through_its_descriptor_and_named_in_the_storage_table() {
    for (name, descriptor, stored) in [("hf-quant/gptq_b4_g32", "GPTQ", "I32"), ("hf-quant/bnb_int8", "BNB_INT8", "I8")] {
        let dir = copy_fixture(&fixture(name), "quant", true);
        let r = run(&dir, &opts(Depth::Headers)).expect("preflight");
        assert_eq!(r.verdict.convert.status, StageStatus::Ok, "{name}: {}", r.render());
        assert_eq!(r.storage.quant_descriptor.as_ref().map(|d| d.name.as_str()), Some(descriptor), "{name}");
        let row = r.storage.rows.iter().find(|x| x.storage == stored).expect("a storage row");
        assert_eq!((row.status.as_str(), row.descriptor.as_ref().map(|d| d.name.as_str())), ("described", Some(descriptor)));
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_quantisation_no_descriptor_reads_is_refused_by_name_with_the_safe_paths() {
    let dir = with_config(&fixture("hf/llama"), "unknown-quant", &|c| {
        c["quantization_config"] = serde_json::json!({ "quant_method": "zzz-quant", "bits": 3 });
    });
    let r = run(&dir, &opts(Depth::Headers)).expect("preflight");
    assert!(codes(&r).contains(&"QUANT_NO_DESCRIPTOR(config/zzz-quant)".to_string()), "{:?}", codes(&r));
    let b = r.blockers().into_iter().find(|b| b.code == "QUANT_NO_DESCRIPTOR").expect("blocker");
    assert!(b.safe_paths.iter().any(|p| p.contains("--quant-format")) && b.evidence.iter().any(|e| e.contains("gptq")), "{b:?}");
    let _ = std::fs::remove_dir_all(dir);

    // A method that is known and not described yet says so, with the note on what describing it takes.
    let dir = with_config(&fixture("hf/llama"), "known-quant", &|c| {
        c["quantization_config"] = serde_json::json!({ "quant_method": "aqlm" });
    });
    let r = run(&dir, &opts(Depth::Headers)).expect("preflight");
    assert!(codes(&r).contains(&"QUANT_KNOWN_UNDESCRIBED(aqlm)".to_string()), "{:?}", codes(&r));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_tensor_the_program_reads_that_is_missing_or_the_wrong_shape_is_found_before_any_download() {
    let missing = edited_header_copy(&fixture("hf/llama"), "missing", &|h| {
        let v = h.remove("model.norm.weight").expect("a norm");
        h.insert("model.norm.weight.renamed".into(), v);
    });
    let r = run(&missing, &opts(Depth::Headers)).expect("preflight");
    assert!(codes(&r).iter().any(|c| c.starts_with("TENSOR_MISSING(")), "{:?}\n{}", codes(&r), r.render());
    assert_eq!(r.tensors.missing_total, 1);
    assert_eq!(r.tensors.unused_total, 1, "the renamed tensor is one nothing reads");
    let _ = std::fs::remove_dir_all(missing);

    let wrong = edited_header_copy(&fixture("hf/llama"), "shape", &|h| {
        h["model.norm.weight"]["shape"] = serde_json::json!([31]);
    });
    let r = run(&wrong, &opts(Depth::Headers)).expect("preflight");
    assert!(codes(&r).iter().any(|c| c.starts_with("TENSOR_SHAPE(")), "{:?}", codes(&r));
    let b = r.blockers().into_iter().find(|b| b.code == "TENSOR_SHAPE").expect("blocker");
    assert!(b.evidence.iter().any(|e| e.contains("[31]") && e.contains("[32]")), "{b:?}");
    let _ = std::fs::remove_dir_all(wrong);
}

#[test]
fn the_chain_conditions_are_shown_as_needed_against_limit_at_a_height() {
    let dir = copy_fixture(&fixture("hf/llama"), "chain-t12", true);
    let r = run(&dir, &opts(Depth::Shape)).expect("preflight");
    assert_eq!(r.depth.reached, Depth::Shape, "{}", r.render());
    let net = r.network.as_ref().expect("network");
    assert_eq!((net.id.as_str(), net.tir_armed), ("testnet-12", true));
    assert_eq!(r.verdict.register.status, StageStatus::Ok, "{}", r.render());
    // The context is the widest the gate admits: the program's own widest is past the canonical prompt's inline bound.
    let l = r.admission.as_ref().and_then(|a| a.layout.as_ref()).expect("layout");
    assert!(l.searched && l.max_context == 32_783 && l.widest_context > l.max_context, "{l:?}");
    for id in ["court_window", "canonical_job", "close_bytes", "terminal_macs", "da_ladder", "fence:palw_tir_v1"] {
        let c = r.chain.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no condition {id}"));
        assert_eq!(c.ok, Some(true), "{c:?}");
    }
    let w = r.chain.iter().find(|c| c.id == "court_window").expect("window");
    assert!(w.needed.expect("needed") < w.limit.expect("limit"), "{w:?}");
    assert!(r.seat.as_ref().is_some_and(|s| s.fits) && r.forecast.as_ref().is_some_and(|f| f.required_ready_seats == 7));
    // The JSON carries no path and no clock.
    let json = r.to_json();
    assert!(!json.contains(dir.to_string_lossy().as_ref()) && !json.contains("timestamp"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_context_past_the_canonical_prompts_bound_is_named_with_the_context_that_registers() {
    let dir = copy_fixture(&fixture("hf/llama"), "chain-ctx", true);
    let r = run(&dir, &Options { max_context: Some(262_144), ..opts(Depth::Shape) }).expect("preflight");
    let b = r.blockers().into_iter().find(|b| b.code == "CANONICAL_JOB_OUT_OF_BOUNDS").expect("blocker");
    assert_eq!((b.have, b.need), (Some(32_767), Some(4_096)), "{b:?}");
    assert!(b.safe_paths.iter().any(|p| p.contains("32,783")));
    assert_eq!(r.verdict.register.status, StageStatus::Blocked);
    assert!(!r.registrable());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_court_window_that_is_too_short_is_named_early_with_the_numbers_and_the_context_that_fits() {
    // The window is a function of the context and the history tile: devnet's court has 300 DAA, and at 65,536 positions it needs 308.
    let dir = copy_fixture(&fixture("hf/llama"), "chain-window", true);
    let r = run(&dir, &Options { network: Some("devnet".into()), max_context: Some(65_536), ..opts(Depth::Shape) }).expect("preflight");
    let b = r.blockers().into_iter().find(|b| b.code == "COURT_WINDOW_EXCEEDED").expect("blocker");
    assert_eq!((b.have, b.need), (Some(308), Some(300)), "{b:?}");
    assert!(b.safe_paths.iter().any(|p| p.contains("--max-context 16384")), "{:?}", b.safe_paths);
    // The other wall it meets is named too, not only the first: 8,191 prompt ids against J5b's 4,096.
    assert!(codes(&r).contains(&"CANONICAL_JOB_OUT_OF_BOUNDS".to_string()), "{:?}", codes(&r));
    // And the IR fence this network has not armed.
    assert!(codes(&r).contains(&"FENCE_NOT_ARMED(palw_tir_v1)".to_string()), "{:?}", codes(&r));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn below_the_fence_the_registration_waits_for_it_and_the_rest_is_judged_as_if_it_were_armed() {
    let dir = copy_fixture(&fixture("hf/llama"), "chain-fence", true);
    let r = run(&dir, &Options { height: Some(100), ..opts(Depth::Shape) }).expect("preflight");
    let c = codes(&r);
    assert!(c.contains(&"FENCE_NOT_ARMED(palw_tir_v1)".to_string()), "{c:?}\n{}", r.render());
    let net = r.network.as_ref().expect("network");
    assert!(!net.tir_armed && net.what_if.is_some());
    // The remaining conditions were judged, at the fence's own height.
    assert_eq!(net.daa, 2_000);
    assert!(r.admission.as_ref().is_some_and(|a| a.gate == "admitted"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_seat_with_too_little_memory_is_told_the_context_that_fits_it() {
    let dir = copy_fixture(&fixture("hf/llama"), "seat", true);
    // 1 GiB is more than this tiny model needs; a share of zero GiB is not.
    let r = run(&dir, &Options { seat_memory_gib: Some(0), ..opts(Depth::Shape) }).expect("preflight");
    let b = r.blockers().into_iter().find(|b| b.code == "SEAT_MEMORY_SHORT").expect("blocker");
    assert_eq!(b.stage, misaka_palw_sdk::preflight::Stage::Mine);
    assert_eq!(r.verdict.mine.status, StageStatus::Blocked);
    assert_eq!(r.verdict.register.status, StageStatus::Ok, "the chain admits it; the seat cannot replay it");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_full_depth_says_what_it_needs() {
    let dir = copy_fixture(&fixture("hf/llama"), "full", false);
    let r = run(&dir, &opts(Depth::Full)).expect("preflight");
    assert_eq!(r.depth.reached, Depth::Shape);
    assert!(r.depth.stopped_at.as_deref().is_some_and(|s| s.contains("pack verify")), "{:?}", r.depth.stopped_at);
    let _ = std::fs::remove_dir_all(dir);
}
