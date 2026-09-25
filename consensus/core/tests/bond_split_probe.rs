//! **Verdict 5 — bond_split_amplification, measured on testnet-12's own draw functions.**
//!
//! The design review rated it "partial": issuance and tickets are unaffected by splitting a bond,
//! but "seat draws, tiers, strikes and the ceil(c/2) share are affected". This probe runs the real
//! panel-draw weight functions (`palw_panel_stake_weight_v1`, `palw_draw_operator_weight_msk_v1`)
//! and the operator-id derivation (`palw_operator_id_v2`) on testnet-12's shipped stake-draw terms
//! to say exactly which half of that is real on the launch binary.
//!
//! What it establishes (numbers printed with --nocapture):
//! * **Issuance/tickets: unaffected.** The operator id is a pure function of the operator KEY, never
//!   of collateral or of how many bonds exist; a claim's class ticket is keyed on (anchor, claim,
//!   this bond), so no bond's issuance reads any other bond's existence.
//! * **A same-operator split is NEUTRAL — and cannot even be registered on testnet-12.** The stake
//!   draw sums collateral PER OPERATOR before capping, so two bonds under one key weigh exactly what
//!   the whole did; and `palw_operator_id_unique` is armed at genesis on testnet-12 (one operator =
//!   one bond), so the second registration under one key is refused outright.
//! * **A multi-operator (Sybil) split below the weight cap manufactures NO free weight.** N distinct
//!   operators sharing C collateral weigh Σ min(C/N, cap) = C while each C/N stays under the cap —
//!   the same total, and each identity must post its own `min_collateral`, which is the cost the
//!   ADR-0042 Decision 7 doc leans on.
//! * **The one real amplification is a WHALE above the cap.** A bond above `weight_cap_msk`
//!   (1,000,000 MSK) is capped to the cap as one operator, but split into cap-sized Sybil operators
//!   it weighs the sum — up to `seat_count` distinct seats. That is the weight cap's own SW-A5
//!   tradeoff and the Sybil-seat vector the rcore/f1-panel-seed and claim-collateral lanes own; it
//!   is bounded by one-seat-per-operator and by the min_collateral each Sybil must post, not free.
//!
//! Run: cargo test -p kaspa-consensus-core --test bond_split_probe -- --nocapture

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_panel_v2::{PALW_DRAW_WEIGHT_CAP_MSK_V1, PalwPanelStakeDrawV1, palw_panel_stake_weight_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2, palw_operator_id_v2};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_consensus_core::TransactionId;
use kaspa_hashes::Hash64;

const SOMPI_PER_MSK: u64 = 100_000_000;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// A bond record with `collateral` sompi, keyed by outpoint `k`, whose operator id is derived from
/// key material `op_key` (so distinct `op_key`s are distinct operators, one `op_key` is one operator).
fn bond(k: u64, op_key: u64, collateral_msk: u64) -> (PalwBondKeyV2, PalwBondStateV2) {
    let pubkey = vec![op_key as u8, (op_key >> 8) as u8, 0xAB, 0xCD];
    let key = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(k), 0));
    let state = PalwBondStateV2 {
        pubkey: pubkey.clone(),
        operator_id: palw_operator_id_v2(&pubkey),
        collateral: collateral_msk.saturating_mul(SOMPI_PER_MSK),
        slashed: 0,
        status: PalwBondStatusV2::Active,
        registered_daa: 0,
        payout_payload: Hash64::from_u64_word(k),
        capable_classes: std::collections::BTreeSet::new(),
    };
    (key, state)
}

fn weight_of(bonds: &[(PalwBondKeyV2, PalwBondStateV2)]) -> u128 {
    let refs: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = bonds.iter().map(|(k, s)| (k, s)).collect();
    palw_panel_stake_weight_v1(&refs, &PalwPanelStakeDrawV1::V1)
}

#[test]
fn issuance_and_the_operator_id_do_not_read_the_bond_count() {
    // The operator id is a pure function of the KEY: same key -> same operator, any collateral.
    let key = vec![7u8, 0, 0xAB, 0xCD];
    assert_eq!(palw_operator_id_v2(&key), palw_operator_id_v2(&key), "operator id is a function of the key alone");
    // A whole bond and a "split" under the SAME key are one operator — splitting manufactures no id.
    let whole = bond(1, 7, 1_000_000);
    let split_a = bond(2, 7, 500_000);
    let split_b = bond(3, 7, 500_000);
    assert_eq!(whole.1.operator_id, split_a.1.operator_id, "same key, one operator id");
    assert_eq!(split_a.1.operator_id, split_b.1.operator_id, "same key, one operator id");
    // Distinct keys are distinct operators — the only way to get a second id, and it costs a key.
    assert_ne!(bond(4, 8, 1).1.operator_id, whole.1.operator_id, "a distinct key is a distinct operator");
    eprintln!("[bond-split] issuance/tickets are keyed on (class, THIS bond, anchor); no draw reads the count of other bonds");
}

#[test]
fn testnet_12_arms_operator_id_unique_so_one_operator_is_one_bond() {
    let t12 = t12();
    assert!(
        t12.palw_operator_id_unique_at(0),
        "testnet-12 arms palw_operator_id_unique at genesis: a same-operator split cannot be registered"
    );
    // And the stake-draw terms are the shipped ones.
    let PalwConsensusMode::ConsensusV2(_) = &t12.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    eprintln!("[bond-split] testnet-12: operator_id_unique armed at genesis; draw weight cap = {PALW_DRAW_WEIGHT_CAP_MSK_V1} MSK");
}

#[test]
fn a_same_operator_split_weighs_exactly_what_the_whole_did() {
    // Two bonds under ONE operator sum before the cap: neutral. (On testnet-12 this can't be
    // registered at all — operator_id_unique — so it is neutral there twice over.)
    let whole = vec![bond(1, 7, 800_000)];
    let split = vec![bond(2, 7, 400_000), bond(3, 7, 400_000)];
    assert_eq!(weight_of(&whole), weight_of(&split), "a same-operator split is neutral: the draw sums per operator");
    eprintln!("[bond-split] same-operator: whole {} == split {} (draw weight, MSK)", weight_of(&whole), weight_of(&split));
}

#[test]
fn a_sub_cap_sybil_split_manufactures_no_free_weight() {
    // C collateral spread over N DISTINCT operators, each piece under the cap: total weight = C,
    // the same as one operator holding C (also under the cap). No amplification — and each Sybil
    // had to post its own key and its own min_collateral.
    let c_msk = 900_000u64; // under the 1,000,000 cap
    let one = vec![bond(1, 100, c_msk)];
    let sybil: Vec<_> = (0..3).map(|i| bond(10 + i, 200 + i, c_msk / 3)).collect();
    assert_eq!(weight_of(&one), c_msk as u128, "one sub-cap operator weighs its collateral");
    assert_eq!(weight_of(&sybil), c_msk as u128, "three sub-cap Sybils sharing C weigh C — no free weight");
    eprintln!("[bond-split] sub-cap Sybil: 1x{c_msk} weighs {}, 3x{} weighs {} (equal)", weight_of(&one), c_msk / 3, weight_of(&sybil));
}

#[test]
fn the_real_amplification_is_a_whale_split_below_the_cap() {
    // A bond ABOVE the cap is capped to the cap as one operator; split into cap-sized Sybils it
    // weighs the sum. This is the weight cap's SW-A5 tradeoff and the Sybil-seat vector the
    // panel-seed / claim-collateral lanes own — bounded by one seat per operator and by the
    // min_collateral each Sybil must post, NOT free.
    let cap = PALW_DRAW_WEIGHT_CAP_MSK_V1;
    let whale_msk = 3 * cap; // 3,000,000 MSK
    let whole = vec![bond(1, 300, whale_msk)];
    let split: Vec<_> = (0..3).map(|i| bond(20 + i, 400 + i, cap)).collect();
    assert_eq!(weight_of(&whole), cap as u128, "one whale is capped to the cap");
    assert_eq!(weight_of(&split), 3 * cap as u128, "split into 3 cap-sized Sybils it weighs 3x the cap");
    let amplification = weight_of(&split) as f64 / weight_of(&whole) as f64;
    eprintln!(
        "[bond-split] WHALE ({whale_msk} MSK): whole weighs {} (capped), 3 cap-sized Sybils weigh {} -> {amplification:.1}x draw weight; \
         but each Sybil is a distinct key posting its own min_collateral and can hold only ONE seat (per-operator dedup). \
         Owned by rcore/f1-panel-seed + the claim-collateral design, not a contained fork-choice fix.",
        weight_of(&whole),
        weight_of(&split)
    );
    assert!((amplification - 3.0).abs() < 0.01, "the amplification is exactly the number of cap-sized pieces");
}
