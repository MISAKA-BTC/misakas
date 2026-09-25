//! **ADR-0152 v3.1 T90 (SW-5, rewritten by SW-A4): the admission jury stays ADR-0147's, past the
//! fence and below it.** The stake-weighted draw (SW-1…SW-10) weighs exactly two draws — the flat
//! class draw and ADR-0147's outsider seat — and never the jury that admits a `Candidate` class.
//!
//! What this pins, on the one jury function every node runs ([`palw_admission_jury_v1`]) and on the
//! fold's one caller of it (`admission_jury_seated`, `palw_state_v2.rs`):
//!
//! * **The jury is the operator ticket order, nothing else.** Over a corpus of synthetic populations
//!   (1–3 bonds per operator, collateral from the 13,000 MSK producer floor to 20M MSK, past
//!   SW-2's 1,000,000 MSK cap), the jury is the `seats` operators with the lowest
//!   `H(jury domain ‖ seed ‖ operator_id)`, recomputed here independently of the function.
//! * **No stake moves a juror.** Re-posting every bond's collateral (to the floor, to the cap, far
//!   past it), splitting an operator across more bonds, and re-listing the population in reverse
//!   leave every jury byte-identical — while the stake race over the SAME populations does move
//!   (the sensitivity control: a weighted draw would have failed this test).
//! * **The fold's jury reads neither the stake nor the fence.** `admission_jury_seated` calls
//!   `palw_admission_jury_v1` and reads no `stake` policy, no `palw_panel_draw_policy_at` and no
//!   `palw_rcore_plus` activation, so the jury testnet-12 seats with `palw_rcore_plus` armed is the
//!   jury its fence-off twin seats on the same state (the function's signature takes no policy).
//!
//! **The kept residual (documented, not asserted as a rate — SW-A4):** an unweighted jury is bought
//! with operator count, not stake. The audit measured that 40 registrant Sybils of 13,000 MSK each
//! win a jury majority with probability 0.9728 against testnet-12's genesis operators; a class bought
//! that way still needs the stake-weighted outsider's `Valid` on each of its claims (T89), which is
//! where §4.3 prices the attack.
//!
//! Run: cargo test -p kaspa-consensus-core --test t90_admission_jury_is_unweighted

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
use kaspa_consensus_core::palw_panel_v2::{
    PALW_DRAW_WEIGHT_CAP_MSK_V1, PalwPanelStakeDrawV1, palw_admission_jury_ticket_v1, palw_admission_jury_v1,
    palw_panel_stake_race_of_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwBondStateV2, PalwBondStatusV2};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

/// The testnet-12 producer floor, whole MSK: the smallest bond a population holds.
const FLOOR: u64 = 13_000;
/// testnet-12's admission jury (ADR-0147): five seats.
const SEATS: u16 = 5;

/// SplitMix64: a deterministic corpus without a crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn operator(op: u64) -> Hash64 {
    Hash64::from_u64_word(0x7090_0000 + op)
}

fn bond(n: u64, op: u64, msk: u64) -> (PalwBondKeyV2, PalwBondStateV2) {
    (
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0x9000_0000 + n), index: 0 }),
        PalwBondStateV2 {
            pubkey: vec![n as u8; 4],
            operator_id: operator(op),
            collateral: msk * SOMPI_PER_KASPA,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: Hash64::from_u64_word(1),
            capable_classes: Default::default(),
        },
    )
}

fn refs(list: &[(PalwBondKeyV2, PalwBondStateV2)]) -> Vec<(&PalwBondKeyV2, &PalwBondStateV2)> {
    list.iter().map(|(k, b)| (k, b)).collect()
}

/// One corpus member: `operators` operators, each with 1–3 bonds of 13,000 MSK … 20M MSK.
fn population(rng: &mut Rng, operators: u64) -> Vec<(PalwBondKeyV2, PalwBondStateV2)> {
    let mut out = Vec::new();
    let mut n = 0;
    for op in 0..operators {
        for _ in 0..1 + rng.below(3) {
            let msk = FLOOR + rng.below(20_000_000 - FLOOR);
            out.push(bond(n, op, msk));
            n += 1;
        }
    }
    out
}

/// ADR-0147's jury, recomputed without the function under test: the distinct operators, ranked by
/// their jury ticket, the first `seats`.
fn jury_by_hand(seed: &Hash64, population: &[(PalwBondKeyV2, PalwBondStateV2)], seats: u16) -> Vec<Hash64> {
    let mut operators: Vec<Hash64> = population.iter().map(|(_, b)| b.operator_id).collect();
    operators.sort();
    operators.dedup();
    let mut ranked: Vec<(Hash64, Hash64)> = operators.into_iter().map(|op| (palw_admission_jury_ticket_v1(seed, &op), op)).collect();
    ranked.sort();
    ranked.into_iter().take(seats as usize).map(|(_, op)| op).collect()
}

/// **T90: the jury is ADR-0147's operator ticket order, and no stake moves a juror** — over 256
/// populations and four re-postings of each, while the stake race over the same populations moves.
#[test]
fn t90_the_admission_jury_is_the_operator_ticket_order_whatever_the_stake() {
    let stake = PalwPanelStakeDrawV1::V1;
    let mut rng = Rng(0x0090_7090_A5A5_0147);
    let mut race_moved = 0usize;
    let mut short_juries = 0usize;
    for member in 0..256u64 {
        let operators = 3 + rng.below(40);
        let pop = population(&mut rng, operators);
        let seed = Hash64::from_u64_word(0x5EED_0000 ^ member.wrapping_mul(0x9E37));
        let jury = palw_admission_jury_v1(&seed, &refs(&pop), SEATS);
        assert_eq!(jury, jury_by_hand(&seed, &pop, SEATS), "member {member}: the jury is the lowest operator tickets");
        let distinct: std::collections::BTreeSet<Hash64> = jury.iter().copied().collect();
        assert_eq!(distinct.len(), jury.len(), "member {member}: one seat per operator, whatever it holds");
        if (operators as usize) < SEATS as usize {
            assert_eq!(jury.len(), operators as usize, "member {member}: a short population is a short jury, returned short");
            short_juries += 1;
        }

        // Every re-posting of the same operators' stake: the floor, the cap, far past it, and a
        // re-draw of every amount.
        let reposted: Vec<Vec<(PalwBondKeyV2, PalwBondStateV2)>> = vec![
            pop.iter().map(|(k, b)| (*k, PalwBondStateV2 { collateral: FLOOR * SOMPI_PER_KASPA, ..b.clone() })).collect(),
            pop.iter()
                .map(|(k, b)| (*k, PalwBondStateV2 { collateral: PALW_DRAW_WEIGHT_CAP_MSK_V1 * SOMPI_PER_KASPA, ..b.clone() }))
                .collect(),
            pop.iter()
                .enumerate()
                .map(|(i, (k, b))| {
                    let msk = if i % 2 == 0 { 50 * PALW_DRAW_WEIGHT_CAP_MSK_V1 } else { FLOOR };
                    (*k, PalwBondStateV2 { collateral: msk * SOMPI_PER_KASPA, ..b.clone() })
                })
                .collect(),
            pop.iter()
                .map(|(k, b)| {
                    let msk = FLOOR + rng.below(20_000_000 - FLOOR);
                    (*k, PalwBondStateV2 { collateral: msk * SOMPI_PER_KASPA, ..b.clone() })
                })
                .collect(),
        ];
        for (r, other) in reposted.iter().enumerate() {
            assert_eq!(palw_admission_jury_v1(&seed, &refs(other), SEATS), jury, "member {member}, re-posting {r}: no juror moved");
        }
        // Splitting every operator's first bond into two (an operator buys no second entry) and
        // listing the population backwards.
        let mut split = pop.clone();
        let extra: Vec<_> = pop
            .iter()
            .enumerate()
            .map(|(i, (_, owner))| {
                let (k, b) = bond(1_000_000 + member * 1_000 + i as u64, 0, FLOOR);
                (k, PalwBondStateV2 { operator_id: owner.operator_id, ..b })
            })
            .collect();
        split.extend(extra);
        assert_eq!(palw_admission_jury_v1(&seed, &refs(&split), SEATS), jury, "member {member}: splitting buys no seat");
        let mut backwards = pop.clone();
        backwards.reverse();
        assert_eq!(palw_admission_jury_v1(&seed, &refs(&backwards), SEATS), jury, "member {member}: the listing order is not read");

        // The control: the stake race over the same populations, re-posted — a weighted draw moves.
        if operators as usize >= SEATS as usize {
            let claim = Hash64::from_u64_word(0x90C1_0000 + member);
            let anchor = BlockHash::from_u64_word(0x90A0_0000 + member);
            let race =
                |list: &[(PalwBondKeyV2, PalwBondStateV2)]| palw_panel_stake_race_of_v1(SEATS, &claim, anchor, &refs(list), &refs(list), &stake);
            let ops = |seats: Vec<kaspa_consensus_core::palw_state_v2::PalwPanelSeatV2>| {
                let mut ops: Vec<Hash64> = seats.into_iter().map(|s| s.operator_id).collect();
                ops.sort();
                ops
            };
            if let (Ok(a), Ok(b)) = (race(&pop), race(&reposted[2])) {
                if ops(a) != ops(b) {
                    race_moved += 1;
                }
            }
        }
    }
    println!("T90: 256 populations; the stake race moved under re-posting in {race_moved}; short juries {short_juries}");
    assert!(race_moved > 0, "the control: the weighted race is sensitive to the stake this test re-posts");
}

/// The body of `fn name(` in `source`, to its closing brace at the same indentation.
fn body_of<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source.find(signature).unwrap_or_else(|| panic!("`{signature}` is in the source"));
    let indent = source[..start].rsplit('\n').next().unwrap_or("").len();
    let close = format!("\n{}}}\n", " ".repeat(indent));
    let end = source[start..].find(&close).unwrap_or_else(|| panic!("`{signature}` closes")) + start;
    &source[start..end]
}

/// **T90, the fold's half: `admission_jury_seated` draws ADR-0147's jury and reads no stake and no
/// fence.** The jury the fold seats is `palw_admission_jury_v1` over the population it filters; the
/// body names no stake policy, no draw-policy resolver and no `palw_rcore_plus` activation, so the
/// jury is the same with the fence armed and with it off (its twin), on every state. And the jury
/// function itself reads each bond's operator id only — never its collateral.
#[test]
fn t90_the_folds_jury_reads_neither_the_stake_nor_the_fence() {
    let state = include_str!("../src/palw_state_v2.rs");
    let seated = body_of(state, "fn admission_jury_seated(");
    assert!(seated.contains("palw_admission_jury_v1("), "the fold seats ADR-0147's jury");
    for forbidden in ["stake", "palw_panel_draw_policy_at", "rcore_plus", "PalwPanelStakeDrawV1", "weight"] {
        assert!(!seated.contains(forbidden), "admission_jury_seated reads `{forbidden}`: the jury must stay unweighted (SW-A4)");
    }
    let panel = include_str!("../src/palw_panel_v2.rs");
    let jury = body_of(panel, "pub fn palw_admission_jury_v1(");
    assert!(jury.contains("operator_id"), "the jury ranks operators");
    for forbidden in ["collateral", "stake", "weight"] {
        assert!(!jury.contains(forbidden), "palw_admission_jury_v1 reads `{forbidden}`: the jury must stay unweighted (SW-A4)");
    }
}
