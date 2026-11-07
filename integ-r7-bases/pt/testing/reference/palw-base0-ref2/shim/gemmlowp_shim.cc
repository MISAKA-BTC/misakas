// The ONLY file of ours in the gemmlowp C++ build. Everything under vendor/gemmlowp/ is
// byte-identical to upstream and is never edited.
//
// # What this file is allowed to do
//
// Expose gemmlowp's two scalar fixed-point primitives to Rust through `extern "C"`, and nothing
// else. It must not reimplement, adjust, clamp, or pre-condition anything: the whole point of
// vendoring is that the answers come from code this project did not write, so any arithmetic added
// here would be arithmetic the differential is no longer testing against a third party.
//
// Concretely — there is no `if` in either wrapper. The `i32::MIN × i32::MIN` saturation, the
// asymmetric rounding nudge, and the truncating division are all upstream's, reached by calling
// upstream's function and returning what it returns.
//
// # The one deliberate difference from a plain call
//
// `RoundingDivideByPOT` is a template over `IntegerType`, and instantiating it at `std::int32_t`
// selects the scalar path — the same path a SIMD build would have to agree with lane-wise. The
// explicit template argument is written out rather than left to deduction so that a future
// vendored header adding an overload cannot silently re-point this call at a different function.

#include <cstdint>

#include "fixedpoint/fixedpoint.h"

extern "C" {

// gemmlowp's `SaturatingRoundingDoublingHighMul`, unmodified.
//
// The name is prefixed rather than bare so a link-time collision with any other vendored
// fixed-point library is a build error instead of a silent substitution of one implementation for
// the other — which, in a crate whose only purpose is comparing implementations, would be the
// worst possible failure mode.
std::int32_t misaka_gemmlowp_srdhm(std::int32_t a, std::int32_t b) {
  return gemmlowp::SaturatingRoundingDoublingHighMul(a, b);
}

// gemmlowp's `RoundingDivideByPOT`, unmodified.
//
// Upstream asserts `0 <= exponent <= 31`. The assert is upstream's contract, not ours to widen, so
// the Rust side is what refuses an out-of-range exponent before calling — leaving it to fire here
// would make the oracle's behaviour depend on whether `NDEBUG` was set, and an oracle that answers
// differently in release than in debug is not an oracle.
std::int32_t misaka_gemmlowp_rounding_divide_by_pot(std::int32_t x, std::int32_t exponent) {
  return gemmlowp::RoundingDivideByPOT<std::int32_t>(x, exponent);
}

}  // extern "C"
