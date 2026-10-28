/*
 * MISAKA shim platform.h for the vendored Berkeley SoftFloat-3e subset.
 *
 * This is the ONE file of ours in the C build (analogous to the per-target
 * `build/<target>/platform.h` upstream expects); everything under
 * ../vendor/softfloat/ is byte-identical to upstream. All other configuration
 * is done via -I include paths and -D defines in build.rs.
 *
 * Choices, relative to upstream's reference `build/Linux-x86_64-GCC/platform.h`:
 *
 *  - INLINE is `static inline` (upstream GCC build uses C99 extern-inline
 *    semantics). With plain C99 `inline`, a compiler at -O0 may emit calls to
 *    the out-of-line symbol, and one primitive our subset inlines from
 *    primitives.h (softfloat_countLeadingZeros32) has no vendored .c backing.
 *    `static inline` gives every translation unit its own definition, which is
 *    correct at any optimization level and with any C compiler. It cannot
 *    change results: these are exact integer helper functions.
 *
 *  - No SOFTFLOAT_BUILTIN_CLZ / SOFTFLOAT_INTRINSIC_INT128 / opts-GCC.h.
 *    The portable table-driven paths (s_countLeadingZeros8.c) are used
 *    instead, keeping the build compiler-agnostic. Performance is irrelevant
 *    here: this library exists only to be differentially tested.
 *
 *  - LITTLEENDIAN comes from the compiler's own byte-order macro (build.rs
 *    additionally refuses big-endian targets outright). It only affects
 *    multi-word struct layouts (uint128 etc.) that the f32-only subset never
 *    executes, but it is set correctly anyway.
 */

#ifndef MISAKA_SOFTFLOAT_SHIM_PLATFORM_H
#define MISAKA_SOFTFLOAT_SHIM_PLATFORM_H

#if defined(__BYTE_ORDER__) && (__BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__)
#define LITTLEENDIAN 1
#elif defined(_MSC_VER) || defined(__x86_64__) || defined(__i386__) || defined(__aarch64__)
#define LITTLEENDIAN 1
#endif

#ifdef _MSC_VER
#define INLINE static __inline
#else
#define INLINE static inline
#endif

#endif /* MISAKA_SOFTFLOAT_SHIM_PLATFORM_H */
