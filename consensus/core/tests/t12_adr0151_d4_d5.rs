//! **ADR-0151 D4 and D5, verified rather than asserted in prose.**
//!
//! Both were drafted as OPEN from a description of the defect. Checking the tree found them already
//! built — D4 by ADR-0144 §9's lock ledger, D5 by the payout carrying no duration term at all — so
//! these tests exist to PIN that, because a property nothing checks is a property that drifts back.
use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// **D4: admission capacity and slash liability are two ledgers, and testnet-12 arms the second.**
///
/// * `reserved_exposure` is the CAPACITY ledger: `reserve_for_claim` adds `claim.reserved` and
///   `release_for_claim` subtracts it **on `Final` and on `Voided` alike** — "the exposure and the
///   immature contribution both belong only to non-terminal claims". So processing capacity returns
///   the moment a claim resolves; a resolved claim does not block new work.
/// * `slashable_locks: BTreeMap<(bond, claim), PalwSlashableLockV1>` is the LIABILITY ledger:
///   `{ claim, amount, expiry_daa }` with `is_live(now_daa)`. Its own doc is D4's sentence — "Final
///   does not erase liability … withdraw is refused while any lock on the bond is live" — and the
///   record deliberately keeps no claim bytes, only who owes what until when.
///
/// The fence that writes the second is `palw_objective_offence` (ADR-0144 §9), which testnet-11
/// schedules at DAA 8,500 and testnet-12 arms from genesis. **That is what makes D4 true on this
/// network and merely available on the other.**
#[test]
fn d4_the_liability_ledger_is_armed_from_genesis() {
    let p = t12();
    assert_eq!(
        p.palw_objective_offence,
        Some(ForkActivation::always()),
        "the lock ledger — a Valid receipt locks max_fraud_gain/3+1, Final keeps the liability, \
         BondRetire while locked is refused — must be in force from DAA 0"
    );
    // The two ledgers are distinct state, not one field doing two jobs: a lock is keyed by
    // `(bond, claim)` and carries its own expiry, while the capacity ledger is keyed by bond alone
    // and carries a running sum. A type-level fact, so it cannot be argued with.
    let lock = kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1 {
        claim: kaspa_consensus_core::Hash64::from_u64_word(1),
        amount: 7,
        expiry_daa: 100,
        // 2026-09-23 audit: the second clock's start; `is_live` below is the DAA-only rule.
        settled_at_final: 0,
    };
    assert!(lock.is_live(99), "a lock is live until its own expiry, not until the claim resolves");
    assert!(!lock.is_live(100));
    assert!(!lock.is_live(101));
}

/// **D5: a longer lifecycle buys no economic weight.**
///
/// `lifecycle duration ≠ economic weight`. The claim's reward is `min(escrow, C_P × rate)` and the
/// panel's share is `clamp(α·C_V / (C_P + α·C_V), S_min, S_max)`; neither expression has a window,
/// a deadline or a DAA count in it. Checked against the SOURCE rather than by re-deriving the
/// formulas, because the property is "no such term exists" and only the text can say that.
#[test]
fn d5_the_payout_carries_no_duration_term() {
    let src = include_str!("../src/palw_economic_payout_v1.rs");
    // Strip nothing: if the word appears even in a comment in this module, someone was thinking
    // about pricing a duration and the test should be read again on purpose.
    for term in ["window", "_daa", "deadline", "expiry"] {
        assert!(
            !src.contains(term),
            "`{term}` appears in palw_economic_payout_v1: a claim's price must be a function of compute \
             and escrow alone. If a duration genuinely belongs in the price, ADR-0151 D5 has to be \
             revisited rather than this test relaxed."
        );
    }
}

/// **D5's other half: a class's deadline comes from its own derived profile.**
///
/// ADR-0133 §11.3 — the deadline a bound panel is judged against is
/// `max(window_receipt, verification_window_spans × span_daa)`: the network's floor for a class that
/// already fits, and the class's own window for one that does not. That is what lets the held 2M row
/// (1,399 spans × 10 DAA against a 600-DAA network window) leave `Probation` without weakening the
/// global deadline for anybody — and, with D5 above, without earning anything for the extra time.
#[test]
fn d5_the_class_receipt_window_is_derived_from_genesis() {
    let p = t12();
    assert_eq!(
        p.palw_class_receipt_window,
        Some(ForkActivation::always()),
        "a class's receipt deadline must be its own derived window from DAA 0, so the 2M row is \
         admissible without stretching the network's"
    );
    assert_eq!(p.palw_seat_gate_possession, Some(ForkActivation::always()), "and the seat gate asks possession, not capacity");
}
