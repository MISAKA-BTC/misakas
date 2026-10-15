//! Actual HF-source decoder compiler, including an explicit upstream image-row input.
//! Conversion produces a new program/PC/R. Calibration is not source-fidelity evidence.
use super::vision38::write_materialised_stage_v38;
use misaka_palw_tir_lower::{
    fidelity::Prepared,
    float_ref::SiteStat,
    lower::{ImageRows, LowerOpts},
    quant::QuantPolicy,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecoderImageRowsV38 {
    pub upstream_stage: u16,
    pub rows: u32,
    pub width: u32,
    pub q: u8,
    pub placeholder: u32,
    pub minimum: i64,
    pub maximum: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecoderFrontendV38 {
    pub version: u16,
    pub statistics_file: String,
    pub statistics_sha256: String,
    /// A built-in data adapter ID, auto or none. Files require the public frontend-pack route.
    pub adapter: String,
    pub original_context: u32,
    pub history_bound: u32,
    pub linear_weight_bits: u8,
    pub gdn_core_wide: bool,
    pub table_shift: u8,
    pub headroom16: f64,
    pub headroom32: f64,
    pub headroom_resid: f64,
    pub image_rows: Option<DecoderImageRowsV38>,
}
pub(super) struct PreparedDecoderV38 {
    pub program: misaka_palw_tir::program_v2::TirProgramV2,
    prepared: Prepared,
    statistics: BTreeMap<String, SiteStat>,
    policy: QuantPolicy,
}
impl PreparedDecoderV38 {
    pub fn prepare(config: &serde_json::Value, p: &DecoderFrontendV38, statistics: &str) -> Result<Self, String> {
        use misaka_palw_tir_lower::hf_schema::{AdapterChoice, ReadOptions};
        if p.version != 38
            || p.original_context == 0
            || p.original_context > p.history_bound
            || !matches!(p.linear_weight_bits, 8 | 16)
            || p.table_shift > 8
            || [p.headroom16, p.headroom32, p.headroom_resid].iter().any(|v| *v <= 0.0 || !v.is_finite())
        {
            return Err("DECODER38_EXPLICIT_PROFILE_OR_CONTEXT_POLICY".into());
        }
        let adapter = match p.adapter.as_str() {
            "auto" => AdapterChoice::Auto,
            "none" => AdapterChoice::None,
            value if value.starts_with("builtin:") => AdapterChoice::BuiltIn(value[8..].to_owned()),
            _ => return Err("DECODER38_UNPINNED_ADAPTER_FILE_USE_FRONTEND_PACK".into()),
        };
        let mut opts = LowerOpts {
            history_bound: p.history_bound,
            max_window: Some(p.original_context),
            linear_weight_bits: p.linear_weight_bits,
            gdn_core_wide: p.gdn_core_wide,
            table_shift: p.table_shift,
            ..Default::default()
        };
        if let Some(image) = &p.image_rows {
            if image.rows == 0
                || image.width == 0
                || image.q > 62
                || image.minimum > image.maximum
                || config["image_token_index"].as_u64() != Some(image.placeholder as u64)
            {
                return Err("DECODER38_ORIGINAL_IMAGE_PLACEMENT_DIFFERS".into());
            }
            opts.image_rows = Some(ImageRows {
                rows: image.rows as usize,
                width: image.width as usize,
                unit: 2f64.powi(-(image.q as i32)),
                placeholder: image.placeholder,
                mrope: None,
            });
        }
        let prepared = misaka_palw_tir_lower::fidelity::prepare_read(
            &config.to_string(),
            &ReadOptions { adapter },
            &opts,
            misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin(),
        )
        .map_err(|e| format!("DECODER38_SOURCE_LOWERING: {e}"))?;
        if prepared.spec.max_position_embeddings != Some(p.original_context as usize) {
            return Err("DECODER38_ORIGINAL_DECLARED_CONTEXT_NOT_EXACT".into());
        }
        if let Some(image) = &p.image_rows {
            if prepared.spec.hidden_size != image.width as usize {
                return Err("DECODER38_IMAGE_HIDDEN_WIDTH_DIFFERS".into());
            }
        }
        let output = misaka_palw_tir::program_v2::OutputDecl::Logits { node: prepared.lowered.program.logits };
        let lifts = p
            .image_rows
            .as_ref()
            .map(|i| {
                vec![(
                    misaka_palw_tir_lower::lower::IMAGE_ROWS_PARAM,
                    misaka_palw_tir::program_v2::InputSource::External { lo: i.minimum, hi: i.maximum },
                )]
            })
            .unwrap_or_default();
        let program = misaka_palw_tir_lower::encoder::lift_v2(&prepared.lowered, &lifts, output).map_err(|e| e.to_string())?;
        let statistics = misaka_palw_tir_lower::calib::stats_from_json_exact(statistics).map_err(|e| e.to_string())?;
        let policy = QuantPolicy { headroom16: p.headroom16, headroom32: p.headroom32, headroom_resid: p.headroom_resid };
        Ok(Self { program, prepared, statistics, policy })
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
        let lifted = (!self.program.inputs.is_empty()).then_some(misaka_palw_tir_lower::lower::IMAGE_ROWS_PARAM);
        write_materialised_stage_v38(
            &self.prepared.lowered,
            &self.prepared.hl,
            &self.prepared.binding,
            &self.statistics,
            &self.policy,
            checkpoint,
            allowed,
            directory,
            stage,
            block_bytes,
            total_cap,
            lifted,
        )
    }
}
