//! Offline RFC-0051 carrier preparation for an already pinned tokenizer.

use kaspa_consensus_core::{
    mldsa87_primitives::{MLDSA87_PUBKEY_LEN, MLDSA87_SIGNATURE_LEN},
    palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1,
    palw_tokenizer_registration_carriage_v1::{TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1, encode_tokenizer_registration_payload_v1},
    palw_tokenizer_registration_v1::{
        TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1, TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1, TOKENIZER_REGISTRATION_PART_CONTEXT_V1,
        TokenizerRegistrationFinishBodyV1, TokenizerRegistrationFinishV1, TokenizerRegistrationOpenBodyV1,
        TokenizerRegistrationOpenV1, TokenizerRegistrationPartBodyV1, TokenizerRegistrationPartV1, TokenizerRegistrationPolicyV1,
        TokenizerRegistrationQuoteV1, TokenizerRegistrationWireV1,
    },
    palw_tokenizer_rendering_v2::{PalwTokenizerRenderingBudgetV2, derive_tokenizer_rendering_v2},
};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::MessageSigner;

/// Inputs for local planning. The caller supplies the pinned Open and expected
/// opener key from its own context. This builder does not authenticate chain state.
pub struct TokenizerRegistrationPlanInputV1<'a> {
    pub open: TokenizerRegistrationOpenBodyV1,
    pub policy: &'a TokenizerRegistrationPolicyV1,
    pub raw: &'a [u8],
    pub eog: PalwTokenTableEogOpeningV1,
    pub opener_public_key: &'a [u8],
    pub proposed_accepting_daa: u64,
    pub declared_key_events: u64,
}

/// Signed payloads and a local fee quote at the proposed accepting DAA.
/// Actual slots and collectible fees depend on the chain's accepted Open.
#[derive(Debug)]
pub struct PreparedTokenizerRegistrationV1 {
    pub open_id: Hash64,
    pub proposed_accepting_daa: u64,
    pub local_quote: TokenizerRegistrationQuoteV1,
    pub open_payload: Vec<u8>,
    pub indexed_part_payloads: Vec<Vec<u8>>,
    pub finish_payload: Vec<u8>,
}

/// Validate the original EOF bytes against the signed statement, then produce
/// purpose-signed TKR1 payloads. Keep Parts and Finish for their accepted slots.
pub fn prepare_tokenizer_registration_v1(
    input: TokenizerRegistrationPlanInputV1<'_>,
    signer: &dyn MessageSigner,
) -> Result<PreparedTokenizerRegistrationV1, String> {
    let TokenizerRegistrationPlanInputV1 { open, policy, raw, eog, opener_public_key, proposed_accepting_daa, declared_key_events } =
        input;
    if opener_public_key.len() != MLDSA87_PUBKEY_LEN || signer.public_key() != opener_public_key {
        return Err("tokenizer registration signer differs from the expected opener key".into());
    }
    if policy.carrier_overhead_bytes < TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1.len() as u64 {
        return Err("tokenizer registration policy omits TKR1 framing bytes".into());
    }
    let local_quote = policy.quote_v1(&open, proposed_accepting_daa, declared_key_events)?;
    let open_id = open.id_v1()?;
    let mut finish = TokenizerRegistrationFinishV1 {
        body: TokenizerRegistrationFinishBodyV1 {
            version: open.version,
            network: open.network,
            policy: open.policy,
            open_id,
            opener: open.opener,
            eog_digest: open.eog_digest,
            finish_until_daa: open.finish_until_daa,
        },
        signature: [0; MLDSA87_SIGNATURE_LEN],
        eog,
    };
    finish.check_open_v1(&open, policy)?;
    derive_tokenizer_rendering_v2(
        open.statement,
        raw,
        &finish.eog,
        PalwTokenizerRenderingBudgetV2 {
            raw_bytes: policy.max_raw_bytes,
            vocab: policy.max_vocab,
            key_events: declared_key_events,
            eog_ids: policy.max_eog_ids,
            total_rendering_bytes: policy.max_rendering_bytes,
        },
    )?;

    let mut signed_open = TokenizerRegistrationOpenV1 { body: open, signature: [0; MLDSA87_SIGNATURE_LEN] };
    signed_open.signature = sign(signer, signed_open.signing_message_v1()?, TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1)?;
    signed_open.verify_signature_v1(opener_public_key)?;
    let open_payload = encode_tokenizer_registration_payload_v1(&TokenizerRegistrationWireV1::Open(signed_open))?;
    let mut indexed_part_payloads = Vec::with_capacity(local_quote.geometry.part_count as usize);
    for index in 0..local_quote.geometry.part_count {
        let (offset, len) = local_quote.geometry.part_v1(index)?;
        let bytes = raw[offset as usize..offset as usize + len].to_vec();
        let mut part = TokenizerRegistrationPartV1 {
            body: TokenizerRegistrationPartBodyV1 {
                version: open.version,
                network: open.network,
                policy: open.policy,
                open_id,
                opener: open.opener,
                index,
                offset,
                payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(&bytes)?,
            },
            signature: [0; MLDSA87_SIGNATURE_LEN],
            bytes,
        };
        part.signature = sign(signer, part.signing_message_v1()?, TOKENIZER_REGISTRATION_PART_CONTEXT_V1)?;
        part.verify_signature_v1(opener_public_key)?;
        indexed_part_payloads.push(encode_tokenizer_registration_payload_v1(&TokenizerRegistrationWireV1::Part(part))?);
    }
    finish.signature = sign(signer, finish.signing_message_v1()?, TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1)?;
    finish.verify_signature_v1(opener_public_key)?;
    let finish_payload = encode_tokenizer_registration_payload_v1(&TokenizerRegistrationWireV1::Finish(finish))?;

    Ok(PreparedTokenizerRegistrationV1 {
        open_id,
        proposed_accepting_daa,
        local_quote,
        open_payload,
        indexed_part_payloads,
        finish_payload,
    })
}

fn sign(signer: &dyn MessageSigner, message: Hash64, context: &[u8]) -> Result<[u8; MLDSA87_SIGNATURE_LEN], String> {
    signer
        .sign_message(message.as_byte_slice(), context)?
        .try_into()
        .map_err(|_| "tokenizer registration signer returned an invalid signature length".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_court_service_v1::ServiceCost,
        palw_fp_constraint_job_v1::PalwTokenTableV1,
        palw_state_v2::PalwBondKeyV2,
        palw_tokenizer_registration_carriage_v1::decode_tokenizer_registration_payload_v1,
        palw_tokenizer_registration_v1::TOKENIZER_REGISTRATION_PART_BYTES_V1,
        palw_tokenizer_rendering_v1::{PalwTokenizerRenderingRuleV1, palw_tokenizer_raw_commitment_v1},
        palw_tokenizer_rendering_v2::{PalwTfp1TokenizerIntakePinsV2, PalwTokenizerRenderingStatementV2},
        tx::TransactionOutpoint,
    };
    use kaspa_pq_validator_core::ValidatorKey;

    #[test]
    fn builds_signed_indexed_payloads_and_refuses_changed_inputs_before_signing() {
        let h = Hash64::from_u64_word;
        let mut raw = br#"{"added_tokens":[{"id":2,"content":"long"}],"model":{"vocab":{"A":0,"b":2},"merges":[]}}"#.to_vec();
        raw.resize(TOKENIZER_REGISTRATION_PART_BYTES_V1 + 1, b' ');
        let table = PalwTokenTableV1 {
            entries: vec![Some(b"A".to_vec()), Some(vec![]), Some(b"long".to_vec()), None],
            eog_token_ids: vec![2],
            tokenizer_id: palw_tokenizer_raw_commitment_v1(&raw),
        };
        let eog = table.eog_opening().unwrap();
        let policy = TokenizerRegistrationPolicyV1 {
            version: 1,
            max_raw_bytes: raw.len() as u64,
            max_vocab: 8,
            max_key_events: 32,
            max_eog_ids: 2,
            max_rendering_bytes: 16,
            max_parser_scratch_bytes: 10_000_000,
            max_carrier_bytes: 80_000,
            max_retained_bytes: 1_000_000,
            max_retained_total_bytes: 2_000_000,
            service_budget: ServiceCost { bytes: 100_000, work: 10_000_000, io: 1_000_000, controls: 1 },
            max_work: 10_000_000,
            max_pending_per_opener: 1,
            max_pending_total: 2,
            max_session_daa: 100,
            retention_daa: 20,
            carrier_overhead_bytes: 4,
            retention_copies: 2,
            metadata_bytes: 64_000,
            work_rates: [1; 7],
            bookkeeping_work_per_step: 1,
            rent_per_carrier_byte: 1,
            rent_per_retained_byte_daa: 1,
            rent_per_work: 1,
            max_registration_charge: 100_000_000,
        };
        let open = TokenizerRegistrationOpenBodyV1 {
            version: 1,
            network: h(1),
            policy: policy.id_v1().unwrap(),
            pins: PalwTfp1TokenizerIntakePinsV2 { class_id: h(2), container_root: h(3), semantics_id: h(4), inventory_id: h(5) },
            artifact_root: h(6),
            statement: PalwTokenizerRenderingStatementV2 {
                tokenizer_id: table.tokenizer_id,
                exact_raw_bytes: raw.len() as u64,
                table_root: table.root(),
                vocab: 4,
                rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
            },
            eog_digest: TokenizerRegistrationFinishV1::eog_digest_v1(&eog).unwrap(),
            opener: PalwBondKeyV2(TransactionOutpoint::new(h(7), 0)),
            nonce: h(8),
            accept_until_daa: 40,
            finish_until_daa: 80,
            material_until_daa: 100,
            max_registration_charge: 100_000_000,
        };
        let key = ValidatorKey::from_seed([42; 32]);
        let input = || TokenizerRegistrationPlanInputV1 {
            open,
            policy: &policy,
            raw: &raw,
            eog: eog.clone(),
            opener_public_key: key.public_key(),
            proposed_accepting_daa: 20,
            declared_key_events: 32,
        };
        let prepared = prepare_tokenizer_registration_v1(input(), &key).unwrap();
        let decode = |payload: &[u8]| decode_tokenizer_registration_payload_v1(payload, &policy).unwrap().unwrap();
        let TokenizerRegistrationWireV1::Open(signed_open) = decode(&prepared.open_payload) else { panic!("Open payload") };
        assert_eq!(signed_open.body, open);
        assert_eq!(prepared.open_id, open.id_v1().unwrap());
        assert_eq!(prepared.local_quote, policy.quote_v1(&open, 20, 32).unwrap());
        signed_open.verify_signature_v1(key.public_key()).unwrap();
        let mut rebuilt = Vec::new();
        for (index, payload) in prepared.indexed_part_payloads.iter().enumerate() {
            let TokenizerRegistrationWireV1::Part(part) = decode(payload) else { panic!("Part payload") };
            assert_eq!(part.body.index as usize, index);
            part.check_open_v1(&open).unwrap();
            part.verify_signature_v1(key.public_key()).unwrap();
            rebuilt.extend_from_slice(&part.bytes);
        }
        assert_eq!(prepared.indexed_part_payloads.len(), 2);
        assert_eq!(rebuilt, raw);
        let TokenizerRegistrationWireV1::Finish(finish) = decode(&prepared.finish_payload) else { panic!("Finish payload") };
        finish.check_open_v1(&open, &policy).unwrap();
        finish.verify_signature_v1(key.public_key()).unwrap();

        struct NoSign<'a>(&'a [u8]);
        impl MessageSigner for NoSign<'_> {
            fn public_key(&self) -> &[u8] {
                self.0
            }
            fn sign_message(&self, _: &[u8], _: &[u8]) -> Result<Vec<u8>, String> {
                panic!("invalid inputs must fail before signing");
            }
        }
        let no_sign = NoSign(key.public_key());
        let mut changed = raw.clone();
        *changed.last_mut().unwrap() = b'\n';
        assert!(prepare_tokenizer_registration_v1(TokenizerRegistrationPlanInputV1 { raw: &changed, ..input() }, &no_sign).is_err());
        assert!(
            prepare_tokenizer_registration_v1(TokenizerRegistrationPlanInputV1 { declared_key_events: 1, ..input() }, &no_sign)
                .is_err()
        );
        let mut bad_eog = eog.clone();
        bad_eog.ids = vec![0];
        assert!(prepare_tokenizer_registration_v1(TokenizerRegistrationPlanInputV1 { eog: bad_eog, ..input() }, &no_sign).is_err());
        let capped = TokenizerRegistrationOpenBodyV1 { max_registration_charge: prepared.local_quote.charge - 1, ..open };
        assert!(prepare_tokenizer_registration_v1(TokenizerRegistrationPlanInputV1 { open: capped, ..input() }, &no_sign).is_err());
    }
}
