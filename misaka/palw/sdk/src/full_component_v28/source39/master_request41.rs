//! Frontend → complete public Master request (RFC-0002 §II.14 "frontend and
//! runtime connection"). A checked frontend request (tokenizer-pinned prompt
//! with every image's processor geometry and token expansion bound by
//! `bind_original_mistral_request_v41`, or a text-only adapter bound by
//! `bind_original_text_request_v39`) becomes the exact `MasterPublicRequestV39`
//! the composer and the native rule consume. Each image enters only by the C_v3
//! commitment of the requester's own native i16 HWC raster and a raw-source
//! identity derived from its pinned SHA-256; the pixels are never synthesized.
//! Nothing here is a fidelity result, a Final or a launch right.
use super::*;
use misaka_palw_kernel::{
    flat_invocation39::{MasterImageV39, MasterProducerV39, MasterPublicRequestV39},
    flat_job39::RequestStopV39,
};

/// Raw-source identity of a requester's native image file (also the material
/// origin the composer requires for that image's descriptor).
pub fn native_image_raw_source_v41(raw_sha256: [u8; 32], shape: [u32; 3]) -> Digest {
    object_id(b"misaka-palw/sdk/original-native-image-file/v41", &(raw_sha256, shape))
}
/// C_v3 commitment of the native i16 `[h, w, 3]` raster, computed with the same
/// two streaming paths `CheckedMaterialFileV39::open` uses, so the material
/// descriptor built from the same file carries the identical tensor root.
pub fn native_image_tensor_v41(path: &Path, raw_sha256: [u8; 32], shape: [u32; 3], workspace: u64) -> Result<Digest, String> {
    let dims = shape.iter().map(|d| *d as usize).collect::<Vec<_>>();
    let dtype = misaka_palw_tir::DType::I16;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let bytes = dims.iter().product::<usize>() as u64 * 2;
    if file.metadata().map_err(|e| e.to_string())?.len() != bytes || hash(path, 256 << 20)? != raw_sha256 {
        return Err("MASTER41_NATIVE_IMAGE_PIN".into());
    }
    let geometry = streamed_commitment_shape_v3(dtype, &dims)?;
    if geometry.workspace_bytes <= workspace {
        Ok(tensor_commitment_streamed_v3(dtype, &dims, workspace, &mut |at, out| {
            file.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
            file.read_exact(out).map_err(|e| e.to_string())
        })?
        .0)
    } else {
        misaka_palw_kernel::flat_material39::tensor_commitment_streamed_v39(dtype, &dims, |offset, count| {
            if count > 4096 {
                return Err("MASTER41_ORIGINAL_LEAF_READ_COUNT".into());
            }
            let mut raw = vec![0; count * 2];
            file.seek(SeekFrom::Start(offset * 2)).map_err(|e| e.to_string())?;
            file.read_exact(&mut raw).map_err(|e| e.to_string())?;
            Ok(raw.chunks_exact(2).map(|b| dtype.decode_le(b)).collect())
        })
    }
}
/// The requester's own principal and the original per-stage producer terms.
pub struct MasterPrincipalV41 {
    pub requester: Digest,
    pub requester_operator: Digest,
    pub first_nonce: Digest,
    pub producers: Vec<MasterProducerV39>,
    pub seed: [u8; 32],
}
/// Build the complete public Master request from a checked multimodal
/// frontend request. Images are ordered by their original index.
pub fn master_request_from_frontend_v41(
    complete: &CheckedCompleteSourceV39,
    checked: &request41::CheckedFrontendRequestV41,
    principal: MasterPrincipalV41,
    workspace: u64,
) -> Result<MasterPublicRequestV39, String> {
    if checked.complete_source() != complete.root() {
        return Err("MASTER41_COMPLETE_SOURCE_DIFFERS".into());
    }
    let RequestStopV39::Decode { prompt, requested_new_tokens, eos } = checked.stop().clone() else {
        return Err("MASTER41_DECODER_STOP_REQUIRED".into());
    };
    let mut images = Vec::with_capacity(checked.images().len());
    for (i, image) in checked.images().iter().enumerate() {
        if image.image as usize != i {
            return Err("MASTER41_ORIGINAL_IMAGE_ORDER".into());
        }
        images.push(MasterImageV39 {
            height: image.shape[0],
            width: image.shape[1],
            tensor: native_image_tensor_v41(&image.raw_path, image.raw_sha256, image.shape, workspace)?,
            raw_source: native_image_raw_source_v41(image.raw_sha256, image.shape),
        });
    }
    let request = MasterPublicRequestV39 {
        version: 39,
        complete_source: complete.root(),
        requester: principal.requester,
        requester_operator: principal.requester_operator,
        first_nonce: principal.first_nonce,
        producers: principal.producers,
        prompt,
        negative: vec![],
        source: vec![],
        key: vec![],
        steps: 0,
        scalars: vec![],
        images,
        seed: principal.seed,
        requested_new_tokens,
        eos,
        finalized: vec![],
    };
    request.check(complete.pipeline())?;
    Ok(request)
}
/// Text-only tokenizer-pinned request adapter (the same adapter schema the
/// Python preparers emit, without images). The template string and tokenizer
/// file must be the ones the complete source's manifest pins.
pub fn bind_original_text_request_v39(
    complete: &CheckedCompleteSourceV39,
    decoder_stage: u16,
    tokenizer_file: &str,
    template: &str,
    request_path: &Path,
    request_sha: [u8; 32],
) -> Result<RequestStopV39, String> {
    let raw = bytes(request_path, 16 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != request_sha {
        return Err("TEXT39_ORIGINAL_REQUEST_SHA".into());
    }
    let request: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let source = complete.sources().get(decoder_stage as usize).ok_or("TEXT39_ORIGINAL_DECODER_STAGE_MISSING")?;
    let tokenizer = complete
        .manifest()
        .manifest()
        .source_files
        .iter()
        .find(|f| f.path == tokenizer_file)
        .ok_or("TEXT39_ORIGINAL_TOKENIZER_FILE_NOT_PINNED")?;
    if request["schema"] != "misaka.palw.original-text-request-adapter.v39"
        || request["template"] != template
        || request["original_context_max"].as_u64() != Some(source.original_context_max() as u64)
        || request["images"].as_array().is_some_and(|a| !a.is_empty())
        || decode::<32>(request["source_pins"]["tokenizer"].as_str().ok_or("TEXT39_TOKENIZER_SHA_MISSING")?)? != tokenizer.sha256
    {
        return Err("TEXT39_ORIGINAL_REQUEST_PROFILE_OR_TOKENIZER_DIFFERS".into());
    }
    let prompt: Vec<u32> = serde_json::from_value(request["prompt"].clone()).map_err(|e| e.to_string())?;
    let eos: Vec<u32> = serde_json::from_value(request["eos"].clone()).map_err(|e| e.to_string())?;
    let requested_new_tokens = u32::try_from(request["requested_new_tokens"].as_u64().ok_or("TEXT39_REQUESTED_NEW_TOKENS")?)
        .map_err(|_| "TEXT39_REQUESTED_NEW_TOKENS")?;
    let stop = RequestStopV39::Decode { prompt, requested_new_tokens, eos };
    let RequestStopV39::Decode { prompt, .. } = &stop else { unreachable!() };
    if stop.maximum_positions()? > source.original_context_max() || prompt.iter().any(|t| *t >= source.program().token_bound) {
        return Err("TEXT39_ORIGINAL_REQUEST_SCOPE_EXCEEDS_SOURCE".into());
    }
    Ok(stop)
}
/// Text-only Master request from a bound stop contract.
pub fn master_request_from_text_v39(
    complete: &CheckedCompleteSourceV39,
    stop: RequestStopV39,
    principal: MasterPrincipalV41,
) -> Result<MasterPublicRequestV39, String> {
    let RequestStopV39::Decode { prompt, requested_new_tokens, eos } = stop else {
        return Err("TEXT39_DECODER_STOP_REQUIRED".into());
    };
    let request = MasterPublicRequestV39 {
        version: 39,
        complete_source: complete.root(),
        requester: principal.requester,
        requester_operator: principal.requester_operator,
        first_nonce: principal.first_nonce,
        producers: principal.producers,
        prompt,
        negative: vec![],
        source: vec![],
        key: vec![],
        steps: 0,
        scalars: vec![],
        images: vec![],
        seed: principal.seed,
        requested_new_tokens,
        eos,
        finalized: vec![],
    };
    request.check(complete.pipeline())?;
    Ok(request)
}
