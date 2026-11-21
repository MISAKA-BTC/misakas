//! Local G-COST reduction over authenticated, completed measurements from a
//! single frozen suite. Transport, evidence authentication and the choice of
//! suite and measurement unit belong to the owning admission reducer.
use super::Verdict;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::RangeInclusive,
};

type Digest = [u8; 64];

pub struct CostPlan<'a> {
    /// Binds the descriptor, generation, rules, unit, routes, trial schedule
    /// and cutoff. Fresh trial inputs are resolved after the audit draw.
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
    /// Inclusive bounds established by the suite's fixed measurement procedure.
    /// Deterministic unit counts may use equal endpoints.
    pub units: RangeInclusive<u64>,
}

pub struct RouteRun {
    pub route: Digest,
    pub run: CostRun,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CostReview(Option<(Digest, Verdict)>);

impl CostReview {
    /// The result of this particular measurement, for reporting.
    pub fn verdict(self) -> Verdict {
        self.0.map_or(Verdict::Unknown, |(_, verdict)| verdict)
    }

    /// Admission supplies its own required scope from the current instance and suite.
    pub(super) fn verdict_for(self, required: Option<Digest>) -> Verdict {
        match self.0 {
            Some((scope, verdict)) if Some(scope) == required => verdict,
            _ => Verdict::Unknown,
        }
    }

    /// The owning reducer supplies the applicable failed input/route pairs
    /// from its case history, independently of the proposed suite.
    pub fn assess(plan: &CostPlan<'_>, references: &[CostRun], candidates: &[RouteRun], prior_failures: &[(Digest, Digest)]) -> Self {
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
        let regression_inputs: BTreeSet<_> = prior_failures.iter().map(|(input, _)| input).collect();
        let covers_history = prior_failures.iter().all(|(input, route)| inputs.contains(input) && routes.contains(route))
            && inputs.difference(&regression_inputs).take(2).count() == 2;
        let mut baseline = BTreeMap::new();
        for run in references {
            if run.scope != plan.scope
                || !inputs.contains(&run.input)
                || run.units.is_empty()
                || *run.units.start() == 0
                || baseline.insert(run.input, run).is_some()
            {
                return Self::default();
            }
        }
        let mut seen = BTreeSet::new();
        let mut cheap = false;
        let mut uncertain = false;
        for candidate in candidates {
            let run = &candidate.run;
            let Some(reference) = baseline.get(&run.input) else { return Self::default() };
            if run.scope != plan.scope
                || run.units.is_empty()
                || !routes.contains(&candidate.route)
                || !seen.insert((run.input, candidate.route))
            {
                return Self::default();
            }
            if run.ticket_input == reference.ticket_input {
                let cutoff = u128::from(plan.cutoff_permille);
                cheap |= u128::from(*run.units.end()) * 1000 < u128::from(*reference.units.start()) * cutoff;
                uncertain |= u128::from(*run.units.start()) * 1000 < u128::from(*reference.units.end()) * cutoff;
            }
        }
        if cheap {
            Self(Some((plan.scope, Verdict::Fail)))
        } else if !uncertain && covers_history && inputs.len().checked_mul(routes.len()) == Some(seen.len()) {
            Self(Some((plan.scope, Verdict::Pass)))
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
        let run = |i: usize, units| CostRun { scope: plan.scope, input: inputs[i], ticket_input: inputs[i], units: units..=units };
        let references = [run(0, 100), run(1, 100)];
        let mut rows = [
            RouteRun { route: routes[0], run: run(0, 50) },
            RouteRun { route: routes[1], run: run(0, 100) },
            RouteRun { route: routes[0], run: run(1, 51) },
            RouteRun { route: routes[1], run: run(1, 100) },
        ];
        let review = |r: &[RouteRun]| CostReview::assess(&plan, &references, r, &[]).verdict();
        assert_eq!(review(&rows), Verdict::Pass);
        assert_eq!(review(&rows[..3]), Verdict::Unknown);
        rows[0].run.units = 49..=50;
        assert_eq!(review(&rows), Verdict::Unknown);
        rows[0].run.units = 48..=49;
        let failed = CostReview::assess(&plan, &references, &rows[..1], &[]);
        assert_eq!(failed.verdict_for(Some(plan.scope)), Verdict::Fail);
        assert_eq!(failed.verdict_for(Some([8; 64])), Verdict::Unknown);
        assert_eq!(failed.verdict_for(None), Verdict::Unknown);
        rows[0].run.ticket_input = [8; 64];
        assert_eq!(review(&rows), Verdict::Pass);
        rows[0].run.scope = [8; 64];
        assert_eq!(review(&rows), Verdict::Unknown);
        rows[0].run.scope = plan.scope;
        rows[0].route = routes[1];
        assert_eq!(review(&rows), Verdict::Unknown);
        rows[0].route = routes[0];
        rows[0].run.ticket_input = inputs[0];
        rows[0].run.units = 50..=55;
        let mut variable_reference = [run(0, 100), run(1, 100)];
        variable_reference[0].units = 100..=110;
        assert_eq!(CostReview::assess(&plan, &variable_reference, &rows, &[]).verdict(), Verdict::Unknown);
        rows[0].run.units = 55..=60;
        assert_eq!(CostReview::assess(&plan, &variable_reference, &rows, &[]).verdict(), Verdict::Pass);
        let single = CostPlan { inputs: &inputs[..1], ..plan };
        assert_eq!(CostReview::assess(&single, &references[..1], &rows[..1], &[]).verdict(), Verdict::Unknown);
    }

    #[test]
    fn repaired_suite_keeps_the_failure_and_two_fresh_inputs() {
        let inputs = [[1; 64], [2; 64], [3; 64]];
        let routes = [[4; 64]];
        let plan = CostPlan { scope: [9; 64], inputs: &inputs, routes: &routes, cutoff_permille: 500 };
        let run = |input| CostRun { scope: plan.scope, input, ticket_input: input, units: 100..=100 };
        let references = inputs.map(run);
        let candidates = inputs.map(|i| RouteRun { route: routes[0], run: run(i) });
        let review = |cases: &[(Digest, Digest)]| CostReview::assess(&plan, &references, &candidates, cases).verdict();
        assert_eq!(review(&[(inputs[0], routes[0])]), Verdict::Pass);
        assert_eq!(review(&[([7; 64], routes[0])]), Verdict::Unknown);
        assert_eq!(review(&[(inputs[0], [7; 64])]), Verdict::Unknown);
        assert_eq!(review(&[(inputs[0], routes[0]), (inputs[1], routes[0])]), Verdict::Unknown);
    }
}
