#!/usr/bin/env python3
"""Unit tests for panel_bias.py: the statistics on synthetic data, the anchor rule, the header
decoding and the state file. Stdlib only; no network.

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


def synthetic_records(n_claims, rng, grind=None, anchors=(G[1], G[6]), executors=(G[1], G[6]), k=5, ext_weight=None):
    """Claims of the floor class drawn fairly from the 8 genesis bonds minus the executor. With
    grind = (anchor, {bonds}), that anchor producer re-rolls each of its panels until it seats
    every bond in the set (what a grinding anchor producer does for its Sybils)."""
    recs = {}
    for n in range(n_claims):
        exe = rng.choice(executors)
        anchor = rng.choice(anchors)
        pop = {b: W_GENESIS for b in G if b != exe}
        if ext_weight:
            pop[EXT] = ext_weight
        seats = race(pop, k, rng)
        if grind and anchor == grind[0]:
            while not grind[1] <= set(seats):
                seats = race(pop, k, rng)
        cid = f"{n:04x}" + "c" * 124
        recs[cid] = {"cls": FLOOR, "exe": exe, "phase": "receipt_licensed", "void": "", "acc": n, "reb": None,
                     "bound": n + 20, "seats": seats, "anchor": f"{n:04x}" + "a" * 124, "anchorDaa": n + 20,
                     "anchorBond": anchor, "anchorAlgo": 6, "anchorCheck": "ok", "pop": dict(pop)}
    return recs


def any_alert(res):
    return any(c.get("significant") for c in res["cells"] if c["anchor"] != "*") or any(o.get("significant") for o in res["omnibus"])


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


class RaoScott(unittest.TestCase):
    def test_null_calibration_and_why_the_correction_is_needed(self):
        """Fair draws of 5 of 7 (the floor under a genesis executor): the corrected test rejects
        at about its nominal rate; the uncorrected Pearson X2 almost never does (draws without
        replacement vary much less than Poisson counts)."""
        rng = random.Random(2026)
        reps, rs_rej, pearson_rej = 300, 0, 0
        for _ in range(reps):
            recs = synthetic_records(150, rng, anchors=(G[1],))
            res = pb.analyse(recs, NAMES, alpha=0.05)
            o = res["omnibus"][0]
            self.assertTrue(o["evaluated"])
            rs_rej += o["p"] <= 0.05
            pearson_rej += pb.chi2_sf(o["x2"], o["bonds"] - 1) <= 0.05
        self.assertTrue(0.02 <= rs_rej / reps <= 0.09, rs_rej / reps)
        self.assertLess(pearson_rej / reps, 0.01)


class Analyse(unittest.TestCase):
    def test_fair_draws_raise_nothing(self):
        res = pb.analyse(synthetic_records(600, random.Random(11)), NAMES, alpha=1e-4)
        self.assertEqual(res["claims_used"], 600)
        self.assertFalse(any_alert(res))
        self.assertEqual(sorted(res["anchors"]), sorted([G[1], G[6]]))
        for c in res["cells"]:
            if c["anchor"] != "*" and c["seat"] == c["anchor"]:
                self.assertGreater(c["exp"], 0)   # an anchor producer can sit on panels it anchors

    def test_executor_never_expected_on_its_own_panel(self):
        recs = synthetic_records(200, random.Random(3), executors=(G[1],))
        res = pb.analyse(recs, NAMES)
        g1 = [c for c in res["cells"] if c["seat"] == G[1]]
        self.assertTrue(all(c["obs"] == 0 for c in g1))
        self.assertFalse(any(c["exp"] > 0 for c in g1))

    def test_a_grinding_anchor_producer_is_caught(self):
        recs = synthetic_records(300, random.Random(5), grind=(G[6], {G[2], G[3]}))
        res = pb.analyse(recs, NAMES, alpha=1e-4)
        sig = {(c["anchor"], c["seat"]) for c in res["cells"] if c["anchor"] != "*" and c.get("significant")}
        self.assertIn((G[6], G[2]), sig)
        self.assertIn((G[6], G[3]), sig)
        self.assertFalse(any(a == G[1] for a, _ in sig), sig)
        omni = {o["anchor"]: o for o in res["omnibus"]}
        self.assertTrue(omni[G[6]]["significant"])
        self.assertFalse(omni[G[1]]["significant"])

    def test_family_wise_false_alarm_rate_is_bounded(self):
        """alpha = 0.05 per family, two families: a fair world alarms in at most ~2 * alpha of runs."""
        rng = random.Random(99)
        runs, alarms = 150, 0
        for _ in range(runs):
            alarms += any_alert(pb.analyse(synthetic_records(120, rng), NAMES, alpha=0.05))
        self.assertLessEqual(alarms / runs, 0.14, alarms / runs)

    def test_stake_weights_move_the_expectation(self):
        recs = synthetic_records(400, random.Random(8), ext_weight=130_000)
        res = pb.analyse(recs, NAMES, alpha=1e-4)
        ext = [c for c in res["cells"] if c["anchor"] == "*" and c["seat"] == EXT][0]
        gen = [c for c in res["cells"] if c["anchor"] == "*" and c["seat"] == G[3]][0]
        self.assertLess(ext["exp"] / ext["n"], gen["exp"] / gen["n"])
        self.assertFalse(any_alert(res))

    def test_window_and_unusable_records_are_skipped(self):
        recs = synthetic_records(50, random.Random(1))
        first = next(iter(recs))
        recs[first]["anchorCheck"] = "mismatch"
        self.assertEqual(pb.analyse(recs, NAMES)["claims_used"], 49)
        self.assertEqual(pb.analyse(recs, NAMES, window_from=60)["claims_used"], 10)


class ExternalAlerts(unittest.TestCase):
    def test_external_seat_and_anchor_are_alerted_once(self):
        recs = synthetic_records(20, random.Random(4))
        a, b = list(recs)[:2]
        recs[a]["seats"][0] = EXT
        recs[b]["anchorBond"] = EXT
        seat, anchor, new_seat, new_anchor = pb.find_external(recs, NAMES, {"external_seat": [], "external_anchor": []}, False)
        self.assertEqual((seat, anchor, new_seat, new_anchor), ([a], [b], [a], [b]))
        _, _, again_seat, again_anchor = pb.find_external(recs, NAMES, {"external_seat": [a], "external_anchor": [b]}, False)
        self.assertEqual((again_seat, again_anchor), ([], []))
        self.assertEqual(pb.find_external(recs, NAMES, {"external_seat": [a], "external_anchor": [b]}, True)[2], [a])


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

    def test_anchor_is_the_first_attempt_chain_block_at_or_past_the_slot(self):
        spec = [("h0", 0, 8), ("h1", 18, 6), ("h2", 20, 8), ("h3", 20, 1), ("h4", 21, 6), ("h5", 21, 9), ("h6", 22, 6)]
        chain = [(h, {"daa": d, "algo": a, "bond": f"{TX}:{i % 8}"}) for i, (h, d, a) in enumerate(spec)]
        daas = [d for _, d, _ in spec]
        self.assertEqual(pb.resolve_anchor({"acc": 0, "reb": None}, chain, daas, 20)[0], "h4")    # slot 20: h2 heartbeat, h3 not a lane
        self.assertEqual(pb.resolve_anchor({"acc": 0, "reb": 1}, chain, daas, 20)[0], "h4")       # the redraw re-bases the slot
        self.assertEqual(pb.resolve_anchor({"acc": 2, "reb": None}, chain, daas, 20)[0], "h6")
        self.assertEqual(pb.resolve_anchor({"acc": 3, "reb": None}, chain, daas, 20), (None, "pending"))
        self.assertEqual(pb.resolve_anchor({"acc": 0, "reb": None}, chain[2:], daas[2:], 20), (None, "below-range"))


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

    def test_prune_forgets_only_old_retired_claims(self):
        st = pb.new_state()
        st["claims"] = synthetic_records(30, random.Random(6))
        ids = list(st["claims"])
        for cid in ids[:20]:
            st["claims"][cid]["gone"] = True
        self.assertEqual(pb.prune_state(st, 45, 20), 5)        # anchorDaa 20..24 are below 25 and retired
        self.assertEqual(len(st["claims"]), 25)


class OfflineRun(unittest.TestCase):
    def run_main(self, st, *extra):
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
        code, text = self.run_main(st)
        self.assertEqual(code, 0, text)
        last = text.strip().splitlines()[-1]
        self.assertTrue(last.startswith("PANEL_BIAS {"))
        summary = json.loads(last[len("PANEL_BIAS "):])
        self.assertEqual((summary["status"], summary["tested"], summary["ext_seat"]), ("OK", 80, 0))
        self.assertIn("Seat draws per anchor producer", text)

    def test_alerts_exit_1(self):
        st = pb.new_state()
        st["claims"] = synthetic_records(300, random.Random(5), grind=(G[6], {G[2], G[3]}))
        code, text = self.run_main(st, "--quiet")
        self.assertEqual(code, 1)
        self.assertEqual(len(text.strip().splitlines()), 1)
        summary = json.loads(text.strip()[len("PANEL_BIAS "):])
        self.assertEqual(summary["status"], "ALERT")
        self.assertTrue(any("g6 anchors seat g2 over" in a for a in summary["alerts"]), summary["alerts"])


if __name__ == "__main__":
    unittest.main()
