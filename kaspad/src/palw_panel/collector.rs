//! Serial funding settlement, readiness, licence and supplementary submission.
//! Read-only receipt facts are separate from the mutable carrier and filing books.
//! No verification pool, replay task or material cache can be mutated here.
use super::*;

pub(super) struct CollectorReadV1<'a> {
    pub(super) receipt_pool_v2: &'a crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV2>,
    pub(super) receipt_pool_v3: &'a crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV3>,
    pub(super) receipt_facts: &'a crate::palw_receipt_pool::ReceiptChainFactsV1,
    pub(super) licence_spread: &'a std::collections::hash_map::RandomState,
    pub(super) own_receipts: &'a HashMap<Hash64, (PalwSeatReceiptV2, u64)>,
    pub(super) own_receipt_class: &'a HashMap<Hash64, Hash64>,
    pub(super) seat_r: bool,
    pub(super) vertex_fence: Option<u64>,
}

pub(super) struct CollectorRefsV1<'a> {
    pub(super) shard_books: &'a mut tir_shard::PalwTirShardBooksV1,
    pub(super) submitted: &'a mut HashMap<Hash64, u64>,
    pub(super) submit_attempts: &'a mut HashMap<Hash64, u32>,
    pub(super) v1_sent: &'a mut HashMap<Hash64, u32>,
    pub(super) supplementary_submitted: &'a mut HashMap<Hash64, u64>,
    pub(super) vertex_book: &'a mut vertex::PalwVertexBookV1,
    pub(super) trap_book: &'a mut mesh::PalwTrapBookV1,
    pub(super) supplementary_v3_submitted: &'a mut HashMap<Hash64, u64>,
    pub(super) supplementary_idle: &'a mut HashMap<Hash64, (u64, u64)>,
    pub(super) licence_idle: &'a mut HashMap<Hash64, (u64, u64)>,
    pub(super) court_outbox: &'a mut CourtOutboxV1,
    pub(super) challenged: &'a mut HashSet<Hash64>,
    pub(super) carrier_chain: &'a mut CarrierChainV1,
    pub(super) registration: &'a mut registration::ClassRegistrationV1,
    pub(super) readiness_lane: &'a mut PalwReadinessLaneV1,
    pub(super) receipt_lanes: &'a mut BTreeMap<Hash64, PalwReadinessLaneV1>,
    pub(super) readiness_waiting: &'a mut bool,
    pub(super) last_lane: &'a mut Option<PalwCarrierLaneV1>,
    pub(super) held_before: &'a mut bool,
    pub(super) canonical_last_daa: &'a mut u64,
    pub(super) improve: &'a mut improve::PalwImproveLoopV1,
}
impl PalwPanelService {
    pub(super) async fn collector_tick_v1(
        self: &Arc<Self>,
        tick: tick_inputs::PanelTickReadV1<'_>,
        tick_guard: &mut PalwTickGuardV1,
        read: CollectorReadV1<'_>,
        books: CollectorRefsV1<'_>,
    ) {
        let session = tick.session();
        let bond_key = tick.bond();
        let bond = bond_key.0;
        let network_domain = tick.domain();
        let current_daa = tick.daa();
        let CollectorReadV1 {
            receipt_pool_v2,
            receipt_pool_v3,
            receipt_facts,
            licence_spread,
            own_receipts,
            own_receipt_class,
            seat_r,
            vertex_fence,
        } = read;
        let CollectorRefsV1 {
            mut shard_books,
            submitted,
            submit_attempts,
            v1_sent,
            supplementary_submitted,
            vertex_book,
            trap_book,
            supplementary_v3_submitted,
            supplementary_idle,
            licence_idle,
            mut court_outbox,
            mut challenged,
            carrier_chain,
            registration,
            mut readiness_lane,
            receipt_lanes,
            readiness_waiting,
            last_lane,
            held_before,
            canonical_last_daa,
            mut improve,
        } = books;
        if self.config.fee_outpoint.is_some() {
            // Resolve the fee UTXO ONCE per tick and then CHAIN it: the change of a carrier
            // this tick already submitted is not in the virtual UTXO set — it is in our own
            // mempool — so re-resolving per claim hands every claim after the first the same
            // spent outpoint, and the mempool refuses it as a double spend. Chaining is also
            // what lets the submitter keep up: one carrier per tick cannot track a lane that
            // mints a claim per block.
            // **The mempool chain survives the tick.**
            //
            // Chaining the change within a tick was not enough: `resolve_fee_funding` reads the
            // VIRTUAL utxo set, so a carrier submitted last tick is invisible to it until it is
            // mined — and the fallback is the CONFIGURED outpoint, which is still unspent in
            // virtual because only the mempool has spent it. Every tick therefore rebuilt a
            // transaction spending an output its own predecessor had already taken, and the
            // mempool refused it as a double spend. Measured on the drill: 53 frauds detected,
            // 115 courts intended, `already spent by transaction … in the mempool`, zero
            // carried.
            //
            // So the chained entry is held across ticks and only re-resolved when we have none
            // — and dropped the moment a submission is refused, because that is the signal that
            // the chain we were extending is not one the mempool will accept.
            if carrier_chain.funding().is_none() {
                carrier_chain.set_funding(self.resolve_fee_funding(&session, palw_fee_funding_floor_v1()).await);
                carrier_chain.set_inflight(0);
            } else if let Some((tip, _)) = carrier_chain.funding().clone() {
                if session.get_virtual_utxo_entry(tip).is_some() {
                    // The tip of our own chain is in the UTXO set, so every carrier behind it
                    // was mined. Nothing is in flight and the budget is whole again.
                    carrier_chain.set_inflight(0);
                    // **P2-6 (F13) on the drain path** (the P2-6 review, MEDIUM): a tip too small
                    // to fund a carrier is exhausted, and was offered every tick while every
                    // carrier failed on it and the matured rewards were never looked at. With
                    // nothing in flight, the funding is resolved afresh — the largest usable
                    // output that pays, never the residue (`palw_fee_funding_floor_v1`).
                    if palw_fee_chain_drained_v1(carrier_chain.funding().as_ref()) {
                        info!(
                            "[{PALW_PANEL}] the carrier chain's change {}:{} holds under {} sompi — too little for a \
                             carrier; resolving funding afresh",
                            tip.transaction_id,
                            tip.index,
                            palw_fee_funding_floor_v1()
                        );
                        carrier_chain.set_funding(self.resolve_fee_funding(&session, palw_fee_funding_floor_v1()).await);
                    }
                } else if !self
                    .flow_context
                    .mining_manager()
                    .clone()
                    .has_transaction(tip.transaction_id, kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
                    .await
                {
                    // **The carrier that would create this change is neither mined nor pending,
                    // so it is gone.** Relay drops orphans silently and mempools evict; without
                    // this the panel waits on an output no one will ever produce, holding at the
                    // in-flight cap forever because the cap only clears when the tip confirms.
                    // Dropping the chain sends the next tick back to `resolve_fee_funding`,
                    // which reads what the chain and our own mempool actually say.
                    warn!(
                        "[{PALW_PANEL}] carrier {} was neither mined nor kept — re-resolving funding from the chain",
                        tip.transaction_id
                    );
                    carrier_chain.set_funding(None);
                    carrier_chain.set_inflight(0);
                }
            }
            // V07: the readiness lane's funding, read as the main chain's tip is (`settle`).
            if let Some((lane_tip, _)) = readiness_lane.funding.clone() {
                let pool = self.flow_context.mining_manager().clone();
                let confirmed = session.get_virtual_utxo_entry(lane_tip).is_some();
                let spent_in_pool = confirmed && pool.outpoint_is_spent_in_mempool(&lane_tip);
                let kept = confirmed
                    || pool
                        .has_transaction(lane_tip.transaction_id, kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly)
                        .await;
                readiness_lane.settle(confirmed, spent_in_pool, kept);
            } else {
                readiness_lane.settle(false, false, false);
            }
            // RFC-0018 (iv): every receipt lane's funding, read the same way.
            for lane in receipt_lanes.values_mut() {
                if let Some((lane_tip, _)) = lane.funding.clone() {
                    let pool = self.flow_context.mining_manager().clone();
                    let confirmed = session.get_virtual_utxo_entry(lane_tip).is_some();
                    let spent_in_pool = confirmed && pool.outpoint_is_spent_in_mempool(&lane_tip);
                    let kept = confirmed
                        || pool
                            .has_transaction(
                                lane_tip.transaction_id,
                                kaspa_mining::model::tx_query::TransactionQuery::TransactionsOnly,
                            )
                            .await;
                    lane.settle(confirmed, spent_in_pool, kept);
                } else {
                    lane.settle(false, false, false);
                }
            }
            let mut held = carrier_chain.inflight() >= MAX_INFLIGHT_CARRIERS;
            // **Our own tip carrier, unconfirmed since it was sent, and for how long** (the pre-t12
            // drill of 2026-09-25). Held means the checks above found it neither mined nor gone:
            // it is in this node's mempool, and `block_daa_score` on its change is the DAA it was
            // sent at (every site writes `current_daa` there). A carrier the fold stopped taking is
            // evicted only at a TEMPLATE build (`palw_mempool_h1_carrier_refusal`), and a seat that
            // mines nothing builds none — m7's duplicate `ObjectiveOffence` (18:24:32) sat in its
            // own mempool for 56 minutes, peers had refused it, and the one carrier slot it held
            // kept every possession proof behind it until the 8k class went HELD. A proof that is
            // due replaces such a carrier (`palw_readiness_replaces_carrier_v1`).
            let stuck_tip: Option<(kaspa_consensus_core::tx::TransactionId, u64)> = if held {
                carrier_chain
                    .funding()
                    .as_ref()
                    .map(|(tip, entry)| (tip.transaction_id, current_daa.saturating_sub(entry.block_daa_score)))
            } else {
                None
            };
            let mut tip_replaced = false;
            // Once per transition, at info: a held submitter stops EVERYTHING it carries —
            // receipts, quorums, canonical claims — and used to say so only at trace level,
            // which is silence. Seen on the Qwen3.5-2B add-model rehearsal: the canonical seat
            // went quiet for 24 minutes at the cap (its chain is mined one carrier per block,
            // and on a relay-less devnet only by itself) with nothing in the log to read.
            if held != (*held_before) {
                if held {
                    info!(
                        "[{PALW_PANEL}] holding every carrier: {inflight} of ours are unconfirmed (cap {MAX_INFLIGHT_CARRIERS}); a \
                         chained carrier is mined only after its parent, so this clears one block at a time",
                        inflight = carrier_chain.inflight()
                    );
                } else {
                    info!("[{PALW_PANEL}] carrier chain confirmed; submitting again");
                }
                (*held_before) = held;
            }
            let mut funding = if held {
                // Back-pressure, not loss. Keeping the objects pending means the next tick
                // re-offers them in priority order (court moves first), rather than building
                // carriers the network will drop before anyone reads them.
                None
            } else {
                carrier_chain.funding().clone()
            };
            if held {
                // Not the same condition as "no fee UTXO", and it must not print as one: this
                // panel is waiting for its own chain to confirm, which is the system working.
                trace!(
                    "[{PALW_PANEL}] holding: {inflight} carriers unconfirmed (cap {MAX_INFLIGHT_CARRIERS})",
                    inflight = carrier_chain.inflight()
                );
            } else if funding.is_none() && receipt_pool_v2.claim_ids().iter().any(|c| !submitted.contains_key(c)) {
                // Once per tick, not once per claim: a pending carrier keeps every quorum
                // unfundable until it is mined, and that used to be tens of thousands of
                // identical lines an hour.
                warn!(
                    "[{PALW_PANEL}] a quorum stands but no fee UTXO resolves — a carrier may still be in flight; else \
                     --palw-fee-outpoint ({}) is unfunded or already spent",
                    self.config.fee_outpoint.as_deref().unwrap_or("unset")
                );
            }
            // **V04: a waiting proof holds the receipts for ITS tick, never longer** (the pre-t12
            // drill of 2026-09-25). `readiness_waiting` was set where a proof found no slot and
            // cleared only where one was carried, so a proof that stopped being due while it
            // waited — its last copy landed and the row went fresh — left it set until the NEXT
            // proof was carried, half a row's age later: m6 waited at 19:21:22 and then carried 26
            // reporter carriers and not one licence, supplementary set or own receipt until
            // 19:46:38. Last tick's wait still asks for the duties again at once (the thirty-second
            // read throttle); from here on the flag is this tick's own, and a proof still due takes
            // the slot at its own site, ahead of the licences, exactly as before.
            let proof_waited_last_tick = std::mem::replace(&mut (*readiness_waiting), false);
            // **P2-6: the priority lane first — the court's moves, data-availability accusations
            // and answers, convictions — except on the licences' turn** (the slot right after a
            // priority carrier, `palw_carrier_licence_turn_v1`), when it goes behind the
            // collector: one carrier is in flight per panel, and a queue that always holds a
            // priority object would otherwise take every slot while claims waiting on their
            // licence void at their receipt deadline. Every site below asks the one gate
            // (`PalwCarrierSlotsV1::offers`: the slot free, and its turn), in `TICK_ORDER`.
            //
            // **M1 (the 2026-09-25 model-registry review): past R-core+ the possession proofs are
            // read before the court queue**, so one whose row is about to lapse takes the slot
            // ahead of it (`ReadinessEscalated`: one a tick, never two slots running). Elsewhere
            // they are read at the Own site, where they always were.
            let mut readiness = if self.consensus_config.params.palw_rcore_plus_active_at(current_daa) {
                Some(self.readiness_duties_for_tick(&session, current_daa, proof_waited_last_tick).await)
            } else {
                None
            };
            let mut slots = PalwCarrierSlotsV1::new(*last_lane);
            // **Any duty due takes the slot of our own STUCK carrier** (V01's panel side, the pre-t12
            // sweep of 2026-09-25). Past twice the landing margin the carrier holding the slot is
            // not landing — dropped in relay, kept by no miner, refused by every template — and
            // until now only a possession proof could take the slot back, so a stuck court move,
            // licence or filing held every other duty until the Low-priority expiry. The slot is
            // opened on the stuck carrier's own input (`open_stuck_carrier_v1`): the sites run in
            // their order, the first carrier sent pays above the stuck one and replaces it
            // (`submit_carrier_v1`), and the replaced object is its lane's to re-offer. A tick that
            // sends nothing leaves the stuck carrier holding the slot, as before.
            let stuck_opened = match stuck_tip {
                Some((stuck, late)) if palw_stuck_carrier_opens_slot_v1(late) => {
                    match self.open_stuck_carrier_v1(&session, stuck, late).await {
                        Some(opened) => {
                            funding = Some(opened);
                            carrier_chain.set_inflight(0);
                            held = false;
                            true
                        }
                        None => false,
                    }
                }
                _ => false,
            };
            // **An escalated proof behind our own late carrier replaces it** (the pre-t12 drill of
            // 2026-09-25): past the landing margin the carrier holding the slot has not landed, and
            // M1 already ranks a proof whose row is about to lapse above whatever took the slot — so
            // the proof spends that carrier's input, replaces it, and the slot is the proof's. The
            // alternation M1 keeps (never two escalated slots running) shares LANDING slots; a
            // carrier that does not land gives nobody a slot, so it is not asked here.
            let replacing = stuck_tip.filter(|(_, late)| {
                !stuck_opened
                    && palw_readiness_replaces_carrier_v1(true, *late)
                    && readiness
                        .as_ref()
                        .is_some_and(|duties| crate::palw_readiness_escalation::palw_escalated_readiness_pick_v1(duties).is_some())
            });
            if replacing.is_some() {
                // The late carrier leaves with its replacement: the site is entered with the slot free.
                carrier_chain.set_inflight(0);
            }
            slots.at(PalwCarrierSiteV1::ReadinessEscalated, carrier_chain.inflight());
            if let Some((stuck, late)) = replacing
                && let Some(duties) = readiness.as_mut()
                && let Some(at) = crate::palw_readiness_escalation::palw_escalated_readiness_pick_v1(duties)
            {
                let duty = duties.remove(at);
                if self
                    .replace_late_carrier_v1(
                        &session,
                        &duty,
                        stuck,
                        late,
                        current_daa,
                        &mut carrier_chain.book_with_funding(&mut funding),
                    )
                    .await
                {
                    (*readiness_waiting) = false;
                    held = false;
                    tip_replaced = true;
                } else {
                    // The late carrier still holds the slot: nothing went out at this site.
                    carrier_chain.set_inflight(MAX_INFLIGHT_CARRIERS);
                    slots.rewind_v1(carrier_chain.inflight());
                    // V07: a declined replacement is a proof the main slot cannot take this tick — our
                    // court move or data-availability answer keeps its turn until LOST
                    // (`palw_carrier_keeps_its_turn_v1`), a carrier that moves money or registers is
                    // never replaced — so the lane is offered it, as the branch below offers it. The
                    // duty left the tick's list above and the Own site never sees it: without this the
                    // ordinary proofs rode the lane while the one about to lapse waited (the int-4
                    // audit of 2026-09-26, the n1 x n2 composition).
                    let main: Vec<TransactionOutpoint> = funding
                        .iter()
                        .chain(carrier_chain.funding().iter())
                        .chain(receipt_lanes.values().filter_map(|l| l.funding.as_ref()))
                        .map(|(outpoint, _)| *outpoint)
                        .collect();
                    (*readiness_waiting) =
                        !self.carry_on_readiness_lane_v1(&session, &duty, current_daa, &mut readiness_lane, &main).await;
                }
            } else if slots.offers(PalwCarrierSiteV1::ReadinessEscalated, carrier_chain.inflight())
                && funding.is_some()
                && let Some(duties) = readiness.as_mut()
                && let Some(at) = crate::palw_readiness_escalation::palw_escalated_readiness_pick_v1(duties)
            {
                let duty = duties.remove(at);
                if self
                    .carry_readiness_proof_v1(&session, &duty, current_daa, &mut carrier_chain.book_with_funding(&mut funding))
                    .await
                {
                    (*readiness_waiting) = false;
                }
            } else if replacing.is_none()
                && let Some(duties) = readiness.as_mut()
                && let Some(at) = crate::palw_readiness_escalation::palw_escalated_readiness_pick_v1(duties)
            {
                // V07: the main slot cannot take the escalated proof this tick — the lane can.
                let main: Vec<TransactionOutpoint> = funding
                    .iter()
                    .chain(carrier_chain.funding().iter())
                    .chain(receipt_lanes.values().filter_map(|l| l.funding.as_ref()))
                    .map(|(outpoint, _)| *outpoint)
                    .collect();
                if self.carry_on_readiness_lane_v1(&session, &duties[at], current_daa, &mut readiness_lane, &main).await {
                    duties.remove(at);
                    (*readiness_waiting) = false;
                }
            }
            if replacing.is_some() && !tip_replaced {
                carrier_chain.set_inflight(carrier_chain.inflight().max(MAX_INFLIGHT_CARRIERS));
            }
            slots.at(PalwCarrierSiteV1::PriorityFirst, carrier_chain.inflight());
            if slots.offers(PalwCarrierSiteV1::PriorityFirst, carrier_chain.inflight()) {
                self.carry_priority_v1(
                    &session,
                    current_daa,
                    &mut court_outbox,
                    &mut carrier_chain.book_with_funding(&mut funding),
                    &mut challenged,
                )
                .await;
            }
            slots.at(PalwCarrierSiteV1::Own, carrier_chain.inflight());
            // **The class registration, ahead of every other ordinary carrier and only once.**
            //
            // A class that is not registered mines nothing, so this is the one object whose
            // absence costs the whole lane rather than one claim. It is offered first for the
            // same reason a court rung is: everything behind it can wait a tick, and it
            // cannot — a producer with a worker and no class is a node doing nothing.
            // **The work queue** (ADR-0074 Decision 1): with nobody asking, run the network's
            // own job and commit it as a canonical free-prompt claim, one per interval per
            // bond, funded like any other carrier. A failed build is retried next interval,
            // not next tick — an inference is not something to spin on.
            if slots.offers(PalwCarrierSiteV1::Own, carrier_chain.inflight())
                && self.config.canonical_claims
                && current_daa >= (*canonical_last_daa).saturating_add(self.config.canonical_interval_daa)
                // V01's panel side: a claim's commitment is priced and paid by its own builder, so
                // it never replaces our stuck carrier — it waits for the next tick.
                && !stuck_opened
                && let Some((funding_outpoint, funding_entry)) = funding.clone()
            {
                (*canonical_last_daa) = current_daa;
                match self.build_canonical_claim(&session, network_domain, bond, current_daa, funding_outpoint, &funding_entry).await {
                    Ok((tx, claim_id, material, _answer_wire_and_signing_lease, routing_work)) => {
                        if let Err(why) = crate::palw_native_routing_work_v2::refresh_v2(&session, routing_work.as_ref()) {
                            warn!("[{PALW_PANEL}] canonical native routing publication deferred: {why}");
                        } else {
                            // The accounting wire/signing reservation follows the actual outer TX through submission.
                            let prepared_carrier = PreparedCarrierV1::new(tx, 0, current_daa);
                            let txid = prepared_carrier.id();
                            match self.submit_prepared_direct_v1(&session, prepared_carrier).await {
                                Ok(sent_carrier) => {
                                    info!("[{PALW_PANEL}] committed canonical claim {claim_id} in tx {txid}");
                                    // The answer beside the question, retained under the claim id
                                    // (the resolver serves it) and broadcast to the seats.
                                    self.retain_own_material(&claim_id, &material);
                                    // DRILL (ADR-0111): the answer envelope, never the capture — the
                                    // width at which no seat holds one and every seat judges by
                                    // intervals, so a named leaf's evidence must come from here.
                                    let served = if self.config.drill_answer_only {
                                        self.derive_answer_envelope_v1(&material).unwrap_or(material)
                                    } else {
                                        material
                                    };
                                    self.flow_context.broadcast_palw_material(claim_id, served).await;
                                    sent_carrier.record_change(
                                        &mut carrier_chain.book_with_funding(&mut funding),
                                        CarrierInFlightUpdateV1::Increment,
                                        |next| self.persist_fee_outpoint(next),
                                    );
                                }
                                Err(e) => {
                                    warn!("[{PALW_PANEL}] the mempool refused the canonical claim: {e}");
                                    funding = None;
                                }
                            }
                        }
                    }
                    Err(e) => warn!("[{PALW_PANEL}] cannot build a canonical claim: {e}"),
                }
            }
            // **RFC-0004 (A10): an evaluation claim the executor finished**, carried beside the
            // canonical claim at the same site and funded the same way. Nothing here runs below the
            // fence: no run is ever ready.
            if slots.offers(PalwCarrierSiteV1::Own, carrier_chain.inflight()) && !stuck_opened && funding.is_some() {
                let _ = self
                    .improve_carry_v1(
                        &mut improve,
                        &session,
                        network_domain,
                        bond,
                        current_daa,
                        &mut carrier_chain.book_with_funding(&mut funding),
                    )
                    .await;
            }
            // **ADR-0135 / audit C-1: a class registered before the registry fence never gets a
            // row.** On a chain whose fence is scheduled but not yet in force, hold the
            // registration until it is — the 2026-09-18 drill registered at boot, crossed the
            // fence at 20, and left a class with a share and no row that the work target then
            // refused for ever.
            // ADR-0147 at the entrance: and not in the landing margin below the admission-
            // independence fence, where a carrier built on the pre-fence share lands past it.
            let registration_waits = kaspa_consensus_core::palw_model_registry_v1::palw_registration_waits_for_fences_v2(
                self.consensus_config.params.palw_model_registry,
                self.consensus_config.params.palw_admission_independence,
                current_daa,
            );
            if registration_waits && self.config.register_class.is_some() && !registration.done() {
                crate::palw_backends::note_throttled_v1("class-registration-waits-for-registry", || {
                    format!(
                        "[{PALW_PANEL}] holding the class registration at daa {current_daa}: the model registry's fence is \
                         scheduled at daa {} and a class registered before it gets no lifecycle row",
                        self.consensus_config.params.palw_model_registry.map(|f| f.daa_score()).unwrap_or(0)
                    )
                });
            }
            if slots.offers(PalwCarrierSiteV1::Own, carrier_chain.inflight())
                && !registration_waits
                && self.config.register_class.is_some()
                && !registration.done()
                && registration.inflight().is_none()
            {
                // Built at the top of the tick (chain reads only); this block owns the SUBMIT
                // because the fee UTXO lives here.
                //
                // **Rebuilt HERE only when the live target has moved.** The registration gate
                // requires `initial_target` to EQUAL the base class's live target, and that
                // target moves at every epoch retarget. The first registration this network
                // saw was built at boot, submitted twenty-eight minutes later after the
                // receipt backlog, mined two minutes after THAT — and dropped at acceptance,
                // because daa 1,000's retarget had moved the base target while the object sat
                // in the cache. A value checked for equality against a moving reference must
                // be read where it is used.
                //
                // Rebuilding unconditionally re-pairs the holding. For court-capable A16 at
                // 2M that is a second ~17 min inventory walk on the same object. When the
                // cached object's target and slash still match the terms, send it.
                if registration.object().is_some() && funding.is_some() {
                    let refresh = match (registration.object(), session.palw_v2_registration_terms()) {
                        (Some(PalwConsensusObjectV2::ClassRegistered { initial_target, slash_value_per_pwu, .. }), Some(terms)) => {
                            *initial_target != terms.initial_target || *slash_value_per_pwu != terms.slash_value_per_pwu
                        }
                        _ => true,
                    };
                    if refresh {
                        match self.build_class_registration(&session).await {
                            Ok(object) => registration.replace_object(object),
                            Err(e) => warn!("[{PALW_PANEL}] cannot refresh the class registration at submit: {e}"),
                        }
                    } else {
                        info!(
                            "[{PALW_PANEL}] submitting the class registration built this process — the base class's live \
                             target still matches"
                        );
                    }
                } else if registration.object().is_some() && funding.is_none() {
                    crate::palw_backends::note_throttled_v1("class-registration-waits-for-funding", || {
                        format!(
                            "[{PALW_PANEL}] class registration is built and waiting: no fee UTXO resolves this tick \
                             (cap {MAX_INFLIGHT_CARRIERS} unconfirmed, or --palw-fee-outpoint unfunded)"
                        )
                    });
                }
                if let Some(object) = registration.cloned_object()
                    && let Some((funding_outpoint, funding_entry)) = funding.clone()
                {
                    match self.build_class_registration_tx(&object, funding_outpoint, &funding_entry, current_daa) {
                        Ok(tx) => {
                            let prepared_carrier = PreparedCarrierV1::new(tx, 0, current_daa);
                            let txid = prepared_carrier.id();
                            match self.submit_prepared_carrier_v1(&session, prepared_carrier).await {
                                Ok(sent_carrier) => {
                                    info!("[{PALW_PANEL}] submitted the class registration in tx {txid}");
                                    registration.record_submission(txid, current_daa);
                                    sent_carrier.record_change(
                                        &mut carrier_chain.book_with_funding(&mut funding),
                                        CarrierInFlightUpdateV1::Increment,
                                        |next| self.persist_fee_outpoint(next),
                                    );
                                }
                                Err(e) => {
                                    warn!("[{PALW_PANEL}] the mempool refused the class registration: {e}");
                                    funding = None;
                                }
                            }
                        }
                        Err(e) => warn!("[{PALW_PANEL}] cannot build the class registration carrier: {e}"),
                    }
                }
            }
            // ADR-0135: this seat's possession proofs, when the registry is in force and one is due
            // — read here, or before the court queue past R-core+ (M1, above).
            let duties = match readiness.take() {
                Some(duties) => duties,
                None => self.readiness_duties_for_tick(&session, current_daa, proof_waited_last_tick).await,
            };
            for duty in duties {
                // **A due proof behind a LOST carrier of ours replaces it** (the pre-t12 drill of
                // 2026-09-25): unconfirmed for the court lane's own re-plan horizon, the carrier is
                // lost by every lane's reckoning, and the proof takes its input and its slot. Once a
                // tick; a replacement that fails leaves the proof waiting as before.
                if held
                    && !tip_replaced
                    && let Some((stuck, late)) = stuck_tip
                    && palw_readiness_replaces_carrier_v1(duty.escalates(), late)
                {
                    tip_replaced = true;
                    if self
                        .replace_late_carrier_v1(
                            &session,
                            &duty,
                            stuck,
                            late,
                            current_daa,
                            &mut carrier_chain.book_with_funding(&mut funding),
                        )
                        .await
                    {
                        (*readiness_waiting) = false;
                        held = false;
                        continue;
                    }
                }
                if funding.is_none() || !slots.offers(PalwCarrierSiteV1::Own, carrier_chain.inflight()) {
                    // V07: the main slot cannot take it this tick — the readiness lane can, past R-core+.
                    let main: Vec<TransactionOutpoint> = funding
                        .iter()
                        .chain(carrier_chain.funding().iter())
                        .chain(receipt_lanes.values().filter_map(|l| l.funding.as_ref()))
                        .map(|(outpoint, _)| *outpoint)
                        .collect();
                    if self.consensus_config.params.palw_rcore_plus_active_at(current_daa)
                        && self.carry_on_readiness_lane_v1(&session, &duty, current_daa, &mut readiness_lane, &main).await
                    {
                        (*readiness_waiting) = false;
                        continue;
                    }
                    (*readiness_waiting) = true;
                    crate::palw_backends::note_throttled_v1("panel-proof-waits", || {
                        format!(
                            "[{PALW_PANEL}] a possession proof for class {} waits for a carrier slot ({inflight} of ours \
                             unconfirmed, cap {MAX_INFLIGHT_CARRIERS}); receipts yield until it lands",
                            duty.class_id().unwrap_or_default(),
                            inflight = carrier_chain.inflight()
                        )
                    });
                    break;
                }
                if self
                    .carry_readiness_proof_v1(&session, &duty, current_daa, &mut carrier_chain.book_with_funding(&mut funding))
                    .await
                {
                    (*readiness_waiting) = false;
                }
            }
            slots.at(PalwCarrierSiteV1::Licences, carrier_chain.inflight());
            // The claims the V2 pool holds anything for — heard V2 receipts, and every claim this
            // node filed on (its V3 filings keep their inner half there) — as `receipts.keys()`
            // named them before the pools were rebuilt.
            //
            // **V04: the oldest bind first, then this collector's own key** (`palw_licence_order`,
            // the pre-t12 drill of 2026-09-25). Walked in id order, every collector carried the
            // same claim — the lowest id with a standing quorum — so the network licensed at one
            // collector's rate, and whenever binds outran it the highest ids waited behind every
            // later bind: `e824102e` sat 28 DAA with all five seats `Valid` while `i0`, one of
            // them, carried 64 lower ids. Past `bound + window_receipt` such a claim redraws with
            // the same id and its second timeout slashes an honest producer.
            let claims: Vec<Hash64> =
                crate::palw_licence_order::palw_licence_claim_order_v1(receipt_pool_v2.claim_ids(), &receipt_facts, |claim| {
                    std::hash::BuildHasher::hash_one(licence_spread, claim)
                });
            if let (queued, Some((oldest, bound_daa))) = crate::palw_licence_order::palw_licence_queue_head_v1(&claims, &receipt_facts)
                && current_daa.saturating_sub(bound_daa) > crate::palw_licence_order::PALW_LICENCE_BACKLOG_WARN_DAA
            {
                crate::palw_backends::note_throttled_v1("licence-backlog", || {
                    let (valid, seats) = crate::palw_licence_order::palw_licence_valid_seats_v1(
                        &receipt_pool_v2.candidates(&oldest, &receipt_facts),
                        &receipt_pool_v3.candidates(&oldest, &receipt_facts),
                        receipt_facts.panel(&oldest),
                    );
                    format!(
                        "[{PALW_PANEL}] licence queue: {queued} bound claim(s) pooled here; the oldest, {oldest}, bound {} DAA \
                         ago (DAA {bound_daa}) with Valid receipts from {valid} of its {seats} seats pooled — offered first, \
                         oldest bind first (V04)",
                        current_daa.saturating_sub(bound_daa)
                    )
                });
            }
            tick_guard.phase("tail-pre-licence");
            let assemble_started = std::time::Instant::now();
            for claim in claims {
                if !slots.offers(PalwCarrierSiteV1::Licences, carrier_chain.inflight()) || (*readiness_waiting) {
                    break;
                }
                if assemble_started.elapsed() >= COLLECTOR_ASSEMBLE_BUDGET {
                    break;
                }
                let Some((funding_outpoint, funding_entry)) = funding.clone() else { break };
                if let Some(at) = submitted.get(&claim)
                    && current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA)
                {
                    continue;
                }
                // The attempt budget bounds consecutive FAILURES to build or submit, not the
                // claim's whole life — it is cleared on a successful submit below, because a
                // carrier that was accepted and then lost is not evidence that this object
                // cannot be built.
                if submit_attempts.get(&claim).copied().unwrap_or(0) >= SUBMIT_ATTEMPTS {
                    continue;
                }
                // Own receipts first, then what was heard of the tip panel's seats — the
                // assembler judges every one.
                let pool = receipt_pool_v2.candidates(&claim, &receipt_facts);
                let v3 = receipt_pool_v3.candidates(&claim, &receipt_facts);
                let licence_fingerprint = palw_supplementary_candidates_fingerprint_v1(&v3, &pool);
                if palw_licence_idle_v1(licence_idle.get(&claim), licence_fingerprint, current_daa) {
                    continue;
                }
                // Past SEAT-R the V1 door is offered its quorum and nothing past it, a different
                // one each time (`palw_v1_offer_v1`); below it, every candidate, as always.
                // **ADR-0152 Q-7's X22, past `palw_rcore_plus`**: coverage, then V1, then S2 — a set
                // that licenses at `basis_k ≥ 2` before the fast path that lands the claim on Q-5's
                // gate (`palw_licence_offer_order_v1`); below the fence the order it always had.
                let Some((object, door)) = kaspa_consensus_core::palw_panel_v2::palw_licence_offer_order_v1(
                    self.consensus_config.params.palw_rcore_plus_active_at(current_daa),
                    || session.palw_v2_receipt_coverage_assemble(claim, v3.clone()),
                    || {
                        if seat_r {
                            palw_v1_offer_v1(&pool, v1_sent.get(&claim).copied().unwrap_or(0), |set| {
                                session.palw_v2_receipt_quorum_assemble(claim, set)
                            })
                        } else {
                            session.palw_v2_receipt_quorum_assemble(claim, pool.clone())
                        }
                    },
                    || session.palw_v2_optimistic_assemble(claim, v3.clone()),
                ) else {
                    licence_idle.insert(claim, (licence_fingerprint, current_daa));
                    continue;
                };
                licence_idle.remove(&claim);
                let through_v1 = door == kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1::Quorum;
                match self.prepare_lifecycle_carrier_v1(&object, funding_outpoint, &funding_entry, current_daa) {
                    Ok(prepared_carrier) => {
                        let txid = prepared_carrier.id();
                        match self.submit_prepared_carrier_v1(&session, prepared_carrier).await {
                            Ok(sent_carrier) => {
                                info!(
                                    "[{PALW_PANEL}] submitted {} for claim {claim} in tx {txid}{}",
                                    object_name(&object),
                                    receipt_facts
                                        .panel(&claim)
                                        .map(|panel| format!(" (bound {} DAA ago)", current_daa.saturating_sub(panel.bound_daa)))
                                        .unwrap_or_default()
                                );
                                sent_carrier.record_change(
                                    &mut carrier_chain.book_with_funding(&mut funding),
                                    CarrierInFlightUpdateV1::Increment,
                                    |next| self.persist_fee_outpoint(next),
                                );
                                submitted.insert(claim, current_daa);
                                submit_attempts.remove(&claim);
                                if through_v1 {
                                    *v1_sent.entry(claim).or_insert(0) += 1;
                                }
                            }
                            Err(e) => {
                                // The chain we were spending is gone or was never there; stop
                                // this tick rather than build more carriers on a dead input.
                                warn!("[{PALW_PANEL}] the mempool refused the {} for claim {claim}: {e}", object_name(&object));
                                funding = None;
                            }
                        }
                    }
                    // **Counted only for a failure THIS claim's object caused** (audit M2-25).
                    // The attempt was charged before the build, so one panel-wide condition —
                    // an unfunded fee outpoint, a mempool refusal, a storage-mass ceiling —
                    // spent an attempt for every pooled claim in the same tick, and three such
                    // ticks retired every quorum the node held. A build that fails on this
                    // object's own shape is a real attempt; anything else is the node's weather.
                    Err(e) => {
                        *submit_attempts.entry(claim).or_insert(0) += 1;
                        warn!("[{PALW_PANEL}] cannot build the carrier for claim {claim}: {e}");
                    }
                }
            }
            // **ADR-0152 SR-10 / Q-7 / V3S-01: the supplementary collector** — after every licence
            // this tick could offer, on the same carrier budget, so it never takes a licence's
            // place. For each claim the chain holds `ReceiptLicensed`, the node offers ONE object
            // (`palw_select_supplementary_offer_v1`): SR-10's V3 set where it lifts an S2 licence
            // off Q-5's gate; else, on a licence awaiting its replay, a full-replay V2 `Valid` of any
            // seat the V2 door takes — the licence's own rider, a partial seat it left out, a seat a
            // V3 set paid — which covers every segment and upgrades it on its own, AHEAD of any pay
            // set, so one silent partial seat cannot hold an honest S2 claim on the gate (the M4
            // review's finding 1); else the V3 set (a release, pay, a `Sampled`). Every candidate is
            // put to the acceptance validators and the set to the fold itself, so what rides is
            // exactly what the doors credit. Sets that release the escrow first, then upgrades, then
            // pay, each soonest-first (`palw_supplementary_offer_order_v1`). Only past
            // `palw_rcore_plus`; below it the V3 door and the gate do not exist and nothing here
            // runs. Nothing is assembled while no carrier can go, and a claim whose candidates came
            // to nothing is not asked again until they change or `COURT_MOVE_REPLAN_DAA` passes
            // (`palw_supplementary_idle_v1`: each ask costs signature checks and folds of the tip).
            //
            // It rides the Licences site of P2-6's one scheduler (`PalwCarrierSlotsV1`): after the
            // collector's licences and before the priority lane's turn after them, so a supplementary
            // set never takes a licence's slot and a slot neither wanted still goes to the court.
            tick_guard.phase("licences");
            if self.consensus_config.params.palw_rcore_plus_active_at(current_daa)
                && slots.offers(PalwCarrierSiteV1::Licences, carrier_chain.inflight())
                && !(*readiness_waiting)
                && funding.is_some()
            {
                let supplementary_started = std::time::Instant::now();
                let mut offers: Vec<(Hash64, kaspa_consensus_core::palw_state_v2::PalwSupplementaryOfferV1)> = Vec::new();
                let mut claims = receipt_pool_v3.claim_ids();
                claims.extend(receipt_pool_v2.claim_ids());
                claims.sort_unstable();
                claims.dedup();
                for claim in claims {
                    if supplementary_started.elapsed() >= COLLECTOR_ASSEMBLE_BUDGET {
                        break;
                    }
                    if supplementary_v3_submitted.get(&claim).is_some_and(|at| current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA))
                    {
                        continue;
                    }
                    let v3 = receipt_pool_v3.candidates(&claim, &receipt_facts);
                    let v2 = receipt_pool_v2.candidates(&claim, &receipt_facts);
                    if v3.is_empty() && v2.is_empty() {
                        continue;
                    }
                    let fingerprint = palw_supplementary_candidates_fingerprint_v1(&v3, &v2);
                    if palw_supplementary_idle_v1(supplementary_idle.get(&claim), fingerprint, current_daa) {
                        continue;
                    }
                    match session.palw_v2_supplementary_assemble(claim, v3, v2) {
                        Some(offer) => {
                            supplementary_idle.remove(&claim);
                            offers.push((claim, offer));
                        }
                        None => {
                            supplementary_idle.insert(claim, (fingerprint, current_daa));
                        }
                    }
                }
                palw_supplementary_offer_order_v1(&mut offers);
                for (claim, offer) in offers {
                    if !slots.offers(PalwCarrierSiteV1::Licences, carrier_chain.inflight()) || (*readiness_waiting) {
                        break;
                    }
                    let Some((funding_outpoint, funding_entry)) = funding.clone() else { break };
                    match self.prepare_lifecycle_carrier_v1(&offer.object, funding_outpoint, &funding_entry, current_daa) {
                        Ok(prepared_carrier) => {
                            let txid = prepared_carrier.id();
                            match self.submit_prepared_carrier_v1(&session, prepared_carrier).await {
                                Ok(sent_carrier) => {
                                    info!(
                                        "[{PALW_PANEL}] submitted a {} supplementary set for claim {claim} in tx {txid} — {} seat(s) \
                                         credited, {} recounted, basis {} → {}{}{} (SR-10, V3S-01)",
                                        match offer.object {
                                            PalwConsensusObjectV2::ReceiptLicensed { .. } => "V2 full-replay",
                                            _ => "V3",
                                        },
                                        offer.effect.credited.len(),
                                        offer.effect.recounted.len(),
                                        offer.effect.basis_k_before,
                                        offer.effect.basis_k_after,
                                        if offer.effect.upgrades { ", lifting the S2 gate" } else { "" },
                                        if offer.effect.releases_escrow { ", releasing the escrow" } else { "" },
                                    );
                                    sent_carrier.record_change(
                                        &mut carrier_chain.book_with_funding(&mut funding),
                                        CarrierInFlightUpdateV1::Increment,
                                        |next| self.persist_fee_outpoint(next),
                                    );
                                    supplementary_v3_submitted.insert(claim, current_daa);
                                }
                                Err(e) => {
                                    warn!("[{PALW_PANEL}] the mempool refused the V3 supplementary set for claim {claim}: {e}");
                                    funding = None;
                                }
                            }
                        }
                        Err(e) => warn!("[{PALW_PANEL}] cannot build the V3 supplementary carrier for claim {claim}: {e}"),
                    }
                }
            }
            // **RFC-0006: what this node carries for layer-sharded panels** — the shards' licensing parts its pool of
            // cell-masked receipts completes, the registrant's plan declaration, and each held shard's possession proof — on
            // the Licences site's carrier budget, after every licence a flat claim could take. Nothing runs on a network
            // that never armed the fence (no plan, no part, no V4 receipt) or in shadow mode.
            if self.consensus_config.params.palw_tir_shard_active_at(current_daa) && !self.config.tir_shard_shadow {
                let mut offers: Vec<(String, PalwConsensusObjectV2, Option<Hash64>)> = shard_books
                    .parts_to_offer(&session, current_daa)
                    .into_iter()
                    .map(|(claim, object)| (format!("a layer-shard licensing part of claim {claim}"), object, Some(claim)))
                    .collect();
                offers.extend(
                    self.tir_shard_objects_v1(&session, bond_key, network_domain, current_daa, &mut shard_books)
                        .await
                        .into_iter()
                        .map(|(what, object)| (what, object, None)),
                );
                for (what, object, part_of) in offers {
                    if !slots.offers(PalwCarrierSiteV1::Licences, carrier_chain.inflight()) || (*readiness_waiting) {
                        break;
                    }
                    if self
                        .carry_object_v1(&session, &object, &what, current_daa, &mut carrier_chain.book_with_funding(&mut funding))
                        .await
                        && let Some(claim) = part_of
                    {
                        shard_books.note_part_carried(claim, current_daa);
                    }
                }
            }
            // **P2-6: the licences' turn passes the slot to the priority lane** when the collector
            // had nothing to carry, in the same tick.
            tick_guard.phase("supplementary");
            slots.at(PalwCarrierSiteV1::PriorityAfterLicences, carrier_chain.inflight());
            if slots.offers(PalwCarrierSiteV1::PriorityAfterLicences, carrier_chain.inflight()) {
                self.carry_priority_v1(
                    &session,
                    current_daa,
                    &mut court_outbox,
                    &mut carrier_chain.book_with_funding(&mut funding),
                    &mut challenged,
                )
                .await;
            }
            slots.at(PalwCarrierSiteV1::OwnReceipts, carrier_chain.inflight());
            // **ADR-0124 Decision 2: a seat carries its own receipt after the licence.** For
            // every claim this seat answered `Valid` on, once the chain has licensed it without
            // crediting this seat, the receipt rides a supplementary `ReceiptLicensed` while
            // the receipt window is open. The chain decides what "in time" and "already
            // credited" mean (`palw_v2_supplementary_receipt_assemble` runs the acceptance
            // validator itself), so this loop only asks, and re-asks after the replan interval
            // if the carrier it sent was lost.
            // **RFC-0007: a sealed vertex rides once, in the seat's own slot** — the oldest not yet sent (or whose carrier has waited out the
            // replan interval), as an ordinary lifecycle carrier; every node relays it and any block takes it, so there is no collector.
            if vertex_fence.is_some()
                && slots.offers(PalwCarrierSiteV1::OwnReceipts, carrier_chain.inflight())
                && !(*readiness_waiting)
                && let Some((funding_outpoint, funding_entry)) = funding.clone()
                // (DRILL ONLY: after the round's first vertex has gone out, its deliberate second one — `take_drill_extra` is `None`
                // on every node that was not started with `--palw-drill-vertex-equivocate-at`.)
                && let Some(vertex) = vertex_book.next_to_send(current_daa).cloned().or_else(|| vertex_book.take_drill_extra())
            {
                let object = vertex::palw_vertex_object_v1(&vertex);
                // **Asked of the chain first** (P2-8e): the fold's own answer on the tip at the virtual's DAA — a vertex the chain
                // already holds (its carrier landed while this tick ran), or one whose clock has passed, is not worth a carrier. It waits
                // out the replan interval and is re-asked; the seat's row appearing at the tip retires it.
                if let Some(answer) = session.palw_object_rehearsal_v1(&object)
                    && !matches!(answer, kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
                {
                    trace!("[{PALW_PANEL}] this seat's vertex of round {} is not offered a carrier now: {answer:?}", vertex.round);
                    vertex_book.mark_sent(vertex.round, current_daa);
                } else {
                    match self.prepare_lifecycle_carrier_v1(&object, funding_outpoint, &funding_entry, current_daa) {
                        Ok(prepared_carrier) => {
                            let txid = prepared_carrier.id();
                            match self.submit_prepared_carrier_v1(&session, prepared_carrier).await {
                                Ok(sent_carrier) => {
                                    info!(
                                        "[{PALW_PANEL}] submitted this seat's vertex of round {} ({} leaves) in tx {txid} (RFC-0007)",
                                        vertex.round,
                                        vertex.leaves.len()
                                    );
                                    sent_carrier.record_change(
                                        &mut carrier_chain.book_with_funding(&mut funding),
                                        CarrierInFlightUpdateV1::Increment,
                                        |next| self.persist_fee_outpoint(next),
                                    );
                                    vertex_book.mark_sent(vertex.round, current_daa);
                                }
                                Err(e) => {
                                    warn!("[{PALW_PANEL}] the mempool refused this seat's vertex of round {}: {e}", vertex.round);
                                    funding = None;
                                }
                            }
                        }
                        Err(e) => {
                            warn!("[{PALW_PANEL}] cannot build the carrier for this seat's vertex of round {}: {e}", vertex.round)
                        }
                    }
                }
            }
            // **RFC-0007 Part IV.1: a trap move** — the commitment (when this bond is drawn by the slot lottery) or the reveal (once the
            // claim's audit window has closed), each carried once from `palw-traps.json`'s plans, rehearsed on the tip first.
            if vertex_fence.is_some()
                && self.consensus_config.params.palw_audit_mesh_active_at(current_daa)
                && slots.offers(PalwCarrierSiteV1::OwnReceipts, carrier_chain.inflight())
                && !(*readiness_waiting)
                && let Some((funding_outpoint, funding_entry)) = funding.clone()
                && let Some(kp) = self.keypair.as_ref()
            {
                trap_book.refresh();
                let sign_trap = |message: &[u8], context: &[u8]| Self::sign_hedged(&kp.signing_key, message, context);
                let audit_window = |claim: &Hash64| session.palw_v2_mesh_audit_window_v1(*claim);
                let next = trap_book
                    .next_reveal(network_domain, bond_key, current_daa, &audit_window, &sign_trap)
                    .map(|(index, reveal)| (index, true, PalwConsensusObjectV2::TrapRevealedV1 { reveal: Box::new(reveal) }))
                    .or_else(|| {
                        trap_book
                            .next_commit(network_domain, bond_key, current_daa, &sign_trap)
                            .map(|(index, trap)| (index, false, PalwConsensusObjectV2::TrapCommittedV1 { trap: Box::new(trap) }))
                    });
                if let Some((index, is_reveal, object)) = next {
                    let rehearsal = session.palw_object_rehearsal_v1(&object);
                    if let Some(answer) = rehearsal
                        && !matches!(answer, kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
                    {
                        trace!(
                            "[{PALW_PANEL}] this seat's trap {} is not offered a carrier now: {answer:?}",
                            if is_reveal { "reveal" } else { "commitment" }
                        );
                    } else {
                        match self.prepare_lifecycle_carrier_v1(&object, funding_outpoint, &funding_entry, current_daa) {
                            Ok(prepared_carrier) => {
                                let txid = prepared_carrier.id();
                                match self.submit_prepared_carrier_v1(&session, prepared_carrier).await {
                                    Ok(sent_carrier) => {
                                        info!(
                                            "[{PALW_PANEL}] submitted this seat's trap {} in tx {txid} (RFC-0007 Part IV.1)",
                                            if is_reveal { "reveal" } else { "commitment" }
                                        );
                                        sent_carrier.record_change(
                                            &mut carrier_chain.book_with_funding(&mut funding),
                                            CarrierInFlightUpdateV1::Increment,
                                            |next| self.persist_fee_outpoint(next),
                                        );
                                        if is_reveal {
                                            trap_book.mark_revealed(index);
                                        } else {
                                            trap_book.mark_committed(index);
                                        }
                                    }
                                    Err(e) => {
                                        warn!("[{PALW_PANEL}] the mempool refused this seat's trap move: {e}");
                                        funding = None;
                                    }
                                }
                            }
                            Err(e) => warn!("[{PALW_PANEL}] cannot build the carrier for this seat's trap move: {e}"),
                        }
                    }
                }
            }
            let mut own: Vec<(Hash64, PalwSeatReceiptV2)> =
                own_receipts.iter().map(|(claim, (receipt, _))| (*claim, receipt.clone())).collect();
            own.sort_by_key(|(claim, _)| *claim);
            for (claim, receipt) in own {
                if let Some(at) = supplementary_submitted.get(&claim)
                    && current_daa < at.saturating_add(COURT_MOVE_REPLAN_DAA)
                {
                    continue;
                }
                let main_takes = slots.offers(PalwCarrierSiteV1::OwnReceipts, carrier_chain.inflight())
                    && !(*readiness_waiting)
                    && funding.is_some();
                if !main_takes {
                    // RFC-0018 (iv): the main slot is taken (one carrier in flight, a court storm, a waiting possession proof):
                    // the receipt rides its class's lane, if this seat holds a spare output for it.
                    let Some(class) = own_receipt_class.get(&claim).copied() else { continue };
                    if !palw_receipt_lane_takes_v1(receipt_lanes.get(&class).map(|l| l.inflight), receipt_lanes.len()) {
                        continue;
                    }
                    let held: Vec<TransactionOutpoint> = funding
                        .iter()
                        .chain(carrier_chain.funding().iter())
                        .chain(readiness_lane.funding.iter())
                        .chain(receipt_lanes.values().filter_map(|l| l.funding.as_ref()))
                        .map(|(outpoint, _)| *outpoint)
                        .collect();
                    let lane = receipt_lanes.entry(class).or_default();
                    if lane.funding.is_none() {
                        if lane.unfunded_at == Some(current_daa) {
                            continue;
                        }
                        lane.funding = self.resolve_readiness_lane_funding_v1(&session, &held).await;
                        if lane.funding.is_none() {
                            lane.unfunded_at = Some(current_daa);
                            continue;
                        }
                    }
                    let Some(object) = session.palw_v2_supplementary_receipt_assemble(claim, vec![receipt]) else { continue };
                    let Some((lane_outpoint, lane_entry)) = lane.funding.clone() else { continue };
                    match self.prepare_lifecycle_carrier_v1(&object, lane_outpoint, &lane_entry, current_daa) {
                        Ok(prepared_carrier) => {
                            let txid = prepared_carrier.id();
                            match self.submit_prepared_carrier_v1(&session, prepared_carrier).await {
                                Ok(sent_carrier) => {
                                    info!(
                                        "[{PALW_PANEL}] submitted this seat's own receipt for claim {claim} on class {class}'s lane in tx {txid}"
                                    );
                                    sent_carrier.record_change(
                                        &mut CarrierBookMutV1::new(&mut lane.funding, &mut lane.inflight),
                                        CarrierInFlightUpdateV1::Set(1),
                                        |_| {},
                                    );
                                    supplementary_submitted.insert(claim, current_daa);
                                }
                                Err(e) => {
                                    warn!("[{PALW_PANEL}] the mempool refused the lane receipt for claim {claim}: {e}");
                                    lane.funding = None;
                                }
                            }
                        }
                        Err(e) => warn!("[{PALW_PANEL}] cannot build the lane carrier for claim {claim}: {e}"),
                    }
                    continue;
                }
                let Some((funding_outpoint, funding_entry)) = funding.clone() else { continue };
                let Some(object) = session.palw_v2_supplementary_receipt_assemble(claim, vec![receipt]) else { continue };
                match self.prepare_lifecycle_carrier_v1(&object, funding_outpoint, &funding_entry, current_daa) {
                    Ok(prepared_carrier) => {
                        let txid = prepared_carrier.id();
                        match self.submit_prepared_carrier_v1(&session, prepared_carrier).await {
                            Ok(sent_carrier) => {
                                info!(
                                    "[{PALW_PANEL}] submitted this seat's own receipt for claim {claim} (supplementary) in tx {txid}"
                                );
                                sent_carrier.record_change(
                                    &mut carrier_chain.book_with_funding(&mut funding),
                                    CarrierInFlightUpdateV1::Increment,
                                    |next| self.persist_fee_outpoint(next),
                                );
                                supplementary_submitted.insert(claim, current_daa);
                            }
                            Err(e) => {
                                warn!("[{PALW_PANEL}] the mempool refused the supplementary receipt for claim {claim}: {e}");
                                funding = None;
                            }
                        }
                    }
                    Err(e) => warn!("[{PALW_PANEL}] cannot build the supplementary carrier for claim {claim}: {e}"),
                }
            }
            (*last_lane) = slots.finish(carrier_chain.inflight());
            // V01's panel side: the opening closes with the tick. One the pool took in the stuck
            // carrier's place is the chain's new tip (written back below); one nothing took leaves
            // the stuck carrier holding the slot, exactly as it did before the tick.
            if stuck_opened {
                let opening = self.carrier_replacement.lock().unwrap().take();
                if opening.is_none_or(|opening| opening.replaced_by.is_none()) {
                    carrier_chain.set_inflight(MAX_INFLIGHT_CARRIERS);
                    held = true;
                }
            }
            // What the next tick continues from. `None` here means a refusal cleared it, and
            // the next tick resolves afresh.
            //
            // **Only when this tick was allowed to spend.** A back-pressured tick sets `funding`
            // to `None` deliberately — it is not a statement about the chain tip, and writing it
            // back DESTROYED a perfectly good one. The tip then had to be recovered by the scan,
            // which could not see it either: the last carrier was still unmined, so its change
            // was in no UTXO set at all. Measured on testnet-11: node 0 held at the cap once and
            // never carried another object, 6,343 warnings later.
            if !held {
                carrier_chain.set_funding(funding);
            }
        }
    }
}
