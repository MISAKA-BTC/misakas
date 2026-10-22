//! Native-stamped V2 material. Retrieval authenticates committed bytes only;
//! it neither adjudicates arithmetic nor supplies selected-verifier approval.
//! Old authority tuples and NativeHeldComplete bytes keep their V1 meaning.
use crate::palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1};
use crate::palw_material_capacity_v1::{
    PALW_MATERIAL_MAX_UNITS_V1, PalwMaterialCapacityErrorV1 as E, PalwMaterialCodecV1, PalwMaterialScopeV1,
};
use crate::palw_native_resource_v1::{NativeRetainedV1, TirNativeRequestsV1, native_value_requests_v1, reserve_native_requests_v1};
use crate::palw_reader_router_v2::{PalwReaderRouterErrorV2, PalwReaderRouterRowV2};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateV2Error};
use crate::palw_step_leg::PalwStepBindingV2;
use crate::palw_step_refute::routing_v2::{PalwRoutedStepRefutationV2, PalwRoutingCourtErrorV2, check_routed_step_committed_half_v2};
use crate::{Hash64, config::params::Params};
use borsh::{BorshDeserialize, BorshSerialize};

fn routing(e: PalwReaderRouterErrorV2) -> E {
    match e {
        PalwReaderRouterErrorV2::Resource(e) => E::LocalResource(e),
        other => E::Authentication(other.to_string()),
    }
}
fn terminal(e: PalwRoutingCourtErrorV2) -> E {
    match e {
        PalwRoutingCourtErrorV2::Resource(e) => E::LocalResource(e),
        other => E::Authentication(other.to_string()),
    }
}
fn state(e: PalwStateV2Error) -> E {
    match e {
        PalwStateV2Error::NativeResource(e) => E::LocalResource(e),
        other => E::Authentication(other.to_string()),
    }
}
fn stream_id<T: BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    struct Stream(blake2b_simd::State);
    impl std::io::Write for Stream {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut out = Stream(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    borsh::to_writer(&mut out, value).expect("hash stream cannot fail");
    Hash64::from_bytes(out.0.finalize().as_bytes().try_into().expect("64-byte digest"))
}

/// Separate journal family. Decoding a historical record does NOT provide
/// current native authority. Reopening verifies bytes against this identity;
/// serving/signing requires a fresh native-positive authority and exact equality.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialAuthorityRecordV2 {
    pub version: u16,
    pub network: Hash64,
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub accepted_daa: u64,
    pub ladder: u64,
    pub form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    pub routing: PalwReaderRouterRowV2,
}
impl PalwMaterialAuthorityRecordV2 {
    pub fn id(&self) -> Hash64 {
        stream_id(b"MISAKA/PALW/ROUTED-AUTHORITY/V2", self)
    }
    pub fn validate(&self, binding: &PalwStepBindingV2) -> Result<(), E> {
        if self.version != 2
            || self.claim == Hash64::default()
            || self.network == Hash64::default()
            || self.execution_root != binding.committed_execution_root
            || self.ladder == 0
            || binding.step_leaf_count == 0
            || binding.step_leaf_count > self.ladder
            || self.class_id != self.routing.declaration.class_id
            || self.artifact_root != self.routing.declaration.artifact_root
            || self.accepted_daa < self.routing.bound_from_daa
        {
            return Err(E::Substitution);
        }
        self.routing.declaration.check_profile(&binding.shape_profile).map_err(routing)?;
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<u8>((borsh::object_length(&binding.shape_profile).map_err(|_| E::Capacity)? as u64).max(1024))
            .map_err(E::LocalResource)?;
        q.vec_v1::<u8>((754 + binding.job_context.network_id.len() as u64).max(1024)).map_err(E::LocalResource)?;
        let _hash = reserve_native_requests_v1(q.bytes_v1()).map_err(E::LocalResource)?;
        crate::palw_step_leg::verify_binding_v1(binding).map(|_| ()).map_err(|e| E::Authentication(e.to_string()))
    }
}

/// Only native-state constructors grant this ephemeral authority. A network
/// caller cannot promote a deserialized journal record into current authority.
#[derive(Debug)]
pub struct PalwMaterialNativeAuthorityV2 {
    record: NativeRetainedV1<PalwMaterialAuthorityRecordV2>,
}
impl PalwMaterialNativeAuthorityV2 {
    pub fn record(&self) -> &PalwMaterialAuthorityRecordV2 {
        &self.record
    }
    pub fn from_native_claim(
        native: &PalwChainStateV2,
        params: &Params,
        network: Hash64,
        claim_id: Hash64,
        network_ladder: u64,
    ) -> Result<Self, E> {
        let claim = native.claim(&claim_id).ok_or(E::Incomplete)?;
        let row = native.reader_router_of_claim_v2(claim_id).map_err(state)?.ok_or(E::Incomplete)?;
        Self::from_row(
            native,
            params,
            network,
            claim_id,
            claim.class_id,
            claim.execution_root,
            claim.accepted_daa,
            network_ladder,
            &row,
        )
    }
    /// Pre-admission Ready uses the processor's own candidate state and actual
    /// attempt identity/DAA, before the claim exists. The processor still checks
    /// the entire signed attempt and own parent/import generation independently.
    pub fn from_native_candidate(
        native: &PalwChainStateV2,
        params: &Params,
        network: Hash64,
        claim: Hash64,
        class: Hash64,
        execution: Hash64,
        accepted_daa: u64,
        network_ladder: u64,
    ) -> Result<Self, E> {
        if let Some(existing) = native.claim(&claim) {
            if existing.class_id != class || existing.execution_root != execution || existing.accepted_daa != accepted_daa {
                return Err(E::Substitution);
            }
            return Self::from_native_claim(native, params, network, claim, network_ladder);
        }
        let row = native.reader_router_declaration_v2(class).map_err(state)?.ok_or(E::Incomplete)?;
        Self::from_row(native, params, network, claim, class, execution, accepted_daa, network_ladder, &row)
    }
    fn from_row(
        native: &PalwChainStateV2,
        params: &Params,
        network: Hash64,
        claim: Hash64,
        class: Hash64,
        execution: Hash64,
        accepted_daa: u64,
        network_ladder: u64,
        row: &PalwReaderRouterRowV2,
    ) -> Result<Self, E> {
        if !params.palw_memory_routing_court_v2.is_some_and(|f| f.is_active(accepted_daa))
            || accepted_daa < row.bound_from_daa
            || row.declaration.class_id != class
            || claim == Hash64::default()
            || network == Hash64::default()
        {
            return Err(E::Substitution);
        }
        if native.kernel_route().is_none_or(|route| Hash64::from_bytes(route.header.policy.network_domain) != network) {
            return Err(E::Substitution);
        }
        let class_row = native.class(&class).ok_or(E::Incomplete)?;
        if class_row.artifact_root != row.declaration.artifact_root || class_row.registrant_bond != Some(row.registrant) {
            return Err(E::Substitution);
        }
        row.declaration.check_profile(native.fp_work_profile_of(&class).ok_or(E::Incomplete)?).map_err(routing)?;
        let mut q = TirNativeRequestsV1::new();
        q.add_v1(native_value_requests_v1(row).map_err(E::LocalResource)?).map_err(E::LocalResource)?;
        q.vec_v1::<PalwMaterialAuthorityRecordV2>(1).map_err(E::LocalResource)?;
        let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::LocalResource)?;
        Ok(Self {
            record: NativeRetainedV1::from_owner_reserved_v1(
                PalwMaterialAuthorityRecordV2 {
                    version: 2,
                    network,
                    claim,
                    execution_root: execution,
                    class_id: class,
                    artifact_root: class_row.artifact_root,
                    accepted_daa,
                    ladder: native.class_step_ladder_v1(&class, network_ladder),
                    form: params.palw_prompt_ids_form_at(accepted_daa),
                    routing: row.clone(),
                },
                lease,
            ),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwHeldMaterialUnitV2 {
    /// V1 membership for non-routed units; a StepLeaf carries the committed
    /// half only and no model parameters. No arithmetic verdict is required.
    Legacy(PalwHeldDisclosureV1),
    Routed(PalwRoutedStepRefutationV2),
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwHeldMaterialBundleV2 {
    pub version: u16,
    pub claim: Hash64,
    pub authority: Hash64,
    pub binding: Box<PalwStepBindingV2>,
    pub units: Vec<(PalwHeldMissingV1, PalwHeldMaterialUnitV2)>,
}
impl PalwHeldMaterialBundleV2 {
    /// Strict canonical demand order replaces a mutable seen-set. In-range
    /// unique units with exact cardinalities cover every committed unit.
    pub fn verify(&self, authority: &PalwMaterialAuthorityRecordV2, max_bytes: u64) -> Result<PalwMaterialScopeV1, E> {
        if self.version != 2 || self.claim != authority.claim || self.authority != authority.id() {
            return Err(E::Substitution);
        }
        let bytes = borsh::object_length(self).map_err(|_| E::Capacity)? as u64;
        if bytes > max_bytes || self.units.len() > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        authority.validate(&self.binding)?;
        let b = &self.binding;
        let prompt = if crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(authority.form, &b.shape_profile)
            == crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
        {
            crate::palw_prompt_ids_v1::prompt_ids_tile_count_v1(u64::from(b.job_context.declared_prefill_tokens)).ok_or(E::Capacity)?
        } else {
            0
        };
        let mut chunks = 0u64;
        if b.checkpoint_count as usize > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        for checkpoint in 0..b.checkpoint_count {
            let covered = crate::palw_context_ladder::palw_checkpoint_covered_at_index_v1(
                &b.shape_profile,
                checkpoint,
                b.checkpoint_profile.checkpoint_interval,
            )
            .ok_or(E::Incomplete)?;
            let positions = crate::palw_context_ladder::palw_checkpoint_positions_at_v1(&b.shape_profile, &b.job_context, covered);
            chunks = chunks
                .checked_add(
                    crate::palw_state_chunk_map::palw_state_chunk_count_at_v1(&b.shape_profile, positions).ok_or(E::Incomplete)?,
                )
                .ok_or(E::Capacity)?;
        }
        let ranges = b.step_leaf_count.div_ceil(u64::from(crate::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1));
        let required = prompt
            .checked_add(chunks)
            .and_then(|n| n.checked_add(ranges))
            .and_then(|n| n.checked_add(b.step_leaf_count))
            .ok_or(E::Capacity)?;
        if required > PALW_MATERIAL_MAX_UNITS_V1 as u64 || self.units.len() as u64 != required {
            return Err(E::Incomplete);
        }
        let mut counts = [0u64; 4];
        let mut previous = None;
        for (missing, value) in &self.units {
            if previous.is_some_and(|p| p >= *missing) {
                return Err(E::Substitution);
            }
            previous = Some(*missing);
            let category = match missing {
                PalwHeldMissingV1::PromptIdsTile { .. } => 0,
                PalwHeldMissingV1::StateChunk { .. } => 1,
                PalwHeldMissingV1::StepRange { first, count } => {
                    let max = u64::from(crate::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1);
                    if *first >= b.step_leaf_count || first % max != 0 || u64::from(*count) != (b.step_leaf_count - first).min(max) {
                        return Err(E::Incomplete);
                    }
                    2
                }
                PalwHeldMissingV1::StepLeaf { leaf } => {
                    if *leaf >= b.step_leaf_count {
                        return Err(E::Incomplete);
                    }
                    3
                }
            };
            verify_material_unit_v2(authority, b, *missing, value)?;
            counts[category] += 1;
        }
        if counts != [prompt, chunks, ranges, b.step_leaf_count] {
            return Err(E::Incomplete);
        }
        Ok(PalwMaterialScopeV1 {
            claim: self.claim,
            commitment: authority.execution_root,
            codec: PalwMaterialCodecV1::NativeHeldCompleteV2,
            bytes,
            units: self.units.len() as u32,
            content: stream_id(b"MISAKA/PALW/ROUTED-BUNDLE/V2", self),
        })
    }
}

pub fn verify_material_unit_v2(
    authority: &PalwMaterialAuthorityRecordV2,
    binding: &PalwStepBindingV2,
    missing: PalwHeldMissingV1,
    value: &PalwHeldMaterialUnitV2,
) -> Result<(), E> {
    authority.validate(binding)?;
    match (missing, value) {
        (PalwHeldMissingV1::StepLeaf { leaf }, PalwHeldMaterialUnitV2::Routed(r)) => {
            if r.step.binding != *binding || r.step.output_opening.leaf_index != leaf {
                return Err(E::Substitution);
            }
            check_routed_step_committed_half_v2(r, authority.execution_root, &authority.routing.declaration, authority.ladder)
                .map_err(terminal)
        }
        (_, PalwHeldMaterialUnitV2::Legacy(d)) => {
            crate::palw_held_da_v1::palw_held_da_check_disclosure_v1(
                &authority.execution_root,
                &missing,
                binding,
                d,
                authority.ladder,
                authority.form,
            )
            .map_err(|e| E::Authentication(e.to_string()))?;
            if let PalwHeldDisclosureV1::StepLeaf { evidence } = d {
                let r = &evidence.refutation;
                let mut q = TirNativeRequestsV1::new();
                q.vec_v1::<(u128, u128, u128)>(u64::from(binding.shape_profile.global_node_count())).map_err(E::LocalResource)?;
                q.vec_v1::<(u128, u128, u128)>(u64::from(binding.shape_profile.global_node_count())).map_err(E::LocalResource)?;
                q.add_v1(native_value_requests_v1(r).map_err(E::LocalResource)?).map_err(E::LocalResource)?;
                let _scratch = reserve_native_requests_v1(q.bytes_v1()).map_err(E::LocalResource)?;
                let coord = crate::palw_step::canonical_step_coordinates(
                    &binding.shape_profile,
                    &binding.job_context,
                    r.output_opening.leaf_index,
                )
                .ok_or(E::Incomplete)?;
                let (node, _) = binding.shape_profile.resolve_node_slot(coord.node_slot).ok_or(E::Incomplete)?;
                if crate::palw_reader_router_v2::needs_reader_router_v2(node) || !evidence.artifact_openings.is_empty() {
                    return Err(E::Substitution);
                }
                crate::palw_step_refute::check_execution_step_committed_half_v1(
                    r,
                    evidence.prompt_ids_opening.as_ref(),
                    authority.ladder,
                )
                .map_err(|e| E::Authentication(e.to_string()))?;
            }
            Ok(())
        }
        _ => Err(E::Substitution),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwRoutedMaterialTargetV2 {
    DaHeld,
    DaKernelWitness,
    Monetary { root: Hash64, consent: Hash64 },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRoutedMaterialReceiptV2 {
    pub stream: Hash64,
    pub sequence: u64,
    pub previous: Hash64,
    pub challenge_deadline_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRoutedMaterialAnswerV2 {
    pub version: u16,
    pub claim: Hash64,
    pub provider: PalwBondKeyV2,
    pub target: PalwRoutedMaterialTargetV2,
    pub refutation: PalwRoutedStepRefutationV2,
    pub receipt: Option<PalwRoutedMaterialReceiptV2>,
    pub signature: Vec<u8>,
}
pub const PALW_ROUTED_MATERIAL_CONTEXT_V2: &[u8] = crate::palw_da_rcore_v1::PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT;
impl PalwRoutedMaterialAnswerV2 {
    /// Sign the complete answer and explicit target, without allocating a
    /// canonical temporary. No signature transfers between V1 and V2 lanes.
    pub fn message(&self, network: Hash64) -> Hash64 {
        stream_id(
            b"MISAKA/PALW/ROUTED-ANSWER/V2",
            &(network, self.version, self.claim, self.provider, &self.target, &self.refutation, &self.receipt),
        )
    }
    pub fn check_shape(&self, ladder: u64) -> Result<(), &'static str> {
        if self.version != 2 || self.refutation.version != 2 || self.refutation.step.output_opening.leaf_index >= ladder {
            return Err("routed material answer version/leaf exceeds native ladder");
        }
        Ok(())
    }
}

/// Domain-local typed planner exposed to capture backends; generic native
/// resource internals remain private to Core. Source types/counts are quoted
/// before allocation, never inferred by multiplying untrusted encoded bytes.
pub struct PalwMaterialConstructionRequestsV2(TirNativeRequestsV1);
impl PalwMaterialConstructionRequestsV2 {
    pub fn new() -> Self {
        Self(TirNativeRequestsV1::new())
    }
    pub fn add_v1(&mut self, bytes: u64) -> Result<(), crate::palw_native_resource_v1::NativeCoverageV1> {
        self.0.add_v1(bytes)
    }
    pub fn vec_v1<T>(&mut self, count: u64) -> Result<(), crate::palw_native_resource_v1::NativeCoverageV1> {
        self.0.vec_v1::<T>(count)
    }
    pub fn bytes_v1(self) -> u64 {
        self.0.bytes_v1()
    }
}
impl Default for PalwMaterialConstructionRequestsV2 {
    fn default() -> Self {
        Self::new()
    }
}

/// A backend quotes concrete typed scratch before invoking its existing capture
/// decoder/builders. The owner remains live until the complete bundle adopts it.
pub fn construct_material_unit_v2(
    mut requests: PalwMaterialConstructionRequestsV2,
    build: impl FnOnce() -> Result<PalwHeldMaterialUnitV2, E>,
) -> Result<NativeRetainedV1<PalwHeldMaterialUnitV2>, E> {
    requests.vec_v1::<NativeRetainedV1<PalwHeldMaterialUnitV2>>(1).map_err(E::LocalResource)?;
    let count = requests.bytes_v1();
    let lease = reserve_native_requests_v1(count).map_err(E::LocalResource)?;
    let value = build()?;
    if native_value_requests_v1(&value).map_err(E::LocalResource)? > count {
        return Err(E::LocalResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<PalwHeldMaterialUnitV2>(
            "routed material construction",
            "backend construction exceeds its admitted native request",
        )));
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
}
impl PalwHeldMaterialBundleV2 {
    /// Enumerate the actual binding's entire demand space. Every callback result
    /// owns its construction ticket; one cumulative destination lease adopts
    /// each unit before the source ticket is released. No lease-vector growth.
    pub fn from_source(
        binding: &PalwStepBindingV2,
        authority: &PalwMaterialNativeAuthorityV2,
        max_bytes: u64,
        mut source: impl FnMut(PalwHeldMissingV1) -> Result<NativeRetainedV1<PalwHeldMaterialUnitV2>, E>,
    ) -> Result<NativeRetainedV1<Self>, E> {
        let a = authority.record();
        a.validate(binding)?;
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<PalwHeldMissingV1>(PALW_MATERIAL_MAX_UNITS_V1 as u64).map_err(E::LocalResource)?;
        let _demand_space = reserve_native_requests_v1(q.bytes_v1()).map_err(E::LocalResource)?;
        let mut missing = Vec::with_capacity(PALW_MATERIAL_MAX_UNITS_V1);
        let mut push = |m| {
            if missing.len() == PALW_MATERIAL_MAX_UNITS_V1 {
                return Err(E::Capacity);
            }
            missing.push(m);
            Ok(())
        };
        if crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(a.form, &binding.shape_profile)
            == crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
        {
            let count = crate::palw_prompt_ids_v1::prompt_ids_tile_count_v1(u64::from(binding.job_context.declared_prefill_tokens))
                .ok_or(E::Capacity)?;
            if count > PALW_MATERIAL_MAX_UNITS_V1 as u64 {
                return Err(E::Capacity);
            }
            for tile in 0..count {
                push(PalwHeldMissingV1::PromptIdsTile { tile: tile.try_into().map_err(|_| E::Capacity)? })?;
            }
        }
        if binding.checkpoint_count as usize > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        for checkpoint in 0..binding.checkpoint_count {
            let covered = crate::palw_context_ladder::palw_checkpoint_covered_at_index_v1(
                &binding.shape_profile,
                checkpoint,
                binding.checkpoint_profile.checkpoint_interval,
            )
            .ok_or(E::Incomplete)?;
            let positions =
                crate::palw_context_ladder::palw_checkpoint_positions_at_v1(&binding.shape_profile, &binding.job_context, covered);
            let count =
                crate::palw_state_chunk_map::palw_state_chunk_count_at_v1(&binding.shape_profile, positions).ok_or(E::Incomplete)?;
            if count > PALW_MATERIAL_MAX_UNITS_V1 as u64 {
                return Err(E::Capacity);
            }
            for chunk in 0..count {
                push(PalwHeldMissingV1::StateChunk { checkpoint, chunk: chunk.try_into().map_err(|_| E::Capacity)? })?;
            }
        }
        let range = crate::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1;
        if binding.step_leaf_count > PALW_MATERIAL_MAX_UNITS_V1 as u64 {
            return Err(E::Capacity);
        }
        // PalwHeldMissing's frozen Ord discriminants place ranges before leaves.
        for first in (0..binding.step_leaf_count).step_by(range as usize) {
            push(PalwHeldMissingV1::StepRange { first, count: (binding.step_leaf_count - first).min(u64::from(range)) as u32 })?;
        }
        for leaf in 0..binding.step_leaf_count {
            push(PalwHeldMissingV1::StepLeaf { leaf })?;
        }
        let mut q = TirNativeRequestsV1::new();
        q.add_v1(native_value_requests_v1(binding).map_err(E::LocalResource)?).map_err(E::LocalResource)?;
        q.vec_v1::<(PalwHeldMissingV1, PalwHeldMaterialUnitV2)>(missing.len() as u64).map_err(E::LocalResource)?;
        let mut lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::LocalResource)?;
        let mut value = Self {
            version: 2,
            claim: a.claim,
            authority: a.id(),
            binding: Box::new(binding.clone()),
            units: Vec::with_capacity(missing.len()),
        };
        let mut encoded = borsh::object_length(&value).map_err(|_| E::Capacity)? as u64;
        for m in missing {
            let unit = source(m)?;
            verify_material_unit_v2(a, binding, m, &unit)?;
            encoded = encoded.checked_add(borsh::object_length(&(m, &*unit)).map_err(|_| E::Capacity)? as u64).ok_or(E::Capacity)?;
            if encoded > max_bytes {
                return Err(E::Capacity);
            }
            let copy = native_value_requests_v1(&*unit).map_err(E::LocalResource)?;
            if let Some(l) = &mut lease {
                l.grow_memory_v1(copy).map_err(E::LocalResource)?;
            } else {
                lease = reserve_native_requests_v1(copy).map_err(E::LocalResource)?;
            }
            let (unit, source_lease) = unit.into_parts_v1();
            value.units.push((m, unit));
            drop(source_lease);
        }
        value.verify(a, max_bytes)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_codec_appends_without_reinterpreting_any_old_discriminant() {
        for (codec, tag) in [
            (PalwMaterialCodecV1::NativeHeldComplete, 0),
            (PalwMaterialCodecV1::KernelPositions, 1),
            (PalwMaterialCodecV1::SegmentedParts, 2),
            (PalwMaterialCodecV1::TypedKernelV4, 3),
            (PalwMaterialCodecV1::NativeHeldCompleteV2, 4),
        ] {
            assert_eq!(borsh::to_vec(&codec).unwrap(), vec![tag]);
        }
    }
}
