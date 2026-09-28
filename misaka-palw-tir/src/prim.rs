//! The closed primitive set of PALW-TIR v1 (spec 04b §4, PALW-TIR-1).
//!
//! **Twenty-five primitives, every one named and defined by mathematics.** No primitive is named or
//! defined by an architecture or a model; the composite operations every family needs (RMSNorm,
//! LayerNorm, softmax, SiLU, RoPE, attention, the gated delta rule, the selective scan, WKV, the
//! MoE combine, BASE-0's `Requantize`, A16's narrowing …) are library subgraphs over these.
//!
//! **The Borsh tag of each variant is its wire value and is frozen** (`Prim::tag`, pinned by
//! `the_primitive_tags_are_frozen`). A new primitive is a new tag at the end and a new
//! `prim_set_id`; an edit to an existing one is never allowed.

use borsh::{BorshDeserialize, BorshSerialize};

/// The rounding rule of a `Div` (spec 04b §5.3). The closed set of lossy rules is
/// `{Floor, HalfUp, HalfAwayFromZero}` for division and `Saturate` for `Clamp`/`StateWrite`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum Rounding {
    /// tag 0: `floor(x / d)` — the arithmetic shift of 04a C1's internal `>> k`.
    Floor,
    /// tag 1: `floor(x / d + 1/2)` — half toward +∞; with `d = 2^31` this is gemmlowp's SRDHM rounding (04a C2).
    HalfUp,
    /// tag 2: `sign(x) · floor(|x| / d + 1/2)` — 04a C1's `RoundingShiftRight` at every width.
    HalfAwayFromZero,
}

impl Rounding {
    pub const ALL: [Rounding; 3] = [Rounding::Floor, Rounding::HalfUp, Rounding::HalfAwayFromZero];
    pub const fn name(self) -> &'static str {
        match self {
            Rounding::Floor => "floor",
            Rounding::HalfUp => "half_up",
            Rounding::HalfAwayFromZero => "half_away_from_zero",
        }
    }
}

/// The relation of a `Compare`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum Cmp {
    /// tag 0
    Eq,
    /// tag 1
    Ne,
    /// tag 2
    Lt,
    /// tag 3
    Le,
    /// tag 4
    Gt,
    /// tag 5
    Ge,
}

impl Cmp {
    pub const ALL: [Cmp; 6] = [Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge];
    pub const fn name(self) -> &'static str {
        match self {
            Cmp::Eq => "eq",
            Cmp::Ne => "ne",
            Cmp::Lt => "lt",
            Cmp::Le => "le",
            Cmp::Gt => "gt",
            Cmp::Ge => "ge",
        }
    }
    pub fn holds(self, a: i128, b: i128) -> bool {
        match self {
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
            Cmp::Gt => a > b,
            Cmp::Ge => a >= b,
        }
    }
}

/// The kind of a primitive (RFC §3.3). It decides what an implementation may do around it: only
/// kinds `S` and `E` may sit inside an order-free region (PALW-TIR-4, spec 04b §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimKind {
    /// Structure and indexing: no arithmetic, exact.
    S,
    /// Exact arithmetic: the mathematical result, which must fit the declared type.
    E,
    /// Lossy: loses information by a named rule.
    L,
    /// Integer transcendental: a fixed-iteration algorithm, lossy.
    T,
    /// Selection and comparison.
    X,
    /// Bounded state over the position scan.
    State,
}

/// A primitive with its attributes. Every attribute a primitive could also read from the node's
/// declared `out` type is NOT an attribute (the target shape of `Reshape`/`Broadcast`, the shape
/// of `Iota`, the type of `Cast`): one fact, one place, one encoding.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum Prim {
    // ---- S: structure -------------------------------------------------------------------------
    /// tag 0. Row-major reinterpretation into `out.shape`.
    Reshape,
    /// tag 1. `out.shape[i] = in.shape[perm[i]]`.
    Transpose { perm: Vec<u8> },
    /// tag 2. `out.shape[axis]` elements of `axis` starting at `start`.
    Slice { axis: u8, start: u32 },
    /// tag 3. Two to eight inputs joined along `axis`.
    Concat { axis: u8 },
    /// tag 4. Numpy expansion of size-1 dimensions (and missing leading ones) to `out.shape`.
    Broadcast,
    /// tag 5. `out[i] = start + step · i[axis]`, no inputs.
    Iota { axis: u8, start: i64, step: i64 },
    /// tag 6. Gather along `axis` of the data by an index tensor; the first `batch_dims`
    /// dimensions of data and indices are shared.
    Gather { axis: u8, batch_dims: u8 },
    // ---- E: exact arithmetic ------------------------------------------------------------------
    /// tag 7. Exact conversion to `out.dtype`; the value must fit.
    Cast,
    /// tag 8. Elementwise `a + b`, broadcasting.
    Add,
    /// tag 9. Elementwise `a − b`, broadcasting.
    Sub,
    /// tag 10. Elementwise `a · b`, broadcasting.
    Mul,
    /// tag 11. `[.., M, K] × [.., K, N] → [.., M, N]`, batch dimensions broadcast; exact
    /// accumulation in `out.dtype`, every partial sum in every order inside it.
    MatMul,
    /// tag 12. Exact sum along `axis` (kept, size 1).
    ReduceSum { axis: u8 },
    /// tag 13. Maximum along `axis` (kept, size 1).
    ReduceMax { axis: u8 },
    // ---- L: lossy -----------------------------------------------------------------------------
    /// tag 14. `round_rule(x / d)` for `d ≥ 1`, broadcasting.
    Div { rule: Rounding },
    /// tag 15. Saturate into `[lo, hi]` — the only narrowing that may lose information.
    Clamp { lo: i64, hi: i64 },
    /// tag 16. `floor(log2 x)` for `x ≥ 1`, `−1` for `x ≤ 0`.
    Log2Floor,
    // ---- T: integer transcendentals (Q24) -----------------------------------------------------
    /// tag 17. `exp(x/2^24)·2^24` for `x ≤ 0` by 04a F1 (positive inputs clamp to 0).
    IntExp,
    /// tag 18. `2^24/√(v/2^24)` by 04a F2 (`v ≤ 0` gives 0).
    IntRsqrt,
    /// tag 19. `ln(x/2^24)·2^24` by 04a (ADR-0052 D) (`x ≤ 0` gives 0).
    IntLn,
    // ---- X: selection -------------------------------------------------------------------------
    /// tag 20. Elementwise relation, `1` or `0` as `i8`, broadcasting.
    Compare { cmp: Cmp },
    /// tag 21. `c ≠ 0 ? a : b`, broadcasting.
    Select,
    /// tag 22. The indices of the `k` largest along `axis` — ties to the lowest index — returned
    /// in ascending index order (ADR-0052 B).
    TopK { axis: u8, k: u32 },
    // ---- State --------------------------------------------------------------------------------
    /// tag 23. Saturate the input into `Fixed` state `state`'s range; the value the next position
    /// reads.
    StateWrite { state: u16 },
    /// tag 24. Append the input row to `Hist` state `state`; the output is the window,
    /// `[H, ..row]`, this position's row last.
    HistAppend { state: u16 },
}

/// Names and tags of the v1 set, in tag order. The prim-set descriptor (spec 04b §4.1) is built
/// from this table; a consensus crate hashes the descriptor into `prim_set_id`.
pub const PRIM_NAMES_V1: [&str; 25] = [
    "Reshape",
    "Transpose",
    "Slice",
    "Concat",
    "Broadcast",
    "Iota",
    "Gather",
    "Cast",
    "Add",
    "Sub",
    "Mul",
    "MatMul",
    "ReduceSum",
    "ReduceMax",
    "Div",
    "Clamp",
    "Log2Floor",
    "IntExp",
    "IntRsqrt",
    "IntLn",
    "Compare",
    "Select",
    "TopK",
    "StateWrite",
    "HistAppend",
];

/// The canonical bytes a consensus crate hashes into `prim_set_id` (keyed BLAKE2b-512 under the
/// caller's domain). They name the set, the spec revision whose text defines the semantics, and
/// every primitive in tag order.
pub fn prim_set_descriptor_v1() -> Vec<u8> {
    let mut s = String::from("palw-tir/v1/spec=04b-tensor-ir/rev1/q=24/prims=");
    for (i, n) in PRIM_NAMES_V1.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{i}:{n}"));
    }
    s.into_bytes()
}

impl Prim {
    pub fn tag(&self) -> u8 {
        match self {
            Prim::Reshape => 0,
            Prim::Transpose { .. } => 1,
            Prim::Slice { .. } => 2,
            Prim::Concat { .. } => 3,
            Prim::Broadcast => 4,
            Prim::Iota { .. } => 5,
            Prim::Gather { .. } => 6,
            Prim::Cast => 7,
            Prim::Add => 8,
            Prim::Sub => 9,
            Prim::Mul => 10,
            Prim::MatMul => 11,
            Prim::ReduceSum { .. } => 12,
            Prim::ReduceMax { .. } => 13,
            Prim::Div { .. } => 14,
            Prim::Clamp { .. } => 15,
            Prim::Log2Floor => 16,
            Prim::IntExp => 17,
            Prim::IntRsqrt => 18,
            Prim::IntLn => 19,
            Prim::Compare { .. } => 20,
            Prim::Select => 21,
            Prim::TopK { .. } => 22,
            Prim::StateWrite { .. } => 23,
            Prim::HistAppend { .. } => 24,
        }
    }

    pub fn name(&self) -> &'static str {
        PRIM_NAMES_V1[self.tag() as usize]
    }

    pub fn kind(&self) -> PrimKind {
        match self.tag() {
            0..=6 => PrimKind::S,
            7..=13 => PrimKind::E,
            14..=16 => PrimKind::L,
            17..=19 => PrimKind::T,
            20..=22 => PrimKind::X,
            _ => PrimKind::State,
        }
    }

    /// How many inputs the primitive takes: `(min, max)`.
    pub fn arity(&self) -> (usize, usize) {
        match self {
            Prim::Iota { .. } => (0, 0),
            Prim::Concat { .. } => (2, 8),
            Prim::Gather { .. } | Prim::Add | Prim::Sub | Prim::Mul | Prim::MatMul | Prim::Div { .. } | Prim::Compare { .. } => (2, 2),
            Prim::Select => (3, 3),
            _ => (1, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_of_each() -> Vec<Prim> {
        vec![
            Prim::Reshape,
            Prim::Transpose { perm: vec![1, 0] },
            Prim::Slice { axis: 0, start: 1 },
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
            Prim::Div { rule: Rounding::Floor },
            Prim::Clamp { lo: -1, hi: 1 },
            Prim::Log2Floor,
            Prim::IntExp,
            Prim::IntRsqrt,
            Prim::IntLn,
            Prim::Compare { cmp: Cmp::Lt },
            Prim::Select,
            Prim::TopK { axis: 0, k: 1 },
            Prim::StateWrite { state: 0 },
            Prim::HistAppend { state: 0 },
        ]
    }

    /// The wire value of every primitive is its Borsh tag, equal to [`Prim::tag`], in the order
    /// [`PRIM_NAMES_V1`] lists. A reordering of the enum would move every class id; this is where
    /// it fails instead.
    #[test]
    fn the_primitive_tags_are_frozen() {
        let all = one_of_each();
        assert_eq!(all.len(), PRIM_NAMES_V1.len());
        for (i, p) in all.iter().enumerate() {
            assert_eq!(p.tag() as usize, i);
            assert_eq!(borsh::to_vec(p).unwrap()[0], p.tag(), "{} encodes under its tag", p.name());
            assert_eq!(p.name(), PRIM_NAMES_V1[i]);
        }
        assert_eq!(borsh::to_vec(&Rounding::HalfAwayFromZero).unwrap(), vec![2]);
        assert_eq!(borsh::to_vec(&Cmp::Ge).unwrap(), vec![5]);
        // An unknown tag is refused, not mapped.
        assert!(borsh::from_slice::<Prim>(&[25]).is_err());
        assert!(borsh::from_slice::<Rounding>(&[3]).is_err());
    }

    /// No primitive name is an architecture's (freeze criterion 1). The check is a denylist of the
    /// words a model-specific op would carry; it is a tripwire, not the argument (spec 04b §4).
    #[test]
    fn no_primitive_is_named_after_a_model() {
        let banned = [
            "qwen", "llama", "mamba", "rwkv", "gdn", "deepseek", "kimi", "gemma", "mistral", "attn", "rope", "moe", "router", "silu",
            "norm", "softmax", "scan", "a16", "base0",
        ];
        for n in PRIM_NAMES_V1 {
            let l = n.to_ascii_lowercase();
            assert!(!banned.iter().any(|b| l.contains(b)), "{n} reads like a model op");
        }
    }

    #[test]
    fn the_descriptor_lists_every_primitive_in_tag_order() {
        let d = String::from_utf8(prim_set_descriptor_v1()).unwrap();
        assert!(d.starts_with("palw-tir/v1/"));
        assert!(d.ends_with("24:HistAppend"));
        assert_eq!(d.matches(':').count(), 25);
    }
}
