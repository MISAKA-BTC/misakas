//! **ADR-0100 Decision 4 — licensing per shard: the object, the quorum and the progress, as far as
//! a pure function goes.**
//!
//! A claim on a sharded class is judged by a panel of `shards × seats_per_shard` seats
//! (`palw_shard_panel_v1`), and its licensing carries `shards × quorum` receipts. One
//! `ReceiptLicensed` object carrying all of them fits a standard transaction up to eight shards
//! (ADR-0098 §1.2: 114,597 bytes at eight, 128,913 at nine, against 120,000) — and a network whose
//! seats hold shards of a K3-class model needs twenty-three (ADR-0099 §1.2). So past eight the
//! licensing is SPLIT: one part per shard, each carrying that shard's quorum, each fitting one
//! carrier whatever the shard count; the claim licenses in the block that lands the last part.
//!
//! This module is the pure half: the part ([`PalwShardReceiptPartV1`]), its wire size, the
//! per-shard quorum over already-verified verdicts ([`palw_shard_quorum_v1`]) and the progress a
//! claim keeps ([`PalwShardLicensingProgressV1`]). The consensus half — the object variant, the
//! acceptance arm that verifies each receipt under the seat bond's key, the stratified panel the
//! chain derives and stores, the claim record's progress field and the fold that licenses on the
//! last part, and the sweep that redraws a shard short of a quorum — is ADR-0100 §6's stated
//! next step, because a claim record that gains a field moves `PALW_STATE_V2_VERSION` and the
//! identity, which is a flag day the operator calls, not a branch. Nothing here is read by any
//! fold.

use crate::Hash64;
use crate::palw_economic_locus_v1::PALW_SEAT_RECEIPT_VALID_WIRE_BYTES_V1;
use crate::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwPanelSeatV2};

/// The most shards a plan may have and still be licensed per shard: the progress bitmap's width.
/// Ninety-two layers is the widest class this tree has priced (ADR-0099 §1.2), and a shard holds
/// at least one layer, so 1,024 is a ceiling nothing reaches.
pub const PALW_SHARD_LICENSING_MAX_SHARDS_V1: u32 = 1_024;

/// **One shard's licensing part**: the claim, which shard of which plan, and that shard's
/// receipts. The payload the future `ShardReceiptLicensed` variant carries.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwShardReceiptPartV1 {
    pub claim: Hash64,
    pub shard_count: u32,
    pub shard_index: u32,
    pub receipts: Vec<PalwSeatReceiptV2>,
}

/// The borsh wire size of a part carrying `receipts` Valid receipts: the object's enum tag (1),
/// `claim` (64), `shard_count` (4), `shard_index` (4), the vector's length prefix (4), the
/// receipts — the shape `palw_receipt_licensed_wire_bytes_v1` prices, plus the two shard fields.
pub const fn palw_shard_receipt_part_wire_bytes_v1(receipts: u64) -> u64 {
    1 + 64 + 4 + 4 + 4 + receipts * PALW_SEAT_RECEIPT_VALID_WIRE_BYTES_V1
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwShardLicensingError {
    #[error("a plan of zero shards")]
    NoShards,
    #[error("shard {shard} of a {count}-shard plan")]
    ShardOutOfRange { shard: u32, count: u32 },
    #[error("{count} shards is past the {max} this progress can track")]
    TooManyShards { count: u32, max: u32 },
    #[error("bond {0:?} is not a seat of this shard")]
    NotASeatOfThisShard(PalwBondKeyV2),
    #[error("seat {0:?} answered twice")]
    SeatAnsweredTwice(PalwBondKeyV2),
    #[error("a quorum of {quorum} over {seats} seats is not a majority")]
    QuorumNotAMajority { quorum: u16, seats: usize },
    #[error("the progress is for {progress} shards and the part names {part}")]
    PartOfAnotherPlan { progress: u32, part: u32 },
}

/// What one shard's receipts amount to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwShardQuorumV1 {
    /// `quorum` seats of the shard said Valid: this shard licenses.
    Licensed { valid: u16 },
    /// `quorum` seats said the producer was unavailable: this shard defaults the producer, which
    /// defaults the claim whatever the other shards found.
    ProducerUnavailable { unavailable: u16 },
    /// Neither verdict reached the quorum yet.
    Short { valid: u16, unavailable: u16, needed: u16 },
}

/// **The quorum of one shard**, over verdicts the acceptance layer has already verified under
/// the seat bonds' keys: every verdict must be a seat of THIS shard's slice of the panel, each
/// seat at most once; `Incapable` counts for neither side (ADR-0065 D4's abstention); a majority
/// quorum makes the two outcomes disjoint, and the caller's `PalwPanelParamsV2` already refuses a
/// non-majority — refused again here so the function stands alone.
pub fn palw_shard_quorum_v1(
    seats: &[PalwPanelSeatV2],
    verdicts: &[(PalwBondKeyV2, PalwReceiptVerdictV2)],
    quorum: u16,
) -> Result<PalwShardQuorumV1, PalwShardLicensingError> {
    if seats.is_empty() || usize::from(quorum) * 2 <= seats.len() {
        return Err(PalwShardLicensingError::QuorumNotAMajority { quorum, seats: seats.len() });
    }
    let mut answered: Vec<PalwBondKeyV2> = Vec::with_capacity(verdicts.len());
    let (mut valid, mut unavailable) = (0u16, 0u16);
    for (bond, verdict) in verdicts {
        if !seats.iter().any(|s| s.bond == *bond) {
            return Err(PalwShardLicensingError::NotASeatOfThisShard(*bond));
        }
        if answered.contains(bond) {
            return Err(PalwShardLicensingError::SeatAnsweredTwice(*bond));
        }
        answered.push(*bond);
        match verdict {
            PalwReceiptVerdictV2::Valid => valid += 1,
            PalwReceiptVerdictV2::Unavailable { .. } => unavailable += 1,
            PalwReceiptVerdictV2::Incapable => {}
        }
    }
    Ok(if valid >= quorum {
        PalwShardQuorumV1::Licensed { valid }
    } else if unavailable >= quorum {
        PalwShardQuorumV1::ProducerUnavailable { unavailable }
    } else {
        PalwShardQuorumV1::Short { valid, unavailable, needed: quorum }
    })
}

/// **Which shards have licensed** — the field a claim record would carry, as a bitmap: sixteen
/// words cover [`PALW_SHARD_LICENSING_MAX_SHARDS_V1`]. A part landing twice is not an error and
/// not a change (the second is a duplicate carrier, refused upstream); the claim licenses when
/// every shard's bit is set.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwShardLicensingProgressV1 {
    pub shard_count: u32,
    pub licensed: Vec<u64>,
}

impl PalwShardLicensingProgressV1 {
    pub fn new(shard_count: u32) -> Result<Self, PalwShardLicensingError> {
        if shard_count == 0 {
            return Err(PalwShardLicensingError::NoShards);
        }
        if shard_count > PALW_SHARD_LICENSING_MAX_SHARDS_V1 {
            return Err(PalwShardLicensingError::TooManyShards { count: shard_count, max: PALW_SHARD_LICENSING_MAX_SHARDS_V1 });
        }
        Ok(Self { shard_count, licensed: vec![0; shard_count.div_ceil(64) as usize] })
    }

    /// Records shard `shard`'s licensing; `Ok(true)` the first time, `Ok(false)` for a repeat.
    pub fn mark(&mut self, shard: u32) -> Result<bool, PalwShardLicensingError> {
        if shard >= self.shard_count {
            return Err(PalwShardLicensingError::ShardOutOfRange { shard, count: self.shard_count });
        }
        let (word, bit) = ((shard / 64) as usize, shard % 64);
        let fresh = self.licensed[word] & (1u64 << bit) == 0;
        self.licensed[word] |= 1u64 << bit;
        Ok(fresh)
    }

    /// Applies a part: the part must be of this plan; the caller has already found its quorum.
    pub fn apply(&mut self, part: &PalwShardReceiptPartV1) -> Result<bool, PalwShardLicensingError> {
        if part.shard_count != self.shard_count {
            return Err(PalwShardLicensingError::PartOfAnotherPlan { progress: self.shard_count, part: part.shard_count });
        }
        self.mark(part.shard_index)
    }

    pub fn is_licensed(&self, shard: u32) -> bool {
        shard < self.shard_count && self.licensed[(shard / 64) as usize] & (1u64 << (shard % 64)) != 0
    }

    pub fn licensed_count(&self) -> u32 {
        self.licensed.iter().map(|w| w.count_ones()).sum()
    }

    pub fn is_complete(&self) -> bool {
        self.licensed_count() == self.shard_count
    }
}

// =================================================================================================
// ADR-0100 Decision 4, the consensus half: the plan a class declares, the shards a bond holds, and
// which claims license by parts.
// =================================================================================================

/// **A class's shard plan, declared once by its registrant** (`ClassShardPlanDeclared`). Immutable
/// like the class's graph: a claim bound after the declaration draws a stratified panel of
/// `shard_count × seats_per_shard` seats and licenses by parts; one bound before keeps its flat
/// panel and the whole-object licence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassShardPlanV1 {
    pub shard_count: u32,
    pub declared_daa: u64,
}

/// A plan has at least two shards: one shard is the flat panel, which needs no plan.
pub const PALW_SHARD_PLAN_MIN_SHARDS_V1: u32 = 2;
/// The most receipts one part may carry — a stateless bound for the ride list; the acceptance
/// layer applies the ruleset's own seats per shard.
pub const PALW_SHARD_PART_MAX_RECEIPTS_V1: usize = 16;

pub const PALW_SHARD_PLAN_DOMAIN_MESSAGE_V1: &[u8] = b"misaka-palw/shard-plan/message/v1";
/// The registrant's ML-DSA-87 context for a plan. In `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`.
pub const PALW_SHARD_PLAN_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/shard-plan/mldsa87/v1";
pub const PALW_BOND_SHARDS_DOMAIN_MESSAGE_V1: &[u8] = b"misaka-palw/bond-shards/message/v1";
/// The bond's ML-DSA-87 context for its shard list. In `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`.
pub const PALW_BOND_SHARDS_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/bond-shards/mldsa87/v1";
/// Every keyed domain this family uses.
pub const PALW_SHARD_LICENSING_ALL_DOMAINS: &[&[u8]] = &[
    PALW_SHARD_PLAN_DOMAIN_MESSAGE_V1,
    PALW_SHARD_PLAN_MLDSA87_CONTEXT,
    PALW_BOND_SHARDS_DOMAIN_MESSAGE_V1,
    PALW_BOND_SHARDS_MLDSA87_CONTEXT,
];

/// Whether a plan of `count` shards is one the chain accepts.
pub fn palw_shard_count_in_range_v1(count: u32) -> bool {
    (PALW_SHARD_PLAN_MIN_SHARDS_V1..=PALW_SHARD_LICENSING_MAX_SHARDS_V1).contains(&count)
}

/// The shape of a bond's shard list: within the plan, strictly ascending, no longer than the plan.
/// An EMPTY list is well-formed and withdraws the bond's shards of that class.
pub fn palw_bond_shards_shape_v1(shard_count: u32, shards: &[u32]) -> Result<(), &'static str> {
    if !palw_shard_count_in_range_v1(shard_count) {
        return Err("the shard count is outside the plan range");
    }
    if shards.len() > shard_count as usize {
        return Err("more shards than the plan has");
    }
    if shards.iter().any(|s| *s >= shard_count) {
        return Err("a shard index past the plan");
    }
    if shards.windows(2).any(|w| w[0] >= w[1]) {
        return Err("the shards are not strictly ascending");
    }
    Ok(())
}

fn keyed_message(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish_message(state: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// What the registrant signs to declare a plan: the network domain, the class, the count.
pub fn palw_class_shard_plan_message_v1(network_domain: Hash64, class_id: &Hash64, shard_count: u32) -> Hash64 {
    let mut s = keyed_message(PALW_SHARD_PLAN_DOMAIN_MESSAGE_V1);
    s.update(network_domain.as_byte_slice());
    s.update(class_id.as_byte_slice());
    s.update(&shard_count.to_le_bytes());
    finish_message(s)
}

/// What a bond signs to declare its shards of a class: the network domain, the bond, the class,
/// the plan's count and the list.
pub fn palw_bond_shards_message_v1(
    network_domain: Hash64,
    bond: &PalwBondKeyV2,
    class_id: &Hash64,
    shard_count: u32,
    shards: &[u32],
) -> Hash64 {
    let mut s = keyed_message(PALW_BOND_SHARDS_DOMAIN_MESSAGE_V1);
    s.update(network_domain.as_byte_slice());
    s.update(&borsh::to_vec(bond).expect("a bond key is borsh-serializable"));
    s.update(class_id.as_byte_slice());
    s.update(&shard_count.to_le_bytes());
    s.update(&(shards.len() as u32).to_le_bytes());
    for shard in shards {
        s.update(&shard.to_le_bytes());
    }
    finish_message(s)
}

/// What the fold needs from the panel parameters, which it does not hold: the seats and the quorum
/// of one shard. Rides in `PalwTransitionExtrasV1::shard_licensing`, `Some` exactly when
/// `Params::palw_shard_licensing` is active at the block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwShardLicensingParamsV1 {
    pub seats_per_shard: u16,
    pub quorum_per_shard: u16,
}

/// **The seats that judge `shard`**: a stratified panel is shard-major, `seats_per_shard` each.
/// `None` when the panel is not `shard_count × seats_per_shard` seats — it was not drawn
/// stratified — or the shard is out of range.
pub fn palw_panel_shard_slice_v1(
    seats: &[PalwPanelSeatV2],
    shard_count: u32,
    seats_per_shard: u16,
    shard: u32,
) -> Option<&[PalwPanelSeatV2]> {
    let spp = usize::from(seats_per_shard);
    if spp == 0 || shard >= shard_count || seats.len() != (shard_count as usize).checked_mul(spp)? {
        return None;
    }
    let start = shard as usize * spp;
    seats.get(start..start + spp)
}

/// **Whether a claim licenses by parts**: its class declared a plan and the panel bound to it has
/// the stratified shape. A flat panel is `seats_per_shard` seats and a stratified one at least
/// twice that, so the shape alone tells them apart — no height comparison, and a claim bound
/// before its class's declaration keeps the whole-object licence it was drawn for.
pub fn palw_claim_licenses_by_parts_v1(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    seats_per_shard: u16,
) -> Option<PalwClassShardPlanV1> {
    let claim = state.claim(claim_id)?;
    let plan = state.class_shard_plan(&claim.class_id)?;
    let panel = state.panel(claim_id)?;
    (panel.seats.len() == (plan.shard_count as usize).checked_mul(usize::from(seats_per_shard))?).then_some(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_mode_v2::PALW_STANDARD_TX_BYTES;
    use crate::tx::TransactionOutpoint;

    fn bond(v: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([v; 64]), 0))
    }
    fn seats(from: u8, n: u8) -> Vec<PalwPanelSeatV2> {
        (from..from + n).map(|v| PalwPanelSeatV2 { bond: bond(v), operator_id: Hash64::from_u64_word(u64::from(v)) }).collect()
    }
    fn unavailable() -> PalwReceiptVerdictV2 {
        PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: 0 }
    }

    /// One part fits one standard transaction at the shipped quorum whatever the shard count —
    /// the reason the licensing is split — and the whole-object form does not past eight.
    #[test]
    fn a_part_fits_one_transaction_at_every_shard_count() {
        assert_eq!(palw_shard_receipt_part_wire_bytes_v1(3), 14_393, "3 receipts + 8 bytes of shard fields over the whole form");
        assert!(palw_shard_receipt_part_wire_bytes_v1(3) <= PALW_STANDARD_TX_BYTES);
        assert!(palw_shard_receipt_part_wire_bytes_v1(5) <= PALW_STANDARD_TX_BYTES, "even a full shard's five");
        assert!(
            crate::palw_economic_locus_v1::palw_receipt_licensed_wire_bytes_v1(3 * 9) > PALW_STANDARD_TX_BYTES,
            "nine shards whole"
        );
    }

    #[test]
    fn a_shards_quorum_counts_only_its_own_seats_once_each_and_incapable_for_neither() {
        let s = seats(1, 5);
        let v = |bonds: &[u8], verdict: PalwReceiptVerdictV2| -> Vec<(PalwBondKeyV2, PalwReceiptVerdictV2)> {
            bonds.iter().map(|b| (bond(*b), verdict)).collect()
        };
        assert_eq!(
            palw_shard_quorum_v1(&s, &v(&[1, 2, 3], PalwReceiptVerdictV2::Valid), 3),
            Ok(PalwShardQuorumV1::Licensed { valid: 3 })
        );
        assert_eq!(
            palw_shard_quorum_v1(&s, &v(&[1, 2], PalwReceiptVerdictV2::Valid), 3),
            Ok(PalwShardQuorumV1::Short { valid: 2, unavailable: 0, needed: 3 })
        );
        assert_eq!(
            palw_shard_quorum_v1(&s, &v(&[1, 2, 3], unavailable()), 3),
            Ok(PalwShardQuorumV1::ProducerUnavailable { unavailable: 3 })
        );
        let mut mixed = v(&[1, 2], PalwReceiptVerdictV2::Valid);
        mixed.extend(v(&[3, 4, 5], PalwReceiptVerdictV2::Incapable));
        assert_eq!(
            palw_shard_quorum_v1(&s, &mixed, 3),
            Ok(PalwShardQuorumV1::Short { valid: 2, unavailable: 0, needed: 3 }),
            "three Incapable seats license nothing and default nobody"
        );
        assert_eq!(
            palw_shard_quorum_v1(&s, &v(&[1, 2, 9], PalwReceiptVerdictV2::Valid), 3),
            Err(PalwShardLicensingError::NotASeatOfThisShard(bond(9))),
            "another shard's seat"
        );
        assert_eq!(
            palw_shard_quorum_v1(&s, &v(&[1, 1, 2], PalwReceiptVerdictV2::Valid), 3),
            Err(PalwShardLicensingError::SeatAnsweredTwice(bond(1)))
        );
        assert_eq!(
            palw_shard_quorum_v1(&s, &[], 2),
            Err(PalwShardLicensingError::QuorumNotAMajority { quorum: 2, seats: 5 }),
            "a non-majority quorum could license and default at once"
        );
    }

    #[test]
    fn the_progress_completes_on_the_last_shard_and_names_a_foreign_part() {
        let mut p = PalwShardLicensingProgressV1::new(23).unwrap();
        assert_eq!(p.licensed.len(), 1);
        for shard in 0..22 {
            assert_eq!(p.mark(shard), Ok(true));
            assert!(!p.is_complete());
        }
        assert_eq!(p.mark(5), Ok(false), "a repeat is not a change");
        assert_eq!(p.licensed_count(), 22);
        let last = PalwShardReceiptPartV1 { claim: Hash64::from_u64_word(1), shard_count: 23, shard_index: 22, receipts: vec![] };
        assert_eq!(p.apply(&last), Ok(true));
        assert!(p.is_complete());
        assert!(p.is_licensed(22) && !PalwShardLicensingProgressV1::new(23).unwrap().is_licensed(22));
        let foreign = PalwShardReceiptPartV1 { shard_count: 24, ..last.clone() };
        assert_eq!(p.apply(&foreign), Err(PalwShardLicensingError::PartOfAnotherPlan { progress: 23, part: 24 }));
        assert_eq!(p.mark(23), Err(PalwShardLicensingError::ShardOutOfRange { shard: 23, count: 23 }));
        assert_eq!(PalwShardLicensingProgressV1::new(0), Err(PalwShardLicensingError::NoShards));
        assert!(matches!(PalwShardLicensingProgressV1::new(2_000), Err(PalwShardLicensingError::TooManyShards { .. })));
        let wide = PalwShardLicensingProgressV1::new(92).unwrap();
        assert_eq!(wide.licensed.len(), 2, "ninety-two shards take two words");
    }
}
