//! **Calibration**: the per-site absolute maxima of a float run, and the code scales they give.
//!
//! A site is a NAME (`b0.q`, `stream_img`, `cond`) — the same name the float reference ([`super::float`]) notes and the
//! lowering reads, so a scale cannot be attached to the wrong tensor. A site's scale is `amax · headroom / code_max`
//! (`i16` codes: `32767`; `i32` carriers: `2^31 − 1`); the modulation vectors use the largest power of two that still
//! fits (a modulation code times `2^(24 − e)` is its Q24 value: an exact multiply). Calibration is registration-time
//! data; nothing here is in a program.

use std::collections::{BTreeMap, BTreeSet};

use crate::quant::{CODE16_MAX, CODE32_MAX, code_scale};

/// The headroom over the calibrated maximum: a site's range is `amax · HEADROOM`, so a job the calibration set did
/// not see clips later than the set's own extremes.
pub const HEADROOM: f64 = 1.25;

/// Per-site absolute maxima, and (for the sites asked for) the whole tensors.
#[derive(Clone, Debug, Default)]
pub struct Calib {
    pub amax: BTreeMap<String, f64>,
    /// Sites whose tensors are kept (block-by-block checks).
    pub keep: BTreeSet<String>,
    pub kept: BTreeMap<String, Vec<f64>>,
}

impl Calib {
    pub fn new() -> Self {
        Self::default()
    }

    /// Note a tensor at a site: its absolute maximum joins the site's.
    pub fn note(&mut self, site: &str, v: &[f64]) {
        let m = v.iter().fold(0f64, |m, x| m.max(x.abs()));
        let e = self.amax.entry(site.to_string()).or_insert(0.0);
        *e = e.max(m);
        if self.keep.contains(site) {
            self.kept.insert(site.to_string(), v.to_vec());
        }
    }

    /// Fold another run's maxima in (a calibration set is several runs).
    pub fn merge(&mut self, other: &Calib) {
        for (k, v) in &other.amax {
            let e = self.amax.entry(k.clone()).or_insert(0.0);
            *e = e.max(*v);
        }
    }

    /// Make two sites one (the joint attention's q of the image stream and of the text stream share a scale): both
    /// take the larger maximum.
    pub fn unify(&mut self, sites: &[&str]) {
        let m = sites.iter().map(|s| self.amax.get(*s).copied().unwrap_or(0.0)).fold(0f64, f64::max);
        for s in sites {
            self.amax.insert((*s).to_string(), m);
        }
    }

    pub fn amax(&self, site: &str) -> f64 {
        *self.amax.get(site).unwrap_or_else(|| panic!("calibration has no site {site:?}"))
    }

    /// The value of one `i16` code at the site.
    pub fn scale16(&self, site: &str) -> f64 {
        code_scale(self.amax(site), CODE16_MAX, HEADROOM)
    }

    /// The value of one `i32` code at the site.
    pub fn scale32(&self, site: &str) -> f64 {
        code_scale(self.amax(site), CODE32_MAX, HEADROOM)
    }

    /// The exponent `e ∈ [0, 24]` of the power-of-two scale `2^-e` of a modulation site: the largest `e` whose
    /// `i16` range `32767 · 2^-e` holds `amax · HEADROOM`.
    pub fn pow2_exp(&self, site: &str) -> u32 {
        let a = (self.amax(site).max(1e-6)) * HEADROOM;
        let mut e = 24u32;
        while e > 0 && a * (1u64 << e) as f64 > CODE16_MAX {
            e -= 1;
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_keeps_its_largest_value_and_a_scale_holds_it() {
        let mut c = Calib::new();
        c.note("x", &[0.5, -3.0, 2.0]);
        c.note("x", &[1.0]);
        assert_eq!(c.amax("x"), 3.0);
        let s = c.scale16("x");
        assert!((3.0 * HEADROOM / s - CODE16_MAX).abs() < 1e-6, "the extreme (with headroom) is the top code");
        assert!(c.scale32("x") < s);
    }

    #[test]
    fn a_modulation_scale_is_the_largest_power_of_two_that_fits() {
        let mut c = Calib::new();
        c.note("m", &[10.0]);
        let e = c.pow2_exp("m");
        // 10 · 1.25 = 12.5; 12.5 · 2^11 = 25,600 fits, 12.5 · 2^12 = 51,200 does not.
        assert_eq!(e, 11);
        c.note("tiny", &[1e-9]);
        assert_eq!(c.pow2_exp("tiny"), 24, "capped at the Q24 point");
    }

    #[test]
    fn unified_sites_share_a_maximum_and_kept_sites_keep_tensors() {
        let mut c = Calib::new();
        c.keep.insert("a".to_string());
        c.note("a", &[1.0, 2.0]);
        c.note("b", &[5.0]);
        c.unify(&["a", "b"]);
        assert_eq!((c.amax("a"), c.amax("b")), (5.0, 5.0));
        assert_eq!(c.kept["a"], vec![1.0, 2.0]);
    }
}
