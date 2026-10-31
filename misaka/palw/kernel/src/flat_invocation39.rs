//! Public original Master39 request and exact per-component maximum projection.
//! These are structural checks, never native Final, UTXO or seed capabilities.
//! The native adapter authenticates every prior fact against genuine original
//! books/once-credit rows before invoking this independently derived projection.
use crate::{
    flat_job39::*,
    flat_material_output39::OutputProjectionV39,
    flat_material39::*,
    flat_pipeline39::{CheckedPipelineV39, PipelineV39},
    flat_source39::CheckedSourceV39,
    hash::{Digest, object_id},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::{
    DType, Tensor,
    pipeline::{Binding, TokenRule, TokenSource, TripRule},
    program_v2::InputSource,
};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MasterImageV39 {
    pub height: u32,
    pub width: u32,
    pub tensor: Digest,
    pub raw_source: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MasterProducerV39 {
    pub producer: Digest,
    pub operator: Digest,
    pub reward: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MasterFinalizedV39 {
    pub slot: u8,
    pub stage: u8,
    pub job: Digest,
    pub claim: Digest,
    pub credit: Digest,
    pub generated: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MasterPublicRequestV39 {
    pub version: u16,
    pub complete_source: Digest,
    pub requester: Digest,
    pub requester_operator: Digest,
    pub first_nonce: Digest,
    pub producers: Vec<MasterProducerV39>,
    pub prompt: Vec<u32>,
    pub negative: Vec<u32>,
    pub source: Vec<u32>,
    pub key: Vec<u32>,
    pub steps: u32,
    pub scalars: Vec<i64>,
    pub images: Vec<MasterImageV39>,
    pub seed: [u8; 32],
    pub requested_new_tokens: u32,
    pub eos: Vec<u32>,
    pub finalized: Vec<MasterFinalizedV39>,
}
impl MasterPublicRequestV39 {
    pub const MAX_BYTES: usize = (40 << 20);
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/original-public-master-request/v39", self)
    }
    pub fn check(&self, pipeline: &CheckedPipelineV39) -> Result<(), String> {
        if self.version != 39
            || [self.complete_source, self.requester, self.requester_operator].contains(&[0; 64])
            || self.producers.len() != pipeline.wire().stages.len()
            || self.producers.len() > 64
            || self.producers.iter().any(|p| {
                [p.producer, p.operator].contains(&[0; 64])
                    || p.producer == self.requester
                    || p.operator == self.requester_operator
                    || p.reward == 0
            })
            || [&self.prompt, &self.negative, &self.source, &self.key].iter().any(|v| v.len() > 1 << 21)
            || self.scalars.len() > 32
            || self.images.len() > 64
            || self.images.iter().any(|i| {
                i.height == 0 || i.width == 0 || i.height > 1 << 24 || i.width > 1 << 24 || [i.tensor, i.raw_source].contains(&[0; 64])
            })
            || self.eos.len() > 256
            || self.eos.windows(2).any(|w| w[0] >= w[1])
            || self.finalized.len() > 16
        {
            return Err("master39 original public request/component/operator/input extent".into());
        }
        crate::flat_execution11::canonical_nonce_v11(&self.first_nonce)?;
        if pipeline.wire().stages.iter().any(|s| matches!(s.trip, TripRule::Decode | TripRule::TextStream))
            && (self.prompt.is_empty() || self.requested_new_tokens == 0 || self.eos.is_empty())
        {
            return Err("master39 original requested maximum/firstEOS contract absent".into());
        }
        for (i, f) in self.finalized.iter().enumerate() {
            if f.generated.len() > 1 << 21
                || [f.job, f.claim, f.credit].contains(&[0; 64])
                || self.finalized[..i].iter().any(|p| p.slot == f.slot && p.stage == f.stage)
            {
                return Err("master39 original finalized token authority extent/alias".into());
            }
        }
        Ok(())
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        let mut r = crate::flat_codec39::Reader39::new(raw, Self::MAX_BYTES)?;
        let version = r.get()?;
        let complete_source = r.get()?;
        let requester = r.get()?;
        let requester_operator = r.get()?;
        let first_nonce = r.get()?;
        let n = r.count(64, 136)?;
        let mut producers = Vec::with_capacity(n);
        for _ in 0..n {
            producers.push(MasterProducerV39 { producer: r.get()?, operator: r.get()?, reward: r.get()? });
        }
        let prompt = r.words(1 << 21)?;
        let negative = r.words(1 << 21)?;
        let source = r.words(1 << 21)?;
        let key = r.words(1 << 21)?;
        let steps = r.get()?;
        let n = r.count(32, 8)?;
        let mut scalars = Vec::with_capacity(n);
        for _ in 0..n {
            scalars.push(r.get()?);
        }
        let n = r.count(64, 136)?;
        let mut images = Vec::with_capacity(n);
        for _ in 0..n {
            images.push(MasterImageV39 { height: r.get()?, width: r.get()?, tensor: r.get()?, raw_source: r.get()? });
        }
        let seed = r.get()?;
        let requested_new_tokens = r.get()?;
        let eos = r.words(256)?;
        let n = r.count(16, 198)?;
        let mut finalized = Vec::with_capacity(n);
        for _ in 0..n {
            finalized.push(MasterFinalizedV39 {
                slot: r.get()?,
                stage: r.get()?,
                job: r.get()?,
                claim: r.get()?,
                credit: r.get()?,
                generated: r.words(1 << 21)?,
            });
        }
        let value = Self {
            version,
            complete_source,
            requester,
            requester_operator,
            first_nonce,
            producers,
            prompt,
            negative,
            source,
            key,
            steps,
            scalars,
            images,
            seed,
            requested_new_tokens,
            eos,
            finalized,
        };
        r.finish(&value)?;
        Ok(value)
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("master39 Vec encoding")
    }
    pub fn stage_nonce(&self, stage: u16) -> Result<Digest, String> {
        let n = crate::flat_execution11::canonical_nonce_v11(&self.first_nonce)?
            .checked_add(stage as u64)
            .ok_or("master39 stage nonce overflow")?;
        let mut raw = [0; 64];
        raw[..8].copy_from_slice(&n.to_le_bytes());
        Ok(raw)
    }
}
/// Values selected by the native reader from independently Final original jobs.
/// This type alone carries no authority; native checks the stored original job,
/// Master stage, class/source, claim, generated root and single-credit identity.
pub struct PriorComponentV39<'a> {
    pub stage: u16,
    pub job: Digest,
    pub class: Digest,
    pub source: Digest,
    pub claim: Digest,
    pub final_book: Digest,
    pub single_credit: Digest,
    pub positions: u32,
    pub generated: &'a [u32],
}
fn source_ids<'a>(
    r: &'a MasterPublicRequestV39,
    source: TokenSource,
    prior: &'a [PriorComponentV39<'a>],
    pipeline: &PipelineV39,
) -> Result<&'a [u32], String> {
    Ok(match source {
        TokenSource::Prompt => &r.prompt,
        TokenSource::Negative => &r.negative,
        TokenSource::Source => &r.source,
        TokenSource::Key => &r.key,
        TokenSource::Generated => {
            let stage = pipeline
                .stages
                .iter()
                .position(|s| matches!(s.trip, TripRule::Decode))
                .ok_or("master39 original Decode stage absent")?;
            prior.iter().find(|p| p.stage as usize == stage).ok_or("master39 genuine decoded upstream Final absent")?.generated
        }
        TokenSource::FinalizedOutput { claim, stage } => {
            &r.finalized
                .iter()
                .find(|f| f.slot == claim && f.stage == stage)
                .ok_or("master39 original finalized token set absent")?
                .generated
        }
    })
}
fn rule(
    r: &MasterPublicRequestV39,
    t: &TokenRule,
    prior: &[PriorComponentV39<'_>],
    p: &PipelineV39,
) -> Result<(Vec<u32>, usize), String> {
    let ids = source_ids(r, t.source, prior, p)?;
    let n =
        t.prefix.len().checked_add(ids.len()).and_then(|n| n.checked_add(t.suffix.len())).ok_or("master39 token template overflow")?;
    let bound = t.pad.map_or(n, |pad| pad.to_len as usize);
    if n > bound || bound > 1 << 21 {
        return Err("master39 token template exceeds original bound".into());
    }
    let mut seq = Vec::with_capacity(bound);
    seq.extend_from_slice(&t.prefix);
    seq.extend_from_slice(ids);
    seq.extend_from_slice(&t.suffix);
    if let Some(pad) = t.pad {
        seq.resize(bound, pad.id);
    }
    Ok((seq, n))
}
fn scalar(dtype: DType, v: i128) -> Result<Digest, String> {
    let t = Tensor::scalar(dtype, v).map_err(|e| e.to_string())?;
    Ok(crate::merkle3::tensor_commitment_v3(&t))
}
fn expected_tensor(dtype: DType, shape: &[u32], ids: &[u32]) -> Result<Digest, String> {
    let t = Tensor::new(dtype, shape.iter().map(|d| *d as usize).collect(), ids.iter().map(|v| *v as i128).collect())
        .map_err(|e| e.to_string())?;
    Ok(crate::merkle3::tensor_commitment_v3(&t))
}
pub struct CheckedComponentInvocationV39 {
    master: Digest,
    stage: u16,
    maximum: Digest,
    input_manifest: Digest,
    projections: Vec<(u8, OutputProjectionV39)>,
}
impl CheckedComponentInvocationV39 {
    pub fn master(&self) -> Digest {
        self.master
    }
    pub fn stage(&self) -> u16 {
        self.stage
    }
    pub fn maximum(&self) -> Digest {
        self.maximum
    }
    pub fn input_manifest(&self) -> Digest {
        self.input_manifest
    }
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/original-component-invocation/v39",
            &(self.master, self.stage, self.maximum, self.input_manifest, &self.projections),
        )
    }
    pub fn projections(&self) -> &[(u8, OutputProjectionV39)] {
        &self.projections
    }
}
/// Per-component admission after upstream Final, before this component's Fund.
/// Requested maximum remains derived from the original master, independently
/// of an upstream decoder's shorter actual first-EOS output.
pub fn check_component_invocation_v39(
    master: Digest,
    r: &MasterPublicRequestV39,
    p: &CheckedPipelineV39,
    source: &CheckedSourceV39,
    stage: u16,
    max: &CheckedJobMaximumV39,
    prior: &[PriorComponentV39<'_>],
) -> Result<CheckedComponentInvocationV39, String> {
    r.check(p)?;
    let wire = p.wire();
    let s = wire.stages.get(stage as usize).ok_or("master39 original component absent")?;
    let producer = r.producers.get(stage as usize).ok_or("master39 original component producer absent")?;
    if master == [0; 64]
        || s.program != stage
        || max.request().requester != r.requester
        || max.request().producer != producer.producer
        || max.request().nonce != r.stage_nonce(stage)?
        || max.input_manifest().master != master
        || max.request().source != source.root()
        || max.input_manifest().bindings.len() != source.program().inputs.len()
    {
        return Err("master39 original component/max/principal/nonce binding differs".into());
    }
    let (tokens, positions, maximum_positions) = match s.trip {
        TripRule::TextStream | TripRule::Decode => {
            if max.request().stop
                != (RequestStopV39::Decode {
                    prompt: r.prompt.clone(),
                    requested_new_tokens: r.requested_new_tokens,
                    eos: r.eos.clone(),
                })
            {
                return Err("master39 original decoder maximum/EOS differs".into());
            }
            (vec![], 0, max.maximum_positions())
        }
        TripRule::Fixed { n } => (vec![0; n as usize], n, n),
        TripRule::JobSteps => (vec![0; r.steps as usize], r.steps, r.steps),
        TripRule::TokenCount => {
            let t = s.tokens.as_ref().ok_or("master39 original token template absent")?;
            let (seq, _) = rule(r, t, prior, wire)?;
            let original = if matches!(t.source, TokenSource::Generated) {
                t.pad.map_or(t.prefix.len() + r.requested_new_tokens as usize + t.suffix.len(), |pad| pad.to_len as usize)
            } else {
                seq.len()
            };
            let n = u32::try_from(original).map_err(|_| "master39 original requested template extent")?;
            (seq.clone(), seq.len() as u32, n)
        }
    };
    if !matches!(s.trip, TripRule::Decode | TripRule::TextStream) {
        match &max.request().stop {
            RequestStopV39::Fixed { positions: p, tokens: t } if *p == positions && maximum_positions == positions && *t == tokens => {
            }
            RequestStopV39::Derived { maximum_positions: p, tokens: t } if *p == maximum_positions && *t == tokens => {}
            _ => return Err("master39 fixed/derived actual tokens or original maximum differs".into()),
        }
    }
    if maximum_positions == 0 || maximum_positions > s.max_trip || max.maximum_positions() != maximum_positions {
        return Err("master39 original requested component trip differs".into());
    }
    let mut projections = Vec::new();
    let mut external = s.bind.iter();
    for (i, (declaration, input)) in source.program().inputs.iter().zip(&max.input_manifest().bindings).enumerate() {
        match (&declaration.source, input) {
            (InputSource::Random { domain, per_step, .. }, InputBindingV39::Random { seed, domain: actual, per_step: step })
                if *seed == r.seed && domain == actual && per_step == step => {}
            (InputSource::External { .. }, InputBindingV39::Material { descriptor, selection }) => {
                if !matches!(selection, MaterialSelectionV39::Whole) || descriptor.requester_operator != r.requester_operator {
                    return Err("master39 original material component projection differs".into());
                }
                let binding = external.next().ok_or("master39 canonical external binding absent")?;
                let expected = match binding {
                    Binding::JobScalar { index } => {
                        Some(scalar(declaration.dtype, *r.scalars.get(*index as usize).ok_or("master39 scalar absent")? as i128)?)
                    }
                    Binding::JobTokens { rule: t } => {
                        let (seq, _) = rule(r, t, prior, wire)?;
                        Some(expected_tensor(declaration.dtype, &declaration.shape, &seq)?)
                    }
                    Binding::JobTokenCount { rule: t } => Some(scalar(declaration.dtype, rule(r, t, prior, wire)?.1 as i128)?),
                    Binding::JobImage { index } => {
                        let image = r.images.get(*index as usize).ok_or("master39 original image absent")?;
                        if declaration.dtype != DType::I16 || declaration.shape != vec![image.height, image.width, 3] {
                            return Err("master39 original image declaration differs".into());
                        }
                        match &descriptor.origin {
                            MaterialOriginV39::External { raw_source, .. } if *raw_source == image.raw_source => {}
                            _ => return Err("master39 original image source differs".into()),
                        }
                        Some(image.tensor)
                    }
                    Binding::StageRowCount { stage: from, drop } => {
                        let q = prior.iter().find(|p| p.stage == *from as u16).ok_or("master39 genuine upstream row count absent")?;
                        let rows_from = if matches!(wire.stages[*from as usize].trip, TripRule::Decode) {
                            r.prompt.len().saturating_sub(1) as u32
                        } else {
                            0
                        };
                        Some(scalar(declaration.dtype, q.positions.saturating_sub(rows_from).saturating_sub(*drop) as i128)?)
                    }
                    Binding::StageRows { stage: from, .. } | Binding::StageFinal { stage: from } => {
                        let q = prior.iter().find(|p| p.stage == *from as u16).ok_or("master39 genuine upstream component absent")?;
                        match &descriptor.origin {
                            MaterialOriginV39::Upstream {
                                master: m,
                                stage: st,
                                job,
                                class,
                                source,
                                claim,
                                final_book,
                                single_credit,
                                ..
                            } if *m == master
                                && *st == q.stage
                                && *job == q.job
                                && *class == q.class
                                && *source == q.source
                                && *claim == q.claim
                                && *final_book == q.final_book
                                && *single_credit == q.single_credit => {}
                            _ => return Err("master39 material borrows another upstream original Final/credit".into()),
                        }
                        let projection = match binding {
                            Binding::StageRows { drop, pad_to, .. } => OutputProjectionV39::Rows {
                                rows_from: if matches!(wire.stages[*from as usize].trip, TripRule::Decode) {
                                    r.prompt.len().saturating_sub(1) as u32
                                } else {
                                    0
                                },
                                drop: *drop,
                                pad_to: Some(*pad_to),
                            },
                            _ => OutputProjectionV39::Whole,
                        };
                        projections.push((i as u8, projection));
                        None
                    }
                };
                if let Some(expected) = expected
                    && (descriptor.tensor != expected || !matches!(descriptor.origin, MaterialOriginV39::External { .. }))
                {
                    return Err("master39 public input tensor does not implement original pipeline binding".into());
                }
            }
            _ => return Err("master39 random/external canonical input binding differs".into()),
        }
    }
    if external.next().is_some() {
        return Err("master39 unused original external binding".into());
    }
    Ok(CheckedComponentInvocationV39 { master, stage, maximum: max.root(), input_manifest: max.input_manifest().root(), projections })
}

// ---- The request composer (RFC-0002 §II.14, "real request composer") ----
//
// The exact inverse of `check_component_invocation_v39`, written with the SAME
// private helpers (`rule`, `scalar`, `expected_tensor`, `source_ids`) so the
// producer side and the native check cannot drift. It derives each stage's
// original maximum length, stop rule and every input's material shape from the
// complete public request and the genuinely Final upstream components the
// caller authenticated (the native reader passes the same `prior`). It never
// assumes an unknown upstream output or a future decoded token: a stage whose
// tokens come from a decoder that is not yet Final, or whose input is an
// upstream output that is not Final, is refused, not guessed.

/// One composed input, in the source program's declared input order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComposedInputV39 {
    /// Fully determined by the request seed and the declaration.
    Random { seed: [u8; 32], domain: u16, per_step: bool },
    /// Values derivable from the public request alone (scalars, token
    /// templates, token counts, upstream row counts). `values` are the exact
    /// elements of the declared tensor; `tensor` is their C_v3 commitment.
    Public { input: u8, dtype: DType, shape: Vec<u32>, values: Vec<i128>, tensor: Digest },
    /// A request image: the request carries only its commitment and raw source.
    Image { input: u8, dtype: DType, shape: Vec<u32>, tensor: Digest, raw_source: Digest },
    /// An earlier genuinely Final component's output, under `projection`.
    Upstream { input: u8, from: u16, projection: OutputProjectionV39 },
}
/// The composed, not yet material-bound, invocation of one component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComposedComponentV39 {
    pub master: Digest,
    pub stage: u16,
    pub source: Digest,
    pub original_task: Digest,
    pub requester: Digest,
    pub requester_operator: Digest,
    pub producer: Digest,
    pub nonce: Digest,
    pub stop: RequestStopV39,
    pub maximum_positions: u32,
    pub inputs: Vec<ComposedInputV39>,
}
/// Identity of a composed public input's raw source (not checked by the native
/// rule for non-image inputs, but deterministic so two composers agree).
pub fn composed_public_raw_source_v39(request: &MasterPublicRequestV39, stage: u16, input: u8) -> Digest {
    object_id(b"misaka-palw/kernel/composed-public-request-input/v39", &(request.root(), stage, input))
}
pub fn compose_component_request_v39(
    master: Digest,
    r: &MasterPublicRequestV39,
    p: &CheckedPipelineV39,
    source: &CheckedSourceV39,
    stage: u16,
    prior: &[PriorComponentV39<'_>],
) -> Result<ComposedComponentV39, String> {
    r.check(p)?;
    let wire = p.wire();
    let s = wire.stages.get(stage as usize).ok_or("compose39 original component absent")?;
    let producer = r.producers.get(stage as usize).ok_or("compose39 original component producer absent")?;
    if master == [0; 64] || s.program != stage {
        return Err("compose39 original Master/component order differs".into());
    }
    for q in prior {
        if q.stage >= stage || prior.iter().filter(|o| o.stage == q.stage).count() != 1 {
            return Err("compose39 prior components must be distinct earlier genuine Finals".into());
        }
    }
    let (stop, maximum_positions) = match s.trip {
        TripRule::TextStream | TripRule::Decode => {
            let stop =
                RequestStopV39::Decode { prompt: r.prompt.clone(), requested_new_tokens: r.requested_new_tokens, eos: r.eos.clone() };
            let n = stop.maximum_positions()?;
            (stop, n)
        }
        TripRule::Fixed { n } => (RequestStopV39::Fixed { positions: n, tokens: vec![0; n as usize] }, n),
        TripRule::JobSteps => (RequestStopV39::Fixed { positions: r.steps, tokens: vec![0; r.steps as usize] }, r.steps),
        TripRule::TokenCount => {
            let t = s.tokens.as_ref().ok_or("compose39 original token template absent")?;
            // A Generated template exists only once its decoder is Final.
            let (seq, _) = rule(r, t, prior, wire)?;
            if matches!(t.source, TokenSource::Generated) {
                let original =
                    t.pad.map_or(t.prefix.len() + r.requested_new_tokens as usize + t.suffix.len(), |pad| pad.to_len as usize);
                let n = u32::try_from(original).map_err(|_| "compose39 original requested template extent")?;
                if seq.len() == n as usize {
                    (RequestStopV39::Fixed { positions: n, tokens: seq }, n)
                } else {
                    (RequestStopV39::Derived { maximum_positions: n, tokens: seq }, n)
                }
            } else {
                let n = u32::try_from(seq.len()).map_err(|_| "compose39 template extent")?;
                (RequestStopV39::Fixed { positions: n, tokens: seq }, n)
            }
        }
    };
    if maximum_positions == 0 || maximum_positions > s.max_trip || maximum_positions > source.original_context_max() {
        return Err("compose39 original requested component trip exceeds the Master or source context".into());
    }
    let mut inputs = Vec::with_capacity(source.program().inputs.len());
    let mut external = s.bind.iter();
    for (i, declaration) in source.program().inputs.iter().enumerate() {
        let input = u8::try_from(i).map_err(|_| "compose39 input index extent")?;
        match &declaration.source {
            InputSource::Random { domain, per_step, .. } => {
                inputs.push(ComposedInputV39::Random { seed: r.seed, domain: *domain, per_step: *per_step })
            }
            InputSource::External { .. } => {
                let binding = external.next().ok_or("compose39 canonical external binding absent")?;
                let public = |values: Vec<i128>| -> Result<ComposedInputV39, String> {
                    let t = Tensor::new(declaration.dtype, declaration.shape.iter().map(|d| *d as usize).collect(), values.clone())
                        .map_err(|e| e.to_string())?;
                    Ok(ComposedInputV39::Public {
                        input,
                        dtype: declaration.dtype,
                        shape: declaration.shape.clone(),
                        values,
                        tensor: crate::merkle3::tensor_commitment_v3(&t),
                    })
                };
                inputs.push(match binding {
                    Binding::JobScalar { index } => {
                        let v = *r.scalars.get(*index as usize).ok_or("compose39 scalar absent")? as i128;
                        // Same commitment as the native rule's `scalar`.
                        let c = public(vec![v])?;
                        if let ComposedInputV39::Public { tensor, .. } = &c
                            && *tensor != scalar(declaration.dtype, v)?
                        {
                            return Err("compose39 scalar declaration is not rank-0".into());
                        }
                        c
                    }
                    Binding::JobTokens { rule: t } => {
                        let (seq, _) = rule(r, t, prior, wire)?;
                        let c = public(seq.iter().map(|v| *v as i128).collect())?;
                        if let ComposedInputV39::Public { tensor, .. } = &c
                            && *tensor != expected_tensor(declaration.dtype, &declaration.shape, &seq)?
                        {
                            return Err("compose39 token tensor differs from the native rule".into());
                        }
                        c
                    }
                    Binding::JobTokenCount { rule: t } => public(vec![rule(r, t, prior, wire)?.1 as i128])?,
                    Binding::JobImage { index } => {
                        let image = r.images.get(*index as usize).ok_or("compose39 original image absent")?;
                        if declaration.dtype != DType::I16 || declaration.shape != vec![image.height, image.width, 3] {
                            return Err("compose39 original image declaration differs".into());
                        }
                        ComposedInputV39::Image {
                            input,
                            dtype: declaration.dtype,
                            shape: declaration.shape.clone(),
                            tensor: image.tensor,
                            raw_source: image.raw_source,
                        }
                    }
                    Binding::StageRowCount { stage: from, drop } => {
                        let q =
                            prior.iter().find(|p| p.stage == *from as u16).ok_or("compose39 upstream row count is not Final yet")?;
                        let rows_from = if matches!(wire.stages[*from as usize].trip, TripRule::Decode) {
                            r.prompt.len().saturating_sub(1) as u32
                        } else {
                            0
                        };
                        public(vec![q.positions.saturating_sub(rows_from).saturating_sub(*drop) as i128])?
                    }
                    Binding::StageRows { stage: from, drop, pad_to } => {
                        prior.iter().find(|p| p.stage == *from as u16).ok_or("compose39 upstream rows are not Final yet")?;
                        let rows_from = if matches!(wire.stages[*from as usize].trip, TripRule::Decode) {
                            r.prompt.len().saturating_sub(1) as u32
                        } else {
                            0
                        };
                        ComposedInputV39::Upstream {
                            input,
                            from: *from as u16,
                            projection: OutputProjectionV39::Rows { rows_from, drop: *drop, pad_to: Some(*pad_to) },
                        }
                    }
                    Binding::StageFinal { stage: from } => {
                        prior.iter().find(|p| p.stage == *from as u16).ok_or("compose39 upstream output is not Final yet")?;
                        ComposedInputV39::Upstream { input, from: *from as u16, projection: OutputProjectionV39::Whole }
                    }
                });
            }
        }
    }
    if external.next().is_some() {
        return Err("compose39 unused original external binding".into());
    }
    Ok(ComposedComponentV39 {
        master,
        stage,
        source: source.root(),
        original_task: source.original_task_metadata(),
        requester: r.requester,
        requester_operator: r.requester_operator,
        producer: producer.producer,
        nonce: r.stage_nonce(stage)?,
        stop,
        maximum_positions,
        inputs,
    })
}
impl ComposedComponentV39 {
    /// The external origin a composed public input's material must carry.
    pub fn public_origin(&self, request: &MasterPublicRequestV39, input: u8) -> MaterialOriginV39 {
        MaterialOriginV39::External {
            original_task: self.original_task,
            raw_source: composed_public_raw_source_v39(request, self.stage, input),
        }
    }
    /// Bind actual material descriptors (built from files/upstream
    /// certificates) to the composition, in declared input order. Every
    /// descriptor is checked against what the request implies; the result is
    /// the exact maximum and manifest the native rule then re-checks.
    pub fn finish(&self, descriptors: &[MaterialDescriptorV39]) -> Result<(JobMaximumV39, MaterialManifestV39), String> {
        let mut next = descriptors.iter();
        let mut bindings = Vec::with_capacity(self.inputs.len());
        for c in &self.inputs {
            let (input, expected) = match c {
                ComposedInputV39::Random { seed, domain, per_step } => {
                    bindings.push(InputBindingV39::Random { seed: *seed, domain: *domain, per_step: *per_step });
                    continue;
                }
                ComposedInputV39::Public { input, tensor, .. } | ComposedInputV39::Image { input, tensor, .. } => {
                    (*input, Some(*tensor))
                }
                ComposedInputV39::Upstream { input, .. } => (*input, None),
            };
            let d = next.next().ok_or("compose39 material descriptor absent")?;
            if d.version != 39
                || d.input != input
                || d.source != self.source
                || d.master != self.master
                || d.requester != self.requester
                || d.requester_operator != self.requester_operator
                || d.nonce != self.nonce
            {
                return Err("compose39 descriptor identity/principal/nonce/input differs".into());
            }
            match (c, &d.origin) {
                (ComposedInputV39::Public { dtype, shape, .. }, MaterialOriginV39::External { original_task, .. }) => {
                    if d.dtype != *dtype || d.shape != *shape || Some(d.tensor) != expected || *original_task != self.original_task {
                        return Err("compose39 public material differs from the request-derived tensor".into());
                    }
                }
                (
                    ComposedInputV39::Image { dtype, shape, raw_source, .. },
                    MaterialOriginV39::External { original_task, raw_source: r },
                ) => {
                    if d.dtype != *dtype
                        || d.shape != *shape
                        || Some(d.tensor) != expected
                        || r != raw_source
                        || *original_task != self.original_task
                    {
                        return Err("compose39 image material differs from the request commitment".into());
                    }
                }
                (ComposedInputV39::Upstream { from, .. }, MaterialOriginV39::Upstream { master, stage, .. }) => {
                    if *master != self.master || stage != from {
                        return Err("compose39 upstream material names another Master/stage".into());
                    }
                }
                _ => return Err("compose39 material origin kind differs from the composed input".into()),
            }
            bindings.push(InputBindingV39::Material { descriptor: d.clone(), selection: MaterialSelectionV39::Whole });
        }
        if next.next().is_some() {
            return Err("compose39 surplus material descriptor".into());
        }
        let manifest = MaterialManifestV39 { version: 39, source: self.source, master: self.master, bindings };
        let maximum = JobMaximumV39 {
            version: 39,
            source: self.source,
            requester: self.requester,
            producer: self.producer,
            nonce: self.nonce,
            stop: self.stop.clone(),
            input_manifest: manifest.root(),
        };
        Ok((maximum, manifest))
    }
}
/// Compose, bind and re-check with the native rule in one call. Returns the
/// checked maximum only if `check_component_invocation_v39` accepts it.
pub fn compose_and_check_component_v39(
    master: Digest,
    r: &MasterPublicRequestV39,
    p: &CheckedPipelineV39,
    source: &CheckedSourceV39,
    stage: u16,
    prior: &[PriorComponentV39<'_>],
    descriptors: &[MaterialDescriptorV39],
) -> Result<(CheckedJobMaximumV39, CheckedComponentInvocationV39), String> {
    let composed = compose_component_request_v39(master, r, p, source, stage, prior)?;
    let (maximum, manifest) = composed.finish(descriptors)?;
    let checked = CheckedJobMaximumV39::check(source, &maximum, &manifest)?;
    let invocation = check_component_invocation_v39(master, r, p, source, stage, &checked, prior)?;
    Ok((checked, invocation))
}
