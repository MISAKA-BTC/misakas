//! **ADR-0099 Decision 3 — a panel stratified by shard, drawn with the panel's own ticket.**
//!
//! A bond that holds one shard of a class cannot judge the whole class, and sortition that ignored
//! which shard a bond holds would seat it on claims it cannot replay — the ADR-0071 SA-1 problem
//! one level down. So a shard is a CAPABILITY: a bond declares `palw_shard_capability_id_v1(class,
//! count, index)` in the `capable_classes` set it already carries (no new field, no new object),
//! and a claim on a sharded class draws `seats_per_shard` seats PER SHARD from the bonds that
//! declared that shard, each shard's draw an independent permutation of the same anchor and claim.
//!
//! The ticket is `derive_panel_v2`'s — `H(anchor ‖ claim ‖ bond)` under its own domain — with the
//! shard mixed in, so a bond holding two shards is drawn independently for each and the seat it
//! wins on one says nothing about the other. One seat per operator per shard, as today.
//!
//! ADR-0098 Decision 5 measured what this costs: the coverage of a one-leaf lie is the s = seats
//! per shard column at every shard count (only that shard's seats can replay it), and the
//! licensing object carries `quorum × shards` receipts — one standard transaction up to eight
//! shards at a quorum of three. Both are `palw_seat_coverage_v1::palw_sharded_panel_v1`'s.
//!
//! Pure: this module reads no chain state. The acceptance rule that would call it, and the
//! per-shard licensing it needs, are ADR-0099 §3's Decision 4 and are not built.

use crate::BlockHash;
use crate::Hash64;
use crate::palw_state_v2::{PalwBondKeyV2, PalwPanelSeatV2};

pub const PALW_SHARD_CAPABILITY_DOMAIN_V1: &[u8] = b"misaka-palw/shard-capability/v1";
pub const PALW_SHARD_PANEL_DOMAIN_SEAT_TICKET_V1: &[u8] = b"misaka-palw/shard-panel/seat-ticket/v1";

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **The capability a bond declares to hold shard `index` of `count` of `class`.** A different
/// count is a different plan and therefore a different capability: a bond that holds layers
/// 0–11 of a 4-shard plan holds nothing of an 8-shard plan.
pub fn palw_shard_capability_id_v1(class_id: &Hash64, shard_count: u32, shard_index: u32) -> Hash64 {
    let mut s = keyed(PALW_SHARD_CAPABILITY_DOMAIN_V1);
    s.update(class_id.as_byte_slice());
    s.update(&shard_count.to_le_bytes());
    s.update(&shard_index.to_le_bytes());
    finish(s)
}

/// A bond as the draw sees it: its key, its operator, and the shards it declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwShardCandidateV1 {
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
    pub shards: Vec<u32>,
}

/// A stratified panel: `seats[shard]` is that shard's seats, in ticket order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwShardPanelV1 {
    pub shard_count: u32,
    pub seats_per_shard: u16,
    pub quorum_per_shard: u16,
    pub seats: Vec<Vec<PalwPanelSeatV2>>,
}

impl PalwShardPanelV1 {
    /// Every seat of every shard, for a caller that hands out duties.
    pub fn all_seats(&self) -> impl Iterator<Item = (u32, &PalwPanelSeatV2)> + '_ {
        self.seats.iter().enumerate().flat_map(|(shard, seats)| seats.iter().map(move |s| (shard as u32, s)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwShardPanelError {
    #[error("a stratified panel needs at least one shard")]
    NoShards,
    #[error("a quorum of {quorum} does not decide a shard of {seats} seats: 2 × quorum must exceed the seats")]
    BadQuorum { seats: u16, quorum: u16 },
    #[error("shard {shard}: {needed} seats needed and {available} eligible bonds (one per operator)")]
    InsufficientEligibleBonds { shard: u32, needed: u16, available: u16 },
}

/// **The draw.** For each shard, the bonds that declared it, ticketed by
/// `H(anchor ‖ claim ‖ count ‖ shard ‖ bond)`, sorted, one seat per operator, the first
/// `seats_per_shard` win. A shard with too few eligible operators refuses the whole panel by
/// name — a claim whose shard cannot be judged is not licensable, and saying which shard is what
/// an operator acts on.
pub fn derive_shard_panel_v1(
    candidates: &[PalwShardCandidateV1],
    shard_count: u32,
    seats_per_shard: u16,
    quorum_per_shard: u16,
    claim_id: &Hash64,
    anchor_block: BlockHash,
) -> Result<PalwShardPanelV1, PalwShardPanelError> {
    if shard_count == 0 {
        return Err(PalwShardPanelError::NoShards);
    }
    if seats_per_shard == 0 || quorum_per_shard == 0 || u32::from(quorum_per_shard) * 2 <= u32::from(seats_per_shard) {
        return Err(PalwShardPanelError::BadQuorum { seats: seats_per_shard, quorum: quorum_per_shard });
    }
    let mut seats = Vec::with_capacity(shard_count as usize);
    for shard in 0..shard_count {
        let mut tickets: Vec<(Hash64, PalwBondKeyV2, Hash64)> = candidates
            .iter()
            .filter(|c| c.shards.contains(&shard))
            .map(|c| {
                let mut t = keyed(PALW_SHARD_PANEL_DOMAIN_SEAT_TICKET_V1);
                t.update(anchor_block.as_byte_slice());
                t.update(claim_id.as_byte_slice());
                t.update(&shard_count.to_le_bytes());
                t.update(&shard.to_le_bytes());
                t.update(&borsh::to_vec(&c.bond).expect("bond keys are borsh-serializable"));
                (finish(t), c.bond, c.operator_id)
            })
            .collect();
        tickets.sort();
        let mut drawn: Vec<PalwPanelSeatV2> = Vec::new();
        let mut operators: Vec<Hash64> = Vec::new();
        for (_, bond, operator_id) in tickets {
            if drawn.len() == usize::from(seats_per_shard) {
                break;
            }
            if operators.contains(&operator_id) {
                continue;
            }
            operators.push(operator_id);
            drawn.push(PalwPanelSeatV2 { bond, operator_id });
        }
        if drawn.len() < usize::from(seats_per_shard) {
            return Err(PalwShardPanelError::InsufficientEligibleBonds {
                shard,
                needed: seats_per_shard,
                available: drawn.len() as u16,
            });
        }
        seats.push(drawn);
    }
    Ok(PalwShardPanelV1 { shard_count, seats_per_shard, quorum_per_shard, seats })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn bond(v: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([v; 64]), 0))
    }

    fn candidates(per_shard: u8, shards: u32) -> Vec<PalwShardCandidateV1> {
        (0..shards)
            .flat_map(|s| {
                (0..per_shard).map(move |i| PalwShardCandidateV1 {
                    bond: bond((s as u8) * 32 + i + 1),
                    operator_id: Hash64::from_u64_word(u64::from(s) * 1_000 + u64::from(i)),
                    shards: vec![s],
                })
            })
            .collect()
    }

    #[test]
    fn a_capability_names_the_class_the_count_and_the_shard() {
        let class = Hash64::from_u64_word(9);
        let a = palw_shard_capability_id_v1(&class, 8, 3);
        assert_ne!(a, palw_shard_capability_id_v1(&class, 8, 4), "another shard");
        assert_ne!(a, palw_shard_capability_id_v1(&class, 16, 3), "another plan");
        assert_ne!(a, palw_shard_capability_id_v1(&Hash64::from_u64_word(10), 8, 3), "another class");
        assert_eq!(a, palw_shard_capability_id_v1(&class, 8, 3));
    }

    #[test]
    fn the_draw_seats_every_shard_one_operator_each_and_names_a_short_shard() {
        let claim = Hash64::from_u64_word(1);
        let anchor = Hash64::from_u64_word(2);
        let all = candidates(7, 4);
        let panel = derive_shard_panel_v1(&all, 4, 5, 3, &claim, anchor).expect("enough bonds");
        assert_eq!(panel.seats.len(), 4);
        for (shard, seats) in panel.seats.iter().enumerate() {
            assert_eq!(seats.len(), 5);
            let mut operators: Vec<Hash64> = seats.iter().map(|s| s.operator_id).collect();
            operators.sort_unstable_by_key(|a| a.as_bytes());
            operators.dedup();
            assert_eq!(operators.len(), 5, "one seat per operator on shard {shard}");
            // A seat holds only a shard its bond declared: every operator seated on this shard is
            // one that declared it, and none of another shard's.
            let declared: Vec<Hash64> = all.iter().filter(|c| c.shards == vec![shard as u32]).map(|c| c.operator_id).collect();
            assert!(seats.iter().all(|s| declared.contains(&s.operator_id)), "shard {shard} seats only bonds that declared it");
        }
        assert_eq!(panel.all_seats().count(), 20);
        // Deterministic.
        assert_eq!(panel, derive_shard_panel_v1(&all, 4, 5, 3, &claim, anchor).unwrap());
        // A different anchor is a different draw somewhere.
        let other = derive_shard_panel_v1(&all, 4, 5, 3, &claim, Hash64::from_u64_word(3)).unwrap();
        assert_ne!(panel, other);

        // Shard 2 cut to four operators of the five needed: refused, and it says which shard.
        let mut cut = all.clone();
        cut.retain(|c| c.shards != vec![2] || c.bond.0.transaction_id.as_bytes()[0] > 2 * 32 + 3);
        assert_eq!(cut.iter().filter(|c| c.shards == vec![2]).count(), 4);
        match derive_shard_panel_v1(&cut, 4, 5, 3, &claim, anchor) {
            Err(PalwShardPanelError::InsufficientEligibleBonds { shard: 2, needed: 5, available: 4 }) => {}
            other => panic!("shard 2 has four operators and must refuse by name, got {other:?}"),
        }
        // Two operators sharing a bond operator id count once.
        let mut twins = candidates(5, 1);
        twins[1].operator_id = twins[0].operator_id;
        match derive_shard_panel_v1(&twins, 1, 5, 3, &claim, anchor) {
            Err(PalwShardPanelError::InsufficientEligibleBonds { shard: 0, needed: 5, available: 4 }) => {}
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            derive_shard_panel_v1(&candidates(5, 1), 1, 4, 2, &claim, anchor),
            Err(PalwShardPanelError::BadQuorum { .. })
        ));
        assert!(matches!(derive_shard_panel_v1(&candidates(5, 1), 0, 5, 3, &claim, anchor), Err(PalwShardPanelError::NoShards)));
    }

    /// A bond holding every shard is drawn independently on each: its rank differs by shard, so
    /// the draw is a function of the shard and not only of the bond.
    #[test]
    fn a_bond_on_every_shard_ranks_differently_per_shard() {
        let mut all: Vec<PalwShardCandidateV1> = (1..=12u8)
            .map(|i| PalwShardCandidateV1 {
                bond: bond(i),
                operator_id: Hash64::from_u64_word(u64::from(i)),
                shards: (0..6).collect(),
            })
            .collect();
        all.sort_by_key(|c| c.bond.0.transaction_id.as_bytes()[0]);
        let panel = derive_shard_panel_v1(&all, 6, 5, 3, &Hash64::from_u64_word(7), Hash64::from_u64_word(8)).unwrap();
        let orders: Vec<Vec<PalwBondKeyV2>> = panel.seats.iter().map(|s| s.iter().map(|x| x.bond).collect()).collect();
        assert!(orders.windows(2).any(|w| w[0] != w[1]), "six shards, six permutations of twelve bonds — not all equal");
    }
}
