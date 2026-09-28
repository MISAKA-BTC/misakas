//! **The lowerer's artifact output: a `PALWTIR1` container** (RFC-0002 Phase F, F3;
//! `misaka_palw_tir_artifact`). Gate 2a's crate-local `PALWTIRA` format grew into it: the same
//! typed little-endian tensors, now in inventory order, 64-byte aligned, beside the program they
//! belong to, so the node's evaluator loads what the lowerer writes and the consensus inventory
//! hashes it front to back.
//!
//! What identifies the artifact is not this file but the inventory root
//! (`kaspa_consensus_core::palw_tir_artifact_v1`) over the program's declarations; the file digest
//! returned here is what a `.palwmanifest` binds to the file.

use crate::error::{LowerError, Result};
use crate::lower::{IntParams, IntTensor};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::{PalwTirContainerV1, write_container_v1};
use std::path::Path;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// BLAKE2b-512 of the program's canonical encoding (plain, for logs; the class's `graph_ir_root` is
/// the keyed form in consensus).
pub fn program_digest(p: &TirProgramV1) -> String {
    hex(blake2b_simd::Params::new().hash_length(64).hash(&p.encode()).as_bytes())
}

/// Write `params` for `program` as a `PALWTIR1` container; returns the file digest (hex).
/// `meta` is provenance (scales, calibration set) and enters no identity.
pub fn write(
    path: &Path,
    program: &TirProgramV1,
    params: &IntParams,
    tokenizer_id: [u8; 64],
    meta: serde_json::Value,
) -> Result<String> {
    let d = write_container_v1(path, program, Vec::new(), tokenizer_id, meta.to_string(), &mut |j, l| {
        params.tensors.get(&(j, l)).map(IntTensor::le_bytes).ok_or_else(|| format!("no tensor for param {j} layer {l:?}"))
    })
    .map_err(|e| LowerError::Io(e.to_string()))?;
    Ok(hex(&d))
}

/// Read a container back as integer params, checking it carries `program`.
pub fn read(path: &Path, program: &TirProgramV1) -> Result<(PalwTirContainerV1, IntParams)> {
    let c = PalwTirContainerV1::open(path).map_err(|e| LowerError::bad(e.to_string()))?;
    if &c.program != program {
        return Err(LowerError::bad("the container carries a different program"));
    }
    let mut params = IntParams::default();
    for e in &c.header.tensors {
        let d = &program.params[e.param as usize];
        let bytes = c.read_tensor_bytes(e.param, e.layer).map_err(|x| LowerError::bad(x.to_string()))?;
        let t = IntTensor::from_le_bytes(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), &bytes)?;
        params.tensors.insert((e.param, e.layer), t);
    }
    Ok((c, params))
}
