//! **One reading of "this class has seats" for every class kind** — RFC-0002 Part II §II.7.5 Proposal A
//! (`palw_class_seating`, lane F, approved 2026-10-01), extracted from RFC-0003's tensor-claim gate
//! (`GenClassNotReady`, §I.4.5) so an IR attempt class, a generative class and a composite class all ask the SAME
//! function and the doors cannot disagree. A child module of `palw_state_v2`, as the generative fold is: it reads the
//! fold's state and its one readiness predicate (`palw_seat_class_not_ready_reason_net_v1` — active, above the floor, a
//! fresh readiness V2 row, free collateral for the readiness multiple, and a composite's parent clause) and restates
//! nothing.
//!
//! # The two floors
//!
//! A class is *seated* for a claim whose executor is `e` iff
//!
//! 1. **possession floor** — `|Ready \ {operator(e)}| ≥ seat_count`: a panel can be drawn without the executor;
//! 2. **independence floor** — `|(Ready ∩ Base) \ {operator(registrant), operator(e)}| ≥ independent_floor`, where
//!    `independent_floor` is the fence's own value ([`PalwClassSeatingTermsV1`], 3 on testnet-12) and `Base` is the
//!    network's base-class population as the admission jury and the per-claim outsider draw read it
//!    ([`palw_base_population_v1`]: one function, so the jury, the outsider seat and this floor cannot drift apart).
//!
//! `Ready` counts **operators**, each once however many bonds it holds. A genesis class has no registrant to exclude.
//!
//! # The function every door asks
//!
//! [`palw_class_seating_v1`] is pure over the state, the bundle's params and the registry fold; it has no side effect.
//! The claim doors ask it through [`PalwFoldReadV1::check_class_seated_v1`] (one error, `ClassNotSeated`, with the generative
//! lane keeping `GenClassNotReady` as its name for the possession floor), the lifecycle step asks it with no executor, and
//! the registry read serves it (`seating`) to the RPC and the preflight. Nothing here reads a fence except through the
//! terms the caller passes: below `palw_class_seating` the claim doors do not ask the independence floor at all, and the
//! generative lane's possession floor is what it was.
//!
//! The cost is one pass over the bond table per ask (the readiness predicate is evaluated once per bond); a lane that asks
//! it for every claim of a block may want a per-block cache keyed by `(class, executor operator)` — the answer depends on
//! nothing else the block's objects can change before the claim folds.

use super::*;
pub use crate::palw_class_seating_fence_v1::PalwClassSeatingTermsV1;
use crate::palw_model_registry_v1::PalwModelRegistryFoldV1;

/// **What the predicate counted** — every number a refusal, the lifecycle, the registry RPC and the preflight need.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClassSeatingV1 {
    /// `|Ready \ {operator(executor)}|` — condition 1, have. With no executor nothing is excluded.
    pub ready_operators: u32,
    /// `seat_count` — condition 1, need.
    pub needed_operators: u32,
    /// `|(Ready ∩ Base) \ {operator(registrant), operator(executor)}|` — condition 2, have.
    pub independent_operators: u32,
    /// The fence's `independent_floor` — condition 2, need.
    pub needed_independent: u32,
    /// `|Base \ {operator(registrant)}|` — the licensable share's denominator.
    pub base_operators: u32,
}

/// **Which floor failed** — possession is asked first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClassNotSeatedV1 {
    Possession { ready: u32, needed: u32 },
    Independence { independent: u32, needed: u32 },
}

impl PalwClassSeatingV1 {
    /// **The verdict**: condition 1 first (possession), then condition 2 (independence).
    pub fn verdict(&self) -> Result<(), PalwClassNotSeatedV1> {
        if self.ready_operators < self.needed_operators {
            return Err(PalwClassNotSeatedV1::Possession { ready: self.ready_operators, needed: self.needed_operators });
        }
        if self.independent_operators < self.needed_independent {
            return Err(PalwClassNotSeatedV1::Independence {
                independent: self.independent_operators,
                needed: self.needed_independent,
            });
        }
        Ok(())
    }

    /// **The class's licensable share** — the probability, in permille, that a claim's outsider (drawn over all of `Base`, not
    /// over its holders: ADR-0147's price, kept on purpose) holds the class: `independent_operators × 1000 / base_operators`,
    /// 0 for an empty base, never above 1,000. Visibility only: no gate reads it.
    pub fn licensable_share_permille(&self) -> u16 {
        if self.base_operators == 0 {
            return 0;
        }
        (u64::from(self.independent_operators) * 1_000 / u64::from(self.base_operators)).min(1_000) as u16
    }

    /// Does the independence floor hold? (The lifecycle's `independence_floor_met`.)
    pub fn independence_floor_met(&self) -> bool {
        self.independent_operators >= self.needed_independent
    }
}

/// **The base population `Base`**, as the admission jury draws it and the outsider seat is drawn from
/// (ADR-0147; the Activation Pool's R2 floor): active bonds a panel can draw — collateral at the panel floor, serving the
/// liveness floor's class — registered before the span's cut (`(span_now − 1) × span_daa`; a claim's door passes
/// `span_now = daa / span_daa`, the jury the span it sits in) and, past
/// `palw_bond_maturity_early`, matured, **less the class's registrant's bond and operator**. One function: the jury's filter,
/// extracted, so the jury, the outsider draw and the seating floor read one population.
///
/// `registrant` is the class record's `registrant_bond` (`None` for a genesis class); `activation_pool` selects the panel
/// floor (past the Activation Pool, which `validate_palw_class_seating` requires of the seating fence) or the network
/// floor (the jury below it).
pub(super) fn palw_base_population_v1<'a>(
    state: &'a PalwChainStateV2,
    params: &PalwStateParamsV2,
    fold: &PalwModelRegistryFoldV1,
    class_id: &Hash64,
    daa: u64,
    span_now: u64,
    activation_pool: bool,
) -> Vec<(&'a PalwBondKeyV2, &'a PalwBondStateV2)> {
    let span_daa = fold.span_daa.max(1);
    let cutoff = span_now.saturating_sub(1).saturating_mul(span_daa);
    let base = params.base_class_id();
    let floor = if activation_pool {
        crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(params.min_collateral_sompi())
    } else {
        params.min_collateral_sompi()
    };
    let registrant = state.classes.get(class_id).and_then(|record| record.registrant_bond);
    let registrant_operator = registrant.and_then(|key| state.bonds.get(&key)).map(|bond| bond.operator_id);
    let matured_by = fold.bond_maturity.map(|maturity| maturity.registered_by_daa(state, params.window_court(), daa));
    state
        .bonds
        .iter()
        .filter(|(key, bond)| {
            matches!(bond.status, PalwBondStatusV2::Active)
                && palw_bond_may_take_work_v2(bond, floor)
                && palw_bond_may_judge_class_v2(bond, &base)
                && bond.registered_daa < cutoff
                && matured_by.is_none_or(|by| bond.registered_daa <= by)
                && Some(**key) != registrant
                && Some(bond.operator_id) != registrant_operator
        })
        .collect()
}

/// **A class's seating, for an executor or for none** (see the module doc). With `Some(executor)` the executor's bond and
/// every bond of its operator are excluded — a claim's door; with `None` nothing is excluded — the lifecycle step and the
/// registry read, which have no executor. `None` is returned only when `Some(executor)` is not in the state (the caller
/// refuses the claim for it before it asks); with no executor it always returns `Some`.
///
/// Pure: it reads the state, the bundle's params and the registry fold only, so the fold, the lifecycle, the RPC and the
/// preflight cannot read different things. `activation_pool` is the Activation Pool's presence (the base population's floor).
pub fn palw_class_seating_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    fold: &PalwModelRegistryFoldV1,
    class_id: &Hash64,
    executor: Option<&PalwBondKeyV2>,
    daa: u64,
    terms: PalwClassSeatingTermsV1,
    activation_pool: bool,
) -> Option<PalwClassSeatingV1> {
    palw_class_seating_root_v1(state, params, fold, class_id, None, executor, daa, terms, activation_pool)
}

/// **[`palw_class_seating_v1`] for the seats that hold ONE ROOT of the class** (lane MU, ADR-0173): `root = None` is the
/// registered (founding) root, whose rows are the legacy table's; any other root reads its own `(bond, class, root)` rows.
#[allow(clippy::too_many_arguments)]
pub fn palw_class_seating_root_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    fold: &PalwModelRegistryFoldV1,
    class_id: &Hash64,
    root: Option<&Hash64>,
    executor: Option<&PalwBondKeyV2>,
    daa: u64,
    terms: PalwClassSeatingTermsV1,
    activation_pool: bool,
) -> Option<PalwClassSeatingV1> {
    let ready = class_ready_operators_v1(state, params, fold, class_id, root, executor, daa)?;
    let base =
        palw_base_population_v1(state, params, fold, class_id, daa, crate::palw_execution_lane_v1::palw_execution_span_v1(daa, fold.span_daa), activation_pool);
    let in_base: std::collections::BTreeSet<&PalwBondKeyV2> = base.iter().map(|(key, _)| *key).collect();
    let registrant_operator = state
        .classes
        .get(class_id)
        .and_then(|record| record.registrant_bond)
        .and_then(|key| state.bonds.get(&key))
        .map(|bond| bond.operator_id);
    let independent = ready
        .iter()
        .filter(|(operator, bonds)| Some(**operator) != registrant_operator && bonds.iter().any(|key| in_base.contains(key)))
        .count();
    let base_operators: std::collections::BTreeSet<Hash64> = base.iter().map(|(_, bond)| bond.operator_id).collect();
    Some(PalwClassSeatingV1 {
        ready_operators: ready.len().min(u32::MAX as usize) as u32,
        needed_operators: u32::from(fold.globals.seat_count),
        independent_operators: independent.min(u32::MAX as usize) as u32,
        needed_independent: u32::from(terms.independent_floor),
        base_operators: base_operators.len().min(u32::MAX as usize) as u32,
    })
}

/// **The operators that hold the class with a fresh possession proof** — each operator once, with the ready bonds of each in key
/// order — by the registry's own five-clause predicate (and a composite's parent clause), the executor's bond and its whole
/// operator excluded. The read both [`palw_class_seating_v1`] and the generative lane's possession floor take.
fn class_ready_operators_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    fold: &PalwModelRegistryFoldV1,
    class_id: &Hash64,
    root: Option<&Hash64>,
    executor: Option<&PalwBondKeyV2>,
    now_daa: u64,
) -> Option<BTreeMap<Hash64, Vec<PalwBondKeyV2>>> {
    let executor_operator = match executor {
        Some(key) => Some(state.bonds.get(key)?.operator_id),
        None => None,
    };
    let mut ready: BTreeMap<Hash64, Vec<PalwBondKeyV2>> = BTreeMap::new();
    for (key, bond) in state.bonds.iter() {
        if executor == Some(key) || executor_operator == Some(bond.operator_id) {
            continue;
        }
        let Some(row) = state.seat_readiness_for_root(key, class_id, root) else { continue };
        let not_ready = crate::palw_model_registry_v1::palw_seat_class_not_ready_reason_v1(state, params, key, class_id, row, now_daa, fold);
        if not_ready.is_none() {
            ready.entry(bond.operator_id).or_default().push(*key);
        }
    }
    Some(ready)
}

impl PalwFoldReadV1<'_> {
    /// **The one door every claim of every class kind asks** (Proposal A, past `Params::palw_class_seating`): the class is
    /// *seated* for `executor` at `daa`, else `ClassNotSeated` naming the floor that failed (the generative lane's possession
    /// floor keeps its own name, `GenClassNotReady`, in its own gate). `Ok` below the fence, for the base class, without a
    /// registry, and for a class the registry gates no claim of (no lifecycle row and no generative row: legacy, never gated).
    pub(super) fn check_class_seated_v1(
        &self,
        class_id: &Hash64,
        executor: &PalwBondKeyV2,
        daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        self.check_class_seated_root_v1(class_id, None, executor, daa)
    }

    /// **Lane MU (ADR-0173, past `palw_audit_1004_v1`): the root's possession floor** — a claim naming a root other than the
    /// class's registered one is refused until `|Ready(root) \ {operator(executor)}| ≥ seat_count`, the rule
    /// [`palw_class_seating_v1`] asks of the founding root, over the `(bond, class, root)` rows. `Ok` below the fence, for the
    /// registered root and without a registry.
    pub(super) fn check_root_possession_v1(
        &self,
        class_id: &Hash64,
        root: &Hash64,
        executor: Option<&PalwBondKeyV2>,
        daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.audit_1004_active_at(daa) || self.state.classes.get(class_id).is_none_or(|class| class.artifact_root == *root) {
            return Ok(());
        }
        let Some(fold) = self.extras.model_registry.as_ref() else { return Ok(()) };
        let ready = class_ready_operators_v1(self.state, self.params, fold, class_id, Some(root), executor, daa)
            .ok_or_else(|| PalwStateV2Error::MissingBond(*executor.expect("only a named executor can be missing")))?;
        let (have, need) = (ready.len().min(u32::MAX as usize) as u32, u32::from(fold.globals.seat_count));
        if have < need {
            return Err(PalwStateV2Error::RootNotSeated { class: *class_id, root: *root, have, need });
        }
        Ok(())
    }

    /// **Lane MU (ADR-0173, RFC-0004 §17.4.1's head switch): may a governed line's head move to `class_id` now?** Past
    /// `palw_audit_1004_v1` the class must be `Active` in the registry's lifecycle AND seated (possession floor with no executor
    /// excluded, and — where `palw_class_seating` is armed — the independence floor too). `true` below the fence and where no
    /// registry runs (the rule is not asked).
    pub(super) fn class_ready_for_head_v1(&self, class_id: &Hash64, daa: u64) -> bool {
        if !self.params.audit_1004_active_at(daa) {
            return true;
        }
        let Some(fold) = self.extras.model_registry.as_ref() else { return true };
        let active = self
            .state
            .model_lifecycle(class_id)
            .is_some_and(|row| matches!(row.state, crate::palw_model_registry_v1::PalwModelLifecycleV1::Active));
        if !active {
            return false;
        }
        let Some(ready) = class_ready_operators_v1(self.state, self.params, fold, class_id, None, None, daa) else { return false };
        if ready.len() < usize::from(fold.globals.seat_count) {
            return false;
        }
        match self.params.class_seating_terms_at(daa) {
            None => true,
            Some(terms) => palw_class_seating_root_v1(
                self.state,
                self.params,
                fold,
                class_id,
                None,
                None,
                daa,
                terms,
                self.extras.activation_pool.is_some(),
            )
            .is_some_and(|seating| seating.verdict().is_ok()),
        }
    }

    /// [`Self::check_class_seated_v1`] over the seats that hold `root` (`None`: the registered root).
    pub(super) fn check_class_seated_root_v1(
        &self,
        class_id: &Hash64,
        root: Option<&Hash64>,
        executor: &PalwBondKeyV2,
        daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        let root = root.filter(|_| self.params.audit_1004_active_at(daa));
        // RFC-0008 v2 (X8R): the composed run's test seam — empty in production (`test_admitted_class_v1`).
        if self.test_admitted_class_v1(class_id) {
            return Ok(());
        }
        let Some(terms) = self.params.class_seating_terms_at(daa) else { return Ok(()) };
        if *class_id == self.params.base_class_id() {
            return Ok(());
        }
        let Some(fold) = self.extras.model_registry.as_ref() else { return Ok(()) };
        if !self.state.model_lifecycles.contains_key(class_id) && !self.state.gen_classes.contains_key(class_id) {
            return Ok(());
        }
        let seating = palw_class_seating_root_v1(
            self.state,
            self.params,
            fold,
            class_id,
            root,
            Some(executor),
            daa,
            terms,
            self.extras.activation_pool.is_some(),
        )
        .ok_or(PalwStateV2Error::MissingBond(*executor))?;
        seating.verdict().map_err(|refusal| match refusal {
            PalwClassNotSeatedV1::Possession { ready, needed } => {
                PalwStateV2Error::ClassNotSeated { class: *class_id, floor: PalwSeatingFloorV1::Possession, have: ready, need: needed }
            }
            PalwClassNotSeatedV1::Independence { independent, needed } => PalwStateV2Error::ClassNotSeated {
                class: *class_id,
                floor: PalwSeatingFloorV1::Independence,
                have: independent,
                need: needed,
            },
        })
    }

    /// **The generative lane's possession floor** — the operators ready besides the executor, for `GenClassNotReady`: the same
    /// read [`palw_class_seating_v1`] takes, asked below the fence too (RFC-0003 §I.4.5's rule, kept).
    pub(super) fn class_possession_v1(
        &self,
        class_id: &Hash64,
        executor: &PalwBondKeyV2,
        now_daa: u64,
        fold: &PalwModelRegistryFoldV1,
    ) -> Option<(u32, u32)> {
        let ready = class_ready_operators_v1(self.state, self.params, fold, class_id, None, Some(executor), now_daa)?;
        Some((ready.len().min(u32::MAX as usize) as u32, u32::from(fold.globals.seat_count)))
    }
}

/// **Which of Proposal A's two floors a `ClassNotSeated` names.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSeatingFloorV1 {
    /// `|Ready \ {operator(executor)}| < seat_count`.
    Possession,
    /// `|(Ready ∩ Base) \ {registrant, executor}| < independent_floor`.
    Independence,
}
