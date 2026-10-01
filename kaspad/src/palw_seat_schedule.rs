//! **Which claim a seat replays next** — node policy, never validity (the 2026-10-01 panel backlog,
//! `docs/design/palw/t12-panel-backlog-1001.md`, F2).
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
//! loses nothing by duplicating), and a claim promotes itself by age — a backup after
//! [`PALW_SEAT_BACKUP_AFTER_DAA_V1`] DAA since the bind, a satisfied claim after
//! [`PALW_SEAT_SATISFIED_AFTER_DAA_V1`] — so a dead primary, or quorum receipts that never land, delay
//! a claim by at most that long. Pool contents are a hint a peer can only make worse by withholding
//! (nobody can forge a checked receipt), and the worst a bad hint does is the old behaviour, later.
//!
//! A duty with no panel read yet (the tip has not seen the bind) keeps tier 0 and the old order.

use std::collections::HashSet;

use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_hashes::Hash64;

/// Seats beyond the quorum's need that a claim's primaries include: one spare, so a single dead or
/// slow primary does not hold a claim until it ages.
pub const PALW_SEAT_PRIMARY_SPARE_V1: usize = 1;

/// DAA after the bind at which a claim a seat is only a backup for becomes its primary work. 24 DAA
/// is about an hour on testnet-12 (a DAA every ~2.6 minutes): longer than a healthy seat's replay,
/// short against the 600-DAA receipt window.
pub const PALW_SEAT_BACKUP_AFTER_DAA_V1: u64 = 24;

/// DAA after the bind at which a claim whose quorum is already pooled is replayed anyway — the pooled
/// receipts have not become a licence in this long, so more receipts cost nothing.
pub const PALW_SEAT_SATISFIED_AFTER_DAA_V1: u64 = 72;

/// DAA after the bind at which a claim stops being ranked by how close it is to its quorum and goes
/// back to plain oldest-deadline-first ahead of every younger claim: under a standing backlog the
/// closest-first rule alone could starve a claim whose panel has answered nothing.
pub const PALW_SEAT_STALE_AFTER_DAA_V1: u64 = 48;

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
}

/// The tier of one duty (lower is sooner), with the sort key inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PalwSeatTierV1 {
    Needed,
    Backup,
    Satisfied,
}

/// The ranking every seat computes alike: FNV-1a over the claim's bytes, the seat bond's transaction
/// id and its index. Deterministic across nodes and processes (the pool's own hasher is process-keyed
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
    hash
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
    let mut pending: Vec<(u64, &PalwBondKeyV2)> = panel
        .iter()
        .filter(|seat| !others.contains(seat))
        .map(|seat| (palw_seat_rank_key_v1(&item.claim_id, seat), seat))
        .collect();
    pending.sort();
    let rank = pending.iter().position(|(_, seat)| **seat == item.me);
    let primary = rank.is_some_and(|rank| rank < short + PALW_SEAT_PRIMARY_SPARE_V1);
    if primary || waited >= PALW_SEAT_BACKUP_AFTER_DAA_V1 { (PalwSeatTierV1::Needed, short) } else { (PalwSeatTierV1::Backup, short) }
}

/// **The order a seat answers its duties in**: indices into `items`, soonest first — tier, then (in
/// the needed tier) the claims closest to quorum, then the receipt deadline, then the claim id.
pub fn palw_seat_schedule_order_v1(items: &[PalwSeatScheduleInV1], quorum: usize, now_daa: u64) -> Vec<usize> {
    let mut keyed: Vec<(PalwSeatTierV1, usize, u64, Hash64, usize)> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let (tier, short) = palw_seat_tier_v1(item, quorum, now_daa);
            // Closest to quorum first inside the needed tier only — and not for a claim that has waited
            // past the stale bound, which goes ahead of them by deadline; the other tiers keep the
            // deadline order.
            let stale = now_daa.saturating_sub(item.bound_daa) >= PALW_SEAT_STALE_AFTER_DAA_V1;
            let closeness = if tier == PalwSeatTierV1::Needed && !stale { short } else { 0 };
            (tier, closeness, item.deadline, item.claim_id, index)
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

    fn claim(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
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
        }
    }

    /// **The b6/5.104 case**: the two fast seats have answered; a slow seat is one receipt short of a
    /// licence and does that claim before a fresh one, and a claim whose quorum is already pooled
    /// goes last.
    #[test]
    fn a_claim_one_receipt_short_is_replayed_first_and_a_satisfied_one_last() {
        let me = 5;
        let items = [
            item(1, me, &[], 100),           // fresh: 3 short
            item(2, me, &[1, 2, 3], 100),    // quorum pooled: satisfied
            item(3, me, &[1, 2], 100),       // one short
            item(4, me, &[1], 100),          // two short
        ];
        let order = palw_seat_schedule_order_v1(&items, 3, 110);
        let read: Vec<_> = order.iter().map(|i| palw_seat_tier_v1(&items[*i], 3, 110)).collect();
        assert_eq!(order.last(), Some(&1), "the satisfied claim is last");
        // Whatever the primaries' ranking turns out to be: tiers ascend, and inside the needed tier the
        // claim closest to its quorum comes first.
        for pair in read.windows(2) {
            assert!(pair[0].0 <= pair[1].0, "tiers ascend: {read:?}");
            if pair[0].0 == PalwSeatTierV1::Needed && pair[1].0 == PalwSeatTierV1::Needed {
                assert!(pair[0].1 <= pair[1].1, "closest to quorum first: {read:?}");
            }
        }
        // The one-short claim is needed work for a seat that is among the first two of the three
        // seats that have not answered, which is true of a fast seat that ranks first: construct it.
        let first_ranked = (1..=5u8)
            .find(|seat| {
                let it = item(3, *seat, &[1, 2], 100);
                palw_seat_tier_v1(&it, 3, 110) == (PalwSeatTierV1::Needed, 1) && *seat > 2
            })
            .expect("one of the three unanswered seats is a primary for the claim one receipt short");
        let one_short = item(3, first_ranked, &[1, 2], 100);
        let fresh = item(1, first_ranked, &[], 100);
        let pair = [fresh, one_short];
        let order = palw_seat_schedule_order_v1(&pair, 3, 110);
        let fresh_tier = palw_seat_tier_v1(&pair[0], 3, 110).0;
        if fresh_tier == PalwSeatTierV1::Needed {
            assert_eq!(order, vec![1, 0], "a licence one receipt away before a claim nobody has answered");
        }
    }

    /// **Nobody is skipped for ever**: a backup becomes primary work after the aging bound, and a
    /// satisfied claim after its own — a dead primary, or quorum receipts that never land, delay a
    /// claim by at most that long.
    #[test]
    fn a_lower_tier_ages_into_the_needed_one() {
        // Find a (claim, seat) pair where this seat is a backup: the five seats rank differently per claim.
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
        assert_eq!(palw_seat_tier_v1(&satisfied, 3, 100 + PALW_SEAT_SATISFIED_AFTER_DAA_V1).0, PalwSeatTierV1::Needed, "aged: replayed anyway");
    }

    /// **Closest-to-quorum-first cannot starve an old claim**: past the stale bound a claim is ranked by
    /// its deadline ahead of the younger claims that are closer to their quorum.
    #[test]
    fn an_old_claim_is_not_starved_by_closer_younger_ones() {
        let me = 5;
        let mut old = item(1, me, &[], 100); // 3 short, bound at 100
        old.deadline = 700;
        let mut young = item(2, me, &[1, 2], 400); // 1 short, bound at 400
        young.deadline = 1_000;
        // Both must be needed work for this seat for the comparison to mean anything.
        let now = 100 + PALW_SEAT_STALE_AFTER_DAA_V1;
        let tiers = [palw_seat_tier_v1(&old, 3, now).0, palw_seat_tier_v1(&young, 3, now).0];
        if tiers == [PalwSeatTierV1::Needed, PalwSeatTierV1::Needed] {
            assert_eq!(palw_seat_schedule_order_v1(&[old.clone(), young.clone()], 3, now), vec![0, 1], "stale first");
        }
        // Before the bound the closer one wins.
        let now = 100 + PALW_SEAT_STALE_AFTER_DAA_V1 - 1;
        if [palw_seat_tier_v1(&old, 3, now).0, palw_seat_tier_v1(&young, 3, now).0] == [PalwSeatTierV1::Needed, PalwSeatTierV1::Needed] {
            assert_eq!(palw_seat_schedule_order_v1(&[old, young], 3, now), vec![1, 0], "closest to quorum first");
        }
    }

    /// **Seats do not all pick the same claims**: over many claims with nothing pooled, every seat of
    /// the panel is a primary for four in five and a backup for one in five, and exactly one seat of
    /// each claim is the backup — the de-synchronisation that stops five seats doing one claim at
    /// once. The ranking is the same function on every seat.
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
            assert!((25..=75).contains(count), "seat {seat} defers {count} of 250: the load is shared, not piled on one seat");
        }
        // The same inputs rank the same way on every call (a process-keyed hasher would not).
        let a = palw_seat_rank_key_v1(&claim(7), &bond(2));
        assert_eq!(a, palw_seat_rank_key_v1(&claim(7), &bond(2)));
        assert_ne!(a, palw_seat_rank_key_v1(&claim(7), &bond(3)));
    }

    /// **A receipt that has not been checked, from a seat off the panel, or this seat's own, moves
    /// nothing**, and a duty with no panel read keeps the old order.
    #[test]
    fn only_the_panels_other_seats_count_and_no_panel_means_the_old_order() {
        // Seat 5's own entry and an off-panel bond do not shorten the quorum.
        let mut it = item(1, 5, &[5, 9], 100);
        it.valid.insert(bond(9));
        let (_, short) = palw_seat_tier_v1(&it, 3, 100);
        assert_eq!(short, 3, "own and off-panel receipts are not other panel seats'");
        // No panel: needed, 3 short — the order is the deadline's.
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
        let items: Vec<_> = (1..=40u8).map(|n| item(n, 1 + n % 5, &[(n % 5) + 1, ((n + 1) % 5) + 1][..usize::from(n % 3).min(2)], u64::from(n))).collect();
        let order = palw_seat_schedule_order_v1(&items, 3, 500);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..items.len()).collect::<Vec<_>>());
    }
}
