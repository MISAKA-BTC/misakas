//! Dedicated fence for the source reservation foundation (inner kernel tag 40; codex used 21, renumbered at the R6781 merge).
//! None on all presets. The onboarding v4 adapter is implemented; funded source fees and security approval remain incomplete.
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::PalwModeV2Error;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::beacon_source::{BeaconSourceTargetV1, BeaconSourceTermsV1};

/// Unapproved source economics/timing limits. The fence is never armable in this binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBeaconSourcePolicyV1 {
    pub deposit_sompi: u64,
    pub max_per_producer: u32,
    pub max_total: u32,
    pub source_final_slack_daa: u64,
}

impl PalwBeaconSourcePolicyV1 {
    pub fn id(&self) -> misaka_palw_kernel::hash::Digest {
        misaka_palw_kernel::hash::object_id(b"misaka-palw/native/beacon-source-policy/v1", self)
    }

    pub fn target(
        &self,
        context: misaka_palw_challenge::BeaconContextV1,
        ledger_policy: &misaka_palw_kernel::ledger::LedgerPolicyV1,
    ) -> Result<BeaconSourceTargetV1, String> {
        use crate::palw_conformance_evidence_v1::{
            PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1, PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1,
        };
        use crate::palw_onboarding_v1::PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1;
        if !context.policy.is_opt_in_source() {
            return Err("native source target needs opt-in v4".into());
        }
        // This horizon belongs to the authenticated onboarding attempt. A
        // different consumer must derive and enforce its own complete liability
        // bound before admitting any source or charging a sponsor.
        if context.subject_kind != misaka_palw_challenge::SubjectKindV1::ModelConformance
            || context.candidate_profile_id == misaka_palw_challenge::RootV1::Absent
        {
            return Err("onboarding source timing cannot authorize a Panel, claim, slice or prosecution target".into());
        }
        let source_floor = ledger_policy
            .challenge_window_daa
            .checked_add(ledger_policy.court_deadline_daa)
            .and_then(|x| x.checked_add(ledger_policy.proof_grace_daa))
            .ok_or("source Final bound overflow")?;
        if self.source_final_slack_daa < source_floor {
            return Err("source Final slack is below the claim/court bound".into());
        }
        let start = context.commitment_position.checked_add(context.policy.anchor_delay_slots).ok_or("source start overflow")?;
        let seal_end = start.checked_add(context.policy.beacon_window_slots).ok_or("source seal end overflow")?;
        let reveal_end = seal_end.checked_add(context.policy.beacon_window_slots).ok_or("source reveal end overflow")?;
        let final_by = reveal_end.checked_add(self.source_final_slack_daa).ok_or("source Final end overflow")?;
        // Last source Final -> lock -> latest posted evidence -> unrefuted decision -> binding liability/court horizon.
        let hold_until = final_by
            .checked_add(context.policy.settlement_depth_d)
            .and_then(|x| x.checked_add(PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1))
            .and_then(|x| x.checked_add(PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1))
            .and_then(|x| x.checked_add(PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1))
            .ok_or("source target liability overflow")?;
        let terms = BeaconSourceTermsV1 {
            context: context.id(),
            seal_start: start,
            seal_end,
            reveal_end,
            final_by,
            hold_until,
            deposit: self.deposit_sompi,
            max_per_producer: self.max_per_producer,
            max_total: self.max_total,
        };
        let target = BeaconSourceTargetV1 { context, terms };
        target.validate()?;
        Ok(target)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBeaconSourceFenceV1 {
    pub activation: ForkActivation,
    pub policy: PalwBeaconSourcePolicyV1,
}

impl PalwBeaconSourceFenceV1 {
    /// Private harness probe; not a price derivation, approved tuple or production default.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            policy: PalwBeaconSourcePolicyV1 { deposit_sompi: 100, max_per_producer: 8, max_total: 64, source_final_slack_daa: 120 },
        }
    }
}

impl Params {
    pub fn palw_beacon_source_opt_in_activation(&self) -> Option<u64> {
        self.palw_beacon_source_opt_in_v1.filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score())
    }

    pub fn validate_palw_beacon_source_opt_in_v1(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_beacon_source_opt_in_activation() {
            Some(_) => Err(PalwModeV2Error::Invalid(
                "palw_beacon_source_opt_in_v1 cannot be armed: funded fees, public timing/resource bounds and security approval are incomplete",
            )),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};

    #[test]
    fn source_opt_in_onboarding_horizon_refuses_other_consumer_subjects_before_monetary_admission() {
        use misaka_palw_challenge::{BeaconContextV1, RootV1, SubjectKindV1};
        let policy = crate::palw_conformance_evidence_v1::palw_onboarding_paid_opt_in_policy_v5();
        let ledger = crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(
            kaspa_hashes::Hash64::from_u64_word(1),
            kaspa_hashes::Hash64::from_u64_word(2),
        );
        let source = PalwBeaconSourceFenceV1::unapproved_probe_at(ForkActivation::new(1));
        let context = BeaconContextV1 {
            chain_genesis: [1; 64],
            ruleset_id: [2; 64],
            policy,
            subject_kind: SubjectKindV1::ModelConformance,
            commitment_root: [3; 64],
            commitment_position: 100,
            challenge_epoch: 0,
            eligible_profiles: std::collections::BTreeSet::from([[4; 64]]),
            excluded_profiles: std::collections::BTreeSet::from([[5; 64]]),
            candidate_profile_id: RootV1::Present([5; 64]),
        };
        let original = source.policy.target(context.clone(), &ledger).unwrap();
        original.validate().unwrap();
        for subject in SubjectKindV1::ALL {
            if subject == SubjectKindV1::ModelConformance {
                continue;
            }
            let mut changed = context.clone();
            changed.subject_kind = subject;
            assert!(source.policy.target(changed.clone(), &ledger).is_err(), "{subject:?}");
            changed.candidate_profile_id = RootV1::Absent;
            assert!(source.policy.target(changed, &ledger).is_err(), "typed absence does not authorize {subject:?}");
        }
        let mut missing = context;
        missing.candidate_profile_id = RootV1::Absent;
        assert!(source.policy.target(missing, &ledger).is_err());
    }

    #[test]
    fn source_opt_in_has_its_own_dormant_fence_and_none_is_fingerprint_neutral() {
        let t12 = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
        for original in [MAINNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS, SIMNET_PARAMS, t12] {
            assert!(original.palw_beacon_source_opt_in_v1.is_none());
            original.validate_palw_beacon_source_opt_in_v1().unwrap();
            let mut p = original.clone();
            let before = (p.consensus_params_id(), p.consensus_schedule_id());
            p.palw_beacon_source_opt_in_v1 = Some(PalwBeaconSourceFenceV1::unapproved_probe_at(ForkActivation::new(1)));
            assert!(p.validate_palw_beacon_source_opt_in_v1().is_err());
            assert_ne!((p.consensus_params_id(), p.consensus_schedule_id()), before);
            let armed = (p.consensus_params_id(), p.consensus_schedule_id());
            p.palw_beacon_source_opt_in_v1.as_mut().unwrap().policy.deposit_sompi += 1;
            assert_ne!(p.consensus_params_id(), armed.0, "source contract changes consensus identity");
            assert_ne!(p.consensus_schedule_id(), armed.1, "source contract changes schedule identity");
            p.palw_beacon_source_opt_in_v1 = Some(PalwBeaconSourceFenceV1::unapproved_probe_at(ForkActivation::never()));
            p.validate_palw_beacon_source_opt_in_v1().unwrap();
            assert!(p.palw_beacon_source_opt_in_activation().is_none());
            assert_eq!(p.consensus_identity_id(), original.consensus_identity_id());
        }
    }
}

/// Frozen per-attempt source contract; kernel route aux table 108 (codex used 46 — RFC-02's model artifact v2 bindings on the
/// integration line, where aux 32–107 are all used; renumbered at the R6781-F merge), independent of kernel reservation table 30.
pub const PALW_BEACON_SOURCE_TABLE_TARGETS_V4: u8 = 108;

impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn beacon_source_target_v4(&self, context: &misaka_palw_kernel::hash::Digest) -> Result<BeaconSourceTargetV1, String> {
        let target: BeaconSourceTargetV1 = self
            .aux_row(PALW_BEACON_SOURCE_TABLE_TARGETS_V4, &borsh::to_vec(context).expect("context serializes"))
            .ok_or("the frozen source target is missing")?;
        target.validate()?;
        if target.terms.context != *context || !target.context.policy.is_opt_in_source() {
            return Err("stored source target context mismatch".into());
        }
        Ok(target)
    }

    pub fn beacon_source_inputs_v4(
        &self,
        fence: PalwBeaconSourceFenceV1,
        daa: u64,
    ) -> Result<misaka_palw_kernel::beacon_source::BeaconSourceInputsV1, String> {
        use misaka_palw_kernel::beacon_source::BeaconSourceInputsV1;
        let mut inputs = BeaconSourceInputsV1 { activation_daa: Some(fence.activation.daa_score()), targets: Default::default() };
        if !fence.activation.is_active(daa) || fence.activation == ForkActivation::never() {
            return Ok(Default::default());
        }
        for ((table, key), bytes) in &self.aux {
            if *table != PALW_BEACON_SOURCE_TABLE_TARGETS_V4 {
                continue;
            }
            let id = borsh::from_slice::<misaka_palw_kernel::hash::Digest>(key).map_err(|e| e.to_string())?;
            let target = borsh::from_slice::<BeaconSourceTargetV1>(bytes).map_err(|e| e.to_string())?;
            target.validate()?;
            if target.terms.context != id {
                return Err("source target key mismatch".into());
            }
            if daa < target.terms.seal_start || daa >= target.terms.seal_end {
                continue;
            }
            let misaka_palw_challenge::RootV1::Present(class) = target.context.candidate_profile_id else {
                return Err("source target candidate missing".into());
            };
            let attempt = self
                .conformance_attempt_v1(&kaspa_hashes::Hash64::from_bytes(class))
                .ok_or("source target has no authenticated attempt")?;
            if !attempt.open() || !attempt.is_opt_in_source() {
                continue;
            }
            if attempt.committed_daa < fence.activation.daa_score() || attempt.beacon_context(&attempt.policy()) != target.context {
                return Err("source target does not bind its authenticated attempt".into());
            }
            // Current policy cannot rewrite an accepted contract; the network identity changes with the tuple.
            if fence.policy.target(target.context.clone(), &self.header.policy)? != target {
                return Err("frozen source terms differ from the network contract".into());
            }
            inputs.targets.insert(id, target);
        }
        Ok(inputs)
    }

    /// Only table-28 opt-ins are converted; ordinary live/revealed/forfeited race seals have no v4 contribution.
    pub fn beacon_opt_in_sources_v4(
        &self,
        context: &misaka_palw_kernel::hash::Digest,
    ) -> Result<Vec<misaka_palw_challenge::OptInSourceV4>, String> {
        use misaka_palw_challenge::{OptInSourceV4, RootV1, SealRevealV3, SealedSourceV3, SourceAttributionV1, SourceFateV3};
        use misaka_palw_kernel::beacon_source::BeaconSourceOutcomeV1;
        use misaka_palw_kernel::lifecycle::ClaimStateV1 as C;
        let target = self.beacon_source_target_v4(context)?;
        let ledger = self.ledger()?;
        let finals: std::collections::BTreeMap<_, _> = self
            .finals_read_v1()?
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
                return Err("source reservation differs from its frozen target".into());
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
