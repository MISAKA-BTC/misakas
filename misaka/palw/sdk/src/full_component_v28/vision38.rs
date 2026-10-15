//! Actual source-backed Pixtral/Mistral3 materialisation. Canonical V2, weights,
//! exact calibration and policy are explicit inputs; this grants no native rights.
use misaka_palw_tir_lower::{
    float_ref::{SiteStat, stream::Streamed},
    hl::HlProgram,
    lower::{Lowered, StreamOpts, TensorSink, materialise_stream, vision},
    quant::QuantPolicy,
    weights::{Binding, Checkpoint, Tensor, TensorMeta, TensorSource},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    ops::Range,
    path::{Path, PathBuf},
    sync::Mutex,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisionFrontendV38 {
    pub version: u16,
    pub statistics_file: String,
    pub statistics_sha256: String,
    /// Original canonical RGB input profile. These values are pinned conversion semantics.
    pub mean: [f64; 3],
    pub std: [f64; 3],
    pub headroom16: f64,
    pub headroom32: f64,
    pub headroom_resid: f64,
}
pub(super) struct PreparedVisionV38 {
    pub program: misaka_palw_tir::program_v2::TirProgramV2,
    pub lowered: Lowered,
    pub hl: HlProgram,
    pub binding: Binding,
    pub statistics: BTreeMap<String, SiteStat>,
    pub policy: QuantPolicy,
}
impl PreparedVisionV38 {
    pub fn prepare(config: &serde_json::Value, profile: &VisionFrontendV38, stats: &str) -> Result<Self, String> {
        if profile.version != 38
            || [profile.headroom16, profile.headroom32, profile.headroom_resid].iter().any(|v| *v <= 0.0 || !v.is_finite())
        {
            return Err("PIXTRAL38_EXPLICIT_PROFILE_OR_POLICY_REQUIRED".into());
        }
        let spec = vision::parse_vision(&config.to_string(), None, Some((profile.mean, profile.std))).map_err(|e| e.to_string())?;
        if !spec.is_pixtral_v38() {
            return Err("PIXTRAL38_OTHER_SOURCE_SEMANTICS".into());
        }
        if spec.h as u64 != config["vision_config"]["image_size"].as_u64().unwrap_or(0) || spec.w != spec.h {
            return Err("PIXTRAL38_ORIGINAL_MAXIMUM_IMAGE_CHANGED".into());
        }
        let (hl, binding) = vision::hl_program(&spec).map_err(|e| e.to_string())?;
        if !binding.ignored_prefixes.is_empty() {
            return Err("PIXTRAL38_IGNORED_ORIGINAL_SOURCE_COMPONENT".into());
        }
        let lowered = vision::lower_vision_with(&hl, &spec, true).map_err(|e| format!("PIXTRAL38_CANONICAL_GRAMMAR: {e}"))?;
        let program = misaka_palw_tir_lower::encoder::vision_v2(&lowered).map_err(|e| e.to_string())?;
        let statistics = misaka_palw_tir_lower::calib::stats_from_json_exact(stats).map_err(|e| e.to_string())?;
        let policy =
            QuantPolicy { headroom16: profile.headroom16, headroom32: profile.headroom32, headroom_resid: profile.headroom_resid };
        Ok(Self { program, lowered, hl, binding, statistics, policy })
    }
    pub fn write_tensors(
        &self,
        checkpoint: &Path,
        allowed: &BTreeSet<String>,
        directory: &Path,
        stage: u16,
        block_bytes: usize,
        total_cap: u64,
    ) -> Result<serde_json::Value, String> {
        write_materialised_stage_v38(
            &self.lowered,
            &self.hl,
            &self.binding,
            &self.statistics,
            &self.policy,
            checkpoint,
            allowed,
            directory,
            stage,
            block_bytes,
            total_cap,
            Some(vision::IMAGE_PARAM),
        )
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn write_materialised_stage_v38(
    lowered: &Lowered,
    hl: &HlProgram,
    binding: &Binding,
    statistics: &BTreeMap<String, SiteStat>,
    policy: &QuantPolicy,
    checkpoint: &Path,
    allowed: &BTreeSet<String>,
    directory: &Path,
    stage: u16,
    block_bytes: usize,
    total_cap: u64,
    lifted_name: Option<&str>,
) -> Result<serde_json::Value, String> {
    let source = Checkpoint::open(checkpoint).map_err(|e| e.to_string())?;
    let source = TrackedSource { source: &source, allowed, touched: Mutex::new(BTreeSet::new()) };
    let loader = Streamed::new(hl, binding, &source).with_lazy((block_bytes / 4).max(1), block_bytes);
    let lifted = lifted_name
        .map(|name| {
            lowered.program.params.iter().position(|p| p.name == name).map(|i| i as u16).ok_or("SOURCE38_DECLARED_INPUT_ABSENT")
        })
        .transpose()?;
    let mut sink =
        DirectorySink { directory: directory.into(), stage, lifted, remaining: 0, total: 0, total_cap, current: None, skip: false };
    let stream = materialise_stream(
        lowered,
        hl,
        &loader,
        statistics,
        policy,
        &StreamOpts { defer_min_elems: 1, block_elems: (block_bytes / 4).max(1) },
        &mut sink,
        &|_, _| {},
    )
    .map_err(|e| e.to_string())?;
    if *source.touched.lock().map_err(|_| "SOURCE38_TRACKING_LOCK")? != *allowed {
        return Err("SOURCE38_DECLARED_BUT_UNREAD_ORIGINAL_TENSOR".into());
    }
    Ok(
        serde_json::json!({"schema":"misaka.palw.actual-source-materialisation.v38","stage":stage,"real_tensor_bytes":sink.total,"residual_scale":stream.resid_scale,"output_scale":stream.logits_scale,"quant_inexact":stream.quant_inexact,"source_fidelity":"NOT_RUN"}),
    )
}

struct TrackedSource<'a> {
    source: &'a Checkpoint,
    allowed: &'a BTreeSet<String>,
    touched: Mutex<BTreeSet<String>>,
}
impl TrackedSource<'_> {
    fn mark(&self, n: &str) -> misaka_palw_tir_lower::Result<()> {
        if !self.allowed.contains(n) {
            return Err(misaka_palw_tir_lower::LowerError::weights("PIXTRAL38_UNDECLARED_SOURCE_TENSOR"));
        }
        self.touched.lock().map_err(|_| misaka_palw_tir_lower::LowerError::weights("source tracking lock"))?.insert(n.into());
        Ok(())
    }
}
impl TensorSource for TrackedSource<'_> {
    fn shape(&self, n: &str) -> Option<Vec<usize>> {
        self.allowed.contains(n).then(|| self.source.shape(n)).flatten()
    }
    fn names(&self) -> Vec<String> {
        self.allowed.iter().cloned().collect()
    }
    fn metadata(&self, n: &str) -> Option<TensorMeta> {
        self.allowed.contains(n).then(|| self.source.metadata(n)).flatten()
    }
    fn load(&self, n: &str) -> misaka_palw_tir_lower::Result<Tensor> {
        self.mark(n)?;
        self.source.load(n)
    }
    fn read_slice(&self, n: &str, r: Range<u64>) -> misaka_palw_tir_lower::Result<Vec<u8>> {
        self.mark(n)?;
        self.source.read_slice(n, r)
    }
    fn load_rows(&self, n: &str, r: Range<usize>) -> misaka_palw_tir_lower::Result<Tensor> {
        self.mark(n)?;
        self.source.load_rows(n, r)
    }
    fn serves_row_ranges(&self) -> bool {
        true
    }
}
struct DirectorySink {
    directory: PathBuf,
    stage: u16,
    lifted: Option<u16>,
    remaining: u64,
    total: u64,
    total_cap: u64,
    current: Option<(std::fs::File, PathBuf, PathBuf)>,
    skip: bool,
}
impl TensorSink for DirectorySink {
    fn begin(&mut self, param: u16, layer: Option<u16>, bytes: u64) -> misaka_palw_tir_lower::Result<()> {
        if let Some((file, partial, _)) = self.current.take() {
            self.total =
                self.total.saturating_sub(file.metadata().map_err(|e| misaka_palw_tir_lower::LowerError::Io(e.to_string()))?.len());
            drop(file);
            std::fs::remove_file(partial).map_err(|e| misaka_palw_tir_lower::LowerError::Io(e.to_string()))?;
        }
        self.skip = self.lifted == Some(param);
        self.remaining = bytes;
        if self.skip {
            return Ok(());
        }
        if self.total.checked_add(bytes).is_none_or(|v| v > self.total_cap) {
            return Err(misaka_palw_tir_lower::LowerError::weights("PIXTRAL38_AGGREGATE_CONVERTED_BYTE_BUDGET"));
        }
        let param = if self.lifted.is_some_and(|lifted| param > lifted) { param - 1 } else { param };
        let final_path = self.directory.join(super::frontend::tensor_name(self.stage, param, layer));
        let partial = final_path.with_extension("partial");
        if final_path.exists() {
            return Err(misaka_palw_tir_lower::LowerError::weights("PIXTRAL38_DUPLICATE_COMPLETED_TENSOR"));
        }
        let file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)
            .map_err(|e| misaka_palw_tir_lower::LowerError::Io(e.to_string()))?;
        self.current = Some((file, partial, final_path));
        Ok(())
    }
    fn push(&mut self, b: &[u8]) -> misaka_palw_tir_lower::Result<()> {
        if b.len() as u64 > self.remaining {
            return Err(misaka_palw_tir_lower::LowerError::weights("PIXTRAL38_TENSOR_WRITE_EXTENT"));
        }
        if !self.skip {
            self.current
                .as_mut()
                .ok_or_else(|| misaka_palw_tir_lower::LowerError::weights("PIXTRAL38_PUSH_OUTSIDE_TENSOR"))?
                .0
                .write_all(b)
                .map_err(|e| misaka_palw_tir_lower::LowerError::Io(e.to_string()))?;
            self.total += b.len() as u64;
        }
        self.remaining -= b.len() as u64;
        Ok(())
    }
    fn end(&mut self) -> misaka_palw_tir_lower::Result<()> {
        if self.remaining != 0 {
            return Err(misaka_palw_tir_lower::LowerError::weights("PIXTRAL38_INCOMPLETE_TENSOR"));
        }
        if let Some((mut file, partial, final_path)) = self.current.take() {
            file.flush().map_err(|e| misaka_palw_tir_lower::LowerError::Io(e.to_string()))?;
            drop(file);
            std::fs::rename(partial, final_path).map_err(|e| misaka_palw_tir_lower::LowerError::Io(e.to_string()))?;
        }
        Ok(())
    }
}
