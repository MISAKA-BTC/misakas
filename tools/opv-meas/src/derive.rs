//! `derive`: the OPV window and collateral formulas, EXECUTABLE (RFC-0015 §6.1, §8.1; the user's 2026-10-08 ruling
//! `T_challenge >= T_beacon + T_fetch + T_check + T_localize + T_file + T_margin`, every term measured), over inputs a measurement
//! gives. Pure arithmetic: no consensus code calls it, no parameter is chosen by it. The kernel's own `OpvPolicyV1::validate` is run on
//! the budgets it derives, so a derived window is judged by the relations the node enforces, not by this file.
//!
//! ```text
//! opv-meas derive --in inputs.json        # {"s_per_daa": 120, "classes": [ {name, positions, ...} ], "economics": {...}}
//! ```
//!
//! Time, per class (seconds; `m` positions are checked, `P` is the claim's positions):
//!
//! ```text
//! T_fetch(m)  = latency + (artifact_bytes + m * position_bytes) / bandwidth
//! T_check(m)  = (check_fixed + m * check_per_position) / check_speedup
//! T_challenge(m) = (T_beacon + T_fetch(m) + T_check(m) + T_localize + T_file) * (1 + margin_frac) + margin_s
//!                  + carrier + reorg_slack                       # the chain's two budgets, not measured here
//! ```
//!
//! Money (BILI as sompi; ADR-0174), with `P_dc` the probability that a fraud is detected AND convicted:
//!
//! ```text
//! coverage c = m / P (one honest verifier, positions drawn uniformly and unpredictably)
//! P_det      = 1 - (1 - q * c)^n            (n independent verifiers, each present with probability q)
//! P_dc       = P_det * (1 - eps_enf)
//! reservation >= ceil( max(gain + default_penalty, ceil(gain / P_dc)) / (1 - accuser) )     # none when P_dc = 0
//! ```
use crate::util::*;
use kaspa_hashes::Hash64;
use misaka_palw_kernel::opv::OpvBudgetsV1;
use serde_json::{Value, json};

const BILI: u128 = 100_000_000; // sompi per BILI (the legacy constant name SOMPI_PER_KASPA), ADR-0174

/// A class's measured or modelled inputs.
#[derive(Clone, Debug)]
pub struct ClassIn {
    pub name: String,
    pub positions: u64,
    pub artifact_bytes: f64,
    pub position_bytes: f64,
    pub bandwidth_bps: f64,
    pub latency_s: f64,
    pub check_fixed_s: f64,
    pub check_per_position_s: f64,
    pub check_speedup: f64,
    pub localize_s: f64,
    pub file_s: f64,
    pub beacon_s: f64,
    pub margin_frac: f64,
    pub margin_s: f64,
}

impl ClassIn {
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let f = |k: &str, d: f64| v.get(k).and_then(Value::as_f64).unwrap_or(d);
        Ok(Self {
            name: v.get("name").and_then(Value::as_str).ok_or("class.name")?.to_string(),
            positions: v.get("positions").and_then(Value::as_u64).ok_or("class.positions")?,
            artifact_bytes: f("artifact_bytes", 0.0),
            position_bytes: f("position_bytes", 0.0),
            bandwidth_bps: f("bandwidth_bps", 1.25e8),
            latency_s: f("latency_s", 0.0),
            check_fixed_s: f("check_fixed_s", 0.0),
            check_per_position_s: f("check_per_position_s", 0.0),
            check_speedup: f("check_speedup", 1.0).max(1e-9),
            localize_s: f("localize_s", 0.0),
            file_s: f("file_s", 0.0),
            beacon_s: f("beacon_s", 0.0),
            margin_frac: f("margin_frac", 0.0),
            margin_s: f("margin_s", 0.0),
        })
    }

    pub fn fetch_s(&self, m: u64) -> f64 {
        self.latency_s + (self.artifact_bytes + m as f64 * self.position_bytes) / self.bandwidth_bps
    }

    pub fn check_s(&self, m: u64) -> f64 {
        (self.check_fixed_s + m as f64 * self.check_per_position_s) / self.check_speedup
    }

    /// `T_challenge` for `m` checked positions, WITHOUT the chain's carrier and reorg budgets (`chain_s` is added by the caller).
    pub fn t_challenge_s(&self, m: u64, chain_s: f64) -> f64 {
        (self.beacon_s + self.fetch_s(m) + self.check_s(m) + self.localize_s + self.file_s) * (1.0 + self.margin_frac)
            + self.margin_s
            + chain_s
    }

    /// The most positions one verifier can check inside `window_s` (capped at the claim's `positions`); `0` when even the fixed part
    /// (the cold artifact, the weights pass) does not fit.
    pub fn positions_within(&self, window_s: f64, chain_s: f64) -> u64 {
        let base = self.t_challenge_s(0, chain_s);
        if base > window_s {
            return 0;
        }
        // The per-position slope in closed form (no float difference of two sums).
        let slope =
            (self.position_bytes / self.bandwidth_bps + self.check_per_position_s / self.check_speedup) * (1.0 + self.margin_frac);
        if slope <= 0.0 {
            return self.positions;
        }
        (((window_s - base) / slope + 1e-9).floor() as u64).min(self.positions)
    }
}

/// `ceil(seconds / s_per_daa)`, at least 1.
pub fn daa(seconds: f64, s_per_daa: f64) -> u64 {
    ((seconds / s_per_daa).ceil() as u64).max(1)
}

/// `P_det = 1 - (1 - q c)^n` as an exact-enough rational `(num, 2^40)`, rounded DOWN (a smaller detection probability is the
/// conservative direction for a collateral).
pub fn p_det_rational(coverage: f64, q: f64, n: u32, eps_enf: f64) -> (u128, u128) {
    let p = (1.0 - (1.0 - (q * coverage).clamp(0.0, 1.0)).powi(n as i32)) * (1.0 - eps_enf).clamp(0.0, 1.0);
    let den = 1u128 << 40;
    (((p * den as f64).floor() as u128).min(den), den)
}

/// The smallest reservation (sompi) that deters: `ceil( max(gain + penalty, ceil(gain / P_dc)) / (1 - accuser) )`.
/// `None` when `P_dc = 0` ("detection probability 0 => no finite collateral is enough").
pub fn reservation_sompi(gain: u128, default_penalty: u128, accuser_permille: u128, p_num: u128, p_den: u128) -> Option<u128> {
    if p_num == 0 || accuser_permille >= 1000 {
        return None;
    }
    let by_detection = (gain * p_den).div_ceil(p_num);
    let base = (gain + default_penalty).max(by_detection);
    Some((base * 1000).div_ceil(1000 - accuser_permille))
}

/// The most gain a reservation defends: `reservation * (1 - accuser) * P_dc` (floor).
pub fn defended_gain_sompi(reservation: u128, accuser_permille: u128, p_num: u128, p_den: u128) -> u128 {
    reservation * (1000 - accuser_permille.min(1000)) / 1000 * p_num / p_den
}

fn bili(s: u128) -> f64 {
    s as f64 / BILI as f64
}

pub fn run(args: &[String]) -> Result<Value, String> {
    let path = arg(args, "--in").ok_or("--in INPUTS.json")?;
    let inp: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| e.to_string())?;
    let s_per_daa = inp.get("s_per_daa").and_then(Value::as_f64).unwrap_or(120.0);
    let ledger = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
    let fence = kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1::interim_v1(
        kaspa_consensus_core::config::params::ForkActivation::new(1),
        vec![],
    );
    let interim = fence.opv_policy();
    let e = inp.get("economics").cloned().unwrap_or(json!({}));
    let ef = |k: &str, d: f64| e.get(k).and_then(Value::as_f64).unwrap_or(d);
    let q = ef("participation_q", 1.0);
    let n_ver = ef("verifiers_n", 1.0) as u32;
    let eps_enf = ef("eps_enf", 0.0);
    let live_per_producer = interim.economics.max_live_claims_per_producer as u128;
    let live_total = interim.economics.max_live_claims_total as u128;
    let gain = interim.max_gain_per_claim(&ledger);
    let penalty = ledger.default_penalty as u128;
    let accuser = ledger.accuser_reward_permille as u128;
    let chain_s = (interim.budgets.carrier_daa + interim.budgets.reorg_slack_daa) as f64 * s_per_daa;
    let interim_window_s = interim.window_daa() as f64 * s_per_daa;

    let mut classes_out = Vec::new();
    for cv in inp.get("classes").and_then(Value::as_array).ok_or("classes[]")? {
        let c = ClassIn::from_json(cv)?;
        let p = c.positions;
        // 1. Time: the whole claim, and what the interim window lets one verifier reach.
        let full = c.t_challenge_s(p, chain_s);
        let in_window = c.positions_within(interim_window_s, chain_s);
        let mut rows = Vec::new();
        for (label, m) in
            [("1 position", 1u64), ("1 %", (p / 100).max(1)), ("10 %", (p / 10).max(1)), ("50 %", (p / 2).max(1)), ("whole claim", p)]
        {
            let t = c.t_challenge_s(m, chain_s);
            let cov = m as f64 / p as f64;
            let (pn, pd) = p_det_rational(cov, q, n_ver, eps_enf);
            rows.push(json!({
                "checked": label, "positions": m, "coverage": cov, "t_fetch_s": c.fetch_s(m), "t_check_s": c.check_s(m), "t_challenge_s": t,
                "t_challenge_daa": daa(t, s_per_daa), "p_dc": pn as f64 / pd as f64,
                "reservation_bili": reservation_sompi(gain, penalty, accuser, pn, pd).map(bili),
                "reservation_x_interim": reservation_sompi(gain, penalty, accuser, pn, pd).map(|r| r as f64 / interim.economics.reservation_per_claim as f64),
                "gain_defended_by_interim_reservation_bili": bili(defended_gain_sompi(interim.economics.reservation_per_claim as u128, accuser, pn, pd)),
            }));
        }
        // 2. The derived budgets, judged by the kernel's own relations (the horizon kept; the base window sized to the first step).
        let judge = |m: u64| -> Value {
            let cold = daa(c.fetch_s(m.max(1)) * (1.0 + c.margin_frac), s_per_daa);
            let chk = daa(c.check_s(m.max(1)) * (1.0 + c.margin_frac), s_per_daa);
            let budgets = OpvBudgetsV1 {
                cold_material_daa: cold,
                check_daa: chk,
                localize_daa: daa(c.localize_s, s_per_daa),
                disclose_daa: interim.budgets.disclose_daa,
                court_daa: daa(c.file_s, s_per_daa).max(interim.budgets.court_daa),
                carrier_daa: interim.budgets.carrier_daa,
                reorg_slack_daa: interim.budgets.reorg_slack_daa,
            };
            let need = budgets.cold_material_daa + budgets.check_daa + budgets.carrier_daa + budgets.reorg_slack_daa;
            let mut derived = interim;
            derived.budgets = budgets;
            derived.window.base_challenge_window_daa = need.saturating_sub(interim.window.verification_horizon_daa).max(1);
            let (pn, pd) = p_det_rational(m as f64 / p as f64, q, n_ver, eps_enf);
            let verdict = match reservation_sompi(gain, penalty, accuser, pn, pd) {
                Some(r) => {
                    derived.economics.reservation_per_claim = r.min(u64::MAX as u128) as u64;
                    derived.economics.assumed_detection_permille = ((pn * 1000 / pd) as u16).clamp(1, 1000);
                    let window = derived.window_daa();
                    let mut with_liability = ledger;
                    with_liability.liability_daa = window + ledger.court_deadline_daa + ledger.proof_grace_daa + 1;
                    json!({
                        "interim_ledger": derived.validate(&ledger).map(|_| "OK".to_string()).unwrap_or_else(|e| e),
                        "liability_daa_needed": with_liability.liability_daa,
                        "with_that_liability": derived.validate(&with_liability).map(|_| "OK".to_string()).unwrap_or_else(|e| e),
                    })
                }
                None => json!("no finite reservation (detection probability 0)"),
            };
            json!({"positions": m, "budgets_daa": {"cold": budgets.cold_material_daa, "check": budgets.check_daa, "localize": budgets.localize_daa,
                   "court": budgets.court_daa, "carrier": budgets.carrier_daa, "reorg": budgets.reorg_slack_daa},
                   "base_window_daa": derived.window.base_challenge_window_daa, "window_daa": derived.window_daa(), "kernel_validate": verdict})
        };
        let judged_interim = judge(in_window);
        let judged_whole = judge(p);
        classes_out.push(json!({
            "name": c.name, "positions": p, "t_challenge_whole_claim_s": full, "t_challenge_whole_claim_daa": daa(full, s_per_daa),
            "interim_window_daa": interim.window_daa(), "positions_within_interim_window": in_window,
            "coverage_within_interim_window": in_window as f64 / p as f64, "rows": rows,
            "derived_for_interim_window_coverage": judged_interim, "derived_for_whole_claim": judged_whole,
        }));
    }
    Ok(json!({
        "cmd": "derive", "s_per_daa": s_per_daa, "max_gain_bili": bili(gain), "default_penalty_bili": bili(penalty), "accuser_permille": accuser,
        "interim_reservation_bili": bili(interim.economics.reservation_per_claim as u128), "live_claims_per_producer": live_per_producer, "live_claims_total": live_total,
        // NOT measured by anything: how many independent honest verifiers check, and how likely each is present. The defaults (one
        // verifier, always present) give the coverage of ONE verifier, which is a lower bound on P_det only if that verifier exists.
        "assumed_inputs": {"participation_q": q, "verifiers_n": n_ver, "eps_enf": eps_enf, "independence_of_verifiers": "assumed by P_det = 1-(1-qc)^n"},
        "classes": classes_out,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAIN: u128 = 20 * BILI;
    const PEN: u128 = 100 * BILI;

    #[test]
    fn reservation_reproduces_the_soundness_dossier_rows() {
        // Row K: gain 20, default penalty 100, accuser 500 permille. P_dc = 1/2 or 1 => 240; 8 of 8,192 positions => 40,960; 0 => none.
        assert_eq!(reservation_sompi(GAIN, PEN, 500, 1, 2), Some(240 * BILI));
        assert_eq!(reservation_sompi(GAIN, PEN, 500, 1, 1), Some(240 * BILI));
        assert_eq!(reservation_sompi(GAIN, PEN, 500, 8, 8192), Some(40_960 * BILI));
        assert_eq!(reservation_sompi(GAIN, PEN, 500, 0, 1), None);
    }

    #[test]
    fn a_defended_gain_inverts_the_reservation() {
        let r = reservation_sompi(GAIN, PEN, 500, 1, 16).unwrap();
        assert!(defended_gain_sompi(r, 500, 1, 16) >= GAIN, "the reservation defends at least the gain it was sized for");
        assert!(defended_gain_sompi(r - BILI, 500, 1, 16) < r, "less collateral never defends more");
    }

    fn class() -> ClassIn {
        ClassIn::from_json(&json!({"name": "t", "positions": 1000, "artifact_bytes": 1e9, "position_bytes": 1e7, "bandwidth_bps": 1e8, "latency_s": 1.0,
            "check_fixed_s": 20.0, "check_per_position_s": 0.5, "localize_s": 3.0, "file_s": 1.0, "beacon_s": 0.0, "margin_frac": 0.25, "margin_s": 10.0})).unwrap()
    }

    #[test]
    fn the_window_formula_is_the_sum_of_its_terms_and_positions_within_inverts_it() {
        let c = class();
        // fetch(0) = 1 + 10 = 11 ; check(0) = 20 ; localize 3 ; file 1 => 35 * 1.25 + 10 + chain 0 = 53.75
        assert!((c.t_challenge_s(0, 0.0) - 53.75).abs() < 1e-9);
        // per position: (1e7/1e8 + 0.5) * 1.25 = 0.75
        assert!((c.t_challenge_s(100, 0.0) - (53.75 + 75.0)).abs() < 1e-9);
        let m = c.positions_within(53.75 + 75.0, 0.0);
        assert_eq!(m, 100);
        assert_eq!(c.positions_within(53.0, 0.0), 0, "the fixed part alone does not fit");
        assert_eq!(c.positions_within(1e9, 0.0), 1000, "capped at the claim");
    }

    #[test]
    fn detection_grows_with_coverage_and_verifiers_and_is_zero_without_either() {
        let (a, d) = p_det_rational(0.1, 1.0, 1, 0.0);
        let (b, _) = p_det_rational(0.1, 1.0, 3, 0.0);
        let (z, _) = p_det_rational(0.0, 1.0, 5, 0.0);
        assert!(a < b && z == 0);
        assert!((a as f64 / d as f64 - 0.1).abs() < 1e-9);
        let (n0, _) = p_det_rational(1.0, 0.0, 5, 0.0);
        assert_eq!(n0, 0, "no verifier present => no detection");
    }
}
