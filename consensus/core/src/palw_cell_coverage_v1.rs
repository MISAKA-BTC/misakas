//! **R6-CELL boundary coverage checker** (RFC-0006 Spec closures 2026-10-15, round 3; PALW-SH-10/11/13). Pure and dormant: no
//! consensus path calls it, it arms nothing and it records no PASS.
//!
//! It ENUMERATES every boundary a cell profile the code accepts can have, and maps each to the localizing verifier that covers it:
//!
//! * **TIR classes** (`palw_tir_shard_v1`, cells verified by `verify_cell_v1`): every node's relation by its constraint family
//!   (the class descriptor's checker), every routing node (TopK/Gather/Compare/Select/ReduceMax, the Selection family), every
//!   block's carry-out → next occurrence's carry-in (the shard cut is one of them), every `Fixed`/`Hist` state, the shard plan's
//!   `(S_L, S_P)` shape, and the decode/logits door.
//! * **Legacy graph classes** (`PalwShapeProfileV3`): every layer by its derived kind, every layer-to-layer carry, the MoE router
//!   of every layer table, and each memory section (attention cache, recurrence state) under the registered state chunk map.
//!
//! A boundary with no verifier is a [`PalwCellCoverageGapV1::Uncovered`] gap; a profile kind the checker does not know (a profile
//! version, map id, shard shape, or a state/layer kind) is [`PalwCellCoverageGapV1::UnknownProfileKind`] — fail closed. Matches
//! over the enumerated enums are exhaustive, so a new variant cannot compile until it is classified here.
//!
//! What "covered" means is stated, not proved, here: the verifier named is the one the code routes that boundary to. The argument
//! that those verifiers jointly cover a claim (and its open parts) is RFC-0006 §R6-CELL round 3.
//!
//! **Round 4 (2026-10-15): every gap is classified** ([`PalwCellUncoveredKindV1`]), and past the dormant fence
//! `palw_cell_coverage_closure_v1` ([`crate::palw_cell_coverage_closure_v1`]) the legacy checker is consensus-read: a class whose
//! legacy coverage has ANY gap is refused at registration, its claims are refused at the claim gate where the fold holds its graph,
//! and the step court convicts where it does not — so every legacy class past the fence is either covered boundary by boundary or
//! explicitly uncovered and unrewarded. Below the fence nothing calls it.

use misaka_palw_kernel::descriptor::KernelDescriptorV1;
use misaka_palw_kernel::family::{CheckerIdV1, ConstraintFamilyV1, family_of_prim};
use misaka_palw_tir::program::{StateKind, TirProgramV1};

use crate::Hash64;
use crate::palw_state_chunk_map as maps;
use crate::palw_step::{
    PALW_STEP_OBJECT_VERSION_V1, PALW_STEP_OBJECT_VERSION_V2, PALW_STEP_OBJECT_VERSION_V3, PalwLayerKindV1, PalwShapeProfileV3,
    kernel_semantics_id_v1,
};
use crate::palw_tir_shard_v1::{
    PALW_TIR_SHARD_MAX_SHARDS_V1, PALW_TIR_SHARD_POSITION_SEGMENTS_LAYERS_ONLY_V1, PALW_TIR_SHARD_POSITION_SEGMENTS_S1_V1,
};

/// A memory section of a legacy state chunk map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCellMemorySectionV1 {
    /// The attention KV cache (a history: `p` rows at position `p`).
    AttentionCache,
    /// The gated-delta recurrence (and its convolution window): a state.
    RecurrenceState,
}

/// One boundary a cell profile can have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwCellBoundaryV1 {
    /// A TIR node's relation (PALW-SH-10).
    TirRelation {
        block: u8,
        node: u16,
        family: ConstraintFamilyV1,
    },
    /// A TIR routing/selection node (MoE TopK, expert Gather, masks).
    TirRouting {
        block: u8,
        node: u16,
    },
    /// A block's carry-out slot read as the next occurrence's carry-in (every layer edge, hence every shard cut; PALW-SH-11).
    TirCarry {
        block: u8,
        slot: u16,
    },
    TirFixedState {
        state: u16,
        per_layer: bool,
    },
    TirHistState {
        state: u16,
        per_layer: bool,
        window: u32,
    },
    /// The declared layer/position partition.
    TirShardPlan {
        s_l: u16,
        s_p: u16,
    },
    /// The logits rows and the delivered token.
    TirDecodeDoor,
    LegacyLayer {
        layer: u16,
        kind: PalwLayerKindV1,
    },
    /// Layer `layer`'s input is layer `layer − 1`'s committed output.
    LegacyLayerCarry {
        layer: u16,
    },
    /// The one MoE router of a layer table (q36 router TopK).
    LegacyRouter {
        layer: u16,
    },
    LegacyMemory {
        map: Hash64,
        section: PalwCellMemorySectionV1,
    },
}

/// The localizing verifier a boundary is routed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCellVerifierV1 {
    /// `verify_cell_v1` (`misaka/palw/tir/exec/src/node/cell.rs`): the cell recomputed from the previous occurrence's committed
    /// carry-out tiles; a lie at a boundary faults the upstream cell and is filed as `TirShardCourtAccused`.
    TirCellRecompute,
    /// The class descriptor's checker for the family (exact recompute, Freivalds, state continuity, edge recompute).
    KernelChecker(CheckerIdV1),
    /// State/HistTile leaves recomputed by the cell verifier plus the descriptor's `StateContinuity`.
    TirStateLeaves,
    /// `palw_tir_shard_plan_shape_v1`'s accepted shapes and contiguous layer partition.
    TirShardPlanShape,
    /// The `TirLogits` / `TirDecodeToken(Tiled)` court arms.
    TirLogitsDoor,
    /// The legacy step court for that layer kind (GDN anchored replay / attention KV anchor and dissection).
    LegacyStepCourt(PalwLayerKindV1),
    /// The q36 router TopK recomputation (exactly one router per table).
    LegacyRouterTopk,
    /// `palw_state_chunk_count_at_v1` enumerates the section: a non-canonical checkpoint index is decidable.
    LegacyStateMapEnumerated,
    /// No map registered: the genesis-anchored long form replays the section.
    LegacyLongForm,
}

/// **Which kind of uncovered boundary a gap is** (round 4). Each kind is REFUSED AT ADMISSION past `palw_cell_coverage_closure_v1`
/// (none is covered by a new court path in this round: each would need a verifier the court does not have — see the variants).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwCellUncoveredKindV1 {
    /// A TIR node whose constraint family the class descriptor has no checker for.
    TirFamilyUnchecked,
    /// A TIR carry-out that is not a commit point (the next cell's verifier does not read it).
    TirCarryNotCommitted,
    /// A TIR state the class descriptor does not check for continuity.
    TirStateUnchecked,
    /// **A hybrid v1/v2 composition's recurrence half**: the map names a `gdn=` half `palw_state_chunk_count_at_v1` does not
    /// enumerate, so a checkpoint's recurrence chunks are not decidable by the court. Covering it needs the recurrence geometry in
    /// the consensus crate and an anchored GDN arm reading it — the v3/v4/v5 compositions are that coverage, so a class wanting
    /// it registers one of them.
    HybridRecurrenceUnenumerated,
    /// **A standalone (solo) recurrence map** (`gdn_state_chunk_map_id_v1/v2/v3`): its geometry is the executor crate's
    /// (`base0_gdn_state_geometry_v*`), so the court cannot count, open or anchor it at all.
    SoloRecurrenceMap,
    /// A known map that does not name a memory section the profile needs (an attention-only map under GDN layers, or the
    /// reverse).
    MapSectionNotSpelled,
    /// A known map whose count the code does not derive at a checkpoint of this profile.
    MapNotEnumerable,
    /// **More than one q36 router in a layer table**: the router slot is not unique (`qwen36_router_topk_slot` answers `None`), so a
    /// routed product's selection is unadjudicable — a court choosing between routers would be choosing the evidence.
    MultiRouterTable,
    /// **A Kimi router** (`KDESC_KIMI_ROUTER_TOPK`, a fenced kernel): its own row recomputes (`kimi_row`), but the routed
    /// gather/combine downstream resolves its routing only through the q36 router slot, so a routed product under it is
    /// unadjudicable.
    KimiRouter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwCellCoverageGapV1 {
    Uncovered { boundary: PalwCellBoundaryV1, kind: PalwCellUncoveredKindV1, why: &'static str },
    UnknownProfileKind { why: String },
}

pub type PalwCellCoverageV1 = Vec<(PalwCellBoundaryV1, PalwCellVerifierV1)>;

/// **What round 4 does with an uncovered kind** (RFC-0006 R6-CELL round 4 matrix).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCellDispositionV1 {
    /// Covered by a court path through an existing one-move verifier (none in round 4; a later round that builds one moves its
    /// kind here and out of the closure's refusal).
    Covered(PalwCellVerifierV1),
    /// Refused at admission past `palw_cell_coverage_closure_v1` — registration (REQ-CELL-1), the claim gate (REQ-CELL-1b) — and
    /// convicted by the court where admission cannot see the graph (REQ-CELL-2): explicitly uncovered and unrewarded.
    RefusedAtAdmission,
}

/// **Round 4's disposition of every uncovered kind.** Exhaustive, so a new kind cannot compile until it is placed. Every kind is
/// refused at admission: the hybrid v1/v2 recurrence half needs the recurrence geometry in this crate and an anchored GDN arm
/// reading it (the v3/v4/v5 compositions are that coverage); a solo map's geometry is the executor's; a multi-router or Kimi-routed
/// table resolves no unique router slot for a routed product; the IR kinds are their descriptor's (outside the legacy closure).
pub fn palw_cell_uncovered_disposition_v1(kind: PalwCellUncoveredKindV1) -> PalwCellDispositionV1 {
    match kind {
        PalwCellUncoveredKindV1::HybridRecurrenceUnenumerated
        | PalwCellUncoveredKindV1::SoloRecurrenceMap
        | PalwCellUncoveredKindV1::MapSectionNotSpelled
        | PalwCellUncoveredKindV1::MapNotEnumerable
        | PalwCellUncoveredKindV1::MultiRouterTable
        | PalwCellUncoveredKindV1::KimiRouter
        | PalwCellUncoveredKindV1::TirFamilyUnchecked
        | PalwCellUncoveredKindV1::TirCarryNotCommitted
        | PalwCellUncoveredKindV1::TirStateUnchecked => PalwCellDispositionV1::RefusedAtAdmission,
    }
}

fn finish(covered: PalwCellCoverageV1, gaps: Vec<PalwCellCoverageGapV1>) -> Result<PalwCellCoverageV1, Vec<PalwCellCoverageGapV1>> {
    if gaps.is_empty() { Ok(covered) } else { Err(gaps) }
}

/// **TIR class coverage**: every boundary of `program` under `descriptor`, with the declared shard shape `(S_L, S_P)` if sharded.
pub fn palw_tir_cell_coverage_v1(
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    shard_plan: Option<(u16, u16)>,
) -> Result<PalwCellCoverageV1, Vec<PalwCellCoverageGapV1>> {
    let mut covered = Vec::new();
    let mut gaps = Vec::new();
    for (b, block) in program.blocks.iter().enumerate() {
        let Ok(bi) = u8::try_from(b) else {
            gaps.push(PalwCellCoverageGapV1::UnknownProfileKind { why: format!("block {b} past the u8 block index") });
            continue;
        };
        for (n, node) in block.nodes.iter().enumerate() {
            let ni = n as u16;
            let family = family_of_prim(&node.prim);
            let boundary = PalwCellBoundaryV1::TirRelation { block: bi, node: ni, family };
            let Some(support) = descriptor.support(family) else {
                gaps.push(PalwCellCoverageGapV1::Uncovered {
                    boundary,
                    kind: PalwCellUncoveredKindV1::TirFamilyUnchecked,
                    why: "the class descriptor has no checker for this family",
                });
                continue;
            };
            covered.push((boundary, PalwCellVerifierV1::KernelChecker(support.checker)));
            match family {
                ConstraintFamilyV1::Selection => covered.push((
                    PalwCellBoundaryV1::TirRouting { block: bi, node: ni },
                    PalwCellVerifierV1::KernelChecker(support.checker),
                )),
                ConstraintFamilyV1::Structure
                | ConstraintFamilyV1::ExactArithmetic
                | ConstraintFamilyV1::DenseMatrix
                | ConstraintFamilyV1::QuantRange
                | ConstraintFamilyV1::Nonlinear
                | ConstraintFamilyV1::RecurrentState
                | ConstraintFamilyV1::MediaPipeline => {}
            }
        }
        for (slot, node) in block.carry_out.iter().enumerate() {
            let boundary = PalwCellBoundaryV1::TirCarry { block: bi, slot: slot as u16 };
            match block.nodes.get(*node as usize) {
                None => gaps.push(PalwCellCoverageGapV1::UnknownProfileKind {
                    why: format!("block {b} carry-out slot {slot} names node {node}, which does not exist"),
                }),
                // The cell verifier reads committed carry-out tiles: an uncommitted carry is not read, so not covered.
                Some(n) if !n.commit => gaps.push(PalwCellCoverageGapV1::Uncovered {
                    boundary,
                    kind: PalwCellUncoveredKindV1::TirCarryNotCommitted,
                    why: "a carry-out that is not a commit point is not read by the next cell's verifier",
                }),
                Some(_) => covered.push((boundary, PalwCellVerifierV1::TirCellRecompute)),
            }
        }
    }
    let state_support = descriptor.support(ConstraintFamilyV1::RecurrentState).map(|s| s.checker);
    for (j, s) in program.states.iter().enumerate() {
        let state = j as u16;
        let boundary = match s.kind {
            StateKind::Fixed { .. } => PalwCellBoundaryV1::TirFixedState { state, per_layer: s.per_layer },
            StateKind::Hist { window } => PalwCellBoundaryV1::TirHistState { state, per_layer: s.per_layer, window },
        };
        if state_support == Some(CheckerIdV1::StateContinuity) {
            covered.push((boundary, PalwCellVerifierV1::TirStateLeaves));
        } else {
            gaps.push(PalwCellCoverageGapV1::Uncovered {
                boundary,
                kind: PalwCellUncoveredKindV1::TirStateUnchecked,
                why: "the class descriptor does not check state continuity",
            });
        }
    }
    if let Some((s_l, s_p)) = shard_plan {
        let layers = program.schedule.layers.len();
        if !(2..=PALW_TIR_SHARD_MAX_SHARDS_V1).contains(&s_l)
            || (s_p != PALW_TIR_SHARD_POSITION_SEGMENTS_LAYERS_ONLY_V1 && s_p != PALW_TIR_SHARD_POSITION_SEGMENTS_S1_V1)
            || s_l as usize > layers
        {
            gaps.push(PalwCellCoverageGapV1::UnknownProfileKind {
                why: format!("shard shape S_L={s_l}, S_P={s_p} over {layers} layers"),
            });
        } else {
            covered.push((PalwCellBoundaryV1::TirShardPlan { s_l, s_p }, PalwCellVerifierV1::TirShardPlanShape));
        }
    }
    covered.push((PalwCellBoundaryV1::TirDecodeDoor, PalwCellVerifierV1::TirLogitsDoor));
    finish(covered, gaps)
}

/// The hybrid compositions whose `gdn=` half the court does not enumerate (v1, v2).
fn map_is_partial_hybrid_v1(map: &Hash64) -> bool {
    [maps::hybrid_state_chunk_map_id_v1(), maps::hybrid_state_chunk_map_id_v2()].contains(map)
}

/// The standalone recurrence maps (their geometry is the executor crate's).
fn map_is_solo_recurrence_v1(map: &Hash64) -> bool {
    [maps::gdn_state_chunk_map_id_v1(), maps::gdn_state_chunk_map_id_v2(), maps::gdn_state_chunk_map_id_v3()].contains(map)
}

/// Which memory sections a known map makes enumerable (`None`: unknown map — fail closed). The default (no map) is the long form.
fn map_sections_v1(map: &Hash64) -> Option<(bool, bool)> {
    let attention_only = [
        maps::integer_kv_state_chunk_map_id_v1(),
        maps::integer_kv_state_chunk_map_id_v2(),
        maps::tiled_kv_state_chunk_map_id_v3(),
        maps::tiled_kv_state_chunk_map_id_v4(),
        // The v1/v2 hybrid compositions name a `gdn=` half `palw_state_chunk_count_at_v1` does not enumerate.
        maps::hybrid_state_chunk_map_id_v1(),
        maps::hybrid_state_chunk_map_id_v2(),
    ];
    let both = [maps::hybrid_state_chunk_map_id_v3(), maps::hybrid_state_chunk_map_id_v4(), maps::hybrid_state_chunk_map_id_v5()];
    // The standalone recurrence maps' geometry lives in the executor crate: the court cannot enumerate them.
    let neither = [maps::gdn_state_chunk_map_id_v1(), maps::gdn_state_chunk_map_id_v2(), maps::gdn_state_chunk_map_id_v3()];
    if attention_only.contains(map) {
        Some((true, false))
    } else if both.contains(map) {
        Some((true, true))
    } else if neither.contains(map) {
        Some((false, false))
    } else {
        None
    }
}

/// **Legacy graph profile coverage**: layers, carries, routers and memory sections of one accepted `PalwShapeProfileV3`.
pub fn palw_legacy_cell_coverage_v1(profile: &PalwShapeProfileV3) -> Result<PalwCellCoverageV1, Vec<PalwCellCoverageGapV1>> {
    let mut covered = Vec::new();
    let mut gaps = Vec::new();
    if ![PALW_STEP_OBJECT_VERSION_V1, PALW_STEP_OBJECT_VERSION_V2, PALW_STEP_OBJECT_VERSION_V3].contains(&profile.version) {
        return Err(vec![PalwCellCoverageGapV1::UnknownProfileKind { why: format!("shape profile version {}", profile.version) }]);
    }
    let q36_router = kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_ROUTER_TOPK);
    let kimi_router = kernel_semantics_id_v1(crate::palw_step_refute::KDESC_KIMI_ROUTER_TOPK);
    let (mut has_attention, mut has_recurrence) = (false, false);
    for layer in 0..profile.layer_count {
        let kind = profile.layer_kind(layer);
        match kind {
            PalwLayerKindV1::GatedDeltaNet => has_recurrence = true,
            PalwLayerKindV1::Attention => has_attention = true,
        }
        covered.push((PalwCellBoundaryV1::LegacyLayer { layer, kind }, PalwCellVerifierV1::LegacyStepCourt(kind)));
        if layer > 0 {
            covered.push((PalwCellBoundaryV1::LegacyLayerCarry { layer }, PalwCellVerifierV1::LegacyStepCourt(kind)));
        }
        let table = profile.layer_table(layer);
        let routers = table.iter().filter(|n| n.kernel_semantics_id == q36_router).count();
        let boundary = PalwCellBoundaryV1::LegacyRouter { layer };
        if table.iter().any(|n| n.kernel_semantics_id == kimi_router) {
            gaps.push(PalwCellCoverageGapV1::Uncovered {
                boundary: boundary.clone(),
                kind: PalwCellUncoveredKindV1::KimiRouter,
                why: "a fenced Kimi router: its routed products resolve no router slot, so they are unadjudicable",
            });
        }
        match routers {
            0 => {}
            1 => covered.push((boundary, PalwCellVerifierV1::LegacyRouterTopk)),
            _ => gaps.push(PalwCellCoverageGapV1::Uncovered {
                boundary,
                kind: PalwCellUncoveredKindV1::MultiRouterTable,
                why: "more than one router in a table: the router slot is not unique, so routed products are unadjudicable",
            }),
        }
    }
    let map = profile.state_chunk_map_id;
    let needed: Vec<PalwCellMemorySectionV1> =
        [(has_attention, PalwCellMemorySectionV1::AttentionCache), (has_recurrence, PalwCellMemorySectionV1::RecurrenceState)]
            .into_iter()
            .filter_map(|(needed, section)| needed.then_some(section))
            .collect();
    if map == Hash64::default() {
        for section in needed {
            covered.push((PalwCellBoundaryV1::LegacyMemory { map, section }, PalwCellVerifierV1::LegacyLongForm));
        }
    } else if let Some((attention, recurrence)) = map_sections_v1(&map) {
        // The table names what the map spells; the code must also actually enumerate it at a checkpoint.
        let enumerable = [1, profile.n_ctx.max(1)].into_iter().all(|p| maps::palw_state_chunk_count_at_v1(profile, p).is_some());
        for section in needed {
            let spelled = match section {
                PalwCellMemorySectionV1::AttentionCache => attention,
                PalwCellMemorySectionV1::RecurrenceState => recurrence,
            };
            let boundary = PalwCellBoundaryV1::LegacyMemory { map, section };
            if spelled && enumerable {
                covered.push((boundary, PalwCellVerifierV1::LegacyStateMapEnumerated));
            } else {
                // The kind names WHY, most specific first: the two named recurrence defects, then the generic ones.
                let kind = if map_is_solo_recurrence_v1(&map) {
                    PalwCellUncoveredKindV1::SoloRecurrenceMap
                } else if map_is_partial_hybrid_v1(&map) && section == PalwCellMemorySectionV1::RecurrenceState {
                    PalwCellUncoveredKindV1::HybridRecurrenceUnenumerated
                } else if !spelled {
                    PalwCellUncoveredKindV1::MapSectionNotSpelled
                } else {
                    PalwCellUncoveredKindV1::MapNotEnumerable
                };
                gaps.push(PalwCellCoverageGapV1::Uncovered {
                    boundary,
                    kind,
                    why: "the registered map does not let the court enumerate this memory section (the long form is refused for a mapped class)",
                });
            }
        }
    } else {
        gaps.push(PalwCellCoverageGapV1::UnknownProfileKind { why: "a state chunk map id outside every known map".into() });
    }
    finish(covered, gaps)
}

/// The shipped legacy ladder families (graph v1, v5, v7, and the v8 hybrid row) at every ladder row, labelled; a family that does
/// not construct at a row is not an accepted profile there and is skipped.
pub fn palw_shipped_legacy_profiles_v1() -> Vec<(String, PalwShapeProfileV3)> {
    use crate::palw_context_ladder as ladder;
    let families: [(&str, ladder::PalwLadderFamilyV1); 7] = [
        ("a16-v1", ladder::PALW_LADDER_FAMILIES_V1[0]),
        ("qwen36-v1", ladder::PALW_LADDER_FAMILIES_V1[1]),
        ("a16-v5", ladder::PALW_LADDER_FAMILIES_V5[0]),
        ("qwen36-v5", ladder::PALW_LADDER_FAMILIES_V5[1]),
        ("a16-v7", ladder::PALW_LADDER_FAMILIES_V7[0]),
        ("qwen36-v7", ladder::PALW_LADDER_FAMILIES_V7[1]),
        ("qwen36-v8", ladder::palw_qwen36_context_row_profile_v8),
    ];
    let mut out = Vec::new();
    for (name, family) in families {
        for row in ladder::PALW_CONTEXT_LADDER_ROWS {
            if let Ok(profile) = family(row) {
                out.push((format!("{name}@{row}"), profile));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_tir_fixture_is_enumerated_and_every_boundary_is_routed() {
        use misaka_palw_kernel::descriptor::{k2_tir_v2_descriptor, k2_tir_v4_descriptor};
        for program in [
            misaka_palw_tir_sketch::fixture::dense_moe_v1(7).program,
            misaka_palw_tir_sketch::fixture::dense_moe_windowed_v1(7, 4).program,
            misaka_palw_tir_sketch::fixture::wide128_v1(7).program,
        ] {
            for d in [k2_tir_v2_descriptor(), k2_tir_v4_descriptor()] {
                // A one-layer program cannot be sharded (S_L ≥ 2 shards of at least one layer each).
                let shard = (program.schedule.layers.len() >= 2).then_some((2, PALW_TIR_SHARD_POSITION_SEGMENTS_LAYERS_ONLY_V1));
                let covered = palw_tir_cell_coverage_v1(&d, &program, shard)
                    .expect("every TIR boundary of a shipped fixture is routed to a verifier");
                let nodes: usize = program.blocks.iter().map(|b| b.nodes.len()).sum();
                let relations = covered.iter().filter(|(b, _)| matches!(b, PalwCellBoundaryV1::TirRelation { .. })).count();
                assert_eq!(relations, nodes, "PALW-SH-10: one relation per node");
                let carries: usize = program.blocks.iter().map(|b| b.carry_out.len()).sum();
                assert_eq!(covered.iter().filter(|(b, _)| matches!(b, PalwCellBoundaryV1::TirCarry { .. })).count(), carries);
                assert_eq!(
                    covered
                        .iter()
                        .filter(|(b, _)| matches!(
                            b,
                            PalwCellBoundaryV1::TirFixedState { .. } | PalwCellBoundaryV1::TirHistState { .. }
                        ))
                        .count(),
                    program.states.len()
                );
                assert!(covered.iter().any(|(b, _)| *b == PalwCellBoundaryV1::TirDecodeDoor));
            }
        }
        // The MoE fixture's routing is enumerated (TopK/Gather are Selection nodes).
        let moe = misaka_palw_tir_sketch::fixture::dense_moe_v1(7).program;
        let covered = palw_tir_cell_coverage_v1(&k2_tir_v2_descriptor(), &moe, None).unwrap();
        assert!(covered.iter().any(|(b, _)| matches!(b, PalwCellBoundaryV1::TirRouting { .. })));
    }

    #[test]
    fn a_tir_boundary_without_a_verifier_and_an_unknown_shard_shape_fail() {
        let mut program = misaka_palw_tir_sketch::fixture::dense_moe_v1(7).program;
        let mut d = misaka_palw_kernel::descriptor::k2_tir_v2_descriptor();
        // A descriptor without the selection family leaves every routing node uncovered.
        d.families.retain(|f| f.family != ConstraintFamilyV1::Selection);
        let gaps = palw_tir_cell_coverage_v1(&d, &program, None).unwrap_err();
        assert!(gaps.iter().all(|g| matches!(
            g,
            PalwCellCoverageGapV1::Uncovered {
                boundary: PalwCellBoundaryV1::TirRelation { family: ConstraintFamilyV1::Selection, .. },
                ..
            }
        )));
        let d = misaka_palw_kernel::descriptor::k2_tir_v2_descriptor();
        for shape in [(1, 1), (65, 1), (2, 0), (2, 7)] {
            let gaps = palw_tir_cell_coverage_v1(&d, &program, Some(shape)).unwrap_err();
            assert!(matches!(gaps[..], [PalwCellCoverageGapV1::UnknownProfileKind { .. }]), "{shape:?}");
        }
        // An uncommitted carry-out is a boundary the next cell does not read.
        let (b, n) = program.blocks.iter().enumerate().find_map(|(b, blk)| blk.carry_out.first().map(|n| (b, *n))).unwrap();
        program.blocks[b].nodes[n as usize].commit = false;
        let gaps = palw_tir_cell_coverage_v1(&d, &program, None).unwrap_err();
        assert!(
            gaps.iter().any(|g| matches!(g, PalwCellCoverageGapV1::Uncovered { boundary: PalwCellBoundaryV1::TirCarry { .. }, .. }))
        );
        // A carry-out naming no node is an unknown profile, never skipped.
        program.blocks[b].carry_out[0] = u16::MAX;
        let gaps = palw_tir_cell_coverage_v1(&d, &program, None).unwrap_err();
        assert!(gaps.iter().any(|g| matches!(g, PalwCellCoverageGapV1::UnknownProfileKind { .. })));
    }

    #[test]
    fn shipped_legacy_profiles_are_enumerated_and_their_gaps_are_named_never_unknown() {
        let profiles = palw_shipped_legacy_profiles_v1();
        assert!(!profiles.is_empty(), "at least one shipped ladder family constructs");
        for (name, profile) in &profiles {
            match palw_legacy_cell_coverage_v1(profile) {
                Ok(covered) => {
                    let layers = covered.iter().filter(|(b, _)| matches!(b, PalwCellBoundaryV1::LegacyLayer { .. })).count();
                    assert_eq!(layers, profile.layer_count as usize, "{name}");
                }
                // A shipped profile may have an UNCOVERED section (recorded, not a PASS); it never has an unknown kind.
                Err(gaps) => assert!(gaps.iter().all(|g| matches!(g, PalwCellCoverageGapV1::Uncovered { .. })), "{name}: {gaps:?}"),
            }
            // The v1/v2 hybrid composition does not enumerate its recurrence half: a hybrid profile under it has that gap.
            let partial = [maps::hybrid_state_chunk_map_id_v1(), maps::hybrid_state_chunk_map_id_v2()];
            let hybrid = (0..profile.layer_count).any(|l| profile.layer_kind(l) == PalwLayerKindV1::GatedDeltaNet);
            if hybrid && partial.contains(&profile.state_chunk_map_id) {
                let gaps = palw_legacy_cell_coverage_v1(profile).unwrap_err();
                assert!(gaps.iter().any(|g| matches!(
                    g,
                    PalwCellCoverageGapV1::Uncovered {
                        boundary: PalwCellBoundaryV1::LegacyMemory { section: PalwCellMemorySectionV1::RecurrenceState, .. },
                        ..
                    }
                )));
            }
        }
    }

    #[test]
    fn unknown_legacy_kinds_fail_closed() {
        let Some((_, base)) = palw_shipped_legacy_profiles_v1().into_iter().next() else { return };
        let mut unknown_version = base.clone();
        unknown_version.version = 99;
        assert!(matches!(
            palw_legacy_cell_coverage_v1(&unknown_version).unwrap_err()[..],
            [PalwCellCoverageGapV1::UnknownProfileKind { .. }]
        ));
        let mut unknown_map = base.clone();
        unknown_map.state_chunk_map_id = kernel_semantics_id_v1("not/a/state/chunk/map");
        assert!(
            palw_legacy_cell_coverage_v1(&unknown_map)
                .unwrap_err()
                .iter()
                .any(|g| matches!(g, PalwCellCoverageGapV1::UnknownProfileKind { .. }))
        );
        // A standalone recurrence map the court cannot enumerate leaves every needed section uncovered.
        let mut standalone = base;
        standalone.state_chunk_map_id = maps::gdn_state_chunk_map_id_v2();
        assert!(
            palw_legacy_cell_coverage_v1(&standalone)
                .unwrap_err()
                .iter()
                .any(|g| matches!(g, PalwCellCoverageGapV1::Uncovered { boundary: PalwCellBoundaryV1::LegacyMemory { .. }, .. }))
        );
    }

    /// **Round 4: the coverage matrix, enumerated** — every map id the code knows, an unknown one, and the router kinds, on a shipped
    /// hybrid. Each case is COVERED (`Ok`) or names its uncovered kind, and every uncovered kind's disposition is refused at
    /// admission (`palw_cell_coverage_closure_v1`). Unverified source (2026-10-15): written, not run.
    #[test]
    fn round4_every_uncovered_kind_is_enumerated_with_its_disposition() {
        use crate::palw_step_refute::{KDESC_KIMI_ROUTER_TOPK, KDESC_Q36_ROUTER_TOPK};
        use PalwCellUncoveredKindV1 as K;
        let (_, mut base) = palw_shipped_legacy_profiles_v1()
            .into_iter()
            .find(|(_, p)| {
                (0..p.layer_count).any(|l| p.layer_kind(l) == PalwLayerKindV1::Attention)
                    && (0..p.layer_count).any(|l| p.layer_kind(l) == PalwLayerKindV1::GatedDeltaNet)
            })
            .expect("a shipped hybrid ladder family constructs");
        base.state_chunk_map_id = Hash64::default();
        // COVERED: no map — the long form replays both sections; one router per table.
        assert!(palw_legacy_cell_coverage_v1(&base).is_ok(), "a hybrid on no map is covered end to end");
        let kinds_of = |p: &PalwShapeProfileV3| -> Vec<K> {
            match palw_legacy_cell_coverage_v1(p) {
                Ok(_) => Vec::new(),
                Err(gaps) => gaps
                    .iter()
                    .filter_map(|g| match g {
                        PalwCellCoverageGapV1::Uncovered { kind, .. } => Some(*kind),
                        PalwCellCoverageGapV1::UnknownProfileKind { .. } => None,
                    })
                    .collect(),
            }
        };
        let unknown = |p: &PalwShapeProfileV3| {
            palw_legacy_cell_coverage_v1(p)
                .is_err_and(|gaps| gaps.iter().any(|g| matches!(g, PalwCellCoverageGapV1::UnknownProfileKind { .. })))
        };
        let on = |map: Hash64| {
            let mut p = base.clone();
            p.state_chunk_map_id = map;
            p
        };
        // Every known map is KNOWN (never an unknown kind); each refused one names its kind.
        let known = [
            maps::integer_kv_state_chunk_map_id_v1(),
            maps::integer_kv_state_chunk_map_id_v2(),
            maps::tiled_kv_state_chunk_map_id_v3(),
            maps::tiled_kv_state_chunk_map_id_v4(),
            maps::hybrid_state_chunk_map_id_v1(),
            maps::hybrid_state_chunk_map_id_v2(),
            maps::hybrid_state_chunk_map_id_v3(),
            maps::hybrid_state_chunk_map_id_v4(),
            maps::hybrid_state_chunk_map_id_v5(),
            maps::gdn_state_chunk_map_id_v1(),
            maps::gdn_state_chunk_map_id_v2(),
            maps::gdn_state_chunk_map_id_v3(),
        ];
        for map in known {
            assert!(!unknown(&on(map)), "a known map is never an unknown kind");
        }
        for map in [maps::hybrid_state_chunk_map_id_v1(), maps::hybrid_state_chunk_map_id_v2()] {
            assert!(kinds_of(&on(map)).contains(&K::HybridRecurrenceUnenumerated), "the v1/v2 hybrid's recurrence half");
        }
        for map in [maps::gdn_state_chunk_map_id_v1(), maps::gdn_state_chunk_map_id_v2(), maps::gdn_state_chunk_map_id_v3()] {
            assert!(kinds_of(&on(map)).iter().all(|k| *k == K::SoloRecurrenceMap), "a solo recurrence map, every needed section");
            assert!(!kinds_of(&on(map)).is_empty());
        }
        for map in [maps::integer_kv_state_chunk_map_id_v1(), maps::tiled_kv_state_chunk_map_id_v3()] {
            assert!(kinds_of(&on(map)).contains(&K::MapSectionNotSpelled), "an attention-only map under GDN layers");
        }
        assert!(unknown(&on(kernel_semantics_id_v1("not/a/state/chunk/map"))), "an unknown map: fail closed");
        // Routers: two q36 routers in one table, and a Kimi router.
        let with = |kernel: &str, extra: usize| {
            let mut p = base.clone();
            for _ in 0..extra {
                let mut node = p.attn_nodes[0].clone();
                node.kernel_semantics_id = kernel_semantics_id_v1(kernel);
                p.attn_nodes.push(node);
            }
            p
        };
        assert!(kinds_of(&with(KDESC_Q36_ROUTER_TOPK, 2)).contains(&K::MultiRouterTable));
        assert!(kinds_of(&with(KDESC_KIMI_ROUTER_TOPK, 1)).contains(&K::KimiRouter));
        // Every kind's disposition: refused at admission (none newly covered in round 4).
        for kind in [
            K::HybridRecurrenceUnenumerated,
            K::SoloRecurrenceMap,
            K::MapSectionNotSpelled,
            K::MapNotEnumerable,
            K::MultiRouterTable,
            K::KimiRouter,
            K::TirFamilyUnchecked,
            K::TirCarryNotCommitted,
            K::TirStateUnchecked,
        ] {
            assert_eq!(palw_cell_uncovered_disposition_v1(kind), PalwCellDispositionV1::RefusedAtAdmission, "{kind:?}");
        }
    }
}
