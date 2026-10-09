#!/usr/bin/env python3
"""ECON lane — the executable model behind docs/design/palw/econ-verifier-incentive-and-allocation.md.

Run:  python3 -I scripts/misaka-palw-econ-verifier-model.py            (every section; exit 1 on a failed check)
      python3 -I scripts/misaka-palw-econ-verifier-model.py --section 9  (one section)

Pure standard library; no network; no consensus or policy code calls it. Money is exact (fractions.Fraction) wherever a closed form
is checked; sweeps over probabilities and allocation curves use floats.

Every printed number carries one label:
  MEASURED  read from consensus code, or from the ledger PoCs misaka-palw-kernel/tests/econ_bounty.rs / econ_seal_veto.rs
  INTERIM   a value the code marks interim (palw_kernel_route_policy_v1, PalwPanelFreeFenceV1::interim_v1) — not production
  DERIVED   computed here from labelled inputs
  ASSUMED   an input chosen for illustration (no measurement exists yet); the user or MEAS must replace it
  PROPOSED  a bar or value ECON proposes; it is POLICY, for the user to fix

Amounts are BILI. a = 49% is the future ruleset's reporter share (ADR-0032's 2026-10-10 amendment, fence palw_reporter_share_v2);
the kernel route's interim value is 50%.
"""

from __future__ import annotations

import argparse
import hashlib
import math
import sys
from dataclasses import dataclass, replace
from fractions import Fraction as Q

FAILS: list[str] = []


def check(cond: bool, what: str) -> None:
    if not cond:
        FAILS.append(what)
        print(f"  !! CHECK FAILED: {what}")


def show(label: str, name: str, value) -> None:
    if isinstance(value, Q):
        value = f"{float(value):,.4f}".rstrip("0").rstrip(".")
    print(f"  [{label:8}] {name}: {value}")


def head(n: int, title: str) -> None:
    print(f"\n=== §{n} {title} ===")


# =====================================================================================================================================
# §1 Terms
# =====================================================================================================================================


@dataclass(frozen=True)
class Terms:
    """One claim's economic terms on the kernel route (RFC-0014/0015)."""

    K: Q  # reservation per claim (the collectable amount at conviction: min(reserved, collateral))
    a: Q  # reporter (accuser) share of what a conviction collects
    D: Q  # pre-Final default penalty
    default_burn: Q  # OPV economics.default_burn_permille / 1000
    R: Q  # Final reward (paid from the poster's escrow)
    w: Q  # work credit per claim
    X: Q  # external gain bound per claim
    f_adm: Q  # OPV admission fee (burned)
    f_job: Q  # job fee (burned, the poster's)
    d: Q  # claim seal deposit
    window: int  # OPV challenge window (DAA)
    court: int  # demand deadline (DAA)
    grace: int  # proof grace (DAA)
    liability: int  # post-Final liability horizon (DAA)

    @property
    def beta_d(self) -> Q:
        """The burned share of a pre-Final default penalty on an OPV claim: max(1 − a, default_burn) (ledger `tick_into`)."""
        return max(1 - self.a, self.default_burn)

    @property
    def G(self) -> Q:
        """RFC-0015 §8's per-claim maximum gain R + w + X."""
        return self.R + self.w + self.X

    @property
    def hold(self) -> int:
        """H_L: the SHORTEST time a claim's reservation is held — an undisputed claim is Final at its window's end and stays liable
        for `liability` more. (A demanded claim holds longer, up to window + court + grace + liability.) Theorem Y's ceiling is an
        upper bound on the achievable yield, so it takes the shortest hold."""
        return self.window + self.liability

    @property
    def hold_max(self) -> int:
        return self.window + self.court + self.grace + self.liability


# INTERIM: palw_kernel_route_policy_v1 + PalwPanelFreeFenceV1::interim_v1, with a = 49% (the future ruleset).
INTERIM_49 = Terms(
    K=Q(1000), a=Q(49, 100), D=Q(100), default_burn=Q(1, 10), R=Q(5), w=Q(5), X=Q(10), f_adm=Q(1), f_job=Q(1), d=Q(1),
    window=50, court=20, grace=10, liability=200,
)
# The OPV test world of the ledger PoCs (ledger_world::policy + opv_world::opv_example), accuser share set to 490‰.
TEST_WORLD_49 = Terms(
    K=Q(1000), a=Q(49, 100), D=Q(100), default_burn=Q(1, 10), R=Q(7), w=Q(13), X=Q(80), f_adm=Q(3), f_job=Q(2), d=Q(1),
    window=50, court=20, grace=10, liability=200,
)
DAA_PER_DAY = 576  # ASSUMED operating cadence: 150 s per DAA (one DAA is at least 120 s)


def floor_q(x: Q) -> Q:
    return Q(math.floor(x))


def section1() -> None:
    head(1, "Terms")
    t = INTERIM_49
    for name in ("K", "a", "D", "R", "w", "X", "f_adm", "f_job", "d"):
        show("INTERIM", name, getattr(t, name))
    show("DERIVED", "beta_d = max(1 - a, default_burn)", t.beta_d)
    show("DERIVED", "G = R + w + X", t.G)
    show("INTERIM", "window / court / grace / liability (DAA)", f"{t.window} / {t.court} / {t.grace} / {t.liability}")
    show("DERIVED", "H_L, the shortest reservation hold / the longest (DAA)", f"{t.hold} / {t.hold_max}")
    check(t.beta_d == Q(51, 100), "beta_d is 51% at a = 49%")


# =====================================================================================================================================
# §2 Self-dealing at 49% (Theorem S) — the loop, the E1 leak, the fix
# =====================================================================================================================================

PATHS = ("direct", "after_default", "after_final")


def recoup(t: Terms, path: str, e1: bool) -> Q:
    """What the coalition takes back from ITS OWN claim's money when it is the earliest sealer and its own demander.

    Today (ledger `convict` + `tick_into`): a pre-Final default pays the demanders D·(1 − β_d); the bounty's basis is the admitted
    reservation (slashed + taken_by_default), capped at what the conviction slashes. Under E1 ("one claim, one 49%") every payout to
    accusers and demanders out of one claim is capped at ⌊a · collected⌋.
    """
    bounty_basis = floor_q(t.a * t.K)
    if path in ("direct", "after_final"):
        return bounty_basis
    share = t.D - floor_q(t.D * t.beta_d)
    if not e1:
        return share + min(bounty_basis, t.K - t.D)
    return share + min(bounty_basis - share, t.K - t.D)


def loop_net(t: Terms, path: str, e1: bool) -> Q:
    """The conviction loop's net to the coalition (collected K out, recoup in); fees and the reward excluded."""
    return -t.K + recoup(t, path, e1)


def a_eff(t: Terms, path: str, e1: bool) -> Q:
    return recoup(t, path, e1) / t.K


def section2() -> None:
    head(2, "Self-dealing at a = 49% (Theorem S): the loop, the E1 leak, the fix")
    tw = TEST_WORLD_49
    # Cross-check against the ledger PoC (econ_bounty.rs): direct -513, after a default -464, after Final -506 (fees 3, reward 7).
    predicted = {"direct": -513, "after_default": -464, "after_final": -506}
    for path in PATHS:
        net = loop_net(tw, path, e1=False) - tw.f_adm + (tw.R if path == "after_final" else 0)
        show("DERIVED", f"test world, {path}: coalition net (= ledger PoC, MEASURED there)", net)
        check(net == predicted[path], f"test-world {path} net matches the ledger PoC's assertion")
    t = INTERIM_49
    print("  interim terms, conviction loop only (collected = K = 1,000):")
    for path in PATHS:
        for e1 in (False, True):
            n = loop_net(t, path, e1)
            show("DERIVED", f"{path:13} {'E1 ' if e1 else 'now'}: net {float(n):8.1f}, recoup share a_eff", f"{float(a_eff(t, path, e1)):.3f}")
    check(loop_net(t, "after_default", False) == -461, "E1 leak: the default path loses only 461 of 1,000 at a = 49%")
    check(all(loop_net(t, p, True) == -510 for p in PATHS), "under E1 every path loses exactly 51%")
    # The supremum over every policy penalty D in [0, K] (1-BILI grid): today -(1 - a)·K·β_d; under E1 -(1 - a)·K.
    worst_now = max(loop_net(replace(t, D=Q(d)), "after_default", False) for d in range(0, 1001))
    worst_e1 = max(loop_net(replace(t, D=Q(d)), "after_default", True) for d in range(0, 1001))
    show("DERIVED", "sup over D of the default path, today", worst_now)
    show("DERIVED", "closed form -(1 - a)·K·β_d (β_d = 1 - a here)", -(1 - t.a) * t.K * (1 - t.a))
    show("DERIVED", "sup over D of the default path, under E1", worst_e1)
    check(abs(worst_now - (-(1 - t.a) * t.K * (1 - t.a))) <= 1, "today's supremum is -(1-a)^2·K = -260.1 (to rounding)")
    check(worst_e1 == -510, "under E1 no penalty D lets a self-dealer lose less than 51%")
    # Self-accusation of an HONEST claim: the exact court dismisses it; the filer pays the dismissed-filing fee; nothing flows in.
    show("DERIVED", "self-accusation of an honest claim (exact court): net", "-f_dis <= 0 (no flow in)")


# =====================================================================================================================================
# §3 ADR-0176 D6 per claim: p·(R_risk + L_collectible_net) > C_saved (+ external gain)
# =====================================================================================================================================


def p_min(t: Terms, c_saved: Q, r_risk: Q, x: Q, a_e: Q) -> Q:
    """The least effective detection·adjudication·collection probability at which a lie does not pay (one claim)."""
    l_net = (1 - a_e) * t.K
    return (c_saved + x) / (r_risk + l_net)


def required_reservation_code(t: Terms, p: Q) -> Q:
    """OPV `required_reservation` (code): max(G + D, G/p) / (1 - a)."""
    return max(t.G + t.D, t.G / p) / (1 - t.a)


def required_reservation_e1_leak(t: Terms, p: Q) -> Q:
    """The same relation with the self-demander's share counted: max(G + D, G/p + D·(1 - β_d)) / (1 - a)."""
    return max(t.G + t.D, t.G / p + (t.D - floor_q(t.D * t.beta_d))) / (1 - t.a)


def section3() -> None:
    head(3, "ADR-0176 D6 per claim, at a = 49%")
    t = INTERIM_49
    rows = []
    for x in (Q(0), t.X):
        for r_risk, when in ((Q(0), "post-Final (reward kept)"), (t.R, "pre-Final (reward forfeited)")):
            for e1 in (True, False):
                a_e = a_eff(t, "after_default", e1)
                pm = p_min(t, c_saved=t.R, r_risk=r_risk, x=x, a_e=a_e)
                rows.append((x, when, e1, pm))
                show("DERIVED", f"C_saved = R = 5, X = {float(x):>4}, {when:28} {'E1 ' if e1 else 'now'} p_min", f"{float(pm):.4%}")
    check(all(r[3] < Q(1, 10) for r in rows), "at the interim K every row's p_min is below 10%")
    # The OPV reservation relation (code) vs the same relation counting the self-demander share.
    for p in (Q(1, 2), Q(1, 20), Q(1, 200)):
        show("DERIVED", f"required reservation at p = {float(p):.3f}: code / with the E1 leak counted",
             f"{float(required_reservation_code(t, p)):.1f} / {float(required_reservation_e1_leak(t, p)):.1f}")
    check(required_reservation_e1_leak(t, Q(1, 200)) > required_reservation_code(t, Q(1, 200)),
          "at a small p the code's relation under-reserves by D(1-β_d)/(1-a)")
    show("DERIVED", "p = 0 (closed model, ADR-0177 D7)", "p_min > 0 always: no reservation deters a lie nobody can check")


# =====================================================================================================================================
# §4 Per bond, per window (ADR-0176 Q/B/R/F) — Theorem Y, the yield ceiling
# =====================================================================================================================================


def yield_ceiling(p: float, a_e: float, lam: float, x: float, r_risk: float, w_daa: int, hold: int) -> float:
    """The largest reward per unit of bonded capital per window W at which deterrence can hold with fully backed reservations.

    Per claim, deterrence needs k = K/R ≥ (λ + x − p·r_risk) / (p·(1 − a_eff)); the capital backs Σ K·hold ≤ C·T, so the reward per
    window is at most C·W/(k·hold). λ: the forger's advantage per unit of reward (compute saved, or reward an honest miner could not
    have earned; ≤ 1 at worst); x: external gain per unit of reward; r_risk: reward forfeited on detection per unit of reward.
    """
    need = lam + x - p * r_risk
    if p <= 0:
        return 0.0
    if need <= 0:
        return math.inf
    k = need / (p * (1 - a_e))
    return w_daa / (k * hold)


def section4() -> None:
    head(4, "Per bond and window (ADR-0176): the yield ceiling (Theorem Y)")
    t = INTERIM_49
    hold = t.hold
    show("DERIVED", "H_L (DAA)", hold)
    for p in (0.0, 0.001, 0.01, 0.05, 0.5):
        y_daa = yield_ceiling(p, 0.49, 1.0, 0.0, 0.0, 1, hold)
        show("DERIVED", f"p = {p:<5}: max reward / capital per DAA (λ = 1, x = 0, post-Final)",
             f"{y_daa:.3e}  (per day {y_daa * DAA_PER_DAY:.3%}, ASSUMED 576 DAA/day)")
    check(yield_ceiling(0.0, 0.49, 1.0, 0.0, 0.0, 1, hold) == 0.0, "p = 0: no positive reward yield is deterrence-safe")
    # E1's leak shrinks the ceiling by (1 - a_eff_now) / (1 - a): 0.461 / 0.51.
    now = float(a_eff(t, "after_default", False))
    ratio = yield_ceiling(0.05, now, 1, 0, 0, 1, hold) / yield_ceiling(0.05, 0.49, 1, 0, 0, 1, hold)
    show("DERIVED", "ceiling today / under E1 (the leak's cost)", f"{ratio:.3f}")
    # ADR-0176 D5's illustration (NOT adopted values): 350 BILI, 100 claims per 7 days.
    week = 7 * DAA_PER_DAY
    concurrent = 100 * hold / week
    k_claim = 350 / concurrent
    show("ASSUMED", "ADR-0176 D5 example: bond 350 BILI, 100 claims / 7 days (4,032 DAA)", f"{concurrent:.2f} claims held at once")
    show("DERIVED", "reservation per claim if fully backed", f"{k_claim:.1f} BILI")
    for p in (0.001, 0.05):
        show("DERIVED", f"largest deterrence-safe reward per claim at p = {p} (λ = 1)", f"{k_claim * p * 0.51:.4f} BILI")


# =====================================================================================================================================
# §5 Concurrent claims: correlation and shared collateral
# =====================================================================================================================================


def binom_pmf(n: int, k: int, p: float) -> float:
    return math.comb(n, k) * p**k * (1 - p) ** (n - k)


def expected_collected(n: int, p: float, k_claim: float, capital: float, mode: str) -> float:
    if mode == "independent":
        return sum(binom_pmf(n, j, p) * min(j * k_claim, capital) for j in range(n + 1))
    if mode == "comonotone":  # all claims detected together, or none
        return p * min(n * k_claim, capital)
    if mode == "cascade":  # one detection convicts all (watchers re-check the bond's other claims in the horizon)
        return (1 - (1 - p) ** n) * min(n * k_claim, capital)
    raise ValueError(mode)


def section5() -> None:
    head(5, "Concurrent claims: correlation and shared collateral")
    n, p, k = 10, 0.05, 1000.0
    for capital in (10_000.0, 5_000.0, 1_000.0):
        kappa = n * k / capital
        row = {m: expected_collected(n, p, k, capital, m) for m in ("independent", "comonotone", "cascade")}
        show("DERIVED", f"N = {n}, p = {p}, K = {k:.0f}, C = {capital:,.0f} (κ = {kappa:g}): E[collected]",
             ", ".join(f"{m} {v:,.1f}" for m, v in row.items()))
        if kappa <= 1:
            check(all(abs(row[m] - n * p * k) < 1e-6 for m in ("independent", "comonotone")),
                  "κ ≤ 1: expected collection is linear, correlation does not matter")
        else:
            check(row["comonotone"] <= n * p * k / kappa + 1e-9, "κ > 1: the comonotone worst case divides deterrence by κ")
    show("DERIVED", "rule", "fully backed reservations (κ ≤ 1, ADR-0176 D4) make correlation irrelevant to the expectation;"
                            " with κ > 1 count C, not ΣK")


# =====================================================================================================================================
# §6 The verifier's incentive: T0, remedies A and B, M*-49 (the P0 proof, part 2)
# =====================================================================================================================================


def h_u64(*parts) -> int:
    m = hashlib.blake2b(digest_size=8)
    for x in parts:
        m.update(repr(x).encode())
    return int.from_bytes(m.digest(), "little")


def remedy_a_grinding(q_ppm: int, lies: int) -> tuple[float, float]:
    """A producer that knows its Sybil's salt grinds its claim id until its fault is inside the Sybil's sample."""
    trials = covered = 0
    for lie in range(lies):
        nonce = 0
        while True:
            cid = h_u64("claim", lie, nonce)
            trials += 1
            if h_u64("sample", "sybil-salt", cid, 1) % 1_000_000 < q_ppm:
                break
            nonce += 1
        covered += h_u64("sample", "honest-salt", cid, 1) % 1_000_000 < q_ppm
    return trials / lies, covered / lies


def b_split(bounty: int, s: int, h: int) -> tuple[int, int]:
    if s + h == 0:
        return 0, 0
    each = bounty // (s + h)
    return each, bounty - each * h


def stake_draw_share(pool: list[tuple[bool, int]], n: int) -> tuple[float, float]:
    total = sum(s for _, s in pool)

    def one(b: int, slot: int) -> bool:
        u = h_u64("draw", b, slot) % total
        for coalition, stake in pool:
            if u < stake:
                return coalition
            u -= stake
        raise AssertionError

    slots = both = 0
    for b in range(n):
        s0, s1 = one(b, 0), one(b, 1)
        slots += s0 + s1
        both += s0 and s1
    return slots / (2 * n), both / n


@dataclass(frozen=True)
class Costs:
    """An honest watcher's costs per check (ASSUMED until MEAS measures them per class)."""

    fetch: Q
    check: Q
    localize: Q
    attest: Q
    carrier: Q
    file: Q
    demand_burn: Q  # expected burned demand bond (served-demand burn, O-C4R4-06), if the watcher must demand
    acquire: Q  # one model acquisition (ADR-0177: the verifier's own copy), amortised below
    checks_per_acquisition: int


def fee_floor(c: Costs) -> Q:
    return c.fetch + c.check + c.localize + c.attest + c.carrier + c.demand_burn + c.acquire / c.checks_per_acquisition


def p_dc(q: float, m: int, sigma: float, theta_h: float) -> float:
    """At least one drawn slot is a diligent honest watcher: slots are stake-weighted, independent (T2)."""
    return q * (1 - (1 - (1 - sigma) * theta_h) ** m)


def q_needed(p_need: float, m: int, sigma: float, theta_h: float) -> float | None:
    cover = 1 - (1 - (1 - sigma) * theta_h) ** m
    if cover <= 0:
        return None
    q = p_need / cover
    return q if q <= 1 else None


def mstar49_split(collected: Q, a: Q, b_cap: Q, drawn_sealers: int) -> tuple[Q, Q]:
    """M*-49: the 49% of what a conviction collected goes first to the DRAWN slots that sealed a convicting proof (equal split, up
    to B_cap together), the remainder to the earliest sealer (today's holder). Returns (each drawn sealer, earliest sealer)."""
    pool = floor_q(a * collected)
    drawn = min(b_cap, pool) if drawn_sealers else Q(0)
    each = floor_q(drawn / drawn_sealers) if drawn_sealers else Q(0)
    return each, pool - each * drawn_sealers


def section6() -> None:
    head(6, "The verifier's incentive (P0 part 2): T0, remedies A/B, and M*-49")
    t = INTERIM_49
    bounty = int(floor_q(t.a * t.K))
    show("DERIVED", "uncontested bounty at 49%", bounty)
    # T0: a bounty-only watcher's break-even fraud rate.
    for share, label in ((bounty, "uncontested"), (bounty // 10, "shared by 10")):
        show("DERIVED", f"T0: fraud rate a watcher spending 1 BILI a check needs ({label})", f"{1 / share:.4%}")
    show("DERIVED", "T0 under M0 capture", "share 0 against a rational liar: no fraud rate suffices")
    # Remedy A.
    mean, honest = remedy_a_grinding(62_500, 2_000)
    show("DERIVED", "remedy A: mean offline trials to self-cover (q = 1/16; 2,000 simulated lies)", f"{mean:.1f}")
    show("DERIVED", "remedy A: honest coverage of the ground ids (≈ q)", f"{honest:.4f}")
    check(12 < mean < 20 and 0.04 < honest < 0.09, "remedy A falls to 1/q grinding")
    # Remedy B at 49%.
    table = [(s, *b_split(bounty, s, 1)) for s in (0, 1, 10, 100, 1000)]
    show("DERIVED", "remedy B at 49% (Sybils s, honest share, coalition recoup)", table)
    check(table[2][1] == 44 and table[3][1] == 4, "remedy B is diluted by Sybil bonds")
    # T2.
    honest_pool = [(False, 300)] * 10
    for name, pool in (("one bond", honest_pool + [(True, 1000)]), ("fifty bonds", honest_pool + [(True, 20)] * 50)):
        share, both = stake_draw_share(pool, 20_000)
        show("DERIVED", f"T2: σ = 1/4 as {name}: slot share, both slots", f"{share:.4f}, {both:.4f}")
        check(0.23 < share < 0.27 and 0.055 < both < 0.07, f"T2 Sybil-neutral ({name})")
    # M*-49 numbers.
    m = 2
    b_cap = m * t.G  # T5's bribery floor
    show("DERIVED", "M*-49: B_cap = m·G (bribery floor, T5)", b_cap)
    each, rest = mstar49_split(t.K, t.a, b_cap, 1)
    show("DERIVED", "M*-49: one honest drawn sealer / the earliest (maybe the liar's) sealer", f"{float(each)} / {float(rest)}")
    check(each + rest == floor_q(t.a * t.K), "M*-49 pays out exactly the 49%, so the coalition still recoups at most a·K")
    c = Costs(fetch=Q(9, 10), check=Q(4), localize=Q(5, 100), attest=Q(2, 100), carrier=Q(2, 100), file=Q(2, 10),
              demand_burn=Q(1, 10), acquire=Q(50), checks_per_acquisition=100)
    f_min = fee_floor(c)
    show("ASSUMED", "watcher costs per check (fetch, check, localize, attest, carrier, demand burn; acquisition 50 over 100)",
         "0.9, 4, 0.05, 0.02, 0.02, 0.1; 0.5")
    show("DERIVED", "T3: the fee floor F_min", f_min)
    worst = f_min - (c.fetch + c.check + c.localize + c.attest + c.carrier + c.demand_burn + c.acquire / c.checks_per_acquisition)
    check(worst == 0 and each >= c.file, "T3: at F = F_min and B_cap/m ≥ c_file an honest drawn slot never loses")
    p_need = float(p_min(t, c_saved=t.R, r_risk=Q(0), x=t.X, a_e=t.a))
    show("DERIVED", "p needed (C_saved = R, X = 10, post-Final, E1)", f"{p_need:.4%}")
    for sigma in (0.0, 0.25, 0.5, 0.9, 1.0):
        for theta in (1.0, 0.5):
            q = q_needed(p_need, m, sigma, theta)
            user = None if q is None else q * m * float(f_min)
            show("DERIVED", f"σ = {sigma:<4} θ_h = {theta:<3}: q needed / user pays per claim",
                 "INFEASIBLE (no honest watcher can be drawn)" if q is None else f"{q:.3%} / {user:.3f} BILI")
    check(q_needed(p_need, m, 1.0, 1.0) is None, "a closed model (σ = 1) admits no q: M* cannot create p > 0")


# =====================================================================================================================================
# §7 GAP-B12: the seal deposit derived from the stall R+1 abandonments buy, the attack cost and the damage
# =====================================================================================================================================


@dataclass(frozen=True)
class Onboarding:
    anchor: int = 2  # INTERIM sealed policy: anchor delay
    W: int = 40  # seal window = reveal window
    retry_limit: int = 2  # R: R + 1 counted attempts


def t_withhold(o: Onboarding) -> int:
    return o.anchor + 2 * o.W + 1


def t_abandon(o: Onboarding, t: Terms) -> int:
    return o.anchor + 2 * o.W + t.window + t.court - 1


def t_self_convict(o: Onboarding, t: Terms) -> int:
    return o.anchor + 2 * o.W + t.window + t.court + t.grace - 1


def section7() -> None:
    head(7, "GAP-B12: the seal deposit, derived")
    o, t = Onboarding(), INTERIM_49
    tw, tab, tcv = t_withhold(o), t_abandon(o, t), t_self_convict(o, t)
    show("MEASURED", "T_w: a withheld seal's veto, commitment to next attempt (DAA)", tw)
    show("MEASURED", "T_ab: an abandoned seal's veto via its own default (econ_seal_veto)", tab)
    show("DERIVED", "T_cv: via a self-conviction at the end of the proof grace (the longest stall a veto buys)", tcv)
    check((tw, tab, tcv) == (83, 151, 161), "the stall per veto")
    carrier = Q(2, 100)  # ASSUMED fee per carried object
    c_w = t.d + carrier  # after G14R's S1 a re-seal forfeits: every withheld veto costs one deposit
    c_ab = t.f_adm + floor_q(t.D * t.beta_d) + 3 * carrier  # seal, reveal, demand
    c_cv = t.f_adm - loop_net(t, "direct", True) + 3 * carrier
    show("DERIVED", "cost of one veto: withhold (d + carrier) / abandon (f_adm + D·β_d + 3 carriers) / self-convict",
         f"{float(c_w):.2f} / {float(c_ab):.2f} / {float(c_cv):.2f}")
    per_daa = {"withhold": c_w / tw, "abandon": c_ab / tab, "self-convict": c_cv / tcv}
    for k, v in per_daa.items():
        show("DERIVED", f"attacker's price per victim-DAA of stall, {k} (N_c = 1)", f"{float(v):.4f} BILI")
    # The parity deposit: withholding must be no cheaper per DAA of stall than the cheapest other veto.
    d_star = (c_ab / tab) * tw - carrier
    show("DERIVED", "d* = (c_ab / T_ab)·T_w − carrier: the deposit at which withholding stops being the cheap lever", d_star)
    check(abs(float(d_star) - 28.6) < 0.2, "d* ≈ 28.6 BILI at the interim terms and 49%")
    # What a deposit above d* buys: nothing — the attacker abandons instead.
    gamma_star = c_ab / tab
    show("DERIVED", "γ*: the delay value per victim-DAA the interim terms deter (N_c = 1)",
         f"{float(gamma_star):.3f} BILI/DAA ≈ {float(gamma_star) * DAA_PER_DAY:.0f} BILI/day")
    # The stall R + 1 abandonments buy, its price, and the damage.
    r1 = o.retry_limit + 1
    show("DERIVED", "stall to exhaustion: (R+1)·T_ab / (R+1)·T_cv (DAA)", f"{r1 * tab} / {r1 * tcv}  (≈ {r1 * tab * 150 / 3600:.1f} h)")
    show("DERIVED", "price of exhaustion by abandonment, (R+1)·c_ab", r1 * c_ab)
    for nu, n_c, c_reg in ((Q(1, 10), 1, Q(200)), (Q(1), 1, Q(200)), (Q(1), 4, Q(200)), (Q(10), 16, Q(2000))):
        damage = n_c * (nu * r1 * tab + c_reg)
        cost = r1 * c_ab
        show("ASSUMED", f"ν = {float(nu):g} BILI/victim-DAA, N_c = {n_c}, C_reg = {float(c_reg):g}: damage / cost (griefing factor)",
             f"{float(damage):,.0f} / {float(cost):,.0f} = {float(damage / cost):.1f}×")
    # Honest job races cap a seal that every producer must post (G14-R4's sizing: (n − 1)·d burned per won claim ≤ 20% of R).
    ceiling = Q(20, 100) * t.R / 1
    show("DERIVED", "honest-race ceiling on a deposit every racer posts (ρ = 20%, R = 5, n = 2)", ceiling)
    check(d_star > ceiling, "d* exceeds the honest-race ceiling: split the prices (S3) — only a beacon-source seal carries d*")
    # Network-wide: one withheld seal per W DAA vetoes every sampled attempt.
    for d in (t.d, d_star):
        show("DERIVED", f"freezing all sampled onboarding for a day at d = {float(d):.1f}", f"{float(d) * DAA_PER_DAY / o.W:,.1f} BILI/day")
    show("DERIVED", "with S4 (a vetoing seal forfeits d per attempt it vetoes)", "the price scales with N_c; without it, d/N_c per victim")


# =====================================================================================================================================
# §8 ADR-0177 D6: open versus closed economics for f(S_m)
# =====================================================================================================================================


@dataclass(frozen=True)
class Curve:
    name: str
    f: object  # callable S -> weight

    def g(self, s: float) -> float:
        """Per-unit weight f(S)/S."""
        return self.f(s) / s if s > 0 else 0.0


def power(alpha: float) -> Curve:
    return Curve(f"S^{alpha:g}", lambda s, a=alpha: s**a)


def threshold(s_star: float) -> Curve:
    return Curve(f"S·min(1,S/{s_star:g})", lambda s, k=s_star: s * min(1.0, s / k))


def capped(s_cap: float) -> Curve:
    return Curve(f"min(S,{s_cap:g})", lambda s, k=s_cap: min(s, k))


CURVES = [power(1.0), power(0.5), capped(2.0), power(1.5), power(2.0), power(3.0), threshold(2.0)]


@dataclass(frozen=True)
class Market:
    """ASSUMED market: n_o other models of capital 1 each; budget 1 per epoch; compute = λ of a reward; capital costs r per epoch."""

    n_o: int = 20
    s_o: float = 1.0
    lam: float = 0.5
    r: float = 0.01
    c_reg: float = 0.005
    c_dist: float = 0.0


def others(c: Curve, mk: Market) -> float:
    return mk.n_o * c.f(mk.s_o)


def alloc(c: Curve, weight: float, mk: Market) -> float:
    return weight / (weight + others(c, mk))


def pi_closed(c: Curve, s0: float, mk: Market, forge: bool) -> float:
    a = alloc(c, c.f(s0), mk)
    return a * (1.0 if forge else 1 - mk.lam) - mk.r * s0


def pi_self(c: Curve, s0: float, e: float, mk: Market, forge: bool, r_b: float) -> float:
    a = alloc(c, c.f(s0 + e), mk)
    return a * (1.0 if forge else 1 - mk.lam) - mk.r * s0 - r_b * e


def pi_open(c: Curve, s0: float, e: float, mk: Market) -> float:
    """Published: external capital E joins only if its own per-unit net covers its capital cost; otherwise the incumbent is alone,
    but now checkable (p > 0), so it must compute."""
    if not external_joins(c, s0, e, mk):
        e = 0.0
    a = alloc(c, c.f(s0 + e), mk)
    return a * s0 / (s0 + e) * (1 - mk.lam) - mk.r * s0 - mk.c_dist


def external_joins(c: Curve, s0: float, e: float, mk: Market) -> bool:
    a = alloc(c, c.f(s0 + e), mk)
    return a / (s0 + e) * (1 - mk.lam) >= mk.r


def split_gain(c: Curve, s0: float, k: int, mk: Market) -> float:
    w1 = c.f(s0)
    wk = k * c.f(s0 / k)
    return (wk / (wk + others(c, mk)) - w1 / (w1 + others(c, mk))) - (k - 1) * mk.c_reg


def concentration_premium(c: Curve, x: float, total: float, n_h: int) -> float:
    """An attacker with a fraction x of all model capital on ONE model, against honest capital spread over n_h models."""
    fa = c.f(x * total)
    fh = n_h * c.f((1 - x) * total / n_h)
    return (fa / (fa + fh)) / x


def ratio(num: float, den: float) -> float:
    if den <= 0:
        return math.inf if num > 0 else -math.inf
    return num / den


BAR_PHI = 2.0  # PROPOSED (the user fixes it): open ≥ 2× closed self-capital
BAR_S0 = (0.25, 1.0)  # PROPOSED: incumbents at or below the median model's capital (1.0 here)
BAR_E = (1.0, 3.0, 9.0)  # PROPOSED: external capital 1×, 3×, 9× the incumbent's own


def flows(c: Curve, mk: Market, external: float, step: float = 0.05) -> list[float]:
    """Competing models: external capital arrives in steps and each step goes to the model whose entrant earns the most per unit
    (ties to the smallest model). Returns the models' final budget shares."""
    caps = [mk.s_o] * (mk.n_o + 1)
    for _ in range(int(round(external / step))):
        tot = sum(c.f(x) for x in caps)

        def entrant(j: int) -> float:
            w = c.f(caps[j] + step)
            return w / (tot - c.f(caps[j]) + w) * step / (caps[j] + step)

        best = max(range(len(caps)), key=lambda j: (round(entrant(j), 12), -caps[j]))
        caps[best] += step
    tot = sum(c.f(x) for x in caps)
    return sorted((c.f(x) / tot for x in caps), reverse=True)


def section8() -> None:
    head(8, "ADR-0177 D6: open versus closed economics for f(S_m)")
    mk = Market()
    show("ASSUMED", "market", f"{mk.n_o} other models of capital {mk.s_o}; budget 1/epoch; λ = {mk.lam}; r = {mk.r}/epoch; "
                              f"C_reg = {mk.c_reg}/epoch; distribution cost {mk.c_dist}")
    show("PROPOSED", "bar O", f"open ≥ {BAR_PHI}× closed self-capital, for S_0 ∈ {BAR_S0} (≤ the median model), e = E/S_0 ∈ {BAR_E}")
    show("PROPOSED", "bar S", "open ≥ equal self-funding (same S_m, own capital at the market rate)")
    show("PROPOSED", "bar A", "an attacker's concentration premium ≤ 1 (subsidy share ≤ its capital share), x ∈ {0.1, 0.25, 0.5}")
    show("PROPOSED", "bar M", "no gain from splitting a model's capital into k near-duplicate registrations, k ∈ {2, 4, 10}")
    total = mk.n_o * mk.s_o
    verdicts = {}
    for c in CURVES:
        print(f"  -- f = {c.name}")
        o_forge, o_honest, s_ok = [], [], []
        for s0 in (0.25, 1.0, 4.0):
            for e_ratio in (1.0, 3.0, 9.0):
                e = e_ratio * s0
                po = pi_open(c, s0, e, mk)
                phi_f = ratio(po, pi_closed(c, s0, mk, forge=True))
                phi_h = ratio(po, pi_closed(c, s0, mk, forge=False))
                phi_s = ratio(po, pi_self(c, s0, e, mk, forge=True, r_b=mk.r))
                joins = external_joins(c, s0, e, mk)
                if s0 in BAR_S0 and e_ratio in BAR_E:
                    o_forge.append(phi_f)
                    o_honest.append(phi_h)
                if joins:
                    s_ok.append(phi_s >= 1)
                if e_ratio in (1.0, 9.0):
                    show("DERIVED", f"S_0 {s0:<4} e {e_ratio:<3}: open/closed (closed forges, p=0) {phi_f:6.2f}; (closed computes) "
                                    f"{phi_h:6.2f}; open/self-fund {phi_s:6.2f}; external joins {joins}", "")
        prem = [concentration_premium(c, x, total, mk.n_o) for x in (0.1, 0.25, 0.5)]
        split = [split_gain(c, 4.0, k, mk) for k in (2, 4, 10)]
        shares = flows(c, mk, external=total)
        show("DERIVED", "competing models: external capital = all existing capital arrives; top model's budget share / HHI",
             f"{shares[0]:.3f} / {sum(x * x for x in shares):.3f} (equal shares: {1 / (mk.n_o + 1):.3f} / {1 / (mk.n_o + 1):.3f})")
        show("DERIVED", "concentration premium at x = 0.1 / 0.25 / 0.5", " / ".join(f"{v:.2f}" for v in prem))
        show("DERIVED", "split gain (share of budget) of S_0 = 4 into k = 2 / 4 / 10", " / ".join(f"{v:+.4f}" for v in split))
        verdicts[c.name] = {
            "O (closed forges)": all(v >= BAR_PHI for v in o_forge),
            "O (closed computes)": all(v >= BAR_PHI for v in o_honest),
            "S": all(s_ok),
            "A": all(v <= 1.0 + 1e-9 for v in prem),
            "M": all(v <= 1e-12 for v in split),
        }
    print("  -- verdicts (True = the bar holds on the grid)")
    for name, v in verdicts.items():
        show("DERIVED", f"{name:18}", "  ".join(f"{k}: {'yes' if b else 'NO '}" for k, b in v.items()))
    # Theorems the sweep illustrates.
    check(not any(v["S"] for v in verdicts.values()), "F2: no curve makes publishing beat equal self-funding")
    check(not any(v["O (closed computes)"] and v["A"] for v in verdicts.values()),
          "F3: no curve meets bar O and bar A together (the publish premium IS a concentration premium)")
    check(not any(v["O (closed forges)"] for v in verdicts.values()),
          "F4: against a closed model that can forge (p = 0) no curve on the grid meets bar O")
    check(verdicts["S^1"]["A"] and verdicts["S^1"]["M"], "linear f is split- and concentration-neutral")
    check(not verdicts["S^0.5"]["M"] and not verdicts["min(S,2)"]["M"], "concave or capped f pays a model split")
    # The identity behind F3: the open premium (capital-constrained incumbent, closed computes, no competition effect) equals the
    # per-unit premium any capital holder gets at the same size.
    c = power(2.0)
    show("DERIVED", "F3 identity (f = S^2): g(2)/g(1) = publish premium at e = 1 = concentration premium of size 2 over size 1",
         f"{c.g(2.0) / c.g(1.0):.2f}")
    # F4: at p = 0 a closed forger earns 1/(1 − λ) per unit of capital what an honest open model of the same S_m earns, for EVERY f.
    show("DERIVED", "F4: closed forging / honest open, same S_m, every f", f"{1 / (1 - mk.lam):.2f}× (= 1/(1 − λ))")
    # Borrowing: a snapshot without a lock makes self-funding free.
    for r_b in (0.0, mk.r, 2 * mk.r, 5 * mk.r):
        phi = ratio(pi_open(power(1.0), 1.0, 1.0, mk), pi_self(power(1.0), 1.0, 1.0, mk, forge=True, r_b=r_b))
        show("DERIVED", f"linear, S_0 = 1, e = 1: open / self-fund at borrowing cost {r_b}/epoch", f"{phi:.2f}")
    # Cap saturation: a per-bond reward cap y_cap·S_0 (ADR-0176) bounds what the incumbent can be paid at all.
    y_cap = 1.5 / (total + 1.0)
    for c in (power(1.0), power(2.0)):
        closed = min(alloc(c, c.f(1.0), mk), y_cap * 1.0)
        opened = min(alloc(c, c.f(2.0), mk) * 0.5, y_cap * 1.0)
        show("DERIVED", f"(ASSUMED cap y_cap = 1.5 × the average yield) f = {c.name}: incumbent payout open / closed",
             f"{opened / closed:.2f}")


# =====================================================================================================================================


SECTIONS = {1: section1, 2: section2, 3: section3, 4: section4, 5: section5, 6: section6, 7: section7, 8: section8}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--section", type=int, choices=sorted(SECTIONS))
    args = ap.parse_args()
    for n, f in SECTIONS.items():
        if args.section is None or args.section == n:
            f()
    print(f"\n{len(FAILS)} check(s) failed" if FAILS else "\nall checks hold")
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
