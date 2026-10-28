//! **ADR-0152 v3.1 T06 (§8.1, M4; §4.3, SW-6, SW-7, SW-10, IA-1b): the collusion EV grid on the real
//! stake-weighted draw.** §4.3 prices the undetectable coverage lie — the full seat AND the lied
//! segment's partial holder both the attacker's (P2), the V1 door with no filing (P3), all five seats
//! (P5), a first panel whose honest attester files only when online (30% offline) and one that files
//! nothing (the free redraw, C3) — against the failed-attempt break-even at RT#2, with the attacker's
//! stake split into floor-sized operators beside testnet-12's eight genesis seats. Its numbers came
//! from `docs/handoff/t12-rcore-20260924/v3calc/v31_stake_draw.py` and `v31_review_numbers.py`
//! (the handoff branch; not on the integration line). This file is those scripts ported, run against
//! the code:
//!
//! * **The law is the race's.** [`t06_the_real_race_follows_the_successive_sampling_law`] draws
//!   thousands of panels through `palw_panel_stake_race_of_v1` (the class draw past
//!   `palw_rcore_plus`) with the real segment assignment (`palw_segment_assignment_v2`), and the
//!   measured distribution of attacker seats and the measured P2 (full seat and holder both the
//!   attacker's, the lie in a named segment) sit within 4σ of the exact successive-sampling law the
//!   thresholds are computed from — at the design point, at the worst admitted state, and at
//!   `stake: None` (ADR-0130's one ticket per operator, `palw_panel_operator_lottery_of_v1`) against
//!   the uniform law. The executor is outside every population (its own operator never sits: the
//!   attacker's producer bond is a separate 13,000 MSK bond, not counted, as §4.3 says).
//! * **The grid.** [`t06_the_ev_grid_reproduces_section_4_3`] recomputes §4.3's stake-weighted EV
//!   table (P2, P3, P5 and the floor / 8k / 2M EV columns, ten rows) and v3's uniform table (six
//!   rows), and the thresholds: 17.29M / 13.39M / 8.32M / 6.63M / 50.83M on the floor (8k and 2M one
//!   operator lower, P5 50.31M / 50.44M), and `stake: None`'s 20 / 16 / 10 / 8 / 56 operators.
//! * **The worst state SW-10 admits.** [`t06_the_worst_admitted_state_under_sw10`] lets the attacker
//!   saturate any `k` of the eight genesis seats (in the base, not eligible) with all its own
//!   operators idle (in both), decides "binds" with the code's own `palw_panel_stake_weight_v1` and
//!   `palw_panel_stake_floor_v1`, and finds §4.3's worst rows (12.74M / 9.88M / 7.15M / 5.72M /
//!   25.61M on the floor, 12.61M / 9.75M / 7.02M / 5.72M / 25.35M on 8k and 2M) and the cliff table
//!   per `k`; with SW-10's executor term at the cap (IA-1b, `X` on both sides of the floor) the free
//!   redraw's worst falls to 6.63M (51, k = 6), P2 with filing stays 12.74M, and k = 5's 15.08M
//!   becomes 14.04M.
//!
//! **What is modelled, and what is not.** EV per attempt is `P·G − (1 − P)·(w + E)` in MSK at RT#2
//! (S0′), with `G = E + w + R`, `E = 720‰` of testnet-12's block subsidy and `(w, R)` the per-class
//! inputs of the scripts. Post-licence detection is not counted (`q = 0`, §4.3's convention — the
//! conservative side: any `q > 0` only lowers the attacker's EV); `q_f` enters as the filing models
//! (file / 30% offline / nothing). The attacker's best split is floor-sized operators (SW-7; the
//! script's Monte Carlo of unequal splits is not ported). The genesis weight, the panel floor, the
//! cap and the 875‰ floor are read from the shipped testnet-12 preset and the draw's own constants.
//!
//! Run: cargo test -p kaspa-consensus-core --test t06_stake_draw_ev_grid -- --nocapture

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI, PALW_T12_GENESIS_BONDS, palw_t12_shipped_params};
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_v1;
use kaspa_consensus_core::palw_panel_v2::{
    PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1, PALW_DRAW_WEIGHT_CAP_MSK_V1, PalwPanelStakeDrawV1, PalwPanelV2Error,
    palw_draw_operator_weight_msk_v1, palw_panel_operator_lottery_of_v1, palw_panel_stake_floor_v1, palw_panel_stake_race_of_v1,
    palw_panel_stake_weight_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2, PalwPanelSeatV2};
use kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

/// The panel's seats; the partial seats partition the job into `SEATS − 1` segments.
const SEATS: usize = 5;
/// The worker escrow's share of the block subsidy, ‰ (`E` in the scripts: 444,562,014,000 × 720‰).
const ESCROW_PERMILLE: u64 = 720;
/// The per-class inputs of `v31_stake_draw.py` (`w`, `R`, sompi): the floor, the 8k row, the 2M row.
const CLASSES: [(&str, u64, u64); 3] =
    [("floor", 10_750_000, 1_000_000), ("8k", 49_432_000_000, 4_945_000_000), ("2M", 5_974_294_000_000, 65_537_000_000)];

/// The model names, in §4.3's order.
const MODELS: [&str; 5] = ["P2 (file)", "P2, offline 30%", "P2, free redraw", "P3 (V1, no filing)", "P5 (all five)"];

/// testnet-12's draw inputs, read from the shipped preset and the draw's constants.
struct Net {
    /// The genesis seats (operators), and each one's SW-2 weight in whole MSK.
    genesis: u64,
    g_w: u64,
    /// The panel floor in whole MSK — a floor-sized Sybil operator.
    sybil: u64,
    /// `E`, sompi.
    e: u64,
    stake: PalwPanelStakeDrawV1,
}

fn net() -> Net {
    let p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let stake = PalwPanelStakeDrawV1::V1;
    let g_w =
        palw_draw_operator_weight_msk_v1((PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI / SOMPI_PER_KASPA) as u128, stake.weight_cap_msk);
    let sybil = palw_panel_collateral_floor_v1(bundle.state.min_collateral_sompi()) / SOMPI_PER_KASPA;
    let e = PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI * ESCROW_PERMILLE / 1000;
    let n = Net { genesis: PALW_T12_GENESIS_BONDS.len() as u64, g_w, sybil, e, stake };
    // The scripts' inputs, so a moved preset is seen here before it is seen in a threshold.
    assert_eq!((n.genesis, n.g_w, n.sybil), (8, 939_063, 130_000), "eight genesis seats of 939,063 MSK; a 130,000 MSK panel floor");
    assert_eq!(n.e, 320_084_650_080, "E = 3,200.85 MSK (720‰ of the genesis-era subsidy)");
    assert_eq!(
        (stake.weight_cap_msk, stake.eligible_floor_permille),
        (PALW_DRAW_WEIGHT_CAP_MSK_V1, PALW_DRAW_ELIGIBLE_FLOOR_PERMILLE_V1)
    );
    assert_eq!((stake.weight_cap_msk, stake.eligible_floor_permille), (1_000_000, 875));
    n
}

// ---- the law: successive sampling without replacement, proportional to weight -----------------

/// `P(A = j)`, `j = 0..=SEATS`, for successive sampling of `SEATS` operators without replacement
/// from `nh` honest operators of weight `h` and `na` attacker operators of weight `a` — the key
/// race's law (§3.14 SW-3), `dist_A` of `v31_stake_draw.py` (one honest type).
fn law(nh: u64, h: f64, na: u64, a: f64) -> [f64; SEATS + 1] {
    // probs[i][j]: i honest and j attacker drawn so far.
    let mut probs = vec![vec![0f64; SEATS + 1]; SEATS + 1];
    probs[0][0] = 1.0;
    for drawn in 0..SEATS {
        let mut next = vec![vec![0f64; SEATS + 1]; SEATS + 1];
        for i in 0..=drawn {
            let j = drawn - i;
            let p = probs[i][j];
            if p == 0.0 {
                continue;
            }
            let rh = nh.saturating_sub(i as u64) as f64 * h;
            let ra = na.saturating_sub(j as u64) as f64 * a;
            let total = rh + ra;
            if total == 0.0 {
                continue;
            }
            if rh > 0.0 {
                next[i + 1][j] += p * rh / total;
            }
            if ra > 0.0 {
                next[i][j + 1] += p * ra / total;
            }
        }
        probs = next;
    }
    let mut out = [0f64; SEATS + 1];
    for (i, row) in probs.iter().enumerate() {
        for (j, p) in row.iter().enumerate() {
            if i + j == SEATS {
                out[j] += p;
            }
        }
    }
    out
}

/// `(P2, P3, P5)` of a law: `P2 = E[A(A − 1)] / 20` (exact under any draw, SW-6: the full seat is
/// uniform over the five positions and the holder over the other four, independent of the keys).
fn metrics(d: &[f64; SEATS + 1]) -> (f64, f64, f64) {
    let p2 = d.iter().enumerate().map(|(a, p)| (a * a.saturating_sub(1)) as f64 * p).sum::<f64>() / (SEATS * (SEATS - 1)) as f64;
    let p3 = d.iter().enumerate().filter(|(a, _)| *a >= 3).map(|(_, p)| p).sum::<f64>();
    (p2, p3, d[SEATS])
}

fn model(name: &str, d: &[f64; SEATS + 1]) -> f64 {
    let (p2, p3, p5) = metrics(d);
    match name {
        "P2 (file)" => p2,
        "P2, offline 30%" => p2 + (1.0 - p2) * 0.3 * p2,
        "P2, free redraw" => 1.0 - (1.0 - p2) * (1.0 - p2),
        "P3 (V1, no filing)" => p3,
        "P5 (all five)" => p5,
        other => panic!("no model {other}"),
    }
}

/// EV per attempt at RT#2 in MSK: `P·G − (1 − P)·(w + E)`.
fn ev(p: f64, e: u64, w: u64, r: u64) -> f64 {
    let g = (e + w + r) as f64;
    let l = (w + e) as f64;
    (p * g - (1.0 - p) * l) / SOMPI_PER_KASPA as f64
}

/// The smallest attacker operator count (floor-sized, `a` each) beside `nh` honest operators of
/// weight `h` whose `model` EV is positive; a panel needs five eligible operators, so fewer is not a
/// capture (`InsufficientEligibleBonds`).
fn threshold(nh: u64, h: f64, a: f64, name: &str, e: u64, w: u64, r: u64) -> u64 {
    let from = (SEATS as u64).saturating_sub(nh).max(1);
    (from..20_000).find(|&m| ev(model(name, &law(nh, h, m, a)), e, w, r) > 0.0).expect("a threshold exists")
}

// ---- the grid ---------------------------------------------------------------------------------

/// §4.3's stake-weighted table: `(operators, share, P2, P3, P5, [floor, 8k, 2M] × [P2, free redraw, P3])`.
#[allow(clippy::type_complexity)]
const STAKE_TABLE: [(u64, f64, f64, f64, f64, [[i64; 3]; 3]); 10] = [
    (10, 0.148, 0.0267, 0.0278, 0.0000, [[-3_030, -2_863, -3_023], [-3_496, -3_303, -3_488], [-59_559, -56_265, -59_423]]),
    (20, 0.257, 0.0789, 0.1325, 0.0007, [[-2_696, -2_231, -2_353], [-3_108, -2_568, -2_709], [-52_964, -43_770, -46_178]]),
    (40, 0.409, 0.1875, 0.3858, 0.0098, [[-2_001, -1_026, -731], [-2_301, -1_167, -825], [-39_222, -19_948, -14_124]]),
    (51, 0.469, 0.2408, 0.5003, 0.0203, [[-1_659, -489, 2], [-1_903, -543, 27], [-32_469, -9_333, 363]]),
    (64, 0.526, 0.2969, 0.6069, 0.0371, [[-1_300, 37, 685], [-1_486, 67, 820], [-25_367, 1_051, 13_860]]),
    (77, 0.571, 0.3462, 0.6879, 0.0574, [[-985, 464, 1_203], [-1_120, 564, 1_423], [-19_138, 9_503, 24_103]]),
    (103, 0.641, 0.4276, 0.7963, 0.1041, [[-464, 1_103, 1_897], [-514, 1_307, 2_229], [-8_836, 22_136, 37_816]]),
    (132, 0.696, 0.4981, 0.8665, 0.1590, [[-12, 1_588, 2_346], [11, 1_871, 2_752], [90, 31_726, 46_709]]),
    (133, 0.697, 0.5003, 0.8683, 0.1608, [[2, 1_602, 2_358], [27, 1_887, 2_765], [360, 31_996, 46_939]]),
    (200, 0.776, 0.6115, 0.9412, 0.2779, [[714, 2_235, 2_824], [854, 2_621, 3_307], [14_432, 44_496, 56_155]]),
];

/// v3's uniform table (one entry per operator, `N = 8 + m`): `(m, P2, P3, P5, [floor, 8k, 2M] × [P2, free redraw, P3])`.
#[allow(clippy::type_complexity)]
const UNIFORM_TABLE: [(u64, f64, f64, f64, [[i64; 3]; 3]); 6] = [
    (4, 0.0909, 0.1515, 0.0, [[-2_619, -2_090, -2_231], [-3_019, -2_404, -2_568], [-51_440, -40_982, -43_771]]),
    (9, 0.2647, 0.5633, 0.0204, [[-1_506, -260, 406], [-1_726, -278, 496], [-29_447, -4_817, 8_344]]),
    (10, 0.2941, 0.6176, 0.0294, [[-1_318, 11, 753], [-1_507, 38, 900], [-25_725, 547, 15_215]]),
    (16, 0.4348, 0.8142, 0.1028, [[-418, 1_156, 2_012], [-460, 1_368, 2_363], [-7_925, 23_172, 40_091]]),
    (20, 0.5026, 0.8769, 0.1578, [[17, 1_617, 2_413], [44, 1_904, 2_829], [662, 32_297, 48_025]]),
    (27, 0.5899, 0.9335, 0.2487, [[576, 2_124, 2_775], [694, 2_493, 3_250], [11_706, 42_319, 55_179]]),
];

fn assert_ev_row(label: &str, d: &[f64; SEATS + 1], e: u64, want: &[[i64; 3]; 3]) {
    for (c, (class, w, r)) in CLASSES.iter().enumerate() {
        let got = ["P2 (file)", "P2, free redraw", "P3 (V1, no filing)"].map(|m| ev(model(m, d), e, *w, *r).round() as i64);
        assert_eq!(got, want[c], "{label}, {class}: EV P2 / free redraw / P3 (MSK)");
    }
}

fn close(got: f64, want: f64, places: i32, what: &str) {
    let tol = 0.5 * 10f64.powi(-places) + 1e-12;
    assert!((got - want).abs() <= tol, "{what}: {got:.6} is not {want} to {places} places");
}

/// **T06, the grid: §4.3's EV tables and thresholds, recomputed.** Every cell of the stake-weighted
/// table (ten rows) and of v3's uniform table (six rows), and the design-point thresholds per model
/// and class, on the stake draw and at `stake: None`.
#[test]
fn t06_the_ev_grid_reproduces_section_4_3() {
    let n = net();
    let (g, h, s) = (n.genesis, n.g_w as f64, n.sybil as f64);
    let honest_base = n.genesis * n.g_w;
    assert_eq!(honest_base, 7_512_504, "the honest base");

    for (m, share, p2, p3, p5, evs) in STAKE_TABLE {
        let d = law(g, h, m, s);
        let (g2, g3, g5) = metrics(&d);
        let stake_msk = m * n.sybil;
        let label = format!("{:.2}M ({m} operators)", stake_msk as f64 / 1e6);
        close(stake_msk as f64 / (stake_msk + honest_base) as f64, share, 3, &format!("{label}: share"));
        close(g2, p2, 4, &format!("{label}: P2"));
        close(g3, p3, 4, &format!("{label}: P3"));
        close(g5, p5, 4, &format!("{label}: P5"));
        assert_ev_row(&label, &d, n.e, &evs);
    }
    for (m, p2, p3, p5, evs) in UNIFORM_TABLE {
        let d = law(g, 1.0, m, 1.0);
        let (g2, g3, g5) = metrics(&d);
        let label = format!("uniform m = {m}");
        close(g2, p2, 4, &format!("{label}: P2"));
        close(g3, p3, 4, &format!("{label}: P3"));
        close(g5, p5, 4, &format!("{label}: P5"));
        assert_ev_row(&label, &d, n.e, &evs);
    }

    // The thresholds (smallest operator count with EV > 0), stake draw and `stake: None`.
    let mut stake_rows = Vec::new();
    let mut uniform_rows = Vec::new();
    for (class, w, r) in CLASSES {
        stake_rows.push(MODELS.map(|m| threshold(g, h, s, m, n.e, w, r)));
        uniform_rows.push(MODELS.map(|m| threshold(g, 1.0, 1.0, m, n.e, w, r)));
        let (st, un) = (stake_rows.last().unwrap(), uniform_rows.last().unwrap());
        println!(
            "T06 {class}: stake-weighted {} | uniform {}",
            MODELS.iter().zip(st).map(|(m, k)| format!("{m} {:.2}M ({k})", (k * n.sybil) as f64 / 1e6)).collect::<Vec<_>>().join(", "),
            MODELS.iter().zip(un).map(|(m, k)| format!("{m} {k}")).collect::<Vec<_>>().join(", ")
        );
    }
    assert_eq!(stake_rows[0], [133, 103, 64, 51, 391], "floor: 17.29M / 13.39M / 8.32M / 6.63M / 50.83M");
    assert_eq!(stake_rows[1], [132, 102, 63, 51, 387], "8k: 17.16M / 13.26M / 8.19M / 6.63M / 50.31M");
    assert_eq!(stake_rows[2], [132, 102, 63, 51, 388], "2M: 17.16M / 13.26M / 8.19M / 6.63M / 50.44M");
    assert_eq!(uniform_rows[0], [20, 16, 10, 8, 56], "stake: None reproduces v3's uniform thresholds on the floor");
    // The ratio row: the design point is 6.65× v3's, in stake.
    let ratio = (stake_rows[0][0] * n.sybil) as f64 / (uniform_rows[0][0] * n.sybil) as f64;
    close(ratio, 6.65, 2, "the design point against v3's uniform draw");
}

// ---- the worst state SW-10 admits -----------------------------------------------------------

fn operator(op: u64) -> Hash64 {
    Hash64::from_u64_word(0x0600_0000 + op)
}

fn bond(n: u64, op: u64, msk: u64) -> (PalwBondKeyV2, PalwBondStateV2) {
    (
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0x0600_0000 + n), index: 0 }),
        PalwBondStateV2 {
            pubkey: vec![n as u8; 4],
            operator_id: operator(op),
            collateral: msk * SOMPI_PER_KASPA,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: Hash64::from_u64_word(1),
            capable_classes: Default::default(),
        },
    )
}

fn refs(list: &[(PalwBondKeyV2, PalwBondStateV2)]) -> Vec<(&PalwBondKeyV2, &PalwBondStateV2)> {
    list.iter().map(|(k, b)| (k, b)).collect()
}

/// The genesis operators are 1..=8; every other operator in these populations is the attacker's
/// (1_000 + j).
fn is_attacker(seat: &PalwPanelSeatV2) -> bool {
    !(1..=8).any(|i| seat.operator_id == operator(i))
}

/// The population of `worst_under_floor`: `k` eligible genesis seats (of eight in the base) and `m`
/// idle floor-sized Sybils (in both). Returns `(eligible, base)`.
#[allow(clippy::type_complexity)]
fn saturated(n: &Net, k: u64, m: u64) -> (Vec<(PalwBondKeyV2, PalwBondStateV2)>, Vec<(PalwBondKeyV2, PalwBondStateV2)>) {
    let mut base: Vec<(PalwBondKeyV2, PalwBondStateV2)> = (1..=n.genesis).map(|i| bond(i, i, n.g_w)).collect();
    base.extend((0..m).map(|j| bond(1_000 + j, 1_000 + j, n.sybil)));
    let eligible = base.iter().filter(|(_, b)| (k + 1..=n.genesis).all(|i| b.operator_id != operator(i))).cloned().collect();
    (eligible, base)
}

/// **The code's SW-10 decision** for `k` eligible genesis seats and `m` idle Sybils beside an
/// executor bond of `x` MSK (`0`: no executor bond could sit): `palw_panel_stake_floor_v1` over
/// `palw_panel_stake_weight_v1` of both lists, and SW-10's executor term — the same weight function
/// over the executor operator's bonds, capped, empty where there is none — added to both sides as
/// `derive_panel_v2_with_policy` adds it.
fn binds(n: &Net, k: u64, m: u64, x: u64) -> bool {
    let (eligible, base) = saturated(n, k, m);
    let executor: Vec<(PalwBondKeyV2, PalwBondStateV2)> = if x == 0 { Vec::new() } else { vec![bond(9_999, 9_999, x)] };
    let x = palw_panel_stake_weight_v1(&refs(&executor), &n.stake);
    assert!(x <= n.stake.weight_cap_msk as u128, "the executor term is capped (IA-1b)");
    match palw_panel_stake_floor_v1(
        palw_panel_stake_weight_v1(&refs(&eligible), &n.stake) + x,
        palw_panel_stake_weight_v1(&refs(&base), &n.stake) + x,
        n.stake.eligible_floor_permille,
    ) {
        Ok(()) => true,
        Err(PalwPanelV2Error::InsufficientEligibleStake { .. }) => false,
        Err(e) => panic!("k = {k}, m = {m}: {e:?}"),
    }
}

/// The Sybils the floor needs with `k` of the genesis seats eligible: the smallest `m` that binds.
fn m_rule(n: &Net, k: u64, x: u64) -> u64 {
    (0..2_000).find(|&m| binds(n, k, m, x)).expect("enough idle Sybils always bind")
}

/// `worst_under_floor`: the cheapest capture over every `k` (ties keep the larger `k`, as the script
/// scans `k` downward), a panel needing five eligible operators. Returns `(m, k)`.
fn worst(n: &Net, name: &str, w: u64, r: u64, x: u64) -> (u64, u64) {
    let mut best: Option<(u64, u64)> = None;
    for k in (0..=n.genesis).rev() {
        let rule = m_rule(n, k, x);
        let race = if k > 0 { threshold(k, n.g_w as f64, n.sybil as f64, name, n.e, w, r) } else { SEATS as u64 };
        let m = rule.max(race).max(if k < SEATS as u64 { SEATS as u64 - k } else { 1 });
        if best.is_none_or(|(b, _)| m < b) {
            best = Some((m, k));
        }
    }
    best.unwrap()
}

/// **T06, SW-10's worst admitted state**: §4.3's worst-state table with the floor decided by the
/// code (`palw_panel_stake_floor_v1` over `palw_panel_stake_weight_v1`), the cliff table per `k`, and
/// IA-1b's executor term at the cap.
#[test]
fn t06_the_worst_admitted_state_under_sw10() {
    let n = net();
    let (floor_w, floor_r) = (CLASSES[0].1, CLASSES[0].2);

    // The floor's rule, per k: 7 of 8 is exactly 875‰ and binds alone; 6 of 8 needs 58 Sybils.
    let rules: Vec<u64> = (0..=n.genesis).rev().map(|k| m_rule(&n, k, 0)).collect();
    assert_eq!(rules, vec![0, 0, 58, 116, 174, 232, 289, 347, 405], "the Sybils the floor needs, k = 8 … 0");

    // The cliff (floor class, P2 with filing and the free redraw), no floor and under it.
    let mut cliff = Vec::new();
    for k in (1..=n.genesis).rev() {
        let p2 = threshold(k, n.g_w as f64, n.sybil as f64, "P2 (file)", n.e, floor_w, floor_r);
        let free = threshold(k, n.g_w as f64, n.sybil as f64, "P2, free redraw", n.e, floor_w, floor_r);
        let under = p2.max(m_rule(&n, k, 0));
        cliff.push((k, p2, free, under));
    }
    assert_eq!(
        cliff,
        vec![
            (8, 133, 64, 133),
            (7, 116, 55, 116),
            (6, 98, 46, 98),
            (5, 81, 37, 116),
            (4, 63, 29, 174),
            (3, 45, 19, 232),
            (2, 26, 3, 289),
            (1, 4, 4, 347),
        ],
        "the cliff table: (k, no floor, no floor with a free redraw, under the 875‰ floor)"
    );
    assert_eq!(m_rule(&n, 0, 0).max(SEATS as u64), 405, "0 of 8: 52.65M of Sybils before any Sybil panel binds");

    // The worst admitted state, per model and class.
    let mut rows = Vec::new();
    for (class, w, r) in CLASSES {
        let row = MODELS.map(|m| worst(&n, m, w, r, 0));
        println!(
            "T06 {class}, worst under SW-10: {}",
            MODELS
                .iter()
                .zip(&row)
                .map(|(m, (s, k))| format!("{m} {:.2}M ({s}, k = {k})", (s * n.sybil) as f64 / 1e6))
                .collect::<Vec<_>>()
                .join(", ")
        );
        rows.push(row);
    }
    assert_eq!(rows[0], [(98, 6), (76, 6), (55, 7), (44, 7), (197, 4)], "floor: 12.74M / 9.88M / 7.15M / 5.72M / 25.61M");
    for (c, class) in [(1, "8k"), (2, "2M")] {
        assert_eq!(
            rows[c].map(|(m, _)| m),
            [97, 75, 54, 44, 195],
            "{class}: 12.61M / 9.75M / 7.02M / 5.72M / 25.35M (before the executor term)"
        );
    }

    // IA-1b: SW-10's executor term at the cap relaxes every floor-bound row by at most X.
    let cap = n.stake.weight_cap_msk;
    assert_eq!(worst(&n, "P2 (file)", floor_w, floor_r, cap), (98, 6), "P2 with filing stays 12.74M: k = 6 is race-bound");
    assert_eq!(worst(&n, "P2, free redraw", floor_w, floor_r, cap), (51, 6), "the free redraw's worst falls to 6.63M (51, k = 6)");
    assert_eq!(m_rule(&n, 6, cap), 51, "6 of 8 eligible: the floor needs 51 Sybils beside a capped executor, not 58");
    assert_eq!(
        threshold(5, n.g_w as f64, n.sybil as f64, "P2 (file)", n.e, floor_w, floor_r).max(m_rule(&n, 5, cap)),
        108,
        "k = 5: 116 / 15.08M becomes 108 / 14.04M"
    );
    for k in 0..=4u64 {
        let before = m_rule(&n, k, 0);
        let after = m_rule(&n, k, cap);
        assert!(
            before - after <= cap.div_ceil(n.sybil) && after * n.sybil > 12_740_000,
            "k = {k}: the executor term moves the floor by at most X ({before} → {after}), still above 12.74M"
        );
    }

    // The rows §4.3 left UNVERIFIED with the executor term (8k/2M, 30% offline, P3, P5), run: counted in
    // Sybil stake a row never rises and falls by at most X (IA-1b); the values are printed for §4.3.
    let bound = cap.div_ceil(n.sybil);
    for (c, (class, w, r)) in CLASSES.iter().enumerate() {
        let with_x = MODELS.map(|m| worst(&n, m, *w, *r, cap));
        for (i, model) in MODELS.iter().enumerate() {
            let (before, after) = (rows[c][i].0, with_x[i].0);
            assert!(after <= before && before - after <= bound, "{class}, {model}: {before} → {after} beside a capped executor");
        }
        println!(
            "T06 {class}, worst under SW-10 beside an executor bond at the cap: {}",
            MODELS
                .iter()
                .zip(&with_x)
                .map(|(m, (s, k))| format!("{m} {:.2}M ({s}, k = {k})", (s * n.sybil) as f64 / 1e6))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

// ---- the law is the race's -------------------------------------------------------------------

/// SplitMix64, for trial seeds.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `trials` panels drawn by `draw` over the same population, each on its own `(claim, anchor)`, with
/// the real segment assignment: the measured `P(A = j)` and the measured P2 (the full seat and the
/// holder of segment `trial mod 4` both the attacker's).
fn measure(
    trials: u64,
    salt: u64,
    draw: &dyn Fn(&Hash64, BlockHash) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error>,
) -> ([f64; SEATS + 1], f64) {
    let mut counts = [0u64; SEATS + 1];
    let mut hits = 0u64;
    for t in 0..trials {
        let word = mix(salt ^ t);
        let claim = Hash64::from_u64_word(word);
        let anchor = BlockHash::from_u64_word(mix(word));
        let seats = draw(&claim, anchor).expect("the population binds");
        assert_eq!(seats.len(), SEATS);
        counts[seats.iter().filter(|s| is_attacker(s)).count()] += 1;
        let assignment = palw_segment_assignment_v2(Hash64::from_u64_word(mix(word)), claim, SEATS as u16);
        let segment = (t % (SEATS as u64 - 1)) as u16;
        let holder = (0..SEATS as u16)
            .find(|&i| i != assignment.full_seat && assignment.mask_of(i).covers(segment))
            .expect("every segment has one partial holder");
        hits += u64::from(is_attacker(&seats[assignment.full_seat as usize]) && is_attacker(&seats[holder as usize]));
    }
    (counts.map(|c| c as f64 / trials as f64), hits as f64 / trials as f64)
}

fn within_4_sigma(measured: f64, exact: f64, trials: u64, what: &str) {
    let sigma = (exact * (1.0 - exact) / trials as f64).sqrt().max(1.0 / trials as f64);
    assert!(
        (measured - exact).abs() <= 4.0 * sigma,
        "{what}: measured {measured:.4}, the law says {exact:.4} (4σ = {:.4})",
        4.0 * sigma
    );
}

/// **T06, the premise: the thresholds' law is the real race's.** On the design point (8 genesis
/// seats beside 133 Sybils), the P3 threshold's population (51), the worst admitted state (6 of 8
/// eligible beside 98) and at `stake: None` (the operator lottery beside 20, against the uniform
/// law), the attacker-seat distribution and P2 measured over the real draw and the real segment
/// assignment sit within 4σ of the law.
#[test]
fn t06_the_real_race_follows_the_successive_sampling_law() {
    let n = net();
    const TRIALS: u64 = 6_000;
    for (label, k, m) in [("design point", 8u64, 133u64), ("P3 threshold", 8, 51), ("worst admitted state", 6, 98)] {
        let (eligible, base) = saturated(&n, k, m);
        let (eligible, base) = (refs(&eligible), refs(&base));
        let draw =
            |claim: &Hash64, anchor: BlockHash| palw_panel_stake_race_of_v1(SEATS as u16, claim, anchor, &eligible, &base, &n.stake);
        let (measured, p2) = measure(TRIALS, 0x06_0000 + k * 1_000 + m, &draw);
        let exact = law(k, n.g_w as f64, m, n.sybil as f64);
        for j in 0..=SEATS {
            within_4_sigma(measured[j], exact[j], TRIALS, &format!("{label}: P(A = {j})"));
        }
        within_4_sigma(p2, metrics(&exact).0, TRIALS, &format!("{label}: P2 through the segment assignment"));
        println!("T06 {label} (k = {k}, m = {m}): P2 measured {p2:.4}, law {:.4}; A {measured:.4?}", metrics(&exact).0);
    }
    // `stake: None`: ADR-0130's one ticket per operator, the uniform law.
    let (eligible, _) = saturated(&n, 8, 20);
    let eligible = refs(&eligible);
    let draw = |claim: &Hash64, anchor: BlockHash| palw_panel_operator_lottery_of_v1(SEATS as u16, claim, anchor, &eligible);
    let (measured, p2) = measure(TRIALS, 0x06_5555, &draw);
    let exact = law(8, 1.0, 20, 1.0);
    for j in 0..=SEATS {
        within_4_sigma(measured[j], exact[j], TRIALS, &format!("stake: None: P(A = {j})"));
    }
    within_4_sigma(p2, metrics(&exact).0, TRIALS, "stake: None: P2");
    // And the two draws differ where they should: at 20 Sybils the stake race seats far fewer.
    let stake_p2 = metrics(&law(8, n.g_w as f64, 20, n.sybil as f64)).0;
    assert!(stake_p2 < 0.2 * metrics(&exact).0, "stake-weighted P2 {stake_p2:.4} against uniform {:.4}", metrics(&exact).0);
    println!("T06 stake: None (m = 20): P2 measured {p2:.4}, uniform law {:.4}", metrics(&exact).0);
}
