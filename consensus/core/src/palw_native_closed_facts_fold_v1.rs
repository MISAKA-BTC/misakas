//! Owning, journalled promotion into compact closed-work accounting. No wire
//! object can append facts; callers first validate the actual source's closure.
use super::*;
use crate::palw_native_closed_facts_v1::*;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1};
use crate::palw_native_settlement_v1::MatureUsefulWorkV1;
fn bad(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
pub(super) fn append_native_closed_fact_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    fact: MatureUsefulWorkV1,
    origin: NativeClosedFactOriginV1,
) -> Result<(), PalwStateV2Error> {
    let row = NativeClosedFactRowV1 { version: 1, fact, origin, closed_at: *ctx };
    row.validate_v1(fact.identity)?;
    if fact.class == b.params.base_class_id() {
        return Err(bad("BASE work cannot enter native closed settlement credit"));
    }
    if let Some(archive) = crate::palw_native39_settlement_checkpoint_v1::read_native39_settlement_checkpoint_v1(&b.state)?
        && fact.accepted_daa < archive.effect.daa_score
        && fact.accepted_blue < archive.effect.blue_score
    {
        return Ok(());
    }
    let key = fact.identity.as_bytes().to_vec();
    if let Some(bytes) = b.state.kernel_route_state_v1().and_then(|route| route.aux.get(&(NATIVE_CLOSED_FACTS_TABLE_V1, key.clone())))
    {
        let old: NativeRetainedV1<NativeClosedFactRowV1> =
            crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(bytes, || borsh::from_slice(bytes)).map_err(
                |e| match e {
                    NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
                    NativeDecodeErrorV1::Decode(e) => bad(e),
                },
            )?;
        old.validate_v1(fact.identity)?;
        if old.fact != fact {
            return Err(bad("conflicting canonical closed-work identity"));
        }
        return Ok(());
    }
    b.write_kernel_row_encoded_v1(NATIVE_CLOSED_FACTS_TABLE_V1, &key, Some(&row))
}
pub(super) fn promote_native39_closed_facts_v1(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_native39_frontier_v1::*;
    let anchors = native39_frontier_anchors_v1(&b.state)?;
    for (_, anchor) in anchors.iter() {
        let Some(fact) = native39_frontier_fact_v1(&b.state, b.params, anchor, ctx)? else { continue };
        if b.params.claim_local_settlement_active_at(ctx.daa_score)
            && !super::palw_claim_local_native39_frontier_eligible_v1(&b.state, b.params, ctx, anchor.claim, anchor)?
        {
            continue;
        }
        let bytes = borsh::object_length(anchor).map_err(bad)? as u64;
        let _scratch = crate::palw_native_resource_v1::reserve_native_requests_v1(
            bytes.checked_mul(2).ok_or(PalwStateV2Error::Overflow("closed native source manifest"))?,
        )
        .map_err(PalwStateV2Error::NativeResource)?;
        let manifest =
            Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/closed-native-source-manifest/v1", anchor));
        let once = crate::palw_settlement_once_v1::PalwSettlementObligationV1::with_party(
            crate::palw_settlement_once_v1::PalwSettlementLegV1::NativeReceiptSpend,
            anchor.receipt.as_bytes(),
            [0; 64],
            u64::from(anchor.quantum),
        );
        let origin = NativeClosedFactOriginV1 {
            family: ClosedFactFamilyV1::NativeMining39,
            subject: anchor.claim,
            work: anchor.work,
            ruleset: b.extras.native_fact_ruleset.ok_or_else(|| bad("closed native fact network ruleset context absent"))?,
            canonical_work_daa: b.extras.canonical_work_daa,
            quantum: Some(anchor.quantum),
            approval: Some(anchor.approval),
            source_manifest: manifest,
            once_obligation: Some(Hash64::from_bytes(once.id_v1())),
            source_point: *ctx,
            source_state_root: parent.state_root(),
            terminal_daa: anchor.established_daa,
            material_until_daa: anchor
                .expires
                .saturating_add(
                    b.params.rfc0018_v1().ok_or_else(|| bad("native closed fact certification absent"))?.certification.lifetime_daa(),
                )
                .saturating_add(b.params.rfc0018_v1().unwrap().certification.prosecution_window_daa),
            quantum_maturity_daa: anchor.quantum_maturity_daa,
            retirement_daa: b.params.claim_retirement_daa(),
        };
        append_native_closed_fact_v1(b, ctx, fact, origin)?;
    }
    Ok(())
}
pub(super) fn sweep_native_closed_facts_v1(b: &mut TransitionBuilder<'_>) -> Result<(), PalwStateV2Error> {
    let Some(archive) = crate::palw_native39_settlement_checkpoint_v1::read_native39_settlement_checkpoint_v1(&b.state)? else {
        return Ok(());
    };
    let facts = read_native_closed_facts_v1(&b.state)?;
    for (id, row) in facts.iter() {
        if row.fact.accepted_daa < archive.effect.daa_score && row.fact.accepted_blue < archive.effect.blue_score {
            b.write_kernel_row(NATIVE_CLOSED_FACTS_TABLE_V1, id.as_bytes().to_vec(), None)?;
        }
    }
    Ok(())
}
