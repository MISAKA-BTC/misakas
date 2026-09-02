//! ADR-0077 Decision 8 / Decision 13, producer side: the free-prompt capture retained SPARSELY —
//! the step leg's leaf hashes folded as they are produced, the tree kept only above a fixed
//! level, every tile re-derived by replay from the checkpoint chunks when an opening is asked for.
//! (Owned by the capture workstream; empty until it lands.)
