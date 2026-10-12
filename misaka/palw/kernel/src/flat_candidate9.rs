//! Non-reward, explicitly versioned mixed-court candidate preparation. Stored
//! separately from execution classes and legacy conformance candidates; the
//! same bonded actor index bounds both preparation catalogs. No Final rights.
use super::*;
use crate::{flat_replay9::FlatReplayEnvelopeV9, flat_scope9::FlatResourceScopeV9};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatCandidatePlanV9 {
    pub version: u16,
    /// Artifact/job role only. The new court set is independently explicit.
    pub descriptor: Digest,
    pub program_root: Digest,
    pub court_set: Digest,
    pub profile: Digest,
    pub max_positions: u32,
    pub encoder: Option<(u16, u32)>,
    pub scope_root: Digest,
    pub replay_root: Digest,
    /// Full 21-axis resource10 envelope at the immutable class maximum.
    pub resource_root: Digest,
    pub scope_metadata_work: u64,
    pub scope_bytes_len: u32,
    /// Bounded native restore tariff: admitted canonical bytes, exact PC,
    /// complete scope recheck and rooted metadata; no tensor execution.
    pub native_restore_work: u64,
}
impl FlatCandidatePlanV9 {
    pub fn root(&self) -> Digest {
        crate::hash::object_id(b"misaka-palw/kernel/flat-candidate-plan/v9", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatCandidateRecordV9 {
    pub binding: Digest,
    pub plan: FlatCandidatePlanV9,
    pub program_bytes: Vec<u8>,
    pub param_commitments: ParamCommitmentsV1,
    /// Native-generated complete metadata, authenticated by plan roots. SDK
    /// checking independently derives both rather than trusting these bytes.
    pub scope_bytes: Vec<u8>,
    pub replay_bytes: Vec<u8>,
}
impl FlatCandidateRecordV9 {
    pub fn id(&self) -> Digest {
        crate::hash::object_id(
            b"misaka-palw/kernel/flat-candidate-class/v9",
            &(self.binding, self.plan.root(), self.param_commitments.root(), VerificationModeV1::OptimisticPublicVerification),
        )
    }
    pub fn checked(&self) -> Result<FlatCandidateViewV9, String> {
        let v =
            derive_flat_candidate_v9(&self.plan.descriptor, &self.program_bytes, &self.param_commitments, self.plan.max_positions)?;
        if v.plan != self.plan
            || borsh::to_vec(&v.scope).map_err(|e| e.to_string())? != self.scope_bytes
            || borsh::to_vec(&v.replay).map_err(|e| e.to_string())? != self.replay_bytes
        {
            return Err("stored flat-v9 plan differs from complete independent derivation".into());
        }
        Ok(v)
    }
}
pub struct FlatCandidateViewV9 {
    pub plan: FlatCandidatePlanV9,
    pub program: TirProgramV1,
    pub encoder: Option<crate::seg_encoder::EncoderBindingV1>,
    pub scope: FlatResourceScopeV9,
    pub replay: FlatReplayEnvelopeV9,
    pub resources: crate::flat_resources10::FlatResourceVectorV10,
}
pub fn derive_flat_candidate_v9(
    descriptor: &Digest,
    program_bytes: &[u8],
    pc: &ParamCommitmentsV1,
    max_positions: u32,
) -> Result<FlatCandidateViewV9, String> {
    let d = [crate::descriptor::k2_tir_v4_descriptor(), crate::descriptor::k2_tir_v5_descriptor()]
        .into_iter()
        .find(|d| d.digest() == *descriptor)
        .ok_or("unknown flat-v9 artifact/job role")?;
    let program = TirProgramV1::decode_canonical(program_bytes).map_err(|e| e.to_string())?;
    derive_decoded_flat_candidate_v9(descriptor, &d, program_bytes, program, pc, max_positions)
}
// Registration reuses its paid canonical decode. The SDK and stored-record
// checker enter through derive_flat_candidate_v9 and derive the same roots.
fn derive_decoded_flat_candidate_v9(
    descriptor: &Digest,
    d: &crate::descriptor::KernelDescriptorV1,
    program_bytes: &[u8],
    program: TirProgramV1,
    pc: &ParamCommitmentsV1,
    max_positions: u32,
) -> Result<FlatCandidateViewV9, String> {
    let encoder = if crate::descriptor::is_encoder_v1(d) { Some(crate::seg_encoder::encoder_binding_v1(&program)?) } else { None };
    let real = encoder.map(|e| e.first_input as usize).unwrap_or(program.params.len());
    // Count before constructing the declared-instance BTreeSet. A tiny carrier
    // cannot make us allocate all instances of a large per-layer namespace.
    let declared_count = program.params[..real]
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { program.schedule.layers.len() } else { 1 }))
        .ok_or("flat-v9 declared parameter-instance count overflow")?;
    if declared_count != pc.by_instance.len() {
        return Err("flat-v9 committed parameter-instance count differs from its declaration".into());
    }
    let encoder = if crate::descriptor::is_encoder_v1(d) {
        let e = encoder.expect("the binding was checked before the instance count");
        crate::seg_encoder::prove_encoder_ranges_v1(&program, &e)?;
        Some(e)
    } else {
        misaka_palw_tir::interval::analyze_ranges(&program).map_err(|e| format!("flat-v9 exact ranges are not proven: {e}"))?;
        None
    };
    let declared = declared_param_instances(&program.params[..real], program.schedule.layers.len());
    check_commitment_set(pc, &declared, &declared)?;
    let resources = crate::flat_resources10::derive_flat_resources_v10(&program, pc, max_positions, encoder.as_ref())?;
    let scope = resources.court;
    let resources = resources.vector;
    let replay =
        crate::flat_replay9::replay_envelope_from_verified_scope_v9(&program, &scope, resources.court_scope, encoder.as_ref())
            .map_err(|e| format!("{e:?}"))?;
    let scope_bytes_len =
        u32::try_from(borsh::to_vec(&scope).map_err(|e| e.to_string())?.len()).map_err(|_| "v9 scope size overflow")?;
    let scope_metadata_work = u64::try_from(scope.metadata_derivation_work).map_err(|_| "v9 scope work overflow")?;
    let native_restore_work = (program_bytes.len() as u64)
        .checked_mul(1024)
        .and_then(|x| x.checked_add((pc.by_instance.len() as u64).checked_mul(69 * 128)?))
        .and_then(|x| x.checked_add(scope_metadata_work))
        .and_then(|x| x.checked_add((scope_bytes_len as u64).checked_mul(64)?))
        .and_then(|x| x.checked_add((scope.relations.len() as u64).checked_mul(4 * 1024)?))
        .and_then(|x| x.checked_add(65536))
        .ok_or("v9 restore work overflow")?;
    let plan = FlatCandidatePlanV9 {
        version: 9,
        descriptor: *descriptor,
        program_root: program_root_v1(program_bytes),
        court_set: scope.court_set,
        profile: scope.profile,
        max_positions,
        encoder: scope.encoder,
        scope_root: resources.court_scope,
        replay_root: replay.root(),
        resource_root: resources.root(),
        scope_metadata_work,
        scope_bytes_len,
        native_restore_work,
    };
    Ok(FlatCandidateViewV9 { plan, program, encoder, scope, replay, resources })
}
/// First debit, before parameter-root hashing or canonical decode. The byte
/// square bounds repeated bounded template parsing/validation, declaration
/// comparisons, encoder lifting, constant range scans and template hashing.
/// Scheduled metadata has a separate exact-count debit after this decode.
/// These are metadata tariffs, not canonical work or reward weight.
pub fn flat_candidate_registration_work_v9(program_bytes: usize, pc_instances: usize) -> Option<u64> {
    let b = u64::try_from(program_bytes).ok()?;
    let pc = u64::try_from(pc_instances).ok()?;
    b.checked_mul(b)?.checked_add(b.checked_mul(64)?)?.checked_add(pc.checked_mul(69 * 128)?)?.checked_add(16384)
}
/// Preliminary wiring/shape quotation tariff. Full geometry and the remaining
/// resource/replay/root work are paid separately before those passes begin.
pub const FLAT_CANDIDATE_METADATA_PASSES_V9: u64 = 4;
/// Second debit is obtained without allocating unrolled wiring or visiting
/// tensor values. Registration calls this only after a canonical typed decode.
pub fn flat_candidate_metadata_work_v9(program: &TirProgramV1) -> Option<u64> {
    if program.blocks.len() > misaka_palw_tir::program::MAX_BLOCKS
        || program.schedule.layers.len() > misaka_palw_tir::program::MAX_LAYERS
        || program.blocks.iter().any(|b| b.nodes.is_empty() || b.nodes.len() > misaka_palw_tir::program::MAX_NODES_PER_BLOCK)
    {
        return None;
    }
    let nodes = |b: u8| u64::try_from(program.blocks.get(b as usize)?.nodes.len()).ok();
    let mut n = nodes(program.schedule.pre)?.checked_add(nodes(program.schedule.post)?)?;
    for block in &program.schedule.layers {
        n = n.checked_add(nodes(*block)?)?;
    }
    n.checked_mul(FLAT_CANDIDATE_METADATA_PASSES_V9)?.checked_mul(1024)
}
/// Native binding checks and the submitted parameter-root hash are paid before
/// the consumer examines them, separately from canonical preparation.
pub fn flat_candidate_binding_work_v9(program_bytes: usize, pc_instances: usize) -> Option<u64> {
    u64::try_from(program_bytes)
        .ok()?
        .checked_mul(64)?
        .checked_add(u64::try_from(pc_instances).ok()?.checked_mul(69 * 128)?)?
        .checked_add(1024)
}
impl FlatCandidateViewV9 {
    pub(crate) fn check_policy(&self, ledger: &KernelLedgerV1) -> Result<(), String> {
        ledger.known_descriptor(&self.plan.descriptor)?;
        ledger.check_court_work_v1(self.scope.worst_court_work)?;
        let ram = ledger.policy.prosecution.max_verifier_ram;
        if [
            self.replay.reference.position_tensor_bytes,
            self.replay.reference.largest_operand_tensor_bytes,
            self.replay.reference.history_rows_tensor_bytes,
            self.replay.topk_auxiliary_bytes,
        ]
        .iter()
        .any(|x| *x > ram)
        {
            return Err("flat-v9 ordinary component memory exceeds consumer policy".into());
        }
        Ok(())
    }
}
impl KernelLedgerV1 {
    pub(super) fn register_flat_candidate_v9(
        &mut self,
        binding: Digest,
        descriptor: Digest,
        program_bytes: &[u8],
        pc: &ParamCommitmentsV1,
        max_positions: u32,
        scope_root: Digest,
        replay_root: Digest,
        resource_root: Digest,
        owner: Digest,
    ) -> Result<Digest, KernelRefusalV1> {
        const NAME: &str = "RegisterFlatModelCandidateV9";
        let rule = |s: String| KernelRefusalV1::rule(NAME, s);
        self.opv_register_gate(NAME)?;
        let d = self.known_descriptor(&descriptor).map_err(rule)?;
        if self.schedule.standing_at(&d.digest(), self.daa) != crate::descriptor::KernelStandingV1::Active {
            return Err(rule("flat-v9 artifact/job role is not active".into()));
        }
        if self.conformance_class_owners.get(&owner).is_some_and(|x| x.len() >= MAX_CONFORMANCE_CLASSES_PER_BOND_V1) {
            return Err(rule("the shared conformance candidate catalog is full".into()));
        }
        if self.native_flat_candidate_ids_v9.len().max(self.flat_conformance_classes_v9.len()) >= FLAT_CANDIDATE_REGISTRY_MAX_V9 {
            return Err(rule("bounded v9 registry is full".into()));
        }
        let work = flat_candidate_registration_work_v9(program_bytes.len(), pc.by_instance.len())
            .ok_or_else(|| rule("flat-v9 admission overflow".into()))?;
        self.check_admission_work_v1(work).map_err(rule)?;
        self.charge(NAME, work)?;
        if !self.attested_artifacts.contains(&pc.root()) {
            return Err(rule("flat-v9 model scope is not authenticated by the consumer".into()));
        }
        let program = TirProgramV1::decode_canonical(program_bytes).map_err(|e| rule(e.to_string()))?;
        let metadata = flat_candidate_metadata_work_v9(&program).ok_or_else(|| rule("flat-v9 scheduled metadata overflow".into()))?;
        let total = work.checked_add(metadata).ok_or_else(|| rule("flat-v9 total admission overflow".into()))?;
        self.check_admission_work_v1(total).map_err(rule)?;
        let charged = self.budget.court_work.checked_add(metadata).filter(|w| *w <= self.policy.admission_work_limit_v1());
        let Some(charged) = charged else {
            return Err(KernelRefusalV1::new(NAME, RefusalKindV1::OverBudget, "the shared admission metadata work budget is spent"));
        };
        // This extends the already paid attempt; it does not buy another
        // adjudication or borrow the work reserved for an ordinary proof.
        self.budget.court_work = charged;
        let encoder = if crate::descriptor::is_encoder_v1(&d) {
            Some(crate::seg_encoder::encoder_binding_v1(&program).map_err(rule)?)
        } else {
            None
        };
        let geometry = crate::flat_scope9::flat_scope_metadata_work_v9(&program, max_positions, encoder.as_ref())
            .map_err(|e| rule(format!("{e:?}")))?;
        // Eight bounded outer passes plus <=256 framing/hash bytes per node
        // cover reference-resource/replay walks and their scope/vector roots.
        // The preliminary four passes were already debited before quotation.
        let nodes = metadata / (FLAT_CANDIDATE_METADATA_PASSES_V9 * 1024);
        let remaining = geometry
            .checked_add(nodes.checked_mul(4 * 1024 + 256 * 64).ok_or_else(|| rule("v9 metadata overflow".into()))?)
            .and_then(|x| x.checked_add(65536))
            .ok_or_else(|| rule("v9 metadata overflow".into()))?;
        let total = total.checked_add(remaining).ok_or_else(|| rule("v9 metadata overflow".into()))?;
        self.check_admission_work_v1(total).map_err(rule)?;
        self.budget.court_work = self
            .budget
            .court_work
            .checked_add(remaining)
            .filter(|x| *x <= self.policy.admission_work_limit_v1())
            .ok_or_else(|| KernelRefusalV1::new(NAME, RefusalKindV1::OverBudget, "shared v9 geometry metadata budget spent"))?;
        let v = derive_decoded_flat_candidate_v9(&descriptor, &d, program_bytes, program, pc, max_positions).map_err(rule)?;
        if scope_root != v.plan.scope_root || replay_root != v.plan.replay_root || resource_root != v.plan.resource_root {
            return Err(rule("flat-v9 declaration differs from whole-graph scope/replay/resource10".into()));
        }
        // Local ordinary-reference component payloads are checked against the
        // consumer's policy. They are not asserted to be total allocator/RSS.
        v.check_policy(self).map_err(rule)?;
        let record = FlatCandidateRecordV9 {
            binding,
            plan: v.plan,
            program_bytes: program_bytes.to_vec(),
            param_commitments: pc.clone(),
            scope_bytes: borsh::to_vec(&v.scope).map_err(|e| rule(e.to_string()))?,
            replay_bytes: borsh::to_vec(&v.replay).map_err(|e| rule(e.to_string()))?,
        };
        let class = record.id();
        if self.native_flat_candidate_ids_v9.contains(&class)
            || self.flat_conformance_classes_v9.contains_key(&class)
            || self.classes.contains_key(&class)
        {
            return Err(rule("flat-v9 candidate already exists".into()));
        }
        self.flat_conformance_classes_v9.insert(class, (owner, record));
        self.native_flat_candidate_ids_v9.insert(class);
        self.conformance_class_owners.entry(owner).or_default().insert(class);
        Ok(class)
    }
}

pub const FLAT_CANDIDATE_METADATA_MAX_BYTES_V9: usize = 8 << 20;
pub struct FlatCandidateNativeViewV9 {
    pub plan: FlatCandidatePlanV9,
    pub program: TirProgramV1,
    pub encoder: Option<crate::seg_encoder::EncoderBindingV1>,
    pub scope: FlatResourceScopeV9,
    pub replay: FlatReplayEnvelopeV9,
}
impl FlatCandidateRecordV9 {
    /// For authenticated, internally admitted rows only. Rederive the complete
    /// scope; the immutable resource vector was independently built at admission
    /// and is never reinterpreted as an execution or reward entitlement here.
    pub fn checked_native(&self) -> Result<FlatCandidateNativeViewV9, String> {
        let program = TirProgramV1::decode_canonical(&self.program_bytes).map_err(|e| e.to_string())?;
        let encoder = if self.plan.descriptor == crate::descriptor::k2_tir_v5_descriptor().digest() {
            Some(crate::seg_encoder::encoder_binding_v1(&program)?)
        } else if self.plan.descriptor == crate::descriptor::k2_tir_v4_descriptor().digest() {
            None
        } else {
            return Err("unknown candidate9 native role".into());
        };
        let scope = decode_scope_v9(&self.scope_bytes)?;
        let replay = decode_replay_v9(&self.replay_bytes)?;
        let scope_root = scope.root();
        if self.plan.version != 9
            || self.program_bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
            || program_root_v1(&self.program_bytes) != self.plan.program_root
            || scope_root != self.plan.scope_root
            || replay.root() != self.plan.replay_root
            || scope.version != 9
            || replay.version != 9
            || replay.scope != scope_root
            || replay.reference.base_scope != scope_root
            || replay.reference.max_positions != self.plan.max_positions
            || scope.profile != self.plan.profile
            || scope.court_set != self.plan.court_set
            || scope.encoder != self.plan.encoder
            || scope.max_positions != self.plan.max_positions
            || self.scope_bytes.len() != self.plan.scope_bytes_len as usize
            || scope.metadata_derivation_work != self.plan.scope_metadata_work as u128
        {
            return Err("rooted native9 metadata differs from admitted plan".into());
        }
        crate::flat_scope9::check_flat_resource_scope_v9(&program, self.plan.max_positions, encoder.as_ref(), &scope)
            .map_err(|e| format!("{e:?}"))?;
        Ok(FlatCandidateNativeViewV9 { plan: self.plan.clone(), program, encoder, scope, replay })
    }
}
fn read<T: BorshDeserialize>(r: &mut std::io::Cursor<&[u8]>) -> Result<T, String> {
    T::deserialize_reader(r).map_err(|e| e.to_string())
}
fn end(r: &std::io::Cursor<&[u8]>) -> Result<(), String> {
    if r.position() as usize == r.get_ref().len() { Ok(()) } else { Err("trailing v9 metadata".into()) }
}
pub fn decode_scope_v9(bytes: &[u8]) -> Result<FlatResourceScopeV9, String> {
    if bytes.len() > FLAT_CANDIDATE_METADATA_MAX_BYTES_V9 {
        return Err("scope9 byte cap".into());
    }
    let mut r = std::io::Cursor::new(bytes);
    let version = read(&mut r)?;
    let profile = read(&mut r)?;
    let court_set = read(&mut r)?;
    let program = read(&mut r)?;
    let max_positions = read(&mut r)?;
    let encoder = read(&mut r)?;
    let count: u32 = read(&mut r)?;
    if count as u128 * 23 > crate::flat_scope9::MAX_SCOPE_RELATION_VISITS_V9
        || count as usize > bytes.len().saturating_sub(r.position() as usize) / 34
    {
        return Err("bounded scope9 occurrence count".into());
    }
    let mut relations = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let occurrence = read(&mut r)?;
        let block = read(&mut r)?;
        let node = read(&mut r)?;
        let ordinary_point = read(&mut r)?;
        let filing_bytes = read(&mut r)?;
        let work = read(&mut r)?;
        let scratch_bytes = read(&mut r)?;
        let n: u32 = read(&mut r)?;
        if n > 9 || n as usize > bytes.len().saturating_sub(r.position() as usize) / 8 {
            return Err("bounded scope9 operand count".into());
        }
        let mut operand_leaves = Vec::with_capacity(n as usize);
        for _ in 0..n {
            operand_leaves.push(read(&mut r)?);
        }
        relations.push(crate::flat_scope9::FlatOccurrenceScopeV9 {
            occurrence,
            block,
            node,
            ordinary_point,
            filing_bytes,
            work,
            scratch_bytes,
            operand_leaves,
        });
    }
    let scope = FlatResourceScopeV9 {
        version,
        profile,
        court_set,
        program,
        max_positions,
        encoder,
        relations,
        metadata_relation_visits: read(&mut r)?,
        metadata_derivation_work: read(&mut r)?,
        worst_court_bytes: read(&mut r)?,
        worst_court_work: read(&mut r)?,
        worst_court_scratch_bytes: read(&mut r)?,
        trace_value_bytes_per_position: read(&mut r)?,
        reference_tensor_bytes_per_position: read(&mut r)?,
        trace_value_bytes_per_claim: read(&mut r)?,
        node_commitments_bytes_per_claim: read(&mut r)?,
    };
    end(&r)?;
    Ok(scope)
}
pub fn decode_replay_v9(bytes: &[u8]) -> Result<FlatReplayEnvelopeV9, String> {
    if bytes.len() > 1024 {
        return Err("replay9 byte cap".into());
    }
    let mut r = std::io::Cursor::new(bytes);
    let version = read(&mut r)?;
    let scope = read(&mut r)?;
    let reference = crate::flat_replay6::FlatReplayEnvelopeV6 {
        version: read(&mut r)?,
        base_scope: read(&mut r)?,
        max_positions: read(&mut r)?,
        position_tensor_bytes: read(&mut r)?,
        largest_operand_tensor_bytes: read(&mut r)?,
        history_rows_tensor_bytes: read(&mut r)?,
        parameter_hash_bytes_per_attempt: read(&mut r)?,
        trace_hash_bytes_per_attempt: read(&mut r)?,
        node_reads_per_attempt: read(&mut r)?,
        hash_reads_per_attempt: read(&mut r)?,
        reply_bytes_per_attempt: read(&mut r)?,
    };
    let out = FlatReplayEnvelopeV9 {
        version,
        scope,
        reference,
        topk_scan_work_per_attempt: read(&mut r)?,
        topk_auxiliary_bytes: read(&mut r)?,
        selected_value_reads_per_attempt: read(&mut r)?,
        selected_reply_bytes_per_attempt: read(&mut r)?,
        header_reads_per_attempt: read(&mut r)?,
        scope_checker_work_per_attempt: read(&mut r)?,
    };
    end(&r)?;
    Ok(out)
}

/// New9 registry and import bounds; these are not ordinary block/court allowances.
pub const FLAT_CANDIDATE_REGISTRY_MAX_V9: usize = 128;
pub const FLAT_CANDIDATE_RECORD_MAX_BYTES_V9: usize = 10 << 20;
pub const FLAT_CANDIDATE_IMPORT_MAX_BYTES_V9: usize = FLAT_CANDIDATE_REGISTRY_MAX_V9 * FLAT_CANDIDATE_RECORD_MAX_BYTES_V9;
/// Fixed-prefix projection of an authenticated native row. Never use this as a
/// checker of remote/unanchored candidate data. No PC/program/scope clone occurs.
pub fn candidate_row_header_v9(row: &[u8]) -> Result<(Digest, Digest, FlatCandidatePlanV9, usize, u32), String> {
    if row.len() > FLAT_CANDIDATE_RECORD_MAX_BYTES_V9 {
        return Err("candidate9 raw row cap".into());
    }
    let mut r = std::io::Cursor::new(row);
    let owner = read(&mut r)?;
    let binding = read(&mut r)?;
    let plan: FlatCandidatePlanV9 = read(&mut r)?;
    let len: u32 = read(&mut r)?;
    let at = r.position() as usize;
    if plan.version != 9 || len as usize > misaka_palw_tir::program::MAX_PROGRAM_BYTES || len as usize > row.len().saturating_sub(at) {
        return Err("candidate9 bounded program prefix".into());
    }
    Ok((owner, binding, plan, at, len))
}
/// Preflight a complete remote/import registry before PC maps or complete scopes
/// are allocated. Stage byte/preparation sums first, then exact geometry visits.
/// Every row must fit the unchanged single-admission ceiling; aggregate import
/// is bounded by128 such attempts and is never a normal-block permission.
pub fn check_candidate_registry_import_v9(rows: &crate::rows::LedgerRowsV1, policy: &LedgerPolicyV1) -> Result<(), String> {
    let cap = (policy.admission_work_limit_v1() as u128)
        .checked_mul(FLAT_CANDIDATE_REGISTRY_MAX_V9 as u128)
        .ok_or("v9 import work cap overflow")?;
    let mut count = 0usize;
    let mut bytes = 0usize;
    let mut charged = 0u128;
    // Raw residency/count validation precedes any program/map allocation.
    for ((table, _), row) in rows {
        if *table == crate::rows::TABLE_FLAT_CONFORMANCE_CLASSES_V9 {
            count += 1;
            bytes = bytes.checked_add(row.len()).ok_or("v9 import bytes overflow")?;
            if count > FLAT_CANDIDATE_REGISTRY_MAX_V9
                || row.len() > FLAT_CANDIDATE_RECORD_MAX_BYTES_V9
                || bytes > FLAT_CANDIDATE_IMPORT_MAX_BYTES_V9
            {
                return Err("v9 import registry count/raw byte cap".into());
            }
        }
    }
    for ((table, _), row) in rows {
        if *table == crate::rows::TABLE_FLAT_CONFORMANCE_CLASSES_V9 {
            let (_, _, plan, at, len) = candidate_row_header_v9(row)?;
            let end = at + len as usize;
            let pc = row.get(end..end + 4).ok_or("v9 import PC count")?;
            let pc = u32::from_le_bytes(pc.try_into().unwrap()) as usize;
            if pc > row.len().saturating_sub(end + 4) / 67 {
                return Err("v9 import PC count exceeds actual bytes".into());
            }
            let initial = flat_candidate_registration_work_v9(len as usize, pc).ok_or("v9 import byte work overflow")?;
            charged = charged.checked_add(initial as u128).ok_or("v9 import work overflow")?;
            if initial > policy.admission_work_limit_v1() || charged > cap {
                return Err("v9 import byte work cap".into());
            }
            let p = TirProgramV1::decode_canonical(&row[at..end]).map_err(|e| e.to_string())?;
            let preliminary = flat_candidate_metadata_work_v9(&p).ok_or("v9 import preliminary overflow")?;
            charged = charged.checked_add(preliminary as u128).ok_or("v9 import work overflow")?;
            if charged > cap {
                return Err("v9 import preliminary cap".into());
            }
            let encoder = if plan.descriptor == crate::descriptor::k2_tir_v5_descriptor().digest() {
                Some(crate::seg_encoder::encoder_binding_v1(&p)?)
            } else if plan.descriptor == crate::descriptor::k2_tir_v4_descriptor().digest() {
                None
            } else {
                return Err("v9 import unknown role".into());
            };
            let geometry = crate::flat_scope9::flat_scope_metadata_work_v9(&p, plan.max_positions, encoder.as_ref())
                .map_err(|e| format!("{e:?}"))?;
            let nodes = preliminary / (FLAT_CANDIDATE_METADATA_PASSES_V9 * 1024);
            let remaining = geometry
                .checked_add(nodes.checked_mul(4 * 1024 + 256 * 64).ok_or("v9 import remaining overflow")?)
                .and_then(|x| x.checked_add(65536))
                .ok_or("v9 import remaining overflow")?;
            let total = initial.checked_add(preliminary).and_then(|x| x.checked_add(remaining)).ok_or("v9 import attempt overflow")?;
            charged = charged.checked_add(remaining as u128).ok_or("v9 import work overflow")?;
            if total > policy.admission_work_limit_v1() || charged > cap {
                return Err("v9 import exact geometry cap".into());
            }
        }
    }
    Ok(())
}

/// Bounded untrusted row decoder. All carrier and namespace counts are checked
/// before nested maps/byte vectors allocate; complete independent checking is
/// still required by from_rows after this decoder returns.
pub fn decode_candidate_row_v9(row: &[u8]) -> Result<(Digest, FlatCandidateRecordV9), String> {
    let (owner, binding, plan, at, len) = candidate_row_header_v9(row)?;
    let mut r = std::io::Cursor::new(row);
    r.set_position((at + len as usize) as u64);
    let program_bytes = row[at..at + len as usize].to_vec();
    let program = TirProgramV1::decode_canonical(&program_bytes).map_err(|e| e.to_string())?;
    let real = if plan.descriptor == crate::descriptor::k2_tir_v5_descriptor().digest() {
        crate::seg_encoder::encoder_binding_v1(&program)?.first_input as usize
    } else if plan.descriptor == crate::descriptor::k2_tir_v4_descriptor().digest() {
        program.params.len()
    } else {
        return Err("candidate9 unknown role".into());
    };
    let declared = program.params[..real]
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { program.schedule.layers.len() } else { 1 }))
        .ok_or("candidate9 namespace overflow")?;
    let count: u32 = read(&mut r)?;
    if count as usize != declared
        || count as usize > 1_583_616 / 67
        || count as usize > row.len().saturating_sub(r.position() as usize) / 67
    {
        return Err("candidate9 parameter count exceeds exact namespace/carrier".into());
    }
    let mut by_instance = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        let key: (u16, Option<u16>) = read(&mut r)?;
        if previous.is_some_and(|k| k >= key) {
            return Err("candidate9 noncanonical parameter map".into());
        }
        let p =
            program.params.get(key.0 as usize).filter(|_| (key.0 as usize) < real).ok_or("candidate9 parameter outside namespace")?;
        if p.per_layer != key.1.is_some() || key.1.is_some_and(|l| l as usize >= program.schedule.layers.len()) {
            return Err("candidate9 parameter layer mismatch".into());
        }
        previous = Some(key);
        by_instance.insert(key, read(&mut r)?);
    }
    fn bytes(r: &mut std::io::Cursor<&[u8]>, max: usize) -> Result<Vec<u8>, String> {
        let n: u32 = read(r)?;
        let at = r.position() as usize;
        if n as usize > max || n as usize > r.get_ref().len().saturating_sub(at) {
            return Err("candidate9 nested byte count exceeds cap/carrier".into());
        }
        let end = at + n as usize;
        let out = r.get_ref()[at..end].to_vec();
        r.set_position(end as u64);
        Ok(out)
    }
    let scope_bytes = bytes(&mut r, FLAT_CANDIDATE_METADATA_MAX_BYTES_V9)?;
    let replay_bytes = bytes(&mut r, 1024)?;
    end(&r)?;
    Ok((
        owner,
        FlatCandidateRecordV9 {
            binding,
            plan,
            program_bytes,
            param_commitments: ParamCommitmentsV1 { by_instance },
            scope_bytes,
            replay_bytes,
        },
    ))
}

impl KernelLedgerV1 {
    /// Operational projection of a locally admitted/journaled candidate only.
    /// This creates no class row or execution rights, and cannot check imported
    /// remote data. The native caller authenticates the original row history.
    pub fn note_native_candidate_projection_v9(&mut self, owner: Digest, class: Digest) -> Result<(), String> {
        if !self.bonds.contains_key(&owner) || self.native_flat_candidate_ids_v9.len() >= FLAT_CANDIDATE_REGISTRY_MAX_V9 {
            return Err("native9 projection owner/global bound".into());
        }
        let owned = self.conformance_class_owners.entry(owner).or_default();
        if owned.len() >= MAX_CONFORMANCE_CLASSES_PER_BOND_V1
            || !owned.insert(class)
            || !self.native_flat_candidate_ids_v9.insert(class)
        {
            return Err("native9 projection shared catalog/duplicate".into());
        }
        Ok(())
    }
    pub fn native_candidate_count_v9(&self) -> usize {
        self.native_flat_candidate_ids_v9.len()
    }
    pub fn native_candidate_owned_v9(&self, owner: &Digest, class: &Digest) -> bool {
        self.conformance_class_owners.get(owner).is_some_and(|ids| ids.contains(class))
    }
}
