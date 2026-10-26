//! **The kernel route's arm-readiness gate** (lane g14p/kernel-route, 2026-10-22; survey rows U01–U04) — the record gate that replaces
//! the three unconditional refusals of the kernel route's fences:
//!
//! ```text
//! U01  palw_probabilistic_constraints_v1  the kernel route (tags 110–119) and its onboarding fold (104–107)      RFC-0011 §15.7, ADR-0171 §5
//! U03  palw_signed_registration_v1        the signed-registration envelope (tag 108), the onboarding surface    ADR-0175, RFC-0014 §16
//! U04  palw_provider_court_v1             the DA16 provider court, the public-DA surface (claim-scoped only)   RFC-0014 §16.4
//! ```
//!
//! **What "the kernel-route acceptance rule" is.** The old refusal said the binary had none. Since G14 lane D it has: where
//! `palw_probabilistic_constraints_v1` is in force the virtual processor folds kernel route objects into `PalwChainStateV2`
//! (`palw_kernel_route_fold_v1`, and through it the v3/v7/v9/v11/v23/v28 folds and `palw_onboarding_fold_v1`), and a kernel claim's
//! acceptance is the ledger's predicate — committed claim, post-commit seat draw ([`crate::palw_kernel_route_v1`]), receipts reaching
//! the assignment's quorum, no open filing or demand, the window passed — and Final / settlement only then (ADR-0171 §2: probabilistic
//! constraint checks are the normal verifier, the court resolves a dispute exactly). That rule is built; what is NOT in this binary is
//! the evidence ADR-0171 §5 and RFC-0011 §15.7 require before any network may arm it. So the refusal is no longer by name: arming is
//! refused until a reviewed change pins an arm-readiness record (same shape and checks as [`crate::palw_g14_gates_v1`]) naming the
//! fence, the network (genesis), the exact height, the source commit that ran the evidence, one evidence digest per requirement and
//! the review digest. **The pinned list is EMPTY**, so every armed height on every real network is still refused — only the reason
//! changed from "no rule" to "no record". No measured value is invented here.
//!
//! **Ordering.** The onboarding envelope and the provider court both ride the kernel route's fold, so each needs
//! `palw_probabilistic_constraints_v1` in force at or below its own height.
//!
//! **Isolation.** An isolated G14 test genesis ([`Params::has_isolated_g14_genesis_v1`]) skips the record pin, exactly as the G14 gates
//! do; no shipped preset carries one. An isolated native calibration34 genesis keeps its own validator (unchanged).
//!
//! **A-2.** No object kind, wire form, state row or fold rule changes here: only validation. No cryptographic proof of computation is
//! used (PRINCIPLES §8): the route's checks are sampled re-execution, algebraic spot checks and the deterministic court.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_g14_gates_v1::PalwG14ArmingRefusalV1;
use crate::palw_mode_v2::PalwModeV2Error;

/// **The kernel route's gated fences.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwKernelRouteGatedFenceV1 {
    /// U01: `palw_probabilistic_constraints_v1` (RFC-0011 §15.7).
    ProbabilisticConstraintsV1,
    /// U03: `palw_signed_registration_v1` (tag 108).
    SignedRegistrationV1,
    /// U04: `palw_provider_court_v1` (DA16).
    ProviderCourtV1,
}

/// **U01's arm-readiness requirements** (ADR-0171 §5, RFC-0011 §15.7): the fold suite, node E2E, the fence-crossing drill, and the
/// measurements and reviews the ADR names. Most are measurements or reviews; none is a value this binary pins.
pub const PALW_KERNEL_ROUTE_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "V1 build, regressions and boundary decode: tags 104–119 dropped by name below the fence; kernel route fold suite \
     (palw_kernel_route_fold_v1, palw_onboarding_fold_v1) and every fold that reads it (v3/v7/v9/v11/v23/v28)",
    "node E2E g14_kernel_route_e2e / g14_registration_e2e: an honest claim reaches its quorum and Finals with no routine segment \
     replay; a forged claim is filed, localized and convicted from public material by a bond outside the assignment; DA failure, \
     no quorum and reorg cannot bypass Final",
    "adversarial suite (RFC-0011 §15.7 arithmetic / adaptive adversary): wrong output scalar, modulus alias, rounding, wrong expert / \
     TopK tie, memory permutation, forged entry state, uncommitted projection, omitted constraint, duplicate work, reused challenges, \
     beacon withholding, correlated seats",
    "review: whole-claim soundness composition and the beacon / transcript review (ADR-0171 §5, RFC-0011 §15.7), independent of the \
     implementers",
    "measured: shadow comparison against the Panel route on the same claims (agreement, false accept / reject, cost)",
    "measured: real 9B-8k and validated 2M reports — producer execution, witness generation, preprocessing, verifier computation, \
     material transfer and worst-case dispute load reported separately",
    "measured: court resource bounds — a late single fault in the largest permitted model / context localized within bounded total \
     work, bytes and deadline; concurrent disputes",
    "drill: the shipped binary crosses the fence; restart / IBD / reorg replay identical across it; claims admitted before it settle \
     under the old rules",
];

/// **U03's arm-readiness requirements** (ADR-0175 free, immutable registration; RFC-0014 §16). It needs no seat economics of its own;
/// it rides the kernel route's onboarding fold, so its ordering is checked separately.
pub const PALW_SIGNED_REGISTRATION_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "V1 build, regressions and boundary decode: tag 108 dropped by name below the fence; onboarding fold suite (palw_onboarding_v1, \
     palw_onboarding_fold_v1)",
    "node E2E g14_registration_e2e: a signed registration under the network's fired fork digest registers an immutable model; a stale \
     / foreign-ruleset / expired envelope and a re-registration of an existing root are refused; the block stands",
    "drill: the shipped binary crosses the fence; a registration submitted before it is dropped by name, one after it registers",
];

/// **U04's arm-readiness requirements** (RFC-0014 §16.4): the court compels claim-specific units only — never model bytes — so its
/// suite must show the withdrawn artifact subject refused at every height and the per-claim / per-operator scope caps held.
pub const PALW_PROVIDER_COURT_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "V1 build, regressions and boundary decode: provider court suite (palw_provider_court_v1); the withdrawn Artifact subject refused \
     past the fence at every height; every compellable unit is claim-specific (palw_court_demand_allowed_v1)",
    "node E2E: a KernelClaim lease answered on time; a withheld claim unit ends in a provider-liable default and the reporter share; \
     a model-bytes demand is refused",
    "measured: the lease reservation, challenge bond and fee against the real retention price and court load (the constants are \
     interim)",
    "drill: the shipped binary crosses the fence; restart / reorg replay identical; claims accepted before it keep the old rules",
];

impl PalwKernelRouteGatedFenceV1 {
    pub const ALL: [Self; 3] = [Self::ProbabilisticConstraintsV1, Self::SignedRegistrationV1, Self::ProviderCourtV1];

    /// The `Params` field's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::ProbabilisticConstraintsV1 => "palw_probabilistic_constraints_v1",
            Self::SignedRegistrationV1 => "palw_signed_registration_v1",
            Self::ProviderCourtV1 => "palw_provider_court_v1",
        }
    }

    /// The evidence a record must carry, one digest per entry, in this order.
    pub fn requirements(self) -> &'static [&'static str] {
        match self {
            Self::ProbabilisticConstraintsV1 => PALW_KERNEL_ROUTE_ARMING_REQUIREMENTS_V1,
            Self::SignedRegistrationV1 => PALW_SIGNED_REGISTRATION_ARMING_REQUIREMENTS_V1,
            Self::ProviderCourtV1 => PALW_PROVIDER_COURT_ARMING_REQUIREMENTS_V1,
        }
    }

    /// Whether arming needs a measured value (the kernel route and the provider court do; registration is tests and a drill).
    pub fn needs_measured_value(self) -> bool {
        !matches!(self, Self::SignedRegistrationV1)
    }

    /// The fence's raw height on `p` (`Some(never())` reads as `None`).
    pub fn armed(self, p: &Params) -> Option<ForkActivation> {
        let raw = match self {
            Self::ProbabilisticConstraintsV1 => p.palw_probabilistic_constraints_v1,
            Self::SignedRegistrationV1 => p.palw_signed_registration_v1,
            Self::ProviderCourtV1 => p.palw_provider_court_v1,
        };
        raw.filter(|f| *f != ForkActivation::never())
    }

    /// The refusal a real network gets while no record for it is pinned.
    pub fn no_record_refusal(self) -> &'static str {
        match self {
            Self::ProbabilisticConstraintsV1 => {
                "palw_probabilistic_constraints_v1 cannot be armed: the kernel-route acceptance rule is built, but no arm-readiness record \
                 (fold suite, node E2E, adversarial suite, soundness / beacon review, shadow comparison, 9B-8k / 2M and court-bound \
                 measurements, fence-crossing drill) is pinned for this network and height (PALW_KERNEL_ROUTE_ARMING_RECORDS_V1 is empty; \
                 RFC-0011 §15.7, ADR-0171 §5)"
            }
            Self::SignedRegistrationV1 => {
                "palw_signed_registration_v1 cannot be armed: no arm-readiness record (onboarding fold suite, registration node E2E, \
                 fence-crossing drill) is pinned for this network and height (PALW_KERNEL_ROUTE_ARMING_RECORDS_V1 is empty; ADR-0175, \
                 RFC-0014 §16)"
            }
            Self::ProviderCourtV1 => {
                "palw_provider_court_v1 cannot be armed: no arm-readiness record (claim-scoped provider court suite, node E2E, measured \
                 lease terms, fence-crossing drill) is pinned for this network and height (PALW_KERNEL_ROUTE_ARMING_RECORDS_V1 is empty; \
                 RFC-0014 §16.4)"
            }
        }
    }
}

/// The record format's version.
pub const PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1: u16 = 1;

/// **An arm-readiness record** — the same fields and checks as [`crate::palw_g14_gates_v1::PalwG14ArmingRecordV1`], for a kernel route
/// fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwKernelRouteArmingRecordV1 {
    pub version: u16,
    pub fence: PalwKernelRouteGatedFenceV1,
    pub genesis: Hash64,
    pub activation_daa: u64,
    pub source_commit: [u8; 20],
    pub evidence: &'static [[u8; 32]],
    pub review: [u8; 32],
}

/// **The pinned records — EMPTY.** Every kernel route fence armed at a real height on a real network is refused.
pub const PALW_KERNEL_ROUTE_ARMING_RECORDS_V1: &[PalwKernelRouteArmingRecordV1] = &[];

/// A record's own form (not whether it is pinned).
pub fn palw_kernel_route_arming_record_form_v1(record: &PalwKernelRouteArmingRecordV1) -> Result<(), PalwG14ArmingRefusalV1> {
    if record.version != PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1 {
        return Err(PalwG14ArmingRefusalV1::Version);
    }
    if record.source_commit == [0; 20] {
        return Err(PalwG14ArmingRefusalV1::SourceCommit);
    }
    let expected = record.fence.requirements().len();
    if record.evidence.len() != expected {
        return Err(PalwG14ArmingRefusalV1::EvidenceCount { expected, got: record.evidence.len() });
    }
    if record.review == [0; 32] {
        return Err(PalwG14ArmingRefusalV1::Review);
    }
    for (i, digest) in record.evidence.iter().enumerate() {
        if *digest == [0; 32] || *digest == record.review || record.evidence[..i].contains(digest) {
            return Err(PalwG14ArmingRefusalV1::Evidence(i));
        }
    }
    Ok(())
}

/// **Does `records` authorize arming `fence` at `activation_daa` on `genesis`?** The first record naming exactly this decides.
pub fn palw_kernel_route_arming_record_admits_v1(
    fence: PalwKernelRouteGatedFenceV1,
    genesis: Hash64,
    activation_daa: u64,
    records: &[PalwKernelRouteArmingRecordV1],
) -> Result<(), PalwG14ArmingRefusalV1> {
    let mut named = records.iter().filter(|r| r.fence == fence && r.genesis == genesis && r.activation_daa == activation_daa);
    let Some(first) = named.next() else { return Err(PalwG14ArmingRefusalV1::NoRecord) };
    palw_kernel_route_arming_record_form_v1(first)
}

impl Params {
    /// **One kernel route fence's record gate** against the binary's pinned (empty) list.
    pub fn validate_palw_kernel_route_fence_record_v1(&self, fence: PalwKernelRouteGatedFenceV1) -> Result<(), PalwModeV2Error> {
        self.validate_palw_kernel_route_fence_record_in_v1(fence, PALW_KERNEL_ROUTE_ARMING_RECORDS_V1)
    }

    /// The record gate against `records`: `Ok` when the fence is not armed (`None`, `never()`); otherwise the ordering (the onboarding
    /// envelope and the provider court need the kernel route at or below them), then — unless the genesis is an isolated G14 test
    /// genesis — a record naming exactly this (fence, network, height) in well-formed shape.
    pub fn validate_palw_kernel_route_fence_record_in_v1(
        &self,
        fence: PalwKernelRouteGatedFenceV1,
        records: &[PalwKernelRouteArmingRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        let Some(at) = fence.armed(self) else { return Ok(()) };
        if fence != PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1 {
            let route = PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1.armed(self);
            if !route.is_some_and(|r| r.daa_score() <= at.daa_score()) {
                return Err(PalwModeV2Error::Invalid(match fence {
                    PalwKernelRouteGatedFenceV1::SignedRegistrationV1 => {
                        "palw_signed_registration_v1 needs palw_probabilistic_constraints_v1 in force at or below it: tag 108 rides the \
                         kernel route's onboarding fold"
                    }
                    _ => {
                        "palw_provider_court_v1 needs palw_probabilistic_constraints_v1 in force at or below it: the provider court rides \
                         the kernel route's public-DA fold"
                    }
                }));
            }
        }
        if self.has_isolated_g14_genesis_v1() {
            return Ok(());
        }
        match palw_kernel_route_arming_record_admits_v1(fence, self.genesis.hash, at.daa_score(), records) {
            Ok(()) => Ok(()),
            Err(PalwG14ArmingRefusalV1::NoRecord) => Err(PalwModeV2Error::Invalid(fence.no_record_refusal())),
            Err(other) => Err(PalwModeV2Error::Invalid(other.reason())),
        }
    }

    /// **U02: the Panel=1 seat policy's gate** (`palw_kernel_route_panel_one_v1`, dormant; the 1-seat policy was adopted on
    /// 2026-10-22, while activation remains unarmed). `Ok` when not armed. Armed, it needs
    /// `palw_probabilistic_constraints_v1` at or below it (the seat draw it changes is the kernel route's, which that fence's record
    /// gate governs) and the V2 bundle's mirror must agree.
    pub fn validate_palw_kernel_route_panel_one_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_kernel_route_panel_one_v1_fence() else { return Ok(()) };
        let route = PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1.armed(self);
        if !route.is_some_and(|r| r.daa_score() <= at.daa_score()) {
            return Err(PalwModeV2Error::Invalid(
                "palw_kernel_route_panel_one_v1 needs palw_probabilistic_constraints_v1 in force at or below it (the kernel route's seat draw)",
            ));
        }
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            if bundle.state.kernel_route_panel_one_from_daa() != Some(at.daa_score()) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_kernel_route_panel_one_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_g14_gates_v1",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};

    fn arm(p: &mut Params, fence: PalwKernelRouteGatedFenceV1, at: Option<ForkActivation>) {
        match fence {
            PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1 => p.palw_probabilistic_constraints_v1 = at,
            PalwKernelRouteGatedFenceV1::SignedRegistrationV1 => p.palw_signed_registration_v1 = at,
            PalwKernelRouteGatedFenceV1::ProviderCourtV1 => p.palw_provider_court_v1 = at,
        }
    }

    const EVIDENCE_8: [[u8; 32]; 8] = [[1; 32], [2; 32], [3; 32], [4; 32], [5; 32], [6; 32], [7; 32], [8; 32]];

    fn record(fence: PalwKernelRouteGatedFenceV1, genesis: Hash64, at: u64) -> PalwKernelRouteArmingRecordV1 {
        PalwKernelRouteArmingRecordV1 {
            version: PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
            fence,
            genesis,
            activation_daa: at,
            source_commit: [9; 20],
            evidence: &EVIDENCE_8[..fence.requirements().len()],
            review: [0xAA; 32],
        }
    }

    #[test]
    fn kernel_route_fences_are_dormant_and_refused_without_a_record() {
        assert!(PALW_KERNEL_ROUTE_ARMING_RECORDS_V1.is_empty(), "no record is pinned in this binary");
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            for fence in PalwKernelRouteGatedFenceV1::ALL {
                assert_eq!(fence.armed(&p), None);
                p.validate_palw_kernel_route_fence_record_v1(fence).unwrap();
                assert!(fence.requirements().len() >= 3, "{fence:?}: fold suite, node E2E, drill at least");
                assert!(fence.requirements().iter().any(|r| r.starts_with("drill")), "{fence:?}: a fence-crossing drill");
            }
        }
        for fence in PalwKernelRouteGatedFenceV1::ALL {
            let mut p = TESTNET_PARAMS;
            arm(&mut p, PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1, Some(ForkActivation::new(1_000)));
            arm(&mut p, fence, Some(ForkActivation::new(1_000)));
            let Err(PalwModeV2Error::Invalid(why)) = p.validate_palw_kernel_route_fence_record_v1(fence) else { panic!("{fence:?}") };
            assert!(why.contains(fence.name()) && why.contains("no arm-readiness record"), "{fence:?}: {why}");
            arm(&mut p, fence, Some(ForkActivation::never()));
            if fence != PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1 {
                p.validate_palw_kernel_route_fence_record_v1(fence).expect("never() is dormant");
            }
        }
    }

    #[test]
    fn a_pinned_record_for_exactly_this_network_and_height_admits_the_kernel_route() {
        let mut p = TESTNET_PARAMS;
        for fence in PalwKernelRouteGatedFenceV1::ALL {
            arm(&mut p, fence, Some(ForkActivation::new(16_000)));
        }
        let g = p.genesis.hash;
        let records: Vec<_> = PalwKernelRouteGatedFenceV1::ALL.iter().map(|f| record(*f, g, 16_000)).collect();
        for fence in PalwKernelRouteGatedFenceV1::ALL {
            p.validate_palw_kernel_route_fence_record_in_v1(fence, &records).expect("the exact record admits");
            assert!(p.validate_palw_kernel_route_fence_record_in_v1(fence, &[record(fence, g, 16_001)]).is_err(), "other height");
            assert!(p.validate_palw_kernel_route_fence_record_in_v1(fence, &[record(fence, Hash64::default(), 16_000)]).is_err());
            assert!(p.validate_palw_kernel_route_fence_record_v1(fence).is_err(), "the binary pins none");
        }
        // Malformed records.
        let f = PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1;
        let mut r = record(f, g, 16_000);
        r.source_commit = [0; 20];
        assert_eq!(palw_kernel_route_arming_record_form_v1(&r), Err(PalwG14ArmingRefusalV1::SourceCommit));
        let mut r = record(f, g, 16_000);
        r.evidence = &EVIDENCE_8[..2];
        assert!(matches!(palw_kernel_route_arming_record_form_v1(&r), Err(PalwG14ArmingRefusalV1::EvidenceCount { .. })));
        let mut r = record(f, g, 16_000);
        r.review = [0; 32];
        assert_eq!(palw_kernel_route_arming_record_form_v1(&r), Err(PalwG14ArmingRefusalV1::Review));
        let mut r = record(f, g, 16_000);
        r.review = [1; 32];
        assert_eq!(palw_kernel_route_arming_record_form_v1(&r), Err(PalwG14ArmingRefusalV1::Evidence(0)));
        let mut r = record(f, g, 16_000);
        r.version = 2;
        assert_eq!(palw_kernel_route_arming_record_form_v1(&r), Err(PalwG14ArmingRefusalV1::Version));
    }

    #[test]
    fn panel_one_seat_policy_is_dormant_by_default_and_selects_one_seat_when_armed() {
        use crate::palw_kernel_route_v1::*;
        // Default (None): the interim 3-of-3 at every height.
        assert_eq!(palw_kernel_route_seats_and_quorum_v1(None, u64::MAX), (3, 3));
        assert_eq!(palw_kernel_route_seats_and_quorum_v1(None, 0), (PALW_KERNEL_ROUTE_INTERIM_SEATS_V1, PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1));
        // Armed at 100: 3-of-3 below, Panel=1 at and past it.
        assert_eq!(palw_kernel_route_seats_and_quorum_v1(Some(100), 99), (3, 3));
        assert_eq!(palw_kernel_route_seats_and_quorum_v1(Some(100), 100), (1, 1));
        assert_eq!(palw_kernel_route_seats_and_quorum_v1(Some(100), u64::MAX), (1, 1));
        for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
            assert_eq!(p.palw_kernel_route_panel_one_v1, None);
            p.validate_palw_kernel_route_panel_one_v1().unwrap();
        }
        let mut p = TESTNET_PARAMS;
        let id = p.consensus_params_id();
        p.palw_kernel_route_panel_one_v1 = Some(ForkActivation::never());
        p.validate_palw_kernel_route_panel_one_v1().expect("never() is dormant");
        p.palw_kernel_route_panel_one_v1 = Some(ForkActivation::new(16_000));
        assert_ne!(p.consensus_params_id(), id, "an armed seat policy is another network");
        assert!(p.validate_palw_kernel_route_panel_one_v1().is_err(), "needs the kernel route at or below it");
        p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(16_000));
        p.sync_palw_g14_gates_v1();
        p.validate_palw_kernel_route_panel_one_v1().expect("route at it, mirror synced");
    }

    #[test]
    fn registration_and_provider_court_need_the_kernel_route_at_or_below_them() {
        let g = TESTNET_PARAMS.genesis.hash;
        for fence in [PalwKernelRouteGatedFenceV1::SignedRegistrationV1, PalwKernelRouteGatedFenceV1::ProviderCourtV1] {
            let records = [record(fence, g, 16_000), record(PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1, g, 16_001)];
            let mut p = TESTNET_PARAMS;
            arm(&mut p, fence, Some(ForkActivation::new(16_000)));
            let Err(PalwModeV2Error::Invalid(why)) = p.validate_palw_kernel_route_fence_record_in_v1(fence, &records) else { panic!() };
            assert!(why.contains("at or below"), "{why}");
            p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(16_001));
            assert!(p.validate_palw_kernel_route_fence_record_in_v1(fence, &records).is_err(), "route above it");
            p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(16_000));
            p.validate_palw_kernel_route_fence_record_in_v1(fence, &records).expect("route at it, record pinned");
        }
    }
}
