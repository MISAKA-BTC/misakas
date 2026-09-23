//! **The second clock's anchor ring and its liveness escape** (2026-09-24 DoS audit, fixes #2, #3
//! and #13a — commit `6bb8c844`), tested at the altitude each rule lives at.
//!
//! Past `palw_audit_2026_09_23` the second clock counts LICENCES — an attempt claim's quorum
//! receipts landing in a block — and each one's DAA joins `recent_anchor_daas`, a rooted, sorted
//! ring pruned on every push to "the `depth` newest entries older than the bind horizon, plus
//! everything inside it". Three pure functions read it:
//!
//! * `palw_anchor_ring_prune_count_v1` — how many of the oldest entries a push may drop;
//! * `palw_second_clock_depth_v1` — the depth the second clock demands, or `None` once no anchor
//!   has settled for `2 × window_court` (the liveness escape: the DAA clock alone decides again);
//! * `palw_settled_anchor_floor_daa_v1` — ADR-0065 D1's floor: the `depth`-th most recent anchor
//!   strictly before a DAA; `None` is the bootstrap waiver, `Some(0)` the conservative answer when
//!   the ring has pruned what the question needs.
//!
//! The ring is also a v21 layout change (hashed only when non-empty, two delta entries appended
//! last, carried in the `0xB3` tail), so the fold tests below revert every block and round-trip the
//! carriage, and a dormant network (fence `None`) is pinned to the root it had BEFORE the ring
//! existed.

use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_settled_anchor_floor_daa_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwDeltaEntryV2,
    PalwPanelSeatV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error,
    PalwTransitionExtrasV1, apply_delta_v2, apply_palw_transition_v2_with_extras, palw_anchor_ring_horizon_v1,
    palw_anchor_ring_prune_count_v1, palw_operator_id_v2, palw_second_clock_depth_v1, revert_delta_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

const PWU_PER_INFERENCE: u64 = 7_900;
const WINDOW_BIND: u64 = 10;
const WINDOW_RECEIPT: u64 = 10;
const WINDOW_CHALLENGE: u64 = 120;
const WINDOW_COURT: u64 = 300;
/// Small, so that a short history both fills and prunes the ring.
const DEPTH: u64 = 3;

const EXECUTOR: u64 = 0xB0;
const SEAT: u64 = 0xD0;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn base() -> Hash64 {
    h(0xBA5E)
}

fn key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
}

fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, WINDOW_BIND, WINDOW_RECEIPT, WINDOW_CHALLENGE, WINDOW_COURT, 1_000, base(), 4, 1_000, 100, 1_000, 0)
        .expect("state params")
        .with_claim_retirement_daa(3_000)
        .expect("retirement")
}

/// The fold's extras past the fence (`fenced = true`, as testnet-12 hands them) or on a network
/// that never armed it (`fenced = false`: the processor hands `settled_anchor_depth: None` there).
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

fn bind(claim: Hash64) -> Obj {
    Obj::PanelBound { claim, anchor: h(77), seats: vec![PalwPanelSeatV2 { bond: key(SEAT), operator_id: h(SEAT) }] }
}

fn license(claim: Hash64) -> Obj {
    Obj::ReceiptLicensed {
        claim,
        receipts: vec![PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: key(SEAT),
            signed_daa: 0,
            signature: Vec::new(),
        }],
    }
}

/// A chain of real folds that keeps every `(parent, delta, child)` it produced.
struct Chain {
    state: PalwChainStateV2,
    params: PalwStateParamsV2,
    extras: PalwTransitionExtrasV1,
    daa: u64,
    history: Vec<(PalwChainStateV2, PalwStateDeltaV2, PalwChainStateV2)>,
    /// The DAA of every licence this chain folded, in order — the brute-force reference the ring
    /// is checked against.
    licences: Vec<u64>,
}

impl Chain {
    fn new(fenced: bool) -> Self {
        let mut chain = Chain {
            state: PalwChainStateV2::genesis(),
            params: params(),
            extras: extras(fenced),
            daa: 0,
            history: Vec::new(),
            licences: Vec::new(),
        };
        let genesis = vec![
            register(EXECUTOR),
            register(SEAT),
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
        chain.step_at(1, false, &genesis).expect("genesis");
        chain
    }

    fn step_at(&mut self, daa: u64, with_attempt: bool, objects: &[Obj]) -> Result<Option<Hash64>, PalwStateV2Error> {
        assert!(daa > self.daa, "DAA moves forward");
        let ctx = PalwBlockContextV2 { block: h(daa | 0x1000_0000), daa_score: daa, blue_score: daa, subsidy: 1_000_000 };
        let envelope = with_attempt.then(|| {
            let target = self.state.class_target(&base()).expect("target").target;
            attempt(kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, PWU_PER_INFERENCE), daa)
        });
        let id = envelope.as_ref().map(|e| attempt_id_v2(&e.attempt));
        let (next, delta) = apply_palw_transition_v2_with_extras(
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
        self.history.push((self.state.clone(), delta, next.clone()));
        self.state = next;
        self.daa = daa;
        Ok(id)
    }

    /// One attempt claim walked to `ReceiptLicensed`: accepted at `at`, bound at `at + 1`, licensed
    /// at `at + 2`. Returns the claim id.
    fn license_one(&mut self, at: u64) -> Hash64 {
        let id = self.step_at(at, true, &[]).expect("attempt").expect("attempt id");
        self.step_at(at + 1, false, &[bind(id)]).expect("bind");
        self.step_at(at + 2, false, &[license(id)]).expect("licence");
        assert!(matches!(self.state.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::ReceiptLicensed { .. })), "licensed");
        self.licences.push(at + 2);
        id
    }

    fn ring_entries_in_history(&self) -> (usize, usize) {
        let mut pushed = 0;
        let mut pruned = 0;
        for (_, delta, _) in &self.history {
            for entry in &delta.entries {
                match entry {
                    PalwDeltaEntryV2::AnchorDaaPushed { .. } => pushed += 1,
                    PalwDeltaEntryV2::AnchorDaaPruned { .. } => pruned += 1,
                    _ => {}
                }
            }
        }
        (pushed, pruned)
    }
}

/// The reference the ring must reproduce: the `depth`-th most recent licence strictly before `x`
/// over the chain's WHOLE licence history, `None` when fewer than `depth` exist.
fn brute_floor(licences: &[u64], x: u64, depth: u64) -> Option<u64> {
    let before: Vec<u64> = licences.iter().copied().filter(|&d| d < x).collect();
    let depth = depth as usize;
    (before.len() >= depth && depth > 0).then(|| before[before.len() - depth])
}

/// A state carrying the given counter and ring — through the carriage, the only door a test has
/// onto a rooted field.
fn state_with(settled: u64, ring: &[u64]) -> PalwChainStateV2 {
    let mut carriage = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    carriage.settled_attempt_finals = settled;
    carriage.recent_anchor_daas = ring.to_vec();
    carriage.into_state(&params(), None).expect("a consistent state")
}

// =============================================================================================
// the pure functions
// =============================================================================================

/// **Pruning keeps exactly what a query from inside the horizon can ask for.** The cutoff is
/// `now − horizon`; an entry AT the cutoff is inside it; of the entries strictly older, the
/// `depth` newest stay.
#[test]
fn the_ring_prune_count_boundaries() {
    let ring = [10u64, 20, 30, 40, 50];
    // now 60, horizon 20: cutoff 40. Strictly older: 10, 20, 30 (three). 40 sits at the cutoff.
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 60, 20, 0), 3, "depth 0 keeps nothing older than the cutoff");
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 60, 20, 1), 2);
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 60, 20, 2), 1);
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 60, 20, 3), 0, "exactly `depth` older entries: nothing to drop");
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 60, 20, 4), 0, "fewer than `depth` older entries: nothing to drop");
    // The cutoff one DAA either side of an entry: 40 is dropped only once it is STRICTLY older.
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 60, 21, 0), 3, "cutoff 39: 40 is inside the horizon");
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 61, 20, 0), 4, "cutoff 41: 40 is older");
    // `now < horizon` saturates the cutoff to 0 — nothing is older than DAA 0.
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 15, 20, 0), 0);
    // A depth that does not fit a usize keeps everything, never wraps.
    assert_eq!(palw_anchor_ring_prune_count_v1(&ring, 1_000, 20, u64::MAX), 0);
    assert_eq!(palw_anchor_ring_prune_count_v1(&[], 1_000, 20, 0), 0, "an empty ring prunes nothing");
    // Duplicate DAAs (several licences in one block) count one each.
    assert_eq!(palw_anchor_ring_prune_count_v1(&[5, 5, 5, 50], 60, 20, 1), 2);
}

/// **The liveness escape, on its boundary.** `E = last anchor at or before now + 2 × window_court`:
/// the second clock still binds at `E − 1`, and is waived at `E` and after.
#[test]
fn the_second_clock_depth_boundaries() {
    let wc = WINDOW_COURT;
    let ring = [100u64];
    let e = 100 + 2 * wc;
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &ring, e - 1, wc), Some(DEPTH), "E − 1: the second clock binds");
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &ring, e, wc), None, "E: waived");
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &ring, e + 1, wc), None, "E + 1: waived");
    // Below the fence nothing moves: `None` in, `None` out, whatever the ring says.
    assert_eq!(palw_second_clock_depth_v1(None, &ring, 101, wc), None);
    // No anchor yet: "the last anchor" is DAA 0, so a young chain binds and an old silent one escapes.
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &[], 2 * wc - 1, wc), Some(DEPTH));
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &[], 2 * wc, wc), None);
    // An anchor LATER than `now` (a D1 read at a panel anchor older than the tip) is not the last
    // anchor AT `now`: the escape is measured from the newest entry at or before it.
    let later = [100u64, 10_000];
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &later, e - 1, wc), Some(DEPTH));
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &later, e, wc), None);
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &later, 10_000, wc), Some(DEPTH), "an anchor AT now counts");
    // A window_court whose double overflows saturates: the escape never fires.
    assert_eq!(palw_second_clock_depth_v1(Some(DEPTH), &ring, u64::MAX, u64::MAX / 2 + 1), Some(DEPTH));
}

/// **ADR-0065 D1's floor from the ring: exact, bootstrap `None`, pruned `Some(0)`.**
#[test]
fn the_settled_anchor_floor_reads_the_ring() {
    let ring = [10u64, 20, 30, 40, 50];
    // Every anchor this chain settled is in the ring: the answers are exact.
    let full = state_with(5, &ring);
    assert_eq!(palw_settled_anchor_floor_daa_v1(&full, 45, 3), Some(20), "the 3rd most recent before 45 is 20");
    assert_eq!(palw_settled_anchor_floor_daa_v1(&full, 41, 3), Some(20));
    assert_eq!(palw_settled_anchor_floor_daa_v1(&full, 40, 3), Some(10), "an anchor AT the query DAA is not before it");
    assert_eq!(palw_settled_anchor_floor_daa_v1(&full, u64::MAX, 3), Some(30), "the coinbase read: the whole chain");
    assert_eq!(palw_settled_anchor_floor_daa_v1(&full, u64::MAX, 5), Some(10));
    // Fewer than `depth` anchors before the query and nothing pruned: the bootstrap waiver.
    assert_eq!(palw_settled_anchor_floor_daa_v1(&full, 30, 3), None, "only 10 and 20 lie before 30, and that is all there ever was");
    assert_eq!(palw_settled_anchor_floor_daa_v1(&full, u64::MAX, 6), None);
    assert_eq!(palw_settled_anchor_floor_daa_v1(&full, 45, 0), None, "depth 0 arms nothing");
    // The same ring on a chain that settled MORE anchors than it holds: the entries the query
    // needs were pruned, and the answer is the conservative end — never the waiver.
    let pruned = state_with(7, &ring);
    assert_eq!(palw_settled_anchor_floor_daa_v1(&pruned, 30, 3), Some(0), "pruned: only genesis bonds are mature");
    assert_eq!(palw_settled_anchor_floor_daa_v1(&pruned, 5, 1), Some(0));
    assert_eq!(palw_settled_anchor_floor_daa_v1(&pruned, 45, 3), Some(20), "a question the ring can answer is still exact");
    // An empty ring on a network that counts but never pushes (below the fence): with anchors
    // settled the ring cannot answer and says so conservatively; with none, the waiver.
    assert_eq!(palw_settled_anchor_floor_daa_v1(&state_with(4, &[]), 45, 3), Some(0));
    assert_eq!(palw_settled_anchor_floor_daa_v1(&PalwChainStateV2::genesis(), 45, 3), None);
}

// =============================================================================================
// the fold
// =============================================================================================

/// **Past the fence: licences push, old entries prune, every block reverts exactly, and the ring
/// answers every query from inside the horizon as the whole history would.**
///
/// Licences land every 9 DAA against a 20-DAA horizon and `depth` 3, so after a few cycles every
/// push prunes. Finals are swept on the DAA clock along the way and must NOT tick the counter.
#[test]
fn the_ring_pushes_prunes_and_reverts_through_the_fold() {
    let mut chain = Chain::new(true);
    let horizon = palw_anchor_ring_horizon_v1(&chain.params);
    assert_eq!(horizon, WINDOW_BIND + WINDOW_RECEIPT);
    let mut ids = Vec::new();
    let mut at = 10u64;
    for _ in 0..24 {
        ids.push(chain.license_one(at));
        at += 9;
        // Every query a live panel can still be validated at: `x` from the horizon's edge to the
        // next block, against the brute force over the whole licence history.
        let now = chain.daa;
        for x in now.saturating_sub(horizon)..=now + 1 {
            assert_eq!(
                palw_settled_anchor_floor_daa_v1(&chain.state, x, DEPTH),
                brute_floor(&chain.licences, x, DEPTH),
                "floor at {x} (now {now}, ring {:?})",
                chain.state.recent_anchor_daas()
            );
        }
        // The ring is bounded: `depth` older entries plus the horizon's.
        let cutoff = now.saturating_sub(horizon);
        let older = chain.state.recent_anchor_daas().iter().filter(|&&d| d < cutoff).count();
        assert!(older as u64 <= DEPTH, "at most `depth` entries older than the cutoff: {:?}", chain.state.recent_anchor_daas());
        assert!(chain.state.recent_anchor_daas().windows(2).all(|w| w[0] <= w[1]), "sorted");
    }
    // Sweep every licensed claim to Final on claimless blocks.
    let settled_before_finals = chain.state.settled_attempt_finals();
    chain.step_at(at + WINDOW_CHALLENGE + 10, false, &[]).expect("claimless sweep");
    let finals =
        ids.iter().filter(|id| matches!(chain.state.claim(id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. }))).count();
    assert_eq!(finals, ids.len(), "every licensed claim reached Final on the DAA clock");
    assert_eq!(
        chain.state.settled_attempt_finals(),
        settled_before_finals,
        "a Final swept on a claimless block settles nothing past the fence"
    );
    assert_eq!(chain.state.settled_attempt_finals(), chain.licences.len() as u64, "the counter counts licences");

    let (pushed, pruned) = chain.ring_entries_in_history();
    println!("licences {}; ring now {:?}; pushed {pushed}, pruned {pruned}", chain.licences.len(), chain.state.recent_anchor_daas());
    assert_eq!(pushed, chain.licences.len(), "one push per licence");
    assert!(pruned > 0, "the history is long enough to prune");
    assert_eq!(pushed - pruned, chain.state.recent_anchor_daas().len());

    // Every block, pushes and prunes included, replays forward and reverts back exactly.
    for (i, (parent, delta, child)) in chain.history.iter().enumerate() {
        let forward = apply_delta_v2(parent, delta, &chain.params).unwrap_or_else(|e| panic!("apply {i}: {e:?}"));
        assert_eq!(&forward, child, "block {i} replays exactly");
        let back = revert_delta_v2(child, delta, &chain.params).unwrap_or_else(|e| panic!("revert {i}: {e:?}"));
        assert_eq!(&back, parent, "block {i} reverts exactly");
        assert_eq!(back.state_root(), parent.state_root(), "block {i}'s root");
    }
    // And the whole chain unwinds to genesis in one reorg.
    let mut s = chain.state.clone();
    for (parent, delta, _) in chain.history.iter().rev() {
        s = revert_delta_v2(&s, delta, &chain.params).expect("reorg");
        assert_eq!(s.state_root(), parent.state_root());
    }
    assert!(s.recent_anchor_daas().is_empty() && s.settled_attempt_finals() == 0);
}

/// **A block whose licence prunes reverts to a ring that is sorted and whole again** — the revert
/// guards refuse a delta applied to the wrong ring instead of corrupting it.
#[test]
fn a_ring_delta_applied_to_the_wrong_state_is_refused() {
    let mut chain = Chain::new(true);
    let mut at = 10u64;
    // Bounded: a ring that never prunes must fail this test, not grow the history until the host
    // kills it.
    for _ in 0..32 {
        chain.license_one(at);
        at += 9;
        if chain.ring_entries_in_history().1 > 0 {
            break;
        }
    }
    assert!(chain.ring_entries_in_history().1 > 0, "the ring pruned within 32 licences");
    let (parent, delta, child) = chain.history.last().cloned().expect("the pruning block");
    assert!(delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::AnchorDaaPruned { .. })));
    assert_eq!(revert_delta_v2(&child, &delta, &chain.params).expect("revert"), parent);
    // Applying the pruning block's delta to its own CHILD (the ring no longer starts with the
    // pruned entry, nor ends below the pushed one) is refused, never silently re-applied.
    assert!(apply_delta_v2(&child, &delta, &chain.params).is_err());
    // Reverting it from its PARENT (the pushed entry is not the ring's last) is refused too.
    assert!(revert_delta_v2(&parent, &delta, &chain.params).is_err());
}

/// **The carriage carries the ring** — `0xB3` is `[counter, ring]`, and a state with a non-empty
/// ring round-trips bit-for-bit under its own root.
#[test]
fn the_carriage_round_trips_a_non_empty_ring() {
    let mut chain = Chain::new(true);
    let mut at = 10u64;
    for _ in 0..8 {
        chain.license_one(at);
        at += 9;
    }
    let state = chain.state.clone();
    assert!(!state.recent_anchor_daas().is_empty() && chain.ring_entries_in_history().1 > 0, "a pruned, non-empty ring");
    let root = state.state_root();
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).expect("serialize");
    let decoded: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("deserialize");
    assert_eq!(decoded.recent_anchor_daas, state.recent_anchor_daas());
    assert_eq!(decoded.settled_attempt_finals, state.settled_attempt_finals());
    let back = decoded.into_state(&chain.params, Some(root)).expect("the root the chain committed to");
    assert_eq!(back, state);
    assert_eq!(back.state_root(), root);
    // A ring edited in transit is a different state: the committed root refuses it.
    let mut tampered: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("deserialize");
    *tampered.recent_anchor_daas.last_mut().unwrap() += 1;
    assert!(tampered.into_state(&chain.params, Some(root)).is_err(), "the ring is rooted");
}

/// **The dormant root pin.** The state root this dormant history produced BEFORE the ring existed:
/// computed on `faf80a4e` (the parent of both `6bb8c844` and the other side of the merge,
/// `b38356fe`) by folding this very scenario with `palw_state_v2.rs` and `palw_panel_v2.rs` checked
/// out at that commit, and identical on this branch — a network with the fence `None` did not move.
const DORMANT_GOLDEN_ROOT: &str =
    "1fba7da1177653f96258afdd12517021c128e499d15c0c98c49a8769ca3025c6b69fc39cc83e214c507b362a142f2b1baa316919aca1c454b53daaabce93a108";

/// **A network that never armed the fence: the counter still ticks at `Final`, the ring stays empty,
/// no ring delta is ever written, and the root is the one it had before the ring existed.**
#[test]
fn a_dormant_network_ticks_at_final_and_never_touches_the_ring() {
    let mut chain = Chain::new(false);
    let mut ids = Vec::new();
    let mut at = 10u64;
    for _ in 0..8 {
        ids.push(chain.license_one(at));
        at += 9;
    }
    assert_eq!(chain.state.settled_attempt_finals(), 0, "below the fence a licence settles nothing");
    chain.step_at(at + WINDOW_CHALLENGE + 10, false, &[]).expect("claimless sweep");
    let finals =
        ids.iter().filter(|id| matches!(chain.state.claim(id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. }))).count();
    assert_eq!(finals, ids.len());
    assert_eq!(chain.state.settled_attempt_finals(), ids.len() as u64, "below the fence the counter ticks at Final, as it always did");
    assert!(chain.state.recent_anchor_daas().is_empty(), "the ring is never written below the fence");
    assert_eq!(chain.ring_entries_in_history(), (0, 0), "no ring delta entry below the fence");
    let root = chain.state.state_root().to_string();
    println!("dormant root = {root}");
    assert_eq!(root, DORMANT_GOLDEN_ROOT, "a dormant network's root moved with the ring");
    // The carriage of a dormant state carries the counter and an empty ring, and round-trips.
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&chain.state)).expect("serialize");
    let back = borsh::from_slice::<PalwStateCarriageV2>(&bytes)
        .expect("deserialize")
        .into_state(&chain.params, Some(chain.state.state_root()));
    assert_eq!(back.expect("round trip"), chain.state);
}
