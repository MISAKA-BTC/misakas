//! RFC-0035 A private funding preparation. Actual output facts and a payer
//! signature are not UTXO admission, an escrow lock, a complete original graph,
//! actor consent, a fee receipt, a funded state row or a spend authorization.
//! No public dispatcher or rooted balance is installed by this component.
use crate::{
    Hash64, TransactionId,
    tx::{Transaction, TransactionOutpoint, TransactionOutput},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/court-original/funding/v1/mldsa87";

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FundingRefusalV1 {
    #[error("unsupported or incomplete funding contract")]
    Contract,
    #[error("funding carrier does not have a current native transaction identity")]
    CarrierIdentity,
    #[error("funding carrier is coinbase or has no ordinary input")]
    CarrierInputs,
    #[error("funding output index is absent")]
    OutputMissing,
    #[error("actual funding output differs from the signed amount, index or payer fallback")]
    OutputMismatch,
    #[error("funding consent is not signed by its exact payer key")]
    Signature,
    #[error("funding scope differs from the native expected original/graph/profile")]
    Scope,
    #[error("actual accepting DAA is outside the signed acceptance interval")]
    AcceptanceWindow,
    #[error("funding amount or material horizon does not cover the native derived requirement")]
    Coverage,
}

/// A fixed-size signing body. No final carrier txid is signed: including a txid
/// whose payload contains this signature would be self-referential. Native code
/// derives the outpoint from the final actual transaction and this exact index.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FundingTermsV1 {
    pub version: u16,
    pub network_domain: Hash64,
    pub claim: Hash64,
    pub job_identity: Hash64,
    pub execution_root: Hash64,
    pub source_id: Hash64,
    pub relative_graph_id: Hash64,
    pub fee_profile_id: Hash64,
    pub capital_profile_id: Hash64,
    pub retention_profile_id: Hash64,
    pub settlement_policy_id: Hash64,
    pub accept_from_daa: u64,
    pub accept_until_daa: u64,
    pub material_until_daa: u64,
    pub funding_output_index: u32,
    pub funding_amount_sompi: u64,
    /// Unkeyed identity of the signing key; never a P2PKH address payload.
    pub payer_key_id: Hash64,
    /// Native keyed MLDSA87 address of that same payer, checked with its key.
    pub refund_payload: Hash64,
    pub nonce: Hash64,
}
impl FundingTermsV1 {
    /// Wire structure only. Profile hashes are not evidence of a derived full
    /// graph or a calibrated fee schedule. Zero is not an unknown cost/profile.
    pub fn check_v1(&self) -> Result<(), FundingRefusalV1> {
        if self.version != 1
            || self.funding_amount_sompi == 0
            || self.accept_from_daa >= self.accept_until_daa
            || self.material_until_daa <= self.accept_until_daa
            || [
                self.network_domain,
                self.claim,
                self.job_identity,
                self.execution_root,
                self.source_id,
                self.relative_graph_id,
                self.fee_profile_id,
                self.capital_profile_id,
                self.retention_profile_id,
                self.settlement_policy_id,
                self.payer_key_id,
                self.refund_payload,
                self.nonce,
            ]
            .contains(&Hash64::default())
        {
            return Err(FundingRefusalV1::Contract);
        }
        Ok(())
    }
    /// Stream the fixed signing fields without allocating a serialization Vec.
    /// The digest authenticates the contract identity, not model computation.
    pub fn id_v1(&self) -> Hash64 {
        struct Sink<'a>(&'a mut blake2b_simd::State);
        impl std::io::Write for Sink<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.update(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/court-original/funding-id/v1").to_state();
        self.serialize(&mut Sink(&mut h)).expect("fixed-size funding terms serialize to the hash sink");
        Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("64-byte funding identity"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FundingConsentV1 {
    pub terms: FundingTermsV1,
    pub signature: Vec<u8>,
}
impl BorshDeserialize for FundingConsentV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let terms = FundingTermsV1::deserialize_reader(reader)?;
        terms.check_v1().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let n = u32::deserialize_reader(reader)?;
        if n as usize != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "funding signature has no exact MLDSA87 length"));
        }
        let mut signature = Vec::new();
        signature.try_reserve_exact(n as usize).map_err(|e| std::io::Error::new(std::io::ErrorKind::OutOfMemory, e.to_string()))?;
        signature.resize(n as usize, 0);
        reader.read_exact(&mut signature)?;
        Ok(Self { terms, signature })
    }
}
impl FundingConsentV1 {
    pub fn check_v1(&self) -> Result<(), FundingRefusalV1> {
        self.terms.check_v1()?;
        if self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
            return Err(FundingRefusalV1::Signature);
        }
        Ok(())
    }
    pub fn verify_signature_v1(&self, payer_pubkey: &[u8]) -> Result<(), FundingRefusalV1> {
        self.check_v1()?;
        if payer_pubkey.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || crate::mldsa87_primitives::mldsa87_key_id(payer_pubkey) != self.terms.payer_key_id
            || kaspa_hashes::blake2b_512_address_payload(payer_pubkey) != self.terms.refund_payload
            || !crate::palw_attempt_material_v3::verify_ready_signature_v3(
                payer_pubkey,
                self.terms.id_v1().as_byte_slice(),
                &self.signature,
                COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1,
            )
        {
            return Err(FundingRefusalV1::Signature);
        }
        Ok(())
    }
    /// Own-output facts only. A normal payer P2PKH fallback protects an
    /// unsuccessful candidate's coins; it is not by itself an escrow lock.
    pub fn check_actual_output_v1(&self, actual: &ActualCarrierFundingV1<'_>, payer_pubkey: &[u8]) -> Result<(), FundingRefusalV1> {
        self.check_v1()?;
        if payer_pubkey.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || crate::mldsa87_primitives::mldsa87_key_id(payer_pubkey) != self.terms.payer_key_id
            || kaspa_hashes::blake2b_512_address_payload(payer_pubkey) != self.terms.refund_payload
            || actual.output_index() != self.terms.funding_output_index
            || actual.output().value != self.terms.funding_amount_sompi
            || crate::mldsa87_primitives::p2pkh_mldsa87_payload(&actual.output().script_public_key) != Some(self.terms.refund_payload)
        {
            return Err(FundingRefusalV1::OutputMismatch);
        }
        Ok(())
    }
}

/// Borrowed actual own-carrier output. No public fields, Clone/Borsh or caller
/// `paid` flag. This does not validate its inputs or certify an accepted UTXO.
#[derive(Debug)]
pub struct ActualCarrierFundingV1<'a> {
    transaction: &'a Transaction,
    index: u32,
    output: &'a TransactionOutput,
}
impl ActualCarrierFundingV1<'_> {
    pub fn transaction_id(&self) -> TransactionId {
        self.transaction.id()
    }
    pub fn outpoint(&self) -> TransactionOutpoint {
        TransactionOutpoint::new(self.transaction_id(), self.index)
    }
    pub fn output_index(&self) -> u32 {
        self.index
    }
    pub fn output(&self) -> &TransactionOutput {
        self.output
    }
}
pub fn inspect_transaction_v1(tx: &Transaction, output_index: u32) -> Result<ActualCarrierFundingV1<'_>, FundingRefusalV1> {
    if !tx.id_is_current() {
        return Err(FundingRefusalV1::CarrierIdentity);
    }
    if tx.is_coinbase() || tx.inputs.is_empty() {
        return Err(FundingRefusalV1::CarrierInputs);
    }
    let output = tx.outputs.get(output_index as usize).ok_or(FundingRefusalV1::OutputMissing)?;
    Ok(ActualCarrierFundingV1 { transaction: tx, index: output_index, output })
}

/// Values recomputed by a future original coordinator. Constructing this DTO
/// from peer values is not graph closure, price calibration or admission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalFundingScopeV1 {
    pub claim: Hash64,
    pub job_identity: Hash64,
    pub execution_root: Hash64,
    pub source_id: Hash64,
    pub relative_graph_id: Hash64,
    pub fee_profile_id: Hash64,
    pub capital_profile_id: Hash64,
    pub retention_profile_id: Hash64,
    pub settlement_policy_id: Hash64,
    pub required_amount_sompi: u64,
    pub required_material_until_daa: u64,
}
/// Preparation result only. No serialization, Clone, paid/complete boolean,
/// state-root funding certificate, mutation method or public spend privilege.
#[derive(Debug)]
pub struct PreparedEscrowFundingV1<'a> {
    actual: ActualCarrierFundingV1<'a>,
    consent: &'a FundingConsentV1,
    payer_pubkey: &'a [u8],
    accepted_daa: u64,
}
impl PreparedEscrowFundingV1<'_> {
    pub fn actual(&self) -> &ActualCarrierFundingV1<'_> {
        &self.actual
    }
    pub fn consent(&self) -> &FundingConsentV1 {
        self.consent
    }
    pub fn payer_pubkey(&self) -> &[u8] {
        self.payer_pubkey
    }
    pub fn accepted_daa(&self) -> u64 {
        self.accepted_daa
    }
}
pub fn prepare_funding_v1<'a>(
    actual: ActualCarrierFundingV1<'a>,
    consent: &'a FundingConsentV1,
    payer_pubkey: &'a [u8],
    expected: &OriginalFundingScopeV1,
    native_network: Hash64,
    accepted_daa: u64,
) -> Result<PreparedEscrowFundingV1<'a>, FundingRefusalV1> {
    consent.check_v1()?;
    let t = &consent.terms;
    if native_network != t.network_domain
        || (
            t.claim,
            t.job_identity,
            t.execution_root,
            t.source_id,
            t.relative_graph_id,
            t.fee_profile_id,
            t.capital_profile_id,
            t.retention_profile_id,
            t.settlement_policy_id,
        ) != (
            expected.claim,
            expected.job_identity,
            expected.execution_root,
            expected.source_id,
            expected.relative_graph_id,
            expected.fee_profile_id,
            expected.capital_profile_id,
            expected.retention_profile_id,
            expected.settlement_policy_id,
        )
    {
        return Err(FundingRefusalV1::Scope);
    }
    if accepted_daa < t.accept_from_daa || accepted_daa > t.accept_until_daa {
        return Err(FundingRefusalV1::AcceptanceWindow);
    }
    let hard = accepted_daa.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA).ok_or(FundingRefusalV1::Coverage)?;
    if expected.required_amount_sompi == 0
        || expected.required_material_until_daa < hard
        || t.funding_amount_sompi < expected.required_amount_sompi
        || t.material_until_daa < expected.required_material_until_daa
    {
        return Err(FundingRefusalV1::Coverage);
    }
    consent.check_actual_output_v1(&actual, payer_pubkey)?;
    consent.verify_signature_v1(payer_pubkey)?;
    Ok(PreparedEscrowFundingV1 { actual, consent, payer_pubkey, accepted_daa })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    /// Synthetic contract identities/amounts test structure only. Real keys and
    /// actual Transaction outputs are used; no UTXO admission or fee result is
    /// claimed, and these amounts are not a production price recommendation.
    fn fixture() -> (Vec<u8>, FundingConsentV1, Transaction, OriginalFundingScopeV1) {
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
        let public = keys.verification_key.as_ref().to_vec();
        let payer = crate::mldsa87_primitives::mldsa87_key_id(&public);
        let terms = FundingTermsV1 {
            version: 1,
            network_domain: h(1),
            claim: h(2),
            job_identity: h(3),
            execution_root: h(4),
            source_id: h(5),
            relative_graph_id: h(6),
            fee_profile_id: h(7),
            capital_profile_id: h(8),
            retention_profile_id: h(9),
            settlement_policy_id: h(10),
            accept_from_daa: 10,
            accept_until_daa: 20,
            material_until_daa: 12_000,
            funding_output_index: 0,
            funding_amount_sompi: 150,
            payer_key_id: payer,
            refund_payload: kaspa_hashes::blake2b_512_address_payload(&public),
            nonce: h(11),
        };
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &keys.signing_key,
            terms.id_v1().as_byte_slice(),
            COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1,
            [92; 32],
        )
        .unwrap()
        .as_ref()
        .to_vec();
        let consent = FundingConsentV1 { terms, signature };
        let tx = Transaction::new(
            0,
            vec![crate::tx::TransactionInput::new(TransactionOutpoint::new(h(77), 0), Vec::new(), 0, 0)],
            vec![TransactionOutput::new(150, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&kaspa_hashes::blake2b_512_address_payload(&public).as_bytes()))],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![1, 2, 3],
        );
        let t = &consent.terms;
        let scope = OriginalFundingScopeV1 {
            claim: t.claim,
            job_identity: t.job_identity,
            execution_root: t.execution_root,
            source_id: t.source_id,
            relative_graph_id: t.relative_graph_id,
            fee_profile_id: t.fee_profile_id,
            capital_profile_id: t.capital_profile_id,
            retention_profile_id: t.retention_profile_id,
            settlement_policy_id: t.settlement_policy_id,
            required_amount_sompi: 100,
            required_material_until_daa: 11_000,
        };
        (public, consent, tx, scope)
    }

    #[test]
    fn real_signature_actual_own_output_and_native_scope_prepare_without_funding_state() {
        let (public, consent, tx, scope) = fixture();
        let before = tx.clone();
        let encoded = borsh::to_vec(&consent).unwrap();
        assert_eq!(borsh::from_slice::<FundingConsentV1>(&encoded).unwrap(), consent);
        for daa in [10, 20] {
            let actual = inspect_transaction_v1(&tx, 0).unwrap();
            let prepared = prepare_funding_v1(actual, &consent, &public, &scope, h(1), daa).unwrap();
            assert_eq!(prepared.actual().outpoint(), TransactionOutpoint::new(tx.id(), 0));
            assert_eq!(prepared.actual().output(), &tx.outputs[0]);
            assert_eq!(prepared.accepted_daa(), daa);
            assert_eq!(prepared.consent(), &consent);
            assert_eq!(prepared.payer_pubkey(), public.as_slice());
        }
        for daa in [9, 21] {
            assert_eq!(
                prepare_funding_v1(inspect_transaction_v1(&tx, 0).unwrap(), &consent, &public, &scope, h(1), daa).unwrap_err(),
                FundingRefusalV1::AcceptanceWindow
            );
        }
        assert_eq!(tx, before, "pure preparation neither spends UTXO nor mutates a state/accounting row");
    }

    #[test]
    fn actual_amount_index_unknown_version_free_script_and_stale_txid_are_refused() {
        let (public, consent, tx, _) = fixture();
        assert_eq!(inspect_transaction_v1(&tx, 1).unwrap_err(), FundingRefusalV1::OutputMissing);
        for mode in 0..4 {
            let mut bad = tx.clone();
            match mode {
                0 => bad.outputs[0].value -= 1,
                1 => {
                    bad.outputs[0].script_public_key =
                        crate::tx::ScriptPublicKey::from_vec(1, tx.outputs[0].script_public_key.script().to_vec())
                }
                2 => bad.outputs[0].script_public_key = crate::tx::ScriptPublicKey::from_vec(0, vec![0x51]),
                _ => bad.outputs[0].script_public_key = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&h(99).as_bytes()),
            }
            assert_eq!(inspect_transaction_v1(&bad, 0).unwrap_err(), FundingRefusalV1::CarrierIdentity);
            bad.finalize();
            assert_eq!(
                consent.check_actual_output_v1(&inspect_transaction_v1(&bad, 0).unwrap(), &public),
                Err(FundingRefusalV1::OutputMismatch)
            );
        }
        let mut changed = consent.clone();
        changed.terms.funding_output_index = 1;
        assert_eq!(changed.check_actual_output_v1(&inspect_transaction_v1(&tx, 0).unwrap(), &public), Err(FundingRefusalV1::OutputMismatch));
    }

    #[test]
    fn cross_network_profile_graph_scope_key_and_underfunding_are_refused() {
        let (public, consent, tx, scope) = fixture();
        assert_eq!(
            prepare_funding_v1(inspect_transaction_v1(&tx, 0).unwrap(), &consent, &public, &scope, h(99), 10).unwrap_err(),
            FundingRefusalV1::Scope
        );
        for mode in 0..9 {
            let mut bad = scope.clone();
            match mode {
                0 => bad.claim = h(99),
                1 => bad.job_identity = h(99),
                2 => bad.execution_root = h(99),
                3 => bad.source_id = h(99),
                4 => bad.relative_graph_id = h(99),
                5 => bad.fee_profile_id = h(99),
                6 => bad.capital_profile_id = h(99),
                7 => bad.retention_profile_id = h(99),
                _ => bad.settlement_policy_id = h(99),
            }
            assert_eq!(
                prepare_funding_v1(inspect_transaction_v1(&tx, 0).unwrap(), &consent, &public, &bad, h(1), 10).unwrap_err(),
                FundingRefusalV1::Scope
            );
        }
        for (amount, horizon) in [(151, 11_000), (0, 11_000), (100, 12_001), (100, 10_009)] {
            let mut bad = scope.clone();
            bad.required_amount_sompi = amount;
            bad.required_material_until_daa = horizon;
            assert_eq!(
                prepare_funding_v1(inspect_transaction_v1(&tx, 0).unwrap(), &consent, &public, &bad, h(1), 10).unwrap_err(),
                FundingRefusalV1::Coverage
            );
        }
        let wrong = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([93; 32]);
        assert_eq!(consent.verify_signature_v1(wrong.verification_key.as_ref()), Err(FundingRefusalV1::Signature));
        let mut altered = consent.clone();
        altered.terms.relative_graph_id = h(99);
        assert_eq!(altered.verify_signature_v1(&public), Err(FundingRefusalV1::Signature));
        let mut altered = consent.clone();
        altered.signature[0] ^= 1;
        assert_eq!(altered.verify_signature_v1(&public), Err(FundingRefusalV1::Signature));
    }

    #[test]
    fn payer_identity_is_not_the_native_address_and_unspendable_refunds_are_refused() {
        let (public, consent, tx, _) = fixture();
        let address = kaspa_hashes::blake2b_512_address_payload(&public);
        assert_ne!(address, consent.terms.payer_key_id);
        assert_eq!(consent.terms.refund_payload, address);
        consent.verify_signature_v1(&public).unwrap();
        consent.check_actual_output_v1(&inspect_transaction_v1(&tx,0).unwrap(), &public).unwrap();
        let wrong = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([93;32]);
        assert_eq!(consent.check_actual_output_v1(&inspect_transaction_v1(&tx,0).unwrap(),wrong.verification_key.as_ref()),
            Err(FundingRefusalV1::OutputMismatch));
        let mut trap = tx.clone();
        trap.outputs[0].script_public_key = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&consent.terms.payer_key_id.as_bytes());
        trap.finalize();
        assert_eq!(consent.check_actual_output_v1(&inspect_transaction_v1(&trap,0).unwrap(),&public),Err(FundingRefusalV1::OutputMismatch));
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91;32]);
        for refund in [consent.terms.payer_key_id, kaspa_hashes::blake2b_512_address_payload(wrong.verification_key.as_ref())] {
            let mut changed = consent.clone();
            changed.terms.refund_payload = refund;
            // Structure has no public key; it cannot derive the native address.
            changed.check_v1().unwrap();
            changed.signature = libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, changed.terms.id_v1().as_byte_slice(),
                COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1,[92;32]).unwrap().as_ref().to_vec();
            // Even a real payer signature cannot assign another/refund trap key.
            assert_eq!(changed.verify_signature_v1(&public),Err(FundingRefusalV1::Signature));
            assert_eq!(changed.check_actual_output_v1(&inspect_transaction_v1(&tx,0).unwrap(),&public),Err(FundingRefusalV1::OutputMismatch));
        }
    }

    #[test]
    fn signature_length_is_rejected_before_reading_any_signature_allocation_body() {
        let (_, consent, _, _) = fixture();
        let prefix = borsh::to_vec(&consent.terms).unwrap();
        for length in [0u32, 1, crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u32 - 1, u32::MAX] {
            let mut bytes = prefix.clone();
            bytes.extend_from_slice(&length.to_le_bytes());
            let mut reader = std::io::Cursor::new(bytes);
            let error = FundingConsentV1::deserialize_reader(&mut reader).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
            assert_eq!(reader.position(), (prefix.len() + 4) as u64);
        }
        let mut bad = consent.terms.clone();
        bad.version = 2;
        assert!(
            FundingConsentV1::deserialize_reader(&mut std::io::Cursor::new(borsh::to_vec(&bad).unwrap())).is_err(),
            "unknown version rejects before reading a vector length"
        );
        let mut truncated = prefix;
        truncated.extend_from_slice(&(crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u32).to_le_bytes());
        assert_eq!(
            FundingConsentV1::deserialize_reader(&mut truncated.as_slice()).unwrap_err().kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }
}
