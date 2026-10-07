//! **Bind first, then draw** (RFC-0011 §15.3).
//!
//! The challenge seed is a function of everything the producer committed — the plan, the class binding,
//! the claim, the evidence root — and of a beacon that exists only after those commitments are bound.
//! A relation's vectors are drawn from the seed under a label naming the relation instance and the
//! repetition, so two relations, two positions or two repetitions never share a vector.
//!
//! Sampling is unbiased: `e` bits are read for GF(2^e − 1) and the single value `2^e − 1` (= p) is rejected.
//!
//! The beacon comes from [`crate::beacon`]: a finalized window of selected-chain blocks strictly after the claim's commitments,
//! with reorg rebinds counted and grinding turned into attempts. Its review as an unbiased beacon remains an activation gate of
//! RFC-0011 §15.3.

use crate::field::{FieldElemV1, Fp};
use crate::hash::{Digest, finish, keyed};

pub const CHALLENGE_SEED_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/challenge-seed/v1";
pub const CHALLENGE_STREAM_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/challenge-stream/v1";

/// What the seed is bound to. Every field is a commitment fixed before the beacon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChallengeBindingV1 {
    pub network_domain: Digest,
    pub claim_id: Digest,
    pub class_binding_id: Digest,
    pub plan_root: Digest,
    pub evidence_root: Digest,
    /// The post-commit beacon value.
    pub beacon: Digest,
}

impl ChallengeBindingV1 {
    pub fn seed(&self) -> Digest {
        let mut s = keyed(CHALLENGE_SEED_DOMAIN_V1);
        for part in [&self.network_domain, &self.claim_id, &self.class_binding_id, &self.plan_root, &self.evidence_root, &self.beacon]
        {
            s.update(part);
        }
        finish(s)
    }
}

/// The label of one relation instance's vectors: position, occurrence, node, repetition, batch slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChallengeLabelV1 {
    /// 0: one relation instance at one position; 1: a scope's cross-token batch of one relation (RFC-0007 §V.3), whose `position`
    /// is the scope's first position and `slice` the scope's index.
    pub kind: u8,
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub repetition: u8,
    pub slice: u32,
}

/// A stream of field elements for one label.
pub struct ChallengeStreamV1 {
    seed: Digest,
    label: ChallengeLabelV1,
    counter: u64,
    buf: Vec<u128>,
}

impl ChallengeStreamV1 {
    pub fn new(seed: Digest, label: ChallengeLabelV1) -> Self {
        Self { seed, label, counter: 0, buf: Vec::new() }
    }

    fn refill(&mut self) {
        let mut s = keyed(CHALLENGE_STREAM_DOMAIN_V1);
        s.update(&self.seed);
        s.update(&[self.label.kind]);
        s.update(&self.label.position.to_le_bytes());
        s.update(&self.label.occurrence.to_le_bytes());
        s.update(&self.label.node.to_le_bytes());
        s.update(&[self.label.repetition]);
        s.update(&self.label.slice.to_le_bytes());
        s.update(&self.counter.to_le_bytes());
        self.counter += 1;
        let block = finish(s);
        // Four 128-bit words per block, consumed in order; a field of 2^e − 1 keeps the low e bits.
        for chunk in block.chunks_exact(16).rev() {
            let mut w = [0u8; 16];
            w.copy_from_slice(chunk);
            self.buf.push(u128::from_le_bytes(w));
        }
    }

    /// One element of GF(2^e − 1): `e` bits read, the single value `p` rejected (no modulo bias).
    pub fn next_in<F: FieldElemV1>(&mut self) -> F {
        loop {
            if self.buf.is_empty() {
                self.refill();
            }
            let v = self.buf.pop().expect("refilled");
            if let Some(f) = F::of_word(v) {
                return f;
            }
        }
    }

    pub fn next_fp(&mut self) -> Fp {
        self.next_in()
    }

    pub fn vector_in<F: FieldElemV1>(&mut self, n: usize) -> Vec<F> {
        (0..n).map(|_| self.next_in()).collect()
    }

    pub fn vector(&mut self, n: usize) -> Vec<Fp> {
        self.vector_in(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(evidence: u8) -> ChallengeBindingV1 {
        ChallengeBindingV1 {
            network_domain: [1; 64],
            claim_id: [2; 64],
            class_binding_id: [3; 64],
            plan_root: [4; 64],
            evidence_root: [evidence; 64],
            beacon: [6; 64],
        }
    }

    #[test]
    fn the_seed_moves_with_every_commitment_and_labels_separate_streams() {
        assert_ne!(binding(5).seed(), binding(7).seed(), "a different evidence root is a different challenge");
        let l = ChallengeLabelV1 { kind: 0, position: 0, occurrence: 1, node: 2, repetition: 0, slice: 0 };
        let a = ChallengeStreamV1::new(binding(5).seed(), l).vector(9);
        let b = ChallengeStreamV1::new(binding(5).seed(), ChallengeLabelV1 { repetition: 1, ..l }).vector(9);
        assert_ne!(a, b);
        assert_eq!(a, ChallengeStreamV1::new(binding(5).seed(), l).vector(9), "deterministic: every node draws the same vector");
        assert!(a.iter().all(|f| f.value() < crate::field::P));
        let c: Vec<crate::field::F89> = ChallengeStreamV1::new(binding(5).seed(), l).vector_in(9);
        assert!(c.iter().all(|f| f.value() < crate::field::F89::MODULUS));
    }
}
