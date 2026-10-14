//! RFC-0008 PAV3 producer/provider adapters. Transport between a producer and its session's
//! providers is the operator's; these functions fix what each side may sign and in which order,
//! so neither side can obtain a signature over a capture the fold would refuse.
//!
//! Order of operations (each step refuses rather than degrades):
//! 1. The producer selects the canonical session with
//!    [`kaspa_consensus_core::palw_attempt_material_v3::select_native_session_v3`] over an
//!    AUTHENTICATED row/bond view (op 242 pages finished by `MonetaryMaterialProofPagerV1`, the
//!    bond table by op 213), executes, and drafts Ready with `draft_native_ready_v3`.
//! 2. Each provider rebuilds the complete typed bundle itself from the capture, durably
//!    publishes it ([`provider_sign_native_ready_v3`]), and only then signs `Ready::id`.
//! 3. The producer assembles the carriage in roster order and seals it with its executor key.
use crate::monetary_material::DurableMonetaryMaterialProviderV1;
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_material_v3::{PALW_MATERIAL_READY_CONTEXT_V3, PalwAttemptMaterialEnvelopeV3, PalwNativeMaterialReadyV3},
    palw_attempt_v2::PalwAttemptEnvelopeV2,
    palw_material_capacity_v1::{PalwHeldMaterialBundleV1, PalwMaterialNativeAuthorityV1},
    palw_monetary_material_v1::PalwMonetaryMaterialRowV1,
    palw_state_v2::PalwBondKeyV2,
};

/// Provider side: admit the exact bundle into the durable store (write, fsync, rename, directory
/// fsync) and sign `Ready::id` only after that publication returned. A crash before publication
/// leaves no signature; a crash after it leaves retained material the signature may cite.
pub fn provider_sign_native_ready_v3(
    provider_store: &mut DurableMonetaryMaterialProviderV1,
    provider: PalwBondKeyV2,
    row: &PalwMonetaryMaterialRowV1,
    ready: &PalwNativeMaterialReadyV3,
    bundle: &PalwHeldMaterialBundleV1,
    authority: PalwMaterialNativeAuthorityV1,
    sign: impl FnOnce(&[u8], &[u8]) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    if bundle.binding.committed_execution_root != authority.execution_root {
        return Err("provider bundle names another execution than the native authority".into());
    }
    let id: Hash64 = provider_store.prepare_native_ready_v3(provider, row, ready, bundle, authority)?;
    if id != ready.id() {
        return Err("provider published a different Ready identity".into());
    }
    sign(id.as_byte_slice(), PALW_MATERIAL_READY_CONTEXT_V3)
}

/// Producer side: every roster member's signature, keyed by provider, assembled in the exact
/// roster order and sealed by the executor. A missing provider leaves no carriage (the attempt
/// cannot be priced under the fence without its full roster); it never substitutes another.
pub fn producer_assemble_native_ready_v3(
    base: PalwAttemptEnvelopeV2,
    ready: PalwNativeMaterialReadyV3,
    signatures: &std::collections::BTreeMap<PalwBondKeyV2, Vec<u8>>,
    seal_with: impl FnOnce(&[u8], &[u8]) -> Result<Vec<u8>, String>,
) -> Result<PalwAttemptMaterialEnvelopeV3, String> {
    if signatures.len() != ready.providers.len() {
        return Err("provider signature set differs from the session roster".into());
    }
    let ordered = ready
        .providers
        .iter()
        .map(|provider| signatures.get(provider).cloned().ok_or("a roster provider has not signed"))
        .collect::<Result<Vec<_>, _>>()?;
    PalwAttemptMaterialEnvelopeV3::assemble(base, ready, ordered, seal_with)
}
