"""Network seat-capital capacity (the floor's panel limit) on testnet-12, shipped vs V02(a).
Measured in the harness: duty 640.17 MSK per seat per bound floor claim (bind -> Final);
lock per Valid seat after Final: 240.1 MSK at the coverage door (k = 2, all five seats) = 1,200.6 per
claim; 160.09 at the V1 quorum door (k = 3, three seats) = 480.3 per claim (bind-deadlock m0)."""
DUTY = 640.1693
T_DUTY = 122
T_LOCK = 3000
C = 939_063.21
CEIL = C / 2
R_ACC = 4 * 13_000
N = 8
def per_claim(lock_total):
    return 5 * DUTY * T_DUTY + lock_total * T_LOCK
for label, lock in (('coverage (all 5, k=2)', 1200.6), ('V1 quorum (3, k=3)', 480.3), ('live mix 89/7/4', 0.89 * 1200.6 + 0.11 * 480.3)):
    pc = per_claim(lock)
    ship = N * CEIL / pc
    v02 = min(N * (C - R_ACC) / pc, N * CEIL / (5 * DUTY * T_DUTY))
    print(f'{label:24s} seat capital {pc/1e6:.2f} M MSK*DAA/claim; long-run network capacity shipped {ship:.3f}/DAA ({ship*80:.0f}/80), V02 {v02:.3f}/DAA ({v02*80:.0f}/80)')
    for r in (1.0, 2.0, 2.8):
        duty_seat = r * DUTY * T_DUTY * 5 / N
        lock_rate = r * lock / N
        t_ship = 177 + (CEIL - duty_seat) / lock_rate
        t_v02 = 177 + (C - R_ACC - duty_seat) / lock_rate
        steady = duty_seat + lock_rate * T_LOCK
        sat_s = 'never' if steady <= CEIL else f'~DAA {t_ship:.0f}'
        sat_v = 'never' if steady <= C - R_ACC else f'~DAA {t_v02:.0f}'
        print(f'    r={r}/DAA: per-seat duties {duty_seat:,.0f}, lock growth {lock_rate:.0f}/DAA/seat; saturates shipped {sat_s}, V02 {sat_v}')
