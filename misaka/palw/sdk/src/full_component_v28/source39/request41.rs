//! Exact source-backed multimodal request binding. The checked value proves a
//! frontend data relation; it grants no native receipt, money or launch rights.
use super::*;
use borsh::BorshSerialize;
use misaka_palw_kernel::{flat_component_source39::CheckedCompleteSourceV39, flat_job39::RequestStopV39};
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize)]
#[serde(deny_unknown_fields)]
pub struct ImageExecutionV41 {
    pub image: u8,
    pub processor_stage: u16,
    pub vision_stage: u16,
    pub original: [u32; 2],
    pub resized: [u32; 2],
    pub image_tokens: Vec<u32>,
    pub params_file: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize)]
#[serde(deny_unknown_fields)]
pub struct CompleteImageContractV41 {
    pub version: u16,
    pub complete_source: String,
    pub tokenizer: String,
    pub tokenizer_file: String,
    pub decoder_stage: u16,
    pub original_context: u32,
    pub images: Vec<ImageExecutionV41>,
}
impl CompleteImageContractV41 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/sdk/complete-original-image-input-contract/v41", self)
    }
}
#[derive(Clone, Debug)]
pub struct BoundImageInputV41 {
    pub image: u8,
    pub processor_source: Digest,
    pub raw_path: PathBuf,
    pub raw_sha256: [u8; 32],
    pub shape: [u32; 3],
}
pub struct CheckedFrontendRequestV41 {
    complete: Digest,
    contract: Digest,
    request_sha256: [u8; 32],
    decoder_source: Digest,
    stop: RequestStopV39,
    images: Vec<BoundImageInputV41>,
}
impl CheckedFrontendRequestV41 {
    pub fn complete_source(&self) -> Digest {
        self.complete
    }
    pub fn contract(&self) -> Digest {
        self.contract
    }
    pub fn request_sha256(&self) -> [u8; 32] {
        self.request_sha256
    }
    pub fn decoder_source(&self) -> Digest {
        self.decoder_source
    }
    pub fn stop(&self) -> &RequestStopV39 {
        &self.stop
    }
    pub fn images(&self) -> &[BoundImageInputV41] {
        &self.images
    }
}
pub(super) fn build_contract_v41(
    plan: &CompleteFrontendV39,
    geometries: &[Option<lower::pixtral_processor41::ProcessorGeometryV41>],
    complete: &CheckedCompleteSourceV39,
    tokenizer: Digest,
) -> Result<Option<CompleteImageContractV41>, String> {
    let mut images = Vec::new();
    let mut fixed_rgb = false;
    for (i, stage) in plan.stages.iter().enumerate() {
        if let FrontendCompilerV39::Vision { processor_stage, .. } = &stage.compiler {
            let Some(processor) = processor_stage else {
                fixed_rgb = true;
                continue;
            };
            let geometry =
                geometries.get(*processor as usize).and_then(|g| g.as_ref()).ok_or("SOURCE41_PROCESSOR_GEOMETRY_MISSING")?;
            let image = match plan.pipeline.stages[*processor as usize].bind.as_slice() {
                [super::super::frontend::BindingDescription::JobImage { index }] => *index,
                _ => return Err("SOURCE41_PROCESSOR_REQUIRES_ORIGINAL_JOB_IMAGE".into()),
            };
            images.push(ImageExecutionV41 {
                image,
                processor_stage: *processor,
                vision_stage: i as u16,
                original: geometry.original,
                resized: geometry.resized,
                image_tokens: geometry.image_tokens.clone(),
                params_file: plan.stages[*processor as usize].config_file.clone(),
            });
        }
    }
    if images.is_empty() {
        return Ok(None);
    }
    if fixed_rgb {
        return Err("SOURCE41_MIXED_FIXED_RGB_AND_FULL_PUBLISHER_PROCESSOR".into());
    }
    images.sort_by_key(|i| i.image);
    if images.len() > 10 || images.iter().enumerate().any(|(i, image)| image.image as usize != i) {
        return Err("SOURCE41_ORIGINAL_IMAGE_INDEX_COVERAGE".into());
    }
    let decoders = plan
        .stages
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match &s.compiler {
            FrontendCompilerV39::Decoder { profile } if profile.image_rows.is_some() => Some((i, profile)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(decoder, profile)] = decoders.as_slice() else {
        return Err("SOURCE41_REQUIRES_EXACT_ORIGINAL_MULTIMODAL_DECODER".into());
    };
    let input = profile.image_rows.as_ref().ok_or("SOURCE41_DECODER_IMAGE_INPUT_MISSING")?;
    let expected = images.iter().map(|i| i.vision_stage).collect::<Vec<_>>();
    match &plan.stages[input.upstream_stage as usize].compiler {
        FrontendCompilerV39::ImageJoin { upstream_stages, .. } if *upstream_stages == expected => {}
        FrontendCompilerV39::Vision { .. } if expected == vec![input.upstream_stage] => {}
        _ => return Err("SOURCE41_COMPLETE_IMAGE_JOIN_ORDER_DIFFERS".into()),
    }
    for image in &images {
        let FrontendCompilerV39::Processor { profile } = &plan.stages[image.processor_stage as usize].compiler else {
            return Err("SOURCE41_ORIGINAL_PROCESSOR_KIND".into());
        };
        if input.placeholder != profile.image_token {
            return Err("SOURCE41_DECODER_ORIGINAL_PLACEHOLDER_DIFFERS".into());
        }
    }
    Ok(Some(CompleteImageContractV41 {
        version: 41,
        complete_source: crate::runtime_pack::commit::hex(&complete.root()),
        tokenizer: crate::runtime_pack::commit::hex(&tokenizer),
        tokenizer_file: plan.tokenizer_file.clone(),
        decoder_stage: *decoder as u16,
        original_context: profile.original_context,
        images,
    }))
}
/// Bind the actual original tokenizer output to every source-backed image
/// processor/vision stage. The caller independently pins BOTH files before the
/// run. DTO eligibility flags are ignored and cannot mint a checked native Final.
pub fn bind_original_mistral_request_v41(
    complete: &CheckedCompleteSourceV39,
    contract_path: &Path,
    contract_sha: [u8; 32],
    request_path: &Path,
    request_sha: [u8; 32],
    request_directory: &Path,
) -> Result<CheckedFrontendRequestV41, String> {
    let contract_raw = bytes(contract_path, 4 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&contract_raw)) != contract_sha {
        return Err("SOURCE41_ORIGINAL_IMAGE_CONTRACT_SHA".into());
    }
    let contract: CompleteImageContractV41 = serde_json::from_slice(&contract_raw).map_err(|e| e.to_string())?;
    if contract.version != 41
        || decode::<64>(&contract.complete_source)? != complete.root()
        || contract.images.len() > 10
        || contract.images.is_empty()
        || contract.images.iter().enumerate().any(|(i, g)| g.image as usize != i || g.image_tokens.len() > 3080)
    {
        return Err("SOURCE41_COMPLETE_SOURCE_IMAGE_CONTRACT_DIFFERS".into());
    }
    let raw = bytes(request_path, 16 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != request_sha {
        return Err("SOURCE41_ORIGINAL_REQUEST_SHA".into());
    }
    let request: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if request["schema"] != "misaka.palw.mistral-original-request-adapter.v39"
        || request["template"] != "mistral-common==1.6.2/Tekken-v11"
        || request["original_context_max"].as_u64() != Some(contract.original_context as u64)
    {
        return Err("SOURCE41_ORIGINAL_REQUEST_PROFILE".into());
    }
    let source = complete.sources().get(contract.decoder_stage as usize).ok_or("SOURCE41_ORIGINAL_DECODER_STAGE_MISSING")?;
    if source.original_context_max() != contract.original_context {
        return Err("SOURCE41_ORIGINAL_DECODER_CONTEXT_DIFFERS".into());
    }
    let tokenizer_pin = complete
        .manifest()
        .manifest()
        .source_files
        .iter()
        .find(|f| f.path == contract.tokenizer_file)
        .ok_or("SOURCE41_ORIGINAL_TOKENIZER_FILE_MISSING")?;
    if decode::<32>(request["source_pins"]["tokenizer"].as_str().ok_or("SOURCE41_ORIGINAL_TOKENIZER_SHA_MISSING")?)?
        != tokenizer_pin.sha256
    {
        return Err("SOURCE41_ORIGINAL_TOKENIZER_SHA_DIFFERS".into());
    }
    let prompt: Vec<u32> = serde_json::from_value(request["prompt"].clone()).map_err(|e| e.to_string())?;
    let eos: Vec<u32> = serde_json::from_value(request["eos"].clone()).map_err(|e| e.to_string())?;
    let requested_new_tokens = u32::try_from(request["requested_new_tokens"].as_u64().ok_or("SOURCE41_REQUESTED_NEW_TOKENS_MISSING")?)
        .map_err(|_| "SOURCE41_REQUESTED_NEW_TOKENS_EXTENT")?;
    let stop = RequestStopV39::Decode { prompt: prompt.clone(), requested_new_tokens, eos };
    if stop.maximum_positions()? > contract.original_context || prompt.iter().any(|t| *t >= source.program().token_bound) {
        return Err("SOURCE41_ORIGINAL_REQUEST_SCOPE_EXCEEDS_SOURCE".into());
    }
    let input_images = request["images"].as_array().ok_or("SOURCE41_ORIGINAL_IMAGES_MISSING")?;
    let spans = request["image_token_spans"].as_array().ok_or("SOURCE41_ORIGINAL_IMAGE_SPANS_MISSING")?;
    if input_images.len() != contract.images.len() || spans.len() != contract.images.len() {
        return Err("SOURCE41_COMPLETE_ORIGINAL_IMAGE_COVERAGE".into());
    }
    let directory = request_directory.canonicalize().map_err(|e| e.to_string())?;
    let mut bound = Vec::new();
    let mut prior = 0usize;
    let mut special_ids = BTreeSet::<u32>::new();
    for ((image, observed), span) in contract.images.iter().zip(input_images).zip(spans) {
        let processor = complete.sources().get(image.processor_stage as usize).ok_or("SOURCE41_PROCESSOR_SOURCE_MISSING")?;
        let vision = complete.sources().get(image.vision_stage as usize).ok_or("SOURCE41_VISION_SOURCE_MISSING")?;
        let original = serde_json::to_value(image.original).map_err(|e| e.to_string())?;
        let resized = serde_json::to_value(image.resized).map_err(|e| e.to_string())?;
        if observed["original"] != original
            || observed["resized"] != resized
            || observed["image_tokens"] != serde_json::to_value(&image.image_tokens).map_err(|e| e.to_string())?
        {
            return Err("SOURCE41_ORIGINAL_GEOMETRY_OR_IMAGE_TOKEN_SEQUENCE_DIFFERS".into());
        }
        let shape = [image.original[0], image.original[1], 3];
        let input = processor.program().inputs.first().ok_or("SOURCE41_PROCESSOR_INPUT_MISSING")?;
        let vision_input = vision.program().inputs.first().ok_or("SOURCE41_VISION_INPUT_MISSING")?;
        if processor.program().inputs.len() != 1
            || input.dtype != misaka_palw_tir::DType::I16
            || input.shape != shape
            || vision_input.dtype != misaka_palw_tir::DType::I16
            || vision_input.shape != [image.resized[0], image.resized[1], 3]
        {
            return Err("SOURCE41_ORIGINAL_REGISTERED_PROCESSOR_VISION_GEOMETRY_DIFFERS".into());
        }
        let start =
            usize::try_from(span["start"].as_u64().ok_or("SOURCE41_IMAGE_SPAN_START")?).map_err(|_| "SOURCE41_IMAGE_SPAN_START")?;
        let end = start.checked_add(image.image_tokens.len()).ok_or("SOURCE41_IMAGE_SPAN_OVERFLOW")?;
        if start < prior
            || span["tokens"].as_u64() != Some(image.image_tokens.len() as u64)
            || prompt.get(start..end) != Some(image.image_tokens.as_slice())
        {
            return Err("SOURCE41_ORIGINAL_IMAGE_PROMPT_SPAN_DIFFERS".into());
        }
        prior = end;
        special_ids.extend(image.image_tokens.iter().copied());
        let params = complete
            .manifest()
            .manifest()
            .source_files
            .iter()
            .find(|f| f.path == image.params_file)
            .ok_or("SOURCE41_ORIGINAL_PROCESSOR_PARAMS_MISSING")?;
        if decode::<32>(request["source_pins"]["params"].as_str().ok_or("SOURCE41_ORIGINAL_PROCESSOR_PARAMS_SHA")?)? != params.sha256 {
            return Err("SOURCE41_ORIGINAL_PROCESSOR_PARAMS_DIFFERS".into());
        }
        let raw_path = relative(&directory, observed["native_input_file"].as_str().ok_or("SOURCE41_RAW_NATIVE_IMAGE_FILE_MISSING")?)?;
        let raw_sha256 = decode::<32>(observed["native_input_sha256"].as_str().ok_or("SOURCE41_RAW_NATIVE_IMAGE_SHA_MISSING")?)?;
        let cells = shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("SOURCE41_NATIVE_IMAGE_EXTENT")?;
        let length = cells.checked_mul(2).ok_or("SOURCE41_NATIVE_IMAGE_EXTENT")?;
        if raw_path.metadata().map_err(|e| e.to_string())?.len() != length || hash(&raw_path, 256 << 20)? != raw_sha256 {
            return Err("SOURCE41_ORIGINAL_NATIVE_IMAGE_BYTES_DIFFER".into());
        }
        bound.push(BoundImageInputV41 { image: image.image, processor_source: processor.root(), raw_path, raw_sha256, shape });
    }
    let mut covered = vec![false; prompt.len()];
    for (image, span) in contract.images.iter().zip(spans) {
        let start = span["start"].as_u64().ok_or("SOURCE41_IMAGE_SPAN_START")? as usize;
        for c in covered.get_mut(start..start + image.image_tokens.len()).ok_or("SOURCE41_IMAGE_SPAN_SCOPE")? {
            *c = true;
        }
    }
    if prompt.iter().zip(covered).any(|(id, covered)| special_ids.contains(id) && !covered) {
        return Err("SOURCE41_UNBOUND_EXTRA_IMAGE_TOKEN_SPAN".into());
    }
    Ok(CheckedFrontendRequestV41 {
        complete: complete.root(),
        contract: contract.root(),
        request_sha256: request_sha,
        decoder_source: source.root(),
        stop,
        images: bound,
    })
}
