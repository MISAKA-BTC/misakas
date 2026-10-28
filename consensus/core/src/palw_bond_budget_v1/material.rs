//! Root-exact monetary/material provenance. This table creates no money or quota.
//! A queue receipt is retained until the validated physical payment plus W.
use super::*;

pub const PALW_BOND_BUDGET_STATE_VERSION_V3: u16 = 3;
pub const PALW_BUDGET_TABLE_MATERIAL_INCOME_V1: u8 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwMaterialPaymentV1 {
    Pending {
        income: Hash64,
    },
    Queued {
        income: Option<Hash64>,
        queue: Option<Hash64>,
        queued_daa: u64,
        paid_daa: Option<u64>,
        release_daa: Option<u64>,
    },
    /// Old cash has no authenticated physical clock; this is an uncertainty boundary.
    Historical {
        until: u64,
    },
    /// Authenticated old source claim/fee, but no saved physical payment DAA.
    UnprovenPhysicalPayment {
        until: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialIncomeV1 {
    /// None quarantines only claims already accepted before the material fence.
    pub root: Option<Hash64>,
    pub amount: u64,
    pub accepted_daa: u64,
    pub payment: PalwMaterialPaymentV1,
}

impl PalwMaterialIncomeV1 {
    pub fn income(&self) -> Option<Hash64> {
        match self.payment {
            PalwMaterialPaymentV1::Pending { income } => Some(income),
            PalwMaterialPaymentV1::Queued { income, .. } => income,
            _ => None,
        }
    }
    pub fn release(&self) -> Option<u64> {
        match self.payment {
            PalwMaterialPaymentV1::Queued { release_daa, .. } => release_daa,
            PalwMaterialPaymentV1::Historical { until } | PalwMaterialPaymentV1::UnprovenPhysicalPayment { until } => Some(until),
            _ => None,
        }
    }
    pub fn valid(&self) -> bool {
        match self.payment {
            PalwMaterialPaymentV1::Pending { .. } => self.amount > 0,
            PalwMaterialPaymentV1::Queued { income, queue, queued_daa, paid_daa, release_daa } => {
                self.amount > 0
                    && queued_daa >= self.accepted_daa
                    && (income.is_some() ^ queue.is_some())
                    && match (paid_daa, release_daa) {
                        (None, None) => true,
                        (Some(paid), Some(release)) => paid >= queued_daa && release > paid,
                        _ => false,
                    }
            }
            PalwMaterialPaymentV1::Historical { until } => self.root.is_none() && self.amount == 0 && until > self.accepted_daa,
            PalwMaterialPaymentV1::UnprovenPhysicalPayment { until } => {
                self.root.is_some() && self.amount > 0 && until > self.accepted_daa
            }
        }
    }
}

pub fn material_pending_id_v1(income: Hash64) -> Hash64 {
    keyed64(b"misaka-palw/material-income/pending/v1", &[income.as_byte_slice()])
}

/// Canonical exact original-entitlement link; a recipient's aggregate queue ID
/// alone cannot identify one source fee within a coalesced payment.
pub fn material_queue_receipt_id_v1(income: Hash64, queued: Hash64, now: u64) -> Hash64 {
    keyed64(
        b"misaka-palw/material-income/queue/v1",
        &[material_pending_id_v1(income).as_byte_slice(), queued.as_byte_slice(), &now.to_le_bytes()],
    )
}

impl PalwBondBudgetStateV1 {
    pub fn activate_material_income_v3(&mut self, j: &mut Vec<PalwBudgetWriteV1>) {
        if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V3 {
            let mut header = self.header.clone();
            header.version = PALW_BOND_BUDGET_STATE_VERSION_V3;
            self.put_header(header, j);
        }
    }

    pub(super) fn material_index_replace_v1(
        &mut self,
        id: Hash64,
        old: Option<&PalwMaterialIncomeV1>,
        new: Option<&PalwMaterialIncomeV1>,
    ) {
        if let Some(old) = old {
            if let Some(ids) = self.material_roots.get_mut(&old.root) {
                ids.remove(&id);
                if ids.is_empty() {
                    self.material_roots.remove(&old.root);
                }
            }
            if let Some(income) = old.income()
                && let Some(ids) = self.material_sources.get_mut(&income)
            {
                ids.remove(&id);
                if ids.is_empty() {
                    self.material_sources.remove(&income);
                }
            }
            if let Some(until) = old.release() {
                self.material_due.remove(&(until, id));
            }
        }
        if let Some(new) = new {
            self.material_roots.entry(new.root).or_default().insert(id);
            if let Some(income) = new.income() {
                self.material_sources.entry(income).or_default().insert(id);
            }
            if let Some(until) = new.release() {
                self.material_due.insert((until, id));
            }
        }
    }

    pub fn put_material_income_v1(
        &mut self,
        id: Hash64,
        row: Option<PalwMaterialIncomeV1>,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V3 || row.as_ref().is_some_and(|row| !row.valid()) {
            return Err(PalwBudgetRefusalV1::Assignment("material income requires a valid v3 record"));
        }
        let old = self.material_income.get(&id).copied();
        self.material_index_replace_v1(id, old.as_ref(), row.as_ref());
        put(&mut self.material_income, PALW_BUDGET_TABLE_MATERIAL_INCOME_V1, &id, row, j);
        Ok(())
    }

    /// Called only with an execution/court subject authenticated by its production consumer.
    /// Binding a sealed beacon subject or an address as an execution root is invalid.
    pub fn bind_material_income_v1(
        &mut self,
        root: Hash64,
        income: Hash64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let held = self
            .claim_row(&income)
            .copied()
            .filter(|row| {
                row.live
                    && matches!(
                        row.origin,
                        PalwBudgetOriginV1::NativeIncome
                            | PalwBudgetOriginV1::NativeIncomePartial
                            | PalwBudgetOriginV1::SourceFee
                            | PalwBudgetOriginV1::DeferredReporterIncome
                    )
            })
            .ok_or(PalwBudgetRefusalV1::NotOpen)?;
        let amount = if held.origin == PalwBudgetOriginV1::DeferredReporterIncome {
            held.reserved.reward_sompi.checked_sub(held.consumed.reward_sompi).ok_or(PalwBudgetRefusalV1::Overflow)?
        } else {
            held.reserved.reward_sompi
        };
        if amount == 0 {
            return Ok(());
        }
        let id = material_pending_id_v1(income);
        let row = PalwMaterialIncomeV1 {
            root: Some(root),
            amount,
            accepted_daa: held.accepted_daa,
            payment: PalwMaterialPaymentV1::Pending { income },
        };
        if let Some(old) = self.material_income.get(&id)
            && (old.root.is_some_and(|old| old != root)
                || old.amount > amount
                || old.accepted_daa != held.accepted_daa
                || old.payment != row.payment)
        {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        self.put_material_income_v1(id, Some(row), j)
    }

    /// Plan every provenance change before a monetary write can occur.
    pub(super) fn material_transfer_plan_v1(
        &self,
        income: Hash64,
        queued: Hash64,
        now: u64,
        amount: u64,
        finished: bool,
    ) -> Result<Vec<(Hash64, Option<PalwMaterialIncomeV1>)>, PalwBudgetRefusalV1> {
        let id = material_pending_id_v1(income);
        let Some(mut pending) = self.material_income.get(&id).copied() else { return Ok(Vec::new()) };
        if pending.payment != (PalwMaterialPaymentV1::Pending { income }) || now < pending.accepted_daa || amount > pending.amount {
            return Err(PalwBudgetRefusalV1::Assignment("material transfer differs from its funded pending subject"));
        }
        let key = material_queue_receipt_id_v1(income, queued, now);
        let old = self.material_income.get(&key).copied();
        let payment =
            PalwMaterialPaymentV1::Queued { income: Some(queued), queue: None, queued_daa: now, paid_daa: None, release_daa: None };
        if old.is_some_and(|old| old.root != pending.root || old.accepted_daa != pending.accepted_daa || old.payment != payment) {
            return Err(PalwBudgetRefusalV1::Duplicate);
        }
        let transferred = amount.checked_add(old.map_or(0, |row| row.amount)).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let queued = PalwMaterialIncomeV1 { root: pending.root, amount: transferred, accepted_daa: pending.accepted_daa, payment };
        pending.amount -= amount;
        Ok(vec![(id, (!finished && pending.amount > 0).then_some(pending)), (key, Some(queued))])
    }

    pub(super) fn apply_material_plan_v1(
        &mut self,
        plan: Vec<(Hash64, Option<PalwMaterialIncomeV1>)>,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) {
        // Plan rows are already valid, and the v3 table cannot downgrade during one operation.
        for (id, row) in plan {
            let old = self.material_income.get(&id).copied();
            self.material_index_replace_v1(id, old.as_ref(), row.as_ref());
            put(&mut self.material_income, PALW_BUDGET_TABLE_MATERIAL_INCOME_V1, &id, row, j);
        }
    }

    pub(super) fn cancel_material_pending_v1(&mut self, income: Hash64, j: &mut Vec<PalwBudgetWriteV1>) {
        self.apply_material_plan_v1(vec![(material_pending_id_v1(income), None)], j);
    }

    pub(super) fn material_payment_plan_v1(
        &self,
        income: Hash64,
        now: u64,
        w: u64,
    ) -> Result<Vec<(Hash64, Option<PalwMaterialIncomeV1>)>, PalwBudgetRefusalV1> {
        let until = now.checked_add(w).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let mut plan = Vec::new();
        for id in self.material_sources.get(&income).into_iter().flatten() {
            let mut row = self.material_income[id];
            if let PalwMaterialPaymentV1::Queued { queued_daa, paid_daa: None, income, queue, .. } = row.payment {
                if now < queued_daa {
                    return Err(PalwBudgetRefusalV1::Assignment("material payment predates its exact queue"));
                }
                row.payment =
                    PalwMaterialPaymentV1::Queued { queued_daa, income, queue, paid_daa: Some(now), release_daa: Some(until) };
                plan.push((*id, Some(row)));
            }
        }
        Ok(plan)
    }

    /// Old raw queues have no authenticated budget recipient/root. They keep only old claims,
    /// and only until that exact queue physically drains + W. No later queue can extend them.
    pub fn confirm_legacy_material_queue_v1(
        &mut self,
        queue: Hash64,
        now: u64,
        w: u64,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let until = now.checked_add(w).ok_or(PalwBudgetRefusalV1::Overflow)?;
        let ids: Vec<_> = self.material_roots.get(&None).into_iter().flatten().copied().collect();
        let mut plan = Vec::new();
        for id in ids {
            let mut row = self.material_income[&id];
            if let PalwMaterialPaymentV1::Queued { income: None, queue: Some(q), queued_daa, paid_daa: None, .. } = row.payment
                && q == queue
            {
                if now < queued_daa {
                    return Err(PalwBudgetRefusalV1::Assignment("legacy material queue paid before migration"));
                }
                row.payment = PalwMaterialPaymentV1::Queued {
                    income: None,
                    queue: Some(q),
                    queued_daa,
                    paid_daa: Some(now),
                    release_daa: Some(until),
                };
                plan.push((id, Some(row)));
            }
        }
        self.apply_material_plan_v1(plan, j);
        Ok(())
    }

    pub fn release_material_income_v1(&mut self, now: u64, j: &mut Vec<PalwBudgetWriteV1>) {
        let ids: Vec<_> = self.material_due.range(..=(now, Hash64::from_bytes([0xff; 64]))).map(|(_, id)| *id).collect();
        self.apply_material_plan_v1(ids.into_iter().map(|id| (id, None)).collect(), j);
    }

    /// None means no material hold; Some(MAX) means physical payment is still unknown.
    pub fn material_hold_until_v1(&self, root: Hash64, include_legacy: bool) -> Option<u64> {
        [Some(root), None]
            .into_iter()
            .filter(|key| key.is_some() || include_legacy)
            .filter_map(|key| self.material_roots.get(&key))
            .flat_map(|ids| ids.iter())
            .map(|id| self.material_income[id].release().unwrap_or(u64::MAX))
            .max()
    }

    pub(super) fn material_consistent_v1(&self) -> Result<(), &'static str> {
        if self.header.version < PALW_BOND_BUDGET_STATE_VERSION_V3 && !self.material_income.is_empty() {
            return Err("material table requires v3");
        }
        let mut roots = BTreeMap::<Option<Hash64>, BTreeSet<Hash64>>::new();
        let mut sources = BTreeMap::<Hash64, BTreeSet<Hash64>>::new();
        let mut due = BTreeSet::new();
        let mut queued_cash = BTreeMap::<Hash64, u64>::new();
        for (id, row) in &self.material_income {
            if !row.valid() {
                return Err("invalid material income provenance");
            }
            if let PalwMaterialPaymentV1::Pending { income } = row.payment {
                let held = self
                    .claims
                    .get(&income)
                    .filter(|held| {
                        held.live
                            && matches!(
                                held.origin,
                                PalwBudgetOriginV1::NativeIncome
                                    | PalwBudgetOriginV1::NativeIncomePartial
                                    | PalwBudgetOriginV1::SourceFee
                                    | PalwBudgetOriginV1::DeferredReporterIncome
                            )
                    })
                    .ok_or("material pending income has no live funded subject")?;
                let amount = if held.origin == PalwBudgetOriginV1::DeferredReporterIncome {
                    held.reserved.reward_sompi.checked_sub(held.consumed.reward_sompi).ok_or("material debt progress exceeds total")?
                } else {
                    held.reserved.reward_sompi
                };
                if row.amount != amount || row.accepted_daa != held.accepted_daa {
                    return Err("material pending amount or clock differs from funded subject");
                }
            }
            if let PalwMaterialPaymentV1::Queued { income: Some(income), paid_daa: None, .. } = row.payment {
                let held = self
                    .claims
                    .get(&income)
                    .filter(|held| held.live && held.in_window && held.origin == PalwBudgetOriginV1::NativeIncomeQueued)
                    .ok_or("unpaid material queue has no actual monetary receipt")?;
                let total = queued_cash.entry(income).or_default();
                *total = total.checked_add(row.amount).ok_or("material queue amount overflow")?;
                if *total > held.consumed.reward_sompi {
                    return Err("material queue amount exceeds its actual cash");
                }
            }
            roots.entry(row.root).or_default().insert(*id);
            if let Some(income) = row.income() {
                sources.entry(income).or_default().insert(*id);
            }
            if let Some(until) = row.release() {
                due.insert((until, *id));
            }
        }
        if roots != self.material_roots || sources != self.material_sources || due != self.material_due {
            return Err("material provenance indexes differ");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(i: u64) -> Hash64 {
        Hash64::from_u64_word(i)
    }
    fn bond(i: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Default::default(), i))
    }
    fn policy(w: u64) -> PalwBondBudgetPolicyV1 {
        PalwBondBudgetPolicyV1 {
            version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
            window_daa: w,
            capital_unit_sompi: 1_000,
            rho: 1,
            claims_per_unit: 10,
            block_units_per_unit: 4 * PALW_BUDGET_BLOCK_UNIT_V1,
            reward_per_unit_sompi: 50,
            final_weight_per_unit: 400,
            max_open_claims_per_bond: 1_000,
            slice_rights_by_rho: false,
            round_rights: PalwRoundRightsPolicyV1::CountAgainstBlocks,
            // PESG §6 (integration; R6781-F merge): the interim liability slots ride along.
            liability_hold_daa: PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1,
            export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
            weight_value: PalwWeightValuePolicyV1::Unknown,
            hold: None,
        }
    }
    fn replay(state: &mut PalwBondBudgetStateV1, writes: &[PalwBudgetWriteV1], reverse: bool) {
        let mut entries: Vec<_> = writes.iter().collect();
        if reverse {
            entries.reverse();
        }
        for entry in entries {
            match entry {
                PalwBudgetWriteV1::Header { old, new } => {
                    let (expected, next) = if reverse { (new, old) } else { (old, new) };
                    assert_eq!(expected.as_ref(), Some(&state.header));
                    state.header = next.clone().unwrap();
                }
                PalwBudgetWriteV1::Row { table, key, old, new } => state.apply_row(*table, key, old, new, reverse).unwrap(),
            }
        }
        state.check_consistency().unwrap();
    }

    #[test]
    fn material_fence_is_independent_dormant_hashed_and_requires_earlier_reporter_and_budget() {
        use crate::config::params::{
            DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params, palw_t12_shipped_params,
        };
        for params in
            [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, palw_rc_shipped_params(), palw_t12_shipped_params()]
        {
            assert!(params.palw_material_income_v3.is_none());
            assert!(!params.palw_material_income_active_at(u64::MAX));
            assert!(params.validate_palw_material_income_v3().is_ok());
            assert!(params.palw_fences_v1().iter().any(|(name, at)| *name == "palw_material_income_v3" && at.is_none()));
            let mut future = params.clone();
            future.palw_material_income_v3 = Some(ForkActivation::new(9_000_002));
            assert_ne!(params.consensus_params_id(), future.consensus_params_id());
            assert_ne!(params.consensus_schedule_id(), future.consensus_schedule_id());
            assert_eq!(params.consensus_identity_id(), future.consensus_identity_id());
            assert!(future.validate_palw_material_income_v3().is_err());
            future.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1::at(ForkActivation::new(9_000_000)));
            future.palw_reporter_income_v2 = Some(ForkActivation::new(9_000_003));
            let PalwModeV2Error::Invalid(why) = future.validate_palw_material_income_v3().unwrap_err() else { panic!("Invalid") };
            assert!(why.contains("at or below"));
            future.palw_reporter_income_v2 = Some(ForkActivation::new(9_000_001));
            let PalwModeV2Error::Invalid(why) = future.validate_palw_material_income_v3().unwrap_err() else { panic!("Invalid") };
            assert!(why.contains("cannot be armed"));
            future.sync_palw_bond_budget_v1();
            if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &future.palw_consensus_mode {
                assert!(!bundle.state.material_income_active_at(9_000_001));
                assert!(bundle.state.material_income_active_at(9_000_002));
            }
            let mut never = params.clone();
            never.palw_material_income_v3 = Some(ForkActivation::never());
            assert_eq!(params.consensus_identity_id(), never.consensus_identity_id());
            assert!(never.validate_palw_material_income_v3().is_ok());
        }
    }

    #[test]
    fn material_v3_keeps_v1_v2_wire_and_roots_and_rebuilds_indexes_on_carriage_and_reorg() {
        let p = policy(280);
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let v1 = borsh::to_vec(&s).unwrap();
        assert_eq!(
            v1,
            borsh::to_vec(&(
                s.header.clone(),
                s.bonds.clone(),
                s.claims.clone(),
                s.releases.clone(),
                s.assignments.clone(),
                s.models.clone(),
                // The integration line's V1 wire: PESG §6's liability releases follow the five tables (R6781-F merge).
                s.liability_releases.clone()
            ))
            .unwrap()
        );
        let mut j = Vec::new();
        s.activate_reporter_income_v2(&mut j);
        let v2 = borsh::to_vec(&s).unwrap();
        assert_eq!(
            v2,
            borsh::to_vec(&(
                s.header.clone(),
                s.bonds.clone(),
                s.claims.clone(),
                s.releases.clone(),
                s.assignments.clone(),
                s.models.clone(),
                s.liability_releases.clone(),
                s.reporter_income.clone()
            ))
            .unwrap()
        );
        let previous = s.clone();
        j.clear();
        s.activate_material_income_v3(&mut j);
        s.hold_native_income_v1(&p, 2_000, h(1), bond(1), 20, 300, 40, &[], true, &mut j).unwrap();
        s.bind_material_income_v1(h(101), h(1), &mut j).unwrap();
        s.pay_native_income_v1(&p, &h(1), 310, 40, h(2), true, &mut j).unwrap();
        s.confirm_native_income_payment_v1(&p, &h(2), 1_000, &mut j).unwrap();
        let loaded: PalwBondBudgetStateV1 = borsh::from_slice(&borsh::to_vec(&s).unwrap()).unwrap();
        assert_eq!(loaded, s);
        loaded.check_consistency().unwrap();
        assert_eq!(loaded.material_hold_until_v1(h(101), false), Some(1_280));
        let mut applied = previous.clone();
        replay(&mut applied, &j, false);
        assert_eq!(applied, s);
        replay(&mut applied, &j, true);
        assert_eq!(applied, previous);
        assert_eq!(borsh::to_vec(&applied).unwrap(), v2);
        assert_ne!(s.root_parts(), previous.root_parts());
    }

    #[test]
    fn root_exact_material_tracks_partial_coalesced_fd_fe_actual_payment_and_ignores_later_bond_income() {
        for w in [280, 436] {
            for (first, second) in [(h(10), h(11)), (h(11), h(10))] {
                let p = policy(w);
                let mut s = PalwBondBudgetStateV1::new(10, &p);
                let mut j = Vec::new();
                s.activate_material_income_v3(&mut j);
                for (income, root, amount) in [(h(1), h(101), 40), (h(2), h(102), 30)] {
                    s.hold_native_income_v1(&p, 2_000, income, bond(1), 20, 30, amount, &[], true, &mut j).unwrap();
                    s.bind_material_income_v1(root, income, &mut j).unwrap();
                }
                s.pay_native_income_v1(&p, &h(1), 40, 10, first, false, &mut j).unwrap();
                s.pay_native_income_v1(&p, &h(2), 40, 30, first, true, &mut j).unwrap();
                assert_eq!(s.claim_row(&first).unwrap().consumed.reward_sompi, 40, "one actual coalesced queue");
                assert_eq!(s.material_hold_until_v1(h(101), false), Some(u64::MAX));
                assert_eq!(s.material_hold_until_v1(h(102), false), Some(u64::MAX));
                s.confirm_native_income_payment_v1(&p, &first, 1_000, &mut j).unwrap();
                assert_eq!(s.material_hold_until_v1(h(102), false), Some(1_000 + w));
                s.release_due(1_000 + w, &mut j);
                s.release_material_income_v1(1_000 + w, &mut j);
                assert_eq!(s.material_hold_until_v1(h(102), false), None, "other roots of the same recipient release independently");
                assert_eq!(
                    s.material_hold_until_v1(h(101), false),
                    Some(u64::MAX),
                    "unpaid remainder still pins only its actual root"
                );
                s.hold_native_income_v1(&p, 2_000, h(3), bond(1), 1_000 + w, 2_000, 20, &[], true, &mut j).unwrap();
                s.bind_material_income_v1(h(103), h(3), &mut j).unwrap();
                s.pay_native_income_v1(&p, &h(1), 1_000 + w, 30, second, true, &mut j).unwrap();
                s.confirm_native_income_payment_v1(&p, &second, 3_000, &mut j).unwrap();
                s.release_due(3_000 + w - 1, &mut j);
                s.release_material_income_v1(3_000 + w - 1, &mut j);
                assert_eq!(s.material_hold_until_v1(h(101), false), Some(3_000 + w));
                s.release_due(3_000 + w, &mut j);
                s.release_material_income_v1(3_000 + w, &mut j);
                assert_eq!(s.material_hold_until_v1(h(101), false), None);
                assert_eq!(s.material_hold_until_v1(h(103), false), Some(u64::MAX), "later income is unrelated to the completed root");
                s.check_consistency().unwrap();
                assert_eq!(s.material_income.len(), 1, "paid material history is compacted, leaving only actual remaining debt");
            }
        }
    }

    #[test]
    fn post_final_cancel_and_reorg_preserve_already_queued_material_cash_and_exact_debt() {
        let p = policy(280);
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        s.activate_material_income_v3(&mut j);
        s.hold_native_income_v1(&p, 2_000, h(1), bond(1), 20, 30, 40, &[], true, &mut j).unwrap();
        s.bind_material_income_v1(h(101), h(1), &mut j).unwrap();
        s.pay_native_income_v1(&p, &h(1), 40, 10, h(10), false, &mut j).unwrap();
        let branch = s.clone();
        j.clear();
        s.cancel_native_income_v1(&h(1), &mut j).unwrap();
        assert_eq!(s.claim_row(&h(10)).unwrap().reserved.reward_sompi, 10, "conviction cannot burn or recreate queued cash");
        assert_eq!(s.material_income.len(), 1);
        assert_eq!(s.material_hold_until_v1(h(101), false), Some(u64::MAX));
        let before = (s.clone(), j.len());
        assert!(s.pay_native_income_v1(&p, &h(1), 50, 30, h(11), true, &mut j).is_err());
        assert_eq!((s.clone(), j.len()), before, "cancelled remaining money cannot revive");
        let cancelled = s.clone();
        replay(&mut s, &j, true);
        assert_eq!(s, branch);
        replay(&mut s, &j, false);
        assert_eq!(s, cancelled);
        s.confirm_native_income_payment_v1(&p, &h(10), 1_000, &mut j).unwrap();
        let loaded: PalwBondBudgetStateV1 = borsh::from_slice(&borsh::to_vec(&s).unwrap()).unwrap();
        assert_eq!(loaded, s);
        s.release_due(1_280, &mut j);
        s.release_material_income_v1(1_280, &mut j);
        assert_eq!(s.material_hold_until_v1(h(101), false), None);
        s.check_consistency().unwrap();
    }

    #[test]
    fn legacy_material_quarantine_freezes_exact_old_queue_generation_only() {
        let p = policy(436);
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        s.activate_material_income_v3(&mut j);
        s.put_material_income_v1(
            h(1),
            Some(PalwMaterialIncomeV1 {
                root: None,
                amount: 0,
                accepted_daa: 20,
                payment: PalwMaterialPaymentV1::Historical { until: 456 },
            }),
            &mut j,
        )
        .unwrap();
        s.put_material_income_v1(
            h(2),
            Some(PalwMaterialIncomeV1 {
                root: None,
                amount: 40,
                accepted_daa: 20,
                payment: PalwMaterialPaymentV1::Queued {
                    income: None,
                    queue: Some(h(10)),
                    queued_daa: 20,
                    paid_daa: None,
                    release_daa: None,
                },
            }),
            &mut j,
        )
        .unwrap();
        assert_eq!(s.material_hold_until_v1(h(101), true), Some(u64::MAX));
        assert_eq!(s.material_hold_until_v1(h(102), false), None, "new claims have no unidentified old duty");
        s.release_material_income_v1(900, &mut j);
        assert_eq!(s.material_hold_until_v1(h(101), true), Some(u64::MAX));
        s.confirm_legacy_material_queue_v1(h(10), 1_000, 436, &mut j).unwrap();
        s.hold_native_income_v1(&p, 2_000, h(3), bond(1), 1_001, 2_000, 20, &[], true, &mut j).unwrap();
        s.bind_material_income_v1(h(102), h(3), &mut j).unwrap();
        s.pay_native_income_v1(&p, &h(3), 1_002, 20, h(4), true, &mut j).unwrap();
        s.confirm_legacy_material_queue_v1(h(10), 1_200, 436, &mut j).unwrap();
        s.release_material_income_v1(1_436, &mut j);
        assert_eq!(
            s.material_hold_until_v1(h(101), true),
            None,
            "a later reuse of the old raw queue cannot move its old physical clock"
        );
        assert_eq!(s.material_hold_until_v1(h(102), false), Some(u64::MAX));
        s.check_consistency().unwrap();
    }

    #[test]
    fn terminal_reporter_debt_retains_material_for_each_physical_tranche_without_adding_quota() {
        let p = policy(280);
        let mut s = PalwBondBudgetStateV1::new(10, &p);
        let mut j = Vec::new();
        s.activate_material_income_v3(&mut j);
        // Explicit terminal-verdict seam. No pre-verdict court soundness is inferred.
        s.stage_reporter_income_v1(h(1), bond(1), 20, 125, &mut j).unwrap();
        s.bind_material_income_v1(h(101), h(1), &mut j).unwrap();
        for (i, amount) in [50, 50, 25].into_iter().enumerate() {
            let queue = h(10 + i as u64);
            let now = 30 + i as u64 * 1_000;
            assert_eq!(s.redeem_reporter_income_v1(&p, 1_000, &h(1), now, queue, u64::MAX, &mut j).unwrap(), amount);
            assert_eq!(s.material_hold_until_v1(h(101), false), Some(u64::MAX));
            s.confirm_native_income_payment_v1(&p, &queue, now + 1, &mut j).unwrap();
            s.release_due(now + 281, &mut j);
            s.release_material_income_v1(now + 281, &mut j);
            s.check_consistency().unwrap();
        }
        assert_eq!(s.material_hold_until_v1(h(101), false), None);
        assert!(s.material_income.is_empty());
        assert_eq!(s.claim_row(&h(1)).unwrap().consumed.reward_sompi, 125);
    }
}
