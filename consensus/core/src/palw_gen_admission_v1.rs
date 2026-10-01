//! **RFC-0003 activation step 4: the pipeline admission — the gate a generative class registration
//! passes** (`ClassRegisteredGenV1`, tag 68), the registration builder, and the preflight a node asks
//! before it signs. The generative twin of Phase F's admission v10
//! ([`crate::palw_tir_admission_v1::verify_class_admission_v10`]), asked of a pipeline, cheapest
//! refusal first:
//!
//! 1. the class's structure under the fence's ceilings for its profile — versions, the strict
//!    decoding of every program and the pipeline (NF-P1…P10), the layouts, the output header, the
//!    offers against the bindings, the IR's admission of the pipeline and, for a text class, each
//!    image slot's price floor ([`palw_gen_class_preflight_v1`]); and the weights' pipeline
//!    inventory ([`crate::palw_gen_artifact_v1`]) — every param byte a court reads has a leaf of the
//!    class's `artifact_root` to be proven under;
//! 2. **the one step tree's leaf count, exactly** ([`PalwGenStepSpaceV1::leaf_count_v1`], in closed
//!    form): the WIDEST job — every stage at its `max_trip`, a text stage's logits leaves at every
//!    position — within the court's ladder and the profile's `max_job_step_leaves` (a bound every
//!    offered job is under, so none is deeper than the ladder); and the YARDSTICK job — the class's
//!    most expensive offered job: the largest offered step count, the longest prompt and negative
//!    prompt, every job scalar at its interval's low end, a text stage's stream filled to its
//!    `max_trip` ([`palw_gen_yardstick_v1`]) — whose count `pwu_per_inference` must equal (nothing a
//!    registrant says about it is believed; no canonical job rides);
//! 3. **every commit point's cone of every stage, at its own tile length** (the IR's admission once
//!    per distinct tile length of a stage, at most [`PALW_GEN_MAX_DISTINCT_TILES_V1`]), against the
//!    court: the tile's multiply-accumulates within the terminal ceiling; the tile's evaluation plus
//!    the worst `Fixed`-state replay the stage's checkpoint interval admits within the IR court's
//!    work limits (`palw_court_v2::palw_tir_court_limits_v1`, which the generative court adjudicates
//!    under); the opened bytes and the close frame within the ruleset's close ceiling and the chunks
//!    the fold assembles (the class referenced by id, never carried). **A cone that reduces over the
//!    history is dissected** under the k-ary court — RFC-0002 F7 composed into the generative court:
//!    F7's obligations (O-1…O-5), its value bound, its round and root-claim sizing and its window
//!    asked of the stage's view verbatim, the terminal one `h_tile` chunk — and must fit the court
//!    whole where the court is not armed (`GenNeedsDissection`). Everything is sized under the
//!    block's box-demand rules (`Params::palw_tir_demand_rules_at`): below `palw_tir_fence2` the
//!    DAA-2,000 release's, under which a dissected cone with a `TopK` is refused by name (its row
//!    understates a tile that straddles TopK rows); past it ref2's H7 row, under which the value bound
//!    `V` covers such a tile and the cone is admitted like any other;
//! 4. the class id is `tir_pipeline_class_id_v1(class, artifact_root)`;
//! 5. **weight: none.** No attempt lane exists for pipelines (Phase F decision 12's reading for
//!    generative classes: `palw_gen_v1` opens registration, panels and the court), so a generative
//!    class registers at 0‰ — permissionless (ADR-0069 D5) — and anything else is refused by name
//!    (`GenEarnsNoWeight`).
//!
//! The close-bytes check of item 3 is a NECESSARY condition, as v10's is: the IR's admission reports
//! the operand bytes a worst tile demands at element granularity, and a close carries whole leaves
//! with their paths (the close proof's wire type, item 3 of the wiring, sets the sufficient bound).

use std::collections::BTreeSet;

use crate::Hash64;
use crate::palw_class_admission_v2::{PalwClassAdmissionError, PalwCourtCostV1};
use crate::palw_gen_class_v1::{
    PalwGenAdmissionCarriageV1, PalwGenClassRecordV1, PalwGenClassReportV1, PalwGenClassV1, palw_gen_class_preflight_v1,
    palw_gen_class_record_v1,
};
use crate::palw_gen_step_v1::PalwGenStepSpaceV1;
use crate::palw_gen_v1::PalwGenFenceV1;
use crate::palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2};
use crate::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwPwuRuleV2};
use crate::palw_tir_admission_v1::{PALW_TIR_CLOSE_FRAME_BYTES_V1, palw_tir_carriable_close_bytes_v1, palw_tir_prim_kernel_id_v1};
use misaka_palw_tir::admit::{LeafV1, TirAdmitError, TirAdmitInputsV1, TirCeilingsV1};
use misaka_palw_tir::admit_v2::{TirJobCeilingsV1, TirPipelineAdmissionV1, tir_admit_pipeline_staged_with_rules_v1};
use misaka_palw_tir::pipeline::{PipelineJob, TirPipelineV1, TripRule, stage_job_facts};
use misaka_palw_tir::program_v2::TirProgramV2;

/// The most distinct commit tile lengths one stage's layout may use (v10's bound, per stage).
pub const PALW_GEN_MAX_DISTINCT_TILES_V1: usize = crate::palw_tir_admission_v1::PALW_TIR_MAX_DISTINCT_TILES_V1;

/// **What the pipeline admission reads from the ruleset at the registration's block.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwGenAdmissionRulesV1 {
    /// `Params::palw_gen_v1`'s value (in force at the block).
    pub fence: PalwGenFenceV1,
    /// **The k-ary court at the block**, resolved as Phase F's rules resolve it
    /// ([`crate::palw_tir_admission_v1::PalwTirAdmissionRulesV1::at`]): a cone that reduces over the
    /// history is dissected (RFC-0002 F7, spec 04b §9.5) only under it, and must fit the court whole
    /// without it.
    pub court: Option<crate::palw_class_admission_v2::PalwKaryCourtV1>,
    /// `Params::palw_held_context` at the block: which clock the dissection's window is read on.
    pub held_armed: bool,
    /// **`Params::palw_held_close_chunks_v1` at the block** (decision 22): with it a close up to the carried cap
    /// (PALW-TIR-38) is convictable under the held regime; without it a held-regime chain convicts in one move
    /// only, and PALW-GEN-21 bounds a class's closes by one carrier (less the accusation's framing).
    pub held_close_chunks: bool,
    /// **The box-demand rules at the block** (spec 04b §10.3): the release's, or ref2's H7 `TopK` row
    /// past `Params::palw_tir_fence2` — read by the stages' admission and the value bound `V`.
    pub demand: crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,
}

impl PalwGenAdmissionRulesV1 {
    /// The rules at `daa_score`, or `None` where `palw_gen_v1` is not in force (a generative
    /// registration is then dropped by name, as an older build skips it).
    pub fn at(params: &crate::config::params::Params, daa_score: u64) -> Option<Self> {
        let fence = params.palw_gen_v1_fence().filter(|f| f.activation.is_active(daa_score))?;
        let tir = crate::palw_tir_admission_v1::PalwTirAdmissionRulesV1::at(params, daa_score);
        Some(Self {
            fence,
            court: tir.and_then(|r| r.court),
            held_armed: tir.is_some_and(|r| r.held.armed),
            held_close_chunks: params.palw_held_close_chunks_active_at(daa_score),
            demand: params.palw_tir_demand_rules_at(daa_score),
        })
    }
}

/// **What admission derived**: the catalog entry, the `gen_classes` row the fold writes, and the
/// preflight's report.
#[derive(Clone, Debug)]
pub struct PalwGenAdmittedV1 {
    pub entry: PalwClassCatalogEntryV2,
    pub record: PalwGenClassRecordV1,
    pub report: PalwGenClassReportV1,
}

fn preflight_error(e: crate::palw_gen_class_v1::PalwGenClassErrorV1) -> PalwClassAdmissionError {
    PalwClassAdmissionError::GenClass(e.to_string())
}

fn exceeds(what: &'static str, got: u64, ceiling: u64) -> Result<(), PalwClassAdmissionError> {
    if got > ceiling { Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling }) } else { Ok(()) }
}

/// **The class's yardstick job** — its most expensive offered job (see the module doc): each stage's
/// trip count (as `stage_job_facts` derives it) and the text stage's prompt length.
pub fn palw_gen_yardstick_v1(
    class: &PalwGenClassV1,
    pipeline: &TirPipelineV1,
    programs: &[TirProgramV2],
) -> Result<(Vec<u32>, u32), PalwClassAdmissionError> {
    let offers = &class.offers;
    let prompt = offers.max_prompt_tokens;
    let generated = match pipeline.stages.iter().find(|st| matches!(st.trip, TripRule::TextStream)) {
        // `T = |prompt| + |generated| − 1`, filled to `max_trip` (the offers put the prompt inside it).
        Some(text) => (text.max_trip as u64 + 1).saturating_sub(prompt as u64) as usize,
        None => 0,
    };
    let job = PipelineJob {
        prompt: vec![0; prompt as usize],
        negative: vec![0; offers.max_negative_tokens as usize],
        steps: offers.steps.last().copied().unwrap_or(0),
        scalars: offers.scalars.iter().map(|s| s.lo).collect(),
        images: Vec::new(),
        generated: vec![0; generated],
        source: vec![0; offers.max_source_tokens as usize],
        ..PipelineJob::default()
    };
    let facts = stage_job_facts(pipeline, programs, &job)
        .map_err(|e| PalwClassAdmissionError::GenClass(format!("the class's most expensive offered job does not run: {e}")))?;
    Ok((facts.iter().map(|f| f.trip).collect(), prompt))
}

/// The IR's admission of the pipeline with stage `s` at `tiles[s]` — the court's reading of every
/// commit point at that length. Job totals are not asked here (the step tree's exact count is).
fn admit_at_tiles(
    class: &PalwGenClassV1,
    ceilings: &crate::palw_gen_v1::PalwGenProfileCeilingsV1,
    tiles: &[u32],
    demand: crate::palw_tir_fence2_v1::PalwTirDemandRulesV1,
) -> Result<TirPipelineAdmissionV1, PalwClassAdmissionError> {
    let stage_inputs: Vec<TirAdmitInputsV1> = class
        .layouts
        .iter()
        .zip(tiles)
        .map(|(l, t)| TirAdmitInputsV1 {
            tile_len: *t,
            h_chunk: l.h_tile,
            ceilings: TirCeilingsV1 {
                max_tile_macs: u64::MAX,
                max_tile_transcendentals: u64::MAX,
                max_tile_opened_bytes: u64::MAX,
                max_tile_operands: u64::MAX,
                max_position_macs: ceilings.max_position_macs,
                max_position_transcendentals: u64::MAX,
                max_state_bytes: ceilings.max_state_bytes,
                max_step_leaves: u64::MAX,
                max_checkpoint_interval: l.checkpoint_interval,
                max_cone_work: 1 << 20,
            },
        })
        .collect();
    let open = TirJobCeilingsV1 {
        max_job_macs: u64::MAX,
        max_job_transcendentals: u64::MAX,
        max_job_step_leaves: u64::MAX,
        max_job_cone_work: ceilings.max_job_cone_work,
    };
    tir_admit_pipeline_staged_with_rules_v1(&class.pipeline, &class.programs, &stage_inputs, &open, demand).map_err(|e| match e {
        TirAdmitError::Program(e) => PalwClassAdmissionError::GenClass(e.to_string()),
        TirAdmitError::Exceeds { limit, at, value, cap } => PalwClassAdmissionError::TirExceeds { limit, at, value, cap },
        TirAdmitError::Inputs(why) => PalwClassAdmissionError::GenClass(why.into()),
    })
}

/// **The primitives a class's programs reach**, as kernel ids (`palw-tir/v1/prim=<Name>`: version 2
/// adds no primitive, PALW-TIR-41).
pub fn palw_gen_reachable_prims_v1(programs: &[TirProgramV2]) -> BTreeSet<Hash64> {
    programs
        .iter()
        .flat_map(|p| p.blocks.iter().flat_map(|b| b.nodes.iter()))
        .map(|n| palw_tir_prim_kernel_id_v1(n.prim.name()))
        .collect()
}

/// **The pipeline admission: may this generative class registration join?** Returns what the fold
/// writes (see the module doc for the order and what each item asks).
pub fn verify_gen_class_admission_v1(
    bundle: &PalwConsensusParamsV2,
    rules: &PalwGenAdmissionRulesV1,
    registration: &PalwConsensusObjectV2,
) -> Result<PalwGenAdmittedV1, PalwClassAdmissionError> {
    let PalwConsensusObjectV2::ClassRegisteredGenV1 { class_id, artifact_root, pwu_rule, share_permille, admission, .. } =
        registration
    else {
        return Err(PalwClassAdmissionError::NotARegistration);
    };
    let class = &admission.class;

    // 1. The structure, under the fence's ceilings for the class's profile.
    let report = palw_gen_class_preflight_v1(class, &rules.fence).map_err(preflight_error)?;
    let ceilings = rules.fence.ceilings.of(report.profile);
    let (programs, pipeline) = (&report.admission.programs, &report.admission.pipeline);
    // The weights have an inventory (a closed form of the declarations): every param byte a court
    // reads has a leaf of the class's `artifact_root` to be proven under.
    crate::palw_gen_artifact_v1::PalwGenInventoryIndexV1::new(programs).ok_or_else(|| {
        PalwClassAdmissionError::GenClass(
            "the class's params have no pipeline inventory (no param, a tensor past 4 GiB, or past a u32 of leaves)".into(),
        )
    })?;

    // 2. The step tree: the widest job within the ladder and the profile; the yardstick's count.
    let ladder = bundle.court.max_step_leaf_count();
    let widest: Vec<u32> = pipeline.stages.iter().map(|st| st.max_trip).collect();
    let worst = PalwGenStepSpaceV1::leaf_count_v1(pipeline, programs, &class.layouts, &widest, Some(0), 1)
        .map_err(|e| PalwClassAdmissionError::GenClass(e.to_string()))?;
    let worst = u64::try_from(worst).unwrap_or(u64::MAX);
    if worst > ladder {
        return Err(PalwClassAdmissionError::DeeperThanTheLadder { worst, ladder });
    }
    if worst > ceilings.max_job_step_leaves {
        return Err(PalwClassAdmissionError::TirExceeds {
            limit: "max_job_step_leaves",
            at: "the widest job".into(),
            value: worst,
            cap: ceilings.max_job_step_leaves,
        });
    }
    let (trips, prompt_len) = palw_gen_yardstick_v1(class, pipeline, programs)?;
    let counted = PalwGenStepSpaceV1::leaf_count_v1(pipeline, programs, &class.layouts, &trips, None, prompt_len)
        .map_err(|e| PalwClassAdmissionError::GenClass(e.to_string()))?;
    let counted = u64::try_from(counted).unwrap_or(u64::MAX);
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

    // 3. Every commit point's cone, at its own tile length, against the court.
    let mut distinct: Vec<Vec<u32>> = Vec::with_capacity(class.layouts.len());
    for (s, layout) in class.layouts.iter().enumerate() {
        let set: BTreeSet<u32> = layout.commit_tiles.iter().copied().collect();
        if set.len() > PALW_GEN_MAX_DISTINCT_TILES_V1 {
            return Err(PalwClassAdmissionError::GenClass(format!(
                "stage {s} uses {} distinct commit tile lengths; at most {PALW_GEN_MAX_DISTINCT_TILES_V1}",
                set.len()
            )));
        }
        distinct.push(set.into_iter().collect());
    }
    let runs_needed = distinct.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let mut runs = Vec::with_capacity(runs_needed);
    for r in 0..runs_needed {
        let tiles: Vec<u32> = distinct.iter().map(|d| *d.get(r).or(d.last()).unwrap_or(&1)).collect();
        runs.push(admit_at_tiles(class, ceilings, &tiles, rules.demand)?);
    }
    let limits = crate::palw_court_v2::palw_tir_court_limits_v1(&bundle.court);
    let work_limit = limits.max_elements.min(limits.max_terms);
    let carried = palw_tir_carriable_close_bytes_v1(&bundle.court).min(bundle.court.max_close_bytes());
    // **PALW-GEN-21** (RFC-0003 §I.4.7, decision 22): on a chain that plays no bisection a lie is convicted in one
    // move unless the held leaf challenge is in force, and a one-move accusation rides ONE carrier — so a close past
    // one carrier less the accusation's framing could never convict and the class is refused. With the fence the
    // carried cap is the bound (PALW-TIR-38), as it is where bisection is played.
    let carriable = if rules.held_armed && !rules.held_close_chunks {
        carried.min(crate::palw_gen_one_move_v1::palw_gen_one_move_max_proof_bytes_v1())
    } else {
        carried
    };
    let (mut worst_close, mut worst_macs, mut worst_operands) = (0u64, 0u64, 0u64);
    for (s, st) in pipeline.stages.iter().enumerate() {
        let program = &programs[st.program as usize];
        let layout = &class.layouts[s];
        let checkpoint = layout.checkpoint_interval as u64;
        let mut commit_tiles = layout.commit_tiles.iter();
        for (bi, block) in program.blocks.iter().enumerate() {
            for (ni, node) in block.nodes.iter().enumerate() {
                if !node.commit {
                    continue;
                }
                let tile_len = *commit_tiles.next().expect("the preflight checked one tile per commit point");
                let r = distinct[s].iter().position(|t| *t == tile_len).expect("every tile length is a distinct one");
                let view = &runs[r].stages[s].admission.view;
                let cone = view
                    .cones
                    .iter()
                    .find(|c| c.block as usize == bi && c.node as usize == ni)
                    .expect("the IR's admission costs every commit point");
                // **A cone that reduces over the history is DISSECTED under the k-ary court** — RFC-0002
                // F7 composed into the generative court: F7's obligations and sizing asked of the
                // stage's view verbatim ([`palw_tir_dissected_cone_admits_parts_v1`]), the terminal one
                // `h_tile` chunk. Without the court such a cone must fit whole.
                let dissected = !cone.h_reductions.is_empty() && rules.court.is_some();
                if !cone.h_reductions.is_empty() {
                    match rules.court {
                        None => {
                            let tile = &cone.tile;
                            let whole = tile.macs.saturating_add(tile.elementwise).saturating_add(tile.transcendentals);
                            if tile.macs > bundle.court.max_terminal_macs() || whole > work_limit {
                                return Err(PalwClassAdmissionError::GenNeedsDissection {
                                    stage: s as u8,
                                    block: bi as u8,
                                    node: ni as u16,
                                });
                            }
                        }
                        Some(k) => {
                            let gen_refused = |why: String| PalwClassAdmissionError::GenDissection {
                                stage: s as u8,
                                block: bi as u8,
                                node: ni as u16,
                                why,
                            };
                            // ref2's H7: below `palw_tir_fence2` §10.3's TopK row undercounts a tile
                            // that straddles TopK rows, so `V` can fall below the claim an honest
                            // responder must post — a dissected cone with a TopK is refused there,
                            // never admitted under a bound it can exceed. Past it the row is H7's
                            // and `V` covers the tile.
                            let vblock = &view.program.blocks[bi];
                            if rules.demand == crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::Release2000
                                && cone
                                    .nodes
                                    .iter()
                                    .any(|n| matches!(vblock.nodes[*n as usize].prim, misaka_palw_tir::Prim::TopK { .. }))
                            {
                                return Err(gen_refused(
                                    "a TopK in a dissected cone: below palw_tir_fence2 the value bound undercounts its tiles (ref2's H7)"
                                        .into(),
                                ));
                            }
                            crate::palw_tir_admission_v1::palw_tir_dissected_cone_admits_parts_v1(
                                bundle,
                                rules.held_armed,
                                rules.demand,
                                &view.program,
                                vblock,
                                bi as u8,
                                ni as u16,
                                tile_len,
                                layout.max_context as u64,
                                layout.h_tile,
                                k,
                            )
                            .map_err(|e| match e {
                                PalwClassAdmissionError::TirDissection { why, .. } => gen_refused(why),
                                other => other,
                            })?;
                            // The root claim rides one carrier: the close frame, the chunk's opened
                            // evidence, the claimed values and the move's frame (a NECESSARY
                            // condition; the finalize's carriage is recorded exactly when built).
                            let values = crate::palw_tir_dissect_v1::palw_tir_dissect_value_bound_v2(
                                &view.program,
                                vblock,
                                ni as u16,
                                tile_len,
                                rules.demand,
                            );
                            let root = PALW_TIR_CLOSE_FRAME_BYTES_V1
                                .saturating_add(cone.terminal_opened_bytes())
                                .saturating_add(values.saturating_mul(20))
                                .saturating_add(crate::palw_tir_admission_v1::PALW_TIR_DISSECT_MOVE_FRAME_BYTES_V1);
                            if root > crate::palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1 {
                                return Err(PalwClassAdmissionError::TirExceeds {
                                    limit: "generative dissection root claim bytes",
                                    at: format!("stage {s} block {bi} node {ni}"),
                                    value: root,
                                    cap: crate::palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1,
                                });
                            }
                        }
                    }
                }
                let (tile, opened) =
                    if dissected { (cone.terminal(), cone.terminal_opened_bytes()) } else { (&cone.tile, cone.tile_opened_bytes) };
                exceeds("generative tile multiply-accumulates", tile.macs, bundle.court.max_terminal_macs())?;
                let mut work = tile.macs.saturating_add(tile.elementwise).saturating_add(tile.transcendentals);
                for leaf in &cone.leaves {
                    if let LeafV1::State(j) = leaf
                        && let Some(state) = view.states.iter().find(|x| x.state == *j)
                    {
                        let per = state.per_position;
                        let per = per.macs.saturating_add(per.elementwise).saturating_add(per.transcendentals);
                        work = work.saturating_add(checkpoint.saturating_sub(1).saturating_mul(per));
                    }
                }
                exceeds("generative cone evaluation work (tile and state replay)", work, work_limit)?;
                let close = PALW_TIR_CLOSE_FRAME_BYTES_V1.saturating_add(opened);
                exceeds("generative close bytes as carried", close, carriable)?;
                worst_close = worst_close.max(close);
                worst_macs = worst_macs.max(tile.macs);
                worst_operands = worst_operands.max(cone.operands);
            }
        }
    }

    // 4. The id.
    let derived = class.class_id(artifact_root);
    if *class_id != derived {
        return Err(PalwClassAdmissionError::GenClassIdIsNotDerived { declared: *class_id, derived });
    }

    // 5. Weight: none (see the module doc).
    if *share_permille != 0 {
        return Err(PalwClassAdmissionError::GenEarnsNoWeight { share: *share_permille });
    }

    let record = palw_gen_class_record_v1(class, artifact_root).map_err(preflight_error)?;
    let entry = PalwClassCatalogEntryV2 {
        class_id: derived,
        artifact_root: *artifact_root,
        max_step_leaf_count: worst,
        canonical_step_leaf_count: counted,
        reachable_kernels: palw_gen_reachable_prims_v1(programs),
        court_cost: PalwCourtCostV1 {
            max_close_bytes: worst_close,
            max_terminal_macs: worst_macs,
            max_operand_count: u32::try_from(worst_operands).unwrap_or(u32::MAX),
        },
    };
    Ok(PalwGenAdmittedV1 { entry, record, report })
}

/// **A generative class registration for a running chain** — the twin of
/// `palw_tir_post_genesis_registration_v1`. The class id is derived from the class and the artifact
/// root; `pwu_per_inference` is the yardstick job's leaf count, so the object and the gate's recount
/// are one count. The signature is the caller's: build once with an empty one to learn the message
/// (`palw_gen_class_registration_message_v1`), then again with it.
#[allow(clippy::too_many_arguments)]
pub fn palw_gen_post_genesis_registration_v1(
    class: PalwGenClassV1,
    artifact_root: Hash64,
    share_permille: u16,
    initial_target: u128,
    slash_value_per_pwu: u64,
    activation_daa: u64,
    registrant_bond: PalwBondKeyV2,
    signature: Vec<u8>,
) -> Result<PalwConsensusObjectV2, PalwClassAdmissionError> {
    let (programs, pipeline) = class.decode().map_err(|e| PalwClassAdmissionError::GenClass(e.to_string()))?;
    let (trips, prompt_len) = palw_gen_yardstick_v1(&class, &pipeline, &programs)?;
    let counted = PalwGenStepSpaceV1::leaf_count_v1(&pipeline, &programs, &class.layouts, &trips, None, prompt_len)
        .map_err(|e| PalwClassAdmissionError::GenClass(e.to_string()))?;
    Ok(PalwConsensusObjectV2::ClassRegisteredGenV1 {
        class_id: class.class_id(&artifact_root),
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: u64::try_from(counted).unwrap_or(u64::MAX) },
        initial_target,
        share_permille,
        activation_daa,
        admission: Box::new(PalwGenAdmissionCarriageV1 { class, registrant_bond, signature }),
    })
}

/// **The gate the acceptance path runs, as a node can ask it before signing.** `Err(GenNeedsItsFence)`
/// where `palw_gen_v1` is not in force at `daa_score`; otherwise [`verify_gen_class_admission_v1`]
/// under the rules at that height. The processor's remaining checks read chain state this function
/// does not hold: the registrant bond's signature and collateral, and the chain's target.
pub fn palw_gen_registration_preflight_at_v1(
    params: &crate::config::params::Params,
    bundle: &PalwConsensusParamsV2,
    object: &PalwConsensusObjectV2,
    daa_score: u64,
) -> Result<PalwGenAdmittedV1, PalwClassAdmissionError> {
    let rules = PalwGenAdmissionRulesV1::at(params, daa_score).ok_or(PalwClassAdmissionError::GenNeedsItsFence)?;
    verify_gen_class_admission_v1(bundle, &rules, object)
}
