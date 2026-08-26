//! **The canonical class registry: model id → arithmetic, graph, artifact shape, canonical job.**
//!
//! A PALW execution class is four things that must agree, and before this module they lived in
//! four places that could disagree:
//!
//! * the **arithmetic constants** (the norm epsilon above all) — the engine norms with the
//!   artifact's and the court re-norms with the class's, so a split convicts honest producers;
//! * the **graph** (`PalwShapeProfileV3`), whose id IS the class id;
//! * the **artifact shape**, which the weights are quantized into;
//! * the **canonical job**, from which `pwu_per_inference` is counted.
//!
//! The split that motivated this was real and shipped: `QWEN25_1_5B` declared `rms_eps_q: 1` while
//! `qwen25-convert` hardcoded `eps_q: 1 << 8` (copied from the floor). Two arithmetic
//! specifications under one model id — an artifact no registered Qwen class could legally run.
//!
//! So: one table, and everything else derives from it. The converter asks it what shape to build;
//! the producer asks it which class it is holding. Neither declares.
//!
//! **This is a build-local table, not consensus.** The chain says which class it wants
//! (`class_id`, `artifact_root`); this says what this binary can supply. [`resolve_class_v1`]
//! matches the two and refuses anything it cannot prove — derive, never declare (ADR-0046).

use crate::artifact::LN_THETA_10000_GEN_Q;
use crate::artifact::{Base0ArtifactV1, Base0ShapeV1};
use crate::inventory::{InventoryBuildError, base0_inventory_v1};
use kaspa_consensus_core::palw_base0_profile::{
    PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, PalwBase0GeometryV1, base0_profile_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_qwen25_profile::{
    QWEN25_1_5B, QWEN25_3B, qwen25_admissible_geometry_v1, qwen25_artifact_shape_v1, qwen25_profile_v1,
};
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_hashes::Hash64;

/// Where an artifact for a class comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactSourceV1 {
    /// Minted from a pinned seed by every node — the floor. No file, no deployment.
    Derived(u64),
    /// Quantized from a checkpoint offline and shipped as a file. The only way a real model can
    /// reach a node, because its weights are not a function of anything the node holds.
    Converted,
}

/// One class this build knows the canonical form of.
#[derive(Clone, Debug)]
pub struct CanonicalClassV1 {
    /// The identity a human uses. For a converted class it is the checkpoint's own id, so a
    /// conversion can be checked against the thing it claims to be.
    pub model_id: &'static str,
    /// The graph. `shape_profile_id()` is the class id the chain registers.
    pub profile: PalwShapeProfileV3,
    /// The shape an artifact for this class must have — arithmetic constants included.
    pub artifact_shape: Base0ShapeV1,
    /// `(prefill, decode)` the class is paid per.
    pub canonical_job: (u32, u32),
    pub source: ArtifactSourceV1,
    /// The geometry the inventory is built at; carries `tile_len`, which the artifact shape does
    /// not, and which decides where every operand row starts.
    pub inventory_geometry: PalwBase0GeometryV1,
}

impl CanonicalClassV1 {
    /// The class id: its graph's id (ADR-0049 Decision G).
    pub fn class_id(&self) -> Hash64 {
        self.profile.shape_profile_id()
    }

    /// The `artifact_root` a registration pins: the Merkle root over the canonical operand
    /// inventory. **Not** `artifact_digest()`, which is the artifact's own content hash and is not
    /// what openings prove against.
    pub fn artifact_root(&self, artifact: &Base0ArtifactV1) -> Result<Hash64, InventoryBuildError> {
        Ok(base0_inventory_v1(artifact, self.inventory_geometry)?.root())
    }

    /// Does this artifact have the shape this class is defined at? Checked field by field rather
    /// than by digest, so the error can say WHICH field disagrees — a digest comparison on a
    /// 1.7 GiB artifact says only "no".
    pub fn shape_matches(&self, artifact: &Base0ArtifactV1) -> Result<(), ClassResolveError> {
        let a = &artifact.shape;
        let w = &self.artifact_shape;
        let fields: [(&'static str, i128, i128); 9] = [
            ("n_layers", a.n_layers as i128, w.n_layers as i128),
            ("n_heads", a.n_heads as i128, w.n_heads as i128),
            ("n_kv_heads", a.n_kv_heads as i128, w.n_kv_heads as i128),
            ("d_head", a.d_head as i128, w.d_head as i128),
            ("d_ff", a.d_ff as i128, w.d_ff as i128),
            ("vocab", a.vocab as i128, w.vocab as i128),
            ("max_position", a.max_position as i128, w.max_position as i128),
            ("eps_q", a.eps_q as i128, w.eps_q as i128),
            ("ln_theta_gen_q", a.ln_theta_gen_q, w.ln_theta_gen_q),
        ];
        for (field, got, want) in fields {
            if got != want {
                return Err(ClassResolveError::ArtifactShape { model_id: self.model_id, field, got, want });
            }
        }
        Ok(())
    }
}

/// Every class this build can supply, at the geometry a given court admits.
///
/// The court is an argument because admissibility is a function of it: Qwen's own declared
/// geometry is far past `PALW_STEP_MAX_LEAVES`, so the class registers at a derived
/// `(tile_len, n_ctx)` and a different court would derive a different one — and therefore a
/// different class id.
pub fn canonical_classes_v1(court: &PalwCourtParamsV2) -> Vec<CanonicalClassV1> {
    let mut out = Vec::new();

    // The floor. Derived, so it needs no deployment and every node has it.
    if let Ok(profile) = base0_profile_v1(PALW_RC_BASE0_GEOMETRY) {
        let g = PALW_RC_BASE0_GEOMETRY;
        out.push(CanonicalClassV1 {
            model_id: "PALW-BASE-0/rc",
            profile,
            artifact_shape: Base0ShapeV1 {
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
            },
            canonical_job: PALW_RC_BASE0_CANONICAL,
            source: ArtifactSourceV1::Derived(crate::rc::PALW_RC_BASE0_SEED),
            inventory_geometry: g,
        });
    }

    for (model_id, declared) in [("Qwen/Qwen2.5-1.5B", QWEN25_1_5B), ("Qwen/Qwen2.5-3B", QWEN25_3B)] {
        let Some(g) = qwen25_admissible_geometry_v1(declared, court) else { continue };
        let Ok(profile) = qwen25_profile_v1(g) else { continue };
        let s = qwen25_artifact_shape_v1(g);
        out.push(CanonicalClassV1 {
            model_id,
            profile,
            artifact_shape: Base0ShapeV1 {
                n_layers: s.n_layers,
                n_heads: s.n_heads,
                n_kv_heads: s.n_kv_heads,
                d_head: s.d_head,
                d_ff: s.d_ff,
                vocab: s.vocab,
                max_position: s.max_position,
                ln_theta_gen_q: LN_THETA_10000_GEN_Q,
                eps_q: s.eps_q,
            },
            // The floor's shape of job, which is what `pwu_per_inference` is counted over. It is a
            // declaration either way — the gate recounts it — so it is stated here once rather
            // than chosen at each call site.
            canonical_job: PALW_RC_BASE0_CANONICAL,
            source: ArtifactSourceV1::Converted,
            inventory_geometry: PalwBase0GeometryV1 {
                layer_count: g.layer_count,
                hidden_dim: g.hidden_dim,
                ffn_dim: g.ffn_dim,
                attn_heads: g.attn_heads,
                attn_head_dim: g.attn_head_dim,
                vocab_size: g.vocab_size,
                n_ctx: g.n_ctx,
                n_threads: g.n_threads,
                rms_eps_q: g.rms_eps_q,
                tile_len: g.tile_len,
            },
        });
    }
    out
}

/// The canonical form of one model id, at this court.
pub fn canonical_class_by_model_id_v1(court: &PalwCourtParamsV2, model_id: &str) -> Option<CanonicalClassV1> {
    canonical_classes_v1(court).into_iter().find(|c| c.model_id == model_id)
}

#[derive(Debug)]
pub enum ClassResolveError {
    /// The chain named a class id no entry in this build produces. The node is older than the
    /// chain, or the class was registered by somebody running a different binary.
    UnknownClass {
        class_id: Hash64,
    },
    /// The class is known and this node holds no artifact for it.
    NoArtifact {
        model_id: &'static str,
    },
    /// An artifact was supplied and is not the shape this class is defined at.
    ArtifactShape {
        model_id: &'static str,
        field: &'static str,
        got: i128,
        want: i128,
    },
    /// The artifact has the right shape and hashes to a different root than the chain registered.
    /// These are the same weights only if this passes.
    ArtifactRoot {
        model_id: &'static str,
        got: Hash64,
        want: Hash64,
    },
    Inventory(InventoryBuildError),
    Artifact(crate::artifact::ArtifactError),
}

impl std::fmt::Display for ClassResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownClass { class_id } => write!(f, "the chain registered class {class_id}, which this build cannot supply"),
            Self::NoArtifact { model_id } => {
                write!(f, "class {model_id} needs a converted artifact and none was supplied (--palw-class-artifact)")
            }
            Self::ArtifactShape { model_id, field, got, want } => {
                write!(f, "the artifact offered for {model_id} has {field} {got}, and the class is defined at {want}")
            }
            Self::ArtifactRoot { model_id, got, want } => {
                write!(f, "the artifact for {model_id} roots to {got} and the chain registered {want} — different weights")
            }
            Self::Inventory(e) => write!(f, "the artifact has no canonical inventory: {e:?}"),
            Self::Artifact(e) => write!(f, "the artifact is not usable: {e:?}"),
        }
    }
}

impl std::error::Error for ClassResolveError {}

/// What a producer needs to run one class.
#[derive(Debug)]
pub struct ResolvedClassV1 {
    pub model_id: &'static str,
    pub profile: PalwShapeProfileV3,
    pub artifact: Base0ArtifactV1,
    pub canonical_job: (u32, u32),
    /// Recomputed here, never taken from the caller: this is the value the chain matched on.
    pub artifact_root: Hash64,
}

/// **Resolve the class the chain named.**
///
/// The chain says `class_id` (which graph) and `artifact_root` (which weights). This finds the
/// entry whose graph has that id, obtains its artifact — derived for the floor, supplied for a
/// converted class — and refuses unless the artifact's own inventory roots to what the chain
/// registered. Both halves are checked because either alone is insufficient: a matching id with
/// the wrong weights is a different execution, and matching weights under the wrong graph is a
/// different class.
///
/// `supplied` is what the operator loaded from files, in no particular order; entries that are not
/// this class are ignored rather than refused, because one node may hold artifacts for several.
pub fn resolve_class_v1(
    court: &PalwCourtParamsV2,
    class_id: Hash64,
    artifact_root: Hash64,
    supplied: &[Base0ArtifactV1],
) -> Result<ResolvedClassV1, ClassResolveError> {
    let entry = canonical_classes_v1(court)
        .into_iter()
        .find(|c| c.class_id() == class_id)
        .ok_or(ClassResolveError::UnknownClass { class_id })?;

    let candidates: Vec<Base0ArtifactV1> = match entry.source {
        ArtifactSourceV1::Derived(seed) => {
            vec![Base0ArtifactV1::derive_deterministic(entry.artifact_shape, seed).map_err(ClassResolveError::Artifact)?]
        }
        ArtifactSourceV1::Converted => supplied.to_vec(),
    };

    let mut shape_error: Option<ClassResolveError> = None;
    let mut root_error: Option<ClassResolveError> = None;
    for artifact in candidates {
        if let Err(e) = entry.shape_matches(&artifact) {
            shape_error.get_or_insert(e);
            continue;
        }
        let got = entry.artifact_root(&artifact).map_err(ClassResolveError::Inventory)?;
        if got != artifact_root {
            root_error.get_or_insert(ClassResolveError::ArtifactRoot { model_id: entry.model_id, got, want: artifact_root });
            continue;
        }
        return Ok(ResolvedClassV1 {
            model_id: entry.model_id,
            profile: entry.profile,
            artifact,
            canonical_job: entry.canonical_job,
            artifact_root: got,
        });
    }
    // The most specific failure wins: a wrong root is a nearer miss than a wrong shape, and both
    // are nearer than having brought nothing.
    Err(root_error.or(shape_error).unwrap_or(ClassResolveError::NoArtifact { model_id: entry.model_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court")
    }

    /// The full ladder with no cost ceiling — the court a Qwen entry needs to exist at all.
    ///
    /// Under the SHIPPED ceilings there is no admissible Qwen geometry (`max_close_bytes` counts
    /// what a close costs to carry, and one 128,256-lane logits row is four times a standard
    /// transaction), so the registry has no Qwen row and the tests that describe one have to say
    /// which court they are describing.
    fn ladder_only_court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::with_cost_ceilings(
            kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
            4,
            2,
            u64::MAX,
            u64::MAX,
            u32::MAX,
        )
        .expect("a court that refuses only on depth is legal")
    }

    /// **The registry is the only arithmetic there is.** Every entry's artifact shape carries the
    /// epsilon its own profile was built with — the split this module exists to close would show
    /// up here as a mismatch between the two.
    #[test]
    fn every_canonical_class_agrees_with_its_own_profile() {
        for c in canonical_classes_v1(&court()) {
            assert_eq!(
                c.artifact_shape.eps_q, c.profile.base0_rms_eps_q,
                "{}: the artifact would be quantized at one epsilon and adjudicated at another",
                c.model_id
            );
            assert_eq!(c.artifact_shape.n_layers, c.profile.layer_count as usize, "{}", c.model_id);
            assert_eq!(c.artifact_shape.d_ff, c.profile.ffn_dim as usize, "{}", c.model_id);
            assert_eq!(c.artifact_shape.vocab, c.profile.vocab_size as usize, "{}", c.model_id);
            assert_eq!(
                c.artifact_shape.n_heads * c.artifact_shape.d_head,
                c.profile.hidden_dim as usize,
                "{}: the artifact's width is not the graph's",
                c.model_id
            );
            assert_eq!(c.artifact_shape.max_position, c.inventory_geometry.n_ctx as usize, "{}", c.model_id);
        }
    }

    /// Qwen2.5-1.5B is registered at the ADMISSIBLE geometry, not the model's own — and its
    /// epsilon is the constant's `1`, which is what the converter must now build at.
    ///
    /// **The shipped court has no such entry at all**, and that is asserted first: the registry is
    /// a function of the court, and a class whose closes no transaction can carry is one the
    /// registry must not offer a converter a target for.
    #[test]
    fn the_qwen_entry_is_the_admissible_one() {
        assert!(
            canonical_class_by_model_id_v1(&court(), "Qwen/Qwen2.5-1.5B").is_none(),
            "the shipped court's cost ceiling admits no Qwen geometry, so the registry must not carry one"
        );
        let c = canonical_class_by_model_id_v1(&ladder_only_court(), "Qwen/Qwen2.5-1.5B").expect("1.5B is in the registry");
        assert_eq!(c.artifact_shape.eps_q, 1, "the canonical epsilon is the class constant's, not the floor's 1<<8");
        // The pair is the COURT's answer, not a literal — `qwen25_admissible_geometry_v1` searches
        // it, and pinning one side of a search is how a fixture stops describing the thing it
        // resolves. Under a ladder-only court the widest context wins, which is a different pair
        // from the one a cost-bounded court would pick.
        let derived = kaspa_consensus_core::palw_qwen25_profile::qwen25_admissible_geometry_v1(QWEN25_1_5B, &ladder_only_court())
            .expect("the ladder admits a pair");
        assert_eq!(c.inventory_geometry.tile_len, derived.tile_len);
        assert_eq!(c.artifact_shape.max_position, derived.n_ctx as usize);
        assert!(derived.n_ctx < QWEN25_1_5B.n_ctx, "the context still shrinks against the model's own 4096");
        assert_eq!(c.artifact_shape.n_kv_heads, 2, "grouped-query attention survives into the artifact shape");
        assert_eq!(c.source, ArtifactSourceV1::Converted);
    }

    /// **The floor resolves with no file at all** — it is derived, which is why the RC needs no
    /// artifact deployment — and it resolves through the SAME path a converted class does.
    #[test]
    fn the_floor_resolves_from_nothing_and_its_root_is_the_pinned_one() {
        let c = canonical_class_by_model_id_v1(&court(), "PALW-BASE-0/rc").expect("the floor is in the registry");
        let artifact = crate::rc::palw_rc_base0_artifact_v1().expect("derives");
        let root = c.artifact_root(&artifact).expect("has an inventory");
        assert_eq!(root, crate::rc::palw_rc_base0_artifact_root_v1().expect("pinned"), "the registry agrees with the RC pin");
        let resolved = resolve_class_v1(&court(), c.class_id(), root, &[]).expect("the floor needs no supplied artifact");
        assert_eq!(resolved.model_id, "PALW-BASE-0/rc");
        assert_eq!(resolved.artifact_root, root);
    }

    /// **The two halves are both load-bearing.** A class id the build does not know is refused; a
    /// known class with the wrong weights is refused by the root, not accepted because the id
    /// matched.
    #[test]
    fn resolution_refuses_a_wrong_id_and_wrong_weights_separately() {
        let c = canonical_class_by_model_id_v1(&court(), "PALW-BASE-0/rc").expect("floor");
        let root = crate::rc::palw_rc_base0_artifact_root_v1().expect("pinned");
        assert!(matches!(
            resolve_class_v1(&court(), Hash64::from_u64_word(0xDEAD), root, &[]),
            Err(ClassResolveError::UnknownClass { .. })
        ));
        match resolve_class_v1(&court(), c.class_id(), Hash64::from_u64_word(0xBAD), &[]) {
            Err(ClassResolveError::ArtifactRoot { got, want, .. }) => {
                assert_eq!(got, root);
                assert_eq!(want, Hash64::from_u64_word(0xBAD));
            }
            other => panic!("weights that are not the registered ones must be refused by the root, got {other:?}"),
        }
    }

    /// A converted class with nothing supplied says WHICH class is missing its artifact, because
    /// "resolution failed" on a node with several artifacts loaded is not actionable.
    #[test]
    fn a_converted_class_with_no_artifact_names_itself() {
        let c = canonical_class_by_model_id_v1(&ladder_only_court(), "Qwen/Qwen2.5-1.5B").expect("1.5B");
        match resolve_class_v1(&ladder_only_court(), c.class_id(), Hash64::from_u64_word(1), &[]) {
            Err(ClassResolveError::NoArtifact { model_id }) => assert_eq!(model_id, "Qwen/Qwen2.5-1.5B"),
            other => panic!("expected NoArtifact, got {other:?}"),
        }
    }
}
