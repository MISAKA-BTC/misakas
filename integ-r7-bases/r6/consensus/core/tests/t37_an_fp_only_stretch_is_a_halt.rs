//! **ADR-0152 v3.1 T37's F18 clause: an FP-only stretch counts as halted.** V-4(b)'s licence halt is
//! "no anchor has settled for `2 × window_court`", and past `palw_rcore_plus` an anchor settles only
//! at an ATTEMPT's replay-backed licence (`basis_k ≥ 2`; `license_claim` and both supplementary
//! doors: `attempt && basis_k ≥ 2`). A free-prompt claim licensed by the full quorum settles nothing,
//! so a stretch in which only free prompts license is a halt: the second clock does not move, rows
//! do not mature, and B-3 does not hold their payees for it — exactly as on a chain where nothing
//! licenses at all. (The rest of T37 — the halt itself, and C5's S2 licence that never ticks until it
//! upgrades — is `vesting_fold_v1`'s `t37_rows_never_mature_during_a_licence_halt` and
//! `rcore_s2_staged_reserve`'s `t37_t74_…`.)
//!
//! On testnet-12's own fold through `rcore_common`'s `Tape` (every block re-applied, reverted,
//! reloaded), with the floor's free-prompt lane made ready as M5's `fp_floor_ready` makes it.
//!
//! Run: cargo test -p kaspa-consensus-core --test t37_an_fp_only_stretch_is_a_halt

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_vesting_v1::palw_chain_vesting_halted_v1;

/// The free-prompt executor bond.
const FP_BOND: u64 = 51;
/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;

/// A V1 quorum licence: every floor seat's `Valid`, signed at the bind.
fn quorum(t: &Tape, claim: Hash64, bound: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ReceiptLicensed {
        claim,
        receipts: t.c.floor_seats().iter().map(|(k, _)| valid(claim, *k, bound)).collect(),
    }
}

/// **T37 / F18.** An attempt licensed by the full quorum settles an anchor (the ring gains its DAA)
/// and goes `Final` with its vesting row. Then only a free prompt licenses — by the same full quorum,
/// `basis_k` 3 — and goes `Final`: neither the licence nor the `Final` settles anything. At the
/// attempt's anchor + `2 · window_court` the chain is halted although a claim licensed inside the
/// window, the attempt's row (past its DAA clock) does not mature, and its payees are not held (B-3
/// reads the halt as it reads a licence-less chain). The control: an attempt licensed at the same
/// point instead of the free prompt settles an anchor and there is no halt.
#[test]
fn t37_f18_a_stretch_in_which_only_free_prompts_license_is_a_halt() {
    let mut c = Chain::new(t12());
    c.attribution = true;
    c.room = true;
    let collateral = at_least_the_floor(&c.p, 1_000_000 * MSK);
    let (mut t, leaves) = fp_floor_ready(c, FP_BOND, collateral);
    let raw = t.c.p.palw_settled_anchor_depth;
    assert!(raw.is_some(), "testnet-12 arms the second clock");
    let wc = t.c.sp.window_court();

    // The attempt: licensed by the full quorum (an anchor settles), then Final with its row.
    let a = t.attempt(None, 0xF18A);
    let bound = t.bind(a);
    let settled = t.c.s.settled_attempt_finals();
    t.step(vec![quorum(&t, a, bound)]);
    let anchor = t.c.daa;
    assert_eq!(t.c.s.settled_attempt_finals(), settled + 1, "an attempt's quorum licence settles an anchor");
    assert_eq!(t.c.s.recent_anchor_daas().last(), Some(&anchor), "and the ring gains its DAA");
    let fork_at = t.len();
    let deadline = t.c.s.deadline_of(&a).expect("the licensed attempt's Final deadline");
    t.at(deadline + 1, vec![]);
    assert!(matches!(t.c.s.claim(&a).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the attempt is Final");
    let row = t.c.s.vesting_row(&a).expect("the attempt's Final writes its row").clone();
    let producer = row.producer_bond;

    // The free prompt: committed, bound, licensed by the full quorum, Final — nothing settles.
    let (commit, f) = fp_commit_of(&t.c, FP_BOND, leaves, 0xF18F);
    let skips = t.block(t.c.daa + 1, vec![commit], None, T12_BLOCK_SUBSIDY_SOMPI).expect("the commitment's block folds");
    assert!(skips.is_empty() && t.c.s.claim(&f).is_some(), "the free prompt is accepted: {skips:?}");
    let fp_bound = t.bind(f);
    let (settled, ring) = (t.c.s.settled_attempt_finals(), t.c.s.recent_anchor_daas().to_vec());
    t.step(vec![quorum(&t, f, fp_bound)]);
    let fp_licensed = t.c.daa;
    let licensed = t.c.s.claim(&f).unwrap().clone();
    assert!(matches!(licensed.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the free prompt licenses");
    assert!(licensed.rcore.basis_k >= 2, "by a replay-backed quorum (basis_k {})", licensed.rcore.basis_k);
    assert_eq!(
        (t.c.s.settled_attempt_finals(), t.c.s.recent_anchor_daas().to_vec()),
        (settled, ring.clone()),
        "F18: a free prompt's licence settles no anchor"
    );
    let fp_deadline = t.c.s.deadline_of(&f).expect("the free prompt's Final deadline");
    t.at(fp_deadline + 1, vec![]);
    assert!(matches!(t.c.s.claim(&f).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the free prompt is Final");
    assert_eq!((t.c.s.settled_attempt_finals(), t.c.s.recent_anchor_daas().to_vec()), (settled, ring), "nor does its Final");

    // `2 · window_court` after the attempt's anchor: halted, although a free prompt licensed inside it.
    let halt = anchor + 2 * wc;
    assert!(fp_licensed < halt, "the free prompt licensed inside the window ({fp_licensed} < {halt})");
    assert!(!palw_chain_vesting_halted_v1(&t.c.s, raw, halt - 1, wc), "one DAA before, the anchor still counts");
    assert!(halt > row.expiry_daa, "the premise: the row's DAA clock is met before the halt");
    t.at(halt, vec![]);
    assert!(palw_chain_vesting_halted_v1(&t.c.s, raw, halt, wc), "F18: an FP-only stretch is a halt");
    t.at(halt + 1, vec![]);
    let held = t.c.s.vesting_row(&a).expect("V-4(b): no row matures during a halt");
    assert_eq!(held.matured_at, None, "not latched");
    assert!(
        !kaspa_consensus_core::palw_state_v2::palw_bond_is_payee_of_unmatured_row_v1(&t.c.s, &t.c.sp, &producer, halt + 1, raw),
        "the halt holds no payee's collateral (B-3 reads the halt, not the free prompt's licence)"
    );

    // The control: an ATTEMPT licensed where the free prompt did settles an anchor — no halt.
    let mut control = t.fork(fork_at);
    let deadline = control.c.s.deadline_of(&a).expect("the attempt's Final deadline");
    control.at(deadline + 1, vec![]);
    let b = control.attempt(None, 0xF18B);
    let b_bound = control.bind(b);
    control.step(vec![quorum(&control, b, b_bound)]);
    let b_anchor = control.c.daa;
    assert_eq!(control.c.s.recent_anchor_daas().last(), Some(&b_anchor), "an attempt's licence settles an anchor");
    let at = halt.max(control.c.daa + 1);
    control.at(at, vec![]);
    assert!(!palw_chain_vesting_halted_v1(&control.c.s, raw, at, wc), "the control: not halted");
    println!("T37/F18: anchor {anchor}; FP licensed {fp_licensed}; halted at {halt} (window_court {wc}); control anchor {b_anchor}");
    t.revert_to_base_and_reapply();
}
