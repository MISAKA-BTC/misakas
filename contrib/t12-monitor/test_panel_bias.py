#!/usr/bin/env python3
"""Unit tests for panel_bias.py: the statistics on synthetic data, the anchor rule, the header
decoding, the state file, and collection from a fake read-only node. Stdlib only; no network.

    python3 -m unittest -v contrib/t12-monitor/test_panel_bias.py
"""
import contextlib
import io
import itertools
import json
import math
import os
import random
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import panel_bias as pb  # noqa: E402

TX = pb.T12_PREMINE
FLOOR = "f1c5635c" + "0" * 120
G = [f"{TX}:{i}" for i in range(8)]
EXT = "ab" * 64 + ":0"
W_GENESIS = 939_063
NAMES = pb.Names(TX, pb.DEFAULT_ROSTER)


def brute_inclusion(weights, k):
    """P(i drawn) and P(i, j drawn) by enumerating every ordered draw of successive sampling."""
    n = len(weights)
    pi = [0.0] * n
    pij = [[0.0] * n for _ in range(n)]
    for seq in itertools.permutations(range(n), k):
        p, left = 1.0, float(sum(weights))
        for i in seq:
            p *= weights[i] / left
            left -= weights[i]
        for a in seq:
            pi[a] += p
            for b in seq:
                pij[a][b] += p
    return pi, pij


def race(pop, k, rng):
    """The exponential race over {bond: weight}: the k smallest Exp(1)/w."""
    keys = sorted(pop, key=lambda b: rng.expovariate(1.0) / pop[b])
    return keys[:k]


def synthetic_records(n_claims, rng, grind=None, anchors=(G[1], G[6]), executors=(G[1], G[6]), k=5, ext_weight=None,
                      start=0, drop=None, prefix=""):
    """Claims of the floor class drawn fairly from the 8 genesis bonds minus the executor, one per
    DAA from `start`. With grind = (anchor, {bonds}), that anchor producer re-rolls each of its
    panels until it seats every bond in the set (what a grinding anchor producer does for its
    Sybils). With drop = (bond, share), that bond is out of the drawable set for that share of the
    claims whatever the anchor (a population-wide effect the model does not see)."""
    recs = {}
    for n in range(start, start + n_claims):
        exe = rng.choice(executors)
        anchor = rng.choice(anchors)
        pop = {b: W_GENESIS for b in G if b != exe}
        if ext_weight:
            pop[EXT] = ext_weight
        drawable = dict(pop)
        if drop and rng.random() < drop[1]:
            drawable.pop(drop[0], None)
        seats = race(drawable, k, rng)
        if grind and anchor == grind[0]:
            while not grind[1] <= set(seats):
                seats = race(drawable, k, rng)
        cid = f"{prefix}{n:05x}" + "c" * (123 - len(prefix))
        recs[cid] = {"cls": FLOOR, "exe": exe, "phase": "receipt_licensed", "void": "", "acc": n, "reb": None,
                     "bound": n + 20, "seats": seats, "anchor": f"{n:05x}" + "a" * 123, "anchorDaa": n + 20,
                     "anchorBond": anchor, "anchorAlgo": 6, "anchorCheck": "ok", "pop": dict(pop)}
    return recs


def any_alert(res):
    cells, omni = pb.bias_findings(res)
    return bool(cells or omni)


def rel_sig(res):
    return {(c["anchor"], c["seat"]) for w in res["windows"] for c in w["rel_cells"] if c.get("significant")}


class InclusionProbabilities(unittest.TestCase):
    def test_equal_weights_are_k_over_n(self):
        pi, pij = pb.inclusion_probabilities([W_GENESIS] * 7, 5)
        for i in range(7):
            self.assertAlmostEqual(pi[i], 5 / 7, places=12)
            for j in range(7):
                self.assertAlmostEqual(pij[i][j], 5 / 7 if i == j else 5 * 4 / (7 * 6), places=12)

    def test_unequal_weights_match_enumeration(self):
        for weights, k in (([1, 2, 3, 5, 8], 3), ([4, 4, 1, 1, 1, 9], 4), ([939_063] * 7 + [130_000] * 2, 5)):
            pi, pij = pb.inclusion_probabilities(weights, k)
            bpi, bpij = brute_inclusion(weights, k)
            self.assertAlmostEqual(sum(pi), k, places=10)
            for i in range(len(weights)):
                self.assertAlmostEqual(pi[i], bpi[i], places=10)
                for j in range(len(weights)):
                    self.assertAlmostEqual(pij[i][j], bpij[i][j], places=10)

    def test_heavier_bond_is_drawn_more_often(self):
        pi, _ = pb.inclusion_probabilities([1_000_000, 130_000, 130_000, 130_000, 130_000, 130_000], 3)
        self.assertGreater(pi[0], pi[1])
        self.assertLess(pi[0], 1.0)

    def test_monte_carlo_fallback_agrees_with_the_dp(self):
        weights = [1, 2, 3, 5, 8, 13]
        exact, _ = pb.inclusion_probabilities(weights, 3)
        mc, mcp = pb.inclusion_probabilities(weights, 3, max_states=1, mc_samples=40_000)
        for a, b in zip(exact, mc):
            self.assertAlmostEqual(a, b, delta=0.012)
        self.assertAlmostEqual(sum(mc), 3, places=9)

    def test_degenerate_sizes(self):
        self.assertEqual(pb.inclusion_probabilities([5, 6, 7], 3)[0], [1.0, 1.0, 1.0])
        self.assertEqual(pb.inclusion_probabilities([5, 6, 7], 0)[0], [0.0, 0.0, 0.0])
        with self.assertRaises(ValueError):
            pb.inclusion_probabilities([1, 0], 1)


class PoissonBinomial(unittest.TestCase):
    def test_equal_ps_are_the_binomial(self):
        pmf = pb.poisson_binomial_pmf([0.3] * 12)
        for x in range(13):
            self.assertAlmostEqual(pmf[x], math.comb(12, x) * 0.3 ** x * 0.7 ** (12 - x), places=12)

    def test_mixed_ps_match_direct_convolution(self):
        rng = random.Random(7)
        ps = [rng.random() for _ in range(25)] + [5 / 7] * 30 + [0.0, 1.0]
        direct = [1.0]
        for p in ps:
            nxt = [0.0] * (len(direct) + 1)
            for x, q in enumerate(direct):
                nxt[x] += q * (1 - p)
                nxt[x + 1] += q * p
            direct = nxt
        pmf = pb.poisson_binomial_pmf(ps)
        self.assertAlmostEqual(sum(pmf), 1.0, places=10)
        for a, b in zip(pmf, direct):
            self.assertAlmostEqual(a, b, places=11)

    def test_two_sided_p(self):
        p, up, lo = pb.poisson_binomial_test([0.5] * 10, 10)
        self.assertAlmostEqual(up, 1 / 1024, places=15)
        self.assertAlmostEqual(p, 2 / 1024, places=15)
        self.assertEqual(pb.poisson_binomial_test([0.5] * 10, 5)[0], 1.0)
        p0, _, lo0 = pb.poisson_binomial_test([0.5] * 10, 0)
        self.assertAlmostEqual(lo0, 1 / 1024, places=15)
        with self.assertRaises(ValueError):
            pb.poisson_binomial_test([0.5] * 3, 4)

    def test_large_n_is_fast_and_normalised(self):
        pmf = pb.poisson_binomial_pmf([5 / 7] * 3000 + [5 / 6] * 1000)
        self.assertAlmostEqual(sum(pmf), 1.0, places=9)
        p, _, _ = pb.poisson_binomial_test([5 / 7] * 3000, 3000)     # seated every time: impossible by chance
        self.assertLess(p, 1e-100)

    def test_normal_fallback_tracks_the_exact_test(self):
        ps = [5 / 7] * 1500 + [5 / 6] * 1500
        mean = sum(ps)
        for obs in (int(mean) - 80, int(mean) - 20, int(mean), int(mean) + 45):
            exact = pb.poisson_binomial_test(ps, obs)[0]
            approx = pb.poisson_binomial_test(ps, obs, exact_max=2000)[0]
            self.assertLess(abs(math.log(approx) - math.log(exact)), 0.35, (obs, exact, approx))


class Hypergeometric(unittest.TestCase):
    def test_pmf_matches_counting(self):
        for N, K, n in ((10, 4, 3), (7, 5, 5), (30, 21, 12), (5, 0, 2), (5, 5, 2)):
            lo, pmf = pb.hypergeom_pmf(N, K, n)
            for i, p in enumerate(pmf):
                x = lo + i
                self.assertAlmostEqual(p, math.comb(K, x) * math.comb(N - K, n - x) / math.comb(N, n), places=12)
            self.assertAlmostEqual(sum(pmf), 1.0, places=12)

    def test_stratified_exact_matches_enumeration(self):
        strata = [(6, 4, 2), (5, 2, 3), (7, 5, 1), (4, 4, 2), (3, 0, 1)]
        dist = {}
        pmfs = [pb.hypergeom_pmf(*s) for s in strata]
        for combo in itertools.product(*[range(len(p)) for _, p in pmfs]):
            x = sum(lo + i for (lo, _), i in zip(pmfs, combo))
            dist[x] = dist.get(x, 0.0) + math.prod(p[i] for (_, p), i in zip(pmfs, combo))
        for obs in sorted(dist):
            up = sum(v for x, v in dist.items() if x >= obs)
            lo_ = sum(v for x, v in dist.items() if x <= obs)
            p2, pu, pl, mean, var = pb.stratified_hypergeom_test(strata, obs)
            self.assertAlmostEqual(pu, up, places=12)
            self.assertAlmostEqual(pl, lo_, places=12)
            self.assertAlmostEqual(p2, min(1.0, 2 * min(up, lo_)), places=12)
        self.assertAlmostEqual(mean, sum(x * v for x, v in dist.items()), places=12)
        self.assertAlmostEqual(var, sum(x * x * v for x, v in dist.items()) - mean ** 2, places=10)

    def test_normal_beyond_the_cost_bound_tracks_the_exact_into_the_tails(self):
        """Out to 5 sd the skewness-corrected normal stays within a factor ~1.5 of the exact tail on
        a large sum, and on a small skewed one it errs on the side of a larger p over the seat
        (no false alarm), where the plain normal errs toward a false alarm."""
        big = [(30, 5, 12)] * 40
        mean, var = pb.stratified_hypergeom_test(big, 80)[3:]
        for z in (-3.0, 3.0, 4.0, 5.0):
            obs = int(round(mean + z * math.sqrt(var)))
            exact = pb.stratified_hypergeom_test(big, obs)[0]
            approx = pb.stratified_hypergeom_test(big, obs, max_cost=1)[0]
            self.assertLess(abs(math.log(approx) - math.log(exact)), 0.45, (z, exact, approx))
        small = [(20, 2, 5)] * 30
        mean, var = pb.stratified_hypergeom_test(small, 15)[3:]
        for z in (4.0, 5.0, 6.0):
            obs = int(round(mean + z * math.sqrt(var)))
            exact = pb.stratified_hypergeom_test(small, obs)[0]
            self.assertGreaterEqual(pb.stratified_hypergeom_test(small, obs, max_cost=1)[0], exact)
            self.assertLess(pb.normal_test(obs, mean, var)[0], exact / 2)


class LinearAlgebra(unittest.TestCase):
    def test_jacobi_reconstructs_the_matrix(self):
        rng = random.Random(3)
        m = 6
        b = [[rng.gauss(0, 1) for _ in range(m)] for _ in range(m)]
        a = [[sum(b[i][k] * b[j][k] for k in range(m)) for j in range(m)] for i in range(m)]
        vals, vecs = pb.sym_eig(a)
        for i in range(m):
            for j in range(m):
                self.assertAlmostEqual(sum(vecs[i][k] * vals[k] * vecs[j][k] for k in range(m)), a[i][j], places=9)

    def test_pseudo_inverse_quadratic(self):
        a = [[4.0, 1.0, 0.5], [1.0, 3.0, 0.2], [0.5, 0.2, 2.0]]
        d = [1.0, -2.0, 0.5]
        # full rank: d' A^-1 d by solving
        q, rank = pb.pinv_quadratic(d, a)
        det = (a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
               + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]))
        inv = [[(a[(j + 1) % 3][(i + 1) % 3] * a[(j + 2) % 3][(i + 2) % 3] - a[(j + 1) % 3][(i + 2) % 3] * a[(j + 2) % 3][(i + 1) % 3]) / det
                for j in range(3)] for i in range(3)]
        self.assertEqual(rank, 3)
        self.assertAlmostEqual(q, sum(d[i] * inv[i][j] * d[j] for i in range(3) for j in range(3)), places=10)
        # a covariance with rows summing to zero (each claim seats exactly k): rank m-1
        c = [[2.0, -1.0, -1.0], [-1.0, 2.0, -1.0], [-1.0, -1.0, 2.0]]
        self.assertEqual(pb.pinv_quadratic([1.0, -1.0, 0.0], c)[1], 2)


class ChiSquare(unittest.TestCase):
    def test_known_quantiles(self):
        self.assertAlmostEqual(pb.chi2_sf(3.841458820694124, 1), 0.05, places=10)
        self.assertAlmostEqual(pb.chi2_sf(5.991464547107979, 2), 0.05, places=10)
        self.assertAlmostEqual(pb.chi2_sf(18.307038053275146, 10), 0.05, places=9)
        for x in (0.01, 0.1025865887751011, 1.0, 4.0, 12.0, 30.0):
            self.assertAlmostEqual(pb.chi2_sf(x, 1) / math.erfc(math.sqrt(x / 2)), 1.0, places=9)          # df 1
            df3 = math.erfc(math.sqrt(x / 2)) + math.sqrt(2 * x / math.pi) * math.exp(-x / 2)
            self.assertAlmostEqual(pb.chi2_sf(x, 3) / df3, 1.0, places=9)                                 # df 3

    def test_df2_closed_form_and_tails(self):
        for x in (0.01, 0.5, 2.0, 9.0, 40.0, 100.0):
            self.assertAlmostEqual(pb.chi2_sf(x, 2) / math.exp(-x / 2), 1.0, places=9)
        self.assertEqual(pb.chi2_sf(0.0, 3.3), 1.0)

    def test_non_integer_df_is_monotone(self):
        vals = [pb.chi2_sf(6.0, df) for df in (1.5, 2.5, 3.7, 6.98, 11.2)]
        self.assertEqual(vals, sorted(vals))


class Holm(unittest.TestCase):
    def test_step_down(self):
        self.assertEqual(pb.holm([0.001, 0.02, 0.03, 0.5], 0.05), [True, False, False, False])
        self.assertEqual(pb.holm([0.01, 0.012, 0.013, 0.014], 0.06), [True, True, True, True])
        self.assertEqual(pb.holm([], 0.05), [])


class Calibration(unittest.TestCase):
    def test_model_rao_scott_calibration_and_why_the_correction_is_needed(self):
        """Fair draws of 5 of 7 (the floor under a genesis executor): the corrected model omnibus
        rejects at about its nominal rate; the uncorrected Pearson X2 almost never does (draws
        without replacement vary much less than Poisson counts)."""
        rng = random.Random(2026)
        reps, rs_rej, pearson_rej = 300, 0, 0
        for _ in range(reps):
            recs = synthetic_records(150, rng, anchors=(G[1],))
            res = pb.analyse(recs, NAMES, alpha=0.05)
            o = [x for x in res["windows"][0]["model_omnibus"] if x["anchor"] == G[1]][0]
            self.assertTrue(o["evaluated"])
            rs_rej += o["p"] <= 0.05
            pearson_rej += pb.chi2_sf(o["x2"], o["bonds"] - 1) <= 0.05
        self.assertTrue(0.02 <= rs_rej / reps <= 0.09, rs_rej / reps)
        self.assertLess(pearson_rej / reps, 0.01)

    def test_relative_omnibus_rejects_at_its_nominal_rate(self):
        """Fair draws, two anchor producers: the generalized CMH omnibus of one producer against the
        other rejects at about its nominal 0.05."""
        rng = random.Random(77)
        reps, rej, evaluated = 300, 0, 0
        for _ in range(reps):
            res = pb.analyse(synthetic_records(200, rng), NAMES, alpha=0.05)
            o = [x for x in res["windows"][0]["rel_omnibus"] if x["anchor"] == G[6]][0]
            if o["evaluated"]:
                evaluated += 1
                rej += o["p"] <= 0.05
        self.assertEqual(evaluated, reps)
        self.assertTrue(0.02 <= rej / reps <= 0.09, rej / reps)


class Analyse(unittest.TestCase):
    def test_fair_draws_raise_nothing(self):
        res = pb.analyse(synthetic_records(600, random.Random(11)), NAMES, alpha=1e-4, windows=(150, 300))
        self.assertEqual(res["claims_used"], 600)
        self.assertEqual([w["name"] for w in res["windows"]], ["all", "last 300", "last 150"])
        self.assertFalse(any_alert(res))
        self.assertEqual(sorted(res["anchors"]), sorted([G[1], G[6]]))
        self.assertGreater(res["comparable"], 500)

    def test_executor_never_expected_on_its_own_panel(self):
        recs = synthetic_records(200, random.Random(3), executors=(G[1],))
        res = pb.analyse(recs, NAMES)
        w = res["windows"][0]
        self.assertFalse([c for c in w["rel_cells"] if c["seat"] == G[1]])          # never seated: no variation to test
        g1 = [c for c in w["model_cells"] if c["seat"] == G[1]]
        self.assertTrue(all(c["obs"] == 0 for c in g1))
        self.assertFalse(any(c["exp"] > 0 for c in g1))

    def test_a_grinding_anchor_producer_is_caught_and_the_model_says_which_side(self):
        recs = synthetic_records(300, random.Random(5), grind=(G[6], {G[2], G[3]}))
        res = pb.analyse(recs, NAMES, alpha=1e-4)
        sig = rel_sig(res)
        self.assertIn((G[6], G[2]), sig)
        self.assertIn((G[6], G[3]), sig)
        omni = {o["anchor"]: o for o in res["windows"][0]["rel_omnibus"]}
        self.assertTrue(omni[G[6]]["significant"])
        model = {(c["anchor"], c["seat"]): c for c in res["windows"][0]["model_cells"]}
        self.assertTrue(model[(G[6], G[2])]["significant"] and model[(G[6], G[3])]["significant"])
        self.assertFalse(model[(G[1], G[2])]["significant"] or model[(G[1], G[3])]["significant"])
        cells, _ = pb.bias_findings(res)
        self.assertEqual(cells[0][0]["anchor"], G[6])                # the side the model disowns comes first

    def test_a_population_wide_effect_is_a_note_not_an_anchor_bias(self):
        """Seat g2 is out of the drawable set for a quarter of the claims whatever the anchor (a
        Valid-lock saturation or a stale readiness row the model cannot see): with anchors g1 79% /
        g6 21% as on the live chain, the model cells light up, the anchor comparison does not."""
        rng = random.Random(11)
        recs = {}
        for n in range(1500):
            recs.update(synthetic_records(1, rng, anchors=(G[1],) if rng.random() < 0.79 else (G[6],), start=n, drop=(G[2], 0.25)))
        res = pb.analyse(recs, NAMES, alpha=1e-4, windows=(150, 600))
        self.assertFalse(any_alert(res))
        notes = {(c["anchor"], c["seat"]) for w in res["windows"] for c in w["model_cells"] if c.get("significant")}
        self.assertIn(("*", G[2]), notes)
        self.assertIn((G[1], G[2]), notes)

    def test_a_grind_after_a_long_honest_history_is_caught_by_the_recent_windows(self):
        rng = random.Random(9)
        recs = synthetic_records(2000, rng)
        recs.update(synthetic_records(200, rng, grind=(G[6], {G[2], G[3]}), start=2000, prefix="g"))
        windows = pb.parse_args([]).windows
        res = pb.analyse(recs, NAMES, alpha=1e-4, windows=windows)
        cells, _ = pb.bias_findings(res)
        caught = [(w, c["seat"]) for c, w, _ in cells if c["anchor"] == G[6] and c["obs"] > c["exp"]]
        self.assertTrue(caught and all(s in (G[2], G[3]) for _, s in caught), caught)
        self.assertTrue(any(w in ("last 100", "last 200") for w, _ in caught), caught)
        whole = pb.analyse(recs, NAMES, alpha=1e-4)              # the whole history alone does not see it yet
        self.assertFalse(any(a == G[6] for a, _ in rel_sig(whole)))

    def test_two_producers_are_one_hypothesis_per_seat(self):
        res = pb.analyse(synthetic_records(300, random.Random(5), grind=(G[6], {G[2], G[3]})), NAMES, alpha=1e-4)
        w = res["windows"][0]
        self.assertEqual(res["cells_tested"], len([c for c in w["rel_cells"] if not c["mirror"]]))
        self.assertEqual(res["cells_tested"] * 2, len(w["rel_cells"]))
        by = {(c["anchor"], c["seat"]): c for c in w["rel_cells"]}
        for seat in (G[2], G[3]):
            self.assertAlmostEqual(by[(G[1], seat)]["p"], by[(G[6], seat)]["p"], places=12)
            self.assertTrue(by[(G[1], seat)]["significant"] and by[(G[6], seat)]["significant"])
        three = pb.analyse(synthetic_records(300, random.Random(5), anchors=(G[0], G[1], G[6])), NAMES)
        self.assertFalse(any(c["mirror"] for c in three["windows"][0]["rel_cells"]))

    def test_family_wise_false_alarm_rate_is_bounded(self):
        """alpha = 0.05 per family, two families, three windows: a fair world alarms in at most ~2 * alpha of runs."""
        rng = random.Random(99)
        runs, alarms = 120, 0
        for _ in range(runs):
            alarms += any_alert(pb.analyse(synthetic_records(240, rng), NAMES, alpha=0.05, windows=(60, 120)))
        self.assertLessEqual(alarms / runs, 0.14, alarms / runs)

    def test_stake_weights_move_the_expectation(self):
        recs = synthetic_records(400, random.Random(8), ext_weight=130_000)
        res = pb.analyse(recs, NAMES, alpha=1e-4)
        model = res["windows"][0]["model_cells"]
        ext = [c for c in model if c["anchor"] == "*" and c["seat"] == EXT][0]
        gen = [c for c in model if c["anchor"] == "*" and c["seat"] == G[3]][0]
        self.assertLess(ext["exp"] / ext["n"], gen["exp"] / gen["n"])
        self.assertFalse(any_alert(res))

    def test_windows_and_unusable_records(self):
        recs = synthetic_records(50, random.Random(1))
        first = next(iter(recs))
        recs[first]["anchorCheck"] = "mismatch"
        self.assertEqual(pb.analyse(recs, NAMES)["claims_used"], 49)
        res = pb.analyse(recs, NAMES, windows=(10, 500))
        self.assertEqual([(w["name"], w["claims"]) for w in res["windows"]], [("all", 49), ("last 10", 10)])  # 500 = all: not tested twice
        recs[first]["anchorCheck"] = "ok"
        recs[first]["popUncertain"] = True
        res = pb.analyse(recs, NAMES)
        self.assertEqual(res["windows"][0]["model_omnibus"][-1]["claims"], 49)   # an uncertain population is left out of the model
        self.assertEqual(sum(o["claims"] for o in res["windows"][0]["rel_omnibus"]), 50)


class ExternalAlerts(unittest.TestCase):
    def test_external_seats_are_alerted_once_per_bond(self):
        recs = synthetic_records(20, random.Random(4))
        a, b = list(recs)[:2]
        recs[a]["seats"][0] = EXT
        recs[b]["seats"][0] = EXT
        ext = pb.find_external(recs, NAMES, {"external_seat_bonds": [], "external_anchor": []}, False)
        self.assertEqual(ext["new_bonds"], [EXT])
        self.assertEqual(sorted(ext["seat_claims"][EXT]), sorted([a, b]))
        again = pb.find_external(recs, NAMES, {"external_seat_bonds": [EXT], "external_anchor": []}, False)
        self.assertEqual(again["new_bonds"], [])
        self.assertEqual(pb.find_external(recs, NAMES, {"external_seat_bonds": [EXT], "external_anchor": []}, True)["new_bonds"], [EXT])

    def test_external_anchors_alert_only_past_the_stopgap(self):
        recs = synthetic_records(40, random.Random(4))
        ids = list(recs)
        recs[ids[3]]["anchorBond"] = EXT          # anchorDaa 23
        recs[ids[30]]["anchorBond"] = EXT         # anchorDaa 50
        none = {"external_seat_bonds": [], "external_anchor": []}
        before = pb.find_external(recs, NAMES, none, False)
        self.assertEqual((before["pre"], before["post"], before["new_post"]), (sorted([ids[3], ids[30]]), [], []))
        armed = pb.find_external(recs, NAMES, none, False, stopgap_from=40)
        self.assertEqual((armed["pre"], armed["post"], armed["new_post"]), ([ids[3]], [ids[30]], [ids[30]]))
        self.assertEqual(pb.find_external(recs, NAMES, {"external_seat_bonds": [], "external_anchor": [ids[30]]}, False, 40)["new_post"], [])


class ChainFacts(unittest.TestCase):
    @staticmethod
    def envelope(cls_hex, bond_tx, bond_ix, op_hex, pubkey=b"\x01" * 2592):
        return (b"PAV2" + struct.pack("<H", 5) + b"\x11" * 64 + b"\x22" * 64 + bytes.fromhex(cls_hex) + bytes.fromhex(bond_tx)
                + struct.pack("<I", bond_ix) + struct.pack("<I", len(pubkey)) + pubkey + bytes.fromhex(op_hex) + b"\x33" * 200)

    def test_decode_pav2(self):
        raw = self.envelope(FLOOR, TX, 6, "44" * 64)
        for form in (list(raw), raw.hex()):
            d = pb.decode_pav2(form)
            self.assertEqual(d, {"class": FLOOR, "bond": f"{TX}:6", "operator": "44" * 64})
        self.assertIsNone(pb.decode_pav2([]))
        self.assertIsNone(pb.decode_pav2(b"PBC1" + raw[4:]))
        self.assertIsNone(pb.decode_pav2(raw[:150]))
        h = pb.header_summary({"daaScore": 39, "powAlgoId": 6, "palwCommitment": list(raw)})
        self.assertEqual((h["daa"], h["algo"], h["bond"]), (39, 6, f"{TX}:6"))
        self.assertIsNone(pb.header_summary({"daaScore": 39, "powAlgoId": 8, "palwCommitment": []})["bond"])

    @staticmethod
    def chain(spec):
        chain = [(h, {"daa": d, "algo": a, "bond": b}) for h, d, a, b in spec]
        return chain, [d for _, d, _, _ in spec]

    def test_the_slot_rule(self):
        spec = [("h0", 0, 8, None), ("h1", 18, 6, G[1]), ("h2", 20, 8, None), ("h3", 20, 1, None), ("h4", 21, 6, G[6]),
                ("h5", 21, 9, G[1]), ("h6", 22, 6, G[1])]
        chain, daas = self.chain(spec)
        r = pb.resolve_anchor
        self.assertEqual(r({"acc": 0, "bound": None, "phase": "provisional"}, chain, daas, 20)["anchor"], "h4")   # slot 20: h2 heartbeat, h3 not a lane
        self.assertEqual(r({"acc": 0, "reb": 1, "bound": None}, chain, daas, 20)["anchor"], "h4")                 # the bind base moves the slot
        self.assertEqual(r({"acc": 2, "bound": 22}, chain, daas, 20)["check"], "ok")
        self.assertEqual(r({"acc": 3, "bound": None}, chain, daas, 20)["check"], "pending")
        self.assertEqual(r({"acc": 0, "bound": None}, chain[2:], daas[2:], 20)["check"], "below-range")
        self.assertEqual(r({"acc": 0, "bound": 21}, chain, daas, 20)["anchor"], "h4")
        self.assertEqual(r({"acc": 0, "bound": 22}, chain, daas, 20)["check"], "mismatch")                       # the slot rule reaches 21
        self.assertEqual(r({"acc": 0, "bound": None, "phase": "voided", "phaseDaa": 21}, chain, daas, 20)["check"], "unbound")
        self.assertEqual(r({"acc": 0, "bound": None, "phase": "voided", "phaseDaa": 30}, chain, daas, 20)["check"], "void-elsewhere")

    def test_a_redrawn_claim_is_checked_from_its_rebound_daa_and_its_first_panel_from_acceptance(self):
        spec = [("h0", 0, 8, None), ("h1", 20, 6, G[1]), ("h2", 45, 6, G[6]), ("h3", 71, 6, G[1])]
        chain, daas = self.chain(spec)
        first_panel = {"acc": 0, "reb": 50, "bound": 20}                   # redrawn at 50, still shows the panel it bound at 20
        self.assertEqual(pb.resolve_anchor(first_panel, chain, daas, 20)["anchor"], "h1")
        rebound = {"acc": 0, "reb": 50, "bound": 71}
        self.assertEqual(pb.resolve_anchor(rebound, chain, daas, 20)["anchor"], "h3")

    def test_a_no_capable_panel_retry_is_followed_only_past_the_resilience_fence(self):
        # accepted 0: slot 20 -> h1 (no capable panel, re-based on 20) -> slot 40 -> h2 @45 (again) -> slot 65 -> h3 @71: bound 71,
        # reboundDaa reset to None at the bind (rcore/f1-registry-resilience).
        spec = [("h0", 0, 8, None), ("h1", 20, 6, G[1]), ("h2", 45, 6, G[6]), ("h2b", 50, 6, G[6]), ("h3", 71, 6, G[1])]
        chain, daas = self.chain(spec)
        rec = {"acc": 0, "reb": None, "bound": 71}
        self.assertEqual(pb.resolve_anchor(rec, chain, daas, 20)["check"], "mismatch")                           # dormant: a broken rule
        res = pb.resolve_anchor(rec, chain, daas, 20, resilience_from=10)
        self.assertEqual((res["check"], res["anchor"], res["retries"]), ("ok", "h3", 2))
        self.assertEqual(pb.resolve_anchor(rec, chain, daas, 20, resilience_from=30)["check"], "mismatch")      # retried below the fence
        self.assertEqual(pb.resolve_anchor({"acc": 0, "reb": None, "bound": 50}, chain, daas, 20, resilience_from=10)["check"], "mismatch")
        redrawn = {"acc": 0, "reb": 30, "bound": 71}                          # a claim that held a panel never retries
        self.assertEqual(pb.resolve_anchor(redrawn, chain, daas, 20, resilience_from=10)["check"], "mismatch")

    def test_past_the_stopgap_only_genesis_attempts_anchor_and_a_bind_elsewhere_is_a_breach(self):
        spec = [("h0", 0, 8, None), ("h1", 20, 6, EXT), ("h2", 21, 6, G[6]), ("h3", 30, 6, EXT), ("h4", 30, 6, G[1]), ("h5", 40, 6, EXT)]
        chain, daas = self.chain(spec)
        admits = pb.anchor_rule(NAMES, stopgap_from=25)
        self.assertEqual(pb.resolve_anchor({"acc": 0, "bound": 20}, chain, daas, 20, admits)["anchor"], "h1")    # below the stopgap
        self.assertEqual(pb.resolve_anchor({"acc": 10, "bound": 30}, chain, daas, 20, admits)["anchor"], "h4")   # h3 is not admitted
        breach = pb.resolve_anchor({"acc": 20, "bound": 40}, chain, daas, 20, admits)
        self.assertEqual((breach["check"], breach["anchor"], breach["breach"]), ("ok", "h5", True))


class StateFile(unittest.TestCase):
    def test_round_trip_is_lossless_and_compact(self):
        st = pb.new_state()
        st["claims"] = synthetic_records(30, random.Random(6))
        next(iter(st["claims"].values()))["seats"][0] = EXT
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "s", "state.json")
            pb.save_state(path, st, TX)
            with open(path) as f:
                disk = json.load(f)
            self.assertNotIn(TX, json.dumps(disk["claims"]))
            self.assertEqual(json.dumps(pb.load_state(path)["claims"], sort_keys=True), json.dumps(st["claims"], sort_keys=True))

    def test_a_v1_state_drops_its_bucket_keyed_checkpoints(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "state.json")
            with open(path, "w") as f:
                json.dump({"version": 1, "claims": {}, "checkpoints": [[25, "ab"]], "alerted": {"external_seat": ["x"], "external_anchor": []}}, f)
            st = pb.load_state(path)
        self.assertEqual((st["version"], st["checkpoints"]), (pb.STATE_VERSION, []))
        self.assertEqual(st["alerted"], {"external_seat_bonds": [], "external_anchor": []})

    def test_prune_forgets_only_old_claims_the_node_no_longer_shows(self):
        st = pb.new_state()
        st["claims"] = synthetic_records(30, random.Random(6))
        ids = list(st["claims"])
        for cid in ids[:20]:
            st["claims"][cid]["gone"] = True
        self.assertEqual(pb.prune_state(st, 45, 20), 5)        # anchorDaa 20..24 are below 25 and retired
        self.assertEqual(len(st["claims"]), 25)
        for cid in ids[20:]:
            st["claims"][cid]["last"] = 7                       # seen this run (now = 7): kept however old
        self.assertEqual(pb.prune_state(st, 1000, 20, now=7), 15)
        self.assertEqual(sorted(st["claims"]), sorted(ids[20:]))


class FakeNode:
    """A canned JSON wRPC node: a selected chain of 2 blocks per DAA (every third an attempt block
    by g1 or g6), floor claims by g1/g6 whose panels bind at their anchor. Read calls only.
    vc_batch caps getVirtualChainFromBlock's added hashes as the real node does (10 x
    mergeset_size_limit); claims_cap caps getPalwClaims at its newest rows as the real node does
    (500); retry = claim indices that NoCapablePanel-retry once before they bind."""

    def __init__(self, n_blocks, claims_at, rng, bad_bound=None, vc_batch=None, claims_cap=None, retry=()):
        self.calls, self.starts = 0, []
        self.vc_batch, self.claims_cap, self.rng = vc_batch, claims_cap, rng
        self.chain = []
        self.extend(n_blocks)
        self.claims = []
        for n, acc in enumerate(claims_at):
            self.add_claim(acc, retry=n in retry, bad=bad_bound is not None and n == bad_bound)

    def extend(self, n_blocks):
        for i in range(len(self.chain), n_blocks):
            h = f"{i:08x}" + "b" * 120
            algo = 6 if i % 3 == 0 else 8
            env = ChainFacts.envelope(FLOOR, TX, 1 if i % 2 else 6, "55" * 64) if algo == 6 else b""
            self.chain.append({"hash": h, "daaScore": i // 2, "powAlgoId": algo, "palwCommitment": list(env)})

    def anchor_at(self, slot):
        return next((b for b in self.chain if b["daaScore"] >= slot and b["powAlgoId"] == 6), None)

    def add_claim(self, acc, retry=False, bad=False, exe=None):
        n = len(self.claims)
        exe = exe or (G[1] if n % 2 else G[6])
        anchor = self.anchor_at(acc + 20)
        if anchor is not None and retry:
            anchor = self.anchor_at(anchor["daaScore"] + 20)
        seats = race({b: W_GENESIS for b in G if b != exe}, 5, self.rng) if anchor else []
        bound = anchor["daaScore"] if anchor else None
        if bad:
            bound += 1
        c = {"claimId": f"{n:04x}" + "d" * 124, "classId": FLOOR, "executorBond": exe, "isFreePrompt": False,
             "phase": "panel_bound" if anchor else "provisional", "voidReason": "", "acceptedDaa": acc,
             "acceptedBlock": "e" * 128, "reboundDaa": None, "boundDaa": bound, "phaseDaa": bound or 0, "seats": seats}
        self.claims.append(c)
        return c

    def call(self, method, params=None):
        self.calls += 1
        params = params or {}
        tip = self.chain[-1]["daaScore"]
        if method == "getPalwNodeStatus":
            return {"genesisHash": pb.T12_GENESIS, "consensusParamsId": pb.T12_FP}
        if method == "getBlockDagInfo":
            return {"virtualDaaScore": tip, "sink": self.chain[-1]["hash"], "network": "testnet-12",
                    "pruningPointHash": self.chain[0]["hash"]}
        if method == "getPalwModelRegistry":
            return {"classes": [{"classId": FLOOR, "isBaseClass": True}]}
        if method == "getPalwPanelSeats":
            return {"seats": []}
        if method == "getPalwClaims":
            b, role = params["bond"], params["role"]
            rows = [c for c in self.claims if (c["executorBond"] == b if role == "executor" else b in c["seats"])]
            rows.sort(key=lambda c: (-c["acceptedDaa"], c["claimId"]))
            truncated = self.claims_cap is not None and len(rows) > self.claims_cap
            if truncated:
                rows = rows[:self.claims_cap]
            return {"bondKnown": b in G, "bondCollateral": 93_906_321_001_040 if b in G else 0, "bondRegisteredDaa": 0,
                    "bondRetiringSinceDaa": None, "bondCapableClasses": [FLOOR] if b in G else [], "claims": rows, "truncated": truncated}
        if method == "getPalwPanelAssignments":
            return {"assignments": [{"claimId": c["claimId"], "licensedState": "panelBound", "fullSeat": c["seats"][0],
                                     "seats": [{"seatId": x, "receiptStatus": "pending"} for x in c["seats"]]}
                                    for c in self.claims if c["seats"]], "truncated": False}
        hashes = [b["hash"] for b in self.chain]
        if method == "getVirtualChainFromBlock":
            self.starts.append(params["startHash"])
            added = hashes[hashes.index(params["startHash"]) + 1:]
            return {"addedChainBlockHashes": added[:self.vc_batch] if self.vc_batch else added, "removedChainBlockHashes": []}
        if method == "getBlocks":
            i = hashes.index(params["lowHash"])
            page = self.chain[i:i + 50]
            return {"blockHashes": [b["hash"] for b in page], "blocks": [{"header": b, "verboseData": {"isChainBlock": True}} for b in page]}
        raise AssertionError(method)


def collect_cfg(*extra):
    cfg = pb.parse_args(["--state", "", *extra])
    cfg.names = NAMES
    return cfg


def run_main(node, path, *extra):
    """pb.main() against a fake node: (exit code, summary dict)."""
    orig = pb.WsRpc

    class Wire:
        def __init__(self, *a, **k):
            self.calls = 0

        def call(self, m, p=None):
            self.calls += 1
            return node.call(m, p)

        def close(self):
            pass
    pb.WsRpc = Wire
    try:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = pb.main(["--state", path, *extra])
    finally:
        pb.WsRpc = orig
    text = out.getvalue()
    return code, json.loads(text.strip().splitlines()[-1][len("PANEL_BIAS "):]), text


class Collect(unittest.TestCase):
    def test_anchors_populations_and_checkpoints_from_a_fake_node(self):
        node = FakeNode(160, [2, 5, 9, 14, 20, 33, 70], random.Random(21))
        st = pb.new_state()
        run = pb.collect(node, st, collect_cfg(), 1)
        self.assertEqual(run["degraded"], [])
        self.assertEqual(run["new_claims"], 7)
        recs = st["claims"]
        bound = [r for r in recs.values() if r.get("seats")]
        self.assertEqual(len(bound), 6)                                  # the DAA-70 claim's slot (90) is past the tip (79)
        for r in bound:
            self.assertEqual(r["anchorCheck"], "ok")
            self.assertEqual(r["anchorDaa"], r["bound"])
            self.assertIn(r["anchorBond"], (G[1], G[6]))
            self.assertEqual(len(r["pop"]), 7)
            self.assertNotIn(r["exe"], r["pop"])
            self.assertFalse(r["popUncertain"])
            self.assertEqual(set(r["verdicts"].values()), {"pending"})
        self.assertEqual([r["anchorCheck"] for r in recs.values() if not r.get("seats")], ["pending"])
        self.assertEqual(node.starts[0], node.chain[0]["hash"])
        self.assertTrue(st["checkpoints"])
        for d, h in st["checkpoints"]:                                     # kept under the block's own DAA
            self.assertEqual(next(b["daaScore"] for b in node.chain if b["hash"] == h), d)
        # The next run starts from a checkpoint below the lowest slot still open (the DAA-70 claim's, 90),
        # not from the pruning point: the claims resolved 'ok' long ago do not hold the scan back.
        node.extend(240)
        node.claims[-1]["boundDaa"] = node.anchor_at(90)["daaScore"]
        node.claims[-1]["seats"] = race({b: W_GENESIS for b in G if b != G[6]}, 5, random.Random(2))
        node.claims[-1]["phase"] = "panel_bound"
        pb.collect(node, st, collect_cfg(), 2)
        start = next(b for b in node.chain if b["hash"] == node.starts[-2])
        self.assertEqual(start["daaScore"], 75)
        self.assertTrue(all(r["anchorCheck"] == "ok" for r in st["claims"].values() if r.get("seats")))

    def test_the_chain_is_paged_to_the_sink_and_a_long_lived_claim_does_not_blind_it(self):
        """The real node answers at most 1,800 added chain blocks per call; scaled here to 60."""
        node = FakeNode(400, [2] + list(range(60, 170, 3)), random.Random(7), vc_batch=60)
        st = pb.new_state()
        for n in range(4):
            run = pb.collect(node, st, collect_cfg(), n + 1)
            self.assertEqual(run["degraded"], [], run["degraded"])
        checks = [r["anchorCheck"] for r in st["claims"].values() if r.get("seats")]
        self.assertEqual(len(checks), 38)
        self.assertEqual(set(checks), {"ok"})
        self.assertGreater(run["chain_pages"], 1)

    def test_a_chain_read_short_of_the_sink_is_degraded(self):
        node = FakeNode(400, [2, 150], random.Random(7), vc_batch=60)
        run = pb.collect(node, pb.new_state(), collect_cfg("--max-chain-pages", "2"), 1)
        self.assertTrue(any("short of the sink" in d for d in run["degraded"]), run["degraded"])
        self.assertTrue(any("past the chain read" in d for d in run["degraded"]), run["degraded"])

    def test_capped_claim_lists_are_normal_and_only_a_gap_degrades(self):
        node = FakeNode(160, [2, 5, 9, 14, 20, 23, 26, 29, 33, 36, 40, 45, 50], random.Random(3), claims_cap=5)
        st = pb.new_state()
        first = pb.collect(node, st, collect_cfg(), 1)
        self.assertEqual(first["degraded"], [])
        self.assertTrue(first["truncated_lists"])
        # an hour later: the newest rows still reach back past the last run; older claims are not 'gone'
        node.extend(170)
        second = pb.collect(node, st, collect_cfg(), 2)
        self.assertEqual(second["degraded"], [])
        self.assertFalse(any(r.get("gone") for r in st["claims"].values()))
        # a long outage: twenty new claims per executor push the newest rows past the last run's reach
        node.extend(400)
        for acc in range(120, 180, 3):
            node.add_claim(acc)
        third = pb.collect(node, st, collect_cfg(), 3)
        self.assertTrue(any("claims in between may be missing" in d for d in third["degraded"]), third["degraded"])

    def test_a_retired_claim_is_gone_when_its_list_is_read_whole(self):
        node = FakeNode(160, [2, 5, 9], random.Random(3))
        st = pb.new_state()
        pb.collect(node, st, collect_cfg(), 1)
        retired = node.claims.pop(0)
        pb.collect(node, st, collect_cfg(), 2)
        self.assertTrue(st["claims"][retired["claimId"]]["gone"])
        self.assertFalse(any(r.get("gone") for cid, r in st["claims"].items() if cid != retired["claimId"]))

    def test_a_no_capable_panel_retry_composes_with_the_resilience_fence(self):
        node = FakeNode(200, [2, 5, 9], random.Random(4), retry={0})
        st = pb.new_state()
        run = pb.collect(node, st, collect_cfg(), 1)
        self.assertTrue(any("slot rule does not lead" in d for d in run["degraded"]))    # dormant fence: a broken rule
        st = pb.new_state()
        run = pb.collect(node, st, collect_cfg("--fence-daa", "0"), 1)
        self.assertEqual(run["degraded"], [])
        rec = st["claims"][node.claims[0]["claimId"]]
        self.assertEqual((rec["anchorCheck"], rec["retries"], rec["anchorDaa"]), ("ok", 1, node.claims[0]["boundDaa"]))

    def test_a_rebound_claim_keeps_its_first_panel(self):
        node = FakeNode(200, [2, 5], random.Random(4))
        st = pb.new_state()
        pb.collect(node, st, collect_cfg(), 1)
        c = node.claims[0]
        first_bound, first_seats = c["boundDaa"], list(c["seats"])
        c["reboundDaa"] = first_bound + 30                         # the receipt deadline passed: redrawn
        pb.collect(node, st, collect_cfg(), 2)
        self.assertEqual(st["claims"][c["claimId"]]["anchorDaa"], first_bound)   # before it rebinds it shows its first panel
        c["boundDaa"] = node.anchor_at(c["reboundDaa"] + 20)["daaScore"]
        c["seats"] = race({b: W_GENESIS for b in G if b != c["executorBond"]}, 5, random.Random(9))
        pb.collect(node, st, collect_cfg(), 3)
        arch = st["claims"][f"{c['claimId']}#{first_bound}"]
        self.assertEqual((arch["seats"], arch["anchorDaa"], arch["anchorCheck"], arch["gone"]), (first_seats, first_bound, "ok", True))
        rec = st["claims"][c["claimId"]]
        self.assertEqual((rec["anchorCheck"], rec["anchorDaa"], rec["seats"]), ("ok", c["boundDaa"], c["seats"]))
        self.assertIn("pop", rec)

    def test_a_bound_daa_the_anchor_rule_does_not_give_is_a_mismatch(self):
        node = FakeNode(120, [2, 5, 9], random.Random(22), bad_bound=1)
        st = pb.new_state()
        run = pb.collect(node, st, collect_cfg(), 1)
        checks = sorted(r["anchorCheck"] for r in st["claims"].values())
        self.assertEqual(checks, ["mismatch", "ok", "ok"])
        self.assertTrue(any("slot rule does not lead" in d for d in run["degraded"]))
        self.assertEqual(pb.analyse(st["claims"], NAMES)["claims_used"], 2)

    def test_an_anchor_whose_producer_does_not_decode_is_degraded(self):
        node = FakeNode(160, [2, 5, 9], random.Random(8))
        for b in node.chain:
            if b["powAlgoId"] == 6:
                b["palwCommitment"] = list(b"PAV2") + [0] * 10
        run = pb.collect(node, pb.new_state(), collect_cfg(), 1)
        self.assertTrue(any("does not decode" in d for d in run["degraded"]), run["degraded"])


class Runs(unittest.TestCase):
    def run_offline(self, st, *extra):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "state.json")
            pb.save_state(path, st, TX)
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                code = pb.main(["--offline", "--state", path, *extra])
            return code, out.getvalue()

    def test_clean_state_exits_0_with_one_summary_line(self):
        st = pb.new_state()
        st["claims"] = synthetic_records(80, random.Random(12))
        code, text = self.run_offline(st)
        self.assertEqual(code, 0, text)
        last = text.strip().splitlines()[-1]
        self.assertTrue(last.startswith("PANEL_BIAS {"))
        summary = json.loads(last[len("PANEL_BIAS "):])
        self.assertEqual((summary["status"], summary["tested"], summary["ext_seat"]), ("OK", 80, 0))
        self.assertIn("against the OTHER anchor producers", text)

    def test_alerts_exit_1(self):
        st = pb.new_state()
        st["claims"] = synthetic_records(300, random.Random(5), grind=(G[6], {G[2], G[3]}))
        code, text = self.run_offline(st, "--quiet")
        self.assertEqual(code, 1)
        self.assertEqual(len(text.strip().splitlines()), 1)
        summary = json.loads(text.strip()[len("PANEL_BIAS "):])
        self.assertEqual(summary["status"], "ALERT")
        self.assertTrue(summary["alerts"][0].startswith("bias: g6 seats g2 OVER") or summary["alerts"][0].startswith("bias: g6 seats g3 OVER"),
                        summary["alerts"])

    def test_a_population_wide_effect_exits_0(self):
        rng = random.Random(11)
        st = pb.new_state()
        for n in range(1500):
            st["claims"].update(synthetic_records(1, rng, anchors=(G[1],) if rng.random() < 0.79 else (G[6],), start=n, drop=(G[2], 0.25)))
        code, text = self.run_offline(st)
        summary = json.loads(text.strip().splitlines()[-1][len("PANEL_BIAS "):])
        self.assertEqual((code, summary["status"], summary["alerts"]), (0, "OK", []))
        self.assertGreater(summary["model_notes"], 0)
        self.assertIn("population note: g2", text)

    def test_a_legitimate_external_bond_alerts_once_then_runs_are_ok(self):
        node = FakeNode(160, [2, 5, 9, 14, 20, 33], random.Random(21))
        node.claims[1]["seats"][0] = EXT
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "state.json")
            code, summary, _ = run_main(node, path, "--quiet")
            self.assertEqual((code, summary["ext_seat_bonds"]), (1, 1))
            self.assertTrue(summary["alerts"][0].startswith("external seat: external"), summary["alerts"])
            node.extend(170)
            node.claims[2]["seats"][0] = EXT                     # the same bond on another panel: no new alert
            code, summary, _ = run_main(node, path, "--quiet")
            self.assertEqual((code, summary["status"], summary["ext_seat"]), (0, "OK", 2))

    def test_an_external_anchor_alerts_only_past_the_stopgap(self):
        node = FakeNode(160, [2, 5, 9, 14, 20, 33], random.Random(21))
        for b in node.chain:
            if b["powAlgoId"] == 6 and b["daaScore"] in (22, 23):
                b["palwCommitment"] = list(ChainFacts.envelope(FLOOR, "ab" * 64, 0, "55" * 64))
        with tempfile.TemporaryDirectory() as d:
            code, summary, _ = run_main(node, os.path.join(d, "a.json"), "--quiet")
            self.assertEqual((code, summary["ext_anchor"], summary["ext_anchor_post_stopgap"]), (0, 1, 0))
            code, summary, _ = run_main(node, os.path.join(d, "b.json"), "--quiet", "--stopgap-from", "10")
            # the stopgap does not admit the external attempt, so the claim that bound there is a breach
            self.assertEqual((code, summary["ext_anchor_post_stopgap"]), (1, 1))
            self.assertTrue(any(a.startswith("external anchor past the stopgap") for a in summary["alerts"]), summary["alerts"])


if __name__ == "__main__":
    unittest.main()
