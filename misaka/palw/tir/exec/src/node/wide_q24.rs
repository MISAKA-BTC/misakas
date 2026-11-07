//! Explicit wide output capture and authenticated event construction; legacy capture magic is unchanged.
use super::tree::TirStepTreeV1;
use kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1;
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::{Hash64, palw_logits_q24_v2 as wide};
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub struct TirWideCaptureV2 {
    pub binding: PalwTirStepBindingV1,
    pub prompt: Vec<u32>,
    pub logits_rows: Vec<Vec<i64>>,
    pub generated: Vec<u32>,
    pub leaves: Vec<PalwStepTileLeafV1>,
}
impl TirWideCaptureV2 {
    pub const MAGIC: [u8; 8] = *b"PALWTRC2";
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Self::MAGIC.to_vec();
        b.extend(borsh::to_vec(self).expect("capture serialization"));
        b
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if !bytes.starts_with(&Self::MAGIC) {
            return Err("wide capture magic".into());
        }
        super::capture_codec::decode_wide(&bytes[8..], bytes.len())
    }
    pub fn event(&self, row: u32, tile: u32) -> Option<wide::WideLogitsEventV2> {
        event(&self.binding.job_context, &self.logits_rows, &self.generated, row, tile)
    }
    pub fn authenticate(&self, cap: u64) -> Result<(), String> {
        kaspa_consensus_core::palw_tir_step_v1::verify_tir_binding_v1(&self.binding, cap).map_err(|e| e.to_string())?;
        let program = self.binding.class.decode_program().map_err(|e| e.to_string())?;
        let vocab = program.blocks[program.schedule.post as usize].nodes[program.logits as usize].out.elements_at(1) as usize;
        if self.logits_rows.iter().any(|r| r.len() != vocab)
            || wide::trace_root(&self.binding.job_context, &self.logits_rows, &self.generated)
                != Some(self.binding.full_logits_trace_root)
        {
            return Err("wide capture trace".into());
        }
        Ok(())
    }
}
pub fn event(ctx: &PalwJobContextV2, rows: &[Vec<i64>], ids: &[u32], row: u32, tile: u32) -> Option<wide::WideLogitsEventV2> {
    let roots: Vec<Hash64> =
        rows.iter().enumerate().map(|(i, r)| wide::row_root(&ctx.context_hash(), i as u32, r)).collect::<Option<_>>()?;
    let row_root = *roots.get(row as usize)?;
    let row_opening = TirStepTreeV1::full(&roots).opening(row as u64)?;
    let lanes = rows.get(row as usize)?;
    let tile_hashes: Vec<_> = lanes
        .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
        .enumerate()
        .map(|(i, l)| wide::tile_leaf(&ctx.context_hash(), row, i as u32, l))
        .collect();
    let tile_opening = TirStepTreeV1::full(&tile_hashes).opening(tile as u64)?;
    let lanes = lanes.chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES).nth(tile as usize)?.to_vec();
    Some(wide::WideLogitsEventV2 { row, generated: ids.to_vec(), row_root, row_opening, lanes, tile_opening })
}
