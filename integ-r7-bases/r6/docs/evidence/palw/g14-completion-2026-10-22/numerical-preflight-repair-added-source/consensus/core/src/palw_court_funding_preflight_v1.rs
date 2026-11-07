//! Transaction-bound pre-UTXO funding preflight, RFC-0035 A candidate.
//!
//! This strengthens local funding preparation with exact parent/transaction/entry
//! binding and one actual fee budget. It is NOT an input authorization or a
//! NativeTransitionPlan accepted by the node. The coordinator must independently
//! resolve the selected-parent state, full graph, obligations, actor authority and
//! accepted transaction order. Caller scope/checkpoint/obligation DTOs cannot
//! establish those facts. No installer, rooted row, script bypass or success path.
use crate::{
    Hash64,
    palw_court_carrier_fee_v1::{CarrierFeeBudgetV1, CarrierFeeRefusalV1},
    palw_court_original_escrow_v1::{
        FundingConsentV1, FundingRefusalV1, OriginalFundingScopeV1, PreparedEscrowFundingV1, inspect_transaction_v1,
        prepare_funding_v1,
    },
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2},
    tx::PopulatedTransaction,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundingCheckpointV1 {
    pub selected_parent: Hash64,
    pub parent_state_root: Hash64,
    pub accepting_point: PalwBlockContextV2,
}
/// Requirements, not payments, prices or an assertion of completeness. Native
/// policy derivation remains external; zero cannot stand for an unknown cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundingCarrierObligationsV1 {
    pub released_bond_burn: u64,
    pub lifecycle_rent: u64,
    pub declared_tir_rent: u64,
    pub court_carriage_fee: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FundingPreflightRefusalV1 {
    Checkpoint,
    Funding(FundingRefusalV1),
    Fee(CarrierFeeRefusalV1),
    Binding,
    Serialization(String),
}
fn digest<T: borsh::BorshSerialize>(domain: &[u8], value: &T) -> Result<Hash64, FundingPreflightRefusalV1> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(domain, value).map_err(FundingPreflightRefusalV1::Serialization)
}
fn exact_transaction(tx: &PopulatedTransaction<'_>) -> Result<Hash64, FundingPreflightRefusalV1> {
    // Borsh includes input signature scripts/sigop counts and storage mass, all
    // excluded from txid. Entries include amount, script, DAA and coinbase flag.
    digest(b"misaka-palw/funding-full-transaction/v1", &(tx.tx, &tx.entries))
}
fn checkpoint(checkpoint: FundingCheckpointV1, state: &PalwChainStateV2) -> Result<(), FundingPreflightRefusalV1> {
    if checkpoint.selected_parent == Hash64::default()
        || checkpoint.accepting_point.block == Hash64::default()
        || checkpoint.accepting_point.block == checkpoint.selected_parent
        || checkpoint.parent_state_root != state.state_root()
    {
        return Err(FundingPreflightRefusalV1::Checkpoint);
    }
    Ok(())
}
/// Immutable borrowed local preflight; not Clone/Borsh or spend authorization.
/// Keeping the populated input entries binds the exact composed-view facts used
/// by the caller; it does not certify their actual existence or script validity.
pub struct TransactionBoundFundingPreflightV1<'a> {
    populated: PopulatedTransaction<'a>,
    funding: PreparedEscrowFundingV1<'a>,
    checkpoint: FundingCheckpointV1,
    exact_transaction: Hash64,
    requirements: FundingCarrierObligationsV1,
    expected_scope: OriginalFundingScopeV1,
    network: Hash64,
    actual_fee: u64,
    remaining_fee: u64,
}
impl TransactionBoundFundingPreflightV1<'_> {
    pub fn funding_v1(&self) -> &PreparedEscrowFundingV1<'_> {
        &self.funding
    }
    pub fn actual_fee_v1(&self) -> u64 {
        self.actual_fee
    }
    pub fn remaining_fee_v1(&self) -> u64 {
        self.remaining_fee
    }
    pub fn checkpoint_v1(&self) -> FundingCheckpointV1 {
        self.checkpoint
    }
    pub fn populated_v1(&self) -> &PopulatedTransaction<'_> {
        &self.populated
    }
    /// Recheck at the actual native validation seam. This does not turn the
    /// result into a capability to skip ordinary input validation or a fee receipt.
    pub fn check_binding_v1(
        &self,
        actual: &PopulatedTransaction<'_>,
        state: &PalwChainStateV2,
        at: FundingCheckpointV1,
        requirements: FundingCarrierObligationsV1,
        expected_scope: &OriginalFundingScopeV1,
        network: Hash64,
    ) -> Result<(), FundingPreflightRefusalV1> {
        checkpoint(at, state)?;
        if at != self.checkpoint
            || requirements != self.requirements
            || expected_scope != &self.expected_scope
            || network != self.network
            || exact_transaction(actual)? != self.exact_transaction
        {
            return Err(FundingPreflightRefusalV1::Binding);
        }
        // Reuse native checked conservation rather than trust a matching raw txid.
        let fee = CarrierFeeBudgetV1::from_populated_v1(actual).map_err(FundingPreflightRefusalV1::Fee)?;
        if fee.actual_fee_v1() != self.actual_fee {
            return Err(FundingPreflightRefusalV1::Binding);
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn preflight_funding_transaction_v1<'a>(
    populated: PopulatedTransaction<'a>,
    consent: &'a FundingConsentV1,
    payer: &'a [u8],
    expected: &OriginalFundingScopeV1,
    network: Hash64,
    state: &PalwChainStateV2,
    at: FundingCheckpointV1,
    requirements: FundingCarrierObligationsV1,
) -> Result<TransactionBoundFundingPreflightV1<'a>, FundingPreflightRefusalV1> {
    checkpoint(at, state)?;
    let mut fee = CarrierFeeBudgetV1::from_populated_v1(&populated).map_err(FundingPreflightRefusalV1::Fee)?;
    let actual =
        inspect_transaction_v1(populated.tx, consent.terms.funding_output_index).map_err(FundingPreflightRefusalV1::Funding)?;
    let funding = prepare_funding_v1(actual, consent, payer, expected, network, at.accepting_point.daa_score)
        .map_err(FundingPreflightRefusalV1::Funding)?;
    let exact_transaction = exact_transaction(&populated)?;
    // Fixed purpose identities, no caller-chosen aliases that could spend one
    // actual fee twice. Escrow principal is already an actual output, never fee.
    for (purpose, amount) in
        [requirements.released_bond_burn, requirements.lifecycle_rent, requirements.declared_tir_rent, requirements.court_carriage_fee]
            .into_iter()
            .enumerate()
    {
        let id = digest(b"misaka-palw/funding-fee-purpose/v1", &(consent.terms.id_v1(), exact_transaction, purpose as u8))?;
        fee.allocate_v1(id.as_bytes(), amount).map_err(FundingPreflightRefusalV1::Fee)?;
    }
    Ok(TransactionBoundFundingPreflightV1 {
        populated,
        funding,
        checkpoint: at,
        exact_transaction,
        requirements,
        expected_scope: expected.clone(),
        network,
        actual_fee: fee.actual_fee_v1(),
        remaining_fee: fee.remaining_v1(),
    })
}

impl std::fmt::Debug for TransactionBoundFundingPreflightV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionBoundFundingPreflightV1")
            .field("checkpoint", &self.checkpoint)
            .field("exact_transaction", &self.exact_transaction)
            .field("actual_fee", &self.actual_fee)
            .field("remaining_fee", &self.remaining_fee)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_court_original_escrow_v1::{COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1, FundingTermsV1};
    use crate::tx::{ScriptPublicKey, UtxoEntry};
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    /// Synthetic contract identities/amounts test structure only. Real keys and
    /// actual Transaction outputs are used; no UTXO admission or fee result is
    /// claimed, and these amounts are not a production price recommendation.
    fn fixture() -> (Vec<u8>, FundingConsentV1, crate::tx::Transaction, OriginalFundingScopeV1) {
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
            refund_payload: payer,
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
        let tx = crate::tx::Transaction::new(
            0,
            vec![crate::tx::TransactionInput::new(crate::tx::TransactionOutpoint::new(h(77), 0), Vec::new(), 0, 0)],
            vec![crate::tx::TransactionOutput::new(150, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&payer.as_bytes()))],
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

    fn at(state: &PalwChainStateV2) -> FundingCheckpointV1 {
        FundingCheckpointV1 {
            selected_parent: h(90),
            parent_state_root: state.state_root(),
            accepting_point: PalwBlockContextV2 { block: h(91), daa_score: 15, blue_score: 16, subsidy: 0 },
        }
    }
    fn requirements() -> FundingCarrierObligationsV1 {
        FundingCarrierObligationsV1 { released_bond_burn: 7, lifecycle_rent: 11, declared_tir_rent: 5, court_carriage_fee: 7 }
    }
    fn entries() -> Vec<UtxoEntry> {
        vec![UtxoEntry::new(180, ScriptPublicKey::default(), 2, false)]
    }
    #[test]
    fn actual_principal_is_not_fee_and_all_obligations_share_one_checked_budget() {
        let (public, consent, tx, scope) = fixture();
        let state = PalwChainStateV2::genesis();
        let plan = preflight_funding_transaction_v1(
            PopulatedTransaction::new(&tx, entries()),
            &consent,
            &public,
            &scope,
            h(1),
            &state,
            at(&state),
            requirements(),
        )
        .unwrap();
        assert_eq!((plan.actual_fee_v1(), plan.remaining_fee_v1()), (30, 0));
        assert_eq!(plan.funding_v1().actual().output().value, 150);
        plan.check_binding_v1(&PopulatedTransaction::new(&tx, entries()), &state, at(&state), requirements(), &scope, h(1)).unwrap();
        let mut excess = requirements();
        excess.court_carriage_fee += 1;
        assert!(matches!(
            preflight_funding_transaction_v1(
                PopulatedTransaction::new(&tx, entries()),
                &consent,
                &public,
                &scope,
                h(1),
                &state,
                at(&state),
                excess
            ),
            Err(FundingPreflightRefusalV1::Fee(CarrierFeeRefusalV1::InsufficientFee))
        ));
        // The synthetic entries are deliberately not represented as a script-validated UTXO receipt.
        assert_eq!(tx.outputs[0].value, 150);
    }
    #[test]
    fn full_transaction_and_every_entry_fact_are_bound_beyond_the_raw_txid() {
        let (public, consent, tx, scope) = fixture();
        let state = PalwChainStateV2::genesis();
        let plan = preflight_funding_transaction_v1(
            PopulatedTransaction::new(&tx, entries()),
            &consent,
            &public,
            &scope,
            h(1),
            &state,
            at(&state),
            requirements(),
        )
        .unwrap();
        for field in 0..4 {
            let mut altered = entries();
            match field {
                0 => altered[0].amount += 1,
                1 => altered[0].block_daa_score += 1,
                2 => altered[0].is_coinbase = true,
                _ => altered[0].script_public_key = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&h(92).as_bytes()),
            }
            assert_eq!(
                plan.check_binding_v1(&PopulatedTransaction::new(&tx, altered), &state, at(&state), requirements(), &scope, h(1)),
                Err(FundingPreflightRefusalV1::Binding)
            );
        }
        for field in 0..3 {
            let mut altered = tx.clone();
            match field {
                0 => altered.inputs[0].signature_script.push(1),
                1 => altered.inputs[0].sig_op_count += 1,
                _ => altered.set_mass(1),
            }
            assert_eq!(altered.id(), tx.id(), "raw identity alone does not bind this field");
            assert!(altered.id_is_current());
            assert_eq!(
                plan.check_binding_v1(
                    &PopulatedTransaction::new(&altered, entries()),
                    &state,
                    at(&state),
                    requirements(),
                    &scope,
                    h(1)
                ),
                Err(FundingPreflightRefusalV1::Binding)
            );
        }
        for field in 0..3 {
            let mut changed_scope = scope.clone();
            match field {
                0 => changed_scope.required_amount_sompi += 1,
                1 => changed_scope.required_material_until_daa += 1,
                _ => changed_scope.fee_profile_id = h(97),
            }
            assert_eq!(
                plan.check_binding_v1(
                    &PopulatedTransaction::new(&tx, entries()),
                    &state,
                    at(&state),
                    requirements(),
                    &changed_scope,
                    h(1)
                ),
                Err(FundingPreflightRefusalV1::Binding)
            );
        }
        assert_eq!(
            plan.check_binding_v1(&PopulatedTransaction::new(&tx, entries()), &state, at(&state), requirements(), &scope, h(98)),
            Err(FundingPreflightRefusalV1::Binding)
        );
        let mut altered_requirements = requirements();
        altered_requirements.released_bond_burn += 1;
        assert_eq!(
            plan.check_binding_v1(&PopulatedTransaction::new(&tx, entries()), &state, at(&state), altered_requirements, &scope, h(1)),
            Err(FundingPreflightRefusalV1::Binding)
        );
        for field in 0..3 {
            let mut changed = at(&state);
            match field {
                0 => changed.selected_parent = h(93),
                1 => changed.accepting_point.daa_score += 1,
                _ => changed.accepting_point.block = h(94),
            }
            assert_eq!(
                plan.check_binding_v1(&PopulatedTransaction::new(&tx, entries()), &state, changed, requirements(), &scope, h(1)),
                Err(FundingPreflightRefusalV1::Binding)
            );
        }
    }
    #[test]
    fn mismatched_root_scope_signature_and_input_conservation_never_make_a_plan() {
        let (public, consent, tx, scope) = fixture();
        let state = PalwChainStateV2::genesis();
        let mut bad = at(&state);
        bad.parent_state_root = h(95);
        assert!(matches!(
            preflight_funding_transaction_v1(
                PopulatedTransaction::new(&tx, entries()),
                &consent,
                &public,
                &scope,
                h(1),
                &state,
                bad,
                requirements()
            ),
            Err(FundingPreflightRefusalV1::Checkpoint)
        ));
        assert!(matches!(
            preflight_funding_transaction_v1(
                PopulatedTransaction { tx: &tx, entries: Vec::new() },
                &consent,
                &public,
                &scope,
                h(1),
                &state,
                at(&state),
                requirements()
            ),
            Err(FundingPreflightRefusalV1::Fee(CarrierFeeRefusalV1::MissingEntries))
        ));
        let mut bad_scope = scope.clone();
        bad_scope.relative_graph_id = h(96);
        assert!(matches!(
            preflight_funding_transaction_v1(
                PopulatedTransaction::new(&tx, entries()),
                &consent,
                &public,
                &bad_scope,
                h(1),
                &state,
                at(&state),
                requirements()
            ),
            Err(FundingPreflightRefusalV1::Funding(FundingRefusalV1::Scope))
        ));
        let mut bad_consent = consent.clone();
        bad_consent.signature[0] ^= 1;
        assert!(matches!(
            preflight_funding_transaction_v1(
                PopulatedTransaction::new(&tx, entries()),
                &bad_consent,
                &public,
                &scope,
                h(1),
                &state,
                at(&state),
                requirements()
            ),
            Err(FundingPreflightRefusalV1::Funding(FundingRefusalV1::Signature))
        ));
        let mut insufficient = entries();
        insufficient[0].amount = 149;
        assert!(matches!(
            preflight_funding_transaction_v1(
                PopulatedTransaction::new(&tx, insufficient),
                &consent,
                &public,
                &scope,
                h(1),
                &state,
                at(&state),
                requirements()
            ),
            Err(FundingPreflightRefusalV1::Fee(CarrierFeeRefusalV1::OutputsExceedInputs))
        ));
    }
}
