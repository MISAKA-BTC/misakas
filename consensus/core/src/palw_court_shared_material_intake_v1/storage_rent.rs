//! Batch storage rent is allocated from the accepted carrier's fee budget.
//! This projection is used alongside UTXO validation and the fee accumulator.
use super::{
    MATERIAL_BATCH_VERSION, MATERIAL_PIPELINED_MAX_BYTES, MaterialActionV1, SignedMaterialIntakeV1, material_intake_schedule_v1,
};
use crate::palw_batch_court_v1::{
    BatchRangeV1, MAX_BATCH_PART_BYTES_V1, MAX_BATCH_RANGES_V1,
    carriage::{
        BATCH_COURT_CARRIER_MAGIC_V1, BATCH_STOP_WIRE_OVERHEAD_V1, BATCH_VERIFY_NUMERICAL_TAG_V1,
        BatchCourtWireV1, BatchMaterialControlV1, batch_stop_wire_shape_v1,
        decode_batch_court_payload_v1,
    },
};
use crate::palw_court_carrier_fee_v1::CarrierFeeBudgetV1;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
    reserve_native_pending_requests_v1,
};
use crate::palw_state_v2::PalwBlockContextV2;
use crate::{
    Hash64,
    tx::{Transaction, ValidatedTransaction},
};
use std::sync::Arc;

fn storage_rent_for_bytes_v1(bytes: u64) -> Result<u64, String> {
    let schedule = material_intake_schedule_v1().map_err(|_| "Batch storage rent schedule absent")?;
    schedule.rent_for_bytes_v1(bytes)
}

/// Quote the existing per-byte storage rate for a V3 material declaration.
pub fn material_batch_storage_rent_v1(intake: &SignedMaterialIntakeV1) -> Result<Option<u64>, String> {
    let MaterialActionV1::Begin { total_bytes, .. } = intake.action else { return Ok(None) };
    if intake.version != MATERIAL_BATCH_VERSION {
        return Ok(None);
    }
    if !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(&total_bytes) {
        return Err("Batch storage rent declaration exceeds its part cap".into());
    }
    storage_rent_for_bytes_v1(total_bytes).map(Some)
}

/// The storage rent comes after the existing obligations. An underpaid
/// declaration may withhold only the remaining fee and cannot open a draft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialBatchCarrierFeeV1 {
    pub required_storage_rent: u64,
    pub withheld_storage_rent: u64,
    pub collectible_fee: u64,
}

impl MaterialBatchCarrierFeeV1 {
    fn split(required_storage_rent: u64, available_fee: u64) -> Self {
        let withheld_storage_rent = required_storage_rent.min(available_fee);
        Self { required_storage_rent, withheld_storage_rent, collectible_fee: available_fee - withheld_storage_rent }
    }

    pub fn pays_storage_rent_v1(self) -> bool {
        self.withheld_storage_rent == self.required_storage_rent
    }
}

/// Decoded once from a literal carrier. No state or acceptance is implied.
#[derive(Debug, PartialEq, Eq)]
struct PreparedBatchCarrierV1 {
    carrier: crate::TransactionId,
    wire: BatchCourtWireV1,
    storage_rent: Option<u64>,
}

impl PreparedBatchCarrierV1 {
    fn decode_v1(carrier: &Transaction) -> Result<Option<Self>, HandoffError> {
        use NativeDecodeErrorV1::{Decode, Resource};
        if carrier.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
            return Ok(None);
        }
        let Some(wire) = decode_batch_court_payload_v1(&carrier.payload).map_err(|error| {
            if error.kind() == std::io::ErrorKind::OutOfMemory {
                Resource(NativeCoverageV1::unsupported::<Self>("Batch carrier decode", &error.to_string()))
            } else {
                Decode(error.to_string())
            }
        })?
        else {
            return Ok(None);
        };
        if !carrier.id_is_current() {
            return Err(Decode("Batch carrier transaction identity is stale".into()));
        }
        let storage_rent = match &wire {
            BatchCourtWireV1::Respond(control) => material_batch_storage_rent_v1(&control.intake).map_err(Decode)?,
            BatchCourtWireV1::Stop(stop) => Some(storage_rent_for_bytes_v1(stop.operands.len() as u64).map_err(Decode)?),
            BatchCourtWireV1::Open(_) | BatchCourtWireV1::VerifyNumerical(_) => None,
        };
        Ok(Some(Self { carrier: carrier.id(), wire, storage_rent }))
    }

    fn wire_v1(&self) -> &BatchCourtWireV1 {
        &self.wire
    }

    fn split_v1(&self, available_fee: u64) -> MaterialBatchCarrierFeeV1 {
        MaterialBatchCarrierFeeV1::split(self.storage_rent.unwrap_or(0), available_fee)
    }

    fn allocate_v1(
        &self,
        budget: &mut CarrierFeeBudgetV1,
        request_root: Hash64,
        at: PalwBlockContextV2,
    ) -> Result<Option<MaterialBatchStorageDebitV1>, String> {
        if self.carrier != budget.carrier_v1() {
            return Err("Batch storage rent budget belongs to another carrier".into());
        }
        let (Some(amount), BatchCourtWireV1::Respond(BatchMaterialControlV1 { demand_ordinal, intake })) =
            (self.storage_rent, &self.wire)
        else {
            return Ok(None);
        };
        if !self.split_v1(budget.remaining_v1()).pays_storage_rent_v1() {
            return Err("Batch storage rent carrier has insufficient available fee".into());
        }
        let declaration = declaration_id(intake, request_root, *demand_ordinal)?;
        // One allocation per physical carrier, even if a caller supplies
        // another request root for the same transaction.
        let purpose =
            crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-storage-fee-purpose/v1", &self.carrier)?;
        budget.allocate_v1(purpose.as_bytes(), amount).map_err(|err| format!("Batch storage rent: {err:?}"))?;
        Ok(Some(MaterialBatchStorageDebitV1 { at, declaration, amount }))
    }
}

/// Ephemeral handoff for a ruleset that has accepted a Batch transaction.
/// The UTXO walk constructs it before dropping the input view. The Court
/// fold consumes it at the same accepting point without retaining UTXOs.
#[derive(Debug, PartialEq, Eq)]
pub struct ValidatedBatchCarrierV1 {
    batch: PreparedBatchCarrierV1,
    budget: CarrierFeeBudgetV1,
    at: PalwBlockContextV2,
}

crate::palw_native_resource_v1::native_struct!(PreparedBatchCarrierV1; carrier, wire, storage_rent);
crate::palw_native_resource_v1::native_struct!(ValidatedBatchCarrierV1; batch, budget, at);

/// Clones of the accepting-block handoff share the decoded bytes and lease.
pub type SharedValidatedBatchCarrierV1 = Arc<NativeRetainedV1<ValidatedBatchCarrierV1>>;

type HandoffError = NativeDecodeErrorV1<String>;

impl ValidatedBatchCarrierV1 {
    pub fn prepare_v1(
        validated: &ValidatedTransaction<'_>,
        preceding_fee: u64,
        at: PalwBlockContextV2,
    ) -> Result<Option<SharedValidatedBatchCarrierV1>, NativeDecodeErrorV1<String>> {
        Self::prepare_reserved_v1(validated, preceding_fee, at, reserve_native_pending_requests_v1)
    }

    fn prepare_reserved_v1(
        validated: &ValidatedTransaction<'_>,
        preceding_fee: u64,
        at: PalwBlockContextV2,
        mut reserve: impl FnMut(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
    ) -> Result<Option<SharedValidatedBatchCarrierV1>, HandoffError> {
        use NativeDecodeErrorV1::{Decode, Resource};
        if validated.tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
            return Ok(None);
        }
        let Some(body) = validated.tx.payload.strip_prefix(BATCH_COURT_CARRIER_MAGIC_V1) else {
            return Ok(None);
        };
        let mut q = TirNativeRequestsV1::new();
        let kind = body.first().copied();
        if matches!(kind, Some(0 | 1 | BATCH_VERIFY_NUMERICAL_TAG_V1)) {
            q.exact_v1::<u8>(crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64).map_err(Resource)?;
        }
        match kind {
            // The bounded decoder may reserve a declared length before discovering
            // truncated data. Quote the codec maximum, never the unread suffix.
            Some(0) => q.exact_v1::<BatchRangeV1>(MAX_BATCH_RANGES_V1 as u64),
            Some(1) => q.exact_v1::<u8>(u64::from(MAX_BATCH_PART_BYTES_V1)),
            Some(2) => {
                batch_stop_wire_shape_v1(&validated.tx.payload).map_err(|why| Decode(why.to_string()))?;
                q.exact_v1::<u8>(validated.tx.payload.len() as u64 - BATCH_STOP_WIRE_OVERHEAD_V1)
            }
            Some(BATCH_VERIFY_NUMERICAL_TAG_V1) => Ok(()),
            _ => return Err(Decode("unknown Batch Court carrier kind".into())),
        }
        .map_err(Resource)?;
        q.exact_v1::<NativeRetainedV1<Self>>(1).map_err(Resource)?;
        q.add_v1((std::mem::size_of::<[usize; 2]>() + std::mem::align_of::<NativeRetainedV1<Self>>()) as u64).map_err(Resource)?;
        q.add_v1(btree_node_envelope_bytes_v1::<crate::TransactionId, SharedValidatedBatchCarrierV1>() as u64).map_err(Resource)?;
        // One preceding allocation in the retained fee budget. During replay
        // one scratch copy coexists and can add the storage debit.
        q.add_v1(3 * btree_node_envelope_bytes_v1::<[u8; 64], u64>() as u64).map_err(Resource)?;
        // Candidate, accepted and final-extra index arrays may coexist.
        for _ in 0..3 {
            q.vec_v1::<(usize, SharedValidatedBatchCarrierV1)>(1).map_err(Resource)?;
        }
        let lease = reserve(q.bytes_v1()).map_err(Resource)?;
        let scratch = (validated.tx.inputs.len() as u64)
            .checked_mul(btree_node_envelope_bytes_v1::<(crate::TransactionId, u32), ()>() as u64)
            .ok_or_else(|| Resource(NativeCoverageV1::overflow::<Self>("Batch fee input set")))?;
        let _scratch = if scratch == 0 { None } else { reserve(scratch).map_err(Resource)? };
        let batch = PreparedBatchCarrierV1::decode_v1(validated.tx)?
            .ok_or_else(|| Decode("validated Batch carrier lost its payload".into()))?;
        let fee_error = |err| Decode(format!("Batch native carrier fee: {err:?}"));
        let mut budget = CarrierFeeBudgetV1::from_validated_v1(validated).map_err(fee_error)?;
        if preceding_fee != 0 {
            let identity =
                crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-preceding-fee/v1", &validated.tx.id())
                    .map_err(Decode)?;
            budget.allocate_v1(identity.as_bytes(), preceding_fee).map_err(fee_error)?;
        }
        Ok(Some(Arc::new(NativeRetainedV1::from_owner_reserved_v1(Self { batch, budget, at }, lease))))
    }

    pub fn fee_v1(&self) -> MaterialBatchCarrierFeeV1 {
        self.batch.split_v1(self.budget.remaining_v1())
    }

    pub(crate) fn wire_at_v1(&self, at: &PalwBlockContextV2) -> Result<&BatchCourtWireV1, String> {
        if self.at != *at {
            return Err("Batch carrier accepting point differs from its UTXO handoff".into());
        }
        Ok(self.batch.wire_v1())
    }

    /// Rehearsal and the final fold use an independent debit projection over
    /// the same immutable UTXO fact. The full payload is never cloned here.
    pub(crate) fn storage_debit_v1(&self, request_root: Option<Hash64>) -> Result<Option<MaterialBatchStorageDebitV1>, String> {
        if matches!(&self.batch.wire, BatchCourtWireV1::Stop(_)) {
            if request_root.is_some() || !self.fee_v1().pays_storage_rent_v1() {
                return Err("Batch Stop carrier has insufficient storage rent or an unexpected Begin request root".into());
            }
            // Stop stores exactly one immutable receipt. The UTXO handoff already
            // withholds this rent from the native fee; there is no Begin ledger.
            return Ok(None);
        }
        if self.batch.storage_rent.is_some() != request_root.is_some() {
            return Err("Batch storage rent handoff requires the accepted request root for Begin".into());
        }
        match request_root {
            Some(root) => self.batch.allocate_v1(&mut self.budget.clone(), root, self.at),
            None => Ok(None),
        }
    }
}

/// Shared fee accounting for an enabled Batch route. Callers supply the fee
/// remaining after mandatory bond burn and all preceding fee allocations.
pub fn material_batch_carrier_fee_v1(carrier: &Transaction, available_fee: u64) -> Result<MaterialBatchCarrierFeeV1, String> {
    if carrier.subnetwork_id == crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE
        && batch_stop_wire_shape_v1(&carrier.payload).map_err(|why| why.to_string())?
    {
        if !carrier.id_is_current() {
            return Err("Batch carrier transaction identity is stale".into());
        }
        let operands_bytes = carrier.payload.len() as u64 - BATCH_STOP_WIRE_OVERHEAD_V1;
        return Ok(MaterialBatchCarrierFeeV1::split(storage_rent_for_bytes_v1(operands_bytes)?, available_fee));
    }
    Ok(match PreparedBatchCarrierV1::decode_v1(carrier).map_err(|error| error.to_string())? {
        Some(batch) => batch.split_v1(available_fee),
        None => MaterialBatchCarrierFeeV1::split(0, available_fee),
    })
}

/// One storage debit allocated from the same native budget as other costs.
/// The caller keeps this fact with the exact accepted object and excludes the
/// amount from collectible fees. It must not survive an accepting-point change.
#[derive(Debug)]
pub struct MaterialBatchStorageDebitV1 {
    at: PalwBlockContextV2,
    declaration: Hash64,
    amount: u64,
}

impl MaterialBatchStorageDebitV1 {
    pub fn allocate_v1(
        carrier: &Transaction,
        budget: &mut CarrierFeeBudgetV1,
        request_root: Hash64,
        at: PalwBlockContextV2,
    ) -> Result<Self, String> {
        let prepared = PreparedBatchCarrierV1::decode_v1(carrier)
            .map_err(|error| error.to_string())?
            .ok_or("Batch storage rent carrier has another purpose")?;
        prepared.allocate_v1(budget, request_root, at)?.ok_or_else(|| "Batch storage rent carrier is not a V3 Begin".into())
    }

    pub fn amount_v1(&self) -> u64 {
        self.amount
    }

    pub(crate) fn check_v1(
        &self,
        intake: &SignedMaterialIntakeV1,
        request_root: Hash64,
        demand_ordinal: u64,
        at: &PalwBlockContextV2,
        amount: u64,
    ) -> Result<(), String> {
        if self.at != *at || self.amount != amount || self.declaration != declaration_id(intake, request_root, demand_ordinal)? {
            return Err("Batch storage rent debit differs from the accepted declaration".into());
        }
        Ok(())
    }
}

fn declaration_id(intake: &SignedMaterialIntakeV1, request_root: Hash64, demand_ordinal: u64) -> Result<Hash64, String> {
    if request_root == Hash64::default() {
        return Err("Batch storage rent lacks the accepted request root".into());
    }
    crate::palw_court_shared_tir_stream_v1::stream_digest(
        b"misaka-palw/batch-storage-declaration/v1",
        &(request_root, demand_ordinal, intake),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_batch_court_v1::carriage::BATCH_COURT_CARRIER_MAGIC_V1;
    use crate::palw_court_shared_material_intake_v1::MaterialTargetV1;
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
    use crate::tx::{
        PopulatedTransaction, ScriptPublicKey, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
        ValidatedTransaction,
    };

    fn assert_batch_lease_lifetime_v1(
        validated: &ValidatedTransaction<'_>,
        at: PalwBlockContextV2,
        retained: u64,
        scratch: u64,
    ) {
        use crate::palw_native_resource_v1::{
            NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1,
        };
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 {
            memory_bytes: retained + scratch,
            database_bytes: 0,
        });
        let owner = ValidatedBatchCarrierV1::prepare_reserved_v1(validated, 1, at, |bytes| {
            checker.reserve_requests_v1(bytes, 0).map(Some)
        })
        .unwrap()
        .unwrap();
        assert_eq!(
            checker.outstanding(),
            (retained, 0),
            "temporary input set charge is released"
        );
        let shared = Arc::clone(&owner);
        drop(owner);
        assert_eq!(
            checker.outstanding(),
            (retained, 0),
            "the remaining owner holds the same lease"
        );
        drop(shared);
        assert_eq!(checker.outstanding(), (0, 0));

        let limited = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 {
            memory_bytes: retained,
            database_bytes: 0,
        });
        let blocked = ValidatedBatchCarrierV1::prepare_reserved_v1(validated, 1, at, |bytes| {
            limited.reserve_requests_v1(bytes, 0).map(Some)
        });
        assert!(matches!(blocked, Err(NativeDecodeErrorV1::Resource(_))));
        assert_eq!(
            limited.outstanding(),
            (0, 0),
            "the first reservation rolls back on scratch refusal"
        );
    }

    #[test]
    fn batch_storage_rent_uses_one_exact_accepted_carrier_allocation() {
        let mut intake = SignedMaterialIntakeV1 {
            version: MATERIAL_BATCH_VERSION,
            claim: Hash64::from_u64_word(1),
            scope_id: Hash64::from_u64_word(2),
            target: MaterialTargetV1::PromptFlat,
            submitter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(3), 0)),
            action: MaterialActionV1::Begin { total_bytes: 1024, body_hash: Hash64::from_u64_word(4) },
            signature: vec![1; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        let amount = material_batch_storage_rent_v1(&intake).unwrap().unwrap();
        assert!(amount > 0);
        let demand_ordinal = 8;
        let mut payload = BATCH_COURT_CARRIER_MAGIC_V1.to_vec();
        payload.extend(
            borsh::to_vec(&BatchCourtWireV1::Respond(BatchMaterialControlV1 { demand_ordinal, intake: intake.clone() })).unwrap(),
        );
        let output = 1_000;
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(5), 0), Vec::new(), 0, 0)],
            vec![TransactionOutput::new(output, ScriptPublicKey::default())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        let budget = |fee| {
            CarrierFeeBudgetV1::from_validated_v1(&ValidatedTransaction::new(
                PopulatedTransaction::new(&tx, vec![UtxoEntry::new(output + fee, ScriptPublicKey::default(), 1, false)]),
                fee,
            ))
            .unwrap()
        };
        let at = PalwBlockContextV2 { block: Hash64::from_u64_word(6), daa_score: 7, blue_score: 8, subsidy: 0 };
        let request = Hash64::from_u64_word(9);
        let other_request = Hash64::from_u64_word(10);
        let short_split = material_batch_carrier_fee_v1(&tx, amount - 1).unwrap();
        assert!(!short_split.pays_storage_rent_v1());
        assert_eq!((short_split.withheld_storage_rent, short_split.collectible_fee), (amount - 1, 0));
        let full_split = material_batch_carrier_fee_v1(&tx, 2 * amount).unwrap();
        assert!(full_split.pays_storage_rent_v1());
        assert_eq!((full_split.withheld_storage_rent, full_split.collectible_fee), (amount, amount));
        let native = ValidatedTransaction::new(
            PopulatedTransaction::new(&tx, vec![UtxoEntry::new(output + 2 * amount + 1, ScriptPublicKey::default(), 1, false)]),
            2 * amount + 1,
        );
        let mut requests = Vec::new();
        let metered = ValidatedBatchCarrierV1::prepare_reserved_v1(&native, 1, at, |bytes| {
            requests.push(bytes);
            Ok(None)
        })
        .unwrap()
        .unwrap();
        assert_eq!(metered.fee_v1(), full_split);
        assert_eq!(requests.len(), 2);
        assert!(requests[0] > crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64 && requests[1] > 0);
        {
            use crate::palw_native_resource_v1::NativeHeapV1;
            let heap = metered.native_heap_v1("accepted Batch handoff").bytes().unwrap();
            let fee_node = btree_node_envelope_bytes_v1::<[u8; 64], u64>() as u64;
            assert!(heap >= fee_node + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64);
            assert!(requests[0] >= heap, "the handoff ticket covers its retained heap");
        }
        let mut malformed = tx.clone();
        malformed.payload[BATCH_COURT_CARRIER_MAGIC_V1.len()] = u8::MAX;
        malformed.finalize();
        let malformed = ValidatedTransaction::new(
            PopulatedTransaction::new(&malformed, native.entries.clone()),
            native.calculated_fee,
        );
        let malformed = ValidatedBatchCarrierV1::prepare_reserved_v1(&malformed, 1, at, |_| {
            panic!("an unknown carrier kind must be rejected before reservation")
        });
        assert!(matches!(malformed, Err(NativeDecodeErrorV1::Decode(_))));
        let denied = ValidatedBatchCarrierV1::prepare_reserved_v1(&native, 1, at, |_| {
            Err(NativeCoverageV1::unsupported::<ValidatedBatchCarrierV1>("Batch test", "no space"))
        });
        assert!(matches!(denied, Err(NativeDecodeErrorV1::Resource(_))));
        assert_batch_lease_lifetime_v1(&native, at, requests[0], requests[1]);
        let handoff = ValidatedBatchCarrierV1::prepare_v1(&native, 1, at).unwrap().unwrap();
        assert_eq!(handoff.fee_v1(), full_split);
        assert!(handoff.wire_at_v1(&at).is_ok());
        assert!(handoff.wire_at_v1(&PalwBlockContextV2 { block: other_request, ..at }).is_err());
        assert!(handoff.storage_debit_v1(None).is_err());
        for _ in 0..2 {
            let debit = handoff.storage_debit_v1(Some(request)).unwrap().unwrap();
            assert!(debit.check_v1(&intake, request, demand_ordinal, &at, amount).is_ok());
            assert_eq!(handoff.fee_v1(), full_split);
        }
        let prepared = PreparedBatchCarrierV1::decode_v1(&tx).unwrap().unwrap();
        let mut short = budget(amount);
        short.allocate_v1([12; 64], 1).unwrap();
        assert!(prepared.allocate_v1(&mut short, request, at).is_err());
        assert_eq!(short.remaining_v1(), amount - 1);
        let mut fees = budget(2 * amount + 1);
        fees.allocate_v1([12; 64], 1).unwrap();
        let paid = prepared.allocate_v1(&mut fees, request, at).unwrap().unwrap();
        assert_eq!(paid.amount_v1(), amount);
        assert_eq!(fees.remaining_v1(), full_split.collectible_fee);
        assert!(prepared.allocate_v1(&mut fees, other_request, at).is_err());
        assert_eq!(fees.remaining_v1(), full_split.collectible_fee);
        assert!(paid.check_v1(&intake, request, demand_ordinal, &at, amount).is_ok());
        assert!(paid.check_v1(&intake, other_request, demand_ordinal, &at, amount).is_err());
        assert!(paid.check_v1(&intake, request, demand_ordinal, &PalwBlockContextV2 { daa_score: 8, ..at }, amount).is_err());
        assert!(paid.check_v1(&intake, request, demand_ordinal + 1, &at, amount).is_err());
        intake.signature[0] ^= 1;
        assert!(paid.check_v1(&intake, request, demand_ordinal, &at, amount).is_err());
        intake.version = 1;
        assert_eq!(material_batch_storage_rent_v1(&intake), Ok(None));
    }
}
