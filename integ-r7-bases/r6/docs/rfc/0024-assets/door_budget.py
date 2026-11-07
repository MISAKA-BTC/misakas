#!/usr/bin/env python3
"""Arithmetic companion for RFC-DRAFT-DOOR3200; never an arming authorization.

Uses only the Python standard library. The tool does not implement ADR-0191,
measure a node, authenticate evidence, or prove the resource/scheduling bounds.
Its path terms must be non-overlapping, conservative bounds derived elsewhere.
"""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
import re
import sys
import unittest
from typing import Any

MAX_U64 = (1 << 64) - 1
REQUIRED_PROFILES = {"32k": 32_768, "2m": 2_097_152}
REQUIRED_PATHS = (
    "honest_cold_verifier",
    "latest_legal_responses",
    "withholding_at_last_obligation",
    "restart_within_envelope",
    "reorg_reacquire_within_envelope",
    "admitted_load_and_owner_contention",
    "other_covered_fault_routes",
)
TERMS = (
    "preparation_daa",
    "open_and_initial_localization_daa",
    "history_bundle_disclosure_and_choice_daa",
    "forced_material_acquisition_daa",
    "verifier_replay_and_unallocated_work_daa",
    "terminal_evaluation_and_inclusion_daa",
    "additional_recovery_daa",
    "unallocated_queue_and_safety_margin_daa",
)
NOTICE = (
    "Arithmetic only: no node measurement, evidence authentication, security proof, "
    "coverage certification, formal ADR-0191 derivation, or arming authorization."
)


def natural(value: Any, name: str, *, positive: bool = False) -> int:
    if type(value) is not int or value < int(positive) or value > MAX_U64:
        raise ValueError(f"{name}: expected {'positive' if positive else 'nonnegative'} u64 integer")
    return value


def ceil_div(a: int, b: int) -> int:
    if a < 0 or b <= 0:
        raise ValueError("ceil_div requires a >= 0 and b > 0")
    return (a + b - 1) // b


def kary_rounds(space: int, arity: int = 16) -> int:
    natural(space, "space")
    if arity != 16:
        raise ValueError("This RFC companion supports the proposed arity=16 only")
    count, rounds = space, 0
    while count > 1:
        count = ceil_div(count, arity)
        rounds += 1
    return rounds


def geometry(history_positions: int, tile_positions: int = 16) -> dict[str, int]:
    natural(history_positions, "history_positions", positive=True)
    natural(tile_positions, "tile_positions", positive=True)
    if tile_positions > 16:
        raise ValueError("A's one-extra-logical-level proposal requires tile_positions <= 16")
    tiles = ceil_div(history_positions, tile_positions)
    tile_rounds = kary_rounds(tiles)
    position_rounds = kary_rounds(min(history_positions, tile_positions))
    logical = tile_rounds + position_rounds
    return {
        "history_positions": history_positions,
        "tile_positions": tile_positions,
        "tile_count": tiles,
        "legacy_tile_logical_levels": tile_rounds,
        "additional_position_logical_levels_upper": position_rounds,
        "single_position_total_logical_levels_upper": logical,
        "depth2_response_phases_upper": ceil_div(logical, 2),
        "note_is_not_daa": 1,
    }


def required_saving(baseline_no_terminal: int, new_additions: int, target: int) -> int:
    for value, name in ((baseline_no_terminal, "baseline"), (new_additions, "new_additions"), (target, "target")):
        natural(value, name)
    return max(0, baseline_no_terminal + new_additions - target)


def _hash_present(value: Any, size: int) -> bool:
    return isinstance(value, str) and re.fullmatch(rf"[0-9a-f]{{{size}}}", value) is not None and set(value) != {"0"}


def check_budget(document: Any) -> tuple[dict[str, Any], int]:
    if not isinstance(document, dict) or document.get("schema_version") != 1:
        raise ValueError("Expected an object with schema_version=1")
    target = natural(document.get("target_daa"), "target_daa", positive=True)
    if target not in (3_120, 3_200):
        raise ValueError("This companion accepts target_daa=3120 or 3200; approval is external")
    profiles = document.get("profiles")
    if not isinstance(profiles, list):
        raise ValueError("profiles must be a list")
    found: set[str] = set()
    incomplete: list[str] = []
    overruns: list[str] = []
    output_profiles: list[dict[str, Any]] = []
    pin = document.get("pin_inputs")
    if not isinstance(pin, dict):
        raise ValueError("pin_inputs must be an object, with null values while unknown")
    for name, size in (("source_commit", 40), ("binary_sha256", 64), ("cost_record_sha256", 64)):
        value = pin.get(name)
        if value is None:
            incomplete.append(f"pin_inputs.{name} UNKNOWN")
        elif not _hash_present(value, size):
            raise ValueError(f"pin_inputs.{name}: invalid/nonzero lowercase hex digest")

    for profile in profiles:
        if not isinstance(profile, dict):
            raise ValueError("Each profile must be an object")
        name = profile.get("profile_id")
        if name not in REQUIRED_PROFILES or name in found:
            raise ValueError(f"Unknown or repeated profile_id: {name!r}")
        found.add(name)
        h = natural(profile.get("history_positions"), f"{name}.history_positions", positive=True)
        if h != REQUIRED_PROFILES[name]:
            raise ValueError(f"{name}: history_positions does not match the RFC's binary K/M definition")
        paths = profile.get("paths")
        if not isinstance(paths, list):
            raise ValueError(f"{name}.paths must be a list")
        seen: set[str] = set()
        results: list[dict[str, Any]] = []
        sums: list[int] = []
        for path in paths:
            if not isinstance(path, dict):
                raise ValueError("Each path must be an object")
            pn = path.get("path_id")
            if pn not in REQUIRED_PATHS or pn in seen:
                raise ValueError(f"{name}: unknown/repeated path_id {pn!r}")
            seen.add(pn)
            terms = path.get("terms")
            if not isinstance(terms, dict) or set(terms) != set(TERMS):
                raise ValueError(f"{name}/{pn}: terms must have exactly the documented non-overlapping keys")
            total = 0
            missing: list[str] = []
            for term in TERMS:
                val = terms[term]
                if val is None:
                    missing.append(term)
                else:
                    total += natural(val, f"{name}/{pn}/{term}")
                    if total > MAX_U64:
                        raise ValueError(f"{name}/{pn}: sum overflows u64")
            derived: int | None = None if missing else total
            observed = path.get("observed_max_daa")
            if observed is None:
                incomplete.append(f"{name}/{pn}: observed span UNKNOWN")
            else:
                natural(observed, f"{name}/{pn}/observed_max_daa")
            refs = path.get("evidence_refs")
            if not isinstance(refs, list) or any(not isinstance(r, str) or not r.strip() for r in refs):
                raise ValueError(f"{name}/{pn}: evidence_refs must be a list of nonblank strings")
            if not refs:
                incomplete.append(f"{name}/{pn}: evidence references absent")
            if missing:
                incomplete.append(f"{name}/{pn}: {len(missing)} terms UNKNOWN")
            else:
                sums.append(total)
                if total > target:
                    overruns.append(f"{name}/{pn}: derived {total} > target {target}")
                if observed is not None and observed > total:
                    overruns.append(f"{name}/{pn}: observed {observed} > derived {total}")
            results.append({
                "path_id": pn,
                "derived_serial_upper_daa": derived,
                "known_subtotal_daa": total,
                "observed_max_daa": observed,
                "unknown_terms": missing,
            })
        for pn in sorted(set(REQUIRED_PATHS) - seen):
            incomplete.append(f"{name}: missing required path {pn}")
        output_profiles.append({
            "profile_id": name,
            "geometry": geometry(h),
            "max_complete_path_upper_daa": max(sums) if sums else None,
            "all_path_bounds_complete": len(sums) == len(REQUIRED_PATHS),
            "paths": results,
        })
    for name in sorted(set(REQUIRED_PROFILES) - found):
        incomplete.append(f"missing required profile {name}")
    status, code = ("OVER_BUDGET_OR_BOUND_VIOLATED", 1) if overruns else (("NOT_VERIFIED", 2) if incomplete else ("IN_BUDGET_ARITHMETIC_ONLY", 0))
    return {
        "status": status,
        "target_daa": target,
        "notice": NOTICE,
        "arming_authorized": False,
        "incomplete": incomplete,
        "violations": overruns,
        "profiles": output_profiles,
    }, code


def template() -> dict[str, Any]:
    return {
        "schema_version": 1,
        "target_daa": 3_200,
        "note": "All null values are UNKNOWN, not zero. Target approval and formal source bound are external.",
        "accounting_contract": (
            "Each term is a disjoint conservative duration bound. Work/queue/permit/transfer already "
            "inside another term must not be double-counted. Do not subtract speculative off-chain "
            "cache savings. Enumerate hidden subpaths and use each category's worst case."
        ),
        "pin_inputs": {"source_commit": None, "binary_sha256": None, "cost_record_sha256": None},
        "profiles": [{
            "profile_id": name,
            "history_positions": h,
            "paths": [{
                "path_id": pn,
                "terms": dict.fromkeys(TERMS),
                "observed_max_daa": None,
                "evidence_refs": [],
            } for pn in REQUIRED_PATHS],
        } for name, h in REQUIRED_PROFILES.items()],
    }


class Tests(unittest.TestCase):
    def test_geometry(self) -> None:
        a, b = geometry(32_768), geometry(2_097_152)
        self.assertEqual((a["tile_count"], a["legacy_tile_logical_levels"], a["single_position_total_logical_levels_upper"], a["depth2_response_phases_upper"]), (2048, 3, 4, 2))
        self.assertEqual((b["tile_count"], b["legacy_tile_logical_levels"], b["single_position_total_logical_levels_upper"], b["depth2_response_phases_upper"]), (131072, 5, 6, 3))

    def test_recurrence_boundaries(self) -> None:
        self.assertEqual(kary_rounds(0), 0)
        self.assertEqual(kary_rounds(1), 0)
        for exponent in range(1, 12):
            self.assertEqual(kary_rounds(16 ** exponent), exponent)
            self.assertEqual(kary_rounds(16 ** exponent + 1), exponent + 1)
        self.assertEqual(geometry(1)["depth2_response_phases_upper"], 0)
        self.assertEqual(geometry(17)["single_position_total_logical_levels_upper"], 2)

    def test_delta(self) -> None:
        self.assertEqual([required_saving(3012, x, 3200) for x in (128, 256, 384)], [0, 68, 196])
        self.assertEqual([required_saving(3196, x, 3200) for x in (128, 256, 384)], [124, 252, 380])
        self.assertEqual(required_saving(3012, 128, 3120), 20)
        self.assertEqual(required_saving(3196, 128, 3120), 204)

    def test_frontier_value_payload(self) -> None:
        self.assertEqual(256 * 4096 * 16, 16 * 1024 * 1024)

    def test_unknown_is_not_zero(self) -> None:
        report, code = check_budget(template())
        self.assertEqual(code, 2)
        self.assertEqual(report["status"], "NOT_VERIFIED")
        self.assertIsNone(report["profiles"][0]["max_complete_path_upper_daa"])
        self.assertFalse(report["arming_authorized"])

    @staticmethod
    def synthetic_complete() -> dict[str, Any]:
        d = template()
        # Explicitly synthetic, unit-test-only metadata; never release evidence.
        d["pin_inputs"] = {"source_commit": "1" * 40, "binary_sha256": "2" * 64, "cost_record_sha256": "3" * 64}
        for p in d["profiles"]:
            for path in p["paths"]:
                path["terms"] = {term: 100 for term in TERMS}
                path["observed_max_daa"] = 700
                path["evidence_refs"] = ["SYNTHETIC-UNIT-TEST-NOT-RELEASE-EVIDENCE"]
        return d

    def test_arithmetic_only_never_authorizes(self) -> None:
        report, code = check_budget(self.synthetic_complete())
        self.assertEqual(code, 0)
        self.assertEqual(report["status"], "IN_BUDGET_ARITHMETIC_ONLY")
        self.assertFalse(report["arming_authorized"])

    def test_over_budget(self) -> None:
        d = self.synthetic_complete()
        d["profiles"][0]["paths"][0]["terms"][TERMS[0]] = 4000
        report, code = check_budget(d)
        self.assertEqual(code, 1)
        self.assertTrue(report["violations"])

    def test_observation_breaks_bound(self) -> None:
        d = self.synthetic_complete()
        d["profiles"][0]["paths"][0]["observed_max_daa"] = 801
        self.assertEqual(check_budget(d)[1], 1)

    def test_invalid_inputs(self) -> None:
        for bad in (-1, True, 1.2, "1", MAX_U64 + 1):
            with self.assertRaises(ValueError):
                natural(bad, "bad")
        d = template()
        d["profiles"].append(copy.deepcopy(d["profiles"][0]))
        with self.assertRaises(ValueError):
            check_budget(d)
        with self.assertRaises(ValueError):
            geometry(100, 32)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--self-test", action="store_true")
    mode.add_argument("--geometry", action="store_true")
    mode.add_argument("--delta-plan", action="store_true")
    mode.add_argument("--check", type=Path, metavar="JSON")
    mode.add_argument("--write-template", type=Path, metavar="JSON")
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(Tests)
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    try:
        if args.write_template:
            args.write_template.write_text(json.dumps(template(), ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            return 0
        if args.geometry:
            result: Any = {"notice": NOTICE, "profiles": {k: geometry(v) for k, v in REQUIRED_PROFILES.items()}}
            code = 0
        elif args.delta_plan:
            result = {
                "notice": "Hypothetical additions; not measured values or proposed deadlines.",
                "rows": [{"target_daa": t, "hypothetical_additions_daa": x,
                          "required_saving_32k_daa": required_saving(3012, x, t),
                          "required_saving_2m_daa": required_saving(3196, x, t)}
                         for t in (3200, 3120) for x in (128, 256, 384)],
            }
            code = 0
        else:
            if args.check.stat().st_size > 4 * 1024 * 1024:
                raise ValueError("Input JSON exceeds the 4 MiB safety cap")
            result, code = check_budget(json.loads(args.check.read_text(encoding="utf-8")))
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return code
    except (ValueError, OSError, UnicodeError) as exc:
        print(json.dumps({"status": "INVALID_INPUT", "error": str(exc), "arming_authorized": False}, ensure_ascii=False), file=sys.stderr)
        return 3


if __name__ == "__main__":
    sys.exit(main())
