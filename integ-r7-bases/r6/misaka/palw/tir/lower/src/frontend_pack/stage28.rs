//! Explicit new V2 frontend contract. Frozen V1 packs and admission remain unchanged.
//! Config expressions and packed-weight decoders are shared with V1, while canonical
//! V2 validates external/random inputs, Rows/Final/Logits and post-state effects.
use super::*;
use misaka_palw_tir::program_v2::{InputDecl, InputSource, OutputDecl, RandomDist, TirProgramV2};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageInputV28 {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<u32>,
    pub source: StageInputSourceV28,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageInputSourceV28 {
    External { lo: i64, hi: i64 },
    Uniform { domain: u16, bits: u8, per_step: bool },
    Normal { domain: u16, per_step: bool },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageOutputV28 {
    Logits { node: u16, scheme_id: String },
    Rows { node: u16 },
    Final { node: u16 },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageSemanticsV28 {
    pub version: u16,
    pub inputs: Vec<StageInputV28>,
    pub output: StageOutputV28,
}
impl StageSemanticsV28 {
    pub fn of_program(p: &TirProgramV2) -> Self {
        Self {
            version: 28,
            inputs: p
                .inputs
                .iter()
                .map(|i| StageInputV28 {
                    name: i.name.clone(),
                    dtype: i.dtype.name().into(),
                    shape: i.shape.clone(),
                    source: match i.source {
                        InputSource::External { lo, hi } => StageInputSourceV28::External { lo, hi },
                        InputSource::Random { domain, dist: RandomDist::Uniform { bits }, per_step } => {
                            StageInputSourceV28::Uniform { domain, bits, per_step }
                        }
                        InputSource::Random { domain, dist: RandomDist::Normal, per_step } => {
                            StageInputSourceV28::Normal { domain, per_step }
                        }
                    },
                })
                .collect(),
            output: match p.output {
                OutputDecl::Logits { node, scheme_id } => StageOutputV28::Logits { node, scheme_id: program::hex(&scheme_id) },
                OutputDecl::Rows { node } => StageOutputV28::Rows { node },
                OutputDecl::Final { node } => StageOutputV28::Final { node },
            },
        }
    }
    pub(super) fn compile(&self, fields: program::Program) -> Result<TirProgramV2> {
        if self.version != 28 || fields.version != 2 || self.inputs.len() > misaka_palw_tir::program_v2::MAX_INPUTS_V2 {
            return Err(bad("FRONTEND_STAGE28_VERSION_OR_INPUT_BOUND"));
        }
        let p = fields.compile_fields()?;
        let inputs = self
            .inputs
            .iter()
            .map(|i| {
                if i.name.len() > misaka_palw_tir::program::MAX_NAME_BYTES || i.shape.len() > 4 {
                    return Err(bad("FRONTEND_STAGE28_INPUT_BOUND"));
                }
                Ok(InputDecl {
                    name: i.name.clone(),
                    dtype: misaka_palw_tir::DType::from_name(&i.dtype)
                        .ok_or_else(|| bad("KERNEL_EXTENSION_REQUIRED: stage28 input dtype"))?,
                    shape: i.shape.clone(),
                    source: match i.source {
                        StageInputSourceV28::External { lo, hi } => InputSource::External { lo, hi },
                        StageInputSourceV28::Uniform { domain, bits, per_step } => {
                            InputSource::Random { domain, dist: RandomDist::Uniform { bits }, per_step }
                        }
                        StageInputSourceV28::Normal { domain, per_step } => {
                            InputSource::Random { domain, dist: RandomDist::Normal, per_step }
                        }
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let output = match &self.output {
            StageOutputV28::Logits { node, scheme_id } => OutputDecl::Logits {
                node: *node,
                scheme_id: program::unhex(scheme_id, 64)?.try_into().map_err(|_| bad("FRONTEND_STAGE28_LOGITS_ID"))?,
            },
            StageOutputV28::Rows { node } => OutputDecl::Rows { node: *node },
            StageOutputV28::Final { node } => OutputDecl::Final { node: *node },
        };
        let program = TirProgramV2 {
            version: 2,
            prim_set_id: p.prim_set_id,
            token_bound: p.token_bound,
            history_bound: p.history_bound,
            inputs,
            params: p.params,
            consts: p.consts,
            states: p.states,
            blocks: p.blocks,
            schedule: p.schedule,
            output,
        };
        TirProgramV2::decode_canonical(&program.encode())
            .map_err(|e| LowerError::not_lowerable(format!("FRONTEND_STAGE28_ADMISSION: {e}")))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageTensorReportV28 {
    pub tensor_bytes: u64,
    pub source_read_bytes: u64,
    pub max_read_bytes: usize,
    pub saturated_values: u64,
    pub source_tensors: Vec<SourceRecord>,
    pub quant_formats: BTreeMap<String, String>,
}
pub struct CompiledStageV28 {
    inner: Compiled,
}
impl CompiledStageV28 {
    pub fn program(&self) -> &TirProgramV2 {
        self.inner.stage_program.as_ref().expect("private V2 compilation")
    }
    pub fn source_names(&self) -> Vec<String> {
        self.inner.bindings.values().flat_map(|r| r.sources.keys().cloned()).collect::<BTreeSet<_>>().into_iter().collect()
    }
    pub fn write_tensor(
        &self,
        source: &dyn TensorSource,
        param: u16,
        layer: Option<u16>,
        out: &mut dyn std::io::Write,
        block_bytes: usize,
    ) -> Result<StageTensorReportV28> {
        self.inner.write_stage_tensor_v28(source, param, layer, out, block_bytes)
    }
}
impl FrontendPack {
    /// This explicit opt-in never calls the old V1 text admission on a V2 stage.
    /// It still rejects unread config, missing/unconsumed tensors and unknown semantics.
    pub fn compile_stage_v28(
        &self,
        config: &Value,
        source: &dyn TensorSource,
        semantics: &StageSemanticsV28,
        block_bytes: usize,
    ) -> Result<CompiledStageV28> {
        let inner = self.compile_inner(config, source, &crate::admission::default_inputs(), block_bytes, Some(semantics))?;
        Ok(CompiledStageV28 { inner })
    }
}
