//! **The numbers `docs/testnet11-join-mining.md` prints for a newcomer's bond, derived.**
//!
//! Issue #95: a newcomer registered a bond for a Qwen3.6 producer and its producer then logged
//! `holding: the bond's exposure ceiling leaves no room for another claim` forever. Two causes,
//! and this file pins the arithmetic behind both halves of the fix.
//!
//! 1. **The collateral.** A claim's exposure is released at `Final`, not at bind, so a bond must
//!    hold every claim of its class that can be in flight at once —
//!    `palw_v2_collateral_for_claim_lifetime_v1`, which genesis has always used and the panel's
//!    `size_bond_collateral` did not. It is a function of the CLASS, and the model tiers are two
//!    to three orders of magnitude past the floor.
//! 2. **The funding UTXO.** A bond carrier holds the collateral in one output and the change in
//!    the other, and KIP-0009 storage mass grows as an output SHRINKS — so naming a large
//!    collateral moves the relay problem onto the CHANGE. `build_registration_carrier` REFUSES a
//!    named collateral rather than raising it, so an operator who funds "the collateral plus a
//!    fee" walks out of one refusal straight into a second and much less legible one.
//!
//! A doc that prints numbers nobody re-derives goes stale silently; these are the numbers, and
//! this test is why they can be trusted.

use kaspa_consensus_core::constants::STORAGE_MASS_PARAMETER;
use kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk;
use kaspa_consensus_core::mass::{UtxoCell, calc_storage_mass, utxo_plurality};
use kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_collateral_for_claim_lifetime_v1;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};

/// `kaspa_mining::MAXIMUM_STANDARD_TRANSACTION_MASS`, restated because `kaspa-mining` depends on
/// this crate and cannot be depended on back. If the two ever disagree the doc's funding figure is
/// the thing that goes wrong, which is what this comment exists to make findable.
const RELAY_MASS_LIMIT: u64 = 480_000;

/// The per-inference work a class registration declares — the input the collateral is sized from.
fn pwu_per_inference(object: &PalwConsensusObjectV2) -> u64 {
    match object {
        PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } => match pwu_rule {
            PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
            PalwPwuRuleV2::MaxPerAttempt(cap) => *cap,
        },
        other => panic!("not a class registration: {other:?}"),
    }
}

/// The artifact root a registration is pinned to does not enter the leaf count, so any value
/// derives the same `pwu_per_inference`. Named rather than inlined so that is stated once.
fn any_root() -> kaspa_consensus_core::Hash64 {
    kaspa_consensus_core::Hash64::from_u64_word(0xA57)
}

/// **The two model tiers' per-inference work, and the collateral each one's claims need.**
///
/// The share and slash arguments do not enter `pwu_per_inference` (it is the canonical job's leaf
/// count under the class's frozen graph), so they are the shipped values only for readability.
#[test]
fn the_model_tier_collateral_the_docs_quote() {
    let (_, _, qwen36) = kaspa_consensus_core::palw_qwen36_profile::qwen36_registration_v3(any_root(), 489, 5, u128::MAX / 2)
        .expect("the Qwen3.6 graph-v3 registration derives");
    let (_, _, a16) = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_registration_v2(any_root(), 489, 5, u128::MAX / 2)
        .expect("the Qwen2.5-A16 registration derives");

    // The measured testnet-11 table (ADR-0076, Relaunch 5e runbook). Asserted so a profile change
    // cannot move the collateral the docs print without failing here first.
    assert_eq!(pwu_per_inference(&qwen36), 2_685_360, "PALW-QWEN36 5bd9ae3d… per-inference work");
    assert_eq!(pwu_per_inference(&a16), 1_589_424, "PALW-QWEN25-A16 71bbb755… per-inference work");

    // The floor's, from the same table — the class `size_bond_collateral` used to size EVERY bond
    // against, whatever the operator asked to produce.
    const FLOOR_PWU_PER_INFERENCE: u64 = 7_708;

    // The figures `docs/testnet11-join-mining.md` prints.
    assert_eq!(palw_v2_collateral_for_claim_lifetime_v1(pwu_per_inference(&qwen36)), 386_745_547_200);
    assert_eq!(palw_v2_collateral_for_claim_lifetime_v1(pwu_per_inference(&a16)), 228_908_844_480);
    assert_eq!(palw_v2_collateral_for_claim_lifetime_v1(FLOOR_PWU_PER_INFERENCE), 1_110_106_160);

    // **The size of the defect, stated as a ratio.** The old default sized one claim on the floor
    // and then took `max(min_collateral_sompi)`, landing on 400,000 sompi for every class; a
    // Qwen3.6 bond needs about 966,864 times that, which is why the producer's very first claim
    // filled the ceiling.
    assert!(
        palw_v2_collateral_for_claim_lifetime_v1(pwu_per_inference(&qwen36)) / 400_000 > 900_000,
        "the floor-priced default was short by nearly six orders of magnitude"
    );
}

/// **The smallest CHANGE output a bond carrier can leave and still be relayed** — the second
/// number a newcomer needs, and the one that turns "fund the collateral" into a refusal.
///
/// The carrier is one input and two outputs, all three paying to P2PKH-ML-DSA-87 scripts, so every
/// cell has plurality 2 and the storage mass is
/// `C·4/collateral + C·4/change − 2·(C / (funding/2))`. With a model tier's collateral the first
/// term is a rounding error and the input term cancels it, so the whole limit falls on the change.
#[test]
fn the_change_output_floor_the_docs_quote() {
    let spk = p2pkh_mldsa87_spk(&[7u8; 64]);
    let p = utxo_plurality(&spk);
    assert_eq!(p, 2, "a 69-byte P2PKH-ML-DSA-87 script occupies two 100-byte storage units");

    let collateral = palw_v2_collateral_for_claim_lifetime_v1(2_685_360); // QWEN36
    let mass = |change: u64| {
        let funding = collateral + change;
        calc_storage_mass(
            false,
            std::iter::once(UtxoCell::new(p, funding)),
            [UtxoCell::new(p, collateral), UtxoCell::new(p, change)].into_iter(),
            STORAGE_MASS_PARAMETER,
        )
        .expect("a two-output carrier's mass is computable")
    };

    // The floor the doc prints, and the value one sompi below it — the assertion that makes this a
    // boundary and not a sample.
    const CHANGE_FLOOR: u64 = 8_333_316;
    assert!(mass(CHANGE_FLOOR) <= RELAY_MASS_LIMIT, "{} must clear {RELAY_MASS_LIMIT}", mass(CHANGE_FLOOR));
    assert!(mass(CHANGE_FLOOR - 1) > RELAY_MASS_LIMIT, "one sompi less must be over the limit");

    // And the round figure the doc actually tells a newcomer to fund with: collateral + 0.1 MSK,
    // which leaves room for the carrier fee (a few hundred thousand sompi) on top of the floor.
    const RECOMMENDED_HEADROOM: u64 = 10_000_000;
    assert!(RECOMMENDED_HEADROOM > CHANGE_FLOOR, "the recommendation must clear the floor it is a margin on");
    assert!(mass(RECOMMENDED_HEADROOM - 400_000) <= RELAY_MASS_LIMIT, "and still clear it after a generous fee");
}
