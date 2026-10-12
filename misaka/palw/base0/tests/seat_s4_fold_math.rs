//! **SEAT-S4's review, kept as a regression:** the seat's streamed range fold at the REAL fold level
//! (12) over trees larger than one block, against the consensus range walk — segment cuts for
//! k = 1..8, exact (dense, align 0) and aligned (fold, align 12) proofs, a one-leaf lie at every
//! structurally distinct place, and a forged sibling. The in-crate sweep stops at n <= 1,000, so at
//! level 12 it never exercises a whole-block digest next to a partial edge.

use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_MAX_LEAVES, step_merkle_range_siblings_v1, step_merkle_root_v1};
use kaspa_consensus_core::palw_verification_v2::palw_segment_leaf_range_v2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::fp_capture::{Base0RangeFoldV1, Base0SparseStepTreeV1, base0_range_sibling_count_v1};

fn leaves(n: usize) -> Vec<Hash64> {
    (0..n as u64).map(|i| Hash64::from_u64_word(i.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA5A5)).collect()
}

fn fold_root(
    n: u64,
    first: u64,
    end: u64,
    level: u32,
    ls: &[Hash64],
    siblings: &[Hash64],
    lie: Option<u64>,
) -> Result<Hash64, String> {
    let mut fold = Base0RangeFoldV1::new(n, first, end - first, level)?;
    for i in first..end {
        let h = if Some(i) == lie { Hash64::from_u64_word(0xBAD0_0000 + i) } else { ls[i as usize] };
        fold.push(i, h)?;
    }
    fold.root_v1(siblings, PALW_STEP_LEG_MAX_LEAVES)
}

#[test]
fn streamed_fold_at_level_12_is_the_consensus_walk() {
    const L: u32 = 12;
    let block = 1u64 << L;
    let mut checked = 0usize;
    for n in [4_097u64, 8_193, 20_000, 65_537] {
        let ls = leaves(n as usize);
        let root = step_merkle_root_v1(&ls).expect("a root");
        let tree = Base0SparseStepTreeV1::from_leaves_v1(&ls, L).expect("folds");
        for k in [1u16, 2, 3, 4, 5, 8] {
            for i in 0..k {
                let (s, e) = palw_segment_leaf_range_v2(n, k, i).unwrap();
                if e == s {
                    continue;
                }
                // Exact (a dense retention's proof), folded by the seat at 12.
                let exact = step_merkle_range_siblings_v1(&ls, s as usize, (e - s) as usize).unwrap();
                assert_eq!(base0_range_sibling_count_v1(n, s, e - s), Some(exact.len()), "n={n} k={k} i={i}");
                assert_eq!(fold_root(n, s, e, L, &ls, &exact, None), Ok(root), "n={n} k={k} i={i} exact");
                // Aligned (a fold's proof at its retained level), siblings from the kept vector.
                let (sf, se) = (s - s % block, (e.div_ceil(block) * block).min(n));
                assert_eq!(tree.span_for_range(s, e - s).unwrap(), (sf, se));
                let aligned = tree.aligned_range_siblings_v1(sf, se - sf).expect("an aligned span opens");
                assert_eq!(aligned, step_merkle_range_siblings_v1(&ls, sf as usize, (se - sf) as usize).unwrap());
                assert_eq!(base0_range_sibling_count_v1(n, sf, se - sf), Some(aligned.len()));
                assert_eq!(fold_root(n, sf, se, L, &ls, &aligned, None), Ok(root), "n={n} k={k} i={i} aligned");
                // A lie anywhere in the proven range moves the root: both edges, the first and last
                // leaf of a whole block, and the middle.
                let mut sites = vec![s, e - 1, (s + e) / 2];
                let fb = s.div_ceil(block) * block;
                if fb < e {
                    sites.push(fb);
                    sites.push((fb + block - 1).min(e - 1));
                }
                for lie in sites {
                    assert_ne!(fold_root(n, s, e, L, &ls, &exact, Some(lie)), Ok(root), "n={n} k={k} i={i} exact lie {lie}");
                    assert_ne!(fold_root(n, sf, se, L, &ls, &aligned, Some(lie)), Ok(root), "n={n} k={k} i={i} aligned lie {lie}");
                }
                // Lies in the overhang the aligned proof adds (outside the segment, inside the span).
                if sf < s {
                    assert_ne!(fold_root(n, sf, se, L, &ls, &aligned, Some(sf)), Ok(root));
                }
                if se > e {
                    assert_ne!(fold_root(n, sf, se, L, &ls, &aligned, Some(se - 1)), Ok(root));
                }
                // A forged sibling never roots.
                for (proof, (a, b)) in [(&exact, (s, e)), (&aligned, (sf, se))] {
                    for j in [0usize, proof.len().saturating_sub(1)].into_iter().filter(|j| *j < proof.len()) {
                        let mut forged = proof.clone();
                        forged[j] = Hash64::from_u64_word(0x5B + j as u64);
                        assert_ne!(fold_root(n, a, b, L, &ls, &forged, None), Ok(root));
                    }
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 80, "{checked}");
}
