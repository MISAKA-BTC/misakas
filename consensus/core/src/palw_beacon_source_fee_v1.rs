//! Independent dormant monetary fence for prepaid beacon-source contracts (inner 61/62; codex used 22/23, renumbered at the R6781-F merge).
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use borsh::{BorshDeserialize, BorshSerialize};

/// Proposed upper bounds for an entire source contract, including a public
/// failure/court path. A report identifier records provenance; it does not prove
/// that measurements or an economic policy have been approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBeaconSourceCostBoundsV1 {
    pub execution_sompi: u64,
    pub carrier_sompi: u64,
    pub material_sompi: u64,
    pub public_court_sompi: u64,
    pub locked_capital_sompi: u64,
    pub minimum_net_receipt_sompi: u64,
    pub maximum_source_deposit_sompi: u64,
    pub maximum_liability_daa: u64,
    pub report_root: [u8; 64],
}

impl PalwBeaconSourceCostBoundsV1 {
    pub fn minimum_gross_fee(&self, terms: &misaka_palw_kernel::beacon_source::BeaconSourceTermsV1) -> Result<u64, String> {
        terms.validate()?;
        let span =
            terms.hold_until.checked_sub(terms.seal_start).and_then(|d| d.checked_add(1)).ok_or("source cost horizon overflow")?;
        // Unknown costs are represented by no bounds, rather than by zero. This
        // tuple is only usable inside its measured/proposed scope.
        if [
            self.execution_sompi,
            self.carrier_sompi,
            self.material_sompi,
            self.public_court_sompi,
            self.locked_capital_sompi,
            self.minimum_net_receipt_sompi,
        ]
        .contains(&0)
            || self.report_root == [0; 64]
            || terms.deposit > self.maximum_source_deposit_sompi
            || span > self.maximum_liability_daa
        {
            return Err("source cost bounds are missing, zero, or outside the collateral/liability scope".into());
        }
        [
            self.execution_sompi,
            self.carrier_sompi,
            self.material_sompi,
            self.public_court_sompi,
            self.locked_capital_sompi,
            self.minimum_net_receipt_sompi,
        ]
        .into_iter()
        .try_fold(0u64, |sum, value| sum.checked_add(value).ok_or_else(|| "source gross cost floor overflow".into()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBeaconSourceFeePolicyV1 {
    pub minimum_fee_sompi: u64,
    pub max_funded_sources: u32,
    pub cost_bounds: Option<PalwBeaconSourceCostBoundsV1>,
}
impl PalwBeaconSourceFeePolicyV1 {
    pub fn id(&self) -> misaka_palw_kernel::hash::Digest {
        misaka_palw_kernel::hash::object_id(b"misaka-palw/native/source-fee-policy/v1", self)
    }

    pub fn minimum_fee_for(&self, target: &misaka_palw_kernel::beacon_source::BeaconSourceTargetV1) -> Result<u64, String> {
        target.validate()?;
        if !target.context.policy.is_paid_opt_in_source() {
            return Ok(self.minimum_fee_sompi);
        }
        let bounds = self.cost_bounds.as_ref().ok_or("mandatory paid source policy has no net cost bounds")?;
        Ok(self.minimum_fee_sompi.max(bounds.minimum_gross_fee(&target.terms)?))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBeaconSourceFeeFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwBeaconSourceFeePolicyV1,
}
impl PalwBeaconSourceFeeFenceV1 {
    /// Mechanics fixture only: not a measured cost floor, economic approval or live network default.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            policy: PalwBeaconSourceFeePolicyV1 {
                minimum_fee_sompi: 50,
                max_funded_sources: 64,
                // Illustrative arithmetic only. This is not a measured source price,
                // a zero-volume retainer, or an approved capital/latency bound.
                cost_bounds: Some(PalwBeaconSourceCostBoundsV1 {
                    execution_sompi: 1,
                    carrier_sompi: 1,
                    material_sompi: 1,
                    public_court_sompi: 1,
                    locked_capital_sompi: 1,
                    minimum_net_receipt_sompi: 1,
                    maximum_source_deposit_sompi: 100,
                    maximum_liability_daa: 10_000,
                    report_root: misaka_palw_kernel::hash::object_id(b"misaka-palw/source-cost/unapproved-probe/v1", &()),
                }),
            },
        }
    }
}
impl Params {
    pub fn validate_palw_beacon_source_fee_v1(&self) -> Result<(), PalwModeV2Error> {
        if self.palw_beacon_source_fee_v1.is_some_and(|f| f.activation != ForkActivation::never()) {
            return Err(PalwModeV2Error::Invalid(
                "palw_beacon_source_fee_v1 cannot be armed: mandatory funded source policy, measured economic floors and public security/resource approval are incomplete",
            ));
        }
        Ok(())
    }
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn beacon_source_fee_inputs_v1(
        &self,
        fence: PalwBeaconSourceFeeFenceV1,
        sources: &misaka_palw_kernel::beacon_source::BeaconSourceInputsV1,
        daa: u64,
    ) -> Result<misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeInputsV1, String> {
        use misaka_palw_kernel::beacon_source_fee::{BeaconSourceFeeInputsV1, BeaconSourceFeeTargetV1};
        if fence.activation == ForkActivation::never() || !fence.activation.is_active(daa) {
            return Ok(Default::default());
        }
        if fence.policy.minimum_fee_sompi == 0 || fence.policy.max_funded_sources == 0 {
            return Err("invalid source fee policy".into());
        }
        let mut inputs = BeaconSourceFeeInputsV1 { activation_daa: Some(fence.activation.daa_score()), targets: Default::default() };
        for (context, target) in &sources.targets {
            if target.context.commitment_position < fence.activation.daa_score() {
                continue;
            }
            let misaka_palw_challenge::RootV1::Present(class) = target.context.candidate_profile_id else {
                return Err("fee candidate missing".into());
            };
            let class = kaspa_hashes::Hash64::from_bytes(class);
            let attempt = self.conformance_attempt_v1(&class).ok_or("fee attempt missing")?;
            if !attempt.open() || attempt.beacon_context(&attempt.policy()) != target.context {
                return Err("fee attempt substituted".into());
            }
            let commitment = self
                .conformance_v1(&class, &kaspa_hashes::Hash64::from_bytes(attempt.commitment.artifact_root))
                .ok_or("fee sponsor commitment missing")?;
            if commitment.committed_daa != attempt.committed_daa
                || commitment.statement_root.as_bytes() != attempt.commitment.statement_root()
            {
                return Err("fee sponsor is not the authenticated attempt signer".into());
            }
            let derived = BeaconSourceFeeTargetV1 {
                target: target.clone(),
                sponsor: crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&commitment.signer),
                minimum_fee: fence.policy.minimum_fee_for(target)?,
                max_sources: fence.policy.max_funded_sources.min(target.terms.max_total),
            };
            if target.context.policy.is_paid_opt_in_source()
                && (self.beacon_source_fee_target_v5(context)? != derived
                    || self.beacon_source_cost_bounds_v5(context)?
                        != fence.policy.cost_bounds.ok_or("paid source cost bounds missing")?)
            {
                return Err("fee floor/sponsor/cap/cost bounds differ from the frozen paid target".into());
            }
            inputs.targets.insert(*context, derived);
        }
        Ok(inputs)
    }
}

/// Separate immutable monetary terms for v5; v4 historical targets keep their schema. Kernel route aux tables 109 / 110 (codex used
/// 47 / 48 — RFC-02's model artifact v2 tables on the integration line; renumbered at the R6781-F merge).
pub const PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5: u8 = 109;
pub const PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5: u8 = 110;
pub type PaidBeaconMaterialV5 = (
    misaka_palw_challenge::PaidOptInContractV5,
    Vec<misaka_palw_challenge::OptInSourceV4>,
    Vec<misaka_palw_challenge::PaidSourcePromiseV5>,
);
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn beacon_source_cost_bounds_v5(
        &self,
        context: &misaka_palw_kernel::hash::Digest,
    ) -> Result<PalwBeaconSourceCostBoundsV1, String> {
        self.aux_row(PALW_BEACON_SOURCE_TABLE_COST_BOUNDS_V5, &borsh::to_vec(context).expect("context serializes"))
            .ok_or_else(|| "frozen paid source cost bounds missing".into())
    }

    pub fn beacon_source_fee_target_v5(
        &self,
        context: &misaka_palw_kernel::hash::Digest,
    ) -> Result<misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1, String> {
        let target: misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTargetV1 = self
            .aux_row(PALW_BEACON_SOURCE_TABLE_FEE_TARGETS_V5, &borsh::to_vec(context).expect("context serializes"))
            .ok_or("frozen paid source target missing")?;
        target.target.validate()?;
        if target.target != self.beacon_source_target_v4(context)?
            || !target.target.context.policy.is_paid_opt_in_source()
            || target.minimum_fee == 0
            || target.max_sources < target.target.context.policy.work_count_k
            || target.max_sources > target.target.terms.max_total
        {
            return Err("frozen mandatory fee target mismatch/floor/cap".into());
        }
        if target.minimum_fee < self.beacon_source_cost_bounds_v5(context)?.minimum_gross_fee(&target.target.terms)? {
            return Err("frozen source fee does not cover the cost bounds and minimum net receipt".into());
        }
        Ok(target)
    }

    /// Derive the whole source/fee inventory from the same rooted route, including
    /// failed promises. No filtering of unpaid or forfeited mixed sources is allowed.
    pub fn paid_beacon_material_v5(&self, context: &misaka_palw_kernel::hash::Digest) -> Result<Option<PaidBeaconMaterialV5>, String> {
        use misaka_palw_challenge::{OptInContractV4, PaidOptInContractV5, PaidSourcePromiseV5};
        let expected = self.beacon_source_fee_target_v5(context)?;
        let ledger = self.ledger()?;
        let sources = self.beacon_opt_in_sources_v4(context)?;
        let promises: Vec<_> = ledger
            .beacon_source_fee_promises
            .range((*context, [0; 64])..=(*context, [255; 64]))
            .map(|((ctx, producer), p)| PaidSourcePromiseV5 {
                context_id: *ctx,
                producer: *producer,
                fee_terms_id: p.terms_id,
                accepted_position: p.accepted_daa,
                fee: p.fee,
            })
            .collect();
        let Some(funded) = ledger.beacon_source_fees.get(context) else {
            if !sources.is_empty() || !promises.is_empty() {
                return Err("mandatory paid sources have no prepaid backing".into());
            }
            return Ok(None);
        };
        if funded.target != expected.target
            || funded.terms.sponsor != expected.sponsor
            || funded.terms.fee_per_source < expected.minimum_fee
            || funded.terms.max_sources < expected.target.context.policy.work_count_k
            || funded.terms.max_sources > expected.max_sources
        {
            return Err("prepaid funding does not meet the frozen monetary target".into());
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
        // Reject incomplete money bindings even if a caller only wants public facts.
        misaka_palw_challenge::collect_paid_opt_in_work_beacon_v5(
            &expected.target.context,
            &contract,
            &sources,
            &promises,
            ledger.daa,
        )
        .map_err(|e| e.to_string())?;
        Ok(Some((contract, sources, promises)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};
    fn cost_terms() -> misaka_palw_kernel::beacon_source::BeaconSourceTermsV1 {
        misaka_palw_kernel::beacon_source::BeaconSourceTermsV1 {
            context: [1; 64],
            seal_start: 10,
            seal_end: 20,
            reveal_end: 30,
            final_by: 40,
            hold_until: 100,
            deposit: 100,
            max_per_producer: 1,
            max_total: 2,
        }
    }
    #[test]
    fn paid_source_cost_floor_covers_each_leg_and_a_positive_net_receipt() {
        let mut bounds = PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::new(1)).policy.cost_bounds.unwrap();
        bounds.execution_sompi = 200;
        bounds.carrier_sompi = 20;
        bounds.material_sompi = 30;
        bounds.public_court_sompi = 40;
        bounds.locked_capital_sompi = 50;
        bounds.minimum_net_receipt_sompi = 60;
        let gross = bounds.minimum_gross_fee(&cost_terms()).unwrap();
        assert_eq!(gross, 400);
        assert_eq!(gross - (200 + 20 + 30 + 40 + 50), 60);
        assert!(gross - 1 < 200 + 20 + 30 + 40 + 50 + bounds.minimum_net_receipt_sompi);
        for zero in 0..7 {
            let mut bad = bounds;
            match zero {
                0 => bad.execution_sompi = 0,
                1 => bad.carrier_sompi = 0,
                2 => bad.material_sompi = 0,
                3 => bad.public_court_sompi = 0,
                4 => bad.locked_capital_sompi = 0,
                5 => bad.minimum_net_receipt_sompi = 0,
                _ => bad.report_root = [0; 64],
            }
            assert!(bad.minimum_gross_fee(&cost_terms()).is_err());
        }
    }
    #[test]
    fn paid_source_cost_floor_refuses_uncovered_horizons_and_overflow() {
        let bounds = PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::new(1)).policy.cost_bounds.unwrap();
        let mut terms = cost_terms();
        terms.deposit = 101;
        assert!(bounds.minimum_gross_fee(&terms).is_err());
        terms = cost_terms();
        let too_short = PalwBeaconSourceCostBoundsV1 { maximum_liability_daa: 90, ..bounds };
        assert!(too_short.minimum_gross_fee(&terms).is_err());
        assert!(PalwBeaconSourceCostBoundsV1 { maximum_liability_daa: 91, ..bounds }.minimum_gross_fee(&terms).is_ok());
        let overflow = PalwBeaconSourceCostBoundsV1 { execution_sompi: u64::MAX, ..bounds };
        assert!(overflow.minimum_gross_fee(&terms).is_err());
    }
    #[test]
    fn source_fee_has_its_own_dormant_fence_and_tuple_fingerprints() {
        let t12 = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
        for original in [MAINNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, SIMNET_PARAMS, t12] {
            assert!(original.palw_beacon_source_fee_v1.is_none());
            original.validate_palw_beacon_source_fee_v1().unwrap();
            let mut p = original.clone();
            let before = (p.consensus_params_id(), p.consensus_schedule_id());
            p.palw_beacon_source_fee_v1 = Some(PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::new(1)));
            assert!(p.validate_palw_beacon_source_fee_v1().is_err());
            let armed = (p.consensus_params_id(), p.consensus_schedule_id());
            assert_ne!(armed, before);
            let mut cost_changed = p.clone();
            cost_changed.palw_beacon_source_fee_v1.as_mut().unwrap().policy.cost_bounds.as_mut().unwrap().locked_capital_sompi += 1;
            assert_ne!(cost_changed.consensus_params_id(), armed.0);
            assert_ne!(cost_changed.consensus_schedule_id(), armed.1);
            for price in [true, false] {
                if price {
                    p.palw_beacon_source_fee_v1.as_mut().unwrap().policy.minimum_fee_sompi += 1;
                } else {
                    p.palw_beacon_source_fee_v1.as_mut().unwrap().policy.max_funded_sources -= 1;
                }
                assert_ne!(p.consensus_params_id(), armed.0);
                assert_ne!(p.consensus_schedule_id(), armed.1);
            }
            p.palw_beacon_source_fee_v1 = Some(PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::never()));
            p.validate_palw_beacon_source_fee_v1().unwrap();
            assert_eq!(p.consensus_identity_id(), original.consensus_identity_id());
        }
    }
}
