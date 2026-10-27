//! Private potential Constraint terminal grammar. Source dependencies are exact;
//! semantic-table supply remains explicitly unresolved, never paid/complete.
use super::{
    SharedTirBuilderStoreV1 as Store,
    constraint_stop::{ConstraintStopEvidenceV1, SignedConstraintStopIntentV1 as StopIntent},
    constraint_terminal::{ConstraintTerminalEvidenceV1, SignedConstraintTerminalIntentV1 as TokenIntent},
    output_terminal::{FullIdsDependencyV1, NativeOutputDutyV1 as Output},
    semantic_material::SemanticTableMaterialV1,
};
use crate::palw_constraint_stream_court_v1::canonical_replay::WorkBudget;
use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
use crate::palw_court_source_material_graph_v3::{
    NativeSourceMaterialGraphV3 as Graph, SourceMaterialNodeKindV3 as Node, SourceMaterialTargetV3 as Material,
};
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1 as E, NativeHeapV1, NativeRetainedV1, TirNativeRequestsV1,
};
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Query, PalwTirPublicSourceV1 as Source};
use crate::{
    Hash64,
    palw_state_v2::{PalwChainStateV2, PalwStateParamsV2},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub(crate) enum ConstraintTargetV1 {
    Token { position: u32, beat: u32, witness: Option<u32> },
    Stop,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConstraintSemanticSupplyV1 {
    /// No SourceGraph semantic supplier/pledge exists. Root identity is NOT a
    /// provider receipt: runtime still authenticates actual EOG/rendering bytes.
    UnresolvedTableSupply {
        table: Hash64,
        vocabulary: u32,
    },
    UnresolvedEogOnlySupply {
        table: Hash64,
        vocabulary: u32,
    },
}
fn resource(s: &str) -> E<String> {
    E::Resource(NativeCoverageV1::unsupported::<ConstraintTargetV1>("constraint family", s))
}
fn bad(s: &str) -> E<String> {
    E::Decode(s.into())
}
fn hash<T: borsh::BorshSerialize>(domain: &[u8], v: &T) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(domain, v)
}
pub(crate) struct ConstraintUniverseV1<'a> {
    frame: &'a Frame,
    source: &'a Source,
    graph: &'a NativeRetainedV1<Graph>,
    profile: Hash64,
    semantics: Hash64,
    table: Hash64,
    tokens: u64,
    output: NativeRetainedV1<Output>,
}
impl<'a> ConstraintUniverseV1<'a> {
    pub(crate) fn derive_native_v1(
        state: &PalwChainStateV2,
        frame: &'a Frame,
        source: &'a Source,
        graph: &'a NativeRetainedV1<Graph>,
        profile: Hash64,
    ) -> Result<NativeRetainedV1<Self>, E<String>> {
        let mut q = TirNativeRequestsV1::new();
        let heap = frame.native_heap_v1("constraint grammar frame scratch");
        q.add_v1(heap.bytes().ok_or(E::Resource(heap))?).map_err(E::Resource)?;
        let bytes = borsh::object_length(frame).map_err(|e| E::Decode(e.to_string()))? as u64;
        q.exact_v1::<u8>(bytes.max(1024).checked_mul(2).ok_or_else(|| resource("constraint hash writer overflow"))?)
            .map_err(E::Resource)?;
        q.exact_v1::<Self>(1).map_err(E::Resource)?;
        let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, frame.plan_v1().original_claim) != Some(frame) {
            return Err(bad("Constraint family substitutes current native frame"));
        }
        graph.check_frame_binding_v3(frame, source, profile).map_err(E::Decode)?;
        let original = frame.original_v1();
        let job = &original.job;
        let p = frame.plan_v1();
        if !job.is_constraint() || !source.supports_tiled_trace_v1() {
            return Err(resource("only tiled native Constraint grammar covered"));
        }
        let s = state.claim_semantics_v1(&p.original_claim).ok_or_else(|| bad("immutable Constraint semantics absent"))?;
        let table = s.token_table_root.ok_or_else(|| bad("Constraint table root absent"))?;
        if !s.matches_context(&source.header_v1().job_context)
            || !s.well_formed_for(&job.class_id, &source.header_v1().artifact_root)
            || s.bound_from_daa > p.terms.accepted_daa
            || Some(table) != crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job)
            || !p
                .semantic
                .as_ref()
                .is_some_and(|x| x.stages.iter().any(|x| x.family == crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Constraint))
        {
            return Err(bad("actual original Constraint stage/immutable table semantics mismatch"));
        }
        let d = source.decode_count_v1();
        let v = source.vocab_v1();
        if d == 0
            || v == 0
            || d != original.decode_tokens_executed
            || d > job.decode_token_limit
            || match original.stop_reason {
                crate::palw_freeprompt_v3::PalwFpStopReasonV3::ExactBudgetReached => d != job.decode_token_limit,
                crate::palw_freeprompt_v3::PalwFpStopReasonV3::EndOfGeneration => d >= job.decode_token_limit,
            }
        {
            return Err(bad("Constraint native stop/count grammar mismatch"));
        }
        let tokens = u64::from(d)
            .checked_mul(u64::from(v))
            .and_then(|x| x.checked_mul(u64::from(v) + 1))
            .ok_or_else(|| resource("Constraint token grammar cardinality overflow"))?;
        tokens.checked_add(1).ok_or_else(|| resource("Constraint total grammar cardinality overflow"))?;
        let output = Output::derive_native_v1(frame, source, graph, profile)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(
            Self { frame, source, graph, profile, semantics: s.commitment(), table, tokens, output },
            lease,
        ))
    }
    pub(crate) fn len_v1(&self) -> u64 {
        self.tokens + 1
    }
    pub(crate) fn target_at_v1(&self, n: u64) -> Option<ConstraintTargetV1> {
        if n > self.tokens {
            return None;
        }
        if n == self.tokens {
            return Some(ConstraintTargetV1::Stop);
        }
        let v = u64::from(self.source.vocab_v1());
        let w = n % (v + 1);
        let rank = n / (v + 1);
        Some(ConstraintTargetV1::Token {
            position: (rank / v) as u32,
            beat: (rank % v) as u32,
            witness: if w == 0 { None } else { Some((w - 1) as u32) },
        })
    }
    pub(crate) fn semantic_supply_v1(&self) -> ConstraintSemanticSupplyV1 {
        ConstraintSemanticSupplyV1::UnresolvedTableSupply { table: self.table, vocabulary: self.source.vocab_v1() }
    }
    pub(crate) fn project_native_v1<'u>(
        &'u self,
        target: ConstraintTargetV1,
        max_visits: u64,
    ) -> Result<NativeRetainedV1<ConstraintDutyV1<'u>>, E<String>> {
        let row = match target {
            ConstraintTargetV1::Token { position, beat, witness } => {
                if position >= self.source.decode_count_v1()
                    || beat >= self.source.vocab_v1()
                    || witness.is_some_and(|x| x >= self.source.vocab_v1())
                {
                    return Err(bad("Constraint target outside native grammar"));
                }
                Some(position)
            }
            ConstraintTargetV1::Stop => None,
        };
        let tiles = if row.is_some() {
            u64::from(self.source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64)
        } else {
            0
        };
        let roles = self.graph.role_slots_v3();
        let count = tiles.checked_mul(roles).ok_or_else(|| resource("Constraint row dependency count overflow"))?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<ConstraintDutyV1>(1).map_err(E::Resource)?;
        q.exact_v1::<ConstraintRowDependencyV1>(count).map_err(E::Resource)?;
        let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        let mut rows = Vec::new();
        let cap = usize::try_from(count).map_err(|_| resource("Constraint row dependency platform overflow"))?;
        rows.try_reserve_exact(cap).map_err(|_| resource("Constraint rows allocation"))?;
        if let Some(row) = row {
            let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(self.frame, self.source, self.profile)
                .map_err(E::Decode)?;
            for (i, n) in self.graph.nodes_v3().enumerate() {
                if i as u64 >= max_visits {
                    return Err(resource("Constraint local graph visits"));
                }
                if let Node::Finish { target: Material::Unit(query), role } = n.kind_v3() {
                    if let Query::TraceTile { row: actual, tile } = set.request_at(*query).map_err(E::Decode)? {
                        if actual == row {
                            if rows.len() == cap {
                                return Err(bad("Constraint excess row suppliers"));
                            }
                            rows.push(ConstraintRowDependencyV1 {
                                tile,
                                role: *role,
                                logical: n.id_v3(),
                                absolute: self.graph.absolute_packet_id_v3(n.id_v3()),
                            });
                        }
                    }
                }
            }
        }
        rows.sort_unstable_by_key(|x| (x.tile, x.role));
        if rows.len() != cap
            || rows.iter().enumerate().any(|(i, x)| u64::from(x.tile) != i as u64 / roles || x.role != i as u64 % roles)
        {
            return Err(bad("Constraint incomplete row supplier set"));
        }
        let logical = hash(b"misaka-palw/constraint-family-logical/v1", &(self.graph.scope_v3(), self.semantics, self.table, target))
            .map_err(E::Decode)?;
        let absolute =
            hash(b"misaka-palw/constraint-family-absolute/v1", &(self.graph.absolute_scope_v3(), logical)).map_err(E::Decode)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(ConstraintDutyV1 { universe: self, target, logical, absolute, rows }, lease))
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ConstraintRowDependencyV1 {
    pub tile: u32,
    pub role: u64,
    pub logical: Hash64,
    pub absolute: [u8; 64],
}
pub(crate) struct ConstraintDutyV1<'a> {
    universe: &'a ConstraintUniverseV1<'a>,
    target: ConstraintTargetV1,
    logical: Hash64,
    absolute: Hash64,
    rows: Vec<ConstraintRowDependencyV1>,
}
impl ConstraintDutyV1<'_> {
    pub(crate) fn full_ids_dependencies_v1(&self) -> &[FullIdsDependencyV1] {
        self.universe.output.allocation_dependencies_v1()
    }
    pub(crate) fn row_dependencies_v1(&self) -> &[ConstraintRowDependencyV1] {
        &self.rows
    }
    pub(crate) fn semantic_supply_v1(&self) -> ConstraintSemanticSupplyV1 {
        match self.target {
            ConstraintTargetV1::Stop => ConstraintSemanticSupplyV1::UnresolvedEogOnlySupply {
                table: self.universe.table,
                vocabulary: self.universe.source.vocab_v1(),
            },
            _ => self.universe.semantic_supply_v1(),
        }
    }
    pub(crate) fn target_v1(&self) -> ConstraintTargetV1 {
        self.target
    }
    pub(crate) fn logical_id_v1(&self) -> Hash64 {
        self.logical
    }
    pub(crate) fn absolute_id_v1(&self) -> Hash64 {
        self.absolute
    }
    fn check_v1(&self, state: &PalwChainStateV2, claim: Hash64, scope: Hash64, semantics: Hash64) -> Result<(), String> {
        let c = crate::palw_court_shared_tir_state_v1::context(state, claim)?;
        let source = c.source(state)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
            .ok_or("Constraint duty current original absent")?;
        self.universe.graph.check_frame_binding_v3(frame, &source, c.profile_id)?;
        if claim != self.universe.frame.plan_v1().original_claim
            || scope != c.scope_id
            || semantics != self.universe.semantics
            || source.source_id_v1() != self.universe.source.source_id_v1()
            || state.claim_semantics_v1(&claim).map(|x| x.commitment()) != Some(semantics)
        {
            return Err("Constraint duty current original/source/semantics mismatch".into());
        }
        Ok(())
    }
}
impl Store<'_> {
    pub(crate) fn build_constraint_token_with_duty_v1<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &TokenIntent,
        material: &'a NativeRetainedV1<SemanticTableMaterialV1>,
        budget: WorkBudget,
        duty: &NativeRetainedV1<ConstraintDutyV1<'_>>,
    ) -> Result<ConstraintTerminalEvidenceV1<'a>, String> {
        duty.check_v1(state, intent.claim, intent.scope_id, intent.semantics_id)?;
        if duty.target != (ConstraintTargetV1::Token { position: intent.position, beat: intent.beat_lane, witness: intent.witness }) {
            return Err("Constraint token intent differs from native projected target".into());
        }
        self.build_constraint_terminal_retained_v1(state, params, rules, intent, material, budget)
    }
    pub(crate) fn build_constraint_stop_with_duty_v1<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &StopIntent,
        material: &'a NativeRetainedV1<SemanticTableMaterialV1>,
        budget: WorkBudget,
        duty: &NativeRetainedV1<ConstraintDutyV1<'_>>,
    ) -> Result<ConstraintStopEvidenceV1<'a>, String> {
        duty.check_v1(state, intent.claim, intent.scope_id, intent.semantics_id)?;
        if duty.target != ConstraintTargetV1::Stop {
            return Err("Constraint stop intent cannot use token duty".into());
        }
        self.build_constraint_stop_terminal_retained_v1(state, params, rules, intent, material, budget)
    }
}

impl Store<'_> {
    /// Selected target only: borrowed actual EOG/renderings may be a strict
    /// subset of potential table supply. Existing replay authenticates and
    /// demands every byte needed by this target; caller retains material owners.
    pub(crate) fn build_constraint_token_material_with_duty_v1<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &TokenIntent,
        material: super::constraint_terminal::ConstraintMaterialV1<'a>,
        budget: WorkBudget,
        duty: &NativeRetainedV1<ConstraintDutyV1<'_>>,
    ) -> Result<ConstraintTerminalEvidenceV1<'a>, String> {
        duty.check_v1(state, intent.claim, intent.scope_id, intent.semantics_id)?;
        if duty.target != (ConstraintTargetV1::Token { position: intent.position, beat: intent.beat_lane, witness: intent.witness }) {
            return Err("Constraint token intent differs from native projected target".into());
        }
        self.build_constraint_terminal(state, params, rules, intent, material, budget)
    }
    /// Stop requires its actual EOG operand, not all vocabulary renderings.
    pub(crate) fn build_constraint_stop_material_with_duty_v1<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        rules: &NativeCourtRulesV1,
        intent: &StopIntent,
        material: super::constraint_terminal::ConstraintMaterialV1<'a>,
        budget: WorkBudget,
        duty: &NativeRetainedV1<ConstraintDutyV1<'_>>,
    ) -> Result<ConstraintStopEvidenceV1<'a>, String> {
        duty.check_v1(state, intent.claim, intent.scope_id, intent.semantics_id)?;
        if duty.target != ConstraintTargetV1::Stop {
            return Err("Constraint stop intent cannot use token duty".into());
        }
        self.build_constraint_stop_terminal_v1(state, params, rules, intent, material, budget)
    }
}

impl ConstraintUniverseV1<'_> {
    pub(super) fn aggregate_binding_v1(&self) -> super::original_aggregate::FamilyBindingV1 {
        (
            self.frame.plan_v1().original_claim,
            self.source.source_id_v1(),
            self.graph.scope_v3(),
            self.graph.absolute_scope_v3(),
            self.profile,
        )
    }
}

impl ConstraintUniverseV1<'_> {
    pub(super) fn aggregate_semantics_v1(&self) -> Hash64 {
        self.semantics
    }
}
