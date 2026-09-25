//! **ADR-0152 v3.1 T02b, the regenesis half (post-edit 9, S-SPEC §1e): at testnet-12's regenesis no
//! X10 fence exists, and RT#2 is S0′ on every class.**
//!
//! `palw_rcore_attributed_charging` (X10) is M6's flag day, armed later on public t12; until it is,
//! the second failed panel forfeits the producer's commitment on EVERY class — the floor, the 8k row
//! (released at licence like the floor, U1), and the 2M row (C7, held to Final, which stays S0′ on
//! both sides of X10). This file drives the three through testnet-12's own fold (`rcore_common`'s
//! `Chain`: every block re-applied, reverted and reloaded): the first panel's receipt window closes
//! (RT#1: a redraw, S0, nothing charged), the second closes too (RT#2), and the claim voids
//! `ReceiptTimeout` with the producer charged exactly `palw_claim_bond_reservation_v1` (`w + esc +
//! rr`) — no strike, no action tier, no consumed offence, no seat charged (a silent seat signed
//! nothing and locked nothing). The fence-crossing twin (RT#2 before X10 forfeits, after it S0
//! outside C7) is written with M6's field, which this line does not have.
//!
//! The 2M row is closed at launch (§4-quater U-D1: its attempts are refused
//! `ClassDeadlineUnmeasured` until a flag day measures its deadline), so its RT#2 is driven on
//! `t12_2m_open` — testnet-12 with the 2M row's measured deadline installed, the premise of every
//! live-2M test (the launch network's refusal is `t12_class_verify_deadline`'s T-D2). The free-prompt lane's
//! RT#2 (with its receipt rights `rr`) is `rcore_m5_q5_gate`'s
//! `t40_dl1_q5_a_compute_priced_fp_licence_voids_not_replay_backed_forfeiting_its_receipt_rights`
//! (NotReplayBacked charged exactly as the second `ReceiptTimeout`, `rr` included).
//!
//! Run: cargo test -p kaspa-consensus-core --test t02b_rt2_is_s0_prime_on_every_class

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_state_v2::{PalwVoidReasonV2, palw_claim_bond_reservation_v1};

/// Empty blocks up to `daa`: the deadline's block, then the first past it.
fn run_to(c: &mut Chain, daa: u64) {
    if daa > c.daa + 1 {
        c.step_at(daa - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }
    c.step_at(daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
}

/// The collateral of every bond on the chain.
fn collaterals(c: &Chain) -> Vec<(PalwBondKeyV2, u64)> {
    c.s.bonds_iter().map(|(k, b)| (*k, b.collateral)).collect()
}

/// **One class's two failed panels**: `open` makes the claim (and names its producer), `seat` binds
/// it to its panel (re-readying a model row's seats first). RT#1 redraws and charges nothing; RT#2
/// voids `ReceiptTimeout` and charges the producer its commitment, exactly, and nothing else.
fn two_failed_panels(
    label: &str,
    mut c: Chain,
    open: impl Fn(&mut Chain) -> (Hash64, PalwBondKeyV2),
    seat: impl Fn(&mut Chain, Hash64),
) {
    let (id, producer) = open(&mut c);
    let commitment = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&id)).expect("the claim's commitment");
    assert!(commitment > 0, "{label}: the claim commits collateral");
    let recorded = PalwStateCarriageV2::from_state(&c.s).consumed_offences.len();
    seat(&mut c, id);
    let before = collaterals(&c);

    // RT#1: the first panel's receipt window closes — a redraw, S0.
    let at = c.s.deadline_of(&id).expect("the first receipt deadline");
    run_to(&mut c, at + 1);
    let redrawn = c.claim(&id);
    assert!(redrawn.rebound_daa.is_some(), "{label}: RT#1 redraws");
    assert!(!redrawn.phase.is_terminal(), "{label}: RT#1 does not void");
    assert_eq!(collaterals(&c), before, "{label}: S0 — RT#1 charges no bond");

    // RT#2: the second panel's window closes too.
    seat(&mut c, id);
    let at = c.s.deadline_of(&id).expect("the second receipt deadline");
    run_to(&mut c, at + 1);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. }),
        "{label}: RT#2 voids ReceiptTimeout: {:?}",
        c.claim(&id).phase
    );
    for (bond, was) in &before {
        let now = c.s.bond(bond).expect("the bond").collateral;
        if *bond == producer {
            assert_eq!(u128::from(was - now), commitment, "{label}: S0′ — the producer forfeits exactly its commitment");
        } else {
            assert_eq!(now, *was, "{label}: no other bond is charged (a silent seat locked nothing): {bond:?}");
        }
    }
    assert_eq!(c.s.withholding_strikes(&producer), None, "{label}: S0′ writes no strike");
    assert_eq!(PalwStateCarriageV2::from_state(&c.s).consumed_offences.len(), recorded, "{label}: S0′ writes no record");
    println!("T02b {label}: RT#2 forfeited {commitment} sompi, S0′");
}

#[test]
fn t02b_rt2_is_s0_prime_on_the_floor_the_8k_row_and_the_2m_row() {
    // The floor.
    two_failed_panels(
        "floor",
        Chain::new(t12()),
        |c| {
            let (producer, _, _) = floor_producer(&c.p);
            (c.floor_claim(0x02B0), producer)
        },
        |c, id| {
            let seats = c.floor_seats();
            c.bind(id, &seats);
        },
    );

    // The 8k row: released at licence like the floor (U1), and RT#2 is S0′ on it as on the floor.
    let p = t12();
    let (short, _) = model_classes(&p);
    two_failed_panels(
        "8k row",
        model_chain(p, short, 1),
        |c| (model_claim(c, short, 1, 0x02B8), bond_key(1)),
        |c, id| {
            let seats = honest_seats(&c.p, 5);
            c.s = readied(&c.sp, &c.s, &honest(&c.p), short, c.daa);
            c.bind(id, &seats);
        },
    );

    // The 2M row (C7) is closed at launch (U-D1; `t12_class_verify_deadline`'s
    // `td2_the_2m_row_is_refused_at_launch_attempt_and_free_prompt` refuses its attempt and its free
    // prompt `ClassDeadlineUnmeasured`), so no 2M claim is accepted on the network that ships…
    // …and once a flag day opens it, its RT#2 is S0′ as well (C7 stays S0′ on both sides of X10).
    let p = t12_2m_open();
    let (_, id2m) = model_classes(&p);
    assert_eq!(p.palw_rcore_conservative_classes, &[id2m], "the premise: C7 is the 2M row");
    two_failed_panels(
        "2M row (C7, past its flag day)",
        model_chain(p, id2m, 1),
        |c| (model_claim(c, id2m, 1, 0x02B2), bond_key(1)),
        |c, id| {
            let seats = honest_seats(&c.p, 5);
            c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
            c.bind(id, &seats);
        },
    );
}

/// **T02's zero cells: `BindTimeout` and `NoCapablePanel` charge nothing** (S0, beside RT#1 above). A
/// claim no panel ever binds voids at its bind window's backstop (no attempt block anchors it here,
/// so step 4c does not fire and the sweep does): on the floor as `BindTimeout`, and on the 8k row —
/// a rowed class whose readied seats have lapsed by then, so it cannot seat a panel — as
/// `NoCapablePanel` (`bind_timeout_reason`, the capacity's failure named). Either way no bond's
/// collateral moves, no strike or record is written, and the producer's reservation leaves the
/// ledger in the void's block (`void_claim`, never `void_and_slash`).
#[test]
fn t02_bind_timeout_and_no_capable_panel_charge_nothing() {
    let p = t12();
    let (short, _) = model_classes(&p);
    let cases: Vec<(&str, Chain, Box<dyn Fn(&mut Chain) -> (Hash64, PalwBondKeyV2)>, PalwVoidReasonV2)> = vec![
        (
            "floor",
            Chain::new(t12()),
            Box::new(|c: &mut Chain| {
                let (producer, _, _) = floor_producer(&c.p);
                (c.floor_claim(0x02B7), producer)
            }),
            PalwVoidReasonV2::BindTimeout,
        ),
        ("8k row", model_chain(p, short, 1), Box::new(move |c: &mut Chain| (model_claim(c, short, 1, 0x02BC), bond_key(1))), PalwVoidReasonV2::NoCapablePanel),
    ];
    for (label, mut c, open, want) in cases {
        let (id, producer) = open(&mut c);
        let reserved_before_void = c.reserved(&producer);
        let commitment = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&id)).expect("the claim's commitment");
        let before = collaterals(&c);
        let recorded = PalwStateCarriageV2::from_state(&c.s).consumed_offences.len();
        let at = c.s.deadline_of(&id).expect("the bind window's backstop");
        run_to(&mut c, at + 1);
        match c.claim(&id).phase {
            PalwClaimPhaseV2::Voided { reason, .. } => assert_eq!(reason, want, "{label}: the bind window's void"),
            other => panic!("{label}: the unbound claim voids at its backstop, got {other:?}"),
        }
        assert_eq!(collaterals(&c), before, "{label}: S0 — no bond is charged");
        assert_eq!(c.s.withholding_strikes(&producer), None, "{label}: no strike");
        assert_eq!(PalwStateCarriageV2::from_state(&c.s).consumed_offences.len(), recorded, "{label}: no record");
        assert!(
            c.reserved(&producer) + commitment <= reserved_before_void,
            "{label}: the reservation ({commitment} sompi) leaves the ledger with the void"
        );
    }
}
