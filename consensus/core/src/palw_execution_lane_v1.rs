//! ADR-0125 — the execution lane's round rules, as pure functions.
//!
//! **Consensus-inert: nothing calls these yet.** The lane that will read them — its algorithm id,
//! envelope, slot rule and acceptance gate — is ADR-0125 §7. What is pinned here first is the
//! arithmetic a fast lane must not get wrong once it exists:
//!
//! * a **round** is one second from the genesis timestamp ([`palw_execution_round_v1`]);
//! * a round's **seed** is the execution commitment of a recent attempt — the nonce-free value the
//!   attempt lane's own lottery hashes (ADR-0072) — and the round, nothing a producer can re-roll
//!   for free ([`palw_execution_seed_source_v1`], [`palw_execution_seed_v1`]). **There is no beacon
//!   here**: main's attempt lottery does not read the ADR-0074 walk, and this lane does not bring it
//!   back;
//! * a domain's **quota** is its share of the previous scheduler epoch's `Final` credits, capped at
//!   [`PALW_EXEC_DOMAIN_CAP_PERMILLE`] and renormalised, in integers
//!   ([`palw_execution_quotas_v1`]) — the reward follows the compute, the chain's block production
//!   does not;
//! * the round's **permits** are the lowest tickets that pass two alternation rules
//!   ([`palw_execution_permits_v1`]): at most one permit an operator a round, at most
//!   `⌈width / 3⌉` permits a security domain a round, and a domain that filled its cap in the
//!   previous round holds no permit in this one — at `width = 1` that is "no two consecutive
//!   blocks from one domain". A round nobody passes is empty.
//!
//! Widening the lane from 1 BPS to 10 BPS is [`PALW_EXEC_PERMITS_PER_ROUND_V1`] growing from 1 to
//! 10 behind fences of its own; nothing in these functions changes between the two. Integer only:
//! a quota two platforms round differently is a fork.

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

/// How far below a candidate's selected parent the attempt that seeds its round must have been
/// accepted, in DAA (ADR-0125 Decision 2). Deep enough that no execution block can swap the seed by
/// choosing which recent blocks its parents include; shallow enough that a round's winners are
/// known only about this many rounds ahead.
pub const PALW_EXEC_SEED_LAG_DAA: u64 = 20;

/// How many recently accepted attempts the PALW state keeps for the seed (ADR-0125 §7): enough that
/// several attempts landing inside one lag still leave a record deep enough to read.
pub const PALW_EXEC_SEED_RING_V1: usize = 4;

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

/// One accepted attempt, as the seed needs it: the accepting block's DAA, and the attempt's
/// `palw_attempt_v2::execution_commitment_v3` — the value its class ticket is a hash of.
///
/// That commitment is the right seed material because it is the one value on an attempt block a
/// producer cannot re-roll for free: the header nonce changes the block hash and never the
/// commitment (ADR-0072 made the nonce a uniqueness field), so moving the seed costs another
/// inference and another lottery win.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecutionSeedRecordV1 {
    pub accepted_daa: u64,
    pub execution_commitment: Hash64,
}

/// **The seed source of a round: the newest recorded attempt at least
/// [`PALW_EXEC_SEED_LAG_DAA`] below the candidate's selected parent.** `records` is the state's
/// ring in the order the fold appended it (newest last). `None` when no record is deep enough —
/// a fresh chain, or several attempts inside one lag — and a round without a seed has no permits,
/// which is an empty round, not a stall: the attempt lane does not wait for it.
pub fn palw_execution_seed_source_v1(records: &[PalwExecutionSeedRecordV1], parent_daa: u64) -> Option<Hash64> {
    records
        .iter()
        .rev()
        .find(|record| record.accepted_daa.checked_add(PALW_EXEC_SEED_LAG_DAA).is_some_and(|deep_at| deep_at <= parent_daa))
        .map(|record| record.execution_commitment)
}

/// `H(domain ‖ source ‖ round)`, with `source` from [`palw_execution_seed_source_v1`].
pub fn palw_execution_seed_v1(source: &Hash64, round: u64) -> Hash64 {
    let mut h = keyed(PALW_EXEC_ROUND_SEED_DOMAIN);
    h.update(source.as_byte_slice());
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

/// **Quotas from credits: proportional, capped at 45 %, renormalised — in integers.**
///
/// `credits` is each domain's `Final` attempt count in the previous scheduler epoch (ADR-0107's
/// count). A domain with no credits has no quota and is not listed. Among the rest:
///
/// * one domain holds the whole lane (there is nobody to cap it against);
/// * two domains split it evenly — no split of 1000‰ between two keeps both under 450‰, and the
///   even split is the one that minimises the larger;
/// * three or more are water-filled: a domain whose proportional share of what remains exceeds the
///   cap is fixed at the cap, and the rest is re-divided among the others until nobody exceeds it
///   (three domains at the cap already exceed 1000‰, so this always ends feasible).
///
/// Shares are exact rationals until the last step, then floored, and the missing permille go one
/// each to the largest remainders, ties to the earlier domain in `credits` order. The result sums
/// to exactly 1000 and is listed in `credits` order.
pub fn palw_execution_quotas_v1(credits: &[(Hash64, u64)]) -> Vec<(Hash64, u16)> {
    let live: Vec<(Hash64, u64)> = credits.iter().copied().filter(|(_, c)| *c > 0).collect();
    match live.len() {
        0 => return Vec::new(),
        1 => return vec![(live[0].0, 1000)],
        2 => return vec![(live[0].0, 500), (live[1].0, 500)],
        _ => {}
    }
    let cap = PALW_EXEC_DOMAIN_CAP_PERMILLE as u128;
    let mut capped = vec![false; live.len()];
    loop {
        let fixed = capped.iter().filter(|c| **c).count() as u128;
        let remaining = 1000u128 - fixed * cap;
        let total: u128 = live.iter().zip(&capped).filter(|(_, c)| !**c).map(|((_, credit), _)| *credit as u128).sum();
        // `remaining × credit / total > cap`, compared without division.
        let newly: Vec<usize> = live
            .iter()
            .enumerate()
            .filter(|(i, (_, credit))| !capped[*i] && remaining * (*credit as u128) > cap * total)
            .map(|(i, _)| i)
            .collect();
        if newly.is_empty() {
            let mut rows: Vec<(usize, u128, u128)> = live
                .iter()
                .enumerate()
                .map(|(i, (_, credit))| {
                    if capped[i] {
                        (i, cap, 0)
                    } else {
                        let scaled = remaining * (*credit as u128);
                        (i, scaled / total, scaled % total)
                    }
                })
                .collect();
            let assigned: u128 = rows.iter().map(|(_, q, _)| *q).sum();
            let mut missing = 1000u128 - assigned;
            // Every uncapped remainder shares the denominator `total`, so remainders compare
            // directly; capped rows carry remainder 0 and are never topped up above the cap.
            let mut order: Vec<usize> = (0..rows.len()).collect();
            order.sort_by(|a, b| rows[*b].2.cmp(&rows[*a].2).then(a.cmp(b)));
            for i in order {
                if missing == 0 {
                    break;
                }
                if !capped[rows[i].0] {
                    rows[i].1 += 1;
                    missing -= 1;
                }
            }
            return rows.into_iter().map(|(i, q, _)| (live[i].0, q as u16)).collect();
        }
        for i in newly {
            capped[i] = true;
        }
    }
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

    fn record(accepted_daa: u64, commitment: u64) -> PalwExecutionSeedRecordV1 {
        PalwExecutionSeedRecordV1 { accepted_daa, execution_commitment: h(commitment) }
    }

    /// **The seed is an attempt's execution commitment, read at a lag — never a beacon, never a
    /// header hash.** The newest record deep enough is the source; a record inside the lag is not
    /// read yet; nothing deep enough is no seed (an empty round); and the round separates seeds.
    #[test]
    fn the_seed_is_the_newest_attempt_commitment_at_least_a_lag_deep() {
        let ring = [record(100, 1), record(220, 2), record(340, 3), record(350, 4)];
        assert_eq!(palw_execution_seed_source_v1(&ring, 369), Some(h(3)), "350 + 20 > 369: the newest is not deep yet");
        assert_eq!(palw_execution_seed_source_v1(&ring, 370), Some(h(4)), "350 + 20 == 370: now it is");
        assert_eq!(palw_execution_seed_source_v1(&ring, 250), Some(h(2)));
        assert_eq!(palw_execution_seed_source_v1(&ring, 119), None, "a fresh chain has no seed: an empty round");
        assert_eq!(palw_execution_seed_source_v1(&[], 1_000), None);
        assert_eq!(palw_execution_seed_source_v1(&[record(u64::MAX, 9)], u64::MAX), None, "an overflowing depth is never deep");
        assert_eq!(PALW_EXEC_SEED_RING_V1, 4);

        let source = h(7);
        assert_ne!(palw_execution_seed_v1(&source, 1), palw_execution_seed_v1(&source, 2));
        assert_ne!(palw_execution_seed_v1(&source, 1), palw_execution_seed_v1(&h(8), 1));
        assert_eq!(palw_execution_seed_v1(&source, 1), palw_execution_seed_v1(&source, 1));
    }

    #[test]
    fn a_round_is_a_second_from_genesis() {
        assert_eq!(palw_execution_round_v1(1_000_000, 1_000_000), 0);
        assert_eq!(palw_execution_round_v1(1_000_999, 1_000_000), 0);
        assert_eq!(palw_execution_round_v1(1_001_000, 1_000_000), 1);
        assert_eq!(palw_execution_round_v1(0, 1_000_000), 0, "before genesis is round 0, never a wrap");
        assert_eq!(palw_execution_round_v1(1_000_000 + 120_000, 1_000_000), 120, "120 rounds a PALW cadence");
    }

    #[test]
    fn quotas_follow_credits_up_to_the_cap_in_integers_and_sum_to_a_thousand() {
        assert!(palw_execution_quotas_v1(&[]).is_empty());
        assert!(palw_execution_quotas_v1(&[(h(1), 0)]).is_empty(), "no credits, no quota");
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 5)]), vec![(h(1), 1000)], "one domain holds the lane");
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 99), (h(2), 1)]), vec![(h(1), 500), (h(2), 500)], "two split evenly");
        assert_eq!(
            palw_execution_quotas_v1(&[(h(1), 5), (h(2), 0), (h(3), 0)]),
            vec![(h(1), 1000)],
            "zero-credit domains are not listed"
        );
        // 70 / 20 / 10: the 70 is capped at 450 and 550 is re-divided 2 : 1 — 366.67 and 183.33,
        // floored to 366 and 183, and the one missing permille goes to the larger remainder.
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 70), (h(2), 20), (h(3), 10)]), vec![(h(1), 450), (h(2), 367), (h(3), 183)]);
        // Under the cap nothing moves.
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 40), (h(2), 35), (h(3), 25)]), vec![(h(1), 400), (h(2), 350), (h(3), 250)]);
        // Equal remainders: the earlier domain takes the missing permille.
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 1), (h(2), 1), (h(3), 1)]), vec![(h(1), 334), (h(2), 333), (h(3), 333)]);
        // Two whales and a minnow: both whales capped, the minnow takes the rest.
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 49), (h(2), 49), (h(3), 2)]), vec![(h(1), 450), (h(2), 450), (h(3), 100)]);
        // A cascade: 60 / 39 / 1. Pass one caps the 60; the 39 is then 550 × 39 / 40 = 536 > 450,
        // so pass two caps it too, and the 1 takes the last 100.
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 60), (h(2), 39), (h(3), 1)]), vec![(h(1), 450), (h(2), 450), (h(3), 100)]);
        // Rounding after a cap: 60 / 30 / 5 / 5 → 450, then 550 split 30 : 5 : 5 = 412.5, 68.75,
        // 68.75; floors sum to 998 and the two missing permille go to the two largest remainders.
        assert_eq!(
            palw_execution_quotas_v1(&[(h(1), 60), (h(2), 30), (h(3), 5), (h(4), 5)]),
            vec![(h(1), 450), (h(2), 412), (h(3), 69), (h(4), 69)]
        );
        for census in [
            vec![(h(1), 1), (h(2), 1), (h(3), 1)],
            vec![(h(1), 3), (h(2), 3), (h(3), 3), (h(4), 1)],
            vec![(h(1), 1_000_000), (h(2), 7), (h(3), 3), (h(4), 1)],
            vec![(h(1), u64::MAX), (h(2), u64::MAX), (h(3), u64::MAX)],
        ] {
            let quotas = palw_execution_quotas_v1(&census);
            let sum: u32 = quotas.iter().map(|(_, p)| *p as u32).sum();
            assert_eq!(sum, 1000, "{census:?}");
            assert!(quotas.iter().all(|(_, p)| (*p as u64) <= PALW_EXEC_DOMAIN_CAP_PERMILLE), "{census:?} breaches the cap");
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
        // two operators) can hold at most two of them under the operator rule.
        assert_eq!(permits.len(), 5);
        let operators: std::collections::BTreeSet<Hash64> = permits.iter().map(|p| p.candidate.operator_id).collect();
        assert_eq!(operators.len(), 5, "one permit an operator");
        assert_eq!(permits.iter().filter(|p| p.candidate.domain == h(1)).count(), 2);
        for (i, permit) in permits.iter().enumerate() {
            assert_eq!(permit.index as usize, i, "the order is the ticket order and the index says so");
        }
        let mut shuffled = candidates.clone();
        shuffled.reverse();
        assert_eq!(palw_execution_permits_v1(&seed, &shuffled, 10, &[]), permits, "deterministic and order-independent");

        // Width 1: one permit, and the domain that held it rests next round.
        let first = palw_execution_permits_v1(&seed, &candidates, 1, &[]);
        assert_eq!(first.len(), 1);
        let held = first[0].candidate.domain;
        let second = palw_execution_permits_v1(&palw_execution_seed_v1(&h(9), 43), &candidates, 1, &first);
        assert_eq!(second.len(), 1);
        assert_ne!(second[0].candidate.domain, held, "no two consecutive rounds from one domain");
        // Width 3, cap 1: three domains, one each; every domain filled its cap, so all rest.
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
    fn a_dominant_domain_holds_at_most_half_a_run_at_width_ten_and_the_count_is_all_that_widens() {
        // Nine operators in domain 1, one each in domains 2 and 3.
        let mut candidates: Vec<PalwExecutionCandidateV1> = (1..=9).map(|i| cand(i, 100 + i, 1)).collect();
        candidates.push(cand(10, 200, 2));
        candidates.push(cand(11, 300, 3));
        let mut previous = Vec::new();
        let mut by_domain: std::collections::BTreeMap<Hash64, usize> = std::collections::BTreeMap::new();
        let mut total = 0usize;
        for round in 0..200u64 {
            let permits = palw_execution_permits_v1(&palw_execution_seed_v1(&h(5), round), &candidates, 10, &previous);
            for permit in &permits {
                *by_domain.entry(permit.candidate.domain).or_insert(0) += 1;
            }
            total += permits.len();
            previous = permits;
        }
        assert!(total > 0);
        assert!(
            by_domain[&h(1)] * 2 <= total,
            "domain 1 holds {} of {total}: capped at four a round and rested after",
            by_domain[&h(1)]
        );
        assert!(by_domain[&h(2)] > 0 && by_domain[&h(3)] > 0, "the small domains are never starved");
        // Widening changes the count and nothing else: width 1's permit is the head of width 10's.
        let seed = palw_execution_seed_v1(&h(5), 7);
        let one = palw_execution_permits_v1(&seed, &candidates, 1, &[]);
        let ten = palw_execution_permits_v1(&seed, &candidates, 10, &[]);
        assert_eq!(one[0].candidate, ten[0].candidate);
        assert_eq!(PALW_EXEC_PERMITS_PER_ROUND_V1, 1, "stage 1 is one permit a round");
    }

    /// **Integer only.** A consensus quota two platforms round differently is a fork, so no
    /// floating-point type may be spelled in this file's code.
    #[test]
    fn no_floating_point_type_is_spelled_in_this_file() {
        let source = include_str!("palw_execution_lane_v1.rs");
        let code: String = source.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        for token in [concat!("f", "32"), concat!("f", "64")] {
            assert!(
                !code.split(|c: char| !c.is_alphanumeric() && c != '_').any(|word| word == token),
                "{token} is spelled in this file"
            );
        }
    }
}
