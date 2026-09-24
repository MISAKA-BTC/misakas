//! **ADR-0152 Phase 2, P2-3 — the vesting rows across a reorg and a pruned sync, at the processor on
//! testnet-12** (phase2-plan §1.3, §4 T48 and T49, §5.1; ADR-0152 V-3, I-6, I-7, I-9).
//!
//! A child of `p2_mint_path`, so every block here is a real testnet-12 block built by the node's own
//! template and heartbeat adapter and held to `Minting::after` (the carve, build == validate, the
//! next-block plan, the latch, the queue lemma, every moved leg minted next block, V-3). What this
//! file adds is MORE THAN ONE NODE: blocks built by one `TestConsensus` arrive at another through
//! `validate_and_insert_block`, which is how a peer's blocks reach a node.
//!
//! * **T48, reorg.** Node A mines the latch (N), the move (N+1) and the mint (N+2) of a row; node B,
//!   from the same parent, mines the row's conviction at N+1, which burns it. A third node sees A's
//!   chain, then B's heavier one, then A's again, and at every switch its state root is the one the
//!   chain's own builder committed (a fresh replay of that chain alone), the mint is an output only
//!   while A is the chain, and the notes of the blocks it reverted and applied account exactly for
//!   the counters' change.
//! * **T49, pruned sync.** A node imports the pruning-point carriage an archival node serves, inside
//!   a backlog of latched-and-carried rows, reporter rows and a 1,020-row queue, and builds the next
//!   eleven blocks byte for byte as the archival node built them; and a carriage at T-3(b)'s row
//!   bound is served within one peer's window.
//! * **The undecodable-delta test, extended to the vesting variants** (the plan names
//!   `palw_v2_an_undecodable_delta_refuses_a_deep_reorg_rather_than_abstaining`): a delta holding
//!   `Vesting`, `VestingNote` and `VestingCounters` entries walks exactly; damaged inside a vesting
//!   entry it is a fault, not an absence, and the reorg that needs it holds the sink.
//!
//! **Why the rows are planted, not mined to `Final`.** A real maturity is `window_court` (3,000 DAA)
//! and thirty licences after a `Final`; T03 carries two real `Final`s through a real panel and takes
//! seven minutes of a debug build for one node. A reorg test needs three nodes, so the rows are
//! planted on the tip through the carriage (`Minting::plant`, as T58 and T47 do) and every block from
//! there on is the node's. The row T48 convicts is planted as a retired `Final` leaves it: its
//! vesting row and the liability row X29 keeps beside it, no claim record — so the kind-4 target
//! resolves from the liability row (`palw_offence_target_v1`'s second source, ADR-0152 v3.1 J-2), `G`
//! is the row's recorded `g_res + escrowed_reward`, and the conviction takes the S-4 funnel every
//! post-`Final` conviction takes (`mark_liability_convicted`, then `post_final_producer_leg_v1` →
//! `burn_vesting_row` and S3's `min(25% · C₀, 3 G)`). Its proof is a REAL floor execution of another
//! job, whose root the claim carries (T18b's borrowed root, J-5).
use super::*;
use crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2;
use crate::pipeline::virtual_processor::processor::PalwWeighFaultV2;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_EXECUTOR_REFUTED_VERSION_V1, PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES, PalwExecutorRefutedEvidenceV1,
    palw_executor_refuted_offence_id_v1,
};
use kaspa_consensus_core::palw_offence_v1::{PalwOffenceKindV1, PalwPanelContradictionV1, palw_offence_evidence_digest_v1};
use kaspa_consensus_core::palw_panel_var_v1::{PalwPanelLiabilityRecordV1, PalwSlashableLockV1};
use kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2;
use kaspa_consensus_core::palw_step_leg::{PalwStepBindingV2, verify_binding_v1};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_consensus_core::palw_vesting_v1::PalwVestingCountersV1;

// ---- more than one node ------------------------------------------------------------------------

/// Every selected-chain block of `chain` from genesis (exclusive) to `upto` (inclusive), oldest first.
fn chain_blocks(chain: &T12Chain, upto: BlockHash) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = upto;
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes.into_iter().map(|hash| chain.ctx.consensus.get_block(hash).expect("the node holds every block of its chain")).collect()
}

/// `block` arrives at `chain` as a peer's block does. It need not become the sink.
async fn arrive(chain: &T12Chain, block: Block, what: &str) {
    let hash = block.header.hash;
    chain
        .ctx
        .consensus
        .validate_and_insert_block(block)
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
}

/// **A second node on `of`'s chain, through `upto`** — testnet-12 with the same harness cards (the
/// same genesis), fed `of`'s blocks in order, with `of`'s plant installed at the block it was planted
/// on before the next block arrives (the plant is a test's hand, so a peer cannot send it; every
/// block after it is validated by this node against the same planted state). From `upto` on the
/// two nodes build and see what the test gives them.
async fn follower(of: &Minting, upto: BlockHash) -> Minting {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    assert_eq!(chain.config.params.genesis.hash, of.chain.config.params.genesis.hash, "one genesis");
    let planted_at = of.planted.last_point().expect("the plant stands at a block").block;
    for block in chain_blocks(&of.chain, upto) {
        let hash = block.header.hash;
        arrive(&chain, block, "a block of the followed chain").await;
        assert_eq!(chain.sink(), hash, "the follower walks the followed chain");
        if hash == planted_at {
            chain.vp().palw_state_v2_store.write().set_tip_for_tests(hash, &of.planted).expect("the same plant, at the same block");
        }
    }
    chain.ctx.simulated_time = of.chain.ctx.simulated_time;
    Minting {
        chain,
        domain: of.domain,
        payloads: of.payloads.clone(),
        planted: of.planted.clone(),
        rows: of.rows.clone(),
        planted_reporters: of.planted_reporters,
        planted_market: of.planted_market,
        row_keys: of.row_keys.clone(),
        reporter_keys: of.reporter_keys.clone(),
        ledger: Ledger::default(),
        books: Books::default(),
        wallets: of.wallets.clone(),
        last_attempt: None,
        nonce: of.nonce,
    }
}

/// **The heartbeat `chain` builds for the slot `like` took** — the node's own template with `like`'s
/// miner data, stamped at `like`'s time with `like`'s nonce, shaped by the node's own adapter. Every
/// other byte is the node's own derivation, so the result IS `like` exactly when the two nodes'
/// states agree (the adapter stamps `max(time, slot)`, and `like`'s time is already at or past it).
fn beat_like(chain: &T12Chain, like: &Block) -> MutableBlock {
    let vp = chain.vp();
    let theirs = vp.coinbase_manager.deserialize_coinbase_payload(&like.transactions[0].payload).expect("a coinbase").miner_data;
    let miner_data = MinerData::new(theirs.script_public_key, theirs.extra_data.to_vec());
    let mut t = chain
        .ctx
        .consensus
        .build_block_template(miner_data, Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&chain.config.params, &mut t.block.header, like.header.timestamp);
    t.block.header.nonce = like.header.nonce;
    t.block.header.finalize();
    let (t, _) = vp.heartbeat_adapt_block_template(t).expect("the heartbeat lane is open on testnet-12");
    t.block
}

/// A plain transfer of `payer`'s wallet back to itself, less a fee — a block's acceptance of it is
/// a UTXO diff that is not empty.
fn transfer(m: &mut Minting, payer: usize) -> (Transaction, TransactionOutpoint) {
    let (outpoint, entry) = m.wallets.remove(&payer).unwrap_or_else(|| panic!("card {payer} has a wallet"));
    let change = entry.amount - CARRIER_FEE;
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(change, card_payout_spk(payer))],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        Vec::new(),
    );
    sign_spend(&mut tx, entry, payer, m.chain.config.params.storage_mass_parameter);
    m.wallets.insert(payer, (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(change, card_payout_spk(payer), 0, false)));
    (tx, outpoint)
}

/// **A real floor execution of `anchor`'s job** — the honest run a producer can borrow (T18b): the
/// floor's canonical job for the anchor, run by the floor's own executor, and the binding the run
/// commits (`verify_binding` recomputes its root). Its `job_id` is `anchor`, so it convicts, by the
/// identity rule's J1, any claim that carries its root and recorded another job.
fn floor_binding_of(m: &Minting, anchor: Hash64) -> PalwStepBindingV2 {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    use misaka_palw_base0::{backend::Base0Backend, classes::resolve_class_v1};
    let bundle = &m.chain.bundle;
    let (_, tip) = m.chain.tip_state();
    let artifact_root = tip.class(&bundle.base_class_id).expect("the floor is registered").artifact_root;
    let backend = Base0Backend::new(
        resolve_class_v1(&bundle.court, bundle.base_class_id, artifact_root, &[])
            .expect("the floor resolves from its registered root"),
    )
    .with_step_ladder_cap(bundle.court.max_step_leaf_count())
    .with_prompt_ids_form(m.chain.config.params.palw_prompt_ids_form_v1());
    let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the floor implies a job");
    let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(
        canonical,
        m.chain.config.params.palw_prefill_draw_active_at(m.sink_daa()),
    );
    let artifact = misaka_palw_base0::rc::palw_rc_base0_artifact_v1().expect("the floor's artifact derives");
    let run = misaka_palw_base0::produce::base0_execute_for_attempt_v1(&artifact, backend.profile(), &job, &prompt)
        .expect("the floor runs its own job");
    verify_binding_v1(&run.binding).expect("an honest binding reproduces its root");
    assert_eq!(run.binding.job_context.job_id, anchor, "an honest binding answers its own anchor");
    run.binding
}

/// **`ExecutorRefuted { IdentityMismatch }` against `executor` on `claim_id`** (ADR-0152 v3.1 J-4,
/// J-5): no signature — the proof is objective and names only the claim's executor.
fn executor_refuted(claim_id: Hash64, executor: PalwBondKeyV2, binding: PalwStepBindingV2) -> PalwConsensusObjectV2 {
    let evidence = borsh::to_vec(&PalwExecutorRefutedEvidenceV1 {
        version: PALW_EXECUTOR_REFUTED_VERSION_V1,
        claim_id,
        contradiction: PalwPanelContradictionV1::IdentityMismatch { binding },
        prompt_ids_opening: None,
        reporter_reveal: Vec::new(),
    })
    .expect("serializes");
    assert!(evidence.len() as u64 <= PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES, "the proof fits the offence cap ({} bytes)", evidence.len());
    PalwConsensusObjectV2::ObjectiveOffence {
        kind: PalwOffenceKindV1::ExecutorRefuted,
        accused: executor,
        evidence_id: palw_offence_evidence_digest_v1(&evidence),
        evidence,
    }
}

/// **The liability row `finalize_claim` writes beside `row`** (past `palw_offence_attribution`, M2:
/// the attribution copies; past `palw_rcore_plus`, S-3: the door, `basis_k`, `G_res` and the escrow)
/// — no signer, no lock: a kind-4 conviction reads neither.
fn liability_of(row: &PalwVestingRowV1) -> PalwPanelLiabilityRecordV1 {
    PalwPanelLiabilityRecordV1 {
        claim_id: row.claim_id,
        work_id: row.claim_id,
        class_id: row.class_id,
        execution_root: row.execution_root,
        output_root: Hash64::from_u64_word(0x0B7),
        executor_bond: row.producer_bond,
        voided_daa: None,
        void_reason: None,
        valid_signers: Vec::new(),
        locked_sompi: 0,
        expiry_daa: row.expiry_daa,
        settled_at_final: row.settled_at_final,
        job_identity: row.job_identity,
        free_prompt: row.free_prompt,
        trace_root: row.trace_root,
        segment_count: row.segment_count,
        licence_door: Some(row.licence_door),
        basis_k: row.basis_k,
        g_res_sompi: 1_000_000,
        escrowed_reward: row.escrowed_reward,
    }
}

/// The notes of `block`'s delta row as `node` stored it.
fn stored_notes(node: &T12Chain, block: BlockHash) -> Vec<PalwVestingNoteV1> {
    let (_, delta) = node.vp().palw_state_v2_store.read().delta_of(block).expect("a validated chain block keeps its delta row");
    palw_vesting_notes_of_delta_v1(&delta).cloned().collect()
}

/// **What `blocks`' notes say the vesting counters moved by**, as `node` stored them: `moved` counts
/// a row whole (every leg, the reserve's included) when it moves, `burned` a row or a seat share when
/// a conviction burns it (phase2-plan §2.5: a deletion alone cannot tell the two apart).
fn counted_by_notes(node: &T12Chain, blocks: &[BlockHash]) -> (i128, i128) {
    let (mut moved, mut burned) = (0i128, 0i128);
    for block in blocks {
        for note in stored_notes(node, *block) {
            match note {
                PalwVestingNoteV1::Moved { source: PalwVestingSourceV1::Row { .. }, legs } => {
                    moved += legs.iter().map(|leg| leg.amount as i128).sum::<i128>()
                }
                PalwVestingNoteV1::Burned { sompi, .. } | PalwVestingNoteV1::ShareBurned { sompi, .. } => burned += sompi as i128,
                _ => {}
            }
        }
    }
    (moved, burned)
}

/// **`challenger` (as `c_node` weighs it) strictly wins the one fork-choice comparator against
/// `incumbent` (as `i_node` weighs it)** — `compare_palw_candidates_v1`: frontier, safe weight, live
/// total, then the tip's hash — and not on the hash. A V2 node's deep-reorg gate asks nothing else
/// (`dns_reorg_outcome`, Unit D), so a fork that only out-works a chain is a coin flip on the tip
/// hash; the chains here differ in live work. The order is a function of the candidate's state, so
/// two nodes holding the two chains weigh them as one node holding both does.
fn dominates(c_node: &T12Chain, challenger: BlockHash, i_node: &T12Chain, incumbent: BlockHash, what: &str) {
    use kaspa_consensus_core::palw_fork_choice::compare_palw_candidates_v1;
    let (c, i) = (
        c_node.vp().palw_candidate_order_v2(challenger).expect("the challenger is weighable"),
        i_node.vp().palw_candidate_order_v2(incumbent).expect("the incumbent is weighable"),
    );
    assert_eq!(compare_palw_candidates_v1(&c, &i), std::cmp::Ordering::Greater, "{what}: {c:?} against {i:?}");
    assert!(
        (c.safe_frontier_blue_score, c.safe_weight, c.live_total) > (i.safe_frontier_blue_score, i.safe_weight, i.live_total),
        "{what}: by weight, not by the tip hash"
    );
}

/// `after − before` of the counters a reorg moved, `(moved, burned)`; `created` must not move (no
/// block here writes a `Final`).
fn counters_moved(before: PalwVestingCountersV1, after: PalwVestingCountersV1) -> (i128, i128) {
    assert_eq!(before.created, after.created, "no Final on either side");
    (after.moved as i128 - before.moved as i128, after.burned as i128 - before.burned as i128)
}

// ---- T48 ----------------------------------------------------------------------------------------

/// **T48: a reorg across a latch, a move, a mint and a burn lands on a fresh replay's root**
/// (phase2-plan §4 T48, §5.1; ADR-0152 V-3, I-9).
///
/// Two rows are planted on the tip `F`: `carried`, latched, six keys (a producer and five seats), and
/// `convicted`, not yet latched, three new keys, its DAA clock out and its licences in, carrying a
/// root that answers another job. Then:
///
/// * **node A** mines `N`, where step 3d latches `convicted` and moves `carried`, whose six keys leave
///   no room in V-7's budget of eight for `convicted`'s three — so it is latched and carried; `N+1`,
///   where it moves; and `N+2`, whose coinbase mints its producer leg — then two attempt blocks;
/// * **node B**, from `F`, mines the same latch at `N` in a block carrying `ExecutorRefuted` against
///   `convicted`'s producer, and at `N+1` the fold convicts it at step 3 — before 3d — and burns the
///   latched row whole (`burn_vesting_row`: burnable until it moves); then an attempt block and a
///   heartbeat;
/// * **node Z** sees A's first three blocks, then all four of B's, then A's last two. A V2 node's
///   deep-reorg gate is the one fork-choice comparator (frontier, safe weight, live total, tip hash),
///   so each switch is made to be decided by weight rather than by the tip hash: B's four blocks carry
///   one claim's live work against A's none, A's five blocks two claims against B's one — and each
///   side out-works the other by blue work when it arrives, as the sink search asks first.
///
/// At each switch Z's root is the one the chain's own builder committed — A's and B's are each a
/// fresh replay of their chain alone — block by block; the mint is a UTXO on Z exactly while A is the
/// chain (`get_virtual_utxo_entry`), and a signed spend of it is `MissingTxOutpoints` on B, at the
/// mempool and on the block path, while on A it is a coinbase output held only by Decision A's
/// maturity; the latch A wrote at `N` is reverted with A's `N` (I-9) and B's own latch stands in its
/// place; the conviction's ledger row, its mark on the liability row and S3's slash exist only on B;
/// and the `Moved`/`Burned` notes of
/// the blocks Z reverted and applied — read off Z's own delta rows, which a reorg keeps — account
/// exactly for the change in the rooted counters.
#[tokio::test]
async fn p2_t48_a_reorg_across_latch_move_mint_and_burn_lands_on_a_fresh_replay_s_root() {
    const PRODUCER_CARD: usize = 6;
    let carried = Hash64::from_u64_word(0x48_0001);
    let convicted = Hash64::from_u64_word(0x48_0002);
    let recorded_job = Hash64::from_u64_word(0x48_70B0);
    let mut a = mined().await;
    let binding = floor_binding_of(&a, Hash64::from_u64_word(0x48_A2C4));
    assert_ne!(binding.job_context.job_id, recorded_job, "the root the row carries answers another job");
    let floor = a.chain.bundle.base_class_id;
    let floor_artifact = a.chain.tip_state().1.class(&floor).expect("the floor").artifact_root;
    a.plant(|p, c| {
        c.vesting.insert(carried, p.row(carried, 0, 0, &[1, 2, 3, 4, 5], p.daa, Clock::Latched));
        let mut row = p.row(convicted, 1, PRODUCER_CARD, &[7, 0], p.daa, Clock::Licensed);
        row.class_id = floor;
        row.artifact_root = floor_artifact;
        row.execution_root = binding.committed_execution_root;
        row.job_identity = recorded_job;
        row.trace_root = binding.full_logits_trace_root;
        // X29: the retired claim's liability row stays while its vesting row does.
        c.panel_liabilities.insert(convicted, liability_of(&row));
        c.vesting.insert(convicted, row);
    });
    let order: Vec<Hash64> = a.planted.vesting_iter_by_expiry().map(|row| row.claim_id).collect();
    assert_eq!(order, vec![carried, convicted], "V-7's order: the carried row first");
    let fork = a.chain.sink();
    let row = a.rows[&convicted].clone();
    let z = follower(&a, fork).await;
    let mut b = follower(&a, fork).await;

    // ---- Node A: latch at N, move at N+1, mint at N+2. ----
    let n = a.step(beat()).await;
    let n_daa = n.block.header.daa_score;
    assert!(n.notes.contains(&PalwVestingNoteV1::Latched { claim_id: convicted, matured_at: n_daa }), "A: latched at N");
    assert!(n.moved_row(&carried) && !n.moved_row(&convicted), "A: the carried row takes the budget at N; the latched one waits");
    assert_eq!(n.child.vesting_row(&convicted).and_then(|r| r.matured_at), Some(n_daa), "A: latched and carried at N");
    let moved = a.step(beat()).await;
    assert!(moved.moved_row(&convicted), "A: it moves at N+1");
    let mint = a.step(beat()).await;
    let coinbase = &mint.block.transactions[0];
    let index = coinbase.outputs.iter().position(|o| *o == output(&row.producer)).expect("A: N+2's coinbase mints the producer leg");
    let mint_outpoint = TransactionOutpoint::new(coinbase.id(), index as u32);
    let mint_daa = mint.block.header.daa_score;
    let a_tail = [a.step(Kind::Attempt(3)).await, a.step(Kind::Attempt(4)).await];
    assert!(a.a_utxo_pays(&row.producer), "A: the mint is an output");

    // ---- Node B: the same latch at N, the conviction at N+1 — the row burns, never moves. ----
    let conviction = executor_refuted(convicted, b.chain.bonds[PRODUCER_CARD], binding);
    {
        let (_, parent) = b.chain.tip_state();
        let last = *parent.last_point().expect("a point");
        let point = PalwBlockContextV2 {
            block: Hash64::from_u64_word(0x48_6A7E),
            daa_score: n_daa,
            blue_score: last.blue_score + 1,
            subsidy: 0,
        };
        b.chain
            .vp()
            .palw_v2_validate_objects(&parent, b.sp(), &point, std::slice::from_ref(&conviction))
            .unwrap_or_else(|why| panic!("the gate admits the conviction of a planted row (its target is the row): {why}"));
    }
    let carrier = b.carrier(1, conviction, None);
    let b_n = b.step(Kind::Heartbeat(vec![carrier.clone()])).await;
    assert!(b_n.block.transactions.iter().any(|tx| tx.id() == carrier.id()), "B: the carrier rides N");
    assert!(b_n.notes.contains(&PalwVestingNoteV1::Latched { claim_id: convicted, matured_at: n_daa }), "B: the same latch at N");
    let b_burn = b.step(beat()).await;
    let offence_id = palw_executor_refuted_offence_id_v1(&b.chain.bonds[PRODUCER_CARD].0, &convicted);
    assert!(
        b_burn.notes.iter().any(|note| matches!(
            note,
            PalwVestingNoteV1::Burned { claim_id, kind: PalwOffenceKindV1::ExecutorRefuted, sompi, offence_id: burned_by, .. }
                if *claim_id == convicted && *sompi == row.total_sompi() && *burned_by == offence_id
        )),
        "B: the conviction at N+1 burns the latched row whole: {:?}",
        b_burn.notes
    );
    assert!(b_burn.child.consumed_offence(&offence_id).is_some(), "B: the kind-4 ledger row");
    assert!(
        b_burn.child.panel_liability(&convicted).is_some_and(|l| l.voided_daa.is_some()),
        "B: the liability row is marked convicted"
    );
    let b_tail = [b.step(Kind::Attempt(5)).await, b.step(beat()).await];
    let b_blocks = [&b_n, &b_burn, &b_tail[0], &b_tail[1]];
    assert!(b_blocks.iter().all(|s| !s.moved_row(&convicted)), "B: the row never moves");
    assert!(!b.a_utxo_pays(&row.producer), "B: nothing mints it");
    let b_tip = b_tail[1].child.clone();

    // A signed spend of the mint, by the payee.
    let spk = p2pkh_mldsa87_spk(row.producer.payload.as_byte_slice());
    let mut spend = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(mint_outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(row.producer.amount - 20_000, spk.clone())],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        Vec::new(),
    );
    sign_spend(
        &mut spend,
        UtxoEntry::new(row.producer.amount, spk, mint_daa, true),
        PRODUCER_CARD,
        a.chain.config.params.storage_mass_parameter,
    );
    let mempool =
        |node: &T12Chain| node.vp().validate_mempool_transaction(&mut MutableTransaction::from_tx(spend.clone()), &Default::default());
    let block_path = |node: &T12Chain, pov: u64| {
        let vp = node.vp();
        let stores = vp.virtual_stores.read();
        vp.validate_transaction_in_utxo_context(&spend, &stores.utxo_set, pov, TxValidationFlags::Full, None).map(|_| ())
    };
    let maturity_floor = a.chain.config.params.coinbase_maturity();
    assert!(matches!(mempool(&b.chain), Err(TxRuleError::MissingTxOutpoints)), "B alone: no such output");

    // ---- Node Z: A, then B, then A. ----
    let root_of = |node: &T12Chain, block: BlockHash| node.vp().palw_state_v2_store.read().state_root_of(block).expect("a delta row");
    let a_first: Vec<BlockHash> = [&n, &moved, &mint].iter().map(|s| s.block.header.hash).collect();
    let a_last: Vec<BlockHash> = a_tail.iter().map(|s| s.block.header.hash).collect();
    let b_all: Vec<BlockHash> = b_blocks.iter().map(|s| s.block.header.hash).collect();
    for s in [&n, &moved, &mint] {
        arrive(&z.chain, s.block.clone(), "A's block").await;
    }
    assert_eq!(z.chain.sink(), mint.block.header.hash, "Z follows A");
    let (_, at_a) = z.chain.tip_state();
    assert_eq!(at_a.state_root(), mint.child.state_root(), "Z on A: A's root");
    assert!(z.chain.ctx.consensus.get_virtual_utxo_entry(mint_outpoint).is_some(), "Z on A: the mint is an output");

    for s in b_blocks {
        arrive(&z.chain, s.block.clone(), "B's block").await;
    }
    dominates(&z.chain, b_all[3], &z.chain, mint.block.header.hash, "B over A");
    assert_eq!(z.chain.sink(), b_all[3], "B out-works and out-weighs A's three blocks: Z reorgs onto B");
    let (_, at_b) = z.chain.tip_state();
    assert_eq!(at_b.state_root(), b_tip.state_root(), "Z on B: the root of B's fresh replay");
    for block in &b_all {
        assert_eq!(root_of(&z.chain, *block), root_of(&b.chain, *block), "Z on B: block {block}'s root is B's");
    }
    assert!(at_b.vesting_row(&convicted).is_none() && at_b.consumed_offence(&offence_id).is_some(), "Z on B: burned, convicted");
    assert!(z.chain.ctx.consensus.get_virtual_utxo_entry(mint_outpoint).is_none(), "Z on B: the reorged-out mint is no output");
    assert!(matches!(mempool(&z.chain), Err(TxRuleError::MissingTxOutpoints)), "Z on B: the mempool refuses the spend");
    assert!(matches!(block_path(&z.chain, mint_daa + 10_000), Err(TxRuleError::MissingTxOutpoints)), "Z on B: and the block path");
    for (i, block) in a_first.iter().enumerate() {
        assert_eq!(
            stored_notes(&z.chain, *block),
            [&n, &moved, &mint][i].notes,
            "Z keeps A's reverted delta row {block} with A's notes"
        );
    }
    let flow_b = counted_by_notes(&z.chain, &b_all);
    let flow_a = counted_by_notes(&z.chain, &a_first);
    assert_eq!(
        counters_moved(at_a.vesting_counters(), at_b.vesting_counters()),
        (flow_b.0 - flow_a.0, flow_b.1 - flow_a.1),
        "A→B: the counters moved by B's notes less A's, inverted"
    );
    assert_eq!(flow_a.1, 0, "A burned nothing");
    assert_eq!(flow_b.1, row.total_sompi() as i128, "B burned the row");
    assert_eq!(flow_a.0 - flow_b.0, row.total_sompi() as i128, "the only move one side has and the other has not is the row's");

    for s in &a_tail {
        arrive(&z.chain, s.block.clone(), "A's block").await;
    }
    dominates(&z.chain, a_last[1], &z.chain, b_all[3], "A over B");
    assert_eq!(z.chain.sink(), a_last[1], "A's five blocks and two claims out-work and out-weigh B's: Z reorgs back");
    let (_, again) = z.chain.tip_state();
    assert_eq!(again.state_root(), a_tail[1].child.state_root(), "Z on A again: the root of A's fresh replay");
    for block in a_first.iter().chain(&a_last) {
        assert_eq!(root_of(&z.chain, *block), root_of(&a.chain, *block), "Z on A again: block {block}'s root is A's");
    }
    assert!(again.consumed_offence(&offence_id).is_none(), "Z on A again: the conviction went with B");
    assert_eq!(again.panel_liability(&convicted).map(|l| l.voided_daa), Some(None), "and so did its mark on the liability row");
    let producer_bond = a.chain.bonds[PRODUCER_CARD];
    assert_eq!(again.bond(&producer_bond).map(|b| b.collateral), a_tail[1].child.bond(&producer_bond).map(|b| b.collateral));
    assert!(
        at_b.bond(&producer_bond).expect("card 6").collateral < again.bond(&producer_bond).expect("card 6").collateral,
        "the slash exists only on B"
    );
    let minted = z.chain.ctx.consensus.get_virtual_utxo_entry(mint_outpoint).expect("Z on A again: the mint is an output again");
    assert_eq!((minted.amount, minted.is_coinbase), (row.producer.amount, true));
    match mempool(&z.chain) {
        Err(TxRuleError::ImmatureCoinbaseSpend(0, outpoint, ..)) => assert_eq!(outpoint, mint_outpoint),
        other => panic!("Z on A again: the output exists and only Decision A's maturity holds it, got {other:?}"),
    }
    // The entry's DAA is its accepting chain block's (N+2's child), as for every coinbase output.
    block_path(&z.chain, minted.block_daa_score + maturity_floor).expect("Z on A again: the block path spends it at its floor");
    let flow_a_all = counted_by_notes(&z.chain, &[a_first.clone(), a_last.clone()].concat());
    assert_eq!(
        counters_moved(at_b.vesting_counters(), again.vesting_counters()),
        (flow_a_all.0 - flow_b.0, flow_a_all.1 - flow_b.1),
        "B→A: the counters moved by A's notes less B's"
    );
    eprintln!(
        "[p2-t48] N at DAA {n_daa}: A {} blocks / B {} blocks; the row {} sompi minted on A at DAA {mint_daa}, burned on B",
        a_first.len() + a_last.len(),
        b_all.len(),
        row.total_sompi()
    );
}

// ---- T49 ----------------------------------------------------------------------------------------

/// **T49: a node that imports its pruning point inside a backlog mints what an archival node mints**
/// (phase2-plan §4 T49, §1.3; F12; ADR-0152 V-3, I-7).
///
/// The archival node plants, on its tip `P`: fifty latched rows (one to five seats each), three
/// reporter rows and a 1,020-row market queue — so the queue holds more than a drain, V-7 gives the
/// rows six of the eight slots, and the rows are carried for many blocks. `P` is the pruning point.
///
/// The importing node follows the archival node's blocks through `P`, sees the header of `P`'s
/// selected-chain child `T` (the witness `import_pruning_point_palw_state` checks the carriage's root
/// against) as a headers-first sync does, and is then left exactly as a pruned join leaves a node: no
/// PALW tip and no delta row at or below `P`. The archival node captures `P` and serves it, the
/// carriage crosses the wire as the borsh bytes `PruningPointPalwState` carries, and the importer
/// installs it. From then on everything the importing node knows about the backlog came through the
/// carriage — the rows, their latches, the reporter rows, the queue and the derived indexes the
/// import rebuilds (I-7).
///
/// Then, for `T` and ten more blocks, the importing node builds its OWN heartbeat for each slot the
/// archival node took, and it is the archival block byte for byte — header hash and coinbase — and the
/// archival block, arriving, is its sink with the archival node's root. Every archival block passes
/// `Minting::after`, so the eleven coinbases mint the reporter rows and the rows' legs one block after
/// each move.
#[tokio::test]
async fn p2_t49_a_node_that_imports_its_pruning_point_inside_a_backlog_mints_what_an_archival_node_mints() {
    const ROWS: u64 = 50;
    const REPORTERS: u64 = 3;
    const QUEUE: u64 = 1_020;
    let mut archival = minting(|p, c| {
        for n in 0..ROWS {
            let claim_id = Hash64::from_u64_word(0x49_0000 + n);
            let seats: Vec<usize> = (1..=1 + n % 5).map(|k| ((n + k) % 8) as usize).collect();
            c.vesting.insert(claim_id, p.row(claim_id, n, (n % 8) as usize, &seats, p.daa, Clock::Latched));
        }
        for j in 0..REPORTERS {
            c.reporter_rewards.insert(
                Hash64::from_u64_word(0x49_6E00 + j),
                PalwPayoutV2 { payload: p.payloads[(j + 2) as usize], amount: 250_000 + j },
            );
        }
        for i in 0..QUEUE {
            c.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: p.payloads[7], amount: 10_000 + i });
        }
    })
    .await;
    let p = archival.chain.sink();
    let planted = archival.planted.clone();
    assert_eq!((planted.vesting_len() as u64, planted.reporter_rewards_iter().count() as u64), (ROWS, REPORTERS));
    assert_eq!(planted.pending_payouts_iter().count() as u64, QUEUE, "a queue of 1,020");
    assert!(planted.vesting_iter_by_expiry().all(|row| row.matured_at.is_some()), "every row latched, waiting on V-7's budget");

    // The importing node: the archival node's blocks through P, validated by its own pipeline.
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let importer = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let below: Vec<Block> = chain_blocks(&archival.chain, p);
    let below_hashes: Vec<BlockHash> = below.iter().map(|b| b.header.hash).collect();
    for block in below {
        arrive(&importer, block, "a block through the pruning point").await;
    }
    assert_eq!(importer.sink(), p);

    // T, P's selected-chain child: mined by the archival node, its header seen first by the importer.
    let t_block = archival.heartbeat_template();
    let t = archival.insert(t_block).await;
    arrive(&importer, Block::from_header_arc(t.block.header.clone()), "T's header").await;

    // The archival node serves P, as `RequestPruningPointPalwState` answers.
    let vp = archival.chain.vp();
    vp.capture_pruning_point_palw_state(p);
    let wire = borsh::to_vec(&vp.pruning_point_palw_state(p).expect("the captured point is servable")).expect("serializes");
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).expect("the wire bytes decode");

    // The importer as a pruned join leaves it — then the import.
    {
        let vp = importer.vp();
        let mut store = vp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("no PALW tip");
        for block in std::iter::once(importer.config.params.genesis.hash).chain(below_hashes.iter().copied()) {
            store.delete_delta_for_tests(block).expect("no delta row at or below the pruning point");
        }
    }
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the served carriage installs against T's committed root");
    let (at, imported) = importer.tip_state();
    assert_eq!((at, imported.state_root()), (p, planted.state_root()), "the imported state is P's, root for root");
    assert!(
        matches!(importer.vp().palw_state_v2_store.read().delta_of(p), Err(kaspa_database::prelude::StoreError::KeyNotFound(_))),
        "and nothing below it came from the importer's own history"
    );
    assert_eq!(
        importer.vp().palw_candidate_state_v2_checked(importer.config.params.genesis.hash).map(|s| s.state_root()),
        Err(PalwWeighFaultV2::NoOpinion),
        "below the pruning point the importer holds nothing — an absence, not a fault"
    );

    // T and ten more: the importer's own block for each slot is the archival block, byte for byte.
    let mut slot = t;
    let mut rows_moved = 0usize;
    for i in 0..11 {
        let twin = beat_like(&importer, &slot.block);
        assert_eq!(twin.transactions[0], slot.block.transactions[0], "slot {i}: the importer's coinbase is the archival node's");
        assert_eq!(twin.header.hash, slot.block.header.hash, "slot {i}: and so is the whole block");
        arrive(&importer, slot.block.clone(), "the archival block").await;
        assert_eq!(importer.sink(), slot.block.header.hash, "slot {i}: the archival block is the importer's sink");
        assert_eq!(importer.tip_state().1.state_root(), slot.child.state_root(), "slot {i}: with the archival node's root");
        rows_moved += slot.moved().iter().filter(|(source, _)| matches!(source, PalwVestingSourceV1::Row { .. })).count();
        if i < 10 {
            let next = archival.heartbeat_template();
            slot = archival.insert(next).await;
        }
    }
    let (_, end) = archival.chain.tip_state();
    assert!(
        archival.ledger.reporters > 0 && archival.ledger.rows > 0,
        "the reporter rows and rows' legs were minted: {:?}",
        archival.ledger
    );
    assert!(
        rows_moved >= 11 && end.vesting_len() > 0,
        "at least a row a block moved, and the backlog is still carried ({} left)",
        end.vesting_len()
    );
    eprintln!(
        "[p2-t49] carriage at P: {} bytes; 11 blocks built identically by the importer; {rows_moved} rows moved, {} still carried; {}",
        wire.len(),
        end.vesting_len(),
        archival.books.summary()
    );
}

/// **T49's size half: a carriage at T-3(b)'s row bound is served within one peer's window**
/// (phase2-plan F12, §1.3; ADR-0152 T-3(b)).
///
/// T-3(b) bounds the live rows by the Finals of the last `121 + 3,000 + 6,000` DAA (`window_challenge_at`
/// + 1, then `window_court`, then the second clock's `2 × window_court`) at one floor `Final` a DAA:
/// 9,121 rows. Each is planted at its widest (five seats, every hash field inhabited), beside the
/// liability row and the five seats' locks X29 keeps while it vests. The archival node captures and
/// serves that tip; the bytes `PruningPointPalwState` carries stay under the requester's
/// `MAX_PALW_STATE_BYTES` and under one peer's serve share per window, and they decode and rebuild
/// to the committed root (the import's gate).
///
/// The two limits live in `kaspa-p2p-flows`, which this crate cannot name; they are restated here with
/// their sources, and a change there must be carried here.
#[tokio::test]
async fn p2_t49_a_carriage_at_the_t3b_row_bound_is_served_within_one_peer_window() {
    /// `protocol/flows/src/ibd/flow.rs`: the requester refuses a larger `PruningPointPalwState`.
    const MAX_PALW_STATE_BYTES: usize = 64 << 20;
    /// `protocol/flows/src/palw_gossip.rs`: one peer's serve share per 60 s window.
    const SERVE_BUDGET_BYTES_PER_PEER: usize = 48 << 20;
    let mut m = mined().await;
    let bound = m.sp().window_challenge_at(0) + 1 + 3 * m.sp().window_court();
    assert_eq!(bound, 9_121, "T-3(b)'s row bound with the second clock (phase2-plan F12)");
    let bonds = m.chain.bonds.clone();
    let word = |tag: u64, n: u64| Hash64::from_u64_word((tag << 48) | n);
    m.plant(|p, c| {
        for n in 0..bound {
            let claim_id = word(0x49B, n);
            let seats: Vec<usize> = (1..=5).map(|k| ((n + k) % 8) as usize).collect();
            let mut row = p.row(claim_id, n, (n % 8) as usize, &seats, p.daa + n, Clock::Licensed);
            row.class_id = word(0xC1A, n);
            row.execution_root = word(0xE7E, n);
            row.artifact_root = word(0xA27, n);
            row.job_identity = word(0x70B, n);
            row.trace_root = word(0x72A, n);
            row.segment_count = 4;
            c.panel_liabilities.insert(
                claim_id,
                PalwPanelLiabilityRecordV1 {
                    claim_id,
                    work_id: word(0x3A1, n),
                    class_id: row.class_id,
                    execution_root: row.execution_root,
                    output_root: word(0x0B7, n),
                    executor_bond: row.producer_bond,
                    voided_daa: None,
                    void_reason: None,
                    valid_signers: seats.iter().map(|s| (bonds[*s].0, word(0x516, n * 8 + *s as u64))).collect(),
                    locked_sompi: 5 * 1_173_690_000_000,
                    expiry_daa: row.expiry_daa,
                    settled_at_final: row.settled_at_final,
                    job_identity: row.job_identity,
                    free_prompt: false,
                    trace_root: row.trace_root,
                    segment_count: row.segment_count,
                    licence_door: Some(row.licence_door),
                    basis_k: row.basis_k,
                    g_res_sompi: 1_000_000,
                    escrowed_reward: row.escrowed_reward,
                },
            );
            for s in &seats {
                c.slashable_locks.insert(
                    (bonds[*s], claim_id),
                    PalwSlashableLockV1 {
                        claim: claim_id,
                        amount: 1_173_690_000_000,
                        expiry_daa: row.expiry_daa,
                        settled_at_final: row.settled_at_final,
                        attested: PalwSegmentMaskV2::full(4),
                        segments: 4,
                    },
                );
            }
            c.vesting.insert(claim_id, row);
        }
    });
    let tip = m.chain.sink();
    let vp = m.chain.vp();
    vp.capture_pruning_point_palw_state(tip);
    let served = vp.pruning_point_palw_state(tip).expect("the captured tip is servable");
    let wire = borsh::to_vec(&served).expect("serializes");
    eprintln!(
        "[p2-t49] carriage at the T-3(b) bound: {} rows, {} liabilities, {} locks = {} bytes ({:.1} MiB; {:.0} bytes a row with its X29 rows)",
        served.vesting.len(),
        served.panel_liabilities.len(),
        served.slashable_locks.len(),
        wire.len(),
        wire.len() as f64 / (1u64 << 20) as f64,
        wire.len() as f64 / bound as f64
    );
    assert_eq!(served.vesting.len() as u64, bound);
    assert!(wire.len() <= MAX_PALW_STATE_BYTES, "under the requester's cap");
    assert!(wire.len() <= SERVE_BUDGET_BYTES_PER_PEER, "within one peer's serve share for one window");
    let decoded: PalwStateCarriageV2 = borsh::from_slice(&wire).expect("the requester decodes it");
    let rebuilt = decoded.into_state(m.sp(), Some(m.planted.state_root())).expect("and it rebuilds to the committed root");
    assert_eq!(rebuilt.vesting_len() as u64, bound);
    palw_vesting_consistency_v1(&rebuilt).expect("V-3, with the indexes the import rebuilt");
}

// ---- the undecodable-delta test, extended to the vesting variants --------------------------------

/// **An undecodable vesting delta is a fault, and the reorg that needs it holds the sink** — the
/// extension phase2-plan §1.3 names of
/// `palw_v2_an_undecodable_delta_refuses_a_deep_reorg_rather_than_abstaining` to a delta holding the
/// new `Vesting`, `VestingNote` and `VestingCounters` entries (I-6: appended after `AnchorDaaPruned`).
///
/// Two latched rows are planted; the block `C` after the plant carries a plain transfer and moves the
/// first (six keys), and the next block `a0` accepts the transfer and moves the second — so `a0`'s delta
/// row holds every vesting entry kind a move writes and its UTXO diff is not empty. A second node
/// builds a competing chain from `C`; a third follows `C`, then takes `a0`:
///
/// 1. healthy, `a0`'s row decodes with the vesting entries in it and the walk back through it lands on
///    `C`'s root exactly;
/// 2. its first vesting entry's tag replaced by one no build defines, then the row cut inside that
///    entry: each is `DataInconsistency` at the store and `StoreUnreadable` to the weigh — a fault;
/// 3. with the damaged row in place the competing chain — which carries a claim, so it wins the
///    fork-choice comparator by live work — arrives heavier: the node cannot revert `a0`,
///    so it HOLDS at `a0` with `a0`'s UTXO set intact — the transfer `a0` accepted is still spent, its
///    change still there — and the competing blocks are not disqualified;
/// 4. the row deleted is `NoOpinion` — an absence, not a fault;
/// 5. the healthy row restored, the next competing block moves the sink to the competing chain, whose
///    root is its builder's.
///
/// t12 does not arm `palw_frontier_provenance` (ADR-0065 D2 is restated as unimplementable there), so
/// the D2 veto's reading of the fault is the original test's, on its preset; here the fault reaches
/// the reorg walk itself.
#[tokio::test]
async fn p2_an_undecodable_vesting_delta_is_a_fault_and_the_reorg_it_blocks_holds_the_sink() {
    let first = Hash64::from_u64_word(0x3B_0001);
    let second = Hash64::from_u64_word(0x3B_0002);
    let mut a = minting(|p, c| {
        c.vesting.insert(first, p.row(first, 0, 0, &[1, 2, 3, 4, 5], p.daa, Clock::Latched));
        c.vesting.insert(second, p.row(second, 1, 6, &[7, 0], p.daa, Clock::Latched));
    })
    .await;
    let (paid, spent) = transfer(&mut a, 2);
    let c_step = a.step(Kind::Heartbeat(vec![paid.clone()])).await;
    assert!(c_step.moved_row(&first) && !c_step.moved_row(&second), "C moves the first row; the second waits");
    let fork = c_step.block.header.hash;
    let mut b = follower(&a, fork).await;
    let z = follower(&a, fork).await;
    let a0 = a.step(beat()).await;
    assert!(a0.moved_row(&second), "a0 moves the second row");
    let a0_hash = a0.block.header.hash;
    let change = TransactionOutpoint::new(paid.id(), 0);
    // The competing chain carries a claim, so it out-weighs `a0` by live work — the comparator's own
    // key, not the tip hash — and the only thing that can hold the node on `a0` is the row.
    let b_blocks = [b.step(Kind::Attempt(3)).await, b.step(beat()).await, b.step(beat()).await];
    dominates(&b.chain, b_blocks[1].block.header.hash, &a.chain, a0_hash, "the competing chain over a0");

    arrive(&z.chain, a0.block.clone(), "a0").await;
    assert_eq!(z.chain.sink(), a0_hash);
    let vp = z.chain.vp();
    let utxo_intact = |what: &str| {
        let consensus = &z.chain.ctx.consensus;
        assert!(consensus.get_virtual_utxo_entry(spent).is_none(), "{what}: the transfer a0 accepted stays spent");
        assert!(consensus.get_virtual_utxo_entry(change).is_some(), "{what}: and its change stays an output");
    };
    utxo_intact("a0 healthy");

    // 1. Healthy.
    let (root, delta) = vp.palw_state_v2_store.read().delta_of(a0_hash).expect("a0's row decodes");
    let is_vesting = |entry: &PalwDeltaEntryV2| {
        matches!(entry, PalwDeltaEntryV2::Vesting { .. } | PalwDeltaEntryV2::VestingNote(_) | PalwDeltaEntryV2::VestingCounters { .. })
    };
    assert!(delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::Vesting { .. })), "a0's row holds a Vesting entry");
    assert!(delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::VestingNote(_))), "a VestingNote");
    assert!(delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::VestingCounters { .. })), "and a VestingCounters");
    assert_eq!(
        vp.palw_candidate_state_v2_checked(fork).map(|s| s.state_root()),
        Ok(c_step.child.state_root()),
        "the walk back through the vesting entries lands on C's root exactly"
    );
    let healthy = borsh::to_vec(&delta).expect("serializes");
    let at = borsh::to_vec(&delta.point).expect("serializes").len()
        + 4
        + delta.entries.iter().take_while(|e| !is_vesting(e)).map(|e| borsh::to_vec(e).expect("serializes").len()).sum::<usize>();
    let entry = delta.entries.iter().find(|e| is_vesting(e)).expect("a vesting entry");
    assert_eq!(&healthy[at..at + borsh::to_vec(entry).unwrap().len()], borsh::to_vec(entry).unwrap().as_slice(), "the entry's bytes");
    let write_row = |bytes: Vec<u8>| {
        vp.palw_state_v2_store
            .write()
            .set_delta_record_for_tests(a0_hash, PalwStateDeltaRecordV2 { state_root: root, delta_borsh: bytes })
            .expect("the fixture writes its own row")
    };

    // 2. Damaged inside the vesting entry: a fault, twice over.
    let mut unknown_tag = healthy.clone();
    unknown_tag[at] = 0xFF;
    let cut = healthy[..at + 3].to_vec();
    for (what, bytes) in [("an unknown tag", unknown_tag), ("a cut inside the entry", cut.clone())] {
        write_row(bytes);
        assert!(
            matches!(vp.palw_state_v2_store.read().delta_of(a0_hash), Err(kaspa_database::prelude::StoreError::DataInconsistency(_))),
            "{what}: the store names it undecodable"
        );
        assert_eq!(
            vp.palw_candidate_state_v2_checked(fork).map(|s| s.state_root()),
            Err(PalwWeighFaultV2::StoreUnreadable),
            "{what}: a fault of this node, never an absence"
        );
    }

    // 3. The reorg that needs the damaged row: the node holds, its UTXO set intact.
    for s in &b_blocks[..2] {
        arrive(&z.chain, s.block.clone(), "a competing block").await;
    }
    assert_eq!(z.chain.sink(), a0_hash, "the competing chain out-works a0, and the node HOLDS: it cannot revert a0");
    utxo_intact("holding on a damaged row");
    for s in &b_blocks[..2] {
        assert_ne!(
            z.chain.ctx.consensus.block_status(s.block.header.hash),
            BlockStatus::StatusDisqualifiedFromChain,
            "this node's gap is not the chain's fault"
        );
    }

    // 4. Deleted: an absence.
    vp.palw_state_v2_store.write().delete_delta_for_tests(a0_hash).expect("empty the row");
    assert_eq!(
        vp.palw_candidate_state_v2_checked(fork).map(|s| s.state_root()),
        Err(PalwWeighFaultV2::NoOpinion),
        "a row this node does not hold is an absence"
    );

    // 5. Restored: the next competing block moves the sink.
    write_row(healthy);
    arrive(&z.chain, b_blocks[2].block.clone(), "a competing block").await;
    assert_eq!(z.chain.sink(), b_blocks[2].block.header.hash, "with its row back, the node reorgs");
    assert_eq!(z.chain.tip_state().1.state_root(), b_blocks[2].child.state_root(), "onto the competing builder's root");
}
