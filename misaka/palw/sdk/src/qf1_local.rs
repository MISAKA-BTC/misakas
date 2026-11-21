//! Node-side pins for the non-rewarding RFC-0066 fixed-scale instance.
use kaspa_consensus_core::{
    palw_qf1::{self, ROLES},
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1},
};
use kaspa_hashes::blake2b_512_keyed;
use misaka_palw_tir_exec::node::{TirArtifactV1, qf1::LocalInstance};
use misaka_palw_tir_lower::qf1::{INVENTORY, Qf1Program, fixed_scale_program};
use sha2::{Digest, Sha256};
pub const TOKENIZER: &str =
    "10ed14bc48937bd836b309447040a8d43d38f103fde39682f7f9f2c4cbb6b4b4666658ab71ed3d90de7c6e2e00cce21c01863b4403ae41dab361b099fdfe2ec5";
const INVENTORY_ROOT: &str =
    "4f049b89bc96796d6c69f30b97067082553b03c4207b270203ee28e962fd00a94ca87a4feaf533b69851880d4fbf2cc1a462245e329d45f52e2a2d104d30a7bd";
pub const MANIFEST: &str = include_str!("qf1_local/fixed-scale.json");

pub fn program() -> Result<Qf1Program, String> {
    let model = fixed_scale_program(tiled_logits_scheme_id_v1().as_bytes()).map_err(|e| e.to_string())?;
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).map_err(|e| e.to_string())?;
    let digest = format!("{:x}", Sha256::digest(model.program.encode()));
    if manifest["program_sha256"].as_str() != Some(digest.as_str()) || manifest["integer_inventory_sha256"].as_str() != Some(INVENTORY)
    {
        return Err("QF1 program or source inventory differs from conversion manifest".into());
    }
    Ok(model)
}

pub fn layout(p: &misaka_palw_tir::TirProgramV1) -> PalwTirLayoutV1 {
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: palw_qf1::CONTEXT,
        checkpoint_interval: 2,
        h_tile: 2,
        commit_tiles: p.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).map(|_| 64).collect(),
        state_tiles: vec![64; p.states.len()],
    }
}

pub fn bind(a: &TirArtifactV1) -> Result<LocalInstance<'_>, String> {
    let model = program()?;
    let class = a.class()?;
    if class.program != model.program.encode()
        || class.layout != layout(&model.program)
        || class.tokenizer_id.to_string() != TOKENIZER
        || a.header().meta != MANIFEST
    {
        return Err("file does not declare the expected fixed-scale conversion".into());
    }
    // The header alone could be copied onto different tensors.
    if a.inventory_root()?.0.to_string() != INVENTORY_ROOT {
        return Err("QF1 inventory differs from the fixed-scale conversion".into());
    }
    let manifest = blake2b_512_keyed(b"MISAKA/private/QF1/manifest/v1", MANIFEST.as_bytes());
    let mut scales = [blake2b_512_keyed(b"MISAKA/private/QF1/scale/v1", b"i16 Q15"); ROLES];
    scales[ROLES - 1] = blake2b_512_keyed(b"MISAKA/private/QF1/scale/v1", b"i16 Q10");
    LocalInstance::bind(a, manifest, model.final_norm, &scales)
}
