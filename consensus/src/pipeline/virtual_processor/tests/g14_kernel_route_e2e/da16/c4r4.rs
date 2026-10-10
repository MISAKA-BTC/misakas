//! **C4 round 4b (2026-10-10): ADR-0177 non-interference against the provider court as it stands on the integration tree.**
//!
//! ADR-0177 D1: the chain does not interfere with acquiring a model — no availability lease as a condition, and stopping, slowing or
//! refusing to provide a model to third parties MUST NOT invalidate a registration, claim, reward or Final weight, slash a bond or
//! impose a consensus service penalty. D2: requests are limited to claim-specific witness/state — no weight-file/range request, no
//! request that enumerates the weights, no cumulative rebuild. DA16's court (dormant behind `palw_provider_court_v1`, which validation
//! refuses at every height) predates the ADR; DA16's re-scope is pending. This module pins what the re-scope must change.

use super::*;
use kaspa_consensus_core::palw_onboarding_v1::PalwOnboardingGateV1;

/// **F-C4R4-19 (ADR-0177 D1/D2; dormant, owner DA16's re-scope): a peer that refuses to serve a model's bytes triggers no
/// precondition, slash or hold.** On the tree today (test-armed court, as in DA16's own tests): an artifact binding is refused until
/// two availability leases of the model's bytes exist; an outsider's challenge naming a WEIGHT LEAF (a model-byte range) is accepted;
/// leaving it unanswered slashes the lease; with every lease charged the pair lapses and the class's onboarding gate holds it
/// `AVAILABILITY_REQUIRED`. Each is a violation the test lists. SAFE: none of them.
#[tokio::test]
#[ignore = "FAIL F-C4R4-19: DA16's artifact leases are availability leases (a precondition, model-byte challenges, a slash, a hold) — ADR-0177 re-scope pending"]
async fn g14_c4r4_adr0177_refusing_to_serve_a_models_bytes_triggers_no_precondition_slash_or_hold() {
    kaspa_core::log::try_init_logger("warn");
    let truth = onb_fixture(11);
    let mut net = Net::over_cfg(court_onboarding_config(), TestConsensus::new);
    net.beat_to(1).await;
    let (p1, p2, outsider) = (2usize, 3usize, 4usize);
    let v2_class = register_v2(&mut net, &truth).await;
    let kernel_root = Hash64::from_bytes(truth.pc.root());
    let subject = ProviderSubjectV1::Artifact { v2_class, kernel_param_root: kernel_root };
    let mut violations: Vec<String> = Vec::new();

    // (1) D1: no availability lease is a precondition. The registrant binds with nobody leasing the model's bytes.
    let o = net.artifact_bound(REGISTRANT, v2_class, kernel_root);
    net.send(vec![(REGISTRANT, o)]).await;
    // (No route row may exist yet: a refused first object writes nothing.)
    if net.api().and_then(|route| route.artifact_binding_v1(&v2_class, &kernel_root)).is_none() {
        violations.push("an artifact binding is refused until two availability leases of the model's bytes exist".into());
        let (lease, serve_until) = (mega(150), net.daa() + 400);
        let (l1, l2) = (net.court_lease(p1, subject, lease, serve_until), net.court_lease(p2, subject, lease, serve_until));
        net.send(vec![(p1, l1), (p2, l2)]).await;
        let o = net.artifact_bound(REGISTRANT, v2_class, kernel_root);
        net.send(vec![(REGISTRANT, o)]).await;
    }
    let binding = net.api().unwrap().artifact_binding_v1(&v2_class, &kernel_root).expect("bound (over leases if it had to be)");
    net.beat_to(binding.matures_daa).await;

    // (2) D1/D2: both peers stop serving the model. An outsider asks each for one weight leaf — a model-byte range.
    let (before1, before2) = (net.collateral(p1), net.collateral(p2));
    let c1 = net.court_challenge(outsider, subject, p1, PublicUnitV1::ArtifactLeaf { index: 0 });
    let c2 = net.court_challenge(outsider, subject, p2, PublicUnitV1::ArtifactLeaf { index: 1 });
    net.send(vec![(outsider, c1), (outsider, c2)]).await;
    let open = net.court(&subject).challenges.len();
    if open > 0 {
        violations.push(format!("{open} challenges naming a weight leaf (a model-byte range) were accepted"));
    }
    let until = net.daa() + 25;
    net.beat_to(until).await;
    let (taken1, taken2) = (before1.saturating_sub(net.collateral(p1)), before2.saturating_sub(net.collateral(p2)));
    if taken1 + taken2 > 0 {
        violations.push(format!("refusing to serve the model's bytes was slashed: {taken1} and {taken2} sompi"));
    }
    if net.api().unwrap().provider_pair_lapsed_since_v1(&v2_class, &kernel_root, binding.bound_daa) {
        violations.push("the pair lapsed: the binding attests nothing".into());
    }
    let gate = net.api().unwrap().onboarding_gate_v1(&v2_class, &truth.artifact_root, net.daa());
    if matches!(gate, PalwOnboardingGateV1::Held { code: "AVAILABILITY_REQUIRED", .. }) {
        violations.push(format!("the class's onboarding gate holds it: {gate:?}"));
    }
    eprintln!("[F-C4R4-19] ADR-0177 violations on the integration tree's provider court: {violations:#?}");
    assert!(violations.is_empty(), "{} ADR-0177 violations: {violations:?}", violations.len());
}
