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
//!    so "this artifact boots" is a computation rather than a plan;
//! 5. **the court's ladder is provisioned for the whole step space**, not for the class set this
//!    genesis happens to carry. `max_step_leaf_count` is a bundle field, so it is inside
//!    `palw_ruleset_id_v2` and therefore inside the network's identity: a class deeper than the
//!    ladder cannot join a running chain, it needs a new ruleset. Unlike the other four gates this
//!    one is not about whether the artifact is correct — it is about whether the network can ever
//!    admit a second class, and the answer is decided once, here, and cannot be revisited. It is a
//!    refusal rather than an installation because silently rewriting a caller's court would change
//!    the ruleset id underneath them.
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
    /// Gate 5. The ladder must be `PALW_RC_COURT_MAX_STEP_LEAF_COUNT`, which is the step space's
    /// own cap: measured, provisioning it there rather than at the RC floor's 184,456 costs four
    /// bisection rounds (18 → 22) and buys every class that could ever be adjudicable, because
    /// `worst_case_step_leaf_count_v1` refuses anything deeper than the cap in the first place.
    #[error(
        "the court's ladder is {got} step leaves; an RC identity must provision the whole step space ({want})          or it can never admit a second class — the value is inside the ruleset id and cannot be revisited"
    )]
    LadderNotProvisionedForTheStepSpace { got: u64, want: u64 },
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
    // Gate 5 first, because it is the only one whose answer expires. Everything below decides
    // whether THIS artifact is correct; this decides whether the network the artifact mints can
    // ever be joined by a class that does not exist yet.
    let ladder = bundle.court.max_step_leaf_count();
    if ladder != crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT {
        return Err(PalwRcIdentityError::LadderNotProvisionedForTheStepSpace {
            got: ladder,
            want: crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT,
        });
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

    /// BASE-0's OWN reachable kernel set (ADR-0040 D/H's ten), not
    /// `catalogued_kernel_ids_v1()`. Building the fixture from the build's own table would make
    /// the coverage gate certify itself, which is the shape the 2026-08-17 re-audit found in the
    /// gate's own signature — and the RC's floor is exactly the class whose adjudicability must
    /// not be assumed. `base0_reaches_only_kernels_this_build_adjudicates` measures that the two
    /// sets really do agree today.
    fn catalog() -> PalwClassCatalogV2 {
        PalwClassCatalogV2::new(vec![PalwClassCatalogEntryV2 {
            class_id: h64(1),
            artifact_root: h64(0xA7),
            max_step_leaf_count: LEAVES,
            canonical_step_leaf_count: CANONICAL,
            reachable_kernels: crate::palw_step_refute::KDESC_BASE0_ALL
                .iter()
                .map(|d| crate::palw_step::kernel_semantics_id_v1(d))
                .collect(),
        }])
        .expect("a well-formed catalog")
    }

    fn bundle(catalog: &PalwClassCatalogV2) -> PalwConsensusParamsV2 {
        let mut b = crate::palw_mode_v2::tests::conforming_bundle();
        b.base_class_id = h64(1);
        b.class_catalog_root = catalog.root();
        b.court = PalwCourtParamsV2::new(crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT, 4, 2)
            .expect("the step space is a legal ladder");
        // The sweep measures rung silence against the STATE's copy, so the two move together or
        // the bundle is audited against one ladder and run against another.
        b.state = b.state.clone().with_turn_deadline_daa(4).expect("4 is inside the court window");
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
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
        ]
    }

    fn ctx() -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(0x6E), daa_score: 0, blue_score: 0, subsidy: 0 }
    }

    /// A base at the frozen cadence and a proportionate depth — the network identity ADR-0036
    /// Decision 2 says mainnet PALW needs, expressed as the edit that makes SIMNET admissible.
    fn base() -> Params {
        let mut p = crate::config::params::SIMNET_PARAMS.clone();
        p.blockrate.target_time_per_block = crate::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS;
        p
    }

    #[test]
    /// **Gate 5: the RC cannot mint an identity that can never admit a second class.**
    ///
    /// `max_step_leaf_count` is a bundle field and the bundle is `palw_ruleset_id_v2`, so the
    /// ladder a network freezes is the deepest class it will ever be able to admit. Sizing it to
    /// the class set this genesis happens to carry is the one mistake here that cannot be repaired
    /// later — by the time the second class exists the number is already part of the network's
    /// identity. So a floor-sized ladder is refused at assembly rather than noticed at registration.
    ///
    /// The price of the alternative is measured, not feared: `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` is
    /// the step space's own cap, four bisection rounds above the RC floor's own worst case
    /// (18 → 22), and nothing deeper than the cap is admissible at all.
    #[test]
    fn an_identity_whose_ladder_cannot_grow_is_refused() {
        let catalog = catalog();
        let mut short = bundle(&catalog);
        // Sized for exactly the class this genesis carries — correct for today, and a network that
        // can never take another class.
        short.court = PalwCourtParamsV2::new(catalog.max_step_leaf_count(), 4, 2).expect("a catalog-sized court is well-formed");
        let err = assemble_palw_rc_identity_v2(&base(), short, &catalog, &genesis_objects(), &ctx())
            .expect_err("a ladder that cannot grow is not an RC identity");
        assert!(
            matches!(err, PalwRcIdentityError::LadderNotProvisionedForTheStepSpace { want, .. }
                if want == crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT),
            "got {err:?}"
        );

        // And the gate is not merely "bigger is fine": the value is exact, because the ruleset id
        // is a hash and two networks with different ladders are different networks.
        let mut over = bundle(&catalog);
        over.court =
            PalwCourtParamsV2::new(crate::palw_class_admission_v2::PALW_RC_COURT_MAX_STEP_LEAF_COUNT + 1, 4, 2).expect("legal");
        assert!(
            assemble_palw_rc_identity_v2(&base(), over, &catalog, &genesis_objects(), &ctx()).is_err(),
            "a ladder past the step space is a ladder for classes that cannot exist"
        );
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

    /// **The shipped PALW-RC network assembles, boots, and runs its own genesis block.**
    ///
    /// `palw_rc_params` is the preset ADR-0042 PR-10 asks for, and this is the check that it is a
    /// network rather than a struct: the bundle validates, the cadence and the window inequality
    /// both hold (the two independent refusals that made a new identity necessary), the genesis
    /// artifact loads against its catalog, and the genesis block's registrations APPLY.
    #[test]
    fn the_shipped_palw_rc_network_boots_and_runs_its_genesis() {
        use crate::palw_state_v2::PalwBondKeyV2;
        use crate::tx::{TransactionId, TransactionOutpoint};

        let catalog = catalog();
        let bond = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0), index: 0 });
        let params = crate::config::params::palw_rc_params(
            h64(1),
            catalog.root(),
            h64(0xC0757),
            CANONICAL,
            h64(0xA7),
            bond,
            vec![7; 32],
            vec![21; 8],
            h64(0x9A11),
        )
        .expect("the RC network is a runnable ruleset");

        // A new identity, and both reasons it had to be one.
        assert_eq!(params.net.to_string(), "testnet-12");
        assert_eq!(params.blockrate.target_time_per_block, crate::palw_mode_v2::PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS);
        assert!(params.palw_credit.is_none(), "a V2 network installs no V1 fence");
        assert!(!params.pow_palw_activation.is_active(u64::MAX - 1), "and activates no V1 PALW PoW");

        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("not V2") };
        assert!(
            params.blockrate.finality_depth < bundle.state.window_challenge(),
            "finality_depth {} must be under w_challenge {} — the rule testnet's 10-BPS depth fails by orders of magnitude",
            params.blockrate.finality_depth,
            bundle.state.window_challenge()
        );

        // Its genesis is its OWN block: a different marker means a different merkle root means a
        // different hash from every other network's.
        assert_ne!(params.genesis.hash, crate::config::params::TESTNET11_PARAMS.genesis.hash);
        assert_ne!(params.genesis.hash, crate::config::params::MAINNET_PARAMS.genesis.hash);

        // The artifact loads against its catalog, and the genesis block's registrations apply —
        // a network whose genesis registers nothing boots and then cannot produce.
        verify_palw_genesis_v2(bundle, &catalog, &bundle.genesis_objects).expect("the genesis artifact loads");
        let point = PalwBlockContextV2 { block: params.genesis.hash, daa_score: params.genesis.daa_score, blue_score: 0, subsidy: 0 };
        let (state, _) = crate::palw_state_v2::apply_palw_transition_v2(
            &PalwChainStateV2::genesis(),
            &bundle.state,
            &point,
            &bundle.genesis_objects,
            None,
        )
        .expect("the genesis registrations apply");
        assert_eq!(state.class_share_permille(&h64(1)), Some(1000), "the liveness floor holds the whole table");
        assert!(state.bond(&bond).is_some(), "and there is a bond to execute under");
    }

    /// **The RC network derives from ONE operator fact, and boots.**
    ///
    /// `palw_rc_params` takes nine arguments; eight are derivable, and eight separate arguments
    /// are eight places a number could be chosen twice. `palw_rc_params_from_artifacts` derives
    /// them from BASE-0's own graph, from the leaf counter, and from this build's adjudication
    /// table, so the only inputs left are the ones no function can invent: the artifact root over
    /// the int8 weights and the pinned sin/cos table, and the operator's bond identities.
    #[test]
    fn the_rc_network_derives_from_its_artifact_root_alone() {
        use crate::palw_state_v2::PalwBondKeyV2;
        use crate::tx::{TransactionId, TransactionOutpoint};

        let artifact_root = h64(0xA7);
        let bond = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0), index: 0 });
        let params = crate::config::params::palw_rc_params_from_artifacts(
            artifact_root,
            bond,
            vec![7; 32],
            vec![21; 8],
            h64(0x9A11),
        )
        .expect("the RC network derives and validates");

        assert_eq!(params.net.to_string(), "testnet-12");
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("not V2") };

        // The class id IS its graph. A class is what it computes, so there is no label to pick.
        let (profile, catalog) = crate::palw_base0_profile::palw_rc_base0_registration_v1(artifact_root).unwrap();
        assert_eq!(bundle.base_class_id, profile.shape_profile_id(), "the class id is the graph's id");
        assert_eq!(bundle.class_catalog_root, catalog.root());
        assert_eq!(
            bundle.court_catalog_root,
            crate::palw_catalog_coverage::palw_court_catalog_root_v1(),
            "the court root commits to THIS BUILD's adjudicable set, not to a value someone typed"
        );

        // `pwu_per_inference` is the COUNTED canonical leaf count, and the genesis loader checks
        // exactly that — so the declaration and the measurement are one number.
        let entry = catalog.entries().first().unwrap();
        let PalwConsensusObjectV2::ClassRegistered { pwu_rule, class_id, artifact_root: registered, .. } =
            &bundle.genesis_objects[0]
        else {
            panic!("the first genesis object registers the class")
        };
        assert_eq!(*class_id, profile.shape_profile_id());
        assert_eq!(*registered, artifact_root);
        assert_eq!(*pwu_rule, PalwPwuRuleV2::DerivedV1 { pwu_per_inference: entry.canonical_step_leaf_count });
        verify_palw_genesis_v2(bundle, &catalog, &bundle.genesis_objects).expect("the genesis artifact loads");

        // And every kernel BASE-0 can reach is one this build adjudicates — the check a faithful
        // profile for the pinned float model fails, because no float quantized matmul is
        // catalogued at all.
        assert!(entry.reachable_kernels.is_subset(&crate::palw_step_refute::catalogued_kernel_ids_v1()));

        // Same artifact root, same network: the identity is a function of its inputs.
        let again =
            crate::config::params::palw_rc_params_from_artifacts(artifact_root, bond, vec![7; 32], vec![21; 8], h64(0x9A11))
                .unwrap();
        assert_eq!(again.consensus_params_id(), params.consensus_params_id());
        // A different artifact root is a different network, because the weights are part of what
        // the class IS.
        let other =
            crate::config::params::palw_rc_params_from_artifacts(h64(0xA8), bond, vec![7; 32], vec![21; 8], h64(0x9A11))
                .unwrap();
        assert_ne!(other.consensus_params_id(), params.consensus_params_id());
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
