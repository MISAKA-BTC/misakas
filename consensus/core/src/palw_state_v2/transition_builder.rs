//! **The transition builder's writers and readers (`impl TransitionBuilder`)** — moved out of `palw_state_v2.rs` (2026-10-12, a behaviour-preserving split: the items are the same; members the parent calls are `pub(super)`).

use super::*;

impl<'a> TransitionBuilder<'a> {
    /// The fold's read-only inputs as one view ([`PalwFoldReadV1`]): the rules an outside caller
    /// must evaluate exactly as this fold does live there, and the methods here delegate.
    pub(super) fn read(&self) -> PalwFoldReadV1<'_> {
        PalwFoldReadV1 {
            state: &self.state,
            params: self.params,
            extras: self.extras,
            inflight_index: Some(&self.inflight_index),
            panel_horizon: Some(&self.panel_horizon),
            own_attempt_class: self.own_attempt_class,
            room_exempt_class: self.room_exempt_class,
        }
    }

    #[cfg(test)]
    pub(super) fn new(
        parent: &PalwChainStateV2,
        params: &'a PalwStateParamsV2,
        unavailable_abstains: bool,
        capability_bound: bool,
        uncertified_weightless: bool,
        da_court: bool,
        extras: &'a PalwTransitionExtrasV1,
    ) -> Self {
        Self::try_new(parent, params, unavailable_abstains, capability_bound, uncertified_weightless, da_court, extras)
            .expect("test transition construction capacity")
    }

    pub(super) fn try_new(
        parent: &PalwChainStateV2,
        params: &'a PalwStateParamsV2,
        unavailable_abstains: bool,
        capability_bound: bool,
        uncertified_weightless: bool,
        da_court: bool,
        extras: &'a PalwTransitionExtrasV1,
    ) -> Result<Self, PalwStateV2Error> {
        let (state, native_reservation) = crate::palw_native_resource_v1::NativeTransitionReservationV1::copy_parent_v1(parent)
            .map_err(PalwStateV2Error::NativeResource)?;
        Ok(Self {
            native_reservation,
            params,
            monetary_native_ready: BTreeMap::new(),
            state,
            entries: Vec::new(),
            registration_index: None,
            unavailable_abstains,
            model_moves: 0,
            exec_origin_budget: Default::default(),
            capability_bound,
            uncertified_weightless,
            da_court,
            extras,
            seen_exec: std::collections::HashSet::new(),
            pending_riders: Vec::new(),
            inflight_index: std::cell::RefCell::new(None),
            panel_horizon: std::cell::Cell::new(None),
            own_attempt_class: None,
            own_attempt_fit: false,
            room_exempt_class: None,
            class_registrations: 0,
            tir_registrations: 0,
            heavy_prompt_ids_charged: 0,
            heavy_prompt_claims: BTreeSet::new(),
            panel_v3_beacons: Vec::new(),
            paid_lifetime_admission_v1: None,
        })
    }

    pub(super) fn registration_index_v1(&mut self) -> Result<&PalwBondRegistrationIndexV1, PalwStateV2Error> {
        if self.registration_index.is_none() {
            use crate::palw_native_resource_v1::{NativeCoverageV1, typed::BTreeNodeEnvelopeV1};
            // Two actual lookups (key/operator), each an outer bucket and one
            // holder node per bond. Hash collisions reduce outer memberships.
            let unit = 2
                * (std::mem::size_of::<BTreeNodeEnvelopeV1<Hash64, BTreeSet<PalwBondKeyV2>>>()
                    + std::mem::size_of::<BTreeNodeEnvelopeV1<PalwBondKeyV2, ()>>());
            let requests = (self.state.bonds.len() as u64).checked_mul(unit as u64).ok_or_else(|| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwBondRegistrationIndexV1>(
                    "registration lookup construction",
                ))
            })?;
            self.native_reservation.grow_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
            let mut index = PalwBondRegistrationIndexV1::default();
            for (key, record) in &self.state.bonds {
                index.note(*key, record, true);
            }
            self.registration_index = Some(index);
        }
        Ok(self.registration_index.as_ref().unwrap())
    }

    pub(super) fn registered_pubkey_v1(&mut self, pubkey: &[u8]) -> Result<bool, PalwStateV2Error> {
        let bucket = registration_key_bucket_v1(pubkey);
        self.registration_index_v1()?;
        Ok(self
            .registration_index
            .as_ref()
            .unwrap()
            .keys
            .get(&bucket)
            .is_some_and(|keys| keys.iter().any(|key| self.state.bonds.get(key).is_some_and(|record| record.pubkey == pubkey))))
    }

    pub(super) fn registered_operator_v1(&mut self, operator: Hash64) -> Result<Option<PalwBondKeyV2>, PalwStateV2Error> {
        Ok(self.registration_index_v1()?.operators.get(&operator).and_then(|holders| holders.first().copied()))
    }

    // Every write goes through one of these, so the delta cannot miss a change.

    pub(super) fn write_bond(&mut self, key: PalwBondKeyV2, new: Option<PalwBondStateV2>) -> Result<(), PalwStateV2Error> {
        use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, typed::BTreeNodeEnvelopeV1};
        let path = "native bond writer clone, memberships and journal";
        // The incoming value and its separately owned map clone coexist with
        // the journal. The old record moves from the already reserved source
        // state into the journal, so it needs no extra deep copy here.
        let mut bound = new
            .native_heap_v1(path)
            .scale::<PalwBondStateV2>(2, path)
            .add::<PalwDeltaEntryV2>(NativeCoverageV1::Bounded { bytes: 8 * std::mem::size_of::<PalwDeltaEntryV2>() as u64 }, path);
        if new.is_some() {
            let primary = std::mem::size_of::<BTreeNodeEnvelopeV1<PalwBondKeyV2, PalwBondStateV2>>();
            let lookup = if self.registration_index.is_some() {
                2 * (std::mem::size_of::<BTreeNodeEnvelopeV1<Hash64, BTreeSet<PalwBondKeyV2>>>()
                    + std::mem::size_of::<BTreeNodeEnvelopeV1<PalwBondKeyV2, ()>>())
            } else {
                0
            };
            bound = bound.add::<PalwBondStateV2>(NativeCoverageV1::Bounded { bytes: (primary + lookup) as u64 }, path);
        }
        self.native_reservation
            .grow_v1(bound.bytes().ok_or_else(|| PalwStateV2Error::NativeResource(bound))?)
            .map_err(PalwStateV2Error::NativeResource)?;

        // ADR-0160 F-W: the bond's cap is a function of its collateral, so every collateral move — a
        // slash lowers it, a refund restores it — re-caps the bond's provisional term HERE, the one
        // site all of them pass (the "recap" of ADR-0160 §7.1).
        let before = self.weight_cap_term(&key);
        let old = match &new {
            Some(record) => self.state.bonds.insert(key, record.clone()),
            None => self.state.bonds.remove(&key),
        };
        if let Some(index) = &mut self.registration_index {
            // Collateral/status-only writes do not change either lookup.
            let same_identity = old
                .as_ref()
                .zip(new.as_ref())
                .is_some_and(|(old, new)| old.pubkey == new.pubkey && old.operator_id == new.operator_id);
            if !same_identity {
                if let Some(record) = &old {
                    index.note(key, record, false);
                }
                if let Some(record) = &new {
                    index.note(key, record, true);
                }
            }
        }
        self.weight_cap_retotal(before, self.weight_cap_term(&key));
        self.entries.push(PalwDeltaEntryV2::Bond { key, old, new });
        Ok(())
    }

    // ---- ADR-0160 F-W (lane cap-weight): the per-bond weight cap's two maintenance moves ----

    /// `min(X_b, W_cap(C_b))`, the bond's share of `bounded_immature` at this instant.
    pub(super) fn weight_cap_term(&self, bond: &PalwBondKeyV2) -> u128 {
        self.state.capacity_weight_index.term(bond, self.state.bonds.get(bond).map(|record| record.collateral))
    }

    /// Move `bounded_immature` from a bond's old capped term to its new one. Exact: the running value
    /// is `Σ old-rule raw + Σ_b term(b)` after every write, so `before` is inside it (W-I6); the
    /// saturation is a guard the load-time re-derivation would refuse, never a path an honest fold takes.
    pub(super) fn weight_cap_retotal(&mut self, before: u128, after: u128) {
        if before == after {
            return;
        }
        debug_assert!(self.state.bounded_immature >= before, "W-I6: the bond's term is inside bounded_immature");
        self.state.bounded_immature = self.state.bounded_immature.saturating_sub(before).saturating_add(after);
    }

    /// **The claim half: every phase door passes `write_claim`** — acceptance, bind, licence, an S2
    /// upgrade, a redraw, a DA accusation and its end, `Final`, a void, retirement — so the bond's
    /// staged sum, its held reservation and its capped term follow the record here and nowhere else.
    /// A no-op wherever the fence's mirror is `None` (every shipped preset) and for every old-rule claim.
    pub(super) fn weight_cap_on_write(&mut self, old: Option<&PalwClaimStateV2>, new: Option<&PalwClaimStateV2>) {
        if self.params.capacity_weight_cap_from_daa().is_none() {
            return;
        }
        let applies = |claim: &&PalwClaimStateV2| crate::palw_weight_cap_v1::palw_weight_cap_applies_v1(self.params, claim);
        let (old, new) = (old.filter(applies), new.filter(applies));
        let Some(bond) = old.or(new).map(|claim| claim.bond) else { return };
        debug_assert!(old.is_none_or(|o| new.is_none_or(|n| o.bond == n.bond)), "a claim never changes bond");
        let before = self.weight_cap_term(&bond);
        if let Some(claim) = old {
            self.state.capacity_weight_index.note(self.params, claim, false);
        }
        if let Some(claim) = new {
            self.state.capacity_weight_index.note(self.params, claim, true);
        }
        self.weight_cap_retotal(before, self.weight_cap_term(&bond));
    }

    pub(super) fn write_exposure(&mut self, key: PalwBondKeyV2, new: Option<u128>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.reserved_exposure, false, "native reserved_exposure writer")?;
        let old = match new {
            Some(value) => self.state.reserved_exposure.insert(key, value),
            None => self.state.reserved_exposure.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Exposure { key, old, new });
        Ok(())
    }

    /// **One exposure ceiling for both arms that ACCUSE** (ADR-0062 SA-7(c)).
    ///
    /// A `DefaultAccused` and a `CourtOpened` are one act priced twice: a bonded party freezes
    /// somebody else's claim by RESERVING against its own collateral. `write_exposure` moves the
    /// ledger and leaves `collateral` alone, so "Active and at or above the registry floor" is a
    /// pair of checks the same bond passes once per live claim on the network — the bound has to
    /// be on the LEDGER, and it has to be the same bound on both arms or they are two mechanisms
    /// that drift. They did drift: SA-7 bounded the accusation and left the court exactly as it
    /// was, under a comment saying the ceiling it lives under "does the counting" — that ceiling is
    /// `check_palw_attempt_admission_v2`'s, and it counts only a bond that wants to PRODUCE. One
    /// bond at the floor could open a court on every licensed claim at once, and because the
    /// challenger-side close charges `min(reserved, floor)` while `slash_bond` clamps at collateral
    /// and returns early at zero, only the FIRST of those closes ever collected anything.
    ///
    /// So both arms reserve here. The figure counted is what the fold can actually TAKE from this
    /// bond for this object (`palw_da_accusation_exposure_v2` on one arm, the same `min(reserved,
    /// floor)` the close collects on the other), which is what makes the ceiling a bound on money
    /// rather than on count: with `max_exposure_ratio_permille ≤ 1000` — refused above 1000 by
    /// `PalwAdmissionParamsV2::new` and by `validate_ruleset_shape`, so it is a checked property of
    /// a bootable ruleset and not a sentence — the sum of one bond's concurrent reservations is at
    /// most its collateral, every refutation's debit lands, and the K-th is funded exactly like the
    /// first.
    ///
    /// **`enforce` is the fence, not a policy.** The court arm predates `palw_da_court` and its
    /// reservation must stay byte-identical on a network the fence has not reached; the accusation
    /// arm is refused outright while the fence is dormant, so it always passes `true`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reserve_accuser_exposure_v2(
        &mut self,
        bond: PalwBondKeyV2,
        collateral: u64,
        amount: u128,
        edge: &'static str,
        enforce: bool,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        // **ADR-0152 past `palw_rcore_plus`: the accuser gate** (the S review's M1). The stake is still
        // written to `reserved_exposure` below (the DA arm's interim ledger until M3), but it is
        // admitted on the free half against the one invariant, not on the claim ledger's 500‰
        // ceiling, which saw neither the courts nor the locks' excess.
        if self.params.rcore_plus_active_at(now_daa) {
            self.check_accuser_room(bond, amount, edge, now_daa)?;
        } else if enforce {
            // What this bond already backs — its live claims, its own registrations, and every
            // accusation and court it already has open — plus what this one would reserve.
            let backed = self
                .state
                .reserved_exposure(&bond)
                .checked_add(self.state.registration_exposure(&bond))
                .ok_or(PalwStateV2Error::Overflow("accuser total exposure"))?;
            let ceiling = (collateral as u128)
                .checked_mul(self.params.fp_max_exposure_ratio_permille as u128)
                .ok_or(PalwStateV2Error::Overflow("accuser exposure ceiling"))?
                / 1000;
            let next = backed.checked_add(amount).ok_or(PalwStateV2Error::Overflow("accuser exposure"))?;
            if next > ceiling {
                return Err(PalwStateV2Error::AccusationExposureCeiling { bond, edge, backed, accusation: amount, ceiling });
            }
        }
        let already = self.state.reserved_exposure.get(&bond).copied().unwrap_or(0);
        let next = already.checked_add(amount).ok_or(PalwStateV2Error::Overflow("accuser exposure"))?;
        self.write_exposure(bond, Some(next))?;
        Ok(())
    }

    /// **A court's challenger stake** (the court opening and the held dissection). Below
    /// `palw_rcore_plus` exactly [`Self::reserve_accuser_exposure_v2`]: the claim's `reserved` joins
    /// the challenger's `reserved_exposure` under the 500‰ ceiling (`write_court` gives it back).
    /// **ADR-0152 A-6 past it: accusers use the free half.** Nothing is written to
    /// `reserved_exposure` — the stake is the session itself, read by [`palw_accuser_exposure_v1`]
    /// through the challenger index and by B-3's exit gate — and the check is `committed + accuser
    /// + new ≤ collateral` (1000‰), so a seat whose 500‰ is full of locks can still accuse.
    pub(super) fn reserve_challenger_v1(
        &mut self,
        bond: PalwBondKeyV2,
        collateral: u64,
        amount: u128,
        edge: &'static str,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.params.rcore_plus_active_at(now_daa) {
            return self.reserve_accuser_exposure_v2(bond, collateral, amount, edge, self.da_court, now_daa);
        }
        self.check_accuser_room(bond, amount, edge, now_daa)
    }

    /// **The accuser gate past `palw_rcore_plus`** (A-6 as amended by the S review, M1): `amount`
    /// fits the free half — `max(committed, 500‰ × C) + accuser + amount ≤ C` — or it is refused
    /// `AccusationExposureCeiling` (backed = `C − room`, ceiling = `C`). Every accusation that stakes
    /// against a bond passes here: the court opening, the held dissection and the data-availability
    /// accusation (M3's redesign routes through it too).
    pub(super) fn check_accuser_room(
        &self,
        bond: PalwBondKeyV2,
        amount: u128,
        edge: &'static str,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        let room = self.gate_room(&bond, now_daa, PalwRcoreGateV1::Accuser);
        if amount > room {
            let collateral = self.state.bonds.get(&bond).map(|record| record.collateral as u128).unwrap_or(0);
            return Err(PalwStateV2Error::AccusationExposureCeiling {
                bond,
                edge,
                backed: collateral.saturating_sub(room),
                accusation: amount,
                ceiling: collateral,
            });
        }
        Ok(())
    }

    pub(super) fn write_registration_exposure(&mut self, key: PalwBondKeyV2, new: Option<u128>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.registration_exposure, false, "native registration_exposure writer")?;
        let old = match new {
            Some(value) => self.state.registration_exposure.insert(key, value),
            None => self.state.registration_exposure.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::RegistrationExposure { key, old, new });
        Ok(())
    }

    /// Reserve this writer's actual row copy, absent membership and journal
    /// replacement before either collection changes. This accounts requested
    /// typed allocation; a separate physical allocator quota remains necessary.
    fn reserve_map_journal_v1<K: crate::palw_native_resource_v1::NativeHeapV1, V: crate::palw_native_resource_v1::NativeHeapV1>(
        &mut self,
        key: &K,
        new: &Option<V>,
        absent: bool,
        path: &'static str,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, typed::BTreeNodeEnvelopeV1};
        // Preserve the historical flags-off path, including its allocation
        // strategy. An injected test owner also makes tracking active.
        if !self.native_reservation.is_tracking_v1() {
            return Ok(());
        }
        let mut bound = new.native_heap_v1(path).scale::<V>(2, path).add::<K>(key.native_heap_v1(path).scale::<K>(2, path), path);
        let mut journal_capacity = None;
        if absent && new.is_some() {
            if cfg!(not(target_pointer_width = "64")) {
                return Err(PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<BTreeMap<K, V>>(
                    path,
                    "pinned BTree allocation layout only covers 64-bit targets",
                )));
            }
            bound = bound.add::<V>(NativeCoverageV1::Bounded { bytes: std::mem::size_of::<BTreeNodeEnvelopeV1<K, V>>() as u64 }, path);
        }
        if self.entries.len() == self.entries.capacity() {
            // The existing allocation and its replacement may coexist during
            // realloc. The old frame remains held by the construction owner.
            let count = self
                .entries
                .capacity()
                .max(2)
                .checked_mul(2)
                .ok_or_else(|| PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>(path)))?;
            let bytes = (count as u64)
                .checked_mul(std::mem::size_of::<PalwDeltaEntryV2>() as u64)
                .ok_or_else(|| PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwDeltaEntryV2>(path)))?;
            bound = bound.add::<PalwDeltaEntryV2>(NativeCoverageV1::Bounded { bytes }, path);
            journal_capacity = Some(count);
        }
        self.native_reservation
            .grow_v1(bound.bytes().ok_or_else(|| PalwStateV2Error::NativeResource(bound))?)
            .map_err(PalwStateV2Error::NativeResource)?;
        if let Some(count) = journal_capacity {
            self.entries.try_reserve_exact(count - self.entries.len()).map_err(|_| {
                PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwDeltaEntryV2>(
                    path,
                    "allocator refused the pre-admitted journal request",
                ))
            })?;
        }
        Ok(())
    }

    /// Chunk assembly owns a copy of the existing group, the incoming part,
    /// its new membership, an eviction-key projection and the explicit whole
    /// byte buffer. Reserve those concrete allocations before any Clone/Vec.
    pub(super) fn reserve_chunk_assembly_v1(&mut self, group: &Hash64, bytes: &[u8]) -> Result<(), PalwStateV2Error> {
        use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, TirNativeRequestsV1, typed::BTreeNodeEnvelopeV1};
        if !self.native_reservation.is_tracking_v1() {
            return Ok(());
        }
        if cfg!(not(target_pointer_width = "64")) {
            return Err(PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwPendingChunksV2>(
                "chunk assembly membership",
                "pinned tree profile requires 64-bit",
            )));
        }
        let mut q = TirNativeRequestsV1::new();
        let mut total = bytes.len() as u64;
        if let Some(pending) = self.state.pending_chunks.get(group) {
            let heap = pending.native_heap_v1("chunk assembly existing group clone");
            q.add_v1(heap.bytes().ok_or_else(|| PalwStateV2Error::NativeResource(heap))?).map_err(PalwStateV2Error::NativeResource)?;
            for part in pending.parts.values() {
                total = total.checked_add(part.len() as u64).ok_or_else(|| {
                    PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<PalwPendingChunksV2>("chunk assembly bytes"))
                })?;
            }
        }
        q.vec_v1::<u8>(bytes.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        q.exact_v1::<BTreeNodeEnvelopeV1<u8, Vec<u8>>>(1).map_err(PalwStateV2Error::NativeResource)?;
        q.vec_v1::<Hash64>(self.state.pending_chunks.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
        q.exact_v1::<u8>(total).map_err(PalwStateV2Error::NativeResource)?;
        self.native_reservation.grow_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)
    }

    /// With native admission enabled, nested structural preflight precedes
    /// decoding and its requests remain under the transition owner. Disabled
    /// admission preserves the historical Borsh decode/semantic path.
    pub(super) fn reserve_chunk_decode_v1(&mut self, whole: &[u8]) -> Result<(), PalwStateV2Error> {
        if !self.native_reservation.is_tracking_v1() {
            return Ok(());
        }
        let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<PalwConsensusObjectV2>(whole, whole.len() as u64)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::Unsupported | std::io::ErrorKind::TimedOut | std::io::ErrorKind::OutOfMemory => {
                    PalwStateV2Error::NativeResource(crate::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                        PalwConsensusObjectV2,
                    >("chunk inner native preflight", &e.to_string()))
                }
                _ => PalwStateV2Error::ChunkedObjectUndecodable(e.to_string()),
            })?;
        self.native_reservation.grow_v1(shape.logical_requests_v1()).map_err(PalwStateV2Error::NativeResource)?;
        Ok(())
    }

    /// ADR-0071 SA-3: record that a bond produced on a class. The `capabilities` collection has
    /// been rooted, carried and delta-tracked since PR-03 with no writer; this is the writer its
    /// record shape (`class_id`, `bond`, `issued_daa`) was evidently reserved for, so SA-3 adds no
    /// collection to `state_root` and moves no golden root while the fence is off.
    pub(super) fn write_capability(&mut self, key: Hash64, new: Option<PalwCapabilityStateV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.capabilities.contains_key(&key), "native capability writer")?;
        let old = match &new {
            Some(record) => self.state.capabilities.insert(key, record.clone()),
            None => self.state.capabilities.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Capability { key, old, new });
        Ok(())
    }

    pub(super) fn write_class_walk(&mut self, key: Hash64, new: Option<PalwClassWalkV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.class_walks.contains_key(&key), "native class_walk writer")?;
        let old = match new {
            Some(value) => self.state.class_walks.insert(key, value),
            None => self.state.class_walks.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::ClassWalk { key, old, new });
        Ok(())
    }

    /// **ADR-0056 Decision 3: take the registry's price, or give it back.** One function for both
    /// directions so the two can never disagree about the amount — the reservation is the
    /// parameter's value at the moment it is taken, and release subtracts exactly that.
    pub(super) fn move_registration_exposure(&mut self, bond: PalwBondKeyV2, take: bool) -> Result<(), PalwStateV2Error> {
        let amount = self.params.registration_exposure_sompi() as u128;
        if amount == 0 {
            return Ok(()); // Decision 3 is off on this network.
        }
        let current = self.state.registration_exposure.get(&bond).copied().unwrap_or(0);
        let next = if take {
            current.checked_add(amount).ok_or(PalwStateV2Error::Overflow("registration_exposure"))?
        } else {
            current.checked_sub(amount).ok_or(PalwStateV2Error::Overflow("registration_exposure underflow"))?
        };
        self.write_registration_exposure(bond, if next == 0 { None } else { Some(next) })?;
        Ok(())
    }

    pub(super) fn write_class(&mut self, key: Hash64, new: Option<PalwClassStateV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.classes.contains_key(&key), "native class writer")?;
        let old = match &new {
            Some(record) => self.state.classes.insert(key, record.clone()),
            None => self.state.classes.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Class { key, old, new });
        Ok(())
    }

    pub(super) fn write_target(&mut self, key: Hash64, new: Option<PalwClassTargetV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.class_targets.contains_key(&key), "native target writer")?;
        let old = match &new {
            Some(record) => self.state.class_targets.insert(key, record.clone()),
            None => self.state.class_targets.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Target { key, old, new });
        Ok(())
    }

    pub(super) fn write_share(&mut self, key: Hash64, new: Option<u16>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.class_shares.contains_key(&key), "native share writer")?;
        let old = match new {
            Some(value) => self.state.class_shares.insert(key, value),
            None => self.state.class_shares.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Share { key, old, new });
        Ok(())
    }

    pub(super) fn write_certified_family(
        &mut self,
        lane: PalwCertifiedLaneV1,
        key: Hash64,
        new: Option<PalwCertifiedFamilyStateV2>,
    ) -> Result<(), PalwStateV2Error> {
        match lane {
            PalwCertifiedLaneV1::Attempt => {
                self.reserve_map_row_v1(&key, &new, |s| &s.certified_families, false, "native certified_families writer")?
            }
            PalwCertifiedLaneV1::FreePrompt => {
                self.reserve_map_row_v1(&key, &new, |s| &s.fp_certified_families, false, "native fp_certified_families writer")?
            }
        }
        let map = match lane {
            PalwCertifiedLaneV1::Attempt => &mut self.state.certified_families,
            PalwCertifiedLaneV1::FreePrompt => &mut self.state.fp_certified_families,
        };
        let old = match &new {
            Some(record) => map.insert(key, record.clone()),
            None => map.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::CertifiedFamily { lane, key, old, new });
        Ok(())
    }

    pub(super) fn write_fp_certified_class(
        &mut self,
        key: Hash64,
        new: Option<PalwClassLaneCertificationV2>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(
            &key,
            &new,
            !self.state.fp_certified_classes.contains_key(&key),
            "native fp_certified_classes writer",
        )?;
        let old = match &new {
            Some(record) => self.state.fp_certified_classes.insert(key, record.clone()),
            None => self.state.fp_certified_classes.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::FpCertifiedClass { key, old, new });
        Ok(())
    }

    pub(super) fn write_pending_chunks(&mut self, key: Hash64, new: Option<PalwPendingChunksV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.pending_chunks.contains_key(&key), "native pending_chunks writer")?;
        let old = match &new {
            Some(record) => self.state.pending_chunks.insert(key, record.clone()),
            None => self.state.pending_chunks.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::PendingChunks { key, old, new });
        Ok(())
    }

    pub(super) fn write_court_close_group(
        &mut self,
        key: (Hash64, PalwCourtSideV1),
        new: Option<PalwCourtCloseGroupV2>,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the row, its assembly-window index node and the IR session clock's re-index node, from the
        // actual row and the actual session (`retain` frees; only the re-insert can allocate).
        self.reserve_map_row_v1(&key, &new, |s| &s.court_close_groups, false, "native court_close_groups writer")?;
        let deadline_node =
            new.as_ref().is_some_and(|record| !self.state.court_close_deadlines.contains(&(record.assembly_deadline_daa, key)));
        self.reserve_index_members_v1::<(u64, (Hash64, PalwCourtSideV1)), ()>(
            u64::from(deadline_node),
            "native court_close_deadlines index",
        )?;
        self.reserve_index_members_v1::<(u64, Hash64), ()>(
            u64::from(self.state.court_sessions.contains_key(&key.0)),
            "native court_deadlines session re-index",
        )?;
        let old = match &new {
            Some(record) => self.state.court_close_groups.insert(key, record.clone()),
            None => self.state.court_close_groups.remove(&key),
        };
        // The assembly-window queue, maintained where the row is (audit H-2c). `assembly_deadline_daa`
        // never moves after the declaration writes it, so this is a write-once insert and a
        // remove — but it is spelled as a full re-index anyway, because an index that is correct
        // only while a field happens not to move is an index the next editor breaks silently.
        if let Some(previous) = &old {
            self.state.court_close_deadlines.remove(&(previous.assembly_deadline_daa, key));
        }
        if let Some(record) = &new {
            self.state.court_close_deadlines.insert((record.assembly_deadline_daa, key));
        }
        self.entries.push(PalwDeltaEntryV2::CourtCloseGroup { key, old, new });
        // RFC-0002 F7: an IR session's clock reads its executor's declared close
        // (`court_session_turn_and_rung_deadline_v2`), so the session is re-indexed when a group comes
        // or goes — a no-op for every other session, whose key does not read the groups.
        let session_id = key.0;
        if let Some(session) = self.state.court_sessions.get(&session_id) {
            let next = court_next_deadline_v2(&self.state, session);
            self.state.court_deadlines.retain(|(_, id)| *id != session_id);
            self.state.court_deadlines.insert((next, session_id));
        }
        Ok(())
    }

    pub(super) fn write_derived_artifact(
        &mut self,
        key: crate::palw_derived_v1::PalwDerivedKeyV1,
        new: Option<crate::palw_derived_v1::PalwDerivedRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.derived_artifacts.contains_key(&key), "native derived_artifacts writer")?;
        let old = match &new {
            Some(record) => self.state.derived_artifacts.insert(key, record.clone()),
            None => self.state.derived_artifacts.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::DerivedArtifact { key, old, new });
        Ok(())
    }

    #[allow(dead_code)] // ADR-0045 Decision 2's boundary derivation is the writer; it lands next.
    pub(super) fn write_epoch_budgets(&mut self, new: Option<PalwEpochBudgetsV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_scalar_write_v1(&new, 1, "native epoch_budgets writer")?;
        let old = std::mem::replace(&mut self.state.epoch_budgets, new.clone());
        self.entries.push(PalwDeltaEntryV2::EpochBudgets { old, new });
        Ok(())
    }

    pub(super) fn write_claim_semantics_v1(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.claim_semantics.contains_key(&key), "native claim_semantics writer")?;
        let old = match &new {
            Some(record) => self.state.claim_semantics.insert(key, record.clone()),
            None => self.state.claim_semantics.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::ClaimSemantics { key, old, new });
        Ok(())
    }

    pub(super) fn write_class_semantics_v1(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_class_semantics_v1::PalwClassSemanticsV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.class_semantics.contains_key(&key), "native class_semantics_v1 writer")?;
        let old = match &new {
            Some(row) => self.state.class_semantics.insert(key, row.clone()),
            None => self.state.class_semantics.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::ClassSemantics { key, old, new });
        Ok(())
    }

    pub(super) fn write_receipt_target(&mut self, key: Hash64, new: Option<PalwClassTargetV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.receipt_targets.contains_key(&key), "native receipt_target writer")?;
        let old = match &new {
            Some(record) => self.state.receipt_targets.insert(key, record.clone()),
            None => self.state.receipt_targets.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::ReceiptTarget { key, old, new });
        Ok(())
    }

    pub(super) fn write_claim(&mut self, key: Hash64, new: Option<PalwClaimStateV2>) -> Result<(), PalwStateV2Error> {
        let (requests, slots) =
            if self.native_reservation.is_tracking_v1() { self.claim_cascade_requests_v1(key, &new)? } else { (0, 0) };
        self.reserve_mutation_sequence_v1(requests, slots)?;
        self.reserve_map_journal_v1(&key, &new, !self.state.claims.contains_key(&key), "native claims writer")?;
        // **X3 × retention (REQ-X3-6; Lead default 2026-10-15 round 3; `palw_claim_hard_deadline_v1`)**: a claim CREATED past the
        // fence owes its material — trace, DA units, every hold that reads the record — through its hard deadline, fixed at
        // acceptance, so a pursuit X3 admits can start from public material any time before `hard − B_remaining`. Raised once, here,
        // the one site every creation passes; never moved after (REQ-X3-3). Below the fence (every shipped ruleset) `None`: the record
        // is written byte for byte.
        let new = match new {
            Some(mut claim) if !self.state.claims.contains_key(&key) => {
                if let Some(retention) = crate::palw_ruling_fences_v1::palw_claim_x3_retention_daa_v1(self.params, &claim) {
                    claim.trace_retention_daa = retention;
                }
                Some(claim)
            }
            other => other,
        };
        // Stamp once, at creation. A later prospective descriptor never changes an old claim.
        if self.extras.class_semantics_active
            && !self.state.claims.contains_key(&key)
            && let Some(claim) = &new
            && let Some(row) = self.state.class_semantics.get(&claim.class_id).cloned()
            && claim.accepted_daa >= row.bound_from_daa
        {
            self.write_claim_semantics_v1(key, Some(row))?;
        }
        if new.is_none()
            && !self.state.panel_liabilities.contains_key(&key)
            && !self.state.vesting.contains_key(&key)
            && self.state.claim_semantics.contains_key(&key)
        {
            self.write_claim_semantics_v1(key, None)?;
        }
        if new.is_none() && self.state.fp_public_jobs.contains_key(&key) {
            self.write_public_fp_job_v1(key, None)?;
        }
        if new.is_none() && self.state.exec_v2_origins.contains_key(&key) {
            self.write_exec_origin(key, None)?;
        }
        // ADR-0088 Decision 3: the claim's root goes with the claim.
        if new.is_none() && self.state.claim_roots.contains_key(&key) {
            self.write_claim_root(key, None)?;
        }
        // ADR-0100 Decision 4: a claim's licensing progress exists only while it is `PanelBound`,
        // so every way out of that phase — licensed, voided, redrawn to `Provisional`, retired —
        // drops it HERE, the one site all of them already pass through.
        if self.state.shard_licensing.contains_key(&key)
            && !new.as_ref().is_some_and(|claim| matches!(claim.phase, PalwClaimPhaseV2::PanelBound { .. }))
        {
            self.write_shard_licensing(key, None)?;
        }
        // ADR-0103 Decision 4: the unit a held accusation named lives exactly as long as the
        // disputed phase it was named in — the disclosure, the sweep's default, a court's void and
        // every other door out of `DefaultDisputed` drop it HERE, the one site all of them pass.
        if self.state.held_da_missing.contains_key(&key)
            && !new.as_ref().is_some_and(|claim| matches!(claim.phase, PalwClaimPhaseV2::DefaultDisputed { .. }))
        {
            self.write_held_da_missing(key, None)?;
        }
        // ADR-0111 Decision 3: the demand record lives as long as the claim can still be tried.
        if self.state.held_leaf_demands.contains_key(&key) && new.as_ref().is_none_or(|claim| claim.phase.is_terminal()) {
            self.write_held_leaf_demands(key, None)?;
        }
        // 4-ter.3 step 6: so do the held forfeits — every door to a terminal phase drops them HERE.
        if new.as_ref().is_none_or(|claim| claim.phase.is_terminal()) {
            let sessions: Vec<Hash64> = self.state.held_forfeits_of_claim(&key).map(|(session, _)| *session).collect();
            for session in sessions {
                self.write_held_forfeit((key, session), None)?;
            }
        }
        // RFC-0007 Part I: a claim's tally lives exactly while it is `PanelBound` (every door out of that phase — licensed, voided,
        // redrawn — drops it HERE), and its `Held` attestations while it is live.
        if self.state.vertex.tallies.contains_key(&key)
            && !new.as_ref().is_some_and(|claim| matches!(claim.phase, PalwClaimPhaseV2::PanelBound { .. }))
        {
            self.write_vertex_tally(key, None)?;
        }
        if self.state.vertex.held.contains_key(&key) && new.as_ref().is_none_or(|claim| claim.phase.is_terminal()) {
            self.write_vertex_held(key, None)?;
        }
        // RFC-0007 Part IV.2: a capped claim's row lives exactly while it waits `Provisional` for the holders; the moment it binds a
        // panel (the ordinary path takes it from there) or leaves the chain, the row goes.
        if self.state.vertex.mesh.capped.contains_key(&key)
            && !new.as_ref().is_some_and(|claim| matches!(claim.phase, PalwClaimPhaseV2::Provisional))
        {
            self.write_mesh_capped(key, None)?;
        }
        // ADR-0124 Decision 3: the seats' duty — and the exposure it stands for — lives exactly as
        // long as the claim is live. Every door to a terminal phase passes here; the exposure is
        // released by the same doors (`release_seat_duties`), BEFORE this write drops the row it
        // reads, so the row and the ledger never disagree about who holds what.
        if self.state.panel_duties.contains_key(&key) && new.as_ref().is_none_or(|claim| claim.phase.is_terminal()) {
            self.write_panel_duties(key, None)?;
        }
        // RFC-0006: a claim's per-shard record lives while the claim does — through `PanelBound` and `ReceiptLicensed`,
        // where the lock, the recount and the pay read it — and goes with it: terminal, retired, or redrawn to
        // `Provisional` (its next panel writes a fresh one).
        if self.state.tir_shard_claims.contains_key(&key)
            && new.as_ref().is_none_or(|claim| claim.phase.is_terminal() || matches!(claim.phase, PalwClaimPhaseV2::Provisional))
        {
            self.write_tir_shard_claim(key, None)?;
        }
        let old = match &new {
            Some(record) => self.state.claims.insert(key, record.clone()),
            None => self.state.claims.remove(&key),
        };
        // ADR-0160 F-W: the bond's staged weight, held reservation and capped term follow the record.
        self.weight_cap_on_write(old.as_ref(), new.as_ref());
        // Audit #13b: the registry's in-flight index, where this fold has built it, moves with
        // the claim — the old record out, the new one in, each only while non-terminal.
        if let Some(index) = self.inflight_index.get_mut().as_mut() {
            if let Some(previous) = &old {
                palw_inflight_index_note_v1(index, previous, false);
            }
            if let Some(record) = &new {
                palw_inflight_index_note_v1(index, record, true);
            }
        }
        // The unresolved index tracks non-terminal claims; keep it in lockstep with every write.
        if let Some(previous) = &old
            && !previous.phase.is_terminal()
        {
            self.state.unresolved.remove(&(previous.accepted_blue_score, key));
        }
        if let Some(record) = &new
            && !record.phase.is_terminal()
        {
            self.state.unresolved.insert((record.accepted_blue_score, key));
        }
        // The work-id index follows the claim's own field the same way (ADR-0074 Decision 4).
        if let Some(previous) = &old
            && let Some(work_id) = previous.work_id
        {
            self.state.work_ids.remove(&work_id);
        }
        if let Some(record) = &new
            && let Some(work_id) = record.work_id
        {
            self.state.work_ids.insert(work_id, key);
        }
        // **The prompt row OUTLIVES its claim** (re-audit 2026-09-19). It used to leave here, with
        // the work-id index, so that the cache rule's window and the duplicate rule's window were
        // one window. They are not one question. `work_ids` asks "is this the same inference a LIVE
        // claim already names", and a retired claim's inference may honestly be run again. The
        // cache rule asks "has this prefill already been paid for", and a retirement does not unpay
        // it — so dropping the row here sold the same prefix a second time on the same bond, for
        // the price of waiting out a challenge window.
        //
        // Keeping it makes the credit a pure function of the prompts committed in this candidate
        // chain, which is what makes the total invariant under retire-and-replay the way it is
        // already invariant under ordering and under bond splitting. A reverted delta still takes
        // the row with it: the write is recorded as a delta entry like any other, so a reorg that
        // undoes the commitment undoes its row and the credit is recomputed on the new chain.
        let _ = &old;
        // **ADR-0062 SA-1: the accuser's stake comes back HERE**, for the reason `write_court`
        // gives about the challenger's — one site that every door already passes through.
        //
        // `DefaultDisputed` is the first phase in this ruleset that holds a SECOND bond's
        // reservation, and a claim leaves it by more doors than the DA session's own sweep: the
        // disclosure arm resumes it, a `CourtClosed { ExecutorGuilty }` on a session that was
        // already open voids it, and so does `ProducerDefaulted` wherever ADR-0065 D4 is not
        // armed. None of those knows what an accusation cost — `release_for_claim` gives back
        // what the PRODUCER put up — and a release each of them had to remember is a release one
        // of them eventually will not.
        //
        // Left behind, the accuser's sompi are state the fold WRITES and cannot READ BACK:
        // `assert_internal_consistency` rebuilds this ledger from the claims, so the next
        // `load_tip` refuses the snapshot the chain just wrote, on every node at once.
        //
        // Only when the claim STOPS being disputed — a rewrite that leaves the phase in place
        // must not give the stake back while the session is still open. Saturating for the reason
        // the challenger's release is: a ledger that cannot go negative is worth more here than
        // an error nobody can clear.
        if let Some(previous) = &old
            && let PalwClaimPhaseV2::DefaultDisputed { accuser, accuser_exposure, .. } = &previous.phase
            && !matches!(new.as_ref().map(|record| &record.phase), Some(PalwClaimPhaseV2::DefaultDisputed { .. }))
        {
            let (accuser, accuser_exposure) = (*accuser, *accuser_exposure);
            let held = self.state.reserved_exposure.get(&accuser).copied().unwrap_or(0);
            let back = held.saturating_sub(accuser_exposure);
            self.write_exposure(accuser, if back == 0 { None } else { Some(back) })?;
        }
        self.entries.push(PalwDeltaEntryV2::Claim { key, old, new });
        Ok(())
    }

    pub(super) fn write_payout(&mut self, key: Hash64, new: Option<PalwPayoutV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.pending_payouts, false, "native pending_payouts writer")?;
        let old = match new {
            Some(record) => self.state.pending_payouts.insert(key, record),
            None => self.state.pending_payouts.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Payout { key, old, new });
        Ok(())
    }

    pub(super) fn write_model_market(&mut self, key: Hash64, new: Option<crate::palw_model_market_v1::PalwModelMarketV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.model_markets, false, "native model_markets writer")?;
        let old = match new {
            Some(record) => self.state.model_markets.insert(key, record),
            None => self.state.model_markets.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::ModelMarket { key, old, new });
        Ok(())
    }

    pub(super) fn write_model_position(&mut self, key: (Hash64, Hash64), units: u64) -> Result<(), PalwStateV2Error> {
        let new = (units > 0).then_some(units);
        self.reserve_map_row_v1(&key, &new, |s| &s.model_positions, false, "native model_positions writer")?;
        let old = match new {
            Some(record) => self.state.model_positions.insert(key, record),
            None => self.state.model_positions.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ModelPosition { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0095 §4.5 — the tenure clock, moved from the one place a balance changes.**
    ///
    /// It starts when a balance rises from zero, is PRESERVED when a holder buys more, and is
    /// reset to the current height whenever the balance falls. So it measures how long since the
    /// holder last sold, which is the honest description of what a membership should reward:
    /// punishing someone for increasing their stake would be perverse, and rewarding someone who
    /// sold last block would make `min_hold_daa` decorative.
    pub(super) fn touch_model_position_tenure(&mut self, key: (Hash64, Hash64), before: u64, after: u64, daa: u64) -> Result<(), PalwStateV2Error> {
        if !self.extras.model_benefits_active {
            return Ok(());
        }
        let old = self.state.model_position_since.get(&key).copied();
        let new = if after == 0 {
            None
        } else if before == 0 || after < before {
            Some(daa)
        } else {
            old.or(Some(daa))
        };
        self.reserve_map_row_v1(&key, &new, |s| &s.model_position_since, false, "native model_position_since writer")?;
        if old != new {
            match new {
                Some(v) => self.state.model_position_since.insert(key, v),
                None => self.state.model_position_since.remove(&key),
            };
            self.entries.push(PalwDeltaEntryV2::ModelPositionSince { key, old, new });
        }
        Ok(())
    }

    /// ADR-0095 §4.1.
    pub(super) fn write_model_benefits(&mut self, key: Hash64, new: Option<crate::palw_model_benefits_v1::PalwModelBenefitsV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.model_benefits, false, "native model_benefits writer")?;
        let old = match &new {
            Some(record) => self.state.model_benefits.insert(key, record.clone()),
            None => self.state.model_benefits.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ModelBenefits { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_model_line(&mut self, key: Hash64, new: Option<crate::palw_model_lines_v1::PalwModelLineV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.model_lines, false, "native model_lines writer")?;
        let old = match &new {
            Some(record) => self.state.model_lines.insert(key, record.clone()),
            None => self.state.model_lines.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ModelLine { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_model_version(&mut self, key: (Hash64, u32), new: Option<crate::palw_model_lines_v1::PalwModelVersionV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.model_versions, false, "native model_versions writer")?;
        let old = match &new {
            Some(record) => self.state.model_versions.insert(key, record.clone()),
            None => self.state.model_versions.remove(&key),
        };
        if old != new {
            let dropped_root = match (&old, &new) {
                (Some(before), None) => Some(before.root),
                (Some(before), Some(after)) if before.root != after.root => Some(before.root),
                _ => None,
            };
            self.entries.push(PalwDeltaEntryV2::ModelVersion { key, old, new });
            if let Some(root) = dropped_root {
                self.release_artifact_root_if_unused(key.0, root)?;
            }
        }
        Ok(())
    }

    /// **ADR-0143: an owner row lives exactly as long as something it describes** (the 2026-09-19
    /// audit).
    ///
    /// The index shipped with no cap, no eviction and no TTL, while every neighbouring registry
    /// table has one — so a version publication with a fresh root bought a permanent row in the
    /// state root for a transaction fee, and the row outlived the version, because
    /// `model_versions` evicts past `PALW_MODEL_VERSION_HISTORY_V1` and the owner row did not.
    ///
    /// Tying the row's life to a carrier makes the index a function of `model_versions` and
    /// `classes`, so it inherits their bounds rather than having none — and it removes the other
    /// half of the same defect, a root reserved for ever by a line that no longer serves it.
    /// A class's registered founding root is never released: the class row is its carrier.
    pub(super) fn release_artifact_root_if_unused(&mut self, line_id: Hash64, root: Hash64) -> Result<(), PalwStateV2Error> {
        if !self.extras.artifact_root_ownership_active {
            return Ok(());
        }
        let Some(line) = self.state.model_lines.get(&line_id).map(|l| l.class_id).or(Some(line_id)) else { return Ok(()) };
        let key = (line, root);
        let Some(owner) = self.state.artifact_owners.get(&key).copied() else { return Ok(()) };
        if owner.line_id != line_id {
            return Ok(());
        }
        if self.state.classes.get(&line).is_some_and(|class| class.artifact_root == root) {
            return Ok(());
        }
        let still_carried =
            self.state.model_versions.range((line_id, 0)..=(line_id, u32::MAX)).any(|(_, version)| version.root == root);
        if !still_carried {
            self.write_artifact_owner(key, None)?;
        }
        Ok(())
    }

    pub(super) fn write_model_proposal(&mut self, key: Hash64, new: Option<crate::palw_model_lines_v1::PalwModelProposalV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.model_proposals, false, "native model_proposals writer")?;
        let old = match &new {
            Some(record) => self.state.model_proposals.insert(key, record.clone()),
            None => self.state.model_proposals.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ModelProposal { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_model_evaluation(
        &mut self,
        key: (Hash64, u32, PalwBondKeyV2),
        new: Option<crate::palw_model_lines_v1::PalwModelEvaluationV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.model_evaluations, false, "native model_evaluations writer")?;
        let old = match &new {
            Some(record) => self.state.model_evaluations.insert(key, record.clone()),
            None => self.state.model_evaluations.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ModelEvaluation { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_evm_settlement(&mut self, key: u32, new: Option<crate::evm::model_market::PalwEvmSettlementV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.evm_settlements, false, "native evm_settlements writer")?;
        let old = match new {
            Some(row) => self.state.evm_settlements.insert(key, row),
            None => self.state.evm_settlements.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::EvmSettlement { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_class_shard_plan(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_shard_licensing_v1::PalwClassShardPlanV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.class_shard_plans.contains_key(&key), "native class_shard_plan writer")?;
        let old = match new {
            Some(row) => self.state.class_shard_plans.insert(key, row),
            None => self.state.class_shard_plans.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ClassShardPlan { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_bond_shards(&mut self, key: (PalwBondKeyV2, Hash64), new: Option<Vec<u32>>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.bond_shards.contains_key(&key), "native bond_shards writer")?;
        let old = match &new {
            Some(row) => self.state.bond_shards.insert(key, row.clone()),
            None => self.state.bond_shards.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::BondShards { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_shard_licensing(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.shard_licensing, false, "native shard_licensing writer")?;
        let old = match &new {
            Some(row) => self.state.shard_licensing.insert(key, row.clone()),
            None => self.state.shard_licensing.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ShardLicensing { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_held_da_missing(&mut self, key: Hash64, new: Option<crate::palw_held_da_v1::PalwHeldMissingV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.held_da_missing, false, "native held_da_missing writer")?;
        let old = match new {
            Some(row) => self.state.held_da_missing.insert(key, row),
            None => self.state.held_da_missing.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::HeldDaMissing { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_class_step_ladder(&mut self, key: Hash64, new: Option<u64>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.class_step_ladders.contains_key(&key), "native class_step_ladder writer")?;
        let old = match new {
            Some(ladder) => self.state.class_step_ladders.insert(key, ladder),
            None => self.state.class_step_ladders.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ClassStepLadder { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0165: write the floor state** (canonical: the default is `None`), one delta entry per change.
    pub(super) fn write_floor_state(&mut self, new: Option<crate::palw_real_share_v1::PalwFloorStateV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native floor_state writer")?;
        let old = self.state.floor_state;
        if old != new {
            self.state.floor_state = new;
            self.entries.push(PalwDeltaEntryV2::RealWork { old, new });
        }
        Ok(())
    }

    /// **ADR-0165: the floor state's time step, at the start of a block's fold** — Normal and Probe expire here, and nowhere
    /// else is time read. A no-op below `Params::palw_floor_reserve_v1`.
    pub(super) fn advance_floor_state(&mut self, daa: u64) -> Result<(), PalwStateV2Error> {
        if !self.params.floor_reserve_active_at(daa) {
            return Ok(());
        }
        let next = crate::palw_real_share_v1::palw_floor_step_v1(self.state.floor_state_v1(), daa, None);
        self.write_floor_state(next.canonical())?;
        Ok(())
    }

    /// **ADR-0165: one REAL attempt FULLY ACCEPTED** — every check passed and its claim is about to be written — steps the
    /// floor state at the accepting block's DAA, with the colour it had in that block's mergeset.
    pub(super) fn note_real_accepted(&mut self, daa: u64, blue: bool) -> Result<(), PalwStateV2Error> {
        let next = crate::palw_real_share_v1::palw_floor_step_v1(
            self.state.floor_state_v1(),
            daa,
            Some(crate::palw_real_share_v1::PalwRealAcceptedV1 { blue }),
        );
        self.write_floor_state(next.canonical())?;
        Ok(())
    }

    pub(super) fn write_class_court_window(&mut self, key: Hash64, new: Option<u64>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(
            &key,
            &new,
            !self.state.class_court_windows.contains_key(&key),
            "native class_court_window writer",
        )?;
        let old = match new {
            Some(window) => self.state.class_court_windows.insert(key, window),
            None => self.state.class_court_windows.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ClassCourtWindow { key, old, new });
        }
        Ok(())
    }

    /// **RFC-0007: the one journaled writer of every vertex table** (`VertexRow`, 101): `table` names the table, and the key and
    /// rows ride as their borsh bytes.
    pub(super) fn write_vertex_row<K, V>(
        &mut self,
        table: u8,
        key: K,
        new: Option<V>,
        map: fn(&mut PalwChainStateV2) -> &mut BTreeMap<K, V>,
    ) -> Result<Option<V>, PalwStateV2Error>
    where
        K: Ord + Clone + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeHeapV1,
        V: Clone + PartialEq + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeHeapV1,
    {
        self.reserve_encoded_row_v1(&key, &new, map, "native vertex row writer")?;
        let rows = map(&mut self.state);
        let old = match new.clone() {
            Some(row) => rows.insert(key.clone(), row),
            None => rows.remove(&key),
        };
        if old != new {
            let encode = |row: &V| borsh::to_vec(row).expect("a vertex row is borsh-serializable");
            self.entries.push(PalwDeltaEntryV2::VertexRow {
                table,
                key: borsh::to_vec(&key).expect("a vertex key is borsh-serializable"),
                old: old.as_ref().map(encode),
                new: new.as_ref().map(encode),
            });
        }
        Ok(old)
    }

    /// **RFC-0007: the one writer of the `(round, seat)` rows.**
    pub(crate) fn write_vertex_round(
        &mut self,
        key: (u64, PalwBondKeyV2),
        new: Option<crate::palw_vertex_v1::PalwVertexRoundRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_vertex_row(PALW_VERTEX_TABLE_ROUNDS_V1, key, new, |s| &mut s.vertex.rounds).map(drop)
    }

    /// **RFC-0007: the one writer of the per-claim tallies.**
    pub(crate) fn write_vertex_tally(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_vertex_v1::PalwVertexTallyV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_vertex_row(PALW_VERTEX_TABLE_TALLIES_V1, key, new, |s| &mut s.vertex.tallies).map(drop)
    }

    /// **RFC-0007: the one writer of the per-claim `Held` attestations.**
    pub(crate) fn write_vertex_held(
        &mut self,
        key: Hash64,
        new: Option<Vec<crate::palw_vertex_v1::PalwVertexHeldRowV1>>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_vertex_row(PALW_VERTEX_TABLE_HELD_V1, key, new, |s| &mut s.vertex.held).map(drop)
    }

    /// **RFC-0007 Part II: the one writer of the witness profiles** (by class).
    pub(crate) fn write_mesh_witness(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_mesh_v1::PalwWitnessProfileRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_vertex_row(PALW_MESH_TABLE_WITNESS_V1, key, new, |s| &mut s.vertex.mesh.witness).map(drop)
    }

    /// **RFC-0007 Part IV.1: the one writer of the audit rows** (by claim).
    pub(crate) fn write_mesh_audit(&mut self, key: Hash64, new: Option<crate::palw_mesh_v1::PalwAuditRowV1>) -> Result<(), PalwStateV2Error> {
        self.write_vertex_row(PALW_MESH_TABLE_AUDITS_V1, key, new, |s| &mut s.vertex.mesh.audits).map(drop)
    }

    /// **RFC-0007 Part IV.1: the one writer of the trap rows** (by commitment).
    pub(crate) fn write_mesh_trap(&mut self, key: Hash64, new: Option<crate::palw_mesh_v1::PalwTrapRowV1>) -> Result<(), PalwStateV2Error> {
        self.write_vertex_row(PALW_MESH_TABLE_TRAPS_V1, key, new, |s| &mut s.vertex.mesh.traps).map(drop)
    }

    /// **RFC-0007 Part IV.2: the one writer of the capped claim rows** (by claim).
    pub(crate) fn write_mesh_capped(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_mesh_v1::PalwCappedClaimRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_vertex_row(PALW_MESH_TABLE_CAPPED_V1, key, new, |s| &mut s.vertex.mesh.capped).map(drop)
    }

    /// **RFC-0006: the one writer of `tir_shard_plans`**, journaled `TirShardPlan`.
    pub(super) fn write_tir_shard_plan(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_tir_shard_v1::PalwTirShardPlanV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.tir_shard_plans.contains_key(&key), "native tir_shard_plan writer")?;
        let old = match &new {
            Some(row) => self.state.tir_shard_plans.insert(key, row.clone()),
            None => self.state.tir_shard_plans.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::TirShardPlan { key, old, new });
        }
        Ok(())
    }

    /// **RFC-0006: the one writer of `tir_shard_claims`**, journaled `TirShardClaim`.
    pub(super) fn write_tir_shard_claim(&mut self, key: Hash64, new: Option<crate::palw_tir_shard_v1::PalwTirShardClaimV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.tir_shard_claims, false, "native tir_shard_claims writer")?;
        let old = match &new {
            Some(row) => self.state.tir_shard_claims.insert(key, row.clone()),
            None => self.state.tir_shard_claims.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::TirShardClaim { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_seat_root_readiness(
        &mut self,
        key: (PalwBondKeyV2, Hash64, Hash64),
        new: Option<crate::palw_model_registry_v1::PalwSeatReadinessRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.seat_root_readiness, false, "native seat_root_readiness writer")?;
        let old = match new {
            Some(row) => self.state.seat_root_readiness.insert(key, row),
            None => self.state.seat_root_readiness.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::SeatRootReadiness { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_seat_availability(
        &mut self,
        key: PalwBondKeyV2,
        new: Option<crate::palw_seat_availability_v1::PalwSeatAvailabilityV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.seat_availability, false, "native seat_availability writer")?;
        let old = match new.clone() {
            Some(row) => self.state.seat_availability.insert(key, row),
            None => self.state.seat_availability.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::SeatAvailability { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_held_leaf_demands(&mut self, key: Hash64, new: Option<BTreeMap<PalwBondKeyV2, u64>>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.held_leaf_demands, false, "native held_leaf_demands writer")?;
        let old = match new.clone() {
            Some(row) => self.state.held_leaf_demands.insert(key, row),
            None => self.state.held_leaf_demands.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::HeldLeafDemands { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_panel_duties(&mut self, key: Hash64, new: Option<PalwPanelDutyRowV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.panel_duties, false, "native panel_duties writer")?;
        let old = match new.clone() {
            Some(row) => self.state.panel_duties.insert(key, row),
            None => self.state.panel_duties.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::PanelDuties { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_panel_reserve(&mut self, new: u64) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native panel_reserve writer")?;
        let old = self.state.panel_reserve_sompi;
        if old != new {
            self.state.panel_reserve_sompi = new;
            self.entries.push(PalwDeltaEntryV2::PanelReserve { old, new });
        }
        Ok(())
    }

    pub(super) fn write_round_span(&mut self, new: u64) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native round_span writer")?;
        let old = self.state.round_span;
        if old != new {
            self.state.round_span = new;
            self.entries.push(PalwDeltaEntryV2::RoundSpan { old, new });
        }
        Ok(())
    }

    pub(super) fn write_round_final(&mut self, key: Hash64, new: Option<crate::palw_execution_lane_v1::PalwExecFinalV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.round_finals, false, "native round_finals writer")?;
        let old = match new {
            Some(row) => self.state.round_finals.insert(key, row),
            None => self.state.round_finals.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::RoundFinal { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_round_schedule(&mut self, key: u64, new: Option<crate::palw_execution_lane_v1::PalwExecScheduleV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.round_schedules, false, "native round_schedules writer")?;
        let old = match new.clone() {
            Some(row) => self.state.round_schedules.insert(key, row),
            None => self.state.round_schedules.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::RoundSchedule { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_round_permits_used(&mut self, key: (u64, u64), new: Option<u16>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.round_permits_used, false, "native round_permits_used writer")?;
        let old = match new {
            Some(bits) => self.state.round_permits_used.insert(key, bits),
            None => self.state.round_permits_used.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::RoundPermitsUsed { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_round_equivocations(&mut self, key: (u64, u64), new: Option<u16>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.round_equivocations, false, "native round_equivocations writer")?;
        let old = match new {
            Some(bits) => self.state.round_equivocations.insert(key, bits),
            None => self.state.round_equivocations.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::RoundEquivocations { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_round_pending(&mut self, key: u64, new: Option<crate::palw_execution_lane_v1::PalwExecSnapshotV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.round_pending, false, "native round_pending writer")?;
        let old = match new.clone() {
            Some(row) => self.state.round_pending.insert(key, row),
            None => self.state.round_pending.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::RoundPending { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_round_seed_anchor(&mut self, new: Option<crate::palw_execution_lane_v1::PalwExecSeedAnchorV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native round_seed_anchor writer")?;
        let old = self.state.round_seed_anchor;
        if old != new {
            self.state.round_seed_anchor = new;
            self.entries.push(PalwDeltaEntryV2::RoundSeedAnchor { old, new });
        }
        Ok(())
    }

    pub(super) fn write_model_lifecycle(&mut self, key: Hash64, new: Option<crate::palw_model_registry_v1::PalwModelLifecycleRowV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.model_lifecycles, false, "native model_lifecycles writer")?;
        // The panel budget's horizon reads these rows (audit #13b's cache).
        self.panel_horizon.set(None);
        let old = match new.clone() {
            Some(row) => self.state.model_lifecycles.insert(key, row),
            None => self.state.model_lifecycles.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ModelLifecycle { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_seat_readiness(
        &mut self,
        key: (PalwBondKeyV2, Hash64),
        new: Option<crate::palw_model_registry_v1::PalwSeatReadinessRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.seat_readiness, false, "native seat_readiness writer")?;
        let old = match new {
            Some(row) => self.state.seat_readiness.insert(key, row),
            None => self.state.seat_readiness.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::SeatReadiness { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_claim_economics(&mut self, key: Hash64, new: Option<crate::palw_economic_payout_v1::PalwClaimEconomicsV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.claim_economics, false, "native claim_economics writer")?;
        let old = match new {
            Some(row) => self.state.claim_economics.insert(key, row),
            None => self.state.claim_economics.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ClaimEconomics { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_work_target_shadow(&mut self, new: Option<crate::palw_work_target_v1::PalwWorkTargetV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native work_target_shadow writer")?;
        let old = std::mem::replace(&mut self.state.work_target_shadow, new);
        if old != new {
            self.entries.push(PalwDeltaEntryV2::WorkTargetShadow { old, new });
        }
        Ok(())
    }

    pub(super) fn write_work_target(&mut self, new: Option<crate::palw_work_target_v1::PalwWorkTargetV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native work_target writer")?;
        let old = std::mem::replace(&mut self.state.work_target, new);
        if old != new {
            self.entries.push(PalwDeltaEntryV2::WorkTarget { old, new });
        }
        Ok(())
    }

    /// ADR-0143 Decision 3: the ONE place a root's owner is written. Every entrance — the founding
    /// reservation, a line founding, a version publication and the activation migration — goes
    /// through here, so "who owns this root" has one writer as well as one reader.
    /// **ADR-0145 §5: record a class's graph.** Only the lane certification calls this, and only
    /// past the fence; the row is what makes a derivation possible at all, so it is written from a
    /// profile the caller has already checked hashes to the class id.
    pub(super) fn write_fp_work_profile(&mut self, key: Hash64, new: Option<Box<crate::palw_step::PalwShapeProfileV3>>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.fp_work_profiles, true, "native fp_work_profiles writer")?;
        let old = self.state.fp_work_profiles.get(&key).cloned();
        if old == new {
            return Ok(());
        }
        match &new {
            Some(profile) => self.state.fp_work_profiles.insert(key, profile.clone()),
            None => self.state.fp_work_profiles.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::FpWorkProfile { key, old, new });
        Ok(())
    }

    /// **ADR-0145 §6: record (or drop) one claim's prompt row.** Written with the commitment and
    /// dropped by `sweep_fp_prompt_rows` when its retention runs out — never with the claim.
    pub(super) fn write_fp_claim_prompt(&mut self, key: (Hash64, PalwBondKeyV2, Hash64), new: Option<PalwFpClaimPromptV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.fp_claim_prompts, true, "native fp_claim_prompts writer")?;
        let old = self.state.fp_claim_prompts.get(&key).cloned();
        if old == new {
            return Ok(());
        }
        match &new {
            Some(row) => self.state.fp_claim_prompts.insert(key, row.clone()),
            None => self.state.fp_claim_prompts.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::FpClaimPrompt { key, old, new });
        Ok(())
    }

    pub(super) fn write_consumed_offence(&mut self, key: Hash64, new: Option<crate::palw_offence_v1::PalwConsumedOffenceV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.consumed_offences, true, "native consumed_offences writer")?;
        let old = self.state.consumed_offences.get(&key).cloned();
        if old == new {
            return Ok(());
        }
        match &new {
            Some(row) => {
                self.state.consumed_offences.insert(key, row.clone());
            }
            None => {
                self.state.consumed_offences.remove(&key);
            }
        };
        self.entries.push(PalwDeltaEntryV2::ConsumedOffence { key, old, new });
        Ok(())
    }

    pub(super) fn write_slashable_lock(
        &mut self,
        key: (PalwBondKeyV2, Hash64),
        new: Option<crate::palw_panel_var_v1::PalwSlashableLockV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.slashable_locks, false, "native slashable_locks writer")?;
        let old = self.state.slashable_locks.get(&key).copied();
        if old == new {
            return Ok(());
        }
        match new {
            Some(lock) => {
                self.state.slashable_locks.insert(key, lock);
            }
            None => {
                self.state.slashable_locks.remove(&key);
            }
        };
        self.entries.push(PalwDeltaEntryV2::SlashableLock { key, old, new });
        Ok(())
    }

    pub(super) fn write_panel_liability(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1>,
    ) -> Result<(), PalwStateV2Error> {
        let requests = if self.native_reservation.is_tracking_v1() { self.panel_liability_copy_requests_v1(&key)? } else { 0 };
        self.reserve_mutation_sequence_v1(requests, 2)?;
        self.reserve_map_journal_v1(&key, &new, !self.state.panel_liabilities.contains_key(&key), "native panel_liabilities writer")?;
        if new.is_none()
            && !self.state.claims.contains_key(&key)
            && !self.state.vesting.contains_key(&key)
            && self.state.claim_semantics.contains_key(&key)
        {
            self.write_claim_semantics_v1(key, None)?;
        }
        let old = self.state.panel_liabilities.get(&key).cloned();
        if old == new {
            return Ok(());
        }
        match &new {
            Some(row) => {
                self.state.panel_liabilities.insert(key, row.clone());
            }
            None => {
                self.state.panel_liabilities.remove(&key);
            }
        };
        self.entries.push(PalwDeltaEntryV2::PanelLiability { key, old, new });
        Ok(())
    }

    /// **R-3's open commitments** (ADR §6 row 13): the one writer of `reporter_commitments`, keeping
    /// both derived indexes (S-SPEC 1g) in lockstep with the map. S-1 lands it with its indexes;
    /// the reporter objects (tags 53/54) and step 2's prune are its callers.
    #[allow(dead_code)]
    pub(super) fn write_reporter_commit(&mut self, key: Hash64, new: Option<PalwReporterCommitV1>) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the row plus its by-DAA node and, for a reporter with no counter yet, its counter node.
        self.reserve_map_row_v1(&key, &new, |s| &s.reporter_commitments, false, "native reporter_commitments writer")?;
        if let Some(record) = &new {
            self.reserve_index_members_v1::<(u64, Hash64), ()>(1, "native commitments_by_daa index")?;
            let counter = u64::from(!self.state.commitments_by_reporter.contains_key(&record.reporter));
            self.reserve_index_members_v1::<PalwBondKeyV2, u32>(counter, "native commitments_by_reporter index")?;
        }
        let old = match new {
            Some(record) => self.state.reporter_commitments.insert(key, record),
            None => self.state.reporter_commitments.remove(&key),
        };
        if old == new {
            return Ok(());
        }
        if let Some(previous) = &old {
            self.state.commitments_by_daa.remove(&(previous.committed_daa, key));
            if let Some(count) = self.state.commitments_by_reporter.get_mut(&previous.reporter) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.state.commitments_by_reporter.remove(&previous.reporter);
                }
            }
        }
        if let Some(record) = &new {
            self.state.commitments_by_daa.insert((record.committed_daa, key));
            let count = self.state.commitments_by_reporter.entry(record.reporter).or_insert(0);
            *count = count.saturating_add(1);
        }
        self.entries.push(PalwDeltaEntryV2::ReporterCommit { key, old, new });
        Ok(())
    }

    /// **M3 (DA-2): open or close one data-availability session** — the rooted write, its delta
    /// entry (74), and both DA indexes kept equal to a rebuild ([`palw_da_indexes_of_v1`]).
    pub(super) fn write_da_session(
        &mut self,
        key: (Hash64, PalwBondKeyV2),
        new: Option<crate::palw_da_rcore_v1::PalwDaSessionV1>,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the row, its deadline node, and the accuser re-index (two scratch sets of the claim's actual
        // accusers plus at most one new by-accuser node), before the scratch sets are built.
        self.reserve_map_row_v1(&key, &new, |s| &s.da_sessions, false, "native da_sessions writer")?;
        self.reserve_da_reindex_v1(&key.0, new.is_some(), 0)?;
        let before = self.da_accusers_of_claim(&key.0);
        let old = match &new {
            Some(session) => self.state.da_sessions.insert(key, session.clone()),
            None => self.state.da_sessions.remove(&key),
        };
        if old == new {
            return Ok(());
        }
        if let Some(previous) = &old {
            self.state.da_deadlines.remove(&(previous.deadline_daa, key.0, key.1));
        }
        if let Some(session) = &new {
            self.state.da_deadlines.insert((session.deadline_daa, key.0, key.1));
        }
        self.reindex_da_accusers(&key.0, before);
        self.entries.push(PalwDeltaEntryV2::DaSession { key, old, new });
        Ok(())
    }

    /// RFC-0006: what one DA re-index allocates — `da_accusers_of_claim` builds two sets (before and after) of the
    /// claim's actual accusers, the session write adds at most one deadline node, and the re-index at most `added`
    /// by-accuser nodes (every accuser of the claim, conservatively). `incoming` counts accusers the new row brings.
    pub(super) fn reserve_da_reindex_v1(&mut self, claim_id: &Hash64, inserts: bool, incoming: u64) -> Result<(), PalwStateV2Error> {
        if !self.native_writers_tracking_v1() {
            return Ok(());
        }
        let sessions = self.state.da_sessions_of(claim_id).count() as u64;
        let refuted = self.state.da_claims.get(claim_id).map_or(0, |record| record.refuted_held.len() as u64);
        let accusers = sessions.saturating_add(refuted).saturating_add(incoming).saturating_add(1);
        let mut q = super::native_writers::NativeWriteQuoteV1::new("native DA accuser re-index scratch");
        q.member::<PalwBondKeyV2, ()>(accusers.saturating_mul(2))?;
        q.member::<(PalwBondKeyV2, Hash64), ()>(accusers)?;
        if inserts {
            q.member::<(u64, Hash64, PalwBondKeyV2), ()>(1)?;
        }
        self.admit_native_write_v1(q)
    }

    /// **ADR-0152-adjacent: write one class's Activation Pool row** — the ONE writer of
    /// `activation_pools` and of its counters, so I2 holds by construction: the row (entry 76) and,
    /// when a sompi field moved, the counters (entry 77) in the same step. Rows are never removed.
    pub(super) fn write_activation_pool(
        &mut self,
        key: Hash64,
        new: crate::palw_activation_pool_v1::PalwActivationPoolV1,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the row (map clone, journal copy, membership) and the counters' second journal slot.
        let quoted = Some(new);
        self.reserve_map_row_v1(&key, &quoted, |s| &s.activation_pools, false, "native activation_pools writer")?;
        self.reserve_journal_slots_v1(1, "native activation pool counters journal")?;
        let Some(new) = quoted else { unreachable!("quoted above") };
        let old = self.state.activation_pools.insert(key, new.clone());
        if old.as_ref() == Some(&new) {
            return Ok(());
        }
        let before = self.state.activation_pool_counters;
        let after = before.moved(old.as_ref(), &new);
        self.entries.push(PalwDeltaEntryV2::ActivationPool { key, old, new: Some(new) });
        if after != before {
            self.state.activation_pool_counters = after;
            self.entries.push(PalwDeltaEntryV2::ActivationPoolCounters { old: before, new: after });
        }
        Ok(())
    }

    /// **ADR-0152-adjacent: one pool payout, accumulated in the payee's pool row** (key prefix
    /// `[0xFE, 0xFF]`, [`crate::palw_activation_pool_v1::palw_activation_pool_payout_key_v1`]) —
    /// `add_panel_payout`'s shape: one payee holds one row however many pools pay it. Written only by
    /// step 3d′'s flush (F5).
    pub(super) fn add_activation_pool_payout(&mut self, payload: Hash64, amount: u64) -> Result<(), PalwStateV2Error> {
        if amount == 0 {
            return Ok(());
        }
        let key = crate::palw_activation_pool_v1::palw_activation_pool_payout_key_v1(&payload);
        let held = self.state.pending_payouts.get(&key).map(|row| row.amount).unwrap_or(0);
        let total = held.checked_add(amount).ok_or(PalwStateV2Error::Overflow("activation pool payout"))?;
        self.write_payout(key, Some(PalwPayoutV2 { payload, amount: total }))?;
        Ok(())
    }

    /// **F5: write one scheduled pool payout** (entry 78). `None` or zero removes it.
    pub(super) fn write_activation_pool_scheduled(&mut self, key: (Hash64, Hash64), new: Option<u64>) -> Result<(), PalwStateV2Error> {
        let new = new.filter(|amount| *amount > 0);
        self.reserve_map_row_v1(&key, &new, |s| &s.activation_pool_scheduled, false, "native activation_pool_scheduled writer")?;
        let old = match new {
            Some(amount) => self.state.activation_pool_scheduled.insert(key, amount),
            None => self.state.activation_pool_scheduled.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ActivationPoolScheduled { key, old, new });
        }
        Ok(())
    }

    /// **F5: owe `amount` to `payload` out of `class_id`'s pool** — the side map's entry grows; the
    /// caller moves the same sompi from a budget into the row's `scheduled_sompi` and writes the row.
    /// A class's scheduled amounts never sum past its `funded_sompi` (a `u64`), so the add is exact.
    pub(super) fn schedule_activation_payout(&mut self, class_id: Hash64, payload: Hash64, amount: u64) -> Result<(), PalwStateV2Error> {
        let held = self.state.activation_pool_scheduled.get(&(class_id, payload)).copied().unwrap_or(0);
        self.write_activation_pool_scheduled((class_id, payload), Some(held.saturating_add(amount)))?;
        Ok(())
    }

    /// **F4: record or drop a Candidate's readiness landing** (entry 79).
    pub(super) fn write_activation_readiness_landed(&mut self, key: (Hash64, PalwBondKeyV2), new: Option<u64>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.activation_readiness_landed, false, "native activation_readiness_landed writer")?;
        let old = match new {
            Some(span) => self.state.activation_readiness_landed.insert(key, span),
            None => self.state.activation_readiness_landed.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ActivationReadinessLanded { key, old, new });
        }
        Ok(())
    }

    /// **F4: drop every readiness landing of `class_id`** — when it leaves `Candidate` (seated) or is
    /// frozen, so the map holds only the classes (a) can still audit.
    pub(super) fn purge_activation_readiness_landed(&mut self, class_id: &Hash64) -> Result<(), PalwStateV2Error> {
        let keys: Vec<(Hash64, PalwBondKeyV2)> = self
            .state
            .activation_readiness_landed
            .range((*class_id, PALW_BOND_KEY_V2_MIN)..)
            .take_while(|((class, _), _)| class == class_id)
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            self.write_activation_readiness_landed(key, None)?;
        }
        Ok(())
    }

    /// **M3 (DA-2): write, update or retire one claim's DA record** — the rooted write, its delta
    /// entry (75), and the accuser index (a refuted entry held keeps its accuser indexed).
    pub(super) fn write_da_claim(&mut self, key: Hash64, new: Option<crate::palw_da_rcore_v1::PalwDaClaimV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.da_claims, false, "native da_claims writer")?;
        let refuted = new.as_ref().map_or(0, |record| record.refuted_held.len() as u64);
        self.reserve_da_reindex_v1(&key, false, refuted)?;
        let before = self.da_accusers_of_claim(&key);
        let old = match &new {
            Some(record) => self.state.da_claims.insert(key, record.clone()),
            None => self.state.da_claims.remove(&key),
        };
        if old == new {
            return Ok(());
        }
        self.reindex_da_accusers(&key, before);
        self.entries.push(PalwDeltaEntryV2::DaClaim { key, old, new });
        Ok(())
    }

    /// **The 2026-09-25 sweep's V03(2)/V05: write or drop a class's probation memory** — the rooted
    /// write and its delta entry (81). An empty memory is no row.
    /// **ADR-0160 F-Q: the one writer of `audit_receipts`**, journaled `AuditReceipts` (83).
    pub(super) fn write_audit_receipts_v1(&mut self, key: Hash64, new: Option<crate::palw_audit_door_v1::PalwAuditStatusV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.audit_receipts, false, "native audit_receipts writer")?;
        let old = match new.clone() {
            Some(status) => self.state.audit_receipts.insert(key, status),
            None => self.state.audit_receipts.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::AuditReceipts { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0160 F-Q (§5.9 (g)): the one writer of `excluded_auditors`**, journaled `AuditorExcluded` (84).
    pub(super) fn write_auditor_excluded_v1(&mut self, key: PalwBondKeyV2, new: Option<u64>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.excluded_auditors, false, "native excluded_auditors writer")?;
        let old = match new {
            Some(daa) => self.state.excluded_auditors.insert(key, daa),
            None => self.state.excluded_auditors.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::AuditorExcluded { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0160 F-N: the one writer of `network_demand`**, journaled `NetworkDemand` (86).
    pub(super) fn write_network_demand_v1(&mut self, key: (PalwBondKeyV2, Hash64), new: Option<u64>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.network_demand, false, "native network_demand writer")?;
        let old = match new {
            Some(until) => self.state.network_demand.insert(key, until),
            None => self.state.network_demand.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::NetworkDemand { key, old, new });
        }
        Ok(())
    }

    /// **RFC-0002 Phase F: the one writer of `tir_classes`**, journaled `TirClass` (88).
    pub(super) fn write_tir_class(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_tir_admission_v1::PalwTirClassRecordV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.tir_classes.contains_key(&key), "native tir_classes writer")?;
        let old = match new.clone() {
            Some(record) => self.state.tir_classes.insert(key, record),
            None => self.state.tir_classes.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::TirClass { key, old, new });
        }
        Ok(())
    }

    /// **RFC-0003: the one writer of `gen_classes`**, journaled `GenClass` (90).
    pub(super) fn write_gen_class(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_gen_class_v1::PalwGenClassRecordV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_journal_v1(&key, &new, !self.state.gen_classes.contains_key(&key), "native gen_classes writer")?;
        let old = match new.clone() {
            Some(record) => self.state.gen_classes.insert(key, record),
            None => self.state.gen_classes.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::GenClass { key, old, new });
        }
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_lines`**, journaled `ImprovementLine` (91). Keeps the
    /// due index and the heads index in step with the row.
    pub(crate) fn write_improvement_line(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_improve_state_v1::PalwImprovementLineV1>,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the row, its due/heads index nodes (an absent head bucket adds its outer node) and the journal's
        // two boxed copies, from the actual row before the clone.
        self.reserve_map_row_v1(&key, &new, |s| &s.improvement_lines, false, "native improvement_lines writer")?;
        if self.native_writers_tracking_v1() {
            let mut q = super::native_writers::NativeWriteQuoteV1::new("native improvement line indexes and journal boxes");
            if let Some(row) = &new {
                q.member::<(u64, Hash64), ()>(1)?.member::<Hash64, ()>(1)?;
                q.member_if::<Hash64, BTreeSet<Hash64>>(!self.state.improvement_heads_of.contains_key(&row.head))?;
            }
            let old_boxed = u64::from(self.state.improvement_lines.contains_key(&key));
            q.exact::<crate::palw_improve_state_v1::PalwImprovementLineV1>(old_boxed + u64::from(new.is_some()))?;
            self.admit_native_write_v1(q)?;
        }
        let old = match new.clone() {
            Some(row) => self.state.improvement_lines.insert(key, row),
            None => self.state.improvement_lines.remove(&key),
        };
        if old == new {
            return Ok(());
        }
        if let Some(old) = &old {
            self.state.improvement_due.remove(&(old.next_due_daa, key));
            if let Some(lines) = self.state.improvement_heads_of.get_mut(&old.head) {
                lines.remove(&key);
                if lines.is_empty() {
                    self.state.improvement_heads_of.remove(&old.head);
                }
            }
        }
        if let Some(new) = &new {
            self.state.improvement_due.insert((new.next_due_daa, key));
            self.state.improvement_heads_of.entry(new.head).or_default().insert(key);
        }
        self.entries.push(PalwDeltaEntryV2::ImprovementLine { key, old: old.map(Box::new), new: new.map(Box::new) });
        Ok(())
    }

    /// **RFC-0004: the one journaled writer of every keyed improvement table** (`ImprovementRow`, 92).
    pub(super) fn write_improvement_row<K, V>(
        &mut self,
        table: u8,
        key: K,
        new: Option<V>,
        map: fn(&mut PalwChainStateV2) -> &mut BTreeMap<K, V>,
    ) -> Result<Option<V>, PalwStateV2Error>
    where
        K: Ord + Clone + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeHeapV1,
        V: Clone + PartialEq + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeHeapV1,
    {
        self.reserve_encoded_row_v1(&key, &new, map, "native improvement row writer")?;
        let rows = map(&mut self.state);
        let old = match new.clone() {
            Some(row) => rows.insert(key.clone(), row),
            None => rows.remove(&key),
        };
        if old != new {
            let encode = |row: &V| borsh::to_vec(row).expect("an improvement row is borsh-serializable");
            self.entries.push(PalwDeltaEntryV2::ImprovementRow {
                table,
                key: borsh::to_vec(&key).expect("an improvement key is borsh-serializable"),
                old: old.as_ref().map(encode),
                new: new.as_ref().map(encode),
            });
        }
        Ok(old)
    }

    /// **RFC-0004: the one writer of `improvement_epochs`**. Keeps the open-epoch count and the
    /// retirement index in step with the header.
    pub(crate) fn write_improvement_epoch(
        &mut self,
        key: (Hash64, u64),
        new: Option<crate::palw_improve_state_v1::PalwImprovementEpochV1>,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the caller-side clone handed to the row writer, and the retirement index node, from the actual row.
        if self.native_writers_tracking_v1() {
            let mut q = super::native_writers::NativeWriteQuoteV1::new("native improvement epoch clone and retiring index");
            q.copy(&new)?;
            q.member::<(u64, Hash64, u64), ()>(u64::from(new.as_ref().and_then(palw_improvement_retiring_entry_v1).is_some()))?;
            self.admit_native_write_v1(q)?;
        }
        let old = self.write_improvement_row(PALW_IMPROVE_TABLE_EPOCHS_V1, key, new.clone(), |s| &mut s.improvement_epochs)?;
        let open = |e: &Option<crate::palw_improve_state_v1::PalwImprovementEpochV1>| e.as_ref().is_some_and(|e| !e.is_decided());
        match (open(&old), open(&new)) {
            (false, true) => self.state.improvement_open_epochs += 1,
            (true, false) => {
                // The index counts what the rows hold; a drift is a restart split (the rebuild would
                // disagree), so tests fail loudly on it rather than saturate over it.
                debug_assert!(self.state.improvement_open_epochs > 0, "the open-epoch index drifted below its rows");
                self.state.improvement_open_epochs = self.state.improvement_open_epochs.saturating_sub(1)
            }
            _ => {}
        }
        let live = |e: &Option<crate::palw_improve_state_v1::PalwImprovementEpochV1>| {
            e.as_ref().map_or(0, palw_improvement_live_results_of_v1)
        };
        debug_assert!(self.state.improvement_live_results >= live(&old), "the live-results index drifted below its rows");
        self.state.improvement_live_results =
            self.state.improvement_live_results.saturating_sub(live(&old)).saturating_add(live(&new));
        if let Some(entry) = old.as_ref().and_then(palw_improvement_retiring_entry_v1) {
            self.state.improvement_retiring.remove(&entry);
        }
        if let Some(entry) = new.as_ref().and_then(palw_improvement_retiring_entry_v1) {
            self.state.improvement_retiring.insert(entry);
        }
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_policies`**, journaled `ImprovementRow` (2).
    pub(crate) fn write_improvement_policy(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_improve_state_v1::PalwImprovementPolicyRecordV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_POLICIES_V1, key, new, |s| &mut s.improvement_policies)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_usage`**, journaled `ImprovementRow` (3).
    pub(crate) fn write_improvement_usage(&mut self, key: Hash64, new: Option<crate::palw_improve_state_v1::PalwImprovementUsageV1>) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_USAGE_V1, key, new, |s| &mut s.improvement_usage)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_heads`**, journaled `ImprovementRow` (4).
    pub(crate) fn write_improvement_head(
        &mut self,
        key: (Hash64, u32),
        new: Option<crate::palw_improve_state_v1::PalwLineageHeadEntryV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_HEADS_V1, key, new, |s| &mut s.improvement_heads)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_pools`**, journaled `ImprovementRow` (5).
    pub(crate) fn write_improvement_pool(&mut self, key: Hash64, new: Option<crate::palw_improve_state_v1::PalwImprovementPoolV1>) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_POOLS_V1, key, new, |s| &mut s.improvement_pools)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_material`**, journaled `ImprovementRow` (6).
    pub(crate) fn write_improvement_material(
        &mut self,
        key: (Hash64, u64),
        new: Option<crate::palw_improve_state_v1::PalwMaterialFrontierV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_MATERIAL_V1, key, new, |s| &mut s.improvement_material)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_candidates`**, journaled `ImprovementRow` (7).
    pub(crate) fn write_improvement_candidate(
        &mut self,
        key: (Hash64, u64, u32),
        new: Option<crate::palw_improve_state_v1::PalwEpochCandidateV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_CANDIDATES_V1, key, new, |s| &mut s.improvement_candidates)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_pool_entries`**, journaled `ImprovementRow` (8).
    pub(crate) fn write_improvement_pool_entry(
        &mut self,
        key: (Hash64, u64, u8, Hash64),
        new: Option<crate::palw_improve_state_v1::PalwPoolEntryV2>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_POOL_ENTRIES_V1, key, new, |s| &mut s.improvement_pool_entries)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_items`**, journaled `ImprovementRow` (9).
    pub(crate) fn write_improvement_item(
        &mut self,
        key: (Hash64, u64, u32),
        new: Option<crate::palw_improve_state_v1::PalwEvalItemV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_ITEMS_V1, key, new, |s| &mut s.improvement_items)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_results`**, journaled `ImprovementRow` (10).
    pub(crate) fn write_improvement_result(
        &mut self,
        key: (Hash64, u64, u32, crate::palw_improve_state_v1::PalwEvalSubjectV1),
        new: Option<crate::palw_improve_state_v1::PalwEvalResultV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_RESULTS_V1, key, new, |s| &mut s.improvement_results)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_earnings`**, journaled `ImprovementRow` (12).
    pub(crate) fn write_improvement_earning(&mut self, key: PalwBondKeyV2, new: Option<u64>) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_EARNINGS_V1, key, new, |s| &mut s.improvement_earnings)?;
        Ok(())
    }

    /// **RFC-0004: the one writer of `improvement_grants`**, journaled `ImprovementRow` (11).
    pub(crate) fn write_improvement_grant(
        &mut self,
        key: (Hash64, u64, u32),
        new: Option<crate::palw_improve_state_v1::PalwRewardGrantV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.write_improvement_row(PALW_IMPROVE_TABLE_GRANTS_V1, key, new, |s| &mut s.improvement_grants)?;
        Ok(())
    }

    /// **RFC-0002 F7: the one writer of `tir_dissections`**, journaled `TirDissection` (89).
    pub(super) fn write_tir_dissection(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0006: `Box::new` and the boxed clone for the map are two box allocations, each owning one deep copy
        // of the actual phase; quoted before the first box exists.
        if self.native_writers_tracking_v1() {
            let mut q = super::native_writers::NativeWriteQuoteV1::new("native tir_dissections writer");
            if let Some(phase) = &new {
                q.exact::<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>(2)?.copy(phase)?.copy(phase)?;
            }
            q.member_if::<Hash64, Box<crate::palw_tir_dissect_v1::PalwTirDissectPhaseV1>>(
                new.is_some() && !self.state.tir_dissections.contains_key(&key),
            )?
            .slots(1)?;
            self.admit_native_write_v1(q)?;
        }
        let new = new.map(Box::new);
        let old = match new.clone() {
            Some(phase) => self.state.tir_dissections.insert(key, phase),
            None => self.state.tir_dissections.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::TirDissection { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0160 F-N: the one writer of `operator_ring`**, journaled `OperatorRing` (87).
    pub(super) fn write_operator_ring_v1(&mut self, key: u64, new: Option<u32>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.operator_ring, false, "native operator_ring writer")?;
        let old = match new {
            Some(count) => self.state.operator_ring.insert(key, count),
            None => self.state.operator_ring.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::OperatorRing { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0164 F-EM: charge the accepting block's DAA** — the ledger's row for `daa` becomes `after_milli`, and every earlier day's
    /// row leaves (DAA never falls along a chain, so an earlier day is never read again).
    pub(super) fn charge_emission_v1(&mut self, daa: u64, after_milli: u64) -> Result<(), PalwStateV2Error> {
        use crate::palw_capacity_s567_v1::{PalwCapacityLedgerKeyV1 as K, PalwCapacityLedgerRowV1 as R};
        let earlier: Vec<K> = self.state.capacity_ledger.range(..K::Emission(daa)).map(|(key, _)| *key).collect();
        for key in earlier {
            self.write_capacity_ledger_v1(key, None)?;
        }
        self.write_capacity_ledger_v1(K::Emission(daa), Some(R::Emission { spent_milli: after_milli }))?;
        Ok(())
    }

    /// **ADR-0164 F-K: count one event in the breaker's row** (past the fence at `daa`; a no-op below it). The row starts fresh at the
    /// epoch of the first event; every counter saturates.
    pub(super) fn note_breaker_v1(&mut self, daa: u64, event: crate::palw_capacity_s567_v1::PalwBreakerEventV1) -> Result<(), PalwStateV2Error> {
        use crate::palw_capacity_s567_v1::{
            PALW_BREAKER_EPOCH_DAA_V1, PalwCapacityLedgerKeyV1 as K, PalwCapacityLedgerRowV1 as R, PalwRhoBreakerV1,
        };
        if !self.params.capacity_breaker_active_at(daa) {
            return Ok(());
        }
        let row = self.state.rho_breaker_v1().cloned().unwrap_or_else(|| PalwRhoBreakerV1::fresh_v1(daa / PALW_BREAKER_EPOCH_DAA_V1));
        self.write_capacity_ledger_v1(K::Breaker, Some(R::Breaker(row.noted_v1(event))))?;
        Ok(())
    }

    /// **ADR-0164 F-K (B1, B2): count one event in `bond`'s own row** (past the fence at `daa`).
    pub(super) fn note_bond_breaker_v1(&mut self, daa: u64, bond: PalwBondKeyV2, event: Option<bool>) -> Result<(), PalwStateV2Error> {
        use crate::palw_capacity_s567_v1::{
            PALW_BREAKER_EPOCH_DAA_V1, PalwBondBreakerV1, PalwCapacityLedgerKeyV1 as K, PalwCapacityLedgerRowV1 as R,
        };
        if !self.params.capacity_breaker_active_at(daa) {
            return Ok(());
        }
        let row =
            self.state.bond_breaker_v1(&bond).cloned().unwrap_or_else(|| PalwBondBreakerV1::fresh_v1(daa / PALW_BREAKER_EPOCH_DAA_V1));
        // `Some(void)`: a claim reached its receipt deadline; `None`: the bond was convicted.
        let next = match event {
            Some(void) => row.reached_v1(void),
            None => row.convicted_v1(),
        };
        self.write_capacity_ledger_v1(K::BondBreaker(bond), Some(R::Bond(next)))?;
        Ok(())
    }

    /// **ADR-0164: the one writer of `capacity_ledger`**, journaled `CapacityLedger` (the last variant).
    pub(super) fn write_capacity_ledger_v1(
        &mut self,
        key: crate::palw_capacity_s567_v1::PalwCapacityLedgerKeyV1,
        new: Option<crate::palw_capacity_s567_v1::PalwCapacityLedgerRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.capacity_ledger, false, "native capacity_ledger writer")?;
        let old = match &new {
            Some(row) => self.state.capacity_ledger.insert(key, row.clone()),
            None => self.state.capacity_ledger.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::CapacityLedger { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0160 F-S: the one writer of `issuance_buckets`**, journaled `IssuanceBucket` (85).
    pub(super) fn write_issuance_bucket_v1(
        &mut self,
        key: PalwBondKeyV2,
        new: Option<crate::palw_issuance_slots_v1::PalwIssuanceBucketV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.issuance_buckets, false, "native issuance_buckets writer")?;
        let old = match new {
            Some(bucket) => self.state.issuance_buckets.insert(key, bucket),
            None => self.state.issuance_buckets.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::IssuanceBucket { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0160 F-N: register the attempt's bond's unmet demand for its class** through `now + H_L`, when
    /// `refused` is a refusal for want of a unit — lane N's `NetworkRoomExhausted`, or the class room's
    /// `FloorRoomExhausted` / `BondClassShareExceeded` (which past F-N divides the room by lane N's rule) —
    /// of its own or a merged attempt. Past `Params::palw_capacity_network_room` only; any other refusal
    /// (the bond's own ceiling, slots, freeze, …) registers nothing.
    pub(super) fn register_network_demand_on_v1(
        &mut self,
        refused: &PalwStateV2Error,
        attempt: &crate::palw_attempt_v2::PalwAttemptUnsignedV2,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if palw_network_refusal_wants_a_unit_v1(refused) && self.params.capacity_network_active_at(now_daa) {
            let key = (PalwBondKeyV2(attempt.executor_bond), attempt.class_id);
            self.write_network_demand_v1(key, Some(now_daa.saturating_add(crate::palw_network_room_v1::PALW_NETWORK_H_L_DAA_V1)))?;
        }
        Ok(())
    }

    /// **ADR-0160 F-N: count an operator's attempt block in the ring** (the block's own work, accepted or
    /// skipped): an attempt whose executor is one of the operator bonds under that bond's registered key.
    /// Entries older than the ring's 32 DAA leave with it. Past `Params::palw_capacity_network_room` only.
    pub(super) fn note_operator_attempt_v1(&mut self, now_daa: u64, attempt: &crate::palw_attempt_v2::PalwAttemptUnsignedV2) -> Result<(), PalwStateV2Error> {
        if !self.params.capacity_network_active_at(now_daa) {
            return Ok(());
        }
        // Lane A's operator bonds (the mirror F-Q's pool reads too); the attempt's signature is the
        // bond's registered key's, checked by admission before the fold.
        let operator = self.params.capacity_audit_operators().contains(&PalwBondKeyV2(attempt.executor_bond));
        let stale: Vec<u64> = self
            .state
            .operator_ring
            .range(..now_daa.saturating_sub(crate::palw_network_room_v1::PALW_NETWORK_RING_DAA_V1 - 1))
            .map(|(daa, _)| *daa)
            .collect();
        for daa in stale {
            self.write_operator_ring_v1(daa, None)?;
        }
        if operator {
            let count = self.state.operator_ring.get(&now_daa).copied().unwrap_or(0).saturating_add(1);
            self.write_operator_ring_v1(now_daa, Some(count))?;
        }
        Ok(())
    }

    pub(super) fn check_network_room_v1(
        &self,
        bond: &PalwBondKeyV2,
        claim: &PalwClaimStateV2,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        self.read().check_network_room_v1(bond, claim, now_daa)
    }

    /// **ADR-0160 lane liab (AG-3): the one writer of `bond_freezes`**, journaled `BondFreeze` (82).
    pub(super) fn write_bond_freeze_v1(
        &mut self,
        key: PalwBondKeyV2,
        new: Option<crate::palw_aggregate_liability_v1::PalwBondFreezeV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.bond_freezes, false, "native bond_freezes writer")?;
        let old = match new {
            Some(freeze) => self.state.bond_freezes.insert(key, freeze),
            None => self.state.bond_freezes.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::BondFreeze { key, old, new });
        }
        Ok(())
    }

    pub(super) fn write_probation_memory_v1(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_model_registry_v1::PalwProbationMemoryV1>,
    ) -> Result<(), PalwStateV2Error> {
        let new = new.filter(|memory| !memory.is_empty());
        self.reserve_map_row_v1(&key, &new, |s| &s.probation_memory, false, "native probation_memory writer")?;
        let old = match &new {
            Some(memory) => self.state.probation_memory.insert(key, memory.clone()),
            None => self.state.probation_memory.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ProbationMemory { key, old, new });
        }
        Ok(())
    }

    /// **4-ter.3 step 6: write or drop one held forfeit** — the rooted write and its delta entry (80).
    pub(super) fn write_held_forfeit(&mut self, key: (Hash64, Hash64), new: Option<PalwHeldForfeitV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.held_forfeits, false, "native held_forfeits writer")?;
        let old = match &new {
            Some(record) => self.state.held_forfeits.insert(key, record.clone()),
            None => self.state.held_forfeits.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::HeldForfeit { key, old, new });
        }
        Ok(())
    }

    /// **4-ter.3 step 6 (the forger's race): the acquitting close's record** — past
    /// `palw_offence_attribution`, a PROVEN `ChallengerDefeated` on a dissection whose bottom read an
    /// anchor, on a claim that can still be tried, keeps what the close took from the challenger
    /// (`before − after`, the debits themselves) and the anchor chunks the bottom opened. One per
    /// session: the session is gone after its close, so it can never be written twice.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_held_forfeit_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        session_id: Hash64,
        challenger: PalwBondKeyV2,
        narrowed_leaf: Option<u64>,
        before: u64,
        proof: &crate::palw_court_v2::PalwCourtVerdictProofV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_attn_court_v1::PalwAttnTileEvidenceV1;
        if !self.extras.offence_attribution_active {
            return Ok(());
        }
        let crate::palw_court_v2::PalwCourtVerdictProofV2::AttnDissection { bottom, .. } = proof else { return Ok(()) };
        let Some(anchor) = bottom.anchor.as_ref() else { return Ok(()) };
        if self.state.claims.get(&claim_id).is_none_or(|claim| claim.phase.is_terminal()) {
            return Ok(());
        }
        let after = self.state.bonds.get(&challenger).map(|record| record.collateral).unwrap_or(before);
        let amount = before.saturating_sub(after);
        let mut chunks: Vec<u32> = [&bottom.k, &bottom.v]
            .into_iter()
            .filter_map(|tile| match tile {
                PalwAttnTileEvidenceV1::Checkpoint { chunk, .. } => Some(chunk.chunk_index),
                PalwAttnTileEvidenceV1::CacheWrites { .. } => None,
            })
            .collect();
        chunks.sort_unstable();
        chunks.dedup();
        let Some(narrowed_leaf) = narrowed_leaf else { return Ok(()) };
        if amount == 0 || chunks.is_empty() {
            return Ok(());
        }
        self.write_held_forfeit(
            (claim_id, session_id),
            Some(PalwHeldForfeitV1 {
                challenger,
                amount,
                anchor_leaf_hash: anchor.opening.leaf_hash,
                anchor_checkpoint: anchor.leaf.checkpoint_index,
                narrowed_leaf,
                chunks,
                closed_daa: ctx.daa_score,
                paused: false,
            }),
        )?;
        // The challenger's demand may already be open (its node files it at the `Terminal` duty, and
        // the forger's close can land after it): it becomes the record's one pause now.
        self.pause_open_demand_of_held_forfeit_v1(ctx.daa_score, (claim_id, session_id))?;
        Ok(())
    }

    /// **4-ter.3 step 6 (the third review's MEDIUM): a demand that PREDATES its record pauses too.** A
    /// non-seat challenger's node files its step-6 demand at its `Terminal` duty, while the held
    /// session is still open; a forger that closes its acquittal after that demand lands gets a record
    /// whose challenger's session was admitted non-pausing ([`Self::held_forfeit_pauses_v1`] runs only
    /// at a session's opening), and one session per accuser left no second demand to open —
    /// `Final` (close + `window_challenge`) came long before the demand's deadline (+ `W_disclose`).
    /// So the record's write CONVERTS that open session into the record's pause, exactly as an opening
    /// under the record would have written it: counted with the seats' OPEN sessions (never in
    /// `opened_by_seat`, a real seat's budget), the claim paused from now when it is the first
    /// (`paused_since`, its deadline disarmed —
    /// the close's re-arm just armed it; the pause is credited back at the session's close as every
    /// seat pause is), and the record marked paused. A seat's session already pauses: nothing to do.
    pub(super) fn pause_open_demand_of_held_forfeit_v1(&mut self, now_daa: u64, key: (Hash64, Hash64)) -> Result<(), PalwStateV2Error> {
        let (claim_id, _) = key;
        let Some(record) = self.state.held_forfeits.get(&key).cloned() else { return Ok(()) };
        if record.paused || self.state.claims.get(&claim_id).is_none_or(|claim| claim.phase.is_terminal()) {
            return Ok(());
        }
        let Some(session) = self.state.da_sessions.get(&(claim_id, record.challenger)).cloned() else { return Ok(()) };
        if session.accuser_is_seat {
            return Ok(());
        }
        self.write_da_session(
            (claim_id, record.challenger),
            Some(crate::palw_da_rcore_v1::PalwDaSessionV1 { accuser_is_seat: true, ..session }),
        )?;
        let mut da = self.state.da_claims.get(&claim_id).cloned().unwrap_or_default();
        // Moved from the non-seat OPEN count to the seats' (DA-5, DL-1). DA-8's lifetime counts stay
        // as its admission left them: `opened_non_seat_total` counted it then, and `opened_by_seat` —
        // a real seat's budget, at most four — never counts a non-seat (the fourth review's HIGH).
        da.open_other_sessions = da.open_other_sessions.saturating_sub(1);
        da.open_seat_sessions = da.open_seat_sessions.saturating_add(1);
        let first = da.open_seat_sessions == 1;
        if first {
            da.paused_since = Some(now_daa);
        }
        self.write_da_claim(claim_id, Some(da))?;
        if first {
            self.disarm_deadline(claim_id);
        }
        self.write_held_forfeit(key, Some(PalwHeldForfeitV1 { paused: true, ..record }))?;
        Ok(())
    }

    /// **4-ter.3 step 6 (the forger's race): the held forfeits a checkpoint-court conviction proves
    /// wrong** — every record of `claim_id` whose acquitting bottom opened the very anchor
    /// (`anchor_leaf_hash`) and chunk the conviction's forged row is in. Read BEFORE the conviction,
    /// whose void drops the claim's records.
    pub(super) fn held_forfeits_proven_wrong_v1(
        &self,
        claim_id: &Hash64,
        accusation: &crate::palw_checkpoint_court_v1::PalwCheckpointAccusationV1,
    ) -> Vec<PalwHeldForfeitV1> {
        self.state
            .held_forfeits_of_claim(claim_id)
            .filter(|(_, record)| {
                record.anchor_leaf_hash == accusation.anchor.opening.leaf_hash && record.chunks.contains(&accusation.chunk.chunk_index)
            })
            .map(|(_, record)| record.clone())
            .collect()
    }

    /// **4-ter.3 step 6 (the forger's race), the withholding door: the held forfeits a DA-7 default
    /// proves wrong** (the second review's HIGH) — every record of `claim_id` whose challenger is the
    /// accuser of one of the claim's DEFAULTED sessions (past its deadline at `now_daa`) whose named
    /// unit is the step-6 demand of that record's anchor ([`PalwHeldForfeitV1::is_demanded_by_v1`]).
    /// A forger that acquitted itself on its forged anchor and then withholds the demanded chunk
    /// (S1, cheaper than S2 at 8k) would otherwise keep the seat's forfeit burned. Read BEFORE the
    /// default's void drops the records.
    pub(super) fn held_forfeits_withheld_v1(&self, claim_id: &Hash64, now_daa: u64) -> Vec<PalwHeldForfeitV1> {
        let defaulted: Vec<(PalwBondKeyV2, crate::palw_da_rcore_v1::PalwDaUnitV1)> = self
            .state
            .da_sessions_of(claim_id)
            .filter(|(_, session)| session.deadline_daa < now_daa)
            .filter_map(|(accuser, session)| session.units.first().map(|named| (*accuser, *named)))
            .collect();
        self.state
            .held_forfeits_of_claim(claim_id)
            .filter(|(_, record)| {
                defaulted.iter().any(|(accuser, named)| *accuser == record.challenger && record.is_demanded_by_v1(named))
            })
            .map(|(_, record)| record.clone())
            .collect()
    }

    /// **4-ter.3 step 6 (the second review's MEDIUM): does `accuser`'s new data-availability session
    /// on `claim_id` pause the claim as a seat's does (DL-1)?** Past `palw_offence_attribution`, when
    /// `accuser` is the challenger of a live held-forfeit record on the claim that has not paused it
    /// yet — marked here, so a record pauses once. A non-seat's session pauses nothing (DA-5), so the
    /// claim reached `Final` at `L + W` while the challenger's step-6 demand, opened the block after
    /// the acquittal moved `L`, ran to `L + 1 + W`: the forger answered after `Final`, where no
    /// checkpoint accusation reaches it. Bounded: one pause per record, one record per session, and
    /// each pause at most `W_disclose` (1,200 on testnet-12), credited back at the close as every
    /// seat pause is. The session is still ADMITTED on the standing its accuser has (DA-8): a
    /// non-seat's demand needs a free non-seat slot on the claim (three open, sixteen over its life),
    /// and those slots can be filled by Sybil accusers ahead of it — an existing DA-8 property this
    /// does not change, recorded as a residual.
    pub(super) fn held_forfeit_pauses_v1(&mut self, claim_id: Hash64, accuser: PalwBondKeyV2) -> Result<bool, PalwStateV2Error> {
        if !self.extras.offence_attribution_active {
            return Ok(false);
        }
        let Some((session, record)) = self
            .state
            .held_forfeits_of_claim(&claim_id)
            .find(|(_, record)| record.challenger == accuser && !record.paused)
            .map(|(session, record)| (*session, record.clone()))
        else {
            return Ok(false);
        };
        self.write_held_forfeit((claim_id, session), Some(PalwHeldForfeitV1 { paused: true, ..record }))?;
        Ok(true)
    }

    /// **4-ter.3 step 6 (the forger's race): make each wrongly charged challenger whole** — after the
    /// conviction has closed (its `CourtConviction` record written last with the producer's
    /// collected debit, which this never touches). The forfeit never left the challenger's bond: a
    /// slash lowers `collateral` and raises `slashed`, the sompi its release must destroy. So the
    /// refund is restored WHERE IT WENT — `slashed` down, `collateral` up by the same amount, clamped
    /// to `slashed` — and it is the challenger's own money, neither minted nor taken from the
    /// producer's debit: the producer's charge, the burn of it and the reporter share are exactly
    /// the conviction's. A bond whose collateral already left through its release spend (its burn
    /// already paid) gains only a record — collateral on a registry row whose outpoint is spent,
    /// which nothing can spend again, so it is harmless: the money is gone, and nothing is minted in
    /// its place. Called after a checkpoint conviction, after a DA-7 default, and (past ADR-0160 F-L)
    /// by an aggregate forfeiture of the claim's producer ([`Self::forfeit_bond_v1`]).
    ///
    /// **ADR-0160 AG-2 (lane liab review 2, finding 1): a challenger forfeited whole keeps its
    /// forfeiture final.** Past F-L an intent-class conviction takes a bond's whole posted collateral
    /// and writes a FINAL freeze, and "a final freeze posts no collateral" is a load invariant
    /// (`assert_bond_freezes_consistency_v1`, run by every carriage load: restart, pruning-point
    /// import). Raising such a bond's collateral here wrote a tip no node could reload — the next sink
    /// search panicked on every F-L node, on every restart. Had the wrong forfeit never been taken,
    /// the forfeiture would have taken it with the rest; so the refund joins the forfeiture: `slashed`
    /// and `collateral` stay, and the freeze's `forfeited_sompi` records it (entry 82). A tier freeze
    /// leaves collateral, so its bond is refunded as any other. Below F-L no freeze exists.
    pub(super) fn refund_held_forfeits_v1(&mut self, forfeits: &[PalwHeldForfeitV1]) -> Result<(), PalwStateV2Error> {
        for record in forfeits {
            let Some(mut bond) = self.state.bonds.get(&record.challenger).cloned() else { continue };
            let refund = record.amount.min(bond.slashed);
            if refund == 0 {
                continue;
            }
            if let Some(freeze) = self.state.bond_freezes.get(&record.challenger).copied().filter(|freeze| freeze.final_) {
                self.write_bond_freeze_v1(
                    record.challenger,
                    Some(crate::palw_aggregate_liability_v1::PalwBondFreezeV1 {
                        forfeited_sompi: freeze.forfeited_sompi.saturating_add(refund),
                        ..freeze
                    }),
                )?;
                continue;
            }
            bond.slashed -= refund;
            bond.collateral = bond.collateral.saturating_add(refund);
            self.write_bond(record.challenger, Some(bond))?;
        }
        Ok(())
    }

    /// The accusers `claim_id` contributes to the A-6 index: its open sessions' and its refuted
    /// entries' — the per-claim half of [`palw_da_indexes_of_v1`].
    pub(super) fn da_accusers_of_claim(&self, claim_id: &Hash64) -> BTreeSet<PalwBondKeyV2> {
        let mut accusers: BTreeSet<PalwBondKeyV2> = self.state.da_sessions_of(claim_id).map(|(accuser, _)| *accuser).collect();
        if let Some(record) = self.state.da_claims.get(claim_id) {
            accusers.extend(record.refuted_held.iter().map(|(accuser, _)| *accuser));
        }
        accusers
    }

    pub(super) fn reindex_da_accusers(&mut self, claim_id: &Hash64, before: BTreeSet<PalwBondKeyV2>) {
        let after = self.da_accusers_of_claim(claim_id);
        for gone in before.difference(&after) {
            self.state.da_by_accuser.remove(&(*gone, *claim_id));
        }
        for added in after.difference(&before) {
            self.state.da_by_accuser.insert((*added, *claim_id));
        }
    }

    /// **ADR-0152 DA-2/DA-5/DA-6 (M3): close one session** — `Refuted` moves its exposure into the
    /// claim's `refuted_held` (DA-6: held until the claim resolves), `Released` returns it (a confirmed
    /// default, a conviction, a void). The record's counts and `last_closed_daa` follow; when the last
    /// SEAT session closes, the pause ends: a live claim's anchors are credited through
    /// [`crate::palw_da_rcore_v1::palw_da_resume_claim_v1`] (unless `credit_pause` is off — the claim is
    /// about to be written terminal by the caller) and its deadline re-armed from DL-1. A terminal
    /// claim's retirement is re-armed once its last session is gone.
    pub(super) fn da_close_session_v1(
        &mut self,
        claim_id: Hash64,
        accuser: PalwBondKeyV2,
        refuted: bool,
        credit_pause: bool,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        let session = self
            .state
            .da_sessions
            .get(&(claim_id, accuser))
            .cloned()
            .ok_or_else(|| PalwStateV2Error::CarriageInconsistent(format!("no DA session of {accuser:?} on claim {claim_id}")))?;
        self.write_da_session((claim_id, accuser), None)?;
        if let Some(mut dispute) = self.state.legacy_disputes.get(&claim_id).cloned()
            && dispute.reacquiring.remove(&accuser).is_some()
        {
            self.write_legacy_dispute(claim_id, Some(dispute))?;
        }
        let mut record = self.state.da_claims.get(&claim_id).cloned().unwrap_or_default();
        if session.accuser_is_seat {
            record.open_seat_sessions = record.open_seat_sessions.saturating_sub(1);
        } else {
            record.open_other_sessions = record.open_other_sessions.saturating_sub(1);
        }
        record.last_closed_daa = Some(now_daa);
        if refuted {
            record.refuted_held.push((accuser, session.exposure));
        }
        let unpaused = session.accuser_is_seat && record.open_seat_sessions == 0;
        let paused_since = if unpaused { record.paused_since.take() } else { None };
        let open_left = record.open_sessions();
        self.write_da_claim(claim_id, Some(record))?;
        let Some(claim) = self.state.claims.get(&claim_id).cloned() else { return Ok(()) };
        if claim.phase.is_terminal() {
            if open_left == 0 {
                self.rearm_claim_deadline_dl1_v1(claim_id, now_daa)?;
            }
            return Ok(());
        }
        if let Some(since) = paused_since {
            // X3 (REQ-X3-3): no DA pause credit for a claim accepted past `palw_claim_hard_deadline_v1` — its deadlines are fixed at
            // acceptance; the credit stays the OLD rule for the claims below it.
            if credit_pause && !crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(self.params, &claim) {
                let resumed = crate::palw_da_rcore_v1::palw_da_resume_claim_v1(&claim, since, now_daa);
                if resumed != claim {
                    self.write_claim(claim_id, Some(resumed))?;
                }
                // §4-quater V6: a long-D claim's pause moves `H` too — before the re-arm reads it.
                self.credit_verify_pause_v6(claim_id, since, now_daa)?;
            }
            self.rearm_claim_deadline_dl1_v1(claim_id, now_daa)?;
        } else if open_left == 0
            && ((matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })
                && palw_rcore_licence_awaits_replay_v1(&claim)
                && self.state.panel_v3.as_ref().is_some_and(|engine| engine.claim_rows().contains_key(&claim_id)))
                || self.state.palw_v2_expiry_hold_applies_v1(self.params, &claim_id, &claim)
                // X3 (REQ-X3-5): any session held this claim's Final (DL-1's X3 row); the last close re-derives it.
                || (matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })
                    && crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(self.params, &claim)))
        {
            // …and the last session on a lane-A claim the part-C hold reaches (SHARD): the same re-derivation.
            // RFC-0010 × G14 (SHARD): the last session on a V3 S2 licence closed — DL-1 re-derives its deadline (none while a court
            // is still open). Dormant: no engine exists on any shipped chain.
            self.rearm_claim_deadline_dl1_v1(claim_id, now_daa)?;
        }
        Ok(())
    }

    /// **ADR-0152 §4-quater V6: a seat-DA pause is returned to a long-D claim's verification
    /// horizon.** DA-5 credits the pause to the phase's anchor — `bound_daa` of a `PanelBound` claim,
    /// `licensed_daa` of a `ReceiptLicensed` one — but `H(c) = B* + D + 1` counts from the rooted
    /// panel bound, which DA-5 never moves: a seat that could only get the producer's material through
    /// the DA court would lose the pause from its `D`, and a licensed claim could `Final` before that
    /// seat finished replaying (K31, red-team F5). So when the claim's last seat session closes, past
    /// the fence and for a claim whose `D(c)` is long (past 120), `panels[c].bound_daa` is advanced by
    /// the SAME credit ([`crate::palw_da_rcore_v1::palw_da_pause_credit_v1`] from `paused_since` to
    /// `now`). An existing rooted field, written through `write_panel` with its delta entry: no new
    /// state. For a `PanelBound` claim the rooted bound stays equal to the phase's shifted bound.
    ///
    /// Long-D only: for `D ≤ 120` DA-5's `licensed_daa` shift already leaves at least the whole
    /// challenge window after the disclosure, and a short claim's panel bound — which the receipt
    /// pool and the supplementary door read — stays what every testnet-12 genesis class has today. `D`
    /// is the claim's own (a free-prompt claim's run included), judged at the bound it is credited
    /// from; the credit only moves the bound forward, so the fence and any measured row that priced
    /// the claim still price it.
    ///
    /// **Receipts signed before the shift no longer verify** (`signed_daa < bound_daa`): every seat
    /// re-signs for the new bound, which is a new duty key on the node. The node must therefore keep a
    /// seat's replay result per (claim, job) — N-2's resumable replay — so a re-sign never costs a
    /// second replay (the review's INFO; a flag-day item: no launch class is long-D).
    pub(super) fn credit_verify_pause_v6(&mut self, claim_id: Hash64, paused_since: u64, now_daa: u64) -> Result<(), PalwStateV2Error> {
        let Some(panel) = self.state.panels.get(&claim_id).cloned() else { return Ok(()) };
        if !self.params.class_verify_deadline_active_at(panel.bound_daa) {
            return Ok(());
        }
        let Some(claim) = self.state.claims.get(&claim_id) else { return Ok(()) };
        // X3 (REQ-X3-3): no horizon credit for a claim accepted past `palw_claim_hard_deadline_v1` (the OLD rule below it).
        if crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(self.params, claim) {
            return Ok(());
        }
        // A `Provisional` claim (redrawn) still names its old panel until the next bind replaces it;
        // there is no horizon to credit until then.
        if !matches!(claim.phase, PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::ReceiptLicensed { .. }) {
            return Ok(());
        }
        let shape = crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::of_claim(claim);
        if self.params.claim_verify_daa_v1(&self.state, &claim.class_id, shape, panel.bound_daa)
            <= crate::palw_class_verify_deadline_v1::PALW_CLASS_VERIFY_LONG_D_DAA_V1
        {
            return Ok(());
        }
        let credited = crate::palw_da_rcore_v1::palw_da_pause_credit_v1(panel.bound_daa, paused_since, now_daa);
        if credited != panel.bound_daa {
            self.write_panel(claim_id, Some(PalwPanelStateV2 { bound_daa: credited, ..panel }))?;
        }
        Ok(())
    }

    /// **DL-1 re-armed from the record**: the claim's one deadline, disarmed and re-derived by
    /// [`palw_rcore_deadline_v1`] at `now_daa` (the block being folded).
    pub(super) fn rearm_claim_deadline_dl1_v1(&mut self, claim_id: Hash64, now_daa: u64) -> Result<(), PalwStateV2Error> {
        self.disarm_deadline(claim_id);
        let Some(claim) = self.state.claims.get(&claim_id).cloned() else { return Ok(()) };
        if let Some(at) = palw_rcore_deadline_v1(&self.state, self.params, &claim_id, &claim, Some(now_daa))? {
            self.arm_deadline(at, claim_id)?;
        }
        Ok(())
    }

    /// **Every `Valid` lock on `claim_id`, by its signers** — looked up per seat of the claim's panel
    /// (and, should the panel be gone, per signer its liability row lists), never by a scan of the
    /// whole lock map (M3 review, LOW). A lock is written only by a seat of the claim's current panel
    /// (the licence doors), so the two sets agree; the result is in key order.
    pub(super) fn claim_locks_v1(
        &self,
        claim_id: &Hash64,
    ) -> Vec<((PalwBondKeyV2, Hash64), crate::palw_panel_var_v1::PalwSlashableLockV1)> {
        let mut signers: Vec<PalwBondKeyV2> =
            self.state.panels.get(claim_id).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
        if let Some(row) = self.state.panel_liabilities.get(claim_id) {
            signers.extend(row.valid_signers.iter().map(|(seat, _)| PalwBondKeyV2(*seat)));
        }
        signers.sort();
        signers.dedup();
        signers
            .into_iter()
            .filter_map(|seat| self.state.slashable_locks.get(&(seat, *claim_id)).map(|lock| ((seat, *claim_id), *lock)))
            .collect()
    }

    /// **ADR-0152 Q-5's first-panel redraw (M4; V3S-01)**: an S2 licence that no supplementary set
    /// raised to `basis_k ≥ 2` by DL-1's gate goes back to `Provisional` for one fresh panel, exactly
    /// as a first `ReceiptTimeout` does from `PanelBound` (`sweep_deadlines`):
    ///
    /// * **Locks and credit go** — every `Valid` signer's lock on the claim (the S2 full seat's, a
    ///   rider's, any a supplementary set added short of 2) and the duty row with its credit, its
    ///   exposure back to each seat (`release_seat_duties`). The claim never reached `Final`, so none
    ///   of it was earned; and past `palw_rcore_plus` a lock on a live claim stays committed whatever
    ///   its clocks say (`palw_bond_committed_v1`), so a lock kept here would hold the seat's room for
    ///   a panel it no longer sits on.
    /// * **The record resets with the one assignment** the nested `rcore` exists for: a
    ///   `Provisional` claim carries the `Default` record (the load invariant refuses anything else),
    ///   and `rebound_daa` records that the one redraw is spent, and anchors the second panel on this
    ///   sweep so its draw deals different seats.
    /// * **The commitment does not move** — `w + esc + rr` while licensed with the escrow held (an
    ///   S2 licence never releases it: SR-1 cond. 1) and `w + esc + rr` while `Provisional` — so SR-4's
    ///   monotone release is not engaged. It goes through `move_commitment`, the one funnel for a
    ///   write that keeps a claim live, so a record that did disagree would fail on every node alike.
    /// * **Open DA sessions stay open** (DA-5): a redraw is not an answer. (A seat session pauses the
    ///   claim's deadline, so this runs only with none open; another accuser's session stays.)
    /// * **The deadline is DL-1's** for the redrawn record: the bind window from `rebound_daa`.
    pub(super) fn redraw_unreplayed_licence_v1(
        &mut self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        debug_assert!(
            palw_rcore_licence_awaits_replay_v1(claim) && claim.rebound_daa.is_none(),
            "only a first-panel S2 licence redraws"
        );
        for (key, _) in self.claim_locks_v1(&claim_id) {
            // G14 G-13: a frame-credited lock stays for the ORIGINAL frame's own conviction while its door is open (no frame → as before).
            if crate::palw_tir_public_claim_v1::palw_tir_public_frame_keeps_lock_v1(&self.state, &key.0, claim_id, now_daa) {
                continue;
            }
            self.write_slashable_lock(key, None)?;
        }
        self.cancel_panel_recipients_v1(claim_id)?;
        self.release_seat_duties(&claim_id)?;
        let mut revived = claim.clone();
        revived.phase = PalwClaimPhaseV2::Provisional;
        revived.rebound_daa = Some(now_daa);
        revived.rcore = PalwClaimRcoreV1::default();
        self.move_commitment(claim, &revived, now_daa)?;
        self.write_claim(claim_id, Some(revived))?;
        self.rearm_claim_deadline_dl1_v1(claim_id, now_daa)
    }

    /// **DA-5 / L-3 (V3S-02, V3S-04): the row and the locks follow a session.** The claim's vesting row
    /// (if it has one) is re-keyed to `max(expiry, until)` and every live lock of its `Valid` signers
    /// re-dated to at least `until` (`until` = the session's deadline + `window_challenge_at`), so a
    /// conviction landing while the session is open, or within the challenge window of its end,
    /// finds the row unmatured and every lock live. Only ever raises. (M3 applies the lock half
    /// before `Final` too: a seat-paused licensed claim can outlast the licence's lock window, and a
    /// default at the `Licensed` stage charges S4 on live locks.)
    pub(super) fn da_rekey_v1(&mut self, claim_id: Hash64, until: u64, now_daa: u64) -> Result<(), PalwStateV2Error> {
        palw_exec_v2_rewards_v1::require_native_session_window(&self.state, self.params, &claim_id, until)?;
        // FP has no vesting row. Past the public-filer fence, a pursuit opened inside its liability window holds that row too,
        // so the next authenticated demand cannot lose its post-Final stage halfway through the reserved pursuit.
        if self.params.legacy_public_filer_active_at(now_daa)
            && self.state.claims.get(&claim_id).is_some_and(|claim| matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }))
            && let Some(row) = self.state.panel_liabilities.get(&claim_id).cloned()
            && row.voided_daa.is_none()
            && row.void_reason.is_none()
            && now_daa < row.expiry_daa
            && row.expiry_daa < until
        {
            self.write_panel_liability(
                claim_id,
                Some(crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1 { expiry_daa: until, ..row }),
            )?;
        }
        if let Some(row) = self.state.vesting.get(&claim_id).cloned()
            && row.expiry_daa < until
        {
            // Through the one writer of `vesting` (the M3 review's F4): the re-key moves the row in
            // every derived index (remove-old-then-insert-new) and journals the `Vesting` delta.
            self.write_vesting(claim_id, Some(crate::palw_vesting_v1::PalwVestingRowV1 { expiry_daa: until, ..row }))?;
        }
        let settled_now = self.state.settled_attempt_finals;
        let depth = self.second_clock_depth(now_daa);
        let window_court = self.params.window_court;
        let stale: Vec<((PalwBondKeyV2, Hash64), crate::palw_panel_var_v1::PalwSlashableLockV1)> = self
            .claim_locks_v1(&claim_id)
            .into_iter()
            .filter(|(_, lock)| lock.expiry_daa < until && lock.is_live_v3(now_daa, settled_now, depth, window_court))
            .collect();
        for (key, lock) in stale {
            self.write_slashable_lock(key, Some(crate::palw_panel_var_v1::PalwSlashableLockV1 { expiry_daa: until, ..lock }))?;
        }
        Ok(())
    }

    /// **Every open session on `claim_id` closes and returns its exposure** (a void, a conviction, a
    /// confirmed default: DA-6's "a confirmed or released session returns its exposure"); with
    /// `convicted`, every `refuted_held` entry returns to its accuser as well (DA-6, N13).
    pub(super) fn da_release_all_v1(&mut self, claim_id: Hash64, convicted: bool, now_daa: u64) -> Result<(), PalwStateV2Error> {
        self.da_release_sessions_v1(claim_id, convicted, now_daa)?;
        // Lane LG14-A: every dispute reservation on the claim ends with the claim's outcome (no-op below the fence).
        self.legacy_dispute_release_all_v1(claim_id, convicted, now_daa)
    }

    /// [`Self::da_release_all_v1`]'s DA half, unchanged.
    pub(super) fn da_release_sessions_v1(&mut self, claim_id: Hash64, convicted: bool, now_daa: u64) -> Result<(), PalwStateV2Error> {
        if !self.state.da_claims.contains_key(&claim_id) {
            return Ok(());
        }
        let accusers: Vec<PalwBondKeyV2> = self.state.da_sessions_of(&claim_id).map(|(accuser, _)| *accuser).collect();
        for accuser in accusers {
            self.da_close_session_v1(claim_id, accuser, false, false, now_daa)?;
        }
        if convicted
            && let Some(record) = self.state.da_claims.get(&claim_id)
            && !record.refuted_held.is_empty()
        {
            let refunded = crate::palw_da_rcore_v1::PalwDaClaimV1 { refuted_held: Vec::new(), ..record.clone() };
            self.write_da_claim(claim_id, Some(refunded))?;
        }
        Ok(())
    }

    /// **ADR-0152 DA-7 (M3): a session defaulted — the claim's withholding is confirmed, once.**
    ///
    /// * **The stage is the claim's at the default**: `Live` (unlicensed), `Licensed`, or `FinalRow`
    ///   (a `Final` claim — the post-`Final` window, DA-8; a session opened before `Final` and still
    ///   open at it is charged at the stage it defaults in).
    /// * **The covering signers (C7)** are the `Valid` signers holding a live lock whose recorded mask
    ///   covers at least one of the DEFAULTING session's unanswered units
    ///   ([`crate::palw_da_rcore_v1::palw_da_unit_covered_by_v1`]); none before a licence.
    /// * **The reward's winner** (R-1, V3S-12) is the accuser of the earliest defaulted session: the
    ///   smallest `(opened_daa, accuser)` among the claim's sessions past their deadline.
    ///
    /// Everything is charged through ONE hook, [`Self::da_default_charge_v1`], which closes every open
    /// session on the claim (exposure returned) and refunds its refuted exposure (a DA default is a
    /// conviction, DA-6).
    pub(super) fn da_default_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        accuser: PalwBondKeyV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_da_rcore_v1::{PalwDaStageV1, palw_da_in_run_rows_v1};
        let now = ctx.daa_score;
        let session = self
            .state
            .da_sessions
            .get(&(claim_id, accuser))
            .cloned()
            .ok_or_else(|| PalwStateV2Error::CarriageInconsistent(format!("no DA session of {accuser:?} on claim {claim_id}")))?;
        let claim = self.state.claims.get(&claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
        let record = self.state.da_claims.get(&claim_id).cloned().unwrap_or_default();
        let in_run = palw_da_in_run_rows_v1(&claim, self.extras.fp_da_pins_active);
        let unanswered: Vec<crate::palw_da_rcore_v1::PalwDaUnitV1> = session
            .units
            .iter()
            .filter(|unit| !self.state.palw_da_unit_available_v1(&claim_id, &record, unit, in_run))
            .copied()
            .collect();
        if unanswered.is_empty() {
            // Unreachable: a session never opens on an answered named unit (`open_da_session_rcore_v1`,
            // M3 review F3), and the answer that completes a session's units refutes and closes it in
            // its own block. A fold every node runs on peer input closes one anyway the way that
            // answer would have, rather than charging anybody.
            return self.da_close_session_v1(claim_id, accuser, true, true, now);
        }
        let stage = match claim.phase {
            PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } => PalwDaStageV1::Live,
            PalwClaimPhaseV2::ReceiptLicensed { .. } => PalwDaStageV1::Licensed,
            PalwClaimPhaseV2::Final { .. } => PalwDaStageV1::FinalRow,
            // Unreachable: a void closes every session (`void_claim`), and past the fence no claim is
            // `DefaultDisputed`. Released, as the void would have.
            PalwClaimPhaseV2::Voided { .. } | PalwClaimPhaseV2::DefaultDisputed { .. } => {
                return self.da_release_all_v1(claim_id, false, now);
            }
        };
        let winner = self
            .state
            .da_sessions_of(&claim_id)
            .filter(|(_, s)| s.deadline_daa < now)
            .map(|(bond, s)| (s.opened_daa, *bond))
            .min()
            .map(|(_, bond)| bond)
            .unwrap_or(accuser);
        // C7: the covering signers, by the ONE predicate Phase 2's responder reads to decide what a
        // signer owes an answer to (`PalwFoldReadV1::da_lock_covers_v1`; P2-7).
        let covering: Vec<PalwBondKeyV2> = if matches!(stage, PalwDaStageV1::Live)
            || !palw_da_signer_liability_armed_v1(self.params, self.extras.seat_da_answer_landed, now)
        {
            Vec::new()
        } else {
            // The bounded candidate set (the panel's seats and the liability row's signers; the S
            // re-review), each read through the ONE covering predicate the node's responder reads.
            let read = self.read();
            self.claim_locks_v1(&claim_id)
                .into_iter()
                .filter(|(_, lock)| read.da_lock_covers_v1(lock, &unanswered, now))
                .map(|((seat, _), _)| seat)
                .collect()
        };
        let g = self.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);
        // ADR-0152 §4-ter.3 step 6 (the second review's HIGH): the acquittals this withholding proves
        // wrong, read before the default's void drops them (a `FinalRow` default finds none: `Final`
        // dropped them, and a pausing challenger's session keeps its claim from `Final`).
        let withheld = self.held_forfeits_withheld_v1(&claim_id, now);
        let charge = PalwDaDefaultChargeV1 { claim_id, producer: claim.bond, stage, g, covering, winner };
        // RFC-0007 §I.7: the attesters of the claim's data, read before the charge's void drops their rows.
        let attesters = palw_vertex_fold_v1::held_attesters_to_charge_v1(&self.state, &claim_id);
        self.da_default_charge_v1(ctx, &claim, &charge)?;
        palw_vertex_fold_v1::charge_held_attesters_v1(self, &claim_id, &attesters)?;
        // ...and, the conviction closed, each such challenger made whole.
        self.refund_held_forfeits_v1(&withheld)?;
        Ok(())
    }

    /// **ADR-0152 DA-7 (M3): THE hook every data-availability default is charged through** — S-4's
    /// conviction funnel, opened on the producer and every covering signer before any lock is taken
    /// (R-2), in this order:
    ///
    /// 1. **S4 on each covering signer** (X7, N9, C7), under the audit's (seat, claim) key
    ///    ([`crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2`]) so a
    ///    `ProducerWithholding` kind 3 filed later is a no-op: its lock taken FIRST (so the void's
    ///    liability row does not list it), then `min(25% · C₀, 3 G)`. Each its own record (`amount`
    ///    nominal, `collected` the seat's own locked debit, `claim_id`); burned, never rewarded
    ///    (R-1, V3S-12). Dormant while `palw_da_signer_liability_armed_v1` is (no covering signer).
    /// 2. **The producer, by stage**: `Live` / `Licensed` → `void_and_slash(ProducerWithholding)`:
    ///    S1 — the commitment (`E` included, from uncommitted stake if the escrow was released, X7)
    ///    and a strike (X11), the third inside 7,500 DAA adding `min(10% · C₀, 3 G)`; the claim voids.
    ///    `FinalRow` → the `Final` reversed while the record exists (#8) and the liability marked —
    ///    both as `ProducerWithholding`, the default it is, never `CourtFraud` (M3 review F1) — then
    ///    S3 through the burn hook (`burn_vesting_row` → `Some` ⇒ `min(25% · C₀, 3 G)`; `None` ⇒
    ///    nothing). No strike after `Final`: S3 is its tier.
    /// 3. **The `DaDefault` record** (kind 5, N6) under
    ///    [`crate::palw_da_rcore_v1::palw_da_offence_id_v1`]`(producer, claim)`, once per claim, root 0,
    ///    `amount` the producer's nominal tier, `collected` the producer's locked debit only; the claim's
    ///    unminted round rights forfeited BY CLAIM (V-2b); the reporter reward opened for the earliest
    ///    defaulted session's accuser on that debit ([`PalwConvictionBasisV1::DaDefault`]; no
    ///    `ReporterCommitted` on the DA key, V3S-03).
    /// 4. **Every open session closes, exposure returned, and `refuted_held` is refunded** (DA-6: a
    ///    default is a conviction).
    ///
    /// 5. **The reporter reward** (R-4, S-7's `open_reporter_reward`): `reward_pending` for
    ///    `charge.winner` on the producer's collected debit only; no `ReporterCommitted` on the DA key
    ///    (V3S-03).
    pub(super) fn da_default_charge_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim: &PalwClaimStateV2,
        charge: &PalwDaDefaultChargeV1,
    ) -> Result<(), PalwStateV2Error> {
        self.da_default_charge_with_producer_leg_v1(ctx, claim, charge, true)
    }

    /// Only the prospective rooted history fold may invoke this with the accepted
    /// pledge and exact expired demand. Existing claims do not gain history custody.
    pub(super) fn staged_history_default_charge_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim: &PalwClaimStateV2,
        contract: &crate::palw_history_custody_v1::PalwHistoryCustodyContractV1,
        default: &crate::palw_history_custody_v1::PalwHistoryRecoveryDefaultV1,
        winner: PalwBondKeyV2,
    ) -> Result<(), PalwStateV2Error> {
        let request = &default.request;
        if contract.pledge.check_v1().is_err()
            || request.check_v1().is_err()
            || contract.pledge.claim != request.claim
            || request.owner != winner
            || contract.producer != claim.bond
            || contract.pledge.execution_root != claim.execution_root
            || contract.custody.accepted_daa != claim.accepted_daa
            || request.sequence > contract.maximum_records
            || request.material_until_daa > contract.pledge.material_until_daa
            || default.opened_daa < request.reserved_daa
            || default.opened_daa >= default.answer_until_daa
            || default.collected_daa != ctx.daa_score
            || ctx.daa_score < default.answer_until_daa
            || ctx.daa_score >= request.material_until_daa
            || !default.custodians.contains(&contract.producer)
            || default.custodians.contains(&winner)
            || default.custodians.iter().any(|bond| {
                *bond != contract.producer
                    && !contract.custody.credited.get(bond).is_some_and(|receipt| receipt.credited_daa <= default.opened_daa)
            })
        {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "history default differs from the accepted pledge and literal expired custody grant".into(),
            ));
        }
        // The literal expired set includes the producer (checked above): its leg runs.
        self.staged_selected_default_charge_v1(ctx, request.claim, claim, contract.producer, &default.custodians, winner)
    }

    /// The rooted prospective source fold supplies the exact expired demand and
    /// its literal unpaid set: every mandatory custodian, the producer included,
    /// that did not reach EOF. Panel membership is not that proof. The producer's
    /// claim-level leg runs only when the producer itself is in that set; silent
    /// signers beside an answering producer bear only their own covering charge.
    /// History invokes this economic leg only after its separate pledge checks.
    pub(super) fn staged_selected_default_charge_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        producer: PalwBondKeyV2,
        unpaid: &std::collections::BTreeSet<PalwBondKeyV2>,
        winner: PalwBondKeyV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_da_rcore_v1::{PalwDaStageV1, palw_da_offence_id_v1};
        if producer != claim.bond || ctx.daa_score < claim.accepted_daa || winner == producer || unpaid.contains(&winner) {
            return Err(PalwStateV2Error::CarriageInconsistent(
                "retained source default changes producer, independent winner or literal signer roles".into(),
            ));
        }
        let covering: std::collections::BTreeSet<PalwBondKeyV2> = unpaid.iter().copied().filter(|bond| *bond != producer).collect();
        if !unpaid.contains(&producer) {
            // The producer answered to EOF: no void, no producer record, no session release.
            let g = self.claim_g_v1(&claim_id).map(|g| g.g()).unwrap_or(0);
            let covering: Vec<PalwBondKeyV2> = covering.into_iter().collect();
            let conv = self.open_conviction_v1(&covering, ctx.daa_score);
            return self.charge_covering_signers_v1(&conv, claim_id, &covering, g);
        }
        let offence = palw_da_offence_id_v1(&producer.0, &claim_id);
        let producer_already_charged = self.state.consumed_offences.contains_key(&offence);
        // A sibling default may already have voided the claim and paid the reporter.
        // Retained family custody must not rerun that economic leg or reset its phase.
        let (stage, live_producer_leg) = match claim.phase {
            PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } => (PalwDaStageV1::Live, true),
            PalwClaimPhaseV2::ReceiptLicensed { .. } => (PalwDaStageV1::Licensed, true),
            PalwClaimPhaseV2::Final { .. } => (PalwDaStageV1::FinalRow, true),
            _ => (PalwDaStageV1::Live, false),
        };
        let charge = PalwDaDefaultChargeV1 {
            claim_id,
            producer,
            stage,
            g: self.claim_g_v1(&claim_id).map(|g| g.g()).unwrap_or(0),
            covering: covering.iter().copied().collect(),
            winner,
        };
        self.da_default_charge_with_producer_leg_v1(ctx, claim, &charge, live_producer_leg && !producer_already_charged)
    }

    fn da_default_charge_with_producer_leg_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim: &PalwClaimStateV2,
        charge: &PalwDaDefaultChargeV1,
        run_producer_leg: bool,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_da_rcore_v1::{PalwDaStageV1, palw_da_offence_id_v1};
        use crate::palw_offence_v1::PalwOffenceKindV1;
        let now = ctx.daa_score;
        let claim_id = charge.claim_id;
        let offence_id = palw_da_offence_id_v1(&charge.producer.0, &claim_id);
        let mut opened = vec![charge.producer];
        opened.extend(charge.covering.iter().copied());
        let conv = self.open_conviction_v1(&opened, now);
        let fp_first = self.post_final_fp_g_v1(&claim_id);
        // 1. S4 on each covering signer, lock first.
        self.charge_covering_signers_v1(&conv, claim_id, &charge.covering, charge.g)?;
        // 2. The producer, by stage.
        let nominal = if !run_producer_leg {
            0
        } else {
            match charge.stage {
                PalwDaStageV1::Live | PalwDaStageV1::Licensed => {
                    self.void_and_slash_at(claim_id, claim, now, PalwVoidReasonV2::ProducerWithholding, Some(&conv))?
                }
                PalwDaStageV1::FinalRow => {
                    // **The reversal records what was confirmed — the WITHHOLDING, never a proof** (M3
                    // review F1): a `CourtFraud` void here would let kind 3's `CourtFraud { voided_daa }`
                    // convict every honest full-mask signer for the producer's silence, whatever
                    // `PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1` says. `ProducerWithholding` leaves N9's
                    // `da_confirmed` gate as the ONLY route from a DA default to a signer.
                    self.reverse_convicted_final(ctx, claim_id, PalwVoidReasonV2::ProducerWithholding)?;
                    self.mark_liability_convicted(claim_id, now, PalwVoidReasonV2::ProducerWithholding)?;
                    self.da_release_all_v1(claim_id, true, now)?;
                    self.post_final_producer_leg_v1(
                        &conv,
                        claim_id,
                        charge.producer,
                        offence_id,
                        PalwOffenceKindV1::DaDefault,
                        charge.g,
                        fp_first,
                    )?
                }
            }
        };
        // 3. The record, once per claim; forfeiture by claim; the reward to the earliest defaulted accuser.
        if !self.state.consumed_offences.contains_key(&offence_id) {
            self.close_conviction_v1(
                &conv,
                offence_id,
                PalwOffenceKindV1::DaDefault,
                charge.producer,
                nominal,
                Hash64::default(),
                claim_id,
                &[charge.producer],
                &[(charge.producer, crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1::DaDefault)],
            )?;
            self.open_reporter_reward(now, offence_id, 0, PalwConvictionBasisV1::DaDefault { accuser: charge.winner })?;
        }
        self.forfeit_minted_round_rights_of_claim(&claim_id)?;
        // 4. Every session closed and every refuted entry refunded (the void did it for a live stage).
        self.da_release_all_v1(claim_id, true, now)?;
        Ok(())
    }

    /// S4 on each literal covering signer, lock first; an already consumed offence or a
    /// signer without a slashable lock on this claim is skipped (never charged twice).
    pub(super) fn charge_covering_signers_v1(
        &mut self,
        conv: &PalwConvictionV1,
        claim_id: Hash64,
        covering: &[PalwBondKeyV2],
        g: u128,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_offence_v1::PalwOffenceKindV1;
        for seat in covering {
            let key = crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &claim_id);
            if self.state.consumed_offences.contains_key(&key) {
                continue;
            }
            let Some(lock) = self.state.slashable_locks.get(&(*seat, claim_id)).copied() else { continue };
            self.write_slashable_lock((*seat, claim_id), None)?;
            let s4 = lock.amount.saturating_add(palw_rcore_s3s4_action_v1(conv.c0(seat), g));
            self.slash_bond(*seat, s4)?;
            self.close_conviction_v1(
                conv,
                key,
                PalwOffenceKindV1::PanelFalseValidV2,
                *seat,
                s4,
                Hash64::default(),
                claim_id,
                &[*seat],
                &[(*seat, crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1::CoveringSigner)],
            )?;
        }
        Ok(())
    }

    /// **DA-6: a retiring claim's DA record goes with it, and the refuted exposure no conviction
    /// refunded is burned** through `slash_seat` (capped at `min_collateral_sompi`, as every seat-side
    /// charge), earning no reward. Retirement never runs with a session open (DL-1 defers it).
    pub(super) fn da_retire_v1(&mut self, claim_id: Hash64) -> Result<(), PalwStateV2Error> {
        let Some(record) = self.state.da_claims.get(&claim_id).cloned() else { return Ok(()) };
        if record.open_sessions() > 0 {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "claim {claim_id} retired with a data-availability session open (DL-1 defers retirement)"
            )));
        }
        for (accuser, amount) in &record.refuted_held {
            self.slash_seat(*accuser, *amount, self.params.min_collateral_sompi())?;
        }
        self.write_da_claim(claim_id, None)?;
        Ok(())
    }

    /// **ADR-0152 §3.6 / R-2 (S-4): a conviction's OPENING** — for every bond the conviction may
    /// charge, its posted collateral `C₀` before this conviction's first debit and whether its
    /// withdrawal gate was SHUT on the pre-state ([`palw_bond_collateral_is_locked_v6`] at the
    /// conviction's DAA, the processor's raw second-clock depth and the mirrored withdrawal delay, the
    /// DA lattice included). Taken before any leg runs — in particular before a kind 3 or a DA-7 S4
    /// deletes the signer's lock, which v6 reads (digest §11 #9), and before any debit, so `C₀` is
    /// never a post-debit collateral (#10). A bond named twice is opened once.
    pub(super) fn open_conviction_v1(&self, bonds: &[PalwBondKeyV2], now_daa: u64) -> PalwConvictionV1 {
        let raw_depth = self.read().settled_anchor_depth();
        // RFC-0015 §8.3.3 (2026-10-15): the same duty gate as the processor's bond lock — also on every network with an OPV policy.
        let duty_gate =
            self.extras.audit_2026_09_23_active || self.extras.kernel_route.as_ref().is_some_and(|route| route.opv.is_some());
        let mut opened: Vec<(PalwBondKeyV2, u64, bool)> = Vec::with_capacity(bonds.len());
        for bond in bonds {
            if opened.iter().any(|(seen, _, _)| seen == bond) {
                continue;
            }
            let (c0, locked) = match self.state.bonds.get(bond) {
                Some(record) => (
                    record.collateral,
                    palw_bond_collateral_is_locked_v6(
                        &self.state,
                        self.params,
                        bond,
                        record,
                        now_daa,
                        self.params.withdrawal_delay_daa(),
                        raw_depth,
                        duty_gate,
                    ),
                ),
                None => (0, false),
            };
            opened.push((*bond, c0, locked));
        }
        PalwConvictionV1 { now_daa, bonds: opened }
    }

    /// **R-2: what a conviction collected from `counted`** — Σ over the bonds it opened with their
    /// exit SHUT of `C₀ − C_now`: exactly the sum of this conviction's `slash_bond` debits of those
    /// bonds (no fold path credits a collateral inside a conviction), and 0 for a bond whose gate was
    /// open, whose collateral may already have left through the UTXO layer where no burn reaches it.
    pub(super) fn conviction_collected_v1(&self, conv: &PalwConvictionV1, counted: &[PalwBondKeyV2]) -> u64 {
        conv.bonds
            .iter()
            .filter(|(bond, _, locked)| *locked && counted.contains(bond))
            .map(|(bond, c0, _)| c0.saturating_sub(self.state.bonds.get(bond).map(|record| record.collateral).unwrap_or(0)))
            .fold(0u64, u64::saturating_add)
    }

    /// **R-2: a conviction's CLOSING** — the consumed record, written LAST, after every leg:
    /// `amount` the nominal tier summed over the conviction's legs (for audit: what the rules asked
    /// for), `collected` the debit actually taken from `counted` ([`Self::conviction_collected_v1`]),
    /// `claim_id` the claim it binds (zero for Eq). `collected ≤ amount` always. Returns `collected`.
    ///
    /// **ADR-0160 lane liab (F-L): every conviction's aggregate liability runs here, after the
    /// conviction's own legs and before its record** ([`Self::aggregate_on_conviction_v1`] over
    /// `liable`, each bond the conviction charges with the class of what it was convicted of). The
    /// record's `collected` is read BEFORE the funnel: it is the TIER debit the conviction's legs
    /// took, exactly as below the fence, so the reporter reward stays "on the collected debit, as
    /// today" (§4.6) — the reporter share of the tier (10%, or ADR-0032's 49% past the dormant
    /// `palw_reporter_share_v2`), never of a forfeited bond (lane liab review, finding 2: counting the
    /// forfeiture turned forcing one DA default of a genesis producer into a ≈ 93,586 MSK bounty at
    /// 10%, against ≈ 320 today). What the forfeiture took is the freeze record's
    /// `forfeited_sompi`. Below the fence the funnel is a no-op and the order changes nothing.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn close_conviction_v1(
        &mut self,
        conv: &PalwConvictionV1,
        key: Hash64,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
        accused: PalwBondKeyV2,
        nominal: u128,
        execution_root: Hash64,
        claim_id: Hash64,
        counted: &[PalwBondKeyV2],
        liable: &[(PalwBondKeyV2, crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1)],
    ) -> Result<u64, PalwStateV2Error> {
        let collected = self.conviction_collected_v1(conv, counted);
        // **ADR-0164 F-K (K2, K1′): what this conviction asked and what it collected**, whether its bond had been forfeited whole already
        // (a forfeited bond has nothing left to collect, so its tier is not a shortfall) and whether the claim it binds held audit
        // receipts (a false audit). Read BEFORE the aggregate funnel moves the freeze.
        if self.params.capacity_breaker_active_at(conv.now_daa) {
            self.note_breaker_v1(
                conv.now_daa,
                crate::palw_capacity_s567_v1::PalwBreakerEventV1::Conviction {
                    nominal: u64::try_from(nominal.min(u128::from(u64::MAX))).unwrap_or(u64::MAX),
                    collected,
                    forfeited_before: self.state.bond_freezes.get(&accused).is_some_and(|freeze| freeze.final_),
                    audited: self.state.audit_status_of_v1(&claim_id).is_some(),
                    accused,
                },
            )?;
        }
        self.note_bond_breaker_v1(conv.now_daa, accused, None)?;
        self.aggregate_on_conviction_v1(conv.now_daa, key, kind, liable)?;
        // **ADR-0160 F-Q (§5.9 (g)): a conviction of a claim that holds audit receipts excludes every
        // auditor that receipted it** from every pool, for good (only a flag day removes one).
        if let Some(status) = self.state.audit_status_of_v1(&claim_id).cloned() {
            for (auditor, _) in status.receipts {
                if !self.state.auditor_excluded_v1(&auditor) {
                    self.write_auditor_excluded_v1(auditor, Some(conv.now_daa))?;
                }
            }
        }
        self.write_consumed_offence(
            key,
            Some(crate::palw_offence_v1::PalwConsumedOffenceV1 {
                kind,
                accused: accused.0,
                amount: u64::try_from(nominal.min(u128::from(u64::MAX))).unwrap_or(u64::MAX),
                accepted_daa: conv.now_daa,
                execution_root,
                collected,
                claim_id,
            }),
        )?;
        Ok(collected)
    }

    /// **ADR-0160 lane liab: the aggregate liability of one conviction** (AG-2, AG-3, AS-3) — the one
    /// funnel every conviction's closing ([`Self::close_conviction_v1`]) runs before its record, past
    /// `Params::palw_capacity_aggregate_liability` resolved at the conviction's DAA (the mirror). Below
    /// it: nothing, and every rule is the one before the fence.
    ///
    /// For each bond the conviction charges (`liable`; a bond named twice takes the stronger class):
    ///
    /// * **intent class** ([`crate::palw_aggregate_liability_v1::palw_offence_is_intent_class_v1`]) —
    ///   AG-2's forfeiture ([`Self::forfeit_bond_v1`]: every live claim voided `AggregateForfeit`, the
    ///   posted collateral whole, the bond's own legs of every vesting row that has not moved) and a
    ///   FINAL freeze. A bond already final-frozen has nothing left to take and keeps its freeze;
    /// * **otherwise** — the tier its caller already charged, and a freeze from this DAA (written, or
    ///   re-dated by a later conviction; a final one stays final), with the bond's own unmatured
    ///   vesting rows re-keyed to the freeze's lift ([`Self::rekey_frozen_rows_v1`]).
    ///
    /// It calls `void_claim` and `slash_bond` and edits neither; its vesting burns take only the
    /// bond's own legs ([`Self::burn_vesting_producer_leg_v1`], [`Self::burn_vesting_seat_legs_v1`]),
    /// never a whole row (`burn_vesting_row` stays the burn of a claim a conviction binds).
    pub(super) fn aggregate_on_conviction_v1(
        &mut self,
        now_daa: u64,
        key: Hash64,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
        liable: &[(PalwBondKeyV2, crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1)],
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_aggregate_liability_v1::{PalwBondFreezeV1, palw_offence_is_intent_class_v1};
        if !self.params.capacity_liability_active_at(now_daa) {
            return Ok(());
        }
        let mut seen: Vec<PalwBondKeyV2> = Vec::with_capacity(liable.len());
        for (bond, _) in liable {
            if seen.contains(bond) || !self.state.bonds.contains_key(bond) {
                continue;
            }
            seen.push(*bond);
            let intent = liable.iter().any(|(other, offence)| other == bond && palw_offence_is_intent_class_v1(*offence));
            let prior = self.state.bond_freezes.get(bond).copied();
            if prior.is_some_and(|freeze| freeze.final_) {
                continue;
            }
            let prior_forfeited = prior.map(|freeze| freeze.forfeited_sompi).unwrap_or(0);
            if intent {
                let taken = self.forfeit_bond_v1(*bond, now_daa, key, kind)?;
                self.write_bond_freeze_v1(
                    *bond,
                    Some(PalwBondFreezeV1 {
                        since_daa: now_daa,
                        offence_key: key,
                        forfeited_sompi: prior_forfeited.saturating_add(taken),
                        final_: true,
                    }),
                )?;
            } else {
                let freeze =
                    PalwBondFreezeV1 { since_daa: now_daa, offence_key: key, forfeited_sompi: prior_forfeited, final_: false };
                self.write_bond_freeze_v1(*bond, Some(freeze))?;
                if let Some(lift) = freeze.lifts_at_v1(self.params.window_court) {
                    palw_exec_v2_rewards_v1::freeze_bond(self, *bond, now_daa, lift)?;
                    self.rekey_frozen_rows_v1(*bond, lift)?;
                }
            }
        }
        Ok(())
    }

    /// **ADR-0160 AG-2: a bond's aggregate forfeiture** — in this order, returning what it took:
    ///
    /// 1. every LIVE claim of the bond (`Provisional`, `PanelBound`, `ReceiptLicensed`; M3 retired
    ///    `DefaultDisputed` past R-core+) voided `AggregateForfeit` through `void_claim`: its withheld
    ///    reward is never minted, its sessions end (a filer's refuted exposure refunded, DA-6), its
    ///    seats leave duty, its commitment is released — into the collateral step 2 takes whole — and
    ///    every challenger holding a held forfeit on it (§4-ter.3 step 6) is made whole
    ///    ([`Self::refund_held_forfeits_v1`], the records read before the void drops them; lane liab
    ///    review 2, finding 2). The void closes both refund doors for good — the challenger's step-6
    ///    demand (DA-7) and the checkpoint accusation need a live claim — and the forger chooses when
    ///    it lands (any intent-class conviction of another of its claims: a default it arranges, a
    ///    self-reported kind 4), so without this a forger expecting to lose its bond on the forged
    ///    claim anyway would take the forfeiture through another claim first and keep the honest
    ///    challenger's forfeit burned. The forfeited producer is treated as having withheld every
    ///    demand it can no longer answer, as its void is charged as a conviction (DA-6 above). A
    ///    challenger the refund reaches had lost a dissection on the claim of a bond now convicted of
    ///    intent; at worst a challenger that was wrong recovers its own forfeit, never anyone's money;
    ///    a challenger itself forfeited keeps the refund in its forfeiture (finding 1);
    /// 2. the posted collateral, whole (`slash_bond`, which saturates: never debt, AG-1);
    /// 3. the bond's unmatured REWARD — every vesting row it is payee of that has not moved (latched
    ///    rows included), and in each only the bond's OWN legs: the producer leg of every row it
    ///    PRODUCED ([`Self::burn_vesting_producer_leg_v1`]) and its seat legs in other producers' rows
    ///    ([`Self::burn_vesting_seat_legs_v1`]). Every other payee of those rows keeps its leg, and
    ///    the reserve stays: the credited seats of a row the bond produced validated a claim no one
    ///    proved false (this conviction binds ANOTHER claim), and they are other bonds — ADR-0160 §4.5
    ///    "burns every unmatured reward of the bond", AG-4 (lane liab review, finding 1: burning the
    ///    whole row with `burn_vesting_row` took the honest seats' 20% pool share, 640.17 MSK per
    ///    floor row). The row stands, so a later conviction binding that claim still finds it (S3).
    ///
    /// Additional EXEC slice/fee funds are included by their payee index, independently of Panel seats;
    /// only the forfeited actor's unpaid funds burn, with their original funding role and offence key.
    /// So A-I1: what it takes is at most the posted collateral plus the bond's own unmatured legs.
    pub(super) fn forfeit_bond_v1(
        &mut self,
        bond: PalwBondKeyV2,
        now_daa: u64,
        key: Hash64,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
    ) -> Result<u64, PalwStateV2Error> {
        let live: Vec<(Hash64, PalwClaimStateV2)> = self
            .state
            .claims
            .iter()
            .filter(|(_, claim)| {
                claim.bond == bond
                    && matches!(
                        claim.phase,
                        PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::ReceiptLicensed { .. }
                    )
            })
            .map(|(id, claim)| (*id, claim.clone()))
            .collect();
        for (id, claim) in live {
            let held: Vec<PalwHeldForfeitV1> = self.state.held_forfeits_of_claim(&id).map(|(_, record)| record.clone()).collect();
            self.void_claim(id, &claim, now_daa, PalwVoidReasonV2::AggregateForfeit)?;
            self.refund_held_forfeits_v1(&held)?;
        }
        let collateral = self.state.bonds.get(&bond).map(|record| record.collateral).unwrap_or(0);
        let mut taken = self.slash_bond(bond, u128::from(collateral))?;
        let produced: Vec<Hash64> =
            self.state.vesting.iter().filter(|(_, row)| row.producer_bond == bond).map(|(id, _)| *id).collect();
        for id in produced {
            taken = taken.saturating_add(self.burn_vesting_producer_leg_v1(id, bond, key, kind)?);
        }
        let sat: Vec<Hash64> =
            self.state.vesting.iter().filter(|(_, row)| row.seats.iter().any(|(seat, _)| *seat == bond)).map(|(id, _)| *id).collect();
        for id in sat {
            taken = taken.saturating_add(self.burn_vesting_seat_legs_v1(id, bond, key)?);
        }
        taken = taken.saturating_add(palw_exec_v2_rewards_v1::forfeit_bond(self, bond, key)?);
        Ok(taken)
    }

    /// **ADR-0160 AG-2: burn the forfeited producer's own leg of a row it produced**, leaving the
    /// row standing with `producer.amount = 0` — its credited seats' legs, its reserve, its clocks
    /// and its attribution copies as they were, so it matures and moves the others' legs on its own
    /// schedule (a zero leg takes no budget and writes nothing, [`PalwVestingLegV1::takes_budget`]).
    /// The leg's sompi go to `vesting_burned` and are never minted; the note is
    /// `ProducerLegBurned`. Returns the sompi burned (0, writing nothing, when the row is gone, is
    /// not `bond`'s, or its producer leg is already 0).
    ///
    /// [`PalwVestingLegV1::takes_budget`]: crate::palw_vesting_v1::PalwVestingLegV1::takes_budget
    pub(super) fn burn_vesting_producer_leg_v1(
        &mut self,
        claim_id: Hash64,
        bond: PalwBondKeyV2,
        offence_id: Hash64,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
    ) -> Result<u64, PalwStateV2Error> {
        use crate::palw_vesting_v1::{PalwVestingNoteV1, PalwVestingRowV1};
        let Some(row) = self.state.vesting.get(&claim_id).cloned() else { return Ok(0) };
        if row.producer_bond != bond || row.producer.amount == 0 {
            return Ok(0);
        }
        let sompi = row.producer.amount;
        let mut counters = self.state.vesting_counters;
        counters.burned = counters.burned.checked_add(u128::from(sompi)).ok_or(PalwStateV2Error::Overflow("vesting burned"))?;
        let producer = PalwPayoutV2 { amount: 0, ..row.producer };
        self.write_vesting(claim_id, Some(PalwVestingRowV1 { producer, ..row }))?;
        self.write_vesting_counters(counters)?;
        self.note_vesting(PalwVestingNoteV1::ProducerLegBurned { claim_id, producer: bond, offence_id, kind, sompi })?;
        Ok(sompi)
    }

    /// **ADR-0160 AG-2 (AS-3): burn `bond`'s seat legs in another producer's vesting row**, leaving
    /// the row's producer, reserve and other seats as they were. The legs leave the row (so the bond
    /// is no longer its payee); their sompi go to `vesting_burned` and are never minted; the note is
    /// the one-seat share's (`ShareBurned`, S4′).
    pub(super) fn burn_vesting_seat_legs_v1(
        &mut self,
        claim_id: Hash64,
        bond: PalwBondKeyV2,
        offence_id: Hash64,
    ) -> Result<u64, PalwStateV2Error> {
        use crate::palw_vesting_v1::{PalwVestingNoteV1, PalwVestingRowV1};
        let Some(row) = self.state.vesting.get(&claim_id).cloned() else { return Ok(0) };
        if !row.seats.iter().any(|(seat, _)| *seat == bond) {
            return Ok(0);
        }
        let sompi = row.seats.iter().filter(|(seat, _)| *seat == bond).map(|(_, leg)| leg.amount).fold(0u64, u64::saturating_add);
        let kept: Vec<(PalwBondKeyV2, PalwPayoutV2)> = row.seats.iter().filter(|(seat, _)| *seat != bond).cloned().collect();
        let mut counters = self.state.vesting_counters;
        counters.burned = counters.burned.checked_add(u128::from(sompi)).ok_or(PalwStateV2Error::Overflow("vesting burned"))?;
        self.write_vesting(claim_id, Some(PalwVestingRowV1 { seats: kept, ..row }))?;
        self.write_vesting_counters(counters)?;
        self.note_vesting(PalwVestingNoteV1::ShareBurned { claim_id, seat: bond, offence_id, sompi })?;
        Ok(sompi)
    }

    /// **ADR-0160 AG-3: no vesting row the frozen bond PRODUCED matures under a tier freeze** — each
    /// unlatched row whose DAA clock runs out before the lift is re-keyed to it through the one
    /// writer of `vesting` (the DA-5 re-key's: the row moves in every derived index), so it waits in
    /// the queue's own order instead of holding the queue's head unlatched. A later conviction
    /// re-keys again; a row written while frozen (a `Final` at or past `since`) already expires past
    /// the lift. Its seat legs in other producers' rows are not held (holding them would hold the
    /// honest producer's row): a tier freeze keeps the producer's value, which is what a correlated
    /// fraud found inside the window burns.
    pub(super) fn rekey_frozen_rows_v1(&mut self, bond: PalwBondKeyV2, lift_daa: u64) -> Result<(), PalwStateV2Error> {
        let rows: Vec<Hash64> = self
            .state
            .vesting
            .iter()
            .filter(|(_, row)| row.producer_bond == bond && row.matured_at.is_none() && row.expiry_daa < lift_daa)
            .map(|(id, _)| *id)
            .collect();
        for id in rows {
            if let Some(row) = self.state.vesting.get(&id).cloned() {
                self.write_vesting(id, Some(crate::palw_vesting_v1::PalwVestingRowV1 { expiry_daa: lift_daa, ..row }))?;
            }
        }
        Ok(())
    }

    /// **U3 (post-edit 12): `G_fp` for an FP claim this conviction is the FIRST to bind after its
    /// `Final`**, read at the opening: the claim's liability row is a free-prompt `Final`'s — written
    /// at the `Final` and not yet marked by any conviction (the marker, as S3's is the row's burn) —
    /// and the claim is still `Final` or retired. `G_fp = g_res_sompi + escrowed_reward` (the row's
    /// v22 fields; `escrowed_reward` is 0 on the FP lane). `None` otherwise.
    pub(super) fn post_final_fp_g_v1(&self, claim_id: &Hash64) -> Option<u128> {
        let row = self.state.panel_liabilities.get(claim_id)?;
        let first_after_final = row.voided_daa.is_none() && row.void_reason.is_none();
        let final_or_retired =
            self.state.claims.get(claim_id).is_none_or(|claim| matches!(claim.phase, PalwClaimPhaseV2::Final { .. }));
        (row.free_prompt && first_after_final && final_or_retired)
            .then(|| row.g_res_sompi.saturating_add(u128::from(row.escrowed_reward)))
    }

    /// **S3 and U3 (S-4): the producer's tier for a conviction that binds a claim after its `Final`**
    /// — called after the claim was acted on (`reverse_convicted_final` / `mark_liability_convicted`).
    ///
    /// * **S3** — [`Self::burn_vesting_row`] reports the row it burned (`Some`: the row's deletion is
    ///   the once-per-claim marker) ⇒ the producer is charged [`palw_rcore_s3s4_action_v1`]`(C₀, G)`.
    /// * **U3** — no row (the FP lane writes none), and this is the first conviction binding the FP
    ///   claim after its `Final` (`fp_first`, [`Self::post_final_fp_g_v1`] at the opening) ⇒ the
    ///   executor is charged the capped producer tier `min(25% · C₀, m · G_fp)`.
    /// * Otherwise nothing: no row (moved, already burned, or none written while
    ///   [`PALW_RCORE_VESTING_ROWS_LANDED_V1`] is `false`).
    ///
    /// Returns the nominal charge.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn post_final_producer_leg_v1(
        &mut self,
        conv: &PalwConvictionV1,
        claim_id: Hash64,
        producer: PalwBondKeyV2,
        offence_id: Hash64,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
        g: u128,
        fp_first: Option<u128>,
    ) -> Result<u128, PalwStateV2Error> {
        let action = match (self.burn_vesting_row(claim_id, offence_id, kind)?, fp_first) {
            (Some(_), _) => palw_rcore_s3s4_action_v1(conv.c0(&producer), g),
            (None, Some(g_fp)) => palw_rcore_s3s4_action_v1(conv.c0(&producer), g_fp),
            (None, None) => return Ok(0),
        };
        self.slash_bond(producer, action)?;
        Ok(action)
    }

    /// **Is `claim_id` live and not yet `Final`?** — the line between S2 (the claim is voided
    /// `CourtFraud`) and the post-`Final` tiers (S3, U3, R-1's `X`). A retired claim is not live.
    pub(super) fn claim_is_live_before_final_v1(&self, claim_id: &Hash64) -> bool {
        self.state
            .claims
            .get(claim_id)
            .is_some_and(|claim| !claim.phase.is_terminal() && !matches!(claim.phase, PalwClaimPhaseV2::Final { .. }))
    }

    /// **ADR-0152 §3.6 (S-4): THE entry a PROVEN court verdict against the executor goes through** —
    /// a court's `CourtClosed { ExecutorGuilty }` (whole or assembled from chunks), the shard and
    /// checkpoint courts' one-move `ExecutorGuilty`, the held-DA leaf verdicts, and the A-held
    /// stream's held-dissection verdict site (call it with the session's challenger and its
    /// `opened_daa`). A court DEFAULT (`PalwVoidReasonV2::CourtDefault`: an unanswered rung, a close
    /// declaration never assembled) is NOT a verdict and never comes here.
    ///
    /// Below `palw_rcore_plus`: the court's own charge, byte for byte — `void_and_slash(CourtFraud)`
    /// and, for a session, the court-time charge. Past it, one conviction: opened on the producer;
    /// the claim acted on by phase (live → voided `CourtFraud` with S2's action and the court time;
    /// `Final` → reversed, then S3/U3); and closed by [`Self::record_court_conviction_v1`].
    pub(super) fn convict_by_court_verdict_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        challenger: PalwBondKeyV2,
        court_opened_daa: Option<u64>,
    ) -> Result<(), PalwStateV2Error> {
        self.convict_by_court_verdict_as_v1(ctx, claim_id, claim, challenger, court_opened_daa, PalwVoidReasonV2::CourtFraud)
    }

    /// [`Self::convict_by_court_verdict_v1`] recording `reason` on the claim (and, past `Final`, the
    /// liability row): `CourtFraud` for a verdict proven against committed data,
    /// [`PalwVoidReasonV2::CourtHeldVerdict`] for a dissection's bottom past
    /// `palw_offence_attribution` ([`palw_court_verdict_void_reason_v1`]). Everything else — the
    /// charge, the court time, S-4's tiers, the `CourtConviction` record and the challenger's reward —
    /// is the funnel's, the same for both.
    pub(super) fn convict_by_court_verdict_as_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        challenger: PalwBondKeyV2,
        court_opened_daa: Option<u64>,
        reason: PalwVoidReasonV2,
    ) -> Result<(), PalwStateV2Error> {
        let now = ctx.daa_score;
        if !self.params.rcore_plus_active_at(now) {
            self.void_and_slash(claim_id, claim, now, reason)?;
            if let Some(opened) = court_opened_daa {
                self.charge_court_time_v1(claim.bond, claim.reserved, opened, now)?;
            }
            return Ok(());
        }
        let producer = claim.bond;
        let conv = self.open_conviction_v1(&[producer], now);
        let g = self.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);
        let live = self.claim_is_live_before_final_v1(&claim_id);
        let fp_first = self.post_final_fp_g_v1(&claim_id);
        let key = palw_court_conviction_offence_id_v1(&producer.0, &claim_id);
        let mut nominal = self.act_on_convicted_claim_as_v1(ctx, claim_id, Some(&conv), reason)?;
        if live {
            if let Some(opened) = court_opened_daa {
                nominal = nominal.saturating_add(self.charge_court_time_v1(producer, claim.reserved, opened, now)?);
            }
        } else {
            let tier = self.post_final_producer_leg_v1(
                &conv,
                claim_id,
                producer,
                key,
                crate::palw_offence_v1::PalwOffenceKindV1::CourtConviction,
                g,
                fp_first,
            )?;
            nominal = nominal.saturating_add(tier);
        }
        // ADR-0160 (D-5): a proven verdict is intent-class; a held dissection's verdict proves the
        // producer's filings false, not the committed execution, and keeps the tier.
        let offence = if reason == PalwVoidReasonV2::CourtHeldVerdict {
            crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1::CourtHeldVerdict
        } else {
            crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1::CourtFraud
        };
        self.record_court_conviction_v1(&conv, claim_id, producer, challenger, nominal, offence)
    }

    /// **§3.6 / N6 / V-2b (S-4): a proven court verdict's `CourtConviction` record (kind 6)** — once
    /// per claim under [`palw_court_conviction_offence_id_v1`]`(producer, claim)`, root 0 and the
    /// claim's unminted round rights forfeited BY CLAIM (V-2b), `collected` the producer's locked
    /// debit (R-2), and the reporter reward opened for the court's challenger
    /// ([`PalwConvictionBasisV1::CourtVerdict`]; the key is public at admission, so it takes no
    /// commitment, V3S-03). The closing half of [`Self::convict_by_court_verdict_v1`]; a site that
    /// charged the producer through the funnel itself calls it directly. A second verdict on the same
    /// claim records nothing.
    pub(super) fn record_court_conviction_v1(
        &mut self,
        conv: &PalwConvictionV1,
        claim_id: Hash64,
        producer: PalwBondKeyV2,
        challenger: PalwBondKeyV2,
        nominal: u128,
        offence: crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1,
    ) -> Result<(), PalwStateV2Error> {
        let key = palw_court_conviction_offence_id_v1(&producer.0, &claim_id);
        if self.state.consumed_offences.contains_key(&key) {
            return Ok(());
        }
        self.close_conviction_v1(
            conv,
            key,
            crate::palw_offence_v1::PalwOffenceKindV1::CourtConviction,
            producer,
            nominal,
            Hash64::default(),
            claim_id,
            &[producer],
            &[(producer, offence)],
        )?;
        self.forfeit_minted_round_rights_of_claim(&claim_id)?;
        self.open_reporter_reward(conv.now_daa, key, 0, PalwConvictionBasisV1::CourtVerdict { challenger })?;
        Ok(())
    }

    /// X11's strikes (row 16): one bond's live list, journaled whole (`Strikes`, a v22 delta).
    pub(super) fn write_strikes(&mut self, key: PalwBondKeyV2, new: Option<Vec<u64>>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.withholding_strikes, false, "native withholding_strikes writer")?;
        let old = match &new {
            Some(list) => self.state.withholding_strikes.insert(key, list.clone()),
            None => self.state.withholding_strikes.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::Strikes { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0152 A-1 on this fold's clocks**: [`palw_bond_committed_v1`] at `now_daa` and the
    /// escaped depth every lock of this fold is read at.
    pub(super) fn committed_at(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        palw_bond_committed_v1(&self.state, bond, now_daa, self.second_clock_depth(now_daa), self.params.window_court)
    }

    /// **The one invariant's room on this fold's clocks** ([`palw_rcore_gate_room_of_v1`], the S
    /// review's M1): a work gate's room is the 500‰ ceiling less `committed` and never past
    /// `collateral − committed − accuser`; an accuser gate's is the free half less the accuser ledger.
    pub(super) fn gate_room(&self, bond: &PalwBondKeyV2, now_daa: u64, gate: PalwRcoreGateV1) -> u128 {
        let Some(record) = self.state.bonds.get(bond) else { return 0 };
        palw_rcore_gate_room_split_of_v1(
            record.collateral,
            self.params.fp_max_exposure_ratio_permille,
            self.committed_at(bond, now_daa),
            // Lane V02: past `palw_final_lock_full_collateral` (at this gate's DAA) the resolved locks
            // leave the work ceiling and stay in the 100% term; 0 below it.
            palw_bond_off_ceiling_v1(&self.state, self.params, bond, now_daa, self.second_clock_depth(now_daa)),
            // Lane V02 (review HIGH): the relief never spends the accuser reserve (0 below the fence).
            palw_bond_accuser_reserve_v1(self.params, now_daa),
            // ADR-0152 §4-ter C4 (the review's F7): a held session at its charge, not its `reserved`.
            self.read().accuser_ledger_v1(bond, now_daa),
            gate,
        )
    }

    /// **ADR-0152 §4-ter C4: what a held dissection's losing challenger is charged, before the
    /// floor's cap** — `max(claim.reserved, G)`, `G = g_res + escrowed_reward` from
    /// [`Self::claim_g_v1`] (S's gain terms, `w + E + R + s`). One spelling for the charge
    /// (`rearm_after_challenger_side_close`) and the reservation that makes it certain
    /// (`open_held_dissection_v1`). The rule is the read's ([`PalwFoldReadV1::held_dissection_charge_v1`]),
    /// so the accuser gates price a held session with it too.
    pub(super) fn held_dissection_charge_v1(&self, claim_id: &Hash64, claim: &PalwClaimStateV2) -> u128 {
        self.read().held_dissection_charge_v1(claim_id, claim)
    }

    /// **ADR-0152 S-SPEC §2 `claim_g_v1`: a claim's gain terms for the action tiers, FROZEN** (the
    /// S-4 review) — [`palw_claim_g_v1`]: the liability row's copy, else the claim record's
    /// licence-time `G_res` ([`PalwClaimRcoreV1::g_res_sompi`]), else (unlicensed) its acceptance
    /// records' `w + R`. Never the live `rcore_g_res`: an action tier, R-1's `X` and DA-6's base are
    /// priced on the gain the licence rested on. `G = g_res + escrowed_reward`.
    pub(super) fn claim_g_v1(&self, claim_id: &Hash64) -> Option<PalwClaimGV1> {
        // The rule lives on the read ([`PalwFoldReadV1::claim_g_v1`]) so the accuser gates outside
        // the builder price a held session with the same `G` (ADR-0152 §4-ter, the review's F7).
        self.read().claim_g_v1(claim_id)
    }

    /// **ADR-0152 SR-6 + Q-3 + L-1: the backed subset of a licence set and the lock it posts**
    /// (S-SPEC §3.4 step 1).
    ///
    /// A carried `Valid` of panel seat `s` is backed iff `s` already locks this claim, or
    /// `committed(s) + max(0, lock_{k′} − duty(c, s)) ≤ C_s × 500‰`. `k′ = max(k, 2)` starts at the
    /// whole set's recount; if the backed subset recounts lower and `k′` falls from 3 to 2 (which
    /// raises the lock), the set is re-tested once at `lock_2` — it cannot fall again, `k′` being 2
    /// or 3. The price every backed signer locks is `lock_{max(basis_k, 2)}` of the final subset,
    /// which is the price it was tested at.
    pub(super) fn rcore_backed_set(
        &self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        carried: &[(crate::palw_panel_v2::PalwSeatReceiptV2, Option<crate::palw_verification_v2::PalwSegmentMaskV2>)],
        now_daa: u64,
    ) -> PalwRcoreBackedSetV1 {
        let seats: Vec<PalwBondKeyV2> =
            self.state.panels.get(&claim_id).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
        let segments = crate::palw_verification_v2::palw_segment_count_v2(seats.len().min(u16::MAX as usize) as u16);
        let full = crate::palw_verification_v2::PalwSegmentMaskV2::full(segments);
        let mut valid: Vec<(PalwBondKeyV2, crate::palw_verification_v2::PalwSegmentMaskV2)> = Vec::new();
        for (receipt, mask) in carried {
            if matches!(receipt.verdict, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid)
                && seats.contains(&receipt.seat_bond)
                && !valid.iter().any(|(bond, _)| *bond == receipt.seat_bond)
            {
                valid.push((receipt.seat_bond, mask.unwrap_or(full)));
            }
        }
        let recount = |set: &[(PalwBondKeyV2, crate::palw_verification_v2::PalwSegmentMaskV2)]| {
            palw_receipt_set_basis_k_v1(&set.iter().map(|(_, mask)| *mask).collect::<Vec<_>>(), segments)
        };
        // ADR-0160 lane liab (AG-3): a frozen seat's `Valid` backs nothing (the map is empty below
        // `Params::palw_capacity_aggregate_liability`).
        let backed_at = |lock: u128| -> Vec<(PalwBondKeyV2, crate::palw_verification_v2::PalwSegmentMaskV2)> {
            valid
                .iter()
                .filter(|(bond, _)| {
                    !crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&self.state, bond)
                        && (self.state.slashable_locks.contains_key(&(*bond, claim_id))
                            || lock.saturating_sub(palw_seat_duty_of_v1(&self.state, &claim_id, bond))
                                <= self.gate_room(bond, now_daa, PalwRcoreGateV1::Work))
                })
                .copied()
                .collect()
        };
        // `G_res` and `s` are read ONCE, and every price of this set is computed from them (the value
        // the licence records: the S-4 review's G freeze) — `PalwFoldReadV1::rcore_lock`, spelled
        // out so the recorded input is the priced one.
        let g_res = self.read().rcore_g_res(&claim_id, claim);
        let buyback = self.read().rcore_buyback_bound(&claim_id, claim);
        // ADR-0160 AS-2 (lane liab): the licence's lock re-priced at the claim's `accepted_daa` — for a
        // credited claim only (stage 2, v3 AS-1′).
        let step = crate::palw_audit_door_v1::palw_capacity_seat_step_v1(self.params, claim);
        let lock_at = |basis_k: u8| {
            crate::palw_aggregate_liability_v1::palw_seat_lock_v2(
                palw_rcore_lock_v1(g_res, claim.escrowed_reward, buyback, basis_k),
                step,
            )
        };
        let first = recount(&valid).max(PALW_RCORE_FINAL_BASIS_K_V1);
        let mut backed = backed_at(lock_at(first));
        if recount(&backed).max(PALW_RCORE_FINAL_BASIS_K_V1) < first {
            backed = backed_at(lock_at(PALW_RCORE_FINAL_BASIS_K_V1));
        }
        let basis_k = recount(&backed);
        PalwRcoreBackedSetV1 { lock: lock_at(basis_k), basis_k, segments, backed, g_res }
    }

    /// **ADR-0152 S-SPEC §3.4: one licence past `palw_rcore_plus`, through any of the three doors.**
    ///
    /// The carried set's outsider is required as it always was (ADR-0147, refused by name); then the
    /// backed subset ([`Self::rcore_backed_set`]) must satisfy the door's rule on its own — a quorum
    /// of `Valid`s (V1), the quorum and a recount of at least 2, which is coverage (V2), the full seat
    /// (S2) — and still name its outsider, or the object is INERT (`Ok`, nothing written; SR-6). An
    /// unbacked `Valid` gets no credit, no lock and no served bit. Every backed signer locks the
    /// set's price with its attested mask (L-1, L-3; a V2 receipt the full cut); `door_lock_prices`
    /// and `lock_1` are not read. The record is staged over what the licence reads and written once
    /// by [`Self::license_claim`].
    pub(super) fn license_rcore_v1(
        &mut self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        door: PalwLicenceDoorV1,
        carried: &[(crate::palw_panel_v2::PalwSeatReceiptV2, Option<crate::palw_verification_v2::PalwSegmentMaskV2>)],
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        let all: Vec<crate::palw_panel_v2::PalwSeatReceiptV2> = carried.iter().map(|(receipt, _)| receipt.clone()).collect();
        palw_licence_names_its_outsider_v1(
            &self.state,
            &claim_id,
            claim,
            &palw_seat_verdicts_of_v2(&all),
            self.extras.admission_independence_daa,
        )?;
        let set = self.rcore_backed_set(claim_id, claim, carried, now_daa);
        let reads: Vec<(crate::palw_panel_v2::PalwSeatReceiptV2, Option<crate::palw_verification_v2::PalwSegmentMaskV2>)> = carried
            .iter()
            .filter(|(receipt, _)| {
                !matches!(receipt.verdict, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid)
                    || set.backed.iter().any(|(bond, _)| *bond == receipt.seat_bond)
            })
            .cloned()
            .collect();
        let read_receipts: Vec<crate::palw_panel_v2::PalwSeatReceiptV2> = reads.iter().map(|(receipt, _)| receipt.clone()).collect();
        let verdicts = palw_seat_verdicts_of_v2(&read_receipts);
        let quorum = crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1 as usize;
        let door_holds = match door {
            PalwLicenceDoorV1::Quorum => set.backed.len() >= quorum,
            PalwLicenceDoorV1::Coverage => set.backed.len() >= quorum && set.basis_k >= PALW_RCORE_FINAL_BASIS_K_V1,
            PalwLicenceDoorV1::Optimistic => {
                let full = self.read().optimistic_full_seat(claim_id);
                full.is_some_and(|full| set.backed.iter().any(|(bond, _)| *bond == full))
            }
            // Dormant on testnet-12 (`palw_shard_licensing` is refused beside R-core+).
            PalwLicenceDoorV1::ShardPart { .. } => false,
        };
        if !door_holds
            || palw_licence_names_its_outsider_v1(&self.state, &claim_id, claim, &verdicts, self.extras.admission_independence_daa)
                .is_err()
        {
            return Ok(());
        }
        self.slash_dissenting_seats(&claim_id, claim, &verdicts, true)?;
        self.slash_silent_seats(&claim_id, claim, &verdicts)?;
        self.credit_seat_receipts(claim_id, &read_receipts, now_daa)?;
        for (bond, mask) in &set.backed {
            self.lock_valid_seat_rcore(*bond, claim_id, set.lock, *mask, set.segments, now_daa)?;
        }
        let mut staged = {
            let refs: Vec<(&crate::palw_panel_v2::PalwSeatReceiptV2, Option<crate::palw_verification_v2::PalwSegmentMaskV2>)> =
                reads.iter().map(|(receipt, mask)| (receipt, *mask)).collect();
            self.staged_licence_v1(claim_id, claim, door, &refs, now_daa)
        };
        debug_assert_eq!(staged.rcore.basis_k, set.basis_k, "the record recounts the backed subset");
        // The S-4 review's G freeze: the `G_res` this licence priced its locks with, recorded once — a
        // claim that already licensed keeps the value its first licence recorded.
        staged.rcore.g_res_sompi = if claim.rcore.licence_door.is_some() { claim.rcore.g_res_sompi } else { set.g_res };
        self.license_claim(claim_id, staged, now_daa)
    }

    /// **Lane g14-close-legacy (audit G-13): the life a `Valid` lock on `claim_id` dated at `now_daa` gets** —
    /// [`PalwStateParamsV2::valid_lock_life_for_claim_v1`] of the claim's acceptance: the claim's liability horizon (`window_court`)
    /// for a claim accepted past `palw_legacy_public_filer_v1`, lane V02's `final_lock_life_at` otherwise (and for a claim the state no
    /// longer holds), byte for byte.
    pub(super) fn valid_lock_life_v1(&self, claim_id: &Hash64, now_daa: u64) -> u64 {
        match self.state.claims.get(claim_id) {
            Some(claim) => self.params.valid_lock_life_for_claim_v1(claim.accepted_daa, now_daa),
            None => self.params.final_lock_life_at(now_daa),
        }
    }

    /// **ADR-0152 L-1 / L-3: a backed `Valid` signer's lock** past `palw_rcore_plus` — the set's
    /// price, the signer's attested mask and the claim's segment cut, dated like every lock. Backing
    /// was established by the caller on the one ledger, so the 100% `slashable_available` ledger is
    /// not read. A signer already locked on this claim keeps its lock (Q-4: never repriced).
    pub(super) fn lock_valid_seat_rcore(
        &mut self,
        seat: PalwBondKeyV2,
        claim_id: Hash64,
        amount: u128,
        attested: crate::palw_verification_v2::PalwSegmentMaskV2,
        segments: u16,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.state.slashable_locks.contains_key(&(seat, claim_id)) {
            return Ok(());
        }
        let expiry_daa = crate::palw_panel_var_v1::palw_panel_liability_expiry_v1(
            self.valid_lock_anchor_v1(&claim_id, now_daa),
            // Lane V02 (post-launch): the seat lock's life is `window_court` below the fence, shortened
            // to `PALW_FINAL_LOCK_LIFE_DAA_V1` past it (keyed on the dating DAA — the `Final` re-dates it).
            // G-13 (lane g14-close-legacy): a claim accepted past `palw_legacy_public_filer_v1` keeps `window_court`.
            self.valid_lock_life_v1(&claim_id, now_daa),
        );
        self.write_slashable_lock(
            (seat, claim_id),
            Some(crate::palw_panel_var_v1::PalwSlashableLockV1 {
                claim: claim_id,
                amount,
                expiry_daa,
                settled_at_final: self.state.settled_attempt_finals,
                attested,
                segments,
            }),
        )?;
        Ok(())
    }

    /// **§4-quater V5: the DAA a `Valid` lock taken at `now_daa` on `claim_id` is dated from** —
    /// `max(now, H(c))`, so a lock written at licence (or by a supplementary receipt) stays live until
    /// `H + window_court`: a conviction an honest seat can only file once its replay finishes still
    /// finds the lock. Below the fence `H` is `None` and this is `now_daa`, byte for byte. The lock
    /// written at Final (`persist_panel_liability`) is dated from `F ≥ H` already.
    pub(super) fn valid_lock_anchor_v1(&self, claim_id: &Hash64, now_daa: u64) -> u64 {
        self.state
            .claims
            .get(claim_id)
            .and_then(|claim| palw_claim_verify_horizon_v1(&self.state, self.params, claim_id, claim))
            .map_or(now_daa, |horizon| now_daa.max(horizon))
    }

    /// [`PalwFoldReadV1::panel_valid_lock_required`], on this fold's inputs.
    pub(super) fn panel_valid_lock_required(&self, claim: &PalwClaimStateV2) -> u128 {
        self.read().panel_valid_lock_required(claim)
    }

    // ---- ADR-0152 v3.1 R, the reporter reward (S-7): the writers of rows 11, 12 and 17 ----------
    //
    // Each journals its own delta entry and nothing else writes the three items, so the delta cannot
    // miss a change. Row 13 (`reporter_commitments`) and its two derived indexes are S-1's one writer,
    // `write_reporter_commit`, keyed by the slot `palw_reporter_commit_slot_v1(commitment, reporter)`.

    /// Row 12: open, improve (`best`) or close one pending reporter reward.
    pub(super) fn write_reward_pending(&mut self, key: Hash64, new: Option<PalwPendingRewardV1>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.reward_pending, false, "native reward_pending writer")?;
        let old = self.state.reward_pending.get(&key).copied();
        if old == new {
            return Ok(());
        }
        match new {
            Some(row) => {
                self.state.reward_pending.insert(key, row);
            }
            None => {
                self.state.reward_pending.remove(&key);
            }
        }
        self.entries.push(PalwDeltaEntryV2::RewardPending { key, old, new });
        Ok(())
    }

    /// Row 11: an award (written by `sweep_reward_reveals` at step 2) or its removal. **Seam for the
    /// vesting stream's step 3d**, which MOVES each award into `pending_payouts` (under
    /// `palw_reporter_payout_key_v1(offence_key)`, S-SPEC P12) and deletes it here with `None`; S
    /// never deletes one itself.
    pub(super) fn write_reporter_reward(&mut self, key: Hash64, new: Option<PalwPayoutV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.reporter_rewards, false, "native reporter_rewards writer")?;
        let old = self.state.reporter_rewards.get(&key).copied();
        if old == new {
            return Ok(());
        }
        match new {
            Some(row) => {
                self.state.reporter_rewards.insert(key, row);
            }
            None => {
                self.state.reporter_rewards.remove(&key);
            }
        }
        self.entries.push(PalwDeltaEntryV2::ReporterReward { key, old, new });
        Ok(())
    }

    /// Row 17, the reporter half: awarded and forgone sompi, journaled whole.
    pub(super) fn write_reporter_counters(&mut self, new: PalwReporterCountersV1) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native reporter_counters writer")?;
        let old = self.state.reporter_counters;
        if old == new {
            return Ok(());
        }
        self.state.reporter_counters = new;
        self.entries.push(PalwDeltaEntryV2::ReporterCounters { old, new });
        Ok(())
    }

    /// **ADR-0152 v3.1 R-1/R-4: open a conviction's reporter reward** — the SEAM S-4's conviction
    /// funnel (and, for a DA default, M3's DA-7) calls at the close of EVERY conviction, naming what
    /// it rested on ([`PalwConvictionBasisV1`]). Writes `reward_pending[offence_key] = { amount,
    /// reveal_until: now + window_receipt, evidence_id, best }` and returns `Some(amount)`, or writes
    /// nothing and returns `None` (below `Params::palw_rcore_plus`, a court default by silence, or a
    /// zero reward). The amount is R-1's ([`palw_reporter_reward_amount_v1`]) over the consumed
    /// record's `collected` less `extracted`; what is not paid as reward stays burned (the slash
    /// already debited it).
    ///
    /// **Only a proven conviction pays (the audit's rule), and the seam enforces it by basis, not by
    /// comment:** a [`PalwConvictionBasisV1::CourtDefault`] — an unanswered rung or the backstop
    /// convicting a responder that made no move — opens nothing, and every basis is checked against
    /// the consumed record's kind (`CheckedEvidence` for kinds 0, 3, 4; `CourtVerdict` and
    /// `CourtDefault` for kind 6; `DaDefault` for kind 5), so a court conviction cannot reach a
    /// reward without the funnel naming it a verdict. **Recorded decision on DA defaults:** a DA
    /// default pays (R-3/DA-7, V3S-12: to the earliest defaulted session's accuser, on the producer's
    /// debit only) — the offence is the withholding itself, of material the producer was bound to
    /// keep and disclose (P2-7), not a presumption drawn from an absent move.
    ///
    /// **The rest of the call contract** (for the audit session's S-4 and for M3):
    ///
    /// 1. Not for S0′ (RT#2, `UnavailableQuorum`, `NotReplayBacked`), a refuted accusation's charge,
    ///    a kind-3 conviction on contradiction 2 (`ProducerWithholding`) or 4 (`CourtFraud`) — those
    ///    restate a DA default or a court verdict and pay no separate reward — or any burned vesting.
    /// 2. **Call it after `write_consumed_offence`** for `offence_key`, in the same block: the
    ///    record's `collected` (R-2: 0 for a bond whose withdrawal gate was open at the conviction,
    ///    the real remainder for a pre-drained one) is the base, and its `accepted_daa` is what a
    ///    commitment must precede and must equal `now_daa`.
    /// 3. **`extracted` is R-1's `X`** ([`palw_reporter_reward_extracted_v1`]): `min(lock, G_res /
    ///    basis_k)` for a load-bearing signer's lock on a claim that reached Final, 0 otherwise.
    /// 4. A step-2 (sweep-time) conviction calls it the same way; the reveal window then runs from
    ///    that block, and the key is swept no earlier than `now + window_receipt + 1`.
    ///
    /// A named winner equal to the accused, or one with no bond record, is recorded with no winner
    /// (R-3's hygiene), so the amount goes to `forgone_sompi` at the sweep. Errors (never a filer's
    /// fault, always the caller's): no consumed record, a record from another block, a basis its
    /// kind does not take, a `CheckedEvidence` with a zero `evidence_id`, a key already pending or
    /// awarded.
    // The seam: S-4's conviction funnel calls it at the close of every conviction that pays (DA-7's
    // charge hook among them).
    pub(crate) fn open_reporter_reward(
        &mut self,
        now_daa: u64,
        offence_key: Hash64,
        extracted: u128,
        basis: PalwConvictionBasisV1,
    ) -> Result<Option<u64>, PalwStateV2Error> {
        use crate::palw_offence_v1::PalwOffenceKindV1 as K;
        // S-4's lib tests read what every conviction site hands this seam (the basis and `X`), as
        // they did through S-4's shim of it.
        #[cfg(test)]
        tests::S4_REWARD_CALLS.with(|calls| calls.borrow_mut().push((now_daa, offence_key, extracted, basis)));
        if !self.params.rcore_plus_active_at(now_daa) {
            return Ok(None);
        }
        let refuse = |why| Err(PalwStateV2Error::ReporterRewardUnopenable { offence_key, why });
        let Some(record) = self.state.consumed_offences.get(&offence_key) else {
            return refuse("no consumed offence under the key (write the record first)");
        };
        if record.accepted_daa != now_daa {
            return refuse("the offence was consumed in another block");
        }
        let (accused, collected, kind) = (PalwBondKeyV2(record.accused), record.collected, record.kind);
        let fits = match basis {
            PalwConvictionBasisV1::CheckedEvidence { .. } => {
                matches!(kind, K::ExecutorEquivocation | K::PanelFalseValidV2 | K::ExecutorRefuted | K::TirIdentityMismatch)
            }
            PalwConvictionBasisV1::CourtVerdict { .. } | PalwConvictionBasisV1::CourtDefault => kind == K::CourtConviction,
            PalwConvictionBasisV1::DaDefault { .. } => kind == K::DaDefault,
        };
        if !fits {
            return refuse("the consumed offence's kind does not rest on this basis");
        }
        // TODO(S-4, the audit's void reason 7): once `PalwVoidReasonV2::CourtDefault` lands, refuse a
        // `CourtVerdict` whose claim (`record.claim_id`) that reason voided — the state's own record
        // of a silent default, so the seam need not take the funnel's word for it.
        let (evidence_id, named) = match basis {
            // The audit's rule: a default by silence is never paid, and its debit stays burned.
            PalwConvictionBasisV1::CourtDefault => return Ok(None),
            PalwConvictionBasisV1::CheckedEvidence { evidence_id } if evidence_id == Hash64::default() => {
                return refuse("a commit-reveal reward needs the consumed evidence's id");
            }
            PalwConvictionBasisV1::CheckedEvidence { evidence_id } => (evidence_id, None),
            PalwConvictionBasisV1::CourtVerdict { challenger } => (Hash64::default(), Some(challenger)),
            PalwConvictionBasisV1::DaDefault { accuser } => (Hash64::default(), Some(accuser)),
        };
        if self.state.reward_pending.contains_key(&offence_key) || self.state.reporter_rewards.contains_key(&offence_key) {
            return refuse("a reward under this key is already pending or awarded");
        }
        // The share is fixed by the conviction's close (`now_daa`, which is the record's `accepted_daa`):
        // 10% below `Params::palw_reporter_share_v2`, 49% at or past it (lane INTF).
        let amount = palw_reporter_reward_amount_at_bps_v1(collected, extracted, self.params.reporter_reward_bps_at(now_daa));
        if amount == 0 {
            return Ok(None);
        }
        let reveal_until =
            now_daa.checked_add(self.params.window_receipt()).ok_or(PalwStateV2Error::Overflow("reporter reveal window"))?;
        let best = named.filter(|reporter| *reporter != accused).and_then(|reporter| {
            self.state.bonds.get(&reporter).map(|bond| PalwRewardWinnerV1 {
                committed_daa: 0,
                commitment: Hash64::default(),
                reporter,
                payload: bond.payout_payload,
            })
        });
        if let Some(winner) = best {
            self.bind_native_reporter_v1(now_daa, offence_key, winner, amount, now_daa)?;
        }
        self.write_reward_pending(offence_key, Some(PalwPendingRewardV1 { amount, reveal_until, evidence_id, best }))?;
        Ok(Some(amount))
    }

    /// [`PalwFoldReadV1::door_lock_prices`], on this fold's inputs (2026-09-23 audit #6).
    pub(super) fn door_lock_prices(
        &self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        door: PalwLicenceDoorV1,
    ) -> PalwDoorLockPricesV1 {
        self.read().door_lock_prices(claim_id, claim, door)
    }

    /// The second clock's depth at `now_daa` AFTER the liveness escape
    /// ([`palw_second_clock_depth_v1`]): `None` below the fence, and `None` once no anchor has
    /// settled for `2 × window_court`.
    pub(super) fn second_clock_depth(&self, now_daa: u64) -> Option<u64> {
        self.read().second_clock_depth(now_daa)
    }

    /// **One anchor settles at `daa`** — the one writer of the second clock. The counter moves;
    /// past `palw_audit_2026_09_23` the anchor's DAA also joins `recent_anchor_daas`, and the
    /// entries no live panel's anchor can still ask about are pruned in the same step
    /// ([`palw_anchor_ring_prune_count_v1`]). Every move is its own delta entry, so a reorg
    /// undoes it exactly.
    pub(super) fn settle_anchor(&mut self, daa: u64, into_ring: bool) -> Result<(), PalwStateV2Error> {
        let old = self.state.settled_attempt_finals;
        let new = old.checked_add(1).ok_or(PalwStateV2Error::Overflow("settled_attempt_finals"))?;
        self.state.settled_attempt_finals = new;
        self.entries.push(PalwDeltaEntryV2::SettledFinals { old, new });
        if into_ring {
            // Chain blocks carry non-decreasing DAA scores, so this is `daa` itself; the `max`
            // only keeps the ring sorted if that ever stopped being true.
            let daa = self.state.recent_anchor_daas.last().map_or(daa, |last| daa.max(*last));
            self.state.recent_anchor_daas.push(daa);
            self.entries.push(PalwDeltaEntryV2::AnchorDaaPushed { daa });
            let prune = palw_anchor_ring_prune_count_v1(
                &self.state.recent_anchor_daas,
                daa,
                palw_anchor_ring_horizon_v1(self.params),
                self.extras.settled_anchor_depth.unwrap_or(0),
            );
            for _ in 0..prune {
                let first = self.state.recent_anchor_daas.remove(0);
                self.entries.push(PalwDeltaEntryV2::AnchorDaaPruned { daa: first });
            }
        }
        Ok(())
    }

    pub(super) fn slashable_available(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        // Their one expression (`palw_slashable_available_v1`, route-matrix #3), under the per-lock
        // liveness and the escaped depth of the 2026-09-24 DoS audit, so the draw and the bind agree.
        self.state.palw_slashable_available_v1(bond, now_daa, self.second_clock_depth(now_daa), self.params.window_court)
    }

    /// [`palw_false_valid_lock_slashable_v1`] on this fold's inputs (the escaped depth the bind reads).
    pub(super) fn false_valid_lock_slashable_v1(
        &self,
        claim_id: &Hash64,
        lock: &crate::palw_panel_var_v1::PalwSlashableLockV1,
        now_daa: u64,
    ) -> bool {
        palw_false_valid_lock_slashable_v1(&self.state, self.params, claim_id, lock, now_daa, self.second_clock_depth(now_daa))
    }

    pub(super) fn slashable_live_locked(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        let (settled_now, depth) = (self.state.settled_attempt_finals, self.second_clock_depth(now_daa));
        self.state
            .slashable_locks_of(bond)
            .filter(|(_, lock)| lock.is_live_v3(now_daa, settled_now, depth, self.params.window_court))
            .map(|(_, lock)| lock.amount)
            .fold(0u128, u128::saturating_add)
    }

    pub(super) fn lock_valid_seat(
        &mut self,
        seat: PalwBondKeyV2,
        claim_id: Hash64,
        required: u128,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.extras.objective_offence_at(now_daa) {
            return Ok(());
        }
        if self.state.slashable_locks.contains_key(&(seat, claim_id)) {
            return Ok(());
        }
        let available = self.slashable_available(&seat, now_daa);
        if available < required {
            return Err(PalwStateV2Error::SeatValidLockRefused { seat, claim: claim_id, required, available });
        }
        let expiry_daa = crate::palw_panel_var_v1::palw_panel_liability_expiry_v1(
            self.valid_lock_anchor_v1(&claim_id, now_daa),
            // Lane V02 (post-launch): the seat lock's life is `window_court` below the fence, shortened
            // to `PALW_FINAL_LOCK_LIFE_DAA_V1` past it (keyed on the dating DAA — the `Final` re-dates it).
            // G-13 (lane g14-close-legacy): a claim accepted past `palw_legacy_public_filer_v1` keeps `window_court`.
            self.valid_lock_life_v1(&claim_id, now_daa),
        );
        self.write_slashable_lock(
            (seat, claim_id),
            Some(crate::palw_panel_var_v1::PalwSlashableLockV1 {
                claim: claim_id,
                amount: required,
                expiry_daa,
                settled_at_final: self.state.settled_attempt_finals,
                // ADR-0152 v22 skeleton: S-3 writes the attested mask and the segment cut here past
                // `palw_rcore_plus`; until then no lock records them.
                attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                segments: 0,
            }),
        )?;
        Ok(())
    }

    /// **2026-09-23 audit, the receipt half of C-3: is every `Valid` seat in this set able to post
    /// its lock?** A `Valid` signature that no collateral stands behind is worth nothing, and below
    /// the fence its refusal (`SeatValidLockRefused`) propagated as the transition's error and
    /// disqualified the block that carried the receipt set — one over-committed seat's signature,
    /// for a relay fee. Past the fence a set with any such seat is INERT: the claim stays
    /// `PanelBound`, the assemblers offer only a set the fold licenses
    /// (`palw_v2_object_licenses_claim_v1`, so kaspad falls through to its next door), and
    /// otherwise the receipt window voids the claim as if no quorum had signed.
    ///
    /// **Audit #6:** each seat is asked for the price of the `door` the set arrives through
    /// ([`Self::door_lock_prices`]) — so an S2 set whose full-replay seat cannot post the whole
    /// gain is inert here, and never an error.
    pub(super) fn receipt_set_is_backed(
        &self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2],
        now_daa: u64,
        door: PalwLicenceDoorV1,
    ) -> bool {
        if !self.extras.objective_offence_at(now_daa) {
            return true;
        }
        let prices = self.door_lock_prices(claim_id, claim, door);
        receipts.iter().all(|receipt| {
            !matches!(receipt.verdict, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid)
                || self.state.slashable_locks.contains_key(&(receipt.seat_bond, claim_id))
                || self.slashable_available(&receipt.seat_bond, now_daa) >= prices.of(&receipt.seat_bond)
        })
    }

    /// Every `Valid` signer of `receipts` locks the price of the `door` they license through
    /// ([`Self::door_lock_prices`]); a seat already locked on this claim keeps its lock.
    pub(super) fn lock_valid_receipts(
        &mut self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2],
        now_daa: u64,
        door: PalwLicenceDoorV1,
    ) -> Result<(), PalwStateV2Error> {
        if !self.extras.objective_offence_at(now_daa) {
            return Ok(());
        }
        let prices = self.door_lock_prices(claim_id, claim, door);
        for receipt in receipts {
            if matches!(receipt.verdict, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid) {
                self.lock_valid_seat(receipt.seat_bond, claim_id, prices.of(&receipt.seat_bond), now_daa)?;
            }
        }
        Ok(())
    }

    pub(super) fn require_panel_lock_eligible(
        &self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        seats: &[PalwPanelSeatV2],
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.extras.objective_offence_at(now_daa) {
            return Ok(());
        }
        // **ADR-0152 L-4b, generalized (S-SPEC §3.3): past `palw_rcore_plus` a seat binds iff it may
        // take work at the panel floor and has room under its 500‰ ceiling for `max(duty_bind,
        // lock_2)`** — the one ledger, not the 100% `slashable_available` ledger (dos_l5_4b's double
        // backing). The duty it reserves is `duty_bind`; the rest of a lock above it is the top-up
        // the licence takes from the same room.
        if self.params.rcore_plus_active_at(now_daa) {
            let distinct: BTreeSet<PalwBondKeyV2> = seats.iter().map(|seat| seat.bond).collect();
            let prices = self.read().rcore_bind_prices(&claim_id, claim, distinct.len(), now_daa);
            let floor = crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(self.params.min_collateral_sompi());
            for seat in &distinct {
                // ADR-0160 lane liab (AG-3): a frozen bond is not a seat (the draw never seats one; the
                // map is empty below the fence).
                let may_work = self.state.bonds.get(seat).is_some_and(|record| palw_bond_may_take_work_v2(record, floor))
                    && !crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&self.state, seat);
                let room = self.gate_room(seat, now_daa, PalwRcoreGateV1::Work);
                if !may_work || room < prices.eligibility {
                    return Err(PalwStateV2Error::SeatValidLockRefused {
                        seat: *seat,
                        claim: claim_id,
                        required: prices.eligibility,
                        available: if may_work { room } else { 0 },
                    });
                }
            }
            return Ok(());
        }
        let required = self.panel_valid_lock_required(claim);
        for seat in seats {
            let available = self.slashable_available(&seat.bond, now_daa);
            if available < required {
                return Err(PalwStateV2Error::SeatValidLockRefused { seat: seat.bond, claim: claim_id, required, available });
            }
        }
        Ok(())
    }

    pub(super) fn persist_panel_liability(
        &mut self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        now_daa: u64,
        voided: Option<(u64, PalwVoidReasonV2)>,
    ) -> Result<(), PalwStateV2Error> {
        if !self.extras.objective_offence_at(now_daa) {
            return Ok(());
        }
        let expiry_daa = crate::palw_panel_var_v1::palw_panel_liability_expiry_v1(now_daa, self.params.window_court);
        // Lane V02 (post-launch): the LOCK re-dates to `F + final_lock_life_at(F)` — `window_court`
        // below the fence (byte for byte the record's `expiry_daa`), `PALW_FINAL_LOCK_LIFE_DAA_V1`
        // past it. The liability RECORD keeps `expiry_daa` (`window_court`), so the conviction path,
        // the vesting-row escrow and the pruning slack are untouched: only the seat's capital, its
        // withdrawal gate and its duty backing (all `is_live`) release `window_court − 1,000` sooner.
        // `lock_expiry_daa ≤ expiry_daa` keeps the sweep's group-prune invariant (a lock's expiry is
        // at most its liability's), so lock and record still prune together at the record's horizon.
        // G-13 (lane g14-close-legacy): a claim accepted past `palw_legacy_public_filer_v1` dates its locks to the record's own
        // horizon (`F + window_court`), so a conviction anywhere inside the liability window finds every relied-on lock live.
        let lock_expiry_daa = crate::palw_panel_var_v1::palw_panel_liability_expiry_v1(
            now_daa,
            self.params.valid_lock_life_for_claim_v1(claim.accepted_daa, now_daa),
        );
        // Lane F2-lock (post-launch, `Params::palw_final_lock_life_retro`): past its height an honest
        // `Final` dates its seat locks to EXACTLY `F + final_lock_life_at(F)` — a lock stamped
        // `L + window_court` at a licence below lane V02's fence is shortened, not kept — so such a claim
        // ends where the crossing block's re-date ends the ones already `Final`. Not for a void (extend-only,
        // as below the height), and not for a claim with data-availability history: DA-5's re-keys hold its
        // locks behind a session's end (`da_rekey_v1`, only ever raising), and a closed session's end is
        // not in the rooted state — such a claim keeps lane V02's extend-only re-date. A shortened lock
        // restarts both clocks here, as an extended one does: its liability begins at the `Final`.
        let exact =
            voided.is_none() && self.params.final_lock_life_retro_active_at(now_daa) && !self.state.da_claims.contains_key(&claim_id);
        let mut valid_signers: Vec<(crate::tx::TransactionOutpoint, Hash64)> = Vec::new();
        let mut locked_sompi = 0u128;
        let keys: Vec<(PalwBondKeyV2, Hash64)> = self.state.slashable_locks.keys().filter(|(_, c)| *c == claim_id).copied().collect();
        for key in keys {
            if let Some(lock) = self.state.slashable_locks.get(&key).copied() {
                locked_sompi = locked_sompi.saturating_add(lock.amount);
                valid_signers.push((key.0.0, key.1));
                if lock.expiry_daa < lock_expiry_daa || (exact && lock.expiry_daa > lock_expiry_daa) {
                    // The liability begins at the Final: both clocks start here.
                    self.write_slashable_lock(
                        key,
                        Some(crate::palw_panel_var_v1::PalwSlashableLockV1 {
                            expiry_daa: lock_expiry_daa,
                            settled_at_final: self.state.settled_attempt_finals,
                            ..lock
                        }),
                    )?;
                }
            }
        }
        // **2026-09-24 DoS audit #12 (a): a `BindTimeout` that no `Valid` signed leaves no
        // liability** (the user's decision; past `palw_audit_2026_09_23` only). The row exists so a
        // `Valid` signer can still be convicted after the claim row retires, and so a contradiction
        // naming the void (`ProducerWithholding`, `CourtFraud`) can still bind. A claim that never
        // bound its panel has neither: no lock was ever taken on it (locks are written at the
        // licence, which a `Provisional` claim never reached) and no contradiction names a
        // `BindTimeout`. The fallback below would list the panel's seats as signers of nothing — a
        // free permanent row per abandoned claim, the cheapest entry in the audit's growth table.
        if valid_signers.is_empty()
            && self.extras.audit_2026_09_23_active
            && matches!(voided, Some((_, PalwVoidReasonV2::BindTimeout)))
        {
            return Ok(());
        }
        if valid_signers.is_empty()
            && let Some(panel) = self.state.panels.get(&claim_id)
        {
            for seat in &panel.seats {
                valid_signers.push((seat.bond.0, claim_id));
            }
        }
        let (voided_daa, void_reason) = match voided {
            Some((daa, reason)) => (Some(daa), Some(reason)),
            None => (None, None),
        };
        // ADR-0152 v22 row 8 (S-3): what the conviction funnel reads after the claim retires — the
        // Final-basis door and recount, and `G_res` COPIED from the claim (the value its licence
        // priced the locks with, or its acceptance records' before one: the S-4 review's G freeze —
        // never recomputed here from this block's live inputs), so `G = g_res + escrowed_reward`
        // survives the claim record. Past `palw_rcore_plus` only.
        let rcore_row = self.params.rcore_plus_active_at(now_daa).then(|| {
            let scale = crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(self.params);
            (claim.rcore.licence_door, claim.rcore.basis_k, palw_claim_frozen_g_res_v1(&scale, claim))
        });
        let attribution = self.extras.offence_attribution_active;
        let segment_count = self
            .state
            .panels
            .get(&claim_id)
            .map(|panel| crate::palw_verification_v2::palw_segment_count_v2(u16::try_from(panel.seats.len()).unwrap_or(u16::MAX)))
            .unwrap_or(0);
        self.write_panel_liability(
            claim_id,
            Some(crate::palw_panel_var_v1::PalwPanelLiabilityRecordV1 {
                claim_id,
                work_id: claim.work_id.unwrap_or(claim.execution_root),
                class_id: claim.class_id,
                execution_root: claim.execution_root,
                output_root: claim.output_root,
                executor_bond: claim.bond,
                voided_daa,
                void_reason,
                valid_signers,
                locked_sompi,
                expiry_daa,
                settled_at_final: self.state.settled_attempt_finals,
                // **ADR-0152 v3.1 J-1 (F1, SPEC §4.1): the attribution copies**, past
                // `offence_attribution_active` only — what a conviction reads once the claim row has
                // retired: the identity, the lane, the trace root (J4) and the panel's segment cut
                // (a V3 receipt's liability).
                job_identity: if attribution { claim.job_identity } else { Hash64::default() },
                free_prompt: attribution && matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. }),
                trace_root: if attribution { claim.trace_root } else { Hash64::default() },
                segment_count: if attribution { segment_count } else { 0 },
                // v22 row 8 (S-3): the door, the basis and the residual gain, past `palw_rcore_plus`.
                licence_door: rcore_row.map(|(door, _, _)| door).unwrap_or(None),
                basis_k: rcore_row.map(|(_, basis_k, _)| basis_k).unwrap_or(0),
                g_res_sompi: rcore_row.map(|(_, _, g_res)| g_res).unwrap_or(0),
                escrowed_reward: if rcore_row.is_some() { claim.escrowed_reward } else { 0 },
            }),
        )?;
        Ok(())
    }

    pub(super) fn consume_objective_offence(
        &mut self,
        ctx: &PalwBlockContextV2,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
        accused: PalwBondKeyV2,
        evidence_id: Hash64,
        evidence: &[u8],
    ) -> Result<(), PalwStateV2Error> {
        self.consume_objective_offence_with_budget_v1(
            ctx,
            kind,
            accused,
            evidence_id,
            evidence,
            crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1::DirectCarrier,
        )
    }

    pub(super) fn consume_objective_offence_with_budget_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
        accused: PalwBondKeyV2,
        evidence_id: Hash64,
        evidence: &[u8],
        evidence_budget: crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_offence_v1::{PalwConsumedOffenceV1, PalwOffenceKindV1, PalwPanelFalseValidEvidenceV1, palw_offence_id_v1};
        if !self.extras.objective_offence_at(ctx.daa_score) {
            return Err(PalwStateV2Error::ObjectiveOffenceDormant);
        }
        // **ADR-0152 v3.1: kinds 5 and 6 are records the FOLD writes — never filed** (the skeleton
        // review's F1). Refused by name on every network before anything else is read, permanently:
        // landing the fold's own writer (M3: 5; S-4: 6) opens no filing route. The processor's gate
        // refuses the same kinds first.
        if kind.is_fold_recorded_only_v1() {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                evidence_id,
                crate::palw_offence_v1::PalwOffenceVerifyError::KindNotFileable(kind.not_fileable_reason_v1()).to_string(),
            ));
        }
        if let Some(reference) = crate::palw_staged_court_state_v1::palw_staged_reference_v1(kind, evidence) {
            if self.extras.staged_public_court.is_none() {
                return Err(PalwStateV2Error::ObjectiveOffenceDormant);
            }
            if !crate::palw_staged_court_state_v1::palw_staged_collection_open_v1(&self.state, &reference, ctx.daa_score) {
                return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                    evidence_id,
                    "staged certificate collection is outside its claim-stamped accepting clock".into(),
                ));
            }
        }
        // **ADR-0152 v2 F2: past `palw_offence_attribution` a false `Valid` has ONE route.** The V1
        // kind is refused by name — its evidence convicts on `job_id == claim_id`, a fixed point no
        // real claim reaches, while a hand-signed V2 receipt still clears it for the state-bound
        // contradictions (`ConflictingPermit` names no claim at all) — and the V2 kind is judged by
        // the adjudicator the processor runs, against the claim's committed root. Below the fence
        // the V2 kind does not exist and everything after this block is the fold as it was.
        if self.extras.offence_attribution_active {
            match kind {
                PalwOffenceKindV1::PanelFalseValid => {
                    return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                        evidence_id,
                        crate::palw_offence_v1::PalwOffenceVerifyError::SupersededOnThisNetwork.to_string(),
                    ));
                }
                PalwOffenceKindV1::PanelFalseValidV2 => {
                    return self.consume_false_valid_v2(ctx, accused, evidence_id, evidence, evidence_budget);
                }
                // ADR-0152 v3.1 J-4 (F1): the executor's own conviction, judged by the same reading.
                PalwOffenceKindV1::ExecutorRefuted => {
                    return self.consume_executor_refuted_v1(ctx, accused, evidence_id, evidence, evidence_budget);
                }
                // RFC-0002 Phase F: an IR executor's identity conviction. Below `palw_tir_v1` the lock
                // at the top of `apply_object` refused it already (an older build cannot decode it).
                PalwOffenceKindV1::TirIdentityMismatch => {
                    return self.consume_tir_identity_mismatch_v1(ctx, accused, evidence_id, evidence, evidence_budget);
                }
                PalwOffenceKindV1::ExecutorEquivocation | PalwOffenceKindV1::CourtExecutorGuilty => {}
                // Refused above; listed so a routing change that forgets them fails to compile.
                PalwOffenceKindV1::DaDefault | PalwOffenceKindV1::CourtConviction => {}
            }
        } else if matches!(
            kind,
            PalwOffenceKindV1::PanelFalseValidV2 | PalwOffenceKindV1::ExecutorRefuted | PalwOffenceKindV1::TirIdentityMismatch
        ) {
            return Err(PalwStateV2Error::ObjectiveOffenceDormant);
        }
        if matches!(kind, PalwOffenceKindV1::CourtExecutorGuilty) {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                evidence_id,
                "CourtExecutorGuilty is filed by CourtClosed, not as a standalone object".into(),
            ));
        }
        if crate::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != evidence_id {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                evidence_id,
                "evidence_id is not the digest of the evidence bytes".into(),
            ));
        }
        let offence_id = palw_offence_id_v1(kind, &accused.0, &evidence_id);
        if self.state.consumed_offences.contains_key(&offence_id) {
            return Ok(());
        }
        // ADR-0152 §3.6 Eq (D6, S-4): past `palw_rcore_plus` an equivocation takes `min(C₀, 3 · G_eq)`.
        if kind == PalwOffenceKindV1::ExecutorEquivocation && self.params.rcore_plus_active_at(ctx.daa_score) {
            return self.convict_equivocation_rcore_v1(ctx, accused, evidence_id, evidence, offence_id);
        }
        // ADR-0151: the execution a conviction forfeits the rights of. Set by the arm that knows one.
        let mut convicted_execution_root = Hash64::default();
        // 2026-09-24 DoS audit #8: the claim a `PanelFalseValid` names — the one whose `Final`, if
        // it has one, the conviction takes back. Set by that arm alone.
        let mut convicted_claim: Option<Hash64> = None;
        let amount = match kind {
            PalwOffenceKindV1::ExecutorEquivocation => self.state.bonds.get(&accused).map(|b| b.collateral).unwrap_or(0),
            PalwOffenceKindV1::PanelFalseValid => {
                let payload: PalwPanelFalseValidEvidenceV1 = borsh::from_slice(evidence).map_err(|_| {
                    PalwStateV2Error::ObjectiveOffenceRefused(offence_id, "PanelFalseValid evidence does not decode".into())
                })?;
                // V1 parity (ADR-0152 v3.1, addendum §4-bis.7): a tag the V1 payload could not
                // decode before the attribution contradictions existed is refused exactly as it was.
                if payload.contradiction.is_attribution_only_v1() {
                    return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                        offence_id,
                        "PanelFalseValid evidence does not decode".into(),
                    ));
                }
                if payload.accused_seat != accused.0 {
                    return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                        offence_id,
                        "the Valid receipt does not name this accused seat".into(),
                    ));
                }
                self.bind_panel_false_valid(ctx, &payload)?;
                let class_id = self
                    .state
                    .claims
                    .get(&payload.claim_id)
                    .map(|c| c.class_id)
                    .or_else(|| self.state.panel_liabilities.get(&payload.claim_id).map(|r| r.class_id));
                let ladder = class_id.and_then(|id| self.state.class_step_ladders.get(&id).copied()).unwrap_or(64);
                let (execution_root, artifact_root) = self.false_valid_execution_roots(&payload)?;
                convicted_execution_root = execution_root;
                convicted_claim = Some(payload.claim_id);
                // R6-CELL round 4: the coverage closure governs a claim accepted at or past its fence (None on shipped networks).
                // RFC-0006 consumer v2: the closure's mode for this claim also knows `palw_memory_routing_court_v2`'s mirror.
                // RFC06-4: with the claim's router binding past consumer v2 — the claim gate's and the court's predicate.
                let closure_mirrors = crate::palw_cell_coverage_closure_v1::PalwClosureMirrorsV1::of(self.params);
                let closure_inputs = crate::palw_cell_coverage_closure_v1::palw_blame_closure_inputs_v1(
                    &self.state,
                    &payload.claim_id,
                    closure_mirrors,
                );
                let cell_closure = closure_inputs.as_ref().map_or(
                    crate::palw_cell_coverage_closure_v1::PalwCellClosureModeV1::Off,
                    |(accepted, row)| {
                        crate::palw_cell_coverage_closure_v1::PalwCellClosureModeV1::for_blame_path(
                            closure_mirrors,
                            *accepted,
                            row.as_ref(),
                            true,
                        )
                    },
                );
                crate::palw_offence_v1::palw_panel_contradiction_convicts_execution_closure_v1(
                    &payload.contradiction,
                    execution_root,
                    artifact_root,
                    ladder,
                    cell_closure,
                )
                .map_err(|e| PalwStateV2Error::ObjectiveOffenceRefused(offence_id, e.to_string()))?;
                if let Some(lock) = self.state.slashable_locks.get(&(accused, payload.claim_id)).copied() {
                    if self.false_valid_lock_slashable_v1(&payload.claim_id, &lock, ctx.daa_score) {
                        self.write_slashable_lock((accused, payload.claim_id), None)?;
                        u64::try_from(lock.amount.min(u128::from(u64::MAX))).unwrap_or(u64::MAX)
                    } else {
                        // Lane V02 (past `palw_final_lock_life`): the lock expired and its collateral
                        // already backs new work — nothing from it, the row left to prune with its
                        // record; the conviction still records and reverses the `Final` below.
                        0
                    }
                } else if self
                    .state
                    .panel_liabilities
                    .get(&payload.claim_id)
                    .is_some_and(|row| row.valid_signers.iter().any(|(seat, _)| *seat == accused.0))
                {
                    0
                } else if self.extras.audit_2026_09_23_active {
                    // **2026-09-24 DoS audit review (#12's paired change): a seat the chain never
                    // relied on is not convicted at all.** Below the fence this arm slashed the
                    // accused's WHOLE collateral: a seat that signed `Valid` and gossiped the
                    // receipt, but that no licence carried (the fourth and fifth `Valid` of a 3-of-5
                    // quorum, or any signer while the claim was still `PanelBound`), holds no lock
                    // and is on no liability row — and lost 939,063 MSK where a carried signer of
                    // the same claim lost its 1,174 MSK lock (`review_economic_whole_collateral_branch`).
                    // Liability follows what the licence rested on: the lock is the price a
                    // `Valid` was charged when it counted, and a `Valid` that counted for nothing
                    // bought nothing a conviction could take back. Refused rather than priced at
                    // some lock it never posted, so the object is dropped with the block standing
                    // and the Final's reversal (#8) still runs through any carried signer, whose
                    // conviction is the one the evidence is about.
                    //
                    // **#12 (a) relies on this arm.** `sweep_panel_obligations` prunes a lock and
                    // its liability row together once both are past their evidence horizon, and a
                    // pruned seat is exactly a seat that falls through to here: below this refusal
                    // every pruned obligation would have become a conviction of the seat's WHOLE
                    // collateral, the opposite of forgetting it. The pruning runs only past the
                    // same fence, so there is no network on which the rows are pruned and this arm
                    // still slashes; `a_pruned_obligation_is_refused_never_the_whole_collateral`
                    // pins the pair.
                    return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                        offence_id,
                        "the accused holds no Valid lock on this claim and no liability row lists it (no locked Valid, \
                         or the obligation is past its evidence horizon)"
                            .into(),
                    ));
                } else {
                    self.state.bonds.get(&accused).map(|b| b.collateral).unwrap_or(0)
                }
            }
            PalwOffenceKindV1::CourtExecutorGuilty => 0,
            // Routed above on every network: consumed by `consume_false_valid_v2` /
            // `consume_executor_refuted_v1` past the fence, refused as dormant below it. An `Err`,
            // never `unreachable!`, so a routing change that forgot this arm refuses the object
            // instead of panicking the node.
            PalwOffenceKindV1::PanelFalseValidV2 | PalwOffenceKindV1::ExecutorRefuted | PalwOffenceKindV1::TirIdentityMismatch => {
                return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                    offence_id,
                    crate::palw_offence_v1::PalwOffenceVerifyError::AttributionDormant.to_string(),
                ));
            }
            // Refused at the top of this function on every network; an `Err` here too, never
            // `unreachable!`, for the reason the arm above gives.
            PalwOffenceKindV1::DaDefault | PalwOffenceKindV1::CourtConviction => {
                return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                    offence_id,
                    crate::palw_offence_v1::PalwOffenceVerifyError::KindNotFileable(kind.not_fileable_reason_v1()).to_string(),
                ));
            }
        };
        self.slash_bond(accused, amount as u128)?;
        // ADR-0151: recorded only where the bundle is armed, so a dormant network's
        // `consumed_offences` rows — and therefore its state root — are what they were.
        let recorded_root = if self.extras.economic_safety.is_some() { convicted_execution_root } else { Hash64::default() };
        self.write_consumed_offence(
            offence_id,
            Some(PalwConsumedOffenceV1 {
                kind,
                accused: accused.0,
                amount,
                accepted_daa: ctx.daa_score,
                execution_root: recorded_root,
                // ADR-0152 v22 skeleton: S-4's conviction funnel records these past `palw_rcore_plus`.
                collected: 0,
                claim_id: Hash64::default(),
            }),
        )?;
        // **2026-09-24 DoS audit #7 and #8: what a conviction takes back.** Both behind the audit
        // fence (testnet-12 only); below it a conviction slashes and records, exactly as before.
        if self.extras.audit_2026_09_23_active
            && let Some(claim_id) = convicted_claim
        {
            // #7: the tickets and permits the convicted work already holds in a minted schedule or
            // a pending snapshot. Reads the root the forfeiture set will read, so the two stages
            // cannot disagree about which work was convicted (zero where ADR-0151 is dormant, and
            // then this is a no-op like the rest of the forfeiture).
            self.forfeit_minted_round_rights(&recorded_root)?;
            // #8: the named claim's own `Final` — its weight, its probe pass, its usage.
            self.reverse_convicted_final(ctx, claim_id, PalwVoidReasonV2::CourtFraud)?;
        }
        Ok(())
    }

    /// **ADR-0152 §3.6 Eq (D6, S-4; testnet-12 only): an `ExecutorEquivocation` past
    /// `palw_rcore_plus` takes `min(C₀, 3 · G_eq)`, not the whole collateral** — `G_eq`
    /// ([`palw_eq_cap_basis_v1`]) of the class the certificate's `job_context.shape_profile_id`
    /// names, else of the base class. No status change, no tombstone (B-4); the record's `claim_id`
    /// and root are zero (a key signed two roots — no claim, no execution); `collected` is the locked
    /// debit (R-2); the reporter reward is commit–reveal on the consumed `evidence_id` (S-SPEC P10).
    pub(super) fn convict_equivocation_rcore_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        accused: PalwBondKeyV2,
        evidence_id: Hash64,
        evidence: &[u8],
        offence_id: Hash64,
    ) -> Result<(), PalwStateV2Error> {
        let now = ctx.daa_score;
        let class_id = borsh::from_slice::<crate::palw_carriage::PalwEquivocationCarriageV1>(evidence)
            .map(|carriage| carriage.certificate.job_context.shape_profile_id)
            .unwrap_or_default();
        let conv = self.open_conviction_v1(&[accused], now);
        let nominal = palw_rcore_eq_cap_v1(conv.c0(&accused), self.read().eq_cap_basis_v1(ctx, &class_id));
        self.slash_bond(accused, nominal)?;
        self.close_conviction_v1(
            &conv,
            offence_id,
            crate::palw_offence_v1::PalwOffenceKindV1::ExecutorEquivocation,
            accused,
            nominal,
            Hash64::default(),
            Hash64::default(),
            &[accused],
            &[(accused, crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1::Equivocation)],
        )?;
        self.open_reporter_reward(now, offence_id, 0, PalwConvictionBasisV1::CheckedEvidence { evidence_id })?;
        Ok(())
    }

    /// **ADR-0152 v2 F2: a false `Valid`, judged once and charged once per (seat, claim).**
    ///
    /// The fold's half of `PanelFalseValidV2`. It runs the SAME adjudicator the processor runs
    /// ([`crate::palw_offence_attribution_v1::palw_check_panel_false_valid_v2`]), minus only the
    /// signature — so a node replaying the chain, a sync walk and the live path reach one verdict
    /// from state alone — and then charges what the licence rested on:
    ///
    /// 1. **One offence per (seat, claim).** The ledger id is
    ///    [`crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2`], which reads neither
    ///    the contradiction nor the bytes: a second proof of the same false `Valid`, or the same one
    ///    re-wrapped, is a no-op, never a second slash.
    /// 2. **The lock, and only the lock.** The seat's `Valid` lock on the claim is taken and removed
    ///    FIRST — before any void below, so `persist_panel_liability` does not list the convicted
    ///    seat again; a seat that holds none but that the liability row lists is convicted for 0 (its
    ///    lock was already spent or pruned with the row's horizon); any other seat is refused, as
    ///    the audit branch of the V1 fold refuses it. The consumed row records what the bond
    ///    actually paid — `slash_bond` clamps a debit at the collateral, and the S stage reads these
    ///    rows as debits (F2 review, F-4).
    /// 3. **The execution, when the fault proves it false.** The consumed row records the claim's
    ///    root only for an execution-proving contradiction (and only where ADR-0151 is armed): a
    ///    `CourtFraud` void names a claim, not a root, and a borrowed root recorded here would forfeit
    ///    the honest lender's rights (a `ProducerWithholding` void is refused past the fence until F3,
    ///    F2 review F-2). For an execution-proving fault the claim is then
    ///    acted on by phase — voided `CourtFraud` with its executor slashed before `Final` (which
    ///    closes the hole where a pre-`Final` conviction left the claim to finalise), the `Final`
    ///    reversed after it, and the liability row marked either way.
    pub(super) fn consume_false_valid_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        accused: PalwBondKeyV2,
        evidence_id: Hash64,
        evidence: &[u8],
        evidence_budget: crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2;
        use crate::palw_offence_v1::{PalwConsumedOffenceV1, PalwOffenceKindV1};
        if crate::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != evidence_id {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                evidence_id,
                "evidence_id is not the digest of the evidence bytes".into(),
            ));
        }
        // ADR-0160 F-B: a batched receipt (`Windowed`) convicts only past the batch fence, as the
        // gate refuses it below (the form did not decode before the fence existed). Refused BEFORE
        // the heavy budget is charged — junk that fails before the recompute costs the budget nothing,
        // and `0e8ec984e`, which cannot decode the form, charges nothing for it (the lane-verify
        // review's heavy-slot finding; the charge reads the same fence as its second lock).
        if !self.params.capacity_batch_active_at(ctx.daa_score)
            && crate::palw_offence_attribution_v1::palw_false_valid_evidence_is_windowed_v1(evidence)
        {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                evidence_id,
                "a Windowed receipt below palw_capacity_batch_licence (ADR-0160 F-B)".into(),
            ));
        }
        self.charge_heavy_prompt_ids_v1(
            ctx.daa_score,
            &accused,
            PalwOffenceKindV1::PanelFalseValidV2,
            evidence_id,
            evidence,
            evidence_budget,
        )?;
        // The reporter slot is F7's; until its fence arms it the adjudicator refuses a filled one.
        // RFC06-4: both mirrors — past consumer v2 the claim is judged with its router binding, as the processor judges it.
        let finding = crate::palw_offence_attribution_v1::palw_check_panel_false_valid_blame_v2(
            &self.state,
            &accused,
            evidence,
            self.params.fp_decode_rules_at(ctx.daa_score),
            false,
            self.identity_rules_v1(ctx.daa_score),
            None,
            self.session_rule_v1(ctx.daa_score),
            evidence_budget,
            crate::palw_cell_coverage_closure_v1::PalwClosureMirrorsV1::of(self.params),
            true,
        )
        .map_err(|e| PalwStateV2Error::ObjectiveOffenceRefused(evidence_id, e.to_string()))?;
        let claim_id = finding.target.claim_id;
        let offence_id = palw_false_valid_offence_id_v2(&accused.0, &claim_id);
        if self.state.consumed_offences.contains_key(&offence_id) {
            return Ok(());
        }
        // ADR-0152 §3.6 (S-4): past `palw_rcore_plus`, the conviction funnel.
        if self.params.rcore_plus_active_at(ctx.daa_score) {
            let offence = crate::palw_aggregate_liability_v1::palw_evidence_contradiction_offence_v1(
                PalwOffenceKindV1::PanelFalseValidV2,
                evidence,
            );
            return self.convict_false_valid_rcore_v1(ctx, accused, evidence_id, &finding, offence_id, offence);
        }
        let amount = if let Some(lock) = self.state.slashable_locks.get(&(accused, claim_id)).copied() {
            if self.false_valid_lock_slashable_v1(&claim_id, &lock, ctx.daa_score) {
                self.write_slashable_lock((accused, claim_id), None)?;
                u64::try_from(lock.amount.min(u128::from(u64::MAX))).unwrap_or(u64::MAX)
            } else {
                // Lane V02: an expired lock past `palw_final_lock_life` gives nothing (in practice
                // unreachable — the fence requires `palw_rcore_plus`, whose funnel is taken above).
                0
            }
        } else if self
            .state
            .panel_liabilities
            .get(&claim_id)
            .is_some_and(|row| row.valid_signers.iter().any(|(seat, _)| *seat == accused.0))
        {
            0
        } else {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                offence_id,
                "the accused holds no Valid lock on this claim and no liability row lists it (no locked Valid, or the \
                 obligation is past its evidence horizon)"
                    .into(),
            ));
        };
        // What the bond can pay: `slash_bond` clamps at the collateral, and the row says what it took.
        let debit = amount.min(self.state.bonds.get(&accused).ok_or(PalwStateV2Error::MissingBond(accused))?.collateral);
        self.slash_bond(accused, debit as u128)?;
        let recorded_root = self.recorded_forfeit_root_v1(finding.forfeit, &finding.target);
        self.write_consumed_offence(
            offence_id,
            Some(PalwConsumedOffenceV1 {
                kind: PalwOffenceKindV1::PanelFalseValidV2,
                accused: accused.0,
                amount: debit,
                accepted_daa: ctx.daa_score,
                execution_root: recorded_root,
                // ADR-0152 v3.1 R-2 (the Phase 1–2 review's L-2): what the conviction took and the
                // claim it is about. `amount` stays the debit until S-4 settles it to the nominal tier.
                collected: debit,
                claim_id,
            }),
        )?;
        if finding.acts_on_claim {
            self.act_on_convicted_claim_v1(ctx, claim_id, None)?;
            self.forfeit_convicted_rights_v1(finding.forfeit, claim_id, &recorded_root)?;
        }
        Ok(())
    }

    /// **ADR-0152 §3.6 (S-4): a `PanelFalseValidV2` conviction past `palw_rcore_plus`, through the
    /// conviction funnel** — opened on the seat and the claim's producer BEFORE the seat's lock is
    /// taken (R-2: v6 reads the lock), then:
    ///
    /// * **S4 on the seat** — its lock, taken first (so a void's liability row does not list it
    ///   again), plus `min(25% · C₀, 3 G)`; once per (seat, claim) — this ledger key — and never
    ///   escalating. A seat the liability row lists but that holds no lock is convicted for 0, as
    ///   before: the chain never relied on a lock it does not hold (the audit's #12), so no action.
    /// * **the claim**, for a finding that acts on it (5, 6, 8–13): live → voided `CourtFraud`, the
    ///   producer's forfeit plus S2's `min(10% · C₀, 3 G)`; `Final` or retired → reversed / marked, then
    ///   S3 through the burn hook, or U3's free-prompt tier.
    /// * **the record, LAST** — `amount` the legs' nominal sum, `collected` the seat's and the
    ///   producer's debits where their exit was shut at the opening, `claim_id`; the rights forfeited
    ///   by the finding's route; and the reporter reward on the consumed `evidence_id`
    ///   ([`PalwConvictionBasisV1::CheckedEvidence`]) with R-1's `X = min(lock, G_res / basis_k)` once
    ///   the claim is past its live pre-`Final` phases. A finding that does not act on the claim —
    ///   `ProducerWithholding` (2), `CourtFraud` (4) — restates a DA default or a court verdict: S4
    ///   still, but no separate reward (R-3).
    pub(super) fn convict_false_valid_rcore_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        accused: PalwBondKeyV2,
        evidence_id: Hash64,
        finding: &crate::palw_offence_attribution_v1::PalwFalseValidFindingV1,
        offence_id: Hash64,
        offence: crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_offence_v1::PalwOffenceKindV1;
        let now = ctx.daa_score;
        let claim_id = finding.target.claim_id;
        let producer = finding.target.executor_bond;
        let row_lock = self.state.slashable_locks.get(&(accused, claim_id)).copied();
        if row_lock.is_none()
            && !self
                .state
                .panel_liabilities
                .get(&claim_id)
                .is_some_and(|row| row.valid_signers.iter().any(|(seat, _)| *seat == accused.0))
        {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                offence_id,
                "the accused holds no Valid lock on this claim and no liability row lists it (no locked Valid, or the \
                 obligation is past its evidence horizon)"
                    .into(),
            ));
        }
        // Lane V02 (past `palw_final_lock_life`, the user's 2026-09-26 decision): a lock the fold no
        // longer counts as committed has released its collateral to new work, so the seat is
        // convicted as one the row lists without a lock — no S4, no reporter extraction, the row left
        // to prune with its record — and the claim's legs below (the `Final` reversed, `E` burned
        // with S3) run unchanged. Read on the pre-state; `row_lock` itself below the fence.
        let lock = row_lock.filter(|lock| self.false_valid_lock_slashable_v1(&claim_id, lock, now));
        // **G-13 (lane g14-close-legacy): no conviction of a relied-on Valid seat that collects nothing.** On a claim accepted past
        // `palw_legacy_public_filer_v1` every relied-on `Valid` lock lives to the claim's liability horizon (`valid_lock_life_v1`), so a
        // seat the chain relied on — it holds a lock, or the liability row was built from locks (`locked_sompi > 0`) and lists it — must
        // still hold a SLASHABLE lock here. One that does not is refused, as the TIR frame refuses a default whose S4 lock is gone,
        // rather than recorded as a conviction that took nothing. Every other claim — and the fallback listing of a panel no licence
        // locked — keeps the old rule (convicted for 0). The one predicate: `palw_g13_relied_lock_lost_v1`.
        if palw_g13_relied_lock_lost_v1(&self.state, self.params, &claim_id, &accused, now, self.second_clock_depth(now)) {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                offence_id,
                "G-13: a relied-on Valid seat of a claim accepted past palw_legacy_public_filer_v1 lost its slashable lock before the \
                 claim's liability horizon (no conviction that collects nothing)"
                    .into(),
            ));
        }
        let conv = self.open_conviction_v1(&[accused, producer], now);
        let gains = self.claim_g_v1(&claim_id).unwrap_or_default();
        let live = self.claim_is_live_before_final_v1(&claim_id);
        let fp_first = self.post_final_fp_g_v1(&claim_id);
        let mut nominal = 0u128;
        // **S4 — and the ADR wording note on its action (the S-4 review, deviation 3): the action
        // `min(25% · C₀, 3 G)` is charged only with a lock the chain still holds for this (seat,
        // claim).** ADR §3.6 writes S4 as "lock + action", which presupposes the lock: the lock is what
        // the licence rested on (L-1), and a seat the liability row lists without one — the row's
        // fallback listing of a panel no licence locked, or a signer the chain never relied on — is
        // convicted for 0 as before (the audit's #12: "a seat the chain never relied on is not
        // convicted at all"), never charged an action on a `Valid` that bought nothing.
        if let Some(lock) = lock {
            self.write_slashable_lock((accused, claim_id), None)?;
            let s4 = lock.amount.saturating_add(palw_rcore_s3s4_action_v1(conv.c0(&accused), gains.g()));
            self.slash_bond(accused, s4)?;
            nominal = s4;
        }
        let recorded_root = self.recorded_forfeit_root_v1(finding.forfeit, &finding.target);
        if finding.acts_on_claim {
            nominal = nominal.saturating_add(self.act_on_convicted_claim_v1(ctx, claim_id, Some(&conv))?);
            if !live {
                let tier = self.post_final_producer_leg_v1(
                    &conv,
                    claim_id,
                    producer,
                    offence_id,
                    PalwOffenceKindV1::PanelFalseValidV2,
                    gains.g(),
                    fp_first,
                )?;
                nominal = nominal.saturating_add(tier);
            }
        }
        // **The kind-3 record covers the WHOLE conviction, the producer's leg included (the S-4
        // review, deviation 1):** its `amount` is the seat's S4 PLUS the producer's S2 (a live claim's
        // `CourtFraud` void) or S3 / U3 (after `Final`), and its `collected` the seat's and the
        // producer's debits where each exit was shut at the opening — R-1's "full conviction" example
        // (32,378.89 MSK on the floor) sums the signers' S4 with the producer's S3 the same way. The
        // record's `accused` is the seat; the producer's leg rides on the conviction that voided or
        // reversed the claim (the first to bind it), and `collected ≤ amount` always.
        // ADR-0160 (AS-3): the seat by the contradiction's class, and — for a finding that acts on the
        // claim, which proves it false — its producer by the same class.
        let mut liable = vec![(accused, offence)];
        if finding.acts_on_claim {
            liable.push((producer, offence));
        }
        self.close_conviction_v1(
            &conv,
            offence_id,
            PalwOffenceKindV1::PanelFalseValidV2,
            accused,
            nominal,
            recorded_root,
            claim_id,
            &[accused, producer],
            &liable,
        )?;
        if finding.acts_on_claim {
            self.forfeit_convicted_rights_v1(finding.forfeit, claim_id, &recorded_root)?;
            let extracted = match lock {
                Some(lock) if !live => palw_reporter_reward_extracted_v1(lock.amount, gains.g_res, gains.basis_k),
                _ => 0,
            };
            self.open_reporter_reward(now, offence_id, extracted, PalwConvictionBasisV1::CheckedEvidence { evidence_id })?;
        }
        Ok(())
    }

    /// **ADR-0152 v3.1 J-5 / SPEC §4.6: is a data-availability binding an answer about THIS claim's
    /// job?** Past `offence_attribution_active`, a binding that reproduces the claim's root but fails
    /// the identity rule (J2, J1, J3, J5a, J4 — J5b's prompt recompute is left to the contradictions)
    /// answers another job or class: it is not an answer, and an accusation carrying one is not an
    /// accusation about this claim (its filer sends `ExecutorRefuted` instead). `None` — the old
    /// behaviour — below the fence and wherever the rule cannot be asked (no identity recorded, a
    /// binding that is not the claim's, which the unit's own check refuses anyway).
    pub(super) fn da_binding_answers_another_job_v1(
        &self,
        now_daa: u64,
        claim_id: &Hash64,
        binding: &crate::palw_step_leg::PalwStepBindingV2,
    ) -> Option<String> {
        if !self.extras.offence_attribution_active {
            return None;
        }
        let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&self.state, claim_id)?;
        match crate::palw_offence_attribution_v1::palw_binding_identity_fault_v1(
            &target,
            binding,
            self.identity_rules_v1(now_daa),
            false,
        ) {
            Ok(Some(fault)) => Some(format!("the binding answers another job or class ({})", fault.code())),
            Ok(None) | Err(_) => None,
        }
    }

    /// [`Self::da_binding_answers_another_job_v1`] over an IR binding (RFC-0002 Phase F): the same
    /// identity rule, J5 derived from the IR class's facts
    /// (`palw_offence_attribution_v1::palw_tir_binding_identity_fault_v1`).
    pub(super) fn da_tir_binding_answers_another_job_v1(
        &self,
        now_daa: u64,
        claim_id: &Hash64,
        binding: &crate::palw_tir_step_v1::PalwTirStepBindingV1,
        ladder: u64,
    ) -> Option<String> {
        if !self.extras.offence_attribution_active {
            return None;
        }
        let target = crate::palw_offence_attribution_v1::palw_offence_target_v1(&self.state, claim_id)?;
        match crate::palw_offence_attribution_v1::palw_tir_binding_identity_fault_v1(
            &target,
            binding,
            self.identity_rules_v1(now_daa),
            false,
            ladder,
        ) {
            Ok(Some(fault)) => Some(format!("the IR binding answers another job or class ({})", fault.code())),
            Ok(None) | Err(_) => None,
        }
    }

    /// **The ladder an IR claim's data availability is checked at** (RFC-0002 Phase F): below
    /// `palw_tir_fence2` the release's `PALW_STEP_LEG_MAX_LEAVES` (2^22), byte for byte; past it the
    /// claim's class's own ladder (`class_step_ladder_v1` over the court's step ladder the held regime
    /// rides — the ladder admission held the class's every job to), never below the release's. A
    /// binding of an execution past 2^22 leaves — every real-size class's job — then verifies, where
    /// below the fence every answer its producer could give was refused and it defaulted.
    pub(super) fn tir_da_ladder_v1(&self, class_id: &Hash64, daa: u64) -> u64 {
        let release = crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
        if !self.params.tir_fence2_active_at(daa) {
            return release;
        }
        self.extras.held_context_ladder.map_or(release, |network| self.state.class_step_ladder_v1(class_id, network).max(release))
    }

    /// The identity rules the attribution adjudicators read: the network's prompt-id form and the
    /// base class (F1's J5 derives the base class's canonical job).
    /// **The heavy prompt budget, charged before the adjudicator computes** (addendum §4-bis.3): a
    /// `PromptNotAnchored { Whole }` that will reach the recompute
    /// ([`crate::palw_offence_attribution_v1::palw_offence_heavy_prompt_charge_v1`], the acceptance
    /// walk's own reading) charges its claim's prefill once per block against
    /// [`crate::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1`] — one 2M check. Refused
    /// (`HeavyBudgetExhausted`) without computing past it; a claim already charged, and everything
    /// else, is free here.
    pub(super) fn charge_heavy_prompt_ids_v1(
        &mut self,
        now_daa: u64,
        accused: &PalwBondKeyV2,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
        evidence_id: Hash64,
        evidence: &[u8],
        evidence_budget: crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1,
    ) -> Result<(), PalwStateV2Error> {
        let Some(charge) = crate::palw_offence_attribution_v1::palw_offence_heavy_prompt_charge_at_v2_with_budget_v1(
            &self.state,
            accused,
            kind,
            evidence,
            self.identity_rules_v1(now_daa),
            // ADR-0160 F-B: below the batch fence a `Windowed` kind-3 is the base's undecodable junk.
            self.params.capacity_batch_active_at(now_daa),
            crate::palw_offence_attribution_v1::PalwSessionRuleV1::at(self.params.legacy_public_filer_active_at(now_daa)),
            evidence_budget,
        ) else {
            return Ok(());
        };
        if self.heavy_prompt_claims.contains(&charge.claim_id) {
            return Ok(());
        }
        let heavy = charge.prompt_ids;
        let charged = self.heavy_prompt_ids_charged.saturating_add(heavy);
        if charged > crate::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1 {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                evidence_id,
                crate::palw_offence_v1::PalwOffenceVerifyError::HeavyBudgetExhausted.to_string(),
            ));
        }
        self.heavy_prompt_ids_charged = charged;
        self.heavy_prompt_claims.insert(charge.claim_id);
        Ok(())
    }

    /// **RFC-0014 §7.4 (lane LG14-A): the session rule a direct proof is judged under at this block** — `DirectProofFirst` past
    /// `palw_legacy_public_filer_v1` (the bundle's mirror), int-12's `RefusedUnderSession` below it. The processor's gate reads the
    /// same fence (`palw_session_rule_at`).
    pub(super) fn session_rule_v1(&self, now_daa: u64) -> crate::palw_offence_attribution_v1::PalwSessionRuleV1 {
        crate::palw_offence_attribution_v1::PalwSessionRuleV1::at(self.params.legacy_public_filer_active_at(now_daa))
    }

    pub(super) fn identity_rules_v1(&self, now_daa: u64) -> crate::palw_offence_attribution_v1::PalwIdentityRulesV1 {
        crate::palw_offence_attribution_v1::PalwIdentityRulesV1 {
            prompt_ids_form: self.extras.prompt_ids_form_v1(),
            base_class_id: self.params.base_class_id(),
            da_signer_liability: palw_da_signer_liability_armed_v1(self.params, self.extras.seat_da_answer_landed, now_daa),
        }
    }

    /// **The root a conviction's ledger row records**: the target's execution root for a fault
    /// that proves the EXECUTION false, where ADR-0151 is armed — the forfeiture set is a function
    /// of it — and zero otherwise: a named void proves nothing about a root, and a claim-proving
    /// fault (F1: 9, 10) proves the CLAIM answers the wrong job or output, not that the root is
    /// false — recording it would forfeit an honest lender's rights on the same root (SPEC §4.5).
    pub(super) fn recorded_forfeit_root_v1(
        &self,
        forfeit: crate::palw_offence_attribution_v1::PalwForfeitScopeV1,
        target: &crate::palw_offence_attribution_v1::PalwOffenceTargetV1,
    ) -> Hash64 {
        match forfeit {
            crate::palw_offence_attribution_v1::PalwForfeitScopeV1::ByRoot if self.extras.economic_safety.is_some() => {
                target.execution_root
            }
            _ => Hash64::default(),
        }
    }

    /// **A claim proven false, acted on by its phase** (SPEC §3.5 step 7, §4.4): voided `CourtFraud`
    /// with its executor slashed before `Final` (reservation, escrow and rights — a live claim's
    /// data-availability session ends with it, its accuser's reservation given back where the
    /// claim is written), its `Final` reversed after, and its liability row marked either way. A
    /// claim already voided keeps the void that happened first; one that has retired leaves only its
    /// row to mark.
    ///
    /// Inside a conviction (`conv`, S-4) the live claim's void is charged at the funnel's `C₀` — S2's
    /// action past `palw_rcore_plus` ([`Self::void_and_slash_at`]); returns that void's nominal charge
    /// (0 for a claim already `Final`, voided or retired: their tiers are the caller's).
    pub(super) fn act_on_convicted_claim_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        conv: Option<&PalwConvictionV1>,
    ) -> Result<u128, PalwStateV2Error> {
        self.act_on_convicted_claim_as_v1(ctx, claim_id, conv, PalwVoidReasonV2::CourtFraud)
    }

    /// [`Self::act_on_convicted_claim_v1`] recording `reason` — `CourtFraud` for a proof of the
    /// execution, [`PalwVoidReasonV2::CourtHeldVerdict`] for a dissection's verdict (charged the
    /// same; no kind-3 basis).
    pub(super) fn act_on_convicted_claim_as_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        claim_id: Hash64,
        conv: Option<&PalwConvictionV1>,
        reason: PalwVoidReasonV2,
    ) -> Result<u128, PalwStateV2Error> {
        // ADR-0152 N13 / DA-6 (M3): a conviction closes every open DA session on the claim (exposure
        // returned) and refunds its refuted exposure — for a live claim through the `CourtFraud` void
        // below (`void_claim`), for a `Final`, voided or retired one here.
        if self.params.rcore_plus_active_at(ctx.daa_score)
            && self.state.claims.get(&claim_id).is_none_or(|claim| claim.phase.is_terminal())
        {
            self.da_release_all_v1(claim_id, true, ctx.daa_score)?;
        }
        match self.state.claims.get(&claim_id).cloned() {
            Some(claim) => match claim.phase {
                PalwClaimPhaseV2::Provisional
                | PalwClaimPhaseV2::PanelBound { .. }
                | PalwClaimPhaseV2::ReceiptLicensed { .. }
                | PalwClaimPhaseV2::DefaultDisputed { .. } => {
                    return self.void_and_slash_at(claim_id, &claim, ctx.daa_score, reason, conv);
                }
                PalwClaimPhaseV2::Final { .. } => {
                    self.reverse_convicted_final(ctx, claim_id, reason)?;
                    self.mark_liability_convicted(claim_id, ctx.daa_score, reason)?;
                }
                PalwClaimPhaseV2::Voided { .. } => self.mark_liability_convicted(claim_id, ctx.daa_score, reason)?,
            },
            None => self.mark_liability_convicted(claim_id, ctx.daa_score, reason)?,
        }
        Ok(0)
    }

    /// **What the convicted work already holds in a minted schedule or a pending snapshot** (the
    /// DoS audit's #7): by ROOT for an execution-proving fault (the root the ledger row now
    /// carries; zero is a no-op), by CLAIM for a claim-proving one (F1, SPEC §4.5) — so a borrowed
    /// root never removes the lender's rights.
    pub(super) fn forfeit_convicted_rights_v1(
        &mut self,
        forfeit: crate::palw_offence_attribution_v1::PalwForfeitScopeV1,
        claim_id: Hash64,
        recorded_root: &Hash64,
    ) -> Result<(), PalwStateV2Error> {
        match forfeit {
            crate::palw_offence_attribution_v1::PalwForfeitScopeV1::ByRoot => self.forfeit_minted_round_rights(recorded_root)?,
            crate::palw_offence_attribution_v1::PalwForfeitScopeV1::ByClaim => self.forfeit_minted_round_rights_of_claim(&claim_id)?,
            crate::palw_offence_attribution_v1::PalwForfeitScopeV1::None => {}
        }
        Ok(())
    }

    /// **ADR-0152 v3.1 J-4 (F1): the executor refuted, judged once and charged once per claim.**
    ///
    /// The fold's half of `ExecutorRefuted` (kind 4): the SAME adjudicator the processor runs
    /// ([`crate::palw_offence_attribution_v1::palw_check_executor_refuted_v1`]), then:
    ///
    /// 1. **One offence per claim** ([`crate::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1`]):
    ///    a second proof of the same claim's falsehood is a no-op.
    /// 2. **The claim, by phase** ([`Self::act_on_convicted_claim_v1`]): live → `void_and_slash`
    ///    (`CourtFraud` — a proven refutation), the executor charged its reservation, escrow and
    ///    rights; `Final` → reversed (the executor's post-`Final` charge is R-core's S3, not this
    ///    stage's); voided or retired → the row marked.
    /// 3. **The ledger row** records the debit the executor actually paid (its collateral before
    ///    less after), and the root only for an execution-proving fault where ADR-0151 is armed.
    /// 4. **The rights**, by root or by claim ([`Self::forfeit_convicted_rights_v1`]).
    ///
    /// The `Valid` signers follow through kind 3 — `CourtFraud { voided_daa }` naming this void, or
    /// the same contradiction.
    pub(super) fn consume_executor_refuted_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        accused: PalwBondKeyV2,
        evidence_id: Hash64,
        evidence: &[u8],
        evidence_budget: crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1;
        use crate::palw_offence_v1::{PalwConsumedOffenceV1, PalwOffenceKindV1};
        if crate::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != evidence_id {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                evidence_id,
                "evidence_id is not the digest of the evidence bytes".into(),
            ));
        }
        self.charge_heavy_prompt_ids_v1(
            ctx.daa_score,
            &accused,
            PalwOffenceKindV1::ExecutorRefuted,
            evidence_id,
            evidence,
            evidence_budget,
        )?;
        // RFC06-4: past consumer v2, kind 3's closure predicate (below it, byte for byte the old call).
        let finding = crate::palw_offence_attribution_v1::palw_check_executor_refuted_blame_v2(
            &self.state,
            &accused,
            evidence,
            self.params.fp_decode_rules_at(ctx.daa_score),
            false,
            self.identity_rules_v1(ctx.daa_score),
            self.session_rule_v1(ctx.daa_score),
            evidence_budget,
            crate::palw_cell_coverage_closure_v1::PalwClosureMirrorsV1::of(self.params),
        )
        .map_err(|e| PalwStateV2Error::ObjectiveOffenceRefused(evidence_id, e.to_string()))?;
        let claim_id = finding.target.claim_id;
        let offence_id = palw_executor_refuted_offence_id_v1(&accused.0, &claim_id);
        if self.state.consumed_offences.contains_key(&offence_id) {
            return Ok(());
        }
        // ADR-0152 §3.6 (S-4): past `palw_rcore_plus`, the conviction funnel — opened on the executor;
        // live → voided `CourtFraud` (forfeit + S2's action), `Final` or retired → reversed / marked
        // and S3 (the burn hook) or U3; the record last, `amount` nominal and `collected` the locked
        // debit; the reward on the consumed evidence (`X` = 0: the producer's own tiers).
        if self.params.rcore_plus_active_at(ctx.daa_score) {
            let now = ctx.daa_score;
            let conv = self.open_conviction_v1(&[accused], now);
            let g = self.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);
            let live = self.claim_is_live_before_final_v1(&claim_id);
            let fp_first = self.post_final_fp_g_v1(&claim_id);
            let mut nominal = self.act_on_convicted_claim_v1(ctx, claim_id, Some(&conv))?;
            if !live {
                let tier = self.post_final_producer_leg_v1(
                    &conv,
                    claim_id,
                    accused,
                    offence_id,
                    PalwOffenceKindV1::ExecutorRefuted,
                    g,
                    fp_first,
                )?;
                nominal = nominal.saturating_add(tier);
            }
            let recorded_root = self.recorded_forfeit_root_v1(finding.forfeit, &finding.target);
            let offence = crate::palw_aggregate_liability_v1::palw_evidence_contradiction_offence_v1(
                PalwOffenceKindV1::ExecutorRefuted,
                evidence,
            );
            self.close_conviction_v1(
                &conv,
                offence_id,
                PalwOffenceKindV1::ExecutorRefuted,
                accused,
                nominal,
                recorded_root,
                claim_id,
                &[accused],
                &[(accused, offence)],
            )?;
            self.forfeit_convicted_rights_v1(finding.forfeit, claim_id, &recorded_root)?;
            self.open_reporter_reward(now, offence_id, 0, PalwConvictionBasisV1::CheckedEvidence { evidence_id })?;
            return Ok(());
        }
        let before = self.state.bonds.get(&accused).map(|bond| bond.collateral).unwrap_or(0);
        self.act_on_convicted_claim_v1(ctx, claim_id, None)?;
        let debit = before.saturating_sub(self.state.bonds.get(&accused).map(|bond| bond.collateral).unwrap_or(0));
        let recorded_root = self.recorded_forfeit_root_v1(finding.forfeit, &finding.target);
        self.write_consumed_offence(
            offence_id,
            Some(PalwConsumedOffenceV1 {
                kind: PalwOffenceKindV1::ExecutorRefuted,
                accused: accused.0,
                amount: debit,
                accepted_daa: ctx.daa_score,
                execution_root: recorded_root,
                // ADR-0152 v3.1 R-2 (the Phase 1–2 review's L-2): what the conviction took and the
                // claim it is about. `amount` stays the debit until S-4 settles it to the nominal tier.
                collected: debit,
                claim_id,
            }),
        )?;
        self.forfeit_convicted_rights_v1(finding.forfeit, claim_id, &recorded_root)?;
        Ok(())
    }

    /// **RFC-0002 Phase F: kind 7, consumed** — [`Self::consume_executor_refuted_v1`]'s body over the IR
    /// adjudicator ([`crate::palw_offence_attribution_v1::palw_check_tir_identity_mismatch_v1`]) and the
    /// IR key: the same conviction funnel past `palw_rcore_plus` (which `palw_tir_v1` requires), the
    /// same forfeiture by claim. No heavy-prompt charge: admission v10 caps an IR class's canonical
    /// prompt at J5b's inline bound.
    pub(super) fn consume_tir_identity_mismatch_v1(
        &mut self,
        ctx: &PalwBlockContextV2,
        accused: PalwBondKeyV2,
        evidence_id: Hash64,
        evidence: &[u8],
        evidence_budget: crate::palw_offence_attribution_v1::PalwObjectiveEvidenceBudgetV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_offence_attribution_v1::{
            palw_check_tir_identity_mismatch_at_v1_with_budget_v1, palw_tir_identity_offence_id_v1,
        };
        use crate::palw_offence_v1::PalwOffenceKindV1;
        if crate::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != evidence_id {
            return Err(PalwStateV2Error::ObjectiveOffenceRefused(
                evidence_id,
                "evidence_id is not the digest of the evidence bytes".into(),
            ));
        }
        if !self.params.rcore_plus_active_at(ctx.daa_score) {
            return Err(PalwStateV2Error::ObjectiveOffenceDormant);
        }
        let finding = palw_check_tir_identity_mismatch_at_v1_with_budget_v1(
            &self.state,
            &accused,
            evidence,
            false,
            self.identity_rules_v1(ctx.daa_score),
            self.session_rule_v1(ctx.daa_score),
            evidence_budget,
        )
        .map_err(|e| PalwStateV2Error::ObjectiveOffenceRefused(evidence_id, e.to_string()))?;
        let claim_id = finding.target.claim_id;
        let offence_id = palw_tir_identity_offence_id_v1(&accused.0, &claim_id);
        if self.state.consumed_offences.contains_key(&offence_id) {
            return Ok(());
        }
        let now = ctx.daa_score;
        let conv = self.open_conviction_v1(&[accused], now);
        let g = self.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);
        let live = self.claim_is_live_before_final_v1(&claim_id);
        let fp_first = self.post_final_fp_g_v1(&claim_id);
        let mut nominal = self.act_on_convicted_claim_v1(ctx, claim_id, Some(&conv))?;
        if !live {
            let tier = self.post_final_producer_leg_v1(
                &conv,
                claim_id,
                accused,
                offence_id,
                PalwOffenceKindV1::TirIdentityMismatch,
                g,
                fp_first,
            )?;
            nominal = nominal.saturating_add(tier);
        }
        let recorded_root = self.recorded_forfeit_root_v1(finding.forfeit, &finding.target);
        let offence = crate::palw_aggregate_liability_v1::palw_evidence_contradiction_offence_v1(
            PalwOffenceKindV1::TirIdentityMismatch,
            evidence,
        );
        self.close_conviction_v1(
            &conv,
            offence_id,
            PalwOffenceKindV1::TirIdentityMismatch,
            accused,
            nominal,
            recorded_root,
            claim_id,
            &[accused],
            &[(accused, offence)],
        )?;
        self.forfeit_convicted_rights_v1(finding.forfeit, claim_id, &recorded_root)?;
        self.open_reporter_reward(now, offence_id, 0, PalwConvictionBasisV1::CheckedEvidence { evidence_id })?;
        Ok(())
    }

    /// **ADR-0152 v2 F2: a liability row whose execution a conviction proved false says so.**
    ///
    /// A `Final` reversed by a conviction, or a claim that retired before one, leaves its liability
    /// row reading "an honest Final" (`voided_daa = None`), and the other `Valid` signers can then
    /// be named only through a contradiction of their own. Marking the row `CourtFraud` at the
    /// conviction's DAA — the reason a court's own finding of a false execution writes — lets a
    /// `CourtFraud` contradiction bind every co-signer through the row, as it does for a claim a
    /// court voided. A row that already records a void keeps it: that is what happened first.
    ///
    /// `reason` is what the conviction PROVED: `CourtFraud` for a proof of the execution; a DA-7
    /// default after `Final` writes `ProducerWithholding` (M3 review F1) — a confirmed silence, which
    /// only N9's `da_confirmed` gate may turn against a signer, never a `CourtFraud` contradiction.
    pub(super) fn mark_liability_convicted(
        &mut self,
        claim_id: Hash64,
        daa: u64,
        reason: PalwVoidReasonV2,
    ) -> Result<(), PalwStateV2Error> {
        let Some(row) = self.state.panel_liabilities.get(&claim_id) else { return Ok(()) };
        if row.voided_daa.is_some() || row.void_reason.is_some() {
            return Ok(());
        }
        let mut marked = row.clone();
        marked.voided_daa = Some(daa);
        marked.void_reason = Some(reason);
        self.write_panel_liability(claim_id, Some(marked))?;
        Ok(())
    }

    /// **2026-09-24 DoS audit #7: forfeiture reaches the execution schedule already minted.**
    ///
    /// ADR-0151 filters a convicted root when the next snapshot is taken and when the next mint
    /// runs; a conviction that lands after either left the tickets it would have filtered in
    /// `round_schedules` (or the Finals in `round_pending`), and the round verdict
    /// (`palw_round_verdicts_v1`) grants whatever the parent state's schedule grants. Rewriting the
    /// rows at the conviction — rather than teaching every reader of a schedule to consult the
    /// forfeiture set — keeps "what a round may be won by" in ONE rooted place: the verdict, the
    /// producer's round view and the equivocation check all read the same pruned schedule, and a
    /// reorg that unwinds the conviction restores the rows through the entries written here
    /// (`RoundSchedule` / `RoundPending`, each carrying its old value). At most two schedules and
    /// one snapshot are kept at a time, so this touches at most three rows.
    pub(super) fn forfeit_minted_round_rights(&mut self, forfeited_root: &Hash64) -> Result<(), PalwStateV2Error> {
        use crate::palw_execution_lane_v1::{palw_execution_schedule_forfeit_v1, palw_execution_snapshot_forfeit_v1};
        if *forfeited_root == Hash64::default() {
            return Ok(());
        }
        for span in self.state.round_schedules.keys().copied().collect::<Vec<_>>() {
            let pruned = self
                .state
                .round_schedules
                .get(&span)
                .and_then(|schedule| palw_execution_schedule_forfeit_v1(schedule, forfeited_root));
            if let Some(pruned) = pruned {
                self.write_round_schedule(span, Some(pruned))?;
            }
        }
        for target in self.state.round_pending.keys().copied().collect::<Vec<_>>() {
            let pruned = self
                .state
                .round_pending
                .get(&target)
                .and_then(|snapshot| palw_execution_snapshot_forfeit_v1(snapshot, forfeited_root));
            if let Some(pruned) = pruned {
                // A snapshot is written only when it lists a domain (`rotate_round_lane`); one the
                // conviction emptied is dropped the same way, and its span is idle, explicitly.
                self.write_round_pending(target, (!pruned.domains.is_empty()).then_some(pruned))?;
            }
        }
        Ok(())
    }

    /// **ADR-0152 v3.1 V-2b / SPEC §4.5: forfeiture by CLAIM** — the sibling of
    /// [`Self::forfeit_minted_round_rights`] for a fault that proves the claim answers the wrong job
    /// or output (F1: 9, 10). The claim's own Finals leave the minted schedules and the pending
    /// snapshots; every other claim's rows — an honest lender's on the SAME execution root included —
    /// are untouched, except that a lender's Final in a minted schedule INHERITS the claim's unspent
    /// tickets where the mint had collapsed the root into the convicted claim's (the Phase 1–2
    /// review's M-1: `palw_execution_schedule_forfeit_claim_v1`). At most three rows, as there; zero
    /// rows touched when the claim has none.
    ///
    /// **And the open span's gathered Finals** (T18k): a root forfeit is filtered out of the next
    /// snapshot by the forfeiture set the ledger row feeds, but a claim-proving conviction records
    /// root 0 — so the claim's own entry in `round_finals` leaves here, or the span it finalized in
    /// would snapshot and mint the voided claim's rights at its end. The span marker goes with the
    /// last entry, as the rotation clears it.
    pub(super) fn forfeit_minted_round_rights_of_claim(&mut self, claim_id: &Hash64) -> Result<(), PalwStateV2Error> {
        use crate::palw_execution_lane_v1::{palw_execution_schedule_forfeit_claim_v1, palw_execution_snapshot_forfeit_claim_v1};
        if self.state.round_finals.contains_key(claim_id) {
            self.write_round_final(*claim_id, None)?;
            if self.state.round_finals.is_empty() {
                self.write_round_span(0)?;
            }
        }
        for span in self.state.round_schedules.keys().copied().collect::<Vec<_>>() {
            // The review's M-1: the claim's unspent tickets pass to the root's remaining Final; a
            // ticket whose round's permit this chain accepted is spent and leaves with the claim.
            let pruned = self.state.round_schedules.get(&span).and_then(|schedule| {
                palw_execution_schedule_forfeit_claim_v1(schedule, claim_id, |round| {
                    self.state.round_permit_used(schedule.span_index, round, 0)
                })
            });
            if let Some(pruned) = pruned {
                self.write_round_schedule(span, Some(pruned))?;
            }
        }
        for target in self.state.round_pending.keys().copied().collect::<Vec<_>>() {
            let pruned = self
                .state
                .round_pending
                .get(&target)
                .and_then(|snapshot| palw_execution_snapshot_forfeit_claim_v1(snapshot, claim_id));
            if let Some(pruned) = pruned {
                self.write_round_pending(target, (!pruned.domains.is_empty()).then_some(pruned))?;
            }
        }
        Ok(())
    }

    /// **2026-09-24 DoS audit #8: a conviction after `Final` takes back what the `Final` counted.**
    ///
    /// A `PanelFalseValid` conviction proves the Valid receipt that licensed this claim was false,
    /// and before this it changed nothing about the claim: the phase stayed `Final`, the claim's
    /// `safe_weight` stayed in fork choice (59,742.94 MSK of it for one 2M claim, the audit's
    /// finding 20), the class kept a probe pass it never earned, the version its usage, and every
    /// receipt block the Final licensed kept its weight. Past the fence the claim is VOIDED —
    /// `CourtFraud`, the reason a proven-false execution already carries when a court finds it —
    /// and each thing its `Final` counted is subtracted where it was counted:
    ///
    /// * **Weight.** An attempt's `Final` added its contribution; a free-prompt `Final` added
    ///   nothing and each spend added `per_quantum`. The amount subtracted is the one
    ///   `retire_claim` would have moved to `retired_safe_weight` — `canonical.unwrap_or(pwu)` for
    ///   an attempt (under ADR-0160 F-W's C7 ceiling where the claim is new-rule, through the same
    ///   `palw_weight_final_safe_v1` `finalize_claim` and `retire_claim` price it with — the identity
    ///   everywhere else), `per_quantum × |spent|` for a free-prompt claim — i.e. the claim's half of
    ///   the CEILING `assert_internal_consistency_v3` re-derives. That is exact wherever ADR-0069
    ///   Decision 7 is dormant (contribution == ceiling). Where it is armed the contribution the
    ///   `Final` actually added may have been zero (a weightless class) and is recorded nowhere, so
    ///   subtracting the CEILING is the only choice that provably keeps `safe_weight <= ceiling`:
    ///   subtracting less could leave `safe_weight` above the bound the claim's removal lowers, and
    ///   `load_tip` refuses such a tip on every node. Understating is the harmless direction (the
    ///   check says so), and saturating keeps a conviction from ever failing the block over it.
    /// * **The spent set is cleared** with the weight: a free-prompt claim with spends outside
    ///   `Final` is refused by the consistency check ("spent before Final"), and a voided claim can
    ///   never spend again, so nothing reads it.
    /// * **Probe**: the pass `finalize_claim` noted is withdrawn from the running total — and from
    ///   the span's, when the `Final` fell in the span the registry is still observing (the span
    ///   step resets that counter, so an older pass is no longer in it) — and a failure is noted, as
    ///   `void_claim` notes one for `CourtFraud`.
    /// * **Usage**: `uncount_claim_usage`, `void_claim`'s own subtraction.
    /// * **Deadline**: the `Final` retirement is disarmed and the `Voided` one armed from the
    ///   conviction's DAA, as `void_claim` arms it.
    ///
    /// **Not reversed, deliberately:** `final_work` — a reader-only shadow that is never in the
    ///   root (op 186's share), whose per-claim amount was the economics snapshot `finalize_claim`
    ///   deletes, so an exact subtraction needs a record the state does not keep; the safe frontier,
    ///   which never retreats by construction (fork choice's first key); payouts, which the next
    ///   block's coinbase has already carried; and the receipt census, a retarget measurement of
    ///   blocks that were in fact produced.
    ///
    /// **The void's reason is the caller's** (M3 review F1): `CourtFraud` for a conviction that proved
    /// the execution false, `ProducerWithholding` for a DA-7 default after `Final` — the one reason
    /// kind 3 can name against a signer only through N9's `da_confirmed` gate.
    ///
    /// Only the NAMED claim, never its siblings by root: the forfeiture of downstream rights is by
    /// root (ADR-0151, #5, #7), but voiding a `Final` another producer reached on the same root
    /// would let one self-convicted copy strip an honest claim's weight. A claim already retired,
    /// or not `Final`, is left alone: its weight moved to `retired_safe_weight` with no record left
    /// to re-derive it from, or it has counted nothing yet.
    pub(super) fn reverse_convicted_final(
        &mut self,
        ctx: &PalwBlockContextV2,
        id: Hash64,
        reason: PalwVoidReasonV2,
    ) -> Result<(), PalwStateV2Error> {
        let Some(claim) = self.state.claims.get(&id).cloned() else { return Ok(()) };
        let PalwClaimPhaseV2::Final { final_daa } = claim.phase else { return Ok(()) };
        let weight = match &claim.source {
            // ADR-0160 F-W (D-3): a new-rule claim's `Final` added its weight under the C7 ceiling
            // (`finalize_claim`), so the reversal takes back that same scaled amount — the expression
            // `retire_claim` moves to `retired_safe_weight`. Subtracting the unscaled
            // `canonical.unwrap_or(pwu)` here removed ~2,400× what a 2M-scale `Final` had added and
            // saturated `safe_weight` to zero, stripping every other `Final` from fork choice. The
            // identity on every old-rule claim and every claim under the ceiling.
            PalwClaimSourceV2::Attempt => crate::palw_weight_cap_v1::palw_weight_final_safe_v1(
                self.params,
                &claim,
                self.canonical_claim_weight(&claim).unwrap_or(claim.pwu as u128),
            ),
            PalwClaimSourceV2::FreePrompt { quanta, spent } => {
                let per_quantum = palw_fp_spend_weight_v1(&self.state, &claim, *quanta, &self.fp_pricing());
                per_quantum.checked_mul(spent.len() as u128).ok_or(PalwStateV2Error::Overflow("convicted free-prompt weight"))?
            }
        };
        // Lane BUDGET: a budgeted claim's reversal takes back exactly what its Final (or its spends) was granted; nothing returns to its
        // bond's window.
        let weight = self.state.bond_budget_weight_or(&id, weight);
        // RFC-0018 Gate C: take back exactly what the Final (or the spends) added — nothing where Gate C withheld it.
        let weight = crate::palw_rfc0018_v1::palw_gate_c_weight_v1(self.params, &self.state, &id, &claim, weight);
        self.bond_budget_close_v1(&id, ctx.daa_score)?;
        self.state.safe_weight = self.state.safe_weight.saturating_sub(weight);
        self.unnote_model_probe_pass(&claim, final_daa, ctx.daa_score)?;
        self.unnote_activation_probe_credits_v1(&id, &claim)?;
        self.note_model_probe(&claim, false, ctx.daa_score)?;
        self.uncount_claim_usage(&id, &claim)?;
        let mut voided = claim.clone();
        voided.phase = PalwClaimPhaseV2::Voided { voided_daa: ctx.daa_score, reason };
        if let PalwClaimSourceV2::FreePrompt { spent, .. } = &mut voided.source {
            spent.clear();
        }
        // RFC-0004 A6: an evaluation claim convicted after `Final` takes its score back out of the epoch while the
        // epoch still takes scores, and frees its job (spec 17 §17.8.6).
        self.note_improvement_eval_reversed_v1(&id, &claim)?;
        self.write_claim(id, Some(voided.clone()))?;
        palw_exec_v2_fold::on_claim_voided_v2(self, &id, ctx.daa_score)?;
        self.disarm_deadline(id);
        self.arm_retirement(id, &voided)?;
        Ok(())
    }

    pub(super) fn false_valid_execution_roots(
        &self,
        payload: &crate::palw_offence_v1::PalwPanelFalseValidEvidenceV1,
    ) -> Result<(Hash64, Hash64), PalwStateV2Error> {
        if let Some(claim) = self.state.claims.get(&payload.claim_id) {
            let artifact = self.state.classes.get(&claim.class_id).map(|c| c.artifact_root).unwrap_or_default();
            return Ok((claim.execution_root, artifact));
        }
        if let Some(row) = self.state.panel_liabilities.get(&payload.claim_id) {
            let artifact = self.state.classes.get(&row.class_id).map(|c| c.artifact_root).unwrap_or_default();
            return Ok((row.execution_root, artifact));
        }
        Err(PalwStateV2Error::ObjectiveOffenceRefused(
            payload.claim_id,
            "PanelFalseValid names neither a live claim nor a liability row".into(),
        ))
    }

    pub(super) fn bind_panel_false_valid(
        &self,
        ctx: &PalwBlockContextV2,
        payload: &crate::palw_offence_v1::PalwPanelFalseValidEvidenceV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_offence_v1::PalwPanelContradictionV1;
        let refused = |why: String| PalwStateV2Error::ObjectiveOffenceRefused(payload.claim_id, why);
        match &payload.contradiction {
            // Both void-naming arms read one predicate (`palw_void_binds_claim_v1`), which ADR-0152
            // v2's adjudicator reads too: the live claim voided for this reason at this DAA, or its
            // liability row recording the same void.
            PalwPanelContradictionV1::ProducerWithholding { voided_daa } => {
                if !self.state.palw_void_binds_claim_v1(&payload.claim_id, PalwVoidReasonV2::ProducerWithholding, *voided_daa) {
                    return Err(refused("ProducerWithholding does not bind this claim or its liability".into()));
                }
            }
            PalwPanelContradictionV1::CourtFraud { voided_daa } => {
                if !self.state.palw_void_binds_claim_v1(&payload.claim_id, PalwVoidReasonV2::CourtFraud, *voided_daa) {
                    return Err(refused("CourtFraud does not bind this claim or its liability".into()));
                }
            }
            PalwPanelContradictionV1::CourtExecutorGuilty { offence_id } => {
                let consumed = self
                    .state
                    .consumed_offences
                    .get(offence_id)
                    .ok_or_else(|| refused("CourtExecutorGuilty names an offence this chain has not consumed".into()))?;
                let executor = self
                    .state
                    .claims
                    .get(&payload.claim_id)
                    .map(|c| c.bond)
                    .or_else(|| self.state.panel_liabilities.get(&payload.claim_id).map(|r| r.executor_bond));
                if executor != Some(PalwBondKeyV2(consumed.accused)) {
                    return Err(refused("CourtExecutorGuilty does not name this claim's executor".into()));
                }
            }
            PalwPanelContradictionV1::ConflictingPermit { span, round, permit_index } => {
                if !self.state.round_equivocated(*span, *round, *permit_index) {
                    return Err(refused("ConflictingPermit names a permit this chain has not burned".into()));
                }
            }
            // **2026-09-24 DoS audit review: an equivocation proves something only about the key
            // that signed it, so it must be the CLAIM EXECUTOR's key.**
            //
            // The acceptance layer (`verify_palw_panel_false_valid_v1`) checks the two attestations
            // under `payload.executor_pubkey` and adjudicates them against a bond record it builds
            // from that key and `carriage.accused_bond_outpoint` — both taken from the evidence
            // itself. The field's doc said the processor checks the key against the claim row; no
            // code did. So anyone with a fresh ML-DSA key could sign two roots under
            // `job_id = claim`, attach a seat's public `Valid` receipt, and convict that seat — and
            // since #7/#8 the same object voided an honest `Final`, took back its weight and
            // forfeited its root's rights, for one object fee and no bond
            // (`review_economic_forged_false_valid`). Past the fence the carriage must name the
            // claim's executor bond — the live claim's `bond`, or the liability row's
            // `executor_bond` once the claim has retired — and the key must be the one that bond
            // registered; a bond this state no longer holds cannot be asked, and is refused (the
            // fail-closed side: no key the chain can vouch for, no conviction). The processor
            // asks the same question before the signature is worth anything, so the two layers
            // agree; the fold is what every node's state root rests on. Below the fence nothing is
            // read and the arm is what it was.
            PalwPanelContradictionV1::ExecutorEquivocation(carriage) if self.extras.audit_2026_09_23_active => {
                let executor = self
                    .state
                    .claims
                    .get(&payload.claim_id)
                    .map(|c| c.bond)
                    .or_else(|| self.state.panel_liabilities.get(&payload.claim_id).map(|r| r.executor_bond))
                    .ok_or_else(|| refused("ExecutorEquivocation names neither a live claim nor a liability row".into()))?;
                if carriage.accused_bond_outpoint != executor.0 {
                    return Err(refused("ExecutorEquivocation does not accuse this claim's executor bond".into()));
                }
                let registered = self
                    .state
                    .bonds
                    .get(&executor)
                    .ok_or_else(|| refused("ExecutorEquivocation names an executor bond this chain no longer holds".into()))?;
                if registered.pubkey != payload.executor_pubkey {
                    return Err(refused("ExecutorEquivocation is not signed under the key the executor bond registered".into()));
                }
            }
            PalwPanelContradictionV1::ExecutorEquivocation(_)
            | PalwPanelContradictionV1::StepArithmetic { .. }
            | PalwPanelContradictionV1::StepStructural(_)
            | PalwPanelContradictionV1::Legs(_)
            | PalwPanelContradictionV1::ForgedOutput { .. } => {}
            // V1 parity: refused right after decode by the caller; here too, as it was refused.
            PalwPanelContradictionV1::IdentityMismatch { .. }
            | PalwPanelContradictionV1::OutputMismatch { .. }
            | PalwPanelContradictionV1::ForgedOutputTiled { .. }
            | PalwPanelContradictionV1::LogitsNotStepOutput { .. }
            | PalwPanelContradictionV1::PromptNotAnchored { .. }
            | PalwPanelContradictionV1::ConstrainedDecode { .. }
            | PalwPanelContradictionV1::ConstrainedDecodeFlat { .. }
            | PalwPanelContradictionV1::RegisteredTableMismatch { .. }
            | PalwPanelContradictionV1::PrefixStateMismatch { .. }
            | PalwPanelContradictionV1::TokenizerTableMismatch { .. }
            | PalwPanelContradictionV1::StagedCourt { .. } => {
                return Err(refused("PanelFalseValid evidence does not decode".into()));
            }
        }
        let _ = ctx;
        Ok(())
    }

    pub(super) fn write_artifact_owner(
        &mut self,
        key: (Hash64, Hash64),
        new: Option<crate::palw_model_lines_v1::PalwArtifactOwnerV1>,
    ) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.artifact_owners, false, "native artifact_owners writer")?;
        let old = self.state.artifact_owners.get(&key).copied();
        if old == new {
            return Ok(());
        }
        match new {
            Some(owner) => {
                self.state.artifact_owners.insert(key, owner);
            }
            None => {
                self.state.artifact_owners.remove(&key);
            }
        }
        self.entries.push(PalwDeltaEntryV2::ArtifactOwner { key, old, new });
        Ok(())
    }

    /// **ADR-0143 Decision 5: the ONE gate a root passes through to enter state.**
    ///
    /// Every entrance calls this — a class reserving its founding root, a line founding, a version
    /// publication — because fixing one call site leaves the others open, which is how the defect
    /// was born. An owned root is refused; a root its own line already owns keeps the row it has,
    /// since the question the index answers is WHICH LINE and moving the version would rewrite
    /// where the claims already counted point.
    ///
    /// Below the fence this does nothing at all, so a chain that has not armed it writes the state
    /// root it always did.
    pub(super) fn claim_artifact_root(
        &mut self,
        root: &Hash64,
        class_id: &Hash64,
        line_id: Hash64,
        version: u32,
    ) -> Result<(), PalwStateV2Error> {
        if !self.extras.artifact_root_ownership_active {
            return Ok(());
        }
        if let Some(owner) = self.state.artifact_owners.get(&(*class_id, *root)).copied() {
            if owner.line_id != line_id {
                return Err(PalwStateV2Error::DuplicateArtifactRoot { root: *root, owner: owner.line_id });
            }
            return Ok(());
        }
        self.write_artifact_owner(
            (*class_id, *root),
            Some(crate::palw_model_lines_v1::PalwArtifactOwnerV1 { class_id: *class_id, line_id, version }),
        )?;
        Ok(())
    }

    pub(super) fn write_final_work(&mut self, key: (u64, Hash64), new: Option<u128>) -> Result<(), PalwStateV2Error> {
        // RFC-0006: an absent outer epoch bucket and an absent inner class membership, from the actual maps.
        if new.is_some() {
            let outer = !self.state.final_work.contains_key(&key.0);
            let inner = self.state.final_work.get(&key.0).is_none_or(|classes| !classes.contains_key(&key.1));
            self.reserve_index_members_v1::<u64, BTreeMap<Hash64, u128>>(u64::from(outer), "native final_work epoch bucket")?;
            self.reserve_index_members_v1::<Hash64, u128>(u64::from(inner), "native final_work class membership")?;
        }
        self.reserve_journal_slots_v1(1, "native final_work journal")?;
        let old = self.state.final_work.get(&key.0).and_then(|classes| classes.get(&key.1)).copied();
        match new {
            Some(work) => {
                self.state.final_work.entry(key.0).or_default().insert(key.1, work);
            }
            None => {
                if let Some(classes) = self.state.final_work.get_mut(&key.0) {
                    classes.remove(&key.1);
                    if classes.is_empty() {
                        self.state.final_work.remove(&key.0);
                    }
                }
            }
        }
        if old != new {
            self.entries.push(PalwDeltaEntryV2::FinalWork { key, old, new });
        }
        Ok(())
    }

    /// **ADR-0137 (shadow): a Final of a model class is `W` of work** — the claim's snapshot where
    /// Upgrade C took one (its attempted compute), the shadow's `W` otherwise; the floor and the
    /// receipt lane are not economic work. Written by closed epoch, pruned past the kept window.
    pub(super) fn note_final_work(&mut self, id: &Hash64, claim: &PalwClaimStateV2, final_daa: u64) -> Result<(), PalwStateV2Error> {
        if !matches!(claim.source, PalwClaimSourceV2::Attempt) || claim.class_id == self.params.base_class_id() {
            return Ok(());
        }
        let work = match self.state.claim_economics.get(id) {
            Some(snapshot) => snapshot.attempted_ccu(),
            None => match self.state.work_target_shadow {
                Some(target) => target.work,
                None => return Ok(()),
            },
        };
        if work == 0 {
            return Ok(());
        }
        let epoch = final_daa / self.params.epoch_length.max(1);
        let current = self.state.final_work.get(&epoch).and_then(|classes| classes.get(&claim.class_id)).copied().unwrap_or(0);
        self.write_final_work((epoch, claim.class_id), Some(current.saturating_add(work)))?;
        let keep_from = epoch.saturating_sub(crate::palw_work_target_v1::PALW_FINAL_WORK_EPOCHS_KEPT_V1);
        let stale: Vec<(u64, Hash64)> =
            self.state.final_work.range(..keep_from).flat_map(|(e, classes)| classes.keys().map(move |c| (*e, *c))).collect();
        for key in stale {
            self.write_final_work(key, None)?;
        }
        Ok(())
    }

    // ---- ADR-0145: the derived basis, resolved once for the fold -----------------------------

    /// **A claim's `safe_weight` contribution on the derived basis** — `None` while the fence is
    /// dormant, which is every shipped preset.
    ///
    /// Reads the SAME rooted state the two later re-derivations read
    /// ([`PalwChainStateV2::palw_claim_canonical_weight_v1`]), so the three cannot drift — the
    /// failure `retire_claim`'s own comment calls "permanent, because the claim is gone and
    /// nothing later can reconcile it".
    pub(super) fn canonical_claim_weight(&self, claim: &PalwClaimStateV2) -> Option<u128> {
        self.state.palw_claim_canonical_weight_v1(claim, self.extras.canonical_work_daa)
    }

    /// ADR-0148: this fold's free-prompt pricing, from the ruleset and the unit's height.
    pub(super) fn fp_pricing(&self) -> PalwFpPricingV1 {
        PalwFpPricingV1::of(self.params, self.extras.canonical_work_daa)
    }

    /// **ADR-0148: open the lane's one receipt target**, once — at the first compute-priced
    /// commitment, from the floor's own receipt target (so a floor quantum keeps the odds it had:
    /// the network quantum IS one floor quantum's share of the floor's job), clamped to the price
    /// ceiling ([`crate::palw_freeprompt_v3::fp_pooled_target_ceiling_v1`]) where the chain holds
    /// a work target. After this only the retarget moves it.
    pub(super) fn seed_fp_pooled_receipt_target(&mut self) -> Result<(), PalwStateV2Error> {
        let key = crate::palw_freeprompt_v3::fp_pooled_receipt_key_v1();
        if self.state.receipt_targets.contains_key(&key) {
            return Ok(());
        }
        let pricing = self.fp_pricing();
        let floor =
            self.state.receipt_targets.get(&self.params.base_class_id).map(|t| t.target).unwrap_or(PALW_RECEIPT_TARGET_SEED_V1);
        let seed = match self.state.work_target.map(|target| target.work.max(target.floor)) {
            Some(work) => {
                floor.min(crate::palw_freeprompt_v3::fp_pooled_target_ceiling_v1(pricing.network_quantum(&self.state), work))
            }
            None => floor,
        };
        self.write_receipt_target(key, Some(PalwClassTargetV2 { target: seed.max(1) }))?;
        Ok(())
    }

    /// **What one draw of a class really costs, for the exposure and work-price paths** — `None`
    /// while the fence is dormant.
    ///
    /// `accepted_daa` is the claim's own, or the accepting block's for a claim being created. It is
    /// saturated into `u64` because that is the width `palw_exposure_pwu_v2` and the ADR-0124 work
    /// price speak in; the largest shipped class's draw is ~2 × 10^11 MAC-equivalents, four orders
    /// under the ceiling, so the saturation is a type boundary rather than a rule.
    ///
    /// ADR-0149 §5: through [`PalwChainStateV2::palw_attempt_per_draw_v1`], so the floor's first
    /// row-less attempt is reserved on the draw its admission was priced on.
    pub(super) fn canonical_per_draw(&self, class_id: &Hash64, accepted_daa: u64) -> Option<u64> {
        self.read().canonical_per_draw(class_id, accepted_daa)
    }

    /// **ADR-0149 §5: the floor's draw as the registry will write it**, for the blocks before it has
    /// — the same table entry `step_model_registry` copies into the floor's row, and the same one
    /// the node hands the admission (`PalwEpochBudgetFencesV1::base_known_draw`), so the gate that
    /// admits the floor's first row-less attempt and the ledger that reserves for it read one
    /// number. `None` below the registry, where nothing past the canonical-work fence can be
    /// (`validate_palw_v2` keeps the registry at or below it).
    pub(super) fn base_known_draw(&self) -> Option<u128> {
        self.read().base_known_draw()
    }

    /// The floor class's leaves-to-derived-work ratio at this block, for the one place a pwu is
    /// multiplied by an absolute price rather than divided by a unit ([`PalwExposureBasisV1`]).
    pub(super) fn exposure_basis(&self, accepted_daa: u64) -> Option<PalwExposureBasisV1> {
        self.read().exposure_basis(accepted_daa)
    }

    /// **The pwu a class's DIFFICULTY is seeded from** — ADR-0145 I1 and the audit's invariant
    /// (vi), "protocol difficulty must not be set from miner-controlled metadata".
    ///
    /// `attempt_target_seed_v1(share, pwu)` read `palw_max_exposure_pwu_of_rule_v1` of the class's
    /// rule at both of its live call sites: the registrant's own declared number, deciding the
    /// class target, hence `expected_attempts`, hence `claim.pwu`, hence fork-choice weight. That
    /// is F1's remaining path, and unlike the work price it had no canonical fallback at all.
    ///
    /// **Normalised, for the reason the exposure is and the price unit is not.** The seed meets an
    /// ABSOLUTE constant — `PALW_ATTEMPT_TARGET_UNIT_SHARE_PWU_V1`, past which the target
    /// saturates and `expected_attempts` pins at one — so a change of unit moves which classes
    /// saturate. The work price and a claim's weight are RATIOS, where the unit divides out, and
    /// normalising one side of those would break the cancellation instead of preserving it. The
    /// rule is the quantity's company, not the quantity: normalise where a pwu meets a constant,
    /// leave it raw where it meets another pwu.
    pub(super) fn class_seed_pwu(&self, class_id: &Hash64, accepted_daa: u64) -> u64 {
        let declared = self.state.classes.get(class_id).map(|record| palw_max_exposure_pwu_of_rule_v1(&record.pwu_rule));
        let Some(declared) = declared else { return 0 };
        let Some(record) = self.state.classes.get(class_id) else { return declared };
        palw_exposure_pwu_v3(record, declared, self.canonical_per_draw(class_id, accepted_daa), self.exposure_basis(accepted_daa))
    }

    // ---- ADR-0132 Upgrade C: the economic payout in the fold ----------------------------------

    /// The registry's work for a class: its lifecycle row's, or the genesis registration's.
    pub(super) fn model_class_work(&self, class_id: &Hash64) -> Option<crate::palw_model_registry_v1::PalwModelWorkV1> {
        if let Some(row) = self.state.model_lifecycles.get(class_id) {
            return Some(row.work);
        }
        self.model_registry_fold().and_then(|fold| fold.genesis_works.get(class_id).copied())
    }

    /// **The claim snapshots its economics at acceptance** (ADR-0132 proposal B): the registry's
    /// work for its class, the class target now and the carrying block's network draw, priced at
    /// the rate now, with the panel's share derived once. Nothing for the liveness floor, nothing
    /// below the fence, nothing for a class the registry holds no work for (it is paid as before).
    pub(super) fn snapshot_claim_economics(&mut self, claim_id: Hash64, class_id: &Hash64, carrying_bits: u32) -> Result<(), PalwStateV2Error> {
        let Some(fold) = self.extras.economic_payout else { return Ok(()) };
        if *class_id == self.params.base_class_id() {
            return Ok(());
        }
        let Some(work) = self.model_class_work(class_id) else { return Ok(()) };
        let seat_count = self.model_registry_fold().map(|registry| registry.globals.seat_count).unwrap_or(0);
        let target = self.state.class_targets.get(class_id).map(|t| t.target).unwrap_or(u128::MAX);
        let snapshot =
            crate::palw_economic_payout_v1::palw_claim_economics_snapshot_v1(&fold, &work, seat_count, target, carrying_bits);
        self.write_claim_economics(claim_id, Some(snapshot))?;
        Ok(())
    }

    /// **A class's cap utilization at a boundary** (ADR-0133 Fence 3): `attempted × rate` against
    /// the escrow a claim of this block holds, in permille; `0` where the payout is dormant or the
    /// block funds no escrow — no verdict, never a false "fine".
    pub(super) fn class_cap_utilization_permille(
        &self,
        work: &crate::palw_model_registry_v1::PalwModelWorkV1,
        expected_attempts_q32: u128,
        ctx: &PalwBlockContextV2,
    ) -> u32 {
        let Some(fold) = self.extras.economic_payout else { return 0 };
        let escrow = worker_carve_v2(self.params, ctx.subsidy, self.extras.escrow_carve);
        if escrow == 0 {
            return 0;
        }
        let attempted = crate::palw_economic_payout_v1::palw_attempted_ccu_v1(
            expected_attempts_q32,
            crate::palw_economic_payout_v1::palw_network_draws_q32_from_bits_v1(fold.block_bits),
            work.economic_ccu_per_claim,
        );
        crate::palw_economic_payout_v1::palw_cap_utilization_permille_v1(attempted, fold.rate_sompi_per_giga, escrow)
    }

    // ---- ADR-0135: the model registry in the fold --------------------------------------------

    /// The registry's fold input, where the fence is active at this block.
    pub(super) fn model_registry_fold(&self) -> Option<&crate::palw_model_registry_v1::PalwModelRegistryFoldV1> {
        self.extras.model_registry.as_ref()
    }

    /// **A possession proof must be newer than the row it would write** (the readiness-horizon
    /// review's replay finding; user decision 2026-09-25, readiness capacity option (a)). A proof's
    /// signature covers (bond, class, span, the opened bytes) and nothing binds it to its carrier, so
    /// anyone may re-file a seat's OLDER proof inside the landing window — and the row writer took
    /// the latest-landed proof, so the replay dated the row back to the old span (at 24 spans a
    /// proof up to 40 spans old writes a row that is stale on arrival, taking the seat out of the
    /// count, the jury and the draw), and re-dated the pool's F4 landing record with it. Past
    /// `Params::palw_readiness_v2_max_age_spans` (the bundle's mirror; genesis-only, so armed is in
    /// force at every DAA — testnet-12 alone) a proof naming a span at or below the row's
    /// `proved_span` is refused by name, so the row and the landing record only move forward. An
    /// equal-span duplicate is refused too: it renews nothing. Below the fence (testnet-11, devnet,
    /// mainnet) the latest proof overwrites, byte for byte as before.
    pub(super) fn refuse_readiness_proof_not_newer_v1(
        &self,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        span: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.params.readiness_v2_max_age_spans().is_none() {
            return Ok(());
        }
        match self.state.seat_readiness.get(&(*bond, *class_id)) {
            Some(row) if span <= row.proved_span => {
                Err(PalwStateV2Error::ReadinessProofNotNewer { bond: *bond, class_id: *class_id, span, row_span: row.proved_span })
            }
            _ => Ok(()),
        }
    }

    /// **A seat's possession proof** (`SeatReadinessProved`): the opening reconstructs the class's
    /// registered artifact root, the leaf is one the (class, bond, span) challenge names, and the
    /// span is this one or the one just closed. The row it writes is what the draw reads.
    pub(super) fn apply_seat_readiness(
        &mut self,
        ctx: &PalwBlockContextV2,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        span: u64,
        opening: &crate::palw_artifact::PalwArtifactOpeningV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_model_registry_v1 as registry;
        let Some(fold) = self.model_registry_fold() else { return Err(PalwStateV2Error::ModelRegistryDormant) };
        let span_daa = fold.span_daa;
        let record = self.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err(PalwStateV2Error::BondNotActive(*bond));
        }
        let class = self.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
        crate::palw_artifact::verify_artifact_opening_v1(opening, class.artifact_root)
            .map_err(|e| PalwStateV2Error::ReadinessProofRefused(format!("{e}")))?;
        let span_now = crate::palw_execution_lane_v1::palw_execution_span_v1(ctx.daa_score, span_daa);
        // A proof for a span not yet opened is refused; one older than the landing allowance is
        // refused (the seat proves again for a current span rather than replaying an old one).
        let landing = registry::palw_readiness_landing_spans_v1(span_daa);
        if span > span_now || span_now - span > landing {
            return Err(PalwStateV2Error::ReadinessProofRefused(format!(
                "the proof names span {span} at span {span_now} (a proof lands within {landing} spans)"
            )));
        }
        // Past the readiness horizon's fence a proof no newer than the row is a replay.
        self.refuse_readiness_proof_not_newer_v1(bond, class_id, span)?;
        let bond_bytes = borsh::to_vec(bond).expect("a bond key is borsh-serializable");
        let seed = registry::palw_readiness_challenge_seed_v1(class_id, &bond_bytes, span);
        let (start, width) = registry::palw_readiness_window_v1(&seed, opening.leaf_count);
        if !registry::palw_readiness_window_contains_v1(start, width, opening.leaf_count, opening.leaf_index) {
            return Err(PalwStateV2Error::ReadinessProofRefused(format!(
                "the opened leaf {} is not one the challenge names ({start}..+{width} of {})",
                opening.leaf_index, opening.leaf_count
            )));
        }
        self.write_seat_readiness(
            (*bond, *class_id),
            Some(registry::PalwSeatReadinessRowV1 {
                proved_daa: span.saturating_mul(span_daa.max(1)),
                proved_span: span,
                leaf_index: opening.leaf_index,
                proof_version: 1,
                chunks: 1,
            }),
        )?;
        self.note_activation_readiness_landing_v1(class_id, bond, span_now)?;
        Ok(())
    }

    /// **A registered class's row, opened from its carriage** (ADR-0135 Decisions 2 and 3): the
    /// work is read off the graph, the profile derived from it, and the class starts PREFETCHING
    /// (or REGISTERED, never admitting, where the graph derives no supported kernel work).
    /// ADR-0135 manifest V2: the registrant's byte count lands on the class's registry row. The
    /// signature is the acceptance layer's (like a readiness proof's); here the rule is who may
    /// speak for the class and what the row does with the number.
    pub(super) fn apply_class_manifest(
        &mut self,
        ctx: &PalwBlockContextV2,
        class_id: &Hash64,
        artifact_bytes: u64,
        registrant_bond: &PalwBondKeyV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_model_registry_v1 as registry;
        let Some(fold) = self.model_registry_fold().cloned() else { return Err(PalwStateV2Error::ModelRegistryDormant) };
        if artifact_bytes == 0 {
            return Err(PalwStateV2Error::ClassManifestRefused(format!(
                "class {class_id}: an artifact of zero bytes is not a thing anyone keeps"
            )));
        }
        let class = self.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
        match class.registrant_bond {
            None => {
                return Err(PalwStateV2Error::ClassManifestRefused(format!(
                    "class {class_id} is a genesis class: its bytes are the catalog's, and no bond registered it"
                )));
            }
            Some(registrant) if registrant != *registrant_bond => {
                return Err(PalwStateV2Error::ClassManifestRefused(format!(
                    "class {class_id} was registered by bond {registrant:?}, not by {registrant_bond:?}"
                )));
            }
            Some(_) => {}
        }
        let record = self.state.bonds.get(registrant_bond).ok_or(PalwStateV2Error::MissingBond(*registrant_bond))?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err(PalwStateV2Error::BondNotActive(*registrant_bond));
        }
        let Some(mut row) = self.state.model_lifecycles.get(class_id).cloned() else {
            return Err(PalwStateV2Error::ClassManifestRefused(format!(
                "class {class_id} has no registry row yet (rows open at a span boundary): nothing to commit the bytes to"
            )));
        };
        // The working set equalled the estimate for every graph the estimate prices today (the
        // artifact whole): it follows the file where it did, and keeps its own value where not.
        let working_set_followed = row.work.working_set_bytes == row.work.artifact_bytes;
        row.work.artifact_bytes = artifact_bytes;
        if working_set_followed {
            row.work.working_set_bytes = artifact_bytes;
        }
        let target = self.state.class_targets.get(class_id).map(|t| t.target).unwrap_or(u128::MAX);
        let expected = crate::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
        row.profile = registry::palw_lifecycle_profile_v1(
            &row.work,
            expected,
            &fold.globals,
            self.extras.seat_gate_possession_at(ctx.daa_score),
        );
        self.write_model_lifecycle(*class_id, Some(row))?;
        Ok(())
    }

    /// **A seat's V2 possession proof** (`SeatReadinessProvedV2`, ADR-0133 §11.2): the multiproof
    /// reconstructs the class's registered artifact root, the leaves it opened are exactly the ones
    /// this (class, bond, span) challenge draws from the whole inventory, and the span is this one
    /// or one the landing window still admits. The row it writes is what the draw reads — stamped
    /// `proof_version = 2`, because past the fence nothing else counts.
    pub(super) fn apply_seat_readiness_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        span: u64,
        proof: &crate::palw_artifact::PalwArtifactMultiproofV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_model_registry_v1 as registry;
        if !self.extras.readiness_v2_active {
            return Err(PalwStateV2Error::ReadinessV2Dormant);
        }
        let Some(fold) = self.model_registry_fold() else { return Err(PalwStateV2Error::ModelRegistryDormant) };
        let span_daa = fold.span_daa;
        let record = self.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err(PalwStateV2Error::BondNotActive(*bond));
        }
        let class = self.state.classes.get(class_id).ok_or(PalwStateV2Error::MissingClass(*class_id))?;
        // The width the challenge names bounds the walk before the walk runs (the 2026-09-18 diff
        // audit): the stateless ride list refuses a wider proof too, and this is the same rule where
        // the state can be read.
        if proof.opened.len() > registry::PALW_READINESS_V2_CHUNKS_V1 as usize {
            return Err(PalwStateV2Error::ReadinessProofRefused(format!(
                "the proof opens {} leaves; no challenge names more than {}",
                proof.opened.len(),
                registry::PALW_READINESS_V2_CHUNKS_V1
            )));
        }
        let legacy_full_challenge = registry::palw_readiness_v2_is_legacy_full_challenge_v1(proof.opened.len(), proof.operand_bytes());
        if proof.operand_bytes() > registry::PALW_READINESS_V2_OPERAND_MAX_BYTES_V1 && !legacy_full_challenge {
            return Err(PalwStateV2Error::ReadinessProofRefused(format!(
                "the proof carries {} bytes of operands, above the {} one carrier holds",
                proof.operand_bytes(),
                registry::PALW_READINESS_V2_OPERAND_MAX_BYTES_V1
            )));
        }
        // **RFC-0004 §6.3/§6.7 (spec 17 §17.7.1): a composite class is possessed over its ADAPTER section.** A
        // composite's registered root is `H(parent_class ‖ parent_root ‖ adapter_root ‖ P)`, which is no
        // inventory root and which no multiproof opens; the proof reconstructs the adapter section's own
        // inventory root instead, the record the candidate's acceptance wrote. The record must root to the
        // class's registered artifact root, so a stale or foreign record opens nothing.
        let possession_root = match self.state.improvement_composite_class(class_id) {
            Some(record) if self.params.improve_active_at(ctx.daa_score) => {
                if record.artifact_root() != class.artifact_root {
                    return Err(PalwStateV2Error::ReadinessProofRefused(
                        "the composite record does not root to the class's registered artifact root".to_string(),
                    ));
                }
                record.adapter_root
            }
            _ => class.artifact_root,
        };
        // **Lane MU (ADR-0173, `palw_audit_1004_v1`): the proof is verified against the root it RECONSTRUCTS, and that root must be
        // one the class has in force** — the registered root (the legacy `(bond, class)` row, as before) or a root a model line
        // published (a `(bond, class, root)` row). Below the fence the proof opens the registered root alone, byte for byte.
        let mut other_root: Option<Hash64> = None;
        if self.params.audit_1004_active_at(ctx.daa_score) {
            let reconstructed = crate::palw_artifact::reconstruct_artifact_multiproof_root_v1(proof)
                .map_err(|e| PalwStateV2Error::ReadinessProofRefused(format!("{e}")))?;
            if reconstructed != possession_root {
                if self.state.improvement_composite_class(class_id).is_some()
                    || !self.state.class_roots_in_force(class_id, ctx.daa_score).contains(&reconstructed)
                {
                    return Err(PalwStateV2Error::ReadinessProofRefused(
                        "the proof reconstructs no root the class has in force".to_string(),
                    ));
                }
                other_root = Some(reconstructed);
            }
        } else {
            crate::palw_artifact::verify_artifact_multiproof_v1(proof, possession_root)
                .map_err(|e| PalwStateV2Error::ReadinessProofRefused(format!("{e}")))?;
        }
        let span_now = crate::palw_execution_lane_v1::palw_execution_span_v1(ctx.daa_score, span_daa);
        let landing = registry::palw_readiness_landing_spans_v1(span_daa);
        if span > span_now || span_now - span > landing {
            return Err(PalwStateV2Error::ReadinessProofRefused(format!(
                "the proof names span {span} at span {span_now} (a proof lands within {landing} spans)"
            )));
        }
        // Past the readiness horizon's fence a proof no newer than the row is a replay: refused before
        // the row or the pool's landing record is touched.
        match &other_root {
            None => self.refuse_readiness_proof_not_newer_v1(bond, class_id, span)?,
            Some(root) => {
                if self.params.readiness_v2_max_age_spans().is_some()
                    && let Some(row) = self.state.seat_root_readiness.get(&(*bond, *class_id, *root))
                    && span <= row.proved_span
                {
                    return Err(PalwStateV2Error::ReadinessProofNotNewer {
                        bond: *bond,
                        class_id: *class_id,
                        span,
                        row_span: row.proved_span,
                    });
                }
            }
        }
        let bond_bytes = borsh::to_vec(bond).expect("a bond key is borsh-serializable");
        let seed = registry::palw_readiness_v2_challenge_seed_v1(class_id, &bond_bytes, span);
        // The first V2 producer opened all sixteen leaves under a 1 MiB ceiling. Preserve that
        // exact historical form while replaying it; the carrier-bounded prefix rule applies only
        // to new proofs. This keeps an upgraded node's PALW state identical to the node that
        // accepted a pre-upgrade block.
        let leaf_index = if legacy_full_challenge {
            let wanted = registry::palw_readiness_v2_leaves_v1(&seed, proof.leaf_count);
            if proof.opened_indices() != wanted {
                return Err(PalwStateV2Error::ReadinessProofRefused(format!(
                    "the legacy full proof opens {:?} of {} leaves; this span's challenge names {:?}",
                    proof.opened_indices(),
                    proof.leaf_count,
                    wanted
                )));
            }
            wanted.first().copied().unwrap_or(0)
        } else {
            // New proofs open only the deterministic prefix that fits one carrier.
            let draw = registry::palw_readiness_v2_draw_v1(&seed, proof.leaf_count);
            let opened: Vec<(u32, usize)> = proof.opened.iter().map(|(index, operand)| (*index, operand.bytes.len())).collect();
            registry::palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened)
                .map_err(PalwStateV2Error::ReadinessProofRefused)?;
            draw.first().copied().unwrap_or(0)
        };
        let row = registry::PalwSeatReadinessRowV1 {
            proved_daa: span.saturating_mul(span_daa.max(1)),
            proved_span: span,
            leaf_index,
            proof_version: 2,
            chunks: proof.opened.len().min(u32::MAX as usize) as u32,
        };
        if let Some(root) = other_root {
            // A non-founding root: its own row, and this seat's rows for roots that have left force are dropped with it (the table
            // is bounded by the roots in force, not by every version ever published).
            let in_force = self.state.class_roots_in_force(class_id, ctx.daa_score);
            let lo = (*bond, *class_id, Hash64::from_bytes([0u8; 64]));
            let hi = (*bond, *class_id, Hash64::from_bytes([0xffu8; 64]));
            let stale: Vec<(PalwBondKeyV2, Hash64, Hash64)> = self
                .state
                .seat_root_readiness
                .range(lo..=hi)
                .map(|(key, _)| *key)
                .filter(|(_, _, held)| !in_force.contains(held))
                .collect();
            for key in stale {
                self.write_seat_root_readiness(key, None)?;
            }
            self.write_seat_root_readiness((*bond, *class_id, root), Some(row))?;
            return Ok(());
        }
        self.write_seat_readiness((*bond, *class_id), Some(row))?;
        self.note_activation_readiness_landing_v1(class_id, bond, span_now)?;
        Ok(())
    }

    /// **ADR-0152-adjacent (Activation Pool), the fix round's F4: the span a readiness proof LANDED
    /// in.** A readiness row is dated by the span its proof names, and a proof may land up to the
    /// landing allowance after it — so under the anchor clock a proof built after the S − 1 anchor
    /// existed can name S − 2 and still land inside S − 1. (a) reads this record instead. Written by
    /// both proof paths (V1 and V2), only for a live `Candidate` with a pool (the one kind of class
    /// (a) audits), past `palw_activation_pool`; the latest proof wins, as it does for the row.
    pub(super) fn note_activation_readiness_landing_v1(&mut self, class_id: &Hash64, bond: &PalwBondKeyV2, span_now: u64) -> Result<(), PalwStateV2Error> {
        if self.extras.activation_pool.is_some()
            && self.state.classes.get(class_id).is_some_and(|class| matches!(class.status, PalwClassStatusV2::Active))
            && self.state.activation_pools.contains_key(class_id)
            && self
                .state
                .model_lifecycles
                .get(class_id)
                .is_some_and(|row| matches!(row.state, crate::palw_model_registry_v1::PalwModelLifecycleV1::Candidate))
        {
            self.write_activation_readiness_landed((*class_id, *bond), Some(span_now))?;
        }
        Ok(())
    }

    pub(super) fn open_model_lifecycle(
        &mut self,
        ctx: &PalwBlockContextV2,
        class_id: Hash64,
        work: &dyn Fn() -> Option<crate::palw_model_registry_v1::PalwModelWorkV1>,
        initial_target: u128,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_model_registry_v1 as registry;
        let Some(fold) = self.model_registry_fold().cloned() else { return Ok(()) };
        // **ADR-0152-adjacent (Activation Pool), the fix round's L4: a re-registration re-enters as a
        // registration.** The arm that calls this is reached by a new class or by a Dormant one being
        // registered again, and R2 stops stepping a Dormant class's row — so the row a re-registration
        // finds is the one its class held when it went quiet (`Active`, say), and it would stand for a
        // span before the step re-read it, skipping the jury a registration owes. Past the fence the
        // row is rewritten to the entry state below; before it, an existing row is kept, as always.
        if self.state.model_lifecycles.contains_key(&class_id) && self.extras.activation_pool.is_none() {
            return Ok(());
        }
        let span_now = crate::palw_execution_lane_v1::palw_execution_span_v1(ctx.daa_score, fold.span_daa);
        let work = work();
        // The 2026-09-25 sweep's V03(2)/V05: a row rewritten to its entry state starts no probation
        // it had, so whatever the old row's probation remembered goes with it.
        if self.state.probation_memory.contains_key(&class_id) {
            self.write_probation_memory_v1(class_id, None)?;
        }
        // **Registered is existence, not eligibility** (ADR-0145 §7, past the fence). A class a
        // registrant bought opens CANDIDATE: it holds a row, it can be read and benchmarked, and
        // it admits no claim and bears no weight until a seat outside the registrant's own
        // identity has proved it is ready to verify it. A class with no registrant bond is one the
        // genesis assembly registered, and it opens exactly as it always did — there is nobody for
        // its seats to be independent of, and the shipped registry has no slack for a new gate.
        let bought = self.state.classes.get(&class_id).is_some_and(|record| record.registrant_bond.is_some());
        let state = if self.extras.admission_independence_at(ctx.daa_score) && bought {
            registry::PalwModelLifecycleV1::Candidate
        } else if work.is_some_and(|w| w.ops_supported) {
            registry::PalwModelLifecycleV1::Prefetching
        } else {
            registry::PalwModelLifecycleV1::Registered
        };
        let work = work.unwrap_or_default();
        let expected = crate::palw_economic_compute_v1::palw_expected_attempts_q32_v1(initial_target.max(1));
        let profile =
            registry::palw_lifecycle_profile_v1(&work, expected, &fold.globals, self.extras.seat_gate_possession_at(ctx.daa_score));
        self.write_model_lifecycle(
            class_id,
            Some(registry::PalwModelLifecycleRowV1 {
                state,
                work,
                profile,
                since_span: span_now,
                probes_passed: 0,
                probes_failed: 0,
                probes_passed_this_span: 0,
                probes_failed_this_span: 0,
                ready_seats: 0,
                inflight_claims: 0,
                utilization_permille: 0,
                admission_milli: 0,
                cap_utilization_permille: 0,
                priced_share_permille: 0,
            }),
        )?;
        Ok(())
    }

    /// The seats ready for a class now (ADR-0135 Decision 4): active, above the floor, with a fresh
    /// possession proof, and free collateral for the readiness multiple of the network's floor.
    pub(super) fn model_registry_ready_seats(
        &self,
        class_id: &Hash64,
        now_daa: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
    ) -> u32 {
        self.read().model_registry_ready_seats(class_id, now_daa, fold)
    }

    /// **Is this bond READY for the class now** — the registry's five-clause predicate, spelled
    /// once. The count above and ADR-0147's admission jury both read it, because "ready" must mean
    /// one thing: a jury that admitted a class on seats the draw could not seat, or held one out on
    /// seats it could, would be a second answer to the same question.
    pub(super) fn model_registry_seat_is_ready(
        &self,
        bond_key: &PalwBondKeyV2,
        bond: &PalwBondStateV2,
        class_id: &Hash64,
        now_daa: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
    ) -> bool {
        self.read().model_registry_seat_is_ready(bond_key, bond, class_id, now_daa, fold)
    }

    /// **ADR-0147: did this `Candidate` class's admission jury sit at this boundary, and does a
    /// majority of it hold the class?** (`admission_jury_seated` in the observation.)
    ///
    /// Four things make the jury independent of the registrant, and each is a structural fact
    /// rather than an identity comparison:
    ///
    /// * **the population is the network's**: every active bond above the floor that serves the
    ///   liveness floor (`palw_bond_may_judge_class_v2` for the base class) — the set every panel
    ///   of the class every node runs is drawn from — minus the registrant's own bond and operator,
    ///   the one party the chain KNOWS holds this class;
    /// * **it is fixed before its randomness**: only bonds registered before the span whose anchor
    ///   seeds the draw began, so no bond in it was registered by a party that had seen the seed —
    ///   without this a registrant grinds keys offline for jury tickets below everyone else's;
    /// * **the randomness costs inference**: the execution lane's seed anchor of the span before
    ///   (ADR-0130), whose re-roll is a winning attempt, not a header;
    /// * **the draw is rationed**: an audit falls once per period ([`palw_admission_audit_due_v1`]),
    ///   so a registrant cannot wait out the odds span by span.
    ///
    /// No anchor for the span before — no attempt was admitted in it — is no audit: the class stays
    /// `Candidate` until an audit span that has one. A short jury (fewer operators in the population
    /// than seats) admits nothing. Both are the fail-closed direction.
    ///
    /// **It returns the jury itself, not only the verdict** (ADR-0152-adjacent: Activation Pool):
    /// `None` where no jury sat (not an audit span, no anchor for the span before, a short
    /// population), else the verdict and each drawn operator with its population bonds in key order,
    /// so the pool's (a) reads the same draw the verdict was taken from and asks its stricter question
    /// of the same bonds. The verdict is the one it always was, predicate for predicate in the same
    /// order.
    pub(super) fn admission_jury_v1(
        &self,
        class_id: &Hash64,
        ctx: &PalwBlockContextV2,
        span_now: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
    ) -> Option<PalwAdmissionJuryV1> {
        use crate::palw_model_registry_v1::{
            palw_admission_audit_due_v1, palw_admission_audit_period_spans_v2, palw_admission_jury_quorum_v1,
            palw_admission_jury_seed_v1,
        };
        let period = palw_admission_audit_period_spans_v2(self.params.epoch_length, fold.span_daa, fold.admission_audit_period_daa);
        // **R2 (ADR-0152-adjacent: Activation Pool; the review's M2): past `palw_activation_pool` a
        // class meets its jury at ITS OWN span of the period** (`(span + H(class_id)) mod period ==
        // 0`), the seed still the anchor of the span before. One jury per period as before; the
        // network's audits, and the readiness proofs aimed at them, stop landing in one span.
        let due = if self.extras.activation_pool.is_some() {
            crate::palw_activation_pool_v1::palw_admission_audit_due_staggered_v1(class_id, span_now, period)
        } else {
            palw_admission_audit_due_v1(span_now, period)
        };
        if !due {
            return None;
        }
        // **ADR-0170 M3: past `palw_anchor_window_v1` the jury's randomness is the latest anchor of the window `S − W … S − 1`**
        // (the span before's alone below it), and its population is cut at that ANCHOR's span, so a bond registered after the seed
        // existed is not on the jury it seeds. The audit's own span, one a period at the class's stagger, is untouched.
        let window = self.params.anchor_window_spans_at(ctx.daa_score);
        let anchor = self.state.round_seed_anchor.as_ref().filter(|anchor| match window {
            Some(window) => crate::palw_anchor_window_v1::palw_anchor_window_admits_v1(anchor.span, span_now, window),
            None => anchor.span.saturating_add(1) == span_now,
        })?;
        let cut_span =
            if window.is_some() { crate::palw_anchor_window_v1::palw_anchor_window_jury_cut_span_v1(anchor.span) } else { span_now };
        // Past the pool's fence the seed leaves the anchor's block hash out (F2: its producer re-rolls
        // the hash for free, and a jury chosen is a set of (a)'s payees chosen).
        let seed = if self.extras.activation_pool.is_some() {
            crate::palw_model_registry_v1::palw_admission_jury_seed_v2(class_id, span_now, &anchor.execution_key)
        } else {
            palw_admission_jury_seed_v1(class_id, span_now, &anchor.block, &anchor.execution_key)
        };
        // **The base population** — one function (`palw_base_population_v1`), the jury's filter extracted, so the jury, the
        // per-claim outsider draw and the seating floor (RFC-0002 Part II Proposal A) read ONE population: past the Activation
        // Pool the bonds a panel can draw (collateral at the panel floor, 130,000 MSK on testnet-12), serving the liveness
        // floor's class, registered before the span's cut, matured past `palw_bond_maturity_early`, and the class's
        // registrant's bond and operator removed. Below the pool: the network floor, as before.
        let population: Vec<(&PalwBondKeyV2, &PalwBondStateV2)> = palw_class_seating_v1::palw_base_population_v1(
            &self.state,
            self.params,
            fold,
            class_id,
            ctx.daa_score,
            cut_span,
            self.extras.activation_pool.is_some(),
        );
        let seats = fold.globals.seat_count.max(1);
        let jury = crate::palw_panel_v2::palw_admission_jury_v1(&seed, &population, seats);
        if jury.len() < seats as usize {
            return None;
        }
        let drawn: Vec<(Hash64, Vec<PalwBondKeyV2>)> = jury
            .iter()
            .map(|operator| {
                (*operator, population.iter().filter(|(_, bond)| bond.operator_id == *operator).map(|(key, _)| **key).collect())
            })
            .collect();
        let ready = drawn
            .iter()
            .filter(|(_, bonds)| {
                bonds.iter().any(|key| {
                    self.state
                        .bonds
                        .get(key)
                        .is_some_and(|bond| self.model_registry_seat_is_ready(key, bond, class_id, ctx.daa_score, fold))
                })
            })
            .count();
        Some(PalwAdmissionJuryV1 { seated: ready >= palw_admission_jury_quorum_v1(seats) as usize, drawn, anchor_span: anchor.span })
    }

    /// **ADR-0152-adjacent (Activation Pool): one class's pool at its span step** (user decision
    /// 2026-09-25; the review's §4), past `Params::palw_activation_pool` and only for a class with a
    /// pool — read after the row's own step, with the state it was in (`before`) and the state it is
    /// in now (`after`):
    ///
    /// * **(a)** at a `Candidate`'s audit, whatever the jury decided ([`Self::schedule_activation_prep_v1`]);
    /// * **`prep` becomes `bonus` once the class is past `Candidate`.** (a) is paid at a Candidate's
    ///   audit and nowhere else, so a preparation budget outside `Candidate` has no payee left: left
    ///   in place it would be sponsors' MSK that no rule could ever pay (conserved, and dead). Moved,
    ///   it waits behind verified service — (b) — which is where the review puts "the bulk of the
    ///   pay". This is the design's own seating rule (B2, "bonus += prep"), kept because the review's
    ///   minimal spec keeps (a) Candidate-only; a later top-up of a class past `Candidate` is wholly
    ///   bonus for the same reason ([`crate::palw_activation_pool_v1::palw_activation_inflow_split_v1`]).
    ///   The class's readiness landings leave with its `Candidate` state (F4);
    /// * **(b)'s run opens when the row ENTERS `Probation`** (from `Prefetching` or `Held`):
    ///   `probe_credited` restarts, so a re-formation credits the operators of ITS probation;
    /// * **(b) is scheduled once the row is `ActiveLimited` or `Active` with credited operators
    ///   waiting** — at `Probation → ActiveLimited` itself, and at each later span an empty bonus
    ///   deferred it to ([`Self::schedule_activation_bonus_v1`]).
    ///
    /// **Nothing here writes the payout queue** (the fix round's F5): a decided payment moves from its
    /// budget into the row's `scheduled_sompi` and the `(class, payee)` side map, and step 3d′ flushes
    /// it after the vesting moves ([`apply_activation_pool_flush_v1`]). So nothing here can fail or
    /// leave a half-written payment (the fix round's L1): every amount is computed first, and every
    /// write is a map entry whose sum is bounded by the row's `funded_sompi`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn step_activation_pool_v1(
        &mut self,
        class_id: &Hash64,
        ctx: &PalwBlockContextV2,
        span_now: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
        terms: &crate::palw_activation_pool_v1::PalwActivationPoolTermsV1,
        before: crate::palw_model_registry_v1::PalwModelLifecycleV1,
        after: crate::palw_model_registry_v1::PalwModelLifecycleV1,
        jury: Option<&PalwAdmissionJuryV1>,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_model_registry_v1::PalwModelLifecycleV1 as L;
        let Some(mut pool) = self.state.activation_pools.get(class_id).cloned() else { return Ok(()) };
        let registrant_operator = self
            .state
            .classes
            .get(class_id)
            .and_then(|record| record.registrant_bond)
            .and_then(|key| self.state.bonds.get(&key))
            .map(|bond| bond.operator_id);
        if matches!(before, L::Candidate)
            && let Some(jury) = jury
        {
            self.schedule_activation_prep_v1(class_id, &mut pool, ctx, span_now, fold, terms, jury, registrant_operator)?;
        }
        if !matches!(after, L::Candidate) {
            if pool.prep_sompi > 0 {
                pool.bonus_sompi = pool.bonus_sompi.saturating_add(pool.prep_sompi);
                pool.prep_sompi = 0;
            }
            if matches!(before, L::Candidate) {
                self.purge_activation_readiness_landed(class_id)?;
            }
        }
        if matches!(after, L::Probation { .. }) && !matches!(before, L::Probation { .. }) {
            pool.probe_credited.clear();
        }
        if matches!(after, L::ActiveLimited { .. } | L::Active) && !pool.probe_credited.is_empty() {
            self.schedule_activation_bonus_v1(class_id, &mut pool, terms, registrant_operator)?;
        }
        self.write_activation_pool(*class_id, pool)?;
        Ok(())
    }

    /// **(a), the preparation reward** (the review's §4 with the user's panel-floor decision): at a
    /// `Candidate`'s own audit, each drawn juror `o` — in draw order — is owed
    /// `a = min(A_MAX(age), ⌊prep / 10⌋)` once per (class, `o`) iff one of its population bonds, the
    /// lowest key that qualifies:
    ///
    /// * holds collateral of at least the PANEL floor (ten network floors — the bonds a panel can
    ///   draw, the outsider seat included; the review's C3 and A2-i, the user's decision), by the
    ///   draw's own predicate (`palw_bond_may_take_work_v2`);
    /// * has its latest readiness proof LANDED at span `S − 2` or earlier (the review's M6 as the fix
    ///   round's F4 corrected it): the span the fold folded the proof in, recorded in
    ///   `activation_readiness_landed`, not the span the proof names — a proof may land up to 40 spans
    ///   after the span it names, so under the anchor clock one built after the S − 1 anchor existed
    ///   could name S − 2 and land in S − 1. Landing at `S − 2` puts it in a block before any block of
    ///   S − 1, and so before the seed. A juror whose latest proof landed in S − 1 is not paid at this
    ///   audit (a node re-proving every few spans is in that position about one audit in four);
    /// * is READY by the registry's one predicate (fresh, active, the readiness multiple free);
    ///
    /// and `o` is not the registrant's operator (the population already excludes it; asked again
    /// so the rule does not rest on the draw's filter). **Whatever the verdict**: the paid fact is the
    /// verified preparation, and a quorum that failed pays its prepared jurors exactly what a seated
    /// one does. At most `seat_count × a` an audit, never past `prep`, never past the payee cap. The
    /// payment is scheduled (F5); the queue's room decides only when it is flushed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn schedule_activation_prep_v1(
        &mut self,
        class_id: &Hash64,
        pool: &mut crate::palw_activation_pool_v1::PalwActivationPoolV1,
        ctx: &PalwBlockContextV2,
        span_now: u64,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
        terms: &crate::palw_activation_pool_v1::PalwActivationPoolTermsV1,
        jury: &PalwAdmissionJuryV1,
        registrant_operator: Option<Hash64>,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_activation_pool_v1::{palw_activation_prep_reward_v1, palw_sorted_insert_v1};
        let a = palw_activation_prep_reward_v1(terms, pool.prep_sompi, ctx.daa_score.saturating_sub(pool.opened_daa));
        // (a) pays a proof that LANDED before the seed existed: two spans before the audit for the anchor of the span before it, as always, and
        // one span before an older anchor's span past `palw_anchor_window_v1` (a juror that sees the seed knows whether it is drawn).
        let Some(landed_by) = crate::palw_anchor_window_v1::palw_anchor_window_prep_landed_by_v1(span_now, jury.anchor_span) else {
            return Ok(());
        };
        if a == 0 {
            return Ok(());
        }
        let panel_floor = crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(self.params.min_collateral_sompi());
        let slots = (terms.prep_payee_cap as usize).saturating_sub(pool.prep_paid.len()).min((pool.prep_sompi / a) as usize);
        let mut payees: Vec<(Hash64, Hash64)> = Vec::new();
        for (operator, bonds) in &jury.drawn {
            if payees.len() >= slots {
                break;
            }
            if Some(*operator) == registrant_operator || pool.prep_paid.binary_search(operator).is_ok() {
                continue;
            }
            let qualifying = bonds.iter().find_map(|key| {
                let bond = self.state.bonds.get(key)?;
                let landed = self.state.activation_readiness_landed.get(&(*class_id, *key)).copied()?;
                (palw_bond_may_take_work_v2(bond, panel_floor)
                    && landed <= landed_by
                    && self.model_registry_seat_is_ready(key, bond, class_id, ctx.daa_score, fold))
                .then_some(bond.payout_payload)
            });
            if let Some(payload) = qualifying {
                payees.push((*operator, payload));
            }
        }
        for (operator, payload) in &payees {
            self.schedule_activation_payout(*class_id, *payload, a)?;
            palw_sorted_insert_v1(&mut pool.prep_paid, *operator);
        }
        let total = a.saturating_mul(payees.len() as u64);
        pool.prep_sompi -= total;
        pool.scheduled_sompi = pool.scheduled_sompi.saturating_add(total);
        Ok(())
    }

    /// **(b), the activation bonus** (the review's A1 fix): each credit in `probe_credited` whose
    /// operator is not the registrant's and was never paid (b) for this class — in the order of the
    /// CREDITED bond's registration, then its operator id (a key cannot be ground for a place: the
    /// design's order), up to the payee cap — is owed `b = ⌊bonus × β / n⌋`, paid to the credited
    /// bond's own payload (the fix round's L3: the seat the chain credited, not the operator's oldest
    /// bond). The list is spent when scheduled (and when it can never pay: the cap reached, nobody
    /// eligible). Deferred, with the list kept, when `b` is zero — a sponsor's later top-up pays these
    /// operators. The queue's room decides only when it is flushed (F5).
    pub(super) fn schedule_activation_bonus_v1(
        &mut self,
        class_id: &Hash64,
        pool: &mut crate::palw_activation_pool_v1::PalwActivationPoolV1,
        terms: &crate::palw_activation_pool_v1::PalwActivationPoolTermsV1,
        registrant_operator: Option<Hash64>,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_activation_pool_v1::{palw_activation_bonus_reward_v1, palw_sorted_insert_v1};
        if palw_activation_bonus_reward_v1(terms, pool.bonus_sompi, 1) == 0 {
            return Ok(());
        }
        let mut payees: Vec<(u64, Hash64, Hash64)> = pool
            .probe_credited
            .iter()
            .filter(|credit| Some(credit.operator) != registrant_operator && pool.bonus_paid.binary_search(&credit.operator).is_err())
            .filter_map(|credit| {
                self.state.bonds.get(&credit.bond).map(|bond| (bond.registered_daa, credit.operator, bond.payout_payload))
            })
            .collect();
        payees.sort();
        payees.truncate((terms.bonus_payee_cap as usize).saturating_sub(pool.bonus_paid.len()));
        if payees.is_empty() {
            pool.probe_credited.clear();
            return Ok(());
        }
        let b = palw_activation_bonus_reward_v1(terms, pool.bonus_sompi, payees.len());
        if b == 0 {
            return Ok(());
        }
        for (_, operator, payload) in &payees {
            self.schedule_activation_payout(*class_id, *payload, b)?;
            palw_sorted_insert_v1(&mut pool.bonus_paid, *operator);
        }
        let total = b.saturating_mul(payees.len() as u64);
        pool.bonus_sompi -= total;
        pool.scheduled_sompi = pool.scheduled_sompi.saturating_add(total);
        pool.probe_credited.clear();
        Ok(())
    }

    /// Claims of a class still in flight (accepted and not terminal): its attempts and — past the
    /// 2026-09-23 audit fence — its free-prompt claims (audit #11, below).
    ///
    /// **Why a free-prompt claim counts.** It is drawn a panel from the same ready seats, its
    /// seats replay the same model, and it holds the same seat duty until it is terminal; the
    /// only thing it does not do is carry weight of its own. Leaving it out made the lane a way
    /// to put verification work on a class's panel that ADR-0137's budget never saw — a held
    /// class's whole ready set could be kept busy by commitments while the room check still
    /// read an empty panel. One claim counts as one of the class's claims, the unit the budget
    /// prices (`economic_ccu_per_claim`); a job-size weighting would need the free-prompt price's
    /// compute and the registry's CCU to be one unit, which nothing on this chain asserts yet.
    /// Below the fence the answer is the attempt count it always was.
    pub(super) fn model_registry_inflight(&self, class_id: &Hash64) -> u32 {
        self.read().model_registry_inflight(class_id)
    }

    /// **The class-local gate at acceptance** (ADR-0135 Decision 6 and ADR-0132 F1): the base class
    /// is never gated (every node runs it, the chain lives on it); a class without a row (registered
    /// before the fence without its carriage) is not gated either; a rowed class must admit claims
    /// and be under its inflight cap.
    /// ADR-0137: `W₀` for this block where the work target is in force — the block's escrow at the
    /// payout's rate (the shadow's rate where, in a test, no payout folds).
    /// **What this block prices a free-prompt claim's receipt rights from** (the audit's #5): its worker
    /// carve and `W₀` itself — never the stepped `W` above it, because the pooled target may ease back
    /// toward `W₀` after acceptance and a bound priced at a higher `W` would under-reserve then. `None`
    /// below the audit fence or where the work target is not in force.
    pub(super) fn fp_receipt_rights_inputs_v1(&self, ctx: &PalwBlockContextV2) -> Option<PalwFpRightsInputsV1> {
        if !self.extras.audit_2026_09_23_active || !self.extras.work_target_active {
            return None;
        }
        let rate = self
            .extras
            .economic_payout
            .map(|fold| fold.rate_sompi_per_giga)
            .or_else(|| self.extras.work_target.as_ref().map(|fold| fold.rate_sompi_per_giga))
            .unwrap_or(crate::palw_work_target_v1::PALW_WORK_TARGET_SHADOW_RATE_SOMPI_PER_GIGA_V1);
        Some(PalwFpRightsInputsV1 {
            worker_carve: worker_carve_v2(self.params, ctx.subsidy, self.extras.escrow_carve),
            work_floor: palw_work_floor_for_block_v1(self.params, ctx.subsidy, self.extras.escrow_carve, rate),
        })
    }

    pub(super) fn work_target_floor(&self, ctx: &PalwBlockContextV2) -> Option<u128> {
        self.read().work_target_floor(ctx)
    }

    /// **ADR-0137 D5: the verification budget in one class's claims** — this class's ready seats'
    /// replay over the shortest admitted window, less every model class's replay in flight,
    /// divided by what one of its claims costs the panel. Returns `(room, inflight_replay,
    /// budget, horizon)`.
    pub(super) fn panel_room_v1(
        &self,
        class_id: &Hash64,
        row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
        fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
        now_daa: u64,
    ) -> (u64, u128, u128, u64) {
        self.read().panel_room_v1(class_id, row, fold, now_daa)
    }

    /// `incoming` is the claim asked about ([`PalwGatedClaimV1`]): an attempt, or past the fence a
    /// free-prompt commitment of so many quanta, which the rate room pools with its class.
    pub(super) fn check_class_admits_claim(
        &self,
        class_id: &Hash64,
        now_daa: u64,
        incoming: PalwGatedClaimV1,
    ) -> Result<(), PalwStateV2Error> {
        self.read().check_class_admits_claim(class_id, now_daa, incoming)
    }

    /// [`PalwFoldReadV1::check_class_verify_admits_v1`] (§4-quater V2), on this fold's inputs.
    pub(super) fn check_class_verify_admits_v1(
        &self,
        class_id: &Hash64,
        now_daa: u64,
        shape: crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1,
        per_claim: bool,
    ) -> Result<(), PalwStateV2Error> {
        self.read().check_class_verify_admits_v1(class_id, now_daa, shape, per_claim)
    }

    /// ADR-0152 T-2(a)'s per-bond share (S-6), past `Params::palw_rcore_plus` only — see
    /// [`PalwFoldReadV1::check_bond_class_share`].
    pub(super) fn check_bond_class_share(
        &self,
        bond: &PalwBondKeyV2,
        class_id: &Hash64,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        self.read().check_bond_class_share(bond, class_id, now_daa)
    }

    /// ADR-0160 F-R's floor room (J-6) — see [`PalwFoldReadV1::check_floor_room_v1`].
    pub(super) fn check_floor_room_v1(
        &self,
        bond: &PalwBondKeyV2,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        self.read().check_floor_room_v1(bond, claim_id, claim, now_daa)
    }

    /// Why a bind window closed: under the registry, a rowed class whose ready seats cannot fill a
    /// panel voids as `NoCapablePanel` — the capacity's failure, named — otherwise `BindTimeout`.
    pub(super) fn bind_timeout_reason(&self, claim: &PalwClaimStateV2, ctx: &PalwBlockContextV2) -> PalwVoidReasonV2 {
        if self.class_cannot_seat_a_panel_v1(claim, ctx) {
            return PalwVoidReasonV2::NoCapablePanel;
        }
        PalwVoidReasonV2::BindTimeout
    }

    /// **`NoCapablePanel`'s test, on the builder's state as it stands**: the registry governs, the
    /// claim's class is a rowed non-base class, and its ready seats are fewer than a panel. The one
    /// spelling [`Self::bind_timeout_reason`] labels a void with and lane F1's step-4c re-anchor
    /// (V03(1)) asks — the latter on the anchor block's pre-object base, the state the draw read.
    pub(super) fn class_cannot_seat_a_panel_v1(&self, claim: &PalwClaimStateV2, ctx: &PalwBlockContextV2) -> bool {
        self.model_registry_fold().is_some_and(|fold| {
            fold.governs_at(ctx.daa_score)
                && claim.class_id != self.params.base_class_id()
                && self.state.model_lifecycles.contains_key(&claim.class_id)
                && self.model_registry_ready_seats(&claim.class_id, ctx.daa_score, fold) < fold.globals.seat_count as u32
        })
    }

    /// A probe of a class under the registry: a `Final` passes, a fault fails; counted for the span.
    /// `now_daa` is the DAA of the block that notes it (lane F1 reads its span).
    pub(super) fn note_model_probe(&mut self, claim: &PalwClaimStateV2, passed: bool, now_daa: u64) -> Result<(), PalwStateV2Error> {
        let Some(span_daa) = self.model_registry_fold().map(|fold| fold.span_daa) else { return Ok(()) };
        if !matches!(claim.source, PalwClaimSourceV2::Attempt) {
            return Ok(());
        }
        let Some(row) = self.state.model_lifecycles.get(&claim.class_id).cloned() else { return Ok(()) };
        // **Lane F1 (the 2026-09-25 sweep's V05): past the registry-resilience fence a failed probe
        // names its producer bond** in the class's probation memory while the class is in its
        // probation (`Probation`, or the readiness hold that keeps it) — iff the fence governs the span
        // the probe is counted in, the span the step will judge it with. The span step resets the
        // probation only once two distinct bonds are named (`palw_lifecycle_step_resilient_v1`); the
        // counters below still count every fault, for the reader and for `ActiveLimited`'s stable span.
        if !passed
            && self.params.registry_resilience_governs_span_v1(
                crate::palw_execution_lane_v1::palw_execution_span_v1(now_daa, span_daa),
                span_daa,
            )
        {
            let mut memory = self.state.probation_memory.get(&claim.class_id).cloned().unwrap_or_default();
            if memory.in_probation(row.state) && memory.note_failed_bond(claim.bond) {
                self.write_probation_memory_v1(claim.class_id, Some(memory))?;
            }
        }
        let mut next = row;
        if passed {
            next.probes_passed_this_span = next.probes_passed_this_span.saturating_add(1);
            next.probes_passed = next.probes_passed.saturating_add(1);
        } else {
            next.probes_failed_this_span = next.probes_failed_this_span.saturating_add(1);
            next.probes_failed = next.probes_failed.saturating_add(1);
        }
        self.write_model_lifecycle(claim.class_id, Some(next))?;
        Ok(())
    }

    /// **ADR-0152-adjacent (Activation Pool), (b)'s tracking: the operators credited on a probe
    /// `Final`** (the review's A1). While a class's row is in `Probation`, each attempt `Final` of it
    /// adds the operators of its credited seats — the seats that filed `Valid` in time, the ADR-0147
    /// outsider included — to its pool's `probe_credited`, sorted, at most `probation_claims ×
    /// seat_count`. Those are the operators strangers verified serving the class: what (b) pays at
    /// `Probation → ActiveLimited`, instead of a population the registrant fills itself. Past
    /// `Params::palw_activation_pool` only, and only for a class with a pool.
    pub(super) fn note_activation_probe_credits_v1(&mut self, claim_id: &Hash64, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        use crate::palw_activation_pool_v1::{PALW_ACTIVATION_PROBE_CREDITED_MAX_V1, palw_activation_probe_credit_cap_v1};
        if self.extras.activation_pool.is_none() || !matches!(claim.source, PalwClaimSourceV2::Attempt) {
            return Ok(());
        }
        if !self
            .state
            .model_lifecycles
            .get(&claim.class_id)
            .is_some_and(|row| matches!(row.state, crate::palw_model_registry_v1::PalwModelLifecycleV1::Probation { .. }))
        {
            return Ok(());
        }
        let Some(mut pool) = self.state.activation_pools.get(&claim.class_id).cloned() else { return Ok(()) };
        let Some(fold) = self.model_registry_fold() else { return Ok(()) };
        let cap = palw_activation_probe_credit_cap_v1(fold.globals.probation_claims as usize, fold.globals.seat_count as usize)
            .min(PALW_ACTIVATION_PROBE_CREDITED_MAX_V1);
        let Some(duties) = self.state.panel_duties.get(claim_id) else { return Ok(()) };
        let credited: Vec<crate::palw_activation_pool_v1::PalwActivationCreditV1> = duties
            .seats
            .iter()
            .filter(|(_, at)| **at != 0)
            .filter_map(|(seat, _)| {
                self.state.bonds.get(seat).map(|bond| crate::palw_activation_pool_v1::PalwActivationCreditV1 {
                    operator: bond.operator_id,
                    bond: *seat,
                    claim: *claim_id,
                })
            })
            .collect();
        let mut changed = false;
        for credit in credited {
            if pool.probe_credited.len() >= cap {
                break;
            }
            changed |= pool.credit(credit);
        }
        if changed {
            self.write_activation_pool(claim.class_id, pool)?;
        }
        Ok(())
    }

    /// **ADR-0152-adjacent (Activation Pool), the fix round's L2: a convicted probe `Final` takes its
    /// credits back.** The operators it credited toward (b) were credited for serving a claim a court
    /// has since proved false; a credit is removed only while it is still waiting in `probe_credited`
    /// — once (b) has been scheduled it is paid (a pool payout is not slashable, the design's B2), and
    /// the conviction reaches those seats through their own locks instead. An operator credited on
    /// two probe `Final`s keeps only the first credit, so convicting that one removes it: the
    /// conservative direction, and a later honest probe credits it again.
    pub(super) fn unnote_activation_probe_credits_v1(&mut self, claim_id: &Hash64, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        if self.extras.activation_pool.is_none() {
            return Ok(());
        }
        let Some(mut pool) = self.state.activation_pools.get(&claim.class_id).cloned() else { return Ok(()) };
        let before = pool.probe_credited.len();
        pool.probe_credited.retain(|credit| credit.claim != *claim_id);
        if pool.probe_credited.len() != before {
            self.write_activation_pool(claim.class_id, pool)?;
        }
        Ok(())
    }

    /// **2026-09-24 DoS audit #8: withdraw the probe pass a convicted `Final` was credited.**
    ///
    /// The inverse of [`Self::note_model_probe`]`(claim, true)` under the same gates. The running
    /// total always loses the pass. The span's counter loses it only while the registry is still
    /// observing the span the `Final` fell in: `step_model_registry` resets `_this_span` at every
    /// boundary (it runs from `rotate_round_lane`, first thing in the block), so a pass from an
    /// earlier span is no longer in that counter and subtracting it would take a pass from some
    /// other claim. Without an open lane the registry never steps and there is only one span.
    /// Saturating, as the counting side is.
    pub(super) fn unnote_model_probe_pass(&mut self, claim: &PalwClaimStateV2, final_daa: u64, now_daa: u64) -> Result<(), PalwStateV2Error> {
        if self.model_registry_fold().is_none() || !matches!(claim.source, PalwClaimSourceV2::Attempt) {
            return Ok(());
        }
        let Some(mut row) = self.state.model_lifecycles.get(&claim.class_id).cloned() else { return Ok(()) };
        row.probes_passed = row.probes_passed.saturating_sub(1);
        let same_span = self.extras.round_lane.is_none_or(|lane| {
            crate::palw_execution_lane_v1::palw_execution_span_v1(final_daa, lane.schedule_span_daa)
                == crate::palw_execution_lane_v1::palw_execution_span_v1(now_daa, lane.schedule_span_daa)
        });
        if same_span {
            row.probes_passed_this_span = row.probes_passed_this_span.saturating_sub(1);
        }
        self.write_model_lifecycle(claim.class_id, Some(row))?;
        Ok(())
    }

    /// **The registry's span step** (ADR-0135 Decisions 5 and 6), at every span boundary: rows are
    /// opened for the genesis classes from the fold's work (the base class ACTIVE and never
    /// gated), every row is observed (ready seats, claims in flight, the span's probes) and
    /// stepped, the profile is re-derived from the class's live target, and the shares of every
    /// rowed class are written from admission — the base class holding what is left, never below
    /// its floor, and a class registered before the fence without a row keeping the share it has.
    pub(super) fn step_model_registry(&mut self, ctx: &PalwBlockContextV2, span_now: u64) -> Result<(), PalwStateV2Error> {
        use crate::palw_model_registry_v1::{
            PalwLifecycleObservationV1, PalwManifestVerdictV1Flag, PalwModelLifecycleRowV1, PalwModelLifecycleV1,
            palw_lifecycle_profile_v1, palw_lifecycle_step_v1,
        };
        let Some(fold) = self.model_registry_fold().cloned() else { return Ok(()) };
        let base = self.params.base_class_id();
        let governs = fold.governs_at(ctx.daa_score);
        // **Lane F1 (V03(2)/V05): the span this step judges is the PARENT's** — every probe counted
        // since the last step was noted by a block of that span — and it is judged by the resilient
        // rules iff the registry-resilience fence governs it (in force at its first DAA), the same key
        // `note_model_probe` names a failed bond by. A block that crosses the height therefore judges
        // the span before it by the shipped rules, byte for byte.
        let resilient = self.state.last_point.is_some_and(|last| {
            self.params.registry_resilience_governs_span_v1(
                crate::palw_execution_lane_v1::palw_execution_span_v1(last.daa_score, fold.span_daa),
                fold.span_daa,
            )
        });
        let class_ids: Vec<Hash64> = self.state.classes.keys().copied().collect();
        for class_id in &class_ids {
            if self.state.model_lifecycles.contains_key(class_id) {
                continue;
            }
            // **A class the chain holds but the build cannot describe gets an INERT row** (the
            // 2026-09-18 audit's C-1 with the operator's point 4): a registration accepted before
            // the registry's fence carries its graph in block data, not in state, so no node can
            // derive its work from the chain — and leaving it row-less left a permanent "legacy"
            // share that no rowed class could use (`legacy_sum` below) and a class the work target
            // refuses with "no row" for ever. The row is `Registered` with zero work: it admits no
            // claim (`admits_claims` is false), it prices nothing, its admission is zero — so its
            // permille returns to the room at this very boundary — and it is the same row on every
            // node, because "the build cannot describe it" is a fact about the build, not the host.
            // A class that wants in registers again past the fence, where the object carries the
            // graph and `open_model_lifecycle` writes the real row in the same block.
            let work = match fold.genesis_works.get(class_id).copied() {
                Some(work) => work,
                None => {
                    self.write_model_lifecycle(
                        *class_id,
                        Some(PalwModelLifecycleRowV1 {
                            state: PalwModelLifecycleV1::Registered,
                            work: crate::palw_model_registry_v1::PalwModelWorkV1::default(),
                            profile: palw_lifecycle_profile_v1(
                                &crate::palw_model_registry_v1::PalwModelWorkV1::default(),
                                0,
                                &fold.globals,
                                self.extras.seat_gate_possession_at(ctx.daa_score),
                            ),
                            since_span: span_now,
                            probes_passed: 0,
                            probes_failed: 0,
                            probes_passed_this_span: 0,
                            probes_failed_this_span: 0,
                            ready_seats: 0,
                            inflight_claims: 0,
                            utilization_permille: 0,
                            admission_milli: 0,
                            cap_utilization_permille: 0,
                            priced_share_permille: 0,
                        }),
                    )?;
                    continue;
                }
            };
            let has_final =
                self.state.claims.values().any(|c| c.class_id == *class_id && matches!(c.phase, PalwClaimPhaseV2::Final { .. }));
            // ADR-0145 §7 past the fence: a class a registrant bought and that has produced
            // NOTHING opens CANDIDATE — existence, no claims, no weight — until an independent seat
            // is ready for it.
            //
            // **A class that already has a Final is not demoted, and that is a decision, not an
            // oversight.** This boundary is the registry's first, so the rows it opens include
            // every class the network was already running; sending one of those back to Candidate
            // would stop a live class at a flag day until somebody else's seat proves possession.
            // The finding this fence closes is about what permissionless registration INVITES, and
            // the release's own safety property is that existing classes are unaffected.
            //
            // **But a BOUGHT class does not grandfather to `Active`** — the 2026-09-19 re-audit's
            // item 7, which the paragraph above used to concede in its last sentence and which a
            // fixture now measures. `has_final` is any claim in phase `Final`, and on a chain whose
            // registry is still dormant a registrant can license its own claim with its own panel
            // and its own receipts (which the independence fence does not stop — see
            // `a_registrant_paying_its_sybils_separately_defeats_the_armed_independence_fence`).
            // One such claim, bought for a transaction fee, used to open the row at `Active` with
            // `admission_permille` 1,000 — skipping `Candidate`, `Prefetching`, `Probation` and
            // `ActiveLimited` in a single step. That is the whole lifecycle brake, bypassed by the
            // thing it exists to brake.
            //
            // The liveness argument above is honoured exactly and no further: a bought class with a
            // Final opens at `Probation`, which ADMITS CLAIMS, so nothing live stops at the flag
            // day — it simply does so at 50 permille and has to earn `Active` the way a stranger
            // does, on completed claims. The base class and the classes the network was born with
            // are untouched, because neither is bought. Below the fence this is the old rule.
            let bought = self.state.classes.get(class_id).is_some_and(|record| record.registrant_bond.is_some());
            let brake = self.extras.admission_independence_at(ctx.daa_score) && bought;
            let state = if *class_id == base || (has_final && !brake) {
                PalwModelLifecycleV1::Active
            } else if has_final {
                PalwModelLifecycleV1::Probation { probes_passed: 0 }
            } else if brake {
                PalwModelLifecycleV1::Candidate
            } else if work.ops_supported {
                PalwModelLifecycleV1::Prefetching
            } else {
                PalwModelLifecycleV1::Registered
            };
            let target = self.state.class_targets.get(class_id).map(|t| t.target).unwrap_or(u128::MAX);
            let expected = crate::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
            let profile =
                palw_lifecycle_profile_v1(&work, expected, &fold.globals, self.extras.seat_gate_possession_at(ctx.daa_score));
            self.write_model_lifecycle(
                *class_id,
                Some(PalwModelLifecycleRowV1 {
                    state,
                    work,
                    profile,
                    since_span: span_now,
                    probes_passed: 0,
                    probes_failed: 0,
                    probes_passed_this_span: 0,
                    probes_failed_this_span: 0,
                    ready_seats: 0,
                    inflight_claims: 0,
                    utilization_permille: 0,
                    admission_milli: 0,
                    cap_utilization_permille: 0,
                    priced_share_permille: 0,
                }),
            )?;
        }
        // The activation grace: rows are open and proofs are taken, but nothing is stepped and no
        // share moves until the fence is one readiness age old — the seats' time to prove.
        if !governs {
            return Ok(());
        }
        let mut admissions: Vec<(Hash64, u64)> = Vec::new();
        let rowed: Vec<Hash64> = self.state.model_lifecycles.keys().copied().collect();
        // **R2 (ADR-0152-adjacent: Activation Pool; the review's M2), past `palw_activation_pool`: a
        // row is observed only when something can come of it.** The rows of a `Dormant` or `Frozen`
        // class are not stepped at all — a Dormant class re-enters through a fresh registration and
        // a Frozen one never does, so a jury seated or a probe counted for either is an answer
        // nobody may act on (the review's M8) — and a `Candidate` row reads its ready seats and its
        // jury only at its own staggered audit span, the one boundary its step can change anything
        // at (`Candidate` leaves only on a seated jury, and admits nothing in between). Everything
        // else is stepped every span exactly as before. Each skipped row admits nothing
        // (`admission_milli` 0): a parked class admits no claim, and a Candidate's admission is
        // zero by its state. So a listing costs the span step one comparison between its audits,
        // not a walk of the bond registry — the price the 1 MSK burn has to cover (the review's A7).
        let r2 = self.extras.activation_pool.is_some();
        let audit_period = crate::palw_model_registry_v1::palw_admission_audit_period_spans_v2(
            self.params.epoch_length,
            fold.span_daa,
            fold.admission_audit_period_daa,
        );
        for class_id in &rowed {
            let row = self.state.model_lifecycles.get(class_id).cloned().expect("just listed");
            // ADR-0147: the jury sits only for a row IN `Candidate`, drawn once here — its verdict is
            // the observation's, and past `palw_activation_pool` the drawn operators are (a)'s.
            let mut jury: Option<PalwAdmissionJuryV1> = None;
            if r2 && *class_id != base {
                let parked = matches!(
                    self.state.classes.get(class_id).map(|record| &record.status),
                    Some(PalwClassStatusV2::Dormant { .. } | PalwClassStatusV2::Frozen { .. })
                );
                let between_audits = matches!(row.state, PalwModelLifecycleV1::Candidate)
                    && !crate::palw_activation_pool_v1::palw_admission_audit_due_staggered_v1(class_id, span_now, audit_period);
                if parked || between_audits {
                    admissions.push((*class_id, 0));
                    continue;
                }
            }
            let target = self.state.class_targets.get(class_id).map(|t| t.target).unwrap_or(u128::MAX);
            let expected = crate::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
            let profile =
                palw_lifecycle_profile_v1(&row.work, expected, &fold.globals, self.extras.seat_gate_possession_at(ctx.daa_score));
            // **RFC-0002 Part II Proposal A (`palw_class_seating`)**: past the fence a class's seats are counted as distinct
            // OPERATORS, once each however many bonds they hold, by the one seating function (executor-less: nothing
            // excluded), and the independence floor is read beside them; below it the bond count the registry always read.
            let (ready, independence_floor_met) = match self.params.class_seating_terms_at(ctx.daa_score) {
                Some(terms) if *class_id != base => {
                    let seating = palw_class_seating_v1(
                        &self.state,
                        self.params,
                        &fold,
                        class_id,
                        None,
                        ctx.daa_score,
                        terms,
                        self.extras.activation_pool.is_some(),
                    )
                    .expect("a seating without an executor always reads");
                    (seating.ready_operators, seating.independence_floor_met())
                }
                _ => (self.model_registry_ready_seats(class_id, ctx.daa_score, &fold), true),
            };
            let inflight = self.model_registry_inflight(class_id);
            // ADR-0132 Upgrade C / ADR-0133 Fence 3: the class's cap utilization at this boundary,
            // and whether it is under the fence's ceiling (always, where nothing prices it).
            let cap = if *class_id == base { 0 } else { self.class_cap_utilization_permille(&row.work, expected, ctx) };
            let cap_ok = self.extras.work_target_active
                || cap == 0
                || self.extras.economic_payout.is_none_or(|fold| cap <= fold.cap_utilization_max_permille as u32);
            let (state, utilization) = if *class_id == base {
                (PalwModelLifecycleV1::Active, 0)
            } else {
                let window = profile.verification_window_spans.max(1) as u128;
                let utilization = if ready == 0 {
                    1_000
                } else if self.extras.work_target_active {
                    // ADR-0137 D5: past the work target the panel's load is replay against the
                    // budget the room check reads — one notion of capacity, so a class the room
                    // admits is not a class the lifecycle holds.
                    let (_, inflight_replay, budget, _) = self.panel_room_v1(class_id, &row, &fold, ctx.daa_score);
                    if budget == 0 { 1_000 } else { (inflight_replay.saturating_mul(1_000) / budget).min(u32::MAX as u128) as u32 }
                } else {
                    ((inflight as u128).saturating_mul(fold.globals.seat_count as u128).saturating_mul(1_000)
                        / (ready as u128).saturating_mul(window))
                    .min(u32::MAX as u128) as u32
                };
                // **2026-09-24 audit #4: past `palw_audit_2026_09_23` utilization is not a lifecycle
                // input.** The room check refuses a claim the panel cannot schedule; holding the
                // CLASS for it added only hysteresis and a probe reset — and the budget is one
                // network-wide number, so a class with nothing in flight was held for another's
                // backlog and could not collect the probes that would release it. The row still
                // records the reading (`utilization_permille` below), for an operator.
                let utilization_gates = !self.extras.audit_2026_09_23_active;
                let obs = PalwLifecycleObservationV1 {
                    manifest: if row.work.ops_supported {
                        PalwManifestVerdictV1Flag::Valid
                    } else {
                        PalwManifestVerdictV1Flag::Invalid
                    },
                    ready_seats: ready,
                    independence_floor_met,
                    probes_passed_this_span: row.probes_passed_this_span,
                    probes_failed_this_span: row.probes_failed_this_span,
                    utilization_permille: if utilization_gates { utilization } else { 0 },
                    collateral_ok: ready >= profile.required_ready_seats,
                    cap_ok,
                    // ADR-0133 §11.3: the derived window against the deadline a claim of this
                    // class is actually judged by. Below the class receipt-window fence that is
                    // the global `window_receipt` — one number for every class, and a class whose
                    // replay outruns it is `overloaded` and `Held`, because stretching the global
                    // deadline would slow every claim on the chain to carry one. Past the fence
                    // the heavy class carries its OWN deadline instead
                    // (`receipt_window_for_claim_v1`), so the question this observation asks is
                    // answered by construction and the class is no longer held for it. Asked
                    // through the same helper the deadline is armed from, so the verdict and the
                    // deadline cannot disagree.
                    window_fits_receipt: (profile.verification_window_spans as u64).saturating_mul(fold.span_daa.max(1))
                        <= self.params.receipt_window_for_claim_v1(
                            &self.state,
                            class_id,
                            crate::palw_class_verify_deadline_v1::PalwClaimVerifyShapeV1::Attempt,
                            ctx.daa_score,
                        ),
                    span_stable: (!utilization_gates || utilization < 1_000)
                        && row.probes_failed_this_span == 0
                        && ready >= profile.required_ready_seats,
                    // ADR-0147: the only reading that moves a class out of `Candidate`, and the
                    // only transition it is read by. Drawn just for a row that IS in `Candidate`
                    // — a state nothing writes below the fence — so no other class pays for a
                    // walk of the bond registry at any boundary.
                    admission_jury_seated: {
                        if matches!(row.state, PalwModelLifecycleV1::Candidate) {
                            jury = self.admission_jury_v1(class_id, ctx, span_now, &fold);
                        }
                        jury.as_ref().is_some_and(|jury| jury.seated)
                    },
                    // RFC-0007 Part IV.2: only a `Prefetching` row reads it.
                    capped_entry: palw_mesh_fold_v1::capped_entry_v1(self, ctx, class_id, &row, ready, fold.globals.seat_count as u32),
                };
                // **Lane F1 (the 2026-09-25 sweep's V03(2)/V05), on a span the fence governs**: the step
                // reads and rewrites the class's probation memory — a readiness hold keeps the
                // probation's progress, and only two distinct bonds' failed probes reset it.
                if resilient {
                    let memory = self.state.probation_memory.get(class_id).cloned().unwrap_or_default();
                    let (next_state, next_memory) = crate::palw_model_registry_v1::palw_lifecycle_step_resilient_v1(
                        row.state,
                        &obs,
                        &profile,
                        &fold.globals,
                        &memory,
                    );
                    if next_memory != memory {
                        self.write_probation_memory_v1(*class_id, Some(next_memory))?;
                    }
                    (next_state, utilization)
                } else {
                    (palw_lifecycle_step_v1(row.state, &obs, &profile, &fold.globals), utilization)
                }
            };
            // **RFC-0007 Part IV.2**: a class stepping into `Capped` takes this boundary's DAA (the step knows no clock), and a class
            // already capped keeps the DAA it entered at.
            let state = match (row.state, state) {
                (PalwModelLifecycleV1::Capped { since_daa }, PalwModelLifecycleV1::Capped { .. }) => {
                    PalwModelLifecycleV1::Capped { since_daa }
                }
                (_, PalwModelLifecycleV1::Capped { .. }) => PalwModelLifecycleV1::Capped { since_daa: ctx.daa_score },
                (_, other) => other,
            };
            let admission_milli = profile.admission_claims_per_span_milli.saturating_mul(state.admission_permille() as u64) / 1_000;
            let changed = state != row.state;
            let next = PalwModelLifecycleRowV1 {
                state,
                work: row.work,
                profile,
                since_span: if changed { span_now } else { row.since_span },
                probes_passed: if changed { 0 } else { row.probes_passed },
                probes_failed: if changed { 0 } else { row.probes_failed },
                probes_passed_this_span: 0,
                probes_failed_this_span: 0,
                ready_seats: ready,
                inflight_claims: inflight,
                utilization_permille: utilization,
                admission_milli,
                cap_utilization_permille: cap,
                priced_share_permille: row.priced_share_permille,
            };
            if next != row {
                self.write_model_lifecycle(*class_id, Some(next))?;
            }
            // RFC-0007 Part IV.2: holders seated — the class's waiting capped claims get their re-verification window.
            if crate::palw_mesh_v1::palw_lifecycle_left_capped_v1(&row.state, &state) {
                palw_mesh_fold_v1::capped_exit_v1(self, ctx, class_id)?;
            }
            // ADR-0152-adjacent (Activation Pool): (a) at this Candidate's audit, prep's move once the
            // class is past Candidate, (b)'s run opened at Probation and paid past it.
            if let Some(terms) = self.extras.activation_pool
                && *class_id != base
            {
                self.step_activation_pool_v1(class_id, ctx, span_now, &fold, &terms, row.state, state, jury.as_ref())?;
            }
            if *class_id != base {
                admissions.push((*class_id, admission_milli));
            }
        }
        // ADR-0137: past the work target the registry moves no share, refreshes no budget and
        // seats no price — a share is a result, not an input.
        if self.extras.work_target_active {
            return Ok(());
        }
        if !self.state.class_shares.contains_key(&base) {
            return Ok(());
        }
        let legacy_sum: u32 = self
            .state
            .class_shares
            .iter()
            .filter(|(id, _)| **id != base && !self.state.model_lifecycles.contains_key(id))
            .map(|(_, s)| *s as u32)
            .sum();
        let floor = self.params.min_base_class_share_permille().min(1_000) as u32;
        let room = 1_000u32.saturating_sub(legacy_sum).saturating_sub(floor);
        let sharing: Vec<(Hash64, u64)> =
            admissions.iter().copied().filter(|(id, _)| self.state.class_shares.contains_key(id)).collect();
        let total: u128 = sharing.iter().map(|(_, a)| *a as u128).sum();
        let mut given = 0u32;
        let mut writes: Vec<(Hash64, u16)> = Vec::new();
        for (id, a) in &sharing {
            let share = if total == 0 { 0 } else { ((*a as u128).saturating_mul(room as u128) / total).min(room as u128) as u32 };
            given = given.saturating_add(share);
            writes.push((*id, share as u16));
        }
        writes.push((base, (1_000u32.saturating_sub(legacy_sum).saturating_sub(given)).max(floor) as u16));
        for (id, share) in writes {
            if self.state.class_shares.get(&id).copied() != Some(share) {
                self.write_share(id, Some(share))?;
            }
        }
        // **A share the registry moves is a budget the epoch must follow** (found by the devnet
        // drill): the epoch budget (ADR-0039 D5) is derived from the shares at the epoch's first
        // block and read by every producer until the next, so a class admitted mid-epoch — its
        // share raised from nothing at a boundary — produced nothing until the epoch turned
        // ("holding: this class's epoch budget is already spent"). The budgets of the epoch this
        // boundary falls in are re-derived from the shares it wrote; the produced counters are
        // untouched, so a class that already spent its blocks gains only what its new share adds.
        // Re-derived at every governed boundary and written only when it differs — so a state
        // whose shares moved before this rule existed (a restarted datadir) catches up at its
        // next boundary, and a boundary that moves nothing writes nothing.
        let epoch_index = ctx.daa_score / self.params.epoch_length;
        if let Some(budgets) = palw_epoch_budgets_for_v2(&self.state, self.params, epoch_index)
            && self.state.epoch_budgets.as_ref() != Some(&budgets)
        {
            self.write_epoch_budgets(Some(budgets))?;
        }
        // **A class the registry seats is a class the registry prices** (the seventh finding, by
        // ADR-0076's rule and no second one): a registration copies the floor's target — op 180's
        // terms — which is the floor's price for the floor's draw rate, and a model whose draw is
        // a whole forward sits thousands of times too hard at it. Admitted at 980 ‰ it produced
        // nothing for the drill's whole second phase, skipped by the retarget for producing nothing
        // and left by the idle convergence for not being behind the incumbent's price. The
        // activation edge already prices a weightless class from the share it is granted and its
        // counted work; this is the same event at the registry's boundary, and it happens once —
        // the class DAA owns the target from there, and every later share move is measured by the
        // retarget against the history the class then has. The floor is not priced here: its
        // target is the class DAA's, and it has history by definition.
        let seated: Vec<Hash64> = self
            .state
            .model_lifecycles
            .iter()
            .filter(|(id, row)| {
                **id != base
                    && row.priced_share_permille == 0
                    // ADR-0145 I4, past the fence: a `Candidate` is a registration, and a
                    // registration must not seat a class target — the seed is
                    // `attempt_target_seed_v1(share, pwu)` over the registrant's own declared pwu,
                    // so seating one would let a class price itself before anyone admitted it.
                    // It joins the list where `Prefetching` does, which is when it leaves.
                    && !matches!(
                        row.state,
                        PalwModelLifecycleV1::Registered | PalwModelLifecycleV1::Prefetching | PalwModelLifecycleV1::Candidate
                    )
                    && self.state.class_shares.get(*id).copied().unwrap_or(0) > 0
            })
            .map(|(id, _)| *id)
            .collect();
        for id in seated {
            let share = self.state.class_shares.get(&id).copied().unwrap_or(0);
            let pwu = self.class_seed_pwu(&id, ctx.daa_score);
            let priced = crate::palw_class_daa::attempt_target_seed_v1(share, pwu);
            if self.state.class_targets.get(&id).map(|t| t.target) != Some(priced) {
                self.write_target(id, Some(PalwClassTargetV2 { target: priced }))?;
            }
            let mut row = self.state.model_lifecycles.get(&id).cloned().expect("listed from the map");
            row.priced_share_permille = share;
            self.write_model_lifecycle(id, Some(row))?;
        }
        Ok(())
    }

    // ---- ADR-0125: the execution lane ----

    /// **ADR-0125 / ADR-0130: the span boundary — participants first, randomness after.**
    ///
    /// At the first chain block `B` of span `n` — the block whose parent chain block (`last_point`)
    /// lies in an earlier span, or the first block the state folds — in this order:
    ///
    /// 1. **Promote.** A pending snapshot whose target is `n` becomes span `n`'s schedule when a seed
    ///    anchor was recorded in span `n − 1`, which is after the snapshot was taken (at the first
    ///    chain block of `n − 1`, whose own anchor is recorded after its rotation). The seed is the
    ///    anchor's execution, `n` and the safe frontier as the parent left it
    ///    (`palw_execution_schedule_seeded_v1`). Without such an anchor the snapshot is dropped and
    ///    span `n` has no schedule: the lane is idle for the span, explicitly. A snapshot for a span
    ///    below `n` — a span no chain block opened — is stale and dropped.
    /// 2. **Snapshot.** Finals gathered in span `n − 1` become the pending snapshot for `n + 1`
    ///    (written only when it lists a domain) and leave; finals of an older span are stale and
    ///    leave with nothing taken from them.
    /// 3. **Reset** the seed anchor. `B`'s own attempt, if it carries one, records the next anchor
    ///    after this ([`Self::record_round_seed_anchor`]).
    ///
    /// So credits earned in span `f` are spendable in span `f + 2` and a span's producers are known
    /// only once the block that opens it exists.
    ///
    /// **Where ADR-0151's economic safety is armed** (testnet-12; the 2026-09-23 route-matrix audit's
    /// #2): the maturity delays the snapshot — its target is `n + 1 + maturity spans` — and the
    /// finals of any earlier span are taken, not only the span just before; a due snapshot is
    /// seeded at the first span whose predecessor recorded an anchor (oldest first, one per span,
    /// dropped only `PALW_EXEC_PENDING_GRACE_SPANS_V1` spans past its target); and its tickets are
    /// minted onto that span's own rounds. Then, at every block, every schedule and permit row
    /// older than the span before this block's is dropped: a round block anchored two spans back can
    /// no longer be accepted, so nothing it could collide with needs keeping.
    pub(super) fn rotate_round_lane(&mut self, ctx: &PalwBlockContextV2, span_daa: u64) -> Result<(), PalwStateV2Error> {
        use crate::palw_execution_lane_v1::{
            PalwExecFinalV1, palw_execution_schedule_assign_quanta_bounded_v1, palw_execution_schedule_assign_quanta_windowed_v1,
            palw_execution_schedule_seeded_v1, palw_execution_schedule_snapshot_v1, palw_execution_span_v1,
        };
        // **ADR-0151: what a conviction has taken back, and how long a fresh right waits.**
        //
        // The forfeiture set is derived from the convictions themselves, so a Final whose execution was
        // convicted mints nothing at any of the three stages a right can sit in. The maturity keeps
        // every right of a Final unused for that long after it, and the rights a conviction could
        // still take past it are the residual the lock prices (`realizable_rights_of_v1`).
        //
        // Both are inert where `Params::palw_economic_safety` is dormant, which is every preset but
        // testnet-12: an empty set and a zero maturity make the mint below the one every other network
        // runs, ticket for ticket.
        //
        // **The maturity delays the SNAPSHOT, not the tickets** (the 2026-09-23 route-matrix audit's
        // #2). It used to be added to every ticket's round — `span_open_round + maturity_rounds`, 1,200
        // DAA × 120 = 144,000 rounds past the span that minted it — while that span's schedule is kept
        // only until the span after next, and a round block is judged against its OWN anchor span's
        // schedule. So every ticket's round fell ~40 hours after the only schedule that listed it had
        // been deleted, and no permit, no algo-10 block and no fee attribution could ever follow a
        // Final, for any class. Now a span's Finals are snapshotted for the span `maturity_spans` later
        // than before, and that span's schedule places its tickets in its own rounds
        // (`palw_execution_mint_quanta_windowed_v1`: consecutively from the span's opening round, at
        // most as many as the span has rounds — the 2^16-round horizon of the older mint put nearly
        // every ticket past its span even with no maturity at all). The right still waits out the
        // whole window, the mint still drops any execution convicted meanwhile, and the ticket lives
        // in the schedule its round is judged by.
        //
        // **The maturity is the one the fold was handed** (`PalwEconomicSafetyFoldV1::maturity_daa`,
        // `Params::palw_exec_quantum_maturity_v1`): on testnet-12 the challenge window the network
        // APPLIES, 120 DAA (user decision 2026-09-25), not the unshortened lattice window of 1,200 —
        // and the lock prices the gap from the same field.
        let (maturity_daa, forfeited) = match self.extras.economic_safety {
            Some(safety) => (safety.maturity_daa, self.state.palw_forfeited_execution_roots_v1()),
            None => (0, std::collections::BTreeSet::new()),
        };
        let maturity_spans = maturity_daa.div_ceil(span_daa.max(1));
        let span_now = palw_execution_span_v1(ctx.daa_score, span_daa);
        let opens_span = self.state.last_point.is_none_or(|last| palw_execution_span_v1(last.daa_score, span_daa) < span_now);
        if opens_span {
            // ADR-0135: the registry steps every class at the boundary, before the lane's own rotation.
            self.step_model_registry(ctx, span_now)?;
            // **Past ADR-0151's bundle a matured snapshot waits for a span that can seed it** (the
            // route-matrix audit's #2). Below it a due snapshot is dropped when the span before had no
            // attempt-carrying chain block to anchor its seed, or when a merge of two blue blocks
            // skipped its span's DAA — and on testnet-12, whose span is one DAA, either happens to
            // ordinary spans, so a Final's rights were lost to the shape of the chain after they had
            // waited out the whole maturity. Here the oldest due snapshot is seeded at the first span
            // whose predecessor recorded an anchor, as that span's schedule (the seed reads the span it
            // is seeded for, and that anchor did not exist when the snapshot was taken), one per span;
            // the rest keep waiting, and a snapshot leaves unseeded only once
            // `PALW_EXEC_PENDING_GRACE_SPANS_V1` spans have opened past its target.
            let mut seeded_this_span = false;
            for target in self.state.round_pending.keys().copied().collect::<Vec<_>>() {
                if target > span_now {
                    // Not yet due: a snapshot targets the span after the one whose first block took
                    // it — or, where ADR-0151's maturity is armed, the span that many spans later — and
                    // waits in `round_pending` until that span opens.
                    continue;
                }
                let snapshot = self.state.round_pending.get(&target).cloned().expect("the key was just listed");
                if let Some(safety) = self.extras.economic_safety {
                    // **ADR-0170 M3: past `palw_anchor_window_v1` the seed is the latest anchor of the window `S − W … S − 1`** that was
                    // recorded at or after the span this snapshot was taken in (participants first, randomness after); below the fence
                    // only the span before's anchor seeds it.
                    let anchor = match self.params.anchor_window_spans_at(ctx.daa_score) {
                        Some(window) => self.state.round_seed_anchor.filter(|anchor| {
                            crate::palw_anchor_window_v1::palw_anchor_window_seeds_snapshot_v1(
                                anchor.span,
                                span_now,
                                window,
                                target,
                                maturity_spans,
                            )
                        }),
                        None => self.state.round_seed_anchor.filter(|anchor| anchor.span + 1 == span_now),
                    };
                    if let (false, Some(anchor), Some(lane)) = (seeded_this_span, anchor, self.extras.round_lane) {
                        self.write_round_pending(target, None)?;
                        let mut due = snapshot;
                        due.target_span = span_now;
                        let (frontier_blue_score, frontier) = self.state.safe_frontier();
                        let mut schedule = palw_execution_schedule_seeded_v1(&due, &anchor, frontier_blue_score, frontier);
                        let window_rounds =
                            crate::palw_execution_quanta_v1::palw_execution_span_rounds_v1(span_daa, safety.target_time_per_block_ms);
                        // **Lane BUDGET (readiness §3e): past `palw_bond_budget_v1` every bond's Round rights are capped BEFORE the
                        // draw's candidate set, and what the draw allocates is reserved in its window.** Below the fence, the mint as
                        // it always was, ticket for ticket.
                        if !self.bond_budget_round_draw_v1(
                            ctx,
                            &mut schedule,
                            lane.execution_quantum,
                            lane.span_open_round,
                            window_rounds,
                            &forfeited,
                        )? {
                            palw_execution_schedule_assign_quanta_windowed_v1(
                                &mut schedule,
                                lane.execution_quantum,
                                lane.span_open_round,
                                window_rounds,
                                &forfeited,
                            );
                        }
                        self.write_round_schedule(span_now, Some(schedule))?;
                        seeded_this_span = true;
                    } else if target.saturating_add(crate::palw_execution_lane_v1::PALW_EXEC_PENDING_GRACE_SPANS_V1) < span_now {
                        self.write_round_pending(target, None)?;
                    }
                    continue;
                }
                self.write_round_pending(target, None)?;
                if target == span_now
                    && let Some(anchor) = self.state.round_seed_anchor
                    && anchor.span + 1 == span_now
                {
                    let (frontier_blue_score, frontier) = self.state.safe_frontier();
                    let mut schedule = palw_execution_schedule_seeded_v1(&snapshot, &anchor, frontier_blue_score, frontier);
                    if let Some(lane) = self.extras.round_lane {
                        // 2026-09-23 audit C-2: a span issues at most as many tickets as its
                        // round-assignment horizon holds; past that the mint's probe went linear
                        // and one honest Final cost hours of fold time. Unbounded below the fence.
                        let max_quanta = if self.extras.audit_2026_09_23_active {
                            crate::palw_execution_quanta_v1::PALW_EXEC_MAX_QUANTA_PER_SPAN_V1
                        } else {
                            usize::MAX
                        };
                        palw_execution_schedule_assign_quanta_bounded_v1(
                            &mut schedule,
                            lane.execution_quantum,
                            lane.span_open_round,
                            // The maturity was served before this span's snapshot was taken (above).
                            0,
                            &forfeited,
                            max_quanta,
                        );
                    }
                    self.write_round_schedule(span_now, Some(schedule))?;
                }
            }
            if !self.state.round_finals.is_empty() {
                // Past ADR-0151's bundle the finals of ANY earlier span are snapshotted — a span the
                // DAA skipped does not take its predecessor's rights with it (#2, as above).
                let takes_snapshot = if self.extras.economic_safety.is_some() {
                    self.state.round_span < span_now
                } else {
                    self.state.round_span + 1 == span_now
                };
                if takes_snapshot {
                    // ADR-0151: a convicted execution does not enter the next span's snapshot either —
                    // the earliest of the three stages a forfeited right can be dropped at.
                    let finals: Vec<PalwExecFinalV1> = self
                        .state
                        .round_finals
                        .values()
                        .copied()
                        .filter(|f| !crate::palw_economic_safety_v1::palw_exec_rights_are_forfeit_v1(&forfeited, &f.execution_root))
                        .collect();
                    let target = span_now.saturating_add(1).saturating_add(maturity_spans);
                    let snapshot = palw_execution_schedule_snapshot_v1(target, &finals);
                    if !snapshot.domains.is_empty() {
                        self.write_round_pending(target, Some(snapshot))?;
                    }
                }
                for key in self.state.round_finals.keys().copied().collect::<Vec<_>>() {
                    self.write_round_final(key, None)?;
                }
                // The span marker names the finals' span and means nothing without them.
                self.write_round_span(0)?;
            }
            // **ADR-0170 M2: past `palw_anchor_window_v1` the anchor survives the span boundary** — it keeps the span it was recorded
            // in, and each reader checks its age against the window. Below the fence it is the span before's alone, and cleared.
            if !self.params.anchor_window_active_at(ctx.daa_score) {
                self.write_round_seed_anchor(None)?;
            }
        }
        let keep_from = span_now.saturating_sub(1);
        for key in self.state.round_schedules.range(..keep_from).map(|(k, _)| *k).collect::<Vec<_>>() {
            self.write_round_schedule(key, None)?;
        }
        for key in self.state.round_permits_used.range(..(keep_from, 0)).map(|(k, _)| *k).collect::<Vec<_>>() {
            self.write_round_permits_used(key, None)?;
        }
        for key in self.state.round_equivocations.range(..(keep_from, 0)).map(|(k, _)| *k).collect::<Vec<_>>() {
            self.write_round_equivocations(key, None)?;
        }
        Ok(())
    }

    /// **ADR-0125 §7.3: a permit proven signed twice.** The acceptance layer checked the evidence
    /// against the parent state — the named span's schedule grants the permit to the bond, and both
    /// signatures verify under its registered key. Here: the lane is open, the span is one the chain
    /// still keeps, and the permit was not burned before; then the bond loses the registry's
    /// collateral floor — what the network asks of anyone who produces — burned like every slash,
    /// and the permit is burned so no merging block grants it from here on.
    pub(super) fn record_round_equivocation(
        &mut self,
        ctx: &PalwBlockContextV2,
        evidence: &crate::palw_execution_lane_v1::PalwExecEquivocationV1,
    ) -> Result<(), PalwStateV2Error> {
        let refused = |why: String| PalwStateV2Error::RoundEquivocationRefused(why);
        let Some(lane) = self.extras.round_lane else {
            return Err(refused("the execution lane is not open".to_string()));
        };
        evidence.validate_shape().map_err(|e| refused(e.to_string()))?;
        let span_now = crate::palw_execution_lane_v1::palw_execution_span_v1(ctx.daa_score, lane.schedule_span_daa);
        if evidence.span > span_now || evidence.span + 1 < span_now {
            return Err(refused(format!("span {} is not one the chain keeps at span {span_now}", evidence.span)));
        }
        // One burn per permit however the two spans a boundary round might be judged by name it.
        for span in [evidence.span.saturating_sub(1), evidence.span, evidence.span + 1] {
            if self.state.round_equivocated(span, evidence.round, evidence.permit_index) {
                return Err(refused(format!("round {} permit {} was burned already", evidence.round, evidence.permit_index)));
            }
        }
        let floor = self.params.min_collateral_sompi() as u128;
        self.slash_bond(evidence.bond, floor)?;
        let key = (evidence.span, evidence.round);
        let bits = self.state.round_equivocations.get(&key).copied().unwrap_or(0);
        self.write_round_equivocations(key, Some(bits | (1u16 << evidence.permit_index)))?;
        Ok(())
    }

    /// **ADR-0125: an attempt that reaches `Final` earns its security domain a credit** — the compute
    /// it certified (`palw_execution_credit_v1`: its exposure pwu, capped at the work price's unit) —
    /// in the span the finalizing block belongs to. Only the attempt lane: a free-prompt claim's
    /// `Final` licenses quanta rather than certifying a block's work, and the lane's quota is the
    /// attempt census.
    pub(super) fn record_round_final(
        &mut self,
        id: Hash64,
        claim: &PalwClaimStateV2,
        final_daa: u64,
        span_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !matches!(claim.source, PalwClaimSourceV2::Attempt) {
            return Ok(());
        }
        let operator_id = self.state.bonds.get(&claim.bond).ok_or(PalwStateV2Error::MissingBond(claim.bond))?.operator_id;
        // ADR-0145: the execution lane's credit is the ADR-0124 work price by another name, so it
        // reads the same basis — measure and unit both derived past the fence, both declared below.
        // Resolved before the class borrow so the two reads of `self.state` do not overlap.
        let canonical = self.canonical_per_draw(&claim.class_id, claim.accepted_daa);
        // A claim names a registered class, and a class row leaves the state only when a delta that
        // registered it is reverted; an attempt whose class is gone, or that certified no compute,
        // earns nothing rather than a guessed credit.
        let exposure = {
            let Some(class) = self.state.classes.get(&claim.class_id) else { return Ok(()) };
            // **2026-09-23 audit C-2: in the unit the quantum was calibrated in.** The lane
            // divides this by `PALW_EXECUTION_QUANTUM_V1`, a constant denominated in collateral
            // pwu, and `class_seed_pwu`'s rule applies: normalise where a pwu meets a constant.
            // The raw derived MAC-eq met it below the fence, 2,810x the unit, and one honest
            // hybrid-512 Final minted 1,585,742 tickets (533 MB of rooted state) where the
            // collateral unit mints 565.
            if self.extras.audit_2026_09_23_active {
                palw_exposure_pwu_v3(class, claim.pwu, canonical, self.exposure_basis(claim.accepted_daa))
            } else {
                palw_exposure_pwu_v2(class, claim.pwu, canonical)
            }
        };
        // Lottery quotas stay clamped at the work-price unit. Execution quanta mint from the
        // unclamped CanonicalWork scalar so a heavier verified job earns more spend-once tickets.
        let credit = if self.extras.round_lane.is_some_and(|lane| lane.execution_quantum > 0) {
            exposure
        } else {
            crate::palw_execution_lane_v1::palw_execution_credit_v1(exposure, self.work_price_unit_at(claim.accepted_daa))
        };
        if credit == 0 {
            return Ok(());
        }
        let family = self.state.fp_certified_classes.get(&claim.class_id).map(|row| row.family_digest);
        let span = crate::palw_execution_lane_v1::palw_execution_span_v1(final_daa, span_daa);
        if self.state.round_finals.is_empty() {
            self.write_round_span(span)?;
        }
        self.write_round_final(
            id,
            Some(crate::palw_execution_lane_v1::PalwExecFinalV1 {
                domain: crate::palw_execution_lane_v1::palw_execution_domain_of_class_v1(family, claim.class_id),
                bond: claim.bond,
                operator_id,
                claim_id: id,
                execution_root: claim.execution_root,
                credit,
                // ADR-0152 v3.1 (review M-1): the order the mint collapses a root's Finals by. 0
                // below the fence, where the mint keeps the lowest claim id as it always did.
                accepted_blue_score: if self.extras.offence_attribution_active { claim.accepted_blue_score } else { 0 },
            }),
        )?;
        Ok(())
    }

    /// **ADR-0130: a chain block that carries an admitted attempt is the open span's seed anchor**
    /// until a later chain block of the span carries one. Recorded after the span's rotation, so the
    /// block that opens a span can anchor the snapshot that block took. `execution_key` is the
    /// attempt's execution commitment under its own header's anchor — the processor derives it from
    /// the header (`palw_execution_key_v1`) and passes it as the fold's `own_execution_key`.
    pub(super) fn record_round_seed_anchor(&mut self, ctx: &PalwBlockContextV2, span_daa: u64, execution_key: Hash64) -> Result<(), PalwStateV2Error> {
        self.record_round_seed_anchor_of(ctx, span_daa, ctx.block, execution_key)?;
        Ok(())
    }

    /// **ADR-0170 M1: [`Self::record_round_seed_anchor`] for an attempt another block carried** — a merged one, blue or red, which the
    /// chain block `ctx` folded. The anchor's span is the span of the block that FOLDED the attempt (the span the fold learned of it
    /// in, which every node holding the chain agrees on), its `block` the attempt's own carrying block and its execution key the
    /// attempt's own (`PalwMergedWorkV1::execution_key`, derived by the processor from the merged header).
    pub(super) fn record_round_seed_anchor_of(
        &mut self,
        ctx: &PalwBlockContextV2,
        span_daa: u64,
        block: BlockHash,
        execution_key: Hash64,
    ) -> Result<(), PalwStateV2Error> {
        let span = crate::palw_execution_lane_v1::palw_execution_span_v1(ctx.daa_score, span_daa);
        // **RFC-0018 §4.2 (`palw_dag_sibling_attribution_v1`, at the folding block's DAA): no last-write-wins.** Within a span the
        // anchor is the admitted attempt of least `H(domain ‖ execution key)` — own or merged, blue or red, whichever block folded it
        // first — so neither the merge order nor which sibling GHOSTDAG selected decides it. Below the fence: the last write, as before.
        if self.params.rfc0018_v1().is_some_and(|mirror| mirror.sibling_attribution_at(ctx.daa_score))
            && let Some(held) = self.state.round_seed_anchor
            && held.span == span
            && crate::palw_rfc0018_v1::palw_sibling_priority_v1(&held.execution_key)
                <= crate::palw_rfc0018_v1::palw_sibling_priority_v1(&execution_key)
        {
            return Ok(());
        }
        self.write_round_seed_anchor(Some(crate::palw_execution_lane_v1::PalwExecSeedAnchorV1 { span, block, execution_key }))?;
        Ok(())
    }

    /// **ADR-0125: the permits this block accepted.** The processor decided them against the parent
    /// state; the fold records them, and refuses a permit already recorded — the two answering
    /// differently is a divergence to surface, never a permit to accept twice.
    pub(super) fn record_round_permits(
        &mut self,
        uses: &[crate::palw_execution_lane_v1::PalwExecPermitUseV1],
    ) -> Result<(), PalwStateV2Error> {
        for used in uses {
            if used.permit_index >= crate::palw_execution_lane_v1::PALW_EXEC_MAX_PERMITS_PER_ROUND_V1 {
                return Err(PalwStateV2Error::RoundPermitRefused(format!(
                    "permit index {} above the widest round",
                    used.permit_index
                )));
            }
            let key = (used.span, used.round);
            let bits = self.state.round_permits_used.get(&key).copied().unwrap_or(0);
            let bit = 1u16 << used.permit_index;
            if bits & bit != 0 {
                return Err(PalwStateV2Error::RoundPermitRefused(format!(
                    "permit (span {}, round {}, index {}) accepted twice",
                    used.span, used.round, used.permit_index
                )));
            }
            self.write_round_permits_used(key, Some(bits | bit))?;
        }
        Ok(())
    }

    // ---- ADR-0124: the panel economy ----

    /// **ADR-0124 Decision 3: put every drawn seat on duty** — one duty entry at `0`, and the
    /// seat's exposure reserved on its bond for the claim's whole life. The draw only seats a bond
    /// whose free collateral covers this (`palw_seat_has_headroom_v1`), so the reservation here is
    /// the one the draw priced.
    ///
    /// **ADR-0130: the amount is [`crate::palw_panel_economy_v1::palw_panel_seat_exposure_v1`]** of the
    /// claim over the seats put on duty, under `extras.panel_reward_multiple_permille` — the floor in
    /// force at THIS block, which for a binding the chain derives at its claim's anchor block is the
    /// floor the draw priced at the anchor. A binding that lands later than its anchor, across the
    /// floor's height, reserves the floor its draw did not price; nothing unbalances, because the
    /// amount is written into the row and the release and the dissent slash read the row.
    pub(super) fn reserve_seat_duties(
        &mut self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        seats: &[PalwPanelSeatV2],
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        // A bond sits once on a panel (the draw dedups by operator); a duplicate is not put on duty
        // twice, so the row holds each seat bond once and the release returns exactly what this took.
        let on_duty: BTreeMap<PalwBondKeyV2, u64> = seats.iter().map(|seat| (seat.bond, 0)).collect();
        // Over the seats ON DUTY — the number `finalize_claim` divides the pool by — so the floor
        // bounds the share a seat can actually be paid. **ADR-0152 L-4 / A-4: past `palw_rcore_plus`
        // the duty is `duty_bind`** — `min(max(λ-term, lock_2), commitment / seats)`, so `seats ×
        // duty` never exceeds what the claim forfeits (F14) — and the `3 × claim.reserved` term is
        // retired. **ADR-0160 lane escrow: past F-E an attributable attempt's seat reserves the
        // eligibility `max(duty_bind, lock_2)` the draw priced** — a credit cuts the commitment the
        // duty is capped by but not the lock, and a seat must never sit on more panels than it can
        // back at licence (`palw_escrow_bind_reserves_the_lock_v1`). Without a credit, on every
        // testnet-12 class, the two are `duty_bind`, byte for byte.
        let seat_exposure = if self.params.rcore_plus_active_at(now_daa) {
            let prices = self.read().rcore_bind_prices(&claim_id, claim, on_duty.len(), now_daa);
            if crate::palw_escrow_funding_v2::palw_escrow_bind_reserves_the_lock_v1(self.params, claim) {
                prices.eligibility
            } else {
                prices.duty_bind
            }
        } else {
            crate::palw_panel_economy_v1::palw_panel_seat_exposure_v1(
                claim.reserved,
                claim.escrowed_reward,
                on_duty.len(),
                self.extras.panel_reward_multiple_permille,
            )
        };
        self.reserve_seat_duties_with(claim_id, seats, seat_exposure, now_daa)
    }

    /// **The one writer of a duty row and its reservations**, for an exposure the caller priced: V2's `reserve_seat_duties` prices
    /// it at the binding block, and the permissionless Panel's fold (RFC-0010) reserves the exposure its engine committed at
    /// admission — one ledger, one writer, so a V3 seat is on duty exactly as a V2 seat is.
    pub(super) fn reserve_seat_duties_with(
        &mut self,
        claim_id: Hash64,
        seats: &[PalwPanelSeatV2],
        seat_exposure: u128,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        self.hold_panel_recipients_v1(claim_id, seats, now_daa)?;
        let on_duty: BTreeMap<PalwBondKeyV2, u64> = seats.iter().map(|seat| (seat.bond, 0)).collect();
        // Reserved in the panel's own order, each bond once — the journal order the row was always
        // reserved in.
        let mut reserved: Vec<PalwBondKeyV2> = Vec::with_capacity(on_duty.len());
        for seat in seats {
            if reserved.contains(&seat.bond) {
                continue;
            }
            reserved.push(seat.bond);
            let current = self.state.reserved_exposure.get(&seat.bond).copied().unwrap_or(0);
            let next = current.checked_add(seat_exposure).ok_or(PalwStateV2Error::Overflow("seat exposure"))?;
            self.write_exposure(seat.bond, Some(next))?;
        }
        self.write_panel_duties(claim_id, Some(PalwPanelDutyRowV1 { seats: on_duty, seat_exposure }))?;
        // Lane PL part E (ADR-0166): each seat named on a bound panel is one assignment in its availability window.
        if self.params.seat_availability_active_at(now_daa) {
            for bond in &reserved {
                let mut row = self.state.seat_availability.get(bond).cloned().unwrap_or_default();
                row.note_assigned(now_daa);
                self.write_seat_availability(*bond, Some(row))?;
            }
        }
        Ok(())
    }

    /// **Release every seat's exposure and drop the duty row** — on `Final`, on every void and on
    /// the receipt-timeout redraw, which deals a different panel. A no-op for a claim whose panel
    /// was bound below the fence (no row), so every existing path is byte-identical there.
    ///
    /// ADR-0130: what is released is the row's stored `seat_exposure`, never a recomputation — a
    /// floor armed between the binding and this block cannot release more than binding reserved.
    pub(super) fn release_seat_duties(&mut self, claim_id: &Hash64) -> Result<(), PalwStateV2Error> {
        let Some(row) = self.state.panel_duties.get(claim_id).cloned() else { return Ok(()) };
        for seat in row.seats.keys() {
            let current = self.state.reserved_exposure.get(seat).copied().unwrap_or(0);
            let next = current.checked_sub(row.seat_exposure).ok_or(PalwStateV2Error::Overflow("seat exposure underflow"))?;
            self.write_exposure(*seat, if next == 0 { None } else { Some(next) })?;
        }
        self.write_panel_duties(*claim_id, None)?;
        Ok(())
    }

    /// **ADR-0124 Decision 2: credit every `Valid` receipt of a seat on duty**, once, at this
    /// block's DAA. A no-op without a duty row (a panel bound below the fence) and for a seat
    /// already credited; a receipt from a bond that is not on duty cannot reach here (the
    /// acceptance layer refuses it as `NotASeat`), and is ignored rather than trusted if it did.
    pub(super) fn credit_seat_receipts(
        &mut self,
        claim_id: Hash64,
        receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2],
        daa_score: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mut row) = self.state.panel_duties.get(&claim_id).cloned() else { return Ok(()) };
        let mut moved = false;
        let mut credited: Vec<PalwBondKeyV2> = Vec::new();
        for receipt in receipts {
            // ADR-0152 Q-1 (§9.1 Q3): a `Sampled` seat is paid as a `Valid` one is. The verdict
            // reaches the fold only past `Params::palw_rcore_plus`, so every other network credits
            // exactly the `Valid`s it always did.
            if !matches!(
                receipt.verdict,
                crate::palw_panel_v2::PalwReceiptVerdictV2::Valid | crate::palw_panel_v2::PalwReceiptVerdictV2::Sampled
            ) {
                continue;
            }
            if let Some(at) = row.seats.get_mut(&receipt.seat_bond)
                && *at == 0
            {
                *at = daa_score.max(1);
                moved = true;
                credited.push(receipt.seat_bond);
            }
        }
        if moved {
            self.write_panel_duties(claim_id, Some(row))?;
        }
        self.record_staged_prefix_receipts_v1(claim_id, receipts, &credited, daa_score)?;
        // Lane PL part E (ADR-0166): a credited receipt is one success in its seat's availability window (once per seat and
        // claim: the duty row's `0 -> credited` edge is the only writer).
        if self.params.seat_availability_active_at(daa_score) {
            for bond in credited {
                let mut avail = self.state.seat_availability.get(&bond).cloned().unwrap_or_default();
                avail.note_credited(daa_score);
                self.write_seat_availability(bond, Some(avail))?;
            }
        }
        Ok(())
    }

    /// **ADR-0124 Decision 2, the supplementary door as the fold sees it.** The acceptance layer
    /// has verified the signatures and the window (`palw_panel_v2::validate_receipt_quorum_v2_with_economy`);
    /// the fold re-derives every structural fact from its own state — the row, the seats, the
    /// verdicts, the window — because the sync walk folds the same object without that layer, and
    /// a credit the walk would write and the live path would not is a root that forks.
    pub(super) fn credit_supplementary_receipts(
        &mut self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2],
        ctx: &PalwBlockContextV2,
    ) -> Result<(), PalwStateV2Error> {
        let refused = |why: String| PalwStateV2Error::SupplementaryReceiptsRefused { claim: claim_id, why };
        // RFC-0006: a claim drawn per layer shard takes no whole-object supplementary set (its seats attest cells).
        if self.claim_licenses_by_parts(&claim_id) {
            return Err(PalwStateV2Error::LicensedByParts(claim_id));
        }
        let duties = self
            .state
            .panel_duties
            .get(&claim_id)
            .map(|row| row.seats.clone())
            .ok_or_else(|| refused("the claim's panel holds no duty row".into()))?;
        let bound_daa =
            self.state.panels.get(&claim_id).map(|panel| panel.bound_daa).ok_or_else(|| refused("no bound panel".into()))?;
        let deadline = palw_claim_receipt_deadline_v1(&self.state, self.params, &claim_id, claim)?
            .ok_or_else(|| refused("no bound panel".into()))?;
        if receipts.is_empty() {
            return Err(refused("it carries no receipt".into()));
        }
        if ctx.daa_score > deadline {
            return Err(refused("the receipt window has closed".into()));
        }
        let mut seen: Vec<PalwBondKeyV2> = Vec::new();
        for receipt in receipts {
            if receipt.claim != claim_id {
                return Err(refused(format!("a receipt names claim {}", receipt.claim)));
            }
            if !matches!(receipt.verdict, crate::palw_panel_v2::PalwReceiptVerdictV2::Valid) {
                return Err(refused(format!("seat {:?} carries a verdict that is not Valid", receipt.seat_bond)));
            }
            // V3S-01 (past the fence, on a licence awaiting its replay): a credited seat not yet
            // counted over the whole job is taken too — the one predicate the acceptance layer reads.
            match duties.get(&receipt.seat_bond) {
                None => return Err(refused(format!("bond {:?} is not on duty", receipt.seat_bond))),
                Some(at) if *at != 0 && !palw_rcore_v2_widens_seat_v1(&self.state, &claim_id, claim, &receipt.seat_bond) => {
                    return Err(refused(format!("seat {:?} is already credited", receipt.seat_bond)));
                }
                Some(_) => {}
            }
            if seen.contains(&receipt.seat_bond) {
                return Err(refused(format!("seat {:?} appears twice", receipt.seat_bond)));
            }
            if receipt.signed_daa < bound_daa || receipt.signed_daa > deadline || receipt.signed_daa > ctx.daa_score {
                return Err(refused(format!("seat {:?} signed outside the receipt window", receipt.seat_bond)));
            }
            seen.push(receipt.seat_bond);
        }
        // Nothing but the credit moves: the phase, the deadlines and the weight are the licensing
        // object's, already written.
        let _ = claim;
        // **ADR-0152 S-SPEC §3.5, past `palw_rcore_plus`: each new `Valid` locks `lock_{max(k, 2)}`
        // of the set's recount after it, with the full mask** (a V2 receipt attests the full cut),
        // backed on the one ledger like a licensing signer — an unbacked one refuses the object as
        // the quorum price's did. An existing lock is never repriced (Q-4).
        //
        // **The recount (Q-3, V3S-01).** A fresh signer adds one to every segment, so for a set of
        // fresh signers Q-3 is `min(3, basis_k + fresh)` exactly (the recorded `basis_k` is never
        // below the recount of the counted masks — every door records a recount, raised never
        // lowered). A seat this set WIDENS (counted by a partial mask on a licence awaiting its
        // replay) adds one to the segments its mask did not cover, which that sum cannot see; so
        // the set's `basis_k` is the larger of the sum and Q-3 over the record as the set leaves it
        // ([`palw_rcore_counted_masks_attesting_whole_v1`]) — the sum itself wherever nothing widens.
        if self.params.rcore_plus_active_at(ctx.daa_score) {
            let cap = crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1 as u8;
            let signers: Vec<PalwBondKeyV2> = receipts.iter().map(|receipt| receipt.seat_bond).collect();
            let fresh = signers.iter().filter(|seat| !self.state.slashable_locks.contains_key(&(**seat, claim_id))).count();
            let (whole, cut) = palw_rcore_counted_masks_attesting_whole_v1(&self.state, &claim_id, &signers);
            let basis_k = claim
                .rcore
                .basis_k
                .saturating_add(fresh.min(u8::MAX as usize) as u8)
                .min(cap)
                .max(palw_receipt_set_basis_k_v1(&whole, cut));
            let lock = self.read().rcore_lock(&claim_id, claim, basis_k);
            let seat_count = self.state.panels.get(&claim_id).map(|panel| panel.seats.len()).unwrap_or(0);
            let segments = crate::palw_verification_v2::palw_segment_count_v2(seat_count.min(u16::MAX as usize) as u16);
            for receipt in receipts {
                let seat = receipt.seat_bond;
                let top_up = lock.saturating_sub(palw_seat_duty_of_v1(&self.state, &claim_id, &seat));
                let room = self.gate_room(&seat, ctx.daa_score, PalwRcoreGateV1::Work);
                if !self.state.slashable_locks.contains_key(&(seat, claim_id)) && room < top_up {
                    return Err(PalwStateV2Error::SeatValidLockRefused { seat, claim: claim_id, required: top_up, available: room });
                }
                // ADR-0160 lane liab (AG-3): a frozen seat's `Valid` backs nothing — refused as a seat
                // with no room is (the map is empty below the fence).
                if crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&self.state, &seat) {
                    return Err(PalwStateV2Error::SeatValidLockRefused { seat, claim: claim_id, required: top_up, available: 0 });
                }
            }
            self.credit_seat_receipts(claim_id, receipts, ctx.daa_score)?;
            for receipt in receipts {
                let full = crate::palw_verification_v2::PalwSegmentMaskV2::full(segments);
                if self.state.slashable_locks.contains_key(&(receipt.seat_bond, claim_id)) {
                    self.widen_counted_seat_v1(receipt.seat_bond, claim_id, full, segments)?;
                } else {
                    self.lock_valid_seat_rcore(receipt.seat_bond, claim_id, lock, full, segments, ctx.daa_score)?;
                }
            }
            return self.stage_supplementary_v1(claim_id, claim, receipts, basis_k, deadline, ctx.daa_score);
        }
        self.credit_seat_receipts(claim_id, receipts, ctx.daa_score)?;
        // A supplementary `Valid` is not part of the set that licensed: it locks the quorum price.
        self.lock_valid_receipts(claim_id, claim, receipts, ctx.daa_score, PalwLicenceDoorV1::Quorum)?;
        Ok(())
    }

    /// **ADR-0152 SR-1b in the V2 door** (S-SPEC §3.5), on receipts `credit_supplementary_receipts`
    /// has already verified: `Valid`, of panel seats on duty the door takes
    /// ([`palw_rcore_v2_door_takes_seat_v1`]: not yet credited, or widened on a licence awaiting its
    /// replay), distinct, inside the receipt window.
    ///
    /// * **Served bits** — each receipt's seat (a seat this set widened was served already).
    /// * **Recount** — `basis_k`, the caller's Q-3 recount of the record as the set leaves it
    ///   (`min(3, basis_k + fresh)` where nothing widens; see `credit_supplementary_receipts`).
    /// * **Upgrade** — the first time the recount reaches 2 the door is recorded as Coverage if a
    ///   mask the chain now counts is partial, otherwise Quorum ([`palw_rcore_upgraded_door_v1`], the
    ///   V3 door's same rule, read after the set's locks are written — so a seat this set widened to
    ///   the whole cut counts as whole); and an attempt's anchor settles (V-8).
    /// * **SR-1b** — while `daa ≤ min(L + ⌊window_challenge_at(L) / 2⌋, bound + window_receipt)` a
    ///   record that now satisfies [`palw_rcore_release_due_v1`] flips `escrow_released` and the
    ///   escrow term leaves the producer's ledger in this block; later, never. Never un-flips (SR-4).
    ///
    /// The lock each receipt takes (L-1's `lock_{max(k,2)}` with the full mask, or its existing lock
    /// widened to the full mask) is written by the caller before this runs.
    pub(super) fn stage_supplementary_v1(
        &mut self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2],
        basis_k: u8,
        receipt_deadline: u64,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        let PalwClaimPhaseV2::ReceiptLicensed { .. } = claim.phase else { return Ok(()) };
        let seats: Vec<PalwBondKeyV2> =
            self.state.panels.get(&claim_id).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
        let mut staged = claim.clone();
        for receipt in receipts {
            let Some(index) = seats.iter().position(|seat| *seat == receipt.seat_bond) else { continue };
            if index < 32 {
                staged.rcore.served_mask |= 1u32 << index;
            }
        }
        let old_k = staged.rcore.basis_k;
        staged.rcore.basis_k = basis_k.max(old_k);
        if old_k < PALW_RCORE_FINAL_BASIS_K_V1 && staged.rcore.basis_k >= PALW_RCORE_FINAL_BASIS_K_V1 {
            staged.rcore.licence_door = Some(palw_rcore_upgraded_door_v1(&self.state, &claim_id));
            // RFC-0018 Gate C's settled-anchor site (the identity below its fence).
            if matches!(claim.source, PalwClaimSourceV2::Attempt) && crate::palw_rfc0018_v1::palw_gate_c_settles_anchor_v1(self.params, claim) {
                self.settle_anchor(now_daa, true)?;
            }
        }
        // SR-1b, the one rule both supplementary doors flip by (F4's V3 door calls it too).
        if palw_rcore_supplementary_flips_v1(self.params, &self.state, &claim_id, &staged, receipt_deadline, now_daa) {
            staged.rcore.escrow_released = true;
        }
        if staged.rcore != claim.rcore {
            let upgraded = palw_rcore_upgrades_v1(claim, &staged);
            self.move_commitment(claim, &staged, now_daa)?;
            self.write_claim(claim_id, Some(staged))?;
            // Q-5 (M4): the gate is lifted, and the claim finalizes at `max(L + wc(L), U)`.
            if upgraded {
                self.rearm_claim_deadline_dl1_v1(claim_id, now_daa)?;
            }
        }
        Ok(())
    }

    /// **ADR-0152 SR-10: the V3 supplementary door, as the fold sees it** — past
    /// `Params::palw_rcore_plus`, on a claim already `ReceiptLicensed`, carried by the existing
    /// `ReceiptLicensedV2` object. The acceptance layer verified the signatures
    /// (`palw_panel_v2::validate_supplementary_receipts_v3`); the fold re-derives every structural
    /// fact from its own state — the duty row, the panel and its assignment, the window, who is
    /// already counted — because the sync walk folds the same object without that layer.
    ///
    /// One funnel, in order:
    /// 1. **Who is new.** Each receipt names this claim and a seat on duty that is not credited and
    ///    holds no lock here, once, signed inside `[bound_daa, bound_daa + window_receipt]` and not
    ///    after this block; the set lands by the receipt deadline. A `Valid` carries its assigned
    ///    mask; `Incapable` is refused on the liveness floor. A set of abstentions only on a claim
    ///    whose `unserved_seen` is already latched has nothing to record and is refused.
    /// 2. **The backed subset and its price (SR-6, L-1).** The new `Valid`s are recounted with the
    ///    claim's counted signers ([`palw_rcore_counted_masks_v1`], Q-3) and priced
    ///    `lock_{max(k, 2)}` ([`PalwFoldReadV1::rcore_lock_required_v1`]). A `Valid` whose seat cannot
    ///    post that lock is unbacked and moves nothing — no lock, no credit, no bit, no count — and
    ///    its seat may carry it again later. If dropping it takes `max(k, 2)` from 3 to 2 the rest are
    ///    re-tested once at `lock_2` (k′ ∈ {2, 3}, so it falls at most once).
    /// 3. **Locks (Q-4, L-3).** Each backed `Valid` locks that price with its own mask and the cut.
    /// 4. **Credit (§9.1 Q3).** Backed `Valid`s and every `Sampled` are credited for seat pay;
    ///    `Unavailable` and `Incapable` are not a discharge the pool pays.
    /// 5. **The record (Q-3, Q-5, SR-1 cond. 2).** `served_mask` gains the backed `Valid` seats' bits
    ///    and nothing else; `unserved_seen` latches on any `Unavailable`, `Incapable` or `Sampled`
    ///    (C1); `basis_k` becomes the recount and is never lowered. On the first crossing to
    ///    `basis_k ≥ 2` of a licence that recorded its door, the door is re-recorded as Coverage if
    ///    any counted mask is partial, else Quorum, and an attempt's anchor settles (V-8). A licence
    ///    that recorded no door (folded before the licence arms stage R-core+'s record) already
    ///    ticked in `license_claim`, so it is not ticked twice.
    /// 6. **SR-1b** where [`palw_rcore_supplementary_flips_v1`] holds — the one rule S-2's V2 door
    ///    (`stage_supplementary_v1`) flips by: `escrow_released` is set on the staged record and the
    ///    producer's ledger moves by the escrow term in the same write (`move_commitment`, SR-3).
    /// 7. **Q-5's re-arm** ([`palw_rcore_upgrades_v1`]): the set that lifts an S2 licence off DL-1's
    ///    gate re-derives its deadline in this block, as the V2 door's does.
    pub(super) fn credit_supplementary_receipts_v3(
        &mut self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        receipts: &[crate::palw_panel_v2::PalwSeatReceiptV3],
        ctx: &PalwBlockContextV2,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_panel_v2::PalwReceiptVerdictV2 as Verdict;
        let refused = |why: String| PalwStateV2Error::SupplementaryV3Refused { claim: claim_id, why };
        if !matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
            return Err(refused("the claim is not licensed".into()));
        }
        // RFC-0006: a claim drawn per layer shard takes no whole-object supplementary set (its seats attest cells).
        if self.claim_licenses_by_parts(&claim_id) {
            return Err(refused("the claim was drawn per layer shard and licenses by its parts".into()));
        }
        let duties = self
            .state
            .panel_duties
            .get(&claim_id)
            .map(|row| row.seats.clone())
            .ok_or_else(|| refused("the claim's panel holds no duty row".into()))?;
        let panel = self.state.panels.get(&claim_id).cloned().ok_or_else(|| refused("no bound panel".into()))?;
        let deadline = palw_claim_receipt_deadline_v1(&self.state, self.params, &claim_id, claim)?
            .ok_or_else(|| refused("no bound panel".into()))?;
        if receipts.is_empty() {
            return Err(refused("it carries no receipt".into()));
        }
        if ctx.daa_score > deadline {
            return Err(refused("the receipt window has closed".into()));
        }
        let now = ctx.daa_score;
        let seat_count = panel.seats.len().min(u16::MAX as usize) as u16;
        let assignment = crate::palw_verification_v2::palw_segment_assignment_v2(panel.anchor, claim_id, seat_count);

        // 1. Who is new, and what each said.
        let mut seen: Vec<PalwBondKeyV2> = Vec::new();
        let mut valid: Vec<(usize, &crate::palw_panel_v2::PalwSeatReceiptV3)> = Vec::new();
        let mut sampled: Vec<crate::palw_panel_v2::PalwSeatReceiptV2> = Vec::new();
        let mut unserved = false;
        for signed in receipts {
            let receipt = &signed.receipt;
            if receipt.claim != claim_id {
                return Err(refused(format!("a receipt names claim {}", receipt.claim)));
            }
            let Some(index) = panel.seats.iter().position(|seat| seat.bond == receipt.seat_bond) else {
                return Err(refused(format!("bond {:?} holds no seat on this panel", receipt.seat_bond)));
            };
            match duties.get(&receipt.seat_bond) {
                None => return Err(refused(format!("bond {:?} is not on duty", receipt.seat_bond))),
                Some(at) if *at != 0 => return Err(refused(format!("seat {:?} is already credited", receipt.seat_bond))),
                Some(_) => {}
            }
            if self.state.slashable_locks.contains_key(&(receipt.seat_bond, claim_id)) {
                return Err(refused(format!("seat {:?} is already counted", receipt.seat_bond)));
            }
            if seen.contains(&receipt.seat_bond) {
                return Err(refused(format!("seat {:?} appears twice", receipt.seat_bond)));
            }
            if receipt.signed_daa < panel.bound_daa || receipt.signed_daa > deadline || receipt.signed_daa > now {
                return Err(refused(format!("seat {:?} signed outside the receipt window", receipt.seat_bond)));
            }
            seen.push(receipt.seat_bond);
            match receipt.verdict {
                Verdict::Valid => {
                    if signed.segments != assignment.mask_of(index as u16) {
                        return Err(refused(format!("seat {:?} attests a mask it was not assigned", receipt.seat_bond)));
                    }
                    valid.push((index, signed));
                }
                Verdict::Incapable => {
                    if !palw_seat_may_plead_incapable_v2(claim.class_id, self.params.base_class_id) {
                        return Err(refused(format!("seat {:?} pleads Incapable on the liveness floor", receipt.seat_bond)));
                    }
                    unserved = true;
                }
                Verdict::Unavailable { .. } => unserved = true,
                // C1: audit, not service — credited for pay below, never a served bit.
                Verdict::Sampled => {
                    unserved = true;
                    sampled.push(receipt.clone());
                }
            }
        }
        // An abstention is neither credited nor locked, so its seat stays uncounted and the same
        // signed receipt could ride again; all it records is the `unserved_seen` latch, so once that
        // is set a set of abstentions only is refused (the acceptance layer's same test, M4 review
        // finding 4).
        if valid.is_empty() && sampled.is_empty() && claim.rcore.unserved_seen {
            return Err(refused("abstentions only, on a claim whose unserved_seen is already latched".into()));
        }

        // 2. The backed subset, recounted with the signers already counted, and its one price.
        let (counted, segments) = palw_rcore_counted_masks_v1(&self.state, &claim_id);
        let old_k = claim.rcore.basis_k;
        let recount = |set: &[(usize, &crate::palw_panel_v2::PalwSeatReceiptV3)]| -> u8 {
            let mut masks = counted.clone();
            masks.extend(set.iter().map(|(_, signed)| signed.segments));
            palw_receipt_set_basis_k_v1(&masks, segments).max(old_k)
        };
        // Every `Valid` is backed where the lock ledger is dormant; `palw_objective_offence` is a
        // prerequisite of the fence, so past it this always prices.
        let locks = self.extras.objective_offence_at(now);
        // ADR-0160 lane liab (AG-3): and never from a frozen seat (the map is empty below the fence).
        let backs = |builder: &Self, seat: &PalwBondKeyV2, price: u128| {
            !crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&builder.state, seat)
                && (!locks || builder.slashable_available(seat, now) >= price)
        };
        let k0 = recount(&valid);
        let first = self.read().rcore_lock_required_v1(claim, k0);
        let mut backed = valid.clone();
        backed.retain(|(_, signed)| backs(self, &signed.receipt.seat_bond, first));
        let mut k = recount(&backed);
        if k.max(PALW_RCORE_FINAL_BASIS_K_V1) < k0.max(PALW_RCORE_FINAL_BASIS_K_V1) {
            let second = self.read().rcore_lock_required_v1(claim, k);
            backed.retain(|(_, signed)| backs(self, &signed.receipt.seat_bond, second));
            k = recount(&backed);
        }
        let price = self.read().rcore_lock_required_v1(claim, k);

        // 3. Locks, each with the mask its seat attested.
        if locks {
            for (_, signed) in &backed {
                self.lock_counted_seat_v1(signed.receipt.seat_bond, claim_id, price, now, signed.segments, segments)?;
            }
        }

        // 4. Credit: the backed `Valid`s and every `Sampled`.
        let mut credit: Vec<crate::palw_panel_v2::PalwSeatReceiptV2> =
            backed.iter().map(|(_, signed)| signed.receipt.clone()).collect();
        credit.extend(sampled);
        self.credit_seat_receipts(claim_id, &credit, now)?;

        // 5. The record.
        let mut staged = claim.clone();
        for (index, _) in &backed {
            if *index < 32 {
                staged.rcore.served_mask |= 1u32 << index;
            }
        }
        staged.rcore.unserved_seen |= unserved;
        staged.rcore.basis_k = k;
        if old_k < PALW_RCORE_FINAL_BASIS_K_V1 && k >= PALW_RCORE_FINAL_BASIS_K_V1 && staged.rcore.licence_door.is_some() {
            staged.rcore.licence_door = Some(palw_rcore_upgraded_door_v1(&self.state, &claim_id));
            // RFC-0018 Gate C's settled-anchor site (the identity below its fence).
            if matches!(claim.source, PalwClaimSourceV2::Attempt) && crate::palw_rfc0018_v1::palw_gate_c_settles_anchor_v1(self.params, claim) {
                self.settle_anchor(now, true)?;
            }
        }

        // 6. SR-1b.
        // The flag, by the one rule S-2's V2 door flips by (`stage_supplementary_v1`).
        if palw_rcore_supplementary_flips_v1(self.params, &self.state, &claim_id, &staged, deadline, now) {
            staged.rcore.escrow_released = true;
        }
        if staged.rcore != claim.rcore {
            let upgraded = palw_rcore_upgrades_v1(claim, &staged);
            // SR-3's funnel, as the V2 door: the producer's ledger moves by `commitment(claim) −
            // commitment(staged)` in the same write — the escrow term leaves it when the flag flips
            // (nothing else this door stages moves the commitment).
            self.move_commitment(claim, &staged, now)?;
            self.write_claim(claim_id, Some(staged))?;
            // 7. Q-5 (M4): the first crossing lifts DL-1's gate — the claim now finalizes at
            //    `max(L + window_challenge_at(L), U)` for this block U, re-derived, never remembered.
            if upgraded {
                self.rearm_claim_deadline_dl1_v1(claim_id, now)?;
            }
        }
        Ok(())
    }

    /// **V3S-01: a counted seat's full-replay V2 `Valid` widens its lock to the whole cut** (the V2
    /// door, [`palw_rcore_v2_widens_seat_v1`]). The seat now vouches for every segment, so its lock
    /// records the full mask and the cut (L-3: Q-6 and C7 place a fault against what it attested) —
    /// and nothing else moves: the amount is the price it was locked at (Q-4, never repriced), the
    /// expiry and the second clock are the lock's own (the `Final` re-dates every lock of the claim).
    pub(super) fn widen_counted_seat_v1(
        &mut self,
        seat: PalwBondKeyV2,
        claim_id: Hash64,
        full: crate::palw_verification_v2::PalwSegmentMaskV2,
        segments: u16,
    ) -> Result<(), PalwStateV2Error> {
        let Some(lock) = self.state.slashable_locks.get(&(seat, claim_id)).copied() else { return Ok(()) };
        if lock.attested == full && lock.segments == segments {
            return Ok(());
        }
        self.write_slashable_lock(
            (seat, claim_id),
            Some(crate::palw_panel_var_v1::PalwSlashableLockV1 { attested: full, segments, ..lock }),
        )?;
        Ok(())
    }

    /// **ADR-0152 SR-10 / L-3: the lock of one `Valid` a supplementary set newly counts** —
    /// `lock_valid_seat`'s record, carrying the segments the seat attested (its receipt's assigned
    /// mask) and the claim's cut, so Q-6 places a fault against what the seat vouched for. The caller
    /// has found the seat backed at `required`; a seat already locked keeps its lock (Q-4).
    pub(super) fn lock_counted_seat_v1(
        &mut self,
        seat: PalwBondKeyV2,
        claim_id: Hash64,
        required: u128,
        now_daa: u64,
        attested: crate::palw_verification_v2::PalwSegmentMaskV2,
        segments: u16,
    ) -> Result<(), PalwStateV2Error> {
        if self.state.slashable_locks.contains_key(&(seat, claim_id)) {
            return Ok(());
        }
        // Lane V02 (post-launch): a supplementary Valid's lock takes the same shortened life as a
        // panel Valid's — `window_court` below the fence, `PALW_FINAL_LOCK_LIFE_DAA_V1` past it.
        // G-13 (lane g14-close-legacy): `window_court` for a claim accepted past `palw_legacy_public_filer_v1`.
        let expiry_daa = crate::palw_panel_var_v1::palw_panel_liability_expiry_v1(now_daa, self.valid_lock_life_v1(&claim_id, now_daa));
        self.write_slashable_lock(
            (seat, claim_id),
            Some(crate::palw_panel_var_v1::PalwSlashableLockV1 {
                claim: claim_id,
                amount: required,
                expiry_daa,
                settled_at_final: self.state.settled_attempt_finals,
                attested,
                segments,
            }),
        )?;
        Ok(())
    }

    /// **ADR-0124 Decision 6: the escrow priced by the claim's work.** The unit is the heaviest
    /// canonical inference among the `Active`, weight-bearing model classes at this block —
    /// never the liveness floor, which is not a model and is paid at the schedule — and the claim
    /// is priced on the pwu its exposure is priced on (`palw_exposure_pwu_v1`): paid on the same
    /// number it can be slashed on. A floor claim, a claim of a class the state no longer holds,
    /// or a network with no weight-bearing model class is paid the escrow whole.
    ///
    /// **ADR-0145**: past the fence both the claim's measure and the unit it is measured against
    /// are the DERIVED work, so the ratio is arithmetic against arithmetic. Pricing only one of the
    /// two would compare MAC-equivalents to step leaves and pay every class the wrong fraction of
    /// its escrow, which is worse than either basis alone.
    pub(super) fn work_priced_escrow(&self, claim: &PalwClaimStateV2) -> u64 {
        if claim.class_id == self.params.base_class_id {
            return claim.escrowed_reward;
        }
        let canonical = self.canonical_per_draw(&claim.class_id, claim.accepted_daa);
        let Some(class) = self.state.classes.get(&claim.class_id) else { return claim.escrowed_reward };
        crate::palw_panel_economy_v1::palw_work_priced_reward_v1(
            claim.escrowed_reward,
            palw_exposure_pwu_v2(class, claim.pwu, canonical),
            self.work_price_unit_at(claim.accepted_daa),
        )
    }

    /// **ADR-0124 Decision 6's unit**: the dearest exposure pwu among the `Active`, weight-bearing
    /// model classes at this block — never the liveness floor's — and zero where no model class
    /// bears weight. The work price and the execution lane's credit (ADR-0125) read this one number.
    ///
    /// **ADR-0145**: the unit follows the CLAIM being priced, not the block, for the reason the
    /// weight does — the numerator is that claim's own measure, and dividing a measure taken on one
    /// basis by a unit taken on the other is a ratio of nothing. While the fence is dormant — every
    /// shipped preset — this returns exactly what the argument-free form returned before it
    /// existed, for every `accepted_daa`.
    ///
    /// **ADR-0145 I4, past `Params::palw_admission_independence`: a registration may not reprice
    /// anybody else.** The unit is a MAX, and every other class's reward is
    /// `escrow × its own pwu / unit` — so one registration naming a large canonical job divides
    /// every incumbent's pay by whatever multiple it chose, without mining a block. Past the fence
    /// a class enters the unit only once its registry row ADMITS CLAIMS, which is the state a class
    /// reaches by being verified rather than by being registered. A class with no row is left
    /// exactly where it was: the registry is what writes rows, and below its fence there are none —
    /// a rule that read "no row, no unit" there would price every network with a dormant registry
    /// at zero and pay every claim the whole escrow.
    ///
    /// Both fences are asked in ONE body ([`palw_work_price_unit_of_v1`]); this method supplies
    /// only the measure.
    pub(super) fn work_price_unit_at(&self, accepted_daa: u64) -> u64 {
        // **Past the bundle the denominator is not a statistic over the registered set at all** —
        // see [`palw_work_target_unit_v1`] for item 6 and why the chain's work target is the answer.
        if let Some(w) = palw_work_target_unit_v1(&self.state, self.extras.canonical_work_daa, accepted_daa) {
            return w;
        }
        palw_work_price_unit_of_v1(
            &self.state,
            &self.params.base_class_id,
            self.uncertified_weightless,
            // The claim's own era, like the work target arm above it: one claim, one basis.
            self.extras.admission_independence_at(accepted_daa),
            |id, record| {
                self.canonical_per_draw(id, accepted_daa).unwrap_or_else(|| palw_max_exposure_pwu_of_rule_v1(&record.pwu_rule))
            },
        )
    }

    /// **A seat's pay, keyed by its payee and accumulated.** One row per payee rather than one per
    /// (claim, seat): a busy seat's shares from several `Final`s merge into one coinbase output,
    /// so the queue grows by the number of DISTINCT payees, not by seats × claims, and the drain's
    /// latency argument (`PALW_V2_MAX_PAYOUTS_PER_BLOCK`) is not negated by a panel of five. The key
    /// is forced under [`PALW_STATE_V2_PANEL_PAYOUT_KEY_PREFIX`], after the claim rows and before the
    /// market's.
    pub(super) fn add_panel_payout(&mut self, payload: Hash64, amount: u64) -> Result<(), PalwStateV2Error> {
        if amount == 0 {
            return Ok(());
        }
        let key = palw_panel_payout_key_v1(&payload);
        let held = self.state.pending_payouts.get(&key).map(|row| row.amount).unwrap_or(0);
        let total = held.checked_add(amount).ok_or(PalwStateV2Error::Overflow("panel payout"))?;
        self.write_payout(key, Some(PalwPayoutV2 { payload, amount: total }))?;
        Ok(())
    }

    pub(super) fn write_claim_root(&mut self, key: Hash64, new: Option<Hash64>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.claim_roots, false, "native claim_roots writer")?;
        let old = match new {
            Some(root) => self.state.claim_roots.insert(key, root),
            None => self.state.claim_roots.remove(&key),
        };
        if old != new {
            self.entries.push(PalwDeltaEntryV2::ClaimRoot { key, old, new });
        }
        Ok(())
    }

    /// ADR-0087 Decision 4: a fee leg leaves the move as a payout to `payload`, keyed by the move
    /// so two moves in one block cannot share a row; `None` burns it (a class with no registrant).
    ///
    /// **The key is minted in the top 1/256 of the key space** (mainnet audit 2026-09-06, M-10).
    /// The drain and `palw_v2_payout_outputs` both take "the first N in `BTreeMap` key order", and
    /// a market row that sorted among the claim rows delayed the escrow release ADR-0042 Decision 10
    /// sizes the queue for. Forcing `PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX` puts every market row
    /// after every claim row whose id does not begin with that byte, so claims keep Decision 10's
    /// latency and market rows take the remainder. The other 63 bytes are the keyed hash they always
    /// were, so two legs of two moves still cannot share a row.
    pub(super) fn write_model_fee(
        &mut self,
        ctx: &PalwBlockContextV2,
        class_id: &Hash64,
        holder: &Hash64,
        leg: &[u8],
        payload: Option<Hash64>,
        amount: u64,
    ) -> Result<u64, PalwStateV2Error> {
        if amount == 0 {
            return Ok(0);
        }
        let Some(payload) = payload else { return Ok(amount) };
        let key = self.model_fee_key_v1(ctx, class_id, holder, leg);
        self.write_payout(key, Some(PalwPayoutV2 { payload, amount }))?;
        Ok(0)
    }

    /// Pure counterpart of the queue write: budget admission and payment use
    /// exactly the same immutable move/leg key before either ledger is changed.
    pub(super) fn model_fee_key_v1(&self, ctx: &PalwBlockContextV2, class_id: &Hash64, holder: &Hash64, leg: &[u8]) -> Hash64 {
        let mut h = keyed(PALW_STATE_V2_DOMAIN_MODEL_PAYOUT);
        h.update(class_id.as_byte_slice());
        h.update(holder.as_byte_slice());
        h.update(&ctx.daa_score.to_le_bytes());
        h.update(&self.model_moves.to_le_bytes());
        h.update(leg);
        let mut key_bytes = finish(h).as_bytes();
        key_bytes[0] = PALW_STATE_V2_MODEL_PAYOUT_KEY_PREFIX;
        Hash64::from_bytes(key_bytes)
    }

    /// **How many payout rows a market move of this shape will write, at most** (audit M-10).
    ///
    /// Counted BEFORE the move touches anything, so a move that would breach
    /// [`PALW_V2_MAX_PENDING_PAYOUTS`] is refused whole rather than half-applied. An upper bound,
    /// not exact: a leg whose amount rounds to zero or whose payee has no bond writes nothing
    /// (`write_model_fee`'s first two lines), so this over-counts in the safe direction.
    ///
    ///   * a buy: the owner leg and the contributor leg  → 2
    ///   * a carrier sell: those two plus `sell-net`     → 3
    ///   * an EVM sell: those two; the net leg is an ADR-0089 Decision 7 settlement credit, and
    ///     "the coinbase never emits it"                 → 2
    ///   * a seed: fee-free by ADR-0090 Decision 2       → 0
    pub(super) fn model_payout_rows_would_add(pays_net_via_coinbase: bool, is_seed: bool) -> usize {
        if is_seed {
            0
        } else if pays_net_via_coinbase {
            3
        } else {
            2
        }
    }

    /// **ADR-0042 Decision 10's queue has a ceiling** (audit M-10). Refuses a market move whose fee
    /// legs would push `pending_payouts` past [`PALW_V2_MAX_PENDING_PAYOUTS`]. Called first in each
    /// of the three move functions, before any write.
    pub(super) fn check_model_payout_room(&self, rows: usize) -> Result<(), PalwStateV2Error> {
        let held = self.state.pending_payouts.len();
        if held.saturating_add(rows) > PALW_V2_MAX_PENDING_PAYOUTS {
            return Err(PalwStateV2Error::ModelPayoutQueueFull { held, want: rows, cap: PALW_V2_MAX_PENDING_PAYOUTS });
        }
        Ok(())
    }

    pub(super) fn write_panel(&mut self, key: Hash64, new: Option<PalwPanelStateV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.panels, false, "native panels writer")?;
        if let Some(panel) = &new {
            self.record_staged_panel_v1(key, panel)?;
        }
        let old = match &new {
            Some(record) => self.state.panels.insert(key, record.clone()),
            None => self.state.panels.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Panel { key, old, new });
        Ok(())
    }

    pub(super) fn write_court(&mut self, key: Hash64, new: Option<PalwCourtSessionStateV2>) -> Result<(), PalwStateV2Error> {
        if let Some(record) = &new {
            self.state.require_paid_consumer_backstop_v1(&record.claim, record.deadline_daa)?;
            if !self.state.court_sessions.contains_key(&key) {
                self.state.require_paid_consumer_dispute_v1(&record.claim, record.opened_daa, record.deadline_daa)?;
            }
        }
        let old = match &new {
            Some(record) => self.state.court_sessions.insert(key, record.clone()),
            None => self.state.court_sessions.remove(&key),
        };
        if let Some(previous) = &old {
            let count = self.state.open_courts_by_claim.get_mut(&previous.claim).expect("index tracks every session");
            *count -= 1;
            if *count == 0 {
                self.state.open_courts_by_claim.remove(&previous.claim);
            }
            // **By SESSION, not by the key this call would recompute.** The index holds at most
            // one entry per session, and since ADR-0082 C-5 the key is a function of state OUTSIDE
            // the session record (the class's `fused_attention`, reached through the claim) — so a
            // removal that recomputed the key would miss the entry on any path where that state
            // has moved since the insert. There is one such path today, the sweep's defensive
            // "the claim is gone, drop the session" branch, and the failure it produces is not a
            // stale row but `MissingSession` on every later block: a chain halt. Removing by
            // session id cannot miss, and the scan is over the open sessions, which
            // `CourtSessionsAtCapacity` bounds.
            self.state.court_deadlines.retain(|(_, session_id)| *session_id != key);
            // **The challenger's stake comes back HERE**, so reserve and release are one pair in
            // one place. Opening a court charges the challenger the claim's own `reserved`; five
            // different sites remove a session — two sweeps, a close, a retirement, a re-open —
            // and a release each of them had to remember is a release one of them eventually will
            // not. The claim is still present at every removal (`retire_claim` closes its sessions
            // before it drops the claim), and a session whose claim is somehow gone had nothing
            // countable to give back.
            //
            // Only on a REMOVAL. `write_court` is also how a session is rewritten in place — the
            // abandoned-ladder refresh re-indexes the same session under the backstop — and giving
            // the stake back there would release it without the session ending, so a challenger
            // would hold an open court for free after one refresh.
            // ADR-0152 A-6: past `palw_rcore_plus` a court's stake never entered `reserved_exposure`
            // (`reserve_challenger_v1`), so nothing comes back out of it.
            if new.is_none()
                && self.params.rcore_plus_from_daa().is_none()
                && let Some(claim) = self.state.claims.get(&previous.claim)
            {
                let held = self.state.reserved_exposure.get(&previous.challenger_bond).copied().unwrap_or(0);
                let back = held.saturating_sub(claim.reserved);
                self.write_exposure(previous.challenger_bond, if back == 0 { None } else { Some(back) })?;
            }
        }
        if let Some(record) = &new {
            *self.state.open_courts_by_claim.entry(record.claim).or_insert(0) += 1;
            let next = court_next_deadline_v2(&self.state, record);
            self.state.court_deadlines.insert((next, key));
        }
        // ADR-0152 A-6 (S-SPEC 1g): the challenger index follows the session, old out, new in.
        if let Some(previous) = &old {
            self.state.courts_by_challenger.remove(&(previous.challenger_bond, key));
        }
        if let Some(record) = &new {
            self.state.courts_by_challenger.insert((record.challenger_bond, key));
        }
        let removed = new.is_none();
        let touched_claim = new.as_ref().map(|record| record.claim).or_else(|| old.as_ref().map(|record| record.claim));
        self.entries.push(PalwDeltaEntryV2::Court { key, old, new });
        // **RFC-0002 F7: an IR dissection is a row ABOUT a session, so it dies with it** — here, for
        // the close groups' reason below: a cleanup each removal site had to remember is a cleanup
        // one of them eventually would not, and `assert_internal_consistency` refuses an orphan.
        if removed && self.state.tir_dissections.contains_key(&key) {
            self.write_tir_dissection(key, None)?;
        }
        // **ADR-0080 design A: a close group is a row ABOUT a session, so it dies with it.**
        //
        // Here rather than at each of the five removal sites, for the reason written above the
        // exposure release: a cleanup each of them has to remember is a cleanup one of them
        // eventually will not, and an orphaned group is a row nothing sweeps whose bytes stay in
        // the state root forever. `assert_internal_consistency` refuses such a state, so the
        // failure would be a node refusing its own tip.
        //
        // **W10 as audit H-2 and L-9 corrected it: the deposit is collected HERE, on every
        // ending.** It used to be taken on two of five — the backstop sweep's own
        // `failed_declarer_side` and an undecodable completion — while `write_court(id, None)`
        // deleted the row on all the others (a real verdict, a rung no-show either way, an
        // unanswered opening, a retirement) with the deposit uncollected. So a declarer that
        // opened a 27-chunk row and never assembled it paid nothing whenever the session ended by
        // any route but its own lapse, which is the free option "junk pays; honesty is refunded"
        // exists to remove. With two live rows it was worse: `failed_declarer_side` finds the
        // CHALLENGER first, convicts, and the executor's row was deleted here for free (L-9) —
        // making an unassembled executor declaration strictly better than no declaration at all.
        //
        // The rule needs no `is_complete()` test and deliberately does not make one: **a group
        // still in the table when its session ends did not deliver the close it pinned.** W7's
        // honest path removes its own row before the decoded `CourtClosed` reaches this function
        // (`end_close_group_delivered`), so the only rows left here are rows nothing was filed
        // against.
        if removed {
            for side in [PalwCourtSideV1::Challenger, PalwCourtSideV1::Executor] {
                if self.state.court_close_groups.contains_key(&(key, side)) {
                    forfeit_close_deposit_v1(self, key, side)?;
                    self.write_court_close_group((key, side), None)?;
                }
            }
        }
        // **Lane PL part C × G14 (SHARD): a court session starts or ends a lane-A claim's hold** — here, at the one writer of
        // sessions, so every opener and every ending moves the deadline as DL-1 derives it. Dormant: the hold reaches no claim on
        // any shipped preset.
        if let Some(claim_id) = touched_claim {
            self.sync_v2_expiry_hold_v1(claim_id)?;
        }
        Ok(())
    }

    /// **Lane PL part C × G14: re-derive the deadline of a `Provisional` / `PanelBound` lane-A claim the hold reaches** (after a
    /// court session opened or ended on it): none while an accusation is pending, DL-1's row once nothing is. (An S2 licence's
    /// court moves are the licensed rows' own: `CourtOpened` disarms, `rearm_claim_after_court_close` re-derives.)
    pub(super) fn sync_v2_expiry_hold_v1(&mut self, claim_id: Hash64) -> Result<(), PalwStateV2Error> {
        let Some(claim) = self.state.claims.get(&claim_id).cloned() else { return Ok(()) };
        if !matches!(claim.phase, PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. })
            || !self.state.palw_v2_expiry_hold_applies_v1(self.params, &claim_id, &claim)
        {
            return Ok(());
        }
        self.disarm_deadline(claim_id);
        if let Some(at) = palw_rcore_deadline_v1(&self.state, self.params, &claim_id, &claim, None)? {
            self.arm_deadline(at, claim_id)?;
        }
        Ok(())
    }

    /// **A chunk arriving into a group that already exists** (audit H-3): the bit and the bytes,
    /// journalled as themselves.
    ///
    /// Not `write_court_close_group(key, Some(grown_record))`, which is how it was written and
    /// which put the whole accumulated group into the delta twice per arrival. The row's
    /// `assembly_deadline_daa` never moves on an arrival, so the deadline index needs no touch
    /// here — the invariant that makes that true is asserted by `assert_internal_consistency`,
    /// which recomputes the index from the records.
    pub(super) fn write_court_close_chunk(
        &mut self,
        key: (Hash64, PalwCourtSideV1),
        index: u8,
        bytes: Vec<u8>,
    ) -> Result<(), PalwStateV2Error> {
        // RFC-0006: the chunk's exact-capacity clone into the group and, for a new index, its membership; the
        // caller-owned `bytes` moves into the journal (retained for the transition, allocated by its decoder).
        if self.native_writers_tracking_v1() {
            let absent = self.state.court_close_groups.get(&key).is_some_and(|group| !group.chunks.contains_key(&index));
            let mut q = super::native_writers::NativeWriteQuoteV1::new("native court close chunk writer");
            q.exact::<u8>(bytes.len() as u64)?.member_if::<u8, Vec<u8>>(absent)?.slots(1)?;
            self.admit_native_write_v1(q)?;
        }
        let group = self.state.court_close_groups.get_mut(&key).expect("a chunk only arrives into a group the caller just read");
        group.present |= 1u64 << index;
        group.chunks.insert(index, bytes.clone());
        self.entries.push(PalwDeltaEntryV2::CourtCloseChunkArrived { key, index, bytes });
        Ok(())
    }

    /// **The one removal that owes nothing** (ADR-0080 W7): the group whose last chunk assembled,
    /// hashed to its own `close_digest` and decoded to this session's `CourtClosed`.
    ///
    /// Called immediately before that close is applied, so by the time [`Self::write_court`]
    /// removes the session there is no row of this side left to charge. Its own name rather than a
    /// bare `write_court_close_group(key, None)` because the DIFFERENCE between this and every
    /// other removal is the whole of W10's "junk pays; honesty is refunded", and a difference that
    /// lives in which line happens to run is a difference the next editor deletes.
    pub(super) fn end_close_group_delivered(&mut self, key: (Hash64, PalwCourtSideV1)) -> Result<(), PalwStateV2Error> {
        self.write_court_close_group(key, None)?;
        Ok(())
    }

    pub(super) fn write_epoch(&mut self, key: Hash64, new: Option<PalwEpochCounterV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.epoch_counters, false, "native epoch_counters writer")?;
        let old = match &new {
            Some(record) => self.state.epoch_counters.insert(key, record.clone()),
            None => self.state.epoch_counters.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::Epoch { key, old, new });
        Ok(())
    }

    pub(super) fn write_receipt_epoch(&mut self, key: Hash64, new: Option<PalwEpochCounterV2>) -> Result<(), PalwStateV2Error> {
        self.reserve_map_row_v1(&key, &new, |s| &s.receipt_epoch_counters, false, "native receipt_epoch_counters writer")?;
        let old = match &new {
            Some(record) => self.state.receipt_epoch_counters.insert(key, record.clone()),
            None => self.state.receipt_epoch_counters.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::ReceiptEpoch { key, old, new });
        Ok(())
    }

    // ---- deadline index (never in the delta: rebuilt facts, not primary data) ----

    pub(super) fn arm_deadline(&mut self, deadline: u64, claim: Hash64) -> Result<(), PalwStateV2Error> {
        let deadline = self.state.staged_retirement_floor_v1(&claim, deadline);
        // **ADR-0152 DL-1's DA rows (M3), at the one primitive every arm site calls**: a live claim a
        // seat session pauses owes no deadline, a terminal claim with a session open owes none (its
        // retirement waits), and a retirement arms no earlier than one DAA after the last close —
        // exactly [`palw_rcore_deadline_v1`]'s rows, so no site (a bind, a redraw, a court clearing, a
        // void or a Final) can arm what DL-1 would not rebuild. `da_claims` is empty below
        // `palw_rcore_plus`, where this is the plain insert it always was.
        let mut deadline = deadline;
        // Lane LG14-A: DL-1's dispute-reservation row, at the same primitive — a claim a live reservation holds owes none.
        if self.state.palw_legacy_dispute_holds_v1(&claim) {
            return Ok(());
        }
        // **RFC-0008 v2: a licensed claim whose work session is not ready owes no `Final` deadline** — the same pause a DA session
        // takes, at the same primitive, so no arm site (a licence, a court close, a DA close, a re-arm) can arm what DL-1 would not
        // rebuild. Only a LICENSED claim's deadline is the `Final` one; a bind or a receipt deadline still fires. The roots table is
        // empty below the fence, where this is one lookup on an empty map.
        if self.state.exec_v2.roots.contains_key(&claim)
            && self.state.claims.get(&claim).is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }))
            && self.state.exec_v2_holds_final_v1(&claim)
        {
            return Ok(());
        }
        // Lane PL part C × G14 (SHARD): DL-1's hold row, at the same primitive — a held lane-A claim owes none.
        if self
            .state
            .claims
            .get(&claim)
            .is_some_and(|c| !c.phase.is_terminal() && self.state.palw_v2_expiry_held_v1(self.params, &claim, c))
        {
            return Ok(());
        }
        if let Some(record) = self.state.da_claims.get(&claim) {
            let terminal = self.state.claims.get(&claim).is_some_and(|c| c.phase.is_terminal());
            if (!terminal && record.open_seat_sessions > 0) || (terminal && record.open_sessions() > 0) {
                return Ok(());
            }
            if terminal && let Some(closed) = record.last_closed_daa {
                deadline = deadline.max(closed.saturating_add(1));
            }
        }
        // RFC-0006: the one index node this arm can add (an already-armed identical row adds none).
        self.reserve_index_members_v1::<(u64, Hash64), ()>(
            u64::from(!self.state.deadlines.contains(&(deadline, claim))),
            "native deadline index",
        )?;
        self.state.deadlines.insert((deadline, claim));
        Ok(())
    }

    pub(super) fn disarm_deadline(&mut self, claim: Hash64) {
        // RFC-0006: removal only — the index is never in the delta (no journal slot) and a `BTreeSet::remove` frees and allocates
        // nothing, so there is nothing to admit. It moves no value and settles no obligation (RFC-0008: not a settlement leg).
        // A claim holds at most one deadline; the scan is over a set that shrinks as claims
        // resolve. (An index keyed the other way would dodge the scan but double the index
        // surface; measured need decides later, behavior cannot change.)
        if let Some(entry) = self.state.deadlines.iter().find(|(_, id)| *id == claim).copied() {
            self.state.deadlines.remove(&entry);
        }
    }

    // ---- shared accounting moves ----

    /// A rollback point for ADR-0058's merged-work fold: the state by value, the delta by
    /// length. `restore` must return the builder to EXACTLY this point or a skipped work would
    /// leave fingerprints in the root — which is why this is a full clone rather than an undo
    /// log: an `apply_*` that grows a new mid-write failure mode cannot corrupt a clone.
    #[cfg(test)]
    pub(super) fn checkpoint(&self) -> (PalwChainStateV2, usize) {
        (self.state.clone(), self.entries.len())
    }

    /// The aggregate construction owner retains this exact state copy until
    /// rollback or the final output ownership split. A local refusal occurs
    /// before Clone, never as a semantic rejection of the carried work.
    pub(super) fn try_checkpoint(&mut self) -> Result<(PalwChainStateV2, usize), PalwStateV2Error> {
        let requests =
            crate::palw_native_resource_v1::native_value_requests_v1(&self.state).map_err(PalwStateV2Error::NativeResource)?;
        self.native_reservation.grow_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
        Ok((self.state.clone(), self.entries.len()))
    }
    pub(super) fn restore(&mut self, checkpoint: (PalwChainStateV2, usize)) {
        self.state = checkpoint.0;
        self.entries.truncate(checkpoint.1);
        // The caches describe the state just replaced; the next reader rebuilds them from the
        // restored one (audit #13b).
        *self.inflight_index.get_mut() = None;
        self.registration_index = None;
        self.panel_horizon.set(None);
    }

    pub(super) fn reserve_for_claim(&mut self, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        // **ADR-0071 SA-3: possession proven by production.** This function is the ONE funnel
        // every accepted claim passes — the block's own attempt, a merged blue's attempt, and a
        // free-prompt commitment — so recording here is the same set the amendment names and a
        // future claim kind cannot forget it. First write only: the fact is "has produced at
        // least once", so re-stamping `issued_daa` on every later claim would make an idempotent
        // fact a per-block state write.
        //
        // **What this costs the state, said out loud.** One permanent row per `(class, bond)` that
        // has produced, and nothing removes it — the row IS the proof, so a sweep would take a
        // seat's eligibility away for having been quiet. The bound is `classes × bonds`: classes
        // are the registry (priced by ADR-0056 Decision 3) and a bond only earns a row by
        // PRODUCING a block, which is a whole accepted attempt with its collateral, its exposure
        // and its lottery behind it. That is a far dearer write than the `BondCapabilityDeclared`
        // SA-1 had to bound, which is why this one is not bounded and that one is.
        if self.capability_bound {
            let key = palw_capability_fact_key_v1(&claim.class_id, &claim.bond);
            if !self.state.capabilities.contains_key(&key) {
                self.write_capability(
                    key,
                    Some(PalwCapabilityStateV2 { class_id: claim.class_id, bond: claim.bond, issued_daa: claim.accepted_daa }),
                )?;
            }
        }
        let current = self.state.reserved_exposure.get(&claim.bond).copied().unwrap_or(0);
        // Option A: the claim's escrow joins its weight on the bond past the fence (0 before it), and a
        // free-prompt claim's receipt rights join them past the audit fence (#5). ADR-0152 SR-3's
        // first transition: what a new claim commits, which at acceptance is the whole reservation.
        let next = palw_claim_commitment_v1(self.params, claim, claim.accepted_daa)
            .and_then(|held| current.checked_add(held))
            .ok_or(PalwStateV2Error::Overflow("reserved_exposure"))?;
        self.write_exposure(claim.bond, Some(next))?;
        // ADR-0160 F-W: a new-rule claim's weight enters through its bond's capped term (`write_claim`).
        // RFC-0018 Gate C's provisional site: the claim's weight "as at acceptance" (the identity below its fence).
        if !crate::palw_weight_cap_v1::palw_weight_cap_applies_v1(self.params, claim) {
            self.state.bounded_immature = self
                .state
                .bounded_immature
                .checked_add(crate::palw_rfc0018_v1::palw_claim_provisional_weight_v1(self.params, claim))
                .ok_or(PalwStateV2Error::Overflow("bounded_immature"))?;
        }
        Ok(())
    }

    /// Release an immature claim's reservations (on `Final` and `Voided` alike — the exposure and
    /// the immature contribution both belong only to non-terminal claims).
    ///
    /// **ADR-0152 SR-3's terminal transition: what the claim commits NOW** — `claim` is the live
    /// record the terminal write replaces, so a licence that released the escrow term already gave
    /// that part back ([`Self::license_claim`], the V2 door) and only the rest leaves here. Below
    /// `palw_rcore_plus` no licence releases anything, so this is the whole reservation
    /// `reserve_for_claim` added, as before.
    pub(super) fn release_for_claim(&mut self, claim: &PalwClaimStateV2, now_daa: u64) -> Result<(), PalwStateV2Error> {
        let current = self.state.reserved_exposure.get(&claim.bond).copied().unwrap_or(0);
        let held = palw_claim_commitment_v1(self.params, claim, now_daa).ok_or(PalwStateV2Error::Overflow("reserved_exposure"))?;
        let next = current.checked_sub(held).ok_or(PalwStateV2Error::Overflow("reserved_exposure underflow"))?;
        self.write_exposure(claim.bond, if next == 0 { None } else { Some(next) })?;
        // ADR-0160 F-W: a new-rule claim's weight leaves through its bond's capped term (`write_claim`).
        // RFC-0018 Gate C's provisional site: exactly what `reserve_for_claim` added (the identity below its fence).
        if !crate::palw_weight_cap_v1::palw_weight_cap_applies_v1(self.params, claim) {
            self.state.bounded_immature = self
                .state
                .bounded_immature
                .checked_sub(crate::palw_rfc0018_v1::palw_claim_provisional_weight_v1(self.params, claim))
                .ok_or(PalwStateV2Error::Overflow("bounded_immature underflow"))?;
        }
        Ok(())
    }

    /// Debit `amount` from a bond's collateral and record it as slashed. **The only place value
    /// leaves a bond in this ruleset.**
    ///
    /// Audit C5 found no slash primitive anywhere in the tree: `void_claim` released the
    /// reservation and wrote a phase, so every "bond slash" in ADR-0042 — the court's
    /// `ExecutorGuilty`, Decision 7's producer default, a seat that reports falsely — cost
    /// exactly nothing. A ruleset whose every penalty is a state label is a ruleset with no
    /// penalties.
    ///
    /// Saturating on the collateral side rather than erroring: a bond may owe more than it holds
    /// (its exposure ceiling bounds work at risk, not the sum of every way it can be convicted),
    /// and a conviction that cannot be recorded because the debtor is already empty must still
    /// be recorded. What it cannot do is go negative.
    ///
    /// The slashed value is BURNED. Where it might otherwise go — redistribution to the panel,
    /// to the challenger, to the next producer — is a live design question this deliberately does
    /// not answer; burning is the only destination that needs no policy and cannot be gamed by
    /// whoever would have received it.
    /// **Charge a SEAT — bounded by what the chain says, not by what a registrant wrote.**
    ///
    /// A seat's penalty is `claim.reserved`, which is `pwu_per_inference x slash_value_per_pwu`. Both factors
    /// come from the class REGISTRATION: the registrant picks the slash value and picks the pwu
    /// ceiling, and nothing bounded either beyond "not zero". So one registration could name a
    /// class whose every claim charges an arbitrary sum, get panels drawn from other operators'
    /// bonds, and empty them — with a single class object and one claim. It composes with the
    /// sortition ignoring execution capability: a seat that cannot run the class is silent, and
    /// silence is charged the same.
    ///
    /// You may put YOUR OWN money at risk on terms you choose; you may not put other people's at
    /// risk on terms you choose. The executor still stands behind `reserved` in full — it chose it
    /// and it collects on it — while a seat's exposure per claim is capped at the registry's own
    /// `min_collateral_sompi`: the most the network ever demanded anyone post to participate.
    pub(super) fn slash_seat(&mut self, bond: PalwBondKeyV2, amount: u128, min_collateral_sompi: u64) -> Result<(), PalwStateV2Error> {
        self.slash_bond(bond, amount.min(min_collateral_sompi as u128)).map(|_| ())
    }

    ///
    /// **Returns its debit** (ADR-0152 R-2, S-4): `min(amount, collateral)`, what actually left the
    /// bond — the conviction funnel's `collected` is the sum of these over the bonds whose exit was
    /// shut at the conviction's opening, never the nominal tier.
    pub(super) fn slash_bond(&mut self, bond: PalwBondKeyV2, amount: u128) -> Result<u64, PalwStateV2Error> {
        let record = self.state.bonds.get(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.clone();
        let debit = u64::try_from(amount.min(record.collateral as u128)).expect("clamped to a u64 collateral");
        if debit == 0 {
            return Ok(0);
        }
        let mut slashed = record;
        slashed.collateral -= debit;
        slashed.slashed = slashed.slashed.checked_add(debit).ok_or(PalwStateV2Error::Overflow("cumulative slash"))?;
        self.write_bond(bond, Some(slashed))?;
        Ok(debit)
    }

    /// **2026-09-24 DoS audit #12 (b): destroy `amount` of a registrant's bond, whole.**
    ///
    /// The same write as [`Self::slash_bond`] — the bond's `collateral` falls, `slashed` rises, one
    /// `Bond` delta entry — because `slashed` is the chain's one record of bond money that must
    /// never be minted back: [`palw_bond_burn_obligation_v2`] makes the release spend leave it
    /// unclaimed and takes it out of the block's fee pool. Unlike a slash it never clamps: a burn
    /// that the bond cannot pay in full is refused (`ClassRegistrationBurnUnaffordable`) — the
    /// caller checks affordability first, and this refuses again rather than burning less.
    ///
    /// `slashed` therefore also counts registration burns past the fence. Nothing reads it as
    /// "convicted": the burn obligation reads it as what the release must destroy. The
    /// free-collateral readers that used to subtract it from the already-net `collateral` — the
    /// fold's own readiness predicate among them, which made paying this price a way to drop out of
    /// the ready count — read [`palw_bond_free_collateral_v1`] instead, which past the fence does
    /// not (review of #12).
    pub(super) fn burn_registration_fee(&mut self, bond: PalwBondKeyV2, amount: u64) -> Result<(), PalwStateV2Error> {
        let collateral = self.state.bonds.get(&bond).ok_or(PalwStateV2Error::MissingBond(bond))?.collateral;
        if collateral < amount {
            return Err(PalwStateV2Error::Overflow("registration burn exceeds the registrant's collateral"));
        }
        self.slash_bond(bond, amount as u128).map(|_| ())
    }

    /// **C-03 (mainnet audit 2026-09-11 deep fence): bill court time to the LOSING party.**
    ///
    /// The court is clocked in DAA but nothing charged for consuming it, so a party that expects to
    /// lose has a strictly-dominant incentive to play every turn at the last legal DAA — dragging one
    /// dispute to its whole `window_court` backstop at zero marginal cost, deferring its slash,
    /// freezing the honest claim, and tying up the challenger's reserved exposure the entire time.
    /// Past the fence the party a session closes AGAINST pays, on top of its verdict slash, a share
    /// of the claim's own reserved stake proportional to how long the session ran:
    /// `claim.reserved × (close − opened) / window_court`, capped at `claim.reserved` (a full-window
    /// stall). So stalling now costs money that grows with the delay, while prompt play is ~free —
    /// and the PREVAILING party is billed nothing, which is the "refunded to the winner" the user
    /// chose, made whole by never being charged rather than by a refund transfer.
    ///
    /// Derived at close from the session's `opened_daa` and this block's DAA — no per-move accrual
    /// state, so no borsh migration. Burned like every other slash. **A no-op below the deep fence**
    /// (and on every network without a V2 bundle), so the court is byte-identical there; callers may
    /// invoke it unconditionally.
    ///
    /// Returns the nominal charge (0 where dormant): the conviction funnel adds it to a court
    /// conviction's `amount` (S-4).
    pub(super) fn charge_court_time_v1(
        &mut self,
        loser: PalwBondKeyV2,
        claim_reserved: u128,
        opened_daa: u64,
        close_daa: u64,
    ) -> Result<u128, PalwStateV2Error> {
        if !self.extras.audit_2026_09_11_deep_active {
            return Ok(0);
        }
        let window = self.params.window_court.max(1);
        let elapsed = close_daa.saturating_sub(opened_daa).min(window);
        // `claim_reserved` is at most a u64 exposure times a u64 slash value, so this fits u128; the
        // `/ window` keeps it ≤ `claim_reserved`, the same ceiling the verdict slash used.
        let charge = claim_reserved.saturating_mul(elapsed as u128) / (window as u128);
        self.slash_bond(loser, charge)?;
        Ok(charge)
    }

    /// Void a claim AND take the collateral it put at risk.
    ///
    /// `claim.reserved` is exactly `pwu_per_inference × slash_value_per_pwu` — the number the exposure ceiling
    /// exists to bound — so the penalty is the stake the claim itself named. No new parameter,
    /// and the punishment scales with the work claimed rather than with a constant somebody
    /// would have to pick.
    pub(super) fn void_and_slash(
        &mut self,
        id: Hash64,
        claim: &PalwClaimStateV2,
        voided_daa: u64,
        reason: PalwVoidReasonV2,
    ) -> Result<(), PalwStateV2Error> {
        self.void_and_slash_at(id, claim, voided_daa, reason, None).map(|_| ())
    }

    /// [`Self::void_and_slash`] inside a conviction (S-4): the producer's forfeit and, past
    /// `palw_rcore_plus`, its action tier (ADR-0152 §3.6), measured on `C₀` from `conv`'s opening (or
    /// the collateral before this void when no conviction is open) and on `G` read before the void.
    /// Returns the NOMINAL amount the bond was charged (the record's `amount`; `collected` is the
    /// funnel's).
    ///
    /// * **S0′** — the second `ReceiptTimeout`, `UnavailableQuorum`, `NotReplayBacked`: the forfeit
    ///   `w + esc + rr` and nothing else: no strike, no action, no record, no reward (the selector
    ///   below, unchanged).
    /// * **S1** — `ProducerWithholding` (past `palw_rcore_plus` only DA-7 writes it): the forfeit (a
    ///   released escrow's `E` comes from uncommitted stake, X7: the debit ignores the ledger) and a
    ///   strike ([`palw_rcore_strike_v1`]); the third strike inside 7,500 DAA adds
    ///   [`palw_rcore_s1s2_action_v1`].
    /// * **S2** — `CourtFraud` (a proven fraud before `Final`: a court's verdict, an execution-proving
    ///   kind 3, a kind 4) and `CourtDefault` (charged EXACTLY as `CourtFraud`, M2's rule): the forfeit
    ///   plus [`palw_rcore_s1s2_action_v1`], once — the void is the marker.
    ///
    /// Below the fence the action is 0 and this is the old charge, byte for byte.
    pub(super) fn void_and_slash_at(
        &mut self,
        id: Hash64,
        claim: &PalwClaimStateV2,
        voided_daa: u64,
        reason: PalwVoidReasonV2,
        conv: Option<&PalwConvictionV1>,
    ) -> Result<u128, PalwStateV2Error> {
        let rcore = self.params.rcore_plus_active_at(voided_daa);
        // Read before the void writes anything: `C₀` (the opening's, else the collateral now) and `G`.
        let (c0, g) = if rcore {
            let c0 = match conv {
                Some(conv) => conv.c0(&claim.bond),
                None => self.state.bonds.get(&claim.bond).map(|bond| bond.collateral).unwrap_or(0),
            };
            (c0, self.claim_g_v1(&id).map(|gains| gains.g()).unwrap_or(0))
        } else {
            (0, 0)
        };
        self.void_claim(id, claim, voided_daa, reason)?;
        // **Lane PL part C (ADR-0166): an expiry of unavailable verifiers is never a charge.** Whatever route a caller reaches
        // here by, `PanelUnavailable` ends the claim and takes nothing: no forfeit, no strike, no action.
        if matches!(
            reason,
            PalwVoidReasonV2::PanelUnavailable
                | PalwVoidReasonV2::SealUnavailable
                | PalwVoidReasonV2::BeaconUnavailable
                | PalwVoidReasonV2::PermissionlessNoCapablePanel
                | PalwVoidReasonV2::WorkRootExpired
                | PalwVoidReasonV2::WorkSliceProvenFalse
                | PalwVoidReasonV2::WorkSliceDefaulted
        ) {
            return Ok(0);
        }
        // **Option A: a PROVEN fraud forfeits the whole fraud gain it reached for — weight and
        // escrow.** The escrow was never paid (it is released at Final, and a void is pre-Final), so
        // this is not a claw-back: it is what makes a lie cost what it would have earned.
        //
        // **And past the 2026-09-23 audit fence, so does withholding** (the audit's #10, the
        // operator's decision): a producer whose default the data-availability court confirmed
        // (`ProducerWithholding`), or whose claim two independently drawn panels could not conclude
        // (the second `ReceiptTimeout`, charged at its call site in `sweep_deadlines`). Before it,
        // binding a claim and serving nothing cost the producer nothing while it held other bonds'
        // seat duty for two windows — a capital-time amplification the audit measured at 3.3
        // million to one. The price is the whole escrow-inclusive reservation, the same figure
        // CourtFraud takes, so withholding is never the cheaper way to fail. The cost falls on a
        // producer whose node is simply down as well; that is the trade the decision accepts.
        let escrow_forfeit = match reason {
            // A court default is charged exactly as a proven fraud (ADR-0152 F2 residual): only the
            // reason it is recorded under differs, so kind 3 cannot read it as a proof.
            PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::CourtDefault | PalwVoidReasonV2::CourtHeldVerdict => true,
            PalwVoidReasonV2::ProducerWithholding | PalwVoidReasonV2::ReceiptTimeout => self.extras.audit_2026_09_23_active,
            // ADR-0152 v3.1 S0′ (v22 skeleton): both are a second failed panel and are charged as the
            // second `ReceiptTimeout` is. M4's Q-5 gate writes 6 (`sweep_deadlines`); S-5 writes 5
            // (not landed), so 5 is still unreachable on every network.
            PalwVoidReasonV2::UnavailableQuorum | PalwVoidReasonV2::NotReplayBacked => self.extras.audit_2026_09_23_active,
            PalwVoidReasonV2::BindTimeout | PalwVoidReasonV2::NoCapablePanel => false,
            // ADR-0160 AG-2: the forfeiture's own void (`aggregate_on_conviction_v1` calls
            // `void_claim`, never this) — the collateral is taken whole beside it.
            PalwVoidReasonV2::AggregateForfeit => false,
            // Lane PL part C: an expiry of unavailable verifiers charges the producer nothing (never reaches here: the sweep voids it
            // through `void_claim`, and the guard below returns before any charge).
            PalwVoidReasonV2::PanelUnavailable => false,
            // RFC-0010: the permissionless Panel's non-fraud terminations (the guard above returns before any charge).
            PalwVoidReasonV2::SealUnavailable
            | PalwVoidReasonV2::BeaconUnavailable
            | PalwVoidReasonV2::PermissionlessNoCapablePanel => false,
            // RFC-0008 v2: an expired work session is a timeout, never a charge (the guard above returns first); a slice proven false or
            // defaulted is the kernel route's charge, never a second one here.
            PalwVoidReasonV2::WorkRootExpired | PalwVoidReasonV2::WorkSliceProvenFalse | PalwVoidReasonV2::WorkSliceDefaulted => false,
        };
        // A free-prompt claim's receipt rights (#5) are its fraud gain as the escrow is an attempt's,
        // so they go on the same reasons. The escrow slot is the ledger's (ADR-0160 E-5): past
        // `palw_capacity_escrow_at_licence` it is `m_c`, so an S0′ charge is the stage commitment and
        // the withheld `E` itself is burned by the void (never minted), not debited from the bond.
        let escrow = if escrow_forfeit {
            self.params
                .claim_escrow_term_v2(claim.accepted_daa, claim.escrowed_reward, &claim.class_id)
                .saturating_add(claim.rights_reserved)
        } else {
            0
        };
        let action = if !rcore {
            0
        } else {
            match reason {
                // S2's action on `CourtFraud` AND on `CourtDefault`. **The court default's action is a
                // user-approved addition to ADR §3.6's S2 table** (2026-09-24, the S-4 review's
                // deviation 2): a producer that falls silent in its own turn must never lose more
                // cheaply than one that plays and loses, so its default is charged exactly the losing
                // charge — forfeit, `min(10% · C₀, 3 G)` and the court time — while it still writes no
                // `CourtConviction` record and opens no reporter reward (a default proves nothing a
                // challenger could be paid for, and nothing a seat can be convicted by). A held
                // dissection's verdict (ADR-0152 §4-ter F3 (B), `CourtHeldVerdict`) is the producer's
                // and is charged as `CourtFraud` is.
                PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::CourtDefault | PalwVoidReasonV2::CourtHeldVerdict => {
                    palw_rcore_s1s2_action_v1(c0, g)
                }
                PalwVoidReasonV2::ProducerWithholding => {
                    let strike = palw_rcore_strike_v1(self.state.withholding_strikes(&claim.bond).unwrap_or(&[]), voided_daa);
                    if let Some(written) = strike.written {
                        self.write_strikes(claim.bond, Some(written))?;
                    }
                    if strike.escalates { palw_rcore_s1s2_action_v1(c0, g) } else { 0 }
                }
                // S0′ and S0: the forfeit (or nothing), never an action.
                PalwVoidReasonV2::ReceiptTimeout
                | PalwVoidReasonV2::UnavailableQuorum
                | PalwVoidReasonV2::NotReplayBacked
                | PalwVoidReasonV2::BindTimeout
                | PalwVoidReasonV2::NoCapablePanel
                | PalwVoidReasonV2::AggregateForfeit
                | PalwVoidReasonV2::PanelUnavailable
                | PalwVoidReasonV2::SealUnavailable
                | PalwVoidReasonV2::BeaconUnavailable
                | PalwVoidReasonV2::PermissionlessNoCapablePanel
                | PalwVoidReasonV2::WorkRootExpired
                | PalwVoidReasonV2::WorkSliceProvenFalse
                | PalwVoidReasonV2::WorkSliceDefaulted => 0,
            }
        };
        let nominal = claim.reserved.saturating_add(escrow).saturating_add(action);
        self.slash_bond(claim.bond, nominal)?;
        Ok(nominal)
    }

    /// Charge every seat whose verdict the panel's own quorum refuted.
    ///
    /// `served_won` is what the quorum concluded; a seat that reported the other way is the one
    /// contradicted. Iterated in the object's order, which is the order the acceptance layer
    /// validated, so two nodes charge the same seats in the same sequence.
    /// **A claim licenses** — the phase, the receipt deadline disarmed, the challenge deadline
    /// armed unless a court is already open. Spelled once for the whole-object licence and the
    /// last shard's part (ADR-0100 Decision 4), so the two cannot license differently.
    ///
    /// **ADR-0152 R-core+ (S-SPEC §3.4 step 5):** `claim` arrives with its `rcore` record staged by
    /// the door ([`Self::staged_licence_v1`]) and is written once. SR-3's flip transition: where the
    /// door released the escrow term, the bond's ledger gives it back in this write
    /// ([`Self::move_commitment`]). V-8: past `palw_rcore_plus` the anchor settles only for an
    /// attempt whose licence set recounted to `basis_k ≥ 2` — an S2 licence and every free-prompt
    /// licence never tick. The deadline is DL-1's ([`palw_rcore_deadline_v1`]).
    pub(super) fn license_claim(&mut self, claim_id: Hash64, claim: PalwClaimStateV2, daa_score: u64) -> Result<(), PalwStateV2Error> {
        crate::palw_panel_lifetime_v2::require_panel_receipt_time_v2(&self.state, self.params, &claim_id, daa_score)?;
        if self.state.paid_consumer_contract_v1(&claim_id).is_some() {
            // The licence's earliest Final is the one Final floor (`palw_claim_final_floor_v1`, the td0 rule; R6781-F merge), plus one.
            let through = palw_claim_final_floor_v1(&self.state, self.params, &claim_id, &claim, daa_score)?
                .checked_add(1)
                .ok_or(PalwStateV2Error::Overflow("paid consumer licence path"))?;
            if !self.state.paid_consumer_service_fits_v1(&claim_id, daa_score, through) {
                return Err(PalwStateV2Error::ExecV2Refused("licence exceeds immutable paid consumer service".into()));
            }
        }
        let rcore = self.params.rcore_plus_active_at(daa_score);
        // **The second clock ticks HERE past `palw_audit_2026_09_23`** (2026-09-24 DoS audit,
        // §4b): a licence needs a quorum's live signatures in the very block that carries it, so
        // heartbeat-only history cannot produce one. `Final` — which the DAA-driven deadline sweep
        // reaches on its own — keeps the tick only below the fence (`finalize_claim`).
        let attempt = matches!(claim.source, PalwClaimSourceV2::Attempt);
        let ticks = if rcore {
            attempt && claim.rcore.basis_k >= PALW_RCORE_FINAL_BASIS_K_V1
        } else {
            self.extras.audit_2026_09_23_active && attempt
        };
        // RFC-0018 Gate C's settled-anchor site: an uncertified licence settles nothing (asked as licensed; identity below its fence).
        if ticks && crate::palw_rfc0018_v1::palw_gate_c_settles_anchor_v1(self.params, &claim) {
            self.settle_anchor(daa_score, true)?;
        }
        let before = self.state.claims.get(&claim_id).cloned();
        // **ADR-0164 F-K (K3's denominator, K6's): a first licence is a claim that reached its receipt deadline in time**; a re-licence of
        // a claim already licensed (a supplementary or upgraded set) is not counted again.
        if !before.as_ref().is_some_and(|before| matches!(before.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })) {
            let credited = crate::palw_audit_door_v1::palw_capacity_claim_credited_v1(self.params, &claim);
            self.note_breaker_v1(daa_score, crate::palw_capacity_s567_v1::PalwBreakerEventV1::Licensed { credited })?;
            self.note_bond_breaker_v1(daa_score, licensed_bond_of(&claim), Some(false))?;
        }
        let mut licensed = claim;
        licensed.phase = PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: daa_score };
        if let Some(before) = &before {
            self.move_commitment(before, &licensed, daa_score)?;
        }
        self.write_claim(claim_id, Some(licensed.clone()))?;
        self.disarm_deadline(claim_id);
        if let Some(deadline) = palw_rcore_deadline_v1(&self.state, self.params, &claim_id, &licensed, Some(daa_score))? {
            self.arm_deadline(deadline, claim_id)?;
        }
        Ok(())
    }

    /// **SR-3's funnel for a write that keeps the claim live**: the producer's `reserved_exposure`
    /// moves by `commitment(before) − commitment(after)` at `now_daa` — the licence's release of the
    /// escrow term, SR-1b's late flip. `checked` both ways, so a double release fails deterministically
    /// on every node rather than being absorbed. The bond never changes under a claim.
    pub(super) fn move_commitment(
        &mut self,
        before: &PalwClaimStateV2,
        after: &PalwClaimStateV2,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        debug_assert_eq!(before.bond, after.bond, "a claim's bond is fixed at acceptance");
        let held = palw_claim_commitment_v1(self.params, before, now_daa).ok_or(PalwStateV2Error::Overflow("reserved_exposure"))?;
        let holds = palw_claim_commitment_v1(self.params, after, now_daa).ok_or(PalwStateV2Error::Overflow("reserved_exposure"))?;
        if held == holds {
            return Ok(());
        }
        let current = self.state.reserved_exposure.get(&after.bond).copied().unwrap_or(0);
        let next = current
            .checked_sub(held)
            .and_then(|rest| rest.checked_add(holds))
            .ok_or(PalwStateV2Error::Overflow("reserved_exposure staging"))?;
        self.write_exposure(after.bond, if next == 0 { None } else { Some(next) })?;
        Ok(())
    }

    /// **ADR-0152 S-SPEC §3.4 step 3: the licence set's R-core+ record**, staged on a copy of `claim`
    /// for [`Self::license_claim`] to write. `receipts` are the carried receipts with the mask each
    /// attests (a V2 receipt the full cut, a V3 receipt its own); a seat is read once, in the order
    /// carried, and a receipt from a bond that is not on the claim's panel is read as nothing.
    ///
    /// * `licence_door` — the door;
    /// * `basis_k` — Q-3's recount over the distinct `Valid` signers' masks
    ///   ([`palw_receipt_set_basis_k_v1`]; the cut is `palw_segment_count_v2(seats)`);
    /// * `served_mask` — bit `i` for panel seat `i` that carried a `Valid`;
    /// * `unserved_seen` — any `Unavailable`, `Incapable` or `Sampled` carried;
    /// * `escrow_released` — [`palw_rcore_release_due_v1`] on the record so far.
    ///
    /// Below `palw_rcore_plus` the claim is returned untouched: the record stays `Default`.
    pub(super) fn staged_licence_v1(
        &self,
        claim_id: Hash64,
        claim: &PalwClaimStateV2,
        door: PalwLicenceDoorV1,
        receipts: &[(&crate::palw_panel_v2::PalwSeatReceiptV2, Option<crate::palw_verification_v2::PalwSegmentMaskV2>)],
        now_daa: u64,
    ) -> PalwClaimStateV2 {
        let mut staged = claim.clone();
        if !self.params.rcore_plus_active_at(now_daa) {
            return staged;
        }
        let seats: Vec<PalwBondKeyV2> =
            self.state.panels.get(&claim_id).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
        let segments = crate::palw_verification_v2::palw_segment_count_v2(seats.len().min(u16::MAX as usize) as u16);
        let full = crate::palw_verification_v2::PalwSegmentMaskV2::full(segments);
        let mut seen: Vec<PalwBondKeyV2> = Vec::new();
        let mut masks = Vec::new();
        let mut served_mask = 0u32;
        let mut unserved_seen = false;
        for (receipt, mask) in receipts {
            let Some(index) = seats.iter().position(|seat| *seat == receipt.seat_bond) else { continue };
            if seen.contains(&receipt.seat_bond) {
                continue;
            }
            seen.push(receipt.seat_bond);
            match receipt.verdict {
                crate::palw_panel_v2::PalwReceiptVerdictV2::Valid => {
                    masks.push(mask.unwrap_or(full));
                    if index < 32 {
                        served_mask |= 1u32 << index;
                    }
                }
                // ADR-0152 Q-1 (F4): a `Sampled` answer counts in no quorum, coverage or upgrade — no
                // mask, no served bit — and, like the other non-`Valid` answers, latches
                // `unserved_seen` (C1), so a set it rides in never releases the escrow at licence.
                crate::palw_panel_v2::PalwReceiptVerdictV2::Unavailable { .. }
                | crate::palw_panel_v2::PalwReceiptVerdictV2::Incapable
                | crate::palw_panel_v2::PalwReceiptVerdictV2::Sampled => {
                    unserved_seen = true;
                }
            }
        }
        staged.rcore = PalwClaimRcoreV1 {
            licence_door: Some(door.into()),
            basis_k: palw_receipt_set_basis_k_v1(&masks, segments),
            escrow_released: false,
            served_mask,
            unserved_seen,
            // Set by the licence from the prices it computed (`license_rcore_v1`), kept otherwise.
            g_res_sompi: claim.rcore.g_res_sompi,
        };
        staged.rcore.escrow_released = palw_rcore_release_due_v1(self.params, &self.state, &claim_id, &staged);
        staged
    }

    /// ADR-0100 Decision 4: whether this claim's panel was drawn per shard. Answers `false`
    /// wherever the fence is dormant — no plan exists there — so the whole-object doors are
    /// byte-identical on every network that has not armed it.
    pub(super) fn claim_licenses_by_parts(&self, claim_id: &Hash64) -> bool {
        // RFC-0006: a claim drawn per layer shard licenses by its parts too (its record exists only past the fence).
        palw_tir_shard_fold_v1::claim_licenses_tir_parts_v1(&self.state, claim_id)
            || self.extras.shard_licensing.is_some_and(|params| {
                crate::palw_shard_licensing_v1::palw_claim_licenses_by_parts_v1(&self.state, claim_id, params.seats_per_shard)
                    .is_some()
            })
    }

    pub(super) fn slash_dissenting_seats(
        &mut self,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        receipts: &[PalwSeatVerdictV2],
        served_won: bool,
    ) -> Result<(), PalwStateV2Error> {
        let seat_cap = self.params.min_collateral_sompi();
        // **ADR-0124 Decision 3: a seat on duty loses what it RESERVED** — and nothing else: what is
        // reserved is what is slashable. ADR-0130: the amount is the one the duty row STORED when
        // the panel bound (the claim-priced stake, or the reward floor where it was in force), never
        // recomputed here. A panel bound below the fence holds no duty row and is charged as before.
        let duty_stake = self.state.panel_duties.get(claim_id).map(|row| row.seat_exposure);
        for receipt in receipts {
            // Only a seat that took a SIDE can have contradicted the quorum. `Incapable` is not a
            // side — it is the seat saying it had no standing to judge — so it is never charged.
            let took = match receipt.answer {
                PalwSeatAnswerV2::Served => true,
                // **ADR-0065 D4.** Past the fence `Withheld` is not a side either. A seat files it
                // when its material never arrived, and it cannot tell a producer that withheld from
                // a pull that was lost — so charging it for disagreeing with a panel that WAS fed
                // charges the honest seat for the network's failure. Measured on testnet-11: every
                // remote seat filed it on ~30% of claims while the producer's co-located seat filed
                // it on none, which is a property of the transport, not of any seat's honesty.
                PalwSeatAnswerV2::Withheld if self.unavailable_abstains => continue,
                PalwSeatAnswerV2::Withheld => false,
                PalwSeatAnswerV2::Incapable => continue,
                // ADR-0152 Q-1: an audit is not a side either — a seat that sampled took no position
                // a quorum could refute, so it is never dissent-slashed.
                PalwSeatAnswerV2::Sampled => continue,
            };
            if took != served_won {
                // **BC-SYBIL (mainnet audit 2026-09-11 deep fence): a seat's stake at risk is the
                // claim's own reserved exposure, not the flat `min_collateral` floor.** Below the
                // fence a dissenting seat lost at most `min_collateral` (0.004 MSK) however much the
                // claim was worth, so a Sybil that owned a quorum could corrupt a high-value verdict
                // for pocket change. Past the fence it risks up to `claim.reserved` — the same
                // `pwu × slash_value_per_pwu` the executor stands behind — clamped to its own
                // collateral by `slash_bond`. Safe against a registrant griefing seats with an
                // arbitrary slash value because the draw only seats bonds that DECLARED
                // `capable_classes` for this class (`palw_bond_may_judge_class_v2`), so a seat only
                // ever risks capital on a class it opted into; BASE-0 (forced-capable) is
                // chain-defined, so its `reserved` is bounded. Below the fence, unchanged.
                if let Some(stake) = duty_stake {
                    self.slash_bond(receipt.seat_bond, stake)?;
                } else if self.extras.audit_2026_09_11_deep_active {
                    self.slash_bond(receipt.seat_bond, claim.reserved)?;
                } else {
                    self.slash_seat(receipt.seat_bond, claim.reserved, seat_cap)?;
                }
            }
        }
        Ok(())
    }

    /// **P0-7: an assigned seat that answered nothing loses collateral.**
    ///
    /// `slash_dissenting_seats` charges a seat that answered the WRONG way. This charges the one
    /// that did not answer at all, which was free: a bond could take panel seats forever, never
    /// file, and pay nothing — so the exposure a seat is supposed to put behind its verdict was
    /// only ever at risk if it chose to speak.
    ///
    /// `answered` is the seat set the concluding object carried. Everything on the panel and not
    /// in it is a no-show. A seat with something to say is never here: `Valid` and `Unavailable`
    /// are both answers, and reporting withheld data is what the `Unavailable` verdict is for.
    ///
    /// It charges the same `reserved` a dissent costs — an unanswered seat and a refuted one both
    /// failed the same duty, and pricing silence below a lie would make silence the better play.
    /// **A seat is not charged for silence, because this chain cannot observe silence** (audit M2-7).
    ///
    /// This used to charge every seat absent from `answered`. Neither call site could supply a
    /// truthful `answered`:
    ///
    /// * the `ReceiptTimeout` arm passed `&[]` — no receipts at all — so it charged the WHOLE panel
    ///   whenever a quorum object failed to land, including when three of five seats had honestly
    ///   pleaded `Incapable` and made both quorums unreachable, which is precisely the situation
    ///   that verdict exists for and whose doc promises "it is never charged";
    /// * the licensing arms passed the verdicts the CARRIED OBJECT happened to hold, and that
    ///   object is one node's RAM at one two-second tick, not the set of seats that answered. Five
    ///   honest seats filing on time, with the collector holding three when its tick fires, charged
    ///   the other two — 2 of 5 seat-duties slashed on every licensed claim of a perfectly honest
    ///   network. A funded submitter could also simply omit two competitors, permanently, for free.
    ///
    /// Consensus cannot repair either: no acceptance rule can know what receipts existed off-chain.
    /// Seat accountability needs receipts that ride the chain independently of a concluding object
    /// (or a concluding object that must carry the whole panel); until one of those exists, the
    /// honest answer is to charge nobody. Bonds are still slashed for things the chain CAN prove —
    /// a producer that cannot show its material, a seat convicted in court.
    ///
    /// Kept as a named no-op rather than deleted so the call sites still say where the duty was
    /// meant to be enforced, and so re-introducing it requires deciding what proves silence.
    ///
    /// **What this leaves standing, said plainly** (re-audit R-7). Removing the charge was right,
    /// and the charge for being WRONG is intact and symmetric — `slash_dissenting_seats` takes the
    /// `reserved` from a `Withheld` seat when the quorum licenses, and from a `Valid` seat when the
    /// quorum defaults the producer. What is missing is the other side of the ledger: **nothing in
    /// this transition ever PAYS a `PalwPanelSeatV2`.** ADR-0033's retired V1 attester accounting pays the
    /// V1 credit lane's attesters, `palw_credit` is `None` on every shipped preset, and the block's
    /// escrowed carve is released to the worker, not to the panel. So answering carries a downside
    /// and no upside, and the seat that maximises its own outcome files nothing — after which the
    /// claim voids at `ReceiptTimeout` (`apply_palw_transition_v4`) and the whole loss lands on an
    /// honest producer.
    ///
    /// That is an economics gap, not a missing slash, and it is deliberately NOT repaired by
    /// inventing a payment rule inside an audit-remediation pass. The two candidate designs, so the
    /// decision is made rather than drifted into: (a) carve a fixed permille of the claim's released
    /// escrow to the seats that formed the concluding quorum; or (b) make silence ineffective
    /// instead of profitable by re-binding a fresh panel once on `ReceiptTimeout` before voiding —
    /// which costs one more `window_bind + window_receipt` in the lattice `validate_palw_v2` now
    /// bounds against the pruning horizon. `a_v2_panel_seat_is_never_paid` pins the current state so
    /// this cannot be half-closed silently.
    pub(super) fn slash_silent_seats(
        &mut self,
        _claim_id: &Hash64,
        _claim: &PalwClaimStateV2,
        _answered: &[PalwSeatVerdictV2],
    ) -> Result<(), PalwStateV2Error> {
        Ok(())
    }

    pub(super) fn finalize_claim(&mut self, id: Hash64, claim: &PalwClaimStateV2, final_daa: u64) -> Result<(), PalwStateV2Error> {
        if self.state.staged_claim_contract_v1(&id).and_then(|c| c.prefix_custody.as_ref()).is_some_and(|c| !c.qualified()) {
            // Refusal to opt into prospective custody buys no Final/activation credit and
            // creates no retroactive DA guilt. Pending accusations keep their existing route.
            if self.state.palw_accusation_pending_v1(&id) {
                return Ok(());
            }
            return self.void_claim(id, claim, final_daa, PalwVoidReasonV2::PanelUnavailable);
        }
        if !self.state.paid_consumer_service_fits_v1(&id, final_daa, final_daa) {
            return Err(PalwStateV2Error::ExecV2Refused("Final exceeds immutable paid consumer service".into()));
        }
        // **RFC-0008 once-only settlement** (dormant `palw_settlement_once_v1`): a claim's issuance-funded Final — its R/F budget
        // draw, every reward leg below, its buyback slice and its EXEC root split — settles once, whichever consumer reaches it. The
        // marker is written here, in this transition, before any of them moves value; a repeated Final of the same claim pays
        // nothing and changes nothing. `Dormant` below the fence: the historical path, byte for byte.
        //
        // **RFC08-2: under the combined liability predicate** (`palw_settlement_liability_fold_v1`): the draw is what the budget
        // can grant (the escrow within the claim's remaining R), its at-once part what this Final exports (the whole, capped at
        // the PESG export cap, where nothing vests; the buyback slice where it vests), the rest vesting at the panel liability's
        // expiry. A refusal closes the obligation at zero and the Final proceeds with its reward withheld — never named, never
        // minted — exactly as the existing caps leave an unbudgeted remainder.
        // Below the fence nothing here is computed or read (the historical path, byte for byte).
        let liability_withheld = self.params.settlement_once_active_at(final_daa) && {
            use super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1;
            use crate::palw_settlement_once_v1::{
                PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
            };
            let vests_at_final = self.params.rcore_plus_active_at(final_daa) || self.state.exec_v2.roots.contains_key(&id);
            let draw = self.issuance_draw_v1(&id, claim.escrowed_reward);
            let cap = self.bond_budget_export_cap_v1(&id, claim, final_daa);
            let (draw, paid_now, vest_until) = if vests_at_final {
                let slice = crate::palw_model_market_v1::palw_model_buyback_slice_v1(draw);
                let slice = if cap.is_some_and(|cap| slice > cap) { 0 } else { slice };
                (draw, slice, Some(crate::palw_panel_var_v1::palw_panel_liability_expiry_v1(final_daa, self.params.window_court())))
            } else {
                let paid = cap.map_or(draw, |cap| draw.min(cap));
                (paid, paid, None)
            };
            let once = self.settle_once_liable_v1(
                final_daa,
                PalwSettlementObligationV1::of(PalwSettlementLegV1::IssuanceFinal, id.as_bytes()),
                PalwSettlementConsumerV1::NativeFinal,
                draw,
                PalwSettlementDebitV1::Issuance { carve: claim.escrowed_reward },
                PalwLegLiabilityV1::V2Claim {
                    claim: id,
                    final_daa: Some(final_daa),
                    horizon: None,
                    draw,
                    paid_now,
                    exported_before: Some(0),
                    vest_until,
                },
            )?;
            match once {
                super::palw_settlement_once_fold_v1::PalwSettleOnceV1::AlreadySettled(_) => return Ok(()),
                super::palw_settlement_once_fold_v1::PalwSettleOnceV1::Refused(_) => true,
                _ => false,
            }
        };
        self.mark_paid_consumer_terminal_v1(id, final_daa)?;
        // **RFC-0018 Gate C (adopted 2026-10-15)**: a governed claim's `Final` completes the procedure only; its certification is
        // committed here (claim, producer and pool cut fixed now; the draw waits for later randomness). A no-op below the fence.
        super::palw_security_certification_fold_v1::commit_at_final_v1(self, &id, claim, final_daa)?;
        self.note_model_probe(claim, true, final_daa)?;
        // ADR-0152-adjacent (Activation Pool): (b)'s tracking — before the duty row leaves below.
        self.note_activation_probe_credits_v1(&id, claim)?;
        self.note_final_work(&id, claim, final_daa)?;
        // RFC-0004 (spec 17 §17.4.5): a Final claim of a governed line's head counts toward its trigger.
        self.note_improvement_usage_at_final_v1(claim, final_daa)?;
        // RFC-0004 A6 (MIP-17): an evaluation claim's Final records its score and pays its fee.
        self.note_improvement_eval_final_v1(&id, claim, final_daa)?;
        self.release_for_claim(claim, final_daa)?;
        // The weight divergence between the lanes (ADR-0044): an attempt's Final IS its block's
        // certified work; a free-prompt Final only LICENSES — its weight arrives per spent
        // quantum, at the receipt block that spends it.
        if matches!(claim.source, PalwClaimSourceV2::Attempt) {
            // **ADR-0069 Decision 7, at the site that feeds `safe(C)`.** A class holding no granted
            // share cannot be prosecuted, so its work buys no fork-choice weight — the block still
            // advanced DAA, was still paid its budgeted subsidy, and its claim still ran this whole
            // lattice to get here. Priced through the one helper, so the re-derivation in
            // `assert_internal_consistency_v3` and `retire_claim` cannot drift from it.
            //
            // **ADR-0145 rides the same helper.** Past `Params::palw_canonical_work` the
            // contribution is the work the chain derived from the class's graph rather than the
            // step-leaf count its registrant wrote — and it is keyed on the CLAIM's own
            // `accepted_daa`, so the two re-derivations above return the same number at whatever
            // height they run.
            let canonical = self.canonical_claim_weight(claim);
            let contribution =
                palw_claim_safe_contribution_v3(&self.state.class_shares, claim, self.uncertified_weightless, canonical);
            // ADR-0160 F-W (D-3): a new-rule claim's Final weight under the C7 ceiling — the identity on
            // every attributable class and every old-rule claim.
            let contribution = crate::palw_weight_cap_v1::palw_weight_final_safe_v1(self.params, claim, contribution);
            // **Lane BUDGET (ADR-0176 D3): the Final weight under the budget** — the grant, never beyond the claim's reserved F (the same
            // number every re-derivation reads back). Unchanged for an unbudgeted claim.
            let contribution = self.bond_budget_final_weight_v1(&id, contribution)?;
            // **RFC-0017 (REQ-ELIG-2): a probation claim's Final is job completion, not consensus credit** — its F was reserved 0, so
            // the grant above is already 0. U-2 (2026-10-15): its pin outlives the Final (retired when the claim leaves the state or
            // is voided), and Gate C below certifies before any credit. Unchanged for every other claim and below the fence.
            let contribution = super::palw_model_eligibility_fold_v1::eligibility_final_weight_v1(self, &id, contribution);
            // **RFC-0018 Gate C's final-weight site** (`palw_independently_work_certified_v1`): an uncertified claim's Final adds no
            // weight. The identity below its fence (keyed on the claim's acceptance DAA) and on every shipped network.
            let contribution = crate::palw_rfc0018_v1::palw_gate_c_weight_v1(self.params, &self.state, &id, claim, contribution);
            self.state.safe_weight =
                self.state.safe_weight.checked_add(contribution).ok_or(PalwStateV2Error::Overflow("safe_weight"))?;
        } else {
            // RFC-0017: a free-prompt claim's receipt spends read the budget's F = 0 (U-2: its pin outlives the Final).
            let _ = super::palw_model_eligibility_fold_v1::eligibility_final_weight_v1(self, &id, 0);
        }
        // ADR-0042 Decision 10: `Final` is where escrow becomes payable, so it is where the
        // release is recorded. Nothing is minted here — this only names an amount and a payee for
        // the next block's coinbase, which is the block that can actually carry an output.
        //
        // **ADR-0152 V-2 (R-core+, testnet-12): past `palw_rcore_plus` the same amounts VEST.**
        // Every sompi this branch would write into `pending_payouts` — the producer's share, each
        // credited seat's `per_seat`, the reserve, or the whole reward where no duty row exists —
        // is named instead in one `PalwVestingRowV1`, written below once the liability record is
        // (so the row copies its attribution fields), and step 3d moves it into the queue only once
        // V-4 matures it. The amounts are exactly the ones below (phase2-plan I-11), so T03's
        // identity closes. NOT vested (V-2): the ADR-0091 buyback slice, which still executes here
        // and is recorded as the row's `buyback_bound`; the work-price remainder, which is never
        // named at all; FP receipt-spend payouts and execution-lane rights, which never pass here.
        // Below the fence (every other network, and a t12 twin with the fence off) nothing moves.
        let vests = self.params.rcore_plus_active_at(final_daa) || self.state.exec_v2.roots.contains_key(&id);
        let mut vested: Option<PalwVestedRewardV1> = None;
        // RFC-0008 v2: whether the claim's work session was settled (and its slice executors' legs read out below). It stays `false`
        // for a claim with no root — every claim below the fence — and then a nil settlement below is a no-op.
        let mut exec_settled = false;
        if claim.escrowed_reward > 0 {
            let exec_legs: Vec<(PalwBondKeyV2, u64)>;
            // The bond must still be there to name a payee. It is, unless the registry dropped a
            // bond that still had live claims — an invariant break, not a payout policy — so this
            // errors rather than skipping: a silently unpaid producer is worse than a refused
            // block, and the refusal names the claim.
            let payout_payload = self.state.bonds.get(&claim.bond).ok_or(PalwStateV2Error::MissingBond(claim.bond))?.payout_payload;
            // ADR-0091 Decisions 1–3: five percent of the escrow buys from the pair of the line
            // the claim ran, where that pair exists and is open; the miner is named the rest.
            // Where no pair takes it the whole escrow is the miner's — nothing is burned for a
            // model. Either way the number the accepting block withheld is the number that leaves
            // here: as a payout, as reserve, never as a mint (Decision 8).
            // **ADR-0124 Decision 6: the price of work.** Past the fence a model class's claim is
            // paid the fraction of its escrow that its class's canonical inference is of the
            // heaviest weight-bearing model class's; the rest is never named — never minted, as a
            // voided escrow is. Below the fence, and for the liveness floor, the escrow whole. The
            // buyback is a slice of what is paid, so a lighter class's pair buys proportionally.
            // **ADR-0132 Upgrade C: the price of compute at one rate.** A claim that snapshotted its
            // economics when it was accepted (`Params::palw_economic_payout` was active there, and
            // its class is a model the registry holds work for) is paid `min(escrow, attempted × rate)`
            // — the class draws, the network draws and one draw's job it ran in expectation, at the
            // rate then — and its panel the share the snapshot derived from the verification compute
            // against the producer's. The rest is never named, as under Decision 6. A claim without
            // a snapshot (the floor, a claim accepted below the fence) is priced exactly as before.
            let economics = self.state.claim_economics.get(&id).copied();
            let escrow = match economics {
                Some(snapshot) => snapshot.priced_reward(claim.escrowed_reward),
                None if self.extras.work_priced_reward_active => self.work_priced_escrow(claim),
                None => claim.escrowed_reward,
            };
            // **Lane BUDGET (ADR-0176 D3, design §2.6): the reward under the budget**, before the buyback and the panel split, so every
            // leg derives from it; the rest is never named — never minted. Unchanged for an unbudgeted claim.
            // RFC08-2: a leg the combined liability predicate refused draws nothing and names nothing.
            let escrow = if liability_withheld { 0 } else { escrow };
            let escrow = self.bond_budget_final_reward_v1(&id, escrow)?;
            // **RFC07 R7-ECON (`palw_model_verification_gate_v1`, round 3): the allocation payout reads its model-epoch's
            // verification** — paid whole when attested (or no Final claim existed to sample), withheld when unattested, deferred while
            // undecided; before every leg below. Unchanged below the fence and for every claim the allocation did not reserve for.
            let escrow = palw_econ_contracts_fold_v1::gate_final_reward_v1(self, &id, claim, escrow, final_daa)?;
            // **RFC-0018 Gate C's coinbase site**: past the fence the issuance-funded (subsidy-carved) reward leg represents consensus
            // credit — it is HELD in the claim's certification row and named only at `SecurityCertified` (burned otherwise). The
            // identity below its fence and on every shipped network.
            let escrow = super::palw_security_certification_fold_v1::hold_issuance_reward_at_final_v1(self, &id, claim, escrow)?;
            // **Lane BUDGET, PESG §6: the external export cap.** What leaves the chain's books at Final — a leg written straight into the
            // payout queue (no vesting), or a buyback slice in a market reserve a holder can sell out of — is at most 0.51 of the
            // collateral the claim's liability still holds; a vesting leg is not an export (it moves once the lock has ended). The rest
            // is never named (never minted). Unchanged below the fence.
            let export_cap = self.bond_budget_export_cap_v1(&id, claim, final_daa);
            let escrow = match export_cap {
                Some(cap) if !vests => escrow.min(cap),
                _ => escrow,
            };
            let slice = if export_cap.is_some_and(|cap| crate::palw_model_market_v1::palw_model_buyback_slice_v1(escrow) > cap) {
                0
            } else {
                self.model_buyback_at_final(&id, claim, escrow)?
            };
            let reward = escrow - slice;
            // **ADR-0124 Decisions 1 and 2: the panel's share.** A claim whose panel holds a duty
            // row — bound past `Params::palw_panel_economy`, which is the only way a row exists —
            // pays every credited seat one fixed share of the pool and the producer the rest of the
            // reward; what no seat was credited for, and the division's dust, is the reserve's and
            // never the producer's. A claim bound below the fence names its producer the whole
            // reward, as before.
            match self.state.panel_duties.get(&id).map(|row| row.seats.clone()) {
                Some(duties) => {
                    let credited: Vec<PalwBondKeyV2> = duties.iter().filter(|(_, at)| **at != 0).map(|(seat, _)| *seat).collect();
                    let pool_permille = economics
                        .map(|snapshot| snapshot.panel_share_permille)
                        .unwrap_or(crate::palw_panel_economy_v1::PALW_PANEL_POOL_PERMILLE_V1 as u16);
                    let split = crate::palw_panel_economy_v1::palw_panel_split_permille_v1(
                        reward,
                        pool_permille,
                        duties.len(),
                        credited.len(),
                    );
                    // **RFC-0006, decision 6: a claim drawn per layer shard pays its credited seats by the work they
                    // vouched for** (`final_legs_v1`: the pool divided by the drawn seats' shares, each at least the floor,
                    // the producer the exact rest, the reserve what no credited seat is owed). Every other claim: one
                    // fixed share a credited seat, as it always was.
                    let (producer_amount, seat_legs, reserve_amount): (u64, Vec<(PalwBondKeyV2, u64)>, u64) =
                        match palw_tir_shard_fold_v1::final_legs_v1(&self.state, &id, reward, pool_permille, &credited) {
                            Some(legs) => legs,
                            None => (
                                split.producer,
                                if split.per_seat > 0 {
                                    credited.iter().map(|seat| (*seat, split.per_seat)).collect()
                                } else {
                                    Vec::new()
                                },
                                split.reserve,
                            ),
                        };
                    // RFC-0008 v2: the root executor's leg is split by work share with the slice executors, once (a no-op, and
                    // `(producer_amount, [])`, for a claim with no work session).
                    let (producer_amount, legs) = palw_exec_v2_fold::settle_at_final_v2(self, &id, producer_amount, final_daa)?;
                    exec_legs = legs;
                    exec_settled = true;
                    if vests {
                        // V-2: the same legs, named in the row — each credited seat's payload read
                        // from its bond HERE, at Final (I-5: a moved leg never re-reads the bond).
                        let mut seats = Vec::new();
                        for (seat, amount) in &seat_legs {
                            let seat_payload = self.state.bonds.get(seat).ok_or(PalwStateV2Error::MissingBond(*seat))?.payout_payload;
                            seats.push((*seat, PalwPayoutV2 { payload: seat_payload, amount: *amount }));
                        }
                        vested = Some(PalwVestedRewardV1 {
                            producer: PalwPayoutV2 { payload: payout_payload, amount: producer_amount },
                            seats,
                            reserve: reserve_amount,
                            buyback: slice,
                        });
                    } else {
                        let finished = !self
                            .state
                            .exec_v2_rewards
                            .get(&id)
                            .is_some_and(|row| row.legs.keys().any(|(bond, _)| *bond == claim.bond));
                        self.pay_native_recipient_v1(id, claim.bond, 0, final_daa, producer_amount, id, finished)?;
                        let mut recipient_amounts: BTreeMap<PalwBondKeyV2, u64> = BTreeMap::new();
                        for (seat, amount) in &seat_legs {
                            let total = recipient_amounts.entry(*seat).or_default();
                            *total = total.checked_add(*amount).ok_or(PalwStateV2Error::Overflow("Panel recipient income"))?;
                        }
                        for (seat, amount) in recipient_amounts {
                            self.pay_native_recipient_v1(
                                id,
                                seat,
                                1,
                                final_daa,
                                amount,
                                palw_panel_payout_key_v1(&self.state.bonds[&seat].payout_payload),
                                true,
                            )?;
                        }
                        if producer_amount > 0 {
                            self.write_payout(id, Some(PalwPayoutV2 { payload: payout_payload, amount: producer_amount }))?;
                        }
                        for (seat, amount) in &seat_legs {
                            let seat_payload = self.state.bonds.get(seat).ok_or(PalwStateV2Error::MissingBond(*seat))?.payout_payload;
                            self.add_panel_payout(seat_payload, *amount)?;
                        }
                        let reserve = self
                            .state
                            .panel_reserve_sompi
                            .checked_add(reserve_amount)
                            .ok_or(PalwStateV2Error::Overflow("panel reserve"))?;
                        self.write_panel_reserve(reserve)?;
                    }
                }
                None if vests => {
                    let (reward, legs) = palw_exec_v2_fold::settle_at_final_v2(self, &id, reward, final_daa)?;
                    exec_legs = legs;
                    exec_settled = true;
                    vested = Some(PalwVestedRewardV1 {
                        producer: PalwPayoutV2 { payload: payout_payload, amount: reward },
                        seats: Vec::new(),
                        reserve: 0,
                        buyback: slice,
                    });
                }
                None => {
                    let (reward, legs) = palw_exec_v2_fold::settle_at_final_v2(self, &id, reward, final_daa)?;
                    exec_legs = legs;
                    exec_settled = true;
                    let finished =
                        !self.state.exec_v2_rewards.get(&id).is_some_and(|row| row.legs.keys().any(|(bond, _)| *bond == claim.bond));
                    self.pay_native_recipient_v1(id, claim.bond, 0, final_daa, reward, id, finished)?;
                    if reward > 0 {
                        self.write_payout(id, Some(PalwPayoutV2 { payload: payout_payload, amount: reward }))?;
                    }
                }
            }
            // Additional EXEC legs retain their distinct role until the root's dependency horizon and native second clock close.
            for (bond, amount) in &exec_legs {
                palw_exec_v2_rewards_v1::add(
                    self,
                    id,
                    *bond,
                    crate::palw_work_slice_v2::PalwExecV2RewardFundV1::NativeSlice,
                    *amount,
                )?;
            }
        }
        // RFC-0008 v2: a claim with a session whose reward leg was nil (no escrow) still settles — once, owing nothing — so the
        // root is marked and the executors' exposure returns.
        if !exec_settled {
            palw_exec_v2_fold::settle_at_final_v2(self, &id, 0, final_daa)?;
        }
        // Lane BUDGET: an attempt's Final is terminal for its budget (a free-prompt claim's spends come after its Final).
        if matches!(claim.source, PalwClaimSourceV2::Attempt) {
            self.bond_budget_close_v1(&id, final_daa)?;
        }
        if let Some(duties) = self.state.panel_duties.get(&id).map(|row| row.seats.clone()) {
            for (seat, _) in duties {
                let future = vested
                    .as_ref()
                    .is_some_and(|reward| reward.seats.iter().any(|(payee, payout)| *payee == seat && payout.amount > 0));
                if !future {
                    self.cancel_native_recipient_v1(id, seat, 1)?;
                }
            }
        }
        // ADR-0132 Upgrade C: the snapshot leaves with the claim — it was read above, once.
        self.write_claim_economics(id, None)?;
        // ADR-0124 Decision 3: the seats leave duty with their exposure — before the phase write
        // below drops the row this reads.
        self.release_seat_duties(&id)?;
        self.persist_panel_liability(id, claim, final_daa, None)?;
        // ADR-0152 V-1/V-2: the row, after the liability record so it can copy that record's
        // attribution fields in the same funnel (N8).
        if let Some(vested) = vested {
            self.write_vesting_row_at_final(id, claim, final_daa, vested)?;
        }
        palw_exec_v2_rewards_v1::finish(self, id)?;
        // ADR-0125: past the lane's fence a finalized attempt is a credit in its span's schedule.
        if let Some(lane) = self.extras.round_lane {
            self.record_round_final(id, claim, final_daa, lane.schedule_span_daa)?;
        }
        let mut finalized = claim.clone();
        finalized.phase = PalwClaimPhaseV2::Final { final_daa };
        self.write_claim(id, Some(finalized.clone()))?;
        // ADR-0152 DA-5 (M3): a session still open at `Final` (a non-seat one — a seat session holds
        // the claim's deadline) keeps the row and the locks behind its own end.
        let open: Vec<u64> = self.state.da_sessions_of(&id).map(|(_, session)| session.deadline_daa).collect();
        for deadline in open {
            let until = deadline.saturating_add(self.params.window_challenge_at(final_daa));
            self.da_rekey_v1(id, until, final_daa)?;
        }
        // Below `palw_audit_2026_09_23` the second clock ticks here, as it always did — counted on
        // every network (a fact about the chain) and read by no rule there. Past the fence it
        // ticks at the licence instead (`license_claim`): a `Final` is reached by the DAA sweep
        // alone and must not settle anything on history nobody signed.
        // RFC-0018 Gate C's settled-anchor site (the identity below its fence).
        if matches!(claim.source, PalwClaimSourceV2::Attempt)
            && !self.extras.audit_2026_09_23_active
            && crate::palw_rfc0018_v1::palw_gate_c_settles_anchor_v1(self.params, claim)
        {
            self.settle_anchor(final_daa, false)?;
        }
        self.disarm_deadline(id);
        self.arm_retirement(id, &finalized)?;
        Ok(())
    }

    pub(super) fn void_claim(
        &mut self,
        id: Hash64,
        claim: &PalwClaimStateV2,
        voided_daa: u64,
        reason: PalwVoidReasonV2,
    ) -> Result<(), PalwStateV2Error> {
        self.mark_paid_consumer_terminal_v1(id, voided_daa)?;
        // **ADR-0164 F-K (K3, K4): a void for want of a quorum receipt or of a panel is a claim reaching its deadline and failing** —
        // counted once, here, where every such void is written. The reasons that carry a conviction are the conviction funnel's (K2).
        match reason {
            PalwVoidReasonV2::ReceiptTimeout | PalwVoidReasonV2::UnavailableQuorum | PalwVoidReasonV2::NotReplayBacked => {
                // A receipt-deadline expiry is a property of the network's verification supply (lane PL: it charges no producer), so it
                // moves the NETWORK row only — never a bond's.
                self.note_breaker_v1(voided_daa, crate::palw_capacity_s567_v1::PalwBreakerEventV1::ReceiptVoid)?;
            }
            // B1 counts only outcomes the producer is answerable for: a data-availability default.
            PalwVoidReasonV2::ProducerWithholding => self.note_bond_breaker_v1(voided_daa, claim.bond, Some(true))?,
            PalwVoidReasonV2::BindTimeout | PalwVoidReasonV2::NoCapablePanel => {
                self.note_breaker_v1(voided_daa, crate::palw_capacity_s567_v1::PalwBreakerEventV1::BindVoid)?
            }
            _ => {}
        }
        // Lane BUDGET (ADR-0176 D4): a void is terminal for the claim's budget — and returns nothing to the window before `d + W`.
        self.bond_budget_close_v1(&id, voided_daa)?;
        // **ADR-0152 §4-ter C3: a void by any route closes every court session on the claim
        // NEUTRALLY**, past `palw_offence_attribution`: the session is removed (its challenger's
        // reservation released by `write_court`), nobody is slashed and no court time is charged —
        // the dispute it held is moot, and neither the decoy nor a seat's second session is the
        // loser of a court that never ended on its merits. The last session to close would have
        // re-armed the claim's `Final`; a voided claim has none to re-arm. Below the fence a void
        // leaves them to their own sweep, as it always did.
        if self.extras.offence_attribution_active {
            let open: Vec<Hash64> = self.state.court_sessions.iter().filter(|(_, s)| s.claim == id).map(|(k, _)| *k).collect();
            for session in open {
                self.write_court(session, None)?;
            }
        }
        // **ADR-0152 DA-6 (M3): a void ends every open DA session on the claim, exposure returned**;
        // a void the PRODUCER is charged for as a fraud refunds the refuted exposure held too:
        // `CourtFraud` (a proven fraud), `ProducerWithholding` (past `palw_rcore_plus` only a DA default
        // writes it) and `CourtDefault` (M3 review, item 5). A court default is no proof against the
        // signers (kind 3 refuses it by name), but it IS the producer's default on its own defence,
        // charged exactly as `CourtFraud` — and DA-6's promise is that a filer whose accusations the
        // producer answered on the garbage path, before defaulting in the court they led to, nets at
        // least zero. Burning that exposure at retirement would charge the honest filer for the
        // producer's silence.
        if self.params.rcore_plus_active_at(voided_daa) {
            // ADR-0160 AG-2: the forfeiture's void is charged to the producer as a conviction is, so a
            // filer's refuted exposure on the claim is refunded with it (DA-6).
            let convicted = matches!(
                reason,
                PalwVoidReasonV2::CourtFraud
                    | PalwVoidReasonV2::ProducerWithholding
                    | PalwVoidReasonV2::CourtDefault
                    | PalwVoidReasonV2::CourtHeldVerdict
                    | PalwVoidReasonV2::AggregateForfeit
            );
            self.da_release_all_v1(id, convicted, voided_daa)?;
        }
        let mut voided = claim.clone();
        voided.phase = PalwClaimPhaseV2::Voided { voided_daa, reason };
        if matches!(
            reason,
            PalwVoidReasonV2::CourtFraud
                | PalwVoidReasonV2::CourtDefault
                | PalwVoidReasonV2::ProducerWithholding
                | PalwVoidReasonV2::CourtHeldVerdict
        ) {
            self.note_model_probe(claim, false, voided_daa)?;
        }
        // ADR-0132 Upgrade C: a voided claim is paid nothing, so its snapshot leaves with it.
        self.write_claim_economics(id, None)?;
        // ADR-0088 Decision 4: a voided claim is subtracted where it was counted.
        self.uncount_claim_usage(&id, claim)?;
        // ADR-0124 Decision 3: every seat on duty leaves it with its exposure, whatever voided the
        // claim — before the phase write drops the row this reads. A seat's fault is the court's
        // and the quorum's business, charged where it is proven, never here.
        self.cancel_panel_recipients_v1(id)?;
        self.cancel_native_recipient_v1(id, claim.bond, 0)?;
        self.release_seat_duties(&id)?;
        // RFC-0008 v2: a work session on the claim goes void with it and its extra executors' exposure returns (a no-op for a claim
        // with no root, which is every claim below the fence).
        palw_exec_v2_fold::on_claim_voided_v2(self, &id, voided_daa)?;
        self.persist_panel_liability(id, claim, voided_daa, Some((voided_daa, reason)))?;
        // Audit C5, free-prompt half: an abandoned commitment holds its reservation for the
        // configured span instead of releasing it here, so a redraw costs collateral rather than
        // a transaction fee. The hold is a DELAY, never a confiscation — `release_abandon_hold`
        // gives every sompi back when the span elapses, and the claim is terminal throughout, so
        // it weighs nothing and can never resume.
        //
        // Immature weight is released either way and immediately: a voided claim contributes no
        // live weight the instant it is voided, which is W5, and the hold is about admission
        // headroom alone.
        //
        // **ADR-0160 E-4: the obligation survives the void.** An attempt claim accepted past
        // `palw_capacity_escrow_at_licence` and voided for a reason nobody was convicted under
        // (`palw_void_reason_keeps_obligation_v1`) takes the same road: its commitment stays on the
        // bond for `h_obl = window_receipt` past the void, so a withheld bind, an abandoned panel or a
        // starved licence never frees the bond's capacity at once (the withdrawal principle, T1–T8).
        let obligation_release = crate::palw_escrow_funding_v2::palw_claim_obligation_release_at_v1(&voided, self.params);
        if palw_claim_is_on_abandon_hold_v2(&voided, self.params, voided_daa) || obligation_release.is_some() {
            // ADR-0160 F-W: a new-rule claim's weight leaves through `write_claim` below.
            // RFC-0018 Gate C's provisional site: exactly what `reserve_for_claim` added (the identity below its fence).
            if !crate::palw_weight_cap_v1::palw_weight_cap_applies_v1(self.params, claim) {
                self.state.bounded_immature = self
                    .state
                    .bounded_immature
                    .checked_sub(crate::palw_rfc0018_v1::palw_claim_provisional_weight_v1(self.params, claim))
                    .ok_or(PalwStateV2Error::Overflow("bounded_immature underflow"))?;
            }
            self.write_claim(id, Some(voided))?;
            // The deadline is re-armed rather than disarmed: the sweep's terminal arm releases
            // the hold when it fires. Deriving it from the record (`voided_daa + hold`) is what
            // keeps the index rebuildable, which `assert_deadline_consistency` checks.
            self.disarm_deadline(id);
            let release_at = match obligation_release {
                Some(release_at) => release_at,
                None => voided_daa.checked_add(self.params.fp_abandon_hold_daa).ok_or(PalwStateV2Error::Overflow("abandon hold"))?,
            };
            self.arm_deadline(release_at, id)?;
            return Ok(());
        }
        self.release_for_claim(claim, voided_daa)?;
        self.write_claim(id, Some(voided.clone()))?;
        self.disarm_deadline(id);
        self.arm_retirement(id, &voided)?;
        Ok(())
    }

    /// Arm a terminal claim's retirement, if this network retires claims at all.
    ///
    /// A no-op when `claim_retirement_daa` is 0 — which is every network before the param — so a
    /// state that never retires is byte-for-byte the state it was.
    pub(super) fn arm_retirement(&mut self, id: Hash64, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        if self.params.claim_retirement_daa == 0 {
            return Ok(());
        }
        let terminal_daa = terminal_daa_of(claim).expect("arm_retirement is called on a terminal claim");
        let ordinary =
            terminal_daa.checked_add(self.params.claim_retirement_daa).ok_or(PalwStateV2Error::Overflow("claim retirement"))?;
        let at = palw_exec_v2_rewards_v1::retirement(&self.state, self.params, &id, Some(ordinary), claim)?
            .expect("retirement is configured");
        self.arm_deadline(at, id)?;
        Ok(())
    }

    /// **Remove a terminal claim and everything keyed to it** (launch blockers §8, third bullet).
    ///
    /// The panel goes with it: `write_panel` had no `None` caller anywhere, so a bound claim's
    /// panel outlived the claim by exactly forever and grew at the same one-per-block rate.
    pub(super) fn retire_claim(&mut self, id: Hash64, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        self.forget_paid_consumer_v1(id)?;
        // A `Final` attempt claim folded its `pwu` into `safe_weight` when it finalized. The
        // running total does not move here — the claim's contribution is simply carried by a
        // different name, so the consistency identity stays an equality rather than degrading to
        // "at least".
        //
        // **ADR-0069 Decision 7 is deliberately NOT applied here, and the reason is the one
        // failure this whole remediation must not cause.** The obvious symmetry — price the
        // retirement through `palw_claim_safe_contribution_v2` too — asks the share table the same
        // question at a LATER chain point, and the answer can have changed in between: the
        // activation edge turns a `Registered` class (no row at all, so weightless) into an
        // `Active` one holding permille, and reclamation takes a walked-away class's row away
        // again. Either move makes `retired_safe_weight` carry a number `safe_weight` never
        // received, and the difference is permanent — the claim is gone, so nothing later can
        // reconcile it. The drift accumulates over the chain's whole life until every
        // pruning-point import is refused for a state that obeyed the rule exactly.
        //
        // So the ATTEMPT arm stays `claim.pwu`, byte-identical to the pre-Decision-7 fold in both
        // fence positions, and `retired_safe_weight` means what its name says with one word added:
        // the MOST safe weight the claims this state no longer holds could have carried. Armed,
        // that makes the consistency identity an upper bound rather than an equality — stated in
        // full at [`PalwChainStateV2::assert_internal_consistency_v2`], which is where the two meet.
        //
        // **The FREE-PROMPT arm is a different question with an exact answer, and leaving it out
        // was a node that refuses its own tip.** A free-prompt claim's spent quanta really do move
        // `safe_weight` (`apply_receipt_spend` adds `pwu / quanta` per spend), and that lane is not
        // priced by Decision 7 in EITHER direction — not at the spend, not in the re-derivation —
        // so the amount `safe_weight` received is a pure function of the record: `per_quantum ×
        // |spent|`, the same expression `assert_internal_consistency_v2` uses for a live one. There
        // is no share table to ask and therefore no later chain point to disagree with, which is
        // exactly what makes the attempt arm hard and this one trivial.
        //
        // Before this, a retiring free-prompt claim with any spent quantum dropped that weight out
        // of the re-derivation and left it in `safe_weight` forever. Nothing on the live fold
        // notices — the fold does not run the check — but `PalwStateCarriageV2::into_state_v2` does,
        // so the node restarted, `load_tip` refused its own durable tip with `CarriageInconsistent`,
        // and every peer importing that pruning-point snapshot was refused with it. Both fence
        // positions failed it: dormant the equality, armed the `<=` bound, because the stranded
        // weight pushes `safe_weight` ABOVE the ceiling rather than below it.
        //
        // **Which is also why arming this fence is a repair only for retirements that have not
        // happened yet.** Once the strand exists there is nothing left to re-derive it from, so no
        // fence position accepts the state again and crossing the activation height changes
        // neither scalar — measured in all three positions by
        // `arming_the_fence_does_not_repair_a_chain_that_already_stranded_weight`. That is why
        // `Params::validate_palw_v2` refuses an activation above genesis: a chain that has already
        // stranded weight is repaired by a re-mint or not at all, and an operator must not be able
        // to reach the wedged state by scheduling the rule that was supposed to prevent it.
        //
        // It is fenced anyway, because `retired_safe_weight` is hashed into `state_root`
        // ([`PalwChainStateV2::state_root`]) and a block that retires a spent free-prompt claim
        // therefore roots differently with the repair than without. Dormant is the broken
        // arithmetic, unchanged, because dormant must be the chain that is already running. The
        // exposure that leaves is pinned by
        // `known_limitation_dormant_a_spent_free_prompt_retirement_strands_its_weight`.
        //
        // **The 2026-09-02 audit found the same defect independently, and its reachability
        // measurement is recorded here because it is what sizes the dormant exposure below.**
        // `assert_internal_consistency` recomputes a `Final` free-prompt claim's contribution as
        // `per_quantum × |spent|`, so an attempt-only fold subtracts that from the recomputation
        // and adds it back to nothing. The check runs inside `into_state`, which `load_tip` calls
        // on every virtual resolution and whose caller panics — so the state is written by every
        // node and then kills every node, on the next resolution and on every restart. The shipped
        // ruleset retires at 3,000 DAA and the receipt use window is 600, so a licensed claim with
        // one spent quantum reaches it in the ordinary course. That audit's remedy was to fold both
        // sources UNFENCED; this fold keeps the fence for the state-root reason argued above, so
        // the defect is closed past the activation and stays open below it, which is exactly what
        // the known-limitation test named just above pins as an executable fact.
        //
        // **ADR-0145 is the one thing the attempt arm DOES have to follow, and for the opposite
        // reason to Decision 7's.** The share table can move between a claim's `Final` and its
        // retirement, which is why that question is not asked twice. The derived basis cannot: it
        // is keyed on the claim's own `accepted_daa` and reads a lifecycle row frozen when the
        // registry opened the class, so `canonical_claim_weight` returns at this later chain point
        // exactly what it returned at `finalize_claim`. Leaving the attempt arm on `claim.pwu` past
        // the fence would book step leaves against a total that received MAC-equivalents, and the
        // difference would be permanent in exactly the way this comment warns about.
        let retiring: Option<u128> = match (&claim.phase, &claim.source) {
            // ADR-0160 F-W (D-3): under the C7 ceiling where the claim is new-rule — the same pure
            // function of the record `finalize_claim` priced it with, so nothing strands.
            (PalwClaimPhaseV2::Final { .. }, PalwClaimSourceV2::Attempt) => {
                Some(crate::palw_weight_cap_v1::palw_weight_final_safe_v1(
                    self.params,
                    claim,
                    self.canonical_claim_weight(claim).unwrap_or(claim.pwu as u128),
                ))
            }
            (PalwClaimPhaseV2::Final { .. }, PalwClaimSourceV2::FreePrompt { quanta, spent }) if self.uncertified_weightless => {
                // `quanta == 0` is unrepresentable (`ZeroQuanta` at creation, and the consistency
                // check refuses it in every phase), so the `unwrap_or(0)` is a division guard and
                // not a policy — a claim that reached here with no quanta carried no spend either.
                // ADR-0148: the same expression the spend added — one function, so the two eras
                // retire exactly what they weighed.
                let per_quantum = palw_fp_spend_weight_v1(&self.state, claim, *quanta, &self.fp_pricing());
                Some(per_quantum.checked_mul(spent.len() as u128).ok_or(PalwStateV2Error::Overflow("retired free-prompt weight"))?)
            }
            _ => None,
        };
        // Lane BUDGET: a budgeted claim retires the weight its Final (or its spends) was granted — the number `safe_weight` received.
        // RFC-0018 §7.5a: a retiring frontier-eligible Final raises the Gate C frontier floor (no-op below the fence).
        super::palw_security_certification_fold_v1::note_retirement_frontier_floor_v1(self, &id, claim)?;
        let retiring = retiring.map(|amount| self.state.bond_budget_weight_or(&id, amount));
        // RFC-0018 Gate C: retire exactly what `safe_weight` received — nothing where Gate C withheld it (identity below its fence).
        let retiring = retiring.map(|amount| crate::palw_rfc0018_v1::palw_gate_c_weight_v1(self.params, &self.state, &id, claim, amount));
        if let Some(amount) = retiring {
            let old = self.state.retired_safe_weight;
            let new = old.checked_add(amount).ok_or(PalwStateV2Error::Overflow("retired_safe_weight"))?;
            self.state.retired_safe_weight = new;
            self.entries.push(PalwDeltaEntryV2::RetiredWeight { old, new });
        }
        // **Everything keyed to the claim goes with it, sessions included.**
        //
        // A court session whose claim has been retired is a backstop armed against nothing: when
        // it fires, `sweep_court_deadlines` looks the claim up, does not find it, and returns
        // `MissingClaim` — which fails the whole transition. That error is a pure function of
        // (parent state, daa score), so every node computes it identically and disqualifies the
        // same block, and then the next one, forever, with no object that can clear it. A claim
        // can reach retirement with a session still open because a VOID does not require the
        // court to have closed, and `CLAIM_RETIREMENT == WINDOW_COURT` on the shipped ruleset, so
        // the two land together rather than needing a DAA jump.
        let orphans: Vec<Hash64> =
            self.state.court_sessions.iter().filter(|(_, session)| session.claim == id).map(|(session_id, _)| *session_id).collect();
        for session_id in orphans {
            self.write_court(session_id, None)?;
        }
        // ADR-0152 DA-6 (M3): the claim's DA record retires with it, burning unrefunded exposure.
        self.da_retire_v1(id)?;
        // Lane LG14-A: and its dispute record, burning every held reservation deposit no conviction refunded.
        self.legacy_dispute_retire_v1(id)?;
        // RFC-0008 v2: active execution rows retire; finite one-use history survives independently (a no-op with no root).
        palw_exec_v2_fold::on_claim_retired_v2(self, &id)?;
        self.write_claim(id, None)?;
        // Lane BUDGET: the claim left the state; its budget row leaves once it is out of its window too.
        self.bond_budget_forget_v1(&id)?;
        // ADR-0160 F-Q: the claim's audit receipts retire with it.
        if self.state.audit_status_of_v1(&id).is_some() {
            self.write_audit_receipts_v1(id, None)?;
        }
        if self.state.panels.contains_key(&id) {
            self.write_panel(id, None)?;
        }
        // ADR-0078 Decision 4: the claim's derivations retire with it (`CLAIM_RETIREMENT`). A row
        // keyed to a claim the chain no longer holds would be a name bound to nothing, and the
        // table is bounded per claim precisely so that it is bounded overall.
        let derived: Vec<crate::palw_derived_v1::PalwDerivedKeyV1> = self
            .state
            .derived_artifacts
            .range(crate::palw_derived_v1::PalwDerivedKeyV1::claim_range(id))
            .map(|(key, _)| *key)
            .collect();
        for key in derived {
            self.write_derived_artifact(key, None)?;
        }
        Ok(())
    }

    /// Give back what [`Self::void_claim`] held. Exposure only — the immature contribution was
    /// released at the void.
    pub(super) fn release_abandon_hold(&mut self, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        let current = self.state.reserved_exposure.get(&claim.bond).copied().unwrap_or(0);
        // The whole reservation, as `release_for_claim` — a held free-prompt claim's receipt rights
        // come back with its weight (its escrow term is zero), and one expression keeps the sites one.
        // ADR-0160 E-4: an attempt's obligation hold gives back what the void found held — the same
        // expression the ledger's re-derivation sums (`palw_claim_held_commitment_v1`; `full` on a
        // free-prompt `BindTimeout`, which no licence ever released).
        let held = palw_claim_held_commitment_v1(self.params, claim).ok_or(PalwStateV2Error::Overflow("reserved_exposure"))?;
        let next = current.checked_sub(held).ok_or(PalwStateV2Error::Overflow("reserved_exposure underflow"))?;
        self.write_exposure(claim.bond, if next == 0 { None } else { Some(next) })?;
        Ok(())
    }
}

impl<'a> TransitionBuilder<'a> {
    /// **The one writer of `vesting`** (ADR-0152 V-3): the row map, its four derived indexes and
    /// the `Vesting` (71) delta entry in one step, so apply, revert and the fold cannot disagree
    /// about the indexes. M3's DA-5 re-key (a session moves its row's `expiry_daa`) writes through
    /// this too — rewriting a row is remove-old-then-insert-new in every index.
    pub(super) fn write_vesting(
        &mut self,
        key: Hash64,
        new: Option<crate::palw_vesting_v1::PalwVestingRowV1>,
    ) -> Result<(), PalwStateV2Error> {
        let requests = if self.native_reservation.is_tracking_v1() { Self::vesting_index_requests_v1(&new)? } else { 0 };
        self.reserve_mutation_sequence_v1(requests, 2)?;
        self.reserve_map_journal_v1(&key, &new, !self.state.vesting.contains_key(&key), "native vesting writer")?;
        if new.is_none()
            && !self.state.claims.contains_key(&key)
            && !self.state.panel_liabilities.contains_key(&key)
            && self.state.claim_semantics.contains_key(&key)
        {
            self.write_claim_semantics_v1(key, None)?;
        }
        let old = self.state.vesting.remove(&key);
        if let Some(previous) = &old {
            self.state.vesting_index_remove(previous);
        }
        if let Some(row) = &new {
            self.state.vesting.insert(key, row.clone());
            self.state.vesting_index_insert(row);
        }
        if old != new {
            self.entries.push(PalwDeltaEntryV2::Vesting { key, old, new });
        }
        Ok(())
    }

    /// The vesting counters (73), journaled whole as `SettledFinals` is.
    pub(super) fn write_vesting_counters(&mut self, new: crate::palw_vesting_v1::PalwVestingCountersV1) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native vesting_counters writer")?;
        let old = self.state.vesting_counters;
        if old != new {
            self.state.vesting_counters = new;
            self.entries.push(PalwDeltaEntryV2::VestingCounters { old, new });
        }
        Ok(())
    }

    /// A journal-only note (72): the cause of a vesting change (phase2-plan §2.5).
    pub(super) fn note_vesting(&mut self, note: crate::palw_vesting_v1::PalwVestingNoteV1) -> Result<(), PalwStateV2Error> {
        self.reserve_journal_slots_v1(1, "native vesting note journal")?;
        self.entries.push(PalwDeltaEntryV2::VestingNote(note));
        Ok(())
    }

    /// **ADR-0152 V-1/V-2: write a Final claim's row** (from `finalize_claim`, past
    /// `palw_rcore_plus` only). The attribution fields are COPIES (v3.1 N8): from the liability
    /// record `persist_panel_liability` has just written — M2 fills its four fields where
    /// `offence_attribution_active` — else the claim record's `job_identity` and zeros; the door
    /// and the basis are `claim.rcore`'s (S's licence doors write them; until they land the door
    /// is `None` and the row records `Quorum`, the zero tag, with `basis_k` 0). The expiry and the
    /// settled count are the lock's (`palw_panel_liability_expiry_v1(final_daa, window_court)`, the
    /// state's `settled_attempt_finals`), so the row and the claim's locks run on the same two
    /// clocks (V-4(a)). Counters: `created`. Note: the buyback slice (V-3's identity).
    pub(super) fn write_vesting_row_at_final(
        &mut self,
        id: Hash64,
        claim: &PalwClaimStateV2,
        final_daa: u64,
        vested: PalwVestedRewardV1,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_vesting_v1::{PalwVestingNoteV1, PalwVestingRowV1};
        debug_assert!(!self.state.vesting.contains_key(&id), "a claim finalizes once, so it writes one row");
        let (job_identity, free_prompt, trace_root, segment_count) = match self.state.panel_liabilities.get(&id) {
            Some(record) => (record.job_identity, record.free_prompt, record.trace_root, record.segment_count),
            None => (claim.job_identity, false, Hash64::default(), 0),
        };
        let row = PalwVestingRowV1 {
            claim_id: id,
            producer_bond: claim.bond,
            class_id: claim.class_id,
            execution_root: claim.execution_root,
            artifact_root: self.state.classes.get(&claim.class_id).map(|class| class.artifact_root).unwrap_or_default(),
            job_identity,
            free_prompt,
            trace_root,
            segment_count,
            licence_door: claim.rcore.licence_door.unwrap_or(crate::palw_economic_safety_v1::PalwLicenceDoorTagV1::Quorum),
            basis_k: claim.rcore.basis_k,
            escrowed_reward: claim.escrowed_reward,
            buyback_bound: vested.buyback,
            producer: vested.producer,
            seats: vested.seats,
            reserve: vested.reserve,
            final_daa,
            expiry_daa: match palw_exec_v2_rewards_v1::horizon(&self.state, self.params, &id)? {
                Some(until) => until.checked_add(1).ok_or(PalwStateV2Error::Overflow("EXEC vesting expiry"))?,
                None => crate::palw_panel_var_v1::palw_panel_liability_expiry_v1(final_daa, self.params.window_court),
            },
            settled_at_final: self.state.settled_attempt_finals,
            matured_at: None,
        };
        let mut counters = self.state.vesting_counters;
        counters.created =
            counters.created.checked_add(row.total_sompi_u128()).ok_or(PalwStateV2Error::Overflow("vesting created"))?;
        self.write_vesting(id, Some(row))?;
        self.write_vesting_counters(counters)?;
        if vested.buyback > 0 {
            self.note_vesting(PalwVestingNoteV1::BuybackAtFinal { claim_id: id, sompi: vested.buyback })?;
        }
        Ok(())
    }

    /// **ADR-0152 V-5: the burn hook** (S-SPEC P5/P12; S-4's conviction funnel will call it, and
    /// until that lands only the tests do). S-1 declared it with a stub; the integration
    /// (rcore/int-1) deleted that stub for this body. Deletes `claim_id`'s row if one exists and
    /// returns its whole amount — `Some` is S3's once-per-claim marker; `None` when the claim has no
    /// row (never written, an FP claim, already burned, or already MOVED to the queue, after which
    /// there is nothing to burn).
    /// A row is burnable until it moves, latched-but-carried included. Burned value goes to
    /// `vesting_burned` and is never minted. Counters: `burned`. Note: `Burned`, naming the offence
    /// and its kind so Phase 2 can attribute every burn (V-2b).
    #[allow(dead_code)] // S-4's conviction funnel calls it; until it lands only the tests do.
    pub(super) fn burn_vesting_row(
        &mut self,
        claim_id: Hash64,
        offence_id: Hash64,
        kind: crate::palw_offence_v1::PalwOffenceKindV1,
    ) -> Result<Option<u64>, PalwStateV2Error> {
        let Some(row) = self.state.vesting.get(&claim_id).cloned() else {
            palw_exec_v2_rewards_v1::burn(self, claim_id)?;
            return Ok(None);
        };
        for (bond, _) in &row.seats {
            self.cancel_native_recipient_v1(claim_id, *bond, 1)?;
        }
        self.cancel_native_recipient_v1(claim_id, row.producer_bond, 0)?;
        let mut counters = self.state.vesting_counters;
        counters.burned = counters.burned.checked_add(row.total_sompi_u128()).ok_or(PalwStateV2Error::Overflow("vesting burned"))?;
        self.write_vesting(claim_id, None)?;
        self.write_vesting_counters(counters)?;
        palw_exec_v2_rewards_v1::burn(self, claim_id)?;
        let sompi = row.total_sompi();
        self.note_vesting(crate::palw_vesting_v1::PalwVestingNoteV1::Burned {
            claim_id,
            offence_id,
            kind,
            sompi,
            legs: row.legs().collect(),
        })?;
        Ok(Some(sompi))
    }

    /// **ADR-0152 V-4: step 3d's latch.** Every unlatched row `palw_vesting_row_maturity_v1` calls
    /// mature gets `matured_at = now`, once; nothing ever clears it but a delta revert (I-9). During
    /// a licence halt (V-4(b), chain-wide) nothing latches and nothing is walked.
    ///
    /// **The walk is the size of what latches, not of what waits** (phase2-plan I-12; review of the
    /// vesting work, finding 5 — the first cut re-examined every row the second clock held, each for
    /// up to `2 × window_court`, every block). Not halted, an unlatched row is mature iff its DAA
    /// clock has run (`expiry ≤ now`) and either no second clock holds (escaped depth `None`, or
    /// `expiry ≤ now − 2 × window_court`, the per-obligation bound) or `depth` anchors have settled
    /// since its Final (`settled_at_final ≤ settled_now − depth`). So:
    /// 1. **the DAA side**: the rows with `expiry` at or below `released` — `now` with no second
    ///    clock, `now − 2 × window_court` with one — are a prefix of `vesting_unlatched`, and every
    ///    one of them latches here, so each is walked once in its life;
    /// 2. **the band** `(released, now]` with a second clock: the mature rows are A ∩ B, A the band
    ///    of `vesting_unlatched`, B the prefix of `vesting_unlatched_by_settled` at
    ///    `settled_now − depth`. The two are walked in lockstep and the walk stops at whichever ends
    ///    first; that side's rows are then tested against the other side's bound, so the result is
    ///    exactly A ∩ B and the cost is `2 × min(|A|, |B|)`. **Along one chain the two clocks are
    ///    co-monotone** — a row written in a later block has a later expiry (`final + window_court`,
    ///    and DAA only grows) and a settled count no smaller (the counter only grows) — so a row in
    ///    A but not B (the second clock holds it) and a row in B but not A (its DAA clock runs) cannot
    ///    both exist: the first would be older than the second yet have counted more anchors. One of
    ///    `A ∖ B`, `B ∖ A` is empty, and the walk costs `2 × (rows latched + 1)`. Only a row written
    ///    out of order — M3's DA-5 re-key moves a row's expiry later and keeps its old count — can
    ///    add to it, one step per such row.
    ///
    /// The candidates are then put through `palw_vesting_row_maturity_v1` itself, the one
    /// definition of V-4 (so M3's V-4(c) session check applies here unchanged), and latched in
    /// `(expiry_daa, claim_id)` order.
    pub(super) fn latch_matured_vesting_rows(&mut self, now_daa: u64, raw_depth: Option<u64>) -> Result<(), PalwStateV2Error> {
        use crate::palw_vesting_v1::{PalwVestingNoteV1, PalwVestingRowV1, palw_vesting_row_maturity_v1};
        use std::ops::Bound;
        let window_court = self.params.window_court;
        let escaped = palw_second_clock_depth_v1(raw_depth, &self.state.recent_anchor_daas, now_daa, window_court);
        if raw_depth.is_some() && escaped.is_none() {
            // V-4(b): a licence halt (`palw_chain_vesting_halted_v1`) latches nothing.
            return Ok(());
        }
        let top = Hash64::from_bytes([0xFF; 64]);
        // A depth of zero holds nothing (`palw_second_clock_holds_v1`: no count is below zero), so
        // it is the DAA clock alone, as no second clock is.
        let second_clock = escaped.filter(|depth| *depth > 0);
        let released = match second_clock {
            None => Some(now_daa),
            Some(_) => now_daa.checked_sub(window_court.saturating_mul(2)),
        };
        let mut candidates: Vec<(u64, Hash64)> = Vec::new();
        // 1. The DAA side: every row here is mature by the DAA clock alone.
        if let Some(released) = released {
            candidates.extend(self.state.vesting_unlatched.range(..=(released, top)).copied());
        }
        // 2. The band, where the second clock decides: A ∩ B by the lockstep walk.
        if let Some(depth) = second_clock
            && let Some(settled_bound) = self.state.settled_attempt_finals.checked_sub(depth)
        {
            let lower = match released {
                Some(released) => Bound::Excluded((released, top)),
                None => Bound::Unbounded,
            };
            let in_band = |expiry: u64| expiry <= now_daa && released.is_none_or(|released| expiry > released);
            let mut band = self.state.vesting_unlatched.range((lower, Bound::Included((now_daa, top))));
            let mut counted = self.state.vesting_unlatched_by_settled.range(..=(settled_bound, top));
            let mut walked_band: Vec<(u64, Hash64)> = Vec::new();
            let mut walked_counted: Vec<(u64, Hash64)> = Vec::new();
            let band_ended = loop {
                match band.next() {
                    None => break true,
                    Some(entry) => walked_band.push(*entry),
                }
                match counted.next() {
                    None => break false,
                    Some(entry) => walked_counted.push(*entry),
                }
            };
            if band_ended {
                // Every band row seen: keep the ones whose anchor count is met.
                for (expiry, claim_id) in walked_band {
                    if self.state.vesting.get(&claim_id).is_some_and(|row| row.settled_at_final <= settled_bound) {
                        candidates.push((expiry, claim_id));
                    }
                }
            } else {
                // Every count-met row seen: keep the ones in the band.
                for (_, claim_id) in walked_counted {
                    if let Some(row) = self.state.vesting.get(&claim_id)
                        && in_band(row.expiry_daa)
                    {
                        candidates.push((row.expiry_daa, claim_id));
                    }
                }
            }
        }
        candidates.sort_unstable();
        for (_, claim_id) in candidates {
            let Some(row) = self.state.vesting.get(&claim_id) else { continue };
            if palw_vesting_row_maturity_v1(&self.state, self.params, row, now_daa, raw_depth).mature_now {
                let latched = PalwVestingRowV1 { matured_at: Some(now_daa), ..row.clone() };
                self.write_vesting(claim_id, Some(latched))?;
                self.note_vesting(PalwVestingNoteV1::Latched { claim_id, matured_at: now_daa })?;
            }
        }
        Ok(())
    }

    /// **ADR-0152 V-7: move one planned move into the queue.** Each leg through the queue's own
    /// writers (phase2-plan I-2): a producer leg under its A-KEY key, a seat leg through
    /// `add_panel_payout` (0xFE, accumulated per payee), a reporter leg under its A-KEY key, the
    /// reserve into `panel_reserve_sompi`. The payee is the payload stored at Final or conviction,
    /// never re-read from a bond (I-5), so a retired or emptied payee is still paid. Then the row
    /// (or the reporter entry) leaves; counters `moved`; notes `Moved` and, for a positive reserve,
    /// `ReserveCredited`. Maturity inserts are exempt from `PALW_V2_MAX_PENDING_PAYOUTS` (M-10):
    /// the budget, not the cap, bounds them.
    pub(super) fn apply_vesting_move(
        &mut self,
        next: &crate::palw_vesting_v1::PalwVestingMoveV1,
        now: u64,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_vesting_v1::{
            PalwVestingLegKindV1, PalwVestingNoteV1, PalwVestingSourceV1, palw_reporter_payout_key_v1, palw_vesting_payout_key_v1,
        };
        // RFC-0008 once-only settlement (dormant): a vesting row (or reporter entitlement) is released once across every consumer.
        // A move another consumer already settled retires its row below without moving value again. `Dormant` below the fence.
        let pays = {
            use crate::palw_settlement_once_v1::{
                PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1,
            };
            let subject = match &next.source {
                PalwVestingSourceV1::Row { claim_id } => claim_id.as_bytes(),
                PalwVestingSourceV1::Reporter { offence_id } => offence_id.as_bytes(),
            };
            let total = next
                .legs
                .iter()
                .try_fold(0u64, |sum, leg| sum.checked_add(leg.amount))
                .ok_or(PalwStateV2Error::Overflow("vesting move total"))?;
            // RFC08-2: a vesting row is its claim's held reward (its own expiry is the horizon it vested past); a reporter
            // entitlement is a slash-funded award that stands behind no claim.
            let leg = match &next.source {
                PalwVestingSourceV1::Row { claim_id } => match self.state.vesting.get(claim_id) {
                    Some(row) => super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::V2Claim {
                        claim: *claim_id,
                        final_daa: Some(row.final_daa),
                        horizon: Some(row.expiry_daa.saturating_sub(1)),
                        draw: 0,
                        paid_now: total,
                        exported_before: None,
                        vest_until: Some(row.expiry_daa),
                    },
                    None => super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::Unsourced("the vesting row of the move"),
                },
                PalwVestingSourceV1::Reporter { .. } => super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1::NoLiableClaim(
                    "a reporter's slash-funded award stands behind no claim",
                ),
            };
            self.settle_once_liable_v1(
                now,
                PalwSettlementObligationV1::of(PalwSettlementLegV1::VestingRelease, subject),
                PalwSettlementConsumerV1::VestingMaturity,
                total,
                PalwSettlementDebitV1::Held { amount: total },
                leg,
            )?
            .pays()
        };
        if pays && let PalwVestingSourceV1::Row { claim_id } = &next.source {
            let row = self.state.vesting.get(claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(*claim_id))?;
            let mut recipients: BTreeMap<PalwBondKeyV2, PalwPayoutV2> = BTreeMap::new();
            for (bond, payout) in &row.seats {
                let total = recipients.entry(*bond).or_insert(PalwPayoutV2 { payload: payout.payload, amount: 0 });
                if total.payload != payout.payload {
                    return Err(PalwStateV2Error::BondBudgetRefused("one Panel recipient has inconsistent frozen payloads".into()));
                }
                total.amount = total.amount.checked_add(payout.amount).ok_or(PalwStateV2Error::Overflow("Panel recipient income"))?;
            }
            for (bond, payout) in recipients {
                self.pay_native_recipient_v1(*claim_id, bond, 1, now, payout.amount, palw_panel_payout_key_v1(&payout.payload), true)?;
            }
            let finished = !self
                .state
                .exec_v2_rewards
                .get(claim_id)
                .is_some_and(|receipt| receipt.legs.keys().any(|(bond, _)| *bond == row.producer_bond));
            self.pay_native_recipient_v1(
                *claim_id,
                row.producer_bond,
                0,
                now,
                row.producer.amount,
                palw_vesting_payout_key_v1(claim_id),
                finished,
            )?;
        }
        if pays
            && let PalwVestingSourceV1::Reporter { offence_id } = &next.source
            && self.params.reporter_income_active_at(now)
        {
            let book = self
                .state
                .bond_budget
                .as_ref()
                .ok_or_else(|| PalwStateV2Error::BondBudgetRefused("reporter move has no budget".into()))?;
            let binding = book.reporter_income.get(&palw_bond_budget_fold_v1::native_reporter_binding_id_v1(*offence_id)).copied();
            if let Some(binding) = binding.filter(|row| row.recipient.is_some()) {
                if binding.accepted_daa >= self.params.reporter_income_from().expect("checked reporter fence")
                    || binding.awarded_daa.is_none()
                    || next.legs.iter().any(|leg| leg.payload != binding.payload || leg.amount != binding.maximum)
                {
                    return Err(PalwStateV2Error::BondBudgetRefused("legacy reporter move differs from frozen entitlement".into()));
                }
                self.pay_native_recipient_v1(
                    *offence_id,
                    binding.recipient.expect("filtered"),
                    3,
                    now,
                    binding.maximum,
                    palw_reporter_payout_key_v1(offence_id),
                    true,
                )?;
            } else {
                let queue = palw_reporter_payout_key_v1(offence_id);
                let legacy =
                    book.reporter_income.get(&palw_bond_budget_fold_v1::legacy_queue_liability_id_v1(queue)).ok_or_else(|| {
                        PalwStateV2Error::BondBudgetRefused("payload-only reporter move has no exact legacy liability".into())
                    })?;
                if legacy.recipient.is_some()
                    || legacy.queue != Some(queue)
                    || legacy.paid_daa.is_some()
                    || next.legs.iter().any(|leg| leg.payload != legacy.payload || leg.amount != legacy.maximum)
                {
                    return Err(PalwStateV2Error::BondBudgetRefused("payload-only reporter move differs from old exact cash".into()));
                }
            }
            // Old payload-only awarded rows are exact migration liabilities:
            // they retain cash; a global new-R quarantine closes their W clock.
        }
        let own_key = match &next.source {
            PalwVestingSourceV1::Row { claim_id } => palw_vesting_payout_key_v1(claim_id),
            PalwVestingSourceV1::Reporter { offence_id } => palw_reporter_payout_key_v1(offence_id),
        };
        for leg in next.legs.iter().filter(|leg| pays && leg.amount > 0) {
            match leg.kind {
                PalwVestingLegKindV1::Seat => self.add_panel_payout(leg.payload, leg.amount)?,
                PalwVestingLegKindV1::Reserve => {
                    let reserve =
                        self.state.panel_reserve_sompi.checked_add(leg.amount).ok_or(PalwStateV2Error::Overflow("panel reserve"))?;
                    self.write_panel_reserve(reserve)?;
                }
                PalwVestingLegKindV1::Producer | PalwVestingLegKindV1::Reporter => {
                    debug_assert_eq!(leg.queue_key, Some(own_key), "a producer or reporter leg lands on its source's A-KEY key");
                    // One key per claim or offence, so it holds no row; accumulated all the same,
                    // so no path can overwrite a queued sompi.
                    let held = self.state.pending_payouts.get(&own_key).map(|row| row.amount).unwrap_or(0);
                    let amount = held.checked_add(leg.amount).ok_or(PalwStateV2Error::Overflow("vesting payout"))?;
                    self.write_payout(own_key, Some(PalwPayoutV2 { payload: leg.payload, amount }))?;
                }
            }
        }
        match &next.source {
            PalwVestingSourceV1::Row { claim_id } => {
                let row = self.state.vesting.get(claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(*claim_id))?;
                // The counters are read BEFORE the row's removal, exactly as the historical order did.
                let mut counters = self.state.vesting_counters;
                if pays {
                    counters.moved =
                        counters.moved.checked_add(row.total_sompi_u128()).ok_or(PalwStateV2Error::Overflow("vesting moved"))?;
                }
                self.write_vesting(*claim_id, None)?;
                if pays {
                    self.write_vesting_counters(counters)?;
                    self.note_vesting(PalwVestingNoteV1::Moved { source: next.source.clone(), legs: next.legs.clone() })?;
                    if row.reserve > 0 {
                        self.note_vesting(PalwVestingNoteV1::ReserveCredited { claim_id: *claim_id, sompi: row.reserve })?;
                    }
                }
            }
            PalwVestingSourceV1::Reporter { offence_id } => {
                // Row 11: S-7 wrote it at step 2; 3d moves it and deletes it (the `ReporterReward`
                // entry, 69). The reporter counters are S's, written at the award.
                // RFC-0006: the removal's journal slot is admitted before the push (the row moves into the journal).
                self.reserve_journal_slots_v1(1, "native reporter reward removal journal")?;
                let old = self.state.reporter_rewards.remove(offence_id);
                if old.is_some() {
                    self.entries.push(PalwDeltaEntryV2::ReporterReward { key: *offence_id, old, new: None });
                }
                if pays {
                    self.note_vesting(PalwVestingNoteV1::Moved { source: next.source.clone(), legs: next.legs.clone() })?;
                }
            }
        }
        Ok(())
    }
}

impl<'a> TransitionBuilder<'a> {
    /// ADR-0095 §4.11 as corrected. The membership's own fence.
    pub(super) fn require_model_benefits(&self) -> Result<(), PalwStateV2Error> {
        if self.extras.model_benefits_active { Ok(()) } else { Err(PalwStateV2Error::ModelLinesNotArmed) }
    }

    pub(super) fn require_model_lines(&self) -> Result<(), PalwStateV2Error> {
        if self.extras.model_lines_active { Ok(()) } else { Err(PalwStateV2Error::ModelLinesNotArmed) }
    }

    pub(super) fn require_active_bond(&self, bond: &PalwBondKeyV2) -> Result<PalwBondStateV2, PalwStateV2Error> {
        let record = self.state.bonds.get(bond).ok_or(PalwStateV2Error::MissingBond(*bond))?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err(PalwStateV2Error::ModelBondNotActive(*bond));
        }
        Ok(record.clone())
    }

    /// The line, with its founding row and version 1 materialised when it had none — the first
    /// write to a founding line is what puts it in the state root (ADR-0088 Decision 1).
    pub(super) fn model_line_for_write(
        &mut self,
        line_id: &Hash64,
    ) -> Result<crate::palw_model_lines_v1::PalwModelLineV1, PalwStateV2Error> {
        if let Some(row) = self.state.model_lines.get(line_id) {
            return Ok(row.clone());
        }
        let line = self.state.model_line_or_founding(line_id).ok_or(PalwStateV2Error::ModelLineMissing(*line_id))?;
        let v1 = self.state.model_version(line_id, 1).ok_or(PalwStateV2Error::ModelVersionMissing(*line_id, 1))?;
        self.write_model_line(*line_id, Some(line.clone()))?;
        self.write_model_version((*line_id, 1), Some(v1))?;
        Ok(line)
    }

    /// **ADR-0162 amendment (user decision 2026-10-16): no Model Position on a genesis or annual-fork model.** Past the tier fence
    /// (`palw_model_tier_v1`, ADR-0179), a model in `ApprovedModels` (genesis assembly, genesis catalog or an active annual edition,
    /// tier 1.0) takes no buy and no new line: only an added (registered, 0.3) model — RFC-0004's add-model path — is positioned.
    /// Positions on the canonical models would hand their earliest holders a first-mover gain on models nobody had to win. Below the
    /// fence, and for a registered model, `Ok` — byte-identical. Existing units stay sellable (only entry is refused).
    pub(super) fn refuse_position_on_approved_model_v1(&self, class_id: &Hash64, daa: u64) -> Result<(), PalwStateV2Error> {
        let Some(mirror) = self.params.model_tier_v1() else { return Ok(()) };
        if !mirror.active_at(daa) {
            return Ok(());
        }
        if crate::palw_model_tier_v1::palw_model_tier_at_v1(&self.state, self.params, class_id, daa).is_approved() {
            return Err(PalwStateV2Error::ModelPositionOnApprovedModel(*class_id));
        }
        Ok(())
    }

    /// **ADR-0162: the market a seed or a buy past the virtual fence (in force from `fence_daa`) acts
    /// on** — [`PalwChainStateV2::model_market_in_force_v2`] at this block, which opens every line
    /// from its creation. Refused `ModelLineOnFloor` where it opens nothing (the floor class has no
    /// line, so no market but one a pre-fence seed made), `ModelLineMissing` for a line the registry
    /// does not know.
    pub(super) fn model_market_for_move_v2(
        &self,
        line_id: &Hash64,
        fence_daa: u64,
    ) -> Result<crate::palw_model_market_v1::PalwModelMarketV1, PalwStateV2Error> {
        match self.state.model_market_in_force_v2(line_id, fence_daa, &self.params.base_class_id()) {
            Some(market) if market.is_open() => Ok(market),
            Some(_) => Err(PalwStateV2Error::ModelLineOnFloor),
            None if self.state.model_line_or_founding(line_id).is_some() => Err(PalwStateV2Error::ModelLineOnFloor),
            None => Err(PalwStateV2Error::ModelLineMissing(*line_id)),
        }
    }

    /// ADR-0088 Decision 8: the leg to the owner and, when a proposal was adopted and the owner
    /// shares, to the contributor; burned where a payee has no bond.
    pub(super) fn pay_model_leg(
        &mut self,
        ctx: &PalwBlockContextV2,
        line_id: &Hash64,
        holder: &Hash64,
        tag: &[u8],
        line: &crate::palw_model_lines_v1::PalwModelLineV1,
        leg: u64,
        after: &mut crate::palw_model_market_v1::PalwModelMarketV1,
    ) -> Result<(), PalwStateV2Error> {
        let owner_bond = line.owner.filter(|b| self.state.bonds.contains_key(b));
        let owner = owner_bond.and_then(|b| self.state.bonds.get(&b)).map(|b| b.payout_payload);
        let contributor_bond = self
            .state
            .model_version(line_id, line.current)
            .and_then(|v| v.adopted_from)
            .and_then(|p| self.state.model_proposals.get(&p).map(|p| p.by))
            .filter(|b| self.state.bonds.contains_key(b));
        let contributor = contributor_bond.and_then(|b| self.state.bonds.get(&b)).map(|b| b.payout_payload);
        let (to_contributor, to_owner) =
            crate::palw_model_lines_v1::split_owner_leg_v1(leg, line.contributor_permille_of_leg, contributor.is_some());
        let mut owner_tag = tag.to_vec();
        owner_tag.extend_from_slice(b"-owner");
        let mut contributor_tag = tag.to_vec();
        contributor_tag.extend_from_slice(b"-contributor");
        let owner_cash = self.improvement_owner_cash_v1(line_id, to_owner, ctx.daa_score);
        let owner_queue = (owner_cash > 0).then(|| self.model_fee_key_v1(ctx, line_id, holder, &owner_tag));
        let contributor_queue = (to_contributor > 0).then(|| self.model_fee_key_v1(ctx, line_id, holder, &contributor_tag));
        // The accepted signed quote fixes the entire collectible fee. Reserve
        // both actual bond recipients before any payout, position or pool write;
        // the pool diversion remains the owner's income. Principal is separate.
        // EVM action refusals retain the block, so the first leg must roll back
        // if the second leg fails (including first-time budget migration).
        if self.params.bond_budget_active_at(ctx.daa_score) {
            let checkpoint = self.try_checkpoint()?;
            let reserve = (|| {
                use crate::palw_bond_budget_v1::PalwBudgetOriginV1;
                if let Some(bond) = owner_bond {
                    self.reserve_bond_income_for_queue_v1(ctx, bond, to_owner, to_owner, PalwBudgetOriginV1::MarketFee, owner_queue)?;
                }
                if let Some(bond) = contributor_bond {
                    self.reserve_bond_income_for_queue_v1(
                        ctx,
                        bond,
                        to_contributor,
                        to_contributor,
                        PalwBudgetOriginV1::MarketFee,
                        contributor_queue,
                    )?;
                }
                Ok(())
            })();
            if let Err(error) = reserve {
                self.restore(checkpoint);
                return Err(error);
            }
        }
        // RFC-0004 (spec 17 §17.11.1): φ of a governed line's owner part feeds its improvement pool.
        // The market counts the pool's part as the owner's leg paid (ADR-0087 M2 keeps closing): φ is
        // the owner's money, routed.
        let owner_part = to_owner;
        let to_owner = self.take_improvement_phi_v1(line_id, to_owner, ctx.daa_score)?;
        let to_pool = owner_part - to_owner;
        debug_assert_eq!(to_owner, owner_cash);
        let burned_owner = self.write_model_fee(ctx, line_id, holder, &owner_tag, owner, to_owner)?;
        let burned_contributor = self.write_model_fee(ctx, line_id, holder, &contributor_tag, contributor, to_contributor)?;
        after.burned_sompi = after.burned_sompi.saturating_add(burned_owner).saturating_add(burned_contributor);
        after.registrant_paid_sompi = after.registrant_paid_sompi.saturating_add(to_owner - burned_owner).saturating_add(to_pool);
        after.contributor_paid_sompi = after.contributor_paid_sompi.saturating_add(to_contributor - burned_contributor);
        Ok(())
    }

    /// **ADR-0091 Decision 2: the chain's own move at a claim's `Final`.** The slice of the escrow
    /// buys from the pair of the line the claim was attributed to (`claim_roots`, written at
    /// accept where the lines fence was armed); the positions the curve gives up are retired.
    /// Returns the slice taken — `0` where no pair takes it (no root recorded, no line, no market,
    /// a closed market: Decision 3), so a reward that finds no pair is a reward paid in full.
    /// Never fails and never touches a holder: the row is the only thing written.
    pub(super) fn model_buyback_at_final(
        &mut self,
        claim_id: &Hash64,
        claim: &PalwClaimStateV2,
        escrow: u64,
    ) -> Result<u64, PalwStateV2Error> {
        use crate::palw_model_market_v1::{palw_model_buyback_quote_v1, palw_model_buyback_slice_v1};
        let Some(root) = self.state.claim_roots.get(claim_id).copied() else { return Ok(0) };
        let Some(line_id) = self.state.artifact_line_of_root(&claim.class_id, &root, self.extras.artifact_root_ownership_active)
        else {
            return Ok(0);
        };
        // ADR-0162 Decision 6: past the virtual fence, the line's market in force where a buy would be
        // admitted; below it, the row as written — one function with ADR-0152's lock price.
        let Some(market) = self.read().model_buyback_market_v2(&line_id) else { return Ok(0) };
        // ADR-0124 Decision 6: `escrow` is the claim's escrow as priced at this block — the whole
        // of it below the work-price fence, so every existing row is byte-identical.
        let Some(quote) = palw_model_buyback_quote_v1(&market, palw_model_buyback_slice_v1(escrow)) else {
            return Ok(0);
        };
        self.write_model_market(line_id, Some(quote.after))?;
        self.model_moves += 1;
        Ok(quote.slice)
    }

    /// ADR-0088 Decision 4: attribute an accepted claim to the version whose root it named, and
    /// remember the root for the court (Decision 3). Nothing below the fence.
    pub(super) fn note_claim_usage(&mut self, claim_id: Hash64, class_id: &Hash64, root: Hash64, pwu: u64, attempt: bool, daa: u64) -> Result<(), PalwStateV2Error> {
        if !self.extras.model_lines_active {
            return Ok(());
        }
        let Some((line_id, version)) =
            self.state.model_version_of_root(class_id, &root, daa, self.extras.artifact_root_ownership_active)
        else {
            return Ok(());
        };
        if self.model_line_for_write(&line_id).is_err() {
            return Ok(());
        }
        let Some(mut row) = self.state.model_versions.get(&(line_id, version)).cloned() else { return Ok(()) };
        row.usage.count(attempt, pwu, daa);
        self.write_model_version((line_id, version), Some(row))?;
        self.write_claim_root(claim_id, Some(root))?;
        Ok(())
    }

    /// ADR-0088 Decision 4: a voided claim is subtracted where it was counted.
    ///
    /// **Past ADR-0143's fence this asks `note_claim_usage`'s own question, with the claim's own
    /// accept height**, so the subtraction lands on the row the addition landed on. Below the fence
    /// it keeps its walk: a third copy of the positional first-match, and one that never filtered by
    /// force while the counting side always did — so on a line that withdrew and republished a root
    /// the two could already disagree. The fence closes that with the rest.
    pub(super) fn uncount_claim_usage(&mut self, claim_id: &Hash64, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        let Some(root) = self.state.claim_roots.get(claim_id).copied() else { return Ok(()) };
        let attempt = matches!(claim.source, PalwClaimSourceV2::Attempt);
        let hit = if self.extras.artifact_root_ownership_active {
            self.state.model_version_of_root(&claim.class_id, &root, claim.accepted_daa, true)
        } else {
            self.state
                .model_lines
                .iter()
                .filter(|(_, l)| l.class_id == claim.class_id)
                .flat_map(|(line_id, _)| {
                    self.state.model_versions.range((*line_id, 0)..=(*line_id, u32::MAX)).map(|((l, n), v)| ((*l, *n), v.root))
                })
                .find(|(_, r)| *r == root)
                .map(|(key, _)| key)
        };
        if let Some(key) = hit
            && let Some(mut row) = self.state.model_versions.get(&key).cloned()
        {
            row.usage.uncount(attempt, claim.pwu);
            self.write_model_version(key, Some(row))?;
        }
        Ok(())
    }

    /// Decision 2's promotion, shared by a non-preview publish and `ModelVersionPromoted`: the
    /// previous current is superseded with a grace, the line's current moves.
    pub(super) fn make_model_version_current(
        &mut self,
        ctx: &PalwBlockContextV2,
        line_id: &Hash64,
        line: &mut crate::palw_model_lines_v1::PalwModelLineV1,
        version: u32,
    ) -> Result<(), PalwStateV2Error> {
        use crate::palw_model_lines_v1::{PALW_VERSION_GRACE_DAA_V1, PalwVersionStatusV1};
        if let Some(mut previous) = self.state.model_versions.get(&(*line_id, line.current)).cloned() {
            previous.status = PalwVersionStatusV1::Superseded { until_daa: ctx.daa_score.saturating_add(PALW_VERSION_GRACE_DAA_V1) };
            self.write_model_version((*line_id, line.current), Some(previous))?;
        }
        line.current = version;
        line.previews.retain(|v| *v != version);
        Ok(())
    }
}
