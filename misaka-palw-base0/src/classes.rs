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
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_A16_CANONICAL, qwen25_a16_profile_v1};
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
    /// Converted like [`Self::Converted`], but the root a registration pins is the artifact's own
    /// **digest**, not an operand-inventory root — the A16 tier's courts open logits tiles from
    /// the trace, not weight rows from an inventory, so the digest is what its openings prove
    /// against. Resolved by [`crate::qwen25_a16_backend::Qwen25A16Backend`], never by the floor's
    /// integer engine.
    ConvertedA16,
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
        match self.source {
            // The A16 tier registers the artifact's own digest — see [`ArtifactSourceV1::ConvertedA16`].
            ArtifactSourceV1::ConvertedA16 => Ok(artifact.artifact_digest()),
            _ => Ok(base0_inventory_v1(artifact, self.inventory_geometry)?.root()),
        }
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

/// Every class this build can supply.
///
/// The court argument is currently unread: every entry is a FROZEN geometry — the floor's
/// constant, and the A16 family's fixed context ladder — because a class id is the profile's id
/// and a geometry that moved with the court would silently rename a class the chain already
/// registered. Whether an entry is admissible under a given court is the admission gate's
/// question (`verify_class_admission_v2`), answered at registration; the
/// `a16_context_ladder_against_the_shipped_bundle` test pins that today's ladder fits today's
/// court. The argument stays because "what this build can supply" is conceptually per-court, and
/// every caller already holds one.
pub fn canonical_classes_v1(court: &PalwCourtParamsV2) -> Vec<CanonicalClassV1> {
    let _ = court;
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

    // **The A16 dense family: one arithmetic lineage, one entry per MODEL.**
    //
    // A class IS its graph (`shape_profile_id`), and the chain refuses a second registration of
    // the same id — so two weight-sets of the same geometry need two profiles, and `n_ctx` is the
    // axis that changes nothing else about the arithmetic. The base model sits at the geometry
    // testnet-11's genesis registered (n_ctx 16, class f942e268…, asserted by test below); each
    // later model in the lineage takes the next context bound. The REAL ceiling is the court's:
    // n_ctx 20 is the last value the RC close budget admits (measured 2026-08-28), so this table
    // has room for four models before the family needs a second axis.
    //
    // `artifact_shape` is what `qwen25-convert --a16` actually writes — `max_position` is the
    // converter's rotary-table default and `eps_q` the engine's, both independent of the profile's
    // `n_ctx`/`rms_eps_q` — so sibling models are NOT separable by shape. Registration therefore
    // takes the model id from the operator (`--palw-register-class <model-id>`) when more than one
    // entry matches, and the chain's duplicate-class refusal backstops a wrong pick of an already
    // registered sibling.
    // n_ctx 17 is BURNED: the 2026-08-28 mispairing registered it on chain with the genesis
    // 1.5B digest (class 7886359c…, root c00faa48…) before the known-weights filter existed.
    // That class stays on chain — nothing produces for it, panels answer Incapable, and the
    // reclaim path is welcome to it — but this ledger must never derive it again, so the Coder
    // takes the next rung.
    // **The corrected A16 graph rides here too, as a class of its own.**
    //
    // The v1 rows carry `qwen25_a16_profile_v1`, which two measurements show does not describe the
    // engine it runs (`docs/palw-fp-on-registered-classes.md`): its pre table omits the embed-lift
    // requant the engine performs, and its state chunk map is one byte per element over an `i32`
    // cache. Either defect alone makes a step leg uncommittable, so no v1 class can serve the
    // free-prompt lane.
    //
    // `qwen25_a16_profile_v2` fixes both, and because a class IS its graph that is a DIFFERENT id —
    // which is why this is a row rather than an edit. The v1 rows stay exactly as they are:
    // testnet-11 registered one of them, and a build that changed it would be a different network
    // wearing the same name. The new row is what `--palw-register-class` can put on a running chain
    // when somebody decides to.
    //
    // It answers to its own name rather than sharing "Qwen/Qwen2.5-1.5B": the flag disambiguates by
    // model id when an artifact's shape matches more than one class, and two rows answering to one
    // name would leave it unable to say which.
    for (model_id, n_ctx, graph_v2) in [
        ("Qwen/Qwen2.5-1.5B", 16u32, false),
        ("Qwen/Qwen2.5-Coder-1.5B-Instruct", 18, false),
        ("Qwen/Qwen2.5-1.5B/graph-v2", 16, true),
    ] {
        let g = PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B };
        let profile =
            if graph_v2 { kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(g) } else { qwen25_a16_profile_v1(g) };
        let Ok(profile) = profile else { continue };
        out.push(CanonicalClassV1 {
            model_id,
            profile,
            artifact_shape: Base0ShapeV1 {
                n_layers: g.layer_count as usize,
                n_heads: g.attn_heads as usize,
                n_kv_heads: g.attn_kv_heads as usize,
                d_head: g.attn_head_dim as usize,
                d_ff: g.ffn_dim as usize,
                vocab: g.vocab_size as usize,
                // The converter's rotary-table default, NOT the profile's n_ctx: the artifact
                // states what the weights can do, the profile states what the court admits.
                max_position: 512,
                // Qwen2.5's rope base is 1e6; the A16 engine norms at the shipped 1 << 8.
                ln_theta_gen_q: crate::artifact::LN_THETA_1000000_GEN_Q,
                eps_q: 1 << 8,
            },
            canonical_job: QWEN25_A16_CANONICAL,
            source: ArtifactSourceV1::ConvertedA16,
            // Unused by the A16 resolve path (the root is a digest), filled from the geometry so
            // nothing downstream reads a floor constant by accident.
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

/// **One class of the qwen36 lineage this build knows the canonical form of.**
///
/// A separate table from [`CanonicalClassV1`] because the artifact is a different животное: a
/// memory-mapped `.palwq36` whose root is COMPUTED over the mapping (`Qwen36ArtifactV1`), not a
/// dense `.palwart` matched by digest — so the shape check, the root derivation and the backend
/// dispatch are all different code. What the two tables share is the CONTRACT: model id →
/// frozen geometry → profile whose id is the class id, and registration/resolution both read
/// this table rather than a hard-coded id.
#[derive(Clone, Debug)]
pub struct Qwen36CanonicalClassV1 {
    pub model_id: &'static str,
    pub geometry: kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1,
    pub canonical_job: (u32, u32),
}

impl Qwen36CanonicalClassV1 {
    pub fn profile(&self) -> Result<PalwShapeProfileV3, kaspa_consensus_core::palw_step::PalwStepError> {
        kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v1(self.geometry)
    }

    /// The class id: its graph's id, same rule as the dense table's.
    pub fn class_id(&self) -> Option<Hash64> {
        self.profile().ok().map(|p| p.shape_profile_id())
    }

    /// Does this artifact have the shape this class is defined at? Dimension by dimension, so
    /// the error names the field. `max_position` is deliberately NOT compared — the artifact
    /// states what its rotary table covers, the profile states what the court admits — and the
    /// epsilons are not either, for the same reason the dense table skips them on its A16 arm:
    /// the engine's integer epsilon is the artifact format's, the class constant is the court's.
    pub fn shape_matches(&self, shape: &crate::qwen36::Qwen36ShapeV1) -> Result<(), String> {
        let g = &self.geometry;
        let want_kinds: Vec<crate::qwen36::Qwen36LayerKind> = (0..g.layer_count as usize)
            .map(|i| {
                if g.full_attention_interval != 0 && (i + 1).is_multiple_of(g.full_attention_interval as usize) {
                    crate::qwen36::Qwen36LayerKind::FullAttention
                } else {
                    crate::qwen36::Qwen36LayerKind::LinearAttention
                }
            })
            .collect();
        if shape.layer_types != want_kinds {
            return Err(format!("{}: the layer stack is not this class's ({} layers)", self.model_id, shape.layer_types.len()));
        }
        let fields: [(&str, usize, usize); 12] = [
            ("d_model", shape.d_model, g.hidden_dim as usize),
            ("n_heads", shape.n_heads, g.attn_heads as usize),
            ("n_kv_heads", shape.n_kv_heads, g.attn_kv_heads as usize),
            ("head_dim", shape.head_dim, g.attn_head_dim as usize),
            ("rotary_dim", shape.rotary_dim, g.rope_dims as usize),
            ("linear_k_heads", shape.linear_k_heads, g.gdn_k_heads as usize),
            ("linear_v_heads", shape.linear_v_heads, g.gdn_v_heads as usize),
            ("conv_kernel", shape.conv_kernel, g.gdn_conv_kernel as usize),
            ("n_experts", shape.n_experts, g.n_experts as usize),
            ("experts_per_token", shape.experts_per_token, g.experts_per_token as usize),
            ("moe_dim", shape.moe_dim, g.moe_dim as usize),
            ("vocab", shape.vocab, g.vocab_size as usize),
        ];
        for (field, got, want) in fields {
            if got != want {
                return Err(format!("{}: the artifact has {field} {got}, and the class is defined at {want}", self.model_id));
            }
        }
        // shared_dim separates the hybrid from the qwen3moe members even at equal dims.
        if shape.shared_dim != g.shared_dim as usize {
            return Err(format!(
                "{}: the artifact has shared_dim {}, and the class is defined at {}",
                self.model_id, shape.shared_dim, g.shared_dim
            ));
        }
        Ok(())
    }
}

/// Every qwen36-lineage class this build can supply, at FROZEN geometries — same discipline as
/// [`canonical_classes_v1`]: a geometry that moved would rename a class the chain already runs.
/// The hybrid's entry derives the class testnet-11 registered; the qwen3moe entry is the first
/// permissionless member (ladder facts on the constant itself).
pub fn qwen36_canonical_classes_v1() -> Vec<Qwen36CanonicalClassV1> {
    vec![
        Qwen36CanonicalClassV1 {
            model_id: "Qwen3.6-35B-A3B",
            geometry: kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B,
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
        },
        Qwen36CanonicalClassV1 {
            model_id: "huihui-ai/Huihui-Qwen3-Coder-30B-A3B-Instruct-abliterated",
            geometry: kaspa_consensus_core::palw_qwen36_profile::QWEN3_CODER_30B_A3B,
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
        },
        Qwen36CanonicalClassV1 {
            model_id: "Qwen/Qwen3.5-2B",
            geometry: kaspa_consensus_core::palw_qwen36_profile::QWEN35_2B,
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
        },
    ]
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
    /// **The geometry `artifact_root` was computed under**, carried rather than re-named.
    ///
    /// Anything that later opens a row of this artifact must root it the same way the chain
    /// matched it, and the family — not a constant at the use site — is what decided that. A
    /// caller that reached for `PALW_RC_BASE0_GEOMETRY` instead would agree with this entry today
    /// and diverge silently the first time a class registers under a different one, producing
    /// openings that prove against a root no court holds.
    pub inventory_geometry: PalwBase0GeometryV1,
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
        // The A16 tier resolves through its own backend (digest root, logits-tile courts) —
        // matching it here would build a 1.7 GiB operand inventory per block and then refuse on
        // the root anyway. Its dispatch lives in the backend registry.
        .filter(|c| !matches!(c.source, ArtifactSourceV1::ConvertedA16))
        .find(|c| c.class_id() == class_id)
        .ok_or(ClassResolveError::UnknownClass { class_id })?;

    let candidates: Vec<Base0ArtifactV1> = match entry.source {
        ArtifactSourceV1::Derived(seed) => {
            vec![Base0ArtifactV1::derive_deterministic(entry.artifact_shape, seed).map_err(ClassResolveError::Artifact)?]
        }
        ArtifactSourceV1::Converted => supplied.to_vec(),
        // Filtered out of `entry` above — an A16 class resolves through its own backend.
        ArtifactSourceV1::ConvertedA16 => unreachable!("A16 entries never reach the floor engine's resolve"),
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
            inventory_geometry: entry.inventory_geometry,
        });
    }
    // The most specific failure wins: a wrong root is a nearer miss than a wrong shape, and both
    // are nearer than having brought nothing.
    Err(root_error.or(shape_error).unwrap_or(ClassResolveError::NoArtifact { model_id: entry.model_id }))
}

#[cfg(test)]
mod tests {

    /// **The corrected A16 graph is in the catalog, registrable, and is not the class testnet-11
    /// carries.**
    ///
    /// Both halves matter. Registrable, because `--palw-register-class` derives what it files from
    /// this catalog and a class it does not know cannot be put on a running chain. Not the same id,
    /// because the two profiles differ in what the court recomputes — and if these ever collided, a
    /// build could change a live network's class without changing what it calls itself.
    #[test]
    fn the_corrected_a16_graph_is_a_registrable_class_of_its_own() {
        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let classes = canonical_classes_v1(&court);
        let v1 = classes.iter().find(|c| c.model_id == "Qwen/Qwen2.5-1.5B").expect("the registered A16 class");
        let v2 = classes.iter().find(|c| c.model_id == "Qwen/Qwen2.5-1.5B/graph-v2").expect("the corrected A16 class");

        assert_ne!(v1.class_id(), v2.class_id(), "a corrected graph is a different class");
        assert_eq!(v1.profile.pre_nodes.len() + 1, v2.profile.pre_nodes.len(), "and the difference is the narrowing it names");
        assert_eq!(
            v2.profile.state_chunk_map_id,
            kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_chunk_map_id_v2(),
            "with the state map its cache actually has"
        );
        // One name, one row: the register flag disambiguates by model id.
        assert_eq!(classes.iter().filter(|c| c.model_id == v2.model_id).count(), 1);
    }
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
    ///
    /// **No caller today.** The tests that described a Qwen court were removed with the row, and
    /// this is kept rather than deleted because the constraint it encodes is still true and still
    /// the reason the registry has no Qwen entry — a fixture is the cheapest place for that to
    /// stay checkable. Allowed rather than silently dead so `-D warnings` stays usable.
    #[allow(dead_code)]
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
            if !matches!(c.source, ArtifactSourceV1::ConvertedA16) {
                assert_eq!(
                    c.artifact_shape.eps_q, c.profile.base0_rms_eps_q,
                    "{}: the artifact would be quantized at one epsilon and adjudicated at another",
                    c.model_id
                );
            }
            assert_eq!(c.artifact_shape.n_layers, c.profile.layer_count as usize, "{}", c.model_id);
            assert_eq!(c.artifact_shape.d_ff, c.profile.ffn_dim as usize, "{}", c.model_id);
            assert_eq!(c.artifact_shape.vocab, c.profile.vocab_size as usize, "{}", c.model_id);
            assert_eq!(
                c.artifact_shape.n_heads * c.artifact_shape.d_head,
                c.profile.hidden_dim as usize,
                "{}: the artifact's width is not the graph's",
                c.model_id
            );
            match c.source {
                // The A16 tier: the artifact states what the weights can do (the converter's
                // rotary-table span), the profile states what the court admits — the two are
                // decoupled BY DESIGN, and the only inequality that matters is that the class
                // never asks for a position the artifact cannot rotate.
                ArtifactSourceV1::ConvertedA16 => assert!(
                    c.artifact_shape.max_position >= c.profile.n_ctx as usize,
                    "{}: the court would admit positions the artifact cannot rotate",
                    c.model_id
                ),
                _ => assert_eq!(c.artifact_shape.max_position, c.inventory_geometry.n_ctx as usize, "{}", c.model_id),
            }
        }
    }

    /// **The A16 family: the base model derives the class testnet-11's genesis registered, and
    /// the Coder sibling derives a DIFFERENT class at the next rung of the context ladder.**
    ///
    /// The first id is pinned byte-for-byte because it is on chain: a registry that stopped
    /// deriving it could no longer name the class the network already runs (the exact failure
    /// this rewrite closed — the old registry enumerated a non-A16 geometry no genesis ever
    /// registered, and `--palw-register-class` refused even the running class's own artifact).
    /// The sibling id only has to be admissible, distinct, and stable-under-this-test.
    #[test]
    fn the_a16_family_derives_the_genesis_class_and_a_distinct_sibling() {
        let base = canonical_class_by_model_id_v1(&court(), "Qwen/Qwen2.5-1.5B").expect("the base A16 model is in the registry");
        assert_eq!(
            base.class_id().to_string(),
            "f942e268f43f05461f648adcb76a1300dbedd93f022d3bba0e88c2ef4349e38f3ac1b70871f3b5195b3b2fb3da221f9c29fe291773a094596add6951aa7902c1",
            "the registry no longer derives the class testnet-11 registered"
        );
        assert_eq!(base.source, ArtifactSourceV1::ConvertedA16);
        assert_eq!(base.canonical_job, QWEN25_A16_CANONICAL);
        assert_eq!(base.profile.n_ctx, 16);

        let coder =
            canonical_class_by_model_id_v1(&court(), "Qwen/Qwen2.5-Coder-1.5B-Instruct").expect("the sibling is in the registry");
        assert_eq!(coder.profile.n_ctx, 18, "the sibling takes the next unburned rung of the ladder");
        assert_ne!(coder.class_id(), base.class_id(), "siblings must be DIFFERENT classes — the chain refuses a duplicate id");
        // And the whole reason registration needs a model id: the two entries are
        // indistinguishable by converted shape.
        assert_eq!(coder.artifact_shape, base.artifact_shape, "siblings share one converted shape by design");
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

    /// **The qwen36 table: two members, two DIFFERENT classes, and shapes that cannot cross.**
    ///
    /// The hybrid entry derives its id from the same geometry the genesis registration used, so
    /// it names the class the chain runs (the fleet's facts are the cross-check; a unit test here
    /// would compare the derivation to itself). What this test can and does pin: the qwen3moe
    /// entry is a distinct class, both profiles actually project, and a synthetic shape of either
    /// member matches exactly its own entry — the mixed cases all name the field that disagrees.
    #[test]
    fn the_qwen36_table_separates_its_members() {
        let table = qwen36_canonical_classes_v1();
        // Three members since the dense tier joined the lineage (Qwen/Qwen3.5-2B expressed as a
        // one-expert mixture) — the count is pinned so a row added without reading this test is
        // still a row added on purpose.
        assert_eq!(table.len(), 3);
        let hybrid = &table[0];
        let coder = &table[1];
        let dense = &table[2];
        assert_eq!(hybrid.model_id, "Qwen3.6-35B-A3B");
        assert_eq!(coder.model_id, "huihui-ai/Huihui-Qwen3-Coder-30B-A3B-Instruct-abliterated");
        assert_eq!(dense.model_id, "Qwen/Qwen3.5-2B");
        let hybrid_id = hybrid.class_id().expect("the hybrid geometry projects");
        let coder_id = coder.class_id().expect("the qwen3moe geometry projects");
        let dense_id = dense.class_id().expect("the dense geometry projects");
        assert_ne!(hybrid_id, coder_id, "two models must be two classes");
        assert_ne!(hybrid_id, dense_id, "the dense member is its own class");
        assert_ne!(coder_id, dense_id, "and not the coder's either");

        let shape_of = |c: &Qwen36CanonicalClassV1| crate::qwen36::Qwen36ShapeV1 {
            layer_types: (0..c.geometry.layer_count as usize)
                .map(|i| {
                    if c.geometry.full_attention_interval != 0 && (i + 1).is_multiple_of(c.geometry.full_attention_interval as usize) {
                        crate::qwen36::Qwen36LayerKind::FullAttention
                    } else {
                        crate::qwen36::Qwen36LayerKind::LinearAttention
                    }
                })
                .collect(),
            d_model: c.geometry.hidden_dim as usize,
            n_heads: c.geometry.attn_heads as usize,
            n_kv_heads: c.geometry.attn_kv_heads as usize,
            head_dim: c.geometry.attn_head_dim as usize,
            rotary_dim: c.geometry.rope_dims as usize,
            linear_k_heads: c.geometry.gdn_k_heads as usize,
            linear_v_heads: c.geometry.gdn_v_heads as usize,
            linear_head_dim: c.geometry.gdn_head_dim as usize,
            conv_kernel: c.geometry.gdn_conv_kernel as usize,
            n_experts: c.geometry.n_experts as usize,
            experts_per_token: c.geometry.experts_per_token as usize,
            moe_dim: c.geometry.moe_dim as usize,
            shared_dim: c.geometry.shared_dim as usize,
            vocab: c.geometry.vocab_size as usize,
            max_position: 512,
            eps_q: 1,
            router_up_bits: 20,
        };
        assert!(hybrid.shape_matches(&shape_of(hybrid)).is_ok());
        assert!(coder.shape_matches(&shape_of(coder)).is_ok());
        assert!(hybrid.shape_matches(&shape_of(coder)).is_err(), "a qwen3moe artifact must not pass as the hybrid");
        assert!(coder.shape_matches(&shape_of(hybrid)).is_err(), "the hybrid's artifact must not pass as qwen3moe");
    }

    /// **A16 entries never resolve through the floor engine.** The registry knows them (they are
    /// what registration derives), but `resolve_class_v1` must skip them: its resolution builds a
    /// full operand inventory — 1.7 GiB of tree per call for a dense artifact — and would then
    /// refuse on the root anyway, because an A16 registration pins the artifact's DIGEST. Their
    /// backend dispatch lives in the kaspad registry, keyed on the same ledger.
    #[test]
    fn a16_entries_are_unknown_to_the_floor_resolver() {
        let c = canonical_class_by_model_id_v1(&court(), "Qwen/Qwen2.5-1.5B").expect("1.5B");
        match resolve_class_v1(&court(), c.class_id(), Hash64::from_u64_word(1), &[]) {
            Err(ClassResolveError::UnknownClass { class_id }) => assert_eq!(class_id, c.class_id()),
            other => panic!("an A16 class id must be UnknownClass to the floor resolver, got {other:?}"),
        }
    }
}
