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
    let dir = copy_fixture(&fixture("hf/qwen4_exp"), "qwen4-exp", true);
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
    assert!(r.seat.as_ref().is_some_and(|s| s.tiers.iter().all(|t| t.fits)) && r.forecast.as_ref().is_some_and(|f| f.required_ready_seats == 7));
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
    let r =
        run(&dir, &Options { network: Some("devnet".into()), max_context: Some(65_536), ..opts(Depth::Shape) }).expect("preflight");
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
fn the_seat_is_judged_against_each_tier_and_a_tier_that_cannot_hold_the_class_is_named() {
    use misaka_palw_sdk::preflight::chain::SeatShare;
    let dir = copy_fixture(&fixture("hf/llama"), "seat", true);
    // The fleet's tiers hold this tiny class.
    let r = run(&dir, &opts(Depth::Shape)).expect("preflight");
    let seat = r.seat.as_ref().expect("seat");
    assert_eq!(seat.tiers.len(), 2);
    assert!(seat.tiers.iter().all(|t| t.fits), "{seat:?}");
    // One tier too small: the class still registers a seat on the other, and the report says which cannot.
    let tiers = vec![SeatShare { name: "tiny".into(), bytes: 1 << 20 }, SeatShare { name: "big".into(), bytes: 8 << 30 }];
    let r = run(&dir, &Options { seat_shares: tiers, ..opts(Depth::Shape) }).expect("preflight");
    assert_eq!(r.verdict.mine.status, StageStatus::Ok);
    assert!(r.notes.iter().any(|n| n.contains("only big can hold the class")), "{:?}", r.notes);
    // No tier holds it: a blocker with the numbers.
    let r = run(&dir, &Options { seat_shares: vec![SeatShare { name: "none".into(), bytes: 0 }], ..opts(Depth::Shape) })
        .expect("preflight");
    let b = r.blockers().into_iter().find(|b| b.code == "SEAT_MEMORY_SHORT").expect("blocker");
    assert_eq!(b.stage, misaka_palw_sdk::preflight::Stage::Mine);
    assert_eq!(r.verdict.register.status, StageStatus::Ok, "the chain admits it; the seat cannot replay it");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_full_depth_without_a_pack_names_what_it_needs_as_a_blocker() {
    let dir = copy_fixture(&fixture("hf/llama"), "full", false);
    let r = run(&dir, &opts(Depth::Full)).expect("preflight");
    assert_eq!(r.depth.reached, Depth::Full);
    let b = r.blockers().into_iter().find(|b| b.code == "PACK_NOT_VERIFIED").expect("PACK_NOT_VERIFIED");
    assert_eq!(b.stage, misaka_palw_sdk::preflight::Stage::Mine);
    assert!(b.safe_paths.iter().any(|p| p.contains("pack build")), "{:?}", b.safe_paths);
    assert_eq!(r.full.as_ref().map(|f| f.pack.as_str()), Some("NO_PACK"));
    let _ = std::fs::remove_dir_all(dir);
}

/// **The residency, read off the program, from headers alone** (ADR-0112 for IR classes,
/// `docs/design/palw/tir/runtime-residency.md`): a mixture's expert stacks are routed and its embedding gathered;
/// the floor is the pinned set, one token's routed rows and one admission in flight; one replay at the canonical job
/// reads the expected union of the routed rows and every gathered row, with read times at the reference rates; a
/// dense decoder routes nothing. Reported, never judged: the verdicts are the same whatever the tier rule.
#[test]
fn the_residency_is_read_off_the_program_and_reported_without_judging() {
    let dir = copy_fixture(&fixture("hf/qwen3_moe"), "moe-residency", true);
    // Every row-addressed param served by rows (a node's rule pins a fixture this small whole).
    let rows = Options { residency_pin_below_bytes: 0, ..opts(Depth::Shape) };
    let r = run(&dir, &rows).expect("preflight");
    let res = r.residency.as_ref().expect("a residency");
    assert!(res.rows.iter().filter(|x| x.tier == "routed").count() >= 3, "the expert stacks: {}", r.render());
    assert!(res.rows.iter().any(|x| x.tier == "gathered"), "the embedding: {}", r.render());
    assert_eq!(res.floor_bytes, res.pinned_bytes + res.routed_token_bytes + res.in_flight_bytes);
    assert_eq!(res.weight_bytes, res.pinned_bytes + res.routed_bytes + res.gathered_bytes);
    let (prefill, decode) = res.replay.job.expect("the shape depth chose a context and its canonical job");
    assert_eq!(res.replay.forwards, u64::from(prefill + decode - 1));
    assert!(res.replay.routed_union_bytes + 1 >= res.routed_token_bytes && res.replay.routed_union_bytes <= res.routed_bytes);
    assert_eq!(res.replay.bytes, res.replay.routed_union_bytes + res.replay.gathered_bytes);
    let at = |mb: u64| res.replay.seconds_at.iter().find(|(m, _)| *m == mb).map(|(_, s)| *s).expect("a reference rate");
    assert!((at(500) - res.replay.bytes as f64 / 5e8).abs() < 1e-9 && at(845) < at(500));
    let text = r.render();
    assert!(text.contains("residency") && text.contains("one replay") && text.contains("estimates"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert!(json["residency"]["floor_bytes"].as_u64().is_some());
    // The node's own rule: the same verdicts; a fixture this small is pinned whole.
    let node = run(&dir, &opts(Depth::Shape)).expect("preflight");
    assert_eq!(node.verdict, r.verdict, "the residency judges nothing");
    let held = node.residency.as_ref().expect("a residency");
    assert!(held.rows.is_empty() && held.floor_bytes == held.weight_bytes, "{}", node.render());
    // A dense decoder routes no stack — what an activation selects rows of is a table of single codes
    // (65,536 of them), which the node's rule pins — and its embedding is gathered.
    let dense = copy_fixture(&fixture("hf/llama"), "llama-residency", true);
    let d = run(&dense, &Options { residency_pin_below_bytes: 0, ..opts(Depth::Shape) }).expect("preflight");
    let dres = d.residency.as_ref().expect("a residency");
    assert!(dres.rows.iter().filter(|x| x.tier == "routed").all(|x| x.row_bytes <= 2 && x.rows == 65_536), "{}", d.render());
    assert!(dres.rows.iter().any(|x| x.tier == "gathered"));
    let node_dense = run(&dense, &opts(Depth::Shape)).expect("preflight");
    let nres = node_dense.residency.as_ref().expect("a residency");
    assert_eq!(nres.routed_bytes, 0, "the node's rule pins the code tables");
    // A dense decoder's floor is its size: a fifth is SHORT, and the report names the exact budget to state.
    assert!(!nres.default_holds_floor, "{}", node_dense.render());
    let state = format!("--palw-class-resident-bytes {}", nres.floor_bytes);
    assert_eq!(nres.state_to_hold.as_deref(), Some(state.as_str()));
    assert!(node_dense.render().contains(&state), "{}", node_dense.render());
    assert!(held.state_to_hold.is_some() == !held.default_holds_floor);
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(dense);
}

/// **A tokenizer is a file the class commits to the bytes of, whatever algorithm it describes.** `tokenizer.json` was the only name
/// (with `tokenizer.model` and `vocab.json`) the preflight knew, so a T5/ALBERT/XLNet checkpoint (`spiece.model`), an XLM-R
/// (`sentencepiece.bpe.model`), a BERT (`vocab.txt`) or a tiktoken model (`*.tiktoken`) was `TOKENIZER_MISSING` beside its own tokenizer.
/// A directory with none is still missing one, and a file that only looks like one (`merges.txt` alone) is not one.
#[test]
fn a_sentencepiece_wordpiece_or_tiktoken_file_is_a_tokenizer_and_nothing_else_is() {
    let dir = copy_fixture(&fixture("hf/llama"), "tokenizer-names", false);
    let missing = |d: &Path| codes(&run(d, &opts(Depth::Headers)).expect("preflight")).iter().any(|c| c == "TOKENIZER_MISSING");
    std::fs::remove_file(dir.join("tokenizer.json")).expect("remove");
    assert!(missing(&dir), "no tokenizer file at all");
    std::fs::write(dir.join("merges.txt"), "a b\n").expect("merges");
    std::fs::write(dir.join("special_tokens_map.json"), "{}").expect("map");
    assert!(missing(&dir), "merges and a special-token map are not a tokenizer");
    for name in ["spiece.model", "sentencepiece.bpe.model", "sentencepiece.model", "tokenizer.model", "vocab.txt", "tekken.json", "cl100k_base.tiktoken", "vocab.json", "tokenizer.json"] {
        std::fs::write(dir.join(name), b"x").expect("tokenizer");
        assert!(!missing(&dir), "{name} is a tokenizer");
        std::fs::remove_file(dir.join(name)).expect("remove");
        assert!(missing(&dir), "back to none");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// **A GGUF LoRA is an adapter, not a model whose hidden size is missing.** llama.cpp's `convert_lora_to_gguf` writes
/// `general.type = adapter` and low-rank pairs; the mapping used to call it "the GGUF's architecture has no mapping" with
/// `GGUF: no llama.embedding_length` — an estimated 6,080 repositories of the census filed under `ARCH_REFUSED`.
#[test]
fn a_gguf_lora_is_named_as_an_adapter_and_not_as_an_architecture_the_frontend_failed_to_map() {
    let dir = scratch("gguf-lora");
    let bytes = gguf_bytes(
        &[("general.architecture", Ok("llama")), ("general.type", Ok("adapter")), ("adapter.type", Ok("lora"))],
        &[("blk.0.attn_q.weight.lora_a", vec![64, 8], 0, 2048), ("blk.0.attn_q.weight.lora_b", vec![8, 64], 0, 2048)],
        false,
    );
    std::fs::write(dir.join("lora.gguf"), &bytes).expect("gguf");
    let r = run(&dir.join("lora.gguf"), &opts(Depth::Headers)).expect("preflight");
    let c = codes(&r);
    assert!(c.iter().any(|x| x == "ADAPTER_REFUSED") && !c.iter().any(|x| x == "ARCH_REFUSED"), "{c:?}\n{}", r.render());
    let b = r.verdict.convert.blockers.iter().find(|b| b.code == "ADAPTER_REFUSED").expect("blocker");
    assert!(b.evidence.iter().any(|e| e.contains("LoRA adapter")), "{:?}", b.evidence);
    let _ = std::fs::remove_dir_all(dir);
}

/// **A PyTorch checkpoint is judged like its safetensors original** (`weights::torchzip`: the pickle is interpreted, never run). The
/// fixture is `hf/llama` re-saved with `torch.save` as `pytorch_model.bin`, in one file and in two shards with an index: the same
/// tensors are bound, the same spec digest read, and the verdict is the same.
#[test]
fn a_pytorch_model_bin_is_judged_like_the_safetensors_it_was_saved_from() {
    let st = copy_fixture(&fixture("hf/llama"), "bin-ref", false);
    let reference = run(&st, &opts(Depth::Headers)).expect("preflight");
    assert_eq!(reference.verdict.convert.status, StageStatus::Ok, "{}", reference.render());
    for (name, shards) in [("hf-bin/llama", 1usize), ("hf-bin/llama-sharded", 2)] {
        let dir = copy_fixture(&fixture(name), &format!("bin-{shards}"), false);
        let r = run(&dir, &opts(Depth::Headers)).expect("preflight");
        assert_eq!(r.verdict.convert.status, StageStatus::Ok, "{name}: {}", r.render());
        assert_eq!(r.tensors.checked, "shapes", "{name}");
        assert_eq!((r.tensors.bound, r.tensors.missing_total, r.tensors.shape_mismatch_total), (reference.tensors.bound, 0, 0), "{name}");
        assert_eq!(r.model.as_ref().map(|m| m.spec_digest.clone()), reference.model.as_ref().map(|m| m.spec_digest.clone()), "{name}");
        assert_eq!(r.source.shards.len(), shards, "{name}");
        // The same weights, in bytes, and none of the tensor data read.
        assert_eq!(r.source.weight_bytes, reference.source.weight_bytes, "{name}");
        assert!(r.input.bytes_read < reference.source.weight_bytes.unwrap_or(0) + 70_000, "{name}: read {}", r.input.bytes_read);
        let _ = std::fs::remove_dir_all(dir);
    }
    let _ = std::fs::remove_dir_all(st);
}

/// A `.bin` the reader refuses by its form is a named `FORMAT_UNSUPPORTED` blocker with its reason and a safe path — not a crash, and
/// not "tensors missing": the legacy serialization, a strided view and a training checkpoint, each beside a real `config.json`.
#[test]
fn a_pytorch_file_the_reader_refuses_is_format_unsupported_with_its_reason() {
    for (file, why) in [("legacy.bin", "legacy"), ("strided.bin", "strided view"), ("nested.bin", "training checkpoint")] {
        let dir = scratch(&format!("refused-{file}"));
        std::fs::copy(fixture("hf/llama").join("config.json"), dir.join("config.json")).expect("config");
        std::fs::write(dir.join("tokenizer.json"), "{}").expect("tokenizer");
        std::fs::copy(fixture("torch").join(file), dir.join("pytorch_model.bin")).expect("bin");
        let r = run(&dir, &opts(Depth::Headers)).expect("preflight");
        let b = r.verdict.convert.blockers.iter().find(|b| b.code == "FORMAT_UNSUPPORTED").unwrap_or_else(|| panic!("{file}: {}", r.render()));
        assert!(b.evidence.iter().any(|e| e.contains(why)), "{file}: {:?}", b.evidence);
        assert!(b.safe_paths.iter().any(|p| p.contains("safetensors")), "{file}: {:?}", b.safe_paths);
        assert_eq!(r.verdict.convert.status, StageStatus::Blocked);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// **An ignored key is withdrawn when the checkpoint says the model is not that class.** `depth_alpha_enabled` is a key transformers'
/// Llama never reads: a Llama config carrying it reads (`tir-lower/tests/hf_keys_ignored.rs`), and the preflight says which keys it
/// ignored. But a `use_qk_norm` beside q/k-norm tensors that no Llama has is an author's different model: refused as it always was.
#[test]
fn a_key_ignored_for_a_transformers_class_is_refused_again_when_the_checkpoint_carries_tensors_the_class_does_not_have() {
    // Junk keys, the plain Llama tensors: reads, and the ignored keys are in the report.
    let dir = with_config(&fixture("hf/llama"), "ignored-ok", &|c| {
        c["depth_alpha_enabled"] = serde_json::json!(true);
        c["organization"] = serde_json::json!("x");
    });
    std::fs::write(dir.join("tokenizer.json"), "{}").expect("tokenizer");
    let r = run(&dir, &opts(Depth::Headers)).expect("preflight");
    assert_eq!(r.verdict.convert.status, StageStatus::Ok, "{}", r.render());
    let said = r.model.as_ref().expect("model").assumed_defaults.join("|");
    assert!(said.contains("ignored `depth_alpha_enabled`") && said.contains("ignored `organization`"), "{said}");
    let _ = std::fs::remove_dir_all(dir);
    // The same keys and a q_norm tensor no Llama has: refused.
    let dir = edited_header_copy(&fixture("hf/llama"), "ignored-withdrawn", &|h| {
        h.insert("model.layers.0.self_attn.q_norm.weight".into(), serde_json::json!({"dtype": "BF16", "shape": [8], "data_offsets": [0, 16]}));
    });
    let text = std::fs::read_to_string(dir.join("config.json")).expect("config");
    let mut c: serde_json::Value = serde_json::from_str(&text).expect("json");
    c["use_qk_norm"] = serde_json::json!(true);
    std::fs::write(dir.join("config.json"), c.to_string()).expect("config");
    let r = run(&dir, &opts(Depth::Headers)).expect("preflight");
    let c = codes(&r);
    assert!(c.iter().any(|x| x == "CONFIG_KEY_UNREAD(use_qk_norm)"), "{c:?}\n{}", r.render());
    let _ = std::fs::remove_dir_all(dir);
}
