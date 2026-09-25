//! **Lane maturity-ext (post-launch, 2026-09-26): ADR-0065 D1 on ADR-0147's admission jury and the
//! registry's ready count, through the fold** (`PalwModelRegistryFoldV1::bond_maturity`, carried
//! where `Params::palw_bond_maturity_early` is active; user decision: "under the same DAA-500 fence").
//!
//! The fixture is ADR-0147's contested network (`to_first_audit`'s): every bond of it registers at DAA
//! 100, the jury's randomness is a floor attempt at DAA 995 (span 99), and the audit block is DAA
//! 1,000 (span 100), where the jury's own cutoff is "registered before span 99 began" (DAA 990). A
//! NEWCOMER serving the floor and holding Kimi registers at DAA 985 — inside that cutoff, so the
//! released jury draws it — and proves possession in the same block.
//!
//! * [`the_window_bars_the_jury_and_the_ready_count_at_the_audit_blocks_daa`]: with every bond ready,
//!   no window is the release (Kimi leaves `Candidate`); a window of 900 puts the network exactly at
//!   its floor at DAA 1,000 (`100 <= 1,000 - 900`): the same verdict, and every ready seat but the
//!   newcomer's; 901 leaves nobody mature — no population, no jury, no ready seat, `Candidate`.
//! * [`a_newcomer_that_decides_the_released_jury_is_not_on_the_matured_one`]: only the registrant's
//!   seven sybils and the newcomer hold Kimi. On a seed where the released jury seats the newcomer
//!   beside exactly two sybils — the newcomer's vote is the third, and Kimi leaves `Candidate` — the
//!   matured jury is drawn without it, holds fewer than three, and Kimi stays; recomputed from outside
//!   the fold both ways.

use super::*;
use crate::palw_model_registry_v1::{PalwBondMaturityFoldV1, palw_admission_jury_seed_v1, palw_model_registry_ready_seats_v1};

/// The newcomer's fixture index (operator `op_id(60)`, no other bond's).
const NEWCOMER: u64 = 40;
/// The DAA the newcomer registers and proves at: inside the jury's cutoff (990), past the network's 100.
const NEWCOMER_DAA: u64 = 985;
/// The audit block's DAA, and the window that puts the network (registered at 100) exactly at its floor.
const AUDIT_DAA: u64 = 1_000;
const WINDOW: u64 = AUDIT_DAA - 100;

fn matured(f: &PalwModelRegistryFoldV1, window: u64) -> PalwModelRegistryFoldV1 {
    PalwModelRegistryFoldV1 {
        bond_maturity: Some(PalwBondMaturityFoldV1 { window_daa: window, settled_anchor_depth: None }),
        ..f.clone()
    }
}

/// `to_first_audit` with the newcomer registered (and proving span 98) at DAA 985. Returns the state
/// before the audit block and the state after it.
fn to_audit_with_newcomer(
    root: Hash64,
    operands: &[crate::palw_artifact::PalwArtifactOperandV1],
    f: &PalwModelRegistryFoldV1,
    holders: &[u64],
    seed_block: u64,
) -> (PalwChainStateV2, PalwChainStateV2) {
    let p = params();
    let (s1, _) = fold_step(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &contested_network(root, false), None, &armed(None))
        .unwrap();
    let mut objects = vec![serving(NEWCOMER, h64(0x9A00 + NEWCOMER), true)];
    objects.extend(holders.iter().chain([NEWCOMER].iter()).map(|n| proof(operands, bond_key(*n), 98)));
    let (s2, _) = fold_step(&s1, &p, &ctx(2, NEWCOMER_DAA, 2), &objects, None, &armed(Some(f.clone()))).unwrap();
    assert_eq!(s2.bond(&bond_key(NEWCOMER)).expect("registered").registered_daa, NEWCOMER_DAA);
    let s3 = seeding_attempt(&s2, &ctx(seed_block, 995, 3), &armed(Some(f.clone())));
    let (s4, _) = fold_step(&s3, &p, &ctx(4, AUDIT_DAA, 4), &[], None, &armed(Some(f.clone()))).unwrap();
    (s3, s4)
}

/// The jury the audit at span 100 draws, recomputed from outside the fold: the rule's population
/// (active, serving the floor, registered before span 99, not the registrant) and — with `window` —
/// ADR-0065 D1's floor at the audit block's DAA.
fn jury_from_outside(before_audit: &PalwChainStateV2, window: Option<u64>) -> Vec<Hash64> {
    let anchor = before_audit.round_seed_anchor().expect("span 99 recorded a seed anchor");
    assert_eq!(anchor.span, 99);
    let seed = palw_admission_jury_seed_v1(&kimi_id(), 100, &anchor.block, &anchor.execution_key);
    let population: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = before_audit
        .bonds_iter()
        .filter(|(key, bond)| {
            matches!(bond.status, PalwBondStatusV2::Active)
                && bond.capable_classes.contains(&h64(1))
                && bond.registered_daa < 990
                && window.is_none_or(|w| bond.registered_daa <= AUDIT_DAA - w)
                && **key != bond_key(9)
        })
        .collect();
    crate::palw_panel_v2::palw_admission_jury_v1(&seed, &population, 5)
}

fn operator_of(state: &PalwChainStateV2, n: u64) -> Hash64 {
    state.bond(&bond_key(n)).expect("a bond").operator_id
}

#[test]
fn the_window_bars_the_jury_and_the_ready_count_at_the_audit_blocks_daa() {
    let (operands, root) = inventory();
    let f = fold(kimi_work());
    let everyone: Vec<u64> = SYBILS.chain(HONEST).collect();
    let p = params();
    let run = |fold: &PalwModelRegistryFoldV1| to_audit_with_newcomer(root, &operands, fold, &everyone, 3);

    // The release: no window. Every holder and the newcomer are ready; the network's jury holds Kimi.
    let (before, released) = run(&f);
    let row = released.model_lifecycle(&kimi_id()).expect("a row");
    assert_eq!(row.state, PalwModelLifecycleV1::Prefetching, "the released jury admits Kimi");
    assert_eq!(row.ready_seats, everyone.len() as u32 + 1, "27 holders and the newcomer are ready");
    assert_eq!(palw_model_registry_ready_seats_v1(&before, &p, &kimi_id(), AUDIT_DAA, &f), everyone.len() as u32 + 1);

    // Window 900: the network is mature at DAA 1,000 exactly, the newcomer is not.
    let fm = matured(&f, WINDOW);
    let (before_m, at_floor) = run(&fm);
    let row = at_floor.model_lifecycle(&kimi_id()).expect("a row");
    assert_eq!(row.state, PalwModelLifecycleV1::Prefetching, "a jury of mature bonds admits Kimi");
    assert_eq!(row.ready_seats, everyone.len() as u32, "every ready seat but the newcomer's");
    assert_eq!(
        palw_model_registry_ready_seats_v1(&before_m, &p, &kimi_id(), AUDIT_DAA, &fm),
        row.ready_seats,
        "the RPC's count is the chain's"
    );
    let newcomer_row = before_m.seat_readiness(&bond_key(NEWCOMER), &kimi_id()).expect("the newcomer proved");
    assert_eq!(
        crate::palw_model_registry_v1::palw_seat_not_ready_reason_v1(
            &before_m,
            &p,
            &bond_key(NEWCOMER),
            newcomer_row,
            AUDIT_DAA,
            &fm
        ),
        Some(crate::palw_model_registry_v1::PALW_SEAT_NOT_READY_IMMATURE_V1),
        "the newcomer is named immature"
    );
    for n in everyone.iter().copied() {
        let row = before_m.seat_readiness(&bond_key(n), &kimi_id()).expect("proved");
        assert_eq!(
            crate::palw_model_registry_v1::palw_seat_not_ready_reason_v1(&before_m, &p, &bond_key(n), row, AUDIT_DAA, &fm),
            None,
            "bond {n}, registered at 100: mature at {AUDIT_DAA} with window {WINDOW}"
        );
    }
    let jury_m = jury_from_outside(&before_m, Some(WINDOW));
    assert!(!jury_m.contains(&operator_of(&before_m, NEWCOMER)), "the newcomer is not in the matured population");

    // Window 901: nobody is mature at DAA 1,000 — no population, no jury, no ready seat.
    let (before_n, none_mature) = run(&matured(&f, WINDOW + 1));
    let row = none_mature.model_lifecycle(&kimi_id()).expect("a row");
    assert_eq!((row.state, row.ready_seats), (PalwModelLifecycleV1::Candidate, 0), "no mature bond: no jury, no ready seat");
    assert!(jury_from_outside(&before_n, Some(WINDOW + 1)).is_empty(), "an empty population draws nobody");
}

#[test]
fn a_newcomer_that_decides_the_released_jury_is_not_on_the_matured_one() {
    let (operands, root) = inventory();
    let f = fold(kimi_work());
    let fm = matured(&f, WINDOW);
    let sybils: Vec<u64> = SYBILS.collect();
    let sybil_ops = |state: &PalwChainStateV2| -> Vec<Hash64> { sybils.iter().map(|n| operator_of(state, *n)).collect() };
    let mut found = None;
    for seed_block in 40..1_200 {
        let (before, released) = to_audit_with_newcomer(root, &operands, &f, &sybils, seed_block);
        let newcomer_op = operator_of(&before, NEWCOMER);
        let ops = sybil_ops(&before);
        let jury = jury_from_outside(&before, None);
        let jury_m = jury_from_outside(&before, Some(WINDOW));
        let ready_released = jury.iter().filter(|o| ops.contains(o) || **o == newcomer_op).count();
        let ready_matured = jury_m.iter().filter(|o| ops.contains(o)).count();
        assert!(!jury_m.contains(&newcomer_op), "seed {seed_block}: never on the matured jury");
        // The released fold agrees with its jury recomputed from outside, on every seed.
        let state = released.model_lifecycle(&kimi_id()).expect("a row").state;
        let expect = if ready_released >= 3 { PalwModelLifecycleV1::Prefetching } else { PalwModelLifecycleV1::Candidate };
        assert_eq!(state, expect, "seed {seed_block}: {ready_released} of five hold Kimi on the released jury");
        if jury.contains(&newcomer_op) && ready_released == 3 && ready_matured < 3 {
            found = Some((seed_block, ready_matured));
            break;
        }
    }
    let (seed_block, ready_matured) = found.expect("a seed where the newcomer's vote is the released jury's third");
    let (_, matured_state) = to_audit_with_newcomer(root, &operands, &fm, &sybils, seed_block);
    let row = matured_state.model_lifecycle(&kimi_id()).expect("a row");
    assert_eq!(
        row.state,
        PalwModelLifecycleV1::Candidate,
        "seed {seed_block}: drawn without the newcomer the jury holds {ready_matured} of five — Kimi stays a Candidate"
    );
    assert_eq!(row.ready_seats, sybils.len() as u32, "the seven sybils are ready; the newcomer is not counted");
}
