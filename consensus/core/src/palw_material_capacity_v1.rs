//! Signed wire capacity, independent of inference RAM. A provider admits the
//! canonical bytes it actually holds, including repeated checkpoint/opening
//! bytes. Unsupported layouts never acquire a capacity certificate.
use crate::{
    palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1},
    palw_step_leg::PalwStepBindingV2,
    Hash64,
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PALW_MATERIAL_MAX_SCOPES_V1: usize = 64;
pub const PALW_MATERIAL_MAX_UNITS_V1: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwMaterialCodecV1 {
    NativeHeldComplete,
    KernelPositions,
    SegmentedParts,
    /// Versioned complete typed court inventory, including snapshot slices and entries.
    TypedKernelV4,
}

/// The full original and each continuation occupy separate signed scopes.
/// `bytes` counts the canonical bundle, including binding/header and every
/// checkpoint, opening and leaf-evidence copy used by the consumer.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialScopeV1 {
    pub claim: Hash64,
    pub commitment: Hash64,
    pub codec: PalwMaterialCodecV1,
    pub bytes: u64,
    pub units: u32,
    pub content: Hash64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMaterialManifestV1 {
    pub root: Hash64,
    pub scopes: Vec<PalwMaterialScopeV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwMaterialCapacityErrorV1 {
    #[error("material exceeds its signed byte/unit/scope capacity")]
    Capacity,
    #[error("material is missing a required original/continuation/unit")]
    Incomplete,
    #[error("material substituted a signed scope, codec or canonical byte digest")]
    Substitution,
    #[error("unsupported material layout: {0}")]
    Unsupported(String),
    #[error("material authentication failed: {0}")]
    Authentication(String),
}
fn digest<T: BorshSerialize>(domain: &[u8], value: &T) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(domain, value))
}
pub fn kernel_material_commitment_v1(row: &misaka_palw_kernel::ledger::ClaimRowV1) -> Hash64 {
    digest(b"MISAKA/PALW/MONETARY-KERNEL-BODY/V1", &(row.class_binding_id, row.job_id, &row.body))
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelMaterialBundleV1 {
    pub claim: Hash64,
    /// `(stage, position, bytes)`, covering every committed position once.
    pub positions: Vec<(u8, u32, Vec<u8>)>,
}
impl PalwKernelMaterialBundleV1 {
    pub fn verify(
        &self,
        ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
        max_bytes: u64,
    ) -> Result<PalwMaterialScopeV1, PalwMaterialCapacityErrorV1> {
        use PalwMaterialCapacityErrorV1 as E;
        let bytes = borsh::object_length(self).map_err(|_| E::Capacity)? as u64;
        if bytes > max_bytes || self.positions.len() > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        let row = ledger.claims.get(&self.claim.as_bytes()).ok_or(E::Incomplete)?;
        if matches!(row.body, misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { .. }) {
            return Err(E::Unsupported("segmented claim requires canonical parts".into()));
        }
        let required = row.body.stages().iter().try_fold(0u64, |n, (_, count)| n.checked_add(u64::from(*count))).ok_or(E::Capacity)?;
        if required != self.positions.len() as u64 {
            return Err(E::Incomplete);
        }
        let mut seen = BTreeSet::new();
        for (stage, position, data) in &self.positions {
            if !seen.insert((*stage, *position)) {
                return Err(E::Substitution);
            }
            ledger
                .classify_served_position_v1(&self.claim.as_bytes(), *stage, *position, data)
                .map_err(|e| E::Authentication(e.into()))?;
        }
        Ok(PalwMaterialScopeV1 {
            claim: self.claim,
            commitment: kernel_material_commitment_v1(row),
            codec: PalwMaterialCodecV1::KernelPositions,
            bytes,
            units: self.positions.len() as u32,
            content: digest(b"MISAKA/PALW/MONETARY-KERNEL-BUNDLE/V1", self),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSegmentedMaterialBundleV1 {
    pub claim: Hash64,
    pub parts: Vec<(u32, u32, Vec<u8>)>,
}
impl PalwSegmentedMaterialBundleV1 {
    pub fn verify(
        &self,
        ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
        max_bytes: u64,
    ) -> Result<PalwMaterialScopeV1, PalwMaterialCapacityErrorV1> {
        use PalwMaterialCapacityErrorV1 as E;
        let bytes = borsh::object_length(self).map_err(|_| E::Capacity)? as u64;
        if bytes > max_bytes || self.parts.len() > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        let row = ledger.claims.get(&self.claim.as_bytes()).ok_or(E::Incomplete)?;
        let misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { evidence, segment_roots, .. } = &row.body else {
            return Err(E::Substitution);
        };
        let class = ledger.classes.get(&row.class_binding_id).ok_or_else(|| E::Unsupported("segmented class codec missing".into()))?;
        // Cardinality cannot be obtained from a RAM estimate or fixed sample:
        // derive each position's actual family codec layout, with a hard limit.
        if evidence.positions as usize > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        let mut required = 0usize;
        for position in 0..evidence.positions {
            required = required
                .checked_add(
                    misaka_palw_kernel::seg_da::position_part_bytes_v1(&class.program, position).map_err(E::Unsupported)?.len(),
                )
                .ok_or(E::Capacity)?;
            if required > PALW_MATERIAL_MAX_UNITS_V1 {
                return Err(E::Capacity);
            }
        }
        if required != self.parts.len() {
            return Err(E::Incomplete);
        }
        let mut seen = BTreeSet::new();
        for (position, part, data) in &self.parts {
            if !seen.insert((*position, *part)) || data.len() as u64 > misaka_palw_kernel::seg_da::SEG_PART_BYTES_V4 {
                return Err(E::Capacity);
            }
            if misaka_palw_kernel::seg_da::classify_part_v1(&class.program, segment_roots, evidence.positions, *position, data)
                .map_err(|e| E::Authentication(e.into()))?
                != *part
            {
                return Err(E::Substitution);
            }
        }
        Ok(PalwMaterialScopeV1 {
            claim: self.claim,
            commitment: kernel_material_commitment_v1(row),
            codec: PalwMaterialCodecV1::SegmentedParts,
            bytes,
            units: self.parts.len() as u32,
            content: digest(b"MISAKA/PALW/MONETARY-SEGMENTED-BUNDLE/V1", self),
        })
    }
}
impl PalwMaterialManifestV1 {
    pub fn validate(&self, max_bytes: u64) -> Result<u64, PalwMaterialCapacityErrorV1> {
        if self.root == Hash64::default()
            || self.scopes.is_empty()
            || self.scopes.len() > PALW_MATERIAL_MAX_SCOPES_V1
            || self.scopes.windows(2).any(|p| p[0].claim >= p[1].claim)
        {
            return Err(PalwMaterialCapacityErrorV1::Capacity);
        }
        // Exact per-contract provider framing: consent key + manifest id +
        // data-map length and each claim key / byte-vector length. The outer
        // map's four bytes are charged separately to the provider total cap.
        let framing = 132u64
            .checked_add((self.scopes.len() as u64).checked_mul(68).ok_or(PalwMaterialCapacityErrorV1::Capacity)?)
            .ok_or(PalwMaterialCapacityErrorV1::Capacity)?;
        let mut total = (borsh::object_length(self).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)? as u64)
            .checked_add(framing)
            .ok_or(PalwMaterialCapacityErrorV1::Capacity)?;
        for scope in &self.scopes {
            if scope.claim == Hash64::default()
                || scope.commitment == Hash64::default()
                || scope.content == Hash64::default()
                || scope.bytes == 0
                || scope.units == 0
                || scope.units as usize > PALW_MATERIAL_MAX_UNITS_V1
            {
                return Err(PalwMaterialCapacityErrorV1::Capacity);
            }
            total = total.checked_add(scope.bytes).ok_or(PalwMaterialCapacityErrorV1::Capacity)?;
        }
        if total > max_bytes {
            return Err(PalwMaterialCapacityErrorV1::Capacity);
        }
        Ok(total)
    }
    pub fn id(&self) -> Hash64 {
        digest(b"MISAKA/PALW/MONETARY-MATERIAL-MANIFEST/V1", self)
    }
    pub fn scope(&self, claim: Hash64) -> Option<&PalwMaterialScopeV1> {
        self.scopes.iter().find(|s| s.claim == claim)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwMaterialNativeAuthorityV1 {
    pub execution_root: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub ladder: u64,
    pub form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwHeldMaterialBundleV1 {
    pub claim: Hash64,
    pub binding: Box<PalwStepBindingV2>,
    pub units: Vec<(PalwHeldMissingV1, PalwHeldDisclosureV1)>,
}
impl PalwHeldMaterialBundleV1 {
    /// The actual backend codec constructs every consumer unit before signing
    /// Ready. Neither RAM estimates nor model labels substitute for its bytes.
    pub fn from_capture(
        claim: Hash64,
        backend: &dyn crate::palw_backend::PalwExecutionBackendV1,
        capture: &[u8],
        prompt: &[u32],
        roots: crate::palw_backend::PalwClaimRootsV1,
        work_leaves: u64,
        authority: PalwMaterialNativeAuthorityV1,
        max_bytes: u64,
    ) -> Result<Self, PalwMaterialCapacityErrorV1> {
        use PalwMaterialCapacityErrorV1 as E;
        let first = crate::palw_da_rcore_v1::palw_da_held_disclosure_from_capture_v1(
            backend,
            capture,
            prompt,
            roots,
            work_leaves,
            PalwHeldMissingV1::StepLeaf { leaf: 0 },
            authority.form,
            || Err("first leaf must carry the binding".into()),
        )
        .map_err(E::Authentication)?;
        let binding = first.0;
        if binding.committed_execution_root != roots.execution_root || binding.step_leaf_count == 0 {
            return Err(E::Substitution);
        }
        let mut missing = Vec::new();
        let tiles = if crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(authority.form, &binding.shape_profile)
            == crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
        {
            crate::palw_prompt_ids_v1::prompt_ids_tile_count_v1(u64::from(binding.job_context.declared_prefill_tokens))
                .ok_or(E::Capacity)?
        } else {
            0
        };
        for tile in 0..tiles {
            missing.push(PalwHeldMissingV1::PromptIdsTile { tile: tile.try_into().map_err(|_| E::Capacity)? });
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
            let chunks =
                crate::palw_state_chunk_map::palw_state_chunk_count_at_v1(&binding.shape_profile, positions).ok_or(E::Incomplete)?;
            if (missing.len() as u64).checked_add(chunks).is_none_or(|n| n > PALW_MATERIAL_MAX_UNITS_V1 as u64) {
                return Err(E::Capacity);
            }
            for chunk in 0..chunks {
                missing.push(PalwHeldMissingV1::StateChunk { checkpoint, chunk: chunk.try_into().map_err(|_| E::Capacity)? });
            }
        }
        if (missing.len() as u64).checked_add(binding.step_leaf_count).is_none_or(|n| n > PALW_MATERIAL_MAX_UNITS_V1 as u64) {
            return Err(E::Capacity);
        }
        for leaf in 0..binding.step_leaf_count {
            missing.push(PalwHeldMissingV1::StepLeaf { leaf });
        }
        let range = crate::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1;
        for first in (0..binding.step_leaf_count).step_by(range as usize) {
            missing.push(PalwHeldMissingV1::StepRange {
                first,
                count: binding.step_leaf_count.saturating_sub(first).min(u64::from(range)) as u32,
            });
        }
        if missing.len() > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        let mut bundle = Self { claim, binding: Box::new(binding.clone()), units: Vec::new() };
        let mut retained = borsh::object_length(&bundle).map_err(|_| E::Capacity)? as u64;
        for missing in missing {
            let (actual, disclosure) = crate::palw_da_rcore_v1::palw_da_held_disclosure_from_capture_v1(
                backend,
                capture,
                prompt,
                roots,
                work_leaves,
                missing,
                authority.form,
                || Ok(binding.clone()),
            )
            .map_err(E::Authentication)?;
            if actual != binding {
                return Err(E::Substitution);
            }
            retained = retained
                .checked_add(borsh::object_length(&(missing, &disclosure)).map_err(|_| E::Capacity)? as u64)
                .ok_or(E::Capacity)?;
            if retained > max_bytes {
                return Err(E::Capacity);
            }
            bundle.units.push((missing, disclosure));
        }
        bundle.verify(authority, max_bytes)?;
        Ok(bundle)
    }
    /// Counts every committed prompt tile, checkpoint chunk and step evidence;
    /// ranges additionally cover all step hashes in canonical 1024-leaf blocks.
    /// Unique in-range units with these exact cardinalities prove coverage.
    pub fn verify(
        &self,
        authority: PalwMaterialNativeAuthorityV1,
        max_bytes: u64,
    ) -> Result<PalwMaterialScopeV1, PalwMaterialCapacityErrorV1> {
        use PalwMaterialCapacityErrorV1 as E;
        let bytes = borsh::object_length(self).map_err(|_| E::Capacity)? as u64;
        if bytes > max_bytes || self.units.len() > PALW_MATERIAL_MAX_UNITS_V1 {
            return Err(E::Capacity);
        }
        let PalwMaterialNativeAuthorityV1 { execution_root, class_id, artifact_root, ladder, form } = authority;
        let b = &self.binding;
        crate::palw_step_leg::verify_binding_v1(b).map_err(|e| E::Authentication(e.to_string()))?;
        if b.committed_execution_root != execution_root || b.step_leaf_count == 0 {
            return Err(E::Substitution);
        }
        let class_form = crate::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(form, &b.shape_profile);
        let prompt = if class_form == crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 {
            crate::palw_prompt_ids_v1::prompt_ids_tile_count_v1(u64::from(b.job_context.declared_prefill_tokens))
                .ok_or_else(|| E::Unsupported("prompt tile layout".into()))?
        } else {
            // Flat native prompts are carried/authenticated in complete leaf evidence;
            // their consumer has no PromptIdsTile demand.
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
            .ok_or_else(|| E::Unsupported("checkpoint coverage".into()))?;
            let positions = crate::palw_context_ladder::palw_checkpoint_positions_at_v1(&b.shape_profile, &b.job_context, covered);
            chunks = chunks
                .checked_add(
                    crate::palw_state_chunk_map::palw_state_chunk_count_at_v1(&b.shape_profile, positions)
                        .ok_or_else(|| E::Unsupported("state chunk codec".into()))?,
                )
                .ok_or(E::Capacity)?;
        }
        let ranges = b.step_leaf_count.div_ceil(u64::from(crate::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1));
        let required = prompt
            .checked_add(chunks)
            .and_then(|n| n.checked_add(b.step_leaf_count))
            .and_then(|n| n.checked_add(ranges))
            .ok_or(E::Capacity)?;
        if required > PALW_MATERIAL_MAX_UNITS_V1 as u64 || self.units.len() as u64 != required {
            return Err(E::Incomplete);
        }
        let mut seen = BTreeSet::new();
        let mut counts = [0u64; 4];
        for (missing, disclosure) in &self.units {
            if !seen.insert(*missing) {
                return Err(E::Substitution);
            }
            let category = match missing {
                PalwHeldMissingV1::PromptIdsTile { .. } => 0,
                PalwHeldMissingV1::StateChunk { .. } => 1,
                PalwHeldMissingV1::StepLeaf { .. } => 2,
                PalwHeldMissingV1::StepRange { first, count } => {
                    let max = u64::from(crate::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1);
                    if first % max != 0 || u64::from(*count) != (b.step_leaf_count.saturating_sub(*first)).min(max) {
                        return Err(E::Incomplete);
                    }
                    3
                }
            };
            crate::palw_held_da_v1::palw_held_da_check_disclosure_v1(&execution_root, missing, b, disclosure, ladder, form)
                .map_err(|e| E::Authentication(e.to_string()))?;
            if let PalwHeldDisclosureV1::StepLeaf { evidence } = disclosure {
                evidence
                    .verdict_at_v2(
                        &crate::palw_shard_court_v1::PalwOneMoveClaimV2 { execution_root, class_id, artifact_root },
                        ladder,
                        true,
                    )
                    .map_err(|e| E::Authentication(e.to_string()))?;
            }
            counts[category] += 1;
        }
        if counts != [prompt, chunks, b.step_leaf_count, ranges] {
            return Err(E::Incomplete);
        }
        Ok(PalwMaterialScopeV1 {
            claim: self.claim,
            commitment: execution_root,
            codec: PalwMaterialCodecV1::NativeHeldComplete,
            bytes,
            units: self.units.len() as u32,
            content: digest(b"MISAKA/PALW/MONETARY-HELD-BUNDLE/V1", self),
        })
    }
}

/// The provider's retained data and counters are recoverable together. Preparing
/// a new generation validates all bytes before mutation; replay is idempotent.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwMonetaryMaterialProviderStoreV1 {
    contracts: BTreeMap<Hash64, (Hash64, PalwMaterialManifestV1, BTreeMap<Hash64, Vec<u8>>)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwMonetaryMaterialDemandV1 {
    Held(PalwHeldMissingV1),
    Kernel { stage: u8, position: u32 },
    Segmented { position: u32, part: u32 },
}
impl PalwMonetaryMaterialProviderStoreV1 {
    /// Recovery of an offer that never obtained an accepted contract. The
    /// original signed preimage is checked locally; native finality must put
    /// every possible enrollment block beyond its admission deadline.
    pub fn discard_expired_offer(
        &mut self,
        enrollment: &crate::palw_monetary_material_v1::PalwMonetaryMaterialEnrollmentV1,
        mut native_expiry: impl FnMut(Hash64, u64) -> Result<bool, String>,
    ) -> Result<bool, PalwMaterialCapacityErrorV1> {
        let id = enrollment.id();
        let Some((_, manifest, _)) = self.contracts.get(&id) else { return Ok(false) };
        if manifest.root != enrollment.terms.root || enrollment.terms.material_manifest.as_ref() != Some(manifest) {
            return Err(PalwMaterialCapacityErrorV1::Substitution);
        }
        if !native_expiry(enrollment.terms.root, enrollment.terms.enrollment_until_daa)
            .map_err(PalwMaterialCapacityErrorV1::Authentication)?
        {
            return Ok(false);
        }
        self.contracts.remove(&id);
        Ok(true)
    }
    pub fn admit_material(
        &mut self,
        consent: Hash64,
        manifest: &PalwMaterialManifestV1,
        max_bytes: u64,
        total_capacity: u64,
        held: &[PalwHeldMaterialBundleV1],
        positions: &[PalwKernelMaterialBundleV1],
        segmented: &[PalwSegmentedMaterialBundleV1],
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        mut authority: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
    ) -> Result<(), PalwMaterialCapacityErrorV1> {
        let mut checked = Vec::new();
        for bundle in held {
            let native = authority(bundle.claim).ok_or(PalwMaterialCapacityErrorV1::Incomplete)?;
            checked
                .push((bundle.verify(native, max_bytes)?, borsh::to_vec(bundle).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)?));
        }
        for bundle in positions {
            checked.push((
                bundle.verify(ledger.ok_or(PalwMaterialCapacityErrorV1::Incomplete)?, max_bytes)?,
                borsh::to_vec(bundle).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)?,
            ));
        }
        for bundle in segmented {
            checked.push((
                bundle.verify(ledger.ok_or(PalwMaterialCapacityErrorV1::Incomplete)?, max_bytes)?,
                borsh::to_vec(bundle).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)?,
            ));
        }
        self.admit_verified(consent, manifest, max_bytes, total_capacity, checked)
    }
    /// Prospective work is checked by the native read-only replay before this
    /// call. Its retained candidate remains the verifier context; no caller's
    /// boolean or wire ledger can replace that authority.
    fn admit_prospective_kernel_v4(
        &mut self,
        consent: Hash64,
        material: &crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialOwnedV4,
        bundle: &crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
        max_bytes: u64,
        total_capacity: u64,
    ) -> Result<(), PalwMaterialCapacityErrorV1> {
        use crate::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4 as B;
        let scope = bundle.verify(material.candidate_ledger(), max_bytes)?;
        let bytes = match bundle {
            B::KernelPositions(b) => borsh::to_vec(b),
            B::SegmentedParts(b) => borsh::to_vec(b),
            B::TypedKernelV4(b) => borsh::to_vec(b),
        }
        .map_err(|_| PalwMaterialCapacityErrorV1::Capacity)?;
        self.admit_verified(consent, material.manifest(), max_bytes, total_capacity, vec![(scope, bytes)])
    }
    pub fn answer_action(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: crate::palw_state_v2::PalwBondKeyV2,
        demand: PalwMonetaryMaterialDemandV1,
    ) -> Result<crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1, PalwMaterialCapacityErrorV1> {
        use crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as A;
        use PalwMaterialCapacityErrorV1 as E;
        let (_, manifest, data) = self.contracts.get(&consent).ok_or(E::Incomplete)?;
        if manifest.root != root {
            return Err(E::Substitution);
        }
        let scope = manifest.scope(dependency).ok_or(E::Incomplete)?;
        let bytes = data.get(&dependency).ok_or(E::Incomplete)?;
        if bytes.len() as u64 != scope.bytes {
            return Err(E::Substitution);
        }
        match demand {
            PalwMonetaryMaterialDemandV1::Held(missing) => self.held_answer_action(consent, root, dependency, provider, missing),
            PalwMonetaryMaterialDemandV1::Kernel { stage, position } => {
                let positions = match scope.codec {
                    PalwMaterialCodecV1::KernelPositions => {
                        let bundle: PalwKernelMaterialBundleV1 =
                            borsh::from_slice(bytes).map_err(|e| E::Authentication(e.to_string()))?;
                        if bundle.claim != dependency || digest(b"MISAKA/PALW/MONETARY-KERNEL-BUNDLE/V1", &bundle) != scope.content {
                            return Err(E::Substitution);
                        }
                        bundle.positions
                    }
                    PalwMaterialCodecV1::TypedKernelV4 => {
                        let bundle: crate::palw_prospective_kernel_material_v4::PalwTypedKernelMaterialBundleV4 =
                            borsh::from_slice(bytes).map_err(|e| E::Authentication(e.to_string()))?;
                        if bundle.claim != dependency
                            || digest(b"MISAKA/PALW/MONETARY-TYPED-KERNEL-BUNDLE/V4", &bundle) != scope.content
                        {
                            return Err(E::Substitution);
                        }
                        bundle.units
                    }
                    _ => return Err(E::Substitution),
                };
                let bytes =
                    positions.into_iter().find(|(s, p, _)| (*s, *p) == (stage, position)).map(|(_, _, b)| b).ok_or(E::Incomplete)?;
                Ok(A::Answer { root, provider, dependency, stage, position, bytes })
            }
            PalwMonetaryMaterialDemandV1::Segmented { position, part } => {
                if scope.codec != PalwMaterialCodecV1::SegmentedParts {
                    return Err(E::Substitution);
                }
                let bundle: PalwSegmentedMaterialBundleV1 = borsh::from_slice(bytes).map_err(|e| E::Authentication(e.to_string()))?;
                if bundle.claim != dependency || digest(b"MISAKA/PALW/MONETARY-SEGMENTED-BUNDLE/V1", &bundle) != scope.content {
                    return Err(E::Substitution);
                }
                let bytes =
                    bundle.parts.into_iter().find(|(p, s, _)| (*p, *s) == (position, part)).map(|(_, _, b)| b).ok_or(E::Incomplete)?;
                Ok(A::SegmentedAnswer { root, provider, dependency, position, part, bytes })
            }
        }
    }
    /// The verifier must use native accepted-delta + selected-chain finality
    /// authority. A proposed release, fixed TTL or merely absent row is refused.
    pub fn release_if_finalized(
        &mut self,
        root: Hash64,
        stored_consent: Hash64,
        release_consent: Hash64,
        tail: Hash64,
        release_block: crate::BlockHash,
        mut native_witness: impl FnMut(Hash64, Hash64, Hash64, crate::BlockHash) -> Result<bool, String>,
    ) -> Result<bool, PalwMaterialCapacityErrorV1> {
        let Some((_, manifest, _)) = self.contracts.get(&stored_consent) else { return Ok(false) };
        if manifest.root != root {
            return Err(PalwMaterialCapacityErrorV1::Substitution);
        }
        if !native_witness(root, release_consent, tail, release_block).map_err(PalwMaterialCapacityErrorV1::Authentication)? {
            return Ok(false);
        }
        self.contracts.remove(&stored_consent);
        Ok(true)
    }
    pub fn encoded(&self) -> Result<Vec<u8>, PalwMaterialCapacityErrorV1> {
        borsh::to_vec(self).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)
    }
    /// Every retained obligation as `(stored consent, signed root)`, in key order, for a provider janitor that asks the
    /// native GC witness about each one. Reading it authorizes nothing: deletion stays [`Self::release_if_finalized`]'s.
    /// Exact retained native codec claims, grouped by stored consent. Metadata
    /// GC uses the pre-delete set and only a verified native GC completion.
    pub fn held_native_authorities_v1(&self) -> BTreeMap<Hash64, BTreeSet<Hash64>> {
        self.contracts
            .iter()
            .map(|(consent, (_, manifest, _))| {
                (
                    *consent,
                    manifest
                        .scopes
                        .iter()
                        .filter(|scope| scope.codec == PalwMaterialCodecV1::NativeHeldComplete)
                        .map(|scope| scope.claim)
                        .collect(),
                )
            })
            .collect()
    }
    pub fn held_contracts_v1(&self) -> Vec<(Hash64, Hash64)> {
        self.contracts.iter().map(|(consent, (_, manifest, _))| (*consent, manifest.root)).collect()
    }
    /// Reopening never trusts persisted counters or hashes. All retained codec
    /// bytes are authenticated again against the caller's coherent native and
    /// kernel snapshot, before the provider exposes a single answer.
    pub fn recover_verified(
        encoded: &[u8],
        max_contract_bytes: u64,
        total_capacity: u64,
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
    ) -> Result<Self, PalwMaterialCapacityErrorV1> {
        crate::palw_native_resource_v1::NativeRetainedV1::<Self>::recover_provider_verified_v1(
            encoded,
            max_contract_bytes,
            total_capacity,
            ledger,
            authority,
        )
        .and_then(|owner| owner.into_unreserved_legacy_v1().map_err(|e| e.to_string()))
        .map_err(PalwMaterialCapacityErrorV1::Authentication)
    }
    fn recover_verified_inner_v1(
        encoded: &[u8],
        max_contract_bytes: u64,
        total_capacity: u64,
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        mut authority: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
    ) -> Result<Self, PalwMaterialCapacityErrorV1> {
        Self::recover_inner_v4(encoded, max_contract_bytes, total_capacity, ledger, &mut authority, None)
    }
    fn recover_inner_v4(
        encoded: &[u8],
        max_contract_bytes: u64,
        total_capacity: u64,
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        mut authority: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
        mut retain_kernel: Option<&mut dyn FnMut(Hash64, &PalwMaterialScopeV1, &[u8]) -> Result<(), String>>,
    ) -> Result<Self, PalwMaterialCapacityErrorV1> {
        use PalwMaterialCapacityErrorV1 as E;
        if encoded.len() as u64 > total_capacity.saturating_add(1 << 20) {
            return Err(E::Capacity);
        }
        let persisted: Self = borsh::from_slice(encoded).map_err(|e| E::Authentication(e.to_string()))?;
        let mut recovered = Self::default();
        for (consent, (id, manifest, data)) in persisted.contracts {
            if id != manifest.id() {
                return Err(E::Substitution);
            }
            let mut checked = Vec::new();
            for scope in &manifest.scopes {
                let bytes = data.get(&scope.claim).ok_or(E::Incomplete)?;
                let actual = if scope.codec != PalwMaterialCodecV1::NativeHeldComplete && retain_kernel.is_some() {
                    // A signed identity frame can be kept without pretending a
                    // historical claim is still present. This grants no answer
                    // authority; that needs a fresh native-positive replay.
                    let domain: &[u8] = match scope.codec {
                        PalwMaterialCodecV1::KernelPositions => b"MISAKA/PALW/MONETARY-KERNEL-BUNDLE/V1",
                        PalwMaterialCodecV1::SegmentedParts => b"MISAKA/PALW/MONETARY-SEGMENTED-BUNDLE/V1",
                        PalwMaterialCodecV1::TypedKernelV4 => b"MISAKA/PALW/MONETARY-TYPED-KERNEL-BUNDLE/V4",
                        PalwMaterialCodecV1::NativeHeldComplete => unreachable!(),
                    };
                    if bytes.len() as u64 != scope.bytes
                        || bytes.get(..64) != Some(scope.claim.as_byte_slice())
                        || bytes.get(64..68).is_none_or(|b| u32::from_le_bytes(b.try_into().unwrap()) != scope.units)
                        || Hash64::from_bytes(misaka_palw_kernel::hash::id(domain, bytes)) != scope.content
                    {
                        return Err(E::Substitution);
                    }
                    retain_kernel.as_mut().unwrap()(consent, scope, bytes).map_err(E::Authentication)?;
                    scope.clone()
                } else {
                    match scope.codec {
                        PalwMaterialCodecV1::NativeHeldComplete => {
                            let bundle: PalwHeldMaterialBundleV1 =
                                borsh::from_slice(bytes).map_err(|e| E::Authentication(e.to_string()))?;
                            let native = authority(scope.claim).ok_or(E::Incomplete)?;
                            bundle.verify(native, max_contract_bytes)?
                        }
                        PalwMaterialCodecV1::KernelPositions => {
                            let bundle: PalwKernelMaterialBundleV1 =
                                borsh::from_slice(bytes).map_err(|e| E::Authentication(e.to_string()))?;
                            bundle.verify(ledger.ok_or(E::Incomplete)?, max_contract_bytes)?
                        }
                        PalwMaterialCodecV1::SegmentedParts => {
                            let bundle: PalwSegmentedMaterialBundleV1 =
                                borsh::from_slice(bytes).map_err(|e| E::Authentication(e.to_string()))?;
                            bundle.verify(ledger.ok_or(E::Incomplete)?, max_contract_bytes)?
                        }
                        PalwMaterialCodecV1::TypedKernelV4 => {
                            let bundle: crate::palw_prospective_kernel_material_v4::PalwTypedKernelMaterialBundleV4 =
                                borsh::from_slice(bytes).map_err(|e| E::Authentication(e.to_string()))?;
                            bundle.verify(ledger.ok_or(E::Incomplete)?, max_contract_bytes)?
                        }
                    }
                };
                checked.push((actual, bytes.clone()));
            }
            if data.len() != checked.len() {
                return Err(E::Substitution);
            }
            recovered.admit_verified(consent, &manifest, max_contract_bytes, total_capacity, checked)?;
        }
        Ok(recovered)
    }
    /// Reading a retained generation does not authorize using it. The caller
    /// must check current native generation/provider/exact scope before answer.
    pub fn material_generation_scope_v4(
        &self,
        preferred: Hash64,
        root: Hash64,
        dependency: Hash64,
    ) -> Option<(Hash64, &PalwMaterialScopeV1)> {
        if let Some((_, m, _)) = self.contracts.get(&preferred) {
            if m.root == root {
                if let Some(s) = m.scope(dependency) {
                    return Some((preferred, s));
                }
            }
        }
        self.contracts.iter().find_map(|(consent, (_, manifest, _))| {
            (manifest.root == root).then(|| manifest.scope(dependency).map(|scope| (*consent, scope))).flatten()
        })
    }
    fn admit_verified(
        &mut self,
        consent: Hash64,
        manifest: &PalwMaterialManifestV1,
        max_bytes: u64,
        total_capacity: u64,
        bundles: Vec<(PalwMaterialScopeV1, Vec<u8>)>,
    ) -> Result<(), PalwMaterialCapacityErrorV1> {
        use PalwMaterialCapacityErrorV1 as E;
        if !self.contracts.contains_key(&consent)
            && self.contracts.len() >= crate::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_MAX_CONTRACTS_V1 as usize
        {
            return Err(E::Capacity);
        }
        let total = manifest.validate(max_bytes)?;
        let mut data = BTreeMap::new();
        for (scope, bytes) in bundles {
            if manifest.scope(scope.claim) != Some(&scope)
                || bytes.len() as u64 != scope.bytes
                || data.insert(scope.claim, bytes).is_some()
            {
                return Err(E::Substitution);
            }
        }
        if data.len() != manifest.scopes.len() {
            return Err(E::Incomplete);
        }
        // Digest/authentication must be verified through the typed ingestion
        // adapter; this low-level method is deliberately private to this module.
        let used = self
            .contracts
            .iter()
            .filter(|(id, _)| **id != consent)
            .try_fold(total.checked_add(4).ok_or(E::Capacity)?, |sum, (_, (_, m, _))| sum.checked_add(m.validate(u64::MAX).ok()?))
            .ok_or(E::Capacity)?;
        if used > total_capacity {
            return Err(E::Capacity);
        }
        if self
            .contracts
            .get(&consent)
            .is_some_and(|existing| existing.0 != manifest.id() || existing.1 != *manifest || existing.2 != data)
        {
            return Err(E::Substitution);
        }
        self.contracts.insert(consent, (manifest.id(), manifest.clone(), data));
        Ok(())
    }
    pub fn admit_held(
        &mut self,
        consent: Hash64,
        manifest: &PalwMaterialManifestV1,
        max_bytes: u64,
        total_capacity: u64,
        bundles: &[PalwHeldMaterialBundleV1],
        mut authority: impl FnMut(Hash64) -> Option<PalwMaterialNativeAuthorityV1>,
    ) -> Result<(), PalwMaterialCapacityErrorV1> {
        let mut checked = Vec::new();
        for bundle in bundles {
            let native = authority(bundle.claim).ok_or(PalwMaterialCapacityErrorV1::Incomplete)?;
            let scope = bundle.verify(native, max_bytes)?;
            let bytes = borsh::to_vec(bundle).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)?;
            checked.push((scope, bytes));
        }
        self.admit_verified(consent, manifest, max_bytes, total_capacity, checked)
    }
    pub fn admit_kernel(
        &mut self,
        consent: Hash64,
        manifest: &PalwMaterialManifestV1,
        max_bytes: u64,
        total_capacity: u64,
        ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
        bundles: &[PalwKernelMaterialBundleV1],
        segmented: &[PalwSegmentedMaterialBundleV1],
    ) -> Result<(), PalwMaterialCapacityErrorV1> {
        let mut checked = Vec::new();
        for bundle in bundles {
            checked
                .push((bundle.verify(ledger, max_bytes)?, borsh::to_vec(bundle).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)?));
        }
        for bundle in segmented {
            checked
                .push((bundle.verify(ledger, max_bytes)?, borsh::to_vec(bundle).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)?));
        }
        self.admit_verified(consent, manifest, max_bytes, total_capacity, checked)
    }
    pub fn held_answer(
        &self,
        consent: Hash64,
        claim: Hash64,
        missing: PalwHeldMissingV1,
    ) -> Result<PalwHeldDisclosureV1, PalwMaterialCapacityErrorV1> {
        let (_, manifest, scopes) = self.contracts.get(&consent).ok_or(PalwMaterialCapacityErrorV1::Incomplete)?;
        if manifest.scope(claim).is_none_or(|s| s.codec != PalwMaterialCodecV1::NativeHeldComplete) {
            return Err(PalwMaterialCapacityErrorV1::Substitution);
        }
        let bundle: PalwHeldMaterialBundleV1 = borsh::from_slice(scopes.get(&claim).ok_or(PalwMaterialCapacityErrorV1::Incomplete)?)
            .map_err(|e| PalwMaterialCapacityErrorV1::Authentication(e.to_string()))?;
        let scope = manifest.scope(claim).ok_or(PalwMaterialCapacityErrorV1::Incomplete)?;
        if bundle.claim != claim
            || bundle.binding.committed_execution_root != scope.commitment
            || borsh::object_length(&bundle).map_err(|_| PalwMaterialCapacityErrorV1::Capacity)? as u64 != scope.bytes
            || digest(b"MISAKA/PALW/MONETARY-HELD-BUNDLE/V1", &bundle) != scope.content
        {
            return Err(PalwMaterialCapacityErrorV1::Substitution);
        }
        bundle
            .units
            .into_iter()
            .find(|(unit, _)| *unit == missing)
            .map(|(_, disclosure)| disclosure)
            .ok_or(PalwMaterialCapacityErrorV1::Incomplete)
    }
    pub fn held_answer_action(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: crate::palw_state_v2::PalwBondKeyV2,
        missing: PalwHeldMissingV1,
    ) -> Result<crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1, PalwMaterialCapacityErrorV1> {
        if self.contracts.get(&consent).is_none_or(|(_, manifest, _)| manifest.root != root) {
            return Err(PalwMaterialCapacityErrorV1::Substitution);
        }
        Ok(crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::HeldAnswer {
            root,
            dependency,
            provider,
            missing,
            disclosure: Box::new(self.held_answer(consent, dependency, missing)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }
    fn manifest() -> PalwMaterialManifestV1 {
        PalwMaterialManifestV1 {
            root: h(1),
            scopes: vec![PalwMaterialScopeV1 {
                claim: h(1),
                commitment: h(2),
                codec: PalwMaterialCodecV1::NativeHeldComplete,
                bytes: 8,
                units: 1,
                content: h(3),
            }],
        }
    }
    #[test]
    fn signed_material_capacity_counts_full_framing_and_refuses_short_or_duplicate_scope() {
        let manifest = manifest();
        let exact = manifest.validate(u64::MAX).unwrap();
        assert_eq!(manifest.validate(exact), Ok(exact));
        assert_eq!(manifest.validate(exact - 1), Err(PalwMaterialCapacityErrorV1::Capacity));
        let mut doubled = manifest.clone();
        doubled.scopes.push(doubled.scopes[0].clone());
        assert_eq!(doubled.validate(u64::MAX), Err(PalwMaterialCapacityErrorV1::Capacity));
    }
    #[test]
    fn provider_capacity_refusal_is_atomic_and_gc_requires_exact_finalized_native_generation() {
        // Storage atomicity seam only: these fabricated bytes deliberately do
        // not claim to certify a model or pass the typed ingestion verifier.
        let manifest = manifest();
        let mut store = PalwMonetaryMaterialProviderStoreV1::default();
        store.admit_verified(h(4), &manifest, 1000, 1004, vec![(manifest.scopes[0].clone(), vec![0; 8])]).unwrap();
        let before = store.clone();
        assert_eq!(store.admit_verified(h(5), &manifest, 1000, 1004, vec![]), Err(PalwMaterialCapacityErrorV1::Incomplete));
        assert_eq!(store, before);
        assert!(!store.release_if_finalized(h(1), h(4), h(6), h(7), h(8), |_, _, _, _| Ok(false)).unwrap());
        assert_eq!(store, before);
        assert!(store.release_if_finalized(h(9), h(4), h(6), h(7), h(8), |_, _, _, _| Ok(true)).is_err());
        assert_eq!(store, before);
        assert!(store
            .release_if_finalized(h(1), h(4), h(6), h(7), h(8), |root, consent, tail, block| {
                assert_eq!((root, consent, tail, block), (h(1), h(6), h(7), h(8)));
                Ok(true)
            })
            .unwrap());
        assert!(store.contracts.is_empty());
    }
    #[test]
    fn restart_does_not_trust_persisted_codec_counter_or_promise() {
        let manifest = manifest();
        let mut store = PalwMonetaryMaterialProviderStoreV1::default();
        store.admit_verified(h(4), &manifest, 1000, 1004, vec![(manifest.scopes[0].clone(), vec![0; 8])]).unwrap();
        let persisted = store.encoded().unwrap();
        assert!(PalwMonetaryMaterialProviderStoreV1::recover_verified(&persisted, 1000, 1004, None, |_| None).is_err());
    }
}

// Owner-local exact private provider layout; no codec or visibility change.
#[path = "palw_native_resource_v1/private/palw_material_capacity_v1.rs"]
mod native_resource_inventory_v1;

#[path = "palw_material_capacity_owner_v1.rs"]
mod material_capacity_owner_v1;

pub use material_capacity_owner_v1::prospective_kernel_bundle_auth_requests_v4;
pub use material_capacity_owner_v1::prospective_kernel_bundle_requests_v4;
