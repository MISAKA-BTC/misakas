//! Errors (04b §9.3). The class is diagnostic; only success versus error is normative
//! (PALW-TIR-34). Every failure of this crate is one of these values — never a panic.

use std::fmt;

/// The nine diagnostic classes of 04b §9.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    Encoding,
    NormalForm,
    Shape,
    Overflow,
    Index,
    Divisor,
    Operand,
    Missing,
    Position,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Encoding => "Encoding",
            Class::NormalForm => "NormalForm",
            Class::Shape => "Shape",
            Class::Overflow => "Overflow",
            Class::Index => "Index",
            Class::Divisor => "Divisor",
            Class::Operand => "Operand",
            Class::Missing => "Missing",
            Class::Position => "Position",
        }
    }

    pub fn from_name(s: &str) -> Option<Class> {
        Some(match s {
            "Encoding" => Class::Encoding,
            "NormalForm" => Class::NormalForm,
            "Shape" => Class::Shape,
            "Overflow" => Class::Overflow,
            "Index" => Class::Index,
            "Divisor" => Class::Divisor,
            "Operand" => Class::Operand,
            "Missing" => Class::Missing,
            "Position" => Class::Position,
            _ => return None,
        })
    }
}

/// A failure: its class and a human-readable reason (the reason is never compared).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirError {
    pub class: Class,
    pub reason: String,
}

impl TirError {
    pub fn new(class: Class, reason: impl Into<String>) -> Self {
        TirError { class, reason: reason.into() }
    }
}

impl fmt::Display for TirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.class.name(), self.reason)
    }
}

impl std::error::Error for TirError {}

pub type Res<T> = Result<T, TirError>;

pub(crate) fn err<T>(class: Class, reason: impl Into<String>) -> Res<T> {
    Err(TirError::new(class, reason))
}
