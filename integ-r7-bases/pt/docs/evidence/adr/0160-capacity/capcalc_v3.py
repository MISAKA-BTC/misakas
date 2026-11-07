#!/usr/bin/env python3
"""ADR-0160 v3 — every number the ADR states, from the code's constants.

    python3 docs/adr/0160-capacity/capcalc_v3.py > docs/adr/0160-capacity/numbers-v3.md

Pure arithmetic (integers in sompi, exact fractions for probabilities). v3 answers the second review of
v2 (93395bafa): the EV of a fraud charges the seat's S4 on the LICENSED branch only (review finding 7);
the credit rests on the audit door (every credited claim replayed by a non-panel operator auditor before
Final), not on a sampled q; the seat-capital rows use the lock life the DAA-500 release arms (finding 2);
lane N's share is work-conserving (finding 5); the breaker's forced-trip costs are what the code collects
(finding 6); T9 carries the anchor and operator-only carriage columns (finding 10).

Constants carry the file:line they are read from (base: rcore/cap-spec2 @ e64348af0 = rcore/int-4 code
40b23d64d unless a branch is named). Measured inputs are named as such and carry their source.
"""
from fractions import Fraction as F
import math

SOMPI = 100_000_000


def msk(sompi):
    return sompi / SOMPI


def fmt(sompi, nd=2):
    return f"{msk(sompi):,.{nd}f}"


# --------------------------------------------------------------------------------------------
# Code constants (unchanged from v2; the reviewer reproduced each)
# --------------------------------------------------------------------------------------------
SUBSIDY = 444_562_014_000  # params.rs:17486 (120,000 ms block)
CARVE_PM = 720  # params.rs:17491
E = SUBSIDY // 1000 * CARVE_PM  # params.rs:17498
assert E == 320_084_650_080
W_FLOOR = 10_752_660  # capacity map §2.2
FCW = 604_250_611  # rcore/cap-weight palw_weight_cap_v1.rs:71
C_PER_FCW = 650_000_000_000  # :75 (6,500 MSK)
W_FCW = 10_752_660  # :79
KAPPA_PM = 500  # palw_fp_devnet_v3.rs:420
C_MIN = 13_000 * SOMPI  # palw_fp_devnet_v3.rs:902
C_SEAT = 130_000 * SOMPI  # params.rs:25063
GENESIS_CARD_MSK = 939_063  # capacity map §0
S1S2_PM, S3S4_PM, ACTION_MULT = 100, 250, 3  # palw_state_v2.rs:949-953
P_STAR = F(1, 2)  # rcore/cap-escrow palw_escrow_funding_v2.rs:73 (the licence probability of one fraud)
WINDOW_COURT = 3_000  # palw_fp_devnet_v3.rs:165-170
LOCK_LIFE = 1_000  # palw_state_v2.rs PALW_FINAL_LOCK_LIFE_DAA_V1 (rcore/int-4, the DAA-500 release)
BLOCK_MASS = 500_000  # body_validation_in_isolation.rs:157
COVERAGE_CARRIER_MASS = 125_768  # capacity map §6.4 (measured)
H_L = 21  # licence latency, floor (capacity map §4, measured 21-23)
H_MAT = 142 + WINDOW_COURT  # accept -> Final (~142) -> maturity (+3,000)

RHOS = [10, 25, 50, 100, 250, 500, 1000]
BONDS = [13_000, 100_000, 1_000_000]

G_MIN = E  # G = g_res + escrowed_reward >= E (palw_state_v2.rs:4546)
THREE_G = ACTION_MULT * G_MIN


def tier_s1s2(c0):
    return min(c0 * S1S2_PM // 1000, THREE_G)


def s4(c_seat):
    """A covering signer's tier beyond its lock: min(25% C0, 3G) (DA-7 step 1; S-4)."""
    return min(c_seat * S3S4_PM // 1000, THREE_G)


def m_ramp(rho):
    return -(-E // rho)


def u_units(c_msk):
    return (c_msk * SOMPI) // C_PER_FCW


def r_budget(c_msk):
    return u_units(c_msk) * W_FCW


def n_bond(c_msk, rho, w_class=W_FLOOR):
    room = c_msk * SOMPI * KAPPA_PM // 1000
    m = m_ramp(rho)
    budget = r_budget(c_msk)
    lo, hi = 0, room // m + 1
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if mid * m + min(mid * w_class, budget) <= room:
            lo = mid
        else:
            hi = mid - 1
    return lo


S4_SEAT = s4(C_SEAT)  # 9,602.54 MSK
TIER_13K = tier_s1s2(C_MIN)  # 1,300 MSK
SEAT_CHARGES = C_SEAT // S4_SEAT  # 13 full S4 charges fit a 130k seat
PROD_FREE_TIERS = (C_MIN * KAPPA_PM // 1000) // TIER_13K  # 5 tiers fit a 13k bond's free half


# --------------------------------------------------------------------------------------------
# Finding 7: the corrected expectation. A fraud licenses with P*; only a LICENSED fraud has a covering
# signer to charge. On the licensed branch the stage commitment was released at the counted licence
# (SR-1), so a detection costs X (beyond the reward it never gets); on the unlicensed branch the claim
# voids and its stage commitment m + w is charged (E-4/E-5 or the conviction).
#     EV(1) = P*[(1-q)E - qX] - (1-P*)(m + w)            (w dropped below: conservative)
#     q_needed = [P*E - (1-P*)m] / [P*(E + X)]  ->  (E - m)/(E + X) at P* = 1/2
# v2 charged X on both branches: EV_v2(1) = (1-q)[P*E - (1-P*)m] - q(m + X); it overstated the
# deterrent by q(1-P*)X ~ 1,200 MSK at q = 250 permille.
# --------------------------------------------------------------------------------------------
def ev1_v3(m, q, x):
    return P_STAR * ((1 - q) * E - q * x) - (1 - P_STAR) * m


def ev1_v2(m, q, x):
    return (1 - q) * (P_STAR * E - (1 - P_STAR) * m) - q * (m + x)


def q_needed_pm(rho, x, ev=ev1_v3):
    m = m_ramp(rho)
    for q in range(0, 1001):
        if ev(m, F(q, 1000), x) <= 0:
            return q
    return 1000


def section(title):
    print()
    print(f"## {title}")
    print()


def main():
    print("# ADR-0160 v3 — the numbers (generated by `capcalc_v3.py`; do not edit by hand)")
    print()
    print(f"* E = {E:,} sompi = {fmt(E, 4)} MSK; seat S4 at 130k = {fmt(S4_SEAT)} MSK ({SEAT_CHARGES} fit a 130k seat); "
          f"tier S1/S2 at 13k = {fmt(TIER_13K)} MSK ({PROD_FREE_TIERS} fit the free half)")
    print("* m_c = ceil(E/rho): " + ", ".join(f"rho {r}: {fmt(m_ramp(r))}" for r in RHOS))

    # ---- T1 ----
    section("T1 — claims a bond holds under the 500 permille ceiling (N_bond), floor/8k, m_c = ceil(E/rho)")
    print("| rho | m_c (MSK) | " + " | ".join(f"{b:,} MSK" for b in BONDS) + " | x today at 13k |")
    print("|---|---|" + "---|" * len(BONDS) + "---|")
    print("| today (w + E) | 3,200.95 | 2 | 15 | 156 | 1 |")
    for r in RHOS:
        ns = [n_bond(b, r) for b in BONDS]
        print(f"| {r} | {fmt(m_ramp(r))} | " + " | ".join(f"{n:,}" for n in ns) + f" | x{ns[0] / 2:,.0f} |")
    print()
    print("W_cap (FCW): " + ", ".join(f"{b:,} MSK -> {u_units(b)}" for b in BONDS) + f", genesis card -> {GENESIS_CARD_MSK * SOMPI // C_PER_FCW}")

    # ---- T2 ----
    section("T2 — q_needed (permille) for EV(1) <= 0 at m_c = ceil(E/rho), P* = 1/2, corrected (X on the licensed branch only)")
    routes = [
        ("seat charge X = S4 (DA-7 on a default, AG-6 on a refutation)", S4_SEAT),
        ("garbage, producer side only (X = S2 tier 1,300 at 13k) — decision 2's literal", TIER_13K),
        ("naive, producer side only (X = 0: a first S1 adds no tier, X11)", 0),
        ("borrowed, producer side only (X = the whole 13k bond, AG-2)", C_MIN),
    ]
    print("| route | X (MSK) | " + " | ".join(f"rho {r}" for r in RHOS) + " |")
    print("|---|---|" + "---|" * len(RHOS))
    for label, x in routes:
        print(f"| {label} | {fmt(x)} | " + " | ".join(str(q_needed_pm(r, x)) for r in RHOS) + " |")
    print(f"| v2's formula, X = S4 (for comparison: X charged on both branches) | {fmt(S4_SEAT)} | "
          + " | ".join(str(q_needed_pm(r, S4_SEAT, ev1_v2)) for r in RHOS) + " |")
    print()
    print("With X = S4 the break-even tends to E/(E + 3E) = 250 permille — exactly q_seat, so v2's 'q_needed <= 143 < q_seat' "
          "reasoning was wrong and the half rule was its only margin (finding 7). Under the audit door (v3) the refusal "
          "probability of a licensed fraud is the audit's detection rate d, 1000 permille by construction for the three "
          "strategies: every row above is met, decision 2's literal row included.")
    print()
    print("EV(1) at the v2 gate q = 250 permille (corrected formula), MSK: " + ", ".join(
        f"rho {r}: {float(ev1_v3(m_ramp(r), F(1, 4), S4_SEAT)) / SOMPI:,.2f}" for r in (10, 100, 1000))
          + "  (v2 printed -1,400 / -1,220 / -1,202)")

    # ---- T3 ----
    section("T3 — decision 2's literal: kind 12 / StepArithmetic at 13k, collected L = w + m_c + min(10% C0, 3G), corrected formula")
    print("| rho | m_c | L collected (MSK) | q_needed (permille) | audit door d | met |")
    print("|---|---|---|---|---|---|")
    for r in RHOS:
        m = m_ramp(r)
        q = q_needed_pm(r, TIER_13K)
        print(f"| {r} | {fmt(m)} | {fmt(W_FLOOR + m + TIER_13K)} | {q} | 1000 | yes |")

    # ---- T4 ----
    section("T4 — the audit door: EV of one 13k producer's full campaign through ONE colluding 130k covering seat, MSK")
    print("K = the bond's outstanding cap N_out(rho) issued before its first conviction freezes it (AG-3). Each attempt licenses "
          "with P* = 1/2 (the colluding seat); each licensed fraud is refused by its audit unless the auditor passes it "
          "(probability beta: a compromised or careless auditor host — beta = 0 under the operator stopgap). A refused "
          "licensed fraud is convicted: the seat's S4 while its collateral lasts (13 charges) and the producer's tier while its "
          "free half lasts (5); an unlicensed attempt loses m_c + w; a passed fraud gains E at maturity.")
    print()
    print("With k_aud = 2 (two distinct pool members must each sign a receipt; v3 requires it from rho 250) a fraud passes "
          "only if both pass it: beta_eff = beta^2 for independent hosts. 'x10 producers' is ten Sybil 13k producers (K = 10 N_out) "
          "sharing the one colluding seat's 13 charges.")
    print()
    print("| rho | producers | k_aud | K | beta = 0 | beta = 10 permille | beta = 40 permille | beta = 58 permille | break-even beta |")
    print("|---|---|---|---|---|---|---|---|---|")

    def campaign(k, m, beta_eff, producers):
        lic = F(k) * P_STAR
        passed = lic * beta_eff
        refused = lic - passed
        gain = passed * E
        lost = (min(refused, SEAT_CHARGES) * S4_SEAT + min(refused, producers * PROD_FREE_TIERS) * TIER_13K
                + (F(k) - lic) * (m + W_FLOOR))
        return gain - lost

    for r, producers, k_aud in ((10, 1, 1), (100, 1, 1), (250, 1, 1), (250, 1, 2), (1000, 1, 1), (1000, 1, 2), (1000, 10, 1), (1000, 10, 2)):
        m = m_ramp(r)
        k = u_units(13_000) * r * producers
        row = [float(campaign(k, m, beta ** k_aud, producers)) / SOMPI for beta in (F(0), F(10, 1000), F(40, 1000), F(58, 1000))]
        be = next((bpm for bpm in range(0, 1001) if campaign(k, m, F(bpm, 1000) ** k_aud, producers) > 0), None)
        print(f"| {r} | {producers} | {k_aud} | {k:,} | " + " | ".join(f"{v:+,.0f}" for v in row) + f" | {be} permille |")
    print()
    print("beta = 0 (every licensed fraud refused): no fraud is ever paid; EV = -(what the refusals collect) - K(1-P*)(m + w).")

    section("T4b — why v3 does not price a SAMPLED audit: v2's campaign without a campaign reduction, corrected formula, MSK")
    print("One colluding 130k seat covers K frauds of a 13k producer; each licensed fraud (P* = 1/2) is detected independently "
          "with q; a detection collects S4 while the seat's collateral lasts (13) and the tier while the producer's free half "
          "lasts (5); an undetected licensed fraud is paid E. q = 657 permille is t12's three non-seat operators at a = 300.")
    print()
    print("| rho | q | K = 10 | K = 100 | K = 1,000 | K = 2,030 |")
    print("|---|---|---|---|---|---|")
    for r in (10, 100, 1000):
        m = m_ramp(r)
        for q in (F(250, 1000), F(657, 1000)):
            row = []
            for k in (10, 100, 1000, 2030):
                lic = F(k) * P_STAR
                det = lic * q
                gain = lic * (1 - q) * E
                lost = min(det, SEAT_CHARGES) * S4_SEAT + min(det, PROD_FREE_TIERS) * TIER_13K + (F(k) - lic) * m
                row.append(float(gain - lost) / SOMPI)
            print(f"| {r} | {int(q * 1000)} | " + " | ".join(f"{v:+,.0f}" for v in row) + " |")

    # ---- T5 ----
    section("T5 — lane S caps per bond (u = floor(C/6,500 MSK)): N_out = u*rho, burst B = max(4, ceil(u*rho/25)), rate r = u*rho/20 per DAA")
    print("| rho | " + " | ".join(f"{b:,}: N_bond / N_out / B / r / per 80 DAA" for b in BONDS) + " |")
    print("|---|" + "---|" * len(BONDS))
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
    print("These are per-bond CEILINGS. A bond reaches them only while lane N's work-conserving share leaves it the room (T12).")

    # ---- T6 ----
    section("T6 — network limiters (floor claims per DAA), with the lock life the DAA-500 release arms")
    duty, lock, h_duty = F(64_017, 100), F(24_010, 100), 122  # capacity map §6.2 (measured prices)
    usable_shipped = F(8 * GENESIS_CARD_MSK * KAPPA_PM, 1000)  # 3,756,252 MSK
    usable_v02 = F(8 * 887_063)  # V02(a): C - 52,000 accuser reserve (capacity map §6.2)
    per_claim_3000 = 5 * duty * h_duty + 5 * lock * WINDOW_COURT
    per_claim_life = 5 * duty * h_duty + 5 * lock * LOCK_LIFE
    seat_shipped = usable_shipped / per_claim_3000
    seat_v02 = usable_v02 / per_claim_3000
    seat_life = usable_v02 / per_claim_life
    print(f"* seat capital per floor claim: {float(per_claim_3000):,.0f} MSK*DAA with F + 3,000 locks, {float(per_claim_life):,.0f} with "
          f"the lock life F + 1,000 (palw_final_lock_life, in the DAA-500 release beside V02(a))")
    print(f"* eight genesis seats: shipped {float(seat_shipped):.2f}/DAA; V02(a) alone {float(seat_v02):.2f}; "
          f"**V02(a) + lock life (what public t12 runs past DAA 500) {float(seat_life):.2f}**")
    print("* credited (AS-1': duty and lock divided by rho_s; AS-5 keeps a credited claim's pre-Final lock at L + window_court, "
          "re-dated to F + 1,000 at Final, so an honest claim's lock life is unchanged): "
          + ", ".join(f"rho {r}: {float(seat_life * r):,.1f}" for r in RHOS))
    A_OP = F(26, 10)  # operator attempts per DAA, live (capacity map §6.5)
    B_LIVE = F(445, 100)
    print(f"* carriage per block: none 3 (125,768 mass each), F-B paths 59 (16 seats) / 64 (8 seats), F-B lists ~700")
    for label, bpd in (("all chain blocks, live 4.45/DAA", B_LIVE), ("operator attempts only, live 2.6/DAA (lane N's reading, D-22)", A_OP)):
        print(f"  * {label}: none {float(3 * bpd):.1f}; paths {float(59 * bpd):.0f}; lists {float(700 * bpd):,.0f}")
    for b_bind in (100, 385, 770):
        print(f"* anchors: B_bind {b_bind} binds per operator anchor block x {float(A_OP)} operator attempts a DAA = {float(b_bind * A_OP):,.0f} binds/DAA")
    print("* the audit door (lane Q): one operator replay per credited claim, by one of the claim's non-panel operator auditors "
          "(3 of 8 on t12); per operator host ~ credited rate / 8 a DAA (T13); floor replays are not compute-bound today "
          "(the floor has no class room), 8k audits use operator GPUs: 8k replay seats x 1.5 (k = 2 panel + 1 audit)")
    vccu_8k = 1_390_562_722_816
    for speed_label, speed in (("engine 1.24e10 MAC-eq/s", 1.24e10), ("consensus 2,000 permille (8e9)", 8e9)):
        print(f"* 8k replay seats ({speed_label}), panel k = 2 / with the audit k = 3: " + ", ".join(
            f"{c}/DAA -> {c * 2 * vccu_8k / (120 * speed):,.0f} / {c * 3 * vccu_8k / (120 * speed):,.0f}" for c in (2.2, 22, 100, 1000)))
    life = 142 + WINDOW_COURT
    print(f"* live claim records ~ rate x {life} DAA; state at ~2 KiB a claim (ESTIMATE); emission one subsidy per claim-bearing block:")
    print("| claims/DAA | live claims | state (MB) | emission a DAA (MSK) | per year (262,980 DAA) |")
    print("|---|---|---|---|---|")
    for rate in (1, 2.7, 10, 100, 1000):
        live = rate * life
        print(f"| {rate:,} | {live:,.0f} | {live * 2048 / 1e6:,.0f} | {msk(SUBSIDY) * rate:,.0f} | {msk(SUBSIDY) * rate * 262_980:,.3g} |")

    # ---- T7 ----
    section("T7 — max uncovered loss (design bar), one bond, N concurrent credited floor claims")
    print("Columns: the pre-licence commitment; the withheld rewards (frozen + burnable + no escape to maturity); what is "
          "extractable before the claim's audit (structural); the LITERAL max run-away with an honest audit pool (every "
          "licensed fraud refused before Final: nothing is ever paid); the literal max if the operator audit pool itself is "
          "compromised (the stopgap's failure, as a compromised operator anchor defeats J-1); the expected run-away for an "
          "auditor that passes a fraction beta = 10 permille of licensed frauds (P* = 1/2).")
    print()
    print("| bond | rho | N | commitment N(m_c + w) (MSK) | withheld N*E (MSK) | extractable before the audit | literal max, honest pool | literal max, pool compromised (N*E) | expected at beta = 10 permille |")
    print("|---|---|---|---|---|---|---|---|---|")
    for b in BONDS:
        for r in (100, 1000):
            nb = n_bond(b, r)
            for n in sorted({min(1000, nb), nb}):
                commit = n * m_ramp(r) + min(n * W_FLOOR, r_budget(b))
                withheld = n * E
                exp = F(n) * P_STAR * F(10, 1000) * E
                print(f"| {b:,} | {r} | {n:,} | {fmt(commit, 0)} | {fmt(withheld, 0)} | 0 | **0** | {fmt(withheld, 0)} | {float(exp) / SOMPI:,.0f} |")
    print()
    print("Today's rule for comparison (rho 1, m = E, no audit): 13k holds 2 claims; an undetected licensed fraud is paid E; "
          "literal max 2E = 6,401.69 MSK, which fits the 6,500 MSK ceiling the bond posts — the property a credit loses unless "
          "the audit refuses every fraud.")

    # ---- T8 ----
    section("T8 — the licensed-unmatured pile one bond builds before its first reward matures (H_mat = 3,142 DAA)")
    print("| rho | 13k: r/DAA | pile (claims) | pile withheld E (MSK) | x its collateral |")
    print("|---|---|---|---|---|")
    for r in RHOS:
        rate = F(u_units(13_000) * r, 20)
        pile = rate * H_MAT
        print(f"| {r} | {float(rate):.1f} | {float(pile):,.0f} | {float(pile * E) / SOMPI:,.0f} | {float(pile * E / (13_000 * SOMPI)):,.0f} |")

    # ---- T9 ----
    section("T9 — network stages (floor claims per DAA): every limiter, with the DAA-500 lock life and lane N's operator-only reading")
    stages = [
        ("N0 today (0e8ec984e, shipped rule)", "-", float(seat_shipped), "13 / 8", "2.6 x B_bind", 1, "-", "seat capital"),
        ("N0' past DAA 500 (V02(a) + lock life)", "DAA-500 release", float(seat_life), "13 / 8", "260", 2.7, "-", "demand (2.7)"),
        ("N1 H_cap, uncredited (rho 10 step, no credit)", "F-W F-L F-E F-R F-B F-S F-N F-K F-Q", float(seat_life), "263 / 153", "260", 4, "-", "seat capital"),
        ("N2 first credited step, rho 10", "+ lane D, the audit door armed and passed (G2')", float(seat_life * 10), "263 / 153", "260", 10, "1.25", "(margin 4.5x) seat capital"),
        ("N3 rho 25", "+ M-0a", float(seat_life * 25), "263 / 153", "260", 25, "3.1", "(margin 4.5x) seat capital / anchors"),
        ("N4 rho 100", "+ F-B lists, M-0b", float(seat_life * 100), "3,115 / 1,820", "260 (G7 2x: 200 fits)", 100, "12.5", "anchors (2.6x), then seats"),
        ("N5 rho 1000 (floor only)", "+ M-1 or more attempt blocks; B_bind >= 770 or more operator attempts; D-19", float(seat_life * 1000), "3,115 / 1,820", "needs B_bind >= 385 (1x) / 770 (2x)", 1000, "125", "anchors and operator carriage (1.8x < 2x), emission, state"),
    ]
    print("| stage | enablers armed | seat capital | carriage all blocks / operator-only (lane N) | anchor binds/DAA (B_bind 100) | target | audits per operator host a DAA | binds |")
    print("|---|---|---|---|---|---|---|---|")
    for name, en, s_cap, carry, anch, target, aud, binds in stages:
        print(f"| {name} | {en} | {s_cap:,.1f} | {carry} | {anch} | {target:,} | {aud} | {binds} |")

    # ---- T10 ----
    section("T10 — M-1 riders: one subsidy a block, split over 1 + n claims (emission a block unchanged)")
    print("| riders n | E_i = E/(1+n) (MSK) | 13k N_bond at rho 100 | at rho 1000 | claims a DAA at 4.45 blocks | emission a DAA (MSK) |")
    print("|---|---|---|---|---|---|")
    for n in (0, 9, 63):
        ei = E // (1 + n)
        n100 = (13_000 * SOMPI * KAPPA_PM // 1000) // (-(-ei // 100))
        n1000 = (13_000 * SOMPI * KAPPA_PM // 1000) // (-(-ei // 1000))
        print(f"| {n} | {fmt(ei)} | {n100:,} | {n1000:,} | {float(B_LIVE * (1 + n)):,.0f} | {float(B_LIVE) * msk(SUBSIDY):,.0f} |")

    # ---- T11 ----
    section("T11 — what one deliberate trip of the network breaker costs, MSK (what the code actually collects)")
    print("v2's K1 (escaped value) as the review recomputed it: a conviction after maturity finds no vesting row "
          "(post_final_producer_leg_v1: 'Otherwise nothing: no row') and no live lock, so the K1 event itself costs 0; the cost "
          "is the pre-maturity detections needed to get an escape, (q/(1-q))(S4 + m_c) each, less the escaped E - m_c:")
    for r in (10, 1000):
        m = m_ramp(r)
        q = F(1, 2)
        per = q / (1 - q) * (S4_SEAT + m) - (E - m)
        print(f"* rho {r}, q 0.5: {fmt(int(per))} per escape, three escapes {fmt(int(3 * per))} (v2 claimed 57,910.16)")
    print("* v2's K1 is also near-unreachable: retirement is max(Final + 3,000, the last session's close + 1) and maturity is "
          "Final + 3,000 or later, so a conviction lands after maturity only in a session that outlives it — v3 drops K1.")
    print(f"* v3 K1' (a conviction of a credited claim that carries an audit receipt): needs an operator auditor to sign a false "
          f"receipt — an outsider cannot force it.")
    print(f"* v3 K2 (slash shortfall, >= 3 DISTINCT bonds short, forfeited bonds excluded): >= 3 x {C_MIN // SOMPI:,} = "
          f"{3 * C_MIN // SOMPI:,} MSK of producer collateral drained by the griefer's own tier convictions.")
    print("* v3 K6 (audit service): operator auditors late on > 5% of the epoch's credited licences — forced only by taking "
          "the operator's hosts down, which is the event it exists to report (the claims wait; nothing is paid unaudited).")
    print("* K3 / K4: unattributable residual only; a griefer's own seats above 20% silence enter the non-service list and "
          "stop counting (qualitative, as v2).")

    # ---- T12 ----
    section("T12 — lane N (work-conserving): a 13k bond's room beside the genesis producers, floor")
    print("L_net = H_L x the stage's smallest limiter (T9). Live mix: the producing genesis cards hold ~55 unlicensed claims "
          "and are not refused (not registered). Full contention: all eight cards (72 units each, 576) registered with unmet "
          "demand; the 13k bond (1 unit) is guaranteed max(floor(L_net x 1/577), 1).")
    print()
    print("| stage | rho | L_net (claims) | 13k ceiling min(N_bond, N_out) | room, live mix | per 80 DAA, live mix | room, full contention | per 80 DAA, contention |")
    print("|---|---|---|---|---|---|---|---|")
    for name, rho, lrate in (("N2", 10, float(seat_life * 10)), ("N3", 25, 111.5), ("N4", 100, 260.0), ("N5", 1000, 1000.0)):
        lnet = int(H_L * lrate)
        ceil_ = min(n_bond(13_000, rho), u_units(13_000) * rho)
        live = min(ceil_, lnet - 55)
        cont = max(lnet * 1 // 577, 1)
        rate = F(u_units(13_000) * rho, 20)
        per_live = float(min(80 * rate, F(80 * live, H_L)))
        per_cont = float(min(80 * rate, F(80 * cont, H_L)))
        print(f"| {name} | {rho} | {lnet:,} | {ceil_:,} | {live:,} | {per_live:,.0f} | {cont:,} | {per_cont:,.0f} |")
    print()
    print("v2's rule (share reserved for every holder, newcomers counted only once they hold a claim) gave the 13k bond "
          "1 / 3 / 10 / 97 at N2-N5 while ~97% of L_net sat idle (the review's table).")

    # ---- T13 ----
    section("T13 — the audit door's load on the operator (floor), and its carriage")
    print("| stage | credited claims/DAA | audits/DAA per operator host (8 hosts) | audit entries per chain block (4.45/DAA) | batch mass at ~0.5k per entry + one 25k signature per auditor-batch |")
    print("|---|---|---|---|---|")
    for name, rate in (("N2", 10), ("N3", 25), ("N4", 100), ("N5", 1000)):
        per_block = rate / 4.45
        print(f"| {name} | {rate:,} | {rate / 8:,.2f} | {per_block:,.1f} | ~{per_block * 500 + 25_000:,.0f} of 500,000 (ESTIMATE; lane Q measures) |")


if __name__ == "__main__":
    main()
