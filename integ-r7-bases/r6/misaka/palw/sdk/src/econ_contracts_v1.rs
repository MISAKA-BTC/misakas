//! Offline drafting and signing of the RFC07/RFC08 signed economic contracts (kernel inner 67–69, 2026-10-15). Each draft is checked
//! against the caller's compiled (dormant) policy and a pinned native state; the node's fold re-checks everything at admission. These
//! helpers grant no activation: where the fence is absent from the compiled params, drafting is refused.
use crate::runtime_pack::commit::Refusal;
use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::palw_econ_contracts_v1::{
    HorizonExtensionTermsV1, MinimumPriceContractTermsV1, ModelVerificationAttestationV1, palw_minimum_price_admit_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::route::KernelRouteObjectV1;

fn active(at: ForkActivation, tip_daa: u64) -> bool {
    at != ForkActivation::never() && at.is_active(tip_daa)
}

/// Inner 67: a drawn verifier's attestation draft (the tally, slot and sample are the node's to check).
pub fn prepare_model_verification_attestation_object_v1(
    params: &Params,
    tip_daa: u64,
    attestation: ModelVerificationAttestationV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    let refuse = |why: &str| Refusal::new("ECON_CONTRACT_INVALID", why);
    params
        .palw_model_verification_gate_v1
        .filter(|f| active(f.activation, tip_daa))
        .ok_or_else(|| refuse("the compiled verification gate fence is absent"))?;
    attestation.validate().map_err(refuse)?;
    Ok(KernelRouteObjectV1::AttestModelVerificationV1 { attestation })
}

/// Inner 68: one party's horizon-extension consent draft, checked against the compiled policy's finite bounds.
pub fn prepare_horizon_extension_object_v1(
    params: &Params,
    tip_daa: u64,
    terms: HorizonExtensionTermsV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    let refuse = |why: &str| Refusal::new("ECON_CONTRACT_INVALID", why);
    let fence = params
        .palw_horizon_extension_v1
        .filter(|f| active(f.activation, tip_daa))
        .ok_or_else(|| refuse("the compiled horizon extension fence is absent"))?;
    fence.policy.validate().map_err(refuse)?;
    terms.validate().map_err(refuse)?;
    if tip_daa > terms.consent_by_daa
        || terms.consent_by_daa - tip_daa > fence.policy.maximum_consent_window_daa
        || terms.new_liability_until_daa - terms.current_liability_until_daa > fence.policy.maximum_extension_daa
        || (fence.policy.require_price_contract && terms.price_contract == [0; 64])
    {
        return Err(refuse("the extension is outside the compiled policy's consent window, extension bound or price requirement"));
    }
    Ok(KernelRouteObjectV1::ConsentHorizonExtensionV1 { terms })
}

/// Inner 69: one party's minimum-price contract draft. `job_escrow_sompi` must be read from the caller's pinned native state; the
/// derived minimum is recomputed here exactly as admission does, so an offer below it is refused before signing.
pub fn prepare_minimum_price_contract_object_v1(
    params: &Params,
    tip_daa: u64,
    job_escrow_sompi: u64,
    contract: MinimumPriceContractTermsV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    let refuse = |why: String| Refusal::new("ECON_CONTRACT_INVALID", why);
    let fence = params
        .palw_minimum_price_v1
        .as_ref()
        .filter(|f| active(f.activation, tip_daa))
        .ok_or_else(|| refuse("the compiled minimum-price fence is absent".into()))?;
    fence.policy.validate().map_err(|e| refuse(e.into()))?;
    palw_minimum_price_admit_v1(&fence.policy, &contract, job_escrow_sompi, tip_daa).map_err(|e| refuse(e.to_string()))?;
    Ok(KernelRouteObjectV1::SignMinimumPriceContractV1 { contract })
}

/// Inner 73 (N3, round 3): one contributor's BootstrapExact seal or reveal draft. The round's windows, the target's commitment
/// position and every judged fact are the node's to derive; drafting only refuses where the compiled bootstrap fence is absent.
pub fn prepare_bootstrap_exact_object_v1(
    params: &Params,
    tip_daa: u64,
    contribution: misaka_palw_kernel::beacon_bootstrap_v1::BootstrapExactContributionV1,
) -> Result<KernelRouteObjectV1, Refusal> {
    let refuse = |why: &str| Refusal::new("ECON_CONTRACT_INVALID", why);
    let fence = params
        .palw_beacon_bootstrap_v1
        .filter(|f| active(f.activation, tip_daa))
        .ok_or_else(|| refuse("the compiled bootstrap fence is absent"))?;
    fence.policy.validate().map_err(refuse)?;
    contribution.validate().map_err(refuse)?;
    if contribution.round.profile != fence.policy.profile.id.as_bytes() {
        return Err(refuse("the round names another profile than the compiled fence's"));
    }
    Ok(KernelRouteObjectV1::BootstrapExactV1 { contribution })
}

/// Sign an inner 39 / 67–69 / 73 draft with the bond the object names as one of its parties (the key must be that bond's registered
/// key).
pub fn sign_econ_contract_object_v1(
    state: &PalwChainStateV2,
    key: &dyn kaspa_pq_validator_core::MessageSigner,
    domain: Hash64,
    bond: PalwBondKeyV2,
    object: &KernelRouteObjectV1,
) -> Result<PalwConsensusObjectV2, String> {
    let me = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond);
    // The `network` field each contract carries is checked by the node against the kernel route's own network domain.
    let parties: Vec<[u8; 64]> = match object {
        KernelRouteObjectV1::AttestDrawnCheckV1 { verifier, .. } => vec![*verifier],
        KernelRouteObjectV1::AttestModelVerificationV1 { attestation } => {
            attestation.validate()?;
            vec![attestation.verifier]
        }
        KernelRouteObjectV1::ConsentHorizonExtensionV1 { terms } => {
            terms.validate()?;
            terms.required_parties()
        }
        KernelRouteObjectV1::SignMinimumPriceContractV1 { contract } => {
            contract.validate()?;
            contract.required_parties()
        }
        KernelRouteObjectV1::BootstrapExactV1 { contribution } => {
            contribution.validate()?;
            vec![contribution.contributor]
        }
        _ => return Err("the economic contract signer accepts inner 39, 67–69 and 73 only".into()),
    };
    let registered = state.bond(&bond).ok_or("signing bond missing from pinned native state")?;
    if !parties.contains(&me)
        || registered.pubkey.as_slice() != key.public_key()
        || !matches!(registered.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
    {
        return Err("the key or registered bond is not a party of this contract".into());
    }
    let bytes = object.encode();
    if bytes.len() > object.max_encoded_bytes()
        || bytes.len() > kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1
    {
        return Err("the contract exceeds the native object bound".into());
    }
    KernelRouteObjectV1::decode(&bytes).map_err(|e| e.to_string())?;
    let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(domain, &bond, &bytes);
    let signature = key.sign_message(
        message.as_byte_slice(),
        kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
    )?;
    Ok(PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature })
}
