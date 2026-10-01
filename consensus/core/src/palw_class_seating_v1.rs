//! **One reading of "this class has seats" for every class kind** — the possession floor of RFC-0002
//! Part II §II.7.5 Proposal A (`palw_class_seating`, lane F, approved 2026-10-01), extracted from RFC-0003's
//! tensor-claim gate (`GenClassNotReady`, §I.4.5) so an IR attempt class, a generative class and a composite
//! class all ask the SAME function and the doors cannot disagree. A child module of `palw_state_v2`, as the
//! generative fold is: it reads the fold's read view ([`PalwFoldReadV1`]) and its one readiness predicate
//! (`model_registry_seat_is_ready` — active, above the floor, a fresh readiness V2 row, free collateral for
//! the readiness multiple) and restates nothing.
//!
//! # The contract lane F builds on
//!
//! [`PalwFoldReadV1::class_seating_v1`] returns, for a class and an optional EXECUTOR bond at a DAA, the
//! operators that hold the class with a fresh possession proof — **each operator once, however many bonds it
//! holds, the executor's bond and its whole operator excluded** (with no executor, nothing is excluded: the
//! lifecycle step and the registry read have none) — with the ready bonds of each, in key order, and the
//! panel's size (`seat_count`, the registry globals'). That is condition 1 of Proposal A, the *possession
//! floor*: [`PalwClassSeatingV1::possession_floor_met`]. Condition 2, the *independence floor* —
//! `|(Ready ∩ Base) \ {operator(registrant), operator(e)}| ≥ ⌊seat_count / 2⌋ + 1` — is a read over the same
//! map ([`PalwClassSeatingV1::ready_operators_in`]: the caller supplies `Base` as a predicate over bond keys
//! and the operators to leave out), so the lane that owns `Base` (the admission jury's and the outsider
//! seat's population) adds it without a second walk of the readiness rows. Nothing here reads a fence: the
//! generative lane asks it under its own fences today, and `palw_class_seating` is the later fence under which
//! every door asks it.
//!
//! The cost is one pass over the bond table per ask (the readiness predicate is evaluated once per bond),
//! which is what the tensor claim's gate cost before the extraction; a lane that asks it for every claim of a
//! block may want a per-block cache keyed by `(class, executor operator)` — the answer depends on nothing
//! else the block's objects can change before the claim folds.

use super::*;

/// What a class's seating is at one DAA, for one executor — see the module doc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PalwClassSeatingV1 {
    /// The operators that hold the class with a fresh readiness V2 possession proof, by the registry's own
    /// five-clause predicate: operator id → the operator's READY bonds, in key order. The executor's bond
    /// and every bond of the executor's operator are absent.
    pub ready: BTreeMap<Hash64, Vec<PalwBondKeyV2>>,
    /// A panel's worth of seats: the registry globals' `seat_count`.
    pub seat_count: u32,
}

impl PalwClassSeatingV1 {
    /// How many distinct operators are ready (the executor's excluded).
    pub(super) fn ready_operators(&self) -> u32 {
        self.ready.len() as u32
    }

    /// **The possession floor** (Proposal A condition 1): a panel can be drawn without the executor.
    pub(super) fn possession_floor_met(&self) -> bool {
        self.ready_operators() >= self.seat_count
    }

    /// **The ready operators that have a bond in a population and are not among `except`** — the read the
    /// independence floor (Proposal A condition 2) takes over `Base`: `in_population` answers "is this bond
    /// in the base population" and `except` names the operators to leave out (the class's registrant's; the
    /// executor's is already absent from [`Self::ready`]). An operator counts once.
    #[allow(dead_code)] // lane F's independence floor (`palw_class_seating`) is its first caller
    pub(super) fn ready_operators_in(&self, in_population: impl Fn(&PalwBondKeyV2) -> bool, except: &[Hash64]) -> u32 {
        self.ready.iter().filter(|(operator, bonds)| !except.contains(operator) && bonds.iter().any(&in_population)).count() as u32
    }
}

impl PalwFoldReadV1<'_> {
    /// **A class's seating, for an executor or for none** (see the module doc). With `Some(executor)` the
    /// executor's bond and operator are excluded — a claim's door; with `None` nothing is excluded — the
    /// lifecycle step and the registry read, which have no executor. `None` is returned only when
    /// `Some(executor)` is not in the state (the caller refuses the claim for it before it asks); with no
    /// executor it always returns `Some`.
    pub(super) fn class_seating_v1(
        &self,
        class_id: &Hash64,
        executor: Option<&PalwBondKeyV2>,
        now_daa: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
    ) -> Option<PalwClassSeatingV1> {
        let executor_operator = match executor {
            Some(key) => Some(self.state.bonds.get(key)?.operator_id),
            None => None,
        };
        let mut ready: BTreeMap<Hash64, Vec<PalwBondKeyV2>> = BTreeMap::new();
        for (key, bond) in self.state.bonds.iter() {
            if executor == Some(key) || executor_operator == Some(bond.operator_id) {
                continue;
            }
            if self.model_registry_seat_is_ready(key, bond, class_id, now_daa, fold) {
                ready.entry(bond.operator_id).or_default().push(*key);
            }
        }
        Some(PalwClassSeatingV1 { ready, seat_count: fold.globals.seat_count as u32 })
    }
}
