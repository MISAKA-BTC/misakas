//! **Three hashes describe a model class, and two of them were the same type.**
//!
//! A class registration pins an `artifact_root`. There are two values a reader can plausibly hand to
//! that name and only one is right:
//!
//! * [`PalwArtifactDigestV1`] — the artifact FILE's own content hash
//!   (`Base0ArtifactV1::artifact_digest`). It answers "are these the same bytes". **Nothing can be
//!   opened against it**, so it is not a consensus identity for a court-capable class.
//! * [`PalwInventoryRootV1`] — the Merkle root over the canonical operand inventory, at a named
//!   profile. It is what an arithmetic close's openings prove against, so it is what a registration
//!   pins and what a producer's class resolve compares.
//! * [`PalwClassIdV1`] — the graph's id (`shape_profile_id`). Not a property of the weights at all.
//!
//! **This module exists because the distinction was carried by comments and got lost twice.** Both
//! roots were `Hash64`, they sat as neighbouring `pub const`s in the genesis card, the genesis side
//! was a human pasting bytes while the runtime side derived its own, and the derivation depends on
//! the PROFILE — so the same file has different inventory roots at different `n_ctx`. testnet-11
//! shipped the digest where the inventory root belonged and its dense tier produced zero blocks;
//! `PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT`'s comment records that. testnet-12 then did the same
//! thing, in the same field, and its four seats logged `holds no artifact whose registered root form
//! is b5baca63…` from genesis over a byte-identical artifact.
//!
//! A comment that has been read and not followed twice is not a weaker comment; it is the wrong
//! mechanism. These are distinct types with no conversion between them, so the substitution that
//! caused both outages stops being expressible.
//!
//! **What is deliberately absent:** `From<Hash64>`, `Deref`, and any `as_*`/`into_*` pair that would
//! let one stand in for another. Crossing out of a type is `into_hash64`, named so a reader can find
//! every boundary where the guarantee stops, and constructing one is a named function that says which
//! quantity was measured.

use crate::Hash64;

/// **The artifact file's content hash — integrity only, never a consensus identity.**
///
/// Use it to answer "is this the file I measured". It cannot answer "does this artifact serve the
/// class the chain registered", because nothing in a close can be opened against it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PalwArtifactDigestV1(Hash64);

/// **The Merkle root over a canonical operand inventory, at one profile** — the value a
/// `ClassRegistered` pins, and the value an opening proves against.
///
/// It is a function of BOTH the artifact and the profile. The same weights under `graph-v7@512` and
/// `graph-v7@2097152` have different inventory roots, which is why a value copied from another
/// network's registration is not a value this one can use.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PalwInventoryRootV1(Hash64);

/// **A class's identity: its graph's id.** A property of the declaration, not of any weights.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PalwClassIdV1(Hash64);

impl PalwArtifactDigestV1 {
    /// The digest an artifact container computed over its own bytes.
    pub const fn measured_over_the_file(h: Hash64) -> Self {
        Self(h)
    }
    pub const fn into_hash64(self) -> Hash64 {
        self.0
    }
}

impl PalwInventoryRootV1 {
    /// The root a canonical-inventory walk produced at a named profile — materialized or streamed,
    /// which `a16_streamed_root.rs` pins equal.
    pub const fn rooted_over_the_inventory(h: Hash64) -> Self {
        Self(h)
    }
    /// The value a chain object carries in its `artifact_root` field. Named for the boundary rather
    /// than for the measurement, because nothing here can check that the chain's value was measured
    /// the right way — that is what the manifest and the startup recomputation are for.
    pub const fn as_registered_on_chain(h: Hash64) -> Self {
        Self(h)
    }
    pub const fn into_hash64(self) -> Hash64 {
        self.0
    }
}

impl PalwClassIdV1 {
    pub const fn of_this_graph(h: Hash64) -> Self {
        Self(h)
    }
    pub const fn into_hash64(self) -> Hash64 {
        self.0
    }
}

impl std::fmt::Display for PalwArtifactDigestV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::fmt::Display for PalwInventoryRootV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::fmt::Display for PalwClassIdV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The substitution that caused both outages does not compile.**
    ///
    /// This is the whole point of the module, so it is asserted the only way a type-level guarantee
    /// can be: by naming the expressions that must be rejected, and pinning that the source carries
    /// no escape hatch that would re-admit them. A `From<Hash64>` on any of the three, or a shared
    /// `Deref`, would make `registration.artifact_root = artifact.artifact_digest()` legal again.
    /// The module's own source, above `#[cfg(test)]` — the needles below appear in this test's own
    /// text, so scanning the whole file would find them and report the guarantee broken by the test
    /// that checks it.
    fn source_above_the_tests() -> &'static str {
        let src = include_str!("palw_class_identity_v1.rs");
        let cut = src.find("#[cfg(test)]").expect("this module has tests");
        &src[..cut]
    }

    #[test]
    fn nothing_converts_a_digest_into_a_root() {
        let src = source_above_the_tests();
        for forbidden in ["impl From<Hash64>", "impl std::ops::Deref", "impl Deref"] {
            assert!(!src.contains(forbidden), "{forbidden} would let a digest stand in for a root again");
        }
        // And the three wrap the same primitive, so only the types keep them apart.
        let h = Hash64::from_u64_word(0xA57);
        let digest = PalwArtifactDigestV1::measured_over_the_file(h);
        let root = PalwInventoryRootV1::rooted_over_the_inventory(h);
        let class = PalwClassIdV1::of_this_graph(h);
        assert_eq!(digest.into_hash64(), root.into_hash64(), "same bytes");
        assert_eq!(root.into_hash64(), class.into_hash64(), "same bytes");
        // `assert_eq!(digest, root)` does not compile: different types, no cross-type PartialEq.
    }

    /// A root is a function of the profile as well as the artifact, so the type carries no
    /// constructor that takes only weights. Stated as a test so the constructor names stay honest.
    #[test]
    fn a_root_is_never_constructed_from_an_artifact_alone() {
        let src = source_above_the_tests();
        assert!(
            src.contains("rooted_over_the_inventory") && src.contains("as_registered_on_chain"),
            "the two ways an inventory root legitimately arrives are a walk and the chain"
        );
        assert!(!src.contains("fn of_artifact"), "there is no artifact-only route to an inventory root");
    }
}
