//! Batch storage rent is allocated from the accepted carrier's fee budget.
//! This projection is used alongside UTXO validation and the fee accumulator.
use super::{
    MATERIAL_BATCH_VERSION, MATERIAL_PIPELINED_MAX_BYTES, MaterialActionV1, SignedMaterialIntakeV1, material_intake_schedule_v1,
};
use crate::palw_court_carrier_fee_v1::CarrierFeeBudgetV1;
use crate::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2};
use crate::{Hash64, tx::Transaction};

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

/// Shared fee accounting for an enabled V3 route. Callers supply the fee
/// remaining after mandatory bond burn and all preceding fee allocations.
pub fn material_batch_carrier_fee_v1(carrier: &Transaction, available_fee: u64) -> Result<MaterialBatchCarrierFeeV1, String> {
    let rent = match batch_storage_declaration(carrier) {
        Some(intake) => material_batch_storage_rent_v1(&intake)?.unwrap_or(0),
        None => 0,
    };
    Ok(MaterialBatchCarrierFeeV1::split(rent, available_fee))
}

fn batch_storage_declaration(carrier: &Transaction) -> Option<Box<SignedMaterialIntakeV1>> {
    let decoded = crate::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(carrier));
    let mut objects = decoded.objects.into_iter();
    match objects.next()?.object {
        PalwConsensusObjectV2::MaterialIntakeV1 { intake } => Some(intake),
        _ => None,
    }
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
        if carrier.id() != budget.carrier_v1() || !carrier.id_is_current() {
            return Err("Batch storage rent budget belongs to another carrier".into());
        }
        let intake = batch_storage_declaration(carrier).ok_or("Batch storage rent carrier has another purpose")?;
        let amount = material_batch_storage_rent_v1(&intake)?.ok_or("Batch storage rent carrier is not a V3 Begin")?;
        let split = MaterialBatchCarrierFeeV1::split(amount, budget.remaining_v1());
        if !split.pays_storage_rent_v1() {
            return Err("Batch storage rent carrier has insufficient available fee".into());
        }
        let declaration = declaration_id(&intake, request_root)?;
        // The same physical carrier has only one allocation for this purpose,
        // even if a caller mistakenly supplies another request identity.
        let purpose =
            crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-storage-fee-purpose/v1", &carrier.id())?;
        budget.allocate_v1(purpose.as_bytes(), amount).map_err(|err| format!("Batch storage rent: {err:?}"))?;
        Ok(Self { at, declaration, amount })
    }

    pub fn amount_v1(&self) -> u64 {
        self.amount
    }

    pub(crate) fn check_v1(
        &self,
        intake: &SignedMaterialIntakeV1,
        request_root: Hash64,
        at: &PalwBlockContextV2,
        amount: u64,
    ) -> Result<(), String> {
        if self.at != *at || self.amount != amount || self.declaration != declaration_id(intake, request_root)? {
            return Err("Batch storage rent debit differs from the accepted declaration".into());
        }
        Ok(())
    }
}

fn declaration_id(intake: &SignedMaterialIntakeV1, request_root: Hash64) -> Result<Hash64, String> {
    if request_root == Hash64::default() {
        return Err("Batch storage rent lacks the accepted request root".into());
    }
    crate::palw_court_shared_tir_stream_v1::stream_digest(b"misaka-palw/batch-storage-declaration/v1", &(request_root, intake))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_court_shared_material_intake_v1::MaterialTargetV1;
    use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
    use crate::tx::{PopulatedTransaction, ScriptPublicKey, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};

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
        let output = 1_000;
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(5), 0), Vec::new(), 0, 0)],
            vec![TransactionOutput::new(output, ScriptPublicKey::default())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::MaterialIntakeV1 { intake: Box::new(intake.clone()) },
            })
            .unwrap(),
        );
        let budget = |fee| {
            CarrierFeeBudgetV1::from_populated_v1(&PopulatedTransaction::new(
                &tx,
                vec![UtxoEntry::new(output + fee, ScriptPublicKey::default(), 1, false)],
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
        let mut short = budget(amount);
        short.allocate_v1([12; 64], 1).unwrap();
        assert!(MaterialBatchStorageDebitV1::allocate_v1(&tx, &mut short, request, at).is_err());
        assert_eq!(short.remaining_v1(), amount - 1);
        let mut fees = budget(2 * amount + 1);
        fees.allocate_v1([12; 64], 1).unwrap();
        let paid = MaterialBatchStorageDebitV1::allocate_v1(&tx, &mut fees, request, at).unwrap();
        assert_eq!(paid.amount_v1(), amount);
        assert_eq!(fees.remaining_v1(), full_split.collectible_fee);
        assert!(MaterialBatchStorageDebitV1::allocate_v1(&tx, &mut fees, other_request, at).is_err());
        assert_eq!(fees.remaining_v1(), full_split.collectible_fee);
        assert!(paid.check_v1(&intake, request, &at, amount).is_ok());
        assert!(paid.check_v1(&intake, other_request, &at, amount).is_err());
        assert!(paid.check_v1(&intake, request, &PalwBlockContextV2 { daa_score: 8, ..at }, amount).is_err());
        intake.signature[0] ^= 1;
        assert!(paid.check_v1(&intake, request, &at, amount).is_err());
        intake.version = 1;
        assert_eq!(material_batch_storage_rent_v1(&intake), Ok(None));
    }
}
