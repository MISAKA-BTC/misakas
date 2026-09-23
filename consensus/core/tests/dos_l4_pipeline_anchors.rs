//! **LANE L4 (area 6) — the second clock's anchors must not be DAA-clock events.**
//!
//! The audit's finding (2026-09-24 DoS audit, §4b, finding 9): `settled_attempt_finals` ticked in
//! `finalize_claim`, whose only caller is `sweep_deadlines` when a `ReceiptLicensed` claim's
//! challenge deadline passes on `ctx.daa_score`. So every claim LICENSED before a heartbeat-only
//! stretch reached `Final` during it with no block but a claimless one — and each of those Finals
//! was an "anchor" for the liability rules. A colluding seat that signed `Valid` on a false claim
//! while normal traffic kept licensing walked away after the pipeline drained on claimless blocks,
//! while the honest seat that judged the last claims stayed frozen.
//!
//! The fix (`6bb8c844`, fix #2): past `palw_audit_2026_09_23` the counter ticks in
//! `license_claim` — a quorum's live signatures in the block that carries them — and not at
//! `Final`. This file drives the REAL fold through the audit's scenario and asserts the fixed
//! behaviour: the claimless stretch finalizes the pipeline and moves the counter by zero, the
//! colluder's `BondRetireRequested` is refused until `depth` further LICENCES have landed after its
//! liability began, and — the liveness escape, fix #3 — on the DAA clock alone once no licence has
//! landed for `2 × window_court`. Below the fence the counter still ticks at `Final`, byte for byte.

use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2,
    PalwPwuRuleV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras,
    palw_operator_id_v2, palw_second_clock_depth_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const PWU_PER_INFERENCE: u64 = 7_900;
const DEPTH: u64 = 30;
// Windows in t12's proportions (challenge 1,200 : court 3,000), scaled 1/10 to keep the run short.
const WINDOW_CHALLENGE: u64 = 120;
const WINDOW_COURT: u64 = 300;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn base() -> Hash64 {
    h(0xBA5E)
}

fn key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

const EXECUTOR: u64 = 0xB0;
const COLLUDER: u64 = 0xC0;
const HONEST: u64 = 0xD0;

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, WINDOW_CHALLENGE, WINDOW_COURT, 1_000, base(), 4, 1_000, 100, 1_000, 0)
        .expect("state params")
        .with_claim_retirement_daa(3_000)
        .expect("retirement")
}

/// The extras testnet-12 hands the fold past the fence (`fenced`), or a network that never armed
/// `palw_audit_2026_09_23` (the processor hands no depth there).
fn extras(fenced: bool) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        audit_2026_09_23_active: fenced,
        settled_anchor_depth: if fenced { Some(DEPTH) } else { None },
        objective_offence_daa: Some(0),
        ..Default::default()
    }
}

fn register(n: u64) -> Obj {
    Obj::BondRegistered {
        bond: key(n),
        pubkey: vec![n as u8; 32],
        operator_pubkey: vec![n as u8; 8],
        collateral: 1_000_000_000_000_000,
        payout_payload: h(0x9A4 + n),
        capable_classes: Default::default(),
        signature: vec![9u8; 64],
    }
}

fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(0xD0),
            challenge: h(nonce ^ 0x00C0_FFEE),
            class_id: base(),
            executor_bond: key(EXECUTOR).0,
            executor_pubkey: vec![EXECUTOR as u8; 32],
            operator_id: palw_operator_id_v2(&[EXECUTOR as u8; 8]),
            artifact_root: h(0xA7),
            trace_root: h(nonce ^ 0x7A),
            output_root: h(nonce ^ 0x00FF),
            pwu,
            trace_manifest_root: h(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: u64::MAX,
            execution_root: h(nonce ^ 0x4E),
        },
        signature: vec![9u8; 64],
    }
}

#[derive(Clone)]
struct Chain {
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    extras: PalwTransitionExtrasV1,
    daa: u64,
}

impl Chain {
    fn try_step(&mut self, with_attempt: bool, objects: &[Obj]) -> Result<Option<Hash64>, PalwStateV2Error> {
        let daa = self.daa + 1;
        let ctx = PalwBlockContextV2 { block: h(daa | 0x1000_0000), daa_score: daa, blue_score: daa, subsidy: 1_000_000 };
        let envelope = with_attempt.then(|| {
            let target = self.state.class_target(&base()).expect("target").target;
            attempt(kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, PWU_PER_INFERENCE), daa)
        });
        let id = envelope.as_ref().map(|e| attempt_id_v2(&e.attempt));
        let (next, _) = apply_palw_transition_v2_with_extras(
            &self.state,
            &self.params,
            &ctx,
            objects,
            envelope.as_ref(),
            false,
            false,
            false,
            false,
            &self.extras,
        )?;
        self.state = next;
        self.daa = daa;
        Ok(id)
    }

    /// Would the next block accept `bond`'s retirement? Folded on a copy; the chain does not move.
    fn retire_would_be(&self, bond: u64) -> Result<(), PalwStateV2Error> {
        let mut probe = self.clone();
        probe.try_step(false, &[Obj::BondRetireRequested { bond: key(bond), signature: vec![9u8; 64] }]).map(|_| ())
    }
}

fn bind(claim: Hash64, seat: u64) -> Obj {
    Obj::PanelBound { claim, anchor: h(77), seats: vec![PalwPanelSeatV2 { bond: key(seat), operator_id: h(seat) }] }
}

fn license(claim: Hash64, seat: u64) -> Obj {
    Obj::ReceiptLicensed {
        claim,
        receipts: vec![PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: key(seat),
            signed_daa: 0,
            signature: Vec::new(),
        }],
    }
}

fn is_final(state: &PalwChainStateV2, id: &Hash64) -> bool {
    matches!(state.claim(id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. }))
}

/// The audit's scenario up to the end of the claimless stretch: claim A judged by the COLLUDER,
/// `traffic − 1` more judged by the HONEST seat, all licensed; then claimless blocks until A's lock
/// has passed its DAA expiry by more than a challenge window. Returns the chain, A's id, every
/// claim id, and the counter when the stretch began.
fn colluder_scenario(fenced: bool) -> (Chain, Hash64, Vec<Hash64>, u64) {
    let mut chain = Chain { state: PalwChainStateV2::genesis(), params: params(), extras: extras(fenced), daa: 0 };
    let genesis = vec![
        register(EXECUTOR),
        register(COLLUDER),
        register(HONEST),
        Obj::ClassRegistered {
            class_id: base(),
            artifact_root: h(0xA7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: PWU_PER_INFERENCE },
            initial_target: u128::MAX / 2,
            share_permille: 1_000,
            activation_daa: 0,
            admission: None,
        },
    ];
    chain.try_step(false, &genesis).expect("genesis");

    // Normal traffic: one attempt per block, each bound one block later and licensed the next.
    // Claim 0 is A — judged by the COLLUDER. Every later claim is judged by the HONEST seat.
    // Traffic runs for `traffic` blocks, i.e. it keeps licensing while A sits in its challenge window.
    let traffic = 45u64;
    let mut ids: Vec<Hash64> = Vec::new();
    for i in 0..traffic as usize {
        let mut objects = Vec::new();
        if i >= 1 {
            objects.push(bind(ids[i - 1], if i - 1 == 0 { COLLUDER } else { HONEST }));
        }
        if i >= 2 {
            objects.push(license(ids[i - 2], if i - 2 == 0 { COLLUDER } else { HONEST }));
        }
        ids.push(chain.try_step(true, &objects).expect("traffic").expect("attempt id"));
    }
    let n = ids.len();
    chain.try_step(false, &[bind(ids[n - 1], HONEST), license(ids[n - 2], HONEST)]).expect("flush");
    chain.try_step(false, &[license(ids[n - 1], HONEST)]).expect("flush");
    let counter_at_stretch = chain.state.settled_attempt_finals();

    // ---- The claimless stretch: no attempt, no panel, no receipt — heartbeats. ----
    let a = ids[0];
    while !is_final(&chain.state, &a) {
        chain.try_step(false, &[]).expect("claimless");
    }
    let lock = chain.state.slashable_lock(key(COLLUDER), a).copied().expect("the colluder's Valid is locked");
    let target = lock.expiry_daa + WINDOW_CHALLENGE + 5;
    while chain.daa < target {
        chain.try_step(false, &[]).expect("claimless");
    }
    (chain, a, ids, counter_at_stretch)
}

/// **L4-P1, fixed: the pipeline no longer pays the second clock.** Every licensed claim reaches
/// `Final` on the claimless stretch and the counter does not move; the colluder's lock stays live
/// past its DAA expiry, and its retirement is refused.
#[test]
fn claimless_finals_do_not_tick_the_second_clock_and_the_colluder_stays_bound() {
    let (chain, a, ids, counter_at_stretch) = colluder_scenario(true);
    let counter = chain.state.settled_attempt_finals();
    let lock = chain.state.slashable_lock(key(COLLUDER), a).copied().expect("the colluder's liability");
    let finals = ids.iter().filter(|id| is_final(&chain.state, id)).count();
    let ring = chain.state.recent_anchor_daas().to_vec();
    let depth_now = palw_second_clock_depth_v1(Some(DEPTH), &ring, chain.daa, WINDOW_COURT);
    println!("=== L4-P1 (fixed): the claimless stretch settles nothing ===");
    println!("licences = {} (the counter at the stretch = {counter_at_stretch}); Finals folded = {finals}", ids.len());
    println!(
        "counter after the stretch = {counter}; A's lock settled_at_final {} expiry_daa {}",
        lock.settled_at_final, lock.expiry_daa
    );
    println!("DAA {}; last licence at {:?}; second clock depth now {depth_now:?}", chain.daa, ring.last());

    assert_eq!(counter_at_stretch, ids.len() as u64, "one tick per licence, none for anything else");
    assert_eq!(finals, ids.len(), "the whole pipeline reached Final on claimless blocks");
    assert_eq!(counter, counter_at_stretch, "Finals swept on the DAA clock do not tick the second clock past the fence");
    assert_eq!(lock.settled_at_final, counter, "the liability began at A's Final, and no anchor has settled since");
    assert!(chain.daa >= lock.expiry_daa, "the DAA half of the lock has run out");
    assert_eq!(depth_now, Some(DEPTH), "the stretch is shorter than 2 × window_court: the second clock binds");
    assert!(lock.is_live_v2(chain.daa, counter, depth_now), "A's lock is live on the second clock");

    let colluder = chain.retire_would_be(COLLUDER);
    println!("colluder retire now: {colluder:?}");
    assert!(
        matches!(colluder, Err(PalwStateV2Error::BondRetireWhileSlashableLocked { bond, .. }) if bond == key(COLLUDER)),
        "the colluder's retirement is refused on claimless history: {colluder:?}"
    );
}

/// **The colluder is released by `depth` further LICENCES — and not by one fewer.** Traffic
/// resumes, judged by the honest seat; after each licence the colluder's retirement is probed.
#[test]
fn the_colluder_is_released_by_depth_further_licences_and_not_one_fewer() {
    let (mut chain, a, _, _) = colluder_scenario(true);
    let lock = chain.state.slashable_lock(key(COLLUDER), a).copied().expect("the colluder's liability");
    let mut ids: Vec<Hash64> = Vec::new();
    let mut released_after = None;
    // Pipelined as before: attempt i, bind i − 1, license i − 2.
    for i in 0..(DEPTH as usize + 8) {
        let mut objects = Vec::new();
        if i >= 1 {
            objects.push(bind(ids[i - 1], HONEST));
        }
        if i >= 2 {
            objects.push(license(ids[i - 2], HONEST));
        }
        ids.push(chain.try_step(true, &objects).expect("traffic").expect("attempt id"));
        let since = chain.state.settled_attempt_finals() - lock.settled_at_final;
        let verdict = chain.retire_would_be(COLLUDER);
        if since < DEPTH {
            assert!(
                matches!(verdict, Err(PalwStateV2Error::BondRetireWhileSlashableLocked { .. })),
                "{since} licences since A's Final (< depth {DEPTH}): refused, got {verdict:?}"
            );
        } else {
            assert!(verdict.is_ok(), "{since} licences since A's Final (>= depth {DEPTH}): accepted, got {verdict:?}");
            released_after.get_or_insert(since);
        }
    }
    println!("colluder released after {released_after:?} further licences (depth {DEPTH})");
    assert_eq!(released_after, Some(DEPTH), "exactly `depth` further licences release it");
}

/// **The liveness escape (fix #3) on the fold's retire gate.** With no licence at all after the
/// stretch, the colluder's lock is waived at `E = last licence + 2 × window_court` and not before:
/// the second clock must bound a freeze, not make it eternal.
#[test]
fn with_no_licence_for_two_court_windows_the_colluder_is_released_by_the_daa_clock_alone() {
    let (mut chain, _, _, _) = colluder_scenario(true);
    let last = *chain.state.recent_anchor_daas().last().expect("the ring holds the last licence");
    let e = last + 2 * WINDOW_COURT;
    assert!(chain.daa + 1 < e - 1, "the stretch ended before the escape");
    while chain.daa + 1 < e - 1 {
        chain.try_step(false, &[]).expect("claimless");
    }
    // The next block is at E − 1.
    let at_e_minus_1 = chain.retire_would_be(COLLUDER);
    chain.try_step(false, &[]).expect("claimless");
    // The next block is at E.
    let at_e = chain.retire_would_be(COLLUDER);
    chain.try_step(false, &[]).expect("claimless");
    let at_e_plus_1 = chain.retire_would_be(COLLUDER);
    println!("last licence {last}; E = {e}; retire at E−1 {at_e_minus_1:?}, at E {at_e:?}, at E+1 {at_e_plus_1:?}");
    assert!(matches!(at_e_minus_1, Err(PalwStateV2Error::BondRetireWhileSlashableLocked { .. })), "E − 1: still bound");
    assert!(at_e.is_ok(), "E: the second clock is waived and the DAA clock alone releases the lock");
    assert!(at_e_plus_1.is_ok(), "E + 1: released");
}

/// **Below the fence the counter still ticks at `Final`** — the audit's measurement, kept as the
/// dormant pin: a network that never armed `palw_audit_2026_09_23` counts every swept `Final` (it
/// reads no depth, so nothing economic depends on it), and its seats are released on the DAA clock
/// alone, byte for byte as before `6bb8c844`.
#[test]
fn below_the_fence_the_pipeline_still_ticks_the_counter_at_final() {
    let (chain, a, ids, counter_at_stretch) = colluder_scenario(false);
    let counter = chain.state.settled_attempt_finals();
    assert_eq!(counter_at_stretch, 0, "below the fence a licence settles nothing");
    assert!(ids.iter().all(|id| is_final(&chain.state, id)));
    assert_eq!(counter, ids.len() as u64, "below the fence every Final ticks, claimless blocks included");
    assert!(chain.state.recent_anchor_daas().is_empty(), "and the ring is never written");
    let lock = chain.state.slashable_lock(key(COLLUDER), a).copied().expect("the colluder's liability");
    assert!(!lock.is_live_v2(chain.daa, counter, None), "the DAA-only rule");
    assert!(chain.retire_would_be(COLLUDER).is_ok(), "below the fence the DAA clock alone releases the colluder");
    assert!(chain.retire_would_be(HONEST).is_ok(), "and the honest seat");
}
