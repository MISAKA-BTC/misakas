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
