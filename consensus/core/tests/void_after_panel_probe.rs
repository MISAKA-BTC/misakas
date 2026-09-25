//! **Can a producer, after it can see its panel, throw the claim away for free and draw again?**
//! (2026-09-25, the user's question before `anchor_delay = 4` or any licence speed-up.) The probe
//! `677ca68b5` found the one lever that was not the void: the anchor block's IDENTITY keyed the panel,
//! and one lottery win gave its producer as many valid identities as it cared to sign. **Lane F1's
//! post-launch fence (`Params::palw_panel_seed_execution`, [`t12_f1`]: testnet-12 as released with the
//! fence armed at [`F1_AT`]) keys the seed on the anchor attempt's execution commitment
//! (`palw_panel_draw_seed_v1`), and this file is its regression**: T2a, T3a and T3b are flipped to
//! assert the fenced behaviour (T2a also shows the released rule below the fence still draws on the
//! identity), the rest hold what the void always did. **What stays open is printed, not hidden**: a new
//! panel costs a new lottery win, and while P0-10 is open a win is ~279 junk BLAKE2b draws (T3a (b),
//! T3b's per-win capture rate) — docs/t12-panel-seed-2026-09-25.md.
//!
//! Everything here is testnet-12's own `Params` (with the fence armed), its bundle's `PalwStateParamsV2`, its genesis state
//! and the real fold (`apply_palw_transition_v7`, through `rcore_common`'s `Chain` extras, which are
//! the processor's `palw_transition_extras_for` minus the header-store fields). Two things the
//! processor does are emulated, and each is stated where it is used:
//!
//! * **step 4c's lane rule** — `sw8_anchor_delay = Some(anchor_delay)` on every ATTEMPT block and on
//!   no other block (`palw_sw8_anchor_delay_for`);
//! * **the derived binding** — `palw_v2_derived_panel_bindings_on_one_state_v1` draws
//!   `derive_panel_v2_with_policy` on the pre-object base (`palw_v2_pre_object_base_v1`) with the policy
//!   `palw_panel_draw_policy_at` resolves, from the SEED `PalwAnchorFactV2::panel_seed` names — past
//!   `palw_rcore_plus`, `H(anchor attempt's execution commitment ‖ claim)` ([`chain_seed`]; the
//!   commitment is the attempt's fold key, which is what `palw_panel_anchor_execution_v1` reads off the
//!   anchor header, T3a) — and [`derive_at`] builds the same call from `Params` (the Valid-lock filter
//!   left `None`: the genesis bonds and the coalition below are far above any lock, so it removes nobody).
//!
//! Tests (the user's T1–T8; T6–T8's code reading is in the report):
//! * `t1_…` — an unbound claim voids in its anchor block: every balance read back.
//! * `t2a_…` — FLIPPED: the panel is a function of the anchor ATTEMPT's execution commitment and the
//!   claim; 256 identities carrying one anchor attempt draw one panel.
//! * `t2b_…` — after the panel: the first `ReceiptTimeout` redraws at S0, the second forfeits (S0′).
//! * `t3a_…` — FLIPPED: one lottery win, 1,000 re-signatures and a nonce inside its bucket — 1,001 valid
//!   block identities, ONE execution commitment read off every header, ONE panel; a new panel costs a
//!   new lottery win (a new execution commitment).
//! * `t3b_…` — FLIPPED: the anchor block's producer grinding identities for its own claim or a victim's
//!   finds one panel per claim, and no coalition quorum it did not draw honestly.
//! * `t4_…` — 1,000 claim → anchor-void → claim cycles on one bond: nothing accumulates.
//! * `t5_…` — after the void the reservation (a 13,000 MSK bond's K = 2) and the 2M lane (cap 1) are
//!   free in the very next block.
//! * `t8_…` — the void replays on restart, reverts on reorg, and a reorg between two anchors moves the
//!   whole obligation with the branch.
//!
//! Run: CARGO_BUILD_JOBS=2 cargo test -p kaspa-consensus-core --test void_after_panel_probe -- --nocapture --test-threads=1
#![allow(clippy::too_many_arguments, dead_code)]

#[path = "rcore_common.rs"]
mod rc;
use rc::*;

use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptEnvelopeV2,
    PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
    execution_commitment_v3, palw_nonce_bucket_v1,
};
use kaspa_consensus_core::palw_model_registry_v1::PalwReadinessPolicyV1;
use kaspa_consensus_core::palw_panel_v2::{
    PalwAnchorFactV2, PalwPanelDrawPolicyV1, PalwPanelIndependenceV1, PalwPanelStakeDrawV1, derive_panel_v2_with_policy,
    palw_bond_maturity_window_v2, palw_panel_anchor_execution_v1, palw_panel_draw_seed_v1, palw_seat_maturity_floor_v1,
};
use kaspa_consensus_core::palw_reward_v2::{PalwRewardStatusV2, palw_reward_status_v2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwPanelSeatV2, PalwStateDeltaV2, PalwStateV2Error, PalwVoidReasonV2, palw_bond_free_slashable_v1,
    palw_v2_pre_object_base_v1,
};
use std::collections::BTreeSet;
use std::time::Instant;

type Obj = PalwConsensusObjectV2;
type Att = (PalwAttemptEnvelopeV2, Hash64, Hash64);

/// The attacker's producing bond.
const ATT: u64 = 0xA1;
/// Another producer (an honest miner, or the victim).
const MINER: u64 = 0xA2;
/// The attacker's coalition of seat Sybils (T3).
const SYBILS: [u64; 3] = [0x5B1, 0x5B2, 0x5B3];

fn anchor_delay(p: &Params) -> u64 {
    bundle(p).panel.anchor_delay()
}

/// **Lane F1's fence height in these runs.** Every chain below starts at DAA 1,001 and every header at
/// 1,000, so every anchor is past it; T2a draws below it too, where the released rule stands. An
/// operator's real height is post-launch and independent of testnet-12's scheduled 1,000; 900 is one.
const F1_AT: u64 = 900;

/// **testnet-12 as released (`0e8ec984e`), with lane F1's fence armed at [`F1_AT`]** — the ruleset a
/// node runs once the operator arms it. Validated, and the fence's predicate checked against
/// [`seed_rule_armed`], which the draws below read (a `Params` per derivation would dominate them).
fn t12_f1() -> Params {
    let mut p = t12();
    assert_eq!(p.palw_panel_seed_execution, None, "the release ships the fence dormant");
    p.palw_panel_seed_execution = Some(kaspa_consensus_core::config::params::ForkActivation::new(F1_AT));
    p.validate_palw_v2().expect("testnet-12 with lane F1 armed is a runnable ruleset");
    for daa in [0, F1_AT - 1, F1_AT, 1_000, 1_001, 1_000_000] {
        assert_eq!(p.palw_panel_seed_execution_active_at(daa), seed_rule_armed(daa), "DAA {daa}");
    }
    p
}

/// `Params::palw_panel_seed_execution_active_at` for [`t12_f1`], at a claim's ANCHOR DAA — the key the
/// processor's anchor walk resolves the seed rule at.
fn seed_rule_armed(anchor_daa: u64) -> bool {
    anchor_daa >= F1_AT
}

fn floor_id(p: &Params) -> Hash64 {
    genesis_classes(p)[0].0
}

// =================================================================================================
// The run: one block per call, through the real fold, with step 4c's lane rule
// =================================================================================================

#[derive(Clone)]
struct Blk {
    block: Hash64,
    daa: u64,
    objects: Vec<Obj>,
    attempt: Option<Att>,
    anchor: Option<u64>,
    subsidy: u64,
    delta: PalwStateDeltaV2,
    state: PalwChainStateV2,
    skips: Vec<String>,
}

struct Run {
    c: Chain,
    base: PalwChainStateV2,
    base_daa: u64,
    blocks: Vec<Blk>,
    /// Check every block as `rcore_common::Tape` does (delta re-applies and reverts, the carriage
    /// reloads under the committed root). Off only in the 1,000-cycle loop.
    check: bool,
    seed: u64,
}

fn fold_on(
    c: &Chain,
    parent: &PalwChainStateV2,
    block: Hash64,
    daa: u64,
    objects: &[Obj],
    attempt: Option<&Att>,
    subsidy: u64,
    anchor: Option<u64>,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error> {
    let point = PalwBlockContextV2 { block, daa_score: daa, blue_score: daa, subsidy };
    let (work, key) = match attempt {
        Some((env, key, _)) => (PalwBlockWorkV3::Attempt(env), *key),
        None => (PalwBlockWorkV3::None, Hash64::default()),
    };
    let mut e = c.extras_at(daa);
    e.own_job_anchor = attempt.map(|a| a.2).unwrap_or_default();
    // processor.rs:10617 — `Some(anchor_delay)` iff `palw_rcore_plus` and the block is an attempt block.
    e.sw8_anchor_delay = anchor;
    fold_with(&c.p, &c.sp, parent, &point, objects, work, key, &e)
}

impl Run {
    fn new(c: Chain) -> Self {
        let (base, base_daa) = (c.s.clone(), c.daa);
        Run { c, base, base_daa, blocks: Vec::new(), check: true, seed: 0x7000 }
    }

    fn s(&self) -> &PalwChainStateV2 {
        &self.c.s
    }

    fn daa(&self) -> u64 {
        self.c.daa
    }

    /// One block, committed. `anchor` is the lane rule's answer for it.
    fn push(
        &mut self,
        block: Hash64,
        daa: u64,
        objects: Vec<Obj>,
        attempt: Option<Att>,
        subsidy: u64,
        anchor: Option<u64>,
    ) -> Result<Vec<String>, PalwStateV2Error> {
        assert!(daa > self.c.daa || self.blocks.is_empty(), "DAA moves forward: {daa} after {}", self.c.daa);
        let parent = self.c.s.clone();
        let (child, delta, skips) = fold_on(&self.c, &parent, block, daa, &objects, attempt.as_ref(), subsidy, anchor)?;
        if self.check {
            assert_eq!(
                apply_delta_v2(&parent, &delta, &self.c.sp).expect("re-applies"),
                child,
                "DAA {daa}: the delta is the transition"
            );
            assert_eq!(revert_delta_v2(&child, &delta, &self.c.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
            let reloaded = PalwStateCarriageV2::from_state(&child)
                .into_state(&self.c.sp, Some(child.state_root()))
                .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads: {e}"));
            assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        }
        let skips: Vec<String> = skips.into_iter().map(|(_, why)| why).collect();
        self.c.s = child.clone();
        self.c.daa = daa;
        self.blocks.push(Blk { block, daa, objects, attempt, anchor, subsidy, delta, state: child, skips: skips.clone() });
        Ok(skips)
    }

    /// A claimless block (a heartbeat's stand-in): never an anchor.
    fn beat(&mut self, daa: u64, objects: Vec<Obj>) {
        let skips = self.push(h(0xBE_0000_0000 + daa), daa, objects, None, 0, None).unwrap_or_else(|e| panic!("beat at {daa}: {e}"));
        assert!(skips.is_empty(), "{skips:?}");
    }

    /// Heartbeats one DAA apart up to `daa` (exclusive of any attempt).
    fn beat_to(&mut self, daa: u64) {
        while self.c.daa + 1 < daa {
            let d = self.c.daa + 1;
            self.beat(d, vec![]);
        }
    }

    /// A floor attempt block by bond `n` at `daa` whose identity is `block`, carrying `objects` (the
    /// derived bindings lead a block's objects). Always an anchor-lane block. Returns the attempt's
    /// claim id and what was skipped.
    fn attempt_block(&mut self, n: u64, daa: u64, block: Hash64, objects: Vec<Obj>) -> (Hash64, Vec<String>) {
        let att = self.next_attempt(n);
        self.attempt_block_of(att, daa, block, objects)
    }

    /// The next floor attempt of bond `n` — `(envelope, execution key, job anchor)` — before any block
    /// carries it, so a binding can be drawn from its seed ([`chain_seed`]) and ride the same block.
    fn next_attempt(&mut self, n: u64) -> Att {
        self.seed += 1;
        let seed = self.seed;
        let (env, key, _) = floor_attempt_of(&self.c, n, seed);
        let job = floor_job_anchor(&self.c.p, bond_key(n), 0x10C0 + seed);
        (env, key, job)
    }

    /// [`Self::attempt_block`] for an attempt made by [`Self::next_attempt`].
    fn attempt_block_of(&mut self, att: Att, daa: u64, block: Hash64, objects: Vec<Obj>) -> (Hash64, Vec<String>) {
        let id = attempt_id_v2(&att.0.attempt);
        let ad = anchor_delay(&self.c.p);
        let skips = self
            .push(block, daa, objects, Some(att), T12_BLOCK_SUBSIDY_SOMPI, Some(ad))
            .unwrap_or_else(|e| panic!("attempt block at {daa}: {e}"));
        (id, skips)
    }
}

/// **The draw policy the processor resolves at `anchor_daa`** (`palw_panel_draw_policy_at`), from
/// `Params`. `valid_lock` is `None` (see the module doc).
fn policy_at(p: &Params, sp: &PalwStateParamsV2, anchor_daa: u64) -> PalwPanelDrawPolicyV1 {
    let readiness = registry_fold(p, anchor_daa).filter(|f| f.governs_at(anchor_daa)).map(|fold| {
        PalwReadinessPolicyV1::at(
            &fold,
            anchor_daa,
            sp.base_class_id(),
            p.palw_audit_2026_09_23_active_at(anchor_daa) && p.palw_readiness_v2_at(anchor_daa),
        )
    });
    PalwPanelDrawPolicyV1 {
        weighted: p.palw_audit_2026_09_11_deep_active_at(anchor_daa),
        economy: p.palw_seat_economy_at(anchor_daa),
        readiness,
        independence: p.palw_admission_independence_daa().map(|from_daa| PalwPanelIndependenceV1 {
            from_daa,
            base_class_id: sp.base_class_id(),
            anchor_daa,
        }),
        valid_lock: None,
        stake: p.palw_rcore_plus.is_some_and(|f| f.is_active(anchor_daa)).then_some(PalwPanelStakeDrawV1::V1),
    }
}

/// The pre-object base the processor draws on for an attempt block at `daa` (`palw_v2_pre_object_base_v1`).
fn pre_object_base(c: &Chain, parent: &PalwChainStateV2, block: Hash64, daa: u64) -> PalwChainStateV2 {
    let point = PalwBlockContextV2 { block, daa_score: daa, blue_score: daa, subsidy: T12_BLOCK_SUBSIDY_SOMPI };
    let f = flags(&c.p, daa);
    let mut e = c.extras_at(daa);
    e.sw8_anchor_delay = Some(anchor_delay(&c.p));
    palw_v2_pre_object_base_v1(
        parent,
        &c.sp,
        &point,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &e,
    )
    .expect("the pre-object base folds")
}

/// **The panel the chain would bind for `claim` in an attempt block at `daa` whose identity is
/// `anchor`**, on `base` (the block's pre-object base) — `derive_panel_v2_with_policy` exactly as
/// `palw_v2_derive_panel_binding_v1` calls it (processor.rs:12611).
fn derive_at(c: &Chain, base: &PalwChainStateV2, daa: u64, anchor: Hash64, claim: &Hash64) -> Result<Vec<PalwPanelSeatV2>, String> {
    let b = bundle(&c.p);
    let window = c.p.palw_bond_maturity.filter(|m| m.activation.is_active(daa)).map(|m| m.window_daa);
    // No settled anchor on these chains: the bootstrap waiver (`palw_settled_anchor_floor_daa_v1` = None).
    let maturity = palw_seat_maturity_floor_v1(daa, window.map(|w| palw_bond_maturity_window_v2(daa, w, None)));
    derive_panel_v2_with_policy(
        base,
        &b.panel,
        claim,
        anchor,
        c.sp.min_collateral_sompi(),
        maturity,
        c.p.palw_capability_bound_at(daa),
        policy_at(&c.p, &c.sp, daa),
    )
    .map_err(|e| e.to_string())
}

fn bound_obj(claim: Hash64, anchor: Hash64, seats: &[PalwPanelSeatV2]) -> Obj {
    Obj::PanelBound { claim, anchor, seats: seats.to_vec() }
}

/// **The seed the chain draws `claim`'s panel from in an anchor block with identity `block` whose
/// attempt is `att`** — `PalwAnchorFactV2::panel_seed`, exactly as the processor's anchor walk builds
/// the fact under [`t12_f1`] (`palw_v2_anchor_fact_with_seed_v1`: the execution commitment, the
/// attempt's fold key `att.1`, when the fence is active at the anchor's DAA; T3a reads the same value
/// off real headers). Past the fence the block identity is in the fact and nowhere in the seed.
fn chain_seed(block: Hash64, daa: u64, att: &Att, claim: &Hash64) -> Hash64 {
    chain_seed_of(block, daa, att.1, claim)
}

/// [`chain_seed`] for an anchor attempt given by its execution commitment alone.
fn chain_seed_of(block: Hash64, daa: u64, execution: Hash64, claim: &Hash64) -> Hash64 {
    PalwAnchorFactV2 {
        anchor_block: block,
        anchor_daa: daa,
        predecessor_daa: daa - 1,
        anchor_execution: seed_rule_armed(daa).then_some(execution),
    }
    .panel_seed(claim)
}

/// A deterministic stand-in for a block identity (t3a shows the anchor producer gets fresh ones for a
/// signature each).
fn identity(i: u64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).to_state();
    state.update(b"void-after-panel/identity");
    state.update(&i.to_le_bytes());
    Hash64::from_slice(state.finalize().as_bytes())
}

/// One bond's books: `(collateral, slashed, reserved, free slashable)`.
fn books(c: &Chain, s: &PalwChainStateV2, bond: &PalwBondKeyV2, daa: u64) -> (u64, u64, u128, u128) {
    let rec = s.bond(bond).expect("the bond");
    let free = palw_bond_free_slashable_v1(s, &c.sp, bond, daa, c.extras_at(daa).settled_anchor_depth);
    (rec.collateral, rec.slashed, s.reserved_exposure(bond), free)
}

fn seats_label(s: &[PalwPanelSeatV2], coalition: &BTreeSet<PalwBondKeyV2>) -> String {
    s.iter().map(|seat| if coalition.contains(&seat.bond) { "S".to_string() } else { "h".to_string() }).collect::<Vec<_>>().join("")
}

/// A chain with the attacker's producing bond (and optionally `MINER`) registered at DAA 1,001.
fn chain_with(p: Params, att_collateral: u64, miner_collateral: Option<u64>, extra: Vec<Obj>) -> Run {
    let mut run = Run::new(Chain::new(p));
    let mut objects = vec![bond_obj(ATT, att_collateral)];
    if let Some(mc) = miner_collateral {
        objects.push(bond_obj(MINER, mc));
    }
    objects.extend(extra);
    run.beat(1_001, objects);
    run
}

// =================================================================================================
// T1 — an unbound claim voids in its anchor block; every balance read back
// =================================================================================================

#[test]
fn t1_an_unbound_claim_voids_in_its_anchor_block_with_nothing_owed() {
    let p = t12_f1();
    let ad = anchor_delay(&p);
    let floor = floor_id(&p);
    let rich = 1_000_000 * 100_000_000u64; // 1,000,000 MSK
    let mut run = chain_with(p.clone(), at_least_the_floor(&p, 13_000 * 100_000_000), Some(rich), vec![]);
    let att = bond_key(ATT);
    let before = books(&run.c, run.s(), &att, run.daa());
    let immature0 = run.s().bounded_immature();
    let safe0 = run.s().safe_weight();
    let frontier0 = run.s().safe_frontier();
    let produced0 = run.s().epoch_counter(&floor).map(|c| c.produced_blocks).unwrap_or(0);

    // The claim.
    let d1 = run.daa() + 1;
    let (claim, skips) = run.attempt_block(ATT, d1, h(0xC1A1), vec![]);
    assert!(skips.is_empty(), "{skips:?}");
    let rec = run.s().claim(&claim).expect("the claim").clone();
    let held = books(&run.c, run.s(), &att, d1);
    let commitment = palw_claim_commitment_v1(&run.c.sp, &rec, d1).expect("a commitment");
    let slot = rec.bind_base_daa() + ad;
    println!("=== T1: an honest floor claim whose anchor block binds nothing ===");
    println!(
        "claim {claim} accepted at DAA {d1}; slot = accepted + anchor_delay {ad} = {slot}; reserved w = {} + escrow term -> commitment {} sompi ({:.4} MSK)",
        rec.reserved,
        commitment,
        msk(commitment)
    );
    println!("attacker bond (collateral, slashed, reserved, free slashable): before {before:?} -> with the claim {held:?}");
    assert_eq!(held.2 - before.2, commitment, "the claim holds its whole commitment on the producer's bond");

    // Heartbeats to the slot: nothing anchors, the claim waits.
    run.beat_to(slot + 1);
    assert_eq!(run.s().claim(&claim).unwrap().phase, PalwClaimPhaseV2::Provisional, "claimless blocks never anchor");
    let parent = run.s().clone();

    // The anchor block (MINER's attempt at the slot) with NO binding among its objects — what the
    // chain folds when its derivation, the gate or the fold refused the draw.
    let anchor_daa = run.daa() + 1;
    assert!(anchor_daa >= slot);
    let (miner_claim, skips) = run.attempt_block(MINER, anchor_daa, h(0xA7C0), vec![]);
    assert!(skips.is_empty(), "{skips:?}");
    let s = run.s().clone();
    let claim_immature = rec.immature_contribution;
    let miner_immature = s.claim(&miner_claim).expect("the anchor block's own claim").immature_contribution;
    let after = books(&run.c, &s, &att, anchor_daa);
    let phase = s.claim(&claim).unwrap().phase.clone();
    println!("anchor block at DAA {anchor_daa} carries no binding -> {phase:?}");
    println!("attacker bond after the void: {after:?}");
    println!(
        "panel liability row: {:?}; slashable locks on the claim: {}; vesting row: {:?}; strikes: {:?}; claim economics: {:?}",
        s.panel_liability(&claim).is_some(),
        genesis_bonds(&p).iter().filter(|(k, _, _)| s.slashable_lock(*k, claim).is_some()).count(),
        s.vesting_row(&claim).is_some(),
        s.withholding_strikes(&att),
        s.claim_economics_of(&claim).is_some()
    );
    println!(
        "reward status {:?}; bounded_immature {immature0} -> +{claim_immature} (the claim) -> {} after the void (= the anchor block's own claim, {miner_immature}); safe_weight {safe0} -> {}; frontier {:?} -> {:?}",
        palw_reward_status_v2(&phase),
        s.bounded_immature(),
        s.safe_weight(),
        frontier0,
        s.safe_frontier()
    );
    println!(
        "the claim's next deadline: {:?} (retirement), floor produced_blocks {produced0} -> {} (the void refunds no production)",
        s.deadline_of(&claim),
        s.epoch_counter(&floor).map(|c| c.produced_blocks).unwrap_or(0)
    );

    assert_eq!(phase, PalwClaimPhaseV2::Voided { voided_daa: anchor_daa, reason: PalwVoidReasonV2::BindTimeout });
    assert_eq!((after.0, after.1), (before.0, before.1), "S0: no collateral moves, nothing slashed");
    assert_eq!(after.2, before.2, "the whole commitment is released at the anchor block");
    assert_eq!(after.3, before.3, "free slashable stake is back where it was");
    assert!(s.panel_liability(&claim).is_none(), "#12(a): a BindTimeout nobody signed leaves no liability row");
    assert!(s.vesting_row(&claim).is_none() && s.withholding_strikes(&att).is_none());
    assert_eq!(palw_reward_status_v2(&phase), PalwRewardStatusV2::Forfeited, "the carve is forgone, never taken from the bond");
    assert!(claim_immature > 0);
    assert_eq!(s.bounded_immature(), immature0 + miner_immature, "W5: the voided claim's immature weight leaves at the void");
    assert_eq!((s.safe_weight(), s.safe_frontier()), (safe0, frontier0), "a voided claim confers no weight and no frontier");
    s.assert_deadline_consistency(&run.c.sp).expect("the deadline index is the claims' recomputed deadlines");

    // The twin: the same anchor block with the derived binding binds the claim and puts five seats on duty.
    let base = pre_object_base(&run.c, &parent, h(0xA7C0), anchor_daa);
    let anchor_att = run.blocks.last().unwrap().attempt.clone().unwrap();
    let seed = chain_seed(h(0xA7C0), anchor_daa, &anchor_att, &claim);
    let seats = derive_at(&run.c, &base, anchor_daa, seed, &claim).expect("genesis binds under the stake draw");
    let twin = fold_on(
        &run.c,
        &parent,
        h(0xA7C0),
        anchor_daa,
        &[bound_obj(claim, seed, &seats)],
        Some(&anchor_att),
        T12_BLOCK_SUBSIDY_SOMPI,
        Some(ad),
    )
    .expect("the anchor block with its binding folds")
    .0;
    let duties: u128 = seats.iter().map(|seat| twin.reserved_exposure(&seat.bond) - parent.reserved_exposure(&seat.bond)).sum();
    println!(
        "twin with the derived binding: {:?}; producer reserved {} (held); five seats' duty reserved {duties} sompi",
        twin.claim(&claim).unwrap().phase,
        twin.reserved_exposure(&att)
    );
    assert!(matches!(twin.claim(&claim).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }));
    assert_eq!(twin.reserved_exposure(&att), held.2, "bound, the producer's commitment stays held");
}

// =================================================================================================
// T2 — when the panel becomes knowable, and what is free after it
// =================================================================================================

#[test]
fn t2a_the_panel_is_a_function_of_the_anchor_attempt_not_of_the_blocks_identity() {
    let p = t12_f1();
    let ad = anchor_delay(&p);
    let mut run = chain_with(p.clone(), at_least_the_floor(&p, 13_000 * 100_000_000), Some(1_000_000 * 100_000_000), vec![]);
    let d1 = run.daa() + 1;
    let (claim, _) = run.attempt_block(ATT, d1, h(0xC1A2), vec![]);
    let (second, _) = run.attempt_block(MINER, d1 + 1, h(0xC1B2), vec![]);
    let slot = run.s().claim(&second).unwrap().bind_base_daa() + ad;
    run.beat_to(slot);
    let parent = run.s().clone();
    let anchor_daa = slot;
    let base = pre_object_base(&run.c, &parent, identity(0), anchor_daa);

    // The same claim, the same state, the same DAA — 256 anchor identities carrying ONE anchor
    // attempt (T3a: what one lottery win buys), and 256 anchor attempts (256 lottery wins).
    const N: u64 = 256;
    let one = run.next_attempt(MINER);
    let (mut by_identity, mut by_attempt): (BTreeSet<Vec<PalwBondKeyV2>>, BTreeSet<Vec<PalwBondKeyV2>>) = Default::default();
    let mut seated: BTreeSet<PalwBondKeyV2> = BTreeSet::new();
    for i in 0..N {
        let seats = derive_at(&run.c, &base, anchor_daa, chain_seed(identity(i), anchor_daa, &one, &claim), &claim).expect("binds");
        by_identity.insert(seats.iter().map(|s| s.bond).collect());
        let other = run.next_attempt(MINER);
        let seats = derive_at(&run.c, &base, anchor_daa, chain_seed(identity(0), anchor_daa, &other, &claim), &claim).expect("binds");
        seated.extend(seats.iter().map(|s| s.bond));
        by_attempt.insert(seats.iter().map(|s| s.bond).collect());
    }
    // And the other input: the claim id (fixed at acceptance) moves it too, under one anchor attempt.
    let other_claim = derive_at(&run.c, &base, anchor_daa, chain_seed(identity(0), anchor_daa, &one, &second), &second)
        .map(|s| s.iter().map(|x| x.bond).collect::<Vec<_>>());
    let first_claim = derive_at(&run.c, &base, anchor_daa, chain_seed(identity(0), anchor_daa, &one, &claim), &claim)
        .map(|s| s.iter().map(|x| x.bond).collect::<Vec<_>>());
    println!("=== T2a (fixed): what the panel is a function of ===");
    println!(
        "claim accepted at DAA {d1}; slot {slot}; anchor = the first ATTEMPT block of the chain at or past the slot; \
         seed = H(anchor attempt's execution commitment ‖ claim)"
    );
    println!(
        "{N} anchor identities, one anchor attempt -> {} panel(s); {N} anchor attempts -> {} distinct ordered panels over {} bonds",
        by_identity.len(),
        by_attempt.len(),
        seated.len()
    );
    println!(
        "one anchor attempt, the other claim (MINER's, accepted one DAA later): {}",
        if other_claim == first_claim { "the same panel" } else { "a different panel" }
    );
    // **Below the fence, the rule testnet-12 launched with**: the same attempt, the same state, the
    // anchor fact without the execution (what the walk builds for an anchor below `F1_AT`) — the seed
    // is the block's identity and the 256 identities draw many panels. The fence is what closes it.
    let released: BTreeSet<Vec<PalwBondKeyV2>> = (0..N)
        .map(|i| {
            let fact =
                PalwAnchorFactV2 { anchor_block: identity(i), anchor_daa, predecessor_daa: anchor_daa - 1, anchor_execution: None };
            derive_at(&run.c, &base, anchor_daa, fact.panel_seed(&claim), &claim).expect("binds").iter().map(|s| s.bond).collect()
        })
        .collect();
    println!("the released rule (below the fence) on the same {N} identities: {} distinct panels", released.len());
    assert!(seed_rule_armed(anchor_daa), "the anchor is past the fence");
    assert!(released.len() as u64 > N / 2, "below the fence the identity still keys the draw (the rule testnet-12 launched with)");
    assert_eq!(by_identity.len(), 1, "the anchor block's identity no longer enters the draw");
    assert!(by_attempt.len() as u64 > N / 2, "each anchor attempt draws its own panel");
    assert!(seated.len() >= 6, "every eligible genesis bond is reachable");
    // Before the anchor exists there is no seed at all: a claimless block cannot anchor (T1), and the
    // claim's own block cannot (slot = accepted + {ad} > accepted).
    assert!(ad >= 1);
}

#[test]
fn t2b_after_the_panel_the_first_receipt_timeout_redraws_at_s0_and_the_second_forfeits() {
    let p = t12_f1();
    let ad = anchor_delay(&p);
    let mut run = chain_with(p.clone(), at_least_the_floor(&p, 13_000 * 100_000_000), Some(1_000_000 * 100_000_000), vec![]);
    let att = bond_key(ATT);
    let before = books(&run.c, run.s(), &att, run.daa());
    let d1 = run.daa() + 1;
    let (claim, _) = run.attempt_block(ATT, d1, h(0xC1A3), vec![]);
    let commitment = run.s().reserved_exposure(&att) - before.2;

    // Panel #1, bound by the chain in the anchor block (MINER's attempt).
    let slot = run.s().claim(&claim).unwrap().bind_base_daa() + ad;
    run.beat_to(slot);
    let a1 = identity(0xA1);
    let base = pre_object_base(&run.c, run.s(), a1, slot);
    let att1 = run.next_attempt(MINER);
    let seed1 = chain_seed(a1, slot, &att1, &claim);
    let p1 = derive_at(&run.c, &base, slot, seed1, &claim).expect("panel #1");
    run.attempt_block_of(att1, slot, a1, vec![bound_obj(claim, seed1, &p1)]);
    let PalwClaimPhaseV2::PanelBound { bound_daa } = run.s().claim(&claim).unwrap().phase else { panic!("bound") };
    let seats_on_duty: u128 = p1.iter().map(|s| run.s().reserved_exposure(&s.bond)).sum();
    let receipt_deadline = run.s().deadline_of(&claim).expect("the receipt deadline");

    // The producer, having seen panel #1 (public since the anchor block), serves nothing and signs
    // nothing. No seat files (the unaccused case: DA-5's pause needs a SEAT session, t67).
    run.beat_to(receipt_deadline + 1);
    run.beat(receipt_deadline + 1, vec![]);
    let redrawn = run.s().claim(&claim).unwrap().clone();
    let at_redraw = books(&run.c, run.s(), &att, run.daa());
    let seats_after: u128 = p1.iter().map(|s| run.s().reserved_exposure(&s.bond)).sum();
    println!("=== T2b: after the panel is public ===");
    println!("panel #1 bound at DAA {bound_daa} (anchor {a1}); receipt deadline {receipt_deadline}");
    println!(
        "first ReceiptTimeout at DAA {}: {:?}, rebound_daa {:?}; attacker books {at_redraw:?} (before the claim {before:?}); \
         panel #1 seats' reserved {seats_on_duty} -> {seats_after}",
        run.daa(),
        redrawn.phase,
        redrawn.rebound_daa
    );
    assert_eq!(redrawn.phase, PalwClaimPhaseV2::Provisional, "the first timeout redraws");
    assert_eq!((at_redraw.0, at_redraw.1), (before.0, before.1), "S0: nothing is charged for the first failed panel");
    assert_eq!(at_redraw.2 - before.2, commitment, "the commitment stays held across the redraw");

    // Panel #2 on the redraw's anchor.
    let slot2 = redrawn.bind_base_daa() + ad;
    run.beat_to(slot2);
    let a2 = identity(0xA2);
    let base2 = pre_object_base(&run.c, run.s(), a2, slot2);
    let att2 = run.next_attempt(MINER);
    let seed2 = chain_seed(a2, slot2, &att2, &claim);
    let p2 = derive_at(&run.c, &base2, slot2, seed2, &claim).expect("panel #2");
    run.attempt_block_of(att2, slot2, a2, vec![bound_obj(claim, seed2, &p2)]);
    let deadline2 = run.s().deadline_of(&claim).expect("the second receipt deadline");
    run.beat_to(deadline2 + 1);
    run.beat(deadline2 + 1, vec![]);
    let end = books(&run.c, run.s(), &att, run.daa());
    let phase = run.s().claim(&claim).unwrap().phase.clone();
    println!(
        "panel #2 (anchor {a2}) {} panel #1; second ReceiptTimeout -> {phase:?}; attacker books {end:?}; charged {} sompi (commitment {commitment})",
        if p1 == p2 { "==" } else { "!=" },
        end.1 - before.1
    );
    assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. }));
    assert_eq!(end.1 - before.1, commitment as u64, "S0′: the second failed panel forfeits the commitment");
    assert_eq!(end.2, before.2);
}

// =================================================================================================
// T3 — who controls the draw: one lottery win, many anchors; author and non-author
// =================================================================================================

#[test]
fn t3a_one_lottery_win_is_one_panel_whatever_identity_its_block_takes() {
    let p = t12_f1();
    let b = bundle(&p);
    let g = genesis_state(&p);
    let floor = floor_id(&p);
    let net = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    let target = kaspa_consensus_core::palw_admission_v2::palw_effective_class_target_v1(&g, &b.state, &floor, None)
        .expect("the floor's effective target");
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x5Eu8; 32]);
    let bond = bond_key(ATT);
    let lane = p.palw_attempt_lane_at(1_000);
    let algo = lane.attempt_algo_id();
    let version = lane.attempt_version();

    let parents: kaspa_consensus_core::header::CompressedParents = vec![vec![h(0x9A7E)]].try_into().unwrap();
    let header0 = Header::new_finalized(
        kaspa_consensus_core::constants::BLOCK_VERSION,
        parents,
        Default::default(),
        Default::default(),
        Default::default(),
        p.genesis.timestamp + 1_000 * p.target_time_per_block(),
        0x207f_ffff,
        (0xC0FFEEu64 << PALW_TICKET_NONCE_BUCKET_LOG2) + 5,
        algo,
        1_000,
        kaspa_consensus_core::BlueWorkType::from_u64(0),
        1_000,
        Default::default(),
    );
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&header0);
    let exec_anchor = execution_anchor_v3(net, pre_pow, floor, &bond.0, header0.nonce);
    let mut attempt = PalwAttemptUnsignedV2 {
        version,
        network_domain: net,
        challenge: challenge_v2(net, pre_pow, header0.timestamp, header0.nonce, floor, &bond.0),
        class_id: floor,
        executor_bond: bond.0,
        executor_pubkey: kp.verification_key.as_ref().to_vec(),
        operator_id: h(0x0B),
        artifact_root: g.class(&floor).unwrap().artifact_root,
        trace_root: Hash64::default(),
        output_root: h(0x0707),
        pwu: 1,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: 999_999,
        execution_root: h(0xE0E0),
    };
    // The lottery: a junk trace root per draw, one BLAKE2b each (no inference — audit P0-10, open).
    let mut next_root = 0u64;
    let mut win = |attempt: &mut PalwAttemptUnsignedV2| -> u64 {
        let mut draws = 0u64;
        loop {
            draws += 1;
            next_root += 1;
            attempt.trace_root = identity(0x7A00_0000 + next_root);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            if class_ticket_v3(attempt, exec_anchor) <= target {
                return draws;
            }
        }
    };
    let t = Instant::now();
    let draws = win(&mut attempt);
    let lottery_ms = t.elapsed().as_secs_f64() * 1e3;
    let expected_draws = kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(target);
    let exec0 = execution_commitment_v3(&attempt, exec_anchor);
    let verify = |env: &PalwAttemptEnvelopeV2| {
        env.validate_signature_v2(|key, msg, sig, ctx| {
            let (Ok(key), Ok(sig)) = (<[u8; 2592]>::try_from(key), <[u8; 4627]>::try_from(sig)) else { return false };
            libcrux_ml_dsa::ml_dsa_87::portable::verify(
                &libcrux_ml_dsa::ml_dsa_87::MLDSA87VerificationKey::new(key),
                msg,
                ctx,
                &libcrux_ml_dsa::ml_dsa_87::MLDSA87Signature::new(sig),
            )
            .is_ok()
        })
    };
    let sign = |a: &PalwAttemptUnsignedV2, rnd: u64| {
        let mut r = [0u8; 32];
        r[..8].copy_from_slice(&rnd.to_le_bytes());
        let sig =
            libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, attempt_id_v2(a).as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT, r)
                .expect("sign");
        PalwAttemptEnvelopeV2 { attempt: a.clone(), signature: sig.as_ref().to_vec() }
    };

    // N re-signatures of the SAME winning attempt: every one a valid block, each its own identity —
    // and every header names the same anchor execution commitment (the walk's own reading of it).
    const N: u64 = 1_000;
    let mut identities: Vec<(Hash64, Hash64)> = Vec::new();
    let t = Instant::now();
    for rnd in 0..N {
        let env = sign(&attempt, rnd);
        let mut hdr = header0.clone();
        hdr.palw_commitment = env.encode_wire();
        hdr.finalize();
        assert_eq!(kaspa_consensus_core::hashing::header::pre_pow_hash_64(&hdr), pre_pow, "the same PoW position");
        env.validate_stateless_v2_at_version(version, net, pre_pow, hdr.timestamp, hdr.nonce).expect("the envelope binds the header");
        verify(&env).expect("the relay path's signature check passes");
        assert!(class_ticket_v3(&env.attempt, exec_anchor) <= target, "the same winning ticket");
        identities.push((hdr.hash, palw_panel_anchor_execution_v1(net, &hdr).expect("an attempt header")));
    }
    let sign_ms = t.elapsed().as_secs_f64() * 1e3 / N as f64;

    // And the other free moves: another nonce inside the bucket, another timestamp.
    for (dn, dt) in [(1u64, 0u64), (2, 0), (0, 1), (0, 2), (3, 5)] {
        let mut hdr = header0.clone();
        hdr.nonce += dn;
        hdr.timestamp += dt;
        assert_eq!(palw_nonce_bucket_v1(hdr.nonce), palw_nonce_bucket_v1(header0.nonce));
        let pre_pow_n = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&hdr);
        let mut moved = attempt.clone();
        moved.challenge = challenge_v2(net, pre_pow_n, hdr.timestamp, hdr.nonce, floor, &bond.0);
        let anchor_n = execution_anchor_v3(net, pre_pow_n, floor, &bond.0, hdr.nonce);
        let env = sign(&moved, 0);
        hdr.palw_commitment = env.encode_wire();
        hdr.finalize();
        env.validate_stateless_v2_at_version(version, net, pre_pow_n, hdr.timestamp, hdr.nonce).expect("binds");
        verify(&env).expect("signed");
        assert!(anchor_n == exec_anchor && class_ticket_v3(&moved, anchor_n) == class_ticket_v3(&attempt, exec_anchor));
        identities.push((hdr.hash, palw_panel_anchor_execution_v1(net, &hdr).expect("an attempt header")));
    }
    let distinct: BTreeSet<Hash64> = identities.iter().map(|(id, _)| *id).collect();
    let executions: BTreeSet<Hash64> = identities.iter().map(|(_, e)| *e).collect();
    assert_eq!(distinct.len(), identities.len(), "every re-signature and every free move is a distinct valid block");
    assert_eq!(executions, BTreeSet::from([exec0]), "…and every one names the one execution commitment");

    // (a) Each identity is the SAME panel for any claim this block anchors.
    let mut run = chain_with(p.clone(), at_least_the_floor(&p, 13_000 * 100_000_000), Some(1_000_000 * 100_000_000), vec![]);
    let d1 = run.daa() + 1;
    let (claim, _) = run.attempt_block(MINER, d1, h(0xC1A4), vec![]);
    let slot = run.s().claim(&claim).unwrap().bind_base_daa() + anchor_delay(&p);
    run.beat_to(slot);
    let base = pre_object_base(&run.c, run.s(), identity(1), slot);
    let seed_of = |id: Hash64, execution: Hash64| chain_seed_of(id, slot, execution, &claim);
    let panels: BTreeSet<Vec<PalwBondKeyV2>> = identities
        .iter()
        .map(|(id, e)| derive_at(&run.c, &base, slot, seed_of(*id, *e), &claim).unwrap().iter().map(|s| s.bond).collect())
        .collect();
    let by_identity: BTreeSet<Vec<PalwBondKeyV2>> = identities
        .iter()
        .take(64)
        .map(|(id, _)| derive_at(&run.c, &base, slot, *id, &claim).unwrap().iter().map(|s| s.bond).collect())
        .collect();

    // (b) Another panel costs another execution commitment: another lottery win.
    const WINS: usize = 16;
    let mut win_draws = vec![draws];
    let mut win_executions = BTreeSet::from([exec0]);
    let mut win_panels = panels.clone();
    for _ in 1..WINS {
        win_draws.push(win(&mut attempt));
        let execution = execution_commitment_v3(&attempt, exec_anchor);
        assert!(win_executions.insert(execution), "a new win is a new execution commitment");
        win_panels
            .insert(derive_at(&run.c, &base, slot, seed_of(identity(1), execution), &claim).unwrap().iter().map(|s| s.bond).collect());
    }
    let mean_draws = win_draws.iter().sum::<u64>() as f64 / WINS as f64;

    println!("=== T3a (fixed): what one re-roll of the panel costs ===");
    println!(
        "lane algo {algo}, envelope v{version}; floor effective target: {expected_draws} expected draws per win; the first win took {draws} junk draws \
         (one BLAKE2b ticket each, no inference), {lottery_ms:.1} ms (debug)"
    );
    println!(
        "{N} ML-DSA-87 re-signatures ({sign_ms:.2} ms each, debug) + 5 nonce/timestamp moves: {} distinct valid block identities, \
         {} execution commitment(s) read off their headers -> {} panel(s) for one claim anchored there (the identity-keyed draw gave {} over the first 64)",
        distinct.len(),
        executions.len(),
        panels.len(),
        by_identity.len()
    );
    println!(
        "{WINS} lottery wins (a mean {mean_draws:.0} junk BLAKE2b draws each while P0-10 is open) -> {} execution commitments -> {} distinct panels",
        win_executions.len(),
        win_panels.len()
    );
    assert_eq!(panels.len(), 1, "(a) 1,005 identities of one anchor attempt draw ONE panel");
    assert!(by_identity.len() > 32, "the identity-keyed draw (the closed hole) moved with the identity");
    assert!(win_panels.len() > WINS / 2, "(b) new wins, new panels: {} of {WINS}", win_panels.len());
}

/// The coalition's three seat Sybils at the panel floor, and the state/chain to draw on.
fn coalition_run(p: &Params) -> (Run, BTreeSet<PalwBondKeyV2>) {
    let economy = p.palw_seat_economy_at(1_000).expect("t12 arms the panel economy");
    let seat_floor = economy.panel_floor_sompi;
    // Declared capable of the floor (a genesis class: no production proof needed, `palw_bond_may_judge_class_v3`).
    let floor = floor_id(p);
    let extra: Vec<Obj> = SYBILS
        .iter()
        .map(|n| match bond_obj(*n, seat_floor) {
            Obj::BondRegistered { bond, pubkey, operator_pubkey, collateral, payout_payload, signature, .. } => Obj::BondRegistered {
                bond,
                pubkey,
                operator_pubkey,
                collateral,
                payout_payload,
                capable_classes: [floor].into_iter().collect(),
                signature,
            },
            _ => unreachable!(),
        })
        .collect();
    let mut run = chain_with(p.clone(), at_least_the_floor(p, 13_000 * 100_000_000), Some(1_000_000 * 100_000_000), extra);
    // Past the bond-maturity window, so the Sybils are seat-eligible (registered by the maturity floor).
    let window = p.palw_bond_maturity.map(|m| m.window_daa).unwrap_or(0);
    println!("testnet-12 bond maturity: {:?}", p.palw_bond_maturity.map(|m| (m.activation.daa_score(), m.window_daa)));
    run.beat(1_002 + window, vec![]);
    (run, SYBILS.iter().map(|n| bond_key(*n)).collect())
}

#[test]
fn t3b_the_anchor_producer_cannot_pick_the_panel_for_its_own_claim_or_a_victims() {
    let p = t12_f1();
    let ad = anchor_delay(&p);
    let quorum = kaspa_consensus_core::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1 as usize;
    let (mut run, coalition) = coalition_run(&p);
    let economy = p.palw_seat_economy_at(1_000).unwrap();
    let genesis_weight: u64 = genesis_bonds(&p).iter().map(|g| g.2.min(1_000_000 * 100_000_000)).sum();
    let coalition_weight = economy.panel_floor_sompi * SYBILS.len() as u64;
    let share = coalition_weight as f64 / (coalition_weight + genesis_weight) as f64;

    // ---- two claims whose slots two DIFFERENT anchor blocks reach: the attacker's, then a victim's --
    let d1 = run.daa() + 1;
    let (own, _) = run.attempt_block(ATT, d1, h(0xC1A5), vec![]);
    run.beat(d1 + 1, vec![]);
    let (victim, _) = run.attempt_block(MINER, d1 + 2, h(0xC1A6), vec![]);
    let slot_own = run.s().claim(&own).unwrap().bind_base_daa() + ad;
    let slot_victim = run.s().claim(&victim).unwrap().bind_base_daa() + ad;
    assert!(slot_victim > slot_own);
    let captured = |seats: &[PalwPanelSeatV2]| seats.iter().filter(|s| coalition.contains(&s.bond)).count() >= quorum;

    // ---- the author's slot: the grind over identities of ONE anchor attempt ----------------------
    run.beat_to(slot_own);
    let base = pre_object_base(&run.c, run.s(), identity(2), slot_own);
    const TRIES: u64 = 4_096;
    let grind_identities = |c: &Chain, base: &PalwChainStateV2, daa: u64, att: &Att, claim: &Hash64, from: u64| {
        let (mut panels, mut hits) = (BTreeSet::new(), 0u64);
        for i in 0..TRIES {
            let seats = derive_at(c, base, daa, chain_seed(identity(from + i), daa, att, claim), claim).unwrap();
            hits += u64::from(captured(&seats));
            panels.insert(seats.iter().map(|s| s.bond).collect::<Vec<_>>());
        }
        (panels.len(), hits)
    };
    let att_own = run.next_attempt(ATT);
    let (own_panels, own_hits) = grind_identities(&run.c, &base, slot_own, &att_own, &own, 0x2_0000);
    // What a capture costs now: a fresh execution commitment per try — each one a lottery win (T3a
    // (b): a new win is a new commitment), stood in for here by a fresh 64-byte value.
    let (mut per_win_hits, mut sybil_seatings) = (0u64, 0u64);
    let t = Instant::now();
    for i in 0..TRIES {
        let seats =
            derive_at(&run.c, &base, slot_own, chain_seed_of(identity(2), slot_own, identity(0xE5E0_0000 + i), &own), &own).unwrap();
        sybil_seatings += seats.iter().filter(|s| coalition.contains(&s.bond)).count() as u64;
        per_win_hits += u64::from(captured(&seats));
    }
    let derive_ms = t.elapsed().as_secs_f64() * 1e3 / TRIES as f64;
    let expected_draws = {
        let b = bundle(&p);
        let target =
            kaspa_consensus_core::palw_admission_v2::palw_effective_class_target_v1(&genesis_state(&p), &b.state, &floor_id(&p), None)
                .expect("the floor's effective target");
        kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(target)
    };
    // The chain binds the one panel of the attacker's anchor attempt.
    let seed_own = chain_seed(h(0xA7_0001), slot_own, &att_own, &own);
    let seats_own = derive_at(&run.c, &base, slot_own, seed_own, &own).unwrap();
    run.attempt_block_of(att_own, slot_own, h(0xA7_0001), vec![bound_obj(own, seed_own, &seats_own)]);
    assert_eq!(run.s().panel(&own).map(|x| (x.anchor, x.seats.clone())), Some((seed_own, seats_own.clone())));

    // ---- the NON-AUTHOR: the attacker's next attempt block is the VICTIM's anchor ------------------
    run.beat_to(slot_victim);
    let base_v = pre_object_base(&run.c, run.s(), identity(4), slot_victim);
    let att_v = run.next_attempt(ATT);
    let (victim_panels, victim_hits) = grind_identities(&run.c, &base_v, slot_victim, &att_v, &victim, 0x4_0000_0000);
    let seed_v = chain_seed(h(0xA7_0002), slot_victim, &att_v, &victim);
    let seats_v = derive_at(&run.c, &base_v, slot_victim, seed_v, &victim).unwrap();
    run.attempt_block_of(att_v, slot_victim, h(0xA7_0002), vec![bound_obj(victim, seed_v, &seats_v)]);
    assert_eq!(run.s().panel(&victim).map(|x| x.seats.clone()), Some(seats_v.clone()), "the chain bound the one panel");

    println!("=== T3b (fixed): the anchor block's producer no longer picks the panel ===");
    println!(
        "coalition: {} seat Sybils at the panel floor {:.0} MSK = {:.2}% of the draw weight; colluding quorum {quorum} of {}",
        SYBILS.len(),
        msk(economy.panel_floor_sompi as u128),
        share * 100.0,
        bundle(&p).panel.seat_count()
    );
    println!(
        "author: {TRIES} identities of its one anchor attempt -> {own_panels} panel(s), coalition quorum in {own_hits}; bound {}",
        seats_label(&seats_own, &coalition)
    );
    println!(
        "non-author (a victim's claim): {TRIES} identities of the attacker's one anchor attempt -> {victim_panels} panel(s), coalition quorum in {victim_hits}; bound {}",
        seats_label(&seats_v, &coalition)
    );
    println!(
        "a capture now takes new execution commitments: {per_win_hits} of {TRIES} fresh anchor attempts gave a quorum ({:.5}; Sybil seatings {:.3}/panel), \
         each a lottery win of ~{expected_draws} junk BLAKE2b draws while P0-10 is open ({derive_ms:.2} ms per derivation, debug)",
        per_win_hits as f64 / TRIES as f64,
        sybil_seatings as f64 / TRIES as f64
    );
    assert_eq!((own_panels, victim_panels), (1, 1), "identities of one anchor attempt draw one panel, author or not");
    assert!(own_hits == 0 || own_hits == TRIES, "the identity grind cannot move the outcome");
    assert!(victim_hits == 0 || victim_hits == TRIES, "the identity grind cannot move the outcome");
    assert!(sybil_seatings > 0, "the Sybils are seat-eligible: the draw still reaches them at their weight");
}

// =================================================================================================
// T4 — 1,000 cycles on one bond
// =================================================================================================

#[test]
fn t4_a_thousand_claim_void_cycles_on_one_bond_accumulate_nothing() {
    let p = t12_f1();
    let ad = anchor_delay(&p);
    let floor = floor_id(&p);
    let floor_bond = at_least_the_floor(&p, 13_000 * 100_000_000);
    let mut run = chain_with(p.clone(), floor_bond, None, vec![]);
    run.check = false;
    let att = bond_key(ATT);
    let before = books(&run.c, run.s(), &att, run.daa());
    let bytes0 = carriage_bytes(run.s());
    const CYCLES: u64 = 1_000;
    // Each attempt block is the anchor of the previous claim (voided there, unbound) and makes the next.
    let mut daa = run.daa() + 1;
    let mut ids = Vec::new();
    let t = Instant::now();
    let mut peak_claims = 0usize;
    for i in 0..CYCLES {
        let (id, skips) = run.attempt_block(ATT, daa, identity(0x44_0000 + i), vec![]);
        assert!(skips.is_empty(), "cycle {i}: the bond has room again at once: {skips:?}");
        ids.push(id);
        peak_claims = peak_claims.max(run.s().claims_iter().count());
        daa += ad;
    }
    let ms = t.elapsed().as_millis();
    let s = run.s();
    let after = books(&run.c, s, &att, run.daa());
    let voided = ids.iter().filter(|id| matches!(s.claim(id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { .. }))).count();
    let retired = ids.iter().filter(|id| s.claim(id).is_none()).count();
    let one = palw_claim_commitment_v1(&run.c.sp, s.claim(ids.last().unwrap()).unwrap(), run.daa()).unwrap();
    println!("=== T4: {CYCLES} claim -> anchor-void -> claim cycles on one {:.0} MSK bond ===", msk(floor_bond as u128));
    println!(
        "{CYCLES} cycles over {} DAA in {ms} ms: voided {voided}, retired {retired}, live 1; claims held at most {peak_claims} (retirement after {} DAA)",
        daa - ad - 1_002,
        run.c.sp.claim_retirement_daa()
    );
    println!("bond books before {before:?} -> after {after:?} (the live claim's commitment {one})");
    println!(
        "strikes {:?}; liability rows {}; floor epoch produced_blocks {:?}; carriage {bytes0} -> {} bytes",
        s.withholding_strikes(&att),
        ids.iter().filter(|id| s.panel_liability(id).is_some()).count(),
        s.epoch_counter(&floor).map(|c| c.produced_blocks),
        carriage_bytes(s)
    );
    assert_eq!((after.0, after.1), (before.0, before.1), "no collateral moved, nothing slashed across 1,000 voids");
    assert_eq!(after.2 - before.2, one, "only the live claim is held");
    assert_eq!(voided + retired, CYCLES as usize - 1);
    assert!(s.withholding_strikes(&att).is_none(), "no strike, no cooldown, no debt");
    assert_eq!(ids.iter().filter(|id| s.panel_liability(id).is_some()).count(), 0);
}

// =================================================================================================
// T5 — what the void frees, and when
// =================================================================================================

#[test]
fn t5a_the_voided_reservation_is_reusable_in_the_next_block() {
    let p = t12_f1();
    let ad = anchor_delay(&p);
    let floor_bond = at_least_the_floor(&p, 13_000 * 100_000_000);
    let mut run = chain_with(p.clone(), floor_bond, Some(1_000_000 * 100_000_000), vec![]);
    let att = bond_key(ATT);
    // Fill the bond: a 13,000 MSK bond backs K = 2 concurrent floor claims.
    let d = run.daa() + 1;
    let (c1, s1) = run.attempt_block(ATT, d, identity(0x51), vec![]);
    let (c2, s2) = run.attempt_block(ATT, d + 1, identity(0x52), vec![]);
    let (c3, s3) = run.attempt_block(ATT, d + 2, identity(0x53), vec![]);
    println!("=== T5a: K on a {:.0} MSK bond, and the void's release ===", msk(floor_bond as u128));
    println!(
        "claims: #1 {:?} #2 {:?} #3 {:?} (skips {s1:?} {s2:?} {s3:?})",
        run.s().claim(&c1).is_some(),
        run.s().claim(&c2).is_some(),
        run.s().claim(&c3).is_some()
    );
    assert!(run.s().claim(&c1).is_some() && run.s().claim(&c2).is_some());
    assert!(run.s().claim(&c3).is_none() && !s3.is_empty(), "the third is over the ceiling: {s3:?}");
    let slot1 = run.s().claim(&c1).unwrap().bind_base_daa() + ad;
    run.beat_to(slot1);
    // The anchor block of claim #1 is the attacker's own attempt: step 4 runs before step 4c, so its
    // own attempt still meets the ceiling in this block and is skipped; #1 voids at the end of it.
    let (c4, s4) = run.attempt_block(ATT, slot1, identity(0x54), vec![]);
    println!(
        "anchor block of #1 at DAA {slot1} (the attacker's own attempt): #1 -> {:?}; its own attempt admitted {} ({s4:?})",
        run.s().claim(&c1).map(|c| c.phase.clone()),
        run.s().claim(&c4).is_some()
    );
    assert!(matches!(run.s().claim(&c1).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, .. }));
    assert!(run.s().claim(&c4).is_none(), "within the anchor block the room is not yet free");
    // The very next block (it is #2's anchor block, so #2 voids there too — after the admission).
    let (c5, s5) = run.attempt_block(ATT, slot1 + 1, identity(0x55), vec![]);
    println!(
        "next block (DAA {}): the attacker's attempt admitted {} ({s5:?}); #2 -> {:?}; reserved {}",
        slot1 + 1,
        run.s().claim(&c5).is_some(),
        run.s().claim(&c2).map(|c| c.phase.clone()),
        run.s().reserved_exposure(&att)
    );
    assert!(run.s().claim(&c5).is_some() && s5.is_empty(), "the voided claim's room is reused one block later");
}

#[test]
fn t5b_the_2m_lane_cap_is_free_again_at_the_void() {
    let p = t12_2m_open();
    let b = bundle(&p);
    let sp = b.state.clone();
    let ad = b.panel.anchor_delay();
    let fold0 = registry_fold(&p, 2_000).expect("t12 arms the registry");
    let d0 = fold0.grace_until_daa.max(1_000) + 10;
    let (_, id2m) = model_classes(&p);
    let ready: Vec<PalwBondKeyV2> = honest(&p).into_iter().take(8).collect();
    let g = readied(&sp, &activated(&sp, &genesis_state(&p), id2m), &ready, id2m, d0);
    let pwu = class_pwu(&p, &g, id2m, d0);
    let per = admitted_per_attempt_on(&p, &g, id2m, pwu, T12_BLOCK_SUBSIDY_SOMPI);
    let coll = at_least_the_floor(&p, per.collateral);
    const A1: u64 = 0x2A1;
    const A2: u64 = 0x2A2;
    let mut c = Chain::new(p.clone());
    c.room = true;
    c.s = g;
    c.daa = d0 - 1;
    let mut run = Run::new(c);
    run.check = false;
    run.beat(d0, vec![bond_obj(A1, coll), bond_obj(A2, coll), bond_obj(MINER, 1_000_000 * 100_000_000)]);
    let attempt_2m = |run: &mut Run, n: u64, seed: u64, daa: u64, anchor: bool| -> (Hash64, Result<Vec<String>, PalwStateV2Error>) {
        run.c.s = readied(&sp, &run.c.s, &ready, id2m, daa);
        let (env, key, id) = junk_attempt(id2m, bond_key(n), pubkey_of(n), &operator_pubkey_of(n), pwu, seed, 0x2D00 + seed);
        let job = execution_anchor_v3(h(NET), h(0x2D00 + seed), id2m, &bond_key(n).0, 7);
        let r =
            run.push(identity(0x2D_0000 + seed), daa, vec![], Some((env, key, job)), T12_BLOCK_SUBSIDY_SOMPI, anchor.then_some(ad));
        (id, r)
    };
    let (first, r1) = attempt_2m(&mut run, A1, 1, d0 + 1, true);
    assert!(r1.as_ref().is_ok_and(|s| s.is_empty()) && run.s().claim(&first).is_some(), "the first 2M claim: {r1:?}");
    let probe = {
        let parent = run.s().clone();
        let (env, key, _) = junk_attempt(id2m, bond_key(A2), pubkey_of(A2), &operator_pubkey_of(A2), pwu, 2, 0x2D02);
        fold_on(
            &run.c,
            &readied(&sp, &parent, &ready, id2m, d0 + 2),
            identity(0x2D_9999),
            d0 + 2,
            &[],
            Some(&(env, key, Hash64::default())),
            T12_BLOCK_SUBSIDY_SOMPI,
            Some(ad),
        )
        .map(|_| ())
    };
    println!("=== T5b: the 2M lane (cap 1, held to Final) ===");
    println!("while the first 2M claim is Provisional, a second 2M attempt -> {probe:?}");
    assert!(matches!(probe, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })), "{probe:?}");
    // Its anchor block: MINER's floor attempt at the slot, binding nothing.
    let slot = run.s().claim(&first).unwrap().bind_base_daa() + ad;
    run.beat_to(slot);
    run.c.s = readied(&sp, &run.c.s, &ready, id2m, slot);
    run.attempt_block(MINER, slot, identity(0x2D_A7C0), vec![]);
    let phase = run.s().claim(&first).unwrap().phase.clone();
    let a1 = books(&run.c, run.s(), &bond_key(A1), slot);
    println!("anchor block at DAA {slot} binds nothing -> the first 2M claim {phase:?}; A1 books {a1:?}");
    assert!(matches!(phase, PalwClaimPhaseV2::Voided { .. }));
    assert_eq!((a1.1, a1.2), (0, 0), "S0 on the 2M row too: nothing slashed, nothing held");
    let (second, r2) = attempt_2m(&mut run, A2, 2, slot + 1, true);
    println!("next block: a second 2M attempt -> {:?} (claim present {})", r2, run.s().claim(&second).is_some());
    assert!(r2.is_ok() && run.s().claim(&second).is_some(), "the 2M lane is free one block after the void");
}

// =================================================================================================
// T8 — restart, reorg, IBD
// =================================================================================================

#[test]
fn t8_the_void_replays_on_restart_and_a_reorg_between_anchors_moves_the_whole_obligation() {
    let p = t12_f1();
    let ad = anchor_delay(&p);
    let mut run = chain_with(p.clone(), at_least_the_floor(&p, 13_000 * 100_000_000), Some(1_000_000 * 100_000_000), vec![]);
    let att = bond_key(ATT);
    let d1 = run.daa() + 1;
    let (claim, _) = run.attempt_block(ATT, d1, h(0xC1A8), vec![]);
    let slot = run.s().claim(&claim).unwrap().bind_base_daa() + ad;
    run.beat_to(slot);
    let fork_at = run.blocks.len();
    let parent = run.s().clone();

    // Branch V: the anchor voids the claim. Branch B: the anchor binds its derived panel.
    let (env, key, _) = floor_attempt_of(&run.c, MINER, 0xB8);
    let job = floor_job_anchor(&p, bond_key(MINER), 0x10C0 + 0xB8);
    let att_blk = (env, key, job);
    let (v, dv, _) = fold_on(&run.c, &parent, identity(0x8A), slot, &[], Some(&att_blk), T12_BLOCK_SUBSIDY_SOMPI, Some(ad)).unwrap();
    let base = pre_object_base(&run.c, &parent, identity(0x8B), slot);
    let seed = chain_seed(identity(0x8B), slot, &att_blk, &claim);
    let seats = derive_at(&run.c, &base, slot, seed, &claim).unwrap();
    let (bnd, db, _) = fold_on(
        &run.c,
        &parent,
        identity(0x8B),
        slot,
        &[bound_obj(claim, seed, &seats)],
        Some(&att_blk),
        T12_BLOCK_SUBSIDY_SOMPI,
        Some(ad),
    )
    .unwrap();

    // Replay: the same inputs fold to the same void (step 4c is a pure function of the block).
    let (v2, dv2, _) = fold_on(&run.c, &parent, identity(0x8A), slot, &[], Some(&att_blk), T12_BLOCK_SUBSIDY_SOMPI, Some(ad)).unwrap();
    assert_eq!((v2.state_root(), &dv2), (v.state_root(), &dv), "the void replays deterministically");

    // Restart: the voided state's carriage, encoded and loaded under its root.
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&v)).unwrap();
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
    let loaded = carriage
        .into_state_v3(&run.c.sp, Some(v.state_root()), flags(&p, slot).uncertified_weightless, p.palw_canonical_work_daa())
        .expect("the voided state loads");
    assert_eq!(loaded, v, "restart loads the void as written");

    // Reorg V -> B -> V through the deltas.
    let back = revert_delta_v2(&v, &dv, &run.c.sp).unwrap();
    assert_eq!(back, parent);
    let to_b = apply_delta_v2(&back, &db, &run.c.sp).unwrap();
    assert_eq!(to_b, bnd);
    let seats_b: u128 = seats.iter().map(|s| bnd.reserved_exposure(&s.bond) - parent.reserved_exposure(&s.bond)).sum();
    let seats_v: u128 = seats.iter().map(|s| v.reserved_exposure(&s.bond) - parent.reserved_exposure(&s.bond)).sum();
    println!("=== T8: restart and reorg across the anchor block (fork after block {fork_at}) ===");
    println!(
        "branch V (void): claim {:?}, producer reserved {}, seats' duty {seats_v}; branch B (bound): claim {:?}, producer reserved {}, seats' duty {seats_b}",
        v.claim(&claim).unwrap().phase,
        v.reserved_exposure(&att),
        bnd.claim(&claim).unwrap().phase,
        bnd.reserved_exposure(&att)
    );
    println!(
        "replay: same root; restart: carriage of {} bytes loads under its root; reorg V->B->V: every state the recorded one",
        bytes.len()
    );
    let again = revert_delta_v2(&to_b, &db, &run.c.sp).and_then(|s| apply_delta_v2(&s, &dv, &run.c.sp)).unwrap();
    assert_eq!(again, v);
    assert!(seats_b > 0 && seats_v == 0);
    assert_eq!(
        v.reserved_exposure(&att),
        parent.reserved_exposure(&att) - palw_claim_commitment_v1(&run.c.sp, parent.claim(&claim).unwrap(), slot).unwrap()
    );
    assert_eq!(bnd.reserved_exposure(&att), parent.reserved_exposure(&att));
}
