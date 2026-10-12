//! **`VirtualStateProcessor`: dns methods** — moved out of `processor.rs` (2026-10-13, a behaviour-preserving split: the methods are the same; each keeps the reach it had).

use super::*;

impl VirtualStateProcessor {
    /// The EVM template's producer policy (never block validity): is DNS finality confirmed and
    /// keeping up with `sink`? Measured in blue score beyond the anchor's healthy distance — see
    /// [`dns_finality_fresh_for_bridge`] — and read the same way the deposit-claim RPC reads it,
    /// so the RPC never accepts a claim the template would then leave out, or the reverse.
    pub(in crate::pipeline::virtual_processor) fn dns_retired_at(&self, daa: u64) -> bool {
        self.palw_dns_retirement.is_some_and(|r| r.activation.is_active(daa))
    }

    /// Re-derives the [`BondMutation`]s a chain block contributed, from its
    /// retained acceptance data (ADR-0009 Addendum A.4). Deterministic, so it
    /// serves both apply (added) and revert (removed).
    pub(super) fn dns_bond_mutations_for_chain_block(&self, chain_block: BlockHash, bond_view: &ActiveBondView) -> Vec<BondMutation> {
        let accepted_daa_score = self.headers_store.get_header(chain_block).unwrap().daa_score;
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        let txs = self.accepted_txs_of_chain_block(chain_block);
        self.dns_bond_mutations_from_txs(&txs, bond_view, accepted_daa_score, min_bond, unbonding_floor)
    }

    /// Shared tail of the two bond-mutation derivations: map accepted txs to mutations, then —
    /// at and above `unbond_authz_mergeset_activation_daa_score` — drop `Unbond` mutations whose
    /// ML-DSA-87 signature does not verify (incident 2026-08-07).
    ///
    /// The own-body `unbond_request_authorized` block gate never sees a request that arrives via
    /// the MERGESET, so authorization has to be re-established at acceptance. This half checks the
    /// signature; the owner-to-record binding is enforced in `ActiveBondView::apply`/`revert` and
    /// in `stage_dns_bond_mutations`, which have the record. Both halves are symmetric between
    /// apply and revert: this one reads only the block's own accepted txs (no chain view), so it
    /// returns the same set every time it is re-derived.
    pub(super) fn dns_bond_mutations_from_txs(
        &self,
        txs: &[Transaction],
        bond_view: &ActiveBondView,
        accepted_daa_score: u64,
        min_bond: u64,
        unbonding_floor: u64,
    ) -> Vec<BondMutation> {
        let enforce = self.dns_params.as_ref().is_some_and(|p| accepted_daa_score >= p.unbond_authz_mergeset_activation_daa_score);
        let mut muts = bond_mutations_from_accepted_txs(txs, accepted_daa_score, min_bond, unbonding_floor, enforce);

        // 2026-08-11 audit P0: evidence that arrives by MERGE was never signature-checked — the
        // three genuineness rules are block-validity gates over the block's OWN body, while these
        // mutations come from everything it ACCEPTS. Drop any `Slash` whose evidence is not
        // proved, exactly as the H-05 half above drops an unauthorized `Unbond`. See
        // `proved_slash_targets` for why this is symmetric between apply and revert.
        if let Some(params) = self.dns_params.as_ref()
            && accepted_daa_score >= params.dns_activation_daa_score
        {
            let proved = super::super::utxo_validation::proved_slash_targets(
                txs,
                bond_view,
                self.genesis.hash,
                accepted_daa_score,
                params.evidence_window_blocks,
            );
            muts.retain(|m| match m {
                BondMutation::Slash(outpoint, _) => proved.contains(outpoint),
                _ => true,
            });
        }
        if enforce {
            let net_id = self.genesis.hash;
            let signed: std::collections::HashSet<(TransactionOutpoint, Hash64)> = unbond_requests_from_accepted_txs(txs)
                .into_iter()
                .map(|(_, req)| req)
                .filter(|req| {
                    let digest = unbond_request_message(net_id.as_byte_slice(), req.bond_outpoint);
                    matches!(
                        verify_mldsa87_with_context(&req.owner_pubkey, &digest.as_bytes(), &req.signature, UNBOND_REQUEST_CONTEXT),
                        Ok(true)
                    )
                })
                .map(|req| (req.bond_outpoint, validator_id_from_pubkey(&req.owner_pubkey)))
                .collect();
            muts.retain(|m| match m {
                BondMutation::Unbond(outpoint, _, Some(claimed)) => signed.contains(&(*outpoint, *claimed)),
                _ => true,
            });
        }
        muts
    }

    /// The per-bond acceptance floors (min stake amount, min unbonding window) from the network's
    /// `DnsParams`, or `(0, 0)` where the overlay is off — so the bond-acceptance filter is a no-op
    /// on networks without `dns_params`.
    pub(in crate::pipeline::virtual_processor) fn dns_bond_floors(&self) -> (u64, u64) {
        self.dns_params.as_ref().map(|p| (p.min_bond_amount_sompi, p.unbonding_period_blocks)).unwrap_or((0, 0))
    }

    /// [`BondMutation`]s for a block whose acceptance data is held in-memory
    /// (the `KeyNotFound` chain block currently being UTXO-validated, before
    /// its `acceptance_data_store` entry is committed). Mirrors
    /// [`Self::dns_bond_mutations_for_chain_block`] but sources the accepted
    /// txs from the provided acceptance data instead of the store.
    pub(super) fn dns_bond_mutations_from_acceptance(
        &self,
        _chain_block: BlockHash,
        acceptance_data: &AcceptanceData,
        bond_view: &ActiveBondView,
        accepted_daa_score: u64,
    ) -> Vec<BondMutation> {
        let (min_bond, unbonding_floor) = self.dns_bond_floors();
        let txs = self.accepted_txs_from_acceptance_data(acceptance_data);
        self.dns_bond_mutations_from_txs(&txs, bond_view, accepted_daa_score, min_bond, unbonding_floor)
    }

    /// kaspa-pq Phase 10/13 (ADR-0009 §"Decision" / ADR-0018 §H): the DNS finality reorg
    /// gate. Returns `true` (candidate sink allowed) unless the overlay is configured, in
    /// the `Active` rollout stage, has a confirmed anchor, and `candidate` would abandon
    /// that anchor's selected chain. **Inert** on every current network (`dns_params` is
    /// `None`) and outside the `Active` stage.
    ///
    /// `reorg_mode` (per-network, ADR-0018 §H) selects the rule when a candidate exits the
    /// confirmed prefix:
    /// - `HardCheckpoint` (PoC/testnet/devnet): reject any such exit.
    /// - `TwoDimensionalDominance` (mainnet): accept only if the candidate **strictly
    ///   out-Works AND out-Stakes** canonical since their common ancestor `I`, each by its
    ///   emergency margin (non-substitutability — neither dimension alone suffices).
    ///
    /// Safety: each branch's StakeScore-since-`I` is scored under **its own** bond set —
    /// `candidate_bond_view` (the sink-search view already advanced to `candidate`) for the
    /// candidate, and the persisted `stake_bonds_store` (still at `prev_sink`, because the
    /// bond store is written only at the final virtual commit, never during this sink
    /// search) for canonical. Scoring a branch under the wrong view could over-credit it
    /// and wrongly accept a confirmed-history-abandoning reorg. Both branches' acceptance
    /// data is committed by the time the gate runs (the candidate's by
    /// `calculate_utxo_state_relatively`), so the per-branch walks are deterministic.
    pub(in crate::pipeline::virtual_processor) fn dns_reorg_outcome(
        &self,
        candidate: BlockHash,
        prev_sink: BlockHash,
        candidate_bond_view: &ActiveBondView,
    ) -> DnsReorgOutcome {
        // **ADR-0128 Decision 5: the stake reorg gate follows the BFT vote, and it is asked FIRST.**
        //
        // Past the fence at the incumbent sink, a candidate that abandons the DNS-final anchor is
        // refused before the PALW comparator below is consulted — a reorg and an extension alike,
        // which is what the early return below used to skip for a reorg. A candidate this does not
        // refuse (it contains the anchor, the anchor is stale on this node's own chain, or nothing
        // is confirmed) goes on exactly as it does without the fence. See `dns_bft_gate_refusal`.
        let incumbent_daa = match self.headers_store.get_daa_score(prev_sink) {
            Ok(daa) => daa,
            Err(_) => return DnsReorgOutcome::DominanceViolation,
        };
        let dns_retired = self.dns_retired_at(incumbent_daa);
        if !dns_retired && let Some(refusal) = self.dns_bft_gate_refusal(candidate, prev_sink) {
            return refusal;
        }
        // **Unit D, site 4: on a V2 network the deep-reorg gate IS the one comparator.**
        //
        // Every actual V2 reorg uses the same PALW comparator as the virtual tip. Consulting
        // depth, raw blue work or the stake overlay INSTEAD would create a second authority (P0-5).
        // This is not a proof that private work cannot mature: colluding producers and Panel seats
        // can supply receipts on a private branch. RFC-0012 requires analysis and adversarial fork
        // drills under that case, rather than assuming its frontier stays at the fork point.
        //
        // **Scoped to actual REORGS, and the scoping is load-bearing.** A first version ran the
        // comparator on every sink-search candidate and the node wedged with "valid sink must
        // exist": `compare_palw_candidates_v1` ties a child of the sink with the sink on frontier
        // and both weights, so the candidate-hash key decided — and roughly half of all ordinary
        // forward progress was refused as a failed reorg. A block that EXTENDS the sink is not
        // abandoning anything, which is why the existing DNS gate scopes itself the same way (it
        // asks whether the candidate still contains the confirmed anchor).
        if self.palw_state_params_v2.is_some()
            && !matches!(self.reachability_service.try_is_chain_ancestor_of(prev_sink, candidate), Ok(true))
        {
            // **An unweighable candidate LOSES; it does not disappear** (launch blockers §7).
            //
            // This used to require BOTH orders to resolve, and fell through to the DNS gate — which
            // answers `GateInactive`, i.e. allow — whenever either was `None`. `palw_state_walk`
            // refuses a missing delta on purpose ("reading absent data as nothing is forbidden",
            // ADR-0042 Decision 5) and `.ok()?` two layers up turned that refusal into `None`, so
            // the deep-reorg gate failed OPEN on exactly the candidate it could not weigh — which
            // is the one an attacker supplies.
            //
            // Three cases, and none of them is "carry on": a weighed challenger against a weighed
            // incumbent is the comparator's; a challenger this node cannot weigh never beats one it
            // can; and an incumbent this node cannot weigh is a state fault, refused rather than
            // silently downgraded to blue work.
            //
            // **ADR-0178: past rule E's fence the two tips' EXCLUSIVE pasts decide, and the absolute orders are not read.** Bonded
            // participation first once both tips stand `W_p` above the fork, then the economic keys over those pasts; a claim both
            // tips hold decides nothing (`palw_rule_e`). Unweighable refuses, as below; an allowed reorg still meets ADR-0065 D2.
            if self.palw_rule_e_active_at(incumbent_daa) {
                return match self.palw_rule_e_gate_v1(candidate, prev_sink, incumbent_daa) {
                    kaspa_consensus_core::palw_fork_authority_v2::PalwDeepReorgV2::Allow => {
                        self.palw_frontier_provenance_outcome(candidate, prev_sink)
                    }
                    kaspa_consensus_core::palw_fork_authority_v2::PalwDeepReorgV2::Refuse => DnsReorgOutcome::DominanceViolation,
                };
            }
            return match (self.palw_candidate_order_v2(prev_sink), self.palw_candidate_order_v2(candidate)) {
                (Some(incumbent), Some(challenger)) => {
                    // **lane: rcore/f1-forkchoice-attacks — an all-economic tie keeps the incumbent
                    // past the fence.** `decide_deep_reorg_v2` allows a `Greater` challenger, whose
                    // last tie-break is the candidate hash, so a private branch tying the incumbent on
                    // all three economic keys wins the reorg by grinding a higher hash (the fork-choice
                    // attack probes measure it). Past `palw_reorg_strict_economic_win` a reorg is
                    // allowed only on a STRICT economic win. The fence is read at the INCUMBENT's DAA
                    // — a candidate's own score is attacker-chosen — exactly as the confirmed-anchor
                    // TTL and `palw_frontier_provenance_outcome` read it. Below the fence this is
                    // byte-identical to `decide_deep_reorg_v2`.
                    //
                    // **lane: rcore/f1-strictwin-tie — past the fence a SHALLOW tie is GHOSTDAG's.**
                    // This gate judges every non-extension sink move, a one-block sibling switch
                    // included, and "a tie keeps the incumbent" is a rule about arrival order: two
                    // honest nodes that saw sibling tips in different orders each kept their own, the
                    // refused sibling left the virtual's parents, so neither merged the other's tip and
                    // the split outlived every later block that tied (measured through the pipeline:
                    // `strict_win_honest_race`, and rcore/cap-weight 3aa4abec4 before it). So the
                    // fence's rule is `palw_reorg_strict_economic_win_v1`: a strict economic win or
                    // loss exactly as before, and an all-economic tie decided by GHOSTDAG's own order
                    // (blue work, then hash — the order this search pops in) when the incumbent's chain
                    // above the fork point spans at most `PALW_REORG_SHALLOW_TIE_DAA_V1` DAA ticks, the
                    // incumbent kept deeper. The depth walk runs only on a tie, keyed as the fence is
                    // (the incumbent's DAA), and answers `false` on any read it cannot make.
                    //
                    // **rcore/cap-s1 — ADR-0160 F-W has no tie rule of its own.** The capacity lane's copy
                    // (`palw_deep_reorg_capacity_v1` behind `palw_capacity_weight_cap`) was this rule
                    // without the no-DAA-lowering check; cap-s1 keeps this one only. F-W requires
                    // strict-win at or below it (and a build whose rule keeps the check,
                    // `PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1`), so wherever F-W is active this
                    // branch runs, with the check.
                    let incumbent_daa = self.headers_store.get_daa_score(prev_sink).unwrap_or(0);
                    let decision = if self.palw_reorg_strict_economic_win.is_some_and(|f| f.is_active(incumbent_daa)) {
                        kaspa_consensus_core::palw_fork_authority_v2::palw_reorg_strict_economic_win_v1(
                            &incumbent,
                            &challenger,
                            || self.palw_reorg_shallow_ghostdag_win_v1(candidate, prev_sink, incumbent_daa),
                        )
                    } else {
                        kaspa_consensus_core::palw_fork_authority_v2::decide_deep_reorg_v2(&incumbent, &challenger)
                    };
                    match decision {
                        kaspa_consensus_core::palw_fork_authority_v2::PalwDeepReorgV2::Allow => {
                            self.palw_frontier_provenance_outcome(candidate, prev_sink)
                        }
                        kaspa_consensus_core::palw_fork_authority_v2::PalwDeepReorgV2::Refuse => DnsReorgOutcome::DominanceViolation,
                    }
                }
                (Some(_), None) => {
                    info!("deep reorg refused: candidate {candidate} cannot be weighed by this node's PALW authority");
                    DnsReorgOutcome::DominanceViolation
                }
                (None, _) => {
                    info!("deep reorg refused: this node cannot weigh its own sink {prev_sink} — PALW state is incomplete");
                    DnsReorgOutcome::DominanceViolation
                }
            };
        }
        if dns_retired {
            // The only remaining case is an extension. Never fall back to Work/Stake here.
            return if self.palw_candidate_order_v2(candidate).is_some() {
                DnsReorgOutcome::GateInactive
            } else {
                DnsReorgOutcome::DominanceViolation
            };
        }
        let Some(dns_params) = self.dns_params.as_ref() else {
            return DnsReorgOutcome::GateInactive;
        };
        let Ok(state) = self.dns_state_store.read().get() else {
            return DnsReorgOutcome::GateInactive; // no DnsState written yet
        };
        if state.rollout_stage != DnsRolloutStage::Active {
            return DnsReorgOutcome::GateInactive; // gate dormant outside the Active stage
        }
        let confirmed = state.last_dns_confirmed_anchor;
        if confirmed == BlockHash::default() {
            return DnsReorgOutcome::GateInactive; // nothing confirmed yet
        }
        let includes = match self.reachability_service.try_is_chain_ancestor_of(confirmed, candidate) {
            Ok(v) => v,
            Err(_) => {
                debug!(
                    "DNS reorg gate: confirmed anchor {confirmed} has no reachability (behind the pruning point - attestation stalled?); gate is a no-op, subsumed by pruning-point finality"
                );
                true
            }
        };

        // Confirmed-anchor TTL (`dns_params.dns_veto_ttl_daa_score`). Measured on THIS node's own
        // canonical tip — never the candidate's, or an attacker could age out the anchor simply by
        // mining a branch far enough ahead and then presenting it (see the field doc). Evaluated
        // before the ancestor search so a node defending a support-less anchor pays nothing for it.
        //
        // The quantity is "my chain advanced this far with no new confirmation", i.e. exactly the
        // dead-branch wedge: the node keeps producing blocks while the branch's attestation flow is
        // gone, so `advance_dns_confirmation` carries the same anchor forward indefinitely. On a
        // chain that is still confirming, this distance stays at ~`lag + epoch` and never trips.
        let canonical_daa = self.headers_store.get_daa_score(prev_sink).unwrap_or_default();
        let anchor_age = canonical_daa.saturating_sub(state.last_dns_confirmed_anchor_daa_score);
        if !includes && dns_params.confirmed_anchor_is_stale(canonical_daa, state.last_dns_confirmed_anchor_daa_score) {
            warn!(
                "DNS reorg gate: confirmed anchor {confirmed} is STALE — this node's chain advanced {anchor_age} DAA past it (TTL {}) without a new confirmation, so the branch it protects has lost its validator support; releasing the veto for candidate {candidate}",
                dns_params.dns_veto_ttl_daa_score
            );
            return DnsReorgOutcome::ConfirmedAnchorStale;
        }

        // The heavy two-dimensional inputs (common ancestor + per-branch Work/Stake walks)
        // are computed ONLY when the candidate abandons the confirmed prefix AND the
        // network runs the mainnet dominance rule. HardCheckpoint and the includes-anchor
        // case ignore Work/Stake, so they skip the walks entirely.
        //
        // `stake_evaluated` records whether the StakeScore walks actually ran (§5-5 skips them
        // whenever the work dimension already settles the verdict), so the log below reports
        // "not evaluated" instead of printing the placeholder zeros as if they were measurements.
        let mut stake_evaluated = false;
        let inputs = if dns_params.reorg_mode == DnsReorgMode::TwoDimensionalDominance && !includes {
            // Selected-chain common ancestor I. Beyond the reorg horizon the DNS gate ABSTAINS
            // (incident 2026-08-03 §8): it has no Work/Stake deltas to judge on, and the base
            // ledger already refuses reorgs below `virtual_finality_point` in `sink_search`
            // (see `candidate_at_or_above_finality`) plus everything under the pruning point.
            //
            // This used to `return false` (unconditional reject). That is what made a partition
            // PERMANENT rather than merely long: once two branches diverged by more than
            // `max_reorg_horizon_blocks` (300) the gate stopped evaluating anything and rejected
            // outright, so no amount of subsequent work on the other branch could ever be
            // considered. On testnet-22 the branches were ~120k blocks apart by the time the
            // split was noticed — far past the horizon — so the deadlock was already sealed.
            // Abstaining hands the decision to GHOSTDAG + the real finality guard instead of
            // adding a second, unreleasable veto on top of them.
            //
            // The horizon is `gate_horizon_blocks()` — the gate's OWN reach, no longer tied to the
            // economic `max_reorg_horizon_blocks` (300 blocks = 30 s at 10 BPS, which made DNS
            // finality a 30-second property). The search is O(log horizon), so the reach is a
            // policy choice rather than a cost ceiling; see `chain_common_ancestor_within`.
            let candidate_work = self.ghostdag_store.get_blue_work(candidate).unwrap_or_default();
            let canonical_work = self.ghostdag_store.get_blue_work(prev_sink).unwrap_or_default();
            // Exact pre-check, before any ancestor search: blue work is cumulative, so for ANY
            // common ancestor `I`, `candidate_after ≤ canonical_after ⟺ candidate_work ≤
            // canonical_work`. A candidate that does not out-work canonical therefore fails
            // `work_ok` and cannot clear the override (multiplier ≥ 1) — `DominanceViolation` is
            // certain. This is the case that dominates a wedge, because every rejection pushes the
            // candidate's parents back onto the heap and those descend below the sink's work.
            if candidate_work <= canonical_work {
                debug!(
                    "DNS reorg gate: candidate {candidate} does not out-work sink {prev_sink} ({candidate_work} <= {canonical_work}); dominance violation without an ancestor search"
                );
                return DnsReorgOutcome::DominanceViolation;
            }
            let Some(ancestor) = self.chain_common_ancestor_within(candidate, prev_sink, dns_params.gate_horizon_blocks()) else {
                debug!(
                    "DNS reorg gate: candidate {candidate} vs sink {prev_sink} common ancestor is beyond the gate horizon ({} blocks); gate abstains (base-ledger finality point still applies)",
                    dns_params.gate_horizon_blocks()
                );
                return DnsReorgOutcome::GateInactive;
            };
            let ancestor_work = self.ghostdag_store.get_blue_work(ancestor).unwrap_or_default();

            // §5-5 cost mitigation (incident 2026-07-19). The two `stake_score_since_ancestor`
            // calls below are each an O(divergence) chain walk (`collect_stake_contributions_v2`
            // from tip back to the ancestor), plus a full `stake_bonds_store` scan — and
            // `sink_search` runs this gate for EVERY candidate of a heavier branch. That is the
            // amplification the report measured as ever-lengthening resolve times.
            //
            // The WORK dimension alone settles the two cases that dominate a wedge, so decide it
            // first from cheap blue_work lookups and skip the walks when they provably cannot
            // change the outcome:
            //   * the work override already accepts        ⇒ stake is irrelevant;
            //   * `work_ok` is false ⇒ the rule needs BOTH ⇒ certain `DominanceViolation`.
            // Both shortcuts are EXACT, not heuristic: neither branch of `check_dns_reorg_rule`
            // consults the stake values in these cases, so feeding it zeros yields the identical
            // verdict. Only the genuinely contested case (out-works canonical but not by the
            // override ratio) still pays for the walks.
            let candidate_after = candidate_work.saturating_sub(ancestor_work);
            let canonical_after = canonical_work.saturating_sub(ancestor_work);
            let work_ok = candidate_after > canonical_after.saturating_add(dns_params.emergency_work_margin);
            let override_ok = dns_params.emergency_work_override_multiplier > 0 && {
                let (bound, overflowed) = canonical_after.overflowing_mul_u64(dns_params.emergency_work_override_multiplier as u64);
                !overflowed && candidate_after > bound
            };
            stake_evaluated = work_ok && !override_ok;

            let (candidate_stake, canonical_stake) = if stake_evaluated {
                let net_id_hash = self.genesis.hash;
                let net_id = net_id_hash.as_byte_slice();
                // Per-branch bond sets (safety — each branch under its OWN view; see doc comment).
                let candidate_bonds = candidate_bond_view.records();
                let canonical_bonds: Vec<StakeBondRecord> =
                    self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();
                (
                    self.stake_score_since_ancestor(candidate, ancestor, &candidate_bonds, dns_params, net_id),
                    self.stake_score_since_ancestor(prev_sink, ancestor, &canonical_bonds, dns_params, net_id),
                )
            } else {
                (StakeScore(0), StakeScore(0))
            };

            reorg_inputs_since_common_ancestor(
                state.rollout_stage,
                dns_params.reorg_mode,
                includes,
                candidate_work,
                canonical_work,
                ancestor_work,
                candidate_stake,
                canonical_stake,
                dns_params.emergency_work_margin,
                dns_params.emergency_stake_margin,
                dns_params.emergency_work_override_multiplier,
                anchor_age,
                dns_params.dns_veto_ttl_daa_score,
            )
        } else {
            // HardCheckpoint, or candidate keeps the confirmed anchor: Work/Stake unused.
            reorg_inputs_since_common_ancestor(
                state.rollout_stage,
                dns_params.reorg_mode,
                includes,
                BlueWorkType::from_u64(0),
                BlueWorkType::from_u64(0),
                BlueWorkType::from_u64(0),
                StakeScore(0),
                StakeScore(0),
                dns_params.emergency_work_margin,
                dns_params.emergency_stake_margin,
                dns_params.emergency_work_override_multiplier,
                anchor_age,
                dns_params.dns_veto_ttl_daa_score,
            )
        };
        let outcome = check_dns_reorg_rule(&inputs);
        if outcome == DnsReorgOutcome::WorkDominanceOverride {
            // Loud on purpose: the stake veto was deliberately released. Either this node is the
            // minority side of a partition and is (correctly) rejoining the work-dominant chain,
            // or an adversary is sustaining >N/(N+1) of total hashpower across the whole fork.
            // Both are operationally significant and must be visible in the log.
            warn!(
                "DNS reorg gate: partition-liveness override — candidate {candidate} out-works sink {prev_sink} by >{}x since the common ancestor (candidate_work_after={}, canonical_work_after={}); accepting despite an unsatisfied stake dimension (stake: {})",
                dns_params.emergency_work_override_multiplier,
                inputs.candidate_work_after,
                inputs.canonical_work_after,
                if stake_evaluated {
                    format!("candidate={}, canonical={}", inputs.candidate_stake_after.0, inputs.canonical_stake_after.0)
                } else {
                    "not evaluated (work dimension already decisive)".to_owned()
                },
            );
        }
        outcome
    }

    /// Searches for the next valid sink block (SINK = Virtual selected parent). The search is performed
    /// in the inclusive past of `tips`.
    /// The provided `diff` is assumed to initially hold the UTXO diff of `prev_sink` from virtual.
    /// The function returns with `diff` being the diff of the new sink from previous virtual.
    /// In addition to the found sink the function also returns a queue of additional virtual
    /// parent candidates ordered in descending blue work order.
    /// Escape-from-a-dead-branch sink preference (see
    /// [`DnsParams::stake_preference_max_work_deficit_multiplier`]): when this node's OWN chain
    /// has demonstrably lost its DNS overlay (Active stage, confirmed anchor stale past the full
    /// veto TTL), screen `tips` for a branch whose overlay is demonstrably alive at
    /// confirmation grade within a bounded work deficit, and return the best qualifier.
    ///
    /// Screening only — the caller still runs UTXO validation and the reorg gate on the result.
    /// Every input is chain-derived and the tie-break is total (stake desc, work-after desc, hash
    /// asc), so all nodes evaluating the same DAG return the same tip; fork choice stays
    /// memoryless — the hysteresis the boundary needs lives in the verdict's asymmetric bars
    /// (own anchor dead past the FULL TTL vs candidate at FULL confirmation depth), not in state.
    ///
    /// The DNS coinbase-settlement context for MEMPOOL ADMISSION (see
    /// [`kaspa_consensus_core::dns_finality::coinbase_spend_settled`]): the current confirmed
    /// anchor from the node's DnsState singleton, plus the network's long-maturity fallback.
    ///
    /// Policy layer only. The singleton is "state as of this node's last virtual commit", which
    /// differs across nodes by their resolve batching — safe for admission (a policy disagreement
    /// keeps a tx out of a mempool, never out of a block's acceptance), disqualifying for
    /// validity. The consensus call site passes `None` and says why.
    #[cfg(test)]
    pub(in crate::pipeline::virtual_processor) fn dns_coinbase_settlement(&self) -> Option<DnsCoinbaseSettlement> {
        // VSTALL (2026-10-10): never `virtual_stores.read()` here — a caller may already hold it, and a second read queues behind a
        // pending commit upgrade forever. The LKG virtual state is the last committed virtual and takes no lock.
        self.dns_coinbase_settlement_at(Some(self.lkg_virtual_state.load().daa_score))
    }

    /// Use the caller's virtual snapshot. Mempool validation already holds its read guard;
    /// acquiring it again behind a queued commit upgrade deadlocks both validation and commit.
    pub(in crate::pipeline::virtual_processor) fn dns_coinbase_settlement_at(
        &self,
        virtual_daa: Option<u64>,
    ) -> Option<DnsCoinbaseSettlement> {
        let dns_params = self.dns_params.as_ref()?;
        let long_maturity_daa = dns_params.coinbase_settlement_long_maturity_daa;
        if long_maturity_daa == 0 {
            return None;
        }
        // RFC-0012: once the virtual is past the retirement the DNS row is a frozen historical fact (nothing writes it any more),
        // and an anchor frozen below the fence must not keep releasing coinbases early in the mempool while the wallet — which
        // clears the shortcut when the node reports none — shows them pending. The long fallback still releases them by the DAA alone.
        let retired = virtual_daa.is_some_and(|daa| self.dns_retired_at(daa));
        let confirmed_anchor_daa =
            if retired {
                None
            } else {
                self.dns_state_store.read().get().ok().and_then(|s| {
                    (s.last_dns_confirmed_anchor != BlockHash::default()).then_some(s.last_dns_confirmed_anchor_daa_score)
                })
            };
        // **DAA-based maturity only** (the user's Mainnet Decision A, 2026-09-24): no second clock
        // on a coinbase. The long fallback matures it by the DAA alone, or a DNS-final anchor
        // releases it early; the attempt lane's settled anchors are not asked (they still gate
        // locks, liabilities, retiring bonds and the D1 maturity floor).
        Some(DnsCoinbaseSettlement { long_maturity_daa, confirmed_anchor_daa })
    }

    /// Both stake walks run under the CANONICAL bond set: a bond created on the candidate branch
    /// above the ancestor is invisible here, which UNDER-counts the candidate — the conservative
    /// direction for a rule whose false positive is "sink moved onto the wrong branch". The cost
    /// note on the reorg gate (§5-5) does not apply: this path is entered only in the dead-anchor
    /// state, which the cheap staleness check settles first on every healthy resolve.
    pub(super) fn dns_stake_preferred_tip(
        &self,
        prev_sink: BlockHash,
        tips: &[BlockHash],
        finality_point: BlockHash,
    ) -> Option<BlockHash> {
        // Authority is selected by the incumbent, never by a candidate branch.
        if self.headers_store.get_daa_score(prev_sink).ok().is_none_or(|daa| self.dns_retired_at(daa)) {
            return None;
        }
        let dns_params = self.dns_params.as_ref()?;
        let mult = dns_params.stake_preference_max_work_deficit_multiplier;
        if mult == 0 || tips.len() < 2 {
            return None;
        }
        let state = self.dns_state_store.read().get().ok()?;
        if state.rollout_stage != DnsRolloutStage::Active || state.last_dns_confirmed_anchor == BlockHash::default() {
            return None;
        }
        let canonical_daa = self.headers_store.get_daa_score(prev_sink).unwrap_or_default();
        if !dns_params.confirmed_anchor_is_stale(canonical_daa, state.last_dns_confirmed_anchor_daa_score) {
            // Own overlay is alive: symmetric-live contests stay work-decided. This is the cheap
            // early exit every healthy resolve takes.
            return None;
        }
        let own_anchor_age = canonical_daa.saturating_sub(state.last_dns_confirmed_anchor_daa_score);
        let canonical_work = self.ghostdag_store.get_blue_work(prev_sink).unwrap_or_default();
        let net_id_hash = self.genesis.hash;
        let net_id = net_id_hash.as_byte_slice();
        let canonical_bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();

        let mut best: Option<(u128, BlueWorkType, BlockHash)> = None;
        for &tip in tips {
            if tip == prev_sink || !self.reachability_service.try_is_chain_ancestor_of(finality_point, tip).unwrap_or(false) {
                continue;
            }
            let Some(ancestor) = self.chain_common_ancestor_within(tip, prev_sink, dns_params.gate_horizon_blocks()) else {
                // Deeper than the gate horizon: the preference abstains exactly where the veto does.
                continue;
            };
            let ancestor_work = self.ghostdag_store.get_blue_work(ancestor).unwrap_or_default();
            let candidate_work_after = self.ghostdag_store.get_blue_work(tip).unwrap_or_default().saturating_sub(ancestor_work);
            let canonical_work_after = canonical_work.saturating_sub(ancestor_work);
            // Work-deficit bound first: it needs two store reads, the stake walks need O(divergence).
            let (bound, overflowed) = candidate_work_after.overflowing_mul_u64(mult as u64);
            if !overflowed && bound <= canonical_work_after {
                continue;
            }
            let candidate_stake = self.stake_score_since_ancestor(tip, ancestor, &canonical_bonds, dns_params, net_id);
            let canonical_stake = self.stake_score_since_ancestor(prev_sink, ancestor, &canonical_bonds, dns_params, net_id);
            let qualifies = stake_preference_verdict(&StakePreferenceInputs {
                rollout_stage: state.rollout_stage,
                own_anchor_age_daa_score: own_anchor_age,
                veto_ttl_daa_score: dns_params.dns_veto_ttl_daa_score,
                multiplier: mult,
                candidate_work_after,
                canonical_work_after,
                candidate_stake_after: candidate_stake,
                canonical_stake_after: canonical_stake,
                emergency_stake_margin: dns_params.emergency_stake_margin,
                required_stake_depth: dns_params.required_stake_depth,
            });
            if qualifies {
                let better = match &best {
                    None => true,
                    Some((best_stake, best_work, best_hash)) => {
                        (candidate_stake.0, candidate_work_after) > (*best_stake, *best_work)
                            || ((candidate_stake.0, candidate_work_after) == (*best_stake, *best_work) && tip < *best_hash)
                    }
                };
                if better {
                    best = Some((candidate_stake.0, candidate_work_after, tip));
                }
            }
        }
        best.map(|(_, _, tip)| tip)
    }
}
