//! Producer/requester-side request composer for owning39 (RFC-0002 §II.14).
//!
//! It calls the kernel's pure `compose_component_request_v39` (the exact
//! inverse of the native `check_component_invocation_v39`) and turns each
//! request-derived public input into a raw material file plus its
//! `MaterialSeedV39`, so `CheckedMaterialFileV39::open` produces the descriptor
//! the native rule expects. Images and upstream outputs are NOT synthesized:
//! an image's pixels come from the requester's own raw source matching the
//! request commitment, and an upstream input comes from
//! `prepare_upstream_material_v39` over a genuinely Final component. A stage
//! whose inputs are not yet available is refused by the kernel composer.
use crate::native_material_v39::MaterialSeedV39;
use misaka_palw_kernel::{
    flat_invocation39::{
        CheckedComponentInvocationV39, ComposedComponentV39, ComposedInputV39, MasterPublicRequestV39, PriorComponentV39,
        compose_and_check_component_v39, compose_component_request_v39,
    },
    flat_job39::{CheckedJobMaximumV39, JobMaximumV39, MaterialManifestV39},
    flat_material39::MaterialDescriptorV39,
    flat_pipeline39::CheckedPipelineV39,
    flat_source39::CheckedSourceV39,
    hash::Digest,
};
use sha2::{Digest as _, Sha256};
use std::{io::Write, path::Path};

/// One request-derived public input ready for `CheckedMaterialFileV39::open`.
pub struct ComposedPublicMaterialV39 {
    pub input: u8,
    pub seed: MaterialSeedV39,
    pub raw: Vec<u8>,
    pub sha256: [u8; 32],
}
/// The inputs a composed stage still needs from outside the request.
#[derive(Debug, serde::Serialize)]
pub struct ComposedNeedsV39 {
    pub images: Vec<(u8, String, String)>,
    pub upstream: Vec<(u8, u16)>,
}

pub fn compose_stage_v39(
    master: Digest,
    request: &MasterPublicRequestV39,
    pipeline: &CheckedPipelineV39,
    source: &CheckedSourceV39,
    stage: u16,
    prior: &[PriorComponentV39<'_>],
) -> Result<ComposedComponentV39, String> {
    compose_component_request_v39(master, request, pipeline, source, stage, prior)
}

/// Encode every public input's exact elements (little-endian at the declared
/// width) with the seed that names its original principal, nonce and origin.
pub fn public_materials_v39(
    composed: &ComposedComponentV39,
    request: &MasterPublicRequestV39,
) -> Result<Vec<ComposedPublicMaterialV39>, String> {
    let mut out = Vec::new();
    for c in &composed.inputs {
        let ComposedInputV39::Public { input, dtype, shape, values, .. } = c else { continue };
        let cells = shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d as usize)).ok_or("compose39 public tensor product")?;
        if values.len() != cells.max(1) {
            return Err("compose39 public values do not fill the declared shape".into());
        }
        let mut raw = Vec::with_capacity(values.len() * dtype.width());
        for v in values {
            dtype.encode_le(*v, &mut raw);
        }
        let seed = MaterialSeedV39 {
            version: 39,
            source: composed.source,
            master: composed.master,
            requester: composed.requester,
            requester_operator: composed.requester_operator,
            nonce: composed.nonce,
            input: *input,
            dtype: *dtype,
            shape: shape.clone(),
            origin: composed.public_origin(request, *input),
        };
        let sha256 = <[u8; 32]>::from(Sha256::digest(&raw));
        out.push(ComposedPublicMaterialV39 { input: *input, seed, raw, sha256 });
    }
    Ok(out)
}
/// What the requester still has to supply (never invented here).
pub fn composed_needs_v39(composed: &ComposedComponentV39) -> ComposedNeedsV39 {
    let hex = crate::runtime_pack::commit::hex;
    let mut needs = ComposedNeedsV39 { images: vec![], upstream: vec![] };
    for c in &composed.inputs {
        match c {
            ComposedInputV39::Image { input, tensor, raw_source, .. } => needs.images.push((*input, hex(tensor), hex(raw_source))),
            ComposedInputV39::Upstream { input, from, .. } => needs.upstream.push((*input, *from)),
            _ => {}
        }
    }
    needs
}
/// Write public material raw files `input-<n>.raw` and seeds `input-<n>.seed39`
/// into a fresh directory; returns their SHA-256 pins for the material open.
pub fn write_public_materials_v39(materials: &[ComposedPublicMaterialV39], out: &Path) -> Result<Vec<(u8, String)>, String> {
    if out.exists() {
        return Err("compose39 fresh output directory required".into());
    }
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    let mut pins = Vec::with_capacity(materials.len());
    for m in materials {
        for (name, bytes) in [
            (format!("input-{}.raw", m.input), m.raw.clone()),
            (format!("input-{}.seed39", m.input), borsh::to_vec(&m.seed).map_err(|e| e.to_string())?),
        ] {
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(out.join(name)).map_err(|e| e.to_string())?;
            f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
        }
        pins.push((m.input, crate::runtime_pack::commit::hex(&m.sha256)));
    }
    Ok(pins)
}
/// Bind descriptors in declared input order and return the maximum/manifest
/// bytes for `NativeConsentWireV39 { maximum, material }`, after the same
/// native rule accepted them.
pub fn finish_stage_v39(
    master: Digest,
    request: &MasterPublicRequestV39,
    pipeline: &CheckedPipelineV39,
    source: &CheckedSourceV39,
    stage: u16,
    prior: &[PriorComponentV39<'_>],
    descriptors: &[MaterialDescriptorV39],
) -> Result<(CheckedJobMaximumV39, CheckedComponentInvocationV39, Vec<u8>, Vec<u8>), String> {
    let (checked, invocation) = compose_and_check_component_v39(master, request, pipeline, source, stage, prior, descriptors)?;
    let maximum: &JobMaximumV39 = checked.request();
    let manifest: &MaterialManifestV39 = checked.input_manifest();
    let (max_bytes, material_bytes) = (maximum.to_bytes(), manifest.to_bytes());
    Ok((checked, invocation, max_bytes, material_bytes))
}
