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

/// **G14 T08 / P1-3 (lane T0308): a rejected response is judged once.** The route object's signature binds only the network, the
/// signer and the object bytes (`palw_kernel_route_message_v1`), so anybody can re-carry a signer's old signed `Respond` in a fresh
/// carrier. A REJECTED response costs its signer `dismissed_proof_fee`; without this marker a third party could replay the same
/// signed junk to burn that fee again and again. Key: the signer, the demanded position and the exact bytes (not the demand's
/// filing DAA, so an old signature is never "new" for a later demand either); row: the DAA it was first rejected at (u64). A key
/// present here drops the object before the ledger classifies or charges anything. A VALID response needs no marker: it closes
/// the demand and the position is served for good (`already served`).
pub const PALW_KERNEL_REJECTED_RESPONSE_TABLE_V1: u8 = 161;

pub fn palw_kernel_rejected_response_key_v1(
    signer: &misaka_palw_kernel::hash::Digest,
    claim: &misaka_palw_kernel::hash::Digest,
    stage: u8,
    position: u32,
    bytes: &[u8],
) -> Vec<u8> {
    let mut hash = keyed(b"misaka-palw/kernel-route/rejected-response/v1");
    hash.update(signer);
    hash.update(claim);
    hash.update(&[stage]);
    hash.update(&position.to_le_bytes());
    hash.update(&(bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
    finish(hash).as_byte_slice().to_vec()
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

    /// **Lane F02b:** the court capital the slice's admissions reserved against `bond` (its rows in the dedicated aux table).
    pub fn kernel_court_capital_reserved_v1(&self, bond: &PalwBondKeyV2) -> u128 {
        use crate::palw_kernel_court_admission_v1::*;
        let Some(k) = self.kernel_route.as_ref() else { return 0 };
        let prefix = kernel_court_capital_payer_prefix_v1(bond);
        k.aux
            .range((PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1, prefix.clone())..)
            .take_while(|((table, key), _)| *table == PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1 && key.starts_with(&prefix))
            .filter_map(|(_, bytes)| borsh::from_slice::<KernelCourtCapitalRowV1>(bytes).ok())
            .fold(0u128, |sum, row| sum.saturating_add(row.amount))
    }

    /// **Lane F02b:** every court capital row (the release sweep's input).
    pub fn kernel_court_capital_rows_v1(&self) -> Vec<crate::palw_kernel_court_admission_v1::KernelCourtCapitalRowV1> {
        use crate::palw_kernel_court_admission_v1::*;
        let Some(k) = self.kernel_route.as_ref() else { return Vec::new() };
        k.aux
            .range((PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1, Vec::new())..)
            .take_while(|((table, _), _)| *table == PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1)
            .filter_map(|(_, bytes)| borsh::from_slice::<KernelCourtCapitalRowV1>(bytes).ok())
            .collect()
    }

    /// **Lane F02b: the court-bound kernel claims whose producer default is held, with the release horizon (hard deadline).** Held
    /// while no inclusion premise is adopted, or (premise adopted, S3c') while any reserved packet of the claim is recorded as
    /// `MissingSupply`. A held default is released without slash at the horizon (the kernel ledger's tick).
    pub fn kernel_court_default_held_v1(
        &self,
        premise: Option<&crate::palw_court_service_binding_v1::CourtServiceInclusionPremiseV1>,
    ) -> std::collections::BTreeMap<[u8; 64], u64> {
        let Some(service) = self.court_service_state_v1() else { return Default::default() };
        let Some(k) = self.kernel_route.as_ref() else { return Default::default() };
        service
            .claims
            .values()
            .filter(|binding| k.kernel_claim_row_v1(&binding.claim).is_some())
            .filter(|binding| {
                premise.is_none()
                    || binding.packet_ids.iter().any(|id| {
                        service.book.assess(id) == crate::palw_court_service_v1::ServiceAssessment::MissingSupply
                    })
            })
            .map(|binding| (binding.claim.as_bytes(), binding.hard_deadline_daa))
            .collect()
    }

    /// **G14 S3c''' (lane LEGACY, 2026-10-27): the kernel claims whose producer default may NOT be attributed now**, with the
    /// horizon at which a held default is released without slash. [`Self::kernel_court_default_held_v1`]'s set, plus every kernel
    /// claim with an open demand for which the producer had no opportunity to answer:
    ///
    /// * premise typed-missing: every demanded kernel claim is held (bound: to its hard deadline; unbound: to the demand's
    ///   deadline plus the ledger's court + proof-grace + liability tail);
    /// * premise adopted, claim bound: held unless some reserved packet's assigned unit `u` has recorded supply, lies inside the
    ///   packet's service interval and satisfies `u + inclusion_delay_daa <= ` the demand's response deadline
    ///   ([`crate::palw_court_service_binding_v1::CourtServiceInclusionPremiseV1::admits_reserved_unit_v1`]).
    ///
    /// Residual (reported): a kernel claim with NO court service binding under an adopted premise keeps the kernel ledger's
    /// deadline default until RFC-0026 S1/S2 binds every kernel claim at admission.
    pub fn kernel_court_default_held_v2(
        &self,
        premise: Option<&crate::palw_court_service_binding_v1::CourtServiceInclusionPremiseV1>,
        ledger: &KernelLedgerV1,
    ) -> std::collections::BTreeMap<[u8; 64], u64> {
        let mut held = self.kernel_court_default_held_v1(premise);
        let Some(k) = self.kernel_route.as_ref() else { return held };
        let mut due: std::collections::BTreeMap<[u8; 64], u64> = std::collections::BTreeMap::new();
        for ((claim, ..), demand) in ledger.demands.iter() {
            let d = due.entry(*claim).or_insert(0);
            *d = (*d).max(demand.deadline_daa);
        }
        let service = self.court_service_state_v1();
        let tail = ledger
            .policy
            .court_deadline_daa
            .saturating_add(ledger.policy.proof_grace_daa)
            .saturating_add(ledger.policy.liability_daa);
        for (claim, deadline) in due {
            if held.contains_key(&claim) || k.kernel_claim_row_v1(&Hash64::from_bytes(claim)).is_none() {
                continue;
            }
            let binding = service.and_then(|s| s.claims.get(&Hash64::from_bytes(claim)).map(|b| (s, b)));
            match (premise, binding) {
                (None, Some((_, b))) => {
                    held.insert(claim, b.hard_deadline_daa);
                }
                (None, None) => {
                    held.insert(claim, deadline.saturating_add(tail));
                }
                (Some(p), Some((s, b))) => {
                    let opportunity = b.packet_ids.iter().any(|id| {
                        let (Some(packet), Some(&unit)) = (s.book.packets.get(id), s.book.assignments.get(id)) else { return false };
                        s.book.supply.contains_key(&unit) && p.admits_reserved_unit_v1(packet.release_daa, packet.service_before, unit, deadline)
                    });
                    if !opportunity {
                        held.insert(claim, b.hard_deadline_daa);
                    }
                }
                (Some(_), None) => {}
            }
        }
        held
    }

    /// What the kernel route has reserved against `bond` (0 with no kernel route): V2's committed-collateral ledger adds it.
    pub fn kernel_reserved(&self, bond: &PalwBondKeyV2) -> u128 {
        self.kernel_route.as_ref().map(|k| k.reserved_of(bond) as u128).unwrap_or(0)
    }
}

impl TransitionBuilder<'_> {
    /// **The one writer of the kernel route's rows**, journaled `KernelRouteRow` (the ledger's tables below 32, the consensus tables
    /// from 32). The route's header must exist.
    ///
    /// RFC-0006: admitted before the map clones (`reserve_kernel_row_write_v1`). A legacy caller encoded `key`/`new` before this
    /// call; their journal-retained lifetime is admitted here too. A capacity refusal is `PalwStateV2Error::NativeResource` (local,
    /// retryable) and leaves the rows and the journal unchanged.
    pub(super) fn write_kernel_row(&mut self, table: u8, key: Vec<u8>, new: Option<Vec<u8>>) -> Result<(), PalwStateV2Error> {
        self.reserve_kernel_row_write_v1(table, &key, &new, true)?;
        self.apply_kernel_row_v1(table, key, new);
        Ok(())
    }

    /// [`Self::write_kernel_row`] for a key/row whose buffers the caller already admitted before encoding
    /// ([`Self::write_kernel_row_encoded_v1`]): only the map clones, the absent membership and the journal slot remain.
    pub(super) fn write_kernel_row_admitted_v1(
        &mut self,
        table: u8,
        key: Vec<u8>,
        new: Option<Vec<u8>>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_kernel_row_write_v1(table, &key, &new, false)?;
        self.apply_kernel_row_v1(table, key, new);
        Ok(())
    }

    fn apply_kernel_row_v1(&mut self, table: u8, key: Vec<u8>, new: Option<Vec<u8>>) {
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
    fn write_kernel_header(&mut self, new: PalwKernelRouteHeaderV1) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the old header copy, the new header's field and journal copies, a first route's empty state, one slot.
        if self.native_writers_tracking_v1() {
            let mut q = super::native_writers::NativeWriteQuoteV1::new("native kernel route header writer");
            if let Some(kernel) = self.state.kernel_route.as_ref() {
                q.copy(&kernel.header)?;
            } else {
                q.exact::<PalwKernelRouteStateV1>(1)?;
            }
            q.copy(&new)?.copy(&new)?.slots(1)?;
            self.admit_native_write_v1(q)?;
        }
        let old = self.state.kernel_route.as_ref().map(|k| k.header.clone());
        if old.as_ref() == Some(&new) {
            return Ok(());
        }
        match self.state.kernel_route.as_mut() {
            Some(kernel) => {
                kernel.header = new.clone();
                kernel.ledger_cache.clear();
            }
            None => self.state.kernel_route = Some(PalwKernelRouteStateV1::with_header(new.clone())),
        }
        self.entries.push(PalwDeltaEntryV2::KernelRouteHeader { old, new: Some(new) });
        Ok(())
    }

    /// A kernel payee's pay joins its one row in the coinbase queue.
    pub(super) fn add_kernel_payout(&mut self, payload: Hash64, amount: u64) -> Result<(), PalwStateV2Error> {
        if amount == 0 {
            return Ok(());
        }
        let key = palw_kernel_payout_key_v1(&payload);
        let held = self.state.pending_payouts.get(&key).map(|row| row.amount).unwrap_or(0);
        let total = held.checked_add(amount).ok_or(PalwStateV2Error::Overflow("kernel payout"))?;
        self.write_payout(key, Some(PalwPayoutV2 { payload, amount: total }))?;
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
    pub(super) fn note_kernel_bond(&mut self, bond: &PalwBondKeyV2) -> Result<misaka_palw_kernel::hash::Digest, PalwStateV2Error> {
        let kid = palw_kernel_bond_id_v1(bond);
        // A fixed `[u8; 64]` digest's Borsh bytes are the digest itself (`key_bytes`), borrowed here without a scratch Vec.
        self.write_kernel_row_encoded_v1(PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, &kid[..], Some(bond))?;
        Ok(kid)
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
            builder.write_kernel_header(created.header)?;
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
pub(super) fn load_ledger(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<crate::palw_native_resource_v1::ReservedKernelLedgerV1, PalwStateV2Error> {
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
) -> Result<crate::palw_native_resource_v1::ReservedKernelLedgerV1, PalwStateV2Error> {
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
            builder.write_kernel_row(scope_table, Vec::new(), Some(borsh::to_vec(&at).expect("an activation serializes")))?;
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
            )?;
        }
    }
    let extras = builder.extras.kernel_route.as_ref().expect("checked by the header");
    // The artifact roots the route may attest: the test hook's list (empty outside a test) and every Matured or Final onboarding
    // binding — the least-trust source (`palw_onboarding_v1`).
    let mut attested = extras.attested_artifacts.clone();
    if let Some(route) = builder.state.kernel_route.as_ref() {
        attested.extend(route.onboarding_attested_roots_v1(ctx.daa_score));
    }
    let mut ledger = builder
        .state
        .kernel_route
        .as_ref()
        .expect("created above")
        .ledger_native_projection_checked_v9()
        .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
    // RFC-0006: every injection below goes through the reserved ledger's typed, admitted methods (no `DerefMut`).
    ledger.begin_block_v1(ctx.daa_score).map_err(|r| refused(r.to_string()))?;
    // The budget bounds the BLOCK: what the block's earlier objects already spent comes back (they were folded one at a time).
    if let Some((blue_score, adjudications, court_work)) =
        builder.state.kernel_route.as_ref().and_then(|k| k.aux_row::<(u64, u32, u64)>(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
        && blue_score == ctx.blue_score
    {
        ledger.restore_budget_v1(misaka_palw_kernel::ledger::BlockBudgetV1 { adjudications, court_work });
    }
    // The attested set IS the derived set (not a growing one): a binding refuted after a root was attested stops attesting it for the
    // next kernel class, and the stale row goes with it.
    // G14R round 3: `palw_verifier_pay_v1`'s terms (M*-49, O2), injected at every load like the attested set (not in the root).
    // G14-S: the court scope (ADR-0177 D2's cumulative bound, `misaka_palw_kernel::court_scope`) is `palw_provider_court_v1`'s — injected
    // at every load like the verifier pay terms (not in the root; its table 28 is written only past it).
    // RFC-0002 P0 residual (RFC02-T): the sealed-bounty rule for kernel filings is `palw_model_refutation_seal_v1`'s — injected at every
    // load (not in the root; its table 19 is written only past it). The contest window is the route's seal TTL.
    let seal_ttl_daa = ledger.policy.seal_ttl_daa;
    // DA16: the claims whose material obligation moved to bonded providers — derived from the court's rooted rows, never a ledger table
    // (and nothing at all below `palw_provider_court_v1`, where no such row can exist).
    let provider_liable = extras.provider_court.is_some().then(|| {
        builder
            .state
            .kernel_route
            .as_ref()
            .map(|k| k.provider_liable_claims_v1())
            .unwrap_or_default()
            .iter()
            .map(|c| c.as_bytes())
            .collect()
    });
    ledger
        .install_consumer_inputs_v1(
            attested.iter().map(|root| root.as_bytes()).collect(),
            extras.verifier_pay.map(crate::palw_verifier_pay_v1::palw_verifier_pay_interim_policy_v1),
            extras.provider_court.map(|activation_daa| misaka_palw_kernel::court_scope::CourtScopePolicyV1 { activation_daa }),
            extras
                .model_refutation_seal
                .map(|activation_daa| misaka_palw_kernel::sealed_bounty::SealedBountyPolicyV1 { activation_daa, contest_daa: seal_ttl_daa }),
            provider_liable,
        )
        .map_err(PalwStateV2Error::NativeResource)?;
    // G14 lane CLOCK: X3 governs the kernel route's claims too — injected at every load (not in the header or root); a claim committed
    // at or after X3 keeps its liability horizon at least `committed + 3,120 + court + grace` (`misaka_palw_kernel::claim_clock`).
    ledger.set_claim_clock_from_daa_v1(builder.params.claim_hard_deadline_v1().map(|m| m.from_daa));
    // Each load reconstructs authenticated open targets. No object supplies this inventory.
    if let Some(fence) = extras.beacon_source.filter(|f| f.activation.is_active(ctx.daa_score)) {
        let inputs =
            builder.state.kernel_route.as_ref().expect("created").beacon_source_inputs_v4(fence, ctx.daa_score).map_err(refused)?;
        ledger.set_beacon_source_inputs_reserved_v1(inputs).map_err(PalwStateV2Error::NativeResource)?.map_err(refused)?;
    }
    if let Some(fence) = extras.beacon_source_fee.filter(|f| f.activation.is_active(ctx.daa_score)) {
        let inputs = builder
            .state
            .kernel_route
            .as_ref()
            .expect("created")
            .beacon_source_fee_inputs_v1(fence, &ledger.beacon_source_inputs, ctx.daa_score)
            .map_err(refused)?;
        ledger.set_beacon_source_fee_inputs_reserved_v1(inputs).map_err(PalwStateV2Error::NativeResource)?.map_err(refused)?;
    }
    // The paid-claim consumer projection was admitted with the projection's own aux-input inventory.
    builder
        .state
        .kernel_route
        .as_ref()
        .expect("created")
        .apply_paid_claim_consumer_inputs_v2(ledger.paid_consumer_projection_mut_v1())
        .map_err(refused)?;
    // G14 lane F02b: a court-service-bound claim's producer default is held (no slash) without an adopted inclusion premise or
    // while a reserved unit has no recorded supply (S3c'), and released without slash at its hard deadline.
    // G14 S3c''': and no default without the producer's opportunity under the premise (`kernel_court_default_held_v2`).
    let held = builder.state.kernel_court_default_held_v2(builder.extras.court_service_inclusion_premise_v1.as_ref(), &ledger);
    if !held.is_empty() {
        ledger.install_court_default_held_v1(held).map_err(PalwStateV2Error::NativeResource)?;
    }
    let caps = builder.kernel_final_reward_caps_v1(&ledger);
    ledger.install_final_reward_caps_v1(caps).map_err(PalwStateV2Error::NativeResource)?;
    // RFC08 R8-ECON (`palw_minimum_price_v1`): every governed job's contracted claim carries its signed funded minimum as a payout
    // floor (a Final the escrow cannot fund at it is neutral, never paid less). Empty below the fence.
    let floors = super::palw_econ_contracts_fold_v1::minimum_price_payout_floors_v1(builder, ctx, &ledger);
    ledger.install_minimum_payouts_v1(floors).map_err(PalwStateV2Error::NativeResource)?;
    // RFC-0015 admission is no longer a list read here: a class is admitted to the mode at its own registration, and only while
    // DERIVED-eligible (`opv_gate_v1`), so the ledger's admitted rows are exactly the classes that were eligible when they registered.
    Ok(ledger)
}

/// **The rows `load_ledger`'s ledger has, without serializing it** (GAP 8): the state's ledger rows with the two consumer-derived
/// tables the load sets (the attested artifacts and the admitted OPV classes) replaced by the ledger's own — exactly
/// `ledger.to_rows()` straight after `load_ledger`.
pub(super) fn loaded_rows(builder: &mut TransitionBuilder<'_>, ledger: &KernelLedgerV1) -> Result<LedgerRowsV1, PalwStateV2Error> {
    use misaka_palw_kernel::rows::{TABLE_ATTESTED_V1, TABLE_FLAT_CONFORMANCE_CLASSES_V9, TABLE_OPV_ADMITTED_V1};
    // RFC-0006 (RFC06-1): the copy is admitted before it exists — exact clones of the kept route rows, and one encoded key a
    // consumer-derived row.
    if builder.native_writers_tracking_v1() {
        let kept = builder.state.kernel_route.as_ref().map_or((0, 0, 0), |k| {
            rows_shape_v1(k.rows.iter().filter(|((table, _), _)| {
                !matches!(*table, TABLE_ATTESTED_V1 | TABLE_OPV_ADMITTED_V1 | TABLE_FLAT_CONFORMANCE_CLASSES_V9)
            }))
        });
        let derived = (ledger.attested_artifacts.len() as u64).saturating_add(ledger.opv.admitted.len() as u64);
        reserve_rows_copy_v1(builder, kept, derived)?;
    }
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
    Ok(rows)
}

/// RFC-0006 (RFC06-1): the audited longest ledger row key (`inventory_ledger_expansion_v1` refuses any longer key).
const PALW_KERNEL_ROW_KEY_MAX_BYTES_V1: u64 = 137;

fn rows_envelope_v1() -> u64 {
    std::mem::size_of::<crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>() as u64
}

/// `(rows, key bytes, row bytes)` of a row iterator.
fn rows_shape_v1<'a>(rows: impl Iterator<Item = (&'a (u8, Vec<u8>), &'a Vec<u8>)>) -> (u64, u64, u64) {
    rows.fold((0u64, 0u64, 0u64), |(n, k, v), ((_, key), row)| {
        (n.saturating_add(1), k.saturating_add(key.len() as u64), v.saturating_add(row.len() as u64))
    })
}

/// RFC-0006 (RFC06-1): admit a row-map copy — `shape` cloned rows (one node envelope each; a clone keeps exact capacity) and
/// `encoded` rows whose key is freshly encoded (pinned Borsh: a 1,024-byte serializer capacity) with an empty row.
fn reserve_rows_copy_v1(builder: &mut TransitionBuilder<'_>, shape: (u64, u64, u64), encoded: u64) -> Result<(), PalwStateV2Error> {
    let (n, keys, rows) = shape;
    let env = rows_envelope_v1();
    let mut q = super::native_writers::NativeWriteQuoteV1::new("kernel ledger rows copy");
    q.exact::<u8>(n.saturating_mul(env).saturating_add(keys).saturating_add(rows))?
        .exact::<u8>(encoded.saturating_mul(env.saturating_add(1024)))?;
    builder.admit_native_write_v1(q)
}

/// RFC-0006 (RFC06-1): admit a copy of the route's whole row map (the `before` of an EXEC reward hook). A no-op with tracking off.
pub(super) fn reserve_route_rows_copy_v1(builder: &mut TransitionBuilder<'_>) -> Result<(), PalwStateV2Error> {
    if !builder.native_writers_tracking_v1() {
        return Ok(());
    }
    let shape = builder.state.kernel_route.as_ref().map_or((0, 0, 0), |k| rows_shape_v1(k.rows.iter()));
    reserve_rows_copy_v1(builder, shape, 0)
}

/// **RFC-0006 (RFC06-1): admit a flush's scratch before it exists** — `ledger.to_rows()` (every row one map node, a freshly encoded
/// key and a freshly encoded row: pinned Borsh, at least 1,024 bytes and at most twice the row) and `diff_rows` (one change per row
/// on either side, each holding exact clones of its key and rows), plus the removed v9 keys.
///
/// The rows after the action are bounded by the rows before plus what the ledger's own tickets admitted: every ledger row was
/// admitted as at least one map-node envelope of the smallest ledger membership (`[u8; 64]` keys), and every row's encoding is no
/// longer than the typed value the tickets admitted (Borsh never encodes a value longer than its inline size plus its heap). So
/// `rows_after ≤ rows_before + admitted / envelope` and `row_bytes_after ≤ row_bytes_before + admitted` — an over-count (the
/// tickets also hold the rows already counted before), never an under-count. A no-op with tracking off.
fn reserve_flush_scratch_v1(
    builder: &mut TransitionBuilder<'_>,
    ledger: &crate::palw_native_resource_v1::ReservedKernelLedgerV1,
    before: &LedgerRowsV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_native_resource_v1::typed::BTreeNodeEnvelopeV1;
    use misaka_palw_kernel::rows::TABLE_FLAT_CONFORMANCE_CLASSES_V9;
    if !builder.native_writers_tracking_v1() {
        return Ok(());
    }
    let (n, keys, rows) = rows_shape_v1(before.iter());
    let admitted = ledger.admitted_memory_v1();
    let smallest = (std::mem::size_of::<BTreeNodeEnvelopeV1<[u8; 64], ()>>() as u64).max(1);
    let rows_after = n.saturating_add(admitted / smallest);
    let bytes_after = rows.saturating_add(admitted);
    let env = rows_envelope_v1();
    let (v9, v9_keys, _) = builder.state.kernel_route.as_ref().map_or((0, 0, 0), |k| {
        rows_shape_v1(k.rows.range((TABLE_FLAT_CONFORMANCE_CLASSES_V9, Vec::new())..(TABLE_FLAT_CONFORMANCE_CLASSES_V9 + 1, Vec::new())))
    });
    let mut q = super::native_writers::NativeWriteQuoteV1::new("kernel ledger flush scratch");
    // to_rows
    q.exact::<u8>(rows_after.saturating_mul(env.saturating_add(2048)))?.exact::<u8>(bytes_after.saturating_mul(2))?;
    // diff_rows
    q.grown::<misaka_palw_kernel::rows::RowChangeV1>(rows_after.saturating_add(n))?
        .exact::<u8>(
            keys.saturating_add(rows_after.saturating_mul(PALW_KERNEL_ROW_KEY_MAX_BYTES_V1))
                .saturating_add(rows)
                .saturating_add(bytes_after),
        )?;
    // removed v9 keys
    q.grown::<Vec<u8>>(v9)?.exact::<u8>(v9_keys)?;
    builder.admit_native_write_v1(q)
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

/// **The derived OPV eligibility at the kernel route's door** (`docs/design/palw/verification/opv-beacon-bootstrap.md` §5.3): a registration under
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

/// **The detection facts of a kernel claim commit** (lane g14-court-capacity-r2): its class and the facts its policy plans on, for
/// EVERY claim kind, N from the claim's priced structure — its evaluated positions:
///
/// | kind | `work_leaves` (what the claim commits) | `context_priced_leaves` (what its job prices) |
/// |---|---|---|
/// | single-program (plain, salted) | `commitments.len()` (one entry per evaluated position) | `prompt + generated − 1` (the job's stream) |
/// | segmented (K2-TIR v4/v5) | `prompt_len + generated − 1` | the same (the tiled job's stream; its evidence carries segment roots) |
/// | pipeline | `Σ` stages `commitments.len()` | the same (each stage's evaluated positions) |
/// | typed `Memory` | `Σ` chunk lengths (every step runs over its chunk) | the same |
/// | typed `Retrieval` | `max(1, result.len())` (one scored entry each) | the same |
/// | typed `Composite` | `Σ` model stages' `commitments.len()` + `Σ` retrieval stages' `result.len()` | the same |
///
/// A plain claim whose committed positions are not its job's stream is refused by the policy's price check (`NotAtTheClaimsPrice`),
/// as a free-prompt claim's is. The collateral is the route's per-claim reservation (`claim_collateral`, what a conviction slashes).
/// `None` for an object that commits no claim, or one whose job the ledger does not know (the kernel refuses it by its own rule).
fn kernel_claim_detection_facts_v1(
    ledger: &KernelLedgerV1,
    object: &KernelRouteObjectV1,
) -> Option<(misaka_palw_kernel::hash::Digest, crate::palw_detection_policy_v1::PalwDetectionClaimFactsV1)> {
    use misaka_palw_kernel::route::SaltedCommitV1 as C;
    use misaka_palw_kernel::spec::{SpecClaimV1 as SC, SpecJobV1 as SJ};
    let u32_of = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let facts = |work: u64, context: u64, prompt: usize, decode: usize| crate::palw_detection_policy_v1::PalwDetectionClaimFactsV1 {
        attempt_lane: false,
        work_leaves: work,
        context_priced_leaves: Some(context),
        prompt_tokens: u32_of(prompt),
        decode_tokens_executed: u32_of(decode),
        collateral_sompi: u128::from(ledger.policy.claim_collateral),
    };
    let stream = |prompt: usize, generated: usize| (prompt as u64 + generated as u64).saturating_sub(1);
    match object {
        KernelRouteObjectV1::CommitClaim { claim, commitments, .. }
        | KernelRouteObjectV1::CommitClaimSalted { commit: C::Claim { claim, commitments, .. }, .. } => {
            let job = ledger.jobs.get(&claim.job_id)?;
            let context = stream(job.prompt.len(), claim.generated.len());
            Some((job.class_binding_id, facts(commitments.len() as u64, context, job.prompt.len(), claim.generated.len())))
        }
        KernelRouteObjectV1::CommitSegmentedClaim { claim, .. }
        | KernelRouteObjectV1::CommitClaimSalted { commit: C::Segmented { claim, .. }, .. } => {
            let row = ledger.tiled_jobs.get(&claim.job_id)?;
            let positions = stream(row.job.prompt_len as usize, claim.generated.len());
            Some((row.job.class_binding_id, facts(positions, positions, row.job.prompt_len as usize, claim.generated.len())))
        }
        KernelRouteObjectV1::CommitPipelineClaim { claim, stages, .. }
        | KernelRouteObjectV1::CommitClaimSalted { commit: C::Pipeline { claim, stages, .. }, .. } => {
            let job = ledger.pipeline_jobs.get(&claim.job_id)?;
            let positions: u64 = stages.iter().map(|s| s.commitments.len() as u64).sum();
            Some((job.class_binding_id, facts(positions, positions, job.facts.prompt.len(), claim.generated.len())))
        }
        KernelRouteObjectV1::Spec { object: misaka_palw_kernel::spec::SpecObjectV1::CommitClaim { claim } }
        | KernelRouteObjectV1::CommitClaimSalted { commit: C::Spec { claim }, .. } => {
            let job = ledger.typed.jobs.get(&claim.job_id())?;
            let (class, positions, prompt, decode) = match (job, claim) {
                (SJ::Memory(j), SC::Memory(c)) => {
                    (j.class, j.chunks.iter().map(|c| c.len() as u64).sum::<u64>(), 0, c.generated.len())
                }
                (SJ::Retrieval(j), SC::Retrieval(c)) => (j.class, (c.result.len() as u64).max(1), 0, 0),
                (SJ::Composite(j), SC::Composite(c)) => {
                    use misaka_palw_kernel::spec::composite::StageClaimV1 as S;
                    let units = c
                        .stages
                        .iter()
                        .map(|s| match s {
                            S::Model { commitments, .. } => commitments.len() as u64,
                            S::Retrieval { result, .. } => result.len() as u64,
                        })
                        .sum::<u64>();
                    (j.class, units, j.prompt.len(), 0)
                }
                // A claim of another kind than its job: the kernel refuses it by its own rule.
                _ => return None,
            };
            Some((class, facts(positions, positions, prompt, decode)))
        }
        _ => None,
    }
}

/// **The detection policy at the kernel route's claim door** (`palw_detection_policy_v1`, lane g14-court-capacity-r2; g14-detection-
/// policy §10 item 2): past the fence a kernel claim commit of ANY kind is DROPPED (nothing written, as `opv_gate_v1` drops an
/// ineligible class's claim: no claim, no reward, weight or consensus credit) when its class has no armed policy, or when the
/// policy's full plan refuses it on the facts its priced structure gives ([`kernel_claim_detection_facts_v1`]): collateral below the
/// deterrence bound, N above `n_max`, a draw its declared resources cannot carry, committed positions not at its job's price. `false`
/// below the fence and for every other object. The kernel route adjudicates in its own ledger court, so the policy's declared court
/// / DA needs stand (`palw_court_claim_budget_v1` is the V2 route's reservation).
fn detection_drops_kernel_claim_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    ledger: &KernelLedgerV1,
    object: &KernelRouteObjectV1,
) -> bool {
    let Some(fence) = builder.params.detection_policy_v1() else { return false };
    if !fence.activation.is_active(ctx.daa_score) {
        return false;
    }
    let Some((class, facts)) = kernel_claim_detection_facts_v1(ledger, object) else { return false };
    if crate::palw_detection_policy_v1::palw_detection_claim_refusal_with_budget_v1(
        builder.params,
        &Hash64::from_bytes(class),
        ctx.daa_score,
        &facts,
        None,
    )
    .is_some()
    {
        return true;
    }
    // **No public checker, no reward** (G14, lane g14-court-capacity-r2): the node's public kernel verifier
    // (`kaspad::palw_kernel_verifier`) re-executes single-program, segmented and typed `Memory` claims; a pipeline, a typed
    // `Composite` or a typed `Retrieval` claim needs more than one model (or a snapshot) and no node service checks or files on it.
    // Past the fence such a claim is not reward-bearing until a checker exists — the option (b) rule for a route with no public
    // conviction path.
    !kernel_claim_kind_publicly_checked_v1(object)
}

/// **Does a node's public kernel verifier check this claim kind?** Single-program (plain, salted), segmented and typed `Memory`:
/// yes; pipeline, typed `Composite` and typed `Retrieval`: no (`kaspad::palw_kernel_verifier`'s module doc).
fn kernel_claim_kind_publicly_checked_v1(object: &KernelRouteObjectV1) -> bool {
    use misaka_palw_kernel::route::SaltedCommitV1 as C;
    use misaka_palw_kernel::spec::SpecClaimV1 as SC;
    match object {
        KernelRouteObjectV1::CommitPipelineClaim { .. } | KernelRouteObjectV1::CommitClaimSalted { commit: C::Pipeline { .. }, .. } => false,
        KernelRouteObjectV1::Spec { object: misaka_palw_kernel::spec::SpecObjectV1::CommitClaim { claim } }
        | KernelRouteObjectV1::CommitClaimSalted { commit: C::Spec { claim }, .. } => matches!(claim, SC::Memory(_)),
        _ => true,
    }
}

/// Write back every ledger row that changed and the scalars; `before` is the rows as `load_ledger` found them. The ledger is then
/// cached on the route state (GAP 8) as the rows' own ledger, so the next object of the chain loads it without rebuilding it.
pub(super) fn flush(
    builder: &mut TransitionBuilder<'_>,
    ledger: &crate::palw_native_resource_v1::ReservedKernelLedgerV1,
    before: &LedgerRowsV1,
) -> Result<(), PalwStateV2Error> {
    // RFC-0006 (RFC06-1): the to_rows map, the diff and the removed v9 keys are admitted before any of them exists.
    reserve_flush_scratch_v1(builder, ledger, before)?;
    let ledger: &KernelLedgerV1 = ledger;
    for ((table, key), old_new) in diff_rows(before, &ledger.to_rows()).into_iter().map(|(k, _old, new)| (k, new)) {
        builder.write_kernel_row(table, key, old_new)?;
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
        builder.write_kernel_row(TABLE_FLAT_CONFORMANCE_CLASSES_V9, key, None)?;
    }
    let header = builder.state.kernel_route.as_ref().expect("a route that flushes exists").header.clone();
    let scalars = ledger.scalars();
    if header.scalars != scalars {
        builder.write_kernel_header(PalwKernelRouteHeaderV1 { scalars, ..header })?;
    }
    let kernel = builder.state.kernel_route.as_mut().expect("a route that flushes exists");
    // Cache refill is optional. Reserve its separate deep payload before cloning;
    // failure evicts the cache and leaves the committed rows authoritative.
    let Ok(cached) = crate::palw_native_resource_v1::ledger_cache_clone_v1(ledger) else {
        kernel.ledger_cache.clear();
        return Ok(());
    };
    let cached = cached.as_rebuilt_from_v1(&kernel.rows);
    if kernel.rows.keys().any(|(t, _)| *t == TABLE_FLAT_CONFORMANCE_CLASSES_V9) {
        kernel.ledger_cache.set_native_projection(cached);
    } else {
        kernel.ledger_cache.set(cached);
    }
    Ok(())
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
    reserve_route_rows_copy_v1(builder)?;
    let route = builder.state.kernel_route.as_ref().expect("EXEC kernel binding");
    let before = route.rows.clone();
    let mut ledger =
        route.ledger_checked_v1().map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
    ledger.set_daa_v1(daa);
    let until = exec_liability_floor_v1(expiry, builder.params.window_court(), &ledger.policy)?;
    let row = ledger.claims.get(&claim.as_bytes()).ok_or_else(|| refused("the EXEC reward claim is missing"))?;
    require_exec_provider_horizon_v1(&builder.state, &claim, row, daa, until).map_err(refused)?;
    let bond = route.bond_key_of(&row.producer).ok_or_else(|| refused("EXEC income recipient has no native bond"))?;
    ledger
        .bind_exec_reward_until_reserved_v1(root.as_bytes(), claim.as_bytes(), expiry, until)
        .map_err(PalwStateV2Error::NativeResource)?
        .map_err(refused)?;
    builder.transfer_kernel_income_to_exec_v1(root, claim, bond, daa, until)?;
    flush(builder, &ledger, &before)?;
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
    reserve_route_rows_copy_v1(builder)?;
    let route = builder.state.kernel_route.as_ref().expect("EXEC kernel binding");
    let before = route.rows.clone();
    let mut ledger =
        route.ledger_checked_v1().map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
    ledger.set_daa_v1(daa);
    let ids: Vec<_> = ledger.bonds.keys().copied().collect();
    for id in ids {
        if let Some(key) = builder.state.kernel_route.as_ref().and_then(|r| r.bond_key_of(&id)) {
            ledger.try_sync_bond_v1(id, builder.kernel_synced_collateral(&key, daa)).map_err(PalwStateV2Error::NativeResource)?;
        }
    }
    let mut events = Vec::new();
    let mut paid = Vec::new();
    let mut remaining = caps.clone();
    for (claim, bond) in claims {
        let cap = remaining.entry(*bond).or_default();
        let fee = ledger
            .finish_exec_reward_reserved_v1(root.as_bytes(), claim.as_bytes(), *cap, &mut events)
            .map_err(PalwStateV2Error::NativeResource)?
            .map_err(refused)?;
        *cap -= fee;
        paid.push(fee);
    }
    apply_settlements_to_exec_vesting(builder, &events, true, Some(root), daa)?;
    flush(builder, &ledger, &before)?;
    Ok(paid)
}

pub(super) fn cancel_exec_rewards_v1(builder: &mut TransitionBuilder<'_>, root: Hash64, daa: u64) -> Result<(), PalwStateV2Error> {
    reserve_route_rows_copy_v1(builder)?;
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let before = route.rows.clone();
    let mut ledger =
        route.ledger_checked_v1().map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
    ledger.set_daa_v1(daa);
    let claims: Vec<_> =
        ledger.exec_reward_holds.iter().filter(|(_, hold)| hold.owner_root == root.as_bytes()).map(|(id, _)| *id).collect();
    let mut events = Vec::new();
    for claim in claims {
        ledger
            .cancel_exec_reward_reserved_v1(root.as_bytes(), claim, &mut events)
            .map_err(PalwStateV2Error::NativeResource)?
            .map_err(refused)?;
    }
    apply_settlements(builder, &events, true, daa)?;
    flush(builder, &ledger, &before)?;
    Ok(())
}

/// **lane CERT: the producer's escrow Final reward starting at `index`**: `PayJobEscrow(poster, x)`, then — only when the escrow held
/// more than was paid — `ReleaseJobEscrow(poster)`, then `FinalReward(producer, x)` of the same claim, NOT followed by `CheckFeePaid`
/// (a verifier's check fee). The kernel's `settle` drops zero-amount instructions, so a fully paid escrow emits no release; this
/// reader accepts both shapes. Returns `(index of the FinalReward, claim, poster)`. Pure.
fn kernel_producer_final_reward_v1(
    events: &[LedgerEventV1],
    index: usize,
) -> Option<(usize, misaka_palw_kernel::hash::Digest, misaka_palw_kernel::hash::Digest)> {
    let settlement = |i: usize| match events.get(i) {
        Some(LedgerEventV1::Settlement(s)) => Some(*s),
        _ => None,
    };
    let pay = settlement(index).filter(|s| s.kind == SettlementKindV1::PayJobEscrow)?;
    let claim = pay.claim?;
    let mut at = index + 1;
    if settlement(at).is_some_and(|s| s.kind == SettlementKindV1::ReleaseJobEscrow && s.claim == Some(claim) && s.bond == pay.bond) {
        at += 1;
    }
    settlement(at).filter(|s| s.kind == SettlementKindV1::FinalReward && s.claim == Some(claim) && s.amount == pay.amount)?;
    let check_fee = matches!(events.get(at + 1), Some(LedgerEventV1::CheckFeePaid { claim: c, .. }) if *c == claim);
    (!check_fee).then_some((at, claim, pay.bond))
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
    // Fail closed on paid-target read failures before any earlier settlement
    // in this batch can write. Genuine absence and a dormant fence are no-ops.
    if builder.params.paid_claim_trap_active_at(now) {
        for ordinal in 0..events.len() {
            if let Some((claim, _, _, _)) = super::palw_settlement_once_fold_v1::kernel_check_fee_at_v1(events, ordinal) {
                super::palw_paid_claim_trap_fold_v1::check_fee_hold_until_checked_v1(builder, &claim, now)?;
            }
        }
    }
    // GAP-5: what this batch actually debited from posters' escrows — the only source a Final reward is paid from. A `FinalReward`
    // with no spent escrow behind it is never paid (there is no unfunded reward path).
    let mut escrow_debited: std::collections::BTreeMap<Option<misaka_palw_kernel::hash::Digest>, u64> = Default::default();
    // Persistent source escrows were debited in an earlier fold. Their backing is the rooted funding row, never a later
    // sponsor balance or this batch's job escrow. All fee instructions are strict, including sweeps.
    let fee_ledger = if events.iter().any(|e| {
        matches!(e, LedgerEventV1::Settlement(s) if matches!(s.kind,
        SettlementKindV1::FundSourceFeeEscrow | SettlementKindV1::PaySourceFeeEscrow | SettlementKindV1::RefundSourceFeeEscrow))
    }) {
        Some(
            builder
                .state
                .kernel_route
                .as_ref()
                .ok_or_else(|| refused("source fee route missing"))?
                .ledger_checked_v1()
                .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?,
        )
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
    // RFC-0008 once-only settlement (dormant `palw_settlement_once_v1`): the event indices of a producer reward triple whose
    // escrow reward another consumer already settled — its release and reward are skipped with its debit.
    let mut already_settled: std::collections::BTreeSet<usize> = Default::default();
    // RFC-0008 (RFC08-1): the next slot ordinal of each `(claim, verifier)` check fee in this batch (the kernel pays every unpaid slot
    // of one verifier in one batch, so the occurrence order is the slot order).
    let mut check_fee_slots: std::collections::BTreeMap<([u8; 64], [u8; 64]), u64> = Default::default();
    // RFC-0007 §4.1 T6 (dormant `palw_paid_claim_trap_v1`): the check fees of paid V2 claims this batch HOLDS, by the ordinal of
    // their poster debit (the held row records what that debit actually took). Empty below the fence.
    let mut held_fees: std::collections::BTreeMap<usize, crate::palw_paid_claim_trap_v1::PalwHeldCheckFeeRowV1> = Default::default();
    // lane CERT (Gate C on the kernel route): the event index of each producer escrow `FinalReward` in this batch, with its claim and
    // poster, so the reward is held for SecurityCertified instead of paid at the ledger's `Final` (a verifier check fee is not one).
    let mut producer_final_rewards: std::collections::BTreeMap<usize, ([u8; 64], [u8; 64])> = Default::default();
    for (ordinal, event) in events.iter().enumerate() {
        let LedgerEventV1::Settlement(s) = event else { continue };
        if already_settled.contains(&ordinal) {
            continue;
        }
        // **One check fee per drawn slot, across every consumer** (RFC08-1): past `palw_settlement_once_v1` the marker of
        // `VerifierCheckFee(claim, verifier, slot)` — not the kernel's per-slot `paid` flag — is what stops a second payment. It is
        // checked and written at the poster's debit, before the reward it funds; a repeat is refused (strict) or skipped whole
        // (lenient: the debit, the release and the reward). `Dormant` below the fence.
        if s.kind == SettlementKindV1::PayJobEscrow
            && let Some((claim, poster, verifier, fee)) = super::palw_settlement_once_fold_v1::kernel_check_fee_at_v1(events, ordinal)
        {
            use crate::palw_settlement_once_v1::{
                PalwSettlementConsumerV1 as C, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
            };
            let consumer = match (exec_root.is_some(), strict) {
                (true, _) => C::ExecKernelFee,
                (false, true) => C::KernelObject,
                (false, false) => C::KernelTick,
            };
            let slot = check_fee_slots.entry((claim, verifier)).or_insert(0);
            let slot_ordinal = *slot;
            let obligation = PalwSettlementObligationV1::with_party(PalwSettlementLegV1::VerifierCheckFee, claim, verifier, slot_ordinal);
            *slot = slot.checked_add(1).ok_or(PalwStateV2Error::Overflow("check fee slot ordinal"))?;
            if let Some(release_after_daa) = super::palw_paid_claim_trap_fold_v1::check_fee_hold_until_checked_v1(builder, &claim, now)? {
                // RFC-0007 §4.1 T6 (dormant `palw_paid_claim_trap_v1`): a paid V2 claim's check fee is HELD — the poster's escrow is
                // debited now, the verifier's `FinalReward` is not paid; the fee is released once, after the claim's trap reveal
                // window (the verifier's liability horizon), less the forfeiture if the court convicts a claim it signed Clean.
                if !super::palw_paid_claim_trap_fold_v1::check_fee_hold_admits_v1(builder, &obligation)? {
                    if strict {
                        return Err(refused("the verifier's check fee for this slot is already held or settled"));
                    }
                    already_settled.extend([ordinal + 1, ordinal + 2]);
                    continue;
                }
                held_fees.insert(
                    ordinal,
                    crate::palw_paid_claim_trap_v1::PalwHeldCheckFeeRowV1 {
                        version: crate::palw_paid_claim_trap_v1::PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
                        claim,
                        verifier,
                        slot: slot_ordinal,
                        poster,
                        fee,
                        release_after_daa,
                    },
                );
                already_settled.insert(ordinal + 2);
            } else {
                // RFC08-2: outside a paid V2 claim past the trap fence a drawn verifier stands behind no claim of its own for its
                // attestation, so its fee has no liability terms to source — refused past the fence, never defaulted.
                let once = builder.settle_once_liable_v1(
                    now,
                    obligation,
                    consumer,
                    fee,
                    PalwSettlementDebitV1::Escrow { poster, amount: fee },
                    super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced(
                        "RFC07-5: a drawn verifier's own liability horizon for its attestation is undefined",
                    ),
                )?;
                if !once.pays() {
                    if strict {
                        return Err(refused(match once {
                            super::palw_settlement_once_fold_v1::PalwSettleOnceV1::Refused(why) => why.name(),
                            _ => "the verifier's check fee for this slot was already settled by another consumer",
                        }));
                    }
                    already_settled.extend([ordinal + 1, ordinal + 2]);
                    continue;
                }
            }
        }
        // **One escrow reward per kernel claim, across every consumer** (the kernel tick, a kernel object arm, the paid-claim Final
        // batch, the EXEC kernel-fee route): the marker of `EscrowFinalReward(claim)` is checked and written at the poster's debit,
        // before the slash and the payout it funds. A second settlement of the same claim moves nothing: a strict object arm is
        // refused (dropped), a lenient tick skips the whole triple. `Dormant` below the fence. A verifier check fee (the same triple
        // followed by `CheckFeePaid`) is its own per-slot leg (`VerifierCheckFee`, above) and is not this one.
        if s.kind == SettlementKindV1::PayJobEscrow
            && let Some((claim, poster, _producer, paid)) =
                super::palw_settlement_once_fold_v1::kernel_producer_reward_at_v1(events, ordinal)
        {
            use crate::palw_settlement_once_v1::{
                PalwSettlementConsumerV1 as C, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
            };
            let consumer = match (exec_root.is_some(), strict) {
                (true, _) => C::ExecKernelFee,
                (false, true) => C::KernelObject,
                (false, false) => C::KernelTick,
            };
            // RFC08-2: paid straight to the producer's queue at once — or, on the EXEC kernel-fee route, held in the root's
            // reward row until its own horizon (that release is the ExecLegPayment leg, under the predicate again).
            let leg = match exec_root {
                Some(root) => match builder.state.exec_v2_rewards.get(&root) {
                    Some(row) => super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::KernelClaim {
                        claim,
                        draw: paid,
                        paid_now: 0,
                        vest_until: Some(row.liability_until.saturating_add(1)),
                    },
                    None => super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced(
                        "the EXEC root's reward row (the kernel fee's holding horizon)",
                    ),
                },
                None => super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::KernelClaim {
                    claim,
                    draw: paid,
                    paid_now: paid,
                    vest_until: None,
                },
            };
            let once = builder.settle_once_liable_v1(
                now,
                PalwSettlementObligationV1::of(PalwSettlementLegV1::EscrowFinalReward, claim),
                consumer,
                paid,
                PalwSettlementDebitV1::Escrow { poster, amount: paid },
                leg,
            )?;
            if !once.pays() {
                if strict {
                    return Err(refused(match once {
                        super::palw_settlement_once_fold_v1::PalwSettleOnceV1::Refused(why) => why.name(),
                        _ => "the claim's escrow reward was already settled by another consumer",
                    }));
                }
                already_settled.extend([ordinal + 1, ordinal + 2]);
                continue;
            }
        }
        if s.kind == SettlementKindV1::PayJobEscrow
            && exec_root.is_none()
            && let Some((reward_at, claim, poster)) = kernel_producer_final_reward_v1(events, ordinal)
        {
            producer_final_rewards.insert(reward_at, (claim, poster));
        }
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
                    // **RFC-0008 once-only settlement (RFC08-1; dormant `palw_settlement_once_v1`)**: the promise `(context,
                    // producer)` is paid once across every consumer and batch, not only within this batch (`fee_paid`). Every fee
                    // instruction is strict, so a repeat or a refusal refuses the batch. RFC08-2: the sponsor-signed terms carry a
                    // hold and a final-by DAA but a beacon source has no claim reservation, X3 deadline or material TTL — no
                    // authentic source for the predicate's terms, so past the fence the payout is refused, never defaulted.
                    let once = builder.settle_once_liable_v1(
                        now,
                        crate::palw_settlement_once_v1::PalwSettlementObligationV1::with_party(
                            crate::palw_settlement_once_v1::PalwSettlementLegV1::SourceFeePromise,
                            context,
                            s.bond,
                            0,
                        ),
                        match (exec_root.is_some(), strict) {
                            (true, _) => crate::palw_settlement_once_v1::PalwSettlementConsumerV1::ExecKernelFee,
                            (false, true) => crate::palw_settlement_once_v1::PalwSettlementConsumerV1::KernelObject,
                            (false, false) => crate::palw_settlement_once_v1::PalwSettlementConsumerV1::KernelTick,
                        },
                        s.amount,
                        crate::palw_settlement_once_v1::PalwSettlementDebitV1::Prepaid { sponsor: funding.terms.sponsor, amount: s.amount },
                        super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced(
                            "a beacon source's fee: no claim reservation, X3 deadline or material TTL backs the source",
                        ),
                    )?;
                    match once {
                        // Withheld by the predicate (a policy outcome, not an inconsistency): the producer is not paid and the
                        // obligation is closed at zero. audit A-4: the sponsor's fee was already debited (`FundSourceFeeEscrow`)
                        // and the kernel ledger counts this promise as discharged (its escrow `held` drops by the fee, no refund
                        // instruction ever follows), so the withheld fee goes back to the SPONSOR here — once, because the zero
                        // marker makes any retry `AlreadySettled` — instead of being destroyed.
                        super::palw_settlement_once_fold_v1::PalwSettleOnceV1::Refused(_) => {
                            let held = fee_held.get_mut(&context).ok_or_else(|| refused("source fee escrow missing"))?;
                            *held = held.checked_sub(s.amount).ok_or_else(|| refused("source fee payout exceeds prepaid escrow"))?;
                            let sponsor = funding.terms.sponsor;
                            let payload = builder
                                .state
                                .kernel_route
                                .as_ref()
                                .and_then(|k| k.bond_key_of(&sponsor))
                                .and_then(|k| builder.state.bonds.get(&k).map(|b| b.payout_payload))
                                .ok_or_else(|| refused("source fee sponsor payee missing"))?;
                            builder.add_kernel_payout(payload, s.amount)?;
                            continue;
                        }
                        super::palw_settlement_once_fold_v1::PalwSettleOnceV1::AlreadySettled(_) => {
                            return Err(refused("the source fee promise was already settled by another consumer"));
                        }
                        _ => {}
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
                let debit = match super::native_writers::local_capacity_or_v1(builder.slash_bond(key, s.amount as u128))? {
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
                    if let Some(mut held) = held_fees.remove(&ordinal) {
                        // RFC-0007 §4.1 T6: a held check fee's debit is the held row, never any Final reward's funding.
                        held.fee = debit;
                        super::palw_paid_claim_trap_fold_v1::write_held_check_fee_v1(builder, &held)?;
                    } else {
                        let funded = escrow_debited.entry(s.claim).or_default();
                        *funded = funded.checked_add(debit).ok_or(PalwStateV2Error::Overflow("kernel escrow debit"))?;
                    }
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
                // lane CERT: past Gate C a kernel claim's `Final` is not SecurityCertified — its escrow reward (debited above) is
                // held in its certification's `'H'` row and paid only when the drawn verifiers certify it (returned otherwise).
                if s.kind == SettlementKindV1::FinalReward
                    && let Some((claim, poster)) = producer_final_rewards.get(&ordinal).copied()
                    && super::palw_security_certification_fold_v1::hold_kernel_final_reward_v1(builder, now, &claim, &s.bond, &poster, amount)?
                {
                    continue;
                }
                if s.kind == SettlementKindV1::FinalReward {
                    let claim = s.claim.ok_or_else(|| refused("a Final fee has no claim"))?;
                    let bond = key.ok_or_else(|| refused("a Final fee recipient has no native bond"))?;
                    let payload =
                        builder.state.bonds.get(&bond).ok_or_else(|| refused("a Final fee recipient is missing"))?.payout_payload;
                    builder.consume_kernel_final_reward_v1(&claim, bond, payload, now, amount)?;
                    // RFC-0016 (dormant): the escrow payout telemetry of the claim's model (consensus-derived, never coinbase).
                    super::palw_rfc0016_fold_v1::rfc0016_note_escrow_payout_v1(builder, now, &claim, amount)?;
                }
                let payee = key.and_then(|key| builder.state.bonds.get(&key).map(|b| b.payout_payload));
                match payee {
                    Some(payload) => match super::native_writers::local_capacity_or_v1(builder.add_kernel_payout(payload, amount))? {
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
    // RFC-0007 §4.1 T6 (dormant `palw_paid_claim_trap_v1`): a paid V2 claim's conviction in this batch is recorded once and its
    // Clean receipt signers are charged their liability. A no-op below the fence.
    super::palw_paid_claim_trap_fold_v1::on_paid_claim_convictions_v1(builder, events, now)?;
    // G14 lane closure G-K1: every conviction in this batch charges the claim's receipt signers' liability locks.
    super::palw_g14_lane_closure_fold_v1::charge_kernel_signers_on_conviction_v1(builder, events)?;
    Ok(())
}

/// **The interim seat draw of a freshly committed claim** (module doc). Distinct operators, none the producer's.
fn draw_seats(
    builder: &TransitionBuilder<'_>,
    claim: &misaka_palw_kernel::hash::Digest,
    producer: &PalwBondKeyV2,
    deadline_daa: u64,
    draw_daa: u64,
) -> KernelAssignmentV1 {
    // Lane g14p/kernel-route U02: the seat policy (interim 3-of-3, or Panel=1 past `palw_kernel_route_panel_one_v1`) at the draw.
    let (seat_count, seat_quorum) = builder.params.kernel_route_seats_and_quorum_v1(draw_daa);
    let seed = palw_kernel_interim_seed_v1(claim);
    let challenge = palw_kernel_interim_challenge_v1(claim);
    draw_seats_seeded(builder, producer, deadline_daa, seat_count, seat_quorum, seed, challenge)
}

/// The seat draw under a given lottery seed and challenge pair (the interim one at the commit, or lane SEED's post-commit Panel=1 one).
fn draw_seats_seeded(
    builder: &TransitionBuilder<'_>,
    producer: &PalwBondKeyV2,
    deadline_daa: u64,
    seat_count: usize,
    seat_quorum: u8,
    seed: Hash64,
    (challenge_anchor, sample_seed): (misaka_palw_kernel::hash::Digest, misaka_palw_kernel::hash::Digest),
) -> KernelAssignmentV1 {
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
        .take(seat_count)
        .map(|(_, bond)| KernelSeatV1 { bond, kernel_bond: palw_kernel_bond_id_v1(&bond), scope: ScopeV1::WholeClaim })
        .collect();
    let quorum = seat_quorum.min(seats.len() as u8);
    KernelAssignmentV1 {
        assignment_root: palw_kernel_assignment_root_v1(&seats, quorum),
        seats,
        challenge_anchor,
        sample_seed,
        deadline_daa,
        quorum,
    }
}

/// **lane SEED (G14 independent review MAJOR-1 / MINOR-1): the deferred Panel=1 seat draw.** A Panel-licensed program claim committed
/// at or past `palw_kernel_route_panel_one_v1` has no assignment at its commit. At the first block whose state holds a round seed
/// anchor (ADR-0130 / ADR-0170, the kind RFC-0018 Gate C draws from) recorded strictly after the commit, its one seat, its challenge
/// anchor and its sample seed are drawn from `H(claim ‖ anchor)` — once: the claim must still be `Checking` with no assignment and no
/// counted receipt. A claim no anchor reaches by its check deadline has no seat, so no receipt can cover it: it times out (never
/// `Final`). Below the fence (every claim committed earlier) nothing here runs, and the interim draw at the commit is unchanged.
fn draw_panel_one_seats_v1(builder: &mut TransitionBuilder<'_>, ledger: &KernelLedgerV1) -> Result<(), PalwStateV2Error> {
    let Some(from) = builder.params.kernel_route_panel_one_from_daa() else { return Ok(()) };
    let Some(lane) = builder.extras.round_lane else { return Ok(()) };
    let Some(anchor) = builder.state.round_seed_anchor else { return Ok(()) };
    let mut due: Vec<(misaka_palw_kernel::hash::Digest, PalwBondKeyV2, u64, u64)> = Vec::new();
    {
        let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
        for (claim, row) in ledger.claims.iter() {
            let misaka_palw_kernel::lifecycle::ClaimStateV1::Checking { anchor_daa: committed_daa, deadline_daa } = row.life.state else {
                continue;
            };
            if committed_daa < from
                || !matches!(row.body, ClaimBodyV1::Program { .. })
                || ledger.mode_of_claim(claim) != Some(misaka_palw_kernel::mode::VerificationModeV1::PanelLicensed)
                || route.assignment_of(claim).is_some()
                || !route.receipts_of(claim).is_empty()
                || route.paid_claim_for_base_v2(claim).ok().flatten().is_some()
                || !palw_kernel_panel_one_anchor_after_commit_v1(committed_daa, lane.schedule_span_daa, Some(&anchor))
            {
                continue;
            }
            let producer = route.bond_key_of(&row.producer).ok_or_else(|| refused("a Panel=1 claim's producer has no V2 bond"))?;
            due.push((*claim, producer, committed_daa, deadline_daa));
        }
    }
    for (claim, producer, committed_daa, deadline_daa) in due {
        let (seat_count, seat_quorum) = builder.params.kernel_route_seats_and_quorum_v1(committed_daa);
        let seed = palw_kernel_panel_one_seed_v1(&claim, &anchor);
        let challenge = palw_kernel_panel_one_challenge_v1(&claim, &anchor);
        let assignment = draw_seats_seeded(builder, &producer, deadline_daa, seat_count, seat_quorum, seed, challenge);
        for seat in &assignment.seats {
            builder.note_kernel_bond(&seat.bond)?;
        }
        builder.write_kernel_row(
            PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1,
            key_bytes(&claim),
            Some(borsh::to_vec(&assignment).expect("an assignment serializes")),
        )?;
    }
    Ok(())
}

/// **Tag 110: one kernel route object.** See the module doc.
pub(super) fn apply_kernel_route_object_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    bytes: &[u8],
    signer: &PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    apply_kernel_route_object_authorized_v2(builder, ctx, bytes, signer, false)
}

/// **Lane F02b: the slice's court admissions of this object's committed claims (pure).** `Ok(Some(vec))`: every Panel-licensed
/// program claim committed by this object admitted (empty when no schedule is in force or no claim qualifies); `Ok(None)`: one was
/// refused — the caller drops the object, nothing is written. Each later claim of the same object is reserved on top of the earlier
/// ones (book and capital).
pub(super) fn kernel_court_admissions_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    ledger: &KernelLedgerV1,
    events: &[LedgerEventV1],
    signer: &PalwBondKeyV2,
) -> Result<Option<Vec<crate::palw_kernel_court_admission_v1::KernelCourtAdmissionV1>>, PalwStateV2Error> {
    let Some(schedule) = builder.extras.kernel_court_admission_v1 else { return Ok(Some(Vec::new())) };
    // The slice's own fence: below `palw_kernel_route_panel_one_v1` nothing is admitted or written.
    if builder.params.kernel_route_panel_one_from_daa().is_none_or(|from| ctx.daa_score < from) {
        return Ok(Some(Vec::new()));
    }
    let mut out: Vec<crate::palw_kernel_court_admission_v1::KernelCourtAdmissionV1> = Vec::new();
    for event in events {
        let LedgerEventV1::ClaimCommitted { claim } = event else { continue };
        let row = ledger.claims.get(claim).ok_or_else(|| refused("accepted kernel claim is missing"))?;
        if ledger.mode_of_claim(claim) != Some(misaka_palw_kernel::mode::VerificationModeV1::PanelLicensed)
            || !matches!(row.body, ClaimBodyV1::Program { .. })
        {
            continue;
        }
        let deadline = row.committed_daa + ledger.policy.check_window_daa;
        let assignment = draw_seats(builder, claim, signer, deadline, ctx.daa_score);
        let mut signers: std::collections::BTreeSet<PalwBondKeyV2> = assignment.seats.iter().map(|s| s.bond).collect();
        signers.insert(*signer);
        let collateral = builder.state.bonds.get(signer).map_or(0, |b| b.collateral as u128);
        // Free = collateral less everything committed, less this object's own kernel reservations (not flushed yet), less the
        // capital of earlier admissions in this object.
        let pending: u128 = events
            .iter()
            .filter_map(|e| match e {
                LedgerEventV1::ClaimCommitted { claim } => ledger.claims.get(claim).map(|c| c.reserved as u128),
                _ => None,
            })
            .sum::<u128>()
            .saturating_add(out.iter().map(|a| a.capital).sum::<u128>());
        let free = collateral.saturating_sub(builder.committed_at(signer, ctx.daa_score)).saturating_sub(pending);
        let existing = out.last().map(|a| &a.book).or(builder.state.court_service_state_v1());
        match crate::palw_kernel_court_admission_v1::kernel_court_admission_candidate_v1(
            existing,
            Hash64::from_bytes(*claim),
            Hash64::from_bytes(row.job_id),
            Hash64::from_bytes(row.class_binding_id),
            row.committed_daa,
            &signers,
            *signer,
            free,
            &schedule,
        ) {
            Ok(admission) => out.push(admission),
            Err(_) => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// **Lane F02b: write the admitted book and every capital lock** (journalled; reorg undoes them with the block).
pub(super) fn write_kernel_court_admissions_v1(
    builder: &mut TransitionBuilder<'_>,
    admissions: Vec<crate::palw_kernel_court_admission_v1::KernelCourtAdmissionV1>,
) -> Result<(), PalwStateV2Error> {
    let Some(last) = admissions.last() else { return Ok(()) };
    builder.write_court_service_state_v1(last.book.clone())?;
    use crate::palw_kernel_court_admission_v1::*;
    for a in &admissions {
        let claim = a.claim;
        let key = kernel_court_capital_key_v1(&a.payer, claim);
        if builder.state.kernel_route.as_ref().is_some_and(|k| k.aux.contains_key(&(PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1, key.clone()))) {
            return Err(refused("kernel court capital row already exists"));
        }
        let row = KernelCourtCapitalRowV1 {
            version: KERNEL_COURT_CAPITAL_ROW_VERSION_V1,
            payer: a.payer,
            claim,
            amount: a.capital,
            hard_deadline_daa: a.hard_deadline_daa,
        };
        row.check_v1().map_err(refused)?;
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1, key, Some(borsh::to_vec(&row).expect("row serializes")))?;
    }
    Ok(())
}

/// **Lane F02b: release the court capital rows** whose claim left the live book (resolved-book release) or whose hard deadline was
/// reached. Removing the row returns the collateral; nothing is charged. Runs once per block in the kernel route's closing step.
pub(super) fn release_kernel_court_capital_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    use crate::palw_kernel_court_admission_v1::*;
    let rows = builder.state.kernel_court_capital_rows_v1();
    for row in rows {
        let bound = builder.state.court_service_state_v1().is_some_and(|s| s.claims.contains_key(&row.claim));
        if !bound || ctx.daa_score >= row.hard_deadline_daa {
            builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_COURT_CAPITAL_V1, kernel_court_capital_key_v1(&row.payer, row.claim), None)?;
        }
    }
    Ok(())
}

pub(super) fn apply_kernel_route_object_authorized_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    bytes: &[u8],
    signer: &PalwBondKeyV2,
    paid_enrollment: bool,
) -> Result<(), PalwStateV2Error> {
    if builder.extras.kernel_route.is_none() {
        return Err(refused("palw_probabilistic_constraints_v1 is not in force at this block"));
    }
    // Funded actions 25..=40 below Final27 (native_final absent) are opaque: validation did not check
    // their signature, so they must have ZERO state effect — no bond, budget, row or fee read/write.
    // Mirrors `palw_native_action_unjudged_at_v1` in the processor exactly.
    if builder.extras.kernel_route.as_ref().is_some_and(|e| match bytes.get(1).copied() {
        Some(misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11) => match bytes.get(2) {
            Some(25..=40) => e.native_final.is_none(),
            // Typed window class (§II.14.60): its own fence AND Final27.
            Some(45 | 46) => e.native_final.is_none() || e.window_class_v37.is_none_or(|at| ctx.daa_score < at),
            _ => false,
        },
        Some(misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14) if matches!(bytes.get(2), Some(19..=40)) => {
            e.native_final.as_ref().is_none_or(|f| !f.activation.is_active(ctx.daa_score))
                || e.stage_execution_v28.as_ref().is_none_or(|f| !f.activation.is_active(ctx.daa_score))
        }
        Some(misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14) if matches!(bytes.get(2), Some(41..=80)) => {
            e.native_final.as_ref().is_none_or(|f| !f.activation.is_active(ctx.daa_score))
                || e.native_execution39.as_ref().is_none_or(|f| !f.activation.is_active(ctx.daa_score))
                // Mining actions 73–79: their own dormant fence too (A-2: unjudged below it, no state effect).
                || matches!(bytes.get(2), Some(73..=80))
                    && e.native_mining39.as_ref().is_none_or(|f| !f.activation.is_active(ctx.daa_score))
        }
        _ => false,
    }) {
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
        Ok(Some(a)) if a.tag == misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11 && matches!(a.action, 41..=44) => {
            return super::palw_native_initial_fold_v31::apply(builder, ctx, signer, a.action, a.body);
        }
        Ok(Some(a)) if a.tag == misaka_palw_kernel::route::TAG_FUNDED_EXECUTION_V11 => {
            return super::palw_flat_execution_fold_v11::apply(builder, ctx, signer, a.action, a.body);
        }
        Ok(Some(a)) if a.tag == misaka_palw_kernel::route::TAG_STAGE_EXECUTION_V14 => {
            if (41..=80).contains(&a.action) {
                return super::palw_native_job_fold_v39::apply(builder, ctx, signer, a.action, a.body);
            }
            return super::palw_flat_stage_fold_v14::apply(builder, ctx, signer, a.action, a.body);
        }
        Err(_) => return Ok(()),
        _ => {}
    }
    let Ok(object) = KernelRouteObjectV1::decode(bytes) else { return Ok(()) };
    if !paid_enrollment {
        let job = match &object {
            KernelRouteObjectV1::CommitClaim { claim, .. } | KernelRouteObjectV1::CommitSegmentedClaim { claim, .. } => {
                Some(claim.job_id)
            }
            KernelRouteObjectV1::CommitPipelineClaim { claim, .. } => Some(claim.job_id),
            KernelRouteObjectV1::CommitClaimSalted { commit, .. } => Some(match commit {
                misaka_palw_kernel::route::SaltedCommitV1::Claim { claim, .. }
                | misaka_palw_kernel::route::SaltedCommitV1::Segmented { claim, .. } => claim.job_id,
                misaka_palw_kernel::route::SaltedCommitV1::Pipeline { claim, .. } => claim.job_id,
                misaka_palw_kernel::route::SaltedCommitV1::Spec { claim } => claim.job_id(),
            }),
            KernelRouteObjectV1::Spec { object: misaka_palw_kernel::spec::SpecObjectV1::CommitClaim { claim } } => {
                Some(claim.job_id())
            }
            _ => None,
        };
        if let (Some(job), Some(route)) = (job, builder.state.kernel_route.as_ref()) {
            for ((table, _), bytes) in route.aux.range(
                (crate::palw_claim_challenge_v2::PAID_CLAIM_JOBS_V2, Vec::new())
                    ..(crate::palw_claim_challenge_v2::PAID_CLAIM_JOBS_V2 + 1, Vec::new()),
            ) {
                if *table == crate::palw_claim_challenge_v2::PAID_CLAIM_JOBS_V2
                    && borsh::from_slice::<misaka_palw_kernel::claim_paid_v2::PaidClaimJobV2>(bytes)
                        .map_err(|e| refused(e.to_string()))?
                        .base_job
                        == job
                {
                    return Err(refused("an enrolled prospective V2 job requires its producer's signed V2 reveal"));
                }
            }
        }
    }
    // RFC08 R8-ECON (`palw_minimum_price_v1`): a job a minimum-price contract names admits only its contracted worker's claim, inside
    // the contract's acceptance deadline and still funded. Below the fence nothing is read.
    super::palw_econ_contracts_fold_v1::minimum_price_commit_gate_v1(builder, ctx, &object)?;
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
            if (41..=80).contains(action) {
                return super::palw_native_job_fold_v39::apply(builder, ctx, signer, *action, body);
            }
            return super::palw_flat_stage_fold_v14::apply(builder, ctx, signer, *action, body);
        }
        KernelRouteObjectV1::AuthorizePanelLifetimeV2 { authorization } => {
            return super::palw_panel_lifetime_fold_v2::authorize(builder, ctx, signer, authorization);
        }
        KernelRouteObjectV1::PaidClaimV2 { object } => {
            return super::palw_claim_challenge_fold_v2::apply_paid_claim_v2(builder, ctx, signer, object);
        }
        KernelRouteObjectV1::EnrollPanelConsumerLifetimeV2 { terms } => {
            return super::palw_panel_lifetime_fold_v2::enroll(builder, ctx, signer, terms);
        }
        // RFC07/RFC08 signed economic contracts (inner 67–69): consumer-owned, each past its own dormant fence.
        KernelRouteObjectV1::AttestModelVerificationV1 { attestation } => {
            return super::palw_econ_contracts_fold_v1::attest_model_verification_v1(builder, ctx, signer, attestation);
        }
        KernelRouteObjectV1::ConsentHorizonExtensionV1 { terms } => {
            return super::palw_econ_contracts_fold_v1::consent_horizon_extension_v1(builder, ctx, signer, terms);
        }
        KernelRouteObjectV1::SignMinimumPriceContractV1 { contract } => {
            return super::palw_econ_contracts_fold_v1::sign_minimum_price_contract_v1(builder, ctx, signer, contract);
        }
        // N3 (inner 73, round 3): a BootstrapExact seal or reveal, past `palw_beacon_bootstrap_v1`.
        KernelRouteObjectV1::BootstrapExactV1 { contribution } => {
            return super::palw_beacon_bootstrap_fold_v1::apply_bootstrap_exact_v1(builder, ctx, signer, contribution);
        }
        // RFC-0017 (inner 74/75): an eligibility exam's commit or a drawn verifier's attestation, past `palw_model_eligibility_v1`.
        KernelRouteObjectV1::CommitEligibilityExamV1 { commit } => {
            return super::palw_model_eligibility_fold_v1::apply_commit_eligibility_exam_v1(builder, ctx, signer, commit);
        }
        KernelRouteObjectV1::AttestEligibilityExamV1 { attestation } => {
            // RFC-0018 Gate C: an attestation naming a claim's certification id (aux 136) is a drawn verifier's verdict on that claim;
            // any other is RFC-0017's exam attestation, as before.
            if let Some(result) =
                super::palw_security_certification_fold_v1::try_attest_claim_certification_v1(builder, ctx, signer, attestation)
            {
                return result;
            }
            return super::palw_model_eligibility_fold_v1::apply_attest_eligibility_exam_v1(builder, ctx, signer, attestation);
        }
        // RFC-0016 V8 (inner 76, audit 16-A): a verifier's capacity attestation, past `palw_elastic_final_capacity_v1`.
        KernelRouteObjectV1::AttestCapacityV1 { attestation } => {
            return super::palw_rfc0016_fold_v1::apply_attest_capacity_v1(builder, ctx, signer, attestation);
        }
        KernelRouteObjectV1::FundPanelBeaconSourceFeeV2 { epoch, terms } => {
            return super::palw_panel_source_fee_fold_v2::fund_panel_sources_v2(builder, ctx, signer, *epoch, terms);
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
        ledger.try_attest_artifact_v1(row.kernel_param_root.as_bytes()).map_err(PalwStateV2Error::NativeResource)?;
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
            ledger.try_attest_artifact_v1(facts.param_root).map_err(PalwStateV2Error::NativeResource)?;
        }
    }
    if matches!(object, KernelRouteObjectV1::Withdraw { .. })
        && candidate_withdraw_held_v9(builder, ctx, &ledger, &palw_kernel_bond_id_v1(signer))?
    {
        return Ok(());
    }
    let before = loaded_rows(builder, &ledger)?;
    // Lane g14-court-capacity-r2: the detection policy's claim door (dormant `palw_detection_policy_v1`).
    if detection_drops_kernel_claim_v1(builder, ctx, &ledger, object) {
        return Ok(());
    }
    match opv_gate_v1(builder, ctx, &ledger, object) {
        OpvGateV1::Drop => return Ok(()),
        OpvGateV1::Admit(classes) => {
            for class in classes {
                ledger
                    .try_admit_optimistic_class_checked_v1(class)
                    .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
            }
        }
        OpvGateV1::Pass => {}
    }
    // User decision 2026-10-17 (F2): a paid-enrolled job MAY be a beacon source — an approved paid claim is the one source of an event,
    // and the paid lock reads only its sources' sealed commitments. The acyclicity rules below still refuse a target's own producer
    // and base job and mutual producer pairs. Below the paid-claim challenge's bootstrap policy (`bootstrap_seed` `None`, the probe and
    // every preset) the earlier exclusion holds byte for byte.
    if let KernelRouteObjectV1::OptInBeaconSource { job, .. } | KernelRouteObjectV1::OptInPaidBeaconSource { job, .. } = object {
        let sources_admit_paid =
            builder.params.palw_claim_challenge_v2.is_some_and(|f| f.activation.is_active(ctx.daa_score) && f.policy.bootstrap_seed.is_some());
        if !sources_admit_paid
            && builder.state.kernel_route.as_ref().is_some_and(|r| r.paid_claim_job_is_enrolled_v3(job).unwrap_or(true))
        {
            return Err(refused("paid checker-enrolled jobs cannot be Panel-independent beacon sources"));
        }
    }
    // RFC-0007 R7-SOURCE acyclicity (inside the dormant palw_beacon_source_fee_v1 path; no preset arms it): the source and
    // challenge dependency graph must stay acyclic. A paid target's own producer or its own base job never seeds that
    // target's verification, and two producers never act as each other's live sources (a two-cycle of mutual
    // certification). A distinct registered operator or key is identity only, not evidence of independence; that stays an
    // explicit assumption (claim-v2 ASSUMPTIONS).
    if let KernelRouteObjectV1::OptInPaidBeaconSource { context, producer, job, .. } = &object
        && let Some(route) = builder.state.kernel_route.as_ref()
        && let Ok(target) = route.paid_claim_authority_v2(context)
    {
        if *producer == target.enrollment.producer || *job == target.job.base_job {
            return Err(refused("a paid target's own producer or base job cannot seed its own verification"));
        }
        let mutual = ledger.beacon_sources.iter().any(|((other_context, source_producer), row)| {
            *source_producer == target.enrollment.producer
                && row.outcome.is_none()
                && route.paid_claim_authority_v2(other_context).is_ok_and(|other| other.enrollment.producer == *producer)
        });
        if mutual {
            return Err(refused("two producers cannot be each other's live beacon sources (mutual certification)"));
        }
    }
    // RFC-0007 RFC07-3 (user decision 2026-10-15, design note §4.2): the seal-before-commit order. A producer may seal as a
    // source only strictly before the commitment of every live paid claim of its own that is a source target, checked whichever
    // edge comes second, so no source cycle of any length can form (`crate::palw_beacon_source_order_v1`). Both opt-in kinds,
    // for paid-claim targets (a target with no paid claim has no producer to cycle through). Dormant: these objects exist only
    // past the opt-in/fee fences, `None` on every preset.
    if let KernelRouteObjectV1::OptInBeaconSource { producer, job, context, .. }
    | KernelRouteObjectV1::OptInPaidBeaconSource { producer, job, context, .. } = &object
        && let Some(route) = builder.state.kernel_route.as_ref()
        && let Some(target_producer) = paid_source_target_producer_v1(route, context)
        && let Some(target_commit) = route.beacon_source_target_v4(context).ok().map(|t| t.context.commitment_position)
        && let Some(seal) = ledger.seals.get(&(*job, *producer))
    {
        use crate::palw_beacon_source_order_v1::{PalwSourceEdgeV1, palw_source_order_admits_v1};
        let new = PalwSourceEdgeV1 {
            source: *producer,
            target_producer,
            sealed_daa: seal.daa,
            target_commit_daa: target_commit,
        };
        // Live = within its signed horizon (failure/refund does not free a slot early, as the kernel's own cap reads it). An edge
        // whose target cannot be resolved to a paid claim carries no producer and cannot close a cycle.
        let live = ledger.beacon_sources.iter().filter(|(_, row)| ctx.daa_score <= row.terms.hold_until).filter_map(|((other, source), row)| {
            let target_producer = paid_source_target_producer_v1(route, other)?;
            let target_commit_daa = route.beacon_source_target_v4(other).ok()?.context.commitment_position;
            Some(PalwSourceEdgeV1 { source: *source, target_producer, sealed_daa: row.sealed_daa, target_commit_daa })
        });
        palw_source_order_admits_v1(live, &new).map_err(|why| refused(why.reason()))?;
    }
    let kid = palw_kernel_bond_id_v1(signer);
    ledger.try_sync_bond_v1(kid, builder.kernel_synced_collateral(signer, ctx.daa_score)).map_err(PalwStateV2Error::NativeResource)?;
    // The other bond a slash can land on is the producer of the claim the object names: its collateral is brought up to date too, so
    // an earlier object of this block (or another lane's slash) can never leave the ledger believing the bond holds more than it does.
    if let KernelRouteObjectV1::FileProof { claim, .. }
    | KernelRouteObjectV1::FileDemand { claim, .. }
    | KernelRouteObjectV1::Respond { claim, .. } = &object
        && let Some(producer) = ledger.claims.get(claim).map(|row| row.producer)
        && let Some(key) = builder.state.kernel_route.as_ref().and_then(|k| k.bond_key_of(&producer))
    {
        ledger
            .try_sync_bond_v1(producer, builder.kernel_synced_collateral(&key, ctx.daa_score))
            .map_err(PalwStateV2Error::NativeResource)?;
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
    // G14 lane closure G-K4: a response's target, recorded below once the object stands.
    let responded = match &object {
        KernelRouteObjectV1::Respond { claim, .. } => Some(*claim),
        _ => None,
    };
    // T08 / P1-3: an old signed response that was already rejected is not a new response (no second classification or fee).
    let rejected_key = match &object {
        KernelRouteObjectV1::Respond { claim, stage, position, bytes } => {
            Some(palw_kernel_rejected_response_key_v1(&kid, claim, *stage, *position, bytes))
        }
        _ => None,
    };
    if let Some(key) = &rejected_key
        && builder
            .state
            .kernel_route
            .as_ref()
            .is_some_and(|route| route.aux.contains_key(&(PALW_KERNEL_REJECTED_RESPONSE_TABLE_V1, key.clone())))
    {
        return Ok(());
    }
    let events = match ledger.apply_object_reserved_v1(object, &AuthV1 { signer_bond: kid }).map_err(PalwStateV2Error::NativeResource)? {
        Ok(events) => events,
        Err(_refusal) => {
            // A refusal that did court work still spent the block's budget (so junk cannot be tried for free); nothing else is kept.
            persist_budget(builder, ctx, &ledger)?;
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
                persist_budget(builder, ctx, &ledger)?;
                return Ok(());
            }

            let fits = ledger.flat_conformance_classes_v9.get(class).is_some_and(|(_, r)| {
                // The candidate was just independently derived by this paid
                // attempt; fixed generated metadata supplies its carrier cap.
                misaka_palw_kernel::ledger::flat_candidate9::decode_scope_v9(&r.scope_bytes)
                    .is_ok_and(|s| s.worst_court_bytes as usize + 134 <= PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1)
            });
            if !fits {
                persist_budget(builder, ctx, &ledger)?;
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
                persist_budget(builder, ctx, &ledger)?;
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
                persist_budget(builder, ctx, &ledger)?;
                return Ok(());
            }
            // Lane F03k2 (inside the dormant `palw_kernel_route_panel_one_v1`): a Panel-licensed class registers only if saturating
            // the court for its claims' whole exposure costs more than a claim's maximum gain; a missing gain bound refuses it.
            let panel_one = builder.params.kernel_route_panel_one_from_daa().is_some_and(|from| ctx.daa_score >= from);
            if panel_one && !ledger.opv.classes.contains(class) {
                let worst = bounds.map_or(u64::MAX, |b| b.max_court_work);
                // G14 lane CLOCK (M-1): past X3 the exposure is the notice window and the gain includes the reservation.
                let clocked = ledger.claim_clock_from_daa_v1().is_some_and(|from| ctx.daa_score >= from);
                if crate::palw_kernel_route_panel_one_censorship_v1::panel_one_censorship_check_clocked_v1(
                    &ledger.policy,
                    worst,
                    builder.params.kernel_panel_one_external_gain_bound_v1(),
                    clocked,
                )
                .is_err()
                {
                    persist_budget(builder, ctx, &ledger)?;
                    return Ok(());
                }
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
        let checkpoint = builder.try_checkpoint()?;
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
            persist_budget(builder, ctx, &ledger)?;
            return Ok(());
        }
    }
    // Lane F02b: the slice's atomic court admission, decided (pure) before any accepted claim/fee/bond write. A refusal drops the
    // object exactly as a refused budget plan does; the rows are written below only once the object stands.
    let court_admissions = match kernel_court_admissions_v1(builder, ctx, &ledger, &events, signer)? {
        Some(admissions) => admissions,
        None => {
            persist_budget(builder, ctx, &ledger)?;
            return Ok(());
        }
    };
    // Reserve the shared Q/R window before any accepted claim/fee/bond write.
    // A refused plan still pays the public verification budget it already used.
    for event in &events {
        if let LedgerEventV1::ClaimCommitted { claim } = event {
            let row = ledger.claims.get(claim).ok_or_else(|| refused("accepted kernel claim is missing"))?;
            let reward = ledger.job_escrows.get(&row.job_id).map_or(0, |escrow| escrow.amount);
            let optimistic =
                ledger.mode_of_claim(claim) == Some(misaka_palw_kernel::mode::VerificationModeV1::OptimisticPublicVerification);
            // **RFC-0016 (dormant)**: past `palw_elastic_final_capacity_v1` an OPV claim is refused before acceptance while its model's
            // open OPV claims need more checks than its ready verifiers give; an admitted one notes its model for the escrow telemetry.
            if optimistic {
                let kernel_class = Hash64::from_bytes(row.class_binding_id);
                let mut natives: Vec<Hash64> = builder
                    .state
                    .kernel_route
                    .as_ref()
                    .into_iter()
                    .flat_map(|route| route.kernel_bindings_v1())
                    .filter(|(_, binding)| binding.kernel_class == kernel_class)
                    .map(|(class, _)| class)
                    .collect();
                natives.sort();
                natives.dedup();
                let native = (natives.len() == 1).then(|| natives[0]);
                let open = ledger
                    .claims
                    .iter()
                    .filter(|(id, other)| {
                        *id != claim
                            && other.class_binding_id == row.class_binding_id
                            && !other.life.state.is_terminal()
                            && ledger.mode_of_claim(id)
                                == Some(misaka_palw_kernel::mode::VerificationModeV1::OptimisticPublicVerification)
                    })
                    .count() as u64;
                if super::native_writers::local_capacity_or_v1(super::palw_rfc0016_fold_v1::rfc0016_opv_claim_committed_v1(
                    builder,
                    ctx.daa_score,
                    claim,
                    native,
                    open,
                ))?
                .is_err()
                {
                    persist_budget(builder, ctx, &ledger)?;
                    return Ok(());
                }
            }
            match builder.plan_kernel_claim_budget_v1(ctx, *signer, Hash64::from_bytes(row.class_binding_id), reward, optimistic) {
                Ok(Some(plan)) => {
                    // RFC-0008 RFC08-2 (dormant `palw_settlement_once_v1`): the combined liability predicate at admission — a
                    // refused claim is dropped with its object, exactly as a refused plan is.
                    if super::native_writers::local_capacity_or_v1(builder.admit_claim_liability_v1(
                        ctx.daa_score,
                        &plan,
                        super::palw_settlement_liability_fold_v1::PalwAdmissionLiabilityV1::KernelClaim { claim: *claim },
                    ))?
                    .is_err()
                    {
                        persist_budget(builder, ctx, &ledger)?;
                        return Ok(());
                    }
                    builder.state.require_kernel_monetary_material_scope_v1(
                        builder.params,
                        Hash64::from_bytes(*claim),
                        row,
                        ctx.daa_score,
                    )?;
                    builder.commit_bond_budget_v1(ctx, super::palw_bond_budget_fold_v1::kernel_budget_claim_id_v1(claim), plan)?;
                    builder.hold_kernel_income_v1(ctx, Hash64::from_bytes(*claim), row.liability_until.unwrap_or(ctx.daa_score))?;
                }
                // RFC-0015 §8.3: an unbudgeted OPV claim arrives here as `Err` (`unbudgeted_panel_free_v1`) and is refused below.
                Ok(None) => {}
                Err(_) => {
                    persist_budget(builder, ctx, &ledger)?;
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
                )?;
            }
        }
    }
    if let Some(key) = rejected_key
        && events.iter().any(|e| matches!(e, LedgerEventV1::ResponseRejected { .. }))
    {
        builder.write_kernel_row(PALW_KERNEL_REJECTED_RESPONSE_TABLE_V1, key, Some(borsh::to_vec(&ctx.daa_score).expect("u64")))?;
    }
    // The object stands: the route has now seen its signer.
    builder.note_kernel_bond(signer)?;
    // G14 lane closure G-K4: a segmented claim's served parts live only in block bodies — record the first block that carried one.
    if let Some(claim) = responded {
        super::palw_g14_lane_closure_fold_v1::note_served_material_v1(builder, ctx, &ledger, &claim)?;
    }
    for event in &events {
        if let LedgerEventV1::ClaimCommitted { claim } = event {
            let deadline = ledger.claims.get(claim).map(|c| c.committed_daa + ledger.policy.check_window_daa).unwrap_or(ctx.daa_score);
            // RFC-0015: an OptimisticPublicVerification claim has no Panel — no seats, no assignment, no receipts.
            let panel_licensed = ledger.mode_of_claim(claim) == Some(misaka_palw_kernel::mode::VerificationModeV1::PanelLicensed);
            if panel_licensed && matches!(ledger.claims.get(claim).map(|c| &c.body), Some(ClaimBodyV1::Program { .. })) {
                // lane SEED (G14 review MAJOR-1): past `palw_kernel_route_panel_one_v1` nothing is drawn at the commit — the seats,
                // challenge anchor and sample seed wait for a round seed anchor later than this block (`draw_panel_one_seats_v1`).
                if builder.params.kernel_route_panel_one_governs_v1(ctx.daa_score) {
                    continue;
                }
                let assignment = draw_seats(builder, claim, signer, deadline, ctx.daa_score);
                for seat in &assignment.seats {
                    builder.note_kernel_bond(&seat.bond)?;
                }
                builder.write_kernel_row(
                    PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1,
                    key_bytes(claim),
                    Some(borsh::to_vec(&assignment).expect("an assignment serializes")),
                )?;
            }
        }
    }
    // (A settlement the real bonds cannot honour fails the object — the walk's rehearsal drops it with its builder, so no half-applied
    // slash survives; the sync above makes this unreachable.)
    apply_settlements(builder, &events, true, ctx.daa_score)?;
    super::palw_source_verifier_economics_fold_v2::court_events(builder,ctx,&events)?;
    builder.close_terminal_kernel_budgets_v1(&ledger)?;
    persist_budget(builder, ctx, &ledger)?;
    // Only the exact signed candidate used the temporary scope. Persisting this root in
    // TABLE_ATTESTED would let an unrelated program borrow it in the next object.
    ledger.restore_attested_v1(original_attested);
    flush(builder, &ledger, &before)?;
    // Lane F02b: after the flush, so the book's context check reads the committed kernel claim rows.
    write_kernel_court_admissions_v1(builder, court_admissions)?;
    if let Some((subject, row)) = scope {
        super::palw_provider_court_fold_v1::write_court_scope_row_v1(builder, &subject, &row)?;
    }
    Ok(())
}

/// **Record what this block's objects have spent of its adjudication budget** (the next object of the same chain block restores it).
fn persist_budget(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, ledger: &KernelLedgerV1) -> Result<(), PalwStateV2Error> {
    let spent = ledger.budget_used();
    if spent != misaka_palw_kernel::ledger::BlockBudgetV1::default() {
        builder.write_kernel_row(
            PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1,
            Vec::new(),
            Some(borsh::to_vec(&(ctx.blue_score, spent.adjudications, spent.court_work)).expect("a budget serializes")),
        )?;
    }
    Ok(())
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
    )?;
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
    if builder.state.kernel_route.as_ref().is_some_and(|r| r.paid_claim_for_base_v2(&receipt.claim_id).ok().flatten().is_some()) {
        return Err(refused("a V1 receipt cannot cover a paid V2 verification identity"));
    }

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
    let before = loaded_rows(builder, &ledger)?;
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
    // G14 lane closure G-K1: a counted PASSING receipt locks its signer's liability on the seat's free collateral until the claim's
    // liability horizon (charged at any conviction of the claim, released after the horizon); a seat that cannot back it is not counted.
    if matches!(receipt.verdict, misaka_palw_kernel::receipt::ReceiptVerdictV1::Pass)
        && !super::palw_g14_lane_closure_fold_v1::lock_kernel_signer_v1(
            builder,
            ctx,
            &seat_key,
            &claim,
            ledger.policy.claim_reward,
            assignment.quorum,
        )?
    {
        return Ok(());
    }
    receipts.push(receipt.clone());
    let mut tally = ConstraintTallyV1::new(TallyPolicyV1 { per_segment_quorum: assignment.quorum }, evidence);
    for r in &receipts {
        tally.add(r, evidence);
    }
    if matches!(tally.state(), TallyStateV1::Covered) {
        // RFC-0006: the job pointer and its events are admitted before the tally mutates (no `DerefMut`).
        let _ = ledger.apply_panel_tally_reserved_v1(&claim).map_err(PalwStateV2Error::NativeResource)?;
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, key_bytes(&claim), None)?;
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1, key_bytes(&claim), None)?;
    } else {
        builder.write_kernel_row(
            PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1,
            key_bytes(&claim),
            Some(borsh::to_vec(&receipts).expect("receipts serialize")),
        )?;
    }
    flush(builder, &ledger, &before)?;
    Ok(())
}

/// **The block's closing step**: the kernel's `tick` (demand deadlines, windows, Final, liability release, seal expiry, escrow
/// return, the served demands' bonds) and its settlements. A no-op for a route with nothing the tick can move.
pub(super) fn tick_kernel_route_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if builder.extras.kernel_route.is_none() {
        return Ok(());
    }
    // The onboarding bindings whose refutation horizon ends release their reservation, whatever the ledger is doing.
    super::palw_onboarding_fold_v1::tick_onboarding_v1(builder, ctx)?;
    // The route's chunk groups past their TTL are dropped and their deposits forfeited, whatever the ledger is doing.
    forfeit_expired_chunk_groups_v1(builder, ctx)?;
    // DA16: the provider court's deadlines (a provider that did not answer is charged), expired leases, lapses — before the kernel's tick,
    // so a claim whose last provider was charged this block is void before its demands are swept.
    super::palw_provider_court_fold_v1::tick_provider_court_v1(builder, ctx)?;
    super::palw_claim_challenge_fold_v2::refresh_pending_paid_source_locks_v2(builder, ctx)?;
    // RFC08 R8-ECON / R8-WINDOW (round 3): stale consents and expired, unused or settled contracts retire, whatever the ledger is
    // doing (a no-op below both fences).
    super::palw_econ_contracts_fold_v1::sweep_econ_contracts_v1(builder, ctx)?;
    // N3 (round 3): due BootstrapExact rounds are judged (facts derived here, never asserted) and old judgements retire.
    super::palw_beacon_bootstrap_fold_v1::sweep_beacon_bootstrap_v1(builder, ctx)?;
    // RFC-0017: committed eligibility exams are drawn once a later seed anchor exists, decided at k positives or their deadline;
    // eligible models past their grace lapse; terminal claims' pins retire. A no-op below `palw_model_eligibility_v1`.
    super::palw_model_eligibility_fold_v1::sweep_model_eligibility_v1(builder, ctx)?;
    // RFC-0018 Gate C: certifications are drawn, decided and — when certified — credited (safe weight, settled anchor, the held
    // issuance-funded reward). A no-op below `palw_independently_work_certified_v1`.
    super::palw_security_certification_fold_v1::sweep_security_certification_v1(builder, ctx)?;
    // RFC-0007 §4.1 T6: held check fees past their trap reveal window (claim terminal) are released once; conviction rows past
    // their window are forgotten. A no-op below `palw_paid_claim_trap_v1`.
    super::palw_paid_claim_trap_fold_v1::release_held_check_fees_v1(builder, ctx.daa_score)?;
    // G14 lane r5-f02-wire (RFC-0026 F02 × ADR-0184 §1): before release, held text29 / funded v11 fees whose no-slash hold has been
    // recovered (supply now recorded, actor silent) are defaulted — refund + slash. A no-op unless the hold, F02 and the slash amount
    // are all in force; a row still past H is released below without a slash.
    super::palw_native_post_final_default_fold_v1::recover_native_post_final_defaults_v1(builder, ctx)?;
    // RFC08 08-B (ADR-0184 §1): held native fees past their liability horizon, with no open dispute, are released once (the
    // payout the Final deferred). A no-op below `palw_native_liability_horizon_v1`.
    super::palw_native_liability_hold_fold_v1::release_held_native_fees_v1(builder, ctx.daa_score, |b, row| {
        super::palw_native_liability_hold_fold_v1::native_held_dispute_open_v1(b, row)
    })?;
    // G14 lane closure G-K1: signer locks whose claim can no longer be convicted are released (a convicted one charged), whatever
    // the ledger is doing.
    super::palw_g14_lane_closure_fold_v1::sweep_kernel_signer_locks_v1(builder, ctx.daa_score)?;
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
    let before = loaded_rows(builder, &ledger)?;
    // Every bond the ledger holds is re-synced from the real chain before it is settled against.
    let held: Vec<misaka_palw_kernel::hash::Digest> = ledger.bonds.keys().copied().collect();
    for kid in held {
        if let Some(key) = builder.state.kernel_route.as_ref().and_then(|k| k.bond_key_of(&kid)) {
            ledger
                .try_sync_bond_v1(kid, builder.kernel_synced_collateral(&key, ctx.daa_score))
                .map_err(PalwStateV2Error::NativeResource)?;
        }
    }
    // lane SEED: the Panel=1 seat draws of claims committed past the fence, once a post-commit seed anchor exists. A no-op below it.
    draw_panel_one_seats_v1(builder, &ledger)?;
    let pending_source_fees: Vec<_> =
        ledger.beacon_source_fee_promises.iter().filter(|(_, promise)| promise.outcome.is_none()).map(|(key, _)| *key).collect();
    let events = ledger.tick_reserved_v1().map_err(PalwStateV2Error::NativeResource)?;
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
    super::palw_source_verifier_economics_fold_v2::court_events(builder,ctx,&events)?;
    builder.close_terminal_kernel_budgets_v1(&ledger)?;
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
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, key_bytes(&claim), None)?;
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1, key_bytes(&claim), None)?;
    }
    flush(builder, &ledger, &before)?;
    // DA16: a demand on a transferred claim that nobody answered is its providers' failure — charge them (never the producer).
    super::palw_provider_court_fold_v1::settle_provider_liable_defaults_v1(builder, ctx, &events)?;
    super::palw_claim_challenge_fold_v2::tick_paid_claim_final_projection_v2(builder, ctx)?;
    release_kernel_court_capital_v1(builder, ctx)?;
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
        )?;
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
    builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, key, None)?;
    super::apply_object(builder, ctx, &inner)
}

/// **The closing step's chunk sweep**: every group past its TTL is dropped and its opener's deposit forfeited (a real slash, burned
/// at release). Lenient (it runs after the rehearsal): a deposit the bond can no longer pay is taken as far as it goes.
fn forfeit_expired_chunk_groups_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let expired = route.expired_chunk_headers_v1(ctx.daa_score);
    for (opener, group, deposit) in expired {
        builder.write_kernel_row(PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, palw_kernel_chunk_group_key_v1(&opener, &group), None)?;
        let _ = super::native_writers::local_capacity_or_v1(builder.slash_bond(opener, deposit as u128))?;
    }
    Ok(())
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
        )?;
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

/// **RFC07-3: the producer of a paid-claim source target**, initial or staged, read from the authority row's fixed-size
/// `(version, enrollment)` prefix (the checker snapshot behind it is never decoded). `None` for a context that names no paid claim.
fn paid_source_target_producer_v1(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    context: &misaka_palw_kernel::hash::Digest,
) -> Option<misaka_palw_kernel::hash::Digest> {
    use crate::palw_claim_challenge_v2::{
        PAID_CLAIM_ENROLLMENTS_V2, PAID_CLAIM_ID_INDEX_PREFIX_V3, PAID_CLAIM_ROUNDS_V2, PalwPaidClaimIndexKindV3, PalwPaidClaimIndexV3,
        paid_claim_index_key_v3,
    };
    use borsh::BorshDeserialize;
    fn enrollment_prefix(
        route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
        primary: &misaka_palw_kernel::hash::Digest,
    ) -> Option<misaka_palw_kernel::claim_paid_v2::PaidClaimEnrollmentV2> {
        let raw = route.aux.get(&(PAID_CLAIM_ENROLLMENTS_V2, primary.to_vec()))?;
        let mut bytes = raw.as_slice();
        (u16::deserialize(&mut bytes).ok()? == 2).then_some(())?;
        misaka_palw_kernel::claim_paid_v2::PaidClaimEnrollmentV2::deserialize(&mut bytes).ok()
    }
    if let Some(enrollment) = enrollment_prefix(route, context) {
        return Some(enrollment.producer);
    }
    // A staged round's context: its index names the round row, whose first field is the enrollment id.
    let index: PalwPaidClaimIndexV3 =
        route.aux_row(PAID_CLAIM_ROUNDS_V2, &paid_claim_index_key_v3(PAID_CLAIM_ID_INDEX_PREFIX_V3, context))?;
    if index.version != 3 || index.kind != PalwPaidClaimIndexKindV3::StagedContext || index.owner != *context {
        return None;
    }
    let round = route.aux.get(&(PAID_CLAIM_ROUNDS_V2, index.target.to_vec()))?;
    let enrollment: misaka_palw_kernel::hash::Digest = round.get(..64)?.try_into().ok()?;
    let by_id: PalwPaidClaimIndexV3 =
        route.aux_row(PAID_CLAIM_ENROLLMENTS_V2, &paid_claim_index_key_v3(PAID_CLAIM_ID_INDEX_PREFIX_V3, &enrollment))?;
    if by_id.version != 3 || by_id.kind != PalwPaidClaimIndexKindV3::Enrollment || by_id.owner != enrollment {
        return None;
    }
    enrollment_prefix(route, &by_id.target).map(|e| e.producer)
}
