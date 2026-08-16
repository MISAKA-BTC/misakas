# misaka-palw-reference2

The PALW **§29 gate-1 "independent second implementation"** of the canonical reference
arithmetic (`kaspa_consensus_core::palw_reference`, ADR-0027 §2): the same frozen IEEE-754
binary32 semantics computed by **Berkeley SoftFloat Release 3e** (John R. Hauser — independent
authorship, independent algorithmic structure), wrapped in the PALW canonicalization contract
and differentially tested against the normative implementation with exact `u32` equality.

**Test/verification-only.** This crate must NEVER be linked into consensus, mining, or
validation paths; the normative implementation remains `palw_reference` alone. Structurally
enforced: nothing in the workspace depends on this crate, and its only tie to consensus code is
a `dev-dependency` used by the differential tests.

Run the gate evidence with:

```
cargo test -p misaka-palw-reference2 --release
```

## Vendored code provenance

| | |
|---|---|
| Upstream | <https://github.com/ucb-bar/berkeley-softfloat-3> |
| Release | Berkeley SoftFloat Release 3e (2018-01-20, John R. Hauser) |
| Commit vendored | `a0c6494cdc11865811dec815d5c0049fba9d82a8` |
| License | BSD-3-Clause (`vendor/softfloat/COPYING.txt`, included verbatim) |
| Local layout | `vendor/softfloat/` mirrors the upstream tree (`source/`, `source/include/`, `source/8086-SSE/`) |

Every file under `vendor/softfloat/` is **byte-identical to upstream** — no edits, ever.
All configuration happens in `build.rs` (defines + include paths) and `shim/platform.h`
(the one file of ours in the C build, standing in for upstream's per-target
`build/<target>/platform.h`).

Vendored subset — only what the f32 add/sub/mul paths need:

- `source/`: `f32_add.c`, `f32_sub.c`, `f32_mul.c`, `s_addMagsF32.c`, `s_subMagsF32.c`,
  `s_normRoundPackToF32.c`, `s_roundPackToF32.c`, `s_normSubnormalF32Sig.c`,
  `s_shortShiftRightJam64.c`, `s_shiftRightJam32.c`, `s_shiftRightJam64.c`,
  `s_countLeadingZeros8.c` (the CLZ table), `softfloat_state.c`
- `source/include/`: `softfloat.h`, `internals.h`, `primitives.h`, `primitiveTypes.h`,
  `softfloat_types.h`
- `source/8086-SSE/` (specialization): `specialize.h`, `s_propagateNaNF32UI.c`,
  `softfloat_raiseFlags.c`
- `COPYING.txt`

Notes on the subset:

- `softfloat_state.c` is required for linking: it defines the `softfloat_roundingMode` /
  `softfloat_detectTininess` / `softfloat_exceptionFlags` globals that `s_roundPackToF32.c`
  and `softfloat_raiseFlags.c` reference.
- `s_f32UIToCommonNaN.c` / `s_commonNaNToF32UI.c` are **not** needed: the 8086-SSE
  `s_propagateNaNF32UI.c` works directly on bit patterns and never touches the `commonNaN`
  conversion helpers (those serve cross-format conversions, which are not vendored).
- `opts-GCC.h` is **not** vendored: we do not define `SOFTFLOAT_BUILTIN_CLZ` /
  `SOFTFLOAT_INTRINSIC_INT128`, so the portable table-driven CLZ path is used instead and the
  build stays compiler-agnostic (performance is irrelevant here).

Build configuration vs upstream's reference `build/Linux-x86_64-GCC` build:

- kept: `-DSOFTFLOAT_FAST_INT64`, `-DINLINE_LEVEL=5`, `-DSOFTFLOAT_ROUND_ODD`
- dropped (division-only; no division vendored): `-DSOFTFLOAT_FAST_DIV32TO16`,
  `-DSOFTFLOAT_FAST_DIV64TO32`
- `INLINE` is `static inline` rather than C99 extern-inline (see `shim/platform.h` for why:
  correctness at any optimization level; cannot affect results)
- `THREAD_LOCAL` is left undefined (plain globals); the Rust wrapper serializes all access
  behind one mutex and pins round-to-nearest-even on every entry

SoftFloat, like `palw_reference`, is pure integer code: `float32_t` is a `struct { uint32_t v; }`
and no value ever touches a hardware float register, so compiler FP flags cannot affect it.

### SHA-256 of every vendored file (commit `a0c6494cdc11865811dec815d5c0049fba9d82a8`)

```
145ea96b4a4a04a1a7738d2a2bf9e830f861971e69606187b018d9e8fc0b95c7  COPYING.txt
83215e4528ffaee4e5bf4975e42acc65eb349dbdf33a408857c707790ed356e7  source/8086-SSE/s_propagateNaNF32UI.c
fe7a5684bc9f72a52e603dbfa0ad23897eb2983df1cb3ea440023308ca14d8bb  source/8086-SSE/softfloat_raiseFlags.c
8447d57528b20f8b5d61c49ad6d4b4e4fdfdbdc3cf2c2328ffd423c022832105  source/8086-SSE/specialize.h
540c8f2d95b30bc846fe5b747aa6aae1bb70c687b9429278678bb0a000f65dcc  source/f32_add.c
bca46360750b343f24aafcfa7c36f24e3fc4fb6465d0c49f60de7980a86bc37b  source/f32_mul.c
c4020ba5c2d5f10e647711bd7287c574ca498b0e469ee99f1f9c68c930d43311  source/f32_sub.c
24ba8b3497c3f3e48e01c22424c6314b53d557eb6dfbb96f02e71743ec1daf11  source/include/internals.h
6b0841b1aa4a9c3a1c0a89e868d5d69592e54360dc8e4ba947f7853243a55dcc  source/include/primitiveTypes.h
f416c1a7a3f84d863ddc089665147b3ea51c62451a49b849c5176cfce402cd1d  source/include/primitives.h
2125ee655579f4153ae305d63e5c1b21897f6ed1f44a81a97e507bd245bcb3cc  source/include/softfloat.h
554233f87179dd7ef6c0d8f3b0f00d39a0cd452d8c4e22cc1b3adca2d8dc2d0a  source/include/softfloat_types.h
5b2d7992841c17e42a96e5aec865b4386cdc7e9030ac6f4d25ad74e4e73fb59b  source/s_addMagsF32.c
8d0076e681bc613b607d8ff9d1b72631a3bfe66dd75ecb7f4faed5ddc3b830ca  source/s_countLeadingZeros8.c
2e473ba709740808cfd59931034be7c67c80c87ddb76a4765dceb23c9bb518d2  source/s_normRoundPackToF32.c
6063e5c0ee98527c1a5d728195d11ef5ccd9077515812b0380a2f1953244c861  source/s_normSubnormalF32Sig.c
aac61a425bed4e973c9e0e1fb254f74cb7c04602131509ed98513269da83e45a  source/s_roundPackToF32.c
96dee4db4ed95ffe7bd44060f980f0a010003a11c20f5757992dcc1f447e0b62  source/s_shiftRightJam32.c
ea54abfce241315f78b69893f6d7b20ab767e5fe8ade6b107245d8dc0745630f  source/s_shiftRightJam64.c
0fcadc566939b0155100929995ee5234fb40cc146cf7093d6c9c15184aaca00c  source/s_shortShiftRightJam64.c
53ca2e9f6f1edc7095133e7c98018d69d1e6cd0d5f5b5c0393c4e656ee9a965c  source/s_subMagsF32.c
5f7a70c4c5823cdde0518db77a74a6549dba62b21d910f6dadb9115b1d478301  source/softfloat_state.c
```

## The canonicalization contract (imposed by `src/lib.rs`, not by SoftFloat)

Raw SoftFloat implements 8086-SSE NaN semantics: payloads propagate (quieted with
`| 0x00400000`) and invalid operations mint the 8086 default NaN `0xFFC00000`. The frozen PALW
ruleset instead requires **every NaN operand or NaN result to become the canonical quiet NaN
`0x7FC00000`**. The wrappers enforce the operand rule *before* SoftFloat is called and
canonicalize NaN results after it, so the two implementations expose bit-identical contracts —
this is exactly the subtlety the differential tests pin first
(`nan_operands_always_canonicalize_in_both_implementations`).

## What the differential tests cover (`tests/differential.rs`)

Exact `u32` equality against `ref_add_v1` / `ref_sub_v1` / `ref_mul_v1` / `ref_neg_v1` /
`ref_dot_v1` / `ref_gemm_v1` on: the full ±special-value matrix (all ordered pairs; add, mul,
sub via the pinned identity, sub via SoftFloat's own `f32_sub`, neg), 2,000,000 random pairs,
500,000 tie-neighborhood pairs, 500,000 subnormal pairs plus gradual-underflow products,
random mixed-magnitude dot vectors (and dot vectors with NaN/Inf/subnormal elements injected),
random GEMM tiles, the dot order witness, and the invalid-operation table. Any single
disagreement is a CRITICAL finding against the gate-1 claim: stop, report the exact bits,
never widen a tolerance.
