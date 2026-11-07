//! Actual original Output and supported native regular-stop families; not full-original/paid capacity.
//! Allocation depends on ALL potential FullIds suppliers; runtime consumes ANY
//! complete source-authenticated FullIds, never a scheduled-finish boolean.
use super::SharedTirBuilderStoreV1 as Store;
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_court_source_material_graph_v3::{
    NativeSourceMaterialGraphV3 as Graph, SourceMaterialNodeKindV3 as Node,
    SourceMaterialTargetV3 as Target,
};
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1 as E, NativeHeapV1, NativeRetainedV1, TirNativeRequestsV1,
};
use crate::palw_tir_objective_v1::{
    PalwTirOutputFaultV1 as Fault, PalwTirOutputOutcomeV1 as Outcome,
};
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use crate::palw_tir_public_source_v1::{
    PalwTirPublicAuthenticatedIdsV1 as Ids, PalwTirPublicSourceV1 as Source,
};
use crate::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2},
};
pub(crate) const OUTPUT_INTENT_CONTEXT_V1: &[u8] = b"misaka-palw/shared-output-terminal/v1/mldsa87";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputDutyScopeV1 {
    OriginalOutputIdentityOnly,
    OriginalOutputAndNativeRegularStopOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputRuntimeGuardV1 {
    AnyActualAuthenticatedFullIdsForCurrentSource,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FullIdsDependencyV1 {
    pub role: u64,
    pub logical_finish: Hash64,
    pub absolute_finish: [u8; 64],
}
/// No decoder/public constructor/Clone. Owns the exact source-graph dependency
/// set, but has NO ServicePacket/price/default/escrow conversion capability.
pub(crate) struct NativeOutputDutyV1 {
    claim: Hash64,
    source: Hash64,
    profile: Hash64,
    allocation: Hash64,
    resource: Hash64,
    relative_graph: Hash64,
    absolute_graph: Hash64,
    accepted: u64,
    output: Hash64,
    regular_stop: RegularStopNodeV1,
    logical: Hash64,
    absolute: Hash64,
    dependencies: Vec<FullIdsDependencyV1>,
}
fn bad(s: &str) -> E<String> {
    E::Decode(s.into())
}
fn resource(s: &str) -> E<String> {
    E::Resource(NativeCoverageV1::unsupported::<NativeOutputDutyV1>(
        "original output duty",
        s,
    ))
}
fn hash<T: borsh::BorshSerialize>(domain: &[u8], v: &T) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(domain, v)
}
impl NativeOutputDutyV1 {
    pub(crate) fn scope_v1(&self) -> OutputDutyScopeV1 {
        match self.regular_stop {
            RegularStopNodeV1::Native { .. } => {
                OutputDutyScopeV1::OriginalOutputAndNativeRegularStopOnly
            }
            _ => OutputDutyScopeV1::OriginalOutputIdentityOnly,
        }
    }
    pub(crate) fn runtime_guard_v1(&self) -> OutputRuntimeGuardV1 {
        OutputRuntimeGuardV1::AnyActualAuthenticatedFullIdsForCurrentSource
    }
    pub(crate) fn logical_id_v1(&self) -> Hash64 {
        self.logical
    }
    pub(crate) fn absolute_id_v1(&self) -> Hash64 {
        self.absolute
    }
    pub(crate) fn allocation_dependencies_v1(&self) -> &[FullIdsDependencyV1] {
        &self.dependencies
    }
    /// Actual frame/source/opaque graph only. `profile` binds the graph's same
    /// namespace; it does not turn that namespace into an adopted policy body.
    pub(crate) fn derive_native_v1(
        frame: &Frame,
        source: &Source,
        graph: &NativeRetainedV1<Graph>,
        profile: Hash64,
    ) -> Result<NativeRetainedV1<Self>, E<String>> {
        let mut q = TirNativeRequestsV1::new();
        let clone = frame.native_heap_v1("output duty frame identity scratch");
        q.add_v1(clone.bytes().ok_or(E::Resource(clone))?)
            .map_err(E::Resource)?;
        let bytes = borsh::object_length(frame).map_err(|e| E::Decode(e.to_string()))? as u64;
        q.exact_v1::<u8>(
            bytes
                .max(1024)
                .checked_mul(2)
                .ok_or_else(|| resource("frame writer overflow"))?,
        )
        .map_err(E::Resource)?;
        q.exact_v1::<Self>(1).map_err(E::Resource)?;
        q.exact_v1::<FullIdsDependencyV1>(graph.role_slots_v3())
            .map_err(E::Resource)?;
        let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1())
            .map_err(E::Resource)?;
        graph
            .check_frame_binding_v3(frame, source, profile)
            .map_err(E::Decode)?;
        if !source.supports_tiled_trace_v1() {
            return Err(resource(
                "Flat FullIds output dependency grammar not covered",
            ));
        }
        let count = usize::try_from(graph.role_slots_v3())
            .map_err(|_| resource("role count platform overflow"))?;
        if count == 0 {
            return Err(bad("native output duty has no source supplier roles"));
        }
        let mut dependencies = Vec::new();
        dependencies
            .try_reserve_exact(count)
            .map_err(|_| resource("dependency allocation"))?;
        for n in graph.nodes_v3() {
            if let Node::Finish {
                target: Target::FullIds,
                role,
            } = n.kind_v3()
            {
                if dependencies.len() == count {
                    return Err(bad("native graph duplicates FullIds finish roles"));
                }
                dependencies.push(FullIdsDependencyV1 {
                    role: *role,
                    logical_finish: n.id_v3(),
                    absolute_finish: graph.absolute_packet_id_v3(n.id_v3()),
                });
            }
        }
        dependencies.sort_unstable_by_key(|p| p.role);
        if dependencies.len() != count
            || dependencies
                .iter()
                .enumerate()
                .any(|(i, p)| p.role != i as u64)
        {
            return Err(bad("native graph lacks exact FullIds supplier finish set"));
        }
        let p = frame.plan_v1();
        let output = frame.original_v1().output_root;
        if output == Hash64::default() {
            return Err(bad("original output commitment is unknown"));
        }
        let logical = hash(
            b"misaka-palw/output-duty-logical/v1",
            &(
                1u16,
                graph.scope_v3(),
                p.original_claim,
                source.source_id_v1(),
                output,
            ),
        )
        .map_err(E::Decode)?;
        let absolute = hash(
            b"misaka-palw/output-duty-absolute/v1",
            &(graph.absolute_scope_v3(), logical),
        )
        .map_err(E::Decode)?;
        let original = frame.original_v1();
        let regular_stop = match regular_stop_rule_v1(&original.job) {
            Err(why) => RegularStopNodeV1::Unsupported(why),
            Ok(kind) => {
                let job = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original);
                let count = original.decode_tokens_executed;
                let reason = original.stop_reason;
                let stop_logical = hash(
                    b"misaka-palw/regular-stop-duty-logical/v1",
                    &(1u16, graph.scope_v3(), logical, job, kind, count, reason),
                )
                .map_err(E::Decode)?;
                let stop_absolute = hash(
                    b"misaka-palw/regular-stop-duty-absolute/v1",
                    &(graph.absolute_scope_v3(), stop_logical),
                )
                .map_err(E::Decode)?;
                RegularStopNodeV1::Native {
                    logical: stop_logical,
                    absolute: stop_absolute,
                    kind,
                    job,
                    count,
                    reason,
                }
            }
        };
        Ok(NativeRetainedV1::from_owner_reserved_v1(
            Self {
                claim: p.original_claim,
                source: source.source_id_v1(),
                profile,
                allocation: p.allocation_id,
                resource: p.resource_plan_id,
                relative_graph: graph.scope_v3(),
                absolute_graph: graph.absolute_scope_v3(),
                accepted: p.terms.accepted_daa,
                output,
                regular_stop,
                logical,
                absolute,
                dependencies,
            },
            lease,
        ))
    }
    fn check_current(&self, frame: &Frame, source: &Source, profile: Hash64) -> Result<(), String> {
        let p = frame.plan_v1();
        if (
            self.claim,
            self.source,
            self.profile,
            self.allocation,
            self.resource,
            self.accepted,
            self.output,
        ) != (
            p.original_claim,
            source.source_id_v1(),
            profile,
            p.allocation_id,
            p.resource_plan_id,
            p.terms.accepted_daa,
            frame.original_v1().output_root,
        ) {
            return Err(
                "output duty differs from current original/source/profile/allocation".into(),
            );
        }
        Ok(())
    }
}
#[derive(Clone, borsh::BorshSerialize)]
pub(crate) struct SignedOutputTerminalIntentV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub policy_id: Hash64,
    pub duty_id: Hash64,
    pub material_id: Hash64,
    pub filer: PalwBondKeyV2,
    pub signature: Vec<u8>,
}
impl SignedOutputTerminalIntentV1 {
    pub(crate) fn signing_message(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 {
            return Err("output terminal intent version".into());
        }
        hash(
            b"misaka-palw/output-terminal-intent/v1",
            &(
                network,
                self.version,
                self.claim,
                self.scope_id,
                self.policy_id,
                self.duty_id,
                self.material_id,
                self.filer,
            ),
        )
    }
}
pub(crate) fn ids_material_id_v1(ids: &Ids) -> Result<Hash64, String> {
    hash(
        b"misaka-palw/output-terminal-ids/v1",
        &(ids.source_id_v1(), ids.rows_root_v1(), ids.ids_v1()),
    )
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputDecisionV1 {
    OutputHolds,
    Fault {
        kind: Fault,
        claim: Hash64,
        execution: Hash64,
        computed: Hash64,
    },
}
pub(crate) struct OutputTerminalEvidenceV1<'a> {
    intent: SignedOutputTerminalIntentV1,
    duty: &'a NativeRetainedV1<NativeOutputDutyV1>,
    ids: &'a Ids,
    decision: OutputDecisionV1,
}
impl OutputTerminalEvidenceV1<'_> {
    pub(crate) fn recheck(
        &self,
        store: &Store<'_>,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
    ) -> Result<OutputDecisionV1, String> {
        let actual = store.replay_output_terminal_v1(
            state,
            params,
            rules,
            &self.intent,
            self.duty,
            self.ids,
        )?;
        if actual != self.decision {
            return Err("output terminal evidence reconstruction mismatch".into());
        }
        Ok(actual)
    }
}
impl Store<'_> {
    /// Existing native primitive and independently reconstructed current state.
    /// Caller separately admits Source/signature/intent-clone/evidence scratch.
    /// Duty ticket is not a paid ingress or admission of those separate owners.
    pub(crate) fn build_output_terminal_v1<'a>(
        &'a self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedOutputTerminalIntentV1,
        duty: &'a NativeRetainedV1<NativeOutputDutyV1>,
    ) -> Result<OutputTerminalEvidenceV1<'a>, String> {
        let ids = self
            .ids
            .ok_or("output terminal lacks source-authenticated FullIds")?;
        let decision = self.replay_output_terminal_v1(state, params, rules, intent, duty, ids)?;
        let evidence = OutputTerminalEvidenceV1 {
            intent: intent.clone(),
            duty,
            ids,
            decision,
        };
        evidence.recheck(self, state, params, rules)?;
        Ok(evidence)
    }
    fn replay_output_terminal_v1(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedOutputTerminalIntentV1,
        duty: &NativeRetainedV1<NativeOutputDutyV1>,
        ids: &Ids,
    ) -> Result<OutputDecisionV1, String> {
        let c = crate::palw_court_shared_tir_state_v1::context(state, intent.claim)?;
        let source = c.source(state)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, intent.claim)
            .ok_or("output terminal original absent")?;
        duty.check_current(frame, &source, c.profile_id)?;
        if intent.scope_id != c.scope_id
            || self.source.source_id_v1() != source.source_id_v1()
            || !rules.tir_active
            || rules.at.daa_score < c.original_accepted_daa
            || rules.at.daa_score >= c.hard_daa
            || rules.step_ladder != frame.plan_v1().owner_work.rules.max_step_leaf_count
            || intent.policy_id
                != crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
            || intent.duty_id != duty.absolute_id_v1()
            || intent.material_id != ids_material_id_v1(ids)?
        {
            return Err(
                "output terminal current source/policy/clock/duty/material mismatch".into(),
            );
        }
        let bond = state
            .bond(&intent.filer)
            .ok_or("output terminal current filer absent")?;
        if intent.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || !matches!(bond.status, PalwBondStatusV2::Active)
            || bond.collateral < params.min_collateral_sompi()
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                &bond.pubkey,
                intent
                    .signing_message(rules.network_domain)?
                    .as_byte_slice(),
                &intent.signature,
                OUTPUT_INTENT_CONTEXT_V1,
            )
        {
            return Err("output terminal current filer authority/signature invalid".into());
        }
        // A scheduled Finish is NEVER substituted for actual authenticated data.
        match crate::palw_tir_objective_v1::adjudicate_original_output_v1(
            state,
            intent.claim,
            frame.original_v1().output_root,
            &source,
            ids,
        )? {
            Outcome::OutputHolds { .. } => Ok(OutputDecisionV1::OutputHolds),
            Outcome::Fault(f) => Ok(OutputDecisionV1::Fault {
                kind: f.fault_v1(),
                claim: f.target_v1().claim_id,
                execution: f.target_v1().execution_root,
                computed: f.computed_output_v1(),
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeRegularStopUnsupportedV1 {
    Constraint,
    CarriedTail,
    NoNativeRule,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub(crate) enum NativeRegularStopRuleV1 {
    KeyedV3,
    PipelineV4,
}
fn regular_stop_rule_v1(
    job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
) -> Result<NativeRegularStopRuleV1, NativeRegularStopUnsupportedV1> {
    if job.is_constraint() {
        return Err(NativeRegularStopUnsupportedV1::Constraint);
    }
    if job.tail.is_some() {
        return Err(NativeRegularStopUnsupportedV1::CarriedTail);
    }
    match crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(job) {
        Some(crate::palw_tir_court_v1::PalwTirSelectionRuleV1::Keyed { .. }) => {
            Ok(NativeRegularStopRuleV1::KeyedV3)
        }
        Some(crate::palw_tir_court_v1::PalwTirSelectionRuleV1::V4 { .. }) => {
            Ok(NativeRegularStopRuleV1::PipelineV4)
        }
        None => Err(NativeRegularStopUnsupportedV1::NoNativeRule),
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegularStopNodeV1 {
    Native {
        logical: Hash64,
        absolute: Hash64,
        kind: NativeRegularStopRuleV1,
        job: Hash64,
        count: u32,
        reason: crate::palw_freeprompt_v3::PalwFpStopReasonV3,
    },
    Unsupported(NativeRegularStopUnsupportedV1),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OriginalTerminalFamilyV1 {
    OutputIdentity,
    NativeRegularStop,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalAllocationParentsV1<'a> {
    /// Potential provider slots, not proof that any provider supplied data.
    AllFullIdsFinishes(&'a [FullIdsDependencyV1]),
    /// Stop is reached only after output identity/domain holds. Its FullIds
    /// suppliers are shared transitively; no duplicated supply vector.
    OutputIdentity { logical: Hash64, absolute: Hash64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OriginalTerminalFamilyNodeV1<'a> {
    pub family: OriginalTerminalFamilyV1,
    pub logical: Hash64,
    pub absolute: Hash64,
    pub parents: TerminalAllocationParentsV1<'a>,
}

impl NativeOutputDutyV1 {
    pub(crate) fn family_nodes_v1(&self) -> impl Iterator<Item = OriginalTerminalFamilyNodeV1<'_>> {
        let output = OriginalTerminalFamilyNodeV1 {
            family: OriginalTerminalFamilyV1::OutputIdentity,
            logical: self.logical,
            absolute: self.absolute,
            parents: TerminalAllocationParentsV1::AllFullIdsFinishes(&self.dependencies),
        };
        let stop = match self.regular_stop {
            RegularStopNodeV1::Native {
                logical, absolute, ..
            } => Some(OriginalTerminalFamilyNodeV1 {
                family: OriginalTerminalFamilyV1::NativeRegularStop,
                logical,
                absolute,
                parents: TerminalAllocationParentsV1::OutputIdentity {
                    logical: self.logical,
                    absolute: self.absolute,
                },
            }),
            _ => None,
        };
        std::iter::once(output).chain(stop)
    }
    pub(crate) fn native_regular_stop_id_v1(
        &self,
    ) -> Result<Hash64, NativeRegularStopUnsupportedV1> {
        match self.regular_stop {
            RegularStopNodeV1::Native { absolute, .. } => Ok(absolute),
            RegularStopNodeV1::Unsupported(why) => Err(why),
        }
    }
}
pub(crate) const REGULAR_STOP_CONTEXT_V1: &[u8] =
    b"misaka-palw/shared-native-regular-stop/v1/mldsa87";
#[derive(Clone, borsh::BorshSerialize)]
pub(crate) struct SignedRegularStopIntentV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub policy_id: Hash64,
    pub duty_id: Hash64,
    pub material_id: Hash64,
    pub decode_count: u32,
    pub stop_reason: crate::palw_freeprompt_v3::PalwFpStopReasonV3,
    pub filer: PalwBondKeyV2,
    pub signature: Vec<u8>,
}
impl SignedRegularStopIntentV1 {
    pub(crate) fn signing_message(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 {
            return Err("native regular stop intent version".into());
        }
        hash(
            b"misaka-palw/native-regular-stop-intent/v1",
            &(
                network,
                self.version,
                self.claim,
                self.scope_id,
                self.policy_id,
                self.duty_id,
                self.material_id,
                self.decode_count,
                self.stop_reason,
                self.filer,
            ),
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegularStopDecisionV1 {
    /// Only this job's native regular stop rule; never selection/claim honesty.
    StopRuleHolds,
    Fault(&'static str),
}
#[derive(Debug)]
pub(crate) enum RegularStopRefusalV1 {
    Unsupported(NativeRegularStopUnsupportedV1),
    OutputPrerequisite(Fault),
    Binding(String),
}
impl From<String> for RegularStopRefusalV1 {
    fn from(s: String) -> Self {
        Self::Binding(s)
    }
}
impl From<&str> for RegularStopRefusalV1 {
    fn from(s: &str) -> Self {
        Self::Binding(s.into())
    }
}
pub(crate) struct RegularStopEvidenceV1<'a> {
    intent: SignedRegularStopIntentV1,
    duty: &'a NativeRetainedV1<NativeOutputDutyV1>,
    ids: &'a Ids,
    decision: RegularStopDecisionV1,
}
impl RegularStopEvidenceV1<'_> {
    pub(crate) fn recheck(
        &self,
        store: &Store<'_>,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
    ) -> Result<RegularStopDecisionV1, RegularStopRefusalV1> {
        let actual = store.replay_regular_stop_v1(
            state,
            params,
            rules,
            &self.intent,
            self.duty,
            self.ids,
        )?;
        if actual != self.decision {
            return Err("native regular stop evidence reconstruction mismatch".into());
        }
        Ok(actual)
    }
}
impl Store<'_> {
    pub(crate) fn build_regular_stop_terminal_v1<'a>(
        &'a self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedRegularStopIntentV1,
        duty: &'a NativeRetainedV1<NativeOutputDutyV1>,
    ) -> Result<RegularStopEvidenceV1<'a>, RegularStopRefusalV1> {
        let ids = self
            .ids
            .ok_or("native regular stop lacks source-authenticated FullIds")?;
        let decision = self.replay_regular_stop_v1(state, params, rules, intent, duty, ids)?;
        let evidence = RegularStopEvidenceV1 {
            intent: intent.clone(),
            duty,
            ids,
            decision,
        };
        evidence.recheck(self, state, params, rules)?;
        Ok(evidence)
    }
    fn replay_regular_stop_v1(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &SignedRegularStopIntentV1,
        duty: &NativeRetainedV1<NativeOutputDutyV1>,
        ids: &Ids,
    ) -> Result<RegularStopDecisionV1, RegularStopRefusalV1> {
        let c = crate::palw_court_shared_tir_state_v1::context(state, intent.claim)?;
        let source = c.source(state)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, intent.claim)
            .ok_or("native regular stop original absent")?;
        let original = frame.original_v1();
        duty.check_current(frame, &source, c.profile_id)?;
        let actual_kind =
            regular_stop_rule_v1(&original.job).map_err(RegularStopRefusalV1::Unsupported)?;
        let (absolute, kind, job, count, reason) = match duty.regular_stop {
            RegularStopNodeV1::Native {
                absolute,
                kind,
                job,
                count,
                reason,
                ..
            } => (absolute, kind, job, count, reason),
            RegularStopNodeV1::Unsupported(why) => {
                return Err(RegularStopRefusalV1::Unsupported(why));
            }
        };
        if kind != actual_kind
            || job != crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original)
            || count != original.decode_tokens_executed
            || reason != original.stop_reason
            || intent.decode_count != count
            || intent.stop_reason != reason
            || intent.scope_id != c.scope_id
            || self.source.source_id_v1() != source.source_id_v1()
            || !rules.tir_active
            || rules.at.daa_score < c.original_accepted_daa
            || rules.at.daa_score >= c.hard_daa
            || rules.step_ladder != frame.plan_v1().owner_work.rules.max_step_leaf_count
            || intent.policy_id
                != crate::palw_court_shared_tir_root_v1::shared_tir_root_policy_id_v1(rules)?
            || intent.duty_id != absolute
            || intent.material_id != ids_material_id_v1(ids)?
        {
            return Err("native regular stop actual source/policy/clock/job/count/reason/duty/material mismatch".into());
        }
        let bond = state
            .bond(&intent.filer)
            .ok_or("native regular stop current filer absent")?;
        if intent.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || !matches!(bond.status, PalwBondStatusV2::Active)
            || bond.collateral < params.min_collateral_sompi()
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                &bond.pubkey,
                intent
                    .signing_message(rules.network_domain)?
                    .as_byte_slice(),
                &intent.signature,
                REGULAR_STOP_CONTEXT_V1,
            )
        {
            return Err("native regular stop current filer authority/signature invalid".into());
        }
        // Same priority as native original Output arm: only a source-affine
        // output-consistent answer reaches the regular stop predicate. No
        // caller-provided success bit can discharge this dependency.
        match crate::palw_tir_objective_v1::adjudicate_original_output_v1(
            state,
            intent.claim,
            original.output_root,
            &source,
            ids,
        )? {
            Outcome::Fault(f) => {
                return Err(RegularStopRefusalV1::OutputPrerequisite(f.fault_v1()));
            }
            Outcome::OutputHolds { .. } => {}
        }
        Ok(
            match crate::palw_tir_objective_v1::original_stop_rule_fault_v1(
                &original.job,
                source.vocab_v1(),
                ids.ids_v1(),
            ) {
                None => RegularStopDecisionV1::StopRuleHolds,
                Some(why) => RegularStopDecisionV1::Fault(why),
            },
        )
    }
}

impl NativeOutputDutyV1 {
    pub(super) fn aggregate_binding_v1(&self)->super::original_aggregate::FamilyBindingV1{(self.claim,self.source,self.relative_graph,self.absolute_graph,self.profile)}
}
