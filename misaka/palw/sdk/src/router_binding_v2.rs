//! RFC-0006 immutable per-reader routing identity, built offline for
//! `misaka palw submit-object`. The caller supplies every reader/router pair;
//! ambiguous graphs never cause this builder to select a router implicitly.
//! Native acceptance separately checks activation, the registered artifact,
//! registrant authority, immutability and resource admission.
use kaspa_consensus_core::{
    Hash64,
    palw_memory_routing_v2::PalwMemoryRoutingObjectV2,
    palw_router_binding_v2::{
        PALW_ROUTER_BINDING_SIGNATURE_CONTEXT_V2, PalwRouterBindingDeclarationV2, palw_router_binding_check_v2,
        palw_router_binding_message_v2,
    },
    palw_state_v2::PalwConsensusObjectV2,
};
use kaspa_pq_validator_core::ValidatorKey;

#[path = "router_binding_v2/court_expectation.rs"]
pub mod court_expectation;

#[path = "router_binding_v2/cli.rs"]
pub mod cli;

/// Sign the existing consensus declaration and return its existing tag-244
/// carrier. No snapshot, registration, approval or activation is synthesized.
pub fn signed_router_binding_v2(
    key: &ValidatorKey,
    network_domain: Hash64,
    declaration: PalwRouterBindingDeclarationV2,
) -> Result<PalwConsensusObjectV2, String> {
    palw_router_binding_check_v2(&declaration).map_err(str::to_owned)?;
    let message = palw_router_binding_message_v2(network_domain, &declaration);
    let signature = key.sign_with_context(message.as_byte_slice(), PALW_ROUTER_BINDING_SIGNATURE_CONTEXT_V2).to_vec();
    Ok(PalwConsensusObjectV2::MemoryRoutingV2 {
        object: Box::new(PalwMemoryRoutingObjectV2::RouterBindingDeclared { declaration, signature }),
    })
}

#[cfg(test)]
#[path = "router_binding_v2/tests.rs"]
mod tests;
