//! **The text encoder's two stages** (RFC-0003 §6): CLIP's text tower, lowered by the HF frontend's own causal-encoder
//! path (`hf_config::encoder::clip_text` → `lower` → [`crate::encoder::causal_v2`]), once as `Rows` (the per-token
//! hidden rows after the final LayerNorm — `last_hidden_state`, the denoiser's `prompt_embeds`) and once as `Final`
//! (the projected pooled vector at the `eos` — `text_embeds`, the denoiser's pooled projection).
//!
//! Both read the same checkpoint; the `Rows` stage is the config read as `CLIPTextModel` (no projection) and the
//! `Final` stage as `CLIPTextModelWithProjection`. SD3's own pipeline feeds the transformer the PENULTIMATE hidden
//! states of its CLIPs and a T5; the first fixture takes the final-norm rows (the denoiser does not care which
//! tensor the pipeline hands it, and a truncated-depth `Rows` program is a later refinement) — recorded as a
//! deliberate difference in the §II.1 checklist, not hidden.

use std::path::Path;
use std::sync::Arc;

use misaka_palw_tir::program_v2::TirProgramV2;
use misaka_palw_tir::{MapParams, ParamSource};

use crate::encoder::{self, EncoderOutput};
use crate::fidelity;
use crate::float_ref::ParamStore;
use crate::float_ref::stream::Resident;
use crate::lower::{IntParams, LowerOpts, materialise};
use crate::quant::QuantPolicy;
use crate::weights::Checkpoint;

/// One lowered text stage: its program, params and the unit of its output (a power of two).
pub struct ClipStage {
    pub program: TirProgramV2,
    pub params: MapParams,
    /// The float value of one code of the output (`2^-q`).
    pub unit: f64,
}

/// `IntParams` as the `MapParams` a pipeline run reads.
pub fn int_to_map(p: &IntParams) -> MapParams {
    let mut out = MapParams::default();
    for (j, layer) in p.tensors.keys() {
        out.tensors.insert((*j, *layer), p.param(*j, *layer).expect("the key is present"));
    }
    out
}

/// Lower the CLIP text tower in `dir` (`config.json` + `model.safetensors`) as `output`. `projection` reads the config as
/// `CLIPTextModelWithProjection` (the pooled `text_embeds`), else as `CLIPTextModel` (`last_hidden_state`). `calib` are
/// token sequences (templated with the class's bos/eos/pad) the encoder is calibrated on.
pub fn lower_clip_stage(dir: &Path, projection: bool, output: EncoderOutput, calib: &[Vec<usize>]) -> Result<ClipStage, String> {
    let text = std::fs::read_to_string(dir.join("config.json")).map_err(|e| format!("{}: {e}", dir.join("config.json").display()))?;
    let mut cfg: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("config.json: {e}"))?;
    cfg["architectures"] = serde_json::json!([if projection { "CLIPTextModelWithProjection" } else { "CLIPTextModel" }]);
    let cfg_text = cfg.to_string();
    let e = |x: crate::error::LowerError| x.to_string();
    // An encoder never runs past its learned positions: its window is its context.
    let ctx = fidelity::prepare(&cfg_text, &LowerOpts::default())
        .map_err(e)?
        .spec
        .max_position_embeddings
        .ok_or("the text tower has no learned position table")?;
    // `table_shift: 1`: the MLP's activation is a table of two artifact pieces, not four — a close at it opens every piece its tile's
    // lookups can land in (an executor's activations can land in all), and four pieces are past a carrier (PALW-GEN-20).
    let prep = fidelity::prepare(&cfg_text, &LowerOpts { max_window: Some(ctx as u32), table_shift: 1, ..LowerOpts::default() }).map_err(e)?;
    let ck = Checkpoint::open(dir).map_err(e)?;
    let (params_f, _) = ParamStore::from_source(&prep.hl, &prep.binding, &ck).map_err(e)?;
    let loader = Resident(Arc::new(params_f));
    let quiet = |_: usize, _: usize| {};
    let stats = fidelity::calibrate(&prep.hl, &loader, calib, &quiet).map_err(e)?;
    let mat = materialise(&prep.lowered, &prep.hl, &loader, &stats, &QuantPolicy::default(), &quiet).map_err(e)?;
    let q = (-mat.logits_scale.log2()).round() as i32;
    if 2f64.powi(-q) != mat.logits_scale {
        return Err(format!("the encoder's output unit {} is not a power of two", mat.logits_scale));
    }
    let program = encoder::causal_v2(&prep.lowered, output).map_err(e)?;
    Ok(ClipStage { program, params: int_to_map(&mat.params), unit: mat.logits_scale })
}
