//! Producer-authorized prospective material, before a consent exists. A local
//! candidate is created only by the actual kernel validation path. It is never
//! installed into native state, paid, or represented as an accepted claim.
use crate::{
    Hash64,
    palw_kernel_parameter_material_v4::{
        PALW_KERNEL_MATERIAL_UNIT_BYTES_V4, PALW_KERNEL_PARAMETER_STAGE_V4, PALW_KERNEL_SEGMENTED_PART_STAGE_V4,
        PalwKernelParameterRowV4, PalwKernelSegmentedPartV4, classify_kernel_parameter_v4, classify_kernel_segmented_v4,
        kernel_parameter_unit_count_v4, kernel_segmented_unit_count_v4,
    },
    palw_kernel_route_v1::palw_kernel_bond_id_v1,
    palw_material_capacity_v1::{
        PALW_MATERIAL_MAX_UNITS_V1, PalwKernelMaterialBundleV1, PalwMaterialCapacityErrorV1, PalwMaterialCodecV1,
        PalwMaterialManifestV1, PalwMaterialScopeV1, PalwSegmentedMaterialBundleV1,
    },
    palw_native_resource_v1::{NativeRetainedV1, ReservedKernelLedgerV1},
    palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    hash::Digest,
    ledger::{ClaimBodyV1, ClaimRowV1, KernelLedgerV1},
    route::SaltedCommitV1,
    spec::{
        SpecClaimV1, SpecClassKindV1,
        composite::{ComponentV1, StageClaimV1},
        retrieval::{RetrievalRootV1, RetrievedV1},
    },
};
/// Local provider ingress freshness only. It does not change any consensus
/// Final, consumer lifetime, material hold or monetary release clock.
pub const PALW_KERNEL_INGRESS_CHECKPOINT_WINDOW_V4: u64 = 600;
pub const PALW_PROSPECTIVE_KERNEL_MATERIAL_CONTEXT_V4: &[u8] = b"misaka-palw/prospective-kernel-material/v4";
pub(crate) fn object_hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let length = borsh::object_length(value).expect("in-memory canonical Borsh length") as u64;
    let mut hash = misaka_palw_kernel::hash::keyed(domain);
    hash.update(&length.to_le_bytes());
    let mut writer = Writer(hash);
    value.serialize(&mut writer).expect("in-memory hash writer");
    Hash64::from_bytes(misaka_palw_kernel::hash::finish(writer.0))
}
fn typed_row<'a>(ledger: &'a KernelLedgerV1, claim: &Digest) -> Result<&'a ClaimRowV1, String> {
    ledger.claims.get(claim).ok_or("complete kernel material claim absent".into())
}
fn retrieval_scope<'a>(
    ledger: &'a KernelLedgerV1,
    row: &'a ClaimRowV1,
    component: u8,
) -> Result<(&'a RetrievalRootV1, &'a [RetrievedV1]), String> {
    let ClaimBodyV1::Spec(body) = &row.body else {
        return Err("typed material body absent".into());
    };
    let class = ledger.typed.classes.get(&row.class_binding_id).ok_or("typed material class absent")?;
    match (&class.kind, &body.claim) {
        (SpecClassKindV1::Retrieval { root }, SpecClaimV1::Retrieval(claim)) if component == 0 => Ok((root, &claim.result)),
        (SpecClassKindV1::Composite { components, .. }, SpecClaimV1::Composite(claim)) => {
            match (components.get(component as usize), claim.stages.get(component as usize)) {
                (Some(ComponentV1::Retrieval(root)), Some(StageClaimV1::Retrieval { result, .. })) => Ok((root, result)),
                _ => Err("typed material component is not retrieval".into()),
            }
        }
        _ => Err("typed retrieval class/claim mismatch".into()),
    }
}
/// Enumerate bounded *ranges*, not an allocated vector of every unit. Original
/// committed nodes, memory entry state, every snapshot slice and every returned
/// entry are all required. The namespaces are the native typed court's own.
pub fn typed_kernel_unit_ranges_v4(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    mut range: impl FnMut(u8, u32) -> Result<(), String>,
) -> Result<u64, String> {
    let row = typed_row(ledger, claim)?;
    let mut total = 0u64;
    let mut add = |stage: u8, count: u64| -> Result<(), String> {
        total = total.checked_add(count).ok_or("typed material unit count overflow")?;
        if total > PALW_MATERIAL_MAX_UNITS_V1 as u64 {
            return Err("typed material unit cap exceeded".into());
        }
        if count > 0 {
            range(stage, u32::try_from(count).map_err(|_| "typed unit count exceeds u32")?)?;
        }
        Ok(())
    };
    match &row.body {
        ClaimBodyV1::Program { commitments, .. } => add(0, commitments.len() as u64)?,
        ClaimBodyV1::Pipeline { stages, .. } => {
            for (stage, body) in stages.iter().enumerate() {
                let stage = u8::try_from(stage).map_err(|_| "pipeline stage namespace overflow")?;
                if stage >= misaka_palw_kernel::spec::MEMORY_PRE_STATE_STAGE_V1 {
                    return Err("pipeline stage collides with reserved complete-unit namespaces".into());
                }
                add(stage, body.commitments.len() as u64)?;
            }
        }
        ClaimBodyV1::Segmented { .. } => add(
            PALW_KERNEL_SEGMENTED_PART_STAGE_V4,
            kernel_segmented_unit_count_v4(ledger, claim, PALW_MATERIAL_MAX_UNITS_V1 as u64)?,
        )?,
        ClaimBodyV1::Spec(body) => {
            let class = ledger.typed.classes.get(&row.class_binding_id).ok_or("typed material class absent")?;
            match (&class.kind, &body.claim) {
                (SpecClassKindV1::Memory { .. }, SpecClaimV1::Memory(claim)) => {
                    let positions = claim
                        .steps
                        .iter()
                        .try_fold(0u64, |n, step| n.checked_add(step.commitments.len() as u64))
                        .ok_or("memory position count overflow")?;
                    add(0, positions)?;
                    add(misaka_palw_kernel::spec::MEMORY_PRE_STATE_STAGE_V1, 1)?;
                }
                (SpecClassKindV1::Retrieval { root }, SpecClaimV1::Retrieval(claim)) => {
                    add(misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1, root.snapshot.slices())?;
                    add(misaka_palw_kernel::spec::RETRIEVAL_ENTRY_STAGE_BASE_V1, claim.result.len() as u64)?;
                }
                (SpecClassKindV1::Composite { components, .. }, SpecClaimV1::Composite(claim)) => {
                    if components.len() != claim.stages.len() {
                        return Err("typed composite stage count mismatch".into());
                    }
                    for (index, (component, stage)) in components.iter().zip(&claim.stages).enumerate() {
                        let index = u8::try_from(index).map_err(|_| "typed component index overflow")?;
                        match (component, stage) {
                            (ComponentV1::Model(_), StageClaimV1::Model { commitments, .. }) => add(index, commitments.len() as u64)?,
                            (ComponentV1::Retrieval(root), StageClaimV1::Retrieval { result, .. }) => {
                                add(
                                    misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1
                                        .checked_add(index)
                                        .ok_or("snapshot namespace overflow")?,
                                    root.snapshot.slices(),
                                )?;
                                add(
                                    misaka_palw_kernel::spec::RETRIEVAL_ENTRY_STAGE_BASE_V1
                                        .checked_add(index)
                                        .ok_or("entry namespace overflow")?,
                                    result.len() as u64,
                                )?;
                            }
                            _ => return Err("typed component kind substituted".into()),
                        }
                    }
                }
                _ => return Err("typed material class/body mismatch".into()),
            }
        }
    }
    add(
        PALW_KERNEL_PARAMETER_STAGE_V4,
        kernel_parameter_unit_count_v4(ledger, claim, PALW_MATERIAL_MAX_UNITS_V1 as u64, PALW_KERNEL_MATERIAL_UNIT_BYTES_V4)?,
    )?;
    Ok(total)
}
pub fn typed_kernel_required_unit_v4(ledger: &KernelLedgerV1, claim: &Digest, stage: u8, position: u32) -> Result<bool, String> {
    let mut found = false;
    typed_kernel_unit_ranges_v4(ledger, claim, |s, count| {
        if s == stage && position < count {
            found = true;
        }
        Ok(())
    })?;
    Ok(found)
}
pub fn typed_kernel_classify_unit_v4(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    stage: u8,
    position: u32,
    bytes: &[u8],
) -> Result<(), String> {
    if !typed_kernel_required_unit_v4(ledger, claim, stage, position)? {
        return Err("unit is outside the exact typed inventory".into());
    }
    let row = typed_row(ledger, claim)?;
    if bytes.len() as u64 > PALW_KERNEL_MATERIAL_UNIT_BYTES_V4 {
        return Err("complete kernel unit exceeds the versioned carrier cap".into());
    }
    if stage == PALW_KERNEL_PARAMETER_STAGE_V4 {
        let unit: PalwKernelParameterRowV4 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
        return classify_kernel_parameter_v4(ledger, claim, position, &unit);
    }
    if stage == PALW_KERNEL_SEGMENTED_PART_STAGE_V4 {
        let unit: PalwKernelSegmentedPartV4 = borsh::from_slice(bytes).map_err(|e| e.to_string())?;
        return classify_kernel_segmented_v4(ledger, claim, position, &unit);
    }
    let bound = ledger.bounds_of(&row.class_binding_id).ok_or("typed response bound absent")?;
    if bytes.len() as u128 > bound.max_response_bytes {
        return Err("typed response exceeds registered court bound".into());
    }
    if row.body.position(stage, position).is_some() {
        return ledger.classify_served_position_v1(claim, stage, position, bytes).map(|_| ()).map_err(str::to_string);
    }
    let entry = misaka_palw_kernel::spec::RETRIEVAL_ENTRY_STAGE_BASE_V1;
    let snapshot = misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1;
    if stage >= entry {
        let (root, result) = retrieval_scope(ledger, row, stage - entry)?;
        let result = result.get(position as usize).ok_or("typed retrieval entry absent")?;
        misaka_palw_kernel::spec::retrieval::classify_entry_response_v1(root, result, bytes).map(|_| ()).map_err(str::to_string)
    } else if stage >= snapshot {
        let (root, _) = retrieval_scope(ledger, row, stage - snapshot)?;
        misaka_palw_kernel::spec::retrieval::classify_slice_response_v1(root, position as u64, bytes)
            .map(|_| ())
            .map_err(str::to_string)
    } else {
        Err("typed unit has no native classifier".into())
    }
}
/// New signed codec: legacy KernelPositions does not silently start accepting
/// snapshot or entry units. Canonical order also removes the old seen-set clone.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTypedKernelMaterialBundleV4 {
    pub claim: Hash64,
    pub units: Vec<(u8, u32, Vec<u8>)>,
}
impl PalwTypedKernelMaterialBundleV4 {
    pub fn verify(&self, ledger: &KernelLedgerV1, max_bytes: u64) -> Result<PalwMaterialScopeV1, PalwMaterialCapacityErrorV1> {
        use PalwMaterialCapacityErrorV1 as E;
        let bytes = borsh::object_length(self).map_err(|_| E::Capacity)? as u64;
        if bytes > max_bytes || self.units.len() > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        let claim = self.claim.as_bytes();
        let row = typed_row(ledger, &claim).map_err(E::Authentication)?;
        let required = typed_kernel_unit_ranges_v4(ledger, &claim, |_, _| Ok(())).map_err(E::Authentication)?;
        if required == 0 || required != self.units.len() as u64 {
            return Err(E::Incomplete);
        }
        if self.units.windows(2).any(|p| (p[0].0, p[0].1) >= (p[1].0, p[1].1)) {
            return Err(E::Substitution);
        }
        for (stage, position, data) in &self.units {
            typed_kernel_classify_unit_v4(ledger, &claim, *stage, *position, data).map_err(E::Authentication)?;
        }
        Ok(PalwMaterialScopeV1 {
            claim: self.claim,
            commitment: crate::palw_material_capacity_v1::kernel_material_commitment_v1(row),
            codec: PalwMaterialCodecV1::TypedKernelV4,
            bytes,
            units: self.units.len() as u32,
            content: object_hash(b"MISAKA/PALW/MONETARY-TYPED-KERNEL-BUNDLE/V4", self),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwProspectiveKernelBundleV4 {
    KernelPositions(PalwKernelMaterialBundleV1),
    SegmentedParts(PalwSegmentedMaterialBundleV1),
    TypedKernelV4(PalwTypedKernelMaterialBundleV4),
}
impl PalwProspectiveKernelBundleV4 {
    pub fn claim(&self) -> Hash64 {
        match self {
            Self::KernelPositions(b) => b.claim,
            Self::SegmentedParts(b) => b.claim,
            Self::TypedKernelV4(b) => b.claim,
        }
    }
    pub fn verify(&self, ledger: &KernelLedgerV1, max_bytes: u64) -> Result<PalwMaterialScopeV1, PalwMaterialCapacityErrorV1> {
        match self {
            Self::KernelPositions(b) => b.verify(ledger, max_bytes),
            Self::SegmentedParts(b) => b.verify(ledger, max_bytes),
            Self::TypedKernelV4(b) => b.verify(ledger, max_bytes),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwProspectiveKernelMaterialIntentV4 {
    pub version: u16,
    pub checkpoint: Hash64,
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
    pub commit_daa: u64,
    pub expires_daa: u64,
    pub producer: PalwBondKeyV2,
    pub providers: Vec<PalwBondKeyV2>,
    pub commit: SaltedCommitV1,
    pub salt: Digest,
    pub manifest: PalwMaterialManifestV1,
}
impl PalwProspectiveKernelMaterialIntentV4 {
    pub fn signing_message(&self, network: Hash64) -> Hash64 {
        object_hash(b"MISAKA/PALW/PROSPECTIVE-KERNEL-MATERIAL/V4", &(network, self))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwProspectiveKernelMaterialRequestV4 {
    pub intent: PalwProspectiveKernelMaterialIntentV4,
    pub producer_signature: Vec<u8>,
    pub bundle: PalwProspectiveKernelBundleV4,
}
impl PalwProspectiveKernelMaterialRequestV4 {
    pub fn id(&self) -> Hash64 {
        object_hash(b"MISAKA/PALW/PROSPECTIVE-KERNEL-REQUEST/V4", self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwProspectiveKernelMaterialAuthorityV4 {
    pub root: Hash64,
    pub class: Hash64,
    pub job: Hash64,
    pub producer: PalwBondKeyV2,
    pub commitment_root: Hash64,
    pub checkpoint: Hash64,
    pub prior_ledger_root: Hash64,
    pub prior_aux_root: Hash64,
    pub commit_daa: u64,
}
pub struct PalwProspectiveKernelMaterialOwnedV4 {
    candidate: ReservedKernelLedgerV1,
    authority: PalwProspectiveKernelMaterialAuthorityV4,
    manifest: NativeRetainedV1<PalwMaterialManifestV1>,
    request: Hash64,
    producer_pubkey: NativeRetainedV1<Vec<u8>>,
    provider_pubkeys: [Option<(PalwBondKeyV2, NativeRetainedV1<Vec<u8>>)>;
        crate::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1],
}
impl PalwProspectiveKernelMaterialOwnedV4 {
    pub fn producer_pubkey(&self) -> &[u8] {
        &self.producer_pubkey
    }
    pub fn provider_pubkey(&self, provider: &PalwBondKeyV2) -> Option<&[u8]> {
        self.provider_pubkeys.iter().flatten().find(|(key, _)| key == provider).map(|(_, key)| key.as_slice())
    }
    pub fn candidate_ledger(&self) -> &KernelLedgerV1 {
        &self.candidate
    }
    pub fn authority(&self) -> &PalwProspectiveKernelMaterialAuthorityV4 {
        &self.authority
    }
    pub fn manifest(&self) -> &PalwMaterialManifestV1 {
        &self.manifest
    }
    pub fn request_id(&self) -> Hash64 {
        self.request
    }
}
pub fn commit_identity_v4(commit: &SaltedCommitV1) -> (Digest, Digest, Digest) {
    match commit {
        SaltedCommitV1::Claim { claim, .. } | SaltedCommitV1::Segmented { claim, .. } => {
            (claim.id(), claim.job_id, claim.producer_bond)
        }
        SaltedCommitV1::Pipeline { claim, .. } => (claim.id(), claim.job_id, claim.producer_bond),
        SaltedCommitV1::Spec { claim } => (claim.id(), claim.job_id(), claim.producer()),
    }
}
/// Unsigned SDK construction rehearsal, intentionally not provider authority.
/// This runs real mathematical admission and preserves its ownership ticket;
/// only the signed request verifier below can issue a provider preflight result.
pub fn prepare_unsigned_kernel_candidate_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    producer: &PalwBondKeyV2,
    commit: &SaltedCommitV1,
    salt: &Digest,
    commit_daa: u64,
    raw_depth: Option<u64>,
) -> Result<ReservedKernelLedgerV1, String> {
    let route = state.kernel_route().ok_or("native kernel route absent")?;
    let (root, job, actor) = commit_identity_v4(commit);
    if actor != palw_kernel_bond_id_v1(producer) {
        return Err("prospective producer differs from actual work".into());
    }
    let bond = state.bond(producer).ok_or("native producer bond absent")?;
    if bond.status != PalwBondStatusV2::Active {
        return Err("prospective producer is not Active".into());
    }
    let mut candidate = route.ledger()?;
    if candidate.claims.contains_key(&root) {
        return Err("prospective work already accepted".into());
    }
    if route.paid_claim_job_is_enrolled_v3(&job)? && candidate.beacon_sources.values().any(|s| s.job == job && s.outcome.is_none()) {
        return Err("paid checker work cannot adopt an independent source opt-in".into());
    }
    let kernel = route.ledger_reserved_of(producer) as u128;
    let committed = crate::palw_state_v2::palw_bond_committed_raw_v1(state, params, producer, commit_daa, raw_depth);
    let non_kernel = committed.saturating_sub(kernel);
    let collateral = (bond.collateral as u128).saturating_sub(non_kernel).max(kernel).min(bond.collateral as u128) as u64;
    candidate.try_sync_bond_v1(actor, collateral).map_err(|e| e.to_string())?;
    candidate.try_replay_salted_claim_v4(commit, salt, actor, commit_daa)?;
    if !candidate.claims.contains_key(&root) {
        return Err("kernel replay did not admit the actual claim".into());
    }
    Ok(candidate)
}
/// The caller supplies its own pinned native checkpoint/state and real ML-DSA
/// verifier. No wire field can supply a ledger, collateral, key or acceptance.
pub fn prospective_kernel_material_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    checkpoint: Hash64,
    now_daa: u64,
    raw_depth: Option<u64>,
    request: &PalwProspectiveKernelMaterialRequestV4,
    max_bytes: u64,
    verify_mldsa: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<PalwProspectiveKernelMaterialOwnedV4, String> {
    let intent = &request.intent;
    let route = state.kernel_route().ok_or("native kernel route absent")?;
    if intent.version != 4
        || intent.checkpoint != checkpoint
        || intent.ledger_root != route.ledger_root()
        || intent.aux_root != route.aux_root()
        || now_daa > intent.expires_daa
        || intent.commit_daa < now_daa
        || intent.commit_daa > intent.expires_daa
        || intent.providers.len() < 2
        || intent.providers.len() > crate::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1
        || intent.providers.windows(2).any(|p| p[0] >= p[1])
        || request.producer_signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
    {
        return Err("prospective material checkpoint/lifetime/roster/framing mismatch".into());
    }
    let producer = state.bond(&intent.producer).ok_or("producer native bond absent")?;
    if producer.status != PalwBondStatusV2::Active {
        return Err("producer native bond inactive".into());
    }
    for (index, key) in intent.providers.iter().enumerate() {
        let provider = state.bond(key).ok_or("provider native bond absent")?;
        if provider.status != PalwBondStatusV2::Active
            || provider.operator_id == producer.operator_id
            || intent.providers[..index]
                .iter()
                .any(|other| state.bond(other).is_some_and(|bond| bond.operator_id == provider.operator_id))
        {
            return Err("provider roster is inactive or shares a native operator with the producer or another provider".into());
        }
    }
    let network = Hash64::from_bytes(route.header.policy.network_domain);
    let message = intent.signing_message(network);
    if !verify_mldsa(
        &producer.pubkey,
        message.as_byte_slice(),
        &request.producer_signature,
        PALW_PROSPECTIVE_KERNEL_MATERIAL_CONTEXT_V4,
    ) {
        return Err("prospective producer intent signature invalid".into());
    }
    let (root, _, _) = commit_identity_v4(&intent.commit);
    if request.bundle.claim().as_bytes() != root || intent.manifest.root.as_bytes() != root || intent.manifest.scopes.len() != 1 {
        return Err("prospective bundle/manifest root substituted".into());
    }
    intent.manifest.validate(max_bytes).map_err(|e| e.to_string())?;
    let mut candidate = prepare_unsigned_kernel_candidate_v4(
        state,
        params,
        &intent.producer,
        &intent.commit,
        &intent.salt,
        intent.commit_daa,
        raw_depth,
    )?;
    if !matches!(&request.bundle, PalwProspectiveKernelBundleV4::TypedKernelV4(_)) {
        return Err("prospective enrollment requires the complete v4 codec, including immutable artifacts".into());
    }
    let coverage = crate::palw_material_capacity_v1::prospective_kernel_bundle_requests_v4(&request.bundle)?;
    let _bundle_decode_lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(coverage.bytes().ok_or_else(|| coverage.to_string())?)
            .map_err(|e| e.to_string())?;
    candidate.try_reserve_kernel_material_v4(&root).map_err(|e| e.to_string())?;
    let auth = crate::palw_material_capacity_v1::prospective_kernel_bundle_auth_requests_v4(&request.bundle, &candidate)?;
    let _bundle_auth_lease = crate::palw_native_resource_v1::reserve_native_requests_v1(auth.bytes().ok_or_else(|| auth.to_string())?)
        .map_err(|e| e.to_string())?;
    let scope = request.bundle.verify(&candidate, max_bytes).map_err(|e| e.to_string())?;
    if intent.manifest.scopes.first() != Some(&scope) {
        return Err("prospective held bytes differ from producer-signed manifest".into());
    }
    let subject = candidate
        .try_claim_challenge_subject_v4(&root)
        .map_err(|e| e.to_string())?
        .ok_or("prospective actual claim subject absent")?;
    let row = candidate.claims.get(&root).ok_or("prospective admitted row absent")?;
    let authority = PalwProspectiveKernelMaterialAuthorityV4 {
        root: Hash64::from_bytes(root),
        class: Hash64::from_bytes(row.class_binding_id),
        job: Hash64::from_bytes(row.job_id),
        producer: intent.producer,
        commitment_root: Hash64::from_bytes(subject.commitment_root),
        checkpoint,
        prior_ledger_root: intent.ledger_root,
        prior_aux_root: intent.aux_root,
        commit_daa: intent.commit_daa,
    };
    let producer_pubkey = NativeRetainedV1::try_from_clone_v1(&producer.pubkey).map_err(|e| e.to_string())?;
    let mut provider_pubkeys = std::array::from_fn(|_| None);
    for (index, key) in intent.providers.iter().enumerate() {
        let provider = state.bond(key).ok_or("provider native bond absent")?;
        provider_pubkeys[index] = Some((*key, NativeRetainedV1::try_from_clone_v1(&provider.pubkey).map_err(|e| e.to_string())?));
    }
    Ok(PalwProspectiveKernelMaterialOwnedV4 {
        producer_pubkey,
        provider_pubkeys,
        candidate,
        authority,
        manifest: NativeRetainedV1::try_from_clone_v1(&intent.manifest).map_err(|e| e.to_string())?,
        request: request.id(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::{
        gate::ProsecutionBoundsV1,
        lifecycle::{ClaimLifecycleV1, LifecyclePolicyV1},
        spec::{
            ComputationSpecV1, SpecClaimBodyV1, SpecClassRowV1, TypedRootV1,
            retrieval::{EntryResponseV1, IndexV1, RetrievalClaimV1, RetrievalItemV1, RetrievalRuleV1, SnapshotDataV1},
        },
    };
    fn fixture() -> (KernelLedgerV1, PalwTypedKernelMaterialBundleV4) {
        // This pure codec fixture does not claim native admission or market
        // approval. The authenticated snapshot/entry paths themselves are real.
        let data = SnapshotDataV1::new(
            vec![
                RetrievalItemV1 { key: vec![1], payload: vec![1] },
                RetrievalItemV1 { key: vec![3], payload: vec![2] },
                RetrievalItemV1 { key: vec![-2], payload: vec![3] },
            ],
            1,
            1,
            2,
        )
        .unwrap();
        let root = RetrievalRootV1 {
            extension: misaka_palw_kernel::spec::k2_tr_v1_descriptor().digest(),
            snapshot: data.snapshot.clone(),
            index: IndexV1::Flat,
            rule: RetrievalRuleV1::TopKCountingV1 { k: 1, score_bits: 16 },
        };
        let result = data.retrieve(&root, &[1]).unwrap();
        let spec = ComputationSpecV1 {
            version: 1,
            mode: misaka_palw_kernel::mode::VerificationModeV1::OptimisticPublicVerification,
            roots: vec![TypedRootV1::RetrievalV1(root.clone())],
        };
        let class = spec.class_id().unwrap();
        let claim = SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: [7; 64], producer_bond: [8; 64], result: result.clone() });
        let id = claim.id();
        let policy =
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::from_bytes([1; 64]), Hash64::from_bytes([2; 64]));
        let mut ledger = crate::palw_kernel_route_v1::palw_kernel_route_template_v1(policy);
        ledger.typed.classes.insert(
            class,
            SpecClassRowV1 {
                spec,
                kind: SpecClassKindV1::Retrieval { root },
                bounds: ProsecutionBoundsV1 {
                    max_public_bytes: 1 << 20,
                    max_opening_bytes: 1 << 20,
                    max_filing_bytes: 1 << 20,
                    max_response_bytes: 1 << 20,
                    max_commit_bytes: 1 << 20,
                    max_localization_rounds: 2,
                    max_court_work: 1 << 20,
                    max_verifier_ram: 1 << 20,
                    max_retained_state: 1 << 20,
                    max_concurrent_sessions: 1,
                    deadline_daa: 10,
                },
            },
        );
        ledger.claims.insert(
            id,
            ClaimRowV1 {
                producer: [8; 64],
                class_binding_id: class,
                job_id: [7; 64],
                body: ClaimBodyV1::Spec(Box::new(SpecClaimBodyV1 { claim, pre_state: vec![], pre_source: None })),
                committed_daa: 1,
                life: ClaimLifecycleV1::new(LifecyclePolicyV1 { check_window_daa: 10, challenge_window_daa: 10 }),
                reserved: 0,
                liability_until: None,
                convicted: false,
                rewarded: false,
                sealed_daa: 0,
            },
        );
        let mut units = vec![];
        for p in 0..2 {
            units.push((misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1, p, data.slice_response(p as u64).unwrap()));
        }
        let (item, path) = data.open(result[0].id).unwrap();
        units.push((
            misaka_palw_kernel::spec::RETRIEVAL_ENTRY_STAGE_BASE_V1,
            0,
            borsh::to_vec(&EntryResponseV1 { item, path }).unwrap(),
        ));
        (ledger, PalwTypedKernelMaterialBundleV4 { claim: Hash64::from_bytes(id), units })
    }
    #[test]
    fn typed_retrieval_requires_all_snapshot_slices_and_real_returned_entry() {
        let (ledger, bundle) = fixture();
        let scope = bundle.verify(&ledger, 1 << 20).unwrap();
        assert_eq!(scope.units, 3);
        assert_eq!(scope.codec, PalwMaterialCodecV1::TypedKernelV4);
        let mut missing = bundle.clone();
        missing.units.pop();
        assert!(matches!(missing.verify(&ledger, 1 << 20), Err(PalwMaterialCapacityErrorV1::Incomplete)));
        let mut duplicated = bundle.clone();
        duplicated.units[1] = duplicated.units[0].clone();
        assert!(duplicated.verify(&ledger, 1 << 20).is_err());
        let mut corrupt = bundle.clone();
        *corrupt.units[0].2.last_mut().unwrap() ^= 1;
        assert!(corrupt.verify(&ledger, 1 << 20).is_err());
        assert!(!typed_kernel_required_unit_v4(&ledger, &bundle.claim.as_bytes(), 0, 0).unwrap());
        assert!(
            !typed_kernel_required_unit_v4(&ledger, &bundle.claim.as_bytes(), misaka_palw_kernel::spec::SNAPSHOT_STAGE_BASE_V1, 2)
                .unwrap()
        );
    }
    #[test]
    fn rfc23s2_archive_recovers_every_typed_unit_and_rejects_missing_or_substituted_authority() {
        use crate::palw_material_capacity_v1::{PalwLiabilityMaterialArchiveV1, PalwMaterialManifestV1};
        use std::collections::BTreeMap;
        let (ledger, bundle) = fixture();
        let scope = bundle.verify(&ledger, 1 << 20).unwrap();
        let consent = Hash64::from_u64_word(900);
        let manifest = PalwMaterialManifestV1 { root: bundle.claim, scopes: vec![scope.clone()] };
        let expected = BTreeMap::from([(consent, (manifest.clone(), 1 << 20))]);
        let raw = borsh::to_vec(&bundle).unwrap();
        let archive = BTreeMap::from([(consent, (manifest.id(), manifest.clone(), BTreeMap::from([(bundle.claim, raw.clone())])))]);
        let encoded = borsh::to_vec(&archive).unwrap();
        // A second cold restore does the same full validation; no warm provider
        // cache, URL or identity-only kernel retention is involved.
        for _ in 0..2 {
            let restored = PalwLiabilityMaterialArchiveV1::recover_v1(&encoded, &expected, 1 << 20, Some(&ledger), |_| None).unwrap();
            let (actual_scope, bytes) = restored.material_v1(consent, bundle.claim).unwrap();
            assert_eq!(actual_scope, &scope);
            assert_eq!(bytes, raw);
            let recovered: PalwTypedKernelMaterialBundleV4 = borsh::from_slice(bytes).unwrap();
            assert_eq!(recovered.verify(&ledger, 1 << 20).unwrap(), scope);
            let provider = crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(consent, 0));
            for (stage, position, original) in &bundle.units {
                let answer = restored
                    .answer_v1(
                        consent,
                        bundle.claim,
                        bundle.claim,
                        provider,
                        crate::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1::Kernel { stage: *stage, position: *position },
                    )
                    .unwrap();
                let crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::Answer { bytes, .. } = &*answer else {
                    panic!("archive must return the ordinary native court response");
                };
                assert_eq!(bytes, original, "every archived snapshot slice and returned entry is actually retrievable");
            }
        }
        assert!(PalwLiabilityMaterialArchiveV1::recover_v1(&encoded, &expected, 1 << 20, None, |_| None).is_err());
        let mut wrong_ledger = ledger.clone();
        wrong_ledger.claims.clear();
        assert!(PalwLiabilityMaterialArchiveV1::recover_v1(&encoded, &expected, 1 << 20, Some(&wrong_ledger), |_| None).is_err());
        let mut missing = bundle.clone();
        missing.units.pop();
        let mut corrupt = bundle.clone();
        *corrupt.units[0].2.last_mut().unwrap() ^= 1;
        for substituted in [missing, corrupt] {
            let bad = BTreeMap::from([(
                consent,
                (manifest.id(), manifest.clone(), BTreeMap::from([(bundle.claim, borsh::to_vec(&substituted).unwrap())])),
            )]);
            assert!(PalwLiabilityMaterialArchiveV1::recover_v1(
                &borsh::to_vec(&bad).unwrap(), &expected, 1 << 20, Some(&ledger), |_| None,
            ).is_err());
        }
        let empty = borsh::to_vec(&BTreeMap::<Hash64, u8>::new()).unwrap();
        assert!(PalwLiabilityMaterialArchiveV1::recover_v1(&empty, &expected, 1 << 20, Some(&ledger), |_| None).is_err());
        let mut wrong = expected.clone();
        wrong.values_mut().next().unwrap().0.root = consent;
        assert!(PalwLiabilityMaterialArchiveV1::recover_v1(&encoded, &wrong, 1 << 20, Some(&ledger), |_| None).is_err());
        assert!(
            PalwLiabilityMaterialArchiveV1::recover_v1(&encoded, &expected, encoded.len() as u64 - 1, Some(&ledger), |_| None)
                .is_err()
        );
        let mut duplicated = 2u32.to_le_bytes().to_vec();
        duplicated.extend_from_slice(&encoded[4..]);
        duplicated.extend_from_slice(&encoded[4..]);
        assert!(
            PalwLiabilityMaterialArchiveV1::recover_v1(&duplicated, &expected, 1 << 20, Some(&ledger), |_| None).is_err(),
            "duplicate consent keys must not silently collapse"
        );
        let mut trailing = encoded;
        trailing.push(0);
        assert!(PalwLiabilityMaterialArchiveV1::recover_v1(&trailing, &expected, 1 << 20, Some(&ledger), |_| None).is_err());
    }

    #[test]
    fn new_typed_codec_appends_without_reinterpreting_old_signed_codec_bytes() {
        assert_eq!(borsh::to_vec(&PalwMaterialCodecV1::NativeHeldComplete).unwrap(), vec![0]);
        assert_eq!(borsh::to_vec(&PalwMaterialCodecV1::KernelPositions).unwrap(), vec![1]);
        assert_eq!(borsh::to_vec(&PalwMaterialCodecV1::SegmentedParts).unwrap(), vec![2]);
        assert_eq!(borsh::to_vec(&PalwMaterialCodecV1::TypedKernelV4).unwrap(), vec![3]);
    }
}
