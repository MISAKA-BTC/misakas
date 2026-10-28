//! **`PALWART3` checks that need the consensus crate** (genesis 8, 2026-10-15).
//!
//! The light packer (`misaka_palw_tir_artifact::palwart_pack`) computes the TIR inventory root with a
//! twin of the consensus function and takes an A16 artifact's `artifact_digest` from the file's own
//! trailer. This module re-derives both with the code the chain runs:
//!
//! * TIR: `palw_tir_inventory_root_streamed_v1` (the consensus leaf and Merkle frontier) over the
//!   container's TIR section, and `palw_tir_graph_ir_root_v1` over its program;
//! * A16: `decode_artifact_file_v1` (which itself refuses a file whose recomputed digest is not the
//!   declared one) and `artifact_digest`.

use crate::tir_stream::{PalwTirRangeSourceV1, palw_tir_inventory_root_streamed_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1;
use misaka_palw_tir_artifact::palwart_pack::verify_v3;
use misaka_palw_tir_artifact::palwart_v3::{PalwArtTirParamsV3, PalwArtV3, section_kind};

/// A TIR section as the consensus streamed inventory's range source.
pub struct PalwArtTirRangesV3<'a, 'c> {
    pub params: &'a PalwArtTirParamsV3<'c>,
}

impl PalwTirRangeSourceV1 for PalwArtTirRangesV3<'_, '_> {
    fn read_range(&self, param: u16, layer: Option<u16>, range: std::ops::Range<u64>, out: &mut [u8]) -> Result<(), String> {
        self.params.read_instance_range(param, layer, range.start, out).map_err(|e| e.to_string())
    }
}

/// The light verification, then the weight section's inner root recomputed by the consensus /
/// BASE-0 code. Every segment must be present.
pub fn verify_deep_v3(a: &PalwArtV3) -> Result<serde_json::Value, String> {
    let mut out = verify_v3(a)?;
    if out["segments_missing"].as_array().is_some_and(|m| !m.is_empty()) {
        return Err("a deep verification needs every segment".into());
    }
    let Some((si, sec)) = a.weights() else { return Ok(out) };
    match sec.kind {
        section_kind::TIR_INVENTORY => {
            let program = misaka_palw_tir::TirProgramV1::decode_canonical(&a.header.program.bytes).map_err(|e| e.to_string())?;
            let params = PalwArtTirParamsV3::bind(a, &program).map_err(|e| e.to_string())?;
            let (root, leaves) = palw_tir_inventory_root_streamed_v1(&program, &PalwArtTirRangesV3 { params: &params })?;
            if root.as_bytes() != sec.inner_root || leaves as u64 != sec.inner_leaf_count {
                return Err("the consensus inventory root differs from the TIR section's inner root".into());
            }
            if palw_tir_graph_ir_root_v1(&a.header.program.bytes).as_bytes() != a.header.program.kernel_id {
                return Err("the consensus graph-IR root differs from the program's kernel id".into());
            }
            out["deep"] = serde_json::json!("ok (consensus inventory root and graph-IR root)");
        }
        section_kind::BASE0_A16 => {
            let bytes = a.read_entry(si, 0).map_err(|e| e.to_string())?;
            let artifact = misaka_palw_base0::artifact::decode_artifact_file_v1(&bytes).map_err(|e| e.to_string())?;
            if artifact.artifact_digest().as_bytes() != sec.inner_root {
                return Err("the recomputed artifact_digest differs from the A16 section's inner root".into());
            }
            out["deep"] = serde_json::json!("ok (artifact_digest recomputed by the BASE-0 decoder)");
        }
        _ => {}
    }
    Ok(out)
}
