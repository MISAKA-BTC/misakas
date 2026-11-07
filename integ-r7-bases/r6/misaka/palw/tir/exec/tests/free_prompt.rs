//! **RFC-0002 Phase H on the node: the IR backend's free-prompt run** (feature `node`). Each tiny tiled class is written as a
//! PALWTIR1 container, opened mapped and served by [`TirBackendV1`]; a free-prompt job is run by the integer engine under its own
//! FP context and assembled by the producer's `palw_fp_commitment_from_context_v3`. What the consensus side then requires of it is
//! checked with the consensus functions themselves: the frame's full allocation (`derive_tir_public_allocation_v1`, which binds the
//! header, roots, count and leaves to the original), the lane's derived leaves and credit (`palw_tir_fp_lane_v1`), and the
//! frame's decode-token door under the job's own rule (honest: no fault; the selection drill: convicted).

#![cfg(feature = "node")]

mod node_common;

use std::sync::Arc;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_fp_execution_v3::{palw_fp_commitment_from_context_v3, palw_fp_job_pin_v1};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFpPrefixStateV1, PalwFpStopReasonV3,
    PalwFreePromptJobV3,
};
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_commitment_v1};
use kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirSelectionRuleV1, check_tir_decode_token_tiled_rule_v1};
use kaspa_consensus_core::palw_tir_public_allocation_v1::{
    PalwTirPublicAllocationQuoteV1, PalwTirPublicAllocationRulesV1, derive_tir_public_allocation_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir_exec::node::{TirArtifactV1, TirBackendV1};
use node_common::{layout, programs, tiled};

const NET: &[u8] = b"testnet-12";
const LADDER: u64 = 1 << 26;

/// Every admissible tiny program under the tiled scheme, served by an IR backend that knows its network.
fn backends(tag: &str) -> Vec<(String, TirBackendV1)> {
    let dir = std::env::temp_dir().join(format!("tir-exec-fp-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut out = Vec::new();
    for (name, program, params) in programs() {
        if analyze_ranges(&program).is_err() || program.params.is_empty() {
            continue;
        }
        let program = TirProgramV1::decode_canonical(&tiled(program).encode()).unwrap();
        let lay = layout(&program, 5, 2, 2, 64);
        let path = dir.join(format!("{}.palwtir", name.replace(' ', "-")));
        let mut tensor = |j: u16, l: Option<u16>| -> Result<Vec<u8>, String> {
            params.tensors.get(&(j, l)).map(|t| t.to_le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
        };
        misaka_palw_tir_artifact::write_container_v1(&path, &program, borsh::to_vec(&lay).unwrap(), [2; 64], name.clone(), &mut tensor)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let artifact = Arc::new(TirArtifactV1::open(&path).unwrap_or_else(|e| panic!("{name}: {e}")));
        let (root, _) = artifact.inventory_root().unwrap();
        let class = artifact.class().unwrap();
        let canonical =
            kaspa_consensus_core::palw_tir_attempt_v1::palw_tir_canonical_context_v1(&class, class.class_id(&root), (4, 3)).unwrap();
        let backend = TirBackendV1::new(name.clone(), artifact, root, canonical, PalwPromptIdsFormV1::Flat, LADDER)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .with_network_id_v1(NET);
        out.push((name, backend));
    }
    assert!(out.len() >= 4, "the tiny program set serves several tiled IR classes");
    out
}

/// A user free-prompt job of `backend`'s class over `prompt`, at `decode` tokens, greedy.
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

fn prompt_of(backend: &TirBackendV1, k: usize) -> Vec<u32> {
    let (_, prompt) = backend.job_for_anchor(Hash64::from_bytes([0x3C ^ k as u8; 64])).unwrap();
    prompt.into_iter().map(|t| t as u32).collect()
}

fn usize_ids(ids: &[u32]) -> Vec<usize> {
    ids.iter().map(|t| *t as usize).collect()
}

/// The frame's native rules at testnet-12's step ladder (`2^22`) and the network's staged config, for a native custodian upper of
/// `custodians` (a network resolves it from its Panel's seat count plus the producer; with `palw_tir_shard_v1` armed, as on
/// testnet-12, a registered TIR class's is a stratified Panel's: 257 — `tir_frame_allocation_holds_at_testnet12s_custodian_upper`).
fn rules(custodians: u64) -> PalwTirPublicAllocationRulesV1 {
    PalwTirPublicAllocationRulesV1 {
        maximum_leaves: 1 << 22,
        maximum_custodians: custodians,
        court: PalwCourtParamsV2::new(1 << 22, 45, 2).unwrap(),
        dissection: true,
        prompt_form: PalwPromptIdsFormV1::Flat,
        config: kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1(),
        // The allocation's shape, not its prices, is under test here: the TEST-ONLY clock (audit §10).
        clock: kaspa_consensus_core::palw_tir_frame_cost_v1::palw_tir_frame_test_only_cost_v1().clock,
    }
}

/// **The honest run is the frame's original, and its selection is the door's rule.** The commitment the producer assembles is
/// accepted by the frame's full allocation over the run's own header (roots, count, leaves bound to the original); its leaves are
/// the lane's derived leaves; its credit is positive; and the decode-token door finds no fault at any position.
#[test]
fn tir_free_prompt_run_is_a_frame_original_whose_work_the_lane_derives() {
    let mut framed = Vec::new();
    for (k, (name, backend)) in backends("honest").into_iter().enumerate() {
        let prompt = prompt_of(&backend, k);
        let job = fp_job(&backend, &prompt, 2);
        let mut streamed = Vec::new();
        let run = backend.execute_free_prompt_streaming(&job, &usize_ids(&prompt), &mut |id| streamed.push(id)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(streamed, run.output_token_ids, "{name}: the streamed ids are the committed run's");
        let shape = backend.capture_shape(&run.outcome.material).expect("an IR capture has a shape");
        let commitment = palw_fp_commitment_from_context_v3(&job, &shape.job_context, &run, 999_999)
            .unwrap_or_else(|e| panic!("{name}: the producer's assembly: {e:?}"));
        assert_eq!(commitment.stop_reason, PalwFpStopReasonV3::ExactBudgetReached);
        let PalwPublicExecutionBindingHeaderV1::Tir(header) = backend.public_execution_binding_header_v1(&run.outcome.material).unwrap()
        else {
            panic!("{name}: an IR capture exports a TIR header");
        };
        let (record, _) =
            kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(backend.class(), &backend.artifact_root()).unwrap();
        // The lane's derivation: the capture's own leaves, and a positive credit for the decode positions (D10: prompt accounted).
        let class = kaspa_consensus_core::palw_tir_fp_lane_v1::palw_tir_fp_class_v1(&record, &backend.class().layout).unwrap();
        let work = kaspa_consensus_core::palw_tir_fp_lane_v1::palw_tir_fp_derive_work_v1(
            &class,
            job.prompt_tokens,
            commitment.decode_tokens_executed,
            job.prompt_tokens,
            PalwFpPrefixStateV1::genesis(job.class_id),
            LADDER,
            false,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(work.derived.total_leaves, commitment.work_leaves, "{name}: the derived leaves are the capture's");
        assert!(work.credited_compute > 0, "{name}: the decode positions earn");
        // The frame's full allocation over this exact original and header (what the fold re-derives at acceptance).
        let geometry = kaspa_consensus_core::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
            class_id: job.class_id,
            producer: PalwBondKeyV2(job.executor_bond),
            execution_root: commitment.execution_root,
            job_identity: palw_fp_job_pin_v1(&commitment),
            accepted_daa: 100,
        };
        // testnet-12's custodian upper (a five-seat Panel and the producer): the frame's resource arithmetic holds there for every
        // corpus class (`tir_frame_allocation_holds_at_testnet12s_custodian_upper`, which also holds it at testnet-12's 257).
        let quote = derive_tir_public_allocation_v1(&geometry, &commitment, &header, &record, &backend.class().layout, None, None, rules(6))
            .unwrap_or_else(|e| panic!("{name}: the frame's allocation: {e}"));
        assert!(matches!(quote, PalwTirPublicAllocationQuoteV1::Complete(_)), "{name}: a complete frame plan");
        framed.push(name.clone());
        // The decode-token door under the job's own rule: no committed token is beaten by the runner-up lane.
        let rule = PalwTirSelectionRuleV1::of_job_v1(&job).unwrap();
        let capture = backend.decode_capture(&run.outcome.material).unwrap();
        let court = backend.court_rules(&PalwCourtParamsV2::new(LADDER, 4, 2).unwrap());
        for row in 0..commitment.decode_tokens_executed {
            let before = &capture.generated[..row as usize];
            let committed = capture.generated[row as usize];
            let beat = rule.select_v1(before, &capture.logits_rows[row as usize], Some(committed)).unwrap();
            let pin = backend.decode_token_pin(&run.outcome.material, row, beat).unwrap();
            assert!(check_tir_decode_token_tiled_rule_v1(&capture.binding, &pin, &court, &rule).is_err(), "{name}: honest row {row}");
        }
    }
    eprintln!("[tir-fp] complete frame originals (6 custodians): {framed:?}");
}

/// **The selection drill (D-06) is convicted by the door; the leaf drill (D-05) commits a self-consistent lie.** The committed
/// token at decode position 0 is the best lane the rule does not choose: the door, pinned at that row against the rule's own lane,
/// convicts. The leaf drill's commitment still assembles (its roots are the corrupted capture's) — what an outsider's cone convicts.
#[test]
fn tir_free_prompt_drills_commit_lies_the_frame_doors_see() {
    let mut convicted = 0usize;
    for (k, (name, backend)) in backends("drill").into_iter().enumerate() {
        let prompt = prompt_of(&backend, k);
        let job = fp_job(&backend, &prompt, 2);
        let honest = backend.execute_free_prompt(&job, &usize_ids(&prompt)).unwrap();
        let lie = backend.execute_free_prompt_with_selection_fault_v1(&job, &usize_ids(&prompt), 0).unwrap();
        let capture = backend.decode_capture(&lie.outcome.material).unwrap();
        let rule = PalwTirSelectionRuleV1::of_job_v1(&job).unwrap();
        let chosen = rule.select_v1(&[], &capture.logits_rows[0], None).unwrap();
        if chosen == capture.generated[0] {
            // A one-lane-admissible row has no runner-up: nothing to lie with at this position.
            continue;
        }
        assert_ne!(lie.outcome.execution_root, honest.outcome.execution_root, "{name}");
        let court = backend.court_rules(&PalwCourtParamsV2::new(LADDER, 4, 2).unwrap());
        let pin = backend.decode_token_pin(&lie.outcome.material, 0, chosen).unwrap();
        let verdict = check_tir_decode_token_tiled_rule_v1(&capture.binding, &pin, &court, &rule);
        assert!(verdict.is_ok(), "{name}: the selection lie is convicted at its row: {verdict:?}");
        convicted += 1;
        let shape = backend.capture_shape(&lie.outcome.material).unwrap();
        palw_fp_commitment_from_context_v3(&job, &shape.job_context, &lie, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let forged = backend.execute_free_prompt_with_injected_fault(&job, &usize_ids(&prompt), 1).unwrap();
        assert_ne!(forged.outcome.execution_root, honest.outcome.execution_root, "{name}: the leaf lie moves the root");
        let shape = backend.capture_shape(&forged.outcome.material).unwrap();
        palw_fp_commitment_from_context_v3(&job, &shape.job_context, &forged, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
    eprintln!("[tir-fp] selection lies convicted by the door: {convicted}");
    assert!(convicted >= 4, "the selection drill is exercised on several classes");
}

/// **A V4 job stops where its decoder stops** (a stop sequence on the first token): the run is re-executed at exactly that count,
/// its context names it (`EndOfGeneration`), and the frame's allocation and the lane's leaves agree with the shorter run.
#[test]
fn tir_free_prompt_v4_stop_runs_to_exactly_the_decoders_stop() {
    for (k, (name, backend)) in backends("stop").into_iter().enumerate() {
        let prompt = prompt_of(&backend, k);
        let greedy = backend.execute_free_prompt(&fp_job(&backend, &prompt, 3), &usize_ids(&prompt)).unwrap();
        let first = greedy.output_token_ids[0];
        let config = kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4 {
            stop_sequences: vec![vec![first]],
            ..kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4::NOOP
        };
        let job = fp_job(&backend, &prompt, 3).into_v4(config);
        let run = backend.execute_free_prompt(&job, &usize_ids(&prompt)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(run.output_token_ids, vec![first], "{name}: the stop sequence ends the answer on its first token");
        assert_eq!(run.facts.decode_tokens_executed, 1);
        assert_eq!(run.facts.stop_reason, PalwFpStopReasonV3::EndOfGeneration);
        let shape = backend.capture_shape(&run.outcome.material).unwrap();
        let commitment = palw_fp_commitment_from_context_v3(&job, &shape.job_context, &run, 0).unwrap();
        let (record, _) =
            kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(backend.class(), &backend.artifact_root()).unwrap();
        let class = kaspa_consensus_core::palw_tir_fp_lane_v1::palw_tir_fp_class_v1(&record, &backend.class().layout).unwrap();
        let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&class).unwrap();
        assert_eq!(
            kaspa_consensus_core::palw_tir_fp_lane_v1::palw_tir_fp_leaf_count_v1(&space, job.prompt_tokens, 1, LADDER).unwrap(),
            commitment.work_leaves,
            "{name}"
        );
    }
}

/// **The frame's native allocation holds at testnet-12's custodian upper** (the gap this lane pinned, inverted by the TIR frame
/// lane's fix of 2026-10-16). testnet-12 arms `palw_tir_shard_v1`, so the upper its registry fold resolves for a registered TIR
/// class is a stratified Panel's — `MAX_STRATIFIED_SEATS_V1` (256) seats plus the producer, 257 — not its five-seat V2 Panel plus
/// the producer (6). The upper is taken from the fold's own resolver over the shipped bundle, not restated. Under the network's
/// staged config and ladder (`2^22`), every corpus class is a complete frame original at every upper from 1 to 6, at 33 and at 257.
///
/// The root cause was the work allocation (`derive_tir_public_work_allocation_v1`): every lane was priced as the outside owner's
/// (which reads every slot's answers, so it grows with the slots) and the claim-wide total was that times the slots — a product
/// that is a sum (one owner lane plus one literal custodian lane per custodian; the claim-wide preparation holds custodian lanes
/// only); the per-lane work was multiplied by every actor's preparations (`slots × passes`) as well; the acquisition's storage
/// envelope was re-read as answer bytes; and the 16 KiB WorkDebit slices were counted over the replay's RAM traffic and over the
/// passes twice. With each full-frame validation (`frame ∝ custodians`) charged per accepting action, that made the SUM cubic in
/// the upper; it is now quadratic, as `original_controls` states.
#[test]
fn tir_frame_allocation_holds_at_testnet12s_custodian_upper() {
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_consensus_core::palw_model_registry_v1::{PalwModelRegistryFoldV1, palw_registry_globals_of_bundle_v1};
    use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, PalwTransitionExtrasV1};
    let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    assert_eq!(t12.net, NetworkId::with_suffix(NetworkType::Testnet, 12));
    assert!(t12.palw_tir_shard_v1.is_some(), "testnet-12 arms the layer-sharded (stratified) Panel for IR classes");
    assert!(t12.palw_shard_licensing.is_none(), "shard licensing (a ×1024 upper) is dormant on testnet-12");
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {
        panic!("testnet-12 is ConsensusV2")
    };
    let panel_upper = u64::from(bundle.panel.seat_count()) + 1;
    assert_eq!(panel_upper, 6, "a five-seat V2 Panel and the producer");
    // The production resolver (`palw_tir_public_allocation_rules_at` → `staged_registered_tir_maximum_custodians_v1`) over the
    // shipped bundle's state params and a registry fold with the network's own globals, at the planned fence (DAA 14,000).
    let registry = PalwModelRegistryFoldV1 {
        globals: palw_registry_globals_of_bundle_v1(bundle),
        span_daa: 1,
        genesis_works: Default::default(),
        grace_until_daa: 0,
        admission_audit_period_daa: None,
        readiness_v2_active: false,
        bond_maturity: None,
    };
    let extras = PalwTransitionExtrasV1 { model_registry: Some(registry), ..Default::default() };
    let t12_upper = PalwChainStateV2::staged_registered_tir_maximum_custodians_v1(14_000, &bundle.state, &extras).unwrap();
    assert_eq!(t12_upper, 257, "a stratified Panel's 256 seats and the producer");
    // G14 audit §11 (d): a FRAME's custodians are only its flat Panel's credited seats and the producer (no typed licence reaches a
    // frame), so the frame route is priced and clocked at 6 on testnet-12, not at the typed routes' 257.
    let frame_upper =
        kaspa_consensus_core::palw_tir_frame_cost_v1::palw_tir_frame_custodian_upper_v1(14_000, &bundle.state, &extras).unwrap();
    assert_eq!(frame_upper, panel_upper, "the frame's upper is the flat Panel and the producer");
    let mut record_line = Vec::new();
    let mut classes = 0usize;
    for (k, (name, backend)) in backends("holds").into_iter().enumerate() {
        let prompt = prompt_of(&backend, k);
        let job = fp_job(&backend, &prompt, 2);
        let run = backend.execute_free_prompt(&job, &usize_ids(&prompt)).unwrap();
        let shape = backend.capture_shape(&run.outcome.material).unwrap();
        let commitment = palw_fp_commitment_from_context_v3(&job, &shape.job_context, &run, 999_999).unwrap();
        let PalwPublicExecutionBindingHeaderV1::Tir(header) =
            backend.public_execution_binding_header_v1(&run.outcome.material).unwrap()
        else {
            panic!("{name}: an IR capture exports a TIR header");
        };
        let (record, _) =
            kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(backend.class(), &backend.artifact_root()).unwrap();
        let geometry = kaspa_consensus_core::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
            class_id: job.class_id,
            producer: PalwBondKeyV2(job.executor_bond),
            execution_root: commitment.execution_root,
            job_identity: palw_fp_job_pin_v1(&commitment),
            accepted_daa: 100,
        };
        for custodians in (1..=panel_upper).chain([33, t12_upper]) {
            match derive_tir_public_allocation_v1(
                &geometry,
                &commitment,
                &header,
                &record,
                &backend.class().layout,
                None,
                None,
                rules(custodians),
            ) {
                Ok(PalwTirPublicAllocationQuoteV1::Complete(plan)) => {
                    assert_eq!(plan.acquisition.maximum_custodians, custodians, "{name}");
                    // The claim-wide totals are the lanes' SUM; the preparation holds custodian lanes only.
                    assert_eq!(plan.owner_work.maximum, plan.owner_work.lanes_sum_v1().unwrap(), "{name}");
                    assert_eq!(plan.preparation_work.per_actor, plan.owner_work.per_custodian, "{name}");
                    if custodians == panel_upper || custodians == t12_upper {
                        record_line.push(format!(
                            "{name}: n={} upper={custodians} parser={:.2e} rent={:.2e} preparation_until=+{:.2e} collect_until=+{:.2e}",
                            commitment.work_leaves,
                            plan.terms.plan.maximum_parser_steps as f64,
                            plan.terms.plan.rent_sompi as f64,
                            (plan.preparation_until_daa - 100) as f64,
                            (plan.terms.collect_until_daa - 100) as f64
                        ));
                    }
                }
                other => panic!("{name}: no complete frame at custodian upper {custodians}: {other:?}"),
            }
        }
        classes += 1;
    }
    assert_eq!(classes, 11, "every corpus class");
    eprintln!("[tir-fp frame at testnet-12's uppers] {record_line:#?}");
}

/// **What the lane does not judge, the backend does not run**: a backend with no network id, and a flat-scheme class.
#[test]
fn tir_free_prompt_refuses_outside_the_lane() {
    let (name, backend) = backends("refuse").into_iter().next().unwrap();
    let prompt = prompt_of(&backend, 0);
    let job = fp_job(&backend, &prompt, 2);
    let (_, bare) = backends("bare").into_iter().find(|(n, _)| *n == name).unwrap();
    let bare = bare.with_network_id_v1(&[]);
    assert!(bare.execute_free_prompt(&job, &usize_ids(&prompt)).is_err(), "no network id, no context");
    let mut other = job.clone();
    other.class_id = Hash64::from_u64_word(1);
    assert!(backend.execute_free_prompt(&other, &usize_ids(&prompt)).is_err(), "another class's job");
}


/// **Audit 02-B: the registered runtime semantics reach the run.** A class registered with a non-default model profile, runtime
/// manifest and class, and CU ruleset (`PalwClassSemanticsV1`): a backend left at its defaults builds a context the registered
/// statement does not match (the frame refuses it: `matches_context`), and the same backend after the node's
/// `set_registered_class_facts_v1` builds one it does — in the run's own capture shape AND its exported public header — while
/// the class id stays the class's whatever is passed.
#[test]
fn tir_free_prompt_contexts_carry_the_registered_semantics_the_node_sets() {
    use kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1;
    use kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3;
    let (name, mut backend) = backends("semantics").into_iter().next().unwrap();
    let mut semantics = PalwClassSemanticsV1::of_registration(backend.class_id(), backend.artifact_root(), backend.canonical(), 0);
    semantics.model_profile_id = Hash64::from_u64_word(0x3D01);
    semantics.runtime_manifest_hash = Hash64::from_u64_word(0x3D02);
    semantics.runtime_class_id = Hash64::from_u64_word(0x3D03);
    semantics.cu_ruleset_id = Hash64::from_u64_word(0x3D04);
    semantics.tokenizer_id = backend.class().tokenizer_id;
    let prompt = prompt_of(&backend, 0);
    let job = fp_job(&backend, &prompt, 2);
    let context_of = |b: &TirBackendV1| {
        let run = b.execute_free_prompt(&job, &usize_ids(&prompt)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let shape = b.capture_shape(&run.outcome.material).expect("an IR capture has a shape");
        let PalwPublicExecutionBindingHeaderV1::Tir(header) = b.public_execution_binding_header_v1(&run.outcome.material).unwrap() else {
            panic!("{name}: an IR capture exports a TIR header");
        };
        (shape.job_context, header.job_context)
    };
    let (default_shape, default_header) = context_of(&backend);
    assert!(!semantics.matches_context(&default_shape) && !semantics.matches_context(&default_header), "the defaults are not the statement");
    backend.set_registered_class_facts_v1(PalwFpClassFactsV3 {
        model_profile_id: semantics.model_profile_id,
        runtime_manifest_hash: semantics.runtime_manifest_hash,
        runtime_class_id: semantics.runtime_class_id,
        shape_profile_id: Hash64::from_u64_word(0xBAD),
        cu_ruleset_id: semantics.cu_ruleset_id,
    });
    let (shape, header) = context_of(&backend);
    assert!(semantics.matches_context(&shape), "{name}: the run is performed under the registered statement");
    assert!(semantics.matches_context(&header), "{name}: and its public header carries it");
    assert_eq!(shape.shape_profile_id, backend.class_id(), "the class id is never the caller's");
    // One field off is refused by the same predicate the frame applies.
    let mut off = semantics.clone();
    off.cu_ruleset_id = Hash64::from_u64_word(0x3D05);
    assert!(!off.matches_context(&header));
}

/// **Lane rfc02-09: the builder door the node's IR-only duty paths use** (`TirBackendV1::with_fp_class_facts_v1`, called by
/// `PalwPanelService::resolve_tir_registered_v1` through `palw_tir_with_registered_facts_v1`). A class registered with non-default
/// semantics executes under the REGISTERED statement — not the backend defaults — and the builder and the trait setter agree
/// byte for byte on the context they build.
#[test]
fn tir_free_prompt_builder_executes_a_non_default_class_under_its_registered_semantics() {
    use kaspa_consensus_core::palw_class_semantics_v1::PalwClassSemanticsV1;
    use kaspa_consensus_core::palw_fp_execution_v3::PalwFpClassFactsV3;
    let mut pair = backends("semantics-builder").into_iter();
    let (name, default_backend) = pair.next().unwrap();
    let (_, mut set_backend) = backends("semantics-builder").into_iter().next().unwrap();
    let mut semantics =
        PalwClassSemanticsV1::of_registration(default_backend.class_id(), default_backend.artifact_root(), default_backend.canonical(), 0);
    semantics.model_profile_id = Hash64::from_u64_word(0x4E01);
    semantics.runtime_manifest_hash = Hash64::from_u64_word(0x4E02);
    semantics.runtime_class_id = Hash64::from_u64_word(0x4E03);
    semantics.cu_ruleset_id = Hash64::from_u64_word(0x4E04);
    semantics.tokenizer_id = default_backend.class().tokenizer_id;
    let facts = PalwFpClassFactsV3 {
        model_profile_id: semantics.model_profile_id,
        runtime_manifest_hash: semantics.runtime_manifest_hash,
        runtime_class_id: semantics.runtime_class_id,
        shape_profile_id: Hash64::from_u64_word(0xBAD),
        cu_ruleset_id: semantics.cu_ruleset_id,
    };
    let prompt = prompt_of(&default_backend, 0);
    let job = fp_job(&default_backend, &prompt, 2);
    let context_of = |b: &TirBackendV1| {
        let run = b.execute_free_prompt(&job, &usize_ids(&prompt)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let shape = b.capture_shape(&run.outcome.material).expect("an IR capture has a shape");
        let PalwPublicExecutionBindingHeaderV1::Tir(header) = b.public_execution_binding_header_v1(&run.outcome.material).unwrap() else {
            panic!("{name}: an IR capture exports a TIR header");
        };
        (shape.job_context, header.job_context)
    };
    let (default_shape, _) = context_of(&default_backend);
    assert!(!semantics.matches_context(&default_shape), "the defaults are not the registered statement");
    let class_id = default_backend.class_id();
    let built = default_backend.with_fp_class_facts_v1(facts.clone());
    let (shape, header) = context_of(&built);
    assert!(semantics.matches_context(&shape), "{name}: the run executes under the registered semantics");
    assert!(semantics.matches_context(&header), "{name}: and its public header carries them");
    assert_eq!(shape.shape_profile_id, class_id, "the class id is never the caller's");
    set_backend.set_registered_class_facts_v1(facts);
    assert_eq!(context_of(&set_backend), (shape, header), "the builder and the node's setter build one context");
}
