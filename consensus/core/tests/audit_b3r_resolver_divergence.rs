//! AUDIT-ONLY (lane B3, re-run). Measures the registration-resolver vs runtime-validation
//! divergence on testnet-12. Creates nothing outside this file, mutates nothing. Delete freely.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_economic_payout_v1::{PalwEconomicPayoutFoldV1, palw_claim_economics_snapshot_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1;
use kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step::step_leaf_count_capped_v1;

/// The per-leaf slot `base0_dense_step_leaves_capped_v1` allocates (a `Hash64`).
const LEAF_SLOT_BYTES: u64 = 64;

#[test]
fn audit_b3r_what_the_seat_allocates_and_what_the_claim_is_worth() {
    let p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is a V2 net") };

    let network_ladder = bundle.court.max_step_leaf_count();
    println!("t12 bundle.court.max_step_leaf_count() = {network_ladder} (2^{:.1})", (network_ladder as f64).log2());
    println!("t12 bundle.court.max_close_bytes()     = {}", bundle.court.max_close_bytes());
    println!("t12 panel seats={} quorum={}", bundle.panel.seat_count(), bundle.panel.quorum());

    let payout = p.palw_economic_payout.expect("t12 arms the payout");
    println!(
        "t12 payout: rate={} sompi/1e9  alpha={}  share[{}..{}]  cap_util_max={}",
        payout.rate_sompi_per_giga,
        payout.panel_share_alpha_permille,
        payout.panel_share_min_permille,
        payout.panel_share_max_permille,
        payout.cap_utilization_max_permille
    );

    // The escrow a t12 claim really carries: worker carve of the block subsidy the chain pays.
    // 444_562_014_000 sompi/block (measured by the recon, CoinbaseManager::calc_block_subsidy) at
    // the fence's 720 permille.
    let escrow_real: u64 = 444_562_014_000u64 / 1_000 * 720;
    println!("escrow per claim (real subsidy x 720 permille) = {escrow_real} sompi = {:.2} MSK", escrow_real as f64 / 1e8);

    for object in bundle.genesis_objects.iter() {
        let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, pwu_rule, share_permille, admission, .. } = object
        else {
            continue;
        };
        let short = &class_id.to_string()[..16];
        let declared = match pwu_rule {
            PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
            other => panic!("t12 registers only derived rules, got {other:?}"),
        };
        println!("\n===== class {short} =====");
        println!("  registered artifact_root = {}", &artifact_root.to_string()[..16]);
        println!("  share_permille           = {share_permille}");
        println!("  declared pwu_per_inference (U1, leaves) = {declared}");

        let Some(carriage) = admission.as_ref() else {
            println!("  (genesis floor: no carriage on the object — the catalog holds its counts)");
            println!("  seat allocation at the NETWORK ladder = {} bytes ({:.2} GiB)",
                network_ladder.saturating_mul(LEAF_SLOT_BYTES),
                network_ladder as f64 * LEAF_SLOT_BYTES as f64 / (1u64 << 30) as f64);
            continue;
        };
        let profile = &carriage.profile;
        let canonical = carriage.canonical.clone();
        let ladder = palw_class_step_ladder_v1(network_ladder, profile);
        println!("  n_ctx={} layers={} class ladder = {ladder} (2^{:.1})", profile.n_ctx, profile.layer_count, (ladder as f64).log2());

        // The job an ATTEMPT actually runs on t12 (palw_prefill_draw armed at 0).
        let attempt_job = palw_attempt_job_v1(canonical.clone(), p.palw_prefill_draw_active_at(0));
        let canonical_leaves = step_leaf_count_capped_v1(profile, &canonical, ladder).expect("the canonical job counts");
        let attempt_leaves = step_leaf_count_capped_v1(profile, &attempt_job, ladder).expect("the attempt job counts");
        println!(
            "  canonical job ({},{}) -> {canonical_leaves} leaves ; attempt job ({},{}) -> {attempt_leaves} leaves",
            canonical.declared_prefill_tokens, canonical.exact_decode_tokens,
            attempt_job.declared_prefill_tokens, attempt_job.exact_decode_tokens
        );
        assert_eq!(canonical_leaves, declared, "the declared pwu is the canonical count");

        // What a seat's `base0_dense_step_leaves_capped_v1` allocates for the HONEST capture, and
        // what it allocates for the largest step_leaf_count the ladder still admits.
        println!(
            "  honest capture leaf vector = {} bytes ({:.3} GiB); attacker-declarable maximum = {} bytes ({:.1} GiB)",
            canonical_leaves.saturating_mul(LEAF_SLOT_BYTES),
            canonical_leaves as f64 * LEAF_SLOT_BYTES as f64 / (1u64 << 30) as f64,
            ladder.saturating_mul(LEAF_SLOT_BYTES),
            ladder as f64 * LEAF_SLOT_BYTES as f64 / (1u64 << 30) as f64
        );

        // The economic value of one licensed claim, priced the way the ledger prices it.
        let descriptor = PalwCanonicalClassDescriptorV1::of(profile, Hash64::default()).expect("a descriptor");
        let draw_ccu = palw_canonical_draw_work_v1(&descriptor, &canonical, true).expect("the draw counts").provisional_scalar_v1();
        println!("  derived work of one draw (U2, MAC-eq) = {draw_ccu}");

        let fold = PalwEconomicPayoutFoldV1 {
            rate_sompi_per_giga: payout.rate_sompi_per_giga,
            panel_share_alpha_permille: payout.panel_share_alpha_permille,
            panel_share_min_permille: payout.panel_share_min_permille,
            panel_share_max_permille: payout.panel_share_max_permille,
            cap_utilization_max_permille: payout.cap_utilization_max_permille,
            block_bits: 0,
        };
        let work = PalwModelWorkV1 {
            verification_ccu: draw_ccu,
            economic_ccu_per_claim: draw_ccu,
            artifact_bytes: 0,
            working_set_bytes: 0,
            ops_supported: true,
        };
        // The class target the genesis object carries.
        let PalwConsensusObjectV2::ClassRegistered { initial_target, .. } = object else { unreachable!() };
        let snapshot = palw_claim_economics_snapshot_v1(&fold, &work, bundle.panel.seat_count(), *initial_target, 0);
        let priced = snapshot.priced_reward(escrow_real);
        println!(
            "  genesis initial_target = {initial_target}  -> priced_reward(escrow={escrow_real}) = {priced} sompi = {:.4} MSK",
            priced as f64 / 1e8
        );
    }
}

/// Does the registration gate check the artifact root against ANYTHING? Re-derive the t12 dense
/// row's own registration with a garbage root and ask the gate.
#[test]
fn audit_b3r_the_registration_gate_never_looks_at_the_artifact_root() {
    use kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v9;
    let p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is a V2 net") };

    let mut checked = 0usize;
    for object in bundle.genesis_objects.iter() {
        let PalwConsensusObjectV2::ClassRegistered { admission: Some(carriage), artifact_root, .. } = object else { continue };
        let profile = carriage.profile.clone();
        let canonical = carriage.canonical.clone();

        // The object exactly as registered, but with the root replaced by a value no artifact on
        // earth produces.
        let mut tampered = object.clone();
        let PalwConsensusObjectV2::ClassRegistered { artifact_root: root_mut, .. } = &mut tampered else { unreachable!() };
        *root_mut = Hash64::from_u64_word(0xDEAD_BEEF);

        let shape = kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(&p, bundle, &profile, 0)
            .expect("t12 has an admission shape at daa 0");
        let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
        let verdict = verify_class_admission_v9(
            bundle,
            &profile,
            &canonical,
            &tampered,
            &certified,
            &[],
            shape.ladder,
            shape.court,
            false,
            shape.token_lift,
            shape.fused_dissectable,
            true,
            shape.held,
            shape.kimi_family,
            // 2026-09-23 audit C-4 fence, as the network resolves it.
            p.palw_audit_2026_09_23_active_at(0),
        );
        match verdict {
            Ok(entry) => {
                println!(
                    "class {}: the gate ADMITS a registration whose artifact_root is 0xDEADBEEF (registered root was {}), and \
                     writes it straight into the catalog entry as {}",
                    &carriage.profile.shape_profile_id().to_string()[..16],
                    &artifact_root.to_string()[..16],
                    &entry.artifact_root.to_string()[..16]
                );
                assert_eq!(entry.artifact_root, Hash64::from_u64_word(0xDEAD_BEEF));
                checked += 1;
            }
            Err(e) => println!("class: refused with {e:?} (NOT for the root, check the variant)"),
        }
    }
    assert!(checked > 0, "at least one t12 row carries a carriage");
}
