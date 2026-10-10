//! **A future family needs no `main` release: a directly built TIR class registers by its bytes alone** (RFC-0002 §II.11.3, the
//! first step of §II.11.4's order).
//!
//! An independent compiler — here a hand-built program, a combination no built-in adapter emits (an `i64` per-layer multiplier, a
//! rounding shift, a clamp) — writes canonical `TirProgramV1` bytes and a `PALWTIR1` artifact. The registry judges it by
//! `prim_set_id`, program version, canonical encoding, admission, the artifact commitment and the lifecycle only:
//!
//! * the verdict, the inventory root and the class id are the same whatever the container's provenance says (a model name, a
//!   frontend, nothing at all) — the provenance enters the file digest only;
//! * the consensus admission and registration sources name no `model_type`, `FeatureId`, adapter, frontend pack or Hub repository;
//! * the onboarding label is `SOURCE_EQUIVALENCE_UNVERIFIED` when no verified runtime pack names the artifact.

use std::borrow::Cow;

use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_param_instances_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::tir_layout::{TirLayoutChoiceV1, tir_choose_layout_v1, tir_class_admission_offline_v1};
use misaka_palw_sdk::tir_manifest::PalwTirManifestV1;
use misaka_palw_sdk::tir_registration::{SOURCE_EQUIVALENCE_UNVERIFIED, TirSourceEquivalenceV1, tir_source_equivalence_v1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::interp::MapParams;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, Ref, Tensor, TensorType, TirProgramV1};

/// A third party's program: an `i8` embedding, two layers of an `i8 [4, 4]` matrix, an `i64` per-layer multiplier and a rounding
/// shift, an `i16` head over 16 ids, committed under the tiled logits scheme. No configuration, no adapter, no feature registry.
fn program() -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("t.embed", DType::I8, &[16, 4], false);
    let w = pb.param("t.w", DType::I8, &[4, 4], true);
    let m = pb.param("t.m", DType::I64, &[4], true);
    let head = pb.param("t.head", DType::I16, &[16, 4], false);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(table, Ref::Input(0), 0, 0);
        let row = b.cast(row, DType::I32);
        b.finish(&[row])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let acc = b.matmul(w, x, DType::I64);
        let acc = b.reshape_fixed(acc, &[4]);
        let y = b.mul(acc, m, DType::I128);
        let y = b.shr(y, 20, misaka_palw_tir::Rounding::HalfAwayFromZero, DType::I128);
        let y = b.clamp(y, -30_000, 30_000, DType::I32);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.reshape_fixed(l, &[16]);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let mut p = pb.finish(pre, vec![layer, layer], post, logits);
    p.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    p
}

fn params_of(p: &TirProgramV1) -> MapParams {
    let mut out = MapParams::default();
    for (j, inst) in palw_tir_param_instances_v1(p).into_iter().enumerate() {
        let d = &p.params[j];
        for l in inst {
            let n: usize = d.shape.iter().map(|x| *x as usize).product();
            let data: Vec<i128> = (0..n)
                .map(|i| {
                    let v = ((i * 37 + j * 11 + l.map_or(0, |l| l as usize) * 5) % 200) as i128 - 100;
                    if d.dtype == DType::I64 { v.abs() * 9_000 + 1 } else { v }
                })
                .collect();
            out.tensors.insert((j as u16, l), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
        }
    }
    out
}

struct Src<'a>(&'a MapParams);
impl PalwTirTensorSourceV1 for Src<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
    }
}

fn write(dir: &std::path::Path, name: &str, p: &TirProgramV1, params: &MapParams, meta: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    misaka_palw_tir_artifact::write_container_v1(&path, p, Vec::new(), [0x70; 64], meta.to_string(), &mut |j, l| {
        params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
    })
    .expect("the container is written");
    path
}

#[test]
fn a_hand_built_class_registers_by_its_bytes_and_its_provenance_moves_nothing() {
    let dir = std::env::temp_dir().join(format!("palw-direct-tir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = program();
    let params = params_of(&p);
    // Two containers of the same program and tensors: one with no provenance at all, one claiming a model, a frontend and a pack.
    let bare = write(&dir, "bare.palwtir", &p, &params, "{}");
    let named = write(
        &dir,
        "named.palwtir",
        &p,
        &params,
        r#"{"model_id":"some-org/some-model","frontend":{"adapter":{"kind":"built-in","id":"llama"}},"pack":"0123"}"#,
    );
    let a = PalwTirManifestV1::derive(&bare).unwrap();
    let b = PalwTirManifestV1::derive(&named).unwrap();
    assert_ne!(a.artifact_digest, b.artifact_digest, "the provenance is in the file digest");
    assert_eq!(a.inventory_root, b.inventory_root, "and in no root");
    assert_eq!(a.graph_ir_root, b.graph_ir_root);
    let (root, _) = kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1(&p, &Src(&params)).unwrap();
    assert_eq!(a.inventory_root, root, "the artifact commitment is the program's declarations and the tensors");

    // The registry's judgment on testnet-12 as shipped (palw_tir_v1 in force): admitted, with no model metadata asked for.
    let params_net = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params_net.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let tokenizer = Hash64::from_bytes([0x70; 64]);
    let chosen = tir_choose_layout_v1(
        &params_net,
        bundle,
        &p,
        tokenizer,
        root,
        a.leaf_count.max(2),
        &TirLayoutChoiceV1 { max_context: Some(64), ..Default::default() },
    )
    .expect("a layout is chosen");
    assert_eq!(chosen.admission, Ok(()), "the hand-built class is admitted: {:?}", chosen.admission);
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: p.encode(),
        layout: chosen.layout.clone(),
        tokenizer_id: tokenizer,
    };
    assert_eq!(tir_class_admission_offline_v1(&params_net, bundle, &class, a.inventory_root), Ok(()));
    assert_eq!(class.class_id(&a.inventory_root), class.class_id(&b.inventory_root), "one class whichever container carried it");

    // The same bytes from another compiler are the same class: re-encoding the decoded program is byte-identical.
    let decoded = class.decode_program().unwrap();
    assert_eq!(decoded.encode(), p.encode());

    // A novel primitive set is refused: the program names a prim_set_id the network has not armed.
    let mut other = p.clone();
    other.prim_set_id[0] ^= 0xff;
    let refused = PalwTirClassV1 { program: other.encode(), ..class.clone() };
    assert!(tir_class_admission_offline_v1(&params_net, bundle, &refused, a.inventory_root).is_err(), "an unknown primitive set");

    // The onboarding label: no verified pack names this artifact.
    let digest_hex: String = a.artifact_digest.iter().map(|x| format!("{x:02x}")).collect();
    match tir_source_equivalence_v1(&digest_hex, None) {
        TirSourceEquivalenceV1::Unverified { code, .. } => assert_eq!(code, SOURCE_EQUIVALENCE_UNVERIFIED),
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The consensus admission and registration sources (outside their tests and comments) name no model identity: no `model_type`, no
/// feature registry, no adapter or frontend, no runtime pack, no Hub repository.
#[test]
fn the_ir_registration_path_names_no_model_identity() {
    let sources = [
        ("palw_tir_admission_v1.rs", include_str!("../../consensus/core/src/palw_tir_admission_v1.rs")),
        ("palw_tir_class_v1.rs", include_str!("../../consensus/core/src/palw_tir_class_v1.rs")),
        ("palw_tir_artifact_v1.rs", include_str!("../../consensus/core/src/palw_tir_artifact_v1.rs")),
        ("palw_tir_v1.rs", include_str!("../../consensus/core/src/palw_tir_v1.rs")),
        ("palw_tir_attempt_v1.rs", include_str!("../../consensus/core/src/palw_tir_attempt_v1.rs")),
    ];
    let banned = [
        "model_type",
        "featureid",
        "feature_id",
        "hf_schema",
        "huggingface",
        "runtime_pack",
        "pack_digest",
        "frontend",
        "adapter",
        "model_id",
    ];
    let mut offenders = Vec::new();
    for (name, text) in sources {
        let code = text.split("\n#[cfg(test)]").next().unwrap_or(text);
        for (i, line) in code.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            let lower = line.split("//").next().unwrap_or("").to_ascii_lowercase();
            for b in banned {
                if lower.contains(b) {
                    offenders.push(format!("{name}:{}: {b}: {}", i + 1, line.trim()));
                }
            }
        }
    }
    assert!(offenders.is_empty(), "the IR registration path names a model identity:\n{}", offenders.join("\n"));
}

/// The declarative route feeds the very same artifact inventory, layout/admission and three
/// engines as a direct compiler. This fixture proves an interface, not real-checkpoint Final.
#[test]
fn third_party_frontend_rebuilds_and_registers_through_the_common_sdk() {
    use misaka_palw_sdk::runtime_pack::{conformance::ConformanceJob, primitive};
    use misaka_palw_tir_lower::frontend_pack::{FORMAT, program::Program};
    use serde_json::{Value, json};
    let dir = std::env::temp_dir().join(format!("palw-frontend-sdk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("checkpoint");
    std::fs::create_dir_all(&model).unwrap();
    let p = program();
    let params = params_of(&p);
    let mut header = serde_json::Map::new();
    let mut data = Vec::new();
    let mut bindings = Vec::new();
    for ((j, l), tensor) in &params.tensors {
        let source = format!("independent.tensor.{j}.{}", l.map_or_else(|| "global".into(), |l| l.to_string()));
        let raw = tensor.to_le_bytes();
        let start = data.len();
        data.extend_from_slice(&raw);
        header.insert(source.clone(),json!({"dtype":p.params[*j as usize].dtype.name().to_ascii_uppercase(),"shape":tensor.shape,"data_offsets":[start,data.len()]}));
        bindings.push(json!({"param":j,"layer":l,"source":source,"import":{"kind":"integer"}}));
    }
    let h = serde_json::to_vec(&header).unwrap();
    let mut raw = (h.len() as u64).to_le_bytes().to_vec();
    raw.extend_from_slice(&h);
    raw.extend_from_slice(&data);
    std::fs::write(model.join("model.safetensors"), raw).unwrap();
    std::fs::write(model.join("config.json"), r#"{"model_type":"NotInAnyRegistry"}"#).unwrap();
    let frontend = dir.join("third-party.json");
    let definition = json!({"format":FORMAT,"id":"unknown-static-combination","scope":{"task":"text-generation","completeness":"full","components":["decoder"]},
        "inert":["model_type"],"program":Program::of(&p),"bindings":bindings});
    std::fs::write(&frontend, definition.to_string()).unwrap();
    let pack_dir = dir.join("pack");
    let artifact = dir.join("model.palwtir");
    let jobs = vec![ConformanceJob { label: "public-prompt".into(), prompt: vec![1, 2, 3, 4], decode: 4 }];
    let built = primitive::build(&model, &frontend, &artifact, &pack_dir, &jobs, Some("fixture-revision".into()), 8).unwrap();
    assert_eq!(built.source_equivalence, SOURCE_EQUIVALENCE_UNVERIFIED);
    assert_eq!(built.implementations.len(), 3);
    let peer = dir.join("peer");
    std::fs::create_dir_all(&peer).unwrap();
    for f in &built.source_files {
        std::fs::copy(model.join(&f.path), peer.join(&f.path)).unwrap();
    }
    let rebuilt = dir.join("rebuilt.palwtir");
    let verified = primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 127).unwrap();
    assert_eq!(&built, verified.pack());
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &artifact, 32).unwrap_err().contains("FRONTEND_OUTPUT_CONFLICT"));
    assert!(
        primitive::build(&model, &frontend, &model.join("model.safetensors"), &pack_dir, &jobs, None, 32)
            .unwrap_err()
            .contains("FRONTEND_OUTPUT_CONFLICT")
    );
    assert_eq!(std::fs::read(&artifact).unwrap(), std::fs::read(&rebuilt).unwrap());
    let pack_again = primitive::build(
        &peer,
        &frontend,
        &dir.join("again.palwtir"),
        &dir.join("again-pack"),
        &jobs,
        Some("fixture-revision".into()),
        32,
    )
    .unwrap();
    assert_eq!(built.digest().unwrap(), pack_again.digest().unwrap(), "no machine path or streaming size in pack identity");

    let vectors = dir.join("vectors.json");
    std::fs::write(&vectors, json!({"sequences":[[1,2,3,4]]}).to_string()).unwrap();
    let cli_pack = dir.join("cli-pack");
    let cli_artifact = dir.join("cli.palwtir");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_palw-class"))
        .args(["pack", "build-frontend", "--model"])
        .arg(&peer)
        .arg("--frontend-pack")
        .arg(&frontend)
        .arg("--pack")
        .arg(&cli_pack)
        .arg("--out")
        .arg(&cli_artifact)
        .arg("--vectors")
        .arg(&vectors)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_palw-class"))
        .args(["pack", "verify-frontend", "--model"])
        .arg(&peer)
        .arg("--artifact")
        .arg(&cli_artifact)
        .arg("--pack")
        .arg(&cli_pack)
        .arg("--rebuild-out")
        .arg(dir.join("cli-rebuilt.palwtir"))
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["rebuild"], "PASS");
    assert_eq!(report["source_equivalence"], SOURCE_EQUIVALENCE_UNVERIFIED);

    let direct = write(&dir, "direct.palwtir", &p, &params, "{}");
    let a = PalwTirManifestV1::derive_streamed(&artifact).unwrap();
    let b = PalwTirManifestV1::derive_streamed(&direct).unwrap();
    assert_eq!(a.inventory_root, b.inventory_root);
    assert_eq!(a.graph_ir_root, b.graph_ir_root);
    let net = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!() };
    let tokenizer = Hash64::from_bytes([0; 64]);
    let chosen = tir_choose_layout_v1(
        &net,
        bundle,
        &p,
        tokenizer,
        a.inventory_root,
        a.leaf_count.max(2),
        &TirLayoutChoiceV1 { max_context: Some(64), ..Default::default() },
    )
    .unwrap();
    assert_eq!(chosen.admission, Ok(()));
    let class =
        PalwTirClassV1 { version: PALW_TIR_CLASS_VERSION_V1, program: p.encode(), layout: chosen.layout, tokenizer_id: tokenizer };
    assert_eq!(tir_class_admission_offline_v1(&net, bundle, &class, a.inventory_root), Ok(()));
    assert_eq!(class.class_id(&a.inventory_root), class.class_id(&b.inventory_root));

    // Public source hashes, frontend identity, integer vectors and fidelity labels are all checked.
    std::fs::write(peer.join("config.json"), "{}").unwrap();
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_SOURCE_MISMATCH"));
    std::fs::copy(model.join("config.json"), peer.join("config.json")).unwrap();
    let sidecar = pack_dir.join(primitive::PACK_FILE);
    let canonical = std::fs::read(&sidecar).unwrap();
    let mut false_profile: Value = serde_json::from_slice(&canonical).unwrap();
    false_profile["admission_profile"]["ceilings"]["max_tile_macs"] = json!(u64::MAX);
    std::fs::write(&sidecar, false_profile.to_string()).unwrap();
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_PROFILE_MISMATCH"));
    let mut false_revision: Value = serde_json::from_slice(&canonical).unwrap();
    false_revision["implementation_revisions"][0]["source_digest"] = json!("00".repeat(64));
    std::fs::write(&sidecar, false_revision.to_string()).unwrap();
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_IMPLEMENTATION_MISMATCH"));
    let mut oversized: Value = serde_json::from_slice(&canonical).unwrap();
    oversized["conformance"][0]["decode"] = json!(u64::MAX);
    std::fs::write(&sidecar, oversized.to_string()).unwrap();
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_CONFORMANCE_LIMIT"));
    let mut false_vector: Value = serde_json::from_slice(&canonical).unwrap();
    false_vector["conformance"][0]["logits_digest"] = json!("00".repeat(32));
    std::fs::write(&sidecar, false_vector.to_string()).unwrap();
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_CONFORMANCE_MISMATCH"));
    let mut false_equivalence: Value = serde_json::from_slice(&canonical).unwrap();
    false_equivalence["source_equivalence"] = json!("VERIFIED");
    std::fs::write(&sidecar, false_equivalence.to_string()).unwrap();
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("unsupported fidelity"));
    std::fs::write(&sidecar, canonical).unwrap();
    let mut wrong = definition.clone();
    wrong["id"] = json!("changed");
    std::fs::write(pack_dir.join(primitive::FRONTEND_FILE), wrong.to_string()).unwrap();
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_BUILD_MISMATCH"));
    assert!(primitive::build(&model, &frontend, &artifact, &pack_dir, &[], None, 32).unwrap_err().contains("jobs required"));
    let _ = std::fs::remove_dir_all(&dir);
}
