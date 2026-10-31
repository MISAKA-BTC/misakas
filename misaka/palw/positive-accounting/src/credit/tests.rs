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
