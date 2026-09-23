//! Adversarial review (economic / DoS re-exploit lens) of fix #3, the liveness escape.
//!
//! The escape (`palw_second_clock_depth_v1`) waives the second clock only when NO licence has
//! landed for `2 × window_court`. It is measured from the LAST licence on the chain, not from the
//! start of any one lock or retirement. So a trickle of one licence every `2 × window_court − 1`
//! DAA keeps the escape from ever firing, while the counter advances by one per licence: a
//! retirement (which needs `depth` licences since it was requested, `palw_bond_collateral_is_locked_v3`)
//! stays frozen for `depth × (2 × window_court − 1)` DAA — not `2 × window_court`.
//!
//! Harness copied from `dos_l4_pipeline_anchors.rs` (windows in t12's proportions, scaled 1/10;
//! depth 30 as t12).
//!
//! **Fixed (2026-09-24 review):** the second clock is bounded per obligation
//! (`palw_second_clock_holds_v1`): past its DAA clock it may hold a lock or a retirement for at most
//! `2 × window_court` more, whatever the licence count — the processor's gate is
//! `palw_bond_collateral_is_locked_v5`. The v4 measurements are kept as PRE-FENCE DEFECT RECORDs
//! (v4 is the gate the fix replaced; below the fence both are the DAA-only rule).

use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2, PalwPwuRuleV2,
    PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras,
    palw_bond_collateral_is_locked_v4, palw_bond_collateral_is_locked_v5, palw_operator_id_v2, palw_second_clock_depth_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const PWU_PER_INFERENCE: u64 = 7_900;
const DEPTH: u64 = 30;
const WINDOW_CHALLENGE: u64 = 120;
const WINDOW_COURT: u64 = 300;
const WITHDRAWAL_DELAY: u64 = 750; // t12's 7,500, scaled 1/10

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
const ATTACKER_SEAT: u64 = 0xC0;
const RETIREE: u64 = 0xD0;

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, WINDOW_CHALLENGE, WINDOW_COURT, 1_000, base(), 4, 1_000, 100, 1_000, 0)
        .expect("state params")
        .with_claim_retirement_daa(3_000)
        .expect("retirement")
}

fn extras() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        audit_2026_09_23_active: true,
        settled_anchor_depth: Some(DEPTH),
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

    /// Jump the DAA forward with one claimless block (the fold only reads `daa_score`).
    fn idle_until(&mut self, daa: u64) {
        if self.daa + 1 < daa {
            self.daa = daa - 1;
        }
        self.try_step(false, &[]).expect("claimless");
    }

    /// One licence by the attacker's own seat: attempt, bind, licence in three blocks.
    fn licence(&mut self) -> u64 {
        let id = self.try_step(true, &[]).expect("attempt").expect("id");
        self.try_step(false, &[bind(id)]).expect("bind");
        self.try_step(false, &[license(id)]).expect("licence");
        self.daa
    }

    /// The withdrawal gate exactly as `processor.rs` evaluates it (`palw_v2_locked_bond_outpoints`):
    /// v5 on the escaped depth, the duty gate on, the second clock bounded by `window_court`.
    fn withdrawal_locked(&self, bond: u64) -> bool {
        let depth = palw_second_clock_depth_v1(Some(DEPTH), self.state.recent_anchor_daas(), self.daa, WINDOW_COURT);
        let record = self.state.bond(&key(bond)).expect("bond");
        palw_bond_collateral_is_locked_v5(&self.state, &key(bond), record, self.daa, WITHDRAWAL_DELAY, depth, true, WINDOW_COURT)
    }

    /// The gate before the fix: v4, the second clock unbounded per obligation.
    fn withdrawal_locked_v4(&self, bond: u64) -> bool {
        let depth = palw_second_clock_depth_v1(Some(DEPTH), self.state.recent_anchor_daas(), self.daa, WINDOW_COURT);
        let record = self.state.bond(&key(bond)).expect("bond");
        palw_bond_collateral_is_locked_v4(&self.state, &key(bond), record, self.daa, WITHDRAWAL_DELAY, depth, true)
    }
}

fn bind(claim: Hash64) -> Obj {
    Obj::PanelBound { claim, anchor: h(77), seats: vec![PalwPanelSeatV2 { bond: key(ATTACKER_SEAT), operator_id: h(ATTACKER_SEAT) }] }
}

fn license(claim: Hash64) -> Obj {
    Obj::ReceiptLicensed {
        claim,
        receipts: vec![PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: key(ATTACKER_SEAT),
            signed_daa: 0,
            signature: Vec::new(),
        }],
    }
}

fn genesis() -> Chain {
    let mut chain = Chain { state: PalwChainStateV2::genesis(), params: params(), extras: extras(), daa: 0 };
    chain
        .try_step(
            false,
            &[
                register(EXECUTOR),
                register(ATTACKER_SEAT),
                register(RETIREE),
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
            ],
        )
        .expect("genesis");
    chain
}

/// Returns the DAA at which the retiree's withdrawal gate first opens, probing after every block.
fn retire_and_watch(trickle: bool, horizon: u64) -> (u64, Option<u64>, u64) {
    retire_and_watch_with(trickle, horizon, Chain::withdrawal_locked)
}

fn retire_and_watch_with(trickle: bool, horizon: u64, locked: fn(&Chain, u64) -> bool) -> (u64, Option<u64>, u64) {
    let mut chain = genesis();
    // Some ordinary history first, so the ring is not empty.
    chain.licence();
    // The retiree holds no lock and no duty: it never judged anything.
    chain.try_step(false, &[Obj::BondRetireRequested { bond: key(RETIREE), signature: vec![9u8; 64] }]).expect("retire");
    let retired_at = chain.daa;
    let gap = 2 * WINDOW_COURT - 1;
    let mut next_licence = chain.daa + gap;
    let mut opened = None;
    let mut licences = 0u64;
    while chain.daa < retired_at + horizon {
        if trickle && chain.daa + 3 >= next_licence {
            chain.licence();
            licences += 1;
            next_licence = chain.daa + gap;
        } else {
            let to = (chain.daa + 50).min(if trickle { next_licence.saturating_sub(3) } else { u64::MAX }).max(chain.daa + 1);
            chain.idle_until(to);
        }
        if !locked(&chain, RETIREE) {
            opened = Some(chain.daa);
            break;
        }
    }
    (retired_at, opened, licences)
}

/// **Fixed: a trickle of licences no longer multiplies the freeze.** One licence every
/// `2 × window_court − 1` DAA still keeps the chain-wide escape from firing, but the retirement's
/// second clock now holds for at most `2 × window_court` past its withdrawal delay: the retiree
/// withdraws by `delay + 2 × window_court` whatever the licence count.
///
/// Fails without the fix (v4): the retiree waits ~`depth × 2 × window_court` (17,970 DAA here).
#[test]
fn review_economic_a_trickle_of_licences_freezes_a_retirement_for_at_most_two_court_windows() {
    let horizon = DEPTH * 2 * WINDOW_COURT + 2_000;
    let (quiet_retired, quiet_open, _) = retire_and_watch(false, horizon);
    let (trickle_retired, trickle_open, licences) = retire_and_watch(true, horizon);
    let quiet_wait = quiet_open.expect("quiet chain opens") - quiet_retired;
    let trickle_wait = trickle_open.expect("trickle chain opens within the horizon") - trickle_retired;
    println!("quiet lane: retiree withdraws {quiet_wait} DAA after retiring; trickle: {trickle_wait} DAA after {licences} licences");
    assert!(quiet_wait <= (2 * WINDOW_COURT).max(WITHDRAWAL_DELAY) + 5, "quiet wait {quiet_wait}");
    // `retire_and_watch` probes after every step, and a step is up to 50 idle DAA (or a 3-block
    // licence), so the opening is observed up to 53 DAA after it happens.
    assert!(trickle_wait <= WITHDRAWAL_DELAY + 2 * WINDOW_COURT + 53, "the bound holds under the trickle: {trickle_wait}");
    assert!(trickle_wait >= WITHDRAWAL_DELAY + 2 * WINDOW_COURT, "and the second clock still bites inside it: {trickle_wait}");
    assert!(licences < DEPTH, "the trickle no longer buys a freeze of `depth` licences ({licences})");
}

/// **Fixed: a re-freeze after the escape is bounded by the same two windows.** One later licence
/// can still re-arm the chain's second clock while the retirement is inside its bound, but past
/// `delay + 2 × window_court` nothing re-freezes it.
#[test]
fn review_economic_a_licence_after_the_bound_does_not_re_freeze_a_released_withdrawal() {
    let mut chain = genesis();
    chain.licence();
    chain.try_step(false, &[Obj::BondRetireRequested { bond: key(RETIREE), signature: vec![9u8; 64] }]).expect("retire");
    let retired_at = chain.daa;
    chain.idle_until(retired_at + WITHDRAWAL_DELAY + 2 * WINDOW_COURT + 1);
    assert!(!chain.withdrawal_locked(RETIREE), "released at {}", chain.daa);
    chain.licence();
    assert!(!chain.withdrawal_locked(RETIREE), "one licence past the bound does not re-freeze the withdrawal");
    assert!(chain.withdrawal_locked_v4(RETIREE), "PRE-FENCE DEFECT RECORD: v4 re-freezes it");
}

#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: the v4 withdrawal gate (second clock unbounded per obligation) freezes a retirement for depth × 2 × window_court under a trickle of licences (review_economic_trickle_freeze); v5 bounds it"]
fn review_economic_one_licence_per_two_court_windows_freezes_every_retirement_for_depth_windows() {
    let horizon = DEPTH * 2 * WINDOW_COURT + 2_000;
    let (quiet_retired, quiet_open, _) = retire_and_watch_with(false, horizon, Chain::withdrawal_locked_v4);
    let (trickle_retired, trickle_open, licences) = retire_and_watch_with(true, horizon, Chain::withdrawal_locked_v4);
    let quiet_wait = quiet_open.expect("quiet chain opens") - quiet_retired;
    let trickle_wait = trickle_open.expect("trickle chain opens within the horizon") - trickle_retired;
    println!("window_court {WINDOW_COURT}, depth {DEPTH}, withdrawal delay {WITHDRAWAL_DELAY} (t12 scaled 1/10)");
    println!("quiet lane: retiree withdraws {quiet_wait} DAA after retiring (escape fires)");
    println!("one licence every {} DAA: retiree withdraws {trickle_wait} DAA after retiring, after {licences} licences", 2 * WINDOW_COURT - 1);
    assert!(quiet_wait <= (2 * WINDOW_COURT).max(WITHDRAWAL_DELAY) + 5, "quiet wait {quiet_wait}");
    assert!(licences >= DEPTH - 1, "the trickle needed {licences} licences");
    assert!(trickle_wait >= (DEPTH - 1) * (2 * WINDOW_COURT - 1), "trickle wait {trickle_wait}");
    assert!(trickle_wait > 20 * quiet_wait, "the trickle multiplies the freeze: {trickle_wait} vs {quiet_wait}");
}

/// The v4 re-freeze just after the escape released the withdrawal.
#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: under the v4 gate one licence after the escape re-freezes an already-open withdrawal (review_economic_trickle_freeze); v5 bounds the re-freeze to 2 × window_court past the delay"]
fn review_economic_one_licence_after_an_escape_re_freezes_a_released_withdrawal() {
    let mut chain = genesis();
    let last = chain.licence();
    chain.try_step(false, &[Obj::BondRetireRequested { bond: key(RETIREE), signature: vec![9u8; 64] }]).expect("retire");
    let retired_at = chain.daa;
    let open_at = (last + 2 * WINDOW_COURT).max(retired_at + WITHDRAWAL_DELAY) + 1;
    chain.idle_until(open_at);
    assert!(!chain.withdrawal_locked_v4(RETIREE), "the escape released the retiree at {}", chain.daa);
    chain.licence();
    assert!(chain.withdrawal_locked_v4(RETIREE), "one licence re-arms the second clock and re-freezes an already-open withdrawal");
}
