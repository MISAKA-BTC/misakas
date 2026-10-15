//! RFC-0006: **the kernel ledger's consensus mutations, each admitted before it runs** — replacing the arbitrary `DerefMut` the
//! kernel-route folds used to mutate a [`ReservedKernelLedgerV1`]. Every quote is derived from the real typed value the mutation
//! inserts (the object being applied, the consumer-derived inputs being installed, the one hold row a binding adds) and is added
//! to the ledger's OWN ticket, which lives exactly as long as the ledger value (the fold drops it after `flush`, whose row writes
//! are separately admitted by the transition's writers).
//!
//! What is NOT derived here (explicit, never implied): `KernelLedgerV1::apply_object`/`tick` have dozens of arms; their new
//! memberships are bounded by the applied object's own typed membership count plus the structural ceiling
//! [`PALW_KERNEL_LEDGER_APPLY_MEMBERSHIP_CEILING_V1`], which is NOT an audited per-arm count. It must be replaced by one before
//! native capacity tracking is armed anywhere (tracking is off on every preset, so no consensus value depends on it).
use super::*;
use misaka_palw_kernel::ledger::{BlockBudgetV1, LedgerEventV1};
use misaka_palw_kernel::route::{AuthV1, KernelRouteObjectV1};
use std::mem::size_of;

/// **UNKNOWN — structural ceiling, not an audited count.** New ledger memberships one `apply_object` or `tick` may create beyond
/// the applied object's own nested memberships (one per top-level ledger collection, rounded up). Local capacity only.
pub const PALW_KERNEL_LEDGER_APPLY_MEMBERSHIP_CEILING_V1: u64 = 64;
/// **UNKNOWN — structural ceiling, not an audited count.** Events one `apply_object`/`tick` step emits per live claim or demand it
/// visits (a Final's escrow triple plus its lifecycle event). Local capacity only.
pub const PALW_KERNEL_LEDGER_EVENTS_PER_ROW_CEILING_V1: u64 = 4;

fn enabled() -> bool {
    native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1()
}

/// The widest ledger membership envelope the ceiling is priced at: a 64-byte key over the larger of a claim row and the applied
/// object's own inline size (a row derived from an object never has a wider inline value than both).
fn widest_member_bytes_v1() -> u64 {
    size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], misaka_palw_kernel::ledger::ClaimRowV1>>()
        .max(size_of::<typed::BTreeNodeEnvelopeV1<[u8; 64], KernelRouteObjectV1>>()) as u64
}

impl ReservedKernelLedgerV1 {
    /// Whether this ledger's mutations are admitted: it holds a ticket (an injected owner, or native admission at load), or
    /// native admission is armed. Off on every preset: no quote is computed and the historical mutation runs unchanged.
    fn tracking_v1(&self) -> bool {
        self.lease.is_some() || enabled()
    }

    /// Extend the ledger's own ticket by `bytes` before a mutation allocates (no-op with tracking off).
    fn grow_v1(&mut self, bytes: u64) -> Result<(), NativeCoverageV1> {
        if bytes == 0 || !self.tracking_v1() {
            return Ok(());
        }
        if let Some(lease) = self.lease.as_mut() {
            lease.grow_memory_v1(bytes)
        } else {
            self.lease = reserve_native_requests_v1(bytes)?;
            Ok(())
        }
    }

    /// **Off-chain previews, rehearsals and tests only** (SDK quotes, remote pursuit planning, fixtures). Growth through this
    /// reference is NOT admitted; consensus folds use the typed methods below instead. Grep-able on purpose.
    pub fn preview_mut_v1(&mut self) -> &mut misaka_palw_kernel::ledger::KernelLedgerV1 {
        &mut self.ledger
    }

    /// The ledger clock. A fixed-size scalar: no allocation.
    pub fn set_daa_v1(&mut self, daa: u64) {
        self.ledger.daa = daa;
    }

    /// `begin_block`: resets the fixed-size per-block budget and advances the clock. No allocation.
    pub fn begin_block_v1(&mut self, daa: u64) -> Result<(), misaka_palw_kernel::route::KernelRefusalV1> {
        self.ledger.begin_block(daa)
    }

    /// `restore_budget`: a fixed-size budget value. No allocation.
    pub fn restore_budget_v1(&mut self, used: BlockBudgetV1) {
        self.ledger.restore_budget(used);
    }

    /// The consensus derived commitment-scope clock. A fixed-size scalar.
    pub fn set_commitment_scope_from_daa_v1(&mut self, from: Option<u64>) {
        self.ledger.commitment_scope_from_daa = from;
    }

    /// **The consumer-derived inputs installed before an object or the tick** (attested roots, interim verifier-pay and
    /// sealed-bounty policies, provider liability set, consumer Final-reward caps). Their heaps are the actual typed values handed
    /// in; they replace the previous values, which drop after the swap, so both coexist for one instant and both are admitted.
    pub fn install_consumer_inputs_v1(
        &mut self,
        attested: std::collections::BTreeSet<[u8; 64]>,
        verifier_pay_policy: Option<misaka_palw_kernel::verifier_pay::VerifierPayPolicyV1>,
        court_scope_policy: Option<misaka_palw_kernel::court_scope::CourtScopePolicyV1>,
        sealed_bounty_policy: Option<misaka_palw_kernel::sealed_bounty::SealedBountyPolicyV1>,
        provider_liable: Option<std::collections::BTreeSet<[u8; 64]>>,
    ) -> Result<(), NativeCoverageV1> {
        let path = "kernel ledger consumer inputs";
        let mut quote = attested.native_heap_v1(path);
        if let Some(liable) = &provider_liable {
            quote = quote.add::<Self>(liable.native_heap_v1(path), path);
        }
        let bytes = quote.bytes().ok_or(quote)?;
        self.grow_v1(bytes)?;
        self.ledger.attested_artifacts = attested;
        self.ledger.verifier_pay_policy = verifier_pay_policy;
        self.ledger.court_scope_policy = court_scope_policy;
        self.ledger.sealed_bounty_policy = sealed_bounty_policy;
        if let Some(liable) = provider_liable {
            self.ledger.provider_liable = liable;
        }
        Ok(())
    }

    /// The consumer Final-reward caps (a typed map built by the fold from its own rows), admitted before it is installed.
    pub fn install_final_reward_caps_v1(&mut self, caps: std::collections::BTreeMap<[u8; 64], u64>) -> Result<(), NativeCoverageV1> {
        let quote = caps.native_heap_v1("kernel ledger consumer Final-reward caps");
        self.grow_v1(quote.bytes().ok_or(quote)?)?;
        self.ledger.consumer_final_reward_caps = caps;
        Ok(())
    }

    /// RFC08 R8-ECON's contracted minimum payout floors (a typed map built by the fold), admitted before it is installed.
    pub fn install_minimum_payouts_v1(&mut self, floors: std::collections::BTreeMap<[u8; 64], u64>) -> Result<(), NativeCoverageV1> {
        let quote = floors.native_heap_v1("kernel ledger contracted minimum payout floors");
        self.grow_v1(quote.bytes().ok_or(quote)?)?;
        self.ledger.consumer_minimum_payouts_v1 = floors;
        Ok(())
    }

    /// **The paid-claim consumer projection**, already admitted by [`Self::reserve_projection_v1`] (whose actual aux-row inventory
    /// counts one consumer membership and two row copies per enrollment). Only the route's own projection installer uses this.
    pub(crate) fn paid_consumer_projection_mut_v1(&mut self) -> &mut misaka_palw_kernel::ledger::KernelLedgerV1 {
        &mut self.ledger
    }

    /// Re-install the attested set the fold saved before an object (the saved value's heap is already admitted by the fold's
    /// original install; the swap drops the current set).
    pub fn restore_attested_v1(&mut self, attested: std::collections::BTreeSet<[u8; 64]>) {
        self.ledger.attested_artifacts = attested;
    }

    /// Beacon-source inputs: the actual typed value, admitted before install.
    pub fn set_beacon_source_inputs_reserved_v1(
        &mut self,
        inputs: misaka_palw_kernel::beacon_source::BeaconSourceInputsV1,
    ) -> Result<Result<(), String>, NativeCoverageV1> {
        let quote = inputs.native_heap_v1("kernel ledger beacon-source inputs");
        self.grow_v1(quote.bytes().ok_or(quote)?)?;
        Ok(self.ledger.set_beacon_source_inputs(inputs))
    }

    /// Beacon-source fee inputs: the actual typed value, admitted before install.
    pub fn set_beacon_source_fee_inputs_reserved_v1(
        &mut self,
        inputs: misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeInputsV1,
    ) -> Result<Result<(), String>, NativeCoverageV1> {
        let quote = inputs.native_heap_v1("kernel ledger beacon-source fee inputs");
        self.grow_v1(quote.bytes().ok_or(quote)?)?;
        Ok(self.ledger.set_beacon_source_fee_inputs(inputs))
    }

    /// **One signed object**: the object's own typed heap (twice: the stored rows and their decoded projections), its nested
    /// typed memberships plus the structural membership ceiling, and the returned events Vec.
    pub fn apply_object_reserved_v1(
        &mut self,
        object: &KernelRouteObjectV1,
        auth: &AuthV1,
    ) -> Result<Result<Vec<LedgerEventV1>, misaka_palw_kernel::route::KernelRefusalV1>, NativeCoverageV1> {
        if self.tracking_v1() {
            let path = "kernel ledger apply_object";
            let heap = object.native_heap_v1(path).scale::<KernelRouteObjectV1>(2, path);
            let heap = heap.bytes().ok_or(heap)?;
            let members = object
                .native_members_v1()
                .and_then(|m| m.checked_add(PALW_KERNEL_LEDGER_APPLY_MEMBERSHIP_CEILING_V1))
                .ok_or_else(|| NativeCoverageV1::overflow::<KernelRouteObjectV1>("apply_object memberships"))?;
            let mut q = TirNativeRequestsV1::new();
            q.add_v1(heap)?;
            q.add_v1(TirNativeRequestsV1::product_v1(members, widest_member_bytes_v1())?)?;
            q.vec_v1::<LedgerEventV1>(members.saturating_mul(PALW_KERNEL_LEDGER_EVENTS_PER_ROW_CEILING_V1))?;
            self.grow_v1(q.bytes_v1())?;
        }
        Ok(self.ledger.apply_object(object, auth))
    }

    /// **The closing tick**: events for every live claim and demand it may visit, and the structural membership ceiling.
    pub fn tick_reserved_v1(&mut self) -> Result<Vec<LedgerEventV1>, NativeCoverageV1> {
        if self.tracking_v1() {
            let rows = (self.ledger.claims.len() as u64).saturating_add(self.ledger.demands.len() as u64);
            let mut q = TirNativeRequestsV1::new();
            q.vec_v1::<LedgerEventV1>(TirNativeRequestsV1::product_v1(rows, PALW_KERNEL_LEDGER_EVENTS_PER_ROW_CEILING_V1)?)?;
            q.add_v1(TirNativeRequestsV1::product_v1(PALW_KERNEL_LEDGER_APPLY_MEMBERSHIP_CEILING_V1, widest_member_bytes_v1())?)?;
            self.grow_v1(q.bytes_v1())?;
        }
        Ok(self.ledger.tick())
    }

    /// A covered Panel tally: the one job pointer it may insert (`prepare_panel_tally_v1`) and its few events.
    pub fn apply_panel_tally_reserved_v1(
        &mut self,
        claim: &[u8; 64],
    ) -> Result<Result<Vec<LedgerEventV1>, misaka_palw_kernel::route::KernelRefusalV1>, NativeCoverageV1> {
        self.prepare_panel_tally_v1(claim)?;
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<LedgerEventV1>(PALW_KERNEL_LEDGER_EVENTS_PER_ROW_CEILING_V1)?;
        self.grow_v1(q.bytes_v1())?;
        Ok(self.ledger.apply_panel_tally(claim, true))
    }

    /// A provider lapse: removals and its few events.
    pub fn provider_lapse_reserved_v1(
        &mut self,
        claim: &[u8; 64],
    ) -> Result<Result<Vec<LedgerEventV1>, misaka_palw_kernel::route::KernelRefusalV1>, NativeCoverageV1> {
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<LedgerEventV1>(PALW_KERNEL_LEDGER_EVENTS_PER_ROW_CEILING_V1)?;
        self.grow_v1(q.bytes_v1())?;
        Ok(self.ledger.provider_lapse(claim))
    }

    /// An EXEC reward binding inserts exactly one hold row for an absent claim (an existing hold is refused by the kernel).
    pub fn bind_exec_reward_until_reserved_v1(
        &mut self,
        root: [u8; 64],
        claim: [u8; 64],
        expires_daa: u64,
        liability_floor: u64,
    ) -> Result<Result<(), String>, NativeCoverageV1> {
        self.reserve_membership_v1::<[u8; 64], misaka_palw_kernel::exec_reward::ExecRewardHoldV1>(
            !self.ledger.exec_reward_holds.contains_key(&claim),
        )?;
        Ok(self.ledger.bind_exec_reward_until(root, claim, expires_daa, liability_floor))
    }

    /// An EXEC reward finish removes its hold and appends the escrow triple to `out`.
    pub fn finish_exec_reward_reserved_v1(
        &mut self,
        root: [u8; 64],
        claim: [u8; 64],
        cap: u64,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<Result<u64, String>, NativeCoverageV1> {
        self.reserve_events_v1(out.len())?;
        Ok(self.ledger.finish_exec_reward(root, claim, cap, out))
    }

    /// An EXEC reward cancellation removes its hold and appends its release to `out`.
    pub fn cancel_exec_reward_reserved_v1(
        &mut self,
        root: [u8; 64],
        claim: [u8; 64],
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<Result<(), String>, NativeCoverageV1> {
        self.reserve_events_v1(out.len())?;
        Ok(self.ledger.cancel_exec_reward(root, claim, out))
    }

    /// **A paid claim's signed lifetime on its existing kernel row**: the fixed-size lifecycle state, Final hold and liability
    /// horizon replace the row's own fields in place — no membership, no heap. `false` when the row is absent (nothing changes).
    pub fn set_claim_lifetime_v1(
        &mut self,
        claim: &[u8; 64],
        state: misaka_palw_kernel::lifecycle::ClaimStateV1,
        final_hold_until: u64,
        liability_until: Option<u64>,
    ) -> bool {
        let Some(row) = self.ledger.claims.get_mut(claim) else { return false };
        row.life.state = state;
        row.life.final_hold_until = final_hold_until;
        row.liability_until = liability_until;
        true
    }

    /// A consented horizon extension rewrites one existing claim row's fixed-size `liability_until` in place (no allocation).
    pub fn extend_claim_liability_by_consent_v1(
        &mut self,
        claim: &[u8; 64],
        expected_until: u64,
        new_until: u64,
        hard_deadline: u64,
    ) -> Result<(), String> {
        self.ledger.extend_claim_liability_by_consent_v1(claim, expected_until, new_until, hard_deadline)
    }

    /// The caller's events Vec grows by at most the per-row ceiling: its geometric replacement while the old buffer is live.
    fn reserve_events_v1(&mut self, held: usize) -> Result<(), NativeCoverageV1> {
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<LedgerEventV1>((held as u64).saturating_add(PALW_KERNEL_LEDGER_EVENTS_PER_ROW_CEILING_V1))?;
        self.grow_v1(q.bytes_v1())
    }
}

#[cfg(test)]
mod tests {
    //! SOURCE ONLY (never compiled or executed in this lane): every typed mutation is refused BEFORE it changes the ledger when its
    //! quote does not fit, keeps its growth on the ledger's own ticket otherwise, and the fixed-size setters allocate nothing.
    use super::*;

    fn owner(limit: u64) -> (std::sync::Arc<NativeDemandCheckerV1>, ReservedKernelLedgerV1) {
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: limit, database_bytes: 0 });
        let policy = crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(crate::Hash64::default(), crate::Hash64::default());
        let ledger = crate::palw_kernel_route_v1::palw_kernel_route_template_v1(policy);
        let lease = Some(checker.reserve_requests_v1(0, 0).unwrap());
        (checker, ReservedKernelLedgerV1::from_preflight_v1(ledger, lease))
    }

    #[test]
    fn apply_object_and_tick_refuse_before_the_ledger_changes() {
        let (checker, mut ledger) = owner(0);
        let rows = ledger.to_rows();
        let object = KernelRouteObjectV1::Withdraw { bond: [3; 64] };
        assert!(ledger.apply_object_reserved_v1(&object, &AuthV1 { signer_bond: [3; 64] }).is_err());
        assert!(ledger.tick_reserved_v1().is_err());
        assert_eq!(ledger.to_rows(), rows, "a refused admission leaves the ledger's rows unchanged");
        assert_eq!(checker.outstanding(), (0, 0));
    }

    #[test]
    fn admitted_growth_lives_on_the_ledgers_own_ticket() {
        let (checker, mut ledger) = owner(u64::MAX);
        let object = KernelRouteObjectV1::Withdraw { bond: [3; 64] };
        // The kernel may refuse the object semantically; the admission itself succeeded and stays on the ticket.
        let _ = ledger.apply_object_reserved_v1(&object, &AuthV1 { signer_bond: [3; 64] }).unwrap();
        let held = checker.outstanding().0;
        assert!(held > 0);
        ledger.set_daa_v1(9);
        ledger.restore_budget_v1(BlockBudgetV1 { adjudications: 0, court_work: 0 });
        assert_eq!(checker.outstanding().0, held, "fixed-size setters admit nothing");
        drop(ledger);
        assert_eq!(checker.outstanding(), (0, 0));
    }

    #[test]
    fn consumer_inputs_are_admitted_from_the_actual_values_before_install() {
        let (checker, mut ledger) = owner(0);
        let attested: std::collections::BTreeSet<[u8; 64]> = [[1; 64], [2; 64]].into_iter().collect();
        assert!(ledger.install_consumer_inputs_v1(attested, None, None, None, None).is_err());
        assert!(ledger.attested_artifacts.is_empty(), "nothing installed on refusal");
        assert!(ledger.install_final_reward_caps_v1([([1; 64], 5u64)].into_iter().collect()).is_err());
        assert!(ledger.consumer_final_reward_caps.is_empty());
        // An empty input needs no growth at all.
        ledger.install_minimum_payouts_v1(Default::default()).unwrap();
        assert_eq!(checker.outstanding(), (0, 0));
    }
}
