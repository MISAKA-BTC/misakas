"""Capacity model v2: one bond producing as fast as the shipped rules allow, with licence latency
and full-service probability bootstrapped from live public t12 (assign_143) or fixed (harness-like).
Blocks are attributed to the FIRST rule that refuses (exposure, T-2(a) share, class room)."""
import json, random, sys, statistics as st
from capmodel import E, W, c_class, CLASS

LIVE = json.load(open(__import__('os').path.join(__import__('os').path.dirname(__file__), '..', 'live', 'live_lat_143.json')))
KEY = {'floor': 'f1c5', '8k': 'ebf4'}

def run(cls, bond_msk, horizon=1000, mode='live', p_full=None, final_after_lic=121, ready_eff=8, room=None,
        prod_gap=0, seed=7, bind_lic_fixed=1):
    rnd = random.Random(seed)
    ceiling = bond_msk * 0.5
    w = W[cls]
    base = CLASS[cls].get('base', False)
    c = None if base else c_class(ready_eff, CLASS[cls]['window'], CLASS[cls]['eccu'])
    share = None if base else -(-c // 2)
    roomcap = None if base else (room if room is not None else c)
    L = LIVE.get(KEY.get(cls, ''), None)
    claims = []
    acc = [0] * (horizon + 1)
    blocked = {'exposure': 0, 'share': 0, 'room': 0, 'production': 0, 'none': 0}
    next_ready = 0
    for t in range(horizon + 1):
        made_this = 0
        while True:
            if t < next_ready:
                blocked['production'] += 1
                break
            committed = 0.0
            unl = 0
            for x in claims:
                if x['fin'] <= t:
                    continue
                rel = x['rel'] is not None and x['rel'] <= t
                committed += w if rel else w + E
                if not (x['lic'] <= t and x['counts']):
                    unl += 1
            if committed + w + E > ceiling + 1e-9:
                if made_this == 0:
                    blocked['exposure'] += 1
                break
            if share is not None and unl + 1 > share:
                if made_this == 0:
                    blocked['share'] += 1
                break
            if roomcap is not None and unl + 1 > roomcap:
                if made_this == 0:
                    blocked['room'] += 1
                break
            if mode == 'live' and L:
                i = rnd.randrange(len(L['acc_bind']))
                ab, bl, full, counts = L['acc_bind'][i], L['bind_lic'][i], L['full'][i], L['counts'][i]
            else:
                ab, bl = 20, bind_lic_fixed
                full = True if p_full is None else (rnd.random() < p_full)
                counts = True
            if p_full is not None and mode == 'live':
                full = rnd.random() < p_full
            lic = t + ab + bl
            fin = lic + final_after_lic
            claims.append(dict(acc=t, lic=lic, rel=(lic if full else None), fin=fin, counts=counts))
            acc[t] += 1
            made_this += 1
            if prod_gap:
                next_ready = t + prod_gap
                break
    return acc, claims, blocked, share, c

def table(cls, bonds, **kw):
    rows = []
    for b in bonds:
        runs = [run(cls, b, seed=s, **kw) for s in range(1, 21)]
        def avg(f):
            return st.mean(f(r) for r in runs)
        acc0 = runs[0][0]
        cum = {r: avg(lambda x, r=r: sum(x[0][:r + 1])) for r in (0, 1, 2, 5, 10, 20, 40, 80)}
        per80 = avg(lambda x: sum(x[0][200:1000]) / 10.0)
        per1000 = avg(lambda x: sum(x[0][0:1000]))
        per10 = avg(lambda x: sum(x[0][200:1000]) / 80.0)
        bl = runs[0][2]
        tot = sum(v for k, v in bl.items())
        dom = max(((k, v) for k, v in bl.items() if k != 'none'), key=lambda kv: kv[1])
        rows.append(dict(cls=cls, bond=b, instant=acc0[0], cum=cum, per10=per10, per80=per80, per1000=per1000,
                         blocked=bl, dominant=dom[0], share=runs[0][3], c=runs[0][4]))
    return rows

if __name__ == '__main__':
    bonds = [13_000, 20_000, 26_000, 50_000, 100_000, 250_000, 500_000, 1_000_000]
    out = {}
    for mode, kw in (('ideal', dict(mode='fixed', bind_lic_fixed=1)), ('ideal8k3', dict(mode='fixed', bind_lic_fixed=3)), ('live', dict(mode='live'))):
        for cls in ('floor', '8k'):
            if mode == 'ideal8k3' and cls == 'floor':
                continue
            rows = table(cls, bonds, **kw)
            out[f'{mode}:{cls}'] = rows
            print(f'== {mode} {cls}')
            for r in rows:
                c = r['cum']
                print(f"{r['bond']:>9} inst {r['instant']:>3} +1 {c[1]:6.1f} +2 {c[2]:6.1f} +5 {c[5]:6.1f} +10 {c[10]:6.1f} +20 {c[20]:6.1f} +40 {c[40]:6.1f} +80 {c[80]:6.1f} | /10 {r['per10']:5.2f} /80 {r['per80']:6.1f} /1000 {r['per1000']:7.1f} | {r['dominant']} {r['blocked']}")
    
