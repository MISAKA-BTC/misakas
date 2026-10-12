//! **Deterministic math for the conversion** (`math: "libm-v1"`).
//!
//! A conversion turns floats into integers: activation tables (`silu`, `gelu`, …), RoPE tables
//! (`sin`, `cos`, `powf`), scales (`log2`), a position-dependent temperature (`log1p`). Each of those
//! is a transcendental function, and the standard library hands them to the platform's libm — Apple's
//! on macOS, glibc's or musl's on Linux — which agree to within an ulp and not to the bit. An ulp
//! decides a rounding often enough, over millions of table entries, that two machines converting the
//! same checkpoint would write different artifacts, and the artifact root is what a class is
//! registered under. A seat that cannot rebuild the root from the public source and the pack cannot
//! verify it.
//!
//! So every transcendental the conversion evaluates goes through this module, which in the default
//! mode [`MathMode::LibmV1`] calls the pure-Rust `libm` crate (a port of musl's): software, with no
//! platform code in it, the same bits on every host. `floor(log2 x)` and `ceil(log2 x)` — which
//! decide the binary exponent of a scale — are read off the float's exponent field rather than
//! computed through `log2`, so they are exact. The functions that are already exactly specified
//! (`sqrt`, `round`, `floor`, `+ − × ÷`) are left to the standard library: IEEE makes them
//! bit-identical everywhere.
//!
//! [`MathMode::Std`] keeps the platform's functions: it reproduces artifacts built before this
//! module existed *on the platform that built them* (a runtime pack records the platform with
//! `math: "std"`), and nowhere else.
//!
//! The mode is process-wide and set once, by the converter, before it runs
//! ([`set_mode`]); `libm`'s outputs are pinned bit for bit by this module's tests, so a dependency
//! update that changed one would fail them and require a new mode name, not a silent change.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathMode {
    /// The platform's libm (legacy; reproducible only where it was built).
    Std,
    /// The pure-Rust `libm` crate, and exact exponent extraction: the same on every platform.
    LibmV1,
}

impl MathMode {
    pub fn name(self) -> &'static str {
        match self {
            MathMode::Std => "std",
            MathMode::LibmV1 => "libm-v1",
        }
    }
    pub fn parse(s: &str) -> Option<MathMode> {
        match s {
            "std" => Some(MathMode::Std),
            "libm-v1" => Some(MathMode::LibmV1),
            _ => None,
        }
    }
}

static MODE: AtomicU8 = AtomicU8::new(1);

/// The mode in force (default: [`MathMode::LibmV1`]).
pub fn mode() -> MathMode {
    if MODE.load(Ordering::Relaxed) == 0 { MathMode::Std } else { MathMode::LibmV1 }
}

/// Set the mode for the process. Call it before a conversion starts, not while one runs.
pub fn set_mode(m: MathMode) {
    MODE.store(matches!(m, MathMode::LibmV1) as u8, Ordering::Relaxed);
}

/// Where this build runs, for a pack that records `math: "std"`: the platform whose libm made the
/// numbers (`os`, `arch`, and the C library family where it matters).
pub fn platform() -> String {
    let env = if cfg!(target_env = "gnu") {
        "gnu"
    } else if cfg!(target_env = "musl") {
        "musl"
    } else if cfg!(target_env = "msvc") {
        "msvc"
    } else {
        ""
    };
    let mut s = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    if !env.is_empty() {
        s.push('-');
        s.push_str(env);
    }
    s
}

macro_rules! f64_fn {
    ($(#[$doc:meta])* $name:ident, $std:ident, $libm:ident) => {
        $(#[$doc])*
        pub fn $name(x: f64) -> f64 {
            match mode() {
                MathMode::Std => x.$std(),
                MathMode::LibmV1 => libm::$libm(x),
            }
        }
    };
}

f64_fn!(exp, exp, exp);
f64_fn!(ln, ln, log);
f64_fn!(ln_1p, ln_1p, log1p);
f64_fn!(tanh, tanh, tanh);
f64_fn!(sin, sin, sin);
f64_fn!(cos, cos, cos);

pub fn powf(x: f64, y: f64) -> f64 {
    match mode() {
        MathMode::Std => x.powf(y),
        MathMode::LibmV1 => libm::pow(x, y),
    }
}

pub fn ln_f32(x: f32) -> f32 {
    match mode() {
        MathMode::Std => x.ln(),
        MathMode::LibmV1 => libm::logf(x),
    }
}

pub fn ln_1p_f32(x: f32) -> f32 {
    match mode() {
        MathMode::Std => x.ln_1p(),
        MathMode::LibmV1 => libm::log1pf(x),
    }
}

pub fn sin_f32(x: f32) -> f32 {
    match mode() {
        MathMode::Std => x.sin(),
        MathMode::LibmV1 => libm::sinf(x),
    }
}

pub fn cos_f32(x: f32) -> f32 {
    match mode() {
        MathMode::Std => x.cos(),
        MathMode::LibmV1 => libm::cosf(x),
    }
}

/// `floor(log2 x)` as an `f64` (`−∞` for 0, `+∞` for ∞, NaN for a negative or NaN `x`, so a caller's
/// `clamp` behaves as it did with `x.log2().floor()`). In [`MathMode::LibmV1`] it is the exact binary
/// exponent of `x`.
pub fn floor_log2(x: f64) -> f64 {
    match mode() {
        MathMode::Std => x.log2().floor(),
        MathMode::LibmV1 => exact_floor_log2(x),
    }
}

/// `ceil(log2 x)` (see [`floor_log2`]).
pub fn ceil_log2(x: f64) -> f64 {
    match mode() {
        MathMode::Std => x.log2().ceil(),
        MathMode::LibmV1 => {
            let f = exact_floor_log2(x);
            if !f.is_finite() {
                return f;
            }
            // A power of two is its own ceiling.
            let bits = x.to_bits();
            let m = bits & ((1u64 << 52) - 1);
            let pow2 = if (bits >> 52) & 0x7ff == 0 { m.is_power_of_two() } else { m == 0 };
            if pow2 { f } else { f + 1.0 }
        }
    }
}

fn exact_floor_log2(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return f64::INFINITY;
    }
    let bits = x.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i64;
    let m = bits & ((1u64 << 52) - 1);
    if e == 0 {
        // Subnormal: m · 2^−1074.
        (63 - m.leading_zeros() as i64 - 1074) as f64
    } else {
        (e - 1023) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialises the tests that change the process-wide mode.
    pub(crate) static MODE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn exact_exponents_are_exact() {
        let _g = MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_mode(MathMode::LibmV1);
        for (x, f, c) in [
            (1.0, 0.0, 0.0),
            (2.0, 1.0, 1.0),
            (3.0, 1.0, 2.0),
            (0.5, -1.0, -1.0),
            (0.75, -1.0, 0.0),
            (1023.999, 9.0, 10.0),
            (1024.0, 10.0, 10.0),
            (f64::MIN_POSITIVE, -1022.0, -1022.0),
            (f64::MIN_POSITIVE / 4.0, -1024.0, -1024.0),
            (5e-324, -1074.0, -1074.0),
        ] {
            assert_eq!((floor_log2(x), ceil_log2(x)), (f, c), "{x:e}");
        }
        // The value `log2` rounds up across an integer: 2^k · (1 − 2^−53) is below 2^k, so its floor is k − 1.
        let below = f64::from_bits(2.0f64.to_bits() - 1);
        assert_eq!((floor_log2(below), ceil_log2(below)), (0.0, 1.0));
        assert_eq!(floor_log2(0.0), f64::NEG_INFINITY);
        assert_eq!(floor_log2(f64::INFINITY), f64::INFINITY);
        assert!(floor_log2(-1.0).is_nan());
        // Agrees with the float function wherever that is not within an ulp of an integer.
        set_mode(MathMode::Std);
        for x in [0.3f64, 1.7, 12.5, 1e-9, 7e14, 0.999] {
            set_mode(MathMode::Std);
            let s = (floor_log2(x), ceil_log2(x));
            set_mode(MathMode::LibmV1);
            assert_eq!(s, (floor_log2(x), ceil_log2(x)), "{x}");
        }
        set_mode(MathMode::LibmV1);
    }

    #[test]
    fn libm_agrees_with_the_platform_to_an_ulp_or_two() {
        let _g = MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let near = |a: f64, b: f64| (a - b).abs() <= 4.0 * f64::EPSILON * a.abs().max(b.abs()).max(1e-300);
        for i in 0..400 {
            let x = -20.0 + 0.1 * i as f64;
            let y = 0.01 + 0.07 * i as f64;
            set_mode(MathMode::Std);
            let s = (exp(x), tanh(x), sin(x), cos(x), ln(y), ln_1p(y), powf(y, x / 7.0));
            set_mode(MathMode::LibmV1);
            let l = (exp(x), tanh(x), sin(x), cos(x), ln(y), ln_1p(y), powf(y, x / 7.0));
            assert!(near(s.0, l.0) && near(s.1, l.1) && near(s.2, l.2) && near(s.3, l.3) && near(s.4, l.4) && near(s.5, l.5) && near(s.6, l.6), "{x} {y}: {s:?} vs {l:?}");
        }
        set_mode(MathMode::LibmV1);
    }

    /// `libm`'s outputs, pinned bit for bit. They are the same on every platform (software, no
    /// platform code); if a dependency update ever changed one, this fails and the change needs a new
    /// mode name (`libm-v2`), not a silent one.
    #[test]
    fn libm_v1_is_pinned_bit_for_bit() {
        let _g = MODE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_mode(MathMode::LibmV1);
        let xs = [0.1f64, 0.5, 1.0, 1.7320508075688772, 2.5, std::f64::consts::PI, 7.25, 40.0, 1e-5, 123.456];
        let mut h = blake2b_simd::Params::new().hash_length(32).to_state();
        for &x in &xs {
            for v in [exp(-x), ln(x), ln_1p(x), tanh(x), sin(x * 3.0), cos(x * 3.0), powf(x, 0.37), powf(10000.0, -x / 64.0)] {
                h.update(&v.to_bits().to_le_bytes());
            }
            for v in [ln_f32(x as f32), ln_1p_f32(x as f32), sin_f32(x as f32 * 3.0), cos_f32(x as f32 * 3.0)] {
                h.update(&v.to_bits().to_le_bytes());
            }
        }
        let got: String = h.finalize().as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(got, PINNED, "libm-v1 changed: a new mode name is needed");
    }

    const PINNED: &str = "556a7d8266e4340e720c13f596fe5f244170f5b1b3adb5c856bf7c2981a49150";
}

/// **`Φ⁻¹(p)`**, the standard normal quantile (`torch.distributions.Normal(0, 1).icdf`), for `p` in `(0, 1)`: bisection on
/// `Φ(z) = ½(1 + erf(z/√2))` with the series `erf` of the float reference, which is plain `+ − × ÷` below `|x| = 3` — so the
/// value is the same on every platform (a registration-time constant of `FFN_ACTIVATION_SPARSITY_V1` is part of the program).
/// Accurate to ~1e-13 absolute for `p` in `[1e-3, 1 − 1e-3]`; `None` outside `(0, 1)`.
pub fn norm_inv_cdf(p: f64) -> Option<f64> {
    if !(p > 0.0 && p < 1.0) {
        return None;
    }
    if p < 0.5 {
        return norm_inv_cdf(1.0 - p).map(|z| -z);
    }
    let (mut lo, mut hi) = (0.0f64, 9.0f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        let phi = 0.5 * (1.0 + crate::float_ref::erf(mid / std::f64::consts::SQRT_2));
        if phi < p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(0.5 * (lo + hi))
}
