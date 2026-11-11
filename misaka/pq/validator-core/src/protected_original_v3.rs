//! Build the actual TFO3 v3 transaction around an already signed original.
//! Neither the immutable FP bytes nor the companion/funding consent is signed
//! again here. The caller obtains those from the protected quote/signing path.

use super::*;
use kaspa_consensus_core::{
    mldsa87_primitives::p2pkh_mldsa87_spk, palw_tir_funded_original_envelope_v1::decode_funded_original_carrier_v3,
    palw_tir_funded_original_envelope_v3::PalwTirFundedOriginalEnvelopeV3,
};

/// Preserve the complete protected payload and fund its dedicated own-output.
/// `fee` is the ordinary transaction input/output difference. The amount in
/// the signed funding consent is a separate output, never subtracted as fee
/// or supplemented with a fabricated legacy TFP1 custody-rent declaration.
/// This builder supports one payer input and funding output index 0 or 1;
/// consensus can support other transaction layouts independently.
pub fn build_protected_original_carrier_v3_with(
    signer: &dyn MessageSigner,
    payload: Vec<u8>,
    funding_outpoint: TransactionOutpoint,
    funding: &UtxoEntry,
    fee: u64,
) -> Result<Transaction, String> {
    let public = PalwTirFundedOriginalEnvelopeV3::decode_v3(&payload)?;
    if !public.is_protected_v3() {
        return Err("protected carrier builder requires TFO3 wire version 3".into());
    }
    let envelope = decode_funded_original_carrier_v3(&payload)?;
    let terms = &envelope.funding.terms;
    let payer_script = p2pkh_mldsa87_spk(&blake2b_512_address_payload(signer.public_key()).as_bytes());
    if envelope.payer_pubkey != signer.public_key() || funding.script_public_key != payer_script {
        return Err("protected carrier input must belong to the consent's exact payer".into());
    }
    envelope.funding.verify_signature_v1(signer.public_key()).map_err(|error| format!("funding consent: {error}"))?;
    let remaining = funding.amount.checked_sub(fee).ok_or("protected carrier fee exceeds its input")?;
    let change = remaining.checked_sub(terms.funding_amount_sompi).ok_or("protected carrier cannot fund consent and fee")?;
    let dedicated = TransactionOutput::new(terms.funding_amount_sompi, payer_script.clone());
    let outputs = match (terms.funding_output_index, change) {
        (0, 0) => vec![dedicated],
        (0, change) => vec![dedicated, TransactionOutput::new(change, payer_script)],
        (1, change) if change > 0 => vec![TransactionOutput::new(change, payer_script), dedicated],
        _ => return Err("protected carrier funding index requires an existing output at index 0 or 1".into()),
    };
    let tx = Transaction::new(
        TX_VERSION,
        vec![TransactionInput::new(funding_outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 1)],
        outputs,
        0,
        SUBNETWORK_ID_PALW_FP_COMMITMENT,
        0,
        payload,
    );
    envelope.inspect_carrier_v1(&tx)?;
    let mtx = MutableTransaction::with_entries(tx, vec![funding.clone()]);
    let reused = Mldsa87SigHashReusedValuesUnsync::new();
    let sighash = calc_mldsa87_signature_hash(&mtx.as_verifiable(), 0, SIG_HASH_ALL, &reused);
    let mut signature = signer
        .sign_message(sighash.as_bytes().as_slice(), MLDSA87_TX_CONTEXT)
        .map_err(|error| format!("protected carrier input signature: {error}"))?;
    if signature.len() != MLDSA87_SIG_LEN {
        return Err("protected carrier input signature has the wrong algorithm width".into());
    }
    signature.push(SIG_HASH_ALL.to_u8());
    let script = ScriptBuilder::new()
        .add_data(&signature)
        .map_err(|error| format!("protected carrier signature push: {error}"))?
        .add_data(signer.public_key())
        .map_err(|error| format!("protected carrier key push: {error}"))?
        .drain();
    let mut tx = mtx.tx;
    tx.inputs[0].signature_script = script;
    envelope.inspect_carrier_v1(&tx)?;
    Ok(tx)
}
