//! **G14 lane escrow-rfc08 (RFC-0035 A / ADR-0202): the funded-escrow rows' one writer** (DORMANT).
//!
//! Every write goes through [`TransitionBuilder::write_funded_escrow_v1`], which asks
//! [`crate::palw_funded_escrow_v1::palw_escrow_row_transition_v1`] before it journals `FundedEscrow { key, old, new }` (delta 235)
//! — so the row's install and its once-only settlement ride the SAME rooted delta the block's other PALW effects ride, which the
//! processor commits in `commit_utxo_state`'s single WriteBatch with the UTXO diff (`PalwUtxoCommitV2`). A revert swaps `old`/`new`
//! (`apply_delta_entry`), restoring the row and therefore the outpoint lock the spend filter derives from it.
//!
//! Callers: settlement facts the processor observed on qualified spends (`PalwTransitionExtrasV1::funded_escrow_settlements`, empty
//! below `palw_tir_public_conviction_v1`). The installer's one caller is the funded-original coordinator
//! (`palw_tir_public_fold_v1::install_original_admission_v1`, round 2), inside the paired checkpoint; it is unreachable in production
//! because the coordinator's schedule is typed-missing.
use super::*;
use crate::palw_funded_escrow_v1::{
    PalwEscrowClaimLiabilityViewV1, PalwEscrowSettlementFactV1, PalwFundedEscrowRowV1, palw_escrow_liability_resolved_v1,
    palw_escrow_row_retirable_v1, palw_escrow_row_transition_v1,
};

fn refused(why: impl std::fmt::Debug) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(format!("RFC-0035 A funded escrow: {why:?}"))
}

impl PalwChainStateV2 {
    /// Route classification for the rooted escrow lock. It confers no spend
    /// authority, and a malformed row at the dedicated key stays protected.
    pub fn public_tfp1_escrow_requires_native_settlement_v1(&self, claim: Hash64) -> bool {
        use crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1;
        self.staged_court_row_v1(&PalwStagedCourtKeyV1::PublicTfp1FrameV1(claim)).is_some()
    }

    pub(super) fn has_funded_escrow_for_claim_v1(&self, claim: Hash64) -> bool {
        let first = (claim, Hash64::from_bytes([0; 64]));
        let last = (claim, Hash64::from_bytes([u8::MAX; 64]));
        self.funded_escrows_by_claim.range(first..=last).next().is_some()
    }

    pub(super) fn replace_funded_escrow_v1(&mut self, key: Hash64, new: Option<PalwFundedEscrowRowV1>) {
        let old = match new {
            Some(row) => self.funded_escrows.insert(key, row),
            None => self.funded_escrows.remove(&key),
        };
        let old_claim = old.map(|row| row.claim);
        let new_claim = new.map(|row| row.claim);
        if old_claim != new_claim {
            if let Some(claim) = old_claim {
                self.funded_escrows_by_claim.remove(&(claim, key));
            }
            if let Some(claim) = new_claim {
                self.funded_escrows_by_claim.insert((claim, key));
            }
        }
    }
}

impl TransitionBuilder<'_> {
    /// The one writer: refuses any transition the row rules do not define, journals the rest.
    pub(super) fn write_funded_escrow_v1(&mut self, key: Hash64, new: Option<PalwFundedEscrowRowV1>) -> Result<(), PalwStateV2Error> {
        let old = self.state.funded_escrows.get(&key).copied();
        palw_escrow_row_transition_v1(&key, old.as_ref(), new.as_ref()).map_err(refused)?;
        if self.native_writers_tracking_v1() {
            let install = old.is_none() && new.is_some();
            let mut quote = native_writers::NativeWriteQuoteV1::new("native funded escrow writer");
            quote.member_if::<Hash64, PalwFundedEscrowRowV1>(install)?.member_if::<(Hash64, Hash64), ()>(install)?.slots(1)?;
            self.admit_native_write_v1(quote)?;
        }
        self.state.replace_funded_escrow_v1(key, new);
        self.entries.push(PalwDeltaEntryV2::FundedEscrow { key, old, new });
        Ok(())
    }

    /// **Install a fresh row** for a fully admitted funded original (the coordinator only). One row per contract and per
    /// outpoint: an outpoint is never escrowed twice.
    pub(super) fn install_funded_escrow_v1(&mut self, row: PalwFundedEscrowRowV1) -> Result<(), PalwStateV2Error> {
        self.reject_positive_claim_alias_v2(row.claim)?;
        if self.state.funded_escrows.contains_key(&row.contract_id)
            || self.state.funded_escrows.values().any(|other| other.funding_outpoint == row.funding_outpoint)
        {
            return Err(refused("a contract or funding outpoint is escrowed once"));
        }
        self.write_funded_escrow_v1(row.contract_id, Some(row))
    }

    /// **G14 item 1 (lane r5): close every open funded liability whose predicate now holds.** Called once per accepting block after
    /// every object (`palw_state_v2.rs`, past `palw_tir_public_conviction_v1` only). The predicate
    /// ([`crate::palw_funded_original_admission_v1::funded_liability_resolved_v1`]) reads only rooted state at `daa`: the row's hard
    /// deadline, the original frame's material horizon, every duty clock of the claim, and the book's assessment of every packet of
    /// the claim's binding. Closing is the one unsettled-row transition the row rules allow (`open → closed`, nothing paid); it is
    /// journalled, so a reorg reopens it. The book's binding itself is NOT removed: its slots all lie before hard, so the reserved
    /// capacity is already released by the accepting clock, and the binding stays as supply history.
    pub(super) fn close_resolved_funded_liabilities_v1(&mut self, daa: u64) -> Result<(), PalwStateV2Error> {
        // Integration (2026-10-22): one closing rule for both sweeps — the strict admission predicate below AND the settlement
        // sweep's verdict/discharge/no-open-pursuit predicate — and one closing path, `clear_funded_escrow_liability_v1`, which also
        // releases the claim's bond capital reservation in the same delta.
        let open: Vec<PalwFundedEscrowRowV1> =
            self.state.funded_escrows.values().filter(|r| r.liability_open && r.settled_by.is_none()).copied().collect();
        for row in open {
            super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::retry_original_archive_ownership_v19(
                self, &row, daa,
            )?;
            // A missing/malformed financial authority is a typed refusal, not
            // a quiet downgrade to a merely pending old escrow.
            if super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::archive_capital_pending_v1(&self.state, &row)?
            {
                continue;
            }
            if self.funded_liability_strictly_resolved_v1(&row, daa)
                && palw_escrow_liability_resolved_v1(&row, &self.funded_escrow_claim_view_v1(&row.claim), daa)
            {
                self.clear_funded_escrow_liability_v1(row.contract_id, daa)?;
            }
        }
        Ok(())
    }

    /// The admission lane's exact predicate ([`crate::palw_funded_original_admission_v1::funded_liability_resolved_v1`]): the row's
    /// hard deadline, the original frame's material horizon, every duty clock of the claim and every packet of its binding decided.
    fn funded_liability_strictly_resolved_v1(&self, row: &PalwFundedEscrowRowV1, daa: u64) -> bool {
        use crate::palw_funded_original_admission_v1::{FundedLiabilityViewV1, funded_liability_resolved_v1};
        // Version2 holds the dedicated extra UTXO capital until its OWN
        // Source16 root/A2 retention has actually been released. A missing or
        // resource-unavailable receipt cannot free principal.
        match super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::archive_capital_pending_v1(&self.state, row) {
            Ok(false) => {}
            Ok(true) | Err(_) => return false,
        }
        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
        match self.state.staged_court_row_v1(&Key::PublicTfp1FrameV1(row.claim)) {
            Some(Row::PublicTfp1FrameV1(frame)) => {
                return self.protected_funded_liability_resolution_v1(row, frame, daa).is_ok();
            }
            Some(_) => return false,
            None => {}
        }
        let material_until_daa = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&self.state, row.claim)
            .map(|frame| frame.plan_v1().terms.material_until_daa);
        let clocks = self
            .state
            .court_duty_clocks_for_claim_v1(row.claim)
            .into_iter()
            .map(|c| (c.response_deadline_daa, c.material_until_daa))
            .collect();
        let packets = match self.state.court_service_state_v1().and_then(|s| s.claims.get(&row.claim).map(|b| (s, b))) {
            Some((service, binding)) => binding.packet_ids.iter().map(|id| service.book.assess(id)).collect(),
            None => Vec::new(),
        };
        funded_liability_resolved_v1(&FundedLiabilityViewV1 {
            daa,
            hard_deadline_daa: row.hard_deadline_daa,
            material_until_daa,
            clocks,
            packets,
        })
    }

    fn protected_funded_liability_resolution_v1(
        &self,
        row: &PalwFundedEscrowRowV1,
        frame: &crate::palw_positive_contract_v2::PublicTfp1FrameV1,
        now: u64,
    ) -> Result<(), &'static str> {
        if !self.protected_funded_service_resolved_v1(row, frame, now) {
            return Err("protected service or material retention is unresolved");
        }
        match self.state.has_public_pending_manifest_v1(frame.original_id_v1()) {
            Ok(false) => {}
            Ok(true) => return Err("protected manifest case is pending"),
            Err(_) => return Err("protected manifest case state is unavailable"),
        }
        if self.state.open_courts_of(&row.claim) != 0
            || self
                .state
                .court_duty_clocks_for_claim_v1(row.claim)
                .iter()
                .any(|clock| now < clock.response_deadline_daa || now < clock.material_until_daa)
        {
            return Err("protected court duty is unresolved");
        }
        if self.state.claim(&row.claim).is_none_or(|claim| !claim.phase.is_terminal()) {
            return Err("protected claim has no terminal disposition");
        }
        // The public court row is an archived refutation locator. A generic
        // Final/Voided claim and a passed deadline supply no paired public
        // disposition and settlement authority. Keep this route closed until
        // the native terminal writer can authenticate and discharge it.
        Err("protected settlement authority is unavailable")
    }

    fn protected_funded_service_resolved_v1(
        &self,
        row: &PalwFundedEscrowRowV1,
        frame: &crate::palw_positive_contract_v2::PublicTfp1FrameV1,
        now: u64,
    ) -> bool {
        use crate::palw_court_service_release_v1::{CourtServiceRowV3, ServiceBindingScopeV1};
        if frame.claim_v1() != row.claim
            || frame.origin_v1().accepted_daa != row.accepted_daa
            || row.accepted_daa.checked_add(crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA) != Some(row.hard_deadline_daa)
            || now < row.hard_deadline_daa.max(frame.material_until_daa_v1())
        {
            return false;
        }
        // Frame and book rows have already passed native write/import checks.
        // An absent binding is not a discharge: compaction leaves a scoped
        // release record, while every live obligation must be decided.
        let Some(service) = self.state.court_service_row_v3_v1() else { return false };
        let scope = ServiceBindingScopeV1 {
            claim: row.claim,
            allocation_id: frame.identity_v1(),
            source_id: frame.source_v1(),
            original_accepted_daa: row.accepted_daa,
            hard_deadline_daa: row.hard_deadline_daa,
        };
        if service.scope_v1(&row.claim) != Some(scope) {
            return false;
        }
        match service.state.claims.get(&row.claim) {
            Some(binding) => !binding.packet_ids.is_empty() && CourtServiceRowV3::binding_decided_v1(&service.state, &row.claim),
            None => service.resolved.get(&row.claim).is_some_and(|resolved| resolved.released_daa <= now),
        }
    }

    /// **Settle (or continue) a row from a qualified spend the processor accepted.** The fact's `old` must be exactly the rooted
    /// row (the processor read the same parent state); anything else is a refusal, never a re-derivation from the processor's word.
    /// G14 r5: `new` is either the settled row or, after a partial settlement, the continued row on the change-back outpoint —
    /// [`palw_escrow_row_transition_v1`] (asked by the writer) admits exactly those.
    pub(super) fn settle_funded_escrow_v1(&mut self, fact: &PalwEscrowSettlementFactV1) -> Result<(), PalwStateV2Error> {
        if self.state.funded_escrows.get(&fact.contract_id) != Some(&fact.old) {
            return Err(refused("a settlement fact whose row is not the rooted row"));
        }
        self.require_legacy_escrow_settlement_v1(fact.old.claim)?;
        // audit A-2 (defence in depth behind the one-escrow-per-transaction rule): a continued row never re-locks on an outpoint
        // another unsettled row already holds — one change-back outpoint backs exactly one row.
        if fact.new.settled_by.is_none()
            && self.state.funded_escrows.iter().any(|(id, row)| {
                *id != fact.contract_id && row.settled_by.is_none() && row.funding_outpoint == fact.new.funding_outpoint
            })
        {
            return Err(refused("a continued escrow row whose change-back outpoint is another live row's"));
        }
        self.write_funded_escrow_v1(fact.contract_id, Some(fact.new))?;
        super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::collect_settled_original_archive_receipt_v1(
            self,
            fact.contract_id,
        )
    }

    /// **G14 r5: the rooted liability view of a row's claim** — open court sessions, whether the claim record is still rooted and
    /// voided by its verdict, whether a post-Final liability record still keeps it convictable. From the builder's state only.
    fn funded_escrow_claim_view_v1(&self, claim: &Hash64) -> PalwEscrowClaimLiabilityViewV1 {
        let record = self.state.claims.get(claim);
        PalwEscrowClaimLiabilityViewV1 {
            open_courts: self.state.open_courts_of(claim),
            claim_live: record.is_some(),
            claim_voided: record.is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Voided { .. })),
            post_final_liability: self.state.panel_liabilities.contains_key(claim),
        }
    }

    /// **G14 r5 (item 2): clear one row's liability and release the claim's bond capital reservation with it.** The row's flag
    /// closes (the only edit the row rules allow on an unsettled row besides a spend), which is what lets the payer's return (after
    /// the hard deadline, or a scheduled return before it) qualify; and the claim's ADR-0176 reservation, if one is still open, is
    /// closed through the ordinary `bond_budget_close_v1` (its own liability-hold horizon applies), so the escrow and the bond's
    /// capital are released by the same event, in the same delta.
    pub(super) fn clear_funded_escrow_liability_v1(&mut self, contract_id: Hash64, now: u64) -> Result<(), PalwStateV2Error> {
        let row = *self.state.funded_escrows.get(&contract_id).ok_or_else(|| refused("no such funded escrow"))?;
        if !row.liability_open || row.settled_by.is_some() {
            return Err(refused("only an unsettled row with an open liability clears"));
        }
        self.require_legacy_escrow_settlement_v1(row.claim)?;
        if super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::archive_capital_pending_v1(&self.state, &row)? {
            return Err(refused("original archive capital still has a native retention owner"));
        }
        let mut cleared = row;
        cleared.liability_open = false;
        self.write_funded_escrow_v1(contract_id, Some(cleared))?;
        let reservation_open = self
            .state
            .bond_budget
            .as_ref()
            .and_then(|budget| budget.claim_row(&row.claim))
            .is_some_and(|reservation| reservation.open);
        if reservation_open {
            self.bond_budget_close_v1(&row.claim, now)?;
        }
        Ok(())
    }

    fn require_legacy_escrow_settlement_v1(&self, claim: Hash64) -> Result<(), PalwStateV2Error> {
        if self.state.public_tfp1_escrow_requires_native_settlement_v1(claim) {
            return Err(refused("protected TFP1 escrow requires its native settlement authority"));
        }
        Ok(())
    }

    /// **G14 r5 (items 2 and 3): the funded-escrow clock**, once per block before the objects (`prepare_block_objects_v1`, right
    /// after the settlements): every unsettled row whose liability is resolved
    /// ([`crate::palw_funded_escrow_v1::palw_escrow_liability_resolved_v1`]: a verdict, a discharge, or the hard deadline passed,
    /// never with a pursuit open) clears it, in contract order; then every settled row past its retention
    /// (`extras.funded_escrow_retention_daa`) leaves the rooted state. A no-op wherever no row exists — every network today.
    pub(super) fn sweep_funded_escrows_v1(&mut self, now: u64) -> Result<(), PalwStateV2Error> {
        if self.state.funded_escrows.is_empty() {
            return Ok(());
        }
        for row in self.state.funded_escrows.values().filter(|r| {
            r.version == crate::palw_funded_escrow_v1::PALW_FUNDED_ESCROW_ORIGINAL_ARCHIVE_ROW_VERSION_V1 && r.settled_by.is_none()
        }) {
            super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::archive_capital_pending_v1(&self.state, row)?;
        }
        let resolved: Vec<Hash64> = self
            .state
            .funded_escrows
            .values()
            .filter(|row| row.settled_by.is_none() && row.liability_open)
            .filter(|row| palw_escrow_liability_resolved_v1(row, &self.funded_escrow_claim_view_v1(&row.claim), now))
            .filter(|row| self.funded_liability_strictly_resolved_v1(row, now))
            .map(|row| row.contract_id)
            .collect();
        for contract_id in resolved {
            self.clear_funded_escrow_liability_v1(contract_id, now)?;
        }
        let retention = self.extras.funded_escrow_retention_daa;
        let retired: Vec<Hash64> = self
            .state
            .funded_escrows
            .values()
            .filter(|row| palw_escrow_row_retirable_v1(row, now, retention))
            .map(|row| row.contract_id)
            .collect();
        for contract_id in retired {
            self.write_funded_escrow_v1(contract_id, None)?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_funded_escrow_v1::{palw_escrow_qualified_spend_v1, palw_escrow_rows_by_outpoint_v1};
    use crate::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0).unwrap()
    }
    fn payload() -> Hash64 {
        kaspa_hashes::blake2b_512_address_payload(&[9u8; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN])
    }
    fn row() -> PalwFundedEscrowRowV1 {
        PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), TransactionOutpoint::new(h(77), 1), 1_000, h(3), payload(), 100, 10_100, false)
            .unwrap()
    }
    fn delta(entries: Vec<PalwDeltaEntryV2>) -> PalwStateDeltaV2 {
        PalwStateDeltaV2 {
            point: PalwBlockContextV2 { block: BlockHash::from_u64_word(1), daa_score: 10_200, blue_score: 1, subsidy: 0 },
            entries,
        }
    }

    /// Called with the actual dedicated frame installed by the paired-writer
    /// fixture. The one service packet below isolates the liability reader.
    pub(crate) fn exercise_protected_funded_liability_v1(parent: &PalwChainStateV2, params: &PalwStateParamsV2, claim: Hash64) {
        use crate::{
            palw_court_service_release_v1::CourtServiceRowV3,
            palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding, court_service_profile_id_v2},
            palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations},
            palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row},
        };
        let frame = parent.public_tfp1_frame_v1(claim).unwrap().unwrap();
        let row = *parent.funded_escrows_v1().values().find(|row| row.claim == claim).unwrap();
        let hard = row.hard_deadline_daa;
        let material_until = frame.material_until_daa_v1();
        assert!(material_until > hard + 1, "fixture must expose a post-hard retention tail");
        let extras = PalwTransitionExtrasV1::default();
        let mut builder = TransitionBuilder::new(parent, params, false, false, false, false, &extras);
        assert!(row.liability_open);
        assert!(!builder.funded_liability_strictly_resolved_v1(&row, material_until), "missing book holds");

        let id = h(501).as_bytes();
        let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        let mut book = ServiceReservations::new(cost).unwrap();
        book.reserve(&[ServicePacket { id, release_daa: row.accepted_daa + 1, service_before: hard, cost }], hard).unwrap();
        let source_profile_id = h(502);
        let mut service = CourtServiceRowV3::from_v2(&CourtServiceStateV2 {
            version: 2,
            profile_id: court_service_profile_id_v2(source_profile_id),
            source_profile_id,
            book,
            claims: BTreeMap::from([(
                claim,
                ServiceClaimBinding {
                    claim,
                    allocation_id: frame.identity_v1(),
                    source_id: frame.source_v1(),
                    original_accepted_daa: row.accepted_daa,
                    hard_deadline_daa: hard,
                    packet_ids: BTreeSet::from([id]),
                },
            )]),
        });
        let install_fixture = |builder: &mut TransitionBuilder<'_>, service: &CourtServiceRowV3| {
            builder.state.staged_courts.insert(Key::CourtService, Row::CourtServiceV3(Box::new(service.clone()).into()));
        };
        service.check().unwrap();
        service.state.check_context(&builder.state).unwrap();
        install_fixture(&mut builder, &service);
        assert!(!builder.funded_liability_strictly_resolved_v1(&row, material_until), "pending packet holds");

        service.state.book.observe_block(row.accepted_daa + 1, h(503).as_bytes(), ServiceCost::default()).unwrap();
        service.state.book.observe_block(hard, h(504).as_bytes(), ServiceCost::default()).unwrap();
        install_fixture(&mut builder, &service);
        builder.close_resolved_funded_liabilities_v1(hard + 1).unwrap();
        assert!(builder.state.funded_escrows_v1()[&row.contract_id].liability_open, "past hard, live binding still owes retention");
        assert!(builder.protected_funded_service_resolved_v1(&row, frame, material_until));

        service.release_v1(&BTreeSet::from([claim]), hard).unwrap();
        install_fixture(&mut builder, &service);
        assert!(!builder.protected_funded_service_resolved_v1(&row, frame, hard + 1), "release does not shorten retention");
        assert!(builder.protected_funded_service_resolved_v1(&row, frame, material_until));
        let mut foreign = service.clone();
        foreign.resolved.get_mut(&claim).unwrap().allocation_id = h(505);
        install_fixture(&mut builder, &foreign);
        assert!(!builder.protected_funded_service_resolved_v1(&row, frame, material_until), "foreign release cannot discharge");

        install_fixture(&mut builder, &service);
        builder.state.claims.get_mut(&claim).unwrap().phase = PalwClaimPhaseV2::Provisional;
        assert_eq!(
            builder.protected_funded_liability_resolution_v1(&row, frame, material_until),
            Err("protected claim has no terminal disposition")
        );
        builder.state.claims.get_mut(&claim).unwrap().phase = PalwClaimPhaseV2::Final { final_daa: hard };
        // A damaged pending-case record must also hold the escrow. This is a
        // negative storage fixture, not a successful case insertion.
        let pending_key = (
            crate::palw_positive_contract_v2::PUBLIC_PENDING_MANIFEST_TABLE_V1,
            [frame.original_id_v1().0.0, h(506).as_bytes()].concat(),
        );
        builder.state.kernel_route.as_mut().unwrap().aux.insert(pending_key.clone(), vec![0]);
        assert_eq!(
            builder.protected_funded_liability_resolution_v1(&row, frame, material_until),
            Err("protected manifest case state is unavailable")
        );
        builder.state.kernel_route.as_mut().unwrap().aux.remove(&pending_key);
        assert_eq!(
            builder.protected_funded_liability_resolution_v1(&row, frame, material_until),
            Err("protected settlement authority is unavailable")
        );
        builder.close_resolved_funded_liabilities_v1(material_until).unwrap();
        builder.sweep_funded_escrows_v1(material_until).unwrap();
        assert!(builder.state.funded_escrows_v1()[&row.contract_id].liability_open);
        assert!(builder.entries.is_empty());

        use crate::palw_funded_escrow_v1::{PalwEscrowDueKindV1, PalwEscrowDueV1, PalwEscrowFeeScheduleV1};
        let amount = row.unreleased_v1().unwrap();
        let legacy_schedule = PalwEscrowFeeScheduleV1::new_v1(
            row.contract_id,
            vec![PalwEscrowDueV1 { kind: PalwEscrowDueKindV1::Burn, payee: None, amount_sompi: amount }],
            1,
        )
        .unwrap();
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(row.funding_outpoint, vec![], 0, 0)],
            vec![],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let plan = palw_escrow_qualified_spend_v1(&row, &tx, &[amount], material_until, Some(&legacy_schedule)).unwrap();
        assert!(!plan.new.liability_open && plan.new.settled_by.is_some());
        let fact = PalwEscrowSettlementFactV1 { contract_id: row.contract_id, old: plan.old, new: plan.new };
        let before = builder.state.clone();
        assert!(matches!(
            builder.settle_funded_escrow_v1(&fact),
            Err(PalwStateV2Error::CarriageInconsistent(reason)) if reason.contains("protected TFP1 escrow requires its native settlement authority")
        ));
        assert_eq!(builder.state, before);
        assert!(builder.entries.is_empty());
    }

    /// **Reorg/undo restores the row and with it the lock**: install, then settle; reverting the settlement's delta brings the
    /// parent's unsettled (therefore locked) row back exactly, root for root, and reverting the install empties the table.
    #[test]
    fn funded_escrow_settlement_reverts_to_the_locked_row() {
        let p = params();
        let extras = PalwTransitionExtrasV1::default();
        let parent = PalwChainStateV2::genesis();
        let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        b.install_funded_escrow_v1(row()).unwrap();
        assert!(b.install_funded_escrow_v1(row()).is_err(), "a contract is escrowed once");
        let installed = b.state.clone();
        let install_delta = delta(b.entries.clone());
        assert!(installed.has_funded_escrow_for_claim_v1(row().claim));
        assert!(!installed.has_funded_escrow_for_claim_v1(h(50_000)));
        let replayed = apply_delta_v2(&parent, &install_delta, &p).unwrap();
        assert!(replayed.has_funded_escrow_for_claim_v1(row().claim));
        assert_eq!(palw_escrow_rows_by_outpoint_v1(installed.funded_escrows_v1()).len(), 1);

        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(row().funding_outpoint, vec![], 0, 0)],
            vec![TransactionOutput::new(1_000, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&payload().as_bytes()))],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let plan = palw_escrow_qualified_spend_v1(&row(), &tx, &[1_000], 10_200, None).unwrap();
        let fact = PalwEscrowSettlementFactV1 { contract_id: h(1), old: plan.old, new: plan.new };
        let mut b = TransitionBuilder::new(&installed, &p, false, false, false, false, &extras);
        b.settle_funded_escrow_v1(&fact).unwrap();
        assert!(b.settle_funded_escrow_v1(&fact).is_err(), "once-only: the rooted row is no longer the fact's old");
        let settled = b.state.clone();
        let settle_delta = delta(b.entries.clone());
        assert!(palw_escrow_rows_by_outpoint_v1(settled.funded_escrows_v1()).is_empty(), "lock released by the settlement");

        let undone = revert_delta_v2(&settled, &settle_delta, &p).unwrap();
        assert_eq!(undone.funded_escrows_v1(), installed.funded_escrows_v1());
        assert_eq!(palw_escrow_rows_by_outpoint_v1(undone.funded_escrows_v1()).len(), 1, "lock restored");
        assert_eq!(undone.state_root(), installed.state_root());
        let before = revert_delta_v2(&installed, &install_delta, &p).unwrap();
        assert!(settled.has_funded_escrow_for_claim_v1(row().claim));
        assert!(undone.has_funded_escrow_for_claim_v1(row().claim));
        assert!(!before.has_funded_escrow_for_claim_v1(row().claim));
        assert!(before.funded_escrows_v1().is_empty());
        assert_eq!(before.state_root(), parent.state_root(), "an empty table roots exactly as before");
    }

    /// **Lane r5: release at resolution.** An open liability blocks the payer's return; it closes only once the predicate holds
    /// (here: no frame, clock or binding, so the hard deadline decides), the close is journalled, and a revert reopens it.
    #[test]
    fn r5_funded_liability_closes_only_at_resolution_and_reverts() {
        let p = params();
        let extras = PalwTransitionExtrasV1::default();
        let open =
            PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), TransactionOutpoint::new(h(77), 1), 1_000, h(3), payload(), 100, 10_100, true)
                .unwrap();
        let parent = PalwChainStateV2::genesis();
        let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
        b.install_funded_escrow_v1(open).unwrap();
        b.close_resolved_funded_liabilities_v1(10_099).unwrap();
        assert!(b.state.funded_escrows_v1()[&h(1)].liability_open, "before hard: still open");
        let installed = b.state.clone();
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(open.funding_outpoint, vec![], 0, 0)],
            vec![TransactionOutput::new(1_000, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&payload().as_bytes()))],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        assert!(palw_escrow_qualified_spend_v1(&open, &tx, &[1_000], 10_200, None).is_err(), "open liability holds the return");
        let mut b = TransitionBuilder::new(&installed, &p, false, false, false, false, &extras);
        b.close_resolved_funded_liabilities_v1(10_100).unwrap();
        let closed = b.state.funded_escrows_v1()[&h(1)];
        assert!(!closed.liability_open && closed.paid_sompi == 0);
        assert!(palw_escrow_qualified_spend_v1(&closed, &tx, &[1_000], 10_200, None).is_ok(), "payer return after resolution");
        let after = b.state.clone();
        let undone = revert_delta_v2(&after, &delta(b.entries.clone()), &p).unwrap();
        assert!(undone.funded_escrows_v1()[&h(1)].liability_open, "reorg reopens the liability");
        assert_eq!(undone.state_root(), installed.state_root());
    }

    /// **G14 lane r6-d1-release (r5 open 4): a funded original admission reserves court capacity when `palw_court_capacity_v1` is
    /// armed** — the funded claim takes its hard-deadline bucket slot (refused when full), is counted by every later admission, and
    /// is entitled to the reserve pool for its first court row. Dormant (nothing reserved, never refused) below the fence.
    #[test]
    fn r6_d1_funded_admission_reserves_court_capacity_under_the_fence() {
        use super::super::palw_court_capacity_fold_v1::{check_funded_court_reserve_v1, palw_funded_court_reserve_claims_v1};
        let base = params();
        let policy = crate::palw_court_capacity_v1::palw_court_capacity_placeholder_policy_v1();
        let armed = base
            .clone()
            .with_court_capacity_v1(Some(crate::palw_court_capacity_v1::PalwCourtCapacityMirrorV1 { from_daa: 0, policy }));
        let cap = policy.admission_claims_per_bucket(armed.turn_deadline_daa());
        let zero_claim = |accepted: u64, retention: u64| {
            use borsh::BorshDeserialize;
            let zeros = [0u8; 8192];
            let mut c = <PalwClaimStateV2 as BorshDeserialize>::deserialize(&mut &zeros[..]).expect("a zero claim decodes");
            c.accepted_daa = accepted;
            c.trace_retention_daa = retention;
            c
        };
        let mut state = PalwChainStateV2::genesis();
        let fund = |state: &mut PalwChainStateV2, i: u64, open: bool| {
            state.claims.insert(h(1_000 + i), zero_claim(100, 50_000));
            let row = PalwFundedEscrowRowV1::fresh_v1(
                h(2_000 + i),
                h(1_000 + i),
                TransactionOutpoint::new(h(3_000 + i), 0),
                1_000,
                h(3),
                payload(),
                100,
                10_100,
                open,
            )
            .unwrap();
            state.funded_escrows.insert(h(2_000 + i), row);
        };
        // A closed liability holds no reserve.
        fund(&mut state, 999, false);
        assert!(!palw_funded_court_reserve_claims_v1(&state).contains(&h(1_999)));
        for i in 0..cap {
            fund(&mut state, i, true);
            check_funded_court_reserve_v1(&state, &armed, h(1_000 + i), 100).expect("the bucket still has room");
        }
        assert_eq!(palw_funded_court_reserve_claims_v1(&state).len() as u64, cap);
        fund(&mut state, cap, true);
        let full = check_funded_court_reserve_v1(&state, &armed, h(1_000 + cap), 100).unwrap_err();
        assert!(matches!(full, PalwStateV2Error::CourtClaimAdmissionFull { admitted, cap: c, .. } if admitted == cap && c == cap));
        // Dormant below the fence: nothing is reserved or refused.
        check_funded_court_reserve_v1(&state, &base, h(1_000 + cap), 100).unwrap();
        let later = base
            .clone()
            .with_court_capacity_v1(Some(crate::palw_court_capacity_v1::PalwCourtCapacityMirrorV1 { from_daa: 101, policy }));
        check_funded_court_reserve_v1(&state, &later, h(1_000 + cap), 100).unwrap();
    }

    /// The carriage round-trips a row; a row violating its invariants does not decode.
    #[test]
    fn funded_escrow_rows_round_trip_the_carriage() {
        let mut state = PalwChainStateV2::genesis();
        let empty_root = state.state_root();
        state.funded_escrows.insert(h(1), row());
        assert_ne!(state.state_root(), empty_root);
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap();
        let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(back.funded_escrows, state.funded_escrows);
        let mut bad = state.clone();
        let mut broken = row();
        broken.returned_sompi = 1; // a return on an unsettled row
        bad.funded_escrows.insert(h(1), broken);
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&bad)).unwrap();
        assert!(borsh::from_slice::<PalwStateCarriageV2>(&bytes).is_err());
    }

    // ---- G14 r5 (lane r5-escrow-settlement): continuation, liability clearing and retirement under revert ----

    fn open_row() -> PalwFundedEscrowRowV1 {
        PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), TransactionOutpoint::new(h(77), 1), 1_000, h(3), payload(), 100, 10_100, true)
            .unwrap()
    }
    fn payee(n: u8) -> crate::tx::ScriptPublicKey {
        crate::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(&[n; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN]).as_bytes(),
        )
    }

    /// **Item 5: every path reverts.** Install (liability open) → partial settlement with change-back (continuation) → liability
    /// cleared by the sweep (claim discharged: no record; at the hard deadline, the strict rule) → the payer's return → retirement past the retention. Reverting each delta in
    /// reverse order restores every intermediate row and root exactly, and the lock map follows the row (old outpoint, new outpoint,
    /// none).
    #[test]
    fn funded_escrow_every_path_reverts_to_its_parent_row() {
        use crate::palw_funded_escrow_v1::{PalwEscrowDueKindV1, PalwEscrowDueV1, PalwEscrowFeeScheduleV1};
        let p = params();
        let extras = PalwTransitionExtrasV1 { funded_escrow_retention_daa: Some(100), ..Default::default() };
        let genesis = PalwChainStateV2::genesis();
        let mut b = TransitionBuilder::new(&genesis, &p, false, false, false, false, &extras);
        b.install_funded_escrow_v1(open_row()).unwrap();
        let s0 = b.state.clone();

        // Partial settlement: work 300 + burn 100, 600 continues on the change-back output.
        let worker = kaspa_hashes::blake2b_512_address_payload(&[11u8; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN]);
        let schedule = PalwEscrowFeeScheduleV1::test_only_v1(
            h(1),
            vec![
                PalwEscrowDueV1 { kind: PalwEscrowDueKindV1::WorkPayment, payee: Some(worker), amount_sompi: 300 },
                PalwEscrowDueV1 { kind: PalwEscrowDueKindV1::Burn, payee: None, amount_sompi: 100 },
            ],
        );
        let payer_spk = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&payload().as_bytes());
        let tx1 = Transaction::new(
            0,
            vec![TransactionInput::new(open_row().funding_outpoint, vec![], 0, 0)],
            vec![TransactionOutput::new(300, payee(11)), TransactionOutput::new(600, payer_spk.clone())],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let plan = palw_escrow_qualified_spend_v1(&open_row(), &tx1, &[1_000], 5_000, Some(&schedule)).unwrap();
        let mut b = TransitionBuilder::new(&s0, &p, false, false, false, false, &extras);
        b.settle_funded_escrow_v1(&PalwEscrowSettlementFactV1 { contract_id: h(1), old: plan.old, new: plan.new }).unwrap();
        let s1 = b.state.clone();
        let d1 = delta(b.entries.clone());
        let locks = palw_escrow_rows_by_outpoint_v1(s1.funded_escrows_v1());
        assert!(locks.contains_key(&TransactionOutpoint::new(tx1.id(), 1)) && locks.len() == 1, "the lock moved to the change-back");

        // The sweep: the claim has no record and no liability record (discharged), but the strict admission predicate
        // (`funded_liability_resolved_v1`) also needs the hard deadline: before it nothing clears, at it the liability clears; no
        // budget engine, so no reservation to close. A second sweep changes nothing.
        let hard = open_row().hard_deadline_daa;
        let mut early = TransitionBuilder::new(&s1, &p, false, false, false, false, &extras);
        early.sweep_funded_escrows_v1(5_001).unwrap();
        assert!(early.entries.is_empty(), "a discharged claim's liability never clears before the hard deadline");
        early.sweep_funded_escrows_v1(hard - 1).unwrap();
        assert!(early.entries.is_empty(), "not one DAA before the hard deadline either");
        let mut b = TransitionBuilder::new(&s1, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(hard).unwrap();
        let s2 = b.state.clone();
        let d2 = delta(b.entries.clone());
        assert!(!s2.funded_escrows_v1()[&h(1)].liability_open);
        let mut again = TransitionBuilder::new(&s2, &p, false, false, false, false, &extras);
        again.sweep_funded_escrows_v1(hard + 1).unwrap();
        assert!(again.entries.is_empty(), "a cleared liability is cleared once");
        assert!(again.clear_funded_escrow_liability_v1(h(1), hard + 1).is_err());

        // Before the hard deadline a return still needs the (typed-missing) schedule (pure rule); once the liability is closed at or
        // after the hard deadline the payer's own return qualifies without one.
        let tx2 = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(tx1.id(), 1), vec![], 0, 0)],
            vec![TransactionOutput::new(600, payer_spk)],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let row2 = s2.funded_escrows_v1()[&h(1)];
        assert!(palw_escrow_qualified_spend_v1(&row2, &tx2, &[600], hard - 1, None).is_err());
        let settled = hard + 3;
        let plan2 = palw_escrow_qualified_spend_v1(&row2, &tx2, &[600], settled, None).unwrap();
        assert_eq!((plan2.returned_now, plan2.new.settled_daa), (600, settled));
        let mut b = TransitionBuilder::new(&s2, &p, false, false, false, false, &extras);
        b.settle_funded_escrow_v1(&PalwEscrowSettlementFactV1 { contract_id: h(1), old: plan2.old, new: plan2.new }).unwrap();
        let s3 = b.state.clone();
        let d3 = delta(b.entries.clone());
        assert!(palw_escrow_rows_by_outpoint_v1(s3.funded_escrows_v1()).is_empty());

        // Retirement: not at settled + retention, only after.
        let mut b = TransitionBuilder::new(&s3, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(settled + 100).unwrap();
        assert!(b.entries.is_empty() && b.state.funded_escrows_v1().contains_key(&h(1)));
        let mut b = TransitionBuilder::new(&s3, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(settled + 101).unwrap();
        let s4 = b.state.clone();
        let d4 = delta(b.entries.clone());
        assert!(s4.funded_escrows_v1().is_empty());
        assert!(!s4.has_funded_escrow_for_claim_v1(open_row().claim));
        assert_eq!(s4.state_root(), genesis.state_root(), "a retired table roots as the empty one");

        // Undo every step.
        let u3 = revert_delta_v2(&s4, &d4, &p).unwrap();
        assert_eq!((u3.funded_escrows_v1(), u3.state_root()), (s3.funded_escrows_v1(), s3.state_root()));
        let u2 = revert_delta_v2(&u3, &d3, &p).unwrap();
        assert_eq!((u2.funded_escrows_v1(), u2.state_root()), (s2.funded_escrows_v1(), s2.state_root()));
        let u1 = revert_delta_v2(&u2, &d2, &p).unwrap();
        assert_eq!((u1.funded_escrows_v1(), u1.state_root()), (s1.funded_escrows_v1(), s1.state_root()));
        assert!(u1.funded_escrows_v1()[&h(1)].liability_open, "the liability reopens with the undo");
        let u0 = revert_delta_v2(&u1, &d1, &p).unwrap();
        assert_eq!((u0.funded_escrows_v1(), u0.state_root()), (s0.funded_escrows_v1(), s0.state_root()));
        assert!(
            palw_escrow_rows_by_outpoint_v1(u0.funded_escrows_v1()).contains_key(&open_row().funding_outpoint),
            "original lock back"
        );

        // Pruned IBD: every intermediate state's carriage reproduces the same rows and root.
        for state in [&s0, &s1, &s2, &s3, &s4] {
            let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(state)).unwrap();
            let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
            assert_eq!(&back.funded_escrows, state.funded_escrows_v1());
            let restored = back.into_state(&p, Some(state.state_root())).unwrap();
            assert_eq!(restored.has_funded_escrow_for_claim_v1(open_row().claim), !state.funded_escrows_v1().is_empty());
            assert_eq!(borsh::to_vec(&PalwStateCarriageV2::from_state(&restored)).unwrap(), bytes);
        }
    }

    /// The sweep never clears under an open pursuit or for a live claim before the hard deadline; it does past the deadline.
    #[test]
    fn funded_escrow_sweep_holds_live_and_pursued_claims() {
        let p = params();
        let extras = PalwTransitionExtrasV1::default();
        let mut state = PalwChainStateV2::genesis();
        state.funded_escrows.insert(h(1), open_row());
        state.open_courts_by_claim.insert(h(2), 1);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(50_000).unwrap();
        assert!(b.entries.is_empty(), "an open pursuit holds the escrow even past the hard deadline");
        state.open_courts_by_claim.remove(&h(2));
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(10_101).unwrap();
        assert!(!b.state.funded_escrows_v1()[&h(1)].liability_open, "past the hard deadline with no pursuit it clears");
        // Below the fence (`None` retention) a settled row never retires.
        let mut settled = open_row();
        settled.liability_open = false;
        settled.returned_sompi = 1_000;
        settled.settled_by = Some(h(9));
        settled.settled_daa = 200;
        let mut state = PalwChainStateV2::genesis();
        state.funded_escrows.insert(h(1), settled);
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(u64::MAX).unwrap();
        assert!(b.entries.is_empty());
    }
}
