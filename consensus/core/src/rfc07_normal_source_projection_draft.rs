//! UNREFERENCED DRAFT: do not register during the coordinated Rust freeze.
//! Exact existing normal-source iteration and selected-approval decision.
//! No V6 source substitution and no new policy, approval or activation.
//! This preserves typed lower ledger errors, not full output ownership:
//! Vec / BTreeMap / event buffers still require a separately measured owner.
//! Source target still uses its legacy raw decoder; see the evidence JSON.
//! The old String methods should become display wrappers only after tests.
use crate::palw_kernel_route_v1::{PalwKernelRouteStateV1, KernelFinalReadV1, panel_record_v1};
use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;

impl PalwKernelRouteStateV1 {
    pub fn finals_read_checked_v1(&self) -> Result<Vec<KernelFinalReadV1>, E<String>> {
        let ledger = self.ledger_native_projection_checked_v9()?;
        let mut out = Vec::new();
        for receipt in ledger.final_receipts() {
            let settlement = crate::palw_kernel_settlement_v1::palw_kernel_row_settlement_v1(&ledger, &receipt.claim, ledger.daa)
                .unwrap_or(crate::palw_kernel_settlement_v1::PalwKernelSettlementV1::Pending);
            let (final_path, event) = self.final_export_v1(&ledger, &receipt, settlement).map_err(E::Decode)?;
            let statement = receipt.assurance.statement();
            let panel_record = panel_record_v1(&ledger, &receipt, final_path, settlement);
            out.push(KernelFinalReadV1 { receipt, final_path, event, statement, settlement: settlement.label(), panel_record });
        }
        Ok(out)
    }

    pub fn beacon_opt_in_sources_checked_v4(
        &self,
        context: &misaka_palw_kernel::hash::Digest,
    ) -> Result<Vec<misaka_palw_challenge::OptInSourceV4>, E<String>> {
        use misaka_palw_challenge::{OptInSourceV4, RootV1, SealRevealV3, SealedSourceV3, SourceAttributionV1, SourceFateV3};
        use misaka_palw_kernel::beacon_source::BeaconSourceOutcomeV1;
        use misaka_palw_kernel::lifecycle::ClaimStateV1 as C;
        let target = self.beacon_source_target_v4(context).map_err(E::Decode)?;
        let ledger = self.ledger_checked_v1()?;
        // The one source of a beacon event is the RFC07 adapter: `finals_read_v1` exports an event only for a paid claim whose
        // canonical paid Final the ASSIGNED quorum approved (selected checkers, validity independent of any Panel); every other
        // Final — an optimistic timeout, a Panel-licensed Final, an unapproved paid claim — exports none and so cannot be a source.
        // User decision 2026-10-17: approved paid claims ARE sources (the earlier exclusion of every paid claim left the pool with
        // no possible member once the adapter landed).
        let finals: std::collections::BTreeMap<_, _> = self
            .finals_read_checked_v1()?
            .into_iter()
            .filter_map(|f| {
                let bytes = f.event?;
                let w: misaka_palw_challenge::AttributedWorkV1 = borsh::from_slice(&bytes).ok()?;
                Some((f.receipt.claim, w.event))
            })
            .collect();
        let mut out = Vec::new();
        for ((id, _), row) in ledger.beacon_sources.range((*context, [0; 64])..=(*context, [255; 64])) {
            if *id != *context || row.terms != target.terms {
                return Err(E::Decode("source reservation differs from its frozen target".into()));
            }
            let reveal = row.revealed.map(|(claim, revealed_daa)| {
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
                terms_id: row.terms.id(),
                opted_in_position: row.opted_in_daa,
                forfeited_position: match row.outcome {
                    Some(BeaconSourceOutcomeV1::Forfeited { daa }) => Some(daa),
                    _ => None,
                },
                source: SealedSourceV3 {
                    source_profile_id: row.profile,
                    attribution: SourceAttributionV1 { producer_id: row.producer, consumer_id: RootV1::Present(row.poster) },
                    seal: row.seal,
                    seal_position: row.sealed_daa,
                    reveal,
                },
            });
        }
        Ok(out)
    }
}
