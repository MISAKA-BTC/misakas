#!/usr/bin/env python3
"""ADR-0160 v2 — every number the ADR states, from the code's constants.

    python3 docs/adr/0160-capacity/capcalc_v2.py > docs/adr/0160-capacity/numbers-v2.md

Pure arithmetic (integers in sompi, exact fractions for the probabilities). Constants carry the
file:line they are read from: the base tree is `rcore/cap-spec2` @ e64348af0 (= rcore/int-4 code,
40b23d64d); a lane constant names its branch. Nothing here is measured; measured inputs are named
as such and carry their source.
"""
from fractions import Fraction as F
import math

SOMPI = 100_000_000


def msk(sompi):
    return sompi / SOMPI


# --------------------------------------------------------------------------------------------
# Code constants
# --------------------------------------------------------------------------------------------
SUBSIDY = 444_562_014_000  # params.rs:17486 PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI (120,000 ms block)
CARVE_PM = 720  # params.rs:17491 PALW_OVERLAY_WORKER_CARVE_PERMILLE_V1
E = SUBSIDY // 1000 * CARVE_PM  # params.rs:17498 PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI
assert E == 320_084_650_080
W_CLASS = {"floor": 10_752_660, "8k": 2_471_600_230, "2M": 5_974_294_206_820}  # capacity map §2.2 (apply_attempt)
FCW = 604_250_611  # rcore/cap-weight palw_weight_cap_v1.rs:71
C_PER_FCW = 650_000_000_000  # :75 (6,500 MSK)
W_FCW = 10_752_660  # :79
C7_CEILING = 138_892_697_241  # :106 (8k raw weight)
KAPPA_PM = 500  # palw_fp_devnet_v3.rs:420 MAX_EXPOSURE_RATIO_PERMILLE
C_MIN = 13_000 * SOMPI  # palw_fp_devnet_v3.rs:902 PALW_MAINNET_MIN_COLLATERAL_SOMPI
C_SEAT = 130_000 * SOMPI  # params.rs:25063 (panel floor = 10 x producer floor)
GENESIS_CARD = 939_063 * SOMPI  # capacity map §0 (eight genesis cards)
S1S2_PM, S3S4_PM, ACTION_MULT = 100, 250, 3  # palw_state_v2.rs:951, :953, :949
Q_SEAT_PM = 250  # rcore/cap-liab palw_aggregate_liability_v1.rs:206
P_STAR = F(1, 2)  # rcore/cap-escrow palw_escrow_funding_v2.rs:57
REPORTER_BPS = 1_000  # palw_state_v2.rs:970
WINDOW_BIND, WINDOW_RECEIPT, WINDOW_CHALLENGE, WINDOW_COURT, ANCHOR_DELAY = 600, 600, 1_200, 3_000, 20  # palw_fp_devnet_v3.rs:165-170
SHORT_CHALLENGE = 120  # palw_state_v2.rs:1429 (Final = L + 120)
CLAIM_RETIREMENT = WINDOW_COURT  # params.rs:1822 (the RC sets claim_retirement_daa = window_court)
BLOCK_MASS = 500_000  # body_validation_in_isolation.rs:157 (max_block_mass, params.rs:11358)
COVERAGE_CARRIER_MASS = 125_768  # capacity map §6.4 (measured)
MERGESET_LIMIT, GHOSTDAG_K = 180, 1  # params.rs:243, :229 (new_two_minute_bps)
STRIKE_EPOCH = 1_000  # palw_state_v2.rs:955

RHOS = [10, 25, 50, 100, 250, 500, 1000]
BONDS = [13_000, 100_000, 1_000_000]

G_MIN = E  # G = g_res + escrowed_reward >= E (palw_state_v2.rs:4546; g_res <= R_budget under F-W)
THREE_G = ACTION_MULT * G_MIN  # 9,602.54 MSK


def tier_s1s2(c0):
    return min(c0 * S1S2_PM // 1000, THREE_G)


def s4(c_seat):
    """A covering signer's S4 beyond its lock: min(25% C0, 3G) (palw_state_v2.rs:3741, DA-7 step 1)."""
    return min(c_seat * S3S4_PM // 1000, THREE_G)


def m_ramp(rho):
    return -(-E // rho)  # ceil(E / rho)


def u_units(c_msk):
    return (c_msk * SOMPI) // C_PER_FCW


def r_budget(c_msk):
    return u_units(c_msk) * W_FCW


def n_bond(c_msk, rho, w_class=W_CLASS["floor"]):
    """Claims a bond holds under the 500 permille ceiling at step rho (m_c = ceil(E/rho) credited;
    the weight term capped at R_budget in total, F-W)."""
    room = c_msk * SOMPI * KAPPA_PM // 1000
    m = m_ramp(rho)
    budget = r_budget(c_msk)
    n = 0
    lo, hi = 0, room // m + 1
    while lo < hi:
        mid = (lo + hi + 1) // 2
        need = mid * m + min(mid * w_class, budget)
        if need <= room:
            lo = mid
        else:
            hi = mid - 1
    return lo


# --------------------------------------------------------------------------------------------
# Pricing by route (decision 2): EV(1) = (1-q)[P*E - (1-P*)(m+w)] - q(m + w + X)
# --------------------------------------------------------------------------------------------
def ev1(m, q, x_of_m):
    return (1 - q) * (P_STAR * E - (1 - P_STAR) * m) - q * (m + x_of_m(m))


def m_star(q, x_of_m):
    """Smallest m in [0, E] with EV(1) <= 0 (w dropped: conservative). EV decreases in m."""
    if ev1(0, q, x_of_m) <= 0:
        return 0
    if ev1(E, q, x_of_m) > 0:
        return E  # no credit can price it below E: the slot stays E
    lo, hi = 0, E
    while lo < hi:
        mid = (lo + hi) // 2
        if ev1(mid, q, x_of_m) <= 0:
            hi = mid
        else:
            lo = mid + 1
    return lo


def q_needed_pm(rho, x_of_m):
    target = m_ramp(rho)
    for q in range(0, 1001):
        if m_star(F(q, 1000), x_of_m) <= target:
            return q
    return 1000


ROUTES = {
    # strategy: (label, X(m) producer-only, X(m) with the v2 rules)
    "naive": (
        "naive (no material) -> DA default S1 (tier class, decision 1)",
        lambda m: 0,  # producer: the forfeit only; the S1 tier lands on the 3rd strike (X11), not per conviction
        lambda m: s4(C_SEAT),  # + DA-7's covering signer, automatic (palw_state_v2.rs:16016), k_min = 1 seat
    ),
    "garbage": (
        "garbage (step arithmetic / logits) -> kind 4, contradiction 5/12 (tier)",
        lambda m: tier_s1s2(C_MIN),  # S2 action at the 13k floor bond: 1,300 MSK (decision 2's ~1,620 at rho 10)
        lambda m: tier_s1s2(C_MIN) + s4(C_SEAT),  # + AG-6: the refutation's covering signer, automatic
    ),
    "borrowed": (
        "borrowed (identity / output / prompt) -> kind 4, contradiction 9/10/11/13 (intent)",
        lambda m: C_MIN - m,  # whole bond: L = C_min in total
        lambda m: C_MIN - m + s4(C_SEAT),
    ),
}

# v2's normative pricing: every strategy that licenses needs >= 1 dishonest Valid (k_min = 1, the S2
# full seat), and its conviction charges that covering signer S4 automatically (DA-7 on a default,
# AG-6 on a refutation). The producer's own tier and forfeiture are margin, never priced: a campaign
# can exhaust the producer's free half, while AG-7 makes each colluding seat's first detection void
# every licence it signed, so the seat's S4 is collected once per campaign-through-that-seat.
X_ROBUST = lambda m: s4(C_SEAT)


def fmt_msk(sompi, nd=2):
    return f"{msk(sompi):,.{nd}f}"


def section(title):
    print()
    print(f"## {title}")
    print()


def main():
    print("# ADR-0160 v2 — the numbers (generated by `capcalc_v2.py`; do not edit by hand)")
    print()
    print(f"* E = {E:,} sompi = {fmt_msk(E, 4)} MSK; 3G >= 3E = {fmt_msk(THREE_G)} MSK; "
          f"tier S1/S2 at 13k = {fmt_msk(tier_s1s2(C_MIN))} MSK; seat S4 at 130k = {fmt_msk(s4(C_SEAT))} MSK")
    print(f"* m_c = ceil(E/rho): " + ", ".join(f"rho {r}: {fmt_msk(m_ramp(r))}" for r in RHOS))

    # ---- 1. per-bond concurrency ----
    section("T1 — claims a bond holds (collateral bound N_bond), floor/8k, m_c = ceil(E/rho)")
    print("| rho | m_c (MSK) | " + " | ".join(f"{b:,} MSK" for b in BONDS) + " | x today at 13k |")
    print("|---|---|" + "---|" * len(BONDS) + "---|")
    today = 2
    print("| today (w + E) | 3,200.95 | 2 | 15 | 156 | 1 |")
    for r in RHOS:
        ns = [n_bond(b, r) for b in BONDS]
        print(f"| {r} | {fmt_msk(m_ramp(r))} | " + " | ".join(f"{n:,}" for n in ns) + f" | x{ns[0] / today:,.0f} |")
    print()
    print("W_cap (FCW, class-neutral, D-1/D-2): " + ", ".join(f"{b:,} MSK -> {u_units(b)}" for b in BONDS)
          + f", genesis card -> {GENESIS_CARD // C_PER_FCW}; R_budget: "
          + ", ".join(f"{b:,} -> {fmt_msk(r_budget(b), 4)} MSK" for b in BONDS))

    # ---- 2. q needed per route ----
    section("T2 — q_needed (credited, permille) for m*(q) <= ceil(E/rho), per strategy and route; P* = 1/2")
    print("| strategy / route | collected X beyond the forfeit | " + " | ".join(f"rho {r}" for r in RHOS) + " |")
    print("|---|---|" + "---|" * len(RHOS))
    for key, (label, x_prod, x_v2) in ROUTES.items():
        for tag, xf in (("today (producer side only)", x_prod), ("v2 (+ automatic covering signer)", x_v2)):
            qs = [q_needed_pm(r, xf) for r in RHOS]
            xs = fmt_msk(xf(0)) if key != "borrowed" else f"C_min - m (+ {fmt_msk(s4(C_SEAT))})" if "v2" in tag else "C_min - m"
            print(f"| {key}: {tag} | {xs} | " + " | ".join(str(q) for q in qs) + " |")
    qs = [q_needed_pm(r, X_ROBUST) for r in RHOS]
    print(f"| **v2 normative, every strategy: X = k_min x S4 (k_min = 1)** | {fmt_msk(s4(C_SEAT))} | " + " | ".join(f"**{q}**" for q in qs) + " |")
    print()
    print("Gate (D-8 half rule, decision 2): measured q >= 2 x max(q_seat = 250, q_needed). A q_needed above 500 is "
          "unreachable under the half rule (measured > 1000). Under the normative row q_needed <= 143 < q_seat, so the "
          "credit is all-or-nothing at q_credit >= 250 (measured >= 500) and then m_c = ceil(E/rho) at every rho.")

    # the user's decision-2 literal check (garbage, producer only, L = w + m_c + 1,300)
    section("T3 — decision 2's literal: kind 12 / StepArithmetic at 13k, L = w + m_c + min(10% C0, 3G)")
    print("| rho | m_c | L collected (MSK) | q_needed (permille) | measured needed (x2) |")
    print("|---|---|---|---|---|")
    for r in RHOS:
        m = m_ramp(r)
        q = q_needed_pm(r, ROUTES["garbage"][1])
        print(f"| {r} | {fmt_msk(m)} | {fmt_msk(W_CLASS['floor'] + m + tier_s1s2(C_MIN))} | {q} | {min(2 * q, 9999)}{' (unreachable)' if 2 * q > 1000 else ''} |")

    # ---- 3. campaign EV under the Bernoulli reduction ----
    section("T4 — per-seat campaign EV(K) = (1-q)^K K x - (1-(1-q)^K) L1 at the credited step (AG-6 + AG-7), MSK")
    print("x = P*E - (1-P*)m (a fraud's net if never detected); L1 = m + S4 (what the first detection collects "
          "automatically); AG-7 voids every other licence the convicted seat signed, so a campaign through one seat "
          "earns only if none of its K claims is detected before maturity.")
    print()
    print("| pricing | rho | q_credit | m_c | EV(1) | EV(10) | EV(100) | EV(1,000) | EV(2,030) |")
    print("|---|---|---|---|---|---|---|---|---|")
    for key, xf in (("normative (X = S4)", X_ROBUST),):
        for r in (10, 100, 1000):
            q = F(max(Q_SEAT_PM, q_needed_pm(r, xf)), 1000)
            m = max(m_ramp(r), m_star(q, xf))
            x = P_STAR * E - (1 - P_STAR) * m
            l1 = m + xf(m)
            evs = []
            for k in (1, 10, 100, 1000, 2030):
                a = (1 - q) ** k
                evs.append(float(a * k * x - (1 - a) * l1) / SOMPI)
            print(f"| {key} | {r} | {int(q * 1000)} | {fmt_msk(m)} | " + " | ".join(f"{v:,.0f}" for v in evs) + " |")

    section("T4b — the same campaign WITHOUT AG-7 (one colluding 130k seat, a 13k producer, pools exhaust), MSK")
    print("Each licensed fraud (P* = 1/2) is detected independently with q; a detection collects S4 while the seat's "
          "collateral lasts (13 charges) and the producer's tier while its free half lasts (5 x 1,300); an undetected "
          "licensed fraud pays E at maturity.")
    print()
    print("| rho | q | K = 10 | K = 100 | K = 1,000 | K = 2,030 |")
    print("|---|---|---|---|---|---|")
    seat_charges = C_SEAT // s4(C_SEAT)
    prod_tiers = (C_MIN * KAPPA_PM // 1000) // tier_s1s2(C_MIN)
    for r in (10, 100, 1000):
        m = m_ramp(r)
        q = F(1, 4)
        row = []
        for k in (10, 100, 1000, 2030):
            lic = F(k) * P_STAR
            det = lic * q
            gain = lic * (1 - q) * E
            lost = min(det, seat_charges) * s4(C_SEAT) + min(det, prod_tiers) * tier_s1s2(C_MIN) + F(k) * m
            row.append(float(gain - lost) / SOMPI)
        print(f"| {r} | 0.25 | " + " | ".join(f"{v:+,.0f}" for v in row) + " |")

    # ---- 4. per-bond DoS caps (lane S) ----
    section("T5 — lane S caps per bond (u = floor(C/6,500 MSK)): N_out = u*rho, burst B = max(4, ceil(u*rho/25)), rate r = u*rho/20 per DAA")
    print("| rho | " + " | ".join(f"{b:,}: N_bond / N_out / B / r / per 80 DAA" for b in BONDS) + " |")
    print("|---|" + "---|" * len(BONDS))
    H_L = 21
    for r in RHOS:
        cells = []
        for b in BONDS:
            u = u_units(b)
            nb = n_bond(b, r)
            nout = u * r
            burst = max(4, -(-u * r // 25))
            rate = F(u * r, 20)
            per80 = min(80 * rate, F(80 * min(nb, nout), H_L))
            cells.append(f"{nb:,} / {nout:,} / {burst:,} / {float(rate):,.1f} / {float(per80):,.0f}")
        print(f"| {r} | " + " | ".join(cells) + " |")
    print()
    print("today's per 80 DAA at 13k: 7.9 (harness, ideal seats; capacity map §4.1). H_L = 21 DAA (licence latency, floor).")

    section("T8 — the licensed-unmatured pile one bond builds before its first reward matures (H_mat = 3,142 DAA)")
    print("| rho | 13k: r/DAA | pile (claims) | pile withheld E (MSK, frozen + burnable) | x its collateral |")
    print("|---|---|---|---|---|")
    H_MAT = 142 + WINDOW_COURT
    for r in RHOS:
        rate = F(u_units(13_000) * r, 20)
        pile = rate * H_MAT
        print(f"| {r} | {float(rate):.1f} | {float(pile):,.0f} | {float(pile * E) / SOMPI:,.0f} | {float(pile * E / (13_000 * SOMPI)):,.0f} |")

    # ---- 5. network limiters ----
    section("T6 — network limiters (floor claims per DAA)")
    duty, lock, h_duty, h_lock = F(64_017, 100), F(24_010, 100), 122, 3_000  # capacity map §6.2 (measured prices)
    usable = 8 * GENESIS_CARD * KAPPA_PM // 1000 / SOMPI  # 3,756,252 MSK
    usable_v02 = 8 * 887_063  # capacity map §6.2 (V02(a): C - 52,000 accuser reserve)
    per_claim = 5 * duty * h_duty + 5 * lock * h_lock
    per_claim_life = 5 * duty * h_duty + 5 * lock * 1_000
    base = usable / per_claim
    print(f"* seat capital per floor claim (coverage, five Valid): {float(per_claim):,.0f} MSK*DAA; 8 genesis seats "
          f"usable {usable:,.0f} MSK -> {float(base):.2f}/DAA; V02(a) {usable_v02:,} -> {float(usable_v02 / per_claim):.2f}; "
          f"+ lock life F+1,000 -> {float(usable_v02 / per_claim_life):.2f}")
    print(f"* x rho_seat (duty and lock divided together, only at q_credit >= q_seat): " + ", ".join(
        f"rho {r}: {float(base * r):,.0f} / {float(usable_v02 / per_claim * r):,.0f} / {float(usable_v02 / per_claim_life * r):,.0f}" for r in RHOS))
    for label, bpd in (("live 4.3-4.6 (mid 4.45)", F(445, 100)), ("harness 12", F(12))):
        today_c = BLOCK_MASS // COVERAGE_CARRIER_MASS
        print(f"* carriage at {label} blocks/DAA: today {today_c}/block -> {float(today_c * bpd):.1f}/DAA; "
              f"F-B paths 59/block (16 seats) -> {float(59 * bpd):.0f}; 64/block (8 seats) -> {float(64 * bpd):.0f}; "
              f"F-B lists ~700/block -> {float(700 * bpd):,.0f}")
    print("* attempt lane: 1 claim per attempt block; live 2.7/DAA (demand), harness 11/DAA (capacity map §5.4); "
          f"DAG ceiling per chain block: mergeset_size_limit {MERGESET_LIMIT} (ghostdag_k {GHOSTDAG_K})")
    life = 142 + CLAIM_RETIREMENT
    print(f"* live claim records ~= rate x {life} DAA (accept -> Final 142 -> retirement +{CLAIM_RETIREMENT}); state bytes at "
          "~2 KiB a claim (claim + panel + duty/lock/liability rows; ESTIMATE), full re-hash per block at ~1 GB/s (ESTIMATE):")
    print("| claims/DAA | live claims | state (MB) | state_root re-hash / merged-attempt clone (ms, each) |")
    print("|---|---|---|---|")
    for rate in (1, 2.7, 10, 100, 1000):
        live = rate * life
        mb = live * 2048 / 1e6
        print(f"| {rate:,} | {live:,.0f} | {mb:,.0f} | {mb:,.0f} |")
    print()
    print("| claims/DAA | emission a DAA at one subsidy a claim-bearing block (MSK) | E escrowed a DAA (MSK) | per year (262,980 DAA) |")
    print("|---|---|---|---|")
    for rate in (1, 2.7, 10, 100, 1000):
        print(f"| {rate:,} | {msk(SUBSIDY) * rate:,.0f} | {msk(E) * rate:,.0f} | {msk(SUBSIDY) * rate * 262_980:,.3g} |")
    vccu_8k = 1_390_562_722_816
    for speed_label, speed in (("engine measured 1.24e10 MAC-eq/s (3,100 permille)", 1.24e10), ("consensus 2,000 permille (8e9)", 8e9)):
        print(f"* 8k replay seats (k = 2 attestations, 120 s a DAA, {speed_label}): " + ", ".join(
            f"{c}/DAA -> {c * 2 * vccu_8k / (120 * speed):,.0f}" for c in (2.2, 22, 100, 1000)))

    # ---- 6. max uncovered loss ----
    section("T7 — max uncovered loss (design bar) at N concurrent claims on one bond, floor/8k")
    print("| bond | rho | N | pre-licence commitment N(m_c+w) (MSK) | withheld rewards N*E, frozen + burnable (MSK) | recoverable before maturity (MSK) | extractable before maturity | uncovered |")
    print("|---|---|---|---|---|---|---|---|")
    for b in BONDS:
        for r in (100, 1000):
            nb = n_bond(b, r)
            for n in sorted({min(1000, nb), nb}):
                commit = n * m_ramp(r) + min(n * W_CLASS["floor"], r_budget(b))
                withheld = n * E
                recov = b * SOMPI + withheld
                print(f"| {b:,} | {r} | {n:,} | {fmt_msk(commit, 0)} | {fmt_msk(withheld, 0)} | {fmt_msk(recov, 0)} | 0 | 0 |")
    print()
    print("2M (C7, q = 0): m = E, C7 cap 1 network-wide: <= E = 3,200.85 MSK uncovered for the one live claim (unchanged from v1).")

    # ---- 7. network stages ----
    section("T9 — network stages (floor claims per DAA): every limiter, and the one that binds")
    b_live = F(445, 100)
    seat = lambda rho_s, v02, life: float((F(usable_v02) if v02 else F(usable)) / (per_claim_life if life else per_claim) * rho_s)
    stages = [
        ("N0 today (0e8ec984e)", "-", seat(1, False, False), float(3 * b_live), "2.7 live / 11 harness", 1, "seat capital"),
        ("N1 H_cap, uncredited (rho 10, q < 250)", "F-W F-L F-E F-R F-B F-S F-N F-K; V02 (DAA-500)", seat(1, True, False), float(59 * b_live), ">= 11", 1.8, "seat capital"),
        ("N2 first credited step, rho 10", "+ lane D (AG-6, AG-7); q >= 500 measured, every strategy", seat(10, True, False), float(59 * b_live), "measure >= 18", 10, "seat capital, then the attempt lane (unmeasured above 11)"),
        ("N3 rho 25", "+ M-0a (journal fold)", seat(25, True, False), float(59 * b_live), "measure >= 44", 25, "the attempt lane / the fold's clone per merged attempt"),
        ("N4 rho 100", "+ F-B full lists, lock life F+1,000, M-0b (state commitment)", seat(100, True, True), float(700 * b_live), "measure >= 100", 100, "the state commitment (M-0b), then the attempt lane"),
        ("N5 rho 1000 (floor only)", "+ M-1 riders or >= 1,000 attempt blocks a DAA; the emission decision (D-19)", seat(1000, True, True), float(700 * b_live), "1,024 (M-1: 16 blocks x 64)", 1000, "carriage 3,115 and seats 4,460 clear; emission and state bind"),
    ]
    print("| stage | enablers armed | seat capital | licence carriage (4.45 blocks/DAA) | attempt lane | target claims/DAA | live claims / state at target | binds |")
    print("|---|---|---|---|---|---|---|---|")
    for name, en, s_cap, carry, att, target, binds in stages:
        live = target * life
        print(f"| {name} | {en} | {s_cap:,.1f} | {carry:,.0f} | {att} | {target:,} | {live:,.0f} / {live * 2048 / 1e6:,.0f} MB | {binds} |")

    # ---- 8. riders (M-1) ----
    section("T10 — M-1 riders: one subsidy a block, split over 1 + n claims (emission a block unchanged)")
    print("| riders n | E_i = E/(1+n) (MSK) | 13k N_bond at rho 100 | at rho 1000 | claims a DAA at 4.45 blocks | emission a DAA (MSK) |")
    print("|---|---|---|---|---|---|")
    for n in (0, 9, 63):
        ei = E // (1 + n)
        n100 = (13_000 * SOMPI * KAPPA_PM // 1000) // (-(-ei // 100))
        n1000 = (13_000 * SOMPI * KAPPA_PM // 1000) // (-(-ei // 1000))
        print(f"| {n} | {fmt_msk(ei)} | {n100:,} | {n1000:,} | {float(b_live * (1 + n)):,.0f} | {float(b_live) * msk(SUBSIDY):,.0f} |")

    # ---- 9. breaker trip cost ----
    section("T11 — what one deliberate trip of the network breaker costs (lane B), MSK")
    s3 = min(C_MIN * S3S4_PM // 1000, THREE_G)
    per_escape = s4(C_SEAT) + s3 - E  # the colluding seat's S4 + the producer's S3, less the escaped E
    print(f"* K1 escaped-and-convicted event: seat S4 {fmt_msk(s4(C_SEAT))} + producer S3 {fmt_msk(s3)} - the escaped E {fmt_msk(E)} = "
          f"{fmt_msk(per_escape)} a planted event; at q_measured 0.5 an escape needs ~2 attempts: ~{fmt_msk(2 * per_escape)}; "
          f"the K1 floor of 3 events: ~{fmt_msk(6 * per_escape)} a trip")
    print(f"* K2 short collection: a bond convicted past its collateral while its colluding seat's S4 is still collected "
          f"(the seat side is never short before {C_SEAT // s4(C_SEAT)} charges): >= one 13k bond forfeited a trip, and a colluding seat")


if __name__ == "__main__":
    main()
