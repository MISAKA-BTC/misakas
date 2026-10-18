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
    // ADR-0190 bounded-round door (DORMANT; measurement only). The gather / model-parameter bound is not derived yet, so the path
    // must refuse without it. The door at a gather value is a SENSITIVITY line, not a bound.
    assert!(
        fc::tir_public_localization_path_bounded_v1(&source, 6, &cfg, &clk, None).is_err(),
        "the bounded door refuses an unbounded gather term"
    );
    let fenced_at = |gather: u64| -> serde_json::Value {
        let p = fc::tir_public_localization_path_bounded_v1(&source, 6, &cfg, &clk, Some(gather)).unwrap();
        let door = door_of(6, p.daa, queue6, fm6, prep6);
        json!({
            "gather_bytes_sensitivity_only": gather,
            "door_daa": door,
            "fits_3120": door <= 3_120,
            "turns": p.turns,
            "path_daa": p.daa,
            "round1_body_bytes": p.round1_body,
            "round2_body_bytes": p.round2_body,
            "round1_daa": p.round1_daa,
            "round2_daa": p.round2_daa,
            "output_check_daa": p.output_check_daa,
            "restart_daa": p.restart_daa,
            "cone_runs": p.cone_runs,
        })
    };
    let fenced = json!({ "serial_cone_rounds": fc::PALW_TIR_BOUNDED_CONE_ROUNDS_V1, "at_zero_gather_sensitivity": fenced_at(0) });
    println!("[door-breakdown-bounded] {}", fenced);
    let horizon = t12_horizon_daa();
    let record = json!({
        "fenced_bounded_round_door": fenced,
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
