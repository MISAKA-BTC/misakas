//! Immutable accepted-claim lifecycle terms. Service clocks never rebase with DA/Panel clocks.
use super::*;

pub const PALW_BOND_BUDGET_STATE_VERSION_V4: u16 = 4;
pub const PALW_BUDGET_TABLE_CONSUMER_CONTRACT_V1: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidConsumerLifecyclePolicyV1 {
    pub version: u16,
    pub service_span_daa: u64,
    pub dispute_tail_daa: u64,
    pub material_tail_daa: u64,
}
impl PalwPaidConsumerLifecyclePolicyV1 {
    pub fn valid(&self) -> bool {
        self.version == 1
            && self.service_span_daa > 0
            && self.dispute_tail_daa > 0
            && self.material_tail_daa > 0
            && self.service_span_daa.checked_add(self.dispute_tail_daa).and_then(|v| v.checked_add(self.material_tail_daa)).is_some()
    }
    pub fn digest(&self) -> Hash64 {
        keyed64(b"misaka-palw/paid-consumer/lifecycle-policy/v1", &[&enc(self)])
    }
    pub fn terms(&self, accepted_daa: u64) -> Result<PalwPaidConsumerTermsV1, PalwBudgetRefusalV1> {
        if !self.valid() {
            return Err(PalwBudgetRefusalV1::Assignment("invalid finite consumer lifecycle policy"));
        }
        let service_until = accepted_daa.checked_add(self.service_span_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let backstop_until = service_until.checked_add(self.dispute_tail_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let material_until = backstop_until.checked_add(self.material_tail_daa).ok_or(PalwBudgetRefusalV1::Overflow)?;
        Ok(PalwPaidConsumerTermsV1 { accepted_daa, service_until, backstop_until, material_until, policy: self.digest() })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPaidConsumerLifecycleFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwPaidConsumerLifecyclePolicyV1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwPaidConsumerLifecycleMirrorV1 {
    pub from_daa: u64,
    pub policy: PalwPaidConsumerLifecyclePolicyV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidConsumerTermsV1 {
    pub accepted_daa: u64,
    pub service_until: u64,
    pub backstop_until: u64,
    /// Already authorized material term. Physical payment+W remains a separate obligation;
    /// this field does not authorize an unpaid queue to extend a provider's signed TTL.
    pub material_until: u64,
    pub policy: Hash64,
}
impl PalwPaidConsumerTermsV1 {
    pub fn valid(&self) -> bool {
        self.accepted_daa < self.service_until && self.service_until < self.backstop_until && self.backstop_until < self.material_until
    }
    pub fn digest_v1(&self, claim: Hash64) -> Hash64 {
        keyed64(b"misaka-palw/paid-consumer/accepted-contract/v1", &[claim.as_byte_slice(), &enc(self)])
    }
    pub fn admits_service(&self, now: u64, through: u64) -> bool {
        now >= self.accepted_daa && now <= self.service_until && through >= now && through <= self.service_until
    }
    pub fn admits_new_dispute(&self, opened: u64, through: u64) -> bool {
        opened >= self.accepted_daa && opened <= self.service_until && through >= opened && through <= self.backstop_until
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPaidConsumerStatusV1 {
    Live,
    Exhausted { daa: u64 },
    Terminal { daa: u64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPaidConsumerContractV1 {
    pub terms: PalwPaidConsumerTermsV1,
    pub status: PalwPaidConsumerStatusV1,
}
impl PalwPaidConsumerContractV1 {
    pub fn valid(&self) -> bool {
        self.terms.valid()
            && match self.status {
                PalwPaidConsumerStatusV1::Live => true,
                PalwPaidConsumerStatusV1::Exhausted { daa } => daa > self.terms.service_until,
                PalwPaidConsumerStatusV1::Terminal { daa } => daa >= self.terms.accepted_daa,
            }
    }
}

impl Params {
    pub fn palw_paid_consumer_lifecycle_active_at(&self, daa: u64) -> bool {
        self.palw_material_income_active_at(daa)
            && self
                .palw_paid_consumer_lifecycle_v1
                .as_ref()
                .is_some_and(|f| f.activation != ForkActivation::never() && f.activation.is_active(daa))
    }
    pub fn validate_palw_paid_consumer_lifecycle_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(f) = self.palw_paid_consumer_lifecycle_v1.as_ref().filter(|f| f.activation != ForkActivation::never()) else {
            return Ok(());
        };
        if !f.policy.valid() {
            return Err(PalwModeV2Error::Invalid("palw_paid_consumer_lifecycle_v1 policy is invalid"));
        }
        if !self.palw_material_income_v3.is_some_and(|at| at != ForkActivation::never() && at.daa_score() <= f.activation.daa_score())
            || !self
                .palw_bond_budget_v1
                .as_ref()
                .is_some_and(|b| b.activation != ForkActivation::never() && b.activation.daa_score() <= f.activation.daa_score())
        {
            return Err(PalwModeV2Error::Invalid("palw_paid_consumer_lifecycle_v1 requires budget/material schemas at or below it"));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_paid_consumer_lifecycle_v1 cannot be armed: complete consumer cost, provider consent and physical queue bounds are unapproved",
        ))
    }
}

impl PalwBondBudgetStateV1 {
    pub fn activate_consumer_contract_v4(&mut self, j: &mut Vec<PalwBudgetWriteV1>) {
        if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V4 {
            let mut header = self.header.clone();
            header.version = PALW_BOND_BUDGET_STATE_VERSION_V4;
            self.put_header(header, j);
        }
    }
    pub(super) fn consumer_index_replace_v1(
        &mut self,
        claim: Hash64,
        old: Option<&PalwPaidConsumerContractV1>,
        new: Option<&PalwPaidConsumerContractV1>,
    ) {
        if let Some(old) = old {
            match old.status {
                PalwPaidConsumerStatusV1::Live => {
                    self.consumer_service_due.remove(&(old.terms.service_until, claim));
                }
                PalwPaidConsumerStatusV1::Exhausted { .. } => {
                    self.consumer_exhausted.remove(&claim);
                }
                PalwPaidConsumerStatusV1::Terminal { .. } => {}
            }
        }
        if let Some(new) = new {
            match new.status {
                PalwPaidConsumerStatusV1::Live => {
                    self.consumer_service_due.insert((new.terms.service_until, claim));
                }
                PalwPaidConsumerStatusV1::Exhausted { .. } => {
                    self.consumer_exhausted.insert(claim);
                }
                PalwPaidConsumerStatusV1::Terminal { .. } => {}
            }
        }
    }
    pub fn put_consumer_contract_v1(
        &mut self,
        claim: Hash64,
        new: Option<PalwPaidConsumerContractV1>,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V4 || new.as_ref().is_some_and(|row| !row.valid()) {
            return Err(PalwBudgetRefusalV1::Assignment("consumer contract requires valid state v4"));
        }
        let old = self.consumer_contracts.get(&claim).copied();
        if let (Some(old), Some(new)) = (old, new)
            && (old.terms != new.terms
                || matches!(old.status, PalwPaidConsumerStatusV1::Terminal { .. }) && old != new
                || matches!(old.status, PalwPaidConsumerStatusV1::Exhausted { .. })
                    && matches!(new.status, PalwPaidConsumerStatusV1::Live))
        {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        self.consumer_index_replace_v1(claim, old.as_ref(), new.as_ref());
        put(&mut self.consumer_contracts, PALW_BUDGET_TABLE_CONSUMER_CONTRACT_V1, &claim, new, j);
        Ok(())
    }
    pub fn consumer_contract_v1(&self, claim: Hash64) -> Option<&PalwPaidConsumerContractV1> {
        self.consumer_contracts.get(&claim)
    }
    pub fn due_consumer_contracts_v1(&self, now: u64) -> Vec<Hash64> {
        self.consumer_service_due.range(..(now, Hash64::from_u64_word(0))).map(|(_, id)| *id).collect()
    }
    pub fn exhausted_consumer_contracts_v1(&self) -> impl Iterator<Item = &Hash64> {
        self.consumer_exhausted.iter()
    }
    pub(super) fn consumer_consistent_v1(&self) -> Result<(), &'static str> {
        if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V4 && !self.consumer_contracts.is_empty() {
            return Err("consumer contracts require state v4");
        }
        let mut due = BTreeSet::new();
        let mut exhausted = BTreeSet::new();
        for (id, row) in &self.consumer_contracts {
            if !row.valid() {
                return Err("invalid immutable consumer contract");
            }
            match row.status {
                PalwPaidConsumerStatusV1::Live => {
                    due.insert((row.terms.service_until, *id));
                }
                PalwPaidConsumerStatusV1::Exhausted { .. } => {
                    exhausted.insert(*id);
                }
                PalwPaidConsumerStatusV1::Terminal { .. } => {}
            }
        }
        if due != self.consumer_service_due || exhausted != self.consumer_exhausted {
            return Err("consumer lifecycle indexes differ");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> PalwPaidConsumerLifecyclePolicyV1 {
        PalwPaidConsumerLifecyclePolicyV1 { version: 1, service_span_daa: 60, dispute_tail_daa: 700, material_tail_daa: 500 }
    }
    fn monetary() -> PalwBondBudgetPolicyV1 {
        PalwBondBudgetPolicyV1 {
            version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
            window_daa: 280,
            capital_unit_sompi: 1_000,
            rho: 1,
            claims_per_unit: 2,
            block_units_per_unit: 2_000_000,
            reward_per_unit_sompi: 2_000,
            final_weight_per_unit: 1_000,
            max_open_claims_per_bond: 100,
            slice_rights_by_rho: false,
            round_rights: PalwRoundRightsPolicyV1::CountAgainstBlocks,
            // PESG §6 (integration; R6781-F merge): the interim liability slots ride along.
            liability_hold_daa: PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1,
            export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
            weight_value: PalwWeightValuePolicyV1::Unknown,
            hold: None,
        }
    }
    fn replay(state: &mut PalwBondBudgetStateV1, journal: &[PalwBudgetWriteV1], reverse: bool) {
        let order: Vec<_> = if reverse { journal.iter().rev().collect() } else { journal.iter().collect() };
        for entry in order {
            match entry {
                PalwBudgetWriteV1::Header { old, new } => {
                    let expected = if reverse { new } else { old };
                    assert_eq!(Some(&state.header), expected.as_ref());
                    state.header = if reverse { old } else { new }.clone().unwrap();
                }
                PalwBudgetWriteV1::Row { table, key, old, new } => state.apply_row(*table, key, old, new, reverse).unwrap(),
            }
        }
    }
    #[test]
    fn finite_contract_v4_preserves_three_old_wires_and_rebuilds_indexes_on_reorg() {
        for version in [1, 2, 3] {
            let mut state = PalwBondBudgetStateV1::new(10, &monetary());
            state.header.version = version;
            let mut expected = enc(&state.header);
            expected.extend(enc(&state.bonds));
            expected.extend(enc(&state.claims));
            expected.extend(enc(&state.releases));
            expected.extend(enc(&state.assignments));
            expected.extend(enc(&state.models));
            // The integration line's V1 wire carries PESG §6's liability releases after the five tables (R6781-F merge).
            expected.extend(enc(&state.liability_releases));
            if version >= 2 {
                expected.extend(enc(&state.reporter_income));
            }
            if version >= 3 {
                expected.extend(enc(&state.material_income));
            }
            assert_eq!(enc(&state), expected);
            assert_eq!(borsh::from_slice::<PalwBondBudgetStateV1>(&expected).unwrap(), state);
        }
        let mut old = PalwBondBudgetStateV1::new(10, &monetary());
        old.header.version = 3;
        let bytes = enc(&old);
        let roots = old.root_parts();
        let mut state = old.clone();
        let mut journal = Vec::new();
        state.activate_consumer_contract_v4(&mut journal);
        let id = Hash64::from_u64_word(7);
        let terms = policy().terms(101).unwrap();
        state
            .put_consumer_contract_v1(
                id,
                Some(PalwPaidConsumerContractV1 { terms, status: PalwPaidConsumerStatusV1::Live }),
                &mut journal,
            )
            .unwrap();
        assert!(state.due_consumer_contracts_v1(161).is_empty());
        assert_eq!(state.due_consumer_contracts_v1(162), vec![id]);
        let loaded: PalwBondBudgetStateV1 = borsh::from_slice(&enc(&state)).unwrap();
        assert_eq!(loaded, state);
        loaded.check_consistency().unwrap();
        let mut restored = old.clone();
        replay(&mut restored, &journal, false);
        assert_eq!(restored, state);
        replay(&mut restored, &journal, true);
        assert_eq!(restored, old);
        assert_eq!(enc(&restored), bytes);
        assert_eq!(restored.root_parts(), roots);
        assert_ne!(state.root_parts(), roots);
    }
    #[test]
    fn finite_contract_terms_and_terminal_clock_never_rebase_and_checked_bounds_refuse() {
        let terms = policy().terms(101).unwrap();
        assert!(terms.admits_service(161, 161));
        assert!(!terms.admits_service(162, 162));
        assert!(terms.admits_new_dispute(161, 861));
        assert!(!terms.admits_new_dispute(162, 861));
        assert!(!terms.admits_new_dispute(161, 862));
        assert!(policy().terms(u64::MAX - 1).is_err());
        let mut state = PalwBondBudgetStateV1::new(10, &monetary());
        let mut j = Vec::new();
        state.activate_consumer_contract_v4(&mut j);
        let id = Hash64::from_u64_word(7);
        let row = PalwPaidConsumerContractV1 { terms, status: PalwPaidConsumerStatusV1::Live };
        state.put_consumer_contract_v1(id, Some(row), &mut j).unwrap();
        let before = state.clone();
        let n = j.len();
        let mut forged = row;
        forged.terms.service_until += 1;
        assert!(state.put_consumer_contract_v1(id, Some(forged), &mut j).is_err());
        assert_eq!(state, before);
        assert_eq!(j.len(), n);
        let ended = PalwPaidConsumerContractV1 { status: PalwPaidConsumerStatusV1::Terminal { daa: 124 }, ..row };
        state.put_consumer_contract_v1(id, Some(ended), &mut j).unwrap();
        assert!(state.put_consumer_contract_v1(id, Some(row), &mut j).is_err());
        assert!(
            state
                .put_consumer_contract_v1(
                    id,
                    Some(PalwPaidConsumerContractV1 { status: PalwPaidConsumerStatusV1::Terminal { daa: 125 }, ..row }),
                    &mut j
                )
                .is_err()
        );
        state.check_consistency().unwrap();
    }
    #[test]
    fn finite_lifecycle_public_presets_none_and_all_arming_refused() {
        use crate::config::params::{
            DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params, palw_t12_shipped_params,
        };
        for params in
            [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_t12_shipped_params(), palw_rc_shipped_params()]
        {
            assert!(params.palw_paid_consumer_lifecycle_v1.is_none());
            assert!(!params.palw_paid_consumer_lifecycle_active_at(u64::MAX));
            params.validate_palw_paid_consumer_lifecycle_v1().unwrap();
            assert!(params.palw_fences_v1().iter().any(|(name, at)| *name == "palw_paid_consumer_lifecycle_v1" && at.is_none()));
            let mut named = params.clone();
            named.palw_paid_consumer_lifecycle_v1 =
                Some(PalwPaidConsumerLifecycleFenceV1 { activation: ForkActivation::new(900_000), policy: policy() });
            assert_ne!(named.consensus_params_id(), params.consensus_params_id());
            assert_ne!(named.consensus_schedule_id(), params.consensus_schedule_id());
            assert_eq!(named.consensus_identity_id(), params.consensus_identity_id());
            let mut repriced = named.clone();
            repriced.palw_paid_consumer_lifecycle_v1.as_mut().unwrap().policy.service_span_daa += 1;
            assert_ne!(named.consensus_params_id(), repriced.consensus_params_id());
            assert_ne!(named.consensus_schedule_id(), repriced.consensus_schedule_id());
        }
        let mut p = palw_t12_shipped_params();
        p.palw_paid_consumer_lifecycle_v1 =
            Some(PalwPaidConsumerLifecycleFenceV1 { activation: ForkActivation::new(100), policy: policy() });
        assert!(p.validate_palw_paid_consumer_lifecycle_v1().is_err());
        p.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1 { activation: ForkActivation::new(100), policy: monetary() });
        p.palw_reporter_income_v2 = Some(ForkActivation::new(100));
        p.palw_material_income_v3 = Some(ForkActivation::new(100));
        assert!(p.validate_palw_paid_consumer_lifecycle_v1().is_err(), "approved consent/cost/physical cash bounds are still absent");
        p.sync_palw_bond_budget_v1();
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            assert_eq!(bundle.state.paid_consumer_lifecycle().unwrap().policy, policy());
        } else {
            panic!("t12 native params");
        }
    }
}
