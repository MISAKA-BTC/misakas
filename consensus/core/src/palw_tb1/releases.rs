//! RFC-0061 §7 public release assembly and verification after native record resolution.
//! The caller supplies the resolved qualification, canonical RoundRef bytes,
//! signed release rows keyed by their actual references, and release-gate/deadline checks.
//! This layer neither defines their wire encodings nor admits a beacon into consensus.
use super::qualification::Tb1Qualification;
use super::reconstruction::reconstruct_dealer;
use super::{Tb1DealerPoint, Tb1Field, Tb1PolynomialError, check_points};
use std::collections::{BTreeMap, btree_map::Entry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tb1ReleaseContext<'a> {
    pub round_ref: &'a [u8],
    pub qual_root: [u8; 64],
    pub freeze_id: [u8; 64],
}

#[derive(Clone, Copy)]
pub struct Tb1HolderRelease<'a> {
    pub context: Tb1ReleaseContext<'a>,
    pub holder_slot: usize,
    pub shares: &'a [(usize, Tb1Field)],
}

/// References identify whole holder releases. A certificate cannot supply its own y-values.
pub struct Tb1DealerCertificate<R> {
    pub dealer_slot: usize,
    pub coefficients: Vec<Tb1Field>,
    pub supporting_releases: Vec<R>,
}

pub struct Tb1BeaconCertificate<'a, R> {
    pub context: Tb1ReleaseContext<'a>,
    pub dealers: Vec<Tb1DealerCertificate<R>>,
}

pub struct Tb1ReleaseSet<'a, R> {
    context: Tb1ReleaseContext<'a>,
    roster: &'a [Tb1Field],
    f: usize,
    qualified: &'a [usize],
    releases: BTreeMap<R, Tb1HolderRelease<'a>>,
}

impl<'a, R: Ord + Copy> Tb1ReleaseSet<'a, R> {
    /// Use the full Q resolved from native preparation outcomes and Ready
    /// records. A release for another qualification cannot borrow this quorum.
    pub fn new(
        context: Tb1ReleaseContext<'a>,
        qualified: &Tb1Qualification<'a>,
        releases: &BTreeMap<R, Tb1HolderRelease<'a>>,
        max_release_records: usize,
    ) -> Result<Self, Tb1PolynomialError> {
        if context.round_ref != qualified.round_ref || context.qual_root != qualified.qual_root {
            return Err(Tb1PolynomialError::ReleaseContext);
        }
        Self::index_releases(context, qualified.roster, qualified.f, qualified.dealers, releases, max_release_records)
    }

    /// Index only complete releases at this freeze. A malformed or unrelated row cannot
    /// suppress a quorum of honest releases. Other signed versions can coexist by reference.
    fn index_releases(
        context: Tb1ReleaseContext<'a>,
        roster: &'a [Tb1Field],
        f: usize,
        qualified: &'a [usize],
        resolved: &BTreeMap<R, Tb1HolderRelease<'a>>,
        max_release_rows: usize,
    ) -> Result<Self, Tb1PolynomialError> {
        if resolved.len() > max_release_rows {
            return Err(Tb1PolynomialError::ReconstructionBudget);
        }
        let releases = resolved
            .iter()
            .filter_map(|(&id, &row)| {
                (row.context == context
                    && row.holder_slot < roster.len()
                    && row.shares.len() == qualified.len()
                    && row.shares.iter().map(|(dealer, _)| dealer).eq(qualified))
                .then_some((id, row))
            })
            .collect();
        Ok(Self { context, roster, f, qualified, releases })
    }

    /// Build all dealer witnesses. Identical retransmissions count once. An equivocating
    /// holder is omitted by the assembler. Discovery of another release does not invalidate
    /// an existing certificate whose own references still have distinct holders and agree.
    pub fn assemble(&self, max_total_matrix_cells: usize) -> Result<Tb1BeaconCertificate<'a, R>, Tb1PolynomialError> {
        let mut holders = BTreeMap::new();
        for (&id, row) in &self.releases {
            match holders.entry(row.holder_slot) {
                Entry::Vacant(entry) => {
                    entry.insert(Some(id));
                }
                Entry::Occupied(mut entry) => {
                    if entry.get().is_some_and(|prior| self.releases[&prior].shares != row.shares) {
                        entry.insert(None);
                    }
                }
            }
        }
        let selected: BTreeMap<_, _> = holders.into_iter().filter_map(|(slot, id)| id.map(|id| (slot, id))).collect();
        let m = selected.len();
        let q = self.roster.len() - self.f;
        let errors = m.checked_sub(q).ok_or(Tb1PolynomialError::Support)?;
        let per_dealer = super::reconstruction::matrix_cells(self.f, errors, m)?;
        if per_dealer > max_total_matrix_cells / self.qualified.len() {
            return Err(Tb1PolynomialError::ReconstructionBudget);
        }
        let dealers = self
            .qualified
            .iter()
            .enumerate()
            .map(|(i, &dealer_slot)| {
                let points: Vec<_> = selected
                    .iter()
                    .map(|(&holder_slot, id)| Tb1DealerPoint { holder_slot, value: self.releases[id].shares[i].1 })
                    .collect();
                let recovered = reconstruct_dealer(self.roster, self.f, &points, per_dealer)?;
                let supporting_releases = recovered.support.iter().map(|p| selected[&p.holder_slot]).collect();
                Ok(Tb1DealerCertificate { dealer_slot, coefficients: recovered.coefficients, supporting_releases })
            })
            .collect::<Result<_, Tb1PolynomialError>>()?;
        Ok(Tb1BeaconCertificate { context: self.context, dealers })
    }

    /// Check each polynomial directly against the shares actually signed by its cited holders.
    /// The complete ordered constants are the input for the profile's subsequent seed encoding.
    pub fn verify(&self, certificate: &Tb1BeaconCertificate<'_, R>) -> Result<Vec<(usize, Tb1Field)>, Tb1PolynomialError> {
        use Tb1PolynomialError::{QualifiedSet, Support};
        if certificate.context != self.context {
            return Err(Tb1PolynomialError::ReleaseContext);
        }
        let certificates = &certificate.dealers;
        if certificates.len() != self.qualified.len() {
            return Err(QualifiedSet);
        }
        self.qualified
            .iter()
            .zip(certificates)
            .enumerate()
            .map(|(i, (&dealer, cert))| {
                if dealer != cert.dealer_slot {
                    return Err(QualifiedSet);
                }
                if !(self.roster.len() - self.f..=self.roster.len()).contains(&cert.supporting_releases.len()) {
                    return Err(Support);
                }
                let points = cert
                    .supporting_releases
                    .iter()
                    .map(|id| {
                        let row = self.releases.get(id).ok_or(Support)?;
                        Ok(Tb1DealerPoint { holder_slot: row.holder_slot, value: row.shares[i].1 })
                    })
                    .collect::<Result<Vec<_>, Tb1PolynomialError>>()?;
                check_points(self.roster, self.f, &cert.coefficients, &points)?;
                Ok((dealer, cert.coefficients[0]))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_math::Uint576;

    fn field(value: usize) -> Tb1Field {
        Tb1Field(Uint576::from_u64(value as u64))
    }

    const EXEC: Tb1ReleaseContext<'static> = Tb1ReleaseContext { round_ref: b"EXEC fixture", qual_root: [1; 64], freeze_id: [2; 64] };
    const Q: [usize; 6] = [0, 1, 2, 4, 5, 6];

    fn rows() -> Vec<Vec<(usize, Tb1Field)>> {
        (1..=7).map(|x| Q.map(|dealer| (dealer, field(100 + dealer + (dealer + 1) * x + 2 * x * x))).to_vec()).collect()
    }
    fn resolved(values: &[Vec<(usize, Tb1Field)>]) -> BTreeMap<usize, Tb1HolderRelease<'_>> {
        values
            .iter()
            .enumerate()
            .map(|(holder_slot, v)| (20 + holder_slot, Tb1HolderRelease { context: EXEC, holder_slot, shares: v }))
            .collect()
    }

    #[test]
    fn qualification_counts_every_eligible_dealer_before_the_fixed_cut() {
        use crate::palw_tb1::qualification::{Tb1DealerEnd, Tb1PreparationCut, Tb1Ready, Tb1VssEnd};
        use Tb1PolynomialError::*;
        let roster = (1..=7).map(field).collect::<Vec<_>>();
        let ends: Vec<_> = (0..7)
            .map(|i| Some(Tb1DealerEnd { end: Tb1VssEnd { transcript_root: [i; 64], end_anchor: [4; 64] }, position: 10 }))
            .collect();
        // Six dealers each reach q=5; dealer 3 is one valid holder short.
        let mut ready = Vec::new();
        for (slot, terminal) in ends.iter().enumerate() {
            for holder_slot in 0..if slot == 3 { 4 } else { 5 } {
                ready.push(Tb1Ready {
                    round_ref: EXEC.round_ref,
                    dealer_slot: slot,
                    holder_slot,
                    position: 11,
                    end: terminal.unwrap().end,
                });
            }
        }
        let fourth = Tb1Ready { round_ref: EXEC.round_ref, dealer_slot: 3, holder_slot: 4, position: 12, end: ends[3].unwrap().end };
        ready.extend([
            Tb1Ready { holder_slot: 0, ..fourth },
            Tb1Ready { holder_slot: 7, ..fourth },
            Tb1Ready { round_ref: b"PANEL", ..fourth },
            Tb1Ready { position: 9, ..fourth },
            Tb1Ready { position: 13, ..fourth },
            Tb1Ready { dealer_slot: 7, ..fourth },
            Tb1Ready { end: Tb1VssEnd { transcript_root: [8; 64], ..fourth.end }, ..fourth },
            Tb1Ready { end: Tb1VssEnd { end_anchor: [8; 64], ..fourth.end }, ..fourth },
        ]);
        ready.reverse();
        let cut = Tb1PreparationCut {
            round_ref: EXEC.round_ref,
            deadline_position: 12,
            observed_through: 13,
            dealer_ends: &ends,
            ready: &ready,
        };
        let resolve = |dealers, snapshot: &Tb1PreparationCut<'_>, budget| {
            Tb1Qualification::resolve(&roster, 2, EXEC.round_ref, EXEC.qual_root, dealers, snapshot, budget)
        };
        assert!(matches!(resolve(&Q, &cut, ready.len() - 1), Err(PreparationBudget)));
        assert!(matches!(resolve(&Q[1..], &cut, ready.len()), Err(QualifiedSet)));
        assert!(matches!(resolve(&Q, &Tb1PreparationCut { round_ref: b"PANEL", ..cut }, ready.len()), Err(PreparationContext)));
        assert!(matches!(resolve(&Q, &Tb1PreparationCut { observed_through: 11, ..cut }, ready.len()), Err(PreparationContext)));
        let qualified = resolve(&Q, &cut, ready.len()).unwrap();
        let values = rows();
        let released = resolved(&values);
        let set = Tb1ReleaseSet::new(EXEC, &qualified, &released, 7).unwrap();
        assert_eq!(set.verify(&set.assemble(336).unwrap()).unwrap(), Q.map(|d| (d, field(100 + d))));
        assert!(matches!(
            Tb1ReleaseSet::new(Tb1ReleaseContext { qual_root: [3; 64], ..EXEC }, &qualified, &released, 7),
            Err(ReleaseContext)
        ));
        ready.push(fourth);
        let cut = Tb1PreparationCut {
            round_ref: EXEC.round_ref,
            deadline_position: 12,
            observed_through: 13,
            dealer_ends: &ends,
            ready: &ready,
        };
        assert!(matches!(resolve(&Q, &cut, ready.len()), Err(QualifiedSet)));
        let all = [0, 1, 2, 3, 4, 5, 6];
        assert!(resolve(&all, &cut, ready.len()).is_ok());
        let mut incomplete = ends.clone();
        incomplete[3] = None;
        let cut = Tb1PreparationCut { dealer_ends: &incomplete, ..cut };
        assert!(resolve(&Q, &cut, ready.len()).is_ok());
        assert!(matches!(resolve(&all, &cut, ready.len()), Err(QualifiedSet)));
    }

    #[test]
    fn each_dealer_uses_its_own_signed_column_and_support_subset() {
        let roster = (1..=7).map(field).collect::<Vec<_>>();
        let mut values = rows();
        values[1][0].1 = field(800);
        values[6][1].1 = field(900);
        let rows = resolved(&values);
        let set = Tb1ReleaseSet::index_releases(EXEC, &roster, 2, &Q, &rows, 7).unwrap();
        let budget = Q.len() * 56;
        assert!(matches!(set.assemble(budget - 1), Err(Tb1PolynomialError::ReconstructionBudget)));
        let mut cert = set.assemble(budget).unwrap();
        let expected = Q.map(|d| (d, field(100 + d)));
        assert_eq!(set.verify(&cert).unwrap(), expected);
        assert_eq!(cert.dealers[0].supporting_releases, [20, 22, 23, 24, 25, 26]);
        assert_eq!(cert.dealers[1].supporting_releases, [20, 21, 22, 23, 24, 25]);
        let last = cert.dealers.pop().unwrap();
        assert_eq!(set.verify(&cert), Err(Tb1PolynomialError::QualifiedSet));
        cert.dealers.push(last);
        cert.context.freeze_id = [44; 64];
        assert_eq!(set.verify(&cert), Err(Tb1PolynomialError::ReleaseContext));
        cert.context = EXEC;
        cert.dealers.swap(0, 1);
        assert_eq!(set.verify(&cert), Err(Tb1PolynomialError::QualifiedSet));
        cert.dealers.swap(0, 1);
        cert.dealers[0].supporting_releases = vec![20, 21, 22, 23, 24];
        assert_eq!(set.verify(&cert), Err(Tb1PolynomialError::InconsistentPoint));
        cert.dealers[0].supporting_releases = vec![20, 22, 23, 24, 25];
        cert.dealers[0].coefficients[0] = field(101);
        assert_eq!(set.verify(&cert), Err(Tb1PolynomialError::InconsistentPoint));
    }

    #[test]
    fn same_holder_and_cross_freeze_references_cannot_supply_a_quorum() {
        let roster = (1..=7).map(field).collect::<Vec<_>>();
        let values = rows();
        let mut rows = resolved(&values);
        let short = &values[0][1..];
        let mut reordered = values[0].clone();
        reordered.swap(1, 2);
        let mut changed = values[0].clone();
        changed[0].1 = field(0);
        let row = rows[&20];
        for (id, context) in [
            (40, Tb1ReleaseContext { round_ref: b"PANEL fixture", ..EXEC }),
            (41, Tb1ReleaseContext { qual_root: [5; 64], ..EXEC }),
            (42, Tb1ReleaseContext { freeze_id: [9; 64], ..EXEC }),
        ] {
            rows.insert(id, Tb1HolderRelease { context, ..row });
        }
        rows.insert(43, Tb1HolderRelease { shares: short, ..row });
        rows.insert(44, Tb1HolderRelease { shares: &reordered, ..row });
        rows.insert(45, Tb1HolderRelease { holder_slot: 7, ..row });
        rows.insert(46, row); // another reference to the same holder values
        assert!(matches!(
            Tb1ReleaseSet::index_releases(EXEC, &roster, 2, &Q, &rows, rows.len() - 1),
            Err(Tb1PolynomialError::ReconstructionBudget)
        ));
        let set = Tb1ReleaseSet::index_releases(EXEC, &roster, 2, &Q, &rows, rows.len()).unwrap();
        let mut cert = set.assemble(336).unwrap();
        let expected = Q.map(|d| (d, field(100 + d)));
        for id in 40..=46 {
            cert.dealers[0].supporting_releases = vec![20, 21, 22, 23, id];
            assert_eq!(set.verify(&cert), Err(Tb1PolynomialError::Support));
        }
        cert.dealers[0].supporting_releases = vec![46, 21, 22, 23, 24];
        assert_eq!(set.verify(&cert).unwrap(), expected);
        rows.insert(47, Tb1HolderRelease { shares: &changed, ..row });
        rows.remove(&26); // one absentee and one equivocator still leave q=5
        let later = Tb1ReleaseSet::index_releases(EXEC, &roster, 2, &Q, &rows, rows.len()).unwrap();
        let mut alternative = later.assemble(6 * 20).unwrap();
        assert!(alternative.dealers.iter().all(|d| d.supporting_releases == [21, 22, 23, 24, 25]));
        assert_eq!(later.verify(&alternative).unwrap(), expected);
        alternative.dealers[0].supporting_releases = cert.dealers[0].supporting_releases.clone();
        assert_eq!(later.verify(&alternative).unwrap(), expected);
        rows.remove(&25);
        let lacking = Tb1ReleaseSet::index_releases(EXEC, &roster, 2, &Q, &rows, rows.len()).unwrap();
        assert!(matches!(lacking.assemble(336), Err(Tb1PolynomialError::Support)));
    }
}
