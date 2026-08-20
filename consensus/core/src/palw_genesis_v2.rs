//! MISAKA PALW-RC genesis loading (ADR-0042 Decision 11, ADR-0045 Decision 1, audit H2).
//!
//! # What this module is for
//!
//! `PalwConsensusParamsV2::validate` runs on the bundle alone — it is what a node calls before it
//! opens a database, and it can only check facts the bundle carries. Three of the ruleset's boot
//! conditions are not among them, because each is a statement about the class CATALOG, and the
//! catalog is deliberately not in the bundle (the ruleset id must be identical on the RC and on
//! mainnet; a catalog in the bundle is exactly the thing someone would be tempted to differ).
//!
//! The catalog travels with the genesis artifact instead, and this is where the two meet:
//!
//! * the catalog is the one the ruleset committed to (`class_catalog_root` recomputes), so "these
//!   are the registered classes" is a hash rather than a claim;
//! * the court's asserted worst-case depth covers the catalog's real one, and every class's
//!   reachable kernels are ones this build adjudicates — both via
//!   [`PalwConsensusParamsV2::verify_against_catalog`];
//! * **every genesis registration agrees with the catalog about the class it registers**, which
//!   is this module's own addition and the reason it exists.
//!
//! # The number that was self-certifying
//!
//! ADR-0045 Decision 1 made an attempt's `pwu` a derivation rather than a bound:
//! `pwu = palw_pwu_v1(class_target, pwu_per_inference)`. That closed a real hole — under
//! `MaxPerAttempt` the class's weight was a collateral measure, not a work measure — but it moved
//! the whole of a class's price onto ONE registered number. `pwu_per_inference` is normatively
//! the counted step-leaf count of the class's canonical inference; nothing checked that it was.
//! An operator who registered a class declaring ten times its real step count would have every
//! block of that class weigh ten times what its work is worth, permanently, with no rule broken
//! anywhere: admission would check the claim against the declaration and find them equal.
//!
//! So the count is a CATALOG fact now ([`PalwClassCatalogEntryV2::canonical_step_leaf_count`]),
//! the registration DECLARES, and this loader demands they agree. The catalog root is hashed into
//! the ruleset id and the ruleset id into the genesis, so the number an operator would have to
//! lie about is one the chain already committed to — which is the same shape that turned
//! `max_step_leaf_count` from an assertion into a check.
//!
//! # Where this runs, and what it is not
//!
//! At genesis-artifact load, before the first transition. It is a boot gate, not a consensus rule:
//! it decides whether THIS NODE will run a network, and every node runs it against the same
//! committed root, so a node that disagrees refuses to start rather than forking. Nothing here
//! activates on any shipped preset — every one of them is `PalwConsensusMode::Disabled` or
//! `LegacyTn11`, and this module is only reachable from a `ConsensusV2` bundle.

use crate::Hash64;
use crate::palw_mode_v2::{PalwClassCatalogV2, PalwConsensusParamsV2};
use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum PalwGenesisV2Error {
    #[error("the ruleset refuses its own catalog: {0}")]
    Ruleset(String),
    #[error("genesis registers class {0} but the committed catalog does not contain it")]
    ClassNotInCatalog(Hash64),
    #[error("genesis registers class {class_id} with artifact root {declared}, but the catalog commits to {catalogued}")]
    ArtifactRootMismatch { class_id: Hash64, declared: Hash64, catalogued: Hash64 },
    #[error(
        "genesis registers class {class_id} declaring {declared} pwu per inference; the catalog counts {counted} \
         (ADR-0045 Decision 1: the declaration is not the fact)"
    )]
    PwuPerInferenceMismatch { class_id: Hash64, declared: u64, counted: u64 },
    #[error(
        "genesis registers class {0} under `MaxPerAttempt` — a value network may only register derived classes \
         (ADR-0042: the derivation is required before any class carries weight)"
    )]
    ClassIsNotDerived(Hash64),
    #[error("genesis registers no class at all — a network with no liveness floor cannot produce a block")]
    NoRegistrations,
    #[error("the first class genesis registers is {first}, not the bundle's liveness floor {base}")]
    FirstClassIsNotTheBase { first: Hash64, base: Hash64 },
    #[error("genesis registers class {0} twice")]
    DuplicateRegistration(Hash64),
}

/// Verify a `ConsensusV2` genesis artifact: the bundle, the catalog preimage behind its
/// `class_catalog_root`, and the consensus objects the genesis block registers.
///
/// `registrations` is the genesis block's object list in its own canonical order — the same slice
/// the first [`crate::palw_state_v2::apply_palw_transition_v2`] will fold. Non-`ClassRegistered`
/// objects are ignored here: bonds are checked by the transition (which is where collateral and
/// operator identity live), and this gate is about the class set alone.
///
/// Returns `Ok` only if the node may start. Every failure names the disagreement rather than a
/// position, because the operator fixing it is holding two artifacts and needs to know which one
/// is wrong.
pub fn verify_palw_genesis_v2(
    bundle: &PalwConsensusParamsV2,
    catalog: &PalwClassCatalogV2,
    registrations: &[PalwConsensusObjectV2],
) -> Result<(), PalwGenesisV2Error> {
    // Root, coverage, and the court's depth against the catalog's — the bundle's own gate, run
    // first so a catalog that is not even the committed one fails before anything is read out of
    // it. (Reading first and checking after is how a substituted catalog gets a vote.)
    bundle.verify_against_catalog(catalog).map_err(|e| PalwGenesisV2Error::Ruleset(e.to_string()))?;

    let mut registered: Vec<Hash64> = Vec::new();
    for object in registrations {
        let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, pwu_rule, .. } = object else {
            continue;
        };
        if registered.contains(class_id) {
            return Err(PalwGenesisV2Error::DuplicateRegistration(*class_id));
        }
        let entry = catalog.entry(class_id).ok_or(PalwGenesisV2Error::ClassNotInCatalog(*class_id))?;
        if *artifact_root != entry.artifact_root {
            return Err(PalwGenesisV2Error::ArtifactRootMismatch {
                class_id: *class_id,
                declared: *artifact_root,
                catalogued: entry.artifact_root,
            });
        }
        match pwu_rule {
            // A network that carries value registers only derived classes. `MaxPerAttempt` is
            // pre-derivation scaffolding — it survives for fixtures and for nets that weigh
            // nothing, and a genesis loader is exactly where "this net carries value" is decided.
            PalwPwuRuleV2::MaxPerAttempt(_) => return Err(PalwGenesisV2Error::ClassIsNotDerived(*class_id)),
            PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => {
                if *pwu_per_inference != entry.canonical_step_leaf_count {
                    return Err(PalwGenesisV2Error::PwuPerInferenceMismatch {
                        class_id: *class_id,
                        declared: *pwu_per_inference,
                        counted: entry.canonical_step_leaf_count,
                    });
                }
            }
        }
        registered.push(*class_id);
    }

    // The liveness floor is registered, and registered FIRST. The transition enforces this too
    // (`FirstClassMustBeTheBase`), and it is repeated here for a reason worth stating: a node that
    // would die on its own genesis block should say so while an operator is still looking at the
    // artifact, not at block 1 with a database already open.
    let first = *registered.first().ok_or(PalwGenesisV2Error::NoRegistrations)?;
    if first != bundle.base_class_id {
        return Err(PalwGenesisV2Error::FirstClassIsNotTheBase { first, base: bundle.base_class_id });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_mode_v2::PalwClassCatalogEntryV2;
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    const LEAVES: u64 = 1 << 16;
    const CANONICAL: u64 = 4_096;

    fn catalog_entry(class_id: Hash64) -> PalwClassCatalogEntryV2 {
        PalwClassCatalogEntryV2 {
            class_id,
            artifact_root: h64(0xA7),
            max_step_leaf_count: LEAVES,
            canonical_step_leaf_count: CANONICAL,
            reachable_kernels: crate::palw_step_refute::catalogued_kernel_ids_v1(),
        }
    }

    fn catalog() -> PalwClassCatalogV2 {
        PalwClassCatalogV2::new(vec![catalog_entry(h64(1))]).expect("the fixture catalog is well-formed")
    }

    /// A bundle whose `class_catalog_root` really is this catalog's, so the tests below fail on
    /// the clause under test rather than on the root.
    fn bundle(catalog: &PalwClassCatalogV2) -> PalwConsensusParamsV2 {
        let mut b = crate::palw_mode_v2::tests::conforming_bundle();
        b.base_class_id = h64(1);
        b.class_catalog_root = catalog.root();
        b.court = crate::palw_mode_v2::PalwCourtParamsV2::new(LEAVES, 4, 2).expect("a court that can walk the catalog");
        b
    }

    fn registration(class_id: Hash64, pwu_per_inference: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: h64(0xA7),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target: u128::MAX / 2,
            share_permille: 1000,
        }
    }

    fn a_bond() -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 }),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral: 100_000,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
        }
    }

    #[test]
    fn an_honest_genesis_artifact_loads() {
        let catalog = catalog();
        // Bonds ride the same object list and are none of this gate's business.
        let objects = vec![registration(h64(1), CANONICAL), a_bond()];
        verify_palw_genesis_v2(&bundle(&catalog), &catalog, &objects).expect("the honest artifact loads");
    }

    /// **The H2 / ADR-0045 Decision 1 defect, by name.** An overstated `pwu_per_inference` is a
    /// permanent multiplier on the class's fork-choice weight that breaks no other rule — every
    /// later check compares the claim to the declaration and finds them equal. The catalog is the
    /// only place the truth lives, and this is the only gate that asks.
    #[test]
    fn an_overstated_pwu_per_inference_is_refused_against_the_catalogs_count() {
        let catalog = catalog();
        let bundle = bundle(&catalog);

        for declared in [CANONICAL * 10, CANONICAL + 1, CANONICAL - 1, 1] {
            let err = verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(1), declared)]).unwrap_err();
            assert_eq!(
                err,
                PalwGenesisV2Error::PwuPerInferenceMismatch { class_id: h64(1), declared, counted: CANONICAL },
                "declaring {declared} against a counted {CANONICAL} must be refused"
            );
        }
        // …and the exact count loads, so the rule is equality and not a bound in either direction.
        verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(1), CANONICAL)]).expect("the counted value loads");
    }

    /// The catalog cannot be swapped for a friendlier one: its root is what the ruleset committed
    /// to, and the root is checked before any entry is read.
    #[test]
    fn a_substituted_catalog_is_refused_before_it_is_consulted() {
        let honest = catalog();
        let bundle = bundle(&honest);

        let mut generous_entry = catalog_entry(h64(1));
        generous_entry.canonical_step_leaf_count = CANONICAL * 10;
        let generous = PalwClassCatalogV2::new(vec![generous_entry]).expect("well-formed, just not the committed one");
        assert_ne!(generous.root(), honest.root(), "the fixture must actually differ");

        let err = verify_palw_genesis_v2(&bundle, &generous, &[registration(h64(1), CANONICAL * 10)]).unwrap_err();
        assert!(matches!(err, PalwGenesisV2Error::Ruleset(_)), "got {err:?}");
    }

    /// A class the catalog never registered, an artifact root that disagrees, and a
    /// pre-derivation rule — each refused, each by its own name.
    #[test]
    fn a_registration_that_disagrees_with_the_catalog_is_refused() {
        let catalog = catalog();
        let bundle = bundle(&catalog);

        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(9), CANONICAL)]).unwrap_err(),
            PalwGenesisV2Error::ClassNotInCatalog(h64(9))
        );

        let mut wrong_artifact = registration(h64(1), CANONICAL);
        if let PalwConsensusObjectV2::ClassRegistered { artifact_root, .. } = &mut wrong_artifact {
            *artifact_root = h64(0xBAD);
        }
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[wrong_artifact]).unwrap_err(),
            PalwGenesisV2Error::ArtifactRootMismatch { class_id: h64(1), declared: h64(0xBAD), catalogued: h64(0xA7) }
        );

        let mut undrived = registration(h64(1), CANONICAL);
        if let PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } = &mut undrived {
            *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(1_000_000);
        }
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[undrived]).unwrap_err(),
            PalwGenesisV2Error::ClassIsNotDerived(h64(1))
        );
    }

    /// The liveness floor must be there, and must be first — refused at load rather than at
    /// block 1, which is the difference between an operator reading an error and an operator
    /// reading a stalled node.
    #[test]
    fn the_liveness_floor_must_be_registered_first() {
        let catalog = PalwClassCatalogV2::new(vec![catalog_entry(h64(1)), catalog_entry(h64(2))]).expect("two classes");
        let bundle = bundle(&catalog);

        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[a_bond()]).unwrap_err(),
            PalwGenesisV2Error::NoRegistrations,
            "a network with no class cannot produce a block"
        );
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(2), CANONICAL), registration(h64(1), CANONICAL)])
                .unwrap_err(),
            PalwGenesisV2Error::FirstClassIsNotTheBase { first: h64(2), base: h64(1) }
        );
        verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(1), CANONICAL), registration(h64(2), CANONICAL)])
            .expect("base first, then the entrant");
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(1), CANONICAL), registration(h64(1), CANONICAL)])
                .unwrap_err(),
            PalwGenesisV2Error::DuplicateRegistration(h64(1))
        );
    }

    /// The catalog's own construction refuses a canonical count that outruns the worst case, so
    /// "priced deeper than the ladder can walk" is unrepresentable rather than merely unchecked.
    #[test]
    fn a_catalog_cannot_price_work_its_ladder_could_not_walk() {
        let mut deeper = catalog_entry(h64(1));
        deeper.canonical_step_leaf_count = LEAVES + 1;
        assert!(PalwClassCatalogV2::new(vec![deeper]).is_err(), "canonical deeper than worst case");

        let mut zero = catalog_entry(h64(1));
        zero.canonical_step_leaf_count = 0;
        assert!(PalwClassCatalogV2::new(vec![zero]).is_err(), "a zero-step canonical inference prices nothing as something");
    }
}
