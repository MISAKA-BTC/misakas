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
            // The v1 A16 row registers the artifact's own digest — see
            // [`ArtifactSourceV1::ConvertedA16`] — and that spelling is kept for it: the row is
            // not court-capable (the one-byte map cannot describe its cache), so nothing opens
            // against its root. The COURT-CAPABLE row (the four-byte map is the discriminator,
            // the same predicate its backend answers `supports_court` with) registers the A16
            // operand-inventory root, because an arithmetic close's openings prove against the
            // registered root and nothing can be opened against a flat digest.
            //
            // **The discriminator is the backend's own `supports_court` predicate**
            // ([`crate::qwen25_a16_backend::a16_court_capable_v1`]), not a v2 equality written out
            // a third time. It was the equality, and ADR-0082's `graph-v5` row — which registers
            // the TILED map precisely so a dissection's bottom can open one history tile — fell
            // through to the flat digest: a court-capable class whose registered root nothing can
            // be opened against, which is the A16 genesis root form defect exactly.
            ArtifactSourceV1::ConvertedA16 => {
                if crate::qwen25_a16_backend::a16_court_capable_v1(&self.profile) {
                    Ok(crate::inventory::a16_inventory_v1(artifact, &self.profile)?.root())
                } else {
                    Ok(artifact.artifact_digest())
                }
            }
            _ => Ok(base0_inventory_v1(artifact, self.inventory_geometry)?.root()),
        }
    }

    /// Does this artifact have the shape this class is defined at? Checked field by field rather
    /// than by digest, so the error can say WHICH field disagrees — a digest comparison on a
    /// 1.7 GiB artifact says only "no".
    pub fn shape_matches(&self, artifact: &Base0ArtifactV1) -> Result<(), ClassResolveError> {
        self.shape_matches_shape_v1(&artifact.shape)
    }

    /// [`Self::shape_matches`] against a header on its own. The whole check reads
    /// `artifact.shape`, so the two are one spelling and a caller that holds only the header (a
    /// certification tool deciding which family a file belongs to) runs the identical comparison.
    pub fn shape_matches_shape_v1(&self, a: &Base0ShapeV1) -> Result<(), ClassResolveError> {
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

/// **The dense A16 converter's rotary span: the widest position an existing artifact can rotate.**
///
/// `qwen25-convert --a16` writes this into every artifact header as `max_position`, and
/// `qwen25-1.5b-a16.palwart` — the shipped file — declares it. It is stated ONCE here because two
/// things read it and they must not be able to disagree: the family's `artifact_shape` above (what
/// a header must say to be this family) and [`A16_GRAPH_V5_MODEL_ID`]'s width below (what the
/// graph-v5 row is projected at). A width that is not the header's is a class those weights cannot
/// execute — `A16ArtifactRowError::WiderThanTheArtifact` is the same fact stated as a refusal.
pub const A16_CONVERTER_ROTARY_SPAN_V1: u32 = 512;

/// The model id of the graph-v5 dense row — **and the width is IN the name**, because `n_ctx` is
/// inside `shape_profile_id` and therefore inside the class id, so a name that omitted it would be
/// a name that does not identify. See [`a16_graph_v5_row_v1`] for why this row exists at all when
/// [`a16_artifact_row_v1`]'s doc says a fourth fixed-width row must not be added.
pub const A16_GRAPH_V5_MODEL_ID: &str = "Qwen/Qwen2.5-1.5B/graph-v5@512";

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
    // axis that changes nothing else about the arithmetic. The base model sits at n_ctx 16, and
    // its id under the v1 graph is `f942e268…` (asserted by the test below). **That id is NOT what
    // testnet-11's genesis registers** — this comment said it was. The genesis calls
    // `qwen25_a16_registration_v2`, so the registered dense class is the graph-v2 row at the same
    // width, `71bbb755…`; `f942e268…` is the superseded v1 graph at that geometry. Same model,
    // same n_ctx, different graph, different class — the distinction this whole file is about,
    // got wrong in the file's own preamble. Each later model takes the next context bound. The REAL ceiling is the court's:
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
    // **`graph-v3` is the row whose epsilon the engine actually executes.** The v2 row corrected the
    // pre table and the state map; it left `rms_eps_q: 1` standing against an artifact the
    // converter builds at `1 << 8`, and the consequence was only measured when the model gate ran
    // on a real converted artifact: `Qwen25A16Backend::from_registered_profile` refuses EVERY dense
    // row — the registered n_ctx-16 one included — with `GeometryMismatch { rms_eps_q: profile 1,
    // artifact 256 }`, because `plan_from_profile` compares the two. So no dense class can be built
    // from its own registered profile, no dense class can be court-capable, and the shipped worker
    // only survives by using `::new`, which compiles no plan and never compares.
    //
    // A corrected epsilon moves `base0_rms_eps_q`, which moves the shape profile id, which IS the
    // class id — so this is a NEW ROW and not an edit, exactly as `graph-v2` was and exactly as the
    // hybrid's `graph-v3` rows are. The rows above stay byte-for-byte as the chain registered them.
    for (model_id, n_ctx, graph_v2, artifact_eps) in [
        ("Qwen/Qwen2.5-1.5B", 16u32, false, false),
        ("Qwen/Qwen2.5-Coder-1.5B-Instruct", 18, false, false),
        ("Qwen/Qwen2.5-1.5B/graph-v2", 16, true, false),
        ("Qwen/Qwen2.5-1.5B/graph-v3", 16, true, true),
    ] {
        let g = PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B };
        let g = if artifact_eps { kaspa_consensus_core::palw_qwen25_profile::qwen25_geometry_artifact_eps(g) } else { g };
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
                max_position: A16_CONVERTER_ROTARY_SPAN_V1 as usize,
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

    // **ADR-0082's graph-v5 dense row — the class the testnet-11 5f genesis registers.**
    //
    // **This sentence has now been wrong in BOTH tenses, hours apart.** It read "registers" while
    // the genesis still called `qwen25_a16_registration_v2` (set: f1c5635c / 5bd9ae3d /
    // 71bbb755, the graph-v2 row at n_ctx 16); it was corrected to "is intended to register"; and
    // fixer FG then landed `qwen25_a16_graph_v5_registration_v1` and made the original true again.
    // **A tense has a shelf life exactly as long as the thing it waits for**, and both spellings
    // were right on the day they were written — which is why the assertion is what carries this
    // now, not the prose: `the_artifact_names_a_class_the_shipped_genesis_actually_registers`
    // reads the shipped genesis object set and was RED before FG and green after, untouched.
    //
    // A `continue`-shaped absence would still be a build that cannot name the class its own chain
    // will run, which is the failure `resolve_class_v1`'s rewrite closed for the floor — so the
    // row stays; only the tense was wrong.
    if let Some(row) = a16_graph_v5_row_v1() {
        out.push(row);
    }
    out
}

/// **The graph-v5 dense row at the artifact's own width** (ADR-0082 Decisions 1 and 4;
/// `docs/testnet11-relaunch-5f-genesis-card.md` §2).
///
/// [`a16_artifact_row_v1`]'s doc says the obvious repair — a fourth fixed-width row — is the one
/// thing that must not be done, and that argument is about a DIFFERENT question: which class a
/// certification tool binds when it is handed a model id. There, a row makes the width a constant
/// again and a wrong width binds silently. Here the question is which classes this binary can
/// SUPPLY, and the genesis registers one that no row spells — so the alternatives are a row or a
/// node that cannot resolve its own chain's class.
///
/// The falsifiability the other doc asks for is kept, by construction rather than by promise:
///
/// * the width is **not typed here**. It is [`A16_CONVERTER_ROTARY_SPAN_V1`], the same constant the
///   family's `artifact_shape` states as `max_position` — so the row's `n_ctx` cannot come to
///   disagree with the header a file must have to be this family (pinned by
///   `the_graph_v5_row_is_the_artifacts_own_width`);
/// * the width is **in the model id**, so a row at another width cannot answer to this name;
/// * the graph is **not a second spelling**. It is
///   [`kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5`] — the ladder's
///   own projection, the one `PALW_LADDER_FAMILIES_V5[0]` prices and
///   `palw_close_chunks_for_ladder_v1` derives the close ceiling over — so the registry and the
///   cost derivation cannot describe different graphs. That was the A16 genesis root form defect:
///   one class root spelled two ways with nothing forcing them equal.
///
/// **The canonical job is derived, not chosen.** ADR-0077 Decision 14's floor is `n_ctx / 8`
/// (`palw_canonical_footprint_floor_v1`), and the family's own `QWEN25_A16_CANONICAL` — `(14, 2)`,
/// a footprint of 15 — is under it at this width, so the gate would refuse the row by name
/// (`CanonicalFootprintUnderTheRow { footprint: 15, floor: 64 }`). The decode half is the family's;
/// the prefill is whatever makes `palw_job_footprint_v1` exactly meet the floor.
///
/// `None` if the projection fails, for the same reason the loop above uses `continue`: a build that
/// cannot project a graph tables no class for it rather than one it cannot execute.
pub fn a16_graph_v5_row_v1() -> Option<CanonicalClassV1> {
    use kaspa_consensus_core::palw_context_ladder::{palw_a16_context_row_profile_v5, palw_canonical_footprint_floor_v1};

    let n_ctx = A16_CONVERTER_ROTARY_SPAN_V1;
    let profile = palw_a16_context_row_profile_v5(n_ctx).ok()?;
    // The geometry the projection was taken at, so the artifact shape below is the same numbers the
    // graph is — never a second transcription of the model.
    let g = kaspa_consensus_core::palw_qwen25_profile::qwen25_geometry_artifact_eps(PalwQwen25GeometryV1 {
        n_ctx,
        ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
    });
    // Decision 14's floor, met exactly: `prefill + max(1, decode) - 1 == floor`.
    let decode = QWEN25_A16_CANONICAL.1.max(1);
    let floor = u32::try_from(palw_canonical_footprint_floor_v1(n_ctx)).ok()?;
    let prefill = floor.saturating_add(1).checked_sub(decode)?;
    Some(CanonicalClassV1 {
        model_id: A16_GRAPH_V5_MODEL_ID,
        profile,
        artifact_shape: Base0ShapeV1 {
            n_layers: g.layer_count as usize,
            n_heads: g.attn_heads as usize,
            n_kv_heads: g.attn_kv_heads as usize,
            d_head: g.attn_head_dim as usize,
            d_ff: g.ffn_dim as usize,
            vocab: g.vocab_size as usize,
            max_position: A16_CONVERTER_ROTARY_SPAN_V1 as usize,
            ln_theta_gen_q: crate::artifact::LN_THETA_1000000_GEN_Q,
            eps_q: kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_ARTIFACT_EPS_Q,
        },
        canonical_job: (prefill, decode),
        source: ArtifactSourceV1::ConvertedA16,
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
    })
}

// =================================================================================================
// The A16 dense row an ARTIFACT names — the width that is not in the table above
// =================================================================================================

/// Why an artifact (or a width) names no bindable A16 dense class.
#[derive(Debug)]
pub enum A16ArtifactRowError {
    /// The file's header is not this family's arithmetic. Carries the FIRST field that disagrees,
    /// from [`CanonicalClassV1::shape_matches`] — the same check the panel pairs with.
    NotThisFamily(ClassResolveError),
    /// This build tables no A16 row at all, so there is nothing to check a header against.
    NoA16Rows,
    /// A width of zero is not a graph.
    ZeroWidth,
    /// The width asked for is wider than the artifact's own rotary table. The file cannot serve
    /// that row, so a certificate for it would be about weights that cannot execute it.
    WiderThanTheArtifact { asked: u64, span: u64 },
    /// The width matches MORE THAN ONE row, so the caller has not named a class. Two
    /// rows can be the same width and different classes — a class IS its graph — and picking one
    /// is how a wrong graph binds silently. Carries every row that matched, so the caller can say
    /// which with `--model-id`, whose names carry the graph.
    AmbiguousAtWidth { asked: u32, model_ids: Vec<&'static str> },
    /// No row is offered at this width. Carries what the catalog
    /// does offer, because "no" without the alternatives is a refusal a reader cannot act on.
    NoRowAtWidth { asked: u32, offered: Vec<(&'static str, u32)> },
}

impl std::fmt::Display for A16ArtifactRowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotThisFamily(e) => write!(f, "not an artifact of the A16 dense family: {e}"),
            Self::NoA16Rows => write!(f, "this build tables no A16 dense row, so no header can be paired against one"),
            Self::ZeroWidth => write!(f, "n_ctx 0 is not a graph"),
            Self::WiderThanTheArtifact { asked, span } => write!(
                f,
                "n_ctx {asked} is wider than the artifact's own rotary span ({span} positions) — these weights cannot serve that row"
            ),
            Self::AmbiguousAtWidth { asked, model_ids } => write!(
                f,
                "n_ctx {asked} names {} rows, not one: [{}]. They are different CLASSES at the same width — a class \
                 is its graph — so pass --model-id to say which; the names carry the graph",
                model_ids.len(),
                model_ids.join(", ")
            ),
            Self::NoRowAtWidth { asked, offered } => write!(
                f,
                "no A16 dense row is offered at n_ctx {asked}. This build offers [{}]. A width no row spells is a \
                 class nothing registers, so it is refused here rather than projected",
                offered.iter().map(|(m, n)| format!("{m} @ {n}")).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

impl std::error::Error for A16ArtifactRowError {}

/// The A16 dense class an artifact header names, and what was checked to get there.
#[derive(Clone, Debug)]
pub struct A16ArtifactRowV1 {
    /// The graph. `shape_profile_id()` is the class id.
    pub profile: PalwShapeProfileV3,
    /// The width the profile is at.
    pub n_ctx: u32,
    /// The artifact's own rotary span (`max_position`) — the widest row this file can serve, and
    /// the width taken when the caller names none.
    pub artifact_span: u32,
    /// Whether `n_ctx` came from the header (`false`) or from the caller narrowing it (`true`).
    pub narrowed: bool,
    /// The catalog row this resolved to. The profile above is ITS profile, not a projection — so
    /// a caller can name what it bound and a reader can find it in the registry.
    pub model_id: &'static str,
    /// The A16 rows whose declared artifact shape this header matches. These are the FAMILY, not
    /// the class: every row of the family declares the same artifact shape, which is precisely
    /// why a model id cannot name a width.
    pub family_rows: Vec<&'static str>,
}

/// **The dense A16 class an ARTIFACT can serve, at a width the artifact itself states.**
///
/// The A16 table above is three fixed widths (16, 18, 16), so `palw-certify bind --model-id`
/// could only ever produce those three classes. `n_ctx` is inside `PalwShapeProfileV3` and
/// therefore inside `shape_profile_id`, so **a model id does not determine a class**: the
/// testnet-11 5f genesis registers the dense tier at n_ctx 512 (fixer FG) and no row here spells
/// the wide one.
///
/// **The obvious repair — a fourth row at 512 — is the one thing that must not be done**, and the
/// reason is falsifiability rather than taste. A row makes the width a CONSTANT again, so a wrong
/// width binds to the wrong class in silence; naming the width, or naming the file that states
/// it, makes a wrong width fail to bind. That difference is the whole design. This table has
/// already failed in exactly that direction: `n_ctx` 17 is marked BURNED in its own comment above
/// by the 2026-08-28 mispairing that registered a class on chain against the genesis constant,
/// past a green suite.
///
/// Deeper: the defect is one class root spelled twice — once derived from the artifact's own
/// inventory, once from a constant — with nothing forcing the two equal (the A16 genesis root
/// defect). This removes the second spelling for this path rather than adding a third, so nothing
/// new about the graph is written here. Every part is borrowed:
///
/// * the **family** is identified by [`CanonicalClassV1::shape_matches`] against this table's A16
///   rows — the same call `DenseLineageV1::pair` makes when the panel pairs a holding with a class;
/// * the **width** is the header's `max_position`, the rotary table's span, which is the widest row
///   the file can serve — and is the number `palw_context_ladder`'s 512 already is ("the dense
///   artifact's rotary table covers 512 positions, the converter's default");
/// * the **graph** is `palw_a16_context_row_profile_v1`, the shipped ladder projection, under the
///   epsilon the artifact executes (`qwen25_a16_artifact_row_profile_v1`).
///
/// `n_ctx` may narrow the row below the header's span (a 256-wide row on a 512-position artifact
/// is a class those weights can serve); it may never widen it, because a rotary table that does
/// not reach the position cannot be executed at it.
///
/// **This does not register anything.** The chain must already carry the derived id as an Active
/// class; a `ClassLaneCertified` binds a lane of a class, it cannot create one.
pub fn a16_artifact_row_v1(
    court: &PalwCourtParamsV2,
    artifact: &Base0ArtifactV1,
    n_ctx: Option<u32>,
    model_id: Option<&str>,
) -> Result<A16ArtifactRowV1, A16ArtifactRowError> {
    a16_row_for_artifact_shape_v1(court, &artifact.shape, n_ctx, model_id)
}

/// **The one width→row choice both bind forms make**, so `--artifact` and `--n-ctx` cannot
/// disagree about which class a width names. `candidates` is what the caller was able to confirm:
/// the rows a header paired with, or — when no artifact is on this machine — every A16 row.
///
/// It never projects. A width no row spells is refused with what the catalog does offer, and a
/// width two rows spell is refused with both names, because two rows at one width are two
/// CLASSES (`n_ctx` is inside `shape_profile_id`) and picking one is how a wrong graph binds in
/// silence.
fn pick_a16_row_at_width_v1<'a>(
    candidates: &[&'a CanonicalClassV1],
    width: u32,
    model_id: Option<&str>,
) -> Result<&'a CanonicalClassV1, A16ArtifactRowError> {
    let mut at_width: Vec<&CanonicalClassV1> = candidates.iter().copied().filter(|c| c.profile.n_ctx == width).collect();
    if let Some(wanted) = model_id.filter(|m| !m.is_empty()) {
        at_width.retain(|c| c.model_id == wanted);
    }
    match at_width.len() {
        1 => Ok(at_width[0]),
        // **Refusing on ambiguity is the whole point.** Two rows can be the same width and
        // different classes; picking one is the original defect with a better input.
        n if n > 1 => {
            Err(A16ArtifactRowError::AmbiguousAtWidth { asked: width, model_ids: at_width.iter().map(|c| c.model_id).collect() })
        }
        _ => Err(A16ArtifactRowError::NoRowAtWidth {
            asked: width,
            offered: candidates.iter().map(|c| (c.model_id, c.profile.n_ctx)).collect(),
        }),
    }
}

/// [`a16_artifact_row_v1`] over the header alone — every check the derivation makes reads the
/// header and nothing else, and a test that had to build 1.5B parameters to exercise a width
/// check would not be run.
pub fn a16_row_for_artifact_shape_v1(
    court: &PalwCourtParamsV2,
    shape: &Base0ShapeV1,
    n_ctx: Option<u32>,
    model_id: Option<&str>,
) -> Result<A16ArtifactRowV1, A16ArtifactRowError> {
    let rows: Vec<CanonicalClassV1> =
        canonical_classes_v1(court).into_iter().filter(|c| matches!(c.source, ArtifactSourceV1::ConvertedA16)).collect();
    if rows.is_empty() {
        return Err(A16ArtifactRowError::NoA16Rows);
    }
    // The panel's own pairing check, per row, so the refusal names the field that disagrees
    // rather than saying only "no".
    let mut family: Vec<&CanonicalClassV1> = Vec::new();
    let mut first_mismatch: Option<ClassResolveError> = None;
    for row in &rows {
        match row.shape_matches_shape_v1(shape) {
            Ok(()) => family.push(row),
            Err(e) => {
                let _ = first_mismatch.get_or_insert(e);
            }
        }
    }
    if family.is_empty() {
        return Err(A16ArtifactRowError::NotThisFamily(first_mismatch.expect("a non-empty row set that matched nothing errored")));
    }

    let span = shape.max_position as u64;
    let asked = n_ctx.map(u64::from).unwrap_or(span);
    if asked == 0 {
        return Err(A16ArtifactRowError::ZeroWidth);
    }
    if asked > span {
        return Err(A16ArtifactRowError::WiderThanTheArtifact { asked, span });
    }
    let width = u32::try_from(asked).map_err(|_| A16ArtifactRowError::WiderThanTheArtifact { asked, span })?;

    // **The row is MATCHED, never projected.** Projecting a graph here was a second spelling of a
    // class root, which is the defect the doc above says this function avoids — and it did not:
    // it projected `palw_a16_context_row_profile_v1` while the registry supplies
    // `palw_a16_context_row_profile_v5` at 512, so a 512-position artifact named class
    // 8d2e6f16… and the genesis registered 4277d84f…. Same width, different graph, different
    // class, and a `ClassLaneCertified` for a class the chain would never hold.
    let picked = pick_a16_row_at_width_v1(&family, width, model_id)?;

    Ok(A16ArtifactRowV1 {
        profile: picked.profile.clone(),
        n_ctx: width,
        artifact_span: u32::try_from(span).unwrap_or(u32::MAX),
        narrowed: asked != span,
        model_id: picked.model_id,
        family_rows: family.iter().map(|c| c.model_id).collect(),
    })
}

/// **The dense A16 row at a width nothing on this machine states** — the lighter form, for an
/// operator who has no artifact where the certificate is being cut.
///
/// The same MATCH [`a16_artifact_row_v1`] ends at, with the artifact's two contributions removed:
/// nothing confirms the family and nothing bounds the width. That is exactly the difference the
/// doc on [`a16_artifact_row_v1`] argues about — a width taken on the operator's word can be wrong
/// in a way a file's own header cannot — so the artifact form is the primary one and this exists
/// for the case where the 1.7 GiB file is somewhere else.
///
/// **This used to PROJECT `palw_a16_context_row_profile_v1`, and that made it the last route in
/// this build that could name a class the chain does not hold.** The artifact route was rewritten
/// to match a catalog row precisely because projecting was a second spelling of the class root;
/// leaving the width-only route projecting kept the defect alive one flag over, where it is
/// hardest to see — `bind --n-ctx 512` printed a class id, every check below it passed (a
/// graph-v2 projection at 512 reaches kernels the graph-v2 family covers), and the certificate
/// named `8d2e6f16…` while the row this catalog spells at that width is `4277d84f…`. Same width,
/// different graph, different class, no error anywhere. (Neither is what the chain holds TODAY —
/// the genesis registered `71bbb755…`, the graph-v2 row at n_ctx 16, until fixer FG landed the v5
/// registration. The defect being described here is real either way: it is about two routes
/// disagreeing, not about which of them the chain later adopts.)
///
/// So this matches too, and a width no row spells now FAILS TO BIND instead of projecting. That
/// is the falsifiability the artifact route's doc argues for, applied to the form that has no
/// artifact to fall back on: the operator's word can still be wrong, but it can no longer be
/// wrong quietly.
pub fn a16_ladder_row_v1(
    court: &PalwCourtParamsV2,
    n_ctx: u32,
    model_id: Option<&str>,
) -> Result<(PalwShapeProfileV3, &'static str), A16ArtifactRowError> {
    if n_ctx == 0 {
        return Err(A16ArtifactRowError::ZeroWidth);
    }
    let rows: Vec<CanonicalClassV1> =
        canonical_classes_v1(court).into_iter().filter(|c| matches!(c.source, ArtifactSourceV1::ConvertedA16)).collect();
    if rows.is_empty() {
        return Err(A16ArtifactRowError::NoA16Rows);
    }
    // No artifact, so nothing narrows the candidates: every A16 row this build tables is in scope,
    // and the width is the only thing choosing. A header would have cut this set down first, which
    // is the whole of what `--artifact` buys.
    let candidates: Vec<&CanonicalClassV1> = rows.iter().collect();
    let picked = pick_a16_row_at_width_v1(&candidates, n_ctx, model_id)?;
    Ok((picked.profile.clone(), picked.model_id))
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
    /// Which node-table revision this row describes the geometry with. 1 is the genesis tables.
    /// 2 was the first correction (2026-09-01) and is RETIRED, never to be reused: its spelling
    /// reached testnet-11 (`6c3bdc67…`, the race ADR-0067 records) while still carrying the
    /// fourth defect and the 17 epsilon, so the number names that orphaned chain row and nothing
    /// in this table. 3 is the shipping correction (`qwen36_profile_v2`'s tables with all six
    /// findings closed, over the artifact-epsilon geometry). A class IS its graph, so every
    /// revision is its own class id over the same weights — the same arrangement the dense
    /// family shipped as `Qwen/Qwen2.5-1.5B/graph-v2`.
    pub graph_version: u8,
}

impl Qwen36CanonicalClassV1 {
    pub fn profile(&self) -> Result<PalwShapeProfileV3, kaspa_consensus_core::palw_step::PalwStepError> {
        match self.graph_version {
            1 => kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v1(self.geometry),
            _ => kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v2(self.geometry),
        }
    }

    /// The class id: its graph's id, same rule as the dense table's.
    pub fn class_id(&self) -> Option<Hash64> {
        self.profile().ok().map(|p| p.shape_profile_id())
    }

    /// Does this artifact have the shape this class is defined at? Dimension by dimension, so
    /// the error names the field. `max_position` is deliberately NOT compared — the artifact
    /// states what its rotary table covers, the profile states what the court admits — and the
    /// epsilons are not either. An earlier version of this sentence said "the engine's integer
    /// epsilon is the artifact format's, the class constant is the court's" — describing, without
    /// noticing, a court that diverges from every honest engine. That split was the fifth
    /// finding; the graph-v3 geometries close it by declaring the artifact's epsilon
    /// (`QWEN36_ARTIFACT_EPS_Q`), and the planner's geometry gate — not this shape check — is
    /// what refuses a declaration whose epsilon the artifact does not execute.
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
            graph_version: 1,
        },
        Qwen36CanonicalClassV1 {
            model_id: "huihui-ai/Huihui-Qwen3-Coder-30B-A3B-Instruct-abliterated",
            geometry: kaspa_consensus_core::palw_qwen36_profile::QWEN3_CODER_30B_A3B,
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
            graph_version: 1,
        },
        Qwen36CanonicalClassV1 {
            model_id: "Qwen/Qwen3.5-2B",
            geometry: kaspa_consensus_core::palw_qwen36_profile::QWEN35_2B,
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
            graph_version: 1,
        },
        // --- graph-v3: the corrected rows, one per geometry ---------------------------------------
        // The v1 rows above stay exactly as the chain registered them; these are the SAME weights
        // under the graph whose names, kernels and epsilon the engine actually executes — the six
        // measured findings closed (the shared-gate collision, the unnamed router widening, the
        // phantom V-cache node, the backwards expert wideness, the unexecuted epsilon, the
        // ungrouped scalar gate). An interpreter follows these rows; it can never follow the v1
        // rows, which is the finding that forced them to exist. The name is v3, not v2, because
        // "graph-v2" is burned: the superseded spelling reached testnet-11 first (tx `6c3bdc67…`)
        // and a registered name cannot be re-pointed at a different id. The geometry is the frozen
        // const under `qwen36_geometry_artifact_eps` — one declared difference, nothing
        // transcribed.
        Qwen36CanonicalClassV1 {
            model_id: "Qwen3.6-35B-A3B/graph-v3",
            geometry: kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B,
            ),
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
            graph_version: 3,
        },
        Qwen36CanonicalClassV1 {
            model_id: "huihui-ai/Huihui-Qwen3-Coder-30B-A3B-Instruct-abliterated/graph-v3",
            geometry: kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                kaspa_consensus_core::palw_qwen36_profile::QWEN3_CODER_30B_A3B,
            ),
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
            graph_version: 3,
        },
        Qwen36CanonicalClassV1 {
            model_id: "Qwen/Qwen3.5-2B/graph-v3",
            geometry: kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                kaspa_consensus_core::palw_qwen36_profile::QWEN35_2B,
            ),
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
            graph_version: 3,
        },
        // Qwen3.8-27B, 2026-09-04: added under graph-v3 only — a class registered today should
        // run the graph the engine executes, not the genesis tables' defective spelling, so it
        // gets no v1 row. Dense FFN as a one-expert mixture, the `Qwen/Qwen3.5-2B` precedent.
        Qwen36CanonicalClassV1 {
            model_id: "Qwen/Qwen3.8-27B/graph-v3",
            geometry: kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                kaspa_consensus_core::palw_qwen36_profile::QWEN38_27B,
            ),
            canonical_job: kaspa_consensus_core::palw_qwen36_profile::QWEN36_RC_CANONICAL,
            graph_version: 3,
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

    /// **A dense row whose declared epsilon is the one its artifact executes — and the older rows
    /// keep theirs.**
    ///
    /// The defect this pins was found by running the model gate on a real converted artifact, not
    /// by reading: `plan_from_profile` compares the profile's `base0_rms_eps_q` with the artifact's
    /// `eps_q` and refuses the pair, so `Qwen25A16Backend::from_registered_profile` fails for every
    /// dense row that existed before `graph-v3` — the chain-registered n_ctx-16 one included. A
    /// class that cannot be built from its own registered profile cannot be court-capable, and a
    /// free-prompt claim on a class that is not court-capable is a claim no dispute can reach.
    ///
    /// What this test checks is the catalog-level fact underneath that refusal: the profile's
    /// epsilon and the artifact shape's epsilon agree for `graph-v3` and disagree for its
    /// predecessors. What it deliberately does NOT check is that a plan compiles over a real
    /// artifact — that needs the 1.7 GiB file and belongs to the drill. Stated rather than implied,
    /// because a unit test that claimed the stronger thing would be claiming what it cannot see.
    #[test]
    fn the_graph_v3_dense_row_declares_the_epsilon_its_artifact_executes() {
        let court = PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2).expect("shipped court");
        let classes = canonical_classes_v1(&court);
        let row = |id: &str| classes.iter().find(|c| c.model_id == id).unwrap_or_else(|| panic!("no {id} row"));

        let v3 = row("Qwen/Qwen2.5-1.5B/graph-v3");
        assert_eq!(
            v3.profile.base0_rms_eps_q, v3.artifact_shape.eps_q,
            "the graph-v3 row must declare the epsilon the converter builds, or plan_from_profile refuses it"
        );
        assert_eq!(v3.profile.base0_rms_eps_q, kaspa_consensus_core::palw_qwen25_profile::QWEN25_A16_ARTIFACT_EPS_Q);

        // The predecessors carry the split, and that is a fact about the chain rather than a bug to
        // fix here: their ids are live and correcting one in place would rename a registered class.
        for older in ["Qwen/Qwen2.5-1.5B", "Qwen/Qwen2.5-1.5B/graph-v2"] {
            let r = row(older);
            assert_ne!(r.profile.base0_rms_eps_q, r.artifact_shape.eps_q, "{older} is expected to carry the historical split");
            assert_ne!(r.class_id(), v3.class_id(), "{older} and graph-v3 must be different classes");
        }
        // And the correction moved nothing else: same graph, same width, one declared difference.
        let v2 = row("Qwen/Qwen2.5-1.5B/graph-v2");
        assert_eq!(v3.profile.n_ctx, v2.profile.n_ctx);
        assert_eq!(v3.profile.pre_nodes.len(), v2.profile.pre_nodes.len());
        assert_eq!(v3.profile.attn_nodes.len(), v2.profile.attn_nodes.len());
        assert_eq!(v3.profile.state_chunk_map_id, v2.profile.state_chunk_map_id);
    }

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
        // Six members: three geometries, each under two graphs — the count is pinned so a row
        // added without reading this test is still a row added on purpose. The second trio is
        // graph-v3 (2026-09-01), the corrected node tables and epsilon over the SAME weights, and
        // the whole point of the pairing is that each is a different class than its v1 twin.
        assert_eq!(table.len(), 7);
        let hybrid = &table[0];
        let coder = &table[1];
        let dense = &table[2];
        // The v3 trio: frozen geometries under the artifact epsilon, corrected graph, and
        // therefore six pairwise-distinct ids.
        assert_eq!(table[3].model_id, "Qwen3.6-35B-A3B/graph-v3");
        assert_eq!(table[4].model_id, "huihui-ai/Huihui-Qwen3-Coder-30B-A3B-Instruct-abliterated/graph-v3");
        assert_eq!(table[5].model_id, "Qwen/Qwen3.5-2B/graph-v3");
        assert_eq!(table[6].model_id, "Qwen/Qwen3.8-27B/graph-v3");
        let mut ids = Vec::new();
        for row in &table {
            ids.push(row.class_id().expect("every row projects"));
        }
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "rows {i} and {j} are the same class — a graph revision that did not move the id");
            }
        }
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

    /// **The fifth finding stays closed, unconditionally.** The real-weights differential that
    /// found it (`qwen36_plan.rs`) is env-gated on a multi-gigabyte artifact, so it guards
    /// nothing in a plain checkout — this pins the same fact locally: every corrected row
    /// declares the epsilon the converter builds ([`QWEN36_ARTIFACT_EPS_Q`]), and every v1 row
    /// still declares its frozen 17, because a v1 geometry that moved would rename a class the
    /// chain already registered.
    #[test]
    fn the_corrected_rows_declare_the_artifact_epsilon() {
        for row in qwen36_canonical_classes_v1() {
            match row.graph_version {
                1 => assert_eq!(row.geometry.rms_eps_q, 17, "{}: the frozen v1 epsilon moved", row.model_id),
                _ => assert_eq!(
                    row.geometry.rms_eps_q,
                    kaspa_consensus_core::palw_qwen36_profile::QWEN36_ARTIFACT_EPS_Q,
                    "{}: a corrected row must declare the epsilon its artifacts execute",
                    row.model_id
                ),
            }
        }
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

    /// **A court-capable row registers a root a court can OPEN — and `graph-v5` is court-capable.**
    ///
    /// The discriminator here was `state_chunk_map_id == integer_kv_state_chunk_map_id_v2()`, the
    /// third copy of the predicate the two backend constructors also spelled. ADR-0082's v5 row
    /// registers the TILED map, so it fell through to `artifact_digest()` — a flat hash of a whole
    /// file, and `PalwProvenOperandsV1::from_openings_v1` proves openings against the registered
    /// root, so an arithmetic close on the row the genesis registers could have proven nothing.
    /// That is the A16 genesis root form defect exactly: one class root spelled two ways with
    /// nothing forcing them equal.
    ///
    /// All three rows are pinned, because the fix widens a predicate and a widened predicate is
    /// how the OTHER side gets broken: v5 → inventory (new), v2 → inventory (unmoved), and the
    /// one-byte-map v1 row → digest (unmoved, and it is a live chain fact).
    #[test]
    fn a_tiled_map_row_registers_the_inventory_root_and_the_one_byte_row_still_registers_the_digest() {
        use kaspa_consensus_core::palw_qwen25_profile::{qwen25_a16_artifact_row_profile_v5, qwen25_geometry_artifact_eps};
        use kaspa_consensus_core::palw_state_chunk_map as map;

        let geometry = qwen25_geometry_artifact_eps(PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 8,
            ffn_dim: 8,
            attn_heads: 2,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 32,
            n_threads: 1,
            ..QWEN25_1_5B
        });
        let artifact_shape = Base0ShapeV1 {
            n_layers: geometry.layer_count as usize,
            n_heads: geometry.attn_heads as usize,
            n_kv_heads: geometry.attn_kv_heads as usize,
            d_head: geometry.attn_head_dim as usize,
            d_ff: geometry.ffn_dim as usize,
            vocab: geometry.vocab_size as usize,
            max_position: geometry.n_ctx as usize,
            ln_theta_gen_q: crate::artifact::LN_THETA_1000000_GEN_Q,
            eps_q: geometry.rms_eps_q,
        };
        let artifact = Base0ArtifactV1::derive_deterministic(artifact_shape, 0x5A16)
            .expect("a valid shape")
            .with_a16_params(crate::engine_a16::derived_a16_store(&artifact_shape))
            .expect("the derived store is sorted and unique");

        let row = |profile: PalwShapeProfileV3| CanonicalClassV1 {
            model_id: "test/a16",
            profile,
            artifact_shape,
            canonical_job: QWEN25_A16_CANONICAL,
            source: ArtifactSourceV1::ConvertedA16,
            inventory_geometry: PALW_RC_BASE0_GEOMETRY,
        };

        let v5 = row(qwen25_a16_artifact_row_profile_v5(geometry).expect("a valid v5 profile"));
        assert!(map::palw_map_addresses_history_tiles_v1(&v5.profile), "the row under test must be the tiled one");
        let v5_root = v5.artifact_root(&artifact).expect("the v5 row has an inventory");
        assert_ne!(v5_root, artifact.artifact_digest(), "a court-capable row must not register a flat digest");
        assert_eq!(
            v5_root,
            crate::inventory::a16_inventory_v1(&artifact, &v5.profile).expect("the v5 inventory builds").root(),
            "it registers the operand inventory an opening proves against"
        );

        let v2 = row(kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(geometry).expect("a valid v2 profile"));
        assert_eq!(v2.profile.state_chunk_map_id, map::integer_kv_state_chunk_map_id_v2());
        assert_eq!(
            v2.artifact_root(&artifact).expect("the v2 row has an inventory"),
            crate::inventory::a16_inventory_v1(&artifact, &v2.profile).expect("the v2 inventory builds").root(),
            "widening the predicate must not move the row already registered"
        );

        let v1 = row(qwen25_a16_profile_v1(geometry).expect("a valid v1 profile"));
        assert!(!map::palw_map_addresses_history_tiles_v1(&v1.profile));
        assert_ne!(v1.profile.state_chunk_map_id, map::integer_kv_state_chunk_map_id_v2());
        assert_eq!(
            v1.artifact_root(&artifact).expect("the v1 row answers"),
            artifact.artifact_digest(),
            "the one-byte-map row is not court-capable and keeps the digest testnet-11 registered"
        );

        // **Is the tokenizer commitment a GENESIS INPUT?** The 5f card has to know, because the
        // shipped dense artifact declares none (64 zero bytes) and binding one is a re-conversion.
        // `ClassRegistered` carries exactly two identities — `class_id` and `artifact_root` — and
        // `PalwShapeProfileV3` has no tokenizer field, so the whole question is which root the row
        // registers. It is answered here rather than reasoned about, in both directions.
        let bound = artifact.clone().with_tokenizer_commitment(Base0ArtifactV1::tokenizer_commitment_of(b"{}"));
        assert_ne!(bound.artifact_digest(), artifact.artifact_digest(), "the commitment is inside the artifact digest");
        assert_eq!(
            v5.artifact_root(&bound).expect("the v5 row has an inventory"),
            v5_root,
            "a court-capable row registers the operand inventory, which the tokenizer is not in — so binding one \
             is NOT a genesis input for this row and the registered root does not move"
        );
        assert_ne!(
            v1.artifact_root(&bound).expect("the v1 row answers"),
            v1.artifact_root(&artifact).expect("the v1 row answers"),
            "a digest-rooted row IS moved by binding a tokenizer — which is why the v1 rows on chain cannot be re-bound"
        );
    }

    // ---------------------------------------------------------------------------------------
    // ADR-0082's graph-v5 dense row — the class the 5f genesis registers (fixer FG)
    // ---------------------------------------------------------------------------------------

    fn v5_row() -> CanonicalClassV1 {
        canonical_class_by_model_id_v1(&court(), A16_GRAPH_V5_MODEL_ID).expect("the graph-v5 dense row is in the registry")
    }

    /// The k-ary court a given ARMED ruleset plays: its derived arity, its court window, and its
    /// prompt-ids form. The same three fields `fuzz_a16::gate_accepts` prices a fused row under.
    ///
    /// **This doc used to say "every field is read off the bundle passed in", and one was not.**
    /// `prompt_ids_form` was the literal `MerkleV1` between two fields taken from the bundle, so
    /// the result LOOKED bundle-derived and was not. The shipped presets cannot be Merkle:
    /// `validate_palw_v2` refuses to assemble a ruleset with `palw_prompt_ids_merkle` armed,
    /// because "no writer or checker on this build reads it". So the close this priced — 81,599 B
    /// — was a court the network refuses to run, and the shipped figure is 81,312 B under Flat.
    ///
    /// 287 bytes, moved by a field nobody chose. That is ADR-0082 §1's own defect — a figure
    /// measured under one configuration quoted as if it held under another — committed inside the
    /// helper whose doc warned about it. The form is now the CALLER's, taken from the same
    /// `Params` that produced the bundle, via `palw_prompt_ids_form_at`: the one place the k-ary
    /// court is decided.
    fn kary_court(
        bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    ) -> kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
        kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
            dissection_arity: bundle.court.dissection_arity(),
            prompt_ids_form,
            window_court_daa: bundle.state.window_court(),
        }
    }

    /// A bundle that REGISTERS the row and then arms `palw_kary_court` — in that order, because
    /// the derived arity is a function of the registered set.
    ///
    /// `palw_attn_widest_registered_site_v2` walks `genesis_objects` for a fused site; with none
    /// the history search is empty and `palw_court_arity_v1` returns the smallest legal arity, 2.
    /// Deriving the arity against an EMPTY bundle and then admitting a fused row under it would be
    /// a court sized for a dispute it does not have — and the gate would refuse the row anyway
    /// (`PricedForADifferentCourt`), because the price is taken at the arity the ruleset froze.
    fn armed_with(
        mut bundle: kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
        registration: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
        bundle.genesis_objects.push(registration.clone());
        let (history, lanes) = kaspa_consensus_core::palw_court_v2::palw_attn_widest_registered_site_v2(&bundle);
        bundle.court =
            kaspa_consensus_core::palw_court_v2::palw_court_params_at_v2(&bundle, true).expect("the windows derive a legal arity");
        println!(
            "armed ruleset: window_court {} turn_deadline {} ladder {} widest fused site history {history} lanes {lanes} \
             -> dissection arity {}",
            bundle.state.window_court(),
            bundle.court.turn_deadline_daa(),
            bundle.court.max_step_leaf_count(),
            bundle.court.dissection_arity()
        );
        bundle
    }

    /// The DEVNET ruleset (`palw_fp_devnet_bundle_v3`) — the one item 2 arms — and the RC's, which
    /// is the ruleset the 5f genesis cuts. Both are exercised because their windows differ and the
    /// row has to be admissible under each at ITS own derived arity.
    fn armed_rulesets(
        registration: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Vec<(
        &'static str,
        kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    )> {
        let devnet = kaspa_consensus_core::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
            Hash64::from_u64_word(0xC1A55),
            Hash64::from_u64_word(0xCA7),
            Hash64::from_u64_word(0xC0757),
            4_096,
            Hash64::from_u64_word(0xA7),
            kaspa_consensus_core::palw_fp_devnet_v3::palw_devnet_bond_registry_v1(
                kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(),
            ),
        )
        .expect("the devnet bundle assembles");
        let rc = match kaspa_consensus_core::config::params::palw_rc_shipped_params().palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b,
            other => panic!("the RC preset is not a v2 bundle: {other:?}"),
        };
        // The form each preset actually plays, from the Params that produced the bundle rather
        // than from a literal beside it. Neither can be Merkle — `validate_palw_v2` refuses an
        // armed `palw_prompt_ids_merkle` — but that is a fact to READ, not one to assume: if a
        // build ever ships the writer and arms the fence, these numbers must move with it.
        let devnet_form = kaspa_consensus_core::config::params::devnet_shipped_params().palw_prompt_ids_form_at(0);
        let rc_form = kaspa_consensus_core::config::params::palw_rc_shipped_params().palw_prompt_ids_form_at(0);
        vec![("devnet", armed_with(devnet, registration), devnet_form), ("rc", armed_with(rc, registration), rc_form)]
    }

    /// **The row's width is the artifact header's, not a number typed beside it.**
    ///
    /// This is the whole answer to [`a16_artifact_row_v1`]'s "a fourth row at 512 is the one thing
    /// that must not be done": the objection is that a row makes the width a CONSTANT that can
    /// silently disagree with the file. Here the two are one constant, and the graph is the
    /// ladder's own projection rather than a second spelling of it — so a wrong width cannot be
    /// expressed, which is what `n_ctx` 17 (BURNED, 2026-08-28) was.
    #[test]
    fn the_graph_v5_row_is_the_artifacts_own_width() {
        let row = v5_row();
        assert_eq!(row.profile.n_ctx, A16_CONVERTER_ROTARY_SPAN_V1, "the graph-v5 row is projected at the converter's span");
        assert_eq!(
            row.artifact_shape.max_position, A16_CONVERTER_ROTARY_SPAN_V1 as usize,
            "the header a file must have to be this family states the same span the row is projected at"
        );
        assert!(
            row.model_id.ends_with(&format!("@{}", row.profile.n_ctx)),
            "n_ctx is inside the class id, so it must be inside the name: {} at n_ctx {}",
            row.model_id,
            row.profile.n_ctx
        );
        // Not a second spelling of the graph: the ladder's own v5 projection, the one
        // `PALW_LADDER_FAMILIES_V5[0]` prices and `palw_close_chunks_for_ladder_v1` derives over.
        let ladder_projection =
            kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5(A16_CONVERTER_ROTARY_SPAN_V1)
                .expect("the ladder projects the v5 row");
        assert_eq!(
            row.class_id(),
            ladder_projection.shape_profile_id(),
            "the registry and the cost derivation must describe ONE graph — the A16 genesis root form defect otherwise"
        );
        // And it is a different class from every row that came before it.
        for other in canonical_classes_v1(&court()).iter().filter(|c| c.model_id != row.model_id) {
            assert_ne!(other.class_id(), row.class_id(), "{} collides with the graph-v5 row", other.model_id);
        }
        println!("graph-v5 dense row: {} n_ctx {} class {}", row.model_id, row.profile.n_ctx, row.class_id());
    }

    /// **The genesis row's graph is the graph-v3 row with the attention site fused — one class,
    /// one spelling.**
    ///
    /// The 5f integrator's ask, and the sharpest form of the falsifiability worry
    /// [`a16_artifact_row_v1`]'s doc raises. Two paths could produce a "graph-v5 dense 512" class:
    /// the table row (what this build supplies) and a derivation from the ARTIFACT — take the
    /// dense graph the artifact's own arithmetic runs, at the width its header states, and apply
    /// ADR-0082 Decision 1's fusion. If those two ever hashed differently, the chain would carry
    /// one class under two ids and every registration would be a coin flip about which.
    ///
    /// So the second path is spelled out here, from its parts, rather than by calling the same
    /// constructor twice: `qwen25_a16_artifact_row_profile_v1` (the graph-v3 dense projection
    /// under the epsilon the artifact executes) at `A16_CONVERTER_ROTARY_SPAN_V1` — which
    /// `the_graph_v5_row_is_the_artifacts_own_width` pins to the header field `max_position` —
    /// then `palw_fuse_attention_site_v5` over its attention table, then Decision 4's tiled map.
    /// Nothing else moves, which is exactly what Decision 1 claims.
    #[test]
    fn the_graph_v5_row_is_the_graph_v3_row_with_its_attention_site_fused() {
        use kaspa_consensus_core::palw_base0_profile::palw_fuse_attention_site_v5;
        use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B, qwen25_a16_artifact_row_profile_v1};
        use kaspa_consensus_core::palw_state_chunk_map as map;

        let row = v5_row();
        let n_ctx = A16_CONVERTER_ROTARY_SPAN_V1;
        assert_eq!(n_ctx as usize, row.artifact_shape.max_position, "the width must be the artifact header's");

        let mut derived = qwen25_a16_artifact_row_profile_v1(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B })
            .expect("the graph-v3 dense row projects at the artifact's width");
        // The v3 row and the v5 row differ in exactly two declared things (Decision 1 and
        // Decision 4). Asserted before the fusion so a third difference cannot hide inside it.
        assert_eq!(derived.state_chunk_map_id, map::integer_kv_state_chunk_map_id_v2(), "the v3 row declares the one-chunk map");
        assert_ne!(derived.shape_profile_id(), row.class_id(), "an unfused v3 row must not already be the v5 class");

        derived.attn_nodes = palw_fuse_attention_site_v5(&derived.attn_nodes).expect("the v3 attention site fuses");
        derived.state_chunk_map_id = map::tiled_kv_state_chunk_map_id_v3();
        derived.validate_shape().expect("the fused table is a well-formed graph");

        assert_eq!(
            derived.shape_profile_id(),
            row.class_id(),
            "the registered graph-v5 row and the artifact-derived fusion are two spellings of one class and they disagree"
        );
        // And the whole struct, not just its id: an id equality could in principle be reached by
        // two profiles the hash does not separate, and the hash is not the thing being tested.
        assert_eq!(derived, row.profile, "the two spellings agree on the id and differ in the graph");
        assert_eq!(
            derived.attn_nodes.len() + 3,
            qwen25_a16_artifact_row_profile_v1(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }).expect("projects").attn_nodes.len(),
            "the fusion turns four committed attention nodes into one"
        );
    }

    /// **The kernel set the graph-v5 row reaches, by id AND by name** — what a fixture that
    /// certifies this class must reach, and nothing else.
    ///
    /// Printed rather than only asserted, because the consumer of this fact is a person building a
    /// drill fixture in another crate: a set stated as a count is not checkable, and one stated as
    /// bare hashes is not readable. The names come from the adjudicator's own descriptor table
    /// (`palw_step_refute::KDESC_*`), so a kernel this build cannot name would show as `?` rather
    /// than be silently dropped.
    ///
    /// The two assertions are the shape of the gap: the v5 row reaches the fused kernel, and it
    /// does NOT reach the four the fusion replaced. That is why a "union with the graph-v2 row"
    /// family cannot be drilled — no profile reaches both sides — and why the fixture has to be a
    /// v5 profile with its own family.
    #[test]
    fn the_graph_v5_rows_kernel_set_is_what_a_fixture_must_reach() {
        use kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1;
        use kaspa_consensus_core::palw_step_refute as kd;
        // `KDESC_ALL` rather than a hand list: a kernel this build catalogues but this test forgot
        // would print as `?` and read as a defect in the row.
        let named: Vec<(&str, kaspa_consensus_core::Hash64)> =
            kd::KDESC_ALL.iter().map(|d| (*d, kaspa_consensus_core::palw_step::kernel_semantics_id_v1(d))).collect();
        let name_of = |id: &kaspa_consensus_core::Hash64| {
            named.iter().find(|(_, k)| k == id).map(|(n, _)| *n).unwrap_or("? (not a KDESC this test lists)")
        };

        let v5 = reachable_kernels_v1(&v5_row().profile);
        println!("graph-v5 dense @ 512 reaches {} kernels:", v5.len());
        for id in &v5 {
            println!("  {}  {}", &id.to_string()[..16], name_of(id));
        }
        let fused = kaspa_consensus_core::palw_step::kernel_semantics_id_v1(kd::KDESC_A16_ATTN_FUSED);
        assert!(v5.contains(&fused), "the graph-v5 row must reach the fused kernel — that is what makes it graph-v5");

        let v2 = reachable_kernels_v1(
            &kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v2(
                kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B_A16,
            )
            .expect("the graph-v2 row projects"),
        );
        let replaced: Vec<&str> =
            [kd::KDESC_A16_ATTN_SCORES, kd::KDESC_A16_SOFTMAX, kd::KDESC_A16_REQUANTIZE, kd::KDESC_A16_ATTN_VALUES]
                .into_iter()
                .filter(|d| {
                    let id = kaspa_consensus_core::palw_step::kernel_semantics_id_v1(d);
                    v2.contains(&id) && !v5.contains(&id)
                })
                .collect();
        println!("graph-v2 reaches {} kernels; the fusion replaces {replaced:?}", v2.len());
        assert!(!v5.is_subset(&v2), "the v5 row reaches a kernel the v2 row does not");
        assert!(!v2.is_subset(&v5), "the v2 row reaches kernels v5 drops — so no single profile reaches the union");
    }

    /// **The registered row declares the TILED map, and therefore registers the INVENTORY root.**
    ///
    /// Both halves by name. `tiled_kv_state_chunk_map_id_v3` is the map ADR-0082 Decision 4's
    /// checkpoint route requires — stream I measured that a v5 row carrying the v2 map is refused
    /// by the checkpoint route as `NotTheTiledMap`, which leaves the row prosecutable only on the
    /// cache-write route stream I found unsound. And [`a16_court_capable_v1`] is the ONE predicate
    /// `CanonicalClassV1::artifact_root` dispatches on: a court-capable row registers the operand
    /// inventory, because `PalwProvenOperandsV1::from_openings_v1` proves openings against the
    /// registered root and nothing opens against a flat file digest.
    ///
    /// The dispatch itself is exercised over a real artifact by
    /// `a_tiled_map_row_registers_the_inventory_root_and_the_one_byte_row_still_registers_the_digest`;
    /// what this adds is that the row the GENESIS registers is on the inventory side of it, which a
    /// synthetic profile cannot say.
    #[test]
    fn the_graph_v5_row_declares_the_tiled_map_and_is_court_capable() {
        use kaspa_consensus_core::palw_state_chunk_map as map;
        let row = v5_row();
        assert_eq!(
            row.profile.state_chunk_map_id,
            map::tiled_kv_state_chunk_map_id_v3(),
            "the graph-v5 row must declare the tiled map — Decision 4's checkpoint route refuses any other by name"
        );
        assert!(map::palw_map_addresses_history_tiles_v1(&row.profile), "the tiled map addresses the history a tile at a time");
        assert!(
            crate::qwen25_a16_backend::a16_court_capable_v1(&row.profile),
            "a16_court_capable_v1 is the predicate artifact_root dispatches on; a false here registers a flat digest"
        );
        assert!(
            kaspa_consensus_core::palw_class_admission_v2::palw_profile_has_fused_attention_v1(&row.profile),
            "graph-v5 IS the fused attention site (Decision 1)"
        );
        assert!(
            !row.profile
                .attn_nodes
                .iter()
                .any(|n| matches!(n.out_len, kaspa_consensus_core::palw_step::PalwStepOutLenV1::KvScaled { .. })),
            "Z0: no committed row of a graph-v5 class has a context-shaped width"
        );
    }

    /// **The row is ADMITTED under an armed k-ary court and REFUSED BY NAME without it** — and the
    /// close it is admitted at is printed, under the ruleset, the map and the route it was measured
    /// on. Run over BOTH armed rulesets, because they derive different arities.
    ///
    /// The refusal is the 5f card §6 fused-attention guard: `FusedAttentionNeedsTheKaryCourt`, "the
    /// class carries a fused attention site and this ruleset's court has no dissection to try it
    /// with". A guard on the way in, beside the drill and not instead of it — the property is
    /// stream I's e2e court drill.
    #[test]
    fn the_graph_v5_row_is_admitted_by_the_kary_court_and_refused_without_it() {
        use kaspa_consensus_core::palw_class_admission_v2 as adm;
        use kaspa_consensus_core::palw_context_ladder as ladder;

        let row = v5_row();
        let canonical =
            kaspa_consensus_core::palw_base0_profile::rc_job_context(&row.profile, row.canonical_job.0, row.canonical_job.1);
        assert!(
            ladder::palw_footprint_meets_the_row_v1(&row.profile, &canonical),
            "Decision 14's floor is {} and the row's canonical job {:?} has footprint {}",
            ladder::palw_canonical_footprint_floor_v1(row.profile.n_ctx),
            row.canonical_job,
            ladder::palw_job_footprint_v1(row.canonical_job.0, row.canonical_job.1)
        );

        // **The shipped registration helper cannot assemble this row, and that is a defect and not
        // a property of the row.** `palw_post_genesis_registration_v1` counts `pwu_per_inference`
        // with `step_leaf_count`, which caps at the EXECUTOR's `PALW_STEP_MAX_LEAVES` (2^22) —
        // while the gate it feeds recounts against the RULESET's ladder
        // (`worst_case_step_leaf_count_capped_v1(profile, rules.ladder)`). ADR-0080 W1b made the
        // executor read the field and `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` moved to 2^26 for exactly
        // this row; the registration side was not moved with it, so the ONE helper that assembles a
        // `ClassRegistered` refuses the class the genesis registers. Pinned rather than worked
        // around silently — the fix is a `_capped_` twin in `palw_class_admission_v2`, which is not
        // this stream's file (reported to the integrator).
        //
        // **The twin landed** as `palw_post_genesis_registration_capped_v1` (audit D H-5b), and the
        // SDK's registration path passes `bundle.court.max_step_leaf_count()`. This arm stays, and
        // now pins the UNCAPPED wrapper's documented default: it is the executor's `2^22`, on
        // purpose, so a caller with no ruleset in hand still gets the old number rather than a
        // silently different one.
        match adm::palw_post_genesis_registration_v1(
            row.profile.clone(),
            canonical.clone(),
            Hash64::from_u64_word(0xA271FAC7),
            0,
            1,
            1,
            0,
            kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                kaspa_consensus_core::tx::TransactionId::default(),
                0,
            )),
            Vec::new(),
        ) {
            Err(adm::PalwClassAdmissionError::Profile(msg)) if msg.contains("exceeding the") => {
                println!("registration helper refuses the graph-v5 512 row (uncapped count): {msg}");
            }
            other => panic!(
                "the helper's cap moved — if it now counts against the ruleset's ladder this arm should be deleted, got {other:?}"
            ),
        }

        // Counted against the RULESET's ladder, which is what the gate recounts against.
        let counted = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(
            &row.profile,
            &canonical,
            kaspa_consensus_core::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
        )
        .expect("the canonical job counts against the fenced ladder");
        println!(
            "graph-v5 dense @ n_ctx {}: canonical job {:?} = {counted} step leaves, counted against the FENCED ladder {} \
             (PALW_CONTEXT_LADDER_MAX_STEP_LEAVES); the executor default PALW_STEP_MAX_LEAVES is {} and the RC ruleset's \
             own ladder is {}",
            row.profile.n_ctx,
            row.canonical_job,
            kaspa_consensus_core::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES,
            kaspa_consensus_core::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT
        );
        let registration = kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
            class_id: row.class_id(),
            artifact_root: Hash64::from_u64_word(0xA271FAC7),
            slash_value_per_pwu: 1,
            pwu_rule: kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
            initial_target: 1,
            share_permille: 0,
            activation_daa: 0,
            admission: Some(Box::new(kaspa_consensus_core::palw_state_v2::PalwClassAdmissionCarriageV2 {
                profile: row.profile.clone(),
                canonical: canonical.clone(),
                registrant_bond: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(
                    kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::default(), 0),
                ),
                signature: Vec::new(),
            })),
        };

        for (name, bundle, prompt_ids_form) in armed_rulesets(&registration) {
            let kary = kary_court(&bundle, prompt_ids_form);
            // Priced for the court that can try it — at the RULESET's ladder, the number the chain
            // recounts against. This priced at `PALW_CONTEXT_LADDER_MAX_STEP_LEAVES` (2^32) and
            // pinned 83,175 B: six rungs more disclosure than the 2^26 court either preset runs,
            // 1,863 bytes above the 81,312 B the genesis assembly registers the same row at — two
            // routes, one row, two prices, and nothing compared them (misaka-testnet-44,
            // 2026-09-03). The route-agreement assertion below is what compares them now.
            let rules = ladder::palw_class_ladder_rules_for_court_v1(&row.profile, Some(kary), bundle.court.max_step_leaf_count())
                .expect("a mapped class has ladder rules");
            let admitted =
                adm::verify_class_admission_v5(&bundle, &row.profile, &canonical, &registration, &[], &[], Some(rules), Some(kary));
            assert!(
                admitted.is_ok(),
                "{name}: the graph-v5 row must be admissible under its own armed court — ruleset ladder {} \
                 (max_step_leaf_count), ladder-rule ladder {}, arity {}: {admitted:?}",
                bundle.court.max_step_leaf_count(),
                rules.ladder,
                kary.dissection_arity
            );

            // The close, DERIVED here and printed with the configuration it was measured under.
            //
            // **Which EVIDENCE ROUTE this is charged at is stream K's answer, not a choice here.**
            // Per-position attention checkpoints cover every position, so the cache-write route is
            // refused for this class and the walk charges the checkpoint route — which is why the
            // binding close is one carrier rather than the three it was when the bottom was priced
            // at the cache-write route (216,019 B). A test that swapped the route by hand would be
            // pricing a route the ruleset no longer plays.
            let cost_rows = adm::derive_court_cost_rows_v1(&row.profile, rules.cost_shape).expect("the cost walk prices the row");
            let binding = cost_rows.first().expect("a priced row has a binding node").clone();
            let chunks = kaspa_consensus_core::palw_mode_v2::palw_close_chunks_for_bytes_v1(binding.close_bytes);
            println!(
                "[{name}] graph-v5 dense @ n_ctx {} arity {} {:?} ids map tiled_v3 checkpoint route: binding close {} B = {} \
                 chunk(s) at {}[{}] {:?} {}",
                row.profile.n_ctx,
                kary.dissection_arity,
                kary.prompt_ids_form,
                binding.close_bytes,
                chunks,
                binding.table,
                binding.index,
                binding.op_kind,
                binding.weight_name
            );
            assert_eq!(
                chunks, 1,
                "{name}: the registered row must be ONE carrier — {} B is {chunks} chunks, and a class whose close needs \
                 more than one carrier cannot be prosecuted on a shipped build",
                binding.close_bytes
            );

            // **Pinned, with the configuration in the message.** `palw_context_ladder`'s own sweep
            // states 82,719 for this row at arity 16 — the arity stream E measured with a 128-lane
            // site. The derivation here reads the ruleset that registers THIS row and returns 2
            // (window 3,000, turn deadline 42, ladder 2^26, history 512 positions at a 16-position
            // tile, 8 lanes), and the whole difference is the per-move disclosure: 1,120 bytes, so
            // 81,599 + 1,120 = 82,719 and the two sweeps agree. A moved arity moves this number and
            // this test says by how much.
            //
            // This pinned `(214_899, 81_599)` before stream K — the cache-write close beside the
            // checkpoint one. There is one route now and therefore one number.
            // **Pinned WITH its whole configuration in the message**, because this number has now
            // been three different values in one afternoon and every one of them was a correct
            // measurement of a court somebody was not running:
            //
            //   81,599  arity 2, MerkleV1 ids  — a court `validate_palw_v2` REFUSES to assemble
            //   83,175  arity 2, Flat ids      — the shipped court, both presets, and the same
            //                                    figure the genesis-registered profile prices at
            //
            // The Merkle figure came from this very helper hardcoding `prompt_ids_form` between
            // two fields it read off the bundle. 287 bytes moved by a field nobody chose.
            // **Route agreement, not a literal**: the genesis assembly prices this row through
            // `qwen25_a16_graph_v5_registration_v1` and the admission/bind route through the rules
            // above; one row under one court has one price, and the chain's is the genesis one.
            let (_, genesis_entry, _) = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_graph_v5_registration_v1(
                Hash64::from_u64_word(0xA271FAC7),
                0,
                1,
                1,
                &bundle,
                prompt_ids_form,
                kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                    kaspa_consensus_core::tx::TransactionId::default(),
                    0,
                )),
            )
            .expect("the genesis route prices the row on this ruleset");
            assert_eq!(
                binding.close_bytes,
                genesis_entry.court_cost.max_close_bytes,
                "{name}: the graph-v5 512 row is priced differently by the genesis route and the admission route — \
                 measured at arity {}, {:?} ids, window_court {}, ladder {}. One row under one court has one price; \
                 check which input the two routes disagree on before touching either number.",
                kary.dissection_arity,
                kary.prompt_ids_form,
                kary.window_court_daa,
                bundle.court.max_step_leaf_count()
            );
            assert_eq!(
                binding.close_bytes, 81_312,
                "{name}: the shipped row's close at the ruleset's 2^26 ladder (was 83,175 at the 2^32 constant)"
            );

            // And WITHOUT the fence: the guard refuses the same row by name.
            match adm::verify_class_admission_v5(&bundle, &row.profile, &canonical, &registration, &[], &[], None, None) {
                Err(adm::PalwClassAdmissionError::FusedAttentionNeedsTheKaryCourt) => {}
                other => panic!("{name}: an unfenced ruleset must refuse the graph-v5 row by name, got {other:?}"),
            }
        }
    }
}
