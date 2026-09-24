//! **LANE L4 (area 6) — an adversarial read of the two-clock fix, and the rework that closed it.**
//!
//! The 2026-09-24 DoS audit found the 09-23 second clock broken in two directions (§4):
//!
//! * **L4-C1** — ADR-0065 D1 maturity read `palw_settled_anchor_floor_daa_v1`, a WALK over the
//!   `Final` attempt claims the state still retained; every claim retires `claim_retirement_daa`
//!   (3,000 on t12) after it went terminal, so a heartbeat-only stretch longer than that emptied
//!   the walk and the "bootstrap waiver" (`None`) handed a sybil D1's bare DAA window.
//! * **L4-C2** — the counter-based halves had no upper bound: with no attempt reaching `Final`
//!   (t12's registry stops seating a panel at the third retirement) every lock, liability and
//!   retiring bond froze forever — up to the whole genesis registry.
//!
//! The rework (`6bb8c844`, fixes #3 and #13a): the floor reads the rooted anchor ring
//! `recent_anchor_daas`, and answers `Some(0)` — only genesis bonds are mature — when what it needs
//! was pruned, never the waiver; and `palw_second_clock_depth_v1` waives the second clock once no
//! anchor has settled for `2 × window_court`, so a freeze lasts at most that long and the DAA
//! clock alone decides after it. The tests below assert that fixed behaviour on testnet-12's own
//! params, the escape on its boundary (`E − 1`, `E`, `E + 1`), and on every exit the audit named:
//! the slash lock, the panel liability, the retiring bond's collateral (`v3` and the withdrawal
//! gate's `v4`), and the fold's retire gate.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{
    PALW_T12_BOND_MATURITY_WINDOW_DAA, PALW_T12_SETTLED_ANCHOR_DEPTH, Params, palw_v2_bond_withdrawal_delay_at_v1,
};
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{
    palw_bond_maturity_window_v2, palw_seat_maturity_floor_v1, palw_settled_anchor_floor_daa_v1,
};
use kaspa_consensus_core::palw_panel_var_v1::{
    PalwPanelLiabilityRecordV1, PalwSlashableLockV1, palw_liability_still_locks_v2, palw_panel_liability_expiry_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2, PalwChainStateV2, PalwConsensusObjectV2,
    PalwStateCarriageV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras,
    palw_bond_collateral_is_locked_v3, palw_bond_collateral_is_locked_v4, palw_second_clock_depth_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn bundle(p: &Params) -> &PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b,
        _ => panic!("testnet-12 is ConsensusV2"),
    }
}

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

fn bond(n: u64, registered_daa: u64, status: PalwBondStatusV2) -> PalwBondStateV2 {
    PalwBondStateV2 {
        pubkey: vec![n as u8; 32],
        operator_id: h(0x0900_0000 + n),
        collateral: PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI,
        slashed: 0,
        status,
        registered_daa,
        payout_payload: h(0x0A00_0000 + n),
        capable_classes: Default::default(),
    }
}

/// The extras testnet-12's processor hands the fold (the fence at DAA 0).
fn t12_extras(p: &Params) -> PalwTransitionExtrasV1 {
    assert!(p.palw_audit_2026_09_23_active_at(0), "testnet-12 arms the audit fence from genesis");
    PalwTransitionExtrasV1 {
        audit_2026_09_23_active: true,
        settled_anchor_depth: p.palw_settled_anchor_depth,
        objective_offence_daa: Some(0),
        ..Default::default()
    }
}

/// `depth` anchors, one every 10 DAA, the last at `last`.
fn ring_ending_at(last: u64, depth: u64) -> Vec<u64> {
    (0..depth).map(|i| last - 10 * (depth - 1 - i)).collect()
}

/// **L4-C1, fixed. D1's second clock no longer waives itself when the Finals retire.**
///
/// (a) The audit's exact state — 10,000 anchors settled, not one in the state any more — now
/// answers `Some(0)`: the ring cannot say when the 30th most recent anchor settled, so only genesis
/// bonds are mature, never the bootstrap waiver.
/// (b) The state a live chain actually carries: the ring holds the last `depth` licences before a
/// heartbeat-only stretch. The sybil that registers during the stretch is refused at the audit's
/// attack time (`claim_retirement + window` after the last anchor), and stays refused until the
/// liveness escape at `2 × window_court`; from then on D1's DAA window alone decides, as it does on
/// every network below the fence.
#[test]
fn d1_second_clock_is_not_waived_once_the_finals_retire() {
    let p = t12();
    let b = bundle(&p);
    let depth = PALW_T12_SETTLED_ANCHOR_DEPTH;
    let retirement = b.state.claim_retirement_daa();
    let window = PALW_T12_BOND_MATURITY_WINDOW_DAA;
    let wc = b.state.window_court();
    let anchor_daa = 50_000u64;
    let sybil_registered = anchor_daa - window;

    // (a) the audit's state
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carriage.bonds.insert(bond_key(7), bond(7, sybil_registered, PalwBondStatusV2::Active));
    carriage.settled_attempt_finals = 10_000;
    let state = carriage.into_state(&b.state, None).expect("consistent");
    let floor = palw_settled_anchor_floor_daa_v1(&state, anchor_daa, depth);
    let widened = palw_bond_maturity_window_v2(anchor_daa, window, floor);
    let registered_by = palw_seat_maturity_floor_v1(anchor_daa, Some(widened)).unwrap();
    println!("=== L4-C1 (fixed): D1 on a chain whose Finals have retired ===");
    println!("(a) counter 10,000, ring empty: floor {floor:?}, D1 window {widened}, registered-by {registered_by}");
    assert_eq!(floor, Some(0), "the pruned answer is the conservative end, never the waiver");
    assert!(sybil_registered > registered_by, "a bond registered after the last anchor is NOT mature");

    // (b) the ring a chain carries into a heartbeat-only stretch
    let last = 40_000u64;
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carriage.settled_attempt_finals = 10_000;
    carriage.recent_anchor_daas = ring_ending_at(last, depth);
    let sybil_registered = last + 1;
    carriage.bonds.insert(bond_key(7), bond(7, sybil_registered, PalwBondStatusV2::Active));
    let state = carriage.into_state(&b.state, None).expect("consistent");
    // What the processor computes (`palw_bond_maturity_window_at`): the escaped depth, then the floor.
    let mature_at = |anchor: u64| -> (Option<u64>, bool) {
        let floor = palw_second_clock_depth_v1(Some(depth), state.recent_anchor_daas(), anchor, wc)
            .and_then(|depth| palw_settled_anchor_floor_daa_v1(&state, anchor, depth));
        let registered_by = palw_seat_maturity_floor_v1(anchor, Some(palw_bond_maturity_window_v2(anchor, window, floor))).unwrap();
        (floor, sybil_registered <= registered_by)
    };
    let attack = last + retirement + window;
    let e = last + 2 * wc;
    let (floor_attack, mature_attack) = mature_at(attack);
    println!(
        "(b) last anchor {last}; sybil registered {sybil_registered}; the audit's attack at {attack}: floor {floor_attack:?} mature {mature_attack}"
    );
    assert_eq!(floor_attack, Some(ring_ending_at(last, depth)[0]), "the ring answers: the 30th most recent anchor");
    assert!(!mature_attack, "zero anchors since registration: not mature at the audit's attack time");
    for anchor in [e - 1, e, e + 1] {
        let (floor, mature) = mature_at(anchor);
        println!("    anchor {anchor} (E {e}): floor {floor:?} mature {mature}");
        assert_eq!(
            mature,
            anchor >= e,
            "the second clock binds until E = last + 2 × window_court, and only then does D1's DAA window decide"
        );
    }
}

/// **L4-C2, fixed. The freeze is bounded.** A seat that signed `Valid` on one of the last
/// `depth − 1` anchors before the lane stopped licensing: its lock, its liability and its retiring
/// bond are held by the second clock up to `E − 1 = last anchor + 2 × window_court − 1`, and at
/// `E` and after the DAA clock alone releases all three — on the predicates, on the withdrawal gate
/// (`v4`, duty gate on) and on the fold's retire gate.
#[test]
fn every_exit_is_released_on_the_daa_clock_after_two_court_windows_without_a_licence() {
    let p = t12();
    let b = bundle(&p);
    let depth = PALW_T12_SETTLED_ANCHOR_DEPTH;
    let wc = b.state.window_court();
    let s0 = 1_234u64;
    let settled_now = s0 + depth - 1; // the lane stopped one anchor short of releasing it
    let last = 20_000u64; // the last licence
    let ring = ring_ending_at(last, depth);
    let e = last + 2 * wc;
    let delay = palw_v2_bond_withdrawal_delay_at_v1(b, p.palw_da_court, last);
    // Every DAA half has run out well before E: the second clock is the only thing still holding.
    let final_daa = last - 5;
    let expiry = palw_panel_liability_expiry_v1(final_daa, wc);
    let since = last - delay; // retired long enough ago that the delay has elapsed by `last`
    assert!(expiry < e - 1 && since + delay < e - 1);

    let lock = PalwSlashableLockV1 {
        claim: h(1),
        amount: 1,
        expiry_daa: expiry,
        settled_at_final: s0,
        attested: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::NONE,
        segments: 0,
    };
    let liability = PalwPanelLiabilityRecordV1 {
        claim_id: h(1),
        work_id: h(2),
        class_id: h(3),
        execution_root: h(4),
        output_root: h(5),
        executor_bond: bond_key(9),
        voided_daa: None,
        void_reason: None,
        valid_signers: vec![(bond_key(1).0, h(1))],
        locked_sompi: 1,
        expiry_daa: expiry,
        settled_at_final: s0,
        job_identity: kaspa_consensus_core::Hash64::default(),
        free_prompt: false,
        trace_root: kaspa_consensus_core::Hash64::default(),
        segment_count: 0,
        licence_door: None,
        basis_k: 0,
        g_res_sompi: 0,
        escrowed_reward: 0,
    };
    let retiring = bond(2, 0, PalwBondStatusV2::Retiring { since_daa: since, settled_at_since: s0 });

    // The state the withdrawal gate and the fold read: the seat (Active, holding the lock), the
    // retiring bond, the ring, the counter.
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carriage.bonds.insert(bond_key(1), bond(1, 0, PalwBondStatusV2::Active));
    carriage.bonds.insert(bond_key(2), retiring.clone());
    carriage.slashable_locks.insert((bond_key(1), h(1)), lock);
    carriage.settled_attempt_finals = settled_now;
    carriage.recent_anchor_daas = ring.clone();
    let state = carriage.into_state(&b.state, None).expect("consistent");
    let extras = t12_extras(&p);

    println!("=== L4-C2 (fixed): the freeze ends at E = last anchor {last} + 2 x window_court {wc} = {e} ===");
    for now in [e - 1, e, e + 1] {
        let escaped = palw_second_clock_depth_v1(Some(depth), &ring, now, wc);
        let bound = now < e;
        let lock_live = lock.is_live_v2(now, settled_now, escaped);
        let liability_live = palw_liability_still_locks_v2(&liability, now, settled_now, escaped);
        let v3 = palw_bond_collateral_is_locked_v3(&retiring, now, delay, settled_now, escaped);
        let v4_retiring = palw_bond_collateral_is_locked_v4(&state, &bond_key(2), &retiring, now, delay, escaped, true);
        let seat = state.bond(&bond_key(1)).unwrap().clone();
        let seat_v4 = palw_bond_collateral_is_locked_v4(&state, &bond_key(1), &seat, now, delay, escaped, true);
        // The fold's retire gate for the seat, on a block at `now`.
        let ctx = PalwBlockContextV2 { block: h(0xB10C_0000 + now), daa_score: now, blue_score: now, subsidy: 0 };
        let retire = [PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(1), signature: vec![9u8; 64] }];
        let folded = apply_palw_transition_v2_with_extras(&state, &b.state, &ctx, &retire, None, false, false, false, false, &extras);
        println!(
            "DAA {now}: depth {escaped:?} lock {lock_live} liability {liability_live} retiring v3 {v3} v4 {v4_retiring} seat v4 {seat_v4} retire {}",
            if folded.is_ok() { "accepted".to_string() } else { format!("{:?}", folded.as_ref().err()) }
        );
        assert_eq!(escaped, if bound { Some(depth) } else { None });
        assert_eq!(lock_live, bound, "the slash lock at {now}");
        assert_eq!(liability_live, bound, "the panel liability at {now}");
        assert_eq!(v3, bound, "the retiring bond's collateral (v3) at {now}");
        assert_eq!(v4_retiring, bound, "the withdrawal gate (v4) on the retiring bond at {now}");
        assert!(seat_v4, "the seat is Active: its collateral is locked whatever the clocks say");
        if bound {
            assert!(
                matches!(folded, Err(PalwStateV2Error::BondRetireWhileSlashableLocked { .. })),
                "the fold refuses the seat's retirement at {now}: {:?}",
                folded.err()
            );
        } else {
            assert!(folded.is_ok(), "the fold accepts the seat's retirement at {now}: {:?}", folded.err());
        }
    }
    // What stays frozen for at most E − last DAA now, on this network's own registry.
    let genesis_bonds = 8u64;
    println!(
        "bounded: a stall freezes the registry ({} MSK) for at most 2 x window_court = {} DAA after the last licence",
        u128::from(genesis_bonds) * u128::from(PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI) / 100_000_000,
        2 * wc
    );
}

/// **PRE-FIX RECORD of L4-C2** — the raw predicates, handed the UN-escaped depth as the 09-23 fix
/// handed them, freeze every exit at any DAA. Kept for the measurement; the fold and the processor
/// now pass `palw_second_clock_depth_v1`'s escaped depth (see the test above).
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: the 09-23 two-clock fix's unbounded freeze (dos_l4_two_clock L4-C2), measured on the raw predicates with the un-escaped depth; closed by 6bb8c844's liveness escape"]
fn every_exit_waits_for_attempt_finals_with_no_upper_bound() {
    let p = t12();
    let b = bundle(&p);
    let depth = PALW_T12_SETTLED_ANCHOR_DEPTH;
    let delay = b.bond.withdrawal_delay_daa();
    let s0 = 1_234u64;

    let lock = PalwSlashableLockV1 {
        claim: h(1),
        amount: 1,
        expiry_daa: 10_000,
        settled_at_final: s0,
        attested: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::NONE,
        segments: 0,
    };
    let retiring = bond(1, 0, PalwBondStatusV2::Retiring { since_daa: 10_000, settled_at_since: s0 });
    for now in [10_000u64, 10_000 + delay, 1_000_000_000, u64::MAX - 1] {
        assert!(lock.is_live_v2(now, s0 + depth - 1, Some(depth)), "lock live at DAA {now}");
        assert!(
            palw_bond_collateral_is_locked_v3(&retiring, now, delay, s0 + depth - 1, Some(depth)),
            "collateral frozen at DAA {now}"
        );
    }
    assert!(!lock.is_live_v2(10_000, s0 + depth, Some(depth)));
    assert!(!palw_bond_collateral_is_locked_v3(&retiring, 10_000 + delay, delay, s0 + depth, Some(depth)));

    // What a stall freezes, on this network's own registry.
    let seats = b.panel.seat_count() as u64;
    let quorum = b.panel.quorum() as u64;
    let genesis_bonds = 8u64;
    // The executor's own operator is excluded from its panel, so `seat_count` OTHER active
    // operators must remain for any attempt claim to bind.
    let max_retirements_before_no_panel = genesis_bonds - 1 - seats;
    println!("=== L4-C2: the freeze ===");
    println!("depth = {depth} attempt Finals; withdrawal delay = {delay} DAA; seat_count = {seats}; quorum = {quorum}");
    println!(
        "a seat's exit after its last Valid needs >= {depth} Finals (lock) then >= {depth} more (Retiring) = {} Finals",
        2 * depth
    );
    println!("genesis registry = {genesis_bonds} bonds x {} sompi", PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI);
    println!(
        "retirements the registry absorbs before no attempt can bind = {max_retirements_before_no_panel}; the next one stops every Final"
    );
    println!(
        "collateral frozen once Finals stop (whole genesis registry) = {} sompi = {} MSK",
        u128::from(genesis_bonds) * u128::from(PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI),
        u128::from(genesis_bonds) * u128::from(PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI) / 100_000_000
    );
}

/// Premise of the never-pruned-rows finding: the objective-offence rules (which write every lock and
/// liability) are armed on testnet-12 from genesis.
#[test]
fn objective_offence_is_armed_on_t12() {
    let p = t12();
    println!("palw_objective_offence_at(0) = {}", p.palw_objective_offence_at(0));
    println!("palw_objective_offence_daa   = {:?}", p.palw_objective_offence_daa());
    assert!(p.palw_objective_offence_at(0));
}
