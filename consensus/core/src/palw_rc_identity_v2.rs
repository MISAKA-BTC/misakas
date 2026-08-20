//! MISAKA PALW-RC network identity (ADR-0036 Decision 2, ADR-0042 PR-10).
//!
//! # Why a new identity, measured rather than asserted
//!
//! ADR-0036 Decision 2 and the 2026-08-16/17 readiness audit (H13) reached the same conclusion
//! independently: the shipped mainnet identity cannot carry PALW. Both state it as prose, and
//! prose is a thing that can quietly stop being true — a preset edit, a Crescendo retune. It is
//! pinned as arithmetic in `palw_schedule`'s
//! `the_shipped_mainnet_identity_cannot_carry_a_palw_schedule`, over the values the binary really
//! ships, and there are TWO independent refusals:
//!
//! * **the cadence** — PALW is frozen at 120 s/block (ADR-0038 Decision H) and mainnet runs
//!   10 BPS. Not a window that can be widened: every window in the ruleset is DAA-denominated, so
//!   the same numbers mean something different on a 100 ms chain;
//! * **the window inequality** — at 10 BPS `finality_depth` is orders of magnitude above either
//!   shipped `w_challenge`, so `finality_depth < w_challenge` fails on the depth rather than on
//!   anything a schedule can choose.
//!
//! Fixing either alone leaves the other standing. That is why the answer is a new network
//! identity and not a parameter change.
//!
//! # What this module is, and what it deliberately is not
//!
//! It is the ASSEMBLY, gated. `assemble_palw_rc_identity_v2` takes a base `Params`, a
//! `ConsensusV2` bundle, its class-catalog preimage and the genesis registration objects, and
//! returns a `Params` only if every gate this lineage has built agrees:
//!
//! 1. the bundle is a well-formed, runnable ruleset (`validate`);
//! 2. the catalog is the one the ruleset committed to, its coverage is complete, and every
//!    registration agrees with it — including the declared `pwu_per_inference`
//!    (`palw_genesis_v2::verify_palw_genesis_v2`);
//! 3. the network is at the frozen cadence, sets no V1 fence, and activates no V1 PALW PoW
//!    (`Params::validate_palw_v2`);
//! 4. the genesis objects actually apply — the first transition runs and its state root exists,
//!    so "this artifact boots" is a computation rather than a plan.
//!
//! It is **not** a shipped preset, and nothing here adds one. `params_do_not_install_a_palw_fence`
//! still holds: every network in `config::params` is `Disabled` or `LegacyTn11`. Shipping is
//! PR-10's last step and needs the operator items ADR-0035 §6 owns — seeds, ports, public entry —
//! plus the measured parameters that are soak outputs (ADR-0036). What this closes is the gap
//! between "the pieces exist" and "the pieces compose", which is where an RC genesis fails if
//! nobody made it compose before the day it is minted.

use crate::config::params::Params;
use crate::palw_genesis_v2::{PalwGenesisV2Error, verify_palw_genesis_v2};
use crate::palw_mode_v2::{PalwClassCatalogV2, PalwConsensusMode, PalwConsensusParamsV2, PalwModeV2Error};
use crate::palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2, PalwStateV2Error, apply_palw_transition_v2};

#[derive(thiserror::Error, Debug)]
pub enum PalwRcIdentityError {
    #[error("the ruleset does not boot: {0}")]
    Ruleset(#[from] PalwModeV2Error),
    #[error("the genesis artifact does not load: {0}")]
    Genesis(#[from] PalwGenesisV2Error),
    #[error("the genesis registrations do not apply: {0}")]
    Transition(#[from] PalwStateV2Error),
    #[error("the base params already carry a PALW mode — an RC identity is assembled from a mode-free base")]
    BaseAlreadyHasAMode,
}

/// The assembled identity: the params a node would run, and the state its genesis block produces.
///
/// The state is returned rather than discarded because it is the evidence: an assembly that
/// type-checks but whose genesis objects cannot apply is exactly the failure this function exists
/// to catch, and the caller holding the resulting root can commit it (Decision 11).
#[derive(Debug)]
pub struct PalwRcIdentityV2 {
    pub params: Params,
    pub genesis_state: PalwChainStateV2,
}

/// Assemble and gate a PALW-RC identity. See the module header for what each gate is for.
///
/// `base` is a mode-free `Params` carrying the network's own identity, cadence and depths —
/// everything that is not PALW. Requiring it to be mode-free is not ceremony: assembling ON TOP
/// of a network that already declares a mode is how two rulesets end up half-applied to one
/// chain, and the caller that wants a different ruleset should start from the base again.
pub fn assemble_palw_rc_identity_v2(
    base: &Params,
    bundle: PalwConsensusParamsV2,
    catalog: &PalwClassCatalogV2,
    genesis_objects: &[PalwConsensusObjectV2],
    genesis_context: &PalwBlockContextV2,
) -> Result<PalwRcIdentityV2, PalwRcIdentityError> {
    if !matches!(base.palw_consensus_mode, PalwConsensusMode::Disabled) {
        return Err(PalwRcIdentityError::BaseAlreadyHasAMode);
    }
    // The catalog gates run BEFORE the params are assembled, so a bad artifact never produces a
    // `Params` value at all — there is no half-built identity for a caller to mistake for one.
    verify_palw_genesis_v2(&bundle, catalog, genesis_objects)?;

    let mut params = base.clone();
    params.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle);
    // Cadence, V1-fence exclusion, V1-PoW exclusion, and the bundle's own `validate`.
    params.validate_palw_v2()?;

    // The last gate, and the one no amount of shape checking replaces: RUN the genesis block. A
    // registration list that passes every static check and then fails to apply — a share table
    // that does not conserve, a class registered before its bond — would be discovered by the
    // first node to boot the network, which is the worst possible time.
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        unreachable!("just installed");
    };
    let state_params = bundle.state.clone();
    let (genesis_state, _delta) =
        apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params, genesis_context, genesis_objects, None)?;
    Ok(PalwRcIdentityV2 { params, genesis_state })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;
    use crate::palw_mode_v2::{PalwClassCatalogEntryV2, PalwCourtParamsV2};
    use crate::palw_state_v2::{PalwBondKeyV2, PalwPwuRuleV2};
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    const LEAVES: u64 = 1 << 16;
    const CANONICAL: u64 = 4_096;

    fn catalog() -> PalwClassCatalogV2 {
        PalwClassCatalogV2::new(vec![PalwClassCatalogEntryV2 {
            class_id: h64(1),
            artifact_root: h64(0xA7),
            max_step_leaf_count: LEAVES,
            canonical_step_leaf_count: CANONICAL,
            reachable_kernels: crate::palw_step_refute::catalogued_kernel_ids_v1(),
        }])
        .expect("a well-formed catalog")
    }

    fn bundle(catalog: &PalwClassCatalogV2) -> PalwConsensusParamsV2 {
        let mut b = crate::palw_mode_v2::tests::conforming_bundle();
        b.base_class_id = h64(1);
        b.class_catalog_root = catalog.root();
        b.court = PalwCourtParamsV2::new(LEAVES, 4, 2).expect("a court that can walk the catalog");
        b
    }

    fn genesis_objects() -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(0xA7),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: CANONICAL },
                initial_target: u128::MAX / 2,
                share_permille: 1000,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 }),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 100_000,
            },
        ]
    }

    fn ctx() -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(0x6E), daa_score: 0, blue_score: 0 }
    }

    /// A base at the frozen cadence and a proportionate depth — the network identity ADR-0036
    /// Decision 2 says mainnet PALW needs, expressed as the edit that makes SIMNET admissible.
    fn base() -> Params {
        let mut p = crate::config::params::SIMNET_PARAMS.clone();
        p.blockrate.target_time_per_block = crate::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS;
        p
    }

    #[test]
    fn an_rc_identity_assembles_and_its_genesis_block_runs() {
        let catalog = catalog();
        let assembled = assemble_palw_rc_identity_v2(&base(), bundle(&catalog), &catalog, &genesis_objects(), &ctx())
            .expect("the artifact assembles");

        assert!(matches!(assembled.params.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)));
        // The genesis block really ran: the floor holds the whole share table and the state has a
        // root to commit (Decision 11).
        assert_eq!(assembled.genesis_state.class_share_permille(&h64(1)), Some(1000));
        assert_ne!(assembled.genesis_state.state_root(), PalwChainStateV2::genesis().state_root());

        // And the identity is deterministic: the same artifact assembles to the same ruleset id
        // and the same genesis root, which is what makes "the RC and mainnet are the same rules"
        // a hash comparison rather than a release note.
        let again = assemble_palw_rc_identity_v2(&base(), bundle(&catalog), &catalog, &genesis_objects(), &ctx()).unwrap();
        assert_eq!(again.params.consensus_params_id(), assembled.params.consensus_params_id());
        assert_eq!(again.genesis_state.state_root(), assembled.genesis_state.state_root());
    }

    /// The cadence gate is the one ADR-0036 Decision 2 turns on, so it is asserted through the
    /// assembly rather than only through `validate_palw_v2` directly.
    #[test]
    fn a_base_at_the_wrong_cadence_cannot_become_an_rc_identity() {
        let catalog = catalog();
        let mut wrong = base();
        wrong.blockrate.target_time_per_block = 100; // 10 BPS — the shipped mainnet cadence
        let err = assemble_palw_rc_identity_v2(&wrong, bundle(&catalog), &catalog, &genesis_objects(), &ctx()).unwrap_err();
        assert!(matches!(err, PalwRcIdentityError::Ruleset(_)), "got {err:?}");
    }

    /// A catalog disagreement fails the assembly rather than producing a `Params` that boots and
    /// then misprices its own class — the whole point of gating before construction.
    #[test]
    fn a_registration_that_disagrees_with_the_catalog_fails_the_assembly() {
        let catalog = catalog();
        let mut lying = genesis_objects();
        if let PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } = &mut lying[0] {
            *pwu_rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: CANONICAL * 10 };
        }
        let err = assemble_palw_rc_identity_v2(&base(), bundle(&catalog), &catalog, &lying, &ctx()).unwrap_err();
        assert!(
            matches!(err, PalwRcIdentityError::Genesis(PalwGenesisV2Error::PwuPerInferenceMismatch { .. })),
            "got {err:?}"
        );
    }

    /// Registrations that pass every static check and cannot APPLY are caught here rather than by
    /// the first node to boot the network. The share table is the sharpest case: the floor must
    /// take the whole 1000‰, and a genesis that gives it less is arithmetic no static check reads.
    #[test]
    fn genesis_objects_that_cannot_apply_fail_the_assembly() {
        let catalog = catalog();
        let mut half = genesis_objects();
        if let PalwConsensusObjectV2::ClassRegistered { share_permille, .. } = &mut half[0] {
            *share_permille = 500;
        }
        let err = assemble_palw_rc_identity_v2(&base(), bundle(&catalog), &catalog, &half, &ctx()).unwrap_err();
        assert!(matches!(err, PalwRcIdentityError::Transition(PalwStateV2Error::FirstShareMustBeWhole { got: 500 })), "got {err:?}");
    }

    /// Assembling on top of a network that already declares a mode is refused: that is how two
    /// rulesets end up half-applied to one chain.
    #[test]
    fn an_rc_identity_is_assembled_from_a_mode_free_base() {
        let catalog = catalog();
        let mut already = base();
        already.palw_consensus_mode = PalwConsensusMode::LegacyTn11;
        let err = assemble_palw_rc_identity_v2(&already, bundle(&catalog), &catalog, &genesis_objects(), &ctx()).unwrap_err();
        assert!(matches!(err, PalwRcIdentityError::BaseAlreadyHasAMode), "got {err:?}");
    }

    /// **Nothing here ships a preset.** The assembly exists so the RC genesis is a checked
    /// artifact, not so a network quietly acquires one — every shipped preset is still fence-free.
    #[test]
    fn assembling_an_identity_installs_no_fence_on_any_shipped_preset() {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET11_PARAMS, TESTNET_PARAMS};
        let catalog = catalog();
        let _ = assemble_palw_rc_identity_v2(&base(), bundle(&catalog), &catalog, &genesis_objects(), &ctx()).unwrap();
        for (name, params) in [
            ("mainnet", &MAINNET_PARAMS),
            ("testnet", &TESTNET_PARAMS),
            ("testnet-11", &TESTNET11_PARAMS),
            ("devnet", &DEVNET_PARAMS),
            ("simnet", &SIMNET_PARAMS),
        ] {
            assert!(params.palw_credit.is_none(), "{name}: no shipped preset installs a V1 fence");
            assert!(
                !matches!(params.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)),
                "{name}: no shipped preset carries a ConsensusV2 bundle"
            );
        }
    }
}
