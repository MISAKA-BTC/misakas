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
    /// lane CERT: a stage driven by `canonical::cert` (Gate C armed with a TEST-ONLY certification policy).
    Cert(&'static str, CertCase),
}

#[derive(Clone, Copy)]
enum CertCase {
    CertifiedReorgIbd,
    Shortfall,
}

const STAGES: &[Stage] = &[
    Stage::Missing {
        stage: "S1 original admission: funds + resources reserved atomically from the original",
        lane: "F02 (P0-1)",
        needs: "a native kernel-route admission reserving every derived duty's capital and fees in the admitting tx \
                (palw_funded_original_coordinator_v1 wired into the kernel route's admission fold)",
    },
    Stage::Missing {
        stage: "S2 duty derivation from the original + actual supply in an accepting block",
        lane: "F02 (P0-1)",
        needs: "kernel-route duties/packets derived from the original via palw_court_service_v1, accepting-block supply recorded",
    },
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
    Stage::Missing {
        stage: "S3b' conviction through the PUBLIC PAID ingress with every court owner occupied/refilled by colluders (T05)",
        lane: "F03k (claude/g14-f03k)",
        needs: "F03b (1298841fa) is on the TIR public route only, in two separate core-fold fixtures (frame fixture (7,1) for \
                capture/refill; history fixture for root->bottom->Voided{CourtHeldVerdict}), with a 3-seat Panel; the kernel route \
                has no shared frontier and no paid-ingress file_proof. Needs one fixture/world serving both legs at Panel=1 and \
                a kernel-route (or slice-switch to TIR public) entry",
    },
    Stage::Run(
        "S3c silence: withheld material -> demand -> deadline default (kernel ledger; F02 premise NOT consulted) (IBD node)",
        Mode::Panel,
        Start::Ibd,
        Outcome::DefaultOnWithheldMaterial,
    ),
    Stage::Missing {
        stage: "S3c' no supply => no slash: an undelivered duty holds evidence/bond without a default",
        lane: "F02 (P0-1)",
        needs: "a supply-absent branch in the kernel route's default fold (the default is deadline-only today)",
    },
    Stage::Hold("S3c'' F02 premise None (TEST-ONLY typed-missing override): the silent producer's demand ends as a no-slash hold"),
    Stage::Pinned("S3c'' F02 pinned premise (production, 4 DAA): the silent producer's supplied demand is attributed and decided"),
    Stage::Missing {
        stage: "S3c''' default under an explicitly test-only inclusion premise on the native processor",
        lane: "F02 (P0-1) / central",
        needs: "the TEST_ONLY_ premise setter and typed-missing override exist (palw_accept test_only_inclusion_premise); the \
                kernel ledger's default must consult the premise (base: it defaults deadline-only, even with no premise)",
    },
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
            Stage::Missing { stage, lane, needs } => rows.push((*stage, format!("UNIMPLEMENTED [{lane}] {needs}"))),
        }
    }
    for (name, status) in &rows {
        eprintln!("[spine] {status} | {name}");
    }
    let open = rows.iter().filter(|(_, s)| s != "PASS").count();
    assert_eq!(open, 0, "the Panel=1 kernel slice is not closed: {open} stage(s) open");
}
