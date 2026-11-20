//! RFC-0061 §5. Resolve Q from the entire canonical preparation cut. The
//! owning reader supplies the VSS outcomes and signature-verified Ready rows
//! at that cut, and authenticates the recorded Q and its root. This projection
//! does not implement the private VSS protocol or verify a signature.
use super::{Tb1Field, Tb1PolynomialError, check_qualified, check_roster};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tb1VssEnd {
    pub transcript_root: [u8; 64],
    pub end_anchor: [u8; 64],
}

/// Position in the owning profile's canonical preparation order (including
/// intra-block order). This is a resolved ordering value, not a wire field.
#[derive(Clone, Copy)]
pub struct Tb1DealerEnd {
    pub end: Tb1VssEnd,
    pub position: u64,
}

#[derive(Clone, Copy)]
pub struct Tb1Ready<'a> {
    pub round_ref: &'a [u8],
    pub dealer_slot: usize,
    pub holder_slot: usize,
    pub position: u64,
    pub end: Tb1VssEnd,
}

/// All terminal dealer rows come from the specified round's native cut, in
/// roster order. None means the dealer did not finish VSS successfully there.
/// Ready rows carry their actual acceptance positions in this same ordering.
pub struct Tb1PreparationCut<'a> {
    pub round_ref: &'a [u8],
    pub deadline_position: u64,
    /// Last position covered by the authenticated preparation read, inclusive.
    pub observed_through: u64,
    pub dealer_ends: &'a [Option<Tb1DealerEnd>],
    pub ready: &'a [Tb1Ready<'a>],
}

pub struct Tb1Qualification<'a> {
    pub(super) round_ref: &'a [u8],
    pub(super) qual_root: [u8; 64],
    pub(super) roster: &'a [Tb1Field],
    pub(super) f: usize,
    pub(super) dealers: &'a [usize],
}

impl<'a> Tb1Qualification<'a> {
    /// Compare the authenticated proposal with every dealer that reached q
    /// distinct holders for its exact VSS end. Resolve the entire cut or fail.
    pub fn resolve(
        roster: &'a [Tb1Field],
        f: usize,
        round_ref: &'a [u8],
        qual_root: [u8; 64],
        dealers: &'a [usize],
        cut: &Tb1PreparationCut<'_>,
        max_ready_records: usize,
    ) -> Result<Self, Tb1PolynomialError> {
        use Tb1PolynomialError::*;
        if cut.ready.len() > max_ready_records {
            return Err(PreparationBudget);
        }
        check_roster(roster, f)?;
        check_qualified(roster, f, dealers)?;
        if round_ref.is_empty()
            || cut.round_ref != round_ref
            || cut.observed_through < cut.deadline_position
            || cut.dealer_ends.len() != roster.len()
        {
            return Err(PreparationContext);
        }
        let mut supporters = BTreeSet::new();
        let mut counts = vec![0usize; roster.len()];
        for row in cut.ready {
            let Some(Some(dealer)) = cut.dealer_ends.get(row.dealer_slot) else { continue };
            if row.round_ref == round_ref
                && row.holder_slot < roster.len()
                && row.end == dealer.end
                && (dealer.position..=cut.deadline_position).contains(&row.position)
                && supporters.insert((row.dealer_slot, row.holder_slot))
            {
                counts[row.dealer_slot] += 1;
            }
        }
        let q = roster.len() - f;
        let complete = counts.iter().enumerate().filter_map(|(i, &count)| (count >= q).then_some(i));
        if !complete.eq(dealers.iter().copied()) {
            return Err(QualifiedSet);
        }
        Ok(Self { round_ref, qual_root, roster, f, dealers })
    }
}
