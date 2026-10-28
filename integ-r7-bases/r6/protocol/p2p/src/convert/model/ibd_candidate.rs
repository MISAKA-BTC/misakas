use kaspa_consensus_core::{BlockHash, header::Header};

/// A peer's answer to "which chain are you on?".
///
/// Every field is the peer's word. `virtual_selected_parent.blue_work` in particular is a claim:
/// a header commits to the value through PoW, but committing to a number is not deriving it
/// correctly from parents — that is contextual validation. Callers must keep this on the claim
/// side of the type system (see `ClaimedBlueWork` in the flows crate).
pub struct IbdCandidateSummary {
    pub virtual_selected_parent: Header,
    pub pruning_point: BlockHash,
    pub genesis_hash: Vec<u8>,
    pub consensus_params_id: Vec<u8>,
}
