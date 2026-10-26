//! **The G14 collusion drill's private rulesets** (`scripts/drills/g14_collusion_drill.py`,
//! `docs/design/palw/g14-collusion-drill-2026-10-16.md`).
//!
//! The drill shows, on real processes, the G14 claim (`docs/design/palw/g14-completeness-proof-2026-10-16.md` §0): the producer and
//! EVERY Panel seat collude, and ONE public bonded verifier outside the Panel — a bond registered after genesis — reaches an objective
//! conviction from public authenticated material alone. It runs on a salted testnet-12 drill genesis (the shipped ruleset on a genesis
//! nobody else is on, [`super::drill`]) with two departures, both stated here and both inside the ruleset id:
//!
//! 1. **The G14 fences are armed** at DAA 0 (`genesis` mode: "G14 from genesis on future networks", user decision 2026-10-16) or at a
//!    small height `H` (`cross` mode: testnet-12's shipped rules below `H`, the DAA-14,000 candidate set from `H`). The set is
//!    `palw_g14_genesis_test_params_v1`'s: option (b) `palw_g14_route_required_v1`, LG14-A, LG14-B, the staged public court with its
//!    TIR and Float32 extensions, `palw_tir_public_conviction_v1`, N4 and `palw_reward_liability_horizon_v1`. **Arming goes through
//!    the test arming-record path** ([`Params::validate_palw_v2_with_arming_records_v1`]) with one record per armed gated fence naming
//!    THIS drill genesis and height; every record is labelled a drill record and is never pinned in the binary. The shipped validation
//!    is unchanged: [`Params::validate_palw_v2`] still refuses this ruleset (asserted by [`private_g14_drill_v1`] itself).
//! 2. **A Panel this host can run**: the bundle's panel is `seats` seats (3 by default) with R-core+'s fixed quorum of three, and
//!    only genesis cards `1..=seats` keep their genesis capability. Card 0 and cards past `seats` are bonds that judge nothing
//!    (an empty `capable_classes`, the honest resting state of any registration — ADR-0071 Decision 3), so the stake-weighted draw's
//!    eligible-stake floor (ADR-0152 SW-10) is not starved by cards the drill does not run. The draw, the receipts, the court and
//!    every G14 rule are the shipped code; only who may sit differs.
//!
//! **Compiled only into the feature-gated private node** (`private-integration-config`, `kaspad::private_integration`): no preset,
//! `Params` default, Args flag or ordinary kaspad path reaches it. No cryptographic computation proof of any kind is used (PRINCIPLES §8): the records are
//! drill labels, the conviction is the shipped public-evidence court.
use kaspa_consensus_core::{
    config::{
        drill::{PalwDrillSaltV1, palw_chain_params_v1, palw_drill_network_v1},
        params::{ForkActivation, Params},
    },
    palw_g14_gates_v1::{PALW_G14_ARMING_RECORD_VERSION_V1, PalwG14ArmingRecordV1, PalwG14GatedFenceV1},
    palw_kernel_route_arming_v1::{PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1, PalwKernelRouteArmingRecordV1, PalwKernelRouteGatedFenceV1},
    palw_mode_v2::PalwConsensusMode,
    palw_panel_v2::PalwPanelParamsV2,
    palw_state_v2::PalwConsensusObjectV2,
};
use kaspa_hashes::blake2b_512_keyed;

/// **The drill Panel's quorum: R-core+'s colluding quorum** (`PALW_PANEL_COLLUDING_QUORUM_V1` = 3). testnet-12 arms R-core+ at
/// genesis, and `validate_palw_v2` refuses a panel quorum other than that constant (the fold checks a licence's backed subset against
/// it, SR-6) — so the smallest Panel the shipped rules allow is three seats, all of which must sign (3-of-3).
pub const PRIVATE_G14_DRILL_QUORUM_V1: u16 = kaspa_consensus_core::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1 as u16;
/// The Panel sizes a quorum of three admits as a strict majority (2·3 > seats).
pub const PRIVATE_G14_DRILL_MIN_SEATS_V1: u16 = PRIVATE_G14_DRILL_QUORUM_V1;
pub const PRIVATE_G14_DRILL_MAX_SEATS_V1: u16 = 2 * PRIVATE_G14_DRILL_QUORUM_V1 - 1;

/// **testnet-12's shipped flag days, compressed** — the heights the kaspad drill tests cross them at (`palw_drill.rs`:
/// `--palw-drill-fence-at=6 --palw-drill-fence2-at=10 --palw-drill-fence3-at=14 --palw-drill-tir-at=20 --palw-drill-tir2-at=24
/// --palw-drill-int11-at=28`), so the drill chain runs the rules live testnet-12 runs today (every flag day in its past: the
/// aggregate liability and its freeze, the capacity ramp, TIR, the int-11 list) instead of testnet-12 as launched. Each move is the
/// shipped drill function and validates the result by itself.
pub const PRIVATE_G14_DRILL_FLAG_DAYS_V1: [u64; 6] = [6, 10, 14, 20, 24, 28];

/// The domain every drill record digest is derived under — a record built here is recognisable as a drill label in its bytes.
/// A blake2b key (at most 64 bytes): every record digest of a drill is keyed by it, never a release record's.
pub const PRIVATE_G14_DRILL_RECORD_DOMAIN_V1: &[u8] = b"MISAKA/G14-COLLUSION-DRILL/TEST-ARMING-RECORD/V1";

/// **Where the G14 fences are armed.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateG14DrillModeV1 {
    /// Every G14 fence at DAA 0 — "G14 from genesis on future networks".
    Genesis,
    /// testnet-12's shipped rules below `at_daa`, the DAA-14,000 candidate set from it.
    Cross { at_daa: u64 },
}

impl PrivateG14DrillModeV1 {
    pub fn at_daa(self) -> u64 {
        match self {
            Self::Genesis => 0,
            Self::Cross { at_daa } => at_daa,
        }
    }

    /// The mode a height names (0: genesis).
    pub fn from_daa(at_daa: u64) -> Self {
        if at_daa == 0 { Self::Genesis } else { Self::Cross { at_daa } }
    }
}

/// **Which G14 set is armed at the mode's height.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateG14DrillSetV1 {
    /// The DAA-14,000 candidate set (`palw_g14_genesis_test_params_v1`'s fences).
    Full,
    /// Option (b) alone (`palw_g14_route_required_v1`, no route armed): every legacy-route family is refused past the height, so the
    /// heartbeat lane must carry block production (release plan B.2's "注意").
    RouteRequiredOnly,
    /// **The kernel slice** (coordinator plan 2026-10-26, lane TNODE): `PALW_T12_KERNEL_SLICE_FENCES_V1` at the height — the int-13
    /// entries, the kernel route with Panel=1, its claim deadline, settlement once, Gate C and route-required — validated with drill
    /// records for both the G14 and the kernel-route gated fences. Every other refusal (values, prerequisites) is the shipped one.
    KernelSlice,
}

/// The drill ruleset and the drill records it validates with.
pub struct PrivateG14DrillV1 {
    pub params: Params,
    pub records: Vec<PalwG14ArmingRecordV1>,
    /// The kernel-route drill records (empty unless the set arms a kernel-route gated fence).
    pub kernel_records: Vec<PalwKernelRouteArmingRecordV1>,
    pub mode: PrivateG14DrillModeV1,
    pub set: PrivateG14DrillSetV1,
    pub seats: u16,
    pub quorum: u16,
}

fn digest32(label: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&blake2b_512_keyed(PRIVATE_G14_DRILL_RECORD_DOMAIN_V1, label).as_bytes()[..32]);
    out
}

/// **One drill arm-readiness record** for `fence` at `at_daa` on `genesis`: one distinct labelled digest per requirement, a labelled
/// review and source commit. It authorizes exactly this (fence, genesis, height) through the record path and nothing else; it is
/// never pinned in [`kaspa_consensus_core::palw_g14_gates_v1::PALW_G14_ARMING_RECORDS_V1`].
pub fn private_g14_drill_record_v1(fence: PalwG14GatedFenceV1, genesis: kaspa_consensus_core::Hash64, at_daa: u64) -> PalwG14ArmingRecordV1 {
    let subject = |what: &str, i: usize| {
        let mut label = Vec::new();
        label.extend_from_slice(what.as_bytes());
        label.extend_from_slice(fence.name().as_bytes());
        label.extend_from_slice(genesis.as_bytes().as_slice());
        label.extend_from_slice(&at_daa.to_le_bytes());
        label.extend_from_slice(&(i as u64).to_le_bytes());
        label
    };
    let evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| digest32(&subject("evidence", i))).collect();
    let mut source_commit = [0u8; 20];
    source_commit.copy_from_slice(&digest32(&subject("source", 0))[..20]);
    PalwG14ArmingRecordV1 {
        version: PALW_G14_ARMING_RECORD_VERSION_V1,
        fence,
        genesis,
        activation_daa: at_daa,
        source_commit,
        // One bounded allocation per drill configuration (as `palw_isolated_g14_genesis_v1` leaks its payload): the record type
        // carries a static evidence list because the binary's pinned list is static.
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: digest32(&subject("review: none — a drill record authorizes a drill chain only", 0)),
    }
}

/// **One kernel-route drill record**: the same labelled construction as [`private_g14_drill_record_v1`], under the kernel-route
/// fence's name and requirement count. Never pinned in `PALW_KERNEL_ROUTE_ARMING_RECORDS_V1`.
pub fn private_kernel_route_drill_record_v1(
    fence: PalwKernelRouteGatedFenceV1,
    genesis: kaspa_consensus_core::Hash64,
    at_daa: u64,
) -> PalwKernelRouteArmingRecordV1 {
    let subject = |what: &str, i: usize| {
        let mut label = Vec::new();
        label.extend_from_slice(b"kernel-route:");
        label.extend_from_slice(what.as_bytes());
        label.extend_from_slice(fence.name().as_bytes());
        label.extend_from_slice(genesis.as_bytes().as_slice());
        label.extend_from_slice(&at_daa.to_le_bytes());
        label.extend_from_slice(&(i as u64).to_le_bytes());
        label
    };
    let evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| digest32(&subject("evidence", i))).collect();
    let mut source_commit = [0u8; 20];
    source_commit.copy_from_slice(&digest32(&subject("source", 0))[..20]);
    PalwKernelRouteArmingRecordV1 {
        version: PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
        fence,
        genesis,
        activation_daa: at_daa,
        source_commit,
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: digest32(&subject("review: none — a drill record authorizes a drill chain only", 0)),
    }
}

/// The kernel-route drill records for every kernel-route gated fence `params` arms.
pub fn private_kernel_route_drill_records_v1(params: &Params) -> Vec<PalwKernelRouteArmingRecordV1> {
    PalwKernelRouteGatedFenceV1::ALL
        .into_iter()
        .filter_map(|fence| fence.armed(params).map(|at| private_kernel_route_drill_record_v1(fence, params.genesis.hash, at.daa_score())))
        .collect()
}

/// The drill records for every gated fence `params` arms (one per fence, at its own height, on its own genesis).
pub fn private_g14_drill_records_v1(params: &Params) -> Vec<PalwG14ArmingRecordV1> {
    PalwG14GatedFenceV1::ALL
        .into_iter()
        .filter_map(|fence| fence.armed(params).map(|at| private_g14_drill_record_v1(fence, params.genesis.hash, at.daa_score())))
        .collect()
}

/// **Build the drill ruleset**: the salted testnet-12 drill params, the drill Panel, the G14 set armed at the mode's height, validated
/// through the record path — and refused by the shipped validation, which this asserts rather than assumes.
pub fn private_g14_drill_v1(
    salt: &PalwDrillSaltV1,
    mode: PrivateG14DrillModeV1,
    set: PrivateG14DrillSetV1,
    seats: u16,
) -> Result<PrivateG14DrillV1, String> {
    if !(PRIVATE_G14_DRILL_MIN_SEATS_V1..=PRIVATE_G14_DRILL_MAX_SEATS_V1).contains(&seats) {
        return Err(format!(
            "the G14 drill Panel has {PRIVATE_G14_DRILL_MIN_SEATS_V1}..={PRIVATE_G14_DRILL_MAX_SEATS_V1} seats: R-core+ fixes the quorum at \
             {PRIVATE_G14_DRILL_QUORUM_V1} and a quorum is a strict majority"
        ));
    }
    if mode.at_daa() == u64::MAX {
        return Err("the G14 drill arms its set at a finite height".into());
    }
    let mut params = palw_chain_params_v1(palw_drill_network_v1(), Some(salt))?;
    {
        use kaspa_consensus_core::config::drill as d;
        let [f1, f2, f3, tir, tir2, int11] = PRIVATE_G14_DRILL_FLAG_DAYS_V1;
        d::palw_drill_post_launch_fences_at_v1(&mut params, f1)?;
        d::palw_drill_post_launch_fences_v2_at_v1(&mut params, f2)?;
        d::palw_drill_post_launch_fences_v3_at_v1(&mut params, f3)?;
        d::palw_drill_tir_fence_at_v1(&mut params, tir)?;
        d::palw_drill_tir_fence2_at_v1(&mut params, tir2)?;
        d::palw_drill_int11_at_v1(&mut params, int11)?;
    }
    if let PrivateG14DrillModeV1::Cross { at_daa } = mode
        && at_daa <= PRIVATE_G14_DRILL_FLAG_DAYS_V1[5]
    {
        return Err(format!(
            "the crossing height must lie past the compressed testnet-12 flag days (DAA {}), so the chain runs today's rules below it",
            PRIVATE_G14_DRILL_FLAG_DAYS_V1[5]
        ));
    }
    // R-core+'s colluding quorum (validate_palw_v2 refuses any other); a strict majority of 3..=5 seats.
    let quorum = PRIVATE_G14_DRILL_QUORUM_V1;
    {
        let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
            return Err("the G14 drill needs testnet-12's ConsensusV2 bundle".into());
        };
        bundle.panel =
            PalwPanelParamsV2::new(seats, quorum, bundle.panel.anchor_delay()).map_err(|e| format!("the drill Panel: {e}"))?;
        let mut card = 0u16;
        for object in bundle.genesis_objects.iter_mut() {
            if let PalwConsensusObjectV2::BondRegistered { capable_classes, .. } = object {
                if card == 0 || card > seats {
                    capable_classes.clear();
                }
                card += 1;
            }
        }
        if card <= seats {
            return Err(format!("the drill genesis registers {card} cards, fewer than a producer and {seats} seats"));
        }
    }
    let at = Some(ForkActivation::new(mode.at_daa()));
    match set {
        PrivateG14DrillSetV1::Full => {
            params.palw_legacy_public_filer_v1 = at;
            params.sync_palw_legacy_public_filer_v1();
            params.palw_legacy_held_da_v2 = at;
            params.palw_staged_public_court_v1 = at;
            params.palw_staged_tir_route_v1 = at;
            params.palw_staged_float_constraint_v1 = at;
            params.palw_tir_public_conviction_v1 = at;
            params.palw_public_evidence_reward_v1 = at;
            params.sync_palw_2026_10_12_ruling_fences_v1();
            params.palw_g14_route_required_v1 = at;
            params.palw_reward_liability_horizon_v1 = at;
        }
        PrivateG14DrillSetV1::RouteRequiredOnly => {
            params.palw_g14_route_required_v1 = at;
        }
        PrivateG14DrillSetV1::KernelSlice => {
            use kaspa_consensus_core::config::params::{PALW_T12_KERNEL_SLICE_FENCES_V1, palw_t12_arm_fence_list_at_v1};
            palw_t12_arm_fence_list_at_v1(&mut params, PALW_T12_KERNEL_SLICE_FENCES_V1, Some(mode.at_daa()));
        }
    }
    params.sync_palw_g14_gates_v1();
    if params.has_isolated_g14_genesis_v1() {
        return Err("the drill exercises the record path: its genesis must not be an isolated G14 test genesis".into());
    }
    let records = private_g14_drill_records_v1(&params);
    let kernel_records = private_kernel_route_drill_records_v1(&params);
    params.validate_palw_v1().map_err(|e| format!("the drill ruleset fails validate_palw_v1: {e}"))?;
    params
        .validate_palw_v2_with_g14_and_kernel_route_records_v1(&records, &kernel_records)
        .map_err(|e| format!("the drill ruleset fails the whole V2 validation with its drill records: {e:?}"))?;
    if params.validate_palw_v2().is_ok() {
        return Err("the shipped validation must still refuse the drill ruleset (no record is pinned in the binary)".into());
    }
    Ok(PrivateG14DrillV1 { params, records, kernel_records, mode, set, seats, quorum })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salt() -> PalwDrillSaltV1 {
        PalwDrillSaltV1::from_bytes([0x14; 32]).unwrap()
    }

    #[test]
    fn g14_drill_genesis_and_cross_rulesets_validate_through_the_record_path_only() {
        let base = palw_chain_params_v1(palw_drill_network_v1(), Some(&salt())).unwrap();
        for (mode, set) in [
            (PrivateG14DrillModeV1::Genesis, PrivateG14DrillSetV1::Full),
            (PrivateG14DrillModeV1::Cross { at_daa: 40 }, PrivateG14DrillSetV1::Full),
            (PrivateG14DrillModeV1::Cross { at_daa: 40 }, PrivateG14DrillSetV1::RouteRequiredOnly),
        ] {
            let drill = private_g14_drill_v1(&salt(), mode, set, 3).unwrap_or_else(|e| panic!("{mode:?} {set:?}: {e}"));
            let p = &drill.params;
            assert_eq!(p.genesis.hash, base.genesis.hash, "the salted drill genesis is unchanged");
            assert_ne!(p.consensus_params_id(), base.consensus_params_id(), "a drill ruleset is its own ruleset id");
            assert!(p.validate_palw_v2().is_err(), "the shipped validation refuses it");
            for record in &drill.records {
                let mut fewer = drill.records.clone();
                fewer.retain(|r| r.fence != record.fence);
                assert!(p.validate_palw_v2_with_arming_records_v1(&fewer).is_err(), "{:?}: every drill record is load-bearing", record.fence);
            }
            let route = p.palw_g14_route_required_v1_mirror().expect("option (b) is armed in every drill set");
            assert_eq!(route.from_daa, mode.at_daa());
            eprintln!(
                "[g14-drill] {mode:?} {set:?}: params {} records {:?} route {route:?} tir {:?} panel {}/{}",
                p.consensus_params_id(),
                drill.records.iter().map(|r| (r.fence.name(), r.activation_daa)).collect::<Vec<_>>(),
                p.palw_tir_public_conviction_v1_fence(),
                drill.seats,
                drill.quorum,
            );
        }
        assert!(private_g14_drill_v1(&salt(), PrivateG14DrillModeV1::Genesis, PrivateG14DrillSetV1::Full, 2).is_err());
        assert!(private_g14_drill_v1(&salt(), PrivateG14DrillModeV1::Genesis, PrivateG14DrillSetV1::Full, 6).is_err());
    }

    #[test]
    fn g14_drill_only_cards_one_to_seats_judge() {
        let drill = private_g14_drill_v1(&salt(), PrivateG14DrillModeV1::Genesis, PrivateG14DrillSetV1::Full, 3).unwrap();
        let PalwConsensusMode::ConsensusV2(bundle) = &drill.params.palw_consensus_mode else { unreachable!() };
        let capable: Vec<bool> = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                PalwConsensusObjectV2::BondRegistered { capable_classes, .. } => Some(!capable_classes.is_empty()),
                _ => None,
            })
            .collect();
        assert_eq!(&capable[..4], &[false, true, true, true]);
        assert!(capable[4..].iter().all(|c| !c), "cards past the Panel judge nothing");
        assert_eq!((bundle.panel.seat_count(), bundle.panel.quorum()), (3, 3));
    }

    /// Lane TNODE: the kernel slice either validates through the drill record path, or is refused for a reason that is NOT a missing
    /// drill record (the record path reaches the kernel-route gate). The outcome is printed for the evidence, never hidden.
    #[test]
    fn kernel_slice_drill_reaches_the_kernel_route_record_path() {
        match private_g14_drill_v1(&salt(), PrivateG14DrillModeV1::Cross { at_daa: 60 }, PrivateG14DrillSetV1::KernelSlice, 3) {
            Ok(drill) => {
                println!(
                    "[tnode] kernel slice validates: g14 records {:?} kernel records {:?}",
                    drill.records.iter().map(|r| r.fence.name()).collect::<Vec<_>>(),
                    drill.kernel_records.iter().map(|r| r.fence.name()).collect::<Vec<_>>()
                );
                assert!(drill.kernel_records.iter().any(|r| r.fence == PalwKernelRouteGatedFenceV1::ProbabilisticConstraintsV1));
            }
            Err(e) => {
                println!("[tnode] kernel slice refused: {e}");
                assert!(!e.contains("PALW_KERNEL_ROUTE_ARMING_RECORDS_V1 is empty"), "the drill record path must reach the gate: {e}");
            }
        }
    }
}
