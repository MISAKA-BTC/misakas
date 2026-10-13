//! Explicit v25 public-court preparation from an independently pinned artifact.
//! A geometry quote grants no checkpoint fidelity, active registration or Final.
//! Complete-source eligibility additionally checks the ORIGINAL source context;
//! increasing a scope argument never increases the artifact's declared context.
use borsh::BorshDeserialize;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use misaka_palw_kernel::{
    flat_scope25::{CheckedResourceScopeV25, derive_checked_flat_resource_scope_v25},
    seg_encoder::EncoderBindingV1,
};
use misaka_palw_tir_artifact::PalwTirContainerV1;
use sha2::{Digest as _, Sha256};
use std::{io::Read, path::Path};

pub struct PreparedArtifactCourtV25 {
    pub artifact_sha256: [u8; 32],
    pub program: misaka_palw_tir::TirProgramV1,
    pub declared_context_max: u32,
    pub court: CheckedResourceScopeV25,
}
fn hash_bounded(path: &Path, maximum: u64) -> Result<[u8; 32], String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if f.metadata().map_err(|e| e.to_string())?.len() > maximum {
        return Err("v25 artifact exceeds explicit file limit".into());
    }
    let mut total = 0u64;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 1 << 16];
    loop {
        let n = f.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).ok_or("v25 artifact length overflow")?;
        if total > maximum {
            return Err("v25 artifact grew beyond explicit file limit".into());
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.finalize().into())
}
/// The explicit file cap is a tooling budget, independent of chain admission.
/// Full bytes are pinned before header decode and rechecked before return.
pub fn prepare_artifact_court_v25(
    path: &Path,
    expected_sha256: [u8; 32],
    maximum_file_bytes: u64,
    positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<PreparedArtifactCourtV25, String> {
    if hash_bounded(path, maximum_file_bytes)? != expected_sha256 {
        return Err("v25 artifact whole-file SHA256 differs".into());
    }
    let container = PalwTirContainerV1::open_bounded(path, maximum_file_bytes, 4 << 20).map_err(|e| e.to_string())?;
    let (declared_context_max, full25) = if container.header.layout.starts_with(&25u16.to_le_bytes()) {
        let full = misaka_palw_kernel::flat_scope25::FullCourtLayoutV25::try_from_slice(&container.header.layout)
            .map_err(|_| "malformed fixed full25 artifact layout")?;
        let maximum = derive_checked_flat_resource_scope_v25(&container.program, full.original_context_max, encoder)
            .map_err(|e| format!("full25 maximum scope: {e:?}"))?;
        full.check_scope(&maximum, &container.program).map_err(|e| format!("full25 layout: {e:?}"))?;
        (full.original_context_max, true)
    } else {
        let old = PalwTirLayoutV1::try_from_slice(&container.header.layout)
            .map_err(|_| "v25 requires an explicitly declared artifact layout")?;
        if old.version != PALW_TIR_LAYOUT_VERSION_V1 {
            return Err("unknown original artifact layout version".into());
        }
        (old.max_context, false)
    };
    if positions == 0 || positions > declared_context_max {
        return Err("v25 scope exceeds ORIGINAL declared artifact context".into());
    }
    let meta = misaka_palw_tir_artifact::metadata_v16::parse_recorded_metadata_v16(&container.header.meta)?;
    if full25
        && meta.get("full_court_source25").and_then(|m| m.get("lowered_context_max")).and_then(|v| v.as_u64())
            != Some(declared_context_max as u64)
    {
        return Err("full25 declared context does not match freshly lowered context pin".into());
    }
    crate::tir_layout::tir_window_covers_context_v1(&meta, positions)?;
    crate::tir_layout::tir_calibration_covers_context_v1(&container.program, &meta, positions)?;
    let court = derive_checked_flat_resource_scope_v25(&container.program, positions, encoder)
        .map_err(|e| format!("v25 public court: {e:?}"))?;
    if hash_bounded(path, maximum_file_bytes)? != expected_sha256 {
        return Err("v25 artifact changed during scope preparation".into());
    }
    Ok(PreparedArtifactCourtV25 { artifact_sha256: expected_sha256, program: container.program, declared_context_max, court })
}
impl PreparedArtifactCourtV25 {
    /// A short diagnostic cannot pass this guard merely because its court is cheap.
    pub fn check_complete_source_context(&self, original_source_context: u32) -> Result<(), String> {
        if original_source_context == 0
            || self.declared_context_max < original_source_context
            || self.court.max_positions() != original_source_context
        {
            return Err("v25 complete checkpoint scope does not cover original source context".into());
        }
        Ok(())
    }
}

pub struct DeclaredFullCourtArtifactV25 {
    pub layout: misaka_palw_kernel::flat_scope25::FullCourtLayoutV25,
    pub parameters: misaka_palw_kernel::trace::ParamCommitmentsV1,
    pub resources: misaka_palw_kernel::flat_resources29::FlatResourcePlanV29,
    pub artifact_sha256: [u8; 32],
    pub source_components: [u8; 64],
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// Declare a freshly converted full25 container. This creates a distinct layout
/// grammar and artifact; it neither invokes nor relaxes legacy class admission.
/// Source fidelity, complete task measurements and chain eligibility are pending.
pub fn declare_full_court_artifact_v25(
    input: &Path,
    expected_sha256: [u8; 32],
    output: &Path,
    original_source_directory: &Path,
    maximum_file_bytes: u64,
    payload_limit: u64,
    workspace_limit: u64,
    transpose_history15: bool,
) -> Result<DeclaredFullCourtArtifactV25, String> {
    use crate::tir_stream::{ContainerRanges, PalwTirRangeSourceV1};
    use misaka_palw_kernel::{
        descriptor::k2_tir_v4_descriptor, flat_resources29::derive_flat_resources_v29, flat_scope25::FullCourtLayoutV25, hash::id,
        public::program_root_v1,
    };
    if input == output || output.exists() {
        return Err("full25 declaration requires a fresh distinct output".into());
    }
    if hash_bounded(input, maximum_file_bytes)? != expected_sha256 {
        return Err("full25 input SHA256 differs".into());
    }
    let container = PalwTirContainerV1::open_bounded(input, maximum_file_bytes, 4 << 20).map_err(|e| e.to_string())?;
    // Only a NEW conversion can mint this marker after checking source files,
    // requested context and the actual frontend inventory. Historical markers are absent.
    let mut meta = misaka_palw_tir_artifact::metadata_v16::parse_metadata_v16(
        &container.header.meta,
        misaka_palw_tir_artifact::metadata_v16::MetadataParseProfileV16::CorrectlyRoundedV16,
    )?;
    let conversion = meta.get("full_court_source25").ok_or("unmarked legacy artifact cannot be promoted into full25")?;
    if conversion.get("schema").and_then(|v| v.as_str()) != Some("misaka.palw.fresh-full-court-conversion.v25")
        || conversion.get("complete_component_inventory").and_then(|v| v.as_bool()) != Some(true)
        || !container.header.layout.is_empty()
    {
        return Err("full25 declaration requires fresh explicitly pinned conversion with no previous layout".into());
    }
    let pin: misaka_palw_tir_lower::full_court_source25::SourcePinV25 =
        serde_json::from_value(conversion.get("source").cloned().ok_or("full25 original source pin missing")?)
            .map_err(|e| e.to_string())?;
    if conversion.get("lowered_context_max").and_then(|v| v.as_u64()) != Some(pin.original_context_max as u64)
        || pin.original_context_max > container.program.history_bound
    {
        return Err("full25 original/lowered contexts differ".into());
    }
    pin.verify_files(original_source_directory).map_err(|e| e.to_string())?;
    crate::tir_layout::tir_window_covers_context_v1(&meta, pin.original_context_max)?;
    crate::tir_layout::tir_calibration_covers_context_v1(&container.program, &meta, pin.original_context_max)?;
    let program = if transpose_history15 {
        misaka_palw_tir_lower::layout_pass15::transpose_history_contractions_v15(&container.program)?.program
    } else {
        container.program.clone()
    };
    if program.params != container.program.params || program.states != container.program.states {
        return Err("full25 transformation changed source parameter/state declarations".into());
    }
    let descriptor = k2_tir_v4_descriptor();
    let source = ContainerRanges::open_with_window(&container, 1 << 16)?;
    let prepared = crate::kernel_params::prepare_kernel_params_v3(
        &descriptor,
        &program,
        &source,
        workspace_limit,
        payload_limit,
        &mut |_, _, _| {},
    )?;
    let inventory = kaspa_consensus_core::palw_tir_artifact_v1::PalwTirModelInventoryV2::new(&descriptor, &program)?;
    let (inventory_root, _) = crate::tir_stream::palw_tir_model_inventory_root_streamed_v2(&inventory, &source)?;
    let resources = derive_flat_resources_v29(&program, &prepared.params, pin.original_context_max, None)?;
    let checked = derive_checked_flat_resource_scope_v25(&program, pin.original_context_max, None)
        .map_err(|e| format!("full25 scope: {e:?}"))?;
    if checked.scope() != &resources.court {
        return Err("full25 public checker/resource scope differs".into());
    }
    let pin_bytes = serde_json::to_vec(&pin).map_err(|e| e.to_string())?;
    let original_source_pin_sha256: [u8; 32] = Sha256::digest(&pin_bytes).into();
    let component_bytes = serde_json::to_vec(&serde_json::json!({"schema":"misaka.palw.complete-original-components.v25",
        "repository":pin.repository,"revision":pin.revision,"advertised_task":pin.advertised_task,
        "required_components":pin.required_components,"complete_tensor_inventory":pin.complete_tensor_inventory,
        "source_files":pin.source_files}))
    .map_err(|e| e.to_string())?;
    let source_components = id(b"misaka-palw/kernel/complete-original-source-components/v25", &component_bytes);
    let layout = FullCourtLayoutV25 {
        version: 25,
        original_context_max: pin.original_context_max,
        lowered_context_max: pin.original_context_max,
        trace_profile: checked.scope().profile,
        court_set: checked.court_set(),
        maximum_scope_root: checked.root(),
        maximum_resource_root: resources.vector.root(),
        program_root: program_root_v1(&program.encode()),
        parameter_root: prepared.params.root(),
        inventory_root: inventory_root.as_bytes(),
        tokenizer_root: container.header.tokenizer_id,
        original_source_pin_sha256,
    };
    layout.check_scope(&checked, &program).map_err(|e| format!("full25 layout: {e:?}"))?;
    meta["full_court_declaration25"] = serde_json::json!({"schema":"misaka.palw.full-court-declaration.v25",
        "layout_root":hex(&layout.root()),"original_source_components":hex(&source_components),
        "source_pin_sha256":hex(&original_source_pin_sha256),"transpose_history15":transpose_history15,
        "input_artifact_sha256":hex(&expected_sha256),"full_task_fidelity_or_native_final":false});
    misaka_palw_tir_artifact::metadata_v16::record_metadata_profile_v16(
        &mut meta,
        misaka_palw_tir_artifact::metadata_v16::MetadataParseProfileV16::CorrectlyRoundedV16,
    )?;
    pin.verify_files(original_source_directory).map_err(|e| e.to_string())?;
    if hash_bounded(input, maximum_file_bytes)? != expected_sha256 {
        return Err("full25 input changed during derivation".into());
    }
    let layout_bytes = borsh::to_vec(&layout).map_err(|e| e.to_string())?;
    misaka_palw_tir_artifact::write_container_v1_streamed(
        output,
        &program,
        layout_bytes,
        container.header.tokenizer_id,
        meta.to_string(),
        &mut |j, l, out| {
            let (_, length) = container.locate(j, l).ok_or("full25 missing source tensor")?;
            let mut at = 0u64;
            let mut buffer = [0u8; 65536];
            while at < length {
                let n = (length - at).min(buffer.len() as u64) as usize;
                source.read_range(j, l, at..at + n as u64, &mut buffer[..n])?;
                out.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                at += n as u64;
            }
            Ok(())
        },
    )
    .map_err(|e| e.to_string())?;
    let artifact_sha256 = hash_bounded(output, maximum_file_bytes)?;
    if hash_bounded(input, maximum_file_bytes)? != expected_sha256 {
        return Err("full25 input changed during streamed output".into());
    }
    Ok(DeclaredFullCourtArtifactV25 { layout, parameters: prepared.params, resources, artifact_sha256, source_components })
}

impl DeclaredFullCourtArtifactV25 {
    /// Canonical new class fields. Native registration independently authenticates
    /// source_binding and recomputes these roots; this object alone grants nothing.
    pub fn native_text_class_v29(
        &self,
        source_binding: [u8; 64],
    ) -> Result<kaspa_consensus_core::palw_native_text_stop_v29::NativeTextClassV29, String> {
        if source_binding == [0; 64] {
            return Err("full25 native class requires authenticated source binding".into());
        }
        Ok(kaspa_consensus_core::palw_native_text_stop_v29::NativeTextClassV29 {
            version: 29,
            source_binding,
            original_program: self.layout.program_root,
            real_parameter: self.layout.parameter_root,
            inventory: self.layout.inventory_root,
            original_source_components: kaspa_consensus_core::palw_native_text_stop_v29::native_text_source_components_v29(
                source_binding,
                self.layout.program_root,
                self.layout.parameter_root,
                self.layout.inventory_root,
            ),
            maximum_positions: self.layout.original_context_max,
            court_profile25: self.layout.court_set,
            maximum_scope25: self.layout.maximum_scope_root,
            maximum_resource29: self.layout.maximum_resource_root,
        })
    }
}

/// Public canonical source declaration already authenticated by the consumer.
/// It contains no HF/model/compiler identity and grants no publisher fidelity.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct CanonicalFullSourceV25 {
    pub source_binding: [u8; 64],
    pub original_program: [u8; 64],
    pub real_parameter: [u8; 64],
    pub inventory: [u8; 64],
    pub original_source_components: [u8; 64],
    pub original_context_max: u32,
}
pub struct PreparedCanonicalFullCourtV25 {
    pub class: kaspa_consensus_core::palw_native_text_stop_v29::NativeTextClassV29,
    pub parameters: misaka_palw_kernel::trace::ParamCommitmentsV1,
    pub resources: misaka_palw_kernel::flat_resources29::FlatResourcePlanV29,
    pub artifact_sha256: [u8; 32],
}
/// Generic open canonical-TIR adapter. No HF metadata, model or compiler allowlist
/// is consulted. Native registration independently authenticates the supplied
/// source declaration; the SDK additionally derives every real PC/R from bytes.
/// A legacy declared layout's context is still a hard upper bound, never lifted.
pub fn prepare_canonical_full_court_v25(
    path: &Path,
    expected_sha256: [u8; 32],
    declaration: &CanonicalFullSourceV25,
    maximum_file_bytes: u64,
    payload_limit: u64,
    workspace_limit: u64,
) -> Result<PreparedCanonicalFullCourtV25, String> {
    if [
        declaration.source_binding,
        declaration.original_program,
        declaration.real_parameter,
        declaration.inventory,
        declaration.original_source_components,
    ]
    .contains(&[0; 64])
        || declaration.original_context_max == 0
    {
        return Err("canonical full25 requires independently authenticated original source roots/context".into());
    }
    if hash_bounded(path, maximum_file_bytes)? != expected_sha256 {
        return Err("canonical full25 whole artifact SHA differs".into());
    }
    let c = PalwTirContainerV1::open_bounded(path, maximum_file_bytes, 4 << 20).map_err(|e| e.to_string())?;
    if misaka_palw_kernel::public::program_root_v1(&c.program.encode()) != declaration.original_program {
        return Err("canonical full25 program differs from original authenticated source".into());
    }
    if !c.header.layout.is_empty() {
        let maximum = if c.header.layout.starts_with(&25u16.to_le_bytes()) {
            let layout =
                misaka_palw_kernel::flat_scope25::FullCourtLayoutV25::try_from_slice(&c.header.layout).map_err(|e| e.to_string())?;
            layout.original_context_max
        } else {
            {
                let layout = PalwTirLayoutV1::try_from_slice(&c.header.layout).map_err(|e| e.to_string())?;
                if layout.version != 1 {
                    return Err("unknown original layout namespace".into());
                }
                layout.max_context
            }
        };
        if declaration.original_context_max > maximum {
            return Err("canonical full25 cannot lift an existing narrower declared artifact".into());
        }
    }
    let descriptor = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor();
    let ranges = crate::tir_stream::ContainerRanges::open_with_window(&c, 1 << 16)?;
    let prepared = crate::kernel_params::prepare_kernel_params_v3(
        &descriptor,
        &c.program,
        &ranges,
        workspace_limit,
        payload_limit,
        &mut |_, _, _| {},
    )?;
    let inventory = kaspa_consensus_core::palw_tir_artifact_v1::PalwTirModelInventoryV2::new(&descriptor, &c.program)?;
    let (r, _) = crate::tir_stream::palw_tir_model_inventory_root_streamed_v2(&inventory, &ranges)?;
    if prepared.params.root() != declaration.real_parameter || r.as_bytes() != declaration.inventory {
        return Err("canonical full25 independently derived complete real PC/inventory differs".into());
    }
    let resources = misaka_palw_kernel::flat_resources29::derive_flat_resources_v29(
        &c.program,
        &prepared.params,
        declaration.original_context_max,
        None,
    )?;
    let canonical_components = kaspa_consensus_core::palw_native_text_stop_v29::native_text_source_components_v29(
        declaration.source_binding,
        declaration.original_program,
        declaration.real_parameter,
        declaration.inventory,
    );
    if declaration.original_source_components != canonical_components {
        return Err("canonical text29 source components differ from exact standalone causal-source binding; pipeline components require Source28".into());
    }
    let class = kaspa_consensus_core::palw_native_text_stop_v29::NativeTextClassV29 {
        version: 29,
        source_binding: declaration.source_binding,
        original_program: declaration.original_program,
        real_parameter: declaration.real_parameter,
        inventory: declaration.inventory,
        original_source_components: declaration.original_source_components,
        maximum_positions: declaration.original_context_max,
        court_profile25: resources.court.court_set,
        maximum_scope25: resources.court.root(),
        maximum_resource29: resources.vector.root(),
    };
    if hash_bounded(path, maximum_file_bytes)? != expected_sha256 {
        return Err("canonical full25 artifact mutated during preparation".into());
    }
    Ok(PreparedCanonicalFullCourtV25 { class, parameters: prepared.params, resources, artifact_sha256: expected_sha256 })
}
