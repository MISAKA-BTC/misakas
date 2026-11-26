#!/usr/bin/env python3
"""Independent invariants and failure cases for the MOE3 design reference."""
from dataclasses import replace
from fractions import Fraction
import json
import random
import unittest

import moe3_reference as m

MEASUREMENTS = {}


def market(x=1000 * m.SCALE, inventory=10_000 * m.SCALE, holders=0, retired=0,
           virtual=0, seed=0, creator="creator", model_id="model"):
    return m.Market(model_id, virtual, x - virtual, seed, inventory, holders, retired,
                    inventory + holders + retired, creator)


def independent_expected(x, inventory, reward):
    # Direct exact-input output, independent of the implementation's ceil(K/X1).
    bft = reward // 20
    verification = reward // 20
    cash = reward // 5
    lp_cost = reward // 5
    miner_cost = reward - bft - verification - cash - lp_cost
    purchase = miner_cost + lp_cost
    q = (purchase * inventory) // (x + purchase)
    lp = (q * lp_cost) // purchase if purchase else 0
    miner = q - lp
    return purchase, cash, q, lp, miner


def check_invariants(test, before, reward, quote):
    b, a, q, ell, miner = independent_expected(before.x, before.inventory_position_atoms, reward)
    test.assertEqual((quote.purchase_misaka_atoms, quote.lp_misaka_atoms,
                      quote.gross_position_atoms, quote.lp_position_atoms, quote.miner_position_atoms),
                     (b, a, q, ell, miner))
    after = quote.after
    test.assertEqual(b + a + quote.bft_misaka_atoms + quote.verification_misaka_atoms, reward)
    test.assertEqual(ell + miner, q)
    if b:
        test.assertEqual(b * miner, quote.miner_purchase_misaka_atoms * q
                         + (q * quote.lp_purchase_misaka_atoms) % b)
    test.assertEqual(after.inventory_position_atoms, before.inventory_position_atoms - miner)
    test.assertEqual(after.holder_position_atoms, before.holder_position_atoms + miner)
    test.assertEqual(after.real_misaka_atoms, before.real_misaka_atoms + b + a)
    test.assertEqual(after.issued_position_atoms, after.inventory_position_atoms
                     + after.holder_position_atoms + after.retired_position_atoms)
    test.assertEqual(after.retired_position_atoms, before.retired_position_atoms)
    test.assertEqual(after.seed_locked_misaka_atoms, before.seed_locked_misaka_atoms)
    test.assertEqual(after.virtual_misaka_atoms, before.virtual_misaka_atoms)
    test.assertGreaterEqual(quote.buy_x * quote.buy_inventory_position_atoms,
                            before.x * before.inventory_position_atoms)
    test.assertGreaterEqual(after.x * after.inventory_position_atoms,
                            quote.buy_x * quote.buy_inventory_position_atoms)
    test.assertEqual(quote.buy_inventory_position_atoms, before.inventory_position_atoms - q)


class ReferenceTests(unittest.TestCase):
    def refusal(self, code, operation):
        with self.assertRaises(m.Refusal) as caught:
            operation()
        self.assertEqual(caught.exception.code, code)

    def test_final_50_20_20_5_5_example_replaces_old_80_percent(self):
        initial = market()
        quote = m.quote_reward(initial, 1000 * m.SCALE)
        check_invariants(self, initial, 1000 * m.SCALE, quote)
        self.assertEqual(quote.bft_misaka_atoms, 50 * m.SCALE)
        self.assertEqual(quote.verification_misaka_atoms, 50 * m.SCALE)
        self.assertEqual(quote.miner_purchase_misaka_atoms, 500 * m.SCALE)
        self.assertEqual(quote.lp_purchase_misaka_atoms, 200 * m.SCALE)
        self.assertEqual(quote.purchase_misaka_atoms, 700 * m.SCALE)
        self.assertEqual(quote.lp_misaka_atoms, 200 * m.SCALE)
        self.assertEqual(m.format_position(quote.gross_position_atoms), "4117.64705882")
        self.assertEqual(m.format_position(quote.lp_position_atoms), "1176.47058823")
        self.assertEqual(m.format_position(quote.miner_position_atoms), "2941.17647059")
        self.assertEqual(quote.after.real_misaka_atoms, 1900 * m.SCALE)
        self.assertEqual(m.format_position(quote.after.inventory_position_atoms), "7058.82352941")
        self.assertNotEqual(quote.lp_position_atoms, quote.gross_position_atoms // 4)
        self.assertNotEqual(quote.bft_misaka_atoms, 100 * m.SCALE)
        MEASUREMENTS["fixed_example"] = quote.to_json()

    def test_position_remainder_all_goes_to_miner(self):
        for q in range(4, 12):
            # X=1 and sufficiently large B gives exactly inventory-1 units.
            before = market(x=1, inventory=q + 1)
            quote = m.quote_reward(before, 100)
            self.assertEqual(quote.gross_position_atoms, q)
            self.assertEqual(quote.lp_position_atoms, 2 * q // 7)
            self.assertEqual(quote.miner_position_atoms, q - 2 * q // 7)
        quote = m.quote_reward(market(x=1, inventory=100), 1001)
        self.assertEqual((quote.purchase_misaka_atoms, quote.lp_misaka_atoms), (701, 200))
        self.assertEqual(quote.miner_purchase_misaka_atoms, 501)
        odd = m.quote_reward(market(x=1, inventory=100), 19)
        self.assertEqual(odd.lp_position_atoms, odd.gross_position_atoms * 3 // 16)
        self.assertNotEqual(odd.lp_position_atoms, odd.gross_position_atoms * 2 // 7)

    def test_zero_is_noop_no_receipt_no_consumption(self):
        book = m.ReferenceBook(market())
        before = book.state
        quote, undo = book.settle("zero", "miner", 0)
        self.assertEqual(quote.after, before.market)
        self.assertEqual(book.state, before)
        self.assertEqual(undo.before, undo.after)
        self.assertEqual(book.state.receipts, ())
        self.assertFalse(book.state.has_permanent_lp)
        self.assertNotIn("zero", book.state.consumed)

    def test_positive_dust_never_partially_commits(self):
        for inventory, reward in [(1000, 1), (1, 100), (2, 100), (3, 100), (4, 100), (10, 2)]:
            book = m.ReferenceBook(market(x=100, inventory=inventory))
            before = book.state
            self.refusal("LiquidityDust", lambda: book.settle("dust", "miner", reward))
            self.assertEqual(book.state, before)
        # At finite precision, even a very large budget cannot release 4 atoms
        # from an inventory of only 4, because the ceil curve retains >=1.
        for inventory in range(1, 5):
            self.refusal("LiquidityDust", lambda: m.quote_reward(market(x=1, inventory=inventory), 10**20))

    def test_min_out_is_net_miner_atoms_and_gross_is_checked(self):
        before = market()
        quote = m.quote_reward(before, 1000 * m.SCALE)
        exact = m.Limits(min_miner_position_atoms=quote.miner_position_atoms,
                         expected_gross_position_atoms=quote.gross_position_atoms)
        self.assertEqual(m.quote_reward(before, 1000 * m.SCALE, exact), quote)
        self.refusal("OutsideQuote", lambda: m.quote_reward(before, 1000 * m.SCALE,
                     replace(exact, min_miner_position_atoms=quote.miner_position_atoms + 1)))
        self.refusal("GrossQuoteMismatch", lambda: m.quote_reward(before, 1000 * m.SCALE,
                     replace(exact, expected_gross_position_atoms=quote.gross_position_atoms + 1)))
        for name, value in (("max_coinbase_misaka_atoms", quote.coinbase_misaka_atoms),
                            ("max_purchase_misaka_atoms", quote.purchase_misaka_atoms),
                            ("max_lp_misaka_atoms", quote.lp_misaka_atoms),
                            ("max_gross_position_atoms", quote.gross_position_atoms),
                            ("max_lp_position_atoms", quote.lp_position_atoms)):
            self.refusal("OutsideQuote", lambda n=name, v=value: m.quote_reward(
                before, 1000 * m.SCALE, replace(exact, **{n: v - 1})))

    def test_internal_operations_are_fee_free_for_every_model(self):
        values = []
        for name, creator in (("original-genesis", None), ("bft-added", "alice"),
                              ("former-fork", "alice"), ("automatic", "alice")):
            initial = market(model_id=name, creator=creator)
            quote = m.quote_reward(initial, 1000 * m.SCALE)
            values.append((quote.purchase_misaka_atoms, quote.lp_misaka_atoms,
                           quote.miner_position_atoms, quote.after.real_misaka_atoms))
            self.assertEqual(quote.after.real_misaka_atoms - initial.real_misaka_atoms, 900 * m.SCALE)
            self.assertEqual(quote.after.retired_position_atoms, initial.retired_position_atoms)
        self.assertEqual(len(set(values)), 1)
        self.assertEqual(m.ALLOCATION_COEFFICIENT, 1)

    def test_uniform_registration_including_creatorless_genesis(self):
        for model_id, creator in (("original-genesis", None), ("bft", "creator"), ("old-fork", "creator")):
            row = m.register_market(model_id, 10_000_000 * m.SCALE, creator)
            self.assertEqual(row.issued_position_atoms, 500_000 * m.SCALE)
            self.assertEqual(row.inventory_position_atoms, row.issued_position_atoms)
            self.assertEqual(row.real_misaka_atoms, 0)
            self.assertEqual(row.profile, "MOE3")
            self.assertEqual(row.creator, creator)
        self.refusal("EmptyCurve", lambda: m.register_market("invalid", 0))
        self.refusal("InvalidIdentity", lambda: m.register_market("invalid", 1, creator=""))

    def test_normal_fees_owner_present_and_absent(self):
        self.assertEqual(m.trade_fees(1000, True), m.Fees(50, 10, 940))
        self.assertEqual(m.trade_fees(1000, False), m.Fees(50, 0, 950))
        for gross in [*range(1001), m.U128_MAX]:
            for present in (False, True):
                fees = m.trade_fees(gross, present)
                self.assertEqual(sum((fees.burn_misaka_atoms, fees.creator_misaka_atoms, fees.net_misaka_atoms)), gross)
                self.assertEqual(fees.burn_misaka_atoms, gross // 20)
                self.assertEqual(fees.creator_misaka_atoms, gross // 100 if present else 0)
        self.refusal("InvalidCreatorPresence", lambda: m.trade_fees(1000, 1))

    def test_coinbase_five_legs_conserve_and_bft_is_only_five_percent_once(self):
        for c in [*range(1001), m.U128_MAX]:
            split = m.split_coinbase(c)
            self.assertEqual(split.bft_misaka_atoms, c // 20)
            self.assertEqual(split.verification_misaka_atoms, c // 20)
            self.assertEqual(split.lp_misaka_atoms, c // 5)
            self.assertEqual(split.lp_purchase_misaka_atoms, c // 5)
            self.assertEqual(split.bft_misaka_atoms + split.verification_misaka_atoms
                             + split.lp_misaka_atoms + split.lp_purchase_misaka_atoms
                             + split.miner_purchase_misaka_atoms, c)
        book = m.ReferenceBook(market())
        book.settle("one-budget", "miner", 1000 * m.SCALE)
        self.assertEqual(book.state.bft_reserved_misaka_atoms, 50 * m.SCALE)
        self.assertEqual(book.state.verification_reserved_misaka_atoms, 50 * m.SCALE)
        self.assertEqual(book.state.coinbase_debited_misaka_atoms, 1000 * m.SCALE)
        self.assertEqual(book.state.market_reserved_misaka_atoms, 0)

    def test_verification_plan_is_misaka_and_not_invented_half_split(self):
        allocation = (m.VerificationAllocation("PANEL", "panel-payee", 40 * m.SCALE),
                      m.VerificationAllocation("GATE_C", "gate-c-payee", 10 * m.SCALE))
        book = m.ReferenceBook(market())
        book.allocate_budget("funded", "miner", 1000 * m.SCALE, allocation)
        book.settle_market("funded")
        self.assertEqual(book.state.receipts[0].verification_allocations, allocation)
        self.assertEqual(dict(book.state.miner_balances).keys(), {"miner"})
        self.assertEqual(book.state.verification_reserved_misaka_atoms, 50 * m.SCALE)
        other = m.ReferenceBook(market())
        other.allocate_budget("reserved-only", "miner", 1000 * m.SCALE)
        self.assertIsNone(other.state.allocations[0].verification_allocations)
        before = other.state
        self.refusal("VerificationPlanTotal", lambda: other.allocate_budget("bad", "miner", 1000 * m.SCALE,
                     (m.VerificationAllocation("PANEL", "payee", 49 * m.SCALE),)))
        self.assertEqual(other.state, before)
        self.refusal("InvalidVerificationRole", lambda: other.allocate_budget("bad", "miner", 1000 * m.SCALE,
                     (m.VerificationAllocation("MINER", "payee", 50 * m.SCALE),)))
        self.assertEqual(other.state, before)

    def test_market_refusal_preserves_prior_verification_and_bft_rights(self):
        book = m.ReferenceBook(market(x=100, inventory=3))
        book.allocate_budget("earned-services", "miner", 1000 * m.SCALE)
        funded = book.state
        self.assertEqual(funded.bft_reserved_misaka_atoms, 50 * m.SCALE)
        self.assertEqual(funded.verification_reserved_misaka_atoms, 50 * m.SCALE)
        self.assertEqual(funded.market_reserved_misaka_atoms, 900 * m.SCALE)
        self.refusal("LiquidityDust", lambda: book.settle_market("earned-services"))
        self.assertEqual(book.state, funded)
        self.refusal("AlreadyAllocated", lambda: book.allocate_budget("earned-services", "miner", 1000 * m.SCALE))
        self.assertEqual(book.state, funded)
        successful = m.ReferenceBook(market())
        successful.allocate_budget("reserved", "miner", 1000 * m.SCALE)
        funded = successful.state
        self.refusal("InjectedFailure", lambda: successful.settle_market("reserved", fail_after="receipt"))
        self.assertEqual(successful.state, funded)

    def test_coupled_model_budget_prevents_insufficient_miner_cost_counterexample(self):
        split = m.split_coinbase(21)
        weights = (("a", 1), ("b", 4), ("c", 4), ("d", 4))
        independent_d = dict(m.apportion(split.miner_purchase_misaka_atoms, weights))
        independent_j = dict(m.apportion(split.lp_purchase_misaka_atoms, weights))
        self.assertEqual((independent_d["a"], independent_j["a"]), (1, 1))
        coupled = m.coupled_model_budgets(split, weights)
        self.assertEqual([(row.model_id, row.miner_purchase_misaka_atoms, row.lp_purchase_misaka_atoms)
                          for row in coupled], [("a", 2, 1), ("b", 3, 1), ("c", 3, 1), ("d", 3, 1)])
        self.assertEqual(m.coupled_model_budgets(split, tuple(reversed(weights))), coupled)
        MEASUREMENTS["coupled_counterexample"] = {
            "coinbase_misaka_atoms": "21", "weights": {key: str(value) for key, value in weights},
            "independent_a": {"miner_cost": "1", "lp_position_cost": "1"},
            "coupled": [{"model": row.model_id, "miner_cost": str(row.miner_purchase_misaka_atoms),
                         "lp_position_cost": str(row.lp_purchase_misaka_atoms)} for row in coupled],
        }

    def test_coupled_apportionment_conserves_global_legs_and_tie_order(self):
        weights = (("c", 4), ("zero", 0), ("a", 1), ("b", 4))
        for c in range(1001):
            split = m.split_coinbase(c)
            rows = m.coupled_model_budgets(split, weights)
            self.assertEqual(sum(row.miner_purchase_misaka_atoms for row in rows), split.miner_purchase_misaka_atoms)
            self.assertEqual(sum(row.lp_purchase_misaka_atoms for row in rows), split.lp_purchase_misaka_atoms)
            self.assertEqual(sum(row.lp_misaka_atoms for row in rows), split.lp_misaka_atoms)
            self.assertEqual(sum(row.purchase_misaka_atoms for row in rows), split.purchase_misaka_atoms)
            for row in rows:
                self.assertEqual(row.lp_misaka_atoms, row.lp_purchase_misaka_atoms)
                self.assertGreaterEqual(row.miner_purchase_misaka_atoms, 2 * row.lp_purchase_misaka_atoms)
                if row.model_id == "zero":
                    self.assertEqual(row.purchase_misaka_atoms, 0)
        self.assertEqual(m.apportion(1, (("b", 1), ("a", 1))), (("a", 1), ("b", 0)))
        self.refusal("DuplicateModel", lambda: m.apportion(1, (("a", 1), ("a", 2))))
        self.refusal("ZeroWeight", lambda: m.apportion(1, (("a", 0),)))

    def test_decimal_codec_roundtrip_and_json_precision(self):
        for text in ("0", "1", "0.00000001", "12.34000000", "4444.44444444"):
            atoms = m.parse_position(text)
            self.assertEqual(m.parse_position(m.format_position(atoms)), atoms)
        self.assertEqual(m.parse_position("1.23456789"), 123456789)
        self.assertEqual(m.parse_position(m.format_position(m.U128_MAX)), m.U128_MAX)
        for atoms in (0, 1, 2**53 + 1, m.U128_MAX):
            encoded = json.dumps({"atoms": m.atoms_json(atoms)})
            decoded = json.loads(encoded)["atoms"]
            self.assertIsInstance(decoded, str)
            self.assertEqual(m.parse_atoms_json(decoded), atoms)
        quote_json = m.quote_reward(market(), 1000 * m.SCALE).to_json()
        self.assertTrue(all(isinstance(v, str) for k, v in quote_json.items() if k != "after"))

    def test_decimal_codec_rejects_lossy_noncanonical_inputs(self):
        for value in (1.0, 1, True, None, "1e2", "NaN", "Infinity", "-1", "+1", " 1", "1 ",
                      ".1", "1.", "01", "00.5", "1.000000001", "１", "1_000", "1\n"):
            self.refusal("InvalidPositionDecimal", lambda v=value: m.parse_position(v))
        for value in (1, 1.0, "-1", "1e2", "01", "0.1", "1\n"):
            self.refusal("InvalidAtomicString", lambda v=value: m.parse_atoms_json(v))
        self.refusal("Overflow128", lambda: m.parse_atoms_json(str(m.U128_MAX + 1)))
        self.refusal("Overflow128", lambda: m.parse_position("9" * 1000))
        self.refusal("InvalidUnsignedInteger", lambda: m.format_position(1.0))

    def test_unsigned_and_wide_arithmetic_boundaries(self):
        for value in (-1, True, 1.0, "1", None):
            self.refusal("InvalidUnsignedInteger", lambda v=value: m.u128(v))
        self.assertEqual(m.u128(m.U128_MAX), m.U128_MAX)
        self.refusal("Overflow128", lambda: m.add128(m.U128_MAX, 1))
        self.assertEqual(m.mul256(m.U128_MAX, m.U128_MAX), m.U128_MAX**2)
        self.refusal("Overflow256", lambda: m.mul256(m.U256_MAX, 2))
        self.assertEqual(m.ceildiv256(m.U256_MAX, m.U256_MAX), 1)
        self.assertEqual(m.ceildiv256(m.U256_MAX - 1, m.U256_MAX), 1)
        self.refusal("InvalidDenominator", lambda: m.ceildiv256(1, 0))
        self.refusal("Overflow128", lambda: replace(market(), virtual_misaka_atoms=m.U128_MAX).validate())

    def test_quote_overflows_and_invalid_values_leave_state_unchanged(self):
        cases = [
            (market(x=m.U128_MAX - 10, inventory=10**12), 1000),
            (market(x=m.U128_MAX // 2, inventory=10**12), m.U128_MAX * 7 // 10),
        ]
        for initial, reward in cases:
            book = m.ReferenceBook(initial)
            before = book.state
            self.refusal("Overflow128", lambda: book.settle("overflow", "miner", reward))
            self.assertEqual(book.state, before)
        book = m.ReferenceBook(market())
        before = book.state
        for value in (-1, 1.0, True):
            self.refusal("InvalidUnsignedInteger", lambda v=value: book.settle("bad", "miner", v))
            self.assertEqual(book.state, before)
        self.refusal("Overflow128", lambda: book.settle("too-big", "miner", m.U128_MAX + 1))
        self.assertEqual(book.state, before)
        self.refusal("UnknownProfile", lambda: book.settle("unknown", "miner", 10**10, profile="MOE2"))
        self.assertEqual(book.state, before)

    def test_valid_u128_boundary_is_not_accidentally_truncated(self):
        split = m.split_coinbase(m.U128_MAX)
        before = market(x=m.U128_MAX - split.purchase_misaka_atoms - split.lp_misaka_atoms,
                        inventory=m.U128_MAX)
        quote = m.quote_reward(before, m.U128_MAX)
        check_invariants(self, before, m.U128_MAX, quote)
        self.assertEqual(quote.after.real_misaka_atoms, m.U128_MAX)
        self.assertEqual(quote.after.issued_position_atoms, m.U128_MAX)
        max_whole = m.U128_MAX // m.SCALE
        legacy = m.LegacyWholeMarket("largest-scaleable", 1, 0, 0, max_whole, 0, 0, max_whole)
        converted = m.MigrationBook().migrate(legacy)
        self.assertEqual(converted.issued_position_atoms, max_whole * m.SCALE)

    def test_atomicity_at_each_failure_point(self):
        for point in ("quote", "miner", "cash", "receipt"):
            book = m.ReferenceBook(market())
            before = book.state
            self.refusal("InjectedFailure", lambda: book.settle("attempt", "miner", 1000 * m.SCALE, fail_after=point))
            self.assertEqual(book.state, before)
            book.settle("attempt", "miner", 1000 * m.SCALE)
            self.assertEqual(len(book.state.receipts), 1)

    def test_single_settlement_and_receipt_is_not_a_balance(self):
        book = m.ReferenceBook(market())
        initial = book.state
        quote, _ = book.settle("claim", "miner", 1000 * m.SCALE)
        after = book.state
        self.assertEqual(dict(after.miner_balances)["miner"], quote.miner_position_atoms)
        self.assertEqual(after.market.real_misaka_atoms - initial.market.real_misaka_atoms
                         + after.verification_reserved_misaka_atoms + after.bft_reserved_misaka_atoms,
                         after.coinbase_debited_misaka_atoms)
        self.assertEqual(after.market.issued_position_atoms, after.market.inventory_position_atoms
                         + after.market.holder_position_atoms + after.market.retired_position_atoms)
        self.assertNotEqual(after.market.issued_position_atoms, after.market.inventory_position_atoms
                            + after.market.holder_position_atoms + after.market.retired_position_atoms
                            + after.receipts[0].lp_position_atoms)
        self.assertTrue(after.has_permanent_lp)
        self.refusal("AlreadySettled", lambda: book.settle("claim", "other-miner", 1000 * m.SCALE))
        self.assertEqual(book.state, after)

    def test_undo_replay_restores_all_economic_state_together(self):
        book = m.ReferenceBook(market())
        initial = book.state
        first_quote, first = book.settle("one", "a", 1000 * m.SCALE)
        after_first = book.state
        _, second = book.settle("two", "b", 1000 * m.SCALE)
        after_second = book.state
        self.refusal("OutOfOrderUndo", lambda: book.undo(first))
        self.assertEqual(book.state, after_second)
        book.undo(second)
        self.assertEqual(book.state, after_first)
        book.undo(first)
        self.assertEqual(book.state, initial)
        replay_quote, _ = book.settle("one", "a", 1000 * m.SCALE)
        self.assertEqual(replay_quote, first_quote)
        self.assertEqual(book.state, after_first)

    def test_one_time_migration_preserves_supply_and_price_units(self):
        legacy = m.LegacyWholeMarket("old", 1000, 500, 100, 700, 200, 100, 1000, "creator")
        book = m.MigrationBook()
        converted = book.migrate(legacy)
        self.assertEqual(converted.inventory_position_atoms, 700 * m.SCALE)
        self.assertEqual(converted.holder_position_atoms, 200 * m.SCALE)
        self.assertEqual(converted.retired_position_atoms, 100 * m.SCALE)
        self.assertEqual(converted.issued_position_atoms, 1000 * m.SCALE)
        self.assertEqual((converted.virtual_misaka_atoms, converted.real_misaka_atoms,
                          converted.seed_locked_misaka_atoms), (1000, 500, 100))
        self.assertEqual(Fraction(1500, legacy.inventory_whole),
                         Fraction(converted.x * m.SCALE, converted.inventory_position_atoms))
        self.assertEqual(converted.migrated_from, "MOE2")
        self.refusal("AlreadyMigrated", lambda: book.migrate(legacy))
        self.refusal("AlreadyMigrated", lambda: m.MigrationBook().migrate(converted))

    def test_migration_refuses_pending_allowances_unknown_and_overflow(self):
        legacy = m.LegacyWholeMarket("old", 1000, 500, 100, 700, 200, 100, 1000)
        for value, code in ((replace(legacy, pending_old_claims=1), "PendingOldClaims"),
                            (replace(legacy, live_old_allowances=1), "LiveOldAllowances"),
                            (replace(legacy, source_profile="unknown"), "UnknownProfile"),
                            (replace(legacy, inventory_whole=m.U128_MAX, issued_whole=m.U128_MAX), "Overflow128"),
                            (replace(legacy, issued_whole=999), "SupplyMismatch")):
            book = m.MigrationBook()
            self.refusal(code, lambda v=value: book.migrate(v))
            self.assertEqual(book.markets, {})

    def test_fixed_lp_can_reduce_post_buy_spot_and_nominal_is_not_price(self):
        quote = m.quote_reward(market(), 1000 * m.SCALE)
        after_buy_price = Fraction(quote.buy_x, quote.buy_inventory_position_atoms)
        after_lp_price = Fraction(quote.after.x, quote.after.inventory_position_atoms)
        self.assertLess(after_lp_price, after_buy_price)
        proportional_lp = quote.lp_misaka_atoms * quote.buy_inventory_position_atoms // quote.buy_x
        self.assertNotEqual(quote.lp_position_atoms, proportional_lp)
        # A hypothetical immediate curve sale: this is a counterexample, not a payout promise.
        k = quote.after.x * quote.after.inventory_position_atoms
        returned_inventory = quote.after.inventory_position_atoms + quote.miner_position_atoms
        cash_after_sell = (k + returned_inventory - 1) // returned_inventory
        gross = quote.after.x - cash_after_sell
        nominal_cost = quote.miner_purchase_misaka_atoms
        self.assertNotEqual(gross, nominal_cost)
        self.assertEqual(nominal_cost, 500 * m.SCALE)
        MEASUREMENTS["price_counterexample"] = {
            "buy_spot_misaka_per_position": str(after_buy_price),
            "lp_spot_misaka_per_position": str(after_lp_price),
            "proportional_lp_position_atoms_not_used": str(proportional_lp),
            "nominal_miner_cost_misaka_atoms": str(nominal_cost),
            "hypothetical_immediate_gross_sale_misaka_atoms": str(gross),
            "hypothetical_creator_present_net_sale_misaka_atoms": str(m.trade_fees(gross, True).net_misaka_atoms),
        }

    def test_small_exhaustive_against_independent_exact_input_formula(self):
        checked = successful = dust = zero = 0
        for x in range(1, 13):
            for inventory in range(1, 21):
                for reward in range(33):
                    checked += 1
                    before = market(x=x, inventory=inventory, holders=3, retired=2)
                    b, a, q, ell, miner = independent_expected(x, inventory, reward)
                    expected_dust = reward > 0 and not all((b, a, q, ell, miner))
                    if expected_dust:
                        dust += 1
                        self.refusal("LiquidityDust", lambda: m.quote_reward(before, reward))
                    else:
                        quote = m.quote_reward(before, reward)
                        check_invariants(self, before, reward, quote)
                        successful += reward > 0
                        zero += reward == 0
        MEASUREMENTS["exhaustive"] = {"cases": checked, "positive_success": successful,
                                       "dust_rejected": dust, "zero_noop": zero,
                                       "domain": "X=1..12, inventory=1..20 atoms, C=0..32 atoms"}

    def test_seeded_random_preservation_with_virtual_reserve_and_retire_history(self):
        seed = 20261126
        rng = random.Random(seed)
        successful = dust = 0
        for _ in range(600):
            virtual = rng.randrange(10**15)
            real = rng.randrange(1, 10**15)
            before = market(x=virtual + real, inventory=rng.randrange(1, 10**15),
                            holders=rng.randrange(10**15), retired=rng.randrange(10**12),
                            virtual=virtual, seed=rng.randrange(real + 1))
            reward = rng.randrange(1, 10**15)
            try:
                quote = m.quote_reward(before, reward)
            except m.Refusal as error:
                self.assertEqual(error.code, "LiquidityDust")
                dust += 1
            else:
                successful += 1
                check_invariants(self, before, reward, quote)
        MEASUREMENTS["seeded_random"] = {"seed": seed, "cases": 600, "positive_success": successful,
                                         "dust_rejected": dust, "amount_range": "0..10^15 atoms"}


if __name__ == "__main__":
    unittest.main(verbosity=2)
