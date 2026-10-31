//! Independent public-API tests. These synthetic rooted records test the A5
//! predicate, not actual Court execution or native admission.

use misaka_palw_positive_accounting::credit::{
    ALL_CREDIT_USES, CheckedCoverage, CreditBinding, CreditDenial, CreditLimits, EconomicReadiness, FraudState, GateCApproval,
    PositiveCreditGate, RootedCreditFacts, RootedCreditView,
};
use misaka_palw_positive_contract::{
    identity::{CaseId, ChainScope, ClaimId, Digest, PlanId, ResultId},
    messages::ClaimStatus,
    ports::CreditGate,
};

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
            let view = GraphView { ready, parents: parents.clone() };
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
    let view = GraphView { ready: 7, parents: [vec![binding(1)], vec![binding(2)], vec![]] };
    for limits in [CreditLimits { max_claims: 2, max_edges: 9 }, CreditLimits { max_claims: 3, max_edges: 1 }] {
        let gate = PositiveCreditGate::new(&view, limits).unwrap();
        for usage in ALL_CREDIT_USES {
            assert_eq!(gate.authorize(&claim(0), usage).err(), Some(CreditDenial::Budget));
        }
    }
}
