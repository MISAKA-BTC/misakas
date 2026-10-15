//! Native-policy-funded kernel material ingress. The prospective verification
//! owner is retained through durable provider publication and liability signing.
//! Neither this draft nor a transport success is native acceptance.
use crate::{
    Hash64,
    palw_material_release_recovery_v2::{PalwMaterialRecoveryActionV2, PalwMaterialRecoverySignatureV2, PalwMaterialRecoveryTermsV2},
    palw_monetary_material_v1::{PalwMonetaryMaterialEnrollmentV1, PalwMonetaryMaterialOfferV1, PalwMonetaryMaterialTermsV1},
    palw_native_resource_v1::{NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_prospective_kernel_material_v4::{PalwProspectiveKernelMaterialOwnedV4, PalwProspectiveKernelMaterialRequestV4},
    palw_state_v2::{PalwChainStateV2, PalwStateParamsV2, palw_bond_committed_raw_v1},
};
use borsh::BorshSerialize;
pub struct PalwKernelMaterialIngressOwnedV4 {
    pub verified: PalwProspectiveKernelMaterialOwnedV4,
    pub enrollment: NativeRetainedV1<PalwMaterialRecoveryActionV2>,
    pub network: Hash64,
    pub context: NativeRetainedV1<crate::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4>,
}
fn hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Result<Hash64, String> {
    struct Sink(blake2b_simd::State);
    impl std::io::Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let len = borsh::object_length(value).map_err(|e| e.to_string())? as u64;
    let mut sink = Sink(misaka_palw_kernel::hash::keyed(domain));
    sink.0.update(&len.to_le_bytes());
    borsh::to_writer(&mut sink, value).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(misaka_palw_kernel::hash::finish(sink.0)))
}
pub fn kernel_material_enrollment_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    raw_depth: Option<u64>,
    request: &PalwProspectiveKernelMaterialRequestV4,
    verified: PalwProspectiveKernelMaterialOwnedV4,
) -> Result<PalwKernelMaterialIngressOwnedV4, String> {
    let local = crate::palw_native_resource_v1::native_declared_logical_limits_v1()
        .ok_or("kernel ingress needs explicitly configured local metadata RAM/DB capacity")?;
    let providers = u64::try_from(request.intent.providers.len()).map_err(|_| "kernel roster count overflow")?;
    if providers == 0 {
        return Err("kernel ingress provider roster empty".into());
    }
    let context = verified.export_context_v4(
        state,
        request,
        crate::palw_kernel_material_context_v4::PalwKernelMaterialContextLimitsV4 {
            max_encoded_bytes: local.database_bytes / providers / 2,
            max_heap_bytes: local.memory_bytes / providers,
        },
    )?;
    let policy = params.monetary_material_at_v1(now).ok_or("kernel monetary fence dormant")?;
    let recovery = params.material_release_recovery_at_v2(now).ok_or("kernel recovery fence dormant")?;
    policy.validate().map_err(str::to_string)?;
    recovery.validate().map_err(str::to_string)?;
    let a = verified.authority();
    if a.root != request.intent.manifest.root || verified.request_id() != request.id() {
        return Err("kernel enrollment request authority differs".into());
    }
    let n = request.intent.providers.len() as u64;
    let fees = policy.minimum_fee_sompi.checked_mul(n).ok_or("kernel provider funding overflows")?;
    let sponsor = state.bond(&a.producer).ok_or("kernel sponsor absent")?;
    if u128::from(sponsor.collateral).saturating_sub(palw_bond_committed_raw_v1(state, params, &a.producer, now, raw_depth))
        < u128::from(fees)
    {
        return Err("kernel sponsor has no own unused fee funding".into());
    }
    for provider in &request.intent.providers {
        let row = state.bond(provider).ok_or("kernel provider absent")?;
        if row.operator_id == sponsor.operator_id
            || u128::from(row.collateral).saturating_sub(palw_bond_committed_raw_v1(state, params, provider, now, raw_depth))
                < u128::from(policy.minimum_collateral_sompi)
        {
            return Err("kernel provider is dependent or has no free collateral".into());
        }
    }
    // The producer signed the immutable deadline. Re-reading the native DAA
    // clock must never manufacture different terms between the two rounds.
    let until = request.intent.expires_daa;
    if until < now || now.checked_add(policy.max_enrollment_daa).is_none_or(|limit| until > limit) {
        return Err("kernel enrollment is expired".into());
    }
    let manifest = verified.manifest();
    let max_material_bytes = manifest.validate(policy.max_bytes_per_contract).map_err(|e| e.to_string())?;
    let network = Hash64::from_bytes(state.kernel_route().ok_or("kernel route absent")?.header.policy.network_domain);
    let nonce = hash(b"MISAKA/PALW/KERNEL-RECOVERY-NONCE/V4", &(network, request.id(), policy.id(), recovery.id()))?;
    let heap = manifest.native_heap_v1("kernel enrollment manifest").bytes().ok_or("kernel manifest native size unknown")?;
    let rows = n
        .checked_mul(
            (std::mem::size_of::<PalwMonetaryMaterialOfferV1>()
                + std::mem::size_of::<PalwMaterialRecoverySignatureV2>()
                + std::mem::size_of::<(crate::palw_state_v2::PalwBondKeyV2, u64, u64)>()) as u64,
        )
        .ok_or("kernel enrollment rows overflow")?;
    let payload = (borsh::object_length(manifest).map_err(|e| e.to_string())? as u64)
        .checked_add(std::mem::size_of::<PalwMonetaryMaterialTermsV1>() as u64)
        .and_then(|v| v.checked_add(n * 84 + 128))
        .ok_or("kernel enrollment payload overflow")?;
    let lease = reserve_native_requests_v1(
        heap.checked_add(rows)
            .and_then(|v| payload.checked_mul(4).and_then(|p| v.checked_add(p)))
            .and_then(|v| {
                v.checked_add(
                    (std::mem::size_of::<PalwMaterialRecoveryActionV2>() + std::mem::size_of::<PalwMonetaryMaterialEnrollmentV1>())
                        as u64,
                )
            })
            .ok_or("kernel enrollment native size overflow")?,
    )
    .map_err(|e| e.to_string())?;
    let monetary = PalwMonetaryMaterialEnrollmentV1 {
        terms: PalwMonetaryMaterialTermsV1 {
            version: 1,
            root: a.root,
            dependencies: vec![],
            exec_plan_root: None,
            sponsor: a.producer,
            nonce,
            policy: policy.id(),
            max_material_bytes,
            material_manifest: Some(manifest.clone()),
            native_session: None,
            window_daa: policy.window_daa,
            enrollment_until_daa: until,
            payment_relative_until_finality: true,
        },
        offers: request
            .intent
            .providers
            .iter()
            .map(|provider| PalwMonetaryMaterialOfferV1 {
                provider: *provider,
                collateral_sompi: policy.minimum_collateral_sompi,
                prepaid_fee_sompi: policy.minimum_fee_sompi,
                signature: vec![],
            })
            .collect(),
    };
    let terms = PalwMaterialRecoveryTermsV2 {
        version: 2,
        root: a.root,
        initial_consent: monetary.id(),
        sponsor: a.producer,
        nonce,
        policy: recovery.id(),
        max_generations: recovery.max_generations_per_contract,
        all_generation_ack: true,
    };
    let enrollment = NativeRetainedV1::from_owner_reserved_v1(
        PalwMaterialRecoveryActionV2::FundedEnroll {
            monetary: Box::new(monetary),
            terms,
            providers: request
                .intent
                .providers
                .iter()
                .map(|provider| PalwMaterialRecoverySignatureV2 { provider: *provider, signature: vec![] })
                .collect(),
            sponsor_signature: vec![],
        },
        lease,
    );
    Ok(PalwKernelMaterialIngressOwnedV4 { verified, enrollment, network, context })
}

/// Identity authentication only. This does not establish computation correctness
/// or native acceptance. The caller supplies its actual configured provider key.
pub fn verify_kernel_context_capsule_v4(
    context: &crate::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
    capsule: &PalwMaterialRecoveryActionV2,
    provider: &crate::palw_state_v2::PalwBondKeyV2,
    pubkey: &[u8],
    network: Hash64,
    verify: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<(Hash64, Hash64), String> {
    use crate::palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1 as DOMAIN, palw_provider_court_message_v1 as message};
    let PalwMaterialRecoveryActionV2::FundedEnroll { monetary, terms, providers, .. } = capsule else {
        return Err("kernel context has no funded identity capsule".into());
    };
    let n = borsh::object_length(capsule).map_err(|e| e.to_string())? as u64;
    let _scratch = reserve_native_requests_v1(n.max(1024).checked_mul(6).ok_or("kernel capsule identity scratch overflow")?)
        .map_err(|e| e.to_string())?;
    let intent = &context.intent;
    if context.version != 4
        || monetary.terms.root != intent.manifest.root
        || monetary.terms.material_manifest.as_ref() != Some(&intent.manifest)
        || monetary.terms.native_session.is_some()
        || monetary.terms.sponsor != intent.producer
        || terms.root != monetary.terms.root
        || terms.initial_consent != monetary.id()
        || terms.sponsor != intent.producer
        || terms.nonce != monetary.terms.nonce
        || monetary.terms.nonce
            != hash(b"MISAKA/PALW/KERNEL-RECOVERY-NONCE/V4", &(network, context.request, monetary.terms.policy, terms.policy))?
        || providers.iter().map(|s| &s.provider).ne(intent.providers.iter())
        || monetary.offers.iter().map(|o| &o.provider).ne(intent.providers.iter())
    {
        return Err("kernel context capsule substitutes signed work/nonce/manifest/roster".into());
    }
    let offer = monetary.offers.iter().find(|p| &p.provider == provider).ok_or("kernel context provider did not consent")?;
    let recovery = providers.iter().find(|p| &p.provider == provider).ok_or("kernel context recovery consent absent")?;
    let ordinary = monetary.payload();
    let recovery_payload = capsule.payload();
    if !verify(pubkey, message(network, 240, provider, &ordinary).as_byte_slice(), &offer.signature, DOMAIN)
        || !verify(pubkey, message(network, 242, provider, &recovery_payload).as_byte_slice(), &recovery.signature, DOMAIN)
        || !verify(
            &context.producer_pubkey,
            intent.signing_message(network).as_byte_slice(),
            &context.producer_signature,
            crate::palw_prospective_kernel_material_v4::PALW_PROSPECTIVE_KERNEL_MATERIAL_CONTEXT_V4,
        )
    {
        return Err("kernel context original producer/provider signatures invalid".into());
    }
    Ok((monetary.id(), hash(b"MISAKA/PALW/RELEASE-RECOVERY-CONSENT/V2", terms)?))
}
/// Only a current positive native contract/generation authorizes replay for an
/// answer. Absence, a saved checkpoint and a local completion flag never do.
pub fn require_kernel_context_current_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    context: &crate::palw_kernel_material_context_v4::PalwProspectiveKernelMaterialContextV4,
    capsule: &PalwMaterialRecoveryActionV2,
    provider: &crate::palw_state_v2::PalwBondKeyV2,
    pubkey: &[u8],
    verify: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<(), String> {
    use crate::{
        palw_material_release_recovery_v2::{PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, PalwMaterialRecoveryRowV2},
        palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1},
        palw_native_resource_v1::{preflight_native_value_v1, retain_native_value_decode_v1},
    };
    if params.material_release_recovery_at_v2(now).is_none() {
        return Err("kernel context recovery fence dormant".into());
    }
    let route = state.kernel_route().ok_or("kernel native route absent")?;
    let network = Hash64::from_bytes(route.header.policy.network_domain);
    let native = state.bond(provider).ok_or("kernel provider native bond unavailable")?;
    if native.pubkey.as_slice() != pubkey {
        return Err("kernel context provider key differs from native bond".into());
    }
    let (consent, recovery) = verify_kernel_context_capsule_v4(context, capsule, provider, pubkey, network, verify)?;
    let root = context.intent.manifest.root;
    let encoded = |table| {
        route
            .aux
            .iter()
            .find(|((t, k), _)| *t == table && k.as_slice() == root.as_byte_slice())
            .map(|(_, b)| b.as_slice())
            .ok_or("current positive kernel contract absent")
    };
    let raw = encoded(PALW_MONETARY_MATERIAL_TABLE_V1)?;
    preflight_native_value_v1::<PalwMonetaryMaterialRowV1>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let monetary =
        retain_native_value_decode_v1(raw, || borsh::from_slice::<PalwMonetaryMaterialRowV1>(raw)).map_err(|e| e.to_string())?;
    let PalwMaterialRecoveryActionV2::FundedEnroll { monetary: original, .. } = capsule else { unreachable!() };
    if monetary.terms != original.terms || !monetary.providers.iter().any(|p| p.provider == *provider && !p.charged) {
        return Err("current kernel covenant terms/provider differ".into());
    }
    let raw = encoded(PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2)?;
    preflight_native_value_v1::<PalwMaterialRecoveryRowV2>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let row = retain_native_value_decode_v1(raw, || borsh::from_slice::<PalwMaterialRecoveryRowV2>(raw)).map_err(|e| e.to_string())?;
    let PalwMaterialRecoveryRowV2::Active(contract) = &*row else {
        return Err("kernel context is terminal; no answers authorized".into());
    };
    if contract.id() != recovery || !contract.generations.iter().any(|g| g.consent == consent && g.providers.contains(provider)) {
        return Err("current kernel recovery does not cover stored provider generation".into());
    }
    Ok(())
}

pub fn require_material_generation_current_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    root: Hash64,
    consent: Hash64,
    provider: crate::palw_state_v2::PalwBondKeyV2,
    scope: &crate::palw_material_capacity_v1::PalwMaterialScopeV1,
) -> Result<(), String> {
    use crate::{
        palw_material_release_recovery_v2::{PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, PalwMaterialRecoveryRowV2},
        palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1},
        palw_native_resource_v1::{preflight_native_value_v1, retain_native_value_decode_v1},
    };
    if params.material_release_recovery_at_v2(now).is_none() {
        return Err("generation fallback requires explicit recovery enrollment".into());
    }
    let route = state.kernel_route().ok_or("generation native route absent")?;
    let encoded = |table| {
        route
            .aux
            .iter()
            .find(|((t, k), _)| *t == table && k.as_slice() == root.as_byte_slice())
            .map(|(_, b)| b.as_slice())
            .ok_or("current material generation absent")
    };
    let raw = encoded(PALW_MONETARY_MATERIAL_TABLE_V1)?;
    preflight_native_value_v1::<PalwMonetaryMaterialRowV1>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let monetary =
        retain_native_value_decode_v1(raw, || borsh::from_slice::<PalwMonetaryMaterialRowV1>(raw)).map_err(|e| e.to_string())?;
    let manifest = monetary.terms.material_manifest.as_ref().or_else(|| monetary.native_binding.as_ref().map(|b| &b.manifest));
    if monetary.terms.root != root
        || !monetary.providers.iter().any(|p| p.provider == provider && !p.charged)
        || manifest.and_then(|m| m.scope(scope.claim)) != Some(scope)
    {
        return Err("retained material differs from current native exact provider/scope".into());
    }
    let raw = encoded(PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2)?;
    preflight_native_value_v1::<PalwMaterialRecoveryRowV2>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let recovery =
        retain_native_value_decode_v1(raw, || borsh::from_slice::<PalwMaterialRecoveryRowV2>(raw)).map_err(|e| e.to_string())?;
    let PalwMaterialRecoveryRowV2::Active(contract) = &*recovery else { return Err("terminal generation cannot answer".into()) };
    if !contract.generations.iter().any(|g| g.consent == consent && g.providers.contains(&provider)) {
        return Err("retained provider generation is not currently enrolled".into());
    }
    Ok(())
}
