//! Explicitly bounded, newly funded provider replacement preparation. The own
//! native covenant authenticates the original work; new 241/242 signatures and
//! actual accepted generation are still required. No answer/credit authority is
//! issued by this preparatory identity replay.
use crate::{
    Hash64,
    palw_kernel_material_context_v4::{
        PalwKernelMaterialContextLimitsV4, PalwProspectiveKernelMaterialContextV4, restore_kernel_material_identity_v4,
    },
    palw_kernel_material_enrollment_v4::hash,
    palw_material_capacity_v1::PalwMaterialManifestV1,
    palw_material_release_recovery_v2::{
        PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, PalwMaterialRecoveryActionV2 as Action, PalwMaterialRecoveryRowV2,
        PalwMaterialRecoverySignatureV2,
    },
    palw_monetary_material_v1::{
        PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1, PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialEnrollmentV1,
        PalwMonetaryMaterialOfferV1, PalwMonetaryMaterialReplacementV1, PalwMonetaryMaterialRowV1,
    },
    palw_native_resource_v1::{
        NativeHeapV1, NativeRetainedV1, NativeWireScanV1, ReservedKernelLedgerV1, preflight_native_value_v1,
        reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_prospective_kernel_material_v4::PalwProspectiveKernelBundleV4,
    palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2, palw_bond_committed_raw_v1},
};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelMaterialReplacementIntentV4 {
    pub version: u16,
    pub root: Hash64,
    pub context_request: Hash64,
    pub previous: Hash64,
    pub sequence: u64,
    pub providers: Vec<PalwBondKeyV2>,
    pub valid_until_daa: u64,
}
/// Prepared material/publication owner only. Private fields prevent manufacturing
/// a verified candidate from an arbitrary wire ledger or successful transport.
pub struct PalwKernelMaterialReplacementOwnedV4 {
    pub proposal: NativeRetainedV1<Action>,
    pub network: Hash64,
    pub daa_score: u64,
    candidate: ReservedKernelLedgerV1,
    manifest: NativeRetainedV1<PalwMaterialManifestV1>,
    producer_key: NativeRetainedV1<Vec<u8>>,
    provider_keys: [Option<(PalwBondKeyV2, NativeRetainedV1<Vec<u8>>)>; 8],
}
impl PalwKernelMaterialReplacementOwnedV4 {
    pub fn candidate_ledger(&self) -> &misaka_palw_kernel::ledger::KernelLedgerV1 {
        &self.candidate
    }
    pub fn manifest(&self) -> &PalwMaterialManifestV1 {
        &self.manifest
    }
    pub fn producer_pubkey(&self) -> &[u8] {
        &self.producer_key
    }
    pub fn provider_pubkey(&self, key: &PalwBondKeyV2) -> Option<&[u8]> {
        self.provider_keys.iter().flatten().find(|(b, _)| b == key).map(|(_, k)| k.as_slice())
    }
}
fn row<T: NativeWireScanV1 + BorshDeserialize>(
    state: &PalwChainStateV2,
    table: u8,
    root: Hash64,
) -> Result<NativeRetainedV1<T>, String> {
    let route = state.kernel_route().ok_or("replacement native route absent")?;
    let bytes = route
        .aux
        .iter()
        .find(|((t, k), _)| *t == table && k.as_slice() == root.as_byte_slice())
        .map(|(_, b)| b.as_slice())
        .ok_or("replacement current row absent")?;
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(|e| e.to_string())?;
    retain_native_value_decode_v1(bytes, || borsh::from_slice::<T>(bytes)).map_err(|e| e.to_string())
}
pub fn kernel_material_replacement_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    raw_depth: Option<u64>,
    context: &PalwProspectiveKernelMaterialContextV4,
    bundle: &PalwProspectiveKernelBundleV4,
    intent: &PalwKernelMaterialReplacementIntentV4,
) -> Result<PalwKernelMaterialReplacementOwnedV4, String> {
    let p = params.monetary_material_at_v1(now).ok_or("replacement monetary fence dormant")?;
    let recovery = params.material_release_recovery_at_v2(now).ok_or("replacement recovery fence dormant")?;
    if intent.version != 4
        || intent.root != context.intent.manifest.root
        || intent.context_request != context.request
        || !(2..=8).contains(&intent.providers.len())
        || intent.providers.windows(2).any(|p| p[0] >= p[1])
        || intent.valid_until_daa < now
        || now.checked_add(p.max_enrollment_daa).is_none_or(|max| intent.valid_until_daa > max)
    {
        return Err("replacement intent identity/roster/signed lifetime differs".into());
    }
    crate::palw_material_release_recovery_v2::bounded_replacement_deadline_v4(now, intent.valid_until_daa, p.max_enrollment_daa)
        .map_err(str::to_string)?;
    let old = row::<PalwMonetaryMaterialRowV1>(state, PALW_MONETARY_MATERIAL_TABLE_V1, intent.root)?;
    let rr = row::<PalwMaterialRecoveryRowV2>(state, PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, intent.root)?;
    let PalwMaterialRecoveryRowV2::Active(contract) = &*rr else { return Err("replacement root already terminal".into()) };
    let route = state.kernel_route().ok_or("replacement route absent")?;
    let network = Hash64::from_bytes(route.header.policy.network_domain);
    let sponsor = state.bond(&old.terms.sponsor).ok_or("replacement sponsor native key absent")?;
    if old.terms.sponsor != context.intent.producer
        || sponsor.status != PalwBondStatusV2::Active
        || sponsor.pubkey != context.producer_pubkey
        || old.terms.material_manifest.as_ref() != Some(&context.intent.manifest)
        || old.terms.native_session.is_some()
        || old.terms.nonce
            != hash(b"MISAKA/PALW/KERNEL-RECOVERY-NONCE/V4", &(network, context.request, old.terms.policy, contract.terms.policy))?
        || contract.terms.root != intent.root
        || contract.terms.nonce != old.terms.nonce
        || contract.terms.sponsor != old.terms.sponsor
        || contract.terms.policy != recovery.id()
        || contract.generations.len() >= contract.terms.max_generations as usize
        || old.consent != intent.previous
        || old.sequence.checked_add(1) != Some(intent.sequence)
        || old.providers.iter().filter(|p| !p.charged || !p.fee_paid).count() + intent.providers.len()
            > PALW_MONETARY_MATERIAL_MAX_PROVIDERS_V1
    {
        return Err("replacement does not match current exact accepted covenant/sequence/capacity".into());
    }
    let fees = p.minimum_fee_sompi.checked_mul(intent.providers.len() as u64).ok_or("replacement fees overflow")?;
    if u128::from(sponsor.collateral).saturating_sub(palw_bond_committed_raw_v1(state, params, &old.terms.sponsor, now, raw_depth))
        < u128::from(fees)
    {
        return Err("replacement sponsor lacks own unused fee funding".into());
    }
    let mut keys: [Option<(PalwBondKeyV2, NativeRetainedV1<Vec<u8>>)>; 8] = std::array::from_fn(|_| None);
    for (index, provider) in intent.providers.iter().enumerate() {
        let native = state.bond(provider).ok_or("replacement provider native bond absent")?;
        if native.status != PalwBondStatusV2::Active
            || native.operator_id == sponsor.operator_id
            || intent.providers[..index].iter().any(|b| state.bond(b).is_some_and(|r| r.operator_id == native.operator_id))
            || old.providers.iter().any(|p| p.provider == *provider && (!p.charged || !p.fee_paid))
            || u128::from(native.collateral).saturating_sub(palw_bond_committed_raw_v1(state, params, provider, now, raw_depth))
                < u128::from(p.minimum_collateral_sompi)
        {
            return Err("replacement provider is dependent/reused or lacks actual capital".into());
        }
        let mut count = 0u64;
        let mut held = old.terms.max_material_bytes;
        for ((table, _), bytes) in &route.aux {
            if *table != PALW_MONETARY_MATERIAL_TABLE_V1 {
                continue;
            }
            preflight_native_value_v1::<PalwMonetaryMaterialRowV1>(bytes.as_slice(), bytes.len() as u64).map_err(|e| e.to_string())?;
            let row = retain_native_value_decode_v1(bytes, || borsh::from_slice::<PalwMonetaryMaterialRowV1>(bytes))
                .map_err(|e| e.to_string())?;
            if row.providers.iter().any(|p| p.provider == *provider && !p.charged) {
                count += 1;
                held = held.checked_add(row.terms.max_material_bytes).ok_or("replacement provider bytes overflow")?;
            }
        }
        if count >= u64::from(p.max_contracts_per_provider) || held > p.max_bytes_per_provider {
            return Err("replacement provider actual native capacity exhausted".into());
        }
        keys[index] = Some((*provider, NativeRetainedV1::try_from_clone_v1(&native.pubkey).map_err(|e| e.to_string())?));
    }
    let local = crate::palw_native_resource_v1::native_declared_logical_limits_v1()
        .ok_or("replacement needs explicit metadata RAM/DB capacity")?;
    let n = intent.providers.len() as u64;
    let candidate = restore_kernel_material_identity_v4(
        context,
        bundle,
        PalwKernelMaterialContextLimitsV4 { max_encoded_bytes: local.database_bytes / n / 2, max_heap_bytes: local.memory_bytes / n },
        old.terms.max_material_bytes,
        crate::palw_attempt_material_v3::verify_ready_signature_v3,
    )?;
    let heap = old
        .terms
        .native_heap_v1("replacement draft")
        .bytes()
        .ok_or("replacement terms heap unknown")?
        .checked_add(std::mem::size_of::<Action>() as u64 + std::mem::size_of::<PalwMonetaryMaterialReplacementV1>() as u64)
        .and_then(|v| {
            v.checked_add(
                n * (std::mem::size_of::<PalwMonetaryMaterialOfferV1>() + std::mem::size_of::<PalwMaterialRecoverySignatureV2>())
                    as u64,
            )
        })
        .ok_or("replacement proposal heap overflow")?;
    let lease = reserve_native_requests_v1(heap).map_err(|e| e.to_string())?;
    let proposal = Action::ReplaceBeforeV4 {
        valid_until_daa: intent.valid_until_daa,
        recovery: contract.id(),
        replacement: Box::new(PalwMonetaryMaterialReplacementV1 {
            root: intent.root,
            previous: intent.previous,
            sequence: intent.sequence,
            enrollment: PalwMonetaryMaterialEnrollmentV1 {
                terms: old.terms.clone(),
                offers: intent
                    .providers
                    .iter()
                    .map(|provider| PalwMonetaryMaterialOfferV1 {
                        provider: *provider,
                        collateral_sompi: p.minimum_collateral_sompi,
                        prepaid_fee_sompi: p.minimum_fee_sompi,
                        signature: vec![],
                    })
                    .collect(),
            },
        }),
        providers: intent
            .providers
            .iter()
            .map(|provider| PalwMaterialRecoverySignatureV2 { provider: *provider, signature: vec![] })
            .collect(),
        sponsor_signature: vec![],
    };
    Ok(PalwKernelMaterialReplacementOwnedV4 {
        proposal: NativeRetainedV1::from_owner_reserved_v1(proposal, lease),
        network,
        daa_score: now,
        candidate,
        manifest: NativeRetainedV1::try_from_clone_v1(&context.intent.manifest).map_err(|e| e.to_string())?,
        producer_key: NativeRetainedV1::try_from_clone_v1(&sponsor.pubkey).map_err(|e| e.to_string())?,
        provider_keys: keys,
    })
}

/// Presence in active OR terminal recovery history preserves material. Missing
/// rows alone never authorize expiry; the caller also needs a signed bounded
/// capsule and a positive own-native finalized clock beyond its exact deadline.
pub fn material_generation_present_v4(state: &PalwChainStateV2, root: Hash64, consent: Hash64) -> Result<bool, String> {
    let Some(route) = state.kernel_route() else { return Ok(false) };
    for ((table, key), bytes) in &route.aux {
        if key.as_slice() != root.as_byte_slice() {
            continue;
        }
        if *table == PALW_MONETARY_MATERIAL_TABLE_V1 {
            preflight_native_value_v1::<PalwMonetaryMaterialRowV1>(bytes.as_slice(), bytes.len() as u64).map_err(|e| e.to_string())?;
            let row = retain_native_value_decode_v1(bytes, || borsh::from_slice::<PalwMonetaryMaterialRowV1>(bytes))
                .map_err(|e| e.to_string())?;
            if row.consent == consent {
                return Ok(true);
            }
        } else if *table == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 {
            preflight_native_value_v1::<PalwMaterialRecoveryRowV2>(bytes.as_slice(), bytes.len() as u64).map_err(|e| e.to_string())?;
            let row = retain_native_value_decode_v1(bytes, || borsh::from_slice::<PalwMaterialRecoveryRowV2>(bytes))
                .map_err(|e| e.to_string())?;
            if row.contract().generations.iter().any(|g| g.consent == consent) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
