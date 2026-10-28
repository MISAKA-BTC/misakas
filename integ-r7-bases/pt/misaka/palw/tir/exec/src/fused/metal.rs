//! **The Metal backend of the fused unit-row kernels (RFC-0002 §7 F-6)** — `l2_unit_q15` and `rms_unit_q24` on the GPU, behind a
//! node flag (`--palw-tir-metal`), bit-identical to the CPU kernels and so to the reference.
//!
//! Every primitive of these templates is integer and every sum order-free, so a GPU computes exactly the integers the reference
//! does (F-6); this module is one more [`super::FusedKernelV1::run`] body, held by the same gate (`tests/metal_gate.rs`, and the
//! gate of `tir-lower`'s `fused_gate` with the backend on). Like every fused kernel it is outside the identity (F-3): no object,
//! class id or fingerprint reads it, and a node without it, or with the flag off, computes the same bytes on the CPU.
//!
//! The kernel (`metal/unit_rows.metal`) computes one row per threadgroup: the exact sum of squares by a tree reduction, the scalar
//! chain once, and the outputs strided. It computes in 64-bit words; the one place the template needs more (`Σx² · 2^24 / n` in
//! `i128`) is split into a quotient and a remainder, and a row whose result leaves the machine word makes the kernel raise a flag,
//! on which this module returns `None` and the CPU kernel computes the call — so the GPU never answers a value it did not compute
//! exactly. Compiled only with the `metal` feature on macOS; otherwise every function answers "not available".

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static ON: AtomicBool = AtomicBool::new(false);
/// Calls the GPU answered, and calls it handed back (a row beyond its word): for the gate and the operator's log.
static ANSWERED: AtomicUsize = AtomicUsize::new(0);
static HANDED_BACK: AtomicUsize = AtomicUsize::new(0);
static MIN_ELEMS: AtomicUsize = AtomicUsize::new(DEFAULT_MIN_ELEMS);

/// A call smaller than this runs on the CPU: a GPU dispatch costs more than the whole row work below it (measured, the doc of
/// `tir-exec-bench --metal`). Tests set it to 0.
pub const DEFAULT_MIN_ELEMS: usize = 1 << 15;

#[cfg(all(feature = "metal", target_os = "macos"))]
mod ffi {
    unsafe extern "C" {
        pub fn tir_metal_available() -> i32;
        pub fn tir_metal_gdn(
            s_now: *const i32,
            k: *const i32,
            v: *const i32,
            q: *const i32,
            p: *const i64,
            h: u32,
            dv: u32,
            dk: u32,
            s_next: *mut i32,
            out: *mut i32,
        ) -> i32;
        pub fn tir_metal_unit_rows(kind: i32, x: *const i32, rows: u32, n: u32, eps: i64, out: *mut i32) -> i32;
    }
}

/// Whether this build carries the backend and the machine has a Metal device.
pub fn metal_available() -> bool {
    #[cfg(all(feature = "metal", target_os = "macos"))]
    {
        // SAFETY: no arguments; the bridge serialises its own state.
        unsafe { ffi::tir_metal_available() == 1 }
    }
    #[cfg(not(all(feature = "metal", target_os = "macos")))]
    {
        false
    }
}

/// **Turn the backend on or off** (the node's `--palw-tir-metal`). `Err` names why it cannot be on: this build has none, or no device.
pub fn set_metal_backend(on: bool) -> Result<(), String> {
    if on && !metal_available() {
        return Err(if cfg!(all(feature = "metal", target_os = "macos")) {
            "no Metal device on this machine".into()
        } else {
            "this build carries no Metal backend (build with the `metal` feature on macOS)".into()
        });
    }
    ON.store(on, Ordering::SeqCst);
    Ok(())
}

/// `(answered by the GPU, handed back to the CPU)` since the process started.
pub fn metal_counts() -> (usize, usize) {
    (ANSWERED.load(Ordering::Relaxed), HANDED_BACK.load(Ordering::Relaxed))
}

pub fn metal_enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

/// The smallest call (in elements) the GPU takes; the rest run on the CPU.
pub fn set_metal_min_elems(n: usize) {
    MIN_ELEMS.store(n, Ordering::SeqCst);
}

/// `kind` 0 = `l2_unit_q15`, 1 = `rms_unit_q24`. `x` is `rows * n` elements (each an `i16`/`i32` code). `None`: not taken (flag off,
/// too small, no backend, or a row beyond the kernel's word) — the caller computes it on the CPU.
pub(crate) fn unit_rows(kind: i32, x: &[i128], n: usize, eps: i128) -> Option<Vec<i128>> {
    if !metal_enabled() || n == 0 || x.len() < MIN_ELEMS.load(Ordering::Relaxed) {
        return None;
    }
    #[cfg(all(feature = "metal", target_os = "macos"))]
    {
        let rows = x.len() / n;
        let (rows32, n32) = (u32::try_from(rows).ok()?, u32::try_from(n).ok()?);
        let lanes: Vec<i32> = x.iter().map(|v| i32::try_from(*v).ok()).collect::<Option<_>>()?;
        let mut out = vec![0i32; lanes.len()];
        let eps = i64::try_from(eps).ok()?;
        // SAFETY: `lanes` and `out` hold `rows * n` elements, which the bridge reads and writes and no more.
        let rc = unsafe { ffi::tir_metal_unit_rows(kind, lanes.as_ptr(), rows32, n32, eps, out.as_mut_ptr()) };
        match rc {
            0 => {
                ANSWERED.fetch_add(1, Ordering::Relaxed);
                Some(out.into_iter().map(i128::from).collect())
            }
            1 => {
                HANDED_BACK.fetch_add(1, Ordering::Relaxed);
                None
            }
            _ => None,
        }
    }
    #[cfg(not(all(feature = "metal", target_os = "macos")))]
    {
        let _ = (kind, eps);
        None
    }
}

/// **The gated delta rule's position on the GPU** (`gdn_step_q36`): `None` unless the backend is on, the call is large enough, and every
/// operand is in the range the kernel computes in (the state `i32`; every per-head scalar a 64-bit word; every divisor a power of two
/// up to `2^62`) — then the integers are the CPU kernel's. `heads[i]` is `(decay, beta, rm, rd, rz, dm, dd, dz, write_shift, om, od, oz)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gdn_step(
    s_now: &[i32],
    k: &[i64],
    v: &[i64],
    q: &[i64],
    dims: (usize, usize, usize),
    state_range: (i64, i64),
    heads: &[[i128; 12]],
) -> Option<(Vec<i32>, Vec<i128>)> {
    if !metal_enabled() || s_now.len() < MIN_ELEMS.load(Ordering::Relaxed) {
        return None;
    }
    #[cfg(all(feature = "metal", target_os = "macos"))]
    {
        let (h, dv, dk) = dims;
        if heads.len() != h || s_now.len() != h * dv * dk {
            return None;
        }
        let word = |x: i128| i64::try_from(x).ok();
        let pow2_exp = |d: i128| (d > 0 && (d & (d - 1)) == 0 && d.trailing_zeros() <= 62).then(|| d.trailing_zeros() as i64);
        let i32s = |a: &[i64]| a.iter().map(|x| i32::try_from(*x).ok()).collect::<Option<Vec<i32>>>();
        let (k32, v32, q32) = (i32s(k)?, i32s(v)?, i32s(q)?);
        let mut p: Vec<i64> = vec![h as i64, dv as i64, dk as i64, state_range.0, state_range.1];
        for field in 0..12 {
            for hd in heads {
                p.push(match field {
                    3 | 6 | 10 => pow2_exp(hd[field])?,
                    _ => word(hd[field])?,
                });
            }
        }
        let mut s_next = vec![0i32; s_now.len()];
        let mut out = vec![0i32; h * dv];
        // SAFETY: every buffer holds the element count the bridge reads and writes (`h·dv·dk`, `h·dk`, `h·dv`, `5 + 12·h`).
        let rc = unsafe {
            ffi::tir_metal_gdn(
                s_now.as_ptr(),
                k32.as_ptr(),
                v32.as_ptr(),
                q32.as_ptr(),
                p.as_ptr(),
                u32::try_from(h).ok()?,
                u32::try_from(dv).ok()?,
                u32::try_from(dk).ok()?,
                s_next.as_mut_ptr(),
                out.as_mut_ptr(),
            )
        };
        if rc != 0 {
            return None;
        }
        ANSWERED.fetch_add(1, Ordering::Relaxed);
        Some((s_next, out.into_iter().map(i128::from).collect()))
    }
    #[cfg(not(all(feature = "metal", target_os = "macos")))]
    {
        let _ = (s_now, k, v, q, dims, state_range, heads);
        None
    }
}

/// `rms_norm_wide_q36`'s call: `ε` arrives as `eps_zero · 2^eps_shift` already (`None` past 63 bits, which the CPU answers).
pub(crate) fn rms_wide_rows(x: &[i128], n: usize, eps: i128) -> Option<Vec<i128>> {
    if eps >= 1 << 62 {
        return None;
    }
    unit_rows(2, x, n, eps)
}
