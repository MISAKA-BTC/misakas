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
use crate::palw_funded_escrow_v1::{PalwEscrowSettlementFactV1, PalwFundedEscrowRowV1, palw_escrow_row_transition_v1};

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
        use crate::palw_funded_original_admission_v1::{FundedLiabilityViewV1, funded_liability_resolved_v1};
        let open: Vec<PalwFundedEscrowRowV1> =
            self.state.funded_escrows.values().filter(|r| r.liability_open && r.settled_by.is_none()).copied().collect();
        for row in open {
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
            let view = FundedLiabilityViewV1 { daa, hard_deadline_daa: row.hard_deadline_daa, material_until_daa, clocks, packets };
            if funded_liability_resolved_v1(&view) {
                let mut closed = row;
                closed.liability_open = false;
                self.write_funded_escrow_v1(row.contract_id, Some(closed))?;
            }
        }
        Ok(())
    }

    /// **Settle a row from a qualified spend the processor accepted.** The fact's `old` must be exactly the rooted row (the
    /// processor read the same parent state); anything else is a refusal, never a re-derivation from the processor's word.
    pub(super) fn settle_funded_escrow_v1(&mut self, fact: &PalwEscrowSettlementFactV1) -> Result<(), PalwStateV2Error> {
        if self.state.funded_escrows.get(&fact.contract_id) != Some(&fact.old) {
            return Err(refused("a settlement fact whose row is not the rooted row"));
        }
        self.write_funded_escrow_v1(fact.contract_id, Some(fact.new))
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
        broken.paid_sompi = 1; // value moved on an unsettled row
        bad.funded_escrows.insert(h(1), broken);
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&bad)).unwrap();
        assert!(borsh::from_slice::<PalwStateCarriageV2>(&bytes).is_err());
    }
}
