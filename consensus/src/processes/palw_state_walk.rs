//! The PALW state walk (ADR-0044 Unit C): reconstructing a candidate chain's
//! [`PalwChainStateV2`] from the anchor and the stored deltas.
//!
//! This is the primitive `calculate_utxo_state_relatively` needs, extracted so it can be tested
//! against the in-memory `PalwStateBookV2` — the same shape the differential gate already runs.
//! The virtual processor's walk is the same two legs this exposes:
//!
//! ```text
//! revert(newest → fork)   the deltas of blocks LEAVING the selected chain
//! apply (fork → newest)   the deltas of blocks JOINING it
//! ```
//!
//! **Why a walker and not a store read.** A node cannot keep one state per chain block (the state
//! is the whole registry), so a candidate's standing is *derived*. Deriving it from deltas rather
//! than re-running transitions is what makes a reorg cost the depth of the reorg instead of the
//! length of the chain — and `apply_delta_v2`/`revert_delta_v2` verify the value each entry
//! replaces, so a walk that drifts from the transition is an error rather than a wrong answer.
//!
//! **Nothing calls this yet.** The virtual-processor wiring is the rest of Unit C; landing the
//! primitive with its equivalence test first means that wiring is written against a test that is
//! red on regression (`docs/palw-fp-wiring-atomicity.md`).

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::palw_state_v2::{
    PalwChainStateV2, PalwStateParamsV2, PalwStateV2Error, apply_delta_v2, revert_delta_v2,
};

use crate::model::stores::palw_state_v2::DbPalwStateV2Store;

#[derive(thiserror::Error, Debug)]
pub enum PalwStateWalkError {
    #[error("no PALW state anchor is stored — a node that has never written one cannot walk")]
    NoAnchor,
    #[error("block {0} has no stored PALW delta — a missing delta is an error, never an empty one")]
    MissingDelta(BlockHash),
    #[error("state error while walking: {0}")]
    State(#[from] PalwStateV2Error),
    #[error("store error while walking: {0}")]
    Store(#[from] kaspa_database::prelude::StoreError),
}

/// The materialized anchor, loaded and verified against its committed root.
///
/// `expected_root` is never `None` here, and that is the point: a carriage's self-consistency
/// cannot catch a coherent lie about a claim's `pwu` (its own doc says so), so the root is what
/// makes a tampered snapshot a lie about a DIFFERENT state rather than a plausible one.
pub fn load_anchor(store: &DbPalwStateV2Store, params: &PalwStateParamsV2) -> Result<(BlockHash, PalwChainStateV2), PalwStateWalkError> {
    // `load_tip` demands the recorded root and runs the full `into_state` rebuild, so a tampered
    // or corrupted snapshot is refused here rather than becoming the walk's starting point.
    store.load_tip(params)?.ok_or(PalwStateWalkError::NoAnchor)
}

/// Walk from `state` (which stands at the anchor / current point) to a candidate, by reverting
/// the deltas of `removed` newest-first and applying the deltas of `added` oldest-first.
///
/// The two slices are exactly `ChainPath { added, removed }`'s, in ITS order — `removed` is
/// ordered from the old sink downwards and `added` from the fork upwards, which is why one is
/// consumed as given and the other in order. Getting that backwards is the kind of mistake the
/// equivalence test below exists to catch, so the caller passes the path and this function owns
/// the direction.
pub fn walk_chain_path(
    store: &DbPalwStateV2Store,
    params: &PalwStateParamsV2,
    mut state: PalwChainStateV2,
    removed: &[BlockHash],
    added: &[BlockHash],
) -> Result<PalwChainStateV2, PalwStateWalkError> {
    for block in removed {
        let delta = load_delta(store, *block)?;
        state = revert_delta_v2(&state, &delta, params)?;
    }
    for block in added {
        let delta = load_delta(store, *block)?;
        state = apply_delta_v2(&state, &delta, params)?;
    }
    Ok(state)
}

fn load_delta(
    store: &DbPalwStateV2Store,
    block: BlockHash,
) -> Result<kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2, PalwStateWalkError> {
    match store.delta_of(block) {
        Ok((_root, delta)) => Ok(delta),
        // A missing delta is a missing FACT (ADR-0042 Decision 5: reading absent data as nothing
        // is forbidden). Treating it as "this block changed nothing" would make a pruned or
        // half-written history look like a valid chain with less work on it. An undecodable row
        // arrives as `DataInconsistency` from the store, which is the same refusal by another
        // name — never "absent".
        Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => Err(PalwStateWalkError::MissingDelta(block)),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::{
        PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2,
        PalwStateBookV2, PalwStateCarriageV2, apply_palw_transition_v3,
    };
    use kaspa_consensus_core::palw_freeprompt_v3::{PalwReceiptSpendUnsignedV3, spend_challenge_v3};
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{CachePolicy, ConnBuilder};
    use kaspa_hashes::Hash64;
    use rocksdb::WriteBatch;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond() -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 })
    }

    fn params() -> PalwStateParamsV2 {
        // base = h64(1) at the whole 1000‰ (granted by the first registration, ADR-0045
        // Decision 3), max_factor 4, tolerance 1000‰, min collateral 100, fp split 800‰.
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 800, 0).unwrap()
    }

    fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(block), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    fn registrations() -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond(),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 100_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
        ]
    }

    fn fp_commit(claim: u64, pwu: u64, quanta: u32) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::FreePromptCommitted {
            claim: h64(claim),
            class_id: h64(1),
            bond: bond(),
            pwu,
            quanta,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
        }
    }

    fn fp_spend(claim: u64, quantum_index: u32) -> PalwReceiptSpendUnsignedV3 {
        let b = bond().0;
        PalwReceiptSpendUnsignedV3 {
            version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
            network_domain: h64(999),
            challenge: spend_challenge_v3(h64(999), h64(0xB0), 1_700, 7, h64(claim), quantum_index, &b),
            claim_id: h64(claim),
            quantum_index,
            beacon_block: h64(0xBEAC),
            producer_bond: b,
            producer_pubkey: vec![7; 4],
        }
    }

    /// One shared prefix, certified to `Final`, then two competing branches — the shape every
    /// reorg test in this project uses. Returns the store, the fork state, and the two branches'
    /// independently-built states.
    #[allow(clippy::type_complexity)]
    fn fixture() -> (
        kaspa_database::utils::DbLifetime,
        std::sync::Arc<kaspa_database::prelude::DB>,
        DbPalwStateV2Store,
        PalwStateParamsV2,
        BlockHash,
        PalwChainStateV2,
        Vec<BlockHash>,
        PalwChainStateV2,
        Vec<BlockHash>,
        PalwChainStateV2,
    ) {
        let (lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwStateV2Store::new(db.clone(), CachePolicy::Count(64));
        store.reindex_if_stale().unwrap();
        let p = params();

        // Every step writes its delta exactly as the virtual processor would, in one batch with
        // (here, nothing else) — the walk then reads only what a real node stored.
        let mut persist = |block: BlockHash, root: Hash64, delta: &kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2| {
            let mut batch = WriteBatch::default();
            store.insert_delta_batch(&mut batch, block, root, delta).unwrap();
            db.write(batch).unwrap();
        };

        let seats = vec![PalwPanelSeatV2 { bond: bond(), operator_id: h64(90) }];
        let steps: Vec<(PalwBlockContextV2, Vec<PalwConsensusObjectV2>)> = vec![
            (ctx(1, 100, 1), registrations()),
            (ctx(2, 101, 2), vec![fp_commit(0xFC, 60, 3)]),
            (ctx(3, 102, 3), vec![PalwConsensusObjectV2::PanelBound { claim: h64(0xFC), anchor: h64(77), seats }]),
            (ctx(4, 103, 4), vec![PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC), receipts: Vec::new() }]),
            (ctx(5, 124, 5), vec![]),
        ];
        let mut state = PalwChainStateV2::genesis();
        for (c, objects) in &steps {
            let (child, delta) = apply_palw_transition_v3(&state, &p, c, objects, PalwBlockWorkV3::None).unwrap();
            persist(c.block, child.state_root(), &delta);
            state = child;
        }
        let fork_block = h64(5);
        let fork_state = state.clone();

        // Branch A: two spends. Branch B: one spend at a heavier blue score.
        let mut branch_a_blocks = Vec::new();
        let mut a = fork_state.clone();
        for (block, daa, blue, quantum) in [(0xA1u64, 130u64, 6u64, 0u32), (0xA2, 131, 7, 1)] {
            let spend = fp_spend(0xFC, quantum);
            let c = ctx(block, daa, blue);
            let (child, delta) = apply_palw_transition_v3(&a, &p, &c, &[], PalwBlockWorkV3::ReceiptSpend(&spend)).unwrap();
            persist(c.block, child.state_root(), &delta);
            branch_a_blocks.push(c.block);
            a = child;
        }
        let mut branch_b_blocks = Vec::new();
        let mut b = fork_state.clone();
        for (block, daa, blue, quantum) in [(0xB1u64, 130u64, 20u64, 0u32)] {
            let spend = fp_spend(0xFC, quantum);
            let c = ctx(block, daa, blue);
            let (child, delta) = apply_palw_transition_v3(&b, &p, &c, &[], PalwBlockWorkV3::ReceiptSpend(&spend)).unwrap();
            persist(c.block, child.state_root(), &delta);
            branch_b_blocks.push(c.block);
            b = child;
        }
        (lt, db, store, p, fork_block, fork_state, branch_a_blocks, a, branch_b_blocks, b)
    }

    /// **The walk equals building the branch.** Reverting branch A's deltas down to the fork and
    /// applying branch B's up reaches the state B was built as — through the store, not in
    /// memory. This is `calculate_utxo_state_relatively`'s two legs, on PALW state.
    #[test]
    fn walking_a_reorg_reaches_the_winning_branch() {
        let (_lt, _db, store, p, _fork, fork_state, a_blocks, a_state, b_blocks, b_state) = fixture();

        // `ChainPath::removed` is ordered from the old sink downwards.
        let removed: Vec<BlockHash> = a_blocks.iter().rev().copied().collect();
        let walked = walk_chain_path(&store, &p, a_state.clone(), &removed, &b_blocks).unwrap();
        assert_eq!(walked, b_state, "the walk reaches the branch built from scratch");
        assert_eq!(walked.state_root(), b_state.state_root());

        // And the other direction, from B back to A — a reorg is symmetric.
        let removed_b: Vec<BlockHash> = b_blocks.iter().rev().copied().collect();
        let back = walk_chain_path(&store, &p, b_state, &removed_b, &a_blocks).unwrap();
        assert_eq!(back, a_state);

        // Reverting the whole of A reaches the fork exactly.
        let at_fork = walk_chain_path(&store, &p, a_state, &removed, &[]).unwrap();
        assert_eq!(at_fork, fork_state);
    }

    /// The walk agrees with the in-memory book — the same states, reached two ways. If a store
    /// walk and the book ever disagreed, one of them would be the node's answer and the other the
    /// test's, which is the shape of every P0-4.
    #[test]
    fn the_store_walk_and_the_in_memory_book_agree() {
        let (_lt, _db, _store, p, _fork, _fork_state, a_blocks, a_state, b_blocks, b_state) = fixture();

        let mut book = PalwStateBookV2::new(p.clone());
        book.insert_genesis(h64(0));
        let seats = vec![PalwPanelSeatV2 { bond: bond(), operator_id: h64(90) }];
        book.apply_block(h64(0), ctx(1, 100, 1), &registrations(), None).unwrap();
        book.apply_block(h64(1), ctx(2, 101, 2), &[fp_commit(0xFC, 60, 3)], None).unwrap();
        book.apply_block(h64(2), ctx(3, 102, 3), &[PalwConsensusObjectV2::PanelBound { claim: h64(0xFC), anchor: h64(77), seats }], None)
            .unwrap();
        book.apply_block(h64(3), ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC), receipts: Vec::new() }], None).unwrap();
        book.apply_block(h64(4), ctx(5, 124, 5), &[], None).unwrap();
        book.apply_block_with_work(h64(5), ctx(0xA1, 130, 6), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0))).unwrap();
        book.apply_block_with_work(h64(0xA1), ctx(0xA2, 131, 7), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 1))).unwrap();
        book.apply_block_with_work(h64(5), ctx(0xB1, 130, 20), &[], PalwBlockWorkV3::ReceiptSpend(&fp_spend(0xFC, 0))).unwrap();

        assert_eq!(book.state_of(a_blocks.last().unwrap()).unwrap(), &a_state, "branch A: book == store-walk source");
        assert_eq!(book.state_of(b_blocks.last().unwrap()).unwrap(), &b_state, "branch B: book == store-walk source");
    }

    /// A missing delta is an ERROR, not an empty one. Reading absent data as "this block changed
    /// nothing" would make a half-written or pruned history look like a shorter, valid chain.
    #[test]
    fn a_missing_delta_stops_the_walk() {
        let (_lt, _db, store, p, _fork, _fork_state, _a, a_state, _b, _b_state) = fixture();
        let err = walk_chain_path(&store, &p, a_state.clone(), &[], &[h64(0xDEAD)]).unwrap_err();
        assert!(matches!(err, PalwStateWalkError::MissingDelta(b) if b == h64(0xDEAD)));

        // And a delta applied to the wrong parent is refused by the delta itself, not silently
        // absorbed — the property that keeps a store walk from drifting from the transition.
        let wrong_parent = walk_chain_path(&store, &p, a_state, &[], &[h64(0xB1)]).unwrap_err();
        assert!(matches!(wrong_parent, PalwStateWalkError::State(_)), "got {wrong_parent:?}");
    }

    /// The anchor loads under its committed root and the walk starts from it; a tampered snapshot
    /// does not load at all.
    #[test]
    fn the_anchor_is_the_walks_starting_point_and_is_verified() {
        let (_lt, db, mut store, p, fork, fork_state, _a, _a_state, b_blocks, b_state) = fixture();
        assert!(matches!(load_anchor(&store, &p), Err(PalwStateWalkError::NoAnchor)), "a fresh node has no anchor");

        let mut batch = WriteBatch::default();
        store.set_tip_batch(&mut batch, fork, &fork_state).unwrap();
        db.write(batch).unwrap();

        let (block, loaded) = load_anchor(&store, &p).expect("the honest anchor loads");
        assert_eq!(block, fork);
        assert_eq!(loaded, fork_state);

        // …and walking branch B forward from the anchor reaches B.
        let walked = walk_chain_path(&store, &p, loaded, &[], &b_blocks).unwrap();
        assert_eq!(walked, b_state);

        // A snapshot whose root no longer matches cannot load. `set_tip_batch` computes the root
        // from the state it is handed (a caller cannot store a snapshot under a root it does not
        // hash to), so the tamper has to be written at the row level — which is exactly the
        // threat: a disk or a tool editing the bytes after the fact.
        let tampered = {
            let mut carriage = PalwStateCarriageV2::from_state(&fork_state);
            carriage.safe_weight += 1;
            borsh::to_vec(&carriage).unwrap()
        };
        let record = store.tip_record().unwrap().expect("the honest tip was just written");
        let mut batch = WriteBatch::default();
        store
            .set_tip_record_batch(
                &mut batch,
                crate::model::stores::palw_state_v2::PalwStateTipRecordV2 {
                    block: record.block,
                    state_root: record.state_root,
                    carriage_borsh: tampered,
                },
            )
            .unwrap();
        db.write(batch).unwrap();
        assert!(
            matches!(load_anchor(&store, &p), Err(PalwStateWalkError::Store(_))),
            "a tampered anchor cannot load"
        );
    }
}
