//! Same capture Borsh framing, with allocation checks before every peer-controlled Vec.
//!
//! A logits outer Vec can carry 24 native bytes per four encoded bytes (empty rows).
//! Checking the total frame length or authenticating metadata after generic Borsh decode
//! does not bound that allocation. The decoder charges the exact reserved element storage
//! and rejects truncated lengths before reserving. False values remain available to courts;
//! this is resource admission, not a judgement of the capture's commitments or honesty.

use borsh::BorshDeserialize;
use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
use kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1;
use kaspa_consensus_core::palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

use super::backend::TirCaptureV1;

// The node already reserves 3 * encoded frame + 16 MiB. Leave one encoded frame's
// worth for the fetched bytes, bounding requested decoded storage by the remainder.
// Allocator overhead, class verification and subsequent prover working sets still need
// their own node admission/RSS measurements; this is not an all-model RSS certificate.
const DECODE_FIXED_HEADROOM: usize = 16 << 20;

pub(super) fn decode(body: &[u8], frame_length: usize) -> Result<TirCaptureV1, String> {
    let limit = frame_length
        .checked_mul(2)
        .and_then(|n| n.checked_add(DECODE_FIXED_HEADROOM))
        .ok_or("the IR capture allocation bound overflows")?;
    decode_with_limit(body, limit).map_err(|e| format!("the IR capture does not decode: {e}"))
}

struct Reader<'a> {
    remaining: &'a [u8],
    storage_left: usize,
}

impl Reader<'_> {
    fn scalar<T: BorshDeserialize>(&mut self) -> Result<T, String> {
        T::deserialize_reader(&mut self.remaining).map_err(|e| e.to_string())
    }

    fn reserve<T>(&mut self, count: usize) -> Result<Vec<T>, String> {
        let bytes = count.checked_mul(std::mem::size_of::<T>()).ok_or("capture allocation overflows")?;
        self.storage_left = self.storage_left.checked_sub(bytes).ok_or("capture decoded storage exceeds its reservation")?;
        let mut out = Vec::new();
        out.try_reserve_exact(count).map_err(|e| format!("capture allocation refused: {e}"))?;
        Ok(out)
    }

    fn length(&mut self, minimum_wire_bytes: usize) -> Result<usize, String> {
        let n = usize::try_from(self.scalar::<u32>()?).map_err(|_| "capture length does not fit usize")?;
        if minimum_wire_bytes == 0 || n > self.remaining.len() / minimum_wire_bytes {
            return Err("capture vector length exceeds the remaining frame".into());
        }
        Ok(n)
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let n = self.length(1)?;
        let mut out = self.reserve::<u8>(n)?;
        out.extend_from_slice(&self.remaining[..n]);
        self.remaining = &self.remaining[n..];
        Ok(out)
    }

    fn words<T: BorshDeserialize>(&mut self) -> Result<Vec<T>, String> {
        let n = self.length(4)?;
        let mut out = self.reserve::<T>(n)?;
        for _ in 0..n {
            out.push(self.scalar()?);
        }
        Ok(out)
    }
}

fn decode_with_limit(body: &[u8], limit: usize) -> Result<TirCaptureV1, String> {
    let mut r = Reader {
        remaining: body,
        storage_left: limit.checked_sub(std::mem::size_of::<TirCaptureV1>()).ok_or("capture allocation bound is too small")?,
    };
    let version = r.scalar()?;
    // All lengths inside the binding are framed too. Generic binding deserialization
    // would otherwise reserve these before this decoder can account for their storage.
    let job_context = PalwJobContextV2 {
        version: r.scalar()?,
        network_id: r.bytes()?,
        job_id: r.scalar()?,
        job_nullifier: r.scalar()?,
        assignment_id: r.scalar()?,
        execution_seed: r.scalar()?,
        model_profile_id: r.scalar()?,
        runtime_manifest_hash: r.scalar()?,
        runtime_class_id: r.scalar()?,
        shape_profile_id: r.scalar()?,
        trace_scheme_id: r.scalar()?,
        cu_ruleset_id: r.scalar()?,
        tokenizer_id: r.scalar()?,
        prompt_token_ids_hash: r.scalar()?,
        declared_prefill_tokens: r.scalar()?,
        exact_decode_tokens: r.scalar()?,
        max_context_tokens: r.scalar()?,
    };
    let class = PalwTirClassV1 {
        version: r.scalar()?,
        program: r.bytes()?,
        layout: PalwTirLayoutV1 {
            version: r.scalar()?,
            max_context: r.scalar()?,
            checkpoint_interval: r.scalar()?,
            h_tile: r.scalar()?,
            commit_tiles: r.words::<u32>()?,
            state_tiles: r.words::<u32>()?,
        },
        tokenizer_id: r.scalar()?,
    };
    let binding = PalwTirStepBindingV1 {
        version,
        job_context,
        class,
        artifact_root: r.scalar()?,
        full_logits_trace_root: r.scalar()?,
        step_leaf_count: r.scalar()?,
        step_merkle_root: r.scalar()?,
        committed_execution_root: r.scalar()?,
    };
    let prompt = r.words::<u32>()?;
    let row_count = r.length(4)?;
    let mut logits_rows = r.reserve::<Vec<i32>>(row_count)?;
    for _ in 0..row_count {
        logits_rows.push(r.words::<i32>()?);
    }
    let generated = r.words::<u32>()?;
    // u16 version, four coordinate u32s, value_count u32 and values Vec length u32.
    let leaf_count = r.length(26)?;
    let mut leaves = r.reserve::<PalwStepTileLeafV1>(leaf_count)?;
    for _ in 0..leaf_count {
        leaves.push(PalwStepTileLeafV1 {
            version: r.scalar()?,
            coord: r.scalar::<PalwStepCoordinateV1>()?,
            value_count: r.scalar()?,
            values_le: r.bytes()?,
        });
    }
    if !r.remaining.is_empty() {
        return Err("trailing bytes after the IR capture".into());
    }
    Ok(TirCaptureV1 { binding, prompt, logits_rows, generated, leaves })
}
