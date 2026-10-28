//! **RFC-0008: budget allocation and replayed history are invariant under relabelling and order, and splits never gain**
//! (ADR-0176 Q/B/R/F windows, ADR-0177 α allocation). SOURCE ONLY — written, never compiled or executed in this lane.
//!
//! Properties (each a separate control, complementing `split_capital_never_raises_caps`,
//! `splitting_a_model_or_a_bond_never_raises_the_total`, `a_role_swap_swaps_the_allocation_and_order_is_irrelevant` and
//! `splitting_bonds_or_claims_never_raises_candidates` in the module's own tests):
//! * operator relabelling — renaming every bond key permutes the per-bond answers and changes nothing else;
//! * genesis-order and registration-order swaps — the same reservations/assignments applied in any order produce byte-identical
//!   state and identical roots (the engine is keyed, never sequenced);
//! * replayed history — a journal applied forward and reverted returns the exact original bytes, and replaying two commuting
//!   journals in either order produces the same state;
//! * claim split — splitting one claim's ask over several claims never lowers its B/R/F footprint and COSTS one `Q` per extra
//!   claim (the split is refused once `Q` binds): a split is never free.
use super::*;
use crate::tx::TransactionOutpoint;

const BILI: u64 = crate::constants::SOMPI_PER_KASPA;

fn bond(i: u32) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(Default::default(), i))
}

fn h(i: u64) -> Hash64 {
    Hash64::from_u64_word(i)
}

/// A TEST policy (not a proposal), outside the validated W range on purpose (as the module's own fixtures are).
fn policy() -> PalwBondBudgetPolicyV1 {
    PalwBondBudgetPolicyV1 {
        version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
        window_daa: 100,
        capital_unit_sompi: 1_000 * BILI,
        rho: 1,
        claims_per_unit: 10,
        block_units_per_unit: 4 * PALW_BUDGET_BLOCK_UNIT_V1,
        reward_per_unit_sompi: 50 * BILI,
        final_weight_per_unit: 400,
        max_open_claims_per_bond: 1_000_000,
        slice_rights_by_rho: false,
        round_rights: PalwRoundRightsPolicyV1::CountAgainstBlocks,
        liability_hold_daa: 30,
        export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
        weight_value: PalwWeightValuePolicyV1::Unknown,
        hold: None,
    }
}

fn allocation(halves: u8) -> PalwModelAllocationPolicyV1 {
    PalwModelAllocationPolicyV1 {
        version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V2,
        epoch_daa: 10,
        seasoning_epochs: 1,
        curve: PalwAllocationCurveV1 { alpha_halves: halves },
        max_models_per_bond: 4,
        average: None,
    }
}

fn ask(reward: u64) -> PalwBudgetAskV1 {
    PalwBudgetAskV1 { block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: reward, final_weight: 10, liability_sompi: 0 }
}

/// Reserve `(claim, bond, reward)` rows in the given order; the resulting state.
fn reserve_all(rows: &[(u64, u32, u64)]) -> PalwBondBudgetStateV1 {
    let p = policy();
    let mut s = PalwBondBudgetStateV1::new(0, &p);
    let mut j = Vec::new();
    for (claim, b, reward) in rows {
        s.reserve_claim_v1(&p, 10_000 * BILI, h(*claim), bond(*b), None, 5, ask(*reward), PalwBudgetOriginV1::Attempt, &mut j)
            .expect("a test reservation fits");
    }
    s
}

#[test]
fn registration_and_genesis_order_swaps_leave_state_and_roots_identical() {
    let rows = [(1, 1, 3 * BILI), (2, 2, 5 * BILI), (3, 1, 7 * BILI), (4, 3, BILI)];
    let forward = reserve_all(&rows);
    let mut reversed_rows = rows;
    reversed_rows.reverse();
    let reversed = reserve_all(&reversed_rows);
    assert_eq!(forward, reversed, "the engine is keyed: insertion order leaves no trace");
    assert_eq!(borsh::to_vec(&forward).unwrap(), borsh::to_vec(&reversed).unwrap());
    assert_eq!(forward.root_parts(), reversed.root_parts());
}

#[test]
fn operator_relabelling_permutes_the_answers_and_nothing_else() {
    // Rename bonds 1 <-> 2: the per-bond rows swap, the totals and every other bond's row are unchanged.
    let original = reserve_all(&[(1, 1, 3 * BILI), (2, 2, 5 * BILI), (3, 3, 7 * BILI)]);
    let relabelled = reserve_all(&[(1, 2, 3 * BILI), (2, 1, 5 * BILI), (3, 3, 7 * BILI)]);
    assert_eq!(original.bond_row(&bond(1)), relabelled.bond_row(&bond(2)));
    assert_eq!(original.bond_row(&bond(2)), relabelled.bond_row(&bond(1)));
    assert_eq!(original.bond_row(&bond(3)), relabelled.bond_row(&bond(3)));
    let total = |s: &PalwBondBudgetStateV1| {
        [1, 2, 3]
            .iter()
            .filter_map(|b| s.bond_row(&bond(*b)).map(|row| row.window))
            .fold(PalwBudgetVectorV1::ZERO, |a, w| a.checked_add(w).unwrap())
    };
    assert_eq!(total(&original), total(&relabelled));
}

#[test]
fn capital_assignment_order_and_operator_labels_do_not_move_model_availability() {
    for halves in [2u8, 3, 4, 8] {
        let a = allocation(halves);
        let run = |order: &[(u32, u64, u64)]| {
            let mut s = PalwBondBudgetStateV1::new(0, &policy());
            let mut j = Vec::new();
            for (b, m, amount) in order {
                s.assign(&a, bond(*b), &[(h(*m), *amount)], 1, *amount, 0, |_| true, &mut j).unwrap();
            }
            let caps: BTreeMap<PalwBondKeyV2, u64> = order.iter().map(|(b, _, x)| (bond(*b), *x)).collect();
            s.roll_epoch(&a, 2, 20, |b| (caps.get(b).copied().unwrap_or(0), true), &mut j);
            s.accrue(1 << 40, &mut j);
            (s.model_available(&h(1)), s.model_available(&h(2)))
        };
        let one = run(&[(1, 1, 400 * BILI), (2, 2, 900 * BILI), (3, 1, 100 * BILI)]);
        let reordered = run(&[(3, 1, 100 * BILI), (2, 2, 900 * BILI), (1, 1, 400 * BILI)]);
        let relabelled = run(&[(7, 1, 400 * BILI), (8, 2, 900 * BILI), (9, 1, 100 * BILI)]);
        assert_eq!(one, reordered, "alpha_halves {halves}: assignment order is irrelevant");
        assert_eq!(one, relabelled, "alpha_halves {halves}: operator/bond labels are irrelevant");
    }
}

#[test]
fn replayed_history_reverts_to_the_exact_bytes_and_commuting_journals_commute() {
    let p = policy();
    let base = reserve_all(&[(1, 1, 3 * BILI)]);
    let bytes = borsh::to_vec(&base).unwrap();
    let apply = |state: &mut PalwBondBudgetStateV1, journal: &[PalwBudgetWriteV1], reverse: bool| {
        let order: Vec<_> = if reverse { journal.iter().rev().collect() } else { journal.iter().collect() };
        for entry in order {
            match entry {
                PalwBudgetWriteV1::Header { old, new } => state.header = if reverse { old } else { new }.clone().unwrap(),
                PalwBudgetWriteV1::Row { table, key, old, new } => state.apply_row(*table, key, old, new, reverse).unwrap(),
            }
        }
    };
    // Two independent reservations (different bonds) journaled separately.
    let mut s = base.clone();
    let (mut ja, mut jb) = (Vec::new(), Vec::new());
    s.reserve_claim_v1(&p, 10_000 * BILI, h(2), bond(2), None, 6, ask(BILI), PalwBudgetOriginV1::Attempt, &mut ja).unwrap();
    s.reserve_claim_v1(&p, 10_000 * BILI, h(3), bond(3), None, 6, ask(2 * BILI), PalwBudgetOriginV1::Attempt, &mut jb).unwrap();
    // Forward replay in the original order reproduces the state.
    let mut ab = base.clone();
    apply(&mut ab, &ja, false);
    apply(&mut ab, &jb, false);
    assert_eq!(ab, s);
    // The journals touch disjoint bonds and claims: the other order reaches the same bytes.
    let mut ba = base.clone();
    apply(&mut ba, &jb, false);
    apply(&mut ba, &ja, false);
    assert_eq!(borsh::to_vec(&ba).unwrap(), borsh::to_vec(&s).unwrap());
    // Reverting both, newest first, restores the exact original bytes and roots.
    apply(&mut ab, &jb, true);
    apply(&mut ab, &ja, true);
    assert_eq!(borsh::to_vec(&ab).unwrap(), bytes);
    assert_eq!(ab.root_parts(), base.root_parts());
}

#[test]
fn a_claim_split_never_lowers_its_footprint_and_costs_one_q_per_extra_claim() {
    let whole = reserve_all(&[(1, 1, 6 * BILI)]);
    let split = reserve_all(&[(1, 1, 2 * BILI), (2, 1, 2 * BILI), (3, 1, 2 * BILI)]);
    let w = whole.bond_row(&bond(1)).unwrap().window;
    let s = split.bond_row(&bond(1)).unwrap().window;
    assert!(s.reward_sompi >= w.reward_sompi, "the split's R footprint is never smaller");
    assert!(s.block_units >= w.block_units && s.final_weight >= w.final_weight, "nor its B/F footprint");
    assert_eq!(s.claims, w.claims + 2, "every extra claim costs one Q: a split is never free");
    // Once Q binds, the split is refused outright while the whole claim still fits.
    let mut tight = policy();
    tight.claims_per_unit = 1; // 10 units of capital -> 10 claims at most
    let capital = 10 * 1_000 * BILI;
    let mut st = PalwBondBudgetStateV1::new(0, &tight);
    let mut j = Vec::new();
    for c in 0..10 {
        st.reserve_claim_v1(&tight, capital, h(100 + c), bond(1), None, 5, ask(BILI), PalwBudgetOriginV1::Attempt, &mut j).unwrap();
    }
    assert!(
        st.reserve_claim_v1(&tight, capital, h(200), bond(1), None, 5, ask(BILI), PalwBudgetOriginV1::Attempt, &mut j).is_err(),
        "the eleventh piece of a split is refused by Q"
    );
}
