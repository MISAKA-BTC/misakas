//! Concrete court carriers from a completed local detection result. A public
//! fault filing is authenticated evidence for deterministic adjudication, not
//! a computation-validity proof. No query failure is an arithmetic conviction.
use super::{Refusal, VerifiedPaidClaimSourcesV2, provider_reader_v4::ProviderCheckedResultV4};
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, reserve_native_requests_v1},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
};
use misaka_palw_kernel::{ledger::OutsiderFindingV1, route::KernelRouteObjectV1};
fn bad(e: impl ToString) -> Refusal {
    Refusal::new("PROVIDER_COURT_ACTION_V4", e.to_string())
}
pub struct ProviderObjectiveActionV4 {
    object: KernelRouteObjectV1,
    _construction: Option<NativeDemandLeaseV1>,
}
impl ProviderObjectiveActionV4 {
    pub fn object(&self) -> &KernelRouteObjectV1 {
        &self.object
    }
}
/// A clean result can issue only its assigned receipt. A concrete fault can
/// issue a kernel FileProof. For a Demand result the caller advances an explicit
/// index and must fund/submit each required obligation independently. This
/// function never converts remote errors, local capacity or absence into guilt.
pub fn objective_action_from_provider_check_v4(
    sources: &VerifiedPaidClaimSourcesV2,
    checked: &ProviderCheckedResultV4,
    actor: PalwBondKeyV2,
    now: u64,
    demand_index: usize,
) -> Result<ProviderObjectiveActionV4, Refusal> {
    let bond = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&actor);
    let claim = sources.authority().enrollment.base_claim;
    match checked.finding() {
        OutsiderFindingV1::Clean => {
            Ok(ProviderObjectiveActionV4 { object: checked.receipt(sources, bond, now)?, _construction: None })
        }
        OutsiderFindingV1::Prosecute(proof) => {
            if demand_index != 0 {
                return Err(bad("one concrete fault has only action index zero"));
            }
            let bytes = proof
                .native_heap_v1("provider court concrete filing copy")
                .bytes()
                .ok_or_else(|| bad("filing native copy coverage unknown"))?;
            let lease = reserve_native_requests_v1(
                bytes
                    .checked_add(std::mem::size_of::<KernelRouteObjectV1>() as u64)
                    .ok_or_else(|| bad("filing copy size overflow"))?,
            )
            .map_err(bad)?;
            Ok(ProviderObjectiveActionV4 {
                object: KernelRouteObjectV1::FileProof { accuser: bond, claim, proof: proof.clone() },
                _construction: lease,
            })
        }
        OutsiderFindingV1::Demand(units) => {
            let &(stage, position) = units.get(demand_index).ok_or_else(|| bad("demand action index outside actual missing units"))?;
            // Only native node positions reported by the actual kernel checker.
            // Complete-codec artifact/segmented units use their provider scope
            // obligation, never an invented legacy FileDemand stage.
            if stage >= 254 {
                return Err(bad("complete provider unit needs monetary material Challenge, not legacy FileDemand"));
            }
            Ok(ProviderObjectiveActionV4 {
                object: KernelRouteObjectV1::FileDemand { demander: bond, claim, stage, position },
                _construction: None,
            })
        }
    }
}
/// Native signing for the existing court carrier. The exact object, registered
/// actor role and network are bound to the same old kernel signing domain; no
/// new approval mode or route codec is introduced. The caller must obtain own
/// admission/rehearsal, funding and durable relay independently.
pub fn sign_provider_objective_action_v4(
    key: &dyn kaspa_pq_validator_core::MessageSigner,
    network: Hash64,
    bond: PalwBondKeyV2,
    action: &ProviderObjectiveActionV4,
) -> Result<NativeRetainedV1<PalwConsensusObjectV2>, Refusal> {
    let actor = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&bond);
    match &action.object {
        KernelRouteObjectV1::PaidClaimV2 {
            object:
                misaka_palw_kernel::claim_paid_v2::PaidClaimObjectV2::Receipt { receipt }
                | misaka_palw_kernel::claim_paid_v2::PaidClaimObjectV2::RoundReceipt { receipt },
        } if receipt.signer == actor => {}
        KernelRouteObjectV1::FileProof { accuser, .. } if *accuser == actor => {}
        KernelRouteObjectV1::FileDemand { demander, .. } if *demander == actor => {}
        _ => return Err(bad("objective action does not name the configured actor's receipt/court role")),
    }
    let n = action.object.encoded_len();
    if n > action.object.max_encoded_bytes() || n > kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 {
        return Err(bad("objective filing exceeds native carrier envelope"));
    }
    let workspace = (n as u64)
        .checked_add(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
        .and_then(|n| n.checked_add(std::mem::size_of::<PalwConsensusObjectV2>() as u64))
        .ok_or_else(|| bad("objective signature workspace overflow"))?;
    let _construction = reserve_native_requests_v1(workspace).map_err(bad)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(bad)?;
    bytes.push(misaka_palw_kernel::route::KERNEL_ROUTE_VERSION_V1);
    borsh::to_writer(&mut bytes, &action.object).map_err(bad)?;
    if bytes.len() != n {
        return Err(bad("objective filing frame changed while encoding"));
    }
    let message = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_message_v1(network, &bond, &bytes);
    let signature = key
        .sign_message(message.as_byte_slice(), kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1)
        .map_err(bad)?;
    if signature.len() != kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err(bad("objective action signer returned wrong signature length"));
    }
    NativeRetainedV1::retain_existing_v1(PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature }).map_err(bad)
}
