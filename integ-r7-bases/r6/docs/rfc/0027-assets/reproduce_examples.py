#!/usr/bin/env python3
"""RFC-0027 illustrative integer accounting only; NOT protocol/node implementation.

Run from any directory:
  python3 reproduce_examples.py --json-out examples-results.json
No external packages, network, market data or timing assumptions are required.
The 12,000-coin liability and every cash-flow value are synthetic, not tariffs.
"""
from __future__ import annotations

import argparse
import json
import unittest
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable

COIN = 100_000_000
MINIMUM = 100_000 * COIN
EXAMPLE_LIABILITY = 12_000 * COIN
MAX_AMOUNT = (1 << 128) - 1


def amount(value: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError("amount must be an integer in sompi")
    if value < 0 or value > MAX_AMOUNT:
        raise ValueError("amount outside the illustrative unsigned 128-bit range")
    return value


def checked_sum(values: Iterable[int]) -> int:
    total = 0
    for value in values:
        total = amount(total + amount(value))
    return total


def required(liability: int, minimum: int = MINIMUM) -> int:
    return max(amount(minimum), amount(liability))


def top_up(balance: int, liability: int) -> int:
    return max(0, required(liability) - amount(balance))


def production_capital(reservations: int, utilization_permille: int) -> int:
    """Only one illustrative legacy constraint; not a full admission quote."""
    amount(reservations)
    if not 1 <= utilization_permille <= 1000:
        raise ValueError("utilization must be in [1, 1000]")
    return amount((reservations * 1000 + utilization_permille - 1) // utilization_permille)


def reimbursement_due(paid: int, already_reimbursed: int, cap: int) -> int:
    ceiling = min(amount(paid), amount(cap))
    amount(already_reimbursed)
    if already_reimbursed > ceiling:
        raise ValueError("existing reimbursement exceeds the cumulative ceiling")
    return ceiling - already_reimbursed


# Tuple fields are conceptual scope labels, not a real wire identity.
Scope = tuple[str, str, str, str]


@dataclass
class IllustrationAccount:
    balance: int
    rows: dict[Scope, int] = field(default_factory=dict)

    @property
    def liability(self) -> int:
        return checked_sum(self.rows.values())

    def admit(self, identity: Scope, cap: int) -> None:
        amount(cap)
        if identity in self.rows:
            if self.rows[identity] != cap:
                raise ValueError("same identity cannot silently change its cap")
            return
        candidate = amount(self.liability + cap)
        if self.balance < required(candidate):
            raise ValueError("insufficient account backing; no partial state update")
        self.rows[identity] = cap

    def settle_terminal(self, identity: Scope, debit: int) -> None:
        """Caller assumes ALL responsibility of this row has legally terminated.

        This is deliberately not a deadline/Final implementation. The example has
        no unresolved post-Final liability and must not be used to release it.
        """
        amount(debit)
        cap = self.rows[identity]
        if debit > cap or debit > self.balance:
            raise ValueError("debit exceeds this row's guarantee")
        remaining = self.liability - cap
        new_balance = self.balance - debit
        if new_balance < remaining:
            raise ValueError("would consume another row's backing")
        self.balance = new_balance
        del self.rows[identity]


def scope(index: int = 0, root: str = "root-A") -> Scope:
    return (f"claim-{index}", "illustration-only", root, "one-terminal-obligation")


def cashflow(conviction: bool) -> dict[str, int]:
    """Actual balances, in coins for the explanatory table.

    Requester prepays 6,000, of which 4,000 funds completed work. At conviction,
    a 12,000 account debit reimburses 4,000 to the requester, gives a fixed
    2,000 bounty to the Panel, and burns 6,000. No second work payment occurs.
    """
    balances = {"miner_account": 100_000, "requester": 6_000,
                "fee_escrow": 0, "panel": 0, "burned": 0}
    balances["requester"] -= 6_000
    balances["fee_escrow"] += 6_000
    balances["fee_escrow"] -= 4_000
    balances["panel"] += 4_000
    if conviction:
        balances["miner_account"] -= 12_000
        balances["requester"] += 4_000
        balances["panel"] += 2_000
        balances["burned"] += 6_000
    balances["requester"] += balances["fee_escrow"]
    balances["fee_escrow"] = 0
    return balances


class ExampleTests(unittest.TestCase):
    def test_minimum_is_a_floor_not_an_extra_fee(self):
        self.assertEqual(required(0), MINIMUM)
        self.assertEqual(required(MINIMUM - 1), MINIMUM)
        self.assertEqual(required(MINIMUM), MINIMUM)
        self.assertEqual(required(MINIMUM + 1), MINIMUM + 1)

    def test_table(self):
        for n, expected in [(1, 100_000), (5, 100_000), (8, 100_000),
                            (9, 108_000), (20, 240_000)]:
            self.assertEqual(required(n * EXAMPLE_LIABILITY), expected * COIN)

    def test_exact_top_up(self):
        self.assertEqual(top_up(MINIMUM, 8 * EXAMPLE_LIABILITY), 0)
        self.assertEqual(top_up(MINIMUM, 9 * EXAMPLE_LIABILITY), 8_000 * COIN)
        self.assertEqual(top_up(99_999 * COIN, 0), COIN)

    def test_duplicate_does_not_add_liability(self):
        a = IllustrationAccount(MINIMUM)
        a.admit(scope(), EXAMPLE_LIABILITY)
        a.admit(scope(), EXAMPLE_LIABILITY)
        self.assertEqual(a.liability, EXAMPLE_LIABILITY)

    def test_other_root_cannot_reuse_liability(self):
        a = IllustrationAccount(MINIMUM)
        a.admit(scope(), EXAMPLE_LIABILITY)
        a.admit(scope(root="root-B"), EXAMPLE_LIABILITY)
        self.assertEqual(a.liability, 2 * EXAMPLE_LIABILITY)

    def test_duplicate_with_changed_cap_is_rejected(self):
        a = IllustrationAccount(MINIMUM)
        a.admit(scope(), EXAMPLE_LIABILITY)
        with self.assertRaises(ValueError):
            a.admit(scope(), EXAMPLE_LIABILITY + 1)
        self.assertEqual(a.liability, EXAMPLE_LIABILITY)

    def test_rejected_admission_is_atomic(self):
        a = IllustrationAccount(MINIMUM)
        for i in range(8):
            a.admit(scope(i), EXAMPLE_LIABILITY)
        before = dict(a.rows)
        with self.assertRaises(ValueError):
            a.admit(scope(8), EXAMPLE_LIABILITY)
        self.assertEqual(a.rows, before)
        self.assertEqual(a.balance, MINIMUM)

    def test_all_claims_can_be_debited_simultaneously(self):
        a = IllustrationAccount(240_000 * COIN)
        for i in range(20):
            a.admit(scope(i), EXAMPLE_LIABILITY)
        for i in range(20):
            a.settle_terminal(scope(i), EXAMPLE_LIABILITY)
            self.assertGreaterEqual(a.balance, a.liability)
        self.assertEqual(a.balance, 0)
        self.assertEqual(a.liability, 0)

    def test_one_claim_cannot_take_whole_account(self):
        a = IllustrationAccount(MINIMUM)
        a.admit(scope(), EXAMPLE_LIABILITY)
        with self.assertRaises(ValueError):
            a.settle_terminal(scope(), MINIMUM)
        self.assertEqual(a.balance, MINIMUM)

    def test_after_slash_floor_can_block_new_admission(self):
        a = IllustrationAccount(MINIMUM)
        a.admit(scope(), EXAMPLE_LIABILITY)
        a.settle_terminal(scope(), EXAMPLE_LIABILITY)
        self.assertEqual(a.balance, 88_000 * COIN)
        with self.assertRaises(ValueError):
            a.admit(scope(1), EXAMPLE_LIABILITY)

    def test_legacy_utilization_is_not_ignored(self):
        p = production_capital(15_000 * COIN, 500)
        self.assertEqual(p, 30_000 * COIN)
        self.assertEqual(MINIMUM + p, 130_000 * COIN)
        self.assertEqual(production_capital(1, 500), 2)

    def test_reimbursement_is_cumulative_not_repeatable(self):
        self.assertEqual(reimbursement_due(4_000, 0, 5_000), 4_000)
        self.assertEqual(reimbursement_due(4_000, 4_000, 5_000), 0)
        with self.assertRaises(ValueError):
            reimbursement_due(4_000, 4_001, 5_000)

    def test_conviction_flow_conserves_money(self):
        balances = cashflow(True)
        self.assertEqual(sum(balances.values()), 106_000)
        self.assertEqual(balances["requester"], 6_000)
        self.assertEqual(balances["panel"], 6_000)
        self.assertEqual(balances["miner_account"], 88_000)

    def test_dismissal_does_not_charge_honest_miner(self):
        balances = cashflow(False)
        self.assertEqual(sum(balances.values()), 106_000)
        self.assertEqual(balances["miner_account"], 100_000)
        self.assertEqual(balances["requester"], 2_000)
        self.assertEqual(balances["panel"], 4_000)

    def test_self_report_coalition_cannot_recover_burn(self):
        balances = cashflow(True)
        controlled = sum(value for name, value in balances.items() if name != "burned")
        self.assertEqual(controlled - 106_000, -6_000)

    def test_invalid_amounts_and_overflow_are_rejected(self):
        for x in [-1, MAX_AMOUNT + 1, 1.5, True]:
            with self.assertRaises((ValueError, TypeError)):
                required(x)
        with self.assertRaises(ValueError):
            checked_sum([MAX_AMOUNT, 1])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ExampleTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    rows = []
    for n in [1, 5, 8, 9, 20]:
        liability = n * EXAMPLE_LIABILITY
        rows.append({"outstanding_claims": n, "account_liability_coins": liability // COIN,
                     "account_required_coins": required(liability) // COIN,
                     "extra_above_100k_coins": top_up(MINIMUM, liability) // COIN})
    sensitivity = []
    for daily in [1, 5, 10, 30]:
        # Synthetic two-day account/legacy hold, NOT an adopted liability horizon.
        outstanding = daily * 2
        account = required(outstanding * EXAMPLE_LIABILITY)
        p = production_capital(outstanding * 3_000 * COIN, 500)
        sensitivity.append({"claims_per_day_assumed": daily, "hold_days_assumed": 2,
                            "outstanding_claims_assumed": outstanding,
                            "account_coins": account // COIN, "production_coins": p // COIN,
                            "total_two_buckets_coins": (account + p) // COIN})
    report = {"scope": "SYNTHETIC_ARITHMETIC_ONLY_NOT_NODE_OR_ECONOMIC_VALIDATION",
              "tests_run": result.testsRun, "passed": result.wasSuccessful(),
              "failures": len(result.failures), "errors": len(result.errors),
              "minimum_sompi": MINIMUM,
              "illustrative_liability_per_claim_coins": 12_000,
              "account_table": rows, "conviction_cashflow_coins": cashflow(True),
              "dismissal_cashflow_coins": cashflow(False),
              "synthetic_capital_sensitivity": sensitivity,
              "unverified": ["real tariff costs", "panel profitability", "miner participation",
                             "REAL and heartbeat rates", "all legal branches of real profiles",
                             "F02/F03 inclusion assumptions", "node consensus and reorg/IBD"]}
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    else:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
