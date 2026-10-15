/// Result of comparing a proposed sink with this node's PALW authority.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwReorgOutcome {
    Allow,
    DominanceViolation,
    FrontierProvenanceViolation,
}

impl PalwReorgOutcome {
    pub(crate) fn is_accept(self) -> bool {
        self == Self::Allow
    }
}
