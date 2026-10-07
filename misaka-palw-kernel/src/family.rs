//! **Constraint families, not model-brand kernels** (RFC-0005 §K.3).
//!
//! Every PALW-TIR v1 primitive belongs to exactly one family. A kernel descriptor lists the families it
//! implements, each with ONE checker and ONE terminal court; a plan cannot choose a weaker checker, and
//! a family a descriptor does not list is `KERNEL_EXTENSION_REQUIRED` — never a successful registration.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::Prim;

/// The reusable families. Discriminants are this crate's spelling, not allocated wire ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ConstraintFamilyV1 {
    /// Reshape, transpose, slice, concat, broadcast, iota: exact index relations.
    Structure = 0,
    /// Cast, add, sub, mul, reduce-sum: exact integer arithmetic that must fit its declared type.
    ExactArithmetic = 1,
    /// `MatMul`: dense, attention and executed-expert products.
    DenseMatrix = 2,
    /// Div (rounding), clamp (saturation), log2-floor: quantization, range and rounding.
    QuantRange = 3,
    /// IntExp, IntRsqrt, IntLn: the fixed-iteration integer transcendentals.
    Nonlinear = 4,
    /// Compare, select, TopK, reduce-max, gather: selection, routing, dynamic indexing, embeddings.
    Selection = 5,
    /// StateWrite, HistAppend: recurrent and history state, authenticated across positions.
    RecurrentState = 6,
    /// Canonical media inputs/outputs and cross-stage pipeline commitments (RFC-0003). No TIR v1
    /// primitive is in it; a v2 pipeline class needs it ([`crate::pipeline`]), and K2-TIR-v3 implements it.
    MediaPipeline = 7,
}

impl ConstraintFamilyV1 {
    pub const ALL: [ConstraintFamilyV1; 8] = [
        Self::Structure,
        Self::ExactArithmetic,
        Self::DenseMatrix,
        Self::QuantRange,
        Self::Nonlinear,
        Self::Selection,
        Self::RecurrentState,
        Self::MediaPipeline,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Structure => "structure",
            Self::ExactArithmetic => "exact-arithmetic",
            Self::DenseMatrix => "dense-matrix",
            Self::QuantRange => "quant-range",
            Self::Nonlinear => "nonlinear",
            Self::Selection => "selection",
            Self::RecurrentState => "recurrent-state",
            Self::MediaPipeline => "media-pipeline",
        }
    }
}

/// The family of a TIR v1 primitive.
pub fn family_of_prim(prim: &Prim) -> ConstraintFamilyV1 {
    use ConstraintFamilyV1 as F;
    match prim {
        Prim::Reshape | Prim::Transpose { .. } | Prim::Slice { .. } | Prim::Concat { .. } | Prim::Broadcast | Prim::Iota { .. } => {
            F::Structure
        }
        Prim::Cast | Prim::Add | Prim::Sub | Prim::Mul | Prim::ReduceSum { .. } => F::ExactArithmetic,
        Prim::MatMul => F::DenseMatrix,
        Prim::Div { .. } | Prim::Clamp { .. } | Prim::Log2Floor => F::QuantRange,
        Prim::IntExp | Prim::IntRsqrt | Prim::IntLn => F::Nonlinear,
        Prim::Compare { .. } | Prim::Select | Prim::TopK { .. } | Prim::ReduceMax { .. } | Prim::Gather { .. } => F::Selection,
        Prim::StateWrite { .. } | Prim::HistAppend { .. } => F::RecurrentState,
    }
}

/// How a relation is checked on the normal path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum CheckerIdV1 {
    /// The relation is recomputed exactly from its opened, authenticated inputs (RFC-0011 §15.1:
    /// "small cheap constraints may be checked exactly"). No error term.
    ExactRecompute = 1,
    /// Freivalds over GF(2^127 − 1) with post-commit public vectors, `t` repetitions per instance.
    FreivaldsM127 = 2,
    /// Exact recompute of the write/append from the entry state, which wiring authenticates as the
    /// predecessor position's committed exit (read-after-write order, initialization, continuity).
    StateContinuity = 3,
    /// **The multi-modulus dense relation** (K2-TIR-v2): Freivalds modulo the fewest of `2^127 − 1`, `2^107 − 1`, `2^89 − 1`
    /// whose product exceeds the relation's integer error span, each modulus with its own post-commit vectors, `t` repetitions.
    /// A nonzero integer error below the product is nonzero modulo one of them (CRT), so per repetition a false product passes
    /// with probability at most `1/(2^89 − 1)`.
    FreivaldsCrtV2 = 4,
    /// **A media-pipeline edge** (K2-TIR-v3): a stage's committed input is recomputed exactly from its binding — a job value
    /// (scalars, token templates, counts, a canonical image), an earlier stage's committed output rows or final value (authenticated
    /// copy and zero pad), or RFC-0003's `R` — and must lie in the input's declared interval. No error term.
    EdgeRecompute = 5,
}

impl CheckerIdV1 {
    pub const fn is_probabilistic(self) -> bool {
        matches!(self, Self::FreivaldsM127 | Self::FreivaldsCrtV2)
    }

    /// What one repetition of the check buys against a fixed false instance, in bits (the worst modulus it may use); 0 for an
    /// exact checker.
    pub const fn per_repetition_bits(self) -> u32 {
        match self {
            Self::FreivaldsM127 => crate::field::FIELD_BITS,
            Self::FreivaldsCrtV2 => crate::field::mersenne_bits(crate::field::MODULI_V2[crate::field::MODULI_V2.len() - 1]),
            Self::ExactRecompute | Self::StateContinuity | Self::EdgeRecompute => 0,
        }
    }
}

/// The terminal court a localized fault is tried in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum CourtIdV1 {
    /// Recompute one primitive instance from its opened inputs and compare with its opened output.
    InstanceRecompute = 1,
    /// One scalar of a product: the dot product of one row and one column, `k` multiply-adds.
    MatMulScalar = 2,
    /// One stage input at one position: its binding recomputed from public job facts and the authenticated upstream values.
    EdgeRecompute = 3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tir_v1_primitive_has_one_family_and_none_is_media() {
        // One of each tag 0..=24, as the IR's own table lists them.
        let all = [
            Prim::Reshape,
            Prim::Transpose { perm: vec![1, 0] },
            Prim::Slice { axis: 0, start: 0 },
            Prim::Concat { axis: 0 },
            Prim::Broadcast,
            Prim::Iota { axis: 0, start: 0, step: 1 },
            Prim::Gather { axis: 0, batch_dims: 0 },
            Prim::Cast,
            Prim::Add,
            Prim::Sub,
            Prim::Mul,
            Prim::MatMul,
            Prim::ReduceSum { axis: 0 },
            Prim::ReduceMax { axis: 0 },
            Prim::Div { rule: misaka_palw_tir::Rounding::Floor },
            Prim::Clamp { lo: 0, hi: 1 },
            Prim::Log2Floor,
            Prim::IntExp,
            Prim::IntRsqrt,
            Prim::IntLn,
            Prim::Compare { cmp: misaka_palw_tir::Cmp::Eq },
            Prim::Select,
            Prim::TopK { axis: 0, k: 1 },
            Prim::StateWrite { state: 0 },
            Prim::HistAppend { state: 0 },
        ];
        for (tag, p) in all.iter().enumerate() {
            assert_eq!(p.tag() as usize, tag);
            assert_ne!(family_of_prim(p), ConstraintFamilyV1::MediaPipeline);
        }
    }
}
