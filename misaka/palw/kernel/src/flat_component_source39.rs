//! Owning39 complete original component/source declarations. No declaration is a
//! publisher authenticity or source-fidelity certificate; native consumers bind
//! the exact complete-source root and genuine upstream Final capabilities.
//! Native consumers authenticate original file/inventory metadata and validate actual
//! pipeline value bindings separately. This grammar prevents dropped declared components,
//! unbound inputs and context relabeling; it is not a publisher/fidelity certificate.
use crate::{
    hash::{Digest, object_id},
    verify::DismissalV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::program_v2::InputSource;
use misaka_palw_tir::program_v39::TirProgramV39;
pub const MAX_COMPONENTS_V39: usize = 64;
pub const MAX_SOURCE_FILES_V39: usize = 1024;
pub const MAX_COMPONENT_INPUTS_V39: usize = 1024;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OriginalFilePinV39 {
    pub path: String,
    pub bytes: u64,
    pub sha256: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceComponentV39 {
    pub name: String,
    pub source_files: Vec<u16>,
    pub stages: Vec<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComponentStageV39 {
    pub original_source: Digest,
    pub original_profile: Digest,
    pub original_parameter: Digest,
    pub original_program: Digest,
    pub original_inventory: Digest,
    pub input_declarations: Digest,
    pub output_declaration: Digest,
    pub original_context_max: u32,
    pub lowered_context_max: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ComponentInputSourceV39 {
    External { job_input: u16, declaration: Digest },
    Upstream { stage: u16, output_declaration: Digest },
    Random { domain: u16, distribution: Digest },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComponentInputBindingV39 {
    pub stage: u16,
    pub input: u16,
    pub source: ComponentInputSourceV39,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompleteComponentManifestV39 {
    pub version: u16,
    pub repository: String,
    pub revision: [u8; 20],
    pub original_task_metadata: Digest,
    pub original_pipeline: Digest,
    pub original_required_components: Vec<String>,
    pub source_files: Vec<OriginalFilePinV39>,
    pub components: Vec<SourceComponentV39>,
    pub stages: Vec<ComponentStageV39>,
    pub inputs: Vec<ComponentInputBindingV39>,
}
impl CompleteComponentManifestV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/complete-original-component-manifest/v39", self)
    }
}
fn bad(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn ordered<T: Ord>(v: &[T]) -> bool {
    v.windows(2).all(|w| w[0] < w[1])
}
pub fn stage_input_declarations_root_v39(program: &TirProgramV39) -> Digest {
    object_id(b"misaka-palw/kernel/original-stage-input-declarations/v39", &program.inputs)
}
pub fn stage_output_declaration_root_v39(program: &TirProgramV39) -> Digest {
    object_id(b"misaka-palw/kernel/original-stage-output-declaration/v39", &program.output)
}
pub fn stage_program_root_v39(program: &TirProgramV39) -> Digest {
    object_id(b"misaka-palw/kernel/original-stage-canonical-program/v39", program)
}
pub struct CheckedComponentManifestV39 {
    manifest: CompleteComponentManifestV39,
}
impl CheckedComponentManifestV39 {
    pub fn manifest(&self) -> &CompleteComponentManifestV39 {
        &self.manifest
    }
    pub fn root(&self) -> Digest {
        self.manifest.root()
    }
}
/// Expected component/task metadata, source inventories and original maxima MUST
/// originate from the independently authenticated original source declaration.
/// Never substitute the lowerer's own list for that source declaration.
fn check_manifest_claims_v39(
    manifest: &CompleteComponentManifestV39,
    programs: &[TirProgramV39],
    authenticated_inventories: &[Digest],
    original_contexts: &[u32],
    original_required_components: &[String],
    original_task_metadata: Digest,
    expected_input_bindings: &[ComponentInputBindingV39],
) -> Result<CheckedComponentManifestV39, DismissalV1> {
    if manifest.version != 39
        || manifest.repository.is_empty()
        || manifest.repository.len() > 256
        || manifest.revision == [0; 20]
        || manifest.original_task_metadata == [0; 64]
        || manifest.original_task_metadata != original_task_metadata
        || manifest.original_required_components != original_required_components
        || !ordered(&manifest.original_required_components)
        || manifest.original_required_components.is_empty()
        || manifest.original_required_components.len() > MAX_COMPONENTS_V39
        || manifest.original_required_components.iter().any(|s| s.is_empty() || s.len() > 128)
        || manifest.components.len() != manifest.original_required_components.len()
        || manifest.source_files.is_empty()
        || manifest.source_files.len() > MAX_SOURCE_FILES_V39
        || manifest.stages.is_empty()
        || manifest.stages.len() > MAX_COMPONENTS_V39
        || manifest.stages.len() != programs.len()
        || programs.len() != authenticated_inventories.len()
        || programs.len() != original_contexts.len()
        || manifest.inputs.len() > MAX_COMPONENT_INPUTS_V39
        || manifest.inputs != expected_input_bindings
    {
        return Err(bad("component39 original declaration/extent differs"));
    }
    if manifest.source_files.windows(2).any(|w| w[0].path >= w[1].path)
        || manifest.source_files.iter().any(|f| {
            f.path.is_empty()
                || f.path.len() > 512
                || f.path.starts_with('/')
                || f.path.contains('\\')
                || f.path.contains('\0')
                || f.path.split('/').any(|p| p.is_empty() || p == "." || p == "..")
                || f.sha256 == [0; 32]
        })
    {
        return Err(bad("component39 file inventory is not canonical"));
    }
    let mut files = vec![false; manifest.source_files.len()];
    let mut stages = vec![false; manifest.stages.len()];
    for (c, required) in manifest.components.iter().zip(original_required_components) {
        if &c.name != required
            || c.source_files.is_empty()
            || !ordered(&c.source_files)
            || !ordered(&c.stages)
            || c.source_files.len() > MAX_SOURCE_FILES_V39
            || c.stages.len() > MAX_COMPONENTS_V39
        {
            return Err(bad("component39 drops/duplicates a required component"));
        }
        for i in &c.source_files {
            *files.get_mut(*i as usize).ok_or_else(|| bad("component39 missing file"))? = true;
        }
        for i in &c.stages {
            *stages.get_mut(*i as usize).ok_or_else(|| bad("component39 missing stage"))? = true;
        }
    }
    if files.contains(&false) || stages.contains(&false) {
        return Err(bad("component39 orphan original source file/stage"));
    }
    if manifest.inputs.windows(2).any(|w| (w[0].stage, w[0].input) >= (w[1].stage, w[1].input)) {
        return Err(bad("component39 input bindings must be canonical stage/input order"));
    }
    let mut input_coordinates = std::collections::BTreeSet::new();
    let mut input_count = 0usize;
    for (i, ((p, r), context)) in programs.iter().zip(authenticated_inventories).zip(original_contexts).enumerate() {
        // Canonical V2 validation remains the original primitive/input grammar.
        TirProgramV39::decode_canonical(&p.encode()).map_err(|_| bad("component39 noncanonical original stage"))?;
        let declared = &manifest.stages[i];
        if *r == [0; 64]
            || *context == 0
            || *context > p.history_bound
            || declared.original_context_max != *context
            || declared.lowered_context_max != *context
            || declared.original_program != stage_program_root_v39(p)
            || declared.original_inventory != *r
            || declared.input_declarations != stage_input_declarations_root_v39(p)
            || declared.output_declaration != stage_output_declaration_root_v39(p)
        {
            return Err(bad("component39 original stage/source/context/IO differs"));
        }
        input_count = input_count.checked_add(p.inputs.len()).ok_or_else(|| bad("component39 input overflow"))?;
    }
    if input_count != manifest.inputs.len() {
        return Err(bad("component39 original input omitted"));
    }
    for b in &manifest.inputs {
        if !input_coordinates.insert((b.stage, b.input)) {
            return Err(bad("component39 duplicate input binding"));
        }
        let program = programs.get(b.stage as usize).ok_or_else(|| bad("component39 missing consuming stage"))?;
        let input = program.inputs.get(b.input as usize).ok_or_else(|| bad("component39 missing consuming input"))?;
        match (&b.source, input.source) {
            (ComponentInputSourceV39::External { declaration, .. }, InputSource::External { .. }) if *declaration != [0; 64] => {}
            (ComponentInputSourceV39::Upstream { stage, output_declaration }, InputSource::External { .. })
                if *stage < b.stage
                    && manifest.stages.get(*stage as usize).is_some_and(|s| s.output_declaration == *output_declaration) => {}
            (ComponentInputSourceV39::Random { domain, distribution }, InputSource::Random { domain: d, dist, per_step })
                if *domain == d
                    && *distribution == object_id(b"misaka-palw/kernel/original-stage-random-source/v39", &(d, dist, per_step)) => {}
            _ => return Err(bad("component39 changes original external/upstream/random input semantics")),
        }
    }
    Ok(CheckedComponentManifestV39 { manifest: manifest.clone() })
}

pub const MAX_COMPONENT_MANIFEST_BYTES_V39: u64 = 4 << 20;
impl CompleteComponentManifestV39 {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        crate::flat_court4::size_with_limit(self, MAX_COMPONENT_MANIFEST_BYTES_V39)?;
        borsh::to_vec(self)
    }
    /// All string/vector lengths are bounded BEFORE allocation; raw Borsh derive
    /// is never the native wire entry point for this manifest.
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        use std::io::{Cursor, Read};
        fn invalid() -> std::io::Error {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bounded component39 manifest")
        }
        fn string(r: &mut Cursor<&[u8]>, max: usize) -> std::io::Result<String> {
            let n = u32::deserialize_reader(r)? as usize;
            if n > max || n > r.get_ref().len().saturating_sub(r.position() as usize) {
                return Err(invalid());
            }
            let mut b = vec![0; n];
            r.read_exact(&mut b)?;
            String::from_utf8(b).map_err(|_| invalid())
        }
        if bytes.len() as u64 > MAX_COMPONENT_MANIFEST_BYTES_V39 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 39 {
            return Err(invalid());
        }
        let repository = string(&mut r, 256)?;
        let revision = <[u8; 20]>::deserialize_reader(&mut r)?;
        let original_task_metadata = Digest::deserialize_reader(&mut r)?;
        let original_pipeline = Digest::deserialize_reader(&mut r)?;
        let original_required_components = crate::flat_court4::vector(&mut r, MAX_COMPONENTS_V39, |r| string(r, 128))?;
        let source_files = crate::flat_court4::vector(&mut r, MAX_SOURCE_FILES_V39, |r| {
            Ok(OriginalFilePinV39 {
                path: string(r, 512)?,
                bytes: u64::deserialize_reader(r)?,
                sha256: <[u8; 32]>::deserialize_reader(r)?,
            })
        })?;
        let components = crate::flat_court4::vector(&mut r, MAX_COMPONENTS_V39, |r| {
            Ok(SourceComponentV39 {
                name: string(r, 128)?,
                source_files: crate::flat_court4::vector(r, MAX_SOURCE_FILES_V39, u16::deserialize_reader)?,
                stages: crate::flat_court4::vector(r, MAX_COMPONENTS_V39, u16::deserialize_reader)?,
            })
        })?;
        let stages = crate::flat_court4::vector(&mut r, MAX_COMPONENTS_V39, ComponentStageV39::deserialize_reader)?;
        let inputs = crate::flat_court4::vector(&mut r, MAX_COMPONENT_INPUTS_V39, ComponentInputBindingV39::deserialize_reader)?;
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        Ok(Self {
            version,
            repository,
            revision,
            original_task_metadata,
            original_pipeline,
            original_required_components,
            source_files,
            components,
            stages,
            inputs,
        })
    }
}

use crate::{
    flat_pipeline39::{CheckedPipelineV39, PipelineV39},
    flat_source39::{CheckedSourceV39, FlatSourceWireV39},
};
/// Immutable whole-source bundle. Its bounded canonical body is paid/restored as
/// one original object, including every advertised component and its real PC.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct CompleteSourceBundleV39 {
    pub version: u16,
    pub manifest: Vec<u8>,
    pub pipeline: Vec<u8>,
    pub sources: Vec<Vec<u8>>,
}
pub const MAX_COMPLETE_SOURCE_BYTES_V39: usize = 64 << 20;
pub struct CheckedCompleteSourceV39 {
    wire: CompleteSourceBundleV39,
    manifest: CheckedComponentManifestV39,
    pipeline: CheckedPipelineV39,
    sources: Vec<CheckedSourceV39>,
    root: Digest,
}
impl CheckedCompleteSourceV39 {
    pub fn root(&self) -> Digest {
        self.root
    }
    pub fn manifest(&self) -> &CheckedComponentManifestV39 {
        &self.manifest
    }
    pub fn pipeline(&self) -> &CheckedPipelineV39 {
        &self.pipeline
    }
    pub fn sources(&self) -> &[CheckedSourceV39] {
        &self.sources
    }
    pub fn wire(&self) -> &CompleteSourceBundleV39 {
        &self.wire
    }
    pub fn restore_bytes(&self) -> u64 {
        crate::flat_court4::size_with_limit(&self.wire, MAX_COMPLETE_SOURCE_BYTES_V39 as u64).expect("bounded39 serialization")
    }
}
pub fn check_component_manifest_v39(
    m: &CompleteComponentManifestV39,
    sources: &[CheckedSourceV39],
    pipeline: &CheckedPipelineV39,
    required: &[String],
    task: Digest,
    inputs: &[ComponentInputBindingV39],
) -> Result<CheckedComponentManifestV39, DismissalV1> {
    let programs = sources.iter().map(|s| s.program().clone()).collect::<Vec<_>>();
    let inventory = sources.iter().map(|s| s.inventory_root()).collect::<Vec<_>>();
    let contexts = sources.iter().map(|s| s.original_context_max()).collect::<Vec<_>>();
    if m.original_pipeline != pipeline.root() || sources.len() != pipeline.wire().stages.len() {
        return Err(bad("component39 exact complete pipeline binding"));
    }
    for (i, s) in sources.iter().enumerate() {
        let d = m.stages.get(i).ok_or_else(|| bad("component39 missing original source"))?;
        if d.original_source != s.root()
            || d.original_profile != s.profile_root()
            || d.original_parameter != s.parameters().root()
            || s.original_task_metadata() != task
        {
            return Err(bad("component39 original source/profile/real PC/task differs"));
        }
    }
    let out = check_manifest_claims_v39(m, &programs, &inventory, &contexts, required, task, inputs)?;
    // Bind exact recipe, not only a caller-supplied upstream declaration digest.
    let mut expected = Vec::new();
    for (i, (stage, source)) in pipeline.wire().stages.iter().zip(sources).enumerate() {
        let mut bindings = stage.bind.iter();
        for (j, input) in source.program().inputs.iter().enumerate() {
            let source = match input.source {
                InputSource::Random { domain, dist, per_step } => ComponentInputSourceV39::Random {
                    domain,
                    distribution: object_id(b"misaka-palw/kernel/original-stage-random-source/v39", &(domain, dist, per_step)),
                },
                InputSource::External { .. } => {
                    let b = bindings.next().ok_or_else(|| bad("component39 absent pipeline input recipe"))?;
                    match b {
                        misaka_palw_tir::pipeline::Binding::StageRows { stage, .. }
                        | misaka_palw_tir::pipeline::Binding::StageFinal { stage }
                        | misaka_palw_tir::pipeline::Binding::StageRowCount { stage, .. } => ComponentInputSourceV39::Upstream {
                            stage: *stage as u16,
                            output_declaration: m.stages[*stage as usize].output_declaration,
                        },
                        _ => ComponentInputSourceV39::External {
                            job_input: j as u16,
                            declaration: object_id(
                                b"misaka-palw/kernel/external-pipeline-recipe/v39",
                                &(i as u16, j as u16, input, b),
                            ),
                        },
                    }
                }
            };
            expected.push(ComponentInputBindingV39 { stage: i as u16, input: j as u16, source });
        }
        if bindings.next().is_some() {
            return Err(bad("component39 surplus pipeline input recipe"));
        }
    }
    if m.inputs != expected {
        return Err(bad("component39 pipeline source/input recipes differ"));
    }
    Ok(out)
}
impl CompleteSourceBundleV39 {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        crate::flat_court4::size_with_limit(self, MAX_COMPLETE_SOURCE_BYTES_V39 as u64)?;
        borsh::to_vec(self)
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        use std::io::{Cursor, Read};
        if raw.len() > MAX_COMPLETE_SOURCE_BYTES_V39 {
            return Err("bundle39 allocation cap".into());
        }
        let mut r = Cursor::new(raw);
        fn blob(r: &mut Cursor<&[u8]>, cap: usize) -> Result<Vec<u8>, String> {
            let n = u32::deserialize_reader(r).map_err(|_| "bundle39 extent")? as usize;
            if n > cap || n > r.get_ref().len().saturating_sub(r.position() as usize) {
                return Err("bundle39 bounded blob".into());
            }
            let mut b = vec![0; n];
            r.read_exact(&mut b).map_err(|_| "bundle39 body")?;
            Ok(b)
        }
        let version = u16::deserialize_reader(&mut r).map_err(|_| "bundle39 version")?;
        if version != 39 {
            return Err("bundle39 namespace".into());
        }
        let manifest = blob(&mut r, MAX_COMPONENT_MANIFEST_BYTES_V39 as usize)?;
        let pipeline = blob(&mut r, crate::flat_pipeline39::MAX_PIPELINE_BYTES_V39)?;
        let n = u32::deserialize_reader(&mut r).map_err(|_| "bundle39 count")? as usize;
        if n == 0 || n > MAX_COMPONENTS_V39 || n > raw.len().saturating_sub(r.position() as usize) / 4 {
            return Err("bundle39 source count".into());
        }
        let mut sources = Vec::with_capacity(n);
        for _ in 0..n {
            sources.push(blob(&mut r, crate::flat_source39::SOURCE39_MAX_BYTES)?);
        }
        let w = Self { version, manifest, pipeline, sources };
        if r.position() != raw.len() as u64 || w.to_bytes().map_err(|e| e.to_string())? != raw {
            return Err("bundle39 canonical trailing/encoding".into());
        }
        Ok(w)
    }
    pub fn check(&self) -> Result<CheckedCompleteSourceV39, String> {
        if self.version != 39 {
            return Err("bundle39 version".into());
        }
        self.to_bytes().map_err(|e| e.to_string())?;
        let manifest = CompleteComponentManifestV39::from_bytes(&self.manifest).map_err(|e| e.to_string())?;
        let mut sources = Vec::new();
        for raw in &self.sources {
            sources.push(FlatSourceWireV39::from_bytes(raw)?.check()?);
        }
        let programs = sources.iter().map(|s| s.program().clone()).collect::<Vec<_>>();
        let pipeline =
            PipelineV39::from_bytes(&self.pipeline).map_err(|e| e.to_string())?.check(&programs).map_err(|e| e.to_string())?;
        let checked = check_component_manifest_v39(
            &manifest,
            &sources,
            &pipeline,
            &manifest.original_required_components,
            manifest.original_task_metadata,
            &manifest.inputs,
        )
        .map_err(|e| format!("{e:?}"))?;
        let root = object_id(
            b"misaka-palw/kernel/complete-original-source-bundle/v39",
            &(
                39u16,
                checked.root(),
                pipeline.root(),
                sources.iter().map(|s| s.root()).collect::<Vec<_>>(),
                self.to_bytes().map_err(|e| e.to_string())?.len() as u64,
            ),
        );
        Ok(CheckedCompleteSourceV39 { wire: self.clone(), manifest: checked, pipeline, sources, root })
    }
}
