//! Original39 maximum invocation. Future decode IDs are absent before Fund;
//! Commit supplies actual first-EOS generation under this immutable contract.
use crate::{
    flat_material39::MaterialDescriptorV39,
    flat_resources39::{CheckedFlatResourcesV39, derive_flat_resources_v39},
    flat_source39::CheckedSourceV39,
    hash::{Digest, object_id},
};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum RequestStopV39 {
    Fixed {
        positions: u32,
        tokens: Vec<u32>,
    },
    Decode {
        prompt: Vec<u32>,
        requested_new_tokens: u32,
        eos: Vec<u32>,
    },
    /// Upstream actual IDs are already Final, while the original Master
    /// requested maximum remains reserved without an EOS-dependent release.
    Derived {
        maximum_positions: u32,
        tokens: Vec<u32>,
    },
}
impl RequestStopV39 {
    pub fn maximum_positions(&self) -> Result<u32, String> {
        match self {
            Self::Fixed { positions, tokens } => {
                if *positions == 0 || tokens.len() != *positions as usize {
                    return Err("job39 original fixed position/token contract".into());
                }
                Ok(*positions)
            }
            Self::Derived { maximum_positions, tokens } => {
                if tokens.is_empty() || tokens.len() > *maximum_positions as usize {
                    return Err("job39 derived actual tokens exceed original Master maximum".into());
                }
                Ok(*maximum_positions)
            }
            Self::Decode { prompt, requested_new_tokens, eos } => {
                if prompt.is_empty()
                    || *requested_new_tokens == 0
                    || eos.is_empty()
                    || eos.len() > 256
                    || eos.windows(2).any(|w| w[0] >= w[1])
                {
                    return Err("job39 original decode maximum/EOS contract".into());
                }
                u32::try_from(prompt.len())
                    .ok()
                    .and_then(|n| n.checked_add(*requested_new_tokens))
                    .and_then(|n| n.checked_sub(1))
                    .ok_or("job39 maximum position overflow".into())
            }
        }
    }
    pub fn actual_tokens(&self, generated: &[u32], bound: u32) -> Result<Vec<u32>, String> {
        let tokens = match self {
            Self::Fixed { tokens, .. } | Self::Derived { tokens, .. } => {
                if !generated.is_empty() {
                    return Err("job39 fixed stage cannot supply future decoder IDs".into());
                }
                tokens.clone()
            }
            Self::Decode { prompt, requested_new_tokens, eos } => {
                if generated.is_empty()
                    || generated.len() > *requested_new_tokens as usize
                    || generated.iter().any(|t| *t >= bound)
                    || generated[..generated.len() - 1].iter().any(|t| eos.binary_search(t).is_ok())
                    || generated.len() < *requested_new_tokens as usize && eos.binary_search(generated.last().unwrap()).is_err()
                {
                    return Err("job39 actual generation is not firstEOS or original maximum stop".into());
                }
                let mut tokens = prompt.clone();
                tokens.extend_from_slice(&generated[..generated.len() - 1]);
                tokens
            }
        };
        if tokens.iter().any(|t| *t >= bound) {
            return Err("job39 original token outside source vocabulary".into());
        }
        Ok(tokens)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MaterialSelectionV39 {
    Whole,
    RowAtPosition,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum InputBindingV39 {
    Material { descriptor: MaterialDescriptorV39, selection: MaterialSelectionV39 },
    Random { seed: [u8; 32], domain: u16, per_step: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MaterialManifestV39 {
    pub version: u16,
    pub source: Digest,
    pub master: Digest,
    pub bindings: Vec<InputBindingV39>,
}
impl MaterialManifestV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/original-input-material-manifest/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct JobMaximumV39 {
    pub version: u16,
    pub source: Digest,
    pub requester: Digest,
    pub producer: Digest,
    pub nonce: Digest,
    pub stop: RequestStopV39,
    pub input_manifest: Digest,
}
impl JobMaximumV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/original-job-maximum/v39", self)
    }
}
pub struct CheckedJobMaximumV39 {
    request: JobMaximumV39,
    resources: CheckedFlatResourcesV39,
    manifest: MaterialManifestV39,
    positions: u32,
}
impl CheckedJobMaximumV39 {
    pub fn check(source: &CheckedSourceV39, r: &JobMaximumV39, m: &MaterialManifestV39) -> Result<Self, String> {
        let positions = r.stop.maximum_positions()?;
        let p = source.program();
        if r.version != 39
            || r.source != source.root()
            || [r.requester, r.producer].contains(&[0; 64])
            || r.requester == r.producer
            || positions > source.original_context_max()
            || m.version != 39
            || m.source != source.root()
            || m.root() != r.input_manifest
            || m.bindings.len() != p.inputs.len()
        {
            return Err("job39 exact original source/request/material maximum differs".into());
        }
        match &r.stop {
            RequestStopV39::Decode { prompt, eos, .. } => {
                if !matches!(p.output, misaka_palw_tir::program_v2::OutputDecl::Logits { .. })
                    || prompt.iter().chain(eos).any(|t| *t >= p.token_bound)
                {
                    return Err("job39 source output/decode token contract".into());
                }
            }
            RequestStopV39::Fixed { tokens, .. } | RequestStopV39::Derived { tokens, .. } => {
                if tokens.iter().any(|t| *t >= p.token_bound) {
                    return Err("job39 original fixed token range".into());
                }
            }
        }
        for (input, (d, b)) in p.inputs.iter().zip(&m.bindings).enumerate() {
            match (b, d.source) {
                (InputBindingV39::Material { descriptor, selection }, misaka_palw_tir::program_v2::InputSource::External { .. }) => {
                    let expected = match selection {
                        MaterialSelectionV39::Whole => d.shape.clone(),
                        MaterialSelectionV39::RowAtPosition => {
                            let material_positions = match &r.stop {
                                RequestStopV39::Derived { tokens, .. } => tokens.len() as u32,
                                _ => positions,
                            };
                            std::iter::once(material_positions).chain(d.shape.iter().copied()).collect()
                        }
                    };
                    if descriptor.input as usize != input
                        || descriptor.source != source.root()
                        || descriptor.master != m.master
                        || descriptor.requester != r.requester
                        || descriptor.nonce != r.nonce
                        || descriptor.dtype != d.dtype
                        || descriptor.shape != expected
                    {
                        return Err("job39 material cannot expand original input shape or rebind upstream owner".into());
                    }
                    // Native registration independently authenticates the descriptor's
                    // original paid seal and upstream Final before using this plan.
                }
                (
                    InputBindingV39::Random { seed, domain, per_step },
                    misaka_palw_tir::program_v2::InputSource::Random { domain: want, per_step: step, .. },
                ) => {
                    if *seed == [0; 32] || *domain != want || *per_step != step {
                        return Err("job39 exact original random source differs".into());
                    }
                }
                _ => return Err("job39 input origin differs from canonical declaration".into()),
            }
        }
        let resources = derive_flat_resources_v39(source, positions)?;
        Ok(Self { request: r.clone(), resources, manifest: m.clone(), positions })
    }
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/checked-original-job-maximum/v39",
            &(self.request.root(), self.resources.root(), self.manifest.root()),
        )
    }
    pub fn request(&self) -> &JobMaximumV39 {
        &self.request
    }
    pub fn resources(&self) -> &CheckedFlatResourcesV39 {
        &self.resources
    }
    pub fn maximum_positions(&self) -> u32 {
        self.positions
    }
    pub fn input_manifest(&self) -> &MaterialManifestV39 {
        &self.manifest
    }
    pub fn check_actual(&self, source: &CheckedSourceV39, generated: &[u32]) -> Result<(Vec<u32>, CheckedFlatResourcesV39), String> {
        if source.root() != self.request.source {
            return Err("job39 actual source substitution".into());
        }
        let tokens = self.request.stop.actual_tokens(generated, source.program().token_bound)?;
        let positions = u32::try_from(tokens.len()).map_err(|_| "job39 actual position extent")?;
        if positions > self.positions {
            return Err("job39 actual length exceeds original maximum".into());
        }
        let actual = derive_flat_resources_v39(source, positions)?;
        actual
            .vector()
            .quantities
            .within(&self.resources.vector().quantities)
            .map_err(|_| "job39 actual resource exceeds original maximum")?;
        Ok((tokens, actual))
    }
}

/// No raw map can supply arbitrary per-position PC roots. The source maximum,
/// actual EOS and original immutable material/seed bindings determine each one.
pub struct ActualInputResolverV39<'a> {
    source: &'a CheckedSourceV39,
    maximum: &'a CheckedJobMaximumV39,
    tokens: Vec<u32>,
    generated: Vec<u32>,
    binding: Digest,
    reader: &'a dyn crate::flat_material39::MaterialPageReaderV39,
    cache: std::cell::RefCell<Option<(u32, std::collections::BTreeMap<u8, Digest>)>>,
}
impl<'a> ActualInputResolverV39<'a> {
    pub fn derive(
        source: &'a CheckedSourceV39,
        maximum: &'a CheckedJobMaximumV39,
        generated: &[u32],
        reader: &'a dyn crate::flat_material39::MaterialPageReaderV39,
    ) -> Result<Self, String> {
        let (tokens, _) = maximum.check_actual(source, generated)?;
        let binding = object_id(
            b"misaka-palw/kernel/original-actual-input-binding/v39",
            &(source.root(), maximum.root(), maximum.input_manifest().root(), &tokens),
        );
        Ok(Self { source, maximum, tokens, generated: generated.to_vec(), binding, reader, cache: std::cell::RefCell::new(None) })
    }
    pub fn root(&self) -> Digest {
        self.binding
    }
    pub fn source(&self) -> Digest {
        self.source.root()
    }
    pub fn maximum(&self) -> Digest {
        self.maximum.root()
    }
    pub fn tokens(&self) -> &[u32] {
        &self.tokens
    }
    pub fn generated(&self) -> &[u32] {
        &self.generated
    }
    pub fn stop(&self) -> &RequestStopV39 {
        &self.maximum.request().stop
    }
    pub fn input_root(&self, position: u32, input: u8) -> Result<Digest, String> {
        if let Some((p, map)) = &*self.cache.borrow()
            && *p == position
            && let Some(root) = map.get(&input)
        {
            return Ok(*root);
        }
        let root = self.input_root_uncached(position, input)?;
        let mut cache = self.cache.borrow_mut();
        if cache.as_ref().is_none_or(|(p, _)| *p != position) {
            *cache = Some((position, std::collections::BTreeMap::new()));
        }
        cache.as_mut().unwrap().1.insert(input, root);
        Ok(root)
    }
    fn input_root_uncached(&self, position: u32, input: u8) -> Result<Digest, String> {
        if position as usize >= self.tokens.len() {
            return Err("input39 actual position exceeds original EOS".into());
        }
        let d = self.source.program().inputs.get(input as usize).ok_or("input39 original input declaration absent")?;
        let b = self.maximum.input_manifest().bindings.get(input as usize).ok_or("input39 original maximum binding absent")?;
        let shape: Vec<_> = d.shape.iter().map(|d| *d as usize).collect();
        match b {
            InputBindingV39::Material { descriptor, selection } => match selection {
                MaterialSelectionV39::Whole => Ok(descriptor.tensor),
                MaterialSelectionV39::RowAtPosition => {
                    let cells = d.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("input39 row cell extent")?;
                    let width = d.dtype.width();
                    let row = position as u64 * cells;
                    crate::flat_material39::tensor_commitment_streamed_v39(d.dtype, &shape, |offset, count| {
                        let offset = row
                            .checked_add(offset)
                            .and_then(|n| n.checked_mul(width as u64))
                            .ok_or("input39 exact upstream row offset")?;
                        let raw = crate::flat_material39::read_material_range_v39(
                            descriptor,
                            self.reader,
                            offset,
                            count.checked_mul(width).ok_or("input39 exact row range")?,
                        )?;
                        Ok(raw.chunks_exact(width).map(|b| d.dtype.decode_le(b)).collect())
                    })
                }
            },
            InputBindingV39::Random { seed, domain, per_step } => {
                let misaka_palw_tir::program_v2::InputSource::Random { dist, .. } = d.source else {
                    return Err("input39 random declaration origin differs".into());
                };
                let step = if *per_step { position } else { 0 };
                crate::flat_material39::tensor_commitment_streamed_v39(d.dtype, &shape, |offset, count| {
                    let mut values = Vec::with_capacity(count);
                    for lane in offset..offset + count as u64 {
                        let word = misaka_palw_gen::rand_word_v1(*domain, seed, step, 0, lane).map_err(|e| format!("{e:?}"))?;
                        let value = match dist {
                            misaka_palw_tir::program_v2::RandomDist::Uniform { .. } => word as i128,
                            misaka_palw_tir::program_v2::RandomDist::Normal => misaka_palw_gen::gauss_q24_v1(word as u16) as i128,
                        };
                        if !d.dtype.contains(value) {
                            return Err("input39 original deterministic random dtype".into());
                        }
                        values.push(value);
                    }
                    Ok(values)
                })
            }
        }
    }
}

/// Online production reads original maximum-bound inputs, never fabricated
/// future generated IDs. This provider is not an actual claim/root authority;
/// after generation ActualInputResolver39 must independently derive all actual
/// IDs/roots before trace/court/native Commit.
pub struct GenerationInputProviderV39<'a> {
    source: &'a CheckedSourceV39,
    maximum: &'a CheckedJobMaximumV39,
    reader: &'a dyn crate::flat_material39::MaterialPageReaderV39,
}
impl<'a> GenerationInputProviderV39<'a> {
    pub fn derive(
        source: &'a CheckedSourceV39,
        maximum: &'a CheckedJobMaximumV39,
        reader: &'a dyn crate::flat_material39::MaterialPageReaderV39,
    ) -> Result<Self, String> {
        if maximum.request().source != source.root()
            || maximum.input_manifest().source != source.root()
            || maximum.request().input_manifest != maximum.input_manifest().root()
            || derive_flat_resources_v39(source, maximum.maximum_positions())?.root() != maximum.resources().root()
        {
            return Err("generation39 original source/maximum/material scope differs".into());
        }
        Ok(Self { source, maximum, reader })
    }
    pub fn read_input_range(&self, position: u32, input: u8, offset: u64, count: usize) -> Result<Vec<i128>, String> {
        original_input_range_v39(
            self.source,
            self.maximum,
            self.reader,
            self.maximum.maximum_positions(),
            position,
            input,
            offset,
            count,
        )
    }
    pub fn input_tensor(&self, position: u32, input: u8) -> Result<misaka_palw_tir::Tensor, String> {
        original_input_tensor_v39(self.source, position, input, |p, i, o, n| self.read_input_range(p, i, o, n))
    }
}
fn original_input_range_v39(
    source: &CheckedSourceV39,
    maximum: &CheckedJobMaximumV39,
    reader: &dyn crate::flat_material39::MaterialPageReaderV39,
    positions: u32,
    position: u32,
    input: u8,
    offset: u64,
    count: usize,
) -> Result<Vec<i128>, String> {
    if position >= positions || count > 4096 {
        return Err("input39 original bounded position/range".into());
    }
    let d = source.program().inputs.get(input as usize).ok_or("input39 declaration absent")?;
    let cells = d.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("input39 cell product")?;
    if offset.checked_add(count as u64).is_none_or(|end| end > cells) {
        return Err("input39 exact original element range".into());
    }
    let b = &maximum.input_manifest().bindings[input as usize];
    let values = match b {
        InputBindingV39::Material { descriptor, selection } => {
            let base = match selection {
                MaterialSelectionV39::Whole => 0,
                MaterialSelectionV39::RowAtPosition => (position as u64).checked_mul(cells).ok_or("input39 row range overflow")?,
            };
            let width = d.dtype.width();
            let at = base.checked_add(offset).and_then(|n| n.checked_mul(width as u64)).ok_or("input39 byte offset overflow")?;
            let raw = crate::flat_material39::read_material_range_v39(
                descriptor,
                reader,
                at,
                count.checked_mul(width).ok_or("input39 byte range overflow")?,
            )?;
            raw.chunks_exact(width).map(|b| d.dtype.decode_le(b)).collect::<Vec<_>>()
        }
        InputBindingV39::Random { seed, domain, per_step } => {
            let misaka_palw_tir::program_v2::InputSource::Random { dist, .. } = d.source else {
                return Err("input39 source random origin mismatch".into());
            };
            let mut values = Vec::with_capacity(count);
            for lane in offset..offset + count as u64 {
                let word = misaka_palw_gen::rand_word_v1(*domain, seed, if *per_step { position } else { 0 }, 0, lane)
                    .map_err(|e| format!("{e:?}"))?;
                values.push(match dist {
                    misaka_palw_tir::program_v2::RandomDist::Uniform { .. } => word as i128,
                    misaka_palw_tir::program_v2::RandomDist::Normal => misaka_palw_gen::gauss_q24_v1(word as u16) as i128,
                });
            }
            values
        }
    };
    let (lo, hi) = d.interval();
    if values.len() != count || values.iter().any(|v| *v < lo || *v > hi) {
        return Err("input39 actual original source interval violated".into());
    }
    Ok(values)
}
fn original_input_tensor_v39(
    source: &CheckedSourceV39,
    position: u32,
    input: u8,
    mut range: impl FnMut(u32, u8, u64, usize) -> Result<Vec<i128>, String>,
) -> Result<misaka_palw_tir::Tensor, String> {
    let d = source.program().inputs.get(input as usize).ok_or("input39 original tensor declaration absent")?;
    let cells = d.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d as u64)).ok_or("input39 tensor product")?;
    if cells.checked_mul(16).is_none_or(|n| n > source.profile().max_input_expanded_bytes) {
        return Err("input39 tensor exceeds original expanded profile before allocation".into());
    }
    let count = usize::try_from(cells).map_err(|_| "input39 tensor platform extent")?;
    let mut data = Vec::new();
    data.try_reserve_exact(count).map_err(|_| "input39 bounded tensor reservation")?;
    let mut at = 0;
    while at < count {
        let n = (count - at).min(4096);
        let values = range(position, input, at as u64, n)?;
        if values.len() != n {
            return Err("input39 exact range extent".into());
        }
        data.extend(values);
        at += n;
    }
    misaka_palw_tir::Tensor::new(d.dtype, d.shape.iter().map(|d| *d as usize).collect(), data).map_err(|e| e.to_string())
}
impl ActualInputResolverV39<'_> {
    pub fn read_input_range(&self, position: u32, input: u8, offset: u64, count: usize) -> Result<Vec<i128>, String> {
        original_input_range_v39(self.source, self.maximum, self.reader, self.tokens.len() as u32, position, input, offset, count)
    }
    pub fn input_tensor(&self, position: u32, input: u8) -> Result<misaka_palw_tir::Tensor, String> {
        original_input_tensor_v39(self.source, position, input, |p, i, o, n| self.read_input_range(p, i, o, n))
    }
}
impl misaka_palw_tir::interp_v2::InputProvider for ActualInputResolverV39<'_> {
    fn input(&self, k: u16, pos: u32) -> Option<misaka_palw_tir::Tensor> {
        self.input_tensor(pos, u8::try_from(k).ok()?).ok()
    }
}
impl misaka_palw_tir::interp_v2::InputProvider for GenerationInputProviderV39<'_> {
    fn input(&self, k: u16, pos: u32) -> Option<misaka_palw_tir::Tensor> {
        self.input_tensor(pos, u8::try_from(k).ok()?).ok()
    }
}

impl JobMaximumV39 {
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("job39 Vec encoding")
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        let mut r = crate::flat_codec39::Reader39::new(raw, (8 << 20) + 4096)?;
        let version = r.get()?;
        if version != 39 {
            return Err("job39 owning version".into());
        }
        let source = r.get()?;
        let requester = r.get()?;
        let producer = r.get()?;
        let nonce = r.get()?;
        let stop = match r.get::<u8>()? {
            0 => RequestStopV39::Fixed { positions: r.get()?, tokens: r.words(1 << 21)? },
            1 => RequestStopV39::Decode { prompt: r.words(1 << 21)?, requested_new_tokens: r.get()?, eos: r.words(256)? },
            2 => RequestStopV39::Derived { maximum_positions: r.get()?, tokens: r.words(1 << 21)? },
            _ => return Err("job39 unknown original stop contract".into()),
        };
        let input_manifest = r.get()?;
        let value = Self { version, source, requester, producer, nonce, stop, input_manifest };
        r.finish(&value)?;
        value.stop.maximum_positions()?;
        Ok(value)
    }
}
impl MaterialManifestV39 {
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("manifest39 Vec encoding")
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        let mut r = crate::flat_codec39::Reader39::new(raw, 65536)?;
        let version = r.get()?;
        if version != 39 {
            return Err("manifest39 owning version".into());
        }
        let source = r.get()?;
        let master = r.get()?;
        let n = r.count(16, 1)?;
        let mut bindings = Vec::with_capacity(n);
        for _ in 0..n {
            bindings.push(match r.get::<u8>()? {
                0 => {
                    let descriptor = MaterialDescriptorV39::read_bounded(&mut r)?;
                    let selection = r.get()?;
                    InputBindingV39::Material { descriptor, selection }
                }
                1 => InputBindingV39::Random { seed: r.get()?, domain: r.get()?, per_step: r.get()? },
                _ => return Err("manifest39 unknown input binding tag".into()),
            });
        }
        let value = Self { version, source, master, bindings };
        r.finish(&value)?;
        Ok(value)
    }
}
