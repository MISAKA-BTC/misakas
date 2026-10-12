//! Attach an already-declared class without converting or choosing its layout again.
//! This records reproducible identity only; registration must still use live preflight.

use super::manifest::{DeclaredClass, PACK_FILE, RuntimePackV1};
use crate::tir_manifest::PalwTirManifestV1;
use std::path::Path;

/// Preserve the independent frontend recipe and bind an existing class through the same exact
/// identity rules. No model-name registry or synthetic ModelSpec record is introduced.
pub fn bind_frontend_class(
    pack_dir: &Path,
    artifact: &Path,
    network: &str,
    out: &Path,
) -> Result<super::primitive::PrimitiveRuntimePackV1, String> {
    use super::primitive::{FRONTEND_FILE, PACK_FILE, PrimitiveRuntimePackV1};
    super::build::network(network)?;
    if out.exists() {
        return Err(format!("{} already exists; use a new pack directory", out.display()));
    }
    let mut pack = PrimitiveRuntimePackV1::read(pack_dir)?;
    pack.check_recipe(pack_dir)?;
    let art = misaka_palw_tir_exec::node::TirArtifactV1::open(artifact)?;
    if art.composite_ref().is_some() {
        return Err("binding a composite requires its parent; this command supports standalone classes only".into());
    }
    let class = art.class()?;
    let m = PalwTirManifestV1::derive_streamed(artifact)?;
    if pack.artifact["inventory_root"] != m.inventory_root.to_string()
        || pack.artifact["tokenizer_id"] != super::manifest::hex(&m.tokenizer_id)
    {
        return Err("the declared artifact's inventory/tokenizer is not the pack's".into());
    }
    let mut p = class.decode_program().map_err(|e| e.to_string())?;
    p.logits_scheme_id = [0; 64];
    let unschemed = kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&p.encode()).to_string();
    if pack.artifact["graph_ir_root"] != m.graph_ir_root.to_string() && pack.artifact["graph_ir_root"] != unschemed {
        return Err("the declared program differs from the frontend pack beyond its logits scheme".into());
    }
    let d = DeclaredClass::from_class(network.into(), &class, &m.inventory_root, super::manifest::hex(&m.artifact_digest))?;
    pack.declared.retain(|old| old.class_id != d.class_id || old.network != d.network);
    pack.declared.push(d);
    if pack.declared.len() > 256 {
        return Err("FRONTEND_PACK_SCHEMA: source/class count".into());
    }
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    std::fs::copy(pack_dir.join(FRONTEND_FILE), out.join(FRONTEND_FILE)).map_err(|e| e.to_string())?;
    if let Some(record) = &pack.fidelity {
        for f in &record.files {
            std::fs::copy(pack_dir.join(&f.path), out.join(&f.path)).map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(out.join(PACK_FILE), serde_json::to_vec_pretty(&pack).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(pack)
}

pub fn bind_class(pack_dir: &Path, artifact: &Path, network: &str, out: &Path) -> Result<RuntimePackV1, String> {
    super::build::network(network)?;
    if out.exists() {
        return Err(format!("{} already exists; use a new pack directory", out.display()));
    }
    let mut pack = RuntimePackV1::parse(&std::fs::read_to_string(pack_dir.join(PACK_FILE)).map_err(|e| e.to_string())?)?;
    let art = misaka_palw_tir_exec::node::TirArtifactV1::open(artifact)?;
    if art.composite_ref().is_some() {
        return Err("binding a composite requires its parent; this command supports standalone classes only".into());
    }
    let class = art.class()?;
    let m = PalwTirManifestV1::derive_streamed(artifact)?;
    if m.inventory_root.to_string() != pack.result.inventory_root || super::manifest::hex(&m.tokenizer_id) != pack.result.tokenizer_id
    {
        return Err("the declared artifact's inventory/tokenizer is not the pack's".into());
    }
    // Declaration may change the logits scheme, never any computation or weight declaration.
    let mut p = class.decode_program().map_err(|e| e.to_string())?;
    p.logits_scheme_id = [0; 64];
    let unschemed = kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&p.encode()).to_string();
    if m.graph_ir_root.to_string() != pack.result.graph_ir_root && unschemed != pack.result.graph_ir_root {
        return Err("the declared program differs from the pack's base program beyond its logits scheme".into());
    }
    let d = DeclaredClass::from_class(network.into(), &class, &m.inventory_root, super::manifest::hex(&m.artifact_digest))?;
    pack.declared.retain(|old| old.class_id != d.class_id || old.network != d.network);
    pack.declared.push(d);
    pack.check_shape()?;
    // Validate every sidecar before creating output; no trust in a cache or copied report.
    for f in &pack.files {
        let bytes = std::fs::read(pack_dir.join(&f.path)).map_err(|e| format!("{}: {e}", f.path))?;
        if bytes.len() as u64 != f.bytes || super::manifest::blake2b256_hex(&bytes) != f.blake2b256 {
            return Err(format!("sidecar {} differs from its pinned hash", f.path));
        }
    }
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    for f in &pack.files {
        let target = out.join(&f.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::copy(pack_dir.join(&f.path), target).map_err(|e| e.to_string())?;
    }
    std::fs::write(out.join(PACK_FILE), pack.to_pretty()).map_err(|e| e.to_string())?;
    Ok(pack)
}
