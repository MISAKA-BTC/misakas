//! **The TIR public frame's operations, measured on the corpus classes** (feature `node`; G14 audit §10,
//! `docs/design/palw/g14-completeness-proof-2026-10-16.md`; records in `docs/design/palw/evidence/tir-frame-measure-2026-10-16/`).
//!
//! Each tiny tiled IR class is written as a PALWTIR1 container and served by [`TirBackendV1`] — the node's production backend — and
//! each frame operation is run through the same API the node's services call:
//!
//! * **execute** — the producer's free-prompt run (one replay pass's work);
//! * **answer (cold)** — the original capture reader answers every step leaf, every trace tile and the IDs, slice by slice
//!   (`slice_quote_v1` / `poll_quoted_v1`), and the owner authenticates each answer against the original roots;
//! * **preparation** — the prepared generation is published slice by slice (`begin_prepared_v1`), then reopened and answers again
//!   (warm);
//! * **per-step check** — for every leaf, the owner's replayed cone refutation and the court's check of it
//!   (`check_tir_cone_refutation_v1`, the shipped court);
//! * **filing** — a forged leaf, refuted from a challenger's store and convicted by the court; the witness size is recorded;
//! * **frame bytes** — the frame's full-frame encode + hash at the custodian uppers 6 and 257 (what `original_controls` charges
//!   32 × 32 steps per byte for on every accepting action).
//!
//! The derived allocation's bounds (testnet-12's staged config, upper 6) are checked against what actually ran: the published
//! generation fits `prepared_disk_bytes`, its slices fit the per-pass slice bound, and every answer fits its unit bound.
//!
//! **What this is and is not.** The counts and byte sizes are deterministic. The times are wall-clock nanoseconds of THIS host and
//! THIS build profile; they are inputs to the measured-value record, never a value pinned here. Arming evidence must come from a
//! release build on the slowest supported node (the release plan's court principle), with real genesis-size classes — the corpus
//! classes are tiny. Set `MISAKA_TIR_FRAME_MEASURE_OUT=<file>` to write the canonical record (one JSON document, keys sorted) and
//! print its BLAKE2b-256 digest, the form the arm-readiness record's V5 evidence takes.

#![cfg(feature = "node")]

mod node_common;

use std::sync::Arc;
use std::time::Instant;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::{PalwExecutionBackendV1, PalwOriginalCaptureReaderV1};
use kaspa_consensus_core::palw_fp_execution_v3::{palw_fp_commitment_from_context_v3, palw_fp_job_pin_v1};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step_leg::{step_merkle_root_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirCourtRulesV1, check_tir_cone_refutation_v1};
use kaspa_consensus_core::palw_tir_public_allocation_v1::{
    PalwTirPublicAllocationMissingV1, PalwTirPublicAllocationQuoteV1, PalwTirPublicAllocationRulesV1, derive_tir_public_allocation_v1,
};
use kaspa_consensus_core::palw_tir_public_source_v1::{PalwTirPublicRequestV1, PalwTirPublicSourceV1};
use kaspa_consensus_core::palw_tir_step_v1::{PalwTirStepSpaceV1, palw_tir_execution_root_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir::{DType, MapParams, TirProgramV1};
use misaka_palw_tir_exec::node::{
    TirArtifactV1, TirBackendV1, TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1, TirStepTreeV1,
};
use misaka_palw_tir_exec::{TirParams, TirPlan};
use node_common::{job, layout, programs, tiled};
use serde_json::{Value, json};

const NET: &[u8] = b"testnet-12";
const LADDER: u64 = 1 << 26;
const PART: u64 = 16_384;

/// **The candidate testnet-12 frame cost** — read from the recorded measurement's candidate
/// (`docs/design/palw/evidence/tir-frame-measure-2026-10-16/candidate-t12-cost.json`): every value there names its source (this
/// harness, the fold-level measurement, testnet-12's shipped parameters, or a network input still to be observed). A candidate, not
/// a pinned record: `PALW_TIR_FRAME_COST_RECORDS_V1` stays empty.
fn candidate() -> kaspa_consensus_core::palw_tir_frame_cost_v1::PalwTirFrameCostV1 {
    use kaspa_consensus_core::palw_rendering_resources_v1::{PalwRenderingResourcePricesV1, PalwStagedPublicCourtConfigV1};
    use kaspa_consensus_core::palw_tir_frame_cost_v1::{PALW_TIR_FRAME_CLOCK_VERSION_V1, PalwTirFrameClockV1, PalwTirFrameCostV1};
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../docs/design/palw/evidence/tir-frame-measure-2026-10-16/candidate-t12-cost.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("the candidate cost record")).unwrap();
    let u = |section: &str, key: &str| v[section][key]["value"].as_u64().unwrap_or_else(|| panic!("{section}.{key}"));
    let cost = PalwTirFrameCostV1 {
        config: PalwStagedPublicCourtConfigV1 {
            prices: PalwRenderingResourcePricesV1 {
                inclusion_margin_daa: u("config", "inclusion_margin_daa"),
                rent_per_stream_byte: u("config", "rent_per_stream_byte"),
                rent_per_parser_step: u("config", "rent_per_parser_step"),
                rent_per_row_byte: u("config", "rent_per_row_byte"),
                deposit_per_move: u("config", "deposit_per_move"),
                reorg_replay_moves: u("config", "reorg_replay_moves"),
            },
            acquisition_daa: u("config", "acquisition_daa"),
            collection_daa: u("config", "collection_daa"),
            reorg_retention_daa: u("config", "reorg_retention_daa"),
            maximum_owned_rows_per_reserver: u("config", "maximum_owned_rows_per_reserver") as u32,
        },
        clock: PalwTirFrameClockV1 {
            version: PALW_TIR_FRAME_CLOCK_VERSION_V1,
            action_parser_steps: u("clock", "action_parser_steps"),
            action_parser_steps_per_custodian: u("clock", "action_parser_steps_per_custodian"),
            frame_passes_per_check: u("clock", "frame_passes_per_check"),
            control_bytes: u("clock", "control_bytes"),
            parallel_bytes_per_daa: u("clock", "parallel_bytes_per_daa"),
            // The harness MEASURES the door, so it does not refuse it here: each plan records its door beside testnet-12's
            // liability horizon (`window_challenge_at + window_court`, the most validation admits — external audit 10-17 §7 P0(3)).
            maximum_door_daa: u64::MAX,
        },
    };
    cost.check_v1().expect("a well-formed candidate");
    cost
}

/// The planned fence height the candidate prices (16,000 is the release plan's target, not fixed).
const PLANNED_FENCE_DAA: u64 = 16_000;
/// testnet-12's liability horizon at the planned fence: `window_challenge_at(fence) + window_court`.
fn t12_horizon_daa() -> u64 {
    let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { unreachable!() };
    bundle.state.window_challenge_at(PLANNED_FENCE_DAA) + bundle.state.window_court()
}

/// **testnet-12's shipped court, the same one `palw-class declare-layout` reads** (`court_params_v1` in the CLI):
/// `max_close_bytes` is the held set's widest close over the ladder, not the default. The earlier fixed
/// `PalwCourtParamsV2::new(1 << 22, 45, 2)` priced the native witness at the default 2,250,000 B and so
/// refused a class the real court would carry (10-18 finding).
fn t12_court() -> PalwCourtParamsV2 {
    let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { unreachable!() };
    bundle.court
}

/// The frame's native rules at testnet-12's ladder, at a custodian upper of `custodians`, with `cost`.
fn rules_with(custodians: u64, cost: kaspa_consensus_core::palw_tir_frame_cost_v1::PalwTirFrameCostV1) -> PalwTirPublicAllocationRulesV1 {
    PalwTirPublicAllocationRulesV1 {
        maximum_leaves: 1 << 22,
        maximum_custodians: custodians,
        court: t12_court(),
        dissection: true,
        prompt_form: PalwPromptIdsFormV1::Flat,
        config: cost.config,
        clock: cost.clock,
    }
}
fn rules(custodians: u64) -> PalwTirPublicAllocationRulesV1 {
    rules_with(custodians, candidate())
}

/// A user free-prompt job of `backend`'s class over `prompt`, at `decode` tokens, greedy (the free-prompt suite's job).
fn fp_job(backend: &TirBackendV1, prompt: &[u32], decode: u32) -> PalwFreePromptJobV3 {
    PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: Hash64::from_u64_word(0x4E7),
        class_id: backend.class_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0),
        executor_pubkey: vec![7; 32],
        operator_id: Hash64::from_u64_word(0xE0),
        anchor_block: Hash64::from_u64_word(0xA9),
        anchor_daa: 1_000,
        job_nonce: [3; 32],
        tokenizer_id: backend.class().tokenizer_id,
        prompt_token_ids_hash: prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::Flat, prompt).unwrap(),
        prompt_tokens: prompt.len() as u32,
        decode_token_limit: decode,
        max_context_tokens: 64,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    }
}

/// The original capture, held in memory and read in the node's bounded reads.
struct Original(Vec<u8>);
impl PalwOriginalCaptureReaderV1 for Original {
    fn len_v1(&self) -> u64 {
        self.0.len() as u64
    }
    fn read_v1(&self, at: u64, length: u32) -> Result<Vec<u8>, String> {
        self.0.get(at as usize..at as usize + length as usize).map(|v| v.to_vec()).ok_or_else(|| "short original".into())
    }
}

/// Nanoseconds since `start`.
fn ns(start: Instant) -> u64 {
    start.elapsed().as_nanos() as u64
}

/// Answers `request` slice by slice; returns (the response, slices, total ns, largest slice ns).
fn answer(
    reader: &mut dyn kaspa_consensus_core::palw_backend::PalwTirPublicCaptureReaderV1,
    request: &PalwTirPublicRequestV1,
) -> (kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicResponseV1, u64, u64, u64) {
    let (mut slices, mut total, mut largest) = (0u64, 0u64, 0u64);
    for _ in 0..10_000_000 {
        let start = Instant::now();
        let quote = reader.slice_quote_v1(request).unwrap();
        let polled = reader.poll_quoted_v1(request, &quote).unwrap();
        let took = ns(start);
        slices += 1;
        total += took;
        largest = largest.max(took);
        if let Some(response) = polled {
            return (response, slices, total, largest);
        }
    }
    panic!("a bounded answer did not finish");
}

/// One class's court fixture under the free-prompt suite's layout (seed 5, C 2, h 2): its runner, its honest retention, the
/// held inventory and its dense preimages.
struct Court {
    space: PalwTirStepSpaceV1,
    plan: TirPlan,
    class: PalwTirClassV1,
    artifact_root: Hash64,
    ctx: kaspa_consensus_core::palw_v2::PalwJobContextV2,
    prompt: Vec<u32>,
}
fn court(program: &TirProgramV1, params: &MapParams) -> Court {
    struct Map<'m>(&'m MapParams);
    impl PalwTirTensorSourceV1 for Map<'_> {
        fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<std::borrow::Cow<'_, [u8]>> {
            self.0.tensors.get(&(param, layer)).map(|t| std::borrow::Cow::Owned(t.to_le_bytes()))
        }
    }
    let plan = TirPlan::compile(program).unwrap();
    let (artifact_root, _) = palw_tir_inventory_root_v1(program, &Map(params)).unwrap();
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: layout(program, 5, 2, 2, 64),
        tokenizer_id: Hash64::from_bytes([3; 64]),
    };
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let prompt: Vec<u32> = (0..4).map(|i| (i * 5 + 3) % program.token_bound).collect();
    let ctx = job(program, 4, 2, class.class_id(&artifact_root), &prompt, PalwPromptIdsFormV1::Flat);
    Court { space, plan, class, artifact_root, ctx, prompt }
}

/// One component's rent / moves / inclusion clock.
fn component(c: &kaspa_consensus_core::palw_rendering_resources_v1::PalwRenderingResourcePlanV1) -> Value {
    json!({ "rent": c.rent_sompi, "parser": c.maximum_parser_steps, "moves": c.maximum_moves, "inclusion_daa": c.inclusion_daa })
}

/// The plan's prices and clocks (relative to `accepted_daa`) and each component's share — one row of the before/after table.
fn bounds(plan: &kaspa_consensus_core::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1) -> Value {
    let base = plan.terms.accepted_daa;
    let owner = plan.owner_terms_v1().unwrap();
    // testnet-12's challenge window and court window (the shipped bundle's state params at the planned fence): a framed claim's
    // Final floor is max(licence + window_challenge, the frame's collect door, and — past the liability horizon fence — licence +
    // window_challenge + window_court, `palw_legacy_fp_liability_floor_v1`). Licensed no earlier than accepted, so these are lower
    // bounds relative to acceptance.
    let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { unreachable!() };
    let wc = bundle.state.window_challenge_at(PLANNED_FENCE_DAA);
    let court = bundle.state.window_court();
    let collect = plan.terms.collect_until_daa - base;
    json!({
        "door_daa": collect,
        "liability_horizon_daa": t12_horizon_daa(),
        "door_fits_horizon": collect <= t12_horizon_daa(),
        "preparation_door_daa": plan.preparation_until_daa - base,
        "stream_door_daa": plan.terms.stream_until_daa - plan.terms.open_until_daa,
        "acquisition_inclusion_daa": plan.acquisition.envelope.inclusion_daa,
        "window_challenge": wc,
        "window_court": court,
        "final_floor": collect.max(wc),
        "final_floor_with_liability_hold": collect.max(wc + court),
        "upper": plan.acquisition.maximum_custodians,
        "claim_rent_sompi": plan.terms.plan.rent_sompi,
        "owner_open_rent_sompi": owner.plan.rent_sompi,
        "owner_open_deposit_sompi": owner.plan.deposit_sompi,
        "preparation_until": plan.preparation_until_daa - base,
        "open_until": plan.terms.open_until_daa - base,
        "stream_until": plan.terms.stream_until_daa - base,
        "collect_until": plan.terms.collect_until_daa - base,
        "frame_bytes": plan.control_footprint.maximum_frame_bytes,
        "frame_writes": plan.control_footprint.maximum_frame_writes,
        "owner_turns": plan.acquisition.maximum_turns,
        "preparation_slices_per_actor": plan.preparation.maximum_slices_per_actor,
        "owner_lane": format!("{:?}", plan.owner_work.per_actor),
        "custodian_lane": format!("{:?}", plan.owner_work.per_custodian),
        "components": {
            "acquisition": component(&plan.acquisition.envelope),
            "preparation_work": component(&plan.preparation_work.envelope),
            "owner_work": component(&plan.owner_work.envelope),
            "preparation": component(&plan.preparation.envelope),
            "executor": component(&plan.executor.envelope),
            "terminal": component(&plan.terminal.envelope),
            "controls": component(&plan.controls),
            "owner_validation": component(&plan.owner_validation),
        },
    })
}

/// The measurements of one class (deterministic counts and sizes, and this host's nanoseconds).
fn measure(name: &str, backend: &TirBackendV1, program: &TirProgramV1, params: Option<&MapParams>, k: usize) -> Value {
    // --- execute: the producer's free-prompt run --------------------------------------------------------------------------
    let (_, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C ^ k as u8; 64])).unwrap();
    let prompt: Vec<u32> = prompt.into_iter().map(|t| t as u32).collect();
    let job = fp_job(backend, &prompt, 2);
    let ids: Vec<usize> = prompt.iter().map(|t| *t as usize).collect();
    let start = Instant::now();
    let run = backend.execute_free_prompt(&job, &ids).unwrap();
    let execute_ns = ns(start);
    let material = run.outcome.material.clone();
    let shape = backend.capture_shape(&material).unwrap();
    let commitment = palw_fp_commitment_from_context_v3(&job, &shape.job_context, &run, 999_999).unwrap();
    let PalwPublicExecutionBindingHeaderV1::Tir(header) = backend.public_execution_binding_header_v1(&material).unwrap() else {
        panic!("{name}: an IR capture exports a TIR header")
    };
    let (record, _) =
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(backend.class(), &backend.artifact_root()).unwrap();
    let source = PalwTirPublicSourceV1::new_v1(
        &record,
        &backend.class().layout,
        &header,
        commitment.execution_root,
        commitment.trace_root,
        LADDER,
    )
    .unwrap();
    let n = source.leaf_count_v1();
    let d = source.decode_count_v1();
    let vocab = source.vocab_v1();
    let tiles = u64::from(vocab).div_ceil(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES as u64) as u32;

    // --- the derived bounds at testnet-12's staged config, upper 6 and 257 ----------------------------------------------------
    let geometry = kaspa_consensus_core::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
        class_id: job.class_id,
        producer: PalwBondKeyV2(job.executor_bond),
        execution_root: commitment.execution_root,
        job_identity: palw_fp_job_pin_v1(&commitment),
        accepted_daa: 100,
    };
    let quote = |upper: u64| match derive_tir_public_allocation_v1(
        &geometry,
        &commitment,
        &header,
        &record,
        &backend.class().layout,
        None,
        None,
        rules(upper),
    ) {
        Ok(PalwTirPublicAllocationQuoteV1::Complete(plan)) => Ok(plan),
        other => Err(format!("{other:?}")),
    };
    let geometry_json = json!({ "leaves": n, "decode": d, "vocab": vocab, "tiles": tiles, "prefill": header.job_context.declared_prefill_tokens });
    let (plan6, plan257) = match (quote(6), quote(257)) {
        (Ok(a), Ok(b)) => (a, b),
        (a, b) if params.is_none() => {
            // A real class whose frame the candidate cost does not complete: the refusal IS the measurement.
            return json!({
                "class": name,
                "geometry": geometry_json,
                "execute": { "ns": execute_ns, "capture_bytes": material.len() },
                "frame_upper6": a.err().unwrap_or_else(|| "complete".into()),
                "frame_upper257": b.err().unwrap_or_else(|| "complete".into()),
            });
        }
        (a, b) => panic!("{name}: no complete frame: {:?} / {:?}", a.err(), b.err()),
    };
    let passes = plan6.preparation.maximum_restarts + 1;
    let unit = plan6.acquisition.unit_bytes_v1(&source).unwrap();

    // --- answer (cold) and the owner's authentication --------------------------------------------------------------------------
    let scratch = tempfile::tempdir().unwrap();
    let original = Arc::new(Original(material.clone()));
    let start = Instant::now();
    let mut reader = backend.open_tir_public_capture_v1(&header, original.clone(), scratch.path()).unwrap();
    let cold_open_ns = ns(start);
    let mut requests: Vec<PalwTirPublicRequestV1> = (0..n).map(|index| PalwTirPublicRequestV1::StepLeaf { index }).collect();
    requests.push(PalwTirPublicRequestV1::Ids { offset: 0 });
    for row in 0..d {
        for tile in 0..tiles {
            requests.push(PalwTirPublicRequestV1::TraceTile { row, tile });
        }
    }
    let (mut cold_slices, mut cold_ns, mut cold_slice_max, mut answer_bytes_max, mut auth_ns, mut auth_max) = (0, 0, 0, 0u64, 0, 0);
    let mut ids = None;
    let mut assembler = kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicIdsAssemblerV1::default();
    for request in &requests {
        let (response, slices, took, largest) = answer(reader.as_mut(), request);
        cold_slices += slices;
        cold_ns += took;
        cold_slice_max = cold_slice_max.max(largest);
        let bytes = borsh::to_vec(&response).unwrap().len() as u64;
        assert!(bytes <= unit, "{name}: an answer ({bytes} bytes) fits the frame's unit bound ({unit})");
        answer_bytes_max = answer_bytes_max.max(bytes);
        let start = Instant::now();
        if matches!(request, PalwTirPublicRequestV1::Ids { .. }) {
            // IDs authenticate as an ordered stream against the original trace root (the owner's assembler).
            if let Some(authenticated) = assembler.push_v1(&source, request, &response).unwrap() {
                ids = Some(authenticated);
            }
        } else {
            source.authenticate_v1(request, &response, ids.as_ref()).unwrap();
        }
        let took = ns(start);
        auth_ns += took;
        auth_max = auth_max.max(took);
    }

    // --- §11 (a): a full step run answered cold fits the per-class window bound (the job's actual leaves, not tile × 4) ---------
    let run_count = (n.min(u64::from(kaspa_consensus_core::palw_tir_public_source_v1::PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1))) as u32;
    let mut run_answer_max = 0u64;
    let mut run_bound = 0u64;
    if run_count > 1 {
        for first in [0, n - u64::from(run_count)] {
            let request = PalwTirPublicRequestV1::StepRun { first, count: run_count };
            let (response, _, _, _) = answer(reader.as_mut(), &request);
            source.authenticate_v1(&request, &response, ids.as_ref()).unwrap();
            let bytes = borsh::to_vec(&response).unwrap().len() as u64;
            let (bound, _) = kaspa_consensus_core::palw_tir_public_owned_v1::request_body_bound_v1(&source, request).unwrap();
            assert!(bytes <= bound, "{name}: a {run_count}-leaf run ({bytes} B) fits its per-class bound ({bound} B)");
            run_answer_max = run_answer_max.max(bytes);
            run_bound = run_bound.max(bound);
        }
    }

    // --- preparation: publish the prepared generation, reopen it, answer warm ---------------------------------------------------
    let durable = tempfile::tempdir().unwrap();
    let need = reader.prepared_need_v1().unwrap();
    let start = Instant::now();
    let mut writer = reader.begin_prepared_v1(durable.path(), 100, 10).unwrap();
    let (mut publish_slices, mut publish_slice_max) = (0u64, 0u64);
    let descriptor = loop {
        let slice_start = Instant::now();
        let quote = writer.slice_quote_v1().unwrap();
        let polled = writer.poll_quoted_v1(&quote).unwrap();
        publish_slice_max = publish_slice_max.max(ns(slice_start));
        publish_slices += 1;
        assert!(publish_slices <= need.maximum_publish_slices, "{name}: publication within its quoted slices");
        if let Some(descriptor) = polled {
            break descriptor;
        }
    };
    let publish_ns = ns(start);
    let publish_io = writer.progress_v1().charged.io_bytes;
    drop(writer);
    drop(reader);
    // The allocation's bounds hold for what ran: the generation fits `prepared_disk_bytes`; a pass is ONE debit (audit §10), its
    // publication's node slices and their IO ride inside the preparer's lane.
    assert!(descriptor.retained_bytes <= plan6.executor.prepared_disk_bytes, "{name}: the generation fits the executor's bound");
    assert!(descriptor.retained_bytes <= plan6.owner_work.prepared_bytes, "{name}: and the work allocation's");
    let per_pass_slices = plan6.preparation.maximum_slices_per_actor / passes;
    assert_eq!(per_pass_slices, 1, "{name}: one debit per pass");
    assert!(publish_io <= plan6.preparation_work.per_actor.io_bytes, "{name}: the publication's IO within the preparer's lane");
    // --- one custodian preparation PASS exactly as the node runs it (audit §10: ONE debit per pass) -------------------------------
    // A fresh cold reader answers `StepLeaf { index: 0 }` until warm, then the prepared writer publishes; every slice's quoted work
    // and the fixed per-slice overhead are charged to one ledger, which must fit the pass's budget (the preparer's lane / passes).
    let pass = {
        use kaspa_consensus_core::palw_tir_frame_cost_v1::{
            PALW_TIR_PUBLIC_SLICE_OVERHEAD_V1, tir_public_pass_budget_v1, tir_public_slice_work_v1,
        };
        let budget = tir_public_pass_budget_v1(plan6.preparation_work.per_actor, passes).unwrap();
        let mut spent = kaspa_consensus_core::palw_tir_public_owned_v1::PalwTirPublicWorkV1::default();
        let mut charge = |quote: &kaspa_consensus_core::palw_backend::PalwTirPublicCaptureSliceV1| {
            spent = spent.checked_add_v1(PALW_TIR_PUBLIC_SLICE_OVERHEAD_V1).unwrap().checked_add_v1(tir_public_slice_work_v1(quote).unwrap()).unwrap();
        };
        let scratch = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let mut reader = backend.open_tir_public_capture_v1(&header, original.clone(), scratch.path()).unwrap();
        let request = PalwTirPublicRequestV1::StepLeaf { index: 0 };
        let mut capture_slices = 0u64;
        loop {
            let quote = reader.slice_quote_v1(&request).unwrap();
            charge(&quote);
            capture_slices += 1;
            if reader.poll_quoted_v1(&request, &quote).unwrap().is_some() {
                break;
            }
        }
        let out = tempfile::tempdir().unwrap();
        let mut writer = reader.begin_prepared_v1(out.path(), 100, 10).unwrap();
        let mut publication_slices = 0u64;
        loop {
            let quote = writer.slice_quote_v1().unwrap();
            charge(&quote);
            publication_slices += 1;
            if writer.poll_quoted_v1(&quote).unwrap().is_some() {
                break;
            }
        }
        let pass_ns = ns(start);
        let mut fitted = spent;
        fitted.attempts = budget.attempts;
        assert!(budget.covers_v1(&fitted), "{name}: one preparation pass ({spent:?}) fits its one debit's budget ({budget:?})");
        json!({
            "capture_slices": capture_slices, "publication_slices": publication_slices, "pass_ns": pass_ns,
            "pass_io_bytes": spent.io_bytes, "budget_io_bytes": budget.io_bytes,
            "pass_hash_bytes": spent.hash_bytes, "budget_hash_bytes": budget.hash_bytes,
            "pass_parser_steps": spent.parser_steps, "budget_parser_steps": budget.parser_steps,
            "debits_per_pass": 1, "passes": passes,
        })
    };
    let start = Instant::now();
    let mut warm = backend.reopen_tir_prepared_capture_v1(&header, &descriptor, durable.path(), 100, 11).unwrap();
    let warm_open_ns = ns(start);
    let (mut warm_slices, mut warm_ns) = (0u64, 0u64);
    for index in 0..n {
        let request = PalwTirPublicRequestV1::StepLeaf { index };
        let (response, slices, took, _) = answer(warm.as_mut(), &request);
        source.authenticate_v1(&request, &response, ids.as_ref()).unwrap();
        warm_slices += slices;
        warm_ns += took;
    }
    drop(warm);

    // The court half needs the class's tensors in memory (`MapParams`, i128 per element): a fixture class only. A real-model run
    // (`params: None`) records it as unmeasured.
    let (step_check, filing) = match params {
        None => (json!("UNMEASURED: no in-memory params for this class (real-model run)"), json!("UNMEASURED")),
        Some(params) => {
    // --- per-step check: the owner's replayed refutation and the court's check, every leaf --------------------------------------
    let c = court(program, params);
    let tparams = TirParams::from_map(&c.plan, params).unwrap();
    let held_source = TirParamsSourceV1(&tparams);
    let inventory = TirHeldInventoryV1::build(program, &held_source).unwrap();
    let runner = TirClassRunnerV1::new(&c.space, &c.plan, &tparams, c.class.class_id(&c.artifact_root)).unwrap();
    let court_rules = PalwTirCourtRulesV1 { max_step_leaf_count: LADDER, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };
    let start = Instant::now();
    let honest = runner.retain(&c.class, c.artifact_root, &c.ctx, &c.prompt, LADDER).unwrap();
    let retain_ns = ns(start);
    let mut dense = Vec::new();
    let start = Instant::now();
    runner.run(&c.ctx, &c.prompt, LADDER, true, &mut |l| dense.push(l.preimage.clone())).unwrap();
    let replay_pass_ns = ns(start);
    let own = TirEvidenceV1::own(&runner, &honest, &inventory, PalwPromptIdsFormV1::Flat, LADDER).unwrap();
    let court_n = honest.binding.step_leaf_count;
    let (mut build_max, mut build_total, mut check_max, mut check_total, mut refutation_bytes_max) = (0u64, 0u64, 0u64, 0u64, 0u64);
    for i in 0..court_n {
        let start = Instant::now();
        let r = own.cone_refutation(i, &court_rules).unwrap();
        let built = ns(start);
        let start = Instant::now();
        assert!(check_tir_cone_refutation_v1(&r, &court_rules).is_err(), "{name}: an honest leaf is acquitted");
        let checked = ns(start);
        build_max = build_max.max(built);
        build_total += built;
        check_max = check_max.max(checked);
        check_total += checked;
        refutation_bytes_max = refutation_bytes_max.max(borsh::to_vec(&r).unwrap().len() as u64);
    }

    // --- filing: a forged leaf, refuted from a challenger's store and convicted --------------------------------------------------
    let intervals = analyze_ranges(&c.space.program).unwrap();
    let mut filing = Value::Null;
    for i in (0..court_n).rev() {
        let leaf = c.space.leaf_at(&c.ctx, i).unwrap();
        let iv = kaspa_consensus_core::palw_tir_court_v1::palw_tir_leaf_interval_v1(&c.space, &intervals, &leaf).unwrap();
        let lane = leaf.value_count as usize / 2;
        let bytes: [u8; 4] = dense[i as usize].values_le[4 * lane..4 * lane + 4].try_into().unwrap();
        let v = if leaf.dtype == DType::Idx { u32::from_le_bytes(bytes) as i128 } else { i32::from_le_bytes(bytes) as i128 };
        let Some(w) = [v + 1, v - 1].into_iter().find(|w| iv.contains(*w)) else { continue };
        let mut lie = dense.clone();
        let forged = if leaf.dtype == DType::Idx { (w as u32).to_le_bytes() } else { (w as i32).to_le_bytes() };
        lie[i as usize].values_le[4 * lane..4 * lane + 4].copy_from_slice(&forged);
        let class_id = c.class.class_id(&c.artifact_root);
        let ctx_hash = c.ctx.context_hash();
        let hashes: Vec<Hash64> = lie.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &class_id, p)).collect();
        let mut binding = honest.binding.clone();
        binding.step_merkle_root = step_merkle_root_v1(&hashes).unwrap();
        binding.committed_execution_root = palw_tir_execution_root_v1(
            &ctx_hash,
            &binding.full_logits_trace_root,
            &class_id,
            binding.step_leaf_count,
            &binding.step_merkle_root,
        );
        let opening = TirStepTreeV1::full(&hashes).opening(i).unwrap();
        let start = Instant::now();
        let challenger = TirEvidenceV1::challenger(
            &runner,
            &binding,
            &honest,
            &opening,
            lie[i as usize].clone(),
            &honest.logits_rows,
            &honest.generated,
            &inventory,
            PalwPromptIdsFormV1::Flat,
            LADDER,
        )
        .unwrap();
        let r = challenger.cone_refutation(i, &court_rules).unwrap();
        let build = ns(start);
        let start = Instant::now();
        let verdict = check_tir_cone_refutation_v1(&r, &court_rules).expect("the forged leaf is convicted");
        let check = ns(start);
        filing = json!({
            "leaf": i,
            "fault": format!("{:?}", verdict.fault),
            "challenger_build_ns": build,
            "court_check_ns": check,
            "witness_bytes": borsh::to_vec(&r).unwrap().len(),
        });
        break;
    }
    assert!(!filing.is_null(), "{name}: a forged leaf is filed and convicted");

    let step_check = json!({
            "leaves": court_n, "retain_ns": retain_ns, "replay_pass_ns": replay_pass_ns,
            "owner_refutation_build_max_ns": build_max, "owner_refutation_build_total_ns": build_total,
            "court_check_max_ns": check_max, "court_check_total_ns": check_total, "refutation_bytes_max": refutation_bytes_max,
        });
    (step_check, filing)
        }
    };

    // --- frame bytes: the full-frame encode + hash at the uppers 6 and 257 ------------------------------------------------------
    let frame = |plan: &kaspa_consensus_core::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1| {
        let bytes = plan.control_footprint.maximum_frame_bytes;
        let buffer = vec![0x5Au8; bytes as usize];
        let start = Instant::now();
        let encoded = borsh::to_vec(plan).unwrap();
        let digest = blake2b_simd::Params::new().hash_length(64).hash(&buffer);
        std::hint::black_box((encoded.len(), digest));
        json!({
            "upper": plan.acquisition.maximum_custodians,
            "frame_bytes": bytes,
            "plan_bytes": borsh::to_vec(plan).unwrap().len(),
            "encode_and_hash_ns": ns(start),
            "charged_parser_steps_per_action": bytes.saturating_mul(32).saturating_mul(32),
            "owner_reads": plan.acquisition.envelope.maximum_moves + plan.owner_work.envelope.maximum_moves,
        })
    };

    json!({
        "class": name,
        "geometry": { "leaves": n, "decode": d, "vocab": vocab, "tiles": tiles, "prefill": header.job_context.declared_prefill_tokens },
        "bounds_upper6": {
            "passes": passes,
            "unit_bytes": unit,
            "prepared_disk_bytes": plan6.executor.prepared_disk_bytes,
            "preparation_slices_per_actor": plan6.preparation.maximum_slices_per_actor,
            "preparation_slices_per_pass": per_pass_slices,
            "owner_lane_attempts": plan6.owner_work.per_actor.attempts,
            "custodian_lane_attempts": plan6.owner_work.per_custodian.attempts,
            "preparation_daa": plan6.preparation.preparation_daa,
            "collect_after_accept_daa": plan6.terms.collect_until_daa - plan6.terms.accepted_daa,
            "rent_sompi": plan6.terms.plan.rent_sompi,
        },
        "execute": { "ns": execute_ns, "capture_bytes": material.len() },
        "answer_cold": {
            "requests": requests.len(), "open_ns": cold_open_ns, "slices": cold_slices, "ns": cold_ns, "slice_max_ns": cold_slice_max,
            "answer_bytes_max": answer_bytes_max, "authenticate_ns": auth_ns, "authenticate_max_ns": auth_max,
        },
        "preparation_pass": pass,
        "step_run": { "count": run_count, "answer_bytes_max": run_answer_max, "bound_bytes": run_bound,
                      "widest_leaf_value_bytes": source.widest_leaf_value_bytes_v1(), "layout_leaf_bytes": source.maximum_leaf_bytes_v1() },
        "preparation": {
            "publish_slices": publish_slices, "publish_ns": publish_ns, "publish_slice_max_ns": publish_slice_max,
            "publish_io_bytes": publish_io, "retained_bytes": descriptor.retained_bytes, "quoted_publish_slices": need.maximum_publish_slices,
            "warm_open_ns": warm_open_ns, "warm_leaf_answers": n, "warm_slices": warm_slices, "warm_ns": warm_ns,
        },
        "step_check": step_check,
        "filing": filing,
        "frame": [frame(&plan6), frame(&plan257)],
        "plans": [bounds(&plan6), bounds(&plan257)],
    })
}

/// **Every corpus class's frame operations, measured** — and the allocation's bounds checked against what ran.
#[test]
fn tir_frame_operations_are_measured_on_every_corpus_class() {
    let dir = tempfile::tempdir().unwrap();
    let mut classes = Vec::new();
    let mut k = 0usize;
    for (name, program, params) in programs() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() {
            continue;
        }
        let program = TirProgramV1::decode_canonical(&tiled(program).encode()).unwrap();
        let lay = layout(&program, 5, 2, 2, 64);
        let path = dir.path().join(format!("{}.palwtir", name.replace(' ', "-")));
        let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
            params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
        };
        misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&lay).unwrap(), [2; 64], name.clone(), &mut tensor)
            .unwrap();
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
        let (root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let canonical =
            kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
        let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, LADDER)
            .unwrap()
            .with_network_id_v1(NET);
        classes.push(measure(&name, &backend, &program, Some(&params), k));
        k += 1;
    }
    assert_eq!(classes.len(), 11, "every corpus class");
    let record = json!({
        "record": "misaka-palw/tir-frame-measure/v1",
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "host": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "threads": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
            "label": std::env::var("MISAKA_MEASURE_HOST").unwrap_or_default(),
        },
        "source_commit": std::env::var("MISAKA_SOURCE_COMMIT").unwrap_or_default(),
        "status": "not arming evidence: tiny corpus classes; wall-clock of this host and build; arming needs a release build on the \
                   slowest supported node with genesis-size classes",
        "classes": classes,
    });
    let canonical = serde_json::to_string(&record).unwrap();
    let digest = blake2b_simd::Params::new().hash_length(32).hash(canonical.as_bytes());
    if let Ok(out) = std::env::var("MISAKA_TIR_FRAME_MEASURE_OUT") {
        std::fs::write(&out, &canonical).unwrap();
        eprintln!("[tir-frame-measure] wrote {out}, blake2b-256 {}", digest.to_hex());
    }
    for class in record["classes"].as_array().unwrap() {
        eprintln!(
            "[tir-frame-measure] {} n={} prepared={}B/{} slices (bound {} per pass) cold={} slices answer_max={}B step_check_max={}ns \
             filing_witness={}B frame6={}B frame257={}B",
            class["class"],
            class["geometry"]["leaves"],
            class["preparation"]["retained_bytes"],
            class["preparation"]["publish_slices"],
            class["bounds_upper6"]["preparation_slices_per_pass"],
            class["answer_cold"]["slices"],
            class["answer_cold"]["answer_bytes_max"],
            class["step_check"]["court_check_max_ns"],
            class["filing"]["witness_bytes"],
            class["frame"][0]["frame_bytes"],
            class["frame"][1]["frame_bytes"],
        );
    }
}

/// **One real class's frame operations** (the court half unmeasured): `MISAKA_TIR_FRAME_MEASURE_PALWTIR` names a `PALWTIR1` file
/// (e.g. `palwart extract-legacy` of a genesis `PALWART3`), opened as the node opens it, with the container's own layout.
/// Run: `MISAKA_TIR_FRAME_MEASURE_PALWTIR=<file> [MISAKA_TIR_FRAME_MEASURE_OUT=<json>] cargo test --release -p misaka-palw-tir-exec
/// --features node --test frame_measure -- --ignored --nocapture tir_frame_operations_on_a_real_class`
#[test]
#[ignore]
fn tir_frame_operations_on_a_real_class() {
    let path = std::path::PathBuf::from(std::env::var("MISAKA_TIR_FRAME_MEASURE_PALWTIR").expect("a PALWTIR1 file"));
    let start = Instant::now();
    let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
    let open_ns = ns(start);
    let start = Instant::now();
    let (root, _) = artifact.inventory_root().unwrap();
    let root_ns = ns(start);
    let class = artifact.class().unwrap();
    let program = TirProgramV1::decode_canonical(&class.program).unwrap();
    let canonical =
        kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, LADDER)
        .unwrap()
        .with_network_id_v1(NET);
    let measured = measure(&name, &backend, &program, None, 0);
    let record = json!({
        "record": "misaka-palw/tir-frame-measure-real/v1",
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "host": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "label": std::env::var("MISAKA_MEASURE_HOST").unwrap_or_default() },
        "source_commit": std::env::var("MISAKA_SOURCE_COMMIT").unwrap_or_default(),
        "artifact": path.display().to_string(),
        "class_id": format!("{}", class.class_id(&root)),
        "inventory_root": format!("{root}"),
        "open_ns": open_ns, "inventory_root_ns": root_ns,
        "class": measured,
    });
    println!("[tir-frame-measure-real] {}", serde_json::to_string(&record).unwrap());
    if let Ok(out) = std::env::var("MISAKA_TIR_FRAME_MEASURE_OUT") {
        std::fs::write(out, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    }
}

/// **Analysis only (2026-10-18, door breakdown).** Decomposes the conviction door of a real class term by term, re-deriving each
/// term from the public frame-cost functions, asserts the decomposition equals the production allocation's door, and perturbs one
/// term at a time. Changes no rule: every perturbed value is computed here, never fed back into consensus.
/// Run: `MISAKA_TIR_FRAME_MEASURE_PALWTIR=<file> MISAKA_DOOR_BREAKDOWN_OUT=<json> cargo test --release -p misaka-palw-tir-exec
///  --features node --test frame_measure door_breakdown_on_a_real_class -- --nocapture`
#[test]
fn door_breakdown_on_a_real_class() {
    use kaspa_consensus_core::palw_tir_frame_cost_v1 as fc;
    use kaspa_consensus_core::palw_tir_public_owned_v1::{PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1, request_body_bound_v1};
    let path = std::path::PathBuf::from(std::env::var("MISAKA_TIR_FRAME_MEASURE_PALWTIR").expect("a PALWTIR1 file"));
    let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
    let (root, _) = artifact.inventory_root().unwrap();
    let class = artifact.class().unwrap();
    let canonical =
        kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, LADDER)
        .unwrap()
        .with_network_id_v1(NET);
    // --- the same execute + binding + source as `measure` (no answers are produced here) --------------------------------------
    let (_, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let prompt: Vec<u32> = prompt.into_iter().map(|t| t as u32).collect();
    let job = fp_job(&backend, &prompt, 2);
    let ids: Vec<usize> = prompt.iter().map(|t| *t as usize).collect();
    let run = backend.execute_free_prompt(&job, &ids).unwrap();
    let material = run.outcome.material.clone();
    let shape = backend.capture_shape(&material).unwrap();
    let commitment = palw_fp_commitment_from_context_v3(&job, &shape.job_context, &run, 999_999).unwrap();
    let PalwPublicExecutionBindingHeaderV1::Tir(header) = backend.public_execution_binding_header_v1(&material).unwrap() else {
        panic!("an IR capture exports a TIR header")
    };
    let (record, _) =
        kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(backend.class(), &backend.artifact_root()).unwrap();
    let source = PalwTirPublicSourceV1::new_v1(
        &record,
        &backend.class().layout,
        &header,
        commitment.execution_root,
        commitment.trace_root,
        LADDER,
    )
    .unwrap();
    let geometry = kaspa_consensus_core::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
        class_id: job.class_id,
        producer: PalwBondKeyV2(job.executor_bond),
        execution_root: commitment.execution_root,
        job_identity: palw_fp_job_pin_v1(&commitment),
        accepted_daa: 100,
    };
    let cost = candidate();
    let quote = |upper: u64| match derive_tir_public_allocation_v1(
        &geometry,
        &commitment,
        &header,
        &record,
        &backend.class().layout,
        None,
        None,
        rules(upper),
    ) {
        Ok(PalwTirPublicAllocationQuoteV1::Complete(plan)) => *plan,
        other => panic!("no complete frame at upper {upper}: {other:?}"),
    };
    let cfg = cost.config;
    let clk = cost.clock;
    let margin = cfg.prices.inclusion_margin_daa;
    let acq_daa = cfg.acquisition_daa;
    let n = source.leaf_count_v1();
    let d = u64::from(source.decode_count_v1());
    let tiles = u64::from(source.vocab_v1()).div_ceil(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
    let tiled = source.supports_tiled_trace_v1() && d > 0;
    let level = |count: u64| (64 - count.max(2).saturating_sub(1).leading_zeros()).min(10) as u8;
    let height = |count: u64| if count <= 1 { 0 } else { u64::from(64 - (count - 1).leading_zeros()) };
    let descent = |count: u64| height(count).div_ceil(10) + 1;
    // Production body bounds of each request type (the bound every span is computed from).
    let body = |r: PalwTirPublicRequestV1| request_body_bound_v1(&source, r).unwrap().0;
    let body_ids = body(PalwTirPublicRequestV1::Ids { offset: 0 });
    let body_leaf = body(PalwTirPublicRequestV1::StepLeaf { index: 0 });
    let body_node = if n > 1 { body(PalwTirPublicRequestV1::StepNode { level: level(n), index: 0 }) } else { body_leaf };
    let body_run = body(PalwTirPublicRequestV1::StepRun { first: 0, count: n.clamp(1, 256) as u32 });
    let body_tile = body(PalwTirPublicRequestV1::TraceTile { row: 0, tile: 0 });
    let body_rows = if d > 1 { body(PalwTirPublicRequestV1::RowsNode { level: level(d), index: 0 }) } else { body_tile };
    let body_tnode = if tiles > 1 { body(PalwTirPublicRequestV1::TileNode { row: 0, level: level(tiles), index: 0 }) } else { body_tile };
    let cone_cap = n.div_ceil(256);
    // The path as production composes it (`tir_public_localization_path_v1`), parameterised by the terms a lever may change:
    // `slots` (upper + 1), the run body, and the cone run count. Returns (turns, DAA, widest span, per-segment DAA).
    let path = |slots: u64, run_body: u64, cone_runs: u64| -> (u64, u64, u64, Vec<(&'static str, u64, u64)>) {
        let span = |b: u64| fc::tir_public_turn_span_v1(b, slots, &cfg, &clk).unwrap();
        let mut segs: Vec<(&'static str, u64, u64)> = Vec::new();
        let push = |segs: &mut Vec<(&'static str, u64, u64)>, name: &'static str, count: u64, each: u64| {
            segs.push((name, count, each));
        };
        let span_leaf = span(body_leaf);
        let span_node = if n > 1 { span(body_node) } else { span_leaf };
        let span_run = if n.clamp(1, 256) > 1 { span(run_body) } else { span_leaf };
        let span_tile = span(body_tile);
        if tiled {
            push(&mut segs, "ids (one demand, complete sequence)", 1, span(body_ids));
        }
        push(&mut segs, "step-tree descent to first differing leaf", descent_units(n), span_node);
        push(&mut segs, "leaf itself", 1, span_leaf);
        push(&mut segs, "cone runs (localisation of the leaf's dependencies)", cone_runs, span_run);
        if tiled {
            push(&mut segs, "rows-tree descent (trace path)", descent(d), if d > 1 { span(body_rows) } else { span_tile });
            push(&mut segs, "tile-tree descent (trace path)", descent(tiles), if tiles > 1 { span(body_tnode) } else { span_tile });
            push(&mut segs, "trace tiles of one row", tiles, span_tile);
        }
        let widest = segs.iter().map(|s| s.2).max().unwrap_or(0);
        push(&mut segs, "output / stop-rule check", 1, widest);
        let restarts = cfg.prices.reorg_replay_moves;
        push(&mut segs, "reorg restart turns (at the widest span)", restarts, widest);
        let turns = segs.iter().map(|s| s.1).sum();
        let daa = segs.iter().map(|s| s.1 * s.2).sum();
        (turns, daa, widest, segs)
    };
    let path_k = |slots: u64, run_body: u64, cone_runs: u64, k: u64| -> u64 {
        let span = |b: u64| {
            let parts = b.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64).max(1);
            let custodian = 1 + 2 + parts;
            let all = slots * k;
            cfg.acquisition_daa + (1 + 1 + custodian) * margin + (all * custodian * clk.control_bytes + all * b).div_ceil(clk.parallel_bytes_per_daa)
        };
        let leaf = span(body_leaf);
        let node = if n > 1 { span(body_node) } else { leaf };
        let run = if n.clamp(1, 256) > 1 { span(run_body) } else { leaf };
        let tile = span(body_tile);
        let mut segs = vec![];
        if tiled { segs.push((1u64, span(body_ids))); }
        segs.push((descent_units(n), node));
        segs.push((1, leaf));
        segs.push((cone_runs, run));
        if tiled {
            segs.push((descent(d), if d > 1 { span(body_rows) } else { tile }));
            segs.push((descent(tiles), if tiles > 1 { span(body_tnode) } else { tile }));
            segs.push((tiles, tile));
        }
        let widest = segs.iter().map(|s| s.1).max().unwrap_or(0);
        segs.push((1, widest));
        segs.push((cfg.prices.reorg_replay_moves, widest));
        segs.iter().map(|s| s.0 * s.1).sum()
    };
    fn descent_units(n: u64) -> u64 {
        let h = if n <= 1 { 0 } else { u64::from(64 - (n - 1).leading_zeros()) };
        h.div_ceil(10) + 1
    }
    // The whole door from its terms (`derive_tir_public_allocation_v1`): preparation + Open window + acquisition path + owner queue
    // + the collection allowance (max(collection, filing)).
    let door_of = |upper: u64, path_daa: u64, queue: u64, filing_moves: u64, prep: u64| -> u64 {
        let post = path_daa + queue;
        let filing = fc::tir_public_filing_daa_v1(filing_moves, &cfg, &clk).unwrap();
        prep + (acq_daa + margin) + post.max(1) + cfg.collection_daa.max(filing) + 0 * upper
    };
    let mut rows = Vec::new();
    let mut prod = Vec::new();
    let real_run_body = body_run;
    let widest_leaf_value = source.widest_leaf_value_bytes_v1();
    for upper in [6u64, 257] {
        let plan = quote(upper);
        let door = plan.terms.collect_until_daa - plan.terms.accepted_daa;
        let prep = fc::tir_public_preparation_door_v1(upper, &cfg, &clk).unwrap();
        let queue = fc::tir_public_owner_queue_wait_v1(acq_daa + margin, &cfg, &clk).unwrap();
        let (turns, path_daa, widest, segs) = path(upper + 1, real_run_body, cone_cap.min(cone_cap));
        // The production run's cone count is the source's own bound, not the ⌈n/256⌉ cap: read it back.
        let cone_actual = source.cone_runs_v1();
        let (t2, p2, _, _) = path(upper + 1, real_run_body, cone_actual);
        assert_eq!(p2, plan.acquisition.envelope.inclusion_daa, "re-derived path == production acquisition inclusion");
        assert_eq!(t2, turns - (cone_cap - cone_actual) * 0 + 0 * turns, "turn count");
        let filing_moves = plan.terminal.envelope.maximum_moves;
        let rebuilt = door_of(upper, p2, queue, filing_moves, prep);
        assert_eq!(rebuilt, door, "re-derived door == production collect_until - accepted (upper {upper})");
        let filing = fc::tir_public_filing_daa_v1(filing_moves, &cfg, &clk).unwrap();
        prod.push(json!({
            "upper": upper,
            "door_daa": door,
            "preparation_daa": prep,
            "open_window_daa": acq_daa + margin,
            "acquisition_path_daa": p2,
            "queue_wait_daa": queue,
            "collection_allowance_daa": cfg.collection_daa.max(filing),
            "filing_daa": filing,
            "collection_daa_config": cfg.collection_daa,
            "cone_runs_actual": cone_actual,
            "cone_runs_cap_ceil_n_256": cone_cap,
            "widest_span_daa": widest,
            "turns": t2,
            "filing_moves": filing_moves,
            "segments": segs.iter().map(|(name, count, each)| json!({"segment": name, "turns": count, "span_daa": each, "daa": count * each})).collect::<Vec<_>>(),
        }));
        let _ = rows.len();
        rows.push(upper);
    }
    // --- levers, one term at a time (computed here only; the production rules are unchanged) ----------------------------------
    let base_plan6 = quote(6);
    let prep6 = fc::tir_public_preparation_door_v1(6, &cfg, &clk).unwrap();
    let queue6 = fc::tir_public_owner_queue_wait_v1(acq_daa + margin, &cfg, &clk).unwrap();
    let cone_actual = source.cone_runs_v1();
    let fm6 = base_plan6.terminal.envelope.maximum_moves;
    let base = door_of(6, path(7, real_run_body, cone_actual).1, queue6, fm6, prep6);
    let mut levers = Vec::new();
    let mut lever = |label: &str, formula: &str, door: u64| {
        levers.push(json!({"lever": label, "formula": formula, "door_daa": door, "fits_3120": door <= 3_120, "vs_base": door as i64 - base as i64}));
    };
    lever("baseline (upper 6, production terms)", "as shipped", base);
    lever("upper 257 (production)", "slots 258", door_of(257, path(258, real_run_body, cone_actual).1, fc::tir_public_owner_queue_wait_v1(acq_daa + margin, &cfg, &clk).unwrap(), fm6, prep6));
    lever("cone turns = 1 (lower bound of any derived cone bound)", "cone_runs 1", door_of(6, path(7, real_run_body, 1).1, queue6, fm6, prep6));
    lever("cone turns = ceil(n/256) cap (no derivation)", &format!("cone_runs {cone_cap}"), door_of(6, path(7, real_run_body, cone_cap).1, queue6, fm6, prep6));
    lever("answer body = real widest leaf x 256 (no run slack)", &format!("run body {} B = 256 x {widest_leaf_value} B value", 256 * widest_leaf_value), door_of(6, path(7, 256 * widest_leaf_value, cone_actual).1, queue6, fm6, prep6));
    lever("answer body = 0 (fixed-part only, lower bound)", "run body 0", door_of(6, path(7, 0, cone_actual).1, queue6, fm6, prep6));
    lever("only demanded custodians answer (slots 2)", "slots 2", door_of(1, path(2, real_run_body, cone_actual).1, queue6, fm6, prep6));
    lever("owner queue wait removed", "queue 0", door_of(6, path(7, real_run_body, cone_actual).1, 0, fm6, prep6));
    lever("cone = 1 AND body = real widest leaf x 256", "both", door_of(6, path(7, 256 * widest_leaf_value, 1).1, queue6, fm6, prep6));
    // K_owner lever: the turn span at K concurrent owners (production K = 8), re-derived from its formula and checked at K = 8.
    let span_k = |b: u64, slots: u64, k: u64| -> u64 {
        let parts = b.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64).max(1);
        let custodian = 1 + 2 + parts;
        let owner = 1 + 1;
        let all = slots * k;
        let parallel = (all * custodian * clk.control_bytes + all * b).div_ceil(clk.parallel_bytes_per_daa);
        cfg.acquisition_daa + (owner + custodian) * margin + parallel
    };
    assert_eq!(span_k(real_run_body, 7, 8), fc::tir_public_turn_span_v1(real_run_body, 7, &cfg, &clk).unwrap(), "K-span formula == production at K = 8");
    lever("K_owner = 1 (one owner, not 8)", "all = slots x 1", door_of(6, path_k(7, real_run_body, cone_actual, 1), queue6, fm6, prep6));
    lever("K_owner = 4", "all = slots x 4", door_of(6, path_k(7, real_run_body, cone_actual, 4), queue6, fm6, prep6));
    // Regression (step 1): with the liability horizon ARMED at 3,120 DAA the real class is refused on the door, not built.
    // The unlimited setting above only measures; it must never count as arming evidence.
    {
        let mut armed = candidate();
        armed.clock.maximum_door_daa = 3_120;
        match derive_tir_public_allocation_v1(&geometry, &commitment, &header, &record, &backend.class().layout, None, None, rules_with(6, armed)) {
            Ok(PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. }) => {
                assert!(
                    missing.iter().any(|m| matches!(m, PalwTirPublicAllocationMissingV1::DoorBeyondLiabilityHorizon { required: 1_062_231, allowed: 3_120 })),
                    "armed at 3,120 the door must be refused: {missing:?}"
                );
            }
            other => panic!("armed at 3,120 the real class must be refused on the door: {other:?}"),
        }
    }
    let horizon = t12_horizon_daa();
    let record = json!({
        "record": "misaka-palw/g14-door-breakdown/v1",
        "date": "2026-10-18",
        "status": "analysis only: no rule, constant, price or window changed; every value re-derived from the public functions",
        "class": name,
        "geometry": { "leaves": n, "decode": d, "vocab": source.vocab_v1(), "tiles": tiles, "prefill": header.job_context.declared_prefill_tokens },
        "config": { "acquisition_daa": acq_daa, "inclusion_margin_daa": margin, "collection_daa": cfg.collection_daa, "reorg_replay_moves": cfg.prices.reorg_replay_moves, "reorg_retention_daa": cfg.reorg_retention_daa },
        "clock": { "control_bytes": clk.control_bytes, "parallel_bytes_per_daa": clk.parallel_bytes_per_daa },
        "answer": { "run_body_bound_bytes": real_run_body, "leaf_body_bound_bytes": body_leaf, "widest_leaf_value_bytes": widest_leaf_value, "ids_body_bytes": body_ids, "tile_body_bytes": body_tile },
        "horizon_candidate_daa": horizon,
        "production": prod,
        "levers": levers,
    });
    println!("[door-breakdown] {}", serde_json::to_string(&record).unwrap());
    if let Ok(out) = std::env::var("MISAKA_DOOR_BREAKDOWN_OUT") {
        std::fs::write(out, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    }
}

/// **Analysis only (2026-10-18, read set).** The court's read set of ONE disputed step, per step kind, on a real class. The read set
/// is the refutation the production builder (`backend.cone_refutation`, the court's own `TirSource` over the dense capture) carries:
/// its step-leaf operands (`operands`), its inventory multiproof (`params`), its prompt and decode carriage. Nothing here
/// materialises the class's i128 params and no rule is changed. Per step kind: sampled disputed leaves; per leaf: distinct leaves,
/// distinct 256-leaf runs, value bytes, the whole-run bytes of the touched runs, the wire bytes of the refutation.
/// Run: `MISAKA_TIR_FRAME_MEASURE_PALWTIR=<file> MISAKA_READ_SET_OUT=<json> [MISAKA_READ_SET_PER_KIND=4] cargo test --release -p
///  misaka-palw-tir-exec --features node --test frame_measure read_set_on_a_real_class -- --nocapture`
#[test]
fn read_set_on_a_real_class() {
    use std::collections::BTreeMap;
    let path = std::path::PathBuf::from(std::env::var("MISAKA_TIR_FRAME_MEASURE_PALWTIR").expect("a PALWTIR1 file"));
    let per_kind: usize = std::env::var("MISAKA_READ_SET_PER_KIND").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
    let (root, _) = artifact.inventory_root().unwrap();
    let class = artifact.class().unwrap();
    let canonical =
        kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().to_string();
    let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, LADDER)
        .unwrap()
        .with_network_id_v1(NET);
    let (_, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
    let prompt: Vec<u32> = prompt.into_iter().map(|t| t as u32).collect();
    let decode: u32 = std::env::var("MISAKA_READ_SET_DECODE").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
    let job = fp_job(&backend, &prompt, decode);
    let ids: Vec<usize> = prompt.iter().map(|t| *t as usize).collect();
    let run = backend.execute_free_prompt(&job, &ids).unwrap();
    let material = run.outcome.material.clone();
    let capture = backend.decode_capture(&material).unwrap();
    let ctx = capture.binding.job_context.clone();
    let n = capture.binding.step_leaf_count;
    let space = backend.space();
    let program = &space.program;
    drop(capture);
    let rules = PalwTirCourtRulesV1 { max_step_leaf_count: LADDER, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };

    // Step kind of every leaf: a commit point by its primitive, a checkpoint tile, or a history tile.
    let prim_name = |p: &misaka_palw_tir::Prim| -> String { format!("{p:?}").chars().take_while(|c| c.is_alphanumeric()).collect() };
    let mut groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut value_count: Vec<u32> = Vec::with_capacity(n as usize);
    for i in 0..n {
        let leaf = space.leaf_at(&ctx, i).unwrap();
        let label = match leaf.kind {
            kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1::Commit { block, node, .. } => {
                let b = &program.blocks[block as usize];
                format!("commit:{}:{}", b.name.split('.').next().unwrap_or(&b.name), prim_name(&b.nodes[node as usize].prim))
            }
            kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1::State { state, .. } => format!("state:{state}"),
            kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1::HistTile { state, .. } => format!("hist:{state}"),
        };
        groups.entry(label).or_default().push(i);
        value_count.push(leaf.value_count);
    }
    let whole_run_bytes = |run: u64| -> u64 {
        let first = run * 256;
        let last = (first + 256).min(n);
        (first..last).map(|i| 4 * value_count[i as usize] as u64).sum()
    };

    let mut kinds = Vec::new();
    let (mut max_runs, mut max_leaves, mut max_value_bytes, mut max_run_bytes, mut max_wire) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut total_trials = 0u64;
    let start_all = Instant::now();
    for (label, idx) in &groups {
        // Evenly spaced disputed leaves across the kind's leaves (every position it occurs at), plus the last one.
        let steps = per_kind.max(2) - 1;
        let mut picks: Vec<u64> = (0..=steps).map(|k| idx[(k * (idx.len() - 1)) / steps]).collect();
        picks.push(*idx.last().unwrap());
        picks.sort_unstable();
        picks.dedup();
        let (mut k_runs, mut k_leaves, mut k_value, mut k_run_bytes, mut k_wire, mut k_params, mut k_prompt_decode) =
            (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
        let mut samples = Vec::new();
        for &i in &picks {
            let r = backend.cone_refutation(&material, i, &rules).unwrap();
            total_trials += 1;
            let mut runs = std::collections::BTreeSet::new();
            let mut value_bytes = 0u64;
            for p in &r.operands.preimages {
                let li = space.leaf_index(&ctx, &p.coord).expect("an operand is a leaf of this job");
                runs.insert(li / 256);
                value_bytes += p.values_le.len() as u64;
            }
            let run_bytes: u64 = runs.iter().map(|r| whole_run_bytes(*r)).sum();
            let params_bytes = borsh::to_vec(&r.params).unwrap().len() as u64;
            let prompt_decode = (borsh::to_vec(&r.prompt_token_ids).unwrap().len()
                + borsh::to_vec(&r.prompt_ids_openings).unwrap().len()
                + borsh::to_vec(&r.decode_tokens).unwrap().len()) as u64;
            let wire = borsh::to_vec(&r).unwrap().len() as u64;
            let leaves = r.operands.preimages.len() as u64;
            k_runs = k_runs.max(runs.len() as u64);
            k_leaves = k_leaves.max(leaves);
            k_value = k_value.max(value_bytes);
            k_run_bytes = k_run_bytes.max(run_bytes);
            k_wire = k_wire.max(wire);
            k_params = k_params.max(params_bytes);
            k_prompt_decode = k_prompt_decode.max(prompt_decode);
            samples.push(json!({
                "leaf": i, "position": space.leaf_at(&ctx, i).unwrap().position, "distinct_leaves": leaves,
                "distinct_runs_256": runs.len(), "value_bytes": value_bytes, "whole_run_bytes": run_bytes,
                "params_multiproof_bytes": params_bytes, "prompt_decode_bytes": prompt_decode, "wire_bytes": wire,
            }));
        }
        max_runs = max_runs.max(k_runs);
        max_leaves = max_leaves.max(k_leaves);
        max_value_bytes = max_value_bytes.max(k_value);
        max_run_bytes = max_run_bytes.max(k_run_bytes);
        max_wire = max_wire.max(k_wire);
        kinds.push(json!({
            "kind": label, "disputed_leaves_of_kind": idx.len(), "sampled": picks.len(),
            "max_distinct_runs_256": k_runs, "max_distinct_leaves": k_leaves, "max_value_bytes": k_value,
            "max_whole_run_bytes": k_run_bytes, "max_params_multiproof_bytes": k_params,
            "max_prompt_decode_bytes": k_prompt_decode, "max_wire_bytes": k_wire, "samples": samples,
        }));
        println!("[read-set] {label}: n={} runs<= {k_runs} leaves<= {k_leaves} value<= {k_value} wire<= {k_wire}", idx.len());
    }
    let record = json!({
        "record": "misaka-palw/tir-read-set-real/v1",
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "host": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "label": std::env::var("MISAKA_MEASURE_HOST").unwrap_or_default() },
        "source_commit": std::env::var("MISAKA_SOURCE_COMMIT").unwrap_or_default(),
        "artifact": path.display().to_string(),
        "class_id": format!("{}", class.class_id(&root)),
        "job": { "prefill": prompt.len(), "decode": decode, "step_leaves": n, "kinds": groups.len(), "trials": total_trials },
        "layout": { "checkpoint_interval": backend.class().layout.checkpoint_interval, "h_tile": backend.class().layout.h_tile, "states": program.states.len(), "blocks": program.blocks.len(), "layers": program.schedule.layers.len() },
        "per_kind_samples": per_kind,
        "elapsed_s": start_all.elapsed().as_secs(),
        "max_over_sampled_kinds": {
            "distinct_runs_256": max_runs, "distinct_leaves": max_leaves, "value_bytes": max_value_bytes,
            "whole_run_bytes": max_run_bytes, "wire_bytes": max_wire,
        },
        "kinds": kinds,
    });
    println!("[read-set] {}", serde_json::to_string(&record["max_over_sampled_kinds"]).unwrap());
    if let Ok(out) = std::env::var("MISAKA_READ_SET_OUT") {
        std::fs::write(out, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    }
}

/// **The position-window rule, checked against the real evaluator** (G14, `derived-upper-1018`). For every disputed step leaf of
/// every fixture class (checkpoint interval 2, history tile 2, a 10-position job, so State and HistTile leaves and their boundaries
/// all occur), the operands the production cone refutation carries must satisfy the window rule: a commit or state operand at
/// position `q` with `q <= p <= q + C + h + 1` (`p` the disputed position), or a HistTile operand at `q < p`; every operand precedes
/// the disputed leaf. Test only; no rule, constant or price.
#[test]
fn read_set_containment_on_fixture_classes() {
    use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1 as K;
    let dir = tempfile::tempdir().unwrap();
    let (mut trials, mut operands, mut state_ops, mut hist_ops) = (0u64, 0u64, 0u64, 0u64);
    let mut contiguous_tiles = 0u64;
    let mut classes = Vec::new();
    for (name, program, params) in programs() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() {
            continue;
        }
        let program = TirProgramV1::decode_canonical(&tiled(program).encode()).unwrap();
        let lay = layout(&program, 5, 2, 2, 64);
        let path = dir.path().join(format!("{}.palwtir", name.replace(' ', "-")));
        let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
            params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
        };
        misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&lay).unwrap(), [2; 64], name.clone(), &mut tensor)
            .unwrap();
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
        let (root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let canonical =
            kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
        let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, LADDER)
            .unwrap()
            .with_network_id_v1(NET);
        let (_, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
        let prompt: Vec<u32> = prompt.into_iter().map(|t| t as u32).collect();
        let job = fp_job(&backend, &prompt, 6);
        let ids: Vec<usize> = prompt.iter().map(|t| *t as usize).collect();
        let run = backend.execute_free_prompt(&job, &ids).unwrap();
        let material = run.outcome.material.clone();
        let capture = backend.decode_capture(&material).unwrap();
        let ctx = capture.binding.job_context.clone();
        let n = capture.binding.step_leaf_count;
        drop(capture);
        let space = backend.space();
        let (c, h) = (space.layout.checkpoint_interval as i64, space.layout.h_tile as i64);
        let rules = PalwTirCourtRulesV1 { max_step_leaf_count: LADDER, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };
        let mut class_ops = 0u64;
        // A completed history tile's leaves are contiguous in the step index (the structural run count assumes it).
        let mut tile_ranges: std::collections::BTreeMap<u32, (u64, u64, u64)> = std::collections::BTreeMap::new();
        for i in 0..n {
            let l = space.leaf_at(&ctx, i).unwrap();
            if matches!(l.kind, K::HistTile { .. }) {
                let e = tile_ranges.entry(l.position).or_insert((i, i, 0));
                e.0 = e.0.min(i);
                e.1 = e.1.max(i);
                e.2 += 1;
            }
        }
        for (pos, (lo, hi, cnt)) in &tile_ranges {
            assert!(hi - lo + 1 == *cnt, "{name}: the history tile at position {pos} is not one contiguous run of leaves");
            contiguous_tiles += 1;
        }
        for i in 0..n {
            let p = space.leaf_at(&ctx, i).unwrap().position as i64;
            let r = backend.cone_refutation(&material, i, &rules).unwrap();
            trials += 1;
            for pre in &r.operands.preimages {
                let li = space.leaf_index(&ctx, &pre.coord).expect("an operand is a leaf of this job");
                let op = space.leaf_at(&ctx, li).unwrap();
                let q = op.position as i64;
                let is_hist = matches!(op.kind, K::HistTile { .. });
                let in_window = q <= p && p <= q + c + h + 1;
                let old_hist = is_hist && q < p;
                assert!(li < i, "{name}: leaf {i} reads leaf {li}, which does not precede it");
                assert!(
                    in_window || old_hist,
                    "{name}: disputed leaf {i} at position {p} reads leaf {li} at position {q} (history: {is_hist}), outside the window rule"
                );
                operands += 1;
                class_ops += 1;
                if matches!(op.kind, K::State { .. }) {
                    state_ops += 1;
                }
                if is_hist {
                    hist_ops += 1;
                }
            }
        }
        classes.push(json!({ "class": name, "step_leaves": n, "disputed_trials": n, "operands_checked": class_ops }));
    }
    assert!(trials > 0 && operands > 0, "no fixture class was checked");
    println!("[containment] trials={trials} operands={operands} state_operands={state_ops} hist_operands={hist_ops} contiguous_history_tiles={contiguous_tiles}");
    if let Ok(out) = std::env::var("MISAKA_CONTAINMENT_OUT") {
        let record = json!({ "trials": trials, "operands": operands, "state_operands": state_ops, "hist_operands": hist_ops, "contiguous_history_tiles": contiguous_tiles, "classes": classes });
        std::fs::write(out, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    }
}

/// **The position-window structure of a real class, params-free** (no artifact tensors are read; `TirArtifactV1::open` and the class
/// record only). Run: `MISAKA_TIR_FRAME_MEASURE_PALWTIR=<file> MISAKA_WINDOW_OUT=<json> cargo test --release -p misaka-palw-tir-exec
///  --features node --test frame_measure structural_read_window_on_a_real_class -- --nocapture --ignored`.
/// Values are the window rule's bound (a superset), not observations. The shipped `cone_runs_bound_v1` is printed beside it.
#[test]
#[ignore]
fn structural_read_window_on_a_real_class() {
    use kaspa_consensus_core::palw_tir_step_v1::{PalwTirJobShapeV1 as J, PalwTirStepSpaceV1};
    let path = std::path::PathBuf::from(std::env::var("MISAKA_TIR_FRAME_MEASURE_PALWTIR").expect("a PALWTIR1 file"));
    let artifact = TirArtifactV1::open(&path).unwrap();
    let class = artifact.class().unwrap();
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let (c, h) = (u64::from(space.layout.checkpoint_interval), u64::from(space.layout.h_tile));
    let max_ctx = space.layout.max_context;
    let hist_tile = space.hist_leaves_per_tile_v1();
    let total_of = |job: &J| -> u64 { (0..job.positions).map(|q| space.leaves_at_position(job, q)).sum() };
    // A prefill of 1 keeps the post occurrence at every position (the upper over all prefills); the cross-check uses the measured job.
    let job_max = J { prefill: 1, positions: max_ctx };
    let leaves: Vec<u64> = (0..max_ctx).map(|q| space.leaves_at_position(&job_max, q)).collect();
    let mut pre = vec![0u64; leaves.len() + 1];
    for (q, l) in leaves.iter().enumerate() {
        pre[q + 1] = pre[q] + l;
    }
    let whole = pre[leaves.len()].div_ceil(256);
    let (mut best_leaves, mut best_runs, mut best_p, mut best_runs_contig) = (0u64, 0u64, 0u32, 0u64);
    let tile_runs = hist_tile.div_ceil(256) + 1;
    for p in 0..max_ctx as u64 {
        let lo = p.saturating_sub(c + h + 1);
        let window = pre[p as usize + 1] - pre[lo as usize];
        let old_hist = (lo / h) * hist_tile;
        let leaves_p = window + old_hist;
        let runs_p = (window.div_ceil(256) + 1 + old_hist).min(whole);
        if leaves_p > best_leaves {
            best_leaves = leaves_p;
            best_p = p as u32;
        }
        best_runs = best_runs.max(runs_p);
        let tiles_old = lo / h;
        best_runs_contig = best_runs_contig.max((window.div_ceil(256) + 1 + tiles_old * tile_runs).min(whole));
    }
    // Cross-check on the measured job (prefill 4, decode 2 -> 5 positions): the leaf count and the shipped bound must match the record.
    let measured = J { prefill: 4, positions: 5 };
    let measured_total = total_of(&measured);
    let measured_bound = space.cone_runs_bound_v1(&measured, measured_total, 256);
    let mut shipped = Vec::new();
    for positions in [6u32, 33, 165, 197, 1024, 4096, max_ctx] {
        let job = J { prefill: 1, positions };
        let total = total_of(&job);
        shipped.push(json!({ "positions": positions, "step_leaves": total, "shipped_cone_runs_bound": space.cone_runs_bound_v1(&job, total, 256) }));
    }
    let record = json!({
        "record": "misaka-palw/tir-structural-window/v1",
        "artifact": path.display().to_string(),
        "layout": { "checkpoint_interval": c, "h_tile": h, "max_context": max_ctx, "hist_leaves_per_tile": hist_tile },
        "cross_check_measured_job": { "prefill": 4, "positions": 5, "step_leaves": measured_total, "shipped_cone_runs_bound": measured_bound },
        "shipped_bound_by_geometry": shipped,
        "window_rule_over_all_legal_disputes": { "max_leaves": best_leaves, "max_runs_256_per_leaf_history": best_runs, "max_runs_256_contiguous_history_tiles": best_runs_contig, "argmax_position": best_p, "whole_runs_256": whole, "total_step_leaves_at_max_context": pre[leaves.len()] },
    });
    println!("{}", serde_json::to_string_pretty(&record).unwrap());
    if let Ok(out) = std::env::var("MISAKA_WINDOW_OUT") {
        std::fs::write(out, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    }
}

/// Structure dump of a real class (params-free): blocks, nodes (prim, inputs, output shape, commit), states, layout. Read only.
/// Run: `MISAKA_TIR_FRAME_MEASURE_PALWTIR=<file> MISAKA_STRUCT_OUT=<json> cargo test --release -p misaka-palw-tir-exec --features node
///  --test frame_measure program_structure_dump_on_a_real_class -- --ignored --nocapture`.
#[test]
#[ignore]
fn program_structure_dump_on_a_real_class() {
    let path = std::path::PathBuf::from(std::env::var("MISAKA_TIR_FRAME_MEASURE_PALWTIR").expect("a PALWTIR1 file"));
    let artifact = TirArtifactV1::open(&path).unwrap();
    let class = artifact.class().unwrap();
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let p = &space.program;
    let blocks: Vec<Value> = p
        .blocks
        .iter()
        .enumerate()
        .map(|(bi, b)| {
            let nodes: Vec<Value> = b
                .nodes
                .iter()
                .enumerate()
                .map(|(ni, n)| {
                    json!({ "i": ni, "prim": format!("{:?}", n.prim), "inputs": format!("{:?}", n.inputs),
                            "out": format!("{:?} {:?}", n.out.dtype, n.out.shape), "commit": n.commit })
                })
                .collect();
            json!({ "block": bi, "name": b.name, "window": space.info.blocks[bi].window, "is_pre": space.info.blocks[bi].is_pre,
                    "is_post": space.info.blocks[bi].is_post, "is_layer": space.info.blocks[bi].is_layer,
                    "carry_in": format!("{:?}", b.carry_in), "carry_out": format!("{:?}", b.carry_out), "nodes": nodes })
        })
        .collect();
    let states: Vec<Value> = p
        .states
        .iter()
        .map(|s| json!({ "name": s.name, "kind": format!("{:?}", s.kind), "dtype": format!("{:?}", s.dtype), "shape": s.shape, "per_layer": s.per_layer }))
        .collect();
    let params: Vec<Value> = p.params.iter().map(|d| json!({ "name": d.name, "dtype": format!("{:?}", d.dtype), "shape": d.shape, "per_layer": d.per_layer })).collect();
    let record = json!({
        "layout": { "checkpoint_interval": space.layout.checkpoint_interval, "h_tile": space.layout.h_tile, "max_context": space.layout.max_context },
        "schedule": { "pre": p.schedule.pre, "layers": p.schedule.layers, "post": p.schedule.post },
        "logits": p.logits, "history_bound": p.history_bound, "token_bound": p.token_bound,
        "carry": format!("{:?}", space.info.carry), "states": states, "params": params, "blocks": blocks,
        "commit_tile_lens": space.layout.commit_tiles,
    });
    if let Ok(out) = std::env::var("MISAKA_STRUCT_OUT") {
        std::fs::write(out, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    }
    println!("blocks={} states={} params={}", p.blocks.len(), p.states.len(), p.params.len());
}

// ---------------------------------------------------------------------------------------------------------------------------
// **Demand-driven operand closure of one disputed step** (analysis; test only; no rule, constant or window is read or changed).
//
// A value-free replica of the court's demand evaluation (`misaka_palw_tir::demand::eval_demanded` and the `TirSource` leaf
// mapping): element SETS replace values. Rules that depend on a value are taken at their worst case over every value the operand
// can hold: `Select` reads both branches; a `Gather` over a parameter reads ONE row per distinct index element, at an unknown
// row, so its reads are counted as a row bound (ceil(bytes / largest inventory leaf) + 1 per contiguous run of the row) rather than
// enumerated. Everything else is exact: the element index maps of each primitive, the commit tiles, checkpoints, history tiles and
// inventory leaves. Output: the set of exact leaves (keyed by position, occurrence, node and tile) with the first consumer kind
// that demanded each, plus the gather row bounds. Token units are counted apart (they are not step leaves).
// ---------------------------------------------------------------------------------------------------------------------------

use kaspa_consensus_core::palw_tir_court_v1::PalwTirInventoryIndexV1;
use misaka_palw_tir::demand::{DemandContext, carry_in_node_v1, hist_row_node_v1, history_length_v1, state_occurrence_v1, state_writer_v1};
use misaka_palw_tir::prim::Prim;
use misaka_palw_tir::program::{INPUT_TOKEN, Ref};
use misaka_palw_tir::types::strides;
use misaka_palw_tir::validate::ProgramInfo;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CLeaf {
    Commit { pos: u32, occ: u16, node: u16, tile: u64 },
    State { pos: u32, inst: u32, tile: u64 },
    Hist { tile_end: u32, inst: u32, sub: u64 },
    Param { leaf: u32 },
}

const K_NODE: u8 = 2;
const K_STATE: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CKey {
    pos: u32,
    sub: u8,
    occ: u16,
    id: u32,
}

fn unravel_v(mut i: usize, st: &[usize]) -> Vec<usize> {
    st.iter()
        .map(|s| {
            let q = i / s;
            i %= s;
            q
        })
        .collect()
}

fn ravel_v(ix: &[usize], st: &[usize]) -> usize {
    ix.iter().zip(st).map(|(a, b)| a * b).sum()
}

fn broadcast_v(o: &[usize], in_shape: &[usize]) -> usize {
    let ist = strides(in_shape);
    let off = o.len() - in_shape.len();
    (0..in_shape.len()).map(|k| if in_shape[k] == 1 { 0 } else { o[off + k] * ist[k] }).sum()
}

fn product_v(s: &[usize]) -> u64 {
    s.iter().fold(1u64, |a, d| a.saturating_mul(*d as u64))
}

/// A set of disjoint half-open element intervals, built from points and merged.
fn intervals_of(mut v: Vec<u64>) -> Vec<(u64, u64)> {
    v.sort_unstable();
    v.dedup();
    let mut out: Vec<(u64, u64)> = Vec::new();
    for x in v {
        match out.last_mut() {
            Some(last) if last.1 == x => last.1 = x + 1,
            _ => out.push((x, x + 1)),
        }
    }
    out
}

struct Closure<'a> {
    prog: &'a TirProgramV1,
    info: &'a ProgramInfo,
    space: &'a PalwTirStepSpaceV1,
    inv: &'a PalwTirInventoryIndexV1,
    seed: CKey,
    pend: std::collections::BTreeMap<CKey, Vec<u64>>,
    /// Exact leaves, with the consumer kind that first demanded each.
    leaves: std::collections::BTreeMap<CLeaf, (u64, &'static str)>,
    /// Gather over a parameter: (pos, occ, node, param, layer+1) -> index element -> data rest offsets.
    gathers: std::collections::BTreeMap<(u32, u16, u16, u16, u16), std::collections::BTreeMap<u64, Vec<u64>>>,
    tokens: BTreeSet<u32>,
    shapes: HashMap<(u8, usize), Rc<Vec<Vec<usize>>>>,
    max_leaf: u64,
    uncovered: BTreeSet<String>,
}

#[derive(Default, Debug)]
struct ClosureSummary {
    /// Exact leaves per consumer kind: (count, bytes).
    exact: std::collections::BTreeMap<&'static str, (u64, u64)>,
    /// Gather-over-parameter row bounds per kind: (leaves, bytes).
    gather: std::collections::BTreeMap<&'static str, (u64, u64)>,
    param_exact: u64,
    tokens: u64,
    uncovered: Vec<String>,
}

impl ClosureSummary {
    fn leaves(&self) -> u64 {
        self.exact.values().map(|v| v.0).sum::<u64>() + self.gather.values().map(|v| v.0).sum::<u64>()
    }
    fn bytes(&self) -> u64 {
        self.exact.values().map(|v| v.1).sum::<u64>() + self.gather.values().map(|v| v.1).sum::<u64>()
    }
}

impl<'a> Closure<'a> {
    fn new(prog: &'a TirProgramV1, info: &'a ProgramInfo, space: &'a PalwTirStepSpaceV1, inv: &'a PalwTirInventoryIndexV1) -> Self {
        // The largest inventory leaf bounds every gather row bound.
        let max_leaf = (0..inv.leaf_count()).filter_map(|i| inv.piece_of(i).map(|p| p.3 as u64)).max().unwrap_or(1).max(1);
        Self {
            prog,
            info,
            space,
            inv,
            seed: CKey { pos: 0, sub: 0, occ: 0, id: 0 },
            pend: Default::default(),
            leaves: Default::default(),
            gathers: Default::default(),
            tokens: BTreeSet::new(),
            shapes: HashMap::new(),
            max_leaf,
            uncovered: BTreeSet::new(),
        }
    }

    fn occ_block(&self, occ: u16) -> Option<u8> {
        let o = occ as usize;
        let l = self.prog.schedule.layers.len();
        if o == 0 {
            Some(self.prog.schedule.pre)
        } else if o <= l {
            Some(self.prog.schedule.layers[o - 1])
        } else if o == l + 1 {
            Some(self.prog.schedule.post)
        } else {
            None
        }
    }

    fn layer_of(&self, occ: u16) -> Option<u16> {
        let l = self.prog.schedule.layers.len();
        (occ >= 1 && (occ as usize) <= l).then(|| occ - 1)
    }

    fn shapes_at(&mut self, block: u8, pos: u32) -> Result<Rc<Vec<Vec<usize>>>, String> {
        let h = history_length_v1(self.info, block, pos).ok_or("the info does not cover the block")?;
        if let Some(s) = self.shapes.get(&(block, h)) {
            return Ok(s.clone());
        }
        let s: Rc<Vec<Vec<usize>>> = Rc::new(self.prog.blocks[block as usize].nodes.iter().map(|n| n.out.resolve(h)).collect());
        self.shapes.insert((block, h), s.clone());
        Ok(s)
    }

    fn push(&mut self, key: CKey, idx: u64) {
        self.pend.entry(key).or_default().push(idx);
    }

    fn seed_node(&mut self, pos: u32, occ: u16, node: u16, elems: Vec<u64>) {
        self.seed = CKey { pos, sub: K_NODE, occ, id: node as u32 };
        self.pend.entry(self.seed).or_default().extend(elems);
    }

    /// Route one operand element (`r` at element `idx`, read from context `(pos, occ)`).
    fn route(&mut self, pos: u32, occ: u16, r: Ref, idx: u64, kind: &'static str) -> Result<(), String> {
        match r {
            Ref::Node(j) => self.route_node(pos, occ, j as u16, idx, kind),
            Ref::CarryIn(k) => {
                let (c, n) = carry_in_node_v1(self.prog, DemandContext { pos, occurrence: occ }, k).ok_or("carry-in names no node")?;
                self.route_node(c.pos, c.occurrence, n, idx, kind)
            }
            Ref::Param(j) => {
                let d = &self.prog.params[j as usize];
                let layer = if d.per_layer { self.layer_of(occ) } else { None };
                let byte = idx * d.dtype.width() as u64;
                let leaf = self.inv.leaf_of(j, layer, byte).ok_or("a param element without an inventory leaf")?;
                let bytes = self.inv.piece_of(leaf).ok_or("no piece")?.3 as u64;
                self.leaves.entry(CLeaf::Param { leaf }).or_insert((bytes, kind));
                Ok(())
            }
            Ref::Const(_) => Ok(()),
            Ref::State(j) => {
                let layer = if self.prog.states[j as usize].per_layer { self.layer_of(occ) } else { None };
                let inst = state_occurrence_v1(self.prog, j, layer).ok_or("no state instance")?;
                self.push(CKey { pos, sub: K_STATE, occ: inst, id: j as u32 }, idx);
                Ok(())
            }
            Ref::Input(j) => {
                if j == INPUT_TOKEN {
                    self.tokens.insert(pos);
                }
                Ok(())
            }
        }
    }

    /// Route one element of node `node` of context `(pos, occ)`: a commit tile is a leaf (unless it is the seed), anything else
    /// is expanded.
    fn route_node(&mut self, pos: u32, occ: u16, node: u16, idx: u64, kind: &'static str) -> Result<(), String> {
        let block = self.occ_block(occ).ok_or("no occurrence")?;
        let committed = self.prog.blocks[block as usize].nodes.get(node as usize).ok_or("no node")?.commit;
        let key = CKey { pos, sub: K_NODE, occ, id: node as u32 };
        if committed && key != self.seed {
            let shapes = self.shapes_at(block, pos)?;
            let total = product_v(&shapes[node as usize]);
            let t = self.space.commit_tile_len(block, node).ok_or("a commit node without a tile")? as u64;
            let tile = idx / t;
            let count = total.saturating_sub(tile * t).min(t);
            self.leaves.entry(CLeaf::Commit { pos, occ, node, tile }).or_insert((4 * count, kind));
        } else {
            self.push(key, idx);
        }
        Ok(())
    }

    /// A history row element read by `HistAppend` (row `row_pos`, lane `within`) at context `(pos, occ)`.
    fn route_hist_row(&mut self, pos: u32, occ: u16, state: u16, row_pos: u32, within: u64) -> Result<(), String> {
        let layer = if self.prog.states[state as usize].per_layer { self.layer_of(occ) } else { None };
        let ht = self.space.layout.h_tile;
        let tile_start = row_pos - row_pos % ht;
        let tile_end = tile_start + (ht - 1);
        if tile_end < pos {
            let k = self.space.hist_instance_index(state, layer).ok_or("history without tiles")?;
            let inst = self.space.hist_instances()[k];
            let sub = within / inst.tile_lanes as u64;
            let row_lanes = inst.elements.saturating_sub(sub * inst.tile_lanes as u64).min(inst.tile_lanes as u64);
            self.leaves.entry(CLeaf::Hist { tile_end, inst: k as u32, sub }).or_insert((4 * row_lanes * ht as u64, "HistAppend(tile)"));
            Ok(())
        } else {
            let (c, n) = hist_row_node_v1(self.prog, state, layer, row_pos).ok_or("the row appends nothing")?;
            self.route_node(c.pos, c.occurrence, n, within, "HistAppend(row)")
        }
    }

    fn expand_node(&mut self, key: CKey, elems: Vec<u64>) -> Result<(), String> {
        let (pos, occ, node) = (key.pos, key.occ, key.id as usize);
        let block = self.occ_block(occ).ok_or("no occurrence")?;
        let shapes = self.shapes_at(block, pos)?;
        let h = history_length_v1(self.info, block, pos).ok_or("info")?;
        let prog: &TirProgramV1 = self.prog;
        let n = prog.blocks[block as usize].nodes[node].clone();
        let ost = strides(&shapes[node]);
        let oshape = |r: Ref| -> Vec<usize> {
            match r {
                Ref::Node(j) => shapes[j as usize].clone(),
                Ref::CarryIn(k) => prog.blocks[block as usize].carry_in[k as usize].resolve(h),
                Ref::Param(j) => prog.params[j as usize].shape.iter().map(|d| *d as usize).collect(),
                Ref::Const(j) => prog.consts[j as usize].shape.iter().map(|d| *d as usize).collect(),
                Ref::State(j) => prog.states[j as usize].shape.iter().map(|d| *d as usize).collect(),
                Ref::Input(_) => Vec::new(),
            }
        };
        let name: &'static str = n.prim.name();
        for e in elems {
            let o = unravel_v(e as usize, &ost);
            match &n.prim {
                Prim::Reshape | Prim::Cast | Prim::Clamp { .. } | Prim::Log2Floor | Prim::IntExp | Prim::IntRsqrt | Prim::IntLn
                | Prim::StateWrite { .. } => {
                    self.route(pos, occ, n.inputs[0], e, name)?;
                }
                Prim::Transpose { perm } => {
                    let ish = oshape(n.inputs[0]);
                    let mut j = vec![0usize; o.len()];
                    for (k, p) in perm.iter().enumerate() {
                        j[*p as usize] = o[k];
                    }
                    self.route(pos, occ, n.inputs[0], ravel_v(&j, &strides(&ish)) as u64, name)?;
                }
                Prim::Slice { axis, start } => {
                    let ish = oshape(n.inputs[0]);
                    let mut q = o.clone();
                    q[*axis as usize] += *start as usize;
                    self.route(pos, occ, n.inputs[0], ravel_v(&q, &strides(&ish)) as u64, name)?;
                }
                Prim::Concat { axis } => {
                    let a = *axis as usize;
                    let mut q = o.clone();
                    for r in n.inputs.clone() {
                        let t = oshape(r);
                        if q[a] < t[a] {
                            self.route(pos, occ, r, ravel_v(&q, &strides(&t)) as u64, name)?;
                            break;
                        }
                        q[a] -= t[a];
                    }
                }
                Prim::Broadcast => {
                    let ish = oshape(n.inputs[0]);
                    self.route(pos, occ, n.inputs[0], broadcast_v(&o, &ish) as u64, name)?;
                }
                Prim::Iota { .. } => {}
                Prim::Gather { axis, batch_dims } => {
                    let (a, bd) = (*axis as usize, *batch_dims as usize);
                    let dsh = oshape(n.inputs[0]);
                    let xsh = oshape(n.inputs[1]);
                    let m = xsh.len() - bd;
                    let mut xi_c: Vec<usize> = o[..bd].to_vec();
                    xi_c.extend_from_slice(&o[a..a + m]);
                    let xi = ravel_v(&xi_c, &strides(&xsh)) as u64;
                    self.route(pos, occ, n.inputs[1], xi, "Gather(index)")?;
                    let rest_sh = &dsh[a + 1..];
                    let rest_c: Vec<usize> = o[a + m..].to_vec();
                    let rest = ravel_v(&rest_c, &strides(rest_sh)) as u64;
                    match n.inputs[0] {
                        Ref::Const(_) => {}
                        Ref::Param(j) if a == 0 && bd == 0 => {
                            let layer = if self.prog.params[j as usize].per_layer { self.layer_of(occ).map(|l| l + 1).unwrap_or(0) } else { 0 };
                            self.gathers
                                .entry((pos, occ, node as u16, j, layer))
                                .or_default()
                                .entry(xi)
                                .or_default()
                                .push(rest);
                        }
                        r => {
                            // Worst case over every data row along the gathered axis: enumerated (sound), flagged when large.
                            let rows = dsh[a];
                            if rows as u64 * (rest_sh.iter().map(|d| *d as u64).product::<u64>().max(1)) > 1 << 20 {
                                self.uncovered.insert(format!("gather over a non-parameter operand with {rows} rows"));
                            }
                            for v in 0..rows {
                                let mut di: Vec<usize> = o[..a].to_vec();
                                di.push(v);
                                di.extend_from_slice(&o[a + m..]);
                                self.route(pos, occ, r, ravel_v(&di, &strides(&dsh)) as u64, "Gather(data)")?;
                            }
                        }
                    }
                }
                Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } | Prim::Compare { .. } => {
                    let ash = oshape(n.inputs[0]);
                    let bsh = oshape(n.inputs[1]);
                    self.route(pos, occ, n.inputs[0], broadcast_v(&o, &ash) as u64, name)?;
                    self.route(pos, occ, n.inputs[1], broadcast_v(&o, &bsh) as u64, name)?;
                }
                Prim::Select => {
                    let csh = oshape(n.inputs[0]);
                    self.route(pos, occ, n.inputs[0], broadcast_v(&o, &csh) as u64, name)?;
                    for r in [n.inputs[1], n.inputs[2]] {
                        let sh = oshape(r);
                        self.route(pos, occ, r, broadcast_v(&o, &sh) as u64, name)?;
                    }
                }
                Prim::MatMul => {
                    let xs = oshape(n.inputs[0]);
                    let ys = oshape(n.inputs[1]);
                    let (xr, yr, or) = (xs.len(), ys.len(), o.len());
                    let (m, kk, nn) = (xs[xr - 2], xs[xr - 1], ys[yr - 1]);
                    let _ = m;
                    let (r, c) = (o[or - 2], o[or - 1]);
                    let batch = &o[..or - 2];
                    let xb = broadcast_v(batch, &xs[..xr - 2]);
                    let yb = broadcast_v(batch, &ys[..yr - 2]);
                    let xo = xb * m * kk + r * kk;
                    let yo = yb * kk * nn + c;
                    for t in 0..kk {
                        self.route(pos, occ, n.inputs[0], (xo + t) as u64, name)?;
                        self.route(pos, occ, n.inputs[1], (yo + t * nn) as u64, name)?;
                    }
                }
                Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => {
                    let a = *axis as usize;
                    let xs = oshape(n.inputs[0]);
                    let ist = strides(&xs);
                    for t in 0..xs[a] {
                        let mut q = o.clone();
                        q[a] = t;
                        self.route(pos, occ, n.inputs[0], ravel_v(&q, &ist) as u64, name)?;
                    }
                }
                Prim::TopK { axis, .. } => {
                    let a = *axis as usize;
                    let xs = oshape(n.inputs[0]);
                    let ist = strides(&xs);
                    for t in 0..xs[a] {
                        let mut q = o.clone();
                        q[a] = t;
                        self.route(pos, occ, n.inputs[0], ravel_v(&q, &ist) as u64, name)?;
                    }
                }
                Prim::HistAppend { state } => {
                    let s = &prog.states[*state as usize];
                    let row_len: usize = s.shape.iter().map(|d| *d as usize).product();
                    let (t, within) = (e as usize / row_len, e as usize % row_len);
                    if t + 1 == h {
                        self.route(pos, occ, n.inputs[0], within as u64, name)?;
                    } else {
                        let row_pos = (pos as usize + 1 + t).checked_sub(h).ok_or("row before position 0")? as u32;
                        self.route_hist_row(pos, occ, *state, row_pos, within as u64)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn expand_state(&mut self, key: CKey, elems: Vec<u64>) -> Result<(), String> {
        let (pos, j) = (key.pos, key.id as u16);
        let layer = if self.prog.states[j as usize].per_layer { self.layer_of(key.occ) } else { None };
        let c = self.space.layout.checkpoint_interval;
        let k = self.space.fixed_instance_index(j, layer).ok_or("no checkpoint for this state")?;
        let inst = self.space.fixed_instances()[k];
        for idx in elems {
            if pos == 0 {
                continue;
            }
            if pos % c != 0 {
                if let Some((occ_w, w)) = state_writer_v1(self.prog, j, layer) {
                    self.route_node(pos - 1, occ_w, w, idx, "State(replay)")?;
                    continue;
                }
            }
            // Checkpoint: the nearest multiple of `c` at or below `pos` (none for position 0).
            let mut at = pos;
            while at % c != 0 {
                at -= 1;
            }
            if at == 0 {
                continue;
            }
            let tile = idx / inst.tile_lanes as u64;
            let count = inst.elements.saturating_sub(tile * inst.tile_lanes as u64).min(inst.tile_lanes as u64);
            self.leaves.entry(CLeaf::State { pos: at - 1, inst: k as u32, tile }).or_insert((4 * count, "State(checkpoint)"));
        }
        Ok(())
    }

    fn run(&mut self) -> Result<(), String> {
        while let Some((key, mut v)) = self.pend.pop_last() {
            v.sort_unstable();
            v.dedup();
            if key.sub == K_NODE {
                self.expand_node(key, v)?;
            } else {
                self.expand_state(key, v)?;
            }
        }
        Ok(())
    }

    /// The summary: exact leaves per kind, gather row bounds per kind, token units.
    fn summary(&self) -> ClosureSummary {
        let mut s = ClosureSummary::default();
        for (leaf, (bytes, kind)) in &self.leaves {
            let e = s.exact.entry(kind).or_insert((0, 0));
            e.0 += 1;
            e.1 += bytes;
            if matches!(leaf, CLeaf::Param { .. }) {
                s.param_exact += 1;
            }
        }
        for ((_, _, node, param, _), by_xi) in &self.gathers {
            let w = self.prog.params[*param as usize].dtype.width() as u64;
            let _ = node;
            for rests in by_xi.values() {
                for (a, b) in intervals_of(rests.clone()) {
                    let len_bytes = (b - a) * w;
                    let leaves = len_bytes.div_ceil(self.max_leaf) + 1;
                    let e = s.gather.entry("Gather(data param)").or_insert((0, 0));
                    e.0 += leaves;
                    e.1 += leaves * self.max_leaf;
                }
            }
        }
        s.tokens = self.tokens.len() as u64;
        s.uncovered = self.uncovered.iter().cloned().collect();
        s
    }
}

/// The exact leaf key of an observed leaf (from the real evaluator), in the same vocabulary as [`CLeaf`].
fn observed_key(space: &PalwTirStepSpaceV1, l: &kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafV1) -> CLeaf {
    use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1 as K;
    match l.kind {
        K::Commit { occurrence, block, node, first_element, .. } => {
            let t = space.commit_tile_len(block, node).unwrap_or(1) as u64;
            CLeaf::Commit { pos: l.position, occ: occurrence as u16, node, tile: first_element / t }
        }
        K::State { instance, first_element, .. } => {
            let t = space.fixed_instances()[instance as usize].tile_lanes as u64;
            CLeaf::State { pos: l.position, inst: instance, tile: first_element / t }
        }
        K::HistTile { instance, first_lane, .. } => {
            let t = space.hist_instances()[instance as usize].tile_lanes as u64;
            CLeaf::Hist { tile_end: l.position, inst: instance, sub: first_lane / t }
        }
    }
}

/// **Containment of the demand-driven closure on the fixture classes** (every disputed step leaf of every fixture class, the real
/// evaluator's cone refutation as the reference). For each disputed commit or checkpoint leaf, the closure seeded by that leaf's
/// own tile must contain every step leaf the evaluator carried (set containment), and the evaluator's inventory leaves must not
/// exceed the closure's parameter leaves (exact plus gather row bounds). A history tile is not a seed: its values are its row
/// commits, which are leaves of the closure already. 0 violations required.
#[test]
fn closure_containment_on_fixture_classes() {
    use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1 as K;
    let dir = tempfile::tempdir().unwrap();
    let (mut trials, mut skipped, mut violations, mut observed_total, mut derived_total) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    let mut classes = Vec::new();
    for (name, program, params) in programs() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() {
            continue;
        }
        let program = TirProgramV1::decode_canonical(&tiled(program).encode()).unwrap();
        let lay = layout(&program, 5, 2, 2, 64);
        let path = dir.path().join(format!("{}.palwtir", name.replace(' ', "-")));
        let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
            params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
        };
        misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&lay).unwrap(), [2; 64], name.clone(), &mut tensor)
            .unwrap();
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap());
        let (root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let canonical =
            kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
        let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, LADDER)
            .unwrap()
            .with_network_id_v1(NET);
        let (_, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C; 64])).unwrap();
        let prompt: Vec<u32> = prompt.into_iter().map(|t| t as u32).collect();
        let job = fp_job(&backend, &prompt, 6);
        let ids: Vec<usize> = prompt.iter().map(|t| *t as usize).collect();
        let run = backend.execute_free_prompt(&job, &ids).unwrap();
        let material = run.outcome.material.clone();
        let capture = backend.decode_capture(&material).unwrap();
        let ctx = capture.binding.job_context.clone();
        let n = capture.binding.step_leaf_count;
        drop(capture);
        let space = backend.space();
        let inv = PalwTirInventoryIndexV1::new(&space.program).unwrap();
        let rules = PalwTirCourtRulesV1 { max_step_leaf_count: LADDER, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };
        let (mut class_trials, mut class_skipped, mut class_viol) = (0u64, 0u64, 0u64);
        for i in 0..n {
            let l = space.leaf_at(&ctx, i).unwrap();
            let seed = match l.kind {
                K::Commit { occurrence, block, node, first_element, .. } => {
                    let _ = block;
                    Some((l.position, occurrence as u16, node, first_element))
                }
                K::State { state, layer, first_element, .. } => {
                    state_writer_v1(&space.program, state, layer).map(|(occ, w)| (l.position, occ, w, first_element))
                }
                K::HistTile { .. } => None,
            };
            let Some((pos, occ, node, first)) = seed else {
                skipped += 1;
                class_skipped += 1;
                continue;
            };
            let elems: Vec<u64> = (first..first + l.value_count as u64).collect();
            let mut c = Closure::new(&space.program, &space.info, space, &inv);
            c.seed_node(pos, occ, node, elems);
            c.run().unwrap_or_else(|e| panic!("{name}: leaf {i}: the closure refused: {e}"));
            let sum = c.summary();
            for k in sum.exact.keys().chain(sum.gather.keys()) {
                kinds.insert(k.to_string());
            }
            let r = backend.cone_refutation(&material, i, &rules).unwrap();
            trials += 1;
            class_trials += 1;
            for pre in &r.operands.preimages {
                let li = space.leaf_index(&ctx, &pre.coord).expect("an operand is a leaf of this job");
                let lo = space.leaf_at(&ctx, li).unwrap();
                let key = observed_key(space, &lo);
                observed_total += 1;
                if !c.leaves.contains_key(&key) {
                    violations += 1;
                    class_viol += 1;
                    eprintln!("{name}: seed {:?} misses observed {:?}", (pos, occ, node), key);
                }
            }
            let observed_params = r.params.opened_count() as u64;
            let derived_params = sum.param_exact + sum.gather.values().map(|v| v.0).sum::<u64>();
            if observed_params > derived_params {
                violations += 1;
                class_viol += 1;
                eprintln!("{name}: seed {:?}: {observed_params} param leaves observed, {derived_params} derived", (pos, occ, node));
            }
            derived_total += sum.leaves();
        }
        assert_eq!(class_viol, 0, "{name}: closure containment violated");
        classes.push(json!({ "class": name, "step_leaves": n, "disputed_trials": class_trials, "skipped_history_or_no_writer": class_skipped, "violations": class_viol }));
    }
    assert!(trials > 0 && observed_total > 0, "no fixture class was checked");
    println!(
        "[closure-containment] trials={trials} skipped={skipped} observed_leaves={observed_total} derived_leaves_sum={derived_total} violations={violations} kinds={kinds:?}"
    );
    assert_eq!(violations, 0, "the closure misses an operand the real evaluator reads");
    if let Ok(out) = std::env::var("MISAKA_CLOSURE_CONTAINMENT_OUT") {
        let record = json!({ "trials": trials, "skipped": skipped, "observed_leaves": observed_total, "derived_leaves_sum": derived_total,
            "violations": violations, "consumer_kinds_exercised": kinds, "classes": classes });
        std::fs::write(out, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    }
}

/// **The closure of disputed steps at the 32K maximum, Huihui-Qwen3.5-2B class** (params-free: the class and its program only; no
/// tensor is read). Seeds: every commit node of the top GDN layer, the top attention layer and `post`, at position `max_context−1`,
/// for the first, middle and last tile of each node. Measurement of sampled seeds (see the record), not a maximum over all tiles.
/// Run: `MISAKA_TIR_FRAME_MEASURE_PALWTIR=<file> MISAKA_CLOSURE_OUT=<json> cargo test --release -p misaka-palw-tir-exec --features node
///  --test frame_measure closure_bound_on_a_real_class -- --ignored --nocapture`.
#[test]
#[ignore]
fn closure_bound_on_a_real_class() {
    let path = std::path::PathBuf::from(std::env::var("MISAKA_TIR_FRAME_MEASURE_PALWTIR").expect("a PALWTIR1 file"));
    let artifact = TirArtifactV1::open(&path).unwrap();
    let class = artifact.class().unwrap();
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let inv = PalwTirInventoryIndexV1::new(&space.program).unwrap();
    let pos = space.layout.max_context - 1;
    let layers = space.program.schedule.layers.len() as u16;
    let mut occs: Vec<u16> = Vec::new();
    // The top GDN layer, the top attention layer, and post.
    let top_gdn = (0..layers).rev().find(|l| space.program.schedule.layers[*l as usize] == space.program.schedule.layers[0]).unwrap();
    let top_attn = (0..layers).rev().find(|l| space.info.blocks[space.program.schedule.layers[*l as usize] as usize].window.is_some());
    if let Some(l) = top_attn {
        occs.push(l + 1);
    }
    occs.push(top_gdn + 1);
    occs.push(layers + 1);
    let mut record = Vec::new();
    let (mut best_total, mut best_seed) = (0u64, String::new());
    let mut per_kind_max: std::collections::BTreeMap<String, u64> = Default::default();
    let started = Instant::now();
    for occ in occs {
        let block = if occ == layers + 1 { space.program.schedule.post } else { space.program.schedule.layers[(occ - 1) as usize] };
        let nodes = space.program.blocks[block as usize].nodes.len();
        for node in 0..nodes {
            if !space.program.blocks[block as usize].nodes[node].commit {
                continue;
            }
            let t = space.commit_tile_len(block, node as u16).unwrap() as u64;
            let total_lanes = {
                let h = history_length_v1(&space.info, block, pos).unwrap();
                space.program.blocks[block as usize].nodes[node].out.resolve(h).iter().map(|d| *d as u64).product::<u64>()
            };
            let tiles = total_lanes.div_ceil(t);
            for tile in [0, tiles / 2, tiles - 1] {
                let first = tile * t;
                let count = total_lanes.saturating_sub(first).min(t);
                let mut c = Closure::new(&space.program, &space.info, &space, &inv);
                c.seed_node(pos, occ, node as u16, (first..first + count).collect());
                let t0 = Instant::now();
                c.run().unwrap();
                let s = c.summary();
                let kind_bytes: serde_json::Map<String, Value> = s
                    .exact
                    .iter()
                    .chain(s.gather.iter())
                    .map(|(k, v)| (k.to_string(), json!({ "leaves": v.0, "bytes": v.1 })))
                    .collect();
                for (k, v) in s.exact.iter().chain(s.gather.iter()) {
                    let e = per_kind_max.entry(k.to_string()).or_insert(0);
                    *e = (*e).max(v.1);
                }
                if s.bytes() > best_total {
                    best_total = s.bytes();
                    best_seed = format!("occ {occ} block {block} node {node} tile {tile}");
                }
                record.push(json!({ "occurrence": occ, "block": block, "node": node, "tile": tile, "tile_len": t,
                    "leaves": s.leaves(), "bytes": s.bytes(), "tokens": s.tokens, "param_exact": s.param_exact,
                    "by_kind": kind_bytes, "uncovered": s.uncovered, "seconds": t0.elapsed().as_secs_f64() }));
            }
        }
    }
    let out = json!({
        "record": "misaka-palw/tir-closure-sampled/v1", "artifact": path.display().to_string(), "position": pos,
        "seeds": record.len(), "max_sampled_bytes": best_total, "max_sampled_seed": best_seed,
        "max_sampled_bytes_per_kind": per_kind_max, "wall_seconds": started.elapsed().as_secs_f64(), "samples": record,
    });
    println!("[closure-real] seeds={} max_sampled_bytes={best_total} at {best_seed}", out["seeds"]);
    if let Ok(o) = std::env::var("MISAKA_CLOSURE_OUT") {
        std::fs::write(o, serde_json::to_string_pretty(&out).unwrap()).unwrap();
    }
}
