//! **G14 U06/U07 (lane g14p/court-clock): `palw_court_clock_v2` on testnet-12's own fold** — the fence test-armed on the bundle's
//! mirror beside `palw_legacy_public_filer_v1` (both refused by real validation; fixtures fold without validating, as LG14-A's do).
//!
//! Every [`Chain::step_at`] block is re-applied, reverted and reloaded, so each state written here is also a reorg check.
//!
//! Run: cargo test -p kaspa-consensus-core --test court_clock_v2_fold
#![allow(warnings)]
#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_court_clock_v2::{
    palw_claim_court_hard_deadline_v2, palw_court_clock_v2_governs, palw_dispute_record_hard_deadline_v2,
};
use kaspa_consensus_core::palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA};
use kaspa_consensus_core::palw_legacy_public_filer_v1::{
    PALW_DISPUTE_RESERVATION_VERSION_V1, PalwDisputeReservationV1, palw_dispute_hard_deadline_v1,
};
use kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1;

/// testnet-12 with the public filer armed from genesis and the court clock v2 at `clock` (`None`: unarmed).
fn armed(clock: Option<u64>) -> Params {
    let mut p = t12();
    p.palw_legacy_public_filer_v1 = Some(ForkActivation::new(0));
    p.palw_court_clock_v2 = clock.map(ForkActivation::new);
    p.sync_palw_legacy_public_filer_v1();
    p.sync_palw_g14_gates_v1();
    p
}

fn reserve(c: &Chain, claim: Hash64, reserver: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let record = c.claim(&claim);
    PalwConsensusObjectV2::DisputeReservedV1 {
        reservation: Box::new(PalwDisputeReservationV1 {
            version: PALW_DISPUTE_RESERVATION_VERSION_V1,
            claim,
            execution_root: record.execution_root,
            trace_root: record.trace_root,
            reserver,
        }),
        signature: Vec::new(),
    }
}

fn licensed_floor_claim(c: &mut Chain, seed: u64) -> Hash64 {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    id
}

fn bystander(c: &Chain) -> PalwBondKeyV2 {
    genesis_bonds(&c.p)[7].0
}

#[test]
fn court_clock_v2_validation_needs_the_filer_and_the_mirror() {
    let mut p = t12();
    p.palw_court_clock_v2 = Some(ForkActivation::new(10));
    p.sync_palw_g14_gates_v1();
    assert!(format!("{:?}", p.validate_palw_court_clock_v2()).contains("palw_legacy_public_filer_v1"));
    let mut p = armed(Some(10));
    assert!(p.validate_palw_court_clock_v2().is_ok());
    p.palw_court_clock_v2 = Some(ForkActivation::new(11));
    assert!(format!("{:?}", p.validate_palw_court_clock_v2()).contains("mirror"));
    let mut never = t12();
    never.palw_court_clock_v2 = Some(ForkActivation::never());
    assert!(never.validate_palw_court_clock_v2().is_ok(), "Some(never()) is dormant");
}

/// **The unarmed fold is byte-identical** with the fence `None`, `Some(never())` and an unreached height.
#[test]
fn court_clock_v2_unarmed_is_byte_identical() {
    let script = |p: Params| {
        let mut c = Chain::new(p);
        let id = licensed_floor_claim(&mut c, 0x51);
        let mut roots = vec![c.s.state_root()];
        c.step(&[reserve(&c, id, bystander(&c))]);
        roots.push(c.s.state_root());
        roots
    };
    let base = script(armed(None));
    let mut never = armed(None);
    never.palw_court_clock_v2 = Some(ForkActivation::never());
    never.sync_palw_g14_gates_v1();
    assert_eq!(script(never), base);
    assert_eq!(script(armed(Some(9_000_000))), base);
}

/// **U07: a dispute extends every clock at once, from the ORIGINAL acceptance.** The undisputed claim reads `a + 3,120`; the first
/// reservation's one write makes it `a + 10,000`, the retention already covers it (F07, raised at creation), the Final hold reaches
/// exactly `a + 10,000` and lapses at `a + 10,001`.
#[test]
fn court_clock_v2_a_dispute_extends_every_clock_atomically_and_reverts() {
    let mut c = Chain::new(armed(Some(0)));
    let id = licensed_floor_claim(&mut c, 0x52);
    let claim = c.claim(&id);
    let a = claim.accepted_daa;
    let w = palw_da_disclose_window_daa_v1(&c.sp);
    assert!(palw_court_clock_v2_governs(&c.sp, &claim));
    assert!(claim.trace_retention_daa >= a + DISPUTE_WINDOW_DAA + w, "F07: the maximal extension is owed from acceptance");
    assert_eq!(palw_claim_court_hard_deadline_v2(&c.sp, None, &claim, w), a + NOTICE_WINDOW_DAA, "undisputed: the normal window");

    // Reorg: the extension is one delta entry set; reverting it restores the undisputed clock.
    let parent = c.s.clone();
    let reserver = bystander(&c);
    c.step(&[reserve(&c, id, reserver)]);
    let record = c.s.legacy_dispute_v1(&id).expect("the record").clone();
    assert_eq!(record.hard_deadline_daa, a + DISPUTE_WINDOW_DAA, "extended to a + 10,000, not filing + 10,000");
    assert_eq!(palw_claim_court_hard_deadline_v2(&c.sp, Some(&record), &c.claim(&id), w), a + DISPUTE_WINDOW_DAA);
    assert_eq!(c.claim(&id).trace_retention_daa, claim.trace_retention_daa, "retention was owed already: no new obligation");
    assert_eq!(c.s.deadline_of(&id), None, "the Final hold rides the same write");
    assert!(parent.legacy_dispute_v1(&id).is_none(), "the parent (the reorged-away branch) holds no extension");

    c.step_at(a + DISPUTE_WINDOW_DAA, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "held through a + 10,000 (inclusive)");
    c.step_at(a + DISPUTE_WINDOW_DAA + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let record = c.s.legacy_dispute_v1(&id).expect("the record").clone();
    assert!(record.live.is_empty() && record.closed.contains(&reserver), "the reservation lapses at a + 10,001");
}

/// **ADR-0198 A boundary (inclusive) and the 10,000 hard deadline, -1 / = / +1**, on a real claim's record.
#[test]
fn court_clock_v2_notice_3120_and_hard_10000_boundaries() {
    let mut c = Chain::new(armed(Some(0)));
    let id = licensed_floor_claim(&mut c, 0x53);
    let claim = c.claim(&id);
    let (a, w) = (claim.accepted_daa, palw_da_disclose_window_daa_v1(&c.sp));
    for (now, ok) in [(a + NOTICE_WINDOW_DAA - 1, true), (a + NOTICE_WINDOW_DAA, true), (a + NOTICE_WINDOW_DAA + 1, false)] {
        let got = palw_dispute_record_hard_deadline_v2(&c.sp, &claim, w, 0, &[], now);
        if ok {
            assert_eq!(got, Ok(a + DISPUTE_WINDOW_DAA), "a first dispute at {now} extends to a + 10,000");
        } else {
            assert!(got.unwrap_err().contains("3,120"));
        }
    }
    let mut record = c.s.legacy_dispute_v1(&id).cloned().unwrap_or_default();
    record.hard_deadline_daa = a + DISPUTE_WINDOW_DAA;
    let hard = palw_claim_court_hard_deadline_v2(&c.sp, Some(&record), &claim, w);
    for (d, inside) in [(a + DISPUTE_WINDOW_DAA - 1, true), (a + DISPUTE_WINDOW_DAA, true), (a + DISPUTE_WINDOW_DAA + 1, false)] {
        assert_eq!(d <= hard, inside);
    }
}

/// **T15: a claim accepted BELOW the fence keeps the old clock** — even when the fence is reached and it is disputed after it.
#[test]
fn court_clock_v2_an_old_claim_keeps_the_old_clock_across_the_fence() {
    let mut c = Chain::new(armed(None));
    let id = licensed_floor_claim(&mut c, 0x54);
    let claim = c.claim(&id);
    let w = palw_da_disclose_window_daa_v1(&c.sp);
    let legacy = palw_dispute_hard_deadline_v1(&claim, w);
    for from in [claim.accepted_daa + 1, claim.accepted_daa + 50] {
        let sp = c.sp.clone().with_court_clock_v2_from_daa(Some(from));
        assert!(!palw_court_clock_v2_governs(&sp, &claim));
        assert_eq!(palw_claim_court_hard_deadline_v2(&sp, None, &claim, w), legacy);
        // Disputed long after the fence, still the old formula; no 3,120 window applies to it.
        assert_eq!(palw_dispute_record_hard_deadline_v2(&sp, &claim, w, 0, &[], from + 20_000), Ok(legacy));
    }
    // At equality the claim IS governed.
    let sp = c.sp.clone().with_court_clock_v2_from_daa(Some(claim.accepted_daa));
    assert!(palw_court_clock_v2_governs(&sp, &claim));
}
