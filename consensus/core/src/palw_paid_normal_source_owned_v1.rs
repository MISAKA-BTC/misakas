//! Retained typed reader for the existing normal V5 source algorithm.
//! This does not certify source independence or authorize activation.
//! Existing validators' internal temporary allocations remain a separate audit;
//! the owners here cover decoded rows and the constructed inventory collections.
use crate::palw_beacon_source_fee_v1::{PaidBeaconMaterialV5, PalwBeaconSourceCostBoundsV1};
use crate::palw_claim_challenge_v2::*;
use crate::palw_claim_interactive_v3::PalwPaidClaimRoundV3;
use crate::palw_kernel_route_v1::{KernelFinalReadV1, PalwKernelRouteStateV1, panel_record_v1};
use crate::palw_native_resource_v1::{
    BTreeNodeEnvelopeV1, NativeCoverageV1, NativeDecodeErrorV1 as E, NativeDemandLeaseV1, NativeRetainedV1, NativeWireScanV1,
    TirNativeRequestsV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_classified_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{beacon_source::BeaconSourceTargetV1, beacon_source_fee::BeaconSourceFeeTargetV1, hash::Digest};
use std::{collections::BTreeMap, io, mem::size_of, ops::Deref};

fn bad(why: impl Into<String>) -> E<String> {
    E::Decode(why.into())
}
fn cap(why: impl std::fmt::Display) -> E<String> {
    E::Resource(NativeCoverageV1::unsupported::<Vec<u8>>("normal source construction", &why.to_string()))
}
fn mul(a: u64, b: u64) -> Result<u64, E<String>> {
    a.checked_mul(b).ok_or_else(|| E::Resource(NativeCoverageV1::overflow::<u64>("normal source quote")))
}
struct Count(u64);
impl io::Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.checked_add(bytes.len() as u64).ok_or_else(|| io::Error::other("normal source codec count overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encoded<T: BorshSerialize>(value: &T) -> Result<u64, E<String>> {
    let mut count = Count(0);
    borsh::to_writer(&mut count, value).map_err(cap)?;
    Ok(count.0)
}
// borsh 1.5/1.6/1.8 helper starts with capacity 1024. Doubling bounds
// geometric growth; this is a pinned codec allocation rule, not wire -> RAM.
fn codec_capacity(bytes: u64) -> Result<u64, E<String>> {
    Ok(mul(bytes, 2)?.max(1024))
}
fn row<T: NativeWireScanV1 + BorshDeserialize, const N: usize>(
    route: &PalwKernelRouteStateV1,
    table: u8,
    key: &[u8; N],
) -> Result<Option<NativeRetainedV1<T>>, E<String>> {
    let Some(raw) =
        crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, table, key).map_err(E::Resource)?
    else {
        return Ok(None);
    };
    // Codec shape failure is Decode. Actual reservation failure remains Resource.
    preflight_native_value_v1::<T>(raw, raw.len() as u64).map_err(|e| bad(e.to_string()))?;
    retain_native_value_decode_classified_v1(raw, || borsh::from_slice(raw).map_err(|e| e.to_string())).map(Some)
}

/// Retain the actual authority / round rather than allocating a flattened clone
/// of both BeaconContexts and their profile sets after their tickets expire.
pub enum Rfc07NormalSourceAuthorityV3 {
    Initial(NativeRetainedV1<PalwPaidClaimAuthorityV2>),
    Staged { authority: NativeRetainedV1<PalwPaidClaimAuthorityV2>, round: NativeRetainedV1<PalwPaidClaimRoundV3> },
}
impl Rfc07NormalSourceAuthorityV3 {
    pub fn context(&self) -> &misaka_palw_challenge::BeaconContextV1 {
        match self {
            Self::Initial(a) => &a.context,
            Self::Staged { round, .. } => &round.context,
        }
    }
    pub fn target(&self) -> &BeaconSourceTargetV1 {
        match self {
            Self::Initial(a) => &a.source_target,
            Self::Staged { round, .. } => &round.target,
        }
    }
    pub fn authority(&self) -> &PalwPaidClaimAuthorityV2 {
        match self {
            Self::Initial(a) | Self::Staged { authority: a, .. } => a,
        }
    }
    pub fn authorized_daa(&self) -> u64 {
        match self {
            Self::Initial(a) => a.authorized_daa,
            Self::Staged { round, .. } => round.accepted_daa,
        }
    }
    pub fn sponsor(&self) -> Option<crate::palw_state_v2::PalwBondKeyV2> {
        match self {
            Self::Initial(a) => a.sponsor,
            Self::Staged { round, .. } => round.sponsor,
        }
    }
}

pub struct Rfc07NormalPaidMaterialV5 {
    value: PaidBeaconMaterialV5,
    _sources: Option<NativeDemandLeaseV1>,
    _promises: Option<NativeDemandLeaseV1>,
}
impl Deref for Rfc07NormalPaidMaterialV5 {
    type Target = PaidBeaconMaterialV5;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl PalwKernelRouteStateV1 {
    pub fn rfc07_normal_source_target_checked_v4(
        &self,
        context: &Digest,
    ) -> Result<NativeRetainedV1<BeaconSourceTargetV1>, E<String>> {
        use crate::palw_beacon_source_opt_in_v1::PALW_BEACON_SOURCE_TABLE_TARGETS_V4;
        let target: NativeRetainedV1<BeaconSourceTargetV1> =
            row(self, PALW_BEACON_SOURCE_TABLE_TARGETS_V4, context)?.ok_or_else(|| bad("the frozen source target is missing"))?;
        target.validate().map_err(bad)?;
        if target.terms.context != *context || !target.context.policy.is_opt_in_source() {
            return Err(bad("stored source target context mismatch"));
        }
        Ok(target)
    }
    pub fn rfc07_normal_fee_target_checked_v5(
        &self,
        context: &Digest,
    ) -> Result<NativeRetainedV1<BeaconSourceFeeTargetV1>, E<String>> {
        use crate::palw_beacon_source_fee_v1::{PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5, PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5};
        let target: NativeRetainedV1<BeaconSourceFeeTargetV1> =
            row(self, PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5, context)?.ok_or_else(|| bad("frozen paid source target missing"))?;
        target.target.validate().map_err(bad)?;
        if target.target != *self.rfc07_normal_source_target_checked_v4(context)?
            || !target.target.context.policy.is_paid_opt_in_source()
            || target.minimum_fee == 0
            || target.max_sources < target.target.context.policy.work_count_k
            || target.max_sources > target.target.terms.max_total
        {
            return Err(bad("frozen mandatory fee target mismatch/floor/cap"));
        }
        let bounds: NativeRetainedV1<PalwBeaconSourceCostBoundsV1> = row(self, PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5, context)?
            .ok_or_else(|| bad("frozen paid source cost bounds missing"))?;
        if target.minimum_fee < bounds.minimum_gross_fee(&target.target.terms).map_err(bad)? {
            return Err(bad("frozen source fee does not cover the cost bounds and minimum net receipt"));
        }
        Ok(target)
    }
    pub fn rfc07_normal_source_authority_checked_v3(&self, context: &Digest) -> Result<Rfc07NormalSourceAuthorityV3, E<String>> {
        // A present corrupt or capacity-refused primary is never a version probe.
        if let Some(a) = row::<PalwPaidClaimAuthorityV2, _>(self, PAID_CLAIM_ENROLLMENTS_V2, context)? {
            a.validate(&*self.ledger_checked_v1()?).map_err(bad)?;
            if a.class.source_policy.interactive_mode == misaka_palw_challenge::InteractiveModeV1::StagedBeacon {
                return Err(bad("staged source needs a producer message and fresh round target"));
            }
            if a.context.id() != *context || *self.rfc07_normal_source_target_checked_v4(context)? != a.source_target {
                return Err(bad("initial source target mismatch"));
            }
            return Ok(Rfc07NormalSourceAuthorityV3::Initial(a));
        }
        let mut key = [0; 65];
        key[0] = PAID_CLAIM_ID_INDEX_PREFIX_V3;
        key[1..].copy_from_slice(context);
        let index: NativeRetainedV1<PalwPaidClaimIndexV3> =
            row(self, PAID_CLAIM_ROUNDS_V2, &key)?.ok_or_else(|| bad("paid staged context index missing"))?;
        if index.version != 3 || index.kind != PalwPaidClaimIndexKindV3::StagedContext || index.owner != *context {
            return Err(bad("paid staged context index invalid"));
        }
        let round: NativeRetainedV1<PalwPaidClaimRoundV3> =
            row(self, PAID_CLAIM_ROUNDS_V2, &index.target)?.ok_or_else(|| bad("staged source index target missing"))?;
        let a = self.paid_claim_enrollment_checked_v6(&round.enrollment)?;
        self.validate_paid_round_v4(&round, &a, &*self.ledger_checked_v1()?).map_err(bad)?;
        if round.id() != index.target
            || round.context.id() != *context
            || *self.rfc07_normal_source_target_checked_v4(context)? != round.target
        {
            return Err(bad("staged source index substitutes its round/context/target"));
        }
        Ok(Rfc07NormalSourceAuthorityV3::Staged { authority: a, round })
    }

    pub fn rfc07_normal_finals_owned_v1(&self) -> Result<NativeRetainedV1<Vec<KernelFinalReadV1>>, E<String>> {
        use misaka_palw_challenge::{AttributedWorkV1, FinalPathV1, RootV1, SourceAttributionV1, WorkFinalEventV1, WorkSourceKindV1};
        use misaka_palw_kernel::opv::FinalReceiptV1;
        let ledger = self.ledger_native_projection_checked_v9()?;
        let count = ledger.claims.len() as u64;
        // Maximum fixed event schema; current canonical event has empty
        // depends_on_profiles and PanelIndependent, hence never exceeds this.
        let exemplar = AttributedWorkV1 {
            event: WorkFinalEventV1 {
                kind: WorkSourceKindV1::RealUsefulWork,
                source_profile_id: [0; 64],
                canonical_work_id: [0; 64],
                execution_commitment: [0; 64],
                accepted_position: 0,
                settlement_position: 0,
                occurrence_index: 0,
                claim_final: false,
                da_satisfied: false,
                validity_independent: false,
                depends_on_profiles: Vec::new(),
                final_path: FinalPathV1::PanelLicensed { panel_seed_id: [0; 64], panel_epoch: 0 },
            },
            attribution: SourceAttributionV1 { producer_id: [0; 64], consumer_id: RootV1::Present([0; 64]) },
        };
        let event_bytes = codec_capacity(encoded(&exemplar)?)?;
        let mut requests = TirNativeRequestsV1::new();
        requests.vec_v1::<FinalReceiptV1>(count).map_err(E::Resource)?;
        // final_receipts uses stable sort_by_key: reserve one full receipt
        // population as a conservative sort scratch upper bound.
        requests.exact_v1::<FinalReceiptV1>(count).map_err(E::Resource)?;
        requests.exact_v1::<KernelFinalReadV1>(count).map_err(E::Resource)?;
        requests.add_v1(mul(count, event_bytes)?).map_err(E::Resource)?;
        // Panel record has a fixed receipt, label and optional producer/payer.
        // Count each actual receipt without encoding or growing an output Vec.
        for claim in ledger.claims.keys() {
            if let Some(receipt) = ledger.final_receipt(claim) {
                let record_bytes = encoded(&(&receipt, "final-revocable", Some([0u8; 64]), Some([0u8; 64])))?;
                requests.add_v1(codec_capacity(record_bytes)?).map_err(E::Resource)?;
            }
        }
        let owner = reserve_native_requests_v1(requests.bytes_v1()).map_err(E::Resource)?;
        let receipts = ledger.final_receipts();
        let mut out = Vec::new();
        out.try_reserve_exact(receipts.len()).map_err(cap)?;
        for receipt in receipts {
            let settlement = crate::palw_kernel_settlement_v1::palw_kernel_row_settlement_v1(&ledger, &receipt.claim, ledger.daa)
                .unwrap_or(crate::palw_kernel_settlement_v1::PalwKernelSettlementV1::Pending);
            let (final_path, event) = self.final_export_v1(&ledger, &receipt, settlement).map_err(bad)?;
            let statement = receipt.assurance.statement();
            let panel_record = panel_record_v1(&ledger, &receipt, final_path, settlement);
            out.push(KernelFinalReadV1 { receipt, final_path, event, statement, settlement: settlement.label(), panel_record });
        }
        Ok(NativeRetainedV1::from_owner_reserved_v1(out, owner))
    }

    pub fn rfc07_normal_opt_in_sources_owned_v4(
        &self,
        context: &Digest,
    ) -> Result<NativeRetainedV1<Vec<misaka_palw_challenge::OptInSourceV4>>, E<String>> {
        use misaka_palw_challenge::{
            OptInSourceV4, RootV1, SealRevealV3, SealedSourceV3, SourceAttributionV1, SourceFateV3, WorkFinalEventV1,
        };
        use misaka_palw_kernel::{beacon_source::BeaconSourceOutcomeV1, lifecycle::ClaimStateV1 as C};
        let target = self.rfc07_normal_source_target_checked_v4(context)?;
        let ledger = self.ledger_checked_v1()?;
        let exported = self.rfc07_normal_finals_owned_v1()?;
        let count = ledger.beacon_sources.range((*context, [0; 64])..=(*context, [255; 64])).count();
        let mut map_quote = TirNativeRequestsV1::new();
        map_quote.exact_v1::<BTreeNodeEnvelopeV1<Digest, WorkFinalEventV1>>(exported.len() as u64).map_err(E::Resource)?;
        let _map_owner = reserve_native_requests_v1(map_quote.bytes_v1()).map_err(E::Resource)?;
        let mut finals = BTreeMap::new();
        for f in exported.iter() {
            let Some(bytes) = &f.event else { continue };
            // Same canonical event decode/skip as the original algorithm.
            let Ok(work) = borsh::from_slice::<misaka_palw_challenge::AttributedWorkV1>(bytes) else { continue };
            debug_assert!(work.event.depends_on_profiles.is_empty(), "canonical exporter has empty profile dependencies");
            finals.insert(f.receipt.claim, work.event);
        }
        let output_bytes =
            (count as u64).checked_mul(size_of::<OptInSourceV4>() as u64).ok_or_else(|| cap("source population overflow"))?;
        let output_owner = reserve_native_requests_v1(output_bytes).map_err(E::Resource)?;
        let mut out = Vec::new();
        out.try_reserve_exact(count).map_err(cap)?;
        for ((id, _), source) in ledger.beacon_sources.range((*context, [0; 64])..=(*context, [255; 64])) {
            if *id != *context || source.terms != target.terms {
                return Err(bad("source reservation differs from its frozen target"));
            }
            let reveal = source.revealed.map(|(claim, revealed_daa)| {
                let fate = match (ledger.claims.get(&claim), finals.get(&claim)) {
                    (_, Some(event)) => SourceFateV3::Final(event.clone()),
                    (Some(c), _) if c.convicted || matches!(c.life.state, C::Unavailable { .. } | C::TimedOut { .. }) => {
                        SourceFateV3::Failed
                    }
                    (Some(_), _) => SourceFateV3::Live,
                    _ => SourceFateV3::Failed,
                };
                SealRevealV3 { reveal_position: revealed_daa, salt: ledger.claim_beacon_salts[&claim], fate }
            });
            out.push(OptInSourceV4 {
                context_id: *context,
                terms_id: source.terms.id(),
                opted_in_position: source.opted_in_daa,
                forfeited_position: match source.outcome {
                    Some(BeaconSourceOutcomeV1::Forfeited { daa }) => Some(daa),
                    _ => None,
                },
                source: SealedSourceV3 {
                    source_profile_id: source.profile,
                    attribution: SourceAttributionV1 { producer_id: source.producer, consumer_id: RootV1::Present(source.poster) },
                    seal: source.seal,
                    seal_position: source.sealed_daa,
                    reveal,
                },
            });
        }
        Ok(NativeRetainedV1::from_owner_reserved_v1(out, output_owner))
    }

    pub fn rfc07_normal_paid_material_owned_v5(&self, context: &Digest) -> Result<Option<Rfc07NormalPaidMaterialV5>, E<String>> {
        use misaka_palw_challenge::{OptInContractV4, PaidOptInContractV5, PaidSourcePromiseV5, SubjectKindV1};
        let expected = self.rfc07_normal_fee_target_checked_v5(context)?;
        if expected.target.context.subject_kind == SubjectKindV1::ClaimVerification {
            self.rfc07_normal_source_authority_checked_v3(context)?;
        }
        if expected.target.context.subject_kind == SubjectKindV1::PanelAssignment {
            // Existing Panel reader is semantic String, with legacy raw decode;
            // this separate boundary is not claimed as typed/owned completion.
            self.panel_source_fee_authority_v2(context).map_err(bad)?;
        }
        let ledger = self.ledger_checked_v1()?;
        let sources = self.rfc07_normal_opt_in_sources_owned_v4(context)?;
        let count = ledger.beacon_source_fee_promises.range((*context, [0; 64])..=(*context, [255; 64])).count();
        let amount = mul(count as u64, size_of::<PaidSourcePromiseV5>() as u64)?;
        let promise_owner = reserve_native_requests_v1(amount).map_err(E::Resource)?;
        let mut promises = Vec::new();
        promises.try_reserve_exact(count).map_err(cap)?;
        for ((ctx, producer), promise) in ledger.beacon_source_fee_promises.range((*context, [0; 64])..=(*context, [255; 64])) {
            promises.push(PaidSourcePromiseV5 {
                context_id: *ctx,
                producer: *producer,
                fee_terms_id: promise.terms_id,
                accepted_position: promise.accepted_daa,
                fee: promise.fee,
            });
        }
        let Some(funded) = ledger.beacon_source_fees.get(context) else {
            if !sources.is_empty() || !promises.is_empty() {
                return Err(bad("mandatory paid sources have no prepaid backing"));
            }
            return Ok(None);
        };
        if funded.target != expected.target
            || funded.terms.sponsor != expected.sponsor
            || funded.terms.fee_per_source < expected.minimum_fee
            || funded.terms.max_sources < expected.target.context.policy.work_count_k
            || funded.terms.max_sources > expected.max_sources
        {
            return Err(bad("prepaid funding does not meet the frozen monetary target"));
        }
        let contract = PaidOptInContractV5 {
            source: OptInContractV4 {
                context_id: *context,
                terms_id: funded.terms.source.id(),
                source_final_by: funded.terms.source.final_by,
            },
            fee_terms_id: funded.terms.id(),
            minimum_fee: expected.minimum_fee,
            fee_per_source: funded.terms.fee_per_source,
            max_sources: funded.terms.max_sources,
            funded_position: funded.funded_daa,
        };
        // Same V5 collector, same source inventory and same ledger position.
        // Its internal workset remains a separately audited ownership boundary.
        misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(
            &expected.target.context,
            &contract,
            &sources,
            &promises,
            ledger.daa,
        )
        .map_err(|e| bad(e.to_string()))?;
        let (sources, source_owner) = sources.into_parts_v1();
        Ok(Some(Rfc07NormalPaidMaterialV5 { value: (contract, sources, promises), _sources: source_owner, _promises: promise_owner }))
    }
}
