//! ADR-0125 — the execution lane's round rules, as pure functions.
//!
//! **Consensus-inert: nothing calls these yet.** The lane that will read them — its algorithm id,
//! envelope, slot rule and acceptance gate — is ADR-0125 §7. What is pinned here first is the
//! arithmetic a fast lane must not get wrong once it exists:
//!
//! * a **round** is one second from the genesis timestamp ([`palw_execution_round_v1`]);
//! * a round's **seed** is the chain's beacon and the round, nothing a producer chooses
//!   ([`palw_execution_seed_v1`]);
//! * a domain's **quota** is its share of the previous scheduler epoch's `Final` credits, capped at
//!   [`PALW_EXEC_DOMAIN_CAP_PERMILLE`] and renormalised ([`palw_execution_quotas_v1`]) — the reward
//!   follows the compute, the chain's block production does not;
//! * the round's **permits** are the lowest tickets that pass two alternation rules
//!   ([`palw_execution_permits_v1`]): at most one permit an operator a round, at most
//!   `⌈width / 3⌉` permits a security domain a round, and a domain that filled its cap in the
//!   previous round holds no permit in this one — at `width = 1` that is "no two consecutive
//!   blocks from one domain". A round nobody passes is empty.
//!
//! Widening the lane from 1 BPS to 10 BPS is [`PALW_EXEC_PERMITS_PER_ROUND_V1`] growing from 1 to
//! 10 behind fences of its own; nothing in these functions changes between the two.

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;

/// One round is one second.
pub const PALW_EXEC_ROUND_MS: u64 = 1_000;

/// Stage 1: one permit a round — 1 BPS. Stages 2, 5 and 10 are this constant, larger, behind
/// their own fences (ADR-0125 Decision 5).
pub const PALW_EXEC_PERMITS_PER_ROUND_V1: u16 = 1;

/// No security domain holds more than 45 % of an epoch's permits, whatever its compute
/// (ADR-0125 Decision 3).
pub const PALW_EXEC_DOMAIN_CAP_PERMILLE: u64 = 450;

/// The domain of a round's seed.
pub const PALW_EXEC_ROUND_SEED_DOMAIN: &[u8] = b"misaka-palw/exec-lane/round-seed/v1";
/// The domain of a bond's ticket in a round.
pub const PALW_EXEC_TICKET_DOMAIN: &[u8] = b"misaka-palw/exec-lane/ticket/v1";

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The round a timestamp falls in: whole seconds since the genesis timestamp; a timestamp before
/// genesis is round 0.
pub fn palw_execution_round_v1(timestamp_ms: u64, genesis_timestamp_ms: u64) -> u64 {
    timestamp_ms.saturating_sub(genesis_timestamp_ms) / PALW_EXEC_ROUND_MS
}

/// `H(domain ‖ beacon ‖ round)`: the beacon is the ADR-0074 fact of the last attempt block at or
/// below the round's anchor, so no producer's own block moves the round it would like.
pub fn palw_execution_seed_v1(beacon: &Hash64, round: u64) -> Hash64 {
    let mut h = keyed(PALW_EXEC_ROUND_SEED_DOMAIN);
    h.update(beacon.as_byte_slice());
    h.update(&round.to_le_bytes());
    finish(h)
}

/// One eligible bond in a round: who it is, whose it is, and which security domain (its class's
/// certified family) it produces for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecutionCandidateV1 {
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
    pub domain: Hash64,
}

/// One permit: the round's index, and the candidate that holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecutionPermitV1 {
    pub index: u16,
    pub candidate: PalwExecutionCandidateV1,
}

/// **Quotas from credits: proportional, capped at 45 %, renormalised.** `credits` is each domain's
/// `Final` attempt count in the previous scheduler epoch (ADR-0107's count). A single domain holds
/// the whole lane — there is nobody to cap it against — and an empty census yields no quota.
/// Returns `(domain, permille)`, summing to 1000 over a non-empty census with more than one domain
/// (largest remainder), and to 1000 for one.
pub fn palw_execution_quotas_v1(credits: &[(Hash64, u64)]) -> Vec<(Hash64, u16)> {
    let total: u128 = credits.iter().map(|(_, c)| *c as u128).sum();
    if total == 0 {
        return Vec::new();
    }
    if credits.len() == 1 {
        return vec![(credits[0].0, 1000)];
    }
    // Raw shares, then the cap, then the remainder redistributed among the uncapped in proportion
    // until nobody is over the cap (at most `n` passes: each pass caps at least one more domain).
    let mut shares: Vec<(Hash64, f64)> = credits.iter().map(|(d, c)| (*d, (*c as f64) / (total as f64))).collect();
    let cap = PALW_EXEC_DOMAIN_CAP_PERMILLE as f64 / 1000.0;
    loop {
        let over: f64 = shares.iter().filter(|(_, s)| *s > cap).map(|(_, s)| s - cap).sum();
        if over <= f64::EPSILON {
            break;
        }
        let under_total: f64 = shares.iter().filter(|(_, s)| *s < cap).map(|(_, s)| *s).sum();
        if under_total <= f64::EPSILON {
            // Everyone is at or over the cap: the cap cannot be honoured, share equally.
            let equal = 1.0 / shares.len() as f64;
            for (_, s) in shares.iter_mut() {
                *s = equal;
            }
            break;
        }
        for (_, s) in shares.iter_mut() {
            if *s > cap {
                *s = cap;
            } else if *s < cap {
                *s += over * (*s / under_total);
            }
        }
    }
    // Integer permille by largest remainder, so the quotas sum to exactly 1000.
    let mut rows: Vec<(usize, u16, f64)> = shares
        .iter()
        .enumerate()
        .map(|(i, (_, s))| {
            let scaled = s * 1000.0;
            let floor = scaled.floor();
            (i, floor as u16, scaled - floor)
        })
        .collect();
    let assigned: u32 = rows.iter().map(|(_, p, _)| *p as u32).sum();
    let mut remainder = 1000u32.saturating_sub(assigned) as usize;
    rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    for row in rows.iter_mut() {
        if remainder == 0 {
            break;
        }
        row.1 += 1;
        remainder -= 1;
    }
    rows.sort_by_key(|(i, _, _)| *i);
    rows.into_iter().map(|(i, p, _)| (shares[i].0, p)).collect()
}

/// The most permits one domain may hold in a round of `width`: a third, rounded up — one at
/// `width = 1` (and so "no two consecutive rounds", below), four at `width = 10`.
pub fn palw_execution_domain_cap_v1(width: u16) -> u16 {
    width.div_ceil(3).max(1)
}

/// **The round's permits.** Every candidate draws `H(domain ‖ seed ‖ bond)`; tickets sort
/// ascending; the first `width` that pass the rules hold the permits in ticket order:
///
/// * one permit an operator a round;
/// * at most [`palw_execution_domain_cap_v1`]`(width)` permits a domain a round;
/// * a domain that held its whole cap in `previous` (the last round's permits) holds nothing now.
///
/// Deterministic in `(seed, candidates, previous)`; the candidate order does not matter. A round
/// in which nobody passes is empty, and that is the rule working: a single live domain cannot
/// chain rounds.
pub fn palw_execution_permits_v1(
    seed: &Hash64,
    candidates: &[PalwExecutionCandidateV1],
    width: u16,
    previous: &[PalwExecutionPermitV1],
) -> Vec<PalwExecutionPermitV1> {
    let cap = palw_execution_domain_cap_v1(width) as usize;
    // Domains that filled their cap last round sit this one out.
    let mut last_counts: std::collections::BTreeMap<Hash64, usize> = std::collections::BTreeMap::new();
    for permit in previous {
        *last_counts.entry(permit.candidate.domain).or_insert(0) += 1;
    }
    let rested: std::collections::BTreeSet<Hash64> = last_counts.iter().filter(|(_, n)| **n >= cap).map(|(d, _)| *d).collect();

    let mut tickets: Vec<(Hash64, PalwExecutionCandidateV1)> = candidates
        .iter()
        .map(|c| {
            let mut h = keyed(PALW_EXEC_TICKET_DOMAIN);
            h.update(seed.as_byte_slice());
            h.update(&borsh::to_vec(&c.bond).expect("bond keys are borsh-serializable"));
            (finish(h), *c)
        })
        .collect();
    tickets.sort_by(|a, b| a.0.as_bytes().cmp(&b.0.as_bytes()).then(a.1.bond.cmp(&b.1.bond)));

    let mut permits = Vec::new();
    let mut operators: std::collections::BTreeSet<Hash64> = std::collections::BTreeSet::new();
    let mut domains: std::collections::BTreeMap<Hash64, usize> = std::collections::BTreeMap::new();
    for (_, candidate) in tickets {
        if permits.len() == width as usize {
            break;
        }
        if rested.contains(&candidate.domain) || operators.contains(&candidate.operator_id) {
            continue;
        }
        let held = domains.entry(candidate.domain).or_insert(0);
        if *held >= cap {
            continue;
        }
        *held += 1;
        operators.insert(candidate.operator_id);
        permits.push(PalwExecutionPermitV1 { index: permits.len() as u16, candidate });
    }
    permits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn cand(bond: u64, operator: u64, domain: u64) -> PalwExecutionCandidateV1 {
        PalwExecutionCandidateV1 {
            bond: PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(bond), 0)),
            operator_id: h(operator),
            domain: h(domain),
        }
    }

    #[test]
    fn a_round_is_a_second_from_genesis_and_the_seed_is_the_beacon_and_the_round() {
        assert_eq!(palw_execution_round_v1(1_000_000, 1_000_000), 0);
        assert_eq!(palw_execution_round_v1(1_000_999, 1_000_000), 0);
        assert_eq!(palw_execution_round_v1(1_001_000, 1_000_000), 1);
        assert_eq!(palw_execution_round_v1(0, 1_000_000), 0, "before genesis is round 0, never a wrap");
        assert_eq!(palw_execution_round_v1(1_000_000 + 120_000, 1_000_000), 120, "120 rounds an anchor");
        let beacon = h(7);
        assert_ne!(palw_execution_seed_v1(&beacon, 1), palw_execution_seed_v1(&beacon, 2));
        assert_ne!(palw_execution_seed_v1(&beacon, 1), palw_execution_seed_v1(&h(8), 1));
        assert_eq!(palw_execution_seed_v1(&beacon, 1), palw_execution_seed_v1(&beacon, 1));
    }

    #[test]
    fn quotas_follow_credits_up_to_the_cap_and_sum_to_a_thousand() {
        assert!(palw_execution_quotas_v1(&[]).is_empty());
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 0)]), vec![], "no credits, no quota");
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 5)]), vec![(h(1), 1000)], "one domain holds the lane");
        // 70 / 20 / 10: the 70 is capped at 450 and its excess flows to the others in proportion.
        let q = palw_execution_quotas_v1(&[(h(1), 70), (h(2), 20), (h(3), 10)]);
        let by: std::collections::BTreeMap<Hash64, u16> = q.iter().copied().collect();
        assert_eq!(by[&h(1)], 450, "capped");
        assert_eq!(by[&h(2)] + by[&h(3)], 550);
        assert_eq!(by[&h(2)], 367, "2 : 1 of the rest, largest remainder");
        assert_eq!(by[&h(3)], 183);
        // Under the cap nothing moves.
        let q = palw_execution_quotas_v1(&[(h(1), 40), (h(2), 35), (h(3), 25)]);
        assert_eq!(q, vec![(h(1), 400), (h(2), 350), (h(3), 250)]);
        // Two whales: both capped, the cap cannot hold — equal shares.
        let q = palw_execution_quotas_v1(&[(h(1), 50), (h(2), 50)]);
        assert_eq!(q, vec![(h(1), 500), (h(2), 500)]);
        for census in
            [vec![(h(1), 1), (h(2), 1), (h(3), 1)], vec![(h(1), 99), (h(2), 1)], vec![(h(1), 3), (h(2), 3), (h(3), 3), (h(4), 1)]]
        {
            let sum: u32 = palw_execution_quotas_v1(&census).iter().map(|(_, p)| *p as u32).sum();
            assert_eq!(sum, 1000, "{census:?}");
        }
    }

    #[test]
    fn the_domain_cap_is_a_third_rounded_up() {
        assert_eq!(palw_execution_domain_cap_v1(1), 1);
        assert_eq!(palw_execution_domain_cap_v1(2), 1);
        assert_eq!(palw_execution_domain_cap_v1(3), 1);
        assert_eq!(palw_execution_domain_cap_v1(5), 2);
        assert_eq!(palw_execution_domain_cap_v1(10), 4);
    }

    #[test]
    fn a_round_hands_one_permit_an_operator_caps_a_domain_and_rests_it_next_round() {
        let candidates = vec![
            cand(1, 100, 1),
            cand(2, 100, 1), // the same operator as bond 1: at most one of them a round
            cand(3, 101, 1),
            cand(4, 102, 2),
            cand(5, 103, 2),
            cand(6, 104, 3),
        ];
        let seed = palw_execution_seed_v1(&h(9), 42);
        let permits = palw_execution_permits_v1(&seed, &candidates, 10, &[]);
        // Six candidates, one operator duplicated: five permits at most, and domain 1 (three bonds,
        // two operators) can hold at most two of them under an operator rule and the cap of four.
        assert_eq!(permits.len(), 5);
        let operators: std::collections::BTreeSet<Hash64> = permits.iter().map(|p| p.candidate.operator_id).collect();
        assert_eq!(operators.len(), 5, "one permit an operator");
        assert_eq!(permits.iter().filter(|p| p.candidate.domain == h(1)).count(), 2);
        // The order is the ticket order and the index says so.
        for (i, permit) in permits.iter().enumerate() {
            assert_eq!(permit.index as usize, i);
        }
        // Deterministic and order-independent.
        let mut shuffled = candidates.clone();
        shuffled.reverse();
        assert_eq!(palw_execution_permits_v1(&seed, &shuffled, 10, &[]), permits);
        // A different seed is a different order.
        let other = palw_execution_permits_v1(&palw_execution_seed_v1(&h(9), 43), &candidates, 10, &[]);
        assert_eq!(other.len(), 5);

        // Width 1: one permit, and the domain that held it rests next round.
        let first = palw_execution_permits_v1(&seed, &candidates, 1, &[]);
        assert_eq!(first.len(), 1);
        let held = first[0].candidate.domain;
        let second = palw_execution_permits_v1(&palw_execution_seed_v1(&h(9), 43), &candidates, 1, &first);
        assert_eq!(second.len(), 1);
        assert_ne!(second[0].candidate.domain, held, "no two consecutive rounds from one domain");
        // Width 3, cap 1: three domains, one each; a domain that filled its cap (one) rests.
        let three = palw_execution_permits_v1(&seed, &candidates, 3, &[]);
        assert_eq!(three.len(), 3);
        let domains: std::collections::BTreeSet<Hash64> = three.iter().map(|p| p.candidate.domain).collect();
        assert_eq!(domains.len(), 3, "one a domain at width 3");
        let next = palw_execution_permits_v1(&palw_execution_seed_v1(&h(9), 43), &candidates, 3, &three);
        assert!(next.is_empty(), "every domain filled its cap of one, so every domain rests: an empty round");
    }

    #[test]
    fn one_live_domain_runs_at_half_the_width_and_never_chains() {
        let candidates = vec![cand(1, 100, 1), cand(2, 101, 1), cand(3, 102, 1)];
        let mut previous = Vec::new();
        let mut produced = 0;
        for round in 0..100u64 {
            let permits = palw_execution_permits_v1(&palw_execution_seed_v1(&h(1), round), &candidates, 1, &previous);
            assert!(permits.len() <= 1);
            if !permits.is_empty() {
                assert!(previous.is_empty(), "a permit never follows a permit from the same (only) domain");
                produced += 1;
            }
            previous = permits;
        }
        assert_eq!(produced, 50, "exactly every other round");
    }

    #[test]
    fn a_ninety_percent_domain_holds_at_most_a_third_of_a_run_at_width_ten_and_the_count_is_all_that_widens() {
        // Nine operators in domain 1, one each in domains 2 and 3.
        let mut candidates: Vec<PalwExecutionCandidateV1> = (1..=9).map(|i| cand(i, 100 + i, 1)).collect();
        candidates.push(cand(10, 200, 2));
        candidates.push(cand(11, 300, 3));
        let mut previous = Vec::new();
        let mut by_domain: std::collections::BTreeMap<Hash64, usize> = std::collections::BTreeMap::new();
        let mut total = 0;
        for round in 0..200u64 {
            let permits = palw_execution_permits_v1(&palw_execution_seed_v1(&h(5), round), &candidates, 10, &previous);
            for permit in &permits {
                *by_domain.entry(permit.candidate.domain).or_insert(0) += 1;
            }
            total += permits.len();
            previous = permits;
        }
        assert!(total > 0);
        let dominant = by_domain[&h(1)] as f64 / total as f64;
        assert!(dominant <= 0.5, "domain 1 holds {dominant:.2} of the permits: capped at four a round and rested after");
        assert!(by_domain[&h(2)] > 0 && by_domain[&h(3)] > 0, "the small domains are never starved");
        // Widening changes the count and nothing else: width 1's permit is the head of width 10's.
        let seed = palw_execution_seed_v1(&h(5), 7);
        let one = palw_execution_permits_v1(&seed, &candidates, 1, &[]);
        let ten = palw_execution_permits_v1(&seed, &candidates, 10, &[]);
        assert_eq!(one[0].candidate, ten[0].candidate);
        assert_eq!(PALW_EXEC_PERMITS_PER_ROUND_V1, 1, "stage 1 is one permit a round");
    }
}
