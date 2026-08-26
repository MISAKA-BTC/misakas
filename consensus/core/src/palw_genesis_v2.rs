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

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
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
    #[error(
        "genesis bond {bond:?} can back {supported} concurrent claims but this ruleset holds a claim for {window_bind} DAA          before `BindTimeout` releases it — and on a ConsensusV2 network DAA only advances when blocks are produced, so          the chain stops after {supported} blocks and can never restart. Declare at least {needed_collateral} sompi"
    )]
    BondCannotSustainBindWindow {
        bond: crate::palw_state_v2::PalwBondKeyV2,
        supported: u128,
        window_bind: u64,
        needed_collateral: u128,
    },
    #[error(
        "genesis registers {operators} distinct bond operator(s); a {seat_count}-seat panel needs {needed} because a claim's          own executor is excluded from its panel and one seat is one operator. No claim could ever be licensed: every one          would void at `BindTimeout`, `safe_weight` would stay zero, and every block's escrowed worker carve would be burned"
    )]
    PanelCannotBeSeated { operators: usize, seat_count: u16, needed: usize },
    #[error(
        "genesis bond {bond:?} declares {declared} sompi of collateral, but its outpoint holds no genesis output at all \
         — a bond whose collateral nothing locks is a number, not a stake"
    )]
    BondOutpointIsNotAGenesisUtxo { bond: crate::palw_state_v2::PalwBondKeyV2, declared: u64 },
    #[error(
        "genesis bond {bond:?} declares {declared} sompi of collateral but its outpoint holds only {held} \
         — the chain cannot take what is not there"
    )]
    BondCollateralNotHeld { bond: crate::palw_state_v2::PalwBondKeyV2, declared: u64, held: u64 },
}

/// Verify a `ConsensusV2` genesis artifact: the bundle, the catalog preimage behind its
/// `class_catalog_root`, and the consensus objects the genesis block registers.
///
/// `registrations` is the genesis block's object list in its own canonical order — the same slice
/// the first [`crate::palw_state_v2::apply_palw_transition_v2`] will fold.
///
/// `genesis_output_value` resolves an outpoint against the network's OWN genesis UTXO set (for a
/// shipped network, `config::premine::genesis_premine_utxos_for`), returning the sompi it holds.
///
/// # Why a bond has to point at money (audit C-08, first half)
///
/// A `BondRegistered` object DECLARES `collateral`, and every ceiling, every slash and Decision
/// 7's whole Sybil bound is denominated in that number. Nothing anywhere locked a UTXO behind it:
/// the transition checked `collateral >= min_collateral_sompi`, which compares one declaration
/// against one constant, so a genesis artifact saying "this bond has staked a million" gave the
/// chain a bond with a million of nothing. Slashing it reduced a field. No coin moved, no
/// spendable balance shrank, and a liar paid what a liar had put up, which was zero.
///
/// The half that closes HERE is existence: a genesis bond's key is an outpoint (that is what
/// `PalwBondKeyV2` is), so the gate demands the outpoint be a real genesis output holding at
/// least the declared collateral. The declaration stops being self-certifying, exactly as
/// `pwu_per_inference` did one field above.
///
/// What this alone does NOT do — and the reason it is a half — is keep the money there: nothing
/// yet stops the owner spending that output in block 1, and a slash still moves no coin. Those
/// are the spend gate and the burn, and they are consensus rules rather than a boot gate.
/// Existence first, because a lock on an outpoint that holds nothing locks nothing.
///
/// Returns `Ok` only if the node may start. Every failure names the disagreement rather than a
/// position, because the operator fixing it is holding two artifacts and needs to know which one
/// is wrong.
pub fn verify_palw_genesis_v2(
    bundle: &PalwConsensusParamsV2,
    catalog: &PalwClassCatalogV2,
    registrations: &[PalwConsensusObjectV2],
    genesis_output_value: impl Fn(&crate::tx::TransactionOutpoint) -> Option<u64>,
) -> Result<(), PalwGenesisV2Error> {
    // Root, coverage, and the court's depth against the catalog's — the bundle's own gate, run
    // first so a catalog that is not even the committed one fails before anything is read out of
    // it. (Reading first and checking after is how a substituted catalog gets a vote.)
    bundle.verify_against_catalog(catalog).map_err(|e| PalwGenesisV2Error::Ruleset(e.to_string()))?;

    let mut registered: Vec<Hash64> = Vec::new();
    // Bonds are collected as they are walked so the two whole-registry gates below — can this
    // registry sustain its own bind window, and can it seat a panel — can be answered once the
    // walk has seen all of it. Both are properties of the SET, which is why neither could be
    // stated inside the per-object loop that used to be all there was here.
    let mut bonds: Vec<(crate::palw_state_v2::PalwBondKeyV2, u64, Hash64)> = Vec::new();
    for object in registrations {
        if let PalwConsensusObjectV2::BondRegistered { bond, collateral, operator_pubkey, .. } = object {
            let Some(held) = genesis_output_value(&bond.0) else {
                return Err(PalwGenesisV2Error::BondOutpointIsNotAGenesisUtxo { bond: *bond, declared: *collateral });
            };
            if held < *collateral {
                return Err(PalwGenesisV2Error::BondCollateralNotHeld { bond: *bond, declared: *collateral, held });
            }
            bonds.push((*bond, *collateral, crate::palw_state_v2::palw_operator_id_v2(operator_pubkey)));
            continue;
        }
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

    // ------------------------------------------------------------------------------------------
    // The two gates that decide whether this registry can run a chain at all.
    //
    // Everything above checks one object against the catalog. These check the SET against the
    // ruleset's own clock and its own panel, and they exist because a bundle can pass every
    // per-object check and still describe a network that stops at block two — which is what the
    // shipped RC bundle did, silently, with no test able to say so.
    // ------------------------------------------------------------------------------------------

    // **Gate 1 — the bind window.** A claim reserves `pwu × slash_value_per_pwu` against its bond
    // and holds it until `BindTimeout` at `accepted_daa + window_bind`. On a ConsensusV2 network
    // EVERY block is an attempt, so every block creates a claim, and DAA advances only when blocks
    // are produced. If a bond's ceiling admits fewer concurrent claims than the window is long,
    // the producer fills the ceiling and then needs DAA it can only get by producing — a deadlock
    // with no timeout, no operator action, and no error message. The chain simply stops.
    let base = registrations
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, slash_value_per_pwu, pwu_rule, initial_target, .. }
                if *class_id == bundle.base_class_id =>
            {
                Some((*slash_value_per_pwu, pwu_rule, *initial_target))
            }
            _ => None,
        })
        .ok_or(PalwGenesisV2Error::FirstClassIsNotTheBase { first, base: bundle.base_class_id })?;
    let (slash_value_per_pwu, pwu_rule, initial_target) = base;
    let pwu = match pwu_rule {
        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => crate::palw_pwu::palw_pwu_v1(initial_target, *pwu_per_inference),
        // A value network cannot register one of these (checked above); the arm keeps the match total.
        PalwPwuRuleV2::MaxPerAttempt(cap) => *cap,
    };
    let per_claim = (pwu as u128).saturating_mul(slash_value_per_pwu as u128).max(1);
    let window_bind = bundle.state.window_bind();
    let ratio = bundle.admission.max_exposure_ratio_permille() as u128;
    for (bond, collateral, _) in &bonds {
        let ceiling = (*collateral as u128).saturating_mul(ratio) / 1000;
        let supported = ceiling / per_claim;
        if supported < window_bind as u128 {
            // The collateral that WOULD carry the window, rounded up through the same permille.
            let needed_ceiling = per_claim.saturating_mul(window_bind as u128);
            let needed_collateral = needed_ceiling.saturating_mul(1000).div_ceil(ratio.max(1));
            return Err(PalwGenesisV2Error::BondCannotSustainBindWindow { bond: *bond, supported, window_bind, needed_collateral });
        }
    }

    // **Gate 2 — the panel.** `derive_panel_v2` excludes a claim's own executor by bond, by
    // operator and by key, and seats at most one bond per operator. So a `seat_count`-seat panel
    // needs `seat_count + 1` DISTINCT operators in the registry, and `BondRegistered` may not ride
    // a transaction (`palw_lifecycle_objects_v2`: "bonds come from genesis") — which makes this a
    // property of the genesis artifact and of nothing else. Below it, no claim is ever licensed:
    // every one voids at `BindTimeout`, `safe_weight` stays zero, and the escrowed worker carve of
    // every block is burned. The chain produces blocks and carries no weight, which is the one
    // failure mode a weight-bearing network must not be able to ship in.
    let mut operators: Vec<Hash64> = bonds.iter().map(|(_, _, op)| *op).collect();
    operators.sort();
    operators.dedup();
    let needed = bundle.panel.seat_count() as usize + 1;
    if operators.len() < needed {
        return Err(PalwGenesisV2Error::PanelCannotBeSeated {
            operators: operators.len(),
            seat_count: bundle.panel.seat_count(),
            needed,
        });
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
            court_cost: crate::palw_class_admission_v2::PalwCourtCostV1 { max_opening_bytes: 1, max_terminal_macs: 1, max_operand_count: 1 },
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
            activation_daa: 0,
            admission: None,
        }
    }

    /// Enough to carry the bind window, which the registry gate now requires — see
    /// `a_registry_that_cannot_outlast_its_own_bind_window_is_refused` for the arithmetic.
    const FIXTURE_COLLATERAL: u64 = 4_000_000;

    fn a_bond() -> PalwConsensusObjectV2 {
        bond_n(1)
    }

    /// The `n`-th fixture bond: its own outpoint, its own key, and its own OPERATOR — because a
    /// panel seats one bond per operator and excludes the executor's, so a registry of clones
    /// seats nobody however many of them there are.
    fn bond_n(n: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 }),
            pubkey: vec![n as u8; 4],
            operator_pubkey: vec![0x21, n as u8, 0, 0, 0, 0, 0, 0],
            collateral: FIXTURE_COLLATERAL,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            signature: Vec::new(),
        }
    }

    /// A registry big enough to seat the fixture bundle's panel — `seat_count + 1` operators,
    /// because a claim's own executor never sits on its own panel.
    fn a_seatable_registry(bundle: &PalwConsensusParamsV2) -> Vec<PalwConsensusObjectV2> {
        (1..=(bundle.panel.seat_count() as u64 + 1)).map(bond_n).collect()
    }

    /// The fixture's genesis UTXO set: every `bond_n` outpoint holds more than it declares.
    /// Everything else resolves to nothing, which is what a bond pointing at no money looks like.
    fn funded(outpoint: &TransactionOutpoint) -> Option<u64> {
        (1..=64u64)
            .any(|n| {
                let PalwConsensusObjectV2::BondRegistered { bond, .. } = bond_n(n) else { unreachable!() };
                *outpoint == bond.0
            })
            .then_some(1_000_000_000)
    }

    #[test]
    fn an_honest_genesis_artifact_loads() {
        let catalog = catalog();
        let b = bundle(&catalog);
        let mut objects = vec![registration(h64(1), CANONICAL)];
        objects.extend(a_seatable_registry(&b));
        verify_palw_genesis_v2(&b, &catalog, &objects, funded).expect("the honest artifact loads");
    }

    /// **Audit C-08, first half: a bond has to point at money that exists.**
    ///
    /// `collateral` is the denominator of every exposure ceiling, every slash and Decision 7's
    /// Sybil bound, and the only thing that had ever checked it was `collateral >=
    /// min_collateral_sompi` — one declaration against one constant. A genesis artifact could
    /// register a bond staking a million with nothing behind it, and slashing that bond reduced a
    /// field while no coin moved and no spendable balance shrank.
    ///
    /// The bond KEY is an outpoint, so the gate has something to ask: does that output exist in
    /// this network's genesis set, and does it hold what the bond says it holds.
    #[test]
    fn a_bond_must_name_a_genesis_output_that_holds_its_collateral() {
        let catalog = catalog();
        let bundle = bundle(&catalog);
        let PalwConsensusObjectV2::BondRegistered { bond, collateral, .. } = a_bond() else { unreachable!() };

        // An outpoint the genesis set does not contain at all — the "declares a million" case.
        let err = verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(1), CANONICAL), a_bond()], |_| None).unwrap_err();
        assert_eq!(err, PalwGenesisV2Error::BondOutpointIsNotAGenesisUtxo { bond, declared: collateral });

        // An outpoint that exists but holds less than the declaration.
        let short = collateral - 1;
        let err =
            verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(1), CANONICAL), a_bond()], |_| Some(short)).unwrap_err();
        assert_eq!(err, PalwGenesisV2Error::BondCollateralNotHeld { bond, declared: collateral, held: short });

        // Exactly the declaration is enough; more is fine (the surplus is simply not staked). The
        // registry has to be seatable for the load to get this far, so this is the whole artifact
        // rather than one bond — which is the point of the two registry gates below it.
        let mut objects = vec![registration(h64(1), CANONICAL)];
        objects.extend(a_seatable_registry(&bundle));
        for held in [collateral, collateral + 1] {
            verify_palw_genesis_v2(&bundle, &catalog, &objects, |_| Some(held))
                .unwrap_or_else(|e| panic!("holding {held} against a declared {collateral} must load, got {e}"));
        }
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
            let err = verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(1), declared)], funded).unwrap_err();
            assert_eq!(
                err,
                PalwGenesisV2Error::PwuPerInferenceMismatch { class_id: h64(1), declared, counted: CANONICAL },
                "declaring {declared} against a counted {CANONICAL} must be refused"
            );
        }
        // …and the exact count loads, so the rule is equality and not a bound in either direction.
        let mut ok_objects = vec![registration(h64(1), CANONICAL)];
        ok_objects.extend(a_seatable_registry(&bundle));
        verify_palw_genesis_v2(&bundle, &catalog, &ok_objects, funded).expect("the counted value loads");
    }

    /// **A registry that cannot outlast its own bind window is refused** — the defect that made
    /// the shipped RC bundle stop at block two.
    ///
    /// Every ConsensusV2 block is an attempt, so every block creates a claim that reserves
    /// `pwu × slash_value_per_pwu` until `BindTimeout` at `+window_bind`. DAA advances only when
    /// blocks are produced. A ceiling admitting fewer concurrent claims than the window is long is
    /// therefore a deadlock with no timeout and no message: the producer fills the ceiling and then
    /// needs DAA it can only get by producing.
    ///
    /// Measured on the SHIPPED numbers this gate would have caught: collateral 400,000 at 500‰ is a
    /// ceiling of 200,000; the floor's claim reserves 15,800 pwu × 5 = 79,000; 200,000 / 79,000 = 2
    /// concurrent claims against a 600-DAA window. Two blocks, then nothing, forever.
    #[test]
    fn a_registry_that_cannot_outlast_its_own_bind_window_is_refused() {
        let catalog = catalog();
        let bundle = bundle(&catalog);
        let window = bundle.state.window_bind();
        let ratio = bundle.admission.max_exposure_ratio_permille() as u128;
        // What one claim of the fixture floor reserves, computed the way admission computes it.
        let pwu = crate::palw_pwu::palw_pwu_v1(u128::MAX / 2, CANONICAL);
        let per_claim = (pwu as u128) * 5;
        let need = (per_claim * window as u128) * 1000 / ratio;

        // One sompi under the requirement is a chain that stops.
        let thin = |collateral: u64| {
            let mut objects = vec![registration(h64(1), CANONICAL)];
            for (i, o) in a_seatable_registry(&bundle).into_iter().enumerate() {
                let PalwConsensusObjectV2::BondRegistered { bond, pubkey, operator_pubkey, payout_payload, .. } = o else {
                    unreachable!()
                };
                // Only the first bond is thinned, so the failure names a bond rather than the set.
                let c = if i == 0 { collateral } else { FIXTURE_COLLATERAL };
                objects.push(PalwConsensusObjectV2::BondRegistered {
                    bond,
                    pubkey,
                    operator_pubkey,
                    collateral: c,
                    payout_payload,
                    signature: Vec::new(),
                });
            }
            verify_palw_genesis_v2(&bundle, &catalog, &objects, |_| Some(u64::MAX))
        };
        let err = thin((need as u64) - 1).unwrap_err();
        match err {
            PalwGenesisV2Error::BondCannotSustainBindWindow { supported, window_bind, needed_collateral, .. } => {
                assert!(supported < window as u128, "it must report FEWER claims than the window is long");
                assert_eq!(window_bind, window);
                assert_eq!(needed_collateral, need, "the message names the collateral that would carry the window");
            }
            other => panic!("a registry that stops the chain must be refused for that reason, got {other}"),
        }
        // Exactly the requirement loads — the gate is a bound, not a preference.
        thin(need as u64).expect("a bond sized to its own bind window is accepted");
    }

    /// **A registry that cannot seat a panel is refused** — the other half of the same artifact.
    ///
    /// `derive_panel_v2` excludes a claim's executor by bond, by operator AND by key, and seats one
    /// bond per operator. So `seat_count` seats need `seat_count + 1` distinct operators, and
    /// `BondRegistered` may not ride a transaction — the registry is fixed at genesis and there is
    /// no later repair. Below the bar every claim voids at `BindTimeout`: the chain makes blocks,
    /// `safe_weight` stays zero, and each block's escrowed worker carve is burned. A network that
    /// produces blocks and carries no weight is the one failure a weight-bearing chain must not be
    /// able to ship in — and nothing else in the tree could say so.
    #[test]
    fn a_registry_that_cannot_seat_a_panel_is_refused() {
        let catalog = catalog();
        let bundle = bundle(&catalog);
        let seats = bundle.panel.seat_count() as u64;

        // One operator short, every other check satisfied.
        let mut short = vec![registration(h64(1), CANONICAL)];
        short.extend((1..=seats).map(bond_n));
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &short, funded).unwrap_err(),
            PalwGenesisV2Error::PanelCannotBeSeated {
                operators: seats as usize,
                seat_count: seats as u16,
                needed: seats as usize + 1
            }
        );

        // **Clones do not count.** A registry of N bonds sharing ONE operator seats nobody, which
        // is the trap an operator bootstrapping alone walks into first.
        let mut clones = vec![registration(h64(1), CANONICAL)];
        for n in 1..=(seats + 1) {
            let PalwConsensusObjectV2::BondRegistered { bond, pubkey, collateral, payout_payload, .. } = bond_n(n) else {
                unreachable!()
            };
            clones.push(PalwConsensusObjectV2::BondRegistered {
                bond,
                pubkey,
                operator_pubkey: vec![21; 8],
                collateral,
                payout_payload,
                signature: Vec::new(),
            });
        }
        assert!(
            matches!(
                verify_palw_genesis_v2(&bundle, &catalog, &clones, funded).unwrap_err(),
                PalwGenesisV2Error::PanelCannotBeSeated { operators: 1, .. }
            ),
            "one operator with many bonds is one operator"
        );

        // And one more operator loads.
        let mut enough = vec![registration(h64(1), CANONICAL)];
        enough.extend(a_seatable_registry(&bundle));
        verify_palw_genesis_v2(&bundle, &catalog, &enough, funded).expect("seat_count + 1 operators seats a panel");
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

        let err = verify_palw_genesis_v2(&bundle, &generous, &[registration(h64(1), CANONICAL * 10)], funded).unwrap_err();
        assert!(matches!(err, PalwGenesisV2Error::Ruleset(_)), "got {err:?}");
    }

    /// A class the catalog never registered, an artifact root that disagrees, and a
    /// pre-derivation rule — each refused, each by its own name.
    #[test]
    fn a_registration_that_disagrees_with_the_catalog_is_refused() {
        let catalog = catalog();
        let bundle = bundle(&catalog);

        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(9), CANONICAL)], funded).unwrap_err(),
            PalwGenesisV2Error::ClassNotInCatalog(h64(9))
        );

        let mut wrong_artifact = registration(h64(1), CANONICAL);
        if let PalwConsensusObjectV2::ClassRegistered { artifact_root, .. } = &mut wrong_artifact {
            *artifact_root = h64(0xBAD);
        }
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[wrong_artifact], funded).unwrap_err(),
            PalwGenesisV2Error::ArtifactRootMismatch { class_id: h64(1), declared: h64(0xBAD), catalogued: h64(0xA7) }
        );

        let mut undrived = registration(h64(1), CANONICAL);
        if let PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } = &mut undrived {
            *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(1_000_000);
        }
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[undrived], funded).unwrap_err(),
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
            verify_palw_genesis_v2(&bundle, &catalog, &[a_bond()], funded).unwrap_err(),
            PalwGenesisV2Error::NoRegistrations,
            "a network with no class cannot produce a block"
        );
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(2), CANONICAL), registration(h64(1), CANONICAL)], funded)
                .unwrap_err(),
            PalwGenesisV2Error::FirstClassIsNotTheBase { first: h64(2), base: h64(1) }
        );
        let mut base_then_entrant = vec![registration(h64(1), CANONICAL), registration(h64(2), CANONICAL)];
        base_then_entrant.extend(a_seatable_registry(&bundle));
        verify_palw_genesis_v2(&bundle, &catalog, &base_then_entrant, funded).expect("base first, then the entrant");
        assert_eq!(
            verify_palw_genesis_v2(&bundle, &catalog, &[registration(h64(1), CANONICAL), registration(h64(1), CANONICAL)], funded)
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
