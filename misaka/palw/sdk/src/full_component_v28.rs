//! Immutable whole-component pipeline writer for explicitly checked Stage28 classes.
//! No metadata-only assertion promotes an old partial pipeline into full-source eligibility.
pub mod decoder38;
pub mod frontend;
pub mod q36mirror40;
pub mod source39;
pub mod vision38;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_component_source28::CheckedComponentManifestV28,
    flat_inputs12::bounded28::CheckedStageClassV28,
    hash::{Digest, object_id},
};
use misaka_palw_tir_artifact::PalwTirContainerV2;
use sha2::{Digest as _, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
};
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct CompletePipelineLayoutV28 {
    pub version: u16,
    pub original_pipeline: Digest,
    pub component_manifest: Digest,
    pub complete_source: Digest,
    pub source_restore_bytes: u64,
    pub stage_classes: Vec<Digest>,
    pub original_maximum_scopes: Vec<Digest>,
    pub original_maximum_resources: Vec<Digest>,
    pub tokenizer: Digest,
}
impl CompletePipelineLayoutV28 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/sdk/complete-original-pipeline-layout/v28", self)
    }
}
fn hash(path: &Path, cap: u64) -> Result<[u8; 32], String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if f.metadata().map_err(|e| e.to_string())?.len() > cap {
        return Err("component28 file cap".into());
    }
    let mut h = Sha256::new();
    let mut total = 0u64;
    let mut b = [0u8; 65536];
    let mut f = f;
    loop {
        let n = f.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).ok_or("component28 extent overflow")?;
        if total > cap {
            return Err("component28 file grew beyond cap".into());
        }
        h.update(&b[..n]);
    }
    Ok(h.finalize().into())
}
/// The input must have been freshly lowered with this exact immutable source28
/// manifest and each original/lowered context. Classes must independently derive
/// their own scope25/resources29/real PC from the authenticated canonical source.
pub fn write_complete_pipeline_v28(
    input: &Path,
    expected_sha256: [u8; 32],
    output: &Path,
    original_source_directory: &Path,
    manifest: &CheckedComponentManifestV28,
    classes: &mut [CheckedStageClassV28],
    maximum_file_bytes: u64,
) -> Result<(CompletePipelineLayoutV28, [u8; 32]), String> {
    if input == output || output.exists() || classes.len() != manifest.manifest().stages.len() {
        return Err("component28 fresh output/exact stage classes required".into());
    }
    if hash(input, maximum_file_bytes)? != expected_sha256 {
        return Err("component28 input whole SHA differs".into());
    }
    let mut f = std::fs::File::open(input).map_err(|e| e.to_string())?;
    let mut prefix = [0u8; 14];
    f.read_exact(&mut prefix).map_err(|e| e.to_string())?;
    let header = u32::from_le_bytes(prefix[10..14].try_into().map_err(|_| "component28 header")?);
    if header > 4 << 20 {
        return Err("component28 header allocation cap".into());
    }
    let container = PalwTirContainerV2::open(input).map_err(|e| e.to_string())?;
    if !container.header.class.is_empty() || container.programs.len() != classes.len() {
        return Err("component28 requires newly lowered undeclared complete pipeline".into());
    }
    let mut meta = misaka_palw_tir_artifact::metadata_v16::parse_metadata_v16(
        &container.header.meta,
        misaka_palw_tir_artifact::metadata_v16::MetadataParseProfileV16::CorrectlyRoundedV16,
    )?;
    let marker = meta.get("fresh_component_conversion28").ok_or("component28 old unmarked partial pipeline cannot be promoted")?;
    let hex = crate::runtime_pack::commit::hex;
    if marker.get("schema").and_then(|v| v.as_str()) != Some("misaka.palw.fresh-complete-component-conversion.v28")
        || marker.get("component_manifest_root").and_then(|v| v.as_str()) != Some(hex(&manifest.root()).as_str())
    {
        return Err("component28 freshly converted original manifest differs".into());
    }
    let root = original_source_directory.canonicalize().map_err(|e| e.to_string())?;
    for pin in &manifest.manifest().source_files {
        let path = root.join(&pin.path).canonicalize().map_err(|e| e.to_string())?;
        if !path.starts_with(&root)
            || path.metadata().map_err(|e| e.to_string())?.len() != pin.bytes
            || hash(&path, pin.bytes)? != pin.sha256
        {
            return Err("component28 original source payload differs".into());
        }
    }
    let sources = classes
        .iter()
        .map(|c| {
            misaka_palw_kernel::flat_inputs12::source28::FlatStageSourceWireV28 {
                version: 28,
                program_bytes: c.source().program().encode(),
                inventory_root: c.source().inventory_root(),
                source_parameters: c.source().parameters().clone(),
            }
            .check()
        })
        .collect::<Result<Vec<_>, String>>()?;
    let bundle = misaka_palw_kernel::flat_inputs12::bounded28::check_stage_bundle_v28(manifest.manifest(), &sources)?;
    for class in classes.iter_mut() {
        class.bind_complete_bundle(&bundle)?;
    }
    for (i, class) in classes.iter().enumerate() {
        class.check_component_manifest(manifest)?;
        if class.wire().stage as usize != i || class.source().program() != &container.programs[i] {
            return Err("component28 original stage/source ordering differs".into());
        }
        struct StageRanges<'a> {
            container: &'a PalwTirContainerV2,
            stage: u16,
            file: std::fs::File,
        }
        impl crate::tir_stream::PalwTirRangeSourceV1 for StageRanges<'_> {
            fn read_range(&self, j: u16, l: Option<u16>, r: std::ops::Range<u64>, out: &mut [u8]) -> Result<(), String> {
                let (offset, len) =
                    self.container.program(self.stage).and_then(|p| p.locate(j, l)).ok_or("component28 missing real source tensor")?;
                if r.start > r.end || r.end > len || r.end - r.start != out.len() as u64 {
                    return Err("component28 source byte range".into());
                }
                crate::tir_stream::pread(&self.file, out, offset.checked_add(r.start).ok_or("component28 byte offset overflow")?)
                    .map_err(|e| e.to_string())
            }
        }
        let ranges =
            StageRanges { container: &container, stage: i as u16, file: std::fs::File::open(input).map_err(|e| e.to_string())? };
        let original = &container.programs[i];
        let view = original.v1_view();
        let inventory = kaspa_consensus_core::palw_tir_artifact_v1::PalwTirModelInventoryV2::new_stage14(&view, original)?;
        let (actual_r, _) = crate::tir_stream::palw_tir_model_inventory_root_streamed_v2(&inventory, &ranges)?;
        if actual_r.as_bytes() != class.source().inventory_root() {
            return Err("component28 actual source inventory differs from authenticated stage R".into());
        }
        let mut actual_pc = misaka_palw_kernel::trace::ParamCommitmentsV1::default();
        let param_view = misaka_palw_tir_artifact::v2::param_view_v2(original);
        let instances = misaka_palw_tir_artifact::param_instances_v1(&param_view);
        for (j, p) in original.params.iter().enumerate() {
            let shape = p.shape.iter().map(|v| *v as usize).collect::<Vec<_>>();
            // Real source PC excludes every appended public/external job parameter.
            for l in &instances[j] {
                let (root, _) =
                    misaka_palw_kernel::merkle3_stream::tensor_commitment_streamed_v3(p.dtype, &shape, 256 << 20, &mut |at, out| {
                        crate::tir_stream::PalwTirRangeSourceV1::read_range(&ranges, j as u16, *l, at..at + out.len() as u64, out)
                    })?;
                actual_pc.by_instance.insert((j as u16, *l), root);
            }
        }
        if &actual_pc != class.source().parameters() {
            return Err("component28 actual real PC differs from authenticated source; job placeholders cannot substitute".into());
        }
        let pin = &manifest.manifest().stages[i];
        if class.maximum_scope().max_positions() != pin.original_context_max {
            return Err("component28 lowered context does not cover source maximum".into());
        }
    }
    let layout = CompletePipelineLayoutV28 {
        version: 28,
        original_pipeline: object_id(b"misaka-palw/sdk/original-complete-pipeline/v28", &container.pipeline),
        component_manifest: manifest.root(),
        complete_source: bundle.root(),
        source_restore_bytes: bundle.bytes(),
        stage_classes: classes.iter().map(|c| c.root()).collect(),
        original_maximum_scopes: classes.iter().map(|c| c.maximum_scope().root()).collect(),
        original_maximum_resources: classes.iter().map(|c| c.maximum_resources().root()).collect(),
        tokenizer: container.header.tokenizer_id,
    };
    meta["complete_pipeline_declaration28"] = serde_json::json!({"schema":"misaka.palw.complete-pipeline-declaration.v28",
        "layout_root":hex(&layout.root()),"manifest_root":hex(&manifest.root()),"source_fidelity_or_native_final":false});
    misaka_palw_tir_artifact::metadata_v16::record_metadata_profile_v16(
        &mut meta,
        misaka_palw_tir_artifact::metadata_v16::MetadataParseProfileV16::CorrectlyRoundedV16,
    )?;
    let mut tensor_file = std::fs::File::open(input).map_err(|e| e.to_string())?;
    misaka_palw_tir_artifact::v2::write_container_v2_streamed_v28(
        output,
        &container.pipeline,
        &container.programs,
        borsh::to_vec(&layout).map_err(|e| e.to_string())?,
        container.header.tokenizer_id,
        meta.to_string(),
        maximum_file_bytes,
        4 << 20,
        &mut |k, j, l, out| {
            let (offset, length) = container.program(k).and_then(|p| p.locate(j, l)).ok_or("component28 missing original tensor")?;
            tensor_file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
            let mut remaining = length;
            let mut b = [0u8; 65536];
            while remaining > 0 {
                let n = remaining.min(b.len() as u64) as usize;
                tensor_file.read_exact(&mut b[..n]).map_err(|e| e.to_string())?;
                out.write_all(&b[..n]).map_err(|e| e.to_string())?;
                remaining -= n as u64;
            }
            Ok(())
        },
    )
    .map_err(|e| e.to_string())?;
    if hash(input, maximum_file_bytes)? != expected_sha256 {
        return Err("component28 input mutated during declaration".into());
    }
    Ok((layout, hash(output, maximum_file_bytes)?))
}

/// New pipeline-conversion producer API: it accepts complete checked original
/// sources and contexts, never an existing partial artifact. Tensor production
/// remains the frontend's bounded stream; the declaration writer independently
/// rehashes every original source PC/R before forming a native class artifact.
pub fn produce_fresh_complete_pipeline_v28(
    output: &Path,
    original_source_directory: &Path,
    pipeline: &misaka_palw_tir::pipeline::TirPipelineV1,
    programs: &[misaka_palw_tir::program_v2::TirProgramV2],
    tokenizer: Digest,
    mut metadata: serde_json::Value,
    manifest: &CheckedComponentManifestV28,
    sources: &[misaka_palw_kernel::flat_inputs12::source28::CheckedStageSourceV28],
    maximum_file_bytes: u64,
    tensor: &mut dyn FnMut(u16, u16, Option<u16>, &mut dyn std::io::Write) -> Result<(), String>,
) -> Result<[u8; 32], String> {
    let bundle = misaka_palw_kernel::flat_inputs12::bounded28::check_stage_bundle_v28(manifest.manifest(), sources)?;
    let directory = original_source_directory.canonicalize().map_err(|e| e.to_string())?;
    for pin in &manifest.manifest().source_files {
        let file = directory.join(&pin.path).canonicalize().map_err(|e| e.to_string())?;
        if !file.starts_with(&directory)
            || file.metadata().map_err(|e| e.to_string())?.len() != pin.bytes
            || hash(&file, pin.bytes)? != pin.sha256
        {
            return Err("fresh28 original pinned source file differs before conversion marker".into());
        }
    }

    if sources.len() != programs.len() || sources.iter().zip(programs).any(|(s, p)| s.program() != p) {
        return Err("fresh28 programs differ from complete original checked sources".into());
    }
    if !metadata.is_object() {
        return Err("fresh28 provenance must be an explicit object".into());
    }
    metadata["fresh_component_conversion28"] = serde_json::json!({"schema":"misaka.palw.fresh-complete-component-conversion.v28",
        "component_manifest_root":crate::runtime_pack::commit::hex(&manifest.root()),
        "complete_source_bundle":crate::runtime_pack::commit::hex(&bundle.root()),
        "original_lowered_contexts":manifest.manifest().stages.iter().map(|s|s.original_context_max).collect::<Vec<_>>(),
        "source_fidelity_or_native_final":false});
    misaka_palw_tir_artifact::metadata_v16::record_metadata_profile_v16(
        &mut metadata,
        misaka_palw_tir_artifact::metadata_v16::MetadataParseProfileV16::CorrectlyRoundedV16,
    )?;
    misaka_palw_tir_artifact::v2::write_container_v2_streamed_v28(
        output,
        pipeline,
        programs,
        vec![],
        tokenizer,
        metadata.to_string(),
        maximum_file_bytes,
        4 << 20,
        tensor,
    )
    .map_err(|e| e.to_string())?;
    hash(output, maximum_file_bytes)
}
