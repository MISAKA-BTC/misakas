//! RFC0040 authoring and bounded public localization. Keys remain with the
//! caller's MessageSigner; signing authenticates finite obligations only.
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_claim_local_v1::prefix_v1::*;
use kaspa_consensus_core::palw_prefix_family_court_v1::{PalwPrefixCourtActionV1, PalwPrefixPacketBuilderV1};
use kaspa_consensus_core::palw_prefix_state_court_v1::{PalwPrefixKvPlanV1, PalwPrefixKvTileV1, palw_prefix_kv_authenticated_bytes_v1};
use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_hashes::Hash64;

fn active(params: &Params, daa: u64) -> Result<(), String> {
    if !params.palw_claim_local_settlement_v1.is_some_and(|f| f.is_active(daa)) {
        return Err("prefix provenance is below the compiled claim-local fence".into());
    }
    Ok(())
}

pub fn sign_export_v1(
    params: &Params, daa: u64, key: &dyn kaspa_pq_validator_core::MessageSigner, intent: PalwPrefixExportIntentV1,
) -> Result<PalwSignedPrefixExportV1, String> {
    active(params, daa)?;
    intent.check_v1().map_err(|e| e.to_string())?;
    if intent.liability_until < daa || intent.producer_key_id != kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(key.public_key()) {
        return Err("source export has expired or names another signing key".into());
    }
    let signature = key.sign_message(intent.id_v1().as_byte_slice(), PREFIX_EXPORT_CONTEXT_V1)?;
    intent.verify_signature_v1(key.public_key(), &signature).map_err(|e| e.to_string())?;
    Ok(PalwSignedPrefixExportV1 { intent, signature })
}

/// The source statement/accepted identity must come from the pinned node's
/// source observation. The fold still verifies the consumer's actual birth.
pub fn sign_use_v1(
    params: &Params, daa: u64, key: &dyn kaspa_pq_validator_core::MessageSigner,
    export: &PalwSignedPrefixExportV1, accepted: &PalwPrefixExportIdentityV1, intent: PalwPrefixUseIntentV1,
) -> Result<PalwSignedPrefixUseV1, String> {
    active(params, daa)?;
    intent.check_v1().map_err(|e| e.to_string())?;
    if intent.consumer_key_id != kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(key.public_key())
        || intent.source != *accepted || accepted.intent != export.intent.id_v1()
        || intent.network != export.intent.network || intent.prefix != export.intent.prefix
        || intent.hard_deadline < daa || intent.hard_deadline > export.intent.liability_until
        || intent.material_until > export.intent.material_until {
        return Err("consumer input differs from the observed finite source export".into());
    }
    let signature = key.sign_message(intent.id_v1().as_byte_slice(), PREFIX_USE_CONTEXT_V1)?;
    intent.verify_signature_v1(key.public_key(), &signature).map_err(|e| e.to_string())?;
    Ok(PalwSignedPrefixUseV1 { intent, signature })
}

pub fn wrap_payload_v1(
    params: &Params, daa: u64, inner: Vec<u8>, exports: Vec<PalwSignedPrefixExportV1>, consumed: Option<PalwSignedPrefixUseV1>,
) -> Result<Vec<u8>, String> {
    active(params, daa)?;
    PalwPrefixDependencyEnvelopeV1 { inner, exports, consumed }.encode_v1().map_err(|e| e.to_string())
}

/// Visit one authenticated dense-cache tile at a time and emit bounded court
/// packets. No Vec of the entire model cache or full action history is built.
/// This localizes bytes against the source execution; model arithmetic remains
/// independently challengeable by the ordinary step court.
pub fn collect_dense_export_packets_v1(
    source_execution_root: Hash64, binding: &PalwStepBindingV2, prefix_tokens: u32,
    maximum_leaves: u64, maximum_packet_bytes: u64,
    mut read_tile: impl FnMut(PalwStepCoordinateV1) -> Result<PalwPrefixKvTileV1, String>,
    mut emit: impl FnMut(Vec<PalwPrefixCourtActionV1>) -> Result<(), String>,
) -> Result<Hash64, String> {
    use kaspa_consensus_core::palw_prefix_state_stream_v1::PalwPrefixStateStreamV1;
    let context = binding.committed_execution_root;
    let mut stream = PalwPrefixStateStreamV1::new_export_v1(context, source_execution_root, binding, prefix_tokens, maximum_leaves)?;
    let plan = PalwPrefixKvPlanV1::of_profile(&binding.shape_profile, prefix_tokens).map_err(str::to_string)?;
    let mut packets = PalwPrefixPacketBuilderV1::new(maximum_packet_bytes)?;
    let mut push = |action| -> Result<(), String> {
        if let Some(packet) = packets.push(action)? { emit(packet)?; }
        Ok(())
    };
    for ordinal in 0..plan.tile_count {
        let (coordinate, _) = plan.coordinate(&binding.shape_profile, ordinal).ok_or("source cache coordinate missing")?;
        let tile = read_tile(coordinate)?;
        let bytes = palw_prefix_kv_authenticated_bytes_v1(binding, &plan, ordinal, &tile, maximum_leaves).map_err(str::to_string)?;
        stream.begin_tile(context, ordinal, &tile.opening)?;
        push(PalwPrefixCourtActionV1::BeginTile { opening: tile.opening.clone() })?;
        for (part, bytes) in bytes.chunks(kaspa_consensus_core::palw_prefix_state_stream_v1::PALW_PREFIX_STREAM_TILE_BYTES_V1).enumerate() {
            let offset = u32::try_from(part.checked_mul(65_536).ok_or("source tile offset overflow")?).map_err(|e| e.to_string())?;
            stream.push_tile_bytes(context, ordinal, offset, bytes)?;
            push(PalwPrefixCourtActionV1::TilePart { offset, bytes: bytes.to_vec() })?;
        }
        stream.finish_tile()?;
        push(PalwPrefixCourtActionV1::FinishTile)?;
    }
    drop(push);
    if let Some(packet) = packets.finish() { emit(packet)?; }
    stream.finish()
}
