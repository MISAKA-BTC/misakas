//! **Lane DA16 — the court scope on the typed roots (ADR-0177 D2; G14C GAP-06 and GAP-52), on the real node.** R4X's harness with
//! `palw_provider_court_v1` also armed (test-armed WITHOUT its validation, which refuses every height): every demand the kernel route
//! admits is claim-specific — a retrieval snapshot's slice and the registered `M0` memory are model content and are never demanded —
//! and every terminal stays reachable by a non-seat bond: a conviction from the verifier's OWN snapshot copy, a retrieval claim's own
//! entry demanded and defaulted when withheld, an honest claim reaching Final though nobody serves the snapshot.

use super::*;
use kaspa_consensus_core::palw_provider_court_v1::ProviderSubjectV1;
use misaka_palw_kernel::spec::RETRIEVAL_ENTRY_STAGE_BASE_V1;
use misaka_palw_kernel::spec::retrieval::EntryResponseV1;

/// R4X's typed network with the provider court (and so the court scope) in force from genesis.
async fn scoped_net() -> Net {
    let (config, bundle, premine, floats) = typed_config(true);
    let mut params = config.params.clone();
    params.palw_provider_court_v1 = Some(ForkActivation::new(0));
    assert!(params.validate_palw_provider_court_v1().is_err(), "the real validation refuses every armed height");
    let config = Config::new(params);
    let chain = t12_genesis_chain_on(TestConsensus::new(&config), &config, &bundle, &premine, &floats);
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let funding = floats.clone();
    let mut net = Net { chain, config, bundle, premine, floats, funding, domain, rnd: 0 };
    net.beat_to(1).await;
    net
}

/// The court scope's per-requester tally of `claim` (the claim's provider-court subject row).
fn tally(net: &Net, claim: &Digest) -> Vec<(Hash64, Vec<(u8, u32)>)> {
    let subject = ProviderSubjectV1::KernelClaim { claim: Hash64::from_bytes(*claim) };
    net.api().and_then(|r| r.provider_subject_v1(&subject)).map(|row| row.requested).unwrap_or_default()
}

/// **Retrieval under the court scope.** (1) An honest claim whose producer serves no part of the snapshot: a slice demand is model
/// content and is refused (no demand row, no tally), and the claim reaches Final — a model not being served voids nothing. (2) A wrong
/// item: a non-seat outsider convicts it from its OWN snapshot copy (`WrongItem`, the item opened against the registered root). (3) An
/// entry of a claim — the claim's own output — demanded by a non-seat bond and served by the producer is public from then on; (4) one
/// withheld is the producer's objective default at the deadline, never a conviction. A replaying node agrees.
#[tokio::test]
async fn da16_scope_a_snapshot_is_never_demanded_and_a_retrieval_claims_own_entries_reach_every_terminal() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = scoped_net().await;
    let class = net.register(retrieval_spec(3)).await;
    let data = corpus();
    let SpecClassKindV1::Retrieval { root } = &net.ledger().typed.classes[&class].kind else { panic!() };
    let root = root.clone();
    let none = MapParams::default();
    let whole = Mirror(&data, 0..0);
    let outsider = 6;

    // ---- (1) an honest claim; nobody serves the snapshot; a slice demand is refused; Final ----
    let j1 = RetrievalJobV1 { class, query: vec![0, 1, 0, 1], nonce: [3; 64] };
    let jid = net.post(SpecJobV1::Retrieval(j1.clone())).await;
    let honest = data.retrieve(&root, &j1.query).unwrap();
    let c1 = net.commit(5, SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: net.kid(5), result: honest })).await;
    net.demand(outsider, c1, SNAPSHOT_STAGE_BASE_V1, 2).await;
    assert!(net.ledger().demands.is_empty(), "a snapshot slice is model content: never demanded (ADR-0177 D2)");
    assert!(tally(&net, &c1).is_empty(), "a refused demand counts nothing");
    net.final_of(&c1).await;

    // ---- (2) a wrong item, convicted from the outsider's own copy ----
    let j2 = RetrievalJobV1 { class, query: vec![1, 1, -2, 2], nonce: [1; 64] };
    let jid = net.post(SpecJobV1::Retrieval(j2.clone())).await;
    let mut wrong = data.retrieve(&root, &j2.query).unwrap();
    wrong[1].payload_digest = payload_digest_v1(&[30, 30]);
    let c2 = net.commit(0, SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: net.kid(0), result: wrong })).await;
    let fault = fault_of(fresh(&net, c2, &Da::default(), &none, &whole, 0x61));
    assert!(matches!(fault, SpecFaultV1::Retrieval { stage: 0, fault: RetrievalFaultV1::WrongItem { index: 1, .. } }), "{fault:?}");
    net.file(outsider, c2, fault).await;
    assert!(net.ledger().claims[&c2].convicted, "reachable from the verifier's own copy: no demand of the snapshot needed");

    // ---- (3) an entry, demanded and served: public; the tally names the requester's operator ----
    let j3 = RetrievalJobV1 { class, query: vec![-3, 2, 2, 0], nonce: [2; 64] };
    let jid = net.post(SpecJobV1::Retrieval(j3.clone())).await;
    let result = data.retrieve(&root, &j3.query).unwrap();
    let c3 = net
        .commit(4, SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: net.kid(4), result: result.clone() }))
        .await;
    let stage = RETRIEVAL_ENTRY_STAGE_BASE_V1;
    net.demand(outsider, c3, stage, 0).await;
    assert!(net.ledger().demands.contains_key(&(c3, stage, 0)), "the claim's own entry is claim-specific: demandable");
    let operator = net.chain.tip_state().1.bond(&net.bond(outsider)).unwrap().operator_id;
    assert_eq!(tally(&net, &c3), vec![(operator, vec![(stage, 0)])]);
    let (item, path) = data.open(result[0].id).unwrap();
    let o =
        net.route(4, &K::Respond { claim: c3, stage, position: 0, bytes: borsh::to_vec(&EntryResponseV1 { item, path }).unwrap() });
    net.send(vec![(4, o)]).await;
    assert!(net.ledger().served.contains_key(&(c3, stage, 0)), "served: public from now on");

    // ---- (4) an entry withheld: the producer's default, never a conviction ----
    let j4 = RetrievalJobV1 { class, query: vec![2, 2, 2, 2], nonce: [4; 64] };
    let jid = net.post(SpecJobV1::Retrieval(j4.clone())).await;
    let result = data.retrieve(&root, &j4.query).unwrap();
    let before = net.collateral(5);
    let c4 = net.commit(5, SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: net.kid(5), result })).await;
    net.demand(outsider, c4, stage, 2).await;
    let deadline = net.ledger().demands[&(c4, stage, 2)].deadline_daa;
    net.beat_to(deadline).await;
    let l = net.ledger();
    assert!(
        matches!(l.claims[&c4].life.state, ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "{:?}",
        l.claims[&c4].life.state
    );
    assert!(!l.claims[&c4].convicted, "a withheld entry is a default, never fraud");
    assert_eq!(net.collateral(5), before - l.policy.default_penalty, "the fixed default penalty");
    net.assert_replays().await;
}

/// **Memory under the court scope**: the first claim's pre-state is the registered `M0` — model content, never demanded; once a
/// claim advanced the line, the next claim's pre-state is that claim's public post-state — claim-specific, demandable.
#[tokio::test]
async fn da16_scope_the_registered_memory_is_never_demanded_and_a_carried_pre_state_is() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = scoped_net().await;
    let class = net.register(memory_spec()).await;
    let mut m = MemWorld { net, fx: memory_fx(), class, jobs: 0 };
    let outsider = 6;
    let job1 = m.job(vec![vec![1, 2, 3]]).await;
    let p1 = m.produce(&job1, 0, &m.m0(), |_, _| {});
    let c1 = m.net.commit(0, SpecClaimV1::Memory(p1.claim.clone())).await;
    m.net.demand(outsider, c1, MEMORY_PRE_STATE_STAGE_V1, 0).await;
    assert!(m.net.ledger().demands.is_empty(), "the registered M0 is model content: never demanded");
    m.net.final_of(&c1).await;

    let job2 = m.job(vec![vec![7, 7]]).await;
    let p2 = m.produce(&job2, 4, &m.chain_head(), |_, _| {});
    let c2 = m.net.commit(4, SpecClaimV1::Memory(p2.claim.clone())).await;
    m.net.demand(outsider, c2, MEMORY_PRE_STATE_STAGE_V1, 0).await;
    assert!(
        m.net.ledger().demands.contains_key(&(c2, MEMORY_PRE_STATE_STAGE_V1, 0)),
        "a pre-state carried by an earlier claim is claim-specific: demandable"
    );
    m.net.assert_replays().await;
}
