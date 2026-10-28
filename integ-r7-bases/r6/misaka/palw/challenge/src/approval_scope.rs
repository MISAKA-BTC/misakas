//! Authenticated finite review horizons. A submitted digest is never a review.
//! Consensus persists the counter and admitted event identities in its rooted,
//! journaled state. Offline grinding/fork choices remain explicit review bounds;
//! this local counter does not pretend to observe them.
use crate::hash::object_id;
use crate::{Digest, PostCommitChallengePolicyV1, ReviewedTupleV2};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LifetimeReviewScopeV3 {
    pub version: u16,
    pub chain: Digest,
    pub ruleset: Digest,
    pub start_daa: u64,
    pub end_daa: u64,
    /// Includes historical authorized suites: policy upgrades cannot reset Q/G.
    pub registry: Vec<ReviewedTupleV2>,
    pub maximum_admissions: u128,
    pub maximum_beacons: u128,
    pub maximum_offline_choices: u128,
    pub external_review_root: Digest,
    pub source_independence_root: Digest,
    pub economic_evidence_root: Digest,
}
impl LifetimeReviewScopeV3 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/review/lifetime-scope/v3", self)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 3
            || self.start_daa > self.end_daa
            || self.registry.is_empty()
            || self.maximum_admissions == 0
            || self.maximum_beacons == 0
            || self.maximum_offline_choices == 0
            || [self.external_review_root, self.source_independence_root, self.economic_evidence_root].contains(&[0; 64])
        {
            return Err("review scope is unknown, empty or unbounded".into());
        }
        for t in &self.registry {
            let covered_beacons = t
                .algebraic
                .adaptive_queries
                .checked_mul(u128::from(t.algebraic.beacons_per_attempt))
                .and_then(|v| v.checked_mul(u128::from(t.policy.retry_limit) + 1))
                .ok_or("review beacon horizon overflow")?;
            if t.algebraic.adaptive_queries < self.maximum_admissions
                || covered_beacons < self.maximum_beacons
                || t.algebraic.grinding_choices_per_beacon < self.maximum_offline_choices
            {
                return Err("review tuple does not cover the shared lifetime query/grinding scope".into());
            }
            crate::approved_v2(&self.registry, &t.algebraic.checker_suite_id, &t.policy).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LifetimeCounterV3 {
    pub admissions: u128,
    pub beacons: u128,
    /// The events must be persisted; retries/transcript rounds need different
    /// identities. Replaying an event never obtains a fresh budget.
    pub admitted_events: std::collections::BTreeSet<Digest>,
    pub authorized_pairs: std::collections::BTreeSet<(Digest, Digest)>,
}
impl LifetimeCounterV3 {
    /// Atomic on failure. The caller authenticates `scope.id()` against its trusted
    /// network release registry before this function; no self-signed approval.
    pub fn admit(
        &mut self,
        scope: &LifetimeReviewScopeV3,
        trusted_scopes: &std::collections::BTreeSet<Digest>,
        chain: Digest,
        ruleset: Digest,
        now: u64,
        suite: Digest,
        policy: &PostCommitChallengePolicyV1,
        event: Digest,
        beacons: u32,
    ) -> Result<(), String> {
        scope.validate()?;
        if !trusted_scopes.contains(&scope.id())
            || scope.chain != chain
            || scope.ruleset != ruleset
            || now < scope.start_daa
            || now > scope.end_daa
            || event == [0; 64]
            || self.admitted_events.contains(&event)
        {
            return Err("unauthenticated/out-of-horizon/replayed review admission".into());
        }
        for (suite, policy_id) in &self.authorized_pairs {
            if !scope.registry.iter().any(|t| t.algebraic.checker_suite_id == *suite && t.policy.id() == *policy_id) {
                return Err("an upgraded lifetime review omitted a historical authorized suite/policy".into());
            }
        }
        crate::approved_v2(&scope.registry, &suite, policy).map_err(|e| e.to_string())?;
        let q = self.admissions.checked_add(1).ok_or("lifetime admissions overflow")?;
        let b = self.beacons.checked_add(u128::from(beacons)).ok_or("lifetime beacons overflow")?;
        if q > scope.maximum_admissions || b > scope.maximum_beacons {
            return Err("review lifetime exhausted".into());
        }
        self.admissions = q;
        self.beacons = b;
        self.admitted_events.insert(event);
        self.authorized_pairs.insert((suite, policy.id()));
        Ok(())
    }
}

/// Actual native inflow/outflow evidence, distinct from illustrative cost bounds.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceEconomicObservationV3 {
    pub chain: Digest,
    pub source_claim: Digest,
    pub prepaid_fee_terms: Digest,
    pub source_operator: Digest,
    pub confirmed_income_transaction: Digest,
    pub income_sompi: u64,
    pub execution_sompi: u64,
    pub carrier_sompi: u64,
    pub material_sompi: u64,
    pub court_sompi: u64,
    pub capital_sompi: u64,
    pub measured_from_daa: u64,
    pub measured_until_daa: u64,
    pub measurement_root: Digest,
}
impl SourceEconomicObservationV3 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/source/economic-observation/v3", self)
    }
    pub fn net_receipt(&self) -> Result<u64, String> {
        if self.confirmed_income_transaction == [0; 64]
            || self.measurement_root == [0; 64]
            || self.measured_from_daa > self.measured_until_daa
            || self.income_sompi == 0
        {
            return Err("source economics needs confirmed native income and scoped measurements".into());
        }
        let cost = [self.execution_sompi, self.carrier_sompi, self.material_sompi, self.court_sompi, self.capital_sompi]
            .into_iter()
            .try_fold(0u64, |s, v| s.checked_add(v).ok_or("source cost overflow"))?;
        self.income_sompi.checked_sub(cost).ok_or_else(|| "source has negative realized net income".into())
    }
}

/// Independent operators and censorship resilience need an authenticated external
/// observation. Distinct keys alone never establish independent control.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourceIndependenceObservationV3 {
    pub chain: Digest,
    pub from_daa: u64,
    pub until_daa: u64,
    pub operator_control_roots: std::collections::BTreeMap<Digest, Digest>,
    pub admitted_source_claims: std::collections::BTreeSet<Digest>,
    pub withheld_source_claims: std::collections::BTreeSet<Digest>,
    pub censored_attempt_roots: std::collections::BTreeSet<Digest>,
    pub external_measurement_root: Digest,
}
impl SourceIndependenceObservationV3 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/source/independence-observation/v3", self)
    }
    pub fn validate(&self) -> Result<(), String> {
        let controls: std::collections::BTreeSet<_> = self.operator_control_roots.values().copied().collect();
        if self.from_daa > self.until_daa
            || self.external_measurement_root == [0; 64]
            || self.operator_control_roots.len() < 2
            || controls.len() != self.operator_control_roots.len()
            || controls.contains(&[0; 64])
            || self.admitted_source_claims.is_empty()
            || !self.admitted_source_claims.is_disjoint(&self.withheld_source_claims)
        {
            return Err("independence observation is missing, aliasing or inconsistent".into());
        }
        Ok(())
    }
}
