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
        needs: "the kernel route's file_proof through palw_court_paid_ingress_v1 (schedule typed-missing until injected) \
                under a continuously refilling coalition",
    },
    Stage::Run(
        "S3c silence: withheld material -> demand -> duty default, fixed penalty, no fraud slash (IBD node)",
        Mode::Panel,
        Start::Ibd,
        Outcome::DefaultOnWithheldMaterial,
    ),
    Stage::Missing {
        stage: "S3c' no supply => no slash: an undelivered duty holds evidence/bond without a default",
        lane: "F02 (P0-1)",
        needs: "a supply-absent branch in the kernel route's default fold (the default is deadline-only today)",
    },
    Stage::Missing {
        stage: "S4 reorg across the default / Final / settlement DAA of this slice (recovery.rs covers the pursuit only)",
        lane: "SPINE",
        needs: "a reorg crossing S3a-c's deciding block, both nodes compared with agree()",
    },
];

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
            Stage::Missing { stage, lane, needs } => rows.push((*stage, format!("UNIMPLEMENTED [{lane}] {needs}"))),
        }
    }
    for (name, status) in &rows {
        eprintln!("[spine] {status} | {name}");
    }
    let open = rows.iter().filter(|(_, s)| s != "PASS").count();
    assert_eq!(open, 0, "the Panel=1 kernel slice is not closed: {open} stage(s) open");
}
