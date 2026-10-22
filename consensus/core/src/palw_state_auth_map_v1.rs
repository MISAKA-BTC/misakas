//! **RFC-0036 / ADR-0203 (Proposed) — the authenticated PALW state map: a pure library, wired to nothing.**
//!
//! Today a PALW collection enters the state root as ONE sequential hash of all its rows (`palw_collection_root_of_entries_v1`,
//! opened by `crate::palw_state_proof_v1`), so the only proof of a row — or of its absence — is the whole collection. A pruned node
//! that fetches rows on demand needs per-row proofs instead. This module is that commitment, built only from hashes (keyed BLAKE2b,
//! a Merkle trie): it authenticates DATA a node fetched; it proves nothing about any computation (PRINCIPLES §8).
//!
//! * [`AuthMapV1`] — one table: a binary Patricia Merkle trie over a prefix-free, order-preserving encoding of raw keys
//!   ([`bits`]). Canonical (the root is a function of the rows), `O(depth)` update.
//! * [`AuthMapProofV1`] — one proof shape for every question: a pruned trie opened exactly where a range may reach. Verified by
//!   [`verify_rows_v1`] it answers **membership**, **non-membership** and **range completeness**; by [`verify_update_v1`] it
//!   turns proven **old values into the new root**; chunked, [`AuthStreamVerifierV1`] reads a **whole table** against one root.
//! * [`AuthStateV1`] / [`verify_state_rows_v1`] / [`verify_state_change_v1`] — the state level: a directory of table roots under a
//!   header that commits the **format version**, the **parent state root** and the **branch anchor**; every node hash commits its
//!   **table id, schema version and limits**. A row of another table, another version, another parent or another branch does not
//!   verify.
//! * **Bounded.** A table declares its key/value limits (hashed into it); a point proof is at most
//!   [`AuthTableSpecV1::point_proof_node_bound`] nodes, and every verifier takes a node ceiling it checks before hashing.
//!
//! Nothing in consensus calls this module. A state root of this form would be a new root version and lives behind the dormant
//! fence `Params::palw_state_row_map_v1` (`None` on every preset, refused if armed: no wiring exists).

pub mod bits;
pub mod proof;
pub mod state;
pub mod trie;

#[cfg(test)]
mod tests;

pub use bits::{BitPathV1, KeyBoundV1, KeyRangeV1, subtree_may_intersect_v1};
pub use proof::{
    AuthMapProofV1, AuthStreamVerifierV1, ProofNodeV1, RowUpdateV1, StreamChunkV1, VerifiedRowsV1, verify_rows_v1, verify_update_v1,
};
pub use state::{
    AUTH_STATE_DIRECTORY_SPEC_V1, AUTH_STATE_FORMAT_V1, AuthProofLimitsV1, AuthStateChangeSetV1, AuthStateHeaderV1, AuthStatePinV1,
    AuthStateRowProofV1, AuthStateV1, AuthTableChangeV1, directory_key_v1, verify_state_change_v1, verify_state_rows_v1,
};
pub use trie::{AUTH_MAP_MAX_KEY_LEN_V1, AUTH_MAP_MAX_VALUE_LEN_V1, AuthMapV1, AuthTableSpecV1, auth_table_root_v1};

/// Every way a proof, an update or a write is refused. None of them means "absent": absence is `Ok(None)` from a verified range.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum AuthMapErrorV1 {
    #[error("the proof does not hash to the committed root")]
    RootMismatch,
    #[error("the proof leaves closed a subtree a proven range can reach")]
    IncompleteRange,
    #[error("an opened node does not lie under the prefix its parent fixes")]
    Misplaced,
    #[error("the proof is not well formed")]
    Malformed,
    #[error("the proof exceeds its size bound")]
    ProofTooLarge,
    #[error("a key exceeds the table's key limit")]
    KeyTooLong,
    #[error("a value exceeds the table's value limit")]
    ValueTooLong,
    #[error("the table spec is outside the module's hard limits")]
    SpecOutOfBounds,
    #[error("the key is outside every proven range: not proven, which is not absent")]
    NotCovered,
    #[error("an update's old value is not the proven value")]
    StaleOldValue,
    #[error("one key is written twice in one change set")]
    DuplicateWrite,
    #[error("the state commits another parent root or another branch")]
    OtherBranch,
    #[error("the state header has another format version")]
    WrongFormat,
    #[error("the state's directory holds no such table (id and version)")]
    TableAbsent,
    #[error("a stream chunk proves no row and is not the last")]
    NoProgress,
    #[error("the stream is already complete")]
    StreamFinished,
}
