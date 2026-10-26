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
        "S3a' approval shortfall / Unavailable verifiers: NotCertified, producer never paid, reward returned to the poster \
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
        lane: "F03 (P0-2)",
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
    Stage::Hold("S3c'' F02 premise None (production, typed-missing): the silent producer's demand ends as a no-slash hold"),
    Stage::Missing {
        stage: "S3c''' default under an explicitly test-only inclusion premise on the native processor",
        lane: "F02 (P0-1) / central",
        needs: "palw_accept.rs copies the const PALW_COURT_SERVICE_INCLUSION_PREMISE_V1 (None) with no test override; a \
                #[cfg(test)] setter taking a TEST_ONLY_ premise, and the kernel ledger's default consulting the premise",
    },
    Stage::Reorg("S4 honest: Final and its payment decided once across a reorg of the deciding block", Outcome::WrongChallengeDismissed),
    Stage::Reorg("S4 silence: the default decided once across a reorg of the deadline block", Outcome::DefaultOnWithheldMaterial),
];

/// **S4: a reorg across the deciding block.** `fork` is a block before the deciding DAA (the honest claim's Final floor, or the
/// silent producer's demand deadline). The producer's node decides at that DAA. Node `Z` replays it and then reorgs onto a heavier
/// branch from `fork` that carries nothing (`recovery::reorged_node`). On `Z` the claim must be decided again, with the same state,
/// collateral, reservation and amount owed, and a node that syncs `Z`'s whole DAG must hold the same root. Nothing is cached between
/// the two decisions.
async fn decided_across_a_reorg(outcome: Outcome) {
    use super::super::recovery::{dag_ibd_node, reorged_node};
    kaspa_core::log::try_init_logger("warn");
    let mode = Mode::Panel;
    let mut w = world(mode).await;
    let funding = newcomer_funding(&w.net);
    let (claim, _seats) = claimed(&mut w, mode, 0, outcome != Outcome::WrongChallengeDismissed).await;
    let deciding = match outcome {
        Outcome::WrongChallengeDismissed => final_floor(&w, mode, &claim.id),
        Outcome::DefaultOnWithheldMaterial => {
            let mut node = start_node(&mut w.net, Start::Ibd).await;
            let bond = register_newcomer(&mut node, funding).await;
            sync(&node, &mut w.net).await;
            know_newcomer(&mut w.net, bond);
            Rules::file_demand(&mut node, NEWCOMER, claim.id, claim.at.0).await;
            sync(&node, &mut w.net).await;
            let read = Rpc::of(&w.net.chain).claim(&claim.id);
            assert_eq!(read.demands.len(), 1, "the demand is open on the producer's node");
            read.demands[0].deadline_daa
        }
        other => panic!("S4 covers the honest and the silent branch here, not {other:?}"),
    };
    let fork = w.net.chain.sink();
    let funding_at_fork = w.net.funding.clone();
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

    let mut z = reorged_node(&w.net, fork, funding_at_fork).await;
    let to = z.daa().max(deciding) + 1;
    z.beat_to(to).await;
    let again = format!("{:?}", z.claim_state(&claim.id));
    assert_eq!(again.split(['{', ' ']).next(), decided.split(['{', ' ']).next(), "Z decides the same branch after the reorg");
    assert_eq!(z.collateral(0), collateral, "the producer's collateral is the same on Z");
    assert_eq!(z.kernel_reserved(0), reserved, "and its reservation");
    assert_eq!(z.owed(0), owed, "and what it is owed (paid once, not twice)");
    if demander {
        assert_eq!(z.owed(NEWCOMER), owed_newcomer, "and the demander's share");
    }
    let d = dag_ibd_node(&z).await;
    z.assert_same(&d.chain, "a node syncing Z's DAG after the reorg");
}

/// **S3c'': with no adopted inclusion premise** (production: `PALW_COURT_SERVICE_INCLUSION_PREMISE_V1 = None`), F02 attributes no
/// default. The silent producer's demanded position passes its deadline, and the claim must then be held: not Final, not
/// convicted, not Unavailable. No penalty is taken, and both the producer's reservation and the demander's bond stay held.
async fn silent_without_premise_holds() {
    kaspa_core::log::try_init_logger("warn");
    assert!(kaspa_consensus_core::palw_court_service_binding_v1::PALW_COURT_SERVICE_INCLUSION_PREMISE_V1.is_none(), "production premise");
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
            Stage::Missing { stage, lane, needs } => rows.push((*stage, format!("UNIMPLEMENTED [{lane}] {needs}"))),
        }
    }
    for (name, status) in &rows {
        eprintln!("[spine] {status} | {name}");
    }
    let open = rows.iter().filter(|(_, s)| s != "PASS").count();
    assert_eq!(open, 0, "the Panel=1 kernel slice is not closed: {open} stage(s) open");
}
