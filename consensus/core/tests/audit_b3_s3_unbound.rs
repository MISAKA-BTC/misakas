//! AUDIT-ONLY (lane B3). Measures the t12 S3 sampling geometry and the panel cut.
//! Creates nothing, mutates nothing. Delete freely.

use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_layer_sample_v3::{PALW_LAYER_SAMPLE_V3_SITES, palw_layer_sample_v3};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_verification_v2::{palw_segment_assignment_v2, palw_segment_count_v2};
use kaspa_hashes::Hash64;

#[test]
fn audit_b3_measure_the_s3_sample_against_the_job() {
    let p = palw_t12_shipped_params();
    println!("t12 palw_verification_v2_at(0)   = {}", p.palw_verification_v2_at(0));
    println!("t12 palw_verification_s3_at(0)   = {}", p.palw_verification_s3_at(0));
    println!("t12 palw_verification_s2         = {:?}", p.palw_verification_s2);
    println!("t12 palw_prefill_draw_active_at(0) = {}", p.palw_prefill_draw_active_at(0));
    println!("t12 palw_held_context_active_at(0) = {}", p.palw_held_context_active_at(0));

    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("t12 is a V2 net") };
    let seats = bundle.panel.seat_count();
    let quorum = bundle.panel.quorum();
    let k = palw_segment_count_v2(seats);
    println!("panel: seat_count={seats} quorum={quorum} segments(K)={k}");

    let a = palw_segment_assignment_v2(Hash64::from_u64_word(0xA11C), Hash64::from_u64_word(0xC1A1), seats);
    println!("assignment: full_seat={} masks={:?}", a.full_seat, a.masks.iter().map(|m| m.0).collect::<Vec<_>>());

    for object in bundle.genesis_objects.iter() {
        let PalwConsensusObjectV2::ClassRegistered { class_id, admission, pwu_rule, .. } = object else { continue };
        let Some(carriage) = admission.as_ref() else {
            println!("class {} : no admission carriage (genesis floor)", &class_id.to_string()[..16]);
            continue;
        };
        let profile = &carriage.profile;
        let canonical = carriage.canonical.clone();
        let attempt_job = palw_attempt_job_v1(canonical.clone(), p.palw_prefill_draw_active_at(0));
        let layers = profile.layer_count as u64;
        let positions = attempt_job.declared_prefill_tokens as u64 + attempt_job.exact_decode_tokens.max(1) as u64 - 1;
        let total_sites = layers * positions;
        let sampled = PALW_LAYER_SAMPLE_V3_SITES as u64;
        println!(
            "class {} n_ctx={} layers={} canonical=({},{}) attempt_job=({},{}) positions={} \
             total (layer,position) sites={} sampled={} fraction=1/{} pwu_rule={:?}",
            &class_id.to_string()[..16],
            profile.n_ctx,
            layers,
            canonical.declared_prefill_tokens,
            canonical.exact_decode_tokens,
            attempt_job.declared_prefill_tokens,
            attempt_job.exact_decode_tokens,
            positions,
            total_sites,
            sampled,
            total_sites / sampled.max(1),
            pwu_rule
        );
        // the draw itself, for one seat
        let sites = palw_layer_sample_v3(
            Hash64::from_u64_word(0xA11C),
            Hash64::from_u64_word(0xC1A1),
            1,
            profile.layer_count,
            u32::try_from(positions).unwrap_or(u32::MAX),
            PALW_LAYER_SAMPLE_V3_SITES,
        );
        println!("   seat 1 draws {} sites: {:?}", sites.len(), &sites[..sites.len().min(8)]);
    }
}
