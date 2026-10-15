//! Source-backed compiler, rather than a wrapper around preconstructed Source28 artifacts.
//! The independently pinned declaration describes the complete original checkpoint/task.
//! Each declarative stage consumes actual source ranges and emits canonical V2 and weights;
//! Source28 PC/R, manifest and pipeline are derived from those emitted bytes.
use super::*;
use crate::runtime_pack::commit::hex;
/// Tooling-only aggregate source/file cap, not native admission or a RAM allocation.
/// Large dense source checkpoints may exceed the older converter's 128GiB budget.
pub const MAX_SOURCE_FILE_BYTES_V28: u64 = 512 << 30;
use misaka_palw_kernel::{flat_component_source28::*, flat_inputs12::source28::FlatStageSourceWireV28};
use misaka_palw_tir::{
    pipeline::*,
    program_v2::{InputSource, TirProgramV2},
};
use misaka_palw_tir_lower::{
    frontend_pack::{FrontendPack, FrontendSource, stage28::StageSemanticsV28},
    weights::{Tensor, TensorMeta, TensorSource},
};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    ops::Range,
    path::PathBuf,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilePin {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalComponent {
    pub name: String,
    pub source_files: Vec<String>,
    pub stages: Vec<u16>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalSource {
    pub repository: String,
    pub revision: String,
    pub advertised_task: String,
    pub task_metadata_file: String,
    pub task_metadata_sha256: String,
    pub source_files: Vec<FilePin>,
    pub components: Vec<OriginalComponent>,
    /// Qualified names `checkpoint-relative-path::tensor-name`, including non-text components.
    pub complete_tensor_inventory: Vec<String>,
    pub original_contexts: Vec<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageFrontend {
    #[serde(default)]
    pub frontend_pack: String,
    pub frontend_sha256: Option<String>,
    pub checkpoint: String,
    #[serde(default)]
    pub config_file: String,
    /// A component config subtree; all keys inside it are checked by strict Cfg.
    pub config_pointer: String,
    pub tensor_names: Vec<String>,
    pub semantics: Option<StageSemanticsV28>,
    /// Direct canonical mode's declarative source imports; no graph/compiler metadata is required.
    #[serde(default)]
    pub source_bindings: serde_json::Value,
    /// Explicit source-backed mathematical compiler; never an already converted artifact.
    pub vision38: Option<super::vision38::VisionFrontendV38>,
    pub decoder38: Option<super::decoder38::DecoderFrontendV38>,
    /// Optional independent compiler output. If supplied it must equal the declarative output.
    #[serde(default)]
    pub canonical_v2: String,
    pub canonical_sha256: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteFrontendV28 {
    pub schema: String,
    pub original: OriginalSource,
    pub stages: Vec<StageFrontend>,
    pub pipeline: PipelineDescription,
    pub tokenizer_file: String,
    /// Explicit inert pointers in source configs outside stage-owned subtrees.
    #[serde(default)]
    pub config_inert: BTreeMap<String, Vec<String>>,
    pub block_bytes: usize,
    pub maximum_file_bytes: u64,
    pub tensor_workspace_bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenDescription {
    pub prefix: Vec<u32>,
    pub source: String,
    pub suffix: Vec<u32>,
    pub pad: Option<(u32, u32)>,
}
impl TokenDescription {
    fn compile(&self) -> Result<TokenRule, String> {
        let source = match self.source.as_str() {
            "prompt" => TokenSource::Prompt,
            "negative" => TokenSource::Negative,
            "source" => TokenSource::Source,
            "generated" => TokenSource::Generated,
            "key" => TokenSource::Key,
            _ => return Err("COMPLETE_FRONTEND_UNKNOWN_TOKEN_SOURCE".into()),
        };
        if self.prefix.len() > MAX_TEMPLATE_TOKENS || self.suffix.len() > MAX_TEMPLATE_TOKENS {
            return Err("COMPLETE_FRONTEND_TOKEN_TEMPLATE_BOUND".into());
        }
        Ok(TokenRule {
            prefix: self.prefix.clone(),
            source,
            suffix: self.suffix.clone(),
            pad: self.pad.map(|(id, to_len)| TokenPad { id, to_len }),
        })
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TripDescription {
    Fixed { n: u32 },
    JobSteps,
    TokenCount,
    TextStream,
    Decode,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BindingDescription {
    JobScalar { index: u8 },
    JobTokens { rule: TokenDescription },
    StageRows { stage: u8, drop: u32, pad_to: u32 },
    StageFinal { stage: u8 },
    StageRowCount { stage: u8, drop: u32 },
    JobTokenCount { rule: TokenDescription },
    JobImage { index: u8 },
}
impl BindingDescription {
    fn compile(&self) -> Result<Binding, String> {
        Ok(match self {
            Self::JobScalar { index } => Binding::JobScalar { index: *index },
            Self::JobTokens { rule } => Binding::JobTokens { rule: rule.compile()? },
            Self::StageRows { stage, drop, pad_to } => Binding::StageRows { stage: *stage, drop: *drop, pad_to: *pad_to },
            Self::StageFinal { stage } => Binding::StageFinal { stage: *stage },
            Self::StageRowCount { stage, drop } => Binding::StageRowCount { stage: *stage, drop: *drop },
            Self::JobTokenCount { rule } => Binding::JobTokenCount { rule: rule.compile()? },
            Self::JobImage { index } => Binding::JobImage { index: *index },
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineStageDescription {
    pub name: String,
    pub trip: TripDescription,
    pub max_trip: u32,
    pub tokens: Option<TokenDescription>,
    pub bind: Vec<BindingDescription>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineDescription {
    pub stages: Vec<PipelineStageDescription>,
    pub output_stage: u8,
}
impl PipelineDescription {
    fn compile(&self, programs: &[TirProgramV2]) -> Result<TirPipelineV1, String> {
        if self.stages.len() != programs.len() || self.stages.len() > MAX_STAGES {
            return Err("COMPLETE_FRONTEND_PIPELINE_STAGE_BOUND".into());
        }
        let stages = self
            .stages
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Ok(StageDecl {
                    name: s.name.clone(),
                    program: i as u16,
                    trip: match s.trip {
                        TripDescription::Fixed { n } => TripRule::Fixed { n },
                        TripDescription::JobSteps => TripRule::JobSteps,
                        TripDescription::TokenCount => TripRule::TokenCount,
                        TripDescription::TextStream => TripRule::TextStream,
                        TripDescription::Decode => TripRule::Decode,
                    },
                    max_trip: s.max_trip,
                    tokens: s.tokens.as_ref().map(TokenDescription::compile).transpose()?,
                    bind: s.bind.iter().map(BindingDescription::compile).collect::<Result<_, String>>()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let p = TirPipelineV1 { version: 1, stages, output_stage: self.output_stage };
        TirPipelineV1::decode_canonical(&p.encode(), programs).map_err(|e| format!("COMPLETE_FRONTEND_PIPELINE_ADMISSION: {e}"))
    }
}
pub(super) fn bytes(path: &Path, cap: u64) -> Result<Vec<u8>, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if f.metadata().map_err(|e| e.to_string())?.len() > cap {
        return Err("COMPLETE_FRONTEND_INPUT_BYTE_BOUND".into());
    }
    let mut b = Vec::new();
    f.take(cap + 1).read_to_end(&mut b).map_err(|e| e.to_string())?;
    if b.len() as u64 > cap {
        return Err("COMPLETE_FRONTEND_INPUT_GREW".into());
    }
    Ok(b)
}
pub(super) fn decode<const N: usize>(s: &str) -> Result<[u8; N], String> {
    misaka_palw_tir_lower::frontend_pack::program::unhex(s, N)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "COMPLETE_FRONTEND_DIGEST_LENGTH".into())
}
pub(super) fn relative(root: &Path, name: &str) -> Result<PathBuf, String> {
    let p = Path::new(name);
    if p.is_absolute() || p.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err("COMPLETE_FRONTEND_RELATIVE_PATH_REQUIRED".into());
    }
    let p = root.join(p).canonicalize().map_err(|e| e.to_string())?;
    if !p.starts_with(root) {
        return Err("COMPLETE_FRONTEND_INPUT_ESCAPE".into());
    }
    Ok(p)
}
struct Subset<'a> {
    source: &'a dyn TensorSource,
    names: BTreeSet<String>,
}
impl TensorSource for Subset<'_> {
    fn validate_snapshot(&self) -> misaka_palw_tir_lower::Result<()> {
        self.source.validate_snapshot()
    }
    fn shape(&self, n: &str) -> Option<Vec<usize>> {
        self.names.contains(n).then(|| self.source.shape(n)).flatten()
    }
    fn load(&self, n: &str) -> misaka_palw_tir_lower::Result<Tensor> {
        if !self.names.contains(n) {
            return Err(misaka_palw_tir_lower::LowerError::weights("undeclared component tensor"));
        }
        self.source.load(n)
    }
    fn names(&self) -> Vec<String> {
        self.names.iter().cloned().collect()
    }
    fn metadata(&self, n: &str) -> Option<TensorMeta> {
        self.names.contains(n).then(|| self.source.metadata(n)).flatten()
    }
    fn read_slice(&self, n: &str, r: Range<u64>) -> misaka_palw_tir_lower::Result<Vec<u8>> {
        if !self.names.contains(n) {
            return Err(misaka_palw_tir_lower::LowerError::weights("undeclared component tensor"));
        }
        self.source.read_slice(n, r)
    }
}
struct TensorWriter<'a> {
    file: &'a mut std::fs::File,
    remaining: u64,
}
impl Write for TensorWriter<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if b.len() as u64 > self.remaining {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "COMPLETE_FRONTEND_TENSOR_WRITE_EXCEEDS_DECLARATION"));
        }
        let n = self.file.write(b)?;
        self.remaining -= n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
struct TensorDirectory {
    directory: PathBuf,
    stage: u16,
}
pub(super) fn tensor_name(stage: u16, param: u16, layer: Option<u16>) -> String {
    format!("stage-{stage}-param-{param}-layer-{}.bin", layer.map(|v| v.to_string()).unwrap_or("global".into()))
}
impl crate::tir_stream::PalwTirRangeSourceV1 for TensorDirectory {
    fn read_range(&self, j: u16, l: Option<u16>, r: Range<u64>, out: &mut [u8]) -> Result<(), String> {
        let mut f = std::fs::File::open(self.directory.join(tensor_name(self.stage, j, l))).map_err(|e| e.to_string())?;
        if r.end < r.start || r.end - r.start != out.len() as u64 || r.end > f.metadata().map_err(|e| e.to_string())?.len() {
            return Err("COMPLETE_FRONTEND_REAL_TENSOR_RANGE".into());
        }
        f.seek(SeekFrom::Start(r.start)).map_err(|e| e.to_string())?;
        f.read_exact(out).map_err(|e| e.to_string())
    }
}
fn input_bindings(pipeline: &TirPipelineV1, programs: &[TirProgramV2]) -> Result<Vec<ComponentInputBindingV28>, String> {
    let mut rows = Vec::new();
    let mut external = 0u16;
    for (s, p) in programs.iter().enumerate() {
        let mut bindings = pipeline.stages[s].bind.iter();
        for (i, input) in p.inputs.iter().enumerate() {
            let source = match input.source {
                InputSource::Random { domain, dist, per_step } => ComponentInputSourceV28::Random {
                    domain,
                    distribution: object_id(b"misaka-palw/kernel/original-stage-random-source/v28", &(domain, dist, per_step)),
                },
                InputSource::External { .. } => {
                    let b = bindings.next().ok_or("COMPLETE_FRONTEND_INPUT_BINDING_ABSENT")?;
                    match b {
                        Binding::StageRows { stage, .. } | Binding::StageFinal { stage } | Binding::StageRowCount { stage, .. } => {
                            ComponentInputSourceV28::Upstream {
                                stage: *stage as u16,
                                output_declaration: stage_output_declaration_root_v28(&programs[*stage as usize]),
                            }
                        }
                        _ => {
                            let job_input = external;
                            external = external.checked_add(1).ok_or("COMPLETE_FRONTEND_EXTERNAL_INPUT_OVERFLOW")?;
                            ComponentInputSourceV28::External {
                                job_input,
                                declaration: object_id(
                                    b"misaka-palw/sdk/original-complete-external-binding/v28",
                                    &(s as u16, i as u16, b),
                                ),
                            }
                        }
                    }
                }
            };
            rows.push(ComponentInputBindingV28 { stage: s as u16, input: i as u16, source });
        }
        if bindings.next().is_some() {
            return Err("COMPLETE_FRONTEND_SURPLUS_INPUT_BINDING".into());
        }
    }
    Ok(rows)
}
/// All work happens from the exact pinned declaration and original checkpoint bytes.
/// Output is fresh and contains the actual canonical programs/tensors/PC/R, never a
/// supplied stage commitment. Failure retains partial diagnostic files, no completion marker.
pub fn compile_complete_frontend_v28(
    plan_path: &Path,
    plan_sha: [u8; 32],
    source_directory: &Path,
    output: &Path,
) -> Result<serde_json::Value, String> {
    if output.exists() {
        return Err("COMPLETE_FRONTEND_FRESH_OUTPUT_REQUIRED".into());
    }
    let raw = bytes(plan_path, 16 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != plan_sha {
        return Err("COMPLETE_FRONTEND_ORIGINAL_DECLARATION_SHA".into());
    }
    let plan: CompleteFrontendV28 = serde_json::from_slice(&raw).map_err(|e| format!("COMPLETE_FRONTEND_GRAMMAR: {e}"))?;
    if plan.schema != "misaka.palw.source-backed-complete-frontend.v28"
        || plan.stages.is_empty()
        || plan.stages.len() > MAX_STAGES
        || plan.stages.len() != plan.original.original_contexts.len()
        || plan.original.components.is_empty()
        || plan.original.components.len() > MAX_COMPONENTS_V28
        || plan.original.advertised_task.is_empty()
        || plan.original.source_files.len() > MAX_SOURCE_FILES_V28
        || plan.original.source_files.is_empty()
        || !(8..=16 << 20).contains(&plan.block_bytes)
        || plan.maximum_file_bytes == 0
        || plan.maximum_file_bytes > MAX_SOURCE_FILE_BYTES_V28
        || plan.tensor_workspace_bytes == 0
        || plan.tensor_workspace_bytes > 256 << 20
    {
        return Err("COMPLETE_FRONTEND_DECLARATION_BOUND".into());
    }
    let source_directory = source_directory.canonicalize().map_err(|e| e.to_string())?;
    let package = plan_path.canonicalize().map_err(|e| e.to_string())?.parent().ok_or("COMPLETE_FRONTEND_PLAN_PARENT")?.to_path_buf();
    let mut pins = Vec::new();
    let mut pin_names = BTreeSet::new();
    let mut source_total = 0u64;
    for f in &plan.original.source_files {
        if !pin_names.insert(f.path.clone()) {
            return Err("COMPLETE_FRONTEND_DUPLICATE_SOURCE_FILE".into());
        }
        let path = relative(&source_directory, &f.path)?;
        let sha = decode::<32>(&f.sha256)?;
        source_total = source_total.checked_add(f.bytes).ok_or("COMPLETE_FRONTEND_SOURCE_EXTENT_OVERFLOW")?;
        if source_total > MAX_SOURCE_FILE_BYTES_V28
            || path.metadata().map_err(|e| e.to_string())?.len() != f.bytes
            || hash(&path, f.bytes)? != sha
        {
            return Err("COMPLETE_FRONTEND_ORIGINAL_SOURCE_FILE_DIFFERS".into());
        }
        pins.push(OriginalFilePinV28 { path: f.path.clone(), bytes: f.bytes, sha256: sha });
    }
    pins.sort_by(|a, b| a.path.cmp(&b.path));
    let task = relative(&source_directory, &plan.original.task_metadata_file)?;
    if !pin_names.contains(&plan.original.task_metadata_file)
        || hash(&task, 4 << 20)? != decode::<32>(&plan.original.task_metadata_sha256)?
    {
        return Err("COMPLETE_FRONTEND_TASK_METADATA_PIN".into());
    }
    let task_bytes = bytes(&task, 4 << 20)?;
    let task_metadata = object_id(b"misaka-palw/sdk/original-task-metadata/v28", &(plan.original.advertised_task.clone(), task_bytes));
    if !pin_names.contains(&plan.tokenizer_file) {
        return Err("COMPLETE_FRONTEND_TOKENIZER_NOT_IN_ORIGINAL_SOURCE".into());
    }
    let tokenizer_path = relative(&source_directory, &plan.tokenizer_file)?;
    let tokenizer = misaka_palw_tir_lower::artifact::tokenizer_id_of(&bytes(&tokenizer_path, 64 << 20)?);
    let mut programs = Vec::new();
    let mut compiled = Vec::new();
    let mut vision_prepared = Vec::new();
    let mut decoder_prepared = Vec::new();
    let mut raw_sources = Vec::new();
    let mut selected_names = Vec::new();
    let mut available = BTreeSet::new();
    let mut consumed = BTreeSet::new();
    let mut provenance = Vec::new();
    let mut configs = BTreeMap::<String, (serde_json::Value, Vec<String>)>::new();
    let mut compiler_inputs = Vec::<(PathBuf, [u8; 32], u64)>::new();
    let mut acquired_checkpoint_files = BTreeSet::new();
    // Structural admission of every component precedes output/tensor allocation.
    for (i, s) in plan.stages.iter().enumerate() {
        let canonical = if s.canonical_v2.is_empty() {
            if s.canonical_sha256.is_some() {
                return Err("COMPLETE_FRONTEND_UNUSED_CANONICAL_PIN".into());
            }
            None
        } else {
            let path = relative(&package, &s.canonical_v2)?;
            let sha = decode::<32>(s.canonical_sha256.as_deref().ok_or("COMPLETE_FRONTEND_CANONICAL_SHA_REQUIRED")?)?;
            let cap = misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64;
            if hash(&path, cap)? != sha {
                return Err("COMPLETE_FRONTEND_CANONICAL_SHA_DIFFERS".into());
            }
            compiler_inputs.push((path.clone(), sha, cap));
            Some(TirProgramV2::decode_canonical(&bytes(&path, cap)?).map_err(|e| e.to_string())?)
        };
        let semantics = s.semantics.clone().or_else(|| canonical.as_ref().map(StageSemanticsV28::of_program));
        let mathematical = s.vision38.is_some() || s.decoder38.is_some();
        if s.vision38.is_some() && s.decoder38.is_some() {
            return Err("COMPLETE_FRONTEND_AMBIGUOUS_MATH_COMPILER".into());
        }
        if !mathematical && semantics.is_none() {
            return Err("COMPLETE_FRONTEND_STAGE_SEMANTICS_REQUIRED".into());
        }
        if mathematical
            && (!s.frontend_pack.is_empty()
                || canonical.is_some()
                || s.semantics.is_some()
                || !s.source_bindings.is_null()
                || !s.config_pointer.is_empty())
        {
            return Err("COMPLETE_FRONTEND_AMBIGUOUS_VISION38_SOURCE_COMPILER".into());
        }
        let pack = if mathematical {
            FrontendPack::parse(&serde_json::json!({"format":misaka_palw_tir_lower::frontend_pack::FORMAT,"id":"explicit-source-backed-pixtral38",
            "scope":{"task":plan.original.advertised_task,"completeness":"full","components":plan.original.components.iter().filter(|c|c.stages.contains(&(i as u16))).map(|c|&c.name).collect::<Vec<_>>()},"program":null,"bindings":[]}).to_string()).map_err(|e|e.to_string())?
        } else if !s.frontend_pack.is_empty() {
            if !s.source_bindings.is_null() {
                return Err("COMPLETE_FRONTEND_AMBIGUOUS_DIRECT_IMPORT".into());
            }
            let path = relative(&package, &s.frontend_pack)?;
            let sha = decode::<32>(s.frontend_sha256.as_deref().ok_or("COMPLETE_FRONTEND_PACK_SHA_REQUIRED")?)?;
            let cap = misaka_palw_tir_lower::frontend_pack::MAX_PACK_BYTES as u64;
            if hash(&path, cap)? != sha {
                return Err("COMPLETE_FRONTEND_PACK_SHA_DIFFERS".into());
            }
            compiler_inputs.push((path.clone(), sha, cap));
            FrontendPack::read(&path).map_err(|e| e.to_string())?
        } else {
            if s.frontend_sha256.is_some() {
                return Err("COMPLETE_FRONTEND_UNUSED_FRONTEND_PIN".into());
            }
            let p = canonical.as_ref().ok_or("COMPLETE_FRONTEND_CANONICAL_OR_FRONTEND_REQUIRED")?;
            let definition = serde_json::json!({"format":misaka_palw_tir_lower::frontend_pack::FORMAT,"id":"canonical-source28-import",
                "scope":{"task":plan.original.advertised_task,"completeness":"full","components":plan.original.components.iter().filter(|c|c.stages.contains(&(i as u16))).map(|c|&c.name).collect::<Vec<_>>()},
                "program":misaka_palw_tir_lower::frontend_pack::program::Program::of_v2(p),"bindings":s.source_bindings});
            FrontendPack::parse(&definition.to_string()).map_err(|e| e.to_string())?
        };
        let expected_components =
            plan.original.components.iter().filter(|c| c.stages.contains(&(i as u16))).map(|c| c.name.clone()).collect::<Vec<_>>();
        if pack.scope().task != plan.original.advertised_task || pack.scope().components != expected_components {
            return Err("COMPLETE_FRONTEND_STAGE_TASK_OR_REQUIRED_COMPONENTS_DIFFER".into());
        }
        let source_path = if s.checkpoint == "." { source_directory.clone() } else { relative(&source_directory, &s.checkpoint)? };
        let source = FrontendSource::open(&source_path, &pack).map_err(|e| e.to_string())?;
        for path in source.files() {
            let path = path.canonicalize().map_err(|e| e.to_string())?;
            let rel = path
                .strip_prefix(&source_directory)
                .map_err(|_| "COMPLETE_FRONTEND_CHECKPOINT_ESCAPE")?
                .to_str()
                .ok_or("COMPLETE_FRONTEND_NONUTF8_SOURCE")?;
            if !pin_names.contains(rel) {
                return Err("COMPLETE_FRONTEND_UNPINNED_CHECKPOINT_SHARD".into());
            }
            acquired_checkpoint_files.insert(rel.to_owned());
        }
        let config_raw = if s.config_file.is_empty() {
            serde_json::json!({})
        } else {
            if !pin_names.contains(&s.config_file) {
                return Err("COMPLETE_FRONTEND_UNPINNED_CONFIG".into());
            }
            serde_json::from_slice::<serde_json::Value>(&bytes(&relative(&source_directory, &s.config_file)?, 64 << 20)?)
                .map_err(|e| e.to_string())?
        };
        let config = source
            .configuration(config_raw.pointer(&s.config_pointer).ok_or("COMPLETE_FRONTEND_CONFIG_POINTER")?.clone())
            .map_err(|e| e.to_string())?;
        if !s.config_file.is_empty() {
            let row = configs.entry(s.config_file.clone()).or_insert_with(|| (config_raw.clone(), Vec::new()));
            if row.0 != config_raw {
                return Err("COMPLETE_FRONTEND_CONFIG_CHANGED_DURING_COMPILATION".into());
            }
            row.1.push(s.config_pointer.clone());
        }
        let names = s.tensor_names.iter().cloned().collect::<BTreeSet<_>>();
        if names.len() != s.tensor_names.len() {
            return Err("COMPLETE_FRONTEND_DUPLICATE_STAGE_TENSOR".into());
        }
        for name in source.names() {
            available.insert(format!("{}::{name}", s.checkpoint));
        }
        for name in &names {
            if source.metadata(name).is_none() {
                return Err("COMPLETE_FRONTEND_ORIGINAL_TENSOR_ABSENT".into());
            }
            consumed.insert(format!("{}::{name}", s.checkpoint));
        }
        let subset = Subset { source: &source, names: names.clone() };
        let (program, stage, vision, decoder, read_names) = if let Some(profile) = &s.vision38 {
            let path = relative(&package, &profile.statistics_file)?;
            let sha = decode::<32>(&profile.statistics_sha256)?;
            if hash(&path, 64 << 20)? != sha {
                return Err("COMPLETE_FRONTEND_VISION38_CALIBRATION_PIN_DIFFERS".into());
            }
            compiler_inputs.push((path.clone(), sha, 64 << 20));
            let stats = bytes(&path, 64 << 20)?;
            let prepared = super::vision38::PreparedVisionV38::prepare(
                &config,
                profile,
                std::str::from_utf8(&stats).map_err(|e| e.to_string())?,
            )?;
            (prepared.program.clone(), None, Some(prepared), None, s.tensor_names.clone())
        } else if let Some(profile) = &s.decoder38 {
            if profile.original_context != plan.original.original_contexts[i] {
                return Err("COMPLETE_FRONTEND_DECODER_ORIGINAL_CONTEXT_DIFFERS".into());
            }
            let path = relative(&package, &profile.statistics_file)?;
            let sha = decode::<32>(&profile.statistics_sha256)?;
            if hash(&path, 64 << 20)? != sha {
                return Err("COMPLETE_FRONTEND_DECODER38_CALIBRATION_PIN_DIFFERS".into());
            }
            compiler_inputs.push((path.clone(), sha, 64 << 20));
            let stats = bytes(&path, 64 << 20)?;
            let prepared = super::decoder38::PreparedDecoderV38::prepare(
                &config,
                profile,
                std::str::from_utf8(&stats).map_err(|e| e.to_string())?,
            )?;
            if let Some(image) = &profile.image_rows {
                let upstream = image.upstream_stage as usize;
                if upstream >= i
                    || !matches!(plan.pipeline.stages[i].bind.as_slice(),[BindingDescription::StageFinal{stage}] if *stage as usize==upstream)
                {
                    return Err("COMPLETE_FRONTEND_DECODER38_IMAGE_NOT_BOUND_TO_ORIGINAL_PRIOR_STAGE".into());
                }
                let source: &TirProgramV2 = &programs[upstream];
                let value = &source.blocks[source.schedule.post as usize].nodes[source.output.node() as usize].out;
                if !matches!(source.output, misaka_palw_tir::program_v2::OutputDecl::Final { .. })
                    || value.dtype != misaka_palw_tir::DType::I32
                    || value.shape != vec![misaka_palw_tir::Dim::Fixed(image.rows), misaka_palw_tir::Dim::Fixed(image.width)]
                {
                    return Err("COMPLETE_FRONTEND_DECODER38_IMAGE_OUTPUT_SHAPE_OR_KIND_DIFFERS".into());
                }
            }
            (prepared.program.clone(), None, None, Some(prepared), s.tensor_names.clone())
        } else {
            let stage = pack
                .compile_stage_v28(
                    &config,
                    &subset,
                    semantics.as_ref().ok_or("COMPLETE_FRONTEND_STAGE_SEMANTICS_REQUIRED")?,
                    plan.block_bytes,
                )
                .map_err(|e| e.to_string())?;
            (stage.program().clone(), Some(stage), None, None, Vec::from_iter(names.iter().cloned()))
        };
        if canonical.as_ref().is_some_and(|p| p != &program) {
            return Err("COMPLETE_FRONTEND_INDEPENDENT_CANONICAL_PROGRAM_DIFFERS".into());
        }
        if plan.original.original_contexts[i] == 0 || program.history_bound < plan.original.original_contexts[i] {
            return Err("COMPLETE_FRONTEND_ORIGINAL_CONTEXT_NOT_LOWERED".into());
        }
        provenance.push(serde_json::json!({"stage":i,"frontend_pack":s.frontend_pack,"frontend_hash":pack.hash(),"config_file":s.config_file,"config_pointer":s.config_pointer,"source_tensor_names":read_names,"original_context_max":plan.original.original_contexts[i]}));
        misaka_palw_tir::interval_v2::analyze_ranges_v2(&program).map_err(|e| format!("COMPLETE_FRONTEND_RANGE_ADMISSION: {e}"))?;
        let mut input_raw = 0u128;
        let mut input_reference = 0u128;
        for input in &program.inputs {
            let cells = input
                .shape
                .iter()
                .try_fold(1u128, |n, d| n.checked_mul(*d as u128))
                .ok_or("COMPLETE_FRONTEND_INPUT_PRODUCT_OVERFLOW")?;
            input_raw = input_raw
                .checked_add(cells.checked_mul(input.dtype.width() as u128).ok_or("COMPLETE_FRONTEND_INPUT_BYTES_OVERFLOW")?)
                .ok_or("COMPLETE_FRONTEND_INPUT_BYTES_OVERFLOW")?;
            input_reference = input_reference
                .checked_add(cells.checked_mul(16).ok_or("COMPLETE_FRONTEND_INPUT_BYTES_OVERFLOW")?)
                .ok_or("COMPLETE_FRONTEND_INPUT_BYTES_OVERFLOW")?;
        }
        if program.inputs.len() > misaka_palw_kernel::flat_inputs12::INPUT_COUNT_MAX_V12
            || input_raw > misaka_palw_kernel::flat_inputs12::INPUT_RAW_BYTES_MAX_V12
            || input_reference > misaka_palw_kernel::flat_inputs12::INPUT_REFERENCE_BYTES_MAX_V12
        {
            return Err("COMPLETE_FRONTEND_SOURCE28_INPUT_BOUND".into());
        }
        let mut instances = 0usize;
        for d in &program.params {
            instances = instances
                .checked_add(if d.per_layer { program.schedule.layers.len() } else { 1 })
                .ok_or("COMPLETE_FRONTEND_PC_COUNT_OVERFLOW")?;
            let shape = d.shape.iter().map(|d| *d as usize).collect::<Vec<_>>();
            if misaka_palw_kernel::merkle3_stream::streamed_commitment_shape_v3(d.dtype, &shape)?.workspace_bytes
                > plan.tensor_workspace_bytes
            {
                return Err("COMPLETE_FRONTEND_REAL_PC_WORKSPACE_BEFORE_CONVERSION".into());
            }
        }
        if instances > misaka_palw_kernel::flat_inputs12::SOURCE_PC_INSTANCES_MAX_V12 {
            return Err("COMPLETE_FRONTEND_SOURCE28_PC_COUNT_BOUND".into());
        }
        programs.push(program);
        compiled.push(stage);
        vision_prepared.push(vision);
        decoder_prepared.push(decoder);
        raw_sources.push(source);
        selected_names.push(names);
    }
    fn leaves(value: &serde_json::Value, pointer: &str, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) if !map.is_empty() => {
                for (k, v) in map {
                    leaves(v, &format!("{pointer}/{}", k.replace('~', "~0").replace('/', "~1")), out);
                }
            }
            serde_json::Value::Array(a) if !a.is_empty() => {
                for (i, v) in a.iter().enumerate() {
                    leaves(v, &format!("{pointer}/{i}"), out);
                }
            }
            _ => out.push(pointer.into()),
        }
    }
    for (file, (config, mut covered)) in configs {
        if let Some(inert) = plan.config_inert.get(&file) {
            for p in inert {
                if config.pointer(p).is_none() {
                    return Err("COMPLETE_FRONTEND_UNKNOWN_INERT_CONFIG_POINTER".into());
                }
                covered.push(p.clone());
            }
        }
        let mut paths = Vec::new();
        leaves(&config, "", &mut paths);
        if paths
            .iter()
            .any(|p| !covered.iter().any(|c| c.is_empty() || p == c || p.strip_prefix(c).is_some_and(|s| s.starts_with('/'))))
        {
            return Err(format!("COMPLETE_FRONTEND_CONFIG_UNREAD_OUTSIDE_STAGE: {file}"));
        }
    }
    if plan.config_inert.keys().any(|file| !plan.stages.iter().any(|s| &s.config_file == file)) {
        return Err("COMPLETE_FRONTEND_UNUSED_INERT_CONFIG_FILE".into());
    }
    fn weight_file(path: &str) -> bool {
        path.ends_with(".safetensors") || path.ends_with(".gguf") || path.ends_with(".bin") || path.ends_with(".palwq36")
    }
    if pin_names.iter().any(|name| weight_file(name) && !acquired_checkpoint_files.contains(name)) {
        return Err("COMPLETE_FRONTEND_PINNED_WEIGHT_SHARD_WAS_NOT_ACQUIRED".into());
    }
    // Discover unpinned original payloads too: a caller cannot omit one source shard
    // from both its claimed tensor inventory and its selected checkpoint list.
    let mut pending = vec![source_directory.clone()];
    let mut found_files = 0usize;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            found_files = found_files.checked_add(1).ok_or("COMPLETE_FRONTEND_SOURCE_DIRECTORY_OVERFLOW")?;
            if found_files > 4096 {
                return Err("COMPLETE_FRONTEND_SOURCE_DIRECTORY_BOUND".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_symlink() {
                return Err("COMPLETE_FRONTEND_SOURCE_DIRECTORY_SYMLINK".into());
            } else if kind.is_file() {
                let path = entry.path();
                let name = path
                    .strip_prefix(&source_directory)
                    .map_err(|_| "COMPLETE_FRONTEND_SOURCE_DIRECTORY_ESCAPE")?
                    .to_str()
                    .ok_or("COMPLETE_FRONTEND_NONUTF8_SOURCE")?;
                if weight_file(name) && (!pin_names.contains(name) || !acquired_checkpoint_files.contains(name)) {
                    return Err("COMPLETE_FRONTEND_UNDECLARED_OR_UNACQUIRED_WEIGHT_PAYLOAD".into());
                }
                let base = path.file_name().and_then(|v| v.to_str()).ok_or("COMPLETE_FRONTEND_NONUTF8_SOURCE")?;
                let source_metadata = name.ends_with(".index.json")
                    || matches!(
                        base,
                        "config.json"
                            | "generation_config.json"
                            | "tokenizer.json"
                            | "tokenizer_config.json"
                            | "special_tokens_map.json"
                            | "preprocessor_config.json"
                            | "processor_config.json"
                            | "image_processor_config.json"
                            | "chat_template.jinja"
                            | "chat_template.json"
                            | "vocab.json"
                            | "merges.txt"
                            | "tokenizer.model"
                    );
                if source_metadata && !pin_names.contains(name) {
                    return Err("COMPLETE_FRONTEND_UNPINNED_ORIGINAL_PREPROCESSING_OR_TASK_METADATA".into());
                }
            }
        }
    }
    let declared = plan.original.complete_tensor_inventory.iter().cloned().collect::<BTreeSet<_>>();
    if declared.len() != plan.original.complete_tensor_inventory.len() || declared != available || declared != consumed {
        return Err("COMPLETE_FRONTEND_ORIGINAL_TENSOR_INVENTORY_INCOMPLETE".into());
    }
    let pipeline = plan.pipeline.compile(&programs)?;
    if pipeline.stages.iter().zip(&plan.original.original_contexts).any(|(s, c)| s.max_trip != *c) {
        return Err("COMPLETE_FRONTEND_PIPELINE_REDUCES_ORIGINAL_CONTEXT".into());
    }
    let components = plan
        .original
        .components
        .iter()
        .map(|c| {
            Ok(SourceComponentV28 {
                name: c.name.clone(),
                source_files: c
                    .source_files
                    .iter()
                    .map(|p| {
                        pins.iter()
                            .position(|f| &f.path == p)
                            .map(|n| n as u16)
                            .ok_or("COMPLETE_FRONTEND_COMPONENT_FILE_ABSENT".to_string())
                    })
                    .collect::<Result<_, _>>()?,
                stages: c.stages.clone(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let required = components.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    let inputs = input_bindings(&pipeline, &programs)?;
    let mut covered_files = BTreeSet::new();
    let mut covered_stages = BTreeSet::new();
    if !required.windows(2).all(|w| w[0] < w[1]) {
        return Err("COMPLETE_FRONTEND_COMPONENT_ORDER_OR_DUPLICATE".into());
    }
    for c in &components {
        if c.name.is_empty()
            || c.name.len() > 128
            || c.source_files.is_empty()
            || !c.source_files.windows(2).all(|w| w[0] < w[1])
            || !c.stages.windows(2).all(|w| w[0] < w[1])
        {
            return Err("COMPLETE_FRONTEND_COMPONENT_DECLARATION".into());
        }
        for f in &c.source_files {
            covered_files.insert(*f as usize);
        }
        for s in &c.stages {
            if *s as usize >= programs.len() {
                return Err("COMPLETE_FRONTEND_COMPONENT_STAGE_ABSENT".into());
            }
            covered_stages.insert(*s as usize);
        }
    }
    if covered_files.len() != pins.len() || covered_stages.len() != programs.len() {
        return Err("COMPLETE_FRONTEND_ORPHAN_ORIGINAL_FILE_OR_STAGE".into());
    }
    // Exact framing estimate has fixed-width digests; no placeholder grants source authority.
    let framing = CompleteComponentManifestV28 {
        version: 28,
        repository: plan.original.repository.clone(),
        revision: decode(&plan.original.revision)?,
        original_task_metadata: task_metadata,
        original_required_components: required.clone(),
        source_files: pins.clone(),
        components: components.clone(),
        stages: programs
            .iter()
            .enumerate()
            .map(|(i, p)| ComponentStageV28 {
                original_program: stage_program_root_v28(p),
                original_inventory: [0; 64],
                input_declarations: stage_input_declarations_root_v28(p),
                output_declaration: stage_output_declaration_root_v28(p),
                original_context_max: plan.original.original_contexts[i],
                lowered_context_max: plan.original.original_contexts[i],
            })
            .collect(),
        inputs: inputs.clone(),
    };
    let mut bundle_bytes = 10u64 + framing.to_bytes().map_err(|e| e.to_string())?.len() as u64;
    for p in &programs {
        let keys = p
            .params
            .iter()
            .try_fold(0u64, |n, d| n.checked_add(if d.per_layer { 69 * p.schedule.layers.len() as u64 } else { 67 }))
            .ok_or("COMPLETE_FRONTEND_BUNDLE_COUNT_OVERFLOW")?;
        bundle_bytes =
            bundle_bytes.checked_add(78 + p.encode().len() as u64 + keys).ok_or("COMPLETE_FRONTEND_BUNDLE_BYTES_OVERFLOW")?;
    }
    if bundle_bytes > 16 << 20 {
        return Err("COMPLETE_FRONTEND_COMPLETE_SOURCE_BUNDLE_BOUND_BEFORE_CONVERSION".into());
    }
    let mut total = 0u64;
    for p in &programs {
        for d in &p.params {
            let cells =
                d.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("COMPLETE_FRONTEND_TENSOR_PRODUCT_OVERFLOW")?;
            let count = if d.per_layer { p.schedule.layers.len() as u64 } else { 1 };
            total = total
                .checked_add(
                    cells
                        .checked_mul(d.dtype.width() as u64)
                        .and_then(|v| v.checked_mul(count))
                        .ok_or("COMPLETE_FRONTEND_TENSOR_EXTENT_OVERFLOW")?,
                )
                .ok_or("COMPLETE_FRONTEND_TOTAL_TENSOR_OVERFLOW")?;
        }
    }
    if total > plan.maximum_file_bytes {
        return Err("COMPLETE_FRONTEND_REAL_SIZE_EXCEEDS_FILE_BUDGET".into());
    }
    std::fs::create_dir(output).map_err(|e| e.to_string())?;
    let tensor_dir = output.join("tensors");
    std::fs::create_dir(&tensor_dir).map_err(|e| e.to_string())?;
    let mut import_log = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("weight-import.jsonl"))
        .map_err(|e| e.to_string())?;
    let mut materialised_scales = vec![None; programs.len()];
    for (i, p) in programs.iter().enumerate() {
        if let Some(prepared) = &vision_prepared[i] {
            let selector = &plan.stages[i].checkpoint;
            let checkpoint = if selector == "." { source_directory.clone() } else { relative(&source_directory, selector)? };
            let report = prepared.write_tensors(
                &checkpoint,
                &selected_names[i],
                &tensor_dir,
                i as u16,
                plan.block_bytes,
                plan.maximum_file_bytes,
            )?;
            materialised_scales[i] = report["output_scale"].as_f64();
            serde_json::to_writer(&mut import_log, &report).map_err(|e| e.to_string())?;
            import_log.write_all(b"\n").map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(prepared) = &decoder_prepared[i] {
            if let Some(image) = plan.stages[i].decoder38.as_ref().and_then(|p| p.image_rows.as_ref()) {
                if materialised_scales[image.upstream_stage as usize] != Some(2f64.powi(-(image.q as i32))) {
                    return Err("COMPLETE_FRONTEND_DECODER38_UPSTREAM_IMAGE_UNIT_DIFFERS".into());
                }
            }
            let selector = &plan.stages[i].checkpoint;
            let checkpoint = if selector == "." { source_directory.clone() } else { relative(&source_directory, selector)? };
            let report = prepared.write_tensors(
                &checkpoint,
                &selected_names[i],
                &tensor_dir,
                i as u16,
                plan.block_bytes,
                plan.maximum_file_bytes,
            )?;
            materialised_scales[i] = report["output_scale"].as_f64();
            serde_json::to_writer(&mut import_log, &report).map_err(|e| e.to_string())?;
            import_log.write_all(b"\n").map_err(|e| e.to_string())?;
            continue;
        }
        let subset = Subset { source: &raw_sources[i], names: selected_names[i].clone() };
        let view = misaka_palw_tir_artifact::v2::param_view_v2(p);
        for (j, layers) in misaka_palw_tir_artifact::param_instances_v1(&view).iter().enumerate() {
            for l in layers {
                let path = tensor_dir.join(tensor_name(i as u16, j as u16, *l));
                let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&path).map_err(|e| e.to_string())?;
                let wanted = p.params[j].shape.iter().map(|v| *v as u64).product::<u64>() * p.params[j].dtype.width() as u64;
                let imported = compiled[i]
                    .as_ref()
                    .ok_or("COMPLETE_FRONTEND_MISSING_TYPED_PRODUCER")?
                    .write_tensor(&subset, j as u16, *l, &mut TensorWriter { file: &mut file, remaining: wanted }, plan.block_bytes)
                    .map_err(|e| e.to_string())?;
                serde_json::to_writer(&mut import_log, &serde_json::json!({"stage":i,"parameter":j,"layer":l,"import":imported}))
                    .map_err(|e| e.to_string())?;
                import_log.write_all(b"\n").map_err(|e| e.to_string())?;
                if file.metadata().map_err(|e| e.to_string())?.len() != wanted {
                    return Err("COMPLETE_FRONTEND_CONVERTED_TENSOR_EXTENT".into());
                }
            }
        }
    }
    let mut wires = Vec::new();
    let mut sources = Vec::new();
    let mut inventories = Vec::new();
    for (i, p) in programs.iter().enumerate() {
        let ranges = TensorDirectory { directory: tensor_dir.clone(), stage: i as u16 };
        let mut pc = misaka_palw_kernel::trace::ParamCommitmentsV1::default();
        let pv = misaka_palw_tir_artifact::v2::param_view_v2(p);
        for (j, instances) in misaka_palw_tir_artifact::param_instances_v1(&pv).iter().enumerate() {
            let d = &p.params[j];
            let shape = d.shape.iter().map(|x| *x as usize).collect::<Vec<_>>();
            for l in instances {
                let (root, _) = misaka_palw_kernel::merkle3_stream::tensor_commitment_streamed_v3(
                    d.dtype,
                    &shape,
                    plan.tensor_workspace_bytes,
                    &mut |at, out| {
                        crate::tir_stream::PalwTirRangeSourceV1::read_range(&ranges, j as u16, *l, at..at + out.len() as u64, out)
                    },
                )?;
                pc.by_instance.insert((j as u16, *l), root);
            }
        }
        let view = p.v1_view();
        let inventory = kaspa_consensus_core::palw_tir_artifact_v1::PalwTirModelInventoryV2::new_stage14(&view, p)?;
        let (r, _) = crate::tir_stream::palw_tir_model_inventory_root_streamed_v2(&inventory, &ranges)?;
        let wire =
            FlatStageSourceWireV28 { version: 28, program_bytes: p.encode(), inventory_root: r.as_bytes(), source_parameters: pc };
        sources.push(wire.check()?);
        inventories.push(r.as_bytes());
        wires.push(wire);
    }
    let manifest = CompleteComponentManifestV28 {
        version: 28,
        repository: plan.original.repository.clone(),
        revision: decode(&plan.original.revision)?,
        original_task_metadata: task_metadata,
        original_required_components: required.clone(),
        source_files: pins,
        components,
        stages: programs
            .iter()
            .enumerate()
            .map(|(i, p)| ComponentStageV28 {
                original_program: stage_program_root_v28(p),
                original_inventory: inventories[i],
                input_declarations: stage_input_declarations_root_v28(p),
                output_declaration: stage_output_declaration_root_v28(p),
                original_context_max: plan.original.original_contexts[i],
                lowered_context_max: plan.original.original_contexts[i],
            })
            .collect(),
        inputs: inputs.clone(),
    };
    let checked = check_component_manifest_v28(
        &manifest,
        &programs,
        &inventories,
        &plan.original.original_contexts,
        &required,
        task_metadata,
        &inputs,
    )
    .map_err(|e| format!("COMPLETE_FRONTEND_MANIFEST_ADMISSION: {e:?}"))?;
    let native = kaspa_consensus_core::palw_flat_stage_v28::NativeStageSourceBundleV28 {
        version: 28,
        manifest: manifest.to_bytes().map_err(|e| e.to_string())?,
        sources: wires.iter().map(|s| borsh::to_vec(s).map_err(|e| e.to_string())).collect::<Result<_, _>>()?,
    };
    let native_bytes = borsh::to_vec(&native).map_err(|e| e.to_string())?;
    kaspa_consensus_core::palw_flat_stage_v28::NativeStageSourceBundleV28::from_bytes(&native_bytes)?.check()?;
    let artifact = output.join("complete.palwtir2");
    let artifact_sha = super::produce_fresh_complete_pipeline_v28(
        &artifact,
        &source_directory,
        &pipeline,
        &programs,
        tokenizer,
        serde_json::json!({"source_backed_frontend28":{"declaration_sha256":hex(&plan_sha),"stages":provenance,"advertised_task":plan.original.advertised_task,"source_fidelity":"NOT_RUN"}}),
        &checked,
        &sources,
        plan.maximum_file_bytes,
        &mut |s, j, l, out| {
            let mut f = std::fs::File::open(tensor_dir.join(tensor_name(s, j, l))).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, out).map_err(|e| e.to_string())?;
            Ok(())
        },
    )?;
    // All source payloads and the declaration are checked again before publishing readiness.
    for pin in &manifest.source_files {
        let path = relative(&source_directory, &pin.path)?;
        if path.metadata().map_err(|e| e.to_string())?.len() != pin.bytes || hash(&path, pin.bytes)? != pin.sha256 {
            return Err("COMPLETE_FRONTEND_SOURCE_MUTATED".into());
        }
    }
    for (path, sha, cap) in compiler_inputs {
        if hash(&path, cap)? != sha {
            return Err("COMPLETE_FRONTEND_COMPILER_INPUT_MUTATED".into());
        }
    }
    if <[u8; 32]>::from(Sha256::digest(bytes(plan_path, 16 << 20)?)) != plan_sha {
        return Err("COMPLETE_FRONTEND_DECLARATION_MUTATED".into());
    }
    std::fs::write(output.join("source-bundle.borsh"), native_bytes).map_err(|e| e.to_string())?;
    std::fs::write(output.join("manifest.borsh"), manifest.to_bytes().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::write(output.join("pipeline.borsh"), pipeline.encode()).map_err(|e| e.to_string())?;
    for (i, p) in programs.iter().enumerate() {
        std::fs::write(output.join(format!("stage-{i}.tir2")), p.encode()).map_err(|e| e.to_string())?;
    }
    import_log.flush().map_err(|e| e.to_string())?;
    let report = serde_json::json!({"schema":"misaka.palw.source-backed-complete-frontend-result.v28","original_declaration_sha256":hex(&plan_sha),"artifact":artifact,"artifact_sha256":hex(&artifact_sha),"component_manifest":hex(&checked.root()),"stages":programs.len(),"real_tensor_bytes":total,"weight_import_log_sha256":hex(&hash(&output.join("weight-import.jsonl"),plan.maximum_file_bytes)?),"complete_original_tensor_inventory":true,"source_fidelity":"NOT_RUN","native_final_or_A6_A8_credit":false});
    std::fs::write(output.join("conversion.json"), serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(report)
}
