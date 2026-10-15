//! Durable intrinsic kernel verification context. Positive current native
//! covenant authority is required on EVERY restore. A saved checkpoint or a
//! missing claim is never acceptance, release, or permission to answer.
use crate::{
    Hash64,
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_native_resource_v1::{NativeHeapV1, NativeRetainedV1, ReservedKernelLedgerV1, reserve_native_requests_v1},
    palw_prospective_kernel_material_v4::{
        PALW_PROSPECTIVE_KERNEL_MATERIAL_CONTEXT_V4, PalwProspectiveKernelBundleV4, PalwProspectiveKernelMaterialIntentV4,
        PalwProspectiveKernelMaterialOwnedV4, PalwProspectiveKernelMaterialRequestV4, commit_identity_v4, object_hash,
    },
    palw_state_v2::PalwChainStateV2,
};
use borsh::{BorshDeserialize, BorshSerialize};

/// Explicit local RAM/DB metadata limits, separate from signed material bytes.
/// The provider ALSO enforces aggregate metadata limits across all contracts.
#[derive(Clone, Copy, Debug)]
pub struct PalwKernelMaterialContextLimitsV4 {
    pub max_encoded_bytes: u64,
    pub max_heap_bytes: u64,
}
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct PalwProspectiveKernelMaterialContextV4 {
    pub version: u16,
    pub prior_route: PalwKernelRouteStateV1,
    pub intent: PalwProspectiveKernelMaterialIntentV4,
    pub producer_signature: Vec<u8>,
    pub producer_pubkey: Vec<u8>,
    pub kernel_collateral: u64,
    pub request: Hash64,
}
fn heap(context: &PalwProspectiveKernelMaterialContextV4) -> Result<u64, String> {
    let path = "kernel durable intrinsic metadata";
    let mut total = std::mem::size_of::<PalwProspectiveKernelMaterialContextV4>() as u64;
    for coverage in [
        context.prior_route.native_heap_v1(path),
        context.intent.native_heap_v1(path),
        context.producer_signature.native_heap_v1(path),
        context.producer_pubkey.native_heap_v1(path),
    ] {
        total = total.checked_add(coverage.bytes().ok_or_else(|| coverage.to_string())?).ok_or("kernel metadata heap overflow")?;
    }
    Ok(total)
}
impl PalwProspectiveKernelMaterialOwnedV4 {
    /// Called under the native own-state pin, not from a remote submitted row.
    /// The outer owner reserves every clone before construction. Decode and
    /// publication additionally require scanner + aggregate store RAM/DB caps.
    pub fn export_context_v4(
        &self,
        state: &PalwChainStateV2,
        request: &PalwProspectiveKernelMaterialRequestV4,
        limits: PalwKernelMaterialContextLimitsV4,
    ) -> Result<NativeRetainedV1<PalwProspectiveKernelMaterialContextV4>, String> {
        let route = state.kernel_route().ok_or("kernel metadata route absent")?;
        if request.id() != self.request_id()
            || route.ledger_root() != request.intent.ledger_root
            || route.aux_root() != request.intent.aux_root
            || self.authority().producer != request.intent.producer
        {
            return Err("durable context substitutes native verified prior state".into());
        }
        let actor = palw_kernel_bond_id_v1(&request.intent.producer);
        let collateral = self.candidate_ledger().bonds.get(&actor).ok_or("candidate producer absent")?.collateral;
        let encoded = 2u64
            .checked_add(borsh::object_length(route).map_err(|e| e.to_string())? as u64)
            .and_then(|n| n.checked_add(borsh::object_length(&request.intent).ok()? as u64))
            .and_then(|n| n.checked_add(4 + request.producer_signature.len() as u64))
            .and_then(|n| n.checked_add(4 + self.producer_pubkey().len() as u64 + 8 + 64))
            .ok_or("durable context encoded length overflow")?;
        if encoded > limits.max_encoded_bytes {
            return Err("kernel metadata exceeds explicit DB cap".into());
        }
        let path = "kernel durable intrinsic metadata construction";
        let mut total = std::mem::size_of::<PalwProspectiveKernelMaterialContextV4>() as u64;
        for coverage in
            [route.native_heap_v1(path), request.intent.native_heap_v1(path), request.producer_signature.native_heap_v1(path)]
        {
            total = total.checked_add(coverage.bytes().ok_or_else(|| coverage.to_string())?).ok_or("metadata copy heap overflow")?;
        }
        total = total.checked_add(self.producer_pubkey().len() as u64).ok_or("metadata key heap overflow")?;
        if total > limits.max_heap_bytes {
            return Err("kernel metadata exceeds explicit RAM cap".into());
        }
        let lease = reserve_native_requests_v1(total).map_err(|e| e.to_string())?;
        let context = PalwProspectiveKernelMaterialContextV4 {
            version: 4,
            prior_route: route.clone(),
            intent: request.intent.clone(),
            producer_signature: request.producer_signature.clone(),
            producer_pubkey: self.producer_pubkey().to_vec(),
            kernel_collateral: collateral,
            request: self.request_id(),
        };
        Ok(NativeRetainedV1::from_owner_reserved_v1(context, lease))
    }
}
/// `current_positive` MUST authenticate current own-node covenant authority:
/// exact request-derived nonce (network/request/policy/recovery), root, manifest,
/// consent generation, and provider's own stored 240/242 signatures under its
/// registered native key. An unaccepted signed offer returns Err. Such bytes
/// may be retained until positive finalized expiry but cannot authorize answers.
/// Terminal receipts use the provider's separate positive native GC path.
///
/// This first version saves all prior canonical rows rather than fabricating an
/// accepted selected ClassRow. Both prior roots and producer signature are
/// rechecked; the real kernel reader and salted admission derive a candidate.
/// The result is local verification material, never installed in native state.
pub fn restore_kernel_material_context_v4(
    context: &PalwProspectiveKernelMaterialContextV4,
    bundle: &PalwProspectiveKernelBundleV4,
    limits: PalwKernelMaterialContextLimitsV4,
    max_material_bytes: u64,
    current_positive: impl FnOnce(&PalwProspectiveKernelMaterialContextV4) -> Result<(), String>,
    verify_mldsa: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<ReservedKernelLedgerV1, String> {
    current_positive(context)?;
    restore_kernel_material_identity_v4(context, bundle, limits, max_material_bytes, verify_mldsa)
}
/// Core-only preparation helper. Callers must first authenticate the exact
/// current covenant/root/request nonce and original native producer key. Its
/// result is material identity data, never permission to answer or earn credit.
/// Public answer restore retains its mandatory current_positive call FIRST.
pub(crate) fn restore_kernel_material_identity_v4(
    context: &PalwProspectiveKernelMaterialContextV4,
    bundle: &PalwProspectiveKernelBundleV4,
    limits: PalwKernelMaterialContextLimitsV4,
    max_material_bytes: u64,
    verify_mldsa: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<ReservedKernelLedgerV1, String> {
    if context.version != 4
        || context.intent.version != 4
        || borsh::object_length(context).map_err(|e| e.to_string())? as u64 > limits.max_encoded_bytes
        || heap(context)? > limits.max_heap_bytes
        || context.prior_route.ledger_root() != context.intent.ledger_root
        || context.prior_route.aux_root() != context.intent.aux_root
        || !matches!(bundle, PalwProspectiveKernelBundleV4::TypedKernelV4(_))
    {
        return Err("durable intrinsic context framing/root/cap mismatch".into());
    }
    let request = object_hash(b"MISAKA/PALW/PROSPECTIVE-KERNEL-REQUEST/V4", &(&context.intent, &context.producer_signature, bundle));
    let (claim, _, actor) = commit_identity_v4(&context.intent.commit);
    if request != context.request
        || bundle.claim().as_bytes() != claim
        || context.intent.manifest.root.as_bytes() != claim
        || actor != palw_kernel_bond_id_v1(&context.intent.producer)
    {
        return Err("durable context request/claim substituted".into());
    }
    let network = Hash64::from_bytes(context.prior_route.header.policy.network_domain);
    let message = context.intent.signing_message(network);
    if !verify_mldsa(
        &context.producer_pubkey,
        message.as_byte_slice(),
        &context.producer_signature,
        PALW_PROSPECTIVE_KERNEL_MATERIAL_CONTEXT_V4,
    ) {
        return Err("durable context producer signature invalid".into());
    }
    let mut candidate = context.prior_route.ledger()?;
    if candidate.claims.contains_key(&claim) {
        return Err("durable prior route already contains prospective claim".into());
    }
    candidate.try_sync_bond_v1(actor, context.kernel_collateral).map_err(|e| e.to_string())?;
    candidate.try_replay_salted_claim_v4(&context.intent.commit, &context.intent.salt, actor, context.intent.commit_daa)?;
    candidate.try_reserve_kernel_material_v4(&claim).map_err(|e| e.to_string())?;
    let coverage = crate::palw_material_capacity_v1::prospective_kernel_bundle_requests_v4(bundle)?;
    let _inner = reserve_native_requests_v1(coverage.bytes().ok_or_else(|| coverage.to_string())?).map_err(|e| e.to_string())?;
    let auth = crate::palw_material_capacity_v1::prospective_kernel_bundle_auth_requests_v4(bundle, &candidate)?;
    let _auth = reserve_native_requests_v1(auth.bytes().ok_or_else(|| auth.to_string())?).map_err(|e| e.to_string())?;
    let scope = bundle.verify(&candidate, max_material_bytes).map_err(|e| e.to_string())?;
    if context.intent.manifest.scopes.len() != 1 || context.intent.manifest.scopes.first() != Some(&scope) {
        return Err("restored material differs from exact enrolled manifest".into());
    }
    Ok(candidate)
}
