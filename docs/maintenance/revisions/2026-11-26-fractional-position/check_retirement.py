#!/usr/bin/env python3
import argparse
import io
import json
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
import test_retirement_reference as tests


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("retirement-check.json"))
    args = parser.parse_args()
    stream = io.StringIO()
    result = unittest.TextTestRunner(stream=stream, verbosity=2).run(unittest.defaultTestLoader.loadTestsFromModule(tests))
    report = {
        "status": "PASS" if result.wasSuccessful() else "FAIL",
        "scope": "abstract frozen role/funding-lot retirement and cumulative Court 49-percent reward escrow / 51-percent basic burn",
        "tests": {"run": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
                  "skipped": len(result.skipped)},
        "measurements": tests.MEASUREMENTS,
        "native_bft_or_court_authorization_verified": False,
        "runtime_or_network_activation_tested": False,
        "limitations": [
            "The new cumulative 49-percent recovery reward pool and remaining 51-percent basic burn are tested; individual responsibility, evidence and judgment rules are not selected or implemented here.",
            "Verified BFT/Court decisions are explicit trusted inputs; signatures, quorum, appeal and proof of individual wrongdoing are not implemented.",
            "Funding source IDs and nonoverlapping ranges stand for authenticated native assets; their real chain provenance is not established here.",
            "Local atomicity and ordered undo/replay are tested, not DAG finality, persistence, RPC, EVM, pruning or IBD.",
            "Earned verification and BFT reward reservations are preserved; actual service qualification and payment routing are outside this model.",
            "Court recovery, eligible-recipient resolution, per-collection delta reward claims and expiry are separate reference phases; native authorization is not verified.",
            "The illustrative claim allocator uses explicit recipient order and weights; it does not establish or replace a native mandatory internal Panel/Gate-C split.",
            "Each reference offence binds one collateral lot; authentic multi-lot recovery and time/appeal policy require native integration.",
            "The model never mints cancellation funds and grants no BFT confiscation payout or bounty.",
        ],
        "test_transcript": stream.getvalue(),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(f"{report['status']}: {result.testsRun} retirement tests; {len(result.failures)} failures, {len(result.errors)} errors")
    if not result.wasSuccessful():
        print(stream.getvalue())
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
