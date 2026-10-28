//! **RFC-0007 RFC07-3 (user decision 2026-10-15, design note §4.2): the seal-before-commit order of beacon sources** — no source
//! cycle of any length can form among live sources.
//!
//! A live source row is an edge `source producer → target producer` with two times: the target claim's commitment position `c`
//! and the source's seal `s`, where the seal window already forces `c < s` (a seal is accepted only after its target commits).
//! The order rule adds one condition between consecutive edges: **a producer may seal as a source only strictly before the
//! commitment of every live paid claim of its own that is itself a source target** — for edges `a → b` (seal `s₁`, target
//! commit `c₁`) and `b → x` (seal `s₂`), `s₂ < c₁`. Along any chain the seal times then strictly decrease (`s₂ < c₁ < s₁`), so a
//! cycle `a₁ → a₂ → … → a_k → a₁` would need `s₁ > s₂ > … > s_k > s₁` — impossible for every `k ≥ 2`. The two-producer guard
//! that already exists becomes the `k = 2` case of the same rule.
//!
//! The rule is checked whichever edge of a pair is admitted second ([`palw_source_order_admits_v1`] looks both ways), so the
//! admission order does not matter. It reads only identities and times already in state; it judges nothing about computation
//! (PRINCIPLES §8) and grants no credit (RFC-0018 Gate C is untouched). It runs only on the dormant beacon source opt-in paths
//! (`palw_beacon_source_opt_in_v1` / `palw_beacon_source_fee_v1`, `None` on every preset).
use misaka_palw_kernel::hash::Digest;

/// One live source edge, as the order rule reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSourceEdgeV1 {
    /// The producer whose sealed job is the entropy source.
    pub source: Digest,
    /// The producer of the paid claim the source serves.
    pub target_producer: Digest,
    /// When the source's seal was accepted.
    pub sealed_daa: u64,
    /// The target claim's commitment position.
    pub target_commit_daa: u64,
}

/// Why an edge is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSourceOrderRefusalV1 {
    /// The source and the target producer are the same producer.
    SelfSource,
    /// The seal does not come after the target's commitment (the seal window's own order).
    SealNotAfterCommit,
    /// The new source has a live targeted claim committed at or before its seal.
    SourceSealsAfterOwnTargetCommit,
    /// The new target's producer is a live source whose seal is at or after this target's commitment.
    TargetProducerSealsAfterCommit,
}

/// **Admit `new` against the live edges** (module doc). `Ok` iff `new` is no self-source, its seal follows its target's
/// commitment, every live edge targeting a claim of `new.source` was committed strictly after `new.sealed_daa`, and every live
/// edge whose source is `new.target_producer` sealed strictly before `new.target_commit_daa`.
pub fn palw_source_order_admits_v1(
    live: impl IntoIterator<Item = PalwSourceEdgeV1>,
    new: &PalwSourceEdgeV1,
) -> Result<(), PalwSourceOrderRefusalV1> {
    if new.source == new.target_producer {
        return Err(PalwSourceOrderRefusalV1::SelfSource);
    }
    if new.target_commit_daa >= new.sealed_daa {
        return Err(PalwSourceOrderRefusalV1::SealNotAfterCommit);
    }
    for edge in live {
        if edge.target_producer == new.source && new.sealed_daa >= edge.target_commit_daa {
            return Err(PalwSourceOrderRefusalV1::SourceSealsAfterOwnTargetCommit);
        }
        if edge.source == new.target_producer && edge.sealed_daa >= new.target_commit_daa {
            return Err(PalwSourceOrderRefusalV1::TargetProducerSealsAfterCommit);
        }
    }
    Ok(())
}

impl PalwSourceOrderRefusalV1 {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::SelfSource => "a producer cannot be a beacon source for its own paid claim",
            Self::SealNotAfterCommit => "a source seal must follow its target's commitment",
            Self::SourceSealsAfterOwnTargetCommit => {
                "seal-before-commit order: this source has a live targeted claim committed at or before its seal (RFC07-3)"
            }
            Self::TargetProducerSealsAfterCommit => {
                "seal-before-commit order: this target's producer is a live source that sealed at or after this commitment (RFC07-3)"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! SOURCE ONLY (not compiled or executed in this pass).
    use super::*;

    fn p(n: u8) -> Digest {
        [n; 64]
    }
    fn edge(source: u8, target: u8, commit: u64, seal: u64) -> PalwSourceEdgeV1 {
        PalwSourceEdgeV1 { source: p(source), target_producer: p(target), sealed_daa: seal, target_commit_daa: commit }
    }

    /// Admit `edges` one by one in `order`; return how many were admitted (an admitted edge joins the live set).
    fn admit_in_order(edges: &[PalwSourceEdgeV1], order: &[usize]) -> usize {
        let mut live: Vec<PalwSourceEdgeV1> = Vec::new();
        for &i in order {
            if palw_source_order_admits_v1(live.iter().copied(), &edges[i]).is_ok() {
                live.push(edges[i]);
            }
        }
        live.len()
    }

    /// A small deterministic generator (no external randomness in a consensus test).
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
    }

    /// The cycle `0 → 1 → … → k-1 → 0` with random times (each edge's own seal after its commit) is never fully admitted, in any
    /// admission order.
    fn assert_no_cycle_is_ever_complete(k: u8, trials: u32) {
        let mut rng = Lcg(0x5EED ^ u64::from(k));
        for _ in 0..trials {
            let edges: Vec<PalwSourceEdgeV1> = (0..k)
                .map(|i| {
                    let commit = rng.next() % 1_000;
                    let seal = commit + 1 + rng.next() % 1_000;
                    edge(i, (i + 1) % k, commit, seal)
                })
                .collect();
            let mut order: Vec<usize> = (0..k as usize).collect();
            for i in (1..order.len()).rev() {
                order.swap(i, (rng.next() as usize) % (i + 1));
            }
            assert!(admit_in_order(&edges, &order) < k as usize, "a {k}-cycle was fully admitted: {edges:?} in {order:?}");
        }
    }

    #[test]
    fn a_two_cycle_is_refused_whichever_edge_comes_second() {
        // a → b and b → a with every time ordering: never both.
        for (c1, s1, c2, s2) in [(10, 20, 5, 30), (10, 20, 15, 18), (1, 100, 2, 99), (50, 60, 40, 45)] {
            let ab = edge(1, 2, c1, s1);
            let ba = edge(2, 1, c2, s2);
            assert!(admit_in_order(&[ab, ba], &[0, 1]) < 2);
            assert!(admit_in_order(&[ab, ba], &[1, 0]) < 2);
        }
        assert_no_cycle_is_ever_complete(2, 2_000);
    }

    #[test]
    fn a_three_cycle_is_refused_in_every_order() {
        assert_no_cycle_is_ever_complete(3, 2_000);
        // An exhaustive grid of small times over all six admission orders.
        for c in 0..4u64 {
            for d in 0..4u64 {
                let edges = [edge(0, 1, c, c + 1 + d), edge(1, 2, d, d + 2), edge(2, 0, c + d, c + d + 3)];
                for order in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
                    assert!(admit_in_order(&edges, &order) < 3);
                }
            }
        }
    }

    #[test]
    fn an_n_cycle_is_refused_for_every_length() {
        for k in [4u8, 5, 8, 16, 32] {
            assert_no_cycle_is_ever_complete(k, 500);
        }
    }

    #[test]
    fn an_acyclic_chain_with_decreasing_seals_is_admitted_and_self_or_inverted_edges_are_not() {
        // a → b → c → d: each producer sealed (as a source) before its own claim committed.
        let chain = [edge(0, 1, 100, 200), edge(1, 2, 60, 90), edge(2, 3, 20, 50)];
        assert_eq!(admit_in_order(&chain, &[0, 1, 2]), 3);
        assert_eq!(admit_in_order(&chain, &[2, 1, 0]), 3);
        assert_eq!(palw_source_order_admits_v1([], &edge(4, 4, 1, 2)), Err(PalwSourceOrderRefusalV1::SelfSource));
        assert_eq!(palw_source_order_admits_v1([], &edge(4, 5, 2, 2)), Err(PalwSourceOrderRefusalV1::SealNotAfterCommit));
        // The existing two-producer guard is the k = 2 case: b → a refused once a → b is live, whatever the times.
        let live = [edge(1, 2, 10, 20)];
        assert!(palw_source_order_admits_v1(live, &edge(2, 1, 1, 9)).is_err());
        assert!(palw_source_order_admits_v1(live, &edge(2, 1, 30, 40)).is_err());
    }
}
