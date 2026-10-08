//! **Where a fact the client shows came from** (RFC-0009 §6): proven against a block the client pinned, or merely reported.
//!
//! Several nodes agreeing is an operational defence — it makes a single lying node useless — and nothing more: nodes on one backend, a
//! colluding majority or a stale view agree too. So a fact read from nodes carries [`UNVERIFIED_REMOTE_STATE`] however many nodes said it,
//! and only a header-anchored proof (`crate::proof`) removes the label, and then only for the pinned block it was proven against. A fact
//! proven ABSENT as of a pinned block says nothing about the chain after it.

use kaspa_hashes::Hash64;

/// The label of every fact read from a node and not proven against a header the client holds.
pub const UNVERIFIED_REMOTE_STATE: &str = "UNVERIFIED_REMOTE_STATE";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Reported by `agreeing` node(s); nothing was proven.
    UnverifiedRemoteState { agreeing: usize },
    /// Proven PRESENT in the state the pinned block's header commits.
    ProvenAtPin { pinned_block: Hash64, header_daa: u64 },
    /// Proven ABSENT from the state the pinned block's header commits (silent about anything after it).
    ProvenAbsentAtPin { pinned_block: Hash64, header_daa: u64 },
}

impl Provenance {
    pub fn is_proven(&self) -> bool {
        !matches!(self, Provenance::UnverifiedRemoteState { .. })
    }

    /// One short line for a screen: the label, or what was proven and against which block.
    pub fn label(&self) -> String {
        match self {
            Provenance::UnverifiedRemoteState { agreeing } => {
                format!("{UNVERIFIED_REMOTE_STATE} ({agreeing} node(s) agree; agreement is not a proof)")
            }
            Provenance::ProvenAtPin { pinned_block, header_daa } => {
                format!("PROVEN against pinned block {} (header DAA {header_daa}); trust root = your pin", short(pinned_block))
            }
            Provenance::ProvenAbsentAtPin { pinned_block, header_daa } => format!(
                "PROVEN ABSENT as of pinned block {} (header DAA {header_daa}); says nothing about later blocks",
                short(pinned_block)
            ),
        }
    }
}

fn short(h: &Hash64) -> String {
    let s = h.to_string();
    format!("{}…", &s[..16])
}

/// A value and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Labelled<T> {
    pub value: T,
    pub provenance: Provenance,
}

impl<T> Labelled<T> {
    pub fn unverified(value: T, agreeing: usize) -> Self {
        Self { value, provenance: Provenance::UnverifiedRemoteState { agreeing } }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_header_anchored_proof_removes_the_label() {
        let many = Provenance::UnverifiedRemoteState { agreeing: 9 };
        assert!(!many.is_proven(), "nine nodes are still not a proof");
        assert!(many.label().starts_with(UNVERIFIED_REMOTE_STATE));
        let pin = Hash64::from_bytes([3; 64]);
        let proven = Provenance::ProvenAtPin { pinned_block: pin, header_daa: 5 };
        assert!(proven.is_proven() && !proven.label().contains(UNVERIFIED_REMOTE_STATE));
        let absent = Provenance::ProvenAbsentAtPin { pinned_block: pin, header_daa: 5 };
        assert!(absent.is_proven() && absent.label().contains("later blocks"));
    }
}
