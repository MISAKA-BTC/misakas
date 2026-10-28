//! Actual producer capture framing and allocation refusal; no model RSS certification.
#![cfg(feature = "node")]
#![allow(warnings)]
mod node_common;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirStepBindingV1, PalwTirStepSpaceV1};
use misaka_palw_tir_exec::node::{TirCaptureV1, TirClassRunnerV1, tir_authenticate_capture_metadata_v1};
use misaka_palw_tir_exec::{TirParams, TirPlan};

fn capture() -> TirCaptureV1 {
    let (_, program, params) = node_common::h_program();
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: node_common::layout(&program, 3, 2, 2, 64),
        tokenizer_id: Hash64::from_bytes([1; 64]),
    };
    let artifact_root = Hash64::from_bytes([0xA7; 64]);
    let class_id = class.class_id(&artifact_root);
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let prompt = vec![3, 8, 13];
    let ctx = node_common::job(&program, 3, 2, class_id, &prompt, PalwPromptIdsFormV1::Flat);
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &params).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, class_id).unwrap();
    let mut leaves = Vec::new();
    let run = runner.run(&ctx, &prompt, 1 << 26, false, &mut |l| leaves.push(l.preimage.clone())).unwrap();
    TirCaptureV1 {
        binding: PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: ctx,
            class,
            artifact_root,
            full_logits_trace_root: run.trace_root,
            step_leaf_count: run.leaf_count,
            step_merkle_root: run.step_merkle_root,
            committed_execution_root: run.execution_root,
        },
        prompt,
        logits_rows: run.logits_rows,
        generated: run.generated,
        leaves,
    }
}

#[test]
fn framed_capture_preserves_dense_fold_and_false_value_wire_bytes() {
    let mut c = capture();
    tir_authenticate_capture_metadata_v1(&c, c.binding.class.class_id(&c.binding.artifact_root), PalwPromptIdsFormV1::Flat, 1 << 26)
        .expect("exact real producer metadata");
    for dense in [true, false] {
        if !dense {
            c.leaves.clear();
        }
        let encoded = c.encode();
        assert_eq!(TirCaptureV1::decode(&encoded).unwrap(), c);
        assert_eq!(borsh::from_slice::<TirCaptureV1>(&encoded[8..]).unwrap(), c, "existing Borsh wire is unchanged");
    }
    c.generated[0] = u32::MAX;
    c.logits_rows[0][0] ^= 1;
    assert_eq!(TirCaptureV1::decode(&c.encode()).unwrap(), c, "codec retains bounded false evidence");
    assert!(
        tir_authenticate_capture_metadata_v1(
            &c,
            c.binding.class.class_id(&c.binding.artifact_root),
            PalwPromptIdsFormV1::Flat,
            1 << 26
        )
        .is_err()
    );
}

#[test]
fn every_capture_vector_refuses_a_truncated_hostile_length_before_allocation() {
    let c = capture();
    let encoded = c.encode();
    let network = 8 + 2 + 2;
    let program = 8 + 2 + borsh::object_length(&c.binding.job_context).unwrap() + 2;
    let layout = program + 4 + c.binding.class.program.len();
    let commit_tiles = layout + 2 + 3 * 4;
    let state_tiles = commit_tiles + 4 + c.binding.class.layout.commit_tiles.len() * 4;
    let prompt = 8 + borsh::object_length(&c.binding).unwrap();
    let rows = prompt + 4 + c.prompt.len() * 4;
    let generated = rows + 4 + c.logits_rows.iter().map(|r| 4 + r.len() * 4).sum::<usize>();
    let leaves = generated + 4 + c.generated.len() * 4;
    let leaf_bytes = leaves + 4 + 2 + 16 + 4;
    for (name, offset) in [
        ("network", network),
        ("program", program),
        ("commit tiles", commit_tiles),
        ("state tiles", state_tiles),
        ("prompt", prompt),
        ("logits rows", rows),
        ("logits lanes", rows + 4),
        ("generated", generated),
        ("leaves", leaves),
        ("leaf payload", leaf_bytes),
    ] {
        let mut hostile = encoded.clone();
        hostile[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let err = TirCaptureV1::decode(&hostile).unwrap_err();
        assert!(err.contains("vector length exceeds the remaining frame"), "{name}: {err}");
    }
}

#[test]
fn nested_empty_rows_cannot_amplify_beyond_the_real_consumer_reservation() {
    let c = capture();
    let mut hostile = TirCaptureV1::MAGIC.to_vec();
    borsh::to_writer(&mut hostile, &c.binding).unwrap();
    borsh::to_writer(&mut hostile, &c.prompt).unwrap();
    let rows = 1_500_000u32;
    hostile.extend_from_slice(&rows.to_le_bytes());
    hostile.resize(hostile.len() + rows as usize * 4, 0); // each row is an encoded empty Vec
    hostile.extend_from_slice(&0u32.to_le_bytes()); // generated
    hostile.extend_from_slice(&0u32.to_le_bytes()); // leaves
    let decoded_headers = rows as usize * std::mem::size_of::<Vec<i32>>();
    assert!(decoded_headers > hostile.len() * 2 + (16 << 20));
    assert!(
        TirCaptureV1::decode(&hostile).unwrap_err().contains("decoded storage exceeds its reservation"),
        "refuse the outer allocation before creating 1.5 million native row headers"
    );
}

#[test]
fn framed_capture_keeps_strict_eof_and_refuses_truncated_frames() {
    let c = capture();
    let mut encoded = c.encode();
    for length in (0..encoded.len()).step_by(17) {
        assert!(TirCaptureV1::decode(&encoded[..length]).is_err());
    }
    encoded.push(0);
    assert!(TirCaptureV1::decode(&encoded).unwrap_err().contains("trailing bytes"));
}
