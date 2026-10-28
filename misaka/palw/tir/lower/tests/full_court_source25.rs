#![allow(warnings)]
use misaka_palw_tir_lower::{
    full_court_source25::*,
    hf_schema::{TensorEntry, TensorIndex},
    model::{FeatureScope, SCOPE_SCHEMA_V1},
};
use sha2::{Digest, Sha256};
fn pin() -> SourcePinV25 {
    SourcePinV25 {
        schema: SOURCE_PIN_SCHEMA_V25.into(),
        repository: "publisher/model".into(),
        revision: "a".repeat(40),
        original_context_max: 32768,
        advertised_task: "text-generation".into(),
        required_components: vec!["text".into()],
        complete_tensor_inventory: vec!["weight".into()],
        source_files: vec![],
    }
}
#[test]
fn source_original_context_inventory_and_all_task_components_cannot_be_lifted_by_new_marker() {
    let p = pin();
    let index = TensorIndex { tensors: [("weight".into(), TensorEntry { dtype: "BF16".into(), shape: Some(vec![2]) })].into() };
    let scope = FeatureScope {
        schema: SCOPE_SCHEMA_V1,
        task: "text-generation".into(),
        input: "token ids".into(),
        output: "next-token logits".into(),
        text_only: false,
        excluded: vec![],
    };
    p.check_frontend(&index, &scope, Some(32768), Some(32768), None).unwrap();
    assert!(p.check_frontend(&index, &scope, Some(32768), Some(512), None).is_err());
    assert!(p.check_frontend(&index, &scope, Some(32768), Some(32768), Some(512)).is_err());
    let mut missing = p.clone();
    missing.complete_tensor_inventory.clear();
    assert!(missing.check_frontend(&index, &scope, Some(32768), Some(32768), None).is_err());
    let mut media = p.clone();
    media.required_components.push("vision".into());
    assert!(media.check_frontend(&index, &scope, Some(32768), Some(32768), None).is_err());
}
#[test]
fn original_source_file_pins_hash_whole_weights_and_refuse_unpinned_shards() {
    let dir = std::env::temp_dir().join(format!(
        "palw-source25-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let mut p = pin();
    for name in ["config.json", "tokenizer.json", "tokenizer_config.json", "model.safetensors"] {
        let b = format!("exact original {name}").into_bytes();
        std::fs::write(dir.join(name), &b).unwrap();
        p.source_files.push(SourceFilePinV25 {
            path: name.into(),
            bytes: b.len() as u64,
            sha256: Sha256::digest(&b).iter().map(|b| format!("{b:02x}")).collect(),
        });
    }
    p.verify_files(&dir).unwrap();
    std::fs::write(dir.join("unlisted.safetensors"), b"an omitted shard").unwrap();
    assert!(p.verify_files(&dir).is_err());
    std::fs::remove_file(dir.join("unlisted.safetensors")).unwrap();
    let original = std::fs::read(dir.join("model.safetensors")).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    std::fs::write(dir.join("model.safetensors"), changed).unwrap();
    assert!(p.verify_files(&dir).is_err());
    std::fs::write(dir.join("model.safetensors"), original).unwrap();
    p.verify_files(&dir).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
