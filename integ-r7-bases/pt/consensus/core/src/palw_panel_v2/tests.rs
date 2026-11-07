//! **`palw_panel_v2`'s `tests` module**, moved out of `palw_panel_v2.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

use super::*;
use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2};
use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2, apply_palw_transition_v2};
use crate::tx::{TransactionId, TransactionOutpoint};

/// Operator identities are DERIVED from a key now, so the fixtures carry a key and let the
/// state machine mint the id — the same path a real registration takes.
fn op_key(v: u64) -> Vec<u8> {
    vec![v as u8; 8]
}

fn op_id(v: u64) -> Hash64 {
    crate::palw_state_v2::palw_operator_id_v2(&op_key(v))
}

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn state_params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap()
}

fn panel_params() -> PalwPanelParamsV2 {
    PalwPanelParamsV2::new(3, 2, 4).unwrap()
}

fn bond_outpoint(v: u64) -> TransactionOutpoint {
    TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 }
}

fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: BlockHash::from_u64_word(block), daa_score: daa, blue_score: blue, subsidy: 0 }
}

fn register(bond: u64, pubkey: u8, operator: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: PalwBondKeyV2(bond_outpoint(bond)),
        pubkey: vec![pubkey; 4],
        operator_pubkey: op_key(operator),
        collateral: 1_000_000,
        payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
        // The fixture's one class. ADR-0071 Decision 3 excludes an undeclared bond from the
        // draw, so a registry of silent bonds seats nobody — which is the rule working, and
        // which is why every fixture that expects a panel has to say what its seats can run.
        capable_classes: std::collections::BTreeSet::from([h64(1)]),
        signature: Vec::new(),
    }
}

/// `register`, declaring a class other than the one the fixture claims under — the only thing
/// that moves between the two halves of the capability test.
fn register_declaring(bond: u64, pubkey: u8, operator: u64, class_id: Hash64) -> PalwConsensusObjectV2 {
    match register(bond, pubkey, operator) {
        PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, collateral, payout_payload, signature, .. } => {
            PalwConsensusObjectV2::BondRegistered {
                bond,
                pubkey,
                operator_pubkey,
                collateral,
                payout_payload,
                capable_classes: std::collections::BTreeSet::from([class_id]),
                signature,
            }
        }
        other => other,
    }
}

/// [`populated_state`] with every bond declaring `class_id` instead of the fixture's.
fn populated_state_declaring(class_id: Hash64) -> (PalwChainStateV2, Hash64) {
    let objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        register_declaring(1, 7, 0x21, class_id),
        register_declaring(2, 8, 0x22, class_id),
        register_declaring(3, 9, 0x23, class_id),
        register_declaring(4, 10, 0x24, class_id),
        register_declaring(5, 11, 0x24, class_id),
        register_declaring(6, 12, 0x21, class_id),
    ];
    let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (s2, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
    (s2, claim_id)
}

fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    let bond = bond_outpoint(1);
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h64(999),
            challenge: challenge_v2(h64(999), h64(5), 1_700, nonce, h64(1), &bond),
            class_id: h64(1),
            executor_bond: bond,
            executor_pubkey: vec![7; 4],
            operator_id: op_id(0x21),
            artifact_root: h64(11),
            trace_root: h64(31),
            output_root: h64(32),
            pwu,
            trace_manifest_root: h64(33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: h64(41),
        },
        signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    }
}

/// Genesis + class + executor (bond 1, operator 0x21) + five more bonds: 2..=6 with distinct
/// operators, except bond 5 SHARES bond 4's operator (the dedup case) and bond 6 shares the
/// EXECUTOR's operator (the exclusion case).
fn populated_state() -> (PalwChainStateV2, Hash64) {
    let objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        register(1, 7, 0x21), // executor
        register(2, 8, 0x22),
        register(3, 9, 0x23),
        register(4, 10, 0x24),
        register(5, 11, 0x24), // same operator as bond 4
        register(6, 12, 0x21), // executor's operator — excluded outright
    ];
    let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (s2, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
    (s2, claim_id)
}

/// **C-02 (deep fence): the weighted draw seats a full panel, is deterministic, and below the
/// fence is byte-identical to the pre-C-02 one-ticket sortition.** `populated_state` has exactly
/// three eligible operators for three seats, so both draws seat all three — which is what pins
/// the liveness property: bucketing gives every eligible bond `>= 1` sub-ticket, so the eligible
/// set (and thus a full panel) is untouched.
#[test]
fn c02_weighted_draw_is_a_full_deterministic_panel_and_weighted_false_is_the_legacy_draw() {
    let (state, claim_id) = populated_state();
    let params = panel_params();
    let mc = state_params().min_collateral_sompi();
    let anchor = BlockHash::from_u64_word(0x5EA7);
    let unweighted =
        derive_panel_v2_with_capability_proof(&state, &params, &claim_id, anchor, mc, None, false, false).expect("unweighted seats");
    let weighted =
        derive_panel_v2_with_capability_proof(&state, &params, &claim_id, anchor, mc, None, false, true).expect("weighted seats");
    assert_eq!(unweighted.len(), params.seat_count as usize, "unweighted seats a full panel");
    assert_eq!(
        weighted.len(),
        params.seat_count as usize,
        "weighted seats a full panel too — the eligible set is untouched (liveness)"
    );
    // Below the fence `weighted == false` is exactly the legacy one-ticket sortition.
    assert_eq!(
        unweighted,
        derive_panel_v2(&state, &params, &claim_id, anchor, mc).expect("legacy draw"),
        "weighted=false is byte-identical to the pre-C-02 draw"
    );
    // Deterministic both sides — a pure function of (claim, anchor, registry).
    assert_eq!(
        weighted,
        derive_panel_v2_with_capability_proof(&state, &params, &claim_id, anchor, mc, None, false, true).expect("again"),
        "the weighted draw is deterministic"
    );
}

/// **C-02 (deep fence): the weighted draw favours higher-collateral operators.** Four eligible
/// operators compete for three seats — one is always left out. One holds `min_collateral`, three
/// hold `4096 × min_collateral`. Below the fence every operator has ONE ticket, so the low one is
/// left out no more often than any other (seated ~3/4 of draws); past the fence the three whales
/// draw 4096 sub-tickets each and crowd it out, so it is seated far less often. That is the whole
/// point of the finding: seat probability tracks stake, so the panel that decides a claim is
/// composed of the operators with the most to lose (whose seat then risks `claim.reserved`,
/// BC-SYBIL). Liveness holds throughout — a full three-seat panel every draw.
#[test]
fn c02_stake_weighting_favours_higher_collateral_seats_past_the_deep_fence() {
    let mc = 100u64; // state_params()'s min_collateral
    let bond = |b: u64, pk: u8, op: u64, collateral: u64| PalwConsensusObjectV2::BondRegistered {
        bond: PalwBondKeyV2(bond_outpoint(b)),
        pubkey: vec![pk; 4],
        operator_pubkey: op_key(op),
        collateral,
        payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
        capable_classes: std::collections::BTreeSet::from([h64(1)]),
        signature: Vec::new(),
    };
    let objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        bond(1, 7, 0x21, 1_000_000),  // executor — excluded from its own panel
        bond(2, 8, 0x22, mc),         // the low-collateral operator
        bond(3, 9, 0x23, mc * 4096),  // whale
        bond(4, 10, 0x24, mc * 4096), // whale
        bond(5, 11, 0x25, mc * 4096), // whale
    ];
    let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (state, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
    let params = panel_params(); // seat_count 3
    let low_op = state.bond(&PalwBondKeyV2(bond_outpoint(2))).expect("the low bond").operator_id;

    let low_seated = |weighted: bool| {
        (0u64..40)
            .filter(|i| {
                let anchor = BlockHash::from_u64_word(0xA000 + i);
                let seats = derive_panel_v2_with_capability_proof(&state, &params, &claim_id, anchor, mc, None, false, weighted)
                    .expect("a full panel");
                assert_eq!(seats.len(), 3, "liveness: a full three-seat panel under weighting too");
                seats.iter().any(|s| s.operator_id == low_op)
            })
            .count()
    };
    let unweighted = low_seated(false);
    let weighted = low_seated(true);
    assert!(
        weighted * 3 < unweighted,
        "past the fence the min-collateral operator is crowded out by the whales: seated {weighted}/40 weighted vs {unweighted}/40 unweighted"
    );
}

/// **ADR-0124 Decisions 3–5 in the draw.** Past the panel-economy fence a bond is drawn only
/// while it holds ten producer floors AND its free collateral covers the seat's reservation
/// under the shared ceiling; and every eligible bond draws one ticket, so the whale is no
/// likelier than the minimum bond — the deep fence's weighting is retired by the economy.
#[test]
fn adr0124_the_economy_draws_by_floor_and_headroom_and_one_ticket_a_bond() {
    let mc = 100u64; // state_params()'s min_collateral: the panel floor is 1,000
    let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
    let bond = |b: u64, pk: u8, op: u64, collateral: u64| PalwConsensusObjectV2::BondRegistered {
        bond: PalwBondKeyV2(bond_outpoint(b)),
        pubkey: vec![pk; 4],
        operator_pubkey: op_key(op),
        collateral,
        payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
        capable_classes: std::collections::BTreeSet::from([h64(1)]),
        signature: Vec::new(),
    };
    let objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        bond(1, 7, 0x21, 1_000_000),     // executor — excluded from its own panel
        bond(2, 8, 0x22, mc),            // holds the registry's floor, not the panel's
        bond(3, 9, 0x23, 1_000),         // holds the panel floor, but its ceiling (500) is under the seat's 600
        bond(4, 10, 0x24, 2_000),        // eligible: ceiling 1,000 covers 600
        bond(5, 11, 0x25, 2_000),        // eligible
        bond(6, 12, 0x26, 2_000 * 4096), // eligible, a whale
    ];
    let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
    let env = attempt(40, 1); // reserved = 40 × 5 = 200, so a seat reserves 600
    let claim_id = attempt_id_v2(&env.attempt);
    let (state, _) = apply_palw_transition_v2(&s1, &sp, &ctx(2, 101, 2), &[], Some(&env)).unwrap();
    assert_eq!(state.claim(&claim_id).unwrap().reserved, 200);
    let params = panel_params(); // seat_count 3
    let economy = crate::palw_panel_economy_v1::PalwSeatEconomyV1 {
        panel_floor_sompi: crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(mc),
        max_exposure_ratio_permille: 500,
        reward_multiple_permille: 0,
    };
    assert_eq!(economy.panel_floor_sompi, 1_000);
    let policy = PalwPanelDrawPolicyV1 {
        weighted: true,
        economy: Some(economy),
        readiness: None,
        independence: None,
        valid_lock: None,
        stake: None,
    };

    for i in 0..40u64 {
        let anchor = BlockHash::from_u64_word(0xB000 + i);
        let seats = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, policy).expect("a full panel");
        assert_eq!(seats.len(), 3);
        for seat in &seats {
            assert!(
                (4..=6u64).any(|n| seat.bond == PalwBondKeyV2(bond_outpoint(n))),
                "only bonds 4, 5 and 6 may sit: {:?} is under the floor or the ceiling",
                seat.bond
            );
        }
        // One ticket a bond: the draw past the economy equals the legacy unweighted draw over the
        // same eligible set, whatever `weighted` says.
        let unweighted = PalwPanelDrawPolicyV1 {
            weighted: false,
            economy: Some(economy),
            readiness: None,
            independence: None,
            valid_lock: None,
            stake: None,
        };
        assert_eq!(
            derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, unweighted).unwrap(),
            seats,
            "the economy retires stake weighting"
        );
    }
    // Below the fence the same registry draws bond 2 and bond 3 too (the legacy predicate).
    let legacy = (0..40u64).any(|i| {
        let anchor = BlockHash::from_u64_word(0xB000 + i);
        derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, PalwPanelDrawPolicyV1::default())
            .unwrap()
            .iter()
            .any(|s| (2..=3u64).any(|n| s.bond == PalwBondKeyV2(bond_outpoint(n))))
    });
    assert!(legacy, "below the fence the registry's floor is the only bar");
    // The acceptance layer recomputes the same panel under the same policy.
    let anchor = BlockHash::from_u64_word(0xB007);
    let seats = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, policy).unwrap();
    let fact = PalwAnchorFactV2 { anchor_block: anchor, anchor_daa: 105, predecessor_daa: 104, anchor_execution: None };
    validate_panel_bound_v2_with_policy(
        &state,
        &params,
        &sp,
        &ctx(3, 106, 3),
        &claim_id,
        &fact,
        anchor,
        &seats,
        None,
        false,
        policy,
        None,
    )
    .expect("the same policy recomputes the same panel");
    assert!(
        matches!(
            validate_panel_bound_v2_with_policy(
                &state,
                &params,
                &sp,
                &ctx(3, 106, 3),
                &claim_id,
                &fact,
                anchor,
                &seats,
                None,
                false,
                PalwPanelDrawPolicyV1::default(),
                None
            ),
            Err(PalwPanelV2Error::PanelMismatch)
        ),
        "a validator below the fence refuses the economy's panel, and vice versa"
    );
}

// ---- ADR-0130: one operator, one lottery entry; and the seat exposure floor ----

fn adr0130_class() -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ClassRegistered {
        class_id: h64(1),
        artifact_root: h64(11),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
        initial_target: u128::MAX / 2,
        share_permille: 1000,
        activation_daa: 0,
        admission: None,
    }
}

fn adr0130_bond(b: u64, pk: u8, op: u64, collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: PalwBondKeyV2(bond_outpoint(b)),
        pubkey: vec![pk; 4],
        operator_pubkey: op_key(op),
        collateral,
        payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
        capable_classes: std::collections::BTreeSet::from([h64(1)]),
        signature: Vec::new(),
    }
}

/// A registry, then the executor's 40-pwu attempt (`reserved` = 200) in a block of `subsidy`.
fn adr0130_state(sp: &PalwStateParamsV2, bonds: Vec<PalwConsensusObjectV2>, subsidy: u64) -> (PalwChainStateV2, Hash64) {
    let mut objects = vec![adr0130_class()];
    objects.extend(bonds);
    let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), sp, &ctx(1, 100, 1), &objects, None).unwrap();
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (s2, _) = apply_palw_transition_v2(&s1, sp, &PalwBlockContextV2 { subsidy, ..ctx(2, 101, 2) }, &[], Some(&env)).unwrap();
    (s2, claim_id)
}

fn adr0130_economy(reward_multiple_permille: u32) -> PalwPanelDrawPolicyV1 {
    PalwPanelDrawPolicyV1 {
        weighted: false,
        readiness: None,
        economy: Some(crate::palw_panel_economy_v1::PalwSeatEconomyV1 {
            panel_floor_sompi: crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(100),
            max_exposure_ratio_permille: 500,
            reward_multiple_permille,
        }),
        independence: None,
        valid_lock: None,
        stake: None,
    }
}

/// **ADR-0130: an operator is ONE lottery entry, whatever it holds.** Operator A holds ten
/// eligible bonds, B, C and D one each, for three seats.
///
/// Deterministically: A is entered once, with its eligible bond whose bond ticket is lowest, under
/// an operator ticket no bond enters — so the operators seated, and their order, are the same
/// whether A holds ten bonds or only one, anchor for anchor. Statistically: over many claim ids A
/// and B are seated equally often (three draws in four each), where the per-bond draw below the
/// fence seated the ten-bond operator almost always. Splitting collateral buys nothing.
#[test]
fn adr0130_an_operator_draws_one_entry_however_many_bonds_it_holds() {
    const A: u64 = 0x30;
    const B: u64 = 0x31;
    let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
    let mut ten = vec![adr0130_bond(1, 7, 0x21, 1_000_000)];
    for i in 0..10u64 {
        ten.push(adr0130_bond(10 + i, 40 + i as u8, A, 1_000_000));
    }
    let others = [adr0130_bond(20, 60, B, 1_000_000), adr0130_bond(21, 61, 0x32, 1_000_000), adr0130_bond(22, 62, 0x33, 1_000_000)];
    ten.extend(others.iter().cloned());
    let (state, claim_id) = adr0130_state(&sp, ten, 0);
    // The same registry with A holding bond 10 alone.
    let mut one = vec![adr0130_bond(1, 7, 0x21, 1_000_000), adr0130_bond(10, 40, A, 1_000_000)];
    one.extend(others.iter().cloned());
    let (lean, lean_claim) = adr0130_state(&sp, one, 0);
    assert_eq!(lean_claim, claim_id, "one attempt, one claim id, in both registries");
    let params = panel_params(); // seat_count 3
    let (a, b) = (op_id(A), op_id(B));
    let a_bonds: Vec<PalwBondKeyV2> = (10..20u64).map(|n| PalwBondKeyV2(bond_outpoint(n))).collect();
    let policy = adr0130_economy(0);

    let mut lottery_a = 0usize;
    let mut lottery_b = 0usize;
    let mut legacy_a = 0usize;
    let mut legacy_b = 0usize;
    const DRAWS: u64 = 400;
    for i in 0..DRAWS {
        let anchor = BlockHash::from_u64_word(0xC000 + i);
        let seats = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, policy).expect("a full panel");
        let lean_seats =
            derive_panel_v2_with_policy(&lean, &params, &claim_id, anchor, 100, None, false, policy).expect("a full panel");
        assert_eq!(
            seats.iter().map(|s| s.operator_id).collect::<Vec<_>>(),
            lean_seats.iter().map(|s| s.operator_id).collect::<Vec<_>>(),
            "anchor {i}: the operators seated, in order, do not depend on how many bonds A holds"
        );
        let operators: std::collections::BTreeSet<Hash64> = seats.iter().map(|s| s.operator_id).collect();
        assert_eq!(operators.len(), 3, "one seat an operator");
        if let Some(seat) = seats.iter().find(|s| s.operator_id == a) {
            // A sits with the one of its bonds whose bond ticket is lowest.
            let best = a_bonds.iter().min_by_key(|bond| (palw_panel_seat_ticket_v1(anchor, &claim_id, bond), **bond)).unwrap();
            assert_eq!(seat.bond, *best, "anchor {i}: an operator sits with its lowest-ticket eligible bond");
            lottery_a += 1;
        }
        lottery_b += usize::from(operators.contains(&b));
        // The same registry below the economy: one ticket a BOND.
        let legacy =
            derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, PalwPanelDrawPolicyV1::default())
                .expect("a full legacy panel");
        legacy_a += usize::from(legacy.iter().any(|s| s.operator_id == a));
        legacy_b += usize::from(legacy.iter().any(|s| s.operator_id == b));
    }
    // Four operators, three seats: each is seated three draws in four.
    for (who, count) in [("A (ten bonds)", lottery_a), ("B (one bond)", lottery_b)] {
        assert!((240..=360).contains(&count), "{who} seated {count}/{DRAWS} under the operator lottery; expected about 300");
    }
    assert!(lottery_a.abs_diff(lottery_b) <= 60, "equal odds: A {lottery_a}, B {lottery_b} of {DRAWS}");
    assert!(
        legacy_a >= 390 && legacy_b <= 330,
        "below the fence ten tickets all but guaranteed A a seat: A {legacy_a}, B {legacy_b} of {DRAWS}"
    );

    // Over many CLAIM ids, straight through the lottery over one eligible list.
    let eligible = palw_panel_eligible_bonds_v2(&state, &claim_id, 100, None, false, None, policy.economy, params.seat_count).unwrap();
    assert_eq!(eligible.len(), 13, "ten of A's bonds and one each of B, C and D");
    let anchor = BlockHash::from_u64_word(0xD00D);
    let (mut by_claim_a, mut by_claim_b) = (0usize, 0usize);
    for word in 0..DRAWS {
        let claim = h64(0xE000_0000 + word);
        let entries = palw_panel_operator_entries_v1(&claim, anchor, &eligible);
        assert_eq!(entries.len(), 4, "one entry an operator, never one a bond");
        let entry_a = entries.iter().find(|e| e.operator_id == a).unwrap();
        assert_eq!(entry_a.operator_ticket, palw_panel_operator_ticket_v1(anchor, &claim, &a), "no bond enters the ticket");
        let seats = palw_panel_operator_lottery_v1(&params, &claim, anchor, &eligible).unwrap();
        by_claim_a += usize::from(seats.iter().any(|s| s.operator_id == a));
        by_claim_b += usize::from(seats.iter().any(|s| s.operator_id == b));
    }
    assert!((240..=360).contains(&by_claim_a) && (240..=360).contains(&by_claim_b), "A {by_claim_a}, B {by_claim_b} of {DRAWS}");
    assert!(by_claim_a.abs_diff(by_claim_b) <= 60, "equal odds over claim ids: A {by_claim_a}, B {by_claim_b}");

    // The lottery refuses a short jury by name, as the per-bond draw did.
    let three: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> =
        eligible.iter().copied().filter(|(_, bond)| bond.operator_id == a || bond.operator_id == b).collect();
    assert_eq!(
        palw_panel_operator_lottery_v1(&params, &claim_id, anchor, &three),
        Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 3, available: 2 }),
        "eleven bonds under two operators seat nobody on a three-seat panel"
    );
}

/// **ADR-0130: below the panel economy the draw is the per-bond sortition, byte for byte** — the
/// bond ticket's spelling moved into one function, and the legacy draw is still "sort every
/// eligible bond by it, first bond of each operator sits". And past the economy the acceptance
/// layer refuses the per-bond panel wherever the lottery deals a different one.
#[test]
fn adr0130_below_the_economy_the_draw_is_the_per_bond_sortition_and_past_it_the_per_bond_panel_is_refused() {
    let (state, claim_id) = populated_state();
    let params = panel_params();
    let sp = state_params();
    let mc = sp.min_collateral_sompi();
    // The legacy ticket, spelled out by hand.
    let bond = PalwBondKeyV2(bond_outpoint(2));
    let anchor = BlockHash::from_u64_word(0x5EA7);
    let mut by_hand = keyed(PALW_PANEL_V2_DOMAIN_SEAT_TICKET);
    by_hand.update(anchor.as_byte_slice());
    by_hand.update(claim_id.as_byte_slice());
    by_hand.update(&borsh::to_vec(&bond).unwrap());
    assert_eq!(palw_panel_seat_ticket_v1(anchor, &claim_id, &bond), finish(by_hand), "the bond ticket is the one it always was");

    let mut refused_somewhere = false;
    for i in 0..40u64 {
        let anchor = BlockHash::from_u64_word(0xF000 + i);
        let legacy = derive_panel_v2(&state, &params, &claim_id, anchor, mc).unwrap();
        let eligible = palw_panel_eligible_bonds_v2(&state, &claim_id, mc, None, false, None, None, params.seat_count).unwrap();
        let mut ranked: Vec<(Hash64, PalwBondKeyV2, Hash64)> =
            eligible.iter().map(|(k, b)| (palw_panel_seat_ticket_v1(anchor, &claim_id, k), **k, b.operator_id)).collect();
        ranked.sort();
        let mut expected: Vec<PalwPanelSeatV2> = Vec::new();
        for (_, bond, operator_id) in ranked {
            if expected.len() < params.seat_count as usize && !expected.iter().any(|s| s.operator_id == operator_id) {
                expected.push(PalwPanelSeatV2 { bond, operator_id });
            }
        }
        assert_eq!(legacy, expected, "anchor {i}: the per-bond sortition, unchanged");

        // Past the economy (no floor), the same registry and anchor.
        let economy = PalwPanelDrawPolicyV1 {
            weighted: false,
            readiness: None,
            economy: Some(crate::palw_panel_economy_v1::PalwSeatEconomyV1 {
                panel_floor_sompi: mc,
                max_exposure_ratio_permille: 1000,
                reward_multiple_permille: 0,
            }),
            independence: None,
            valid_lock: None,
            stake: None,
        };
        let lottery = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, mc, None, false, economy).unwrap();
        let fact = PalwAnchorFactV2 { anchor_block: anchor, anchor_daa: 105, predecessor_daa: 104, anchor_execution: None };
        let validate = |seats: &[PalwPanelSeatV2], policy| {
            validate_panel_bound_v2_with_policy(
                &state,
                &params,
                &sp,
                &ctx(3, 106, 3),
                &claim_id,
                &fact,
                anchor,
                seats,
                None,
                false,
                policy,
                None,
            )
        };
        assert_eq!(validate(&lottery, economy), Ok(()), "anchor {i}: derive and validate agree past the economy");
        assert_eq!(validate(&legacy, PalwPanelDrawPolicyV1::default()), Ok(()), "…and below it");
        if lottery != legacy {
            refused_somewhere = true;
            assert_eq!(validate(&legacy, economy), Err(PalwPanelV2Error::PanelMismatch), "anchor {i}: the per-bond panel is refused");
            assert_eq!(validate(&lottery, PalwPanelDrawPolicyV1::default()), Err(PalwPanelV2Error::PanelMismatch));
        }
    }
    assert!(refused_somewhere, "the two draws must differ somewhere, or the refusal above is vacuous");
}

/// **The 2026-09-23 route-matrix audit's #3: a bond that cannot post the bind's Valid lock is
/// not drawn, and the panel the draw names instead is one the bind takes.**
///
/// Bond 4 clears the registry floor and backs the claim-priced stake, but its whole collateral
/// is one sompi short of what one `Valid` signature on the claim must lock. Drawn blind to the
/// lock, it lands on the panel for some anchors, and past the audit fence that `PanelBound` is
/// inert — the claim stays `Provisional` until its bind window voids it (the audit's probe: one
/// such bond blocked 17 of 20 floor claims). With the lock in the policy it is never drawn, the
/// panel is always bonds 2, 3 and 5, and the fold binds it.
#[test]
fn route_matrix_3_a_bond_that_cannot_post_the_valid_lock_is_not_drawn() {
    use crate::palw_state_v2::{PalwClaimPhaseV2, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras};
    let sp = state_params();
    let extras = PalwTransitionExtrasV1 { objective_offence_daa: Some(0), audit_2026_09_23_active: true, ..Default::default() };
    let registry = |cheap: u64| {
        vec![
            adr0130_bond(1, 7, 0x21, 1_000_000), // executor — excluded from its own panel
            adr0130_bond(2, 8, 0x22, 1_000_000),
            adr0130_bond(3, 9, 0x23, 1_000_000),
            adr0130_bond(4, 10, 0x24, cheap),
            adr0130_bond(5, 11, 0x25, 1_000_000),
        ]
    };
    // A 4,000-pwu attempt, so its lock sits well above the registry's 100-sompi floor.
    let with_claim = |bonds: Vec<PalwConsensusObjectV2>| {
        let mut objects = vec![adr0130_class()];
        objects.extend(bonds);
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(4_000, 1);
        let (s2, _) = apply_palw_transition_v2(&s1, &sp, &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        (s2, attempt_id_v2(&env.attempt))
    };
    // The lock is a function of the claim alone, so price it once on an ample registry.
    let (probe, probe_claim) = with_claim(registry(1_000_000));
    let required = crate::palw_state_v2::palw_panel_valid_lock_required_v1(&probe, &sp, &extras, probe.claim(&probe_claim).unwrap());
    assert!(required > 101, "the lock ({required}) sits above the registry floor, or this test proves nothing");
    let (state, claim_id) = with_claim(registry(required as u64 - 1));
    assert_eq!(claim_id, probe_claim);
    let lock =
        PalwPanelValidLockV1 { required, now_daa: 103, settled_anchor_depth: None, window_court: sp.window_court(), rcore: None };
    let cheap = PalwBondKeyV2(bond_outpoint(4));
    assert!(!lock.admits(&state, &cheap) && lock.admits(&state, &PalwBondKeyV2(bond_outpoint(2))));

    let params = panel_params(); // seat_count 3
    let with_lock = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..Default::default() };
    let bind = |seats: &[PalwPanelSeatV2]| {
        let (next, _) = apply_palw_transition_v2_with_extras(
            &state,
            &sp,
            &ctx(3, 103, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: BlockHash::from_u64_word(0x77), seats: seats.to_vec() }],
            None,
            false,
            false,
            false,
            false,
            &extras,
        )
        .expect("an ineligible panel is inert past the fence, never the block's error");
        matches!(next.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. })
    };
    let mut blind_drew_the_cheap_bond = false;
    for i in 0..24u64 {
        let anchor = BlockHash::from_u64_word(0x3A00 + i);
        let blind =
            derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, PalwPanelDrawPolicyV1::default())
                .unwrap();
        if blind.iter().any(|seat| seat.bond == cheap) {
            blind_drew_the_cheap_bond = true;
            assert!(!bind(&blind), "anchor {i}: the bind refuses the seat that cannot post its lock");
        }
        let seats = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, with_lock).unwrap();
        let mut sat: Vec<PalwBondKeyV2> = seats.iter().map(|seat| seat.bond).collect();
        sat.sort();
        let mut backed: Vec<PalwBondKeyV2> = [2u64, 3, 5].iter().map(|n| PalwBondKeyV2(bond_outpoint(*n))).collect();
        backed.sort();
        assert_eq!(sat, backed, "anchor {i}: only bonds that can post the lock sit");
        assert!(bind(&seats), "anchor {i}: and the bind takes that panel");
    }
    assert!(blind_drew_the_cheap_bond, "the blind draw must seat bond 4 somewhere, or the refusal above is vacuous");
}

/// **ADR-0130: a bond is drawn only while its free collateral can reserve the floor.** The claim
/// escrows 62,000 (620 ‰ of a 100,000 subsidy), so a seat of a three-seat panel is paid at most
/// 12,400 / 3 = 4,133; its claim-priced stake is 3 × 200 = 600. Under a 500 ‰ ceiling a 2,000-sompi
/// bond backs 1,000 — enough for 600, not for the floor — and a 10,000-sompi bond backs 5,000.
#[test]
fn adr0130_a_bond_that_cannot_reserve_the_floor_is_not_drawn_and_too_few_operators_refuse() {
    let sp = state_params().with_fp_exposure_ceiling(500).unwrap().with_worker_carve_permille(620).unwrap();
    let bonds = vec![
        adr0130_bond(1, 7, 0x21, 1_000_000), // executor — excluded from its own panel
        adr0130_bond(2, 8, 0x22, 2_000),     // backs 1,000: the stake, never the floor
        adr0130_bond(3, 9, 0x23, 2_000),
        adr0130_bond(4, 10, 0x24, 10_000), // backs 5,000
        adr0130_bond(5, 11, 0x25, 10_000),
        adr0130_bond(6, 12, 0x26, 9_000), // backs 4,500
    ];
    let (state, claim_id) = adr0130_state(&sp, bonds, 100_000);
    let claim = state.claim(&claim_id).unwrap();
    assert_eq!((claim.escrowed_reward, claim.reserved), (62_000, 200));
    let params = panel_params(); // seat_count 3
    let seat = |lambda| crate::palw_panel_economy_v1::palw_panel_seat_exposure_v1(200, 62_000, 3, lambda);
    assert_eq!((seat(0), seat(1_000), seat(1_100), seat(2_000)), (600, 4_133, 4_546, 8_266));

    let eligible = |lambda| -> Vec<u64> {
        let mut out: Vec<u64> = palw_panel_eligible_bonds_v2(
            &state,
            &claim_id,
            100,
            None,
            false,
            None,
            adr0130_economy(lambda).economy,
            params.seat_count,
        )
        .unwrap()
        .into_iter()
        .map(|(key, _)| (2..=6u64).find(|n| *key == PalwBondKeyV2(bond_outpoint(*n))).unwrap())
        .collect();
        out.sort();
        out
    };
    assert_eq!(eligible(0), vec![2, 3, 4, 5, 6], "no floor: every bond backs the claim-priced stake");
    assert_eq!(eligible(1_000), vec![4, 5, 6], "λ = 1: the 2,000-sompi bonds cannot reserve 4,133");
    assert_eq!(eligible(1_100), vec![4, 5], "λ = 1.1: nor can the 9,000-sompi bond reserve 4,546");
    assert!(eligible(2_000).is_empty(), "λ = 2: nobody here can reserve 8,266");

    for i in 0..20u64 {
        let anchor = BlockHash::from_u64_word(0xA130 + i);
        let seats = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, adr0130_economy(1_000))
            .expect("three operators can reserve the floor");
        let mut sat: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.bond).collect();
        sat.sort();
        let mut thick: Vec<PalwBondKeyV2> = (4..=6u64).map(|n| PalwBondKeyV2(bond_outpoint(n))).collect();
        thick.sort();
        assert_eq!(sat, thick, "anchor {i}: only the bonds that can reserve the floor sit");
        // The acceptance layer recomputes the same panel under the same floor, and refuses it
        // under none wherever the two differ.
        let fact = PalwAnchorFactV2 { anchor_block: anchor, anchor_daa: 105, predecessor_daa: 104, anchor_execution: None };
        let validate = |policy| {
            validate_panel_bound_v2_with_policy(
                &state,
                &params,
                &sp,
                &ctx(3, 106, 3),
                &claim_id,
                &fact,
                anchor,
                &seats,
                None,
                false,
                policy,
                None,
            )
        };
        assert_eq!(validate(adr0130_economy(1_000)), Ok(()), "anchor {i}: derive and validate agree");
        let unfloored = derive_panel_v2_with_policy(&state, &params, &claim_id, anchor, 100, None, false, adr0130_economy(0)).unwrap();
        if unfloored != seats {
            assert_eq!(validate(adr0130_economy(0)), Err(PalwPanelV2Error::PanelMismatch));
        }
    }
    for (lambda, available) in [(1_100u32, 2u16), (2_000, 0)] {
        assert_eq!(
            derive_panel_v2_with_policy(
                &state,
                &params,
                &claim_id,
                BlockHash::from_u64_word(0xA130),
                100,
                None,
                false,
                adr0130_economy(lambda)
            ),
            Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 3, available }),
            "λ = {lambda} ‰: too few operators can reserve the floor, and a short jury is refused"
        );
    }
}

/// **D2's fast path must never skip a panel that could be majority-new.**
///
/// The gate scans deltas only when this says so, so a `false` that should have been `true` is a
/// rule that silently does not apply. Checked exhaustively over every legal `(seat_count,
/// quorum)` this ruleset admits, against the definition it is a shortcut for: "some panel could
/// seat `quorum` bonds that are all new".
#[test]
fn the_provenance_fast_path_never_skips_a_reachable_quorum() {
    for seats in 1u16..=16 {
        for quorum in 1u16..=seats {
            let Ok(params) = PalwPanelParamsV2::new(seats, quorum, 4) else { continue };
            for minted in 0usize..=(seats as usize + 2) {
                // The thing the scan looks for: a panel of `seats` seats holding `quorum` or
                // more bonds drawn from the `minted` new ones. It is reachable exactly when
                // there are at least `quorum` of them.
                let reachable = minted >= quorum as usize;
                let scans = palw_minted_seats_can_reach_quorum_v1(minted, &params);
                assert!(
                    !reachable || scans,
                    "seats {seats} quorum {quorum} minted {minted}: a quorum of new seats is reachable and the gate would not look"
                );
            }
        }
    }
}

/// And on the shipped shape it is the number the design names, so a change to either half of
/// the panel is visible here rather than only in a comment.
#[test]
fn the_shipped_panel_tolerates_two_minted_bonds_without_scanning() {
    let shipped =
        PalwPanelParamsV2::new(crate::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS, crate::palw_fp_devnet_v3::PALW_V2_PANEL_QUORUM, 4)
            .expect("the shipped panel shape is legal");
    assert!(!palw_minted_seats_can_reach_quorum_v1(2, &shipped), "two new bonds cannot be a majority of five seats");
    assert!(palw_minted_seats_can_reach_quorum_v1(3, &shipped), "three can, so the gate looks");
}

#[test]
fn params_refuse_shapes_that_cannot_judge() {
    assert!(PalwPanelParamsV2::new(0, 1, 1).is_err(), "zero seats");
    assert!(PalwPanelParamsV2::new(3, 0, 1).is_err(), "zero quorum");
    assert!(PalwPanelParamsV2::new(3, 4, 1).is_err(), "quorum above seats");
    // Audit C5: a non-majority quorum lets Valid and Unavailable both form on one panel.
    assert!(PalwPanelParamsV2::new(4, 2, 1).is_err(), "2 of 4 is not a majority — both verdicts could reach it");
    assert!(PalwPanelParamsV2::new(3, 1, 1).is_err(), "1 of 3 lets one seat void an honest claim");
    assert!(PalwPanelParamsV2::new(4, 3, 1).is_ok(), "3 of 4 is a strict majority");
    assert!(PalwPanelParamsV2::new(1, 1, 1).is_ok(), "a single-seat panel is degenerate but self-consistent");
    assert!(PalwPanelParamsV2::new(3, 3, 0).is_err(), "zero anchor delay");
    let p = PalwPanelParamsV2::new(3, 2, 4).unwrap();
    assert!(p.validate_against_state_params(&state_params()).is_ok(), "delay 4 inside bind window 10");
    let too_late = PalwPanelParamsV2::new(3, 2, 10).unwrap();
    assert!(too_late.validate_against_state_params(&state_params()).is_err(), "anchor at the deadline binds nothing");
}

/// **A seat that cannot judge is not charged for saying so — except on the floor.**
///
/// Sortition never asked which classes a node can execute, so a seat routinely landed on a
/// family it did not hold. With two verdicts it had no honest move: `Valid` is a lie,
/// `Unavailable` is a signed accusation against a producer that did nothing wrong, and silence
/// is charged as a no-show. Every road ended in a slash for the offence of being picked.
///
/// The escape has to be closed on the liveness floor, or a quorum of seats could plead their
/// way out of judging the one class the chain is guaranteed to have.
#[test]
fn a_seat_may_plead_incapable_except_on_the_floor() {
    use crate::palw_state_v2::{PalwSeatAnswerV2, palw_seat_may_plead_incapable_v2, palw_seat_verdicts_of_v2};
    let floor = h64(0xF100);
    assert!(!palw_seat_may_plead_incapable_v2(floor, floor), "no node may claim it cannot run BASE-0");
    assert!(palw_seat_may_plead_incapable_v2(h64(0xC1), floor), "an entrant class is a different matter");

    // And the plea reduces to an answer that takes no side, so the transition can tell it
    // apart from both a vote and a no-show.
    let receipts = vec![
        PalwSeatReceiptV2 {
            claim: h64(1),
            seat_bond: PalwBondKeyV2(bond_outpoint(2)),
            verdict: PalwReceiptVerdictV2::Incapable,
            signed_daa: 1,
            signature: Vec::new(),
        },
        PalwSeatReceiptV2 {
            claim: h64(1),
            seat_bond: PalwBondKeyV2(bond_outpoint(3)),
            verdict: PalwReceiptVerdictV2::Valid,
            signed_daa: 1,
            signature: Vec::new(),
        },
    ];
    let answers = palw_seat_verdicts_of_v2(&receipts);
    assert_eq!(answers[0].answer, PalwSeatAnswerV2::Incapable);
    assert_eq!(answers[1].answer, PalwSeatAnswerV2::Served);
}

/// **A bond with nothing left to lose stops being a seat.**
///
/// `Active` is a status, not a balance. `slash_bond` clamps every debit to the collateral that
/// remains, so once a bond reaches zero every further charge is a silent success — and the
/// sortition, which only ever asked for `Active`, went on seating it for the rest of the
/// chain's life. That is a juror who cannot be fined: the one seat a fraud court must not have.
///
/// Asserted on the predicate the draw calls, because the state transition offers no way to
/// hand a test an exhausted bond without running a whole court to produce one.
/// **A seat must be able to run the class it is drawn to judge** (ADR-0071 Decision 3).
///
/// Asserted as a difference: the same registry, the same anchor, the same claim — only the
/// declarations move. A test that checked the positive case alone would pass for a draw that
/// ignored capability entirely, which is the state this Decision found.
#[test]
fn a_bond_that_cannot_run_the_class_is_not_drawn_to_judge_it() {
    let (state, claim_id) = populated_state();
    let anchor = BlockHash::from_u64_word(0xA0C0);

    // Every bond declares the fixture's class: the draw seats, as it always did.
    let full = derive_panel_v2(&state, &panel_params(), &claim_id, anchor, 0).expect("a declaring registry seats");
    assert_eq!(full.len(), 3, "the baseline is the same panel the exclusion test asserts, or this measures nothing");

    // The same registry, the same anchor, the same claim — declaring a class nobody is
    // claiming under. Nothing else moves.
    let (deaf, deaf_claim) = populated_state_declaring(h64(0xDEAD));
    assert_eq!(deaf_claim, claim_id, "the claim is the same one; only the declarations differ");
    let err =
        derive_panel_v2(&deaf, &panel_params(), &claim_id, anchor, 0).expect_err("a registry that cannot run the class seats nobody");
    assert!(
        matches!(err, PalwPanelV2Error::InsufficientEligibleBonds { .. }),
        "and it fails CLOSED — a short panel is not a smaller panel, it is a claim that never binds: {err:?}"
    );
}

#[test]
fn a_bond_with_nothing_left_to_lose_may_not_take_work() {
    use crate::palw_state_v2::{PalwBondStateV2, PalwBondStatusV2, palw_bond_may_take_work_v2};
    let live = PalwBondStateV2 {
        pubkey: vec![1, 2, 3],
        operator_id: h64(0x21),
        collateral: 1_000,
        slashed: 0,
        status: PalwBondStatusV2::Active,
        registered_daa: 0,
        payout_payload: Hash64::default(),
        capable_classes: Default::default(),
    };
    assert!(palw_bond_may_take_work_v2(&live, 1_000), "a fully-collateralised Active bond seats");

    let exhausted = PalwBondStateV2 { collateral: 0, slashed: 1_000, ..live.clone() };
    assert!(matches!(exhausted.status, PalwBondStatusV2::Active), "slashing never changed the status — that is the defect");
    assert!(!palw_bond_may_take_work_v2(&exhausted, 0), "and it must not seat even where the floor is zero");

    let thin = PalwBondStateV2 { collateral: 999, slashed: 1, ..live.clone() };
    assert!(!palw_bond_may_take_work_v2(&thin, 1_000), "a bond that could not register today does not seat today");

    let retiring = PalwBondStateV2 { status: PalwBondStatusV2::Retiring { since_daa: 5, settled_at_since: 0 }, ..live };
    assert!(!palw_bond_may_take_work_v2(&retiring, 0), "and retirement still excludes, collateral or not");
}

// ---- ADR-0071 SA-3: a seat is drawn only after a production fact -------------------------

/// [`populated_state`] with class `h64(1)` registered by a REGISTRANT bond rather than by the
/// genesis assembly, folded past the capability fence.
///
/// The distinction is the whole of SA-3's exemption: a class with `registrant_bond == None` is
/// one a genesis assembly registered, and those stay drawable on a fresh network; a class an
/// entrant bought has to be proven by production. This fixture builds the second kind, so the
/// test can see the rule bite.
fn populated_state_with_a_registrant_class() -> (PalwChainStateV2, Hash64) {
    use crate::palw_state_v2::{PalwBlockWorkV3, PalwClassAdmissionCarriageV2, apply_palw_transition_v6};
    let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
        .expect("the floor's geometry projects");
    let canonical = crate::palw_base0_profile::rc_job_context(&profile, 2, 2);
    // The bonds come first: a registration's carriage names a bond the chain must already hold.
    let bonds = vec![
        register(1, 7, 0x21),
        register(2, 8, 0x22),
        register(3, 9, 0x23),
        register(4, 10, 0x24),
        register(5, 11, 0x24),
        register(6, 12, 0x21),
    ];
    let (s0, ..) = apply_palw_transition_v6(
        &PalwChainStateV2::genesis(),
        &state_params(),
        None,
        &ctx(1, 100, 1),
        &bonds,
        PalwBlockWorkV3::None,
        &[],
        false,
        true,
        false,
        false,
    )
    .expect("the registry loads");
    let class = PalwConsensusObjectV2::ClassRegistered {
        class_id: h64(1),
        artifact_root: h64(11),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
        initial_target: u128::MAX / 2,
        share_permille: 1000,
        activation_daa: 0,
        // The transition reads only `registrant_bond` from the carriage — the graph walk is
        // the acceptance layer's — so this is the cheapest well-formed stand-in.
        admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
            registrant_bond: PalwBondKeyV2(bond_outpoint(1)),
            profile,
            canonical,
            signature: Vec::new(),
        })),
    };
    let (s1, ..) = apply_palw_transition_v6(
        &s0,
        &state_params(),
        None,
        &ctx(2, 101, 2),
        &[class],
        PalwBlockWorkV3::None,
        &[],
        false,
        true,
        false,
        false,
    )
    .expect("an entrant registers the class");
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (s2, ..) = apply_palw_transition_v6(
        &s1,
        &state_params(),
        None,
        &ctx(3, 102, 3),
        &[],
        PalwBlockWorkV3::Attempt(&env),
        &[],
        false,
        true,
        false,
        false,
    )
    .expect("the executor produces on it");
    (s2, claim_id)
}

/// **SA-3: declaring is a claim, producing is a proof.**
///
/// Every bond in the fixture declared the class. Only the executor has ever produced on it,
/// and the executor is excluded from its own panel — so past the fence the draw finds nobody,
/// and it fails CLOSED, which is `derive_panel_v2`'s standing behaviour for a registry it
/// cannot seat. Before the fence the same registry seats a full panel, which is the half that
/// says the rule and not the fixture is what changed.
///
/// Silence is not judged here and that is the point: the five bonds that declared and never
/// produced are not charged, not convicted and not accused. They are simply not drawn.
#[test]
fn a_bond_that_declared_but_never_produced_is_not_drawn_for_a_registrant_class() {
    let (state, claim_id) = populated_state_with_a_registrant_class();
    let anchor = BlockHash::from_u64_word(0xA0C0);

    let before = derive_panel_v2(&state, &panel_params(), &claim_id, anchor, 0).expect("without the fence the registry seats a panel");
    assert_eq!(before.len(), 3, "three eligible seats, as in every other fixture");

    let err = derive_panel_v2_with_capability_proof(&state, &panel_params(), &claim_id, anchor, 0, None, true, false)
        .expect_err("past the fence only a bond that produced on this class may judge it");
    assert!(
        matches!(err, PalwPanelV2Error::InsufficientEligibleBonds { available: 0, .. }),
        "and it fails CLOSED rather than seating a short panel: {err:?}"
    );

    // The executor DID produce — the fact is on the chain — and is excluded for a different
    // reason entirely. Asserting it here keeps the two exclusions from being confused.
    let executor = PalwBondKeyV2(bond_outpoint(1));
    assert!(crate::palw_state_v2::palw_bond_produced_on_class_v1(&state, &executor, &h64(1)), "the producer's own fact was recorded");
    for bond in [2u64, 3, 4, 5, 6] {
        assert!(
            !crate::palw_state_v2::palw_bond_produced_on_class_v1(&state, &PalwBondKeyV2(bond_outpoint(bond)), &h64(1)),
            "and nobody else's was"
        );
    }
}

/// **The liveness proof the fence cannot ship without.**
///
/// A short draw is `InsufficientEligibleBonds`, so every claim then voids at `BindTimeout`
/// with its escrow burned — and the shipped genesis registry has ZERO slack by construction
/// (`seat_count + 1` bonds, executor excluded) with no bond having produced anything at block
/// 0. What keeps it drawable past the fence is the genesis-class exemption, and this asserts
/// it over the REAL shipped assembly rather than a fixture: for every class the card
/// registers, the fence removes exactly nobody, and what is left is still enough operators to
/// seat a panel after excluding any one of them as the executor.
#[test]
fn the_shipped_registry_still_draws_a_full_panel_with_the_capability_fence_armed() {
    use crate::config::params::palw_rc_shipped_params;
    use crate::palw_mode_v2::PalwConsensusMode;
    use crate::palw_state_v2::{palw_bond_may_judge_class_v3, palw_bond_may_take_work_v2};

    let params = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("the shipped RC preset carries a V2 bundle");
    };
    let sp = bundle.state.clone();
    let genesis_ctx =
        PalwBlockContextV2 { block: params.genesis.hash, daa_score: params.genesis.daa_score, blue_score: 0, subsidy: 0 };
    let (booted, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &genesis_ctx, &bundle.genesis_objects, None)
        .expect("the shipped genesis applies");

    let seats_needed = bundle.panel.seat_count() as usize;
    let min_collateral = sp.min_collateral_sompi();
    let classes: Vec<Hash64> = booted.classes_iter().map(|(id, _)| *id).collect();
    assert!(!classes.is_empty(), "the shipped card registers classes");

    for class_id in classes {
        let count = |proof: bool| -> usize {
            let mut operators: Vec<Hash64> = booted
                .bonds_iter()
                .filter(|(key, bond)| {
                    palw_bond_may_take_work_v2(bond, min_collateral)
                        && palw_bond_may_judge_class_v3(&booted, key, bond, &class_id, proof)
                })
                .map(|(_, bond)| bond.operator_id)
                .collect();
            operators.sort();
            operators.dedup();
            operators.len()
        };
        let open = count(false);
        let fenced = count(true);
        assert_eq!(
            open, fenced,
            "class {class_id:?}: the capability fence removed a seat from the shipped genesis registry — every claim on this \
                 network would void at BindTimeout. The genesis-class exemption is what must keep this equal."
        );
        // The executor is drawn out of this same set, so a panel needs one spare.
        assert!(
            fenced > seats_needed,
            "class {class_id:?}: {fenced} eligible operators past the fence, and a panel of {seats_needed} needs one spare for \
                 the executor it excludes"
        );
    }
}

/// **The audit register's P0-7 exclusion red test.** The trio reads the ONE registry: the
/// executor's bond, its operator (even on a different bond), and its key never seat — and
/// one operator never seats twice.
#[test]
fn palw_v2_executor_excluded_from_own_panel() {
    let (state, claim_id) = populated_state();
    let anchor = BlockHash::from_u64_word(0xA0C0);
    let seats = derive_panel_v2(&state, &panel_params(), &claim_id, anchor, 0).expect("3 eligible seats exist");
    assert_eq!(seats.len(), 3);
    for seat in &seats {
        assert_ne!(seat.bond, PalwBondKeyV2(bond_outpoint(1)), "the executor's bond never seats");
        assert_ne!(seat.bond, PalwBondKeyV2(bond_outpoint(6)), "the executor's OPERATOR never seats, whatever the bond");
        assert_ne!(seat.operator_id, op_id(0x21), "no seat carries the executor's operator id");
    }
    let mut operators: Vec<Hash64> = seats.iter().map(|s| s.operator_id).collect();
    operators.sort();
    operators.dedup();
    assert_eq!(operators.len(), seats.len(), "one operator, one seat");
    // Eligible after exclusions: bonds 2, 3, and ONE of {4, 5} — exactly 3. Which of 4/5 seats
    // is the ticket order's business; that it is exactly one of them is the dedup working.
    assert!(seats.iter().any(|s| s.operator_id == op_id(0x24)), "the shared operator got exactly one of its two bonds seated");
}

/// **ADR-0065 D1 — a bond may not be minted and used in the same breath.**
///
/// The safety claim it serves: a holder of one bond could fork from any point, fold sybil
/// `BondRegistered` objects into the fork's OWN blocks, seat panels from them immediately and
/// grow a private `safe_frontier`. `palw_fork_choice`'s stated invariant — *a fork nobody could
/// see collects no receipts, so it has no frontier* — was false, because the fork could mint
/// its own jurors. With a window, seating them costs the fork the DAA it has to actually
/// advance.
///
/// Both positions on ONE registry, because the rule is only visible as a difference. And the
/// third assertion is the one that matters for liveness: the same floor that excludes the
/// newcomer leaves every older bond exactly where it was — a floor that quietly thinned the
/// existing registry would stop a live chain, since a short draw is no panel at all.
#[test]
fn adr_0065_d1_a_fresh_bond_may_not_take_a_seat_until_it_has_stood() {
    let (state, claim_id) = populated_state();
    // The whole registry stands at DAA 100; one newcomer registers a century later.
    let (late, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(3, 200, 3), &[register(7, 13, 0x27)], None).unwrap();
    let anchor = BlockHash::from_u64_word(0xA1);
    let four = PalwPanelParamsV2::new(4, 3, 4).unwrap();

    // FENCE OFF — today's rule. The newcomer is seatable the instant it is registered, which
    // is exactly the property that makes a private fork's own bonds usable on that fork.
    let seats = derive_panel_v2(&late, &four, &claim_id, anchor, 0).expect("four eligible once the newcomer exists");
    assert!(
        seats.iter().any(|s| s.bond == PalwBondKeyV2(bond_outpoint(7))),
        "without the rule a bond registered a moment ago judges a claim"
    );

    // FENCE ON. Anchor at 250, window 100 ⇒ a seat's bond must date from 150 or earlier.
    let floor = palw_seat_maturity_floor_v1(250, Some(100));
    assert_eq!(floor, Some(150), "the floor is the anchor minus the window, computed in one place");
    assert!(
        matches!(
            derive_panel_v2_with_maturity(&late, &four, &claim_id, anchor, 0, floor),
            Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 4, available: 3 })
        ),
        "the newcomer is not eligible yet, and a short draw is no panel — never a smaller one"
    );

    // …and the rule takes nothing away from the bonds that were already standing: with the
    // floor, the draw is exactly the draw the registry made BEFORE the newcomer existed. (This
    // used to compare against the post-registration draw without the floor, which only agrees
    // when the newcomer happens not to be drawn — a fact about one claim id, not about the
    // rule, and the ADR-0072 attempt-version bump moved the id and exposed it.)
    let unchanged =
        derive_panel_v2_with_maturity(&late, &panel_params(), &claim_id, anchor, 0, floor).expect("the older bonds still seat");
    assert_eq!(unchanged, derive_panel_v2(&state, &panel_params(), &claim_id, anchor, 0).unwrap(), "same seats, same order");
    assert!(unchanged.iter().all(|s| s.bond != PalwBondKeyV2(bond_outpoint(7))), "and none of them is the newcomer");

    // Once the window has actually elapsed the newcomer joins on its own merits.
    let matured = palw_seat_maturity_floor_v1(400, Some(100));
    assert_eq!(matured, Some(300));
    let seats = derive_panel_v2_with_maturity(&late, &four, &claim_id, anchor, 0, matured).expect("the newcomer has now stood");
    assert!(seats.iter().any(|s| s.bond == PalwBondKeyV2(bond_outpoint(7))), "maturity is a delay, not an exclusion");

    // `None` is the rule off, and a window longer than the chain saturates to zero rather than
    // wrapping to `u64::MAX` — which would admit every bond and read as the rule working.
    assert_eq!(palw_seat_maturity_floor_v1(250, None), None);
    assert_eq!(palw_seat_maturity_floor_v1(10, Some(1_000)), Some(0));
}

/// **ADR-0065 D1 is armable on the genesis this build actually ships.**
///
/// `arming_bond_maturity_needs_a_registry_with_a_spare_seat` (params.rs) proves the CONFIG
/// gate accepts the fence on the shipped preset. That is a boot check; it says nothing about
/// whether panels still draw once the rule is in force, and a fence that validates and then
/// starves every draw is the same halt with a friendlier error site.
///
/// So this runs the shipped registry: apply the genesis objects exactly as a booting node
/// does, bind a claim under one of the real cards, and draw with the maturity floor on.
///
/// The third position is the one the registry grew for. D1's guard exists because a seat
/// LEAVING is what makes a matured registry thin: the replacement is itself immature for a
/// whole window, so for that window the chain runs one bond short. At `seat_count + 1` the
/// departure alone is fatal — the remaining bonds cannot fill the panel once the executor is
/// excluded. At `seat_count + 3` it is absorbed twice over. Both are asserted here, on one
/// registry, because the margin is only visible as a difference.
#[test]
fn the_shipped_registry_draws_under_an_armed_maturity_fence_even_after_a_seat_leaves() {
    use crate::config::params::palw_rc_shipped_params;
    use crate::palw_mode_v2::PalwConsensusMode;

    let params = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("the shipped RC preset carries a V2 bundle");
    };
    let sp = bundle.state.clone();

    // 1. Boot the genesis registry the way a node does — through the transition, not by
    //    reading the card. A registry that parses and does not apply is the failure this
    //    whole gate line keeps finding.
    let genesis_ctx =
        PalwBlockContextV2 { block: params.genesis.hash, daa_score: params.genesis.daa_score, blue_score: 0, subsidy: 0 };
    let (booted, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &genesis_ctx, &bundle.genesis_objects, None)
        .expect("the shipped genesis registrations apply");

    let cards: Vec<_> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, .. } => {
                Some((*bond, pubkey.clone(), operator_pubkey.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        cards.len(),
        crate::palw_fp_devnet_v3::palw_v2_maturity_armable_bonds_v1(),
        "the shipped registry is the size that can carry the fence"
    );

    // 2. A claim under the FIRST shipped card, so the draw has a real executor to exclude by
    //    bond, operator and key — the three exclusions are what turn `seat_count` seats into
    //    a `seat_count + 1` requirement.
    let (exec_bond, exec_pubkey, exec_operator) = cards[0].clone();
    let class_id = sp.base_class_id();
    let artifact_root = bundle
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id: c, artifact_root, .. } if *c == class_id => Some(*artifact_root),
            _ => None,
        })
        .expect("the floor class is registered at genesis");
    let env = PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h64(0xD0),
            challenge: challenge_v2(h64(0xD0), h64(5), 1_700, 1, class_id, &exec_bond.0),
            class_id,
            executor_bond: exec_bond.0,
            executor_pubkey: exec_pubkey,
            operator_id: crate::palw_state_v2::palw_operator_id_v2(&exec_operator),
            artifact_root,
            trace_root: h64(31),
            output_root: h64(32),
            pwu: 1,
            trace_manifest_root: h64(33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: h64(41),
        },
        signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let claim_id = attempt_id_v2(&env.attempt);
    let bound_daa = params.genesis.daa_score + 1;
    let (live, _) = apply_palw_transition_v2(
        &booted,
        &sp,
        &PalwBlockContextV2 { block: BlockHash::from_u64_word(0xB1), daa_score: bound_daa, blue_score: 1, subsidy: 0 },
        &[],
        Some(&env),
    )
    .expect("a claim binds under a shipped bond");

    // 3. The fence in force, a full window after the genesis registrations. Every shipped card
    //    is mature, so the floor takes nothing away and the panel is the one the rule-off draw
    //    would have produced.
    let anchor = BlockHash::from_u64_word(0xA1);
    let window = 1_000;
    let matured = palw_seat_maturity_floor_v1(params.genesis.daa_score + window + 1, Some(window));
    let armed = derive_panel_v2_with_maturity(&live, &bundle.panel, &claim_id, anchor, 0, matured)
        .expect("the shipped registry seats a panel with the maturity fence armed");
    assert_eq!(armed.len(), bundle.panel.seat_count() as usize, "a full panel, not a short one");
    assert_eq!(
        armed,
        derive_panel_v2(&live, &bundle.panel, &claim_id, anchor, 0).expect("and the same panel with the rule off"),
        "on a matured registry the floor changes no seat"
    );

    // 4. **A seat leaves.** Retire one card that is not the executor's and draw again: the
    //    registry absorbs it. This is the property `seat_count + 3` buys.
    let departing = cards[1].0;
    let (after, _) = apply_palw_transition_v2(
        &live,
        &sp,
        &PalwBlockContextV2 { block: BlockHash::from_u64_word(0xB2), daa_score: bound_daa + 1, blue_score: 2, subsidy: 0 },
        &[PalwConsensusObjectV2::BondRetireRequested { bond: departing, signature: Vec::new() }],
        None,
    )
    .expect("a retire request applies");
    let seats = derive_panel_v2_with_maturity(&after, &bundle.panel, &claim_id, anchor, 0, matured)
        .expect("one seat leaving does not stop the draw");
    assert_eq!(seats.len(), bundle.panel.seat_count() as usize);
    assert!(seats.iter().all(|s| s.bond != departing), "and the departed bond is not on it");

    // 5. The counterfactual, on the same machinery: at the bare `seat_count + 1` the SAME
    //    departure empties the panel. Without this the assertion above would pass on a
    //    registry of any size and prove nothing about the margin.
    let bare = crate::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1();
    let trimmed: Vec<_> = bundle
        .genesis_objects
        .iter()
        .filter({
            let mut kept = 0usize;
            move |o| {
                if matches!(o, PalwConsensusObjectV2::BondRegistered { .. }) {
                    kept += 1;
                    kept <= bare
                } else {
                    true
                }
            }
        })
        .cloned()
        .collect();
    let (small, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &genesis_ctx, &trimmed, None)
        .expect("the trimmed registry applies too");
    let (small_live, _) = apply_palw_transition_v2(
        &small,
        &sp,
        &PalwBlockContextV2 { block: BlockHash::from_u64_word(0xB1), daa_score: bound_daa, blue_score: 1, subsidy: 0 },
        &[],
        Some(&env),
    )
    .unwrap();
    derive_panel_v2_with_maturity(&small_live, &bundle.panel, &claim_id, anchor, 0, matured)
        .expect("at seat_count + 1 the panel still draws while nobody has left");
    let (small_after, _) = apply_palw_transition_v2(
        &small_live,
        &sp,
        &PalwBlockContextV2 { block: BlockHash::from_u64_word(0xB2), daa_score: bound_daa + 1, blue_score: 2, subsidy: 0 },
        &[PalwConsensusObjectV2::BondRetireRequested { bond: departing, signature: Vec::new() }],
        None,
    )
    .unwrap();
    assert!(
        matches!(
            derive_panel_v2_with_maturity(&small_after, &bundle.panel, &claim_id, anchor, 0, matured),
            Err(PalwPanelV2Error::InsufficientEligibleBonds { .. })
        ),
        "at seat_count + 1 the same departure is a halt — which is why the genesis grew"
    );
}

/// **The shortfall counter must agree with the draw, or the warning built on it lies.**
///
/// `palw_seatable_operators_v1` exists to let an operator be told that ADR-0065 D1 has outrun
/// the live registry. A counter that answered differently from `derive_panel_v2_with_maturity`
/// would produce exactly the failure this whole ADR line keeps finding — a diagnostic that
/// reports health while the thing it describes is broken, or cries wolf while it is fine.
///
/// So this asserts the AGREEMENT, not the count: across every maturity floor that matters,
/// "the draw succeeds" and "enough operators remain once the executor's is removed" are the
/// same predicate. Sharing `palw_bond_may_take_work_v2` and the maturity comparison is what
/// makes that true; this is what stops it from silently stopping being true.
#[test]
fn the_seatable_counter_agrees_with_the_draw_it_warns_about() {
    let (state, claim_id) = populated_state();
    // A newcomer a century after the rest, so some floors admit it and some do not.
    let (late, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(3, 200, 3), &[register(7, 13, 0x27)], None).unwrap();
    let anchor = BlockHash::from_u64_word(0xA1);

    // The executor is bond 1 under operator 0x21, and bond 6 shares that operator — so the
    // draw's three exclusions remove ONE operator from the count, never more.
    let executor_operator = op_id(0x21);

    for (anchor_daa, window) in [(250u64, None), (250, Some(100)), (400, Some(100)), (150, Some(1_000)), (10_000, Some(1))] {
        let floor = palw_seat_maturity_floor_v1(anchor_daa, window);
        let counted = palw_seatable_operators_v1(&late, 0, floor);
        // What the draw has left after the executor's own operator is excluded.
        let available = counted.saturating_sub(1);
        for seats in 1u16..=6 {
            let Ok(params) = PalwPanelParamsV2::new(seats, 1, 4) else { continue };
            let drew = derive_panel_v2_with_maturity(&late, &params, &claim_id, anchor, 0, floor);
            assert_eq!(
                drew.is_ok(),
                available >= seats as usize,
                "floor {floor:?}, {seats} seats: the counter says {counted} operators ({available} after the \
                     executor) and the draw says {drew:?} — these must never disagree"
            );
            if let Ok(panel) = drew {
                assert_eq!(panel.len(), seats as usize);
                assert!(panel.iter().all(|s| s.operator_id != executor_operator), "the executor's operator never sits");
            }
        }
    }
}

/// **The rate limiter, including the two arms that were wrong before it was a function.**
///
/// It gates a WARNING, so being wrong is not a consensus fault — it is the difference between
/// an operator hearing why the chain stopped and not hearing it.
#[test]
fn the_shortfall_limiter_reports_at_the_start_and_survives_a_reorg() {
    let interval = 600;
    // **An escalation always speaks**, whatever the interval says. The band only ever worsens
    // by the registry losing another operator, and the interval is a bind window — roughly 20
    // hours at the live cadence — so without this the step from "the margin is gone" to "no
    // claim can bind" would be invisible for most of a day.
    assert!(palw_shortfall_report_is_due_v1(1_000, 1_001, interval, true));
    assert!(palw_shortfall_report_is_due_v1(1_000, 1_000, u64::MAX, true));

    // Cold start reports immediately. The saturating add makes `last + interval` equal
    // `u64::MAX`, so without the sentinel arm no first report would ever be due.
    assert!(palw_shortfall_report_is_due_v1(PALW_SHORTFALL_NEVER_REPORTED, 0, interval, false));
    assert!(palw_shortfall_report_is_due_v1(PALW_SHORTFALL_NEVER_REPORTED, u64::MAX, interval, false));

    // Inside the window, going forward: quiet.
    assert!(!palw_shortfall_report_is_due_v1(1_000, 1_000, interval, false));
    assert!(!palw_shortfall_report_is_due_v1(1_000, 1_599, interval, false));
    // At the boundary and past it: due. Exactly at `last + interval` counts, or the effective
    // interval would silently be one longer than it says.
    assert!(palw_shortfall_report_is_due_v1(1_000, 1_600, interval, false));
    assert!(palw_shortfall_report_is_due_v1(1_000, 9_999, interval, false));

    // **A reorg to a lower score re-opens the report.** This is the arm that matters: the
    // caller runs per chain block added, so a reorg replays scores below the old tip. Without
    // it the node goes quiet from the reorg until the new branch passes `last + interval`.
    assert!(palw_shortfall_report_is_due_v1(10_000, 9_000, interval, false));
    assert!(palw_shortfall_report_is_due_v1(10_000, 9_999, interval, false));
    assert!(!palw_shortfall_report_is_due_v1(10_000, 10_000, interval, false), "the same score is not a reorg");

    // An interval large enough to overflow must not wrap into "always due".
    assert!(!palw_shortfall_report_is_due_v1(1_000, 1_001, u64::MAX, false));
    assert!(palw_shortfall_report_is_due_v1(1_000, 999, u64::MAX, false), "…and a reorg still speaks");
}

/// **The count is the REGISTRY's, and a draw's requirement is claim-relative — so the warning
/// built on it must not speak in absolutes.**
///
/// `palw_seatable_operators_v1` counts operators with at least one eligible, mature bond. The
/// draw then excludes the claim's executor by bond, operator and key. Normally the executor's
/// operator IS one of the counted ones, so a draw has `counted - 1` to choose from and needs
/// `seat_count + 1` counted overall.
///
/// But an executor's bond only has to EXIST for the draw to run, not to be eligible — a
/// producer whose bond retires or is slashed under the floor while its claim is still
/// Provisional is exactly that case. Its operator is then absent from the count, the draw has
/// all `counted` to choose from, and `counted == seat_count` is enough.
///
/// Measured here rather than argued: at three eligible operators a three-seat panel DRAWS,
/// while the `seat_count + 1` threshold the warning alarms on says four are needed. The
/// threshold is still the right alarm — it is the point where claims from healthy bonds start
/// failing — but the message may not say "no panel can be drawn", because this is a panel
/// being drawn. That is why it says "every claim from a still-eligible bond".
#[test]
fn a_claim_whose_own_operator_has_left_can_still_seat_a_panel() {
    let (state, claim_id) = populated_state();
    // Retire BOTH bonds under the executor's operator (0x21 holds bonds 1 and 6), so the
    // executor's bond is present-but-ineligible and its operator has nothing seatable.
    let (retired, _) = apply_palw_transition_v2(
        &state,
        &state_params(),
        &ctx(3, 200, 3),
        &[
            PalwConsensusObjectV2::BondRetireRequested { bond: PalwBondKeyV2(bond_outpoint(1)), signature: Vec::new() },
            PalwConsensusObjectV2::BondRetireRequested { bond: PalwBondKeyV2(bond_outpoint(6)), signature: Vec::new() },
        ],
        None,
    )
    .unwrap();
    let counted = palw_seatable_operators_v1(&retired, 0, None);
    assert_eq!(counted, 3, "operators 0x22, 0x23, 0x24 remain; the executor's 0x21 does not");

    let anchor = BlockHash::from_u64_word(0xA1);
    let params = PalwPanelParamsV2::new(3, 2, 4).unwrap();
    let drew = derive_panel_v2_with_maturity(&retired, &params, &claim_id, anchor, 0, None);
    assert!(drew.is_ok(), "three counted operators fill three seats when none of them is the executor's");
    assert_eq!(counted, params.seat_count() as usize, "…at exactly seat_count, one BELOW the warning's threshold");
    assert!(
        counted < params.seat_count() as usize + 1,
        "so the alarm fires here, and its wording must be true anyway — see the warning in \
             `palw_warn_if_maturity_outruns_the_registry`"
    );

    // **And the band BELOW it is unconditional**, which is what lets the warning say "NO claim
    // can seat a panel" there without qualification: at fewer than `seat_count` eligible
    // operators the draw fails even in this most favourable case, where the executor costs
    // nothing because it is not counted.
    let bigger = PalwPanelParamsV2::new(4, 3, 4).unwrap();
    assert!(counted < bigger.seat_count() as usize, "three counted against four seats");
    assert!(
        matches!(
            derive_panel_v2_with_maturity(&retired, &bigger, &claim_id, anchor, 0, None),
            Err(PalwPanelV2Error::InsufficientEligibleBonds { .. })
        ),
        "below seat_count nothing can seat a panel, whoever the executor is — the three bands are \
             seatable < seat_count (never), == seat_count (only an unseatable executor), > seat_count (always)"
    );
}

/// The two filters the counter applies, each shown to bite on its own.
#[test]
fn the_seatable_counter_dedups_operators_and_respects_maturity() {
    let (state, _) = populated_state();
    // Six bonds over FOUR operators: 0x21 twice (bonds 1 and 6) and 0x24 twice (bonds 4, 5).
    assert_eq!(state.bonds_iter().count(), 6, "six bonds…");
    assert_eq!(palw_seatable_operators_v1(&state, 0, None), 4, "…and four operators, because the draw seats one each");

    // A newcomer under a FRESH operator raises the count once it has matured, and not before.
    let (late, _) = apply_palw_transition_v2(&state, &state_params(), &ctx(3, 200, 3), &[register(7, 13, 0x27)], None).unwrap();
    assert_eq!(palw_seatable_operators_v1(&late, 0, None), 5, "the rule off: it counts immediately");
    assert_eq!(
        palw_seatable_operators_v1(&late, 0, palw_seat_maturity_floor_v1(250, Some(100))),
        4,
        "the rule on and the window unelapsed: the newcomer is not seatable yet"
    );
    assert_eq!(palw_seatable_operators_v1(&late, 0, palw_seat_maturity_floor_v1(400, Some(100))), 5, "…and once it has stood, it is");

    // The collateral floor bites too: raise it above what every bond holds and nobody is
    // seatable, which is the slashed-to-nothing case the shared predicate exists for.
    assert_eq!(palw_seatable_operators_v1(&late, u64::MAX, None), 0, "a floor nobody meets leaves no seats");
}

#[test]
fn the_draw_is_a_function_of_the_anchor_and_fails_closed_when_thin() {
    let (state, claim_id) = populated_state();
    let a = derive_panel_v2(&state, &panel_params(), &claim_id, BlockHash::from_u64_word(0xA1), 0).unwrap();
    let b = derive_panel_v2(&state, &panel_params(), &claim_id, BlockHash::from_u64_word(0xA1), 0).unwrap();
    assert_eq!(a, b, "same anchor, same panel — determinism");

    // Ask for more seats than the registry can seat: refused, never quietly smaller.
    // (3 of 4: the majority the exclusivity invariant now requires.)
    let wide = PalwPanelParamsV2::new(4, 3, 4).unwrap();
    assert!(matches!(
        derive_panel_v2(&state, &wide, &claim_id, BlockHash::from_u64_word(0xA1), 0),
        Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 4, available: 3 })
    ));
}

#[test]
fn panel_bound_acceptance_checks_slot_window_and_exact_seats() {
    let (state, claim_id) = populated_state();
    let p = panel_params();
    let sp = state_params();
    // Claim accepted at daa 101; anchor slot = 105; bind deadline = 111.
    let anchor_block = BlockHash::from_u64_word(0xA0C0);
    let anchor = PalwAnchorFactV2 { anchor_block, anchor_daa: 105, predecessor_daa: 104, anchor_execution: None };
    let seats = derive_panel_v2(&state, &p, &claim_id, anchor_block, 0).unwrap();

    let ok = validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 106, 3), &claim_id, &anchor, anchor_block, &seats);
    assert!(ok.is_ok(), "a conforming binding is accepted: {ok:?}");

    // Anchor before the slot.
    let early = PalwAnchorFactV2 { anchor_block, anchor_daa: 104, predecessor_daa: 103, anchor_execution: None };
    assert!(matches!(
        validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 106, 3), &claim_id, &early, anchor_block, &seats),
        Err(PalwPanelV2Error::AnchorMismatch(_))
    ));
    // Not the FIRST block at the slot.
    let not_first = PalwAnchorFactV2 { anchor_block, anchor_daa: 106, predecessor_daa: 105, anchor_execution: None };
    assert!(matches!(
        validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 107, 3), &claim_id, &not_first, anchor_block, &seats),
        Err(PalwPanelV2Error::AnchorMismatch(_))
    ));
    // Binding before the anchor exists.
    assert!(matches!(
        validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 104, 3), &claim_id, &anchor, anchor_block, &seats),
        Err(PalwPanelV2Error::BindOutsideWindow(_))
    ));
    // Binding past the deadline.
    assert!(matches!(
        validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 112, 3), &claim_id, &anchor, anchor_block, &seats),
        Err(PalwPanelV2Error::BindOutsideWindow(_))
    ));
    // A groomed panel (seats reordered) is not the derived panel.
    let mut reordered = seats.clone();
    reordered.reverse();
    assert!(matches!(
        validate_panel_bound_v2(&state, &p, &sp, &ctx(3, 106, 3), &claim_id, &anchor, anchor_block, &reordered),
        Err(PalwPanelV2Error::PanelMismatch)
    ));
}

/// A claim with its panel bound at DAA 106 — the starting point for every receipt test.
#[allow(clippy::type_complexity)]
fn licensed_fixture() -> (PalwChainStateV2, Hash64, PalwStateParamsV2, PalwPanelParamsV2, Hash64, Vec<PalwPanelSeatV2>, u64) {
    let (state, claim_id) = populated_state();
    let p = panel_params();
    let sp = state_params();
    let anchor_block = BlockHash::from_u64_word(0xA0C0);
    let seats = derive_panel_v2(&state, &p, &claim_id, anchor_block, 0).unwrap();
    let (bound, _) = apply_palw_transition_v2(
        &state,
        &sp,
        &ctx(3, 106, 3),
        &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: anchor_block, seats: seats.clone() }],
        None,
    )
    .unwrap();
    (bound, claim_id, sp, p, h64(999), seats, 106)
}

/// The shipped panel: five seats, quorum three. `populated_state` has only three eligible
/// operators, so the five-seat coverage test registers its own spare operators.
fn five_seat_licensed_fixture() -> (PalwChainStateV2, Hash64, PalwStateParamsV2, PalwPanelParamsV2, Hash64, Vec<PalwPanelSeatV2>) {
    five_seat_licensed_fixture_posting(40, |_| 1_000_000)
}

/// [`five_seat_licensed_fixture`] for a `pwu` claim, each bond posting `collateral(bond)` — what
/// the objective-offence ledger's lock prices are measured against. The draw is unweighted, so
/// the seats and the full seat are the same whatever the bonds post.
#[allow(clippy::type_complexity)]
fn five_seat_licensed_fixture_posting(
    pwu: u64,
    collateral: impl Fn(PalwBondKeyV2) -> u64,
) -> (PalwChainStateV2, Hash64, PalwStateParamsV2, PalwPanelParamsV2, Hash64, Vec<PalwPanelSeatV2>) {
    let register = |bond: u64, pubkey: u8, operator: u64| {
        let mut object = register(bond, pubkey, operator);
        if let PalwConsensusObjectV2::BondRegistered { bond, collateral: posted, .. } = &mut object {
            *posted = collateral(*bond);
        }
        object
    };
    let objects = vec![
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(1),
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        },
        register(1, 7, 0x21),
        register(2, 8, 0x22),
        register(3, 9, 0x23),
        register(4, 10, 0x24),
        register(7, 13, 0x25),
        register(8, 14, 0x26),
        register(9, 15, 0x27),
    ];
    let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
    let env = attempt(pwu, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (state, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
    let p = PalwPanelParamsV2::new(crate::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS, crate::palw_fp_devnet_v3::PALW_V2_PANEL_QUORUM, 4)
        .unwrap();
    let sp = state_params();
    let anchor_block = BlockHash::from_u64_word(0xA0C0);
    let seats = derive_panel_v2(&state, &p, &claim_id, anchor_block, 0).expect("five eligible operators seat five");
    assert_eq!(seats.len(), 5);
    let (bound, _) = apply_palw_transition_v2(
        &state,
        &sp,
        &ctx(3, 106, 3),
        &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: anchor_block, seats: seats.clone() }],
        None,
    )
    .unwrap();
    (bound, claim_id, sp, p, h64(999), seats)
}

/// Receipts: the full path from a bound panel to both quorum outcomes, with every refusal
/// shape on the way.
#[test]
fn receipt_quorum_licenses_and_unavailable_quorum_defaults_the_producer() {
    let (state, claim_id) = populated_state();
    let p = panel_params();
    let sp = state_params();
    let anchor_block = BlockHash::from_u64_word(0xA0C0);
    let seats = derive_panel_v2(&state, &p, &claim_id, anchor_block, 0).unwrap();
    let (bound, _) = apply_palw_transition_v2(
        &state,
        &sp,
        &ctx(3, 106, 3),
        &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: anchor_block, seats: seats.clone() }],
        None,
    )
    .unwrap();

    let net = h64(999);
    // The "signature" fixture: sig = pubkey bytes; the verifier checks exactly that, plus the
    // context (the family's own) and the message (recomputed).
    // The panel bound at daa 106, so the receipt window is [106, 106 + window_receipt].
    const SIGNED_DAA: u64 = 108;
    // An `Unavailable` names the obligation it says went unmet: a chunk the attempt committed
    // to, requested inside the retention window and before the seat signed about it.
    let unavailable = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 107 };
    let sign_as = |seat: &PalwPanelSeatV2, verdict: PalwReceiptVerdictV2| PalwSeatReceiptV2 {
        claim: claim_id,
        verdict,
        seat_bond: seat.bond,
        signed_daa: SIGNED_DAA,
        signature: bound.bond(&seat.bond).unwrap().pubkey.clone(),
    };
    let verify = |key: &[u8], message: &[u8], sig: &[u8], context: &[u8]| {
        assert_eq!(context, PALW_RECEIPT_V2_MLDSA87_CONTEXT, "the family picks its context");
        assert_eq!(message.len(), 64);
        key == sig
    };
    let here = ctx(9, 110, 9);
    let check = |st: &PalwChainStateV2, receipts: &[PalwSeatReceiptV2]| {
        validate_receipt_quorum_v2(st, &p, &sp, &here, net, &claim_id, receipts, verify)
    };

    // Two Valid receipts (quorum 2) license.
    let receipts = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[1], PalwReceiptVerdictV2::Valid)];
    assert_eq!(check(&bound, &receipts), Ok(PalwReceiptQuorumV2::Licensed { valid: 2 }));

    // Two Unavailable receipts justify the producer default — the seats answered.
    let receipts = vec![sign_as(&seats[0], unavailable), sign_as(&seats[1], unavailable)];
    assert_eq!(check(&bound, &receipts), Ok(PalwReceiptQuorumV2::ProducerUnavailable { unavailable: 2 }));

    // **ADR-0065 D4, the same receipt set past the fence.** It is still a well-formed answer —
    // signed, seated, in-window, naming an obligation the producer had — it simply decides
    // nothing, and the true tally is still reported so an operator can see how many seats got
    // nothing. Asserted here, beside the position it replaces, because a rule tested only in
    // its new position cannot show that anything changed.
    assert!(
        matches!(
            validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &receipts, verify, true, None),
            Err(PalwPanelV2Error::NoQuorum { valid: 0, unavailable: 2, needed: 2 })
        ),
        "past the fence an Unavailable quorum licenses nothing, and the count is still visible"
    );
    // And the licensing direction is untouched — D4 removes one verdict's power, not the panel's.
    let served = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[1], PalwReceiptVerdictV2::Valid)];
    assert_eq!(
        validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &served, verify, true, None),
        Ok(PalwReceiptQuorumV2::Licensed { valid: 2 })
    );

    // **A malformed abstention must not kill a licensable set.** The obligation gate refuses
    // an `Unavailable` naming a chunk the attempt never committed to, and refuses the WHOLE
    // receipt set with it — which is right while the verdict is an accusation and wrong once
    // it is not: one seat's bad field would otherwise void a claim three seats verified.
    let bad_chunk = PalwReceiptVerdictV2::Unavailable { chunk_index: u32::MAX, requested_daa: SIGNED_DAA };
    let mixed = vec![
        sign_as(&seats[0], PalwReceiptVerdictV2::Valid),
        sign_as(&seats[1], PalwReceiptVerdictV2::Valid),
        sign_as(&seats[2], bad_chunk),
    ];
    assert!(
        matches!(check(&bound, &mixed), Err(PalwPanelV2Error::UnmetObligationNotProven { .. })),
        "with the fence off it is an accusation, and a contentless one poisons the set"
    );
    assert_eq!(
        validate_receipt_quorum_v2_with_policy(&bound, &p, &sp, &here, net, &claim_id, &mixed, verify, true, None),
        Ok(PalwReceiptQuorumV2::Licensed { valid: 2 }),
        "past the fence it accuses nobody, so it is not checked as an accusation and the claim licenses"
    );

    // A split (1 Valid, 1 Unavailable) is no quorum for either transition.
    let receipts = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[1], unavailable)];
    assert!(matches!(check(&bound, &receipts), Err(PalwPanelV2Error::NoQuorum { valid: 1, unavailable: 1, needed: 2 })));

    // A non-seat cannot vote.
    let outsider = PalwSeatReceiptV2 {
        claim: claim_id,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: PalwBondKeyV2(bond_outpoint(1)), // the executor, who is precisely not a seat
        signed_daa: SIGNED_DAA,
        signature: vec![7; 4],
    };
    assert!(matches!(check(&bound, &[outsider]), Err(PalwPanelV2Error::NotASeat(_))));

    // One seat cannot vote twice.
    let receipts = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid), sign_as(&seats[0], PalwReceiptVerdictV2::Valid)];
    assert!(matches!(check(&bound, &receipts), Err(PalwPanelV2Error::DuplicateSeat(_))));

    // A garbage signature refuses the set.
    let mut forged = sign_as(&seats[0], PalwReceiptVerdictV2::Valid);
    forged.signature = vec![0xFF; 4];
    assert!(matches!(check(&bound, &[forged]), Err(PalwPanelV2Error::ReceiptSignatureInvalid)));

    // Before a panel is bound, no quorum can form (wrong phase).
    let receipts = vec![sign_as(&seats[0], PalwReceiptVerdictV2::Valid)];
    assert!(matches!(check(&state, &receipts), Err(PalwPanelV2Error::WrongPhase { .. })));
}

/// **Audit C5: an `Unavailable` must name an obligation the producer actually had, inside a
/// window it could have discharged.**
///
/// A quorum of `Unavailable` voids an honest producer's claim. It used to be a bare tag: no
/// request, no chunk, no time — an accusation with no content, mintable whenever it suited
/// the accuser. Nothing on-chain can prove a byte went unsent, and this does not pretend to.
/// What it removes is the CONTENTLESS accusation: the receipt must name a chunk the attempt
/// committed to, a request made after the panel existed and before the seat signed about it,
/// and one inside the retention window the producer actually owed.
#[test]
fn adr0133_v2_a_licence_by_coverage_is_v1_for_full_masks_and_needs_two_attestations_a_segment() {
    use crate::palw_verification_v2::{PalwSegmentMaskV2, palw_segment_assignment_v2};
    let (state, claim_id, sp, p, net, seats) = five_seat_licensed_fixture();
    const SIGNED_DAA: u64 = 108;
    let verify = |key: &[u8], _m: &[u8], sig: &[u8], _c: &[u8]| key == sig;
    let here = ctx(9, 130, 9);
    let k = (seats.len() as u16).saturating_sub(1).max(1);
    let sign = |seat: &PalwPanelSeatV2, verdict: PalwReceiptVerdictV2, segments: PalwSegmentMaskV2| PalwSeatReceiptV3 {
        receipt: PalwSeatReceiptV2 {
            claim: claim_id,
            verdict,
            seat_bond: seat.bond,
            signed_daa: SIGNED_DAA,
            signature: state.bond(&seat.bond).unwrap().pubkey.clone(),
        },
        segments,
    };
    let check =
        |r: Vec<PalwSeatReceiptV3>| validate_receipt_coverage_v2(&state, &p, &sp, &here, net, &claim_id, &r, verify, false, None);
    let quorum = p.quorum() as usize;
    assert!(quorum >= 2 && seats.len() > quorum, "the fixture's panel has room for a missing seat");
    let panel = state.panel(&claim_id).unwrap();
    let a = palw_segment_assignment_v2(panel.anchor, claim_id, seats.len() as u16);

    // A full mask on a partial seat is not that seat's assignment.
    let stolen_full = sign(&seats[(a.full_seat as usize + 1) % seats.len()], PalwReceiptVerdictV2::Valid, PalwSegmentMaskV2::full(k));
    assert!(matches!(check(vec![stolen_full]), Err(PalwPanelV2Error::MaskNotAssigned { .. })));

    // The assigned masks of every seat (full + four disjoint partials) license: quorum of 5 ≥ 3
    // and every segment has two attestations (full seat + unique partial).
    let by_duty: Vec<_> = seats.iter().enumerate().map(|(i, s)| sign(s, PalwReceiptVerdictV2::Valid, a.mask_of(i as u16))).collect();
    assert!(matches!(check(by_duty.clone()), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })), "{a:?}");
    // Without the full seat each segment has one partial holder — coverage short, even with 4 Valids.
    let partial_only: Vec<_> = by_duty.iter().enumerate().filter(|(i, _)| *i as u16 != a.full_seat).map(|(_, r)| r.clone()).collect();
    assert!(matches!(check(partial_only), Err(PalwPanelV2Error::CoverageShort { have: 1, need: 2, .. })), "{a:?}");
    // Full seat plus two partials: quorum of 3, but two segments have only the full seat.
    let mut few: Vec<_> = vec![by_duty[a.full_seat as usize].clone()];
    few.extend(by_duty.iter().enumerate().filter(|(i, _)| *i as u16 != a.full_seat).map(|(_, r)| r.clone()).take(2));
    assert_eq!(few.len(), quorum);
    assert!(matches!(check(few), Err(PalwPanelV2Error::CoverageShort { have: 1, need: 2, .. })), "{a:?}");
    // A partial that attests a neighbour's segment is refused.
    let neighbour = (a.full_seat as usize + 1) % seats.len();
    let wrong = sign(
        &seats[neighbour],
        PalwReceiptVerdictV2::Valid,
        PalwSegmentMaskV2::single((a.mask_of(neighbour as u16).0.trailing_zeros() as u16 + 1) % k),
    );
    assert!(matches!(check(vec![wrong]), Err(PalwPanelV2Error::MaskNotAssigned { .. })));

    // A widened mask is refused with the signature: the mask is signed.
    // The mask is signed: widening a receipt from one segment to all changes what the seat signed.
    let m_narrow = palw_receipt_message_v3(net, claim_id, PalwReceiptVerdictV2::Valid, SIGNED_DAA, PalwSegmentMaskV2::single(0));
    let m_wide = palw_receipt_message_v3(net, claim_id, PalwReceiptVerdictV2::Valid, SIGNED_DAA, PalwSegmentMaskV2::full(k));
    assert_ne!(m_narrow, m_wide, "the signed message carries the mask");
    assert_ne!(m_wide, palw_receipt_message_v2(net, claim_id, PalwReceiptVerdictV2::Valid, SIGNED_DAA), "…and is not the V2 message");
    // An outsider and a duplicate are refused as in V1.
    let outsider = PalwSeatReceiptV3 {
        receipt: PalwSeatReceiptV2 {
            claim: claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: PalwBondKeyV2(bond_outpoint(1)),
            signed_daa: SIGNED_DAA,
            signature: vec![7; 4],
        },
        segments: PalwSegmentMaskV2::full(k),
    };
    assert!(matches!(check(vec![outsider]), Err(PalwPanelV2Error::NotASeat(_))));
    assert!(matches!(check(vec![by_duty[0].clone(), by_duty[0].clone()]), Err(PalwPanelV2Error::DuplicateSeat(_))));
}

/// The five-seat fixture with each seat's duty receipt (`Valid`, its assigned mask), the full
/// seat's among them, and the coverage check bound as the processor binds it.
struct LicenceSelectionFixture {
    state: PalwChainStateV2,
    claim_id: Hash64,
    sp: PalwStateParamsV2,
    p: PalwPanelParamsV2,
    net: Hash64,
    seats: Vec<PalwPanelSeatV2>,
    bonds: Vec<PalwBondKeyV2>,
    anchor: Hash64,
    full_seat: usize,
    by_duty: Vec<PalwSeatReceiptV3>,
    /// The objective-offence ledger's height in the fold's extras: `None` prices no lock.
    objective_offence_daa: Option<u64>,
}

impl LicenceSelectionFixture {
    const SIGNED_DAA: u64 = 108;

    fn new() -> Self {
        Self::from_fixture(five_seat_licensed_fixture(), None)
    }

    /// A `pwu` claim whose bonds post `collateral(bond)`, folded with the lock ledger armed from
    /// genesis — testnet-12's `palw_objective_offence` — so each `Valid` must be backed at its
    /// door's price.
    fn posting(pwu: u64, collateral: impl Fn(PalwBondKeyV2) -> u64) -> Self {
        Self::from_fixture(five_seat_licensed_fixture_posting(pwu, collateral), Some(0))
    }

    #[allow(clippy::type_complexity)]
    fn from_fixture(
        fixture: (PalwChainStateV2, Hash64, PalwStateParamsV2, PalwPanelParamsV2, Hash64, Vec<PalwPanelSeatV2>),
        objective_offence_daa: Option<u64>,
    ) -> Self {
        let (state, claim_id, sp, p, net, seats) = fixture;
        let panel = state.panel(&claim_id).unwrap();
        let bonds: Vec<PalwBondKeyV2> = panel.seats.iter().map(|seat| seat.bond).collect();
        let anchor = panel.anchor;
        let a = crate::palw_verification_v2::palw_segment_assignment_v2(anchor, claim_id, seats.len() as u16);
        let by_duty = seats
            .iter()
            .enumerate()
            .map(|(i, seat)| PalwSeatReceiptV3 {
                receipt: PalwSeatReceiptV2 {
                    claim: claim_id,
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: seat.bond,
                    signed_daa: Self::SIGNED_DAA,
                    signature: state.bond(&seat.bond).unwrap().pubkey.clone(),
                },
                segments: a.mask_of(i as u16),
            })
            .collect();
        Self { state, claim_id, sp, p, net, seats, bonds, anchor, full_seat: a.full_seat as usize, by_duty, objective_offence_daa }
    }

    fn check(&self, receipts: &[PalwSeatReceiptV3]) -> PalwCoverageVerdictV2 {
        let verify = |key: &[u8], _m: &[u8], sig: &[u8], _c: &[u8]| key == sig;
        validate_receipt_coverage_v2(
            &self.state,
            &self.p,
            &self.sp,
            &ctx(9, 130, 9),
            self.net,
            &self.claim_id,
            receipts,
            verify,
            false,
            None,
        )
    }

    /// The optimistic door as the acceptance arm asks it.
    fn door(&self, receipts: &[PalwSeatReceiptV3]) -> bool {
        crate::palw_optimistic_licence_v2::palw_optimistic_licence_admits_v2(
            self.anchor,
            self.claim_id,
            &self.bonds,
            receipts,
            &self.check(receipts),
        )
    }

    /// The fold, asked as the processor asks it before it offers a set.
    fn folds(&self, object: PalwConsensusObjectV2, audit: bool) -> bool {
        let extras = crate::palw_state_v2::PalwTransitionExtrasV1 {
            verification_v2_active: true,
            verification_s2_active: true,
            audit_2026_09_23_active: audit,
            objective_offence_daa: self.objective_offence_daa,
            ..Default::default()
        };
        crate::palw_state_v2::palw_v2_object_licenses_claim_v1(
            &self.state,
            &self.sp,
            &ctx(9, 130, 9),
            &object,
            false,
            false,
            false,
            false,
            &extras,
        )
    }

    fn optimistic<F: Fn(&[PalwSeatReceiptV3]) -> bool>(
        &self,
        pool: &[PalwSeatReceiptV3],
        licenses: F,
    ) -> Option<Vec<PalwSeatReceiptV3>> {
        palw_select_optimistic_licence_v2(&self.state, &self.claim_id, None, pool, |r: &[PalwSeatReceiptV3]| self.check(r), licenses)
    }

    fn coverage(&self, pool: &[PalwSeatReceiptV3]) -> Option<Vec<PalwSeatReceiptV3>> {
        self.coverage_with(pool, |_| true)
    }

    fn coverage_with<F: Fn(&[PalwSeatReceiptV3]) -> bool>(
        &self,
        pool: &[PalwSeatReceiptV3],
        licenses: F,
    ) -> Option<Vec<PalwSeatReceiptV3>> {
        palw_select_coverage_licence_v2(pool, |r: &[PalwSeatReceiptV3]| self.check(r), licenses)
    }

    /// `licenses` as the processor binds it for the optimistic door: the fold, asked of the object.
    fn folds_optimistic(&self, audit: bool) -> impl Fn(&[PalwSeatReceiptV3]) -> bool + '_ {
        move |receipts: &[PalwSeatReceiptV3]| {
            self.folds(PalwConsensusObjectV2::OptimisticLicensed { claim: self.claim_id, receipts: receipts.to_vec() }, audit)
        }
    }

    /// …and for the coverage door.
    fn folds_coverage(&self, audit: bool) -> impl Fn(&[PalwSeatReceiptV3]) -> bool + '_ {
        move |receipts: &[PalwSeatReceiptV3]| {
            self.folds(PalwConsensusObjectV2::ReceiptLicensedV2 { claim: self.claim_id, receipts: receipts.to_vec() }, audit)
        }
    }

    fn valid_count(receipts: &[PalwSeatReceiptV3]) -> usize {
        receipts.iter().filter(|r| matches!(r.receipt.verdict, PalwReceiptVerdictV2::Valid)).count()
    }
}

fn permutations(items: &[usize]) -> Vec<Vec<usize>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for (i, first) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, *first);
            out.push(tail);
        }
    }
    out
}

/// **The licence stall, reproduced and now closed** (2026-09-24). The audit's verifier replayed
/// the processor's two greedy loops against this validator, for the pool order a stuck
/// testnet-12 floor claim had, and found the optimistic licence built only when the full-replay
/// seat's `Valid` was among the first two in the pool and the coverage licence never. The loops
/// are gone; this runs the selection that replaced them (`palw_select_*_licence_v2`, which both
/// processor assemblers call) with the full seat at every pool position, and keeps the acceptance
/// facts the fix rests on.
#[test]
fn verifier_greedy_assembler_drops_a_late_full_seat() {
    let f = LicenceSelectionFixture::new();
    let full = f.by_duty[f.full_seat].clone();
    let partials: Vec<_> = f.by_duty.iter().enumerate().filter(|(i, _)| *i != f.full_seat).map(|(_, r)| r.clone()).collect();

    // The acceptance facts. The optimistic arm takes `Ok` or `NoQuorum`, so {full, one partial}
    // and all five are accepted, and three or four `Valid`s are refused as `CoverageShort`.
    let with = |n: usize| {
        let mut v = vec![full.clone()];
        v.extend(partials.iter().take(n).cloned());
        v
    };
    assert!(matches!(f.check(&with(0)), Err(PalwPanelV2Error::NoQuorum { .. })));
    assert!(matches!(f.check(&with(1)), Err(PalwPanelV2Error::NoQuorum { .. })));
    assert!(matches!(f.check(&with(2)), Err(PalwPanelV2Error::CoverageShort { .. })));
    assert!(matches!(f.check(&with(3)), Err(PalwPanelV2Error::CoverageShort { .. })));
    assert!(matches!(f.check(&with(4)), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })));
    assert!(f.door(&with(0)) && f.door(&with(1)) && !f.door(&with(2)) && !f.door(&with(3)) && f.door(&with(4)));

    for pos in 0..5usize {
        // The whole panel filed, the full seat at `pos`: both doors open, each with all five.
        let mut pool = partials.clone();
        pool.insert(pos, full.clone());
        let optimistic = f.optimistic(&pool, |_| true).unwrap_or_else(|| panic!("full seat at {pos}: no optimistic licence"));
        assert!(f.door(&optimistic), "full seat at {pos}: the optimistic set is one the door takes");
        assert_eq!(optimistic.len(), 5, "full seat at {pos}: all five validated, so all five ride");
        let coverage = f.coverage(&pool).unwrap_or_else(|| panic!("full seat at {pos}: no coverage licence"));
        assert!(matches!(f.check(&coverage), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })));

        // d0815709's pool when it stalled: four receipts, the full seat's arriving late. The
        // optimistic licence forms from the full seat and one partial; coverage cannot.
        let mut stalled = partials[..3].to_vec();
        stalled.insert(pos.min(3), full.clone());
        let optimistic = f.optimistic(&stalled, |_| true).unwrap_or_else(|| panic!("full seat at {pos} of four: no licence"));
        assert_eq!(optimistic.len(), 2, "full seat at {pos} of four: the full seat and one partial");
        assert_eq!(optimistic[0], full, "the full seat's Valid is taken first");
        assert!(f.door(&optimistic));
        assert!(f.coverage(&stalled).is_none(), "four receipts cannot cover a four-segment cut twice");
    }
}

/// **The selection offers a licence exactly when some set of the pool is one — and only a set
/// its door accepts and the fold licenses.** Every arrival order of every subset of the five
/// duty receipts (325 pools), against a brute force over every subset of each pool with the
/// acceptance predicates: the optimistic door both as the shared predicate and as the arm's own
/// spelling (`Ok` or `NoQuorum`), and coverage's `Licensed`.
#[test]
fn the_licence_selection_offers_exactly_what_the_doors_accept_in_every_arrival_order() {
    let f = LicenceSelectionFixture::new();
    let quorum = f.p.quorum() as usize;
    let mut pools = 0;
    for mask in 1u32..(1 << 5) {
        let members: Vec<usize> = (0..5).filter(|i| mask & (1 << i) != 0).collect();
        let pick =
            |sub: u32| -> Vec<PalwSeatReceiptV3> { (0..5).filter(|i| sub & (1 << i) != 0).map(|i| f.by_duty[i].clone()).collect() };
        let subsets = || (1u32..(1 << 5)).filter(move |sub| sub & !mask == 0);
        let optimistic_exists = subsets().any(|sub| f.door(&pick(sub)));
        let coverage_exists = subsets().any(|sub| matches!(f.check(&pick(sub)), Ok(PalwReceiptQuorumV2::Licensed { .. })));
        let has_full = members.contains(&f.full_seat);
        assert_eq!(optimistic_exists, has_full, "{members:?}: the full seat's Valid is necessary and sufficient");
        assert_eq!(coverage_exists, members.len() == 5, "{members:?}: coverage needs the whole panel");

        for order in permutations(&members) {
            pools += 1;
            let pool: Vec<PalwSeatReceiptV3> = order.iter().map(|i| f.by_duty[*i].clone()).collect();
            let optimistic = f.optimistic(&pool, |_| true);
            assert_eq!(optimistic.is_some(), optimistic_exists, "{order:?}");
            if let Some(set) = &optimistic {
                assert!(f.door(set), "{order:?}: the shared predicate");
                assert!(
                    matches!(f.check(set), Ok(_) | Err(PalwPanelV2Error::NoQuorum { .. })),
                    "{order:?}: the arm's own spelling of the door"
                );
                let valid = LicenceSelectionFixture::valid_count(set);
                assert!(valid < quorum || valid == 5, "{order:?}: never {valid} Valids short of coverage");
                let expected = if members.len() == 5 { 5 } else { members.len().min(2) };
                assert_eq!(set.len(), expected, "{order:?}");
                assert_eq!(set[0], f.by_duty[f.full_seat], "{order:?}: the full seat first");
                if set.len() == 2 {
                    let first_partial = order.iter().find(|i| **i != f.full_seat).unwrap();
                    assert_eq!(set[1], f.by_duty[*first_partial], "{order:?}: the rider is the first partial to arrive");
                }
                for audit in [false, true] {
                    let object = PalwConsensusObjectV2::OptimisticLicensed { claim: f.claim_id, receipts: set.clone() };
                    assert!(f.folds(object, audit), "{order:?}: the fold licenses it (audit fence {audit})");
                }
            }
            let coverage = f.coverage(&pool);
            assert_eq!(coverage.is_some(), coverage_exists, "{order:?}");
            if let Some(set) = &coverage {
                assert!(matches!(f.check(set), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })), "{order:?}");
                for audit in [false, true] {
                    let object = PalwConsensusObjectV2::ReceiptLicensedV2 { claim: f.claim_id, receipts: set.clone() };
                    assert!(f.folds(object, audit), "{order:?}: the fold licenses it (audit fence {audit})");
                }
            }
        }
    }
    assert_eq!(pools, 325, "5 + 20 + 60 + 120 + 120 arrival orders");
}

/// **A receipt that poisons the set, or repeats a seat, is dropped — it does not take the
/// licence with it.** A forged copy of the full seat's `Valid` ahead of the real one, a partial
/// signed over a neighbour's mask, and a second copy of a partial.
#[test]
fn the_licence_selection_drops_a_poisoned_receipt_and_a_repeated_seat() {
    let f = LicenceSelectionFixture::new();
    let full = f.by_duty[f.full_seat].clone();
    let partials: Vec<_> = f.by_duty.iter().enumerate().filter(|(i, _)| *i != f.full_seat).map(|(_, r)| r.clone()).collect();
    let mut forged = full.clone();
    forged.receipt.signature = vec![0xFF; 4];
    let mut stolen = partials[1].clone();
    stolen.segments = partials[0].segments;
    assert!(matches!(f.check(std::slice::from_ref(&forged)), Err(PalwPanelV2Error::ReceiptSignatureInvalid)));
    assert!(matches!(f.check(std::slice::from_ref(&stolen)), Err(PalwPanelV2Error::MaskNotAssigned { .. })));

    let pool = vec![forged.clone(), stolen.clone(), partials[0].clone(), partials[0].clone(), full.clone()];
    assert_eq!(f.optimistic(&pool, |_| true), Some(vec![full.clone(), partials[0].clone()]));
    assert_eq!(f.coverage(&pool), None);

    let mut whole = pool.clone();
    whole.extend(partials.iter().cloned());
    let optimistic = f.optimistic(&whole, |_| true).expect("the whole panel");
    assert_eq!(optimistic.len(), 5);
    assert!(!optimistic.contains(&forged) && !optimistic.contains(&stolen));
    let coverage = f.coverage(&whole).expect("the whole panel covers");
    assert_eq!(coverage.len(), 5);
    assert!(matches!(f.check(&coverage), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })));
}

/// **A rider the fold will not license does not cost the claim its licence** — a `Valid` whose
/// seat cannot post its lock makes the whole set license nothing. The selection passes over it
/// to the next rider, falls back to the full seat alone when there is none, and offers nothing
/// only when the fold refuses that too.
#[test]
fn the_optimistic_selection_falls_back_past_a_rider_the_fold_refuses() {
    let f = LicenceSelectionFixture::new();
    let full = f.by_duty[f.full_seat].clone();
    let partials: Vec<_> = f.by_duty.iter().enumerate().filter(|(i, _)| *i != f.full_seat).map(|(_, r)| r.clone()).collect();
    let unbacked = partials[0].receipt.seat_bond;
    let fold = |set: &[PalwSeatReceiptV3]| set.iter().all(|r| r.receipt.seat_bond != unbacked);

    // Three `Valid`s (the full seat and two partials), the unbacked seat first to arrive: the
    // next partial rides instead.
    let pool = vec![partials[0].clone(), partials[1].clone(), full.clone()];
    assert_eq!(f.optimistic(&pool, fold), Some(vec![full.clone(), partials[1].clone()]));
    // The unbacked seat the only partial: the full seat alone.
    let pool = vec![partials[0].clone(), full.clone()];
    assert_eq!(f.optimistic(&pool, fold), Some(vec![full.clone()]));
    // The whole panel: all five are refused by the fold, then {full, the unbacked seat}, then
    // {full, the next partial} is taken.
    let mut whole = partials.clone();
    whole.push(full.clone());
    assert_eq!(f.optimistic(&whole, fold), Some(vec![full.clone(), partials[1].clone()]));
    // A rider the fold takes is kept.
    let pool = vec![partials[1].clone(), full.clone()];
    assert_eq!(f.optimistic(&pool, fold), Some(vec![full.clone(), partials[1].clone()]));
    // A full seat the fold refuses (it cannot post the whole gain): nothing is offered, so the
    // collector falls through to the next door.
    assert_eq!(f.optimistic(&whole, |_| false), None);
}

/// **With the lock ledger armed, each `Valid` must be backed at its door's price — and the
/// selection offers exactly what the door and the real fold both take** (the review of the
/// licence-stall fix). testnet-12 arms `palw_objective_offence` at genesis. On a 400-pwu claim
/// the optimistic door prices the full seat at the whole gain and every other `Valid` at the
/// quorum price, and the coverage door prices every `Valid` at the quorum price. Every arrival
/// order of every subset of the five duty receipts, for six postings, is checked against a
/// brute force over every subset of the pool with the door predicates and the fold, on both
/// sides of the audit fence (such a set is inert past it and refused below it, and neither
/// licenses).
///
/// What it pins past the fence: a full seat that can bind but cannot post the whole gain gets
/// no optimistic licence in any arrival order, so its claim licenses through coverage with all
/// five or not at all. A rider that cannot post the quorum price is passed over for the next.
#[test]
fn the_licence_selection_prices_each_valid_at_its_door_with_the_lock_ledger_armed() {
    const PWU: u64 = 400;
    let generous = 1_000_000u64;
    let probe = LicenceSelectionFixture::posting(PWU, |_| generous);
    let claim = probe.state.claim(&probe.claim_id).unwrap();
    let facts = crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim(claim, 5);
    let pre_fence = crate::palw_panel_var_v1::PalwClaimFraudFactsV1::from_claim_pre_2026_09_23(claim, 5);
    let quorum_price = crate::palw_panel_var_v1::palw_panel_seat_required_v1(&facts)
        .max(crate::palw_panel_var_v1::palw_panel_seat_required_v1(&pre_fence));
    let full_price =
        crate::palw_offence_v1::palw_min_slashable_per_colluding_seat_v1(crate::palw_panel_var_v1::palw_max_fraud_gain_v1(&facts), 1);
    // `binds` backs the quorum price and not the whole gain; `floor`, the registry's minimum,
    // backs neither.
    let (binds, floor) = (quorum_price as u64 * 3 / 2, probe.sp.min_collateral_sompi());
    assert!(
        u128::from(floor) < quorum_price && quorum_price < u128::from(binds) && u128::from(binds) < full_price,
        "the postings straddle the prices: floor {floor}, quorum {quorum_price}, binds {binds}, whole gain {full_price}"
    );
    let full = probe.bonds[probe.full_seat];
    let partials: Vec<PalwBondKeyV2> = probe.bonds.iter().copied().filter(|b| *b != full).collect();
    let postings: Vec<(&str, Vec<(PalwBondKeyV2, u64)>)> = vec![
        ("every seat generous", vec![]),
        ("the full seat binds but cannot post the whole gain", vec![(full, binds)]),
        ("the full seat at the floor", vec![(full, floor)]),
        ("one partial at the floor", vec![(partials[0], floor)]),
        ("two partials at the floor", vec![(partials[0], floor), (partials[1], floor)]),
        ("the full seat binds, one partial at the floor", vec![(full, binds), (partials[0], floor)]),
    ];
    for (name, short) in &postings {
        let f = LicenceSelectionFixture::posting(PWU, |bond| short.iter().find(|(b, _)| *b == bond).map_or(generous, |(_, c)| *c));
        assert_eq!((f.bonds.clone(), f.full_seat), (probe.bonds.clone(), probe.full_seat), "{name}: the draw is unweighted");
        for audit in [false, true] {
            let (fold_optimistic, fold_coverage) = (f.folds_optimistic(audit), f.folds_coverage(audit));
            for mask in 1u32..(1 << 5) {
                let members: Vec<usize> = (0..5).filter(|i| mask & (1 << i) != 0).collect();
                let pick = |sub: u32| -> Vec<PalwSeatReceiptV3> {
                    (0..5).filter(|i| sub & (1 << i) != 0).map(|i| f.by_duty[i].clone()).collect()
                };
                let subsets = || (1u32..(1 << 5)).filter(move |sub| sub & !mask == 0);
                let optimistic_exists = subsets().any(|sub| f.door(&pick(sub)) && fold_optimistic(&pick(sub)));
                let coverage_exists = subsets()
                    .any(|sub| matches!(f.check(&pick(sub)), Ok(PalwReceiptQuorumV2::Licensed { .. })) && fold_coverage(&pick(sub)));
                for order in permutations(&members) {
                    let pool: Vec<PalwSeatReceiptV3> = order.iter().map(|i| f.by_duty[*i].clone()).collect();
                    let optimistic = f.optimistic(&pool, &fold_optimistic);
                    assert_eq!(optimistic.is_some(), optimistic_exists, "{name}, audit {audit}, {order:?}");
                    if let Some(set) = &optimistic {
                        assert!(f.door(set) && fold_optimistic(set), "{name}, audit {audit}, {order:?}: door and fold");
                    }
                    let coverage = f.coverage_with(&pool, &fold_coverage);
                    assert_eq!(coverage.is_some(), coverage_exists, "{name}, audit {audit}, {order:?}");
                    if let Some(set) = &coverage {
                        assert!(matches!(f.check(set), Ok(PalwReceiptQuorumV2::Licensed { valid: 5 })) && fold_coverage(set));
                    }
                }
            }
        }
    }

    // The cases by name, past the fence.
    let posting = |short: Vec<(PalwBondKeyV2, u64)>| {
        LicenceSelectionFixture::posting(PWU, move |bond| short.iter().find(|(b, _)| *b == bond).map_or(generous, |(_, c)| *c))
    };
    let receipt_of =
        |f: &LicenceSelectionFixture, bond: PalwBondKeyV2| f.by_duty.iter().find(|r| r.receipt.seat_bond == bond).unwrap().clone();
    // A full seat that binds but cannot post the whole gain: no optimistic licence from any
    // pool, and the whole panel licenses through coverage.
    let f = posting(vec![(full, binds)]);
    let whole: Vec<PalwSeatReceiptV3> = f.by_duty.clone();
    assert_eq!(f.optimistic(&whole, f.folds_optimistic(true)), None);
    assert_eq!(f.optimistic(&whole[..2], f.folds_optimistic(true)), None);
    assert_eq!(f.coverage_with(&whole, f.folds_coverage(true)).map(|set| set.len()), Some(5));
    // Below the fence every door is the quorum price, which the same seat does post.
    assert_eq!(f.optimistic(&whole, f.folds_optimistic(false)).map(|set| set.len()), Some(5));
    // A partial at the floor, first to arrive: the next partial rides; alone beside the full
    // seat, the full seat licenses alone; and coverage, which needs it, does not form.
    let f = posting(vec![(partials[0], floor)]);
    let (fr, p0, p1) = (receipt_of(&f, full), receipt_of(&f, partials[0]), receipt_of(&f, partials[1]));
    assert_eq!(f.optimistic(&[p0.clone(), p1.clone(), fr.clone()], f.folds_optimistic(true)), Some(vec![fr.clone(), p1.clone()]));
    assert_eq!(f.optimistic(&[p0.clone(), fr.clone()], f.folds_optimistic(true)), Some(vec![fr.clone()]));
    assert_eq!(f.coverage_with(&f.by_duty, f.folds_coverage(true)), None);
}

/// **The order is the selection's one policy**: the full seat's `Valid`, the outsider's `Valid`,
/// every other `Valid`, then everything else — each rank in arrival order.
#[test]
fn the_licence_candidate_order_is_full_seat_then_outsider_then_valid_then_the_rest() {
    let f = LicenceSelectionFixture::new();
    let full = f.seats[f.full_seat].bond;
    let others: Vec<PalwBondKeyV2> = f.bonds.iter().copied().filter(|b| *b != full).collect();
    let outsider = others[2];
    let receipt = |seat: PalwBondKeyV2, verdict: PalwReceiptVerdictV2| PalwSeatReceiptV3 {
        receipt: PalwSeatReceiptV2 { claim: f.claim_id, verdict, seat_bond: seat, signed_daa: 108, signature: vec![1] },
        segments: crate::palw_verification_v2::PalwSegmentMaskV2::single(0),
    };
    let unavailable = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 107 };
    let pool = vec![
        receipt(others[0], unavailable),
        receipt(others[1], PalwReceiptVerdictV2::Valid),
        receipt(full, PalwReceiptVerdictV2::Incapable),
        receipt(outsider, PalwReceiptVerdictV2::Valid),
        receipt(others[0], PalwReceiptVerdictV2::Valid),
        receipt(full, PalwReceiptVerdictV2::Valid),
    ];
    let positions = |ordered: Vec<&PalwSeatReceiptV3>| -> Vec<usize> {
        ordered.iter().map(|r| pool.iter().position(|p| std::ptr::eq(p, *r)).unwrap()).collect()
    };
    assert_eq!(positions(palw_licence_candidate_order_v2(&pool, Some(full), Some(outsider))), vec![5, 3, 1, 4, 0, 2]);
    assert_eq!(positions(palw_licence_candidate_order_v2(&pool, Some(full), None)), vec![5, 1, 3, 4, 0, 2]);
    assert_eq!(positions(palw_licence_candidate_order_v2(&pool, None, None)), vec![1, 3, 4, 5, 0, 2]);
}

#[test]
fn an_unavailable_receipt_must_name_an_obligation_the_producer_had() {
    let (state, claim_id, sp, p, net, seats, bound_daa) = licensed_fixture();
    let verify = |key: &[u8], _m: &[u8], sig: &[u8], _c: &[u8]| key == sig;
    let here = ctx(9, 130, 9);
    let claim = state.claim(&claim_id).unwrap();
    let retention = claim.trace_retention_daa;
    let chunks = claim.trace_chunk_count;

    let receipt = |verdict: PalwReceiptVerdictV2, signed_daa: u64| PalwSeatReceiptV2 {
        claim: claim_id,
        verdict,
        seat_bond: seats[0].bond,
        signed_daa,
        signature: state.bond(&seats[0].bond).unwrap().pubkey.clone(),
    };
    let check = |r: Vec<PalwSeatReceiptV2>| validate_receipt_quorum_v2(&state, &p, &sp, &here, net, &claim_id, &r, verify);

    // The well-formed accusation is short of quorum here (one seat of a 3/2 panel), which is
    // the shape we want: it reaches the counting stage rather than being refused.
    let ok = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: bound_daa + 1 };
    assert!(matches!(check(vec![receipt(ok, bound_daa + 2)]), Err(PalwPanelV2Error::NoQuorum { unavailable: 1, .. })));

    // A chunk the attempt never committed to is a demand the producer never owed.
    let bad_chunk = PalwReceiptVerdictV2::Unavailable { chunk_index: chunks, requested_daa: bound_daa + 1 };
    assert!(matches!(check(vec![receipt(bad_chunk, bound_daa + 2)]), Err(PalwPanelV2Error::UnmetObligationNotProven { .. })));

    // A request that predates the panel is not about this panel's duty.
    let early = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: bound_daa - 1 };
    assert!(matches!(check(vec![receipt(early, bound_daa + 2)]), Err(PalwPanelV2Error::UnmetObligationNotProven { .. })));

    // A request the seat had not yet made when it signed about it.
    let ahead = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: bound_daa + 3 };
    assert!(matches!(check(vec![receipt(ahead, bound_daa + 2)]), Err(PalwPanelV2Error::UnmetObligationNotProven { .. })));

    // A request past the retention deadline: the obligation had ended.
    let late = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: retention + 1 };
    assert!(matches!(
        check(vec![receipt(late, retention + 2)]),
        Err(PalwPanelV2Error::UnmetObligationNotProven { .. } | PalwPanelV2Error::ReceiptOutsideWindow { .. })
    ));
}

/// **Audit C5: the receipt duty has a clock.**
///
/// `validate_receipt_quorum_v2` took no block context at all, so nothing bounded WHEN a seat
/// could answer — a receipt could be signed before the panel existed, long after the window
/// closed, or dated into the future.
#[test]
fn a_receipt_outside_its_window_is_not_a_discharge_of_the_duty() {
    let (state, claim_id, sp, p, net, seats, bound_daa) = licensed_fixture();
    let verify = |key: &[u8], _m: &[u8], sig: &[u8], _c: &[u8]| key == sig;
    let receipt = |signed_daa: u64| PalwSeatReceiptV2 {
        claim: claim_id,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: seats[0].bond,
        signed_daa,
        signature: state.bond(&seats[0].bond).unwrap().pubkey.clone(),
    };
    let at = |block_daa: u64, r: Vec<PalwSeatReceiptV2>| {
        validate_receipt_quorum_v2(&state, &p, &sp, &ctx(9, block_daa, 9), net, &claim_id, &r, verify)
    };

    // Inside the window: reaches the counting stage.
    assert!(matches!(at(bound_daa + 5, vec![receipt(bound_daa + 1)]), Err(PalwPanelV2Error::NoQuorum { valid: 1, .. })));
    // Before the panel was bound.
    assert!(matches!(at(bound_daa + 5, vec![receipt(bound_daa - 1)]), Err(PalwPanelV2Error::ReceiptOutsideWindow { .. })));
    // Past the receipt deadline.
    let past = bound_daa + sp.window_receipt() + 1;
    assert!(matches!(at(past + 5, vec![receipt(past)]), Err(PalwPanelV2Error::ReceiptOutsideWindow { .. })));
    // Dated after the block that carries it.
    assert!(matches!(at(bound_daa + 1, vec![receipt(bound_daa + 2)]), Err(PalwPanelV2Error::ReceiptOutsideWindow { .. })));
}

/// The verdicts sign DIFFERENT messages: an `Unavailable` signature cannot be replayed as a
/// `Valid` one — the distinctness that lets a seat report withheld data safely.
///
/// And every field an `Unavailable` carries is inside its message. Signing only the verdict
/// TAG would have let one signature stand behind any chunk index and any request time — the
/// whole content of the accusation swapped under a signature that stayed valid.
#[test]
fn the_two_verdicts_are_two_messages_and_every_field_is_signed() {
    let unavail = PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 107 };
    let m_valid = palw_receipt_message_v2(h64(999), h64(1), PalwReceiptVerdictV2::Valid, 108);
    let m_unavail = palw_receipt_message_v2(h64(999), h64(1), unavail, 108);
    assert_ne!(m_valid, m_unavail);
    assert_ne!(palw_receipt_message_v2(h64(998), h64(1), PalwReceiptVerdictV2::Valid, 108), m_valid, "network binds");
    assert_ne!(palw_receipt_message_v2(h64(999), h64(2), PalwReceiptVerdictV2::Valid, 108), m_valid, "claim binds");
    assert_ne!(palw_receipt_message_v2(h64(999), h64(1), PalwReceiptVerdictV2::Valid, 109), m_valid, "the signing time binds");
    assert_ne!(
        palw_receipt_message_v2(h64(999), h64(1), PalwReceiptVerdictV2::Unavailable { chunk_index: 1, requested_daa: 107 }, 108),
        m_unavail,
        "the accused chunk binds"
    );
    assert_ne!(
        palw_receipt_message_v2(h64(999), h64(1), PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 106 }, 108),
        m_unavail,
        "the request time binds"
    );
}

#[test]
fn the_v2_panel_domains_are_distinct() {
    let mut seen: Vec<&[u8]> = PALW_PANEL_V2_ALL_DOMAINS.to_vec();
    seen.sort();
    let before = seen.len();
    seen.dedup();
    assert_eq!(seen.len(), before);
}

/// **How many operators a class needs to be alive — as arithmetic, not as folklore.**
///
/// The number governs every deployment decision (how many hosts, how much hardware for a
/// family whose seats must hold particular hardware) and it had been stated wrongly twice in
/// one session: once as "5 seats, 5 quorum" and once as "the RC declares no per-class floor".
/// Both are measured here instead.
///
/// The rule that makes it a hard count rather than a target: `derive_panel_v2` **refuses** a
/// short draw. It does not seat four of five and carry on — `quorum` is how many must AGREE,
/// never how many must EXIST. So a class needs its full seat count of eligible operators for
/// any claim to bind at all, and one operator short is not degraded service, it is a chain
/// where every claim voids at `BindTimeout` and every block's worker carve burns.
///
/// Eligibility excludes the executor three ways (bond, operator, key) and seats one bond per
/// operator, so the count is `seat_count + 1` DISTINCT OPERATORS — five extra bonds under one
/// operator buy exactly one seat.
#[test]
fn a_class_needs_seat_count_plus_one_distinct_operators() {
    // Five operators total: one executor + four others, against a 5-seat panel.
    let mut objects = vec![PalwConsensusObjectV2::ClassRegistered {
        class_id: h64(1),
        artifact_root: h64(11),
        slash_value_per_pwu: 1,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(1_000_000),
        initial_target: u128::MAX / 2,
        share_permille: 1000,
        activation_daa: 0,
        admission: None,
    }];
    objects.push(register(1, 7, 0x21)); // the executor
    for k in 0..4u64 {
        objects.push(register(10 + k, 20 + k as u8, 0x30 + k));
    }
    // Plus three MORE bonds under an operator that already has one: extra collateral, zero
    // extra seats. This is the Sybil bound Decision 7 rests on, and it is why the answer is
    // counted in operators.
    for k in 0..3u64 {
        objects.push(register(50 + k, 50 + k as u8, 0x30));
    }
    let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx(1, 100, 1), &objects, None).unwrap();
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let (state, _) = apply_palw_transition_v2(&s1, &state_params(), &ctx(2, 101, 2), &[], Some(&env)).unwrap();
    let anchor = BlockHash::from_u64_word(0xA1);

    // Four eligible OPERATORS (eight eligible bonds) against five seats: refused, and the
    // error reports the operator count rather than the bond count.
    let five = PalwPanelParamsV2::new(5, 3, 4).unwrap();
    match derive_panel_v2(&state, &five, &claim_id, anchor, 0) {
        Err(PalwPanelV2Error::InsufficientEligibleBonds { needed, available }) => {
            assert_eq!(needed, 5);
            assert_eq!(available, 4, "eight bonds under four operators seat four — one per operator");
        }
        other => panic!("a short panel must be refused, not seated short: {other:?}"),
    }

    // The floor `(2, 2)` a class may thin to needs three distinct operators: executor + two.
    let two = PalwPanelParamsV2::new(2, 2, 4).unwrap();
    let seats = derive_panel_v2(&state, &two, &claim_id, anchor, 0).expect("two seats from four eligible operators");
    assert_eq!(seats.len(), 2);
    let mut ops: Vec<Hash64> = seats.iter().map(|s| s.operator_id).collect();
    ops.sort();
    ops.dedup();
    assert_eq!(ops.len(), 2, "one seat per operator");
    assert!(!ops.contains(&op_id(0x21)), "the executor's operator is never seated");
}

// ---- ADR-0100 Decision 4: the stratified draw and a shard's part, from chain state ----

fn shard_extras() -> crate::palw_state_v2::PalwTransitionExtrasV1 {
    crate::palw_state_v2::PalwTransitionExtrasV1 {
        shard_licensing: Some(crate::palw_shard_licensing_v1::PalwShardLicensingParamsV1 { seats_per_shard: 3, quorum_per_shard: 2 }),
        ..Default::default()
    }
}

fn shard_fold(
    parent: &PalwChainStateV2,
    c: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    att: Option<&PalwAttemptEnvelopeV2>,
) -> PalwChainStateV2 {
    crate::palw_state_v2::apply_palw_transition_v2_with_extras(
        parent,
        &state_params(),
        c,
        objects,
        att,
        false,
        false,
        false,
        false,
        &shard_extras(),
    )
    .expect("the fixture folds")
    .0
}

/// Class `h64(1)` registered by bond 9, a two-shard plan, and bonds 2..=8 (distinct operators)
/// holding shards: 2, 3, 4 → shard 0; 5, 6, 7 → shard 1; 8 → both. `drop` bonds declare none.
fn sharded_registry(drop: &[u64]) -> (PalwChainStateV2, Hash64) {
    let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).expect("projects");
    let mut objects: Vec<PalwConsensusObjectV2> = (1..=9).map(|v| register(v, v as u8 + 6, 0x20 + v)).collect();
    objects.push(PalwConsensusObjectV2::ClassRegistered {
        class_id: h64(1),
        artifact_root: h64(11),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
        initial_target: u128::MAX / 2,
        share_permille: 1000,
        activation_daa: 0,
        admission: Some(Box::new(crate::palw_state_v2::PalwClassAdmissionCarriageV2 {
            registrant_bond: PalwBondKeyV2(bond_outpoint(9)),
            canonical: crate::palw_base0_profile::rc_job_context(&profile, 2, 2),
            profile,
            signature: Vec::new(),
        })),
    });
    let s1 = shard_fold(&PalwChainStateV2::genesis(), &ctx(1, 100, 1), &objects, None);
    let mut declarations =
        vec![PalwConsensusObjectV2::ClassShardPlanDeclared { class_id: h64(1), shard_count: 2, signature: vec![1; 4] }];
    for (bond, shards) in [(2u64, vec![0u32]), (3, vec![0]), (4, vec![0]), (5, vec![1]), (6, vec![1]), (7, vec![1]), (8, vec![0, 1])] {
        if drop.contains(&bond) {
            continue;
        }
        declarations.push(PalwConsensusObjectV2::BondShardsDeclared {
            bond: PalwBondKeyV2(bond_outpoint(bond)),
            class_id: h64(1),
            shard_count: 2,
            shards,
            signature: vec![1; 4],
        });
    }
    let s2 = shard_fold(&s1, &ctx(2, 101, 2), &declarations, None);
    let env = attempt(40, 1);
    let claim_id = attempt_id_v2(&env.attempt);
    let s3 = shard_fold(&s2, &ctx(3, 102, 3), &[], Some(&env));
    (s3, claim_id)
}

/// **The stratified draw seats each shard from the bonds that hold it**, one operator a shard,
/// stored shard-major; the binding validator recomputes it only when told the claim is
/// sharded, and a shard nobody can fill refuses the whole draw by name.
#[test]
fn a_stratified_draw_seats_each_shard_from_the_bonds_that_hold_it() {
    let (state, claim_id) = sharded_registry(&[]);
    let anchor_block = BlockHash::from_u64_word(0xA1);
    let seats =
        derive_stratified_panel_v2(&state, &panel_params(), &claim_id, anchor_block, 0, None, false, 2).expect("both shards fill");
    assert_eq!(seats.len(), 6, "two shards × three seats");
    let holds = |bond: &PalwBondKeyV2, shard: u32| state.shards_of_bond(bond, &h64(1)).is_some_and(|list| list.contains(&shard));
    assert!(seats[..3].iter().all(|seat| holds(&seat.bond, 0)), "shard 0's seats hold shard 0");
    assert!(seats[3..].iter().all(|seat| holds(&seat.bond, 1)), "shard 1's seats hold shard 1");
    assert!(seats.iter().all(|seat| seat.bond != PalwBondKeyV2(bond_outpoint(1))), "the executor is never seated");
    assert_eq!(seats, derive_stratified_panel_v2(&state, &panel_params(), &claim_id, anchor_block, 0, None, false, 2).unwrap());

    let anchor = PalwAnchorFactV2 { anchor_block, anchor_daa: 106, predecessor_daa: 105, anchor_execution: None };
    let sp = state_params();
    validate_panel_bound_v2_with_shards(
        &state,
        &panel_params(),
        &sp,
        &ctx(4, 107, 4),
        &claim_id,
        &anchor,
        anchor_block,
        &seats,
        None,
        false,
        false,
        Some(2),
    )
    .expect("the stratified binding validates as stratified");
    assert_eq!(
        validate_panel_bound_v2_with_shards(
            &state,
            &panel_params(),
            &sp,
            &ctx(4, 107, 4),
            &claim_id,
            &anchor,
            anchor_block,
            &seats,
            None,
            false,
            false,
            None
        ),
        Err(PalwPanelV2Error::PanelMismatch),
        "and not as flat"
    );

    let (short, short_claim) = sharded_registry(&[3, 4]);
    assert_eq!(
        derive_stratified_panel_v2(&short, &panel_params(), &short_claim, anchor_block, 0, None, false, 2),
        Err(PalwPanelV2Error::InsufficientEligibleShardBonds { shard: 0, needed: 3, available: 2 })
    );
}

/// **A part is validated over its own shard's seats**, and the whole-object door refuses a
/// claim drawn per shard.
#[test]
fn a_shard_part_is_validated_over_its_shards_seats_and_the_whole_door_is_shut() {
    let (s3, claim_id) = sharded_registry(&[]);
    let anchor_block = BlockHash::from_u64_word(0xA1);
    let seats = derive_stratified_panel_v2(&s3, &panel_params(), &claim_id, anchor_block, 0, None, false, 2).unwrap();
    let s4 = shard_fold(
        &s3,
        &ctx(4, 107, 4),
        &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: anchor_block, seats: seats.clone() }],
        None,
    );
    let receipt = |seat: &PalwPanelSeatV2| PalwSeatReceiptV2 {
        claim: claim_id,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: seat.bond,
        signed_daa: 108,
        signature: vec![1; 8],
    };
    let part = |shard: u32, receipts: Vec<PalwSeatReceiptV2>| crate::palw_shard_licensing_v1::PalwShardReceiptPartV1 {
        claim: claim_id,
        shard_count: 2,
        shard_index: shard,
        receipts,
    };
    let yes = |_: &[u8], _: &[u8], _: &[u8], _: &[u8]| true;
    let no = |_: &[u8], _: &[u8], _: &[u8], _: &[u8]| false;
    let (pp, sp, at) = (panel_params(), state_params(), ctx(5, 109, 5));
    let shard0 = part(0, vec![receipt(&seats[0]), receipt(&seats[1])]);
    assert_eq!(
        validate_shard_receipt_part_v1(&s4, &pp, &sp, &at, h64(999), &shard0, yes, false),
        Ok(PalwReceiptQuorumV2::Licensed { valid: 2 })
    );
    assert_eq!(
        validate_shard_receipt_part_v1(&s4, &pp, &sp, &at, h64(999), &shard0, no, false),
        Err(PalwPanelV2Error::ReceiptSignatureInvalid)
    );
    let stray = part(0, vec![receipt(&seats[0]), receipt(&seats[3])]);
    assert_eq!(
        validate_shard_receipt_part_v1(&s4, &pp, &sp, &at, h64(999), &stray, yes, false),
        Err(PalwPanelV2Error::NotASeat(seats[3].bond))
    );
    let foreign = crate::palw_shard_licensing_v1::PalwShardReceiptPartV1 { shard_count: 3, ..shard0.clone() };
    assert_eq!(
        validate_shard_receipt_part_v1(&s4, &pp, &sp, &at, h64(999), &foreign, yes, false),
        Err(PalwPanelV2Error::ShardPlanMismatch { declared: 2, part: 3 })
    );
    let receipts: Vec<PalwSeatReceiptV2> = seats[..3].iter().map(receipt).collect();
    assert_eq!(
        validate_receipt_quorum_v2_with_policy(&s4, &pp, &sp, &at, h64(999), &claim_id, &receipts, yes, false, None),
        Err(PalwPanelV2Error::LicensedByParts(claim_id)),
        "a claim drawn per shard has no whole-object licence"
    );
    let s5 = shard_fold(&s4, &ctx(5, 109, 5), &[PalwConsensusObjectV2::ShardReceiptLicensed { part: shard0.clone() }], None);
    assert_eq!(
        validate_shard_receipt_part_v1(&s5, &pp, &sp, &ctx(6, 110, 6), h64(999), &shard0, yes, false),
        Err(PalwPanelV2Error::ShardAlreadyLicensed { shard: 0 })
    );
}

/// **The receipt pool's read is the tip's bound panel and the registry's keys, and nothing
/// else** (node policy; the 2026-09-24 launch review's receipt-pool flush). A `PanelBound` claim
/// answers with its panel as the state holds it; a claim in any other phase, or none at all,
/// answers nothing — so the pool can never be told a panel the chain does not hold; a bond the
/// registry holds answers with its registered key, and an invented one with nothing.
#[test]
fn the_receipt_pool_reads_only_bound_panels_and_registered_keys() {
    let (state, claim_id, _sp, _p, _net, seats) = five_seat_licensed_fixture();
    let panel = state.panel(&claim_id).unwrap().clone();
    let stranger = PalwBondKeyV2(bond_outpoint(0xDEAD));
    let bonds: Vec<PalwBondKeyV2> = seats.iter().map(|seat| seat.bond).chain([stranger]).collect();
    let facts = palw_receipt_pool_facts_v1(&state, &[h64(0xBAD), claim_id], &bonds);
    assert_eq!(
        facts.panels,
        vec![PalwReceiptPanelFactV1 {
            claim_id,
            bound_daa: panel.bound_daa,
            anchor: panel.anchor,
            seats: panel.seats.iter().map(|seat| seat.bond).collect(),
        }],
        "the bound claim answers with its panel, the invented one with nothing"
    );
    assert_eq!(facts.seat_keys.len(), seats.len(), "every seat's key and no stranger's");
    for (bond, key) in &facts.seat_keys {
        assert_eq!(key, &state.bond(bond).unwrap().pubkey);
    }
    assert!(facts.seat_keys.iter().all(|(bond, _)| *bond != stranger));

    // A claim the state holds but has not bound yet has no panel to name.
    let (provisional, unbound) = populated_state();
    assert!(matches!(provisional.claim(&unbound).unwrap().phase, PalwClaimPhaseV2::Provisional));
    assert!(palw_receipt_pool_facts_v1(&provisional, &[unbound], &[]).panels.is_empty());
}

/// **ADR-0152 v3.1 SW: the stake-weighted panel draw's pure half** (§3.14, T85–T88, T92–T94 on
/// synthetic states). The integration — the anchor-time policy value, the room's `ready_eff` in
/// the fold, T89–T91 — is M4's, after M3.
mod adr0152_stake_draw {
    use super::*;
    use crate::palw_panel_economy_v1::{PalwSeatEconomyV1, palw_panel_collateral_floor_v1};

    // ---- T88: the corpus `stake: None` must reproduce byte for byte ----------------------

    /// One state of the T88 corpus: the claim on it, the panel shape, the state params it was
    /// folded under, and every policy it is drawn with.
    struct T88Fixture {
        name: &'static str,
        state: PalwChainStateV2,
        claim_id: Hash64,
        params: PalwPanelParamsV2,
        policies: Vec<PalwPanelDrawPolicyV1>,
    }

    fn t88_bond(b: u64, pk: u8, op: u64, collateral: u64, classes: &[u64]) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(bond_outpoint(b)),
            pubkey: vec![pk; 4],
            operator_pubkey: op_key(op),
            collateral,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            capable_classes: classes.iter().map(|c| h64(*c)).collect(),
            signature: Vec::new(),
        }
    }

    /// ADR-0147's BOUGHT class (id 2): a carriage naming `registrant`, which is what makes the
    /// fold record a `registrant_bond` and so what makes its claims outsider-judged.
    fn t88_bought_class(registrant: PalwBondKeyV2) -> PalwConsensusObjectV2 {
        let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor's geometry projects");
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(2),
            artifact_root: h64(12),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
            initial_target: u128::MAX / 2,
            share_permille: 0,
            activation_daa: 0,
            admission: Some(Box::new(crate::palw_state_v2::PalwClassAdmissionCarriageV2 {
                registrant_bond: registrant,
                canonical: crate::palw_base0_profile::rc_job_context(&profile, 2, 2),
                profile,
                signature: Vec::new(),
            })),
        }
    }

    /// The floor (class 1) and a bought class 2 registered by bond 9; bonds 2..=5 the
    /// registrant's (they serve both), bonds 11..=18 the network's (the floor only), and the
    /// executor's bond 1; one attempt of class 2 by bond 1 at DAA 101.
    fn t88_bought_state(sp: &PalwStateParamsV2, collateral: impl Fn(u64) -> u64) -> (PalwChainStateV2, Hash64) {
        let mut objects = vec![adr0130_class(), t88_bond(1, 7, 0x21, collateral(1), &[1])];
        objects.push(t88_bond(9, 19, 0x29, collateral(9), &[1, 2]));
        for n in 2..=5u64 {
            objects.push(t88_bond(n, 20 + n as u8, 0x40 + n, collateral(n), &[1, 2]));
        }
        for n in 11..=18u64 {
            objects.push(t88_bond(n, 20 + n as u8, 0x40 + n, collateral(n), &[1]));
        }
        objects.push(t88_bought_class(PalwBondKeyV2(bond_outpoint(9))));
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), sp, &ctx(1, 100, 1), &objects, None).unwrap();
        let mut env = attempt(40, 1);
        env.attempt.class_id = h64(2);
        env.attempt.artifact_root = h64(12);
        env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, 1, h64(2), &bond_outpoint(1));
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) =
            apply_palw_transition_v2(&s1, sp, &PalwBlockContextV2 { subsidy: 100_000, ..ctx(2, 101, 2) }, &[], Some(&env)).unwrap();
        (s2, claim_id)
    }

    fn t88_economy(floor: u64, ratio: u32, lambda: u32) -> PalwSeatEconomyV1 {
        PalwSeatEconomyV1 { panel_floor_sompi: floor, max_exposure_ratio_permille: ratio, reward_multiple_permille: lambda }
    }

    /// Every combination of `weighted`, `economy`, `independence` and `valid_lock` the corpus
    /// draws a state under, `stake: None` throughout.
    fn t88_policies(
        economies: &[Option<PalwSeatEconomyV1>],
        independences: &[Option<PalwPanelIndependenceV1>],
        locks: &[Option<PalwPanelValidLockV1>],
    ) -> Vec<PalwPanelDrawPolicyV1> {
        let mut out = Vec::new();
        for weighted in [false, true] {
            for economy in economies {
                for independence in independences {
                    for valid_lock in locks {
                        out.push(PalwPanelDrawPolicyV1 {
                            weighted,
                            economy: *economy,
                            readiness: None,
                            independence: *independence,
                            valid_lock: *valid_lock,
                            stake: None,
                        });
                    }
                }
            }
        }
        out
    }

    fn t88_corpus() -> Vec<T88Fixture> {
        let lock = |sp: &PalwStateParamsV2, required: u128| PalwPanelValidLockV1 {
            required,
            now_daa: 103,
            settled_anchor_depth: None,
            window_court: sp.window_court(),
            rcore: None,
        };
        let independence = |from_daa: u64, anchor_daa: u64| PalwPanelIndependenceV1 { from_daa, base_class_id: h64(1), anchor_daa };
        let mut out = Vec::new();

        // (a) The dedup/exclusion registry: six bonds, bond 5 sharing bond 4's operator, bond 6
        // the executor's operator.
        let sp = state_params();
        let (state, claim_id) = populated_state();
        out.push(T88Fixture {
            name: "populated",
            state,
            claim_id,
            params: panel_params(),
            policies: t88_policies(
                &[None, Some(t88_economy(100, 1000, 0)), Some(t88_economy(1_000, 500, 0))],
                &[None, Some(independence(0, 150)), Some(independence(0, 100))],
                &[None, Some(lock(&sp, 1)), Some(lock(&sp, 2_000_000))],
            ),
        });

        // (b) ADR-0130's λ registry: bonds that back the claim-priced stake and not the floor.
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap().with_worker_carve_permille(620).unwrap();
        let bonds = vec![
            adr0130_bond(1, 7, 0x21, 1_000_000),
            adr0130_bond(2, 8, 0x22, 2_000),
            adr0130_bond(3, 9, 0x23, 2_000),
            adr0130_bond(4, 10, 0x24, 10_000),
            adr0130_bond(5, 11, 0x25, 10_000),
            adr0130_bond(6, 12, 0x26, 9_000),
            adr0130_bond(7, 13, 0x25, 12_000), // a second bond of bond 5's operator
        ];
        let (state, claim_id) = adr0130_state(&sp, bonds, 100_000);
        let floor = palw_panel_collateral_floor_v1(100);
        out.push(T88Fixture {
            name: "adr0130-lambda",
            state,
            claim_id,
            params: panel_params(),
            policies: t88_policies(
                &[None, Some(t88_economy(floor, 500, 0)), Some(t88_economy(floor, 500, 1_000)), Some(t88_economy(floor, 500, 1_100))],
                &[None, Some(independence(0, 150))],
                &[None, Some(lock(&sp, 9_500))],
            ),
        });

        // (c) A wide registry on a five-seat panel, collateral spread over three orders.
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let mut bonds = vec![adr0130_bond(1, 7, 0x21, 1_000_000)];
        for n in 2..=13u64 {
            bonds.push(adr0130_bond(n, 20 + n as u8, 0x60 + n, 1_000 * (1 + (n * 37) % 11) * 10u64.pow((n % 3) as u32)));
        }
        let (state, claim_id) = adr0130_state(&sp, bonds, 0);
        out.push(T88Fixture {
            name: "wide",
            state,
            claim_id,
            params: PalwPanelParamsV2::new(5, 3, 4).unwrap(),
            policies: t88_policies(
                &[None, Some(t88_economy(1_000, 500, 0)), Some(t88_economy(1_000, 1000, 0))],
                &[None, Some(independence(0, 150))],
                &[None, Some(lock(&sp, 5_000))],
            ),
        });

        // (d) ADR-0147: a claim of a bought class, so an outsider sits first where the policy
        // configures independence at or below the claim's acceptance.
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let (state, claim_id) = t88_bought_state(&sp, |n| 1_000_000 + 7_919 * n);
        out.push(T88Fixture {
            name: "bought",
            state,
            claim_id,
            params: PalwPanelParamsV2::new(3, 2, 4).unwrap(),
            policies: t88_policies(
                &[None, Some(t88_economy(1_000, 500, 0))],
                &[None, Some(independence(0, 150)), Some(independence(102, 150)), Some(independence(0, 100))],
                &[None, Some(lock(&sp, 1_030_000))],
            ),
        });
        out
    }

    /// The corpus drawn, every result folded into one digest: `(fixture, policy index, anchor,
    /// Ok(seats) as Borsh | Err as its Debug)`. Returns the digest and the number of results
    /// that were panels and refusals, so a caller can see the corpus is not vacuous.
    fn t88_digest(with: impl Fn(PalwPanelDrawPolicyV1) -> PalwPanelDrawPolicyV1) -> (String, usize, usize) {
        let mut digest = blake2b_simd::Params::new().hash_length(32).to_state();
        let (mut panels, mut refusals) = (0usize, 0usize);
        for fixture in t88_corpus() {
            for (p, policy) in fixture.policies.iter().enumerate() {
                for i in 0..24u64 {
                    let anchor = BlockHash::from_u64_word(0x8800 + i);
                    let drawn = derive_panel_v2_with_policy(
                        &fixture.state,
                        &fixture.params,
                        &fixture.claim_id,
                        anchor,
                        100,
                        None,
                        false,
                        with(*policy),
                    );
                    digest.update(fixture.name.as_bytes());
                    digest.update(&(p as u64).to_le_bytes());
                    digest.update(&i.to_le_bytes());
                    match drawn {
                        Ok(seats) => {
                            panels += 1;
                            digest.update(b"ok");
                            digest.update(&borsh::to_vec(&seats).unwrap());
                        }
                        Err(e) => {
                            refusals += 1;
                            digest.update(b"err");
                            digest.update(format!("{e:?}").as_bytes());
                        }
                    }
                }
            }
        }
        (faster_hex::hex_string(digest.finalize().as_bytes()), panels, refusals)
    }

    /// **T88 (SW-1): `stake: None` is today's draw, byte for byte.** The corpus — four
    /// registries (dedup and executor exclusions, ADR-0130's λ floor with a two-bond operator,
    /// a wide five-seat registry, and ADR-0147's bought class with its outsider) under every
    /// combination of `weighted`, `economy`, `independence` and `valid_lock` — is drawn at 24
    /// anchors each, and the digest of every panel and every refusal is pinned to the value
    /// the draw produced at `f1dfb33b`, before the stake draw existed.
    #[test]
    fn t88_stake_none_is_todays_draw_byte_for_byte() {
        let (digest, panels, refusals) = t88_digest(|policy| policy);
        eprintln!("T88 corpus digest {digest}: {panels} panels, {refusals} refusals");
        assert!(panels > 1_000 && refusals > 100, "the corpus draws panels and refusals both: {panels} / {refusals}");
        assert_eq!(
            digest, "76bb846d28e5007f9224ed883ae6252deea8233ecadfda3b2c60aff428c1062a",
            "stake: None moved a panel or a refusal somewhere in the corpus"
        );
    }

    /// **T88's other half: `stake: Some` replaces the lottery, whatever `weighted` says.** The
    /// same corpus under the stake draw is a different digest (so the pin above is not vacuous),
    /// and `weighted: true` and `false` draw the same panel or the same refusal at every point.
    #[test]
    fn t88_stake_some_replaces_the_lottery_whatever_weighted_says() {
        let stake = |policy: PalwPanelDrawPolicyV1| PalwPanelDrawPolicyV1 { stake: Some(PalwPanelStakeDrawV1::V1), ..policy };
        let (digest, panels, _) = t88_digest(stake);
        assert!(panels > 500, "the stake draw seats panels on the corpus: {panels}");
        assert_ne!(digest, "76bb846d28e5007f9224ed883ae6252deea8233ecadfda3b2c60aff428c1062a", "the stake draw is a different draw");
        let (flat, _, _) = t88_digest(|policy| stake(PalwPanelDrawPolicyV1 { weighted: false, ..policy }));
        let (bucketed, _, _) = t88_digest(|policy| stake(PalwPanelDrawPolicyV1 { weighted: true, ..policy }));
        assert_eq!(flat, bucketed, "C-02's `weighted` is not read under the stake draw");
    }

    // ---- realistic registries (whole-MSK collateral) -----------------------------------------

    const MSK: u64 = crate::constants::SOMPI_PER_KASPA;
    /// A testnet-12 genesis seat: 939,063.21 MSK (whole-MSK weight 939,063).
    const GENESIS_SEAT: u64 = 939_063 * MSK + 21_000_000;
    /// The testnet-12 seat floor, 130,000 MSK — a floor-sized Sybil operator.
    const FLOOR_SEAT: u64 = 130_000 * MSK;

    fn sw_economy() -> PalwSeatEconomyV1 {
        t88_economy(FLOOR_SEAT, 500, 0)
    }

    /// Past `palw_rcore_plus` on testnet-12: the economy and the stake draw's v3.1 terms.
    fn sw_policy() -> PalwPanelDrawPolicyV1 {
        PalwPanelDrawPolicyV1 { economy: Some(sw_economy()), stake: Some(PalwPanelStakeDrawV1::V1), ..Default::default() }
    }

    /// The same network below the fence: ADR-0130's operator lottery.
    fn lottery_policy() -> PalwPanelDrawPolicyV1 {
        PalwPanelDrawPolicyV1 { economy: Some(sw_economy()), ..Default::default() }
    }

    fn five() -> PalwPanelParamsV2 {
        PalwPanelParamsV2::new(5, 3, 4).unwrap()
    }

    fn anchor(i: u64) -> BlockHash {
        BlockHash::from_u64_word(0x5700_0000 + i)
    }

    /// A class whose attempts may claim `2 × 10^13` pwu, so one of them reserves 1,000,000 MSK
    /// (at the network's 5 sompi a pwu) on its producer — past any genesis seat's 500‰ ceiling
    /// (469,531 MSK) — and one attempt SATURATES a bond's economy headroom. Folded without
    /// the 2026-09-23 audit extras, the only place an attempt is refused at the ceiling, so the
    /// reservation lands whole.
    const HEAVY_PWU: u64 = 20_000_000_000_000;

    fn heavy_class() -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(3),
            artifact_root: h64(13),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(HEAVY_PWU),
            initial_target: u128::MAX / 2,
            share_permille: 0,
            activation_daa: 0,
            admission: None,
        }
    }

    /// Fold one attempt of `class` by bond `b` (key `pk`, operator `op`) at `daa`, at the first
    /// nonce the fold admits.
    #[allow(clippy::too_many_arguments)]
    fn fold_attempt(
        parent: &PalwChainStateV2,
        sp: &PalwStateParamsV2,
        daa: u64,
        b: u64,
        pk: u8,
        op: u64,
        class_id: Hash64,
        root: Hash64,
        pwu: u64,
    ) -> (PalwChainStateV2, Hash64) {
        for nonce in 1..400u64 {
            let mut env = attempt(pwu, nonce);
            env.attempt.class_id = class_id;
            env.attempt.artifact_root = root;
            env.attempt.executor_bond = bond_outpoint(b);
            env.attempt.executor_pubkey = vec![pk; 4];
            env.attempt.operator_id = op_id(op);
            env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, nonce, class_id, &bond_outpoint(b));
            let id = attempt_id_v2(&env.attempt);
            if let Ok((next, _)) = apply_palw_transition_v2(parent, sp, &ctx(daa, daa, daa), &[], Some(&env))
                && next.claim(&id).is_some()
            {
                return (next, id);
            }
        }
        panic!("no attempt of bond {b} admits at daa {daa}")
    }

    /// A registry of `(bond, operator, collateral)` rows beside the executor (bond 1, operator
    /// 0x21) at DAA 100, every bond serving the floor; one heavy attempt per bond in `saturate`
    /// (DAA 101, 102, …); then the claim under judgement — the executor's 40-pwu floor attempt,
    /// `reserved` 200, so a seat reserves 600 sompi. Keys are `30 + bond`.
    fn sw_state(rows: &[(u64, u64, u64)], saturate: &[u64]) -> (PalwChainStateV2, Hash64) {
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let mut objects = vec![adr0130_class(), heavy_class(), adr0130_bond(1, 7, 0x21, 1_000_000)];
        objects.extend(rows.iter().map(|(b, op, c)| adr0130_bond(*b, 30 + *b as u8, *op, *c)));
        let (mut state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
        let mut daa = 101;
        for b in saturate {
            let op = rows.iter().find(|row| row.0 == *b).expect("a saturated bond is a row").1;
            state = fold_attempt(&state, &sp, daa, *b, 30 + *b as u8, op, h64(3), h64(13), HEAVY_PWU).0;
            daa += 1;
        }
        fold_attempt(&state, &sp, daa, 1, 7, 0x21, h64(1), h64(11), 40)
    }

    /// The eight testnet-12 genesis seats (bonds 2..=9) and `small` floor-sized operators
    /// (bonds 10, 11, …).
    fn genesis_and_small(small: u64) -> Vec<(u64, u64, u64)> {
        let mut rows: Vec<(u64, u64, u64)> = (2..=9u64).map(|b| (b, 0x40 + b, GENESIS_SEAT)).collect();
        rows.extend((0..small).map(|i| (10 + i, 0x4A + i, FLOOR_SEAT)));
        rows
    }

    /// The class population and SW-10's base for the claim, as `derive_panel_v2_with_policy`
    /// computes them under `sw_policy()` (no outsider).
    #[allow(clippy::type_complexity)]
    fn populations<'a>(
        state: &'a PalwChainStateV2,
        claim_id: &Hash64,
    ) -> (Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>, Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)>) {
        let eligible = palw_panel_eligible_bonds_v2(state, claim_id, 100, None, false, None, Some(sw_economy()), 5).unwrap();
        let base =
            palw_panel_stake_base_bonds_judging_v1(state, claim_id, &h64(1), 100, None, false, None, Some(sw_economy()), 5).unwrap();
        (eligible, base)
    }

    /// **The exact successive-sampling law by enumeration of ordered draws**: every operator's
    /// inclusion probability, and `P(A = a)` for the `marked` operators. Independent of the
    /// race: it draws seat by seat in proportion to weight among those not yet seated.
    fn exact_law(weights: &[f64], seats: usize, marked: &[bool]) -> (Vec<f64>, Vec<f64>) {
        #[allow(clippy::too_many_arguments)]
        fn walk(w: &[f64], seats: usize, marked: &[bool], seq: &mut Vec<usize>, p: f64, rem: f64, inc: &mut [f64], law: &mut [f64]) {
            if seq.len() == seats || rem <= 0.0 {
                for &i in seq.iter() {
                    inc[i] += p;
                }
                law[seq.iter().filter(|&&i| marked[i]).count()] += p;
                return;
            }
            for (i, wi) in w.iter().enumerate() {
                if seq.contains(&i) {
                    continue;
                }
                seq.push(i);
                walk(w, seats, marked, seq, p * wi / rem, rem - wi, inc, law);
                seq.pop();
            }
        }
        let mut inc = vec![0.0; weights.len()];
        let mut law = vec![0.0; seats + 1];
        walk(weights, seats, marked, &mut Vec::new(), 1.0, weights.iter().sum(), &mut inc, &mut law);
        (inc, law)
    }

    /// `|measured − p| ≤ 4σ` of a binomial proportion over `n` draws.
    fn within_4_sigma(hits: usize, n: usize, p: f64) -> bool {
        let measured = hits as f64 / n as f64;
        (measured - p).abs() <= 4.0 * (p * (1.0 - p) / n as f64).sqrt()
    }

    // ---- T85: the integer −log2, the key order, the cap ---------------------------------------

    /// `(u, palw_draw_neg_log2_q64_v1(u))`: the six edges, then 64 values of splitmix64 from
    /// seed `0x5712_A152`. Generated outside the tree by a Python port of §3.14's routine
    /// (`t85_golden.py`, beside this commit's review notes), which checks every output against
    /// `−log2((u + 1) / 2^64)` from an 80-digit decimal logarithm: worst error `1.44 × 10^-19`
    /// (under three units of `2^-64`, always above the exact value — the fraction bits
    /// truncate), against the `4 × 10^-15` §3.14 states.
    const T85_VECTORS: [(u64, u128); 70] = [
        (0x0000000000000000, 0x400000000000000000),
        (0x0000000000000001, 0x3f0000000000000000),
        (0x7fffffffffffffff, 0x10000000000000000),
        (0x8000000000000000, 0xfffffffffffffffe),
        (0xfffffffffffffffe, 0x2),
        (0xffffffffffffffff, 0x0),
        (0x6f0f403721bcd0aa, 0x1346e6e6b1bf945db),
        (0xcc48eed2a23d2ccc, 0x5357fbf482e9dc53),
        (0xa5d9f964820d1450, 0xa0520e78dc92b8a3),
        (0x1d3811b4b2c524b5, 0x32193f887684f1843),
        (0xc890505e92c5144a, 0x5a221e4aca7d17c9),
        (0x0dd572e79332baca, 0x435ba6f2e7716b368),
        (0x70f4e4352085f99e, 0x12e2d10d7faead78c),
        (0x28164e8dc7cd47ac, 0x2acc861338f9bcf56),
        (0x8e7d596406d58786, 0xd864b605086db2d2),
        (0x789235dd2c34f28f, 0x11615161fc80e9ea0),
        (0xff1cf402f0ab197f, 0x14820c86687c893),
        (0x81f7c1d9eed22496, 0xfa5d860b1161167d),
        (0x3639e8beb8807177, 0x23d34741903cfd8cf),
        (0x4af892770b56e5bf, 0x1c590b92232fc90d7),
        (0x2bab0702e7601b8c, 0x28d2ea447071b093d),
        (0xe00339b41b285a75, 0x314bdeb4fa70f6dd),
        (0xb605ffe5afd25605, 0x7df4ff1090dac063),
        (0x8c145cfd0f1c21d0, 0xdeb19a924a89178f),
        (0xf94f9fb8e8d4708e, 0x9c748e17e97ef88),
        (0x22cd9af9af93a1ab, 0x2e0fc967414b3a5d4),
        (0xfd2f03424842de22, 0x415edbf7292f5ba),
        (0x247d8bfc0540f800, 0x2cf807bdd1c4cfa28),
        (0xe22e0137a0d0e72c, 0x2dbd99ab9518c221),
        (0xc004ddc0c718c35a, 0x6a368991defac57e),
        (0xe8af11918c1297d2, 0x2345119e09dbe26e),
        (0xc78d5daa8ba63461, 0x5c002ac937d1c69d),
        (0xf72f254e22c46335, 0xcf1560c808af770),
        (0x3ee13160bb812e38, 0x20685c23837ca6fb9),
        (0xad1b1d3e35688dba, 0x90825654e2bc48b9),
        (0xe9ba05e099cba1e8, 0x219e4a1466267fbd),
        (0x01480ef3d43358be, 0x7a466a3035c381c3f),
        (0x5f9ed7f03c63fd85, 0x16bb66ae1801f8f26),
        (0x045a675b561e3d3c, 0x5e0c0eb52a9835556),
        (0xaa0f6b9e047b73b9, 0x9710a90ac3fbdb4d),
        (0xad9e506ecb9d3080, 0x8f6ad44a50a48bec),
        (0x910e54e7d6ff0a0f, 0xd1ccfb0f424ae85f),
        (0xf205af685e54574c, 0x14bcb3ea5a890d1f),
        (0xa3340f0d447fae95, 0xa643de728d886c7b),
        (0x10cc420d47dabf92, 0x3ee06e1765a7b9c45),
        (0x5095f9c2ac742948, 0x1aae4431af73682d9),
        (0xbe8eeb226ea39f15, 0x6d088a0d488bbd7b),
        (0x25d6deed77794673, 0x2c21869bfaca1a418),
        (0x50d6646795125df1, 0x1a9bd7f34d623cf30),
        (0x5ab6d28f7e12738e, 0x17f2aa24a6ff67e5e),
        (0x4d11633eafaca04d, 0x1bb6083a0f0daffb0),
        (0x2abf4be66f7daa88, 0x2950db21676dc68e2),
        (0x7c222d3ec2f9dcf5, 0x10b540d12a3d7e077),
        (0x6c798c8fb6389914, 0x13d20f090f5185f25),
        (0x5561907a877fa57b, 0x1958b2e27cee38d56),
        (0x517425c83e98f488, 0x1a6ef7a5e456674b6),
        (0xa571e6f61813d29d, 0xa13a1894d49e771d),
        (0x34e44700758e0471, 0x2466863408dcc3bd2),
        (0x935bbfd164172771, 0xcbfc09d44253174d),
        (0xb79973b7876bb430, 0x7ac5e7bb25c254a7),
        (0x8e5361f7bef9c060, 0xd8d18cc2629af30b),
        (0x7b2dc216fedd9f75, 0x10e2e1266f9b47014),
        (0xf9db28871146c3ed, 0x8f8cdc246c0f101),
        (0xa52f4785c864523f, 0xa1cef02837301c3c),
        (0x90dad5f0caa23611, 0xd2502fc3be7a7583),
        (0x6b712376f16623bb, 0x140a98172e6b25f91),
        (0x4c16f1ed05e6584f, 0x1c018626ae9f2dddc),
        (0xae78235afd59e726, 0x8d9c9849f3a1c036),
        (0x2eebaf871e547def, 0x272a63b53974b38c7),
        (0xb278b4a1367eeb4a, 0x853c36ea74408f0e),
    ];

    fn splitmix64(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// **T85 (SW-3): the integer `−log2` is the normative routine, monotone, and never
    /// overflows; the key order is the cross-multiplication with ties by operator id; `W` is
    /// capped.** Debug builds check every `u128` operation for overflow, so running the routine
    /// over the vectors and the sweep IS the no-overflow check.
    #[test]
    fn t85_the_integer_log_the_key_order_and_the_cap() {
        // The golden vectors, and the table's pseudo-random half is what splitmix64 says.
        let mut seed = 0x5712_A152u64;
        for (i, (u, l)) in T85_VECTORS.iter().enumerate() {
            assert_eq!(palw_draw_neg_log2_q64_v1(*u), *l, "vector {i}: u = {u:#x}");
            if i >= 6 {
                assert_eq!(*u, splitmix64(&mut seed), "vector {i} is splitmix64's");
            }
            // And against the float logarithm, loosely (the tight bound is the script's).
            let float = 64.0 - ((*u as f64) + 1.0).log2();
            assert!((*l as f64 / 2f64.powi(64) - float).abs() < 1e-9, "vector {i}: {l:#x} against {float}");
        }
        assert_eq!(palw_draw_neg_log2_q64_v1(0), 64u128 << 64, "u = 0 is the largest key, 2^70");
        assert_eq!(palw_draw_neg_log2_q64_v1(u64::MAX), 0, "u = 2^64 − 1 is the smallest");

        // Monotone: never rises as u rises — over 20,000 draws, every power of two and its
        // neighbours, and every step across the 2^63 and 2^64 boundaries.
        let mut us: Vec<u64> = (0..20_000).map(|_| splitmix64(&mut seed)).collect();
        for k in 0..64u32 {
            let p = 1u64 << k;
            us.extend([p.wrapping_sub(1), p, p.saturating_add(1)]);
        }
        us.extend((0..512u64).map(|d| (1u64 << 63) - 256 + d));
        us.extend((0..256u64).map(|d| u64::MAX - d));
        us.sort_unstable();
        us.dedup();
        let mut previous = u128::MAX;
        for u in &us {
            let l = palw_draw_neg_log2_q64_v1(*u);
            assert!(l <= previous, "monotone at u = {u:#x}");
            assert!(l <= 64u128 << 64);
            previous = l;
        }

        // The key order: `L_i · W_j < L_j · W_i`, ties by operator id.
        let (a, b) = (h64(1), h64(2));
        assert_eq!(palw_draw_key_cmp_v1(6, 3, &b, 5, 3, &a), std::cmp::Ordering::Greater, "6/3 after 5/3");
        assert_eq!(palw_draw_key_cmp_v1(6, 3, &b, 4, 2, &a), std::cmp::Ordering::Greater, "6/3 = 4/2: the id decides, 2 after 1");
        assert_eq!(palw_draw_key_cmp_v1(6, 3, &a, 4, 2, &b), std::cmp::Ordering::Less, "…and 1 before 2");
        assert_eq!(palw_draw_key_cmp_v1(100, 2, &b, 60, 1, &a), std::cmp::Ordering::Less, "50 before 60: the heavier key wins");
        // The extremes multiply inside u128.
        let top = 64u128 << 64;
        assert_eq!(
            palw_draw_key_cmp_v1(top, PALW_DRAW_WEIGHT_MAX_MSK_V1, &a, top - 1, PALW_DRAW_WEIGHT_MAX_MSK_V1, &b),
            std::cmp::Ordering::Greater
        );
        assert_eq!(palw_draw_key_cmp_v1(top, u64::MAX, &a, top, u64::MAX, &b), std::cmp::Ordering::Less, "clamped, then the id");

        // W: whole MSK, capped at the policy's cap, under 2^40, never below 1.
        let cap = PalwPanelStakeDrawV1::V1.weight_cap_msk;
        assert_eq!(cap, 1_000_000);
        assert_eq!(palw_draw_operator_weight_msk_v1((GENESIS_SEAT / MSK) as u128, cap), 939_063, "a genesis seat, uncapped");
        assert_eq!(palw_draw_operator_weight_msk_v1(20_000_000, cap), 1_000_000, "a 20M operator weighs the cap");
        assert_eq!(palw_draw_operator_weight_msk_v1(0, cap), 1, "every eligible operator has a key");
        assert_eq!(palw_draw_operator_weight_msk_v1(u128::MAX, u64::MAX), PALW_DRAW_WEIGHT_MAX_MSK_V1, "the arithmetic bound");

        // The ticket is H(domain ‖ anchor ‖ claim ‖ operator) under the lottery's hasher, and u
        // is its first eight bytes little-endian.
        let (anchor, claim, operator) = (BlockHash::from_u64_word(9), h64(8), h64(7));
        let mut by_hand = keyed(PALW_PANEL_V2_DOMAIN_STAKE_TICKET);
        by_hand.update(anchor.as_byte_slice());
        by_hand.update(claim.as_byte_slice());
        by_hand.update(operator.as_byte_slice());
        let ticket = finish(by_hand);
        assert_eq!(palw_panel_stake_ticket_v1(anchor, &claim, &operator), ticket);
        assert_eq!(palw_draw_ticket_u64_v1(&ticket).to_le_bytes(), ticket.as_byte_slice()[..8]);
        assert_ne!(palw_panel_stake_outsider_ticket_v1(anchor, &claim, &operator), ticket, "the outsider has its own domain");
        assert_ne!(palw_panel_operator_ticket_v1(anchor, &claim, &operator), ticket, "and the lottery's is not reused");
    }

    // ---- lane PL part E: the availability factor scales the RACE weight and nothing else -----------------

    /// **A seat with zero successful receipts is drawn at 0.5x weight; a long high-availability seat at 1.2x; the entries
    /// stay a total order.** Dormant (`availability_at: None`) the entries are today's, byte for byte.
    #[test]
    fn pl_e_a_seat_with_no_successes_is_drawn_at_half_weight() {
        use crate::palw_seat_availability_v1::*;
        let (mut state, claim_id) = sw_state(&genesis_and_small(1), &[]);
        let (eligible, _) = populations(&state, &claim_id);
        let at = anchor(7);
        let plain = palw_panel_stake_entries_v1(&claim_id, at, &eligible, &PalwPanelStakeDrawV1::V1);
        let dormant = palw_panel_stake_entries_availability_v1(&claim_id, at, &eligible, &PalwPanelStakeDrawV1::V1, Some(&state));
        assert_eq!(plain, dormant, "dormant: byte for byte");
        let now = 3 * PALW_AVAIL_EPOCH_DAA_V1 * PALW_AVAIL_WINDOW_EPOCHS_V1;
        let boosted = plain[0].bond;
        let normal = plain[1].bond;
        let mut high = PalwSeatAvailabilityV1 { first_credit_daa: Some(0), epochs: vec![] };
        for i in 0..30u64 {
            high.note_assigned(now - 100 + i);
            high.note_credited(now - 100 + i);
        }
        let mut ok = PalwSeatAvailabilityV1::default();
        for i in 0..3u64 {
            ok.note_credited(now - 100 + i);
        }
        state.set_seat_availability_row_for_tests(boosted, high);
        state.set_seat_availability_row_for_tests(normal, ok);
        let (eligible, _) = populations(&state, &claim_id);
        let armed = PalwPanelStakeDrawV1 { availability_at: Some(now), ..PalwPanelStakeDrawV1::V1 };
        let entries = palw_panel_stake_entries_availability_v1(&claim_id, at, &eligible, &armed, Some(&state));
        let weight = |bond: PalwBondKeyV2, list: &[PalwPanelStakeEntryV1]| list.iter().find(|e| e.bond == bond).unwrap().weight_msk;
        assert_eq!(weight(boosted, &entries), palw_seat_availability_scale_weight_v1(weight(boosted, &plain), 1_200));
        assert_eq!(weight(normal, &entries), weight(normal, &plain), "1.0x");
        let nobody = plain.iter().map(|e| e.bond).find(|b| *b != boosted && *b != normal).unwrap();
        assert_eq!(weight(nobody, &entries), weight(nobody, &plain) / 2, "zero successful receipts: 0.5x");
        assert!(entries.windows(2).all(|w| w[0].key_cmp(&w[1]) != std::cmp::Ordering::Greater), "still sorted by key");
        // Deterministic: the same inputs, the same entries.
        assert_eq!(entries, palw_panel_stake_entries_availability_v1(&claim_id, at, &eligible, &armed, Some(&state)));
    }

    // ---- T86: the law ---------------------------------------------------------------------------

    /// **T86 (SW-3, SW-6): the draw is successive sampling.** Eight genesis seats and three
    /// floor-sized operators on a five-seat panel, over 2^16 anchors: every operator's inclusion
    /// is within 4σ of the exact successive-sampling inclusion (0.58477 a genesis seat, 0.10729 a
    /// 130k operator — enumerated here, and `v31_stake_draw.py`'s `dist_A((8, 939,063), (3,
    /// 130,000))` gives the same `E[A]/3`). The S1 assignment is a hash of `(anchor, claim,
    /// seat_count)` that the race does not feed, so the full seat and a segment's holder are a
    /// uniform ordered pair of positions: the measured P2 of the three small operators matches
    /// `E[A(A − 1)] / 20` of the exact law. And with equal weights the race is today's uniform
    /// law, `5/11` each.
    #[test]
    fn t86_the_draw_is_successive_sampling_and_s1_is_untouched() {
        let (state, claim_id) = sw_state(&genesis_and_small(3), &[]);
        let (eligible, base) = populations(&state, &claim_id);
        assert_eq!((eligible.len(), base.len()), (11, 11));
        let stake = PalwPanelStakeDrawV1::V1;
        let operators: Vec<Hash64> = (2..=12u64).map(|b| op_id(if b <= 9 { 0x40 + b } else { 0x4A + b - 10 })).collect();
        let small = |op: &Hash64| operators[8..].contains(op);
        let weights: Vec<f64> = (0..11).map(|i| if i < 8 { 939_063.0 } else { 130_000.0 }).collect();
        let marked: Vec<bool> = (0..11).map(|i| i >= 8).collect();
        let (inclusion, law) = exact_law(&weights, 5, &marked);
        assert!((inclusion[0] - 0.584_767_875_6).abs() < 1e-9 && (inclusion[8] - 0.107_285_665_1).abs() < 1e-9);
        let p2_exact = law.iter().enumerate().map(|(a, p)| (a * a.saturating_sub(1)) as f64 * p).sum::<f64>() / 20.0;
        assert!((p2_exact - 0.002_431_702_46).abs() < 1e-9, "E[A(A − 1)] / 20 = {p2_exact}");

        const N: usize = 1 << 16;
        let mut seated = [0usize; 11];
        let mut p2_hits = 0usize;
        let lottery = lottery_policy();
        let mut differs = 0usize;
        let mut s1_s3 = blake2b_simd::Params::new().hash_length(32).to_state();
        for i in 0..N as u64 {
            let at = anchor(i);
            let seats = palw_panel_stake_race_of_v1(5, &claim_id, at, &eligible, &base, &stake).expect("a full panel");
            for seat in &seats {
                seated[operators.iter().position(|op| *op == seat.operator_id).unwrap()] += 1;
            }
            // S1: the full seat and segment 0's partial holder, by position.
            let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(at, claim_id, 5);
            let holder = (0..5u16).find(|s| *s != assignment.full_seat && assignment.mask_of(*s).covers(0)).unwrap();
            p2_hits +=
                usize::from(small(&seats[assignment.full_seat as usize].operator_id) && small(&seats[holder as usize].operator_id));
            if i < 64 {
                // The race IS the draw: `derive_panel_v2_with_policy` seats the same panel.
                let derived = derive_panel_v2_with_policy(&state, &five(), &claim_id, at, 100, None, false, sw_policy()).unwrap();
                assert_eq!(derived, seats);
                // SW-6: the assignment and the S3 sites read the bind — `(anchor, claim,
                // seat_count)` and `(anchor, claim, seat_index)` — and never the panel, so the race
                // cannot move them; their outputs are pinned below against today's.
                let other = derive_panel_v2_with_policy(&state, &five(), &claim_id, at, 100, None, false, lottery).unwrap();
                differs += usize::from(other != seats);
                s1_s3.update(format!("{assignment:?}").as_bytes());
                for s in 0..5u16 {
                    let sites = crate::palw_layer_sample_v3::palw_layer_sample_v3(at, claim_id, s, 4, 64, 3);
                    s1_s3.update(format!("{sites:?}").as_bytes());
                }
            }
        }
        assert!(differs > 0, "the lottery and the race must disagree somewhere, or the comparison is vacuous");
        assert_eq!(
            faster_hex::hex_string(s1_s3.finalize().as_bytes()),
            "67384c79d5fa0ac675d0bdba2d84e1c5a13872ba841530ec1f5be1cbe33ae303",
            "the S1 assignment and the S3 sites are today's"
        );
        for (i, hits) in seated.iter().enumerate() {
            assert!(within_4_sigma(*hits, N, inclusion[i]), "operator {i}: seated {hits} of {N}, exact inclusion {:.5}", inclusion[i]);
        }
        assert!(within_4_sigma(p2_hits, N, p2_exact), "P2 measured {p2_hits} of {N}, exact {p2_exact:.6}");

        // Equal weights: a cap of one MSK makes every weight 1, and the race is uniform.
        let uniform = PalwPanelStakeDrawV1 { weight_cap_msk: 1, ..stake };
        let mut seated = [0usize; 11];
        const M: usize = 1 << 14;
        for i in 0..M as u64 {
            for seat in palw_panel_stake_race_of_v1(5, &claim_id, anchor(i), &eligible, &base, &uniform).unwrap() {
                seated[operators.iter().position(|op| *op == seat.operator_id).unwrap()] += 1;
            }
        }
        for (i, hits) in seated.iter().enumerate() {
            assert!(within_4_sigma(*hits, M, 5.0 / 11.0), "equal weights: operator {i} seated {hits} of {M}");
        }
    }

    // ---- T87: the weight and the keys ----------------------------------------------------------

    /// **T87 (SW-2, SW-7): an operator's weight sums its eligible bonds, a bond registered at or
    /// after the anchor adds nothing, no other operator moves a key, and splitting leaves the
    /// first seat's law and `P(at least one seat)` where they were.**
    #[test]
    fn t87_the_weight_sums_eligible_bonds_and_keys_are_the_operators_own() {
        let stake = PalwPanelStakeDrawV1::V1;
        // Operator X (0x50) holds two bonds, 400,000 and 300,000 MSK (ids are not unique on this
        // fixture; on testnet-12 they are, and the sum is one bond's collateral).
        let mut rows = genesis_and_small(0);
        rows.truncate(6);
        rows.extend([(20, 0x50, 400_000 * MSK), (21, 0x50, 300_000 * MSK)]);
        let (state, claim_id) = sw_state(&rows, &[]);
        let (eligible, _) = populations(&state, &claim_id);
        let x = op_id(0x50);
        for i in 0..16u64 {
            let entries = palw_panel_stake_entries_v1(&claim_id, anchor(i), &eligible, &stake);
            let entry = entries.iter().find(|e| e.operator_id == x).unwrap();
            assert_eq!(entry.weight_msk, 700_000, "the sum of X's two bonds");
            let best = [20u64, 21]
                .iter()
                .map(|n| PalwBondKeyV2(bond_outpoint(*n)))
                .min_by_key(|bond| (palw_panel_seat_ticket_v1(anchor(i), &claim_id, bond), *bond))
                .unwrap();
            assert_eq!(entry.bond, best, "X sits with today's candidate bond");
            assert_eq!(entries.iter().filter(|e| e.operator_id == x).count(), 1, "one entry an operator");
        }
        // Past the cap the sum stops.
        let mut heavy = rows.clone();
        heavy.push((22, 0x50, 600_000 * MSK));
        let (heavy_state, heavy_claim) = sw_state(&heavy, &[]);
        assert_eq!(heavy_claim, claim_id);
        let (heavy_eligible, _) = populations(&heavy_state, &heavy_claim);
        let entries = palw_panel_stake_entries_v1(&claim_id, anchor(0), &heavy_eligible, &stake);
        assert_eq!(entries.iter().find(|e| e.operator_id == x).unwrap().weight_msk, 1_000_000, "1.3M capped at 1,000,000");

        // A bond registered AT or after the anchor adds nothing (ADR-0147's cut, SW-8): X's
        // second bond registers at DAA 150; under independence the claim anchored at DAA 150
        // draws exactly the panel the registry without that bond draws, and at 151 it counts.
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let mut early = rows.clone();
        early.pop();
        let (lean, lean_claim) = sw_state(&early, &[]);
        assert_eq!(lean_claim, claim_id);
        let (late, _) = apply_palw_transition_v2(&lean, &sp, &ctx(150, 150, 150), &[adr0130_bond(21, 51, 0x50, 300_000 * MSK)], None)
            .expect("the late bond registers");
        let at = |anchor_daa| PalwPanelDrawPolicyV1 {
            independence: Some(PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa }),
            ..sw_policy()
        };
        let mut moved = false;
        for i in 0..64u64 {
            let draw = |state: &PalwChainStateV2, policy| {
                derive_panel_v2_with_policy(state, &five(), &claim_id, anchor(i), 100, None, false, policy).unwrap()
            };
            assert_eq!(draw(&late, at(150)), draw(&lean, at(150)), "anchor {i}: registered at the anchor, it adds nothing");
            moved |= draw(&late, at(151)) != draw(&lean, at(151));
        }
        assert!(moved, "registered before the anchor, the bond adds weight — or the refusal above is vacuous");

        // Adding or removing any OTHER operator never moves an operator's key.
        let (wide, _) = sw_state(&genesis_and_small(3), &[]);
        let (all, _) = populations(&wide, &claim_id);
        let without: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> =
            all.iter().copied().filter(|(_, bond)| bond.operator_id != op_id(0x44)).collect();
        let with_stranger_row = {
            let mut rows = genesis_and_small(3);
            rows.push((30, 0x70, 250_000 * MSK));
            rows
        };
        let (stranger, _) = sw_state(&with_stranger_row, &[]);
        let (with, _) = populations(&stranger, &claim_id);
        for i in 0..32u64 {
            let full = palw_panel_stake_entries_v1(&claim_id, anchor(i), &all, &stake);
            for other in [
                palw_panel_stake_entries_v1(&claim_id, anchor(i), &without, &stake),
                palw_panel_stake_entries_v1(&claim_id, anchor(i), &with, &stake),
            ] {
                for entry in &other {
                    if let Some(same) = full.iter().find(|e| e.operator_id == entry.operator_id) {
                        assert_eq!(same, entry, "anchor {i}: an operator's key is its own");
                    }
                }
            }
        }

        // Splitting: X of 260,000 MSK against X1 + X2 of 130,000 each, beside the eight genesis
        // seats. The group's first key is distributed as X's (the minimum of two exponentials of
        // rate W/2 is one of rate W), so the first seat's law and P(at least one seat) are the
        // same — exactly in law, and within 4σ here over 2^14 anchors each.
        let mut whole = genesis_and_small(0);
        whole.push((20, 0x50, 260_000 * MSK));
        let mut split = genesis_and_small(0);
        split.extend([(20, 0x50, FLOOR_SEAT), (21, 0x51, FLOOR_SEAT)]);
        let group = [op_id(0x50), op_id(0x51)];
        let mut weights: Vec<f64> = vec![939_063.0; 8];
        weights.push(260_000.0);
        let marked: Vec<bool> = (0..9).map(|i| i == 8).collect();
        let (inclusion, _) = exact_law(&weights, 5, &marked);
        let first = 260_000.0 / (8.0 * 939_063.0 + 260_000.0);
        const N: usize = 1 << 14;
        for (label, rows) in [("whole", whole), ("split", split)] {
            let (state, claim_id) = sw_state(&rows, &[]);
            let (eligible, base) = populations(&state, &claim_id);
            let (mut any, mut head) = (0usize, 0usize);
            for i in 0..N as u64 {
                let seats = palw_panel_stake_race_of_v1(5, &claim_id, anchor(i), &eligible, &base, &stake).unwrap();
                any += usize::from(seats.iter().any(|s| group.contains(&s.operator_id)));
                head += usize::from(group.contains(&seats[0].operator_id));
            }
            assert!(within_4_sigma(any, N, inclusion[8]), "{label}: at least one seat {any} of {N}, exact {:.5}", inclusion[8]);
            assert!(within_4_sigma(head, N, first), "{label}: the first seat {head} of {N}, exact {first:.5}");
        }
    }

    // ---- build = accept ------------------------------------------------------------------------

    /// **Build = accept (SW-1, the pure part of T89).** `validate_panel_bound_v2_with_policy`
    /// recomputes through the same functions: under the stake policy the race's panel is
    /// accepted and the lottery's refused wherever they differ, and under `stake: None` the
    /// reverse — for a genesis class and for an ADR-0147 outsider-judged claim.
    #[test]
    fn build_equals_accept_and_each_draw_refuses_the_others_panel() {
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let (state, claim_id) = sw_state(&genesis_and_small(3), &[]);
        let check = |state: &PalwChainStateV2, claim_id: &Hash64, params: &PalwPanelParamsV2, stake_policy, lottery| {
            let slot = state.claim(claim_id).unwrap().bind_base_daa() + params.anchor_delay();
            let mut differs = 0usize;
            for i in 0..48u64 {
                let at = anchor(i);
                let fact = PalwAnchorFactV2 { anchor_block: at, anchor_daa: slot, predecessor_daa: slot - 1, anchor_execution: None };
                // SW-8 (M4): the binding block IS the anchor block — `anchor(i)`'s own word.
                let in_anchor = ctx(0x5700_0000 + i, slot, 9_000);
                let later = ctx(9_000, slot + 1, 9_000);
                let validate_at = |at_block: &PalwBlockContextV2, seats: &[PalwPanelSeatV2], policy| {
                    validate_panel_bound_v2_with_policy(
                        state, params, &sp, at_block, claim_id, &fact, at, seats, None, false, policy, None,
                    )
                };
                let validate = |seats: &[PalwPanelSeatV2], policy| validate_at(&in_anchor, seats, policy);
                let raced = derive_panel_v2_with_policy(state, params, claim_id, at, 100, None, false, stake_policy).unwrap();
                let drawn = derive_panel_v2_with_policy(state, params, claim_id, at, 100, None, false, lottery).unwrap();
                assert_eq!(validate(&raced, stake_policy), Ok(()), "anchor {i}: the race's panel is accepted under its policy");
                assert_eq!(validate(&drawn, lottery), Ok(()), "anchor {i}: and the lottery's under its own");
                // SW-8: a block after the anchor block — inside the bind window — may carry the
                // lottery's panel as it always could, and never the race's.
                assert!(
                    matches!(validate_at(&later, &raced, stake_policy), Err(PalwPanelV2Error::BindOutsideWindow(_))),
                    "anchor {i}: under the stake draw a later block cannot bind the claim"
                );
                assert_eq!(validate_at(&later, &drawn, lottery), Ok(()), "anchor {i}: stake None keeps today's window");
                if raced != drawn {
                    differs += 1;
                    assert_eq!(validate(&drawn, stake_policy), Err(PalwPanelV2Error::PanelMismatch), "anchor {i}");
                    assert_eq!(validate(&raced, lottery), Err(PalwPanelV2Error::PanelMismatch), "anchor {i}");
                }
            }
            assert!(differs > 0, "the two draws must differ somewhere, or the refusals are vacuous");
        };
        check(&state, &claim_id, &five(), sw_policy(), lottery_policy());

        // The outsider-judged claim: honest network operators (11..=18) at genesis size, the
        // registrant's four Sybils (2..=5) at the floor, the registrant (9) excluded.
        let (bought, bought_claim) = t88_bought_state(&sp, |n| if (11..=18).contains(&n) { GENESIS_SEAT } else { FLOOR_SEAT });
        let independent = |policy: PalwPanelDrawPolicyV1| PalwPanelDrawPolicyV1 {
            independence: Some(PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa: 150 }),
            ..policy
        };
        let three = PalwPanelParamsV2::new(3, 2, 4).unwrap();
        check(&bought, &bought_claim, &three, independent(sw_policy()), independent(lottery_policy()));
    }

    /// **SW-5: the outsider races by stake over the network's population.** Beside eight honest
    /// genesis-size operators, the registrant's four 130k Sybils sit as the outsider with the
    /// first-key probability `520,000 / 8,032,504 = 0.0647` (the uniform outsider ticket gave
    /// them 4/12); within 4σ over 2^12 anchors. The class's own seats follow, without the
    /// outsider's operator.
    #[test]
    fn sw5_the_outsider_is_the_smallest_stake_outsider_key() {
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let (state, claim_id) = t88_bought_state(&sp, |n| if (11..=18).contains(&n) { GENESIS_SEAT } else { FLOOR_SEAT });
        let policy = PalwPanelDrawPolicyV1 {
            independence: Some(PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa: 150 }),
            ..sw_policy()
        };
        let sybil = |bond: &PalwBondKeyV2| (2..=5u64).any(|n| *bond == PalwBondKeyV2(bond_outpoint(n)));
        let honest = |bond: &PalwBondKeyV2| (11..=18u64).any(|n| *bond == PalwBondKeyV2(bond_outpoint(n)));
        const N: usize = 1 << 12;
        let mut sybil_outsiders = 0usize;
        for i in 0..N as u64 {
            let seats = derive_panel_v2_with_policy(
                &state,
                &PalwPanelParamsV2::new(3, 2, 4).unwrap(),
                &claim_id,
                anchor(i),
                100,
                None,
                false,
                policy,
            )
            .expect("the outsider and two class seats");
            assert!(sybil(&seats[0].bond) || honest(&seats[0].bond), "the outsider comes from the floor's population");
            assert!(!seats[1..].iter().any(|s| s.operator_id == seats[0].operator_id), "one operator, one seat");
            sybil_outsiders += usize::from(sybil(&seats[0].bond));
            if i < 32 {
                // The outsider is the population's first stake-outsider entry.
                let eligible = palw_panel_eligible_bonds_judging_v1(
                    &state,
                    &claim_id,
                    &h64(1),
                    100,
                    Some(149),
                    false,
                    None,
                    Some(sw_economy()),
                    3,
                )
                .unwrap()
                .into_iter()
                .filter(|(key, _)| **key != PalwBondKeyV2(bond_outpoint(9)))
                .collect::<Vec<_>>();
                let first = palw_panel_stake_entries_under_v1(
                    &claim_id,
                    anchor(i),
                    &eligible,
                    palw_panel_stake_outsider_ticket_v1,
                    &PalwPanelStakeDrawV1::V1,
                )[0];
                assert_eq!((seats[0].bond, seats[0].operator_id), (first.bond, first.operator_id));
            }
        }
        let p = 520_000.0 / (8.0 * 939_063.0 + 520_000.0);
        assert!(within_4_sigma(sybil_outsiders, N, p), "Sybil outsiders {sybil_outsiders} of {N}, expected {p:.4}");
    }

    // ---- T92: liveness is the lottery's -------------------------------------------------------

    /// **T92 (SW-9): `InsufficientEligibleBonds` under exactly the operator lottery's condition,
    /// over the T88 corpus** — for every draw without an outsider the two refuse together, with
    /// the same `needed` and `available`, and for an outsider-judged claim `NoOutsider` is
    /// shared; any other refusal is the same refusal. **And an operator holding 95% of the
    /// weight sits once and the panel fills.**
    #[test]
    fn t92_liveness_is_the_lotterys_and_a_95_percent_operator_sits_once() {
        let (mut short, mut stake_floor) = (0usize, 0usize);
        for fixture in t88_corpus() {
            for policy in &fixture.policies {
                let lottery = PalwPanelDrawPolicyV1 { stake: None, ..*policy };
                let raced = PalwPanelDrawPolicyV1 { stake: Some(PalwPanelStakeDrawV1::V1), ..*policy };
                let claim = fixture.state.claim(&fixture.claim_id).unwrap();
                let outsider_judged = policy
                    .independence
                    .filter(|i| i.governs(claim))
                    .is_some_and(|i| crate::palw_state_v2::palw_claim_is_outsider_judged_v1(&fixture.state, claim, Some(i.from_daa)));
                for i in 0..8u64 {
                    let at = BlockHash::from_u64_word(0x9200 + i);
                    let draw = |policy| {
                        derive_panel_v2_with_policy(&fixture.state, &fixture.params, &fixture.claim_id, at, 100, None, false, policy)
                    };
                    let (a, b) = (draw(lottery), draw(raced));
                    stake_floor += usize::from(matches!(b, Err(PalwPanelV2Error::InsufficientEligibleStake { .. })));
                    match (&a, &b) {
                        (Err(PalwPanelV2Error::NoOutsider(x)), other) | (other, Err(PalwPanelV2Error::NoOutsider(x))) => {
                            assert_eq!(other, &Err(PalwPanelV2Error::NoOutsider(*x)), "{}: NoOutsider together", fixture.name);
                        }
                        _ if outsider_judged => {}
                        (Err(e @ PalwPanelV2Error::InsufficientEligibleBonds { .. }), other)
                        | (other, Err(e @ PalwPanelV2Error::InsufficientEligibleBonds { .. })) => {
                            short += 1;
                            assert_eq!(other, &Err(e.clone()), "{}: InsufficientEligibleBonds together", fixture.name);
                        }
                        (Ok(_), Ok(_)) | (Ok(_), Err(PalwPanelV2Error::InsufficientEligibleStake { .. })) => {}
                        (x, y) => assert_eq!(x, y, "{}: every other refusal is the same refusal", fixture.name),
                    }
                }
            }
        }
        assert!(short > 50, "the corpus reaches the short-panel refusal: {short}");
        eprintln!("T92: {short} short draws refused together; {stake_floor} stake-floor refusals where the lottery bound");

        // The 95% operator: 1,000,000 MSK (two bonds, 700k + 600k, capped) against ten operators
        // of 5,263 MSK (52,630 MSK in all), on a network whose floor is 1,000 MSK.
        let mut rows = vec![(20, 0x50, 700_000 * MSK), (21, 0x50, 600_000 * MSK)];
        rows.extend((0..10u64).map(|i| (30 + i, 0x60 + i, 5_263 * MSK)));
        let (state, claim_id) = sw_state(&rows, &[]);
        let low_floor = PalwPanelDrawPolicyV1 { economy: Some(t88_economy(1_000 * MSK, 500, 0)), ..sw_policy() };
        let whale = op_id(0x50);
        for i in 0..256u64 {
            let seats = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, low_floor)
                .expect("the panel fills");
            assert_eq!(seats.len(), 5, "anchor {i}: five seats");
            let operators: std::collections::BTreeSet<Hash64> = seats.iter().map(|s| s.operator_id).collect();
            assert_eq!(operators.len(), 5, "anchor {i}: one seat an operator");
            assert_eq!(seats.iter().filter(|s| s.operator_id == whale).count(), 1, "anchor {i}: the 95% operator sits, once");
        }
    }

    // ---- T93: the one ledger decides eligibility, never a key ---------------------------------

    /// **T93, the pure part (SW-2, SW-8): changing another bond's commitments moves no eligible
    /// operator's key; only an operator's own eligibility changes the panel.** Nine operators
    /// of 2,000 sompi (weight 1 each under the 1-MSK floor) back a 600-sompi seat under a 500‰
    /// ceiling. Bond 3 produces a 10-pwu claim (reserves 50): every entry is unchanged, key for
    /// key, and so is every panel. Bond 2 produces a 100-pwu claim (reserves 500, so 500 + 600
    /// exceeds its 1,000): its operator leaves the race and every other entry keeps its key and
    /// its order — the panel is the old order without it. Eight of nine operators still clear
    /// SW-10's floor (889‰).
    #[test]
    fn t93_another_bonds_commitments_move_no_key() {
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let rows: Vec<(u64, u64, u64)> = (2..=10u64).map(|b| (b, 0x20 + b, 2_000)).collect();
        let (before, claim_id) = sw_state(&rows, &[]);
        let (quiet, _) = fold_attempt(&before, &sp, 102, 3, 33, 0x23, h64(1), h64(11), 10);
        let (loaded, _) = fold_attempt(&quiet, &sp, 103, 2, 32, 0x22, h64(1), h64(11), 100);
        let economy = Some(t88_economy(1_000, 500, 0));
        let policy = PalwPanelDrawPolicyV1 { economy, stake: Some(PalwPanelStakeDrawV1::V1), ..Default::default() };
        let params = panel_params(); // three seats
        let eligible = |state: &PalwChainStateV2| {
            palw_panel_eligible_bonds_v2(state, &claim_id, 100, None, false, None, economy, 3)
                .unwrap()
                .into_iter()
                .map(|(k, b)| (*k, b.clone()))
                .collect::<Vec<_>>()
        };
        fn as_refs(list: &[(PalwBondKeyV2, PalwBondStateV2)]) -> Vec<(&PalwBondKeyV2, &PalwBondStateV2)> {
            list.iter().map(|(k, b)| (k, b)).collect()
        }
        let (e0, e1, e2) = (eligible(&before), eligible(&quiet), eligible(&loaded));
        assert_eq!((e0.len(), e1.len(), e2.len()), (9, 9, 8), "only bond 2 left the eligible list");
        assert!(
            quiet.reserved_exposure(&PalwBondKeyV2(bond_outpoint(3))) > before.reserved_exposure(&PalwBondKeyV2(bond_outpoint(3)))
        );
        let gone = op_id(0x22);
        let mut changed = 0usize;
        for i in 0..64u64 {
            let at = anchor(i);
            let k0 = palw_panel_stake_entries_v1(&claim_id, at, &as_refs(&e0), &PalwPanelStakeDrawV1::V1);
            let k1 = palw_panel_stake_entries_v1(&claim_id, at, &as_refs(&e1), &PalwPanelStakeDrawV1::V1);
            let k2 = palw_panel_stake_entries_v1(&claim_id, at, &as_refs(&e2), &PalwPanelStakeDrawV1::V1);
            assert_eq!(k0, k1, "anchor {i}: another bond's commitments move no key");
            let without: Vec<PalwPanelStakeEntryV1> = k1.iter().copied().filter(|e| e.operator_id != gone).collect();
            assert_eq!(k2, without, "anchor {i}: the ineligible operator leaves, and nobody else moves");
            let draw = |state| derive_panel_v2_with_policy(state, &params, &claim_id, at, 100, None, false, policy).unwrap();
            assert_eq!(draw(&before), draw(&quiet), "anchor {i}: the same panel");
            let after = draw(&loaded);
            let expected: Vec<PalwPanelSeatV2> =
                without.iter().take(3).map(|e| PalwPanelSeatV2 { bond: e.bond, operator_id: e.operator_id }).collect();
            assert_eq!(after, expected, "anchor {i}: the old order without bond 2's operator");
            changed += usize::from(after != draw(&quiet));
        }
        assert!(changed > 0, "bond 2 must have sat somewhere, or its leaving is vacuous");
    }

    // ---- T94: SW-10 --------------------------------------------------------------------------

    /// **T94 (SW-10): the eligible-stake floor.** The eight genesis seats beside five idle
    /// floor-sized operators, with `k` genesis seats SATURATED — their economy headroom
    /// spent by work they produced, so the headroom filter drops them while the base keeps them.
    /// `k = 1` binds (7,223,441 of 8,162,504 MSK, 885‰); `k = 2` refuses (6,284,378: 770‰); and
    /// SW-A1's cliff, seven saturated, refuses where the lottery seats at least four idle Sybils
    /// in five seats. Eight genesis seats alone bind at exactly 875‰ (seven eligible) and refuse at
    /// six. `stake: None` never refuses for stake.
    #[test]
    fn t94_the_eligible_stake_floor() {
        let g = 939_063u128;
        for (small, saturated, expected) in [
            (5u64, vec![], Ok(())),
            (5, vec![2u64], Ok(())),
            (5, vec![2, 3], Err((6 * g + 650_000, 8 * g + 650_000))),
            (5, vec![2, 3, 4, 5, 6, 7, 8], Err((g + 650_000, 8 * g + 650_000))),
            (0, vec![2], Ok(())),
            (0, vec![2, 3], Err((6 * g, 8 * g))),
        ] {
            let (state, claim_id) = sw_state(&genesis_and_small(small), &saturated);
            let (eligible, base) = populations(&state, &claim_id);
            assert_eq!(base.len(), 8 + small as usize, "the base keeps every seat");
            assert_eq!(eligible.len(), base.len() - saturated.len(), "the headroom drops exactly the saturated seats");
            for i in 0..8u64 {
                let raced = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, sw_policy());
                match expected {
                    Ok(()) => assert!(raced.is_ok(), "{small} small, {saturated:?} saturated: binds, got {raced:?}"),
                    Err((eligible, base)) => assert_eq!(
                        raced,
                        Err(PalwPanelV2Error::InsufficientEligibleStake { eligible, base }),
                        "{small} small, {saturated:?} saturated: refused"
                    ),
                }
                // Below the fence the same state draws whenever five operators are eligible.
                let lottery = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, lottery_policy());
                assert!(lottery.is_ok(), "{small} small, {saturated:?} saturated: stake None never refuses for stake");
                if saturated.len() == 7 {
                    let sybils = lottery.unwrap().iter().filter(|s| s.operator_id != op_id(0x49)).count();
                    assert!(sybils >= 4, "SW-A1: the lottery's panel is the idle Sybils'");
                }
            }
        }
        // The rule itself: exactly at the floor binds; one short refuses; no overflow at the top.
        assert_eq!(palw_panel_stake_floor_v1(875, 1_000, 875), Ok(()));
        assert_eq!(
            palw_panel_stake_floor_v1(874, 1_000, 875),
            Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 874, base: 1_000 })
        );
        assert_eq!(palw_panel_stake_floor_v1(7 * g, 8 * g, 875), Ok(()), "7 of 8 genesis seats is exactly 875‰");
        assert_eq!(palw_panel_stake_floor_v1(0, 0, 875), Ok(()), "an empty base refuses nothing (the count refused first)");
        assert!(palw_panel_stake_floor_v1(u128::MAX / 2, u128::MAX / 2, 875).is_ok());
    }

    /// **T94 (SW-10): the class's base keeps the seats the Valid lock refuses.** The route
    /// matrix's Valid-lock filter ([`PalwPanelValidLockV1`]) is load-dependent like the
    /// economy headroom: a seat already standing behind the live locks of claims it judged is
    /// exactly the "locked honest seat" SW-A1 is about, so it leaves the eligible list and stays
    /// in the base — the base is never filtered by `lock.admits`. The test above spends seats'
    /// headroom; this one leaves every headroom whole and locks seats instead: a live slashable
    /// lock of `939,063 − 50,000` MSK on genesis seats, under a Valid lock of 100,000 MSK the
    /// idle floor-sized operators can post. Two locked seats refuse with the exact split
    /// (`6 × 939,063` against `8 × 939,063`: 750‰; with five 130k operators beside them 770‰),
    /// one locked seat binds (exactly 875‰; 885‰), and the locked seat never sits. Applying the
    /// lock to the base as well would make base equal eligible and bind every row — SW-A1's
    /// cliff back for locked seats — and fails here. `stake: None` draws every row.
    #[test]
    fn t94_the_class_base_keeps_seats_the_valid_lock_refuses() {
        use crate::palw_panel_var_v1::PalwSlashableLockV1;
        use crate::palw_state_v2::PalwStateCarriageV2;
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let g = 939_063u128;
        let lock = PalwPanelValidLockV1 {
            required: 100_000 * MSK as u128,
            now_daa: 103,
            settled_anchor_depth: None,
            window_court: sp.window_court(),
            rcore: None,
        };
        let with_lock = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..sw_policy() };
        let lottery_with_lock = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..lottery_policy() };
        for (small, locked, expected) in [
            (0u64, vec![], Ok(())),
            (0, vec![2u64], Ok(())),
            (0, vec![2, 3], Err((6 * g, 8 * g))),
            (5, vec![2], Ok(())),
            (5, vec![2, 3], Err((6 * g + 650_000, 8 * g + 650_000))),
        ] {
            let (unlocked, claim_id) = sw_state(&genesis_and_small(small), &[]);
            // A live lock on each seat in `locked`, standing on all of its posted collateral but
            // 50,000 MSK — below the Valid lock's 100,000, so `lock.admits` refuses it.
            let mut carriage = PalwStateCarriageV2::from_state(&unlocked);
            for b in &locked {
                carriage.slashable_locks.insert(
                    (PalwBondKeyV2(bond_outpoint(*b)), h64(0x10C4)),
                    PalwSlashableLockV1 {
                        claim: h64(0x10C4),
                        amount: (GENESIS_SEAT - 50_000 * MSK) as u128,
                        expiry_daa: 1_000_000,
                        settled_at_final: 0,
                        // The v22 skeleton's fields (71748a97): unwritten until S-3.
                        attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                        segments: 0,
                    },
                );
            }
            let state = carriage.into_state(&sp, None).expect("consistent");
            let (eligible, base) = populations(&state, &claim_id);
            assert_eq!(eligible.len(), 8 + small as usize, "the lock spends no headroom: every seat passes it");
            assert_eq!(base.len(), eligible.len());
            let refused: Vec<&PalwBondKeyV2> = eligible.iter().map(|(key, _)| *key).filter(|key| !lock.admits(&state, key)).collect();
            let expected_refused: Vec<PalwBondKeyV2> = locked.iter().map(|b| PalwBondKeyV2(bond_outpoint(*b))).collect();
            assert_eq!(
                refused,
                expected_refused.iter().collect::<Vec<_>>(),
                "{small} small: the Valid lock refuses exactly the locked seats"
            );
            for i in 0..8u64 {
                let raced = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, with_lock);
                match expected {
                    Ok(()) => {
                        let seats = raced.unwrap_or_else(|e| panic!("{small} small, {locked:?} locked, anchor {i}: binds, got {e:?}"));
                        assert_eq!(seats.len(), 5);
                        assert!(
                            seats.iter().all(|seat| !expected_refused.contains(&seat.bond)),
                            "{small} small, {locked:?} locked, anchor {i}: a locked seat never sits"
                        );
                    }
                    Err((eligible, base)) => assert_eq!(
                        raced,
                        Err(PalwPanelV2Error::InsufficientEligibleStake { eligible, base }),
                        "{small} small, {locked:?} locked, anchor {i}: refused, the base keeping the locked seats"
                    ),
                }
                // Without the lock in the policy the same state binds: the refusal is the lock's.
                assert!(derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, sw_policy()).is_ok());
                // Below the fence the lock filters the same seats and six operators still fill five.
                let lottery = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, lottery_with_lock);
                let lottery =
                    lottery.unwrap_or_else(|e| panic!("{small} small, {locked:?} locked: stake None never refuses for stake: {e:?}"));
                assert!(lottery.iter().all(|seat| !expected_refused.contains(&seat.bond)));
            }
        }
    }

    /// **The integration's rule (S-3 × SW): past `palw_rcore_plus` the one ledger decides WHETHER a
    /// bond is drawn, posted stake decides HOW OFTEN.** Under S-3's one-ledger seat filter
    /// (`PalwPanelValidLockV1::rcore`: `committed + eligibility ≤ collateral × 500‰`) the economy's
    /// headroom is asked of no list, so the eligible list before the lock IS SW-10's base; the
    /// filter then drops exactly the seats whose one-ledger commitments leave no room — a seat
    /// saturated by its own claim's reservation, or standing behind a live lock — and the base keeps
    /// them. So T94's two tables hold unchanged under the filter that replaces the route matrix's
    /// posted-collateral lock: one loaded genesis seat binds (7/8, and 885‰ beside five 130k
    /// operators), two refuse with the exact split, and a loaded seat never sits. The filter's
    /// `eligibility` is 50,000 MSK, which a 130k operator's 65,000 MSK ceiling covers.
    #[test]
    fn t94_under_the_one_ledger_filter_free_stake_decides_whether_and_posted_stake_how_often() {
        use crate::palw_panel_var_v1::PalwSlashableLockV1;
        use crate::palw_state_v2::PalwStateCarriageV2;
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let g = 939_063u128;
        let filter = PalwPanelValidLockV1 {
            // Unread under the one-ledger filter (S-3): a lock no bond could post shows it.
            required: u128::MAX,
            now_daa: 103,
            settled_anchor_depth: None,
            window_court: sp.window_court(),
            rcore: Some(PalwRcoreSeatFilterV1 {
                gain_scale: Default::default(),
                eligibility: 50_000 * MSK as u128,
                ceiling_permille: 500,
                resolved_locks_off_ceiling: false,
                accuser_reserve: 0,
                held_charge_floor: None,
            }),
        };
        let policy = PalwPanelDrawPolicyV1 { valid_lock: Some(filter), ..sw_policy() };
        let key = |b: u64| PalwBondKeyV2(bond_outpoint(b));
        // Loaded by a live lock (T94's second table) or by its own claim's reservation (T94's
        // first): the one ledger reads both.
        let locked = |small: u64, loaded: &[u64]| {
            let (unlocked, claim_id) = sw_state(&genesis_and_small(small), &[]);
            let mut carriage = PalwStateCarriageV2::from_state(&unlocked);
            for b in loaded {
                carriage.slashable_locks.insert(
                    (key(*b), h64(0x10C4)),
                    PalwSlashableLockV1 {
                        claim: h64(0x10C4),
                        amount: (GENESIS_SEAT - 50_000 * MSK) as u128,
                        expiry_daa: 1_000_000,
                        settled_at_final: 0,
                        attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                        segments: 0,
                    },
                );
            }
            (carriage.into_state(&sp, None).expect("consistent"), claim_id)
        };
        let saturated = |small: u64, loaded: &[u64]| sw_state(&genesis_and_small(small), loaded);
        for (how, load) in [
            ("locked", &locked as &dyn Fn(u64, &[u64]) -> (PalwChainStateV2, Hash64)),
            ("saturated", &saturated as &dyn Fn(u64, &[u64]) -> (PalwChainStateV2, Hash64)),
        ] {
            for (small, loaded, expected) in [
                (0u64, vec![], Ok(())),
                (0, vec![2u64], Ok(())),
                (0, vec![2, 3], Err((6 * g, 8 * g))),
                (5, vec![2], Ok(())),
                (5, vec![2, 3], Err((6 * g + 650_000, 8 * g + 650_000))),
            ] {
                let label = format!("{how}: {small} small, {loaded:?} loaded");
                let (state, claim_id) = load(small, &loaded);
                let before_lock = palw_panel_eligible_bonds_judging_v2(
                    &state,
                    &claim_id,
                    &h64(1),
                    100,
                    None,
                    false,
                    None,
                    Some(sw_economy()),
                    5,
                    false,
                )
                .unwrap();
                let (_, base) = populations(&state, &claim_id);
                assert_eq!(
                    before_lock.iter().map(|(k, _)| **k).collect::<Vec<_>>(),
                    base.iter().map(|(k, _)| **k).collect::<Vec<_>>(),
                    "{label}: without the economy's headroom the list before the lock is SW-10's base"
                );
                assert_eq!(base.len(), 8 + small as usize, "{label}: the base keeps every loaded seat");
                let refused: Vec<PalwBondKeyV2> = base.iter().map(|(k, _)| **k).filter(|k| !filter.admits(&state, k)).collect();
                assert_eq!(refused, loaded.iter().map(|b| key(*b)).collect::<Vec<_>>(), "{label}: the filter drops the loaded seats");
                for i in 0..8u64 {
                    let raced = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, policy);
                    match expected {
                        Ok(()) => {
                            let seats = raced.unwrap_or_else(|e| panic!("{label}, anchor {i}: binds, got {e:?}"));
                            assert_eq!(seats.len(), 5);
                            assert!(
                                seats.iter().all(|seat| !refused.contains(&seat.bond)),
                                "{label}, anchor {i}: a loaded seat never sits"
                            );
                        }
                        Err((eligible, base)) => assert_eq!(
                            raced,
                            Err(PalwPanelV2Error::InsufficientEligibleStake { eligible, base }),
                            "{label}, anchor {i}: refused, the base keeping the loaded seats"
                        ),
                    }
                }
            }
        }
    }

    /// **SW-10 for the outsider, and the refusal order.** The outsider's population is the
    /// floor's minus the registrant: honest operators of 130k and the registrant's Sybils of
    /// 939k. A Valid lock of 500,000 MSK drops every honest bond (posted below it), which leaves
    /// the outsider's eligible weight at `4 × 939,063` of a base of `4 × 939,063 + 8 × 130,000`;
    /// the executor (bond 1, a 939k floor seat) adds its own term to both sides (M4 review
    /// finding 1), so the floor compares `5 × 939,063` with `5 × 939,063 + 8 × 130,000` (818‰):
    /// refused, where the lottery seats an outsider. And the class's operator count is checked
    /// before either floor, so a short class is `InsufficientEligibleBonds` whatever the floors
    /// say.
    #[test]
    fn sw10_the_outsider_floor_and_the_refusal_order() {
        let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
        let (state, claim_id) = t88_bought_state(&sp, |n| if (11..=18).contains(&n) { FLOOR_SEAT } else { GENESIS_SEAT });
        let lock = PalwPanelValidLockV1 {
            required: 500_000 * MSK as u128,
            now_daa: 103,
            settled_anchor_depth: None,
            window_court: sp.window_court(),
            rcore: None,
        };
        let independence = PalwPanelIndependenceV1 { from_daa: 0, base_class_id: h64(1), anchor_daa: 150 };
        let raced = PalwPanelDrawPolicyV1 { independence: Some(independence), valid_lock: Some(lock), ..sw_policy() };
        let lottery = PalwPanelDrawPolicyV1 { stake: None, ..raced };
        let g = 939_063u128;
        for i in 0..8u64 {
            let three = PalwPanelParamsV2::new(3, 2, 4).unwrap();
            assert_eq!(
                derive_panel_v2_with_policy(&state, &three, &claim_id, anchor(i), 100, None, false, raced),
                Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 5 * g, base: 5 * g + 8 * 130_000 }),
                "anchor {i}: the outsider's floor refuses, the executor's term on both sides"
            );
            assert!(derive_panel_v2_with_policy(&state, &three, &claim_id, anchor(i), 100, None, false, lottery).is_ok());
            assert_eq!(
                palw_panel_outsider_seat_v1(&state, &three, &claim_id, anchor(i), 100, Some(149), false, &raced, &independence),
                Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 5 * g, base: 5 * g + 8 * 130_000 }),
                "the public outsider seat applies its own floor"
            );
        }
        // The order inside the race: a short class is refused by count before any floor.
        let (plain, plain_claim) = sw_state(&genesis_and_small(0), &[]);
        let (eligible, base) = populations(&plain, &plain_claim);
        let stake = PalwPanelStakeDrawV1::V1;
        assert_eq!(
            palw_panel_stake_race_with_v1(9, &plain_claim, anchor(0), &eligible, &base, 0, &stake, Some((0, 1)), None),
            Err(PalwPanelV2Error::InsufficientEligibleBonds { needed: 9, available: 8 })
        );
        assert_eq!(
            palw_panel_stake_race_with_v1(5, &plain_claim, anchor(0), &eligible, &base, 0, &stake, Some((0, 1)), None),
            Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 0, base: 1 })
        );
    }

    // ---- SW-9: the room's effective ready count ------------------------------------------------

    /// **SW-9 / T91's arithmetic: `ready_eff = min(ready, max(seat_count, ⌊ΣW / W_max⌋))` over
    /// capped weights.** The ADR's three examples: the eight genesis seats give 8; with forty
    /// 130k operators beside them, 13 of 48; and one 20M operator beside the eight genesis seats
    /// — 5 in §3.14's pre-cap text — gives **8** once its weight is capped at 1,000,000
    /// (`⌊8,512,504 / 1,000,000⌋ = 8`; `v31_review_numbers.out`: "one 20M operator + 8 genesis,
    /// capped: ready_eff 8"). SW-A6's residual: a class held by forty 130k operators counts 40,
    /// and one ready operator at the cap cuts it to 6.
    #[test]
    fn sw9_ready_eff_over_capped_weights() {
        let cap = PalwPanelStakeDrawV1::V1.weight_cap_msk;
        let w = |msk: u128| palw_draw_operator_weight_msk_v1(msk, cap);
        let genesis = vec![w(939_063); 8];
        assert_eq!(palw_panel_ready_eff_v1(&genesis, 5), 8);
        let mut wide = genesis.clone();
        wide.extend(vec![w(130_000); 40]);
        assert_eq!(palw_panel_ready_eff_v1(&wide, 5), 13, "8 genesis + 40 × 130k: 13 of 48");
        let mut whale = vec![w(20_000_000)];
        whale.extend(genesis.clone());
        assert_eq!(whale[0], 1_000_000, "the 20M operator weighs the cap");
        assert_eq!(palw_panel_ready_eff_v1(&whale, 5), 8, "one 20M operator + 8 genesis: 8 capped (5 uncapped)");
        assert_eq!(
            palw_panel_ready_eff_v1(&[20_000_000, 939_063, 939_063, 939_063, 939_063, 939_063, 939_063, 939_063, 939_063], 5),
            5,
            "uncapped, as §3.14's text had it"
        );
        let mut wide_whale = wide.clone();
        wide_whale.push(w(20_000_000));
        assert_eq!(palw_panel_ready_eff_v1(&wide_whale, 5), 13, "SW-A6: the cap keeps that lever at 13");
        let smalls = vec![w(130_000); 40];
        assert_eq!(palw_panel_ready_eff_v1(&smalls, 5), 40);
        let mut smalls_whale = smalls.clone();
        smalls_whale.push(w(1_000_000));
        assert_eq!(palw_panel_ready_eff_v1(&smalls_whale, 5), 6, "SW-A6's residual: 40 → 6");
        assert_eq!(palw_panel_ready_eff_v1(&[], 5), 0, "no ready operator");
        assert_eq!(palw_panel_ready_eff_v1(&[w(130_000); 3], 5), 3, "never more than the ready operators");
    }

    // ---- lane F1 (post-launch fence): the panel seed is the anchor ATTEMPT's execution --------

    /// **The anchor-identity re-roll, closed at the pure layer** (`wf_72c1a397-e23`; the fence
    /// is `Params::palw_panel_seed_execution`, which reaches this layer as
    /// `PalwAnchorFactV2::anchor_execution`). The processor's end (the anchor walk reading the
    /// commitment off the anchor header, on a real testnet-12 chain that crosses the fence) is
    /// `t12_panel_seed_fence`; the attacker's grind against it is
    /// `consensus/core/tests/void_after_panel_probe.rs` (T2a, T3a, T3b) and
    /// `consensus/core/tests/void_after_panel_adv_verify.rs` (a1–a4).
    mod panel_seed_2026_09_25 {
        use super::*;
        use crate::palw_state_v2::{PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras};
        use crate::palw_verification_v2::palw_segment_assignment_v2;

        fn exec(i: u64) -> Hash64 {
            h64(0xE5EC_0000 + i)
        }

        fn fact(block: BlockHash, slot: u64, execution: Option<Hash64>) -> PalwAnchorFactV2 {
            PalwAnchorFactV2 { anchor_block: block, anchor_daa: slot, predecessor_daa: slot - 1, anchor_execution: execution }
        }

        /// **The seed is `H(execution ‖ claim)`: every identity of one anchor attempt names one seed.**
        /// Below the fence (`None`) it is the anchor block, byte for byte; above it no block identity
        /// enters, a different execution or a different claim is a different seed, and the seed is
        /// never the execution itself or the identity.
        #[test]
        fn the_seed_is_the_execution_and_the_claim_never_the_identity() {
            let claim = h64(0xC1A1);
            let one = fact(anchor(0), 105, Some(exec(0))).panel_seed(&claim);
            for i in 0..256u64 {
                assert_eq!(fact(anchor(i), 105, Some(exec(0))).panel_seed(&claim), one, "identity {i}: one execution, one seed");
                assert_eq!(fact(anchor(i), 105, None).panel_seed(&claim), anchor(i), "identity {i}: below the fence, the block");
            }
            assert_eq!(one, palw_panel_draw_seed_v1(&exec(0), &claim));
            assert_ne!(one, exec(0));
            assert_ne!(one, anchor(0));
            assert_ne!(fact(anchor(0), 105, Some(exec(1))).panel_seed(&claim), one, "another execution, another seed");
            assert_ne!(fact(anchor(0), 105, Some(exec(0))).panel_seed(&h64(0xC1A2)), one, "another claim, another seed");
            // The anchor DAA and predecessor are the walk's facts, never seed inputs.
            assert_eq!(fact(anchor(0), 999, Some(exec(0))).panel_seed(&claim), one);
        }

        /// **The gate demands the seed and the panel drawn from it; the fold stores the seed and the
        /// assignment reads it back.** On a stake-draw state, 32 anchor identities carrying ONE
        /// execution: every one derives the same panel, the gate accepts `PanelBound { anchor: seed }`
        /// with it in that block (SW-8) and refuses the block identity as the anchor
        /// (`AnchorMismatch`) and any panel drawn from the identity where it differs
        /// (`PanelMismatch`); the fold stores `anchor = seed`, and the segment assignment (and so the
        /// S3 sample and S2 door, which read the same field) is one assignment for all 32.
        #[test]
        fn the_gate_demands_the_seed_and_the_fold_stores_it() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (state, claim_id) = sw_state(&genesis_and_small(5), &[]);
            let slot = state.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
            let extras = PalwTransitionExtrasV1 { panel_economy_active: true, ..Default::default() };
            let (mut panels, mut assignments, mut identity_differs) =
                (std::collections::BTreeSet::new(), std::collections::BTreeSet::new(), 0usize);
            for i in 0..32u64 {
                let f = fact(anchor(i), slot, Some(exec(7)));
                let point = ctx(0x5700_0000 + i, slot, 9_000);
                let seed = f.panel_seed(&claim_id);
                let validate = |proposed: Hash64, seats: &[PalwPanelSeatV2]| {
                    validate_panel_bound_v2_with_policy(
                        &state,
                        &five(),
                        &sp,
                        &point,
                        &claim_id,
                        &f,
                        proposed,
                        seats,
                        None,
                        false,
                        sw_policy(),
                        None,
                    )
                };
                let seats = derive_panel_v2_with_policy(&state, &five(), &claim_id, seed, 100, None, false, sw_policy()).unwrap();
                assert_eq!(validate(seed, &seats), Ok(()), "identity {i}: build = accept on the seed");
                assert!(
                    matches!(validate(anchor(i), &seats), Err(PalwPanelV2Error::AnchorMismatch(_))),
                    "identity {i}: the block identity is not the panel's anchor any more"
                );
                let by_identity =
                    derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i), 100, None, false, sw_policy()).unwrap();
                if by_identity != seats {
                    identity_differs += 1;
                    assert_eq!(validate(seed, &by_identity), Err(PalwPanelV2Error::PanelMismatch), "identity {i}");
                }
                let object = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: seed, seats: seats.clone() };
                let (folded, _) =
                    apply_palw_transition_v2_with_extras(&state, &sp, &point, &[object], None, false, false, false, false, &extras)
                        .expect("the anchor block folds its binding");
                let stored = folded.panel(&claim_id).expect("bound").clone();
                assert_eq!((stored.anchor, &stored.seats), (seed, &seats), "identity {i}: the fold stores the seed");
                assignments.insert(format!("{:?}", palw_segment_assignment_v2(stored.anchor, claim_id, stored.seats.len() as u16)));
                panels.insert(seats.iter().map(|seat| seat.bond).collect::<Vec<_>>());
            }
            assert_eq!(panels.len(), 1, "32 identities of one anchor attempt draw ONE panel");
            assert_eq!(assignments.len(), 1, "…and one segment assignment");
            assert!(identity_differs > 0, "the identity-keyed draw moved with the identity, or the refusal is vacuous");
        }

        /// **Honest paths keep their law.** The stake race over execution-keyed seeds seats each
        /// operator at its exact inclusion probability (successive sampling, `exact_law`), within 4σ
        /// over 2^12 executions — eight genesis seats and five floor operators — exactly as the
        /// identity-keyed draw did: the seed changes who can steer the draw, not its distribution.
        #[test]
        fn execution_keyed_panels_keep_the_draw_s_law() {
            let (state, claim_id) = sw_state(&genesis_and_small(5), &[]);
            let rows = genesis_and_small(5);
            let mut weights: Vec<f64> = vec![939_063.0; 8];
            weights.extend([130_000.0; 5]);
            let marked = vec![false; weights.len()];
            let (inclusion, _) = exact_law(&weights, 5, &marked);
            const N: usize = 1 << 12;
            for (label, keyed) in [("execution-keyed", true), ("identity-keyed", false)] {
                let mut seated = vec![0usize; rows.len()];
                let mut distinct = std::collections::BTreeSet::new();
                for i in 0..N as u64 {
                    let seed = if keyed { palw_panel_draw_seed_v1(&exec(i), &claim_id) } else { anchor(i) };
                    let seats = derive_panel_v2_with_policy(&state, &five(), &claim_id, seed, 100, None, false, sw_policy()).unwrap();
                    for seat in &seats {
                        let index = rows.iter().position(|row| PalwBondKeyV2(bond_outpoint(row.0)) == seat.bond).expect("a row");
                        seated[index] += 1;
                    }
                    distinct.insert(seats.iter().map(|seat| seat.bond).collect::<Vec<_>>());
                }
                for (index, hits) in seated.iter().enumerate() {
                    assert!(
                        within_4_sigma(*hits, N, inclusion[index]),
                        "{label}: bond {} seated {hits} of {N}, exact {:.5}",
                        rows[index].0,
                        inclusion[index]
                    );
                }
                assert!(distinct.len() > N / 4, "{label}: {} distinct panels over {N} seeds", distinct.len());
            }
        }
    }

    // ---- M4: the integration (ADR-0152 §3.14 SW-1, SW-8, SW-9, SW-10 at the fold) ----------

    /// **ADR-0152 M4's core half** — the stake draw where the processor wires it: one state per
    /// block (SW-8), bind only in the anchor block (SW-8), the room's `ready_eff` (SW-9), and the
    /// fold's side of SW-10's halt. The processor half (the resolver, the chain's derivation on a
    /// real testnet-12 chain) is `consensus/src/pipeline/virtual_processor/tests/
    /// t12_stake_draw_integration.rs`; the room through the fold and op 186 on testnet-12's rows is
    /// `consensus/core/tests/adr0152_sw9_ready_eff_room.rs`.
    mod m4 {
        use super::*;
        use crate::palw_state_v2::{
            PalwBondStatusV2, PalwClaimPhaseV2, PalwStateCarriageV2, PalwTransitionExtrasV1, PalwVoidReasonV2,
            apply_palw_transition_v2_with_extras, palw_v2_apply_one_object_v1, palw_v2_pre_object_base_v1, revert_delta_v2,
        };

        fn key(b: u64) -> PalwBondKeyV2 {
            PalwBondKeyV2(bond_outpoint(b))
        }

        /// The fold's extras past the panel economy (seats go on duty and hold their exposure),
        /// everything else dormant — enough for a binding to load the seats it seats.
        fn duty_extras() -> PalwTransitionExtrasV1 {
            PalwTransitionExtrasV1 { panel_economy_active: true, ..Default::default() }
        }

        /// A block that is `word`'s own anchor at `daa`: the anchor fact every claim anchored there
        /// reads (its predecessor below both slots), and the chain point that binds in it.
        fn anchored_at(word: u64, daa: u64, predecessor_daa: u64) -> (PalwAnchorFactV2, PalwBlockContextV2) {
            (
                PalwAnchorFactV2 {
                    anchor_block: BlockHash::from_u64_word(word),
                    anchor_daa: daa,
                    predecessor_daa,
                    anchor_execution: None,
                },
                ctx(word, daa, daa),
            )
        }

        #[allow(clippy::too_many_arguments)]
        fn validate(
            state: &PalwChainStateV2,
            sp: &PalwStateParamsV2,
            point: &PalwBlockContextV2,
            claim_id: &Hash64,
            fact: &PalwAnchorFactV2,
            proposed_anchor: Hash64,
            seats: &[PalwPanelSeatV2],
            policy: PalwPanelDrawPolicyV1,
        ) -> Result<(), PalwPanelV2Error> {
            validate_panel_bound_v2_with_policy(
                state,
                &five(),
                sp,
                point,
                claim_id,
                fact,
                proposed_anchor,
                seats,
                None,
                false,
                policy,
                None,
            )
        }

        fn draw(
            state: &PalwChainStateV2,
            claim_id: &Hash64,
            at: BlockHash,
            policy: PalwPanelDrawPolicyV1,
        ) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
            derive_panel_v2_with_policy(state, &five(), claim_id, at, 100, None, false, policy)
        }

        /// **T91 through the counting function (SW-9):** `palw_panel_ready_eff_of_bonds_v1` over
        /// the bonds of a registry reproduces §3.14's examples from the bonds' own posted
        /// collateral, grouped per operator: 8 genesis seats → 8; with forty 130k operators, 13 of
        /// 48; one 20M operator more, still 13 (capped); one 20M operator beside the genesis seats,
        /// 8; forty 130k alone, 40, and one at the cap beside them, 6. One operator's two bonds are
        /// ONE ready operator whose weight sums before the cap.
        #[test]
        fn t91_ready_eff_counts_ready_operators_over_capped_weights() {
            let mut rows = genesis_and_small(40);
            rows.push((60, 0x90, 20_000_000 * MSK));
            rows.push((61, 0x91, 1_000_000 * MSK));
            rows.push((62, 0x92, 600_000 * MSK));
            rows.push((63, 0x92, 600_000 * MSK));
            let (state, _) = sw_state(&rows, &[]);
            let eff = |bonds: &[u64]| {
                palw_panel_ready_eff_of_bonds_v1(
                    bonds.iter().map(|b| state.bond(&key(*b)).expect("registered")),
                    5,
                    &PalwPanelStakeDrawV1::V1,
                )
            };
            let genesis: Vec<u64> = (2..=9).collect();
            let smalls: Vec<u64> = (10..50).collect();
            let with = |a: &[u64], b: &[u64]| [a, b].concat();
            assert_eq!(eff(&genesis), 8, "8 genesis seats");
            assert_eq!(eff(&with(&genesis, &smalls)), 13, "8 genesis + 40 × 130k: 13 of 48");
            assert_eq!(eff(&with(&with(&genesis, &smalls), &[60])), 13, "one ready 20M operator more: still 13 (capped)");
            assert_eq!(eff(&with(&[60], &genesis)), 8, "one 20M operator + 8 genesis: 8 (5 uncapped)");
            assert_eq!(eff(&smalls), 40, "40 × 130k");
            assert_eq!(eff(&with(&smalls, &[61])), 6, "SW-A6's residual: one operator at the cap cuts 40 to 6");
            assert_eq!(
                eff(&with(&smalls, &[62, 63])),
                6,
                "one operator's two 600k bonds: one ready operator, 1.2M summed and capped at 1M"
            );
            assert_eq!(eff(&[]), 0);
        }

        /// **T89 (SW-8): two claims bound in one block — the second sees the first's duties.**
        /// Nine operators of equal weight beside the executor; one of them ("thin", bond 2) has
        /// economy headroom for exactly ONE seat of these claims. Both claims anchor at the same
        /// block. The processor derives them the way the acceptance walk validates them: on the
        /// block's pre-object base, advanced by the first binding in claim-id order. Wherever thin
        /// sits on the first panel, it is not eligible for the second (its duty took its
        /// headroom), and:
        ///
        /// * build = accept: the second panel derived on the advanced state is what the gate
        ///   accepts there (and the first on the base);
        /// * the parent-state derivation the chain ran before SW-8 draws thin onto the second panel
        ///   at some anchors, and the gate refuses that panel on the state it actually reads
        ///   (`PanelMismatch`) — the drop that used to send the claim to a later block's retry;
        /// * fold: the block folding both bindings stores exactly the two derived panels.
        #[test]
        fn t89_two_claims_in_one_block_the_second_sees_the_first_s_duties() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let mut rows: Vec<(u64, u64, u64)> = (3..=10u64).map(|b| (b, 0x20 + b, 1_000_000)).collect();
            rows.insert(0, (2, 0x22, 2_000));
            let (s0, first_claim) = sw_state(&rows, &[]);
            let (s0, second_claim) = fold_attempt(&s0, &sp, 102, 1, 7, 0x21, h64(1), h64(11), 40);
            assert_ne!(first_claim, second_claim, "two claims");
            let (a, b) = if first_claim < second_claim { (first_claim, second_claim) } else { (second_claim, first_claim) };
            let economy = Some(t88_economy(1_000, 500, 0));
            let policy = PalwPanelDrawPolicyV1 { economy, stake: Some(PalwPanelStakeDrawV1::V1), ..Default::default() };
            let thin = key(2);
            let (mut thin_first, mut legacy_refused) = (0usize, 0usize);
            for i in 0..48u64 {
                let word = 0x5A00_0000 + i;
                // Both claims' slots (105, 106) lie behind one anchor at 106.
                let (fact, point) = anchored_at(word, 106, 104);
                let at = fact.anchor_block;
                let base = palw_v2_pre_object_base_v1(&s0, &sp, &point, false, false, false, false, &duty_extras()).unwrap();
                let p1 = draw(&base, &a, at, policy).unwrap();
                assert_eq!(validate(&base, &sp, &point, &a, &fact, at, &p1, policy), Ok(()), "anchor {i}: build = accept, first");
                let first = PalwConsensusObjectV2::PanelBound { claim: a, anchor: at, seats: p1.clone() };
                let advanced = palw_v2_apply_one_object_v1(&base, &sp, &point, &first, false, false, false, false, &duty_extras())
                    .expect("the first binding folds");
                let p2 = draw(&advanced, &b, at, policy).unwrap();
                assert_eq!(
                    validate(&advanced, &sp, &point, &b, &fact, at, &p2, policy),
                    Ok(()),
                    "anchor {i}: build = accept, second, on the advanced state"
                );
                if p1.iter().any(|s| s.bond == thin) {
                    thin_first += 1;
                    assert!(advanced.reserved_exposure(&thin) > base.reserved_exposure(&thin), "the first panel's duty loads thin");
                    assert!(!p2.iter().any(|s| s.bond == thin), "anchor {i}: thin has no headroom left for the second");
                    let legacy = draw(&base, &b, at, policy).unwrap();
                    if legacy != p2 {
                        legacy_refused += 1;
                        assert_eq!(
                            validate(&advanced, &sp, &point, &b, &fact, at, &legacy, policy),
                            Err(PalwPanelV2Error::PanelMismatch),
                            "anchor {i}: the parent-state panel is refused on the state acceptance reads"
                        );
                    }
                }
                // Fold: the block folding both stores both derived panels.
                let second = PalwConsensusObjectV2::PanelBound { claim: b, anchor: at, seats: p2.clone() };
                let (folded, _) = apply_palw_transition_v2_with_extras(
                    &s0,
                    &sp,
                    &point,
                    &[first, second],
                    None,
                    false,
                    false,
                    false,
                    false,
                    &duty_extras(),
                )
                .expect("the block folds both bindings");
                assert_eq!(folded.panel(&a).map(|p| &p.seats), Some(&p1), "anchor {i}: fold = build, first");
                assert_eq!(folded.panel(&b).map(|p| &p.seats), Some(&p2), "anchor {i}: fold = build, second");
            }
            assert!(
                thin_first > 0 && legacy_refused > 0,
                "the premise must occur: thin first {thin_first}, legacy refused {legacy_refused}"
            );
        }

        /// **T89 (SW-8): the Valid-lock filter excludes a bond, build = accept = fold.** A live
        /// lock leaves genesis seat 2 unable to post the bind's Valid lock; the draw skips it, the
        /// gate accepts the panel under the same policy, and the fold binds it. (T94's twin in the
        /// pure half shows the base keeps the seat for SW-10.)
        #[test]
        fn t89_the_valid_lock_filter_excludes_a_bond_build_accept_fold() {
            use crate::palw_panel_var_v1::PalwSlashableLockV1;
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (unlocked, claim_id) = sw_state(&genesis_and_small(5), &[]);
            let mut carriage = PalwStateCarriageV2::from_state(&unlocked);
            carriage.slashable_locks.insert(
                (key(2), h64(0x10C4)),
                PalwSlashableLockV1 {
                    claim: h64(0x10C4),
                    amount: (GENESIS_SEAT - 50_000 * MSK) as u128,
                    expiry_daa: 1_000_000,
                    settled_at_final: 0,
                    attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                    segments: 0,
                },
            );
            let state = carriage.into_state(&sp, None).expect("consistent");
            let lock = PalwPanelValidLockV1 {
                required: 100_000 * MSK as u128,
                now_daa: 103,
                settled_anchor_depth: None,
                window_court: sp.window_court(),
                rcore: None,
            };
            assert!(!lock.admits(&state, &key(2)) && lock.admits(&state, &key(3)));
            let policy = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..sw_policy() };
            let slot = state.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
            for i in 0..24u64 {
                let (fact, point) = anchored_at(0x5B00_0000 + i, slot, slot - 1);
                let seats = draw(&state, &claim_id, fact.anchor_block, policy).expect("7 of 8 genesis weight stays eligible");
                assert!(!seats.iter().any(|s| s.bond == key(2)), "anchor {i}: the locked seat never sits");
                assert_eq!(validate(&state, &sp, &point, &claim_id, &fact, fact.anchor_block, &seats, policy), Ok(()));
                let object = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: fact.anchor_block, seats: seats.clone() };
                let (folded, _) = apply_palw_transition_v2_with_extras(
                    &state,
                    &sp,
                    &point,
                    &[object],
                    None,
                    false,
                    false,
                    false,
                    false,
                    &duty_extras(),
                )
                .unwrap();
                assert_eq!(folded.panel(&claim_id).map(|p| &p.seats), Some(&seats), "anchor {i}: fold = build");
            }
        }

        /// **T89 (SW-8): a redraw binds in the REDRAW's anchor block, and only there.** A bound
        /// panel whose receipt window closes with no conclusion is revived once (`rebound_daa`),
        /// and its second panel anchors on the sweep. The gate accepts the second panel at the
        /// redraw's anchor block, refuses it at any later block inside the redraw's bind window
        /// (`BindOutsideWindow`, SW-8) and at the FIRST panel's anchor (whose slot the claim has
        /// left); under `stake: None` the later block still binds, as it always could.
        #[test]
        fn t89_a_redraw_binds_only_in_the_redraw_s_anchor_block() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (s0, claim_id) = sw_state(&genesis_and_small(5), &[]);
            let slot = s0.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
            let (fact1, point1) = anchored_at(0x5C00_0001, slot, slot - 1);
            let p1 = draw(&s0, &claim_id, fact1.anchor_block, sw_policy()).unwrap();
            let (s1, _) = apply_palw_transition_v2_with_extras(
                &s0,
                &sp,
                &point1,
                &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: fact1.anchor_block, seats: p1 }],
                None,
                false,
                false,
                false,
                false,
                &duty_extras(),
            )
            .unwrap();
            // The receipt window closes with no conclusion: the sweep revives the claim once.
            let sweep = slot + sp.window_receipt() + 1;
            let (s2, _) = apply_palw_transition_v2_with_extras(
                &s1,
                &sp,
                &ctx(0x5C00_0002, sweep, sweep),
                &[],
                None,
                false,
                false,
                false,
                false,
                &duty_extras(),
            )
            .unwrap();
            let revived = s2.claim(&claim_id).unwrap();
            assert!(matches!(revived.phase, PalwClaimPhaseV2::Provisional), "{:?}", revived.phase);
            assert_eq!(revived.rebound_daa, Some(sweep));
            let slot2 = sweep + five().anchor_delay();
            let (fact2, point2) = anchored_at(0x5C00_0003, slot2, slot2 - 1);
            let p2 = draw(&s2, &claim_id, fact2.anchor_block, sw_policy()).unwrap();
            assert_eq!(validate(&s2, &sp, &point2, &claim_id, &fact2, fact2.anchor_block, &p2, sw_policy()), Ok(()));
            let later = ctx(0x5C00_0004, slot2 + 1, slot2 + 1);
            assert!(matches!(
                validate(&s2, &sp, &later, &claim_id, &fact2, fact2.anchor_block, &p2, sw_policy()),
                Err(PalwPanelV2Error::BindOutsideWindow(_))
            ));
            assert!(
                matches!(
                    validate(&s2, &sp, &point1, &claim_id, &fact1, fact1.anchor_block, &p2, sw_policy()),
                    Err(PalwPanelV2Error::AnchorMismatch(_) | PalwPanelV2Error::BindOutsideWindow(_))
                ),
                "the first panel's anchor is not the redraw's"
            );
            let lottery = draw(&s2, &claim_id, fact2.anchor_block, lottery_policy()).unwrap();
            assert_eq!(validate(&s2, &sp, &later, &claim_id, &fact2, fact2.anchor_block, &lottery, lottery_policy()), Ok(()));
        }

        /// **T89 (SW-8): a reorg across the bind.** The bind folded at anchor A reverts to the
        /// exact parent state; the competing chain's anchor B (the first block at the same slot on
        /// that chain) draws on the same one state, and its gate refuses A's panel (the object
        /// names A) and accepts B's. Nothing about A's draw survives on B's chain.
        #[test]
        fn t89_a_reorg_across_the_bind_re_derives_on_the_new_chain() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (s0, claim_id) = sw_state(&genesis_and_small(5), &[]);
            let slot = s0.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
            let (fact_a, point_a) = anchored_at(0x5D00_000A, slot, slot - 1);
            let (fact_b, point_b) = anchored_at(0x5D00_000B, slot, slot - 1);
            let pa = draw(&s0, &claim_id, fact_a.anchor_block, sw_policy()).unwrap();
            let bound_a = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: fact_a.anchor_block, seats: pa.clone() };
            let (sa, delta) =
                apply_palw_transition_v2_with_extras(&s0, &sp, &point_a, &[bound_a], None, false, false, false, false, &duty_extras())
                    .unwrap();
            assert!(matches!(sa.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }));
            let back = revert_delta_v2(&sa, &delta, &sp).unwrap();
            assert_eq!(back.state_root(), s0.state_root(), "the reorg restores the parent state exactly");
            let pb = draw(&back, &claim_id, fact_b.anchor_block, sw_policy()).unwrap();
            assert_eq!(pb, draw(&s0, &claim_id, fact_b.anchor_block, sw_policy()).unwrap());
            assert!(matches!(
                validate(&back, &sp, &point_b, &claim_id, &fact_b, fact_a.anchor_block, &pa, sw_policy()),
                Err(PalwPanelV2Error::AnchorMismatch(_))
            ));
            assert_eq!(validate(&back, &sp, &point_b, &claim_id, &fact_b, fact_b.anchor_block, &pb, sw_policy()), Ok(()));
            let bound_b = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: fact_b.anchor_block, seats: pb.clone() };
            let (sb, _) = apply_palw_transition_v2_with_extras(
                &back,
                &sp,
                &point_b,
                &[bound_b],
                None,
                false,
                false,
                false,
                false,
                &duty_extras(),
            )
            .unwrap();
            assert_eq!(sb.panel(&claim_id).map(|p| &p.seats), Some(&pb));
        }

        /// **T93 at the fold (SW-8): nothing after the anchor moves a bound panel, and nothing
        /// after it can bind an unbound one.** A panel is derived and folded in its anchor block.
        /// Then, on the chain after it: the attacker's own claims (a Sybil's attempts), a carried
        /// licence's lock on a seat, and a slash of an unrelated bond. The stored panel never
        /// changes, and every operator's key (`L`, `W`) but the slashed one's is what it was — the
        /// one ledger moves eligibility, a slash moves only its own bond's weight. And for a claim
        /// whose draw did NOT bind in its anchor block, the attacker retiring its own seated
        /// Sybil afterwards buys nothing: the panel re-derived on the later state differs, and
        /// the gate refuses it at every later block (`BindOutsideWindow`) — no retry exists.
        #[test]
        fn t93_nothing_after_the_anchor_moves_the_panel() {
            use crate::palw_panel_var_v1::PalwSlashableLockV1;
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (s0, claim_id) = sw_state(&genesis_and_small(5), &[]);
            let slot = s0.claim(&claim_id).unwrap().bind_base_daa() + five().anchor_delay();
            let (fact, point) = anchored_at(0x5E00_0001, slot, slot - 1);
            let at = fact.anchor_block;
            let panel = draw(&s0, &claim_id, at, sw_policy()).unwrap();
            let (bound, _) = apply_palw_transition_v2_with_extras(
                &s0,
                &sp,
                &point,
                &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: at, seats: panel.clone() }],
                None,
                false,
                false,
                false,
                false,
                &duty_extras(),
            )
            .unwrap();
            let keys_of = |state: &PalwChainStateV2| {
                let (eligible, _) = populations(state, &claim_id);
                palw_panel_stake_entries_v1(&claim_id, at, &eligible, &PalwPanelStakeDrawV1::V1)
                    .into_iter()
                    .map(|e| (e.operator_id, (e.neg_log2_q64, e.weight_msk)))
                    .collect::<std::collections::BTreeMap<_, _>>()
            };
            let keys0 = keys_of(&s0);
            // The attacker's own claims: a floor-sized Sybil (bond 10, operator 0x4A) produces.
            let (after_claims, _) = fold_attempt(&bound, &sp, slot + 1, 10, 40, 0x4A, h64(1), h64(11), 40);
            // A carried licence's Valid lock on the first seat.
            let seat = panel[0].bond;
            let mut carriage = PalwStateCarriageV2::from_state(&after_claims);
            carriage.slashable_locks.insert(
                (seat, h64(0x93A1)),
                PalwSlashableLockV1 {
                    claim: h64(0x93A1),
                    amount: 10_000 * MSK as u128,
                    expiry_daa: 1_000_000,
                    settled_at_final: 0,
                    attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                    segments: 0,
                },
            );
            // A slash of an unrelated bond: one that does not sit on this panel.
            let unrelated = (2..=14u64).map(key).find(|k| !panel.iter().any(|s| s.bond == *k)).expect("thirteen bonds, five seats");
            let slashed_operator = carriage.bonds.get(&unrelated).unwrap().operator_id;
            {
                let bond = carriage.bonds.get_mut(&unrelated).unwrap();
                bond.collateral -= 400_000 * MSK;
                bond.slashed += 400_000 * MSK;
            }
            let later = carriage.into_state(&sp, None).expect("consistent");
            for (label, state) in [("the attacker's claims", &after_claims), ("a lock and a slash", &later)] {
                assert_eq!(state.panel(&claim_id).map(|p| &p.seats), Some(&panel), "{label}: the bound panel stands");
                assert!(matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "{label}");
            }
            let keys1 = keys_of(&later);
            for (operator, k) in &keys0 {
                if *operator == slashed_operator {
                    assert_eq!(keys1[operator].0, k.0, "the slashed operator's L is its seed's");
                    continue;
                }
                assert_eq!(keys1.get(operator), Some(k), "no other operator's key moved");
            }

            // The retry path: a draw that did not bind in its anchor block. The attacker retires
            // its own seated Sybil after seeing the anchor; the re-derived panel on the later
            // state differs, and no later block may carry it.
            let mut retry_blocked = 0usize;
            for i in 0..32u64 {
                let (fact, point) = anchored_at(0x5E10_0000 + i, slot, slot - 1);
                let first = draw(&s0, &claim_id, fact.anchor_block, sw_policy()).unwrap();
                let Some(own) = first.iter().find(|s| (10..=14u64).map(key).any(|k| k == s.bond)) else { continue };
                let mut c = PalwStateCarriageV2::from_state(&s0);
                c.bonds.get_mut(&own.bond).unwrap().status = PalwBondStatusV2::Retiring { since_daa: slot, settled_at_since: 0 };
                let retired = c.into_state(&sp, None).expect("consistent");
                let redrawn = draw(&retired, &claim_id, fact.anchor_block, sw_policy()).unwrap();
                assert_ne!(redrawn, first, "anchor {i}: the retirement moved the panel it would re-derive");
                let after = ctx(0x5E20_0000 + i, slot + 1, slot + 1);
                assert!(
                    matches!(
                        validate(&retired, &sp, &after, &claim_id, &fact, fact.anchor_block, &redrawn, sw_policy()),
                        Err(PalwPanelV2Error::BindOutsideWindow(_))
                    ),
                    "anchor {i}: a later block cannot carry the retry"
                );
                // The anchor block itself reads the state before the retirement.
                assert_eq!(validate(&s0, &sp, &point, &claim_id, &fact, fact.anchor_block, &first, sw_policy()), Ok(()));
                retry_blocked += 1;
            }
            assert!(retry_blocked > 0, "a Sybil must have sat somewhere, or the retry case is vacuous");
        }

        /// **T94 at the fold (SW-10 + SW-8): `InsufficientEligibleStake` halts binding, and the
        /// claim voids `BindTimeout` without forfeit — AT its anchor block.** Two of the eight
        /// genesis seats saturated (6/8, 750‰): the draw refuses in the anchor block, and the gate
        /// refuses ANY proposed panel there with the same error (build = accept on the refusal —
        /// the lottery's panel included). Nothing binds, and past `palw_rcore_plus` the anchor
        /// block's own fold voids the claim `BindTimeout` at its step 4c (M4 review finding 3,
        /// DL-1's exact void DAA: the anchor block's), not 600 DAA later at the window: no bond is
        /// slashed, no collateral moves, the executor's reservation is released, and the deadline
        /// index, the carriage rebuild and a revert all agree with it. A block before the slot, or
        /// one past it that may not anchor (`sw8_anchor_delay` `None`: a heartbeat), voids nothing.
        /// The fence-off twin (`None` everywhere) keeps the claim until the bind window's sweep.
        /// `stake: None` binds the same claim.
        #[test]
        fn t94_a_refused_draw_binds_nothing_and_the_claim_voids_bind_timeout_without_forfeit() {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let (s0, claim_id) = sw_state(&genesis_and_small(0), &[2, 3]);
            let claim = s0.claim(&claim_id).unwrap().clone();
            let slot = claim.bind_base_daa() + five().anchor_delay();
            let (fact, point) = anchored_at(0x5F00_0001, slot, slot - 1);
            let g = 939_063u128;
            let refusal = PalwPanelV2Error::InsufficientEligibleStake { eligible: 6 * g, base: 8 * g };
            assert_eq!(draw(&s0, &claim_id, fact.anchor_block, sw_policy()), Err(refusal.clone()));
            let lottery = draw(&s0, &claim_id, fact.anchor_block, lottery_policy()).expect("stake None never refuses for stake");
            assert_eq!(
                validate(&s0, &sp, &point, &claim_id, &fact, fact.anchor_block, &lottery, sw_policy()),
                Err(refusal),
                "the gate refuses on the same state for the same reason"
            );
            let fold = |at: &PalwBlockContextV2, extras: &PalwTransitionExtrasV1| {
                apply_palw_transition_v2_with_extras(&s0, &sp, at, &[], None, false, false, false, false, extras).unwrap()
            };
            let phase = |state: &PalwChainStateV2| state.claim(&claim_id).unwrap().phase.clone();
            let executor_reserved = s0.reserved_exposure(&key(1));
            let no_forfeit = |after: &PalwChainStateV2, label: &str| {
                assert!(after.panel(&claim_id).is_none(), "{label}: no panel was ever bound");
                for (k, bond) in s0.bonds_iter() {
                    let now = after.bond(k).unwrap();
                    assert_eq!((now.collateral, now.slashed), (bond.collateral, bond.slashed), "{label}: S0, no forfeit on {k:?}");
                }
                assert!(after.reserved_exposure(&key(1)) < executor_reserved, "{label}: the executor's reservation is released");
            };

            // Past the fence, on a block that may anchor a panel.
            let armed = PalwTransitionExtrasV1 { sw8_anchor_delay: Some(five().anchor_delay()), ..duty_extras() };
            let (before, _) = fold(&ctx(0x5F00_0003, slot - 1, slot - 1), &armed);
            assert_eq!(phase(&before), PalwClaimPhaseV2::Provisional, "a block below the slot is not the claim's anchor");
            let (heartbeat, _) = fold(&ctx(0x5F00_0004, slot + 1, slot + 1), &duty_extras());
            assert_eq!(phase(&heartbeat), PalwClaimPhaseV2::Provisional, "a block that may not anchor voids nothing");
            let (voided, delta) = fold(&point, &armed);
            assert_eq!(
                phase(&voided),
                PalwClaimPhaseV2::Voided { voided_daa: slot, reason: PalwVoidReasonV2::BindTimeout },
                "the anchor block voids the claim it did not bind, at its own DAA"
            );
            no_forfeit(&voided, "step 4c");
            voided.assert_deadline_consistency(&sp).expect("the deadline index is the claims' recomputed deadlines");
            let rebuilt = PalwStateCarriageV2::from_state(&voided).into_state(&sp, None).expect("the carriage rebuild accepts it");
            assert_eq!(rebuilt.state_root(), voided.state_root(), "a restart reproduces the void");
            assert_eq!(revert_delta_v2(&voided, &delta, &sp).unwrap().state_root(), s0.state_root(), "a reorg undoes it");

            // The fence-off twin: nothing voids the claim at its anchor; the window's sweep does.
            let (twin_at_anchor, _) = fold(&point, &duty_extras());
            assert_eq!(phase(&twin_at_anchor), PalwClaimPhaseV2::Provisional, "fence off: the claim waits");
            let deadline = claim.bind_base_daa() + sp.window_bind();
            let (swept, _) = fold(&ctx(0x5F00_0002, deadline + 1, deadline + 1), &duty_extras());
            assert!(
                matches!(phase(&swept), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }),
                "{:?}",
                phase(&swept)
            );
            no_forfeit(&swept, "the window's sweep");
        }

        /// `sw_state` with the executor ONE OF the eight genesis seats — testnet-12's shape, where
        /// every producer is a genesis card: bond 1 (key 7, operator 0x21) posts a genesis seat's
        /// collateral beside the seven others (bonds 2..=8). `saturate` as `sw_state`'s, bond 1
        /// included (the executor's own heavy claim fills its own headroom).
        fn genesis_executor_state(saturate: &[u64]) -> (PalwChainStateV2, Hash64) {
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let rows: Vec<(u64, u64, u64)> = (2..=8u64).map(|b| (b, 0x40 + b, GENESIS_SEAT)).collect();
            let mut objects = vec![adr0130_class(), heavy_class(), adr0130_bond(1, 7, 0x21, GENESIS_SEAT)];
            objects.extend(rows.iter().map(|(b, op, c)| adr0130_bond(*b, 30 + *b as u8, *op, *c)));
            let (mut state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
            let mut daa = 101;
            for b in saturate {
                let (pk, op) = if *b == 1 { (7, 0x21) } else { (30 + *b as u8, rows.iter().find(|row| row.0 == *b).unwrap().1) };
                state = fold_attempt(&state, &sp, daa, *b, pk, op, h64(3), h64(13), HEAVY_PWU).0;
                daa += 1;
            }
            fold_attempt(&state, &sp, daa, 1, 7, 0x21, h64(1), h64(11), 40)
        }

        /// **T94, the M4 review's finding 1: a genesis card's own claim keeps SW-10's one-seat
        /// tolerance.** On testnet-12 the executor is one of the eight genesis seats and may not
        /// sit, so the seven others are the whole population; with one of them saturated they are
        /// `6/7 = 857‰`, which alone would refuse. SW-10's executor term counts the executor's own
        /// capped weight on both sides (`palw_panel_stake_executor_bonds_judging_v1`), so the claim
        /// is measured over the eight:
        ///
        /// * one other seat saturated: `7/8 = 875‰`, the draw binds (and never seats the executor
        ///   or the saturated seat);
        /// * the executor saturated as well — the busiest producer, its own claims filling its own
        ///   headroom — still `7/8`: its load is unread, it cannot sit anyway;
        /// * two other seats saturated: `6/8`, `InsufficientEligibleStake { 6g, 8g }` — the ADR's
        ///   "refuses with two", reported with the term on both sides;
        /// * an executor outside the population (`sw_state`'s, below the panel floor) adds nothing,
        ///   and eight genesis seats with one saturated bind at `7/8` as the pure half pinned;
        /// * `stake: None` binds every one of them (no stake floor).
        #[test]
        fn t94_a_genesis_executor_keeps_the_one_saturated_seat_tolerance() {
            let g = 939_063u128;
            let stake = PalwPanelStakeDrawV1::V1;
            let weight = |list: &[(&PalwBondKeyV2, &PalwBondStateV2)]| palw_panel_stake_weight_v1(list, &stake);
            let executor_weight = |state: &PalwChainStateV2, claim: &Hash64| {
                weight(
                    &palw_panel_stake_executor_bonds_judging_v1(state, claim, &h64(1), 100, None, false, None, Some(sw_economy()), 5)
                        .unwrap(),
                )
            };

            let (one, c1) = genesis_executor_state(&[2]);
            let (eligible, base) = populations(&one, &c1);
            assert_eq!((weight(&eligible), weight(&base)), (6 * g, 7 * g), "the seven others, one saturated");
            assert!(
                matches!(palw_panel_stake_floor_v1(6 * g, 7 * g, 875), Err(PalwPanelV2Error::InsufficientEligibleStake { .. })),
                "the premise: the seven alone refuse at 857‰"
            );
            assert_eq!(executor_weight(&one, &c1), g, "the executor is a genesis seat");

            let (busy, cb) = genesis_executor_state(&[1, 2]);
            assert!(busy.reserved_exposure(&key(1)) > one.reserved_exposure(&key(1)), "the busy executor's own heavy claim loads it");
            assert_eq!(executor_weight(&busy, &cb), g, "the executor term reads no headroom");

            for i in 0..24u64 {
                for (label, state, claim) in [("one saturated", &one, &c1), ("and the executor busy", &busy, &cb)] {
                    let seats = draw(state, claim, anchor(i), sw_policy()).unwrap_or_else(|e| panic!("{label}, anchor {i}: {e:?}"));
                    assert_eq!(seats.len(), 5, "{label}: a full jury");
                    assert!(!seats.iter().any(|s| s.bond == key(1) || s.bond == key(2)), "{label}, anchor {i}: {seats:?}");
                }
            }

            let (two, c2) = genesis_executor_state(&[2, 3]);
            for i in 0..8u64 {
                assert_eq!(
                    draw(&two, &c2, anchor(i), sw_policy()),
                    Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 6 * g, base: 8 * g }),
                    "two other seats saturated: 6/8 refuses"
                );
            }
            for (state, claim) in [(&one, &c1), (&busy, &cb), (&two, &c2)] {
                assert!(draw(state, claim, anchor(0), lottery_policy()).is_ok(), "stake None has no stake floor");
            }

            let (outside, co) = sw_state(&genesis_and_small(0), &[2]);
            assert_eq!(executor_weight(&outside, &co), 0, "an executor below the panel floor could not sit: no term");
            assert!(draw(&outside, &co, anchor(0), sw_policy()).is_ok(), "eight genesis seats, one saturated: 7/8 binds");
        }
    }

    // ---- lane F2 (post-launch, 2026-09-27): a draw refused for eligibility re-anchors -------------

    /// **Lane F2 (`Params::palw_floor_refusal_retry`) at the fold and at the draw.** The fold's step
    /// 2e′ asks [`PalwStakeDrawCensusV1`] which unbound claims the stake draw refuses for eligibility
    /// at EVERY seed on the anchor block's pre-object base, and step 4c re-anchors those instead of
    /// voiding them. The fixture is testnet-12's shape on the live chain between DAA ~624 and 749:
    /// the executor one of eight genesis seats, two other seats standing behind live locks of their
    /// whole collateral, so the draw refuses on SW-10's floor at `6g / 8g` (the executor's term on
    /// both sides) — every class could still seat a panel, so V03(1) never reached them.
    mod f2_floor_refusal_retry {
        use super::*;
        use crate::palw_panel_var_v1::PalwSlashableLockV1;
        use crate::palw_state_v2::{
            PalwBondStatusV2, PalwClaimPhaseV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwTransitionExtrasV1, PalwVoidReasonV2,
            apply_delta_v2, apply_palw_transition_v2_with_extras, palw_claim_awaits_ncp_retry_v1, palw_panel_valid_lock_v1,
            revert_delta_v2,
        };

        const G: u128 = 939_063;

        fn key(b: u64) -> PalwBondKeyV2 {
            PalwBondKeyV2(bond_outpoint(b))
        }

        /// The fixture's state params with the floor-refusal retry's mirror at `armed`.
        fn sp(armed: Option<u64>) -> PalwStateParamsV2 {
            state_params().with_fp_exposure_ceiling(500).unwrap().with_floor_refusal_retry_from_daa(armed)
        }

        /// One draw point at the anchor block `at` (below lane A the block's own DAA reaches every
        /// slot), with testnet-12's stake policy.
        fn draw_inputs(at: u64) -> PalwSw8DrawInputsV1 {
            PalwSw8DrawInputsV1 {
                seat_count: five().seat_count,
                points: vec![PalwSw8DrawPointV1 {
                    max_slot: at,
                    draw_daa: at,
                    policy: sw_policy(),
                    bond_maturity_window: None,
                    settled_anchor_depth: None,
                    capability_proof: false,
                    shard_licensing: false,
                }],
            }
        }

        /// A block past the lock ledger (2026-09-23 audit, objective offences) and the panel
        /// economy: seats go on duty, and a seat is drawn only where it can post the claim's lock.
        fn ledger_extras() -> PalwTransitionExtrasV1 {
            PalwTransitionExtrasV1 {
                panel_economy_active: true,
                audit_2026_09_23_active: true,
                objective_offence_daa: Some(0),
                ..Default::default()
            }
        }

        /// The anchor block at `at`: SW-8's step 4c armed (`sw8_anchor_delay`) and its draw points.
        fn anchor_extras(at: u64) -> PalwTransitionExtrasV1 {
            PalwTransitionExtrasV1 {
                sw8_anchor_delay: Some(five().anchor_delay()),
                sw8_draw: Some(draw_inputs(at)),
                ..ledger_extras()
            }
        }

        /// testnet-12's shape: the executor bond 1 (key 7, operator 0x21) a genesis seat beside the
        /// seven others (bonds 2..=8), its 40-pwu floor claim accepted at DAA 101 (slot 105, backstop
        /// 111) — and each bond of `locked` standing behind a live lock of its WHOLE collateral until
        /// DAA `until` (`is_live` below it), so it can post no Valid lock and the draw skips it.
        fn t12_shaped(locked: &[u64], until: u64) -> (PalwChainStateV2, Hash64) {
            let sp = sp(None);
            let mut objects = vec![adr0130_class(), adr0130_bond(1, 7, 0x21, GENESIS_SEAT)];
            objects.extend((2..=8u64).map(|b| adr0130_bond(b, 30 + b as u8, 0x40 + b, GENESIS_SEAT)));
            let (state, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &sp, &ctx(1, 100, 1), &objects, None).unwrap();
            let (state, claim_id) = fold_attempt(&state, &sp, 101, 1, 7, 0x21, h64(1), h64(11), 40);
            (with_locks(&state, locked, until), claim_id)
        }

        /// `state` with a live lock of the whole collateral on each bond of `locked` until `until`.
        fn with_locks(state: &PalwChainStateV2, locked: &[u64], until: u64) -> PalwChainStateV2 {
            let mut carriage = PalwStateCarriageV2::from_state(state);
            for b in locked {
                let claim = h64(0xF2_0000 + b);
                carriage.slashable_locks.insert(
                    (key(*b), claim),
                    PalwSlashableLockV1 {
                        claim,
                        amount: GENESIS_SEAT as u128,
                        expiry_daa: until,
                        settled_at_final: 0,
                        attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                        segments: 0,
                    },
                );
            }
            carriage.into_state(&sp(None), None).expect("consistent")
        }

        /// The draw the chain derives in the anchor block `at` on `state` (the processor's
        /// `palw_v2_derive_panel_binding_v1`): testnet-12's stake policy with the claim's Valid
        /// lock priced at the binding block.
        fn chain_draw(
            state: &PalwChainStateV2,
            claim_id: &Hash64,
            at: u64,
            seed: u64,
        ) -> Result<Vec<PalwPanelSeatV2>, PalwPanelV2Error> {
            let claim = state.claim(claim_id).unwrap();
            let lock = palw_panel_valid_lock_v1(state, &sp(None), &ledger_extras(), claim_id, claim, at, 5);
            assert!(lock.is_some_and(|lock| lock.required > 0), "the premise: the ledger prices a lock no locked seat can post");
            derive_panel_v2_with_policy(
                state,
                &five(),
                claim_id,
                BlockHash::from_u64_word(seed),
                100,
                None,
                false,
                PalwPanelDrawPolicyV1 { valid_lock: lock, ..sw_policy() },
            )
        }

        fn fold(
            parent: &PalwChainStateV2,
            p: &PalwStateParamsV2,
            at: u64,
            objects: &[PalwConsensusObjectV2],
            extras: &PalwTransitionExtrasV1,
        ) -> (PalwChainStateV2, PalwStateDeltaV2) {
            let (next, delta) = apply_palw_transition_v2_with_extras(
                parent,
                p,
                &ctx(0xF200 + at, at, at),
                objects,
                None,
                false,
                false,
                false,
                false,
                extras,
            )
            .unwrap();
            next.assert_deadline_consistency(p).expect("the deadline index is the claims' recomputed deadlines");
            (next, delta)
        }

        /// Restart and reorg reproduce `child`: the carriage round trip (decode, rebuild the deadline
        /// index, every load check) gives its root and deadlines, and its delta reverts to `parent`
        /// and re-applies to it.
        fn restart_and_reorg_reproduce(
            parent: &PalwChainStateV2,
            child: &PalwChainStateV2,
            delta: &PalwStateDeltaV2,
            p: &PalwStateParamsV2,
        ) {
            // Every claim's deadline, as the index holds it (the index itself is the state's own).
            let deadlines = |s: &PalwChainStateV2| s.claims_iter().map(|(id, _)| (*id, s.deadline_of(id))).collect::<Vec<_>>();
            let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(child)).unwrap();
            let reloaded =
                borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(p, Some(child.state_root())).expect("reloads");
            assert_eq!(reloaded.state_root(), child.state_root(), "a restart reloads the same root");
            assert_eq!(deadlines(&reloaded), deadlines(child), "…and re-derives the same deadlines");
            reloaded.assert_deadline_consistency(p).expect("the reloaded index is the claims' recomputed deadlines");
            let back = revert_delta_v2(child, delta, p).expect("reverts");
            assert_eq!(back.state_root(), parent.state_root(), "a reorg reverts the block to its parent");
            assert_eq!(deadlines(&back), deadlines(parent));
            let again = apply_delta_v2(parent, delta, p).expect("re-applies");
            assert_eq!(again.state_root(), child.state_root(), "…and re-applying it is the child");
            assert_eq!(deadlines(&again), deadlines(child));
        }

        /// **A floor refusal re-anchors, keeps everything the claim owed, and binds at a later slot
        /// once the room is back.** Two other seats locked until DAA 107: at the claim's anchor slot
        /// (105) the draw refuses `InsufficientEligibleStake { 6g, 8g }` at every seed, and past the
        /// fence the anchor block re-bases the claim on itself — `Provisional`, `rebound_daa` 105,
        /// reservation, escrow and the backstop `accepted + window_bind` (111) untouched, the retry
        /// recognised from rooted state, a restart and a reorg reproducing it. The locks lapse, and
        /// the anchor block of the next slot (109) draws a full jury that binds; the bound claim keeps
        /// its one redraw (`rebound_daa` back to `None`). Below the fence the same anchor block voids it
        /// `BindTimeout`, SW-8's rule, as the live chain did 172 times.
        #[test]
        fn f2_a_floor_refusal_re_anchors_and_binds_at_a_later_slot_once_room_frees() {
            let (s1, claim_id) = t12_shaped(&[2, 3], 107);
            let claim = s1.claim(&claim_id).unwrap().clone();
            let slot = claim.bind_base_daa() + five().anchor_delay();
            assert_eq!((claim.accepted_daa, slot, s1.deadline_of(&claim_id)), (101, 105, Some(111)), "the premise's clock");
            for seed in 0..16u64 {
                assert_eq!(
                    chain_draw(&s1, &claim_id, slot, 0xF2A0 + seed),
                    Err(PalwPanelV2Error::InsufficientEligibleStake { eligible: 6 * G, base: 8 * G }),
                    "seed {seed}: two locked seats refuse the draw on SW-10's floor, whatever the seed"
                );
            }
            // Below the fence: SW-8's void in the anchor block, as shipped.
            let (below, _) = fold(&s1, &sp(None), slot, &[], &anchor_extras(slot));
            assert_eq!(
                below.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { voided_daa: slot, reason: PalwVoidReasonV2::BindTimeout },
                "below the fence the anchor block voids the claim it could not bind"
            );
            // Past it: re-anchored.
            let p = sp(Some(0));
            let reserved = s1.reserved_exposure(&key(1));
            let (s2, d2) = fold(&s1, &p, slot, &[], &anchor_extras(slot));
            let retried = s2.claim(&claim_id).unwrap().clone();
            assert_eq!((retried.phase.clone(), retried.rebound_daa), (PalwClaimPhaseV2::Provisional, Some(slot)), "re-anchored");
            assert_eq!(retried.escrowed_reward, claim.escrowed_reward, "the escrow stays with the claim");
            assert_eq!(s2.reserved_exposure(&key(1)), reserved, "so does the executor's reservation");
            assert_eq!(s2.deadline_of(&claim_id), Some(111), "the backstop does not move with the retry");
            assert!(palw_claim_awaits_ncp_retry_v1(&s2, &p, &claim_id, &retried), "the retry is read off rooted state");
            assert!(s2.panel(&claim_id).is_none());
            restart_and_reorg_reproduce(&s1, &s2, &d2, &p);
            // The locks lapse at 107; the anchor block of the next slot (109) binds a full jury.
            let next = slot + five().anchor_delay();
            let seats = chain_draw(&s2, &claim_id, next, 0xF2B0).expect("the room is back: the draw seats a panel");
            assert_eq!(seats.len(), 5);
            let bind = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(0xF2B0), seats };
            let (s3, d3) = fold(&s2, &p, next, &[bind], &anchor_extras(next));
            let bound = s3.claim(&claim_id).unwrap();
            assert_eq!(bound.phase, PalwClaimPhaseV2::PanelBound { bound_daa: next }, "bound at the later slot");
            assert_eq!(bound.rebound_daa, None, "a floor retry is not the one redraw");
            assert_eq!(bound.escrowed_reward, claim.escrowed_reward, "the escrow rides to the bind, nothing burned");
            restart_and_reorg_reproduce(&s2, &s3, &d3, &p);
        }

        /// **Repeated refusals void only at the backstop, labelled as today.** With the locks standing
        /// past the whole bind window, the claim re-anchors at 105 (next slot 109 ≤ 111) and then, at
        /// 109, its next slot (113) would pass the backstop: that anchor block voids it `BindTimeout`,
        /// SW-8's label. With no anchor block after 105 the window's sweep voids it at 112, the
        /// backstop's own label. Neither charges anything (S0) and both release the reservation.
        #[test]
        fn f2_repeated_refusals_void_only_at_the_backstop_with_todays_label() {
            let (s1, claim_id) = t12_shaped(&[2, 3], 1_000);
            let p = sp(Some(0));
            let (s2, _) = fold(&s1, &p, 105, &[], &anchor_extras(105));
            assert_eq!(s2.claim(&claim_id).unwrap().rebound_daa, Some(105), "the premise: re-anchored once");
            let collateral = |s: &PalwChainStateV2| (1..=8u64).map(|b| s.bond(&key(b)).unwrap().collateral).collect::<Vec<_>>();
            let (s3, _) = fold(&s2, &p, 109, &[], &anchor_extras(109));
            assert_eq!(
                s3.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { voided_daa: 109, reason: PalwVoidReasonV2::BindTimeout },
                "the next slot would pass the backstop: this anchor block voids it"
            );
            assert_eq!(collateral(&s3), collateral(&s1), "S0: nothing charged");
            assert!(s3.reserved_exposure(&key(1)) < s2.reserved_exposure(&key(1)), "the reservation is released");
            let (swept, _) = fold(&s2, &p, 112, &[], &ledger_extras());
            assert_eq!(
                swept.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { voided_daa: 112, reason: PalwVoidReasonV2::BindTimeout },
                "with no anchor block the backstop's sweep voids it"
            );
            assert_eq!(collateral(&swept), collateral(&s1), "S0 at the backstop too");
        }

        /// **A claim that has held a panel is never retried.** Bound at 109 (the first test's path),
        /// its panel says nothing, and the receipt timeout redraws it once (`rebound_daa` 120, its
        /// first panel's record kept). At its new slot (124) two seats are locked again and the draw
        /// refuses on the floor — and the anchor block voids it: its second window was the redraw.
        #[test]
        fn f2_a_claim_that_held_a_panel_is_not_retried() {
            let (s1, claim_id) = t12_shaped(&[2, 3], 107);
            let p = sp(Some(0));
            let (s2, _) = fold(&s1, &p, 105, &[], &anchor_extras(105));
            let seats = chain_draw(&s2, &claim_id, 109, 0xF2C0).unwrap();
            let bind = PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(0xF2C0), seats };
            let (s3, _) = fold(&s2, &p, 109, &[bind], &anchor_extras(109));
            let receipt_deadline = s3.deadline_of(&claim_id).expect("a receipt deadline");
            let (s4, _) = fold(&s3, &p, receipt_deadline + 1, &[], &ledger_extras());
            let redrawn = s4.claim(&claim_id).unwrap().clone();
            assert_eq!(
                (redrawn.phase.clone(), redrawn.rebound_daa),
                (PalwClaimPhaseV2::Provisional, Some(receipt_deadline + 1)),
                "the premise: the one redraw"
            );
            assert!(s4.panel(&claim_id).is_some(), "the first panel's record stays");
            assert!(!palw_claim_awaits_ncp_retry_v1(&s4, &p, &claim_id, &redrawn), "a redrawn claim is no retry");
            let slot = redrawn.bind_base_daa() + five().anchor_delay();
            let locked = with_locks(&s4, &[2, 3], 10_000);
            assert!(
                matches!(chain_draw(&locked, &claim_id, slot, 0xF2C1), Err(PalwPanelV2Error::InsufficientEligibleStake { .. })),
                "the premise: the redraw's draw refuses on the floor"
            );
            let (s5, _) = fold(&locked, &p, slot, &[], &anchor_extras(slot));
            assert_eq!(
                s5.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { voided_daa: slot, reason: PalwVoidReasonV2::BindTimeout },
                "a claim that held a panel voids in its anchor block"
            );
        }

        /// **The verdict is the pre-object base's, never what the anchor block carries.** (a) Drawable
        /// before the block's objects — no lock — and made thin by them (three seats retire in the
        /// anchor block): voided `BindTimeout`, so no object the anchor producer carries turns a draw
        /// it dislikes into a retry. (b) Thin before them, and made drawable by them (eight fresh
        /// genesis-sized seats register in the anchor block): still re-anchored, so no object turns a
        /// retry into a void. (c) Without its draw points (`sw8_draw: None`) the armed fold judges
        /// nothing and voids, as below the fence.
        #[test]
        fn f2_the_verdict_reads_the_pre_object_base_not_what_the_anchor_block_carries() {
            let p = sp(Some(0));
            let retire = |b: u64| PalwConsensusObjectV2::BondRetireRequested { bond: key(b), signature: vec![1] };
            let (open, claim_id) = t12_shaped(&[], 0);
            assert!(chain_draw(&open, &claim_id, 105, 0xF2D0).is_ok(), "the premise (a): drawable before the objects");
            let (a, _) = fold(&open, &p, 105, &[retire(2), retire(3), retire(4)], &anchor_extras(105));
            assert!(
                [2u64, 3, 4].iter().all(|b| matches!(a.bond(&key(*b)).unwrap().status, PalwBondStatusV2::Retiring { .. })),
                "the premise (a): the anchor block's own objects retire three seats"
            );
            assert_eq!(
                a.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { voided_daa: 105, reason: PalwVoidReasonV2::BindTimeout },
                "(a) drawable at the draw: the block's own retirements buy no retry"
            );
            let (thin, claim_id) = t12_shaped(&[2, 3], 1_000);
            // Eight genesis-sized seats: `(6 + 8) / (8 + 8)` = 875‰ binds (fewer would not).
            let fresh: Vec<PalwConsensusObjectV2> =
                (20..28u64).map(|b| adr0130_bond(b, 30 + b as u8, 0x40 + b, GENESIS_SEAT)).collect();
            let (b, _) = fold(&thin, &p, 105, &fresh, &anchor_extras(105));
            assert!((20..28u64).all(|n| b.bond(&key(n)).is_some()), "the premise (b): eight seats register in the block");
            assert!(chain_draw(&b, &claim_id, 105, 0xF2D1).is_ok(), "the premise (b): drawable after the objects");
            assert_eq!(
                (b.claim(&claim_id).unwrap().phase.clone(), b.claim(&claim_id).unwrap().rebound_daa),
                (PalwClaimPhaseV2::Provisional, Some(105)),
                "(b) thin at the draw: re-anchored, though the block's own registrations made it drawable after"
            );
            let (c, _) = fold(&thin, &p, 105, &[], &PalwTransitionExtrasV1 { sw8_draw: None, ..anchor_extras(105) });
            assert_eq!(
                c.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { voided_daa: 105, reason: PalwVoidReasonV2::BindTimeout },
                "(c) no draw points: nothing judged, SW-8's void"
            );
        }

        /// **Below the fence the fold is byte for byte the shipped one.** At the anchor block the
        /// armed-later params (the mirror at 200), the dormant params and the dormant params without
        /// draw points fold one state and one delta — a refused draw voided `BindTimeout` in all three.
        #[test]
        fn f2_below_the_fence_the_fold_is_the_shipped_one() {
            let (s1, claim_id) = t12_shaped(&[2, 3], 1_000);
            let later = fold(&s1, &sp(Some(200)), 105, &[], &anchor_extras(105));
            let dormant = fold(&s1, &sp(None), 105, &[], &anchor_extras(105));
            let shipped = fold(&s1, &sp(None), 105, &[], &PalwTransitionExtrasV1 { sw8_draw: None, ..anchor_extras(105) });
            for (label, (state, delta)) in [("armed later", &later), ("dormant", &dormant)] {
                assert_eq!(state.state_root(), shipped.0.state_root(), "{label}: the shipped root");
                assert_eq!(delta, &shipped.1, "{label}: the shipped delta");
            }
            assert_eq!(
                shipped.0.claim(&claim_id).unwrap().phase,
                PalwClaimPhaseV2::Voided { voided_daa: 105, reason: PalwVoidReasonV2::BindTimeout }
            );
        }

        /// **The census answers the draw's own question at every seed** — over T88's corpus under
        /// the stake draw (the lottery's policies with `stake: Some`, its posted-collateral locks and
        /// ADR-0147's outsider seats included) and under S-3's one-ledger filter on the locked and
        /// saturated genesis states: where the census says "refuses at every seed", 24 draws refuse
        /// with an eligibility error (`InsufficientEligibleStake`, `InsufficientEligibleBonds`,
        /// `NoOutsider`); where it says otherwise, a claim with no outsider binds at all 24, and an
        /// outsider-judged one binds at one seed at least. Both answers occur, and so do all three
        /// refusal kinds, so the comparison is not vacuous.
        #[test]
        fn f2_the_census_answers_the_draws_question_at_every_seed() {
            let eligibility = |e: &PalwPanelV2Error| {
                matches!(
                    e,
                    PalwPanelV2Error::InsufficientEligibleStake { .. }
                        | PalwPanelV2Error::InsufficientEligibleBonds { .. }
                        | PalwPanelV2Error::NoOutsider(_)
                )
            };
            let (mut refused, mut drawable, mut outsider_drawable) = (0usize, 0usize, 0usize);
            let mut kinds = std::collections::HashSet::new();
            let mut check = |label: &str,
                             state: &PalwChainStateV2,
                             params: &PalwPanelParamsV2,
                             claim_id: &Hash64,
                             policy: PalwPanelDrawPolicyV1| {
                let point = PalwSw8DrawPointV1 {
                    max_slot: u64::MAX,
                    draw_daa: 0,
                    policy,
                    bond_maturity_window: None,
                    settled_anchor_depth: None,
                    capability_proof: false,
                    shard_licensing: false,
                };
                let lock = policy.valid_lock;
                let mut census =
                    PalwStakeDrawCensusV1::new(state, params.seat_count, 100, &point, None, lock.map(|lock| lock.frame_v1()));
                let verdict = census.refuses_at_every_seed(claim_id, lock.as_ref());
                assert_eq!(census.refuses_at_every_seed(claim_id, lock.as_ref()), verdict, "{label}: a shared verdict is the verdict");
                let draws: Vec<_> = (0..24u64)
                    .map(|i| {
                        derive_panel_v2_with_policy(
                            state,
                            params,
                            claim_id,
                            BlockHash::from_u64_word(0xF2E0 + i),
                            100,
                            None,
                            false,
                            policy,
                        )
                    })
                    .collect();
                let outsider = policy.independence.is_some_and(|independence| {
                    independence.governs(state.claim(claim_id).unwrap())
                        && crate::palw_state_v2::palw_claim_is_outsider_judged_v1(
                            state,
                            state.claim(claim_id).unwrap(),
                            Some(independence.from_daa),
                        )
                });
                if verdict {
                    refused += 1;
                    for drawn in &draws {
                        match drawn {
                            Err(e) if eligibility(e) => {
                                kinds.insert(std::mem::discriminant(e));
                            }
                            other => panic!("{label}: the census says every seed refuses, a draw gave {other:?}"),
                        }
                    }
                } else if outsider {
                    outsider_drawable += 1;
                    assert!(draws.iter().any(|d| d.is_ok()), "{label}: the census says a seed seats it, no draw did: {:?}", draws[0]);
                } else {
                    drawable += 1;
                    for drawn in &draws {
                        assert!(drawn.is_ok(), "{label}: no outsider, drawable at one seed is drawable at all: {drawn:?}");
                    }
                }
            };
            let stake = |policy: PalwPanelDrawPolicyV1| PalwPanelDrawPolicyV1 { stake: Some(PalwPanelStakeDrawV1::V1), ..policy };
            for fixture in t88_corpus() {
                for (i, policy) in fixture.policies.iter().enumerate() {
                    check(&format!("{} #{i}", fixture.name), &fixture.state, &fixture.params, &fixture.claim_id, stake(*policy));
                }
            }
            // S-3's one-ledger filter, on testnet-12's shape: seats loaded by live locks or by their
            // own claims' reservations, from none to four, under a spread of eligibility prices.
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let filter = |eligibility: u128| PalwPanelValidLockV1 {
                required: u128::MAX,
                now_daa: 103,
                settled_anchor_depth: None,
                window_court: sp.window_court(),
                rcore: Some(PalwRcoreSeatFilterV1 {
                    gain_scale: Default::default(),
                    eligibility,
                    ceiling_permille: 500,
                    resolved_locks_off_ceiling: false,
                    accuser_reserve: 0,
                    held_charge_floor: None,
                }),
            };
            for loaded in [vec![], vec![2u64], vec![2, 3], vec![2, 3, 4], vec![2, 3, 4, 5]] {
                let (state, claim_id) = t12_shaped(&loaded, 1_000);
                for eligibility in [1u128, 50_000 * MSK as u128, 469_000 * MSK as u128, 470_000 * MSK as u128] {
                    let policy = PalwPanelDrawPolicyV1 { valid_lock: Some(filter(eligibility)), ..sw_policy() };
                    check(&format!("locked {loaded:?}, eligibility {eligibility}"), &state, &five(), &claim_id, policy);
                }
                if loaded.len() <= 2 {
                    let (state, claim_id) = sw_state(&genesis_and_small(3), &loaded);
                    let policy = PalwPanelDrawPolicyV1 { valid_lock: Some(filter(50_000 * MSK as u128)), ..sw_policy() };
                    check(&format!("saturated {loaded:?}"), &state, &five(), &claim_id, policy);
                }
            }
            println!(
                "census vs draw: {refused} refused at every seed, {drawable} drawable, {outsider_drawable} outsider-judged drawable"
            );
            assert!(refused > 0 && drawable > 0 && outsider_drawable > 0, "both answers occur");
            assert_eq!(
                kinds.len(),
                3,
                "every eligibility refusal occurs: InsufficientEligibleStake, InsufficientEligibleBonds, NoOutsider"
            );
        }

        /// **The census walks the bonds once per anchor block, not once per claim** — measured, not
        /// asserted by construction: over a registry of the eight genesis seats and 120 floor-sized
        /// operators, one census answering 64 claims is compared with the draw the chain derives for
        /// those claims (which walks every bond's lock ledger per list per claim). Printed for the
        /// report; the assertion is only that the census is the cheaper of the two.
        #[test]
        fn f2_the_census_costs_one_walk_per_anchor_block() {
            let (state, claim_id) = sw_state(&genesis_and_small(120), &[2, 3]);
            let sp = state_params().with_fp_exposure_ceiling(500).unwrap();
            let lock = PalwPanelValidLockV1 {
                required: u128::MAX,
                now_daa: 103,
                settled_anchor_depth: None,
                window_court: sp.window_court(),
                rcore: Some(PalwRcoreSeatFilterV1 {
                    gain_scale: Default::default(),
                    eligibility: 50_000 * MSK as u128,
                    ceiling_permille: 500,
                    resolved_locks_off_ceiling: false,
                    accuser_reserve: 0,
                    held_charge_floor: None,
                }),
            };
            let policy = PalwPanelDrawPolicyV1 { valid_lock: Some(lock), ..sw_policy() };
            let point = PalwSw8DrawPointV1 {
                max_slot: u64::MAX,
                draw_daa: 0,
                policy,
                bond_maturity_window: None,
                settled_anchor_depth: None,
                capability_proof: false,
                shard_licensing: false,
            };
            const CLAIMS: usize = 64;
            let started = std::time::Instant::now();
            let mut census = PalwStakeDrawCensusV1::new(&state, 5, 100, &point, None, Some(lock.frame_v1()));
            let built = started.elapsed();
            // Distinct prices, so no claim shares another's verdict: the per-claim cost, measured.
            for i in 0..CLAIMS {
                let priced = PalwPanelValidLockV1 {
                    rcore: lock.rcore.map(|f| PalwRcoreSeatFilterV1 { eligibility: f.eligibility + i as u128, ..f }),
                    ..lock
                };
                census.refuses_at_every_seed(&claim_id, Some(&priced));
            }
            let census_total = started.elapsed();
            let started = std::time::Instant::now();
            for i in 0..CLAIMS {
                let _ = derive_panel_v2_with_policy(&state, &five(), &claim_id, anchor(i as u64), 100, None, false, policy);
            }
            let draws = started.elapsed();
            println!(
                "[f2 cost] {} bonds: census build {:?}, census + {CLAIMS} verdicts {:?}; {CLAIMS} chain draws {:?}",
                state.bonds_iter().count(),
                built,
                census_total,
                draws
            );
            assert!(census_total < draws, "one census and {CLAIMS} verdicts cost less than {CLAIMS} draws");
        }
    }
}

/// **ADR-0152 Q-1: `Sampled` is appended and pinned — Borsh index 3, message tag 4.** The index
/// follows `Valid` (0), `Unavailable` (1) and `Incapable` (2), so no existing encoding moves; the
/// tag follows 1/2/3, so no signature over another verdict reads as a `Sampled`. The V3 message
/// wraps the V2 one and so carries the tag with the mask.
#[test]
fn rcore_m4_sampled_is_borsh_index_3_and_message_tag_4() {
    assert_eq!(borsh::to_vec(&PalwReceiptVerdictV2::Valid).unwrap(), vec![0]);
    assert_eq!(borsh::to_vec(&PalwReceiptVerdictV2::Unavailable { chunk_index: 7, requested_daa: 9 }).unwrap()[0], 1);
    assert_eq!(borsh::to_vec(&PalwReceiptVerdictV2::Incapable).unwrap(), vec![2]);
    assert_eq!(borsh::to_vec(&PalwReceiptVerdictV2::Sampled).unwrap(), vec![3]);
    assert_eq!(borsh::from_slice::<PalwReceiptVerdictV2>(&[3]).unwrap(), PalwReceiptVerdictV2::Sampled);

    let (net, claim, signed_daa) = (h64(0x4E), h64(0xC1), 1_234u64);
    let mut by_hand = keyed(PALW_RECEIPT_V2_DOMAIN_MESSAGE);
    by_hand.update(net.as_byte_slice());
    by_hand.update(claim.as_byte_slice());
    by_hand.update(&[4u8]);
    by_hand.update(&signed_daa.to_le_bytes());
    let v2 = palw_receipt_message_v2(net, claim, PalwReceiptVerdictV2::Sampled, signed_daa);
    assert_eq!(v2, finish(by_hand), "H(domain ‖ network ‖ claim ‖ 4 ‖ signed_daa)");
    for other in [
        PalwReceiptVerdictV2::Valid,
        PalwReceiptVerdictV2::Incapable,
        PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 0 },
    ] {
        assert_ne!(palw_receipt_message_v2(net, claim, other, signed_daa), v2, "{other:?}");
    }
    let mask = crate::palw_verification_v2::PalwSegmentMaskV2::single(2);
    let mut v3_by_hand = keyed(PALW_RECEIPT_V3_DOMAIN_MESSAGE);
    v3_by_hand.update(v2.as_byte_slice());
    v3_by_hand.update(&mask.0.to_le_bytes());
    assert_eq!(palw_receipt_message_v3(net, claim, PalwReceiptVerdictV2::Sampled, signed_daa, mask), finish(v3_by_hand));
}
