//! `derive`: the OPV window and collateral formulas, EXECUTABLE (RFC-0015 §6.1, §8.1, §8.3.6; the user's 2026-10-08 ruling
//! `T_challenge >= T_beacon + T_fetch + T_check + T_localize + T_file + T_margin`, every term measured, and the 2026-10-10 design
//! changes ADR-0177 D7 and ADR-0032's 49 % reporter share), over inputs a measurement gives. Pure arithmetic: no consensus code calls
//! it, no parameter is chosen by it. The kernel's own `OpvPolicyV1::validate` is run on the budgets it derives, so a derived window is
//! judged by the relations the node enforces, not by this file.
//!
//! ```text
//! opv-meas derive --in inputs.json        # {"s_per_daa": 120, "classes": [ {name, positions, ...} ], "economics": {...}}
//! ```
//!
//! Time, per class (seconds; `m` positions are checked, `P` is the claim's positions):
//!
//! ```text
//! T_fetch(m)   = latency + m * position_bytes / bandwidth          the CLAIM's public material only: a protocol term
//! T_acquire    = artifact_bytes / bandwidth                         the registered MODEL: an off-chain acquisition ASSUMPTION
//!                                                                   (ADR-0177 D7), reported beside the window and never inside it
//! T_check(m)   = (check_fixed - [model_auth_s when model_auth_once] + m * check_per_position) / check_speedup
//! T_challenge(m) = (T_beacon + T_fetch(m) + T_check(m) + T_localize + T_file) * (1 + margin_frac) + margin_s
//!                  + carrier + reorg_slack                          # the chain's two budgets, not measured here
//! ```
//!
//! Money (BILI as sompi; ADR-0174). `P_dc` is the probability that a fraud is detected AND convicted AND its slash collected, and
//! it is CONDITIONAL on acquisition: `h` is how many of the `n` verifiers hold the registered model (ADR-0177 D7: the chain does not
//! make a model obtainable, so `h` can be 0 and then `P_dc = 0`). `r` is the reporter's share of a collected slash (ADR-0032, 4,900
//! bps): a producer that reports itself recovers `r` of what it is slashed, so what a conviction costs it is `(1 - r) * S`, never the
//! gross `S` (ADR-0176 D6).
//!
//! ```text
//! coverage c = m / P (one honest verifier, positions drawn uniformly and unpredictably)
//! P_det      = 1 - (1 - q * c)^h
//! P_dc       = P_det * (1 - eps_enf) * collection_rate
//! reservation >= ceil( max(gain + default_penalty, ceil(gain / P_dc)) / (1 - r) )          # none when P_dc = 0
//! deterrable reward rate per BILI of bond (per DAA) = P_dc * (1 - r) / L_h                 # L_h: the liability horizon in DAA
//! ```
use crate::util::*;
use kaspa_hashes::Hash64;
use misaka_palw_kernel::opv::OpvBudgetsV1;
use serde_json::{Value, json};

const BILI: u128 = 100_000_000; // sompi per BILI (the legacy constant name SOMPI_PER_KASPA), ADR-0174

/// ADR-0032's 2026-10-10 amendment: the PALW reporter share, 4,900 bps = 490 permille.
pub const REPORTER_RETURN_PERMILLE: u128 = 490;

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
    /// The part of `check_fixed_s` that authenticates the verifier's own copy of the model against the registered root (the weight
    /// pass). The reference verifier pays it on every check.
    pub model_auth_s: f64,
    /// ASSUMED implementation property: the verifier authenticates its copy once, when it acquires it, and keeps the result — so the
    /// window does not carry `model_auth_s`. `false` is the reference verifier, which re-authenticates on every check.
    pub model_auth_once: bool,
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
            model_auth_s: f("model_auth_s", 0.0),
            model_auth_once: v.get("model_auth_once").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    /// The protocol's fetch term: the claim's public material for `m` positions (the registered model is NOT in it, ADR-0177 D7).
    pub fn fetch_s(&self, m: u64) -> f64 {
        self.latency_s + m as f64 * self.position_bytes / self.bandwidth_bps
    }

    /// The off-chain acquisition of the registered model: an assumption about what a verifier already holds, never a window term.
    pub fn acquire_model_s(&self) -> f64 {
        self.latency_s + self.artifact_bytes / self.bandwidth_bps
    }

    pub fn check_s(&self, m: u64) -> f64 {
        let fixed = if self.model_auth_once { (self.check_fixed_s - self.model_auth_s).max(0.0) } else { self.check_fixed_s };
        (fixed + m as f64 * self.check_per_position_s) / self.check_speedup
    }

    /// `T_challenge` for `m` checked positions, WITHOUT the chain's carrier and reorg budgets (`chain_s` is added by the caller).
    pub fn t_challenge_s(&self, m: u64, chain_s: f64) -> f64 {
        (self.beacon_s + self.fetch_s(m) + self.check_s(m) + self.localize_s + self.file_s) * (1.0 + self.margin_frac)
            + self.margin_s
            + chain_s
    }

    /// The most positions one verifier can check inside `window_s` (capped at the claim's `positions`); `0` when even the fixed part
    /// (the weights pass) does not fit.
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

/// `P_det = 1 - (1 - q c)^h` as an exact-enough rational `(num, 2^40)`, rounded DOWN (a smaller detection probability is the
/// conservative direction for a collateral). `holders` is the number of verifiers that HOLD the registered model: a verifier
/// without it cannot check (ADR-0177 D7), so `holders = 0` (a closed model) gives exactly 0.
pub fn p_det_rational(coverage: f64, q: f64, holders: u32, eps_enf: f64) -> (u128, u128) {
    let p = (1.0 - (1.0 - (q * coverage).clamp(0.0, 1.0)).powi(holders as i32)) * (1.0 - eps_enf).clamp(0.0, 1.0);
    let den = 1u128 << 40;
    (((p * den as f64).floor() as u128).min(den), den)
}

/// `P_dc = P_det * collection_rate`, rounded down: a slash that is convicted but not collected deters nothing.
pub fn p_dc_rational(coverage: f64, q: f64, holders: u32, eps_enf: f64, collection_rate: f64) -> (u128, u128) {
    let (n, d) = p_det_rational(coverage, q, holders, eps_enf);
    (((n as f64) * collection_rate.clamp(0.0, 1.0)).floor() as u128, d)
}

/// The smallest reservation (sompi) that deters: `ceil( max(gain + penalty, ceil(gain / P_dc)) / (1 - r) )`, with `r` the
/// reporter's share of a collected slash (permille): a self-reporter recovers `r`, so only `(1 - r)` of the slash is a loss.
/// `None` when `P_dc = 0` ("detection probability 0 => no finite collateral is enough").
pub fn reservation_sompi(gain: u128, default_penalty: u128, reporter_return_permille: u128, p_num: u128, p_den: u128) -> Option<u128> {
    if p_num == 0 || reporter_return_permille >= 1000 {
        return None;
    }
    let by_detection = (gain * p_den).div_ceil(p_num);
    let base = (gain + default_penalty).max(by_detection);
    Some((base * 1000).div_ceil(1000 - reporter_return_permille))
}

/// The most gain a reservation defends: `reservation * (1 - r) * P_dc` (floor).
pub fn defended_gain_sompi(reservation: u128, reporter_return_permille: u128, p_num: u128, p_den: u128) -> u128 {
    reservation * (1000 - reporter_return_permille.min(1000)) / 1000 * p_num / p_den
}

/// How many claims of `reservation` a bond of `bond` free collateral holds at once (the same collateral is never counted twice).
pub fn live_claims_supported(bond: u128, reservation: u128) -> u128 {
    bond.checked_div(reservation).unwrap_or(0)
}

/// **The deterrable reward rate** (BILI of attributable gain per DAA, per BILI of bond): a bond `C` that carries undetected gain for
/// the liability horizon `L_h` is deterred only if `rate * L_h * C <= P_dc * (1 - r) * C` (one conviction slashes the bond once, and
/// detection of one claim is taken as detection of the producer's whole set — the correlation the premise asks to be counted).
/// Rate and claim capacity `rho` are independent: a larger `rho` shrinks the per-claim gain, not this bound (ADR-0176 D2).
pub fn deterrable_reward_rate(p_num: u128, p_den: u128, reporter_return_permille: u128, liability_horizon_daa: u64) -> f64 {
    if liability_horizon_daa == 0 || p_den == 0 {
        return 0.0;
    }
    (p_num as f64 / p_den as f64) * (1000 - reporter_return_permille.min(1000)) as f64 / 1000.0 / liability_horizon_daa as f64
}

fn bili(s: u128) -> f64 {
    s as f64 / BILI as f64
}

/// The economic inputs, in sompi where they are money.
#[derive(Clone, Copy, Debug)]
struct Econ {
    gain: u128,
    penalty: u128,
    /// ADR-0032: the reporter's share of a collected slash (permille).
    reporter: u128,
    /// The interim kernel ledger's `accuser_reward_permille`, shown only as a comparator.
    kernel_accuser: u128,
    q: f64,
    n: u32,
    /// Verifiers that hold the registered model (ADR-0177 D7). `<= n`.
    holders: u32,
    eps_enf: f64,
    collection: f64,
    bond: u128,
    horizon_daa: u64,
    live_per_producer: u128,
    live_total: u128,
    interim_reservation: u128,
}

impl Econ {
    /// The economics of one checked coverage `c`, for `holders` model-holding verifiers.
    fn at(&self, cov: f64, holders: u32) -> Value {
        let (pn, pd) = p_dc_rational(cov, self.q, holders.min(self.n), self.eps_enf, self.collection);
        let mut v = self.at_p(pn, pd);
        v["model_holders"] = json!(holders.min(self.n));
        v
    }

    /// The economics of a stated `P_dc = pn / pd` (no claim about where it comes from).
    fn at_p(&self, pn: u128, pd: u128) -> Value {
        let res = reservation_sompi(self.gain, self.penalty, self.reporter, pn, pd);
        let res_kernel = reservation_sompi(self.gain, self.penalty, self.kernel_accuser, pn, pd);
        json!({
            "p_dc": pn as f64 / pd as f64,
            "reservation_bili": res.map(bili),
            "reservation_at_interim_kernel_500_permille_bili": res_kernel.map(bili),
            "reservation_x_interim": res.map(|r| r as f64 / self.interim_reservation as f64),
            "gain_defended_by_interim_reservation_bili": bili(defended_gain_sompi(self.interim_reservation, self.reporter, pn, pd)),
            "live_claims_a_bond_holds": res.map(|r| live_claims_supported(self.bond, r) as f64),
            "collateral_for_the_interim_live_cap_per_producer_bili": res.map(|r| bili(r * self.live_per_producer)),
            "collateral_the_ledger_locks_at_the_interim_cap_bili": res.map(|r| bili(r * self.live_total)),
            "deterrable_reward_per_daa_per_bili_of_bond": deterrable_reward_rate(pn, pd, self.reporter, self.horizon_daa),
        })
    }
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
    let n_ver = ef("verifiers_n", 1.0) as u32;
    let gain = interim.max_gain_per_claim(&ledger);
    let interim_horizon = interim.window_daa() + ledger.liability_daa;
    let econ = Econ {
        gain,
        penalty: ledger.default_penalty as u128,
        reporter: ef("reporter_return_permille", REPORTER_RETURN_PERMILLE as f64) as u128,
        kernel_accuser: ledger.accuser_reward_permille as u128,
        q: ef("participation_q", 1.0),
        n: n_ver,
        holders: (ef("model_holders_n", n_ver as f64) as u32).min(n_ver),
        eps_enf: ef("eps_enf", 0.0),
        collection: ef("collection_rate", 1.0),
        bond: (ef("bond_bili", 13_000.0) * BILI as f64) as u128,
        horizon_daa: ef("liability_horizon_daa", interim_horizon as f64) as u64,
        live_per_producer: interim.economics.max_live_claims_per_producer as u128,
        live_total: interim.economics.max_live_claims_total as u128,
        interim_reservation: interim.economics.reservation_per_claim as u128,
    };
    let chain_s = (interim.budgets.carrier_daa + interim.budgets.reorg_slack_daa) as f64 * s_per_daa;
    let interim_window_s = interim.window_daa() as f64 * s_per_daa;

    let den = 1u128 << 40;
    let by_p_dc: Vec<Value> = inp
        .get("p_dc_values")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_f64)
                .map(|p| {
                    let mut v = econ.at_p(((p * den as f64).floor() as u128).min(den), den);
                    v["p_dc_stated"] = json!(p);
                    v
                })
                .collect()
        })
        .unwrap_or_default();

    let mut classes_out = Vec::new();
    for cv in inp.get("classes").and_then(Value::as_array).ok_or("classes[]")? {
        let c = ClassIn::from_json(cv)?;
        let p = c.positions;
        // 1. Time: the whole claim, and what the interim window lets one verifier reach. The model is NOT in it.
        let full = c.t_challenge_s(p, chain_s);
        let in_window = c.positions_within(interim_window_s, chain_s);
        let mut rows = Vec::new();
        for (label, m) in
            [("1 position", 1u64), ("1 %", (p / 100).max(1)), ("10 %", (p / 10).max(1)), ("50 %", (p / 2).max(1)), ("whole claim", p)]
        {
            let t = c.t_challenge_s(m, chain_s);
            let cov = m as f64 / p as f64;
            rows.push(json!({
                "checked": label, "positions": m, "coverage": cov, "t_fetch_claim_material_s": c.fetch_s(m), "t_check_s": c.check_s(m),
                "t_challenge_s": t, "t_challenge_daa": daa(t, s_per_daa),
                "model_held_by_the_verifiers": econ.at(cov, econ.holders),
                "closed_model_no_third_party_holds_it": econ.at(cov, 0),
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
            let cov = m as f64 / p as f64;
            let (pn, pd) = p_dc_rational(cov, econ.q, econ.holders, econ.eps_enf, econ.collection);
            let verdict = match reservation_sompi(gain, econ.penalty, econ.reporter, pn, pd) {
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
                        // The kernel's own relation counts the GROSS reservation as the producer's loss (no reporter self-return).
                        "kernel_required_reservation_bili_gross": bili(derived.required_reservation(&ledger)),
                        "reservation_net_of_reporter_return_bili": bili(r),
                    })
                }
                None => json!("no finite reservation (detection probability 0)"),
            };
            json!({"positions": m, "budgets_daa": {"cold_claim_material": budgets.cold_material_daa, "check": budgets.check_daa,
                   "localize": budgets.localize_daa, "court": budgets.court_daa, "carrier": budgets.carrier_daa, "reorg": budgets.reorg_slack_daa},
                   "base_window_daa": derived.window.base_challenge_window_daa, "window_daa": derived.window_daa(), "kernel_validate": verdict})
        };
        let judged_interim = judge(in_window);
        let judged_whole = judge(p);
        classes_out.push(json!({
            "name": c.name, "positions": p,
            // OFF-CHAIN, an assumption about what the verifier already holds: never part of T_challenge (ADR-0177 D7).
            "t_acquire_registered_model_s_off_chain": c.acquire_model_s(), "model_auth_once_assumed": c.model_auth_once,
            "t_challenge_whole_claim_s": full, "t_challenge_whole_claim_daa": daa(full, s_per_daa),
            "interim_window_daa": interim.window_daa(), "positions_within_interim_window": in_window,
            "coverage_within_interim_window": in_window as f64 / p as f64, "rows": rows,
            "derived_for_interim_window_coverage": judged_interim, "derived_for_whole_claim": judged_whole,
            "input_labels": cv.get("labels").cloned().unwrap_or(Value::Null),
        }));
    }
    Ok(json!({
        "cmd": "derive", "s_per_daa": s_per_daa, "max_gain_bili": bili(gain), "default_penalty_bili": bili(econ.penalty),
        "reporter_return_permille": econ.reporter, "net_loss_fraction_of_a_collected_slash": (1000 - econ.reporter) as f64 / 1000.0,
        "interim_kernel_accuser_permille_comparator": econ.kernel_accuser,
        "interim_reservation_bili": bili(econ.interim_reservation), "live_claims_per_producer": econ.live_per_producer, "live_claims_total": econ.live_total,
        "bond_bili": bili(econ.bond), "liability_horizon_daa": econ.horizon_daa,
        // NOT measured by anything: how many independent honest verifiers check, how likely each is present, and above all how many of
        // them HOLD the registered model (ADR-0177 D7: the chain does not make it obtainable; for a closed model that is 0). The
        // defaults (one verifier, always present, holding the model) give the coverage of ONE verifier, a lower bound on P_det only if
        // that verifier exists.
        "assumed_inputs": {"participation_q": econ.q, "verifiers_n": econ.n, "model_holders_n": econ.holders, "eps_enf": econ.eps_enf,
                           "collection_rate": econ.collection, "independence_of_verifiers": "assumed by P_det = 1-(1-qc)^h"},
        "input_labels": inp.get("labels").cloned().unwrap_or(Value::Null),
        // The collateral as a function of a STATED detection probability alone (a formula table, not a result about any network).
        "by_p_dc": by_p_dc,
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
        // Row K at the interim ledger's 500 permille: P_dc = 1/2 or 1 => 240; 8 of 8,192 positions => 40,960; 0 => none.
        assert_eq!(reservation_sompi(GAIN, PEN, 500, 1, 2), Some(240 * BILI));
        assert_eq!(reservation_sompi(GAIN, PEN, 500, 1, 1), Some(240 * BILI));
        assert_eq!(reservation_sompi(GAIN, PEN, 500, 8, 8192), Some(40_960 * BILI));
        assert_eq!(reservation_sompi(GAIN, PEN, 500, 0, 1), None);
    }

    #[test]
    fn the_loss_is_the_slash_net_of_the_49_percent_self_return() {
        // ADR-0032: a self-reporter recovers 49 % of a collected slash, so only 51 % is a loss. 20 BILI of gain at 8/8,192 detection
        // needs S with 0.51 S / 1,024 >= 20 -> S = 40,156.86... BILI, not the 40,960 a 50 % share (or 20,480 the gross slash) gives.
        let r = reservation_sompi(GAIN, PEN, REPORTER_RETURN_PERMILLE, 8, 8192).unwrap();
        assert_eq!(r, (GAIN * 1024 * 1000).div_ceil(510));
        assert!(r < 40_960 * BILI && r > 40_156 * BILI && r < 40_157 * BILI);
        // The net loss of that reservation covers the gain over the detection probability (never the gross slash).
        assert!(r * (1000 - REPORTER_RETURN_PERMILLE) / 1000 * 8 / 8192 >= GAIN);
        let gross = GAIN * 1024; // the reservation if the gross slash were taken as the loss: 20,480 BILI
        assert!(
            gross * (1000 - REPORTER_RETURN_PERMILLE) / 1000 * 8 / 8192 < GAIN,
            "the gross-slash reservation under-reserves by ~2x"
        );
        // A higher reporter share needs MORE collateral, never less.
        assert!(reservation_sompi(GAIN, PEN, 600, 8, 8192).unwrap() > r);
        assert!(reservation_sompi(GAIN, PEN, 1000, 8, 8192).is_none(), "a 100 % share makes a self-conviction free");
    }

    #[test]
    fn a_defended_gain_inverts_the_reservation() {
        let r = reservation_sompi(GAIN, PEN, REPORTER_RETURN_PERMILLE, 1, 16).unwrap();
        assert!(
            defended_gain_sompi(r, REPORTER_RETURN_PERMILLE, 1, 16) >= GAIN,
            "the reservation defends at least the gain it was sized for"
        );
        assert!(defended_gain_sompi(r - BILI, REPORTER_RETURN_PERMILLE, 1, 16) < r, "less collateral never defends more");
    }

    fn class() -> ClassIn {
        ClassIn::from_json(&json!({"name": "t", "positions": 1000, "artifact_bytes": 1e9, "position_bytes": 1e7, "bandwidth_bps": 1e8, "latency_s": 1.0,
            "check_fixed_s": 20.0, "check_per_position_s": 0.5, "localize_s": 3.0, "file_s": 1.0, "beacon_s": 0.0, "margin_frac": 0.25, "margin_s": 10.0})).unwrap()
    }

    #[test]
    fn the_window_formula_is_the_sum_of_its_terms_and_positions_within_inverts_it() {
        let c = class();
        // fetch(0) = 1 (latency; the 1 GB model is NOT in it) ; check(0) = 20 ; localize 3 ; file 1 => 25 * 1.25 + 10 + chain 0 = 41.25
        assert!((c.t_challenge_s(0, 0.0) - 41.25).abs() < 1e-9);
        // per position: (1e7/1e8 + 0.5) * 1.25 = 0.75
        assert!((c.t_challenge_s(100, 0.0) - (41.25 + 75.0)).abs() < 1e-9);
        let m = c.positions_within(41.25 + 75.0, 0.0);
        assert_eq!(m, 100);
        assert_eq!(c.positions_within(41.0, 0.0), 0, "the fixed part alone does not fit");
        assert_eq!(c.positions_within(1e9, 0.0), 1000, "capped at the claim");
    }

    #[test]
    fn the_registered_model_is_an_off_chain_assumption_not_a_window_term() {
        // ADR-0177 D7: the artifact's bytes move T_acquire, never T_challenge.
        let mut big = class();
        big.artifact_bytes = 1e12;
        assert_eq!(big.t_challenge_s(10, 0.0), class().t_challenge_s(10, 0.0));
        assert!(big.acquire_model_s() > class().acquire_model_s() * 900.0);
        // The claim's public material, by contrast, IS in it.
        let mut wide = class();
        wide.position_bytes = 1e9;
        assert!(wide.t_challenge_s(10, 0.0) > class().t_challenge_s(10, 0.0) + 100.0);
        // Authenticating the verifier's own copy once, at acquisition, takes it out of the window; the reference verifier keeps it in.
        let mut c = class();
        c.model_auth_s = 8.0;
        let reference = c.t_challenge_s(0, 0.0);
        c.model_auth_once = true;
        assert!((reference - c.t_challenge_s(0, 0.0) - 8.0 * 1.25).abs() < 1e-9);
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

    #[test]
    fn a_closed_model_has_no_detection_and_no_finite_collateral() {
        // ADR-0177 D7: a verifier that does not hold the registered model cannot check; if nobody but the producer holds it, P = 0.
        let (pn, pd) = p_dc_rational(1.0, 1.0, 0, 0.0, 1.0);
        assert_eq!(pn, 0);
        assert_eq!(reservation_sompi(GAIN, PEN, REPORTER_RETURN_PERMILLE, pn, pd), None);
        assert_eq!(deterrable_reward_rate(pn, pd, REPORTER_RETURN_PERMILLE, 250), 0.0, "no reward rate is deterrable");
        // A full check by one holder is not the same as an open model: one holder at coverage 1 reserves the floor.
        let (on, od) = p_dc_rational(1.0, 1.0, 1, 0.0, 1.0);
        assert!(reservation_sompi(GAIN, PEN, REPORTER_RETURN_PERMILLE, on, od).is_some());
    }

    #[test]
    fn an_uncollected_slash_deters_nothing() {
        let (full, d) = p_dc_rational(0.01, 1.0, 1, 0.0, 1.0);
        let (half, _) = p_dc_rational(0.01, 1.0, 1, 0.0, 0.5);
        assert_eq!(half * 2, full);
        assert!(
            reservation_sompi(GAIN, PEN, REPORTER_RETURN_PERMILLE, half, d).unwrap()
                > reservation_sompi(GAIN, PEN, REPORTER_RETURN_PERMILLE, full, d).unwrap()
        );
    }

    #[test]
    fn exposure_and_the_deterrable_rate_have_the_right_shape() {
        // A 13,000 BILI bond holds 13 interim 1,000 BILI reservations and 0 of 40,157 BILI: the same collateral is never counted twice.
        assert_eq!(live_claims_supported(13_000 * BILI, 1_000 * BILI), 13);
        assert_eq!(live_claims_supported(13_000 * BILI, 40_157 * BILI), 0);
        assert_eq!(live_claims_supported(1, 0), 0);
        // Full detection, one 250-DAA horizon: 0.51 / 250 BILI of attributable gain per DAA per BILI of bond.
        let full = deterrable_reward_rate(1, 1, REPORTER_RETURN_PERMILLE, 250);
        assert!((full - 0.51 / 250.0).abs() < 1e-12);
        // Less detection or a longer horizon lowers it; it never rises with a larger reporter share.
        assert!(deterrable_reward_rate(1, 1024, REPORTER_RETURN_PERMILLE, 250) < full / 1000.0);
        assert!(deterrable_reward_rate(1, 1, REPORTER_RETURN_PERMILLE, 500) < full);
        assert!(deterrable_reward_rate(1, 1, 700, 250) < full);
    }

    /// The committed inputs (the numbers `opv-measurements.md` §4 quotes) run through the same code the tables came from.
    #[test]
    fn the_committed_inputs_reproduce_the_tables_the_note_quotes() {
        let dir = format!("{}/../../docs/design/palw/opv-measurements-data/derive", env!("CARGO_MANIFEST_DIR"));
        let run_file = |f: &str| run(&["derive".into(), "--in".into(), format!("{dir}/{f}")]).expect("derive");
        let one = run_file("in-v2-one-verifier.json");
        assert_eq!(one["reporter_return_permille"], 490);
        // The collateral by stated detection probability, net of the 49 % self-return.
        let by_p = one["by_p_dc"].as_array().unwrap();
        let res = |p: f64| {
            by_p.iter().find(|r| (r["p_dc_stated"].as_f64().unwrap() - p).abs() < 1e-12).unwrap()["reservation_bili"].as_f64()
        };
        assert!((res(1.0).unwrap() - 120.0 / 0.51).abs() < 1e-4, "{:?}", res(1.0));
        assert!((res(0.001).unwrap() - 20_000.0 / 0.51).abs() < 1e-3);
        assert!((res(8.0 / 8192.0).unwrap() - 20_480.0 / 0.51).abs() < 1e-3);
        assert_eq!(res(0.0), None, "detection probability 0: no finite collateral");
        let classes = one["classes"].as_array().unwrap();
        let by_name = |n: &str| classes.iter().find(|c| c["name"] == n).unwrap_or_else(|| panic!("class {n}"));
        // The model is off the clock: a 1 Gbps link changes the claim-material term only, never the acquisition, which is its own line.
        let (none, link) = (by_name("qwen25-0.5b P=8192 no-link"), by_name("qwen25-0.5b P=8192 1Gbps(assumed link)"));
        assert!(link["t_acquire_registered_model_s_off_chain"].as_f64().unwrap() > 5.0);
        assert!(none["t_challenge_whole_claim_s"].as_f64().unwrap() < link["t_challenge_whole_claim_s"].as_f64().unwrap());
        // One reference verifier reaches 226 of 8,192 positions inside the interim window; the auth-once sensitivity reaches more.
        assert_eq!(none["positions_within_interim_window"], 226);
        assert!(by_name("qwen25-0.5b P=8192 auth-once no-link")["positions_within_interim_window"].as_u64().unwrap() > 226);
        // Every closed-model row has no finite reservation, for every class.
        for c in classes {
            for r in c["rows"].as_array().unwrap() {
                let closed = &r["closed_model_no_third_party_holds_it"];
                assert_eq!(closed["p_dc"], 0.0);
                assert!(closed["reservation_bili"].is_null() && closed["deterrable_reward_per_daa_per_bili_of_bond"] == 0.0);
            }
        }
        // 9B-8k, one reference verifier, deterrence only: 5 of 8,192 positions reach the window; the reservation is tens of thousands of BILI.
        let nine = by_name("9B-8k reference no-link");
        assert!(nine["positions_within_interim_window"].as_u64().unwrap() < 10);
        // More verifiers who hold the model lower the collateral; holders = 0 raises it without bound (none).
        let ten = run_file("in-v2-ten-verifiers-half-present.json");
        let three = run_file("in-v2-three-of-ten-hold-the-model.json");
        let first = |v: &Value, n: &str| {
            let c = v["classes"].as_array().unwrap().iter().find(|c| c["name"] == n).unwrap().clone();
            c["rows"][2]["model_held_by_the_verifiers"]["p_dc"].as_f64().unwrap()
        };
        let n = "qwen25-0.5b P=8192 no-link";
        assert!(
            first(&one, n) < first(&three, n) && first(&three, n) < first(&ten, n),
            "detection rises with the verifiers that hold the model"
        );
    }
}
