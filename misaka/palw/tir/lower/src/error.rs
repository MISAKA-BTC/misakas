//! Errors. The one that matters is [`LowerError::NotLowerable`]: it is the refusal the RFC's
//! `check-architecture` tool prints as `NOT_LOWERABLE(reason)` (RFC-0002 §8).

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum LowerError {
    /// The configuration is well formed but describes something this lowerer does not model, or
    /// models only with a value it has not implemented. **Never a silent ignore**: every key that
    /// can change the math is either understood or lands here.
    #[error("NOT_LOWERABLE({0})")]
    NotLowerable(String),
    /// The configuration is malformed (wrong JSON type, missing required key, inconsistent dims).
    #[error("bad config: {0}")]
    BadConfig(String),
    /// A checkpoint tensor is missing, has the wrong shape or dtype, or the container is corrupt.
    #[error("weights: {0}")]
    Weights(String),
    #[error("io: {0}")]
    Io(String),
    /// A float-reference evaluation failed (shape mismatch between graph and weights, …).
    #[error("eval: {0}")]
    Eval(String),
}

impl LowerError {
    pub fn not_lowerable(s: impl Into<String>) -> Self {
        LowerError::NotLowerable(s.into())
    }
    pub fn bad(s: impl Into<String>) -> Self {
        LowerError::BadConfig(s.into())
    }
    pub fn weights(s: impl Into<String>) -> Self {
        LowerError::Weights(s.into())
    }
    pub fn eval(s: impl Into<String>) -> Self {
        LowerError::Eval(s.into())
    }
}

pub type Result<T> = std::result::Result<T, LowerError>;
