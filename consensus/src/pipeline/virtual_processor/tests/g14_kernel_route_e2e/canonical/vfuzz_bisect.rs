//! **lane V-FUZZ follow-up (coordinator 2026-10-28): why the honest Panel=1 claim does not commit with the 17,400 list armed.**
//! Bisects the list (prefixes armed at DAA 0 over the J1 world, test-only mirrors restored) and, for a failing prefix, replays the
//! reveal against the tip ledger to read the ledger's own refusal.

use super::cert::*;
use super::*;
use futures_util::FutureExt;
use kaspa_consensus_core::config::params::{PALW_T12_G14_17000_FENCES_V1, palw_t12_arm_after_int13_at_v1};
use kaspa_consensus_core::palw_rfc0018_v1::PalwRfc0018MirrorV1;
use misaka_palw_sdk::kernel_job_v1::{kernel_job_from_text_v1, kernel_tokenizer_v1, tests::tiny_tokenizer_json};
use std::panic::AssertUnwindSafe;

fn panic_text(e: Box<dyn std::any::Any + Send>) -> String {
    e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "?".into())
}

/// The J1 world with the first `k` entries of the 17,400 list armed at `at`, the harness's TEST-ONLY mirrors restored.
fn prefix_config(k: usize, at: u64) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, _, premine, floats) = cert_panel_one_config();
    let mut params = config.params.clone();
    palw_t12_arm_after_int13_at_v1(&mut params, &PALW_T12_G14_17000_FENCES_V1[..k], Some(at));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &mut params.palw_consensus_mode else { unreachable!() };
    let mirror = PalwRfc0018MirrorV1 { certification: TEST_ONLY_KERNEL_CERT_POLICY_V1, ..*b.state.rfc0018_v1().expect("Gate C mirrored") };
    b.state = b
        .state
        .clone()
        .with_test_only_kernel_panel_one_external_gain_bound_v1(Some(super::f03k::TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI))
        .with_rfc0018_v1(Some(mirror));
    let bundle = b.clone();
    (Config::new(params), bundle, premine, floats)
}

/// Post a job and claim it; `Ok` when the claim commits, otherwise what was seen (seal row, the ledger's replayed refusal).
async fn probe_claim(parts: (Config, PalwConsensusParamsV2, Premine, Premine)) -> Result<(), String> {
    let mut w = World::on(Net::over_cfg(parts, TestConsensus::new)).await;
    let tok = kernel_tokenizer_v1(&tiny_tokenizer_json(), None).unwrap();
    let job = kernel_job_from_text_v1(&tok.tokenizer, "hi a", Hash64::from_bytes(w.class), 64, MAX_POSITIONS, 3, Hash64::from_bytes([0x71u8; 64]))
        .map_err(|e| format!("job: {e:?}"))?;
    let o = w.net.route(1, &K::PostJob { job: job.clone() });
    w.net.send(vec![(1, o)]).await;
    if !w.net.ledger().jobs.contains_key(&job.id()) {
        return Err(format!("the job did not post (daa {})", w.net.daa()));
    }
    let producer = 0usize;
    let ledger = w.net.ledger();
    let generated = greedy(&w.fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
    let kid = w.net.kid(producer);
    let produced = produce(&w.fx, &ledger, &w.class, &job, kid, generated, |_| {});
    let (seal, reveal) = seal_and_reveal(&ledger, kid, &produced.object);
    let salted = matches!(reveal, K::CommitClaimSalted { .. });
    let o = w.net.route(producer, &seal);
    w.net.send(vec![(producer, o)]).await;
    let after_seal = w.net.ledger();
    let sealed = after_seal.seals.contains_key(&(job.id(), kid));
    let mut replay = KernelLedgerV1::clone(&after_seal);
    let replayed = replay.apply_object(&reveal, &misaka_palw_kernel::route::AuthV1 { signer_bond: kid }).map(|_| ()).map_err(|e| format!("{e:?}"));
    let o = w.net.route(producer, &reveal);
    w.net.send(vec![(producer, o)]).await;
    if w.net.ledger().claims.contains_key(&produced.claim.id()) {
        return Ok(());
    }
    let state = w.net.chain.tip_state().1;
    let bindings: Vec<_> = state.kernel_route().map(|r| r.kernel_bindings_v1()).unwrap_or_default();
    let ours = bindings.iter().filter(|(_, b)| b.kernel_class == Hash64::from_bytes(w.class)).count();
    Err(format!(
        "bindings_total={} bindings_of_class={ours} salted={salted} salted_from={:?} ledger_daa={} sealed={sealed} ledger_replay(reveal)={replayed:?}",
        bindings.len(),
        after_seal.salted_seals_from(),
        after_seal.daa
    ))
}

#[tokio::test]
async fn vfuzz_claim_commit_bisect_the_17400_list() {
    kaspa_core::log::try_init_logger("error");
    let n = PALW_T12_G14_17000_FENCES_V1.len();
    let run = |k: usize, at: u64| async move { AssertUnwindSafe(probe_claim(prefix_config(k, at))).catch_unwind().await.map_err(panic_text) };
    eprintln!("VFUZZ-BISECT prefix=0/{n} -> {:?}", run(0, 0).await);
    eprintln!("VFUZZ-BISECT prefix={n}/{n} -> {:?}", run(n, 0).await);
    let (mut lo, mut hi) = (0usize, n);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let r = run(mid, 0).await;
        let ok = matches!(r, Ok(Ok(())));
        eprintln!("VFUZZ-BISECT prefix={mid}/{n} ok={ok} {r:?}");
        if ok { lo = mid } else { hi = mid }
    }
    eprintln!("VFUZZ-BISECT first failing entry: #{hi} {}", PALW_T12_G14_17000_FENCES_V1[hi - 1].name);
    // only that entry (with nothing else from the list)
    let only = PALW_T12_G14_17000_FENCES_V1[hi - 1];
    let (config, _, premine, floats) = prefix_config(0, 0);
    let mut params = config.params.clone();
    palw_t12_arm_after_int13_at_v1(&mut params, &[only], Some(0));
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &params.palw_consensus_mode else { unreachable!() };
    let parts = (Config::new(params.clone()), b.clone(), premine, floats);
    let r = AssertUnwindSafe(probe_claim(parts)).catch_unwind().await.map_err(panic_text);
    eprintln!("VFUZZ-BISECT only {} -> {r:?}", only.name);
    // the full list armed later, crossed by the chain during setup (closer to a flag day than genesis arming)
    for at in [40u64, 400] {
        eprintln!("VFUZZ-BISECT full list at DAA {at} -> {:?}", run(n, at).await);
    }
}

/// **lane GC-DUTY decisive test (Lead 2026-10-28)**: the FULL 17,400 list armed at DAA 0 (F and open-claim estimates in, R unchanged).
/// (i) own attempts are admitted under the budget (counted, per bond, with the anchor each records); (iii) a refused one is a clean skip
/// (the block stands, the chain advances); (ii) a Panel=1 claim is seated from a post-commit seed anchor, goes Final, is certified by
/// the drawn verifiers (Gate C 'K') and paid. Prints the anchor gap and seating → Final in DAA.
#[tokio::test]
async fn gc_duty_full_17400_list_real_attempts_anchor_and_a_panel_one_claim_is_seated_final_certified_and_paid() {
    use super::cert::{attest, cert_row, until_decided, until_drawn};
    kaspa_core::log::try_init_logger("error");
    let n = PALW_T12_G14_17000_FENCES_V1.len();
    let mut w = World::on(Net::over_cfg(prefix_config(n, 0), TestConsensus::new)).await;
    // (ii): a Panel=1 claim seated from a post-commit anchor → Final → Gate C → payout.
    let commit_daa = w.net.daa();
    let (claim, _seats) = claimed(&mut w, Mode::Panel, 0, false).await;
    let seated_daa = w.net.daa();
    let floor = final_floor(&w, Mode::Panel, &claim.id);
    w.net.beat_to(floor).await;
    assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&claim.id));
    let final_daa = w.net.daa();
    eprintln!("GC-DUTY 17400 Panel=1: committed ~DAA {commit_daa}, seated by DAA {seated_daa}, Final at DAA {final_daa} (seat → Final {} DAA)", final_daa - seated_daa);
    let policy = w.policy();
    let row = until_drawn(&mut w.net, &claim.id).await;
    use misaka_palw_kernel::model_eligibility_v1::EligibilityVerdictV1::Reproduced;
    let drawn: Vec<usize> = (0..8usize).filter(|c| row.draws.contains(&w.net.kid(*c))).collect();
    for card in drawn {
        attest(&mut w.net, &claim.id, card, Reproduced).await;
    }
    let (producer_before, _) = until_decided(&mut w.net, &claim.id, TEST_ONLY_KERNEL_CERT_POLICY_V1.prosecution_window_daa + 50).await;
    let decided = cert_row(&w.net, &claim.id).unwrap();
    assert_eq!(
        decided.outcome.map(|(o, _)| o),
        Some(kaspa_consensus_core::palw_security_certification_v1::PalwCertificationOutcomeV1::Certified)
    );
    assert_eq!(w.net.owed(0), producer_before + Rules::final_reward(&policy), "the certified Final pays");
    eprintln!("GC-DUTY 17400 Gate C certified at DAA {}; paid", w.net.daa());

    // (i)+(iii), after the Panel=1 claim is paid (its producer helpers 5..7 need the class to admit them): attempts by cards 1..3, 12 rounds.
    // Before each, the chain's readiness verdict for the card (`palw_producer_facts_v2`) is read: a card the registry does not admit
    // now is recorded with its reason and skipped (the harness's `attempt` panics on a not-ready card), never mistaken for P_m or a cap.
    let ttpb = w.net.ttpb();
    let (mut admitted, mut skipped, mut anchors) = (0u32, 0u32, Vec::new());
    let mut not_ready: Vec<(usize, usize, String)> = Vec::new();
    let mut attempt_claims = Vec::new();
    for round in 0..12usize {
        let card = 1 + round % 3;
        let base = w.net.chain.bundle.base_class_id;
        let facts = w.net.chain.ctx.consensus.palw_producer_facts_v2(base, Some(w.net.chain.bonds[card].0)).expect("facts");
        if let Err(why) = facts.ready_to_produce(&TestConsensus::palw_v2_registry_keypair(card as u64).verification_key.as_ref().to_vec()) {
            not_ready.push((round, card, format!("{why}: {:?}", facts.class_admission_refusal)));
            continue;
        }
        let sink_before = w.net.chain.sink();
        let daa_before = w.net.daa();
        let (_, c) = w.net.chain.attempt(card, ttpb, Vec::new(), &|_| true).await;
        attempt_claims.push(c);
        let s = w.net.chain.tip_state().1;
        // (iii) the attempt block is accepted and becomes the sink whether or not its claim is admitted (a refused attempt is a
        // skip, not an invalid block). The DAA score is not the right witness: a harness attempt block can share its parent's DAA.
        assert_ne!(w.net.chain.sink(), sink_before, "the attempt block is accepted (the chain advances) whether or not its claim is admitted");
        eprintln!("GC-DUTY 17400 attempt round {round}: sink DAA {daa_before} -> {}", w.net.daa());
        if s.claim(&c).is_some() { admitted += 1 } else { skipped += 1 }
        if let Some(a) = s.round_seed_anchor().copied() {
            if anchors.last().is_none_or(|(_, b)| *b != a.block) {
                anchors.push((w.net.daa(), a.block));
            }
        }
    }
    let gaps: Vec<u64> = anchors.windows(2).map(|p| p[1].0 - p[0].0).collect();
    eprintln!("GC-DUTY 17400 attempts: admitted {admitted}, skipped {skipped}; anchors at {:?} (gaps {gaps:?})", anchors.iter().map(|a| a.0).collect::<Vec<_>>());
    eprintln!("GC-DUTY 17400 not ready (registry): {not_ready:?}");
    assert!(
        not_ready.iter().all(|(_, _, why)| why.starts_with(kaspa_consensus_core::palw_producer_v2::PALW_NOT_READY_CLASS_NOT_ADMITTING_V2)),
        "a card is held only by the registry's admission (not P_m, not a Q/B/R/F cap): {not_ready:?}"
    );
    assert!(admitted > 0, "REAL attempts are admitted under the full list");
    assert!(!anchors.is_empty(), "admitted attempts record seed anchors");

    // ---- across the first roll where ΣA becomes non-zero (the bootstrap ends) ----
    // Card 1 assigns capital to the model its attempt recorded (tag 140); after the seasoning the roll gives that model S̄ > 0, so the
    // allocation is no longer empty and P_m applies. An attempt of the funded model is then still admitted and anchors.
    let state = w.net.chain.tip_state().1;
    let budget = state.bond_budget().expect("the budget engine");
    assert!(budget.allocation_bootstrapping_v1(), "until capital is assigned the allocation is empty (bootstrap)");
    let model = attempt_claims
        .iter()
        .find_map(|c| budget.claim_row(c).and_then(|row| row.model))
        .map(|(m, _)| m)
        .expect("an admitted attempt recorded its model (it seeds S_m)");
    let bond = w.net.chain.bonds[1];
    let amount = state.bond(&bond).expect("card 1's bond").collateral / 2;
    let assignments = vec![(model, amount)];
    let message = kaspa_consensus_core::palw_bond_budget_v1::palw_capital_assignment_message_v1(w.net.domain, &bond, &assignments, 1);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &TestConsensus::palw_v2_registry_keypair(1).signing_key,
        message.as_byte_slice(),
        kaspa_consensus_core::palw_bond_budget_v1::PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1,
        [0x21u8; 32],
    )
    .expect("sign")
    .as_ref()
    .to_vec();
    w.net.send(vec![(1, Obj::BondCapitalAssignedV1 { bond, assignments, sequence: 1, signature })]).await;
    let epoch = kaspa_consensus_core::palw_bond_budget_v1::palw_model_bond_allocation_estimate_v1().epoch_daa;
    let target = (w.net.daa() / epoch + 3) * epoch + 1;
    w.net.beat_to(target).await;
    let state = w.net.chain.tip_state().1;
    let budget = state.bond_budget().expect("the budget engine");
    assert!(!budget.allocation_bootstrapping_v1(), "past the seasoned roll ΣA > 0: the bootstrap has ended (DAA {})", w.net.daa());
    assert!(budget.model_blocks_available_v1(&model, w.net.daa()) > 0, "the funded model has block opportunity");
    let anchor_before = state.round_seed_anchor().copied();
    let (_, c) = w.net.chain.attempt(2, ttpb, Vec::new(), &|_| true).await;
    let state = w.net.chain.tip_state().1;
    assert!(state.claim(&c).is_some(), "after the bootstrap an attempt of the funded model is still admitted");
    assert_ne!(state.round_seed_anchor().copied(), anchor_before, "and it records a seed anchor");
    eprintln!("GC-DUTY 17400 post-bootstrap (DAA {}): attempt admitted under P_m, anchor recorded", w.net.daa());
}

/// **lane GC-DUTY option (C)**: every prefix of the 17,400 list (0..=n, armed at DAA 0) lets the honest Panel=1 claim commit.
#[tokio::test]
async fn gc_duty_every_prefix_of_the_17400_list_commits_the_honest_panel_one_claim() {
    kaspa_core::log::try_init_logger("error");
    let n = PALW_T12_G14_17000_FENCES_V1.len();
    let mut failed = Vec::new();
    for k in 0..=n {
        let r = AssertUnwindSafe(probe_claim(prefix_config(k, 0))).catch_unwind().await.map_err(panic_text);
        eprintln!("GC-DUTY prefix={k}/{n} -> {r:?}");
        if !matches!(r, Ok(Ok(()))) {
            failed.push((k, format!("{r:?}")));
        }
    }
    assert!(failed.is_empty(), "prefixes that do not commit: {failed:?}");
}
