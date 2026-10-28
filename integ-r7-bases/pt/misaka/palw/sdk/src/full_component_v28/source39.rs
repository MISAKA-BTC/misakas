//! Source-backed complete Program39 compiler. Original and candidate parameter
//! streams are separate passes. The first pass retains at most one tensor;
//! every candidate C_v3 must match before a complete-source marker is written.
pub mod master_request41;
pub mod request41;
use super::{
    decoder38::DecoderFrontendV38,
    frontend::{OriginalSource, PipelineDescription, TripDescription, bytes, decode, relative, tensor_name},
    hash,
    vision38::{VisionFrontendV38, write_materialised_stage_v39},
};
use misaka_palw_kernel::{
    flat_component_source39::*,
    flat_pipeline39::PipelineV39,
    flat_source39::{FlatSourceWireV39, OriginalTensorV39, derive_inventory_root_v39},
    hash::{Digest, object_id},
    merkle3_stream::{streamed_commitment_shape_v3, tensor_commitment_streamed_v3},
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::{
    pipeline::{Binding, StageDecl, TripRule},
    program_v2::InputSource,
    program_v39::{DimV39, SourceProfileV39, TirProgramV39},
};
use misaka_palw_tir_lower::{
    float_ref::SiteStat,
    hl::HlProgram,
    lower::{self, ImageRows, LowerOpts, StreamOpts},
    quant::QuantPolicy,
    weights::{Binding as WeightBinding, Checkpoint, TensorSource},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterImportV39 {
    pub parameter: u16,
    pub layer: Option<u16>,
    pub source: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrontendCompilerV39 {
    FrontendPack {
        pack_file: String,
        pack_sha256: String,
    },
    Vision {
        profile: VisionFrontendV38,
        processor_stage: Option<u16>,
    },
    Processor {
        profile: lower::pixtral_processor41::ProcessorProfileV41,
    },
    ImageJoin {
        upstream_stages: Vec<u16>,
        output_scale_q: u8,
    },
    Decoder {
        profile: DecoderFrontendV38,
    },
    /// Public canonical39 requires only actual typed source ranges. No HF class,
    /// model/compiler registry, prepared commitment or publisher allowlist exists.
    Canonical {
        program_file: String,
        program_sha256: String,
        imports: Vec<ParameterImportV39>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageFrontendV39 {
    pub checkpoint: String,
    pub config_file: String,
    pub tensor_names: Vec<String>,
    pub compiler: FrontendCompilerV39,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteFrontendV39 {
    pub schema: String,
    pub original: OriginalSource,
    pub stages: Vec<StageFrontendV39>,
    pub pipeline: PipelineDescription,
    pub tokenizer_file: String,
    pub block_bytes: usize,
    pub maximum_output_bytes: u64,
    pub commitment_workspace_bytes: u64,
}
enum RecipeV39 {
    Vision(lower::vision::LoweredVisionV39),
    Decoder(lower::source39::LoweredSourceV39),
}
struct PreparedSourceV39 {
    program: TirProgramV39,
    recipe: RecipeV39,
    hl: HlProgram,
    binding: WeightBinding,
    statistics: BTreeMap<String, SiteStat>,
    policy: QuantPolicy,
}
impl PreparedSourceV39 {
    fn vision(
        config: &serde_json::Value,
        p: &VisionFrontendV38,
        stats: &str,
        geometry: Option<&lower::pixtral_processor41::ProcessorGeometryV41>,
    ) -> Result<Self, String> {
        if p.version != 38 || [p.headroom16, p.headroom32, p.headroom_resid].iter().any(|n| !n.is_finite() || *n <= 0.) {
            return Err("VISION39_EXPLICIT_PROFILE_POLICY".into());
        }
        let spec =
            lower::vision::parse_vision(&config.to_string(), geometry.map(|g| (g.resized[0], g.resized[1])), Some((p.mean, p.std)))
                .map_err(|e| e.to_string())?;
        if !spec.is_pixtral_v38()
            || spec.h as u64 > config["vision_config"]["image_size"].as_u64().unwrap_or(0)
            || spec.w as u64 > config["vision_config"]["image_size"].as_u64().unwrap_or(0)
        {
            return Err("VISION39_ORIGINAL_PIXTRAL_GEOMETRY".into());
        }
        let (hl, binding) = lower::vision::hl_program(&spec).map_err(|e| e.to_string())?;
        if !binding.ignored_prefixes.is_empty() {
            return Err("VISION39_IGNORED_ORIGINAL_COMPONENT".into());
        }
        let lowered =
            lower::vision::lower_vision_pixels_v39(&hl, &spec, SourceProfileV39::full_source(), geometry.map_or(0, |g| g.pixel_shift))
                .map_err(|e| e.to_string())?;
        let program = lowered.program.clone();
        let statistics = misaka_palw_tir_lower::calib::stats_from_json_exact(stats).map_err(|e| e.to_string())?;
        Ok(Self {
            program,
            recipe: RecipeV39::Vision(lowered),
            hl,
            binding,
            statistics,
            policy: QuantPolicy { headroom16: p.headroom16, headroom32: p.headroom32, headroom_resid: p.headroom_resid },
        })
    }
    fn decoder(config: &serde_json::Value, p: &DecoderFrontendV38, stats: &str) -> Result<Self, String> {
        use misaka_palw_tir_lower::hf_schema::{AdapterChoice, ReadOptions};
        if p.version != 38
            || p.original_context == 0
            || p.original_context > p.history_bound
            || !matches!(p.linear_weight_bits, 8 | 16)
            || p.table_shift > 8
            || [p.headroom16, p.headroom32, p.headroom_resid].iter().any(|n| !n.is_finite() || *n <= 0.)
        {
            return Err("DECODER39_EXPLICIT_PROFILE_POLICY".into());
        }
        let adapter = match p.adapter.as_str() {
            "auto" => AdapterChoice::Auto,
            "none" => AdapterChoice::None,
            s if s.starts_with("builtin:") => AdapterChoice::BuiltIn(s[8..].into()),
            _ => return Err("DECODER39_UNKNOWN_DATA_ADAPTER_USE_CANONICAL_SOURCE".into()),
        };
        let spec = misaka_palw_tir_lower::hf_config::parse_config_str_read(
            &config.to_string(),
            &ReadOptions { adapter },
            misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin(),
        )
        .map_err(|e| e.to_string())?;
        if spec.max_position_embeddings != Some(p.original_context as usize) {
            return Err("DECODER39_ORIGINAL_CONTEXT_DIFFERS".into());
        }
        if p.image_rows.is_some()
            && (spec.prefix_lm || spec.layers.iter().any(|l| matches!(l.mixer, misaka_palw_tir_lower::spec::Mixer::CrossAttention(_))))
        {
            return Err("DECODER39_PREFIX_OR_CROSS_IMAGE_SEMANTICS_REQUIRED".into());
        }
        let mut opts = LowerOpts {
            history_bound: p.history_bound,
            max_window: Some(p.original_context),
            linear_weight_bits: p.linear_weight_bits,
            gdn_core_wide: p.gdn_core_wide,
            table_shift: p.table_shift,
            ..Default::default()
        };
        let mut lifts = Vec::new();
        if let Some(image) = &p.image_rows {
            if image.rows == 0
                || image.width as usize != spec.hidden_size
                || image.q > 62
                || image.minimum > image.maximum
                || config["image_token_index"].as_u64() != Some(image.placeholder as u64)
            {
                return Err("DECODER39_ORIGINAL_IMAGE_PLACEMENT_DIFFERS".into());
            }
            opts.image_rows = Some(ImageRows {
                rows: image.rows as usize,
                width: image.width as usize,
                unit: 2f64.powi(-(image.q as i32)),
                placeholder: image.placeholder,
                mrope: None,
            });
            lifts.push((lower::IMAGE_ROWS_PARAM, InputSource::External { lo: image.minimum, hi: image.maximum }));
        }
        let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| e.to_string())?;
        let binding = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).map_err(|e| e.to_string())?;
        opts.quant = misaka_palw_tir_lower::weights::quant_layouts(&hl, &binding).map_err(|e| e.to_string())?;
        let lowered =
            lower::source39::lower_source_v39(&hl, &opts, SourceProfileV39::full_source(), &lifts).map_err(|e| e.to_string())?;
        let program = lowered.program.clone();
        let statistics = misaka_palw_tir_lower::calib::stats_from_json_exact(stats).map_err(|e| e.to_string())?;
        Ok(Self {
            program,
            recipe: RecipeV39::Decoder(lowered),
            hl,
            binding,
            statistics,
            policy: QuantPolicy { headroom16: p.headroom16, headroom32: p.headroom32, headroom_resid: p.headroom_resid },
        })
    }
    #[allow(clippy::too_many_arguments)]
    fn write(
        &self,
        checkpoint: &Path,
        allowed: &BTreeSet<String>,
        directory: &Path,
        stage: u16,
        block: usize,
        cap: u64,
        audit: bool,
        workspace: u64,
    ) -> Result<(serde_json::Value, Vec<OriginalTensorV39>), String> {
        let lifted = match &self.recipe {
            RecipeV39::Vision(v) => vec![v.lifted_parameter()],
            RecipeV39::Decoder(v) => v.lifted_parameters().to_vec(),
        };
        write_materialised_stage_v39(
            &self.hl,
            &self.binding,
            checkpoint,
            allowed,
            directory,
            stage,
            block,
            cap,
            &lifted,
            audit.then_some((&self.program, workspace)),
            &|loader, sink| {
                let opts = StreamOpts { defer_min_elems: 1, block_elems: (block / 4).max(1) };
                match &self.recipe {
                    RecipeV39::Vision(v) => {
                        v.materialise_stream(&self.hl, loader, &self.statistics, &self.policy, &opts, sink, &|_, _| {})
                    }
                    RecipeV39::Decoder(v) => {
                        v.materialise_stream(&self.hl, loader, &self.statistics, &self.policy, &opts, sink, &|_, _| {})
                    }
                }
            },
        )
    }
}
fn pipeline(description: &PipelineDescription, programs: &[TirProgramV39]) -> Result<PipelineV39, String> {
    if description.stages.len() != programs.len() {
        return Err("SOURCE39_PIPELINE_COMPONENT_COVERAGE".into());
    }
    let stages = description
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
                tokens: s.tokens.as_ref().map(|r| r.compile()).transpose()?,
                bind: s.bind.iter().map(|b| b.compile()).collect::<Result<_, String>>()?,
            })
        })
        .collect::<Result<_, String>>()?;
    let wire = PipelineV39 { version: 39, stages, output_stage: description.output_stage };
    wire.check(programs).map_err(|e| e.to_string())?;
    Ok(wire)
}
fn input_bindings(p: &PipelineV39, programs: &[TirProgramV39]) -> Result<Vec<ComponentInputBindingV39>, String> {
    let mut out = Vec::new();
    for (i, program) in programs.iter().enumerate() {
        let mut bindings = p.stages[i].bind.iter();
        for (j, input) in program.inputs.iter().enumerate() {
            let source = match input.source {
                InputSource::Random { domain, dist, per_step } => ComponentInputSourceV39::Random {
                    domain,
                    distribution: object_id(b"misaka-palw/kernel/original-stage-random-source/v39", &(domain, dist, per_step)),
                },
                InputSource::External { .. } => {
                    let b = bindings.next().ok_or("SOURCE39_PIPELINE_BINDING_ABSENT")?;
                    match b {
                        Binding::StageRows { stage, .. } | Binding::StageFinal { stage } | Binding::StageRowCount { stage, .. } => {
                            ComponentInputSourceV39::Upstream {
                                stage: *stage as u16,
                                output_declaration: stage_output_declaration_root_v39(&programs[*stage as usize]),
                            }
                        }
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
            out.push(ComponentInputBindingV39 { stage: i as u16, input: j as u16, source });
        }
        if bindings.next().is_some() {
            return Err("SOURCE39_PIPELINE_BINDING_SURPLUS".into());
        }
    }
    Ok(out)
}
fn hash_parameters(program: &TirProgramV39, directory: &Path, stage: u16, workspace: u64) -> Result<Vec<OriginalTensorV39>, String> {
    let mut out = Vec::new();
    for (j, d) in program.params.iter().enumerate() {
        let layers =
            if d.per_layer { (0..program.schedule.layers.len()).map(|l| Some(l as u16)).collect::<Vec<_>>() } else { vec![None] };
        for layer in layers {
            let path = directory.join(tensor_name(stage, j as u16, layer));
            let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
            let (root, bound) = tensor_commitment_streamed_v3(d.dtype, &shape, workspace, &mut |at, out| {
                f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
                f.read_exact(out).map_err(|e| e.to_string())
            })?;
            if f.metadata().map_err(|e| e.to_string())?.len() != bound.tensor_bytes {
                return Err("SOURCE39_CANDIDATE_PARAMETER_BYTE_EXTENT".into());
            }
            out.push(OriginalTensorV39 {
                param: j as u16,
                layer,
                dtype: d.dtype,
                shape: d.shape.clone(),
                raw_bytes: bound.tensor_bytes,
                source_tensor: root,
            });
        }
    }
    Ok(out)
}
/// Exact direct integer imports are also useful for an independent canonical
/// compiler. A float or quantized import must select an actual compiler instead
/// of reinterpreting float/packed storage as an integer parameter.
fn write_imports(
    program: &TirProgramV39,
    imports: &[ParameterImportV39],
    source: &dyn TensorSource,
    allowed: &BTreeSet<String>,
    directory: &Path,
    stage: u16,
    block: usize,
    workspace: u64,
    audit: bool,
) -> Result<Vec<OriginalTensorV39>, String> {
    let expected = program
        .params
        .iter()
        .enumerate()
        .flat_map(|(j, d)| {
            if d.per_layer {
                (0..program.schedule.layers.len()).map(|l| (j as u16, Some(l as u16))).collect::<Vec<_>>()
            } else {
                vec![(j as u16, None)]
            }
        })
        .collect::<Vec<_>>();
    if imports.iter().map(|i| (i.parameter, i.layer)).collect::<Vec<_>>() != expected
        || imports.iter().map(|i| i.source.clone()).collect::<BTreeSet<_>>() != *allowed
    {
        return Err("SOURCE39_DIRECT_IMPORT_EXACT_COVERAGE".into());
    }
    let mut records = Vec::new();
    for import in imports {
        let d = &program.params[import.parameter as usize];
        let meta = source.metadata(&import.source).ok_or("SOURCE39_DIRECT_SOURCE_TENSOR_ABSENT")?;
        if !meta.dtype.eq_ignore_ascii_case(d.dtype.name()) || meta.shape != d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>() {
            return Err("SOURCE39_DIRECT_SOURCE_DTYPE_SHAPE".into());
        }
        let (shape, source_name) = (d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>(), &import.source);
        let (root, bound) = tensor_commitment_streamed_v3(d.dtype, &shape, workspace, &mut |at, out| {
            let raw = source.read_slice(source_name, at..at + out.len() as u64).map_err(|e| e.to_string())?;
            if raw.len() != out.len() {
                return Err("SOURCE39_DIRECT_ORIGINAL_SHORT_READ".into());
            }
            out.copy_from_slice(&raw);
            Ok(())
        })?;
        if meta.bytes != bound.tensor_bytes {
            return Err("SOURCE39_DIRECT_SOURCE_BYTES".into());
        }
        if !audit {
            let mut f = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(directory.join(tensor_name(stage, import.parameter, import.layer)))
                .map_err(|e| e.to_string())?;
            let mut at = 0;
            while at < meta.bytes {
                let end = meta.bytes.min(at.checked_add(block as u64).ok_or("SOURCE39_DIRECT_RANGE_OVERFLOW")?);
                let raw = source.read_slice(source_name, at..end).map_err(|e| e.to_string())?;
                if raw.len() as u64 != end - at {
                    return Err("SOURCE39_DIRECT_CANDIDATE_SHORT_READ".into());
                }
                f.write_all(&raw).map_err(|e| e.to_string())?;
                at = end;
            }
            f.sync_all().map_err(|e| e.to_string())?;
        }
        records.push(OriginalTensorV39 {
            param: import.parameter,
            layer: import.layer,
            dtype: d.dtype,
            shape: d.shape.clone(),
            raw_bytes: bound.tensor_bytes,
            source_tensor: root,
        });
    }
    Ok(records)
}

struct EmptySource39;
impl TensorSource for EmptySource39 {
    fn names(&self) -> Vec<String> {
        vec![]
    }
    fn shape(&self, _: &str) -> Option<Vec<usize>> {
        None
    }
    fn load(&self, _: &str) -> misaka_palw_tir_lower::Result<misaka_palw_tir_lower::weights::Tensor> {
        Err(misaka_palw_tir_lower::LowerError::bad("SOURCE39_PROCESSOR_HAS_NO_WEIGHTS"))
    }
}
struct SubsetSource39<'a> {
    source: &'a dyn TensorSource,
    names: &'a BTreeSet<String>,
}
impl TensorSource for SubsetSource39<'_> {
    fn names(&self) -> Vec<String> {
        self.names.iter().cloned().collect()
    }
    fn metadata(&self, n: &str) -> Option<misaka_palw_tir_lower::weights::TensorMeta> {
        self.names.contains(n).then(|| self.source.metadata(n)).flatten()
    }
    fn shape(&self, n: &str) -> Option<Vec<usize>> {
        self.names.contains(n).then(|| self.source.shape(n)).flatten()
    }
    fn load(&self, n: &str) -> misaka_palw_tir_lower::Result<misaka_palw_tir_lower::weights::Tensor> {
        if !self.names.contains(n) {
            return Err(misaka_palw_tir_lower::LowerError::bad("SOURCE39_UNDECLARED_SOURCE_TENSOR"));
        }
        self.source.load(n)
    }
    fn read_slice(&self, n: &str, r: std::ops::Range<u64>) -> misaka_palw_tir_lower::Result<Vec<u8>> {
        if !self.names.contains(n) {
            return Err(misaka_palw_tir_lower::LowerError::bad("SOURCE39_UNDECLARED_SOURCE_TENSOR"));
        }
        self.source.read_slice(n, r)
    }
    fn validate_snapshot(&self) -> misaka_palw_tir_lower::Result<()> {
        self.source.validate_snapshot()
    }
}
#[allow(clippy::too_many_arguments)]
fn write_public_frontend39(
    compiled: &misaka_palw_tir_lower::frontend_pack::source39::CompiledSourceV39,
    source: &dyn TensorSource,
    directory: &Path,
    stage: u16,
    block: usize,
    workspace: u64,
    audit: bool,
) -> Result<Vec<OriginalTensorV39>, String> {
    let p = compiled.program();
    let mut records = Vec::new();
    for (j, d) in p.params.iter().enumerate() {
        let layers = if d.per_layer { (0..p.schedule.layers.len()).map(|l| Some(l as u16)).collect::<Vec<_>>() } else { vec![None] };
        for layer in layers {
            let path = directory.join(tensor_name(stage, j as u16, layer));
            let mut f = std::fs::OpenOptions::new().create_new(true).write(true).open(&path).map_err(|e| e.to_string())?;
            compiled.write_tensor(source, j as u16, layer, &mut f, block).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            drop(f);
            let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
            let (root, bound) = tensor_commitment_streamed_v3(d.dtype, &shape, workspace, &mut |at, out| {
                f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
                f.read_exact(out).map_err(|e| e.to_string())
            })?;
            if f.metadata().map_err(|e| e.to_string())?.len() != bound.tensor_bytes {
                return Err("SOURCE39_FRONTEND_TENSOR_EXTENT".into());
            }
            records.push(OriginalTensorV39 {
                param: j as u16,
                layer,
                dtype: d.dtype,
                shape: d.shape.clone(),
                raw_bytes: bound.tensor_bytes,
                source_tensor: root,
            });
            drop(f);
            if audit {
                std::fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(records)
}
/// Fresh compilation only. No old context512 artifact, caller PC/R, readiness
/// flag or component whitelist can be promoted through this entry point.
pub fn compile_complete_frontend_v39(
    declaration: &Path,
    pin: [u8; 32],
    source_directory: &Path,
    output: &Path,
) -> Result<serde_json::Value, String> {
    if output.exists() {
        return Err("SOURCE39_FRESH_OUTPUT_REQUIRED".into());
    }
    let raw = bytes(declaration, 16 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != pin {
        return Err("SOURCE39_INDEPENDENT_DECLARATION_SHA".into());
    }
    let plan: CompleteFrontendV39 = serde_json::from_slice(&raw).map_err(|e| format!("SOURCE39_DECLARATION_GRAMMAR:{e}"))?;
    if plan.schema != "misaka.palw.source-backed-complete-frontend.v39"
        || plan.stages.is_empty()
        || plan.stages.len() > 64
        || plan.stages.len() != plan.original.original_contexts.len()
        || plan.stages.len() != plan.pipeline.stages.len()
        || plan.original.advertised_task.is_empty()
        || plan.original.components.is_empty()
        || plan.original.components.len() > 64
        || plan.original.source_files.is_empty()
        || plan.original.source_files.len() > 1024
        || !(8..=16 << 20).contains(&plan.block_bytes)
        || plan.maximum_output_bytes == 0
        || plan.maximum_output_bytes > 512 << 30
        || plan.commitment_workspace_bytes == 0
        || plan.commitment_workspace_bytes > 256 << 20
    {
        return Err("SOURCE39_DECLARATION_RESOURCE_BOUND".into());
    }
    let root = source_directory.canonicalize().map_err(|e| e.to_string())?;
    let package = declaration.canonicalize().map_err(|e| e.to_string())?.parent().ok_or("SOURCE39_DECLARATION_PARENT")?.to_path_buf();
    let mut files = Vec::new();
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for f in &plan.original.source_files {
        if !names.insert(f.path.clone()) {
            return Err("SOURCE39_DUPLICATE_ORIGINAL_FILE".into());
        }
        let path = relative(&root, &f.path)?;
        total = total.checked_add(f.bytes).ok_or("SOURCE39_ORIGINAL_FILE_SUM_OVERFLOW")?;
        if total > 512 << 30
            || path.metadata().map_err(|e| e.to_string())?.len() != f.bytes
            || hash(&path, f.bytes)? != decode::<32>(&f.sha256)?
        {
            return Err("SOURCE39_ORIGINAL_FILE_PIN_DIFFERS".into());
        }
        files.push(OriginalFilePinV39 { path: f.path.clone(), bytes: f.bytes, sha256: decode::<32>(&f.sha256)? });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    if !names.contains(&plan.original.task_metadata_file) || !names.contains(&plan.tokenizer_file) {
        return Err("SOURCE39_TASK_TOKENIZER_ORIGINAL_PIN_REQUIRED".into());
    }
    let task_bytes = bytes(&relative(&root, &plan.original.task_metadata_file)?, 4 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&task_bytes)) != decode::<32>(&plan.original.task_metadata_sha256)? {
        return Err("SOURCE39_TASK_METADATA_SHA_DIFFERS".into());
    }
    let task = object_id(b"misaka-palw/sdk/original-complete-task-metadata/v39", &(&plan.original.advertised_task, task_bytes));
    let tokenizer_bytes = bytes(&relative(&root, &plan.tokenizer_file)?, 64 << 20)?;
    let tokenizer = misaka_palw_tir_lower::artifact::tokenizer_id_of(&tokenizer_bytes);
    let mut programs: Vec<TirProgramV39> = Vec::new();
    let mut prepared = Vec::new();
    let mut checkpoints: Vec<Box<dyn TensorSource>> = Vec::new();
    let mut public_frontends = Vec::new();
    let mut geometries: Vec<Option<lower::pixtral_processor41::ProcessorGeometryV41>> = Vec::new();
    let mut source_paths = Vec::new();
    let mut selected = Vec::new();
    let mut original_payloads = BTreeSet::new();
    let mut available = BTreeSet::new();
    let mut consumed = BTreeSet::new();
    let mut compiler_pins = Vec::<(PathBuf, [u8; 32], u64)>::new();
    for (i, stage) in plan.stages.iter().enumerate() {
        let source_path = if stage.checkpoint == "." { root.clone() } else { relative(&root, &stage.checkpoint)? };
        let public_pack = if let FrontendCompilerV39::FrontendPack { pack_file, pack_sha256 } = &stage.compiler {
            let path = relative(&package, pack_file)?;
            let sha = decode::<32>(pack_sha256)?;
            let cap = misaka_palw_tir_lower::frontend_pack::source39::MAX_PACK_BYTES_V39 as u64;
            if hash(&path, cap)? != sha {
                return Err("SOURCE39_PUBLIC_FRONTEND_SHA_DIFFERS".into());
            }
            let raw = bytes(&path, cap)?;
            compiler_pins.push((path, sha, cap));
            let pack = misaka_palw_tir_lower::frontend_pack::source39::FrontendPackV39::parse(
                std::str::from_utf8(&raw).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            let expected =
                plan.original.components.iter().filter(|c| c.stages.contains(&(i as u16))).map(|c| c.name.clone()).collect::<Vec<_>>();
            if pack.scope().completeness != misaka_palw_tir_lower::frontend_pack::Completeness::Full
                || pack.scope().task != plan.original.advertised_task
                || pack.scope().components != expected
            {
                return Err("SOURCE39_PUBLIC_FRONTEND_COMPONENT_TASK_DIFFERS".into());
            }
            Some(pack)
        } else {
            None
        };
        let mut public_configuration = None;
        let (checkpoint, acquired): (Box<dyn TensorSource>, Vec<PathBuf>) =
            if matches!(stage.compiler, FrontendCompilerV39::Processor { .. } | FrontendCompilerV39::ImageJoin { .. }) {
                if !stage.checkpoint.is_empty() || !stage.tensor_names.is_empty() {
                    return Err("SOURCE39_PROCESSOR_HAS_NO_MODEL_WEIGHT_IMPORTS".into());
                }
                (Box::new(EmptySource39), vec![])
            } else if let Some(pack) = &public_pack {
                let source =
                    misaka_palw_tir_lower::frontend_pack::FrontendSource::open_v39(&source_path, pack).map_err(|e| e.to_string())?;
                let acquired = source.files();
                let sidecar = if stage.config_file.is_empty() {
                    serde_json::json!({})
                } else {
                    if !names.contains(&stage.config_file) {
                        return Err("SOURCE39_UNPINNED_ORIGINAL_CONFIG".into());
                    }
                    serde_json::from_slice(&bytes(&relative(&root, &stage.config_file)?, 64 << 20)?).map_err(|e| e.to_string())?
                };
                public_configuration = Some(source.configuration(sidecar).map_err(|e| e.to_string())?);
                (Box::new(source), acquired)
            } else {
                let source = Checkpoint::open(&source_path).map_err(|e| e.to_string())?;
                let acquired = source.files.iter().map(|f| f.path.clone()).collect();
                (Box::new(source), acquired)
            };
        for file in &acquired {
            let path = file.canonicalize().map_err(|e| e.to_string())?;
            let path = path
                .strip_prefix(&root)
                .map_err(|_| "SOURCE39_CHECKPOINT_PATH_ESCAPE")?
                .to_str()
                .ok_or("SOURCE39_CHECKPOINT_PATH_UTF8")?;
            if !names.contains(path) {
                return Err("SOURCE39_UNPINNED_ORIGINAL_SHARD".into());
            }
            original_payloads.insert(path.to_owned());
        }
        let tensor_names = stage.tensor_names.iter().cloned().collect::<BTreeSet<_>>();
        if tensor_names.len() != stage.tensor_names.len() {
            return Err("SOURCE39_DUPLICATE_ORIGINAL_STAGE_TENSOR".into());
        }
        for name in checkpoint.names() {
            available.insert(format!("{}::{name}", stage.checkpoint));
        }
        for name in &tensor_names {
            if checkpoint.metadata(name).is_none() {
                return Err("SOURCE39_ORIGINAL_STAGE_TENSOR_ABSENT".into());
            }
            consumed.insert(format!("{}::{name}", stage.checkpoint));
        }
        let config = if let Some(config) = public_configuration {
            config
        } else if stage.config_file.is_empty() {
            serde_json::json!({})
        } else {
            if !names.contains(&stage.config_file) {
                return Err("SOURCE39_UNPINNED_ORIGINAL_CONFIG".into());
            }
            serde_json::from_slice(&bytes(&relative(&root, &stage.config_file)?, 64 << 20)?).map_err(|e| e.to_string())?
        };
        let mut statistics = |file: &str, sha: &str| -> Result<String, String> {
            let path = relative(&package, file)?;
            let sha = decode::<32>(sha)?;
            if hash(&path, 64 << 20)? != sha {
                return Err("SOURCE39_CALIBRATION_INPUT_SHA_DIFFERS".into());
            }
            let raw = bytes(&path, 64 << 20)?;
            compiler_pins.push((path, sha, 64 << 20));
            String::from_utf8(raw).map_err(|e| e.to_string())
        };
        let mut public_compiled = None;
        let mut stage_geometry = None;
        let (program, recipe) = match &stage.compiler {
            FrontendCompilerV39::FrontendPack { .. } => {
                let pack = public_pack.as_ref().ok_or("SOURCE39_PUBLIC_PACK_INTERNAL")?;
                let subset = SubsetSource39 { source: checkpoint.as_ref(), names: &tensor_names };
                let compiled = pack.compile(&config, &subset, plan.block_bytes).map_err(|e| e.to_string())?;
                let p = compiled.program().clone();
                public_compiled = Some(compiled);
                (p, None)
            }
            FrontendCompilerV39::ImageJoin { upstream_stages, output_scale_q } => {
                if *output_scale_q > 62
                    || upstream_stages.is_empty()
                    || upstream_stages.len() > 16
                    || upstream_stages.windows(2).any(|w| w[0] >= w[1])
                    || upstream_stages.iter().any(|s| *s as usize >= i)
                    || !stage.config_file.is_empty()
                {
                    return Err("SOURCE39_IMAGE_JOIN_ORIGINAL_ORDER_OR_PROFILE".into());
                }
                let bindings = &plan.pipeline.stages[i].bind;
                if bindings.len() != upstream_stages.len()
                    || bindings
                        .iter()
                        .zip(upstream_stages)
                        .any(|(b, s)| !matches!(b,super::frontend::BindingDescription::StageFinal{stage}if u16::from(*stage)==*s))
                {
                    return Err("SOURCE39_IMAGE_JOIN_ORIGINAL_EDGES_DIFFER".into());
                }
                let upstream = upstream_stages.iter().map(|s| &programs[*s as usize]).collect::<Vec<_>>();
                let p = lower::source39::join_image_rows_v39(&upstream, SourceProfileV39::full_source()).map_err(|e| e.to_string())?;
                (p, None)
            }
            FrontendCompilerV39::Processor { profile } => {
                let v = &config["vision_encoder"];
                if stage.config_file.is_empty()
                    || v["max_image_size"].as_u64() != Some(profile.maximum_image_size as u64)
                    || v["patch_size"].as_u64() != Some(profile.patch as u64)
                    || v["spatial_merge_size"].as_u64() != Some(profile.merge as u64)
                    || v["image_token_id"].as_u64() != Some(profile.image_token as u64)
                    || v["image_break_token_id"].as_u64() != Some(profile.break_token as u64)
                    || v["image_end_token_id"].as_u64() != Some(profile.end_token as u64)
                {
                    return Err("SOURCE39_PUBLISHER_PROCESSOR_METADATA_DIFFERS".into());
                }
                let (program, geometry) = lower::pixtral_processor41::lower_processor_v41(profile, SourceProfileV39::full_source())
                    .map_err(|e| e.to_string())?;
                stage_geometry = Some(geometry);
                (program, None)
            }
            FrontendCompilerV39::Vision { profile, processor_stage } => {
                let geometry = if let Some(upstream) = processor_stage {
                    if *upstream as usize >= i
                        || !matches!(plan.pipeline.stages[i].bind.as_slice(),[super::frontend::BindingDescription::StageFinal{stage}]if u16::from(*stage)==*upstream)
                    {
                        return Err("SOURCE39_VISION_PROCESSOR_ORIGINAL_EDGE_DIFFERS".into());
                    }
                    Some(geometries[*upstream as usize].as_ref().ok_or("SOURCE39_UPSTREAM_IS_NOT_EXPLICIT_PROCESSOR41")?)
                } else {
                    None
                };
                let p = PreparedSourceV39::vision(
                    &config,
                    profile,
                    &statistics(&profile.statistics_file, &profile.statistics_sha256)?,
                    geometry,
                )?;
                (p.program.clone(), Some(p))
            }
            FrontendCompilerV39::Decoder { profile } => {
                if profile.original_context != plan.original.original_contexts[i] {
                    return Err("SOURCE39_ORIGINAL_DECODER_CONTEXT_DIFFERS".into());
                }
                let p =
                    PreparedSourceV39::decoder(&config, profile, &statistics(&profile.statistics_file, &profile.statistics_sha256)?)?;
                if let Some(image) = &profile.image_rows {
                    let upstream = image.upstream_stage as usize;
                    if upstream >= i
                        || !matches!(plan.pipeline.stages[i].bind.as_slice(),[super::frontend::BindingDescription::StageFinal{stage}]if *stage as usize==upstream)
                    {
                        return Err("SOURCE39_DECODER_IMAGE_ORIGINAL_EDGE_DIFFERS".into());
                    }
                    let u = &programs[upstream];
                    let out = &u.blocks[u.schedule.post as usize].nodes[u.output.node() as usize].out;
                    if !matches!(u.output, misaka_palw_tir::program_v2::OutputDecl::Final { .. })
                        || out.dtype != misaka_palw_tir::DType::I32
                        || out.shape != vec![DimV39::Fixed(image.rows), DimV39::Fixed(image.width)]
                    {
                        return Err("SOURCE39_DECODER_IMAGE_OUTPUT_TYPE_DIFFERS".into());
                    }
                }
                (p.program.clone(), Some(p))
            }
            FrontendCompilerV39::Canonical { program_file, program_sha256, .. } => {
                if !stage.config_file.is_empty() {
                    return Err("SOURCE39_CANONICAL_UNUSED_CONFIG_USE_ORIGINAL_TASK_METADATA".into());
                }
                let path = relative(&package, program_file)?;
                let sha = decode::<32>(program_sha256)?;
                if hash(&path, 16 << 20)? != sha {
                    return Err("SOURCE39_CANONICAL_COMPILER_SHA_DIFFERS".into());
                }
                let program = TirProgramV39::decode_canonical(&bytes(&path, 16 << 20)?)?;
                compiler_pins.push((path, sha, 16 << 20));
                (program, None)
            }
        };
        if plan.original.original_contexts[i] == 0
            || program.history_bound < plan.original.original_contexts[i]
            || plan.pipeline.stages[i].max_trip < plan.original.original_contexts[i]
        {
            return Err("SOURCE39_ORIGINAL_FULL_CONTEXT_NOT_LOWERED".into());
        }
        let mut instances = 0usize;
        for d in &program.params {
            instances = instances
                .checked_add(if d.per_layer { program.schedule.layers.len() } else { 1 })
                .ok_or("SOURCE39_PC_INSTANCE_SUM")?;
            let shape = d.shape.iter().map(|n| *n as usize).collect::<Vec<_>>();
            if streamed_commitment_shape_v3(d.dtype, &shape)?.workspace_bytes > plan.commitment_workspace_bytes {
                return Err("SOURCE39_PC_WORKSPACE_BEFORE_MATERIALISATION".into());
            }
        }
        if instances > 16384 {
            return Err("SOURCE39_PC_INSTANCE_CAP".into());
        }
        programs.push(program);
        prepared.push(recipe);
        checkpoints.push(checkpoint);
        public_frontends.push(public_compiled);
        geometries.push(stage_geometry);
        source_paths.push(source_path);
        selected.push(tensor_names);
    }
    let declared = plan.original.complete_tensor_inventory.iter().cloned().collect::<BTreeSet<_>>();
    if declared.len() != plan.original.complete_tensor_inventory.len() || declared != available || declared != consumed {
        return Err("SOURCE39_COMPLETE_ORIGINAL_TENSOR_INVENTORY_DIFFERS".into());
    }
    // Discover omitted source payloads, including shards omitted from both the
    // caller's tensor list and checkpoint index. No symlink can bypass the pins.
    let mut pending = vec![root.clone()];
    let mut count = 0usize;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            count = count.checked_add(1).ok_or("SOURCE39_DIRECTORY_COUNT")?;
            if count > 4096 {
                return Err("SOURCE39_SOURCE_DIRECTORY_BOUND".into());
            }
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err("SOURCE39_SOURCE_SYMLINK".into());
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                let path = entry.path();
                let name =
                    path.strip_prefix(&root).map_err(|_| "SOURCE39_DIRECTORY_ESCAPE")?.to_str().ok_or("SOURCE39_DIRECTORY_UTF8")?;
                let payload =
                    name.ends_with(".safetensors") || name.ends_with(".bin") || name.ends_with(".gguf") || name.ends_with(".palwq36");
                if payload && (!names.contains(name) || !original_payloads.contains(name)) {
                    return Err("SOURCE39_UNDECLARED_OR_UNACQUIRED_ORIGINAL_PAYLOAD".into());
                }
            }
        }
    }
    let pipeline = pipeline(&plan.pipeline, &programs)?;
    let checked_pipeline = pipeline.check(&programs).map_err(|e| e.to_string())?;
    std::fs::create_dir(output).map_err(|e| e.to_string())?;
    let parameters = output.join("parameters");
    let audit = output.join("original-stream-audit");
    std::fs::create_dir(&parameters).map_err(|e| e.to_string())?;
    std::fs::create_dir(&audit).map_err(|e| e.to_string())?;
    let mut sources = Vec::new();
    let mut scopes = Vec::new();
    let mut vectors = Vec::new();
    let mut materialisation = Vec::new();
    let mut candidate_total = 0u64;
    for (i, program) in programs.iter().enumerate() {
        let stage = i as u16;
        // This pass rereads original source and pins transformed integer semantics
        // before the candidate path exists. Audit files are deleted tensor by tensor.
        let (original, observation) = if let Some(p) = &prepared[i] {
            let (report, records) = p.write(
                &source_paths[i],
                &selected[i],
                &audit,
                stage,
                plan.block_bytes,
                plan.maximum_output_bytes,
                true,
                plan.commitment_workspace_bytes,
            )?;
            (records, report)
        } else if let FrontendCompilerV39::ImageJoin { upstream_stages, output_scale_q } = &plan.stages[i].compiler {
            let scale = 2f64.powi(-(*output_scale_q as i32));
            if upstream_stages
                .iter()
                .any(|s| materialisation.get(*s as usize).and_then(|v: &serde_json::Value| v["output_scale"].as_f64()) != Some(scale))
            {
                return Err("SOURCE39_IMAGE_JOIN_INTEGER_UNITS_DIFFER".into());
            }
            (
                Vec::new(),
                serde_json::json!({"schema":"misaka.palw.original-image-projector-join.v39","output_scale":scale,"upstream_stages":upstream_stages,"source_fidelity":"NOT_RUN"}),
            )
        } else if let Some(geometry) = &geometries[i] {
            (
                Vec::new(),
                serde_json::json!({"schema":"misaka.palw.publisher-pixtral-processor.v41","geometry":geometry,"pixel_unit":1.0/64.0,"source_fidelity":"NOT_RUN"}),
            )
        } else if let Some(compiled) = &public_frontends[i] {
            let subset = SubsetSource39 { source: checkpoints[i].as_ref(), names: &selected[i] };
            (
                write_public_frontend39(compiled, &subset, &audit, stage, plan.block_bytes, plan.commitment_workspace_bytes, true)?,
                compiled.provenance(),
            )
        } else {
            let FrontendCompilerV39::Canonical { imports, .. } = &plan.stages[i].compiler else {
                return Err("SOURCE39_COMPILER_INTERNAL_VARIANT".into());
            };
            (
                write_imports(
                    program,
                    imports,
                    checkpoints[i].as_ref(),
                    &selected[i],
                    &audit,
                    stage,
                    plan.block_bytes,
                    plan.commitment_workspace_bytes,
                    true,
                )?,
                serde_json::json!({"source_fidelity":"NOT_RUN"}),
            )
        };
        let pc = ParamCommitmentsV1 { by_instance: original.iter().map(|r| ((r.param, r.layer), r.source_tensor)).collect() };
        let inventory_root = derive_inventory_root_v39(program, task, &original)?;
        let wire = FlatSourceWireV39 {
            version: 39,
            program_bytes: program.encode(),
            inventory_root,
            original_inventory: original.clone(),
            real_parameters: pc,
            original_context_max: plan.original.original_contexts[i],
            original_task_metadata: task,
        };
        let checked = wire.check()?;
        let maximum = misaka_palw_kernel::flat_resources39::derive_flat_resources_v39(&checked, pipeline.stages[i].max_trip)
            .map_err(|e| format!("SOURCE39_FULL_ORIGINAL_SCOPE_STAGE_{i}:{e}"))?;
        let bytes = original.iter().try_fold(0u64, |n, r| n.checked_add(r.raw_bytes).ok_or("SOURCE39_PARAMETER_SUM_OVERFLOW"))?;
        candidate_total = candidate_total.checked_add(bytes).ok_or("SOURCE39_TOTAL_PARAMETER_OVERFLOW")?;
        if candidate_total > plan.maximum_output_bytes {
            return Err("SOURCE39_AGGREGATE_REAL_PARAMETER_BUDGET".into());
        }
        if let Some(p) = &prepared[i] {
            p.write(
                &source_paths[i],
                &selected[i],
                &parameters,
                stage,
                plan.block_bytes,
                bytes,
                false,
                plan.commitment_workspace_bytes,
            )?;
        } else if geometries[i].is_some() || matches!(plan.stages[i].compiler, FrontendCompilerV39::ImageJoin { .. }) {
            // The processor has no model parameters; its exact resize tables
            // are canonical Program39 constants, already part of original P.
        } else if let Some(compiled) = &public_frontends[i] {
            let subset = SubsetSource39 { source: checkpoints[i].as_ref(), names: &selected[i] };
            write_public_frontend39(compiled, &subset, &parameters, stage, plan.block_bytes, plan.commitment_workspace_bytes, false)?;
        } else {
            let FrontendCompilerV39::Canonical { imports, .. } = &plan.stages[i].compiler else {
                return Err("SOURCE39_COMPILER_INTERNAL_VARIANT".into());
            };
            write_imports(
                program,
                imports,
                checkpoints[i].as_ref(),
                &selected[i],
                &parameters,
                stage,
                plan.block_bytes,
                plan.commitment_workspace_bytes,
                false,
            )?;
        }
        let candidate = hash_parameters(program, &parameters, stage, plan.commitment_workspace_bytes)?;
        if candidate != original {
            return Err("SOURCE39_CANDIDATE_DIFFERS_FROM_INDEPENDENT_ORIGINAL_STREAM".into());
        }
        // The calibrated original vision output and decoder image-row input must
        // denote the same integer unit; equal shape alone does not establish that.
        if let FrontendCompilerV39::Decoder { profile } = &plan.stages[i].compiler {
            if let Some(image) = &profile.image_rows {
                let value: &serde_json::Value = &materialisation[image.upstream_stage as usize];
                if value["output_scale"].as_f64() != Some(2f64.powi(-(image.q as i32))) {
                    return Err("SOURCE39_VISION_DECODER_INTEGER_UNIT_DIFFERS".into());
                }
            }
        }
        std::fs::write(output.join(format!("stage-{i}.tir39")), &wire.program_bytes).map_err(|e| e.to_string())?;
        std::fs::write(output.join(format!("stage-{i}.source39")), wire.to_bytes()).map_err(|e| e.to_string())?;
        std::fs::write(output.join(format!("stage-{i}.maximum-scope39")), borsh::to_vec(maximum.scope()).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        std::fs::write(
            output.join(format!("stage-{i}.maximum-resources39")),
            borsh::to_vec(maximum.vector()).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        scopes.push(maximum.scope().root());
        vectors.push(maximum.root());
        sources.push(checked);
        materialisation.push(observation);
    }
    std::fs::remove_dir(&audit).map_err(|e| e.to_string())?;
    let mut required = plan.original.components.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    required.sort();
    if required.windows(2).any(|w| w[0] == w[1]) {
        return Err("SOURCE39_DUPLICATE_REQUIRED_COMPONENT".into());
    }
    let mut components = Vec::new();
    for name in &required {
        let c = plan.original.components.iter().find(|c| &c.name == name).ok_or("SOURCE39_MISSING_ORIGINAL_COMPONENT")?;
        let mut source_files = c
            .source_files
            .iter()
            .map(|name| files.iter().position(|f| &f.path == name).map(|i| i as u16).ok_or("SOURCE39_COMPONENT_ORIGINAL_FILE_ABSENT"))
            .collect::<Result<Vec<_>, _>>()?;
        source_files.sort();
        let mut stages = c.stages.clone();
        stages.sort();
        components.push(SourceComponentV39 { name: name.clone(), source_files, stages });
    }
    let stages = sources
        .iter()
        .enumerate()
        .map(|(i, s)| ComponentStageV39 {
            original_source: s.root(),
            original_profile: s.profile_root(),
            original_parameter: s.parameters().root(),
            original_program: stage_program_root_v39(s.program()),
            original_inventory: s.inventory_root(),
            input_declarations: stage_input_declarations_root_v39(s.program()),
            output_declaration: stage_output_declaration_root_v39(s.program()),
            original_context_max: plan.original.original_contexts[i],
            lowered_context_max: plan.original.original_contexts[i],
        })
        .collect();
    let manifest = CompleteComponentManifestV39 {
        version: 39,
        repository: plan.original.repository.clone(),
        revision: decode::<20>(&plan.original.revision)?,
        original_task_metadata: task,
        original_pipeline: checked_pipeline.root(),
        original_required_components: required.clone(),
        source_files: files,
        components,
        stages,
        inputs: input_bindings(&pipeline, &programs)?,
    };
    check_component_manifest_v39(&manifest, &sources, &checked_pipeline, &required, task, &manifest.inputs)
        .map_err(|e| format!("SOURCE39_COMPONENT_ADMISSION:{e:?}"))?;
    let bundle = CompleteSourceBundleV39 {
        version: 39,
        manifest: manifest.to_bytes().map_err(|e| e.to_string())?,
        pipeline: pipeline.to_bytes().map_err(|e| e.to_string())?,
        sources: sources.iter().map(|s| s.wire().to_bytes()).collect(),
    };
    let complete = bundle.check()?;
    let image_contract = request41::build_contract_v41(&plan, &geometries, &complete, tokenizer)?;
    // Final pins are checked after BOTH independent passes, before emitting the
    // immutable completion marker. Source mutation leaves only diagnostics.
    for f in &complete.manifest().manifest().source_files {
        let path = relative(&root, &f.path)?;
        if path.metadata().map_err(|e| e.to_string())?.len() != f.bytes || hash(&path, f.bytes)? != f.sha256 {
            return Err("SOURCE39_ORIGINAL_FILE_CHANGED_DURING_COMPILATION".into());
        }
    }
    for (path, sha, cap) in compiler_pins {
        if hash(&path, cap)? != sha {
            return Err("SOURCE39_COMPILER_INPUT_CHANGED".into());
        }
    }
    if hash(declaration, 16 << 20)? != pin {
        return Err("SOURCE39_DECLARATION_CHANGED".into());
    }
    if let Some(contract) = &image_contract {
        std::fs::write(output.join("image-contract41.json"), serde_json::to_vec_pretty(contract).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    std::fs::write(output.join("complete-source39.borsh"), bundle.to_bytes().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    std::fs::write(output.join("component-manifest39.borsh"), &bundle.manifest).map_err(|e| e.to_string())?;
    std::fs::write(output.join("pipeline39.borsh"), &bundle.pipeline).map_err(|e| e.to_string())?;
    let hex = crate::runtime_pack::commit::hex;
    let report = serde_json::json!({"schema":"misaka.palw.fresh-complete-source-conversion.v39","declaration_sha256":hex(&pin),"repository":plan.original.repository,"revision":plan.original.revision,"complete_source_root":hex(&complete.root()),"pipeline_root":hex(&complete.pipeline().root()),"component_manifest_root":hex(&complete.manifest().root()),"source_restore_bytes":complete.restore_bytes(),"source_roots":sources.iter().map(|s|hex(&s.root())).collect::<Vec<_>>(),"maximum_scopes":scopes.iter().map(|r|hex(r)).collect::<Vec<_>>(),"maximum_resources":vectors.iter().map(|r|hex(r)).collect::<Vec<_>>(),"original_contexts":plan.original.original_contexts,"tokenizer_root":hex(&tokenizer),"actual_parameter_bytes":candidate_total,"materialisation":materialisation,"processor_geometry":geometries,"image_contract":image_contract.as_ref().map(|c|crate::runtime_pack::commit::hex(&c.root())),"source_fidelity":"NOT_RUN","native_final":"NOT_RUN","a6_eligible":false,"a8_eligible":false});
    std::fs::write(output.join("conversion.json"), serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(report)
}

/// Exact complete bundle for a source-backed standalone text program (including
/// the original PALWQ361 runtime). This is the same manifest/pipeline39 admission
/// used by multi-component conversion; old artifacts are never lifted here.
pub fn single_source_complete_bundle_v39(
    original: &OriginalSource,
    source: &misaka_palw_kernel::flat_source39::CheckedSourceV39,
) -> Result<CompleteSourceBundleV39, String> {
    if original.components.len() != 1
        || original.components[0].stages != vec![0]
        || original.original_contexts != vec![source.original_context_max()]
        || !source.program().inputs.is_empty()
        || !matches!(source.program().output, misaka_palw_tir::program_v2::OutputDecl::Logits { .. })
    {
        return Err("SOURCE39_COMPLETE_STANDALONE_TEXT_SCOPE".into());
    }
    let mut files = original
        .source_files
        .iter()
        .map(|f| Ok(OriginalFilePinV39 { path: f.path.clone(), bytes: f.bytes, sha256: decode::<32>(&f.sha256)? }))
        .collect::<Result<Vec<_>, String>>()?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let source_files = original.components[0]
        .source_files
        .iter()
        .map(|name| {
            files.iter().position(|f| &f.path == name).map(|i| i as u16).ok_or("SOURCE39_COMPLETE_STANDALONE_SOURCE_FILE".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let pipeline = PipelineV39 {
        version: 39,
        stages: vec![StageDecl {
            name: "complete-original-decoder39".into(),
            program: 0,
            trip: TripRule::TextStream,
            max_trip: source.original_context_max(),
            tokens: None,
            bind: vec![],
        }],
        output_stage: 0,
    };
    let checked_pipeline = pipeline.check(&[source.program().clone()]).map_err(|e| e.to_string())?;
    let mut source_files = source_files;
    source_files.sort();
    let manifest = CompleteComponentManifestV39 {
        version: 39,
        repository: original.repository.clone(),
        revision: decode::<20>(&original.revision)?,
        original_task_metadata: source.original_task_metadata(),
        original_pipeline: checked_pipeline.root(),
        original_required_components: vec![original.components[0].name.clone()],
        source_files: files,
        components: vec![SourceComponentV39 { name: original.components[0].name.clone(), source_files, stages: vec![0] }],
        stages: vec![ComponentStageV39 {
            original_source: source.root(),
            original_profile: source.profile_root(),
            original_parameter: source.parameters().root(),
            original_program: stage_program_root_v39(source.program()),
            original_inventory: source.inventory_root(),
            input_declarations: stage_input_declarations_root_v39(source.program()),
            output_declaration: stage_output_declaration_root_v39(source.program()),
            original_context_max: source.original_context_max(),
            lowered_context_max: source.original_context_max(),
        }],
        inputs: vec![],
    };
    let bundle = CompleteSourceBundleV39 {
        version: 39,
        manifest: manifest.to_bytes().map_err(|e| e.to_string())?,
        pipeline: pipeline.to_bytes().map_err(|e| e.to_string())?,
        sources: vec![source.wire().to_bytes()],
    };
    bundle.check()?;
    Ok(bundle)
}
