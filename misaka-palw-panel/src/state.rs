use crate::{draw::*, types::*};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// New versioned carriage; it never changes the historical V2 state or PanelBound codec.
/// JSON is for observation only. Borsh carriage import requires the exact committed root.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionlessPanelStateV1 {
    version: u16,
    network: Hash64,
    ruleset: Hash64,
    policy: PanelPolicyV1,
    tip: Hash64,
    height: u64,
    daa: u64,
    next_order: u64,
    claims: BTreeMap<Hash64, ClaimRecordV3>,
    /// Retained after expiry: a new signature/id cannot obtain another draw for the same work.
    work_ids: BTreeSet<Hash64>,
    beacons: BTreeMap<u64, Hash64>,
    #[serde(serialize_with = "serialize_reservations")]
    reservations: BTreeMap<BondIdV1, u128>,
}

impl PermissionlessPanelStateV1 {
    pub fn new(
        network: Hash64,
        ruleset: Hash64,
        policy: PanelPolicyV1,
        tip: Hash64,
        height: u64,
        daa: u64,
    ) -> Result<Self, PanelErrorV1> {
        policy.validate()?;
        Ok(Self {
            version: WIRE_VERSION_V1,
            network,
            ruleset,
            policy,
            tip,
            height,
            daa,
            next_order: 0,
            claims: BTreeMap::new(),
            work_ids: BTreeSet::new(),
            beacons: BTreeMap::new(),
            reservations: BTreeMap::new(),
        })
    }

    pub fn claim(&self, id: &Hash64) -> Option<&ClaimRecordV3> {
        self.claims.get(id)
    }
    pub fn claims(&self) -> impl Iterator<Item = (&Hash64, &ClaimRecordV3)> {
        self.claims.iter()
    }
    pub fn reserved(&self, id: &BondIdV1) -> u128 {
        self.reservations.get(id).copied().unwrap_or(0)
    }
    pub fn root(&self) -> Hash64 {
        digest("misaka-palw/panel-v3/state", self)
    }
    pub fn policy(&self) -> PanelPolicyV1 {
        self.policy
    }

    // ---- the keyed decomposition a production state store journals (cursor + rows; reservations are derived) ----

    pub fn network(&self) -> Hash64 {
        self.network
    }
    pub fn ruleset(&self) -> Hash64 {
        self.ruleset
    }
    pub fn tip(&self) -> Hash64 {
        self.tip
    }
    pub fn height(&self) -> u64 {
        self.height
    }
    pub fn daa(&self) -> u64 {
        self.daa
    }
    pub fn next_order(&self) -> u64 {
        self.next_order
    }

    /// The scalar header; the rest of the state is [`Self::claim_rows`], [`Self::work_id_rows`] and [`Self::beacon_rows`].
    pub fn cursor(&self) -> PanelCursorV1 {
        PanelCursorV1 {
            version: self.version,
            network: self.network,
            ruleset: self.ruleset,
            policy: self.policy,
            tip: self.tip,
            height: self.height,
            daa: self.daa,
            next_order: self.next_order,
        }
    }

    /// An empty state at `cursor` (its policy must validate, its version be this one).
    pub fn from_cursor(cursor: PanelCursorV1) -> Result<Self, PanelErrorV1> {
        cursor.policy.validate()?;
        if cursor.version != WIRE_VERSION_V1 {
            return Err(PanelErrorV1::InvalidCarriage);
        }
        Ok(Self {
            version: cursor.version,
            network: cursor.network,
            ruleset: cursor.ruleset,
            policy: cursor.policy,
            tip: cursor.tip,
            height: cursor.height,
            daa: cursor.daa,
            next_order: cursor.next_order,
            claims: BTreeMap::new(),
            work_ids: BTreeSet::new(),
            beacons: BTreeMap::new(),
            reservations: BTreeMap::new(),
        })
    }

    pub fn claim_rows(&self) -> &BTreeMap<Hash64, ClaimRecordV3> {
        &self.claims
    }
    pub fn work_id_rows(&self) -> &BTreeSet<Hash64> {
        &self.work_ids
    }
    pub fn beacon_rows(&self) -> &BTreeMap<u64, Hash64> {
        &self.beacons
    }

    /// Delta application. These write one keyed row (or the cursor) and nothing else; a delta is applied row by row and
    /// [`Self::refresh_derived`] then re-derives the reservations. They validate nothing: a delta is the journal of a fold that
    /// already did, and the loader's [`Self::check_consistency`] is what refuses a state no fold could have written.
    pub fn set_cursor(&mut self, cursor: PanelCursorV1) {
        self.version = cursor.version;
        self.network = cursor.network;
        self.ruleset = cursor.ruleset;
        self.policy = cursor.policy;
        self.tip = cursor.tip;
        self.height = cursor.height;
        self.daa = cursor.daa;
        self.next_order = cursor.next_order;
    }
    pub fn put_claim_row(&mut self, id: Hash64, record: Option<ClaimRecordV3>) {
        match record {
            Some(record) => {
                self.claims.insert(id, record);
            }
            None => {
                self.claims.remove(&id);
            }
        }
    }
    pub fn put_work_id_row(&mut self, id: Hash64, present: bool) {
        if present {
            self.work_ids.insert(id);
        } else {
            self.work_ids.remove(&id);
        }
    }
    pub fn put_beacon_row(&mut self, epoch: u64, output: Option<Hash64>) {
        match output {
            Some(output) => {
                self.beacons.insert(epoch, output);
            }
            None => {
                self.beacons.remove(&epoch);
            }
        }
    }
    /// Re-derive the reservations from the bound claims (the only derived field).
    pub fn refresh_derived(&mut self) -> Result<(), PanelErrorV1> {
        self.rebuild_reservations()
    }
    /// Every invariant a state a fold could have written holds (the carriage import's check, without the root).
    pub fn check_consistency(&self) -> Result<(), PanelErrorV1> {
        self.validate()
    }
    /// Claims that are not terminal: the ones the engine is still clocking.
    pub fn live_claims(&self) -> impl Iterator<Item = (&Hash64, &ClaimRecordV3)> {
        self.claims.iter().filter(|(_, r)| !r.phase.terminal())
    }

    /// Complete state snapshots are bounded before deserialization to prevent attacker-supplied
    /// collection lengths allocating unbounded memory; integrity is checked against consensus root.
    pub fn import(
        bytes: &[u8],
        expected_root: Hash64,
        network: Hash64,
        ruleset: Hash64,
        policy: PanelPolicyV1,
    ) -> Result<Self, PanelErrorV1> {
        policy.validate()?;
        let limit = 4096u64
            + policy.max_tracked_claims as u64
                * (8192 + policy.max_candidates as u64 * 400 + (policy.max_retries as u64 + 1) * policy.seat_count as u64 * 100);
        if bytes.len() as u64 > limit {
            return Err(PanelErrorV1::ResourceLimit);
        }
        // Check the trusted consensus commitment before decoding any attacker-supplied lengths.
        if digest_bytes("misaka-palw/panel-v3/state", bytes) != expected_root {
            return Err(PanelErrorV1::InvalidCarriage);
        }
        let state = Self::try_from_slice(bytes).map_err(|_| PanelErrorV1::InvalidCarriage)?;
        if state.version != WIRE_VERSION_V1
            || state.network != network
            || state.ruleset != ruleset
            || state.policy != policy
            || state.root() != expected_root
        {
            return Err(PanelErrorV1::InvalidCarriage);
        }
        state.validate()?;
        Ok(state)
    }

    /// No producer/lane/signature/nonce input. Due assignments fold against the same pre-object
    /// headroom; admissions and newly certified outputs cannot change an already due Panel.
    ///
    /// `fold` is the whole step at once. A host that folds a block in the order the chain's own state transition runs it (due
    /// assignments against the pre-object base, certified outputs among the block's objects, admissions after them) calls the
    /// three stages separately: [`Self::advance`], [`Self::accept_beacon`] and [`Self::admit`]. The stages are the same code, in the
    /// same order, so `fold(step)` equals `advance` + `accept_beacon`* + `admit` (pinned by a test): a beacon-bearing block is inside
    /// the contribution window, and a claim is made ready only after it, so no claim's readiness depends on the order.
    pub fn fold(&self, step: &SelectedChainStepV1, view: &impl ConsensusViewV1) -> Result<(Self, PanelFoldEventsV1), PanelErrorV1> {
        self.validate()?;
        self.check_step(step)?;
        if step.admissions.len() > self.policy.max_admissions_per_block as usize
            || step.beacons.len() > self.policy.max_beacons_per_block as usize
        {
            return Err(PanelErrorV1::ResourceLimit);
        }
        let mut next = self.clone();
        let mut events = PanelFoldEventsV1::default();
        next.stage_release(step.daa, view, &mut events)?;
        next.stage_seal(step.daa, view, &mut events)?;
        for proof in &step.beacons {
            next.stage_beacon(step.daa, proof, view)?;
        }
        next.stage_ready(step.daa, &mut events)?;
        next.stage_assign(step.block, step.daa, view, &mut events)?;
        for (index, claim) in step.admissions.iter().enumerate() {
            next.stage_admit(step.block, step.height, step.daa, index as u32, claim)?;
        }
        next.stage_close(step)?;
        Ok((next, events))
    }

    fn check_step(&self, step: &SelectedChainStepV1) -> Result<(), PanelErrorV1> {
        if step.parent != self.tip || step.height != add(self.height, 1)? || step.daa < self.daa || step.block == step.parent {
            return Err(PanelErrorV1::NoncanonicalStep);
        }
        Ok(())
    }

    /// **Stage 1 of a block: everything that depends only on the parent base** — validated terminal outcomes release their
    /// reservations, claims past the checkpoint depth are sealed against the PARENT checkpoint, certified-output windows that
    /// closed make claims ready or terminate them `BeaconUnavailable`, and due assignments (first draws and receipt-timeout
    /// retries) are made against the host's headroom view. The cursor then moves to the step's block. Admissions and certified
    /// outputs of this block are not here: they are later stages, so neither can change a Panel that was already due.
    pub fn advance(
        &self,
        step: &SelectedChainStepV1,
        view: &impl ConsensusViewV1,
    ) -> Result<(Self, PanelFoldEventsV1), PanelErrorV1> {
        self.validate()?;
        self.check_step(step)?;
        if !step.admissions.is_empty() || !step.beacons.is_empty() {
            return Err(PanelErrorV1::NoncanonicalStep);
        }
        let mut next = self.clone();
        let mut events = PanelFoldEventsV1::default();
        next.stage_release(step.daa, view, &mut events)?;
        next.stage_seal(step.daa, view, &mut events)?;
        next.stage_ready(step.daa, &mut events)?;
        next.stage_assign(step.block, step.daa, view, &mut events)?;
        next.stage_close(step)?;
        Ok((next, events))
    }

    /// **Stage 2: one certified epoch output carried by the block `advance` moved to.** Transactional and per output: a host that
    /// drops a refused output and keeps the block (an invalid proof is never a reason to invalidate a block) calls this once per
    /// carried output and keeps the state of those that return `Ok`.
    pub fn accept_beacon(&self, proof: &BeaconProofV1, view: &impl ConsensusViewV1) -> Result<Self, PanelErrorV1> {
        let mut next = self.clone();
        next.stage_beacon(self.daa, proof, view)?;
        next.retain_needed_beacons();
        next.validate()?;
        Ok(next)
    }

    /// **Stage 3: the block's admitted claims, in the chain's own acceptance order.** Per claim, not per block: a claim the
    /// engine refuses (a duplicate work identity, a bound that is full) is returned with its reason and the others are admitted,
    /// because a production block that carries a refused claim stands — the host terminates that claim itself. At most
    /// `max_admissions_per_block` are admitted; the rest are refused `ResourceLimit`. The block is the cursor's.
    pub fn admit(&self, admissions: &[AdmittedClaimV1]) -> Result<(Self, Vec<(Hash64, PanelErrorV1)>), PanelErrorV1> {
        let mut next = self.clone();
        let mut refused = Vec::new();
        let (block, height, daa) = (next.tip, next.height, next.daa);
        let mut admitted = 0u32;
        for (index, claim) in admissions.iter().enumerate() {
            let outcome = if admitted >= next.policy.max_admissions_per_block {
                Err(PanelErrorV1::ResourceLimit)
            } else {
                // A refused admission mutates nothing (every check precedes the first write).
                next.stage_admit(block, height, daa, index as u32, claim)
            };
            match outcome {
                Ok(()) => admitted += 1,
                Err(why) => refused.push((claim.claim_id, why)),
            }
        }
        next.retain_needed_beacons();
        next.validate()?;
        Ok((next, refused))
    }

    fn stage_release(&mut self, daa: u64, view: &impl ConsensusViewV1, events: &mut PanelFoldEventsV1) -> Result<(), PanelErrorV1> {
        // A validated terminal outcome releases its reservation, never a local fetch failure.
        for (id, record) in &mut self.claims {
            if !record.phase.terminal() && view.terminal_claim(id) {
                record.phase = ClaimPhaseV3::Released { daa };
                events.released.push(*id);
            }
        }
        self.rebuild_reservations()
    }

    fn stage_seal(&mut self, step_daa: u64, view: &impl ConsensusViewV1, events: &mut PanelFoldEventsV1) -> Result<(), PanelErrorV1> {
        // Seal against the *parent* checkpoint, before this block's registrations/topups/objects.
        let (policy, network, ruleset, tip, height, daa) =
            (self.policy, self.network, self.ruleset, self.tip, self.height, self.daa);
        for record in self.claims.values_mut() {
            if record.phase != ClaimPhaseV3::PendingSeal {
                continue;
            }
            if step_daa > add(record.accepted_daa, policy.seal_wait_daa)? {
                Self::void(record, step_daa, NonFraudReasonV1::SealUnavailable, events);
                continue;
            }
            if height < add(record.accepted_height, policy.seal_depth_blocks)? {
                continue;
            }
            let epoch = add(daa / policy.beacon_period_daa, 1)?;
            let release = epoch.checked_mul(policy.beacon_period_daa).ok_or(PanelErrorV1::Overflow)?;
            // Strictly future even if the selected-chain DAA skips an epoch; the step itself cannot
            // simultaneously seal and supply already-known entropy.
            if step_daa >= release {
                Self::void(record, step_daa, NonFraudReasonV1::SealUnavailable, events);
                continue;
            }
            let mut candidates = view.candidates(&record.claim)?;
            if candidates.len() > policy.max_candidates as usize {
                return Err(PanelErrorV1::ResourceLimit);
            }
            candidates.sort_by_key(|s| s.bond);
            let mut bonds = BTreeSet::new();
            for s in &candidates {
                if !bonds.insert(s.bond)
                    || s.collateral < policy.min_collateral
                    || add(s.registered_daa, policy.bond_maturity_daa)? > daa
                    || s.bond == record.claim.producer
                    || s.operator == record.claim.producer_operator
                    || s.key == record.claim.producer_key
                    || s.roles == 0
                    || s.roles & !3 != 0
                    || s.capability_root == Hash64::default()
                    || s.readiness_root == Hash64::default()
                {
                    return Err(PanelErrorV1::InvalidSnapshot);
                }
            }
            let mut snapshot = PanelSnapshotV1 {
                root: Hash64::default(),
                checkpoint: tip,
                checkpoint_daa: daa,
                checkpoint_height: height,
                policy_id: policy.id(),
                class_id: record.claim.class_id,
                excluded_bond: record.claim.producer,
                excluded_operator: record.claim.producer_operator,
                excluded_key: record.claim.producer_key,
                candidates,
            };
            snapshot.root = snapshot.computed_root();
            // Signature and claim lookup id are deliberately absent. Acceptance order is assigned
            // by the fold, never supplied by a miner and never broken by a grindable claim hash.
            let id = digest(
                "misaka-palw/panel-v3/seal",
                &(
                    network,
                    ruleset,
                    record.claim.work_id,
                    (
                        record.claim.class_id,
                        record.claim.producer,
                        record.claim.producer_operator,
                        record.claim.producer_key,
                        record.claim.immutable_fields,
                        record.claim.required_exposure,
                    ),
                    record.accepted_block,
                    record.accepted_daa,
                    record.accepted_height,
                    record.acceptance_order,
                    record.occurrence_index,
                    tip,
                    height,
                    daa,
                    release,
                    epoch,
                    snapshot.root,
                ),
            );
            record.seal = Some(ClaimSealV1 {
                id,
                accepted_block: record.accepted_block,
                accepted_daa: record.accepted_daa,
                accepted_height: record.accepted_height,
                acceptance_order: record.acceptance_order,
                occurrence_index: record.occurrence_index,
                checkpoint: tip,
                checkpoint_daa: daa,
                checkpoint_height: height,
                anchor_slot: release,
                beacon_epoch: epoch,
            });
            record.snapshot = Some(snapshot);
            record.phase = ClaimPhaseV3::Sealed;
        }
        Ok(())
    }

    /// Epoch evidence is bounded, checked independently of the Panel, and immutable once stored.
    /// Two proofs for the same output are duplicates, not another opportunity.
    fn stage_beacon(&mut self, step_daa: u64, proof: &BeaconProofV1, view: &impl ConsensusViewV1) -> Result<(), PanelErrorV1> {
        if proof.proof.len() > self.policy.max_beacon_proof_bytes as usize {
            return Err(PanelErrorV1::ResourceLimit);
        }
        let release = proof.epoch.checked_mul(self.policy.beacon_period_daa).ok_or(PanelErrorV1::Overflow)?;
        let deadline = add(release, self.policy.beacon_wait_daa)?;
        if step_daa < release
            || step_daa > deadline
            || !self.claims.values().any(|r| !r.phase.terminal() && r.seal.as_ref().is_some_and(|s| s.beacon_epoch == proof.epoch))
        {
            return Err(PanelErrorV1::InvalidBeacon);
        }
        view.verify_beacon(
            &BeaconRequestV1 {
                network: self.network,
                ruleset: self.ruleset,
                scheme: self.policy.beacon_scheme,
                epoch: proof.epoch,
                release_daa: release,
                deadline_daa: deadline,
            },
            proof,
        )?;
        if let Some(old) = self.beacons.get(&proof.epoch) {
            if *old != proof.output {
                return Err(PanelErrorV1::BeaconEquivocation);
            }
        } else {
            self.beacons.insert(proof.epoch, proof.output);
        }
        Ok(())
    }

    fn stage_ready(&mut self, step_daa: u64, events: &mut PanelFoldEventsV1) -> Result<(), PanelErrorV1> {
        let (policy, network, ruleset) = (self.policy, self.network, self.ruleset);
        for record in self.claims.values_mut() {
            if record.phase != ClaimPhaseV3::Sealed {
                continue;
            }
            let seal = record.seal.as_ref().ok_or(PanelErrorV1::InvalidCarriage)?;
            let ready = add(seal.anchor_slot, policy.beacon_wait_daa)?;
            if step_daa <= ready {
                continue;
            } // Entire contribution window closes first.
            if let Some(output) = self.beacons.get(&seal.beacon_epoch) {
                let snapshot = record.snapshot.as_ref().ok_or(PanelErrorV1::InvalidCarriage)?;
                let seed = panel_seed_v3(network, ruleset, seal, snapshot, *output);
                let beacon_id = digest(
                    "misaka-palw/panel-v3/beacon",
                    &(network, ruleset, policy.beacon_scheme, seal.beacon_epoch, output),
                );
                record.phase = ClaimPhaseV3::EntropyReady {
                    ready_daa: ready,
                    assignment_point: add(ready, policy.assignment_delay_daa)?,
                    seed,
                    beacon_id,
                };
            } else {
                Self::void(record, step_daa, NonFraudReasonV1::BeaconUnavailable, events);
            }
        }
        Ok(())
    }

    fn stage_assign(
        &mut self,
        block: Hash64,
        step_daa: u64,
        view: &impl ConsensusViewV1,
        events: &mut PanelFoldEventsV1,
    ) -> Result<(), PanelErrorV1> {
        // Timeouts and initial assignments share one canonical bounded queue. Producer cannot choose
        // a subset. Retry inputs use the original seed and never acquire timeout-block entropy.
        let policy = self.policy;
        let mut due = Vec::new();
        for (id, r) in &self.claims {
            let is_due = match &r.phase {
                ClaimPhaseV3::EntropyReady { assignment_point, .. } => step_daa >= *assignment_point,
                ClaimPhaseV3::Bound(b) => match view.receipt_clock(id) {
                    Some(ReceiptClockV1::Paused) => false,
                    Some(ReceiptClockV1::Running { bound_daa }) => step_daa > add(bound_daa, policy.receipt_window_daa)?,
                    None => step_daa > add(b.bound_daa, policy.receipt_window_daa)?,
                },
                _ => false,
            };
            if is_due {
                due.push((r.seal.as_ref().unwrap().beacon_epoch, r.acceptance_order, r.occurrence_index, *id));
            }
        }
        due.sort();
        for (_, _, _, id) in due.into_iter().take(policy.max_assignments_per_block as usize) {
            let mut r = self.claims.remove(&id).unwrap();
            let (seed, beacon_id, ready, point, retry) = match &r.phase {
                ClaimPhaseV3::EntropyReady { seed, beacon_id, ready_daa, assignment_point } => {
                    (*seed, *beacon_id, *ready_daa, *assignment_point, 0)
                }
                ClaimPhaseV3::Bound(b) => (b.panel_seed_v3, b.beacon_id, b.entropy_ready_daa, b.assignment_point, b.retry_index + 1),
                _ => unreachable!(),
            };
            self.rebuild_reservations()?; // Remove this claim's previous reservation atomically.
            if retry > policy.max_retries {
                Self::void(&mut r, step_daa, NonFraudReasonV1::PanelUnavailable, events);
            } else {
                let snapshot = r.snapshot.as_ref().unwrap();
                let mut used = r.used_operators.clone();
                let mut seats = Vec::new();
                for (role, needed) in [
                    (OUTSIDER_ROLE_V1, policy.outsider_seats),
                    (CLASS_ROLE_V1, policy.seat_count - policy.outsider_seats),
                ] {
                    let mut selected = 0;
                    for bond in seat_order_v1(snapshot, seed, retry, role, &used)? {
                        if selected == needed {
                            break;
                        }
                        let candidate = snapshot.candidates.iter().find(|s| s.bond == bond).unwrap();
                        if used.contains(&candidate.operator) {
                            continue;
                        }
                        let free =
                            view.available_collateral(&bond).min(candidate.collateral as u128).saturating_sub(self.reserved(&bond));
                        if free < r.claim.required_exposure as u128 {
                            continue;
                        }
                        seats.push(bond);
                        used.push(candidate.operator);
                        selected += 1;
                    }
                    if selected < needed {
                        break;
                    }
                }
                if seats.len() != policy.seat_count as usize {
                    Self::void(&mut r, step_daa, NonFraudReasonV1::NoCapablePanel, events);
                } else {
                    let binding = PanelBoundV3 {
                        claim_seal_id: r.seal.as_ref().unwrap().id,
                        panel_snapshot_root: snapshot.root,
                        beacon_id,
                        panel_seed_v3: seed,
                        entropy_ready_daa: ready,
                        assignment_point: point,
                        binding_block: block,
                        bound_daa: step_daa,
                        retry_index: retry,
                        seats,
                        exposure: r.claim.required_exposure,
                    };
                    r.used_operators = used;
                    r.binding_history.push(binding.clone());
                    events.bindings.insert(id, binding.clone());
                    r.phase = ClaimPhaseV3::Bound(binding);
                }
            }
            self.claims.insert(id, r);
            self.rebuild_reservations()?;
        }
        Ok(())
    }

    fn stage_admit(
        &mut self,
        block: Hash64,
        height: u64,
        daa: u64,
        index: u32,
        claim: &AdmittedClaimV1,
    ) -> Result<(), PanelErrorV1> {
        if self.claims.contains_key(&claim.claim_id) || self.work_ids.contains(&claim.work_id) {
            return Err(PanelErrorV1::DuplicateClaim);
        }
        if claim.required_exposure == 0 || claim.immutable_fields == Hash64::default() || claim.work_id == Hash64::default() {
            return Err(PanelErrorV1::InvalidSnapshot);
        }
        let pending = self.claims.values().filter(|r| !r.phase.terminal());
        if self.claims.len() >= self.policy.max_tracked_claims as usize
            || pending.clone().count() >= self.policy.max_pending as usize
            || pending.filter(|r| r.claim.producer == claim.producer).count() >= self.policy.max_pending_per_bond as usize
        {
            return Err(PanelErrorV1::ResourceLimit);
        }
        let order = self.next_order;
        self.next_order = add(order, 1)?;
        self.work_ids.insert(claim.work_id);
        self.claims.insert(
            claim.claim_id,
            ClaimRecordV3 {
                claim: claim.clone(),
                accepted_block: block,
                accepted_daa: daa,
                accepted_height: height,
                acceptance_order: order,
                occurrence_index: index,
                seal: None,
                snapshot: None,
                used_operators: Vec::new(),
                binding_history: Vec::new(),
                phase: ClaimPhaseV3::PendingSeal,
            },
        );
        Ok(())
    }

    fn stage_close(&mut self, step: &SelectedChainStepV1) -> Result<(), PanelErrorV1> {
        self.tip = step.block;
        self.height = step.height;
        self.daa = step.daa;
        self.retain_needed_beacons();
        self.validate()
    }

    /// Beacons are retained only while a nonterminal claim can need their output.
    fn retain_needed_beacons(&mut self) {
        let epochs: BTreeSet<_> =
            self.claims.values().filter(|r| !r.phase.terminal()).filter_map(|r| r.seal.as_ref().map(|s| s.beacon_epoch)).collect();
        self.beacons.retain(|e, _| epochs.contains(e));
    }

    fn void(r: &mut ClaimRecordV3, daa: u64, reason: NonFraudReasonV1, e: &mut PanelFoldEventsV1) {
        r.phase = ClaimPhaseV3::Voided { daa, reason };
        e.non_fraud_voids.insert(r.claim.claim_id, reason);
    }

    fn rebuild_reservations(&mut self) -> Result<(), PanelErrorV1> {
        self.reservations.clear();
        for r in self.claims.values() {
            if let ClaimPhaseV3::Bound(b) = &r.phase {
                for seat in &b.seats {
                    let amount = self.reservations.entry(*seat).or_default();
                    *amount = amount.checked_add(b.exposure as u128).ok_or(PanelErrorV1::Overflow)?;
                }
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), PanelErrorV1> {
        self.policy.validate()?;
        if self.claims.len() > self.policy.max_tracked_claims as usize
            || self.work_ids.len() != self.claims.len()
            || self.claims.values().filter(|r| !r.phase.terminal()).count() > self.policy.max_pending as usize
            || self.beacons.len() > self.policy.max_pending as usize
        {
            return Err(PanelErrorV1::InvalidCarriage);
        }
        let mut orders = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for (id, r) in &self.claims {
            if *id != r.claim.claim_id
                || !ids.insert(r.claim.work_id)
                || !self.work_ids.contains(&r.claim.work_id)
                || !orders.insert(r.acceptance_order)
                || r.acceptance_order >= self.next_order
                || r.binding_history.len() > self.policy.max_retries as usize + 1
            {
                return Err(PanelErrorV1::InvalidCarriage);
            }
            if r.accepted_height > self.height
                || r.accepted_daa > self.daa
                || r.claim.required_exposure == 0
                || r.seal.is_some() != r.snapshot.is_some()
            {
                return Err(PanelErrorV1::InvalidCarriage);
            }
            match &r.phase {
                ClaimPhaseV3::PendingSeal if r.seal.is_some() || !r.binding_history.is_empty() => {
                    return Err(PanelErrorV1::InvalidCarriage);
                }
                ClaimPhaseV3::Sealed | ClaimPhaseV3::EntropyReady { .. } | ClaimPhaseV3::Bound(_) if r.seal.is_none() => {
                    return Err(PanelErrorV1::InvalidCarriage);
                }
                _ => {}
            }
            if let Some(snapshot) = &r.snapshot {
                let seal = r.seal.as_ref().ok_or(PanelErrorV1::InvalidCarriage)?;
                if snapshot.policy_id != self.policy.id()
                    || snapshot.root != snapshot.computed_root()
                    || snapshot.candidates.len() > self.policy.max_candidates as usize
                    || snapshot.class_id != r.claim.class_id
                    || snapshot.excluded_bond != r.claim.producer
                    || snapshot.excluded_operator != r.claim.producer_operator
                    || snapshot.excluded_key != r.claim.producer_key
                    || snapshot.checkpoint != seal.checkpoint
                    || snapshot.checkpoint_daa != seal.checkpoint_daa
                    || snapshot.checkpoint_height != seal.checkpoint_height
                    || seal.accepted_block != r.accepted_block
                    || seal.accepted_height != r.accepted_height
                    || seal.accepted_daa != r.accepted_daa
                    || seal.acceptance_order != r.acceptance_order
                    || seal.occurrence_index != r.occurrence_index
                    || seal.checkpoint_height != add(r.accepted_height, self.policy.seal_depth_blocks)?
                    || seal.anchor_slot
                        != seal.beacon_epoch.checked_mul(self.policy.beacon_period_daa).ok_or(PanelErrorV1::Overflow)?
                    || seal.checkpoint_daa >= seal.anchor_slot
                {
                    return Err(PanelErrorV1::InvalidCarriage);
                }
                let mut previous = None;
                for s in &snapshot.candidates {
                    if previous.is_some_and(|p| p >= s.bond)
                        || s.collateral < self.policy.min_collateral
                        || add(s.registered_daa, self.policy.bond_maturity_daa)? > snapshot.checkpoint_daa
                        || s.bond == snapshot.excluded_bond
                        || s.operator == snapshot.excluded_operator
                        || s.key == snapshot.excluded_key
                        || s.roles == 0
                        || s.roles & !3 != 0
                        || s.capability_root == Hash64::default()
                        || s.readiness_root == Hash64::default()
                    {
                        return Err(PanelErrorV1::InvalidCarriage);
                    }
                    previous = Some(s.bond);
                }
                let mut used = Vec::new();
                for (index, binding) in r.binding_history.iter().enumerate() {
                    if binding.retry_index as usize != index
                        || binding.seats.len() != self.policy.seat_count as usize
                        || binding.claim_seal_id != seal.id
                        || binding.panel_snapshot_root != snapshot.root
                        || binding.exposure != r.claim.required_exposure
                        || binding.entropy_ready_daa != add(seal.anchor_slot, self.policy.beacon_wait_daa)?
                        || binding.assignment_point != add(binding.entropy_ready_daa, self.policy.assignment_delay_daa)?
                        || binding.bound_daa < binding.assignment_point
                        || binding.bound_daa > self.daa
                    {
                        return Err(PanelErrorV1::InvalidCarriage);
                    }
                    for (seat_index, bond) in binding.seats.iter().enumerate() {
                        let s = snapshot.candidates.iter().find(|s| &s.bond == bond).ok_or(PanelErrorV1::InvalidCarriage)?;
                        let role = if seat_index < self.policy.outsider_seats as usize { OUTSIDER_ROLE_V1 } else { CLASS_ROLE_V1 };
                        if used.contains(&s.operator) || s.roles & role == 0 {
                            return Err(PanelErrorV1::InvalidCarriage);
                        }
                        used.push(s.operator);
                    }
                }
                if used != r.used_operators {
                    return Err(PanelErrorV1::InvalidCarriage);
                }
                if let ClaimPhaseV3::Bound(b) = &r.phase {
                    if r.binding_history.last() != Some(b) {
                        return Err(PanelErrorV1::InvalidCarriage);
                    }
                }
            }
        }
        let mut copy = self.clone();
        copy.rebuild_reservations()?;
        if copy.reservations != self.reservations {
            return Err(PanelErrorV1::InvalidCarriage);
        }
        Ok(())
    }
}

fn serialize_reservations<S: serde::Serializer>(map: &BTreeMap<BondIdV1, u128>, serializer: S) -> Result<S::Ok, S::Error> {
    // Outpoint keys are structured; amounts remain exact for JavaScript observers too.
    Serialize::serialize(&map.iter().map(|(bond, amount)| (bond, amount.to_string())).collect::<Vec<_>>(), serializer)
}
