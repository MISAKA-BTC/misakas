//! **lane CLOCK — the kernel route claim's X3 notice / hard clock on the native processor** (2026-10-26, review part 2 B-1 / M-1).
//!
//! The Panel=1 world (producer and its one seat collude on a lie) with `palw_claim_hard_deadline_v1` (X3, the DECIDED policy: the
//! windows' composition with the user's 2026-10-22 目安) and `palw_provider_court_v1` armed from genesis through the slice's own
//! entries (`PALW_T12_FULL_*_ENTRY.set`, which also sync the bundle's mirrors). Past X3 the kernel claim's liability horizon is at
//! least `committed + 3,120 + court + grace` (`misaka_palw_kernel::claim_clock`), so an outsider who files AFTER the kernel's old
//! 200-DAA post-Final horizon still convicts and slashes the reservation; without X3 the same filing is refused (the old horizon).
//!
//! FIXTURE LIMITS (reported, not hidden): the processor's `cfg(test)` adjudication seam caps a block at 4 runs (production 64), so
//! the class relation is fed the TEST-ONLY gain bound `TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI` (1 MSK), not the approved
//! 500 MSK (which the 64-run relation passes: `clock_m1_saturation_cost_beats_reservation_reward_and_external_gain`). Gate C is not
//! armed here (lane CERT's harness covers S3a'), and the whole slice list at a low height is not driven end to end (report residual).
//! No cryptographic proof of computation is used (PRINCIPLES §8).

use super::*;
use super::f03k::{TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI, panel_one_config};
use kaspa_consensus_core::config::params::{PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY, PALW_T12_FULL_PROVIDER_COURT_ENTRY};

/// The Panel=1 world's config with X3 (decided policy) and the provider court armed at 0 through the slice's entries, or not.
fn clock_config(clocked: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = panel_one_config(Some(0), Some(TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI));
    if !clocked {
        return (config, bundle, premine, floats);
    }
    let mut params = config.params.clone();
    (PALW_T12_FULL_PROVIDER_COURT_ENTRY.set)(&mut params, Some(ForkActivation::new(0)));
    (PALW_T12_FULL_CLAIM_HARD_DEADLINE_ENTRY.set)(&mut params, Some(ForkActivation::new(0)));
    let x3 = params.palw_claim_hard_deadline_v1.expect("X3 installed");
    assert!(
        kaspa_consensus_core::palw_ruling_fences_v1::palw_x3_unmeasured_terms_v1(&x3.policy).is_empty(),
        "the slice entry installs the decided policy, not the probe"
    );
    assert_eq!(x3.policy.retention_cost_per_daa_sompi, 36_058, "c_retain 目安 (2026-10-22)");
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        unreachable!("testnet-12 is ConsensusV2")
    };
    assert_eq!(bundle.state.claim_hard_deadline_v1().map(|m| m.from_daa), Some(0), "the mirror the fold reads");
    (Config::new(params), bundle, premine, floats)
}

/// A covered lie reaches Final; the outsider files `after_final_by` DAA past the claim's OLD post-Final horizon (Final + 200).
async fn late_outsider(clocked: bool, after_final_by: u64) {
    kaspa_core::log::try_init_logger("warn");
    let mode = Mode::Panel;
    let mut w = World::on(Net::over_cfg(clock_config(clocked), TestConsensus::new)).await;
    let before = w.net.collateral(0);
    let (claim, seats) = claimed(&mut w, mode, 0, true).await;
    assert_eq!(seats.len(), 1, "Panel=1");
    let floor = final_floor(&w, mode, &claim.id);
    let slashed = reservation(&w, mode);
    let policy = w.policy();
    w.net.beat_to(floor).await;
    assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&claim.id));
    let row = w.net.ledger().claims[&claim.id].clone();
    let until = row.liability_until.expect("Final sets the horizon");
    let old = w.net.daa().saturating_add(policy.liability_daa);
    if clocked {
        assert!(until >= row.committed_daa + 3_120 + policy.court_deadline_daa + policy.proof_grace_daa, "the clock's floor: {until}");
        assert!(until <= row.committed_daa + 10_000, "inside the hard window: {until}");
    } else {
        assert!(until <= old + 2, "the historical horizon: {until} vs {old}");
    }
    let file_at = old + after_final_by;
    w.net.beat_to(file_at).await;

    let outsider = w.outsiders(&claim, &seats, 1)[0];
    let fresh = Rpc::of(&w.net.chain).verifier(&claim.id, 0x5A);
    let model =
        acquire(&[ModelPeer::Serves(w.fx.params.clone())], &registered_root(&fresh, &claim.id)).expect("a peer serves the model");
    let da = claim.published(&w.fx, &[]);
    let OutsiderFindingV1::Prosecute(proof) = fresh.check(claim.id, &da, &model) else {
        panic!("the outsider's verifier finds the lie from public material")
    };
    let owed_before = w.net.owed(outsider);
    Rules::file_proof(&mut w.net, outsider, claim.id, proof).await;
    let convicted = w.net.ledger().claims[&claim.id].convicted;
    if clocked {
        assert!(convicted, "past the old 200-DAA horizon, inside the notice window: the outsider convicts");
        assert_eq!(w.net.collateral(0), before - slashed, "the producer bond lost the reservation");
        assert_eq!(w.net.owed(outsider) - owed_before, Rules::bounty(&policy, slashed), "the outsider's bounty");
    } else {
        assert!(!convicted, "without X3 the kernel's own 200-DAA horizon has closed: the filing is refused");
        assert_eq!(w.net.collateral(0), before, "and nothing is slashed");
    }
}

#[tokio::test]
async fn g14_clock_x3_outsider_convicts_after_the_old_kernel_horizon() {
    late_outsider(true, 50).await;
}

#[tokio::test]
async fn g14_clock_control_without_x3_the_old_horizon_refuses_the_same_filing() {
    late_outsider(false, 50).await;
}
