//! Input-derived bounds for an atomic Selection terminal using the existing
//! signed DecodeToken conviction. These quantities do not grant a service slot.

use crate::{
    Hash64,
    mldsa87_primitives::MLDSA87_SIGNATURE_LEN,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1 as Fault, source_tir_program_shape_v1},
    palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2},
    palw_step_leg::step_leg_max_opening_siblings_v1,
    palw_step_refute::PALW_LOGITS_TILE_LANES,
    palw_tir_admission_v1::PalwTirClassRecordV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_DOMAIN_V1, PalwTirLayoutV1, palw_tir_class_id_from_digests_v1},
    palw_tir_public_carriage_v1::{PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1, PalwTirPublicControlTargetV1},
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use borsh::BorshSerialize;

/// Exact canonical upper lengths for one terminal and the inputs to price one
/// objective replay. Archive row multiplicity, settlement writes, native pass
/// count, UTXO/script cost, and conversion into service work units are the
/// enclosing profile's responsibilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct SelectionTerminalBoundsV2 {
    pub pin_bytes: u64,
    pub stripped_binding_bytes: u64,
    pub proof_bytes: u64,
    pub evidence_bytes: u64,
    pub signed_control_bytes: u64,
    pub archive_cause_bytes: u64,
    pub ids: u64,
    pub tile_lanes: u64,
    pub row_siblings: u64,
    pub tile_siblings: u64,
    pub lane_keys: u64,
    pub penalty_id_visits: u64,
    pub bias_comparisons: u64,
    pub sampling_keys: u64,
    pub program_bytes: u64,
    pub program_structural_items: u64,
    pub source_construction_requests: u64,
}

fn length(value: &impl BorshSerialize) -> Result<u64, Fault<String>> {
    let bytes = borsh::object_length(value).map_err(|error| Fault::Decode(error.to_string()))?;
    u64::try_from(bytes).map_err(|_| overflow())
}

fn overflow() -> Fault<String> {
    Fault::Resource(NativeCoverageV1::overflow::<SelectionTerminalBoundsV2>("Selection terminal bounds"))
}

#[allow(dead_code)]
impl SelectionTerminalBoundsV2 {
    pub(crate) fn derive_v2(
        original: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        record: &PalwTirClassRecordV1,
        layout: &PalwTirLayoutV1,
    ) -> Result<Self, Fault<String>> {
        super::PlainBatchOutputV2::full_ids_bound_v2(original, source).map_err(|why| Fault::Decode(format!("{why:?}")))?;
        let header = source.header_v1();
        if record.facts.class_id != original.job.class_id
            || record.logits_vocab != source.vocab_v1()
            || record.tokenizer_id != header.job_context.tokenizer_id
            || stream_digest(PALW_TIR_LAYOUT_DOMAIN_V1, layout).map_err(Fault::Decode)? != record.layout_digest
            || palw_tir_class_id_from_digests_v1(
                PALW_TIR_CLASS_VERSION_V1,
                &record.graph_ir_root,
                &record.layout_digest,
                &header.artifact_root,
                &record.tokenizer_id,
            ) != record.facts.class_id
        {
            return Err(Fault::Decode("Selection terminal bounds substitute the original registered class or layout".into()));
        }
        let ids = u64::from(source.decode_count_v1());
        let vocab = u64::from(source.vocab_v1());
        let tile_lanes = vocab.min(PALW_LOGITS_TILE_LANES as u64);
        let row_siblings = step_leg_max_opening_siblings_v1(ids) as u64;
        let tile_siblings = step_leg_max_opening_siblings_v1(vocab.div_ceil(PALW_LOGITS_TILE_LANES as u64)) as u64;

        // The original source bounds ids and vocabulary to u32. The scalar
        // encoding has position/beat, one row root, three Vec prefixes and
        // three openings (u64 index, Hash64 leaf, u32 sibling count).
        let pin_bytes = 312 + 4 * ids + 8 * tile_lanes + 64 * row_siblings + 128 * tile_siblings;
        // PalwTirStepBindingV1 with a borrowed class layout and an EMPTY
        // program. Borsh serializes these tuples without adding any framing.
        let stripped_binding_bytes = length(&(
            header.version,
            &header.job_context,
            (PALW_TIR_CLASS_VERSION_V1, &[] as &[u8], layout, record.tokenizer_id),
            header.artifact_root,
            header.full_logits_trace_root,
            header.step_leaf_count,
            header.step_merkle_root,
            header.committed_execution_root,
        ))?;
        let proof_bytes = stripped_binding_bytes.checked_add(pin_bytes + 1).ok_or_else(overflow)?;
        let evidence_bytes = proof_bytes.checked_add(1).ok_or_else(overflow)?;
        let signed_overhead = length(&(
            1u16,
            [Hash64::default(); 3],
            PalwTirPublicControlTargetV1::Conviction,
            0u64,
            0u64,
            PalwBondKeyV2(crate::tx::TransactionOutpoint::default()),
            0u8,  // Convict action discriminant
            0u32, // signature Vec length prefix
        ))?;
        let signed_control_bytes = evidence_bytes
            .checked_add(signed_overhead)
            .and_then(|bytes| bytes.checked_add(MLDSA87_SIGNATURE_LEN as u64))
            .ok_or_else(overflow)?;
        let archive_prefix = length(&(
            0u8, // CourtEndEvidenceV1::TirConviction discriminant
            PalwBlockContextV2 { block: Hash64::default(), daa_score: 0, blue_score: 0, subsidy: 0 },
            Hash64::default(),
            PalwBondKeyV2(crate::tx::TransactionOutpoint::default()),
        ))?;
        let archive_cause_bytes = evidence_bytes.checked_add(archive_prefix).ok_or_else(overflow)?;

        let job = &original.job;
        // The plain-profile check excluded Prefix and Constraint. Use the
        // remaining decoder dispatch of PalwTirSelectionRuleV1::of_job_v1
        // without cloning the V4 config merely to inspect its dimensions.
        let (penalty_id_visits, bias_comparisons) = match (&job.decode, job.is_v4() || job.is_v5() || job.is_eval()) {
            (Some(config), true) => {
                let entries = config.logit_bias.len() as u64;
                // binary_search_by_key performs a final comparison as well.
                let comparisons = if entries == 0 { 0 } else { entries.ilog2() as u64 + 2 };
                (2 * (ids - 1).min(u64::from(config.penalty_window)), 2 * comparisons)
            }
            _ => (0, 0),
        };
        // This also checks that the actual program length and digest equal
        // the registered values used in the class-ID comparison above.
        let source_construction_requests = PalwTirPublicSourceV1::native_construction_requests_v1(record, layout, header)?;
        let (program_shape, _) = source_tir_program_shape_v1(&record.program)?;
        Ok(Self {
            pin_bytes,
            stripped_binding_bytes,
            proof_bytes,
            evidence_bytes,
            signed_control_bytes,
            archive_cause_bytes,
            ids,
            tile_lanes,
            row_siblings,
            tile_siblings,
            lane_keys: 2,
            penalty_id_visits,
            bias_comparisons,
            sampling_keys: if job.sampling_v2().is_greedy() { 0 } else { 2 },
            program_bytes: u64::from(record.program_bytes),
            program_structural_items: program_shape.structural_items_v1(),
            source_construction_requests,
        })
    }

    /// This is the inner conviction wire ceiling and original proof limit.
    /// Outer versioned transport bytes and transaction mass are checked by the
    /// compiler for the concrete carrier it selects.
    pub(crate) fn fits_signed_control_v2(self, witness_bytes: u64) -> bool {
        self.proof_bytes <= witness_bytes && self.signed_control_bytes <= PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw_court_end_archive_v1::CourtEndEvidenceV1,
        palw_court_v2::PalwCourtVerdictProofV2,
        palw_tir_public_carriage_v1::{PalwTirPublicControlActionV1, PalwTirPublicControlV1, PalwTirPublicConvictionEvidenceV1},
        palw_tir_public_claim_v1::{
            palw_tir_public_frame_fixture_ruled_v1, palw_tir_public_frame_fixture_two_tile_v1, palw_tir_public_frame_fixture_v1,
            palw_tir_public_history_frame_fixture_v1,
        },
    };

    #[test]
    fn bct2_selection_terminal_bounds_match_signed_and_archive_codecs() {
        use crate::palw_step_leg::PalwStepOpeningV1;
        use crate::palw_step_refute::PalwTiledDecodePinV1;
        let v4 = palw_tir_public_frame_fixture_ruled_v1(
            1,
            |job| {
                job.version = crate::palw_freeprompt_v3::PALW_FP_V4_VERSION;
                job.temperature_q = 1 << 24;
                job.decode = Some(crate::palw_decode_pipeline_v4::DecodeConfigV4 {
                    repeat_penalty_q: 2 * 65536,
                    penalty_window: 4,
                    logit_bias: vec![(0, 1), (1, 1)],
                    ..crate::palw_decode_pipeline_v4::DecodeConfigV4::NOOP
                });
            },
            None,
        );
        for (fixture, two_tiles, v4) in [
            (palw_tir_public_frame_fixture_v1(None, 1), false, false),
            (palw_tir_public_frame_fixture_two_tile_v1(), true, false),
            (palw_tir_public_history_frame_fixture_v1(), false, false),
            (v4, false, true),
        ] {
            let frame = &fixture.frame;
            let original = frame.original_v1();
            let source = PalwTirPublicSourceV1::new_native_v1(
                &fixture.record,
                &fixture.layout,
                frame.header_v1(),
                original.execution_root,
                original.trace_root,
                fixture.rules.maximum_leaves,
            )
            .unwrap();
            let quote = SelectionTerminalBoundsV2::derive_v2(original, &source, &fixture.record, &fixture.layout).unwrap();
            assert_eq!(quote.tile_siblings, u64::from(two_tiles));
            assert_eq!(quote.ids, u64::from(original.decode_tokens_executed));
            assert_eq!(quote.lane_keys, 2);
            if v4 {
                assert_eq!(quote.penalty_id_visits, 2 * (quote.ids - 1).min(4));
                assert_eq!(quote.bias_comparisons, 6);
                assert_eq!(quote.sampling_keys, 2);
            }
            let mut wrong_layout = fixture.layout.clone();
            wrong_layout.checkpoint_interval += 1;
            assert!(matches!(
                SelectionTerminalBoundsV2::derive_v2(original, &source, &fixture.record, &wrong_layout),
                Err(Fault::Decode(_))
            ));
            let mut wrong_record = fixture.record.clone();
            wrong_record.program_bytes -= 1;
            assert!(matches!(
                SelectionTerminalBoundsV2::derive_v2(original, &source, &wrong_record, &fixture.layout),
                Err(Fault::Decode(_))
            ));
            wrong_record.program_bytes = fixture.record.program_bytes;
            std::sync::Arc::make_mut(&mut wrong_record.program)[0] ^= 1;
            assert!(matches!(
                SelectionTerminalBoundsV2::derive_v2(original, &source, &wrong_record, &fixture.layout),
                Err(Fault::Decode(_))
            ));

            let mut binding = frame
                .header_v1()
                .resolve_registered_layout_v1(&fixture.record, &fixture.layout, original.execution_root, fixture.rules.maximum_leaves)
                .unwrap();
            crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut binding);
            assert_eq!(length(&binding).unwrap(), quote.stripped_binding_bytes);
            let opening = |siblings| PalwStepOpeningV1 {
                leaf_index: 0,
                leaf_hash: Hash64::default(),
                siblings: vec![Hash64::default(); siblings as usize],
            };
            // Maximal codec geometry, independent of validity of the
            // opening values. Both lane operands may use the same full tile.
            let pin = PalwTiledDecodePinV1 {
                position: 0,
                generated_token_ids: vec![0; quote.ids as usize],
                row_root: Hash64::default(),
                row_opening: opening(quote.row_siblings),
                committed_tile_lanes: vec![0; quote.tile_lanes as usize],
                committed_opening: opening(quote.tile_siblings),
                beat_tile_lanes: vec![0; quote.tile_lanes as usize],
                beat_opening: opening(quote.tile_siblings),
                beat_lane: 0,
            };
            assert_eq!(length(&pin).unwrap(), quote.pin_bytes);
            let proof = PalwCourtVerdictProofV2::TirDecodeTokenTiled { binding: Box::new(binding), pin };
            assert_eq!(length(&proof).unwrap(), quote.proof_bytes);
            let evidence = PalwTirPublicConvictionEvidenceV1::DecodeToken { proof: Box::new(proof) };
            assert_eq!(length(&evidence).unwrap(), quote.evidence_bytes);
            let filer = PalwBondKeyV2(crate::tx::TransactionOutpoint::default());
            let control = PalwTirPublicControlV1 {
                version: 1,
                claim: frame.plan_v1().original_claim,
                allocation_id: frame.plan_v1().allocation_id,
                source_id: source.source_id_v1(),
                target: PalwTirPublicControlTargetV1::Conviction,
                expected_turn: 0,
                declared_rent_sompi: 1,
                signer: filer,
                action: PalwTirPublicControlActionV1::Convict(evidence.clone()),
                signature: vec![0; MLDSA87_SIGNATURE_LEN],
            };
            assert_eq!(length(&control).unwrap(), quote.signed_control_bytes);
            let cause = CourtEndEvidenceV1::TirConviction {
                point: PalwBlockContextV2 { block: Hash64::default(), daa_score: 0, blue_score: 0, subsidy: 0 },
                claim: control.claim,
                filer,
                evidence: Box::new(evidence),
            };
            assert_eq!(length(&cause).unwrap(), quote.archive_cause_bytes);
            assert!(quote.fits_signed_control_v2(quote.proof_bytes));
            assert!(!quote.fits_signed_control_v2(quote.proof_bytes - 1));
        }
    }
}
