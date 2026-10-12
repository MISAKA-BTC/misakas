//! Compiles the vendored Berkeley SoftFloat-3e subset (see vendor/softfloat/).
//!
//! The vendored sources are byte-identical to upstream; ALL configuration happens
//! here (defines + include paths) and in shim/platform.h (the one file of ours in
//! the C build). Defines mirror the upstream reference build
//! (build/Linux-x86_64-GCC/Makefile): -DSOFTFLOAT_FAST_INT64, -DINLINE_LEVEL=5,
//! -DSOFTFLOAT_ROUND_ODD, -DSOFTFLOAT_FAST_DIV32TO16, -DSOFTFLOAT_FAST_DIV64TO32
//! (the division defines joined when the ruleset-v2 division paths were vendored;
//! before that no division was vendored and they were omitted).

fn main() {
    // The shim platform.h derives LITTLEENDIAN from compiler macros; refuse
    // big-endian targets outright rather than silently building an untested
    // configuration. (The f32 paths are endian-independent, but fail closed.)
    let endian = std::env::var("CARGO_CFG_TARGET_ENDIAN").expect("cargo sets CARGO_CFG_TARGET_ENDIAN");
    assert_eq!(endian, "little", "misaka-palw-reference2 vendored-softfloat build is only validated on little-endian targets");

    let sources = [
        // f32 operations + the internal helpers they call.
        "vendor/softfloat/source/f32_add.c",
        "vendor/softfloat/source/f32_sub.c",
        "vendor/softfloat/source/f32_mul.c",
        "vendor/softfloat/source/s_addMagsF32.c",
        "vendor/softfloat/source/s_subMagsF32.c",
        "vendor/softfloat/source/s_normRoundPackToF32.c",
        "vendor/softfloat/source/s_roundPackToF32.c",
        "vendor/softfloat/source/s_normSubnormalF32Sig.c",
        // Ruleset-v2 f32 operations: fused multiply-add, division, square root.
        "vendor/softfloat/source/f32_mulAdd.c",
        "vendor/softfloat/source/s_mulAddF32.c",
        "vendor/softfloat/source/f32_div.c",
        "vendor/softfloat/source/f32_sqrt.c",
        // Ruleset-v2 f64 operations + their round/normalize/magnitude helpers.
        "vendor/softfloat/source/f64_add.c",
        "vendor/softfloat/source/f64_sub.c",
        "vendor/softfloat/source/f64_mul.c",
        "vendor/softfloat/source/f64_div.c",
        "vendor/softfloat/source/f64_mulAdd.c",
        "vendor/softfloat/source/s_addMagsF64.c",
        "vendor/softfloat/source/s_subMagsF64.c",
        "vendor/softfloat/source/s_mulAddF64.c",
        "vendor/softfloat/source/s_normRoundPackToF64.c",
        "vendor/softfloat/source/s_roundPackToF64.c",
        "vendor/softfloat/source/s_normSubnormalF64Sig.c",
        // Ruleset-v2 width conversions (f32↔f64, f16↔f32) + the f16 helpers.
        "vendor/softfloat/source/f32_to_f64.c",
        "vendor/softfloat/source/f64_to_f32.c",
        "vendor/softfloat/source/f16_to_f32.c",
        "vendor/softfloat/source/f32_to_f16.c",
        "vendor/softfloat/source/s_roundPackToF16.c",
        "vendor/softfloat/source/s_normSubnormalF16Sig.c",
        // Shift/CLZ primitives. At INLINE_LEVEL=5 the callers use the
        // `static inline` definitions from primitives.h; these out-of-line
        // fallbacks are compiled anyway, exactly as the upstream build does.
        "vendor/softfloat/source/s_shortShiftRightJam64.c",
        "vendor/softfloat/source/s_shiftRightJam32.c",
        "vendor/softfloat/source/s_shiftRightJam64.c",
        "vendor/softfloat/source/s_countLeadingZeros8.c", // the CLZ lookup table
        "vendor/softfloat/source/s_countLeadingZeros16.c",
        "vendor/softfloat/source/s_countLeadingZeros64.c",
        // 128-bit primitives used by the FAST_INT64 f64 paths.
        "vendor/softfloat/source/s_mul64To128.c",
        "vendor/softfloat/source/s_add128.c",
        "vendor/softfloat/source/s_sub128.c",
        "vendor/softfloat/source/s_shortShiftLeft128.c",
        "vendor/softfloat/source/s_shortShiftRightJam128.c",
        "vendor/softfloat/source/s_shiftRightJam128.c",
        // Division/sqrt seed approximations. With SOFTFLOAT_FAST_DIV64TO32 defined,
        // softfloat_approxRecip32_1 becomes a macro and this object goes unreferenced;
        // it is compiled anyway so the source set stays valid under either choice.
        "vendor/softfloat/source/s_approxRecip32_1.c",
        "vendor/softfloat/source/s_approxRecipSqrt32_1.c",
        "vendor/softfloat/source/s_approxRecip_1Ks.c", // seed tables for approxRecip32_1
        "vendor/softfloat/source/s_approxRecipSqrt_1Ks.c", // seed tables for approxRecipSqrt32_1
        // Global state: softfloat_roundingMode / softfloat_detectTininess /
        // softfloat_exceptionFlags definitions (referenced by roundPack/raiseFlags).
        "vendor/softfloat/source/softfloat_state.c",
        // 8086-SSE specialization: NaN propagation rules + raiseFlags. The Rust
        // wrapper canonicalizes NaNs on both sides of the call, so no
        // specialization-specific payload behavior can reach a test result.
        "vendor/softfloat/source/8086-SSE/s_propagateNaNF32UI.c",
        "vendor/softfloat/source/8086-SSE/softfloat_raiseFlags.c",
        "vendor/softfloat/source/8086-SSE/s_propagateNaNF64UI.c",
        // commonNaN carriers for the width conversions (f16/f32/f64 in either direction).
        "vendor/softfloat/source/8086-SSE/s_f16UIToCommonNaN.c",
        "vendor/softfloat/source/8086-SSE/s_f32UIToCommonNaN.c",
        "vendor/softfloat/source/8086-SSE/s_f64UIToCommonNaN.c",
        "vendor/softfloat/source/8086-SSE/s_commonNaNToF16UI.c",
        "vendor/softfloat/source/8086-SSE/s_commonNaNToF32UI.c",
        "vendor/softfloat/source/8086-SSE/s_commonNaNToF64UI.c",
    ];

    let mut build = cc::Build::new();
    build
        .include("shim") // our platform.h (INLINE / LITTLEENDIAN config)
        .include("vendor/softfloat/source/include")
        .include("vendor/softfloat/source/8086-SSE")
        .define("SOFTFLOAT_FAST_INT64", None)
        .define("INLINE_LEVEL", "5")
        .define("SOFTFLOAT_ROUND_ODD", None)
        .define("SOFTFLOAT_FAST_DIV32TO16", None)
        .define("SOFTFLOAT_FAST_DIV64TO32", None)
        // Vendored third-party code compiled as-is; don't spray its (benign)
        // warnings over our build output.
        .warnings(false);
    for src in sources {
        build.file(src);
    }
    build.compile("misaka_palw_softfloat_ref2");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=shim/platform.h");
    println!("cargo:rerun-if-changed=vendor/softfloat");
}
