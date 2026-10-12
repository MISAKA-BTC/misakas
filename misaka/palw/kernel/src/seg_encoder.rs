//! **K2-TIR-v5: encoders and heads as ONE position whose ids are the job's** — `docs/design/palw/k2-real-scale.md` §12.
//!
//! A bidirectional encoder is lowered as one position over a padded token axis (`misaka-palw-tir-lower::lower::bidir`): every value
//! is a `[L, …]` tensor of all rows at once, and the program reads the job's ids and their count as two params. In the version-1 view
//! of the encoder's version-2 program those two are the LAST params: `input.ids` (`idx [L]`) and `input.count` (`idx []`). Under
//! K2-TIR-v5 they are the job's input, not the artifact ([`EncoderBindingV1`]):
//!
//! * the ids are the tiled job's prompt (the template-applied ids the gateway forms: `[CLS] ‖ text ‖ [SEP]`), padded to `L` with
//!   [`ENCODER_PAD_ID_V1`]. A pad row never reaches a real row or the pooling (every key at or past `count` is masked), so the pad id
//!   does not change the result;
//! * the count is the prompt's length;
//! * a court reads the ids through the job's prompt tile (`ElementFaultV1::token`; `L ≤ 4,096`, so one tile holds them all) and the
//!   count from the job.
//!
//! Everything else is K2-TIR-v4's: every value committed under the one position root, element courts, per-position DA, the gate, and
//! the re-execution detector (`crate::seg_detect`).

use misaka_palw_tir::program::{INPUT_TOKEN, Ref, TirProgramV1};
use misaka_palw_tir::{DType, ParamSource, Tensor};

use crate::seg::PROMPT_TILE_IDS_V1;
use crate::trace::{StageBindingV1, TraceV1, trace_stage_v1};

/// The name of an encoder's ids param (`idx [L]`).
pub const ENCODER_IDS_PARAM_V1: &str = "input.ids";
/// The name of an encoder's count param (`idx []`).
pub const ENCODER_COUNT_PARAM_V1: &str = "input.count";
/// The id the semantics pad the job's ids with, up to `L`.
pub const ENCODER_PAD_ID_V1: u32 = 0;

/// **Where an encoder program's job input sits**: the index of its first input param (`input.ids`; `input.count` follows) and the
/// padded axis `L`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncoderBindingV1 {
    pub first_input: u16,
    pub l: u32,
}

impl EncoderBindingV1 {
    /// The wiring's stage binding: params from `first_input` are inputs 0 (the ids) and 1 (the count).
    pub fn stage(&self) -> StageBindingV1 {
        StageBindingV1 { first_input: self.first_input, post_writers: Vec::new(), inputs: 2 }
    }

    /// **The job's two input tensors**: the prompt's ids padded to `L` with [`ENCODER_PAD_ID_V1`], and its length.
    pub fn inputs(&self, prompt: &[u32]) -> Result<(Tensor, Tensor), String> {
        Ok((self.ids(prompt)?, self.count(prompt.len() as u32)?))
    }

    /// Input 0: the prompt's ids padded to `L`.
    pub fn ids(&self, prompt: &[u32]) -> Result<Tensor, String> {
        if prompt.is_empty() || prompt.len() > self.l as usize {
            return Err(format!("an encoder job of {} ids for an axis of {}", prompt.len(), self.l));
        }
        let mut ids: Vec<i128> = prompt.iter().map(|t| *t as i128).collect();
        ids.resize(self.l as usize, ENCODER_PAD_ID_V1 as i128);
        Tensor::new(DType::Idx, vec![self.l as usize], ids).map_err(|e| e.to_string())
    }

    /// Input 1: the prompt's length (the job states it; no tile is read).
    pub fn count(&self, prompt_len: u32) -> Result<Tensor, String> {
        if prompt_len == 0 || prompt_len > self.l {
            return Err(format!("an encoder job of {prompt_len} ids for an axis of {}", self.l));
        }
        Tensor::scalar(DType::Idx, prompt_len as i128).map_err(|e| e.to_string())
    }
}

/// **The encoder binding of a program**, or why it has none: its last two params are `input.ids` (`idx [L]`, global, `1 ≤ L ≤ 4,096`)
/// and `input.count` (`idx []`, global), and no node reads the per-position token (the ids are the input).
pub fn encoder_binding_v1(program: &TirProgramV1) -> Result<EncoderBindingV1, String> {
    let n = program.params.len();
    if n < 2 {
        return Err("an encoder program declares its ids and count as its last two params".into());
    }
    let (ids, count) = (&program.params[n - 2], &program.params[n - 1]);
    if ids.name != ENCODER_IDS_PARAM_V1 || count.name != ENCODER_COUNT_PARAM_V1 {
        return Err(format!(
            "an encoder program's last two params are `{ENCODER_IDS_PARAM_V1}` and `{ENCODER_COUNT_PARAM_V1}`, not `{}` and `{}`",
            ids.name, count.name
        ));
    }
    if ids.dtype != DType::Idx || ids.shape.len() != 1 || ids.per_layer {
        return Err(format!("`{ENCODER_IDS_PARAM_V1}` is a global `idx [L]`"));
    }
    if count.dtype != DType::Idx || !count.shape.is_empty() || count.per_layer {
        return Err(format!("`{ENCODER_COUNT_PARAM_V1}` is a global `idx []`"));
    }
    let l = ids.shape[0];
    if l == 0 || l as usize > PROMPT_TILE_IDS_V1 {
        return Err(format!("an encoder axis of {l} ids: K2-TIR-v5 reads the ids from one prompt tile (1 ..= {PROMPT_TILE_IDS_V1})"));
    }
    if program
        .blocks
        .iter()
        .flat_map(|b| &b.nodes)
        .any(|node| node.inputs.iter().any(|r| matches!(r, Ref::Input(j) if *j == INPUT_TOKEN)))
    {
        return Err("an encoder program reads its ids as its input, never the per-position token".into());
    }
    Ok(EncoderBindingV1 { first_input: (n - 2) as u16, l })
}

/// **The encoder's ranges, proven with its inputs' intervals** (the exact-result rule, RFC-0002 spec 04b §7). PALW-TIR v1's analysis
/// reads a param as its dtype's whole range, so a Gather by the ids would be refused as unbounded. The ids are the job's, in
/// `[0, token_bound − 1]` (every posted tile is checked against the class's token bound), and the count is in `[0, L]`. So the view is lifted
/// back to a version-2 program with those two inputs declared `External` and proven by the version-2 analysis (`interval_v2`), exactly
/// as a pipeline stage's view is (`crate::check::RangeRuleV1::ProvenByV2`).
pub fn prove_encoder_ranges_v1(program: &TirProgramV1, binding: &EncoderBindingV1) -> Result<(), String> {
    use misaka_palw_tir::program_v2::{InputSource, OutputDecl, TirProgramV2};
    if program.token_bound == 0 {
        return Err("an encoder program with no id".into());
    }
    let lifts = [
        (binding.first_input, InputSource::External { lo: 0, hi: program.token_bound as i64 - 1 }),
        (binding.first_input + 1, InputSource::External { lo: 0, hi: binding.l as i64 }),
    ];
    let p2 = TirProgramV2::from_v1_lifting_params(program, &lifts, OutputDecl::Final { node: program.logits })
        .map_err(|e| format!("the encoder's version-2 form: {e}"))?;
    misaka_palw_tir::interval_v2::analyze_ranges_v2(&p2)
        .map(|_| ())
        .map_err(|e| format!("the encoder's ranges are not proven (an exact primitive can overflow): {e}"))
}

/// **The trace of an encoder job**: the one position, with the job's ids and count as the inputs.
pub fn trace_encoder_v1(
    program: &TirProgramV1,
    params: &dyn ParamSource,
    binding: &EncoderBindingV1,
    prompt: &[u32],
) -> Result<TraceV1, String> {
    let (ids, count) = binding.inputs(prompt)?;
    trace_stage_v1(program, Some(&binding.stage()), params, &[0], &|k, _| match k {
        0 => Some(ids.clone()),
        1 => Some(count.clone()),
        _ => None,
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_without_the_two_trailing_inputs_has_no_encoder_binding() {
        let fx = misaka_palw_tir_sketch::fixture::wide128_v1(1);
        assert!(encoder_binding_v1(&fx.program).is_err(), "a decoder reads the per-position token and has no ids param");
        let b = EncoderBindingV1 { first_input: 3, l: 8 };
        let (ids, count) = b.inputs(&[5, 6, 7]).unwrap();
        assert_eq!(ids.data, vec![5, 6, 7, 0, 0, 0, 0, 0]);
        assert_eq!((count.data.clone(), count.shape.len()), (vec![3], 0));
        assert!(b.inputs(&[]).is_err() && b.inputs(&[1; 9]).is_err(), "between one id and the axis");
    }
}
