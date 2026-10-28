"""Discrete-event model of one producer bond's claim capacity on testnet-12 (release 0e8ec984e).

Every rule below is the shipped rule, cited in the capacity map:
  * exposure gate (fold palw_state_v2.rs:31087-31106, admission item 8 palw_admission_v2.rs:~681):
      committed + (w_c + E) <= floor(C * 500/1000)   [and <= C - committed - accuser]
      committed = sum over own live claims of commitment(claim)            (palw_state_v2.rs:2692)
      commitment = w_c + E   while unreleased (Provisional/Bound/Licensed without escrow release)
                 = w_c       once escrow_released (SR-1: Quorum/Coverage door, k>=2, ALL seats served)
                 = 0         at Final (and at a void without abandon hold)
  * T-2(a) per-bond share for a non-base class (palw_state_v2.rs:13349-13391):
      unlicensed(bond, class) + 1 <= ceil(c_class/2),  c_class = floor(ready_eff*RW*U*window/(1000*eccu*seats))
      'unlicensed' = in flight and not palw_rcore_counts_licensed_v1 (ReceiptLicensed with basis_k>=2)
  * class room (network-wide, palw_state_v2.rs:13025-13129 + 13208-13260): owed_class + 1 <= c_class_eff
Timeline of one claim (live-measured defaults, overridable):
  accept t -> bind t+anchor_delay(20)+anchor_wait -> licence bind+lic_after_bind
  -> escrow release at licence if all seats served (else at Final) -> Final at L+121.
"""
import math, json, sys, random

E = 3_200.8465008          # MSK escrow per claim (worker carve of the block subsidy, class independent)
W = {'floor': 0.1075266, '8k': 24.7160023, '2M': 59_742.9397}   # MSK weight term per claim (claim.reserved)
RW = 2_400_000_000_000     # reference_work_per_span
U = 700                    # utilization permille
SEATS = 5

def c_class(ready_eff, window, eccu, others_scaled_per_span=0.0):
    per_span = ready_eff * RW * U // 1000
    free = max(0.0, per_span - others_scaled_per_span)
    return int(free * window // (eccu * SEATS))

CLASS = {
    'floor': dict(base=True),
    '8k': dict(base=False, window=3, eccu=1_388_926_972_416),
    '2M': dict(base=False, window=2799, eccu=3_357_281_757_221_376, c7=True, static_cap=1),
}

def share_of(cls, ready_eff=8):
    k = CLASS[cls]
    if k['base']:
        return None
    if k.get('c7'):
        c = k['static_cap']
    else:
        c = c_class(ready_eff, k['window'], k['eccu'])
    return -(-c // 2)

def simulate(cls, bond_msk, horizon=1000, anchor_delay=20, anchor_wait=0, lic_after_bind=(1,), p_full=1.0,
             lic_counts=True, final_after_lic=121, prod_gap=0.0, max_per_daa=10**6, ready_eff=8,
             class_room=None, seed=1, release_lag=0):
    """Returns per-claim lives and per-DAA accepted counts for ONE bond producing as fast as the rules allow.
    prod_gap: DAA an inference takes (0 = consensus-bound upper bound); class_room: network room cap (None = c_class)."""
    rnd = random.Random(seed)
    ceiling = bond_msk * 500 / 1000.0
    w = W[cls]
    share = share_of(cls, ready_eff)
    room_cap = class_room
    if room_cap is None and not CLASS[cls]['base']:
        room_cap = CLASS[cls]['static_cap'] if CLASS[cls].get('c7') else c_class(ready_eff, CLASS[cls]['window'], CLASS[cls]['eccu'])
    claims = []  # dicts: acc, lic, rel, fin, full, counted
    accepted = [0] * (horizon + 1)
    next_ready = 0.0
    for t in range(horizon + 1):
        made = 0
        while made < max_per_daa and t >= next_ready - 1e-9:
            committed = 0.0
            unl = 0
            for c in claims:
                if c['fin'] <= t:
                    continue
                released = c['rel'] is not None and c['rel'] <= t
                committed += w if released else (w + E)
                counted = c['lic'] <= t and c['counts']
                if not counted:
                    unl += 1
            if committed + w + E > ceiling + 1e-9:
                break
            if share is not None and unl + 1 > share:
                break
            if room_cap is not None and unl + 1 > room_cap:
                break
            lab = rnd.choice(lic_after_bind) if isinstance(lic_after_bind, (list, tuple)) else lic_after_bind
            bind = t + anchor_delay + anchor_wait
            lic = bind + lab
            full = rnd.random() < p_full
            rel = (lic + release_lag) if full else None
            fin = lic + final_after_lic
            claims.append(dict(acc=t, bind=bind, lic=lic, rel=rel, fin=fin, full=full, counts=lic_counts))
            accepted[t] += 1
            made += 1
            if prod_gap > 0:
                next_ready = t + prod_gap
                break
    return claims, accepted

def cum_at(accepted, rel):
    return sum(accepted[: rel + 1])

if __name__ == '__main__':
    for cls in ('floor', '8k'):
        for b in (13_000, 20_000, 26_000, 50_000, 100_000, 250_000, 500_000, 1_000_000):
            claims, acc = simulate(cls, b, horizon=1000, lic_after_bind=(1,) if cls == 'floor' else (3,))
            print(cls, b, 'instant', acc[0], [cum_at(acc, r) for r in (1, 2, 5, 10, 20, 40, 80)], 'per80(80..160)', sum(acc[80:160]), 'per1000', sum(acc[:1000]))
