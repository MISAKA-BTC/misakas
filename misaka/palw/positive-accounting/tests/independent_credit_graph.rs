//! Independent public-API tests. These synthetic rooted records test the A5
//! predicate, not actual Court execution or native admission.

use misaka_palw_positive_accounting::credit::{
    ALL_CREDIT_USES, CheckedCoverage, CreditBinding, CreditDenial, CreditLimits, EconomicReadiness, FraudState, GateCApproval,
    PositiveCreditGate, RootedCreditFacts, RootedCreditView,
};
use misaka_palw_positive_contract::{
    identity::{CaseId, ChainScope, ClaimId, Digest, PlanId, ResultId},
    messages::ClaimStatus,
    ports::{CreditGate, CreditUse},
};
use std::cell::RefCell;

fn digest(n: u8) -> Digest {
    Digest([n; 64])
}
fn claim(n: usize) -> ClaimId {
    ClaimId(digest(n as u8 + 10))
}
fn scope() -> ChainScope {
    ChainScope { network: digest(1), ruleset: digest(2), branch: digest(3) }
}
fn binding(n: usize) -> CreditBinding {
    CreditBinding {
        scope: scope(),
        claim: claim(n),
        case: CaseId(digest(n as u8 + 20)),
        result: ResultId(digest(n as u8 + 30)),
        output: digest(n as u8 + 40),
        dependencies: digest(n as u8 + 50),
    }
}
struct GraphView {
    ready: u8,
    parents: [Vec<CreditBinding>; 3],
    entitlements: PrivateEntitlements,
}
/// Explicit synthetic rights for this private fixture, never native rights.
enum PrivateEntitlements {
    All,
    Only(Vec<(CreditBinding, CreditUse)>),
}
impl RootedCreditView for GraphView {
    fn scope(&self) -> ChainScope {
        scope()
    }
    fn claim(&self, id: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        let n = (0..3).find(|n| claim(*n) == id).ok_or(CreditDenial::MissingClaim(id))?;
        Ok(RootedCreditFacts {
            binding: binding(n),
            plan: PlanId(digest(99)),
            mandatory_relations: 17,
            status: if self.ready & (1 << n) != 0 { ClaimStatus::PositivePassed } else { ClaimStatus::Pending },
            gate_c: GateCApproval::Approved(binding(n)),
            fraud: FraudState::Clear,
            maturity_and_liability: EconomicReadiness::Ready,
            carrier_and_funding: EconomicReadiness::Ready,
        })
    }
    fn checked_coverage(&self, id: ClaimId) -> Result<CheckedCoverage, CreditDenial> {
        let facts = self.claim(id)?;
        Ok(CheckedCoverage {
            binding: facts.binding,
            plan: facts.plan,
            mandatory_relations: 17,
            checked_relations: 17,
            status: ClaimStatus::PositivePassed,
        })
    }
    fn checked_dependencies(&self, id: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        let n = (0..3).find(|n| claim(*n) == id).ok_or(CreditDenial::MissingClaim(id))?;
        Ok(&self.parents[n])
    }
    fn authorize_entitlement(&self, binding: &CreditBinding, usage: CreditUse) -> Result<(), CreditDenial> {
        match &self.entitlements {
            PrivateEntitlements::All => Ok(()),
            PrivateEntitlements::Only(rights) if rights.contains(&(*binding, usage)) => Ok(()),
            PrivateEntitlements::Only(_) => Err(CreditDenial::Entitlement { claim: binding.claim, usage }),
        }
    }
}

/// An independent oracle: boolean matrix closure rather than the implementation's
/// bounded DFS/coloring. Cycles outside the root's reachable ancestors are inert.
fn expected(mut reach: [[bool; 3]; 3], root: usize, ready: u8) -> bool {
    for k in 0..3 {
        for i in 0..3 {
            for j in 0..3 {
                reach[i][j] |= reach[i][k] && reach[k][j];
            }
        }
    }
    (0..3).all(|n| {
        if n != root && !reach[root][n] {
            return true;
        }
        ready & (1 << n) != 0 && !reach[n][n]
    })
}

#[test]
fn all_three_vertex_graphs_and_readiness_sets_match_the_independent_oracle() {
    // Includes all self edges, directed cycles, diamonds and unrelated components.
    // Every graph explicitly has all PRIVATE fixture entitlements: the oracle
    // tests the shared Positive/ancestry predicate conditional on those rights.
    for edges in 0u16..512 {
        let mut adjacency = [[false; 3]; 3];
        let parents = std::array::from_fn(|child| {
            (0..3)
                .filter_map(|parent| {
                    let present = edges & (1 << (child * 3 + parent)) != 0;
                    adjacency[child][parent] = present;
                    present.then(|| binding(parent))
                })
                .collect()
        });
        for ready in 0..8 {
            let view = GraphView { ready, parents: parents.clone(), entitlements: PrivateEntitlements::All };
            let gate = PositiveCreditGate::new(&view, CreditLimits { max_claims: 3, max_edges: 9 }).unwrap();
            for root in 0..3 {
                let allow = expected(adjacency, root, ready);
                for usage in ALL_CREDIT_USES {
                    assert_eq!(
                        gate.authorize(&claim(root), usage).is_ok(),
                        allow,
                        "edges={edges:09b}, ready={ready:03b}, root={root}, use={usage:?}"
                    );
                }
                assert_eq!(gate.credited_weight(claim(root), 123_456_789), if allow { 123_456_789 } else { 0 });
            }
        }
    }
}

#[test]
fn the_first_node_over_the_cap_is_not_silently_treated_as_an_approved_leaf() {
    let view = GraphView { ready: 7, parents: [vec![binding(1)], vec![binding(2)], vec![]], entitlements: PrivateEntitlements::All };
    for limits in [CreditLimits { max_claims: 2, max_edges: 9 }, CreditLimits { max_claims: 3, max_edges: 1 }] {
        let gate = PositiveCreditGate::new(&view, limits).unwrap();
        for usage in ALL_CREDIT_USES {
            assert_eq!(gate.authorize(&claim(0), usage).err(), Some(CreditDenial::Budget));
        }
    }
}

/// Deliberately omits the entitlement override despite all other ready facts.
struct NoEntitlementImplementation(GraphView);
impl RootedCreditView for NoEntitlementImplementation {
    fn scope(&self) -> ChainScope {
        self.0.scope()
    }
    fn claim(&self, id: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        self.0.claim(id)
    }
    fn checked_coverage(&self, id: ClaimId) -> Result<CheckedCoverage, CreditDenial> {
        self.0.checked_coverage(id)
    }
    fn checked_dependencies(&self, id: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        self.0.checked_dependencies(id)
    }
}

fn limits() -> CreditLimits {
    CreditLimits { max_claims: 3, max_edges: 9 }
}

#[test]
fn a_positive_ready_view_without_an_entitlement_implementation_denies_every_use() {
    let view =
        NoEntitlementImplementation(GraphView { ready: 7, parents: [vec![], vec![], vec![]], entitlements: PrivateEntitlements::All });
    let gate = PositiveCreditGate::new(&view, limits()).unwrap();
    for usage in ALL_CREDIT_USES {
        assert_eq!(gate.authorize(&claim(0), usage).err(), Some(CreditDenial::Entitlement { claim: claim(0), usage }));
    }
    assert_eq!(gate.credited_weight(claim(0), u128::MAX), 0);
    let consumers = misaka_palw_positive_accounting::credit::consumers::CreditConsumers::new(&gate);
    assert_eq!(consumers.dag_weight(&claim(0), u128::MAX), 0);
    assert_eq!(consumers.rule_e_capital(&claim(0), u128::MAX), 0);
    assert_eq!(
        consumers.work_receipt(&claim(0)).err(),
        Some(CreditDenial::Entitlement { claim: claim(0), usage: CreditUse::WorkReceipt })
    );
}

#[test]
fn one_existing_use_never_implies_any_of_the_other_seven_rights() {
    let uses = [
        CreditUse::Reward,
        CreditUse::WorkReceipt,
        CreditUse::DagWeight,
        CreditUse::RuleE,
        CreditUse::Settlement,
        CreditUse::PrefixReuse,
        CreditUse::Dependency,
        CreditUse::WorkerPayment,
    ];
    assert_eq!(ALL_CREDIT_USES.len(), uses.len());
    assert!(uses.iter().all(|usage| ALL_CREDIT_USES.contains(usage)));
    for granted in uses {
        let view = GraphView {
            ready: 7,
            parents: [vec![], vec![], vec![]],
            entitlements: PrivateEntitlements::Only(vec![(binding(0), granted)]),
        };
        let gate = PositiveCreditGate::new(&view, limits()).unwrap();
        for requested in uses {
            let result = gate.authorize(&claim(0), requested);
            if requested == granted {
                assert_eq!(result.unwrap().usage(), requested);
            } else {
                assert_eq!(result.err(), Some(CreditDenial::Entitlement { claim: claim(0), usage: requested }));
            }
        }
    }
}

#[test]
fn paid_worker_payment_and_contract_settlement_cannot_authorize_primary_reward_or_work_credit() {
    // Explicit private rights for a paid-only case. They authorize contractual
    // payment/settlement, not primary issuance or B/F/R economic work credit.
    let view = GraphView {
        ready: 7,
        parents: [vec![], vec![], vec![]],
        entitlements: PrivateEntitlements::Only(vec![(binding(0), CreditUse::WorkerPayment), (binding(0), CreditUse::Settlement)]),
    };
    let gate = PositiveCreditGate::new(&view, limits()).unwrap();
    for usage in ALL_CREDIT_USES {
        assert_eq!(
            gate.authorize(&claim(0), usage).is_ok(),
            matches!(usage, CreditUse::WorkerPayment | CreditUse::Settlement),
            "requested={usage:?}"
        );
    }
    let consumers = misaka_palw_positive_accounting::credit::consumers::CreditConsumers::new(&gate);
    assert_eq!(consumers.worker_payment(&claim(0)).unwrap().usage(), CreditUse::WorkerPayment);
    assert_eq!(consumers.settlement(&claim(0)).unwrap().usage(), CreditUse::Settlement);
    assert_eq!(
        consumers.inference_reward(&claim(0)).err(),
        Some(CreditDenial::Entitlement { claim: claim(0), usage: CreditUse::Reward })
    );
    assert_eq!(consumers.dag_weight(&claim(0), u128::MAX), 0);
    assert_eq!(consumers.rule_e_capital(&claim(0), u128::MAX), 0);
    assert_eq!(
        consumers.work_receipt(&claim(0)).err(),
        Some(CreditDenial::Entitlement { claim: claim(0), usage: CreditUse::WorkReceipt })
    );
}

#[test]
fn positive_ready_reuse_rights_do_not_create_paid_work_credit() {
    // Synthetic reuse-only rights model the boundary for a paid Final with no
    // existing B/F/R work entitlement. Positive/P readiness cannot add one.
    let view = GraphView {
        ready: 7,
        parents: [vec![], vec![], vec![]],
        entitlements: PrivateEntitlements::Only(vec![(binding(0), CreditUse::PrefixReuse), (binding(0), CreditUse::Dependency)]),
    };
    let gate = PositiveCreditGate::new(&view, limits()).unwrap();
    for usage in ALL_CREDIT_USES {
        assert_eq!(
            gate.authorize(&claim(0), usage).is_ok(),
            matches!(usage, CreditUse::PrefixReuse | CreditUse::Dependency),
            "requested={usage:?}"
        );
    }
    let consumers = misaka_palw_positive_accounting::credit::consumers::CreditConsumers::new(&gate);
    assert_eq!(consumers.dag_weight(&claim(0), u128::MAX), 0);
    assert_eq!(consumers.rule_e_capital(&claim(0), u128::MAX), 0);
    assert_eq!(
        consumers.work_receipt(&claim(0)).err(),
        Some(CreditDenial::Entitlement { claim: claim(0), usage: CreditUse::WorkReceipt })
    );
}

struct RecordingEntitlements {
    view: GraphView,
    calls: RefCell<Vec<(CreditBinding, CreditUse)>>,
}
impl RootedCreditView for RecordingEntitlements {
    fn scope(&self) -> ChainScope {
        self.view.scope()
    }
    fn claim(&self, id: ClaimId) -> Result<RootedCreditFacts, CreditDenial> {
        self.view.claim(id)
    }
    fn checked_coverage(&self, id: ClaimId) -> Result<CheckedCoverage, CreditDenial> {
        self.view.checked_coverage(id)
    }
    fn checked_dependencies(&self, id: ClaimId) -> Result<&[CreditBinding], CreditDenial> {
        self.view.checked_dependencies(id)
    }
    fn authorize_entitlement(&self, binding: &CreditBinding, usage: CreditUse) -> Result<(), CreditDenial> {
        self.calls.borrow_mut().push((*binding, usage));
        self.view.authorize_entitlement(binding, usage)
    }
}

#[test]
fn root_receives_the_requested_use_and_all_ancestors_receive_only_dependency_use() {
    for requested in ALL_CREDIT_USES {
        let view = RecordingEntitlements {
            view: GraphView {
                ready: 7,
                parents: [vec![binding(1)], vec![binding(2)], vec![]],
                entitlements: PrivateEntitlements::Only(vec![
                    (binding(0), requested),
                    (binding(1), CreditUse::Dependency),
                    (binding(2), CreditUse::Dependency),
                ]),
            },
            calls: RefCell::new(Vec::new()),
        };
        let gate = PositiveCreditGate::new(&view, limits()).unwrap();
        assert_eq!(gate.authorize(&claim(0), requested).unwrap().usage(), requested);
        let calls = view.calls.borrow();
        for expected in [(binding(0), requested), (binding(1), CreditUse::Dependency), (binding(2), CreditUse::Dependency)] {
            assert!(calls.contains(&expected), "missing {expected:?}, requested={requested:?}");
        }
        assert!(calls.iter().all(|(actual, usage)| {
            (*actual == binding(0) && *usage == requested)
                || ([binding(1), binding(2)].contains(actual) && *usage == CreditUse::Dependency)
        }));
    }
}

#[test]
fn losing_an_ancestor_dependency_right_denies_a_fresh_read_even_with_other_rights() {
    for requested in ALL_CREDIT_USES {
        let mut view =
            GraphView { ready: 7, parents: [vec![binding(1)], vec![binding(2)], vec![]], entitlements: PrivateEntitlements::All };
        assert!(PositiveCreditGate::new(&view, limits()).unwrap().authorize(&claim(0), requested).is_ok());
        // Keep all rights on the root and immediate parent. The grandparent's
        // Positive/Gate C facts and every other right remain ready too.
        view.entitlements = PrivateEntitlements::Only(
            ALL_CREDIT_USES
                .into_iter()
                .flat_map(|usage| [(binding(0), usage), (binding(1), usage)])
                .chain(ALL_CREDIT_USES.into_iter().filter(|usage| *usage != CreditUse::Dependency).map(|usage| (binding(2), usage)))
                .collect(),
        );
        assert_eq!(
            PositiveCreditGate::new(&view, limits()).unwrap().authorize(&claim(0), requested).err(),
            Some(CreditDenial::Entitlement { claim: claim(2), usage: CreditUse::Dependency })
        );
    }
}
