//! **ADR-0133 S2 — optimistic licence, audit, Court.**
//!
//! Past `Params::palw_verification_s2` a claim may license on the full-replay seat's `Valid`
//! alone (`OptimisticLicensed`). Every other seat is an auditor: it samples (S3 if armed, S1
//! segments otherwise) and files a court accusation if the sample disagrees; it does not block
//! the licence. The existing court still voids a convicted claim and slashes. The V1 quorum and
//! the V2 coverage licence remain valid doors — S2 is an additional, faster one, used when
//! speed wins.
//!
//! The assignment is S1's: the bind still names one full-replay seat. S2 only changes how many
//! `Valid` receipts a licence needs from that assignment.

use crate::palw_panel_v2::{PalwPanelV2Error, PalwReceiptQuorumV2, PalwSeatReceiptV3};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_verification_v2::{PalwSegmentAssignmentV2, palw_segment_assignment_v2};
use kaspa_hashes::Hash64;

pub const PALW_OPTIMISTIC_LICENCE_V2_DOMAIN: &[u8] = b"misaka-palw/verification-s2/licence/v1";
pub const PALW_OPTIMISTIC_LICENCE_V2_ALL_DOMAINS: &[&[u8]] = &[PALW_OPTIMISTIC_LICENCE_V2_DOMAIN];

/// The bind's full-replay seat is the one S2 will license from.
pub fn palw_optimistic_full_seat_bond_v2(assignment: &PalwSegmentAssignmentV2, seats: &[PalwBondKeyV2]) -> Option<PalwBondKeyV2> {
    seats.get(assignment.full_seat as usize).copied()
}

/// `true` when `valid` includes a `Valid` receipt from the bind's full-replay seat.
pub fn palw_optimistic_licence_v2(
    assignment: &PalwSegmentAssignmentV2,
    seats: &[PalwBondKeyV2],
    valid_bonds: &[PalwBondKeyV2],
) -> bool {
    match palw_optimistic_full_seat_bond_v2(assignment, seats) {
        Some(full) => valid_bonds.contains(&full),
        None => false,
    }
}

/// Assemble the optimistic licence: the V3 receipts of this claim, of which the full-replay
/// seat's `Valid` is necessary and sufficient. Other receipts ride along so the fold can credit
/// them; they are not required for the door.
pub fn palw_optimistic_receipts_license_v2(
    anchor: Hash64,
    claim_id: Hash64,
    seats: &[PalwBondKeyV2],
    receipts: &[PalwSeatReceiptV3],
) -> Result<PalwReceiptQuorumV2, PalwPanelV2Error> {
    let assignment = palw_segment_assignment_v2(anchor, claim_id, seats.len() as u16);
    let valid: Vec<PalwBondKeyV2> = receipts
        .iter()
        .filter(|r| matches!(r.receipt.verdict, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid) && r.receipt.claim == claim_id)
        .map(|r| r.receipt.seat_bond)
        .collect();
    if palw_optimistic_licence_v2(&assignment, seats, &valid) {
        Ok(PalwReceiptQuorumV2::Licensed { valid: valid.len() as u16 })
    } else {
        Err(PalwPanelV2Error::NoQuorum { valid: valid.len() as u16, unavailable: 0, needed: 1 })
    }
}

/// **The coverage half of the optimistic door, as the acceptance layer applies it.** The set's
/// receipts are checked by `validate_receipt_coverage_v2` for everything a receipt must be (a seat,
/// once, signed over its mask, inside the window), and the door takes `Ok` or `NoQuorum` from it and
/// nothing else. So a set below the quorum passes without coverage, and a set AT or past the quorum
/// must cover: three or four `Valid` on the shipped five-seat panel are `CoverageShort` and refused.
///
/// One predicate, asked by the processor's `OptimisticLicensed` arm and by the node's selection
/// (`palw_panel_v2::palw_select_optimistic_licence_v2`), so the set a node offers and the set a block
/// accepts cannot be decided by two copies of the rule.
pub fn palw_optimistic_coverage_admits_v2(coverage: &Result<PalwReceiptQuorumV2, PalwPanelV2Error>) -> bool {
    matches!(coverage, Ok(_) | Err(PalwPanelV2Error::NoQuorum { .. }))
}

/// **Would the acceptance layer take this set as an `OptimisticLicensed`?** Both of its checks: the
/// full-replay seat's `Valid` ([`palw_optimistic_receipts_license_v2`]) and the coverage verdict
/// `coverage` — `validate_receipt_coverage_v2` over the same `receipts` at the carrying point — as
/// [`palw_optimistic_coverage_admits_v2`] reads it.
pub fn palw_optimistic_licence_admits_v2(
    anchor: Hash64,
    claim_id: Hash64,
    seats: &[PalwBondKeyV2],
    receipts: &[PalwSeatReceiptV3],
    coverage: &Result<PalwReceiptQuorumV2, PalwPanelV2Error>,
) -> bool {
    palw_optimistic_receipts_license_v2(anchor, claim_id, seats, receipts).is_ok() && palw_optimistic_coverage_admits_v2(coverage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_panel_v2::PalwReceiptVerdictV2;
    use crate::palw_verification_v2::PalwSegmentMaskV2;
    use crate::tx::TransactionOutpoint;

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n as u64), index: 0 })
    }

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    #[test]
    fn the_full_seat_alone_licenses_and_a_partial_seat_does_not() {
        let seats = vec![bond(1), bond(2), bond(3), bond(4), bond(5)];
        let assignment = palw_segment_assignment_v2(h(9), h(8), seats.len() as u16);
        let full = palw_optimistic_full_seat_bond_v2(&assignment, &seats).expect("a five-seat panel has a full seat");
        assert!(palw_optimistic_licence_v2(&assignment, &seats, &[full]));
        let other = seats.iter().copied().find(|b| *b != full).unwrap();
        assert!(!palw_optimistic_licence_v2(&assignment, &seats, &[other]));
        assert!(!palw_optimistic_licence_v2(&assignment, &seats, &[]));
    }

    #[test]
    fn the_assembler_takes_the_full_seats_valid_and_refuses_without_it() {
        let seats = vec![bond(1), bond(2), bond(3)];
        let assignment = palw_segment_assignment_v2(h(1), h(2), 3);
        let full = palw_optimistic_full_seat_bond_v2(&assignment, &seats).unwrap();
        let receipt = PalwSeatReceiptV3 {
            receipt: crate::palw_panel_v2::PalwSeatReceiptV2 {
                claim: h(2),
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: full,
                signed_daa: 10,
                signature: vec![1],
            },
            segments: PalwSegmentMaskV2::full(assignment.segments),
        };
        assert!(matches!(
            palw_optimistic_receipts_license_v2(h(1), h(2), &seats, &[receipt.clone()]),
            Ok(PalwReceiptQuorumV2::Licensed { valid: 1 })
        ));
        let mut wrong = receipt;
        wrong.receipt.seat_bond = seats.iter().copied().find(|b| *b != full).unwrap();
        assert!(palw_optimistic_receipts_license_v2(h(1), h(2), &seats, &[wrong]).is_err());
    }
}
