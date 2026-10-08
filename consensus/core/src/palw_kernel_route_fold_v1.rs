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

/// The fold's signature check is the acceptance layer's (a pure transition holds no crypto): by the time a receipt reaches here its
/// signature has verified under the seat bond's registered key, so the kernel's structural admission is asked with this.
struct SignatureAlreadyVerified;
impl ReceiptSignatureVerifier for SignatureAlreadyVerified {
    fn verify(&self, _seat_bond: &misaka_palw_kernel::hash::Digest, _message: &misaka_palw_kernel::hash::Digest, _signature: &[u8]) -> bool {
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
        let map = if table < PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1 { &mut kernel.rows } else { &mut kernel.aux };
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
            Some(kernel) => kernel.header = new.clone(),
            None => {
                self.state.kernel_route =
                    Some(PalwKernelRouteStateV1 { header: new.clone(), rows: LedgerRowsV1::new(), aux: Default::default() })
            }
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
    fn note_kernel_bond(&mut self, bond: &PalwBondKeyV2) -> misaka_palw_kernel::hash::Digest {
        let kid = palw_kernel_bond_id_v1(bond);
        self.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, key_bytes(&kid), Some(borsh::to_vec(bond).expect("a bond key serializes")));
        kid
    }
}

/// The ledger policy and the OPV policy this block's rules put the route under (both genesis constants of the network).
fn route_policies(
    builder: &TransitionBuilder<'_>,
) -> Result<(misaka_palw_kernel::ledger::LedgerPolicyV1, Option<misaka_palw_kernel::opv::OpvPolicyV1>), PalwStateV2Error> {
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
    Ok((policy, opv_policy))
}

/// **The route's header, created on first use** under the policies the processor resolved (and refused if the stored route was folded
/// under others). Every object that writes a row — a kernel object or an onboarding one — starts here, so the rows always have a
/// header to live under.
pub(super) fn ensure_route_header(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let (policy, opv_policy) = route_policies(builder)?;
    match builder.state.kernel_route.as_ref() {
        None => {
            let created =
                PalwKernelRouteStateV1::new(policy, opv_policy, misaka_palw_kernel::rows::LedgerScalarsV1 { daa: ctx.daa_score, burned: 0 });
            builder.write_kernel_header(created.header);
        }
        Some(kernel) if kernel.header.policy != policy || kernel.header.opv != opv_policy => {
            return Err(refused("the stored kernel route was folded under another policy"));
        }
        Some(_) => {}
    }
    Ok(())
}

/// **The ledger the block's kernel moves run on**: the route's header created on first use (under the interim policy the processor
/// resolved), the ledger rebuilt from the rows, the block begun.
fn load_ledger(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<KernelLedgerV1, PalwStateV2Error> {
    ensure_route_header(builder, ctx)?;
    let extras = builder.extras.kernel_route.as_ref().expect("checked by the header");
    // The artifact roots the route may attest: the test hook's list (empty outside a test) and every Matured or Final onboarding
    // binding — the least-trust source (`palw_onboarding_v1`).
    let mut attested = extras.attested_artifacts.clone();
    let admitted: Vec<Hash64> = extras.opv.as_ref().map(|o| o.admitted_classes.clone()).unwrap_or_default();
    let opv_declared = extras.opv.is_some();
    if let Some(route) = builder.state.kernel_route.as_ref() {
        attested.extend(route.onboarding_attested_roots_v1(ctx.daa_score));
    }
    let mut ledger = builder.state.kernel_route.as_ref().expect("created above").ledger().map_err(refused)?;
    ledger.begin_block(ctx.daa_score).map_err(|r| refused(r.to_string()))?;
    // The budget bounds the BLOCK: what the block's earlier objects already spent comes back (they were folded one at a time).
    if let Some((blue_score, adjudications, court_work)) = builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|k| k.aux_row::<(u64, u32, u64)>(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
        && blue_score == ctx.blue_score
    {
        ledger.restore_budget(misaka_palw_kernel::ledger::BlockBudgetV1 { adjudications, court_work });
    }
    // The attested set IS the derived set (not a growing one): a binding refuted after a root was attested stops attesting it for the
    // next kernel class, and the stale row goes with it.
    ledger.attested_artifacts = attested.iter().map(|root| root.as_bytes()).collect();
    // The network policy's admissions (consensus, not a registrant's choice); idempotent, so an admitted class is one row.
    if opv_declared {
        for class in admitted {
            ledger.admit_optimistic_class(class.as_bytes()).map_err(|r| refused(r.to_string()))?;
        }
    }
    Ok(ledger)
}

/// Write back every ledger row that changed and the scalars; `before` is the rows as `load_ledger` found them.
fn flush(builder: &mut TransitionBuilder<'_>, ledger: &KernelLedgerV1, before: &LedgerRowsV1) {
    for ((table, key), old_new) in diff_rows(before, &ledger.to_rows()).into_iter().map(|(k, _old, new)| (k, new)) {
        builder.write_kernel_row(table, key, old_new);
    }
    let header = builder.state.kernel_route.as_ref().expect("a route that flushes exists").header.clone();
    let scalars = ledger.scalars();
    if header.scalars != scalars {
        builder.write_kernel_header(PalwKernelRouteHeaderV1 { scalars, ..header });
    }
}

/// **Apply a ledger's settlement instructions to the real bonds** (module doc).
///
/// `strict` (every object arm): a slash the real bond cannot pay in full refuses the object (it is dropped, the block stands). The closing
/// tick is not strict: it runs after the rehearsal, so an error there would fail the whole block — and with every held bond re-synced
/// just before it, the ledger's clamped instructions are always payable; if that ever failed, taking what the bond holds is the safe side.
fn apply_settlements(builder: &mut TransitionBuilder<'_>, events: &[LedgerEventV1], strict: bool) -> Result<(), PalwStateV2Error> {
    // GAP-5: what this batch actually debited from posters' escrows — the only source a Final reward is paid from. A `FinalReward`
    // with no spent escrow behind it is never paid (there is no unfunded reward path).
    let mut escrow_debited: u64 = 0;
    for event in events {
        let LedgerEventV1::Settlement(s) = event else { continue };
        let key = builder.state.kernel_route.as_ref().and_then(|k| k.bond_key_of(&s.bond));
        match s.kind {
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
                if s.kind == SettlementKindV1::PayJobEscrow {
                    escrow_debited = escrow_debited.saturating_add(debit);
                }
            }
            SettlementKindV1::AccuserReward | SettlementKindV1::DemanderShare | SettlementKindV1::FinalReward => {
                let amount = if s.kind == SettlementKindV1::FinalReward {
                    let funded = s.amount.min(escrow_debited);
                    escrow_debited -= funded;
                    if funded != s.amount && strict {
                        return Err(refused("a Final reward with no spent escrow behind it (GAP-5: nothing is issued)"));
                    }
                    funded
                } else {
                    s.amount
                };
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
    // A malformed or over-size encoding, or a signer that is no Active bond, is a dropped object.
    let Ok(object) = KernelRouteObjectV1::decode(bytes) else { return Ok(()) };
    if bytes.len() > PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 {
        return Ok(());
    }
    if !builder.state.bonds.get(signer).is_some_and(|b| matches!(b.status, PalwBondStatusV2::Active)) {
        return Ok(());
    }
    let mut ledger = load_ledger(builder, ctx)?;
    let before = ledger.to_rows();
    let kid = palw_kernel_bond_id_v1(signer);
    ledger.sync_bond(kid, builder.kernel_synced_collateral(signer, ctx.daa_score));
    // The other bond a slash can land on is the producer of the claim the object names: its collateral is brought up to date too, so
    // an earlier object of this block (or another lane's slash) can never leave the ledger believing the bond holds more than it does.
    if let KernelRouteObjectV1::FileProof { claim, .. } | KernelRouteObjectV1::FileDemand { claim, .. } | KernelRouteObjectV1::Respond { claim, .. } =
        &object
        && let Some(producer) = ledger.claims.get(claim).map(|row| row.producer)
        && let Some(key) = builder.state.kernel_route.as_ref().and_then(|k| k.bond_key_of(&producer))
    {
        ledger.sync_bond(producer, builder.kernel_synced_collateral(&key, ctx.daa_score));
    }
    let events = match ledger.apply_object(&object, &AuthV1 { signer_bond: kid }) {
        Ok(events) => events,
        Err(_refusal) => {
            // A refusal that did court work still spent the block's budget (so junk cannot be tried for free); nothing else is kept.
            persist_budget(builder, ctx, &ledger);
            return Ok(());
        }
    };
    // A class registers only if its worst filing, response and commitments can actually be carried (chunking counted).
    for event in &events {
        if let LedgerEventV1::ClassRegistered { class } = event {
            let bounds = ledger.classes.get(class).map(|c| c.bounds).or_else(|| ledger.pipeline_classes.get(class).map(|c| c.bounds));
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
    apply_settlements(builder, &events, true)?;
    persist_budget(builder, ctx, &ledger);
    flush(builder, &ledger, &before);
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
pub(super) fn charge_route_budget_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    court_work: u64,
) -> Result<bool, PalwStateV2Error> {
    let (policy, _) = route_policies(builder)?;
    let (adjudications, work) = match builder
        .state
        .kernel_route
        .as_ref()
        .and_then(|k| k.aux_row::<(u64, u32, u64)>(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
    {
        Some((blue_score, a, w)) if blue_score == ctx.blue_score => (a, w),
        _ => (0, 0),
    };
    if adjudications >= policy.max_adjudications_per_block || work.saturating_add(court_work) > policy.max_court_work_per_block {
        return Ok(false);
    }
    builder.write_kernel_row(
        PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1,
        Vec::new(),
        Some(borsh::to_vec(&(ctx.blue_score, adjudications + 1, work.saturating_add(court_work))).expect("a budget serializes")),
    );
    Ok(true)
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
    let before = ledger.to_rows();
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
        )
    });
    if !busy {
        return Ok(());
    }
    let mut ledger = load_ledger(builder, ctx)?;
    let before = ledger.to_rows();
    // Every bond the ledger holds is re-synced from the real chain before it is settled against.
    let held: Vec<misaka_palw_kernel::hash::Digest> = ledger.bonds.keys().copied().collect();
    for kid in held {
        if let Some(key) = builder.state.kernel_route.as_ref().and_then(|k| k.bond_key_of(&kid)) {
            ledger.sync_bond(kid, builder.kernel_synced_collateral(&key, ctx.daa_score));
        }
    }
    let events = ledger.tick();
    apply_settlements(builder, &events, false)?;
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
    let expired: Vec<(PalwBondKeyV2, Hash64, u64)> = route
        .chunk_groups_v1()
        .into_iter()
        .filter(|(_, _, g)| ctx.daa_score > g.expires_daa)
        .map(|(opener, group, g)| (opener, group, g.deposit))
        .collect();
    for (opener, group, deposit) in expired {
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, palw_kernel_chunk_group_key_v1(&opener, &group), None);
        let _ = builder.slash_bond(opener, deposit as u128);
    }
}
