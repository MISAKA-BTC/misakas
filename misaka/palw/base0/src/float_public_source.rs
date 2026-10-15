//! **The producer/custodian side of the greedy Float32 Constraint source** (behind
//! `palw_staged_float_constraint_v1`).
//!
//! A `Float32` class commits each decode call's whole f32 logits row as one keyed event hash
//! under its v2 event tree. The node's custodian responder answers a staged Float32 source from
//! two places: the producer's own sidecar `{claim}.float-public`, or this backend's projection of
//! a retained payload, which is what a Panel custodian holds.
//!
//! **The retained material of a Float32 run is the generic public sidecar itself**
//! ([`kaspa_consensus_core::palw_float_constraint_source_v1`]: `MSKF32P1`, the ids, summary and
//! event hashes, then the f32 rows). [`float_public_material_v1`] builds it from the run's rows
//! and ids. A Float32 executor retains it as its run material: the node wraps it with the job in
//! the FP capture, and Panel custodians receive and retain that capture. Because the rows ARE the
//! material:
//! * the producer's `retain_float_v1` copies it to the claim's sidecar
//!   ([`write_float_public_source_v1`]);
//! * every custodian's responder opens its retained payload range directly
//!   ([`open_float_public_source_v1`]).
//!
//! Both files fall under the existing claim-scoped lease and shared GC
//! (`SharedClaimCacheGcV1`, `UnpublishedProjectionV1`, the FP payload retention). No private
//! state is retained, and nothing here executes.
//!
//! The integer families (BASE-0, A16, Qwen36) never write this material, so for their runs
//! [`write_float_public_source_v1`] answers `false` and [`open_float_public_source_v1`] refuses.
//! Every byte is authenticated by the chain against the claim's trace root; a bad sidecar costs
//! the custodian its own unpaid slot, never a verdict.
use kaspa_consensus_core::{
    Hash64,
    palw_backend::{PalwFloatPublicSourceReaderV1, PalwOriginalCaptureReaderV1},
    palw_float_constraint_source_v1::{
        PALW_FLOAT_PUBLIC_SIDECAR_MAGIC_V1, PalwFloatPublicSidecarHeaderV1, PalwFloatPublicSidecarReaderV1, write_float_public_sidecar_v1,
    },
    palw_float_decode_v1::palw_float32_event_phase_v1,
    palw_step_leg::PalwStepBindingV2,
    palw_step_refute::PalwDecodeTokensV1,
    palw_v2::{
        PalwJobContextV2, PalwLogitsDtypeV2, PalwStopReasonV2, PalwTracePhaseV2, PalwTraceSummaryV2, full_logits_trace_root_v2,
        logits_event_hash_v2, output_token_ids_hash_v2, trace_event_merkle_root_v2,
    },
};
use std::sync::Arc;

/// The retained public material of one greedy Float32 run: the generic sidecar bytes over its
/// committed rows (one per decode call; event `i` selects generated id `i`) and its ids. A
/// non-finite logit is refused, as the reference worker aborts before committing one. Returns
/// the material and the v2 trace root it commits, which the producer's binding must carry.
pub fn float_public_material_v1(
    job_context: &PalwJobContextV2,
    rows: &[Vec<f32>],
    generated: &[u32],
) -> Result<(Vec<u8>, Hash64), String> {
    let decode = job_context.exact_decode_tokens as usize;
    let vocab = rows.first().map(Vec::len).ok_or("a Float32 run has at least one decode row")?;
    if rows.len() != decode || generated.len() != decode || vocab == 0 || rows.iter().any(|row| row.len() != vocab) {
        return Err("Float32 material needs one full-vocabulary row per generated id".into());
    }
    let context = job_context.context_hash();
    let n_vocab = u32::try_from(vocab).map_err(|_| "Float32 vocabulary exceeds u32")?;
    let mut scratch = Vec::new();
    let events = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let (phase, step) = palw_float32_event_phase_v1(i as u32);
            logits_event_hash_v2(&context, phase, step, i as u32, n_vocab, row, &mut scratch).map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<Hash64>, String>>()?;
    let ordered = trace_event_merkle_root_v2(&events).map_err(|e| e.to_string())?;
    let summary = PalwTraceSummaryV2 {
        vocab_size: n_vocab,
        logits_dtype: PalwLogitsDtypeV2::F32Le,
        declared_prefill_tokens: job_context.declared_prefill_tokens,
        exact_decode_tokens: job_context.exact_decode_tokens,
        event_count: job_context.exact_decode_tokens,
        first_event_kind: PalwTracePhaseV2::Prefill,
        last_event_kind: if decode == 1 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode },
        output_token_ids_hash: output_token_ids_hash_v2(generated),
        stop_reason: PalwStopReasonV2::ExactBudgetReached,
    };
    let root = full_logits_trace_root_v2(&context, &summary, &ordered);
    let header = PalwFloatPublicSidecarHeaderV1 {
        job_context_hash: context,
        ids: PalwDecodeTokensV1 { summary, ordered_event_commitment: ordered, generated_token_ids: generated.to_vec() },
        events,
    };
    let mut material = Vec::new();
    write_float_public_sidecar_v1(&mut material, &header, |i| Ok(rows[i as usize].clone()))?;
    Ok((material, root))
}

struct BorrowedV1<'a>(&'a [u8]);
impl PalwOriginalCaptureReaderV1 for BorrowedV1<'_> {
    fn len_v1(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_v1(&self, offset: u64, length: u32) -> Result<Vec<u8>, String> {
        let start = usize::try_from(offset).map_err(|_| "borrowed Float32 offset overflow")?;
        let end = start.checked_add(length as usize).ok_or("borrowed Float32 range overflow")?;
        self.0.get(start..end).map(<[u8]>::to_vec).ok_or_else(|| "borrowed Float32 material truncated".into())
    }
}

/// The producer's capture-save projection: the run's material, when it is a Float32 run's,
/// copied to the claim's sidecar in bounded chunks after checking that it is one committed
/// trace. `false` for any other family's material.
pub fn write_float_public_source_v1(material: &[u8], out: &mut dyn std::io::Write) -> Result<bool, String> {
    if material.len() < 12 || &material[..8] != PALW_FLOAT_PUBLIC_SIDECAR_MAGIC_V1 {
        return Ok(false);
    }
    // Structural check only (one committed trace, exact length); the chain authenticates.
    PalwFloatPublicSidecarReaderV1::open_v1(Arc::new(BorrowedV1(material)) as Arc<dyn PalwOriginalCaptureReaderV1>)?;
    for chunk in material.chunks(16_384) {
        out.write_all(chunk).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// A custodian's or producer's bounded reader over retained Float32 material (or the sidecar),
/// refused unless it is THIS binding's run: a Float32 lane, the same vocabulary, context, decode
/// count and v2 trace root.
pub fn open_float_public_source_v1(
    source: Arc<dyn PalwOriginalCaptureReaderV1>,
    binding: &PalwStepBindingV2,
) -> Result<Box<dyn PalwFloatPublicSourceReaderV1>, String> {
    if binding.shape_profile.lane != kaspa_consensus_core::palw_step::PalwStepLaneV1::Float32 {
        return Err("an integer class has no Float32 public rows".into());
    }
    let reader = PalwFloatPublicSidecarReaderV1::open_v1(source)?;
    let header = reader.header_v1();
    let context = binding.job_context.context_hash();
    let summary = &header.ids.summary;
    if header.job_context_hash != context
        || summary.vocab_size != binding.shape_profile.vocab_size
        || summary.exact_decode_tokens != binding.job_context.exact_decode_tokens
        || full_logits_trace_root_v2(&context, summary, &header.ids.ordered_event_commitment) != binding.full_logits_trace_root
    {
        return Err("retained Float32 material is not this claim's committed run".into());
    }
    Ok(Box::new(reader))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatSourceActionV1 as Action;
    use kaspa_consensus_core::palw_float_constraint_source_v1::PalwFloatSourceCursorV1 as Cursor;

    /// A producer retains its Float32 run as the sidecar material. The producer's projection copies
    /// it, and a custodian's copy, read through the bounded reader, streams a row that
    /// re-derives the committed trace root and yields the reference worker's greedy lane.
    #[test]
    fn retained_float_material_streams_an_authenticated_row_from_any_retained_copy() {
        let mut context = kaspa_consensus_core::palw_base0_profile::rc_job_context(
            &kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
                .expect("base0 geometry"),
            2,
            3,
        );
        context.exact_decode_tokens = 3;
        let vocab = 20_000usize;
        let rows: Vec<Vec<f32>> =
            (0..3).map(|r| (0..vocab).map(|l| ((l * 7 + r * 13) % 101) as f32 * 0.5 - 20.0).collect()).collect();
        let generated = vec![4u32, 77, 19_999];
        let (material, root) = float_public_material_v1(&context, &rows, &generated).unwrap();
        let mut sidecar = Vec::new();
        assert!(write_float_public_source_v1(&material, &mut sidecar).unwrap());
        assert_eq!(sidecar, material);
        assert!(!write_float_public_source_v1(b"MSKFLAT1 an integer family's capture", &mut Vec::new()).unwrap());
        let mut reader =
            PalwFloatPublicSidecarReaderV1::open_v1(Arc::new(BorrowedV1(&sidecar)) as Arc<dyn PalwOriginalCaptureReaderV1>).unwrap();
        let header = reader.header_v1().clone();
        assert_eq!(full_logits_trace_root_v2(&context.context_hash(), &header.ids.summary, &header.ids.ordered_event_commitment), root);
        let position = 1u32;
        let mut stream =
            kaspa_consensus_core::palw_float_decode_v1::PalwFloatRowStreamV1::new_v1(&context.context_hash(), position, vocab as u32, &[77])
                .unwrap();
        loop {
            let offset = stream.lanes_read_v1();
            if offset == vocab as u32 {
                break;
            }
            let Some(Action::RowPart { offset: at, bytes }) = reader.poll_v1(Cursor::RowPart { offset }, Some(position)).unwrap() else {
                panic!("a row part")
            };
            assert!(bytes.len() <= 65_536);
            stream.part_v1(at, &bytes).unwrap();
        }
        let Some(Action::RowFinish { opening }) = reader.poll_v1(Cursor::RowFinish, Some(position)).unwrap() else { panic!("a finish") };
        let facts =
            stream.finish_v1(&context.context_hash(), &root, &header.ids.summary, &header.ids.ordered_event_commitment, &opening).unwrap();
        assert_eq!(facts.greedy_lane as usize, kaspa_consensus_core::palw_float_decode_v1::palw_float32_greedy_select_v1(&rows[1]).unwrap());
    }
}
