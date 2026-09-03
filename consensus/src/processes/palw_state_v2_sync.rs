//! MISAKA PALW V2 chain-state sync (ADR-0042 Decision 5, PR-08's walk shape).
//!
//! The ordering discipline between [`DbPalwStateV2Store`] and a `ChainPath`: retreat along
//! `removed` (which `calculate_chain_path` yields **child-first**, old sink downward), advance
//! along `added` (**parent-first**, fork point upward) — the same walk every other chain-scoped
//! overlay runs at virtual commit (`stage_compute_capabilities` is the template).
//!
//! # Dormant, like the store under it
//!
//! Nothing constructs this on any preset. It lands before its caller for the same reason the
//! carriage store did: the subtle part is the DISCIPLINE — revert order, tip movement,
//! all-or-nothing advance — and that is unit-lockable now, while the processor seam (and the
//! validation rule that must precede it, see below) is PR-10's.
//!
//! # The error contract is the design
//!
//! [`PalwStateSyncV2::advance`] is **transactional in memory**: it folds the steps on a local
//! state and installs the result only if every step applied. A step that fails names its block
//! and leaves `self` untouched; the caller discards the `WriteBatch`, so disk and memory agree
//! that nothing happened.
//!
//! This matters because of WHERE a failure can come from once wired. The walk runs at virtual
//! commit — after block validation. A transition that fails HERE therefore means block
//! validation admitted something the state machine refuses: a rule/state divergence, which no
//! amount of local handling makes safe. The contract makes that expressible — the caller gets
//! the failing block and a hard error, never a half-advanced tip — and it fixes the wiring
//! order: **the stateful admission check must run at block validation (the fifth sibling of the
//! four demand gates) before this walk can be called on a live network.** Wiring the walk first
//! would turn any adversarial block into a commit-time abort.

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2, PalwStateParamsV2, PalwStateV2Error,
    apply_palw_transition_v2_with_policies, revert_delta_v2,
};
use kaspa_database::prelude::StoreError;
use rocksdb::WriteBatch;

use crate::model::stores::palw_state_v2::DbPalwStateV2Store;

/// One chain block's contribution to the V2 state, assembled by the caller (which holds the
/// header, the carried envelope and — once PR-10 defines their carrier — the consensus objects).
#[derive(Clone, Debug)]
pub struct PalwChainStepV2 {
    pub ctx: PalwBlockContextV2,
    pub objects: Vec<PalwConsensusObjectV2>,
    pub attempt: Option<PalwAttemptEnvelopeV2>,
}

#[derive(thiserror::Error, Debug)]
pub enum PalwSyncV2Error {
    #[error("palw v2 sync store error: {0}")]
    Store(#[from] StoreError),
    #[error("palw v2 transition failed at block {block}: {source}")]
    // Boxed: the transition error is the largest thing this enum can carry (272 bytes), and every
    // `Result` on the sync walk would otherwise be that wide on its Ok path too.
    State { block: BlockHash, source: Box<PalwStateV2Error> },
    #[error("retreat expects the current tip {expected:?} first, got {got}")]
    NotAtTip { expected: Option<BlockHash>, got: BlockHash },
    #[error("the sync has no tip — install genesis before advancing or retreating")]
    NoTip,
    #[error("genesis is already installed at {0}; a second install would orphan the chain's state")]
    TipExists(BlockHash),
    #[error("after retreating, the state's last point {found:?} does not name the fork point {expected}")]
    ForkPointMismatch { expected: BlockHash, found: Option<BlockHash> },
}

/// The materialized V2 state at the selected sink, kept in step with the store.
///
/// The in-memory state is the working copy; the store rows are what a restart resumes from.
/// Every mutation stages its rows into the caller's `WriteBatch` and mutates `self` only on
/// success, so "batch written" and "self mutated" can only diverge if the caller drops a batch —
/// in which case a reload from the store reproduces exactly the pre-batch tip.
pub struct PalwStateSyncV2 {
    params: PalwStateParamsV2,
    tip: Option<(BlockHash, PalwChainStateV2)>,
    /// ADR-0065 D4's fence, carried so this walk resolves the verdict policy the same way the
    /// virtual processor does — per step, from that step's own DAA score.
    ///
    /// It is here while the module is still dormant deliberately. A replay path that folded with
    /// the pre-D4 rule while the live path folded with the post-D4 one would compute a different
    /// state root for the same block and reject the chain it was syncing, and the moment to close
    /// that is before there is a caller, not after someone has to debug it.
    unavailable_abstains: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0069 Decision 7's fence, carried for exactly the reason above: a replay that priced a
    /// weightless class's work differently from the live fold would compute a different state root
    /// for the same block and reject the chain it was syncing.
    uncertified_weightless: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0062's fence, carried for exactly the reason the one above it is: a replay path that
    /// folded with the pre-DA-court rule while the live path folded with the post-court one would
    /// compute a different state root for the same block and reject the chain it was syncing.
    da_court: Option<kaspa_consensus_core::config::params::ForkActivation>,
    // **`palw_kary_court` is deliberately NOT carried here, and that is not the gap the three
    // fences above are** (ADR-0082 Decision 2, audit A C-5).
    //
    // The dissection is fenced at the ACCEPTANCE layer — `palw_v2_validate_objects` admits the
    // three `CourtAttn*` moves only past `palw_kary_court_active_at`, and this walk replays the
    // objects that layer already admitted — so the fold has no fence to resolve for them, and
    // `PalwCourtSessionStateV2::dissection` being `Some` is itself the record that the fence was
    // active when the phase opened.
    //
    // The clock C-5 adds at a terminal ladder reads `PalwClassStateV2::fused_attention`, which is
    // written by this same fold from the graph the registration carried, and is likewise the
    // record that the fence was armed: `verify_class_admission_v6` refuses a fused profile unless
    // its `court` argument is `Some`, and that argument IS
    // `VirtualStateProcessor::palw_kary_court_active_at`. So the walk cannot resolve it
    // differently from the live path — there is nothing here to resolve. A future rule that made
    // the dissection's own arithmetic fence-dependent inside the transition WOULD need threading,
    // exactly as the three above do.
}

impl PalwStateSyncV2 {
    /// Resume from the store: the tip snapshot loads root-verified, or the sync starts empty on
    /// a fresh database.
    pub fn load(
        store: &DbPalwStateV2Store,
        params: PalwStateParamsV2,
        unavailable_abstains: Option<kaspa_consensus_core::config::params::ForkActivation>,
        uncertified_weightless: Option<kaspa_consensus_core::config::params::ForkActivation>,
        da_court: Option<kaspa_consensus_core::config::params::ForkActivation>,
    ) -> Result<Self, PalwSyncV2Error> {
        let tip = store.load_tip(&params)?;
        Ok(Self { params, tip, unavailable_abstains, uncertified_weightless, da_court })
    }

    pub fn tip(&self) -> Option<(&BlockHash, &PalwChainStateV2)> {
        self.tip.as_ref().map(|(block, state)| (block, state))
    }

    /// Install the genesis point: the empty state, standing at the genesis block. Objects the
    /// genesis itself registers (BASE-0, the initial bonds — PR-10's loader) are the first
    /// [`Self::advance`] step, exactly as `PalwStateBookV2` models them.
    pub fn install_genesis(
        &mut self,
        store: &mut DbPalwStateV2Store,
        batch: &mut WriteBatch,
        genesis_block: BlockHash,
    ) -> Result<(), PalwSyncV2Error> {
        if let Some((block, _)) = &self.tip {
            return Err(PalwSyncV2Error::TipExists(*block));
        }
        let state = PalwChainStateV2::genesis();
        store.set_tip_batch(batch, genesis_block, &state)?;
        self.tip = Some((genesis_block, state));
        Ok(())
    }

    /// Apply `steps` (parent-first, a `ChainPath::added` walk) on top of the current tip.
    ///
    /// All or nothing: the fold runs on a local state, rows stage into `batch` as they succeed,
    /// and `self` moves only when every step applied. On failure the caller MUST discard the
    /// batch — the error names the block whose transition the state machine refused, which on a
    /// wired network is a validation/state divergence, not a recoverable condition.
    pub fn advance(
        &mut self,
        store: &mut DbPalwStateV2Store,
        batch: &mut WriteBatch,
        steps: &[PalwChainStepV2],
    ) -> Result<(), PalwSyncV2Error> {
        let Some((_, tip_state)) = &self.tip else {
            return Err(PalwSyncV2Error::NoTip);
        };
        let Some(last) = steps.last() else {
            return Ok(());
        };
        let mut current = tip_state.clone();
        for step in steps {
            let (next, delta) = apply_palw_transition_v2_with_policies(
                &current,
                &self.params,
                &step.ctx,
                &step.objects,
                step.attempt.as_ref(),
                self.unavailable_abstains.is_some_and(|fence| fence.is_active(step.ctx.daa_score)),
                // **ADR-0071's capability bound is NOT resolved here, and that is the pre-merge
                // behaviour rather than a decision taken at the merge.** Before ADR-0069 D7 this
                // walk called `apply_palw_transition_v2_with_verdict_policy`, which passes `false`
                // for it; the two policies were unified into one face so a third could not be added
                // to one and forgotten in the other, and passing `false` here keeps the walk
                // byte-identical to what it computed before. It is correct only while
                // `palw_capability_bound` is `None` on every preset — which it is — and it is a
                // REAL gap the moment that fence is armed: a sync walk that resolved the fence
                // differently from the fold would write a different state root for the same block,
                // which is the disagreement this face exists to prevent. Arming ADR-0071 means
                // threading the fence onto this walker first.
                false,
                self.uncertified_weightless.is_some_and(|fence| fence.is_active(step.ctx.daa_score)),
                self.da_court.is_some_and(|fence| fence.is_active(step.ctx.daa_score)),
            )
            .map_err(|source| PalwSyncV2Error::State { block: step.ctx.block, source: Box::new(source) })?;
            store.insert_delta_batch(batch, step.ctx.block, next.state_root(), &delta)?;
            current = next;
        }
        store.set_tip_batch(batch, last.ctx.block, &current)?;
        self.tip = Some((last.ctx.block, current));
        Ok(())
    }

    /// Revert `removed` (child-first, a `ChainPath::removed` walk) down to `fork_point`.
    ///
    /// Each block's stored delta is verified against the state it claims to have produced
    /// (`revert_delta_v2` checks every replaced value), its row is deleted, and the tip lands on
    /// `fork_point` — cross-checked against the reverted state's own `last_point` where one
    /// exists, so a caller that mis-names the fork point is refused instead of installing a tip
    /// whose label and contents disagree.
    pub fn retreat(
        &mut self,
        store: &mut DbPalwStateV2Store,
        batch: &mut WriteBatch,
        removed: &[BlockHash],
        fork_point: BlockHash,
    ) -> Result<(), PalwSyncV2Error> {
        let Some((tip_block, tip_state)) = &self.tip else {
            return Err(PalwSyncV2Error::NoTip);
        };
        if removed.is_empty() {
            return Ok(());
        }
        if removed[0] != *tip_block {
            return Err(PalwSyncV2Error::NotAtTip { expected: Some(*tip_block), got: removed[0] });
        }
        let mut current = tip_state.clone();
        for (index, block) in removed.iter().enumerate() {
            // The state must be AT `block` before its delta reverts it. The first iteration is
            // the tip check above; later iterations verify the chain's own linkage.
            if index > 0 {
                match current.last_point() {
                    Some(point) if point.block == *block => {}
                    other => {
                        return Err(PalwSyncV2Error::NotAtTip { expected: other.map(|p| p.block), got: *block });
                    }
                }
            }
            let (_, delta) = store.delta_of(*block)?;
            current = revert_delta_v2(&current, &delta, &self.params)
                .map_err(|source| PalwSyncV2Error::State { block: *block, source: Box::new(source) })?;
            store.delete_delta_batch(batch, *block)?;
        }
        if let Some(point) = current.last_point()
            && point.block != fork_point
        {
            return Err(PalwSyncV2Error::ForkPointMismatch { expected: fork_point, found: Some(point.block) });
        }
        store.set_tip_batch(batch, fork_point, &current)?;
        self.tip = Some((fork_point, current));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptUnsignedV2, challenge_v2};
    use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwPwuRuleV2, PalwStateBookV2, palw_operator_id_v2};
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{CachePolicy, ConnBuilder};

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, class_id(), 4, 1000, 100, 1000, 0).unwrap()
    }

    fn class_id() -> Hash64 {
        Hash64::from_u64_word(1)
    }

    fn bond_key() -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 })
    }

    fn genesis_block() -> BlockHash {
        BlockHash::from_u64_word(0x6E5)
    }

    fn ctx(block_word: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: BlockHash::from_u64_word(block_word), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    fn registrations() -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: class_id(),
                artifact_root: Hash64::from_u64_word(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ]
    }

    /// An attempt the STATEFUL side admits (bond, class, operator, artifact, pwu, ticket). The
    /// stateless side (signature, challenge-vs-position) belongs to the acceptance layer and the
    /// finalizer arm — not re-checked by the transition, so fixture values suffice there.
    fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
        let net = Hash64::from_u64_word(0x7E57);
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: net,
            challenge: challenge_v2(net, Hash64::from_u64_word(0xB0), 1_700_000_000, nonce, class_id(), &bond_key().0),
            class_id: class_id(),
            executor_bond: bond_key().0,
            executor_pubkey: vec![7; 4],
            operator_id: palw_operator_id_v2(&[21; 8]),
            artifact_root: Hash64::from_u64_word(11),
            trace_root: Hash64::from_u64_word(0x7A),
            output_root: Hash64::from_u64_word(0x07),
            pwu,
            trace_manifest_root: Hash64::from_u64_word(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: 1_000_000,
            execution_root: Hash64::from_u64_word(0x41),
        };
        PalwAttemptEnvelopeV2 { attempt, signature: vec![0x5A; 4627] }
    }

    /// Find an EXECUTION whose class ticket the (generous) initial target admits, so the fixture
    /// chain carries a claim that won its draw without hand-picking magic numbers. The transition
    /// itself no longer draws the lottery — since ADR-0072 the ticket is the header's, checked
    /// beside the position — but the book should still record blocks that would have won it.
    fn admitted_attempt(pwu: u64) -> PalwAttemptEnvelopeV2 {
        use kaspa_consensus_core::palw_attempt_v2::{class_ticket_v3, execution_anchor_v3};
        let anchor = execution_anchor_v3(Hash64::from_u64_word(0x7E57), Hash64::from_u64_word(0xB0), class_id(), &bond_key().0, 0);
        (0..64u64)
            .map(|execution| {
                let mut env = attempt(pwu, 0);
                env.attempt.execution_root = Hash64::from_u64_word(0x4100 + execution);
                env
            })
            .find(|env| class_ticket_v3(&env.attempt, anchor) <= u128::MAX / 2)
            .expect("a 2^-1 target admits one of 64 executions")
    }

    fn steps() -> Vec<PalwChainStepV2> {
        vec![
            PalwChainStepV2 { ctx: ctx(0xB1, 100, 100), objects: registrations(), attempt: None },
            PalwChainStepV2 { ctx: ctx(0xB2, 110, 110), objects: vec![], attempt: Some(admitted_attempt(100)) },
        ]
    }

    /// The sync IS the book, through disk and restarts: advancing the same steps produces the
    /// book's states, a reload resumes at the same tip, and a retreat lands bit-exactly on the
    /// intermediate state while deleting exactly the reverted row.
    #[test]
    fn the_sync_reproduces_the_book_across_restart_and_reorg() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        // The reference: the in-memory book over the same steps.
        let mut book = PalwStateBookV2::new(params());
        book.insert_genesis(genesis_block());
        let steps = steps();
        let mut parent = genesis_block();
        for step in &steps {
            book.apply_block(parent, step.ctx, &step.objects, step.attempt.as_ref()).unwrap();
            parent = step.ctx.block;
        }

        // The subject: sync + store + batches.
        let mut sync = PalwStateSyncV2::load(&store, params(), None, None, None).unwrap();
        assert!(sync.tip().is_none(), "a fresh database has no tip");
        let mut batch = WriteBatch::default();
        sync.install_genesis(&mut store, &mut batch, genesis_block()).unwrap();
        sync.advance(&mut store, &mut batch, &steps).unwrap();
        db.write(batch).unwrap();

        let (tip_block, tip_state) = sync.tip().unwrap();
        assert_eq!(*tip_block, steps[1].ctx.block);
        assert_eq!(tip_state, book.state_of(&steps[1].ctx.block).unwrap(), "the sync's tip is the book's state");

        // A restart resumes at the same tip, root-verified.
        let resumed = PalwStateSyncV2::load(&store, params(), None, None, None).unwrap();
        let (r_block, r_state) = resumed.tip().unwrap();
        assert_eq!((r_block, r_state), (tip_block, tip_state));

        // Reorg: retreat the top block. Child-first list, fork point named.
        let mut batch = WriteBatch::default();
        sync.retreat(&mut store, &mut batch, &[steps[1].ctx.block], steps[0].ctx.block).unwrap();
        db.write(batch).unwrap();
        let (tip_block, tip_state) = sync.tip().unwrap();
        assert_eq!(*tip_block, steps[0].ctx.block);
        assert_eq!(tip_state, book.state_of(&steps[0].ctx.block).unwrap(), "retreat lands on the book's intermediate state");
        assert!(!store.has_delta(steps[1].ctx.block).unwrap(), "the reverted row is gone");
        assert!(store.has_delta(steps[0].ctx.block).unwrap(), "the surviving row is not");

        // And the other branch applies on the fork point.
        let branch = PalwChainStepV2 { ctx: ctx(0xB3, 111, 111), objects: vec![], attempt: Some(admitted_attempt(200)) };
        let mut batch = WriteBatch::default();
        sync.advance(&mut store, &mut batch, std::slice::from_ref(&branch)).unwrap();
        db.write(batch).unwrap();
        assert_eq!(*sync.tip().unwrap().0, branch.ctx.block);
    }

    /// All or nothing: a failing step names its block and moves nothing — the tip stays, and a
    /// dropped batch leaves the store resuming at the pre-advance tip.
    #[test]
    fn a_failing_step_moves_neither_tip_nor_store() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let mut sync = PalwStateSyncV2::load(&store, params(), None, None, None).unwrap();
        let mut batch = WriteBatch::default();
        sync.install_genesis(&mut store, &mut batch, genesis_block()).unwrap();
        db.write(batch).unwrap();

        // Step 1 registers; step 2 carries an attempt naming a bond nobody registered.
        let mut orphan = admitted_attempt(100);
        orphan.attempt.executor_bond = TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xBAD), index: 0 };
        let bad_steps = vec![
            PalwChainStepV2 { ctx: ctx(0xB1, 100, 100), objects: registrations(), attempt: None },
            PalwChainStepV2 { ctx: ctx(0xB2, 110, 110), objects: vec![], attempt: Some(orphan) },
        ];

        let mut batch = WriteBatch::default();
        match sync.advance(&mut store, &mut batch, &bad_steps) {
            Err(PalwSyncV2Error::State { block, .. }) => assert_eq!(block, bad_steps[1].ctx.block, "the error names the block"),
            other => panic!("an unregistered bond must refuse the step, got {other:?}"),
        }
        drop(batch); // the contract: a failed advance's batch is discarded

        let (tip_block, _) = sync.tip().unwrap();
        assert_eq!(*tip_block, genesis_block(), "the in-memory tip did not move");
        // Probe through a FRESH store over the same database: the staged-then-dropped rows must
        // not exist durably, and the polluted write-through cache of the old handle must not be
        // what answers (the carriage store's crash-window lesson, applied to a refusal).
        let fresh = DbPalwStateV2Store::new(db, CachePolicy::Count(16));
        let resumed = PalwStateSyncV2::load(&fresh, params(), None, None, None).unwrap();
        assert_eq!(*resumed.tip().unwrap().0, genesis_block(), "and neither did the durable one");
        assert!(!fresh.has_delta(bad_steps[0].ctx.block).unwrap(), "no row of the refused walk was committed");
    }

    /// Retreat discipline: the walk must start at the tip, and the fork point must be the block
    /// the reverted state itself names — both refusals, not corrections.
    #[test]
    fn retreat_refuses_wrong_order_and_wrong_fork_point() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(16));
        store.reindex_if_stale().unwrap();

        let steps = steps();
        let mut sync = PalwStateSyncV2::load(&store, params(), None, None, None).unwrap();
        let mut batch = WriteBatch::default();
        sync.install_genesis(&mut store, &mut batch, genesis_block()).unwrap();
        sync.advance(&mut store, &mut batch, &steps).unwrap();
        db.write(batch).unwrap();

        // Not starting at the tip: refused before anything stages.
        let mut batch = WriteBatch::default();
        assert!(matches!(
            sync.retreat(&mut store, &mut batch, &[steps[0].ctx.block], genesis_block()),
            Err(PalwSyncV2Error::NotAtTip { .. })
        ));

        // Right order, wrong fork point: refused after the fold, nothing installed.
        let mut batch = WriteBatch::default();
        assert!(matches!(
            sync.retreat(&mut store, &mut batch, &[steps[1].ctx.block], genesis_block()),
            Err(PalwSyncV2Error::ForkPointMismatch { .. })
        ));
        drop(batch);
        assert_eq!(*sync.tip().unwrap().0, steps[1].ctx.block, "a refused retreat moves nothing");
        assert!(store.has_delta(steps[1].ctx.block).unwrap());
    }
}
