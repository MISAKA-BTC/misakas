//! The one error type. Every malformed program, every out-of-range value and every overflow of an
//! exact primitive is one of these — never a panic (PALW-TIR-9's totality, PALW-EX-5).
//!
//! **Normative content is `Ok` versus `Err`, not the class.** Two conforming implementations must
//! agree on whether a program decodes, whether it is in normal form and whether an evaluation
//! succeeds. The [`TirErrorKind`] is diagnostic: it names which rule fired so a disagreement can be
//! localised, and the golden vectors record it, but a second implementation that reports a
//! different class for the same failing input is not in conflict with this one.

use thiserror::Error;

/// Which rule a failure broke. Diagnostic (see the module note).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TirErrorKind {
    /// The bytes are not the Borsh encoding of a `TirProgramV1` (or are not its only encoding).
    Encoding,
    /// The program decodes but violates a structural normal-form rule (PALW-TIR-6/7).
    NormalForm,
    /// A type or shape rule failed (PALW-TIR-8).
    Shape,
    /// An exact primitive's result, or a partial sum of an exact reduction, left its declared
    /// type (PALW-TIR-9 at run time: how a verifier defect surfaces).
    Overflow,
    /// A gather index outside its axis, or a `TopK`/`Slice` bound that does not fit.
    Index,
    /// A `Div` divisor below one.
    Divisor,
    /// A value handed to the interpreter (param, committed operand, state, token) is not a value
    /// of its declared type, or has the wrong shape.
    Operand,
    /// A cone evaluation needed a value nobody supplied.
    Missing,
    /// The position is at or beyond `history_bound`, or a history has the wrong length.
    Position,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("{kind:?}: {msg}")]
pub struct TirError {
    pub kind: TirErrorKind,
    pub msg: String,
}

impl TirError {
    pub fn new(kind: TirErrorKind, msg: impl Into<String>) -> Self {
        Self { kind, msg: msg.into() }
    }
}

pub type TirResult<T> = Result<T, TirError>;

pub(crate) fn err<T>(kind: TirErrorKind, msg: impl Into<String>) -> TirResult<T> {
    Err(TirError::new(kind, msg))
}
