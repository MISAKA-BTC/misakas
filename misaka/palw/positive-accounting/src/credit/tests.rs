use super::*;

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn id(n: u8) -> ClaimId {
    ClaimId(d(n))
}
fn scope() -> ChainScope {
    ChainScope { network: d(1), ruleset: d(2), branch: d(3) }
}
fn binding(n: u8) -> CreditBinding {
    CreditBinding { scope: scope(), claim: id(n), case: CaseId(d(n)), result: ResultId(d(n)), output: d(100), dependencies: d(n) }
}
struct Row {
    facts: RootedCreditFacts,
    coverage: Option<CheckedCoverage>,
    dependencies: Option<Vec<CreditBinding>>,
    entitlements: FixtureEntitlements,
}
#[derive(Clone, Copy)]
enum FixtureEntitlements {
    All,
    Only(&'static [CreditUse]),
}
struct View(BTreeMap<ClaimId, Row>);
impl RootedCreditView for View {
    fn scope(&self) -> ChainScope {
        scope()
    }
    fn claim(&self, claim: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        self.0.get(&claim).map(|r| r.facts).ok_or(CreditDenial::MissingClaim(claim))
    }
    fn checked_coverage(&self, claim: ClaimId) -> Result<CheckedCoverage, CreditDenial> {
        self.0.get(&claim).and_then(|r| r.coverage).ok_or(CreditDenial::MissingLocalVerification(claim))
    }
    fn checked_dependencies(&self, claim: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        self.0.get(&claim).and_then(|r| r.dependencies.as_deref()).ok_or(CreditDenial::MissingDependencyBinding(claim))
    }
    fn authorize_entitlement(&self, binding: &CreditBinding, usage: CreditUse) -> Result<(), CreditDenial> {
        let row = self.0.get(&binding.claim).ok_or(CreditDenial::MissingClaim(binding.claim))?;
        if row.facts.binding != *binding {
            return Err(CreditDenial::Binding(binding.claim));
        }
        match row.entitlements {
            FixtureEntitlements::All => Ok(()),
            FixtureEntitlements::Only(uses) if uses.contains(&usage) => Ok(()),
            FixtureEntitlements::Only(_) => Err(CreditDenial::Entitlement { claim: binding.claim, usage }),
        }
    }
}
fn ready(n: u8, parents: &[u8]) -> Row {
    let binding = binding(n);
    let plan = PlanId(d(99));
    Row {
        facts: RootedCreditFacts {
            binding,
            plan,
            mandatory_relations: 10,
            status: ClaimStatus::PositivePassed,
            gate_c: GateCApproval::Approved(binding),
            fraud: FraudState::Clear,
            maturity_and_liability: EconomicReadiness::Ready,
            carrier_and_funding: EconomicReadiness::Ready,
        },
        coverage: Some(CheckedCoverage {
            binding,
            plan,
            mandatory_relations: 10,
            checked_relations: 10,
            status: ClaimStatus::PositivePassed,
        }),
        dependencies: Some(parents.iter().copied().map(super::tests::binding).collect()),
        entitlements: FixtureEntitlements::All,
    }
}
fn view() -> View {
    View(BTreeMap::from([(id(10), ready(10, &[]))]))
}
fn gate(v: &View) -> PositiveCreditGate<'_, View> {
    PositiveCreditGate::new(v, CreditLimits { max_claims: 100, max_edges: 100 }).unwrap()
}
fn denied(v: &View, claim: ClaimId, expected: CreditDenial) {
    for usage in ALL_CREDIT_USES {
        assert_eq!(gate(v).authorize(&claim, usage).err(), Some(expected), "{usage:?}");
    }
    assert_eq!(gate(v).credited_weight(claim, u128::MAX), 0);
}

#[test]
fn all_consumers_share_the_current_predicate_and_preserve_the_exact_weight() {
    let v = view();
    for usage in ALL_CREDIT_USES {
        let permit = gate(&v).authorize(&id(10), usage).unwrap();
        assert_eq!(permit.binding(), binding(10));
        assert_eq!(permit.usage(), usage);
    }
    assert_eq!(gate(&v).credited_weight(id(10), u128::MAX), u128::MAX);
    denied(&v, id(11), CreditDenial::MissingClaim(id(11)));
}

#[test]
fn ready_paid_settlement_entitlement_does_not_grant_weight_receipts_or_rule_e() {
    let mut v = view();
    // Private fixture for an existing settlement-only entitlement. The full
    // coverage, drawn approval and economic readiness supplied by ready() stay
    // intact; none can manufacture a different existing right.
    v.0.get_mut(&id(10)).unwrap().entitlements = FixtureEntitlements::Only(&[CreditUse::Settlement]);
    for usage in ALL_CREDIT_USES {
        if usage == CreditUse::Settlement {
            assert!(gate(&v).authorize(&id(10), usage).is_ok());
        } else {
            assert_eq!(gate(&v).authorize(&id(10), usage).err(), Some(CreditDenial::Entitlement { claim: id(10), usage }));
        }
    }
    let gate = gate(&v);
    let consumers = consumers::CreditConsumers::new(&gate);
    assert_eq!(consumers.dag_weight(&id(10), u128::MAX), 0);
    assert_eq!(consumers.rule_e_capital(&id(10), u128::MAX), 0);
    assert!(consumers.work_receipt(&id(10)).is_err());
    assert!(consumers.settlement(&id(10)).is_ok());
}

/// Deliberately omits the entitlement port, while retaining every other ready
/// fact. The default must reject all uses, not infer rights from readiness.
struct MissingEntitlement(View);
impl RootedCreditView for MissingEntitlement {
    fn scope(&self) -> ChainScope {
        self.0.scope()
    }
    fn claim(&self, claim: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        self.0.claim(claim)
    }
    fn checked_coverage(&self, claim: ClaimId) -> Result<CheckedCoverage, CreditDenial> {
        self.0.checked_coverage(claim)
    }
    fn checked_dependencies(&self, claim: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        self.0.checked_dependencies(claim)
    }
}

#[test]
fn missing_existing_entitlement_rejects_all_seven_ready_credit_uses() {
    let view = MissingEntitlement(view());
    let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 10, max_edges: 10 }).unwrap();
    for usage in ALL_CREDIT_USES {
        assert_eq!(gate.authorize(&id(10), usage).err(), Some(CreditDenial::Entitlement { claim: id(10), usage }));
    }
}

#[test]
fn neither_saved_pass_nor_drawn_approval_replaces_local_full_coverage() {
    let mut v = view();
    v.0.get_mut(&id(10)).unwrap().coverage = None;
    denied(&v, id(10), CreditDenial::MissingLocalVerification(id(10)));
    v.0.get_mut(&id(10)).unwrap().coverage = ready(10, &[]).coverage;
    for count in [0, 9, 11] {
        v.0.get_mut(&id(10)).unwrap().coverage.as_mut().unwrap().checked_relations = count;
        denied(&v, id(10), CreditDenial::IncompleteCoverage(id(10)));
    }
    // A smaller complete plan is not the admitted A1 mandatory plan.
    let coverage = v.0.get_mut(&id(10)).unwrap().coverage.as_mut().unwrap();
    coverage.checked_relations = 1;
    coverage.mandatory_relations = 1;
    denied(&v, id(10), CreditDenial::IncompleteCoverage(id(10)));
}

#[test]
fn every_nonpassed_outcome_is_uncredited_including_panel_failure_and_timeout() {
    for status in [
        ClaimStatus::ResultCommitted,
        ClaimStatus::Pending,
        ClaimStatus::ClaimRefuted,
        ClaimStatus::EndedUnverified,
        ClaimStatus::Revoked,
    ] {
        let mut v = view();
        v.0.get_mut(&id(10)).unwrap().facts.status = status;
        denied(&v, id(10), CreditDenial::PositiveNotPassed(id(10)));
    }
}

#[test]
fn positive_pass_does_not_replace_gate_c_or_economic_and_fraud_conditions() {
    for approval in [GateCApproval::Pending, GateCApproval::Rejected, GateCApproval::Approved(binding(11))] {
        let mut v = view();
        v.0.get_mut(&id(10)).unwrap().facts.gate_c = approval;
        denied(&v, id(10), CreditDenial::GateC(id(10)));
    }
    for state in [FraudState::AcceptedPending, FraudState::Refuted] {
        let mut v = view();
        v.0.get_mut(&id(10)).unwrap().facts.fraud = state;
        denied(&v, id(10), CreditDenial::Fraud(id(10)));
    }
    let mut v = view();
    v.0.get_mut(&id(10)).unwrap().facts.maturity_and_liability = EconomicReadiness::Held;
    denied(&v, id(10), CreditDenial::MaturityOrLiability(id(10)));
    let mut v = view();
    v.0.get_mut(&id(10)).unwrap().facts.carrier_and_funding = EconomicReadiness::Held;
    denied(&v, id(10), CreditDenial::CarrierOrFunding(id(10)));
}

#[test]
fn input_branch_ruleset_output_and_plan_substitution_fail_closed() {
    for field in 0..5 {
        let mut v = view();
        let c = v.0.get_mut(&id(10)).unwrap().coverage.as_mut().unwrap();
        match field {
            0 => c.binding.scope.branch = d(200),
            1 => c.binding.scope.ruleset = d(200),
            2 => c.binding.output = d(200),
            3 => c.binding.result = ResultId(d(200)),
            _ => c.plan = PlanId(d(200)),
        }
        denied(&v, id(10), CreditDenial::Binding(id(10)));
    }
    let mut v = view();
    v.0.get_mut(&id(10)).unwrap().facts.binding.scope.branch = d(200);
    denied(&v, id(10), CreditDenial::Scope(id(10)));
}

#[test]
fn unverified_or_revoked_ancestor_holds_only_its_descendants() {
    let mut v = View(BTreeMap::from([
        (id(10), ready(10, &[])),
        (id(11), ready(11, &[])),
        (id(12), ready(12, &[10])),
        (id(13), ready(13, &[12])),
    ]));
    for status in [ClaimStatus::Pending, ClaimStatus::Revoked, ClaimStatus::EndedUnverified] {
        v.0.get_mut(&id(10)).unwrap().facts.status = status;
        for child in [10, 12, 13] {
            denied(&v, id(child), CreditDenial::PositiveNotPassed(id(10)));
        }
        assert!(gate(&v).authorize(&id(11), CreditUse::Reward).is_ok());
    }
    v.0.get_mut(&id(10)).unwrap().facts.status = ClaimStatus::PositivePassed;
    assert!(gate(&v).authorize(&id(13), CreditUse::Settlement).is_ok());
    v.0.get_mut(&id(10)).unwrap().facts.fraud = FraudState::AcceptedPending;
    denied(&v, id(13), CreditDenial::Fraud(id(10)));
}

#[test]
fn cycles_missing_bindings_and_noncanonical_parent_lists_are_rejected() {
    let mut v = view();
    v.0.get_mut(&id(10)).unwrap().dependencies = None;
    denied(&v, id(10), CreditDenial::MissingDependencyBinding(id(10)));
    v.0.get_mut(&id(10)).unwrap().dependencies = Some(vec![binding(11)]);
    denied(&v, id(10), CreditDenial::MissingClaim(id(11)));
    v.0.get_mut(&id(10)).unwrap().dependencies = Some(vec![binding(10)]);
    denied(&v, id(10), CreditDenial::DependencyCycle(id(10)));
    v.0.get_mut(&id(10)).unwrap().dependencies = Some(vec![binding(11)]);
    v.0.insert(id(11), ready(11, &[10]));
    denied(&v, id(10), CreditDenial::DependencyCycle(id(10)));
    for parents in [vec![binding(11), binding(11)], vec![binding(12), binding(11)]] {
        v.0.get_mut(&id(10)).unwrap().dependencies = Some(parents);
        denied(&v, id(10), CreditDenial::NonCanonicalDependencies(id(10)));
    }
}

#[test]
fn diamond_checks_every_expected_export_binding_even_for_an_already_visited_parent() {
    let mut v = View(BTreeMap::from([
        (id(10), ready(10, &[])),
        (id(11), ready(11, &[10])),
        (id(12), ready(12, &[10])),
        (id(13), ready(13, &[11, 12])),
    ]));
    assert!(gate(&v).authorize(&id(13), CreditUse::PrefixReuse).is_ok());
    v.0.get_mut(&id(12)).unwrap().dependencies.as_mut().unwrap()[0].output = d(200);
    denied(&v, id(13), CreditDenial::Binding(id(10)));
}

#[test]
fn dependency_work_and_stack_growth_are_bounded_without_truncating_ancestors() {
    let v = View(BTreeMap::from([(id(10), ready(10, &[])), (id(11), ready(11, &[10])), (id(12), ready(12, &[11]))]));
    for limits in [CreditLimits { max_claims: 2, max_edges: 2 }, CreditLimits { max_claims: 3, max_edges: 1 }] {
        let gate = PositiveCreditGate::new(&v, limits).unwrap();
        assert_eq!(gate.authorize(&id(12), CreditUse::Dependency).err(), Some(CreditDenial::Budget));
    }
}

#[test]
fn the_production_native_port_remains_unconditionally_closed() {
    for usage in ALL_CREDIT_USES {
        assert_eq!(UnavailableNativeCredit.authorize(&id(10), usage), Err(CreditDenial::MissingNativeResultBinding));
    }
}

/// Synthetic lineage fixture only. The production override must obtain these
/// facts from native Result/Case rows, accepted-prefix state and ancestry
/// service capabilities; this private set is not a native admission adapter.
struct LineageFixture {
    claims: View,
    admitted_scopes: BTreeMap<ClaimId, ChainScope>,
    retained_origins: std::collections::BTreeSet<Digest>,
}
impl RootedCreditView for LineageFixture {
    fn scope(&self) -> ChainScope {
        scope()
    }
    fn claim(&self, claim: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        self.claims.claim(claim)
    }
    fn validate_scope(&self, binding: &CreditBinding) -> Result<(), CreditDenial> {
        let admitted = self.admitted_scopes.get(&binding.claim).ok_or(CreditDenial::MissingClaim(binding.claim))?;
        if binding.scope != *admitted
            || binding.scope.network != scope().network
            || binding.scope.ruleset != scope().ruleset
            || !self.retained_origins.contains(&admitted.branch)
        {
            return Err(CreditDenial::Scope(binding.claim));
        }
        Ok(())
    }
    fn checked_coverage(&self, claim: ClaimId) -> Result<CheckedCoverage, CreditDenial> {
        self.claims.checked_coverage(claim)
    }
    fn checked_dependencies(&self, claim: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        self.claims.checked_dependencies(claim)
    }
    fn authorize_entitlement(&self, binding: &CreditBinding, usage: CreditUse) -> Result<(), CreditDenial> {
        self.claims.authorize_entitlement(binding, usage)
    }
}
fn lineage_fixture() -> LineageFixture {
    let mut claims = View(BTreeMap::from([(id(10), ready(10, &[])), (id(11), ready(11, &[])), (id(12), ready(12, &[]))]));
    for (claim, row) in &mut claims.0 {
        row.facts.binding.scope.branch = claim.0;
        row.facts.gate_c = GateCApproval::Approved(row.facts.binding);
        row.coverage.as_mut().unwrap().binding = row.facts.binding;
    }
    let parent = claims.0.get(&id(10)).unwrap().facts.binding;
    claims.0.get_mut(&id(12)).unwrap().dependencies = Some(vec![parent]);
    let admitted_scopes = claims.0.iter().map(|(id, row)| (*id, row.facts.binding.scope)).collect();
    LineageFixture { claims, admitted_scopes, retained_origins: [d(10), d(11), d(12)].into_iter().collect() }
}

#[test]
fn fixed_admission_anchors_can_differ_across_a_valid_dependency_lineage() {
    let view = lineage_fixture();
    let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 10, max_edges: 10 }).unwrap();
    for usage in ALL_CREDIT_USES {
        assert!(gate.authorize(&id(12), usage).is_ok());
        assert!(gate.authorize(&id(11), usage).is_ok());
    }
    assert_eq!(gate.credited_weight(id(12), 101), 101);
}

#[test]
fn removing_a_parent_admission_origin_holds_its_child_and_preserves_independent_credit() {
    let mut view = lineage_fixture();
    // Retain stale child/parent rows deliberately: the current native scope
    // check must refuse the removed origin before any child consumer credits it.
    view.retained_origins.remove(&d(10));
    let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 10, max_edges: 10 }).unwrap();
    for usage in ALL_CREDIT_USES {
        assert_eq!(gate.authorize(&id(12), usage).err(), Some(CreditDenial::Scope(id(10))));
        assert!(gate.authorize(&id(11), usage).is_ok());
    }
    assert_eq!(gate.credited_weight(id(12), 101), 0);
}

#[test]
fn lineage_membership_never_waives_the_admitted_network_or_ruleset_binding() {
    for field in 0..2 {
        let mut view = lineage_fixture();
        let row = view.claims.0.get_mut(&id(10)).unwrap();
        if field == 0 {
            row.facts.binding.scope.network = d(200);
        } else {
            row.facts.binding.scope.ruleset = d(200);
        }
        row.facts.gate_c = GateCApproval::Approved(row.facts.binding);
        row.coverage.as_mut().unwrap().binding = row.facts.binding;
        let parent = row.facts.binding;
        view.claims.0.get_mut(&id(12)).unwrap().dependencies = Some(vec![parent]);
        let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 10, max_edges: 10 }).unwrap();
        for usage in ALL_CREDIT_USES {
            assert_eq!(gate.authorize(&id(12), usage).err(), Some(CreditDenial::Scope(id(10))));
        }
    }
}
