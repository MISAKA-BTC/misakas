//! **RFC-0002 Phase F, step F6: admission v10 — the gate an IR class registration passes**
//! (`docs/design/palw/tir/phase-f-integration.md` §2.4), the registration builder, and the
//! preflight a node asks before it signs.
//!
//! [`verify_class_admission_v10`] is the IR twin of `verify_class_admission_v9`: the same questions
//! (is every dispute adjudicable, does the longest job fit the ladder, does prosecuting it cost what
//! the ruleset allows, is `pwu_per_inference` the counted one, is weight earned), asked of a program
//! instead of a shape profile, cheapest refusal first:
//!
//! 1. the class's and layout's versions; the program within the fence's `max_program_bytes`, and
//!    decoded strictly (spec 04b §4.4: normal form, types, ranges);
//! 2. the program's `prim_set_id` is the fence's; its logits scheme is flat or tiled; its logits row
//!    is no wider than its token bound (a decoded token is fed back as an input); the held history
//!    bound is not admitted in IR v1 (the held regime's machinery is legacy-binding-shaped);
//! 3. the layout: the step space builds ([`PalwTirStepSpaceV1::new`]: one tile per committed node,
//!    state tiles, `h_tile`); `max_context` within the program's history bound and the fence's;
//!    the unrolled node count within `max_unrolled_nodes`;
//! 4. `tir_admit_v1` (tir/core) at the fence's position, state and cone-work ceilings, once per
//!    distinct commit tile length (at most [`PALW_TIR_MAX_DISTINCT_TILES_V1`]); the peak live bytes;
//! 5. every commit point's cone at its own tile length: a cone that reduces over the history must be
//!    adjudicable whole until the history dissection is wired (step F7); the tile's MACs within the
//!    terminal ceiling; the tile's evaluation plus the worst `Fixed`-state replay the layout's
//!    checkpoint interval admits within the IR court's work limits
//!    (`palw_court_v2::palw_tir_court_limits_v1`), so an honest close is never refused for its work;
//!    the program, the opened operand bytes and the frame within `max_close_bytes`;
//! 6. the canonical job: exactly the attempt formula's yardstick context
//!    ([`crate::palw_tir_attempt_v1::palw_tir_job_context_v1`] at `(f − 1, 2)`), its prompt within
//!    J5b's inline bound (4,096 ids — the only prompt check an IR claim has); the deepest legal job
//!    within the ladder; `pwu_per_inference` the canonical count;
//! 7. the class id is `tir_class_id_v1(class, artifact_root)`;
//! 8. weight: a nonzero share needs a certified family covering the program's primitives
//!    (`family_certified_for_weight_v2` over the `palw-tir/v1/prim=<Name>` ids) — registration at
//!    0‰ stays permissionless (ADR-0069 D5).
//!
//! The close-bytes check of item 5 is a NECESSARY condition: `tir_admit_v1` reports the operand
//! bytes a worst tile demands at element granularity, and a close carries whole step leaves and
//! whole inventory pieces with their paths. The court's own cost gate refuses an oversize close; a
//! sufficient bound needs the per-leaf demand from `tir_admit_v1` (a tir/core follow-up).

use std::collections::BTreeSet;

use crate::Hash64;
use crate::palw_class_admission_v2::{PalwClassAdmissionError, PalwCourtCostV1, PalwHeldAdmissionV1};
use crate::palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2};
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use crate::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_of_v1, palw_tir_job_context_v1};
use crate::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1};
use crate::palw_tir_step_v1::PalwTirStepSpaceV1;
use crate::palw_tir_v1::PalwTirFenceV1;
use crate::palw_v2::PalwJobContextV2;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::admit::{TirAdmissionV1, TirAdmitError, TirAdmitInputsV1, TirCeilingsV1, tir_admit_v1};

/// The most distinct commit tile lengths a layout may use: admission runs `tir_admit_v1` once per
/// length, so this bounds its work at a small multiple of one run.
pub const PALW_TIR_MAX_DISTINCT_TILES_V1: usize = 8;

/// **The bytes an IR close carries beside the program and the operands it opens**: the binding's
/// other fields (the job context, the layout, the tokenizer, the roots), the disputed leaf's opening
/// and preimage frame, the range sibling sets and the artifact paths' framing. A frame, not a
/// measurement of paths: see the module doc on why the close check is a necessary condition.
pub const PALW_TIR_CLOSE_FRAME_BYTES_V1: u64 = 16 << 10;

/// The version of [`PalwTirClassRecordV1`].
pub const PALW_TIR_CLASS_RECORD_VERSION_V1: u16 = 1;

/// **The kernel id of a TIR primitive**: `kernel_semantics_id_v1("palw-tir/v1/prim=<Name>")` — a
/// namespace the legacy catalog never holds (design §2.7), so an IR class's `reachable_kernels` can
/// be certified by a family that names these ids and by nothing else.
pub fn palw_tir_prim_kernel_id_v1(name: &str) -> Hash64 {
    crate::palw_step::kernel_semantics_id_v1(&format!("palw-tir/v1/prim={name}"))
}

/// Every primitive's kernel id, in the primitive set's order.
pub fn palw_tir_prim_kernel_ids_v1() -> BTreeSet<Hash64> {
    misaka_palw_tir::prim::PRIM_NAMES_V1.iter().map(|name| palw_tir_prim_kernel_id_v1(name)).collect()
}

/// **The primitives a program reaches**, as kernel ids — its catalog entry's `reachable_kernels`,
/// and the set a certified family must cover for weight.
pub fn palw_tir_reachable_prims_v1(program: &TirProgramV1) -> BTreeSet<Hash64> {
    program.blocks.iter().flat_map(|b| b.nodes.iter()).map(|n| palw_tir_prim_kernel_id_v1(n.prim.name())).collect()
}

/// **What admission v10 reads from the ruleset at the registration's block** — resolved once,
/// by [`Self::at`], for the acceptance path and the node's preflight alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirAdmissionRulesV1 {
    /// `Params::palw_tir_v1`'s value (in force at the block).
    pub fence: PalwTirFenceV1,
    /// `Params::palw_held_context` (and the panel-DA reading) at the block.
    pub held: PalwHeldAdmissionV1,
    /// `Params::palw_prompt_ids_form_at` at the block.
    pub prompt_ids_form: PalwPromptIdsFormV1,
    /// **The k-ary court at the block** (`Params::palw_kary_court`, the arity the ruleset derives,
    /// its window): `None` where it is not armed. A cone that reduces over the history is dissected
    /// only under it (spec 04b §9.5, RFC-0002 F7); without it such a cone must fit the court whole.
    pub court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
}

impl PalwTirAdmissionRulesV1 {
    /// The rules at `daa_score`, or `None` where `palw_tir_v1` is not in force (an IR registration
    /// is then dropped by name, as an older build skips it).
    pub fn at(params: &crate::config::params::Params, daa_score: u64) -> Option<Self> {
        let fence = params.palw_tir_v1_fence().filter(|f| f.activation.is_active(daa_score))?;
        let held_armed = params.palw_held_context_active_at(daa_score);
        // The court the acceptance path resolves (`palw_court_params_at`), as the legacy shape reads
        // it (`palw_admission_shape_at_v1`): held, its arity is derived with no leaf ladder.
        let court = match &params.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) if params.palw_kary_court_active_at(daa_score) => {
                crate::palw_court_v2::palw_court_params_held_at_v2(bundle, true, held_armed).ok().map(|derived| {
                    crate::palw_class_admission_v2::PalwKaryCourtV1 {
                        dissection_arity: derived.dissection_arity(),
                        prompt_ids_form: params.palw_prompt_ids_form_at(daa_score),
                        window_court_daa: bundle.state.window_court(),
                    }
                })
            }
            _ => None,
        };
        Some(Self {
            fence,
            held: PalwHeldAdmissionV1 { armed: held_armed, panel_da: params.palw_panel_da_at(daa_score) },
            prompt_ids_form: params.palw_prompt_ids_form_at(daa_score),
            court,
        })
    }
}

/// **What the chain keeps of an admitted IR class** (the `tir_classes` table's row): the facts its
/// attempt jobs, its DA draws and its court read, so none of them decodes a program — and the
/// program itself, which every IR object that carries a binding references by class instead of
/// carrying (the coordinator's decision of 2026-09-28: an accusation fits one carrier whatever the
/// program's size). Every field is derived by admission from the carried class; none is declared.
///
/// **Rooted without the program's bytes**: [`Self::rooted_bytes_v1`] is the record with `program`
/// emptied, and `graph_ir_root` — the keyed hash of the program — commits to it; a carriage whose
/// program does not hash to its `graph_ir_root` is refused at load ([`Self::check_program_v1`]).
/// So a state root costs nothing per program byte, and the program stays shared (`Arc`) across the
/// candidate states a node holds.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirClassRecordV1 {
    /// [`PALW_TIR_CLASS_RECORD_VERSION_V1`].
    pub version: u16,
    /// J5's inputs (the class id among them).
    pub facts: PalwTirJobFactsV1,
    pub graph_ir_root: Hash64,
    pub layout_digest: Hash64,
    pub tokenizer_id: Hash64,
    pub prim_set_id: Hash64,
    /// The logits row's width: what a trace event's tile index is bounded by.
    pub logits_vocab: u32,
    /// The program's canonical bytes, counted.
    pub program_bytes: u32,
    /// **The dissected commit points** (spec 04b §9.5.1), `(block, node)` in block then node order:
    /// the commit points whose cone reduces over `H`
    /// ([`crate::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1`]). A class with any owes
    /// the terminal move of its court (`PalwClassStateV2::fused_attention`): a root claim at a
    /// dissected leaf, an acquitting close at any other.
    pub dissected: Vec<(u8, u16)>,
    /// The program's canonical bytes (`graph_ir_root` is their keyed hash): what the chain puts
    /// back into every IR binding an object carries with its program empty.
    pub program: std::sync::Arc<Vec<u8>>,
}

impl PalwTirClassRecordV1 {
    /// A row with every field derived from `class_id` alone — for tests that need a row to exist.
    #[cfg(test)]
    pub(crate) fn test_row_v1(class_id: Hash64) -> Self {
        Self {
            version: PALW_TIR_CLASS_RECORD_VERSION_V1,
            facts: PalwTirJobFactsV1 { class_id, max_context: 64, token_bound: 8, tiled: true, held: false },
            graph_ir_root: class_id,
            layout_digest: class_id,
            tokenizer_id: class_id,
            prim_set_id: crate::palw_tir_v1::palw_tir_prim_set_id_v1(),
            logits_vocab: 8,
            program_bytes: 1,
            dissected: Vec::new(),
            program: std::sync::Arc::new(Vec::new()),
        }
    }

    /// **The bytes the `tir_classes` root commits for this record**: the record with its program
    /// emptied (any field added later is committed with it, by construction). The program is
    /// committed through `graph_ir_root`.
    pub fn rooted_bytes_v1(&self) -> Vec<u8> {
        let view = Self { program: std::sync::Arc::new(Vec::new()), ..self.clone() };
        borsh::to_vec(&view).expect("a record is borsh-serializable")
    }

    /// **The load check**: the program hashes to `graph_ir_root` and has the recorded length — what
    /// makes rooting the record without the program's bytes a commitment to them.
    pub fn check_program_v1(&self) -> Result<(), &'static str> {
        if self.program.len() as u64 != self.program_bytes as u64 {
            return Err("a tir_classes row's program is not its recorded length");
        }
        if crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&self.program) != self.graph_ir_root {
            return Err("a tir_classes row's program does not hash to its graph_ir_root");
        }
        Ok(())
    }

    /// Logits tiles per row under the class's scheme: one for the flat scheme, `⌈vocab / 4096⌉` for
    /// the tiled one — the event space a data-availability draw picks from.
    pub fn logits_tiles(&self) -> u32 {
        if self.facts.tiled {
            (self.logits_vocab as usize).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES) as u32
        } else {
            1
        }
    }
}

/// **An IR class's record, derived from the class and its inventory root** — the ONE derivation
/// admission v10 returns and the fold writes (it decodes the program strictly; the program is
/// returned for the caller's other readings). Every field is a function of the carried class.
pub fn palw_tir_class_record_v1(
    class: &PalwTirClassV1,
    artifact_root: &Hash64,
) -> misaka_palw_tir::TirResult<(PalwTirClassRecordV1, TirProgramV1)> {
    let program = class.decode_program()?;
    let class_id = class.class_id(artifact_root);
    let post = &program.blocks[program.schedule.post as usize];
    let logits_vocab = post.nodes[program.logits as usize].out.elements_at(1);
    let record = PalwTirClassRecordV1 {
        version: PALW_TIR_CLASS_RECORD_VERSION_V1,
        facts: PalwTirJobFactsV1::of(class, &program, class_id),
        graph_ir_root: class.graph_ir_root(),
        layout_digest: class.layout_digest(),
        tokenizer_id: class.tokenizer_id,
        prim_set_id: Hash64::from_bytes(program.prim_set_id),
        logits_vocab: u32::try_from(logits_vocab).unwrap_or(u32::MAX),
        program_bytes: u32::try_from(class.program.len()).unwrap_or(u32::MAX),
        dissected: crate::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(&program),
        program: std::sync::Arc::new(class.program.clone()),
    };
    Ok((record, program))
}

/// **An IR binding as the chain adjudicates it**: the binding an object carried — its class's
/// program EMPTY, as every IR object carries it (the program is the registered class's, held in
/// its `tir_classes` row) — with the record's program put back. Refused when the object carried a
/// program (one encoding per object; the chain already holds the bytes). Whether the filled class
/// IS the registered class is then the binding check's: its id must be the one the job context
/// names and the claim recorded.
pub fn palw_tir_binding_with_program_v1(
    binding: &crate::palw_tir_step_v1::PalwTirStepBindingV1,
    record: &PalwTirClassRecordV1,
) -> Result<crate::palw_tir_step_v1::PalwTirStepBindingV1, &'static str> {
    if !binding.class.program.is_empty() {
        return Err("an IR binding on chain carries no program: the chain holds the registered class's");
    }
    let mut filled = binding.clone();
    filled.class.program = record.program.as_ref().clone();
    Ok(filled)
}

/// **Empties a binding's program** — what a filer does to every IR binding before an object carries
/// it ([`palw_tir_binding_with_program_v1`] is the chain's inverse).
pub fn palw_tir_binding_strip_program_v1(binding: &mut crate::palw_tir_step_v1::PalwTirStepBindingV1) {
    binding.class.program = Vec::new();
}

fn tir_program_error(e: misaka_palw_tir::TirError) -> PalwClassAdmissionError {
    PalwClassAdmissionError::TirProgram(e.to_string())
}

fn exceeds(what: &'static str, got: u64, ceiling: u64) -> Result<(), PalwClassAdmissionError> {
    if got > ceiling { Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling }) } else { Ok(()) }
}

/// **What one lifecycle object may weigh on the wire**: every dissection move rides one carrier (only
/// a `FamilyCertified` rides in chunks), whose payload is at most one object chunk.
pub const PALW_TIR_DISSECT_CARRIER_BYTES_V1: u64 = crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES as u64;

/// **The object frame of a dissection move beside its payload**: the variant tag, the session id,
/// the arity or round, and the mover's ML-DSA-87 signature (4,627 bytes) with its length prefix.
pub const PALW_TIR_DISSECT_MOVE_FRAME_BYTES_V1: u64 = 1 + 64 + 4 + 4 + 4_627 + 64;

/// **Admission's part of the history dissection, for one dissected cone** (spec 04b §9.5.6):
///
/// * O-1 to O-3 ([`crate::palw_tir_dissect_v1::palw_tir_dissect_obligations_v1`]);
/// * the claim's values, bounded by the box demand at `H = 1`
///   ([`crate::palw_tir_dissect_v1::palw_tir_dissect_value_bound_v1`]), within the claim cap;
/// * a round at the court's arity fits one carrier, and so does the root claim — the close frame, the
///   chunk's opened operands (what the finalize and its probes read, bounded by one chunk) and the
///   claim; the program is referenced, never carried — both NECESSARY conditions, as the close
///   check is;
/// * O-5: the whole exchange inside `window_court` at the court's arity, on the network's clock
///   (the held clock opens at the accusation's leaf: no ladder rounds), over `max_context` positions
///   in `h_tile` tiles — the legacy fused row's window rule applied to this site.
#[allow(clippy::too_many_arguments)]
fn palw_tir_dissected_cone_admits_v1(
    bundle: &PalwConsensusParamsV2,
    rules: &PalwTirAdmissionRulesV1,
    class: &PalwTirClassV1,
    block: &misaka_palw_tir::program::Block,
    bi: u8,
    ni: u16,
    tile_len: u32,
    cone: &misaka_palw_tir::admit::ConeV1,
    k: crate::palw_class_admission_v2::PalwKaryCourtV1,
) -> Result<(), PalwClassAdmissionError> {
    use crate::palw_tir_dissect_v1 as d;
    let refused = |why: String| PalwClassAdmissionError::TirDissection { block: bi, node: ni, why };
    d::palw_tir_dissect_obligations_v1(block, ni).map_err(|e| refused(e.to_string()))?;
    let reductions = d::palw_tir_cone_reductions_v1(block, ni).len();
    let values = d::palw_tir_dissect_value_bound_v1(block, ni, tile_len);
    if values > d::PALW_TIR_DISSECT_MAX_VALUES as u64 {
        return Err(refused(format!("a claim may carry {values} values; at most {}", d::PALW_TIR_DISSECT_MAX_VALUES)));
    }
    let round = d::palw_tir_dissect_round_bytes_v1(k.dissection_arity, reductions, values).saturating_add(PALW_TIR_DISSECT_MOVE_FRAME_BYTES_V1);
    exceeds("IR dissection round bytes", round, PALW_TIR_DISSECT_CARRIER_BYTES_V1)?;
    // The root claim references the registered program (its binding rides with the program empty),
    // so what it weighs is the close frame, the opened evidence, the claim and the move's frame.
    let root = PALW_TIR_CLOSE_FRAME_BYTES_V1
        .saturating_add(cone.terminal_opened_bytes())
        .saturating_add(values.saturating_mul(20))
        .saturating_add(PALW_TIR_DISSECT_MOVE_FRAME_BYTES_V1);
    exceeds("IR dissection root claim bytes", root, PALW_TIR_DISSECT_CARRIER_BYTES_V1)?;
    let played = bundle
        .court
        .with_dissection_arity(k.dissection_arity)
        .map_err(|e| refused(format!("the court's dissection arity is not legal: {e}")))?;
    let history = class.layout.max_context as u64;
    let tile = class.layout.h_tile.max(1);
    let admits = if rules.held.armed {
        crate::palw_attn_court_v1::palw_attn_court_admits_row_held_v1(&played, history, tile, k.window_court_daa)
    } else {
        crate::palw_attn_court_v1::palw_attn_court_admits_row_v1(&played, history, tile, k.window_court_daa)
    };
    admits.map(|_| ()).map_err(|e| match e {
        crate::palw_attn_court_v1::PalwAttnCourtError::OverrunsWindow { moves, deadline, reserve, window_court } => {
            PalwClassAdmissionError::CourtWindowTooShort { needed: moves.saturating_mul(deadline).saturating_add(reserve), window: window_court }
        }
        _ => PalwClassAdmissionError::CourtWindowTooShort { needed: u64::MAX, window: k.window_court_daa },
    })
}

/// **Admission v10: may this IR class registration join?** Returns the catalog entry and the
/// `tir_classes` record the fold writes. `certified` must hash to `bundle.court_e2e_root` (it is read
/// only for a nonzero share); `chain_certified` is the chain's own certified families.
pub fn verify_class_admission_v10(
    bundle: &PalwConsensusParamsV2,
    rules: &PalwTirAdmissionRulesV1,
    registration: &PalwConsensusObjectV2,
    certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
    chain_certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
) -> Result<(PalwClassCatalogEntryV2, PalwTirClassRecordV1), PalwClassAdmissionError> {
    let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, artifact_root, pwu_rule, share_permille, admission, .. } =
        registration
    else {
        return Err(PalwClassAdmissionError::NotARegistration);
    };
    let class = &admission.class;
    let canonical = &admission.canonical;
    let ceilings = &rules.fence.ceilings;

    // 1. Versions, bytes, decoding.
    if class.version != PALW_TIR_CLASS_VERSION_V1 || class.layout.version != PALW_TIR_LAYOUT_VERSION_V1 {
        return Err(PalwClassAdmissionError::TirLayout("the class or its layout is not version 1".into()));
    }
    exceeds("IR program bytes", class.program.len() as u64, ceilings.max_program_bytes as u64)?;
    let program = class.decode_program().map_err(tir_program_error)?;

    // 2. The primitive set, the scheme, the token bound, the history bound.
    let prim_set = Hash64::from_bytes(program.prim_set_id);
    if prim_set != rules.fence.prim_set_id {
        return Err(PalwClassAdmissionError::TirPrimSet { program: prim_set, fence: rules.fence.prim_set_id });
    }
    let scheme = Hash64::from_bytes(program.logits_scheme_id);
    if scheme != crate::palw_step_refute::flat_logits_scheme_id_v1() && scheme != crate::palw_step_refute::tiled_logits_scheme_id_v1()
    {
        return Err(PalwClassAdmissionError::TirLayout(format!(
            "the program commits its logits under scheme {scheme}, which this build cannot adjudicate"
        )));
    }
    let post = &program.blocks[program.schedule.post as usize];
    let logits_vocab = post.nodes[program.logits as usize].out.elements_at(1);
    if logits_vocab == 0 || logits_vocab > program.token_bound as u64 {
        return Err(PalwClassAdmissionError::TirLayout(format!(
            "the logits row is {logits_vocab} wide and the token bound is {}: a selected token must be an input the program \
             accepts",
            program.token_bound
        )));
    }
    if program.history_bound == misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err(PalwClassAdmissionError::TirLayout(
            "a program at the held history bound is not admitted by IR v1: the held regime's accusations and answers are \
             legacy-binding-shaped"
                .into(),
        ));
    }

    // 3. The layout and the unrolled size.
    let layout = &class.layout;
    if layout.max_context > program.history_bound || layout.max_context > ceilings.max_context {
        return Err(PalwClassAdmissionError::TirLayout(format!(
            "max_context {} exceeds the program's history bound {} or the network's {}",
            layout.max_context, program.history_bound, ceilings.max_context
        )));
    }
    let space = PalwTirStepSpaceV1::new(class).map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    let unrolled: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
    exceeds("IR unrolled nodes", unrolled, ceilings.max_unrolled_nodes as u64)?;

    // 4. `tir_admit_v1`, once per distinct commit tile length, at the fence's ceilings; the court's
    //    own tile bounds are applied per commit point below, at the point's own length.
    let tile_lens: BTreeSet<u32> = layout.commit_tiles.iter().copied().collect();
    if tile_lens.len() > PALW_TIR_MAX_DISTINCT_TILES_V1 {
        return Err(PalwClassAdmissionError::TirLayout(format!(
            "{} distinct commit tile lengths; at most {PALW_TIR_MAX_DISTINCT_TILES_V1}",
            tile_lens.len()
        )));
    }
    let admit_ceilings = TirCeilingsV1 {
        max_tile_macs: u64::MAX,
        max_tile_transcendentals: u64::MAX,
        max_tile_opened_bytes: u64::MAX,
        max_tile_operands: u64::MAX,
        max_position_macs: ceilings.max_macs_per_position,
        max_position_transcendentals: u64::MAX,
        max_state_bytes: ceilings.max_state_bytes,
        max_step_leaves: u64::MAX,
        max_checkpoint_interval: layout.checkpoint_interval.max(1),
        max_cone_work: ceilings.max_cone_work,
    };
    let mut runs: Vec<(u32, TirAdmissionV1)> = Vec::with_capacity(tile_lens.len());
    for &tile_len in &tile_lens {
        let inputs = TirAdmitInputsV1 { tile_len, h_chunk: layout.h_tile, ceilings: admit_ceilings };
        let admitted = tir_admit_v1(&class.program, &inputs).map_err(|e| match e {
            TirAdmitError::Program(e) => tir_program_error(e),
            TirAdmitError::Exceeds { limit, at, value, cap } => PalwClassAdmissionError::TirExceeds { limit, at, value, cap },
            TirAdmitError::Inputs(why) => PalwClassAdmissionError::TirLayout(why.into()),
        })?;
        runs.push((tile_len, admitted));
    }
    let first = &runs.first().expect("a program has a logits commit point, so a tile length").1;
    exceeds("IR peak live bytes", first.position.peak_live_bytes, ceilings.max_peak_live_bytes)?;

    // 5. Every commit point's cone at its own tile length, against the court.
    let limits = crate::palw_court_v2::palw_tir_court_limits_v1(&bundle.court);
    let work_limit = limits.max_elements.min(limits.max_terms);
    let checkpoint = layout.checkpoint_interval as u64;
    let mut commit_tiles = layout.commit_tiles.iter();
    let (mut worst_close, mut worst_macs, mut worst_operands) = (0u64, 0u64, 0u64);
    for (bi, block) in program.blocks.iter().enumerate() {
        for (ni, node) in block.nodes.iter().enumerate() {
            if !node.commit {
                continue;
            }
            let tile_len = *commit_tiles.next().expect("the step space checked one tile per committed node");
            let run = &runs.iter().find(|(t, _)| *t == tile_len).expect("a run per distinct length").1;
            let cone = run
                .cones
                .iter()
                .find(|c| c.block as usize == bi && c.node as usize == ni)
                .expect("tir_admit_v1 costs every commit point");
            // **A cone that reduces over the history is DISSECTED under the k-ary court** (spec 04b
            // §9.5, RFC-0002 F7): its terminal is one `h_tile` chunk (`cone.terminal()`), and what
            // admission owes the court is that the exchange can be played — the obligations of
            // §9.5.6, a round and a root claim that each fit one carrier, the whole dispute inside
            // `window_court`. Without the court nothing dissects, so such a cone is adjudicated whole
            // and its tile at `H = W` must fit like any other.
            let dissected = !cone.h_reductions.is_empty() && rules.court.is_some();
            if !cone.h_reductions.is_empty() {
                match rules.court {
                    None => {
                        let whole = cone.tile.macs.saturating_add(cone.tile.elementwise).saturating_add(cone.tile.transcendentals);
                        if cone.tile.macs > bundle.court.max_terminal_macs() || whole > work_limit {
                            return Err(PalwClassAdmissionError::TirNeedsDissection { block: bi as u8, node: ni as u16 });
                        }
                    }
                    Some(k) => palw_tir_dissected_cone_admits_v1(bundle, rules, class, block, bi as u8, ni as u16, tile_len, cone, k)?,
                }
            }
            let (tile, opened) = if dissected { (cone.terminal(), cone.terminal_opened_bytes()) } else { (&cone.tile, cone.tile_opened_bytes) };
            exceeds("IR tile multiply-accumulates", tile.macs, bundle.court.max_terminal_macs())?;
            let mut work = tile.macs.saturating_add(tile.elementwise).saturating_add(tile.transcendentals);
            for leaf in &cone.leaves {
                if let misaka_palw_tir::admit::LeafV1::State(j) = leaf
                    && let Some(state) = run.states.iter().find(|s| s.state == *j)
                {
                    let per = state.per_position;
                    let per = per.macs.saturating_add(per.elementwise).saturating_add(per.transcendentals);
                    work = work.saturating_add(checkpoint.saturating_sub(1).saturating_mul(per));
                }
            }
            exceeds("IR cone evaluation work (tile and state replay)", work, work_limit)?;
            let close = (class.program.len() as u64).saturating_add(PALW_TIR_CLOSE_FRAME_BYTES_V1).saturating_add(opened);
            exceeds("IR close bytes", close, bundle.court.max_close_bytes())?;
            worst_close = worst_close.max(close);
            worst_macs = worst_macs.max(tile.macs);
            worst_operands = worst_operands.max(cone.operands);
        }
    }

    // 6. The canonical job, the ladder, the counted pwu.
    let derived_id = class.class_id(artifact_root);
    let facts = PalwTirJobFactsV1::of(class, &program, derived_id);
    let formula = palw_tir_attempt_canonical_of_v1(layout.max_context).ok_or_else(|| {
        PalwClassAdmissionError::TirCanonicalNotTheFormula(format!(
            "max_context {} is too narrow for the attempt formula (f − 1, 2), f = max_context / 8 ≥ 2",
            layout.max_context
        ))
    })?;
    let expected = palw_tir_job_context_v1(&facts, formula);
    if *canonical != expected {
        return Err(PalwClassAdmissionError::TirCanonicalNotTheFormula(format!(
            "the canonical job must be the attempt formula's yardstick context at ({}, {})",
            formula.0, formula.1
        )));
    }
    // **Every IR claim's prompt is attributable.** The identity rule recomputes the anchor's prompt
    // root inline (J5b) up to 4,096 ids, and no IR route opens a longer prompt tile by tile (the
    // legacy `PromptNotAnchored` carries a legacy binding): so an IR class's canonical prompt is at
    // most the inline bound — `max_context` ≤ 32,783 — until such a route exists.
    if formula.0 > crate::palw_attempt_rules_v1::PALW_J5_INLINE_PROMPT_IDS_V1 {
        return Err(PalwClassAdmissionError::TirCanonicalNotTheFormula(format!(
            "a {}-id canonical prompt is past J5b's inline bound of {} ids, and no IR route attributes a longer one \
             (max_context at most 32,783)",
            formula.0,
            crate::palw_attempt_rules_v1::PALW_J5_INLINE_PROMPT_IDS_V1
        )));
    }
    let _ = rules.prompt_ids_form;
    let ladder = bundle.court.max_step_leaf_count();
    let deepest = PalwJobContextV2 {
        declared_prefill_tokens: 1,
        exact_decode_tokens: layout.max_context,
        max_context_tokens: u32::MAX,
        ..expected.clone()
    };
    let worst = space
        .leaf_count_capped(&deepest, ladder)
        .map_err(|_| PalwClassAdmissionError::DeeperThanTheLadder { worst: u64::MAX, ladder })?;
    let counted = space.leaf_count_capped(canonical, ladder).map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    if counted > worst {
        return Err(PalwClassAdmissionError::CanonicalDeeperThanWorstCase { canonical: counted, worst });
    }
    match pwu_rule {
        PalwPwuRuleV2::MaxPerAttempt(_) => return Err(PalwClassAdmissionError::ClassIsNotDerived),
        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } if *pwu_per_inference != counted => {
            return Err(PalwClassAdmissionError::PwuPerInferenceMismatch { declared: *pwu_per_inference, counted });
        }
        PalwPwuRuleV2::DerivedV1 { .. } => {}
    }

    // 7. The id.
    if *class_id != derived_id {
        return Err(PalwClassAdmissionError::TirClassIdIsNotDerived { declared: *class_id, derived: derived_id });
    }

    // 8. Weight.
    let reachable = palw_tir_reachable_prims_v1(&program);
    if *share_permille > 0 {
        let covered = crate::palw_e2e_adjudicability::family_certified_for_weight_v2(
            bundle.court_e2e_root,
            certified,
            chain_certified,
            &reachable,
        )
        .map_err(|e| PalwClassAdmissionError::Profile(e.to_string()))?;
        if covered.is_none() {
            return Err(PalwClassAdmissionError::NotEndToEndCertified { share: *share_permille });
        }
    }

    let entry = PalwClassCatalogEntryV2 {
        class_id: derived_id,
        artifact_root: *artifact_root,
        max_step_leaf_count: worst,
        canonical_step_leaf_count: counted,
        reachable_kernels: reachable,
        court_cost: PalwCourtCostV1 {
            max_close_bytes: worst_close,
            max_terminal_macs: worst_macs,
            max_operand_count: u32::try_from(worst_operands).unwrap_or(u32::MAX),
        },
    };
    // The record by the one derivation the fold writes it with.
    let (record, _) = palw_tir_class_record_v1(class, artifact_root).map_err(tir_program_error)?;
    debug_assert_eq!(record.facts, facts);
    Ok((entry, record))
}

/// **An IR class registration for a running chain** — the twin of
/// `palw_post_genesis_registration_capped_v1`. The class id is derived from the class and the
/// inventory root; `pwu_per_inference` is counted from the carried canonical job against `ladder`
/// (the network's), so the object and the gate's recount are one count. The signature is the
/// caller's: build once with an empty one to learn the message
/// (`palw_tir_class_registration_message_v1`), then again with it.
#[allow(clippy::too_many_arguments)]
pub fn palw_tir_post_genesis_registration_v1(
    class: PalwTirClassV1,
    canonical: PalwJobContextV2,
    artifact_root: Hash64,
    share_permille: u16,
    initial_target: u128,
    slash_value_per_pwu: u64,
    activation_daa: u64,
    registrant_bond: crate::palw_state_v2::PalwBondKeyV2,
    signature: Vec<u8>,
    ladder: u64,
) -> Result<PalwConsensusObjectV2, PalwClassAdmissionError> {
    let class_id = class.class_id(&artifact_root);
    let space = PalwTirStepSpaceV1::new(&class).map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    let counted = space.leaf_count_capped(&canonical, ladder).map_err(|e| PalwClassAdmissionError::TirLayout(e.to_string()))?;
    Ok(PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille,
        activation_daa,
        admission: Box::new(crate::palw_tir_class_v1::PalwTirAdmissionCarriageV1 { class, canonical, registrant_bond, signature }),
    })
}

/// **The gate the acceptance path runs, as a node can ask it before signing** — the IR twin of
/// the SDK's processor-same preflight. `Err(TirNeedsItsFence)` where `palw_tir_v1` is not in force
/// at `daa_score`; otherwise [`verify_class_admission_v10`] under the rules at that height, with
/// the RC's committed certified families and `chain_certified` (the chain's own). The processor's
/// remaining checks read chain state this function does not hold: the registrant bond's signature
/// and collateral, the chain's target, and the share the certification decides.
pub fn palw_tir_registration_preflight_at_v1(
    params: &crate::config::params::Params,
    bundle: &PalwConsensusParamsV2,
    object: &PalwConsensusObjectV2,
    daa_score: u64,
    chain_certified: &[crate::palw_e2e_adjudicability::PalwE2eFamilyV1],
) -> Result<(PalwClassCatalogEntryV2, PalwTirClassRecordV1), PalwClassAdmissionError> {
    let rules = PalwTirAdmissionRulesV1::at(params, daa_score).ok_or(PalwClassAdmissionError::TirNeedsItsFence)?;
    let certified = crate::palw_e2e_adjudicability::palw_rc_certified_families_v1();
    verify_class_admission_v10(bundle, &rules, object, &certified, chain_certified)
}
