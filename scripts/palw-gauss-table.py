#!/usr/bin/env python3
"""Generate RFC-0003 §I.1.5's pinned Gaussian table (`PALW_GAUSS_Q24_V1`).

A consensus rule may not evaluate a transcendental, so the standard normal quantile is enumerated
ONCE here, at a pinned resolution, and the runtime does one array index — the shape of ADR-0082
Decision 11's Gumbel table (`scripts/palw-gumbel-table.py`), which this script follows.

  Arithmetic, stated so a reader can re-derive every entry:

      N      = 65536 = 2**16                     the table's length; the index is a 16-bit word of R
      p_i    = (i + 1/2) / N                     the midpoint of bucket i (an exact dyadic rational),
                                                 so no entry is the degenerate p = 0 or p = 1
      z_i    = PHI^-1(p_i)                       the standard normal quantile
      entry  = round_half_away_from_zero(z_i * 2**24)          Q24, as an i32

  z_i is computed in `decimal` at PREC significant digits by Newton's method on PHI(z) - p, where

      PHI(z) = (1 + erf(z / sqrt 2)) / 2,   erf(x) = (2 / sqrt pi) e^(-x^2) sum_{n>=0} (2x^2)^n x / (1*3*...*(2n+1))

  (a series of positive terms: no cancellation inside it). Only i < N/2 (z < 0) is computed;
  entry[N-1-i] = -entry[i] exactly, because p_(N-1-i) = 1 - p_i and PHI^-1(1 - p) = -PHI^-1(p), and
  rounding half away from zero is odd. The rounding of every entry is decided far above the 24
  fractional bits: the script refuses to emit a table if any scaled value lies within 10^-40 of a
  half. Properties it asserts: strictly increasing, antisymmetric, inside i32.

The pin is BLAKE2b-512 keyed with the ASCII bytes `misaka-palw/rand/gauss-q24/v1` over the 65,536
entries as little-endian i32 — exactly the bytes of the table file.

Usage:  python3 scripts/palw-gauss-table.py --out misaka-palw-gen/data/gauss-q24-v1.bin   # write it, print the pin
        python3 scripts/palw-gauss-table.py --check misaka-palw-gen/data/gauss-q24-v1.bin # regenerate and compare
        python3 scripts/palw-gauss-table.py --hash                                        # print the pin only

Standard library only (no mpmath), so the table can be regenerated on any machine with Python 3.
"""

import argparse
import hashlib
import math
import statistics
import sys
from decimal import ROUND_HALF_UP, Decimal, getcontext, localcontext

PREC = 80
getcontext().prec = PREC

TABLE_LEN = 1 << 16
Q24_ONE = 1 << 24
KEY = b"misaka-palw/rand/gauss-q24/v1"


def machin_pi() -> Decimal:
    """pi = 16 atan(1/5) - 4 atan(1/239), each by its alternating series, at PREC + 10 digits."""
    with localcontext() as ctx:
        ctx.prec = PREC + 10

        def atan_inv(n: int) -> Decimal:
            x = Decimal(1) / n
            x2 = x * x
            term, total, k = x, x, 1
            eps = Decimal(10) ** -(PREC + 15)
            while True:
                term *= -x2
                t = term / (2 * k + 1)
                if abs(t) < eps:
                    break
                total += t
                k += 1
            return total

        return +(16 * atan_inv(5) - 4 * atan_inv(239))


PI = machin_pi()
SQRT2 = Decimal(2).sqrt()
SQRT_PI = PI.sqrt()
SQRT_2PI = (2 * PI).sqrt()
EPS_SERIES = Decimal(10) ** -(PREC + 5)


def erf_positive(x: Decimal) -> Decimal:
    """erf(x) for x >= 0 by the all-positive series."""
    x2 = x * x
    two_x2 = 2 * x2
    term, total, n = x, x, 0
    while True:
        n += 1
        term = term * two_x2 / (2 * n + 1)
        total += term
        if term < total * EPS_SERIES:
            break
    return 2 / SQRT_PI * (-x2).exp() * total


def erf_alternating(x: Decimal) -> Decimal:
    """erf(x) by the alternating Taylor series, at a much higher precision to absorb its
    cancellation — an independent check of `erf_positive`, never used for the table."""
    with localcontext() as ctx:
        ctx.prec = PREC + 60
        x2 = x * x
        term, total, n = x, x, 0
        eps = Decimal(10) ** -(PREC + 50)
        while True:
            n += 1
            term = -term * x2 / n
            t = term / (2 * n + 1)
            total += t
            if abs(t) < eps:
                break
        pi = machin_pi()  # PREC + 10 digits is ample for a PREC-digit comparison
        return +(2 / pi.sqrt() * total)


def phi_cdf(z: Decimal) -> Decimal:
    if z < 0:
        return (1 - erf_positive(-z / SQRT2)) / 2
    return (1 + erf_positive(z / SQRT2)) / 2


def phi_pdf(z: Decimal) -> Decimal:
    return (-(z * z) / 2).exp() / SQRT_2PI


def quantile_lower(p: Decimal) -> Decimal:
    """PHI^-1(p) for 0 < p < 1/2, by Newton from the float quantile."""
    z = Decimal(repr(statistics.NormalDist().inv_cdf(float(p))))
    stop = Decimal(10) ** -(PREC - 10)
    for _ in range(12):
        dz = (phi_cdf(z) - p) / phi_pdf(z)
        z -= dz
        if abs(dz) < stop:
            return z
    raise RuntimeError(f"Newton did not converge at p = {p}")


def entry_lower(i: int) -> int:
    assert 0 <= i < TABLE_LEN // 2
    p = Decimal(2 * i + 1) / Decimal(2 * TABLE_LEN)  # exact: a dyadic rational with 17 bits
    scaled = quantile_lower(p) * Q24_ONE
    mag = abs(scaled)
    frac = mag - mag.to_integral_value(rounding="ROUND_FLOOR")
    if abs(frac - Decimal("0.5")) < Decimal(10) ** -40:
        raise RuntimeError(f"entry {i}: the scaled value is within 1e-40 of a half; rounding is not decided")
    return int(scaled.quantize(Decimal(1), rounding=ROUND_HALF_UP))  # ROUND_HALF_UP = half away from zero


def self_check() -> None:
    for x in ["0.001", "0.5", "1", "2.25", "3.1"]:
        a, b = erf_positive(Decimal(x)), erf_alternating(Decimal(x))
        assert abs(a - b) < Decimal(10) ** -(PREC - 8), f"erf({x}): the two series disagree"
        assert abs(float(a) - math.erf(float(x))) < 1e-15, f"erf({x}) disagrees with the float erf"


def table() -> list[int]:
    self_check()
    lower = [entry_lower(i) for i in range(TABLE_LEN // 2)]
    entries = lower + [-v for v in reversed(lower)]
    assert len(entries) == TABLE_LEN
    assert all(-(2**31) <= e < 2**31 for e in entries), "an entry left i32"
    assert all(a < b for a, b in zip(entries, entries[1:])), "the quantile is strictly increasing; the table must be too"
    assert all(entries[TABLE_LEN - 1 - i] == -entries[i] for i in range(TABLE_LEN)), "the table is antisymmetric"
    return entries


def table_bytes(entries: list[int]) -> bytes:
    return b"".join(int(e).to_bytes(4, "little", signed=True) for e in entries)


def table_hash(data: bytes) -> str:
    return hashlib.blake2b(data, digest_size=64, key=KEY).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--out", help="write the table file and print its pin")
    g.add_argument("--check", help="regenerate and compare with this table file")
    g.add_argument("--hash", action="store_true", help="print only the BLAKE2b-512 pin")
    args = ap.parse_args()
    data = table_bytes(table())
    pin = table_hash(data)
    if args.hash:
        print(pin)
    elif args.out:
        with open(args.out, "wb") as f:
            f.write(data)
        print(f"wrote {len(data)} bytes to {args.out}")
        print(pin)
    else:
        with open(args.check, "rb") as f:
            on_disk = f.read()
        if on_disk != data:
            print(f"{args.check} differs from the regenerated table", file=sys.stderr)
            return 1
        print(f"{args.check} is the regenerated table; pin {pin}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
