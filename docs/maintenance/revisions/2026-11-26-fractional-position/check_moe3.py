#!/usr/bin/env python3
"""Run MOE3 reference checks and write an auditable JSON result, not native status."""
import argparse
import io
import json
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
import moe3_reference as reference
import test_moe3_reference as tests


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("math-check.json"))
    args = parser.parse_args()
    suite = unittest.defaultTestLoader.loadTestsFromModule(tests)
    transcript = io.StringIO()
    result = unittest.TextTestRunner(stream=transcript, verbosity=2).run(suite)
    report = {
        "status": "PASS" if result.wasSuccessful() else "FAIL",
        "scope": "MOE3 50/20/20/5/5 arithmetic, coupled allocation, decimal codec and abstract allocation/market journal",
        "profile": reference.PROFILE,
        "position_decimals": reference.POSITION_DECIMALS,
        "position_scale_atomic_string": str(reference.SCALE),
        "nominal_permille": {"miner_position_cost": 500, "lp_position_cost": 200, "lp_misaka": 200,
                             "panel_gate_c_misaka_total": 50, "bft_misaka": 50},
        "bounded_amount_bits": 128,
        "bounded_two_factor_product_bits": 256,
        "native_runtime_tested": False,
        "network_activation_tested": False,
        "unittests": {"run": result.testsRun, "failures": len(result.failures),
                      "errors": len(result.errors), "skipped": len(result.skipped)},
        "measurements": tests.MEASUREMENTS,
        "limitations": [
            "Finite probes supplement the elementary identities in MATH.md; they are not an all-input machine proof.",
            "The journal checks separate P/F reservation and local market atomicity/undo, not DAG consensus, persistence, IBD or native/EVM routing.",
            "No work qualification, issuance entitlement, panel/court funding, export cap, maturity or deadline is bypassed or verified here.",
            "No BPS price-limit evaluator is implemented; three-factor comparisons may need checked u512 or exact comparison decomposition.",
            "Uniform coefficient 1 and no origin branch are reference interfaces; native selection/cap removal is not tested.",
            "Coupled global budget apportionment is tested separately; the settlement journal is single-model and no native multi-market transaction is implemented.",
            "Migration is an abstract one-time book. Old wire bytes, allowances, pending claims and authenticated activation snapshots require native integration.",
            "LiquidityDust still exists at atom precision; no all-state liveness, future price, MEV or profitability guarantee is made.",
        ],
        "test_transcript": transcript.getvalue(),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(f"{report['status']}: {result.testsRun} tests; "
          f"{len(result.failures)} failures, {len(result.errors)} errors")
    print(json.dumps(report["measurements"].get("exhaustive", {}), ensure_ascii=False))
    print(json.dumps(report["measurements"].get("seeded_random", {}), ensure_ascii=False))
    if not result.wasSuccessful():
        print(transcript.getvalue())
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
