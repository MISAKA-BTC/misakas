//! **`palw_class_seating` — the fence of RFC-0002 Part II §II.7.5 Proposal A (approved 2026-10-01): one seating rule for every
//! class kind.** The predicate is `palw_class_seating_v1.rs` (a child module of the fold); this file is the fence's
//! side of it: the terms, the value `Params::palw_class_seating` carries, the entry a flag day or a drill arms it with, the
//! bundle's mirror, and the refusals `validate_palw_v2` makes.
//!
//! **Dormant.** `None` on every preset (testnet-12 included) and in no flag-day list: it is hashed Some-only into both
//! fingerprints with its value, only its activation is visited by `for_each_fence`, and the whole option collapses from
//! `Some(never())` — `palw_gen_v1`'s shape — so a dormant network fingerprints as a build without the field. Arming is the
//! RFC-0003/0004 flag day's entry, which names the height and the floor; until then a drill arms it
//! (`--palw-drill-class-seating-at`).
//!
//! **The parameter.** `independent_floor` is the fence's own value ([`PalwClassSeatingFenceV1`]), so a later flag-day entry can
//! raise it (SEAT-10) and it never moves under a running chain. The default is the jury's strict majority of a panel
//! ([`PALW_CLASS_SEATING_T12_INDEPENDENT_FLOOR_V1`], 3 on testnet-12's panel of five).

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **testnet-12's `independent_floor`: `⌊seat_count / 2⌋ + 1` = 3 of a panel of five** — the number of operators the network
/// already accepts as evidence of independence when it admits a bought class (the admission jury's strict majority,
/// `palw_admission_jury_quorum_v1`), made a continuing condition.
pub const PALW_CLASS_SEATING_T12_INDEPENDENT_FLOOR_V1: u16 = 3;

/// **The fence's terms in force at a DAA** (`Params::palw_class_seating_terms_at`): what the predicate reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassSeatingTermsV1 {
    /// Operators that are neither the claim's executor nor the class's registrant, hold the class ready, and are in the
    /// network's base population (the one the admission jury and the outsider seat draw from) — at least this many.
    pub independent_floor: u16,
}

/// **A later flag-day entry's raise of the floor** (SEAT-10): from `activation` on the floor is `independent_floor` instead of
/// the fence's own. The fence's single value carries it, so the raise is a height of the fork id like any other and a floor never
/// moves under a running chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassSeatingRaiseV1 {
    pub activation: ForkActivation,
    pub independent_floor: u16,
}

/// **`Params::palw_class_seating`'s value**: the fence and what it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassSeatingFenceV1 {
    pub activation: ForkActivation,
    pub independent_floor: u16,
    /// A later raise of the floor, if a flag-day entry made one (`None` on every shipped preset and in every drill).
    pub raise: Option<PalwClassSeatingRaiseV1>,
}

/// **The fence as the bundle mirrors it** (`PalwStateParamsV2::class_seating`): the height, the floor and the raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwClassSeatingMirrorV1 {
    pub from_daa: u64,
    pub independent_floor: u16,
    pub raise: Option<(u64, u16)>,
}

impl PalwClassSeatingMirrorV1 {
    /// The terms in force at `daa`: `None` below the fence; the raised floor from the raise's height on.
    pub fn terms_at(&self, daa: u64) -> Option<PalwClassSeatingTermsV1> {
        if daa < self.from_daa {
            return None;
        }
        let independent_floor = match self.raise {
            Some((at, floor)) if daa >= at => floor,
            _ => self.independent_floor,
        };
        Some(PalwClassSeatingTermsV1 { independent_floor })
    }
}

impl PalwClassSeatingFenceV1 {
    /// testnet-12's value at a height: the jury's strict majority of its panel of five.
    pub fn testnet12_v1(activation: ForkActivation) -> Self {
        Self { activation, independent_floor: PALW_CLASS_SEATING_T12_INDEPENDENT_FLOOR_V1, raise: None }
    }

    /// A drill's value at a height — testnet-12's: a drill drills what ships.
    pub fn drill_v1(activation: ForkActivation) -> Self {
        Self::testnet12_v1(activation)
    }

    /// The same fence with its floor raised from a later height (SEAT-10: a later flag-day entry raises it).
    pub fn with_raise(self, activation: ForkActivation, independent_floor: u16) -> Self {
        Self { raise: Some(PalwClassSeatingRaiseV1 { activation, independent_floor }), ..self }
    }

    /// The bundle's mirror of this fence.
    pub(crate) fn mirror(&self) -> PalwClassSeatingMirrorV1 {
        PalwClassSeatingMirrorV1 {
            from_daa: self.activation.daa_score(),
            independent_floor: self.independent_floor,
            raise: self.raise.filter(|r| r.activation != ForkActivation::never()).map(|r| (r.activation.daa_score(), r.independent_floor)),
        }
    }

    /// What the fence adds to a fingerprint beside its height. `consensus_params_id` and `consensus_schedule_id` write these
    /// same bytes.
    pub(crate) fn write_value_into(&self, h: &mut kaspa_hashes::ConsensusParamsId) {
        h.write(self.independent_floor.to_le_bytes());
        // The raise is Some-only inside the Some-only fence: a fence without one writes nothing more.
        if let Some(raise) = self.raise {
            h.write(b"raise");
            h.write(raise.activation.daa_score().to_le_bytes());
            h.write(raise.independent_floor.to_le_bytes());
        }
    }
}

/// **The entry a flag day (or a drill) arms the seating rule with**, through its own `set`, which writes the bundle's mirror.
/// In NO testnet-12 flag-day list today: the fence is dormant on every network.
pub const PALW_CLASS_SEATING_ENTRY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_class_seating",
    set: |params, at| {
        params.palw_class_seating = at.map(PalwClassSeatingFenceV1::testnet12_v1);
        params.sync_palw_class_seating();
    },
};

/// The drill's one-entry list (`--palw-drill-class-seating-at`, [`crate::config::drill::palw_drill_class_seating_at_v1`]).
pub const PALW_DRILL_CLASS_SEATING_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_CLASS_SEATING_ENTRY_V1];

impl Params {
    /// `palw_class_seating`, resolved: `Some` only on a `ConsensusV2` network that armed it with a real height.
    pub fn palw_class_seating_fence(&self) -> Option<PalwClassSeatingFenceV1> {
        match (&self.palw_consensus_mode, self.palw_class_seating) {
            (PalwConsensusMode::ConsensusV2(_), Some(fence)) if fence.activation != ForkActivation::never() => Some(fence),
            _ => None,
        }
    }

    /// **The seating rule's terms in force at `daa_score`** — `None` below the fence, on every shipped preset.
    pub fn palw_class_seating_terms_at(&self, daa_score: u64) -> Option<PalwClassSeatingTermsV1> {
        self.palw_class_seating_fence().and_then(|fence| fence.mirror().terms_at(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params (`PalwStateParamsV2::class_seating`), which the fold, the
    /// lifecycle step and the registry read consult. Written here and nowhere else; `None` where the fence is not armed (or is
    /// `never()`). Call it wherever the fence is set on an assembled ruleset; [`Self::validate_palw_class_seating`] refuses a
    /// ruleset whose copy disagrees.
    pub fn sync_palw_class_seating(&mut self) {
        let armed = self.palw_class_seating.filter(|f| f.activation != ForkActivation::never());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_class_seating(armed.map(|f| f.mirror()));
        }
    }

    /// **The seating fence's own refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * a V2 bundle whose mirror is not the fence's;
    /// * arming on a ruleset that is not `ConsensusV2`;
    /// * an `independent_floor` of 0 (a rule that asks nothing) or above the panel's `seat_count` (a floor no panel could meet);
    /// * arming without `palw_gen_v1` in force at or below it — the generative lane's possession floor *is* condition 1, and a
    ///   network that has not armed it has no second door to keep in step;
    /// * arming without `palw_audit_2026_09_23` in force at or below it — the readiness predicate the rule counts with reads
    ///   collateral as the net figure only past it, and the claim gate asks the rule in the same block.
    ///
    /// A `Some(never())` value is dormant and passes (it collapses out of the identity).
    pub fn validate_palw_class_seating(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.class_seating(),
            _ => None,
        };
        let armed = self.palw_class_seating.filter(|f| f.activation != ForkActivation::never());
        if mirror != armed.map(|f| f.mirror()) {
            return Err(PalwModeV2Error::Invalid(
                "palw_class_seating disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_class_seating after \
                 the bundle is assembled",
            ));
        }
        let Some(fence) = armed else { return Ok(()) };
        let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else {
            return Err(PalwModeV2Error::Invalid("palw_class_seating is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        };
        let seats = u64::from(bundle.panel.seat_count());
        for floor in std::iter::once(fence.independent_floor).chain(fence.raise.map(|r| r.independent_floor)) {
            if floor == 0 {
                return Err(PalwModeV2Error::Invalid("palw_class_seating's independent_floor is 0: a floor that asks nothing"));
            }
            if u64::from(floor) > seats {
                return Err(PalwModeV2Error::Invalid(
                    "palw_class_seating's independent_floor is above the panel's seat_count: no panel could meet it",
                ));
            }
        }
        if let Some(raise) = fence.raise.filter(|r| r.activation != ForkActivation::never()) {
            if raise.activation.daa_score() <= fence.activation.daa_score() {
                return Err(PalwModeV2Error::Invalid("palw_class_seating's raise is not above the fence's own height"));
            }
            if raise.independent_floor < fence.independent_floor {
                return Err(PalwModeV2Error::Invalid("palw_class_seating's raise lowers the floor: a floor never falls"));
            }
        }
        let at = fence.activation.daa_score();
        let gen_ok = self.palw_gen_v1.is_some_and(|g| g.activation != ForkActivation::never() && g.activation.daa_score() <= at);
        if !gen_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_class_seating needs palw_gen_v1 in force at or below it: the generative lane's possession floor is its \
                 first condition, asked through the one function",
            ));
        }
        let pool_ok = self.palw_activation_pool.is_some_and(|p| p.activation != ForkActivation::never() && p.activation.daa_score() <= at);
        if !pool_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_class_seating needs palw_activation_pool in force at or below it: the base population the independence floor \
                 reads is the pool's (the panel's collateral floor), the one the admission jury draws from",
            ));
        }
        let audit_ok =
            self.palw_audit_2026_09_23.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !audit_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_class_seating needs palw_audit_2026_09_23 in force at or below it: the readiness predicate it counts with \
                 reads free collateral as the net figure only past it",
            ));
        }
        Ok(())
    }
}
