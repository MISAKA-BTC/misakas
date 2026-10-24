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
/// Select the next action from the actual bounded provider check, borrowing its
/// finding without cloning the unit list or allocating another reservation.
///
/// The caller must resolve `unit_obligation_accepted` against its own current
/// canonical native ledger for this exact claim and actor (or positively served
/// material). A durable local outbox, broadcast, or spent input is not such an
/// observation. Recompute this selection when that canonical view changes;
/// the returned index is not a durable cursor and does not authenticate credit.
/// Clean and concrete-fault results have exactly one action, index zero.
/// `None` means only that every missing unit already has an observed obligation;
/// it never turns a Demand result into a clean receipt or verifier approval.
pub fn next_provider_objective_action_index_v4(
    checked: &ProviderCheckedResultV4,
    unit_obligation_accepted: impl FnMut(u8, u32) -> Result<bool, Refusal>,
) -> Result<Option<usize>, Refusal> {
    next_objective_action_index_v4(checked.finding(), unit_obligation_accepted)
}
impl VerifiedPaidClaimSourcesV2 {
    /// Select against the same own canonical route that authenticated these
    /// paid sources. A publicly served position or this actor's existing
    /// demand avoids duplicate filing; another actor's demand alone does not.
    /// The reserved ledger owner remains live throughout the borrowed scan.
    /// Reacquire these sources after a canonical route change. `None` is not a
    /// clean receipt, a completed verification, or selected-verifier approval.
    pub fn next_provider_objective_action_index_v4(
        &self,
        checked: &ProviderCheckedResultV4,
        actor: PalwBondKeyV2,
    ) -> Result<Option<usize>, Refusal> {
        if !matches!(checked.finding(), OutsiderFindingV1::Demand(_)) {
            return next_provider_objective_action_index_v4(checked, |_, _| unreachable!("only demands inspect native obligations"));
        }
        let ledger = self.route.ledger().map_err(bad)?;
        let claim = self.authority.enrollment.base_claim;
        let demander = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&actor);
        next_provider_objective_action_index_v4(checked, |stage, position| {
            let key = (claim, stage, position);
            Ok(ledger.served.contains_key(&key)
                || ledger.demands.get(&key).is_some_and(|row| row.demanders.iter().any(|(bond, _)| *bond == demander)))
        })
    }
}
fn next_objective_action_index_v4(
    finding: &OutsiderFindingV1,
    mut unit_obligation_accepted: impl FnMut(u8, u32) -> Result<bool, Refusal>,
) -> Result<Option<usize>, Refusal> {
    match finding {
        OutsiderFindingV1::Clean | OutsiderFindingV1::Prosecute(_) => Ok(Some(0)),
        OutsiderFindingV1::Demand(units) => {
            if units.is_empty() {
                return Err(bad("empty demand result has no objective action"));
            }
            for (index, &(stage, position)) in units.iter().enumerate() {
                if stage >= 254 {
                    return Err(bad("complete provider unit needs monetary material Challenge, not legacy FileDemand"));
                }
                if !unit_obligation_accepted(stage, position)? {
                    return Ok(Some(index));
                }
            }
            Ok(None)
        }
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
            if demand_index != 0 {
                return Err(bad("one clean result has only action index zero"));
            }
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
    // Preserve the receipt namespace chosen from the authenticated sources,
    // including an existing private V6 binding. Signing cannot install that
    // binding or replace native assignment/source/activation validation.
    match &action.object {
        KernelRouteObjectV1::PaidClaimV2 {
            object:
                misaka_palw_kernel::claim_paid_v2::PaidClaimObjectV2::Receipt { receipt }
                | misaka_palw_kernel::claim_paid_v2::PaidClaimObjectV2::RoundReceipt { receipt }
                | misaka_palw_kernel::claim_paid_v2::PaidClaimObjectV2::ReceiptApprovedBootstrapV6 { receipt },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_only_observed_units_in_the_actual_finding_order() {
        let finding = OutsiderFindingV1::Demand(vec![(1, 19), (0, 7), (3, 21)]);
        let mut observed = Vec::new();
        let next = next_objective_action_index_v4(&finding, |stage, position| {
            observed.push((stage, position));
            Ok((stage, position) == (1, 19))
        })
        .unwrap();
        assert_eq!(next, Some(1));
        assert_eq!(observed, vec![(1, 19), (0, 7)]);
        assert_eq!(next_objective_action_index_v4(&finding, |_, _| Ok(true)).unwrap(), None);
        // A reorg removing the first obligation requires it again. A saved
        // cursor must never make this newly missing unit disappear.
        assert_eq!(next_objective_action_index_v4(&finding, |_, _| Ok(false)).unwrap(), Some(0));
    }

    #[test]
    fn clean_and_concrete_fault_never_consult_a_demand_observation() {
        assert_eq!(next_objective_action_index_v4(&OutsiderFindingV1::Clean, |_, _| panic!("not a demand")).unwrap(), Some(0));
        let finding = OutsiderFindingV1::Prosecute(misaka_palw_kernel::route::ProsecutionV1::Kernel(vec![]));
        assert_eq!(next_objective_action_index_v4(&finding, |_, _| panic!("not a demand")).unwrap(), Some(0));
    }

    #[test]
    fn empty_unsupported_and_unknown_observations_never_finish_the_check() {
        assert!(next_objective_action_index_v4(&OutsiderFindingV1::Demand(vec![]), |_, _| Ok(true)).is_err());
        for stage in [254, 255] {
            let finding = OutsiderFindingV1::Demand(vec![(stage, 4)]);
            assert!(next_objective_action_index_v4(&finding, |_, _| panic!("unsupported unit must not be skipped")).is_err());
        }
        let finding = OutsiderFindingV1::Demand(vec![(0, 1), (0, 2)]);
        let result = next_objective_action_index_v4(&finding, |_, position| {
            if position == 1 { Ok(true) } else { Err(bad("own native observation unavailable")) }
        });
        assert!(result.is_err());
    }
}

#[cfg(test)]
#[path = "provider_actions_v4_signing_tests.rs"]
mod signing_tests;
