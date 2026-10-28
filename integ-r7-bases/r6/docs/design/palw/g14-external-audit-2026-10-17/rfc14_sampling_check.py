#!/usr/bin/env python3
"""Independent RFC14 sampling arithmetic and a Python port of the legacy draw.

This does NOT execute, build or validate the Rust node. The golden-vector check
only compares a translation of the pure draw algorithm with its checked-in vector.
Probability results assume a fixed bad set and unbiased, distinct sampling;
cryptographic grinding, claim coverage, deadlines and court capacity are excluded.
Run: python3 rfc14_sampling_check.py --output sampling_results.json
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import math
from fractions import Fraction
from pathlib import Path


def validate_counts(n: int, bad: int, k: int) -> None:
    if any(not isinstance(v, int) or isinstance(v, bool) for v in (n, bad, k)):
        raise TypeError("n, bad and k must be integers")
    if n < 1 or not 0 <= bad <= n or not 0 <= k <= n:
        raise ValueError("Require n >= 1 and 0 <= bad, k <= n")


def miss_probability(n: int, bad: int, k: int) -> Fraction:
    """Exact probability of zero bad intervals in k uniform draws without replacement."""
    validate_counts(n, bad, k)
    if k > n - bad:
        return Fraction(0)
    return Fraction(math.comb(n - bad, k), math.comb(n, k))


def minimum_samples(n: int, bad: int, max_miss: Fraction) -> int:
    """Smallest k meeting the specified miss bound; uses exact comparisons."""
    validate_counts(n, bad, 0)
    if bad == 0 or not 0 < max_miss < 1:
        raise ValueError("Require bad >= 1 and 0 < max_miss < 1")
    lo, hi = 0, n
    while lo < hi:
        mid = (lo + hi) // 2
        if miss_probability(n, bad, mid) <= max_miss:
            hi = mid
        else:
            lo = mid + 1
    return lo


def hash64_word(word: int) -> bytes:
    """crypto/hashes/src/hash64.rs:218-219: seven zero LE lanes, then word."""
    if not 0 <= word < 2**64:
        raise ValueError("word outside u64 range")
    return b"\0" * 56 + word.to_bytes(8, "little")


def legacy_draw(network: bytes, beacon: bytes, claim: bytes, seat: int,
                k: int, interval_count: int) -> list[int]:
    """Port of palw_fp_interval_v1.rs:39-70, NOT a new consensus implementation."""
    if any(len(v) != 64 for v in (network, beacon, claim)):
        raise ValueError("All hash values must have 64 bytes")
    if not 0 <= seat <= 255 or not 0 <= k < 2**32 or not 0 <= interval_count < 2**32:
        raise ValueError("Invalid u8/u32 argument")
    if interval_count == 0 or k == 0:
        return []
    if interval_count <= k:
        return list(range(interval_count))
    out: list[int] = []
    seen: set[int] = set()
    draw = 0
    while len(out) < k:
        state = hashlib.blake2b(digest_size=64, key=b"misaka-palw/fp-v3/seat-interval-draw/v1")
        state.update(network + beacon + claim + bytes([seat]) + draw.to_bytes(8, "little"))
        index = int.from_bytes(state.digest()[:8], "little") % interval_count
        if index not in seen:
            out.append(index)
            seen.add(index)
        draw += 1
    return out


def self_tests() -> dict[str, object]:
    # Exhaustive subset enumeration cross-check, independent of the closed form.
    cases = 0
    for n in range(1, 11):
        for k in range(n + 1):
            samples = list(itertools.combinations(range(n), k))
            for bad in range(n + 1):
                misses = sum(all(i >= bad for i in sample) for sample in samples)
                assert miss_probability(n, bad, k) == Fraction(misses, len(samples))
                cases += 1
    h = hash64_word
    draw = legacy_draw(h(0x11), h(0x22), h(0x33), 2, 4, 1000)
    assert draw == [645, 20, 372, 227], draw
    # Re-running the same inputs 100 times does not create any additional coverage.
    repeats = {tuple(legacy_draw(h(0x11), h(0x22), h(0x33), 2, 4, 1000)) for _ in range(100)}
    assert len(repeats) == 1
    assert legacy_draw(h(1), h(2), h(3), 0, 4, 3) == [0, 1, 2]
    assert minimum_samples(1000, 1, Fraction(1, 10)) == 900
    assert minimum_samples(1000, 1, Fraction(1, 100)) == 990
    return {
        "exhaustive_probability_cases_passed": cases,
        "ported_golden_vector": draw,
        "identical_input_repetitions": 100,
        "distinct_draw_sets_in_identical_repetitions": len(repeats),
        "rust_tests_executed": False,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="Write an optional JSON result file")
    args = parser.parse_args()
    tests = self_tests()
    rows = []
    for bad in (1000, 500, 100, 10, 1):
        rows.append({
            "total_intervals": 1000,
            "detectably_bad_intervals": bad,
            "detection_percent_at_k4": float(100 * (1 - miss_probability(1000, bad, 4))),
            "minimum_k_for_90_percent": minimum_samples(1000, bad, Fraction(1, 10)),
            "minimum_k_for_99_percent": minimum_samples(1000, bad, Fraction(1, 100)),
        })
    result = {
        "scope": "Independent Python calculation; illustrative N=1000, not a genesis model measurement",
        "assumptions": ["fixed detectably bad set", "uniform distinct sampling", "one honest checker actually checks this claim"],
        "excluded": ["Rust execution", "claim-check coverage", "adaptive/grinding attacks", "proof localisation/inclusion", "court capacity", "collateral collection"],
        "self_tests": tests,
        "illustrative_cases": rows,
    }
    text = json.dumps(result, ensure_ascii=False, indent=2)
    print(text)
    if args.output:
        args.output.write_text(text + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
