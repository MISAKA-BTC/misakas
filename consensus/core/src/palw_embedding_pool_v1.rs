//! **RFC-0001 §2.8 — the pooling an embedding is defined by** (node-only for the local route; the
//! same functions are the definition a claimed embedding class would be held to, RFC-0003's
//! `PalwGenProfileV1::Embedding`).
//!
//! An embedding is the class's final-layer hidden state, pooled over the prompt's positions by one
//! fixed rule and — optionally — scaled to a unit vector by a fixed integer rule. Everything here is
//! integer arithmetic with its rounding stated, so two hosts holding the same hidden states produce
//! the same bytes:
//!
//! * **mean** — the position-wise sum (`i64`) divided by the count, rounded toward −∞
//!   (`div_euclid`);
//! * **last** — the final position's row, unchanged;
//! * **L2 → Q24** — `q[i] = round_half_away(raw[i] · 2^24 / ‖raw‖)` with `‖raw‖ = ⌊√Σ raw[i]²⌋`
//!   (integer square root over `u128`), so a non-zero vector comes out with a norm within
//!   `1/‖raw‖` (the floor) plus `dims/2^24` (the rounding) of `1.0` in relative terms; the zero
//!   vector stays zero.

/// How the positions' rows become one vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PalwEmbeddingPoolV1 {
    Mean = 0,
    LastToken = 1,
}

impl PalwEmbeddingPoolV1 {
    pub fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Mean),
            1 => Some(Self::LastToken),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Mean => "mean",
            Self::LastToken => "last",
        }
    }
}

/// One in Q24 — the fixed point of the normalized vector.
pub const PALW_EMBEDDING_Q24_ONE_V1: i64 = 1 << 24;

/// The most dimensions a pooled vector may have (a bound on what a worker will allocate).
pub const PALW_EMBEDDING_MAX_DIMS_V1: usize = 1 << 16;

/// Pool `rows` (one hidden-state row per position, equal widths) by `pool`. Refused, by name: no
/// rows; rows of different widths; a width of zero or past [`PALW_EMBEDDING_MAX_DIMS_V1`].
pub fn palw_embedding_pool_v1(rows: &[Vec<i32>], pool: PalwEmbeddingPoolV1) -> Result<Vec<i32>, &'static str> {
    let Some(first) = rows.first() else { return Err("no positions to pool") };
    let dims = first.len();
    if dims == 0 || dims > PALW_EMBEDDING_MAX_DIMS_V1 {
        return Err("a hidden row of zero or too many dimensions");
    }
    if rows.iter().any(|r| r.len() != dims) {
        return Err("hidden rows of different widths");
    }
    match pool {
        PalwEmbeddingPoolV1::LastToken => Ok(rows[rows.len() - 1].clone()),
        PalwEmbeddingPoolV1::Mean => {
            let count = rows.len() as i64;
            let mut sums = vec![0i64; dims];
            for row in rows {
                for (sum, value) in sums.iter_mut().zip(row) {
                    *sum = sum.checked_add(i64::from(*value)).ok_or("a pooled sum overflowed")?;
                }
            }
            sums.into_iter().map(|s| i32::try_from(s.div_euclid(count)).map_err(|_| "a pooled mean is outside i32")).collect()
        }
    }
}

/// `⌊√n⌋` over `u128`, exactly.
pub fn palw_isqrt_u128_v1(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    // Newton from above: starts at a power of two ≥ √n and decreases monotonically to ⌊√n⌋.
    let mut x = 1u128 << (128 - n.leading_zeros()).div_ceil(2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// The unit-norm form of `raw` in Q24 (see the module note). All-zero input gives all zeros.
pub fn palw_embedding_l2_q24_v1(raw: &[i32]) -> Vec<i32> {
    let sumsq: u128 = raw.iter().map(|v| (i128::from(*v) * i128::from(*v)) as u128).sum();
    let norm = palw_isqrt_u128_v1(sumsq);
    if norm == 0 {
        return vec![0; raw.len()];
    }
    let norm = norm as i128;
    raw.iter()
        .map(|v| {
            let numerator = i128::from(*v) * i128::from(PALW_EMBEDDING_Q24_ONE_V1);
            // Round half away from zero: add half the divisor in the numerator's direction, truncate.
            let half = if numerator < 0 { -(norm / 2) } else { norm / 2 };
            ((numerator + half) / norm) as i32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_rounds_toward_negative_infinity_and_last_is_the_final_row() {
        let rows = vec![vec![1, -1, 10], vec![2, -2, 11], vec![2, -2, 11]];
        assert_eq!(palw_embedding_pool_v1(&rows, PalwEmbeddingPoolV1::Mean).unwrap(), vec![1, -2, 10]);
        assert_eq!(palw_embedding_pool_v1(&rows, PalwEmbeddingPoolV1::LastToken).unwrap(), vec![2, -2, 11]);
        assert_eq!(palw_embedding_pool_v1(&rows[..1], PalwEmbeddingPoolV1::Mean).unwrap(), vec![1, -1, 10]);
    }

    #[test]
    fn pooling_refuses_by_name() {
        assert_eq!(palw_embedding_pool_v1(&[], PalwEmbeddingPoolV1::Mean), Err("no positions to pool"));
        assert_eq!(palw_embedding_pool_v1(&[vec![]], PalwEmbeddingPoolV1::Mean), Err("a hidden row of zero or too many dimensions"));
        assert_eq!(palw_embedding_pool_v1(&[vec![1, 2], vec![1]], PalwEmbeddingPoolV1::LastToken), Err("hidden rows of different widths"));
        let wide = vec![0i32; PALW_EMBEDDING_MAX_DIMS_V1 + 1];
        assert!(palw_embedding_pool_v1(&[wide], PalwEmbeddingPoolV1::Mean).is_err());
        assert_eq!(PalwEmbeddingPoolV1::from_tag(2), None);
    }

    #[test]
    fn the_integer_square_root_is_exact_at_the_edges() {
        for n in [0u128, 1, 2, 3, 4, 15, 16, 17, (1 << 64) - 1, 1 << 64, u128::MAX] {
            let r = palw_isqrt_u128_v1(n);
            assert!(r * r <= n, "{n}");
            assert!(r.checked_mul(r).is_some());
            assert!(n < (r + 1).checked_mul(r + 1).unwrap_or(u128::MAX) || (r + 1).checked_mul(r + 1).is_none(), "{n}");
        }
        assert_eq!(palw_isqrt_u128_v1(1 << 126), 1 << 63);
    }

    #[test]
    fn a_normalized_vector_has_unit_norm_in_q24_and_the_same_direction() {
        let raw = vec![3_000, -4_000, 0, 12_000, 32_767, -32_767];
        let q = palw_embedding_l2_q24_v1(&raw);
        let norm2: i128 = q.iter().map(|v| i128::from(*v) * i128::from(*v)).sum();
        let one = i128::from(PALW_EMBEDDING_Q24_ONE_V1);
        let err = (norm2 - one * one).abs() as f64 / (one * one) as f64;
        assert!(err < 1e-4, "squared norm is 1.0 in Q24 to within {err} (the floor in the norm: 1/‖raw‖)");
        assert!(q[0] > 0 && q[1] < 0 && q[2] == 0 && q[3] > q[0], "the direction is kept");
        // Scaling the input does not change the output beyond rounding.
        let doubled: Vec<i32> = raw.iter().map(|v| v / 2 * 2).collect();
        let qd = palw_embedding_l2_q24_v1(&doubled);
        assert!(q.iter().zip(&qd).all(|(a, b)| (a - b).abs() < 1 << 12));
        // Zero stays zero; a single non-zero lane is exactly one.
        assert_eq!(palw_embedding_l2_q24_v1(&[0, 0, 0]), vec![0, 0, 0]);
        assert_eq!(palw_embedding_l2_q24_v1(&[0, -7, 0]), vec![0, -(1 << 24), 0]);
        // The extremes do not overflow or panic.
        let big = vec![i32::MAX; 4096];
        let qb = palw_embedding_l2_q24_v1(&big);
        assert!(qb.iter().all(|v| *v > 0));
        assert_eq!(palw_embedding_l2_q24_v1(&[i32::MIN, i32::MIN]).len(), 2);
    }

    #[test]
    fn rounding_is_half_away_from_zero_and_deterministic() {
        // raw = [1, 1]: norm = 1 (floor of sqrt 2), so each lane is exactly 2^24 — the floor in the
        // norm is part of the definition, and it is stated.
        assert_eq!(palw_embedding_l2_q24_v1(&[1, 1]), vec![1 << 24, 1 << 24]);
        assert_eq!(palw_embedding_l2_q24_v1(&[-1, 1]), vec![-(1 << 24), 1 << 24]);
        assert_eq!(palw_embedding_l2_q24_v1(&[5, 2]), palw_embedding_l2_q24_v1(&[5, 2]));
    }
}
