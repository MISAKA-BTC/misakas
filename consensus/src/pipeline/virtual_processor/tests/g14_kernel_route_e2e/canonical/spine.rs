//! **lane SPINE — the G14 vertical slice's backbone** (2026-10-26; goal: G14 armed for ONE slice at testnet-12 DAA 17,000).
//!
//! Slice: the Panel-licensed kernel route (`Mode::Panel`, every seat colluding), the class card 1 registers in `world` on the
//! canonical testnet-12 harness, the single-prompt decode job of `claimed`. Each stage drives the real virtual processor, fold,
//! mempool/template and a cold node (IBD or pruned import) through the existing `canonical` driver; a stage no lane has built yet
//! is reported UNIMPLEMENTED with its owning lane and the missing function. The table test FAILS while any stage is not PASS:
//! nothing here is skipped, weakened or counted as done. No cryptographic proof of computation is used (PRINCIPLES §8).

use super::*;
use futures_util::FutureExt;

enum Stage {
    Run(&'static str, Mode, Start, Outcome),
    Reorg(&'static str, Outcome),
    Hold(&'static str),
    /// The pinned production premise in effect on the node build (F02, user-approved 4-DAA estimate 2026-10-26).
    Pinned(&'static str),
    Missing { stage: &'static str, lane: &'static str, needs: &'static str },
    /// A native run (lane LEGACY 2026-10-27): the stage's own driver on the real processor.
    Native(&'static str, NativeCase),
    /// lane CERT: a stage driven by `canonical::cert` (Gate C armed with a TEST-ONLY certification policy).
    Cert(&'static str, CertCase),
}

#[derive(Clone, Copy)]
enum NativeCase {
    AdmissionAndSupply,
    T05BeforeFinal,
    T05AfterFinal,
    PremiseTypedMissingBound,
    PremiseOpportunityDefaults,
}

#[derive(Clone, Copy)]
enum CertCase {
    CertifiedReorgIbd,
    Shortfall,
}

const STAGES: &[Stage] = &[
    Stage::Native(
        "S1+S2 original admission reserves court capital + service book atomically in the accepting block; duties derived from the \
         original; every accepting block records its supply (TEST-ONLY admission schedule; production None)",
        NativeCase::AdmissionAndSupply,
    ),
    Stage::Run(
        "S3a honest: wrong challenge dismissed, Final on its own clock, paid once (IBD node)",
        Mode::Panel,
        Start::Ibd,
        Outcome::WrongChallengeDismissed,
    ),
    Stage::Cert(
        "S3a' per-claim lottery approval: Final alone pays nothing; k drawn approvals + prosecution window -> paid once; a reorg \
         past the approvals takes the payment back; IBD agrees (TEST-ONLY cert policy)",
        CertCase::CertifiedReorgIbd,
    ),
    Stage::Cert(
        "S3a' approval shortfall / Unavailable verifiers: NotCertified, producer never paid, reward less the V3 burn share returned to the poster \
         (TEST-ONLY cert policy)",
        CertCase::Shortfall,
    ),
    Stage::Run(
        "S3b lie, all seats colluding: outsider convicts before Final, producer slashed (pruned-import node)",
        Mode::Panel,
        Start::PrunedImport,
        Outcome::ConvictedBeforeFinal,
    ),
    Stage::Run("S3b lie after Final: outsider convicts, liability stands (IBD node)", Mode::Panel, Start::Ibd, Outcome::ConvictedAfterFinal),
    Stage::Native(
        "S3b' T05 on the native kernel route, Panel=1: Sybils occupy and refill the court with paid junk filings twice; a fresh outsider \
         convicts before Final (f03k::t05)",
        NativeCase::T05BeforeFinal,
    ),
    Stage::Native("S3b' T05 after Final on the native kernel route, Panel=1 (f03k::t05)", NativeCase::T05AfterFinal),
    Stage::Run(
        "S3c silence: withheld material -> demand -> deadline default (kernel ledger; F02 premise NOT consulted) (IBD node)",
        Mode::Panel,
        Start::Ibd,
        Outcome::DefaultOnWithheldMaterial,
    ),
    Stage::Missing {
        stage: "S3c' no supply => no slash: an undelivered duty holds evidence/bond without a default",
        lane: "F02 (P0-1)",
        needs: "the fold holds a MissingSupply binding (kernel_court_default_held_v1/v2; fold test f02b_held_set_*), but no native \
                block produces MissingSupply on the kernel route: supply is withheld only when unreserved court ops spend reserved \
                capacity, and kernel-route objects are not court-accounted (palw_court_accounted_op_v1)",
    },
    Stage::Hold("S3c'' F02 premise None (TEST-ONLY typed-missing override): the silent producer's demand ends as a no-slash hold"),
    Stage::Pinned("S3c'' F02 pinned premise (production, 4 DAA): the silent producer's supplied demand is attributed and decided"),
    Stage::Native(
        "S3c''' native, bound claim, premise typed-missing (TEST-ONLY override): the silent producer is held, no default",
        NativeCase::PremiseTypedMissingBound,
    ),
    Stage::Native(
        "S3c''' native, bound claim, pinned premise: a supplied unit fits the deadline -> the silent producer defaults",
        NativeCase::PremiseOpportunityDefaults,
    ),
    Stage::Reorg("S4 honest: Final and its payment decided once across a reorg of the deciding block", Outcome::WrongChallengeDismissed),
    Stage::Reorg("S4 silence: the default decided once across a reorg of the deadline block", Outcome::DefaultOnWithheldMaterial),
];

/// **Node `Z` on a competing branch that re-carries everything.** `Z` replays `main`. A second chain then forks at `fork` (before the
/// claim) and re-mines each of `main`'s selected-chain blocks above `fork` as a new block. Each new block carries the same
/// non-coinbase transactions: the claim, the Panel receipts and the demand. Heartbeats are added until its blue work exceeds A's.
/// Under the strict economic win rule the branch therefore carries the same economic content plus more work. The gate is not
/// loosened.
async fn recarried_node(main: &Net, fork: kaspa_hashes::Hash64) -> Net {
    let z = main.replay().await;
    let mut zn = main.on_chain(z);
    zn.chain.ctx.simulated_time = main.chain.ctx.simulated_time;
    let mut b = t12_genesis_chain(&main.config, &main.bundle, &main.premine, &main.floats);
    let up = chain_blocks(&main.chain, fork);
    b.ctx.simulated_time = up.last().unwrap().header.timestamp;
    for blk in up {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let ttpb = main.ttpb();
    let a_blocks: Vec<Block> = chain_blocks(&main.chain, main.chain.sink()).into_iter().skip_while(|blk| blk.header.hash != fork).skip(1).collect();
    assert!(!a_blocks.is_empty(), "A has blocks above the fork");
    let mut b_blocks = Vec::new();
    for blk in &a_blocks {
        let txs: Vec<Transaction> = blk.transactions.iter().skip(1).cloned().collect();
        b_blocks.push(b.heartbeat(ttpb, txs).await);
    }
    let a_work = main.chain.ctx.consensus.get_block(main.chain.sink()).unwrap().header.blue_work;
    while b.ctx.consensus.get_block(b.sink()).unwrap().header.blue_work <= a_work {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    for blk in &b_blocks {
        arrive(&zn.chain, blk.clone(), "B's re-carrying block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "the re-carrying branch B wins under the strict economic rule: Z reorgs onto B");
    for blk in &a_blocks {
        assert!(!spine_of(&zn.chain).contains(&blk.header.hash), "A's block {} is off Z's selected chain", blk.header.hash);
    }
    zn.chain.ctx.simulated_time = zn.chain.ctx.simulated_time.max(b.ctx.simulated_time);
    zn
}

/// The sompi the selected chain's coinbases paid to card `card`'s registered payout script.
fn delivered(net: &Net, card: usize) -> u64 {
    let payload = net.chain.tip_state().1.bond(&net.bond(card)).expect("the bond").payout_payload;
    let spk = p2pkh_mldsa87_spk(payload.as_byte_slice());
    chain_blocks(&net.chain, net.chain.sink())
        .iter()
        .flat_map(|b| b.transactions[0].outputs.iter())
        .filter(|o| o.script_public_key == spk)
        .map(|o| o.value)
        .sum()
}

fn spine_of(chain: &T12Chain) -> Vec<kaspa_hashes::Hash64> {
    chain_blocks(chain, chain.sink()).into_iter().map(|b| b.header.hash).collect()
}

/// **S4: a reorg across the deciding block.** `fork` is a block before the deciding DAA (the honest claim's Final floor, or the
/// silent producer's demand deadline). The producer's node decides at that DAA. Node `Z` replays it and then reorgs onto a heavier
/// branch from `fork` that carries nothing (`recovery::reorged_node`). On `Z` the claim must be decided again, with the same state,
/// collateral, reservation and amount owed, and a node that syncs `Z`'s whole DAG must hold the same root. Nothing is cached between
/// the two decisions.
async fn decided_across_a_reorg(outcome: Outcome) {
    use super::super::recovery::dag_ibd_node;
    kaspa_core::log::try_init_logger("warn");
    // The inclusion-premise override is process-wide: run on the production premise, serialized against the TEST-ONLY override.
    let _production = crate::pipeline::virtual_processor::processor::palw_accept::test_only_inclusion_premise::production_guard_v1();
    let mode = Mode::Panel;
    let mut w = world(mode).await;
    let funding = newcomer_funding(&w.net);
    let (claim, _seats) = claimed(&mut w, mode, 0, outcome != Outcome::WrongChallengeDismissed).await;
    let mut newcomer = None;
    let deciding = match outcome {
        Outcome::WrongChallengeDismissed => final_floor(&w, mode, &claim.id),
        Outcome::DefaultOnWithheldMaterial => {
            let mut node = start_node(&mut w.net, Start::Ibd).await;
            let bond = register_newcomer(&mut node, funding).await;
            sync(&node, &mut w.net).await;
            know_newcomer(&mut w.net, bond);
            newcomer = Some(bond);
            Rules::file_demand(&mut node, NEWCOMER, claim.id, claim.at.0).await;
            sync(&node, &mut w.net).await;
            let read = Rpc::of(&w.net.chain).claim(&claim.id);
            assert_eq!(read.demands.len(), 1, "the demand is open on the producer's node");
            read.demands[0].deadline_daa
        }
        other => panic!("S4 covers the honest and the silent branch here, not {other:?}"),
    };
    w.net.beat_to(deciding).await;
    let decided = format!("{:?}", w.net.claim_state(&claim.id));
    match outcome {
        Outcome::WrongChallengeDismissed => assert!(decided.starts_with("Final"), "A decides Final: {decided}"),
        _ => assert!(decided.starts_with("Unavailable"), "A decides the default: {decided}"),
    }
    // The newcomer has a bond only on the silent branch (it filed the demand).
    let demander = outcome == Outcome::DefaultOnWithheldMaterial;
    let (collateral, reserved, owed, owed_newcomer) =
        (w.net.collateral(0), w.net.kernel_reserved(0), w.net.owed(0), if demander { w.net.owed(NEWCOMER) } else { 0 });

    // The fork is the last selected-chain block below the deciding DAA. Under the strict economic rule, an all-economic tie reorgs
    // only inside PALW_REORG_SHALLOW_TIE_DAA_V1 (2 DAA) of the incumbent sink, and a fork below the claim is refused by design
    // (measured: DominanceViolation). So B crosses the deciding block, re-carrying whatever A carried above the fork.
    let fork = chain_blocks(&w.net.chain, w.net.chain.sink())
        .into_iter()
        .filter(|blk| blk.header.daa_score < deciding)
        .last()
        .expect("a block below the deciding DAA")
        .header
        .hash;
    let mut z = recarried_node(&w.net, fork).await;
    if let Some(bond) = newcomer {
        know_newcomer(&mut z, bond);
    }
    let to = z.daa().max(deciding) + 1;
    z.beat_to(to).await;
    let again = format!("{:?}", z.claim_state(&claim.id));
    assert_eq!(again.split(['{', ' ']).next(), decided.split(['{', ' ']).next(), "Z decides the same branch after the reorg");
    assert_eq!(z.collateral(0), collateral, "the producer's collateral is the same on Z");
    assert_eq!(z.kernel_reserved(0), reserved, "and its reservation");
    // Z's chain runs past A's tip, so the queue may already have drained into coinbases. The invariant is queued + delivered,
    // counted on each node's own selected chain: paid once, never twice and never lost.
    assert_eq!(z.owed(0) + delivered(&z, 0), owed + delivered(&w.net, 0), "what the producer is owed + was paid (once)");
    if demander {
        assert_eq!(z.owed(NEWCOMER) + delivered(&z, NEWCOMER), owed_newcomer + delivered(&w.net, NEWCOMER), "and the demander's");
    }
    let d = dag_ibd_node(&z).await;
    z.assert_same(&d.chain, "a node syncing Z's DAG after the reorg");
}

/// **S3c'': with no adopted inclusion premise** — since the 2026-10-26 pin a node build always has one, so this case is reached only
/// through the TEST-ONLY typed-missing override (`set_typed_missing_v1`). F02 attributes no default. The silent producer's demanded position passes its deadline, and the claim must then be held: not Final, not
/// convicted, not Unavailable. No penalty is taken, and both the producer's reservation and the demander's bond stay held.
async fn silent_without_premise_holds() {
    kaspa_core::log::try_init_logger("warn");
    let _missing = crate::pipeline::virtual_processor::processor::palw_accept::test_only_inclusion_premise::set_typed_missing_v1(
        "TEST_ONLY_TYPED_MISSING_PREMISE",
    );
    assert_eq!(crate::pipeline::virtual_processor::processor::palw_accept::test_only_inclusion_premise::effective_v1(), None);
    let mode = Mode::Panel;
    let mut w = world(mode).await;
    let funding = newcomer_funding(&w.net);
    let before = w.net.collateral(0) - Rules::admission_cost(&w, mode);
    let (claim, _seats) = claimed(&mut w, mode, 0, true).await;
    let mut node = start_node(&mut w.net, Start::Ibd).await;
    let bond = register_newcomer(&mut node, funding).await;
    sync(&node, &mut w.net).await;
    know_newcomer(&mut w.net, bond);
    Rules::file_demand(&mut node, NEWCOMER, claim.id, claim.at.0).await;
    sync(&node, &mut w.net).await;
    let read = Rpc::of(&w.net.chain).claim(&claim.id);
    assert_eq!(read.demands.len(), 1, "the demand is open");
    let held = w.net.kernel_reserved(0);
    w.net.beat_to(read.demands[0].deadline_daa + 2).await;
    let st = w.net.claim_state(&claim.id);
    assert!(!matches!(st, ClaimStateV1::Unavailable { .. } | ClaimStateV1::Final { .. }), "no premise: held, not decided: {st:?}");
    assert!(!w.net.ledger().claims[&claim.id].convicted, "and not convicted");
    assert_eq!(w.net.collateral(0), before, "no default penalty without a premise");
    assert_eq!(w.net.kernel_reserved(0), held, "the reservation stays held");
    assert!(w.net.kernel_reserved(NEWCOMER) > 0, "the demand bond stays held");
}

#[tokio::test]
async fn g14_spine_s3c_without_inclusion_premise_is_a_no_slash_hold() {
    silent_without_premise_holds().await;
}

/// **S3c'' (node build): the pinned premise is in effect** (no override). The same silent producer, demanded by an outsider, is
/// decided: the claim ends `Unavailable` with the producer defaulted and the default penalty taken — never held forever.
async fn silent_under_pinned_premise_is_decided() {
    kaspa_core::log::try_init_logger("warn");
    let _production = crate::pipeline::virtual_processor::processor::palw_accept::test_only_inclusion_premise::production_guard_v1();
    let pinned = kaspa_consensus_core::palw_court_service_binding_v1::PALW_COURT_SERVICE_INCLUSION_PREMISE_V1;
    assert!(pinned.is_some_and(|p| p.inclusion_delay_daa == 4), "the approved 4-DAA premise is pinned");
    assert_eq!(crate::pipeline::virtual_processor::processor::palw_accept::test_only_inclusion_premise::effective_v1(), pinned);
    let mode = Mode::Panel;
    let mut w = world(mode).await;
    let funding = newcomer_funding(&w.net);
    let before = w.net.collateral(0) - Rules::admission_cost(&w, mode);
    let (claim, _seats) = claimed(&mut w, mode, 0, true).await;
    let mut node = start_node(&mut w.net, Start::Ibd).await;
    let bond = register_newcomer(&mut node, funding).await;
    sync(&node, &mut w.net).await;
    know_newcomer(&mut w.net, bond);
    Rules::file_demand(&mut node, NEWCOMER, claim.id, claim.at.0).await;
    sync(&node, &mut w.net).await;
    let read = Rpc::of(&w.net.chain).claim(&claim.id);
    assert_eq!(read.demands.len(), 1, "the demand is open");
    w.net.beat_to(read.demands[0].deadline_daa + 2).await;
    let st = w.net.claim_state(&claim.id);
    assert!(matches!(st, ClaimStateV1::Unavailable { producer_defaulted: true, .. }), "pinned premise: decided default: {st:?}");
    assert!(w.net.collateral(0) < before, "the default penalty is taken");
}

#[tokio::test]
async fn g14_spine_s3c_pinned_premise_decides_the_silent_default() {
    silent_under_pinned_premise_is_decided().await;
}

#[tokio::test]
async fn g14_spine_s4_honest_final_is_decided_once_across_a_reorg() {
    decided_across_a_reorg(Outcome::WrongChallengeDismissed).await;
}

#[tokio::test]
async fn g14_spine_s4_default_is_decided_once_across_a_reorg() {
    decided_across_a_reorg(Outcome::DefaultOnWithheldMaterial).await;
}

#[tokio::test]
async fn g14_spine_panel1_kernel_route_stage_table() {
    let mut rows = Vec::new();
    for s in STAGES {
        match s {
            Stage::Run(name, mode, start, outcome) => {
                let r = std::panic::AssertUnwindSafe(canonical(*mode, *start, *outcome)).catch_unwind().await;
                rows.push((*name, if r.is_ok() { "PASS".to_string() } else { "FAIL".to_string() }));
            }
            Stage::Cert(name, case) => {
                let r = match case {
                    CertCase::CertifiedReorgIbd => {
                        std::panic::AssertUnwindSafe(super::cert::s3a_prime_certified_reorg_ibd()).catch_unwind().await
                    }
                    CertCase::Shortfall => std::panic::AssertUnwindSafe(super::cert::s3a_prime_shortfall()).catch_unwind().await,
                };
                rows.push((*name, if r.is_ok() { "PASS".to_string() } else { "FAIL".to_string() }));
            }
            Stage::Reorg(name, outcome) => {
                let r = std::panic::AssertUnwindSafe(decided_across_a_reorg(*outcome)).catch_unwind().await;
                rows.push((*name, if r.is_ok() { "PASS".to_string() } else { "FAIL".to_string() }));
            }
            Stage::Hold(name) => {
                let r = std::panic::AssertUnwindSafe(silent_without_premise_holds()).catch_unwind().await;
                rows.push((*name, if r.is_ok() { "PASS".to_string() } else { "FAIL".to_string() }));
            }
            Stage::Pinned(name) => {
                let r = std::panic::AssertUnwindSafe(silent_under_pinned_premise_is_decided()).catch_unwind().await;
                rows.push((*name, if r.is_ok() { "PASS".to_string() } else { "FAIL".to_string() }));
            }
            Stage::Native(name, case) => {
                let r = match case {
                    NativeCase::AdmissionAndSupply => std::panic::AssertUnwindSafe(s1_s2_admission_and_supply()).catch_unwind().await,
                    NativeCase::T05BeforeFinal => std::panic::AssertUnwindSafe(super::f03k::t05(false, true)).catch_unwind().await,
                    NativeCase::T05AfterFinal => std::panic::AssertUnwindSafe(super::f03k::t05(true, true)).catch_unwind().await,
                    NativeCase::PremiseTypedMissingBound => {
                        std::panic::AssertUnwindSafe(s3c_triple_prime_native(true)).catch_unwind().await
                    }
                    NativeCase::PremiseOpportunityDefaults => {
                        std::panic::AssertUnwindSafe(s3c_triple_prime_native(false)).catch_unwind().await
                    }
                };
                rows.push((*name, if r.is_ok() { "PASS".to_string() } else { "FAIL".to_string() }));
            }
            Stage::Missing { stage, lane, needs } => rows.push((*stage, format!("UNIMPLEMENTED [{lane}] {needs}"))),
        }
    }
    for (name, status) in &rows {
        eprintln!("[spine] {status} | {name}");
    }
    let open = rows.iter().filter(|(_, s)| s != "PASS").count();
    assert_eq!(open, 0, "the Panel=1 kernel slice is not closed: {open} stage(s) open");
}

// ---- lane LEGACY 2026-10-27: S1 / S2 / S3c''' on the native processor with a TEST-ONLY kernel court admission schedule ---------

/// The Panel=1 world with the F02b kernel court admission schedule in force from genesis — **TEST ONLY**
/// (`KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1`, lane VAL's estimates; production stays `None`).
async fn admitted_world() -> World {
    let (config, mut bundle, premine, floats) =
        super::f03k::panel_one_config(Some(0), Some(super::f03k::TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI));
    let mut params = config.params.clone();
    bundle.state = bundle.state.clone().with_test_only_kernel_court_admission_v1(Some(
        kaspa_consensus_core::palw_kernel_court_admission_v1::KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1(),
    ));
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &mut params.palw_consensus_mode {
        *b = bundle.clone();
    }
    World::on(Net::over_cfg((Config::new(params), bundle, premine, floats), TestConsensus::new)).await
}

/// **S1 + S2 (native):** the original's admission reserves the court capital and the service book in the claim's own accepting
/// block (one journalled block: the capital row, the binding and the claim commit together); every duty packet is derived from the
/// original under the schedule's profile; each accepting block afterwards records its actual supply, so every packet is assessed
/// `OpportunityProvided` once its interval has passed.
async fn s1_s2_admission_and_supply() {
    use kaspa_consensus_core::palw_court_service_v1::ServiceAssessment;
    kaspa_core::log::try_init_logger("warn");
    let _production = crate::pipeline::virtual_processor::processor::palw_accept::test_only_inclusion_premise::production_guard_v1();
    let mode = Mode::Panel;
    let mut w = admitted_world().await;
    let free_before = w.net.collateral(0);
    let (claim, seats) = claimed(&mut w, mode, 0, false).await;
    assert_eq!(seats.len(), 1, "Panel=1");
    let state = w.net.chain.tip_state().1;
    let id = Hash64::from_bytes(claim.id);
    let service = state.court_service_state_v1().expect("S1: the admission installed the court service book");
    let binding = service.claims.get(&id).expect("S1: the claim is bound at admission");
    let rows: Vec<_> = state.kernel_court_capital_rows_v1().into_iter().filter(|r| r.claim == id).collect();
    assert_eq!(rows.len(), 1, "S1: one capital lock for the claim");
    assert_eq!(rows[0].payer, w.net.bond(0), "S1: the producer pays the court capital");
    assert!(rows[0].amount > 0 && rows[0].amount as u64 <= free_before, "S1: a real reservation within free collateral");
    assert_eq!(rows[0].hard_deadline_daa, binding.hard_deadline_daa, "S1: capital and book share the hard deadline");
    let committed = w.net.ledger().claims[&claim.id].committed_daa;
    assert_eq!(binding.original_accepted_daa, committed, "S1: reserved in the claim's own accepting block");
    // S2: duties derived from the original (profile packets), each assigned a reserved unit inside its interval.
    assert!(!binding.packet_ids.is_empty(), "S2: duty packets derived from the original");
    let mut last = 0u64;
    for p in &binding.packet_ids {
        let packet = service.book.packet(p).expect("S2: the packet is reserved in the book");
        let unit = service.book.assigned_daa(p).expect("S2: the packet has a reserved unit");
        assert!(packet.release_daa <= unit && unit < packet.service_before, "S2: the unit lies in the packet's interval");
        assert!(unit > committed, "S2: units follow the original's acceptance");
        last = last.max(packet.service_before);
    }
    w.net.beat_to(last + 1).await;
    let state = w.net.chain.tip_state().1;
    let service = state.court_service_state_v1().expect("the book");
    if let Some(binding) = service.claims.get(&id) {
        for p in &binding.packet_ids {
            let unit = service.book.assigned_daa(p).unwrap();
            assert!(service.book.supply_at(unit).is_some(), "S2: the accepting block at {unit} recorded its supply");
            assert_eq!(service.book.assess(p), ServiceAssessment::OpportunityProvided, "S2: supplied");
        }
    }
}

#[tokio::test]
async fn g14_spine_s1_s2_admission_reserves_and_blocks_supply() {
    s1_s2_admission_and_supply().await;
}

/// **S3c''' (native):** on a claim BOUND at admission, the kernel ledger's default consults the premise rule. Typed-missing
/// (TEST-ONLY override): the silent producer is held, not defaulted. Pinned premise with supplied units before the deadline: the
/// held set excludes the claim (an opportunity existed) and the silent producer defaults.
async fn s3c_triple_prime_native(typed_missing: bool) {
    use crate::pipeline::virtual_processor::processor::palw_accept::test_only_inclusion_premise as premise;
    kaspa_core::log::try_init_logger("warn");
    let _g = if typed_missing { premise::set_typed_missing_v1("TEST_ONLY_TYPED_MISSING_PREMISE") } else { premise::production_guard_v1() };
    let mode = Mode::Panel;
    let mut w = admitted_world().await;
    let funding = newcomer_funding(&w.net);
    let (claim, _seats) = claimed(&mut w, mode, 0, true).await;
    let id = Hash64::from_bytes(claim.id);
    assert!(w.net.chain.tip_state().1.court_service_state_v1().is_some_and(|s| s.claims.contains_key(&id)), "bound at admission");
    let mut node = start_node(&mut w.net, Start::Ibd).await;
    let bond = register_newcomer(&mut node, funding).await;
    sync(&node, &mut w.net).await;
    know_newcomer(&mut w.net, bond);
    Rules::file_demand(&mut node, NEWCOMER, claim.id, claim.at.0).await;
    sync(&node, &mut w.net).await;
    let read = Rpc::of(&w.net.chain).claim(&claim.id);
    assert_eq!(read.demands.len(), 1, "the demand is open");
    let deadline = read.demands[0].deadline_daa;
    let held = w.net.chain.tip_state().1.kernel_court_default_held_v2(premise::effective_v1().as_ref(), &w.net.ledger());
    assert_eq!(held.contains_key(&claim.id), typed_missing, "the premise rule on the native state");
    w.net.beat_to(deadline + 2).await;
    let st = w.net.claim_state(&claim.id);
    if typed_missing {
        assert!(!matches!(st, ClaimStateV1::Unavailable { .. } | ClaimStateV1::Final { .. }), "typed-missing: held: {st:?}");
    } else {
        assert!(matches!(st, ClaimStateV1::Unavailable { producer_defaulted: true, .. }), "opportunity + silence: default: {st:?}");
    }
}

#[tokio::test]
async fn g14_spine_s3c_triple_prime_native_typed_missing_holds_a_bound_claim() {
    s3c_triple_prime_native(true).await;
}

#[tokio::test]
async fn g14_spine_s3c_triple_prime_native_opportunity_then_silence_defaults() {
    s3c_triple_prime_native(false).await;
}
