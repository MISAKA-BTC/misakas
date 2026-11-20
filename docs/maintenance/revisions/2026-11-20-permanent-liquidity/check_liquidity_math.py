#!/usr/bin/env python3
"""MOE2 arithmetic/reference-state checks; standard library only.

This is not a native/runtime, cryptographic, liveness, MEV, or profitability test.
The mathematical proof is in MATH_PROOF.md. Integer probes supplement that proof.
Writes only liquidity-math-check.json (or the explicitly supplied --output).
"""

from __future__ import annotations

import argparse
import copy
import json
import random
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path


def ceildiv(n, d):
    if n < 0 or d <= 0:
        raise ValueError("invalid nonnegative ceil division")
    return (n + d - 1) // d


def quote(X, U, R, arithmetic_limit=None):
    if not all(isinstance(v, int) for v in (X, U, R)) or X <= 0 or U < 1 or R < 0:
        raise ValueError("invalid market or reward")
    B = (300 * R) // 1000
    A = (200 * R) // 1000
    D = R - B - A
    X1 = X + B
    U1 = ceildiv(X * U, X1)
    q = U - U1
    ell = (A * U1) // X1
    burned = q - ell
    X2, U2 = X1 + A, U1 + ell
    products = (300 * R, 200 * R, X * U, A * U1, X2 * U2)
    if arithmetic_limit is not None:
        values = (X, U, R, B, A, D, X1, U1, q, ell, burned, X2, U2, *products)
        if any(v > arithmetic_limit for v in values):
            raise OverflowError("checked arithmetic bound exceeded")
    dust = (B > 0 and q == 0) or (A > 0 and ell == 0)
    return {"B": B, "A": A, "D": D, "X1": X1, "U1": U1,
            "q": q, "ell": ell, "burn": burned, "X2": X2, "U2": U2,
            "K1": X1 * U1, "K2": X2 * U2, "LiquidityDust": dust}


def assert_arithmetic(X, U, R, H=0):
    z = quote(X, U, R)
    B, A, D = z["B"], z["A"], z["D"]
    q, ell, burned = z["q"], z["ell"], z["burn"]
    U1, X1, U2, X2 = z["U1"], z["X1"], z["U2"], z["X2"]
    assert R == B + A + D and 0 <= A <= B and D >= 0
    assert q == (B * U) // X1
    assert 0 <= ell <= q <= U - 1
    assert U2 == U - burned and 1 <= U2 <= U
    assert X2 == X + B + A
    assert z["K2"] >= z["K1"] >= X * U
    remainder = A * U1 - ell * X1
    assert 0 <= remainder < X1
    assert X2 * U1 - X1 * U2 == remainder
    # delta = remainder / (X1 * U2), so 0 <= delta < 1/U2.
    assert X2 * U1 >= X1 * U2
    T, T2 = U + H, U2 + H
    assert T2 == T - burned and 1 <= T2 <= T
    assert ceildiv(z["K2"], T2) * T >= ceildiv(X * U, T) * T2
    if U <= 2:
        assert ell == 0
    return z


@dataclass
class Market:
    V: int
    seed_locked: int
    supply: int
    model_id: str = "additional-model"
    artifact_root: str = "fixed-root"
    r: int = field(init=False)
    U: int = field(init=False)
    retired: int = 0
    holders: dict = field(default_factory=dict)
    receipts: dict = field(default_factory=dict)
    settled: set = field(default_factory=set)
    paid_worker_residual: int = 0
    successful_reward_total: int = 0
    user_buy_gross: int = 0
    user_sell_net: int = 0
    fee_burn: int = 0
    fee_owner: int = 0
    has_permanent_lp: bool = False
    protected_roots: set = field(default_factory=set)

    def __post_init__(self):
        self.r = self.seed_locked
        self.U = self.supply
        if self.V + self.r <= 0 or self.U < 1:
            raise ValueError("invalid initial market")

    @property
    def X(self):
        return self.V + self.r

    def validate(self):
        assert self.U >= 1 and self.r >= self.seed_locked >= 0
        assert all(v >= 0 for v in self.holders.values())
        assert self.U + sum(self.holders.values()) + self.retired == self.supply
        assert self.X * self.U >= (self.V + self.seed_locked) * self.supply
        assert (self.seed_locked + self.user_buy_gross + self.successful_reward_total
                == self.r + self.user_sell_net + self.fee_burn + self.fee_owner
                + self.paid_worker_residual)
        assert self.has_permanent_lp == bool(self.receipts)
        if self.has_permanent_lp:
            assert self.artifact_root in self.protected_roots
        # Receipts are history, never an additional Position-supply term.
        for rec in self.receipts.values():
            assert rec["A"] > 0 and rec["ell"] > 0
            assert not rec["redeemable"] and rec["owner"] is None

    def buy(self, holder, gross):
        if gross < 0:
            return "Invalid"
        b, o = (50 * gross) // 1000, (30 * gross) // 1000
        net = gross - b - o
        next_U = ceildiv(self.X * self.U, self.X + net)
        out = self.U - next_U
        if out <= 0:
            return "ZeroOutput"
        self.U = next_U
        self.r += net
        self.holders[holder] = self.holders.get(holder, 0) + out
        self.user_buy_gross += gross
        self.fee_burn += b
        self.fee_owner += o
        self.validate()
        return "Bought"

    def sell(self, holder, units):
        if units <= 0 or units > self.holders.get(holder, 0):
            return "Invalid"
        next_U = self.U + units
        next_X = ceildiv(self.X * self.U, next_U)
        gross = self.X - next_X
        b, o = (50 * gross) // 1000, (30 * gross) // 1000
        net = gross - b - o
        if net <= 0 or gross > self.r - self.seed_locked:
            return "UnfundedOrZeroOutput"
        self.U = next_U
        self.r -= gross
        self.holders[holder] -= units
        self.user_sell_net += net
        self.fee_burn += b
        self.fee_owner += o
        self.validate()
        return "Sold"

    def transfer(self, sender, receiver, units):
        if units < 0 or units > self.holders.get(sender, 0):
            return "Invalid"
        if sender != receiver:
            self.holders[sender] = self.holders.get(sender, 0) - units
            self.holders[receiver] = self.holders.get(receiver, 0) + units
        self.validate()
        return "Transferred"

    def settle(self, claim_id, R, now=0, original_deadline=10, arithmetic_limit=None):
        # The fixture uses an inclusive original deadline. It never resets it.
        if claim_id in self.settled:
            return "AlreadySettled"
        if now > original_deadline:
            return "ExpiredUnsuccessful"
        try:
            z = quote(self.X, self.U, R, arithmetic_limit)
        except (ValueError, OverflowError):
            return "InvalidOrOverflow"
        if z["LiquidityDust"]:
            return "LiquidityDust"
        old_floor_n, old_floor_d = ceildiv(self.X * self.U, self.supply - self.retired), self.supply - self.retired
        seed_before = self.seed_locked
        self.r += z["B"] + z["A"]
        self.U = z["U2"]
        self.retired += z["burn"]
        self.paid_worker_residual += z["D"]
        self.successful_reward_total += R
        self.settled.add(claim_id)
        if z["A"] > 0:
            self.receipts[claim_id] = {
                "A": z["A"], "ell": z["ell"], "q": z["q"],
                "burn": z["burn"], "owner": None, "redeemable": False,
                "transferable": False, "fee_claim": False,
            }
            self.has_permanent_lp = True
            self.protected_roots.add(self.artifact_root)
        assert self.seed_locked == seed_before
        T2 = self.supply - self.retired
        assert ceildiv(self.X * self.U, T2) * old_floor_d >= old_floor_n * T2
        self.validate()
        return "Settled"

    def core_designation_allowed(self, candidate_id, candidate_root):
        return not ((candidate_id == self.model_id and self.has_permanent_lp)
                    or candidate_root in self.protected_roots)

    def withdraw_permanent_liquidity(self, _claim_id):
        return "NonRedeemable"


def named_cases():
    rows = []
    cases = [
        ("zero reward", 100, 10, 0),
        ("zero market legs but residual remains", 100, 10, 3),
        ("both quantities are zero", 100, 10, 10),
        ("positive purchase but zero LP units", 100, 10, 40),
        ("exact nominal 30/20 example", 200, 50, 100),
        ("rounding permits zero surplus burn", 1, 3, 5),
    ]
    for name, X, U, R in cases:
        z = assert_arithmetic(X, U, R)
        rows.append({"case": name, "X": X, "U": U, "R": R, **z})
    assert rows[1]["B"] == rows[1]["A"] == 0 and rows[1]["D"] == 3
    assert rows[2]["q"] == rows[2]["ell"] == 0 and rows[2]["LiquidityDust"]
    assert rows[3]["q"] > 0 and rows[3]["ell"] == 0 and rows[3]["LiquidityDust"]
    assert rows[4]["ell"] == 3 and (2 * rows[4]["q"]) // 3 == 4
    assert rows[5]["ell"] == rows[5]["q"] == 1 and rows[5]["burn"] == 0

    # Reachable state: V=100, seed=10, S=1000; a user supplies net 2090.
    V, seed, S, initial_net = 100, 10, 1000, 2090
    X = V + seed + initial_net
    U = ceildiv((V + seed) * S, X)
    H = S - U
    z = quote(X, U, 1000)
    r2 = seed + initial_net + z["B"] + z["A"]
    sell_X = ceildiv(z["K2"], z["U2"] + H)
    gross = z["X2"] - sell_X
    remaining = r2 - gross
    assert seed <= remaining < seed + z["A"]
    rows.append({"case": "adding LP BILI to seed_locked wrongly rejects a funded sell",
                 "V": V, "old_seed_locked": seed, "supply": S,
                 "user_buy_net": initial_net, "pre_X": X, "pre_U": U, "holder_units": H,
                 "quote": z, "reserve_after_reward": r2,
                 "sell_all_gross": gross, "reserve_after_sell": remaining,
                 "correct_old_seed_guard_accepts": gross <= r2 - seed,
                 "incorrect_increased_seed_guard_accepts": gross <= r2 - (seed + z["A"])})
    return rows


def lifecycle_checks():
    rows = []
    m = Market(100, 0, 10)
    before = copy.deepcopy(m)
    assert m.settle("dust", 10, now=1, original_deadline=2) == "LiquidityDust"
    assert m == before
    assert m.settle("dust", 10, now=2, original_deadline=2) == "LiquidityDust"
    assert m == before
    assert m.settle("dust", 10, now=3, original_deadline=2) == "ExpiredUnsuccessful"
    assert m == before and not m.settled and not m.receipts
    rows.append("dust/retry/expiry leave pool, burn, residual payment, receipts and success markers unchanged")

    n = Market(100, 10, 1000)
    # A 11847 gross buy has a 10900 net leg under the current integer fees.
    assert n.buy("holder", 11847) == "Bought"
    assert n.U < 20
    before_dust = copy.deepcopy(n)
    assert n.settle("retry", 1000, now=1, original_deadline=3) == "LiquidityDust"
    assert n == before_dust
    assert n.sell("holder", 900) == "Sold"
    assert n.settle("retry", 1000, now=3, original_deadline=3) == "Settled"
    after = copy.deepcopy(n)
    assert n.settle("retry", 1000, now=3, original_deadline=3) == "AlreadySettled"
    assert n == after
    assert n.withdraw_permanent_liquidity("retry") == "NonRedeemable" and n == after
    assert not n.core_designation_allowed(n.model_id, n.artifact_root)
    assert not n.core_designation_allowed("alias-model", n.artifact_root)
    rows.append("dust can resolve after a valid market move before the unchanged deadline; no replay or withdrawal")

    o = Market(100, 0, 1000)
    snap = copy.deepcopy(o)
    assert o.settle("overflow", 2 ** 128 - 1, arithmetic_limit=2 ** 128 - 1) == "InvalidOrOverflow"
    assert o == snap
    assert o.settle("small", 3) == "Settled"
    assert o.r == snap.r and o.U == snap.U and o.paid_worker_residual == 3
    assert not o.receipts
    rows.append("checked overflow is rejected without effects; B=A=0 leaves only the residual worker leg")

    p = Market(100, 0, 1000)
    snap = copy.deepcopy(p)
    assert p.settle("rollback", 1000) == "Settled"
    committed = copy.deepcopy(p)
    p = copy.deepcopy(snap)  # discard the local transaction/canonical-state branch
    assert p == snap
    assert p.settle("rollback", 1000) == "Settled" and p == committed
    rows.append("whole-state undo/replay restores money, Position supply, receipt and permanent flag together")
    return rows


def mixed_sequences():
    rng = random.Random(0x4D4F4532)
    totals = Counter()
    for sequence in range(64):
        m = Market(1000 + 13 * sequence, sequence % 11, 500000)
        for step in range(256):
            holder = "h" + str(rng.randrange(4))
            operation = rng.randrange(5)
            before = copy.deepcopy(m)
            if operation == 0:
                result = m.buy(holder, rng.randrange(1, 100000))
            elif operation == 1:
                units = rng.randrange(0, m.holders.get(holder, 0) + 2)
                result = m.sell(holder, units)
            elif operation == 2:
                result = m.settle(f"{sequence}:{step}", rng.randrange(0, 100000))
            elif operation == 3:
                units = rng.randrange(0, m.holders.get(holder, 0) + 2)
                result = m.transfer(holder, "h" + str(rng.randrange(4)), units)
            else:
                result = m.settle(f"expired:{sequence}:{step}", rng.randrange(0, 100000),
                                  now=11, original_deadline=10)
            totals[result] += 1
            if result not in {"Bought", "Sold", "Transferred", "Settled"}:
                assert m == before
            m.validate()
    return {"sequences": 64, "operations_per_sequence": 256,
            "operations": 64 * 256, "outcomes": dict(sorted(totals.items()))}


def run():
    count, dust = 0, 0
    for X in range(1, 41):
        for U in range(1, 41):
            for R in range(0, 251):
                z = assert_arithmetic(X, U, R, H=R % 13)
                count += 1
                dust += z["LiquidityDust"]
    rng = random.Random(0x4C505F33)
    for _ in range(10000):
        X = max(1, rng.getrandbits(rng.choice((1, 8, 32, 64, 128))))
        U = max(1, rng.getrandbits(rng.choice((1, 8, 19, 32, 64))))
        R = rng.getrandbits(rng.choice((1, 8, 32, 64, 128, 256)))
        assert_arithmetic(X, U, R, H=rng.getrandbits(32))
    return {
        "status": "PASS", "scope": "MOE2 finite integer probes and reference-state accounting",
        "profile": "B=floor(300R/1000), A=floor(200R/1000), D=R-B-A; whole Position; ceil buy; floor LP return",
        "exhaustive": {"X": [1, 40], "U": [1, 40], "R": [0, 250],
                       "cases": count, "dust_quotes": dust},
        "wide_integer_cases": 10000,
        "named_cases": named_cases(),
        "lifecycle_checks": lifecycle_checks(),
        "mixed_sequences": mixed_sequences(),
        "proved_separately_in_math_document": [
            "ell <= q for all nonnegative integer rewards and X>0, U>=1",
            "Position and BILI conservation; no extra LP supply term",
            "K2>=K and nondecreasing ceil(K/T)/T for T=U+holder_units",
            "LP-only price rounding delta in [0, 1/(U1+ell))",
            "U<=2 cannot supply positive paired LP under A<=B",
        ],
        "limitations": [
            "Finite probes do not replace the universal algebraic proof.",
            "Python arbitrary-precision arithmetic does not establish native integer widths or codec bounds.",
            "The reference settlement assumes the separate PALW validation and funding gates are already satisfied.",
            "No native/EVM deployment, global DAG reorg theorem, universal LP settlement liveness, future market price, MEV resistance or profitability is proved.",
            "A permanent receipt records non-redeemable contributions; it is not a cash floor, Position balance or circulating LP token.",
            "Reference expiry is inclusive at original_deadline; the active timing profile must bind the exact native boundary.",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path,
                        default=Path(__file__).with_name("liquidity-math-check.json"))
    args = parser.parse_args()
    report = run()
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"status": report["status"], "exhaustive_cases": report["exhaustive"]["cases"],
                      "wide_integer_cases": report["wide_integer_cases"],
                      "sequence_operations": report["mixed_sequences"]["operations"],
                      "report": str(args.output.resolve())}, ensure_ascii=False))


if __name__ == "__main__":
    main()
