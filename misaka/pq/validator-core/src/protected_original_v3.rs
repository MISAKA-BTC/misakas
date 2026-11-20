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
        return Err("protected carrier builder requires a protected TFO3 wire version".into());
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

/// Finish one quoted original without changing the literal FP signature/claim.
/// The payer explicitly supplies the UTXO, fee and nonce; the quote only sets
/// the dedicated funding amount and acceptance clock, rechecked by consensus.
#[allow(clippy::too_many_arguments)]
pub fn build_quoted_protected_original_carrier_v1_with(
    executor: &dyn MessageSigner,
    payer: &dyn MessageSigner,
    original_payload: Vec<u8>,
    header: kaspa_consensus_core::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    material: &kaspa_consensus_core::palw_protected_original_quote_v1::ProtectedOriginalQuoteMaterialV1,
    quote: &kaspa_consensus_core::palw_protected_original_quote_v1::ProtectedOriginalQuoteV1,
    nonce: Hash64,
    funding_outpoint: TransactionOutpoint,
    funding: &UtxoEntry,
    fee: u64,
) -> Result<Transaction, String> {
    use kaspa_consensus_core::{
        palw_court_original_escrow_v1::{COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1, FundingConsentV1, FundingTermsV1},
        palw_protected_original_quote_v1::PROTECTED_ORIGINAL_SIGN_CONTEXT_V1,
        palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1,
        palw_tir_protected_fp_envelope_v2::PalwTirProtectedFpEnvelopeV2,
    };
    let original = PalwTirProtectedFpEnvelopeV2::new_v1(original_payload, header)?;
    let fp = original.original_fp_v1()?;
    fp.validate_signature_v3(kaspa_consensus_core::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
        .map_err(|e| format!("literal FP signature: {e}"))?;
    quote.check_signing_material_v1(&fp.commitment, original.header_v1(), material, executor.public_key())?;
    if fee < quote.original_fee_sompi || funding.amount.checked_sub(fee).is_none_or(|left| left < quote.funding.required_amount_sompi)
    {
        return Err("protected original input cannot cover its quoted fee and dedicated funding".into());
    }
    let scope = &quote.funding;
    let (accept_from_daa, accept_until_daa, material_until_daa) = quote.funding_window_v2(&fp.commitment)?;
    let terms = FundingTermsV1 {
        version: 1,
        network_domain: fp.commitment.job.network_domain,
        claim: scope.claim,
        job_identity: scope.job_identity,
        execution_root: scope.execution_root,
        source_id: scope.source_id,
        relative_graph_id: scope.relative_graph_id,
        fee_profile_id: scope.fee_profile_id,
        capital_profile_id: scope.capital_profile_id,
        retention_profile_id: scope.retention_profile_id,
        settlement_policy_id: scope.settlement_policy_id,
        accept_from_daa,
        accept_until_daa,
        material_until_daa,
        funding_output_index: 0,
        funding_amount_sompi: scope.required_amount_sompi,
        payer_key_id: kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(payer.public_key()),
        refund_payload: blake2b_512_address_payload(payer.public_key()),
        nonce,
    };
    terms.check_v1().map_err(|e| format!("quoted funding terms: {e}"))?;
    let signature = executor
        .sign_message(&quote.companion.id().0.0, PROTECTED_ORIGINAL_SIGN_CONTEXT_V1)
        .map_err(|e| format!("companion signer refused: {e}"))?;
    let companion = quote.signed_companion_bytes_v1(executor.public_key(), &signature)?;
    let consent = FundingConsentV1 {
        signature: payer
            .sign_message(terms.id_v1().as_byte_slice(), COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1)
            .map_err(|e| format!("funding signer refused: {e}"))?,
        terms,
    };
    consent.verify_signature_v1(payer.public_key()).map_err(|e| format!("funding consent: {e}"))?;
    let funded = PalwTirFundedOriginalEnvelopeV1::new_protected_v2(original, consent, payer.public_key().to_vec())?
        .encode_protected_inner_v2()?;
    let frame = if quote.version == 3 {
        PalwTirFundedOriginalEnvelopeV3::from_parts_segmented_v5
    } else if quote.version == 2 {
        PalwTirFundedOriginalEnvelopeV3::from_parts_relative_v4
    } else {
        PalwTirFundedOriginalEnvelopeV3::from_parts_protected_v3
    };
    let payload = frame(
        &funded,
        &material.prompt,
        companion.as_slice().try_into().map_err(|_| "companion width")?,
        quote.manifest.as_slice().try_into().map_err(|_| "manifest width")?,
        &material.hf_descriptor,
    )?
    .encode_v3()?;
    build_protected_original_carrier_v3_with(payer, payload, funding_outpoint, funding, fee)
}
