#!/usr/bin/env python3
"""Generate ADR-0082 Decision 11's pinned Gumbel table (`PALW_GUMBEL_Q24_V1`).

The rule the table serves is `argmax_j (logit_j * T_ONE + T_q * G_j)`, all in i64, with
`T_ONE = 1 << 24` (Q24, the `palw_base0::K` fixed point the whole class already uses). `G_j` is
a Gumbel(0, 1) variate, and a Gumbel variate is a transcendental of a uniform — which a consensus
rule may not evaluate. So the variates are enumerated ONCE here, at a pinned resolution, and the
runtime does one array index.

  Arithmetic, stated so a reader can re-derive every entry:

      N      = 8192 = 2**13                        the table's length; the index is 13 bits
      u_i    = (i + Fraction(1, 2)) / N            the midpoint of bucket i, so no entry is the
                                                   degenerate u = 0 or u = 1 where the quantile
                                                   diverges
      G(u)   = -ln(-ln(u))                         the Gumbel(0, 1) quantile
      entry  = round_half_even(G(u_i) * 2**24)     Q24, as an i32

`u_i` is an exact rational and `G` is evaluated in `decimal` at 60 significant digits, so the
rounding is decided far above the 24 fractional bits it lands in; the same table comes out of any
correct implementation. The extremes are `G(u_0) = -2.2725...` and `G(u_8191) = +9.7031...`, so
every entry is inside i32 by a factor of ~13, and `T_q * G_j` is inside i64 by a factor of ~13
even at `T_q = u32::MAX` (`the_keyed_row_cannot_overflow_an_i64` pins that).

Usage:  python3 scripts/palw-gumbel-table.py            # prints the Rust table body
        python3 scripts/palw-gumbel-table.py --hash     # prints the BLAKE2b-512 pin only

The hash is over the 8192 entries as little-endian i32, keyed with the module's own domain
string, and `the_gumbel_table_is_the_one_the_script_generates` in
`consensus/core/src/palw_decode_select_v2.rs` re-derives it from the shipped array.
"""

import argparse
import hashlib
from decimal import Decimal, getcontext, ROUND_HALF_EVEN
from fractions import Fraction

getcontext().prec = 60

TABLE_LEN = 1 << 13
Q24_ONE = 1 << 24
DOMAIN = b"misaka-palw/decode-select-v2/gumbel/v1"


def gumbel_q24(i: int) -> int:
    """`round(-ln(-ln((i + 1/2) / N)) * 2**24)`, decided at 60 significant digits."""
    u = Fraction(2 * i + 1, 2 * TABLE_LEN)
    u_dec = Decimal(u.numerator) / Decimal(u.denominator)
    g = -(-u_dec.ln()).ln()
    scaled = g * Decimal(Q24_ONE)
    return int(scaled.to_integral_value(rounding=ROUND_HALF_EVEN))


def table() -> list[int]:
    return [gumbel_q24(i) for i in range(TABLE_LEN)]


def table_hash(entries: list[int]) -> str:
    h = hashlib.blake2b(digest_size=64, key=DOMAIN)
    for e in entries:
        h.update(int(e).to_bytes(4, "little", signed=True))
    return h.hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--hash", action="store_true", help="print only the BLAKE2b-512 pin")
    args = ap.parse_args()
    entries = table()
    assert len(entries) == TABLE_LEN
    assert all(-(2**31) <= e < 2**31 for e in entries), "an entry left i32"
    assert entries == sorted(entries), "the quantile is strictly increasing; the table must be too"
    if args.hash:
        print(table_hash(entries))
        return
    print(f"// {TABLE_LEN} entries of Q24 `-ln(-ln((i + 0.5) / {TABLE_LEN}))`.")
    print(f"// blake2b-512 keyed with {DOMAIN.decode()}:")
    print(f"//   {table_hash(entries)}")
    print(f"pub const PALW_GUMBEL_Q24_V1: [i32; {TABLE_LEN}] = [")
    for row in range(0, TABLE_LEN, 8):
        chunk = ", ".join(str(e) for e in entries[row : row + 8])
        print(f"    {chunk},")
    print("];")


if __name__ == "__main__":
    main()
