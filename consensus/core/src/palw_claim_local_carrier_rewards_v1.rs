//! Iterative traversal of retained carrier and prefix dependency births. A raw
//! claim id is never resolved against a newer live acceptance of that same id.
use super::*;
use crate::palw_native_resource_v1::{NativeDemandLeaseV1, reserve_native_requests_v1};

type Birth = (Hash64, Hash64, u64, u64);
type Entry = (Option<PalwClaimLocalAdmissionContextV1>, Option<NativeDemandLeaseV1>);

fn enqueue(pending: &mut BTreeMap<Birth, Entry>, context: PalwClaimLocalAdmissionContextV1) -> Result<(), PalwStateV2Error> {
    let point = context.accepting_point();
    let key = (context.claim_id(), point.block, point.daa_score, point.blue_score);
    if pending.contains_key(&key) {
        return Ok(());
    }
    let demand = crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<Birth, Entry>();
    let lease = reserve_native_requests_v1(demand as u64).map_err(PalwStateV2Error::NativeResource)?;
    pending.insert(key, (Some(context), lease));
    Ok(())
}

/// Resolve against the consumer's retained parent. A retired source is located
/// by the exact winning-carrier write retained in its real admission frame.
fn carrier_source_origin(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    consumer: &PalwClaimLocalAdmissionContextV1,
    source_id: Hash64,
) -> Result<Option<PalwClaimLocalAdmissionContextV1>, PalwStateV2Error> {
    let born = consumer.original_claim();
    frame_read(consumer.frame(), |prior, _current| {
        if let Some(row) = prior.claims.get(&source_id) {
            if source_id == consumer.claim_id()
                || row.accepted_daa > born.accepted_daa
                || row.accepted_blue_score >= born.accepted_blue_score
            {
                return Err(bad("consumed carrier dependency is not causally prior"));
            }
            return origin_for(state, params, source_id, row);
        }
        use crate::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1;
        let Some(winner) = prior
            .kernel_route
            .as_ref()
            .and_then(|route| route.aux.get(&(PALW_FP_WINNING_CARRIER_TABLE_V1, source_id.as_byte_slice().to_vec())))
        else {
            return Ok(None);
        };
        let mut found = None;
        for (key, bytes) in frames(state) {
            let frame = frame_checked(state, key, bytes)?;
            if frame.point.daa_score > born.accepted_daa || frame.point.blue_score >= born.accepted_blue_score {
                continue;
            }
            let wrote_winner = frame.native_changes.iter().any(|change| {
                change.owner == PalwClaimLocalRowOwnerV1::KernelRoute
                    && change.table == PALW_FP_WINNING_CARRIER_TABLE_V1
                    && change.key.as_slice() == source_id.as_byte_slice()
                    && change.new.as_deref() == Some(winner.as_slice())
            });
            if !wrote_winner {
                continue;
            }
            if let Some(context) = state.claim_local_admission_context_v1(params, &frame.point, source_id)? {
                if found.is_some() {
                    return Err(bad("consumed carrier has ambiguous retained births"));
                }
                found = Some(context);
            }
        }
        Ok(found)
    })
}

pub(super) fn readiness(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: u64,
    id: Hash64,
    born: &PalwClaimStateV2,
    target: Hash64,
    target_birth: &PalwClaimStateV2,
    retiring: bool,
) -> Result<RewardReadinessV1, PalwStateV2Error> {
    let Some(origin) = origin_for(state, params, id, born)? else {
        return Ok(RewardReadinessV1::Hold(u64::MAX));
    };
    let mut pending = BTreeMap::new();
    enqueue(&mut pending, origin)?;
    let mut result = RewardReadinessV1::Ready;
    while let Some(cursor) = pending.values_mut().find_map(|(cursor, _lease)| cursor.take()) {
        let cursor_id = cursor.claim_id();
        let cursor_birth = cursor.original_claim();
        result = result.join(source(state, params, now, cursor_id, cursor_birth, target, target_birth, retiring)?);
        result = result.join(prefix_rewards::readiness(state, params, now, cursor_id, cursor_birth, target, target_birth, retiring)?);
        if result == RewardReadinessV1::Denied {
            return Ok(result);
        }
        for input in &cursor.frame().inputs {
            let PalwClaimLocalInputV1::ConsumedCarrier { consumer, source: source_id } = input else { continue };
            if *consumer != cursor_id {
                continue;
            }
            // Admission derives these edges from the parent's winning-carrier
            // index. Its source row is the parent's exact acceptance, including
            // sources that have since retired or been re-admitted elsewhere.
            let next = carrier_source_origin(state, params, &cursor, *source_id)?;
            if let Some(next) = next {
                enqueue(&mut pending, next)?;
            } else if let Some(decision)=super::super::palw_claim_local_compact_sources_v1::carrier(state,&cursor,*source_id)?{
                result=result.join(decision);
            } else {
                result = result.join(RewardReadinessV1::Hold(u64::MAX));
            }
        }
        // Prefix traversal checks its own signed finite bounds and convictions.
        // Also visit each prefix source here so its carrier dependencies are not
        // skipped in a mixed prefix/carrier chain.
        if let Some(consumed) = prefix_rewards::prefix_use(&cursor)? {
            let identity = consumed.intent.source;
            let point = match state.claim_local_prefix_source_point_v1(&identity) {
                Ok(point) => point,
                Err(PalwStateV2Error::NativeResource(error)) => return Err(PalwStateV2Error::NativeResource(error)),
                Err(_) => {
                    result = result.join(RewardReadinessV1::Hold(u64::MAX));
                    continue;
                }
            };
            if let Some(next) = state.claim_local_admission_context_v1(params, &point, identity.source_claim)? {
                enqueue(&mut pending, next)?;
            } else {
                result = result.join(RewardReadinessV1::Hold(u64::MAX));
            }
        }
    }
    Ok(result)
}
