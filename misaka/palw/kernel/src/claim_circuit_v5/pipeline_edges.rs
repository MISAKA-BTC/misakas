//! Real pipeline edge semantics over the upstream circuit wires and public job
//! values. No host evaluation of a witness-valued upstream tensor is used.
//! The future whole-pipeline constructor must bind job images/finalized outputs
//! to native job authority before invoking these helpers; this is not admission.
use super::{
    scalar::IntV5,
    tensor::{CircuitShapeV5, CircuitTensorV5},
    *,
};
use misaka_palw_tir::{
    DType,
    pipeline::{Binding, PipelineJob, StageDecl, TokenRule, TokenSource, TripRule},
    program_v2::{InputDecl, InputSource, OutputDecl, RandStepRule, RandomDist},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineEdgeLimitsV5 {
    pub maximum_output_elements: u64,
    pub maximum_template_tokens: u64,
    pub maximum_image_bytes: u64,
    pub maximum_random_hashes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipelineEdgeCostV5 {
    pub output_elements: u64,
    pub public_random_hashes: u64,
    pub public_template_tokens: u64,
    pub public_image_bytes: u64,
}
/// Borrowed stage output handles from the shared graph. A stage's metadata is
/// checked against its actual row inventory, without cloning any tensor.
pub struct PipelineStageWiresV5<'a> {
    pub output: OutputDecl,
    pub trip: u32,
    pub rows_from: u32,
    pub rows: &'a [&'a CircuitTensorV5<'a>],
}
fn source<'a>(rule: &TokenRule, job: &'a PipelineJob) -> Result<&'a [u32], String> {
    Ok(match rule.source {
        TokenSource::Prompt => &job.prompt,
        TokenSource::Negative => &job.negative,
        TokenSource::Source => &job.source,
        TokenSource::Generated => &job.generated,
        TokenSource::Key => &job.key,
        TokenSource::FinalizedOutput { claim, stage } => {
            job.finalized.get(&(claim, stage)).ok_or("native finalized token output absent")?
        }
    })
}
fn lengths(rule: &TokenRule, job: &PipelineJob, limits: PipelineEdgeLimitsV5) -> Result<(usize, usize), String> {
    let unpadded = rule
        .prefix
        .len()
        .checked_add(source(rule, job)?.len())
        .and_then(|n| n.checked_add(rule.suffix.len()))
        .ok_or("pipeline token template length overflow")?;
    let total = rule.pad.map_or(unpadded, |pad| pad.to_len as usize);
    if total < unpadded || total as u64 > limits.maximum_template_tokens {
        return Err("pipeline token template exceeds admitted pad/cap".into());
    }
    Ok((unpadded, total))
}
fn token(rule: &TokenRule, job: &PipelineJob, at: usize) -> Result<u32, String> {
    let ids = source(rule, job)?;
    if at < rule.prefix.len() {
        return Ok(rule.prefix[at]);
    }
    let at = at - rule.prefix.len();
    if at < ids.len() {
        return Ok(ids[at]);
    }
    let at = at - ids.len();
    if at < rule.suffix.len() {
        return Ok(rule.suffix[at]);
    }
    rule.pad.map(|pad| pad.id).ok_or("pipeline token beyond unpadded template".into())
}
/// Allocation-free native trip and row-start derivation. Token material is
/// available through `stage_token_v5`, rather than an unbounded temporary Vec.
pub fn stage_run_v5(stage: &StageDecl, job: &PipelineJob, limits: PipelineEdgeLimitsV5) -> Result<(u32, u32), String> {
    let tokens = if let Some(rule) = &stage.tokens {
        lengths(rule, job, limits)?.1
    } else if matches!(stage.trip, TripRule::TextStream | TripRule::Decode) {
        job.prompt.len().checked_add(job.generated.len().saturating_sub(1)).ok_or("stream token length overflow")?
    } else {
        0
    };
    if tokens as u64 > limits.maximum_template_tokens {
        return Err("stage token count cap exceeded".into());
    }
    let trip = match stage.trip {
        TripRule::Fixed { n } => n,
        TripRule::JobSteps => job.steps,
        TripRule::TokenCount | TripRule::TextStream | TripRule::Decode => u32::try_from(tokens).map_err(|_| "stage trip overflow")?,
    };
    if trip == 0 || trip > stage.max_trip {
        return Err("stage trip outside registered bounds".into());
    }
    let from = if matches!(stage.trip, TripRule::TextStream | TripRule::Decode) {
        u32::try_from(job.prompt.len()).map_err(|_| "stream prompt length overflow")?.saturating_sub(1)
    } else {
        0
    };
    Ok((trip, from))
}
pub fn stage_token_v5(stage: &StageDecl, job: &PipelineJob, position: u32, limits: PipelineEdgeLimitsV5) -> Result<u32, String> {
    let (trip, _) = stage_run_v5(stage, job, limits)?;
    if position >= trip {
        return Err("stage token position exceeds trip".into());
    }
    if let Some(rule) = &stage.tokens {
        if position as usize >= lengths(rule, job, limits)?.1 {
            return Err("stage token template shorter than trip".into());
        }
        token(rule, job, position as usize)
    } else if matches!(stage.trip, TripRule::TextStream | TripRule::Decode) {
        let at = position as usize;
        if at < job.prompt.len() {
            Ok(job.prompt[at])
        } else {
            job.generated.get(at - job.prompt.len()).copied().ok_or("stream generated token absent".into())
        }
    } else {
        Ok(0)
    }
}
fn shape(decl: &InputDecl, limits: PipelineEdgeLimitsV5) -> Result<([usize; 4], usize, usize), String> {
    if decl.shape.len() > 4 {
        return Err("pipeline input rank exceeds TIR-v2".into());
    }
    let mut dims = [0usize; 4];
    for (i, d) in decl.shape.iter().enumerate() {
        dims[i] = *d as usize;
    }
    let count = CircuitShapeV5::new(&dims[..decl.shape.len()])?.elements();
    if count as u64 > limits.maximum_output_elements {
        return Err("pipeline edge output element cap exceeded".into());
    }
    Ok((dims, decl.shape.len(), count))
}
fn interval(builder: &mut CircuitBuilderV5, decl: &InputDecl, value: &IntV5) -> Result<(), String> {
    let (lo, hi) = decl.interval();
    builder.assert_interval(value, lo, hi)
}
/// All seven external binding kinds. StageRows/StageFinal copy upstream wires;
/// scalar/token/count/image values are exact public job constants. An image's
/// byte authority remains a native job/manifest obligation of the caller.
pub fn emit_external_input_v5<'a>(
    builder: &mut CircuitBuilderV5,
    decl: &InputDecl,
    binding: &Binding,
    job: &PipelineJob,
    current_stage: usize,
    upstream: &[PipelineStageWiresV5<'_>],
    limits: PipelineEdgeLimitsV5,
    memory: &CircuitMemoryBudgetV5,
) -> Result<(CircuitTensorV5<'a>, PipelineEdgeCostV5), String> {
    let InputSource::External { lo, hi } = decl.source else {
        return Err("external edge on random input".into());
    };
    if lo > hi || !decl.dtype.contains(lo as i128) || !decl.dtype.contains(hi as i128) {
        return Err("pipeline external interval malformed".into());
    }
    let (dims, rank, elements) = shape(decl, limits)?;
    let dims = &dims[..rank];
    let mut cost = PipelineEdgeCostV5 { output_elements: elements as u64, ..Default::default() };
    let up = |stage: u8| -> Result<&PipelineStageWiresV5<'_>, String> {
        if stage as usize >= current_stage {
            return Err("pipeline edge refers forward".into());
        }
        let up = upstream.get(stage as usize).ok_or("upstream stage absent")?;
        if up.trip == 0 || up.rows.len() != up.trip as usize {
            return Err("upstream row inventory differs from actual trip".into());
        }
        Ok(up)
    };
    // Validate complete shapes and public capacities before output allocation.
    match binding {
        Binding::JobScalar { index } => {
            if !dims.is_empty() || job.scalars.get(*index as usize).is_none() {
                return Err("pipeline scalar edge shape/index".into());
            }
        }
        Binding::JobTokens { rule } => {
            let (_, total) = lengths(rule, job, limits)?;
            if decl.dtype != DType::Idx || dims != [total] {
                return Err("pipeline token edge dtype/shape".into());
            }
            cost.public_template_tokens = total as u64;
        }
        Binding::JobTokenCount { rule } => {
            if decl.dtype != DType::Idx || !dims.is_empty() {
                return Err("pipeline token count edge shape".into());
            }
            cost.public_template_tokens = lengths(rule, job, limits)?.1 as u64;
        }
        Binding::StageRows { stage, drop, pad_to } => {
            let up = up(*stage)?;
            if matches!(up.output, OutputDecl::Final { .. })
                || dims.first() != Some(&(*pad_to as usize))
                || up.trip.saturating_sub(up.rows_from).saturating_sub(*drop) > *pad_to
            {
                return Err("pipeline rows edge kind/pad".into());
            }
            for row in up.rows {
                if row.dtype() != decl.dtype || row.shape().dims() != &dims[1..] {
                    return Err("upstream rows differ from input row type".into());
                }
            }
        }
        Binding::StageFinal { stage } => {
            let up = up(*stage)?;
            let row = up.rows.last().ok_or("upstream final absent")?;
            if !matches!(up.output, OutputDecl::Final { .. }) || row.dtype() != decl.dtype || row.shape().dims() != dims {
                return Err("pipeline final edge kind/type".into());
            }
        }
        Binding::StageRowCount { stage, .. } => {
            up(*stage)?;
            if decl.dtype != DType::Idx || !dims.is_empty() {
                return Err("pipeline row count shape".into());
            }
        }
        Binding::JobImage { index } => {
            let image = job.images.get(*index as usize).ok_or("native job image absent")?;
            let bytes =
                u64::from(image.h).checked_mul(u64::from(image.w)).and_then(|n| n.checked_mul(3)).ok_or("job image byte overflow")?;
            if bytes > limits.maximum_image_bytes
                || bytes != image.rgb.len() as u64
                || decl.dtype != DType::I16
                || dims != [image.h as usize, image.w as usize, 3]
            {
                return Err("native job image shape/byte cap".into());
            }
            cost.public_image_bytes = bytes;
        }
    }
    let value = CircuitTensorV5::from_word_stream(builder, decl.dtype, dims, memory, |b, index| {
        let word = match binding {
            Binding::JobScalar { index } => b.integer(job.scalars[*index as usize] as i128),
            Binding::JobTokens { rule } => b.integer(token(rule, job, index)? as i128),
            Binding::JobTokenCount { rule } => b.integer(lengths(rule, job, limits)?.0 as i128),
            Binding::StageRowCount { stage, drop } => {
                let up = up(*stage)?;
                b.integer(up.trip.saturating_sub(up.rows_from).saturating_sub(*drop) as i128)
            }
            Binding::JobImage { index: image } => b.integer(job.images[*image as usize].rgb[index] as i128),
            Binding::StageFinal { stage } => up(*stage)?.rows.last().ok_or("upstream final absent")?.at(index)?,
            Binding::StageRows { stage, drop, .. } => {
                let up = up(*stage)?;
                let width = elements / dims[0];
                let row = (up.rows_from as usize).saturating_add(*drop as usize).saturating_add(index / width);
                if row < up.rows.len() { up.rows[row].at(index % width)? } else { b.integer(0) }
            }
        };
        interval(b, decl, &word)?;
        Ok(word)
    })?;
    Ok((value, cost))
}
/// Random values are public functions of the real seed/item/position, not
/// witness-selected values. Use the native R implementation one digest block
/// at a time, with its actual hash count admitted before allocating output.
/// This public computation is charged separately; no sublinear claim is made.
pub fn emit_random_input_v5<'a>(
    builder: &mut CircuitBuilderV5,
    decl: &InputDecl,
    random: crate::pipeline_public::PipelineRandomV1,
    position: u32,
    limits: PipelineEdgeLimitsV5,
    memory: &CircuitMemoryBudgetV5,
) -> Result<(CircuitTensorV5<'a>, PipelineEdgeCostV5), String> {
    let InputSource::Random { domain, dist, per_step } = decl.source else {
        return Err("random edge on external input".into());
    };
    let (bits, rule) = misaka_palw_tir::program_v2::random_input_domain_v2(domain).ok_or("unknown TIR random domain")?;
    if match rule {
        RandStepRule::Zero => per_step,
        RandStepRule::PerStep => !per_step,
        RandStepRule::Declared => false,
    } {
        return Err("random input contradicts native step rule".into());
    }
    match dist {
        RandomDist::Uniform { bits: declared } if declared == bits && decl.dtype == DType::Idx => {}
        RandomDist::Normal if bits == 16 && decl.dtype == DType::I32 => {}
        _ => return Err("random distribution/domain/dtype mismatch".into()),
    }
    let domain = misaka_palw_gen::rand::rand_domain_v1(domain).ok_or("native random domain absent")?;
    let (dims, rank, elements) = shape(decl, limits)?;
    let words = domain.words_per_digest() as usize;
    if words == 0 {
        return Err("native random domain has zero block width".into());
    }
    let hashes = elements.div_ceil(words) as u64;
    if hashes > limits.maximum_random_hashes {
        return Err("public random hash cap exceeded before output allocation".into());
    }
    let step = if per_step { position } else { 0 };
    let mut cached = None;
    let mut digest = [0u8; 64];
    let value = CircuitTensorV5::from_word_stream(builder, decl.dtype, &dims[..rank], memory, |b, index| {
        let block = (index / words) as u64;
        if cached != Some(block) {
            digest = misaka_palw_gen::rand::rand_digest_v1(domain, &random.seed, step, random.item, block);
            cached = Some(block);
        }
        let word = misaka_palw_gen::rand::digest_word_v1(&digest, domain.word_bits, (index % words) as u32)
            .ok_or("native random word absent")?;
        let value = match dist {
            RandomDist::Uniform { .. } => word as i128,
            RandomDist::Normal => misaka_palw_gen::gauss_q24_v1(word as u16) as i128,
        };
        let value = b.integer(value);
        interval(b, decl, &value)?;
        Ok(value)
    })?;
    Ok((value, PipelineEdgeCostV5 { output_elements: elements as u64, public_random_hashes: hashes, ..Default::default() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::{
        Tensor,
        pipeline::{JobImageV1, RandomSource},
    };
    fn limits() -> PipelineEdgeLimitsV5 {
        PipelineEdgeLimitsV5 {
            maximum_output_elements: 1000,
            maximum_template_tokens: 1000,
            maximum_image_bytes: 3000,
            maximum_random_hashes: 100,
        }
    }
    fn graph_limits() -> CircuitLimitsV5 {
        CircuitLimitsV5 {
            maximum_gates: 1_000_000,
            maximum_input_bits: 10000,
            maximum_assertions: 100_000,
            maximum_graph_bytes: 20_000_000,
            maximum_witness_bytes: 1_000_000,
        }
    }
    fn decl(dtype: DType, shape: &[u32]) -> InputDecl {
        InputDecl {
            name: "edge".into(),
            dtype,
            shape: shape.to_vec(),
            source: InputSource::External { lo: dtype.min_value() as i64, hi: dtype.max_value() as i64 },
        }
    }
    fn assert_tensor(b: &mut CircuitBuilderV5, actual: &CircuitTensorV5<'_>, expected: &Tensor) -> Result<(), String> {
        if actual.dtype() != expected.dtype || actual.shape().dims() != expected.shape {
            return Err("edge output metadata mismatch".into());
        }
        for (i, value) in expected.data.iter().enumerate() {
            b.assert_integer_equal(&actual.at(i)?, &b.integer(*value))?;
        }
        Ok(())
    }
    fn check(binding: Binding, input: InputDecl, job: &PipelineJob, rows: &[Tensor], kind: OutputDecl, expected: Tensor) {
        let memory = CircuitMemoryBudgetV5::new(25_000_000);
        let emit = |b: &mut CircuitBuilderV5| -> Result<(), String> {
            let rows: Vec<_> = rows.iter().map(|t| CircuitTensorV5::input(b, t.dtype, &t.shape)).collect::<Result<_, _>>()?;
            let refs: Vec<_> = rows.iter().collect();
            let upstream = [PipelineStageWiresV5 { output: kind, trip: rows.len() as u32, rows_from: 0, rows: &refs }];
            let (actual, _) = emit_external_input_v5(b, &input, &binding, job, 1, &upstream, limits(), &memory)?;
            assert_tensor(b, &actual, &expected)
        };
        let mut count = CircuitBuilderV5::preflight(graph_limits()).unwrap();
        emit(&mut count).unwrap();
        let mut b = CircuitBuilderV5::new_planned(graph_limits(), count.preflight_cost().unwrap(), memory.clone()).unwrap();
        emit(&mut b).unwrap();
        let graph = b.finish_reserved().unwrap();
        let bytes: Vec<_> = rows.iter().flat_map(Tensor::to_le_bytes).collect();
        graph.evaluate_bytes(&bytes, graph_limits()).unwrap();
        drop(graph);
        assert_eq!(memory.held(), 0);
    }
    #[test]
    fn all_seven_external_edges_use_real_public_values_or_upstream_wires() {
        let job = PipelineJob {
            prompt: vec![7, 8],
            scalars: vec![-3],
            images: vec![JobImageV1 { h: 1, w: 1, rgb: vec![1, 2, 3] }],
            ..Default::default()
        };
        let rule = TokenRule {
            prefix: vec![1],
            source: TokenSource::Prompt,
            suffix: vec![2],
            pad: Some(misaka_palw_tir::pipeline::TokenPad { id: 0, to_len: 6 }),
        };
        let empty = OutputDecl::Rows { node: 0 };
        check(Binding::JobScalar { index: 0 }, decl(DType::I8, &[]), &job, &[], empty, Tensor::scalar(DType::I8, -3).unwrap());
        check(
            Binding::JobTokens { rule: rule.clone() },
            decl(DType::Idx, &[6]),
            &job,
            &[],
            empty,
            Tensor::new(DType::Idx, vec![6], vec![1, 7, 8, 2, 0, 0]).unwrap(),
        );
        check(Binding::JobTokenCount { rule }, decl(DType::Idx, &[]), &job, &[], empty, Tensor::scalar(DType::Idx, 4).unwrap());
        let rows = [Tensor::scalar(DType::I8, -1).unwrap(), Tensor::scalar(DType::I8, 2).unwrap()];
        check(
            Binding::StageRows { stage: 0, drop: 0, pad_to: 3 },
            decl(DType::I8, &[3]),
            &job,
            &rows,
            empty,
            Tensor::new(DType::I8, vec![3], vec![-1, 2, 0]).unwrap(),
        );
        check(
            Binding::StageRowCount { stage: 0, drop: 1 },
            decl(DType::Idx, &[]),
            &job,
            &rows,
            empty,
            Tensor::scalar(DType::Idx, 1).unwrap(),
        );
        check(
            Binding::StageFinal { stage: 0 },
            decl(DType::I8, &[]),
            &job,
            &rows,
            OutputDecl::Final { node: 0 },
            Tensor::scalar(DType::I8, 2).unwrap(),
        );
        check(
            Binding::JobImage { index: 0 },
            decl(DType::I16, &[1, 1, 3]),
            &job,
            &[],
            empty,
            Tensor::new(DType::I16, vec![1, 1, 3], vec![1, 2, 3]).unwrap(),
        );
    }
    #[test]
    fn random_blocks_match_native_R_and_refuse_unfunded_hash_work_before_output() {
        let random = crate::pipeline_public::PipelineRandomV1 { seed: [3; 32], item: 9 };
        let input = InputDecl {
            name: "normal".into(),
            dtype: DType::I32,
            shape: vec![35],
            source: InputSource::Random { domain: 2, dist: RandomDist::Normal, per_step: true },
        };
        let expected = random.random(2, RandomDist::Normal, 3, &[35]).unwrap();
        let memory = CircuitMemoryBudgetV5::new(25_000_000);
        let emit = |b: &mut CircuitBuilderV5| -> Result<(), String> {
            let (actual, cost) = emit_random_input_v5(b, &input, random, 3, limits(), &memory)?;
            if cost.public_random_hashes != 2 {
                return Err("random digest block bound mismatch".into());
            }
            assert_tensor(b, &actual, &expected)
        };
        let mut count = CircuitBuilderV5::preflight(graph_limits()).unwrap();
        emit(&mut count).unwrap();
        let mut b = CircuitBuilderV5::new_planned(graph_limits(), count.preflight_cost().unwrap(), memory.clone()).unwrap();
        emit(&mut b).unwrap();
        let graph = b.finish_reserved().unwrap();
        graph.evaluate_bytes(&[], graph_limits()).unwrap();
        drop(graph);
        assert_eq!(memory.held(), 0);
        let mut no_hash = limits();
        no_hash.maximum_random_hashes = 0;
        let mut count = CircuitBuilderV5::preflight(graph_limits()).unwrap();
        assert!(emit_random_input_v5(&mut count, &input, random, 3, no_hash, &memory).is_err());
        assert_eq!(memory.held(), 0);
    }
}
