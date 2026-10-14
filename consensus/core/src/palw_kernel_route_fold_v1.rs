//! **G14 lane D: the kernel route in the fold** — dormant behind `Params::palw_probabilistic_constraints_v1`, which no network can
//! arm. A child module of `palw_state_v2`, as the vertex fold is, so it reads the builder and writes the state only through their
//! journaled writers. The ledger, the objects, the receipts and the settlement instructions are `misaka-palw-kernel`'s; the
//! messages, the interim policy and the state are [`crate::palw_kernel_route_v1`]'s.
//!
//! # One object, one transaction (eager)
//!
//! Every kernel object is applied by loading the ledger from the state's rows, running the kernel's own per-object API, applying the
//! settlement instructions it returned to the real bonds, and writing the rows that changed back — all in the arm of that object. A
//! kernel refusal (a rule, an over-size object, an unauthorized actor) DROPS the object: nothing is written and the block stands, as
//! every other dropped object does. The block's closing step ([`tick_kernel_route_v1`], after the objects) runs the kernel's `tick`
//! (demand deadlines, windows, Final, liability release) and applies its settlements the same way.
//!
//! # Bonds
//!
//! The kernel sees a bond only as a digest ([`palw_kernel_bond_id_v1`]); the state's bond-key table maps it back. Before an actor's
//! object is applied (and before the closing tick, for every bond the ledger holds) the ledger is told the bond's collateral **net of
//! every non-kernel reservation** — what V2's committed-collateral ledger ([`palw_bond_committed_v1`]) says the bond stands behind,
//! less the kernel's own reservation, which V2 reads back from the ledger's rows ([`PalwChainStateV2::kernel_reserved`]) — and never
//! below what the kernel has already reserved, so `kernel reserved ≤ synced collateral ≤ collateral`. Reserve and release
//! instructions need no write (V2 reads the ledger's own reservation); a slash is a real [`TransitionBuilder::slash_bond`] of exactly
//! the slashed amount (the synced collateral guarantees it never clamps, so the split to accuser, demanders and the burn conserves); an
//! accuser's reward, a demander's share and a Final reward are payouts in the coinbase queue under the kernel's payout prefix; a kernel
//! `Withdraw` is the route forgetting the bond, not a V2 exit. **GAP-5** (user-pays escrow): a Final reward is never new money — a
//! posted job's escrow is a reservation on its poster's bond, the job's first Final debits it (a real `slash_bond`, burned at release)
//! and pays the producer exactly that, and the fold pays a `FinalReward` only out of the `PayJobEscrow` debit before it in the same
//! batch: Σ kernel payouts ≤ Σ kernel debits, with no coinbase rule involved.
//!
//! # Seats (INTERIM)
//!
//! The Panel's coverage is not an object anybody submits. When a claim is committed the fold draws its seats — an integer exponential
//! race over the Active, distinct-operator V2 bonds that clear the producer collateral floor, seeded by the claim id alone: **INTERIM,
//! grindable, not a production beacon** — and a seat's signed receipt ([`PalwConsensusObjectV2::KernelConstraintReceiptV1`]) is the ONLY
//! source of `apply_panel_tally`. G14 does not rest on them: the public court convicts a claim every assigned seat covered.

use super::*;
use crate::palw_kernel_route_v1::*;
use misaka_palw_kernel::ledger::{ClaimBodyV1, KernelLedgerV1, LedgerEventV1};
use misaka_palw_kernel::receipt::{
    ClaimFactsV1, ConstraintTallyV1, PalwConstraintReceiptV1, ReceiptSignatureVerifier, SignedConstraintReceiptV1, TallyPolicyV1,
    TallyStateV1, admit_receipt_v1,
};
use misaka_palw_kernel::route::{AuthV1, KernelRouteObjectV1};
use misaka_palw_kernel::rows::{LedgerRowsV1, diff_rows};
use misaka_palw_kernel::settle::SettlementKindV1;
use misaka_palw_kernel::verify::ScopeV1;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.into())
}

fn key_bytes(d: &misaka_palw_kernel::hash::Digest) -> Vec<u8> {
    borsh::to_vec(d).expect("a digest serializes")
}

/// A source reserves income before its claim is revealed. The signed job and
/// seal are its immutable subject; a later reveal cannot choose another R row.
pub(super) fn source_income_subject_v1(job: &misaka_palw_kernel::hash::Digest, seal: &misaka_palw_kernel::hash::Digest) -> Hash64 {
    let mut hash = keyed(b"misaka-palw/bond-budget/source-subject/v1");
    hash.update(job);
    hash.update(seal);
    finish(hash)
}

/// The fold's signature check is the acceptance layer's (a pure transition holds no crypto): by the time a receipt reaches here its
/// signature has verified under the seat bond's registered key, so the kernel's structural admission is asked with this.
struct SignatureAlreadyVerified;
impl ReceiptSignatureVerifier for SignatureAlreadyVerified {
    fn verify(
        &self,
        _seat_bond: &misaka_palw_kernel::hash::Digest,
        _message: &misaka_palw_kernel::hash::Digest,
        _signature: &[u8],
    ) -> bool {
        true
    }
}

impl PalwChainStateV2 {
    /// The kernel route's state, once the fence's first block has folded.
    pub fn kernel_route(&self) -> Option<&PalwKernelRouteStateV1> {
        self.kernel_route.as_ref()
    }

    /// What the kernel route has reserved against `bond` (0 with no kernel route): V2's committed-collateral ledger adds it.
    pub fn kernel_reserved(&self, bond: &PalwBondKeyV2) -> u128 {
        self.kernel_route.as_ref().map(|k| k.reserved_of(bond) as u128).unwrap_or(0)
    }
}

impl TransitionBuilder<'_> {
    /// **The one writer of the kernel route's rows**, journaled `KernelRouteRow` (the ledger's tables below 32, the consensus tables
    /// from 32). The route's header must exist.
    pub(super) fn write_kernel_row(&mut self, table: u8, key: Vec<u8>, new: Option<Vec<u8>>) {
        let Some(kernel) = self.state.kernel_route.as_mut() else { return };
        let ledger_table = table < PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1;
        if ledger_table {
            // A ledger row changes: the cached ledger is no longer the rows' (the flush sets it again once it has written them).
            kernel.ledger_cache.clear();
        }
        let map = if ledger_table { &mut kernel.rows } else { &mut kernel.aux };
        let old = match &new {
            Some(bytes) => map.insert((table, key.clone()), bytes.clone()),
            None => map.remove(&(table, key.clone())),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::KernelRouteRow { table, key, old, new });
        }
    }

    /// **The one writer of the kernel route's header**, journaled `KernelRouteHeader`.
    fn write_kernel_header(&mut self, new: PalwKernelRouteHeaderV1) {
        let old = self.state.kernel_route.as_ref().map(|k| k.header.clone());
        if old.as_ref() == Some(&new) {
            return;
        }
        match self.state.kernel_route.as_mut() {
            Some(kernel) => {
                kernel.header = new.clone();
                kernel.ledger_cache.clear();
            }
            None => self.state.kernel_route = Some(PalwKernelRouteStateV1::with_header(new.clone())),
        }
        self.entries.push(PalwDeltaEntryV2::KernelRouteHeader { old, new: Some(new) });
    }

    /// A kernel payee's pay joins its one row in the coinbase queue.
    pub(super) fn add_kernel_payout(&mut self, payload: Hash64, amount: u64) -> Result<(), PalwStateV2Error> {
        if amount == 0 {
            return Ok(());
        }
        let key = palw_kernel_payout_key_v1(&payload);
        let held = self.state.pending_payouts.get(&key).map(|row| row.amount).unwrap_or(0);
        let total = held.checked_add(amount).ok_or(PalwStateV2Error::Overflow("kernel payout"))?;
        self.write_payout(key, Some(PalwPayoutV2 { payload, amount: total }));
        Ok(())
    }

    /// **The collateral the kernel is told a bond has**: the real collateral net of every non-kernel reservation, never below the
    /// kernel's own reservation and never above the collateral (module doc).
    fn kernel_synced_collateral(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u64 {
        let Some(record) = self.state.bonds.get(bond) else { return 0 };
        // The LEDGER's own reservation: a chunk group's deposit (also the route's, also in `committed`) is outside the ledger, so the
        // ledger is synced net of it like any other non-kernel reservation and never lends it to a claim.
        let kernel = self.state.kernel_route.as_ref().map(|k| k.ledger_reserved_of(bond) as u128).unwrap_or(0);
        let committed = self.committed_at(bond, now_daa);
        let non_kernel = committed.saturating_sub(kernel);
        let free = (record.collateral as u128).saturating_sub(non_kernel);
        u64::try_from(free.max(kernel).min(record.collateral as u128)).expect("bounded by a u64 collateral")
    }

    /// Record that the route has seen `bond` (its kernel digest maps back to it).
    pub(super) fn note_kernel_bond(&mut self, bond: &PalwBondKeyV2) -> misaka_palw_kernel::hash::Digest {
        let kid = palw_kernel_bond_id_v1(bond);
        self.write_kernel_row(
            PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1,
            key_bytes(&kid),
            Some(borsh::to_vec(bond).expect("a bond key serializes")),
        );
        kid
    }
}

/// The ledger policy and the OPV policy this block's rules put the route under (both genesis constants of the network).
#[allow(clippy::type_complexity)]
fn route_policies(
    builder: &TransitionBuilder<'_>,
) -> Result<(misaka_palw_kernel::ledger::LedgerPolicyV1, Option<misaka_palw_kernel::opv::OpvPolicyV1>, Option<u64>), PalwStateV2Error>
{
    let Some(extras) = builder.extras.kernel_route.as_ref() else {
        return Err(refused("palw_probabilistic_constraints_v1 is not in force at this block"));
    };
    let mut policy = palw_kernel_route_policy_v1(extras.network_domain, extras.ruleset_digest);
    if let Some(cap) = extras.max_adjudications_per_block {
        policy.max_adjudications_per_block = cap;
    }
    let opv_policy = extras.opv.as_ref().map(|o| o.policy);
    // A policy that violates the kernel's own relations never creates a ledger (Params validation refuses it at startup; this is the
    // fold's own guard, so a bad value fails the block loudly instead of panicking inside the ledger constructor).
    if let Some(p) = &opv_policy {
        p.validate(&policy).map_err(refused)?;
    }
    Ok((policy, opv_policy, extras.typed_roots))
}

/// **The route's header, created on first use** under the policies the processor resolved (and refused if the stored route was folded
/// under others). Every object that writes a row — a kernel object or an onboarding one — starts here, so the rows always have a
/// header to live under.
pub(super) fn ensure_route_header(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let (policy, opv_policy, typed_roots) = route_policies(builder)?;
    match builder.state.kernel_route.as_ref() {
        None => {
            let created = PalwKernelRouteStateV1::new_typed(
                policy,
                opv_policy,
                typed_roots,
                misaka_palw_kernel::rows::LedgerScalarsV1 { daa: ctx.daa_score, burned: 0 },
            );
            builder.write_kernel_header(created.header);
        }
        Some(kernel)
            if kernel.header.policy != policy || kernel.header.opv != opv_policy || kernel.header.typed_roots != typed_roots =>
        {
            return Err(refused("the stored kernel route was folded under another policy"));
        }
        Some(_) => {}
    }
    Ok(())
}

/// **The ledger the block's kernel moves run on**: the route's header created on first use (under the interim policy the processor
/// resolved), the ledger rebuilt from the rows, the block begun.
pub(super) fn load_ledger(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<KernelLedgerV1, PalwStateV2Error> {
    load_ledger_with_projection_v9(builder, ctx, None, true)
}

pub(super) struct NativeProjectionPrepaidV9(());
pub(super) fn paid_projection_v9() -> NativeProjectionPrepaidV9 {
    NativeProjectionPrepaidV9(())
}

/// Count only fixed authenticated keys/owner prefixes. No model bytes are read.
pub(super) fn projection_work_v9(builder: &TransitionBuilder<'_>) -> Result<u64, PalwStateV2Error> {
    use misaka_palw_kernel::{ledger::flat_candidate9::FLAT_CANDIDATE_REGISTRY_MAX_V9, rows::TABLE_FLAT_CONFORMANCE_CLASSES_V9};
    let count = builder
        .state
        .kernel_route
        .as_ref()
        .map(|k| {
            k.rows.range((TABLE_FLAT_CONFORMANCE_CLASSES_V9, Vec::new())..(TABLE_FLAT_CONFORMANCE_CLASSES_V9 + 1, Vec::new())).count()
        })
        .unwrap_or(0);
    if count > FLAT_CANDIDATE_REGISTRY_MAX_V9 {
        return Err(refused("v9 fixed-prefix registry cap"));
    }
    (count as u64).checked_mul(9216).ok_or_else(|| refused("v9 fixed-prefix work overflow"))
}

fn load_ledger_with_projection_v9(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    paid: Option<NativeProjectionPrepaidV9>,
    may_spend_reserve: bool,
) -> Result<KernelLedgerV1, PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    let scope_table = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_COMMITMENT_SCOPE_V1;
    let stored_scope = builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|route| route.aux.get(&(scope_table, Vec::new())))
        .map(|bytes| borsh::from_slice::<u64>(bytes).map_err(|_| refused("malformed authenticated court-scope activation")))
        .transpose()?;
    let configured_scope = builder.extras.kernel_route.as_ref().and_then(|extras| extras.commitment_scope);
    if stored_scope.is_some() && stored_scope != configured_scope {
        return Err(refused("the court-scope activation differs from the authenticated route"));
    }
    if let Some(at) = configured_scope {
        if at > ctx.daa_score {
            return Err(refused("the court-scope activation is not in force"));
        }
        if stored_scope.is_none() {
            builder.write_kernel_row(scope_table, Vec::new(), Some(borsh::to_vec(&at).expect("an activation serializes")));
        }
    }
    if paid.is_none() {
        let work = projection_work_v9(builder)?;
        if work != 0 {
            let policy = route_policies(builder)?.0;
            let (a, w) = builder
                .state
                .kernel_route
                .as_ref()
                .and_then(|k| k.aux_row::<(u64, u32, u64)>(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
                .filter(|(blue, _, _)| *blue == ctx.blue_score)
                .map(|(_, a, w)| (a, w))
                .unwrap_or_default();
            let limit = if may_spend_reserve { policy.max_court_work_per_block } else { policy.admission_work_limit_v1() };
            let next = w.checked_add(work).filter(|w| *w <= limit).ok_or_else(|| refused("v9 fixed-prefix block work cap"))?;
            builder.write_kernel_row(
                PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1,
                Vec::new(),
                Some(borsh::to_vec(&(ctx.blue_score, a, next)).expect("fixed prefix budget")),
            );
        }
    }
    let extras = builder.extras.kernel_route.as_ref().expect("checked by the header");
    // The artifact roots the route may attest: the test hook's list (empty outside a test) and every Matured or Final onboarding
    // binding — the least-trust source (`palw_onboarding_v1`).
    let mut attested = extras.attested_artifacts.clone();
    if let Some(route) = builder.state.kernel_route.as_ref() {
        attested.extend(route.onboarding_attested_roots_v1(ctx.daa_score));
    }
    let mut ledger = builder.state.kernel_route.as_ref().expect("created above").ledger_native_projection_v9().map_err(refused)?;
    ledger.begin_block(ctx.daa_score).map_err(|r| refused(r.to_string()))?;
    // The budget bounds the BLOCK: what the block's earlier objects already spent comes back (they were folded one at a time).
    if let Some((blue_score, adjudications, court_work)) =
        builder.state.kernel_route.as_ref().and_then(|k| k.aux_row::<(u64, u32, u64)>(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
        && blue_score == ctx.blue_score
    {
        ledger.restore_budget(misaka_palw_kernel::ledger::BlockBudgetV1 { adjudications, court_work });
    }
    // The attested set IS the derived set (not a growing one): a binding refuted after a root was attested stops attesting it for the
    // next kernel class, and the stale row goes with it.
    ledger.attested_artifacts = attested.iter().map(|root| root.as_bytes()).collect();
    // G14R round 3: `palw_verifier_pay_v1`'s terms (M*-49, O2), injected at every load like the attested set (not in the root).
    ledger.verifier_pay_policy = extras.verifier_pay.map(crate::palw_verifier_pay_v1::palw_verifier_pay_interim_policy_v1);
    // G14-S: the court scope (ADR-0177 D2's cumulative bound, `misaka_palw_kernel::court_scope`) is `palw_provider_court_v1`'s — injected
    // at every load like the verifier pay terms (not in the root; its table 28 is written only past it).
    ledger.court_scope_policy =
        extras.provider_court.map(|activation_daa| misaka_palw_kernel::court_scope::CourtScopePolicyV1 { activation_daa });
    // RFC-0002 P0 residual (RFC02-T): the sealed-bounty rule for kernel filings is `palw_model_refutation_seal_v1`'s — injected at every
    // load (not in the root; its table 19 is written only past it). The contest window is the route's seal TTL.
    let seal_ttl_daa = ledger.policy.seal_ttl_daa;
    ledger.sealed_bounty_policy = extras
        .model_refutation_seal
        .map(|activation_daa| misaka_palw_kernel::sealed_bounty::SealedBountyPolicyV1 { activation_daa, contest_daa: seal_ttl_daa });
    // DA16: the claims whose material obligation moved to bonded providers — derived from the court's rooted rows, never a ledger table
    // (and nothing at all below `palw_provider_court_v1`, where no such row can exist).
    if extras.provider_court.is_some() {
        ledger.provider_liable = builder
            .state
            .kernel_route
            .as_ref()
            .map(|k| k.provider_liable_claims_v1())
            .unwrap_or_default()
            .iter()
            .map(|c| c.as_bytes())
            .collect();
    }
    // Each load reconstructs authenticated open targets. No object supplies this inventory.
    if let Some(fence) = extras.beacon_source.filter(|f| f.activation.is_active(ctx.daa_score)) {
        let inputs =
            builder.state.kernel_route.as_ref().expect("created").beacon_source_inputs_v4(fence, ctx.daa_score).map_err(refused)?;
        ledger.set_beacon_source_inputs(inputs).map_err(refused)?;
    }
    if let Some(fence) = extras.beacon_source_fee.filter(|f| f.activation.is_active(ctx.daa_score)) {
        let inputs = builder
            .state
            .kernel_route
            .as_ref()
            .expect("created")
            .beacon_source_fee_inputs_v1(fence, &ledger.beacon_source_inputs, ctx.daa_score)
            .map_err(refused)?;
        ledger.set_beacon_source_fee_inputs(inputs).map_err(refused)?;
    }
    ledger.consumer_final_reward_caps = builder.kernel_final_reward_caps_v1(&ledger);
    // RFC-0015 admission is no longer a list read here: a class is admitted to the mode at its own registration, and only while
    // DERIVED-eligible (`opv_gate_v1`), so the ledger's admitted rows are exactly the classes that were eligible when they registered.
    Ok(ledger)
}

/// **The rows `load_ledger`'s ledger has, without serializing it** (GAP 8): the state's ledger rows with the two consumer-derived
/// tables the load sets (the attested artifacts and the admitted OPV classes) replaced by the ledger's own — exactly
/// `ledger.to_rows()` straight after `load_ledger`.
fn loaded_rows(builder: &TransitionBuilder<'_>, ledger: &KernelLedgerV1) -> LedgerRowsV1 {
    use misaka_palw_kernel::rows::{TABLE_ATTESTED_V1, TABLE_FLAT_CONFORMANCE_CLASSES_V9, TABLE_OPV_ADMITTED_V1};
    let mut rows: LedgerRowsV1 = builder
        .state
        .kernel_route
        .as_ref()
        .map(|k| {
            k.rows
                .iter()
                .filter(|((table, _), _)| {
                    !matches!(*table, TABLE_ATTESTED_V1 | TABLE_OPV_ADMITTED_V1 | TABLE_FLAT_CONFORMANCE_CLASSES_V9)
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();
    for root in &ledger.attested_artifacts {
        rows.insert((TABLE_ATTESTED_V1, key_bytes(root)), Vec::new());
    }
    for class in &ledger.opv.admitted {
        rows.insert((TABLE_OPV_ADMITTED_V1, key_bytes(class)), Vec::new());
    }
    debug_assert!(rows == ledger.to_rows(), "the loaded rows are the loaded ledger's");
    rows
}

/// What the derived-eligibility gate decides for one kernel object.
enum OpvGateV1 {
    /// Apply the object as it is.
    Pass,
    /// Admit these (mode-bound) ids to the mode first: the registration of an eligible class.
    Admit(Vec<misaka_palw_kernel::hash::Digest>),
    /// Drop the object, nothing written: an OPV registration or claim of a class that is not eligible at this block.
    Drop,
}

/// **The derived OPV eligibility at the kernel route's door** (`docs/design/palw/opv-beacon-bootstrap.md` §5.3): a registration under
/// `OptimisticPublicVerification` is admitted only for a class `opv_eligibility_v1` derives eligible now (or that the test seam names),
/// and every claim of an OPV class commits only while its class is eligible. A claim keeps what it was admitted with; a class that
/// loses eligibility (its binding refuted, its kernel retired, the network's deny-list) takes no new claim.
fn opv_gate_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    ledger: &KernelLedgerV1,
    object: &KernelRouteObjectV1,
) -> OpvGateV1 {
    use crate::palw_opv_bootstrap_v1::{OpvClassFactsV1, OpvEligibilityViewV1};
    let (Some(opv), Some(route)) =
        (builder.extras.kernel_route.as_ref().and_then(|e| e.opv.as_ref()), builder.state.kernel_route.as_ref())
    else {
        return OpvGateV1::Pass;
    };
    let view = OpvEligibilityViewV1::of(opv);
    let eligible =
        |class: &misaka_palw_kernel::hash::Digest| route.opv_class_eligibility_v1(ledger, class, ctx.daa_score, &view).is_ok();
    // A typed claim's class, through its job (an unknown job or class: the kernel refuses the claim by its own rule, so pass it on).
    let spec_claim_eligible = |claim: &misaka_palw_kernel::spec::SpecClaimV1| {
        use misaka_palw_kernel::spec::SpecJobV1 as J;
        let Some(job) = ledger.typed.jobs.get(&claim.job_id()) else { return true };
        let class = match job {
            J::Memory(j) => j.class,
            J::Retrieval(j) => j.class,
            J::Composite(j) => j.class,
        };
        let Some(row) = ledger.typed.classes.get(&class) else { return true };
        route.opv_spec_eligibility_v1(ledger, &row.spec, ctx.daa_score, &view).is_ok()
    };
    match object {
        KernelRouteObjectV1::RegisterClassV2 { mode, descriptor, program_bytes, plan, param_commitments } if mode.is_optimistic() => {
            let facts = OpvClassFactsV1::of_registration(*descriptor, program_bytes, plan, param_commitments);
            match route.opv_eligibility_v1(ledger, &facts, ctx.daa_score, &view) {
                Ok(_) => OpvGateV1::Admit(vec![facts.opv_id]),
                // Not admitted: the kernel refuses the registration by its own rule ("not admitted"), budget and all.
                Err(_) => OpvGateV1::Pass,
            }
        }
        KernelRouteObjectV1::RegisterPipelineClassV2 { mode, .. } if mode.is_optimistic() => {
            // No onboarding path exists for a pipeline (GAP-B4): only the test seam names one. Its ids are admitted ahead and the
            // kernel matches the registration's own id against them (a refused registration flushes nothing).
            OpvGateV1::Admit(view.test_eligible.iter().filter(|id| !view.denied.contains(id)).map(|id| id.as_bytes()).collect())
        }
        // RFC-0004 Part II (R4X): a typed-root class registers only under OPV, admitted by its derived eligibility (G14C GAP-50):
        // a `Weights` spec IS today's single-program registration; a `Memory` class's is its rule program's over its weights; a
        // `Composite` needs every stage's component eligible; a `Retrieval` class has no conformance statement yet (never eligible but
        // through the test seam, and never past the deny-list).
        KernelRouteObjectV1::Spec { object: misaka_palw_kernel::spec::SpecObjectV1::RegisterClass { spec } }
            if spec.mode.is_optimistic() =>
        {
            match (spec.class_id(), route.opv_spec_eligibility_v1(ledger, spec, ctx.daa_score, &view)) {
                (Ok(class), Ok(_)) => OpvGateV1::Admit(vec![class]),
                // Not admitted, or a malformed spec: the kernel refuses it by its own rule.
                _ => OpvGateV1::Pass,
            }
        }
        KernelRouteObjectV1::CommitClaim { claim, .. } => match ledger.jobs.get(&claim.job_id).map(|j| j.class_binding_id) {
            Some(class) if ledger.opv.classes.contains(&class) && !eligible(&class) => OpvGateV1::Drop,
            _ => OpvGateV1::Pass,
        },
        KernelRouteObjectV1::CommitPipelineClaim { claim, .. } => {
            match ledger.pipeline_jobs.get(&claim.job_id).map(|j| j.class_binding_id) {
                Some(class) if ledger.opv.classes.contains(&class) && !eligible(&class) => OpvGateV1::Drop,
                _ => OpvGateV1::Pass,
            }
        }
        // K2-TIR-v4/v5 (OPV-only): a segmented claim commits only while its class is eligible, as every other OPV claim.
        KernelRouteObjectV1::CommitSegmentedClaim { claim, .. } => {
            match ledger.tiled_jobs.get(&claim.job_id).map(|j| j.job.class_binding_id) {
                Some(class) if ledger.opv.classes.contains(&class) && !eligible(&class) => OpvGateV1::Drop,
                _ => OpvGateV1::Pass,
            }
        }
        // Past the fence every reveal is salted (G14-R4's claim seal v2, inner kind 20): the same gate on the carried commit. A typed
        // `Spec` claim passes here — its class's eligibility path is RFC-0004 Part II's (GAP-B16).
        KernelRouteObjectV1::CommitClaimSalted { commit, .. } => {
            use misaka_palw_kernel::route::SaltedCommitV1 as C;
            let class = match commit {
                C::Claim { claim, .. } => ledger.jobs.get(&claim.job_id).map(|j| j.class_binding_id),
                C::Pipeline { claim, .. } => ledger.pipeline_jobs.get(&claim.job_id).map(|j| j.class_binding_id),
                C::Segmented { claim, .. } => ledger.tiled_jobs.get(&claim.job_id).map(|j| j.job.class_binding_id),
                // A typed claim is re-gated by its class's DERIVED eligibility (G14C): a class whose component lost eligibility (its
                // binding refuted, its kernel retired, denied) takes no new claim.
                C::Spec { claim } => return if spec_claim_eligible(claim) { OpvGateV1::Pass } else { OpvGateV1::Drop },
            };
            match class {
                Some(class) if ledger.opv.classes.contains(&class) && !eligible(&class) => OpvGateV1::Drop,
                _ => OpvGateV1::Pass,
            }
        }
        // The unsalted typed reveal (kind 19, before salted seals are required) is gated the same way.
        KernelRouteObjectV1::Spec { object: misaka_palw_kernel::spec::SpecObjectV1::CommitClaim { claim } } => {
            if spec_claim_eligible(claim) { OpvGateV1::Pass } else { OpvGateV1::Drop }
        }
        _ => OpvGateV1::Pass,
    }
}

/// Write back every ledger row that changed and the scalars; `before` is the rows as `load_ledger` found them. The ledger is then
/// cached on the route state (GAP 8) as the rows' own ledger, so the next object of the chain loads it without rebuilding it.
pub(super) fn flush(builder: &mut TransitionBuilder<'_>, ledger: &KernelLedgerV1, before: &LedgerRowsV1) {
    for ((table, key), old_new) in diff_rows(before, &ledger.to_rows()).into_iter().map(|(k, _old, new)| (k, new)) {
        builder.write_kernel_row(table, key, old_new);
    }
    // Existing admitted v9 records were projected without copying their large
    // bytes. Preserve them exactly, except an authenticated withdrawal removed
    // their owner/class entry. Only this action's new record appears in diff.
    use misaka_palw_kernel::rows::TABLE_FLAT_CONFORMANCE_CLASSES_V9;
    let removed: Vec<Vec<u8>> = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .rows
        .range((TABLE_FLAT_CONFORMANCE_CLASSES_V9, Vec::new())..(TABLE_FLAT_CONFORMANCE_CLASSES_V9 + 1, Vec::new()))
        .filter_map(|((_, key), row)| {
            let class: misaka_palw_kernel::hash::Digest = borsh::from_slice(key).ok()?;
            let owner: misaka_palw_kernel::hash::Digest = row.get(..64)?.try_into().ok()?;
            (!ledger.native_candidate_owned_v9(&owner, &class)).then(|| key.clone())
        })
        .collect();
    for key in removed {
        builder.write_kernel_row(TABLE_FLAT_CONFORMANCE_CLASSES_V9, key, None);
    }
    let header = builder.state.kernel_route.as_ref().expect("a route that flushes exists").header.clone();
    let scalars = ledger.scalars();
    if header.scalars != scalars {
        builder.write_kernel_header(PalwKernelRouteHeaderV1 { scalars, ..header });
    }
    let kernel = builder.state.kernel_route.as_mut().expect("a route that flushes exists");
    let cached = ledger.clone().as_rebuilt_from_v1(&kernel.rows);
    if kernel.rows.keys().any(|(t, _)| *t == TABLE_FLAT_CONFORMANCE_CLASSES_V9) {
        kernel.ledger_cache.set_native_projection(cached);
    } else {
        kernel.ledger_cache.set(cached);
    }
}

/// Latest permitted root Final plus native evidence window and the complete kernel court path.
/// Checked additions refuse an impossible horizon before any consensus write.
pub(super) fn exec_liability_floor_v1(
    expiry: u64,
    native_window: u64,
    policy: &misaka_palw_kernel::ledger::LedgerPolicyV1,
) -> Result<u64, PalwStateV2Error> {
    crate::palw_exec_v2_verify::palw_exec_v2_retention_terms_v1(expiry, 0, native_window, policy)
        .map(|terms| terms.liability_until)
        .map_err(refused)
}

/// A transfer changes who owes material, not the term signed by its providers.
pub(super) fn require_exec_provider_horizon_v1(
    state: &PalwChainStateV2,
    claim: &Hash64,
    row: &misaka_palw_kernel::ledger::ClaimRowV1,
    daa: u64,
    liability_floor: u64,
) -> Result<(), &'static str> {
    use crate::palw_provider_court_v1::ProviderSubjectV1;
    let route = state.kernel_route.as_ref().ok_or("the EXEC kernel route is missing")?;
    let subject = ProviderSubjectV1::KernelClaim { claim: *claim };
    let Some(transfer) = route.provider_subject_v1(&subject).filter(|transfer| transfer.transferred_daa.is_some()) else {
        return Ok(());
    };
    if transfer.lapsed_daa.is_some() {
        return Err("a lapsed provider transfer cannot attach to EXEC");
    }
    let producer = route.bond_key_of(&row.producer).ok_or("the transferred EXEC producer has no bond mapping")?;
    let operator = state.bond(&producer).ok_or("the transferred EXEC producer bond is missing")?.operator_id;
    let until = liability_floor.max(row.liability_until.unwrap_or(0));
    let availability = state.provider_availability_v1(&subject, daa, until, Some(operator));
    if !availability.ready {
        return Err("the transferred EXEC claim needs two distinct non-producer operator leases through its consumer horizon");
    }
    if availability.reserved < row.reserved {
        return Err("the transferred EXEC provider leases reserve less than its claim");
    }
    Ok(())
}

/// A provider failure is the transferred claim's failure, including after computation Final.
/// Ordinary expiry after the collectible horizon is not a failure.
pub(super) fn exec_provider_lapsed_v1(state: &PalwChainStateV2, claim: &Hash64, row: &misaka_palw_kernel::ledger::ClaimRowV1) -> bool {
    use crate::palw_provider_court_v1::ProviderSubjectV1;
    state
        .kernel_route
        .as_ref()
        .and_then(|route| route.provider_subject_v1(&ProviderSubjectV1::KernelClaim { claim: *claim }))
        .is_some_and(|transfer| {
            transfer.transferred_daa.is_some()
                && transfer.lapsed_daa.is_some_and(|at| row.liability_until.is_none_or(|until| at <= until))
        })
}

/// Consumer hooks retain the current block budget; they do not begin a new kernel block.
pub(super) fn bind_exec_reward_v1(
    builder: &mut TransitionBuilder<'_>,
    root: Hash64,
    claim: Hash64,
    expiry: u64,
    daa: u64,
) -> Result<(), PalwStateV2Error> {
    let route = builder.state.kernel_route.as_ref().expect("EXEC kernel binding");
    let before = route.rows.clone();
    let mut ledger = route.ledger().map_err(refused)?;
    ledger.daa = daa;
    let until = exec_liability_floor_v1(expiry, builder.params.window_court(), &ledger.policy)?;
    let row = ledger.claims.get(&claim.as_bytes()).ok_or_else(|| refused("the EXEC reward claim is missing"))?;
    require_exec_provider_horizon_v1(&builder.state, &claim, row, daa, until).map_err(refused)?;
    let bond = route.bond_key_of(&row.producer).ok_or_else(|| refused("EXEC income recipient has no native bond"))?;
    ledger.bind_exec_reward_until(root.as_bytes(), claim.as_bytes(), expiry, until).map_err(refused)?;
    builder.transfer_kernel_income_to_exec_v1(root, claim, bond, daa, until)?;
    flush(builder, &ledger, &before);
    Ok(())
}

/// Pay only actual poster debits, inside each executor's remaining root share.
pub(super) fn finish_exec_rewards_v1(
    builder: &mut TransitionBuilder<'_>,
    root: Hash64,
    claims: &[(Hash64, PalwBondKeyV2)],
    caps: &std::collections::BTreeMap<PalwBondKeyV2, u64>,
    daa: u64,
) -> Result<Vec<u64>, PalwStateV2Error> {
    let route = builder.state.kernel_route.as_ref().expect("EXEC kernel binding");
    let before = route.rows.clone();
    let mut ledger = route.ledger().map_err(refused)?;
    ledger.daa = daa;
    let ids: Vec<_> = ledger.bonds.keys().copied().collect();
    for id in ids {
        if let Some(key) = builder.state.kernel_route.as_ref().and_then(|r| r.bond_key_of(&id)) {
            ledger.sync_bond(id, builder.kernel_synced_collateral(&key, daa));
        }
    }
    let mut events = Vec::new();
    let mut paid = Vec::new();
    let mut remaining = caps.clone();
    for (claim, bond) in claims {
        let cap = remaining.entry(*bond).or_default();
        let fee = ledger.finish_exec_reward(root.as_bytes(), claim.as_bytes(), *cap, &mut events).map_err(refused)?;
        *cap -= fee;
        paid.push(fee);
    }
    apply_settlements_to_exec_vesting(builder, &events, true, Some(root), daa)?;
    flush(builder, &ledger, &before);
    Ok(paid)
}

pub(super) fn cancel_exec_rewards_v1(builder: &mut TransitionBuilder<'_>, root: Hash64, daa: u64) -> Result<(), PalwStateV2Error> {
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let before = route.rows.clone();
    let mut ledger = route.ledger().map_err(refused)?;
    ledger.daa = daa;
    let claims: Vec<_> =
        ledger.exec_reward_holds.iter().filter(|(_, hold)| hold.owner_root == root.as_bytes()).map(|(id, _)| *id).collect();
    let mut events = Vec::new();
    for claim in claims {
        ledger.cancel_exec_reward(root.as_bytes(), claim, &mut events).map_err(refused)?;
    }
    apply_settlements(builder, &events, true, daa)?;
    flush(builder, &ledger, &before);
    Ok(())
}

/// **Apply a ledger's settlement instructions to the real bonds** (module doc).
///
/// `strict` (every object arm): a slash the real bond cannot pay in full refuses the object (it is dropped, the block stands). The closing
/// tick is not strict: it runs after the rehearsal, so an error there would fail the whole block — and with every held bond re-synced
/// just before it, the ledger's clamped instructions are always payable; if that ever failed, taking what the bond holds is the safe side.
pub(super) fn apply_settlements(
    builder: &mut TransitionBuilder<'_>,
    events: &[LedgerEventV1],
    strict: bool,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    apply_settlements_to_exec_vesting(builder, events, strict, None, now)
}

fn apply_settlements_to_exec_vesting(
    builder: &mut TransitionBuilder<'_>,
    events: &[LedgerEventV1],
    strict: bool,
    exec_root: Option<Hash64>,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    // GAP-5: what this batch actually debited from posters' escrows — the only source a Final reward is paid from. A `FinalReward`
    // with no spent escrow behind it is never paid (there is no unfunded reward path).
    let mut escrow_debited: std::collections::BTreeMap<Option<misaka_palw_kernel::hash::Digest>, u64> = Default::default();
    // Persistent source escrows were debited in an earlier fold. Their backing is the rooted funding row, never a later
    // sponsor balance or this batch's job escrow. All fee instructions are strict, including sweeps.
    let fee_ledger = if events.iter().any(|e| {
        matches!(e, LedgerEventV1::Settlement(s) if matches!(s.kind,
        SettlementKindV1::FundSourceFeeEscrow | SettlementKindV1::PaySourceFeeEscrow | SettlementKindV1::RefundSourceFeeEscrow))
    }) {
        Some(builder.state.kernel_route.as_ref().ok_or_else(|| refused("source fee route missing"))?.ledger().map_err(refused)?)
    } else {
        None
    };
    let mut fee_held: std::collections::BTreeMap<_, _> =
        fee_ledger.as_ref().map(|l| l.beacon_source_fees.iter().map(|(c, f)| (*c, f.held)).collect()).unwrap_or_default();
    // The old ledger root and this exact ordered settlement batch identify
    // objective court receipts independently of payout payloads or a retry.
    // The event ordinal also separates several demanders/defaults in one tick.
    let reporter_batch = if builder.params.reporter_income_active_at(now) {
        let mut hash = keyed(b"misaka-palw/bond-budget/kernel-reporter-batch/v1");
        let route = builder.state.kernel_route.as_ref().ok_or_else(|| refused("reporter route missing"))?;
        hash.update(route.ledger_root().as_byte_slice());
        hash.update(&now.to_le_bytes());
        let settlements: Vec<_> =
            events.iter().filter_map(|event| if let LedgerEventV1::Settlement(s) = event { Some(*s) } else { None }).collect();
        hash.update(&borsh::to_vec(&settlements).expect("settlements serialize"));
        Some(finish(hash))
    } else {
        None
    };
    let mut reporter_funded: std::collections::BTreeMap<_, u64> = Default::default();
    let mut fee_paid = std::collections::BTreeSet::new();
    for (ordinal, event) in events.iter().enumerate() {
        let LedgerEventV1::Settlement(s) = event else { continue };
        let key = builder.state.kernel_route.as_ref().and_then(|k| k.bond_key_of(&s.bond));
        match s.kind {
            SettlementKindV1::FundSourceFeeEscrow => {
                let context = s.claim.ok_or_else(|| refused("source fee funding context missing"))?;
                if s.amount == 0 || fee_held.contains_key(&context) {
                    return Err(refused("zero or duplicate source fee escrow funding"));
                }
                let key = key.ok_or_else(|| refused("source fee sponsor bond missing"))?;
                let debit = builder.slash_bond(key, u128::from(s.amount))?;
                if debit != s.amount {
                    return Err(refused("source fee escrow not fully prepaid"));
                }
                fee_held.insert(context, debit);
            }
            SettlementKindV1::PaySourceFeeEscrow | SettlementKindV1::RefundSourceFeeEscrow => {
                let context = s.claim.ok_or_else(|| refused("source fee payout context missing"))?;
                let l = fee_ledger.as_ref().expect("loaded for fee settlements");
                let funding = l.beacon_source_fees.get(&context).ok_or_else(|| refused("source fee payout not prepaid"))?;
                if s.kind == SettlementKindV1::RefundSourceFeeEscrow {
                    if s.bond != funding.terms.sponsor {
                        return Err(refused("source fee refund recipient substituted"));
                    }
                } else {
                    let promise =
                        l.beacon_source_fee_promises.get(&(context, s.bond)).ok_or_else(|| refused("source fee promise missing"))?;
                    if promise.outcome.is_some() || promise.fee != s.amount || !fee_paid.insert((context, s.bond)) {
                        return Err(refused("source fee receipt differs from the once-only promise"));
                    }
                }
                let held = fee_held.get_mut(&context).ok_or_else(|| refused("source fee escrow missing"))?;
                *held = held.checked_sub(s.amount).ok_or_else(|| refused("source fee payout exceeds prepaid escrow"))?;
                let payload = key
                    .and_then(|k| builder.state.bonds.get(&k).map(|b| b.payout_payload))
                    .ok_or_else(|| refused("source fee payee missing"))?;
                builder.add_kernel_payout(payload, s.amount)?;
            }
            SettlementKindV1::SlashFraud
            | SettlementKindV1::SlashDefault
            | SettlementKindV1::SlashFiling
            | SettlementKindV1::AdmissionFee
            | SettlementKindV1::JobFee
            | SettlementKindV1::PayJobEscrow
            | SettlementKindV1::ForfeitSealDeposit
            | SettlementKindV1::ForfeitDemandBond => {
                let Some(key) = key else {
                    if strict {
                        return Err(refused("a slash names a bond the route never saw"));
                    }
                    continue;
                };
                let debit = match builder.slash_bond(key, s.amount as u128) {
                    Ok(debit) => debit,
                    Err(e) if strict => return Err(e),
                    Err(_) => continue,
                };
                if debit != s.amount && strict {
                    return Err(refused(format!("a slash of {} took {debit}: the synced collateral did not cover it", s.amount)));
                }
                if matches!(s.kind, SettlementKindV1::SlashFraud | SettlementKindV1::SlashDefault) {
                    let funded = reporter_funded.entry(s.claim).or_default();
                    *funded = funded.checked_add(debit).ok_or(PalwStateV2Error::Overflow("reporter collateral debit"))?;
                }
                if s.kind == SettlementKindV1::PayJobEscrow {
                    let funded = escrow_debited.entry(s.claim).or_default();
                    *funded = funded.checked_add(debit).ok_or(PalwStateV2Error::Overflow("kernel escrow debit"))?;
                }
            }
            // ADR-0176 HOOK `budget-final` on the node (lane BUDGET): a payout is paid only inside the R / F reservation the claim
            // made at acceptance — this arm is where the consumer re-checks it, beside the escrow check below.
            SettlementKindV1::AccuserReward | SettlementKindV1::DemanderShare | SettlementKindV1::FinalReward => {
                let amount = if s.kind == SettlementKindV1::FinalReward {
                    let debited = escrow_debited.entry(s.claim).or_default();
                    let funded = s.amount.min(*debited);
                    *debited -= funded;
                    if funded != s.amount && strict {
                        return Err(refused("a Final reward with no spent escrow behind it (GAP-5: nothing is issued)"));
                    }
                    funded
                } else {
                    s.amount
                };
                if matches!(s.kind, SettlementKindV1::AccuserReward | SettlementKindV1::DemanderShare)
                    && let Some(batch) = reporter_batch
                {
                    // No recipient cap refusal is allowed to erase an objective
                    // conviction/default. Preserve the whole funded entitlement
                    // and let pre-object ticks redeem exact R-limited tranches.
                    let claim = s.claim.ok_or_else(|| refused("reporter award has no terminal court subject"))?;
                    let terminal = events.iter().any(|event| match event {
                        LedgerEventV1::Convicted { claim: proved, accuser, accuser_reward, .. }
                            if s.kind == SettlementKindV1::AccuserReward =>
                        {
                            *proved == claim && *accuser == s.bond && *accuser_reward == amount
                        }
                        LedgerEventV1::ProducerDefault { claim: defaulted, .. }
                        | LedgerEventV1::PostFinalDefault { claim: defaulted, .. }
                            if s.kind == SettlementKindV1::DemanderShare =>
                        {
                            *defaulted == claim
                        }
                        _ => false,
                    });
                    if !terminal {
                        return Err(refused("reporter award has no objective terminal verdict receipt"));
                    }
                    let funded = reporter_funded.entry(s.claim).or_default();
                    *funded = funded.checked_sub(amount).ok_or_else(|| refused("reporter award lacks collected collateral"))?;
                    let bond = key.ok_or_else(|| refused("reporter award recipient has no native bond"))?;
                    let mut hash = keyed(b"misaka-palw/bond-budget/kernel-reporter-award/v1");
                    hash.update(batch.as_byte_slice());
                    hash.update(&(ordinal as u64).to_le_bytes());
                    builder.stage_reporter_recipient_v1(finish(hash), bond, now, amount, Hash64::from_bytes(claim))?;
                    continue;
                }
                if s.kind == SettlementKindV1::FinalReward
                    && let Some(root) = exec_root
                {
                    let bond = key.ok_or_else(|| refused("an EXEC fee names an unknown bond"))?;
                    super::palw_exec_v2_rewards_v1::add(
                        builder,
                        root,
                        bond,
                        crate::palw_work_slice_v2::PalwExecV2RewardFundV1::KernelFee,
                        amount,
                    )?;
                    continue;
                }
                if s.kind == SettlementKindV1::FinalReward {
                    let claim = s.claim.ok_or_else(|| refused("a Final fee has no claim"))?;
                    let bond = key.ok_or_else(|| refused("a Final fee recipient has no native bond"))?;
                    let payload =
                        builder.state.bonds.get(&bond).ok_or_else(|| refused("a Final fee recipient is missing"))?.payout_payload;
                    builder.consume_kernel_final_reward_v1(&claim, bond, payload, now, amount)?;
                }
                let payee = key.and_then(|key| builder.state.bonds.get(&key).map(|b| b.payout_payload));
                match payee {
                    Some(payload) => match builder.add_kernel_payout(payload, amount) {
                        Ok(()) => {}
                        Err(e) if strict => return Err(e),
                        Err(_) => {}
                    },
                    None if strict => return Err(refused("a payout names a bond the route never saw")),
                    None => {}
                }
            }
            // The reservation is the ledger's row (V2 reads it back), the burn is the slash that preceded it, and a withdrawal is
            // the route forgetting the bond.
            SettlementKindV1::ReserveClaim
            | SettlementKindV1::ReleaseClaim
            | SettlementKindV1::ReserveDemand
            | SettlementKindV1::ReleaseDemand
            | SettlementKindV1::ReserveJobEscrow
            | SettlementKindV1::ReleaseJobEscrow
            | SettlementKindV1::ReserveSealDeposit
            | SettlementKindV1::ReleaseSealDeposit
            | SettlementKindV1::Burn
            | SettlementKindV1::Withdraw => {}
        }
    }
    Ok(())
}

/// **The interim seat draw of a freshly committed claim** (module doc). Distinct operators, none the producer's.
fn draw_seats(
    builder: &TransitionBuilder<'_>,
    claim: &misaka_palw_kernel::hash::Digest,
    producer: &PalwBondKeyV2,
    deadline_daa: u64,
) -> KernelAssignmentV1 {
    let seed = palw_kernel_interim_seed_v1(claim);
    let producer_operator = builder.state.bonds.get(producer).map(|b| b.operator_id);
    let floor = builder.params.min_collateral_sompi();
    // One entry per operator: its best (smallest) key over its eligible bonds.
    let mut best: BTreeMap<Hash64, (u128, PalwBondKeyV2)> = BTreeMap::new();
    for (key, bond) in builder.state.bonds.iter() {
        if key == producer || !matches!(bond.status, PalwBondStatusV2::Active) || bond.collateral < floor {
            continue;
        }
        if Some(bond.operator_id) == producer_operator {
            continue;
        }
        let weight = ((bond.collateral / crate::constants::SOMPI_PER_KASPA).clamp(1, 1_000_000)) as u128;
        let k = palw_kernel_interim_key_v1(&seed, &bond.operator_id, weight);
        let entry = best.entry(bond.operator_id).or_insert((k, *key));
        if (k, *key) < *entry {
            *entry = (k, *key);
        }
    }
    let mut ranked: Vec<(u128, PalwBondKeyV2)> = best.into_values().collect();
    ranked.sort();
    let seats: Vec<KernelSeatV1> = ranked
        .into_iter()
        .take(PALW_KERNEL_ROUTE_INTERIM_SEATS_V1)
        .map(|(_, bond)| KernelSeatV1 { bond, kernel_bond: palw_kernel_bond_id_v1(&bond), scope: ScopeV1::WholeClaim })
        .collect();
    let quorum = PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1.min(seats.len() as u8);
    let (challenge_anchor, sample_seed) = palw_kernel_interim_challenge_v1(claim);
    KernelAssignmentV1 {
        assignment_root: palw_kernel_assignment_root_v1(&seats, quorum),
        seats,
        challenge_anchor,
        sample_seed,
        deadline_daa,
        quorum,
    }
}

/// **Tag 110: one kernel route object.** See the module doc.
pub(super) fn apply_kernel_route_object_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    bytes: &[u8],
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    if builder.extras.kernel_route.is_none() {
        return Err(refused("palw_probabilistic_constraints_v1 is not in force at this block"));
    }
    // Funded actions 25..=40 below Final27 (native_final absent) are opaque: validation did not check
    // their signature, so they must have ZERO state effect — no bond, budget, row or fee read/write.
    // Mirrors `palw_native_action_unjudged_at_v1` in the processor exactly.
    if builder.extras.kernel_route.as_ref().is_some_and(|e| e.native_final.is_none())
        && bytes.get(1).copied() == Some(misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11)
        && bytes.get(2).is_some_and(|a| (25..=40).contains(a))
    {
        return Ok(());
    }
    // A malformed or over-size encoding, or a signer that is no Active bond, is a dropped object.
    if bytes.len() > PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 {
        return Ok(());
    }
    if !builder.state.bonds.get(signer).is_some_and(|b| matches!(b.status, PalwBondStatusV2::Active)) {
        return Ok(());
    }
    match misaka_palw_kernel::route::borrowed_native_action_v1(bytes) {
        Ok(Some(a)) if a.tag==misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11 && matches!(a.action,41..=44)=>return super::palw_native_initial_fold_v31::apply(builder,ctx,signer,a.action,a.body),
        Ok(Some(a)) if a.tag==misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11=>return super::palw_flat_execution_fold_v11::apply(builder,ctx,signer,a.action,a.body),
        Ok(Some(a)) if a.tag==misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14=>return super::palw_flat_stage_fold_v14::apply(builder,ctx,signer,a.action,a.body),
        Err(_)=>return Ok(()),
        _=>{}
    }
    let Ok(object) = KernelRouteObjectV1::decode(bytes) else { return Ok(()) };
    // RFC-0002 P0 (RFC02-T): past `palw_model_refutation_seal_v1` a candidate refutation is only a sealed reveal (inner 37); the frozen
    // unsealed refutations pay whoever is included first — an equivalent re-encoding of an honest finder's proof — so they are refused
    // by name before any court, fee or row.
    if super::palw_model_refutation_seal_fold_v4::sealed_refutations_in_force_v4(builder)
        && matches!(
            object,
            KernelRouteObjectV1::RefuteFlatModelVectorV3 { .. }
                | KernelRouteObjectV1::RefuteModelVectorV2 { .. }
                | KernelRouteObjectV1::RefuteModelArtifactV2 { .. }
        )
    {
        return Err(refused(
            "an unsealed candidate refutation is refused past palw_model_refutation_seal_v1: seal (inner 36), then reveal (inner 37)",
        ));
    }
    // Inner 22/23 are consensus-owned state/collateral courts, never bare-ledger moves.
    match &object {
        KernelRouteObjectV1::FundedExecutionV11 { action, body } => {
            return super::palw_flat_execution_fold_v11::apply(builder, ctx, signer, *action, body);
        }
        KernelRouteObjectV1::StageExecutionV14 { action, body } => {
            return super::palw_flat_stage_fold_v14::apply(builder, ctx, signer, *action, body);
        }
        KernelRouteObjectV1::SealModelRefutationV4 { slot, seal } => {
            return super::palw_model_refutation_seal_fold_v4::seal(builder, ctx, signer, *slot, seal);
        }
        KernelRouteObjectV1::RevealModelRefutationV4 { slot, kind, target, salt, proof } => {
            return super::palw_model_refutation_seal_fold_v4::reveal(builder, ctx, signer, *slot, *kind, target, salt, proof);
        }
        KernelRouteObjectV1::PostFlatModelVectorV9 { class, binding, post } => {
            return super::palw_flat_model_vector_fold_v9::post(builder, ctx, signer, class, binding, post);
        }
        KernelRouteObjectV1::SealFlatModelProofV9 { vector, seal } => {
            return super::palw_flat_model_vector_fold_v9::post_seal(builder, ctx, signer, vector, seal);
        }
        KernelRouteObjectV1::RevealFlatModelProofV9 { vector, salt, proof } => {
            return super::palw_flat_model_vector_fold_v9::reveal(builder, ctx, signer, vector, salt, proof);
        }
        KernelRouteObjectV1::ExpireFlatModelSealV9 { vector, actor } => {
            return super::palw_flat_model_vector_fold_v9::expire(builder, ctx, signer, vector, actor);
        }
        KernelRouteObjectV1::DemandFlatModelUnitV9 { vector, slot, position, occurrence, node, kind, index } => {
            use crate::palw_flat_model_vector_da_v9::{FlatVectorUnitKindV9 as Kind, FlatVectorUnitV9};
            let kind = match kind {
                0 if *index == 0 => Kind::Node,
                1 => Kind::Hash(*index),
                2 => Kind::Value(*index),
                3 if *index == 0 => Kind::Header,
                _ => return Err(refused("unknown v9 public unit")),
            };
            return super::palw_flat_model_vector_da_fold_v9::demand(
                builder,
                ctx,
                signer,
                vector,
                *slot,
                FlatVectorUnitV9 { position: *position, occurrence: *occurrence, node: *node, kind },
            );
        }
        KernelRouteObjectV1::AnswerFlatModelUnitV9 { demand, answer } => {
            return super::palw_flat_model_vector_da_fold_v9::answer(builder, ctx, signer, demand, answer);
        }
        KernelRouteObjectV1::SettleFlatModelDemandV9 { demand } => {
            return super::palw_flat_model_vector_da_fold_v9::settle_demand(builder, ctx, signer, demand);
        }
        KernelRouteObjectV1::PostFlatModelVectorV7 { class, binding, post } => {
            return super::palw_flat_model_vector_fold_v7::post(builder, ctx, signer, class, binding, post);
        }
        KernelRouteObjectV1::SealFlatModelProofV7 { vector, seal } => {
            return super::palw_flat_model_vector_fold_v7::post_seal(builder, ctx, signer, vector, seal);
        }
        KernelRouteObjectV1::RevealFlatModelProofV7 { vector, salt, proof } => {
            return super::palw_flat_model_vector_fold_v7::reveal(builder, ctx, signer, vector, salt, proof);
        }
        KernelRouteObjectV1::ExpireFlatModelSealV7 { vector, actor } => {
            return super::palw_flat_model_vector_fold_v7::expire(builder, ctx, signer, vector, actor);
        }
        KernelRouteObjectV1::DemandFlatModelUnitV7 { vector, slot, position, occurrence, node, kind, index } => {
            use crate::palw_flat_model_vector_da_v7::{FlatVectorUnitKindV7 as Kind, FlatVectorUnitV7};
            let kind = match kind {
                0 if *index == 0 => Kind::Node,
                1 => Kind::Hash(*index),
                2 => Kind::Value(*index),
                _ => return Err(refused("unknown v7 public unit")),
            };
            return super::palw_flat_model_vector_da_fold_v7::demand(
                builder,
                ctx,
                signer,
                vector,
                *slot,
                FlatVectorUnitV7 { position: *position, occurrence: *occurrence, node: *node, kind },
            );
        }
        KernelRouteObjectV1::AnswerFlatModelUnitV7 { demand, answer } => {
            return super::palw_flat_model_vector_da_fold_v7::answer(builder, ctx, signer, demand, answer);
        }
        KernelRouteObjectV1::SettleFlatModelDemandV7 { demand } => {
            return super::palw_flat_model_vector_da_fold_v7::settle_demand(builder, ctx, signer, demand);
        }
        KernelRouteObjectV1::PostFlatModelVectorV6 { class, binding, post } => {
            return super::palw_flat_model_vector_fold_v3::post_protected(builder, ctx, signer, class, binding, post);
        }
        KernelRouteObjectV1::SealFlatModelProofV6 { vector, seal } => {
            return super::palw_flat_model_seal_fold_v6::post_seal(builder, ctx, signer, vector, seal);
        }
        KernelRouteObjectV1::RevealFlatModelProofV6 { vector, salt, proof } => {
            return super::palw_flat_model_seal_fold_v6::reveal(builder, ctx, signer, vector, salt, proof);
        }
        KernelRouteObjectV1::ExpireFlatModelSealV6 { vector, actor } => {
            return super::palw_flat_model_seal_fold_v6::expire(builder, ctx, signer, vector, actor);
        }
        KernelRouteObjectV1::DemandFlatModelVectorV3 { vector, slot, position, occurrence, node, leaf } => {
            let unit = crate::palw_flat_model_vector_da_v3::FlatVectorUnitV3 {
                position: *position,
                occurrence: *occurrence,
                node: *node,
                leaf: *leaf,
            };
            return super::palw_flat_model_vector_fold_v3::demand(builder, ctx, signer, vector, *slot, unit);
        }
        KernelRouteObjectV1::AnswerFlatModelVectorV3 { demand, answer } => {
            return super::palw_flat_model_vector_fold_v3::answer(builder, ctx, signer, demand, answer);
        }
        KernelRouteObjectV1::SettleFlatModelVectorDemandV3 { demand } => {
            return super::palw_flat_model_vector_fold_v3::settle_demand(builder, ctx, signer, demand);
        }
        KernelRouteObjectV1::PostFlatModelVectorV3 { class, binding, post } => {
            return super::palw_flat_model_vector_fold_v3::post(builder, ctx, signer, class, binding, post);
        }
        KernelRouteObjectV1::RefuteFlatModelVectorV3 { vector, proof } => {
            return super::palw_flat_model_vector_fold_v3::refute(builder, ctx, signer, vector, proof);
        }
        KernelRouteObjectV1::DemandModelVectorV2 { vector, slot, position, occurrence, node, axis, line, tile } => {
            let unit = crate::palw_model_vector_da_v2::ModelVectorUnitV2 {
                position: *position,
                occurrence: *occurrence,
                node: *node,
                axis: *axis,
                line: *line,
                tile: *tile,
            };
            return super::palw_model_vector_fold_v2::demand(builder, ctx, signer, vector, *slot, unit);
        }
        KernelRouteObjectV1::AnswerModelVectorV2 { demand, answer } => {
            return super::palw_model_vector_fold_v2::answer(builder, ctx, signer, demand, answer);
        }
        KernelRouteObjectV1::SettleModelVectorDemandV2 { demand } => {
            return super::palw_model_vector_fold_v2::settle_demand(builder, ctx, signer, demand);
        }
        KernelRouteObjectV1::PostModelVectorV2 { class, binding, post } => {
            return super::palw_model_vector_fold_v2::post(builder, ctx, signer, class, binding, post);
        }
        KernelRouteObjectV1::RefuteModelVectorV2 { vector, proof } => {
            return super::palw_model_vector_fold_v2::refute(builder, ctx, signer, vector, proof);
        }
        KernelRouteObjectV1::CompleteModelConformanceV2 { class, binding, proof } => {
            return super::palw_model_conformance_fold_v2::complete(builder, ctx, signer, class, binding, proof);
        }
        KernelRouteObjectV1::BindModelArtifactV2 { descriptor, program_bytes, model_inventory_root, param_commitments } => {
            return super::palw_model_artifact_fold_v2::bind(
                builder,
                ctx,
                signer,
                descriptor,
                program_bytes,
                model_inventory_root,
                param_commitments,
            );
        }
        KernelRouteObjectV1::RefuteModelArtifactV2 { binding, proof } => {
            return super::palw_model_artifact_fold_v2::refute(builder, ctx, signer, binding, proof);
        }
        _ => {}
    }
    let mut projection_paid = None;
    let scoped = if matches!(object, KernelRouteObjectV1::RegisterFlatModelCandidateV9 { .. }) {
        let Some((o, id, paid)) = super::palw_model_artifact_fold_v2::flat_candidate_v9(builder, ctx, signer, &object)? else {
            return Ok(());
        };
        projection_paid = Some(paid);
        Some((o, id))
    } else if matches!(object, KernelRouteObjectV1::RegisterFlatModelCandidateV7 { .. }) {
        let Some(scoped) = super::palw_model_artifact_fold_v2::flat_candidate_v7(builder, ctx, signer, &object)? else {
            return Ok(());
        };
        Some(scoped)
    } else {
        super::palw_model_artifact_fold_v2::candidate(builder, ctx, &object)?
    };
    let proof = matches!(object, KernelRouteObjectV1::FileProof { .. });
    let mut ledger = load_ledger_with_projection_v9(builder, ctx, projection_paid, proof)?;
    let original_attested = ledger.attested_artifacts.clone();
    if let Some((_, id)) = &scoped {
        let row = builder.state.kernel_route.as_ref().unwrap().model_artifact_binding_header_v2(id).unwrap();
        ledger.attest_artifact(row.kernel_param_root.as_bytes());
    }
    let object = scoped.as_ref().map(|(o, _)| o).unwrap_or(&object);
    if let KernelRouteObjectV1::RegisterClassV2 { mode, descriptor, program_bytes, plan, param_commitments } = &object
        && mode.is_optimistic()
        && let (Some(route), Some(opv)) =
            (builder.state.kernel_route.as_ref(), builder.extras.kernel_route.as_ref().and_then(|e| e.opv.as_ref()))
    {
        let facts =
            crate::palw_opv_bootstrap_v1::OpvClassFactsV1::of_registration(*descriptor, program_bytes, plan, param_commitments);
        if route
            .model_conformance_eligibility_v2(
                &ledger,
                &facts,
                ctx.daa_score,
                &crate::palw_opv_bootstrap_v1::OpvEligibilityViewV1::of(opv),
            )
            .is_ok()
        {
            ledger.attest_artifact(facts.param_root);
        }
    }
    if matches!(object, KernelRouteObjectV1::Withdraw { .. })
        && candidate_withdraw_held_v9(builder, ctx, &ledger, &palw_kernel_bond_id_v1(signer))?
    {
        return Ok(());
    }
    let before = loaded_rows(builder, &ledger);
    match opv_gate_v1(builder, ctx, &ledger, object) {
        OpvGateV1::Drop => return Ok(()),
        OpvGateV1::Admit(classes) => {
            for class in classes {
                ledger.admit_optimistic_class(class).map_err(|r| refused(r.to_string()))?;
            }
        }
        OpvGateV1::Pass => {}
    }
    let kid = palw_kernel_bond_id_v1(signer);
    ledger.sync_bond(kid, builder.kernel_synced_collateral(signer, ctx.daa_score));
    // The other bond a slash can land on is the producer of the claim the object names: its collateral is brought up to date too, so
    // an earlier object of this block (or another lane's slash) can never leave the ledger believing the bond holds more than it does.
    if let KernelRouteObjectV1::FileProof { claim, .. }
    | KernelRouteObjectV1::FileDemand { claim, .. }
    | KernelRouteObjectV1::Respond { claim, .. } = &object
        && let Some(producer) = ledger.claims.get(claim).map(|row| row.producer)
        && let Some(key) = builder.state.kernel_route.as_ref().and_then(|k| k.bond_key_of(&producer))
    {
        ledger.sync_bond(producer, builder.kernel_synced_collateral(&key, ctx.daa_score));
    }
    // DA16 / ADR-0177 D2: past `palw_provider_court_v1` a demand names a claim-specific unit or nothing, within its requester's tally.
    let scope = match &object {
        KernelRouteObjectV1::FileDemand { claim, stage, position, .. }
            if builder.extras.kernel_route.as_ref().is_some_and(|k| k.provider_court.is_some()) =>
        {
            match super::palw_provider_court_fold_v1::court_scope_admit_demand_v1(builder, &ledger, signer, claim, *stage, *position) {
                Ok(scope) => scope,
                Err(_) => return Ok(()),
            }
        }
        _ => None,
    };
    let events = match ledger.apply_object(object, &AuthV1 { signer_bond: kid }) {
        Ok(events) => events,
        Err(_refusal) => {
            // A refusal that did court work still spent the block's budget (so junk cannot be tried for free); nothing else is kept.
            persist_budget(builder, ctx, &ledger);
            return Ok(());
        }
    };
    // A class registers only if its worst filing, response and commitments can actually be carried (chunking counted).
    for event in &events {
        if let LedgerEventV1::FlatConformanceClassRegisteredV9 { class } = event {
            let route = builder.state.kernel_route.as_ref().unwrap();
            use misaka_palw_kernel::{
                ledger::flat_candidate9::FLAT_CANDIDATE_REGISTRY_MAX_V9, rows::TABLE_FLAT_CONFORMANCE_CLASSES_V9,
            };
            if route.rows.contains_key(&(TABLE_FLAT_CONFORMANCE_CLASSES_V9, borsh::to_vec(class).unwrap()))
                || route.rows.keys().filter(|(table, _)| *table == TABLE_FLAT_CONFORMANCE_CLASSES_V9).count()
                    >= FLAT_CANDIDATE_REGISTRY_MAX_V9
            {
                persist_budget(builder, ctx, &ledger);
                return Ok(());
            }

            let fits = ledger.flat_conformance_classes_v9.get(class).is_some_and(|(_, r)| {
                // The candidate was just independently derived by this paid
                // attempt; fixed generated metadata supplies its carrier cap.
                misaka_palw_kernel::ledger::flat_candidate9::decode_scope_v9(&r.scope_bytes)
                    .is_ok_and(|s| s.worst_court_bytes as usize + 134 <= PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1)
            });
            if !fits {
                persist_budget(builder, ctx, &ledger);
                return Ok(());
            }
        }
        if let LedgerEventV1::FlatConformanceClassRegisteredV7 { class } = event {
            let fits = ledger
                .flat_conformance_classes_v7
                .get(class)
                .and_then(|(_, r)| r.checked().ok())
                .is_some_and(|v| v.scope.worst_court_bytes as usize + 134 <= PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1);
            if !fits {
                persist_budget(builder, ctx, &ledger);
                return Ok(());
            }
        }
        if let LedgerEventV1::ClassRegistered { class } | LedgerEventV1::ConformanceClassRegistered { class } = event {
            // Every kind's bounds: single program, pipeline, typed (RFC-0004 Part II).
            let bounds = ledger.bounds_of(class).or_else(|| ledger.conformance_classes.get(class).map(|(_, row)| row.bounds));
            let fits = bounds.is_some_and(|b| {
                misaka_palw_kernel::ledger::carrier_fit_v1(
                    &b,
                    PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1,
                    PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1,
                    PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1,
                )
                .is_ok()
            });
            if !fits {
                persist_budget(builder, ctx, &ledger);
                return Ok(());
            }
        }
    }
    // A paid opt-in reserves its fixed signed fee for the actual source bond
    // before admission changes any native money or source rows. Roll back even
    // initial budget creation if the recipient has no R room.
    if let KernelRouteObjectV1::OptInPaidBeaconSource { context, producer, .. } = &object {
        let promise =
            ledger.beacon_source_fee_promises.get(&(*context, *producer)).ok_or_else(|| refused("paid source promise missing"))?;
        let source = ledger.beacon_sources.get(&(*context, *producer)).ok_or_else(|| refused("paid source row missing"))?;
        if *producer != kid || promise.accepted_daa != ctx.daa_score {
            return Err(refused("paid source recipient or acceptance position differs"));
        }
        let checkpoint = builder.checkpoint();
        if builder
            .reserve_source_income_v1(
                ctx,
                Hash64::from_bytes(*context),
                source_income_subject_v1(&source.job, &source.seal),
                *signer,
                source.terms.hold_until,
                promise.fee,
            )
            .is_err()
        {
            builder.restore(checkpoint);
            persist_budget(builder, ctx, &ledger);
            return Ok(());
        }
    }
    // Reserve the shared Q/R window before any accepted claim/fee/bond write.
    // A refused plan still pays the public verification budget it already used.
    for event in &events {
        if let LedgerEventV1::ClaimCommitted { claim } = event {
            let row = ledger.claims.get(claim).ok_or_else(|| refused("accepted kernel claim is missing"))?;
            let reward = ledger.job_escrows.get(&row.job_id).map_or(0, |escrow| escrow.amount);
            match builder.plan_kernel_claim_budget_v1(ctx, *signer, Hash64::from_bytes(row.class_binding_id), reward) {
                Ok(Some(plan)) => {
                    builder.commit_bond_budget_v1(ctx, super::palw_bond_budget_fold_v1::kernel_budget_claim_id_v1(claim), plan)?;
                    builder.hold_kernel_income_v1(ctx, Hash64::from_bytes(*claim), row.liability_until.unwrap_or(ctx.daa_score))?;
                }
                Ok(None) => {}
                Err(_) => {
                    persist_budget(builder, ctx, &ledger);
                    return Ok(());
                }
            }
        }
    }
    if let Some((_, binding)) = &scoped {
        for event in &events {
            if let LedgerEventV1::ConformanceClassRegistered { class }
            | LedgerEventV1::FlatConformanceClassRegisteredV7 { class }
            | LedgerEventV1::FlatConformanceClassRegisteredV9 { class } = event
            {
                builder.write_kernel_row(
                    crate::palw_model_artifact_v2::PALW_MODEL_ARTIFACT_CANDIDATE_TABLE_V2,
                    borsh::to_vec(&Hash64::from_bytes(*class)).unwrap(),
                    Some(borsh::to_vec(binding).unwrap()),
                );
            }
        }
    }
    // The object stands: the route has now seen its signer.
    builder.note_kernel_bond(signer);
    for event in &events {
        if let LedgerEventV1::ClaimCommitted { claim } = event {
            let deadline = ledger.claims.get(claim).map(|c| c.committed_daa + ledger.policy.check_window_daa).unwrap_or(ctx.daa_score);
            // RFC-0015: an OptimisticPublicVerification claim has no Panel — no seats, no assignment, no receipts.
            let panel_licensed = ledger.mode_of_claim(claim) == Some(misaka_palw_kernel::mode::VerificationModeV1::PanelLicensed);
            if panel_licensed && matches!(ledger.claims.get(claim).map(|c| &c.body), Some(ClaimBodyV1::Program { .. })) {
                let assignment = draw_seats(builder, claim, signer, deadline);
                for seat in &assignment.seats {
                    builder.note_kernel_bond(&seat.bond);
                }
                builder.write_kernel_row(
                    PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1,
                    key_bytes(claim),
                    Some(borsh::to_vec(&assignment).expect("an assignment serializes")),
                );
            }
        }
    }
    // (A settlement the real bonds cannot honour fails the object — the walk's rehearsal drops it with its builder, so no half-applied
    // slash survives; the sync above makes this unreachable.)
    apply_settlements(builder, &events, true, ctx.daa_score)?;
    builder.close_terminal_kernel_budgets_v1(&ledger);
    persist_budget(builder, ctx, &ledger);
    // Only the exact signed candidate used the temporary scope. Persisting this root in
    // TABLE_ATTESTED would let an unrelated program borrow it in the next object.
    ledger.attested_artifacts = original_attested;
    flush(builder, &ledger, &before);
    if let Some((subject, row)) = scope {
        super::palw_provider_court_fold_v1::write_court_scope_row_v1(builder, &subject, &row);
    }
    Ok(())
}

/// **Record what this block's objects have spent of its adjudication budget** (the next object of the same chain block restores it).
fn persist_budget(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, ledger: &KernelLedgerV1) {
    let spent = ledger.budget_used();
    if spent != misaka_palw_kernel::ledger::BlockBudgetV1::default() {
        builder.write_kernel_row(
            PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1,
            Vec::new(),
            Some(borsh::to_vec(&(ctx.blue_score, spent.adjudications, spent.court_work)).expect("a budget serializes")),
        );
    }
}

/// **Charge one adjudication (and `court_work`) of THIS block's budget to an onboarding object** (tag 109's judgement): the same
/// budget, row and caps the kernel's own objects spend (`load_ledger` restores it for the next kernel object of the block), so the
/// block is bounded across both. `Ok(false)`: the budget is spent and nothing is charged (the object is dismissed, the block stands).
///
/// `may_spend_reserve` (C4 F-C4R4-10 / -11): every onboarding object stops short of the runs the kernel reserves for proofs
/// (`prosecution_reserved_runs`, as every kernel object but a `FileProof` does) — except a conformance REFUTATION, which is a proof:
/// it may spend them, and one that proves nothing pays `dismissed_proof_fee`, as a dismissed `FileProof` does.
pub(super) fn charge_route_budget_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    court_work: u64,
    may_spend_reserve: bool,
) -> Result<bool, PalwStateV2Error> {
    let (policy, _, _) = route_policies(builder)?;
    let (adjudications, work) = match builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|k| k.aux_row::<(u64, u32, u64)>(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
    {
        Some((blue_score, a, w)) if blue_score == ctx.blue_score => (a, w),
        _ => (0, 0),
    };
    let spent = misaka_palw_kernel::ledger::BlockBudgetV1 { adjudications, court_work: work };
    let Some(next) = spent.charged_v1(&policy, court_work, may_spend_reserve) else {
        return Ok(false);
    };
    builder.write_kernel_row(
        PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1,
        Vec::new(),
        Some(borsh::to_vec(&(ctx.blue_score, next.adjudications, next.court_work)).expect("a budget serializes")),
    );
    Ok(true)
}

/// The route's ledger policy at this block (the fee a dismissed proof pays, for an onboarding refutation).
pub(super) fn route_ledger_policy_v1(
    builder: &TransitionBuilder<'_>,
) -> Result<misaka_palw_kernel::ledger::LedgerPolicyV1, PalwStateV2Error> {
    Ok(route_policies(builder)?.0)
}

/// **Tag 111: a seat's constraint receipt.** Its signature verified at acceptance; the fold admits it structurally against the claim's
/// interim assignment, counts it, and — the tally covered — tells the ledger the Panel passed the claim.
pub(super) fn apply_kernel_receipt_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    receipt: &PalwConstraintReceiptV1,
    signature: &[u8],
) -> Result<(), PalwStateV2Error> {
    if builder.extras.kernel_route.is_none() {
        return Err(refused("palw_probabilistic_constraints_v1 is not in force at this block"));
    }
    let claim = receipt.claim_id;
    let (assignment, counted) = match builder.state.kernel_route.as_ref() {
        Some(kernel) => match kernel.assignment_of(&claim) {
            Some(assignment) => (assignment, kernel.receipts_of(&claim)),
            None => return Ok(()),
        },
        None => return Ok(()),
    };
    // The seat's operator is the bond's: a receipt cannot count under another operator's name.
    let Some(seat_key) = assignment.seats.iter().find(|s| s.kernel_bond == receipt.seat_bond).map(|s| s.bond) else { return Ok(()) };
    if builder.state.bonds.get(&seat_key).map(|b| b.operator_id.as_bytes()) != Some(receipt.seat_operator) {
        return Ok(());
    }
    let mut ledger = load_ledger(builder, ctx)?;
    let before = loaded_rows(builder, &ledger);
    let Some(row) = ledger.claims.get(&claim) else { return Ok(()) };
    let ClaimBodyV1::Program { evidence, .. } = &row.body else { return Ok(()) };
    let Some(class) = ledger.classes.get(&row.class_binding_id) else { return Ok(()) };
    let assignments: Vec<(misaka_palw_kernel::hash::Digest, ScopeV1)> =
        assignment.seats.iter().map(|s| (s.kernel_bond, s.scope.clone())).collect();
    let facts = ClaimFactsV1 {
        descriptor: &class.descriptor,
        evidence,
        claim_id: claim,
        assignment_root: assignment.assignment_root,
        challenge_anchor: assignment.challenge_anchor,
        sample_seed: assignment.sample_seed,
        assignments: &assignments,
        deadline_daa: assignment.deadline_daa,
    };
    let signed = SignedConstraintReceiptV1 { receipt: receipt.clone(), signature: signature.to_vec() };
    if admit_receipt_v1(&signed, &facts, &SignatureAlreadyVerified).is_err() {
        return Ok(());
    }
    let mut receipts = counted;
    if receipts.iter().any(|r| r.seat_bond == receipt.seat_bond && r.scope_root == receipt.scope_root) {
        return Ok(()); // a seat's first receipt for a duty is the one counted
    }
    receipts.push(receipt.clone());
    let mut tally = ConstraintTallyV1::new(TallyPolicyV1 { per_segment_quorum: assignment.quorum }, evidence);
    for r in &receipts {
        tally.add(r, evidence);
    }
    if matches!(tally.state(), TallyStateV1::Covered) {
        let _ = ledger.apply_panel_tally(&claim, true);
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, key_bytes(&claim), None);
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1, key_bytes(&claim), None);
    } else {
        builder.write_kernel_row(
            PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1,
            key_bytes(&claim),
            Some(borsh::to_vec(&receipts).expect("receipts serialize")),
        );
    }
    flush(builder, &ledger, &before);
    Ok(())
}

/// **The block's closing step**: the kernel's `tick` (demand deadlines, windows, Final, liability release, seal expiry, escrow
/// return, the served demands' bonds) and its settlements. A no-op for a route with nothing the tick can move.
pub(super) fn tick_kernel_route_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if builder.extras.kernel_route.is_none() {
        return Ok(());
    }
    // The onboarding bindings whose refutation horizon ends release their reservation, whatever the ledger is doing.
    super::palw_onboarding_fold_v1::tick_onboarding_v1(builder, ctx);
    // The route's chunk groups past their TTL are dropped and their deposits forfeited, whatever the ledger is doing.
    forfeit_expired_chunk_groups_v1(builder, ctx);
    // DA16: the provider court's deadlines (a provider that did not answer is charged), expired leases, lapses — before the kernel's tick,
    // so a claim whose last provider was charged this block is void before its demands are swept.
    super::palw_provider_court_fold_v1::tick_provider_court_v1(builder, ctx)?;
    let Some(kernel) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    // Anything the tick moves: claims and demands, and (G14-R4) bonded seals that expire, escrows that return and served demands'
    // bonds — a route whose only rows are a seal or an escrow must still forfeit or return it on time.
    let busy = kernel.rows.keys().any(|(table, _)| {
        use misaka_palw_kernel::rows as r;
        matches!(
            *table,
            r::TABLE_CLAIMS_V1
                | r::TABLE_DEMANDS_V1
                | r::TABLE_SEALS_V1
                | r::TABLE_PROOF_SEALS_V1
                | r::TABLE_JOB_ESCROWS_V1
                | r::TABLE_SERVED_DEMAND_BONDS_V1
                | r::TABLE_SEG_PROGRESS_V1
                | r::TABLE_VERIFIER_PAY_V1
                // codex RFC-7: the source rows (table 30) tick too; its prepaid fees ride table 27 (kinds 5 / 6, R6781-F merge).
                | r::TABLE_BEACON_SOURCES_V1
        )
    });
    if !busy {
        return Ok(());
    }
    let mut ledger = load_ledger(builder, ctx)?;
    let before = loaded_rows(builder, &ledger);
    // Every bond the ledger holds is re-synced from the real chain before it is settled against.
    let held: Vec<misaka_palw_kernel::hash::Digest> = ledger.bonds.keys().copied().collect();
    for kid in held {
        if let Some(key) = builder.state.kernel_route.as_ref().and_then(|k| k.bond_key_of(&kid)) {
            ledger.sync_bond(kid, builder.kernel_synced_collateral(&key, ctx.daa_score));
        }
    }
    let pending_source_fees: Vec<_> =
        ledger.beacon_source_fee_promises.iter().filter(|(_, promise)| promise.outcome.is_none()).map(|(key, _)| *key).collect();
    let events = ledger.tick();
    // Consume the same deferred R reservation before paying the separately
    // prepaid escrow. A principal refund cancels the entitlement and earns no
    // income; unchanged pending promises retain their reservation through H.
    for (context, producer) in pending_source_fees {
        use misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeOutcomeV1;
        let promise = &ledger.beacon_source_fee_promises[&(context, producer)];
        let Some(outcome) = promise.outcome else { continue };
        let source = ledger.beacon_sources.get(&(context, producer)).ok_or_else(|| refused("settled fee source missing"))?;
        let bond = builder
            .state
            .kernel_route
            .as_ref()
            .and_then(|route| route.bond_key_of(&producer))
            .ok_or_else(|| refused("settled fee recipient bond missing"))?;
        let subject = source_income_subject_v1(&source.job, &source.seal);
        match outcome {
            BeaconSourceFeeOutcomeV1::Paid { daa, claim, .. } => {
                if daa != ctx.daa_score {
                    return Err(refused("source income payment position differs"));
                }
                builder.bind_material_income_root_v1(
                    ctx.daa_score,
                    Hash64::from_bytes(claim),
                    super::palw_bond_budget_fold_v1::source_income_id_v1(Hash64::from_bytes(context), subject, &bond),
                )?;
                builder.settle_source_income_v1(
                    ctx,
                    Hash64::from_bytes(context),
                    subject,
                    bond,
                    promise.accepted_daa,
                    source.terms.hold_until,
                    promise.fee,
                )?;
            }
            BeaconSourceFeeOutcomeV1::Refunded { .. } => {
                builder.cancel_source_income_v1(ctx, Hash64::from_bytes(context), subject, bond, promise.accepted_daa)?;
            }
        }
    }
    apply_settlements(builder, &events, false, ctx.daa_score)?;
    builder.close_terminal_kernel_budgets_v1(&ledger);
    // Assignments and counted receipts of claims that can no longer be covered are dropped.
    let stale: Vec<misaka_palw_kernel::hash::Digest> = builder
        .state
        .kernel_route
        .as_ref()
        .map(|k| {
            k.aux
                .keys()
                .filter(|(table, _)| *table == PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1)
                .filter_map(|(_, key)| borsh::from_slice::<misaka_palw_kernel::hash::Digest>(key).ok())
                .filter(|claim| ledger.claims.get(claim).is_none_or(|row| row.life.state.is_terminal()))
                .collect()
        })
        .unwrap_or_default();
    for claim in stale {
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, key_bytes(&claim), None);
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1, key_bytes(&claim), None);
    }
    flush(builder, &ledger, &before);
    // DA16: a demand on a transferred claim that nobody answered is its providers' failure — charge them (never the producer).
    super::palw_provider_court_fold_v1::settle_provider_liable_defaults_v1(builder, ctx, &events)?;
    Ok(())
}

// ---- tag 113: the route's own chunk lane (C4 F-C4R3-03) ------------------------------------------------------------------------

/// **Tag 113: one chunk of a prosecution object in the route's own chunk lane** (C4 F-C4R3-03).
///
/// The certification lane's `ObjectChunk` table is ONE network-wide table of eight groups, opened by unsigned chunks for a flat rent
/// and held for 4,000 DAA: eight junk chunks (1.6 KAS) held every chunked prosecution off the chain past the lie's window and its
/// whole liability horizon. Here:
///
/// * a group is keyed `(opener, group)` and every chunk is SIGNED by the opener's Active bond (checked at acceptance), so a bond's
///   room is its own — no bond, and no set of bonds, can occupy another's;
/// * a bond holds at most [`PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1`] open groups, each backed by a deposit of
///   [`PALW_KERNEL_CHUNK_DEPOSIT_PER_PART_SOMPI_V1`] per declared part from its FREE collateral (V2's committed-collateral ledger and
///   both withdrawal gates read it), returned when the group completes and forfeited when it expires — junk pays, honesty is refunded;
/// * a group lives at most [`PALW_KERNEL_CHUNK_TTL_MAX_DAA_V1`] and never past its target's deadline (a proof group that cannot
///   complete before the claim's horizon is worthless), and one whose target can no longer use it is refused at its first chunk;
/// * the completing chunk assembles the object (its bytes must hash to the group id), which must be a prosecution of the group's
///   target ([`palw_kernel_chunk_inner_matches_target_v1`]); the object is then applied by its own arm, exactly as if carried bare.
///
/// Every refusal is an error: the walk's rehearsal drops the chunk and the block stands. The lane is the kernel route's; other lanes'
/// chunked objects keep the certification lane, unchanged.
pub(super) fn apply_kernel_route_chunk_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    chunk: &PalwKernelChunkV1,
) -> Result<(), PalwStateV2Error> {
    if builder.extras.kernel_route.is_none() {
        return Err(refused("palw_probabilistic_constraints_v1 is not in force at this block"));
    }
    if matches!(chunk.target, PalwKernelChunkTargetV1::FundedProofV11 { .. }) {
        return super::palw_flat_execution_fold_v11::apply_chunk(builder, ctx, chunk);
    }
    if matches!(chunk.target, PalwKernelChunkTargetV1::StageProofV14 { .. }) {
        return super::palw_flat_stage_fold_v14::apply_chunk(builder, ctx, chunk);
    }
    if chunk.count == 0 || chunk.count > PALW_OBJECT_CHUNK_MAX_COUNT {
        return Err(PalwStateV2Error::ChunkCountOutOfRange { count: chunk.count, max: PALW_OBJECT_CHUNK_MAX_COUNT });
    }
    if chunk.index >= chunk.count {
        return Err(PalwStateV2Error::ChunkIndexOutOfRange { index: chunk.index, count: chunk.count });
    }
    if chunk.bytes.is_empty() || chunk.bytes.len() > PALW_OBJECT_CHUNK_MAX_BYTES {
        return Err(PalwStateV2Error::ChunkTooLarge { bytes: chunk.bytes.len(), max: PALW_OBJECT_CHUNK_MAX_BYTES });
    }
    if !builder.state.bonds.get(&chunk.opener).is_some_and(|b| matches!(b.status, PalwBondStatusV2::Active)) {
        return Err(refused("a kernel chunk's opener is not an Active bond"));
    }
    if let PalwKernelChunkTargetV1::FlatProofV6 { actor, vector } = chunk.target {
        super::palw_flat_model_vector_fold_v3::enabled(builder, ctx)?;
        let policy = route_ledger_policy_v1(builder)?;
        let runs = if policy.prosecution_reserved_runs() == 0 {
            policy.max_adjudications_per_block
        } else {
            policy.prosecution_reserved_runs()
        };
        let valid = runs >= 2
            && actor == palw_kernel_bond_id_v1(&chunk.opener)
            && builder.state.kernel_route.as_ref().is_some_and(|r| {
                r.chunk_target_deadline_v1(&chunk.target, ctx.daa_score).is_some()
                    && r.flat_model_vector_header_v3(&vector)
                        .and_then(|h| protected_flat_chunk_finish_work_v6(&h))
                        .is_some_and(|w| w <= policy.guaranteed_proof_work_v1())
            });
        // Fixed seal authentication precedes assembly. Unsealed spam consumes only
        // admission capacity; a mature authenticated filer uses the proof reserve.
        // Read/clone/serialize the bounded own group and assemble/hash the final
        // carrier. Price that worst envelope before variable stored-group decode.
        let work = super::palw_flat_model_seal_fold_v6::metadata_work(chunk.bytes.len() + 3 * PALW_FLAT_PROOF_CHUNK_MAX_BYTES_V6)
            .ok_or_else(|| refused("protected chunk metadata work overflows"))?;
        let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, &chunk.opener, work, valid)? else {
            return Ok(());
        };
        builder.slash_bond(chunk.opener, fee)?;
        if !valid {
            return Ok(());
        }
    }
    if let PalwKernelChunkTargetV1::FlatProofV7 { actor, vector } = chunk.target {
        super::palw_flat_model_vector_fold_v3::enabled(builder, ctx)?;
        let policy = route_ledger_policy_v1(builder)?;
        let runs = if policy.prosecution_reserved_runs() == 0 {
            policy.max_adjudications_per_block
        } else {
            policy.prosecution_reserved_runs()
        };
        let valid = runs >= 2
            && actor == palw_kernel_bond_id_v1(&chunk.opener)
            && builder.state.kernel_route.as_ref().is_some_and(|r| {
                r.chunk_target_deadline_v1(&chunk.target, ctx.daa_score).is_some()
                    && r.flat_model_vector_header_v3(&vector)
                        .and_then(|h| protected_flat_chunk_finish_work_v7(&h))
                        .is_some_and(|w| w <= policy.guaranteed_proof_work_v1())
            });
        // Fixed seal authentication precedes assembly. Unsealed spam consumes only
        // admission capacity; a mature authenticated filer uses the proof reserve.
        // Read/clone/serialize the bounded own group and assemble/hash the final
        // carrier. Price that worst envelope before variable stored-group decode.
        let work = super::palw_flat_model_vector_fold_v7::metadata_work(chunk.bytes.len() + 3 * PALW_FLAT_PROOF_CHUNK_MAX_BYTES_V7)
            .ok_or_else(|| refused("protected chunk metadata work overflows"))?;
        let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, &chunk.opener, work, valid)? else {
            return Ok(());
        };
        builder.slash_bond(chunk.opener, fee)?;
        if !valid {
            return Ok(());
        }
    }
    if let PalwKernelChunkTargetV1::FlatProofV9 { actor, vector } = chunk.target {
        super::palw_flat_model_vector_fold_v3::enabled(builder, ctx)?;
        let policy = route_ledger_policy_v1(builder)?;
        let runs = if policy.prosecution_reserved_runs() == 0 {
            policy.max_adjudications_per_block
        } else {
            policy.prosecution_reserved_runs()
        };
        let valid = runs >= 2
            && actor == palw_kernel_bond_id_v1(&chunk.opener)
            && builder.state.kernel_route.as_ref().is_some_and(|r| {
                r.chunk_target_deadline_v1(&chunk.target, ctx.daa_score).is_some()
                    && r.flat_model_vector_header_v3(&vector)
                        .and_then(|h| protected_flat_chunk_finish_work_v9(r, &h))
                        .is_some_and(|w| w <= policy.guaranteed_proof_work_v1())
            });
        // Fixed seal authentication precedes assembly. Unsealed spam consumes only
        // admission capacity; a mature authenticated filer uses the proof reserve.
        // Read/clone/serialize the bounded own group and assemble/hash the final
        // carrier. Price that worst envelope before variable stored-group decode.
        let cap = builder
            .state
            .kernel_route
            .as_ref()
            .and_then(|r| r.flat_model_vector_header_v3(&vector))
            .and_then(|h| protected_flat_chunk_cap_v9(&h))
            .ok_or_else(|| refused("no admitted typed9 assembly cap"))?;
        let work = super::palw_flat_model_vector_fold_v9::metadata_work(chunk.bytes.len() + 3 * cap)
            .ok_or_else(|| refused("protected chunk metadata work overflows"))?;
        let Some(fee) = super::palw_flat_model_vector_fold_v3::charge(builder, ctx, &chunk.opener, work, valid)? else {
            return Ok(());
        };
        builder.slash_bond(chunk.opener, fee)?;
        if !valid {
            return Ok(());
        }
    }
    // A target lives in the route's state (a claim's rows, a binding's aux row): no route, nothing to prosecute.
    let Some(route) = builder.state.kernel_route.as_ref() else {
        return Err(refused("no kernel route state: a kernel chunk has no target"));
    };
    let key = palw_kernel_chunk_group_key_v1(&chunk.opener, &chunk.group);
    let mut group = match route.chunk_group_v1(&chunk.opener, &chunk.group) {
        Some(stored) => {
            if stored.count != chunk.count || stored.target != chunk.target {
                return Err(PalwStateV2Error::ChunkGroupIncoherent { group: chunk.group });
            }
            if stored.parts.contains_key(&chunk.index) {
                return Err(PalwStateV2Error::ChunkDuplicate { group: chunk.group, index: chunk.index });
            }
            if ctx.daa_score > stored.expires_daa {
                return Err(refused("a kernel chunk group past its TTL takes no more parts (its deposit is forfeited)"));
            }
            stored
        }
        None => {
            // The opener's own room: a bound per bond, never a network-wide table.
            if route.chunk_groups_of_v1(&chunk.opener) >= PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1 {
                return Err(refused(format!(
                    "the opener already holds {PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1} open kernel chunk groups (the per-bond limit)"
                )));
            }
            // Never longer than the target can use it.
            let Some(deadline) = route.chunk_target_deadline_v1(&chunk.target, ctx.daa_score).filter(|d| *d >= ctx.daa_score) else {
                return Err(refused("a kernel chunk group's target can no longer use it (decided, unknown, or past its horizon)"));
            };
            let deposit = PALW_KERNEL_CHUNK_DEPOSIT_PER_PART_SOMPI_V1.saturating_mul(u64::from(chunk.count));
            let collateral = builder.state.bonds.get(&chunk.opener).map(|b| b.collateral as u128).unwrap_or(0);
            if collateral.saturating_sub(builder.committed_at(&chunk.opener, ctx.daa_score)) < deposit as u128 {
                return Err(refused("the opener's free collateral does not cover the kernel chunk group's deposit"));
            }
            PalwKernelChunkGroupV1 {
                target: chunk.target,
                count: chunk.count,
                opened_daa: ctx.daa_score,
                expires_daa: ctx.daa_score.saturating_add(PALW_KERNEL_CHUNK_TTL_MAX_DAA_V1).min(deadline),
                deposit,
                parts: BTreeMap::new(),
            }
        }
    };
    let proof_cap = match chunk.target {
        PalwKernelChunkTargetV1::FlatProofV6 { .. } => Some(PALW_FLAT_PROOF_CHUNK_MAX_BYTES_V6),
        PalwKernelChunkTargetV1::FlatProofV7 { .. } => Some(PALW_FLAT_PROOF_CHUNK_MAX_BYTES_V7),
        PalwKernelChunkTargetV1::FlatProofV9 { vector, .. } => Some(
            route
                .flat_model_vector_header_v3(&vector)
                .and_then(|h| protected_flat_chunk_cap_v9(&h))
                .ok_or_else(|| refused("no typed9 assembly cap"))?,
        ),
        _ => None,
    };
    if proof_cap
        .is_some_and(|cap| group.parts.values().map(Vec::len).sum::<usize>().checked_add(chunk.bytes.len()).is_none_or(|n| n > cap))
    {
        return Err(refused("protected chunk assembly exceeds its reveal byte bound"));
    }
    group.parts.insert(chunk.index, chunk.bytes.clone());
    if group.parts.len() < group.count as usize {
        builder.write_kernel_row(
            PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1,
            key,
            Some(borsh::to_vec(&group).expect("a group serializes")),
        );
        return Ok(());
    }
    // Complete: the object it carried is applied HERE, by the arm a directly carried one goes through.
    let mut whole = Vec::with_capacity(group.parts.values().map(Vec::len).sum());
    for i in 0..group.count {
        whole.extend_from_slice(&group.parts[&i]);
    }
    let computed = palw_object_chunk_group_id_v1(&whole);
    if computed != chunk.group {
        return Err(PalwStateV2Error::ChunkGroupHashMismatch { group: chunk.group, computed });
    }
    let inner: PalwConsensusObjectV2 =
        borsh::from_slice(&whole).map_err(|e| PalwStateV2Error::ChunkedObjectUndecodable(e.to_string()))?;
    if !palw_kernel_chunk_inner_matches_target_v1(&inner, &chunk.target) {
        return Err(refused("the assembled object is not a prosecution of its group's target"));
    }
    if matches!(
        chunk.target,
        PalwKernelChunkTargetV1::FlatProofV6 { .. }
            | PalwKernelChunkTargetV1::FlatProofV7 { .. }
            | PalwKernelChunkTargetV1::FlatProofV9 { .. }
    ) && !matches!(&inner, PalwConsensusObjectV2::KernelRouteV1 { signer, .. } if *signer == chunk.opener)
    {
        return Err(refused("a protected reveal must be signed by its chunk opener"));
    }
    // OPV-BOOT #1: conformance evidence rides only a group its class's registrant opened — every action but a Refute (anyone's), so
    // a registrant's action added later (OPV-BOOT's PostComplete) is the registrant's here too, never anyone's by default.
    if let PalwConsensusObjectV2::ConformanceEvidenceV1 { v2_class, action, .. } = &inner
        && !matches!(action.as_ref(), crate::palw_conformance_evidence_v1::ConformanceEvidenceActionV1::Refute { .. })
        && builder.state.classes.get(v2_class).and_then(|c| c.registrant_bond) != Some(chunk.opener)
    {
        return Err(refused("conformance evidence other than a Refute rides only a group its class's registrant opened"));
    }
    // The group delivered: its row (and with it the deposit) goes before the object is applied.
    builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, key, None);
    super::apply_object(builder, ctx, &inner)
}

/// **The closing step's chunk sweep**: every group past its TTL is dropped and its opener's deposit forfeited (a real slash, burned
/// at release). Lenient (it runs after the rehearsal): a deposit the bond can no longer pay is taken as far as it goes.
fn forfeit_expired_chunk_groups_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    let Some(route) = builder.state.kernel_route.as_ref() else { return };
    let expired = route.expired_chunk_headers_v1(ctx.daa_score);
    for (opener, group, deposit) in expired {
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, palw_kernel_chunk_group_key_v1(&opener, &group), None);
        let _ = builder.slash_bond(opener, deposit as u128);
    }
}

/// Removing a candidate cannot remove the public program/PC while an admitted
/// mixed vector still owes its proof/material horizon. Decode fixed headers
/// only; never clone a variable posting or execute a model. The work-only debit
/// is bounded and precedes the header walk, and cannot consume prosecution runs.
fn candidate_withdraw_held_v9(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    ledger: &KernelLedgerV1,
    owner: &misaka_palw_kernel::hash::Digest,
) -> Result<bool, PalwStateV2Error> {
    if super::palw_flat_execution_fold_v11::candidate_withdraw_held(builder, ctx, ledger, owner)? {
        return Ok(true);
    }
    use crate::palw_flat_model_vector_v3::{FLAT_MODEL_VECTOR_TABLE_V3, FlatModelVectorHeaderV3};
    use borsh::BorshDeserialize;
    let policy = route_policies(builder)?.0;
    let count_cap = policy.admission_work_limit_v1() / 65536;
    let count = builder
        .state
        .kernel_route
        .as_ref()
        .unwrap()
        .aux
        .range((FLAT_MODEL_VECTOR_TABLE_V3, Vec::new())..(FLAT_MODEL_VECTOR_TABLE_V3 + 1, Vec::new()))
        .take(count_cap as usize + 1)
        .count() as u64;
    if count > count_cap {
        return Ok(true);
    }
    let work = count.checked_mul(65536).ok_or_else(|| refused("candidate retention metadata overflow"))?;
    if work != 0 {
        let (a, w) = builder
            .state
            .kernel_route
            .as_ref()
            .and_then(|k| k.aux_row::<(u64, u32, u64)>(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
            .filter(|(blue, _, _)| *blue == ctx.blue_score)
            .map(|(_, a, w)| (a, w))
            .unwrap_or_default();
        let Some(w) = w.checked_add(work).filter(|w| *w <= policy.admission_work_limit_v1()) else { return Ok(true) };
        builder.write_kernel_row(
            PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1,
            Vec::new(),
            Some(borsh::to_vec(&(ctx.blue_score, a, w)).expect("retention metadata budget")),
        );
    }
    let route = builder.state.kernel_route.as_ref().unwrap();
    for ((_, key), row) in route.aux.range((FLAT_MODEL_VECTOR_TABLE_V3, Vec::new())..(FLAT_MODEL_VECTOR_TABLE_V3 + 1, Vec::new())) {
        let id: Hash64 = borsh::from_slice(key).map_err(|e| refused(format!("candidate retention vector key: {e}")))?;
        if !route.flat_model_vector_protected_v7(&id) && !route.flat_model_vector_protected_v9(&id) {
            continue;
        }
        let h = FlatModelVectorHeaderV3::deserialize_reader(&mut std::io::Cursor::new(row))
            .map_err(|e| refused(format!("candidate retention header: {e}")))?;
        if !h.refuted && ctx.daa_score < h.liability_until && ledger.native_candidate_owned_v9(owner, &h.class.as_bytes()) {
            return Ok(true);
        }
    }
    Ok(false)
}
