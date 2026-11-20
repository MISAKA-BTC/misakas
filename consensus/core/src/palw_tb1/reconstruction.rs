//! Off-chain reconstruction of one frozen TB1 dealer's publicly released polynomial.
//! The caller authenticates each holder release and its context before supplying the points.
use super::{Tb1DealerPoint, Tb1Field, Tb1PolynomialError, check_points, check_roster};
use kaspa_math::Uint576;
use std::collections::BTreeSet;

const ZERO: Tb1Field = Tb1Field(Uint576::ZERO);

fn modulus() -> Uint576 {
    (Uint576::from_u64(1) << 521) - Uint576::from_u64(1)
}

fn difference(a: Tb1Field, b: Tb1Field) -> Tb1Field {
    Tb1Field(if a.0 >= b.0 { a.0 - b.0 } else { modulus() - (b.0 - a.0) })
}

pub struct Tb1RecoveredDealer {
    pub coefficients: Vec<Tb1Field>,
    pub support: Vec<Tb1DealerPoint>,
}

pub(super) fn matrix_cells(f: usize, errors: usize, releases: usize) -> Result<usize, Tb1PolynomialError> {
    errors
        .checked_mul(2)
        .and_then(|twice| f.checked_add(twice))
        .and_then(|terms| terms.checked_add(2))
        .and_then(|width| width.checked_mul(releases))
        .ok_or(Tb1PolynomialError::ReconstructionBudget)
}

/// Recover the unique degree-at-most-f polynomial supported by at least n-f distinct releases.
/// With m available releases there may be at most m-(n-f) errors. The assembler's matrix budget
/// limits its own work and has no effect on how a node verifies an already assembled certificate.
pub fn reconstruct_dealer(
    roster: &[Tb1Field],
    f: usize,
    releases: &[Tb1DealerPoint],
    max_matrix_cells: usize,
) -> Result<Tb1RecoveredDealer, Tb1PolynomialError> {
    use Tb1PolynomialError::*;
    check_roster(roster, f)?;
    let quorum = roster.len() - f;
    if !(quorum..=roster.len()).contains(&releases.len()) {
        return Err(Support);
    }
    let errors = releases.len() - quorum;
    let numerator_terms = f + errors + 1;
    let cells = matrix_cells(f, errors, releases.len())?;
    if cells > max_matrix_cells {
        return Err(ReconstructionBudget);
    }
    let width = cells / releases.len();
    let mut matrix = Vec::new();
    matrix.try_reserve_exact(cells).map_err(|_| ReconstructionBudget)?;
    matrix.resize(cells, ZERO);
    let mut seen = BTreeSet::new();
    let one = Tb1Field(Uint576::from_u64(1));
    for (release, row) in releases.iter().zip(matrix.chunks_exact_mut(width)) {
        let x = *roster.get(release.holder_slot).ok_or(Support)?;
        if !seen.insert(release.holder_slot) {
            return Err(Support);
        }
        // Berlekamp-Welch: N(x_i) - y_i * sum(E_j x_i^j) = y_i x_i^e,
        // with monic locator E of degree e and deg(N) <= f+e.
        let (numerator, locator_and_rhs) = row.split_at_mut(numerator_terms);
        let mut power = one;
        for entry in numerator {
            *entry = power;
            power = power.mul_add(x, ZERO);
        }
        let mut weighted_power = release.value;
        for entry in &mut locator_and_rhs[..errors] {
            *entry = difference(ZERO, weighted_power);
            weighted_power = weighted_power.mul_add(x, ZERO);
        }
        locator_and_rhs[errors] = weighted_power;
    }

    let mut numerator = solve(&mut matrix, width)?;
    let mut locator = numerator.split_off(numerator_terms);
    locator.push(one);
    let mut coefficients = vec![ZERO; f + 1];
    // When fewer than e releases are bad the locator need not be unique. Every solution still
    // satisfies N = P*E if q consistent points exist, since q > f+e. Check the division and
    // then the actual support before accepting the candidate.
    for degree in (0..=f).rev() {
        let leading = numerator[degree + errors];
        coefficients[degree] = leading;
        for (j, &coefficient) in locator.iter().enumerate() {
            numerator[degree + j] = difference(numerator[degree + j], leading.mul_add(coefficient, ZERO));
        }
    }
    if numerator.iter().any(|&c| c != ZERO) {
        return Err(InconsistentPoint);
    }
    let mut support: Vec<_> = releases
        .iter()
        .copied()
        .filter(|point| coefficients.iter().rev().fold(ZERO, |y, &c| y.mul_add(roster[point.holder_slot], c)) == point.value)
        .collect();
    support.sort_unstable_by_key(|point| point.holder_slot);
    check_points(roster, f, &coefficients, &support)?;
    Ok(Tb1RecoveredDealer { coefficients, support })
}

/// Row-reduce the augmented system. Free locator variables can be zero.
fn solve(matrix: &mut [Tb1Field], width: usize) -> Result<Vec<Tb1Field>, Tb1PolynomialError> {
    let unknowns = width - 1;
    let mut pivots = Vec::new();
    for col in 0..unknowns {
        let rank = pivots.len();
        let Some(offset) = matrix[rank * width..].chunks_exact(width).position(|row| row[col] != ZERO) else { continue };
        for j in col..width {
            matrix.swap(rank * width + j, (rank + offset) * width + j);
        }
        let tail = &mut matrix[rank * width..];
        let (pivot, remaining) = tail.split_at_mut(width);
        let inverse = Tb1Field(pivot[col].0.mod_inverse(modulus()).ok_or(Tb1PolynomialError::InconsistentPoint)?);
        for value in &mut pivot[col..] {
            *value = value.mul_add(inverse, ZERO);
        }
        for row in remaining.chunks_exact_mut(width) {
            let factor = row[col];
            if factor == ZERO {
                continue;
            }
            for j in col..width {
                row[j] = difference(row[j], factor.mul_add(pivot[j], ZERO));
            }
        }
        pivots.push(col);
    }
    if matrix[pivots.len() * width..].chunks_exact(width).any(|row| row[unknowns] != ZERO) {
        return Err(Tb1PolynomialError::InconsistentPoint);
    }
    let mut solution = vec![ZERO; unknowns];
    for (r, &col) in pivots.iter().enumerate().rev() {
        let row = &matrix[r * width..(r + 1) * width];
        let sum = (col + 1..unknowns).fold(ZERO, |sum, j| row[j].mul_add(solution[j], sum));
        solution[col] = difference(row[unknowns], sum);
    }
    Ok(solution)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tb1::{Tb1DealerPolynomial, qualified_constants};

    fn field(x: u64) -> Tb1Field {
        Tb1Field(Uint576::from_u64(x))
    }

    #[test]
    fn every_two_fault_pattern_reconstructs_the_same_quorum_polynomial() {
        let roster: Vec<_> = (1..=7).map(field).collect();
        let coefficients = [difference(ZERO, field(3)), field(4), field(2)];
        let genuine: Vec<_> = roster
            .iter()
            .enumerate()
            .map(|(slot, &x)| Tb1DealerPoint {
                holder_slot: slot,
                value: coefficients.iter().rev().fold(ZERO, |y, &c| y.mul_add(x, c)),
            })
            .collect();
        for omitted in 0u8..128 {
            for altered in 0u8..128 {
                if omitted & altered != 0 || (omitted | altered).count_ones() > 2 {
                    continue;
                }
                let published: Vec<_> = genuine
                    .iter()
                    .rev()
                    .copied()
                    .filter(|p| omitted & (1 << p.holder_slot) == 0)
                    .map(|mut p| {
                        if altered & (1 << p.holder_slot) != 0 {
                            p.value = difference(p.value, field(38));
                        }
                        p
                    })
                    .collect();
                let result = reconstruct_dealer(&roster, 2, &published, 56).unwrap();
                assert_eq!(result.coefficients, coefficients);
                let expected: Vec<_> = genuine.iter().copied().filter(|p| (omitted | altered) & (1 << p.holder_slot) == 0).collect();
                assert_eq!(result.support, expected);
            }
        }
        let recovered = reconstruct_dealer(&roster, 2, &genuine, 56).unwrap();
        let qualified = [0, 2, 3, 4, 6];
        let certs: Vec<_> = qualified
            .iter()
            .rev()
            .map(|&dealer_slot| Tb1DealerPolynomial {
                dealer_slot,
                coefficients: &recovered.coefficients,
                support: &recovered.support,
            })
            .collect();
        assert_eq!(qualified_constants(&roster, 2, &qualified, &certs).unwrap(), qualified.map(|slot| (slot, coefficients[0])));
    }

    #[test]
    fn no_quorum_or_workspace_cannot_produce_a_candidate() {
        let roster: Vec<_> = (1..=7).map(field).collect();
        let mut points: Vec<_> =
            (0..7).map(|slot| Tb1DealerPoint { holder_slot: slot, value: field((slot as u64 + 1).pow(3)) }).collect();
        assert!(reconstruct_dealer(&roster, 2, &points, 56).is_err()); // No quadratic fits five points of x³.
        assert!(matches!(reconstruct_dealer(&roster, 2, &points[..4], 56), Err(Tb1PolynomialError::Support)));
        assert!(matches!(reconstruct_dealer(&roster, 2, &points, 55), Err(Tb1PolynomialError::ReconstructionBudget)));
        points[0] = points[1];
        assert!(matches!(reconstruct_dealer(&roster, 2, &points, 56), Err(Tb1PolynomialError::Support)));
        points[0].holder_slot = 7;
        assert!(matches!(reconstruct_dealer(&roster, 2, &points, 56), Err(Tb1PolynomialError::Support)));
    }
}
