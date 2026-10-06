//! **Runtime packs end to end** (RFC-0002 Part II, requirement 2): a pack is built from a tiny Hugging
//! Face fixture and verified; the artifact is rebuilt from the public source and the pack's profile and
//! its roots are the pack's, on one thread and on seven; every claim a pack makes fails when it is
//! tampered with; the logit convention is a fact, not a label.

use misaka_palw_sdk::runtime_pack::manifest::{PACK_FILE, RuntimePackV1};
use misaka_palw_sdk::runtime_pack::{BuildOpts, Status, VerifyOpts, build_pack, verify_pack};
use misaka_palw_tir_lower::convert::{CalibInput, ConvertRequest};
use std::path::{Path, PathBuf};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures").join(rel)
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("runtime-pack-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch");
    d
}

/// Calibration sequences: `n` of `len` tokens below `vocab`, seeded.
fn calibration(dir: &Path, vocab: usize, n: usize, len: usize) -> CalibInput {
    let seqs = misaka_palw_tir_lower::fidelity::random_sequences(vocab, n, len, 11);
    let c = CalibInput { sequences: seqs, source: serde_json::json!("random (seed 11)") };
    let _ = std::fs::write(dir.join("calib.json"), serde_json::json!({ "source": c.source, "sequences": c.sequences }).to_string());
    c
}

fn build(src: &Path, work: &Path, vocab: usize, with_reference: bool) -> (RuntimePackV1, String, PathBuf) {
    let pack_dir = work.join("pack");
    let mut req = ConvertRequest::new(src, work.join("artifact.palwtir"));
    req.calib = Some(calibration(work, vocab, 3, 12));
    let mut o = BuildOpts::new(req, &pack_dir, "fixture");
    o.prompts = 2;
    o.prefill = 5;
    o.decode = 2;
    if with_reference {
        o.hf_reference = Some(src.join("logits.json"));
    }
    let b = build_pack(&o, &|_| {}).unwrap_or_else(|e| panic!("build: {e}"));
    (b.pack, b.digest, pack_dir)
}

fn statuses(r: &misaka_palw_sdk::runtime_pack::VerifyReport) -> String {
    r.checks.iter().map(|c| format!("{}:{:?}", c.name, c.status)).collect::<Vec<_>>().join(" ")
}

#[test]
fn a_pack_builds_verifies_and_rebuilds_the_same_artifact_on_any_thread_count() {
    let work = scratch("rebuild");
    let src = fixture("hf/llama");
    let (pack, digest, pack_dir) = build(&src, &work, 64, true);
    assert_eq!(pack.converter.math.mode, "libm-v1");
    assert_eq!(pack.logits.convention, "legacy-greedy-only");
    assert_eq!(pack.frontend.level, "B", "llama has a built-in adapter file");
    assert!(
        pack.frontend.adapter.kind == "built-in"
            && pack.frontend.adapter.id.as_deref() == Some("llama")
            && pack.frontend.template.is_none()
    );
    assert_eq!(pack.conformance.vectors.len(), 2);
    assert!(pack.hf_reference.as_ref().is_some_and(|h| h.positions == 10 && h.measured.top1 >= 0.8));
    let h = pack.hf_reference.as_ref().expect("reference");
    eprintln!(
        "fit: slope {:.4} corr {:.5} top1 {:.3} KL {:.5}",
        h.measured.slope, h.measured.corr, h.measured.top1, h.measured.kl_mean
    );
    // The manifest on disk parses back to the same pack and the same digest; a pretty file is not another identity.
    let text = std::fs::read_to_string(pack_dir.join(PACK_FILE)).expect("manifest");
    assert_eq!(RuntimePackV1::parse(&text).expect("parses").digest(), digest);

    // Verified from the artifact it describes, and from a rebuild of the public source.
    let mut o = VerifyOpts::new(&pack_dir);
    o.artifact = Some(work.join("artifact.palwtir"));
    o.model = Some(src.clone());
    let r = verify_pack(&o, &|_| {}).expect("verifies");
    assert!(r.verified(), "{}", r.render());
    for threads in [1usize, 7] {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().expect("pool");
        let mut o = VerifyOpts::new(&pack_dir);
        o.model = Some(src.clone());
        o.rebuild = true;
        let r = pool.install(|| verify_pack(&o, &|_| {})).expect("verifies");
        assert!(r.verified(), "{threads} thread(s): {}", r.render());
        let art = r.checks.iter().find(|c| c.name == "artifact").expect("an artifact check");
        assert_eq!(art.status, Status::Pass);
    }

    // Built again from the same inputs the pack is the same pack.
    let work2 = scratch("rebuild2");
    let (_, digest2, _) = build(&src, &work2, 64, true);
    assert_eq!(digest, digest2, "a pack is a function of its inputs");
    let _ = std::fs::remove_dir_all(work);
    let _ = std::fs::remove_dir_all(work2);
}

#[test]
fn every_claim_of_a_pack_fails_when_it_is_tampered_with() {
    let work = scratch("tamper");
    let src = fixture("hf/llama");
    let (pack, _, pack_dir) = build(&src, &work, 64, true);
    let artifact = work.join("artifact.palwtir");
    let verify_with = |dir: &Path, model: Option<&Path>| {
        let mut o = VerifyOpts::new(dir);
        o.artifact = Some(artifact.clone());
        o.model = model.map(Path::to_path_buf);
        verify_pack(&o, &|_| {}).expect("runs")
    };
    let failing = |r: &misaka_palw_sdk::runtime_pack::VerifyReport| -> Vec<String> {
        r.checks.iter().filter(|c| c.status == Status::Fail).map(|c| c.name.clone()).collect()
    };
    let copy_pack = |tag: &str| -> PathBuf {
        let d = work.join(format!("pack-{tag}"));
        std::fs::create_dir_all(&d).expect("dir");
        for e in std::fs::read_dir(&pack_dir).expect("pack").flatten() {
            std::fs::copy(e.path(), d.join(e.file_name())).expect("copy");
        }
        d
    };
    let edit = |tag: &str, f: &dyn Fn(&mut serde_json::Value)| -> PathBuf {
        let d = copy_pack(tag);
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(d.join(PACK_FILE)).expect("manifest")).expect("json");
        f(&mut v);
        std::fs::write(d.join(PACK_FILE), serde_json::to_string_pretty(&v).expect("json")).expect("write");
        d
    };
    // The honest pack passes everything that can be checked from the artifact.
    let r = verify_with(&pack_dir, Some(&src));
    assert!(r.verified(), "{}", r.render());

    // The artifact's identity.
    let d = edit("root", &|v| {
        let s = v["result"]["inventory_root"].as_str().expect("root").to_string();
        // A different last digit (the root itself moves with the lowering: it may end in 0).
        let last = if s.ends_with('0') { '1' } else { '0' };
        v["result"]["inventory_root"] = format!("{}{last}", &s[..s.len() - 1]).into();
    });
    assert!(failing(&verify_with(&d, None)).contains(&"artifact".to_string()));
    // A conformance vector: the decoded tokens, then the logits digest.
    let d = edit("tokens", &|v| {
        let t = v["conformance"]["vectors"][0]["tokens"][0].as_u64().expect("token");
        v["conformance"]["vectors"][0]["tokens"][0] = ((t + 1) % 64).into();
    });
    assert!(failing(&verify_with(&d, None)).contains(&"conformance".to_string()));
    let d = edit("digest", &|v| {
        let s = v["conformance"]["vectors"][1]["logits_digest"].as_str().expect("digest").to_string();
        v["conformance"]["vectors"][1]["logits_digest"] = format!("0{}", &s[1..]).into();
    });
    assert!(failing(&verify_with(&d, None)).contains(&"conformance".to_string()));
    // The pinned inputs: the statistics are a sidecar, hashed.
    let d = copy_pack("stats");
    let mut stats = std::fs::read(d.join("stats.json")).expect("stats");
    let n = stats.len() / 2;
    stats[n] = if stats[n] == b'7' { b'8' } else { b'7' };
    std::fs::write(d.join("stats.json"), stats).expect("write");
    assert!(failing(&verify_with(&d, None)).contains(&"sidecars".to_string()));
    // The adapter: another hash is another reading of the configuration.
    let d = edit("adapter", &|v| v["frontend"]["adapter"]["hash"] = "0".repeat(128).into());
    assert!(failing(&verify_with(&d, None)).contains(&"adapter".to_string()));
    // The source: a weight file with one byte changed.
    let tampered = work.join("src");
    std::fs::create_dir_all(&tampered).expect("dir");
    for e in std::fs::read_dir(&src).expect("fixture").flatten() {
        std::fs::copy(e.path(), tampered.join(e.file_name())).expect("copy");
    }
    let mut w = std::fs::read(tampered.join("model.safetensors")).expect("weights");
    let n = w.len() - 3;
    w[n] ^= 1;
    std::fs::write(tampered.join("model.safetensors"), w).expect("write");
    assert!(failing(&verify_with(&pack_dir, Some(&tampered))).contains(&"source".to_string()));
    // A descriptor pin that is not this build's.
    let d = edit("quant", &|v| {
        v["quant"]["descriptors"] = serde_json::json!([{ "name": "GPTQ", "digest": "0".repeat(64), "source": "built-in" }]);
    });
    assert!(failing(&verify_with(&d, None)).contains(&"descriptors".to_string()));

    // The logit convention is a fact: say the codes are 1000 times larger and the reference refuses.
    let d = edit("scale", &|v| {
        let bits = u64::from_str_radix(v["logits"]["scale_bits"].as_str().expect("scale bits"), 16).expect("hex");
        let s = f64::from_bits(bits) * 1000.0;
        v["logits"]["scale_bits"] = format!("{:016x}", s.to_bits()).into();
    });
    let r = verify_with(&d, None);
    let hf = r.checks.iter().find(|c| c.name == "hf-reference").expect("a reference check");
    assert_eq!(hf.status, Status::Fail, "{}", r.render());
    assert!(hf.detail.contains("slope"), "{}", hf.detail);
    // …and a pack that claims q24-natural units must say a scale of exactly 2^-24.
    let d = edit("q24", &|v| {
        v["logits"]["convention"] = "q24-natural-v1".into();
        // The lowerer writes q24 natural-log units by default now (LOGITS_Q24_V1: scale exactly 2^-24): the claim is made false by the scale.
        v["logits"]["scale_bits"] = format!("{:016x}", 0.5f64.to_bits()).into();
    });
    let text = std::fs::read_to_string(d.join(PACK_FILE)).expect("manifest");
    assert!(RuntimePackV1::parse(&text).unwrap_err().contains("2^-24"));
    let _ = pack;
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn a_program_outside_its_reference_tolerance_is_not_packed() {
    let work = scratch("tolerance");
    let src = fixture("hf/llama");
    let mut req = ConvertRequest::new(&src, work.join("artifact.palwtir"));
    req.calib = Some(calibration(&work, 64, 3, 12));
    let mut o = BuildOpts::new(req, work.join("pack"), "fixture");
    o.prompts = 1;
    o.hf_reference = Some(src.join("logits.json"));
    o.tolerance.slope_min = 1.5;
    let e = build_pack(&o, &|_| {}).err().expect("refused");
    assert!(e.contains("outside the tolerance") && e.contains("slope"), "{e}");
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn a_vision_language_pack_says_what_it_does_not_compute_and_a_quantised_one_pins_its_descriptor() {
    let work = scratch("scope");
    let (pack, _, pack_dir) = build(&fixture("hf/llava"), &work, 64, true);
    let left: Vec<&str> =
        pack.features.scope["excluded"].as_array().expect("excluded").iter().filter_map(|e| e["kind"].as_str()).collect();
    assert_eq!(left, vec!["vision", "projector"], "{}", pack.features.scope);
    assert_eq!(pack.features.scope["text_only"], true);
    assert_eq!(pack.frontend.level, "B");
    assert!(pack.frontend.adapter.kind == "built-in" && pack.frontend.adapter.hash.as_ref().is_some_and(|h| h.len() == 128));
    let shown = misaka_palw_sdk::runtime_pack::cli::show(&pack);
    assert!(shown.contains("NOT computed: vision tower, multimodal projector"), "{shown}");
    let mut o = VerifyOpts::new(&pack_dir);
    o.artifact = Some(work.join("artifact.palwtir"));
    o.model = Some(fixture("hf/llava"));
    let r = verify_pack(&o, &|_| {}).expect("verifies");
    assert!(r.verified(), "{}", r.render());
    let _ = std::fs::remove_dir_all(work);

    let work = scratch("quant");
    let (pack, _, pack_dir) = build(&fixture("hf-quant/ct_pack_b4_g32"), &work, 128, true);
    assert_eq!(pack.quant.descriptors.len(), 1);
    assert_eq!(
        (pack.quant.descriptors[0].name.as_str(), pack.quant.descriptors[0].source.as_str()),
        ("CT_PACK_QUANTIZED", "built-in")
    );
    assert!(pack.features.used.iter().any(|u| u["id"] == "QUANT_DESCRIBED_INTEGERS_V1"), "{:?}", pack.features.used);
    let mut o = VerifyOpts::new(&pack_dir);
    o.model = Some(fixture("hf-quant/ct_pack_b4_g32"));
    o.rebuild = true;
    let r = verify_pack(&o, &|_| {}).expect("verifies");
    assert!(r.verified(), "{}", statuses(&r));
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn a_gguf_pack_pins_the_block_formats_it_is_read_with() {
    let work = scratch("gguf");
    let src = fixture("gguf/gguf_llama_q4_k_m");
    let (pack, _, pack_dir) = build(&src, &work, 256, true);
    assert_eq!(pack.model.format, "gguf");
    assert_eq!(pack.model.files.len(), 1);
    assert_eq!(pack.model.files[0].path, "model.gguf");
    let names: Vec<&str> = pack.quant.descriptors.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"Q4_K") && pack.quant.descriptors.iter().all(|d| d.source == "built-in"), "{names:?}");
    let mut o = VerifyOpts::new(&pack_dir);
    o.model = Some(src.clone());
    o.rebuild = true;
    let r = verify_pack(&o, &|_| {}).expect("verifies");
    assert!(r.verified(), "{}", r.render());
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn a_declared_class_is_reproduced_from_the_pack_and_its_class_id_cannot_be_changed() {
    use misaka_palw_sdk::runtime_pack::DeclareOpts;
    use misaka_palw_sdk::tir_layout::TirLayoutChoiceV1;
    let work = scratch("declared");
    let src = fixture("hf/llama");
    let pack_dir = work.join("pack");
    let mut req = ConvertRequest::new(&src, work.join("artifact.palwtir"));
    req.calib = Some(calibration(&work, 64, 3, 12));
    let mut o = BuildOpts::new(req, &pack_dir, "fixture");
    o.prompts = 1;
    o.prefill = 4;
    o.decode = 1;
    o.declare = vec![DeclareOpts {
        network: "testnet-12".into(),
        choice: TirLayoutChoiceV1 { max_context: Some(256), ..TirLayoutChoiceV1::default() },
    }];
    let b = build_pack(&o, &|_| {}).unwrap_or_else(|e| panic!("build: {e}"));
    assert_eq!(b.pack.declared.len(), 1);
    assert_eq!((b.pack.declared[0].network.as_str(), b.pack.declared[0].max_context), ("testnet-12", 256));
    let mut v = VerifyOpts::new(&pack_dir);
    v.artifact = Some(work.join("artifact.palwtir"));
    let r = verify_pack(&v, &|_| {}).expect("verifies");
    assert!(r.ok(), "{}", r.render());
    let d = r.checks.iter().find(|c| c.name == "declared").expect("a declared check");
    assert_eq!(d.status, Status::Pass, "{}", d.detail);
    // The class a registration carries is the artifact with its layout declared: a pack verifies against that file too (its file digest
    // and graph root are the declared class's, which the pack pins), so `tir-registration --pack` can be given the class file.
    let mut vd = VerifyOpts::new(&pack_dir);
    vd.artifact = Some(work.join("artifact.palwtir.testnet-12.palwtir"));
    let r = verify_pack(&vd, &|_| {}).expect("verifies the declared form");
    let a = r.checks.iter().find(|c| c.name == "artifact").expect("an artifact check");
    assert_eq!(a.status, Status::Pass, "{}", r.render());
    assert!(a.detail.contains("declared for testnet-12"), "{}", a.detail);
    // Another class id is not this artifact's class.
    let mut m: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pack_dir.join(PACK_FILE)).expect("manifest")).expect("json");
    m["declared"][0]["class_id"] = "0".repeat(128).into();
    std::fs::write(pack_dir.join(PACK_FILE), serde_json::to_string_pretty(&m).expect("json")).expect("write");
    let r = verify_pack(&v, &|_| {}).expect("verifies");
    let d = r.checks.iter().find(|c| c.name == "declared").expect("a declared check");
    assert_eq!(d.status, Status::Fail, "{}", r.render());
    let _ = std::fs::remove_dir_all(work);
}

#[test]
fn an_mxfp4_pack_serves_the_packed_experts_as_the_float_export_and_rebuilds() {
    let work = scratch("mxfp4");
    let src = fixture("hf-quant/mxfp4_gptoss");
    let (pack, _, pack_dir) = build(&src, &work, 128, true);
    assert!(pack.quant.descriptors.iter().any(|d| d.name == "MXFP4_HF" && d.source == "built-in"), "{:?}", pack.quant.descriptors);
    assert!(pack.features.used.iter().any(|u| u["id"] == "QUANT_DESCRIBED_FLOATS_V1"), "{:?}", pack.features.used);
    assert_eq!(pack.model.architectures, vec!["GptOssForCausalLM".to_string()]);
    let mut o = VerifyOpts::new(&pack_dir);
    o.model = Some(src);
    o.rebuild = true;
    let r = verify_pack(&o, &|_| {}).expect("verifies");
    assert!(r.verified(), "{}", r.render());
    let _ = std::fs::remove_dir_all(work);
}

/// **L2: a streamed conformance gives the vectors a loaded one gives** (RFC-0002 Part II §II.9): built streamed or loaded, the pack's
/// conformance vectors, their digests and the fit to the Hugging Face reference are the same bytes, and either verifies either's pack.
/// Both references decode parameters on demand; independent conformance has no whole-artifact size cutoff.
#[test]
fn a_streamed_conformance_is_the_loaded_ones_and_never_holds_the_artifact_whole() {
    use misaka_palw_sdk::runtime_pack::conformance;
    let work_loaded = scratch("stream-loaded");
    let work_streamed = scratch("stream-streamed");
    let src = fixture("hf/llama");
    let build_with = |work: &Path, streamed: bool| {
        let pack_dir = work.join("pack");
        let mut req = ConvertRequest::new(&src, work.join("artifact.palwtir"));
        req.calib = Some(calibration(work, 64, 3, 12));
        let mut o = BuildOpts::new(req, &pack_dir, "fixture");
        o.prompts = 2;
        o.prefill = 5;
        o.decode = 2;
        o.hf_reference = Some(src.join("logits.json"));
        o.streamed = Some(streamed);
        let b = build_pack(&o, &|_| {}).unwrap_or_else(|e| panic!("build (streamed {streamed}): {e}"));
        (b.pack, pack_dir)
    };
    let (loaded, loaded_dir) = build_with(&work_loaded, false);
    let (streamed, streamed_dir) = build_with(&work_streamed, true);
    assert_eq!(loaded.conformance, streamed.conformance, "the vectors and their digests are the same bytes");
    assert_eq!(loaded.result, streamed.result, "and so is the artifact");
    assert_eq!(
        loaded.hf_reference.as_ref().map(|h| h.measured.clone()),
        streamed.hf_reference.as_ref().map(|h| h.measured.clone()),
        "the fit too"
    );
    // Either verifies either's pack, both ways.
    for (dir, art) in [(&loaded_dir, work_loaded.join("artifact.palwtir")), (&streamed_dir, work_streamed.join("artifact.palwtir"))] {
        for streamed_verify in [false, true] {
            let mut v = VerifyOpts::new(dir);
            v.artifact = Some(art.clone());
            v.model = Some(src.clone());
            v.streamed = Some(streamed_verify);
            let r = verify_pack(&v, &|_| {}).expect("verifies");
            assert!(r.ok(), "streamed {streamed_verify}: {}", r.render());
            let c = r.checks.iter().find(|c| c.name == "conformance").expect("a conformance check");
            assert_eq!(c.status, Status::Pass, "{}", c.detail);
            assert_eq!(c.detail.contains("streamed"), streamed_verify, "{}", c.detail);
        }
    }
    // The streamed run's own report: the reference never held more than its largest tensor, and the independent implementation ran.
    let jobs = conformance::jobs(64, 1, 4, 1, 3);
    let positions = std::cell::RefCell::new(Vec::new());
    let (_, note) = conformance::run_streamed_with_progress(
        &work_streamed.join("artifact.palwtir"),
        &jobs,
        conformance::ImplSet::default(),
        &|_| {},
        &|job, position| positions.borrow_mut().push((job, position)),
    )
    .expect("streams");
    let expected: Vec<_> =
        jobs.iter().enumerate().flat_map(|(ji, job)| (0..job.prompt.len() + job.decode).map(move |p| (ji, p))).collect();
    assert_eq!(*positions.borrow(), expected);
    assert!(note.ran.contains(&"independent".to_string()) && note.ref2_skipped.is_none(), "{note:?}");
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open(&work_streamed.join("artifact.palwtir")).expect("opens");
    let largest = container
        .header
        .tensors
        .iter()
        .map(|e| container.program.params[e.param as usize].shape.iter().product::<u32>() as u64 * 16)
        .max()
        .unwrap_or(0);
    assert!(note.reference_peak_tensor_bytes <= largest, "{} > {largest}", note.reference_peak_tensor_bytes);
    assert!(note.independent_peak_tensor_bytes > 0 && note.independent_peak_tensor_bytes <= largest, "{note:?}");
    let _ = std::fs::remove_dir_all(work_loaded);
    let _ = std::fs::remove_dir_all(work_streamed);
}

#[test]
fn binding_preserves_nondefault_layout_without_search_and_refuses_mutations() {
    use misaka_palw_sdk::runtime_pack::bind::bind_class;
    use misaka_palw_sdk::tir_layout::{TirLayoutChoiceV1, tir_declare_layout_v1};
    let work = scratch("bind-exact");
    let (base, _, dir) = build(&fixture("hf/llama"), &work, 64, false);
    let params = misaka_palw_sdk::runtime_pack::build::network("testnet-12").unwrap();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("network")
    };
    let classfile = work.join("declared.palwtir");
    let choice = TirLayoutChoiceV1 { max_context: Some(256), tile_len: 16, h_chunk: 8, logits_tile: Some(32), ..Default::default() };
    let chosen = tir_declare_layout_v1(&params, bundle, &work.join("artifact.palwtir"), &classfile, &choice, None).unwrap();
    let bound_dir = work.join("bound");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_palw-class"))
        .args(["pack", "bind-class", "--pack"])
        .arg(&dir)
        .arg("--artifact")
        .arg(&classfile)
        .args(["--network", "testnet-12", "--out"])
        .arg(&bound_dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let bound = RuntimePackV1::parse(&std::fs::read_to_string(bound_dir.join(PACK_FILE)).unwrap()).unwrap();
    assert_ne!(bound.digest(), base.digest());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), bound.digest());
    let d = &bound.declared[0];
    assert_eq!(d.class_id, chosen.class_id.to_string());
    assert_eq!(d.exact_layout.as_ref().unwrap().commit_tiles, chosen.layout.commit_tiles);
    assert_eq!(d.exact_layout.as_ref().unwrap().state_tiles, chosen.layout.state_tiles);
    assert!(bind_class(&dir, &classfile, "testnet-12", &bound_dir).unwrap_err().contains("already exists"));
    for artifact in [work.join("artifact.palwtir"), classfile.clone()] {
        let mut o = VerifyOpts::new(&bound_dir);
        o.artifact = Some(artifact);
        let report = verify_pack(&o, &|_| {}).unwrap();
        assert!(report.ok(), "{}", report.render());
        assert!(report.checks.iter().any(|c| c.name == "declared" && c.status == Status::Pass), "{}", report.render());
    }
    let mut o = VerifyOpts::new(&bound_dir);
    o.artifact = Some(classfile);
    for field in [
        "max_context",
        "checkpoint_interval",
        "h_tile",
        "layout_digest",
        "class_id",
        "file_digest",
        "version",
        "commit_tiles",
        "state_tiles",
        "logits_scheme_id",
    ] {
        let mut v = serde_json::to_value(&bound).unwrap();
        match field {
            "commit_tiles" | "state_tiles" => v["declared"][0]["exact_layout"][field][0] = 8.into(),
            "logits_scheme_id" => v["declared"][0]["exact_layout"][field] = "0".repeat(128).into(),
            "layout_digest" | "class_id" | "file_digest" => v["declared"][0][field] = "0".repeat(128).into(),
            "version" => v["declared"][0]["exact_layout"][field] = 99.into(),
            "h_tile" => v["declared"][0][field] = 4.into(),
            _ => v["declared"][0][field] = 3.into(),
        }
        std::fs::write(bound_dir.join(PACK_FILE), serde_json::to_string(&v).unwrap()).unwrap();
        let report = verify_pack(&o, &|_| {}).unwrap();
        assert!(!report.ok(), "mutation {field}: {}", report.render());
    }
    let mut legacy = bound.clone();
    legacy.declared[0].exact_layout = None;
    std::fs::write(bound_dir.join(PACK_FILE), legacy.to_pretty()).unwrap();
    let report = verify_pack(&o, &|_| {}).unwrap();
    assert!(report.ok() && !report.verified(), "{}", report.render());
    assert!(report.checks.iter().any(|c| c.name == "declared" && c.status == Status::Skipped));
    // A copied sidecar cannot silently redefine the evidence, and no output is created on this failure.
    std::fs::write(dir.join("stats.json"), "tampered").unwrap();
    let bad = work.join("bad");
    assert!(bind_class(&dir, o.artifact.as_ref().unwrap(), "testnet-12", &bad).unwrap_err().contains("sidecar"));
    assert!(!bad.exists());
    let _ = std::fs::remove_dir_all(work);
}
