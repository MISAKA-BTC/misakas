while let Ok(event) = inbox.try_recv() {
                match event {
                    PalwGossipEvent::Material { claim, bytes } => {
                        // **Persisted the moment it arrives, before any verdict.** The bytes are
                        // self-authenticating (every reader verifies against the claim's committed
                        // roots), and the producer that broadcast them may be gone by the time a
                        // panel — or a court — needs them: five outside floor producers stopped
                        // their nodes on 2026-08-28 and every in-flight claim of theirs defaulted,
                        // because the only durable copies were the producers' own. One surviving
                        // copy anywhere in the fleet now serves the whole network via the pull.
                        // **Nothing is written to disk here** (audit M2-2). This ran for EVERY
                        // gossiped material — no check that the claim exists on chain, that this
                        // node is seated on it, that the bytes verify, or that a bonded party sent
                        // them — with a 72-hour mtime sweep as the only bound, on the same volume
                        // as the consensus database. Retention now happens where the bytes have
                        // been proven to be the claim's, in the duty loop below.
                        pool_books.material.admit(claim, bytes);
                        // And the whole-pool ceiling, enforced HERE — a bound checked on the next
                        // tick is a bound the sender outruns (audit3 S-02).
                        pool_books.material.evict_after_gossip(claim, &mut first_seen);
                    }
                    PalwGossipEvent::Receipt { bytes } => {
                        // **Held for this tick's chain read, not pooled here** (the receipt-pool
                        // flush fix). Whether a receipt may be kept depends on the panel the tip
                        // binds and the key the registry holds for its seat, which the drain has
                        // not read yet; the admission after the duty read decides
                        // (`receipt_pool_tick_v1`). What the drain can refuse without the chain it
                        // refuses (`receipt_arrival_push_v1`): bytes that are no receipt, and a
                        // signature that is not an ML-DSA-87 signature's length (the audit's
                        // one-to-eight-byte junk) — no key verifies either, and neither may take a
                        // place in the queue.
                        // RFC-0006: a cell-masked receipt has its own queue and pool (borsh refuses trailing bytes, so no other
                        // version decodes as a V4 receipt and it decodes as none of them).
                        if !tir_shard::palw_tir_shard_arrival_push_v1(&mut shard_books.arrivals, &bytes) {
                            crate::palw_receipt_pool::receipt_arrival_push_v1(&mut receipt_arrivals, &bytes);
                        }
                    }
                    // ADR-0077 Decision 8. Held, not judged: the seat binds an opening to the
                    // claim's roots and replays the interval before believing anything in it.
                    PalwGossipEvent::IntervalOpening { claim, interval_index, bytes } => {
                        // G14 detection policy: the pair ceiling follows the largest k the armed policies draw (512 without one).
                        let pairs = kaspa_consensus_core::palw_detection_policy_v1::palw_detection_retention_pairs_v1(
                            self.consensus_config.params.palw_detection_policy_v1.as_ref(),
                        ) as usize;
                        if !pool_books.intervals.admit(claim, interval_index, bytes, pairs) {
                            // Counted, never silent: a dropped opening the seat drew is a capacity refusal, not a sampling miss.
                            self.config.telemetry.detection_loss(
                                Hash64::default(),
                                kaspa_consensus_core::palw_detection_policy_v1::PalwDetectionLossV1::CapacityRefusal,
                            );
                        }
                    }
                }
            }