//! Generic actual-artifact binding diagnostic. Authenticate every v3 parameter
//! from bounded raw reads, then change one element of a small tensor and produce
//! two public openings for the dormant node court. No model identity is consulted.
use kaspa_consensus_core::{
    Hash64,
    palw_onboarding_v1::{ArtifactMismatchProofV1, ArtifactTileOpeningV3, verify_artifact_mismatch_v1},
};
use misaka_palw_kernel::{
    descriptor::k2_tir_v4_descriptor,
    merkle::AXIS_ROW,
    merkle3::{LeafOpeningV3, TILE_V3, tensor_commitment_v3},
    trace::ParamCommitmentsV1,
};
use misaka_palw_sdk::tir_stream::{ArtifactTileCourtBundleV3, ContainerRanges, palw_tir_open_leaf_streamed_v1};
use misaka_palw_tir::Tensor;
use std::{io::Read, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() != 5 {
        return Err(
            "usage: prepare-artifact-tile-court <artifact> <params.borsh> <new-bundle.borsh> <hash-workspace MiB> <payload MiB>"
                .into(),
        );
    }
    let mib = |s: &str| -> Result<u64, Box<dyn std::error::Error>> {
        Ok(s.parse::<u64>()?.checked_mul(1 << 20).filter(|n| *n > 0).ok_or("invalid MiB cap")?)
    };
    let path = Path::new(&a[0]);
    let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(path)?;
    let scheme = Hash64::from_bytes(c.program.logits_scheme_id);
    if scheme != kaspa_consensus_core::palw_step_refute::flat_logits_scheme_id_v1()
        && scheme != kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1()
    {
        return Err("the source has no adjudicable logits scheme; use declare-layout before producing a V2 binding witness".into());
    }
    let class = kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1 {
        version: 1,
        program: c.header.program.clone(),
        layout: borsh::from_slice(&c.header.layout)?,
        tokenizer_id: Hash64::from_bytes(c.header.tokenizer_id),
    };
    if class.program != c.program.encode() {
        return Err("the container's program is not canonical".into());
    }

    let started = std::time::Instant::now();
    let prepared = misaka_palw_sdk::kernel_params::prepare_kernel_params_file_v3(
        &k2_tir_v4_descriptor(),
        path,
        mib(&a[3])?,
        mib(&a[4])?,
        &mut |_, _, _| {},
    )?;
    let mut wire = Vec::new();
    std::fs::File::open(&a[1])?.take((64 << 20) + 1).read_to_end(&mut wire)?;
    if wire.len() > 64 << 20 {
        return Err("prepared map exceeds 64 MiB parse limit".into());
    }
    let params: ParamCommitmentsV1 = borsh::from_slice(&wire)?;
    if params != prepared.params {
        return Err("the actual artifact differs from the prepared parameter map".into());
    }

    let (&(param, layer), _) = params
        .by_instance
        .iter()
        .find(|((j, _), _)| {
            let d = &c.program.params[*j as usize];
            d.shape.iter().try_fold(1u64, |n, v| n.checked_mul(*v as u64)).is_some_and(|n| n > 0 && n <= TILE_V3)
        })
        .ok_or("no bounded whole tensor for this diagnostic")?;
    let d = &c.program.params[param as usize];
    let shape: Vec<usize> = d.shape.iter().map(|v| *v as usize).collect();
    let true_tensor = Tensor::from_le_bytes(d.dtype, &shape, &c.read_tensor_bytes(param, layer)?)?;
    let true_leaf = LeafOpeningV3::of(&true_tensor, AXIS_ROW, 0, 0).ok_or("no row tile")?;
    if !true_leaf.authenticates(&params.by_instance[&(param, layer)]) {
        return Err("the selected tensor no longer authenticates".into());
    }
    let index =
        kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_leaf_index_v1(&c.program, param, layer, 0).ok_or("no inventory leaf")?;
    let (inventory_root, v2_opening) = palw_tir_open_leaf_streamed_v1(&c.program, &ContainerRanges::open(&c)?, index)?;
    let honest = ArtifactMismatchProofV1::TileV3 {
        commitments: params.clone(),
        param,
        layer,
        kernel_tile: ArtifactTileOpeningV3::new(true_leaf)?,
        v2_opening: v2_opening.clone(),
    };
    if verify_artifact_mismatch_v1(&c.program, inventory_root, Hash64::from_bytes(params.root()), &honest).is_ok() {
        return Err("an honest binding was falsely convicted".into());
    }
    let mut wrong_tensor = true_tensor;
    wrong_tensor.data[0] = if wrong_tensor.data[0] == 0 { 1 } else { 0 };
    let mut wrong_params = params.clone();
    wrong_params.by_instance.insert((param, layer), tensor_commitment_v3(&wrong_tensor));
    let false_root = wrong_params.root();
    let false_binding = ArtifactMismatchProofV1::TileV3 {
        commitments: wrong_params,
        param,
        layer,
        kernel_tile: ArtifactTileOpeningV3::new(LeafOpeningV3::of(&wrong_tensor, AXIS_ROW, 0, 0).ok_or("no wrong tile")?)?,
        v2_opening,
    };
    verify_artifact_mismatch_v1(&c.program, inventory_root, Hash64::from_bytes(false_root), &false_binding)?;
    let bundle = ArtifactTileCourtBundleV3 {
        version: 1,
        program_bytes: c.program.encode(),
        class,
        inventory_root,
        params,
        honest,
        false_binding,
    };
    let bytes = borsh::to_vec(&bundle)?;
    // The diagnostic produces a new artifact; never overwrite an existing evidence file.
    use std::io::Write;
    std::fs::OpenOptions::new().write(true).create_new(true).open(&a[2])?.write_all(&bytes)?;
    let hex = misaka_palw_sdk::runtime_pack::commit::hex;
    println!(
        "{}",
        serde_json::json!({
            "schema": "misaka.palw.artifact-tile-court.v3", "param": param, "layer": layer,
            "inventory_root": inventory_root.to_string(), "true_kernel_root": hex(&bundle.params.root()), "false_kernel_root": hex(&false_root),
            "instances_authenticated": bundle.params.by_instance.len(), "honest_proves_no_fault": true, "false_binding_proven": true,
            "bundle_bytes": bytes.len(), "honest_filing_bytes": borsh::to_vec(&bundle.honest)?.len(), "false_filing_bytes": borsh::to_vec(&bundle.false_binding)?.len(),
            "elapsed_ms": started.elapsed().as_millis(), "conformance_fidelity_activation_or_final_proven": false
        })
    );
    Ok(())
}
