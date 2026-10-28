//! **ADR-0152 v3.1 SW (the pure half): SW-10's floor priced against the review's own numbers, and
//! the race's independence from how its inputs are listed.** Synthetic populations only, through
//! the public race (`palw_panel_stake_race_of_v1`) and weight (`palw_panel_stake_weight_v1`).
//!
//! `docs/handoff/t12-rcore-20260924/v3calc/v31_review_numbers.py`'s `worst_under_floor` is what
//! SW-10's 875‰ was chosen on: an attacker saturates any `k` of the eight genesis seats (they stay
//! in the base, they leave the eligible list) and keeps all of its own floor-sized operators idle
//! (in both). Its `m_rule` is the Sybil count the floor then needs. The first test runs the real
//! race over that split for every `k` and finds the same count, so the floor as built is the floor
//! the review priced — including its accepted corner, a Sybil-only panel at `k = 0` once the Sybils
//! weigh seven times the honest base (405 × 130,000 = 52.65M MSK; the ADR's worst ADMITTED state is
//! `k = 6` at 12.74M, where the Sybils must also win the race).

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
use kaspa_consensus_core::palw_panel_v2::*;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

/// A testnet-12 genesis seat, whole MSK (`G_W` in the script).
const G: u64 = 939_063;
/// The testnet-12 panel floor, whole MSK (`SEAT_FLOOR`): a floor-sized Sybil operator.
const S: u64 = 130_000;
/// The panel's seats (`SEATS`).
const SEATS: u16 = 5;

fn operator(op: u64) -> Hash64 {
    Hash64::from_u64_word(0x1000 + op)
}

fn bond(n: u64, op: u64, msk: u64) -> (PalwBondKeyV2, PalwBondStateV2) {
    (
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 }),
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

/// `worst_under_floor`'s `m_rule` at 875‰: the Sybils the floor needs with `k` of the eight genesis
/// seats eligible, `⌈(875 × 8 − 1000 × k) × G_W / ((1000 − 875) × SEAT_FLOOR)⌉`, zero when the
/// honest seats alone clear it.
fn script_m_rule(k: u64) -> u64 {
    let need = (875i128 * 8 - 1000 * k as i128) * G as i128;
    if need <= 0 { 0 } else { ((need + (125 * S as i128) - 1) / (125 * S as i128)) as u64 }
}

/// **SW-10 as built is `worst_under_floor`'s split.** For `k = 0..=8` eligible genesis seats (the
/// rest saturated: in the base, not eligible) beside `m` idle Sybils (in both), the smallest `m` with
/// which the race binds is the script's `m_rule` — raised to `5 − k` where the count, not the
/// floor, is what refuses: 405, 347, 289, 232, 174, 116, 58, 0, 0. At `k = 0` that panel is all
/// Sybils, the corner the ADR accepts and prices.
#[test]
fn sw10_floor_is_the_split_v31_review_numbers_prices() {
    let stake = PalwPanelStakeDrawV1::V1;
    let claim = Hash64::from_u64_word(77);
    let anchor = BlockHash::from_u64_word(5);
    let mut minima = Vec::new();
    for k in 0..=8u64 {
        let mut found = None;
        for m in 0..1_000u64 {
            let mut base: Vec<(PalwBondKeyV2, PalwBondStateV2)> = (0..8).map(|i| bond(i + 1, i + 1, G)).collect();
            base.extend((0..m).map(|j| bond(100 + j, 100 + j, S)));
            let eligible: Vec<(PalwBondKeyV2, PalwBondStateV2)> = base
                .iter()
                .filter(|(_, b)| b.collateral == S * SOMPI_PER_KASPA || (1..=k).any(|i| b.operator_id == operator(i)))
                .cloned()
                .collect();
            match palw_panel_stake_race_of_v1(SEATS, &claim, anchor, &refs(&eligible), &refs(&base), &stake) {
                Ok(seats) => {
                    if k == 0 {
                        assert!(
                            seats.iter().all(|seat| (1..=8).all(|i| seat.operator_id != operator(i))),
                            "k = 0: only Sybils are eligible, so only Sybils sit"
                        );
                    }
                    found = Some(m);
                    break;
                }
                Err(PalwPanelV2Error::InsufficientEligibleStake { .. }) => {}
                Err(PalwPanelV2Error::InsufficientEligibleBonds { needed, available }) => {
                    assert_eq!((needed, available as u64), (SEATS, k + m), "k = {k}, m = {m}: the count refuses first");
                }
                Err(e) => panic!("k = {k}, m = {m}: {e:?}"),
            }
        }
        let m = found.expect("enough Sybils always bind");
        assert_eq!(m, script_m_rule(k).max((SEATS as u64).saturating_sub(k)), "k = {k}");
        minima.push(m);
    }
    assert_eq!(minima, vec![405, 347, 289, 232, 174, 116, 58, 0, 0]);
}

/// **The race does not read the order of its input, and an operator's weight is the capped sum of
/// its bonds.** One operator splitting 1,300,000 MSK across a 700k and a 600k bond weighs 1,000,000
/// (summed per operator, then capped — splitting buys nothing), and 256 anchors seat the same five
/// distinct operators whether the population is listed forwards or backwards.
#[test]
fn stake_race_ignores_input_order_and_caps_each_operators_sum() {
    let stake = PalwPanelStakeDrawV1::V1;
    let claim = Hash64::from_u64_word(78);
    let mut pop: Vec<(PalwBondKeyV2, PalwBondStateV2)> = (0..8).map(|i| bond(i + 1, i + 1, G)).collect();
    pop.extend((0..12).map(|j| bond(100 + j, 100 + j, S)));
    pop.push(bond(500, 500, 700_000));
    pop.push(bond(501, 500, 600_000));
    assert_eq!(palw_panel_stake_weight_v1(&refs(&pop), &stake), 8 * G as u128 + 12 * S as u128 + 1_000_000);
    let mut rev = pop.clone();
    rev.reverse();
    for a in 0..256u64 {
        let anchor = BlockHash::from_u64_word(0xA000 + a);
        let fwd = palw_panel_stake_race_of_v1(SEATS, &claim, anchor, &refs(&pop), &refs(&pop), &stake).unwrap();
        let back = palw_panel_stake_race_of_v1(SEATS, &claim, anchor, &refs(&rev), &refs(&rev), &stake).unwrap();
        assert_eq!(fwd, back, "anchor {a}");
        let ops: std::collections::BTreeSet<_> = fwd.iter().map(|s| s.operator_id).collect();
        assert_eq!(ops.len(), SEATS as usize, "anchor {a}: one seat per operator");
    }
}
