# `misaka-palw-base0-ref2`

Verification-only. **Never a consensus dependency** — nothing in `kaspad`'s dependency graph reaches
this crate, and nothing here is on a block-validation path.

It holds two independent checks on `kaspa_consensus_core::palw_base0`, the ADR-0040 arithmetic that
`PALW-BASE-0` is defined by, plus the differential that compares them.

| | what it is | what independence it gives |
| --- | --- | --- |
| `src/primitives.rs` | all seven primitives re-derived with exact `i128` division and **no shift operator** | structural — catches a coding mistake, not a misreading of the spec |
| `src/gemmlowp.rs` + `vendor/gemmlowp/` | upstream Google gemmlowp, byte-identical, behind an `extern "C"` shim | **authorship** — written years earlier by people with no knowledge of this project |

The two together cover different failure modes, and neither subsumes the other. gemmlowp is the
authority for `SRDHM` and `RoundingShiftRight` and defines nothing else; the other five primitives
(`IntExp`, `IntRsqrt`, `IntRecip`, `Rescale`, and the 64-bit shift) exist only in ADR-0040, so
structural independence is all that is available for them.

## What the differential found

Three defects, all on the first run, all of which the first implementation's own tests had passed.

**1. `SRDHM` disagreed with upstream gemmlowp on 50.1 % of inputs.** Upstream computes
`(ab_64 + nudge) / (1ll << 31)` — C integer **division**, which truncates toward zero — and its
asymmetric nudge (`1 - 2^30` for negatives rather than `-2^30`) exists precisely to turn that
truncation into round-half-away-from-zero. The reference paired that nudge with an arithmetic shift,
which floors, applying the correction twice. Every negative product came out one unit further from
zero: `srdhm(-2^30, 2^30)` returned `-2^29 - 1` where the exact value is `-2^29`.

This was serious out of proportion to its size, and the reason is ADR-0040 C2's own justification
for choosing the primitive: *that it is already implemented identically in several independent
codebases*. A third party writing `PALW-BASE-0` against real gemmlowp would have disagreed with the
reference on half of all inputs — and under optimistic verification a systematic disagreement is not
a rounding difference. It is a conviction and a slashed bond. The reference would have been
convicting third parties for being correct.

**2. `RoundingShiftRight` was not round-half-away-from-zero.** `(x ± 2^(s-1)) >> s` floors after
nudging, so for negatives the nudge and the floor push the same way instead of opposing:
`RSR(-64, 1)` returned `-33` where the exact quotient is `-32` and needs no rounding at all. It also
overflowed `i32` on 3.2 % of pairs, wrapping the sign of the largest accumulators. ADR-0040 C1
*stated* the rule correctly; the pseudocode under it did not implement the rule it stated. Both are
now written on the magnitude, which has no negative branch to get wrong.

**3. `RoundingShiftRight64` panicked on overflow** near `i64`'s ends. Its comment justified `i64`
arithmetic by what `rescale_q` passes in — but it is a public total function, and a reachable panic
is the remote-halt failure mode `palw_base0_ops` refuses by construction.

Two things generalise:

* All three were on **negative or extreme inputs**. The reference's existing negative test cases
  were all *exact halves*, where the defective and correct forms happen to agree — so the suite
  looked like it covered negatives and did not.
* A second implementation must **re-declare the pinned constants, not import them**. Sharing them
  made a mutation of `RSQRT_ITERS` invisible, because both sides moved together. Constants are
  specification (ADR-0040 F2), so they are compared, not shared.

## The vendored tree

| | |
| --- | --- |
| Upstream | <https://github.com/google/gemmlowp> |
| Commit vendored | `16e8662c34917be0065110bfcd9cc27d30f52fdf` |
| License | Apache-2.0 (`vendor/gemmlowp/LICENSE`) |
| Local layout | `vendor/gemmlowp/` mirrors the upstream tree (`fixedpoint/`, `internal/`) |

Every file under `vendor/gemmlowp/` is **byte-identical to upstream — no edits, ever.** That is not
left to this document: `src/gemmlowp.rs`'s `integrity` module hashes each file at test time against
the digests below, so an edit fails by filename. An oracle this project has edited is not an oracle.
The SHA-256 implementation used for the check is written out in that module rather than taken as a
dependency, since a hash policing vendored code should not itself arrive through the dependency
graph it polices; it is validated against NIST's published vectors.

gemmlowp's fixed-point layer is header-only, so **nothing of upstream's is compiled** — there is no
build configuration to reproduce and therefore none to get wrong, which is a stronger position than
`misaka-palw-reference2`'s vendored SoftFloat. The only translation unit is `shim/gemmlowp_shim.cc`,
which is ours and contains no arithmetic and no `if`: two `extern "C"` wrappers that call upstream
and return what it returns.

`build.rs` passes no `-march`/`-mavx` flags, deliberately. `internal/detect_platform.h` picks a SIMD
header from the compiler's own macros, and forcing a wider instruction set would build the oracle
differently from how a consumer of this crate builds it. All five SIMD headers are vendored so the
include closure resolves on any target, even though the scalar `std::int32_t` path is the one called.

### Files vendored, and why each is present

- `fixedpoint/fixedpoint.h` — `SaturatingRoundingDoublingHighMul` and `RoundingDivideByPOT`.
- `internal/detect_platform.h` — included by the above; selects the SIMD path.
- `fixedpoint/fixedpoint_{neon,sse,avx,msa,wasmsimd}.h` — conditionally included by
  `fixedpoint.h` depending on which macro `detect_platform.h` sets. Present so the closure resolves
  wherever the crate is built (on Apple silicon, `fixedpoint_neon.h` *is* pulled in).
- `LICENSE`, `AUTHORS` — the license the vendored code is redistributed under.

### SHA-256 of every vendored file (commit `16e8662c34917be0065110bfcd9cc27d30f52fdf`)

```
916234caa03bbb2769b278e165515a8ca9fa9d8f60b7b57a5dd6a4f026208ce2  AUTHORS
cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30  LICENSE
f1b11e756ba138b42abd2f39095fd4d740a26b10ff1a8c682c2eb4d273658cf6  fixedpoint/fixedpoint.h
e6a6fa2a5fcf5207e152eb0aff459003890d9632e9743377e291d57a3b4379c3  fixedpoint/fixedpoint_avx.h
71985120ddeeacfc8b3eed81f084c7fc35b5c47bd2b62241505bb2834f91402f  fixedpoint/fixedpoint_msa.h
83f64af6555d6c59b916f59ee2a837b1cd4140c0b7c26791ac3f0b36975f0ad0  fixedpoint/fixedpoint_neon.h
c729d7abe8c52829be63bc0b51def7aee7a2b700aaaedbcd9bc3605605d9ce2e  fixedpoint/fixedpoint_sse.h
17552be58bf100860f5a131491d126e795d2225af4a9f467d5bf1735aaa26d62  fixedpoint/fixedpoint_wasmsimd.h
bfa61d487156c68cb11fd2b114e0aa68ac048ec15b4e44631da2bc6b033a3f10  internal/detect_platform.h
```

Reproduce by cloning the commit and hashing the same paths:

```bash
git clone https://github.com/google/gemmlowp && cd gemmlowp && git checkout 16e8662c34917be0065110bfcd9cc27d30f52fdf && shasum -a 256 AUTHORS LICENSE fixedpoint/fixedpoint*.h internal/detect_platform.h
```

## What is still not established

**gemmlowp does not adjudicate five of the seven primitives.** `IntExp` (ADR-0040 F1), `IntRsqrt`
and `IntRecip` (F2), `Rescale` (H) and `RoundingShiftRight64` are this project's own definitions —
`IntExp`'s `Poly2` triple and `IntRsqrt`'s seed table have no upstream at all. For those five, one
author wrote both sides, so a misreading of the specification would be reproduced rather than
caught. The closest available third parties would be the I-BERT reference implementation for
`IntExp` and a published integer-Newton `rsqrt`; neither is vendored here.

The mechanical no-shift rule is what buys what can be bought without a third party: it is
deliberately orthogonal to the axis all three defects were on, since a silent floor and a silent
wrap both need a shift to hide in. It is enforced by a test that reads this crate's own source, not
by this paragraph — it was one line from decaying when that check was added.

## Running it

```bash
cargo test -p misaka-palw-base0-ref2 --release
```

Release is worth the flag: the differential is roughly 5M exact-equality comparisons — exhaustive on
small windows, complete on the type boundaries, then sampled with a fixed-seed LCG so a failing case
is a failing case on every machine and in every future run.
