//! **C4 round 4b (2026-10-10): ADR-0032's 49% PALW reporter share, attacked as self-dealing.**
//!
//! The user's ruling: a self-reporter recovers 49% of what it is slashed, so its net loss is at least 51% of the COLLECTED slash
//! (ADR-0032 / ADR-0176 D6) — never count the gross slash as the attacker's loss. These tests check that statement on the functions
//! the folds call at or past INTF's dormant fence `palw_reporter_share_v2` (`*_v2`, 4,900 bps; below it `*_v1` keeps the historical
//! 1,000 bps of live int-12), with R-1's `X` and integer rounding, then the DA-6 exposure at 49%, and the kernel route's own interim
//! share.
//!
//! Convention (rounds 1–4): a defect's test asserts the SAFE property and is `#[ignore = "FAIL F-C4R4-nn …"]`; an `obs_` test pins
//! by-design behaviour, quantified.

#![allow(warnings)]
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_da_rcore_v1::{
    PALW_DA_REFUTED_COST_BPS_V1, PALW_DA_REFUTED_COST_BPS_V2, PalwDaStageV1, palw_da_session_exposure_v1, palw_da_session_exposure_v2,
    palw_da_stage_reward_base_v1,
};
use kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1;
use kaspa_consensus_core::palw_state_v2::{
    PALW_RCORE_REPORTER_REWARD_BPS_V1, PALW_RCORE_REPORTER_REWARD_BPS_V2, palw_reporter_reward_amount_v1,
    palw_reporter_reward_amount_v2, palw_reporter_reward_extracted_v1,
};

const MSK: u64 = 100_000_000;

/// The collected debits worth trying: the ADR's worked examples, the rounding edges and the top of the range.
fn collected_grid() -> Vec<u64> {
    let mut v = vec![0, 1, 2, 3, 99, 100, 101, 102, 103, 1_000, 1_003, 9_999, 10_000, 10_001];
    v.extend([320_095 * MSK / 100, 3_237_889 * MSK / 100, 960_289 * MSK / 100, 19_079_747 * MSK / 100, 13_000 * MSK]);
    v.extend([u64::MAX / 3, u64::MAX - 1, u64::MAX]);
    v
}

/// **Holds: every conviction's self-reporter keeps at most 49% of what that conviction collected**, whatever `X` (R-1's deduction
/// only lowers the base) and with the share rounded DOWN to whole sompi — so the self-dealing coalition's loss on each conviction is
/// at least `⌈51% × collected⌉`. One offence pays one reward (R-4) and a DA default rewards the producer's debit only, never a
/// covering signer's S4 (DA-7: burned), so a producer that funds its own reporter Sybil recycles at most 49% of each slash.
#[test]
fn r_core_a_self_reporter_loses_at_least_51_percent_of_every_collected_debit() {
    assert_eq!(PALW_RCORE_REPORTER_REWARD_BPS_V2, 4_900, "past palw_reporter_share_v2: ADR-0032's 49%");
    assert_eq!(PALW_RCORE_REPORTER_REWARD_BPS_V1, 1_000, "below it (live int-12): the historical 10%");
    let mut checked = 0u64;
    for collected in collected_grid() {
        for x in [0u128, 1, 2, 3, 100, collected as u128 / 2, collected as u128, collected as u128 + 1, u128::MAX] {
            let r = palw_reporter_reward_amount_v2(collected, x) as u128;
            let c = collected as u128;
            assert!(palw_reporter_reward_amount_v1(collected, x) as u128 <= r, "10% never pays more than 49%");
            assert!(r * 10_000 <= c * 4_900, "collected {c}, X {x}: reward {r} above 49%");
            // The loss is at least 51% of the collected debit, rounded up in the attacker's disfavour.
            assert!((c - r) * 100 >= c * 51, "collected {c}, X {x}: loss {} below 51%", c - r);
            checked += 1;
        }
    }
    eprintln!("[C4R4 49%] R-1: reward ≤ ⌊49% × collected⌋ and loss ≥ 51% of collected over {checked} (collected, X) pairs");
}

/// **F-C4R4-18 (P3, accounting exactness): the post-Final self-conviction bound `−51% × (ΣS − G_res)` must hold to the sompi.**
///
/// ADR-0032's amendment states that after Final a self-convicting coalition nets `−0.51 × (ΣS − G_res) < 0` and that "integer rounding
/// further reduces what is received". R-1's own rounding does (the reward floors), but R-5's `X = min(lock, ⌊G_res / basis_k⌋)` floors
/// the DEDUCTION, so the `basis_k` deductions sum to less than `G_res` by up to `basis_k − 1` sompi and the reward base is that much
/// larger: the coalition nets up to `0.49 × (basis_k − 1)` sompi MORE than the stated bound. Economically nothing (under 1 sompi at
/// `k = 3`), but the stated bound is not exact; `G_res.div_ceil(basis_k)` restores it (the deductions then cover `G_res`).
/// Counterexample: `G_res` = 2 sompi, `k` = 3 locks of 1 sompi plus a 99-sompi action each.
#[test]
#[ignore = "FAIL F-C4R4-18: X floors G_res / basis_k, so a post-Final self-conviction can net up to 0.49 × (k − 1) sompi above −51% × (ΣS − G_res)"]
fn r_core_the_post_final_self_conviction_bound_holds_to_the_sompi() {
    let mut worst: Option<(u64, u8, u64, u64, i128, i128)> = None;
    for g_res in [1u64, 2, 3, 5, 7, 12 * MSK / 100 + 1, 543 * MSK + 2] {
        for k in [2u8, 3] {
            for lock_over in [0u64, 1] {
                for action in [0u64, 1, 99, 1_300 * MSK + 1] {
                    let lock = g_res.div_ceil(k as u64) + lock_over;
                    let c = lock + action;
                    let x = palw_reporter_reward_extracted_v1(lock as u128, g_res as u128, k);
                    let rewards: i128 = (0..k).map(|_| palw_reporter_reward_amount_v2(c, x) as i128).sum();
                    let total = c as i128 * k as i128;
                    if total <= g_res as i128 {
                        continue;
                    }
                    let net = g_res as i128 - total + rewards;
                    // The bound, ×100: net ≤ −0.51 × (ΣS − G_res).
                    let excess = net * 100 + 51 * (total - g_res as i128);
                    if excess > 0 && worst.is_none_or(|w| excess > w.5) {
                        worst = Some((g_res, k, lock, action, net, excess));
                    }
                }
            }
        }
    }
    if let Some((g, k, lock, action, net, excess)) = worst {
        eprintln!(
            "[F-C4R4-18] G_res {g}, k {k}, lock {lock}, action {action}: the coalition nets {net} sompi, {:.2} sompi above \
             −51% × (ΣS − G_res)",
            excess as f64 / 100.0
        );
    }
    assert!(worst.is_none(), "the post-Final bound is exceeded: {worst:?}");
}

/// **OBS (DA-6 at 49%): a refuted DA session now burns 49% of its stage's reward base** — 4.9× the 10% figure — so an honest verifier
/// that asks a producer for material it then serves pays `⌈49% × S_P⌉` (capped at `min_collateral`): 1,568.49 MSK at the floor's
/// `Live` stage (320.10 MSK at 10%). With ADR-0177 (no availability duty, nothing published proactively) every honest material request
/// is such a session, so the 49% share raises the verifier's cost of checking an honest claim 4.9× together with its bounty —
/// PRINCIPLES §6 condition 6 (verifier incentive) is not improved by the share. And the module's promise that "the refuted cost never
/// exceeds the reward a correct accusation earns" holds only up to rounding and `S_P` vs `collected`: the exposure rounds UP from the
/// stage base (3,201.0 MSK), the reward DOWN from the collected debit (3,200.95 MSK in the ADR's example) — the cost is 0.0245 MSK more.
#[test]
fn obs_da6_a_refuted_session_costs_49_percent_of_its_stage_base() {
    let floor = 13_000 * MSK;
    let live = palw_da_stage_reward_base_v1(PalwDaStageV1::Live, 3_201 * MSK as u128, floor, u128::MAX / 4);
    let exposure = palw_da_session_exposure_v2(live, floor);
    let at_10 = palw_da_session_exposure_v1(live, floor);
    let reward = palw_reporter_reward_amount_v2(320_095 * MSK / 100, 0) as u128;
    eprintln!(
        "[O-C4R4-DA6] Live base {live}: a refuted session burns {exposure} (at 10%: {at_10}, ×{:.1}); a correct accusation on the \
         ADR's collected debit (3,200.95 MSK) earns {reward}: the cost exceeds it by {}",
        exposure as f64 / at_10 as f64,
        exposure as i128 - reward as i128
    );
    assert_eq!((PALW_DA_REFUTED_COST_BPS_V1, PALW_DA_REFUTED_COST_BPS_V2), (1_000, 4_900));
    assert_eq!((at_10, exposure), (32_010_000_000, 156_849_000_000));
    assert!(exposure > reward, "rounding up from the stage base vs down from the collected debit");
    assert_eq!(palw_da_session_exposure_v2(u128::from(floor) * 10, floor), u128::from(floor), "capped at min_collateral");
}

/// **F-C4R4-16 (P2, dormant): the kernel route's interim accuser share must not exceed ADR-0032's 49%.** The route's interim ledger
/// policy pays a convicting accuser `accuser_reward_permille` = 500 (and a default's demanders the same share): a self-reporter
/// recovers 50% of its slash, so its net loss is 50% — under the 51% ADR-0032 / ADR-0176 D6 state for PALW. The OPV relations
/// (`required_reservation` divides by `1 − share`) and `LedgerPolicyV1::validate` follow whatever the field says, so 490 is a
/// one-constant change (with the kernel tests' bounty figures).
#[test]
// F-C4R4-16 is fixed in integ (PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1 = 490), so this guard runs (merge sweep 2026-10-15).
fn the_kernel_routes_interim_accuser_share_is_at_most_adr_0032s_49_percent() {
    let p = palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
    let recovered = p.claim_collateral as u128 * p.accuser_reward_permille as u128 / 1000;
    eprintln!(
        "[F-C4R4-16] interim kernel route: accuser share {}‰ — a self-reporter recovers {recovered} of a {} slash ({:.1}% net loss)",
        p.accuser_reward_permille,
        p.claim_collateral,
        100.0 - p.accuser_reward_permille as f64 / 10.0
    );
    assert!(p.accuser_reward_permille <= 490, "{}‰ > 490‰", p.accuser_reward_permille);
}
