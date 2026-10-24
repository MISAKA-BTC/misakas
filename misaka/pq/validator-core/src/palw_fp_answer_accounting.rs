//! Explicit transaction accounting carriage, after the native fence and before funding signatures.
use super::*;
use kaspa_consensus_core::palw_fp_answer_accounting_v1::{
    palw_fp_accounting_original_decode_v1, palw_fp_answer_check_context_v1, palw_fp_answer_envelope_retain_encode_v1,
};
use kaspa_consensus_core::{
    config::params::Params,
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_v2::PalwJobContextV2,
};
type Fault = NativeDecodeErrorV1<&'static str>;
fn invalid(s: &'static str) -> Fault {
    NativeDecodeErrorV1::Decode(s)
}

/// Only the additional accounting wire/signing workspace. Existing transaction allocations keep their original owners.
/// The submitter must hold this guard until its actual signed TX has crossed the native submission door.
#[derive(Default)]
pub struct FpAnswerTransactionLeaseV1 {
    _wire: Option<NativeDemandLeaseV1>,
    _signing: Option<NativeDemandLeaseV1>,
}

/// Keep the original literal payload (plain/TFP1/HFP1/TFO1), all inputs, outputs, fee and consent terms.
/// After wrapping, re-sign every native funding input over the actual outer FPA1 bytes.
/// `root` is the producer backend's result under `context`; a seat independently recomputes its own binding.
#[allow(clippy::too_many_arguments)]
pub fn wrap_fp_answer_accounting_tx_with_v1(
    signer: &dyn MessageSigner,
    params: &Params,
    daa: u64,
    tx: Transaction,
    entries: &[UtxoEntry],
    expected: &PalwFreePromptCommitmentV3,
    context: &PalwJobContextV2,
    ids: &[u32],
    root: Option<Hash64>,
) -> Result<(Transaction, FpAnswerTransactionLeaseV1), Fault> {
    if !params.palw_fp_answer_accounting_v1_fence().is_some_and(|f| f.is_active(daa)) {
        return Ok((tx, FpAnswerTransactionLeaseV1::default()));
    }
    wrap_fp_answer_accounting_tx_explicit_with_v1(signer, tx, entries, expected, context, ids, root)
}

/// Explicit carriage only: the caller must obtain its native quote and bind its own executed
/// context before calling. This function enables no consensus fence and supplies no policy defaults.
#[allow(clippy::too_many_arguments)]
pub fn wrap_fp_answer_accounting_tx_explicit_with_v1(
    signer: &dyn MessageSigner,
    mut tx: Transaction,
    entries: &[UtxoEntry],
    expected: &PalwFreePromptCommitmentV3,
    context: &PalwJobContextV2,
    ids: &[u32],
    root: Option<Hash64>,
) -> Result<(Transaction, FpAnswerTransactionLeaseV1), Fault> {
    if tx.subnetwork_id != SUBNETWORK_ID_PALW_FP_COMMITMENT
        || tx.inputs.is_empty()
        || tx.inputs.len() != entries.len()
        || signer.public_key().len() != kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN
    {
        return Err(invalid("accounting funding input/signer shape"));
    }
    palw_fp_answer_check_context_v1(expected, context, ids, root).map_err(invalid)?;
    let decoded = palw_fp_accounting_original_decode_v1(&tx.payload)?;
    if decoded.commitment != *expected {
        return Err(invalid("accounting carrier original differs from executed claim"));
    }
    let wire = palw_fp_answer_envelope_retain_encode_v1(&tx.payload, ids)?;
    let public = signer.public_key();
    // Signature Vec's one-byte sighash append and ScriptBuilder's two pushes can each grow geometrically.
    // Three times each actual typed byte array covers old + new buffers during growth, not only final len.
    let sig = kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
    let per_input = std::mem::size_of::<UtxoEntry>()
        .checked_add(sig.checked_mul(3).ok_or_else(|| invalid("accounting signing quote overflow"))?)
        .and_then(|n| n.checked_add((sig + 1 + public.len() + 8) * 3))
        .ok_or_else(|| invalid("accounting signing quote overflow"))?;
    let mut quote = per_input.checked_mul(entries.len()).ok_or_else(|| invalid("accounting signing quote overflow"))?;
    for entry in entries {
        quote = quote.checked_add(entry.script_public_key.script().len()).ok_or_else(|| invalid("accounting entry quote overflow"))?;
    }
    let signing = reserve_native_requests_v1(quote as u64).map_err(NativeDecodeErrorV1::Resource)?;
    let (payload, wire_lease) = wire.into_parts_v1();
    tx.payload = payload;
    tx.finalize();
    let mut mtx = MutableTransaction::with_entries(tx, entries.to_vec());
    for index in 0..mtx.tx.inputs.len() {
        let hash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), index, SIG_HASH_ALL, &Mldsa87SigHashReusedValuesUnsync::new());
        let mut signature = signer
            .sign_message(hash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT)
            .map_err(|_| invalid("accounting outer funding signer refused"))?;
        if signature.len() != kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err(invalid("accounting funding signature width"));
        }
        signature.push(SIG_HASH_ALL.to_u8());
        mtx.tx.inputs[index].signature_script = ScriptBuilder::new()
            .add_data(&signature)
            .and_then(|b| b.add_data(public))
            .map_err(|_| invalid("accounting funding script encoding"))?
            .drain();
    }
    mtx.tx.finalize();
    Ok((mtx.tx, FpAnswerTransactionLeaseV1 { _wire: wire_lease, _signing: signing }))
}
