//! **The PALW-RC network's BASE-0 artifact, and the root a genesis pins** (road-map Gate 4).
//!
//! # Why this can be a derivation, when a class artifact usually cannot
//!
//! `palw_base0_profile`'s own module doc draws the line: "BASE-0 has no file: it is a
//! specification, and its dimensions are what the network registering it chose." That is not true
//! of a converted class — Qwen's weights are a checkpoint somebody trained and everybody must
//! agree byte-for-byte on — but it is exactly true of the floor. ADR-0039 makes BASE-0 the
//! permanent liveness floor: it exists so the chain can always produce and always adjudicate, not
//! so it can answer questions. Nothing about that job needs trained weights.
//!
//! So the floor's artifact is PRODUCED rather than downloaded, and the production rule is pinned
//! here. The consequence is the property a genesis wants most: `artifact_root` is **re-derivable**.
//! Any participant, in any language, can rebuild these bytes from the seed and check the root the
//! chain committed to — no 4.5 MiB blob to host, to mirror, or to be handed a wrong copy of.
//!
//! [`Base0ArtifactV1::is_derived`] is still true of it, and that is a statement of fact rather than
//! a disqualification. What that flag exists to prevent is a fixture being logged as a TRAINED
//! model; the floor never claims to be one.
//!
//! # What is pinned, and what an operator still owns
//!
//! Pinned here: the geometry (`PALW_RC_BASE0_GEOMETRY`), the seed, and therefore every weight byte
//! and `artifact_root`. Not here, and not mintable by code: the genesis BOND — which premine output
//! backs it, and the ML-DSA-87 keys that sign under it. Those are operator facts, and
//! `palw-rc-genesis` is the tool that turns them into the constants a binary ships.

use kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY;
use kaspa_hashes::Hash64;

use crate::artifact::{ArtifactError, Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
use crate::inventory::{InventoryBuildError, base0_inventory_v1};

/// The seed the RC floor's weights are derived from.
///
/// A number with no meaning, which is the point: it is a nothing-up-my-sleeve constant for a class
/// whose weights carry no information. Changing it changes every byte and therefore
/// `artifact_root`, which is why it is pinned rather than passed.
pub const PALW_RC_BASE0_SEED: u64 = 0x5041_4C57_5F52_4330; // "PALW_RC0"

/// The RC floor's shape, from the geometry the network registers it with.
///
/// Every field is the geometry's, so the two cannot describe different classes — which is the
/// mistake `Base0ArtifactV1::check_geometry` exists to catch and this avoids having to catch.
pub fn palw_rc_base0_shape_v1() -> Base0ShapeV1 {
    let g = PALW_RC_BASE0_GEOMETRY;
    Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        // Multi-head: the floor has no grouped-query attention to express.
        n_kv_heads: g.attn_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: LN_THETA_10000_GEN_Q,
        eps_q: g.rms_eps_q,
    }
}

/// The RC floor's artifact — the same bytes on every machine, from the seed alone.
pub fn palw_rc_base0_artifact_v1() -> Result<Base0ArtifactV1, ArtifactError> {
    Base0ArtifactV1::derive_deterministic(palw_rc_base0_shape_v1(), PALW_RC_BASE0_SEED)
}

/// **`artifact_root` — the one genesis input code cannot mint, minted.**
///
/// The Merkle root over the floor's canonical inventory (ADR-0049 Decision G): one leaf per operand
/// row, at the coordinates `palw_step_refute` opens against. This is the value a genesis pins and
/// every node checks its own derivation against.
pub fn palw_rc_base0_artifact_root_v1() -> Result<Hash64, InventoryBuildError> {
    let artifact = palw_rc_base0_artifact_v1().map_err(InventoryBuildError::Geometry)?;
    Ok(base0_inventory_v1(&artifact, PALW_RC_BASE0_GEOMETRY)?.root())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The floor's artifact is a function of nothing but the pin.**
    ///
    /// Two derivations agree, the geometry it claims is the geometry it has, and the inventory
    /// covers the graph — which together are what let a genesis pin a root instead of hosting a
    /// blob. A root that could not be re-derived would be a 4.5 MiB file every participant has to
    /// be handed correctly.
    #[test]
    fn the_rc_floor_derives_one_artifact_and_one_root() {
        let a = palw_rc_base0_artifact_v1().expect("the floor's shape is legal");
        let b = palw_rc_base0_artifact_v1().unwrap();
        assert_eq!(a.artifact_digest(), b.artifact_digest(), "one seed, one artifact");

        // The class it belongs to is the graph the chain registers, and the artifact agrees with
        // that geometry rather than merely being handed it.
        let class_id = a.execution_class_id(PALW_RC_BASE0_GEOMETRY).expect("the artifact IS this geometry's");
        let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        assert_eq!(class_id, profile.shape_profile_id());

        let root = palw_rc_base0_artifact_root_v1().expect("the inventory builds");
        assert_ne!(root, Hash64::default(), "a zero root would verify nothing forever");
        assert_eq!(root, palw_rc_base0_artifact_root_v1().unwrap(), "and it is re-derivable, which is the whole point");

        // The inventory the root is over really covers the graph — a root over an artifact missing
        // a tensor the graph reads is a class whose steps adjudicate `Unadjudicable` at exactly
        // the nodes that read it.
        let inventory = base0_inventory_v1(&a, PALW_RC_BASE0_GEOMETRY).unwrap();
        inventory.verify_covers_profile(&profile).expect("every tensor the floor's graph names is carried");
    }

    /// **The pin and the derivation are the same value.**
    ///
    /// `consensus-core` ships `artifact_root` as a constant because a verifier must not need the
    /// weights; this crate can derive it because a producer has them. Nothing else in the tree can
    /// hold both sides, so this is where the two meet — and it fails if either moves, which is the
    /// only thing that keeps a pinned hash honest.
    #[test]
    fn the_pinned_rc_artifact_root_is_the_one_the_floor_derives() {
        assert_eq!(
            palw_rc_base0_artifact_root_v1().expect("the floor's artifact derives"),
            kaspa_consensus_core::config::params::PALW_RC_GENESIS_ARTIFACT_ROOT,
            "the pinned RC artifact root is not the one this build derives — re-run `palw-rc-genesis` \
             and update the constant, or find out why the derivation moved"
        );
    }
}
