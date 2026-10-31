//! Resumable public-discovery adapter for original Master32 invocations. The
//! complete independently authenticated reference is caller-owned; the root
//! locator is logarithmic in its directory. Only the selected relation is
//! replayed again, under a separate prepaid original-maximum attempt.
use crate::flat_root_locator_v9::{FlatRootLocatorV9, RootFindingV9};
use misaka_palw_kernel::{
    element::SegClaimContextV1,
    flat_court5::FlatSegContextV5,
    flat_detect5::{FlatFindingV5, FlatPublicReadsV5, FlatReadV5},
    flat_detect25::{FlatHeaderReadV25, FlatPublicReadsV25, FlatValueReadV25},
    flat_inputs12::{composition32::*, court32::*, replay32::*},
    flat_quota11::stage14::{StageTraceV14, bounded28::*},
    flat_topk7::TopKScanBudgetV7,
    job::DecodeRuleV1,
    merkle5::FlatLeafHashV5,
    seg::{NodeOpeningV1, prompt_root_of_ids_v1},
};
use misaka_palw_tir::ParamSource;
use std::cell::Cell;

pub struct NativeInvocationLocatorV32<'a> {
    maximum: &'a CheckedMaximumInvocationV32,
    invocation: &'a CheckedStageInvocationV32<'a>,
    quota: &'a CheckedStageQuotaV28,
    reference: &'a IndependentInvocationReplayV32,
    post: &'a StageTraceV14,
    locator: FlatRootLocatorV9<'a>,
    selected: Option<IndependentRelationV32>,
    calls: Cell<u32>,
    scan: TopKScanBudgetV7,
    roots_match: bool,
}
struct Reads<'a> {
    inner: &'a dyn FlatPublicReadsV25,
    calls: &'a Cell<u32>,
}
impl Reads<'_> {
    fn spend(&self) -> bool {
        let n = self.calls.get();
        if n == 0 {
            return false;
        }
        self.calls.set(n - 1);
        true
    }
}
impl FlatPublicReadsV5 for Reads<'_> {
    fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
        if self.spend() { self.inner.node(u) } else { None }
    }
    fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
        if self.spend() { self.inner.hash(u) } else { None }
    }
}
impl FlatPublicReadsV25 for Reads<'_> {
    fn value(&self, u: FlatValueReadV25) -> Option<Vec<u8>> {
        if self.spend() {
            self.inner.value(u).filter(|v| v.len() as u64 <= misaka_palw_kernel::flat_detect25::TYPED_VALUE_MAX_BYTES_V25)
        } else {
            None
        }
    }
    fn header(&self, u: FlatHeaderReadV25) -> Option<Vec<u8>> {
        if self.spend() {
            self.inner.header(u).filter(|v| v.len() as u64 <= misaka_palw_kernel::flat_detect25::TYPED_HEADER_MAX_BYTES_V25)
        } else {
            None
        }
    }
}
impl<'a> NativeInvocationLocatorV32<'a> {
    pub fn new(
        maximum: &'a CheckedMaximumInvocationV32,
        invocation: &'a CheckedStageInvocationV32<'a>,
        quota: &'a CheckedStageQuotaV28,
        reference: &'a IndependentInvocationReplayV32,
        post: &'a StageTraceV14,
        scan: TopKScanBudgetV7,
    ) -> Result<Self, String> {
        if reference.invocation_root() != invocation.invocation_root()
            || reference.input_binding_root() != invocation.input_binding_root()
            || post.job_input != invocation.trace_input_root()
            || post.admission != invocation.job().admission().root()
            || post.quota != quota.root()
            || reference.commitments().positions() != invocation.positions()
            || !quota.is_invocation_v32()
        {
            return Err("native32 locator changes original actual source/trace/reference/quota".into());
        }
        let calls = quota.requests_per_owner().checked_mul(2).ok_or("native32 original acquisition attempt overflow")?;
        Ok(Self {
            maximum,
            invocation,
            quota,
            reference,
            post,
            locator: FlatRootLocatorV9::new(reference.commitments(), &post.segment_roots)?,
            selected: None,
            calls: Cell::new(calls),
            scan,
            roots_match: reference.commitments().segment_roots() == post.segment_roots,
        })
    }
    pub fn remaining_calls(&self) -> u32 {
        self.calls.get()
    }
    pub fn advance(
        &mut self,
        public: &dyn FlatPublicReadsV25,
        reference_session: &mut InvocationReplaySessionV32,
        material: &dyn ParamSource,
    ) -> StageFindingV32 {
        if self.calls.get() == 0 {
            return StageFindingV32::Inconsistent("native32 original public acquisition attempts spent".into());
        }
        let reads = Reads { inner: public, calls: &self.calls };
        let finding = self.locator.advance(&reads);
        let unit = match finding {
            RootFindingV9::Demand(r) => return StageFindingV32::Demand(r),
            RootFindingV9::Inconsistent(e) => return StageFindingV32::Inconsistent(e),
            RootFindingV9::Divergent(o) => FlatReadV5 { position: o.position, occurrence: o.occurrence, node: o.node, leaf: None },
            RootFindingV9::Clean => {
                let Some(index) = self.reference.decode_mismatch() else { return StageFindingV32::Clean };
                let program = self.invocation.actual_program();
                let occurrence = program.occurrences().len() as u16 - 1;
                FlatReadV5 {
                    position: self.invocation.prompt().len() as u32 - 1 + index,
                    occurrence,
                    node: program.output.node(),
                    leaf: None,
                }
            }
        };
        if self.selected.as_ref().is_none_or(|s| s.unit != unit) {
            let selected = match reference_session.reexecute_selected(
                self.maximum,
                self.invocation,
                self.quota,
                material,
                self.invocation.generated(),
                unit,
            ) {
                Ok(s) => s,
                Err(e) => return StageFindingV32::Inconsistent(e),
            };
            if selected.commitments().segment_roots() != self.reference.commitments().segment_roots() {
                return StageFindingV32::Inconsistent("native32 independent reference changed between paid attempts".into());
            }
            self.selected = selected.into_selected_relation();
        }
        let Some(selected) = &self.selected else {
            return StageFindingV32::Inconsistent("native32 selected independent relation absent".into());
        };
        let view = self.invocation.actual_program().v1_view();
        let pc = match self.invocation.combined_parameters_at(unit.position) {
            Ok(p) => p,
            Err(e) => return StageFindingV32::Inconsistent(e),
        };
        let logits = matches!(self.invocation.actual_program().output, misaka_palw_tir::program_v2::OutputDecl::Logits { .. });
        let prompt = self.invocation.prompt();
        let generated = if logits { self.invocation.generated() } else { &[0][..] };
        let base = FlatSegContextV5 {
            profile: misaka_palw_kernel::flat_court5::flat_trace_profile_id_v5(),
            claim: SegClaimContextV1 {
                program: &view,
                params: &pc,
                segment_roots: &self.post.segment_roots,
                positions: self.invocation.positions(),
                prompt_len: prompt.len() as u32,
                prompt_root: prompt_root_of_ids_v1(prompt),
                inline_prompt: Some(prompt),
                generated,
                decode: DecodeRuleV1::Greedy,
                encoder: None,
            },
        };
        if self.locator_is_clean() {
            return match misaka_palw_kernel::flat_detect5::locate_selected_decode_v5(&base, &reads, unit.position, &selected.expected)
            {
                FlatFindingV5::Clean => StageFindingV32::Clean,
                FlatFindingV5::Demand(r) => StageFindingV32::Demand(r),
                FlatFindingV5::Inconsistent(e) => StageFindingV32::Inconsistent(e),
                FlatFindingV5::Fault(f) => match f.to_bytes() {
                    Ok(raw) => StageFindingV32::Fault(raw),
                    Err(e) => StageFindingV32::Inconsistent(e.to_string()),
                },
            };
        }
        locate_stage_relation_v32(
            self.invocation,
            self.maximum.scope(),
            base,
            &reads,
            unit,
            &selected.expected,
            &|| Ok(selected.operands.clone()),
            &|| None,
            &mut self.scan,
        )
    }
    fn locator_is_clean(&self) -> bool {
        self.roots_match
    }
}
