#!/usr/bin/env python3
"""derive_mirror.py — a line-for-line Python mirror of the ARITHMETIC of `opv-meas derive` (tools/opv-meas/src/derive.rs).

It exists because the cargo run of the Rust tool was queued behind other lanes when the 2026-10-10 revision of opv-measurements.md was
written. It computes the same window and collateral tables from the same labelled inputs (same formulas, same integer rounding, the
same 2^40 probability grid) and leaves out only what needs the kernel crate: `OpvPolicyV1::validate` verdicts and the kernel's
`required_reservation`. The Rust tool is the contract; when it has run, `out-v2-*.json` from it replaces these outputs and the two
are compared (every shared number must agree).

Usage (python -I):  derive_mirror.py IN.json OUT.json
"""
import json
import math
import sys

BILI = 10**8
DEN = 1 << 40
GAIN, PEN = 20 * BILI, 100 * BILI                 # interim OPV terms: reward 5 + credit 5 + external bound 10; default penalty 100
LIVE_PER, LIVE_TOTAL, INTERIM_RES = 3, 32, 1000 * BILI
WINDOW_DAA, CARRIER, REORG, LIABILITY = 50, 2, 2, 200   # interim: window 40 + 10, carrier 2, reorg 2, post-Final liability 200


def ceil_div(a, b): return -(-a // b)
def reservation(gain, pen, r, pn, pd):
    if pn == 0 or r >= 1000: return None
    by_det = ceil_div(gain * pd, pn)
    base = max(gain + pen, by_det)
    return ceil_div(base * 1000, 1000 - r)
def defended(res, r, pn, pd): return res * (1000 - min(r, 1000)) // 1000 * pn // pd
def p_det(cov, q, h, eps):
    p = (1.0 - (1.0 - min(max(q * cov, 0.0), 1.0)) ** h) * min(max(1.0 - eps, 0.0), 1.0)
    return min(math.floor(p * DEN), DEN), DEN
def p_dc(cov, q, h, eps, kappa):
    n, d = p_det(cov, q, h, eps)
    return math.floor(n * min(max(kappa, 0.0), 1.0)), d
def rate(pn, pd, r, horizon):
    if horizon == 0 or pd == 0: return 0.0
    return (pn / pd) * (1000 - min(r, 1000)) / 1000.0 / horizon
def bili(x): return None if x is None else x / BILI
def daa(sec, s): return max(1, math.ceil(sec / s))
def run(path):
    inp = json.load(open(path))
    s_per_daa = inp.get('s_per_daa', 120.0)
    e = inp.get('economics', {})
    n_ver = int(e.get('verifiers_n', 1))
    q = e.get('participation_q', 1.0); eps = e.get('eps_enf', 0.0); kappa = e.get('collection_rate', 1.0)
    r = int(e.get('reporter_return_permille', 490))
    holders = min(int(e.get('model_holders_n', n_ver)), n_ver)
    bond = int(e.get('bond_bili', 13000.0) * BILI)
    horizon = int(e.get('liability_horizon_daa', WINDOW_DAA + LIABILITY))
    def at_p(pn, pd):
        res = reservation(GAIN, PEN, r, pn, pd); resk = reservation(GAIN, PEN, 500, pn, pd)
        return {'p_dc': pn / pd, 'reservation_bili': bili(res), 'reservation_at_interim_kernel_500_permille_bili': bili(resk),
                'reservation_x_interim': None if res is None else res / INTERIM_RES,
                'gain_defended_by_interim_reservation_bili': bili(defended(INTERIM_RES, r, pn, pd)),
                'live_claims_a_bond_holds': None if res is None else float(bond // res),
                'collateral_for_the_interim_live_cap_per_producer_bili': None if res is None else bili(res * LIVE_PER),
                'collateral_the_ledger_locks_at_the_interim_cap_bili': None if res is None else bili(res * LIVE_TOTAL),
                'deterrable_reward_per_daa_per_bili_of_bond': rate(pn, pd, r, horizon)}
    def at(cov, h):
        pn, pd = p_dc(cov, q, min(h, n_ver), eps, kappa)
        v = at_p(pn, pd); v['model_holders'] = min(h, n_ver); return v
    by_p = []
    for p in inp.get('p_dc_values', []):
        v = at_p(min(math.floor(p * DEN), DEN), DEN); v['p_dc_stated'] = p; by_p.append(v)
    chain_s = (CARRIER + REORG) * s_per_daa
    window_s = WINDOW_DAA * s_per_daa
    classes = []
    for c in inp['classes']:
        g = lambda k, d=0.0: c.get(k, d)
        P = c['positions']
        bw = g('bandwidth_bps', 1.25e8); lat = g('latency_s'); speed = max(g('check_speedup', 1.0), 1e-9)
        fixed = g('check_fixed_s'); auth = g('model_auth_s'); once = c.get('model_auth_once', False)
        fetch = lambda m: lat + m * g('position_bytes') / bw
        check = lambda m: ((max(fixed - auth, 0.0) if once else fixed) + m * g('check_per_position_s')) / speed
        mf = g('margin_frac')
        tch = lambda m: (g('beacon_s') + fetch(m) + check(m) + g('localize_s') + g('file_s')) * (1 + mf) + g('margin_s') + chain_s
        base = tch(0)
        if base > window_s: within = 0
        else:
            slope = (g('position_bytes') / bw + g('check_per_position_s') / speed) * (1 + mf)
            within = P if slope <= 0 else min(math.floor((window_s - base) / slope + 1e-9), P)
        rows = []
        for lab, m in (('1 position', 1), ('1 %', max(P // 100, 1)), ('10 %', max(P // 10, 1)), ('50 %', max(P // 2, 1)), ('whole claim', P)):
            t = tch(m); cov = m / P
            rows.append({'checked': lab, 'positions': m, 'coverage': cov, 't_fetch_claim_material_s': fetch(m), 't_check_s': check(m), 't_challenge_s': t,
                         't_challenge_daa': daa(t, s_per_daa), 'model_held_by_the_verifiers': at(cov, holders), 'closed_model_no_third_party_holds_it': at(cov, 0)})
        full = tch(P)
        classes.append({'name': c['name'], 'positions': P, 't_acquire_registered_model_s_off_chain': lat + g('artifact_bytes') / bw, 'model_auth_once_assumed': once,
                        't_challenge_whole_claim_s': full, 't_challenge_whole_claim_daa': daa(full, s_per_daa), 'interim_window_daa': WINDOW_DAA,
                        'positions_within_interim_window': within, 'coverage_within_interim_window': within / P, 'rows': rows})
    return {'reporter_return_permille': r, 'by_p_dc': by_p, 'classes': classes}


if __name__ == "__main__":
    out = run(sys.argv[1])
    json.dump(out, open(sys.argv[2], "w"), indent=1)
    print(f"wrote {sys.argv[2]}")
