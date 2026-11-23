//! RFC-0061 private profile for signed qualification, release and seed reconstruction.
//! The native owner supplies the authenticated roster, VSS outcomes, original
//! timing plan, branch positions and resource limits for the frozen purpose.
pub mod exec_close_carrier;
pub mod exec_registration;
pub mod freeze;
pub mod moe2_exec_close;
pub mod panel_reply_carrier;
pub mod qf1_original_carrier;
pub mod qualification;
pub mod reconstruction;
pub mod release_batch_carrier;
pub mod releases;
pub mod signatures;

use kaspa_math::{Uint576, Uint3072};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tb1Field(Uint576);

impl Tb1Field {
    pub fn from_bytes(bytes: &[u8; 66]) -> Option<Self> {
        // p has its low 521 bits set. Both p itself and nonzero padding are noncanonical.
        if bytes[0] > 1 || (bytes[0] == 1 && bytes[1..].iter().all(|b| *b == 255)) {
            return None;
        }
        Uint576::from_be_bytes_var(bytes).ok().map(Self)
    }

    pub fn to_bytes(self) -> [u8; 66] {
        self.0.to_be_bytes()[6..].try_into().expect("521-bit field encoding")
    }

    fn widened(self) -> Uint3072 {
        let mut wide = Uint3072::ZERO;
        wide.0[..9].copy_from_slice(&self.0.0);
        wide
    }

    fn mul_add(self, x: Self, coefficient: Self) -> Self {
        let p = (Uint3072::from_u64(1) << 521) - Uint3072::from_u64(1);
        let product = self.widened() * x.widened() + coefficient.widened();
        // For canonical inputs product < p². A single fold at bit 521 and
        // at most one subtraction give its unique representative modulo p.
        let folded = (product & p) + (product >> 521);
        let reduced = if folded >= p { folded - p } else { folded };
        let mut low = Uint576::ZERO;
        low.0.copy_from_slice(&reduced.0[..9]);
        Self(low)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tb1DealerPoint {
    pub holder_slot: usize,
    pub value: Tb1Field,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tb1PolynomialError {
    Threshold,
    Coefficients,
    Roster,
    Support,
    InconsistentPoint,
    QualifiedSet,
    PreparationContext,
    PreparationBudget,
    ReleaseContext,
    ReleaseTiming,
    ReconstructionBudget,
}

/// Check the disclosed candidate polynomial at every signed supporting point.
/// Coordinates are those of the frozen roster. The same share cannot count for
/// two slots, and all submitted support must agree, including points beyond q.
pub fn check_dealer_polynomial(
    roster: &[Tb1Field],
    f: usize,
    coefficients: &[Tb1Field],
    support: &[Tb1DealerPoint],
) -> Result<(), Tb1PolynomialError> {
    check_roster(roster, f)?;
    check_points(roster, f, coefficients, support)
}

fn check_roster(roster: &[Tb1Field], f: usize) -> Result<(), Tb1PolynomialError> {
    use Tb1PolynomialError::*;
    let n = f.checked_mul(3).and_then(|m| m.checked_add(1)).ok_or(Threshold)?;
    if roster.len() != n {
        return Err(Threshold);
    }
    let mut coordinates = BTreeSet::new();
    if roster.iter().any(|x| x.0 == Uint576::ZERO || !coordinates.insert(*x)) {
        return Err(Roster);
    }
    Ok(())
}

fn check_points(
    roster: &[Tb1Field],
    f: usize,
    coefficients: &[Tb1Field],
    support: &[Tb1DealerPoint],
) -> Result<(), Tb1PolynomialError> {
    use Tb1PolynomialError::*;
    if coefficients.len() != f + 1 {
        return Err(Coefficients);
    }
    if support.len() < roster.len() - f || support.len() > roster.len() {
        return Err(Support);
    }
    let mut slots = BTreeSet::new();
    for point in support {
        let x = *roster.get(point.holder_slot).ok_or(Support)?;
        if !slots.insert(point.holder_slot) {
            return Err(Support);
        }
        let actual = coefficients.iter().rev().fold(Tb1Field(Uint576::ZERO), |value, &c| value.mul_add(x, c));
        if actual != point.value {
            return Err(InconsistentPoint);
        }
    }
    Ok(())
}

// Caller has already checked the roster and its threshold.
fn check_qualified(roster: &[Tb1Field], f: usize, qualified: &[usize]) -> Result<(), Tb1PolynomialError> {
    use Tb1PolynomialError::QualifiedSet;
    if qualified.len() < roster.len() - f
        || qualified.len() > roster.len()
        || !qualified.is_sorted_by(|a, b| a < b)
        || qualified.last().is_none_or(|&slot| slot >= roster.len())
    {
        return Err(QualifiedSet);
    }
    Ok(())
}

pub struct Tb1DealerPolynomial<'a> {
    pub dealer_slot: usize,
    pub coefficients: &'a [Tb1Field],
    pub support: &'a [Tb1DealerPoint],
}

/// Project the constants for the entire previously qualified dealer set.
/// The caller resolves Q and each point from authenticated releases at the
/// same native freeze. Arrival order and supporting subsets do not enter the output.
pub fn qualified_constants(
    roster: &[Tb1Field],
    f: usize,
    qualified: &[usize],
    dealers: &[Tb1DealerPolynomial<'_>],
) -> Result<Vec<(usize, Tb1Field)>, Tb1PolynomialError> {
    use Tb1PolynomialError::QualifiedSet;
    check_roster(roster, f)?;
    check_qualified(roster, f, qualified)?;
    if dealers.len() != qualified.len() {
        return Err(QualifiedSet);
    }
    let mut by_slot = BTreeMap::new();
    for dealer in dealers {
        if by_slot.insert(dealer.dealer_slot, dealer).is_some() {
            return Err(QualifiedSet);
        }
    }
    qualified
        .iter()
        .map(|slot| {
            let dealer = by_slot.get(slot).ok_or(QualifiedSet)?;
            check_points(roster, f, dealer.coefficients, dealer.support)?;
            Ok((*slot, dealer.coefficients[0]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn small(v: u64) -> Tb1Field {
        Tb1Field(Uint576::from_u64(v))
    }

    #[test]
    fn canonical_521_bit_field_and_wide_product() {
        let mut bytes = [255; 66];
        bytes[0] = 1;
        assert!(Tb1Field::from_bytes(&bytes).is_none());
        bytes[65] = 254;
        let minus_one = Tb1Field::from_bytes(&bytes).unwrap();
        assert_eq!(minus_one.to_bytes(), bytes);
        assert_eq!(minus_one.mul_add(minus_one, small(0)), small(1));
        assert_eq!(minus_one.mul_add(minus_one, minus_one), small(0));
        bytes[0] = 2;
        assert!(Tb1Field::from_bytes(&bytes).is_none());
    }

    #[test]
    fn whole_qualified_set_is_bound_once_in_frozen_order() {
        let roster: Vec<_> = (1..=4).map(small).collect();
        let coefficients = [small(7), small(2)];
        let points: Vec<_> = (0..4).map(|slot| Tb1DealerPoint { holder_slot: slot, value: small(9 + 2 * slot as u64) }).collect();
        let dealer = |slot, skip| Tb1DealerPolynomial { dealer_slot: slot, coefficients: &coefficients, support: &points[skip..] };
        let fixed = [0, 2, 3];
        let reversed = [dealer(3, 0), dealer(2, 1), dealer(0, 0)];
        assert_eq!(qualified_constants(&roster, 1, &fixed, &reversed).unwrap(), fixed.map(|slot| (slot, small(7))));
        assert_eq!(qualified_constants(&roster, 1, &fixed, &reversed[..2]), Err(Tb1PolynomialError::QualifiedSet));
        for invalid in [[0, 0, 3], [0, 2, 4], [2, 0, 3], [0, 1, 3]] {
            assert_eq!(qualified_constants(&roster, 1, &invalid, &reversed), Err(Tb1PolynomialError::QualifiedSet));
        }
        let duplicate = [dealer(0, 1), dealer(0, 0), dealer(3, 1)];
        assert_eq!(qualified_constants(&roster, 1, &fixed, &duplicate), Err(Tb1PolynomialError::QualifiedSet));
    }

    #[test]
    fn seven_slots_require_five_distinct_consistent_public_points() {
        let roster: Vec<_> = (1..=7).map(small).collect();
        let poly = [small(3), small(4), small(2)];
        let mut points: Vec<_> = (0..6)
            .map(|slot| {
                let x = slot as u64 + 1;
                Tb1DealerPoint { holder_slot: slot, value: small(3 + 4 * x + 2 * x * x) }
            })
            .collect();
        assert_eq!(check_dealer_polynomial(&roster, 2, &poly, &points[..3]), Err(Tb1PolynomialError::Support));
        assert!(check_dealer_polynomial(&roster, 2, &poly, &points[..5]).is_ok());
        points[5].value = small(42);
        assert_eq!(check_dealer_polynomial(&roster, 2, &poly, &points), Err(Tb1PolynomialError::InconsistentPoint));
        points[5] = points[0];
        assert_eq!(check_dealer_polynomial(&roster, 2, &poly, &points), Err(Tb1PolynomialError::Support));
    }
}
