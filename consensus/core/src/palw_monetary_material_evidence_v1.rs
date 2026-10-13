//! Explicit actual-cost evidence admission. A past profitable service does not
//! establish a future upper bound on sparse-chain payment or recovery time.
//! This API therefore never silently arms the public monetary material gate.
use crate::{
    Hash64,
    palw_monetary_material_v1::{PalwMonetaryMaterialPaymentObservationV1, PalwMonetaryMaterialPolicyV1, PalwMonetaryMaterialRowV1},
};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialCostEvidenceV1 {
    pub version: u16,
    pub consent: Hash64,
    pub material_manifest: Hash64,
    pub measurement_begin_daa: u64,
    pub measurement_end_daa: u64,
    pub retained_bytes: u64,
    pub storage_sompi: u64,
    pub network_sompi: u64,
    pub verification_sompi: u64,
    pub recovery_sompi: u64,
    pub court_sompi: u64,
    pub measurement_root: Hash64,
    pub release_block: crate::BlockHash,
    pub payments: Vec<PalwMonetaryMaterialPaymentObservationV1>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{palw_material_capacity_v1::*, palw_monetary_material_v1::*};
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn fixture() -> (PalwMonetaryMaterialPolicyV1, PalwMonetaryMaterialRowV1, PalwMonetaryMaterialCostEvidenceV1) {
        let policy = PalwMonetaryMaterialPolicyV1 {
            version: 1,
            max_contracts: 10,
            max_contracts_per_provider: 5,
            max_bytes_per_contract: 1000,
            max_bytes_per_provider: 10000,
            minimum_collateral_sompi: 10,
            minimum_fee_sompi: 10,
            window_daa: 436,
            response_window_daa: 10,
            max_enrollment_daa: 100,
            cost_report_root: h(1),
        };
        let manifest = PalwMaterialManifestV1 {
            root: h(2),
            scopes: vec![PalwMaterialScopeV1 {
                claim: h(2),
                commitment: h(3),
                codec: PalwMaterialCodecV1::NativeHeldComplete,
                bytes: 8,
                units: 1,
                content: h(4),
            }],
        };
        let paid = PalwMonetaryMaterialPaymentObservationV1 {
            root: h(2),
            income: h(6),
            queue: h(7),
            amount: 10,
            coinbase_queue_amount: 20,
            paid_block: h(8),
            paid_daa: 100,
            material_receipt_id: h(9),
            entitlement: Some(h(10)),
        };
        // A reporting seam with one retired fee entitlement. Model/native
        // authentication is delegated to the mandatory explicit callbacks.
        let row = PalwMonetaryMaterialRowV1 {
            terms: PalwMonetaryMaterialTermsV1 {
                version: 1,
                root: h(2),
                dependencies: Vec::new(),
                exec_plan_root: None,
                sponsor: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(11), 0)),
                nonce: h(12),
                policy: policy.id(),
                max_material_bytes: 1000,
                material_manifest: Some(manifest.clone()),
                window_daa: 436,
                enrollment_until_daa: 90,
                payment_relative_until_finality: true,
            },
            consent: h(13),
            sequence: 0,
            accepted_daa: 50,
            root_seen: true,
            providers: Vec::new(),
            challenges: Vec::new(),
            last_tail_until: Some(536),
            tail_block: Some(h(14)),
            tail_blue_score: Some(1000),
            last_payment: Some(paid.clone()),
            payment_history_root: h(15),
            service_fee_count: 1,
            service_fee_total_sompi: 10,
            service_fee_paid_sompi: 10,
            service_fee_history_root: monetary_service_fee_history_next_v1(Hash64::default(), h(10), 10),
            queued_entitlements: Default::default(),
        };
        let evidence = PalwMonetaryMaterialCostEvidenceV1 {
            version: 1,
            consent: row.consent,
            material_manifest: manifest.id(),
            measurement_begin_daa: 50,
            measurement_end_daa: 600,
            retained_bytes: manifest.validate(1000).unwrap(),
            storage_sompi: 3,
            network_sompi: 2,
            verification_sompi: 1,
            recovery_sompi: 1,
            court_sompi: 1,
            measurement_root: h(16),
            release_block: h(17),
            payments: vec![paid],
        };
        (policy, row, evidence)
    }
    #[test]
    fn economic_evidence_needs_every_native_and_review_authority_and_never_invents_a_future_time_bound() {
        let (policy, row, evidence) = fixture();
        assert!(
            assess_monetary_material_cost_v1(
                &policy,
                &row,
                &evidence,
                |_| Err("row proof missing".into()),
                |_| Ok(()),
                |_, _| Ok(()),
                |_, _, _, _| Ok(true)
            )
            .is_err()
        );
        assert!(
            assess_monetary_material_cost_v1(
                &policy,
                &row,
                &evidence,
                |_| Ok(()),
                |_| Err("body/output proof missing".into()),
                |_, _| Ok(()),
                |_, _, _, _| Ok(true)
            )
            .is_err()
        );
        assert!(
            assess_monetary_material_cost_v1(
                &policy,
                &row,
                &evidence,
                |_| Ok(()),
                |_| Ok(()),
                |_, _| Err("cost review missing".into()),
                |_, _, _, _| Ok(true)
            )
            .is_err()
        );
        let assessed =
            assess_monetary_material_cost_v1(&policy, &row, &evidence, |_| Ok(()), |_| Ok(()), |_, _| Ok(()), |_, _, _, _| Ok(true))
                .unwrap();
        assert_eq!(assessed.actual_fee_income_sompi, 10);
        assert_eq!(assessed.measured_surplus_sompi, Some(2));
        assert_eq!(assessed.future_bound, PalwMonetaryMaterialFutureBoundV1::PaymentAndRecoveryTimeUnknown);
        let mut wrong = evidence.clone();
        wrong.payments[0].entitlement = Some(h(18));
        assert_eq!(
            assess_monetary_material_cost_v1(&policy, &row, &wrong, |_| Ok(()), |_| Ok(()), |_, _| Ok(()), |_, _, _, _| Ok(true)),
            Err(PalwMonetaryMaterialEvidenceErrorV1::Payment)
        );
    }
}
impl PalwMonetaryMaterialCostEvidenceV1 {
    pub fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"MISAKA/PALW/MONETARY-MATERIAL-COST-EVIDENCE/V1", self))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwMonetaryMaterialFutureBoundV1 {
    /// No liveness theorem bounds staging/inclusion/recovery on sparse DAA.
    PaymentAndRecoveryTimeUnknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMonetaryMaterialAssessmentV1 {
    pub evidence: Hash64,
    pub actual_fee_income_sompi: u128,
    pub measured_cost_sompi: u128,
    pub measured_surplus_sompi: Option<u128>,
    pub wire_bytes: u64,
    pub completed_release: crate::BlockHash,
    pub future_bound: PalwMonetaryMaterialFutureBoundV1,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwMonetaryMaterialEvidenceErrorV1 {
    #[error("cost evidence is outside the signed consent/manifest/measurement scope")]
    Scope,
    #[error("cost evidence omits, duplicates or substitutes an actual service-fee payment")]
    Payment,
    #[error("material cost/capacity arithmetic overflowed")]
    Capacity,
    #[error("native physical payment or finalized release authority is unavailable: {0}")]
    Native(String),
    #[error("exact cost report review authority is unavailable: {0}")]
    Review(String),
}

/// Callbacks must verify actual canonical native payment bodies/outputs and
/// the exact reviewed external measurement provenance respectively. Ledger
/// promisePaid, a plain report hash or a merely proposed release is insufficient.
pub fn assess_monetary_material_cost_v1(
    policy: &PalwMonetaryMaterialPolicyV1,
    row: &PalwMonetaryMaterialRowV1,
    evidence: &PalwMonetaryMaterialCostEvidenceV1,
    mut native_contract_row: impl FnMut(&PalwMonetaryMaterialRowV1) -> Result<(), String>,
    mut native_payment: impl FnMut(&PalwMonetaryMaterialPaymentObservationV1) -> Result<(), String>,
    mut reviewed_measurement: impl FnMut(Hash64, &PalwMonetaryMaterialCostEvidenceV1) -> Result<(), String>,
    mut finalized_release: impl FnMut(Hash64, Hash64, Hash64, crate::BlockHash) -> Result<bool, String>,
) -> Result<PalwMonetaryMaterialAssessmentV1, PalwMonetaryMaterialEvidenceErrorV1> {
    use PalwMonetaryMaterialEvidenceErrorV1 as E;
    // The caller must pin the entire accepted native row (including retired fee
    // history), not merely an otherwise valid consent/absence or report hash.
    native_contract_row(row).map_err(E::Native)?;
    policy.validate().map_err(|_| E::Scope)?;
    let manifest = row.terms.material_manifest.as_ref().ok_or(E::Scope)?;
    let wire_bytes = manifest.validate(row.terms.max_material_bytes).map_err(|_| E::Capacity)?;
    let tail = row.tail_block.ok_or(E::Scope)?;
    if evidence.version != 1
        || evidence.consent != row.consent
        || row.terms.policy != policy.id()
        || evidence.material_manifest != manifest.id()
        || evidence.retained_bytes != wire_bytes
        || evidence.measurement_begin_daa > row.accepted_daa
        || evidence.measurement_end_daa <= evidence.measurement_begin_daa
        || evidence.measurement_root == Hash64::default()
        || evidence.release_block == Hash64::default()
        || row.last_tail_until.is_some_and(|tail| evidence.measurement_end_daa < tail)
        || evidence.payments.len() > 8192
        || evidence.payments.len() as u64 != row.service_fee_count
        || row.service_fee_paid_sompi != row.service_fee_total_sompi
    {
        return Err(E::Scope);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut income = 0u128;
    let mut history = Hash64::default();
    for payment in &evidence.payments {
        let entitlement = payment.entitlement.ok_or(E::Payment)?;
        if !seen.insert(entitlement)
            || payment.root != row.terms.root
            || payment.paid_daa < evidence.measurement_begin_daa
            || payment.paid_daa > evidence.measurement_end_daa
            || payment.amount == 0
            || payment.coinbase_queue_amount < payment.amount
            || payment.paid_block == Hash64::default()
        {
            return Err(E::Payment);
        }
        history = crate::palw_monetary_material_v1::monetary_service_fee_history_next_v1(history, entitlement, payment.amount);
        native_payment(payment).map_err(E::Native)?;
        income = income.checked_add(u128::from(payment.amount)).ok_or(E::Capacity)?;
    }
    if income != row.service_fee_total_sompi || history != row.service_fee_history_root {
        return Err(E::Payment);
    }
    reviewed_measurement(policy.cost_report_root, evidence).map_err(E::Review)?;
    if !finalized_release(row.terms.root, row.consent, tail, evidence.release_block).map_err(E::Native)? {
        return Err(E::Native("release is not finalized".into()));
    }
    let cost =
        [evidence.storage_sompi, evidence.network_sompi, evidence.verification_sompi, evidence.recovery_sompi, evidence.court_sompi]
            .into_iter()
            .try_fold(0u128, |sum, part| sum.checked_add(u128::from(part)))
            .ok_or(E::Capacity)?;
    Ok(PalwMonetaryMaterialAssessmentV1 {
        evidence: evidence.id(),
        actual_fee_income_sompi: income,
        measured_cost_sompi: cost,
        measured_surplus_sompi: income.checked_sub(cost),
        wire_bytes,
        completed_release: evidence.release_block,
        future_bound: PalwMonetaryMaterialFutureBoundV1::PaymentAndRecoveryTimeUnknown,
    })
}
