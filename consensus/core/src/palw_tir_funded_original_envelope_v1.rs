//! RFC-0035 A isolated funded-original transport. Literal TFP1 bytes and old
//! claim identities/signatures are preserved. Decode/output matching confers no
//! UTXO lock, payer authorization, paid fee, complete graph or accepted original.
use crate::mldsa87_primitives::{MLDSA87_PUBKEY_LEN, MLDSA87_SIGNATURE_LEN, mldsa87_key_id};
use crate::palw_court_original_escrow_v1::{ActualCarrierFundingV1, FundingConsentV1, inspect_transaction_v1};
use crate::palw_tir_public_fp_envelope_v1::{PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1, PalwTirPublicFpEnvelopeV1};
use crate::tx::Transaction;
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};

/// Independent framing, never a TFP1 version extension or old lifecycle tag.
pub const PALW_TIR_FUNDED_ORIGINAL_MAGIC_V1: &[u8; 4] = b"TFO1";
/// Keep the existing public-original transport ceiling. New metadata is not a
/// reason to raise legacy payload, signature, custody or transaction limits.
pub const PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1: usize = PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirFundedOriginalEnvelopeV1 {
    pub version: u16,
    /// Exact standalone old TFP1 encoding, including magic and strict EOF.
    pub original_envelope: Vec<u8>,
    pub funding: FundingConsentV1,
    /// Fixed algorithm width, checked before allocation. This transport key is
    /// not a current-authority or role-consent check by itself.
    pub payer_pubkey: Vec<u8>,
}
/// Classification only, deliberately independent of decode success or any
/// activation fence. A truncated TFO1 framing cannot fall back to legacy FP.
pub fn is_funded_original_carrier_v1(tx: &Transaction) -> bool {
    tx.subnetwork_id == crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT
        && (tx.payload.starts_with(PALW_TIR_FUNDED_ORIGINAL_MAGIC_V1)
            || (!tx.payload.is_empty() && PALW_TIR_FUNDED_ORIGINAL_MAGIC_V1.starts_with(&tx.payload)))
}
fn invalid(why: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.into())
}
impl BorshDeserialize for PalwTirFundedOriginalEnvelopeV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let version = u16::deserialize_reader(r)?;
        if version != 1 {
            return Err(invalid("unknown TFO1 version before allocation"));
        }
        let original_envelope = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1)?;
        let funding = FundingConsentV1::deserialize_reader(r)?;
        let payer_pubkey = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, MLDSA87_PUBKEY_LEN)?;
        let out = Self { version, original_envelope, funding, payer_pubkey };
        out.check_v1().map_err(invalid)?;
        Ok(out)
    }
}
impl PalwTirFundedOriginalEnvelopeV1 {
    pub fn new_v1(original_envelope: Vec<u8>, funding: FundingConsentV1, payer_pubkey: Vec<u8>) -> Result<Self, String> {
        let out = Self { version: 1, original_envelope, funding, payer_pubkey };
        out.check_v1()?;
        Ok(out)
    }
    /// Structural and transaction-local identity checks, never a substitute for
    /// native original quote, explicit signature verification or actual funds.
    pub fn check_v1(&self) -> Result<(), String> {
        self.funding.check_v1().map_err(|e| format!("{e:?}"))?;
        let t = &self.funding.terms;
        if self.version != 1
            || self.payer_pubkey.len() != MLDSA87_PUBKEY_LEN
            || self.funding.signature.len() != MLDSA87_SIGNATURE_LEN
            || t.version != 1
            || t.funding_amount_sompi == 0
            || t.accept_from_daa > t.accept_until_daa
            || mldsa87_key_id(&self.payer_pubkey) != t.payer_key_id
        {
            return Err("TFO1 version, key/signature shape, funding interval or payer identity".into());
        }
        if self.original_envelope.is_empty() || self.original_envelope.len() > PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1 {
            return Err("TFO1 original envelope exceeds existing transport bound".into());
        }
        let original = PalwTirPublicFpEnvelopeV1::decode_v1(&self.original_envelope)?;
        let fp = original.original_fp_v1()?;
        if (t.claim, t.job_identity, t.execution_root) != (fp.claim_id(), original.pledge.job_identity, fp.commitment.execution_root) {
            return Err("TFO1 funding substitutes its literal original claim/job/execution".into());
        }
        if borsh::object_length(self)
            .map_err(|e| e.to_string())?
            .checked_add(4)
            .is_none_or(|n| n > PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1)
        {
            return Err("TFO1 metadata exceeds existing outer transport ceiling".into());
        }
        Ok(())
    }
    pub fn encode_v1(&self) -> Result<Vec<u8>, String> {
        self.check_v1()?;
        let mut out = PALW_TIR_FUNDED_ORIGINAL_MAGIC_V1.to_vec();
        out.extend(borsh::to_vec(self).map_err(|e| e.to_string())?);
        Ok(out)
    }
    pub fn decode_v1(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1 {
            return Err("TFO1 exceeds total wire ceiling before allocation".into());
        }
        let body = bytes.strip_prefix(PALW_TIR_FUNDED_ORIGINAL_MAGIC_V1).ok_or("TFO1 magic absent")?;
        borsh::from_slice(body).map_err(|e| e.to_string())
    }
    /// This explicit method preserves the literal old companion; extractors
    /// must never emit its FP object without the funded coordinator succeeding.
    pub fn original_v1(&self) -> Result<PalwTirPublicFpEnvelopeV1, String> {
        self.check_v1()?;
        PalwTirPublicFpEnvelopeV1::decode_v1(&self.original_envelope)
    }
    /// Bundle facts and the once-only fee allocator from the SAME populated
    /// transaction. Entries still need the native composed UTXO view and normal
    /// script/acceptance validation. Funding output value is already an output,
    /// so it is never added to the available input/output fee difference.
    pub fn inspect_populated_carrier_v1<'a>(
        &self,
        tx: &crate::tx::PopulatedTransaction<'a>,
    ) -> Result<(ActualCarrierFundingV1<'a>, crate::palw_court_carrier_fee_v1::CarrierFeeBudgetV1), String> {
        let facts = self.inspect_carrier_v1(tx.tx)?;
        let fee = crate::palw_court_carrier_fee_v1::CarrierFeeBudgetV1::from_populated_v1(tx).map_err(|e| format!("{e:?}"))?;
        Ok((facts, fee))
    }
    /// Match only this exact actual carrier's own funding output. No input
    /// amounts, fee or acceptance flag are supplied by this transport, so none
    /// can be misreported as proof of payment. Final txid is obtained here,
    /// outside the signed terms; there is no funding-txid self-reference.
    pub fn inspect_carrier_v1<'a>(&self, tx: &'a Transaction) -> Result<ActualCarrierFundingV1<'a>, String> {
        let literal = self.encode_v1()?;
        let payload = if tx.payload.starts_with(b"FPA1") {
            crate::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_view_v1(&tx.payload, u32::MAX as usize)
                .ok_or("invalid FPA1 funded carrier")?.inner
        } else { &tx.payload };
        // Inspect the actual outer TX: its signed input scope and funding output are never projected.
        if tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT || payload != literal {
            return Err("TFO1 is not paired to this literal original carrier".into());
        }
        let t = &self.funding.terms;
        let facts = inspect_transaction_v1(tx, t.funding_output_index).map_err(|e| format!("{e:?}"))?;
        self.funding.check_actual_output_v1(&facts, &self.payer_pubkey).map_err(|e| format!("{e:?}"))?;
        Ok(facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_court_original_escrow_v1::FundingTermsV1;
    use crate::{
        Hash64,
        tx::{PopulatedTransaction, ScriptPublicKey, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    };
    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    /// Synthetic transport fields only: signatures are deliberately not valid
    /// consent, and UTXO entries in these tests are not native acceptance proof.
    fn envelope() -> PalwTirFundedOriginalEnvelopeV1 {
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_fixture_v1();
        let fp = crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            commitment: frame.original_v1().clone(),
            prompt_token_ids: crate::palw_tir_court_v1::test_support::tiny_execution(None).prompt,
            signature: vec![7; MLDSA87_SIGNATURE_LEN],
        };
        let original = PalwTirPublicFpEnvelopeV1::new_v1(
            borsh::to_vec(&fp).unwrap(),
            frame.header_v1().clone(),
            frame.custody_v1().pledge_v1().clone(),
            frame.plan_v1().terms.plan.rent_sompi,
        )
        .unwrap();
        let key = vec![9; MLDSA87_PUBKEY_LEN];
        let payer = mldsa87_key_id(&key);
        let funding = FundingConsentV1 {
            terms: FundingTermsV1 {
                version: 1,
                network_domain: h(1),
                claim: fp.claim_id(),
                job_identity: original.pledge.job_identity,
                execution_root: fp.commitment.execution_root,
                source_id: frame.plan_v1().source_id,
                relative_graph_id: h(2),
                fee_profile_id: h(3),
                capital_profile_id: h(4),
                retention_profile_id: h(5),
                settlement_policy_id: h(6),
                accept_from_daa: 100,
                accept_until_daa: 200,
                material_until_daa: 20_000,
                funding_output_index: 1,
                funding_amount_sompi: 1000,
                payer_key_id: payer,
                refund_payload: kaspa_hashes::blake2b_512_address_payload(&key),
                nonce: h(7),
            },
            signature: vec![8; MLDSA87_SIGNATURE_LEN],
        };
        PalwTirFundedOriginalEnvelopeV1::new_v1(original.encode_v1().unwrap(), funding, key).unwrap()
    }
    fn carrier(e: &PalwTirFundedOriginalEnvelopeV1) -> Transaction {
        Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(h(99), 0), vec![], 0, 0)],
            vec![
                TransactionOutput::new(1, ScriptPublicKey::default()),
                TransactionOutput::new(1000, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&kaspa_hashes::blake2b_512_address_payload(&e.payer_pubkey).as_bytes())),
            ],
            0,
            crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            0,
            e.encode_v1().unwrap(),
        )
    }
    #[test]
    fn roundtrip_preserves_literal_legacy_original_claim_and_signatures() {
        let e = envelope();
        let bytes = e.encode_v1().unwrap();
        let decoded = PalwTirFundedOriginalEnvelopeV1::decode_v1(&bytes).unwrap();
        assert_eq!(decoded, e);
        let old = e.original_v1().unwrap();
        let inner = old.original_fp_v1().unwrap();
        assert_eq!(decoded.original_envelope, old.encode_v1().unwrap());
        assert_eq!(decoded.original_v1().unwrap().original_fp_v1().unwrap(), inner);
        assert!(PalwTirPublicFpEnvelopeV1::decode_v1(&bytes).is_err());
        assert!(borsh::from_slice::<crate::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3>(&bytes).is_err());
        let mut changed = e.clone();
        changed.funding.terms.nonce = h(8);
        assert_ne!(e.funding.terms.id_v1(), changed.funding.terms.id_v1());
        assert_eq!(changed.original_v1().unwrap().original_fp_v1().unwrap().claim_id(), inner.claim_id());
        assert!(
            e.funding.verify_signature_v1(&e.payer_pubkey).is_err(),
            "transport shape must not certify a synthetic payer signature"
        );
    }
    #[test]
    fn new_framing_never_downgrades_to_legacy_even_when_malformed_or_truncated() {
        let e = envelope();
        let tx = carrier(&e);
        assert!(is_funded_original_carrier_v1(&tx));
        let mut old = tx.clone();
        old.payload = e.original_envelope.clone();
        old.finalize();
        assert!(!is_funded_original_carrier_v1(&old));
        let mut wrong = tx.clone();
        wrong.subnetwork_id = crate::subnets::SUBNETWORK_ID_NATIVE;
        wrong.finalize();
        assert!(!is_funded_original_carrier_v1(&wrong));
        for bytes in [b"T".to_vec(), b"TF".to_vec(), b"TFO".to_vec(), b"TFO1".to_vec(), b"TFO1\xff\xff".to_vec()] {
            let mut truncated = tx.clone();
            truncated.payload = bytes;
            truncated.finalize();
            assert!(is_funded_original_carrier_v1(&truncated));
            assert!(PalwTirFundedOriginalEnvelopeV1::decode_v1(&truncated.payload).is_err());
        }
        // Use the real old extractor, not a hand-written version probe. It must
        // produce no old object from a valid or malformed new transport.
        let params = crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_for_tests(h(1), h(2), h(3)).unwrap().freeprompt;
        let extracted = crate::palw_fp_objects_v3::palw_fp_objects_from_accepted_txs_v3(
            &[tx],
            h(1),
            &params,
            h(4),
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            |_, _, _, _| true,
        );
        assert!(extracted.objects.is_empty());
        assert_eq!(extracted.skipped.len(), 1);
    }
    #[test]
    fn bounded_decoding_rejects_lengths_versions_tails_and_identity_substitution() {
        let e = envelope();
        let bytes = e.encode_v1().unwrap();
        for cut in [0, 3, 4, 5, 6, 9, 10, bytes.len() - 1] {
            assert!(PalwTirFundedOriginalEnvelopeV1::decode_v1(&bytes[..cut]).is_err());
        }
        let mut bad = bytes.clone();
        bad.extend([0]);
        assert!(PalwTirFundedOriginalEnvelopeV1::decode_v1(&bad).is_err());
        let mut bad = bytes.clone();
        bad[4..6].copy_from_slice(&2u16.to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV1::decode_v1(&bad).is_err());
        let mut bad = bytes.clone();
        bad[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV1::decode_v1(&bad).is_err());
        let sig_len = 10 + e.original_envelope.len() + borsh::object_length(&e.funding.terms).unwrap();
        let mut bad = bytes.clone();
        bad[sig_len..sig_len + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV1::decode_v1(&bad).is_err());
        let key_len = sig_len + 4 + MLDSA87_SIGNATURE_LEN;
        let mut bad = bytes.clone();
        bad[key_len..key_len + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PalwTirFundedOriginalEnvelopeV1::decode_v1(&bad).is_err());
        for n in 0..4 {
            let mut bad = e.clone();
            match n {
                0 => bad.funding.terms.claim = h(9),
                1 => bad.funding.terms.job_identity = h(9),
                2 => bad.funding.terms.execution_root = h(9),
                _ => bad.payer_pubkey[0] ^= 1,
            };
            assert!(bad.encode_v1().is_err());
        }
        assert!(PalwTirFundedOriginalEnvelopeV1::decode_v1(&vec![0; PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1 + 1]).is_err());
    }
    #[test]
    fn accounting_outer_keeps_actual_funding_identity_and_rejects_fee_output_substitution() {
        let e = envelope();
        let plain = carrier(&e);
        let mut outer = plain.clone();
        outer.payload = crate::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_encode_v1(&plain.payload, &[7,8]);
        outer.finalize();
        assert_ne!(plain.id(), outer.id());
        let pairing = crate::palw_funded_original_coordinator_v1::palw_funded_original_answer_pairing_v1(&outer).unwrap();
        assert_eq!(pairing.carrier(), &outer);
        assert_eq!(pairing.outpoint(), TransactionOutpoint::new(outer.id(), 1));
        let populated = PopulatedTransaction::new(&outer, vec![UtxoEntry::new(1011, ScriptPublicKey::default(), 1, false)]);
        let (_, fee) = e.inspect_populated_carrier_v1(&populated).unwrap();
        assert_eq!(fee.actual_fee_v1(), 10, "funding remains an output, not a fee");
        for n in 0..3 {
            let mut wrong = outer.clone();
            match n {
                0 => wrong.outputs[1].value -= 1,
                1 => wrong.payload = crate::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_encode_v1(&outer.payload, &[7,8]),
                _ => wrong.payload.push(0),
            }
            wrong.finalize();
            assert!(e.inspect_carrier_v1(&wrong).is_err());
        }
        assert!(e.funding.verify_signature_v1(&e.payer_pubkey).is_err(), "framing never authenticates synthetic consent or answer/output binding");
    }
    #[test]
    fn actual_own_output_and_once_only_fee_are_bound_to_the_identical_carrier() {
        let e = envelope();
        let tx = carrier(&e);
        let entry = UtxoEntry::new(1011, ScriptPublicKey::default(), 1, false);
        let populated = PopulatedTransaction::new(&tx, vec![entry.clone()]);
        let (facts, mut fee) = e.inspect_populated_carrier_v1(&populated).unwrap();
        assert_eq!(facts.outpoint(), TransactionOutpoint::new(tx.id(), 1));
        assert_eq!(facts.output().value, 1000);
        assert_eq!(fee.carrier_v1(), facts.transaction_id());
        assert_eq!(fee.actual_fee_v1(), 10, "funding output is not also fee credit");
        fee.allocate_v1([1; 64], 7).unwrap();
        assert!(fee.allocate_v1([2; 64], 7).is_err());
        assert_eq!(fee.remaining_v1(), 3);
        for n in 0..5 {
            let mut bad = tx.clone();
            match n {
                0 => bad.outputs[1].value += 1,
                1 => bad.outputs[1].script_public_key = ScriptPublicKey::default(),
                2 => bad.outputs[1].script_public_key = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&[3; 64]),
                3 => bad.payload.push(0),
                _ => bad.outputs.truncate(1),
            };
            bad.finalize();
            assert!(e.inspect_carrier_v1(&bad).is_err());
        }
        let mut stale = tx.clone();
        stale.outputs[0].value += 1;
        assert!(e.inspect_carrier_v1(&stale).is_err());
        assert!(e.inspect_populated_carrier_v1(&PopulatedTransaction { tx: &tx, entries: vec![] }).is_err());
        let poor = PopulatedTransaction::new(&tx, vec![UtxoEntry::new(1000, ScriptPublicKey::default(), 1, false)]);
        assert!(e.inspect_populated_carrier_v1(&poor).is_err());
        let mut other = tx.clone();
        other.outputs[0].value += 1;
        other.finalize();
        assert_ne!(
            e.inspect_carrier_v1(&other).unwrap().outpoint(),
            facts.outpoint(),
            "final carrier id is native, not self-referenced in signed terms"
        );
    }
}
