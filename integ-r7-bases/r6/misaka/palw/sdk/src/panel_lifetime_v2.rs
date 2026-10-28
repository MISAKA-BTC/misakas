//! Caller-pinned native readers and producer-only Panel consent preparation.
//! Prospective consent has no future claim/header hash. Native execution consumes
//! it once and captures its immutable statement before any Panel entropy.
use crate::runtime_pack::commit::Refusal;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_panel_lifetime_v2::{
    PalwPanelAuthorizationRowV2, PalwPanelLifetimeEnrollmentV2, PanelLifetimeAuthorizationV2,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::route::KernelRouteObjectV1;

pub fn panel_lifetime_from_kernel_rows_v2(
    header: &[u8],
    rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    ledger_root: &Hash64,
    aux_root: &Hash64,
    claim: &Hash64,
) -> Result<Option<PalwPanelLifetimeEnrollmentV2>, Refusal> {
    let route =
        kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(header, rows, ledger_root, aux_root)
            .map_err(|e| Refusal::new("ROWS_NOT_THE_CHAIN", e))?;
    route.panel_lifetime_enrollment_v2(claim).map_err(|e| Refusal::new("PANEL_LIFETIME_INVALID", e))
}

pub fn panel_authorization_from_kernel_rows_v2(
    header: &[u8],
    rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    ledger_root: &Hash64,
    aux_root: &Hash64,
    authorization: &[u8; 64],
) -> Result<PalwPanelAuthorizationRowV2, Refusal> {
    let route =
        kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(header, rows, ledger_root, aux_root)
            .map_err(|e| Refusal::new("ROWS_NOT_THE_CHAIN", e))?;
    route.panel_authorization_v2(authorization).map_err(|e| Refusal::new("PANEL_LIFETIME_INVALID", e))
}

/// `state` must be restored against a separately authenticated selected-chain
/// state root. The caller's DAA is that snapshot's point, not a forecast.
pub fn prepare_panel_authorization_object_v2(
    params: &Params,
    state: &PalwChainStateV2,
    producer: &PalwBondKeyV2,
    tip_daa: u64,
    terms: PanelLifetimeAuthorizationV2,
) -> Result<KernelRouteObjectV1, Refusal> {
    let refuse = |why: &str| Refusal::new("PANEL_LIFETIME_INVALID", why);
    terms.validate().map_err(refuse)?;
    let fence = params
        .palw_panel_lifetime_v2
        .filter(|f| f.activation.is_active(tip_daa))
        .ok_or_else(|| refuse("the compiled prospective Panel lifetime fence is absent"))?;
    fence.policy.validate().map_err(refuse)?;
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(refuse("compiled native params missing"));
    };
    let panel = bundle.state.panel_v3().ok_or_else(|| refuse("compiled Panel mirror missing"))?;
    let class = Hash64::from_bytes(terms.class);
    let bond = state.bond(producer).ok_or_else(|| refuse("producer bond not in the pinned state"))?;
    if !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
        || state.class(&class).is_none()
        || state.last_point().is_none_or(|p| p.daa_score != tip_daa)
    {
        return Err(refuse("producer, class or actual snapshot position is not admitted"));
    }
    let court = state.class_model_court_window_v1(&class).unwrap_or(bundle.state.window_court());
    let tail = court
        .checked_mul(2)
        .and_then(|n| n.checked_add(bundle.state.window_challenge()))
        .ok_or_else(|| refuse("court liability overflow"))?;
    if terms.producer != kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(producer)
        || terms.network != panel.network.as_bytes()
        || terms.ruleset != panel.ruleset.as_bytes()
        || terms.accept_by_daa < tip_daa
        || tip_daa.checked_add(fence.policy.maximum_service_daa).is_none_or(|end| terms.service_until_daa > end)
        || terms.service_until_daa.checked_add(tail).is_none_or(|end| terms.liability_until_daa < end)
        || terms.maximum_retries > fence.policy.maximum_retries
        || terms.maximum_retries > u32::from(panel.policy.max_retries)
        || terms.maximum_pause_daa > fence.policy.maximum_pause_daa
    {
        return Err(refuse("authority or finite service limits differ from compiled native policy"));
    }
    Ok(KernelRouteObjectV1::AuthorizePanelLifetimeV2 { authorization: terms })
}

/// Optional exact reaffirmation uses the statement already captured at native
/// admission. It cannot replace an epoch, extend a clock or reset a counter.
pub fn prepare_panel_lifetime_object_v2(
    params: &Params,
    state: &PalwChainStateV2,
    producer: &PalwBondKeyV2,
    claim: &Hash64,
    tip_daa: u64,
) -> Result<KernelRouteObjectV1, Refusal> {
    let refuse = |why: &str| Refusal::new("PANEL_LIFETIME_INVALID", why);
    let fence = params
        .palw_panel_lifetime_v2
        .filter(|f| f.activation.is_active(tip_daa))
        .ok_or_else(|| refuse("compiled Panel lifetime fence absent"))?;
    let route = state.kernel_route().ok_or_else(|| refuse("native consent route missing"))?;
    let row = route
        .panel_lifetime_enrollment_v2(claim)
        .map_err(|e| refuse(&e))?
        .ok_or_else(|| refuse("native admission has not captured this claim"))?;
    let native_claim = state.claim(claim).ok_or_else(|| refuse("native claim missing"))?;
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(refuse("compiled native params missing"));
    };
    kaspa_consensus_core::palw_panel_lifetime_v2::panel_lifetime_claim_v2(state, &bundle.state, claim, native_claim)
        .map_err(|e| refuse(&e))?
        .ok_or_else(|| refuse("compiled native mirror differs from captured consent"))?;
    if row.signer != *producer
        || row.policy != fence.policy.id()
        || tip_daa > row.terms.service_until_daa
        || state.last_point().is_none_or(|p| p.daa_score != tip_daa)
        || state.panel_v3().and_then(|p| p.claim(claim)).is_none_or(|r| r.phase != misaka_palw_panel::ClaimPhaseV3::PendingSeal)
    {
        return Err(refuse("exact attachment authority, snapshot position or pre-entropy stage differs"));
    }
    Ok(KernelRouteObjectV1::EnrollPanelConsumerLifetimeV2 { terms: row.terms })
}

/// Match the key to the actual registered producer bond before signing inner65/66.
pub fn sign_panel_lifetime_object_v2(
    state: &PalwChainStateV2,
    key: &dyn kaspa_pq_validator_core::MessageSigner,
    domain: Hash64,
    bond: PalwBondKeyV2,
    object: &KernelRouteObjectV1,
) -> Result<PalwConsensusObjectV2, String> {
    let (actor, network) = match object {
        KernelRouteObjectV1::AuthorizePanelLifetimeV2 { authorization } => {
            authorization.validate()?;
            (authorization.producer, authorization.network)
        }
        KernelRouteObjectV1::EnrollPanelConsumerLifetimeV2 { terms } => {
            terms.validate()?;
            (terms.producer, terms.network)
        }
        _ => return Err("Panel producer signer accepts inner65/66 only".into()),
    };
    let registered = state.bond(&bond).ok_or("signing bond missing from pinned native state")?;
    if network != domain.as_bytes()
        || actor != kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond)
        || registered.pubkey.as_slice() != key.public_key()
        || !matches!(registered.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
    {
        return Err("key and registered producer authority differ".into());
    }
    let bytes = object.encode();
    if bytes.len() > object.max_encoded_bytes()
        || bytes.len() > kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1
    {
        return Err("Panel consent exceeds the native object bound".into());
    }
    KernelRouteObjectV1::decode(&bytes).map_err(|e| e.to_string())?;
    let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(domain, &bond, &bytes);
    let signature = key.sign_message(
        message.as_byte_slice(),
        kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
    )?;
    Ok(PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature })
}

pub mod cli;
