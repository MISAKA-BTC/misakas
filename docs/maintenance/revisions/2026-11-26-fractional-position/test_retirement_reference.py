#!/usr/bin/env python3
from dataclasses import replace
import unittest

import moe3_reference as math
import retirement_reference as r

S = math.SCALE
MEASUREMENTS = {}


def native_lot(ledger, lot_id, model, owner, kind, role, amount, *, asset="MISAKA", status="LIVE",
               virtual=0, history=0):
    source_id = "source:" + lot_id
    ledger.add_source(r.FundingSource(source_id, asset, amount, model if asset == "POSITION" else None))
    ledger.add_lot(r.Lot(lot_id, model, owner, kind, role, asset, amount, amount, status,
                        r.FundingRange(source_id, 0, amount), virtual, history))


def fixture():
    ledger = r.RetirementLedger()
    native_lot(ledger, "pool", "bad-model", "protocol", "POOL", "MINER", 200 * S,
               virtual=10_000 * S, history=1000 * S)
    native_lot(ledger, "registrant", "bad-model", "registrant", "BOND", "REGISTRANT", 500 * S)
    ledger.add_source(r.FundingSource("same-owner-collateral", "MISAKA", 10_000 * S))
    ledger.add_lot(r.Lot("mining", "bad-model", "same-person", "BOND", "MINER", "MISAKA", 6000 * S, 6000 * S,
                        "LIVE", r.FundingRange("same-owner-collateral", 0, 6000 * S)))
    ledger.add_lot(r.Lot("panel", "bad-model", "same-person", "BOND", "PANEL", "MISAKA", 4000 * S, 4000 * S,
                        "LIVE", r.FundingRange("same-owner-collateral", 6000 * S, 4000 * S)))
    native_lot(ledger, "gate-c", "bad-model", "suspected-verifier", "BOND", "GATE_C", 2000 * S)
    native_lot(ledger, "other-mining", "good-model", "same-person", "BOND", "MINER", 5000 * S)
    native_lot(ledger, "pending-miner-cash", "bad-model", "miner", "REWARD", "MINER", 300 * S, status="ESCROWED")
    native_lot(ledger, "pending-miner-position", "bad-model", "miner", "REWARD", "MINER", 50 * S,
               status="ESCROWED", asset="POSITION")
    ledger.add_lot(r.Lot("unissued-miner", "bad-model", "miner", "REWARD", "MINER", "MISAKA",
                        900 * S, 900 * S, "UNISSUED", None))
    native_lot(ledger, "settled-miner-cash", "bad-model", "miner", "REWARD", "MINER", 700 * S, status="SETTLED")
    native_lot(ledger, "settled-miner-position", "bad-model", "miner", "REWARD", "MINER", 70 * S,
               status="SETTLED", asset="POSITION")
    native_lot(ledger, "earned-panel", "bad-model", "panel-payee", "REWARD", "PANEL", 50 * S, status="ESCROWED")
    native_lot(ledger, "earned-gate-c", "bad-model", "gate-payee", "REWARD", "GATE_C", 40 * S, status="SETTLED")
    native_lot(ledger, "earned-bft", "bad-model", "bft-payee", "REWARD", "BFT", 50 * S, status="ESCROWED")
    return ledger


def retire(ledger, case="case"):
    ledger.freeze_case(case, "bad-model")
    return ledger.retire(case, r.VerifiedRetirementDecision(case, "bad-model", True))


def court_order(ledger, lot_id, maximum, offence="individual-violation", *, verified=True, recipients=None, deadline=1000):
    if recipients is None:
        recipients = (r.CourtRecipient("PANEL", "proving-verifier", 1),)
    return r.VerifiedCourtOrder("order:" + offence, offence, lot_id, ledger.lot(lot_id).owner,
                                maximum, recipients, deadline, verified)


class RetirementTests(unittest.TestCase):
    def refusal(self, code, fn):
        with self.assertRaises(math.Refusal) as caught:
            fn()
        self.assertEqual(caught.exception.code, code)

    def test_role_lots_6000_mining_burn_4000_panel_protected(self):
        ledger = fixture()
        before_panel = ledger.lot("panel")
        receipt, _ = retire(ledger)
        self.assertEqual(ledger.lot("mining").remaining_atoms, 0)
        self.assertEqual(ledger.lot("panel"), before_panel)
        self.assertEqual(ledger.lot("panel").remaining_atoms, 4000 * S)
        self.assertEqual(ledger.lot("registrant").remaining_atoms, 0)
        self.assertEqual(receipt.paid_to_bft_misaka_atoms, 0)
        self.assertEqual(receipt.bounty_misaka_atoms, 0)
        self.assertEqual(receipt.burned_misaka_atoms, 7000 * S)
        ledger.conservation()
        MEASUREMENTS["role_lot_example"] = {"same_owner_mining_burn_misaka": "6000",
                                           "same_owner_panel_protected_misaka": "4000",
                                           "bft_payout_misaka": "0", "bounty_misaka": "0"}

    def test_pool_burn_is_current_real_cash_only(self):
        ledger = fixture()
        receipt, _ = retire(ledger)
        self.assertEqual(dict(receipt.burns_by_purpose)["POOL"], 200 * S)
        self.assertEqual(ledger.lot("pool").virtual_pool_misaka_atoms, 10_000 * S)
        self.assertEqual(ledger.lot("pool").historical_lp_misaka_atoms, 1000 * S)
        self.assertNotEqual(dict(receipt.burns_by_purpose)["POOL"], 1200 * S)
        MEASUREMENTS["pool_example"] = {"current_real_burn_misaka": "200", "historical_lp_not_reburned_misaka": "1000",
                                        "virtual_not_cash_misaka": "10000"}

    def test_unissued_cancel_cash_burn_position_retire_are_distinct(self):
        ledger = fixture()
        initial = ledger.conservation()
        receipt, _ = retire(ledger)
        self.assertEqual(receipt.cancelled_unissued_misaka_atoms, 900 * S)
        self.assertEqual(receipt.retired_position_atoms, 50 * S)
        self.assertEqual(ledger.lot("unissued-miner").status, "CANCELLED")
        self.assertEqual(ledger.lot("pending-miner-cash").status, "BURNED")
        self.assertEqual(ledger.lot("pending-miner-position").status, "RETIRED")
        after = ledger.conservation()
        self.assertEqual(initial["initial_misaka_atoms"], after["initial_misaka_atoms"])
        self.assertEqual(initial["available_misaka_atoms"] - after["available_misaka_atoms"], 7000 * S)
        self.assertEqual(initial["available_position_atoms"] - after["available_position_atoms"], 50 * S)
        MEASUREMENTS["native_conservation"] = {key: str(value) for key, value in after.items()}

    def test_other_models_settled_rewards_and_earned_verifier_cash_stay(self):
        ledger = fixture()
        protected_ids = ["panel", "gate-c", "other-mining", "settled-miner-cash", "settled-miner-position",
                         "earned-panel", "earned-gate-c", "earned-bft"]
        protected = {key: ledger.lot(key) for key in protected_ids}
        retire(ledger)
        self.assertEqual({key: ledger.lot(key) for key in protected_ids}, protected)

    def test_bft_protects_verifier_then_verified_court_individual_slash_remains(self):
        ledger = fixture()
        retire(ledger)
        self.assertEqual(ledger.lot("gate-c").remaining_atoms, 2000 * S)
        before = ledger.state
        self.refusal("UnverifiedCourtOrder", lambda: ledger.court_collect(
            court_order(ledger, "gate-c", 500 * S, "fake", verified=False), "fake-collection", 500 * S))
        self.assertEqual(ledger.state, before)
        order = court_order(ledger, "gate-c", 500 * S, "proven-personal-violation")
        ledger.court_collect(order, "collection", 500 * S)
        ledger.bind_court_plan(order.offence_id, order.recipients, True)
        ledger.claim_court_reward(order.offence_id, "collection")
        self.assertEqual(ledger.lot("gate-c").remaining_atoms, 1500 * S)
        self.assertEqual(ledger.state.burned_misaka_atoms, 7000 * S)
        self.assertEqual(ledger.state.court_debited_misaka_atoms, 500 * S)
        self.assertEqual(ledger.state.court_burned_misaka_atoms, 255 * S)
        self.assertEqual(sum(row.misaka_atoms for row in ledger.state.court_credits), 245 * S)
        self.assertEqual(ledger.lot("panel").remaining_atoms, 4000 * S)
        ledger.conservation()
        self.refusal("CourtCollectionAlreadyUsed", lambda: ledger.court_collect(order, "collection", 500 * S))

    def test_court_during_freeze_cannot_cause_double_burn(self):
        ledger = fixture()
        ledger.freeze_case("case", "bad-model")
        ledger.court_collect(court_order(ledger, "mining", 1000 * S, "prior-court"), "prior-collection", 1000 * S)
        receipt, _ = ledger.retire("case", r.VerifiedRetirementDecision("case", "bad-model", True))
        self.assertEqual(dict(receipt.burns_by_purpose)["BOND:MINER"], 5000 * S)
        self.assertEqual(receipt.burned_misaka_atoms, 6000 * S)
        self.assertEqual(ledger.state.court_debited_misaka_atoms, 1000 * S)
        ledger.conservation()

    def test_frozen_lot_roles_and_new_target_lots_cannot_be_changed(self):
        ledger = fixture()
        ledger.freeze_case("case", "bad-model")
        frozen = ledger.state
        self.refusal("FrozenClassification", lambda: ledger.reclassify_bond("mining", "PANEL"))
        self.refusal("FrozenClassification", lambda: ledger.reclassify_bond("panel", "MINER"))
        self.assertEqual(ledger.state, frozen)
        self.refusal("FrozenClassification", lambda: ledger.add_lot(r.Lot(
            "new-unissued", "bad-model", "miner", "REWARD", "MINER", "MISAKA", 1, 1, "UNISSUED", None)))
        self.assertEqual(ledger.state, frozen)
        # Court remains callable: it changes current amount, not purpose/funding identity.
        ledger.court_collect(court_order(ledger, "panel", 1, "court"), "court-collection", 1)
        ledger.retire("case", r.VerifiedRetirementDecision("case", "bad-model", True))

    def test_duplicate_native_identity_and_overlapping_ranges_rejected(self):
        ledger = fixture()
        before = ledger.state
        duplicate = r.Lot("second-label", "other-model", "attacker", "BOND", "PANEL", "MISAKA",
                          6000 * S, 6000 * S, "LIVE", r.FundingRange("same-owner-collateral", 0, 6000 * S))
        self.refusal("OverlappingFunding", lambda: ledger.add_lot(duplicate))
        self.assertEqual(ledger.state, before)
        self.refusal("OverlappingFunding", lambda: ledger.add_lot(replace(duplicate, funding=r.FundingRange(
            "same-owner-collateral", 1000 * S, 6000 * S))))
        self.assertEqual(ledger.state, before)
        self.refusal("DuplicateLot", lambda: ledger.add_lot(ledger.lot("panel")))
        self.refusal("DuplicateFundingSource", lambda: ledger.add_source(
            r.FundingSource("same-owner-collateral", "POSITION", 10_000 * S, "bad-model")))
        self.assertEqual(ledger.state, before)

    def test_burned_ranges_cannot_be_relabelled_as_new_money(self):
        ledger = fixture()
        retire(ledger)
        before = ledger.state
        self.refusal("OverlappingFunding", lambda: ledger.add_lot(r.Lot(
            "resurrection", "another-model", "attacker", "BOND", "MINER", "MISAKA", 200 * S, 200 * S,
            "LIVE", r.FundingRange("source:pool", 0, 200 * S))))
        self.assertEqual(ledger.state, before)

    def test_one_time_atomic_retirement_and_ordered_undo_replay(self):
        for stage in ("lots", "receipt"):
            ledger = fixture()
            ledger.freeze_case("case", "bad-model")
            before = ledger.state
            decision = r.VerifiedRetirementDecision("case", "bad-model", True)
            self.refusal("InjectedFailure", lambda: ledger.retire("case", decision, fail_after=stage))
            self.assertEqual(ledger.state, before)
            first, undo = ledger.retire("case", decision)
            after = ledger.state
            self.refusal("AlreadyRetired", lambda: ledger.retire("case", decision))
            self.assertEqual(ledger.state, after)
            ledger.undo(undo)
            self.assertEqual(ledger.state, before)
            replay, _ = ledger.retire("case", decision)
            self.assertEqual(replay, first)
            self.assertEqual(ledger.state, after)
            ledger.conservation()

    def test_unverified_bft_and_wrong_case_are_not_retirement_authority(self):
        ledger = fixture()
        ledger.freeze_case("case", "bad-model")
        before = ledger.state
        for decision in (r.VerifiedRetirementDecision("case", "bad-model", False),
                         r.VerifiedRetirementDecision("other-case", "bad-model", True),
                         r.VerifiedRetirementDecision("case", "good-model", True)):
            self.refusal("UnverifiedRetirementDecision", lambda d=decision: ledger.retire("case", d))
            self.assertEqual(ledger.state, before)

    def test_market_failure_rights_are_separately_funded_and_survive_retirement(self):
        market = math.Market("bad-model", 100, 0, 0, 3, 0, 0, 3)
        book = math.ReferenceBook(market)
        book.allocate_budget("services-earned", "miner", 1000 * S,
                             (math.VerificationAllocation("PANEL", "verifier", 50 * S),))
        self.refusal("LiquidityDust", lambda: book.settle_market("services-earned"))
        self.assertEqual(book.state.verification_reserved_misaka_atoms, 50 * S)
        self.assertEqual(book.state.bft_reserved_misaka_atoms, 50 * S)
        ledger = r.RetirementLedger()
        native_lot(ledger, "market-funds", "bad-model", "miner", "REWARD", "MINER", 900 * S, status="ESCROWED")
        native_lot(ledger, "panel-right", "bad-model", "verifier", "REWARD", "PANEL", 50 * S, status="ESCROWED")
        native_lot(ledger, "bft-right", "bad-model", "bft", "REWARD", "BFT", 50 * S, status="ESCROWED")
        receipt, _ = retire(ledger)
        self.assertEqual(receipt.burned_misaka_atoms, 900 * S)
        self.assertEqual(ledger.lot("panel-right").remaining_atoms, 50 * S)
        self.assertEqual(ledger.lot("bft-right").remaining_atoms, 50 * S)
        ledger.conservation()

    def test_court_actual_recovery_49_pool_51_burn_and_one_atom_rounding(self):
        for collected, reward, burned in ((1, 0, 1), (100, 49, 51)):
            ledger = r.RetirementLedger()
            native_lot(ledger, "collateral", "bad-model", "defendant", "BOND", "GATE_C", 1000)
            recipients = (r.CourtRecipient("PANEL", "finder", 30), r.CourtRecipient("GATE_C", "prover", 19))
            order = court_order(ledger, "collateral", 1000, recipients=recipients)
            ledger.court_collect(order, "actual-collection", collected)
            self.assertEqual(ledger.state.court_debited_misaka_atoms, collected)
            self.assertEqual(ledger.state.court_burned_misaka_atoms, burned)
            self.assertEqual(sum(row.remaining_misaka_atoms for row in ledger.state.court_reward_escrows), reward)
            # The uncollected part of the authorized penalty creates no reward.
            self.assertEqual(ledger.state.court_recoveries[0].rewarded_misaka_atoms, reward)
            ledger.bind_court_plan(order.offence_id, recipients, True)
            if reward:
                ledger.claim_court_reward(order.offence_id, "actual-collection")
                self.assertEqual([row.misaka_atoms for row in ledger.state.court_credits], [30, 19])
            self.assertEqual(sum(row.misaka_atoms for row in ledger.state.court_credits), reward)
            ledger.conservation()
        MEASUREMENTS["court_actual_recovery"] = {"authorized_penalty_atoms": "1000", "collected_atoms": "100",
                                                "panel_gate_c_total_atoms": "49", "burn_atoms": "51",
                                                "uncollected_amount_earns_reward": False}

    def test_cumulative_one_atom_collections_equal_single_collection(self):
        ledgers = []
        for split in (False, True):
            ledger = r.RetirementLedger()
            native_lot(ledger, "collateral", "bad-model", "defendant", "BOND", "PANEL", 100)
            order = court_order(ledger, "collateral", 100)
            for index, amount in enumerate(([1] * 100) if split else [100]):
                ledger.court_collect(order, f"collection:{index}", amount)
            ledger.bind_court_plan(order.offence_id, order.recipients, True)
            for escrow in tuple(ledger.state.court_reward_escrows):
                ledger.claim_court_reward(order.offence_id, escrow.collection_id)
            self.assertEqual(ledger.state.court_burned_misaka_atoms, 51)
            self.assertEqual(sum(row.misaka_atoms for row in ledger.state.court_credits), 49)
            self.assertEqual(ledger.state.court_recoveries[0].recovered_misaka_atoms, 100)
            ledger.conservation()
            ledgers.append(ledger)
        MEASUREMENTS["cumulative_collection"] = {"single_100_atoms_reward": "49", "hundred_single_atoms_reward": "49",
                                                "burn_in_both": "51", "recipient_pool_recomputed_from_cumulative_recovery": True,
                                                "previously_paid_recipient_amounts_reapportioned": False}

    def test_bad_or_missing_recipients_never_undo_collection(self):
        for proposed in ((), (r.CourtRecipient("PANEL", "defendant", 1),)):
            ledger = r.RetirementLedger()
            native_lot(ledger, "collateral", "bad-model", "defendant", "BOND", "MINER", 100)
            order = court_order(ledger, "collateral", 100, recipients=proposed)
            ledger.court_collect(order, "collection", 100)
            collected = ledger.state
            self.assertEqual(ledger.lot("collateral").remaining_atoms, 0)
            self.assertEqual(ledger.state.court_burned_misaka_atoms, 51)
            self.refusal("CourtRecipientPlanUnresolved", lambda: ledger.claim_court_reward(order.offence_id, "collection"))
            self.assertEqual(ledger.state, collected)
            if proposed:
                self.refusal("DefendantCannotReceive", lambda: ledger.bind_court_plan(order.offence_id, proposed, True))
            else:
                self.refusal("MissingCourtRecipientPlan", lambda: ledger.bind_court_plan(order.offence_id, proposed, True))
            self.assertEqual(ledger.state, collected)
            legal = (r.CourtRecipient("GATE_C", "eligible-prover", 1),)
            self.refusal("UnverifiedCourtRecipientPlan", lambda: ledger.bind_court_plan(order.offence_id, legal, False))
            self.assertEqual(ledger.state, collected)
            ledger.bind_court_plan(order.offence_id, legal, True)
            ledger.claim_court_reward(order.offence_id, "collection")
            self.assertEqual(ledger.state.court_credits[0].liability_identity, "eligible-prover")
            self.assertEqual(ledger.state.court_credits[0].misaka_atoms, 49)
            ledger.conservation()

    def test_original_deadline_burns_unclaimed_rewards_without_reset(self):
        ledger = r.RetirementLedger()
        native_lot(ledger, "collateral", "bad-model", "defendant", "BOND", "MINER", 200)
        order = court_order(ledger, "collateral", 200, recipients=(), deadline=10)
        ledger.court_collect(order, "on-time", 100, now=1)
        self.refusal("CourtRewardNotExpired", lambda: ledger.expire_court_rewards(order.offence_id, now=10))
        ledger.expire_court_rewards(order.offence_id, now=11)
        self.assertEqual(ledger.state.court_burned_misaka_atoms, 100)
        self.assertEqual(ledger.state.court_reward_forfeited_misaka_atoms, 49)
        self.refusal("CourtRewardExpired", lambda: ledger.bind_court_plan(order.offence_id,
                     (r.CourtRecipient("PANEL", "late", 1),), True, now=11))
        ledger.court_collect(order, "late-recovery", 100, now=12)
        self.assertEqual(ledger.state.court_burned_misaka_atoms, 200)
        self.assertEqual(ledger.state.court_reward_forfeited_misaka_atoms, 98)
        self.assertEqual(sum(row.remaining_misaka_atoms for row in ledger.state.court_reward_escrows), 0)
        ledger.conservation()

    def test_collection_ids_scope_cap_and_recipient_plan_are_fixed(self):
        ledger = r.RetirementLedger()
        native_lot(ledger, "collateral", "bad-model", "defendant", "BOND", "MINER", 200)
        order = court_order(ledger, "collateral", 100)
        ledger.court_collect(order, "one", 50)
        prior = ledger.state
        self.refusal("CourtCollectionAlreadyUsed", lambda: ledger.court_collect(order, "one", 50))
        self.refusal("CourtScopeChanged", lambda: ledger.court_collect(replace(order, max_recovery_atoms=200), "two", 1))
        self.assertEqual(ledger.state, prior)
        ledger.bind_court_plan(order.offence_id, order.recipients, True)
        self.refusal("CourtRecipientPlanChanged", lambda: ledger.bind_court_plan(order.offence_id,
                     (r.CourtRecipient("GATE_C", "replacement", 1),), True))
        ledger.claim_court_reward(order.offence_id, "one")
        paid = ledger.state.court_credits
        ledger.court_collect(order, "two", 50)
        ledger.claim_court_reward(order.offence_id, "two")
        self.assertEqual(ledger.state.court_credits[:len(paid)], paid)
        self.refusal("CourtRewardAlreadyFinalized", lambda: ledger.claim_court_reward(order.offence_id, "one"))
        self.refusal("CourtRecoveryCap", lambda: ledger.court_collect(order, "three", 1))
        ledger.conservation()

    def test_court_first_protects_49_escrow_bft_burns_only_remaining(self):
        ledger = r.RetirementLedger()
        native_lot(ledger, "collateral", "bad-model", "defendant", "BOND", "MINER", 100)
        order = court_order(ledger, "collateral", 100)
        ledger.court_collect(order, "first-collection", 40)
        escrow = ledger.state.court_reward_escrows
        receipt, _ = retire(ledger)
        self.assertEqual(receipt.burned_misaka_atoms, 60)
        self.assertEqual(ledger.state.court_reward_escrows, escrow)
        self.assertEqual(sum(row.remaining_misaka_atoms for row in escrow), 19)
        self.assertEqual(ledger.state.court_burned_misaka_atoms, 21)
        ledger.bind_court_plan(order.offence_id, order.recipients, True)
        ledger.claim_court_reward(order.offence_id, "first-collection")
        self.assertEqual(sum(row.misaka_atoms for row in ledger.state.court_credits), 19)
        ledger.conservation()

    def test_bft_first_same_collateral_court_recovery_and_reward_are_zero(self):
        ledger = r.RetirementLedger()
        native_lot(ledger, "collateral", "bad-model", "defendant", "BOND", "MINER", 100)
        order = court_order(ledger, "collateral", 100)
        receipt, _ = retire(ledger)
        self.assertEqual(receipt.burned_misaka_atoms, 100)
        ledger.court_collect(order, "zero-recovery", 0)
        self.assertEqual(ledger.state.court_debited_misaka_atoms, 0)
        self.assertEqual(ledger.state.court_recoveries[0].rewarded_misaka_atoms, 0)
        self.assertEqual(ledger.state.court_reward_escrows, ())
        prior = ledger.state
        self.refusal("CourtAmount", lambda: ledger.court_collect(order, "double-recovery", 1))
        self.assertEqual(ledger.state, prior)
        ledger.conservation()

    def test_court_u128_boundary_overflow_and_non_collateral_are_refused(self):
        ledger = r.RetirementLedger()
        native_lot(ledger, "largest", "bad-model", "defendant", "BOND", "GATE_C", math.U128_MAX)
        order = court_order(ledger, "largest", math.U128_MAX)
        ledger.court_collect(order, "largest", math.U128_MAX)
        expected_reward = math.U128_MAX * 49 // 100
        self.assertEqual(ledger.state.court_recoveries[0].rewarded_misaka_atoms, expected_reward)
        ledger.conservation()
        native_lot(ledger, "one-more", "bad-model", "defendant", "BOND", "GATE_C", 1)
        prior = ledger.state
        self.refusal("Overflow128", lambda: ledger.court_collect(
            court_order(ledger, "one-more", 1, "second-offence"), "overflow", 1))
        self.assertEqual(ledger.state, prior)
        base = fixture()
        for lot_id in ("pool", "unissued-miner", "pending-miner-cash"):
            before = base.state
            self.refusal("NotCourtCollateral", lambda key=lot_id: base.court_collect(
                court_order(base, key, 100), "cannot-fabricate-S:" + key, 0))
            self.assertEqual(base.state, before)


if __name__ == "__main__":
    unittest.main(verbosity=2)
