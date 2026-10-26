#!/usr/bin/env python3
"""Lane VAL independent recomputation of the G14 value estimates (2026-10-26). Pure arithmetic, stdlib only.
Inputs are the recorded ones (P1 measurements, P3/P4 pin). Exit 1 if any inequality fails."""
from math import comb, ceil

SOMPI = 10**8
ok = True


def check(name, cond, detail):
    global ok
    ok &= bool(cond)
    print(f"{'PASS' if cond else 'FAIL'}  {name}: {detail}")


def tail(n, p, t):
    return sum(comb(n, i) * p**i * (1 - p) ** (n - i) for i in range(t, n + 1))


# --- certification m/k under f = 0.25 (user 2026-10-22); m/k pinned 2026-10-22 by P4
m, k, f, resp, draws_per_year = 75, 41, 0.25, 0.90, 4096 * 365
p_false = tail(m, f, k)
p_honest = tail(m, (1 - f) * resp, k)
check("cert false-certification per draw", p_false < 1e-7, f"P={p_false:.3e}/draw, {p_false*draws_per_year:.3e}/year")
check("cert honest certifies", p_honest >= 0.99, f"P={p_honest:.4f}")

# --- detection 90-99 % (D2 draw cap 64)
for lie in (0.0354, 0.05, 0.10):
    d = 1 - (1 - lie) ** 64
    check(f"D2 detection lie={lie:.2%}", d >= 0.90, f"{d:.4f}")
# finding: at the full 4,096/day draw cap the yearly expectation exceeds 1e-3; the smallest m/k that keeps it below with
# honest certification >= 0.99 (response 90 %) is m=89, k=50.
p89 = tail(89, f, 50)
check("cert alt m=89 k=50 at full cap", p89 * draws_per_year < 1e-3 and tail(89, (1 - f) * resp, 50) >= 0.99,
      f"{p89*draws_per_year:.3e}/year, honest {tail(89, (1-f)*resp, 50):.4f}")
print(f"      min lie fraction detected at 90 %: {1 - 0.1 ** (1/64):.4%}, at 99 %: {1 - 0.01 ** (1/64):.4%}")

# --- V1 service profile
pkt, span, per, signers, budget_pk, block, median_tx = 34_000, 833, 12, 2, 2, 125_000, 56_134
check("V1 intervals end before a+10,000", 1 + per * span <= 10_000, f"{1 + per*span}")
check("V1 packet holds largest object", pkt >= 33_821, f"{pkt}")
check("V1 leaves median tx room", block - budget_pk * pkt >= median_tx, f"{block - budget_pk*pkt}")
lam = budget_pk / (signers * per)
print(f"      sustained Panel=1 originals: {lam:.4f}/DAA = {lam*720:.1f}/nominal day")

# --- V2 funded original
carriage = pkt * 50
fee = 2 * signers * per * carriage
check("V2 fee = 2x reserved carriage", fee == 81_600_000, f"{fee} sompi = {fee/SOMPI} MSK")
check("V2 heap >= TIR retained max", 65_536 >= 59_494, "65,536 B")

# --- V3 escrow
w, r, c, b = 800, 100, 0, 100
check("V3 shares sum 1000", w + r + c + b == 1000, "")
check("V3 self-dealing loses", b > 0, f"self-paid claim loses {b/10}% of F (no subsidy)")

# --- V11 rho, session weight
rho, weight, cap, window, bond = 6_250, 13, 42, 10_000, 13_000 * SOMPI
jam = rho * cap * window * weight
check("V11 session-DAA pays carriage share", rho * weight * cap >= 68_000 * 50, f"{rho*weight*cap} >= {68_000*50}")
check("V11 full-window jam >= 300 MSK", jam >= 300 * SOMPI,
      f"{jam/SOMPI:.0f} MSK (bond-level jam would need rho {ceil(bond/(cap*window*weight))})")
honest_occ = rho * 336 * weight

# --- V4 ingress and honest cost recovery
ingress = 1_100_000
check("V4 ingress >= opening carriage", ingress >= 16_571 * 50, f"{16_571*50}")
honest_cost = ingress + honest_occ + 8 * carriage
reporter = 0.49 * 3 * SOMPI  # 49 % reporter share of the smallest decided slash (D2, 3 MSK)
check("honest prosecutor recovers cost", reporter >= honest_cost,
      f"cost {honest_cost/SOMPI:.4f} MSK, reward {reporter/SOMPI:.2f} MSK")
check("V4 reserved R leaves room", cap - cap // 2 >= 1, f"R={cap//2}")

# --- V5 material
check("V5 consistent with c_retain", ceil(2_250_000 * 50 / 3_120) == 36_058, "36,058 sompi/DAA")
print(f"      filling every block for a nominal day with material costs {720*block*50/SOMPI:.0f} MSK")

raise SystemExit(0 if ok else 1)
