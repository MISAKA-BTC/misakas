//! **The order a collector offers licences in** — node policy, never validity (V04, the pre-t12
//! drill of 2026-09-25: honest claims whose seats all filed `Valid` sat `PanelBound` for 11–28 DAA).
//!
//! # What broke
//!
//! The collector walked its V2 receipt pool in claim-id order (`PalwReceiptPoolV1::claim_ids`, a
//! `sort_unstable` of the ids) and carried the FIRST claim whose licence assembled. One carrier is
//! in flight per panel (`MAX_INFLIGHT_CARRIERS`), so that is one licence a confirmed carrier, and
//! every collector reads the same tip and the same gossip — so every collector carried the SAME
//! claim, the lowest id with a standing quorum:
//!
//! * **The network licensed at one collector's rate, however many collectors it had.** Every
//!   carrier but one per round is a duplicate the fold drops (V06's "already credited", ~7 in 10
//!   licence carriers on the drill).
//! * **The queue was a priority queue keyed by the claim id.** Whenever binds outran that one
//!   collector's rate the highest ids waited — not for their turn, since every new bind with a lower
//!   id went ahead of them, but until binds slowed. On the drill `e824102e` (bound 53, all five
//!   seats `Valid` by 19:30) waited 28 DAA while `i0`, one of its OWN seats, carried 64 licences for
//!   lower ids and one other; the claims left behind were exactly the ids from `0xcf` up, and each
//!   licensed at its first submission (the doors were never the problem).
//! * **It does not end at the window.** Past `bound + window_receipt` the claim redraws with the same
//!   id — the same place in the queue — and the second `ReceiptTimeout` is `void_and_slash` of the
//!   honest producer (`audit_2026_09_23`, armed on testnet-12 from genesis): its escrow-inclusive
//!   reservation, 3,200.85 MSK on a floor claim. A claimant who grinds low claim ids jumps the queue
//!   and pushes everyone else's toward it.
//!
//! # What holds now
//!
//! 1. **The oldest bind first** ([`palw_licence_claim_order_v1`]). Every claim bound at or before a
//!    claim's own bind DAA is ahead of it and nothing bound later ever is, so the set ahead of a
//!    claim only shrinks once its bind DAA has passed: every claim whose quorum stands is carried
//!    after finitely many licences, whatever its id. A redraw is a new bind (its new panel's
//!    `bound_daa`, from which its new receipt deadline runs), so among claims of one receipt window
//!    this is also the deadline order; a heavy class's longer window only puts its deadline later.
//! 2. **Within one bind DAA, a process-keyed order**: each collector ranks the claims bound at the
//!    same DAA by a key only it knows (`RandomState`), so collectors spread over them instead of
//!    all carrying the same one, and no claimant can grind an id to the front of anyone's queue.
//! 3. **A claim the tip holds no bound panel for goes last** — nothing assembles for it (the doors
//!    take receipts for a `PanelBound` claim only), and it costs the loop a consensus ask.
//!
//! Nothing here reaches consensus: which licence a node offers first is its own business, and the
//! fold judges whatever arrives exactly as before (no licence becomes acceptable earlier than it was,
//! so no withdrawal-after-observation window moves).

use std::collections::HashSet;

use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_hashes::Hash64;

use crate::palw_receipt_pool::{PanelFactV1, ReceiptChainFactsV1};

/// **A licence waiting longer than this since its bind is worth a line** (V04): on the drill a claim
/// with a standing quorum licensed 0–5 DAA after its bind when the collectors kept up.
pub const PALW_LICENCE_BACKLOG_WARN_DAA: u64 = 10;

/// **The claims a collector offers licences for, in the order it offers them** (V04; module header).
///
/// `claims` is what the pool holds anything for ([`crate::palw_receipt_pool::PalwReceiptPoolV1::claim_ids`]);
/// `facts` is this tick's read of the tip; `spread` is this collector's own key (a process-keyed
/// hash in the service, so no two collectors — and no claimant — share it). The result is sorted
/// by `(bound DAA, spread, id)`, a claim with no bound panel at the tip counted as bound at
/// `u64::MAX`.
pub fn palw_licence_claim_order_v1(claims: Vec<Hash64>, facts: &ReceiptChainFactsV1, spread: impl Fn(&Hash64) -> u64) -> Vec<Hash64> {
    let mut keyed: Vec<(u64, u64, Hash64)> = claims
        .into_iter()
        .map(|claim| (facts.panel(&claim).map_or(u64::MAX, |panel| panel.bound_daa), spread(&claim), claim))
        .collect();
    keyed.sort_unstable();
    keyed.dedup_by_key(|(_, _, claim)| *claim);
    keyed.into_iter().map(|(_, _, claim)| claim).collect()
}

/// **The licence queue at a glance** (V04: "INFO does not show what a collector's pool holds"): the
/// claims in `ordered` the tip holds bound, and the oldest of them with its bind DAA. `ordered` is
/// [`palw_licence_claim_order_v1`]'s output, so the oldest is its first bound entry.
pub fn palw_licence_queue_head_v1(ordered: &[Hash64], facts: &ReceiptChainFactsV1) -> (usize, Option<(Hash64, u64)>) {
    let bound: Vec<(Hash64, u64)> =
        ordered.iter().filter_map(|claim| facts.panel(claim).map(|panel| (*claim, panel.bound_daa))).collect();
    (bound.len(), bound.first().copied())
}

/// **How many of `panel`'s seats this node holds a `Valid` receipt from**, of either version, and
/// how many seats it has — the V04 line's "every seat answered here and there is still no licence".
/// Pooled receipts, checked or not (the assembler checks them); `(0, 0)` with no bound panel.
pub fn palw_licence_valid_seats_v1(v2: &[PalwSeatReceiptV2], v3: &[PalwSeatReceiptV3], panel: Option<&PanelFactV1>) -> (usize, usize) {
    let Some(panel) = panel else { return (0, 0) };
    let valid: HashSet<PalwBondKeyV2> = v2
        .iter()
        .chain(v3.iter().map(|r| &r.receipt))
        .filter(|r| r.verdict == PalwReceiptVerdictV2::Valid && panel.seats.contains(&r.seat_bond))
        .map(|r| r.seat_bond)
        .collect();
    (valid.len(), panel.seats.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_receipt_pool::PalwReceiptPoolV1;
    use kaspa_consensus_core::palw_panel_v2::{PalwReceiptPanelFactV1, PalwReceiptPoolFactsV1};
    use kaspa_consensus_core::tx::TransactionOutpoint;
    use std::collections::{BTreeMap, HashMap};

    /// The drill's receipt window on testnet-12 (`window_receipt`): a claim still `PanelBound` at
    /// `bound + 600` redraws, and at the second such deadline its producer is slashed.
    const WINDOW_RECEIPT: u64 = 600;

    fn splitmix(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A claim id as the chain makes them: 64 bytes that look random.
    fn claim_id(state: &mut u64) -> Hash64 {
        let mut bytes = [0u8; 64];
        for chunk in bytes.chunks_exact_mut(8) {
            chunk.copy_from_slice(&splitmix(state).to_le_bytes());
        }
        Hash64::from_bytes(bytes)
    }

    /// A deterministic stand-in for the service's process-keyed hash (`RandomState`), one per collector.
    fn keyed(seed: u64) -> impl Fn(&Hash64) -> u64 {
        move |claim: &Hash64| {
            let mut state = seed;
            for word in claim.as_byte_slice().chunks_exact(8) {
                state ^= u64::from_le_bytes(word.try_into().unwrap());
                splitmix(&mut state);
            }
            splitmix(&mut state)
        }
    }

    fn facts_of(bound: &BTreeMap<Hash64, u64>) -> ReceiptChainFactsV1 {
        let mut facts = ReceiptChainFactsV1::default();
        facts.refresh(
            PalwReceiptPoolFactsV1 {
                panels: bound
                    .iter()
                    .map(|(claim, daa)| PalwReceiptPanelFactV1 {
                        claim_id: *claim,
                        bound_daa: *daa,
                        anchor: Hash64::default(),
                        seats: vec![],
                    })
                    .collect(),
                seat_keys: vec![],
            },
            &HashSet::new(),
        );
        facts
    }

    /// This node's own `Valid` receipt on `claim` — what puts the claim in the pool the collector walks.
    fn own_valid(claim: Hash64, seat: u64) -> PalwSeatReceiptV2 {
        PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(seat), 0)),
            signed_daa: 0,
            signature: vec![],
        }
    }

    #[test]
    fn the_oldest_bind_goes_first_whatever_its_id() {
        let mut rng = 7;
        let old_high = Hash64::from_bytes([0xFF; 64]);
        let young_low = Hash64::from_bytes([0x00; 64]);
        let mut bound = BTreeMap::new();
        bound.insert(old_high, 53);
        bound.insert(young_low, 60);
        let mut claims = vec![young_low, old_high];
        for _ in 0..20 {
            let c = claim_id(&mut rng);
            bound.insert(c, 55 + splitmix(&mut rng) % 4);
            claims.push(c);
        }
        let facts = facts_of(&bound);
        for seed in 0..32 {
            let order = palw_licence_claim_order_v1(claims.clone(), &facts, keyed(seed));
            assert_eq!(order.first(), Some(&old_high), "the claim bound first is offered first, id 0xff… or not");
            assert_eq!(order.last(), Some(&young_low), "the claim bound last is offered last, id 0x00… or not");
            let daas: Vec<u64> = order.iter().map(|c| bound[c]).collect();
            assert!(daas.windows(2).all(|w| w[0] <= w[1]), "bind order, every collector: {daas:?}");
        }
    }

    #[test]
    fn a_claim_the_tip_holds_no_panel_for_goes_last_and_nothing_is_lost() {
        let mut rng = 11;
        let unbound = Hash64::from_bytes([0x00; 64]);
        let mut bound = BTreeMap::new();
        let mut claims = vec![unbound];
        for daa in 0..6 {
            let c = claim_id(&mut rng);
            bound.insert(c, daa);
            claims.push(c);
        }
        claims.push(claims[1]);
        let order = palw_licence_claim_order_v1(claims.clone(), &facts_of(&bound), keyed(1));
        assert_eq!(order.last(), Some(&unbound));
        assert_eq!(order.len(), 7, "every claim once, a repeat folded");
        let (n, head) = palw_licence_queue_head_v1(&order, &facts_of(&bound));
        assert_eq!(n, 6, "the unbound claim is not in the queue");
        assert_eq!(head.map(|(_, daa)| daa), Some(0));
    }

    #[test]
    fn collectors_spread_over_one_bind_daa_instead_of_herding() {
        let mut rng = 3;
        let mut bound = BTreeMap::new();
        let claims: Vec<Hash64> = (0..4).map(|_| claim_id(&mut rng)).collect();
        for c in &claims {
            bound.insert(*c, 70);
        }
        let facts = facts_of(&bound);
        let firsts: HashSet<Hash64> = (0..8).map(|seed| palw_licence_claim_order_v1(claims.clone(), &facts, keyed(seed))[0]).collect();
        assert!(firsts.len() >= 2, "eight collectors do not all open on one claim of four: {}", firsts.len());
        let again = palw_licence_claim_order_v1(claims.clone(), &facts, keyed(5));
        assert_eq!(again, palw_licence_claim_order_v1(claims, &facts, keyed(5)), "one collector's order is stable tick to tick");
    }

    /// **The collector loop, as the service runs it** (`palw_panel.rs`, the Licences site): each
    /// round every collector walks its order, skips a claim it submitted within
    /// `COURT_MOVE_REPLAN_DAA`, carries the first claim still `PanelBound` (every claim here has all
    /// five seats `Valid`, so each one's licence assembles), and has one carrier in flight — one
    /// licence a round. A claim any collector carried is licensed by the round's end; the rest of the
    /// carriers for it are the fold's duplicates.
    struct Race {
        collectors: usize,
        binds_per_daa: usize,
        /// Confirmed carriers per collector per DAA with the Licences site free — the drill's `i0`
        /// carried about one licence a minute at ~130 s a DAA.
        rounds_per_daa: usize,
        daas: u64,
        /// A claim with the highest possible id, bound at this DAA.
        victim_bound: u64,
    }

    #[derive(Clone, Copy)]
    enum Order {
        /// 0e8ec984e: `let claims: Vec<Hash64> = receipt_pool_v2.claim_ids(); for claim in claims { … }`.
        Release,
        /// [`palw_licence_claim_order_v1`] on each collector's own key.
        OldestFirst,
    }

    struct Outcome {
        /// claim → (bound DAA, licensed DAA).
        licensed: HashMap<Hash64, (u64, u64)>,
        /// claim → bound DAA, never licensed.
        pending: BTreeMap<Hash64, u64>,
        victim: Hash64,
        carriers: usize,
        rounds_with_backlog: usize,
        licences_in_backlog_rounds: usize,
    }

    impl Outcome {
        fn victim_licensed(&self) -> Option<u64> {
            self.licensed.get(&self.victim).map(|(_, at)| *at)
        }

        fn max_wait(&self) -> u64 {
            self.licensed.values().map(|(bound, at)| at - bound).max().unwrap_or(0)
        }

        /// Claims whose receipt window closed with every seat `Valid` and no licence: each one redraws.
        fn timed_out(&self, now: u64) -> usize {
            self.pending.values().filter(|bound| **bound + WINDOW_RECEIPT < now).count()
        }
    }

    fn race(r: &Race, order: Order) -> Outcome {
        let replan = crate::palw_panel::COURT_MOVE_REPLAN_DAA;
        let mut rng = 0x5EED;
        let victim = Hash64::from_bytes([0xFF; 64]);
        // One pool stands for every collector's: gossip delivered every seat's receipt to all of them.
        let mut pool: PalwReceiptPoolV1<PalwSeatReceiptV2> = PalwReceiptPoolV1::new(Hash64::default());
        let mut pending: BTreeMap<Hash64, u64> = BTreeMap::new();
        let mut licensed: HashMap<Hash64, (u64, u64)> = HashMap::new();
        let mut submitted: Vec<HashMap<Hash64, u64>> = vec![HashMap::new(); r.collectors];
        let keys: Vec<_> = (0..r.collectors as u64).map(|j| keyed(0xC011_EC70 + j)).collect();
        let (mut carriers, mut rounds_with_backlog, mut licences_in_backlog_rounds) = (0, 0, 0);
        for daa in 0..r.daas {
            for _ in 0..r.binds_per_daa {
                let c = claim_id(&mut rng);
                pending.insert(c, daa);
                pool.insert_own(own_valid(c, 1), daa);
            }
            if daa == r.victim_bound {
                pending.insert(victim, daa);
                pool.insert_own(own_valid(victim, 1), daa);
            }
            for _ in 0..r.rounds_per_daa {
                let facts = facts_of(&pending);
                let mut picks: HashSet<Hash64> = HashSet::new();
                for (j, key) in keys.iter().enumerate() {
                    let claims = pool.claim_ids();
                    let walk = match order {
                        Order::Release => claims,
                        Order::OldestFirst => palw_licence_claim_order_v1(claims, &facts, key),
                    };
                    let pick = walk
                        .into_iter()
                        .find(|claim| pending.contains_key(claim) && !submitted[j].get(claim).is_some_and(|at| daa < at + replan));
                    if let Some(claim) = pick {
                        submitted[j].insert(claim, daa);
                        picks.insert(claim);
                        carriers += 1;
                    }
                }
                if pending.len() > r.collectors {
                    rounds_with_backlog += 1;
                    licences_in_backlog_rounds += picks.len();
                }
                for claim in picks {
                    let bound = pending.remove(&claim).expect("picked from the pending claims");
                    licensed.insert(claim, (bound, daa));
                }
                pool.retain_own(|claim, _| pending.contains_key(claim));
            }
        }
        Outcome { licensed, pending, victim, carriers, rounds_with_backlog, licences_in_backlog_rounds }
    }

    /// The drill's shape (DAA 40–69 of the pre-t12 chain): eight collectors, four binds a DAA, each
    /// collector's Licences site free about twice a DAA.
    fn the_drill() -> Race {
        Race { collectors: 8, binds_per_daa: 4, rounds_per_daa: 2, daas: 1 + WINDOW_RECEIPT + 2, victim_bound: 1 }
    }

    /// **The reproducer (V04 on 0e8ec984e).** A claim with all five seats `Valid`, bound at DAA 1,
    /// whose id happens to sort last, is never licensed: every collector carries the lowest id each
    /// round, the network licenses two claims a DAA against four bound, and a lower id is always
    /// waiting. At `bound + 600` it redraws — same id, same place in the queue — and the second
    /// timeout slashes its honest producer.
    #[test]
    fn v04_on_the_release_order_an_all_valid_claim_is_never_licensed_while_binds_outrun_one_collector() {
        let r = the_drill();
        let out = race(&r, Order::Release);
        assert_eq!(out.victim_licensed(), None, "0e8ec984e licenses the highest id only once binds slow");
        assert!(out.pending.contains_key(&out.victim));
        assert!(out.timed_out(r.daas) >= 1, "its receipt window closes with every seat Valid");
        let starved = out.pending.values().filter(|bound| **bound + 100 < r.daas).count();
        assert!(starved > 100, "and it is not alone: {starved} all-Valid claims wait past 100 DAA");
        assert_eq!(
            out.licences_in_backlog_rounds, out.rounds_with_backlog,
            "one licence a round with a backlog, whatever the collector count: the herd"
        );
        assert_eq!(
            out.carriers,
            r.rounds_per_daa * r.daas as usize * r.collectors,
            "every collector carried every round — seven of eight for nothing"
        );
    }

    /// **The fix, on the same load**: the victim is licensed within a DAA of its bind, no claim waits
    /// more than a few DAA, and the collectors license several claims a round.
    #[test]
    fn v04_oldest_first_licenses_every_all_valid_claim_within_a_few_daa_on_the_drill_load() {
        let r = the_drill();
        let out = race(&r, Order::OldestFirst);
        let at = out.victim_licensed().expect("the highest id is licensed");
        assert!(at <= r.victim_bound + 1, "licensed at DAA {at}, bound at {}", r.victim_bound);
        assert!(out.max_wait() <= 3, "no all-Valid claim waits past a few DAA: {}", out.max_wait());
        assert_eq!(out.timed_out(r.daas), 0);
        assert!(
            out.pending.values().all(|bound| *bound + 3 >= r.daas),
            "only the last DAAs' binds are still waiting: {:?}",
            out.pending.values().collect::<Vec<_>>()
        );
    }

    /// **Oldest first is starvation-free even where no spread can help** — one collector, binds
    /// above its rate: licences come out in bind order (FIFO), so the backlog ages evenly and the
    /// claim bound first is carried first, where the release order carries the lowest id first and
    /// leaves the oldest high id behind every later bind.
    #[test]
    fn v04_one_collector_licenses_in_bind_order_and_the_release_order_does_not() {
        let r = Race { collectors: 1, binds_per_daa: 3, rounds_per_daa: 2, daas: 60, victim_bound: 1 };
        let fifo = race(&r, Order::OldestFirst);
        let inversions = |out: &Outcome| {
            let mut by_bound: Vec<(u64, Option<u64>)> =
                out.licensed.values().map(|(b, at)| (*b, Some(*at))).chain(out.pending.values().map(|b| (*b, None))).collect();
            by_bound.sort_unstable();
            // A claim bound strictly later licensed strictly before an older one, or while it waits.
            let mut n = 0;
            for (i, (bi, ai)) in by_bound.iter().enumerate() {
                for (bj, aj) in &by_bound[i + 1..] {
                    if bj > bi && (ai.is_none() && aj.is_some() || matches!((ai, aj), (Some(x), Some(y)) if y < x)) {
                        n += 1;
                    }
                }
            }
            n
        };
        assert_eq!(inversions(&fifo), 0, "no later bind jumps an older one");
        assert!(fifo.victim_licensed().is_some(), "the highest id is carried in its bind's turn");
        let release = race(&r, Order::Release);
        assert!(inversions(&release) > 0);
        assert_eq!(release.victim_licensed(), None, "the release order leaves the oldest high id behind every later bind");
    }

    #[test]
    fn the_queue_line_counts_the_valid_seats_pooled_for_the_oldest_claim() {
        let claim = Hash64::from_u64_word(0xD4CD);
        let seat = |v: u64| PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(v), 0));
        let panel = PanelFactV1 { bound_daa: 53, anchor: Hash64::default(), seats: (1..=5).map(seat).collect() };
        let v2 = |v: u64, verdict| PalwSeatReceiptV2 { claim, verdict, seat_bond: seat(v), signed_daa: 54, signature: vec![] };
        let v3 = |v: u64| PalwSeatReceiptV3 {
            receipt: v2(v, PalwReceiptVerdictV2::Valid),
            segments: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2(1),
        };
        // Seats 1 and 2 by V2 and V3 both, 3 by V3, 4 Unavailable, 9 off the panel.
        let heard_v2 = [
            v2(1, PalwReceiptVerdictV2::Valid),
            v2(2, PalwReceiptVerdictV2::Valid),
            v2(4, PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 54 }),
            v2(9, PalwReceiptVerdictV2::Valid),
        ];
        let heard_v3 = [v3(1), v3(2), v3(3)];
        assert_eq!(palw_licence_valid_seats_v1(&heard_v2, &heard_v3, Some(&panel)), (3, 5));
        assert_eq!(palw_licence_valid_seats_v1(&heard_v2, &heard_v3, None), (0, 0));
    }

    fn production_panel() -> &'static str {
        let whole = include_str!("palw_panel.rs");
        &whole[..whole.find("#[cfg(test)]\nmod tests {").expect("the test module")]
    }

    /// **The service walks this order** (V04's first node-side cause, read off the production source
    /// the way the panel's other wiring tests read it).
    #[test]
    fn the_collector_walks_the_oldest_first_order() {
        let production = production_panel();
        let licences = production.find("slots.at(PalwCarrierSiteV1::Licences, inflight);").expect("the Licences site");
        let walk = &production[licences..];
        let walk = &walk[..walk.find("for claim in claims {").expect("the collector's walk")];
        assert!(
            walk.contains(
                "crate::palw_licence_order::palw_licence_claim_order_v1(receipt_pool_v2.claim_ids(), &receipt_facts, |claim| {"
            ),
            "the licences are offered oldest bind first, not in id order"
        );
        assert!(!walk.contains("let claims: Vec<Hash64> = receipt_pool_v2.claim_ids();"), "the id-order walk is gone");
        assert!(walk.contains("std::hash::BuildHasher::hash_one(&licence_spread, claim)"), "on this process's own key");
        assert!(production.contains("let licence_spread = std::collections::hash_map::RandomState::new();"));
    }

    /// **A possession proof's wait holds the receipts for its own tick, never longer** (V04's second
    /// node-side cause: m6 carried no licence for 25 minutes after one proof waited once).
    #[test]
    fn the_proof_wait_is_not_a_latch() {
        let production = production_panel();
        let licences = production.find("slots.at(PalwCarrierSiteV1::Licences, inflight);").expect("the Licences site");
        let reset =
            production.find("let proof_waited_last_tick = std::mem::replace(&mut readiness_waiting, false);").expect("the reset");
        let first_read =
            production.find("self.readiness_duties_for_tick(&session, current_daa, proof_waited_last_tick)").expect("M1's read");
        assert!(reset < first_read && first_read < licences, "the wait is reset each tick, before any proof is read");
        assert_eq!(
            production.matches("readiness_duties_for_tick(&session, current_daa, proof_waited_last_tick)").count(),
            2,
            "both reads — M1's and the Own site's — are cued by last tick's wait"
        );
        assert!(
            !production.contains("readiness_duties_for_tick(&session, current_daa, readiness_waiting)"),
            "no read is cued by the latch"
        );
        // Between the reset and the Licences site, the flag is set only where a proof of THIS tick waits.
        let tick = &production[reset..licences];
        assert!(tick.matches("readiness_waiting = true;").count() == 2, "a failed replacement, and a proof with no slot");
    }
}
