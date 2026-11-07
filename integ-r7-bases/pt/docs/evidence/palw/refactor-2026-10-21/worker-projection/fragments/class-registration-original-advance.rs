if let Some((txid, sent_daa)) = class_registration_inflight {
                // Did the class actually appear? `registered_class_ids` is the chain's own answer,
                // and it is the ONE fact that distinguishes "the carrier was mined and the object
                // stood" from "the carrier was mined and the object was dropped inside it".
                //
                // The id is the object we already built — never `class_registration_id()`, which
                // re-pairs the holding and at 2M walks the A16 inventory again (~17 min) just to
                // learn a Hash64 we signed. A court-capable A16 registration that paid that walk
                // to submit would then pay it every tick until the carrier landed.
                let landed = match (session.palw_v2_registration_terms(), &class_registration) {
                    (Some(terms), Some(PalwConsensusObjectV2::ClassRegistered { class_id, .. }))
                    | (Some(terms), Some(PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id, .. })) => {
                        terms.registered_class_ids.contains(class_id)
                    }
                    (Some(terms), _) => self.class_registration_id().is_some_and(|id| terms.registered_class_ids.contains(&id)),
                    // No terms (no V2 bundle) or no id this node can derive: the honest answer is
                    // "cannot tell", and the retry horizon below is what keeps that from latching.
                    _ => false,
                };
                // **Is the carrier still queued here?** A carrier leaves the mempool when a block
                // takes it (or when it is evicted); the fold registers the class in that same block,
                // so a carrier gone from the pool with no class a few DAA later was dropped inside
                // its block (the D-F drill of 2026-09-29: a second IR registration in one block) or
                // is gone — and waiting out the whole horizon for it is what left that node silent.
                let in_mempool = landed
                    || self
                        .flow_context
                        .mining_manager()
                        .clone()
                        .has_transaction(txid, kaspa_mining::model::tx_query::TransactionQuery::All)
                        .await;
                if in_mempool {
                    class_registration_left_pool = None;
                } else if class_registration_left_pool.is_none() {
                    class_registration_left_pool = Some(current_daa);
                }
                match palw_class_registration_step_v1(
                    landed,
                    in_mempool,
                    sent_daa,
                    class_registration_left_pool,
                    class_registration_drops,
                    current_daa,
                ) {
                    PalwClassRegistrationStepV1::Landed => {
                        info!("[{PALW_PANEL}] the class registration in tx {txid} is on the chain");
                        class_registration_inflight = None;
                        class_registration_done = true;
                        class_registration_drops = 0;
                    }
                    PalwClassRegistrationStepV1::Wait => {}
                    PalwClassRegistrationStepV1::Retry(why) => {
                        match why {
                            // The object did not stand in the block that took its carrier (or the
                            // carrier was evicted): rebuilt from the live terms and sent again now.
                            PalwClassRegistrationRetryV1::LeftThePoolWithoutTheClass => {
                                class_registration_drops = class_registration_drops.saturating_add(1);
                                warn!(
                                    "[{PALW_PANEL}] the class registration carrier {txid} (sent at daa {sent_daa}) left the \
                                     mempool at daa {} and the class is not registered at {current_daa}: its object was \
                                     dropped inside the block that took it (that block's \"a class registration was \
                                     dropped\" line says why) or the carrier was evicted — rebuilding and resubmitting \
                                     (drop {class_registration_drops} in a row)",
                                    class_registration_left_pool.unwrap_or(current_daa)
                                );
                            }
                            // The carrier had a generous window to be mined and the class still is
                            // not there. Believing a MEMPOOL receipt forever is what made that loss
                            // permanent and silent.
                            PalwClassRegistrationRetryV1::StillQueuedPastTheHorizon => warn!(
                                "[{PALW_PANEL}] the class registration carrier {txid} was sent at daa {sent_daa} and the class is \
                                 still not registered at {current_daa} — rebuilding and retrying"
                            ),
                        }
                        class_registration_inflight = None;
                        class_registration_left_pool = None;
                        class_registration = None;
                    }
                }
            }
            if self.config.register_class.is_some()
                && !class_registration_done
                && class_registration_inflight.is_none()
                && class_registration.is_none()
            {
                match self.build_class_registration(&session).await {
                    Ok(object) => {
                        info!("[{PALW_PANEL}] built a class registration for this node's worker");
                        class_registration = Some(object);
                    }
                    Err(e) => {
                        // **A class already on the chain is done.** The builder refuses a class the
                        // chain holds, and nothing else ever set `done` — so a registrant restarted
                        // after its class landed (or one whose carrier stood while it was down)
                        // refused every tick, for good, and skipped its duties behind it.
                        let id = *class_registration_known_id.get_or_insert_with(|| self.class_registration_id());
                        let on_chain = id
                            .zip(session.palw_v2_registration_terms())
                            .is_some_and(|(id, terms)| terms.registered_class_ids.contains(&id));
                        if on_chain {
                            info!(
                                "[{PALW_PANEL}] this node's class {} is already registered on the chain — nothing to register",
                                id.unwrap_or_default()
                            );
                            class_registration_done = true;
                        } else {
                            // Once a minute: a gate that refuses (an IR class below `palw_tir_v1`, say)
                            // refuses every tick until the chain moves, and the retry each tick is what
                            // registers it then.
                            crate::palw_backends::note_throttled_v1("class-registration-build", || {
                                format!("[{PALW_PANEL}] cannot register this node's class: {e}")
                            })
                        }
                    }
                }
            }
            // **Submit HERE, before the duty sweep.** The funded carrier used to be built at the
            // bottom of the tick, after every seated claim had been judged. On a 2M A16 registrant
            // that sweep never finished (one replay of a foreign duty sat for tens of minutes), so
            // the object existed in memory and the chain never saw it. The fee UTXO is readable
            // now; the live target was just sampled into the object.
            if self.config.register_class.is_some()
                && !class_registration_done
                && class_registration_inflight.is_none()
                && class_registration.is_some()
            {
                let registration_waits = kaspa_consensus_core::palw_model_registry_v1::palw_registration_waits_for_fences_v2(
                    self.consensus_config.params.palw_model_registry,
                    self.consensus_config.params.palw_admission_independence,
                    current_daa,
                );
                if registration_waits {
                    crate::palw_backends::note_throttled_v1("class-registration-waits-for-registry", || {
                        format!(
                            "[{PALW_PANEL}] holding the class registration at daa {current_daa}: the model registry's fence is \
                             scheduled at daa {} and a class registered before it gets no lifecycle row",
                            self.consensus_config.params.palw_model_registry.map(|f| f.daa_score()).unwrap_or(0)
                        )
                    });
                } else if let Some((funding_outpoint, funding_entry)) =
                    self.resolve_fee_funding(&session, palw_fee_funding_floor_v1()).await
                {
                    if let Some(object) = class_registration.clone() {
                        match self.build_class_registration_tx(&object, funding_outpoint, &funding_entry, current_daa) {
                            Ok(tx) => {
                                let prepared_carrier = PreparedCarrierV1::new(tx, 0, current_daa);
                                let txid = prepared_carrier.id();
                                match self.submit_prepared_direct_v1(&session, prepared_carrier).await {
                                    Ok(sent_carrier) => {
                                        info!("[{PALW_PANEL}] submitted the class registration in tx {txid}");
                                        class_registration_inflight = Some((txid, current_daa));
                                        class_registration_left_pool = None;
                                        sent_carrier.record_change(
                                            &mut carrier_chain.book(),
                                            CarrierInFlightUpdateV1::SaturatingIncrement,
                                            |next| self.persist_fee_outpoint(next),
                                        );
                                    }
                                    Err(e) => warn!("[{PALW_PANEL}] the mempool refused the class registration: {e}"),
                                }
                            }
                            Err(e) => warn!("[{PALW_PANEL}] cannot build the class registration carrier: {e}"),
                        }
                    }
                } else {
                    crate::palw_backends::note_throttled_v1("class-registration-waits-for-funding", || {
                        format!(
                            "[{PALW_PANEL}] class registration is built and waiting: no fee UTXO resolves this tick \
                             (--palw-fee-outpoint {})",
                            self.config.fee_outpoint.as_deref().unwrap_or("unset")
                        )
                    });
                }
            }
            // Do not spend the rest of the tick on a foreign duty backlog while this node's built
            // registration waits for its funding — that is the stall that left the 2M object unsent.
            // Only then (`palw_class_registration_holds_the_tick_v1`): a registrant is a seat, and
            // while its gate refuses, its carrier waits for a block, or its class already stands, it
            // does its duties.
            if palw_class_registration_holds_the_tick_v1(
                self.config.register_class.is_some(),
                class_registration_done,
                class_registration.is_some(),
                class_registration_inflight.is_some(),
            ) {
                continue;
            }