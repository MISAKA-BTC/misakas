//! `PalwConsensusMode` — one switch, or none (ADR-0042 Decision 1, PR-10), and the ruleset
//! fingerprint that makes RC == mainnet a hash a node checks (Decision 11).
//!
//! The five `Option` fences were a machine that invited half-activation: individually
//! flippable, discovered mid-audit to interlock in ways their docs had to warn about. On the V2
//! lineage a network is in exactly ONE mode, and `ConsensusV2` carries **all** of the ruleset or
//! none of it:
//!
//! * every sub-parameter block is constructor-validated (no `Default`s anywhere in the bundle);
//! * [`PalwConsensusParamsV2::validate`] holds the Decision 1 startup invariants — the checks a
//!   node runs before it dials a peer, failing which it does not boot;
//! * [`Params::validate_palw_v2`][crate::config::params::Params] (the config gate) additionally
//!   refuses a MIXED params set: a `ConsensusV2` network may not set any V1 fence and may not
//!   activate any V1 PALW PoW — there is no path that switches on half of two lineages.
//!
//! The fingerprint ([`palw_ruleset_id_v2`]) is the canonical hash of the whole bundle. Network
//! identity is deliberately NOT inside it — the challenge's `network_domain` carries that
//! (Decision 3a), so RC and mainnet can share one ruleset id while a testnet block still cannot
//! replay on mainnet. The P2P handshake exchanges it and drops a mismatched peer early; the
//! mainnet binary reads the RC's canonical ruleset bytes rather than a human re-typing numbers.
//!
//! What this module does NOT do: mint the RC genesis. The class/court catalog ROOTS are
//! committed here; the genesis that registers BASE-0 must hash to them, and the boot path that
//! loads it verifies the preimages and runs `verify_catalog_coverage_v1` against this build's
//! own catalog — those land with the RC genesis itself, which is a network artifact, not a
//! library one.

use crate::Hash64;
use crate::palw_admission_v2::{PalwAdmissionParamsV2, PalwAdmissionV2Error};
use crate::palw_freeprompt_v3::{PalwFpV3Error, PalwFreePromptParamsV3};
use crate::palw_panel_v2::PalwPanelParamsV2;
use crate::palw_reward_v2::PalwRewardParamsV2;
use crate::palw_state_v2::{PalwStateParamsV2, PalwStateV2Error};
use blake2b_simd::Params as Blake2bParams;

pub const PALW_RULESET_ID_V2_DOMAIN: &[u8] = b"misaka-palw/ruleset-id-v2/v1";

pub const PALW_MODE_V2_ALL_DOMAINS: &[&[u8]] = &[PALW_RULESET_ID_V2_DOMAIN];

/// Bond-side network constants (ADR-0042 Decision 6's withdrawal-delay clause).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBondParamsV2 {
    /// Minimum slashable collateral a bond registers with, in sompi.
    min_collateral_sompi: u64,
    /// DAA delay between a retirement request and withdrawal. The startup invariant demands it
    /// exceed the whole liability period, so a bond cannot commit fraud and leave before it is
    /// provable.
    withdrawal_delay_daa: u64,
}

impl PalwBondParamsV2 {
    pub fn new(min_collateral_sompi: u64, withdrawal_delay_daa: u64) -> Result<Self, PalwModeV2Error> {
        if min_collateral_sompi == 0 {
            return Err(PalwModeV2Error::Invalid("a zero minimum collateral bonds nothing"));
        }
        if withdrawal_delay_daa == 0 {
            return Err(PalwModeV2Error::Invalid("a zero withdrawal delay lets a bond leave mid-liability"));
        }
        Ok(Self { min_collateral_sompi, withdrawal_delay_daa })
    }

    pub fn min_collateral_sompi(&self) -> u64 {
        self.min_collateral_sompi
    }

    pub fn withdrawal_delay_daa(&self) -> u64 {
        self.withdrawal_delay_daa
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwModeV2Error {
    #[error("invalid V2 bundle: {0}")]
    Invalid(&'static str),
    #[error("invalid V2 bundle: {0}")]
    State(#[from] PalwStateV2Error),
    #[error("invalid V2 bundle: {0}")]
    Admission(#[from] PalwAdmissionV2Error),
    #[error("invalid V2 bundle: {0}")]
    FreePrompt(#[from] PalwFpV3Error),
}

/// The whole V2 ruleset, or none of it. Field order is part of the fingerprint preimage —
/// reordering is a different ruleset.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConsensusParamsV2 {
    /// == [`crate::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION`]; a bundle claiming another
    /// protocol is another ruleset.
    pub protocol_version: u16,
    /// == [`crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2`]; the only algorithm a V2 network
    /// demands or accepts.
    pub algorithm_id: u8,
    /// The permanently-Active liveness floor (ADR-0039 W6′): PALW-BASE-0's class id.
    pub base_class_id: Hash64,
    /// Commitment to the genesis-registered class set. The genesis loader verifies preimages.
    pub class_catalog_root: Hash64,
    /// Commitment to the adjudicable primitive set. The boot path runs
    /// `verify_catalog_coverage_v1` against this build's own catalog.
    pub court_catalog_root: Hash64,
    pub state: PalwStateParamsV2,
    pub admission: PalwAdmissionParamsV2,
    pub panel: PalwPanelParamsV2,
    pub reward: PalwRewardParamsV2,
    pub bond: PalwBondParamsV2,
    /// ADR-0044: the free-prompt receipt lane — a REQUIRED part of the bundle, not a fence. A
    /// ruleset without it is a different ruleset (a different `palw_ruleset_id_v2`), never this
    /// one with a switch off.
    pub freeprompt: PalwFreePromptParamsV3,
    /// Reorg safety margin added to the liability period in the withdrawal-delay invariant.
    pub reorg_margin_daa: u64,
    /// Measured worst-case honest prosecution time (the ladder-gap measurement's output); the
    /// court backstop window must exceed it or an honest prosecution can be timed out by its
    /// own clock.
    pub worst_case_court_duration_daa: u64,
}

impl PalwConsensusParamsV2 {
    /// The Decision 1 startup invariants. A node holding a `ConsensusV2` mode whose bundle fails
    /// any of these does not boot — there is no degraded mode, because a degraded mode is a
    /// half-flip with a friendlier name.
    pub fn validate(&self) -> Result<(), PalwModeV2Error> {
        if self.protocol_version != crate::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION {
            return Err(PalwModeV2Error::Invalid("protocol_version is not the V2 attempt version"));
        }
        if self.algorithm_id != crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
            return Err(PalwModeV2Error::Invalid("algorithm_id is not the committed-V2 id"));
        }
        if self.class_catalog_root == Hash64::default() || self.court_catalog_root == Hash64::default() {
            return Err(PalwModeV2Error::Invalid("a zero catalog root commits to nothing adjudicable"));
        }

        // BASE-0 exists and holds non-zero target share, and the share table sums to EXACTLY the
        // denominator here (the constructor allows partial tables for tests; a live bundle does
        // not get that latitude — an unallocated permille is a half-flip of the emission).
        let base_share = self
            .state
            .class_daa()
            .share_permille(&self.base_class_id)
            .ok_or(PalwModeV2Error::Invalid("BASE-0 carries no share — the liveness floor is unfunded"))?;
        if base_share == 0 {
            return Err(PalwModeV2Error::Invalid("BASE-0's share is zero"));
        }
        if self.state.class_daa().shares_sum_permille() != 1000 {
            return Err(PalwModeV2Error::Invalid("the class share table must allocate exactly 1000 permille"));
        }

        // Table coherence: every share-bearing class has an epoch budget and every budgeted
        // class has a share — a class present in one table and absent from the other is exactly
        // the between-tables gap audits kept finding.
        for class_id in self.state.class_daa().class_ids() {
            if self.admission.class_epoch_budget_pwu(&class_id).is_none() {
                return Err(PalwModeV2Error::Invalid("a share-bearing class has no epoch budget"));
            }
        }
        for class_id in self.admission.budgeted_class_ids() {
            if self.state.class_daa().share_permille(&class_id).is_none() {
                return Err(PalwModeV2Error::Invalid("a budgeted class has no share"));
            }
        }

        // The anchor slot sits strictly inside the bind window (PR-06's cross-check).
        self.panel
            .validate_against_state_params(&self.state)
            .map_err(|_| PalwModeV2Error::Invalid("the anchor slot does not sit inside the bind window"))?;

        // The court backstop exceeds the measured worst-case honest prosecution.
        if self.worst_case_court_duration_daa == 0 {
            return Err(PalwModeV2Error::Invalid("an unmeasured court duration cannot gate anything"));
        }
        if self.state.window_court() <= self.worst_case_court_duration_daa {
            return Err(PalwModeV2Error::Invalid("window_court does not fit the worst-case honest prosecution"));
        }

        // Withdrawal outlasts the whole liability period plus the reorg margin: a bond cannot
        // commit fraud and leave before it is provable.
        let liability = self
            .state
            .window_bind()
            .checked_add(self.state.window_receipt())
            .and_then(|x| x.checked_add(self.state.window_challenge()))
            .and_then(|x| x.checked_add(self.state.window_court()))
            .and_then(|x| x.checked_add(self.reorg_margin_daa))
            .ok_or(PalwModeV2Error::Invalid("the liability period overflows the DAA score"))?;
        if self.bond.withdrawal_delay_daa() <= liability {
            return Err(PalwModeV2Error::Invalid("the withdrawal delay does not outlast the liability period"));
        }

        // ---- ADR-0044: the free-prompt lane's startup invariants ----

        // The source split holds BOTH lanes open: a zero attempt share has no beacons (F16), a
        // full one has no receipts. The split lives in the state params (the retarget consumes
        // it); this gate is where its live range is enforced.
        let split = self.state.fp_attempt_share_permille();
        if !(1..=999).contains(&split) {
            return Err(PalwModeV2Error::Invalid("a live FP network needs 1..=999‰ attempt share — both lanes must exist"));
        }

        // Every share-bearing class must hold a non-zero COMPOSED share in both lanes, or the
        // retarget's skip arm silently freezes that lane's price for that class.
        for class_id in self.state.class_daa().class_ids() {
            let share = self.state.class_daa().share_permille(&class_id).expect("iterating the table's own keys");
            for lane_permille in [split as u32, 1000 - split as u32] {
                if (share as u32 * lane_permille + 500) / 1000 == 0 {
                    return Err(PalwModeV2Error::Invalid("a class's composed share rounds to zero in one lane — its price would freeze"));
                }
            }
        }

        // A late beacon must still bind inside the bind window: the panel's anchor is the FIRST
        // attempt-class block at the slot, and the declared worst-case gap to one is part of the
        // ruleset. Without this a thin floor quietly turns every FP claim into a BindTimeout.
        let worst_anchor = self
            .panel
            .anchor_delay()
            .checked_add(self.freeprompt.max_beacon_gap_daa())
            .ok_or(PalwModeV2Error::Invalid("the anchor slot plus the beacon gap overflows the DAA score"))?;
        if worst_anchor >= self.state.window_bind() {
            return Err(PalwModeV2Error::Invalid("anchor_delay + max_beacon_gap must sit inside the bind window"));
        }

        // The draw beacon sits past the reorgable fringe of the certification it draws for.
        if self.freeprompt.receipt_maturity_daa() < self.reorg_margin_daa {
            return Err(PalwModeV2Error::Invalid("receipt maturity must cover the reorg margin"));
        }
        Ok(())
    }

    /// Does this bundle accept the given header algorithm? A V2+FP network runs exactly two
    /// block kinds: the attempt id and the receipt id (ADR-0044 Decision 1). This is the
    /// two-id acceptance the FP-08 seam swap wires into the header/pruning gates; until then
    /// the wired seam still demands the attempt id exclusively, and no live network carries a
    /// bundle at all.
    pub fn accepts_algo_id(&self, algo_id: u8) -> bool {
        algo_id == self.algorithm_id || algo_id == self.freeprompt.receipt_algorithm_id()
    }
}

/// A network is in exactly one PALW mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwConsensusMode {
    /// Hash networks: PALW machinery inert. Every shipped preset.
    Disabled,
    /// The ADR-0035 algo-4 soak, exactly as the existing legacy fields describe it — this
    /// variant marks the mode; the legacy knobs stay where they live, because duplicating them
    /// here would be a second source for the same facts.
    LegacyTn11,
    /// The RC / mainnet ruleset — all of it.
    ConsensusV2(PalwConsensusParamsV2),
}

impl PalwConsensusMode {
    /// The algorithm a network in this mode DEMANDS. `None` leaves the decision to the existing
    /// (V1 / hash) rules; `Some` is exclusive — a V2 network accepts nothing else.
    pub fn required_algo_id(&self) -> Option<u8> {
        match self {
            PalwConsensusMode::Disabled | PalwConsensusMode::LegacyTn11 => None,
            PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.algorithm_id),
        }
    }
}

/// Decision 11: `H(canonical(bundle))`. Everything that decides consensus is inside; network
/// identity is not (the challenge's `network_domain` carries it), so the RC and mainnet share
/// one id and a node can CHECK sameness instead of trusting a release note.
pub fn palw_ruleset_id_v2(bundle: &PalwConsensusParamsV2) -> Hash64 {
    let bytes = borsh::to_vec(bundle).expect("the V2 bundle is borsh-serializable");
    let mut state = Blake2bParams::new().hash_length(64).key(PALW_RULESET_ID_V2_DOMAIN).to_state();
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::PalwClassDaaV2Params;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    pub(crate) fn conforming_freeprompt() -> PalwFreePromptParamsV3 {
        PalwFreePromptParamsV3::new(
            crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
            1_000,
            10,
            crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 },
            64,
            4_096,
            512,
            150,
            200,
            5,
        )
        .unwrap()
    }

    pub(crate) fn conforming_bundle() -> PalwConsensusParamsV2 {
        let base = h64(1);
        let class_daa = PalwClassDaaV2Params::new([(base, 1000u16)].into_iter().collect(), 4).unwrap();
        PalwConsensusParamsV2 {
            protocol_version: crate::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION,
            algorithm_id: crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            base_class_id: base,
            class_catalog_root: h64(0xCA7),
            court_catalog_root: h64(0xC0517),
            // Split 800‰: a live FP bundle holds BOTH lanes open (1..=999 is the gate).
            state: PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, 800, class_daa).unwrap(),
            admission: PalwAdmissionParamsV2::new(500, [(base, 10_000u128)].into_iter().collect()).unwrap(),
            panel: PalwPanelParamsV2::new(3, 2, 4).unwrap(),
            reward: PalwRewardParamsV2::new(620).unwrap(),
            bond: PalwBondParamsV2::new(20_000, 2_000).unwrap(),
            freeprompt: conforming_freeprompt(),
            reorg_margin_daa: 100,
            worst_case_court_duration_daa: 400,
        }
    }

    #[test]
    fn a_conforming_bundle_validates_and_fingerprints_deterministically() {
        let bundle = conforming_bundle();
        bundle.validate().expect("the fixture bundle holds every startup invariant");
        let id = palw_ruleset_id_v2(&bundle);
        assert_eq!(id, palw_ruleset_id_v2(&bundle.clone()), "the fingerprint is a pure function of the bundle");
        // The bundle accepts exactly its two block kinds (ADR-0044 Decision 1) — and nothing else.
        assert!(bundle.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2));
        assert!(bundle.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3));
        for other in [0u8, 1, 2, 3, 4, 5, 8, 0xff] {
            assert!(!bundle.accepts_algo_id(other), "algo {other} is neither lane");
        }
        assert_eq!(PalwConsensusMode::ConsensusV2(bundle).required_algo_id(), Some(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2));
        assert_eq!(PalwConsensusMode::Disabled.required_algo_id(), None);
        assert_eq!(PalwConsensusMode::LegacyTn11.required_algo_id(), None);
    }

    /// Every startup invariant refuses its own violation — the Decision 1 list, executable.
    #[test]
    fn every_startup_invariant_refuses_its_violation() {
        let cases: Vec<(&str, Box<dyn Fn(&mut PalwConsensusParamsV2)>)> = vec![
            ("protocol", Box::new(|b| b.protocol_version = 1)),
            ("algorithm", Box::new(|b| b.algorithm_id = 4)),
            ("class root", Box::new(|b| b.class_catalog_root = Hash64::default())),
            ("court root", Box::new(|b| b.court_catalog_root = Hash64::default())),
            ("unfunded base", Box::new(|b| b.base_class_id = h64(9))),
            (
                "partial share table",
                Box::new(|b| {
                    b.state = PalwStateParamsV2::new(
                        100,
                        10,
                        10,
                        20,
                        500,
                        1000,
                        800,
                        PalwClassDaaV2Params::new([(h64(1), 900u16)].into_iter().collect(), 4).unwrap(),
                    )
                    .unwrap()
                }),
            ),
            (
                "one-lane split (1000‰ has no receipts)",
                Box::new(|b| {
                    b.state = PalwStateParamsV2::new(
                        100,
                        10,
                        10,
                        20,
                        500,
                        1000,
                        1000,
                        PalwClassDaaV2Params::new([(h64(1), 1000u16)].into_iter().collect(), 4).unwrap(),
                    )
                    .unwrap()
                }),
            ),
            (
                "composed share rounds to zero in the receipt lane",
                Box::new(|b| {
                    // A 1‰ class under an 800/200 split: receipt-lane composed share is
                    // (1 × 200 + 500) / 1000 = 0 — its receipt price would silently freeze.
                    let table = [(h64(1), 999u16), (h64(2), 1u16)].into_iter().collect();
                    b.state =
                        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, 800, PalwClassDaaV2Params::new(table, 4).unwrap())
                            .unwrap();
                    b.admission = PalwAdmissionParamsV2::new(
                        500,
                        [(h64(1), 10_000u128), (h64(2), 10u128)].into_iter().collect(),
                    )
                    .unwrap();
                }),
            ),
            (
                "beacon gap outside the bind window",
                Box::new(|b| {
                    b.freeprompt = PalwFreePromptParamsV3::new(
                        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                        1_000,
                        10,
                        crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 },
                        64,
                        4_096,
                        512,
                        100,
                        200,
                        6, // anchor_delay 4 + gap 6 = 10 ≥ window_bind 10
                    )
                    .unwrap()
                }),
            ),
            (
                "receipt maturity inside the reorg margin",
                Box::new(|b| {
                    b.freeprompt = PalwFreePromptParamsV3::new(
                        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                        1_000,
                        10,
                        crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 },
                        64,
                        4_096,
                        512,
                        99, // reorg_margin_daa is 100
                        200,
                        5,
                    )
                    .unwrap()
                }),
            ),
            (
                "budgetless share",
                Box::new(|b| b.admission = PalwAdmissionParamsV2::new(500, std::collections::BTreeMap::new()).unwrap()),
            ),
            (
                "shareless budget",
                Box::new(|b| {
                    b.admission =
                        PalwAdmissionParamsV2::new(500, [(h64(1), 10_000u128), (h64(2), 5u128)].into_iter().collect()).unwrap()
                }),
            ),
            ("anchor outside bind window", Box::new(|b| b.panel = PalwPanelParamsV2::new(3, 2, 10).unwrap())),
            ("unmeasured court", Box::new(|b| b.worst_case_court_duration_daa = 0)),
            ("court window too small", Box::new(|b| b.worst_case_court_duration_daa = 500)),
            ("withdrawal inside liability", Box::new(|b| b.bond = PalwBondParamsV2::new(20_000, 640).unwrap())),
        ];
        for (name, mutate) in cases {
            let mut bundle = conforming_bundle();
            mutate(&mut bundle);
            assert!(bundle.validate().is_err(), "{name}: the mutated bundle must not validate");
        }
    }

    /// Every shipped preset is `Disabled` — the V2 lineage's dormancy is one enum arm, checked
    /// as a fact, and the config gate holds the atomicity rules the arm implies.
    #[test]
    fn every_shipped_preset_is_disabled_and_the_gate_refuses_mixed_lineages() {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};
        for params in [&MAINNET_PARAMS, &TESTNET_PARAMS, &DEVNET_PARAMS, &SIMNET_PARAMS] {
            assert_eq!(params.palw_consensus_mode, PalwConsensusMode::Disabled, "{} ships Disabled", params.net);
            params.validate_palw_v2().expect("Disabled has nothing to validate");
        }
        // …except the PALW staging net, which says what it is since the Relaunch-2 re-genesis.
        assert_eq!(TESTNET11_PARAMS.palw_consensus_mode, PalwConsensusMode::LegacyTn11, "t11 marks the legacy lineage");
        TESTNET11_PARAMS.validate_palw_v2().expect("LegacyTn11 adds no new constraints");

        // A ConsensusV2 params set with a conforming bundle and no V1 residue validates.
        // SIMNET is the clean base: its `pow_palw_activation` is `never()`.
        let mut v2 = SIMNET_PARAMS.clone();
        v2.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        v2.validate_palw_v2().expect("a pure V2 set validates");

        // …a broken bundle does not…
        let mut broken = conforming_bundle();
        broken.worst_case_court_duration_daa = 0;
        let mut bad_bundle = SIMNET_PARAMS.clone();
        bad_bundle.palw_consensus_mode = PalwConsensusMode::ConsensusV2(broken);
        assert!(bad_bundle.validate_palw_v2().is_err(), "the startup invariants gate the config");

        // …and mixing lineages does not — in BOTH shapes. DEVNET is a live V1 PALW network, so
        // declaring a V2 mode on it is refused outright…
        let mut mixed_pow = DEVNET_PARAMS.clone();
        mixed_pow.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        assert!(mixed_pow.validate_palw_v2().is_err(), "a V1 PALW PoW activation under a V2 mode is half of two lineages");
        // …and so is a V1 fence smuggled under a V2 mode on a clean base.
        let mut mixed_fence = SIMNET_PARAMS.clone();
        mixed_fence.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        mixed_fence.palw_ramp = Some(crate::palw_weight::PalwWeightParamsV1 { receipt_quorum: 2, rho_r_permille: 250 });
        assert!(mixed_fence.validate_palw_v2().is_err(), "a V1 fence under a V2 mode is the five-fences defect reborn");
    }

    /// **The PR-08 seam is inert on every shipped network.** For every preset, at a sweep of DAA
    /// scores, the mode-aware required-algo answer equals the V1 cascade's answer byte for byte —
    /// so threading the mode through the header, virtual and pruning-proof gates changed nothing
    /// any node accepts today. Only a `ConsensusV2` network answers differently, and no shipped
    /// preset is one.
    #[test]
    fn the_mode_seam_changes_no_shipped_networks_required_algo() {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};
        use crate::pow_layer0::{
            POW_ALGO_ID_PALW_COMMITTED_V2, check_algo_id, check_algo_id_for_mode, required_algo_id, required_algo_id_for_mode,
        };

        for params in [&MAINNET_PARAMS, &TESTNET_PARAMS, &TESTNET11_PARAMS, &DEVNET_PARAMS, &SIMNET_PARAMS] {
            let mode_required = params.palw_consensus_mode.required_algo_id();
            assert_eq!(mode_required, None, "{} demands no V2 id", params.net);
            for daa in [0u64, 1, 1_000, 1_000_000, u64::MAX - 1] {
                let (o, l, s) = (
                    params.pow_palw_ollama_activation.is_active(daa),
                    params.pow_palw_activation.is_active(daa),
                    params.pow_blake2b_sha3_activation.is_active(daa),
                );
                let v1 = required_algo_id(o, l, s);
                assert_eq!(
                    required_algo_id_for_mode(mode_required, o, l, s),
                    v1,
                    "{} @ {daa}: the seam moved a live network",
                    params.net
                );
                assert_eq!(check_algo_id_for_mode(v1, mode_required, o, l, s), check_algo_id(v1, o, l, s));
                assert!(
                    check_algo_id_for_mode(POW_ALGO_ID_PALW_COMMITTED_V2, mode_required, o, l, s).is_err(),
                    "{} @ {daa} must still refuse a V2 header",
                    params.net
                );
            }
        }
    }

    /// The mode is in the P2P consensus fingerprint — through the ruleset id, so the handshake
    /// commitment and the ruleset commitment cannot drift — and `Disabled` leaves the
    /// fingerprint exactly where it was before the field existed.
    #[test]
    fn the_mode_moves_the_consensus_fingerprint() {
        use crate::config::params::DEVNET_PARAMS;
        let disabled = DEVNET_PARAMS.consensus_params_id();
        let mut legacy = DEVNET_PARAMS.clone();
        legacy.palw_consensus_mode = PalwConsensusMode::LegacyTn11;
        let mut v2 = DEVNET_PARAMS.clone();
        v2.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        let legacy_id = legacy.consensus_params_id();
        let v2_id = v2.consensus_params_id();
        assert_ne!(disabled, legacy_id, "the legacy mark separates at handshake");
        assert_ne!(disabled, v2_id, "a V2 network separates at handshake");
        assert_ne!(legacy_id, v2_id);

        // And two V2 networks with different bundles separate too, through the ruleset id.
        let mut other = conforming_bundle();
        other.reward = crate::palw_reward_v2::PalwRewardParamsV2::new(621).unwrap();
        let mut v2b = DEVNET_PARAMS.clone();
        v2b.palw_consensus_mode = PalwConsensusMode::ConsensusV2(other);
        assert_ne!(v2_id, v2b.consensus_params_id(), "a different ruleset is a different handshake");
    }

    /// Decision 11's property: any consensus-deciding byte moves the id, and network identity is
    /// not in the preimage at all (there is no field for it — RC and mainnet share the id by
    /// construction, and the challenge's network_domain keeps their blocks apart).
    #[test]
    fn every_bundle_byte_moves_the_ruleset_id() {
        let base_id = palw_ruleset_id_v2(&conforming_bundle());
        let mutations: Vec<(&str, Box<dyn Fn(&mut PalwConsensusParamsV2)>)> = vec![
            ("reward carve", Box::new(|b| b.reward = PalwRewardParamsV2::new(621).unwrap())),
            ("panel quorum", Box::new(|b| b.panel = PalwPanelParamsV2::new(3, 3, 4).unwrap())),
            ("bond floor", Box::new(|b| b.bond = PalwBondParamsV2::new(20_001, 2_000).unwrap())),
            ("reorg margin", Box::new(|b| b.reorg_margin_daa += 1)),
            ("court measurement", Box::new(|b| b.worst_case_court_duration_daa += 1)),
            (
                "exposure ratio",
                Box::new(|b| b.admission = PalwAdmissionParamsV2::new(501, [(h64(1), 10_000u128)].into_iter().collect()).unwrap()),
            ),
            (
                "free-prompt quantum",
                Box::new(|b| {
                    b.freeprompt = PalwFreePromptParamsV3::new(
                        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                        1_001,
                        10,
                        crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 },
                        64,
                        4_096,
                        512,
                        100,
                        200,
                        5,
                    )
                    .unwrap()
                }),
            ),
            (
                "free-prompt cu price",
                Box::new(|b| {
                    b.freeprompt = PalwFreePromptParamsV3::new(
                        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                        1_000,
                        10,
                        crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 2, decode_weight: 64 },
                        64,
                        4_096,
                        512,
                        100,
                        200,
                        5,
                    )
                    .unwrap()
                }),
            ),
        ];
        for (name, mutate) in mutations {
            let mut bundle = conforming_bundle();
            mutate(&mut bundle);
            assert!(bundle.validate().is_ok(), "{name}: this mutation is a VALID other ruleset");
            assert_ne!(palw_ruleset_id_v2(&bundle), base_id, "{name}: a consensus byte moved and the id did not");
        }
    }
}
