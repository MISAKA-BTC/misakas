//! Explicit full-source pipeline component declaration, independent of old source14.
//! Native consumers authenticate original file/inventory metadata and validate actual
//! pipeline value bindings separately. This grammar prevents dropped declared components,
//! unbound inputs and context relabeling; it is not a publisher/fidelity certificate.
use crate::{
    hash::{Digest, object_id},
    verify::DismissalV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::program_v2::{InputSource, TirProgramV2};
pub const MAX_COMPONENTS_V28: usize = 64;
pub const MAX_SOURCE_FILES_V28: usize = 1024;
pub const MAX_COMPONENT_INPUTS_V28: usize = 1024;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OriginalFilePinV28 {
    pub path: String,
    pub bytes: u64,
    pub sha256: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceComponentV28 {
    pub name: String,
    pub source_files: Vec<u16>,
    pub stages: Vec<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComponentStageV28 {
    pub original_program: Digest,
    pub original_inventory: Digest,
    pub input_declarations: Digest,
    pub output_declaration: Digest,
    pub original_context_max: u32,
    pub lowered_context_max: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ComponentInputSourceV28 {
    External { job_input: u16, declaration: Digest },
    Upstream { stage: u16, output_declaration: Digest },
    Random { domain: u16, distribution: Digest },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ComponentInputBindingV28 {
    pub stage: u16,
    pub input: u16,
    pub source: ComponentInputSourceV28,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CompleteComponentManifestV28 {
    pub version: u16,
    pub repository: String,
    pub revision: [u8; 20],
    pub original_task_metadata: Digest,
    pub original_required_components: Vec<String>,
    pub source_files: Vec<OriginalFilePinV28>,
    pub components: Vec<SourceComponentV28>,
    pub stages: Vec<ComponentStageV28>,
    pub inputs: Vec<ComponentInputBindingV28>,
}
impl CompleteComponentManifestV28 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/complete-original-component-manifest/v28", self)
    }
}
fn bad(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn ordered<T: Ord>(v: &[T]) -> bool {
    v.windows(2).all(|w| w[0] < w[1])
}
pub fn stage_input_declarations_root_v28(program: &TirProgramV2) -> Digest {
    object_id(b"misaka-palw/kernel/original-stage-input-declarations/v28", &program.inputs)
}
pub fn stage_output_declaration_root_v28(program: &TirProgramV2) -> Digest {
    object_id(b"misaka-palw/kernel/original-stage-output-declaration/v28", &program.output)
}
pub fn stage_program_root_v28(program: &TirProgramV2) -> Digest {
    object_id(b"misaka-palw/kernel/original-stage-canonical-program/v28", program)
}
pub struct CheckedComponentManifestV28 {
    manifest: CompleteComponentManifestV28,
}
impl CheckedComponentManifestV28 {
    pub fn manifest(&self) -> &CompleteComponentManifestV28 {
        &self.manifest
    }
    pub fn root(&self) -> Digest {
        self.manifest.root()
    }
}
/// Expected component/task metadata, source inventories and original maxima MUST
/// originate from the independently authenticated original source declaration.
/// Never substitute the lowerer's own list for that source declaration.
pub fn check_component_manifest_v28(
    manifest: &CompleteComponentManifestV28,
    programs: &[TirProgramV2],
    authenticated_inventories: &[Digest],
    original_contexts: &[u32],
    original_required_components: &[String],
    original_task_metadata: Digest,
    expected_input_bindings: &[ComponentInputBindingV28],
) -> Result<CheckedComponentManifestV28, DismissalV1> {
    if manifest.version != 28
        || manifest.repository.is_empty()
        || manifest.repository.len() > 256
        || manifest.revision == [0; 20]
        || manifest.original_task_metadata == [0; 64]
        || manifest.original_task_metadata != original_task_metadata
        || manifest.original_required_components != original_required_components
        || !ordered(&manifest.original_required_components)
        || manifest.original_required_components.is_empty()
        || manifest.original_required_components.len() > MAX_COMPONENTS_V28
        || manifest.original_required_components.iter().any(|s| s.is_empty() || s.len() > 128)
        || manifest.components.len() != manifest.original_required_components.len()
        || manifest.source_files.is_empty()
        || manifest.source_files.len() > MAX_SOURCE_FILES_V28
        || manifest.stages.is_empty()
        || manifest.stages.len() > MAX_COMPONENTS_V28
        || manifest.stages.len() != programs.len()
        || programs.len() != authenticated_inventories.len()
        || programs.len() != original_contexts.len()
        || manifest.inputs.len() > MAX_COMPONENT_INPUTS_V28
        || manifest.inputs != expected_input_bindings
    {
        return Err(bad("component28 original declaration/extent differs"));
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
        return Err(bad("component28 file inventory is not canonical"));
    }
    let mut files = vec![false; manifest.source_files.len()];
    let mut stages = vec![false; manifest.stages.len()];
    for (c, required) in manifest.components.iter().zip(original_required_components) {
        if &c.name != required
            || c.source_files.is_empty()
            || !ordered(&c.source_files)
            || !ordered(&c.stages)
            || c.source_files.len() > MAX_SOURCE_FILES_V28
            || c.stages.len() > MAX_COMPONENTS_V28
        {
            return Err(bad("component28 drops/duplicates a required component"));
        }
        for i in &c.source_files {
            *files.get_mut(*i as usize).ok_or_else(|| bad("component28 missing file"))? = true;
        }
        for i in &c.stages {
            *stages.get_mut(*i as usize).ok_or_else(|| bad("component28 missing stage"))? = true;
        }
    }
    if files.contains(&false) || stages.contains(&false) {
        return Err(bad("component28 orphan original source file/stage"));
    }
    if manifest.inputs.windows(2).any(|w| (w[0].stage, w[0].input) >= (w[1].stage, w[1].input)) {
        return Err(bad("component28 input bindings must be canonical stage/input order"));
    }
    let mut input_coordinates = std::collections::BTreeSet::new();
    let mut input_count = 0usize;
    for (i, ((p, r), context)) in programs.iter().zip(authenticated_inventories).zip(original_contexts).enumerate() {
        // Canonical V2 validation remains the original primitive/input grammar.
        TirProgramV2::decode_canonical(&p.encode()).map_err(|_| bad("component28 noncanonical original stage"))?;
        let declared = &manifest.stages[i];
        if *r == [0; 64]
            || *context == 0
            || *context > p.history_bound
            || declared.original_context_max != *context
            || declared.lowered_context_max != *context
            || declared.original_program != stage_program_root_v28(p)
            || declared.original_inventory != *r
            || declared.input_declarations != stage_input_declarations_root_v28(p)
            || declared.output_declaration != stage_output_declaration_root_v28(p)
        {
            return Err(bad("component28 original stage/source/context/IO differs"));
        }
        input_count = input_count.checked_add(p.inputs.len()).ok_or_else(|| bad("component28 input overflow"))?;
    }
    if input_count != manifest.inputs.len() {
        return Err(bad("component28 original input omitted"));
    }
    for b in &manifest.inputs {
        if !input_coordinates.insert((b.stage, b.input)) {
            return Err(bad("component28 duplicate input binding"));
        }
        let program = programs.get(b.stage as usize).ok_or_else(|| bad("component28 missing consuming stage"))?;
        let input = program.inputs.get(b.input as usize).ok_or_else(|| bad("component28 missing consuming input"))?;
        match (&b.source, input.source) {
            (ComponentInputSourceV28::External { declaration, .. }, InputSource::External { .. }) if *declaration != [0; 64] => {}
            (ComponentInputSourceV28::Upstream { stage, output_declaration }, InputSource::External { .. })
                if *stage < b.stage
                    && manifest.stages.get(*stage as usize).is_some_and(|s| s.output_declaration == *output_declaration) => {}
            (ComponentInputSourceV28::Random { domain, distribution }, InputSource::Random { domain: d, dist, per_step })
                if *domain == d
                    && *distribution == object_id(b"misaka-palw/kernel/original-stage-random-source/v28", &(d, dist, per_step)) => {}
            _ => return Err(bad("component28 changes original external/upstream/random input semantics")),
        }
    }
    Ok(CheckedComponentManifestV28 { manifest: manifest.clone() })
}

pub const MAX_COMPONENT_MANIFEST_BYTES_V28: u64 = 4 << 20;
impl CompleteComponentManifestV28 {
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        crate::flat_court4::size_with_limit(self, MAX_COMPONENT_MANIFEST_BYTES_V28)?;
        borsh::to_vec(self)
    }
    /// All string/vector lengths are bounded BEFORE allocation; raw Borsh derive
    /// is never the native wire entry point for this manifest.
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        use std::io::{Cursor, Read};
        fn invalid() -> std::io::Error {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bounded component28 manifest")
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
        if bytes.len() as u64 > MAX_COMPONENT_MANIFEST_BYTES_V28 {
            return Err(invalid());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 28 {
            return Err(invalid());
        }
        let repository = string(&mut r, 256)?;
        let revision = <[u8; 20]>::deserialize_reader(&mut r)?;
        let original_task_metadata = Digest::deserialize_reader(&mut r)?;
        let original_required_components = crate::flat_court4::vector(&mut r, MAX_COMPONENTS_V28, |r| string(r, 128))?;
        let source_files = crate::flat_court4::vector(&mut r, MAX_SOURCE_FILES_V28, |r| {
            Ok(OriginalFilePinV28 {
                path: string(r, 512)?,
                bytes: u64::deserialize_reader(r)?,
                sha256: <[u8; 32]>::deserialize_reader(r)?,
            })
        })?;
        let components = crate::flat_court4::vector(&mut r, MAX_COMPONENTS_V28, |r| {
            Ok(SourceComponentV28 {
                name: string(r, 128)?,
                source_files: crate::flat_court4::vector(r, MAX_SOURCE_FILES_V28, u16::deserialize_reader)?,
                stages: crate::flat_court4::vector(r, MAX_COMPONENTS_V28, u16::deserialize_reader)?,
            })
        })?;
        let stages = crate::flat_court4::vector(&mut r, MAX_COMPONENTS_V28, ComponentStageV28::deserialize_reader)?;
        let inputs = crate::flat_court4::vector(&mut r, MAX_COMPONENT_INPUTS_V28, ComponentInputBindingV28::deserialize_reader)?;
        if r.position() != bytes.len() as u64 {
            return Err(invalid());
        }
        Ok(Self {
            version,
            repository,
            revision,
            original_task_metadata,
            original_required_components,
            source_files,
            components,
            stages,
            inputs,
        })
    }
}
