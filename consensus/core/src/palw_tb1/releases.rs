//! RFC-0061 §7 public release assembly and verification after native record resolution.
//! The caller supplies the resolved qualification, canonical RoundRef bytes,
//! signed release rows keyed by their actual references, and positions from one native cut.
//! This layer neither defines their wire encodings nor admits a beacon into consensus.
use super::qualification::Tb1Qualification;
use super::reconstruction::reconstruct_dealer;
use super::signatures;
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

/// A release accepted on the same ordered native history as its certificate.
pub struct Tb1SignedRelease<'a, 'sig> {
    pub row: Tb1HolderRelease<'a>,
    pub accepted_position: u64,
    pub signature: &'sig [u8],
}

/// Resolved by the owner from the fixed TimingPlan, FreezeRecord and the
/// native stability record at the candidate certificate's position. Reading
/// this projection neither authorizes a holder to disclose shares nor
/// establishes stability on a different branch.
#[derive(Clone, Copy)]
pub struct Tb1ReleaseCut<'a> {
    pub context: Tb1ReleaseContext<'a>,
    pub closed_at: u64,
    pub stable_at: u64,
    pub release_from: u64,
    pub deadline_exclusive: u64,
    pub certificate_at: u64,
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

/// The RFC-0061 output for this private encoding. A legacy native beacon has a different type and width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tb1BeaconSeed([u8; 32]);

impl Tb1BeaconSeed {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub struct Tb1ReleaseSet<'a, R> {
    context: Tb1ReleaseContext<'a>,
    roster: &'a [Tb1Field],
    f: usize,
    qualified: &'a [usize],
    releases: BTreeMap<R, Tb1HolderRelease<'a>>,
}

impl Tb1Qualification<'_> {
    /// Signer and certificate reader use the same qualified dealer columns,
    /// slot and registered key. The native freeze/stability gate is separate.
    pub fn holder_release_message(
        &self,
        row: &Tb1HolderRelease<'_>,
        key: &libcrux_ml_dsa::ml_dsa_65::MLDSA65VerificationKey,
        byte_limit: usize,
    ) -> Result<Vec<u8>, Tb1PolynomialError> {
        if row.context.round_ref != self.round_ref
            || row.context.qual_root != self.qual_root
            || self.roster.holder_keys.get(row.holder_slot).is_none_or(|registered| registered.as_slice() != key.as_slice())
            || row.shares.len() != self.dealers.len()
            || !row.shares.iter().map(|(dealer, _)| dealer).eq(self.dealers)
        {
            return Err(Tb1PolynomialError::ReleaseContext);
        }
        signatures::release_message(row, byte_limit).ok_or(Tb1PolynomialError::ReleaseContext)
    }
}

impl<'a, R: Ord + Copy> Tb1ReleaseSet<'a, R> {
    /// Use the full Q resolved from native preparation outcomes and Ready
    /// records. A release for another qualification cannot borrow this quorum.
    pub fn new(
        cut: &Tb1ReleaseCut<'a>,
        qualified: &Tb1Qualification<'a>,
        resolved: &BTreeMap<R, Tb1SignedRelease<'a, '_>>,
        max_release_records: usize,
        max_message_bytes: usize,
    ) -> Result<Self, Tb1PolynomialError> {
        let context = cut.context;
        if context.round_ref != qualified.round_ref || context.qual_root != qualified.qual_root {
            return Err(Tb1PolynomialError::ReleaseContext);
        }
        if qualified.preparation_deadline > cut.closed_at
            || cut.closed_at > cut.stable_at
            || cut.stable_at >= cut.release_from
            || !(cut.release_from..cut.deadline_exclusive).contains(&cut.certificate_at)
        {
            return Err(Tb1PolynomialError::ReleaseTiming);
        }
        if resolved.len() > max_release_records {
            return Err(Tb1PolynomialError::ReconstructionBudget);
        }
        let releases = resolved
            .iter()
            .filter_map(|(&id, signed)| {
                let row = signed.row;
                let key = qualified.roster.holder_keys.get(row.holder_slot)?;
                if row.context != context || !(cut.release_from..cut.certificate_at).contains(&signed.accepted_position) {
                    return None;
                }
                let msg = qualified.holder_release_message(&row, key, max_message_bytes).ok()?;
                signatures::verify(key, signed.signature, &msg, signatures::RELEASE_CONTEXT).then_some((id, row))
            })
            .collect();
        Ok(Self { context, roster: qualified.roster.coordinates, f: qualified.roster.f, qualified: qualified.dealers, releases })
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
                if !points.is_sorted_by(|a, b| a.holder_slot < b.holder_slot) {
                    return Err(Support);
                }
                check_points(self.roster, self.f, &cert.coefficients, &points)?;
                Ok((dealer, cert.coefficients[0]))
            })
            .collect()
    }

    /// Derive §8's seed only after verifying every dealer against the same release cut.
    /// Private encoding: length-prefixed domain and RoundRef, two 64-byte roots,
    /// then a counted vector of (u32 slot, 66-byte field). Integers are big-endian.
    pub fn verify_seed(&self, certificate: &Tb1BeaconCertificate<'_, R>) -> Result<Tb1BeaconSeed, Tb1PolynomialError> {
        use sha3::{
            Shake256,
            digest::{ExtendableOutput, Update},
        };

        let constants = self.verify(certificate)?;
        let mut hash = Shake256::default();
        for bytes in [b"PALW-TB1/beacon".as_slice(), self.context.round_ref] {
            hash.update(&u32::try_from(bytes.len()).map_err(|_| Tb1PolynomialError::ReleaseContext)?.to_be_bytes());
            hash.update(bytes);
        }
        hash.update(&self.context.qual_root);
        hash.update(&self.context.freeze_id);
        hash.update(&u32::try_from(constants.len()).map_err(|_| Tb1PolynomialError::QualifiedSet)?.to_be_bytes());
        for (slot, value) in constants {
            hash.update(&u32::try_from(slot).map_err(|_| Tb1PolynomialError::QualifiedSet)?.to_be_bytes());
            hash.update(&value.to_bytes());
        }
        let mut seed = [0; 32];
        hash.finalize_xof_into(&mut seed);
        Ok(Tb1BeaconSeed(seed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tb1::qualification::Tb1Roster;
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

    use libcrux_ml_dsa::ml_dsa_65::{self, MLDSA65KeyPair, MLDSA65VerificationKey};
    use std::sync::LazyLock;
    static KEYS: LazyLock<Vec<MLDSA65KeyPair>> = LazyLock::new(|| (0..7).map(|i| ml_dsa_65::generate_key_pair([i; 32])).collect());
    static PUBLIC: LazyLock<Vec<MLDSA65VerificationKey>> = LazyLock::new(|| KEYS.iter().map(|k| k.verification_key.clone()).collect());
    const CUT: Tb1ReleaseCut<'static> =
        Tb1ReleaseCut { context: EXEC, closed_at: 14, stable_at: 16, release_from: 17, deadline_exclusive: 25, certificate_at: 24 };
    const BYTE_LIMIT: usize = 4096;

    fn sign(slot: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
        ml_dsa_65::sign(&KEYS[slot].signing_key, message, context, [55; 32]).unwrap().as_slice().to_vec()
    }

    fn signed_set<'a>(
        cut: Tb1ReleaseCut<'a>,
        qualified: &Tb1Qualification<'a>,
        rows: &BTreeMap<usize, Tb1HolderRelease<'a>>,
        budget: usize,
    ) -> Result<Tb1ReleaseSet<'a, usize>, Tb1PolynomialError> {
        let signatures: BTreeMap<_, _> = rows
            .iter()
            .map(|(&id, row)| {
                let bytes = signatures::release_message(row, BYTE_LIMIT);
                let signature = bytes
                    .filter(|_| row.holder_slot < KEYS.len())
                    .map_or_else(Vec::new, |msg| sign(row.holder_slot, &msg, signatures::RELEASE_CONTEXT));
                (id, signature)
            })
            .collect();
        let signed = rows
            .iter()
            .map(|(&id, &row)| (id, Tb1SignedRelease { row, accepted_position: 17, signature: &signatures[&id] }))
            .collect();
        Tb1ReleaseSet::new(&cut, qualified, &signed, budget, BYTE_LIMIT)
    }

    fn fixture_qualification(roster: &[Tb1Field]) -> Tb1Qualification<'_> {
        Tb1Qualification {
            round_ref: EXEC.round_ref,
            qual_root: EXEC.qual_root,
            roster: Tb1Roster { coordinates: roster, holder_keys: &PUBLIC, f: 2 },
            preparation_deadline: 12,
            dealers: &Q,
        }
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
                    signature: &[],
                    end: terminal.unwrap().end,
                });
            }
        }
        let mut fourth = Tb1Ready {
            round_ref: EXEC.round_ref,
            dealer_slot: 3,
            holder_slot: 4,
            position: 12,
            end: ends[3].unwrap().end,
            signature: &[],
        };
        let fourth_sig = sign(4, &signatures::ready_message(&fourth, BYTE_LIMIT).unwrap(), signatures::READY_CONTEXT);
        fourth.signature = &fourth_sig;
        let ready_signatures: Vec<_> = ready
            .iter()
            .map(|row| sign(row.holder_slot, &signatures::ready_message(row, BYTE_LIMIT).unwrap(), signatures::READY_CONTEXT))
            .collect();
        for (row, sig) in ready.iter_mut().zip(&ready_signatures) {
            row.signature = sig;
        }
        let wrong_key = sign(6, &signatures::ready_message(&fourth, BYTE_LIMIT).unwrap(), signatures::READY_CONTEXT);
        ready.extend([
            Tb1Ready { signature: &wrong_key, ..fourth },
            Tb1Ready { holder_slot: 0, ..fourth },
            Tb1Ready { holder_slot: 7, ..fourth },
            Tb1Ready { round_ref: b"PANEL", ..fourth },
            Tb1Ready { position: 9, ..fourth },
            Tb1Ready { position: 10, ..fourth },
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
            Tb1Qualification::resolve(
                Tb1Roster { coordinates: &roster, holder_keys: &PUBLIC, f: 2 },
                EXEC.round_ref,
                [17; 64], // reserved resources reference
                dealers,
                snapshot,
                budget,
                BYTE_LIMIT,
            )
        };
        assert!(matches!(resolve(&Q, &cut, ready.len() - 1), Err(PreparationBudget)));
        assert!(matches!(resolve(&Q[1..], &cut, ready.len()), Err(QualifiedSet)));
        assert!(matches!(resolve(&Q, &Tb1PreparationCut { round_ref: b"PANEL", ..cut }, ready.len()), Err(PreparationContext)));
        assert!(matches!(resolve(&Q, &Tb1PreparationCut { observed_through: 11, ..cut }, ready.len()), Err(PreparationContext)));
        let mut duplicates = PUBLIC.clone();
        duplicates[3] = duplicates[4].clone();
        assert!(matches!(
            Tb1Qualification::resolve(
                Tb1Roster { coordinates: &roster, holder_keys: &duplicates, f: 2 },
                EXEC.round_ref,
                [17; 64], // reserved resources reference
                &Q,
                &cut,
                ready.len(),
                BYTE_LIMIT
            ),
            Err(Roster)
        ));
        let qualified = resolve(&Q, &cut, ready.len()).unwrap();
        let context = Tb1ReleaseContext { qual_root: qualified.root(), ..EXEC };
        let mut reordered = ready.clone();
        reordered.reverse();
        reordered.push(reordered[0]);
        let reordered_cut = Tb1PreparationCut { ready: &reordered, ..cut };
        assert_eq!(resolve(&Q, &reordered_cut, reordered.len()).unwrap().root(), qualified.root());
        let reduced: Vec<_> = ready.iter().copied().filter(|r| !(r.dealer_slot == 3 && r.holder_slot == 2)).collect();
        let reduced_cut = Tb1PreparationCut { ready: &reduced, ..cut };
        assert_ne!(resolve(&Q, &reduced_cut, reduced.len()).unwrap().root(), qualified.root());
        let values = rows();
        let mut released = resolved(&values);
        for row in released.values_mut() {
            row.context = context;
        }
        let set = signed_set(Tb1ReleaseCut { context, ..CUT }, &qualified, &released, 7).unwrap();
        assert_eq!(set.verify(&set.assemble(336).unwrap()).unwrap(), Q.map(|d| (d, field(100 + d))));
        assert!(matches!(
            signed_set(Tb1ReleaseCut { context: Tb1ReleaseContext { qual_root: [3; 64], ..EXEC }, ..CUT }, &qualified, &released, 7),
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
    fn shares_require_the_fixed_key_preimage_and_native_release_window() {
        let roster = (1..=7).map(field).collect::<Vec<_>>();
        let qualified = fixture_qualification(&roster);
        let values = rows();
        let owners = resolved(&values);
        let signatures: Vec<_> = owners
            .values()
            .map(|row| sign(row.holder_slot, &signatures::release_message(row, BYTE_LIMIT).unwrap(), signatures::RELEASE_CONTEXT))
            .collect();
        // Four timely, distinct holders cannot form q=5 without the fifth row.
        let mut releases: BTreeMap<_, _> = owners
            .iter()
            .take(4)
            .map(|(&id, &row)| {
                (id, Tb1SignedRelease { row, accepted_position: 17, signature: signatures[row.holder_slot].as_slice() })
            })
            .collect();
        let row = owners[&24];
        let msg = signatures::release_message(&row, BYTE_LIMIT).unwrap();
        let domain = sign(4, &msg, signatures::READY_CONTEXT);
        let mut forged_values = row.shares.to_vec();
        forged_values[0].1 = field(5000);
        for (candidate, position, sig) in [
            (row, 16, &signatures[4][..]),
            (row, 24, &signatures[4]),
            (row, 25, &signatures[4]),
            (row, 17, &signatures[3]),
            (row, 17, &domain),
            (row, 17, &signatures[4][1..]),
            (Tb1HolderRelease { shares: &forged_values, ..row }, 17, &signatures[4]),
        ] {
            releases.insert(24, Tb1SignedRelease { row: candidate, accepted_position: position, signature: sig });
            assert!(matches!(
                Tb1ReleaseSet::new(&CUT, &qualified, &releases, 5, BYTE_LIMIT).unwrap().assemble(336),
                Err(Tb1PolynomialError::Support)
            ));
        }
        releases.insert(24, Tb1SignedRelease { row, accepted_position: 23, signature: &signatures[4] });
        for cut in [
            Tb1ReleaseCut { closed_at: 11, ..CUT },
            Tb1ReleaseCut { stable_at: 13, ..CUT },
            Tb1ReleaseCut { stable_at: 17, ..CUT },
            Tb1ReleaseCut { certificate_at: 16, ..CUT },
            Tb1ReleaseCut { certificate_at: 25, ..CUT },
        ] {
            assert!(matches!(Tb1ReleaseSet::new(&cut, &qualified, &releases, 5, BYTE_LIMIT), Err(Tb1PolynomialError::ReleaseTiming)));
        }
        let set = Tb1ReleaseSet::new(&CUT, &qualified, &releases, 5, msg.len()).unwrap();
        assert_eq!(set.verify(&set.assemble(336).unwrap()).unwrap(), Q.map(|d| (d, field(100 + d))));
        assert!(matches!(
            Tb1ReleaseSet::new(&CUT, &qualified, &releases, 5, msg.len() - 1).unwrap().assemble(336),
            Err(Tb1PolynomialError::Support)
        ));
    }

    #[test]
    fn each_dealer_uses_its_own_signed_column_and_support_subset() {
        let roster = (1..=7).map(field).collect::<Vec<_>>();
        let mut values = rows();
        values[1][0].1 = field(800);
        values[6][1].1 = field(900);
        let rows = resolved(&values);
        let set = signed_set(CUT, &fixture_qualification(&roster), &rows, 7).unwrap();
        let budget = Q.len() * 56;
        assert!(matches!(set.assemble(budget - 1), Err(Tb1PolynomialError::ReconstructionBudget)));
        let mut cert = set.assemble(budget).unwrap();
        let expected = Q.map(|d| (d, field(100 + d)));
        assert_eq!(set.verify(&cert).unwrap(), expected);
        assert_eq!(cert.dealers[0].supporting_releases, [20, 22, 23, 24, 25, 26]);
        assert_eq!(cert.dealers[1].supporting_releases, [20, 21, 22, 23, 24, 25]);
        // Independent Python hashlib.shake_256 vector, with different support per dealer.
        let seed = set.verify_seed(&cert).unwrap();
        assert_eq!(
            seed.as_bytes(),
            &[
                216, 165, 187, 203, 58, 37, 35, 27, 134, 9, 38, 89, 251, 228, 122, 201, 137, 6, 83, 105, 80, 175, 95, 106, 243, 30,
                14, 206, 23, 235, 221, 131
            ]
        );
        cert.dealers[0].supporting_releases.pop();
        assert_eq!(set.verify_seed(&cert).unwrap(), seed);
        cert.dealers[0].supporting_releases.swap(0, 1);
        assert_eq!(set.verify_seed(&cert), Err(Tb1PolynomialError::Support));
        cert.dealers[0].supporting_releases.swap(0, 1);
        let last = cert.dealers.pop().unwrap();
        assert_eq!(set.verify_seed(&cert), Err(Tb1PolynomialError::QualifiedSet));
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
            signed_set(CUT, &fixture_qualification(&roster), &rows, rows.len() - 1),
            Err(Tb1PolynomialError::ReconstructionBudget)
        ));
        let set = signed_set(CUT, &fixture_qualification(&roster), &rows, rows.len()).unwrap();
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
        let later = signed_set(CUT, &fixture_qualification(&roster), &rows, rows.len()).unwrap();
        let mut alternative = later.assemble(6 * 20).unwrap();
        assert!(alternative.dealers.iter().all(|d| d.supporting_releases == [21, 22, 23, 24, 25]));
        assert_eq!(later.verify(&alternative).unwrap(), expected);
        alternative.dealers[0].supporting_releases = cert.dealers[0].supporting_releases.clone();
        assert_eq!(later.verify(&alternative).unwrap(), expected);
        rows.remove(&25);
        let lacking = signed_set(CUT, &fixture_qualification(&roster), &rows, rows.len()).unwrap();
        assert!(matches!(lacking.assemble(336), Err(Tb1PolynomialError::Support)));
    }
}
