//! Strict ORIGINAL TIR full-allocation FP framing. The literal original FP
//! payload, claim, PoW input and signature are not rewritten. This DTO does not
//! verify native payment, registered class/layout, signature or admission, and
//! contains no custody delegation. Merkle/header hashes authenticate original
//! material identity, not correctness of model computation.
use crate::palw_freeprompt_v3::{PALW_FP_COMMITMENT_TX_MAX_BYTES, PalwFpCommitmentTxPayloadV3};
use crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1;
use crate::palw_tir_public_custody_v1::PalwTirPublicCustodyPledgeV1;
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::Cursor;

pub const PALW_TIR_PUBLIC_FP_ENVELOPE_MAGIC_V1: &[u8; 4] = b"TFP1";
pub const PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1: usize = 2048;
/// One fixed MLDSA87 pledge plus all of its bounded identity/clock fields.
pub const PALW_TIR_PUBLIC_FP_PLEDGE_MAX_BYTES_V1: usize = 8192;
pub const PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1: usize =
    4 + 2 + 8 + 4 + PALW_FP_COMMITMENT_TX_MAX_BYTES + PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1 + PALW_TIR_PUBLIC_FP_PLEDGE_MAX_BYTES_V1;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPublicFpEnvelopeV1 {
    pub version: u16,
    /// Untrusted burn declaration, not proof of payment. Native admission must
    /// compare it to the original full plan; UTXO fee burning is independent.
    pub declared_rent_sompi: u64,
    pub original_payload: Vec<u8>,
    pub header: PalwPublicTirBindingHeaderV1,
    pub pledge: PalwTirPublicCustodyPledgeV1,
}
impl BorshDeserialize for PalwTirPublicFpEnvelopeV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = u16::deserialize_reader(reader)?;
        if version != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unknown TFP1 envelope version before payload allocation",
            ));
        }
        let declared_rent_sompi = u64::deserialize_reader(reader)?;
        if declared_rent_sompi == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TFP1 has no rent declaration"));
        }
        let original_payload = crate::palw_public_binding_v1::read_bounded_bytes_v1(reader, PALW_FP_COMMITMENT_TX_MAX_BYTES)?;
        // The shared header decoder bounds network_id before allocation. The
        // pledge decoder independently bounds its one signature before allocation.
        let header = PalwPublicTirBindingHeaderV1::deserialize_reader(reader)?;
        let pledge = PalwTirPublicCustodyPledgeV1::deserialize_reader(reader)?;
        let result = Self { version, declared_rent_sompi, original_payload, header, pledge };
        result.check_v1().map_err(|why| std::io::Error::new(std::io::ErrorKind::InvalidData, why))?;
        Ok(result)
    }
}
impl PalwTirPublicFpEnvelopeV1 {
    pub fn new_v1(
        original_payload: Vec<u8>,
        header: PalwPublicTirBindingHeaderV1,
        pledge: PalwTirPublicCustodyPledgeV1,
        declared_rent_sompi: u64,
    ) -> Result<Self, String> {
        let result = Self { version: 1, declared_rent_sompi, original_payload, header, pledge };
        result.check_v1()?;
        Ok(result)
    }
    fn check_shape_v1(&self) -> Result<(), String> {
        if self.version != 1
            || self.declared_rent_sompi == 0
            || self.original_payload.is_empty()
            || self.original_payload.len() > PALW_FP_COMMITMENT_TX_MAX_BYTES
            || self.original_payload.starts_with(PALW_TIR_PUBLIC_FP_ENVELOPE_MAGIC_V1)
            || self.original_payload.starts_with(b"HFP1")
            || self.header.version != crate::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1
        {
            return Err("TFP1 envelope is recursive, oversized or not the exact prospective version".into());
        }
        crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(self.header.clone()).check_wire_v1()?;
        self.pledge.check_v1()?;
        if borsh::object_length(&self.header).map_err(|e| e.to_string())? > PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1
            || borsh::object_length(&self.pledge).map_err(|e| e.to_string())? > PALW_TIR_PUBLIC_FP_PLEDGE_MAX_BYTES_V1
            || borsh::object_length(self)
                .map_err(|e| e.to_string())?
                .checked_add(4)
                .is_none_or(|bytes| bytes > PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1)
        {
            return Err("TFP1 metadata exceeds its independent bounded wire envelope".into());
        }
        Ok(())
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.original_fp_v1().map(|_| ())
    }
    pub fn encode_v1(&self) -> Result<Vec<u8>, String> {
        self.check_v1()?;
        let mut bytes = PALW_TIR_PUBLIC_FP_ENVELOPE_MAGIC_V1.to_vec();
        bytes.extend(borsh::to_vec(self).map_err(|e| e.to_string())?);
        Ok(bytes)
    }
    pub fn decode_v1(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PALW_TIR_PUBLIC_FP_ENVELOPE_MAX_BYTES_V1 {
            return Err("TFP1 exceeds its total wire bound before allocation".into());
        }
        let body = bytes.strip_prefix(PALW_TIR_PUBLIC_FP_ENVELOPE_MAGIC_V1).ok_or("TFP1 magic is absent")?;
        // from_slice requires complete EOF; there is no ignored delegation or tail.
        borsh::from_slice(body).map_err(|e| e.to_string())
    }
    /// Decode the literal original codec, with bounded outer payload, public key,
    /// prompt list and signature. Existing version-specific inner job tails use
    /// the original job decoder, not a second reinterpreted job format. Their
    /// native structural/arming/fee/signature checks remain with admission.
    pub fn original_fp_v1(&self) -> Result<PalwFpCommitmentTxPayloadV3, String> {
        self.check_shape_v1()?;
        let payload = decode_original(&self.original_payload)?;
        let original = &payload.commitment;
        let ctx = &self.header.job_context;
        let class = crate::palw_fp_execution_v3::PalwFpClassFactsV3 {
            model_profile_id: ctx.model_profile_id,
            runtime_manifest_hash: ctx.runtime_manifest_hash,
            runtime_class_id: ctx.runtime_class_id,
            shape_profile_id: ctx.shape_profile_id,
            cu_ruleset_id: ctx.cu_ruleset_id,
        };
        let facts = crate::palw_fp_execution_v3::PalwFpRunFactsV3 {
            decode_tokens_executed: original.decode_tokens_executed,
            stop_reason: original.stop_reason,
            full_logits_trace_root: original.trace_root,
            activation_leg_root: Hash64::default(),
            checkpoint_leg_root: Hash64::default(),
            step_leg_root: self.header.step_merkle_root,
            step_leaf_count: original.work_leaves,
        };
        let expected = crate::palw_fp_execution_v3::palw_fp_job_context_v3(&original.job, &class, &facts, &ctx.network_id)
            .map_err(|e| e.to_string())?;
        let root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &ctx.context_hash(),
            &self.header.full_logits_trace_root,
            &ctx.shape_profile_id,
            self.header.step_leaf_count,
            &self.header.step_merkle_root,
        );
        if expected != *ctx
            || ctx.shape_profile_id != original.job.class_id
            || original.execution_root != self.header.committed_execution_root
            || root != self.header.committed_execution_root
            || original.trace_root != self.header.full_logits_trace_root
            || original.work_leaves != self.header.step_leaf_count
            || self.pledge.claim != payload.claim_id()
            || self.pledge.producer != PalwBondKeyV2(original.job.executor_bond)
            || self.pledge.execution_root != original.execution_root
            || self.pledge.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original)
            || self.pledge.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(ctx)
        {
            return Err("TFP1 substitutes original FP claim, producer, header context, roots, count or job".into());
        }
        Ok(payload)
    }
    /// Transaction-local pairing with the ordinary extractor's object. This
    /// performs no receipt/capital/payment admission and verifies no signatures.
    pub fn matches_object_v1(&self, object: &crate::palw_state_v2::PalwConsensusObjectV2) -> Result<(), String> {
        let payload = self.original_fp_v1()?;
        let c = &payload.commitment;
        let crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
            claim,
            class_id,
            bond,
            executor_pubkey,
            work_leaves,
            prompt_token_ids_hash,
            prompt_tokens,
            prompt_token_ids,
            decode_tokens_executed,
            trace_root,
            output_root,
            execution_root,
            trace_chunk_count,
            trace_retention_daa,
            job_pin,
            public_job,
            consumed_prefix_state,
            eval,
        } = object
        else {
            return Err("TFP1 is not paired with an original FP object".into());
        };
        if *claim != payload.claim_id()
            || *class_id != c.job.class_id
            || *bond != self.pledge.producer
            || *executor_pubkey != c.job.executor_pubkey
            || *work_leaves != c.work_leaves
            || *prompt_token_ids_hash != c.job.prompt_token_ids_hash
            || *prompt_tokens != c.job.prompt_tokens
            || *prompt_token_ids != payload.prompt_token_ids
            || *decode_tokens_executed != c.decode_tokens_executed
            || *trace_root != c.trace_root
            || *output_root != c.output_root
            || *execution_root != c.execution_root
            || *trace_chunk_count != c.trace_chunk_count
            || *trace_retention_daa != c.trace_retention_daa
            || *job_pin != self.pledge.job_identity
            || *consumed_prefix_state != payload.consumed_prefix_state_v1()
            || eval.is_some()
            || public_job.as_deref() != Some(c)
        {
            return Err("TFP1 is not the literal original FP object's transaction-local scope".into());
        }
        Ok(())
    }
}

fn decode_original(bytes: &[u8]) -> Result<PalwFpCommitmentTxPayloadV3, String> {
    // The original job's first variable field is its executor key. Preflight
    // that declaration before invoking the unchanged original job-tail codec.
    let mut key = Cursor::new(bytes);
    let version = u16::deserialize_reader(&mut key).map_err(|e| e.to_string())?;
    if version != crate::palw_freeprompt_v3::PALW_FP_V3_VERSION {
        return Err("TFP1 original payload is not the original FP V3 transport".into());
    }
    let _job_version = u16::deserialize_reader(&mut key).map_err(|e| e.to_string())?;
    let _network = Hash64::deserialize_reader(&mut key).map_err(|e| e.to_string())?;
    let _class = Hash64::deserialize_reader(&mut key).map_err(|e| e.to_string())?;
    let _bond = crate::tx::TransactionOutpoint::deserialize_reader(&mut key).map_err(|e| e.to_string())?;
    let key_length = u32::deserialize_reader(&mut key).map_err(|e| e.to_string())? as usize;
    if key_length > crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN || key_length > bytes.len().saturating_sub(key.position() as usize) {
        return Err("TFP1 original executor key exceeds native bound before allocation".into());
    }
    let mut reader = Cursor::new(bytes);
    let version = u16::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
    let commitment =
        crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3::deserialize_reader(&mut reader).map_err(|e| e.to_string())?;
    let count = u32::deserialize_reader(&mut reader).map_err(|e| e.to_string())? as usize;
    let remaining = bytes.len().saturating_sub(reader.position() as usize);
    if count > PALW_FP_COMMITMENT_TX_MAX_BYTES / 4 || count > remaining / 4 {
        return Err("TFP1 original prompt vector exceeds remaining wire bytes before allocation".into());
    }
    let mut prompt_token_ids = Vec::new();
    prompt_token_ids.try_reserve_exact(count).map_err(|_| "TFP1 bounded original prompt allocation failed")?;
    for _ in 0..count {
        prompt_token_ids.push(u32::deserialize_reader(&mut reader).map_err(|e| e.to_string())?);
    }
    let signature = crate::palw_public_binding_v1::read_signature_v1(&mut reader).map_err(|e| e.to_string())?;
    if reader.position() as usize != bytes.len() {
        return Err("TFP1 original FP has trailing bytes".into());
    }
    let result = PalwFpCommitmentTxPayloadV3 { version, commitment, prompt_token_ids, signature };
    if borsh::to_vec(&result).map_err(|e| e.to_string())? != bytes {
        return Err("TFP1 original FP encoding is not canonical".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn envelope() -> PalwTirPublicFpEnvelopeV1 {
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_fixture_v1();
        let payload = PalwFpCommitmentTxPayloadV3 {
            version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            commitment: frame.original_v1().clone(),
            prompt_token_ids: crate::palw_tir_court_v1::test_support::tiny_execution(None).prompt,
            signature: vec![7; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        // A synthetic original payload/pledge is useful transport evidence only:
        // no actual MLDSA, PoW, native payment or accepted standing is claimed.
        PalwTirPublicFpEnvelopeV1::new_v1(
            borsh::to_vec(&payload).unwrap(),
            frame.header_v1().clone(),
            frame.custody_v1().pledge_v1().clone(),
            frame.plan_v1().terms.plan.rent_sompi,
        )
        .unwrap()
    }
    #[test]
    fn actual_original_codec_roundtrip_preserves_literal_payload_and_signed_message() {
        let original = envelope();
        let decoded = PalwTirPublicFpEnvelopeV1::decode_v1(&original.encode_v1().unwrap()).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.original_payload, original.original_payload);
        let fp = original.original_fp_v1().unwrap();
        let recovered = decoded.original_fp_v1().unwrap();
        assert_eq!(recovered, fp);
        assert_eq!(recovered.claim_id(), fp.claim_id());
        assert_eq!(recovered.signature, fp.signature);
        assert_eq!(borsh::to_vec(&recovered).unwrap(), original.original_payload);
        let mut zero = original.clone();
        zero.declared_rent_sompi = 0;
        assert!(zero.check_v1().is_err());
        let mut different_rent = original.clone();
        different_rent.declared_rent_sompi += 1;
        // A positive declaration alone is not payment or native price proof.
        assert_eq!(different_rent.original_fp_v1().unwrap(), fp);
    }
    #[test]
    fn root_job_context_count_and_pledge_substitutions_are_refused() {
        let original = envelope();
        for n in 0..7 {
            let mut bad = original.clone();
            match n {
                0 => bad.header.committed_execution_root = Hash64::from_u64_word(99),
                1 => bad.header.step_merkle_root = Hash64::from_u64_word(99),
                2 => bad.header.job_context.execution_seed = Hash64::from_u64_word(99),
                3 => bad.header.job_context.exact_decode_tokens += 1,
                4 => bad.pledge.claim = Hash64::from_u64_word(99),
                5 => bad.pledge.producer = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(99), 0)),
                _ => bad.pledge.job_identity = Hash64::from_u64_word(99),
            }
            assert!(bad.check_v1().is_err());
        }
    }
    #[test]
    fn recursion_unknown_versions_trailing_and_preallocation_bounds_fail_closed() {
        let original = envelope();
        let encoded = original.encode_v1().unwrap();
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(PalwTirPublicFpEnvelopeV1::decode_v1(&trailing).is_err());
        let mut version = encoded;
        version[4..6].copy_from_slice(&2u16.to_le_bytes());
        assert!(PalwTirPublicFpEnvelopeV1::decode_v1(&version).is_err());
        let mut bad = original.clone();
        bad.original_payload = vec![0; PALW_FP_COMMITMENT_TX_MAX_BYTES + 1];
        assert!(bad.check_v1().is_err());
        for magic in [b"TFP1", b"HFP1"] {
            let mut bad = original.clone();
            bad.original_payload = magic.to_vec();
            assert!(bad.check_v1().is_err());
        }
        let mut length = b"TFP1".to_vec();
        length.extend(1u16.to_le_bytes());
        length.extend(1u64.to_le_bytes());
        length.extend(u32::MAX.to_le_bytes());
        assert!(PalwTirPublicFpEnvelopeV1::decode_v1(&length).is_err());
        let mut key = original.original_payload;
        // u16 payload +u16 job +two64-byte hashes +68-byte outpoint.
        key[200..204].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_original(&key).is_err());
    }
    #[test]
    fn same_transaction_object_pairing_does_not_accept_another_public_job() {
        let original = envelope();
        let fp = original.original_fp_v1().unwrap();
        let c = &fp.commitment;
        let mut object = crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted {
            claim: fp.claim_id(),
            class_id: c.job.class_id,
            bond: original.pledge.producer,
            executor_pubkey: c.job.executor_pubkey.clone(),
            work_leaves: c.work_leaves,
            prompt_token_ids_hash: c.job.prompt_token_ids_hash,
            prompt_tokens: c.job.prompt_tokens,
            prompt_token_ids: fp.prompt_token_ids.clone(),
            decode_tokens_executed: c.decode_tokens_executed,
            trace_root: c.trace_root,
            output_root: c.output_root,
            execution_root: c.execution_root,
            trace_chunk_count: c.trace_chunk_count,
            trace_retention_daa: c.trace_retention_daa,
            consumed_prefix_state: fp.consumed_prefix_state_v1(),
            job_pin: original.pledge.job_identity,
            public_job: Some(Box::new(c.clone())),
            eval: None,
        };
        original.matches_object_v1(&object).unwrap();
        let crate::palw_state_v2::PalwConsensusObjectV2::FreePromptCommitted { public_job, .. } = &mut object else { unreachable!() };
        *public_job = None;
        assert!(original.matches_object_v1(&object).is_err());
    }
}
