//! **Lane F02b (G14 / t12 DAA 17,000 slice): atomic paid court admission of a kernel-route Panel=1 original.**
//!
//! The slice (lane SPINE): the kernel route in `Mode::Panel`, Panel=1, the genesis class card 1 registers, a single-prompt decode
//! job (`ClaimBodyV1::Program`). When such a claim is committed, [`kernel_court_admission_candidate_v1`] derives, from the original
//! alone, the canonical court service packet of EVERY legal branch:
//!
//! * for each liable signer (the producer and every drawn seat, canonical order) and each turn `k < packets_per_signer`;
//! * turn 0 (the initial notice) has one branch; every follow-up turn `k >= 1` has `follow_up_branches_per_turn` adaptive
//!   branches (the challenger's choice at that turn). A packet binds `(turn, branch)`, not the path, so reserving every branch of
//!   every turn reserves the capacity of every path while staying linear in turns;
//! * branch 0 keeps the existing identity [`court_service_packet_id_v1`] (the F02 attribution's own-packet lookup is unchanged);
//!   branch `b >= 1` has its own keyed identity.
//!
//! and reserves, all-or-nothing, (a) every packet in the rooted court service book and (b) the court capital of every packet
//! (`packets x capital_per_packet_sompi`) from the producer's FREE collateral. Either both are possible or the candidate is refused
//! and the caller drops the object with nothing written. The capital reservation is held as a non-claim collateral lock under a
//! derived id ([`kernel_court_funding_lock_id_v1`]) that no claim has, so it counts in `palw_bond_committed_v1` until the original's
//! hard deadline and no conviction path can take it.
//!
//! **2026-10-26 (coordinator decision):** the capital is NOT a `slashable_locks` entry. It is a dedicated versioned row
//! [`KernelCourtCapitalRowV1`] in the kernel route's aux table [`PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1`], written only past the
//! slice's fence `palw_kernel_route_panel_one_v1`. No conviction or default path reads that table: the row is released (removed,
//! returning the collateral) exactly when the claim's binding leaves the live book (resolved-book release) or at its hard deadline.
//!
//! Values: production is [`PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1`] (estimate, lane G14-FIX SPEC-1), in force only past
//! `palw_kernel_route_panel_one_v1`; on every build where that fence is unarmed no admission happens.
//! [`KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1`] is built from lane VAL's PROVISIONAL ESTIMATES
//! (`palw_g14_values_v1`) for tests; `follow_up_branches_per_turn` has no VAL value and is a test-only 2 there.
//!
//! No cryptographic proof of computation is used (PRINCIPLES §8); no per-model verification (§9). Hashes are identities only.
use crate::{
    Hash64,
    palw_court_policy_v1::DISPUTE_WINDOW_DAA,
    palw_court_service_binding_v1::{CourtServiceProfileV1, court_service_packet_id_v1},
    palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding, court_service_profile_id_v2},
    palw_court_service_v1::{ServicePacket, ServiceRefusal, ServiceReservations},
    palw_state_v2::PalwBondKeyV2,
};
use std::collections::BTreeSet;

/// The schedule a kernel-route original is admitted under. Every field is a policy value; none is measured here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KernelCourtAdmissionScheduleV1 {
    pub version: u16,
    pub profile: CourtServiceProfileV1,
    /// Adaptive branches of each follow-up turn (turn 0 has exactly one).
    pub follow_up_branches_per_turn: u64,
    /// Court capital reserved per packet from the producer's free collateral (sompi).
    pub capital_per_packet_sompi: u64,
}

/// **The production schedule (lane G14-FIX SPEC-1, 2026-10-27): PROVISIONAL ESTIMATE (目安, user decision 2026-10-22), not measured.**
/// In force only past `palw_kernel_route_panel_one_v1` (testnet-12: the G14 list at DAA 17,400): the processor copies it into the
/// fold's extras only at or above that fence ([`crate::palw_state_v2::PalwStateParamsV2::kernel_court_admission_schedule_v1`]), and
/// the admission fold re-checks the same fence, so below it every block is byte-identical to before.
///
/// Basis — every field is an existing user-approved `palw_g14_values_v1` value; nothing new is decided here:
/// * `profile` = V1 [`crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1`] (12 turns/signer, span 833 DAA, 1
///   packet/DAA budget; already the production F02 profile `PALW_COURT_SERVICE_PROFILE_V1`);
/// * `follow_up_branches_per_turn` = [`crate::palw_g14_values_v1::PALW_G14_FOLLOW_UP_BRANCHES_PER_TURN_ESTIMATE_V1`] = 2 (approved
///   2026-10-26; Panel=1 reserves `2 × (1 + 11 × 2) = 46` packets, under `c·W = 9,999`);
/// * `capital_per_packet_sompi` = [`crate::palw_g14_values_v1::PALW_G14_CAPITAL_PER_TURN_SOMPI_V1`] = 3.4 M sompi (2 × one packet's
///   carriage), so one Panel=1 original reserves `46 × 0.034 = 1.564` MSK from the producer's FREE collateral — about 0.14 % of the
///   kernel per-claim collateral (1,080 → 1,100 MSK, lane ACT2), paid by the producer, separate from the claim's B = F = 0 and the
///   poster-paid escrow (no subsidy, no escrow clip).
///
/// Revisit when: the court's measured branching factor per turn differs from 2; the packet size / carriage rate / V1 profile
/// changes; or measured producer free collateral shows admissions refused for capital. Register: `docs/governance/parameter-register.md`
/// "2026-10-27 lane G14-FIX SPEC-1".
pub const PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1: Option<KernelCourtAdmissionScheduleV1> = Some(KernelCourtAdmissionScheduleV1 {
    version: 1,
    profile: crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1,
    follow_up_branches_per_turn: crate::palw_g14_values_v1::PALW_G14_FOLLOW_UP_BRANCHES_PER_TURN_ESTIMATE_V1,
    capital_per_packet_sompi: crate::palw_g14_values_v1::PALW_G14_CAPITAL_PER_TURN_SOMPI_V1,
});

/// TEST-ONLY follow-up fan-out (no value lane number exists): two adaptive branches per follow-up turn.
pub const TEST_ONLY_FOLLOW_UP_BRANCHES_PER_TURN_V1: u64 = 2;

impl KernelCourtAdmissionScheduleV1 {
    /// **TEST-ONLY**, from lane VAL's provisional estimates (V1 profile, capital per turn). Not a production value.
    pub fn test_only_g14_estimate_v1() -> Self {
        Self {
            version: 1,
            profile: crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1,
            follow_up_branches_per_turn: TEST_ONLY_FOLLOW_UP_BRANCHES_PER_TURN_V1,
            capital_per_packet_sompi: crate::palw_g14_values_v1::PALW_G14_CAPITAL_PER_TURN_SOMPI_V1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelCourtAdmissionRefusalV1 {
    ScheduleInvalid,
    NoLiableSigner,
    Overflow,
    ProfileExceedsHard,
    ProfileMismatch,
    AlreadyBound,
    Reservation(ServiceRefusal),
    /// The producer's free collateral cannot hold the court capital of every branch.
    InsufficientCapital { required: u128, free: u128 },
    Inconsistent,
}

fn keyed(key: &[u8], bytes: &[u8]) -> Hash64 {
    Hash64::from_bytes(
        blake2b_simd::Params::new().hash_length(64).key(key).hash(bytes).as_bytes().try_into().expect("64-byte digest"),
    )
}

/// Packet identity of `(signer, turn, branch)`; branch 0 is exactly [`court_service_packet_id_v1`].
pub fn kernel_court_branch_packet_id_v1(
    claim: Hash64,
    allocation: Hash64,
    source: Hash64,
    signer: &PalwBondKeyV2,
    turn: u64,
    branch: u64,
) -> [u8; 64] {
    if branch == 0 {
        return court_service_packet_id_v1(claim, allocation, source, signer, turn);
    }
    let bytes = borsh::to_vec(&(claim, allocation, source, signer, turn, branch)).expect("packet id serialization");
    *keyed(b"misaka-palw/kernel-court-branch-packet/v1", &bytes).as_byte_slice()
}

/// The kernel route aux table of the court capital rows (unused by every other table id).
/// **Namespace note (AUDIT-2 m3):** 150/151 are reused in separate namespaces — `PalwConsensusObjectV2` object tags 150/151 (the
/// provider lease / challenge, `PALW_PROVIDER_LEASE_TAG_V1` / `PALW_PROVIDER_CHALLENGE_TAG_V1`), the kernel-route aux table id
/// `PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1` = 150, and `PalwDeltaEntryV2::SeatRootReadiness` = 150. No collision (different key
/// spaces); THIS one is the kernel-route aux table id.
pub const PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1: u8 = 150;
pub const KERNEL_COURT_CAPITAL_ROW_VERSION_V1: u16 = 1;

/// **The court capital a kernel-route original reserved from its producer's collateral** (counts in `palw_bond_committed_v1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct KernelCourtCapitalRowV1 {
    pub version: u16,
    pub payer: PalwBondKeyV2,
    pub claim: Hash64,
    pub amount: u128,
    /// The original's hard deadline `a + 10,000`: the row is released no later than here.
    pub hard_deadline_daa: u64,
}

impl KernelCourtCapitalRowV1 {
    pub fn check_v1(&self) -> Result<(), &'static str> {
        if self.version != KERNEL_COURT_CAPITAL_ROW_VERSION_V1 || self.amount == 0 {
            return Err("invalid kernel court capital row");
        }
        Ok(())
    }
}

/// Row key: the payer first, so a bond's capital is one prefix range.
pub fn kernel_court_capital_key_v1(payer: &PalwBondKeyV2, claim: Hash64) -> Vec<u8> {
    borsh::to_vec(&(payer, claim)).expect("capital key serialization")
}
pub fn kernel_court_capital_payer_prefix_v1(payer: &PalwBondKeyV2) -> Vec<u8> {
    borsh::to_vec(payer).expect("bond key serialization")
}

/// Every legal branch's packet of one original. Refused (never truncated) when an interval would reach the hard deadline.
pub fn kernel_court_packets_v1(
    claim: Hash64,
    allocation: Hash64,
    source: Hash64,
    accepted_daa: u64,
    signers: &BTreeSet<PalwBondKeyV2>,
    schedule: &KernelCourtAdmissionScheduleV1,
) -> Result<Vec<ServicePacket>, KernelCourtAdmissionRefusalV1> {
    use KernelCourtAdmissionRefusalV1 as R;
    let p = &schedule.profile;
    if schedule.version != 1
        || schedule.follow_up_branches_per_turn == 0
        || schedule.capital_per_packet_sompi == 0
        || p.version != 1
        || p.packets_per_signer == 0
        || p.service_span_daa == 0
        || p.packet_cost.controls != 1
    {
        return Err(R::ScheduleInvalid);
    }
    if signers.is_empty() {
        return Err(R::NoLiableSigner);
    }
    let hard = accepted_daa.checked_add(DISPUTE_WINDOW_DAA).ok_or(R::Overflow)?;
    let first = accepted_daa.checked_add(1).ok_or(R::Overflow)?;
    let mut packets = Vec::new();
    for signer in signers {
        for turn in 0..p.packets_per_signer {
            let release = turn.checked_mul(p.service_span_daa).and_then(|o| first.checked_add(o)).ok_or(R::Overflow)?;
            let before = release.checked_add(p.service_span_daa).ok_or(R::Overflow)?;
            if before > hard {
                return Err(R::ProfileExceedsHard);
            }
            let branches = if turn == 0 { 1 } else { schedule.follow_up_branches_per_turn };
            for branch in 0..branches {
                packets.push(ServicePacket {
                    id: kernel_court_branch_packet_id_v1(claim, allocation, source, signer, turn, branch),
                    release_daa: release,
                    service_before: before,
                    cost: p.packet_cost,
                });
            }
        }
    }
    Ok(packets)
}

/// The admitted result: the next rooted book and the capital lock the caller writes (both, or neither).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelCourtAdmissionV1 {
    pub book: CourtServiceStateV2,
    pub claim: Hash64,
    pub payer: PalwBondKeyV2,
    pub capital: u128,
    pub hard_deadline_daa: u64,
    pub packets: usize,
}

/// **Atomic candidate (pure).** `existing` is the live book (or the book of an earlier admission in the same object);
/// `payer_free_collateral` is the producer's collateral less everything already committed (including this claim's own kernel
/// reservation). `Err` = drop the object, nothing changes.
#[allow(clippy::too_many_arguments)]
pub fn kernel_court_admission_candidate_v1(
    existing: Option<&CourtServiceStateV2>,
    claim: Hash64,
    allocation: Hash64,
    source: Hash64,
    accepted_daa: u64,
    signers: &BTreeSet<PalwBondKeyV2>,
    payer: PalwBondKeyV2,
    payer_free_collateral: u128,
    schedule: &KernelCourtAdmissionScheduleV1,
) -> Result<KernelCourtAdmissionV1, KernelCourtAdmissionRefusalV1> {
    use KernelCourtAdmissionRefusalV1 as R;
    let packets = kernel_court_packets_v1(claim, allocation, source, accepted_daa, signers, schedule)?;
    let capital = (packets.len() as u128).checked_mul(schedule.capital_per_packet_sompi as u128).ok_or(R::Overflow)?;
    if capital > payer_free_collateral {
        return Err(R::InsufficientCapital { required: capital, free: payer_free_collateral });
    }
    let hard = accepted_daa.checked_add(DISPUTE_WINDOW_DAA).ok_or(R::Overflow)?;
    let profile = &schedule.profile;
    let mut book = match existing {
        Some(old) => {
            if old.source_profile_id != profile.id() || old.book.budget != profile.budget {
                return Err(R::ProfileMismatch);
            }
            if old.claims.contains_key(&claim) {
                return Err(R::AlreadyBound);
            }
            old.clone()
        }
        None => CourtServiceStateV2 {
            version: 2,
            profile_id: court_service_profile_id_v2(profile.id()),
            source_profile_id: profile.id(),
            book: ServiceReservations::new(profile.budget).map_err(R::Reservation)?,
            claims: Default::default(),
        },
    };
    book.book.reserve(&packets, hard).map_err(R::Reservation)?;
    book.claims.insert(claim, ServiceClaimBinding {
        claim,
        allocation_id: allocation,
        source_id: source,
        original_accepted_daa: accepted_daa,
        hard_deadline_daa: hard,
        packet_ids: packets.iter().map(|p| p.id).collect(),
    });
    book.check_live_v1().map_err(|_| R::Inconsistent)?;
    Ok(KernelCourtAdmissionV1 {
        book,
        claim,
        payer,
        capital,
        hard_deadline_daa: hard,
        packets: packets.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(h(0xB0), n))
    }
    fn signers() -> BTreeSet<PalwBondKeyV2> {
        [bond(1), bond(2)].into_iter().collect()
    }

    /// Every adaptive branch of every follow-up turn is derived; branch 0 keeps the F02 identity; counts follow the schedule.
    #[test]
    fn f02b_every_follow_up_branch_is_derived() {
        let s = KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1();
        let packets = kernel_court_packets_v1(h(1), h(2), h(3), 1_000, &signers(), &s).unwrap();
        let per_signer = 1 + (s.profile.packets_per_signer - 1) * s.follow_up_branches_per_turn;
        assert_eq!(packets.len() as u64, 2 * per_signer);
        let ids: BTreeSet<_> = packets.iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), packets.len(), "distinct identities");
        for signer in signers() {
            for turn in 0..s.profile.packets_per_signer {
                assert!(ids.contains(&court_service_packet_id_v1(h(1), h(2), h(3), &signer, turn)), "branch 0 is the F02 id");
                let branches = if turn == 0 { 1 } else { s.follow_up_branches_per_turn };
                for b in 0..branches {
                    assert!(ids.contains(&kernel_court_branch_packet_id_v1(h(1), h(2), h(3), &signer, turn, b)));
                }
                assert!(!ids.contains(&kernel_court_branch_packet_id_v1(h(1), h(2), h(3), &signer, turn, branches)));
            }
        }
        assert!(packets.iter().all(|p| p.service_before <= 1_000 + DISPUTE_WINDOW_DAA && p.release_daa > 1_000));
    }

    /// Success reserves packets and capital together; a refusal (capital -1, capacity, duplicate) leaves the input book unchanged.
    #[test]
    fn f02b_admission_is_all_or_nothing() {
        let s = KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1();
        let need = kernel_court_packets_v1(h(1), h(2), h(3), 1_000, &signers(), &s).unwrap().len() as u128
            * s.capital_per_packet_sompi as u128;
        assert_eq!(
            kernel_court_admission_candidate_v1(None, h(1), h(2), h(3), 1_000, &signers(), bond(1), need - 1, &s),
            Err(KernelCourtAdmissionRefusalV1::InsufficientCapital { required: need, free: need - 1 })
        );
        let a = kernel_court_admission_candidate_v1(None, h(1), h(2), h(3), 1_000, &signers(), bond(1), need, &s).unwrap();
        assert_eq!((a.capital, a.payer, a.hard_deadline_daa), (need, bond(1), 1_000 + DISPUTE_WINDOW_DAA));
        assert_eq!(a.book.claims[&h(1)].packet_ids.len(), a.packets);
        let before = a.book.clone();
        assert_eq!(
            kernel_court_admission_candidate_v1(Some(&a.book), h(1), h(2), h(3), 1_000, &signers(), bond(1), need, &s),
            Err(KernelCourtAdmissionRefusalV1::AlreadyBound)
        );
        let mut other = s;
        other.profile.packets_per_signer -= 1;
        assert_eq!(
            kernel_court_admission_candidate_v1(Some(&a.book), h(9), h(2), h(3), 1_000, &signers(), bond(1), need, &other),
            Err(KernelCourtAdmissionRefusalV1::ProfileMismatch)
        );
        assert_eq!(a.book, before, "a refused candidate never touches its input");
        // Capacity: with a 1-packet/DAA budget, originals accepted at the same DAA eventually exhaust the first interval.
        let mut book = a.book.clone();
        let mut refused = None;
        for n in 10..2_000u64 {
            match kernel_court_admission_candidate_v1(Some(&book), h(n), h(2), h(3), 1_000, &signers(), bond(1), u128::MAX, &s) {
                Ok(next) => book = next.book,
                Err(e) => {
                    refused = Some((n, e));
                    break;
                }
            }
        }
        let (n, e) = refused.expect("capacity is finite");
        assert_eq!(e, KernelCourtAdmissionRefusalV1::Reservation(ServiceRefusal::NoCapacity));
        assert!(!book.claims.contains_key(&h(n)), "the refused original reserved nothing");
        book.check().unwrap();
        assert!(PALW_KERNEL_COURT_ADMISSION_SCHEDULE_V1.is_some(), "G14-FIX SPEC-1: production is pinned (fence-gated)");
    }

    #[test]
    fn f02b_capital_row_key_and_check() {
        let row = KernelCourtCapitalRowV1 { version: 1, payer: bond(1), claim: h(1), amount: 5, hard_deadline_daa: 10 };
        row.check_v1().unwrap();
        assert!(KernelCourtCapitalRowV1 { version: 2, ..row }.check_v1().is_err());
        assert!(KernelCourtCapitalRowV1 { amount: 0, ..row }.check_v1().is_err());
        assert!(kernel_court_capital_key_v1(&bond(1), h(1)).starts_with(&kernel_court_capital_payer_prefix_v1(&bond(1))));
        assert!(!kernel_court_capital_key_v1(&bond(2), h(1)).starts_with(&kernel_court_capital_payer_prefix_v1(&bond(1))));
    }

    #[test]
    fn f02b_invalid_schedule_or_no_signer_is_refused() {
        let mut s = KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1();
        assert_eq!(
            kernel_court_packets_v1(h(1), h(2), h(3), 5, &BTreeSet::new(), &s),
            Err(KernelCourtAdmissionRefusalV1::NoLiableSigner)
        );
        s.follow_up_branches_per_turn = 0;
        assert_eq!(kernel_court_packets_v1(h(1), h(2), h(3), 5, &signers(), &s), Err(KernelCourtAdmissionRefusalV1::ScheduleInvalid));
        let mut s = KernelCourtAdmissionScheduleV1::test_only_g14_estimate_v1();
        s.profile.service_span_daa = DISPUTE_WINDOW_DAA;
        assert_eq!(kernel_court_packets_v1(h(1), h(2), h(3), 5, &signers(), &s), Err(KernelCourtAdmissionRefusalV1::ProfileExceedsHard));
    }
}
