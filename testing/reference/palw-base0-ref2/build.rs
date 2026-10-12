//! Compiles `shim/gemmlowp_shim.cc` against the vendored gemmlowp headers (see vendor/gemmlowp/).
//!
//! gemmlowp's fixed-point layer is header-only, so there is nothing of upstream's to compile — the
//! only translation unit is our shim, and everything under `vendor/gemmlowp/` is included
//! byte-identically. That is a stronger position than reference2's vendored SoftFloat, where a
//! build configuration has to be reproduced: here there is no configuration to get wrong.
//!
//! No `-march`/`-mavx` flags are passed, deliberately. `detect_platform.h` selects a SIMD header
//! from the compiler's own macros, and forcing a wider instruction set would mean the oracle was
//! built differently from how a consumer of this crate would build it. The scalar `std::int32_t`
//! path this crate calls is the same in every configuration; letting the target decide keeps that
//! claim testable rather than assumed.

fn main() {
    cc::Build::new()
        .cpp(true)
        .std("c++11")
        .file("shim/gemmlowp_shim.cc")
        .include("vendor/gemmlowp")
        // Vendored third-party headers compiled as-is; their (benign) warnings are not ours to fix,
        // and fixing them would mean editing files that must stay byte-identical to upstream.
        .warnings(false)
        .compile("misaka_palw_base0_gemmlowp");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=shim/gemmlowp_shim.cc");
    println!("cargo:rerun-if-changed=vendor/gemmlowp");
}
