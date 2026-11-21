//! Local G-COST reduction over authenticated, completed measurements from a
//! single frozen suite. Transport, evidence authentication and the choice of
//! suite and measurement unit belong to the owning admission reducer.
use super::Verdict;
use std::collections::{BTreeMap, BTreeSet};

type Digest = [u8; 64];

pub struct CostPlan<'a> {
    /// Binds the descriptor, generation, rules, unit, routes, trial schedule
    /// and cutoff. Actual trial inputs are resolved after the audit draw.
    pub scope: Digest,
    pub inputs: &'a [Digest],
    pub routes: &'a [Digest],
    /// A matching result strictly below this fraction of the reference fails.
    pub cutoff_permille: u16,
}

pub struct CostRun {
    pub scope: Digest,
    pub input: Digest,
    /// Digest of the canonical pre-hash Ticket input, including all four roots.
    pub ticket_input: Digest,
    /// Same accounting unit, including the plan's allocation of shared setup.
    pub units: u64,
}

pub struct RouteRun {
    pub route: Digest,
    pub run: CostRun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CostReview(Verdict);

impl Default for CostReview {
    fn default() -> Self {
        Self(Verdict::Unknown)
    }
}

impl CostReview {
    pub fn verdict(self) -> Verdict {
        self.0
    }

    pub fn assess(plan: &CostPlan<'_>, references: &[CostRun], candidates: &[RouteRun]) -> Self {
        let inputs: BTreeSet<_> = plan.inputs.iter().collect();
        let routes: BTreeSet<_> = plan.routes.iter().collect();
        if inputs.len() < 2
            || routes.is_empty()
            || inputs.len() != plan.inputs.len()
            || routes.len() != plan.routes.len()
            || !(1..=1000).contains(&plan.cutoff_permille)
        {
            return Self::default();
        }
        let mut baseline = BTreeMap::new();
        for run in references {
            if run.scope != plan.scope || !inputs.contains(&run.input) || run.units == 0 || baseline.insert(run.input, run).is_some() {
                return Self::default();
            }
        }
        let mut seen = BTreeSet::new();
        let mut cheap = false;
        for candidate in candidates {
            let run = &candidate.run;
            let Some(reference) = baseline.get(&run.input) else { return Self::default() };
            if run.scope != plan.scope || !routes.contains(&candidate.route) || !seen.insert((run.input, candidate.route)) {
                return Self::default();
            }
            cheap |= run.ticket_input == reference.ticket_input
                && u128::from(run.units) * 1000 < u128::from(reference.units) * u128::from(plan.cutoff_permille);
        }
        if cheap {
            Self(Verdict::Fail)
        } else if inputs.len().checked_mul(routes.len()) == Some(seen.len()) {
            Self(Verdict::Pass)
        } else {
            Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_cost_screen_uses_every_planned_input_and_route() {
        let inputs = [[1; 64], [2; 64]];
        let routes = [[3; 64], [4; 64]];
        let plan = CostPlan { scope: [9; 64], inputs: &inputs, routes: &routes, cutoff_permille: 500 };
        let run = |i: usize, units| CostRun { scope: plan.scope, input: inputs[i], ticket_input: inputs[i], units };
        let references = [run(0, 100), run(1, 100)];
        let mut rows = [
            RouteRun { route: routes[0], run: run(0, 50) },
            RouteRun { route: routes[1], run: run(0, 100) },
            RouteRun { route: routes[0], run: run(1, 51) },
            RouteRun { route: routes[1], run: run(1, 100) },
        ];
        let review = |r: &[RouteRun]| CostReview::assess(&plan, &references, r).verdict();
        assert_eq!(review(&rows), Verdict::Pass);
        assert_eq!(review(&rows[..3]), Verdict::Unknown);
        rows[0].run.units = 49;
        assert_eq!(review(&rows[..1]), Verdict::Fail);
        rows[0].run.ticket_input = [8; 64];
        assert_eq!(review(&rows), Verdict::Pass);
        rows[0].run.scope = [8; 64];
        assert_eq!(review(&rows), Verdict::Unknown);
        rows[0].run.scope = plan.scope;
        rows[0].route = routes[1];
        assert_eq!(review(&rows), Verdict::Unknown);
        let single = CostPlan { inputs: &inputs[..1], ..plan };
        assert_eq!(CostReview::assess(&single, &references[..1], &rows[..1]).verdict(), Verdict::Unknown);
    }
}
