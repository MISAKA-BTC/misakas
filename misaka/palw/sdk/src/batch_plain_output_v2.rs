//! Offline BCT2 plain Output controls. A signed request asks the node to
//! inspect accepted FullIds; it never asserts a verdict or creates a receipt.

use kaspa_consensus_core::{
    Hash64,
    mldsa87_primitives::{MLDSA87_PUBKEY_LEN, MLDSA87_SIGNATURE_LEN},
    palw_batch_court_v1::carriage::{
        BatchPlainOutputTerminalV2, BatchPlainOutputWireV2, PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2, SignedBatchPlainOutputV2,
        encode_batch_plain_output_payload_v2,
    },
    palw_state_v2::PalwBondKeyV2,
};
use kaspa_pq_validator_core::MessageSigner;

/// Identities read by the caller from the intended network and accepted
/// original/material. This offline helper does not authenticate that read,
/// establish a service reservation, or grant the filer native authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchPlainOutputRequestV2 {
    pub network: Hash64,
    pub claim: Hash64,
    pub scope: Hash64,
    pub source: Hash64,
    pub notice: Hash64,
    pub policy: Hash64,
    pub material: Hash64,
    pub filer: PalwBondKeyV2,
}

impl BatchPlainOutputRequestV2 {
    fn unsigned(self) -> SignedBatchPlainOutputV2 {
        SignedBatchPlainOutputV2 {
            version: 2,
            claim: self.claim,
            scope: self.scope,
            source: self.source,
            notice: self.notice,
            policy: self.policy,
            material: self.material,
            filer: self.filer,
            signature: [0; MLDSA87_SIGNATURE_LEN],
        }
    }

    /// Recompute the native digest before requesting any signature.
    pub fn signing_message_v2(self) -> Result<Hash64, String> {
        self.unsigned().signing_message_v2(self.network)
    }
}

/// Sign the exact BCT2 request and verify the signer's response before
/// returning a payload. The caller must obtain the expected key from its
/// authenticated bond view. Funding, submission and native acceptance are
/// separate steps, including the current profile and booked service slot.
pub fn prepare_batch_plain_output_request_v2(
    request: BatchPlainOutputRequestV2,
    expected_filer_public_key: &[u8],
    signer: &dyn MessageSigner,
) -> Result<Vec<u8>, String> {
    if expected_filer_public_key.len() != MLDSA87_PUBKEY_LEN || signer.public_key() != expected_filer_public_key {
        return Err("plain Batch Output signer differs from the expected filer key".into());
    }
    let mut signed = request.unsigned();
    let message = signed.signing_message_v2(request.network)?;
    signed.signature = signer
        .sign_message(message.as_byte_slice(), PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2)?
        .try_into()
        .map_err(|_| "plain Batch Output signer returned an invalid signature length")?;
    signed.verify_signature_v2(request.network, expected_filer_public_key)?;
    encode_batch_plain_output_payload_v2(&BatchPlainOutputWireV2::Verify(signed)).map_err(|error| error.to_string())
}

/// Relay a reference to a native, retained receipt. Terminal has no inner
/// signature: the node rechecks its own rooted receipt, current context and
/// assigned terminal slot. These two identifiers are not evidence of a
/// ruling, and this helper cannot construct or replace that receipt.
pub fn prepare_batch_plain_output_terminal_v2(claim: Hash64, receipt_digest: Hash64) -> Result<Vec<u8>, String> {
    encode_batch_plain_output_payload_v2(&BatchPlainOutputWireV2::Terminal(BatchPlainOutputTerminalV2 { claim, receipt_digest }))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{palw_batch_court_v1::carriage::decode_batch_plain_output_payload_v2, tx::TransactionOutpoint};
    use kaspa_pq_validator_core::ValidatorKey;

    fn request() -> BatchPlainOutputRequestV2 {
        let h = Hash64::from_u64_word;
        BatchPlainOutputRequestV2 {
            network: h(1),
            claim: h(2),
            scope: h(3),
            source: h(4),
            notice: h(5),
            policy: h(6),
            material: h(7),
            filer: PalwBondKeyV2(TransactionOutpoint::new(h(8), 9)),
        }
    }

    #[test]
    fn batch_plain_output_sdk_signature_binds_every_identity_and_purpose() {
        let key = ValidatorKey::from_seed([43; 32]);
        let r = request();
        let payload = prepare_batch_plain_output_request_v2(r, key.public_key(), &key).unwrap();
        assert!(payload.starts_with(b"BCT2"));
        let Some(BatchPlainOutputWireV2::Verify(signed)) = decode_batch_plain_output_payload_v2(&payload).unwrap() else {
            panic!("expected BCT2 Verify");
        };
        signed.verify_signature_v2(r.network, key.public_key()).unwrap();
        assert!(signed.verify_signature_v2(Hash64::from_u64_word(99), key.public_key()).is_err());
        for field in 0..9 {
            let mut changed = signed.clone();
            match field {
                0 => changed.version = 1,
                1 => changed.claim = Hash64::from_u64_word(99),
                2 => changed.scope = Hash64::from_u64_word(99),
                3 => changed.source = Hash64::from_u64_word(99),
                4 => changed.notice = Hash64::from_u64_word(99),
                5 => changed.policy = Hash64::from_u64_word(99),
                6 => changed.material = Hash64::from_u64_word(99),
                7 => changed.filer.0.transaction_id = Hash64::from_u64_word(99),
                8 => changed.filer.0.index += 1,
                _ => unreachable!(),
            }
            assert!(changed.verify_signature_v2(r.network, key.public_key()).is_err(), "unbound field {field}");
        }
        let mut wrong_purpose = signed.clone();
        wrong_purpose.signature = key
            .sign_message(signed.signing_message_v2(r.network).unwrap().as_byte_slice(), b"misaka-palw/batch-demand/v1/mldsa87")
            .unwrap()
            .try_into()
            .unwrap();
        assert!(wrong_purpose.verify_signature_v2(r.network, key.public_key()).is_err());
        // Framing is version-specific even though the native fields are fixed width.
        let mut old_prefix = payload;
        old_prefix[..4].copy_from_slice(b"BCT1");
        assert!(decode_batch_plain_output_payload_v2(&old_prefix).unwrap().is_none());
    }

    #[test]
    fn batch_plain_output_sdk_rejects_invalid_inputs_and_bad_signer_responses() {
        let key = ValidatorKey::from_seed([44; 32]);
        struct NoSign<'a>(&'a [u8]);
        impl MessageSigner for NoSign<'_> {
            fn public_key(&self) -> &[u8] {
                self.0
            }
            fn sign_message(&self, _: &[u8], _: &[u8]) -> Result<Vec<u8>, String> {
                panic!("invalid inputs must fail before signing")
            }
        }
        let no_sign = NoSign(key.public_key());
        let mut invalid = request();
        invalid.network = Hash64::default();
        assert!(prepare_batch_plain_output_request_v2(invalid, key.public_key(), &no_sign).is_err());
        assert!(prepare_batch_plain_output_request_v2(request(), &[], &no_sign).is_err());
        let other = ValidatorKey::from_seed([45; 32]);
        assert!(prepare_batch_plain_output_request_v2(request(), other.public_key(), &no_sign).is_err());

        struct BadSigner<'a> {
            key: &'a [u8],
            length: usize,
        }
        impl MessageSigner for BadSigner<'_> {
            fn public_key(&self) -> &[u8] {
                self.key
            }
            fn sign_message(&self, _: &[u8], _: &[u8]) -> Result<Vec<u8>, String> {
                Ok(vec![0; self.length])
            }
        }
        for length in [0, MLDSA87_SIGNATURE_LEN - 1, MLDSA87_SIGNATURE_LEN, MLDSA87_SIGNATURE_LEN + 1] {
            let signer = BadSigner { key: key.public_key(), length };
            assert!(prepare_batch_plain_output_request_v2(request(), key.public_key(), &signer).is_err());
        }
    }

    #[test]
    fn batch_plain_output_sdk_terminal_only_encodes_a_receipt_reference() {
        let r = request();
        let digest = Hash64::from_u64_word(10);
        let payload = prepare_batch_plain_output_terminal_v2(r.claim, digest).unwrap();
        assert_eq!(payload.len(), 4 + 1 + 2 * 64);
        assert_eq!(
            decode_batch_plain_output_payload_v2(&payload).unwrap(),
            Some(BatchPlainOutputWireV2::Terminal(BatchPlainOutputTerminalV2 { claim: r.claim, receipt_digest: digest }))
        );
        assert!(prepare_batch_plain_output_terminal_v2(Hash64::default(), digest).is_err());
        assert!(prepare_batch_plain_output_terminal_v2(r.claim, Hash64::default()).is_err());
    }
}
