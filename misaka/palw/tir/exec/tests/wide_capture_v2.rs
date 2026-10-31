#![cfg(feature = "node")]
//! Bounded private capture framing and data identity, without an arithmetic verdict.
mod run_wide_common;

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64, palw_logits_q24_v2,
    palw_step_leg::{PalwStepTileLeafV1, step_merkle_root_v1, step_tile_leaf_hash_v1},
    palw_tir_step_v1::{
        PalwTirStepBindingV1, PalwTirStepSpaceV1, PalwTirStepSpaceV2, palw_tir_execution_root_v1, verify_tir_binding_v2,
    },
};
use misaka_palw_tir_exec::node::{
    TirCaptureV1, TirClassRunnerV1,
    run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirJobRunWideQ24V2, TirWideRunLimitsV2},
    wide_capture_v2::{TirWideCaptureV2, WideCaptureAuthLimitsV2, WideCaptureLimitsV2},
};
use run_wide_common::*;

const MAGIC: &[u8; 8] = b"PALWTRC2";
const CODEC_SCRATCH: u64 = 8192;
// Conservative caller-declared envelope for independent fixture/plan/params,
// expected/verified bindings, frames, reference copies and the run when another
// operation does not already count it. This is not measured backing or RSS.
const EXTERNAL: u64 = 64 << 20;

fn limits() -> WideCaptureLimitsV2 {
    WideCaptureLimitsV2 {
        max_frame_bytes: 1 << 26,
        max_decoded_bytes: 1 << 26,
        max_rows: 1 << 16,
        max_row_lanes: 1 << 16,
        max_prompt_ids: 1 << 16,
        max_leaves: LEAF_CAP,
        max_leaf_payload_bytes: 1 << 20,
        max_total_leaf_payload_bytes: 1 << 26,
        max_operation_bytes: 1 << 30,
        external_retained_bytes: EXTERNAL,
    }
}

fn auth_limits() -> WideCaptureAuthLimitsV2 {
    WideCaptureAuthLimitsV2 {
        max_step_leaves: LEAF_CAP,
        max_work: 1 << 44,
        max_scratch_bytes: 1 << 28,
        max_operation_bytes: 1 << 30,
        external_retained_bytes: EXTERNAL,
    }
}

fn run_limits() -> TirWideRunLimitsV2 {
    TirWideRunLimitsV2 {
        max_positions: MAX_CONTEXT,
        max_step_leaves: LEAF_CAP,
        max_row_lanes: 16,
        max_work: 1 << 40,
        max_requested_bytes: 1 << 30,
    }
}

// Independent public wire schema. Production has no generic deserializer or
// public mutator; attacks enter only through its bounded byte decoder.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct RawCapture {
    binding: PalwTirStepBindingV1,
    prompt: Vec<u32>,
    logits_rows: Vec<Vec<i64>>,
    generated: Vec<u32>,
    leaves: Vec<PalwStepTileLeafV1>,
}

impl RawCapture {
    fn frame(&self) -> Vec<u8> {
        let mut frame = MAGIC.to_vec();
        frame.extend(borsh::to_vec(self).unwrap());
        frame
    }

    fn assert_capture(&self, capture: &TirWideCaptureV2) {
        assert_eq!(capture.binding(), &self.binding);
        assert_eq!(capture.prompt(), self.prompt);
        assert_eq!(capture.logits_rows(), self.logits_rows);
        assert_eq!(capture.generated(), self.generated);
        assert_eq!(capture.leaves(), self.leaves);
        assert_eq!(capture.is_dense(), !self.leaves.is_empty());
    }

    fn recommit(&mut self) {
        let binding = &mut self.binding;
        let context = binding.job_context.context_hash();
        let class = binding.class.class_id(&binding.artifact_root);
        if !self.leaves.is_empty() {
            let hashes: Vec<_> = self.leaves.iter().map(|leaf| step_tile_leaf_hash_v1(&context, &class, leaf)).collect();
            binding.step_merkle_root = step_merkle_root_v1(&hashes).unwrap();
        }
        binding.full_logits_trace_root =
            palw_logits_q24_v2::trace_root(&binding.job_context, &self.logits_rows, &self.generated).unwrap();
        binding.committed_execution_root = palw_tir_execution_root_v1(
            &context,
            &binding.full_logits_trace_root,
            &class,
            binding.step_leaf_count,
            &binding.step_merkle_root,
        );
    }
}

fn binding(f: &Fixture<'_>, run: &TirJobRunWideQ24V2) -> PalwTirStepBindingV1 {
    PalwTirStepBindingV1 {
        version: 1,
        job_context: f.job.clone(),
        class: f.class.clone(),
        artifact_root: f.artifact,
        full_logits_trace_root: run.trace_root(),
        step_leaf_count: run.leaf_count(),
        step_merkle_root: run.step_merkle_root(),
        committed_execution_root: run.execution_root(),
    }
}

fn with_producer(window: u32, body: impl FnOnce(&Fixture<'_>, &TirJobRunWideQ24V2, &PalwTirStepBindingV1, Vec<PalwStepTileLeafV1>)) {
    with_fixture(window, |wide, _| {
        let space = PalwTirStepSpaceV2::new(&wide.class).unwrap();
        let runner = TirClassRunnerWideQ24V2::new(&space, wide.plan, wide.params, wide.job.shape_profile_id).unwrap();
        let quote = runner.quote(&wide.job, run_limits()).unwrap();
        // One pre-reserved collection during this single producer execution.
        // Capture construction receives the immutable run; it cannot invoke it.
        let mut leaves = Vec::with_capacity(quote.leaf_count as usize);
        let run = runner
            .run(&wide.job, PROMPT, wide.job.context_hash(), run_limits(), &mut |leaf| leaves.push(leaf.preimage.clone()))
            .unwrap();
        let expected = binding(wide, &run); // Independently fixed before capture delivery.
        body(wide, &run, &expected, leaves);
    });
}

fn raw_source(run: &TirJobRunWideQ24V2, expected: &PalwTirStepBindingV1, leaves: Vec<PalwStepTileLeafV1>) -> RawCapture {
    RawCapture {
        binding: expected.clone(),
        prompt: PROMPT.to_vec(),
        logits_rows: run.logits_rows().to_vec(),
        generated: run.generated().to_vec(),
        leaves,
    }
}

#[test]
fn literal_wide_borsh_schema_preserves_i64_endpoints_and_old_v1_capture_grammar() {
    for window in [1, 3] {
        with_producer(window, |f, run, expected, leaves| {
            let raw = raw_source(run, expected, leaves.clone());
            let expected_frame = raw.frame();
            let capture = TirWideCaptureV2::from_run(expected, PROMPT, run, leaves, limits()).unwrap();
            raw.assert_capture(&capture);
            assert_eq!(TirWideCaptureV2::MAGIC, *MAGIC);
            assert_eq!(capture.encode_bounded(limits()).unwrap(), expected_frame);
            assert_eq!(borsh::to_vec(&capture).unwrap(), expected_frame[8..]);
            assert_eq!(borsh::from_slice::<RawCapture>(&expected_frame[8..]).unwrap(), raw);
            assert_eq!(&expected_frame[8..10], &1u16.to_le_bytes()); // Binding remains V1.
            let rows = 8 + borsh::object_length(expected).unwrap() + 4 + PROMPT.len() * 4;
            assert_eq!(&expected_frame[rows..rows + 4], &6u32.to_le_bytes());
            assert_eq!(&expected_frame[rows + 4..rows + 8], &4u32.to_le_bytes());
            let first_row: Vec<u8> = [i64::MIN, 1082i64 << 24, i64::MAX, i64::MAX].into_iter().flat_map(i64::to_le_bytes).collect();
            assert_eq!(&expected_frame[rows + 8..rows + 40], first_row);
            let decoded = TirWideCaptureV2::decode_bounded(&expected_frame, limits()).unwrap();
            raw.assert_capture(&decoded);
            let verified = verify_tir_binding_v2(expected, LEAF_CAP).unwrap();
            decoded.authenticate_expected(expected, &verified, auth_limits()).unwrap();
            assert_eq!(f.job.tokenizer_id, Hash64::from_bytes([0; 64]));
            assert_eq!(f.class.tokenizer_id, Hash64::from_u64_word(18));
            assert_eq!(f.class.layout.max_context, MAX_CONTEXT);
            assert_eq!(f.job.max_context_tokens, MAX_CONTEXT);
            assert!(TirCaptureV1::decode(&expected_frame).is_err());
            drop(capture);
            drop(decoded);
            let fold = TirWideCaptureV2::from_run(expected, PROMPT, run, Vec::new(), limits()).unwrap();
            assert!(!fold.is_dense());
            fold.authenticate_expected(expected, &verified, auth_limits()).unwrap();
        });
    }

    with_fixture(3, |_, narrow| {
        let space = PalwTirStepSpaceV1::new(&narrow.class).unwrap();
        let runner = TirClassRunnerV1::new(&space, narrow.plan, narrow.params, narrow.job.shape_profile_id).unwrap();
        let mut leaves = Vec::new();
        let run = runner.run(&narrow.job, PROMPT, LEAF_CAP, true, &mut |leaf| leaves.push(leaf.preimage.clone())).unwrap();
        let legacy = TirCaptureV1 {
            binding: PalwTirStepBindingV1 {
                version: 1,
                job_context: narrow.job.clone(),
                class: narrow.class.clone(),
                artifact_root: narrow.artifact,
                full_logits_trace_root: run.trace_root,
                step_leaf_count: run.leaf_count,
                step_merkle_root: run.step_merkle_root,
                committed_execution_root: run.execution_root,
            },
            prompt: PROMPT.to_vec(),
            logits_rows: run.logits_rows,
            generated: run.generated,
            leaves,
        };
        let old_body =
            borsh::to_vec(&(&legacy.binding, &legacy.prompt, &legacy.logits_rows, &legacy.generated, &legacy.leaves)).unwrap();
        // Freeze the actual historical magic, rather than inventing a TRC1 tag.
        assert_eq!(TirCaptureV1::MAGIC, *b"PALWTIRC");
        let mut old_frame = b"PALWTIRC".to_vec();
        old_frame.extend(old_body);
        assert_eq!(legacy.encode(), old_frame);
        assert_eq!(TirCaptureV1::decode(&old_frame).unwrap(), legacy);
        assert!(TirWideCaptureV2::decode_bounded(&old_frame, limits()).is_err());
        let mut relabelled = old_frame;
        relabelled[..8].copy_from_slice(MAGIC);
        if let Ok(delivery) = TirWideCaptureV2::decode_bounded(&relabelled, limits()) {
            // A relabelled legacy program cannot produce the required V2
            // pre-admitted space, regardless of accidental byte decodability.
            assert!(verify_tir_binding_v2(delivery.binding(), LEAF_CAP).is_err());
        }
    });
}

#[test]
fn bounded_false_ids_and_values_remain_data_evidence_without_selection_or_arithmetic_credit() {
    with_producer(3, |_, run, original, leaves| {
        let honest = raw_source(run, original, leaves);
        let original_verified = verify_tir_binding_v2(original, LEAF_CAP).unwrap();
        for alteration in 0..4 {
            let mut raw = honest.clone();
            match alteration {
                0 => raw.generated[0] = 0,                                                // Authenticated but non-greedy ID.
                1 => *raw.generated.last_mut().unwrap() = u32::MAX,                       // Public OOV evidence.
                2 => raw.logits_rows[0][0] ^= 17,                                         // Dense final leaf deliberately differs.
                _ => raw.leaves[0].values_le[..4].copy_from_slice(&128i32.to_le_bytes()), // I8 interval violation.
            }
            raw.recommit();
            let expected_false_data = raw.binding.clone();
            let false_verified = verify_tir_binding_v2(&expected_false_data, LEAF_CAP).unwrap();
            let capture = TirWideCaptureV2::decode_bounded(&raw.frame(), limits()).unwrap();
            raw.assert_capture(&capture);
            // Identity of bounded evidence does not evaluate its disclosed
            // arithmetic, enforce argmax/vocabulary, or award any verdict.
            capture.authenticate_expected(&expected_false_data, &false_verified, auth_limits()).unwrap();
            assert!(capture.authenticate_expected(original, &original_verified, auth_limits()).is_err());
            assert_eq!(capture.encode_bounded(limits()).unwrap(), raw.frame());
        }
        let mut uncommitted = honest.clone();
        uncommitted.generated[0] ^= 1;
        let capture = TirWideCaptureV2::decode_bounded(&uncommitted.frame(), limits()).unwrap();
        assert!(capture.authenticate_expected(original, &original_verified, auth_limits()).is_err());
        let valid = TirWideCaptureV2::decode_bounded(&honest.frame(), limits()).unwrap();
        valid.authenticate_expected(original, &original_verified, auth_limits()).unwrap();
    });
}

#[test]
fn independent_expected_binding_and_pre_admitted_geometry_cannot_be_replaced_by_delivery() {
    with_producer(3, |_, run, original, leaves| {
        let honest = raw_source(run, original, leaves);
        let original_verified = verify_tir_binding_v2(original, LEAF_CAP).unwrap();
        let valid = TirWideCaptureV2::decode_bounded(&honest.frame(), limits()).unwrap();
        for alteration in 0..6 {
            let mut raw = honest.clone();
            match alteration {
                0 => raw.binding.job_context.job_id = Hash64::from_u64_word(901),
                1 => raw.binding.class.tokenizer_id = Hash64::from_u64_word(902),
                2 => raw.binding.artifact_root = Hash64::from_u64_word(903),
                3 => raw.binding.job_context.execution_seed[0] ^= 1,
                4 => raw.binding.job_context.tokenizer_id = Hash64::from_u64_word(904),
                _ => {
                    raw.leaves.clear();
                    raw.binding.class.layout.checkpoint_interval = 2;
                }
            }
            raw.binding.job_context.shape_profile_id = raw.binding.class.class_id(&raw.binding.artifact_root);
            let foreign_space = PalwTirStepSpaceV2::new(&raw.binding.class).unwrap();
            raw.binding.step_leaf_count = foreign_space.leaf_count_capped(&raw.binding.job_context, LEAF_CAP).unwrap();
            raw.recommit();
            let independently_foreign = raw.binding.clone();
            let foreign_verified = verify_tir_binding_v2(&independently_foreign, LEAF_CAP).unwrap();
            let delivery = TirWideCaptureV2::decode_bounded(&raw.frame(), limits()).unwrap();
            assert!(delivery.authenticate_expected(original, &original_verified, auth_limits()).is_err(), "substitution {alteration}");
            assert!(valid.authenticate_expected(original, &foreign_verified, auth_limits()).is_err(), "stale space {alteration}");
            assert!(valid.authenticate_expected(&independently_foreign, &original_verified, auth_limits()).is_err());
            valid.authenticate_expected(original, &original_verified, auth_limits()).unwrap();
        }
        let mut substituted_source = original.clone();
        substituted_source.job_context.job_id = Hash64::from_u64_word(999);
        assert!(TirWideCaptureV2::from_run(&substituted_source, PROMPT, run, Vec::new(), limits()).is_err());
    });
}

#[test]
fn dense_data_requires_full_canonical_geometry_even_when_altered_roots_are_self_consistent() {
    with_producer(3, |_, run, original, leaves| {
        let honest = raw_source(run, original, leaves);
        let wide_index = honest.leaves.iter().position(|leaf| leaf.version == 2).unwrap();
        for alteration in 0..9 {
            let mut raw = honest.clone();
            match alteration {
                0 => {
                    raw.leaves.remove(1);
                }
                1 => raw.leaves.swap(0, 1),
                2 => raw.leaves[1] = raw.leaves[0].clone(),
                3 => raw.leaves[wide_index].version = 1,
                4 => raw.leaves[0].version = 2,
                5 => raw.leaves[wide_index].values_le.truncate(16),
                6 => raw.leaves[0].value_count -= 1,
                7 => raw.leaves[0].coord.tile_index += 1,
                _ => raw.leaves[wide_index].values_le.push(0),
            }
            raw.recommit();
            let expected_delivery_roots = raw.binding.clone();
            let verified = verify_tir_binding_v2(&expected_delivery_roots, LEAF_CAP).unwrap();
            let capture = TirWideCaptureV2::decode_bounded(&raw.frame(), limits()).unwrap();
            raw.assert_capture(&capture);
            assert!(
                capture.authenticate_expected(&expected_delivery_roots, &verified, auth_limits()).is_err(),
                "accepted coherently rooted malformed dense geometry {alteration}"
            );
        }
        let mut fold = honest;
        fold.leaves.clear();
        let capture = TirWideCaptureV2::decode_bounded(&fold.frame(), limits()).unwrap();
        let verified = verify_tir_binding_v2(original, LEAF_CAP).unwrap();
        capture.authenticate_expected(original, &verified, auth_limits()).unwrap();
        assert!(!capture.is_dense());
    });
}

#[test]
fn every_peer_vector_length_truncation_eof_and_empty_row_amplification_is_bounded() {
    with_producer(3, |_, run, expected, leaves| {
        let raw = raw_source(run, expected, leaves);
        let frame = raw.frame();
        let network = 8 + 2 + 2;
        let program = 8 + 2 + borsh::object_length(&raw.binding.job_context).unwrap() + 2;
        let layout = program + 4 + raw.binding.class.program.len();
        let commit_tiles = layout + 2 + 3 * 4;
        let state_tiles = commit_tiles + 4 + raw.binding.class.layout.commit_tiles.len() * 4;
        let prompt = 8 + borsh::object_length(&raw.binding).unwrap();
        let rows = prompt + 4 + raw.prompt.len() * 4;
        let generated = rows + 4 + raw.logits_rows.iter().map(|row| 4 + row.len() * 8).sum::<usize>();
        let leaves = generated + 4 + raw.generated.len() * 4;
        let leaf_payload = leaves + 4 + 2 + 16 + 4;
        for (field, offset) in [
            ("network", network),
            ("program", program),
            ("commit tiles", commit_tiles),
            ("state tiles", state_tiles),
            ("prompt", prompt),
            ("row headers", rows),
            ("i64 row lanes", rows + 4),
            ("generated", generated),
            ("leaves", leaves),
            ("leaf payload", leaf_payload),
        ] {
            let mut hostile = frame.clone();
            hostile[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(TirWideCaptureV2::decode_bounded(&hostile, limits()).is_err(), "accepted hostile {field} length");
        }
        for length in 0..frame.len() {
            assert!(TirWideCaptureV2::decode_bounded(&frame[..length], limits()).is_err(), "accepted truncated frame {length}");
        }
        let mut trailing = frame.clone();
        trailing.push(0);
        assert!(TirWideCaptureV2::decode_bounded(&trailing, limits()).is_err());

        // 4096 empty rows use only 16 KiB on wire but ~96 KiB of native
        // Vec headers. Admission must reject before reserving that outer Vec.
        let mut amplification = MAGIC.to_vec();
        borsh::to_writer(&mut amplification, &raw.binding).unwrap();
        borsh::to_writer(&mut amplification, &raw.prompt).unwrap();
        amplification.extend_from_slice(&4096u32.to_le_bytes());
        amplification.resize(amplification.len() + 4096 * 4, 0);
        amplification.extend_from_slice(&0u32.to_le_bytes());
        amplification.extend_from_slice(&0u32.to_le_bytes());
        let mut bounded = limits();
        bounded.max_rows = 4096;
        bounded.max_decoded_bytes = 64 << 10;
        bounded.max_operation_bytes = amplification.len() as u64 + bounded.max_decoded_bytes + CODEC_SCRATCH + EXTERNAL;
        assert!(4096 * std::mem::size_of::<Vec<i64>>() as u64 > bounded.max_decoded_bytes);
        let refusal = TirWideCaptureV2::decode_bounded(&amplification, bounded).err().unwrap();
        assert!(refusal.contains("decoded storage") || refusal.contains("backing"), "{refusal}");
    });
}

#[test]
fn build_copy_encode_decode_and_authentication_charge_every_simultaneous_owner_at_exact_caps() {
    with_producer(3, |_, run, expected, mut leaves| {
        let original_quote = TirWideCaptureV2::quote_from_run(expected, PROMPT, run, &leaves, limits()).unwrap();
        let original_capacity = leaves.capacity();
        leaves.reserve_exact(64);
        let quote = TirWideCaptureV2::quote_from_run(expected, PROMPT, run, &leaves, limits()).unwrap();
        let spare_bytes = (leaves.capacity() - original_capacity) as u64 * std::mem::size_of::<PalwStepTileLeafV1>() as u64;
        assert!(spare_bytes > 0);
        assert_eq!(quote.capture_bytes - original_quote.capture_bytes, spare_bytes);
        assert_eq!(quote.moved_leaf_bytes - original_quote.moved_leaf_bytes, spare_bytes);
        assert_eq!(quote.source_run_bytes, run.reserved_bytes());
        assert!(quote.moved_leaf_bytes > 0 && quote.moved_leaf_bytes < quote.capture_bytes);
        assert_eq!(quote.input_frame_bytes, 0);
        assert_eq!(quote.operation_peak_bytes, run.reserved_bytes() + quote.capture_bytes + quote.scratch_bytes + EXTERNAL);
        let mut exact = limits();
        exact.max_frame_bytes = quote.encoded_frame_bytes;
        exact.max_decoded_bytes = quote.capture_bytes;
        exact.max_rows = run.generated().len() as u64;
        exact.max_row_lanes = 4;
        exact.max_prompt_ids = PROMPT.len() as u64;
        exact.max_leaves = leaves.len() as u64;
        exact.max_leaf_payload_bytes = leaves.iter().map(|leaf| leaf.values_le.len() as u64).max().unwrap();
        exact.max_total_leaf_payload_bytes = leaves.iter().map(|leaf| leaf.values_le.len() as u64).sum();
        exact.max_operation_bytes = quote.operation_peak_bytes;
        for ceiling in 0..10 {
            let mut one_short = exact;
            match ceiling {
                0 => one_short.max_frame_bytes -= 1,
                1 => one_short.max_decoded_bytes -= 1,
                2 => one_short.max_rows -= 1,
                3 => one_short.max_row_lanes -= 1,
                4 => one_short.max_prompt_ids -= 1,
                5 => one_short.max_leaves -= 1,
                6 => one_short.max_leaf_payload_bytes -= 1,
                7 => one_short.max_total_leaf_payload_bytes -= 1,
                8 => one_short.max_operation_bytes -= 1,
                _ => one_short.external_retained_bytes = u64::MAX,
            }
            assert!(TirWideCaptureV2::quote_from_run(expected, PROMPT, run, &leaves, one_short).is_err(), "build cap {ceiling}");
        }
        let capture = TirWideCaptureV2::from_run(expected, PROMPT, run, leaves, exact).unwrap();
        assert_eq!(capture.backing_bytes().unwrap(), quote.capture_bytes);
        let encode = capture.quote_encode(limits()).unwrap();
        assert_eq!(encode.capture_bytes, quote.capture_bytes);
        assert_eq!(encode.source_run_bytes, 0);
        assert_eq!(encode.scratch_bytes, CODEC_SCRATCH);
        assert_eq!(encode.operation_peak_bytes, encode.capture_bytes + encode.encoded_frame_bytes + CODEC_SCRATCH + EXTERNAL);
        let mut encode_limits = limits();
        encode_limits.max_operation_bytes = encode.operation_peak_bytes;
        encode_limits.max_frame_bytes = encode.encoded_frame_bytes;
        encode_limits.max_decoded_bytes = encode.capture_bytes;
        let frame = capture.encode_bounded(encode_limits).unwrap();
        assert_eq!(frame.len() as u64, encode.encoded_frame_bytes);
        encode_limits.max_operation_bytes -= 1;
        assert!(capture.encode_bounded(encode_limits).is_err());

        let decoded = TirWideCaptureV2::decode_bounded(&frame, limits()).unwrap();
        let decoded_bytes = decoded.backing_bytes().unwrap();
        assert!(decoded_bytes <= capture.backing_bytes().unwrap()); // Spare moved capacity is absent on wire.
        let mut decode_limits = limits();
        decode_limits.max_frame_bytes = frame.len() as u64;
        decode_limits.max_decoded_bytes = decoded_bytes;
        decode_limits.max_operation_bytes = frame.len() as u64 + decoded_bytes + CODEC_SCRATCH + EXTERNAL;
        assert!(TirWideCaptureV2::decode_bounded(&frame, decode_limits).is_ok());
        for ceiling in 0..3 {
            let mut one_short = decode_limits;
            match ceiling {
                0 => one_short.max_frame_bytes -= 1,
                1 => one_short.max_decoded_bytes -= 1,
                _ => one_short.max_operation_bytes -= 1,
            }
            assert!(TirWideCaptureV2::decode_bounded(&frame, one_short).is_err(), "decode cap {ceiling}");
        }

        let verified = verify_tir_binding_v2(expected, LEAF_CAP).unwrap();
        let auth = capture.quote_auth_expected(expected, &verified, auth_limits()).unwrap();
        assert_eq!(auth.capture_bytes, quote.capture_bytes);
        assert_eq!(auth.external_retained_bytes, EXTERNAL);
        assert_eq!(auth.operation_peak_bytes, auth.capture_bytes + auth.scratch_bytes + EXTERNAL);
        assert!(auth.work > 0 && auth.scratch_bytes > 0);
        let mut exact_auth = auth_limits();
        exact_auth.max_step_leaves = expected.step_leaf_count;
        exact_auth.max_work = auth.work;
        exact_auth.max_scratch_bytes = auth.scratch_bytes;
        exact_auth.max_operation_bytes = auth.operation_peak_bytes;
        capture.authenticate_expected(expected, &verified, exact_auth).unwrap();
        for ceiling in 0..5 {
            let mut one_short = exact_auth;
            match ceiling {
                0 => one_short.max_step_leaves -= 1,
                1 => one_short.max_work -= 1,
                2 => one_short.max_scratch_bytes -= 1,
                3 => one_short.max_operation_bytes -= 1,
                _ => one_short.external_retained_bytes = u64::MAX,
            }
            assert!(capture.authenticate_expected(expected, &verified, one_short).is_err(), "auth cap {ceiling}");
        }
        // Failed delivery and resource admission do not consume any session or
        // mutate the immutable capture. The exact same original remains usable.
        capture.authenticate_expected(expected, &verified, exact_auth).unwrap();
        assert_eq!(capture.encode_bounded(limits()).unwrap(), frame);
    });
}
