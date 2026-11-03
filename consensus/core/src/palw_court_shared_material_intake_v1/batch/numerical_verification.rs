//! Signed authority for the booked numerical verification stage.
use super::admission::batch_numerical_service_ids_v1;
use crate::Hash64;
use crate::palw_batch_court_v1::carriage::{BATCH_OPEN_WIRE_OVERHEAD_V1, read_batch_signature_v1};
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};

const SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/batch-numerical-verification/v1/mldsa87";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub(crate) struct SignedNumericalBatchVerificationV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope: Hash64,
    pub notice: Hash64,
    pub leaf: u64,
    pub policy: Hash64,
    pub filer: PalwBondKeyV2,
    pub signature: Vec<u8>,
}

impl SignedNumericalBatchVerificationV1 {
    pub(crate) fn signing_message_v1(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 || [network, self.claim, self.scope, self.notice, self.policy].contains(&Hash64::default()) {
            return Err("numerical Batch verification has an invalid signing scope".into());
        }
        let (verification, terminal) = batch_numerical_service_ids_v1(self.claim, self.scope)
            .map_err(|why| format!("numerical Batch service identity: {why:?}"))?;
        // Bind the immutable packet IDs. The actual accepting DAA is
        // independently checked against the current assignment after repair.
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/batch-numerical-verification-signing/v1",
            &(network, self.version, self.claim, self.scope, self.notice, self.leaf, self.policy, self.filer, verification, terminal),
        )
    }

    fn verify_signature_v1(&self, network: Hash64, pubkey: &[u8]) -> Result<(), String> {
        if self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                pubkey,
                self.signing_message_v1(network)?.as_byte_slice(),
                &self.signature,
                SIGNATURE_CONTEXT_V1,
            )
        {
            return Err("invalid numerical Batch verification signature".into());
        }
        Ok(())
    }
}

impl BorshDeserialize for SignedNumericalBatchVerificationV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let version = u16::deserialize_reader(r)?;
        if version != 1 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "unknown numerical Batch verification version"));
        }
        Ok(Self {
            version,
            claim: Hash64::deserialize_reader(r)?,
            scope: Hash64::deserialize_reader(r)?,
            notice: Hash64::deserialize_reader(r)?,
            leaf: u64::deserialize_reader(r)?,
            policy: Hash64::deserialize_reader(r)?,
            filer: PalwBondKeyV2::deserialize_reader(r)?,
            signature: read_batch_signature_v1(r)?,
        })
    }
}

/// Authenticate a prospective verification control before running the
/// bounded replay. A successful finding still needs a journalled writer.
pub(crate) fn verify_rooted_numerical_batch_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
    signed: &SignedNumericalBatchVerificationV1,
    budget: super::native_ranges::NumericalBatchReplayBudgetV1,
) -> Result<super::native_ranges::NumericalBatchFindingV1, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    use crate::palw_court_shared_tir_state_v1::{self as shared, BatchNoticeTargetV1};
    use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
    use crate::palw_state_v2::PalwBondStatusV2;
    let invalid = |why: &str| E::Decode(why.into());
    let context = shared::context(state, signed.claim).map_err(E::Decode)?;
    let frame = super::super::frame_of(state, signed.claim).map_err(E::Decode)?;
    let policy = crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules).map_err(E::Decode)?;
    if signed.scope != context.scope_id || signed.policy != policy {
        return Err(invalid("numerical Batch verification changes its original scope or native policy"));
    }
    super::check_external_batch_filer_v1(frame, signed.filer).map_err(E::Decode)?;
    let bond = state.bond(&signed.filer).ok_or_else(|| invalid("numerical Batch verification filer is absent"))?;
    if !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < params.min_collateral_sompi() {
        return Err(invalid("numerical Batch verification filer is no longer eligible"));
    }
    let Some(Row::CourtSharedQuery(notice)) = state.staged_court_row_v1(&Key::CourtSharedQuery(signed.notice)) else {
        return Err(invalid("numerical Batch verification has no accepted notice"));
    };
    let target = crate::palw_batch_court_v1::rooted::checked_initial_notice_target_v1(state, context, notice, signed.notice)
        .map_err(E::Decode)?;
    if (notice.claim, notice.scope_id, notice.opened_by) != (signed.claim, signed.scope, signed.filer)
        || target != Some(BatchNoticeTargetV1::Numerical { leaf: signed.leaf })
    {
        return Err(invalid("numerical Batch verification differs from its accepted notice"));
    }
    let service = state.court_service_state_v1().ok_or_else(|| invalid("numerical Batch verification has no service book"))?;
    context.check_binding(service).map_err(E::Decode)?;
    let binding = service.claims.get(&signed.claim).ok_or_else(|| invalid("numerical Batch verification has no original binding"))?;
    let (packet, _) = batch_numerical_service_ids_v1(signed.claim, signed.scope)
        .map_err(|_| invalid("numerical Batch verification packet identities are invalid"))?;
    let bytes = (borsh::object_length(signed).map_err(|why| E::Decode(why.to_string()))? as u64)
        .checked_add(BATCH_OPEN_WIRE_OVERHEAD_V1).ok_or_else(|| invalid("numerical Batch verification size overflow"))?;
    if !binding.packet_ids.contains(&packet) || service.book.assigned_daa(&packet) != Some(rules.at.daa_score)
        || service.book.packet(&packet).is_none_or(|packet| bytes > packet.cost.bytes)
    {
        return Err(invalid("numerical Batch verification exceeds its reserved slot"));
    }
    signed.verify_signature_v1(rules.network_domain, &bond.pubkey).map_err(E::Decode)?;
    super::native_ranges::replay_numerical_batch_v1(state, signed.claim, rules, budget)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numerical_signature_binds_notice_leaf_policy_and_network() {
        let h = Hash64::from_u64_word;
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([123; 32]);
        let mut intent = SignedNumericalBatchVerificationV1 {
            version: 1, claim: h(1), scope: h(2), notice: h(3), leaf: 4, policy: h(5),
            filer: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(6), 7)), signature: vec![],
        };
        let net = h(8);
        intent.signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &keys.signing_key, intent.signing_message_v1(net).unwrap().as_byte_slice(), SIGNATURE_CONTEXT_V1, [124; 32],
        ).unwrap().as_ref().to_vec();
        let bytes = borsh::to_vec(&intent).unwrap();
        assert_eq!(borsh::from_slice::<SignedNumericalBatchVerificationV1>(&bytes).unwrap(), intent);
        let key = keys.verification_key.as_ref();
        assert!(intent.verify_signature_v1(net, key).is_ok());
        assert!(intent.verify_signature_v1(h(9), key).is_err());
        for altered in [
            SignedNumericalBatchVerificationV1 { leaf: 5, ..intent.clone() },
            SignedNumericalBatchVerificationV1 { notice: h(9), ..intent.clone() },
            SignedNumericalBatchVerificationV1 { policy: h(9), ..intent.clone() },
            SignedNumericalBatchVerificationV1 { scope: h(9), ..intent.clone() },
            SignedNumericalBatchVerificationV1 {
                filer: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(6), 8)), ..intent.clone()
            },
        ] {
            assert!(altered.verify_signature_v1(net, key).is_err());
        }
        let mut unsupported_version = bytes.clone();
        unsupported_version[0] = 2;
        assert!(borsh::from_slice::<SignedNumericalBatchVerificationV1>(&unsupported_version).is_err());
        let mut wrong_signature_length = bytes.clone();
        let length_offset = bytes.len() - intent.signature.len() - size_of::<u32>();
        wrong_signature_length[length_offset..length_offset + size_of::<u32>()].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<SignedNumericalBatchVerificationV1>(&wrong_signature_length).is_err());
        let mut truncated = bytes;
        truncated.pop();
        assert!(borsh::from_slice::<SignedNumericalBatchVerificationV1>(&truncated).is_err());
        let mut trailing = borsh::to_vec(&intent).unwrap();
        trailing.push(0);
        assert!(borsh::from_slice::<SignedNumericalBatchVerificationV1>(&trailing).is_err());
    }
}
