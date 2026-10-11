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
    frontend_sdk(0);
}
#[test]
fn third_party_packed_descriptor_rebuilds_and_registers_through_the_common_sdk() {
    frontend_sdk(1);
}
#[test]
fn third_party_tensors_descriptor_rebuilds_and_registers_through_the_common_sdk() {
    frontend_sdk(2);
}
#[test]
fn third_party_blocks_descriptor_rebuilds_and_registers_through_the_common_sdk() {
    frontend_sdk(3);
}
#[test]
fn third_party_role_json_descriptor_rebuilds_and_registers_through_the_common_sdk() {
    frontend_sdk(4);
}
#[test]
fn third_party_native_gguf_rebuilds_and_registers_through_the_common_sdk() {
    frontend_sdk(5);
}
#[test]
fn third_party_split_gguf_rebuilds_and_registers_through_the_common_sdk() {
    frontend_sdk(6);
}
fn frontend_sdk(saved: usize) {
    let packed = saved != 0;
    use misaka_palw_sdk::runtime_pack::{conformance::ConformanceJob, primitive};
    use misaka_palw_tir_lower::frontend_pack::{FORMAT, program::Program};
    use serde_json::{Value, json};
    let dir = std::env::temp_dir().join(format!("palw-frontend-sdk-{}-{saved}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("checkpoint");
    std::fs::create_dir_all(&model).unwrap();
    let p = program();
    let mut params = params_of(&p);
    let gguf = saved >= 5;
    if packed {
        for ((j, l), t) in &mut params.tensors {
            if *j == 1 {
                t.data = (0..16).map(|i| ((i * 3 + l.unwrap() as usize * 5) % 16) as i128 - 8).collect();
            }
            if gguf && *j == 2 {
                for v in &mut t.data {
                    *v += 9_007_199_254_740_993;
                }
            }
        }
    }
    let mut header = serde_json::Map::new();
    let mut data = Vec::new();
    let mut bindings = Vec::new();
    for ((j, l), tensor) in &params.tensors {
        let source = format!("independent.tensor.{j}.{}", l.map_or_else(|| "global".into(), |l| l.to_string()));
        let raw = if packed && *j == 1 {
            tensor.data.chunks_exact(2).map(|b| ((b[0] + 8) as u8) | (((b[1] + 8) as u8) << 4)).collect()
        } else {
            tensor.to_le_bytes()
        };
        let start = data.len();
        data.extend_from_slice(&raw);
        header.insert(source.clone(),json!({"dtype":p.params[*j as usize].dtype.name().to_ascii_uppercase(),"shape":tensor.shape,"data_offsets":[start,data.len()]}));
        if packed && *j == 1 {
            header.insert(source.clone(), json!({"dtype":"U8","shape":[4,2],"data_offsets":[start,data.len()]}));
            let mut roles = json!({"codes":source});
            let mut inert = json!({});
            if saved == 3 || gguf {
                roles = json!({"data":source});
            }
            if saved == 4 {
                let metadata_name = format!("{source}.metadata");
                let metadata = br#"{"shape":[4,4],"offset":-8.0,"flavor":"nibble"}"#;
                let at = data.len();
                data.extend(metadata);
                header.insert(metadata_name.clone(), json!({"dtype":"U8","shape":[metadata.len()],"data_offsets":[at,data.len()]}));
                roles["meta"] = json!(metadata_name);
                inert = json!({"meta":["flavor"]});
            }
            bindings.push(json!({"param":j,"layer":l,"import":{"kind":"descriptor","format":"public-nibble",
                "roles":roles,"config":{},"metadata_inert":inert,"shift":0,"round":"half_away_from_zero","overflow":"reject"}}));
        } else {
            bindings.push(json!({"param":j,"layer":l,"source":source,"import":{"kind":"integer"}}));
        }
    }
    let h = serde_json::to_vec(&header).unwrap();
    let mut raw = (h.len() as u64).to_le_bytes().to_vec();
    raw.extend_from_slice(&h);
    raw.extend_from_slice(&data);
    std::fs::write(model.join("model.safetensors"), raw).unwrap();
    let weight_name = if saved == 6 {
        "model.gguf.index.json"
    } else if gguf {
        "model.gguf"
    } else {
        "model.safetensors"
    };
    if gguf {
        let string = |out: &mut Vec<u8>, s: &str| {
            out.extend((s.len() as u64).to_le_bytes());
            out.extend(s.as_bytes());
        };
        let write_part = |path: &str, part: Option<u16>, range: std::ops::Range<usize>| {
            let mut bytes = b"GGUF".to_vec();
            bytes.extend(3u32.to_le_bytes());
            bytes.extend((range.len() as u64).to_le_bytes());
            let primary = part.is_none_or(|n| n == 0);
            bytes.extend((u64::from(primary) * 2 + u64::from(part.is_some()) * 3).to_le_bytes());
            if let Some(no) = part {
                for (key, value) in [("split.no", no), ("split.count", 3)] {
                    string(&mut bytes, key);
                    bytes.extend(2u32.to_le_bytes());
                    bytes.extend(value.to_le_bytes());
                }
                string(&mut bytes, "split.tensors.count");
                bytes.extend(5u32.to_le_bytes());
                bytes.extend((params.tensors.len() as i32).to_le_bytes());
            }
            if primary {
                string(&mut bytes, "general.architecture");
                bytes.extend(8u32.to_le_bytes());
                string(&mut bytes, "UnregisteredThirdPartyArchitecture");
                string(&mut bytes, "tokenizer.ggml.tokens");
                bytes.extend(9u32.to_le_bytes());
                bytes.extend(8u32.to_le_bytes());
                bytes.extend(16u64.to_le_bytes());
                for i in 0..16 {
                    string(&mut bytes, &format!("tok{i}"));
                }
            }
            let mut body = Vec::new();
            for ((j, l), tensor) in params.tensors.iter().skip(range.start).take(range.len()) {
                body.resize(body.len().div_ceil(32) * 32, 0);
                let name = format!("independent.tensor.{j}.{}", l.map_or_else(|| "global".into(), |l| l.to_string()));
                string(&mut bytes, &name);
                bytes.extend((tensor.shape.len() as u32).to_le_bytes());
                for dim in tensor.shape.iter().rev() {
                    bytes.extend((*dim as u64).to_le_bytes());
                }
                let ty: u32 = match j {
                    1 => 65535,
                    2 => 27,
                    3 => 25,
                    _ => 24,
                };
                bytes.extend(ty.to_le_bytes());
                bytes.extend((body.len() as u64).to_le_bytes());
                if *j == 1 {
                    body.extend(tensor.data.chunks_exact(2).map(|b| ((b[0] + 8) as u8) | (((b[1] + 8) as u8) << 4)));
                } else {
                    body.extend(tensor.to_le_bytes());
                }
            }
            bytes.resize(bytes.len().div_ceil(32) * 32, 0);
            bytes.extend(body);
            std::fs::write(model.join(path), bytes).unwrap();
        };
        if saved == 6 {
            write_part("metadata.bin", Some(0), 0..0);
            write_part("part-1.bin", Some(1), 0..2);
            write_part("part-2.bin", Some(2), 2..params.tensors.len());
            std::fs::write(
                model.join(weight_name),
                json!({"format":"misaka.palw.gguf-checkpoint.v1",
                "parts":["part-2.bin","metadata.bin","part-1.bin"]})
                .to_string(),
            )
            .unwrap();
        } else {
            write_part(weight_name, None, 0..params.tensors.len());
        }
        std::fs::remove_file(model.join("model.safetensors")).unwrap();
    }
    std::fs::write(model.join("config.json"), r#"{"model_type":"NotInAnyRegistry"}"#).unwrap();
    let frontend = dir.join("third-party.json");
    let mut definition = json!({"format":FORMAT,"id":"unknown-static-combination","scope":{"task":"text-generation","completeness":"full","components":["decoder"]},
        "inert":["model_type"],"program":Program::of(&p),"bindings":bindings});
    if packed {
        let floats: Vec<u8> = [-8f32, 7.0, 0.0, -1.0].into_iter().flat_map(f32::to_le_bytes).collect();
        definition["quant_formats"] = json!({"public-nibble":{"schema":"misaka.palw.quant-format.v1","name":"IndependentlyPublishedNibble",
            "layout":{"kind":"virtual","roles":[{"name":"codes","suffix":".arbitrary","dtypes":["U8"],"rank":2}],
                "axes":["o","i"],"shape":["dim_codes[0]","dim_codes[1]*2"]},
            "decode":{"target":"tensor","value":"((codes[o,i/2] >> (4*(i%2))) & 15)-8"},
            "tests":[{"roles":{"codes":{"dtype":"U8","shape":[1,2],"hex":"f078"}},
                "values_f32_hex":misaka_palw_tir_lower::frontend_pack::program::hex(&floats)}]}});
    }
    if saved == 2 {
        definition["quant_formats"]["public-nibble"]["layout"] = json!({"kind":"tensors","roles":[{"name":"codes","suffix":".anything","dtypes":["U8"],"rank":2}],"dims":{"out":"dim_codes[0]","inp":"dim_codes[1]*2"}});
        definition["quant_formats"]["public-nibble"]["decode"] = json!({"target":"integers","group":{"size":2},"q":"(codes[o,i/2] >> (4*(i%2))) & 15","scale":"1","zero":"8","code":{"min":0,"max":15}});
    } else if saved == 3 || gguf {
        definition["quant_formats"]["public-nibble"]["layout"] =
            json!({"kind":"blocks","elems":4,"bytes":2,"fields":[{"name":"codes","at":0,"type":"u8","count":2}]});
        definition["quant_formats"]["public-nibble"]["decode"] = json!({"target":"integers","group":{"size":4},"q":"(codes[e/2] >> (4*(e%2))) & 15","scale":"1","zero":"8","code":{"min":0,"max":15}});
        let test = &mut definition["quant_formats"]["public-nibble"]["tests"][0];
        test.as_object_mut().unwrap().remove("roles");
        test["block_hex"] = json!("f078");
        if gguf {
            definition["quant_formats"]["public-nibble"]["ids"] = json!([{"scheme":"ggml","id":65535}]);
            definition["inert"].as_array_mut().unwrap().push(json!("general.architecture"));
            definition["inert"].as_array_mut().unwrap().push(json!("tokenizer.ggml.tokens"));
        }
    } else if saved == 4 {
        definition["quant_formats"]["public-nibble"]["layout"] = json!({"kind":"tensors","roles":[{"name":"codes","suffix":".anything","dtypes":["U8"],"rank":2},{"name":"meta","suffix":".metadata","dtypes":["U8"],"rank":1}],"dims":{"out":"rows","inp":"cols"}});
        definition["quant_formats"]["public-nibble"]["params"] = json!({"rows":{"from_role":{"role":"meta","path":"shape[0]","kind":"int"}},"cols":{"from_role":{"role":"meta","path":"shape[1]","kind":"int"}},"offset":{"from_role":{"role":"meta","path":"offset","kind":"float"}}});
        definition["quant_formats"]["public-nibble"]["decode"] =
            json!({"target":"floats","value":"((codes[o,i/2] >> (4*(i%2))) & 15) + offset"});
        let test = &mut definition["quant_formats"]["public-nibble"]["tests"][0];
        let vector_meta = br#"{"shape":[1,4],"offset":-8.0}"#;
        test["roles"]["meta"] =
            json!({"dtype":"U8","shape":[vector_meta.len()],"hex":misaka_palw_tir_lower::frontend_pack::program::hex(vector_meta)});
    }
    std::fs::write(&frontend, definition.to_string()).unwrap();
    let pack_dir = dir.join("pack");
    let artifact = dir.join("model.palwtir");
    let jobs = vec![ConformanceJob { label: "public-prompt".into(), prompt: vec![1, 2, 3, 4], decode: 4 }];
    let built = primitive::build(
        &model,
        &frontend,
        &artifact,
        &pack_dir,
        &jobs,
        Some("fixture-revision".into()),
        if saved == 4 { 16 } else { 8 },
    )
    .unwrap();
    assert_eq!(built.source_equivalence, SOURCE_EQUIVALENCE_UNVERIFIED);
    assert_eq!(built.implementations.len(), 3);
    assert_eq!(built.build.quant_formats.len(), usize::from(packed));
    let peer = dir.join("peer");
    std::fs::create_dir_all(&peer).unwrap();
    for f in &built.source_files {
        std::fs::copy(model.join(&f.path), peer.join(&f.path)).unwrap();
    }
    if saved == 6 {
        assert_eq!(built.source_files.len(), 5, "index, three parts and config are all pinned");
        for name in ["metadata.bin", "part-1.bin", "part-2.bin"] {
            assert!(
                primitive::build(&model, &frontend, &model.join(name), &pack_dir, &jobs, None, 32)
                    .unwrap_err()
                    .contains("FRONTEND_OUTPUT_CONFLICT")
            );
        }
    }
    let rebuilt = dir.join("rebuilt.palwtir");
    let verified = primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 127).unwrap();
    assert_eq!(&built, verified.pack());
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &artifact, 32).unwrap_err().contains("FRONTEND_OUTPUT_CONFLICT"));
    assert!(
        primitive::build(&model, &frontend, &model.join(weight_name), &pack_dir, &jobs, None, 32)
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
    if gguf {
        let explicit = primitive::build(
            &peer.join(weight_name),
            &frontend,
            &dir.join("file.palwtir"),
            &dir.join("file-pack"),
            &jobs,
            Some("fixture-revision".into()),
            64,
        )
        .unwrap();
        assert_eq!(built.digest().unwrap(), explicit.digest().unwrap(), "directory/file source selection has no identity priority");
        primitive::verify(&pack_dir, &peer.join(weight_name), &artifact, &dir.join("file-rebuilt.palwtir"), 16).unwrap();
        let payload_name = if saved == 6 { "part-2.bin" } else { weight_name };
        let original = std::fs::read(peer.join(payload_name)).unwrap();
        let mut corrupted = original.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        std::fs::write(peer.join(payload_name), corrupted).unwrap();
        let prior = std::fs::read(&rebuilt).unwrap();
        assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_SOURCE_MISMATCH"));
        assert_eq!(prior, std::fs::read(&rebuilt).unwrap());
        std::fs::write(peer.join(payload_name), original).unwrap();
        if saved == 6 {
            let original = std::fs::read(peer.join(weight_name)).unwrap();
            std::fs::write(
                peer.join(weight_name),
                json!({"format":"misaka.palw.gguf-checkpoint.v1",
                "parts":["metadata.bin","part-1.bin","part-2.bin"]})
                .to_string(),
            )
            .unwrap();
            assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_SOURCE_MISMATCH"));
            assert_eq!(prior, std::fs::read(&rebuilt).unwrap());
            std::fs::write(peer.join(weight_name), original).unwrap();
        }
        std::fs::write(peer.join("tokenizer.json"), "{}").unwrap();
        assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_SOURCE_MISMATCH"));
        std::fs::remove_file(peer.join("tokenizer.json")).unwrap();
        #[cfg(unix)]
        {
            let conflict = dir.join("conflict-pack");
            std::fs::create_dir_all(&conflict).unwrap();
            let prior = std::fs::read(model.join(weight_name)).unwrap();
            std::os::unix::fs::symlink(model.join(weight_name), conflict.join(primitive::FRONTEND_FILE)).unwrap();
            assert!(
                primitive::build(&model, &frontend, &dir.join("conflict-artifact"), &conflict, &jobs, None, 32)
                    .unwrap_err()
                    .contains("FRONTEND_OUTPUT_CONFLICT")
            );
            assert_eq!(std::fs::read(model.join(weight_name)).unwrap(), prior);
            std::fs::remove_file(conflict.join(primitive::FRONTEND_FILE)).unwrap();
            std::fs::hard_link(model.join(weight_name), conflict.join(primitive::PACK_FILE)).unwrap();
            assert!(
                primitive::build(&model, &frontend, &dir.join("hardlink-artifact"), &conflict, &jobs, None, 32)
                    .unwrap_err()
                    .contains("FRONTEND_OUTPUT_CONFLICT")
            );
            assert_eq!(std::fs::read(model.join(weight_name)).unwrap(), prior);
        }
    }

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
    let tokenizer = Hash64::from_bytes(a.tokenizer_id);
    if gguf {
        assert_ne!(a.tokenizer_id, [0; 64]);
    } else {
        assert_eq!(a.tokenizer_id, [0; 64]);
    }
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

    if saved == 0 || saved == 6 {
        let fidelity_dir = frontend_fidelity(&dir, &pack_dir, &peer, &artifact);
        frontend_beacon(&dir, &fidelity_dir, &peer, &artifact, &p, &params, &class);
    }

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
    std::fs::write(&rebuilt, b"keep prior published output").unwrap();
    assert!(primitive::verify(&pack_dir, &peer, &artifact, &rebuilt, 32).unwrap_err().contains("FRONTEND_CONFORMANCE_MISMATCH"));
    assert_eq!(std::fs::read(&rebuilt).unwrap(), b"keep prior published output");
    assert!(!std::fs::read_dir(&dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".frontend-pack-")));
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

/// Exercise the public protocol with an independently submitted graph and raw/split sources.
/// Facts are explicitly synthetic: this proves shared tool behavior, not a node or approved policy.
fn frontend_beacon(
    dir: &std::path::Path,
    pack_dir: &std::path::Path,
    model: &std::path::Path,
    base: &std::path::Path,
    program: &TirProgramV1,
    params: &MapParams,
    class: &PalwTirClassV1,
) {
    use misaka_palw_sdk::runtime_pack::{beacon_run::*, commit::*, conformance::ImplSet, facts::MemoryFactSource, primitive};
    use serde_json::{Value, json};
    let class_file = dir.join("beacon-class.palwtir");
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open(base).unwrap();
    misaka_palw_tir_artifact::write_container_v1(
        &class_file,
        program,
        borsh::to_vec(&class.layout).unwrap(),
        container.header.tokenizer_id,
        container.header.meta,
        &mut |j, l| Ok(params.tensors[&(j, l)].to_le_bytes()),
    )
    .unwrap();
    let mut policy = misaka_palw_challenge::reference_policy_v1(3, 2, 40, 5, 3);
    policy.security_bits = 8;
    let mut scope = ConformanceScopeV1::new(2, 3, 1, 8);
    scope.vector_fault_ppm = 1_000_000;
    scope.leaf_fault_ppm = 500_000;
    let opts = CommitParamsV1::new(
        "testnet-12",
        misaka_palw_challenge::hash::named_id("independent-genesis"),
        misaka_palw_challenge::hash::named_id("independent-ruleset"),
        policy,
        scope,
    );
    assert_eq!(bind_commitment(pack_dir, &class_file, &opts, &|_| {}).unwrap_err().code, "LAYOUT_REQUIRED");
    let wrong_class_file = dir.join("wrong-beacon-class.palwtir");
    let mut wrong_program = program.clone();
    let misaka_palw_tir::Prim::Clamp { hi, .. } = &mut wrong_program.blocks.last_mut().unwrap().nodes.last_mut().unwrap().prim else {
        panic!()
    };
    *hi -= 1;
    misaka_palw_tir_artifact::write_container_v1(
        &wrong_class_file,
        &wrong_program,
        borsh::to_vec(&class.layout).unwrap(),
        container.header.tokenizer_id,
        "{}".into(),
        &mut |j, l| Ok(params.tensors[&(j, l)].to_le_bytes()),
    )
    .unwrap();
    let wrong_out = dir.join("wrong-bound-frontend");
    assert!(misaka_palw_sdk::runtime_pack::bind::bind_frontend_class(pack_dir, &wrong_class_file, "testnet-12", &wrong_out).is_err());
    assert!(!wrong_out.exists());
    let bound_dir = dir.join("bound-frontend-pack");
    let pack = misaka_palw_sdk::runtime_pack::bind::bind_frontend_class(pack_dir, &class_file, "testnet-12", &bound_dir).unwrap();
    assert_eq!(pack.declared.len(), 1);
    primitive::verify(&bound_dir, model, base, &dir.join("bound-rebuilt"), 32).unwrap();
    assert!(misaka_palw_sdk::runtime_pack::bind::bind_frontend_class(pack_dir, &class_file, "testnet-12", &bound_dir).is_err());
    let state = dir.join("frontend-beacon-state");
    let (b, _) = commit_conformance(&bound_dir, &class_file, &state, &opts, &|_| {}).unwrap();
    let container = misaka_palw_tir_artifact::PalwTirContainerV1::open(&class_file).unwrap();
    assert_eq!(b.commitment.program_root, misaka_palw_kernel::public::program_root_v1(&container.program.encode()));
    assert_ne!(b.commitment.program_root, *PalwTirManifestV1::derive_streamed(&class_file).unwrap().graph_ir_root.as_byte_slice());
    let descriptor = misaka_palw_sdk::preflight::kernel::reference_program_kernels()
        .into_iter()
        .find(|(_, d)| d.digest() == b.commitment.kernel_descriptor_id)
        .unwrap()
        .1;
    let plan = misaka_palw_kernel::plan::plan_for_tir_program_v1(
        &descriptor,
        &container.program,
        b.commitment.program_root,
        b.admission.positions,
    )
    .unwrap();
    assert_eq!(b.commitment.verification_plan_root, plan.root());
    assert_eq!(b.class_id, *class.class_id(&PalwTirManifestV1::derive_streamed(base).unwrap().inventory_root).as_byte_slice());
    assert_eq!(b.commitment.calibration_id, misaka_palw_challenge::RootV1::Absent);
    assert!(b.admission.hypothetically_armed);
    assert_eq!(b.admission.shipped_outcome, "KERNEL_NOT_ACTIVE");
    for positions in [0, class.layout.max_context + 1, u32::MAX] {
        let mut bad = opts.clone();
        bad.plan_positions = Some(positions);
        assert_eq!(bind_commitment(&bound_dir, &class_file, &bad, &|_| {}).unwrap_err().code, "SCOPE_INVALID");
    }
    let facts = synthetic_facts(&b.commitment, &opts.policy, 100, "independent-frontend");
    let source = MemoryFactSource(facts.clone());
    let root = hex(&b.commitment.statement_root());
    let mut run = RunInput {
        pack_dir: &bound_dir,
        artifact: &class_file,
        state_dir: &state,
        commitment: &root,
        source: &source,
        impls: ImplSet::default(),
        max_checks: Some(2),
        fault: None,
    };
    assert!(matches!(run_conformance(&run, &|_| {}).unwrap(), RunOutcome::Interrupted { done: 2, .. }));
    run.max_checks = None;
    let RunOutcome::Evidence { evidence, dir: evidence_dir, local, provenance, .. } = run_conformance(&run, &|_| {}).unwrap() else {
        panic!("expected completed evidence")
    };
    assert!(local.is_ok());
    assert!(matches!(provenance, misaka_palw_sdk::runtime_pack::facts::FactsProvenanceV1::Synthetic { .. }));
    let ev = evidence_dir.join("evidence.borsh");
    let mut verify = VerifyInput {
        pack_dir: &bound_dir,
        artifact: &class_file,
        state_dir: &state,
        commitment: &root,
        evidence: &ev,
        source: &source,
        rerun: true,
        impls: ImplSet::default(),
    };
    assert!(verify_conformance(&verify, &|_| {}).unwrap().is_pass());
    verify.rerun = false;
    assert!(matches!(verify_conformance(&verify, &|_| {}).unwrap(), Verdict::NotReproduced { .. }));
    verify.rerun = true;
    let mut forged = evidence;
    forged.reference_result_root[0] ^= 1;
    let false_ev = evidence_dir.join("forged.borsh");
    std::fs::write(&false_ev, borsh::to_vec(&forged).unwrap()).unwrap();
    verify.evidence = &false_ev;
    assert!(!verify_conformance(&verify, &|_| {}).unwrap().is_pass());
    verify.evidence = &ev;
    // All provenance, compiler, implementation, vector and exact-layout changes invalidate the
    // pre-beacon commitment. Neither an edited flag nor an alternate manifest selects a weaker path.
    let file = bound_dir.join(primitive::PACK_FILE);
    let original = std::fs::read(&file).unwrap();
    for (pointer, value) in [
        ("/revision", json!("different-revision")),
        ("/source_files/0/sha256", json!("11".repeat(32))),
        ("/build/pack_hash", json!("00".repeat(64))),
        ("/build/compiler_source_digest", json!("00".repeat(64))),
        ("/implementation_revisions/0/source_digest", json!("00".repeat(64))),
        ("/declared/0/exact_layout/commit_tiles/0", json!(1)),
        ("/conformance/0/logits_digest", json!("00".repeat(32))),
        ("/source_equivalence", json!("VERIFIED")),
        ("/fidelity/policy/max_abs", json!(format!("{:016x}", 100f64.to_bits()))),
        ("/fidelity/reference/measured/rmse", json!(format!("{:016x}", 99f64.to_bits()))),
    ] {
        let mut changed: Value = serde_json::from_slice(&original).unwrap();
        *changed.pointer_mut(pointer).unwrap() = value;
        std::fs::write(&file, changed.to_string()).unwrap();
        assert_eq!(verify_conformance(&verify, &|_| {}).unwrap_err().code, "COMMITMENT_STALE", "{pointer}");
    }
    std::fs::write(&file, &original).unwrap();
    let front = bound_dir.join(primitive::FRONTEND_FILE);
    let original_front = std::fs::read(&front).unwrap();
    let mut changed: Value = serde_json::from_slice(&original_front).unwrap();
    changed["id"] = json!("substituted");
    std::fs::write(&front, changed.to_string()).unwrap();
    assert_eq!(verify_conformance(&verify, &|_| {}).unwrap_err().code, "COMMITMENT_STALE");
    std::fs::write(&front, original_front).unwrap();
    std::fs::write(bound_dir.join("pack.json"), "{}").unwrap();
    assert_eq!(bind_commitment(&bound_dir, &class_file, &opts, &|_| {}).unwrap_err().code, "PACK_MISMATCH");
    std::fs::remove_file(bound_dir.join("pack.json")).unwrap();
    assert!(verify_conformance(&verify, &|_| {}).unwrap().is_pass());

    // A new process uses the same bind/commit/run/verify commands as a ModelSpec pack.
    let cli_bound = dir.join("cli-bound-frontend");
    let bin = env!("CARGO_BIN_EXE_palw-class");
    let output = std::process::Command::new(bin)
        .args(["pack", "bind-class", "--pack"])
        .arg(pack_dir)
        .arg("--artifact")
        .arg(&class_file)
        .args(["--network", "testnet-12", "--out"])
        .arg(&cli_bound)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), pack.digest().unwrap());
    let facts_file = dir.join("frontend-facts.json");
    std::fs::write(
        &facts_file,
        serde_json::to_vec_pretty(&misaka_palw_sdk::runtime_pack::facts::facts_to_json(&facts, &opts.policy.id())).unwrap(),
    )
    .unwrap();
    let output = std::process::Command::new(bin)
        .args(["pack", "verify-conformance", "--pack"])
        .arg(&cli_bound)
        .arg("--artifact")
        .arg(&class_file)
        .arg("--state")
        .arg(&state)
        .arg("--commitment")
        .arg(&root)
        .arg("--facts")
        .arg(&facts_file)
        .arg("--evidence")
        .arg(&ev)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("PASS") && stdout.contains("SYNTHETIC"), "{stdout}");
}

/// A deliberately synthetic reference exercises the logit interface and strict predeclared
/// criteria. It provides no evidence of fidelity to a real HF model, routing or task quality.
fn frontend_fidelity(
    dir: &std::path::Path,
    pack_dir: &std::path::Path,
    model: &std::path::Path,
    artifact: &std::path::Path,
) -> std::path::PathBuf {
    use misaka_palw_sdk::runtime_pack::{frontend_fidelity::*, hfref::*, manifest::LogitsSection, primitive};
    use serde_json::{Value, json};
    let art = misaka_palw_tir_exec::node::TirArtifactV1::open(artifact).unwrap();
    let scale = 1.0 / 256.0;
    let sequences: Vec<_> = [vec![1, 2, 3, 4], vec![9, 2, 6, 1]]
        .into_iter()
        .map(|tokens| {
            let logits = program_logits_streamed(&art, &tokens, scale).unwrap().into_iter().flatten().map(|v| v as f32).collect();
            HfSequence { tokens, logits }
        })
        .collect();
    let reference = HfReference {
        producer: json!({"fixture_reference":true,"source":"same integer program; no real HF model"}),
        vocab: 16,
        sequences,
    };
    let ref_dir = dir.join("synthetic-reference");
    std::fs::create_dir(&ref_dir).unwrap();
    reference.write(&ref_dir).unwrap();
    let policy = FidelityPolicy {
        schema: SCHEMA.into(),
        checkpoint_revision: "fixture-revision".into(),
        task: "text-generation".into(),
        context: 8,
        minimum_sequences: 2,
        minimum_positions: 8,
        fit_math: "libm-v1".into(),
        logits: LogitsSection { convention: "legacy-greedy-only".into(), scale, tolerance: default_tolerance() },
        max_abs: 1e-5,
        rmse_max: 1e-5,
        max_import_saturated_values: 0,
    };
    let policy_file = dir.join("fidelity-policy.json");
    let policy_bytes = serde_json::to_vec_pretty(&policy).unwrap();
    std::fs::write(&policy_file, &policy_bytes).unwrap();
    let out = dir.join("frontend-fidelity-pack");
    let pack = attach(pack_dir, artifact, &ref_dir, &policy_file, &out).unwrap();
    assert_eq!(pack.source_equivalence, SOURCE_EQUIVALENCE_UNVERIFIED);
    assert_eq!(pack.fidelity.as_ref().unwrap().policy_digest, policy.digest().unwrap());
    let rebuild = dir.join("fidelity-rebuilt");
    let verified = primitive::verify(&out, model, artifact, &rebuild, 32).unwrap();
    assert_eq!(verified.report().unwrap()["reference_logits"], "WITHIN_PREDECLARED_TOLERANCE");
    assert_eq!(verified.report().unwrap()["task_quality"], "UNVERIFIED");
    assert!(
        primitive::verify(&out, model, artifact, &out.join(HF_REFERENCE_FILE), 32).unwrap_err().contains("FRONTEND_OUTPUT_CONFLICT")
    );
    let prior = b"prior published artifact";
    std::fs::write(&rebuild, prior).unwrap();
    let manifest = out.join(primitive::PACK_FILE);
    let original = std::fs::read(&manifest).unwrap();
    let mut false_fit: Value = serde_json::from_slice(&original).unwrap();
    false_fit["fidelity"]["reference"]["measured"]["rmse"] = json!(format!("{:016x}", 99f64.to_bits()));
    std::fs::write(&manifest, false_fit.to_string()).unwrap();
    assert!(primitive::verify(&out, model, artifact, &rebuild, 32).unwrap_err().contains("FRONTEND_FIDELITY_MISMATCH"));
    assert_eq!(std::fs::read(&rebuild).unwrap(), prior);
    std::fs::write(&manifest, &original).unwrap();
    let raw_path = out.join(HF_REFERENCE_LOGITS_FILE);
    let raw = std::fs::read(&raw_path).unwrap();
    let mut changed = raw.clone();
    changed[0] ^= 1;
    std::fs::write(&raw_path, changed).unwrap();
    assert!(primitive::verify(&out, model, artifact, &rebuild, 32).unwrap_err().contains("FRONTEND_FIDELITY_MISMATCH"));
    assert_eq!(std::fs::read(&rebuild).unwrap(), prior);
    std::fs::write(&raw_path, raw).unwrap();
    for (pointer, value) in [
        ("/checkpoint_revision", json!("wrong-revision")),
        ("/task", json!("wrong-task")),
        ("/fit_math", json!("std")),
        ("/context", json!(0)),
        ("/logits/scale_bits", json!(format!("{:016x}", f64::NAN.to_bits()))),
    ] {
        let mut bad: Value = serde_json::from_slice(&policy_bytes).unwrap();
        *bad.pointer_mut(pointer).unwrap() = value;
        std::fs::write(&policy_file, bad.to_string()).unwrap();
        let bad_out = dir.join("bad-fidelity-policy");
        assert!(
            attach(pack_dir, artifact, &dir.join("nonexistent-reference"), &policy_file, &bad_out)
                .unwrap_err()
                .contains("FRONTEND_FIDELITY_POLICY"),
            "{pointer}"
        );
        assert!(!bad_out.exists());
    }
    let mut bad = policy.clone();
    bad.minimum_positions = 100;
    std::fs::write(&policy_file, serde_json::to_vec(&bad).unwrap()).unwrap();
    let bad_out = dir.join("bad-fidelity-coverage");
    assert!(attach(pack_dir, artifact, &ref_dir, &policy_file, &bad_out).unwrap_err().contains("FRONTEND_FIDELITY_COVERAGE"));
    assert!(!bad_out.exists());
    std::fs::write(&policy_file, &policy_bytes).unwrap();
    let mut false_reference = reference.clone();
    for s in &mut false_reference.sequences {
        for logit in &mut s.logits {
            *logit += 10.0;
        }
    }
    false_reference.write(&ref_dir).unwrap();
    let bad_out = dir.join("bad-fidelity-logits");
    assert!(attach(pack_dir, artifact, &ref_dir, &policy_file, &bad_out).unwrap_err().contains("FRONTEND_FIDELITY_FAILED"));
    assert!(!bad_out.exists());
    reference.write(&ref_dir).unwrap();
    assert_eq!(std::fs::read(&manifest).unwrap(), original);
    let cli_out = dir.join("cli-fidelity-pack");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_palw-class"))
        .args(["pack", "attach-frontend-fidelity", "--pack"])
        .arg(pack_dir)
        .arg("--artifact")
        .arg(artifact)
        .arg("--hf-reference")
        .arg(&ref_dir)
        .arg("--fidelity-policy")
        .arg(&policy_file)
        .arg("--out")
        .arg(&cli_out)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["pack_digest"], pack.digest().unwrap());
    assert_eq!(report["source_equivalence"], SOURCE_EQUIVALENCE_UNVERIFIED);
    assert!(!std::fs::read_dir(dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".frontend-pack-")));
    out
}

#[test]
fn hf_reference_refuses_malformed_shapes_tokens_paths_extents_and_nonfinite_logits() {
    use misaka_palw_sdk::runtime_pack::hfref::*;
    use serde_json::json;
    let dir = std::env::temp_dir().join(format!("palw-hf-reference-refusals-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let good = json!({"schema":HF_REFERENCE_SCHEMA_V1,"producer":{"fixture":true},"vocab":2,
        "logits_file":HF_REFERENCE_LOGITS_FILE,"sequences":[{"tokens":[0,1]}]});
    std::fs::write(dir.join(HF_REFERENCE_FILE), good.to_string()).unwrap();
    let bits: Vec<u8> = [0f32, 1., 2., 3.].into_iter().flat_map(f32::to_le_bytes).collect();
    std::fs::write(dir.join(HF_REFERENCE_LOGITS_FILE), &bits).unwrap();
    assert!(HfReference::load(&dir).is_ok());
    for (pointer, value) in [
        ("/vocab", json!(0)),
        ("/vocab", json!(u64::MAX)),
        ("/logits_file", json!("../secret")),
        ("/sequences/0/tokens", json!([0, "1"])),
        ("/sequences/0/tokens", json!([0, 2])),
        ("/sequences", json!([])),
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        std::fs::write(dir.join(HF_REFERENCE_FILE), bad.to_string()).unwrap();
        assert!(HfReference::load(&dir).is_err(), "{pointer}");
    }
    std::fs::write(dir.join(HF_REFERENCE_FILE), good.to_string()).unwrap();
    for raw in [
        bits[..bits.len() - 1].to_vec(),
        [bits.clone(), vec![0]].concat(),
        [f32::NAN.to_le_bytes().to_vec(), bits[4..].to_vec()].concat(),
    ] {
        std::fs::write(dir.join(HF_REFERENCE_LOGITS_FILE), raw).unwrap();
        assert!(HfReference::load(&dir).is_err());
    }
    std::fs::remove_file(dir.join(HF_REFERENCE_FILE)).unwrap();
    std::fs::write(dir.join("logits.json"), json!({"tokens":[0,1,0],"logits_full":[[0,1],[2],[3,4,5]]}).to_string()).unwrap();
    assert!(HfReference::load(&dir).is_err(), "ragged rows cannot cancel their total lengths");
    std::fs::remove_file(dir.join("logits.json")).unwrap();
    std::fs::write(dir.join("hf.json"), json!({"vocab":2,"sequences":[[0,1]]}).to_string()).unwrap();
    std::fs::write(dir.join("hf-logits.f32"), [bits.clone(), bits].concat()).unwrap();
    assert!(HfReference::load(&dir).is_err(), "audit data cannot leave unread logits");
    let _ = std::fs::remove_dir_all(dir);
}
