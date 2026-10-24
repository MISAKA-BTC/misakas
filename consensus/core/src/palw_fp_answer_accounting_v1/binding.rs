//! Transaction accounting answer binding. Served-material FPA1 is deliberately absent from this codec.
use super::{PALW_FP_ANSWER_ENVELOPE_MAGIC_V1, palw_fp_answer_envelope_view_v1};
use crate::{
    Hash64,
    palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PalwFreePromptCommitmentV3},
    palw_native_resource_v1::{
        NativeDecodeErrorV1, NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1, preflight_native_value_v1,
        reserve_native_requests_v1,
    },
    palw_v2::PalwJobContextV2,
};
type Fault = NativeDecodeErrorV1<&'static str>;
fn invalid(why: &'static str) -> Fault {
    NativeDecodeErrorV1::Decode(why)
}

impl NativeWireScanV1 for PalwFpCommitmentTxPayloadV3 {
    fn scan_native_wire_v1<R: std::io::Read>(r: &mut NativeWireReaderV1<R>) -> std::io::Result<NativeWireShapeV1> {
        <(u16, PalwFreePromptCommitmentV3, Vec<u32>, Vec<u8>)>::scan_native_wire_v1(r)
    }
}

/// Locate the old literal original inside a previously validated accounting inner payload. This is not an envelope validator.
/// Exactly the specified order: FPA1 outside plain/TFP1/HFP1/TFO1, and TFO1 around a literal TFP1.
pub fn palw_fp_accounting_original_bytes_v1(inner: &[u8]) -> Option<&[u8]> {
    fn bytes_at(payload: &[u8], offset: usize, cap: usize) -> Option<&[u8]> {
        let count = u32::from_le_bytes(payload.get(offset..offset.checked_add(4)?)?.try_into().ok()?) as usize;
        if count == 0 || count > cap {
            return None;
        }
        let start = offset.checked_add(4)?;
        payload.get(start..start.checked_add(count)?)
    }
    use crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES as FP_MAX;
    let original = if inner.starts_with(b"TFP1") {
        bytes_at(inner, 14, FP_MAX)?
    } else if inner.starts_with(b"HFP1") {
        bytes_at(inner, 6, FP_MAX)?
    } else if inner.starts_with(b"TFO1") {
        let tfp = bytes_at(inner, 6, crate::palw_tir_public_fp_envelope_v1::PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1)?;
        if !tfp.starts_with(b"TFP1") {
            return None;
        }
        bytes_at(tfp, 14, FP_MAX)?
    } else {
        inner
    };
    if original.is_empty()
        || original.len() > FP_MAX
        || original.starts_with(PALW_FP_ANSWER_ENVELOPE_MAGIC_V1)
        || original.starts_with(b"TFP1")
        || original.starts_with(b"HFP1")
        || original.starts_with(b"TFO1")
    {
        return None;
    }
    Some(original)
}

pub fn palw_fp_accounting_original_decode_v1(inner: &[u8]) -> Result<NativeRetainedV1<PalwFpCommitmentTxPayloadV3>, Fault> {
    let original = palw_fp_accounting_original_bytes_v1(inner).ok_or_else(|| invalid("accounting inner wrapper order"))?;
    let shape = preflight_native_value_v1::<PalwFpCommitmentTxPayloadV3>(original, original.len() as u64)
        .map_err(|_| invalid("accounting original typed wire shape"))?;
    let lease = reserve_native_requests_v1(shape.logical_requests_v1()).map_err(NativeDecodeErrorV1::Resource)?;
    let payload = borsh::from_slice(original).map_err(|_| invalid("accounting original bounded codec"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(payload, lease))
}

/// Native execution context + actually decoded answer, before a producer signs or a seat licenses.
/// A root is only an identity binding; it does not establish correctness of model computation.
pub fn palw_fp_answer_check_context_v1(
    c: &PalwFreePromptCommitmentV3,
    ctx: &PalwJobContextV2,
    ids: &[u32],
    root: Option<Hash64>,
) -> Result<(), &'static str> {
    if ids.len() != c.decode_tokens_executed as usize || ctx.exact_decode_tokens != c.decode_tokens_executed {
        return Err("accounting answer/executed count mismatch");
    }
    if crate::palw_fp_execution_v3::palw_fp_job_pin_v1(c) != crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(ctx) {
        return Err("accounting execution context differs from claim");
    }
    if root != Some(c.output_root) {
        return Err("accounting answer does not reproduce committed output root");
    }
    Ok(())
}

/// Reserve then serialize slices directly: no temporary inner/answer Vec clones.
pub fn palw_fp_answer_envelope_retain_encode_v1(inner: &[u8], ids: &[u32]) -> Result<NativeRetainedV1<Vec<u8>>, Fault> {
    if inner.is_empty() || inner.starts_with(PALW_FP_ANSWER_ENVELOPE_MAGIC_V1) {
        return Err(invalid("nested or empty accounting envelope"));
    }
    let cap = if inner.starts_with(b"TFP1") {
        crate::palw_tir_public_fp_envelope_v1::PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1
    } else if inner.starts_with(b"HFP1") {
        crate::palw_history_custody_v1::PALW_HISTORY_FP_ENVELOPE_MAX_BYTES_V1
    } else if inner.starts_with(b"TFO1") {
        crate::palw_tir_funded_original_envelope_v1::PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1
    } else {
        crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES
    };
    if inner.len() > cap {
        return Err(invalid("accounting inner exceeds existing schema cap"));
    }
    let inner_len = u32::try_from(inner.len()).map_err(|_| invalid("accounting inner length overflow"))?;
    let answer_len = u32::try_from(ids.len()).map_err(|_| invalid("accounting answer length overflow"))?;
    let bytes = inner
        .len()
        .checked_add(ids.len().checked_mul(4).ok_or_else(|| invalid("accounting answer byte overflow"))?)
        .and_then(|n| n.checked_add(12))
        .ok_or_else(|| invalid("accounting envelope length overflow"))?;
    let quote =
        bytes.checked_add(std::mem::size_of::<NativeRetainedV1<Vec<u8>>>()).ok_or_else(|| invalid("accounting owner overflow"))?;
    let lease = reserve_native_requests_v1(quote as u64).map_err(NativeDecodeErrorV1::Resource)?;
    let mut wire = Vec::with_capacity(bytes);
    wire.extend_from_slice(PALW_FP_ANSWER_ENVELOPE_MAGIC_V1);
    wire.extend_from_slice(&inner_len.to_le_bytes());
    wire.extend_from_slice(inner);
    wire.extend_from_slice(&answer_len.to_le_bytes());
    for id in ids {
        wire.extend_from_slice(&id.to_le_bytes());
    }
    if palw_fp_answer_envelope_view_v1(&wire, ids.len()).is_none() {
        return Err(invalid("accounting inner exceeds existing schema cap"));
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(wire, lease))
}

/// Bind the literal accounting transaction to the chain's existing seat duty and this seat's own backend.
/// This checks answer identity, never licenses correctness or replaces selected-verifier approval.
pub fn palw_fp_answer_bind_duty_v1(
    tx: &crate::tx::Transaction,
    duty: &crate::palw_producer_v2::PalwSeatDutyV2,
    backend: &dyn crate::palw_backend::PalwExecutionBackendV1,
) -> Result<crate::tx::TransactionId, Fault> {
    if !duty.free_prompt || tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT || !tx.id_is_current() {
        return Err(invalid("accounting carrier namespace or stale actual ID"));
    }
    let view = palw_fp_answer_envelope_view_v1(&tx.payload, u32::MAX as usize)
        .ok_or_else(|| invalid("missing accounting transaction envelope"))?;
    let original = palw_fp_accounting_original_decode_v1(view.inner)?;
    let c = &original.commitment;
    if crate::palw_freeprompt_v3::fp_claim_id_v3(c) != duty.claim_id
        || c.job.class_id != duty.class_id
        || c.job.executor_bond != duty.executor_bond.0
        || c.output_root != duty.output_root
        || c.execution_root != duty.execution_root
        || c.trace_root != duty.trace_root
        || duty.job_identity == Hash64::default()
        || crate::palw_fp_execution_v3::palw_fp_job_pin_v1(c) != duty.job_identity
    {
        return Err(invalid("accounting carrier does not name this accepted claim/context"));
    }
    if view.answer_len_v1() != c.decode_tokens_executed as usize {
        return Err(invalid("accounting carried answer count"));
    }
    let answer = super::palw_fp_answer_for_carrier_v1(tx, tx.id(), c.decode_tokens_executed)?;
    // The context owns only a bounded network-id Vec; all remaining fields are fixed-size.
    let _context_lease =
        reserve_native_requests_v1((std::mem::size_of::<PalwJobContextV2>() + crate::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES) as u64)
            .map_err(NativeDecodeErrorV1::Resource)?;
    let ctx = backend
        .fp_job_context_for_executed_v1(&c.job, c.decode_tokens_executed)
        .ok_or_else(|| invalid("accounting backend context unavailable"))?;
    palw_fp_answer_check_context_v1(c, &ctx, &answer, backend.output_root_for_context_v1(&ctx, &answer)).map_err(invalid)?;
    Ok(tx.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accounting_binding_codec_bounds_and_wrapper_order() {
        let plain = [1u8, 2, 3];
        let mut tfp = b"TFP1".to_vec();
        tfp.extend_from_slice(&[0; 10]);
        tfp.extend_from_slice(&(plain.len() as u32).to_le_bytes());
        tfp.extend_from_slice(&plain);
        let mut hfp = b"HFP1".to_vec();
        hfp.extend_from_slice(&[0; 2]);
        hfp.extend_from_slice(&(plain.len() as u32).to_le_bytes());
        hfp.extend_from_slice(&plain);
        let mut tfo = b"TFO1".to_vec();
        tfo.extend_from_slice(&[0; 2]);
        tfo.extend_from_slice(&(tfp.len() as u32).to_le_bytes());
        tfo.extend_from_slice(&tfp);
        for inner in [&plain[..], &tfp, &hfp, &tfo] {
            assert_eq!(palw_fp_accounting_original_bytes_v1(inner), Some(&plain[..]));
            let wire = palw_fp_answer_envelope_retain_encode_v1(inner, &[7, 9]).unwrap();
            let view = palw_fp_answer_envelope_view_v1(&wire, 2).unwrap();
            assert_eq!(view.inner, inner);
            assert!(view.answer_ids_v1().eq([7, 9]));
            assert!(matches!(palw_fp_accounting_original_decode_v1(inner), Err(NativeDecodeErrorV1::Decode(_))));
            assert!(palw_fp_answer_envelope_retain_encode_v1(&wire, &[7]).is_err());
        }
        assert!(palw_fp_accounting_original_bytes_v1(b"TFO1\0\0\x01\0\0\0x").is_none());
        assert!(
            palw_fp_answer_envelope_retain_encode_v1(&vec![0; crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES + 1], &[])
                .is_err()
        );
    }
}
