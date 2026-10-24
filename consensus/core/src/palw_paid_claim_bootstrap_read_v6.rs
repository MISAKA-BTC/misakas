//! Fail-closed native bootstrap inputs. Absence is an exact, wholly unfunded
//! enrollment, never a codec/binding/capacity failure. No activation is enabled.
use super::*;
use crate::palw_beacon_source_fee_v1::{
    PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5 as COST, PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5 as FEE, PaidBeaconMaterialV5,
    PalwBeaconSourceCostBoundsV1,
};
use crate::palw_beacon_source_opt_in_v1::PALW_BEACON_SOURCE_TABLE_TARGETS_V4 as TARGET;
use crate::palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1};
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1 as E, NativeDemandLeaseV1, NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1,
    preflight_native_value_v1, reserve_native_requests_v1,
};
use misaka_palw_challenge::{OptInSourceV4, PaidSourcePromiseV5};
use misaka_palw_kernel::{beacon_source::BeaconSourceTargetV1, beacon_source_fee::BeaconSourceFeeTargetV1, ledger::KernelLedgerV1};
fn bad(why: impl Into<String>) -> E<String> {
    E::Decode(why.into())
}

#[cfg(test)]
thread_local! { static CHECKER: std::cell::RefCell<Option<std::sync::Arc<crate::palw_native_resource_v1::NativeDemandCheckerV1>>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_checker_v6<T>(
    checker: std::sync::Arc<crate::palw_native_resource_v1::NativeDemandCheckerV1>,
    run: impl FnOnce() -> T,
) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CHECKER.with(|c| *c.borrow_mut() = None);
        }
    }
    CHECKER.with(|c| {
        assert!(c.borrow().is_none());
        *c.borrow_mut() = Some(checker);
    });
    let _reset = Reset;
    run()
}
fn reserve(bytes: u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1> {
    #[cfg(test)]
    if let Some(result) = CHECKER.with(|c| c.borrow().as_ref().map(|c| c.reserve_requests_v1(bytes, 0).map(Some))) {
        return result;
    }
    reserve_native_requests_v1(bytes)
}
fn read<T: NativeWireScanV1 + BorshDeserialize>(
    route: &PalwKernelRouteStateV1,
    table: u8,
    key: &Digest,
) -> Result<Option<NativeRetainedV1<T>>, E<String>> {
    let Some(bytes) =
        crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, table, key).map_err(E::Resource)?
    else {
        return Ok(None);
    };
    let shape = preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map_err(|e| bad(e.to_string()))?;
    let lease = reserve(shape.logical_requests_v1()).map_err(E::Resource)?;
    let row = borsh::from_slice(bytes).map_err(|e| bad(e.to_string()))?;
    Ok(Some(NativeRetainedV1::from_owner_reserved_v1(row, lease)))
}
impl PalwKernelRouteStateV1 {
    /// Historical closure stays historical: no live-claim revalidation can reopen
    /// bootstrap after the first admitted positive-quorum Final. Invalid bytes
    /// and mismatched keys are errors, not evidence that no approved Final exists.
    pub(crate) fn paid_claim_approved_final_exists_checked_v6(&self) -> Result<bool, E<String>> {
        let mut found = false;
        for ((_, key), _) in self.aux.range((PAID_CLAIM_FINALS_V2, Vec::new())..(PAID_CLAIM_FINALS_V2 + 1, Vec::new())) {
            let id: Digest = key.as_slice().try_into().map_err(|_| bad("paid Final key is not a digest"))?;
            let row = read::<PalwPaidClaimFinalV2>(self, PAID_CLAIM_FINALS_V2, &id)?.ok_or_else(|| bad("paid Final disappeared"))?;
            if row.enrollment != id {
                return Err(bad("paid Final key/enrollment mismatch"));
            }
            found |= row.complete_receipts > 0 && row.completed_rounds > 0;
        }
        Ok(found)
    }
    /// Native caller supplies its already checked ledger. No nested String
    /// projection, discarded errors, or arbitrary alternate randomness mode.
    pub(crate) fn paid_claim_bootstrap_material_checked_v6(
        &self,
        a: &PalwPaidClaimAuthorityV2,
        ledger: &KernelLedgerV1,
    ) -> Result<Option<NativeRetainedV1<PaidBeaconMaterialV5>>, E<String>> {
        let context = a.context.id();
        let target = read::<BeaconSourceTargetV1>(self, TARGET, &context)?;
        let fee = read::<BeaconSourceFeeTargetV1>(self, FEE, &context)?;
        let cost = read::<PalwBeaconSourceCostBoundsV1>(self, COST, &context)?;
        let source_range = (context, [0; 64])..=(context, [255; 64]);
        let source_count = ledger.beacon_sources.range(source_range.clone()).count();
        let promise_count = ledger.beacon_source_fee_promises.range(source_range.clone()).count();
        let funding = ledger.beacon_source_fees.get(&context);
        if a.sponsor.is_none()
            && target.is_none()
            && fee.is_none()
            && cost.is_none()
            && funding.is_none()
            && source_count == 0
            && promise_count == 0
        {
            return Ok(None);
        }
        let sponsor = a.sponsor.ok_or_else(|| bad("unfunded paid authority has source/funding rows"))?;
        let target = target.ok_or_else(|| bad("paid source target missing"))?;
        let fee = fee.ok_or_else(|| bad("paid source fee target missing"))?;
        let cost = cost.ok_or_else(|| bad("paid source cost bounds missing"))?;
        target.validate().map_err(bad)?;
        if *target != a.source_target
            || target.terms.context != context
            || target.context != a.context
            || !target.context.policy.is_paid_opt_in_source()
            || fee.target != *target
            || fee.sponsor != palw_kernel_bond_id_v1(&sponsor)
            || fee.minimum_fee == 0
            || fee.max_sources < target.context.policy.work_count_k
            || fee.max_sources > target.terms.max_total
            || fee.minimum_fee < cost.minimum_gross_fee(&target.terms).map_err(bad)?
        {
            return Err(bad("paid source target/fee/cost authority mismatch"));
        }
        let funded = funding.ok_or_else(|| bad("funded paid authority lacks prepaid backing"))?;
        if funded.target != *target
            || funded.terms.sponsor != fee.sponsor
            || funded.terms.source != target.terms
            || funded.terms.fee_per_source < fee.minimum_fee
            || funded.terms.max_sources < target.context.policy.work_count_k
            || funded.terms.max_sources > fee.max_sources
        {
            return Err(bad("paid source funding replaces frozen terms"));
        }
        if source_count > target.terms.max_total as usize || promise_count > target.terms.max_total as usize {
            return Err(bad("paid source inventory exceeds its frozen capacity"));
        }
        let mut requests = TirNativeRequestsV1::new();
        requests.exact_v1::<OptInSourceV4>(source_count as u64).map_err(E::Resource)?;
        requests.exact_v1::<PaidSourcePromiseV5>(promise_count as u64).map_err(E::Resource)?;
        // Retain the concrete collector scratch bound through the caller's seal-reveal
        // or Final-based draw. Event exports have no dependency Vec entries, exactly
        // like the existing RFC07 adapter; each collector's owning Vec/sets is bounded
        // by these actual frozen inventory counts, not by encoded byte lengths.
        use crate::palw_native_resource_v1::BTreeNodeEnvelopeV1;
        use misaka_palw_challenge::{BeaconSourceV1, SealRevealV3, SealedSourceV3, WorkFinalEventV1};
        let n = source_count as u64;
        requests.vec_v1::<SealedSourceV3>(n).map_err(E::Resource)?;
        for _ in 0..3 {
            requests.vec_v1::<&SealedSourceV3>(n).map_err(E::Resource)?;
        }
        for _ in 0..3 {
            requests.exact_v1::<BTreeNodeEnvelopeV1<Digest, ()>>(n).map_err(E::Resource)?;
        }
        requests.exact_v1::<BTreeNodeEnvelopeV1<Digest, &PaidSourcePromiseV5>>(promise_count as u64).map_err(E::Resource)?;
        requests.vec_v1::<(&SealedSourceV3, SealRevealV3, WorkFinalEventV1)>(n).map_err(E::Resource)?;
        requests.vec_v1::<BeaconSourceV1>(n).map_err(E::Resource)?;
        requests.vec_v1::<Digest>(n + 1).map_err(E::Resource)?;
        requests.vec_v1::<(Digest, u64, Digest)>(n).map_err(E::Resource)?;
        let lease = reserve(requests.bytes_v1()).map_err(E::Resource)?;
        let mut sources = Vec::new();
        sources
            .try_reserve_exact(source_count)
            .map_err(|_| E::Resource(NativeCoverageV1::unsupported::<OptInSourceV4>("native paid sources", "allocator refusal")))?;
        for ((id, _), row) in ledger.beacon_sources.range(source_range.clone()) {
            if *id != context || row.terms != target.terms {
                return Err(bad("source reservation differs from frozen target"));
            }
            let reveal = if let Some((claim, position)) = row.revealed {
                let event = if let Some(receipt) = ledger.final_receipt(&claim) {
                    if ledger.paid_claim_consumers_v2.contains_key(&claim)
                        && crate::palw_exec_v2_verify::palw_exec_v2_selected_approval_v1(self, &crate::Hash64::from_bytes(claim))
                    {
                        let settlement = crate::palw_kernel_settlement_v1::palw_kernel_row_settlement_v1(ledger, &claim, ledger.daa)
                            .unwrap_or(crate::palw_kernel_settlement_v1::PalwKernelSettlementV1::Pending);
                        crate::palw_kernel_route_v1::selected_approval_work_v6(ledger, &receipt, settlement)
                            .map_err(bad)?
                            .map(|w| w.event)
                    } else {
                        None
                    }
                } else {
                    None
                };
                use misaka_palw_kernel::lifecycle::ClaimStateV1 as C;
                let fate = match (ledger.claims.get(&claim), event) {
                    (_, Some(event)) => misaka_palw_challenge::SourceFateV3::Final(event),
                    (Some(c), _) if c.convicted || matches!(c.life.state, C::Unavailable { .. } | C::TimedOut { .. }) => {
                        misaka_palw_challenge::SourceFateV3::Failed
                    }
                    (Some(_), _) => misaka_palw_challenge::SourceFateV3::Live,
                    _ => misaka_palw_challenge::SourceFateV3::Failed,
                };
                let salt = *ledger.claim_beacon_salts.get(&claim).ok_or_else(|| bad("revealed source lacks its signed salt"))?;
                Some(misaka_palw_challenge::SealRevealV3 { reveal_position: position, salt, fate })
            } else {
                None
            };
            sources.push(OptInSourceV4 {
                context_id: context,
                terms_id: row.terms.id(),
                opted_in_position: row.opted_in_daa,
                forfeited_position: match row.outcome {
                    Some(misaka_palw_kernel::beacon_source::BeaconSourceOutcomeV1::Forfeited { daa }) => Some(daa),
                    _ => None,
                },
                source: misaka_palw_challenge::SealedSourceV3 {
                    source_profile_id: row.profile,
                    attribution: misaka_palw_challenge::SourceAttributionV1 {
                        producer_id: row.producer,
                        consumer_id: misaka_palw_challenge::RootV1::Present(row.poster),
                    },
                    seal: row.seal,
                    seal_position: row.sealed_daa,
                    reveal,
                },
            });
        }
        let mut promises = Vec::new();
        promises.try_reserve_exact(promise_count).map_err(|_| {
            E::Resource(NativeCoverageV1::unsupported::<PaidSourcePromiseV5>("native paid promises", "allocator refusal"))
        })?;
        promises.extend(ledger.beacon_source_fee_promises.range(source_range).map(|((ctx, producer), p)| PaidSourcePromiseV5 {
            context_id: *ctx,
            producer: *producer,
            fee_terms_id: p.terms_id,
            accepted_position: p.accepted_daa,
            fee: p.fee,
        }));
        let contract = misaka_palw_challenge::PaidOptInContractV5 {
            source: misaka_palw_challenge::OptInContractV4 {
                context_id: context,
                terms_id: funded.terms.source.id(),
                source_final_by: funded.terms.source.final_by,
            },
            fee_terms_id: funded.terms.id(),
            minimum_fee: fee.minimum_fee,
            fee_per_source: funded.terms.fee_per_source,
            max_sources: funded.terms.max_sources,
            funded_position: funded.funded_daa,
        };
        misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(&a.context, &contract, &sources, &promises, ledger.daa)
            .map_err(|e| bad(e.to_string()))?;
        Ok(Some(NativeRetainedV1::from_owner_reserved_v1((contract, sources, promises), lease)))
    }
}
