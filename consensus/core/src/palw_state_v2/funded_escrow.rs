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

impl TransitionBuilder<'_> {
    /// The one writer: refuses any transition the row rules do not define, journals the rest.
    pub(super) fn write_funded_escrow_v1(&mut self, key: Hash64, new: Option<PalwFundedEscrowRowV1>) -> Result<(), PalwStateV2Error> {
        let old = self.state.funded_escrows.get(&key).copied();
        palw_escrow_row_transition_v1(&key, old.as_ref(), new.as_ref()).map_err(refused)?;
        match new {
            Some(row) => self.state.funded_escrows.insert(key, row),
            None => self.state.funded_escrows.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::FundedEscrow { key, old, new });
        Ok(())
    }

    /// **Install a fresh row** for a fully admitted funded original (the coordinator only). One row per contract and per
    /// outpoint: an outpoint is never escrowed twice.
    pub(super) fn install_funded_escrow_v1(&mut self, row: PalwFundedEscrowRowV1) -> Result<(), PalwStateV2Error> {
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
        let material_until_daa =
            crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&self.state, row.claim).map(|frame| frame.plan_v1().terms.material_until_daa);
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
        funded_liability_resolved_v1(&FundedLiabilityViewV1 { daa, hard_deadline_daa: row.hard_deadline_daa, material_until_daa, clocks, packets })
    }

    /// **Settle (or continue) a row from a qualified spend the processor accepted.** The fact's `old` must be exactly the rooted
    /// row (the processor read the same parent state); anything else is a refusal, never a re-derivation from the processor's word.
    /// G14 r5: `new` is either the settled row or, after a partial settlement, the continued row on the change-back outpoint —
    /// [`palw_escrow_row_transition_v1`] (asked by the writer) admits exactly those.
    pub(super) fn settle_funded_escrow_v1(&mut self, fact: &PalwEscrowSettlementFactV1) -> Result<(), PalwStateV2Error> {
        if self.state.funded_escrows.get(&fact.contract_id) != Some(&fact.old) {
            return Err(refused("a settlement fact whose row is not the rooted row"));
        }
        self.write_funded_escrow_v1(fact.contract_id, Some(fact.new))
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
        let mut cleared = row;
        cleared.liability_open = false;
        self.write_funded_escrow_v1(contract_id, Some(cleared))?;
        let reservation_open =
            self.state.bond_budget.as_ref().and_then(|budget| budget.claim_row(&row.claim)).is_some_and(|reservation| reservation.open);
        if reservation_open {
            self.bond_budget_close_v1(&row.claim, now)?;
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
mod tests {
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
        PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), TransactionOutpoint::new(h(77), 1), 1_000, h(3), payload(), 100, 10_100, false).unwrap()
    }
    fn delta(entries: Vec<PalwDeltaEntryV2>) -> PalwStateDeltaV2 {
        PalwStateDeltaV2 { point: PalwBlockContextV2 { block: BlockHash::from_u64_word(1), daa_score: 10_200, blue_score: 1, subsidy: 0 }, entries }
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
        assert!(before.funded_escrows_v1().is_empty());
        assert_eq!(before.state_root(), parent.state_root(), "an empty table roots exactly as before");
    }

    /// **Lane r5: release at resolution.** An open liability blocks the payer's return; it closes only once the predicate holds
    /// (here: no frame, clock or binding, so the hard deadline decides), the close is journalled, and a revert reopens it.
    #[test]
    fn r5_funded_liability_closes_only_at_resolution_and_reverts() {
        let p = params();
        let extras = PalwTransitionExtrasV1::default();
        let open = PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), TransactionOutpoint::new(h(77), 1), 1_000, h(3), payload(), 100, 10_100, true).unwrap();
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
        let armed = base.clone().with_court_capacity_v1(Some(crate::palw_court_capacity_v1::PalwCourtCapacityMirrorV1 { from_daa: 0, policy }));
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
            let row = PalwFundedEscrowRowV1::fresh_v1(h(2_000 + i), h(1_000 + i), TransactionOutpoint::new(h(3_000 + i), 0), 1_000, h(3),
                payload(), 100, 10_100, open).unwrap();
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
        let later = base.clone().with_court_capacity_v1(Some(crate::palw_court_capacity_v1::PalwCourtCapacityMirrorV1 { from_daa: 101, policy }));
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
        PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), TransactionOutpoint::new(h(77), 1), 1_000, h(3), payload(), 100, 10_100, true).unwrap()
    }
    fn payee(n: u8) -> crate::tx::ScriptPublicKey {
        crate::mldsa87_primitives::p2pkh_mldsa87_spk(
            &kaspa_hashes::blake2b_512_address_payload(&[n; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN]).as_bytes(),
        )
    }

    /// **Item 5: every path reverts.** Install (liability open) → partial settlement with change-back (continuation) → liability
    /// cleared by the sweep (claim discharged: no record) → final return → retirement past the retention. Reverting each delta in
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

        // The sweep: the claim has no record and no liability record (discharged) ⇒ the liability clears; no budget engine, so no
        // reservation to close. A second sweep changes nothing.
        let mut b = TransitionBuilder::new(&s1, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(5_001).unwrap();
        let s2 = b.state.clone();
        let d2 = delta(b.entries.clone());
        assert!(!s2.funded_escrows_v1()[&h(1)].liability_open);
        let mut again = TransitionBuilder::new(&s2, &p, false, false, false, false, &extras);
        again.sweep_funded_escrows_v1(5_002).unwrap();
        assert!(again.entries.is_empty(), "a cleared liability is cleared once");
        assert!(again.clear_funded_escrow_liability_v1(h(1), 5_002).is_err());

        // Before the hard deadline the payer return still needs the (typed-missing) schedule; with it, the rest returns.
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
        assert!(palw_escrow_qualified_spend_v1(&row2, &tx2, &[600], 5_003, None).is_err());
        let plan2 = palw_escrow_qualified_spend_v1(&row2, &tx2, &[600], 5_003, Some(&schedule)).unwrap();
        assert_eq!((plan2.returned_now, plan2.new.settled_daa), (600, 5_003));
        let mut b = TransitionBuilder::new(&s2, &p, false, false, false, false, &extras);
        b.settle_funded_escrow_v1(&PalwEscrowSettlementFactV1 { contract_id: h(1), old: plan2.old, new: plan2.new }).unwrap();
        let s3 = b.state.clone();
        let d3 = delta(b.entries.clone());
        assert!(palw_escrow_rows_by_outpoint_v1(s3.funded_escrows_v1()).is_empty());

        // Retirement: not at settled + retention, only after.
        let mut b = TransitionBuilder::new(&s3, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(5_103).unwrap();
        assert!(b.entries.is_empty() && b.state.funded_escrows_v1().contains_key(&h(1)));
        let mut b = TransitionBuilder::new(&s3, &p, false, false, false, false, &extras);
        b.sweep_funded_escrows_v1(5_104).unwrap();
        let s4 = b.state.clone();
        let d4 = delta(b.entries.clone());
        assert!(s4.funded_escrows_v1().is_empty());
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
        assert!(palw_escrow_rows_by_outpoint_v1(u0.funded_escrows_v1()).contains_key(&open_row().funding_outpoint), "original lock back");

        // Pruned IBD: every intermediate state's carriage reproduces the same rows and root.
        for state in [&s0, &s1, &s2, &s3, &s4] {
            let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(state)).unwrap();
            let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
            assert_eq!(&back.funded_escrows, state.funded_escrows_v1());
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
