//! Native family service inventory and data-identity authentication. A supplied body is
//! never an assertion of model correctness, a court verdict or a payment authorization.
use crate::palw_court_family_supplier_v1::FamilySupplierRoleV1 as Role;
use crate::palw_court_shared_material_intake_v1::{self as intake, MaterialTargetV1};
use crate::palw_native_resource_v1::{NativeCoverageV1 as C, NativeDecodeErrorV1 as E, NativeRetainedV1 as Held, TirNativeRequestsV1};
use crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 as Frame;
use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Query, PalwTirPublicSourceV1 as Source};
use crate::{
    Hash64,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const FAMILY_MATERIAL_SIGNATURE_CONTEXT_V2: &[u8] = b"misaka-palw/court-family-material/v2/mldsa87";
pub const FAMILY_MATERIAL_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/court-family-material/v1/mldsa87";
pub const FAMILY_MATERIAL_PART_BYTES_V1: usize = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum FamilyMaterialTargetV1 {
    Input(MaterialTargetV1),
    SemanticTable,
    TokenizerRaw,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum FamilyMaterialActionV1 {
    Begin { total_bytes: u64, body_hash: Hash64 },
    Part { draft: Hash64, offset: u64, bytes: Vec<u8> },
    Finish { draft: Hash64 },
    BeginReserved { total_bytes: u64, body_hash: Hash64, quote: FamilyDraftQuoteV2 },
}
impl BorshDeserialize for FamilyMaterialActionV1 {
    fn deserialize_reader<R: std::io::Read>(r: &mut R) -> std::io::Result<Self> {
        Ok(match u8::deserialize_reader(r)? {
            0 => Self::Begin { total_bytes: u64::deserialize_reader(r)?, body_hash: Hash64::deserialize_reader(r)? },
            1 => Self::Part {
                draft: Hash64::deserialize_reader(r)?,
                offset: u64::deserialize_reader(r)?,
                bytes: crate::palw_public_binding_v1::read_bounded_bytes_v1(r, FAMILY_MATERIAL_PART_BYTES_V1)?,
            },
            2 => Self::Finish { draft: Hash64::deserialize_reader(r)? },
            3 => Self::BeginReserved {
                total_bytes: u64::deserialize_reader(r)?,
                body_hash: Hash64::deserialize_reader(r)?,
                quote: FamilyDraftQuoteV2::deserialize_reader(r)?,
            },
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "family material action")),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedFamilyMaterialV1 {
    pub version: u16,
    pub contract: Hash64,
    pub target: FamilyMaterialTargetV1,
    pub submitter: PalwBondKeyV2,
    pub action: FamilyMaterialActionV1,
    pub signature: Vec<u8>,
}
impl SignedFamilyMaterialV1 {
    pub fn signing_message_v1(&self, network: Hash64) -> Result<Hash64, String> {
        if !matches!(self.version, 1 | 2)
            || (self.version == 1 && matches!(self.action, FamilyMaterialActionV1::BeginReserved { .. }))
            || network == Hash64::default()
            || self.contract == Hash64::default()
        {
            return Err("family material version/network/contract".into());
        }
        digest(
            if self.version == 2 { b"misaka-palw/family-material/control/v2" } else { b"misaka-palw/family-material/control/v1" },
            &(network, self.version, self.contract, self.target, self.submitter, &self.action),
        )
    }
    pub fn signature_context_v2(&self) -> &'static [u8] {
        if self.version == 2 { FAMILY_MATERIAL_SIGNATURE_CONTEXT_V2 } else { FAMILY_MATERIAL_SIGNATURE_CONTEXT_V1 }
    }
    pub fn reserved_response_horizon_v2(&self) -> Option<u64> {
        match &self.action {
            FamilyMaterialActionV1::BeginReserved { quote, .. } => Some(quote.respond_until_daa),
            _ => None,
        }
    }
    pub fn verify_v1(&self, network: Hash64, key: &[u8]) -> Result<(), String> {
        if self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                key,
                self.signing_message_v1(network)?.as_byte_slice(),
                &self.signature,
                self.signature_context_v2(),
            )
        {
            return Err("family material signature invalid".into());
        }
        Ok(())
    }
}
fn digest<T: BorshSerialize>(domain: &[u8], value: &T) -> Result<Hash64, String> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(domain, value)
}
fn bad(s: impl Into<String>) -> E<String> {
    E::Decode(s.into())
}
fn local_allocation(e: impl ToString) -> E<String> {
    E::Resource(C::unsupported::<NativeFamilyInventoryV1>("family material allocation", &e.to_string()))
}
fn add(a: u64, b: u64) -> Result<u64, E<String>> {
    a.checked_add(b).ok_or_else(|| E::Resource(C::overflow::<NativeFamilyInventoryV1>("family material bytes")))
}
/// Opaque native-derived inventory. No wire decoder or caller-written completion bit.
/// Bounds are service reservation upper bounds; they are not actual supplied bytes.
#[derive(Debug)]
pub struct NativeFamilyInventoryV1 {
    claim: Hash64,
    source: Hash64,
    relative_graph: Hash64,
    role: Role,
    id: Hash64,
    targets: Vec<(FamilyMaterialTargetV1, u64)>,
    maximum_bytes: u64,
    // Local data-identity memo for this opaque native derivation; never encoded.
    source_inputs: Hash64,
    native_inclusion_margin_daa_v2: u64,
    // Opaque native-only derivation purpose; never a carrier-selected subset.
    generated_semantic_only_v3: bool,
    carrier_projection_v3: Option<FamilyCarrierProjectionFactsV3>,
}
impl NativeFamilyInventoryV1 {
    pub fn carrier_input_bound_v3(&self) -> Option<u64> {
        self.carrier_projection_v3.map(|f| f.input_bound)
    }
    pub fn carrier_coordinates_v3(&self) -> Option<Hash64> {
        self.carrier_projection_v3.map(|f| f.coordinates)
    }
    pub(crate) fn carrier_projection_v3(&self) -> Option<FamilyCarrierProjectionFactsV3> {
        self.carrier_projection_v3
    }
    pub(crate) fn generated_semantic_only_v3(&self) -> bool {
        self.generated_semantic_only_v3
    }
    pub(crate) fn native_inclusion_margin_v2(&self) -> u64 {
        self.native_inclusion_margin_daa_v2
    }
    pub fn claim_v1(&self) -> Hash64 {
        self.claim
    }
    pub fn source_v1(&self) -> Hash64 {
        self.source
    }
    pub fn relative_graph_v1(&self) -> Hash64 {
        self.relative_graph
    }
    pub fn role_v1(&self) -> Role {
        self.role
    }
    pub fn id_v1(&self) -> Hash64 {
        self.id
    }
    pub fn targets_v1(&self) -> &[(FamilyMaterialTargetV1, u64)] {
        &self.targets
    }
    pub fn max_material_bytes_v1(&self) -> u64 {
        self.maximum_bytes
    }
    pub fn target_bound_v1(&self, target: FamilyMaterialTargetV1) -> Result<u64, String> {
        self.targets
            .binary_search_by_key(&target, |x| x.0)
            .ok()
            .map(|i| self.targets[i].1)
            .ok_or_else(|| "family material target outside complete native inventory".into())
    }
    pub fn target_id_v1(&self, target: FamilyMaterialTargetV1) -> Result<Hash64, String> {
        self.target_bound_v1(target)?;
        digest(b"misaka-palw/family-material/target/v1", &(self.id, target))
    }
    pub fn packet_count_v1(&self) -> Result<u64, String> {
        self.targets.iter().try_fold(0u64, |n, (_, bytes)| {
            n.checked_add(bytes.div_ceil(FAMILY_MATERIAL_PART_BYTES_V1 as u64))
                .and_then(|x| x.checked_add(2))
                .ok_or_else(|| "family packet count overflow".into())
        })
    }
}
/// Opaque exact EOF inventory derived only by authenticating the complete canonical
/// table against the immutable rendering/EOG root and native vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckedFamilyTableInventoryV1 {
    root: Hash64,
    tokenizer: Hash64,
    vocab: u32,
    bytes: u64,
}
crate::palw_native_resource_v1::native_struct!(CheckedFamilyTableInventoryV1; root, tokenizer, vocab, bytes);
impl CheckedFamilyTableInventoryV1 {
    /// Authority comes only from canonical EOF authentication, never a declared length.
    pub(crate) fn exact_bytes_for_context_v2(
        &self,
        semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
        vocab: u32,
    ) -> Result<u64, String> {
        if semantics.token_table_root != Some(self.root)
            || semantics.tokenizer_id != self.tokenizer
            || vocab != self.vocab
            || self.bytes == 0
            || self.bytes > crate::palw_fp_constraint_job_v1::PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1 as u64
        {
            return Err("authenticated table differs from native immutable semantic context".into());
        }
        Ok(self.bytes)
    }
    /// Conservative keyed hash work for the authenticated EOF: rendering/EOG
    /// leaves, index-wrapped leaves, promote-odd internal nodes and body hashes.
    /// Per-leaf rounding and the implicit token ids must not disappear when
    /// many vocabulary entries have empty or absent renderings.
    pub(crate) fn authentication_blake_blocks_bound_v2(
        &self,
        semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
        vocab: u32,
    ) -> Result<u64, String> {
        let bytes = self.exact_bytes_for_context_v2(semantics, vocab)?;
        u64::from(vocab)
            .checked_mul(8)
            .and_then(|work| bytes.div_ceil(128).checked_mul(2).and_then(|body| work.checked_add(body)))
            .and_then(|work| work.checked_add(8))
            .ok_or_else(|| "authenticated table hash-work overflow".into())
    }
}
pub fn prepare_family_table_inventory_v1(
    bytes: &[u8],
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    vocab: u32,
) -> Result<CheckedFamilyTableInventoryV1, E<String>> {
    let root = semantics.token_table_root.ok_or_else(|| bad("family table immutable root missing"))?;
    authenticate_table_v1(bytes, vocab, root, semantics.tokenizer_id)?;
    Ok(CheckedFamilyTableInventoryV1 { root, tokenizer: semantics.tokenizer_id, vocab, bytes: bytes.len() as u64 })
}
/// Data retained by native admission for exact historical allocation re-derivation.
/// This is not an incoming grant or a substitute for the configured native rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilyOriginalRuleContextV2 {
    pub court: crate::palw_mode_v2::PalwCourtParamsV2,
    pub dissection: bool,
    pub prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
}
impl FamilyOriginalRuleContextV2 {
    pub(crate) fn from_native_v2(rules: crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationRulesV1) -> Self {
        Self { court: rules.court, dissection: rules.dissection, prompt_form: rules.prompt_form }
    }
    pub(crate) fn native_rules_v2(
        &self,
        descriptor: &crate::palw_tir_public_state_v1::PalwTirPublicSourceDescriptorV1,
    ) -> crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationRulesV1 {
        crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationRulesV1 {
            maximum_leaves: descriptor.maximum_leaves,
            maximum_custodians: descriptor.native_custodian_upper,
            court: self.court,
            dissection: self.dissection,
            prompt_form: self.prompt_form,
            config: descriptor.native_config,
            clock: descriptor.native_clock,
        }
    }
}
crate::palw_native_resource_v1::native_struct!(FamilyOriginalRuleContextV2; court, dissection, prompt_form);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyOriginalRuleContextV2; crate::palw_mode_v2::PalwCourtParamsV2, bool, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1);
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilyTableInventoryRowV1 {
    pub claim: Hash64,
    /// Checked original carrier's funding outpoint, not a material uploader.
    pub carrier: crate::tx::TransactionOutpoint,
    pub allocation_id: Hash64,
    pub allocation_rules_v2: Option<FamilyOriginalRuleContextV2>,
    pub bytes: Vec<u8>,
    pub material_until_daa: u64,
}
pub(crate) fn read_family_table_inventory_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    vocab: u32,
) -> Result<Option<CheckedFamilyTableInventoryV1>, E<String>> {
    let Some(route) = state.kernel_route_state_v1() else { return Ok(None) };
    let Some(bytes) = route.aux.get(&(FAMILY_MATERIAL_TABLE_V1, material_aux_key_v1(4, claim, None))) else { return Ok(None) };
    let row = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<FamilyTableInventoryRowV1, std::io::Error>(
        bytes,
        || borsh::from_slice(bytes),
    )
    .map_err(|e| match e {
        E::Resource(r) => E::Resource(r),
        E::Decode(e) => bad(e.to_string()),
    })?;
    if row.claim != claim {
        return Err(bad("family table inventory substitutes original"));
    }
    prepare_family_table_inventory_v1(&row.bytes, semantics, vocab).map(Some)
}
/// The original transport is a nonexclusive physical supply of its complete
/// authenticated table. It has carrier/allocation provenance, and is never
/// relabelled as a signed multipart upload by an actor that did not upload it.
pub(crate) fn read_authenticated_family_table_inventory_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
) -> Result<Option<Held<FamilyTableInventoryRowV1>>, E<String>> {
    let Some(route) = state.kernel_route_state_v1() else { return Ok(None) };
    let Some(bytes) = route.aux.get(&(FAMILY_MATERIAL_TABLE_V1, material_aux_key_v1(4, claim, None))) else { return Ok(None) };
    let row = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<FamilyTableInventoryRowV1, std::io::Error>(
        bytes,
        || borsh::from_slice(bytes),
    )
    .map_err(|why| match why {
        E::Resource(r) => E::Resource(r),
        E::Decode(e) => bad(e.to_string()),
    })?;
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .ok_or_else(|| bad("rooted table inventory original absent"))?;
    let now = state.last_point().map_or(0, |p| p.daa_score);
    if row.claim != claim || row.allocation_id != frame.plan_v1().allocation_id {
        return Err(bad("rooted table inventory transport allocation differs"));
    }
    if now > row.material_until_daa {
        return Ok(None);
    }
    let inventory = derive_bound_current_family_inventory_classified_v3(state, claim, Role::SemanticTable)?;
    let target = inventory.target_id_v1(FamilyMaterialTargetV1::SemanticTable).map_err(bad)?;
    let body = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&row.bytes);
    let contracts = state.family_supplier_rows_v1().map_err(|why| match why {
        crate::palw_state_v2::PalwStateV2Error::NativeResource(r) => E::Resource(r),
        other => bad(other.to_string()),
    })?;
    let supplied = contracts.iter().any(|(_, contract)| {
        let terms = &contract.consent.terms;
        terms.claim == claim
            && terms.role == Role::SemanticTable
            && terms.inventory == inventory.id_v1()
            && contract.allocation == row.allocation_id
            && terms.source == inventory.source_v1()
            && terms.relative_graph == inventory.relative_graph_v1()
            && terms.max_material_bytes == inventory.max_material_bytes_v1()
            && now >= contract.accepted_daa
            && now <= terms.material_until_daa
            && row.material_until_daa >= terms.material_until_daa
            && contract.completed.get(&target) == Some(&body)
    });
    if !supplied {
        return Ok(None);
    }
    Ok(Some(row))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn derive_family_inventory_v1(
    state: &PalwChainStateV2,
    frame: &Frame,
    source: &Held<Source>,
    graph: &crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,
    profile: Hash64,
    program: &misaka_palw_tir::program::TirProgramV1,
    cover: &crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1,
    semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    role: Role,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    let table = if role == Role::SemanticTable {
        semantics
            .map(|sem| read_family_table_inventory_v1(state, frame.plan_v1().original_claim, sem, source.vocab_v1()))
            .transpose()?
            .flatten()
    } else {
        None
    };
    derive_family_inventory_with_table_v1(state, frame, source, graph, profile, program, cover, semantics, role, table.as_ref())
}
/// A complete opaque native derivation may be reused while its canonical data
/// inputs are identical. This memo has no wire decoder and grants no computation
/// credit; all material openings are still authenticated below.
fn family_inventory_source_inputs_v1(
    state: &PalwChainStateV2,
    frame: &Frame,
    profile: Hash64,
    semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    role: Role,
    table: Option<&CheckedFamilyTableInventoryV1>,
) -> Result<Hash64, E<String>> {
    let class = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class).ok_or_else(|| bad("family source fingerprint class absent"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class)
        .ok_or_else(|| bad("family source fingerprint layout absent"))?;
    let eof = if role == Role::TokenizerArtifact {
        semantics.and_then(|sem| state.staged_tokenizer_inventory_v1(sem.tokenizer_id))
    } else {
        None
    };
    let table = if role == Role::SemanticTable { table.map(|t| (t.root, t.tokenizer, t.vocab, t.bytes)) } else { None };
    let schedule = intake::material_intake_schedule_v1().map_err(|_| bad("family material adopted cover budget missing"))?;
    let limits = (
        schedule.cover.max_original_leaves,
        schedule.cover.max_total_edges,
        schedule.cover.max_total_coordinates,
        schedule.cover.max_root_memberships,
    );
    digest(
        b"misaka-palw/family-material/local-source-inputs/v1",
        &(frame, record, layout, semantics, eof, profile, role, table, limits),
    )
    .map_err(bad)
}

/// Native preclaim seam: all inputs are retained native originals, registered programs and
/// the complete all-root cover. Semantic roots come from the immutable prospective snapshot.
#[allow(clippy::too_many_arguments)]
pub(crate) fn derive_family_inventory_with_table_v1(
    state: &PalwChainStateV2,
    frame: &Frame,
    source: &Held<Source>,
    graph: &crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,
    profile: Hash64,
    program: &misaka_palw_tir::program::TirProgramV1,
    cover: &crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1,
    semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    role: Role,
    table_inventory: Option<&CheckedFamilyTableInventoryV1>,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    derive_family_inventory_with_table_mode_v3(
        state,
        frame,
        source,
        graph,
        profile,
        program,
        cover,
        semantics,
        role,
        table_inventory,
        false,
    )
}
/// Newly signed profile3 generated-selection/stop service. Numeric and consumed
/// prefix boundary streams retain their original all-branch SourceGraph duties.
#[allow(clippy::too_many_arguments)]
pub(crate) fn derive_family_inventory_with_table_v3(
    state: &PalwChainStateV2,
    frame: &Frame,
    source: &Held<Source>,
    graph: &crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,
    profile: Hash64,
    program: &misaka_palw_tir::program::TirProgramV1,
    cover: &crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1,
    semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    role: Role,
    table_inventory: Option<&CheckedFamilyTableInventoryV1>,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    derive_family_inventory_with_table_mode_v3(
        state,
        frame,
        source,
        graph,
        profile,
        program,
        cover,
        semantics,
        role,
        table_inventory,
        true,
    )
}
fn generated_semantic_query_v2(query: &Query) -> bool {
    matches!(query, Query::RowsNode { .. } | Query::TileNode { .. } | Query::TraceTile { .. })
}
#[allow(clippy::too_many_arguments)]
fn derive_family_inventory_with_table_mode_v3(
    state: &PalwChainStateV2,
    frame: &Frame,
    source: &Held<Source>,
    graph: &crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,
    profile: Hash64,
    program: &misaka_palw_tir::program::TirProgramV1,
    cover: &crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalRootInputCoverV1,
    semantics: Option<&crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    role: Role,
    table_inventory: Option<&CheckedFamilyTableInventoryV1>,
    semantic_v3: bool,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    let semantic_v3 = semantic_v3 && role == Role::PrefixGeneratedSelectionAndStop;
    graph.check_frame_binding_v3(frame, source, profile).map_err(bad)?;
    if cover.parameters_v1().source_id != source.source_id_v1() || cover.parameters_v1().original_leaves != source.leaf_count_v1() {
        return Err(bad("family input cover changes original"));
    }
    let claim = frame.plan_v1().original_claim;
    let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, source, profile).map_err(bad)?;
    let sem = if let Some(sem) = semantics {
        if !sem.matches_context(&source.header_v1().job_context)
            || !sem.well_formed_for(&frame.original_v1().job.class_id, &frame.header_v1().artifact_root)
            || sem.bound_from_daa > frame.plan_v1().terms.accepted_daa
        {
            return Err(bad("family semantic snapshot changes original"));
        }
        Some(sem)
    } else {
        None
    };
    let prompt_count = source.header_v1().job_context.declared_prefill_tokens;
    let prompt_form = frame.plan_v1().owner_work.rules.prompt_form;
    let count = match role {
        Role::ParameterAndPrompt => add(
            cover.parameters_v1().union.len() as u64,
            match prompt_form {
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat => 1,
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 => {
                    crate::palw_prompt_ids_v1::prompt_ids_tile_count_v1(u64::from(prompt_count))
                        .ok_or_else(|| bad("family prompt geometry"))?
                }
            },
        )?,
        Role::SemanticTable | Role::TokenizerArtifact => 1,
        Role::PrefixGeneratedSelectionAndStop => {
            let job = &frame.original_v1().job;
            if job.version != crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION
                || !matches!(job.tail.as_ref(), Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Prefix(_)))
            {
                return Err(bad("family prefix role requires immutable Prefix original"));
            }
            crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| bad(e.to_string()))?;
            if sem.is_none() {
                return Err(bad("family Prefix immutable semantic snapshot absent"));
            }
            // The literal snapshot (including a historical None relation) is bound below.
            // Material supply does not invent a stronger prefix-state semantic relation.
            if semantic_v3 {
                let mut count = 1u64; // exact full FullIds body
                for ordinal in 0..set.len() {
                    if generated_semantic_query_v2(&set.request_at(ordinal).map_err(bad)?) {
                        count = add(count, 1)?;
                    }
                }
                count
            } else {
                add(set.len(), 1)?
            }
        }
    };
    let mut memory = TirNativeRequestsV1::new();
    memory.exact_v1::<NativeFamilyInventoryV1>(1).map_err(E::Resource)?;
    memory.exact_v1::<(FamilyMaterialTargetV1, u64)>(count).map_err(E::Resource)?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(memory.bytes_v1()).map_err(E::Resource)?;
    let mut targets = Vec::new();
    targets.try_reserve_exact(usize::try_from(count).map_err(|_| bad("family inventory host count"))?).map_err(local_allocation)?;
    match role {
        Role::ParameterAndPrompt => {
            let input = intake::input_material_demand_v1(program, &cover.parameters_v1().union, prompt_form, prompt_count)?;
            for leaf in &cover.parameters_v1().union {
                let n = crate::palw_tir_court_v1::palw_tir_param_carriage_bytes_v1(
                    program,
                    crate::palw_tir_court_v1::PalwTirParamCarriageV1::PerLeafOpenings,
                    &[*leaf],
                )
                .and_then(|n| n.checked_sub(4))
                .ok_or_else(|| bad("family parameter opening width"))?;
                targets.push((FamilyMaterialTargetV1::Input(MaterialTargetV1::Parameter { leaf: *leaf }), n));
            }
            match prompt_form {
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat => {
                    targets.push((FamilyMaterialTargetV1::Input(MaterialTargetV1::PromptFlat), input.prompt_bytes))
                }
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 => {
                    for tile in 0..input.prompt_targets {
                        let tile = u32::try_from(tile).map_err(|_| bad("family prompt tile index"))?;
                        targets.push((
                            FamilyMaterialTargetV1::Input(MaterialTargetV1::PromptTile { tile }),
                            intake::prompt_tile_bound(prompt_count, tile).map_err(bad)?,
                        ));
                    }
                }
            }
        }
        Role::SemanticTable => {
            let s = sem.ok_or_else(|| bad("family semantic table immutable snapshot absent"))?;
            if s.token_table_root.is_none()
                || u64::from(source.vocab_v1())
                    .checked_add(1)
                    .is_none_or(|n| n > crate::palw_fp_constraint_job_v1::PALW_TOKEN_TABLE_MAX_LEAVES_V1)
            {
                return Err(bad("family semantic table lacks pinned canonical geometry"));
            }
            let table = table_inventory.ok_or_else(|| bad("family table exact authenticated EOF inventory missing"))?;
            if Some(table.root) != s.token_table_root || table.tokenizer != s.tokenizer_id || table.vocab != source.vocab_v1() {
                return Err(bad("family table inventory substitutes immutable semantics"));
            }
            targets.push((FamilyMaterialTargetV1::SemanticTable, table.bytes));
        }
        Role::TokenizerArtifact => {
            let s = sem.ok_or_else(|| bad("family raw tokenizer immutable snapshot absent"))?;
            let eof = state
                .staged_tokenizer_inventory_v1(s.tokenizer_id)
                .ok_or_else(|| bad("family tokenizer lacks authenticated actual EOF"))?;
            eof.check_v1().map_err(bad)?;
            if eof.artifact_hash != s.tokenizer_id
                || eof.completed_daa > frame.plan_v1().terms.accepted_daa
                || s.raw_tokenizer_bytes != Some(eof.actual_bytes)
            {
                return Err(bad("family tokenizer EOF differs from immutable snapshot"));
            }
            // Canonical Vec framing preserves a nonempty wire body even for an
            // authenticated zero-byte (therefore potentially invalid) raw artifact.
            targets.push((FamilyMaterialTargetV1::TokenizerRaw, add(eof.actual_bytes, 4)?));
        }
        Role::PrefixGeneratedSelectionAndStop => {
            for ordinal in 0..set.len() {
                let request = set.request_at(ordinal).map_err(bad)?;
                if matches!(request, Query::Ids { .. }) || (semantic_v3 && !generated_semantic_query_v2(&request)) {
                    continue;
                }
                targets.push((
                    FamilyMaterialTargetV1::Input(MaterialTargetV1::Unit { ordinal }),
                    crate::palw_tir_public_owned_v1::request_body_bound_v1(source, request).map_err(bad)?.0,
                ));
            }
            targets.push((
                FamilyMaterialTargetV1::Input(MaterialTargetV1::FullIds),
                crate::palw_tir_public_owned_v1::request_body_bound_v1(source, Query::Ids { offset: 0 }).map_err(bad)?.0,
            ));
        }
    }
    if semantic_v3 && targets.len() as u64 != count {
        return Err(bad("generated semantic count/enumeration mismatch"));
    }
    targets.sort_unstable_by_key(|x| x.0);
    if targets.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(bad("family native target repeats"));
    }
    let maximum_bytes = targets.iter().try_fold(0u64, |n, (_, bytes)| add(n, *bytes))?;
    let semantic_id = sem.map(|s| s.commitment());
    let id = if semantic_v3 {
        digest(
            b"misaka-palw/family-material/generated-semantic-inventory/v3",
            &(3u16, claim, source.source_id_v1(), graph.scope_v3(), role, semantic_id, &targets),
        )
    } else {
        // Literal historical V1 tuple/domain and complete enumeration.
        digest(
            b"misaka-palw/family-material/inventory/v1",
            &(claim, source.source_id_v1(), graph.scope_v3(), role, semantic_id, &targets),
        )
    }
    .map_err(bad)?;
    let old_inputs = family_inventory_source_inputs_v1(state, frame, profile, semantics, role, table_inventory)?;
    let source_inputs = if semantic_v3 {
        digest(b"misaka-palw/family-material/generated-semantic-inputs/v3", &(3u16, old_inputs)).map_err(bad)?
    } else {
        old_inputs
    };
    Ok(Held::from_owner_reserved_v1(
        NativeFamilyInventoryV1 {
            claim,
            source: source.source_id_v1(),
            relative_graph: graph.scope_v3(),
            role,
            id,
            targets,
            maximum_bytes,
            source_inputs,
            native_inclusion_margin_daa_v2: frame.plan_v1().acquisition.config.prices.inclusion_margin_daa,
            generated_semantic_only_v3: semantic_v3,
            carrier_projection_v3: None,
        },
        lease,
    ))
}
/// Reconstruct a current accepted inventory from native state, never a supplier DTO.
pub(crate) fn derive_current_family_inventory_classified_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    role: Role,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    derive_current_family_inventory_mode_v3(state, claim, role, false)
}
/// A retained contract chooses its exact signed historical derivation. Today's
/// Active state, new defaults or a role-only lookup cannot reinterpret its ID.
pub(crate) fn derive_contract_family_inventory_classified_v3(
    state: &PalwChainStateV2,
    contract: &crate::palw_court_family_supplier_v1::FamilySupplierRowV1,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    let t = &contract.consent.terms;
    let generation = crate::palw_court_family_supplier_v1::signed_profile_generation_v3(t).map_err(bad)?;
    let mode = generation == 3;
    let inventory = derive_current_family_inventory_mode_v3(state, t.claim, t.role, mode)?;
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, t.claim)
        .ok_or_else(|| bad("bound family source original absent"))?;
    if frame.plan_v1().allocation_id != contract.allocation
        || frame.plan_v1().terms.accepted_daa != contract.accepted_daa
        || inventory.source_v1() != t.source
        || inventory.relative_graph_v1() != t.relative_graph
    {
        return Err(bad("family contract changes retained original source scope"));
    }
    if generation == 3 {
        let id = contract.consent.id_v1().map_err(bad)?;
        let key = crate::palw_court_family_supplier_v1::family_carrier_projection_key_v3(id);
        let raw = state
            .kernel_route_state_v1()
            .and_then(|route| {
                route.aux.iter().find_map(|((table, k), v)| {
                    (*table == crate::palw_court_family_supplier_v1::FAMILY_SUPPLIER_TABLE_V1 && k.as_slice() == key).then_some(v)
                })
            })
            .ok_or_else(|| bad("profile3 carrier birth facts absent"))?;
        let row: crate::palw_court_family_supplier_v1::FamilyCarrierProjectionRowV3 =
            borsh::from_slice(raw).map_err(|e| bad(e.to_string()))?;
        if row.version != 3
            || row.contract != id
            || row.claim != t.claim
            || row.accepted_daa != contract.accepted_daa
            || row.source != t.source
            || row.allocation != contract.allocation
            || row.accepted_block == Hash64::default()
            || row.funding_contract == Hash64::default()
        {
            return Err(bad("profile3 carrier birth facts scope changed"));
        }
        state.check_family_carrier_birth_v3(contract, &row).map_err(|e| match e {
            crate::palw_state_v2::PalwStateV2Error::NativeResource(c) => E::Resource(c),
            other => bad(other.to_string()),
        })?;
        if let Some(escrow) = state.funded_escrows_v1().get(&row.funding_contract)
            && (escrow.claim != t.claim || escrow.accepted_daa != row.accepted_daa)
        {
            return Err(bad("carrier facts differ from native funded birth"));
        }
        let inventory = bind_family_carrier_facts_v3(inventory, row.facts)?;
        if inventory.id_v1() != t.inventory {
            return Err(bad("profile3 carrier facts change signed inventory"));
        }
        Ok(inventory)
    } else {
        Ok(inventory)
    }
}
fn derive_current_family_inventory_mode_v3(
    state: &PalwChainStateV2,
    claim: Hash64,
    role: Role,
    semantic_v3: bool,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .ok_or_else(|| bad("family material original missing"))?;
    let c = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(bad)?;
    let class = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class).ok_or_else(|| bad("family material class missing"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class)
        .ok_or_else(|| bad("family material layout missing"))?;
    let source = frame.source_identity_native_v2(record, layout, frame.descriptor_v1().maximum_leaves)?;
    if source.source_id_v1() != c.source_id {
        return Err(bad("family current context substitutes native source"));
    }
    let cover = Source::root_input_cover_native_v1(
        &source,
        record,
        layout,
        intake::material_intake_schedule_v1().map_err(|_| bad("family material adopted cover budget missing"))?.cover,
    )?;
    let program = crate::palw_native_resource_v1::decode_native_tir_program_v1(&record.program).map_err(|e| match e {
        E::Resource(r) => E::Resource(r),
        E::Decode(e) => bad(e.to_string()),
    })?;
    let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, c.profile_id).map_err(bad)?;
    let up = set.signed_data_transport_upper().map_err(bad)?;
    let edges = up
        .fresh_responses
        .checked_mul(up.maximum_custodians.checked_add(1).ok_or_else(|| bad("family role count overflow"))?)
        .and_then(|n| up.streams.checked_mul(2).and_then(|s| n.checked_add(s)))
        .and_then(|n| n.checked_add(up.parts))
        .ok_or_else(|| bad("family source edge count overflow"))?;
    let graph = crate::palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3::derive_native_v3(
        frame,
        &source,
        c.profile_id,
        crate::palw_court_source_material_graph_v3::SourceGraphBudgetV3 {
            max_queries: set.len(),
            max_roles: up.maximum_custodians,
            max_vertices: up.signed_controls,
            max_edges: edges,
        },
    )?;
    let semantics = state.claim_semantics_v1(&claim);
    let table = if role == Role::SemanticTable {
        semantics.map(|sem| read_family_table_inventory_v1(state, claim, sem, source.vocab_v1())).transpose()?.flatten()
    } else {
        None
    };
    derive_family_inventory_with_table_mode_v3(
        state,
        frame,
        &source,
        &graph,
        c.profile_id,
        &program,
        &cover,
        semantics,
        role,
        table.as_ref(),
        semantic_v3,
    )
}
/// Rooted consumers select the unique contract of the current native birth.
/// Old standalone/legacy material keeps the literal V1 derivation when no
/// family contract owns that role. A later raw-ID birth is never an alias.
pub(crate) fn derive_bound_current_family_inventory_classified_v3(
    state: &PalwChainStateV2,
    claim: Hash64,
    role: Role,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
        .ok_or_else(|| bad("current bound family original absent"))?;
    let accepted = frame.plan_v1().terms.accepted_daa;
    let rows = state.family_supplier_rows_v1().map_err(|e| match e {
        crate::palw_state_v2::PalwStateV2Error::NativeResource(c) => E::Resource(c),
        other => bad(other.to_string()),
    })?;
    let mut matches = rows
        .iter()
        .filter(|(_, row)| row.consent.terms.claim == claim && row.consent.terms.role == role && row.accepted_daa == accepted);
    let Some((_, contract)) = matches.next() else { return derive_current_family_inventory_classified_v1(state, claim, role) };
    if matches.next().is_some() {
        return Err(bad("current family role has ambiguous signed birth contracts"));
    }
    derive_contract_family_inventory_classified_v3(state, contract)
}
pub(crate) fn derive_current_family_inventory_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    role: Role,
) -> Result<Held<NativeFamilyInventoryV1>, String> {
    derive_current_family_inventory_classified_v1(state, claim, role).map_err(|e| e.to_string())
}
/// Optional all-ID result may populate the existing authenticated shared-ID row; every other
/// result is data identity alone. No user-controlled completion status enters this type.
pub(crate) enum AuthenticatedFamilyMaterialV1 {
    Body,
    Ids { owner: Held<crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1> },
}
/// Authenticate a bounded canonical token-table file without unbounded Borsh Vec decoding.
/// All renderings and the EOG inventory must reach the immutable semantic root.
fn authenticate_table_v1(bytes: &[u8], vocabulary: u32, table_root: Hash64, tokenizer: Hash64) -> Result<(), E<String>> {
    use crate::palw_fp_constraint_job_v1 as table;
    if bytes.len() > table::PALW_TOKEN_TABLE_MAX_FILE_BYTES_V1
        || vocabulary == 0
        || u64::from(vocabulary) + 1 > table::PALW_TOKEN_TABLE_MAX_LEAVES_V1
    {
        return Err(bad("family table bound/geometry"));
    }
    let mut cursor = bytes.strip_prefix(&table::PALW_TOKEN_TABLE_FILE_MAGIC_V1[..]).ok_or_else(|| bad("family token table magic"))?;
    let count = u32::deserialize_reader(&mut cursor).map_err(|e| bad(e.to_string()))?;
    if count != vocabulary {
        return Err(bad("family token table cardinality changes original"));
    }
    let mut memory = TirNativeRequestsV1::new();
    memory
        .exact_v1::<Hash64>((u64::from(count) + 1).checked_mul(4).ok_or_else(|| bad("family table root memory overflow"))?)
        .map_err(E::Resource)?;
    let mut lease = crate::palw_native_resource_v1::reserve_native_requests_v1(memory.bytes_v1()).map_err(E::Resource)?;
    let mut leaves = Vec::new();
    leaves.try_reserve_exact(count as usize + 1).map_err(local_allocation)?;
    for id in 0..count {
        let present = u8::deserialize_reader(&mut cursor).map_err(|e| bad(e.to_string()))?;
        let rendering = match present {
            0 => None,
            1 => {
                let n = u32::deserialize_reader(&mut cursor).map_err(|e| bad(e.to_string()))? as usize;
                let value = cursor.get(..n).ok_or_else(|| bad("family rendering truncation"))?;
                cursor = &cursor[n..];
                Some(value)
            }
            _ => return Err(bad("family rendering option tag")),
        };
        leaves.push(table::table_leaf_hash_v1(id, rendering));
    }
    let eog_count = u32::deserialize_reader(&mut cursor).map_err(|e| bad(e.to_string()))? as usize;
    if eog_count == 0 || eog_count > vocabulary as usize || eog_count > cursor.len() / 4 {
        return Err(bad("family EOG inventory truncation"));
    }
    if let Some(lease) = lease.as_mut() {
        lease
            .grow_memory_v1((eog_count as u64).checked_mul(8).ok_or_else(|| bad("family EOG memory overflow"))?)
            .map_err(E::Resource)?;
    }
    let mut eog = Vec::new();
    eog.try_reserve_exact(eog_count).map_err(local_allocation)?;
    for _ in 0..eog_count {
        let id = u32::deserialize_reader(&mut cursor).map_err(|e| bad(e.to_string()))?;
        if id >= vocabulary {
            return Err(bad("family EOG id outside original vocabulary"));
        }
        eog.push(id);
    }
    let raw_tokenizer = Hash64::deserialize_reader(&mut cursor).map_err(|e| bad(e.to_string()))?;
    if raw_tokenizer != tokenizer || !cursor.is_empty() {
        return Err(bad("family table tokenizer/trailing data"));
    }
    eog.sort_unstable();
    if eog.windows(2).any(|ids| ids[0] == ids[1]) {
        return Err(bad("family EOG inventory contains duplicate ids"));
    }
    leaves.push(table::table_eog_leaf_hash_v1(&eog));
    if crate::palw_step_leg::step_merkle_root_capped_v1(&leaves, table::PALW_TOKEN_TABLE_MAX_LEAVES_V1)
        .map_err(|e| bad(e.to_string()))?
        != table_root
    {
        return Err(bad("family token table does not reach immutable semantic root"));
    }
    Ok(())
}
pub(crate) fn authenticate_family_material_classified_v1(
    state: &PalwChainStateV2,
    inventory: &NativeFamilyInventoryV1,
    target: FamilyMaterialTargetV1,
    bytes: &[u8],
) -> Result<AuthenticatedFamilyMaterialV1, E<String>> {
    let maximum = inventory.target_bound_v1(target).map_err(bad)?;
    if bytes.is_empty() || bytes.len() as u64 > maximum {
        return Err(bad("family body exceeds native target bound"));
    }
    let c = crate::palw_court_shared_tir_state_v1::context(state, inventory.claim).map_err(bad)?;
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, inventory.claim)
        .ok_or_else(|| bad("family authentication original missing"))?;
    let class = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class).ok_or_else(|| bad("family authentication class missing"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, class)
        .ok_or_else(|| bad("family authentication layout missing"))?;
    let source = frame.source_identity_native_v2(record, layout, frame.descriptor_v1().maximum_leaves)?;
    if source.source_id_v1() != c.source_id {
        return Err(bad("family authentication context changes source"));
    }
    let semantics = state.claim_semantics_v1(&inventory.claim);
    let table = if inventory.role == Role::SemanticTable {
        semantics.map(|sem| read_family_table_inventory_v1(state, inventory.claim, sem, source.vocab_v1())).transpose()?.flatten()
    } else {
        None
    };
    let old_inputs = family_inventory_source_inputs_v1(state, frame, c.profile_id, semantics, inventory.role, table.as_ref())?;
    let current_inputs = if inventory.generated_semantic_only_v3 {
        digest(b"misaka-palw/family-material/generated-semantic-inputs/v3", &(3u16, old_inputs)).map_err(bad)?
    } else {
        old_inputs
    };
    let current_inputs = if let Some(facts) = inventory.carrier_projection_v3 {
        digest(b"misaka-palw/family-material/carrier-bound-inputs/v3", &(3u16, current_inputs, facts)).map_err(bad)?
    } else {
        current_inputs
    };
    if current_inputs != inventory.source_inputs {
        return Err(bad("family body belongs to stale or substituted native inventory"));
    }
    match target {
        FamilyMaterialTargetV1::Input(MaterialTargetV1::FullIds) => {
            use crate::palw_tir_public_source_v1::{PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1, PalwTirPublicResponseV1 as Response};
            let (_, ram) = crate::palw_tir_public_owned_v1::request_body_bound_v1(&source, Query::Ids { offset: 0 }).map_err(bad)?;
            // The owned-ID quote includes all chunk bytes and complete-vector RAM.
            let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(ram).map_err(E::Resource)?;
            let mut ids = Vec::new();
            ids.try_reserve_exact(source.decode_count_v1() as usize).map_err(local_allocation)?;
            let mut root = None;
            let mut at = 0usize;
            for offset in (0..source.decode_count_v1()).step_by(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
                let request = Query::Ids { offset };
                let width = source.unit_bound_v1(&request).map_err(bad)?.maximum_response_bytes as usize;
                let end = at.checked_add(width).filter(|n| *n <= bytes.len()).ok_or_else(|| bad("family complete IDs truncated"))?;
                let response = source.decode_response_v1(&request, &bytes[at..end]).map_err(bad)?;
                let Response::Ids { rows_root, ids: chunk } = response else { return Err(bad("family complete IDs response type")) };
                if root.is_some_and(|old| old != rows_root) || ids.len() != offset as usize {
                    return Err(bad("family complete IDs changes root/offset"));
                }
                root = Some(rows_root);
                ids.extend_from_slice(&chunk);
                at = end;
            }
            if at != bytes.len() {
                return Err(bad("family complete IDs trailing bytes"));
            }
            let owner =
                source.retain_complete_ids_classified_v2(root.ok_or_else(|| bad("family complete IDs has no actual EOF"))?, &ids)?;
            Ok(AuthenticatedFamilyMaterialV1::Ids { owner })
        }
        FamilyMaterialTargetV1::Input(target) => {
            let _scratch = crate::palw_native_resource_v1::reserve_native_requests_v1(
                maximum.checked_mul(2).ok_or_else(|| bad("family material hash scratch overflow"))?,
            )
            .map_err(E::Resource)?;
            match target {
                MaterialTargetV1::Parameter { leaf } => {
                    if bytes.len() as u64 != maximum {
                        return Err(bad("family parameter exact native width"));
                    }
                    let program =
                        crate::palw_native_resource_v1::decode_native_tir_program_v1(&record.program).map_err(|e| match e {
                            E::Resource(r) => E::Resource(r),
                            E::Decode(e) => bad(e.to_string()),
                        })?;
                    let q = crate::palw_tir_court_v1::TirInventoryOpeningVerifierV1::index_requests(&program).map_err(E::Resource)?;
                    let _index = crate::palw_native_resource_v1::reserve_native_requests_v1(q).map_err(E::Resource)?;
                    let verifier =
                        crate::palw_tir_court_v1::TirInventoryOpeningVerifierV1::new(&program, source.header_v1().artifact_root)
                            .ok_or_else(|| bad("family parameter inventory absent"))?;
                    let opening = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<
                        crate::palw_artifact::PalwArtifactOpeningV1,
                        std::io::Error,
                    >(bytes, || borsh::from_slice(bytes))
                    .map_err(|e| match e {
                        E::Resource(r) => E::Resource(r),
                        E::Decode(e) => bad(e.to_string()),
                    })?;
                    if opening.leaf_index != leaf {
                        return Err(bad("family parameter changes requested coordinate"));
                    }
                    verifier.check(&opening).map_err(|e| bad(e.to_string()))?;
                }
                MaterialTargetV1::PromptFlat => {
                    if frame.plan_v1().owner_work.rules.prompt_form != crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
                        || bytes.len() as u64 != maximum
                    {
                        return Err(bad("family prompt form/width"));
                    }
                    let ids = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<Vec<u32>, std::io::Error>(
                        bytes,
                        || borsh::from_slice(bytes),
                    )
                    .map_err(|e| match e {
                        E::Resource(r) => E::Resource(r),
                        E::Decode(e) => bad(e.to_string()),
                    })?;
                    let context = &source.header_v1().job_context;
                    if ids.len() != context.declared_prefill_tokens as usize
                        || !crate::palw_prompt_ids_v1::prompt_token_ids_match_v1(
                            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                            &ids,
                            &context.prompt_token_ids_hash,
                        )
                    {
                        return Err(bad("family prompt changes original count/commitment"));
                    }
                }
                MaterialTargetV1::PromptTile { tile } => {
                    if frame.plan_v1().owner_work.rules.prompt_form != crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 {
                        return Err(bad("family prompt tile wrong native form"));
                    }
                    let opening = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<
                        crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1,
                        std::io::Error,
                    >(bytes, || borsh::from_slice(bytes))
                    .map_err(|e| match e {
                        E::Resource(r) => E::Resource(r),
                        E::Decode(e) => bad(e.to_string()),
                    })?;
                    if opening.tile_index != tile {
                        return Err(bad("family prompt tile changes native coordinate"));
                    }
                    let context = &source.header_v1().job_context;
                    crate::palw_prompt_ids_v1::verify_prompt_ids_opening_v1(
                        &context.prompt_token_ids_hash,
                        context.declared_prefill_tokens,
                        &opening,
                    )
                    .map_err(|e| bad(e.to_string()))?;
                }
                MaterialTargetV1::Unit { ordinal } => {
                    let set =
                        crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, c.profile_id).map_err(bad)?;
                    let query = set.request_at(ordinal).map_err(bad)?;
                    if matches!(query, Query::Ids { .. }) {
                        return Err(bad("family source IDs require complete EOF target"));
                    }
                    let bound = source.unit_bound_v1(&query).map_err(bad)?;
                    let _response =
                        crate::palw_native_resource_v1::reserve_native_requests_v1(bound.resident_ram_bytes).map_err(E::Resource)?;
                    let response = source.decode_response_v1(&query, bytes).map_err(bad)?;
                    let rows = if matches!(query, Query::RowsNode { .. } | Query::TileNode { .. } | Query::TraceTile { .. }) {
                        use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
                        let Some(Row::CourtSharedIds(ids)) = state.staged_court_row_v1(&Key::CourtSharedIds(inventory.claim)) else {
                            return Err(bad("family source row/tile requires actual authenticated complete IDs"));
                        };
                        ids.check(&Key::CourtSharedIds(inventory.claim)).map_err(bad)?;
                        if ids.scope_id != c.scope_id {
                            return Err(bad("family source IDs from foreign scope"));
                        }
                        source.validate_complete_ids_classified_v2(ids.rows_root, &ids.ids)?;
                        Some(crate::palw_tir_public_source_v1::PalwTirPublicRowsAnchorV1::from_native_v1(&source, ids.rows_root))
                    } else {
                        None
                    };
                    source.authenticate_with_rows_v1(&query, &response, None, rows.as_ref()).map_err(bad)?;
                }
                MaterialTargetV1::FullIds => return Err(bad("family full IDs dispatch mismatch")),
            }
            Ok(AuthenticatedFamilyMaterialV1::Body)
        }
        FamilyMaterialTargetV1::SemanticTable => {
            let sem = state.claim_semantics_v1(&inventory.claim).ok_or_else(|| bad("family semantic snapshot absent"))?;
            authenticate_table_v1(
                bytes,
                source.vocab_v1(),
                sem.token_table_root.ok_or_else(|| bad("family semantic root absent"))?,
                sem.tokenizer_id,
            )?;
            Ok(AuthenticatedFamilyMaterialV1::Body)
        }
        FamilyMaterialTargetV1::TokenizerRaw => {
            let sem = state.claim_semantics_v1(&inventory.claim).ok_or_else(|| bad("family tokenizer snapshot absent"))?;
            let eof = state
                .staged_tokenizer_inventory_v1(sem.tokenizer_id)
                .ok_or_else(|| bad("family tokenizer authenticated EOF absent"))?;
            eof.check_v1().map_err(bad)?;
            let raw = family_tokenizer_raw_payload_v1(bytes, eof.actual_bytes).map_err(bad)?;
            if sem.raw_tokenizer_bytes != Some(eof.actual_bytes)
                || crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(raw) != sem.tokenizer_id
            {
                return Err(bad("family raw bytes differ from authenticated actual EOF/hash"));
            }
            Ok(AuthenticatedFamilyMaterialV1::Body)
        }
    }
}

pub(crate) fn authenticate_family_material_v1(
    state: &PalwChainStateV2,
    inventory: &NativeFamilyInventoryV1,
    target: FamilyMaterialTargetV1,
    bytes: &[u8],
) -> Result<AuthenticatedFamilyMaterialV1, String> {
    authenticate_family_material_classified_v1(state, inventory, target, bytes).map_err(|e| e.to_string())
}

pub const FAMILY_MATERIAL_TABLE_V1: u8 = 169;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilyMaterialDraftV1 {
    pub contract: Hash64,
    pub target: FamilyMaterialTargetV1,
    pub submitter: PalwBondKeyV2,
    pub total_bytes: u64,
    pub body_hash: Hash64,
    pub offset: u64,
    pub expires_daa: u64,
    pub material_until_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilyMaterialPartV1 {
    pub draft: Hash64,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub expires_daa: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilyMaterialBodyV1 {
    pub contract: Hash64,
    pub target: FamilyMaterialTargetV1,
    pub inventory: Hash64,
    pub bytes: Vec<u8>,
    pub digest: Hash64,
    pub completed_by: PalwBondKeyV2,
    pub accepted_daa: u64,
    pub material_until_daa: u64,
}
pub fn family_material_draft_id_v1(
    contract: Hash64,
    target: FamilyMaterialTargetV1,
    submitter: PalwBondKeyV2,
    total: u64,
    body_hash: Hash64,
) -> Result<Hash64, String> {
    digest(b"misaka-palw/family-material/draft/v1", &(contract, target, submitter, total, body_hash))
}
pub(crate) fn material_aux_key_v1(kind: u8, id: Hash64, offset: Option<u64>) -> Vec<u8> {
    let mut key = Vec::with_capacity(if offset.is_some() { 73 } else { 65 });
    key.push(kind);
    key.extend_from_slice(id.as_byte_slice());
    if let Some(offset) = offset {
        key.extend_from_slice(&offset.to_le_bytes());
    }
    key
}
impl crate::palw_native_resource_v1::NativeHeapV1 for FamilyMaterialTargetV1 {
    fn native_heap_v1(&self, _: &str) -> C {
        C::zero()
    }
}
impl crate::palw_native_resource_v1::NativeHeapV1 for FamilyMaterialActionV1 {
    fn native_heap_v1(&self, path: &str) -> C {
        match self {
            Self::Part { bytes, .. } => bytes.native_heap_v1(path),
            _ => C::zero(),
        }
    }
}
crate::palw_native_resource_v1::native_struct!(SignedFamilyMaterialV1;version,contract,target,submitter,action,signature);
crate::palw_native_resource_v1::native_struct!(FamilyTableInventoryRowV1;claim,carrier,allocation_id,allocation_rules_v2,bytes,material_until_daa);
crate::palw_native_resource_v1::native_struct!(FamilyMaterialDraftV1;contract,target,submitter,total_bytes,body_hash,offset,expires_daa,material_until_daa);
crate::palw_native_resource_v1::native_struct!(FamilyMaterialPartV1;draft,offset,bytes,expires_daa);
crate::palw_native_resource_v1::native_struct!(FamilyMaterialBodyV1;contract,target,inventory,bytes,digest,completed_by,accepted_daa,material_until_daa);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyTableInventoryRowV1;Hash64,crate::tx::TransactionOutpoint,Hash64,Option<FamilyOriginalRuleContextV2>,Vec<u8>,u64);
impl crate::palw_native_resource_v1::NativeWireScanV1 for FamilyMaterialTargetV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        use crate::palw_native_resource_v1::NativeWireShapeV1;
        r.step()?;
        match r.byte()? {
            0 => MaterialTargetV1::scan_native_wire_v1(r),
            1 | 2 => Ok(NativeWireShapeV1::default()),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "family material target")),
        }
    }
}
impl crate::palw_native_resource_v1::NativeWireScanV1 for FamilyMaterialActionV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        use crate::palw_native_resource_v1::NativeWireShapeV1;
        r.step()?;
        let mut s = NativeWireShapeV1::default();
        match r.byte()? {
            0 => {
                s = s.merge(u64::scan_native_wire_v1(r)?)?;
                s = s.merge(Hash64::scan_native_wire_v1(r)?)?;
            }
            1 => {
                s = s.merge(Hash64::scan_native_wire_v1(r)?)?;
                s = s.merge(u64::scan_native_wire_v1(r)?)?;
                s = s.merge(Vec::<u8>::scan_native_wire_v1(r)?)?;
            }
            2 => s = s.merge(Hash64::scan_native_wire_v1(r)?)?,
            3 => {
                s = s.merge(u64::scan_native_wire_v1(r)?)?;
                s = s.merge(Hash64::scan_native_wire_v1(r)?)?;
                s = s.merge(FamilyDraftQuoteV2::scan_native_wire_v1(r)?)?;
            }
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "family material action")),
        }
        Ok(s)
    }
}
crate::palw_native_resource_v1::wire::wire_struct!(SignedFamilyMaterialV1;u16,Hash64,FamilyMaterialTargetV1,PalwBondKeyV2,FamilyMaterialActionV1,Vec<u8>);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyMaterialDraftV1;Hash64,FamilyMaterialTargetV1,PalwBondKeyV2,u64,Hash64,u64,u64,u64);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyMaterialPartV1;Hash64,u64,Vec<u8>,u64);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyMaterialBodyV1;Hash64,FamilyMaterialTargetV1,Hash64,Vec<u8>,Hash64,PalwBondKeyV2,u64,u64);

pub fn sign_family_material_v1(
    mut control: SignedFamilyMaterialV1,
    network: Hash64,
    mut sign: impl FnMut(&PalwBondKeyV2, &[u8], &[u8]) -> Option<Vec<u8>>,
) -> Option<SignedFamilyMaterialV1> {
    let message = control.signing_message_v1(network).ok()?;
    control.signature = sign(&control.submitter, message.as_byte_slice(), control.signature_context_v2())
        .filter(|s| s.len() == crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)?;
    Some(control)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn family_material_table_authenticates_all_renderings_eog_and_tokenizer() {
        let raw = b"literal tokenizer bytes, parser validity is not asserted";
        let tokenizer = crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(raw);
        let table = crate::palw_fp_constraint_job_v1::PalwTokenTableV1 {
            entries: vec![Some(b"a".to_vec()), None, Some(vec![])],
            eog_token_ids: vec![2],
            tokenizer_id: tokenizer,
        };
        let body = crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&table);
        assert!(authenticate_table_v1(&body, 3, table.root(), tokenizer).is_ok());
        assert!(authenticate_table_v1(&body, 2, table.root(), tokenizer).is_err());
        assert!(authenticate_table_v1(&body, 3, Hash64::default(), tokenizer).is_err());
        assert!(authenticate_table_v1(&body, 3, table.root(), Hash64::default()).is_err());
        let mut altered = table.clone();
        altered.entries[0] = Some(b"b".to_vec());
        assert!(
            authenticate_table_v1(
                &crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&altered),
                3,
                table.root(),
                tokenizer
            )
            .is_err()
        );
        altered = table.clone();
        altered.eog_token_ids = vec![3];
        assert!(
            authenticate_table_v1(
                &crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&altered),
                3,
                table.root(),
                tokenizer
            )
            .is_err()
        );
        altered = table.clone();
        altered.eog_token_ids = vec![2, 2];
        assert!(
            authenticate_table_v1(
                &crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&altered),
                3,
                altered.root(),
                tokenizer,
            )
            .is_err(),
            "even a matching root cannot authenticate duplicate EOG ids"
        );
        altered.eog_token_ids = vec![2, 0];
        assert!(
            authenticate_table_v1(
                &crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&altered),
                3,
                altered.root(),
                tokenizer,
            )
            .is_ok(),
            "distinct EOG ids retain the existing order-independent root"
        );
        altered.eog_token_ids = vec![0, 1, 2, 2];
        assert!(
            authenticate_table_v1(
                &crate::palw_fp_constraint_job_v1::palw_token_table_file_encode_v1(&altered),
                3,
                altered.root(),
                tokenizer,
            )
            .is_err(),
            "EOG storage cannot exceed the authenticated vocabulary envelope"
        );
        let mut bomb = body.clone();
        bomb[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(authenticate_table_v1(&bomb, 3, table.root(), tokenizer).is_err());
        let mut trailing = body.clone();
        trailing.push(0);
        assert!(authenticate_table_v1(&trailing, 3, table.root(), tokenizer).is_err());
    }
    #[test]
    fn family_material_raw_eof_frame_preserves_empty_and_malformed_literal_artifacts() {
        assert_eq!(family_tokenizer_raw_payload_v1(&[0, 0, 0, 0], 0).unwrap(), b"");
        let raw = b"not valid tokenizer JSON".to_vec();
        let body = borsh::to_vec(&raw).unwrap();
        assert_eq!(family_tokenizer_raw_payload_v1(&body, raw.len() as u64).unwrap(), raw);
        assert!(family_tokenizer_raw_payload_v1(&body, raw.len() as u64 + 1).is_err());
        let mut changed = body.clone();
        changed.push(0);
        assert!(family_tokenizer_raw_payload_v1(&changed, raw.len() as u64).is_err());
    }
    #[test]
    fn family_material_part_decoder_bounds_before_allocating() {
        let action =
            FamilyMaterialActionV1::Part { draft: Hash64::from_u64_word(1), offset: 0, bytes: vec![0; FAMILY_MATERIAL_PART_BYTES_V1] };
        assert_eq!(borsh::from_slice::<FamilyMaterialActionV1>(&borsh::to_vec(&action).unwrap()).unwrap(), action);
        let too_large = FamilyMaterialActionV1::Part {
            draft: Hash64::from_u64_word(1),
            offset: 0,
            bytes: vec![0; FAMILY_MATERIAL_PART_BYTES_V1 + 1],
        };
        assert!(borsh::from_slice::<FamilyMaterialActionV1>(&borsh::to_vec(&too_large).unwrap()).is_err());
    }
    #[test]
    fn family_material_signatures_bind_exact_contract_target_and_literal_body() {
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([125; 32]);
        let network = Hash64::from_u64_word(3);
        let control = SignedFamilyMaterialV1 {
            version: 1,
            contract: Hash64::from_u64_word(4),
            target: FamilyMaterialTargetV1::SemanticTable,
            submitter: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(5), 0)),
            action: FamilyMaterialActionV1::Begin { total_bytes: 100, body_hash: Hash64::from_u64_word(6) },
            signature: vec![],
        };
        let signed = sign_family_material_v1(control, network, |_, message, context| {
            Some(libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [126; 32]).unwrap().as_ref().to_vec())
        })
        .unwrap();
        assert!(signed.verify_v1(network, key.verification_key.as_ref()).is_ok());
        let mut changed = signed.clone();
        changed.target = FamilyMaterialTargetV1::TokenizerRaw;
        assert!(changed.verify_v1(network, key.verification_key.as_ref()).is_err());
        changed = signed.clone();
        changed.contract = Hash64::from_u64_word(7);
        assert!(changed.verify_v1(network, key.verification_key.as_ref()).is_err());
        changed = signed.clone();
        changed.action = FamilyMaterialActionV1::Begin { total_bytes: 101, body_hash: Hash64::from_u64_word(6) };
        assert!(changed.verify_v1(network, key.verification_key.as_ref()).is_err());
        assert!(signed.verify_v1(Hash64::from_u64_word(8), key.verification_key.as_ref()).is_err());
    }
}

/// Borrowed native consumers acquire a retained literal body and reauthenticate it against
/// the current immutable original. A completion receipt or an off-chain cache is insufficient.
pub(crate) fn read_authenticated_family_material_classified_v1(
    state: &PalwChainStateV2,
    inventory: &NativeFamilyInventoryV1,
    target: FamilyMaterialTargetV1,
) -> Result<Option<Held<FamilyMaterialBodyV1>>, E<String>> {
    let target_id = inventory.target_id_v1(target).map_err(bad)?;
    let key = material_aux_key_v1(3, target_id, None);
    let Some(bytes) = state.kernel_route_state_v1().and_then(|r| r.aux.get(&(FAMILY_MATERIAL_TABLE_V1, key))) else { return Ok(None) };
    let body = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(bytes, || {
        borsh::from_slice::<FamilyMaterialBodyV1>(bytes)
    })
    .map_err(|e| match e {
        E::Resource(r) => E::Resource(r),
        E::Decode(e) => bad(e.to_string()),
    })?;
    let now = state.last_point().map_or(0, |p| p.daa_score);
    if body.inventory != inventory.id
        || body.target != target
        || body.accepted_daa > now
        || now > body.material_until_daa
        || crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body.bytes) != body.digest
    {
        return Err(bad("family retained material changes inventory/digest/clock"));
    }
    authenticate_family_material_classified_v1(state, inventory, target, &body.bytes)?;
    Ok(Some(body))
}

pub(crate) fn read_authenticated_family_material_v1(
    state: &PalwChainStateV2,
    inventory: &NativeFamilyInventoryV1,
    target: FamilyMaterialTargetV1,
) -> Result<Option<Held<FamilyMaterialBodyV1>>, String> {
    read_authenticated_family_material_classified_v1(state, inventory, target).map_err(|e| e.to_string())
}

/// The raw payload is a canonical Borsh Vec, inspected without allocation.
/// Its count must equal the independently authenticated EOF, including zero.
pub fn family_tokenizer_raw_payload_v1(body: &[u8], actual_bytes: u64) -> Result<&[u8], String> {
    let count = body.get(..4).ok_or("family tokenizer raw frame truncated")?;
    let declared = u32::from_le_bytes(count.try_into().map_err(|_| "family tokenizer raw frame")?) as u64;
    if declared != actual_bytes || body.len() as u64 != actual_bytes.checked_add(4).ok_or("family tokenizer raw EOF overflow")? {
        return Err("family tokenizer raw frame differs from actual EOF".into());
    }
    Ok(&body[4..])
}

/// Exact independently priced upload consent. The book's historical scalar
/// geometry is authenticated at birth and retained so restore does not require
/// today's expanded service book or today's free principal.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilyDraftQuoteV2 {
    pub version: u16,
    pub accept_from_daa: u64,
    pub accept_until_daa: u64,
    pub respond_until_daa: u64,
    pub profile: Hash64,
    pub book: Hash64,
    pub book_bytes: u64,
    pub book_heap: u64,
    pub fee_sompi: u64,
    pub collateral_sompi: u64,
    pub reserved_memory: u64,
    pub retained_bytes: u64,
    pub journal_bytes: u64,
    pub prior_rows: u64,
    pub prior_key_bytes: u64,
    pub prior_events: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct FamilyDraftReservationV2 {
    pub contract: Hash64,
    pub version: u16,
    pub claim: Hash64,
    pub draft: Hash64,
    pub accepted_daa: u64,
    pub expires_daa: u64,
    pub material_until_daa: u64,
    pub quote: FamilyDraftQuoteV2,
    pub transfer_first_daa: u64,
    pub transfer_span_daa: u64,
    pub begin: SignedFamilyMaterialV1,
}
crate::palw_native_resource_v1::native_struct!(FamilyDraftQuoteV2;version,accept_from_daa,accept_until_daa,respond_until_daa,profile,book,book_bytes,book_heap,fee_sompi,collateral_sompi,reserved_memory,retained_bytes,journal_bytes,prior_rows,prior_key_bytes,prior_events);
crate::palw_native_resource_v1::native_struct!(FamilyDraftReservationV2;contract,version,claim,draft,accepted_daa,expires_daa,material_until_daa,quote,transfer_first_daa,transfer_span_daa,begin);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyDraftQuoteV2;u16,u64,u64,u64,Hash64,Hash64,u64,u64,u64,u64,u64,u64,u64,u64,u64,u64);
impl crate::palw_native_resource_v1::NativeWireScanV1 for FamilyDraftReservationV2 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        use std::io::Read;
        r.step()?;
        r.skip(64 + 2 + 64 + 64 + 3 * 8)?;
        // The quote's reserved_memory is a stack-read predecode request. No raw
        // row length or peer-estimated multiplication substitutes for this schema.
        r.skip(2 + 3 * 8 + 2 * 64 + 4 * 8)?;
        let mut word = [0; 8];
        r.read_exact(&mut word)?;
        let held = u64::from_le_bytes(word);
        r.skip(7 * 8)?;
        let mut shape = SignedFamilyMaterialV1::scan_native_wire_v1(r)?;
        shape.heap = shape.heap.checked_add(held).ok_or_else(|| std::io::Error::other("draft predecode capacity overflow"))?;
        Ok(shape)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct FamilyPrimaryDraftV2 {
    pub contract: Hash64,
    pub version: u16,
    pub claim: Hash64,
    pub target: Hash64,
    pub draft: Hash64,
    pub material_until_daa: u64,
}
crate::palw_native_resource_v1::native_struct!(FamilyPrimaryDraftV2;contract,version,claim,target,draft,material_until_daa);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyPrimaryDraftV2;Hash64,u16,Hash64,Hash64,Hash64,u64);

/// New birth identity includes the exact independently signed quote. Retrying
/// the same bytes after expiry at a new quoted point cannot collide with a
/// historical receipt. Primary and legacy V1 identities remain literal.
pub fn family_material_draft_id_v2(
    contract: Hash64,
    target: FamilyMaterialTargetV1,
    submitter: PalwBondKeyV2,
    total: u64,
    body_hash: Hash64,
    quote: &FamilyDraftQuoteV2,
) -> Result<Hash64, String> {
    digest(b"misaka-palw/family-material/draft/v2", &(contract, target, submitter, total, body_hash, quote))
}

/// Fixed retained facts of actual carrier coordinates. Reading this wire value
/// alone grants no native quote capability; ingress derives Native below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct FamilyCarrierProjectionFactsV3 {
    pub input_count: u64,
    pub distinct_txids: u64,
    pub prefix_use: Option<Hash64>,
    pub input_bound: u64,
    pub coordinates: Hash64,
}
impl FamilyCarrierProjectionFactsV3 {
    pub(crate) fn check_v3(&self) -> Result<(), String> {
        if self.input_count == 0
            || self.input_count > u64::from(u32::MAX)
            || self.distinct_txids == 0
            || self.distinct_txids > self.input_count
            || self.coordinates == Hash64::default()
            || self.prefix_use == Some(Hash64::default())
            || self.input_bound
                != self.distinct_txids.checked_add(u64::from(self.prefix_use.is_some())).ok_or("carrier input bound overflow")?
        {
            return Err("carrier projection facts change native shape".into());
        }
        Ok(())
    }
}
crate::palw_native_resource_v1::native_struct!(FamilyCarrierProjectionFactsV3;input_count,distinct_txids,prefix_use,input_bound,coordinates);
crate::palw_native_resource_v1::wire::wire_struct!(FamilyCarrierProjectionFactsV3;u64,u64,Option<Hash64>,u64,Hash64);
/// No decoder/producer scalar constructor: only actual native transaction input
/// coordinates plus an authenticated same-carrier PrefixUse can create this.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeFamilyCarrierInputsV3 {
    facts: FamilyCarrierProjectionFactsV3,
}
struct CarrierCoordinatesV3<'a>(&'a [crate::tx::TransactionInput]);
impl BorshSerialize for CarrierCoordinatesV3<'_> {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
        u32::try_from(self.0.len())
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "carrier input count"))?
            .serialize(w)?;
        for input in self.0 {
            input.previous_outpoint.serialize(w)?;
        }
        Ok(())
    }
}
impl NativeFamilyCarrierInputsV3 {
    pub(crate) fn derive_v3(
        inputs: &[crate::tx::TransactionInput],
        prefix: Option<&crate::palw_claim_local_v1::prefix_v1::PalwSignedPrefixUseV1>,
        claim: Hash64,
        network: Hash64,
        executor: &[u8],
    ) -> Result<Self, E<String>> {
        if inputs.is_empty() {
            return Err(bad("profile3 carrier must have an actual ordinary input"));
        }
        let n = u64::from(u32::try_from(inputs.len()).map_err(|_| bad("carrier input count exceeds native wire"))?);
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, ()>>(n).map_err(E::Resource)?;
        let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        let mut ids = std::collections::BTreeSet::new();
        for input in inputs {
            ids.insert(input.previous_outpoint.transaction_id);
        }
        let prefix_use = if let Some(signed) = prefix {
            if signed.intent.base_commitment != claim || signed.intent.network != network {
                return Err(bad("carrier PrefixUse differs from exact original"));
            }
            signed.intent.verify_signature_v1(executor, &signed.signature).map_err(|e| bad(e.to_string()))?;
            Some(signed.intent.id_v1())
        } else {
            None
        };
        let distinct_txids = ids.len() as u64;
        let facts = FamilyCarrierProjectionFactsV3 {
            input_count: n,
            distinct_txids,
            prefix_use,
            input_bound: add(distinct_txids, u64::from(prefix_use.is_some()))?,
            coordinates: digest(
                b"misaka-palw/family-carrier-coordinates/v3",
                &(3u16, claim, network, CarrierCoordinatesV3(inputs), prefix_use),
            )
            .map_err(bad)?,
        };
        facts.check_v3().map_err(bad)?;
        Ok(Self { facts })
    }
    pub(crate) fn facts_v3(&self) -> FamilyCarrierProjectionFactsV3 {
        self.facts
    }
}
pub(crate) fn bind_family_carrier_inputs_v3(
    inventory: Held<NativeFamilyInventoryV1>,
    carrier: &NativeFamilyCarrierInputsV3,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    bind_family_carrier_facts_v3(inventory, carrier.facts)
}
fn bind_family_carrier_facts_v3(
    inventory: Held<NativeFamilyInventoryV1>,
    facts: FamilyCarrierProjectionFactsV3,
) -> Result<Held<NativeFamilyInventoryV1>, E<String>> {
    facts.check_v3().map_err(bad)?;
    let (mut inventory, lease) = inventory.into_parts_v1();
    if inventory.carrier_projection_v3.is_some() {
        return Err(bad("family carrier facts already bound"));
    }
    inventory.id = digest(b"misaka-palw/family-material/carrier-bound-inventory/v3", &(3u16, inventory.id, facts)).map_err(bad)?;
    inventory.source_inputs =
        digest(b"misaka-palw/family-material/carrier-bound-inputs/v3", &(3u16, inventory.source_inputs, facts)).map_err(bad)?;
    inventory.carrier_projection_v3 = Some(facts);
    Ok(Held::from_owner_reserved_v1(inventory, lease))
}

#[cfg(test)]
mod carrier_projection_v3_tests {
    use super::*;
    #[test]
    fn family_profile3_carrier_facts_use_actual_distinct_inputs_and_survive_signing() {
        use crate::tx::{TransactionInput, TransactionOutpoint};
        let h = Hash64::from_u64_word;
        let mut inputs = vec![
            TransactionInput::new(TransactionOutpoint::new(h(1), 0), vec![], 0, 1),
            TransactionInput::new(TransactionOutpoint::new(h(1), 1), vec![], 0, 1),
            TransactionInput::new(TransactionOutpoint::new(h(2), 0), vec![], 0, 1),
        ];
        let before = NativeFamilyCarrierInputsV3::derive_v3(&inputs, None, h(3), h(4), &[]).unwrap().facts_v3();
        assert_eq!((before.input_count, before.distinct_txids, before.input_bound), (3, 2, 2));
        inputs[0].signature_script = vec![7; 33];
        assert_eq!(
            NativeFamilyCarrierInputsV3::derive_v3(&inputs, None, h(3), h(4), &[]).unwrap().facts_v3(),
            before,
            "final input signatures do not require an unknown transaction identity in the quote"
        );
        inputs[0].previous_outpoint.index = 2;
        let changed = NativeFamilyCarrierInputsV3::derive_v3(&inputs, None, h(3), h(4), &[]).unwrap().facts_v3();
        assert_ne!(before.coordinates, changed.coordinates);
        assert_eq!(before.input_bound, changed.input_bound);
        assert!(NativeFamilyCarrierInputsV3::derive_v3(&[], None, h(3), h(4), &[]).is_err());
        let mut impossible = before;
        impossible.input_bound = 1;
        assert!(impossible.check_v3().is_err());
    }
}

pub(crate) struct FamilySourceFingerprintInputsV16<'a> {
    pub record: &'a crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    pub layout: &'a crate::palw_tir_class_v1::PalwTirLayoutV1,
    pub semantics: Option<&'a crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    pub eof: Option<&'a crate::palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1>,
    pub cover_limits: (u64, u64, u64, u64),
}
/// Literal legacy digest tuple/order/domain preserved. Source16 can supply its
/// actual admitted signed-source snapshot, separately from B's postblock view.
pub(crate) fn family_inventory_source_inputs_typed_v16(
    frame: &Frame,
    profile: Hash64,
    role: Role,
    table: Option<&CheckedFamilyTableInventoryV1>,
    inputs: &FamilySourceFingerprintInputsV16<'_>,
) -> Result<Hash64, E<String>> {
    let eof = if role == Role::TokenizerArtifact { inputs.eof } else { None };
    let table = if role == Role::SemanticTable { table.map(|t| (t.root, t.tokenizer, t.vocab, t.bytes)) } else { None };
    digest(
        b"misaka-palw/family-material/local-source-inputs/v1",
        &(frame, inputs.record, inputs.layout, inputs.semantics, eof, profile, role, table, inputs.cover_limits),
    )
    .map_err(bad)
}
