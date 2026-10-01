//! **Calibration statistics as a file that round-trips exactly** (`misaka.palw.calib-stats.v1`).
//!
//! A conversion's integers are a function of the calibration statistics (a site's absmax sizes
//! its code scale, a channel's absmax picks the outliers), so for anyone to rebuild a registered
//! artifact from its runtime pack the statistics must travel as a file that gives back, bit for
//! bit, the numbers it was written from. JSON floats do not: `serde_json` parses a decimal to the
//! nearest double only approximately (its `float_roundtrip` feature exists because it can be one ulp
//! off), and one ulp of an absmax moves a multiplier's rounding often enough to change a tensor.
//! So this format writes every float as its IEEE bit pattern — an integer, which JSON carries
//! exactly — in a fixed field order, and its digest ([`stats_digest`]) is over that canonical text.
//!
//! The legacy `--stats-out` of `palw-tir-fidelity` (plain `serde` floats) is still read
//! ([`stats_from_json`] recognises it by the absence of a schema), for measuring; a runtime pack
//! accepts only this one ([`stats_from_json_exact`]).

use crate::error::{LowerError, Result};
use crate::float_ref::SiteStat;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CALIB_STATS_SCHEMA_V1: &str = "misaka.palw.calib-stats.v1";

#[derive(Serialize, Deserialize)]
struct SiteStatV1 {
    absmax_bits: u64,
    sum_sq_bits: u64,
    count: u64,
    pos0_absmax_bits: u64,
    rest_absmax_bits: u64,
    chan_absmax_bits: Vec<u32>,
    ragged: bool,
}

#[derive(Serialize, Deserialize)]
struct FileV1 {
    schema: String,
    sites: BTreeMap<String, SiteStatV1>,
}

impl From<&SiteStat> for SiteStatV1 {
    fn from(s: &SiteStat) -> Self {
        SiteStatV1 {
            absmax_bits: s.absmax.to_bits(),
            sum_sq_bits: s.sum_sq.to_bits(),
            count: s.count,
            pos0_absmax_bits: s.pos0_absmax.to_bits(),
            rest_absmax_bits: s.rest_absmax.to_bits(),
            chan_absmax_bits: s.chan_absmax.iter().map(|x| x.to_bits()).collect(),
            ragged: s.ragged,
        }
    }
}

impl From<SiteStatV1> for SiteStat {
    fn from(s: SiteStatV1) -> Self {
        SiteStat {
            absmax: f64::from_bits(s.absmax_bits),
            sum_sq: f64::from_bits(s.sum_sq_bits),
            count: s.count,
            pos0_absmax: f64::from_bits(s.pos0_absmax_bits),
            rest_absmax: f64::from_bits(s.rest_absmax_bits),
            chan_absmax: s.chan_absmax_bits.into_iter().map(f32::from_bits).collect(),
            ragged: s.ragged,
        }
    }
}

/// The canonical text of `stats`: sites in name order, fields in fixed order, floats as bits.
pub fn stats_to_json(stats: &BTreeMap<String, SiteStat>) -> String {
    let f = FileV1 { schema: CALIB_STATS_SCHEMA_V1.into(), sites: stats.iter().map(|(k, v)| (k.clone(), SiteStatV1::from(v))).collect() };
    serde_json::to_string(&f).expect("plain data serialises")
}

/// Read a `misaka.palw.calib-stats.v1` file (exact), or — with a `legacy` fallback — a plain
/// `serde` map of the old `--stats-out` (floats as decimals: not bit-exact, for measuring only).
pub fn stats_from_json_exact(text: &str) -> Result<BTreeMap<String, SiteStat>> {
    let f: FileV1 = serde_json::from_str(text).map_err(|e| LowerError::bad(format!("calibration statistics: {e}")))?;
    if f.schema != CALIB_STATS_SCHEMA_V1 {
        return Err(LowerError::bad(format!("calibration statistics: schema `{}`, not {CALIB_STATS_SCHEMA_V1}", f.schema)));
    }
    Ok(f.sites.into_iter().map(|(k, v)| (k, v.into())).collect())
}

/// [`stats_from_json_exact`], else the legacy map. The second element says which it was.
pub fn stats_from_json(text: &str) -> Result<(BTreeMap<String, SiteStat>, bool)> {
    if text.contains(CALIB_STATS_SCHEMA_V1) {
        return Ok((stats_from_json_exact(text)?, true));
    }
    let legacy: BTreeMap<String, SiteStat> =
        serde_json::from_str(text).map_err(|e| LowerError::bad(format!("calibration statistics (legacy format): {e}")))?;
    Ok((legacy, false))
}

/// `BLAKE2b-256` of the canonical text — the statistics' identity, what a runtime pack pins.
pub fn stats_digest(stats: &BTreeMap<String, SiteStat>) -> [u8; 32] {
    let h = blake2b_simd::Params::new().hash_length(32).key(b"misaka-palw/calib-stats/v1").hash(stats_to_json(stats).as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(h.as_bytes());
    out
}

pub fn stats_digest_hex(stats: &BTreeMap<String, SiteStat>) -> String {
    stats_digest(stats).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BTreeMap<String, SiteStat> {
        let mut m = BTreeMap::new();
        // Values whose decimal round trip is not exact in a best-effort parser: long mantissas.
        let awkward = [0.1f64, 1.0 / 3.0, 5e-324, 1.7976931348623157e308, 2.2250738585072014e-308, 123_456_789.123_456_79];
        for (i, a) in awkward.iter().enumerate() {
            m.insert(
                format!("L{i}.attn.q"),
                SiteStat {
                    absmax: *a,
                    sum_sq: a * 0.5,
                    count: 7 + i as u64,
                    pos0_absmax: a / 3.0,
                    rest_absmax: a * 0.7,
                    chan_absmax: vec![(a.min(1e30) as f32), 1.0 / 7.0, f32::MIN_POSITIVE, 16777217.0],
                    ragged: i % 2 == 0,
                },
            );
        }
        m
    }

    #[test]
    fn statistics_round_trip_bit_for_bit() {
        let s = sample();
        let text = stats_to_json(&s);
        assert!(text.contains(CALIB_STATS_SCHEMA_V1));
        let back = stats_from_json_exact(&text).expect("parses");
        assert_eq!(back.len(), s.len());
        for (k, v) in &s {
            let b = &back[k];
            assert_eq!(b.absmax.to_bits(), v.absmax.to_bits());
            assert_eq!(b.sum_sq.to_bits(), v.sum_sq.to_bits());
            assert_eq!(b.pos0_absmax.to_bits(), v.pos0_absmax.to_bits());
            assert_eq!(b.rest_absmax.to_bits(), v.rest_absmax.to_bits());
            assert_eq!(b.chan_absmax.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), v.chan_absmax.iter().map(|x| x.to_bits()).collect::<Vec<_>>());
            assert_eq!((b.count, b.ragged), (v.count, v.ragged));
        }
        // The text is canonical: writing what was read gives the same text and the same digest.
        assert_eq!(stats_to_json(&back), text);
        assert_eq!(stats_digest(&back), stats_digest(&s));
        let mut other = s.clone();
        other.get_mut("L0.attn.q").expect("site").absmax = f64::from_bits(s["L0.attn.q"].absmax.to_bits() + 1);
        assert_ne!(stats_digest(&other), stats_digest(&s), "one ulp is a different statistic");
    }

    #[test]
    fn the_legacy_map_is_recognised_and_a_foreign_schema_is_refused() {
        let s = sample();
        let legacy = serde_json::to_string(&s).expect("legacy json");
        let (back, exact) = stats_from_json(&legacy).expect("legacy parses");
        assert!(!exact && back.len() == s.len());
        assert!(stats_from_json_exact(&legacy).is_err(), "a pack takes only the exact format");
        let wrong = stats_to_json(&s).replace(CALIB_STATS_SCHEMA_V1, "misaka.palw.calib-stats.v2");
        assert!(stats_from_json_exact(&wrong).is_err());
    }
}
