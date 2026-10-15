#!/usr/bin/env python3
"""RFC-0017 model eligibility — analytic gate math and a Monte-Carlo driver for the mandatory 300M attack (§7).

What this is: a DRIVER. Every model parameter is a named constant below (or a CLI override), every result is computed from them,
and every result is reported separately per metric — never folded into a "safe/unsafe" verdict. The driver is not a validation of
the chain implementation (it models the rules of `consensus/core/src/palw_model_eligibility_v1.rs`, it does not run them), and
its outputs are not production approval of any value: the RFC-0017 §11 design values it reads by default are unapproved.

What it reports, per scenario and horizon (30 / 90 days):
  max_reward_outflow     the attacker's share of the PALW reward budget R_PALW over the horizon (upper envelope: every eligible
                         day at its allocation share, clipped by the per-bond cap model below; probation days at the probation cap)
  adopted_block_share    the attacker's share of reward-bearing PALW blocks over the horizon (same envelope)
  final_weight_share     the attacker's share of Final weight (probation claims carry none under the initial proposal)
  honest_wait_days       mean days until an honest new model is promoted (attempt-budget contention and verifier obstruction)
  re_review_success      the probability the attacker passes at least one exam (promotion or renewal) over the horizon

Model (all simplifications are stated; a reviewer changes a constant, not the code):
  * One day is one step. Exams are independent: each has m stake-proportional draws with replacement from the GLOBAL verifier
    stake; a draw is the attacker's with probability f = A_verifier / (A_verifier + V_honest). The attacker attests positive on
    every attacker position (approve-without-checking); an honest position attests positive only if the model is obtainable and
    the verifier is not obstructed (probability HONEST_CHECK_RATE × (1 − OBSTRUCTION)). A closed model is never obtainable by an
    honest verifier, so only attacker positions are positive. Randomness bias and examiner attacks are NOT modelled (RFC §5
    lists them as separate factors).
  * Allocation share of an eligible model: S^α / Σ S^α over eligible models (ADR-0177, α = 1.5, binding), with S the model's
    miner+verifier capital as the RFC-0015 §8.5.5 30-day average (modelled as the steady value; the averaging delay is ignored,
    which favours the attacker). Registered / Lapsed models count 0 (RFC-0017 GAP-E1 assumes the call-site filter is wired).
  * Per-bond cap: BOND_CAP_MULTIPLE × the attacker's capital share bounds its reward share (None = no cap modelled). The ADR-0176
    caps are not edited here and their values are undecided; the multiple is a named stand-in.
  * Probation: an attacker model in probation earns at most PROBATION_MODEL_PERMILLE of blocks (and of R_PALW), with no Final
    weight; the network-wide probation budget PROBATION_NETWORK_PERMILLE caps all probation models together.
  * Attempts: the network admits ATTEMPTS_NETWORK_PER_EPOCH per ATTEMPT_EPOCH_DAYS; each lineage at most one open exam and a
    cooldown; the attacker spends up to ATTACKER_ATTEMPT_SHARE of the network budget (the rest is the honest queue).
  * Mode switching (§7-8): the hold W is 1 day (Panel=1) or 7 days (Panel=0); a switch never re-acquires capacity, modelled as
    the per-bond cap recovering at the slower mode's rate for W days after each switch.

Run: python3 scripts/sim/misaka-palw-rfc0017-eligibility-sim.py [--trials N] [--seed S]
"""

import argparse
import math
import random
from dataclasses import dataclass, replace

# --- the user's threat assumption (RFC-0017 §1) ------------------------------------------------------------------------------
TOTAL_SUPPLY_BILI = 10_000_000_000      # so that 300M is 3 % (the user's assumption; purchase price and liquidity out of scope)
ATTACKER_BILI = 300_000_000

# --- binding allocation (RFC-0015 §8.5.5) ---------------------------------------------------------------------------------------
ALPHA = 1.5

# --- RFC-0017 §11 design values (UNAPPROVED; mirror palw_model_eligibility_v1.rs *_DESIGN_V1) ----------------------------------
# U-1 (user decision 2026-10-15): ECON's 24-of-47 is the calibration candidate and the driver's default; 4-of-7 is a test-only
# comparison column (TEST_M / TEST_K), never the default.
EXAM_M = 47
EXAM_K = 24
TEST_M, TEST_K = 7, 4
PROBATION_MIN_DAYS = 7
PROBATION_MODEL_PERMILLE = 2
PROBATION_NETWORK_PERMILLE = 20
ATTEMPTS_PER_LINEAGE = 24
ATTEMPTS_NETWORK_PER_EPOCH = 64
ATTEMPT_EPOCH_DAYS = 30
ATTEMPT_COOLDOWN_DAYS = 3
ATTEST_WINDOW_DAYS = 2
RENEWAL_EPOCH_DAYS = 30
LAPSE_GRACE_DAYS = 2

# --- model stand-ins (named; every one is an assumption, not a measurement) --------------------------------------------------
HONEST_CHECK_RATE = 0.95          # an honest drawn verifier with the model checks and attests in time
ATTACKER_ATTEMPT_SHARE = 0.5      # the share of the network attempt budget the attacker consumes (spam / redraw)
BOND_CAP_MULTIPLE = None          # reward share ≤ multiple × capital share (None: not modelled — ADR-0176 values undecided)
HOLD_DAYS = {"panel1": 1, "panel0": 7}


@dataclass(frozen=True)
class Scenario:
    name: str
    attacker_miner_bili: float        # attacker capital bound as miner capital to its model(s)
    attacker_verifier_bili: float     # attacker capital bound as verifier stake (global pool)
    attacker_models: int              # the attacker's capital split evenly over this many closed models
    honest_capital_bili: float        # honest miner+verifier capital (bonded, counted for allocation)
    honest_models: int                # honest capital spread evenly over this many models
    honest_verifier_bili: float       # honest stake in the global verifier pool (≠ total supply share)
    closed: bool = True               # the attacker's model is closed (honest verifiers can never obtain it)
    open_until_day: int = 0           # §7-5: open (honest-verifiable) until this day, closed after
    obstruction: float = 0.0          # §7-7: fraction of honest verifiers obstructed (evidence / delivery)
    redraw_withdraw: bool = False     # §7-6: the attacker withdraws / re-registers after seeing draws (attempts still counted)
    mode_switch_days: int = 0         # §7-8: switch Panel=1 / Panel=0 every this many days (0: no switching)


def p_pass(f: float, m: int = EXAM_M, k: int = EXAM_K) -> float:
    """P(≥ k of m stake-proportional draws are the attacker's) — the pass-alone probability (RFC-0017 §5)."""
    return sum(math.comb(m, i) * f**i * (1 - f) ** (m - i) for i in range(k, m + 1))


def p_honest_pass(f: float, available: float, m: int = EXAM_M, k: int = EXAM_K) -> float:
    """P(≥ k positive positions) for an OPEN honest model: an honest position is positive with prob `available`; an attacker
    position never is (it obstructs by withholding)."""
    q = (1 - f) * available
    return sum(math.comb(m, i) * q**i * (1 - q) ** (m - i) for i in range(k, m + 1))


def alpha_share(attacker: list, honest: list, alpha: float = ALPHA) -> float:
    a = sum(x**alpha for x in attacker if x > 0)
    h = sum(x**alpha for x in honest if x > 0)
    return a / (a + h) if a + h > 0 else 0.0


def run(s: Scenario, horizon: int, trials: int, rng: random.Random) -> dict:
    f = s.attacker_verifier_bili / (s.attacker_verifier_bili + s.honest_verifier_bili) if s.attacker_verifier_bili > 0 else 0.0
    honest_models = [s.honest_capital_bili / s.honest_models] * s.honest_models
    per_model = s.attacker_miner_bili / max(1, s.attacker_models)
    capital_share = s.attacker_miner_bili / (s.attacker_miner_bili + s.honest_capital_bili)
    attacker_budget = int(ATTEMPTS_NETWORK_PER_EPOCH * ATTACKER_ATTEMPT_SHARE)
    honest_budget = ATTEMPTS_NETWORK_PER_EPOCH - attacker_budget
    sums = dict(outflow=0.0, blocks=0.0, weight=0.0, passed=0, wait=0.0)
    for _ in range(trials):
        # Each attacker model: stage per day — 'P' probation, 'E' eligible until `until`, 'L' lapsed.
        stage = ["P"] * s.attacker_models
        since = [0] * s.attacker_models
        until = [0] * s.attacker_models
        attempts = [0] * s.attacker_models
        last = [-10**9] * s.attacker_models
        spent_epoch, epoch = 0, 0
        outflow = blocks = weight = 0.0
        any_pass = False
        slow_until = -1
        for day in range(horizon):
            if day // ATTEMPT_EPOCH_DAYS != epoch:
                epoch, spent_epoch = day // ATTEMPT_EPOCH_DAYS, 0
            closed_now = s.closed and day >= s.open_until_day
            avail = 0.0 if closed_now else HONEST_CHECK_RATE * (1 - s.obstruction)
            # Exams (promotion after the minimum probation; renewal inside the window before `until`).
            for i in range(s.attacker_models):
                due = (stage[i] == "P" and day - since[i] >= PROBATION_MIN_DAYS) or stage[i] == "L" or (
                    stage[i] == "E" and until[i] - day <= ATTEST_WINDOW_DAYS + 1
                )
                if not due or attempts[i] >= ATTEMPTS_PER_LINEAGE or day - last[i] < ATTEMPT_COOLDOWN_DAYS:
                    continue
                if spent_epoch >= attacker_budget:
                    break
                spent_epoch += 1
                attempts[i] += 1
                last[i] = day
                # Positive positions: attacker positions always; honest ones only if obtainable and unobstructed.
                positives = sum(1 for _ in range(EXAM_M) if (rng.random() < f) or (rng.random() < avail))
                ok = positives >= EXAM_K
                if s.redraw_withdraw and not ok:
                    pass  # a withdrawal refunds nothing: the attempt is already counted above
                if ok:
                    any_pass = True
                    base = until[i] if stage[i] == "E" else day
                    stage[i], since[i], until[i] = "E", day, max(base, day) + RENEWAL_EPOCH_DAYS
            for i in range(s.attacker_models):
                if stage[i] == "E" and day > until[i] + LAPSE_GRACE_DAYS:
                    stage[i], since[i] = "L", day
            # Today's shares.
            eligible = [per_model if st == "E" else 0.0 for st in stage]
            share = alpha_share(eligible, honest_models)
            if BOND_CAP_MULTIPLE is not None:
                cap = BOND_CAP_MULTIPLE * capital_share
                if s.mode_switch_days and slow_until >= day:
                    cap *= HOLD_DAYS["panel1"] / HOLD_DAYS["panel0"]
                share = min(share, cap)
            probation = sum(1 for st in stage if st == "P")
            prob_share = min(probation * PROBATION_MODEL_PERMILLE, PROBATION_NETWORK_PERMILLE) / 1000.0
            if s.mode_switch_days and day % s.mode_switch_days == 0 and day > 0:
                slow_until = day + HOLD_DAYS["panel0"]
            outflow += share + prob_share
            blocks += share + prob_share
            weight += share  # probation claims carry no Final weight (initial proposal)
        sums["outflow"] += outflow / horizon
        sums["blocks"] += blocks / horizon
        sums["weight"] += weight / horizon
        sums["passed"] += 1 if any_pass else 0
        # Honest new model waiting time under the remaining attempt budget and obstruction (open model).
        hp = p_honest_pass(f, HONEST_CHECK_RATE * (1 - s.obstruction))
        wait, day, used = PROBATION_MIN_DAYS, PROBATION_MIN_DAYS, 0
        while day < 10 * horizon:
            if honest_budget <= 0:
                wait = float("inf")
                break
            if rng.random() < hp:
                wait = day
                break
            used += 1
            day += ATTEMPT_COOLDOWN_DAYS + ATTEST_WINDOW_DAYS
        else:
            wait = float("inf")
        sums["wait"] += wait
    return {
        "f_drawn": f,
        "p_pass_alone": p_pass(f),
        "max_reward_outflow": sums["outflow"] / trials,
        "adopted_block_share": sums["blocks"] / trials,
        "final_weight_share": sums["weight"] / trials,
        "honest_wait_days": sums["wait"] / trials,
        "re_review_success": sums["passed"] / trials,
    }


def scenarios() -> list:
    a = float(ATTACKER_BILI)
    base = dict(honest_capital_bili=a, honest_models=1, honest_verifier_bili=a)
    return [
        Scenario("1 all on one closed model", a / 2, a / 2, 1, **base),
        Scenario("2 honest spread over 100 models", a / 2, a / 2, 1, honest_capital_bili=10 * a, honest_models=100, honest_verifier_bili=10 * a),
        Scenario("2 honest spread over 1000 models", a / 2, a / 2, 1, honest_capital_bili=10 * a, honest_models=1000, honest_verifier_bili=10 * a),
        Scenario("3 split over 20 models and bonds", a / 2, a / 2, 20, honest_capital_bili=10 * a, honest_models=100, honest_verifier_bili=10 * a),
        Scenario("4 small honest verifier stake", a / 2, a / 2, 1, honest_capital_bili=10 * a, honest_models=100, honest_verifier_bili=a / 3),
        Scenario("5 open, closed after promotion (day 10)", a / 2, a / 2, 1, honest_capital_bili=10 * a, honest_models=100, honest_verifier_bili=10 * a, open_until_day=10),
        Scenario("6 withdraw / re-register after draws", a / 2, a / 2, 1, honest_capital_bili=10 * a, honest_models=100, honest_verifier_bili=10 * a, redraw_withdraw=True),
        Scenario("7 obstruct 30 % of honest verifiers", a / 2, a / 2, 1, honest_capital_bili=10 * a, honest_models=100, honest_verifier_bili=10 * a, obstruction=0.3),
        Scenario("8 1-day/7-day mode switching", a / 2, a / 2, 1, honest_capital_bili=10 * a, honest_models=100, honest_verifier_bili=10 * a, mode_switch_days=5),
    ]


# --- the verifier-stake sweep (RFC-0017 §5.1, user decision U-3) ------------------------------------------------------------
# The attacker's share f of the DRAWN verifier stake — not of total supply — decides P_pass. These are the points the RFC tabulates;
# 1/3 and 0.6 are the two §7 driver cases (150M attacker verifier stake vs 300M and vs 100M honest verifier stake).
SWEEP_F = (0.03, 0.05, 0.10, 0.20, 0.25, 1 / 3, 0.40, 0.50, 0.60, 0.75)
ECON_M, ECON_K = 47, 24                  # the ECON round-2 gate's unapproved interim (readiness §3f) — the default since U-1
ATTEMPT_FEE_BILI = 100_000               # PALW_ELIG_ATTEMPT_FEE_DESIGN_V1 in BILI (placeholder; user decision U-4)


def single_exam_mc(f: float, trials: int, rng: random.Random, m: int = EXAM_M, k: int = EXAM_K) -> float:
    """Monte-Carlo of one exam with a closed model: only attacker positions attest positively."""
    passed = 0
    for _ in range(trials):
        if sum(1 for _ in range(m) if rng.random() < f) >= k:
            passed += 1
    return passed / trials


def sweep(trials: int, rng: random.Random) -> None:
    per_epoch = ATTEMPTS_NETWORK_PER_EPOCH
    print("Pass-alone probability vs the attacker's share f of the DRAWN verifier stake (closed model; honest verifiers cannot attest)")
    print(f"  {'f':>6} {'P_pass 24-of-47':>16} {'MC (n=' + str(trials) + ')':>14} {'any of 64':>10} {'any of 192':>11} "
          f"{'4-of-7 (test-only)':>19}")
    for f in SWEEP_F:
        pp = p_pass(f)
        mc = single_exam_mc(f, trials, rng)
        print(
            f"  {f:6.3f} {pp:16.3e} {mc:14.6f} {1 - (1 - pp) ** per_epoch:10.6f} {1 - (1 - pp) ** (3 * per_epoch):11.6f} "
            f"{p_pass(f, TEST_M, TEST_K):19.6f}"
        )
    print(f"  (64 = one 30-day epoch of the network attempt budget; 192 = 90 days. Griefing price of filling it: "
          f"{ATTEMPTS_NETWORK_PER_EPOCH * ATTEMPT_FEE_BILI:,} BILI burned per epoch at the placeholder fee.)")
    print()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--trials", type=int, default=2_000)
    ap.add_argument("--seed", type=int, default=17)
    ap.add_argument("--sweep-trials", type=int, default=200_000)
    args = ap.parse_args()
    rng = random.Random(args.seed)
    sweep(args.sweep_trials, rng)

    print("RFC-0017 analytic gate math (illustrative targets, not payouts; not soundness, not conviction probability)")
    pp = p_pass(0.03)
    print(f"  P_pass(f=3 %, {EXAM_K}-of-{EXAM_M}) = {pp:.6e} per attempt; 1-(1-p)^10000 = {1 - (1 - pp) ** 10_000:.4f}")
    for n in (1, 100, 1000):
        a = 0.03**ALPHA
        h = 0.97**ALPHA / math.sqrt(n) if ALPHA == 1.5 else n * (0.97 / n) ** ALPHA
        print(f"  alpha={ALPHA}: attacker 3 % concentrated vs honest 97 % over n={n}: P_attack = {a / (a + h):.4%}")
    print()
    print("Monte-Carlo driver — model parameters are the named constants in this file; outputs are per metric, never a verdict.")
    print(f"  trials={args.trials} seed={args.seed} BOND_CAP_MULTIPLE={BOND_CAP_MULTIPLE} HONEST_CHECK_RATE={HONEST_CHECK_RATE} "
          f"ATTACKER_ATTEMPT_SHARE={ATTACKER_ATTEMPT_SHARE}")
    hdr = f"{'scenario':44} {'days':>4} {'f_drawn':>8} {'P_pass':>10} {'outflow':>8} {'blocks':>8} {'weight':>8} {'wait_d':>7} {'rereview':>8}"
    print(hdr)
    for s in scenarios():
        for horizon in (30, 90):
            r = run(s, horizon, args.trials, rng)
            print(
                f"{s.name:44} {horizon:4d} {r['f_drawn']:8.4f} {r['p_pass_alone']:10.3e} {r['max_reward_outflow']:8.4f} "
                f"{r['adopted_block_share']:8.4f} {r['final_weight_share']:8.4f} {r['honest_wait_days']:7.1f} {r['re_review_success']:8.4f}"
            )


if __name__ == "__main__":
    main()
