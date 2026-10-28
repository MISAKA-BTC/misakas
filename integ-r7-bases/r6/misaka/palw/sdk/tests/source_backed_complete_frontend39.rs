//! Staged source39 regression sources. Run only during the authorized validation phase.
use misaka_palw_sdk::full_component_v28::source39::compile_complete_frontend_v39;
use misaka_palw_tir::{
    DType, Ref, TensorType,
    builder::ProgramBuilder,
    program_v2::{InputSource, OutputDecl, TirProgramV2},
};
use misaka_palw_tir_lower::frontend_pack::{program::hex, source39::FORMAT_V39};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn sha(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}
fn write(root: &Path, name: &str, b: &[u8]) {
    std::fs::write(root.join(name), b).unwrap();
}
fn program(input: bool) -> misaka_palw_tir::program_v39::TirProgramV39 {
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
    use misaka_palw_tir::program_v39::*;
    let p = TirProgramV39 {
        version: 39,
        profile: SourceProfileV39::full_source(),
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: p.token_bound,
        history_bound: p.history_bound,
        inputs: p.inputs,
        params: p.params,
        consts: p.consts,
        states: p.states,
        blocks: p
            .blocks
            .iter()
            .map(|b| BlockV39 {
                name: b.name.clone(),
                carry_in: b.carry_in.iter().map(TensorTypeV39::from_frozen).collect(),
                nodes: b.nodes.iter().map(NodeV39::from_frozen).collect(),
                carry_out: b.carry_out.clone(),
            })
            .collect(),
        schedule: p.schedule,
        output: p.output,
    };
    TirProgramV39::decode_canonical(&p.encode()).unwrap()
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
        let root =
            std::env::temp_dir().join(format!("source-backed39-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
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
        let pack=serde_json::to_vec(&json!({"format":FORMAT_V39,"id":"public-owning39","scope":{"task":"image-transform","completeness":"full","components":["encoder"]},"program":hex(&p0.encode()),"bindings":[{"param":0,"layer":null,"source":"tensor.w","import":{"kind":"integer"}}]})).unwrap();
        write(&root, "encoder.frontend39.json", &pack);
        write(&root, "decoder.tir39", &p1.encode());
        let files = [("config.json", b"{}".to_vec()), ("model.safetensors", weights), ("tokenizer.json", b"{}".to_vec())];
        let plan = json!({"schema":"misaka.palw.source-backed-complete-frontend.v39","original":{"repository":"independent/future-components","revision":"1111111111111111111111111111111111111111","advertised_task":"image-transform","task_metadata_file":"config.json","task_metadata_sha256":sha(b"{}"),"source_files":files.iter().map(|(n,b)|json!({"path":n,"bytes":b.len(),"sha256":sha(b)})).collect::<Vec<_>>(),"components":[{"name":"encoder","source_files":["config.json","model.safetensors","tokenizer.json"],"stages":[0]},{"name":"projection","source_files":["config.json","model.safetensors"],"stages":[1]}],"complete_tensor_inventory":["model.safetensors::tensor.w"],"original_contexts":[1,1]},"stages":[{"checkpoint":"model.safetensors","config_file":"config.json","tensor_names":["tensor.w"],"compiler":{"kind":"frontend_pack","pack_file":"encoder.frontend39.json","pack_sha256":sha(&pack)}},{"checkpoint":"model.safetensors","config_file":"","tensor_names":["tensor.w"],"compiler":{"kind":"canonical","program_file":"decoder.tir39","program_sha256":sha(&p1.encode()),"imports":[{"parameter":0,"layer":null,"source":"tensor.w"}]}}],"pipeline":{"stages":[{"name":"encoder","trip":{"kind":"fixed","n":1},"max_trip":1,"tokens":null,"bind":[]},{"name":"projection","trip":{"kind":"fixed","n":1},"max_trip":1,"tokens":null,"bind":[{"kind":"stage_rows","stage":0,"drop":0,"pad_to":1}]}],"output_stage":1},"tokenizer_file":"tokenizer.json","block_bytes":65536,"maximum_output_bytes":1048576,"commitment_workspace_bytes":1048576});
        Self { root, plan }
    }
    fn run(&self) -> Result<Value, String> {
        let raw = serde_json::to_vec(&self.plan).unwrap();
        write(&self.root, "declaration.json", &raw);
        compile_complete_frontend_v39(
            &self.root.join("declaration.json"),
            Sha256::digest(raw).into(),
            &self.root,
            &self.root.join("output"),
        )
    }
}
#[test]
fn actual_frontend_and_public_canonical39_share_complete_admission() {
    let f = Fixture::new();
    let report = f.run().unwrap();
    assert_eq!(report["source_fidelity"], "NOT_RUN");
    assert_eq!(report["a6_eligible"], false);
    assert_eq!(report["native_final"], "NOT_RUN");
    let bytes = std::fs::read(f.root.join("output/complete-source39.borsh")).unwrap();
    let b = misaka_palw_kernel::flat_component_source39::CompleteSourceBundleV39::from_bytes(&bytes).unwrap();
    let checked = b.check().unwrap();
    assert_eq!(checked.sources().len(), 2);
    assert_eq!(checked.manifest().manifest().original_required_components, vec!["encoder", "projection"]);
    assert_eq!(std::fs::read(f.root.join("output/parameters/stage-0-param-0-layer-global.bin")).unwrap(), vec![1, 2, 3, 4]);
    assert!(!f.root.join("output/original-stream-audit").exists());
    let mut forged = b.clone();
    let mut source = misaka_palw_kernel::flat_source39::FlatSourceWireV39::from_bytes(&forged.sources[0]).unwrap();
    *source.real_parameters.by_instance.get_mut(&(0, None)).unwrap() = [9; 64];
    forged.sources[0] = source.to_bytes();
    assert!(forged.check().is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(misaka_palw_kernel::flat_component_source39::CompleteSourceBundleV39::from_bytes(&trailing).is_err());
}
#[test]
fn full_original_context_cannot_be_claimed_on_short_fixed_run() {
    let mut f = Fixture::new();
    f.plan["original"]["original_contexts"] = json!([2, 1]);
    assert!(f.run().unwrap_err().contains("ORIGINAL_FULL_CONTEXT_NOT_LOWERED"));
    assert!(!f.root.join("output").exists());
}
#[test]
fn missing_original_tensor_and_changed_frontend_are_refused() {
    let mut f = Fixture::new();
    f.plan["original"]["complete_tensor_inventory"] = json!([]);
    assert!(f.run().unwrap_err().contains("COMPLETE_ORIGINAL_TENSOR_INVENTORY_DIFFERS"));
    assert!(!f.root.join("output").exists());
    let f = Fixture::new();
    write(&f.root, "encoder.frontend39.json", b"{}");
    assert!(f.run().unwrap_err().contains("PUBLIC_FRONTEND_SHA_DIFFERS"));
}
#[test]
fn missing_component_and_forged_pipeline_recipe_do_not_mint_complete_source() {
    let f = Fixture::new();
    f.run().unwrap();
    use misaka_palw_kernel::flat_component_source39::*;
    let mut b = CompleteSourceBundleV39::from_bytes(&std::fs::read(f.root.join("output/complete-source39.borsh")).unwrap()).unwrap();
    let mut m = CompleteComponentManifestV39::from_bytes(&b.manifest).unwrap();
    m.components.remove(0);
    b.manifest = m.to_bytes().unwrap();
    assert!(b.check().is_err());
}
