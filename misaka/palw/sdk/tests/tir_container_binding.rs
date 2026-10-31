//! RFC-0002 / genesis 8 audit 02-A: a real `PALWART3` container (a `PALWTIR1` IR class packed without touching a weight byte)
//! binds its registered TIR class — and changing only the container, only the inner inventory root or only the runtime is
//! refused by the chain's own check (`check_tir_container_binding_v1`).
#![allow(warnings)]
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_consensus_core::palw_tir_container_v1::check_tir_container_binding_v1;
use misaka_palw_sdk::tir_container::{palwart_root_opening_v1, tir_container_binding_v1};
use misaka_palw_sdk::tir_stream::palw_tir_inventory_root_of_file_v1;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::palwart_pack::{PalwArtPackInputsV3, pack_legacy_v3};
use misaka_palw_tir_artifact::palwart_v3::{self as v3, PalwArtSegmentDoneV3, PalwArtV3, palwart_root_v3};
use std::path::{Path, PathBuf};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "tir-container-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// The golden `dense-gqa-2layer` IR class under the tiled logits scheme, written as the `PALWTIR1` file a node is deployed with.
fn legacy(dir: &Path) -> (PathBuf, PalwTirClassV1) {
    let vector = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../consensus-vectors/tir-v1/programs/dense-gqa-2layer.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&vector).expect("the golden program")).expect("json");
    let mut program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let program = TirProgramV1::decode_canonical(&program.encode()).expect("still canonical");
    let mut tensors = std::collections::BTreeMap::new();
    for t in v["params"].as_array().unwrap() {
        tensors.insert(
            (t["param"].as_u64().unwrap() as u16, t["layer"].as_u64().map(|l| l as u16)),
            unhex(t["le_hex"].as_str().unwrap()),
        );
    }
    let mut commit_tiles = Vec::new();
    for (bi, b) in program.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit {
                let logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                commit_tiles.push(if logits { PALW_LOGITS_TILE_LANES as u32 } else { 8 });
            }
        }
    }
    let layout = PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 64,
        checkpoint_interval: 2,
        h_tile: 4,
        commit_tiles,
        state_tiles: program.states.iter().map(|_| 4).collect(),
    };
    let tokenizer = [9u8; 64];
    let path = dir.join("dense.palwtir");
    misaka_palw_tir_artifact::write_container_v1(
        &path,
        &program,
        borsh::to_vec(&layout).unwrap(),
        tokenizer,
        "{}".into(),
        &mut |j, l| tensors.get(&(j, l)).cloned().ok_or_else(|| format!("no tensor {j} {l:?}")),
    )
    .expect("the container");
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout,
        tokenizer_id: Hash64::from_bytes(tokenizer),
    };
    (path, class)
}

fn inputs(src: &Path, out: &Path) -> PalwArtPackInputsV3 {
    std::fs::write(src.join("tokenizer.json"), br#"{"version":"1.0"}"#).unwrap();
    std::fs::write(src.join("LICENSE"), b"Apache License\n").unwrap();
    PalwArtPackInputsV3 {
        source_dir: Some(src.to_path_buf()),
        manifest: serde_json::json!({"schema": "misaka.genesis8.palwart-manifest.v1", "name": "Dense-PALW", "license": {"id": "apache-2.0"}}),
        family: v3::family::QWEN2_DENSE,
        out_dir: out.to_path_buf(),
        stem: "Dense-PALW".into(),
        segment_bytes: 1 << 20,
        license_files: Vec::new(),
        calibration_files: Vec::new(),
        provenance: serde_json::json!({"test": true}),
    }
}

#[test]
fn a_packed_tir_container_binds_its_registered_class_and_one_field_changes_are_refused() {
    let (src, out) = (tmp("src"), tmp("out"));
    let (path, class) = legacy(&src);
    let (artifact_root, _) = palw_tir_inventory_root_of_file_v1(&path).expect("the class's inventory root");
    let (record, _) = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&class, &artifact_root).expect("record");
    let class_id = class.class_id(&artifact_root);
    let hook: v3::PalwArtSegmentHookV3<'_> = Box::new(|_: &PalwArtSegmentDoneV3| -> Result<(), String> { Ok(()) });
    let packed = pack_legacy_v3(&path, &inputs(&src, &out), hook).expect("packed");
    let art = PalwArtV3::open(&packed.written.index_path).expect("opens");
    // The consensus opening is the artifact crate's root, field for field (the twin pin).
    assert_eq!(palwart_root_opening_v1(&art.header).root_v1(), Hash64::from_bytes(palwart_root_v3(&art.header)));
    assert_eq!(Hash64::from_bytes(art.root), Hash64::from_bytes(palwart_root_v3(&art.header)));
    let honest = tir_container_binding_v1(&art.header, class_id).expect("binding");
    let row = check_tir_container_binding_v1(&honest, &class_id, &artifact_root, &record, 1).expect("the packed container binds");
    assert_eq!(row.container_root, Hash64::from_bytes(art.root));
    let check = |b: &kaspa_consensus_core::palw_tir_container_v1::PalwTirContainerBindingV1| {
        check_tir_container_binding_v1(b, &class_id, &artifact_root, &record, 1)
    };
    // Only the container: a re-pack with another manifest is another root; naming it over this opening is refused.
    let out2 = tmp("out2");
    let mut other = inputs(&src, &out2);
    other.manifest["name"] = serde_json::json!("Dense-PALW-renamed");
    let repacked = pack_legacy_v3(&path, &other, Box::new(|_: &PalwArtSegmentDoneV3| -> Result<(), String> { Ok(()) })).unwrap();
    assert_ne!(repacked.written.root, art.root, "another manifest is another container");
    let mut b = honest.clone();
    b.container_root = Hash64::from_bytes(repacked.written.root);
    assert!(check(&b).is_err(), "another container's root over this opening");
    // …while that container's own binding is a valid binding of the same class (the registrant chooses which, once).
    assert!(check(&tir_container_binding_v1(&repacked.written.header, class_id).unwrap()).is_ok());
    // Only the inner root: the same container identity naming another inventory.
    let mut b = honest.clone();
    b.opening.sections.iter_mut().find(|s| v3::section_kind::is_weights(s.kind)).unwrap().inner_root = Hash64::from_u64_word(1);
    b.container_root = b.opening.root_v1();
    assert_eq!(check(&b).unwrap_err(), "the bound container's inner inventory root is not the class's registered artifact root");
    // Only the runtime: the same inventory under another tokenizer.
    let mut b = honest.clone();
    b.opening.tokenizer_id = Hash64::from_u64_word(2);
    b.container_root = b.opening.root_v1();
    assert_eq!(check(&b).unwrap_err(), "the bound container's tokenizer is not the registered class's");
    // And another class (another inventory root registered elsewhere) cannot borrow this container.
    assert!(check_tir_container_binding_v1(&honest, &class_id, &Hash64::from_u64_word(3), &record, 1).is_err());
    for d in [src, out, out2] {
        let _ = std::fs::remove_dir_all(d);
    }
}
