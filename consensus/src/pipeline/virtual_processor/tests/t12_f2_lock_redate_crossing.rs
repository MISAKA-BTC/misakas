//! **Lane F2-lock at the processor, on testnet-12: `Params::palw_final_lock_life_retro` crossed by a
//! real chain.**
//!
//! testnet-12 with harness cards (`t12_round_lane_e2e`'s harness: harness keys on the eight cards, the
//! premine imported, the EVM lane inert, PoW skipped and nothing else), lane V02's two lock fences
//! (`palw_final_lock_life`, `palw_final_lock_full_collateral`) at [`V02`] as the fleet runs them at DAA
//! 750, and lane F2-lock at [`H`]. Every claim is card 0's floor attempt, bound by card 7's attempt at
//! its anchor slot (SW-8), licensed by a quorum of the drawn seats' real ML-DSA-87 `Valid` receipts
//! assembled by the node's own assembler and carried on a funded 0x4b carrier:
//!
//! * claim A is licensed and `Final` below lane V02: its seat locks are dated `F + window_court`;
//! * claim D is licensed below lane V02 (its locks `max(L, H(c)) + window_court`) and still licensed
//!   at the crossing;
//! * **the crossing block** (the first chain block at or past `H`, built by the node's own template
//!   and folded by its own pipeline) re-dates A's locks to `max(F + 1,000, H)` and leaves D's;
//! * **D's `Final` past `H`** dates its locks to exactly `F + 1,000` (lane V02 alone keeps the licence's);
//! * a second armed node fed the chain reaches the same sink and the same PALW root;
//! * a node running lane V02 without this fence accepts every block below `H` with the same root, and
//!   refuses the chain at the crossing (the re-date is in the committed root: a flag day).
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_FINAL_LOCK_LIFE_DAA_V1, PalwBondKeyV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj,
};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// Lane V02's two lock fences (the fleet's DAA 750, moved here so a short chain has long locks).
const V02: u64 = 250;
/// Lane F2-lock's height: past lane V02, below claim A's `F + 1,000` (so A's locks end there, not at
/// `H`), and below claim D's `Final` (so D is licensed and not yet `Final` at the crossing).
const H: u64 = 300;

/// testnet-12 with harness cards, lane V02's two lock fences at [`V02`] and, when `retro`, lane F2-lock
/// at [`H`] — each through its field and its mirror, as the operator's build assembles them.
fn t12_rules(retro: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, _, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_final_lock_life_retro, None, "testnet-12 ships the fence dormant");
    let mut params = config.params.clone();
    params.palw_final_lock_life = Some(ForkActivation::new(V02));
    params.sync_palw_final_lock_life();
    params.palw_final_lock_full_collateral = Some(ForkActivation::new(V02));
    params.sync_palw_final_lock_full_collateral();
    params.palw_final_lock_life_retro = retro.then(|| ForkActivation::new(H));
    params.sync_palw_final_lock_life_retro();
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

/// The chain, with every block this test inserted recorded as `(hash, DAA, the PALW root after it)`.
struct Run {
    chain: T12Chain,
    domain: Hash64,
    blocks: Vec<(BlockHash, u64, Hash64)>,
}

impl Run {
    fn record(&mut self, block: &Block) {
        let (tip, state) = self.chain.tip_state();
        assert_eq!(tip, block.header.hash, "the recorded block is the tip");
        self.blocks.push((block.header.hash, block.header.daa_score, state.state_root()));
    }

    fn sink_daa(&self) -> u64 {
        self.chain.daa_of(self.chain.sink())
    }

    async fn beat(&mut self, txs: Vec<Transaction>) {
        let ttpb = self.chain.config.params.target_time_per_block();
        let block = self.chain.heartbeat(ttpb, txs).await;
        self.record(&block);
    }

    async fn beat_to(&mut self, daa: u64) {
        for _ in 0..(4 * daa + 400) {
            if self.sink_daa() >= daa {
                return;
            }
            self.beat(Vec::new()).await;
        }
        panic!("the chain reaches DAA {daa}");
    }

    async fn attempt(&mut self, card: usize) -> Hash64 {
        let ttpb = self.chain.config.params.target_time_per_block();
        let (block, claim_id) = self.chain.attempt(card, ttpb, Vec::new(), &|_| true).await;
        self.record(&block);
        claim_id
    }

    /// A floor claim by card 0 accepted at or past `from`, bound by card 7's attempt at its slot, and
    /// licensed by a quorum of its drawn seats' signed `Valid`s — assembled by the node's own assembler
    /// and carried on a 0x4b carrier funded by card `payer`'s genesis fee float. Returns the claim and
    /// its licence DAA.
    async fn licensed_claim(&mut self, from: u64, float: &(TransactionOutpoint, UtxoEntry), payer: usize) -> (Hash64, u64) {
        self.beat_to(from).await;
        let claim_id = self.attempt(0).await;
        let slot = self.chain.tip_state().1.claim(&claim_id).expect("the attempt made its claim").bind_base_daa()
            + self.chain.bundle.panel.anchor_delay();
        self.beat_to(slot).await;
        self.attempt(7).await;
        let (_, state) = self.chain.tip_state();
        assert!(
            matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
            "card 7's attempt at the slot binds the claim"
        );
        let panel = state.panel(&claim_id).expect("a bound claim has a panel").clone();
        let signed_daa = self.chain.ctx.consensus.get_virtual_daa_score();
        let receipts: Vec<PalwSeatReceiptV2> = panel
            .seats
            .iter()
            .take(self.chain.bundle.panel.quorum() as usize)
            .map(|seat| {
                let card = self.chain.bonds.iter().position(|b| *b == seat.bond).expect("every seat is a genesis card");
                let message = palw_receipt_message_v2(self.domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
                let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                    &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
                    message.as_byte_slice(),
                    PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                    [0x11u8; 32],
                )
                .expect("sign")
                .as_ref()
                .to_vec();
                PalwSeatReceiptV2 {
                    claim: claim_id,
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: seat.bond,
                    signed_daa,
                    signature,
                }
            })
            .collect();
        let object = self.chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
        assert!(matches!(object, Obj::ReceiptLicensed { .. }), "a Valid quorum licenses the floor claim: {object:?}");
        let carrier = {
            use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
            let payload =
                borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
            let (float_outpoint, float_entry) = float.clone();
            let mut tx = Transaction::new(
                crate::constants::TX_VERSION,
                vec![TransactionInput::new(float_outpoint, vec![], 0, 1)],
                vec![TransactionOutput::new(float_entry.amount - 300_000, card_payout_spk(payer))],
                0,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                payload,
            );
            sign_spend(&mut tx, float_entry, payer, self.chain.config.params.storage_mass_parameter);
            tx
        };
        self.beat(vec![carrier]).await;
        self.beat(Vec::new()).await; // accepts the carrying block's transactions
        match self.chain.tip_state().1.claim(&claim_id).unwrap().phase.clone() {
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => (claim_id, licensed_daa),
            other => panic!("the carried quorum licenses the claim; it is {other:?}"),
        }
    }

    /// Heartbeats until `claim` is `Final`; returns its `Final` DAA.
    async fn until_final(&mut self, claim: Hash64) -> u64 {
        for _ in 0..2_000 {
            if let Some(PalwClaimPhaseV2::Final { final_daa }) = self.chain.tip_state().1.claim(&claim).map(|c| c.phase.clone()) {
                return final_daa;
            }
            self.beat(Vec::new()).await;
        }
        panic!("claim {claim} reaches Final");
    }

    /// `(seat, expiry)` of every lock on `claim`.
    fn locks(&self, claim: Hash64) -> Vec<(PalwBondKeyV2, u64)> {
        let (_, state) = self.chain.tip_state();
        self.chain.bonds.iter().filter_map(|seat| state.slashable_lock(*seat, claim).map(|lock| (*seat, lock.expiry_daa))).collect()
    }
}

/// Every chain block of `run`, genesis excluded, in chain order (the selected-parent walk).
fn chain_blocks(run: &Run) -> Vec<Block> {
    let vp = run.chain.vp();
    let genesis = run.chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = run.chain.sink();
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes.into_iter().map(|hash| run.chain.ctx.consensus.get_block(hash).expect("the node holds its chain")).collect()
}

/// **The crossing** (module doc).
#[tokio::test]
async fn t12_the_crossing_block_redates_the_long_final_locks_on_a_real_chain() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_rules(true);
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let sp = chain.vp().palw_state_params_v2.clone().expect("testnet-12 is ConsensusV2");
    assert_eq!(sp.final_lock_life_retro_from_daa(), Some(H), "the processor carries the fence it was built with");
    assert_eq!(sp.final_lock_life_from_daa(), Some(V02));
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let mut run = Run { chain, domain, blocks: Vec::new() };
    let wc = bundle.state.window_court();
    let life = PALW_FINAL_LOCK_LIFE_DAA_V1;

    // ---- Claim A: licensed and Final below lane V02 --------------------------------------------------
    run.beat(Vec::new()).await;
    let (a, la) = run.licensed_claim(1, &floats[0], 0).await;
    let fa = run.until_final(a).await;
    assert!(fa < V02, "A is Final below lane V02 ({fa})");
    let a_locks = run.locks(a);
    assert_eq!(a_locks.len(), bundle.panel.quorum() as usize, "a lock per Valid signer");
    assert!(a_locks.iter().all(|(_, expiry)| *expiry == fa + wc), "A: F + window_court ({a_locks:?})");
    assert!(fa + life > H, "the premise: A's F + 1,000 lies past H");

    // ---- Claim D: licensed below lane V02, still licensed at the crossing ------------------------------
    let (d, ld) = run.licensed_claim(V02 - 45, &floats[1], 1).await;
    assert!(ld < V02, "D is licensed below lane V02 ({ld})");
    let d_locks = run.locks(d);
    let d_long = d_locks[0].1;
    assert!(d_long >= ld + wc && d_locks.iter().all(|(_, expiry)| *expiry == d_long), "D: the licence's long lock ({d_locks:?})");
    run.beat_to(H - 1).await;
    assert!(run.sink_daa() < H, "the chain stands below H");
    assert!(
        matches!(run.chain.tip_state().1.claim(&d).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "D is not Final at the crossing"
    );
    let below = run.blocks.len();

    // ---- The crossing block ------------------------------------------------------------------------
    run.beat_to(H).await;
    let (crossing_hash, crossing_daa, _) = *run.blocks.last().unwrap();
    assert!(crossing_daa >= H && run.blocks[run.blocks.len() - 2].1 < H, "the first chain block at or past H");
    let a_after = run.locks(a);
    assert!(a_after.iter().all(|(_, expiry)| *expiry == fa + life), "A: re-dated to max(F + 1,000, H) = F + 1,000 ({a_after:?})");
    assert_eq!(run.locks(d), d_locks, "D (licensed, not Final): untouched");

    // ---- D's Final past H ----------------------------------------------------------------------------
    let fd = run.until_final(d).await;
    assert!(fd >= H);
    let d_after = run.locks(d);
    assert!(d_after.iter().all(|(_, expiry)| *expiry == fd + life), "D: dated exactly F + 1,000 at its Final ({d_after:?})");
    eprintln!(
        "[t12-f2lock] V02 {V02}, H {H}: A licensed {la}, Final {fa}, locks {} -> {} at the crossing block {crossing_hash} (DAA \
         {crossing_daa}); D licensed {ld} (locks {d_long}), Final {fd} -> {}; {} blocks",
        fa + wc,
        fa + life,
        fd + life,
        run.blocks.len()
    );

    // ---- A second armed node agrees ----------------------------------------------------------------
    let blocks = chain_blocks(&run);
    let (_, armed_tip) = run.chain.tip_state();
    let follower = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for block in &blocks {
        let hash = block.header.hash;
        follower
            .ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("block {hash} of the armed chain was refused by a second armed node: {e}"));
    }
    assert_eq!(follower.sink(), run.chain.sink(), "the second node walks the same chain");
    assert_eq!(follower.tip_state().1.state_root(), armed_tip.state_root(), "and folds the same PALW root");

    // ---- A node without the fence: the same chain below H, refused at the crossing -------------------
    let (lane_v02_config, lane_v02_bundle, ..) = t12_rules(false);
    let released = t12_genesis_chain(&lane_v02_config, &lane_v02_bundle, &premine, &floats);
    let roots: std::collections::HashMap<BlockHash, Hash64> = run.blocks.iter().map(|(hash, _, root)| (*hash, *root)).collect();
    let mut refused_at = None;
    for block in &blocks {
        let (hash, daa) = (block.header.hash, block.header.daa_score);
        let inserted = released.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await;
        if inserted.is_err() || released.sink() != hash {
            refused_at = Some(daa);
            break;
        }
        assert!(daa < H || hash == crossing_hash, "a block past the crossing folds on the lane-V02 node only through it");
        if let Some(root) = roots.get(&hash)
            && daa < H
        {
            assert_eq!(released.tip_state().1.state_root(), *root, "below H (DAA {daa}) the two nodes fold the same root");
        }
    }
    let refused_at = refused_at.expect("the lane-V02 node refuses the armed chain past H");
    assert!(refused_at >= H && refused_at <= crossing_daa + 1, "refused at the crossing (or its child): DAA {refused_at}");
    assert!(below > 0);
}
