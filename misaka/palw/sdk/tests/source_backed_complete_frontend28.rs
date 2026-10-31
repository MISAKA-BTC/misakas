//! Source-backed producer tests; staged for the later validation phase.
#![allow(warnings)]
use misaka_palw_sdk::full_component_v28::frontend::compile_complete_frontend_v28;
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, TirProgramV2};
use misaka_palw_tir::{DType, Ref, TensorType, builder::ProgramBuilder};
use misaka_palw_tir_lower::frontend_pack::{self, program::Program, stage28::StageSemanticsV28};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn sha(b: &[u8]) -> String {
    frontend_pack::program::hex(&Sha256::digest(b))
}
fn write(root: &Path, name: &str, b: &[u8]) {
    std::fs::write(root.join(name), b).unwrap();
}
fn program(input: bool) -> TirProgramV2 {
    let mut b = ProgramBuilder::new(8, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
    let weight = b.param("weight", DType::I8, &[4], false);
    let image = if input { Some(b.param("upstream", DType::I32, &[1, 4], false)) } else { None };
    let pre = {
        let mut p = b.block("pre", vec![]);
        let mut v = p.cast(weight, DType::I32);
        if let Some(image) = image {
            let x = p.reshape_fixed(image, &[4]);
            v = p.add(v, x, DType::I32);
        }
        p.finish(&[v])
    };
    let post = {
        let mut p = b.block("post", vec![TensorType::fixed(DType::I32, &[4])]);
        let v = p.clamp(Ref::CarryIn(0), -1024, 1024, DType::I32);
        p.commit(v);
        p.finish(&[])
    };
    let node = (b.blocks[post as usize].nodes.len() - 1) as u16;
    let v1 = b.finish(pre, vec![], post, node);
    let output = if input { OutputDecl::Final { node } } else { OutputDecl::Rows { node } };
    let inputs = if input { vec![(1, InputSource::External { lo: -1024, hi: 1024 })] } else { vec![] };
    let p = TirProgramV2::from_v1_lifting_params(&v1, &inputs, output).unwrap();
    TirProgramV2::decode_canonical(&p.encode()).unwrap()
}
struct Fixture {
    root: PathBuf,
    plan: Value,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "source-backed-complete28-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let header = serde_json::to_vec(&json!({"tensor.w":{"dtype":"I8","shape":[4],"data_offsets":[0,4]}})).unwrap();
        let mut weights = (header.len() as u64).to_le_bytes().to_vec();
        weights.extend(header);
        weights.extend([1, 2, 3, 4]);
        write(&root, "model.safetensors", &weights);
        write(&root, "config.json", b"{}");
        write(&root, "tokenizer.json", b"{}");
        let p0 = program(false);
        let p1 = program(true);
        let bindings = json!([{"param":0,"layer":null,"source":"tensor.w","import":{"kind":"integer"}}]);
        let pack = json!({"format":frontend_pack::FORMAT,"id":"independent-image-stage","scope":{"task":"image-transform","completeness":"full","components":["encoder"]},"program":Program::of_v2(&p0),"bindings":bindings});
        let pack = serde_json::to_vec(&pack).unwrap();
        write(&root, "encoder.frontend.json", &pack);
        write(&root, "decoder.tir2", &p1.encode());
        let files = [("config.json", b"{}".to_vec()), ("model.safetensors", weights), ("tokenizer.json", b"{}".to_vec())];
        let plan = json!({"schema":"misaka.palw.source-backed-complete-frontend.v28","original":{"repository":"independent/future-components","revision":"1111111111111111111111111111111111111111","advertised_task":"image-transform","task_metadata_file":"config.json","task_metadata_sha256":sha(b"{}"),
            "source_files":files.iter().map(|(name,b)|json!({"path":name,"bytes":b.len(),"sha256":sha(b)})).collect::<Vec<_>>(),
            "components":[{"name":"encoder","source_files":["config.json","model.safetensors","tokenizer.json"],"stages":[0]},{"name":"projection","source_files":["config.json","model.safetensors"],"stages":[1]}],
            "complete_tensor_inventory":["model.safetensors::tensor.w"],"original_contexts":[1,1]},
            "stages":[{"frontend_pack":"encoder.frontend.json","frontend_sha256":sha(&pack),"checkpoint":"model.safetensors","config_file":"config.json","config_pointer":"","tensor_names":["tensor.w"],"semantics":StageSemanticsV28::of_program(&p0)},
                {"checkpoint":"model.safetensors","config_pointer":"","tensor_names":["tensor.w"],"canonical_v2":"decoder.tir2","canonical_sha256":sha(&p1.encode()),"source_bindings":bindings}],
            "pipeline":{"stages":[{"name":"encoder","trip":{"kind":"fixed","n":1},"max_trip":1,"tokens":null,"bind":[]},{"name":"projection","trip":{"kind":"fixed","n":1},"max_trip":1,"tokens":null,"bind":[{"kind":"stage_rows","stage":0,"drop":0,"pad_to":1}]}],"output_stage":1},"tokenizer_file":"tokenizer.json","block_bytes":65536,"maximum_file_bytes":1048576,"tensor_workspace_bytes":1048576});
        Self { root, plan }
    }
    fn run(&self) -> Result<Value, String> {
        let raw = serde_json::to_vec(&self.plan).unwrap();
        write(&self.root, "declaration.json", &raw);
        compile_complete_frontend_v28(
            &self.root.join("declaration.json"),
            Sha256::digest(raw).into(),
            &self.root,
            &self.root.join("output"),
        )
    }
}
#[test]
fn frontend_and_direct_v2_emit_same_real_source_admission() {
    let f = Fixture::new();
    let report = f.run().unwrap();
    assert_eq!(report["source_fidelity"], "NOT_RUN");
    assert_eq!(report["native_final_or_A6_A8_credit"], false);
    let raw = std::fs::read(f.root.join("output/source-bundle.borsh")).unwrap();
    let bundle = kaspa_consensus_core::palw_flat_stage_v28::NativeStageSourceBundleV28::from_bytes(&raw).unwrap();
    let (checked, sources) = bundle.check().unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(checked.manifest().manifest().original_required_components, vec!["encoder", "projection"]);
    let artifact = misaka_palw_tir_artifact::PalwTirContainerV2::open(&f.root.join("output/complete.palwtir2")).unwrap();
    assert_eq!(artifact.programs.len(), 2);
    assert_eq!(artifact.programs[1], program(true));
    assert_eq!(std::fs::read(f.root.join("output/tensors/stage-0-param-0-layer-global.bin")).unwrap(), vec![1, 2, 3, 4]);
}
#[test]
fn whole_source_inventory_cannot_be_replaced_by_consumed_subset() {
    let mut f = Fixture::new();
    f.plan["original"]["complete_tensor_inventory"] = json!([]);
    assert!(f.run().unwrap_err().contains("ORIGINAL_TENSOR_INVENTORY_INCOMPLETE"));
    assert!(!f.root.join("output").exists());
}
#[test]
fn reduced_pipeline_cannot_claim_full_original_context() {
    let mut f = Fixture::new();
    f.plan["original"]["original_contexts"] = json!([2, 1]);
    assert!(f.run().unwrap_err().contains("PIPELINE_REDUCES_ORIGINAL_CONTEXT"));
    assert!(!f.root.join("output").exists());
}
#[test]
fn changed_frontend_pack_is_rejected_before_tensor_conversion() {
    let f = Fixture::new();
    write(&f.root, "encoder.frontend.json", b"{}");
    assert!(f.run().unwrap_err().contains("PACK_SHA_DIFFERS"));
    assert!(!f.root.join("output").exists());
}
