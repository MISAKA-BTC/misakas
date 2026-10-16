//! **The TIR public frame's operations, measured on the corpus classes** (feature `node`; audit §10, the frame pricing proposal
//! `docs/design/palw/tir-frame-pricing-proposal-2026-10-16.md`).
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
    PalwTirPublicAllocationQuoteV1, PalwTirPublicAllocationRulesV1, derive_tir_public_allocation_v1,
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

/// The frame's native rules at testnet-12's ladder and staged config (`network_v1`), at a custodian upper of `custodians`.
fn rules(custodians: u64) -> PalwTirPublicAllocationRulesV1 {
    PalwTirPublicAllocationRulesV1 {
        maximum_leaves: 1 << 22,
        maximum_custodians: custodians,
        court: PalwCourtParamsV2::new(1 << 22, 45, 2).unwrap(),
        dissection: true,
        prompt_form: PalwPromptIdsFormV1::Flat,
        config: kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
    }
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
    tparams: TirParams,
    class: PalwTirClassV1,
    artifact_root: Hash64,
    ctx: kaspa_consensus_core::palw_v2::PalwJobContextV2,
    prompt: Vec<u32>,
    inventory: TirHeldInventoryV1,
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
    let tparams = TirParams::from_map(&plan, params).unwrap();
    let inventory = TirHeldInventoryV1::build(program, &TirParamsSourceV1(&tparams)).unwrap();
    Court { space, plan, tparams, class, artifact_root, ctx, prompt, inventory }
}

/// The measurements of one class (deterministic counts and sizes, and this host's nanoseconds).
fn measure(name: &str, backend: &TirBackendV1, program: &TirProgramV1, params: &MapParams, k: usize) -> Value {
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
        Ok(PalwTirPublicAllocationQuoteV1::Complete(plan)) => plan,
        other => panic!("{name}: no complete frame at {upper}: {other:?}"),
    };
    let plan6 = quote(6);
    let plan257 = quote(257);
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
    for request in &requests {
        let (response, slices, took, largest) = answer(reader.as_mut(), request);
        cold_slices += slices;
        cold_ns += took;
        cold_slice_max = cold_slice_max.max(largest);
        let bytes = borsh::to_vec(&response).unwrap().len() as u64;
        assert!(bytes <= unit, "{name}: an answer ({bytes} bytes) fits the frame's unit bound ({unit})");
        answer_bytes_max = answer_bytes_max.max(bytes);
        let start = Instant::now();
        let unit_auth = source.authenticate_v1(request, &response, ids.as_ref()).unwrap();
        let took = ns(start);
        auth_ns += took;
        auth_max = auth_max.max(took);
        if let kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicAuthenticatedUnitV1::Ids(authenticated) = unit_auth {
            ids = Some(authenticated);
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
    // The allocation's bounds hold for what ran: the generation fits `prepared_disk_bytes`, its slices the per-pass bound.
    assert!(descriptor.retained_bytes <= plan6.executor.prepared_disk_bytes, "{name}: the generation fits the executor's bound");
    assert!(descriptor.retained_bytes <= plan6.owner_work.prepared_bytes, "{name}: and the work allocation's");
    let per_pass_slices = plan6.preparation.maximum_slices_per_actor / passes;
    assert!(publish_slices <= per_pass_slices, "{name}: {publish_slices} publication slices within the per-pass bound {per_pass_slices}");
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

    // --- per-step check: the owner's replayed refutation and the court's check, every leaf --------------------------------------
    let c = court(program, params);
    let runner = TirClassRunnerV1::new(&c.space, &c.plan, &c.tparams, c.class.class_id(&c.artifact_root)).unwrap();
    let court_rules = PalwTirCourtRulesV1 { max_step_leaf_count: LADDER, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };
    let start = Instant::now();
    let honest = runner.retain(&c.class, c.artifact_root, &c.ctx, &c.prompt, LADDER).unwrap();
    let retain_ns = ns(start);
    let mut dense = Vec::new();
    let start = Instant::now();
    runner.run(&c.ctx, &c.prompt, LADDER, true, &mut |l| dense.push(l.preimage.clone())).unwrap();
    let replay_pass_ns = ns(start);
    let own = TirEvidenceV1::own(&runner, &honest, &c.inventory, PalwPromptIdsFormV1::Flat, LADDER).unwrap();
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
            &c.inventory,
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
        "preparation": {
            "publish_slices": publish_slices, "publish_ns": publish_ns, "publish_slice_max_ns": publish_slice_max,
            "publish_io_bytes": publish_io, "retained_bytes": descriptor.retained_bytes, "quoted_publish_slices": need.maximum_publish_slices,
            "warm_open_ns": warm_open_ns, "warm_leaf_answers": n, "warm_slices": warm_slices, "warm_ns": warm_ns,
        },
        "step_check": {
            "leaves": court_n, "retain_ns": retain_ns, "replay_pass_ns": replay_pass_ns,
            "owner_refutation_build_max_ns": build_max, "owner_refutation_build_total_ns": build_total,
            "court_check_max_ns": check_max, "court_check_total_ns": check_total, "refutation_bytes_max": refutation_bytes_max,
        },
        "filing": filing,
        "frame": [frame(&plan6), frame(&plan257)],
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
        classes.push(measure(&name, &backend, &program, &params, k));
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
