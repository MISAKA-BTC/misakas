//! Native consumers and retirement read the same root-exact payment provenance.
use super::*;
fn keyed64(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut hash = keyed(domain);
    for part in parts {
        hash.update(part);
    }
    finish(hash)
}

impl PalwChainStateV2 {
    pub fn material_monetary_ledger_v1(&self) -> Option<&PalwBondBudgetStateV1> {
        self.bond_budget.as_ref()
    }
    /// Last material release for a native root and its authenticated EXEC dependencies.
    /// MAX means an actual funded right/queue still has no physical payment clock.
    /// This does not extend any signed kernel/provider term; admission must separately
    /// obtain consent/funding for a sufficient finite term.
    pub fn material_income_hold_until_v1(&self, params: &PalwStateParamsV2, root: Hash64) -> Option<u64> {
        self.material_income_hold_without_covenant_v1(params, root).max(self.monetary_material_hold_v1(root))
    }
    pub fn material_income_hold_without_covenant_v1(&self, params: &PalwStateParamsV2, root: Hash64) -> Option<u64> {
        let book = self.bond_budget.as_ref()?;
        if params.material_income_from().is_none() || book.header.version < PALW_BOND_BUDGET_STATE_VERSION_V3 {
            return None;
        }
        let old = self.claims.get(&root).is_some_and(|claim| claim.accepted_daa < params.material_income_from().unwrap_or(u64::MAX));
        let mut until =
            book.material_hold_until_v1(root, old).max(self.paid_consumer_terms_v1(&root).map(|terms| terms.material_until));
        if let Some(work) = self.exec_v2.roots.get(&root) {
            for claim in std::iter::once(work.prefix_claim)
                .chain(self.exec_v2.slices.range((root, 0)..=(root, u32::MAX)).map(|(_, slice)| slice.evidence_root))
            {
                until = until.max(book.material_hold_until_v1(claim, false));
            }
        }
        // A kernel dependency can also be queried directly by its material provider.
        // Its original fee hold may have moved into the native composite allocation.
        for (native, work) in &self.exec_v2.roots {
            if work.prefix_claim == root
                || self.exec_v2.slices.range((*native, 0)..=(*native, u32::MAX)).any(|(_, slice)| slice.evidence_root == root)
            {
                until = until
                    .max(book.material_hold_until_v1(*native, false))
                    .max(self.paid_consumer_terms_v1(native).map(|terms| terms.material_until));
            }
        }
        until
    }
}

impl TransitionBuilder<'_> {
    /// `root` is the authenticated execution claim ID, not the source seal/job subject,
    /// payout payload, recipient bond or consumer beacon context. Bind before moving money.
    pub(in crate::palw_state_v2) fn bind_material_income_root_v1(
        &mut self,
        now: u64,
        root: Hash64,
        income: Hash64,
    ) -> Result<(), PalwStateV2Error> {
        self.bind_material_income_checked_v1(now, root, income, false)
    }

    // Only the issuer consumer can derive this exception, from the actual
    // allocation row and native claim. Other income on the same BASE root
    // must still pass the generic enrollment guard above.
    pub(super) fn bind_issuer_material_income_root_v1(&mut self, now: u64, root: Hash64) -> Result<(), PalwStateV2Error> {
        if !self.params.material_income_active_at(now) {
            return Ok(());
        }
        let row = self
            .state
            .bond_budget
            .as_ref()
            .and_then(|book| book.claim_row(&root))
            .copied()
            .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("issuer allocation row absent".into()))?;
        let floor_issuer = self
            .state
            .claim(&root)
            .is_some_and(|claim| claim.bond == row.bond && native_floor_issuer_v1(self.params, Some(claim), row.origin));
        self.bind_material_income_checked_v1(now, root, native_income_id_v1(root, &row.bond, 0), floor_issuer)
    }

    pub(super) fn bind_selected_panel_material_income_root_v1(
        &mut self,
        now: u64,
        root: Hash64,
        bond: PalwBondKeyV2,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.material_income_active_at(now) {
            return Ok(());
        }
        let floor_seat = self.state.bond_budget.as_ref().and_then(|book| book.claim_row(&root)).is_some_and(|row| {
            self.state
                .claim(&root)
                .is_some_and(|claim| claim.bond == row.bond && native_floor_issuer_v1(self.params, Some(claim), row.origin))
        }) && self.state.panels.get(&root).is_some_and(|panel| panel.seats.iter().any(|seat| seat.bond == bond));
        self.bind_material_income_checked_v1(now, root, native_income_id_v1(root, &bond, 1), floor_seat)
    }

    fn bind_material_income_checked_v1(
        &mut self,
        now: u64,
        root: Hash64,
        income: Hash64,
        floor_issuer: bool,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.material_income_active_at(now) {
            return Ok(());
        }
        if !floor_issuer {
            self.state.require_monetary_material_enrollment_v1(self.params, root, now, None)?;
        }
        self.bond_budget_op(super::PalwBudgetGrowthV1::point(), |budget, _, j| budget.bind_material_income_v1(root, income, j))?
            .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("material root has no monetary engine".into()))?
            .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))
    }

    pub(super) fn initialize_material_income_v3(
        &mut self,
        ctx: &PalwBlockContextV2,
        policy: &PalwBondBudgetPolicyV1,
    ) -> Result<(), PalwStateV2Error> {
        let now = ctx.daa_score;
        let until = now.checked_add(policy.window_daa).ok_or(PalwStateV2Error::Overflow("legacy material uncertainty boundary"))?;
        // Freeze exactly the old monetary rows. Ongoing later income is never part of this quarantine.
        let old: Vec<_> = self
            .state
            .bond_budget
            .as_ref()
            .expect("budget precedes material fence")
            .claims
            .iter()
            .filter(|(_, row)| {
                row.live
                    && matches!(
                        row.origin,
                        PalwBudgetOriginV1::NativeIncome
                            | PalwBudgetOriginV1::NativeIncomePartial
                            | PalwBudgetOriginV1::SourceFee
                            | PalwBudgetOriginV1::DeferredReporterIncome
                    )
            })
            .map(|(id, row)| (*id, *row))
            .collect();
        let mut queues = self.state.pending_payouts.clone();
        for entry in &self.entries {
            if let PalwDeltaEntryV2::Payout { key, old: Some(payout), new: None } = entry {
                queues.insert(*key, *payout);
            }
        }
        let drained: BTreeSet<_> = self
            .entries
            .iter()
            .filter_map(|entry| match entry {
                PalwDeltaEntryV2::Payout { key, old: Some(_), new: None } => Some(*key),
                _ => None,
            })
            .collect();
        // RFC-0006 (RFC06-1): the boundary and every frozen old monetary row is a listed row of the op.
        let old_rows = old.len().saturating_add(queues.len()).saturating_add(1);
        self.bond_budget_op(super::PalwBudgetGrowthV1::rows(old_rows), |budget, _, j| {
            budget.activate_material_income_v3(j);
            let boundary = keyed64(b"misaka-palw/material-income/legacy-boundary/v1", &[&now.to_le_bytes()]);
            budget.put_material_income_v1(
                boundary,
                Some(PalwMaterialIncomeV1 {
                    root: None,
                    amount: 0,
                    accepted_daa: now,
                    payment: PalwMaterialPaymentV1::Historical { until },
                }),
                j,
            )?;
            for (income, row) in old {
                let amount = if row.origin == PalwBudgetOriginV1::DeferredReporterIncome {
                    row.reserved.reward_sompi.checked_sub(row.consumed.reward_sompi).ok_or(PalwBudgetRefusalV1::Overflow)?
                } else {
                    row.reserved.reward_sompi
                };
                if amount > 0 {
                    budget.put_material_income_v1(
                        material_pending_id_v1(income),
                        Some(PalwMaterialIncomeV1 {
                            root: None,
                            amount,
                            accepted_daa: row.accepted_daa,
                            payment: PalwMaterialPaymentV1::Pending { income },
                        }),
                        j,
                    )?;
                }
            }
            for (queue, payout) in queues {
                if payout.amount == 0 {
                    continue;
                }
                let id = keyed64(b"misaka-palw/material-income/legacy-queue/v1", &[queue.as_byte_slice(), &now.to_le_bytes()]);
                let paid = drained.contains(&queue);
                budget.put_material_income_v1(
                    id,
                    Some(PalwMaterialIncomeV1 {
                        root: None,
                        amount: payout.amount,
                        accepted_daa: now,
                        payment: PalwMaterialPaymentV1::Queued {
                            income: None,
                            queue: Some(queue),
                            queued_daa: now,
                            paid_daa: paid.then_some(now),
                            release_daa: paid.then_some(until),
                        },
                    }),
                    j,
                )?;
            }
            Ok::<(), PalwBudgetRefusalV1>(())
        })?
        .expect("created budget")
        .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
        // Migrate actual native fee escrows, not inferred payouts or future income.
        let mut fee_holds = Vec::new();
        if let Some(route) = self.state.kernel_route.as_ref() {
            for ((table, key), bytes) in &route.aux {
                if *table != crate::palw_verifier_fee_consent_v1::TABLE {
                    continue;
                }
                if key.first() == Some(&crate::palw_verifier_fee_consent_v1::HELD) {
                    let row: crate::palw_verifier_fee_consent_v1::HeldVerifierFeeV1 = borsh::from_slice(bytes)
                        .map_err(|_| PalwStateV2Error::CarriageInconsistent("fee migration malformed hold".into()))?;
                    fee_holds.push((row.terms.claim, row.terms.verifier, row.terms.slot, row.amount));
                } else if key.first() == Some(&crate::palw_paid_claim_trap_v1::PALW_PAID_CLAIM_TRAP_HELD_FEE_PREFIX_V1) {
                    let row: crate::palw_paid_claim_trap_v1::PalwHeldCheckFeeRowV1 = borsh::from_slice(bytes)
                        .map_err(|_| PalwStateV2Error::CarriageInconsistent("paid fee migration malformed hold".into()))?;
                    fee_holds.push((row.claim, row.verifier, row.slot, row.fee));
                }
            }
        }
        for (claim, verifier, slot, amount) in fee_holds {
            self.hold_verifier_fee_material_v1(now, claim, verifier, slot, amount)?;
        }
        // Authenticate the known native held subjects. Unidentified old rights retain
        // their exact old IDs globally; they are never guessed from a matching address.
        let mut subjects = BTreeMap::new();
        for (root, claim) in &self.state.claims {
            subjects.insert(native_income_id_v1(*root, &claim.bond, 0), *root);
        }
        for (root, panel) in &self.state.panels {
            for seat in &panel.seats {
                subjects.insert(native_income_id_v1(*root, &seat.bond, 1), *root);
            }
        }
        for (root, work) in &self.state.exec_v2.roots {
            for bond in &work.extra_executors {
                subjects.insert(native_income_id_v1(*root, bond, 2), *root);
            }
        }
        for (offence, verdict) in &self.state.consumed_offences {
            if verdict.claim_id != Hash64::from_u64_word(0) {
                if let Some(pending) = self.state.reward_pending.get(offence)
                    && let Some(winner) = pending.best
                {
                    subjects.insert(native_income_id_v1(*offence, &winner.reporter, 3), verdict.claim_id);
                }
                let award = native_reporter_binding_id_v1(*offence);
                if self.state.bond_budget.as_ref().is_some_and(|book| {
                    book.reporter_income
                        .get(&award)
                        .is_some_and(|binding| binding.recipient.is_some() && binding.awarded_daa.is_some())
                }) {
                    subjects.insert(award, verdict.claim_id);
                }
            }
        }
        // Source outcomes authenticate their revealed execution claim, unlike
        // old payload-only reporter queues. Preserve that known root directly.
        if let Some(route) = self.state.kernel_route.clone() {
            let ledger = route.ledger().map_err(PalwStateV2Error::BondBudgetRefused)?;
            for ((context, producer), promise) in &ledger.beacon_source_fee_promises {
                use misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeOutcomeV1;
                let source = ledger
                    .beacon_sources
                    .get(&(*context, *producer))
                    .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("material source promise has no signed source".into()))?;
                let Some(bond) = route.bond_key_of(producer) else { continue };
                let subject = super::super::palw_kernel_route_fold_v1::source_income_subject_v1(&source.job, &source.seal);
                let income = source_income_id_v1(Hash64::from_bytes(*context), subject, &bond);
                match promise.outcome {
                    None => {
                        if let Some((claim, _)) = source.revealed {
                            subjects.insert(income, Hash64::from_bytes(claim));
                        }
                    }
                    Some(BeaconSourceFeeOutcomeV1::Refunded { .. }) => {}
                    Some(BeaconSourceFeeOutcomeV1::Paid { daa, claim, .. }) => {
                        let root = Hash64::from_bytes(claim);
                        let queue = self
                            .state
                            .bonds
                            .get(&bond)
                            .map(|row| crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&row.payout_payload));
                        let queued = queue.map(|queue| native_queued_income_id_v1(queue, &bond)).filter(|queued| {
                            daa == ledger.daa
                                && self.state.bond_budget.as_ref().is_some_and(|book| {
                                    book.claim_row(queued).is_some_and(|row| {
                                        row.live
                                            && row.origin == PalwBudgetOriginV1::NativeIncomeQueued
                                            && row.consumed.reward_sompi >= promise.fee
                                    })
                                })
                        });
                        let payment = match queued {
                            Some(queued) => PalwMaterialPaymentV1::Queued {
                                income: Some(queued),
                                queue: None,
                                queued_daa: daa,
                                paid_daa: None,
                                release_daa: None,
                            },
                            None => PalwMaterialPaymentV1::UnprovenPhysicalPayment { until },
                        };
                        let id = keyed64(b"misaka-palw/material-income/legacy-source/v1", &[income.as_byte_slice()]);
                        let row = PalwMaterialIncomeV1 {
                            root: Some(root),
                            amount: promise.fee,
                            accepted_daa: promise.accepted_daa,
                            payment,
                        };
                        self.bond_budget_op(super::PalwBudgetGrowthV1::point(), |budget, _, j| {
                            budget.put_material_income_v1(id, Some(row), j)
                        })?
                        .expect("created material source engine")
                        .map_err(|why| PalwStateV2Error::BondBudgetRefused(why.to_string()))?;
                    }
                }
            }
        }
        for (income, root) in subjects {
            if self.state.bond_budget.as_ref().expect("created").material_income.contains_key(&material_pending_id_v1(income)) {
                self.bind_material_income_root_v1(now, root, income)?;
            }
        }
        Ok(())
    }

    /// Only changed roots are re-armed. A legacy uncertainty record affects old
    /// claims once on migration/payment/release, never every bond's future income.
    pub(in crate::palw_state_v2) fn rearm_material_retirements_v1(&mut self, now: u64) -> Result<(), PalwStateV2Error> {
        if !self.params.material_income_active_at(now) || self.params.claim_retirement_daa == 0 {
            return Ok(());
        }
        let mut roots = BTreeSet::new();
        let mut legacy = false;
        for entry in &self.entries {
            if let PalwDeltaEntryV2::KernelRouteRow { table, old, new, .. } = entry
                && *table == crate::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_TABLE_V1
            {
                for bytes in [old, new].into_iter().flatten() {
                    let row: crate::palw_monetary_material_v1::PalwMonetaryMaterialRowV1 = borsh::from_slice(bytes).map_err(|_| {
                        PalwStateV2Error::CarriageInconsistent("monetary material retirement row does not decode".into())
                    })?;
                    roots.extend(self.state.monetary_material_scope_v1(&row));
                }
            }
            if let PalwDeltaEntryV2::BondBudgetRow { table, old, new, .. } = entry
                && *table == PALW_BUDGET_TABLE_MATERIAL_INCOME_V1
            {
                for bytes in [old, new].into_iter().flatten() {
                    let row: PalwMaterialIncomeV1 = borsh::from_slice(bytes)
                        .map_err(|_| PalwStateV2Error::CarriageInconsistent("material retirement row does not decode".into()))?;
                    if let Some(root) = row.root {
                        roots.insert(root);
                    } else {
                        legacy = true;
                    }
                }
            }
        }
        if legacy {
            roots.extend(
                self.state
                    .claims
                    .iter()
                    .filter(|(_, claim)| claim.accepted_daa < self.params.material_income_from().unwrap_or(0))
                    .map(|(id, _)| *id),
            );
        }
        // A dependency's payment updates every authenticated native root which references it.
        let dependencies = roots.clone();
        roots.extend(
            self.state
                .exec_v2
                .roots
                .iter()
                .filter(|(root, work)| {
                    dependencies.contains(&work.prefix_claim)
                        || self
                            .state
                            .exec_v2
                            .slices
                            .range((**root, 0)..=(**root, u32::MAX))
                            .any(|(_, slice)| dependencies.contains(&slice.evidence_root))
                })
                .map(|(id, _)| *id),
        );
        for root in roots {
            if let Some(claim) = self.state.claims.get(&root).filter(|claim| claim.phase.is_terminal()).cloned() {
                // The existing exact operational entry must fire first, even
                // when a material change occurs in the block crossing it. The
                // sweep releases the commitment and then arms material-safe
                // retirement; no accepted collateral clock is extended.
                if terminal_operational_release_at_v1(&claim, self.params, None)?
                    .is_some_and(|release| self.state.deadlines.contains(&(release, root)))
                {
                    continue;
                }
                self.disarm_deadline(root);
                self.arm_retirement(root, &claim)?;
            }
        }
        Ok(())
    }
}

/// Fee-slot identity, shared by ordinary and paid-V2 consumers of the once-only leg.
fn verifier_fee_material_id(claim: [u8; 64], verifier: [u8; 64], slot: u64) -> Hash64 {
    keyed64(b"misaka-palw/material-income/verifier-fee/v1", &[&claim, &verifier, &slot.to_le_bytes()])
}
impl TransitionBuilder<'_> {
    /// Call only after the native consumer actually debited this exact escrow.
    /// Provenance only: no new money, quota, deadline or provider consent is created.
    pub(in crate::palw_state_v2) fn hold_verifier_fee_material_v1(
        &mut self,
        now: u64,
        claim: [u8; 64],
        verifier: [u8; 64],
        slot: u64,
        amount: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.material_income_active_at(now) || amount == 0 {
            return Ok(());
        }
        let obligation = verifier_fee_material_id(claim, verifier, slot);
        let id = material_pending_id_v1(obligation);
        let row = PalwMaterialIncomeV1 {
            root: Some(Hash64::from_bytes(claim)),
            amount,
            accepted_daa: now,
            payment: PalwMaterialPaymentV1::HeldEscrow { obligation },
        };
        self.bond_budget_op(super::PalwBudgetGrowthV1::point(), |b, _, j| {
            if b.material_income.contains_key(&id) {
                return Err(PalwBudgetRefusalV1::Duplicate);
            }
            b.put_material_income_v1(id, Some(row), j)
        })?
        .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("fee material engine missing".into()))?
        .map_err(|e| PalwStateV2Error::BondBudgetRefused(e.to_string()))
    }
    /// `queue=None` cancels the hold (burn/refusal or an existing budget-backed
    /// queue receipt). A raw payout uses Some only AFTER its actual queue write.
    pub(in crate::palw_state_v2) fn finish_verifier_fee_material_v1(
        &mut self,
        now: u64,
        claim: [u8; 64],
        verifier: [u8; 64],
        slot: u64,
        amount: u64,
        queue: Option<Hash64>,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.material_income_active_at(now) {
            return Ok(());
        }
        let obligation = verifier_fee_material_id(claim, verifier, slot);
        let id = material_pending_id_v1(obligation);
        let held = self
            .state
            .bond_budget
            .as_ref()
            .and_then(|b| b.material_income.get(&id))
            .copied()
            .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("fee material escrow missing".into()))?;
        if held.payment != (PalwMaterialPaymentV1::HeldEscrow { obligation }) || amount > held.amount {
            return Err(PalwStateV2Error::BondBudgetRefused("fee material differs from actual escrow".into()));
        }
        if let Some(queue) = queue {
            let mut total = amount;
            for m in self.state.bond_budget.as_ref().expect("held engine").material_income.values() {
                let same_queue = match m.payment {
                    PalwMaterialPaymentV1::Queued { income: None, queue: Some(q), paid_daa: None, .. } => q == queue,
                    PalwMaterialPaymentV1::Queued { income: Some(income), paid_daa: None, .. } => self
                        .state
                        .bond_budget
                        .as_ref()
                        .and_then(|b| b.claim_row(&income))
                        .is_some_and(|held| native_queued_income_id_v1(queue, &held.bond) == income),
                    _ => false,
                };
                if same_queue && m.root.is_some() {
                    total = total.checked_add(m.amount).ok_or(PalwStateV2Error::Overflow("raw fee queue receipts"))?;
                }
            }
            if total > self.state.pending_payouts.get(&queue).map_or(0, |p| p.amount) {
                return Err(PalwStateV2Error::BondBudgetRefused("fee material receipts exceed actual queue cash".into()));
            }
        }
        self.bond_budget_op(super::PalwBudgetGrowthV1::point(), |b, _, j| {
            if queue
                .filter(|_| amount > 0)
                .is_some_and(|q| b.material_income.contains_key(&material_queue_receipt_id_v1(obligation, q, now)))
            {
                return Err(PalwBudgetRefusalV1::Duplicate);
            }
            if now < held.accepted_daa {
                return Err(PalwBudgetRefusalV1::Assignment("fee queue precedes escrow"));
            }
            b.put_material_income_v1(id, None, j)?;
            if let Some(queue) = queue.filter(|_| amount > 0) {
                let receipt = material_queue_receipt_id_v1(obligation, queue, now);
                b.put_material_income_v1(
                    receipt,
                    Some(PalwMaterialIncomeV1 {
                        amount,
                        payment: PalwMaterialPaymentV1::Queued {
                            income: None,
                            queue: Some(queue),
                            queued_daa: now,
                            paid_daa: None,
                            release_daa: None,
                        },
                        ..held
                    }),
                    j,
                )?;
            }
            Ok::<(), PalwBudgetRefusalV1>(())
        })?
        .expect("held engine")
        .map_err(|e| PalwStateV2Error::BondBudgetRefused(e.to_string()))
    }
}

impl PalwChainStateV2 {
    pub(crate) fn assert_fee_material_provenance_v1(&self) -> Result<(), PalwStateV2Error> {
        let bad = |s: &str| PalwStateV2Error::CarriageInconsistent(s.into());
        let Some(book) = self.bond_budget.as_ref().filter(|b| b.header.version >= PALW_BOND_BUDGET_STATE_VERSION_V3) else {
            return Ok(());
        };
        let mut escrows = BTreeMap::new();
        if let Some(route) = self.kernel_route.as_ref() {
            for ((table, key), bytes) in &route.aux {
                if *table != crate::palw_verifier_fee_consent_v1::TABLE {
                    continue;
                }
                let (claim, verifier, slot, amount) = if key.first() == Some(&crate::palw_verifier_fee_consent_v1::HELD) {
                    let held: crate::palw_verifier_fee_consent_v1::HeldVerifierFeeV1 =
                        borsh::from_slice(bytes).map_err(|_| bad("malformed fee escrow"))?;
                    if *key
                        != crate::palw_verifier_fee_consent_v1::fee_key(
                            crate::palw_verifier_fee_consent_v1::HELD,
                            &held.terms.claim,
                            &held.terms.verifier,
                            held.terms.slot,
                        )
                    {
                        return Err(bad("fee escrow key mismatch"));
                    }
                    (held.terms.claim, held.terms.verifier, held.terms.slot, held.amount)
                } else if key.first() == Some(&crate::palw_paid_claim_trap_v1::PALW_PAID_CLAIM_TRAP_HELD_FEE_PREFIX_V1) {
                    let held: crate::palw_paid_claim_trap_v1::PalwHeldCheckFeeRowV1 =
                        borsh::from_slice(bytes).map_err(|_| bad("malformed paid fee escrow"))?;
                    if *key != crate::palw_paid_claim_trap_v1::palw_paid_trap_held_fee_key_v1(&held.claim, &held.verifier, held.slot) {
                        return Err(bad("paid fee escrow key mismatch"));
                    }
                    (held.claim, held.verifier, held.slot, held.fee)
                } else {
                    continue;
                };
                if amount > 0
                    && escrows.insert(verifier_fee_material_id(claim, verifier, slot), (Hash64::from_bytes(claim), amount)).is_some()
                {
                    return Err(bad("duplicate native fee escrow identity"));
                }
            }
        }
        let mut cash = BTreeMap::<Hash64, u64>::new();
        for row in book.material_income.values().filter(|m| m.root.is_some()) {
            match row.payment {
                PalwMaterialPaymentV1::HeldEscrow { obligation } => {
                    if escrows.remove(&obligation) != row.root.map(|root| (root, row.amount)) {
                        return Err(bad("fee material and native escrow differ"));
                    }
                }
                PalwMaterialPaymentV1::Queued { income, queue, paid_daa: None, .. } => {
                    let queue = match (income, queue) {
                        (None, Some(q)) => q,
                        (Some(id), None) => {
                            let held = book.claim_row(&id).ok_or_else(|| bad("material queue budget missing"))?;
                            let bond = self.bonds.get(&held.bond).ok_or_else(|| bad("material queue bond missing"))?;
                            let q = crate::palw_kernel_route_v1::palw_kernel_payout_key_v1(&bond.payout_payload);
                            if native_queued_income_id_v1(q, &held.bond) != id {
                                continue;
                            }
                            q
                        }
                        _ => return Err(bad("material queue identity malformed")),
                    };
                    let total = cash.entry(queue).or_default();
                    *total = total.checked_add(row.amount).ok_or_else(|| bad("material queue total overflow"))?;
                }
                _ => {}
            }
        }
        if !escrows.is_empty() {
            return Err(bad("native fee escrow lacks material provenance"));
        }
        for (q, total) in cash {
            if total > self.pending_payouts.get(&q).map_or(0, |p| p.amount) {
                return Err(bad("exact material receipts exceed actual queue cash"));
            }
        }
        Ok(())
    }
}
