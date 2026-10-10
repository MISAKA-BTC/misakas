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
# §2b The default-before-conviction rule: the 49% bound (I-49) against no dilution of an honest accuser (I-D, C4 F-C4R3-02)
# =====================================================================================================================================

RULES = {
    "NOW": "before F-C4R4-15 (econ-m2 measured 539): demanders paid D(1 - β_d) at the default; bounty min(a(S + D), S) (basis as if no default)",
    "O1": "one cap across both legs: bounty min(a(S + D) - demanders_paid, S)",
    "O2": "demanders credited only without a conviction: the share is held at the default; a conviction in the horizon pays one "
    "pool min(a(S + D), S) to the bounty holder and burns the held share; no conviction by the horizon pays the demanders then",
    "O3": "burn the whole pre-Final default penalty (β_d = 1): no demanders' share; bounty as today",
    "O4": "bounty on the slash alone: min(a·S, S) — what integration 323ea161a adopted (F-C4R4-15)",
}


def legs(t: Terms, rule: str, convicted: bool) -> tuple[Q, Q, str]:
    """For one claim with a pre-Final default (penalty D) and, if `convicted`, a conviction of the rest S = K - D: what ALL the
    demanders get together, what the bounty holder gets, and when the demanders are paid."""
    s_rest = t.K - t.D
    burn = Q(1) if rule == "O3" else t.beta_d
    share = t.D - floor_q(t.D * burn)
    if not convicted:
        return share, Q(0), ("at the horizon (default + liability)" if rule == "O2" else "at the default")
    basis = t.K if rule != "O4" else s_rest
    pool = floor_q(t.a * basis)
    if rule == "O1":
        return share, min(pool - share, s_rest), "at the default"
    if rule == "O2":
        return Q(0), min(pool, s_rest), "never (folded into the conviction's pool)"
    return share, min(pool, s_rest), "at the default"


def section2b() -> None:
    head(2, "b — default before conviction: I-49 (≤ 49% recouped) against I-D (no dilution of an honest accuser)")
    t = INTERIM_49
    rows = {}
    for rule, text in RULES.items():
        print(f"  -- {rule}: {text}")
        dem, bounty, when = legs(t, rule, convicted=True)
        dem_nc, _, when_nc = legs(t, rule, convicted=False)
        self_recoup = dem + bounty  # every demander and the bounty holder are the producer's own Sybils
        honest_bounty = bounty  # the coalition self-defaults through its Sybil demander; an honest accuser holds the bounty
        honest_bounty_no_default = floor_q(t.a * t.K)  # the same lie, convicted without a default first
        i49 = self_recoup <= floor_q(t.a * t.K)
        idil = honest_bounty >= honest_bounty_no_default
        rows[rule] = (i49, idil)
        show("DERIVED", "all roles the producer's own: recoup / collected",
             f"{float(self_recoup):.0f} / 1000 = {float(self_recoup) / 10:.1f}%  → I-49 {'holds' if i49 else 'BROKEN'}")
        show("DERIVED", "honest accuser after the coalition's self-default / without it",
             f"{float(honest_bounty):.0f} / {float(honest_bounty_no_default):.0f}  → I-D {'holds' if idil else 'BROKEN'}"
             + ("" if idil else f" (diluted by {float(honest_bounty_no_default - honest_bounty):.0f})"))
        show("DERIVED", "honest demanders on a defaulted lie that someone convicts", f"{float(dem):.0f}, paid {when}")
        show("DERIVED", "honest demanders on a default with no conviction", f"{float(dem_nc):.0f}, paid {when_nc}")
        show("DERIVED", "an honest demander beside one Sybil demander (no conviction)", f"{float(dem_nc / 2):.1f}")
    check(rows["NOW"] == (False, True), "before F-C4R4-15: I-D held, I-49 was broken (53.9%)")
    check(rows["O1"] == (True, False) and rows["O4"] == (True, False), "one cap (O1) or a slash-only basis (O4) breaks I-D")
    check(rows["O2"] == (True, True) and rows["O3"] == (True, True), "O2 and O3 keep both invariants")
    # Under M*-49 the honest drawn sealers take the first B_cap of the pool: O1's dilution reaches them only if pool - share < B_cap.
    b_cap = 2 * t.G
    _, bounty, _ = legs(t, "O1", convicted=True)
    show("DERIVED", "under M*-49 with O1: the pool left after the demanders' share vs B_cap = m·G", f"{float(bounty):.0f} vs {float(b_cap):.0f}")
    check(bounty >= b_cap, "under M*-49, O1's dilution falls on the earliest sealer's remainder, not on the drawn sealers")
    # How large O1's dilution can get: D(1 - β_d) ≤ a·D, i.e. at most D/K of the honest bounty a·K.
    worst = max(
        (legs(replace(t, D=Q(d)), "NOW", True)[1] - legs(replace(t, D=Q(d)), "O1", True)[1]) / floor_q(t.a * t.K)
        for d in range(0, 511, 10)
    )
    show("DERIVED", "O1: the worst dilution of an honest accuser over D ∈ [0, 510]", f"{float(worst):.1%} of its bounty (≤ D/K)")


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
# §9 The ECON gate of docs/palw-round-exec-additional-acceptance-2026-10-10.md §4–§5: small vs large models, same bond and period
# =====================================================================================================================================

TICKET_PWU = 100_000  # MEASURED: PALW_EXECUTION_QUANTUM_V1 (one ticket per 100,000 pwu of verified CanonicalWork)
SLOTS_PER_DAA = 150  # ASSUMED: one Round slot a second (120 per 120 s, MEASURED on t12) at 150 s per DAA
SPAN_BOUND = 65_536  # MEASURED: PALW_EXEC_MAX_QUANTA_PER_SPAN_V1


@dataclass(frozen=True)
class GateModel:
    """One model class as the gate needs it. Work is CanonicalWork (pwu, compute-proportional by construction: graph × positions)."""

    name: str
    params_b: float  # parameter count (billions) — recorded, never read by any rule
    tokens_per_job: int
    mpwu_per_job: float  # canonical work per job, in millions of pwu
    sec_per_mpwu: float  # processing time on the bond's hardware


SMALL = GateModel("small 0.5B", 0.5, 1024, 1.6, 1.25)  # MEASURED scale: "a ~1.6M-pwu QWEN25-scale job is about 16 tickets"
LARGE = GateModel("large 9B", 9.0, 1024, 28.8, 1.25)  # ASSUMED: work ∝ parameters at the same tokens (18×)


@dataclass(frozen=True)
class GateEnv:
    """ASSUMED baseline (every value is an illustration; P-1..P-11 are not set)."""

    capital: float = 13_000.0  # MEASURED: t12's producer minimum bond (RFC-0015 §8.3.6)
    unit: float = 1_000.0  # u
    window_daa: int = 400  # W — inside the band P-1/P-11 derive (§10)
    rights_per_unit: float = 30.0  # Round rights per u per W
    claims_per_unit: float = 5.0  # q·ρ per u per W
    reward_per_unit: float = 4.0  # r: escrowed claim reward per u per W (BILI)
    price_per_mpwu: float = 1.0  # escrow the users pay per Mpwu of work (market)
    compute_per_mpwu: float = 0.5  # the producer's compute cost per Mpwu
    large_cost_mult: float = 1.0  # μ_L: a large model's compute cost per Mpwu relative to a small one's (memory-bound decode, GPUs)
    ticket_subsidy: float = 0.05  # v_t: the Round's work reward per executed ticket
    ticket_fee: float = 0.01  # f_t: market fees per executed Round
    per_claim_overhead: float = 1.04  # INTERIM: OPV admission fee 1 + seal/reveal carriers
    capital_cost_per_year: float = 0.10  # 10% a year on the locked bond
    participants: int = 10  # bonds of the same size competing for the shared window (each fields its capped candidates)
    demand_mpwu: float = 1_000.0  # work the market offers this bond per W


def gate_row(m: GateModel, env: GateEnv, demand_mpwu: float | None = None) -> dict:
    d = env.demand_mpwu if demand_mpwu is None else demand_mpwu
    caps_u = env.capital / env.unit
    q_max = math.floor(caps_u * env.claims_per_unit)
    r_max = caps_u * env.reward_per_unit
    rounds_cap = math.floor(caps_u * env.rights_per_unit)
    hw_jobs = math.floor(env.window_daa * 150 / (m.mpwu_per_job * m.sec_per_mpwu))  # ASSUMED 150 s/DAA, one accelerator
    escrow_job = env.price_per_mpwu * m.mpwu_per_job
    jobs_by = {"demand": math.floor(d / m.mpwu_per_job), "Q cap": q_max, "R cap": math.floor(r_max / escrow_job), "hardware": hw_jobs}
    jobs = min(jobs_by.values())
    binding = min(jobs_by, key=jobs_by.get)
    work = jobs * m.mpwu_per_job
    earned = work * 1e6 / TICKET_PWU
    candidates = min(earned, rounds_cap)
    capacity = env.window_daa * SLOTS_PER_DAA
    others = (env.participants - 1) * rounds_cap
    s = min(1.0, capacity / (candidates + others)) if candidates + others > 0 else 1.0
    allocated = candidates * s
    executed = allocated  # ASSUMED: every allocated permit is used
    cost_mult = env.large_cost_mult if m is LARGE else 1.0
    compute = work * env.compute_per_mpwu * cost_mult
    fees = executed * env.ticket_fee
    reward = jobs * escrow_job + executed * env.ticket_subsidy
    overhead = jobs * env.per_claim_overhead
    capital_cost = env.capital * env.capital_cost_per_year * env.window_daa * 150 / 31_536_000
    revenue = reward + fees
    net = revenue - compute - overhead - capital_cost
    tickets_per_sec = (m.mpwu_per_job * 1e6 / TICKET_PWU) / (m.mpwu_per_job * m.sec_per_mpwu)
    demand_rate = d * 1e6 / TICKET_PWU / (env.window_daa * 150)
    rate = min(tickets_per_sec, demand_rate)
    time_to_cap = rounds_cap / rate / 150 if earned >= rounds_cap and rate > 0 else math.inf
    # Marginal revenue of one more job at this point: tickets only below the Round cap; escrow only below R and Q caps.
    t_job = m.mpwu_per_job * 1e6 / TICKET_PWU
    below_round = max(0.0, min(t_job, rounds_cap - earned)) * s * (env.ticket_subsidy + env.ticket_fee)
    room = jobs < min(jobs_by["Q cap"], jobs_by["R cap"])
    marginal = (escrow_job + below_round - m.mpwu_per_job * env.compute_per_mpwu * cost_mult - env.per_claim_overhead) if room else 0.0
    return dict(
        model=m.name, params_b=m.params_b, tokens=jobs * m.tokens_per_job, jobs=jobs, binding=binding, canonical_mpwu=work,
        compute=compute, time_s=work * m.sec_per_mpwu, earned=earned, candidates=candidates, allocated=allocated, executed=executed,
        occupancy=(allocated + others * s) / capacity, own_share=allocated / capacity, fees=fees, work_reward=reward,
        rev_per_compute=revenue / compute if compute else math.nan, net=net, net_per_compute=net / compute if compute else math.nan,
        capital_cost=capital_cost, time_to_cap_daa=time_to_cap, marginal_past=marginal, round_cap=rounds_cap, sat=s,
        sum_caps=env.participants * rounds_cap,
    )


# The criteria, FIXED BEFORE any result is read (PROPOSED; the user may change them, but not after seeing the numbers).
CRITERIA = {
    "G1": "below every cap, unsaturated: tickets per Mpwu equal across sizes (±2%) and revenue per compute cost large/small in [0.8, 1.25]",
    "G1n": "the same with per-claim overhead and capital cost included (net revenue per compute cost) — must also lie in [0.8, 1.25]",
    "G2": "past the Round cap the marginal Round revenue is 0 for both sizes (capital, not compute, bounds Round income)",
    "G3": "saturated: the allocation probability per candidate is equal across sizes (±1%)",
    "G4": "both sizes net-viable at the baseline; the grid share where exactly one size is viable is reported with its cause",
    "G5": "no rule reads the parameter count (structural)",
    "G6": "the claim-count cap Q does not bind before the Round cap for the smallest job (Q_max · tickets_per_job ≥ Round cap)",
    "G7": "split: k small jobs and one large job of the same total work field the same candidates (≤ 1 ticket per Final)",
    "G8": "P-11: Σ Round caps per W < 65,536 in every scenario judged",
}


def section9() -> None:
    head(9, "ECON gate (Round/EXEC acceptance §4–§5): small vs large models, same bond and period")
    for k, v in CRITERIA.items():
        show("PROPOSED", f"criterion {k}", v)
    base = GateEnv()
    scenarios = {
        "before cap, unsaturated": replace(base, demand_mpwu=28.8, participants=10),
        "after cap, unsaturated": replace(base, demand_mpwu=1_000.0, participants=10),
        "before cap, saturated": replace(base, demand_mpwu=28.8, participants=160),
        "after cap, saturated": replace(base, demand_mpwu=1_000.0, participants=160),
    }
    keys = ("jobs", "binding", "canonical_mpwu", "tokens", "compute", "time_s", "candidates", "allocated", "executed", "occupancy",
            "fees", "work_reward", "rev_per_compute", "net_per_compute", "capital_cost", "time_to_cap_daa", "marginal_past")
    results = {}
    for name, env in scenarios.items():
        print(f"  -- {name} (demand {env.demand_mpwu:g} Mpwu/W, {env.participants} bonds, Round cap {gate_row(SMALL, env)['round_cap']}"
              f" tickets/W, Σ caps {gate_row(SMALL, env)['sum_caps']:,})")
        for m in (SMALL, LARGE):
            r = gate_row(m, env)
            results[(name, m.name)] = r
            cells = []
            for k in keys:
                v = r[k]
                cells.append(f"{k}={v:.3g}" if isinstance(v, float) else f"{k}={v}")
            show("DERIVED", f"{m.name:10}", ", ".join(cells))
    judged = {}
    # G1 / G1n: below every cap and unsaturated — use equal work that fits under the caps (one large job vs 18 small).
    env = replace(base, demand_mpwu=28.8, participants=10)
    rs, rl = gate_row(SMALL, env), gate_row(LARGE, env)
    tpm = (rl["candidates"] / rl["canonical_mpwu"]) / (rs["candidates"] / rs["canonical_mpwu"])
    rpc = rl["rev_per_compute"] / rs["rev_per_compute"]
    npc = rl["net_per_compute"] / rs["net_per_compute"] if rs["net_per_compute"] > 0 else math.inf
    show("DERIVED", "G1 point: 28.8 Mpwu each (18 small jobs vs 1 large), Round cap 390, 10 bonds",
         f"tickets/Mpwu ratio {tpm:.3f}; revenue/compute ratio {rpc:.3f}; net/compute ratio {npc:.3f} (small {rs['net_per_compute']:.2f},"
         f" large {rl['net_per_compute']:.2f})")
    judged["G1"] = abs(tpm - 1) <= 0.02 and 0.8 <= rpc <= 1.25
    judged["G1n"] = 0.8 <= npc <= 1.25
    # G7: the same point.
    judged["G7"] = abs(rs["candidates"] - rl["candidates"]) <= 18
    # G2: past the cap, unsaturated.
    for m in (SMALL, LARGE):
        r = results[("after cap, unsaturated", m.name)]
        t_job = m.mpwu_per_job * 10
        extra_round = max(0.0, min(t_job, r["round_cap"] - r["earned"]))
        judged.setdefault("G2", True)
        judged["G2"] = judged["G2"] and extra_round == 0
    # G3: saturated — the per-candidate allocation probability is the same s for both.
    a, b = results[("after cap, saturated", SMALL.name)], results[("after cap, saturated", LARGE.name)]
    judged["G3"] = abs(a["sat"] - b["sat"]) <= 0.01
    # G5 structural: gate_row reads params_b only to record it.
    judged["G5"] = True
    # G6.
    caps_u = base.capital / base.unit
    judged["G6"] = math.floor(caps_u * base.claims_per_unit) * SMALL.mpwu_per_job * 10 >= math.floor(caps_u * base.rights_per_unit)
    judged["G8"] = all(gate_row(SMALL, e)["sum_caps"] < SPAN_BOUND for e in scenarios.values())
    # G4: viability on the sensitivity grid.
    grid = []
    for demand in (10.0, 100.0, 1_000.0):
        for fee in (0.0, 0.01, 0.1):
            for c in (0.25, 0.5, 1.0):
                for mu in (1.0, 1.5, 2.0):
                    for parts in (10, 80, 160):
                        e = replace(base, demand_mpwu=demand, ticket_fee=fee, compute_per_mpwu=c, large_cost_mult=mu, participants=parts)
                        grid.append((e, gate_row(SMALL, e)["net"] > 0, gate_row(LARGE, e)["net"] > 0))
    n = len(grid)
    both = sum(1 for _, s_, l_ in grid if s_ and l_)
    only_s = sum(1 for _, s_, l_ in grid if s_ and not l_)
    only_l = sum(1 for _, s_, l_ in grid if l_ and not s_)
    neither = n - both - only_s - only_l
    show("DERIVED", f"sensitivity grid ({n} cells: demand × fee × compute × μ_L × participants): both / only small / only large / neither",
         f"{both} / {only_s} / {only_l} / {neither}")
    by_demand = {dm: sum(1 for e, s_, l_ in grid if e.demand_mpwu == dm and l_ and not s_) for dm in (10.0, 100.0, 1_000.0)}
    show("DERIVED", "cells where only the large model is viable, by demand (Mpwu/W)", by_demand)
    by_dem_s = {dm: sum(1 for e, s_, l_ in grid if e.demand_mpwu == dm and s_ and not l_) for dm in (10.0, 100.0, 1_000.0)}
    show("DERIVED", "cells where only the small model is viable, by demand (Mpwu/W; a large job is 28.8)", by_dem_s)
    by_c = {c: sum(1 for e, s_, l_ in grid if e.compute_per_mpwu == c and not s_ and l_) for c in (0.25, 0.5, 1.0)}
    show("DERIVED", "cells where only the large model is viable, by compute cost per Mpwu", by_c)
    rb_s, rb_l = gate_row(SMALL, base), gate_row(LARGE, base)
    judged["G4"] = rb_s["net"] > 0 and rb_l["net"] > 0
    show("DERIVED", "baseline net per W: small / large", f"{rb_s['net']:.1f} / {rb_l['net']:.1f} BILI (capital cost {rb_s['capital_cost']:.0f})")
    # The overhead finding: the per-claim fixed fee against the smallest job's escrow.
    show("DERIVED", "per-claim overhead / escrow per job: small / large",
         f"{base.per_claim_overhead / (base.price_per_mpwu * SMALL.mpwu_per_job):.0%} / "
         f"{base.per_claim_overhead / (base.price_per_mpwu * LARGE.mpwu_per_job):.1%}")
    show("DERIVED", "least bond for one large job's full tickets (288) at the baseline rights", f"{288 * base.unit / base.rights_per_unit:,.0f} BILI")
    for k in CRITERIA:
        show("DERIVED", f"verdict {k}", "PASS" if judged.get(k) else "FAIL")
    check(judged["G1"] and not judged["G1n"], "G1 holds on compute, and fails once per-claim overhead is counted (small jobs pay it 18×)")
    big = results[("after cap, unsaturated", LARGE.name)]
    show("DERIVED", "G2's cause: at high demand the large model is stopped by", f"the {big['binding']} (one 28.8-BILI escrow fills "
         f"⌊{base.capital / base.unit * base.reward_per_unit:.0f} / 28.8⌋ = 1 job) at {big['candidates']:.0f} of {big['round_cap']} tickets")
    check(not judged["G2"] and big["binding"] == "R cap", "G2 fails for the large model by the R cap's granularity, not by compute")
    check(judged["G3"] and judged["G7"], "saturation and splitting are size-neutral")


# =====================================================================================================================================
# §10 BUDGET's POLICY list P-1..P-11: values or methods the models derive
# =====================================================================================================================================


def section10() -> None:
    head(10, "BUDGET POLICY P-1..P-11 (bond-budget-and-model-allocation.md §10)")
    t = INTERIM_49
    lifetime = t.hold_max
    show("DERIVED", "P-1 W lower bound: W ≥ L, the longest claim lifetime (payments per span ≤ 2·R_max, BUDGET §2.4)", f"W ≥ {lifetime} DAA")
    for slots in (120, 150):
        show("DERIVED", f"P-1/P-11 W upper bound if the window must be able to saturate: W ≤ 65,536 / slots per DAA ({slots})",
             f"W ≤ {SPAN_BOUND // slots} DAA")
    show("DERIVED", "P-1 band at the interim terms (ASSUMED 150 s/DAA)", f"{lifetime} ≤ W ≤ {SPAN_BOUND // 150} DAA")
    for w in (400, 4032):
        for p in (0.01, 0.05):
            y = yield_ceiling(p, 0.49, 1.0, 0.0, 0.0, w, t.hold)
            show("DERIVED", f"P-2 r/u ceiling (Theorem Y; λ = 1, x = 0) at W = {w}, p = {p}", f"r ≤ {y * 1000:.1f} BILI per u = 1,000 BILI per W")
    show("DERIVED", "P-2 r, w, fees together", "(r + w·value_of_weight + expected fee income) per u per W ≤ the Theorem Y ceiling")
    show("DERIVED", "P-2 q·ρ lower bound (G6): claims per u per W ≥ Round rights per u per W / tickets of the smallest admitted job", "e.g. 10 / 16 = 0.63")
    show("DERIVED", "P-2 open-claim cap (§2.5, κ ≤ 1)", f"max_open_claims ≤ ⌊C / K⌋ = {13_000 // 1_000} at C = 13,000, K = 1,000")
    show("PROPOSED", "P-3 ρ, slice_rights_by_rho", "slice = true (D2's A → A/m); ρ from the Q need above, never to raise B/R/F")
    show("PROPOSED", "P-4 epoch E", "E = W (one clock), or a whole multiple of W")
    show("PROPOSED", "P-5 seasoning", "≥ 1 full epoch, with the capital locked ≥ E + W (no point snapshot; D-snap)")
    show("PROPOSED", "P-6 f and the bar", "linear (§5: the only split- and concentration-neutral curve) or no model leg (D-F); bar per §5.4")
    show("PROPOSED", "P-7 ΣA = 0", "keep v1: not minted")
    show("ASSUMED", "P-8 max_models_per_bond", "no economic derivation under linear f (splitting is neutral); bound it by snapshot work, e.g. 8")
    show("PROPOSED", "P-9 fee-only Rounds", "ExecutionCap { rights_per_unit = the Round rights } — keeps B for reward blocks; either mode is capped")
    show("PROPOSED", "P-10 fee income and R_max", "not in R_max (v1); count the expected fee income in Theorem Y's x when setting r and rights")
    for c_total in (1e6, 1e7):
        show("DERIVED", f"P-11 rights per u per W so that Σ caps < 65,536 with total locked capital {c_total:,.0f} BILI",
             f"≤ {SPAN_BOUND * 1000 / c_total:.2f}")


# =====================================================================================================================================
# §11 f(S_m) under the user's premise (readiness §3f): bond gathered = users gathered; an honest-capital majority as in BFT
# =====================================================================================================================================


def binom_tail(m: int, p: float, k: int) -> float:
    """P(Bin(m, p) ≥ k)."""
    return sum(binom_pmf(m, j, p) for j in range(k, m + 1))


SIGMA_MAX = 1 / 3  # PROPOSED security assumption: the adversary holds at most 1/3 of locked capital AND of verifier stake (BFT)
EPS_CLOSED = 0.01  # PROPOSED bar B-R: a closed model's model-leg pay probability ≤ 1% (the stake-only residual)
DELTA_OPEN = 0.01  # PROPOSED bar B-H: an honest open model is paid with probability ≥ 99%
PHI_BAR = 2.0  # PROPOSED bar B-O: open ≥ 2× closed for the incumbent, net of the closed model's forging saving (λ)


def gate_probs(m: int, k: int, sigma: float, fetch_fail: float = 0.0) -> tuple[float, float]:
    """The verified-work gate: per model and epoch, m slots drawn by stake from the GLOBAL verifier pool each check one sampled claim
    with their own copy of the model; the model leg is paid iff ≥ k attest. A closed model: only adversarial slots can attest
    (an honest slot has no copy). An honest open model: adversarial slots withhold (griefing); an honest slot attests unless it failed
    to fetch the model in time."""
    closed = binom_tail(m, sigma, k)
    opened = binom_tail(m, (1 - sigma) * (1 - fetch_fail), k)
    return closed, opened


def smallest_gate(sigma: float, eps: float, delta: float, m_max: int = 201) -> tuple[int, int] | None:
    for m in range(1, m_max + 1):
        for k in range(1, m + 1):
            c, o = gate_probs(m, k, sigma)
            if c <= eps and o >= 1 - delta:
                return m, k
    return None


def gated_publish_ratio(c: Curve, s0: float, e: float, mk: Market, m: int, k: int, sigma: float, omega: float = 0.0,
                        fetch_fail: float = 0.0) -> float:
    """Incumbent's model-leg income open / closed under the gate (gross of the capital cost both pay). Closed: the model leg times P_closed, no compute (it forges, p = 0). Open:
    times P_open, computes honestly; pro-rata intra-model split, plus an owner leg ω of the model's allocation."""
    p_c, p_o = gate_probs(m, k, sigma, fetch_fail)
    if not external_joins(c, s0, e, mk):
        e = 0.0
    a_open = alloc(c, c.f(s0 + e), mk) * p_o
    # Gross of the capital cost, which both pay alike on S_0 (net of it the closed side is often negative, an infinite ratio).
    open_income = omega * a_open + (1 - omega) * a_open * s0 / (s0 + e) * (1 - mk.lam)
    closed_income = alloc(c, c.f(s0), mk) * p_c
    return ratio(open_income, closed_income)


def section11() -> None:
    head(11, "f(S_m) under 'bond gathered = users gathered' with an honest-capital majority (BFT premise)")
    show("PROPOSED", "assumption A-BFT", f"the adversary controls ≤ σ_max = {SIGMA_MAX:.3f} of locked model capital and of the global "
                                         "verifier stake; honest = protocol-following (attests only what it checked)")
    show("PROPOSED", "bars (fixed before judging)", f"B-O open ≥ {PHI_BAR}× closed net of forging; B-R closed paid ≤ {EPS_CLOSED:.0%};"
                                                     f" B-H honest open paid ≥ {1 - DELTA_OPEN:.0%}; B-A adversary share ≤ its capital share;"
                                                     " B-M no split gain; all at σ ≤ σ_max")
    mk = Market()
    # 1. Without a verification gate the premise alone does not help: F4 still gives the closed forger 1/(1 − λ).
    show("DERIVED", "no gate: closed forging / honest open per unit capital, any f", f"{1 / (1 - mk.lam):.2f}× (F4 unchanged)")
    # 2. The gate's smallest (m, k) at σ_max.
    g = smallest_gate(SIGMA_MAX, EPS_CLOSED, DELTA_OPEN)
    check(g is not None, "a gate meeting B-R and B-H exists at σ = 1/3")
    m, k = g
    pc, po = gate_probs(m, k, SIGMA_MAX)
    show("DERIVED", "smallest gate (m slots, k attestations) meeting B-R and B-H at σ = 1/3", f"m = {m}, k = {k}: P_closed {pc:.4f}, P_open {po:.4f}")
    for sig in (0.2, 0.25):
        gg = smallest_gate(sig, EPS_CLOSED, DELTA_OPEN)
        show("DERIVED", f"smallest gate at σ = {sig}", f"m = {gg[0]}, k = {gg[1]}")
    # 3. The bars for each curve, under the gate, at σ = σ_max.
    total = mk.n_o * mk.s_o
    verdict = {}
    for c in (power(1.0), power(0.5), capped(2.0), power(2.0), threshold(2.0)):
        phis = [gated_publish_ratio(c, s0, e * s0, mk, m, k, SIGMA_MAX) for s0 in (0.25, 1.0, 4.0) for e in (0.0, 1.0, 9.0)]
        prem = [concentration_premium(c, x, total, mk.n_o) * pc / po for x in (0.1, 0.25, SIGMA_MAX)]
        split = [split_gain(c, 4.0, kk, mk) for kk in (2, 4, 10)]
        verdict[c.name] = {"B-O": min(phis) >= PHI_BAR, "B-R": pc <= EPS_CLOSED, "B-H": po >= 1 - DELTA_OPEN,
                           "B-A": max(prem) <= 1.0, "B-M": max(split) <= 1e-12}
        show("DERIVED", f"f = {c.name:14} gated: min open/closed over S_0 ∈ {{0.25,1,4}}, e ∈ {{0,1,9}}", f"{min(phis):.1f}×; "
             f"adversary premium (paid share / capital share) ≤ {max(prem):.3f}; split gain ≤ {max(split):+.4f}")
    for name, v in verdict.items():
        show("DERIVED", f"{name:16}", "  ".join(f"{b}: {'yes' if ok else 'NO '}" for b, ok in v.items()))
    check(all(verdict["S^1"].values()), "under A-BFT with the gate, linear f meets every bar")
    # 4. Owner leg: what an intra-model owner share adds (paid only out of the model's verified, gated allocation).
    for omega in (0.0, 0.05, 0.10):
        phi = gated_publish_ratio(power(1.0), 1.0, 9.0, mk, m, k, SIGMA_MAX, omega=omega)
        show("DERIVED", f"linear, S_0 = 1, e = 9, owner leg ω = {omega}", f"open/closed {phi:.1f}×")
    # 5. Where it breaks.
    print("  -- where it breaks (linear f, the σ = 1/3 gate)")
    for sig in (0.30, 1 / 3, 0.40, 0.45, 0.50):
        c_, o_ = gate_probs(m, k, sig)
        phi = gated_publish_ratio(power(1.0), 1.0, 0.0, mk, m, k, sig)
        show("DERIVED", f"σ = {sig:.3f}", f"P_closed {c_:.4f}, P_open {o_:.4f}, open/closed at e = 0: {phi:.1f}×"
             + ("" if c_ <= EPS_CLOSED and o_ >= 1 - DELTA_OPEN else "  ← a bar fails"))
    crit = next(s / 1000 for s in range(334, 501) if gated_publish_ratio(power(1.0), 1.0, 0.0, mk, m, k, s / 1000) < PHI_BAR)
    show("DERIVED", "σ at which B-O (2×) first fails for this gate", f"{crit:.3f}")
    for ff in (0.0, 0.02, 0.05, 0.10):
        c_, o_ = gate_probs(m, k, SIGMA_MAX, ff)
        show("DERIVED", f"honest fetch failure {ff:.0%} (open but slow to obtain)", f"P_open {o_:.4f}" + ("" if o_ >= 0.99 else "  ← B-H fails"))
    # 6. The gate's price: m checks per model per epoch at F_min (§6 ASSUMED costs).
    f_min = 5.59
    show("DERIVED", "the gate's cost per model per epoch (m · F_min, F_min ASSUMED 5.59)", f"{m * f_min:.0f} BILI")
    show("DERIVED", "the residual stake-only issuance a σ = 1/3 closed coalition can draw (linear f)",
         f"≤ P_closed · σ = {pc * SIGMA_MAX:.4%} of the model budget")


# =====================================================================================================================================
# §12 ADR-0177's revised goal (2026-10-10): strongly favour models that gather more effective locked miner bond; A_m = S_m^α
# =====================================================================================================================================


@dataclass(frozen=True)
class AggCosts:
    """Variable costs per BILI of model-leg payment (ASSUMED until measured) — they define d."""

    kappa: float = 0.5  # compute a verified claim needs per BILI of model-leg payment (the claim-work requirement)
    phi_pub: float = 0.5  # share of that compute the public model's users already pay through escrow
    phi_closed: float | None = None  # the same for the closed model; None = the same user market as the public one (users need no
    # weights to post jobs); 0 = it self-posts its jobs
    overhead: float = 0.05  # per-claim fees per BILI paid (admission fee, carriers)
    access: float = 0.02  # a joiner's cost to obtain, store and serve the public model, per BILI paid


@dataclass(frozen=True)
class AggNet:
    """ASSUMED network: total model capital 100 units, model budget 1 per epoch (average yield 0.01 per unit), 20 other models."""

    total: float = 100.0
    n_other: int = 20
    y_cap: float = 0.02  # the per-bond cap R_max / C per epoch (ADR-0176), here 2× the average yield
    r: float = 0.0025  # capital cost per unit per epoch (a quarter of the average yield)


def agg_rho(alpha: float, s_m: float, caps: list[float]) -> float:
    """Per-unit allocation of model m: S_m^(α−1) / Σ_j S_j^α (budget 1)."""
    return s_m ** (alpha - 1) / sum(x**alpha for x in caps)


def agg_world(alpha: float, s_pub: float, s_closed: float, net: AggNet) -> tuple[float, float, float]:
    rest = net.total - s_pub - s_closed
    caps = [s_pub, s_closed] + [rest / net.n_other] * net.n_other
    return agg_rho(alpha, s_pub, caps), agg_rho(alpha, s_closed, caps), sum(x**alpha for x in caps)


def margins(c: AggCosts) -> dict:
    """Net margin per BILI of model-leg payment. d (the cost divisor of the user's sketch) = closed margin / public margin."""
    phi_c = c.phi_pub if c.phi_closed is None else c.phi_closed
    pub = 1 - c.overhead - c.kappa * (1 - c.phi_pub) - c.access
    closed_h = 1 - c.overhead - c.kappa * (1 - phi_c)  # region H: the closed owner computes honestly
    closed_z = 1 - c.overhead + c.kappa * phi_c  # region Z (p = 0): it forges, and keeps its users' escrow if any
    return {"pub": pub, "H": closed_h, "Z": closed_z, "d_H": closed_h / pub, "d_Z": closed_z / pub}


AGG_BAR = {
    "range": (4.0, 30.0),  # S_public / S_closed, with S_closed = 1% of all model capital
    "multiplier": 2.0,  # per-unit-capital NET profit of joining the large public model ≥ 2× self-mining the small closed one
    "basis": "net profit (after compute, fees, access and capital cost), region H, wherever the public model is below the cap",
    "cost grid": "κ ∈ {0.3, 0.5, 0.7}, φ_pub ∈ {0, 0.5, 1}, overhead 0.05, access ∈ {0.02, 0.05}",
}


def agg_profit(alpha: float, ratio_pc: float, c: AggCosts, net: AggNet, region: str) -> dict:
    s_c = 0.01 * net.total
    s_p = ratio_pc * s_c
    rp, rc, _ = agg_world(alpha, s_p, s_c, net)
    mg = margins(c)
    pay_p, pay_c = min(rp, net.y_cap), min(rc, net.y_cap)
    prof_p = pay_p * mg["pub"] - net.r
    prof_c = pay_c * mg[region] - net.r
    return {"alloc": rp / rc, "payment": pay_p / pay_c, "net": ratio(prof_p, prof_c), "capped": rp >= net.y_cap,
            "withheld": (rp - net.y_cap) * s_p if rp > net.y_cap else 0.0, "prof_p": prof_p, "prof_c": prof_c}


def agg_saturation(alpha: float, net: AggNet, s_c: float = 1.0) -> float | None:
    """The public capital at which its per-unit allocation reaches the per-bond cap (the advantage stops growing there)."""
    for i in range(1, 1000):
        s_p = i * 0.1
        if s_p + s_c >= net.total:
            return None
        if agg_world(alpha, s_p, s_c, net)[0] >= net.y_cap:
            return s_p
    return None


def agg_flows(alpha: float, net: AggNet, c: AggCosts, step: float = 0.5, moving: float = 50.0) -> tuple[float, float, float]:
    """Participant movement: `moving` units of capital leave equal small models one step at a time for the model where a unit
    earns the most (capped payment × public margin). Returns (top model's capital share, its reward share, withheld budget share)."""
    n = net.n_other + 2
    caps = [net.total / n] * n
    mg = margins(c)["pub"]
    for _ in range(int(moving / step)):
        tot = sum(x**alpha for x in caps)
        best = max(range(n), key=lambda j: (round(min((caps[j] + step) ** (alpha - 1) / tot, net.y_cap) * mg, 12), -j))
        movers = [j for j in range(n) if j != best and caps[j] >= step]
        if not movers:
            break
        src = min(movers, key=lambda j: (round(min(caps[j] ** (alpha - 1) / tot, net.y_cap), 12), -j))
        here = min(caps[src] ** (alpha - 1) / tot, net.y_cap)
        there = min((caps[best] + step) ** (alpha - 1) / tot, net.y_cap)
        if there <= here:
            break
        caps[src] -= step
        caps[best] += step
    tot = sum(x**alpha for x in caps)
    alloc_ = [x**alpha / tot for x in caps]
    paid = [min(a, net.y_cap * x) for a, x in zip(alloc_, caps)]
    top = max(range(n), key=lambda j: caps[j])
    return caps[top] / net.total, paid[top] / sum(paid), 1 - sum(paid)


def section12() -> None:
    head(12, "ADR-0177 revised goal: bond aggregation favoured, A_m = S_m^α (readiness §3g)")
    for k, v in AGG_BAR.items():
        show("PROPOSED", f"bar, {k}", v)
    net, base = AggNet(), AggCosts()
    mg = margins(base)
    show("DERIVED", "d (closed margin / public margin per BILI paid) at the baseline costs", f"d_H {mg['d_H']:.3f} (honest closed), "
         f"d_Z {mg['d_Z']:.3f} (closed forging at p = 0)")
    for phi in (0.0, 0.5, 1.0):
        m2 = margins(replace(base, phi_pub=phi))
        show("DERIVED", f"d with users paying φ_pub = {phi} of the compute", f"d_H {m2['d_H']:.3f}, d_Z {m2['d_Z']:.3f}")
    alphas = (1.0, 1.25, 1.5, 2.0, 3.0)
    verdict = {}
    for a in alphas:
        sat = agg_saturation(a, net)
        print(f"  -- α = {a}: the public model reaches the per-bond cap at S_public ≈ {sat if sat is None else round(sat, 1)} "
              f"(of {net.total:g}; S_closed = 1)")
        cells = fails = 0
        failing = set()
        for rpc in (4.0, 10.0, 30.0):
            rh, rz = agg_profit(a, rpc, base, net, "H"), agg_profit(a, rpc, base, net, "Z")
            show("DERIVED", f"S_pub/S_closed = {rpc:>4g}: allocation / payment / net (H) / net (Z)",
                 f"{rh['alloc']:7.2f} / {rh['payment']:6.2f} / {rh['net']:6.2f} / {rz['net']:6.2f}" + ("  capped" if rh["capped"] else ""))
        for kappa in (0.3, 0.5, 0.7):
            for phi in (0.0, 0.5, 1.0):
                for acc in (0.02, 0.05):
                    c = replace(base, kappa=kappa, phi_pub=phi, access=acc)
                    for rpc in (4.0, 10.0, 30.0):
                        r_ = agg_profit(a, rpc, c, net, "H")
                        if r_["capped"]:
                            continue
                        cells += 1
                        if not (r_["prof_p"] > 0 and r_["net"] >= AGG_BAR["multiplier"]):
                            fails += 1
                            failing.add((kappa, phi, rpc))
        verdict[a] = (cells, fails, failing)
    for a, (cells, fails, failing) in verdict.items():
        why = sorted({(k_, p_) for k_, p_, _ in failing})
        show("DERIVED", f"verdict α = {a}: cells below the cap passing / judged", f"{cells - fails} / {cells}"
             + (f"; failing (κ, φ_pub): {why}" if why else "  → PASS"))
    check(verdict[1.0][1] == verdict[1.0][0], "α = 1 never gives a 2× net advantage")
    check(all(k_ == 0.7 and p_ == 0.0 for k_, p_, _ in verdict[2.0][2]),
          "α = 2 passes everywhere below the cap except where users pay no compute and κ = 0.7 (public mining is unprofitable)")
    # Where the caps take the advantage away.
    for a in (1.5, 2.0, 3.0):
        r_ = agg_profit(a, 30.0, base, net, "H")
        show("DERIVED", f"α = {a}, ratio 30: capped {r_['capped']}, payment ratio {r_['payment']:.2f} (allocation {r_['alloc']:.1f}),"
                        f" budget withheld by the cap", f"{r_['withheld']:.1%}")
    # Concentration (accepted residual risk): reward share against capital share; the operator's initial-capital advantage.
    for a in (1.5, 2.0, 3.0):
        prem = [concentration_premium(power(a), x, net.total, net.n_other) for x in (0.05, 0.1, 0.2, 0.33)]
        show("DERIVED", f"α = {a}: reward share / capital share of one holder at x = 0.05 / 0.1 / 0.2 / 0.33 (before caps)",
             " / ".join(f"{p:.2f}" for p in prem))
    for x0 in (0.02, 0.05, 0.10):
        a = 2.0
        fa = (x0 * net.total) ** a
        fh = net.n_other * ((1 - x0) * net.total / net.n_other) ** a
        share = fa / (fa + fh)
        capped_share = min(share, net.y_cap * x0 * net.total)
        show("DERIVED", f"α = 2: an operator's model with {x0:.0%} of capital (others spread over 20 models): reward share",
             f"{share:.1%} before the cap ({share / x0:.1f}× its capital share), {capped_share:.1%} under the cap")
    for a in (1.5, 2.0):
        top_c, top_r, wh = agg_flows(a, net, base)
        show("DERIVED", f"α = {a}: participant movement (50 units migrate): top model capital / reward share / budget withheld",
             f"{top_c:.0%} / {top_r:.0%} / {wh:.0%}")
    # p = 0, every owner closes: what remains.
    show("DERIVED", "region Z (every owner closes, p = 0): model-leg fraud revenue", "the whole paid model budget, every epoch, for any α "
         "(the curve only moves it toward larger capital); plus closed models' users' escrow")
    show("DERIVED", "region Z: consensus impact", "Final weight on forged claims up to each bond's F_max(C, W): capital-proportional, "
         "not scaled by α (ADR-0177 revision); the curve is not the resolution")


# =====================================================================================================================================


SECTIONS = {1: section1, 2: lambda: (section2(), section2b()), 3: section3, 4: section4, 5: section5, 6: section6, 7: section7, 8: section8, 9: section9, 10: section10, 11: section11, 12: section12}


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
