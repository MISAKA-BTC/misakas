//! **ADR-0152 Phase 2, P2-4 — the EVM twin: maturity payouts and market settlements on testnet-12
//! with the EVM lane as shipped** (phase2-plan §4 T50, §5.4; ADR-0152 V-7, F6; ADR-0089 Decision 6).
//!
//! Every other test of the mint path runs the lane inert, because its harness stamps a simulated
//! clock onto templates after the build and the lane executes against the header's timestamp. Here
//! nothing is re-stamped: every block is the node's own template at this host's clock — an attempt
//! (algo 6) whose carriage, nonce and signature are all a producer adds, none of which the lane
//! reads, or a heartbeat (algo 8) the node's adapter shapes, re-committing the lane when it moves the
//! stamp into its slot. So `evm_commitment_root` is the builder's on every block, and a block the
//! node accepts as its sink is one whose validation re-executed the lane to the same root: build ==
//! validate, the lane included, on every block (`Minting::after` holds each to the mint path's checks
//! besides). The chain runs as far as the drift tolerance lets a beat be stamped ahead of the wall
//! clock, which is why the one heartbeat stamped into its slot comes early and the rest are attempts.
//!
//! **What the plan's T50 asked, and what V-7 now answers.** The plan (written before ADR-0152 v3.1's
//! market reserve) expected a maturity burst to refuse 3c's market actions. Under V-7 as landed a
//! burst takes at most `8 − PALW_V2_VESTING_MARKET_RESERVE` drain slots while market rows wait: the
//! market rows stand at most at the 1,024 cap after 3c (the non-market part is drained by then) and
//! 3d adds at most six, so step 1b of the next block leaves room for at least one two-row move,
//! whatever the backlog — room that step 3′'s refunds and 3c's earlier Filled moves then share, and
//! whose exhaustion is the fold's `PAYOUT_QUEUE_FULL` (`palw_state_v2`'s M-10 tests; T30/T58 for the
//! drain). The test therefore pins the landed rule at the processor: EVM sells folded at 3c in the
//! very block whose 3d moves a burst row are settled for their own reason (`MARKET_MISSING`: the line
//! has no seeded market), never for the queue; their settlements are the child's `MarketSettle` ops
//! in order; and the lane agrees on build and validate throughout.
//!
//! **The EVM account is funded the way a user funds one**: a `EVM_DEPOSIT_LOCK` output spent from a
//! card's fee float, claimed by a `DepositClaim` the node's own template carries. Its two sells are
//! EIP-1559 transactions signed with a fixed test key; the signature cannot be produced here
//! (`kaspa-consensus` links no secp256k1 signer), so they are fixtures, and the test decodes each and
//! checks its sender, target, nonce, chain id and calldata against this crate's own encoders before
//! it uses them — a fixture that drifts fails with the regeneration recipe.
use super::super::t12_round_lane_e2e::t12_with_harness_cards_and_evm;
use super::*;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::evm::model_market::{
    MISAKA_MODEL_WRITER, PALW_EVM_ACTION_SELL, PalwEvmSettlementOutcomeV1, refusal, send_action_sell_calldata,
};
use kaspa_consensus_core::evm::{DepositClaim, EVM_CHAIN_ID, EvmAddress, EvmSystemOp, EvmTemplateData};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_vesting_v1::PALW_V2_VESTING_MARKET_RESERVE;

/// The test account: the address of the secp256k1 key `[0x50; 32]`.
const ACCOUNT: [u8; 20] =
    [0xb3, 0xc7, 0x7f, 0xc7, 0xb3, 0xb1, 0xdd, 0x1a, 0x72, 0xb3, 0x5d, 0x7c, 0x72, 0x18, 0x11, 0xae, 0x12, 0xf6, 0x63, 0xaa];
/// What the account deposits: 1 MSK, far above two calls' gas at `MAX_FEE`.
const DEPOSIT: u64 = 100_000_000;
const GAS_LIMIT: u64 = 200_000;
/// 10 gwei — ten times the lane's initial base fee.
const MAX_FEE: u128 = 10_000_000_000;
/// The account's two sells, `(units_in, raw EIP-1559 bytes)`, at nonces 0 and 1: `sendAction(bytes)`
/// to the writer carrying `send_action_sell_calldata(line, units_in, 0)`, `line` the first model class
/// of testnet-12's genesis (a founding line, which the writer's view lists), no value, `GAS_LIMIT`,
/// `MAX_FEE`, no tip. Regenerate with any EIP-1559 signer over those fields and the key `[0x50; 32]`
/// (the failure message below prints the calldata and the fields).
const SELLS: [(u64, &str); 2] = [
    (
        1,
        concat!(
            "02f90150834d534b80808502540be40083030d4094000000000000000000000000000000000000f01380b8e4df42f68f0000000000000000",
            "0000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000084",
            "0100000274c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da40907",
            "0cb2c98b8e861598db902f7a0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000",
            "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c080a054eba2d49a",
            "9c85a48fbd73a0e02997e2fa60288915cda2dbc908bbfb4ba3a657a0619f1a9aa247fd19f86e0492102143ce81e7bab365fbc88a8fb131c3",
            "a543a993",
        ),
    ),
    (
        2,
        concat!(
            "02f90150834d534b01808502540be40083030d4094000000000000000000000000000000000000f01380b8e4df42f68f0000000000000000",
            "0000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000084",
            "0100000274c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da40907",
            "0cb2c98b8e861598db902f7a0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000",
            "000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000c001a0b1179f885d",
            "013e10bf66fe9060d8a97470d909b66b8f3cca3859bc851f7d1e55a05dbc1f2303a430aeece4d3ef6407d0b102a551cf5efae57eb669c4f6",
            "75a07c51",
        ),
    ),
];

/// **The EVM-active twin of `mined()`**: testnet-12 with the harness cards and the lane as shipped,
/// at genesis. `Minting`'s plant, `after` and books are the mint path's; its `step` is not (it
/// re-stamps), so blocks are made by [`own_attempt`] and [`own_beat`] and taken by [`take`].
fn evm_minting() -> Minting {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards_and_evm(true);
    assert!(config.params.is_evm_active(0) && config.params.palw_model_evm.is_some(), "the lane and its market face as shipped");
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let (_, tip) = chain.tip_state();
    let payloads = chain.bonds.iter().map(|b| tip.bond(b).expect("a genesis card").payout_payload).collect();
    Minting {
        chain,
        domain,
        payloads,
        planted: tip,
        rows: BTreeMap::new(),
        planted_reporters: 0,
        planted_market: 0,
        row_keys: BTreeSet::new(),
        reporter_keys: BTreeSet::new(),
        ledger: Ledger::default(),
        books: Books::default(),
        wallets: floats.into_iter().enumerate().collect(),
        last_attempt: None,
        nonce: 0x50_0000,
    }
}

fn no_evm() -> EvmTemplateData {
    EvmTemplateData { evm_coinbase: EvmAddress::from_bytes([0xCB; 20]), transactions: Vec::new(), system_ops: Vec::new() }
}

/// **Card `card`'s attempt on the node's own template, at this host's clock** — `T12Chain`'s attempt
/// without the re-stamp: the template (with `evm`'s payload candidates) keeps the timestamp its
/// builder executed the lane against; the producer sets the nonce, builds the carriage from
/// `palw_producer_facts_v2` at the template's point, wins the class lottery by its trace root and
/// signs. Returns the block and its claim id.
fn own_attempt(m: &mut Minting, card: usize, txs: Vec<Transaction>, evm: EvmTemplateData) -> (MutableBlock, Hash64) {
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
        PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
    };
    m.nonce += 1;
    let bond = m.chain.bonds[card];
    let consensus = &m.chain.ctx.consensus;
    let mut t = consensus
        .build_block_template_with_evm(
            MinerData::new(card_payout_spk(card), vec![]),
            Box::new(OnetimeTxSelector::new(txs)),
            TemplateBuildMode::Standard,
            evm,
        )
        .expect("a template");
    assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id), "the attempt lane");
    t.block.header.nonce = m.nonce;
    let facts = consensus.palw_producer_facts_v2(m.chain.bundle.base_class_id, Some(bond.0)).expect("the floor answers");
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    let pubkey = key.verification_key.as_ref().to_vec();
    facts.ready_to_produce(&pubkey).unwrap_or_else(|why| panic!("card {card} is not ready to produce: {why}"));
    let header = &t.block.header;
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
    let mut attempt = PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain: m.domain,
        challenge: challenge_v2(m.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
        class_id: facts.class_id,
        executor_bond: bond.0,
        executor_pubkey: pubkey,
        operator_id: facts.bond.as_ref().expect("a registered card").operator_id,
        artifact_root: facts.artifact_root,
        trace_root: Hash64::default(),
        output_root: Hash64::from_u64_word(0x0070_5000_0000_0000 | m.nonce),
        execution_root: Hash64::from_u64_word(0xE7EC_5000_0000_0000 | m.nonce),
        pwu: facts.pwu,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
    };
    let anchor = execution_anchor_v3(m.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
    let won = (0u64..4_000_000).any(|draw| {
        attempt.trace_root = Hash64::from_u64_word((m.nonce << 32) ^ draw ^ 0x7A50_0000_0000_0000);
        attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
        class_ticket_v3(&attempt, anchor) <= facts.class_target
    });
    assert!(won, "the floor's class lottery is winnable");
    let claim_id = attempt_id_v2(&attempt);
    let signature =
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, claim_id.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT, [0x50; 32])
            .expect("ML-DSA-87 signs")
            .as_ref()
            .to_vec();
    t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
    t.block.header.finalize();
    (t.block, claim_id)
}

/// **The node's own heartbeat** (the H1 miner's pass): its template at this host's clock, a nonce,
/// the lane's adapter. Returns the block and the header the builder produced.
fn own_beat(m: &mut Minting) -> (MutableBlock, Header) {
    m.nonce += 1;
    let mut t = m
        .chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    let built = t.block.header.clone();
    t.block.header.nonce = m.nonce;
    t.block.header.finalize();
    let (t, _) = m.chain.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open");
    (t.block, built)
}

/// Insert a block of [`own_attempt`] / [`own_beat`], demand it is the UTXO-valid sink — the node
/// re-executed the lane to the root its builder committed — and hold it to `Minting::after`.
async fn take(m: &mut Minting, block: MutableBlock, attempt: Option<(usize, Hash64)>, what: &str) -> Stepped {
    let (parent_hash, parent) = m.chain.tip_state();
    let block = block.to_immutable();
    let hash = block.header.hash;
    m.chain
        .ctx
        .consensus
        .validate_and_insert_block(block.clone())
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
    assert_eq!(m.chain.ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid, "{what}: UTXO-valid, the lane agreeing");
    assert_eq!(m.chain.sink(), hash, "{what} is the sink");
    assert_ne!(block.header.evm_commitment_root, Hash64::default(), "{what}: the lane committed");
    let stepped = m.after(parent_hash, parent, block, attempt.map(|(_, claim)| claim));
    // `after` reads an attempt's carve off its child's coinbase as the worker base less the carve,
    // paid to the attempt's card. An attempt that carries fee-paying transactions is paid its fee
    // share in the same output, so its carve is booked (the withheld side) but not read there.
    m.last_attempt = attempt.filter(|_| stepped.block.transactions.len() == 1).map(|(card, _)| (hash, card));
    stepped
}

/// Whether `block`'s coinbase renders `parent`'s first eight queue rows (the payouts it mints).
fn mints_the_queue(parent: &PalwChainStateV2, block: &Block) -> usize {
    let prefix: Vec<TransactionOutput> =
        parent.pending_payouts_iter().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).map(|(_, p)| output(p)).collect();
    assert!(
        block.transactions[0].outputs.windows(prefix.len().max(1)).any(|w| prefix.is_empty() || w == prefix.as_slice()),
        "the coinbase renders the parent queue's first {} rows",
        prefix.len()
    );
    prefix.len()
}

/// The account's sells, each decoded and checked against the fields it must carry.
fn sells(line: &Hash64) -> Vec<Vec<u8>> {
    SELLS
        .iter()
        .enumerate()
        .map(|(nonce, (units, raw_hex))| {
            let calldata = send_action_sell_calldata(line, *units, 0);
            let recipe = format!(
                "regenerate: EIP-1559, key [0x50; 32], chain_id {EVM_CHAIN_ID}, nonce {nonce}, gas_limit {GAS_LIMIT}, \
                 max_fee_per_gas {MAX_FEE}, max_priority_fee_per_gas 0, to 0x{}, value 0, input 0x{}",
                faster_hex::hex_string(&MISAKA_MODEL_WRITER.as_bytes()),
                faster_hex::hex_string(&calldata)
            );
            let mut raw = vec![0u8; raw_hex.len() / 2];
            faster_hex::hex_decode(raw_hex.as_bytes(), &mut raw).unwrap_or_else(|_| panic!("sell {nonce} is not hex; {recipe}"));
            let tx = kaspa_evm::tx::decode_eth_tx(&raw).unwrap_or_else(|e| panic!("sell {nonce} does not decode ({e:?}); {recipe}"));
            assert_eq!(
                (tx.from, tx.to, tx.nonce, tx.chain_id, tx.gas_limit, tx.max_fee_per_gas, tx.value, &tx.input),
                (
                    ACCOUNT,
                    Some(MISAKA_MODEL_WRITER.as_bytes()),
                    nonce as u64,
                    Some(EVM_CHAIN_ID),
                    GAS_LIMIT,
                    MAX_FEE,
                    [0u8; 32],
                    &calldata
                ),
                "sell {nonce} is not the fixture it must be; {recipe}"
            );
            raw
        })
        .collect()
}

/// **T50: with the lane as shipped, a maturity burst's payouts and the market's settlements ride the
/// node's own blocks, and the lane agrees on build and validate** (phase2-plan T50, §5.4).
///
/// On testnet-12 with the EVM lane active from genesis and nothing re-stamped:
///
/// 1. the node's first heartbeat, stamped at the clock; then a burst is planted — eight latched
///    six-key rows and 64 market rows, so V-7 keeps the market's two drain slots throughout;
/// 2. the next heartbeat's slot is two minutes ahead: the adapter stamps it there, into another
///    second than its builder's, and re-derives `evm_commitment_root` for the stamp — a heartbeat
///    restamped with the burst's first move queued, whose coinbase mints the parent's first eight
///    rows, and which the node accepts;
/// 3. an attempt carries a card's `EVM_DEPOSIT_LOCK` output to the test account; the next attempt's
///    template carries the `DepositClaim` for it — the account is funded as a user funds one;
/// 4. an attempt's payload carries the account's two sells on a founding model line; the next
///    attempt's lane executes them (two `ActionQueued`), and its fold, in ONE block, settles both at
///    step 3c — `Refused { MARKET_MISSING }`, in order, the room V-7 leaves the market after a drain
///    that took six burst keys and two market rows ≥ 2 — and moves a burst row at step 3d;
/// 5. the next attempt's template carries exactly those settlements as `MarketSettle` ops, in order,
///    and the node accepts it.
///
/// Every block is the node's own template and is its UTXO-valid sink (the lane re-executed to the
/// committed root), and every one passes `Minting::after`: the carve off each attempt's child
/// coinbase (but the lock's carrier's, whose fee share rides the same output), the parent queue's
/// first eight rows minted, the next-block plan, the latch, the queue lemma, every moved leg in the
/// child's first eight, V-3.
#[tokio::test]
async fn p2_t50_with_the_lane_as_shipped_a_burst_s_payouts_and_the_market_s_settlements_ride_the_node_s_own_blocks() {
    let mut m = evm_minting();
    let line = {
        let (_, genesis) = m.chain.tip_state();
        let base = m.chain.bundle.base_class_id;
        *genesis.classes_iter().map(|(id, _)| id).find(|id| **id != base).expect("testnet-12 registers a model class at genesis")
    };
    let raws = sells(&line);

    // 1. The first heartbeat, then the burst.
    let (b0, _) = own_beat(&mut m);
    take(&mut m, b0, None, "the first heartbeat").await;
    m.plant(|p, c| {
        for n in 0..8u64 {
            let claim_id = Hash64::from_u64_word(0x50_0000 + n);
            let producer = (n % 8) as usize;
            let seats: Vec<usize> = (1..=5).map(|k| ((n + k) % 8) as usize).collect();
            c.vesting.insert(claim_id, p.row(claim_id, n, producer, &seats, p.daa, Clock::Latched));
        }
        for i in 0..64 {
            c.pending_payouts.insert(market_key(i), PalwPayoutV2 { payload: p.payloads[7], amount: 10_000 + i });
        }
    });

    // 2. The heartbeat stamped into its slot, re-committed, minting.
    let (b1, built) = own_beat(&mut m);
    let (_, parent) = m.chain.tip_state();
    assert_ne!(b1.header.timestamp / 1000, built.timestamp / 1000, "the adapter stamped the beat into another second, its slot");
    assert_ne!(b1.header.evm_commitment_root, built.evm_commitment_root, "and re-derived the lane's commitment for the stamp");
    let restamped = take(&mut m, b1, None, "the restamped heartbeat").await;
    assert_eq!(mints_the_queue(&parent, &restamped.block), PALW_V2_MAX_PAYOUTS_PER_BLOCK, "a full drain of payouts");
    assert!(restamped.moved().iter().any(|(s, _)| matches!(s, PalwVestingSourceV1::Row { .. })), "and the burst's first move");

    // 3. The deposit: a lock output, then the claim in the node's own template.
    let (lock, lock_entry) = m.wallets.remove(&1).expect("card 1's float");
    let mut lock_tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(lock, vec![], 0, 1)],
        vec![
            TransactionOutput::new(
                DEPOSIT,
                kaspa_txscript::script_class::evm_deposit_lock_script(ACCOUNT, 1_000_000, 0, card_payout_spk(1).script()),
            ),
            TransactionOutput::new(lock_entry.amount - DEPOSIT - CARRIER_FEE, card_payout_spk(1)),
        ],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        Vec::new(),
    );
    sign_spend(&mut lock_tx, lock_entry, 1, m.chain.config.params.storage_mass_parameter);
    let (e1, claim) = own_attempt(&mut m, 0, vec![lock_tx.clone()], no_evm());
    let e1 = take(&mut m, e1, Some((0, claim)), "the attempt carrying the lock").await;
    assert!(e1.block.transactions.iter().any(|tx| tx.id() == lock_tx.id()), "the lock rides");
    let deposit = DepositClaim {
        deposit_outpoint: TransactionOutpoint::new(lock_tx.id(), 0),
        evm_address: EvmAddress::from_bytes(ACCOUNT),
        amount_sompi: DEPOSIT,
        claim_tip_sompi: 0,
    };
    let (e2, claim) = own_attempt(&mut m, 2, Vec::new(), EvmTemplateData { system_ops: vec![deposit.clone()], ..no_evm() });
    assert_eq!(e2.evm_payload.system_ops, vec![EvmSystemOp::DepositClaim(deposit)], "the node's template carries the claim");
    take(&mut m, e2, Some((2, claim)), "the attempt claiming the deposit").await;

    // 4. The sells ride a payload; the next block's lane queues them and its fold settles them.
    let (e3, claim) = own_attempt(&mut m, 3, Vec::new(), EvmTemplateData { transactions: raws.clone(), ..no_evm() });
    assert_eq!(e3.evm_payload.transactions, raws, "the payload carries both sells");
    take(&mut m, e3, Some((3, claim)), "the attempt carrying the sells").await;
    let (e4, claim) = own_attempt(&mut m, 4, Vec::new(), no_evm());
    let settling = take(&mut m, e4, Some((4, claim)), "the attempt whose lane executes the sells").await;
    let settlements = settling.child.evm_settlements();
    assert_eq!(settlements.len(), 2, "both sells were queued and folded: {settlements:?}");
    for (i, s) in settlements.iter().enumerate() {
        assert_eq!(
            (s.seq, s.account, s.line_id, s.action, s.escrow_sompi),
            (i as u32, EvmAddress::from_bytes(ACCOUNT), line, PALW_EVM_ACTION_SELL, 0)
        );
        assert_eq!(
            s.outcome,
            PalwEvmSettlementOutcomeV1::Refused { reason: refusal::MARKET_MISSING },
            "sell {i}: refused for its own reason — never PAYOUT_QUEUE_FULL: V-7 left the market its room"
        );
    }
    let moved_keys: usize = settling
        .moved()
        .iter()
        .filter(|(s, _)| matches!(s, PalwVestingSourceV1::Row { .. }))
        .map(|(_, legs)| legs.iter().filter(|leg| leg.takes_budget()).count())
        .sum();
    assert!(moved_keys > 0, "the same block's 3d moved a burst row");
    let drained_market =
        settling.parent.pending_payouts_iter().take(PALW_V2_MAX_PAYOUTS_PER_BLOCK).filter(|(k, _)| is_market(k)).count();
    assert!(drained_market >= PALW_V2_VESTING_MARKET_RESERVE, "the drain before 3c took the market's reserve");
    let after_drain = settling.parent.pending_payouts_iter().count() - PALW_V2_MAX_PAYOUTS_PER_BLOCK;
    assert!(
        PALW_V2_MAX_PENDING_PAYOUTS - after_drain >= PALW_V2_VESTING_MARKET_RESERVE,
        "so 3c had room for a two-row move ({after_drain} rows after the drain)"
    );

    // 5. The child's payload carries the settlements as `MarketSettle`, in order, and is accepted.
    let (e5, claim) = own_attempt(&mut m, 5, Vec::new(), no_evm());
    let ops: Vec<EvmSystemOp> = settlements.iter().map(|s| EvmSystemOp::MarketSettle(*s)).collect();
    assert_eq!(e5.evm_payload.system_ops, ops, "the child's template carries its parent's settlements");
    let settled = take(&mut m, e5, Some((5, claim)), "the attempt settling the sells").await;
    assert_eq!(settled.block.evm_payload.system_ops, ops);
    eprintln!(
        "[p2-t50] line {line}: {} settlements refused MARKET_MISSING in a block moving {moved_keys} burst keys; {}",
        settlements.len(),
        m.books.summary()
    );
}
