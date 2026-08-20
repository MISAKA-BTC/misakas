//! Installing the free-prompt V2 ruleset onto a network's parameters (ADR-0044 FP-09).
//!
//! # Why this is a function and not a `const`
//!
//! Every shipped preset is a `const Params`. A `ConsensusV2` bundle is not const-constructible —
//! it carries class lists, bond registrations and per-class tables — and more importantly its
//! genesis bond's PUBLIC KEY is a network artifact that some operator holds the secret half of.
//! A library cannot invent one, so the preset is a function over the artifacts a genesis
//! actually registered, and a caller holding an `Ok` holds parameters a node will boot on.
//!
//! # What installing changes, and what it must not
//!
//! Installing flips `palw_consensus_mode` to `ConsensusV2`, which changes which headers are valid
//! wholesale — so it changes `consensus_params_id`, and two nodes that disagree about whether it
//! is installed cannot handshake. That is the intended behaviour (ADR-0042 Decision 11) and it is
//! what makes an accidental half-rollout a refused connection rather than a partition.
//!
//! What it must NOT do is leave a MIXED lineage. `LegacyTn11` and `ConsensusV2` are two different
//! PALW rulesets, and the legacy V1 knobs (`palw_credit`, `palw_fork_choice`, `palw_schedule`,
//! `palw_ramp`, `palw_block_commitment`) belong to the first. A network carrying both would run
//! two weighing rules over one chain, which is the shape of P0-5. So this refuses rather than
//! silently clearing them: an operator who set a V1 knob meant something by it, and dropping it
//! quietly would be this function deciding a consensus question.

use crate::config::params::Params;
use crate::Hash64;
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::tx::TransactionOutpoint;

/// The genesis artifacts a free-prompt network is founded on.
///
/// Every field is something the genesis *registers*, not something a library chooses: the class
/// the network audits within, the court catalog it prosecutes under, and the one bond that must
/// exist before any block can be produced at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFpGenesisArtifactsV3 {
    /// The registered BASE-0 class id.
    pub base_class_id: Hash64,
    /// The court catalog root — the ruleset refutations are adjudicated under.
    pub court_catalog_root: Hash64,
    /// The genesis bond's outpoint, and the key its holder signs with.
    pub genesis_bond: TransactionOutpoint,
    pub genesis_bond_pubkey: Vec<u8>,
    pub genesis_operator_id: Hash64,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwFpPresetV3Error {
    #[error("the bundle does not validate: {0}")]
    Bundle(#[from] PalwModeV2Error),
    #[error(
        "these parameters already run a PALW ruleset ({0}) — installing the free-prompt ruleset on top would leave two \
         weighing rules over one chain"
    )]
    AlreadyPalw(&'static str),
    #[error("these parameters carry the legacy V1 PALW knob `{0}`, which belongs to a different ruleset")]
    LegacyKnob(&'static str),
}

/// Install the free-prompt V2 ruleset onto `params`.
///
/// Refuses a base that already runs PALW in any form — see the module docs. On success the
/// returned `Params` differ from the input in exactly one field, and therefore in their
/// `consensus_params_id`.
pub fn palw_fp_install_v3(mut params: Params, artifacts: &PalwFpGenesisArtifactsV3) -> Result<Params, PalwFpPresetV3Error> {
    match &params.palw_consensus_mode {
        PalwConsensusMode::Disabled => {}
        PalwConsensusMode::LegacyTn11 => return Err(PalwFpPresetV3Error::AlreadyPalw("LegacyTn11")),
        PalwConsensusMode::ConsensusV2(_) => return Err(PalwFpPresetV3Error::AlreadyPalw("ConsensusV2")),
    }
    for (name, present) in [
        ("palw_credit", params.palw_credit.is_some()),
        ("palw_fork_choice", params.palw_fork_choice.is_some()),
        ("palw_schedule", params.palw_schedule.is_some()),
        ("palw_ramp", params.palw_ramp.is_some()),
        ("palw_block_commitment", params.palw_block_commitment.is_some()),
    ] {
        if present {
            return Err(PalwFpPresetV3Error::LegacyKnob(name));
        }
    }
    let bundle = crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_derived_root_v3(
        artifacts.base_class_id,
        artifacts.court_catalog_root,
        artifacts.genesis_bond,
        artifacts.genesis_bond_pubkey.clone(),
        artifacts.genesis_operator_id,
    )?;
    params.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle);
    Ok(params)
}

/// The free-prompt DEVNET preset: `DEVNET_PARAMS` with the ruleset installed.
///
/// This is the shape an RC network takes — the base is a shipped preset and the only difference
/// is the mode, so everything a devnet already agreed about (timestamps, subsidies, pruning) is
/// untouched and the diff a reviewer reads is one field.
pub fn palw_fp_devnet_params_v3(artifacts: &PalwFpGenesisArtifactsV3) -> Result<Params, PalwFpPresetV3Error> {
    palw_fp_install_v3(crate::config::params::DEVNET_PARAMS, artifacts)
}

/// The free-prompt SIMNET preset — the same install on the base with no PALW lineage at all,
/// which is what the harness and the drills run on.
pub fn palw_fp_simnet_params_v3(artifacts: &PalwFpGenesisArtifactsV3) -> Result<Params, PalwFpPresetV3Error> {
    palw_fp_install_v3(crate::config::params::SIMNET_PARAMS, artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET11_PARAMS, TESTNET_PARAMS};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn artifacts() -> PalwFpGenesisArtifactsV3 {
        PalwFpGenesisArtifactsV3 {
            base_class_id: h64(0xBA5E),
            court_catalog_root: h64(0xC0757),
            genesis_bond: TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(0xB0D), 0),
            genesis_bond_pubkey: vec![0x11; 32],
            genesis_operator_id: h64(0xE0),
        }
    }

    /// **Installing is visible in the handshake.** The ruleset decides block validity wholesale,
    /// so it is inside `consensus_params_id` — a node that installed it and one that did not
    /// cannot connect, which is what turns a half-rollout into a refusal instead of a partition.
    #[test]
    fn installing_changes_the_consensus_params_id() {
        let installed = palw_fp_devnet_params_v3(&artifacts()).expect("the devnet preset installs");
        assert_ne!(
            installed.consensus_params_id(),
            DEVNET_PARAMS.consensus_params_id(),
            "a network running the free-prompt ruleset must not fingerprint as a dormant one"
        );
        assert!(matches!(installed.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)));
        // Two installs of the SAME artifacts agree — the id is a function of the ruleset, not of
        // when it was built.
        assert_eq!(installed.consensus_params_id(), palw_fp_devnet_params_v3(&artifacts()).unwrap().consensus_params_id());
        // And different artifacts are a different network.
        let mut other = artifacts();
        other.genesis_operator_id = h64(0xE1);
        assert_ne!(installed.consensus_params_id(), palw_fp_devnet_params_v3(&other).unwrap().consensus_params_id());
    }

    /// **No shipped preset moves.** This module exists beside them; if merely defining it changed
    /// any of the five, that would be a flag day nobody asked for.
    #[test]
    fn every_shipped_preset_is_untouched() {
        for (name, params) in [
            ("mainnet", MAINNET_PARAMS),
            ("testnet10", TESTNET_PARAMS),
            ("testnet11", TESTNET11_PARAMS),
            ("simnet", SIMNET_PARAMS),
            ("devnet", DEVNET_PARAMS),
        ] {
            match params.palw_consensus_mode {
                PalwConsensusMode::ConsensusV2(_) => panic!("{name} ships the V2 ruleset — this preset is not installed anywhere"),
                _ => {}
            }
        }
    }

    /// **A mixed lineage is refused, not merged.** Installing on a network that already runs a
    /// PALW ruleset — or that carries a V1 knob — would put two weighing rules over one chain.
    #[test]
    fn a_mixed_palw_lineage_is_refused() {
        assert_eq!(
            palw_fp_install_v3(TESTNET11_PARAMS, &artifacts()).unwrap_err(),
            PalwFpPresetV3Error::AlreadyPalw("LegacyTn11"),
            "testnet11 runs the algo-4 lineage"
        );
        let installed = palw_fp_simnet_params_v3(&artifacts()).unwrap();
        assert_eq!(
            palw_fp_install_v3(installed, &artifacts()).unwrap_err(),
            PalwFpPresetV3Error::AlreadyPalw("ConsensusV2"),
            "installing twice is not idempotent, it is a mistake"
        );

        let mut with_knob = SIMNET_PARAMS;
        with_knob.palw_ramp = Some(crate::palw_weight::PalwWeightParamsV1 { receipt_quorum: 3, rho_r_permille: 500 });
        assert_eq!(palw_fp_install_v3(with_knob, &artifacts()).unwrap_err(), PalwFpPresetV3Error::LegacyKnob("palw_ramp"));
    }

    /// A preset that will not boot is not a preset. The bundle's own gate runs at construction,
    /// so a bad artifact set fails HERE rather than at a node's first block.
    #[test]
    fn a_preset_that_would_not_boot_is_refused() {
        let mut empty_key = artifacts();
        empty_key.genesis_bond_pubkey = Vec::new();
        assert!(
            matches!(palw_fp_simnet_params_v3(&empty_key), Err(PalwFpPresetV3Error::Bundle(_))),
            "a genesis bond with no key can never sign, so no block could ever be produced"
        );
    }
}
