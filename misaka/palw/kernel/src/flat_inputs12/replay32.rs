//! Independently acquired full reference for native invocation32. The original
//! maximum is charged BEFORE any source callback or execution; actual EOS does
//! not restore allowances. This local reference is neither a native Final nor
//! a promise of small ordinary-check work. Public localization/court consumes
//! its selected original relation separately under the same invocation root.
use super::{bounded28::*, composition32::*, court32::*};
use crate::{
    flat_quota11::{QuotaQuantitiesV11, stage14::bounded28::*},
    flat_resources10::ResourceAxisV10 as A,
    hash::Digest,
    seg::SegmentedCommitmentsV1,
};
use misaka_palw_tir::{ParamSource, Tensor};

pub struct InvocationReplaySessionV32 {
    remaining: QuotaQuantitiesV11,
    attempts: u32,
}
pub struct IndependentInvocationReplayV32 {
    invocation: Digest,
    inputs: Digest,
    commitments: SegmentedCommitmentsV1,
    decode_mismatch: Option<u32>,
    selected: Option<IndependentRelationV32>,
}
pub struct IndependentRelationV32 {
    pub unit: crate::flat_detect5::FlatReadV5,
    pub expected: Tensor,
    pub operands: Vec<Tensor>,
}
impl IndependentInvocationReplayV32 {
    pub fn selected_relation(&self) -> Option<&IndependentRelationV32> {
        self.selected.as_ref()
    }
    pub fn into_selected_relation(self) -> Option<IndependentRelationV32> {
        self.selected
    }
    pub fn invocation_root(&self) -> Digest {
        self.invocation
    }
    pub fn input_binding_root(&self) -> Digest {
        self.inputs
    }
    pub fn commitments(&self) -> &SegmentedCommitmentsV1 {
        &self.commitments
    }
    pub fn decode_mismatch(&self) -> Option<u32> {
        self.decode_mismatch
    }
    pub fn matches_public_roots(&self, roots: &[Digest]) -> bool {
        self.decode_mismatch.is_none() && self.commitments.segment_roots() == roots
    }
}
impl InvocationReplaySessionV32 {
    pub fn new(allowance: QuotaQuantitiesV11, attempts: u32) -> Result<Self, String> {
        if attempts == 0 || attempts > 2 {
            return Err("invocation32 reference attempts must be bounded before execution".into());
        }
        Ok(Self { remaining: allowance, attempts })
    }
    pub fn remaining(&self) -> &QuotaQuantitiesV11 {
        &self.remaining
    }
    pub fn reexecute(
        &mut self,
        maximum: &CheckedMaximumInvocationV32,
        invocation: &CheckedStageInvocationV32<'_>,
        original_quota: &CheckedStageQuotaV28,
        material: &dyn ParamSource,
        generated: &[u32],
    ) -> Result<IndependentInvocationReplayV32, String> {
        self.execute(maximum, invocation, original_quota, material, generated, None)
    }
    pub fn reexecute_selected(
        &mut self,
        maximum: &CheckedMaximumInvocationV32,
        invocation: &CheckedStageInvocationV32<'_>,
        original_quota: &CheckedStageQuotaV28,
        material: &dyn ParamSource,
        generated: &[u32],
        unit: crate::flat_detect5::FlatReadV5,
    ) -> Result<IndependentInvocationReplayV32, String> {
        self.execute(maximum, invocation, original_quota, material, generated, Some(unit))
    }
    fn execute(
        &mut self,
        maximum: &CheckedMaximumInvocationV32,
        invocation: &CheckedStageInvocationV32<'_>,
        original_quota: &CheckedStageQuotaV28,
        material: &dyn ParamSource,
        generated: &[u32],
        selected: Option<crate::flat_detect5::FlatReadV5>,
    ) -> Result<IndependentInvocationReplayV32, String> {
        let class = invocation.class();
        let maximum_job = class.check_job(maximum.job())?;
        let quote = derive_stage_quota_v32(class, &maximum_job)?;
        if generated != invocation.generated()
            || quote.root() != original_quota.root()
            || !original_quota.is_invocation_v32()
            || invocation.positions() > maximum.job().positions
            || maximum.scope().root() != derive_stage_court_scope_v32(class)?.root()
        {
            return Err("invocation32 replay changes original maximum quota/source/court".into());
        }
        let mut cost = quote.plan().record().baseline;
        // Two authenticated source-tree passes (registered PC + independent R
        // acquisition) are distinct from primitive evaluation and trace hashes.
        cost.set(A::ParameterHashBytes, cost.get(A::ParameterHashBytes).checked_mul(2).ok_or("invocation32 source hash overflow")?);
        cost.within(&self.remaining)?;
        if self.attempts == 0 {
            return Err("invocation32 independent retry allowance spent".into());
        }
        self.remaining = self.remaining.checked_sub(&cost)?;
        self.attempts -= 1;
        // Every failure from this point retains the complete original debit.
        let source = class.source();
        let real = source.program();
        for ((index, layer), expected) in &source.parameters().by_instance {
            let decl = real.params.get(*index as usize).ok_or("invocation32 source namespace includes a virtual input")?;
            let t = material.param(*index, *layer).ok_or("invocation32 independently acquired original weights absent")?;
            if t.dtype != decl.dtype
                || t.shape != decl.shape.iter().map(|n| *n as usize).collect::<Vec<_>>()
                || crate::merkle3::tensor_commitment_v3(&t) != *expected
            {
                return Err("invocation32 acquired source does not authenticate original real PC".into());
            }
        }
        let logits = matches!(invocation.actual_program().output, misaka_palw_tir::program_v2::OutputDecl::Logits { .. });
        let tokens = if logits {
            if generated.is_empty()
                || generated.len() > invocation.maximum_new_tokens() as usize
                || generated.iter().any(|id| *id >= real.token_bound)
                || generated[..generated.len() - 1].contains(&invocation.eos_token())
                || generated.last() != Some(&invocation.eos_token()) && generated.len() != invocation.maximum_new_tokens() as usize
            {
                return Err("invocation32 independently checked stop grammar differs".into());
            }
            let mut t = invocation.prompt().to_vec();
            t.extend_from_slice(&generated[..generated.len() - 1]);
            t
        } else {
            invocation.prompt().to_vec()
        };
        if tokens.len() != invocation.positions() as usize {
            return Err("invocation32 actual reference tokens differ from original invocation extent".into());
        }
        let output = invocation.actual_program().output.node() as usize;
        let occurrence = invocation.actual_program().occurrences().len() - 1;
        let view = invocation.actual_program().v1_view();
        let prepared = crate::element::PreparedV1::without_stage_v32(&view, invocation.actual_program())?;
        let sources = match selected {
            Some(u) if u.position < invocation.positions() && u.leaf.is_none() && prepared.valid(u.occurrence, u.node) => {
                crate::element::operand_sources(&prepared, u.position, u.occurrence, u.node)?
            }
            Some(_) => return Err("invocation32 selected reference coordinate exceeds original actual graph".into()),
            None => vec![],
        };
        let mut previous = std::collections::BTreeMap::new();
        let mut selected_value = None;
        let mut roots = Vec::with_capacity(tokens.len());
        let mut mismatch = None;
        crate::trace::trace_streaming_stage_v32(
            invocation.actual_program(),
            material,
            &tokens,
            &|k, p| invocation.input(k, p).cloned(),
            &mut |position, values| {
                if logits && position as usize + 1 >= invocation.prompt().len() {
                    let i = position as usize + 1 - invocation.prompt().len();
                    if i < generated.len() {
                        let row = &values[occurrence][output];
                        let best = row
                            .data
                            .iter()
                            .enumerate()
                            .max_by(|(ai, a), (bi, b)| a.cmp(b).then_with(|| bi.cmp(ai)))
                            .map(|(i, _)| i as u32);
                        if best != Some(generated[i]) && mismatch.is_none() {
                            mismatch = Some(i as u32);
                        }
                    }
                }
                if let Some(unit) = selected {
                    use crate::{element::OperandSourceV1 as O, trace::SourceV1 as S};
                    for src in &sources {
                        if let O::Source(S::Node { position: q, occurrence, node }) = src {
                            if *q == position && *q < unit.position {
                                previous.insert((*q, *occurrence, *node), values[*occurrence as usize][*node as usize].clone());
                            }
                        }
                    }
                    if position == unit.position {
                        let mut operands = Vec::with_capacity(sources.len());
                        for src in &sources {
                            let value = match src {
                                O::Token => Tensor::scalar(misaka_palw_tir::DType::Idx, tokens[position as usize] as i128),
                                O::Position => Tensor::scalar(misaka_palw_tir::DType::Idx, position as i128),
                                O::Source(S::Node { position: q, occurrence, node }) => {
                                    if *q == position {
                                        Ok(values[*occurrence as usize][*node as usize].clone())
                                    } else {
                                        previous.get(&(*q, *occurrence, *node)).cloned().ok_or_else(|| {
                                            misaka_palw_tir::TirError::new(
                                                misaka_palw_tir::TirErrorKind::Missing,
                                                "stage32 selected predecessor absent",
                                            )
                                        })
                                    }
                                }
                                O::Source(S::Param { index, layer }) => {
                                    let t = if (*index as usize) < real.params.len() {
                                        material.param(*index, *layer)
                                    } else {
                                        invocation.input(*index - real.params.len() as u16, position).cloned()
                                    };
                                    t.ok_or_else(|| {
                                        misaka_palw_tir::TirError::new(
                                            misaka_palw_tir::TirErrorKind::Missing,
                                            "stage32 selected original parameter/input absent",
                                        )
                                    })
                                }
                                O::Source(S::Const(index)) => {
                                    let c = &view.consts[*index as usize];
                                    Tensor::from_le_bytes(c.dtype, &c.shape.iter().map(|n| *n as usize).collect::<Vec<_>>(), &c.data)
                                }
                                O::Source(S::Zeros { dtype, shape }) => Ok(Tensor::zeros(*dtype, shape)),
                                O::Source(S::Public(value)) => Tensor::scalar(misaka_palw_tir::DType::Idx, *value as i128),
                                O::Source(S::Input { k, position }) => invocation.input(*k, *position).cloned().ok_or_else(|| {
                                    misaka_palw_tir::TirError::new(
                                        misaka_palw_tir::TirErrorKind::Missing,
                                        "stage32 selected position input absent",
                                    )
                                }),
                            }?;
                            operands.push(value);
                        }
                        selected_value = Some(IndependentRelationV32 {
                            unit,
                            expected: values[unit.occurrence as usize][unit.node as usize].clone(),
                            operands,
                        });
                    }
                }
                let row = values
                    .iter()
                    .map(|o| o.iter().map(crate::flat_court5::flat_profile_commitment_v5).collect::<Result<Vec<_>, _>>())
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|s| misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Malformed, s.to_string()))?;
                roots.push(row);
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(IndependentInvocationReplayV32 {
            invocation: invocation.invocation_root(),
            inputs: invocation.input_binding_root(),
            commitments: SegmentedCommitmentsV1::new(roots),
            decode_mismatch: mismatch,
            selected: selected_value,
        })
    }
}
