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

Vendored subset — what the ruleset-v1 f32 add/sub/mul paths need, plus the ruleset-v2
surface (f32 fma/div/sqrt, the f64 family, and the f32↔f64 / f16↔f32 conversions):

- `source/`, v1 core: `f32_add.c`, `f32_sub.c`, `f32_mul.c`, `s_addMagsF32.c`,
  `s_subMagsF32.c`, `s_normRoundPackToF32.c`, `s_roundPackToF32.c`,
  `s_normSubnormalF32Sig.c`, `s_shortShiftRightJam64.c`, `s_shiftRightJam32.c`,
  `s_shiftRightJam64.c`, `s_countLeadingZeros8.c` (the CLZ table), `softfloat_state.c`
- `source/`, v2 ops: `f32_mulAdd.c`, `s_mulAddF32.c`, `f32_div.c`, `f32_sqrt.c`,
  `f64_add.c`, `f64_sub.c`, `f64_mul.c`, `f64_div.c`, `f64_mulAdd.c`, `s_addMagsF64.c`,
  `s_subMagsF64.c`, `s_mulAddF64.c`, `s_normRoundPackToF64.c`, `s_roundPackToF64.c`,
  `s_normSubnormalF64Sig.c`
- `source/`, v2 conversions: `f32_to_f64.c`, `f64_to_f32.c`, `f16_to_f32.c`,
  `f32_to_f16.c`, `s_roundPackToF16.c`, `s_normSubnormalF16Sig.c`
- `source/`, v2 primitives: `s_countLeadingZeros16.c`, `s_countLeadingZeros64.c`,
  `s_mul64To128.c`, `s_add128.c`, `s_sub128.c`, `s_shortShiftLeft128.c`,
  `s_shortShiftRightJam128.c`, `s_shiftRightJam128.c`, `s_approxRecip32_1.c`,
  `s_approxRecipSqrt32_1.c`, `s_approxRecip_1Ks.c`, `s_approxRecipSqrt_1Ks.c`
- `source/include/`: `softfloat.h`, `internals.h`, `primitives.h`, `primitiveTypes.h`,
  `softfloat_types.h`
- `source/8086-SSE/` (specialization): `specialize.h`, `s_propagateNaNF32UI.c`,
  `s_propagateNaNF64UI.c`, `softfloat_raiseFlags.c`, and the `commonNaN` carriers for the
  cross-format conversions: `s_f16UIToCommonNaN.c`, `s_f32UIToCommonNaN.c`,
  `s_f64UIToCommonNaN.c`, `s_commonNaNToF16UI.c`, `s_commonNaNToF32UI.c`,
  `s_commonNaNToF64UI.c`
- `COPYING.txt`

Notes on the subset:

- `softfloat_state.c` is required for linking: it defines the `softfloat_roundingMode` /
  `softfloat_detectTininess` / `softfloat_exceptionFlags` globals that `s_roundPackToF32.c`
  and `softfloat_raiseFlags.c` reference.
- The `commonNaN` conversion helpers joined with the v2 cross-format conversions; the Rust
  wrappers still canonicalize NaNs on both sides of every call, so no specialization payload
  behavior can reach a test result through them.
- `s_mul64To128M.c` is **not** vendored: it is the `!SOFTFLOAT_FAST_INT64` twin of
  `s_mul64To128.c`, unreachable under this build's defines.
- `opts-GCC.h` is **not** vendored: we do not define `SOFTFLOAT_BUILTIN_CLZ` /
  `SOFTFLOAT_INTRINSIC_INT128`, so the portable table-driven CLZ path is used instead and the
  build stays compiler-agnostic (performance is irrelevant here).

Build configuration vs upstream's reference `build/Linux-x86_64-GCC` build:

- kept: `-DSOFTFLOAT_FAST_INT64`, `-DINLINE_LEVEL=5`, `-DSOFTFLOAT_ROUND_ODD`,
  `-DSOFTFLOAT_FAST_DIV32TO16`, `-DSOFTFLOAT_FAST_DIV64TO32` (the division defines joined
  when the ruleset-v2 division paths were vendored; before that no division was vendored
  and they were omitted)
- `INLINE` is `static inline` rather than C99 extern-inline (see `shim/platform.h` for why:
  correctness at any optimization level; cannot affect results)
- `THREAD_LOCAL` is left undefined (plain globals); the Rust wrapper serializes all access
  behind one mutex and pins round-to-nearest-even on every entry

SoftFloat, like `palw_reference`, is pure integer code: `float32_t` is a `struct { uint32_t v; }`
and no value ever touches a hardware float register, so compiler FP flags cannot affect it.

### SHA-256 of every vendored file (commit `a0c6494cdc11865811dec815d5c0049fba9d82a8`)

```
145ea96b4a4a04a1a7738d2a2bf9e830f861971e69606187b018d9e8fc0b95c7  COPYING.txt
7e25730d684ec0a3945c7950f8ba4208ae7a08cb0fe3ab60e66d9744654f6e07  source/8086-SSE/s_commonNaNToF16UI.c
190c04615aec8b828bfa1c46e8ebc31cc669a4e6f41ae273d09f16708cafbfe6  source/8086-SSE/s_commonNaNToF32UI.c
c9abe3713181963175bf95324103a9a7fa022488eba9264b1b97ef7cd9adae1a  source/8086-SSE/s_commonNaNToF64UI.c
506e7c6e841fdf332ba5283f512634bf559e93c3fd1d58ccf7e61202b47c6d6d  source/8086-SSE/s_f16UIToCommonNaN.c
a2a2aa1f4198c1c522ccba123388912dbb37699bd504bbf61014b43834a88a8c  source/8086-SSE/s_f32UIToCommonNaN.c
0af1257c9928b3c5692ca53975b7d9fb9af389a08e2d264469db328a894c949c  source/8086-SSE/s_f64UIToCommonNaN.c
83215e4528ffaee4e5bf4975e42acc65eb349dbdf33a408857c707790ed356e7  source/8086-SSE/s_propagateNaNF32UI.c
e3584b53c927e73a18457654125a558c58501ad7cc1de9bd3c6db75e06c190aa  source/8086-SSE/s_propagateNaNF64UI.c
fe7a5684bc9f72a52e603dbfa0ad23897eb2983df1cb3ea440023308ca14d8bb  source/8086-SSE/softfloat_raiseFlags.c
8447d57528b20f8b5d61c49ad6d4b4e4fdfdbdc3cf2c2328ffd423c022832105  source/8086-SSE/specialize.h
49121b429cb3e2d2da4cf7c32b3e48c8f2de14c7a1d1bee964202e5f338bc217  source/f16_to_f32.c
540c8f2d95b30bc846fe5b747aa6aae1bb70c687b9429278678bb0a000f65dcc  source/f32_add.c
ec341d5028f63f19230729ed8d8aca424b08f221adfdce4ebce3caa6d54ba0d8  source/f32_div.c
bca46360750b343f24aafcfa7c36f24e3fc4fb6465d0c49f60de7980a86bc37b  source/f32_mul.c
867914dd6c8dc3a03e1369b7a65fb1cf2975ef2d1a1b5bbf3eeb1b8758cb3e2b  source/f32_mulAdd.c
2eb2d1a5a35d2e839aa69e18bda3868ef5f86c214df6adad3ae50cb8a6195dc8  source/f32_sqrt.c
c4020ba5c2d5f10e647711bd7287c574ca498b0e469ee99f1f9c68c930d43311  source/f32_sub.c
6a413eb2ca2de4e345b7e8aaf416a2987c3ec8cb13b6135953d470df67fc4f1d  source/f32_to_f16.c
d25caefe4a26b9e9e1fa1cc53c683d8a22468122b2d09c16deb0e833702ada10  source/f32_to_f64.c
edb05c732053d1107456c95b7f9b18e58a25546925d3ffafac27b5f263db0b2e  source/f64_add.c
c44985b8084d6407fd3cd4043dbe02c2651721ac03d1c3920798fc1b75ea2f3e  source/f64_div.c
335137bca17d29eddb451f562869ca7327c5fe472f347d2e36c55e4c2219e79b  source/f64_mul.c
1d16eff3acdec91e3aee893c4f389cf23df998e2bd199344d0574c01a9b09bcc  source/f64_mulAdd.c
6b581000e70e679e057d2ce17e0839aa5a7da6982685b9718b6540069441e689  source/f64_sub.c
48e998019d76613444c6f20e5d272464abd2657795830905dcf629bb18d90529  source/f64_to_f32.c
24ba8b3497c3f3e48e01c22424c6314b53d557eb6dfbb96f02e71743ec1daf11  source/include/internals.h
6b0841b1aa4a9c3a1c0a89e868d5d69592e54360dc8e4ba947f7853243a55dcc  source/include/primitiveTypes.h
f416c1a7a3f84d863ddc089665147b3ea51c62451a49b849c5176cfce402cd1d  source/include/primitives.h
2125ee655579f4153ae305d63e5c1b21897f6ed1f44a81a97e507bd245bcb3cc  source/include/softfloat.h
554233f87179dd7ef6c0d8f3b0f00d39a0cd452d8c4e22cc1b3adca2d8dc2d0a  source/include/softfloat_types.h
c87826a723bda5596d23e7141cd38e0cc0ab07446ee149d8981a3a5045d84b1f  source/s_add128.c
5b2d7992841c17e42a96e5aec865b4386cdc7e9030ac6f4d25ad74e4e73fb59b  source/s_addMagsF32.c
2b4f69206b4e8d3fc26bc64ca147663530b68b53b0b0e7817c724e2d99376ec5  source/s_addMagsF64.c
38e984f9c6ea5843216ba73d08fafd217b285badce557bc9eb58227acc3667ea  source/s_approxRecip32_1.c
dae5502facf520d0915403967e3436e1c2d2f5b848527accdb3b1d2a3654a47f  source/s_approxRecipSqrt32_1.c
d7c4b4833ce9f42e86d9933a550ad0d6e4b8bc4df4bb30518c8a747b12699bf8  source/s_approxRecipSqrt_1Ks.c
a073abc2d5e7a4dfd97f323027d1ef1d370225aa922ae2395398ff3c7c8e9633  source/s_approxRecip_1Ks.c
7c2f9d6638550f9928fca7ac2969e34939208fce544458f9ce6d4c74eaa18033  source/s_countLeadingZeros16.c
f8fc823d585542552de1295cc8181369799030e7c67d2fa21ae6f7be16bb6840  source/s_countLeadingZeros64.c
8d0076e681bc613b607d8ff9d1b72631a3bfe66dd75ecb7f4faed5ddc3b830ca  source/s_countLeadingZeros8.c
1cff12724898aaace69ba3d7301404ded67156ba13466aec4a25f1a3cbb9564d  source/s_mul64To128.c
fdfa3a9d52c8e8406d5121ed3c11838c0f643c37dadecb408ff1842a05a7dcd1  source/s_mulAddF32.c
1761bdb8cc07d782f17a59ae62f931bf7a238fdabf8f3dc7190d0f40237469f7  source/s_mulAddF64.c
2e473ba709740808cfd59931034be7c67c80c87ddb76a4765dceb23c9bb518d2  source/s_normRoundPackToF32.c
4e65ccecf8ee96e2c5815575df3a0e4495ac82987a841868cb1d130556a3dd0a  source/s_normRoundPackToF64.c
f41c767257079404ec64f70806782c65690720a4e953e34c6c909b2bf9ead563  source/s_normSubnormalF16Sig.c
6063e5c0ee98527c1a5d728195d11ef5ccd9077515812b0380a2f1953244c861  source/s_normSubnormalF32Sig.c
be1efe0196eacce551f117e58f5d2bc053a4a2897f37fa5805b23e7d4e00de95  source/s_normSubnormalF64Sig.c
91f093c7d2d32f24350fcfb350c1f0a82eec6b2202c7c45ddd38f57f0b62a8f9  source/s_roundPackToF16.c
aac61a425bed4e973c9e0e1fb254f74cb7c04602131509ed98513269da83e45a  source/s_roundPackToF32.c
e549468a71417c8fefd927d582ec8481e2d6f595c406a4db5efff9b462f8e2c8  source/s_roundPackToF64.c
ffb26555c1aba397392ca7f3f3edd0d470203767fd819977b078a29fcc649823  source/s_shiftRightJam128.c
96dee4db4ed95ffe7bd44060f980f0a010003a11c20f5757992dcc1f447e0b62  source/s_shiftRightJam32.c
ea54abfce241315f78b69893f6d7b20ab767e5fe8ade6b107245d8dc0745630f  source/s_shiftRightJam64.c
ea7a5f701460e6385993c1caf6f4a9b9b07a193ffed028f7fa8d5b0b6f1b3582  source/s_shortShiftLeft128.c
3302697be75bb96f76c6a436ddc41e7d583604f63332fee40aee2d10c3298463  source/s_shortShiftRightJam128.c
0fcadc566939b0155100929995ee5234fb40cc146cf7093d6c9c15184aaca00c  source/s_shortShiftRightJam64.c
7a6eed592e83937944e8beb8944da7f2b8ee00568777191d537fe357d96cf762  source/s_sub128.c
53ca2e9f6f1edc7095133e7c98018d69d1e6cd0d5f5b5c0393c4e656ee9a965c  source/s_subMagsF32.c
733f17b90486602ff6c67fa3d99166a841b03276c0fcc7da66751d1e8631f01f  source/s_subMagsF64.c
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
