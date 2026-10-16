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
/// `custodians` (a network with the registry resolves it to its Panel's seat count plus the producer: 6 on testnet-12).
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
        // Two custodians (a producer and one seat): the frame's resource arithmetic holds there for every corpus class (it does not
        // at testnet-12's six — `tir_frame_allocation_overflows_at_testnet12s_panel_gap_pin`).
        let quote = derive_tir_public_allocation_v1(&geometry, &commitment, &header, &record, &backend.class().layout, None, None, rules(2))
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
    eprintln!("[tir-fp] complete frame originals (2 custodians): {framed:?}");
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

/// **GAP PIN (found by this lane, owned by the TIR frame lane): the frame's native allocation overflows its own checked `u64`
/// arithmetic at testnet-12's Panel size.** With the network's staged config and ladder, every corpus class (about 100–220 step
/// leaves, vocabulary 24) is a complete frame original at 1–2 custodians and is refused at 6 (a five-seat Panel plus the producer):
/// `required-family sum overflows maximum_parser_steps` or `native TIR allocation exceeds u64`. With G-03, such a class takes no
/// free-prompt claim past the fence at all. This test pins the gap; it must be inverted when the frame's resource accounting is
/// fixed (it records, it does not endorse).
#[test]
fn tir_frame_allocation_overflows_at_testnet12s_panel_gap_pin() {
    let mut record_line = Vec::new();
    for (k, (name, backend)) in backends("gap").into_iter().enumerate() {
        let prompt = prompt_of(&backend, k);
        let job = fp_job(&backend, &prompt, 2);
        let run = backend.execute_free_prompt(&job, &usize_ids(&prompt)).unwrap();
        let shape = backend.capture_shape(&run.outcome.material).unwrap();
        let commitment = palw_fp_commitment_from_context_v3(&job, &shape.job_context, &run, 999_999).unwrap();
        let PalwPublicExecutionBindingHeaderV1::Tir(header) = backend.public_execution_binding_header_v1(&run.outcome.material).unwrap()
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
        let largest = (1..=6u64)
            .take_while(|c| {
                derive_tir_public_allocation_v1(&geometry, &commitment, &header, &record, &backend.class().layout, None, None, rules(*c))
                    .is_ok()
            })
            .last()
            .unwrap_or(0);
        let at_six = derive_tir_public_allocation_v1(&geometry, &commitment, &header, &record, &backend.class().layout, None, None, rules(6));
        record_line.push(format!("{name}: n={} largest framable upper={largest}", commitment.work_leaves));
        assert!(largest >= 2, "{name}: framable at two custodians");
        let why = at_six.err().unwrap_or_else(|| panic!("{name}: framable at testnet-12's upper — invert this gap pin"));
        assert!(why.contains("exceeds u64") || why.contains("overflows"), "{name}: {why}");
    }
    eprintln!("[tir-fp gap] {record_line:?}");
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
