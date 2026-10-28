//! End-of-tick retention, replay retirement and receipt bookkeeping sweeps.
//! This scope cannot spend carrier funding or advance court sessions.
use super::*;

pub(super) struct RetirementReadV1<'a> {
    pub(super) duties: &'a [kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2],
    pub(super) court_progress: &'a court_progress::CourtProgressResultV1,
    pub(super) court_outbox: &'a CourtOutboxV1,
    pub(super) challenged: &'a HashSet<Hash64>,
    pub(super) seat_r: bool,
}

pub(super) struct RetirementRefsV1<'a> {
    pub(super) pool_books: &'a mut PoolBooksV1,
    pub(super) receipt_pool_v2: &'a mut crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV2>,
    pub(super) receipt_pool_v3: &'a mut crate::palw_receipt_pool::PalwReceiptPoolV1<PalwSeatReceiptV3>,
    pub(super) own_filed: &'a mut HashMap<SeatDutyPanelKeyV1, crate::palw_receipt_pool::OwnFiledV1>,
    pub(super) own_filed_whole: &'a mut HashMap<SeatDutyPanelKeyV1, crate::palw_receipt_pool::OwnFiledV1>,
    pub(super) answered: &'a mut HashSet<SeatDutyPanelKeyV1>,
    pub(super) first_seen: &'a mut HashMap<Hash64, u64>,
    pub(super) replayed: &'a mut HashSet<SeatDutyPanelKeyV1>,
    pub(super) seat_replays: &'a mut PalwSeatReplaysV1,
    pub(super) seat_resumes: &'a mut PalwSeatResumesV1,
    pub(super) service: &'a mut PalwSeatServiceV1,
    pub(super) replay_refuted: &'a mut HashSet<Hash64>,
    pub(super) evidence_kept: &'a mut HashSet<Hash64>,
    pub(super) capture_sampled: &'a mut HashSet<Hash64>,
    pub(super) s3_samplings: &'a mut HashMap<SeatDutyPanelKeyV1, tokio::task::JoinHandle<PalwSeatS3SampleV1>>,
    pub(super) s3_sampled: &'a mut HashSet<SeatDutyPanelKeyV1>,
    pub(super) requested: &'a mut HashMap<Hash64, u64>,
    pub(super) requested_intervals: &'a mut HashMap<(Hash64, u32), u64>,
    pub(super) own_receipts: &'a mut HashMap<Hash64, (PalwSeatReceiptV2, u64)>,
    pub(super) own_receipt_class: &'a mut HashMap<Hash64, Hash64>,
    pub(super) accusations: &'a mut PalwSeatAccusationsV1,
    pub(super) submit_attempts: &'a mut HashMap<Hash64, u32>,
    pub(super) v1_sent: &'a mut HashMap<Hash64, u32>,
    pub(super) supplementary_submitted: &'a mut HashMap<Hash64, u64>,
    pub(super) supplementary_v3_submitted: &'a mut HashMap<Hash64, u64>,
    pub(super) supplementary_idle: &'a mut HashMap<Hash64, (u64, u64)>,
    pub(super) licence_idle: &'a mut HashMap<Hash64, (u64, u64)>,
    pub(super) receipt_lanes: &'a mut BTreeMap<Hash64, PalwReadinessLaneV1>,
    pub(super) retention_liabilities: &'a mut HashMap<Hash64, u64>,
    pub(super) liabilities_unread: &'a mut HashSet<Hash64>,
    pub(super) own_executions: &'a mut HashMap<Hash64, Vec<u8>>,
    pub(super) submitted: &'a mut HashMap<Hash64, u64>,
}
impl PalwPanelService {
    pub(super) async fn retire_tick_v1(
        self: &Arc<Self>,
        tick: tick_inputs::PanelTickReadV1<'_>,
        read: RetirementReadV1<'_>,
        books: RetirementRefsV1<'_>,
    ) {
        let session = tick.session();
        let bond_key = tick.bond();
        let current_daa = tick.daa();
        let RetirementReadV1 { duties, court_progress, court_outbox, challenged, seat_r } = read;
        let RetirementRefsV1 {
            pool_books,
            receipt_pool_v2,
            receipt_pool_v3,
            own_filed,
            own_filed_whole,
            answered,
            first_seen,
            replayed,
            seat_replays,
            seat_resumes,
            service,
            replay_refuted,
            evidence_kept,
            capture_sampled,
            s3_samplings,
            s3_sampled,
            requested,
            requested_intervals,
            own_receipts,
            own_receipt_class,
            accusations,
            submit_attempts,
            v1_sent,
            supplementary_submitted,
            supplementary_v3_submitted,
            supplementary_idle,
            licence_idle,
            receipt_lanes,
            retention_liabilities,
            liabilities_unread,
            own_executions,
            submitted,
        } = books;
        // Forget what the chain has moved past: a claim with no duty and no unsubmitted quorum
        // holds memory for nothing. (Duties vanish when a claim leaves `PanelBound`.)
        //
        // **A disputed claim is not moved past.** A court opens on a `ReceiptLicensed` claim,
        // which is a phase past `PanelBound` — so the old rule dropped the tiles exactly when a
        // dispute could start needing them, and neither party could disclose or refute. Claims
        // under an open court keep their material for as long as the session lives.
        // **And a claim this node has not yet judged is not moved past either.**
        //
        // The sequence that bites: a claim licenses, its seat duty vanishes, `submitted` holds
        // it, and the material is evicted — and only THEN does a court open on it. By the time
        // the court duty appears the tiles are gone, so the challenger cannot refute and the
        // responder cannot disclose. Keeping court duties alone was not enough because the
        // eviction happens strictly earlier.
        //
        // The window is bounded by this node's own decision latency rather than by the claim's
        // life: a challenger holds a licensed claim's capture until it has either reproduced it
        // (nothing to say) or carried a `CourtOpened` — both of which put it in `challenged` on
        // the next tick. A producer needs no such rule; it re-broadcasts its own material while
        // its claim is unresolved, and `mark_own_material` feeds that back to itself.
        let mut live: HashSet<Hash64> = duties.iter().map(|d| d.claim_id).collect();
        live.extend(court_progress.court_duties.iter().map(|d| d.claim_id));
        live.extend(court_outbox.pending().iter().filter_map(|(_, _, _, o)| match o {
            PalwConsensusObjectV2::CourtOpened { claim, .. } => Some(*claim),
            _ => None,
        }));
        if self.config.challenge {
            live.extend(
                session
                    .palw_disputable_claims_v2(vec![bond_key])
                    .into_iter()
                    .filter_map(|d| (!challenged.contains(&d.claim_id)).then_some(d.claim_id)),
            );
        }
        // **"Not yet submitted" is not a bounded condition.** The rule was: keep a claim's
        // material while it is LIVE (a duty or a dispute names it) or while this node has not
        // submitted for it. The second half never becomes false for a claim the panel cannot
        // submit for — no quorum assembled, the class was never resolvable, the receipt window
        // lapsed — so its material stayed in memory for the life of the process. Measured on
        // testnet-11: hundreds of such claims a day, each holding up to four gossiped
        // materials, against a node whose RSS climbed from zero to 11 GB in twelve hours and
        // was OOM-killed roughly every thirty.
        //
        // Bounded by the claim's own age instead. Past the window, a claim that is not live
        // is one no duty, no dispute and no license will ever ask about again — and the
        // court's own path does not depend on this pool anyway: it re-reads the capture from
        // `retained_capture` on disk, which is what the retention obligation is for.
        // **A claim with no recorded arrival is STALE, not immortal** (audit M2-2). `is_some_and`
        // returned false for exactly the entries that dominate the pool — the ones this seat
        // has no duty on (a seat is drawn on ~5/N of claims) — so the retention bound applied
        // only to claims that were already bounded by their duty, and every foreign claim was
        // kept for the life of the process. Arrivals are stamped at pool insert now, so `None`
        // here means an entry older than any bookkeeping this process still holds.
        let stale = |claim: &Hash64| {
            first_seen.get(claim).map(|seen| current_daa > seen.saturating_add(PANEL_POOL_RETENTION_DAA)).unwrap_or(true)
        };
        pool_books.material.sweep(|claim| live.contains(claim) || (!submitted.contains_key(claim) && !stale(claim)));
        // The receipt pools by the same rule (`ReceiptSweepV1`): a heard claim aged from when the
        // pool first heard it (a peer's receipt can arrive before this node has seen the bind
        // that makes the claim its duty, so a young foreign claim is kept), and this node's own
        // receipts from when it filed them — kept while a duty names the claim, however long
        // the claim's class keeps it `PanelBound`, and otherwise until this node submits for
        // it or the retention age passes. Never because of anything that arrived, and never at
        // the duty's global `receipt_deadline`, which a heavy class outlives (ADR-0133 §11.3).
        let receipt_sweep = crate::palw_receipt_pool::ReceiptSweepV1 {
            live: &live,
            submitted: &submitted,
            current_daa,
            retention_daa: PANEL_POOL_RETENTION_DAA,
        };
        let carried_nothing = HashMap::new();
        let pools_sweep = palw_receipt_pools_sweep_v1(
            self.consensus_config.params.palw_rcore_plus_active_at(current_daa),
            receipt_sweep,
            &carried_nothing,
        );
        receipt_pool_v2.sweep(&pools_sweep);
        receipt_pool_v3.sweep(&pools_sweep);
        // The bookkeeping keyed on those claims goes with them, or the maps that decide what to
        // keep become the thing that grows.
        first_seen.retain(|claim, _| {
            pool_books.material.map().contains_key(claim) || receipt_pool_v2.contains_claim(claim) || live.contains(claim)
        });
        requested.retain(|claim, _| first_seen.contains_key(claim) || live.contains(claim));
        // `answered` was missed by this sweep and grew for the life of the process — one entry
        // per claim this seat ever filed on, on a node whose RSS history is the reason the rest
        // of these lines exist. Dropping a non-live entry cannot cause a double-file: a claim
        // leaves `live` only once no duty names it, and the duty loop refuses anything past
        // `receipt_deadline` regardless of what this set remembers.
        answered.retain(|(claim, _, _, _, _)| live.contains(claim));
        // The same argument for `replayed`, which only the duty loop reads, for a live duty.
        replayed.retain(|(claim, _, _, _, _)| live.contains(claim));
        // Q-7's samplings go with their duties; a dropped handle detaches its task, which runs
        // out and releases its ledger reservation on its own.
        s3_sampled.retain(|(claim, _, _, _, _)| live.contains(claim));
        s3_samplings.retain(|(claim, _, _, _, _), _| live.contains(claim));
        // SEAT-R's replays live exactly as long as this seat's duty on their claim: one whose claim
        // left the duties (licensed, redrawn away from this seat, voided) stops holding a slot.
        let duty_claims: HashSet<Hash64> = duties.iter().map(|duty| duty.claim_id).collect();
        seat_replays.retain_live(|claim| duty_claims.contains(claim));
        // A claim this seat's replay refuted goes to the challenger's half once its duty ends —
        // licensed by other seats, redrawn away, or voided — and not before: noted while the duty
        // stands, the fault gate would stop the full seat's fault finders, which name the leaf.
        // The ledger keeps any address they named; this adds the claim, unaddressed.
        for claim in replay_refuted.iter().filter(|claim| !duty_claims.contains(*claim)) {
            self.note_seat_fault_v1(*claim, 0, 0);
        }
        replay_refuted.retain(|claim| live.contains(claim));
        seat_resumes.retain_live(|claim| duty_claims.contains(claim));
        // N-5's second half, past SEAT-R: a duty this seat was never served that ended because a
        // licence landed on it (`palw_seat_unserved_licence_v1`). P2-6 accuses its producer at once
        // (ADR-0152 §3.8, DA-6, DA-9: "and at once when a licence lands on a claim that did not
        // serve it"): noted here, asked of the chain and queued on the next tick — and filed only
        // if this seat has not accused the claim already, beside its `Unavailable`. "Never served"
        // is `PalwSeatServiceV1`'s: a claim served on any later tick of its duty — a job held, a
        // replay that ran, an authenticated opening — is not in `ended` (the P2-6 review, HIGH).
        let ended = service.ended(|claim| duty_claims.contains(claim));
        if !ended.is_empty() {
            let rows = self
                .consensus_manager
                .consensus()
                .unguarded_session()
                .spawn_blocking(move |c| {
                    c.palw_claim_rows_v1(bond_key, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Seat, true, 500)
                })
                .await
                .map(|read| read.rows)
                .unwrap_or_default();
            for (claim, since) in ended {
                let phase = rows.iter().find(|row| row.claim_id == claim).map(|row| &row.phase);
                if palw_seat_unserved_licence_v1(phase) {
                    let rcore = self.consensus_config.params.palw_rcore_plus_active_at(current_daa);
                    info!(
                        "[{PALW_PANEL}] claim {claim}: licensed while this seat was never served its material (waiting since DAA \
                         {since}) — {} (N-5)",
                        if rcore { "accusing its producer of withholding (P2-6)" } else { "nothing filed below palw_rcore_plus" }
                    );
                    if rcore {
                        accusations.want(claim, current_daa.saturating_add(PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1));
                    }
                }
            }
        }
        // **G14 funnel: follow every claim this node detected to its chain outcome** (accepted, convicted / defaulted, collected,
        // invalidated; deadline and unassigned losses). One read of this seat's rows, only while something is followed.
        // `collected` is inferred from the fold's void-and-slash of a claim that reserved collateral (the row carries no slash
        // amount); a node-local metric, never a consensus input.
        let watched = self.config.telemetry.detection_watched();
        if !watched.is_empty() {
            let rows = self
                .consensus_manager
                .consensus()
                .unguarded_session()
                .spawn_blocking(move |c| {
                    c.palw_claim_rows_v1(bond_key, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Seat, true, 4_096)
                })
                .await
                .map(|read| read.rows)
                .unwrap_or_default();
            for claim in watched {
                if let Some(row) = rows.iter().find(|row| row.claim_id == claim) {
                    let outcome = kaspa_consensus_core::palw_detection_policy_v1::palw_detection_chain_outcome_v1(&row.phase);
                    self.config.telemetry.detection_follow(claim, outcome, row.reserved > 0);
                }
            }
        }
        // The foreign retention keeps what a live claim may still ask for: a C7 claim's window
        // (13,995 DAA on testnet-12 past §4-quater, once the 2M row opens) outlives the directory's
        // age bound.
        // R2: past SEAT-R also every claim this seat held a duty on, until its retention horizon.
        if seat_r {
            retention_liabilities.retain(|_, horizon| *horizon >= current_daa);
            liabilities_unread.retain(|claim| retention_liabilities.contains_key(claim));
            *self.foreign_pinned.lock().unwrap() = palw_seat_retention_pins_v1(&live, &retention_liabilities, current_daa);
        } else {
            *self.foreign_pinned.lock().unwrap() = live.clone();
        }
        // P2-7: and every claim a live lock of this bond still answers for, or an open session
        // demands a unit of now (`disclosure_retain`, read off the chain's locks and sessions, so a
        // restart that forgot the duty book keeps them too — pinned already at the duty read, before
        // any seat duty could prune). Empty below `palw_rcore_plus`.
        if !court_progress.disclosure_retain.is_empty() {
            self.foreign_pinned.lock().unwrap().extend(court_progress.disclosure_retain.iter().copied());
        }
        // H1's other half, past SEAT-R: the served openings of a claim no duty, court or dispute
        // names any more leave the pool. It admits no new `(claim, interval)` pair past its ceiling
        // and nothing else ever removed one, so a node stopped hearing openings — every re-ask of a
        // jammed or refused one included — after a few hundred claims.
        if seat_r {
            pool_books.intervals.resume_mut().retain(|(claim, _), _| live.contains(claim));
            // And the asks' pacing beside them (the second pass, finding 6): one entry a `(claim,
            // interval)` asked for, each C7 segment's included, and nothing else ever removed one.
            requested_intervals.retain(|(claim, _), _| live.contains(claim));
        }
        evidence_kept.retain(|claim| live.contains(claim));
        capture_sampled.retain(|claim| live.contains(claim));
        // A re-send is scheduled for exactly the duties `answered` remembers: one the chain has
        // moved past is never re-sent (`own_receipts_due_v1` asks for the standing duty anyway),
        // and one a reorg brings back is re-sent again while the claim is still live. No
        // deadline: the standing duty is the chain's own window.
        own_filed.retain(|key, _| answered.contains(key));
        own_filed_whole.retain(|key, _| answered.contains(key));
        // ADR-0124 Decision 2: a receipt this seat may still carry itself lives exactly as long
        // as its window; the chain refuses anything past `receipt_deadline` regardless.
        own_receipts.retain(|_, (_, deadline)| current_daa <= *deadline);
        own_receipt_class.retain(|claim, _| own_receipts.contains_key(claim));
        receipt_lanes.retain(|class, lane| lane.funding.is_some() || own_receipt_class.values().any(|c| c == class));
        supplementary_submitted.retain(|claim, _| own_receipts.contains_key(claim));
        supplementary_v3_submitted.retain(|claim, _| receipt_pool_v3.contains_claim(claim) || receipt_pool_v2.contains_claim(claim));
        supplementary_idle.retain(|claim, _| receipt_pool_v3.contains_claim(claim) || receipt_pool_v2.contains_claim(claim));
        licence_idle.retain(|claim, _| receipt_pool_v3.contains_claim(claim) || receipt_pool_v2.contains_claim(claim));
        // Our own executions are only needed while the dispute they support is open.
        own_executions.retain(|claim, _| live.contains(claim));
        submit_attempts.retain(|claim, _| receipt_pool_v2.contains_claim(claim));
        v1_sent.retain(|claim, _| receipt_pool_v2.contains_claim(claim));
        submitted.retain(|_claim, at| current_daa <= at.saturating_add(PANEL_POOL_RETENTION_DAA));
        trace!(
            "[{PALW_PANEL}] tick: {} duties, {} claims pooled (V2), {} (V3)",
            duties.len(),
            receipt_pool_v2.claim_count(),
            receipt_pool_v3.claim_count()
        );
    }
}
