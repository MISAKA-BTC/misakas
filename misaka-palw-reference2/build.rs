//! Compiles the vendored Berkeley SoftFloat-3e f32 subset (see vendor/softfloat/).
//!
//! The vendored sources are byte-identical to upstream; ALL configuration happens
//! here (defines + include paths) and in shim/platform.h (the one file of ours in
//! the C build). Defines mirror the upstream reference build
//! (build/Linux-x86_64-GCC/Makefile): -DSOFTFLOAT_FAST_INT64, -DINLINE_LEVEL=5,
//! -DSOFTFLOAT_ROUND_ODD. Upstream's -DSOFTFLOAT_FAST_DIV32TO16 /
//! -DSOFTFLOAT_FAST_DIV64TO32 are omitted: they only touch division code, and no
//! division is vendored.

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
        // Shift/CLZ primitives. At INLINE_LEVEL=5 the callers use the
        // `static inline` definitions from primitives.h; these out-of-line
        // fallbacks are compiled anyway, exactly as the upstream build does.
        "vendor/softfloat/source/s_shortShiftRightJam64.c",
        "vendor/softfloat/source/s_shiftRightJam32.c",
        "vendor/softfloat/source/s_shiftRightJam64.c",
        "vendor/softfloat/source/s_countLeadingZeros8.c", // the CLZ lookup table
        // Global state: softfloat_roundingMode / softfloat_detectTininess /
        // softfloat_exceptionFlags definitions (referenced by roundPack/raiseFlags).
        "vendor/softfloat/source/softfloat_state.c",
        // 8086-SSE specialization: NaN propagation rules + raiseFlags. The Rust
        // wrapper canonicalizes NaNs on both sides of the call, so no
        // specialization-specific payload behavior can reach a test result.
        "vendor/softfloat/source/8086-SSE/s_propagateNaNF32UI.c",
        "vendor/softfloat/source/8086-SSE/softfloat_raiseFlags.c",
    ];

    let mut build = cc::Build::new();
    build
        .include("shim") // our platform.h (INLINE / LITTLEENDIAN config)
        .include("vendor/softfloat/source/include")
        .include("vendor/softfloat/source/8086-SSE")
        .define("SOFTFLOAT_FAST_INT64", None)
        .define("INLINE_LEVEL", "5")
        .define("SOFTFLOAT_ROUND_ODD", None)
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
