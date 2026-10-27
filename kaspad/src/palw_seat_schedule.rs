//! **Which claim a seat replays next** — node policy, never validity (the 2026-10-01 panel backlog,
//! `docs/design/palw/verification/t12-panel-backlog-1001.md`, F2).
//!
//! # What it fixes
//!
//! A claim is licensed by a quorum of its panel — 3 of 5 seats. Every seat of a panel replayed every
//! claim, oldest receipt deadline first, whatever the other four had already done: so all five seats
//! walked the same list in the same order, the fast seats put two or three `Valid` receipts in the
//! pool within seconds, and the slow seats — on testnet-12 the five seats on one 8-core host, whose
//! replays took 7–50 minutes against ~5 s elsewhere — then spent their slots on the claims that were
//! already licensed or were one receipt short of it from somebody else. Because a drawn panel
//! needs at least one slow seat to reach its quorum (the fast seats are at most two of five on every
//! panel), the slowest seats gated the licence, and their slots were the scarcest resource on the
//! network; by panel composition about 60 % of what they replayed was not needed.
//!
//! # The rule
//!
//! For each due duty the seat reads, from its own receipt pool (checked signatures only — a receipt
//! nobody has verified moves nothing), how many OTHER seats of the claim's panel already have a
//! `Valid` receipt, and sorts its duties by tier:
//!
//! * **tier 0 — needed**: the claim is short of its quorum, and this seat is one of the
//!   `still_needed + SPARE` first seats, in a ranking every seat computes the same way (a hash of the
//!   claim and the seat's bond over the panel's seats that have not answered). Ordered closest to
//!   quorum first (a claim one receipt short is the cheapest licence there is), then by receipt
//!   deadline. The `SPARE` seat means one dead or slow primary does not stall a claim.
//! * **tier 1 — backup**: short of quorum, but this seat is ranked past the primaries.
//! * **tier 2 — satisfied**: the quorum is already pooled; the claim needs a carrier, not a replay.
//!
//! **Nothing is skipped, and nothing waits forever.** A lower tier is *later*, not *never*: when the
//! seat has a free slot and nothing in a higher tier, it works the lower tier (a seat that is idle
//! loses nothing by duplicating). Two guards keep the order from starving a claim:
//!
//! * **the oldest [`PALW_SEAT_OLDEST_GUARD_V1`] due claims that are not already at quorum go first**,
//!   whatever their tier — by rank, not by age, so it holds under a standing backlog (where every claim is
//!   old and an age threshold would promote them all and switch the scheduler off): the queue is FIFO at
//!   the head and closest-to-quorum behind it, and a claim whose primaries are dead reaches the head when
//!   the claims before it are done;
//! * **a very old claim promotes itself** — a backup after [`PALW_SEAT_BACKUP_AFTER_DAA_V1`] DAA since the
//!   bind, a satisfied claim after [`PALW_SEAT_SATISFIED_AFTER_DAA_V1`] — against a pathology the rank
//!   guard does not see (pooled receipts that never become a licence).
//!
//! Pool contents are a hint a peer can only make worse by withholding (nobody can forge a checked
//! receipt), and the worst a bad hint does is the old behaviour, later.
//!
//! A duty with no panel read yet (the tip has not seen the bind) keeps tier 0 and the old order.

use std::collections::HashSet;

use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_hashes::Hash64;

/// Seats beyond the quorum's need that a claim's primaries include: one spare, so a single dead or
/// slow primary does not hold a claim until it ages.
pub const PALW_SEAT_PRIMARY_SPARE_V1: usize = 1;

/// DAA after the bind at which a claim a seat is only a backup for becomes its primary work. 120 DAA
/// is about five hours on testnet-12 (a DAA every ~2.6 minutes): 20× the longest healthy wait, a fifth
/// of the 600-DAA receipt window — a backstop, not the mechanism (the oldest-first guard is).
pub const PALW_SEAT_BACKUP_AFTER_DAA_V1: u64 = 120;

/// DAA after the bind at which a claim whose quorum is already pooled is replayed anyway — the pooled
/// receipts have not become a licence in this long, so more receipts cost nothing.
pub const PALW_SEAT_SATISFIED_AFTER_DAA_V1: u64 = 72;

/// How many of the oldest due, not-yet-satisfied claims are answered first whatever their tier.
pub const PALW_SEAT_OLDEST_GUARD_V1: usize = 2;

/// A class is slow on this seat when its last replay here took this many DAA (~21 minutes at a DAA
/// every ~2.6 minutes). The 5.104 seats' 8k replays took 14-55 minutes (5 on an idle host) and held
/// the share's 3.37 of 3.5 GiB the whole time, which is what collapsed their floor receipts at 05:00.
pub const PALW_SEAT_BIG_SLOW_DAA_V1: u64 = 8;

/// A slow class's claim is urgent — no longer deferred behind lighter work — within this many DAA of its
/// receipt deadline (a fifth of the 600-DAA window, ~5 hours: five times the longest measured replay).
pub const PALW_SEAT_BIG_URGENT_DAA_V1: u64 = 120;

/// A replay needing at least this many bytes is "big": [`crate::palw_panel`] gates its START (never a
/// running one) while lighter work waits.
pub const PALW_SEAT_BIG_REPLAY_BYTES_V1: u64 = 2 << 30;

/// What the scheduler reads of one due duty.
#[derive(Clone, Debug)]
pub struct PalwSeatScheduleInV1 {
    pub claim_id: Hash64,
    /// The receipt deadline the duty loop reads for the claim (the chain's per-claim one past SEAT-R).
    pub deadline: u64,
    pub bound_daa: u64,
    /// This seat.
    pub me: PalwBondKeyV2,
    /// The claim's bound panel, as the tip's read reported it — `None` before the tip has seen the
    /// bind.
    pub panel: Option<Vec<PalwBondKeyV2>>,
    /// Seats of the panel with a checked `Valid` receipt pooled (this seat's own are not in a due
    /// duty's pool: a duty it has answered is not due).
    pub valid: HashSet<PalwBondKeyV2>,
    /// Whether the duty is due now (this seat has not answered it and its deadline has not passed): a
    /// duty that is not due is listed last and never counts toward the oldest-first guard.
    pub due: bool,
    /// A claim of a class this seat replays slowly (a replay of the class has taken it
    /// [`PALW_SEAT_BIG_SLOW_DAA_V1`] DAA or more) and far from its deadline ([`PALW_SEAT_BIG_URGENT_DAA_V1`]):
    /// its replay would hold the seat's whole memory share for most of an hour, and every lighter duty
    /// behind it waits on the ledger. Such a claim goes after the lighter ones, and takes no place in the
    /// oldest-first guard, until it is urgent.
    pub big: bool,
}

/// The tier of one duty (lower is sooner), with the sort key inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PalwSeatTierV1 {
    Needed,
    Backup,
    Satisfied,
}

/// The ranking every seat computes alike: FNV-1a over the claim's bytes, the seat bond's transaction
/// id and its index, finished with murmur3's fmix64. Deterministic across nodes and processes (the pool's own hasher is process-keyed
/// and would give each seat a different order).
pub fn palw_seat_rank_key_v1(claim: &Hash64, seat: &PalwBondKeyV2) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    eat(&claim.as_bytes());
    eat(&seat.0.transaction_id.as_bytes());
    eat(&seat.0.index.to_le_bytes());
    // murmur3's fmix64: FNV alone mixes the last bytes poorly, and two seats' keys differ only in theirs.
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^ (hash >> 33)
}

/// **The tier of `item` at `now_daa` under a quorum of `quorum` seats**, and how many receipts it is
/// still short (`quorum − valid others`).
pub fn palw_seat_tier_v1(item: &PalwSeatScheduleInV1, quorum: usize, now_daa: u64) -> (PalwSeatTierV1, usize) {
    let Some(panel) = &item.panel else { return (PalwSeatTierV1::Needed, quorum) };
    let others: HashSet<&PalwBondKeyV2> = item.valid.iter().filter(|seat| **seat != item.me && panel.contains(seat)).collect();
    let short = quorum.saturating_sub(others.len());
    let waited = now_daa.saturating_sub(item.bound_daa);
    if short == 0 {
        return if waited >= PALW_SEAT_SATISFIED_AFTER_DAA_V1 { (PalwSeatTierV1::Needed, 1) } else { (PalwSeatTierV1::Satisfied, 0) };
    }
    // The seats that have not answered, ranked; this seat is among them (its duty is due).
    let mut pending: Vec<(u64, &PalwBondKeyV2)> =
        panel.iter().filter(|seat| !others.contains(seat)).map(|seat| (palw_seat_rank_key_v1(&item.claim_id, seat), seat)).collect();
    pending.sort();
    let rank = pending.iter().position(|(_, seat)| **seat == item.me);
    let primary = rank.is_some_and(|rank| rank < short + PALW_SEAT_PRIMARY_SPARE_V1);
    if primary || waited >= PALW_SEAT_BACKUP_AFTER_DAA_V1 { (PalwSeatTierV1::Needed, short) } else { (PalwSeatTierV1::Backup, short) }
}

/// **Is there lighter work waiting?** A due duty, short of its quorum, that is not big: while there is,
/// the seat does not START a big replay ([`crate::palw_panel::PalwSeatReplaysV1::set_big_gate`]).
pub fn palw_seat_light_pending_v1(items: &[PalwSeatScheduleInV1], quorum: usize, now_daa: u64) -> bool {
    items.iter().any(|item| item.due && !item.big && palw_seat_tier_v1(item, quorum, now_daa).0 != PalwSeatTierV1::Satisfied)
}

/// **The order a seat answers its duties in**: indices into `items`, soonest first — the oldest
/// [`PALW_SEAT_OLDEST_GUARD_V1`] due claims short of their quorum, then by tier, then (in the needed
/// tier) the claims closest to quorum, then the receipt deadline, then the claim id; duties that are
/// not due last.
#[allow(dead_code)] // F2's per-seat order, superseded by `palw_seat_shared_order_v1` (O-LIVE-1); kept as the simulation's baseline.
pub fn palw_seat_schedule_order_v1(items: &[PalwSeatScheduleInV1], quorum: usize, now_daa: u64) -> Vec<usize> {
    let tiers: Vec<(PalwSeatTierV1, usize)> = items.iter().map(|item| palw_seat_tier_v1(item, quorum, now_daa)).collect();
    // The guard: the oldest due claims that are not already at quorum.
    let mut candidates: Vec<usize> = (0..items.len()).filter(|i| items[*i].due && tiers[*i].0 != PalwSeatTierV1::Satisfied).collect();
    candidates.sort_by_key(|i| (items[*i].deadline, items[*i].claim_id));
    // A big (slow-class, not yet urgent) claim waits behind the lighter ones and takes no guard place.
    candidates.retain(|i| !items[*i].big);
    let guarded: HashSet<usize> = candidates.into_iter().take(PALW_SEAT_OLDEST_GUARD_V1).collect();
    let mut keyed: Vec<(u8, PalwSeatTierV1, usize, u64, Hash64, usize)> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let (tier, short) = tiers[index];
            // Closest to quorum first inside the needed tier only; the other tiers keep the deadline order.
            let closeness = if tier == PalwSeatTierV1::Needed { short } else { 0 };
            let band = if !item.due {
                3
            } else if guarded.contains(&index) {
                0
            } else if item.big && tier != PalwSeatTierV1::Satisfied {
                2
            } else {
                1
            };
            (band, tier, closeness, item.deadline, item.claim_id, index)
        })
        .collect();
    keyed.sort();
    keyed.into_iter().map(|(.., index)| index).collect()
}

/// A due claim within this many DAA of its receipt deadline is answered before the shared FIFO, soonest
/// deadline first (O-LIVE-1 keeps the deadline guarantee the F2 guard gave): a fifth of the 600-DAA
/// window, the same horizon a slow class's claim stops being deferred at.
pub const PALW_SEAT_SHARED_URGENT_DAA_V1: u64 = PALW_SEAT_BIG_URGENT_DAA_V1;

/// A claim this seat is one of the last this many panel seats without a checked `Valid` for is
/// answered first (O-LIVE-1 follow-up): the Coverage door needs `basis_k ≥ 2`, which the shipped
/// geometry (one full seat, four one-partition seats) reaches only with all five seats' `Valid`, so
/// the last missing seat is what every licence waits on.
pub const PALW_SEAT_LAST_MISSING_V1: usize = 2;

/// **Whether this seat's own `Valid` for the claim is already checked in the pool** — after a restart
/// the in-memory `answered` set is empty, but the receipt the seat filed comes back from its peers
/// (signed by this seat's key, so nobody else can put it there). Such a duty is answered: it is not
/// replayed again.
pub fn palw_seat_answered_in_pool_v1(item: &PalwSeatScheduleInV1) -> bool {
    item.valid.contains(&item.me)
}

/// **How many seats of the claim's panel have no checked `Valid` yet** (this seat included), or
/// `None` before the tip has seen the bind.
pub fn palw_seat_missing_v1(item: &PalwSeatScheduleInV1) -> Option<usize> {
    item.panel.as_ref().map(|panel| panel.iter().filter(|seat| !item.valid.contains(seat)).count())
}

/// **The order a seat answers its duties in — one order shared by every seat** (O-LIVE-1, 2026-10-27).
///
/// F2's order ([`palw_seat_schedule_order_v1`]) read a 3-of-5 quorum and a per-seat backup rank; the
/// Coverage door in force needs all five seats (see [`PALW_SEAT_LAST_MISSING_V1`]), so a claim three
/// seats had answered looked "satisfied" to F2 while it waited on the other two. This order:
///
/// 1. due claims this seat is among the last [`PALW_SEAT_LAST_MISSING_V1`] missing seats of — fewest
///    missing first, then deadline, bind DAA, claim id;
/// 2. due claims within [`PALW_SEAT_SHARED_URGENT_DAA_V1`] of their receipt deadline, soonest first;
/// 3. the other due claims, **oldest bind first, then claim id** (the same FIFO on every seat);
/// 4. duties that are not due, or whose own `Valid` is already in the pool ([`palw_seat_answered_in_pool_v1`]).
///
/// Keys 2–3 are chain data (deadline, bind DAA, claim id). Key 1 reads the checked receipt pool — the
/// chain holds no per-claim `Valid` tally the node can read before a licence — and a pooled receipt
/// cannot be forged, only withheld, which falls back to keys 2–3. Node policy only: no consensus
/// state, identity or fingerprint reads it.
pub fn palw_seat_shared_order_v1(items: &[PalwSeatScheduleInV1], _quorum: usize, now_daa: u64) -> Vec<usize> {
    palw_seat_shared_order_with_v1(items, now_daa, true)
}

/// [`palw_seat_shared_order_v1`], with key 1 switchable (the simulation's "shared FIFO only" arm).
pub fn palw_seat_shared_order_with_v1(items: &[PalwSeatScheduleInV1], now_daa: u64, last_missing: bool) -> Vec<usize> {
    let mut keyed: Vec<(u8, usize, u64, u64, Hash64, usize)> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let missing = palw_seat_missing_v1(item).unwrap_or(usize::MAX);
            let urgent = item.deadline <= now_daa.saturating_add(PALW_SEAT_SHARED_URGENT_DAA_V1);
            if !item.due || palw_seat_answered_in_pool_v1(item) {
                (3, 0, item.bound_daa, 0, item.claim_id, index)
            } else if last_missing && missing <= PALW_SEAT_LAST_MISSING_V1 {
                (0, missing, item.deadline, item.bound_daa, item.claim_id, index)
            } else if urgent {
                (1, 0, item.deadline, item.bound_daa, item.claim_id, index)
            } else {
                (2, 0, item.bound_daa, 0, item.claim_id, index)
            }
        })
        .collect();
    keyed.sort();
    keyed.into_iter().map(|(.., index)| index).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::tx::TransactionOutpoint;

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_bytes([n; 64]), 0))
    }

    /// A claim id as the chain makes them: 64 bytes that look random (a splitmix stream seeded by `n`).
    fn claim(n: u8) -> Hash64 {
        let mut state = u64::from(n).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA5A5_A5A5;
        let mut bytes = [0u8; 64];
        for chunk in bytes.chunks_exact_mut(8) {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            chunk.copy_from_slice(&(z ^ (z >> 31)).to_le_bytes());
        }
        Hash64::from_bytes(bytes)
    }

    fn panel() -> Vec<PalwBondKeyV2> {
        (1..=5).map(bond).collect()
    }

    fn item(n: u8, me: u8, valid: &[u8], bound_daa: u64) -> PalwSeatScheduleInV1 {
        PalwSeatScheduleInV1 {
            claim_id: claim(n),
            deadline: 1_000 + u64::from(n),
            bound_daa,
            me: bond(me),
            panel: Some(panel()),
            valid: valid.iter().map(|b| bond(*b)).collect(),
            due: true,
            big: false,
        }
    }

    /// **The b6/5.104 case**: the fast seats have answered; a slow seat is one receipt short of a
    /// licence and does that claim before a fresh one, and a claim whose quorum is already pooled goes
    /// last. Run at a quorum of 4 over the five seats so that every unsatisfied claim has this seat among
    /// its primaries whatever the hash ranking says (the ranking is tested apart), and past the
    /// oldest-first guard: the two oldest claims lead, then closeness.
    #[test]
    fn a_claim_one_receipt_short_is_replayed_before_a_fresh_one_and_a_satisfied_one_last() {
        let me = 5;
        let mut items = vec![item(10, me, &[], 100), item(11, me, &[], 100)];
        items[0].deadline = 500;
        items[1].deadline = 500; // the two oldest take the guard
        items.push(item(3, me, &[1, 2, 3], 100)); // 2: one short
        items.push(item(1, me, &[], 100)); //        3: fresh, four short
        items.push(item(4, me, &[1, 2], 100)); //    4: two short
        items.push(item(2, me, &[1, 2, 3, 4], 100)); // 5: quorum pooled
        for (index, short) in [(2, 1), (3, 4), (4, 2)] {
            assert_eq!(palw_seat_tier_v1(&items[index], 4, 110), (PalwSeatTierV1::Needed, short), "claim {index} is primary work");
        }
        assert_eq!(palw_seat_tier_v1(&items[5], 4, 110).0, PalwSeatTierV1::Satisfied);
        assert_eq!(
            palw_seat_schedule_order_v1(&items, 4, 110),
            vec![0, 1, 2, 4, 3, 5],
            "guard, then closest to quorum, satisfied last"
        );
    }

    /// **The oldest-first guard holds under a standing backlog**: with every claim old (an age
    /// threshold would have promoted them all and switched the scheduler off), the two oldest due claims
    /// short of their quorum go first, then the rest by closeness; a claim whose duty is not due, or
    /// whose quorum is already pooled, does not take a guard place.
    #[test]
    fn the_two_oldest_due_claims_go_first_under_a_backlog_and_the_rest_by_closeness() {
        let me = 5;
        let now = 10_000; // every claim below has waited far past every age bound
        let mut items = Vec::new();
        for n in 1..=8u8 {
            let valid: &[u8] = match n % 3 {
                0 => &[1, 2],
                1 => &[1],
                _ => &[],
            };
            let mut it = item(n, me, valid, 100);
            it.deadline = 2_000 - 10 * u64::from(n); // claim 8 is the oldest, claim 1 the youngest
            items.push(it);
        }
        items[7].due = false; // the very oldest is not due (answered)
        items[5].valid = [1, 2, 3].map(bond).into_iter().collect(); // claim 6: quorum pooled
        let order = palw_seat_schedule_order_v1(&items, 3, now);
        let due_unsatisfied_oldest: Vec<usize> = {
            let mut c: Vec<usize> = (0..items.len())
                .filter(|i| items[*i].due && palw_seat_tier_v1(&items[*i], 3, now).0 != PalwSeatTierV1::Satisfied)
                .collect();
            c.sort_by_key(|i| items[*i].deadline);
            c.into_iter().take(PALW_SEAT_OLDEST_GUARD_V1).collect()
        };
        let mut led: Vec<usize> = order[..PALW_SEAT_OLDEST_GUARD_V1].to_vec();
        led.sort_unstable();
        let mut want = due_unsatisfied_oldest.clone();
        want.sort_unstable();
        assert_eq!(led, want, "the guard's places lead");
        assert_eq!(order.last(), Some(&7), "a duty that is not due is last");
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..items.len()).collect::<Vec<_>>(), "a permutation");
    }

    /// **A big claim — a slow class far from its deadline — waits behind the lighter ones**, takes no
    /// guard place, and is not lighter work for the gate; urgent, it is an ordinary claim again.
    #[test]
    fn a_big_claim_waits_behind_lighter_work_until_it_is_urgent() {
        let me = 5;
        let mut big = item(1, me, &[], 100);
        big.deadline = 100; // the oldest by far
        big.big = true;
        let light_a = item(2, me, &[], 100);
        let light_b = item(3, me, &[], 100);
        let items = [big.clone(), light_a.clone(), light_b.clone()];
        let order = palw_seat_schedule_order_v1(&items, 4, 110);
        assert_eq!(order.last(), Some(&0), "the big claim is last although it is the oldest");
        assert!(palw_seat_light_pending_v1(&items, 4, 110), "lighter work waits: the gate closes");
        // Only big claims: nothing lighter waits, the gate opens, and the big claim is guarded.
        assert!(!palw_seat_light_pending_v1(&[big.clone()], 4, 110));
        assert_eq!(palw_seat_schedule_order_v1(&[big.clone()], 4, 110), vec![0]);
        // A lighter duty that is not due, or whose quorum is pooled, is not lighter work.
        let mut answered = light_a.clone();
        answered.due = false;
        let pooled = item(4, me, &[1, 2, 3, 4], 100);
        assert!(!palw_seat_light_pending_v1(&[big.clone(), answered, pooled], 4, 110));
        // Urgent: the panel clears `big`, and the claim is an ordinary (here the oldest, guarded) one.
        big.big = false;
        assert_eq!(palw_seat_schedule_order_v1(&[big, light_a, light_b], 4, 110)[0], 0, "urgent: first again");
    }

    /// **Nobody is skipped for ever**: a backup becomes primary work after the aging bound, and a
    /// satisfied claim after its own — a backstop for what the rank guard does not see.
    #[test]
    fn a_lower_tier_ages_into_the_needed_one() {
        let mut backup = None;
        for n in 1..=64u8 {
            for me in 1..=5u8 {
                let it = item(n, me, &[], 100);
                if palw_seat_tier_v1(&it, 3, 100).0 == PalwSeatTierV1::Backup {
                    backup = Some(it);
                }
            }
        }
        let it = backup.expect("with 3 + 1 spare of 5 primaries, one seat in five is a backup");
        assert_eq!(palw_seat_tier_v1(&it, 3, 100 + PALW_SEAT_BACKUP_AFTER_DAA_V1 - 1).0, PalwSeatTierV1::Backup);
        assert_eq!(palw_seat_tier_v1(&it, 3, 100 + PALW_SEAT_BACKUP_AFTER_DAA_V1).0, PalwSeatTierV1::Needed, "aged: primary work");
        let satisfied = item(9, 5, &[1, 2, 3], 100);
        assert_eq!(palw_seat_tier_v1(&satisfied, 3, 100 + PALW_SEAT_SATISFIED_AFTER_DAA_V1 - 1).0, PalwSeatTierV1::Satisfied);
        assert_eq!(
            palw_seat_tier_v1(&satisfied, 3, 100 + PALW_SEAT_SATISFIED_AFTER_DAA_V1).0,
            PalwSeatTierV1::Needed,
            "aged: replayed anyway"
        );
    }

    /// **Seats do not all pick the same claims**: over many claims with nothing pooled, exactly one seat
    /// of each claim is the backup (3 needed + 1 spare = 4 primaries of 5) — the de-synchronisation that
    /// stops five seats doing one claim at once — and the load is shared, not piled on one seat. The
    /// ranking is the same function on every seat.
    #[test]
    fn exactly_one_seat_in_five_defers_a_fresh_claim() {
        let mut backups_per_seat = [0usize; 5];
        for n in 1..=250u8 {
            let mut backups = 0;
            for me in 1..=5u8 {
                if palw_seat_tier_v1(&item(n, me, &[], 100), 3, 100).0 == PalwSeatTierV1::Backup {
                    backups += 1;
                    backups_per_seat[usize::from(me) - 1] += 1;
                }
            }
            assert_eq!(backups, 1, "claim {n}: 3 needed + 1 spare = 4 primaries of 5");
        }
        for (seat, count) in backups_per_seat.iter().enumerate() {
            assert!((30..=70).contains(count), "seat {seat} defers {count} of 250: the load is shared, not piled on one seat");
        }
        let a = palw_seat_rank_key_v1(&claim(7), &bond(2));
        assert_eq!(a, palw_seat_rank_key_v1(&claim(7), &bond(2)));
        assert_ne!(a, palw_seat_rank_key_v1(&claim(7), &bond(3)));
    }

    /// **A receipt from a seat off the panel, or this seat's own, moves nothing**, and a duty with no
    /// panel read keeps the old order.
    #[test]
    fn only_the_panels_other_seats_count_and_no_panel_means_the_old_order() {
        let it = item(1, 5, &[5, 9], 100);
        let (_, short) = palw_seat_tier_v1(&it, 3, 100);
        assert_eq!(short, 3, "own and off-panel receipts are not other panel seats'");
        // No panel: needed, 3 short — the order is the deadline's (the guard takes the two oldest).
        let mut no_panel = [item(1, 5, &[], 100), item(2, 5, &[], 100), item(3, 5, &[], 100)];
        for it in &mut no_panel {
            it.panel = None;
        }
        no_panel[0].deadline = 30;
        no_panel[1].deadline = 10;
        no_panel[2].deadline = 20;
        assert_eq!(palw_seat_schedule_order_v1(&no_panel, 3, 100), vec![1, 2, 0], "soonest receipt deadline first");
    }

    /// **Under a backlog the scheduler is a permutation**: every duty is listed once, whatever the
    /// pool holds.
    #[test]
    fn the_order_is_a_permutation_of_the_duties() {
        let items: Vec<_> = (1..=40u8)
            .map(|n| item(n, 1 + n % 5, &[(n % 5) + 1, ((n + 1) % 5) + 1][..usize::from(n % 3).min(2)], u64::from(n)))
            .collect();
        let order = palw_seat_schedule_order_v1(&items, 3, 500);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..items.len()).collect::<Vec<_>>());
    }

    /// **O-LIVE-1: every seat picks the same claims in the same order**, whatever order the duty list
    /// arrives in and whichever seat it is (nothing pooled yet: the live case, `sched_satisfied=0`).
    #[test]
    fn the_shared_order_is_the_same_on_every_seat_and_for_every_listing() {
        let base: Vec<PalwSeatScheduleInV1> = (1..=20u8)
            .map(|n| {
                let mut it = item(n, 1, &[], 100 + u64::from(n * 7 % 11));
                it.deadline = it.bound_daa + 600;
                it
            })
            .collect();
        let picks = |items: &[PalwSeatScheduleInV1]| -> Vec<Hash64> {
            palw_seat_shared_order_v1(items, 3, 200).into_iter().map(|i| items[i].claim_id).collect()
        };
        let want = picks(&base);
        for me in 1..=5u8 {
            for rotate in [0usize, 3, 7, 19] {
                let mut items: Vec<_> = base
                    .iter()
                    .cloned()
                    .map(|mut it| {
                        it.me = bond(me);
                        it
                    })
                    .collect();
                items.rotate_left(rotate);
                items.reverse();
                assert_eq!(picks(&items), want, "seat {me}, listing rotated {rotate} and reversed");
            }
        }
        // Oldest bind first, the claim id breaking ties.
        let mut sorted = base.clone();
        sorted.sort_by_key(|it| (it.bound_daa, it.claim_id));
        assert_eq!(want, sorted.iter().map(|it| it.claim_id).collect::<Vec<_>>());
    }

    /// **The deadline guarantee holds**: a younger claim within the urgent horizon of its deadline goes
    /// before every older claim that is not, soonest deadline first; a duty that is not due is last.
    #[test]
    fn a_claim_near_its_deadline_goes_first_in_the_shared_order() {
        let now = 1_000;
        let mut old = item(1, 5, &[], 500);
        old.deadline = now + PALW_SEAT_SHARED_URGENT_DAA_V1 + 1;
        let mut young_urgent = item(2, 5, &[], 900);
        young_urgent.deadline = now + 40;
        let mut younger_more_urgent = item(3, 5, &[], 950);
        younger_more_urgent.deadline = now + 10;
        let mut answered = item(5, 5, &[], 100);
        answered.due = false;
        let items = [old, young_urgent, younger_more_urgent, answered];
        assert_eq!(palw_seat_shared_order_v1(&items, 3, now), vec![2, 1, 0, 3]);
    }

    /// **The last missing seat goes first**: a claim four seats have answered leads one three have,
    /// which leads an urgent or older claim nobody has; one where this seat is not among the last two
    /// keeps its FIFO place.
    #[test]
    fn the_last_missing_seat_answers_that_claim_first() {
        let now = 1_000;
        let mut oldest_fresh = item(1, 5, &[], 100);
        oldest_fresh.deadline = now + 10; // urgent too
        let three_answered = item(2, 5, &[1, 2, 3], 900); // this seat one of the last 2
        let four_answered = item(3, 5, &[1, 2, 3, 4], 950); // this seat the last
        let mut two_answered = item(4, 5, &[1, 2], 50); // 3 missing: FIFO (oldest of all)
        two_answered.deadline = now + 500;
        let items = [oldest_fresh, three_answered, four_answered, two_answered];
        assert_eq!(palw_seat_shared_order_v1(&items, 3, now), vec![2, 1, 0, 3]);
        assert_eq!(palw_seat_missing_v1(&items[2]), Some(1));
    }

    /// **A duty whose own `Valid` is already pooled is not replayed again** (b6 after its restart at
    /// 12:36): listed last, as a duty that is not due.
    #[test]
    fn a_duty_answered_before_a_restart_is_skipped() {
        let mine = item(1, 5, &[5], 10); // own Valid heard back from the peers
        let fresh = item(2, 5, &[], 900);
        assert!(palw_seat_answered_in_pool_v1(&mine));
        assert_eq!(palw_seat_shared_order_v1(&[mine, fresh], 3, 1_000), vec![1, 0]);
    }

    /// **Simulation (O-DIAG2's model)**: panels of 5 drawn from the seats, a licence only when ALL FIVE
    /// seats' `Valid` are in (Coverage door, `basis_k ≥ 2`), a claim every `every_min` minutes, 2 slots a
    /// seat, each seat's replays taking `120 / rate` minutes (±15%), receipts visible to the other seats
    /// 3 minutes later, a DAA every 2.6 minutes, a 600-DAA receipt window. `mode`: 0 = F2 (as live, read
    /// at quorum 3), 1 = shared FIFO, 2 = last-missing-first. Returns `(licences, claims, replays, open)`.
    fn simulate(mode: u8, rates: &[u64], every_min: u64, hours: u64) -> (usize, usize, usize, usize) {
        let gossip_min = 3;
        let seats = rates.len() as u8;
        let mix = |a: u64, b: u64| -> u64 {
            let mut z = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        struct Claim {
            id: Hash64,
            bound: u64,
            panel: Vec<u8>,
            receipts: Vec<(u8, u64)>,
            answered: HashSet<u8>,
            licensed: bool,
        }
        let daa = |minute: u64| minute * 10 / 26;
        let mut claims: Vec<Claim> = Vec::new();
        let mut running: Vec<Vec<(usize, u64)>> = vec![Vec::new(); usize::from(seats)];
        let (mut licences, mut spent) = (0usize, 0usize);
        for minute in 0..hours * 60 {
            let now = daa(minute);
            if minute % every_min == 0 {
                let c = claims.len() as u64;
                let mut bytes = [0u8; 64];
                for (k, chunk) in bytes.chunks_exact_mut(8).enumerate() {
                    chunk.copy_from_slice(&mix(c + 1, k as u64).to_le_bytes());
                }
                let mut pool: Vec<u8> = (1..=seats).collect();
                pool.sort_by_key(|s| mix(c, u64::from(*s)));
                claims.push(Claim {
                    id: Hash64::from_bytes(bytes),
                    bound: now,
                    panel: pool[..5].to_vec(),
                    receipts: Vec::new(),
                    answered: HashSet::new(),
                    licensed: false,
                });
            }
            for seat in 1..=seats {
                let slots = &mut running[usize::from(seat - 1)];
                for (c, _) in slots.iter().filter(|(_, end)| *end == minute) {
                    let claim = &mut claims[*c];
                    claim.answered.insert(seat);
                    spent += 1;
                    if now <= claim.bound + 600 {
                        claim.receipts.push((seat, minute + gossip_min));
                        if !claim.licensed && claim.receipts.len() >= 5 {
                            claim.licensed = true;
                            licences += 1;
                        }
                    }
                }
                slots.retain(|(_, end)| *end > minute);
            }
            for seat in 1..=seats {
                let busy: Vec<usize> = running[usize::from(seat - 1)].iter().map(|(c, _)| *c).collect();
                if busy.len() >= 2 {
                    continue;
                }
                let index: Vec<usize> = (0..claims.len())
                    .filter(|c| {
                        let claim = &claims[*c];
                        claim.panel.contains(&seat)
                            && !claim.licensed
                            && !claim.answered.contains(&seat)
                            && !busy.contains(c)
                            && now <= claim.bound + 600
                    })
                    .collect();
                let items: Vec<PalwSeatScheduleInV1> = index
                    .iter()
                    .map(|c| {
                        let claim = &claims[*c];
                        PalwSeatScheduleInV1 {
                            claim_id: claim.id,
                            deadline: claim.bound + 600,
                            bound_daa: claim.bound,
                            me: bond(seat),
                            panel: Some(claim.panel.iter().map(|s| bond(*s)).collect()),
                            valid: claim.receipts.iter().filter(|(_, at)| *at <= minute).map(|(s, _)| bond(*s)).collect(),
                            due: true,
                            big: false,
                        }
                    })
                    .collect();
                let order = match mode {
                    0 => palw_seat_schedule_order_v1(&items, 3, now),
                    1 => palw_seat_shared_order_with_v1(&items, now, false),
                    _ => palw_seat_shared_order_v1(&items, 3, now),
                };
                let base = 120 / rates[usize::from(seat - 1)];
                for pick in order.into_iter().take(2 - busy.len()) {
                    let c = index[pick];
                    let took = base * 85 / 100 + mix(c as u64, u64::from(seat) + 99) % (base * 30 / 100 + 1);
                    running[usize::from(seat - 1)].push((c, minute + took.max(1)));
                }
            }
        }
        let now = daa(hours * 60);
        let open = claims.iter().filter(|c| !c.licensed && now <= c.bound + 600).count();
        (licences, claims.len(), spent, open)
    }

    /// **The three orders under 5-of-5 coverage** at the live inflow (15 claims an hour) over 48 hours,
    /// 8 seats (5 fast at 8–10 replays an hour, 3 slow at 3–6) and 7 (the slowest retired); run with
    /// `--nocapture` for the numbers. Last-missing-first must form no fewer licences than either.
    #[test]
    fn last_missing_first_forms_coverage_licences_under_the_live_inflow() {
        for (name, rates, every) in [
            ("8 seats 15/h", &[10u64, 9, 9, 8, 8, 6, 4, 3][..], 4u64),
            ("7 seats 15/h", &[10u64, 9, 9, 8, 8, 6, 4][..], 4),
            ("8 seats 10/h", &[10u64, 9, 9, 8, 8, 6, 4, 3][..], 6),
            ("7 seats 10/h", &[10u64, 9, 9, 8, 8, 6, 4][..], 6),
        ] {
            let r: Vec<_> = (0..3u8).map(|mode| simulate(mode, rates, every, 48)).collect();
            eprintln!(
                "O-LIVE-1 sim {name} 48h claims={}: F2 lic={} open={} rep={} | FIFO lic={} open={} rep={} | last-missing lic={} open={} rep={}",
                r[0].1, r[0].0, r[0].3, r[0].2, r[1].0, r[1].3, r[1].2, r[2].0, r[2].3, r[2].2
            );
            assert!(r[2].0 * 100 >= r[0].0 * 99 && r[2].0 * 100 >= r[1].0 * 99, "{name}: last-missing-first forms no fewer licences");
        }
    }
}
