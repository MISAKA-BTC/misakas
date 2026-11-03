//! Signed authority for the booked numerical verification stage.
use super::admission::batch_numerical_service_ids_v1;
use crate::Hash64;
use crate::palw_batch_court_v1::carriage::{BATCH_OPEN_WIRE_OVERHEAD_V1, read_batch_signature_v1};
use crate::palw_court_service_v1::ServiceCost;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1;
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};

pub const BATCH_NUMERICAL_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/batch-numerical-verification/v1/mldsa87";
const SIGNATURE_BYTES: usize = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
// Version, four identifiers, leaf, filer outpoint, vector length and signature.
const SIGNED_BODY_BYTES: u64 = (2 + 4 * 64 + 8 + 64 + 4 + 4 + SIGNATURE_BYTES) as u64;
const VERIFICATION_CARRIER_BYTES: u64 = SIGNED_BODY_BYTES + BATCH_OPEN_WIRE_OVERHEAD_V1;

/// The native profile supplies the replay ceilings and the costs outside the
/// arithmetic meter. Admission and acceptance use the same service quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumericalBatchReplayBudgetV1 {
    pub work: PalwTirBuilderWorkLimitsV1,
    pub workspace_bytes: u64,
    /// Maximum bytes the verification can write as its retained TIR finding.
    pub max_fault_bytes: u64,
    /// Non-arithmetic work, including signature, source and evidence checks.
    pub base_work: u64,
    pub io: u64,
}

/// Resolved by the native rules, with the exact service profile it prices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumericalBatchReplayProfileV1 {
    pub service_profile: Hash64,
    pub budget: NumericalBatchReplayBudgetV1,
}

impl NumericalBatchReplayBudgetV1 {
    pub(crate) fn service_cost_v1(self) -> Option<ServiceCost> {
        if self.workspace_bytes == 0 || self.base_work == 0 || self.io == 0
            || !(1..=crate::palw_batch_court_v1::rooted::MAX_NUMERICAL_FAULT_BYTES_V1 as u64).contains(&self.max_fault_bytes)
            || self.work.max_source_reads == 0 || self.work.max_target_requests == 0
            || self.work.evaluation.max_elements == 0 || self.work.evaluation.max_terms == 0
        {
            return None;
        }
        // Both the construction pass and the independent adjudicator can
        // reach the supplied evaluation ceilings.
        let work = self.work.evaluation.max_elements.checked_add(self.work.evaluation.max_terms)?
            .checked_mul(2)?
            .checked_add(self.work.max_target_requests)?
            .checked_add(self.work.max_source_reads)?
            .checked_add(self.base_work)?;
        let bytes = VERIFICATION_CARRIER_BYTES.checked_add(self.max_fault_bytes)?;
        Some(ServiceCost { bytes, work, io: self.io, controls: 1 })
    }

    #[cfg(test)]
    pub(crate) fn test_only_v1() -> Self {
        Self {
            work: PalwTirBuilderWorkLimitsV1 {
                evaluation: misaka_palw_tir::demand::DemandLimits { max_elements: 1, max_terms: 2 },
                max_target_requests: 3,
                max_source_reads: 4,
            },
            workspace_bytes: 4096,
            max_fault_bytes: 2048,
            base_work: 5,
            io: 6,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct SignedNumericalBatchVerificationV1 {
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
    pub fn signing_message_v1(&self, network: Hash64) -> Result<Hash64, String> {
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
        if self.signature.len() != SIGNATURE_BYTES
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                pubkey,
                self.signing_message_v1(network)?.as_byte_slice(),
                &self.signature,
                BATCH_NUMERICAL_SIGNATURE_CONTEXT_V1,
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

/// The template and accepting fold read the same rooted reservation.
pub(crate) fn reserved_numerical_verification_slot_v1<'a>(
    state: &'a crate::palw_state_v2::PalwChainStateV2,
    signed: &SignedNumericalBatchVerificationV1,
    daa: u64,
) -> Result<&'a crate::palw_court_service_v1::ServicePacket, String> {
    use crate::palw_court_shared_tir_state_v1::context;
    let context = context(state, signed.claim)?;
    let service = state.court_service_state_v1().ok_or("numerical Batch verification has no service book")?;
    context.check_binding(service)?;
    let binding = service.claims.get(&signed.claim).ok_or("numerical Batch verification has no original binding")?;
    let (packet, _) = batch_numerical_service_ids_v1(signed.claim, signed.scope)
        .map_err(|_| "numerical Batch verification packet identities are invalid")?;
    if signed.scope != context.scope_id || !binding.packet_ids.contains(&packet) || service.book.assigned_daa(&packet) != Some(daa) {
        return Err("numerical Batch verification is outside its reserved slot".into());
    }
    service.book.packet(&packet)
        .filter(|packet| packet.cost.bytes > VERIFICATION_CARRIER_BYTES)
        .ok_or_else(|| "numerical Batch verification has no correctly sized slot".into())
}

/// Authenticate the verification control and charge the originally booked
/// service before running the bounded replay.
pub(crate) fn verify_rooted_numerical_batch_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
    signed: &SignedNumericalBatchVerificationV1,
    budget: NumericalBatchReplayBudgetV1,
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
    if (notice.claim, notice.scope_id, notice.opened_by) != (signed.claim, signed.scope, signed.filer) {
        return Err(invalid("numerical Batch verification differs from its accepted notice"));
    }
    let cost = budget.service_cost_v1().ok_or_else(|| invalid("invalid numerical Batch replay budget"))?;
    let packet = reserved_numerical_verification_slot_v1(state, signed, rules.at.daa_score).map_err(E::Decode)?;
    if packet.cost != cost {
        return Err(invalid("numerical Batch verification exceeds its reserved slot"));
    }
    signed.verify_signature_v1(rules.network_domain, &bond.pubkey).map_err(E::Decode)?;
    let target = crate::palw_batch_court_v1::rooted::checked_initial_notice_target_v1(state, context, notice, signed.notice)
        .map_err(E::Decode)?;
    if target != Some(BatchNoticeTargetV1::Numerical { leaf: signed.leaf }) {
        return Err(invalid("numerical Batch verification differs from its accepted notice"));
    }
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
            &keys.signing_key, intent.signing_message_v1(net).unwrap().as_byte_slice(), BATCH_NUMERICAL_SIGNATURE_CONTEXT_V1, [124; 32],
        ).unwrap().as_ref().to_vec();
        let bytes = borsh::to_vec(&intent).unwrap();
        assert_eq!(bytes.len() as u64, SIGNED_BODY_BYTES);
        assert_eq!(borsh::from_slice::<SignedNumericalBatchVerificationV1>(&bytes).unwrap(), intent);
        use crate::palw_batch_court_v1::carriage::{
            BATCH_COURT_CARRIER_MAGIC_V1, BATCH_VERIFY_NUMERICAL_TAG_V1, BatchCourtWireV1, decode_batch_court_payload_v1,
        };
        let wire = BatchCourtWireV1::VerifyNumerical(intent.clone());
        let mut payload = BATCH_COURT_CARRIER_MAGIC_V1.to_vec();
        payload.extend(borsh::to_vec(&wire).unwrap());
        assert_eq!(payload[BATCH_COURT_CARRIER_MAGIC_V1.len()], BATCH_VERIFY_NUMERICAL_TAG_V1);
        assert_eq!(payload.len() as u64, VERIFICATION_CARRIER_BYTES);
        assert_eq!(decode_batch_court_payload_v1(&payload).unwrap(), Some(wire.clone()));
        use crate::palw_court_shared_material_intake_v1::ValidatedBatchCarrierV1;
        use crate::palw_state_v2::PalwBlockContextV2;
        use crate::tx::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint,
            TransactionOutput, UtxoEntry, ValidatedTransaction};
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(h(10), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(1, ScriptPublicKey::default())],
            0, crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE, 0, payload.clone(),
        );
        let utxo = UtxoEntry::new(4, ScriptPublicKey::default(), 1, false);
        let tx = ValidatedTransaction::new(PopulatedTransaction::new(&tx, vec![utxo]), 3);
        let at = PalwBlockContextV2 { block: h(11), daa_score: 8, blue_score: 9, subsidy: 0 };
        let handoff = ValidatedBatchCarrierV1::prepare_v1(&tx, 1, at).unwrap().unwrap();
        assert_eq!(handoff.wire_at_v1(&at).unwrap(), &wire);
        assert!(handoff.wire_at_v1(&PalwBlockContextV2 { daa_score: 9, ..at }).is_err());
        assert!(handoff.storage_debit_v1(None).unwrap().is_none());
        assert_eq!((handoff.fee_v1().withheld_storage_rent, handoff.fee_v1().collectible_fee), (0, 2));
        payload.pop();
        assert!(decode_batch_court_payload_v1(&payload).is_err());
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

    #[test]
    fn numerical_replay_quotes_both_passes_and_rejects_unfunded_work() {
        let mut budget = NumericalBatchReplayBudgetV1::test_only_v1();
        let cost = budget.service_cost_v1().unwrap();
        assert_eq!(cost, ServiceCost {
            bytes: VERIFICATION_CARRIER_BYTES + budget.max_fault_bytes, work: 18, io: 6, controls: 1,
        });
        assert!(NumericalBatchReplayBudgetV1 { workspace_bytes: 0, ..budget }.service_cost_v1().is_none());
        assert!(NumericalBatchReplayBudgetV1 { base_work: 0, ..budget }.service_cost_v1().is_none());
        assert!(NumericalBatchReplayBudgetV1 { io: 0, ..budget }.service_cost_v1().is_none());
        assert!(NumericalBatchReplayBudgetV1 { max_fault_bytes: 0, ..budget }.service_cost_v1().is_none());
        assert!(NumericalBatchReplayBudgetV1 { max_fault_bytes: u64::MAX, ..budget }.service_cost_v1().is_none());
        budget.work.evaluation.max_terms = u64::MAX;
        assert!(budget.service_cost_v1().is_none());
    }
}
