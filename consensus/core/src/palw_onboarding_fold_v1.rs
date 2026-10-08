//! **The fold arms of the onboarding objects (tags 104–107)** — child module of `palw_state_v2`, like the kernel route's fold. See
//! [`crate::palw_onboarding_v1`] for the design. Rows live in the kernel route's aux tables 36–38 (written through the route's one
//! journaled writer), the reservation is mirrored into V2's committed-collateral ledger, and the closing tick releases a binding's
//! reservation when its liability horizon ends.
//!
//! A refusal is an `Err`: the acceptance walk's rehearsal drops the object and the block stands, as for every V2 object.

use super::palw_kernel_route_fold_v1::ensure_route_header;
use super::*;
use crate::palw_onboarding_v1::*;
use misaka_palw_challenge::ConformanceCommitmentV1;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

fn key2(a: &Hash64, b: &Hash64) -> Vec<u8> {
    borsh::to_vec(&(*a, *b)).expect("two digests serialize")
}

impl PalwChainStateV2 {
    /// What the onboarding bindings hold against `bond` (0 with no kernel route): V2's committed-collateral ledger and both
    /// withdrawal gates add it beside the kernel ledger's own reservation, so no V2 gate counts a binding's slice as free.
    pub fn onboarding_reserved(&self, bond: &PalwBondKeyV2) -> u128 {
        self.kernel_route.as_ref().map(|k| k.onboarding_reserved_v1(bond) as u128).unwrap_or(0)
    }
}

/// The V2 class `class` as a binding needs it: live (Registered or Active), an IR class, and `signer` its registrant.
fn registrant_class<'a>(
    builder: &'a TransitionBuilder<'_>,
    class: &Hash64,
    signer: &PalwBondKeyV2,
) -> Result<(&'a PalwClassStateV2, &'a crate::palw_tir_admission_v1::PalwTirClassRecordV1), PalwStateV2Error> {
    let state = builder.state.classes.get(class).ok_or_else(|| refused("no such V2 class"))?;
    if !matches!(state.status, PalwClassStatusV2::Registered { .. } | PalwClassStatusV2::Active) {
        return Err(refused("only a Registered or Active class can be bound"));
    }
    if state.registrant_bond != Some(*signer) {
        return Err(refused("only the class's registrant can speak for its artifact"));
    }
    let tir = builder
        .state
        .tir_class_v1(class)
        .ok_or_else(|| refused("only an IR class has an inventory a binding can be challenged against"))?;
    let active = builder.state.bonds.get(signer).is_some_and(|b| matches!(b.status, PalwBondStatusV2::Active));
    if !active {
        return Err(refused("the signer is no Active bond"));
    }
    Ok((state, tir))
}

fn route_of<'a>(builder: &'a TransitionBuilder<'_>) -> Result<&'a crate::palw_kernel_route_v1::PalwKernelRouteStateV1, PalwStateV2Error> {
    builder.state.kernel_route.as_ref().ok_or_else(|| refused("the kernel route has no state"))
}

/// **Tag 104.** See the module doc of [`crate::palw_onboarding_v1`].
pub(super) fn apply_artifact_bound_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    v2_class: &Hash64,
    kernel_param_root: &Hash64,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    registrant_class(builder, v2_class, signer)?;
    // One live binding per class: an artifact is one set of bytes, so two different kernel roots cannot both be true of it.
    let live = route_of(builder)?.artifact_bindings_of_v1(v2_class);
    if live.iter().any(|(root, row)| !row.refuted && root == kernel_param_root) {
        return Err(refused("this artifact binding already exists"));
    }
    if live.iter().any(|(_, row)| !row.refuted) {
        return Err(refused("the class is already bound to another kernel root, and is not refuted"));
    }
    // The statement is bonded: a slice of the signer's FREE collateral is held until the refutation horizon ends.
    let collateral = builder.state.bonds.get(signer).map(|b| b.collateral as u128).unwrap_or(0);
    let free = collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score));
    if free < PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as u128 {
        return Err(refused("the signer's free collateral does not cover the binding's reservation"));
    }
    let row = ArtifactBindingRowV1 {
        binder: *signer,
        bound_daa: ctx.daa_score,
        matures_daa: ctx.daa_score.saturating_add(PALW_ONBOARDING_BINDING_WINDOW_DAA_V1),
        final_daa: ctx.daa_score.saturating_add(PALW_ONBOARDING_BINDING_LIABILITY_DAA_V1),
        reserved: PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1,
        refuted: false,
    };
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1,
        key2(v2_class, kernel_param_root),
        Some(borsh::to_vec(&row).expect("a binding row serializes")),
    );
    Ok(())
}

/// **Tag 105.** A proven mismatch refutes the binding, slashes its reservation and pays the challenger its share.
pub(super) fn apply_artifact_challenged_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    v2_class: &Hash64,
    kernel_param_root: &Hash64,
    challenger: &PalwBondKeyV2,
    proof: &ArtifactMismatchProofV1,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    let row = route_of(builder)?
        .artifact_binding_v1(v2_class, kernel_param_root)
        .ok_or_else(|| refused("no such artifact binding"))?;
    if row.refuted {
        return Err(refused("the binding is already refuted"));
    }
    if ctx.daa_score >= row.final_daa {
        return Err(refused("the binding is past its refutation horizon"));
    }
    let (challenger_active, challenger_operator, payout) = match builder.state.bonds.get(challenger) {
        Some(b) => (matches!(b.status, PalwBondStatusV2::Active), b.operator_id, b.payout_payload),
        None => return Err(refused("the challenger is no bond")),
    };
    if !challenger_active {
        return Err(refused("the challenger is no Active bond"));
    }
    // A binder cannot refute its own statement for the bounty.
    if builder.state.bonds.get(&row.binder).is_some_and(|b| b.operator_id == challenger_operator) {
        return Err(refused("a binding is refuted by an operator other than its binder's"));
    }
    let class = builder.state.classes.get(v2_class).ok_or_else(|| refused("no such V2 class"))?;
    let tir = builder.state.tir_class_v1(v2_class).ok_or_else(|| refused("not an IR class"))?;
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(tir.program.as_slice())
        .map_err(|_| refused("the class's stored program does not decode"))?;
    verify_artifact_mismatch_v1(&program, class.artifact_root, *kernel_param_root, proof).map_err(refused)?;
    // The fraud is proven: the reservation is forfeit (the slash's burn at release takes its half), the rest pays the challenger.
    let slashed = builder.slash_bond(row.binder, row.reserved as u128)?;
    let reward = slashed.saturating_mul(PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1) / 1000;
    builder.add_kernel_payout(payout, reward)?;
    let refuted = ArtifactBindingRowV1 { reserved: 0, refuted: true, ..row };
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1,
        key2(v2_class, kernel_param_root),
        Some(borsh::to_vec(&refuted).expect("a binding row serializes")),
    );
    Ok(())
}

/// **Tag 106.**
pub(super) fn apply_kernel_bound_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    v2_class: &Hash64,
    kernel_class: &Hash64,
    challenge_policy_id: &Hash64,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    let (_class, tir) = registrant_class(builder, v2_class, signer)?;
    let route = route_of(builder)?;
    if route.kernel_binding_v1(v2_class).is_some() {
        return Err(refused("the class is already kernel-bound: a plan or program cannot be substituted afterwards"));
    }
    // The kernel class stands only because the route registered it, after PUBLIC_PROSECUTION_COMPLETE(plan, profile).
    let record = route
        .kernel_class_record_v1(kernel_class)
        .ok_or_else(|| refused("the route holds no such kernel class (its registration IS the PUBLIC_PROSECUTION_COMPLETE gate)"))?;
    // The same program: byte for byte the canonical program the V2 class registered.
    if record.program_bytes.as_slice() != tir.program.as_slice() {
        return Err(refused("the kernel class runs another program than the V2 class registered"));
    }
    // The same artifact: the kernel class's commitments root is the root of a live (unrefuted) binding of THIS class.
    let kernel_param_root = Hash64::from_bytes(record.param_commitments.root());
    match route.artifact_binding_v1(v2_class, &kernel_param_root) {
        Some(binding) if !binding.refuted => {}
        _ => return Err(refused("the kernel class's artifact is not bound to this V2 class (or its binding is refuted)")),
    }
    // The network's challenge policy, the one the route's ledger is configured under — a class cannot choose a lighter one.
    if challenge_policy_id.as_bytes() != route.header.policy.challenge_policy_id {
        return Err(refused("not the network's challenge policy"));
    }
    let row = KernelBindingRowV1 {
        kernel_class: *kernel_class,
        kernel_param_root,
        plan_root: Hash64::from_bytes(record.plan.root()),
        challenge_policy_id: *challenge_policy_id,
        binder: *signer,
        bound_daa: ctx.daa_score,
    };
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_KERNEL_BINDINGS_V1,
        borsh::to_vec(v2_class).expect("a digest serializes"),
        Some(borsh::to_vec(&row).expect("a binding row serializes")),
    );
    Ok(())
}

/// **Tag 107.** Every bound root and policy of the RFC-0013 statement must be the chain's own, so a commitment made under another
/// policy, plan, program, kernel or artifact is refused here and not discovered when the beacon locks.
pub(super) fn apply_conformance_committed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    commitment: &ConformanceCommitmentV1,
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    commitment.well_formed().map_err(refused)?;
    let class = Hash64::from_bytes(commitment.candidate_id);
    let (state, _tir) = registrant_class(builder, &class, signer)?;
    let route = route_of(builder)?;
    let binding = route.kernel_binding_v1(&class).ok_or_else(|| refused("bind the class to a kernel class before committing a conformance statement"))?;
    let record = route.kernel_class_record_v1(&binding.kernel_class).ok_or_else(|| refused("the bound kernel class is gone"))?;
    let extras = builder.extras.kernel_route.as_ref().ok_or_else(|| refused("the kernel route is not in force"))?;
    let checks: [(bool, &str); 8] = [
        (commitment.chain_genesis == extras.chain_genesis.as_bytes(), "another chain's genesis"),
        (commitment.ruleset_id == route.header.policy.ruleset_digest, "another ruleset"),
        (commitment.challenge_policy_id == binding.challenge_policy_id.as_bytes(), "not the challenge policy the class is bound to"),
        (commitment.artifact_root == state.artifact_root.as_bytes(), "not the class's registered artifact root"),
        (
            commitment.program_root == misaka_palw_kernel::public::program_root_v1(&record.program_bytes),
            "not the bound kernel class's program root",
        ),
        (commitment.verification_plan_root == binding.plan_root.as_bytes(), "not the bound kernel class's plan root"),
        (commitment.kernel_descriptor_id == record.descriptor, "not the bound kernel class's kernel descriptor"),
        (
            matches!(
                commitment.subject_kind,
                misaka_palw_challenge::SubjectKindV1::ModelConformance | misaka_palw_challenge::SubjectKindV1::KernelConformance
            ),
            "not a conformance subject",
        ),
    ];
    if let Some((_, why)) = checks.iter().find(|(ok, _)| !ok) {
        return Err(refused(format!("conformance commitment: {why}")));
    }
    // One commitment per (class, artifact root): a different artifact is a different class and needs its own; the same one cannot
    // be replaced by a statement that happens to suit a later beacon.
    if route.conformance_v1(&class, &state.artifact_root).is_some() {
        return Err(refused("a conformance commitment is already on chain for this class and artifact"));
    }
    let row = ConformanceRowV1 {
        statement_root: Hash64::from_bytes(commitment.statement_root()),
        signer: *signer,
        committed_daa: ctx.daa_score,
    };
    let artifact_root = state.artifact_root;
    builder.write_kernel_row(
        PALW_ONBOARDING_TABLE_CONFORMANCE_V1,
        key2(&class, &artifact_root),
        Some(borsh::to_vec(&row).expect("a conformance row serializes")),
    );
    Ok(())
}

/// **The closing step**: a binding whose refutation horizon has ended releases its reservation (the row stays: a Final binding is
/// the record the activation gate reads). Deterministic, in key order.
pub(super) fn tick_onboarding_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    let Some(route) = builder.state.kernel_route.as_ref() else { return };
    let due: Vec<(Vec<u8>, ArtifactBindingRowV1)> = route
        .aux
        .range((PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, Vec::new())..(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1 + 1, Vec::new()))
        .filter_map(|((_, key), row)| Some((key.clone(), borsh::from_slice::<ArtifactBindingRowV1>(row).ok()?)))
        .filter(|(_, row)| row.reserved > 0 && !row.refuted && ctx.daa_score >= row.final_daa)
        .collect();
    for (key, row) in due {
        let released = ArtifactBindingRowV1 { reserved: 0, ..row };
        builder.write_kernel_row(PALW_ONBOARDING_TABLE_ARTIFACT_BINDINGS_V1, key, Some(borsh::to_vec(&released).expect("a row serializes")));
    }
}
