//! Genesis 8 — packing legacy artifacts into `PALWART3` without touching a weight byte.
//!
//! * A16 (`PALWB0A2`, what `qwen25-convert --a16` writes — genesis #1): the section's inner root is the
//!   artifact's `artifact_digest`; `extract-legacy` returns the exact file.
//! * TIR (`PALWTIR1`, what `palw-tir-convert` writes — genesis #2…#10): one entry per instance in
//!   inventory order, the inner root is the consensus inventory root, re-derived from the container's
//!   own bytes by `verify --deep`; `extract-legacy` re-assembles a byte-identical `PALWTIR1` file.
//! * The common sections carry the checkpoint's tokenizer, template, configs and license as data.
//!
//! Written, not run here (the validation lane compiles and runs it).

use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q, encode_artifact_file_v1};
use misaka_palw_base0::engine_a16::derived_a16_store;
use misaka_palw_base0::tir_a16::convert_a16_to_tir;
use kaspa_consensus_core::palw_artifact::{PalwArtifactMerkleFrontierV1, artifact_leaf_parts_v1};
use misaka_palw_sdk::palwart_deep::verify_deep_v3;
use misaka_palw_sdk::tir_stream::palw_tir_inventory_root_of_file_v1;
use misaka_palw_tir_artifact::PalwTirContainerV1;
use misaka_palw_tir_artifact::palwart_pack::{
    PalwArtPackInputsV3, TwinFrontierV1, extract_legacy_v3, pack_legacy_v3, tir_inventory_root_twin_v1, twin_graph_ir_root_v1, twin_leaf_v1,
    verify_v3,
};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir_artifact::palwart_v3::{self as v3, PalwArtSegmentDoneV3, PalwArtV3, section_kind};
use std::path::{Path, PathBuf};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "palwart-pack-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn artifact(layers: usize, vocab: usize) -> Base0ArtifactV1 {
    let shape = Base0ShapeV1 {
        n_layers: layers,
        n_heads: 4,
        n_kv_heads: 2,
        d_head: 8,
        d_ff: 48,
        vocab,
        max_position: 32,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: 1,
    };
    Base0ArtifactV1::derive_deterministic(shape, 0x6E5).expect("shape").with_a16_params(derived_a16_store(&shape)).expect("store")
}

/// A checkpoint directory with the data files a pack carries (never executed).
fn checkpoint(dir: &Path) {
    std::fs::write(dir.join("tokenizer.json"), br#"{"version":"1.0","model":{"type":"BPE"}}"#).unwrap();
    std::fs::write(dir.join("tokenizer_config.json"), br#"{"chat_template":"{{ messages }}"}"#).unwrap();
    std::fs::write(dir.join("config.json"), br#"{"architectures":["Qwen2ForCausalLM"],"model_type":"qwen2"}"#).unwrap();
    std::fs::write(dir.join("LICENSE"), b"Apache License\nVersion 2.0\n").unwrap();
}

fn inputs(src: &Path, out: &Path, family: u16) -> PalwArtPackInputsV3 {
    PalwArtPackInputsV3 {
        source_dir: Some(src.to_path_buf()),
        manifest: serde_json::json!({
            "schema": "misaka.genesis8.palwart-manifest.v1",
            "name": "Example-PALW",
            "source": {"repo": "example/model", "revision": "0123456789abcdef0123456789abcdef01234567"},
            "license": {"id": "apache-2.0"},
        }),
        family,
        out_dir: out.to_path_buf(),
        stem: "Example-PALW".into(),
        segment_bytes: 1 << 20,
        license_files: Vec::new(),
        calibration_files: Vec::new(),
        provenance: serde_json::json!({"test": true}),
    }
}

fn no_hook<'a>() -> v3::PalwArtSegmentHookV3<'a> {
    Box::new(|_: &PalwArtSegmentDoneV3| -> Result<(), String> { Ok(()) })
}

#[test]
fn an_a16_artifact_is_carried_whole_and_extracts_byte_identical() {
    let (src, out) = (tmp("a16-src"), tmp("a16-out"));
    checkpoint(&src);
    let a = artifact(2, 64);
    let legacy = src.join("model.a16.palwart");
    let bytes = encode_artifact_file_v1(&a);
    std::fs::write(&legacy, &bytes).unwrap();
    let packed = pack_legacy_v3(&legacy, &inputs(&src, &out, v3::family::QWEN2_DENSE), no_hook()).expect("packed");
    assert_eq!(packed.weights.inner_root, a.artifact_digest().as_bytes(), "the legacy identity is the inner root");
    let art = PalwArtV3::open(&packed.written.index_path).expect("opens");
    assert_eq!(art.root, packed.written.root);
    let report = verify_v3(&art).expect("verifies");
    assert!(report["inner_root"].as_str().unwrap().starts_with("ok (declared artifact_digest"));
    let deep = verify_deep_v3(&art).expect("deep");
    assert_eq!(deep["deep"], serde_json::json!("ok (artifact_digest recomputed by the BASE-0 decoder)"));
    // The common sections are there, as data.
    for kind in [section_kind::TOKENIZER, section_kind::CHAT_TEMPLATE, section_kind::SOURCE_METADATA, section_kind::LICENSE] {
        assert!(art.section(kind).is_some(), "{}", section_kind::name(kind));
    }
    assert!(art.find_entry(section_kind::CHAT_TEMPLATE, "tokenizer_config.json#chat_template").is_some());
    assert!(art.find_entry(section_kind::LICENSE, "source/LICENSE").is_some());
    let manifest: serde_json::Value = serde_json::from_slice(&art.header.manifest).unwrap();
    assert_eq!(manifest["weights"]["legacy_format"], serde_json::json!("PALWB0A2 (legacy BASE-0 A16 .palwart)"));
    // Extract: the exact legacy file.
    let back = out.join("back.palwart");
    extract_legacy_v3(&art, &back).expect("extracts");
    assert_eq!(std::fs::read(&back).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn a_tir_artifact_keeps_its_inventory_root_and_extracts_byte_identical() {
    let (src, out) = (tmp("tir-src"), tmp("tir-out"));
    checkpoint(&src);
    let legacy = src.join("model.palwtir");
    convert_a16_to_tir(&artifact(3, 100), HISTORY_BOUND_V1_SMALL, &legacy, "{\"converter\":\"test\"}".into()).expect("converted");
    let (root, leaves) = palw_tir_inventory_root_of_file_v1(&legacy).expect("legacy root");
    let packed = pack_legacy_v3(&legacy, &inputs(&src, &out, v3::family::QWEN35_GDN_HYBRID), no_hook()).expect("packed");
    assert_eq!(packed.weights.inner_root, root.as_bytes());
    assert_eq!(packed.weights.inner_leaf_count, leaves as u64);
    let art = PalwArtV3::open(&packed.written.index_path).expect("opens");
    let report = verify_v3(&art).expect("verifies");
    assert_eq!(report["inner_root"], serde_json::json!("ok (TIR inventory root)"), "re-derived from the container's own bytes");
    let deep = verify_deep_v3(&art).expect("deep");
    assert_eq!(deep["deep"], serde_json::json!("ok (consensus inventory root and graph-IR root)"));
    let back = out.join("back.palwtir");
    extract_legacy_v3(&art, &back).expect("extracts");
    assert_eq!(std::fs::read(&back).unwrap(), std::fs::read(&legacy).unwrap(), "byte-identical PALWTIR1");
    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn the_same_legacy_artifact_packs_to_the_same_root_on_every_run() {
    let src = tmp("det-src");
    checkpoint(&src);
    let legacy = src.join("model.palwtir");
    convert_a16_to_tir(&artifact(1, 17), HISTORY_BOUND_V1_SMALL, &legacy, "{}".into()).expect("converted");
    let (o1, o2) = (tmp("det-1"), tmp("det-2"));
    let mut i2 = inputs(&src, &o2, v3::family::QWEN35_GDN_HYBRID);
    i2.segment_bytes = 2 << 20;
    i2.provenance = serde_json::json!({"host": "other"});
    let a = pack_legacy_v3(&legacy, &inputs(&src, &o1, v3::family::QWEN35_GDN_HYBRID), no_hook()).unwrap();
    let b = pack_legacy_v3(&legacy, &i2, no_hook()).unwrap();
    assert_eq!(a.written.root, b.written.root, "segment size and provenance are not identity");
    let mut i3 = inputs(&src, &tmp("det-3"), v3::family::QWEN35_GDN_HYBRID);
    i3.manifest["license"]["id"] = serde_json::json!("llama3.3");
    let c = pack_legacy_v3(&legacy, &i3, no_hook()).unwrap();
    assert_ne!(a.written.root, c.written.root, "the manifest is identity");
    let _ = std::fs::remove_dir_all(&src);
}

#[test]
fn an_unsupported_file_is_refused_by_name() {
    let (src, out) = (tmp("refuse-src"), tmp("refuse-out"));
    let f = src.join("model.gguf");
    std::fs::write(&f, b"GGUF\x03\0\0\0rest").unwrap();
    let e = pack_legacy_v3(&f, &inputs(&src, &out, v3::family::NONE), no_hook()).unwrap_err();
    assert!(e.contains("unknown"), "{e}");
    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&out);
}

/// The light packer's twin of the consensus inventory root (leaf, Merkle frontier, graph-IR root) is
/// the consensus function, on every container shape these tests make and on every frontier size.
#[test]
fn the_packer_twin_is_the_consensus_inventory_root() {
    for (tag, layers, vocab) in [("t1", 1usize, 17usize), ("t2", 2, 64), ("t3", 3, 100)] {
        let src = tmp(tag);
        let legacy = src.join("model.palwtir");
        convert_a16_to_tir(&artifact(layers, vocab), HISTORY_BOUND_V1_SMALL, &legacy, "{}".into()).expect("converted");
        let c = PalwTirContainerV1::open(&legacy).unwrap();
        let (want, n) = palw_tir_inventory_root_of_file_v1(&legacy).expect("consensus");
        let (got, m) = tir_inventory_root_twin_v1(&c).expect("twin");
        assert_eq!((got, m), (want.as_bytes(), n as u64), "{tag}");
        assert_eq!(
            twin_graph_ir_root_v1(&c.header.program),
            kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&c.header.program).as_bytes()
        );
        let _ = std::fs::remove_dir_all(&src);
    }
    for (name, layer, start, len) in [("w", None, 0u32, 0usize), ("blk.q", Some(3u16), 32768, 100), ("e", Some(0), 7, 32768)] {
        let bytes: Vec<u8> = (0..len).map(|i| (i * 31 % 251) as u8).collect();
        assert_eq!(twin_leaf_v1(name, layer, start, &bytes), artifact_leaf_parts_v1(name, layer, start, &bytes).as_bytes());
    }
    for n in 1..=33u32 {
        let (mut a, mut b) = (TwinFrontierV1::default(), PalwArtifactMerkleFrontierV1::new());
        for i in 0..n {
            let leaf = twin_leaf_v1("x", Some(i as u16), i, &i.to_le_bytes());
            a.push(leaf);
            b.push(kaspa_consensus_core::Hash64::from_bytes(leaf));
        }
        assert_eq!(a.root(), b.root().map(|h| h.as_bytes()), "{n} leaves");
    }
}
