//! **The fold of object tag 170** (RFC-0006 memory/routing consumer v2, `crate::palw_memory_routing_v2`). A child module of
//! `palw_state_v2`, so it reaches the builder's one writers (the claim conviction, the seat slash, the kernel-route aux row) and
//! nothing else.
//!
//! Every entry refuses without the per-block extras `memory_routing_court` (the fence's runtime mirror — absent on every shipped
//! network, where the A-2 gate never lets a tag-170 object reach the fold at all), and a refusal writes nothing.
use super::*;
use crate::palw_memory_routing_v2::{PalwMemoryRoutingObjectV2, palw_memory_routing_verdict_v2};
use crate::palw_router_binding_v2::{
    PALW_ROUTER_BINDING_TABLE_V2, PALW_ROUTER_BINDING_VERSION_V2, PalwRouterBindingRowV2, palw_router_binding_check_v2,
    palw_router_binding_key_v2,
};

/// **The class's router binding row, if the chain holds one** (aux 139). `None` where the kernel-route store is not open or the
/// class declared none.
pub fn palw_router_binding_row_of_v2(state: &PalwChainStateV2, class_id: &Hash64) -> Option<PalwRouterBindingRowV2> {
    state.kernel_route.as_ref()?.aux_row::<PalwRouterBindingRowV2>(PALW_ROUTER_BINDING_TABLE_V2, &palw_router_binding_key_v2(class_id))
}

pub(super) fn apply_memory_routing_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwMemoryRoutingObjectV2,
) -> Result<(), PalwStateV2Error> {
    let limits = builder.extras.memory_routing_court.ok_or(PalwStateV2Error::ShardCourtDormant)?;
    match object {
        // The registrant's signature is the acceptance layer's (it holds the registry keys and the network domain), exactly as
        // `ClassSemanticsDeclaredV1`'s is; the fold checks the declaration against the graph and keeps it immutable.
        PalwMemoryRoutingObjectV2::RouterBindingDeclared { declaration, .. } => {
            palw_router_binding_check_v2(declaration).map_err(PalwStateV2Error::ImmutableModelUpdate)?;
            let class = builder
                .state
                .class(&declaration.class_id)
                .ok_or(PalwStateV2Error::ImmutableModelUpdate("router binding names an unregistered class"))?;
            if class.artifact_root != declaration.artifact_root || class.registrant_bond.is_none() {
                return Err(PalwStateV2Error::ImmutableModelUpdate(
                    "router binding must name the originally registered artifact and a signing registrant",
                ));
            }
            if builder.state.kernel_route.is_none() {
                // The row lives in the kernel route's aux store; a binding that could not be stored must not be accepted as if it were.
                return Err(PalwStateV2Error::ImmutableModelUpdate("router binding needs the kernel-route aux store (aux 139)"));
            }
            if palw_router_binding_row_of_v2(&builder.state, &declaration.class_id).is_some() {
                return Err(PalwStateV2Error::ImmutableModelUpdate("a class's router binding is immutable once declared"));
            }
            let row = PalwRouterBindingRowV2 {
                version: PALW_ROUTER_BINDING_VERSION_V2,
                bound_from_daa: ctx.daa_score,
                readers: declaration.readers.clone(),
            };
            let bytes = borsh::to_vec(&row).map_err(|_| PalwStateV2Error::ImmutableModelUpdate("router binding row encoding"))?;
            builder.write_kernel_row(PALW_ROUTER_BINDING_TABLE_V2, palw_router_binding_key_v2(&declaration.class_id), Some(bytes));
            Ok(())
        }
        // The one-move accusation: the framing checks of tag 243's arm, then consumer v2's terminal, then the same settlement.
        PalwMemoryRoutingObjectV2::Accused { accusation } => {
            let claim_id = accusation.claim;
            let source = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
            let _claim_copy = crate::palw_native_resource_v1::reserve_native_requests_v1(
                crate::palw_native_resource_v1::native_value_requests_v1(source).map_err(PalwStateV2Error::NativeResource)?,
            )
            .map_err(PalwStateV2Error::NativeResource)?;
            let claim = source.clone();
            let ladder = builder.state.class_step_ladder_v1(&claim.class_id, limits.ladder);
            accusation.validate_shape(ladder).map_err(|e| PalwStateV2Error::ShardCourt(e.into()))?;
            if accusation.encoded_bytes().map_err(|e| PalwStateV2Error::ShardCourt(e.into()))? > limits.max_close_bytes {
                return Err(PalwStateV2Error::ShardCourt("memory/routing accusation exceeds the current court close ceiling".into()));
            }
            let reserved_final = matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
                && builder.state.legacy_dispute_reservation_v1(&claim_id, &accusation.accuser_bond).is_some();
            if claim.phase.is_terminal() && !reserved_final {
                return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "MemoryRoutingV2" });
            }
            if accusation.executor_bond != claim.bond {
                return Err(PalwStateV2Error::ShardCourtExecutorIsNotTheClaims(claim_id));
            }
            if accusation.execution_root != claim.execution_root || accusation.trace_root != claim.trace_root {
                return Err(PalwStateV2Error::ShardCourtRootsDiffer(claim_id));
            }
            let accuser = accusation.accuser_bond;
            if accuser == claim.bond {
                return Err(PalwStateV2Error::ShardCourtAccuserIsTheProducer(accuser));
            }
            if !builder.extras.offence_attribution_active
                && let Some((session, _)) = builder.state.court_sessions.iter().find(|(_, row)| row.claim == claim_id)
            {
                return Err(PalwStateV2Error::ShardCourtClaimUnderSession { claim: claim_id, session: *session });
            }
            let record = builder.state.bonds.get(&accuser).ok_or(PalwStateV2Error::MissingBond(accuser))?;
            if !matches!(record.status, PalwBondStatusV2::Active) {
                return Err(PalwStateV2Error::BondNotActive(accuser));
            }
            let floor = builder.params.min_collateral_sompi();
            if record.collateral < floor {
                return Err(PalwStateV2Error::BondBelowFloor { bond: accuser, collateral: record.collateral, floor });
            }
            let artifact_root =
                builder.state.classes.get(&claim.class_id).ok_or(PalwStateV2Error::MissingClass(claim.class_id))?.artifact_root;
            let actual = crate::palw_shard_court_v1::PalwOneMoveClaimV2 {
                execution_root: claim.execution_root,
                class_id: claim.class_id,
                artifact_root,
            };
            let binding_row = palw_router_binding_row_of_v2(&builder.state, &claim.class_id);
            let verdict =
                palw_memory_routing_verdict_v2(accusation, &actual, ladder, binding_row.as_ref(), claim.accepted_daa).map_err(|e| match e {
                    crate::palw_step_refute::memory_v2::PalwGdnCourtErrorV2::Resource(cause) => PalwStateV2Error::NativeResource(cause),
                    other => PalwStateV2Error::ShardCourt(other.to_string()),
                })?;
            match verdict {
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::ExecutorGuilty => {
                    builder.convict_by_court_verdict_v1(ctx, claim_id, &claim, accuser, None)?
                }
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::FalseAccusation => {
                    let charge = crate::palw_shard_court_v1::palw_shard_court_false_accusation_charge_v1(claim.reserved, floor);
                    builder.slash_seat(accuser, charge, floor)?;
                }
                crate::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection => {
                    return Err(PalwStateV2Error::ShardCourt("consumer v2 must never invent an attention dissection".into()));
                }
            }
            Ok(())
        }
    }
}
