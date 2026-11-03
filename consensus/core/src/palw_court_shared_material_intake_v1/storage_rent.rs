//! Batch storage rent is allocated from the accepted carrier's fee budget.
//! This projection is used alongside UTXO validation and the fee accumulator.
use super::{
    MATERIAL_BATCH_VERSION, MATERIAL_PIPELINED_MAX_BYTES, MaterialActionV1, SignedMaterialIntakeV1, material_intake_schedule_v1,
};
use crate::palw_batch_court_v1::carriage::{BatchCourtWireV1, BatchMaterialControlV1, decode_batch_court_carrier_v1};
use crate::palw_court_carrier_fee_v1::CarrierFeeBudgetV1;
use crate::palw_state_v2::PalwBlockContextV2;
use crate::{
    Hash64,
    tx::{Transaction, ValidatedTransaction},
};

/// Quote the existing per-byte storage rate for a V3 declaration only.
pub fn material_batch_storage_rent_v1(intake: &SignedMaterialIntakeV1) -> Result<Option<u64>, String> {
    let MaterialActionV1::Begin { total_bytes, .. } = intake.action else { return Ok(None) };
    if intake.version != MATERIAL_BATCH_VERSION {
        return Ok(None);
    }
    if !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(&total_bytes) {
        return Err("Batch storage rent declaration exceeds its part cap".into());
    }
    let schedule = material_intake_schedule_v1().map_err(|_| "Batch storage rent schedule absent")?;
    Ok(Some(schedule.sompi_per_byte.checked_mul(total_bytes).ok_or("Batch storage rent overflow")?))
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
    fn decode_v1(carrier: &Transaction) -> Result<Option<Self>, String> {
        let Some(wire) = decode_batch_court_carrier_v1(carrier)? else { return Ok(None) };
        if !carrier.id_is_current() {
            return Err("Batch carrier transaction identity is stale".into());
        }
        let storage_rent = match &wire {
            BatchCourtWireV1::Respond(control) => material_batch_storage_rent_v1(&control.intake)?,
            BatchCourtWireV1::Open(_) => None,
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

impl ValidatedBatchCarrierV1 {
    pub fn prepare_v1(
        validated: &ValidatedTransaction<'_>,
        preceding_fee: u64,
        at: PalwBlockContextV2,
    ) -> Result<Option<Self>, String> {
        let Some(batch) = PreparedBatchCarrierV1::decode_v1(validated.tx)? else { return Ok(None) };
        let fee_error = |err| format!("Batch native carrier fee: {err:?}");
        let mut budget = CarrierFeeBudgetV1::from_validated_v1(validated).map_err(fee_error)?;
        if preceding_fee != 0 {
            let identity =
                crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-preceding-fee/v1", &validated.tx.id())?;
            budget.allocate_v1(identity.as_bytes(), preceding_fee).map_err(fee_error)?;
        }
        Ok(Some(Self { batch, budget, at }))
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
        if self.batch.storage_rent.is_some() != request_root.is_some() {
            return Err("Batch storage rent handoff requires the accepted request root for Begin".into());
        }
        match request_root {
            Some(root) => self.batch.allocate_v1(&mut self.budget.clone(), root, self.at),
            None => Ok(None),
        }
    }
}

/// Shared fee accounting for an enabled V3 route. Callers supply the fee
/// remaining after mandatory bond burn and all preceding fee allocations.
pub fn material_batch_carrier_fee_v1(carrier: &Transaction, available_fee: u64) -> Result<MaterialBatchCarrierFeeV1, String> {
    Ok(match PreparedBatchCarrierV1::decode_v1(carrier)? {
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
        let prepared = PreparedBatchCarrierV1::decode_v1(carrier)?.ok_or("Batch storage rent carrier has another purpose")?;
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
