"""Model v3: licence outcomes as observed live (DAA 140-170, 384 licences, no later upgrades seen):
floor: all5 90.6 %, k>=2 but not all served 5.7 % (escrow to Final), S2 k=1 3.8 % (escrow AND the
claim held to bound+601, then a redraw: modelled as released at bound+601+22).
8k: all5 69 %, k>=2 partial 15 %, S2 15 %.
Latencies bootstrapped from live acc->bind / bind->lic. One bond producing as fast as the rules allow
(production gap 0 = consensus bound; 8k also with a 2.5 DAA inference gap as the live b0)."""
import json, random, statistics as st, sys
from capmodel import E, W, c_class, CLASS
LIVE = json.load(open(__import__('os').path.join(__import__('os').path.dirname(__file__), '..', 'live', 'live_lat_143.json')))
MIX = {'floor': (0.906, 0.057, 0.038), '8k': (0.69, 0.155, 0.155)}
KEY = {'floor': 'f1c5', '8k': 'ebf4'}

def run(cls, bond, horizon=1000, seed=1, mix=None, prod_gap=0.0, ready_eff=8, room=None, lat='live'):
    rnd = random.Random(seed)
    ceiling = bond * 0.5
    w = W[cls]
    base = CLASS[cls].get('base', False)
    c = None if base else c_class(ready_eff, CLASS[cls]['window'], CLASS[cls]['eccu'])
    share = None if base else -(-c // 2)
    roomcap = None if base else (room if room is not None else c)
    pf, pp, ps = mix or MIX[cls]
    L = LIVE[KEY[cls]]
    claims = []
    acc = [0] * (horizon + 1)
    next_ready = 0.0
    for t in range(horizon + 1):
        while t >= next_ready - 1e-9:
            committed = 0.0
            unl = 0
            for x in claims:
                if x['fin'] <= t:
                    continue
                rel = x['rel'] is not None and x['rel'] <= t
                committed += w if rel else w + E
                if not (x['cnt'] is not None and x['cnt'] <= t):
                    unl += 1
            if committed + w + E > ceiling + 1e-9:
                break
            if share is not None and unl + 1 > share:
                break
            if roomcap is not None and unl + 1 > roomcap:
                break
            if lat == 'live':
                i = rnd.randrange(len(L['acc_bind']))
                ab, bl = L['acc_bind'][i], L['bind_lic'][i]
            else:
                ab, bl = lat
            bind = t + ab
            lic = bind + bl
            u = rnd.random()
            if u < pf:
                x = dict(rel=lic, cnt=lic, fin=lic + 121)
            elif u < pf + pp:
                x = dict(rel=None, cnt=lic, fin=lic + 121)
            else:  # S2: counted and released only after bound+601 (then a redraw licenses it)
                late = bind + 601 + 22
                x = dict(rel=late, cnt=late, fin=late + 121)
            claims.append(x)
            acc[t] += 1
            if prod_gap:
                next_ready = t + prod_gap
                break
    return acc

def summary(cls, bond, **kw):
    runs = [run(cls, bond, seed=s, **kw) for s in range(1, 31)]
    cum = lambda r: st.mean(sum(a[:r + 1]) for a in runs)
    per80_first = st.mean(sum(a[0:80]) for a in runs)
    per80_late = st.mean(sum(a[200:1000]) / 10 for a in runs)
    per1000 = st.mean(sum(a[0:1000]) for a in runs)
    return dict(inst=runs[0][0], c20=cum(20), c40=cum(40), c80=cum(80), first80=per80_first, per80=per80_late, per1000=per1000)

if __name__ == '__main__':
    for cls, gaps in (('floor', (0,)), ('8k', (0, 2.5))):
        for gap in gaps:
            print(f'== {cls} live latencies + live licence mix, production gap {gap}')
            for b in (13_000, 20_000, 26_000, 50_000, 100_000, 250_000, 500_000, 1_000_000):
                s = summary(cls, b, prod_gap=gap)
                print(f"{b:>9} inst {s['inst']:>3} +20 {s['c20']:6.1f} +40 {s['c40']:6.1f} +80 {s['c80']:6.1f} | first80 {s['first80']:6.1f} steady/80 {s['per80']:6.1f} /1000 {s['per1000']:7.1f}")
    print('== 8k ideal mix (all5), live latency, gap 2.5')
    for b in (13_000, 26_000, 100_000):
        s = summary('8k', b, mix=(1, 0, 0), prod_gap=2.5)
        print(b, s)
    print('== floor ideal mix (all5), live latency')
    for b in (13_000, 26_000, 100_000):
        s = summary('floor', b, mix=(1, 0, 0))
        print(b, s)
