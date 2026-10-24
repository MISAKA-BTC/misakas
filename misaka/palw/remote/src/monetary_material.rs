//! Durable typed provider for signed payment-relative material obligations.
//! A fixed old manifest TTL cannot delete these bytes. Every reopen rechecks
//! full codec coverage; GC requires the native finalized accepted-release API.
use kaspa_consensus_core::{
    palw_material_capacity_v1::{PalwHeldMaterialBundleV1, PalwMaterialManifestV1, PalwMonetaryMaterialProviderStoreV1},
    palw_monetary_material_v1::{PalwMonetaryMaterialActionV1, PalwMonetaryMaterialEnrollmentV1},
    palw_state_v2::PalwBondKeyV2,
    Hash64,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Observe this exact replacement in an own-native covenant read. A higher
/// sequence alone may belong to a competing replacement and is not acceptance.
pub fn replacement_generation_observed_v4(
    intent: &kaspa_consensus_core::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementIntentV4,
    expected_consent: Option<Hash64>,
    current: &kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
) -> Result<bool, String> {
    if current.terms.root != intent.root {
        return Err("replacement observation belongs to another root".into());
    }
    if current.sequence < intent.sequence {
        return Ok(false);
    }
    if current.sequence == intent.sequence && expected_consent == Some(current.consent) {
        return Ok(true);
    }
    Err("native generation advanced without the exact saved replacement consent; acceptance unconfirmed".into())
}

pub struct DurableMonetaryMaterialProviderV1 {
    // The kernel releases this lease on crash; a PID-file lease would strand
    // capacity or allow concurrent snapshots to overwrite accepted material.
    _lease: File,
    file: PathBuf,
    max_contract_bytes: u64,
    total_capacity: u64,
    store: kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwMonetaryMaterialProviderStoreV1>,
    contexts: Option<crate::kernel_material_context_v4::KernelMaterialContextArchiveV4>,
    expiry_cursor: Option<Hash64>,
}
impl DurableMonetaryMaterialProviderV1 {
    /// Actual native work replay and complete bundle verification precede this
    /// call; a liability signature must follow successful atomic publication.
    pub fn prepare_prospective_kernel_v4(
        &mut self,
        provider: PalwBondKeyV2,
        enrollment: &PalwMonetaryMaterialEnrollmentV1,
        material: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelMaterialOwnedV4,
        bundle: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
        context: &kaspa_consensus_core::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
        capsule: &kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2,
    ) -> Result<Hash64, String> {
        if !enrollment.offers.iter().any(|o| o.provider == provider)
            || enrollment.terms.sponsor != material.authority().producer
            || enrollment.terms.root != material.authority().root
            || enrollment.terms.material_manifest.as_ref() != Some(material.manifest())
            || enrollment.terms.native_session.is_some()
            || enrollment.terms.max_material_bytes > self.max_contract_bytes
        {
            return Err("kernel material/enrollment native authority differs".into());
        }
        // These are independently supplied borrowed inputs. Publishing the
        // material under one consent and the restart context under another
        // leaves a successfully prepared provider unable to restore its bytes.
        // Match the unsigned financial identity; provider signatures may be
        // added only after publication and are authenticated by native ingress.
        let kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2::FundedEnroll {
            monetary, ..
        } = capsule else {
            return Err("prospective publication requires its initial funded enrollment capsule".into());
        };
        if context.request != material.request_id()
            || context.intent.manifest != *material.manifest()
            || monetary.terms != enrollment.terms
            || monetary.offers.len() != enrollment.offers.len()
            || monetary.offers.iter().zip(&enrollment.offers).any(|(a, b)| {
                (a.provider, a.collateral_sompi, a.prepaid_fee_sompi)
                    != (b.provider, b.collateral_sompi, b.prepaid_fee_sompi)
            })
        {
            return Err("prospective publication substitutes native request/context/financial consent".into());
        }
        let consent = enrollment.id();
        let candidate = self.store.provider_admit_prospective_kernel_v4(
            consent,
            material,
            bundle,
            enrollment.terms.max_material_bytes,
            self.total_capacity,
        )?;
        self.contexts.as_mut().ok_or("kernel ingress has no explicitly configured metadata capacities")?.insert(context, capsule)?;
        self.publish(candidate)?;
        Ok(consent)
    }
    /// New native quote authenticates the original work and accepted obligation;
    /// newly signed replacement still cannot answer before native acceptance.
    pub fn prepare_replacement_kernel_v4(
        &mut self,
        provider: PalwBondKeyV2,
        material: &kaspa_consensus_core::palw_kernel_material_replacement_v4::PalwKernelMaterialReplacementOwnedV4,
        bundle: &kaspa_consensus_core::palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
        context: &kaspa_consensus_core::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
    ) -> Result<Hash64, String> {
        // This fixed request ID comes from native covenant/nonce and identity
        // replay, not the independently supplied context. Refuse before hashing
        // the capsule, constructing a candidate, or publishing either file.
        if material.context_request() != context.request {
            return Err("replacement publication substitutes own native context request".into());
        }
        let (e, consent, _) =
            kaspa_consensus_core::palw_kernel_material_enrollment_v4::kernel_material_capsule_identity_v4(&material.proposal)?;
        if !e.offers.iter().any(|p| p.provider == provider)
            || e.terms.root != context.intent.manifest.root
            || e.terms.material_manifest.as_ref() != Some(&context.intent.manifest)
            || material.manifest() != &context.intent.manifest
            || e.terms.max_material_bytes > self.max_contract_bytes
        {
            return Err("replacement publication substitutes own exact grant".into());
        }
        let candidate =
            self.store.provider_admit_replacement_kernel_v4(material, bundle, e.terms.max_material_bytes, self.total_capacity)?;
        self.contexts.as_mut().ok_or("replacement metadata capacities absent")?.insert(context, &material.proposal)?;
        self.publish(candidate)?;
        Ok(consent)
    }
    pub fn persist_kernel_signatures_v4(
        &mut self,
        context: &kaspa_consensus_core::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
        capsule: &kaspa_consensus_core::palw_material_release_recovery_v2::PalwMaterialRecoveryActionV2,
    ) -> Result<(), String> {
        self.contexts.as_mut().ok_or("kernel context archive unavailable")?.insert(context, capsule)?;
        Ok(())
    }
    /// Prospective consent is immutable. Ready binds the actual one-claim capture
    /// only after the whole typed bundle has been atomically published.
    pub fn prepare_native_ready_v3(
        &mut self,
        provider: PalwBondKeyV2,
        row: &kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
        ready: &kaspa_consensus_core::palw_attempt_material_v3::PalwNativeMaterialReadyV3,
        bundle: &PalwHeldMaterialBundleV1,
        authority: kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1,
    ) -> Result<Hash64, String> {
        if row.terms.root != ready.session
            || row.consent != ready.consent
            || row.terms.native_session.is_none()
            || row.native_binding.is_some()
            || !ready.providers.contains(&provider)
            || bundle.claim != ready.binding.claim
            || row.terms.max_material_bytes > self.max_contract_bytes
        {
            return Err("provider prospective session or exact ready differs".into());
        }
        let candidate = self.store.provider_admit_material_v1(
            row.consent,
            &ready.binding.manifest,
            row.terms.max_material_bytes,
            self.total_capacity,
            std::slice::from_ref(bundle),
            &[],
            &[],
            None,
            |id| (id == bundle.claim).then_some(authority),
        )?;
        self.publish(candidate)?;
        Ok(ready.id())
    }
    /// Authority metadata has a separate V2 file family. Its complete identity
    /// is fsynced before material publication and before a Ready signature.
    pub fn prepare_native_ready_routed_v2(
        &mut self,
        provider: PalwBondKeyV2,
        row: &kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
        ready: &kaspa_consensus_core::palw_attempt_material_v3::PalwNativeMaterialReadyV3,
        bundle: &kaspa_consensus_core::palw_routed_material_v2::PalwHeldMaterialBundleV2,
        authority: &kaspa_consensus_core::palw_routed_material_v2::PalwMaterialNativeAuthorityV2,
    ) -> Result<Hash64, kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1> {
        use kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E;
        if row.terms.root != ready.session
            || row.consent != ready.consent
            || row.terms.native_session.is_none()
            || row.native_binding.is_some()
            || !ready.providers.contains(&provider)
            || bundle.claim != ready.binding.claim
            || row.terms.max_material_bytes > self.max_contract_bytes
            || authority.record().claim != bundle.claim
        {
            return Err(E::Substitution);
        }
        let candidate = self.store.provider_admit_routed_held_v2(
            row.consent,
            &ready.binding.manifest,
            row.terms.max_material_bytes,
            self.total_capacity,
            bundle,
            authority,
        )?;
        self.sweep_routed_authorities_v2()?;
        save_routed_authority_v2(&self.file, authority.record(), self.max_contract_bytes)?;
        self.publish_routed_v2(candidate)?;
        Ok(ready.id())
    }
    /// Explicit versioned response. The caller supplies a fresh native-positive
    /// authority, selected accepted provider generation and exact challenge
    /// receipt; a locally recovered authority record cannot call this API.
    pub fn answer_routed_material_v2(
        &self,
        consent: Hash64,
        root: Hash64,
        claim: Hash64,
        provider: PalwBondKeyV2,
        leaf: u64,
        authority: &kaspa_consensus_core::palw_routed_material_v2::PalwMaterialNativeAuthorityV2,
        receipt: Option<kaspa_consensus_core::palw_routed_material_v2::PalwRoutedMaterialReceiptV2>,
        sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> Result<
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
            kaspa_consensus_core::palw_routed_material_v2::PalwRoutedMaterialAnswerV2,
        >,
        kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1,
    > {
        use kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E;
        let historical = load_routed_authority_v2(&self.file, claim, self.max_contract_bytes)?;
        if &*historical != authority.record() {
            return Err(E::Substitution);
        }
        self.store.provider_routed_answer_v2(consent, root, claim, provider, leaf, authority, receipt, sign)
    }
    fn publish_routed_v2(
        &mut self,
        candidate: kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwMonetaryMaterialProviderStoreV1>,
    ) -> Result<(), kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1> {
        use kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E;
        let len = borsh::object_length(&*candidate).map_err(|_| E::Capacity)? as u64;
        let _encoding =
            kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(len.checked_mul(2).ok_or(E::Capacity)?)
                .map_err(E::LocalResource)?;
        let raw = borsh::to_vec(&*candidate).map_err(|e| E::Authentication(e.to_string()))?;
        atomic_routed_file_v2(&self.file, &raw)?;
        self.store = candidate;
        self.sweep_routed_authorities_v2()?;
        Ok(())
    }
    /// Orphan metadata is never answer authority. With the store's process
    /// lease/mutex held, remove only this version's rows no retained generation
    /// references, after its authoritative snapshot publication has completed.
    fn sweep_routed_authorities_v2(&self) -> Result<(), kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1> {
        use kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E;
        let directory = self.file.with_extension("routing-authorities-v2");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(E::Authentication(e.to_string())),
        };
        for entry in entries {
            let path = entry.map_err(|e| E::Authentication(e.to_string()))?.path();
            if path.extension().is_none_or(|v| v != "v2") {
                continue;
            }
            let claim: Hash64 =
                path.file_stem().and_then(|v| v.to_str()).ok_or(E::Substitution)?.parse().map_err(|_| E::Substitution)?;
            if !self.store.holds_routed_native_claim_v2(claim) {
                fs::remove_file(path).map_err(|e| E::Authentication(e.to_string()))?;
            }
        }
        File::open(directory).and_then(|d| d.sync_all()).map_err(|e| E::Authentication(e.to_string()))
    }
    pub fn prepare_material(
        &mut self,
        provider: PalwBondKeyV2,
        enrollment: &PalwMonetaryMaterialEnrollmentV1,
        held: &[PalwHeldMaterialBundleV1],
        positions: &[kaspa_consensus_core::palw_material_capacity_v1::PalwKernelMaterialBundleV1],
        segmented: &[kaspa_consensus_core::palw_material_capacity_v1::PalwSegmentedMaterialBundleV1],
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
    ) -> Result<Hash64, String> {
        if !enrollment.offers.iter().any(|o| o.provider == provider) || enrollment.terms.max_material_bytes > self.max_contract_bytes {
            return Err("provider did not offer this complete material capacity".into());
        }
        let manifest = enrollment.terms.material_manifest.as_ref().ok_or("material enrollment lacks its exact signed manifest")?;
        if manifest.root != enrollment.terms.root {
            return Err("material manifest obligation was substituted".into());
        }
        let candidate = self.store.provider_admit_material_v1(
            enrollment.id(),
            manifest,
            enrollment.terms.max_material_bytes,
            self.total_capacity,
            held,
            positions,
            segmented,
            ledger,
            authority,
        )?;
        self.publish(candidate)?;
        Ok(enrollment.id())
    }
    pub fn answer_material(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        demand: kaspa_consensus_core::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1,
    ) -> Result<PalwMonetaryMaterialActionV1, String> {
        self.answer_material_owned_v1(consent, root, dependency, provider, demand)?
            .into_unreserved_legacy_v1()
            .map_err(|e| e.to_string())
    }
    pub fn answer_material_owned_v1(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        demand: kaspa_consensus_core::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwMonetaryMaterialActionV1>, String> {
        if self.store.material_generation_scope_v4(consent, root, dependency).is_some_and(|(_, scope)| {
            scope.codec == kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldCompleteV2
        }) {
            return Err("V2 routed material requires its fresh native authority and explicit V2 response API".into());
        }
        // Every retained generation is examined, not only the first candidate: any kernel-codec generation of this
        // root/dependency must go through the current native-positive replay path (`answer_material_current_v4`).
        if self.contexts.is_some()
            && self.store.material_generation_scopes_v4(consent, root, dependency).any(|(_, scope)| {
                scope.codec != kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCodecV1::NativeHeldComplete
            })
        {
            return Err("retained kernel material requires a current native-positive replay before answering".into());
        }
        self.store.provider_answer_action_v1(consent, root, dependency, provider, demand)
    }
    /// Try retained candidates starting with the preferred consent. Every
    /// candidate needs current native generation/provider/exact-scope authority
    /// before and after assembly. Kernel data additionally replays its intrinsic
    /// context against the current signed covenant. Assembly refusal stops the read.
    pub fn answer_material_current_v4(
        &self,
        preferred: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        pubkey: &[u8],
        demand: kaspa_consensus_core::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1,
        native: &dyn kaspa_consensus_core::api::ConsensusApi,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwMonetaryMaterialActionV1>, String> {
        self.answer_material_generation_v4(preferred, root, dependency, provider, pubkey, demand, native, false)
    }
    /// A read request names one signed generation. Unlike court recovery, it
    /// cannot silently answer under a different consent with equal scope bytes.
    pub fn answer_material_exact_current_v4(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        pubkey: &[u8],
        demand: kaspa_consensus_core::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1,
        native: &dyn kaspa_consensus_core::api::ConsensusApi,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwMonetaryMaterialActionV1>, String> {
        self.answer_material_generation_v4(consent, root, dependency, provider, pubkey, demand, native, true)
    }
    fn answer_material_generation_v4(
        &self,
        preferred: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        pubkey: &[u8],
        demand: kaspa_consensus_core::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1,
        native: &dyn kaspa_consensus_core::api::ConsensusApi,
        exact: bool,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwMonetaryMaterialActionV1>, String> {
        use kaspa_consensus_core::{
            palw_attempt_material_v3::verify_ready_signature_v3, palw_kernel_material_context_v4::restore_kernel_material_context_v4,
            palw_material_capacity_v1::PalwMaterialCodecV1,
        };
        self.answer_current_generation_with_v4(
            preferred,
            root,
            dependency,
            exact,
            |consent, scope| native.palw_material_generation_authority_checked_v4(root, consent, provider, scope),
            |consent, scope| {
                if scope.codec == PalwMaterialCodecV1::NativeHeldCompleteV2 {
                    return Err("V2 routed generation requires its explicit own-authority response API".into());
                }
                if scope.codec != PalwMaterialCodecV1::NativeHeldComplete {
                    let archive = self.contexts.as_ref().ok_or("kernel answers need explicit durable context capacities")?;
                    let record = archive.get(consent)?.ok_or("kernel intrinsic context absent")?;
                    if record.context.intent.manifest.scope(dependency) != Some(scope) {
                        return Err("kernel stored generation scope differs from context".into());
                    }
                    let bundle = self.store.provider_kernel_bundle_owned_v4(consent, dependency)?;
                    let _candidate = restore_kernel_material_context_v4(
                        &record.context,
                        &bundle,
                        archive.limits(),
                        self.max_contract_bytes,
                        |context| native.palw_kernel_material_context_authority_v4(context, &record.capsule, provider, pubkey),
                        verify_ready_signature_v3,
                    )?;
                    // Retain both context and replay tickets while the response
                    // owner is built. Its returned bytes carry their own lease.
                    return self.store.provider_answer_action_v1(consent, root, dependency, provider, demand);
                }
                self.store.provider_answer_action_v1(consent, root, dependency, provider, demand)
            },
        )
    }
    // Only a denied initial native authority lookup permits trying another
    // retained generation. In particular, local allocation / replay / assembly
    // refusals must not turn into a successful fallback under another consent.
    fn answer_current_generation_with_v4<T>(
        &self,
        preferred: Hash64,
        root: Hash64,
        dependency: Hash64,
        exact: bool,
        mut authority: impl FnMut(
            Hash64,
            &kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialScopeV1,
        ) -> Result<(), kaspa_consensus_core::palw_kernel_material_enrollment_v4::PalwMaterialGenerationAuthorityErrorV4>,
        mut build: impl FnMut(Hash64, &kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialScopeV1) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut last = String::from("no current eligible retained material generation");
        for (consent, scope) in self.store.material_generation_scopes_v4(preferred, root, dependency) {
            if exact && consent != preferred {
                continue;
            }
            if let Err(reason) = authority(consent, scope) {
                match reason {
                    kaspa_consensus_core::palw_kernel_material_enrollment_v4::PalwMaterialGenerationAuthorityErrorV4::Denied(reason) => {
                        last = reason;
                        continue;
                    }
                    reason => return Err(reason.to_string()),
                }
            }
            let answer = build(consent, scope)?;
            // Assembly can outlive the authority read across a reorg. Keep the
            // returned owner (and its allocation ticket) through this recheck;
            // a changed generation drops it before any bytes leave this API.
            authority(consent, scope).map_err(|why| why.to_string())?;
            return Ok(answer);
        }
        Err(last)
    }

    pub fn open(
        file: &Path,
        max_contract_bytes: u64,
        total_capacity: u64,
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
    ) -> Result<Self, String> {
        Self::open_inner(file, max_contract_bytes, total_capacity, ledger, authority, None)
    }
    pub fn open_with_kernel_contexts_v4(
        file: &Path,
        max_contract_bytes: u64,
        total_capacity: u64,
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
        limits: kaspa_consensus_core::palw_kernel_material_context_v4::PalwKernelMaterialContextLimitsV4,
    ) -> Result<Self, String> {
        let archive = crate::kernel_material_context_v4::KernelMaterialContextArchiveV4::open(
            &file.with_extension("kernel-context-v4"),
            limits,
        )?;
        Self::open_inner(file, max_contract_bytes, total_capacity, None, authority, Some(archive))
    }
    fn open_inner(
        file: &Path,
        max_contract_bytes: u64,
        total_capacity: u64,
        ledger: Option<&misaka_palw_kernel::ledger::KernelLedgerV1>,
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
        contexts: Option<crate::kernel_material_context_v4::KernelMaterialContextArchiveV4>,
    ) -> Result<Self, String> {
        if max_contract_bytes == 0 || total_capacity < max_contract_bytes {
            return Err("provider material capacity is invalid".into());
        }
        let parent = file.parent().ok_or("provider file has no parent")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = file.file_name().and_then(|name| name.to_str()).ok_or("provider file name is not UTF-8")?;
        let lease = acquire_provider_lease(&parent.join(format!("{name}.lock")))?;
        let store = match File::open(file) {
            Ok(source) => {
                let limit = total_capacity.checked_add(1 << 20).ok_or("provider persistence cap overflow")?;
                if source.metadata().map_err(|e| e.to_string())?.len() > limit {
                    return Err("provider snapshot exceeds its byte cap".into());
                }
                let expected = source.metadata().map_err(|e| e.to_string())?.len();
                let _raw_lease = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(
                    expected.checked_add(1).and_then(|n| n.checked_mul(2)).ok_or("provider raw snapshot request overflow")?,
                )
                .map_err(|e| e.to_string())?;
                let mut bytes = Vec::new();
                source.take(expected + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                if bytes.len() as u64 != expected {
                    return Err("provider snapshot changed extent during read".into());
                }
                if let Some(archive) = contexts.as_ref() {
                    kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1::<PalwMonetaryMaterialProviderStoreV1>::recover_provider_routed_v2(&bytes,max_contract_bytes,total_capacity,None,authority,|claim|load_routed_authority_v2(file,claim,max_contract_bytes),Some(&mut |consent,scope,_|{
                        let record=archive.get(consent)?.ok_or("kernel retention context missing; no answers authorized")?;
                        let (monetary,identity,_)=kaspa_consensus_core::palw_kernel_material_enrollment_v4::kernel_material_capsule_identity_v4(&record.capsule)?;
                        let length=borsh::object_length(&record.capsule).map_err(|e|e.to_string())? as u64;
                        let _scratch=kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(length.checked_mul(4).ok_or("kernel retention identity scratch overflow")?).map_err(|e|e.to_string())?;
                        if identity!=consent || monetary.terms.material_manifest.as_ref()!=Some(&record.context.intent.manifest) || record.context.intent.manifest.scope(scope.claim)!=Some(scope){return Err("kernel retained bytes substitute exact saved identity".into())}
                        // Retention only: this local identity cannot authorize a
                        // response, GC, funding or a native accepted claim.
                        Ok(())
                    })).map_err(|e|e.to_string())?
                } else {
                    // V2 metadata authenticates retained identity only. Current
                    // native authority is still required by the serving API.
                    kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1::<PalwMonetaryMaterialProviderStoreV1>::recover_provider_routed_v2(
                        &bytes,max_contract_bytes,total_capacity,ledger,authority,|claim|load_routed_authority_v2(file,claim,max_contract_bytes),None).map_err(|e|e.to_string())?
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1::retain_existing_v1(
                    PalwMonetaryMaterialProviderStoreV1::default(),
                )
                .map_err(|e| e.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        };
        // Pending snapshots were never signature-authorized: the signing API
        // returns only after atomic publication. With the kernel lease held,
        // these exact owned names cannot belong to a concurrent live writer.
        let prefix = format!("{name}.pending-");
        for entry in fs::read_dir(parent).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix(&prefix))
                .is_some_and(|suffix| suffix.parse::<u32>().is_ok())
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        let opened =
            Self { _lease: lease, file: file.to_path_buf(), max_contract_bytes, total_capacity, store, contexts, expiry_cursor: None };
        opened.sweep_routed_authorities_v2().map_err(|e| e.to_string())?;
        Ok(opened)
    }
    fn publish(
        &mut self,
        candidate: kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwMonetaryMaterialProviderStoreV1>,
    ) -> Result<(), String> {
        let encoded = candidate.provider_encoded_v1()?;
        let parent = self.file.parent().ok_or("provider file has no parent directory")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let name = self.file.file_name().and_then(|name| name.to_str()).ok_or("provider filename is not UTF-8")?;
        let temporary = parent.join(format!("{name}.pending-{}", std::process::id()));
        // Never overwrite another writer's pending generation. Failure retains
        // the old memory/durable generation and does not authorize a signature.
        let mut output = OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|e| e.to_string())?;
        let result = (|| {
            output.write_all(&encoded)?;
            output.sync_all()?;
            fs::rename(&temporary, &self.file)?;
            File::open(parent)?.sync_all()?;
            Ok::<_, std::io::Error>(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(temporary);
            return Err(error.to_string());
        }
        self.store = candidate;
        self.sweep_routed_authorities_v2().map_err(|e| e.to_string())?;
        Ok(())
    }
    /// Call before signing the enrollment. The complete canonical material and
    /// exact signed scope must be durable before a provider accepts liability.
    pub fn prepare_held(
        &mut self,
        provider: PalwBondKeyV2,
        enrollment: &PalwMonetaryMaterialEnrollmentV1,
        bundles: &[PalwHeldMaterialBundleV1],
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
    ) -> Result<Hash64, String> {
        if !enrollment.offers.iter().any(|offer| offer.provider == provider)
            || enrollment.terms.max_material_bytes > self.max_contract_bytes
        {
            return Err("provider did not offer this signed material capacity".into());
        }
        let manifest: &PalwMaterialManifestV1 =
            enrollment.terms.material_manifest.as_ref().ok_or("signed enrollment lacks a material manifest")?;
        let candidate = self.store.provider_admit_material_v1(
            enrollment.id(),
            manifest,
            enrollment.terms.max_material_bytes,
            self.total_capacity,
            bundles,
            &[],
            &[],
            None,
            authority,
        )?;
        self.publish(candidate)?;
        Ok(enrollment.id())
    }
    pub fn answer_held(
        &self,
        consent: Hash64,
        root: Hash64,
        dependency: Hash64,
        provider: PalwBondKeyV2,
        missing: kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1,
    ) -> Result<PalwMonetaryMaterialActionV1, String> {
        self.answer_material_owned_v1(
            consent,
            root,
            dependency,
            provider,
            kaspa_consensus_core::palw_material_capacity_v1::PalwMonetaryMaterialDemandV1::Held(missing),
        )?
        .into_unreserved_legacy_v1()
        .map_err(|e| e.to_string())
    }
    pub fn prepare_replacement_held(
        &mut self,
        provider: PalwBondKeyV2,
        replacement: &kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialReplacementV1,
        bundles: &[PalwHeldMaterialBundleV1],
        authority: impl FnMut(Hash64) -> Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialNativeAuthorityV1>,
    ) -> Result<Hash64, String> {
        let e = &replacement.enrollment;
        if replacement.root != e.terms.root
            || !e.offers.iter().any(|o| o.provider == provider)
            || e.terms.max_material_bytes > self.max_contract_bytes
        {
            return Err("replacement substituted its provider/root/capacity".into());
        }
        let manifest = e.terms.material_manifest.as_ref().ok_or("replacement lacks its exact material manifest")?;
        let candidate = self.store.provider_admit_material_v1(
            replacement.id(),
            manifest,
            e.terms.max_material_bytes,
            self.total_capacity,
            bundles,
            &[],
            &[],
            None,
            authority,
        )?;
        self.publish(candidate)?;
        Ok(replacement.id())
    }
    /// `(stored consent, root)` of every retained obligation (read-only; see `monetary_material_gc`).
    pub fn held_native_authorities(&self) -> std::collections::BTreeMap<Hash64, std::collections::BTreeSet<Hash64>> {
        self.store.held_native_authorities_v1()
    }
    pub fn held_contracts(&self) -> Vec<(Hash64, Hash64)> {
        self.store.held_contracts_v1()
    }
    /// The snapshot file this store publishes to (the janitor keeps its journal and intent marker beside it).
    pub fn file(&self) -> &Path {
        &self.file
    }
    pub fn gc_finalized(
        &mut self,
        root: Hash64,
        stored_consent: Hash64,
        release_consent: Hash64,
        tail: Hash64,
        release_block: kaspa_consensus_core::BlockHash,
        native_witness: impl FnMut(Hash64, Hash64, Hash64, kaspa_consensus_core::BlockHash) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let (candidate, released) =
            self.store.provider_release_finalized_v1(root, stored_consent, release_consent, tail, release_block, native_witness)?;
        if !released {
            return Ok(false);
        }
        self.publish(candidate)?;
        if let Some(archive) = self.contexts.as_mut() {
            archive.remove_after_verified_gc(stored_consent)?;
        }
        Ok(true)
    }
    /// WAL replay may observe raw material already deleted. Recheck the same
    /// positive native witness before deleting a remaining context, and finish
    /// this step before recording completion or signing the provider ACK.
    pub(crate) fn complete_metadata_gc_v4(
        &mut self,
        root: Hash64,
        stored: Hash64,
        consent: Hash64,
        tail: Hash64,
        block: kaspa_consensus_core::BlockHash,
        mut witness: impl FnMut(Hash64, Hash64, Hash64, kaspa_consensus_core::BlockHash) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let Some(archive) = self.contexts.as_mut() else { return Ok(true) };
        if archive.get(stored)?.is_none() {
            return Ok(true);
        }
        if self.store.holds_material_consent_v4(stored) {
            return Ok(false);
        }
        if !witness(root, consent, tail, block)? {
            return Ok(false);
        }
        archive.remove_after_verified_gc(stored)?;
        Ok(true)
    }
    /// One bounded round-robin metadata row per tick; no first-live row can
    /// starve later expired offers. A failed read leaves its WAL and bytes held.
    pub fn expire_unused_kernel_offer_tick_v4(&mut self, native: &dyn kaspa_consensus_core::api::ConsensusApi) -> Result<(), String> {
        let Some(archive) = self.contexts.as_ref() else { return Ok(()) };
        let pending = archive.pending_expiry();
        let Some(consent) = pending.map(|(id, _, _)| id).or_else(|| archive.next_consent(self.expiry_cursor)) else { return Ok(()) };
        self.expiry_cursor = Some(consent);
        let record = archive.get(consent)?;
        let Some(record) = record else {
            // A durable expiry intent is published before deleting raw bytes,
            // and metadata is deleted only after raw publication. Missing
            // metadata can finish that intent, never authorize a fresh delete.
            if pending.is_none() || self.store.holds_material_consent_v4(consent) {
                return Err("expiry metadata missing while material remains".into());
            }
            self.contexts.as_mut().unwrap().finish_expiry(consent)?;
            return Ok(());
        };
        let (enrollment, id, until) =
            kaspa_consensus_core::palw_kernel_material_enrollment_v4::kernel_material_capsule_identity_v4(&record.capsule)?;
        let root = enrollment.terms.root;
        if id != consent || pending.is_some_and(|(_, r, u)| r != root || u != until) {
            return Err("kernel expiry intent substitutes exact signed generation/deadline".into());
        }
        if !native.palw_kernel_material_capsule_unused_finalized_v4(&record.capsule)? {
            return Ok(());
        }
        self.contexts.as_mut().unwrap().begin_expiry(consent, root, until)?;
        if self.store.holds_material_consent_v4(consent) {
            let (candidate, expired) = self.store.provider_discard_expired_kernel_capsule_v4(&record.capsule, |capsule| {
                native.palw_kernel_material_capsule_unused_finalized_v4(capsule)
            })?;
            if !expired {
                return Ok(());
            }
            self.publish(candidate)?;
        }
        let archive = self.contexts.as_mut().unwrap();
        archive.remove_after_verified_gc(consent)?;
        archive.finish_expiry(consent)?;
        Ok(())
    }
    pub fn discard_expired_offer(
        &mut self,
        enrollment: &PalwMonetaryMaterialEnrollmentV1,
        native_expiry: impl FnMut(Hash64, u64) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let (candidate, expired) = self.store.provider_discard_expired_offer_v1(enrollment, native_expiry)?;
        if !expired {
            return Ok(false);
        }
        if let Some(archive) = self.contexts.as_mut() {
            archive.begin_expiry(enrollment.id(), enrollment.terms.root, enrollment.terms.enrollment_until_daa)?;
        }
        self.publish(candidate)?;
        if let Some(archive) = self.contexts.as_mut() {
            archive.remove_after_verified_gc(enrollment.id())?;
            archive.finish_expiry(enrollment.id())?;
        }
        Ok(true)
    }
}

#[cfg(unix)]
pub(crate) fn acquire_provider_lease(path: &Path) -> Result<File, String> {
    use std::os::fd::AsRawFd;
    let lease = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).map_err(|e| e.to_string())?;
    // SAFETY: flock uses the live File's descriptor; no pointers or aliasing.
    if unsafe { libc::flock(lease.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!("provider snapshot is already leased or unavailable: {}", std::io::Error::last_os_error()));
    }
    Ok(lease)
}
#[cfg(not(unix))]
pub(crate) fn acquire_provider_lease(_path: &Path) -> Result<File, String> {
    Err("durable monetary material provider needs a kernel file lease on this platform".into())
}

fn routed_authority_path_v2(file: &Path, claim: Hash64) -> PathBuf {
    file.with_extension("routing-authorities-v2").join(format!("{claim}.v2"))
}
fn load_routed_authority_v2(
    file: &Path,
    claim: Hash64,
    max_bytes: u64,
) -> Result<
    kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<
        kaspa_consensus_core::palw_routed_material_v2::PalwMaterialAuthorityRecordV2,
    >,
    kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1,
> {
    use kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E;
    use kaspa_consensus_core::palw_native_resource_v1::{
        reserve_native_requests_v1, retain_native_value_decode_classified_v1, NativeDecodeErrorV1,
    };
    let source = File::open(routed_authority_path_v2(file, claim)).map_err(|e| E::Authentication(e.to_string()))?;
    let n = source.metadata().map_err(|e| E::Authentication(e.to_string()))?.len();
    if n > max_bytes || n == 0 {
        return Err(E::Capacity);
    }
    let _raw =
        reserve_native_requests_v1(n.checked_add(1).and_then(|n| n.checked_mul(2)).ok_or(E::Capacity)?).map_err(E::LocalResource)?;
    let mut raw = Vec::with_capacity(usize::try_from(n.checked_add(1).ok_or(E::Capacity)?).map_err(|_| E::Capacity)?);
    source.take(n + 1).read_to_end(&mut raw).map_err(|e| E::Authentication(e.to_string()))?;
    if raw.len() as u64 != n {
        return Err(E::Substitution);
    }
    let record = retain_native_value_decode_classified_v1::<
        kaspa_consensus_core::palw_routed_material_v2::PalwMaterialAuthorityRecordV2,
        std::io::Error,
    >(&raw, || borsh::from_slice(&raw))
    .map_err(|e| match e {
        NativeDecodeErrorV1::Resource(e) => E::LocalResource(e),
        NativeDecodeErrorV1::Decode(e) => E::Authentication(e.to_string()),
    })?;
    if record.version != 2 || record.claim != claim {
        return Err(E::Substitution);
    }
    Ok(record)
}
fn save_routed_authority_v2(
    file: &Path,
    record: &kaspa_consensus_core::palw_routed_material_v2::PalwMaterialAuthorityRecordV2,
    max_bytes: u64,
) -> Result<(), kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1> {
    use kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E;
    let target = routed_authority_path_v2(file, record.claim);
    if target.exists() {
        if &*load_routed_authority_v2(file, record.claim, max_bytes)? != record {
            return Err(E::Substitution);
        }
        return Ok(());
    }
    let parent = target.parent().ok_or(E::Incomplete)?;
    fs::create_dir_all(parent).map_err(|e| E::Authentication(e.to_string()))?;
    if let Some(ancestor) = parent.parent() {
        File::open(ancestor).and_then(|d| d.sync_all()).map_err(|e| E::Authentication(e.to_string()))?;
    }
    let cap = kaspa_consensus_core::palw_monetary_material_v1::PALW_MONETARY_MATERIAL_MAX_CONTRACTS_V1 as usize
        * kaspa_consensus_core::palw_material_capacity_v1::PALW_MATERIAL_MAX_SCOPES_V1;
    let mut count = 0usize;
    for entry in fs::read_dir(parent).map_err(|e| E::Authentication(e.to_string()))? {
        let entry = entry.map_err(|e| E::Authentication(e.to_string()))?;
        if entry.path().extension().is_some_and(|v| v == "v2") {
            count += 1;
        }
        if count >= cap {
            return Err(E::Capacity);
        }
    }
    let n = borsh::object_length(record).map_err(|_| E::Capacity)? as u64;
    if n > max_bytes {
        return Err(E::Capacity);
    }
    let _raw = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(n.checked_mul(2).ok_or(E::Capacity)?)
        .map_err(E::LocalResource)?;
    let raw = borsh::to_vec(record).map_err(|e| E::Authentication(e.to_string()))?;
    atomic_routed_file_v2(&target, &raw)
}
fn atomic_routed_file_v2(
    target: &Path,
    bytes: &[u8],
) -> Result<(), kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1> {
    use kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E;
    let parent = target.parent().ok_or(E::Incomplete)?;
    fs::create_dir_all(parent).map_err(|e| E::Authentication(e.to_string()))?;
    let name = target.file_name().and_then(|n| n.to_str()).ok_or(E::Incomplete)?;
    let pending = parent.join(format!("{name}.routed-pending-{}", std::process::id()));
    let result = (|| {
        let mut output = OpenOptions::new().write(true).create_new(true).open(&pending)?;
        output.write_all(bytes)?;
        output.sync_all()?;
        fs::rename(&pending, target)?;
        File::open(parent)?.sync_all()?;
        Ok::<_, std::io::Error>(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(pending);
        return Err(E::Authentication(e.to_string()));
    }
    Ok(())
}

#[cfg(test)]
#[path = "monetary_material_current_tests.rs"]
mod current_generation_tests;
