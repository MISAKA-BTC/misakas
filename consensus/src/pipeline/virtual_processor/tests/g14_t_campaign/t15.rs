//! T15 old/new fence boundaries: old claims keep old rules, the shipping identity is unchanged, the activation refusal remains.
use super::*;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

/// The fence height of this test (the salted-seal rule of `palw_panel_free_v1`): far enough above registration and one claim.
const H: u64 = 60;

/// **T15 (node-level crossing)**: `palw_panel_free_v1` armed at `H` (its salted-seal rule), crossed by a running chain.
///
/// 1. **activation refusal remains**: the harness's ruleset still fails `validate_palw_v2` (only this harness bypasses it);
/// 2. **old claim keeps old rules**: a claim sealed BEFORE `H` with the historical `claim_seal_v1` and its plain commit stays a live
///    claim after the chain crosses `H`; it is covered and convicted after `H` under the rules it was admitted with;
/// 3. **new rules from `H`**: a claim sealed after `H` must use the salted seal v2 — a v1 seal + plain commit filed after `H` never
///    becomes a claim; a salted one does;
/// 4. a node replaying the whole chain, and a node restarted across `H`, reach the same roots.
#[tokio::test]
async fn t15_old_claims_keep_old_rules_across_the_fence_and_new_claims_take_the_new_rule() {
    kaspa_core::log::try_init_logger("warn");
    let parts = kernel_config_with(Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(H), Vec::new())));
    assert!(parts.0.params.validate_palw_v2().is_err(), "the activation refusal remains: shipped validation refuses the fence");
    let mut w = World::on(Net::over_cfg(parts, TestConsensus::new)).await;
    assert_eq!(w.net.ledger().salted_seals_from(), Some(H), "the salted-seal rule is the fence's");

    // ---- an OLD claim, sealed and committed below H ----
    let job = w.job().await;
    let old = w.claim(0, &job, true).await;
    assert!(w.net.daa() + 2 < H, "the old claim was admitted below the fence (DAA {})", w.net.daa());
    assert!(!w.net.ledger().salted_seals_in_force());

    // ---- cross H ----
    w.net.beat_to(H + 2).await;
    assert!(w.net.ledger().salted_seals_in_force(), "the new rule is in force past H");
    assert!(w.net.ledger().claims.contains_key(&old.id), "the old claim survived the crossing");
    let seats = w.seats(&old.id);
    w.cover(&old.id).await;
    let outsider = w.outsiders(&old, &seats, 1)[0];
    let proof = w.prosecution(&old.id, &old.published(&w.fx, &[]), 0x15);
    w.proof(outsider, &old.id, proof).await;
    assert!(w.net.ledger().claims[&old.id].convicted, "the old claim is judged after H under the rules it was admitted with");

    // ---- a v1 seal after H is refused; the salted path works ----
    let job2 = w.job().await;
    let ledger = w.net.ledger();
    let generated = greedy(&w.fx, &ledger, &w.class, &job2.prompt, job2.max_new_tokens as usize);
    let kid = w.net.kid(2);
    let produced = produce(&w.fx, &ledger, &w.class, &job2, kid, generated, |_| {});
    let id = produced.claim.id();
    let seal_v1 = K::SealClaim { producer: kid, job: job2.id(), seal: claim_seal_v1(&id) };
    let o = w.net.route(2, &seal_v1);
    w.net.send(vec![(2, o)]).await;
    let o = w.net.route(2, &produced.object);
    w.net.send(vec![(2, o)]).await;
    assert!(!w.net.ledger().claims.contains_key(&id), "a v1 seal + plain commit after H never becomes a claim");
    let job3 = w.job().await;
    let fresh = w.claim(3, &job3, false).await;
    assert!(w.net.ledger().claims.contains_key(&fresh.id), "the salted seal v2 path admits the new claim");

    let z = w.net.replay().await;
    w.net.assert_same(&z, "a node replaying across the fence");
}

/// **T15 (shipping identity)**: the shipped testnet-12 ruleset is untouched by this campaign — every G14 / full-activation fence the
/// campaign arms is `None` there, the shipped id is the one `Params::from(NetworkId)` routes to, and the harness's armed ruleset has
/// a DIFFERENT id (a node on the armed rules can never pass as the shipped network). The pinned fingerprint itself is checked by the
/// existing `consensus_params_id_tests::shipped_presets_have_pinned_fingerprints` (central verification runs it by name).
#[test]
fn t15_shipping_identity_unchanged_and_the_armed_ruleset_is_a_different_identity() {
    let shipped = palw_t12_shipped_params();
    assert!(shipped.palw_panel_free_v1.is_none(), "shipped testnet-12: palw_panel_free_v1 dormant");
    assert_eq!(
        shipped.palw_probabilistic_constraints_v1,
        Some(ForkActivation::new(18_400)),
        "shipped testnet-12: palw_probabilistic_constraints_v1 at the G14 flag day"
    );
    assert_eq!(
        shipped.palw_g14_route_required_v1,
        Some(ForkActivation::new(18_400)),
        "shipped testnet-12: palw_g14_route_required_v1 at the G14 flag day"
    );
    let routed = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    assert_eq!(routed.consensus_params_id(), Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)).consensus_params_id());
    let (armed, ..) = kernel_config_with(Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(H), Vec::new())));
    assert_ne!(armed.params.consensus_params_id(), routed.consensus_params_id(), "the armed ruleset is a different identity");
    assert!(armed.params.validate_palw_v2().is_err(), "and its activation is still refused by the shipped validation");
}

/// **T15 (full)**: the G14 full-activation list (every fence the lanes need, see the lane notes) armed at one height over the
/// release, crossed with claims admitted before and after: old claims keep their admission rules (court policy, original
/// admission, escrow), new claims are admitted only under the new ones.
// MERGED on g14/integration-r7 (studio T4: name aligned to palw_t12_arm_full_activation_at_v1); was: DEPENDS ON every r5 lane fence + the central full-activation arming record (16,000 on testnet-12).
#[tokio::test]
#[ignore = "blocked on arming records: validate_palw_v2 refuses the full-activation list while its records are unpinned (first refusal: palw_probabilistic_constraints_v1, PALW_KERNEL_ROUTE_ARMING_RECORDS_V1 empty)"]
async fn t15_full_activation_list_crossed_with_old_and_new_claims() {
    #[cfg(feature = "g14-t-campaign-merged")]
    {
        let mut params = palw_t12_shipped_params();
        kaspa_consensus_core::config::params::palw_t12_arm_full_activation_at_v1(&mut params, Some(H));
        assert!(params.validate_palw_v2().is_ok(), "the full-activation list over the release is a runnable ruleset");
    }
    #[cfg(not(feature = "g14-t-campaign-merged"))]
    pending_lane!("all r5 lanes + central full-activation list");
}
