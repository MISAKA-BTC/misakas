use std::{
    ops::{ControlFlow, DerefMut},
    sync::{Arc, atomic::Ordering},
};

use itertools::Itertools;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockLevel, BlueWorkType,
    blockhash::{BlockHashExtensions, BlockHashes, ORIGIN},
    errors::pruning::{ProofWeakness, PruningImportError, PruningImportResult},
    header::Header,
    pruning::{PruningPointProof, PruningProofMetadata},
};
use kaspa_core::info;
use kaspa_database::{
    prelude::{CachePolicy, ConnBuilder, StoreResultUnitExt},
    utils::DbLifetime,
};
use kaspa_pow::{calc_block_level_check_pow_layer0, calc_block_level_layer0};
use kaspa_utils::vec::VecExtensions;
use parking_lot::RwLock;
use rayon::prelude::*;
use rocksdb::WriteBatch;

use crate::{
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader},
            headers_selected_tip::HeadersSelectedTipStoreReader,
            reachability::{DbReachabilityStore, ReachabilityStoreReader},
            relations::{DbRelationsStore, RelationsStoreReader},
        },
    },
    processes::{
        ghostdag::protocol::GhostdagManager, pruning_proof::GhostdagReaderExt, reachability::inquirer as reachability,
        relations::RelationsStoreExtensions,
    },
};

use super::PruningProofManager;

struct ProofContext {
    _headers_store: Arc<DbHeadersStore>,
    ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    _relations_stores: Vec<DbRelationsStore>,
    _reachability_stores: Vec<Arc<RwLock<DbReachabilityStore>>>,
    _ghostdag_managers:
        Vec<GhostdagManager<DbGhostdagStore, DbRelationsStore, MTReachabilityService<DbReachabilityStore>, DbHeadersStore>>,
    selected_tip_by_level: Vec<BlockHash>,

    pp_header: Arc<Header>,
    _pp_level: BlockLevel,

    _db_lifetime: DbLifetime,
}

struct ProofLevelContext<'a> {
    ghostdag_store: &'a DbGhostdagStore,
    selected_tip: BlockHash,
}

impl ProofLevelContext<'_> {
    /// Returns an option of the hash of the challenger and defender's common ancestor at this level.
    /// If no such ancestor exists, returns None.
    fn find_common_ancestor(challenger: &Self, defender: &Self) -> Option<BlockHash> {
        let mut current = challenger.selected_tip;
        let mut challenger_gd_of_current = challenger.ghostdag_store.get_compact_data(current).unwrap();
        loop {
            if defender.ghostdag_store.has(current).unwrap() {
                break Some(current);
            } else {
                current = challenger_gd_of_current.selected_parent;
                if current.is_origin() {
                    break None;
                }
                challenger_gd_of_current = challenger.ghostdag_store.get_compact_data(current).unwrap();
            };
        }
    }

    /// Returns the blue work difference between the level selected tip and `ancestor`
    fn blue_work_diff(&self, ancestor: BlockHash) -> BlueWorkType {
        self.ghostdag_store
            .get_blue_work(self.selected_tip)
            .unwrap()
            .saturating_sub(self.ghostdag_store.get_blue_work(ancestor).unwrap())
    }

    /// Returns the overall blue score for this level (essentially the level selected tip blue score)
    fn blue_score(&self) -> u64 {
        self.ghostdag_store.get_blue_score(self.selected_tip).unwrap()
    }
}

/// The maximum number of header slots a pruning proof may carry before ANY inference is spawned
/// (ADR-0041 Decision 3).
///
/// On a PALW network each proof header costs one full LLM inference, serialized on a single spawn
/// gate, so an uncapped proof is a remote stall — a peer can replay the network's own historical
/// headers, valid PoW and all, and buy the victim tens of hours of work for a few MB (audit H1).
///
/// Derived from the validator's OWN params, never from the proof's claimed `daa_score`: the honest
/// builder's per-level working set is `2 · pruning_proof_m` (`build.rs` VecDeque capacity and cache
/// policy), and there are `max_block_level + 1` levels. The cap is deliberately GENEROUS — the
/// honest proof is a pyramid whose high levels hold only `O(m)` headers total, so a real proof sits
/// far under this ceiling (≈ 9 000 slots at testnet-11 against a ceiling of 502 000), while a
/// 1 GiB junk message sits far over it. It bounds the pathological case; the tight bound is
/// sampling (ADR-0041 Decision 1), which this cap precedes rather than replaces.
///
/// A free function, not a method, so it is checkable by a unit test with no `PruningProofManager`.
pub(super) fn proof_header_budget(max_block_level: BlockLevel, pruning_proof_m: u64) -> usize {
    (max_block_level as usize + 1).saturating_mul(2usize.saturating_mul(pruning_proof_m as usize))
}

/// Refuses a proof whose total header count exceeds [`proof_header_budget`]. Counts SLOTS
/// (`sum(level.len())`), because that is what the per-header PoW loop iterates — a header duplicated
/// across levels is a slot the loop still visits.
pub(super) fn check_proof_header_budget(
    proof: &PruningPointProof,
    max_block_level: BlockLevel,
    pruning_proof_m: u64,
) -> PruningImportResult<()> {
    let total: usize = proof.iter().map(|level| level.len()).sum();
    let cap = proof_header_budget(max_block_level, pruning_proof_m);
    if total > cap {
        return Err(PruningImportError::PruningProofOversized { total, cap });
    }
    Ok(())
}

/// End index of the PoW batch that starts at `start` (ADR-0041 Decision 2).
///
/// At most `batch` headers, stopping at the first one that fails the cheap shape gate — so a header
/// the walk is going to reject never buys an inference, and neither does anything behind it. Always
/// at least `start + 1`: the caller has already gated `start` itself and needs its result now.
///
/// Split out of the walk because this arithmetic is the only part of the batching that can be
/// wrong, and inside the walk it is unreachable by a test.
fn pow_batch_end(start: usize, len: usize, batch: usize, gated_ok: impl Fn(usize) -> bool) -> usize {
    debug_assert!(start < len, "the walk only asks about a header it is standing on");
    // `batch.max(1)` is belt and braces: `inference_concurrency()` already refuses 0, and a 0 here
    // would return `start`, hand the walk an empty window and panic it on the pop below.
    let limit = (start + batch.max(1)).min(len);
    start + 1 + (start + 1..limit).take_while(|&k| gated_ok(k)).count()
}

impl ProofContext {
    /// Build the full context from the proof
    fn from_proof(
        ppm: &PruningProofManager,
        proof: &PruningPointProof,
        log_validating: bool,
    ) -> Result<ControlFlow<(), ProofContext>, PruningImportError> {
        if proof.len() != ppm.max_block_level as usize + 1 {
            return Err(PruningImportError::ProofNotEnoughLevels(ppm.max_block_level as usize + 1));
        }

        if proof[0].is_empty() {
            return Err(PruningImportError::PruningProofNotEnoughHeaders);
        }

        // ADR-0041 Decision 3: bound the number of header slots BEFORE the per-header PoW loop below
        // spawns one inference each. This is the single chokepoint both entry points reach
        // (`validate_pruning_point_proof` and `..._standalone` both go through `from_proof`).
        check_proof_header_budget(proof, ppm.max_block_level, ppm.pruning_proof_m)?;

        let ghostdag_k = ppm.ghostdag_k;

        let headers_estimate = ppm.estimate_proof_unique_size(proof);

        //
        // Initialize stores
        //

        let (db_lifetime, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let cache_policy = CachePolicy::Count(2 * ppm.pruning_proof_m as usize);
        let headers_store =
            Arc::new(DbHeadersStore::new(db.clone(), CachePolicy::Count(headers_estimate), CachePolicy::Count(headers_estimate)));
        let ghostdag_stores = (0..=ppm.max_block_level)
            .map(|level| Arc::new(DbGhostdagStore::new(db.clone(), level, cache_policy, cache_policy)))
            .collect_vec();
        let mut relations_stores =
            (0..=ppm.max_block_level).map(|level| DbRelationsStore::new(db.clone(), level, cache_policy, cache_policy)).collect_vec();
        let reachability_stores = (0..=ppm.max_block_level)
            .map(|level| Arc::new(RwLock::new(DbReachabilityStore::with_block_level(db.clone(), cache_policy, cache_policy, level))))
            .collect_vec();

        let reachability_services = (0..=ppm.max_block_level)
            .map(|level| MTReachabilityService::new(reachability_stores[level as usize].clone()))
            .collect_vec();

        let ghostdag_managers = ghostdag_stores
            .iter()
            .cloned()
            .enumerate()
            .map(|(level, ghostdag_store)| {
                GhostdagManager::with_level(
                    ppm.genesis_hash,
                    ghostdag_k,
                    ghostdag_store,
                    relations_stores[level].clone(),
                    headers_store.clone(),
                    reachability_services[level].clone(),
                    level as BlockLevel,
                    ppm.max_block_level,
                    // ADR-0060: ε work for heartbeat blocks in proof validation as in building.
                    matches!(ppm.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)),
                )
            })
            .collect_vec();

        {
            let mut batch = WriteBatch::default();
            for level in 0..=ppm.max_block_level {
                let level = level as usize;
                reachability::init(reachability_stores[level].write().deref_mut()).unwrap();
                relations_stores[level].insert_batch(&mut batch, ORIGIN, BlockHashes::new(vec![])).unwrap();
                ghostdag_stores[level].insert(ORIGIN, ghostdag_managers[level].origin_ghostdag_data()).unwrap();
            }

            db.write(batch).unwrap();
        }

        let proof_pp_header = proof[0].last().expect("checked if empty").clone();
        // Gate this peer-supplied header BEFORE its PoW is computed (audit P0-1 / P0-2): this is the
        // first Layer-0 PoW the proof path runs, and it is on a fully peer-controlled header
        // (`proof[0].last()`, a level-0 block). It is re-checked inside the per-level loop below; the
        // redundant call is cheap and keeps this earliest site from being an ungated path to the
        // finalizer.
        ppm.check_proof_header_shape(&proof_pp_header, 0)?;
        let proof_pp_level = calc_block_level_layer0(&proof_pp_header, &ppm.network_id, ppm.max_block_level);
        let proof_pp = proof_pp_header.hash;

        //
        // Populate stores
        //

        let mut selected_tip_by_level = vec![None; ppm.max_block_level as usize + 1];
        for level in (0..=ppm.max_block_level).rev() {
            // Before processing this level, check if the process is exiting so we can end early
            if ppm.is_consensus_exiting.load(Ordering::Relaxed) {
                return Ok(ControlFlow::Break(()));
            }

            if log_validating {
                info!("Validating level {level} from the pruning point proof ({} headers)", proof[level as usize].len());
            }
            let level_idx = level as usize;
            let mut selected_tip =
                proof[level as usize].first().map(|header| header.hash).ok_or(PruningImportError::PruningProofNotEnoughHeaders)?;
            // ADR-0041 Decision 2. On a PALW network `calc_block_level_check_pow_layer0` is a full
            // LLM inference, and it is a pure function of the header — so a bounded batch of them
            // runs in parallel and the loop consumes the results in order.
            //
            // The loop itself stays strictly sequential: every line after the PoW checks mutates
            // stores whose order IS the validation. Errors stay in header order too, so nothing
            // about what is accepted changes — only the wall clock. Up to `batch` inferences may be
            // spent on headers a later error makes moot; that is the price of the parallelism, and
            // it is bounded by the same constant rather than by the level's length.
            let level_headers = &proof[level as usize];
            let batch = kaspa_pow::palw::inference_concurrency();
            let mut pow_ahead: std::collections::VecDeque<(BlockLevel, bool)> = std::collections::VecDeque::new();

            for (i, header) in level_headers.iter().enumerate() {
                // Gate the peer-supplied proof header BEFORE its PoW is computed (audit P0-1 / P0-2).
                // The order matters: `calc_block_level_check_pow_layer0` runs the Layer-1 finalizer,
                // whose PALW arm escalates a REGISTERED runtime's persistent failure into a node-wide
                // panic (a missing runtime is a failed PoW since ADR-0042 Decision 4) and whose
                // unknown-id path was a remote panic before it was made total — so a peer-chosen
                // `pow_algo_id` must be rejected here, not after: an inference-priced id this network
                // never demands must not be able to spend this node's inference budget either.
                // `check_algo_id` enforces the SAME per-DAA
                // required-algo rule the main pipeline applies (not the looser `check_algo_id_known`),
                // because proof-only headers below the pruning point are never re-processed by the
                // main pipeline; parentless roots are exempt from that rule exactly as the pipeline
                // exempts genesis. It also refuses a malformed `palw_commitment` before persistence.
                ppm.check_proof_header_shape(header, level)?;

                if pow_ahead.is_empty() {
                    // Refill with this header plus the following ones that also pass the shape gate,
                    // up to the concurrency bound. Stopping the scan at the first shape failure is
                    // what keeps the gate above meaningful in the batched case: that header, and
                    // everything after it, stays unpriced until the loop reaches it and rejects it.
                    let end = pow_batch_end(i, level_headers.len(), batch, |k| {
                        ppm.check_proof_header_shape(&level_headers[k], level).is_ok()
                    });
                    let window = &level_headers[i..end];
                    // One header is the overwhelmingly common case — steady state, and every
                    // network whose PoW is a hash — so keep it off the thread pool entirely.
                    if window.len() == 1 {
                        pow_ahead.push_back(calc_block_level_check_pow_layer0(header, &ppm.network_id, ppm.max_block_level));
                    } else {
                        pow_ahead.extend(
                            window
                                .par_iter()
                                .map(|h| calc_block_level_check_pow_layer0(h, &ppm.network_id, ppm.max_block_level))
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                let (header_level, pow_passes) = pow_ahead.pop_front().expect("the refill above leaves at least this header");

                if header_level < level {
                    return Err(PruningImportError::PruningProofWrongBlockLevel(header.hash, header_level, level));
                }
                if !ppm.skip_proof_of_work && !pow_passes {
                    return Err(PruningImportError::ProofOfWorkFailed(header.hash, level));
                }

                headers_store.insert(header.hash, header.clone(), header_level).idempotent().unwrap();

                // Filter out parents that do not appear at the pruning proof:
                let parents = ppm
                    .parents_manager
                    .parents_at_level(header, level)
                    .iter()
                    .copied()
                    .filter(|parent| ghostdag_stores[level_idx].has(*parent).unwrap())
                    .collect_vec();

                // Only the first block at each level is allowed to have no known parents
                if parents.is_empty() && i != 0 {
                    return Err(PruningImportError::PruningProofHeaderWithNoKnownParents(header.hash, level));
                }

                for &parent in parents.iter() {
                    if headers_store.get_header(parent).unwrap().blue_work >= header.blue_work {
                        return Err(PruningImportError::PruningProofInconsistentBlueWork(header.hash, level));
                    }
                }

                let parents: BlockHashes = parents.push_if_empty(ORIGIN).into();

                if relations_stores[level_idx].has(header.hash).unwrap() {
                    return Err(PruningImportError::PruningProofDuplicateHeaderAtLevel(header.hash, level));
                }

                relations_stores[level_idx].insert(header.hash, parents.clone()).unwrap();
                let ghostdag_data = Arc::new(ghostdag_managers[level_idx].ghostdag(&parents));
                ghostdag_stores[level_idx].insert(header.hash, ghostdag_data.clone()).unwrap();

                // Update the selected tip
                selected_tip = ghostdag_managers[level_idx].find_selected_parent([selected_tip, header.hash]);

                let mut level_reachability = reachability_stores[level_idx].write();
                let mut reachability_mergeset = ghostdag_data
                    .unordered_mergeset_without_selected_parent()
                    .filter(|hash| level_reachability.has(*hash).unwrap())
                    .collect_vec()
                    .into_iter();

                reachability::add_block(
                    level_reachability.deref_mut(),
                    header.hash,
                    ghostdag_data.selected_parent,
                    &mut reachability_mergeset,
                )
                .unwrap();

                if selected_tip == header.hash {
                    reachability::hint_virtual_selected_parent(level_reachability.deref_mut(), header.hash).unwrap();
                }
                drop(level_reachability);
            }

            if level < ppm.max_block_level {
                let block_at_depth_m_at_next_level = ghostdag_stores[level_idx + 1]
                    .block_at_depth(selected_tip_by_level[level_idx + 1].unwrap(), ppm.pruning_proof_m)
                    .unwrap();
                if !relations_stores[level_idx].has(block_at_depth_m_at_next_level).unwrap() {
                    return Err(PruningImportError::PruningProofMissingBlockAtDepthMFromNextLevel(level, level + 1));
                }
            }

            // The selected tip at a given level must be anchored to the pruning point:
            // - At levels ≤ the pruning-point level, the selected tip must be the pruning point itself.
            // - At higher levels, it must be a parent of the pruning point at that level.
            if level <= proof_pp_level {
                if selected_tip != proof_pp {
                    return Err(PruningImportError::PruningProofSelectedTipIsNotThePruningPoint(selected_tip, level));
                }
            } else if !ppm.parents_manager.parents_at_level(&proof_pp_header, level).contains(&selected_tip) {
                return Err(PruningImportError::PruningProofSelectedTipNotParentOfPruningPoint(selected_tip, level));
            }

            let tip_blue_score = ghostdag_stores[level_idx].get_blue_score(selected_tip).expect("tip expected");
            let level_root = proof[level_idx].first().expect("checked earlier").hash;
            if level_root != ppm.genesis_hash && tip_blue_score < 2 * ppm.pruning_proof_m {
                return Err(PruningImportError::PruningProofSelectedTipNotEnoughBlueScore(selected_tip, level, tip_blue_score));
            }

            selected_tip_by_level[level_idx] = Some(selected_tip);
        }

        let selected_tip_by_level = selected_tip_by_level.into_iter().map(|selected_tip| selected_tip.unwrap()).collect();

        let ctx = ProofContext {
            _db_lifetime: db_lifetime,
            _headers_store: headers_store,
            ghostdag_stores,
            _relations_stores: relations_stores,
            _reachability_stores: reachability_stores,
            _ghostdag_managers: ghostdag_managers,
            selected_tip_by_level,
            pp_header: proof_pp_header,
            _pp_level: proof_pp_level,
        };

        Ok(ControlFlow::Continue(ctx))
    }

    /// Returns a per-level context
    fn level(&self, level: BlockLevel) -> ProofLevelContext<'_> {
        ProofLevelContext {
            ghostdag_store: &self.ghostdag_stores[level as usize],
            selected_tip: self.selected_tip_by_level[level as usize],
        }
    }
}

impl PruningProofManager {
    /// Validates an incoming pruning point proof against the current consensus.
    ///
    /// The function reconstructs temporary stores for both the
    /// challenger proof and the current (defender) consensus, validates all
    /// selected tips, and compares blue work including pruning-period work.
    ///
    /// Returns `Ok(())` if the proof is valid and superior, or an appropriate
    /// `PruningImportError` otherwise.
    /// Validates an incoming proof's own soundness, WITHOUT comparing it to the local chain.
    ///
    /// Same structural work as [`Self::validate_pruning_point_proof`] — level DAGs rebuilt, headers
    /// and PoW checked, selected tips validated — minus `compare_proofs_inner`.
    ///
    /// That comparison asks "is this better than what I already have?", and answering it requires
    /// the local chain to be authoritative. During bootstrap recovery it is not: the chain held is
    /// provisional, adopted by a race and not yet acted upon, and the whole point is to weigh a
    /// stranger's chain against it. Asked comparatively, an unrelated history is rejected with
    /// "no shared blocks with the known level DAGs" before its own validity is ever reported —
    /// which made a validated proof an unreachable precondition for the permit that needed one.
    ///
    /// So soundness is established here and superiority is judged separately, on figures this node
    /// derived, by `decide_commit` and `authorize_bootstrap_recovery`. Nothing is skipped; the two
    /// questions are simply asked one at a time. Callers syncing normally must keep using the
    /// comparative form — being handed a worse chain is exactly what it protects them from.
    pub fn validate_pruning_point_proof_standalone(&self, proof: &PruningPointProof) -> PruningImportResult<()> {
        ProofContext::from_proof(self, proof, true)?.continue_value().ok_or(PruningImportError::PruningValidationInterrupted)?;
        Ok(())
    }

    pub fn validate_pruning_point_proof(
        &self,
        proof: &PruningPointProof,
        proof_metadata: &PruningProofMetadata,
    ) -> PruningImportResult<()> {
        // Initialize the stores for the incoming pruning proof (the challenger)
        let challenger =
            ProofContext::from_proof(self, proof, true)?.continue_value().ok_or(PruningImportError::PruningValidationInterrupted)?;

        // Get the proof for the current consensus (the defender) and recreate the stores for it
        // This is expected to be fast because if a proof exists, it will be cached.
        let defender_proof = self.get_pruning_point_proof();
        let defender = ProofContext::from_proof(self, &defender_proof, false)
            .expect("local")
            .continue_value()
            .ok_or(PruningImportError::PruningValidationInterrupted)?;

        Ok(self.compare_proofs_inner(
            defender,
            challenger,
            self.headers_selected_tip_store.read().get().unwrap().blue_work,
            proof_metadata.relay_block_blue_work,
        )?)
    }

    /// Compares two MLS pruning proofs and determines whether the challenger supersedes the defender.
    ///
    /// The comparison is performed level-by-level, considering only levels that satisfy the
    /// ≥2M threshold. When a common ancestor exists at a given level, the proofs are
    /// compared by their accumulated blue work from that ancestor onward, including the
    /// respective pruning-period work; otherwise, if no common ancestor is found, the
    /// challenger is considered better only if it possesses a qualifying level where the
    /// defender does not.
    ///
    /// The challenger is considered better only if it is *strictly* superior according to
    /// these criteria. In case of equality, or when no strict advantage can be established,
    /// the defender is favored to preserve stability.
    fn compare_proofs_inner(
        &self,
        defender: ProofContext,
        challenger: ProofContext,
        defender_relay_blue_work: BlueWorkType,
        challenger_relay_blue_work: BlueWorkType,
    ) -> Result<(), ProofWeakness> {
        // Both pruning-period terms are measured over the SAME span of history —
        // from one common cut (the lower of the two pruning points' cumulative blue
        // work) up to each side's tip claim. Subtracting each side's OWN pruning
        // point (the previous behavior) compared different windows: a defender with
        // a stale pruning point kept the shared heavy history inside its window
        // while the challenger's window had advanced past it, so the shared work
        // counted for the defender in full and for the challenger only via the
        // sampled level chains. On testnet-10 (defender offline 9 days across a
        // ~75000x difficulty cliff) that credited the defender ~10^5x extra shared
        // work and rejected the strictly heavier majority chain forever ("weaker
        // than local"). The challenger's span beyond its proven pruning point stays
        // a CLAIM — verified after acceptance, exactly as before — and ties still
        // favor the defender below, so the conservative bias direction is intact.
        let period_cut = defender.pp_header.blue_work.min(challenger.pp_header.blue_work);
        let (defender_pruning_period_work, challenger_claimed_pruning_period_work) =
            same_span_pruning_period_works(period_cut, defender_relay_blue_work, challenger_relay_blue_work);

        for level in 0..=self.max_block_level {
            // Init level ctxs
            let challenger_level_ctx = challenger.level(level);
            let defender_level_ctx = defender.level(level);

            // Next check is to see if the challenger's proof is "better" than the defender's
            // Step 1 - look only at levels that have a full proof (at least 2M blocks)
            if challenger_level_ctx.blue_score() < 2 * self.pruning_proof_m {
                continue;
            }

            // Step 2 - if a common ancestor exists between the challenger and defender proofs,
            // compare their accumulated blue work from that ancestor onward.
            // The challenger proof is better iff the blue work difference from the ancestor
            // to the challenger's selected tip, plus its pruning-period work, is strictly
            // greater than the corresponding defender value.
            if let Some(common_ancestor) = ProofLevelContext::find_common_ancestor(&challenger_level_ctx, &defender_level_ctx) {
                if defender_level_ctx.blue_work_diff(common_ancestor).saturating_add(defender_pruning_period_work)
                    >= challenger_level_ctx.blue_work_diff(common_ancestor).saturating_add(challenger_claimed_pruning_period_work)
                {
                    return Err(ProofWeakness::InsufficientBlueWork);
                }

                return Ok(());
            }
        }

        if defender.pp_header.hash == self.genesis_hash {
            // If the challenger has better tips and the defender's pruning point is still
            // genesis, we consider the challenger to be better.
            return Ok(());
        }

        // If we got here it means there's no level with shared blocks
        // between the challenger and the defender. In this case we
        // consider the challenger to be better if it has at least one level
        // with 2M blue blocks where the defender doesn't.
        for level in (0..=self.max_block_level).rev() {
            if challenger.level(level).blue_score() < 2 * self.pruning_proof_m {
                continue;
            }
            if defender.level(level).blue_score() < 2 * self.pruning_proof_m {
                return Ok(());
            }
        }

        drop(challenger);
        drop(defender);

        Err(ProofWeakness::NotEnoughHeaders)
    }

    /// Compares two MLS pruning proofs and determines whether the challenger supersedes the defender.
    ///
    /// See [`PruningProofManager::compare_proofs_inner`] for more details.
    ///
    /// Exposed here for local revalidation needs.
    pub(crate) fn _compare_proofs(
        &self,
        defender: &PruningPointProof,
        challenger: &PruningPointProof,
        defender_relay_blue_work: BlueWorkType,
        challenger_relay_blue_work: BlueWorkType,
    ) -> ControlFlow<(), Result<(), ProofWeakness>> {
        ControlFlow::Continue(self.compare_proofs_inner(
            ProofContext::from_proof(self, defender, false).expect("local")?,
            ProofContext::from_proof(self, challenger, false).expect("local")?,
            defender_relay_blue_work,
            challenger_relay_blue_work,
        ))
    }
}

/// The two L0 "pruning period" terms of the proof comparison, measured over the
/// SAME span of history: from `period_cut` (the lower of the two proofs' pruning
/// point cumulative blue works) up to each side's tip claim. See the call site in
/// [`PruningProofManager::compare_proofs_inner`] for why one common cut is load-
/// bearing: per-side cuts credit a stale defender with shared history the
/// challenger's window has already advanced past (the testnet-10 permanent
/// "weaker than local" rejection of the strictly heavier majority chain).
fn same_span_pruning_period_works(
    period_cut: BlueWorkType,
    defender_relay_blue_work: BlueWorkType,
    challenger_relay_blue_work: BlueWorkType,
) -> (BlueWorkType, BlueWorkType) {
    (defender_relay_blue_work.saturating_sub(period_cut), challenger_relay_blue_work.saturating_sub(period_cut))
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use kaspa_consensus_core::header::Header;
    use std::sync::Arc;

    fn level(n: usize) -> Vec<Arc<Header>> {
        // The content is irrelevant to the budget check — it counts slots, not headers. Distinct
        // hashes only so the vector is not obviously degenerate.
        (0..n).map(|i| Arc::new(Header::from_precomputed_hash((i as u64).into(), vec![]))).collect()
    }

    /// The cap is derived from the validator's own params and refuses an oversized proof BEFORE any
    /// PoW — the whole point being that the refusal costs nothing.
    #[test]
    fn an_oversized_proof_is_refused_at_the_budget() {
        let (mbl, m) = (250u8, 1_000u64);
        let cap = proof_header_budget(mbl, m);
        assert_eq!(cap, (250 + 1) * 2 * 1_000, "the ceiling is (max_block_level+1) x 2 x m");

        // A realistic proof — a pyramid, ~9 full base levels and tiny tops — sits far under it.
        let mut realistic: PruningPointProof = vec![level(1); mbl as usize + 1];
        for lvl in realistic.iter_mut().take(9) {
            *lvl = level(1_000);
        }
        let realistic_total: usize = realistic.iter().map(|l| l.len()).sum();
        assert!(realistic_total < cap, "an honest proof ({realistic_total}) must never hit the cap ({cap})");
        assert!(check_proof_header_budget(&realistic, mbl, m).is_ok());

        // Exactly at the cap passes; one slot over is refused, and the error names both numbers.
        let at_cap: PruningPointProof = vec![level(2 * m as usize); mbl as usize + 1];
        assert_eq!(at_cap.iter().map(|l| l.len()).sum::<usize>(), cap);
        assert!(check_proof_header_budget(&at_cap, mbl, m).is_ok());

        let mut over = at_cap;
        over[0].push(Arc::new(Header::from_precomputed_hash(u64::MAX.into(), vec![])));
        match check_proof_header_budget(&over, mbl, m) {
            Err(PruningImportError::PruningProofOversized { total, cap: reported }) => {
                assert_eq!(total, cap + 1);
                assert_eq!(reported, cap);
            }
            other => panic!("expected PruningProofOversized, got {other:?}"),
        }
    }

    /// The count is SLOTS, not distinct headers: a header duplicated across levels is a slot the PoW
    /// loop still visits, so it must still count against the cap. A test that used distinct headers
    /// per level would let a duplicate-padding attack slip the cap.
    #[test]
    fn the_budget_counts_slots_not_distinct_headers() {
        let (mbl, m) = (250u8, 1_000u64);
        let cap = proof_header_budget(mbl, m);
        let one = Arc::new(Header::from_precomputed_hash(7u64.into(), vec![]));
        // The SAME header, cap+1 times, spread one-per-level then piled onto level 0.
        let mut proof: PruningPointProof = vec![vec![]; mbl as usize + 1];
        for _ in 0..=cap {
            proof[0].push(one.clone());
        }
        assert_eq!(proof.iter().map(|l| l.len()).sum::<usize>(), cap + 1);
        assert!(
            matches!(check_proof_header_budget(&proof, mbl, m), Err(PruningImportError::PruningProofOversized { .. })),
            "duplicate headers are still slots and must still be capped"
        );
    }

    /// The ceiling never wraps, even at the widest params a network could carry.
    #[test]
    fn the_budget_saturates_rather_than_wraps() {
        assert_eq!(proof_header_budget(u8::MAX, u64::MAX), usize::MAX);
        assert_eq!(proof_header_budget(0, 1_000), 2_000, "one level still gets its own working set");
    }
}

#[cfg(test)]
mod period_window_tests {
    use super::*;

    fn bw(v: u64) -> BlueWorkType {
        BlueWorkType::from_u64(v)
    }

    /// Contemporaneous pruning points (the healthy regime): one common cut equals
    /// the previous per-side subtraction, so the comparison is unchanged.
    #[test]
    fn contemporaneous_pps_match_previous_semantics() {
        let (def_pp, chal_pp) = (bw(1_000), bw(1_000));
        let cut = def_pp.min(chal_pp);
        let (d, c) = same_span_pruning_period_works(cut, bw(1_500), bw(1_400));
        assert_eq!(d, bw(500));
        assert_eq!(c, bw(400));
    }

    /// The testnet-10 shape: the defender's pruning point is stale (predates a
    /// shared heavy segment S), the challenger's has advanced past S. Per-side cuts
    /// gave defender S+its own post-fork work vs challenger's post-fork work alone
    /// (S counted once, for the defender) — the stale side won on shared history.
    /// One common cut counts S for BOTH, so the decision falls to the real
    /// post-cut difference and the heavier challenger wins.
    #[test]
    fn stale_defender_pp_no_longer_keeps_shared_history_to_itself() {
        // Shared history: cumulative work 1_000 at the stale defender pp, then a
        // heavy shared segment S = 1_000_000 up to the fork. Defender adds 300
        // after the fork; challenger adds 3_000 and its pp advanced past S.
        let def_pp = bw(1_000);
        let chal_pp = bw(1_000_000); // within/past the shared heavy segment
        let def_tip = bw(1_000 + 1_000_000 + 300);
        let chal_tip = bw(1_000 + 1_000_000 + 3_000);

        // Previous per-side semantics: defender period dwarfs the challenger's.
        let old_def = def_tip.saturating_sub(def_pp); // S + 300
        let old_chal = chal_tip.saturating_sub(chal_pp); // ~4_000
        assert!(old_def > old_chal, "the old windows made the stale defender look heavier");

        // Same-span semantics: both periods include S; the heavier tip wins.
        let cut = def_pp.min(chal_pp);
        let (d, c) = same_span_pruning_period_works(cut, def_tip, chal_tip);
        assert!(c > d, "same-span windows let the strictly heavier challenger win");
        assert_eq!(c.saturating_sub(d), bw(2_700), "decided by the true post-fork difference");
    }

    /// Symmetry: a stale CHALLENGER pruning point gets the same treatment (the cut
    /// is the lower of the two), so the fix is not a challenger-favoring bias.
    #[test]
    fn stale_challenger_pp_is_symmetric() {
        let def_pp = bw(1_000_000);
        let chal_pp = bw(1_000);
        let cut = def_pp.min(chal_pp);
        let (d, c) = same_span_pruning_period_works(cut, bw(1_003_000), bw(1_001_300));
        assert!(d > c, "the heavier defender still wins under the common cut");
    }
}

#[cfg(test)]
mod pow_batch_tests {
    use super::pow_batch_end;
    use std::collections::VecDeque;

    #[test]
    fn a_batch_of_one_is_exactly_the_current_header() {
        // The default, and every network whose PoW is a hash: no batching, no thread pool.
        for start in 0..5 {
            assert_eq!(pow_batch_end(start, 5, 1, |_| true), start + 1);
        }
    }

    #[test]
    fn a_batch_is_clipped_by_the_level_and_by_the_gate() {
        assert_eq!(pow_batch_end(0, 10, 4, |_| true), 4, "a full batch");
        assert_eq!(pow_batch_end(8, 10, 4, |_| true), 10, "clipped by the end of the level");
        assert_eq!(pow_batch_end(9, 10, 4, |_| true), 10, "the last header alone");
        assert_eq!(pow_batch_end(0, 10, 4, |k| k != 2), 2, "clipped by a gate failure inside the batch");
        assert_eq!(pow_batch_end(0, 10, 4, |k| k != 1), 1, "a gate failure at the very next header");
        assert_eq!(pow_batch_end(0, 10, 4, |k| k != 0), 4, "the START header's gate is the caller's business");
    }

    #[test]
    fn a_zero_batch_still_yields_a_header() {
        // `inference_concurrency()` refuses 0, but an empty window would panic the walk's `pop`,
        // so the helper must not be the place that trusts its caller.
        assert_eq!(pow_batch_end(3, 10, 0, |_| true), 4);
    }

    /// The walk must consume exactly one result per header, in header order, must never price a
    /// header the gate rejects, and must not speculate further than `batch` past where it stops.
    ///
    /// Simulates the refill loop over every combination of batch size, level length, and the two
    /// ways a walk can stop: a gate failure (which the batch scan can see, so it prices nothing
    /// beyond it) and a PoW-derived failure (which it cannot, so it may have priced up to a batch
    /// of headers the walk never reaches — the bounded waste Decision 2 pays for its parallelism).
    #[test]
    fn the_batched_walk_prices_each_header_once_in_order_and_speculates_no_further_than_a_batch() {
        for batch in 1..=6usize {
            for len in 1..=12usize {
                for gate_bad in 0..=len {
                    for pow_bad in 0..=len {
                        let gated = |k: usize| k != gate_bad;
                        let mut priced: Vec<usize> = Vec::new();
                        let mut ahead: VecDeque<usize> = VecDeque::new();
                        let mut consumed: Vec<usize> = Vec::new();

                        for i in 0..len {
                            if !gated(i) {
                                break; // the walk returns the gate's error here
                            }
                            if ahead.is_empty() {
                                let end = pow_batch_end(i, len, batch, gated);
                                assert!(end > i && end <= len, "the window must be non-empty and in range");
                                for k in i..end {
                                    priced.push(k);
                                    ahead.push_back(k);
                                }
                            }
                            consumed.push(ahead.pop_front().expect("the refill leaves at least this header"));
                            if i == pow_bad {
                                break; // the walk returns a PoW-derived error here
                            }
                        }

                        let stop = gate_bad.min(pow_bad + 1).min(len);
                        assert_eq!(consumed, (0..stop).collect::<Vec<_>>(), "batch={batch} len={len}");
                        assert!(priced.iter().all(|&k| gated(k)), "a gate-rejected header was priced");
                        assert!(priced.windows(2).all(|w| w[1] == w[0] + 1), "headers were priced out of order");
                        assert!(
                            priced.len() - consumed.len() < batch,
                            "speculation ran {} past the walk, further than the batch bound {batch}",
                            priced.len() - consumed.len()
                        );
                    }
                }
            }
        }
    }
}
