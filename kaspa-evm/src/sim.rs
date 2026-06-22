//! kaspa-pq EVM Lane (ADR-0020 §16): read-only `eth_call` / `eth_estimateGas`
//! simulation. Seeds a fresh revm `CacheDB` from a committed state snapshot and
//! transacts WITHOUT committing — it never mutates consensus state, so it is
//! safe to run on demand from the RPC layer.

use crate::snapshot::seed_cachedb;
use crate::{EVM_SPEC_ID, EvmExecError};
use kaspa_consensus_core::evm::{EvmAddress, EvmStateSnapshot, EvmU256};
use revm::Evm;
use revm::primitives::{Address, B256, ExecutionResult, TxEnv, TxKind, U256};

/// The `(from, to, value, data, gas)` of an `eth_call` / `eth_estimateGas` request.
#[derive(Clone, Debug, Default)]
pub struct EthCall {
    pub from: EvmAddress,
    /// `None` ⇒ contract creation.
    pub to: Option<EvmAddress>,
    pub value: EvmU256,
    pub data: Vec<u8>,
    /// `0` ⇒ use the block gas limit.
    pub gas_limit: u64,
}

/// The block context the call executes against (the canonical EVM head).
#[derive(Clone, Debug, Default)]
pub struct EthCallEnv {
    pub chain_id: u64,
    pub number: u64,
    pub timestamp: u64,
    pub coinbase: EvmAddress,
    pub gas_limit: u64,
}

/// Outcome of a simulated call.
#[derive(Clone, Debug)]
pub struct EthCallOutcome {
    pub success: bool,
    pub output: Vec<u8>,
    pub gas_used: u64,
}

#[inline]
fn to_address(a: &EvmAddress) -> Address {
    Address::from_slice(&a.as_bytes())
}

/// The effective gas cap for a request (the call's own limit, else the block's,
/// else a generous default).
#[inline]
fn effective_gas(call_gas: u64, env_gas: u64) -> u64 {
    if call_gas != 0 {
        call_gas
    } else if env_gas != 0 {
        env_gas
    } else {
        30_000_000
    }
}

/// Run `call` against `snapshot` read-only (no commit). `Err` only on a DB /
/// setup fault; a reverted or halted call returns `Ok` with `success = false`
/// (and any revert data in `output`).
pub fn simulate_call(snapshot: &EvmStateSnapshot, env: &EthCallEnv, call: &EthCall) -> Result<EthCallOutcome, EvmExecError> {
    let mut db = seed_cachedb(snapshot)?;
    let gas_limit = effective_gas(call.gas_limit, env.gas_limit);

    // Build the tx env separately, then assign it (mirrors the executor pattern).
    let mut txenv = TxEnv::default();
    txenv.caller = to_address(&call.from);
    txenv.transact_to = match &call.to {
        Some(a) => TxKind::Call(to_address(a)),
        None => TxKind::Create,
    };
    txenv.value = U256::from_be_bytes(call.value.to_be_bytes());
    txenv.data = call.data.clone().into();
    txenv.gas_limit = gas_limit;
    // eth_call pays no fee; a zero basefee (below) makes a zero gas price valid.
    txenv.gas_price = U256::ZERO;
    // `None` nonce ⇒ revm skips the nonce check (read-only semantics).
    txenv.nonce = None;
    txenv.chain_id = Some(env.chain_id);

    let mut evm = Evm::builder()
        .with_db(&mut db)
        .with_spec_id(EVM_SPEC_ID)
        .modify_cfg_env(|c| c.chain_id = env.chain_id)
        .modify_block_env(|b| {
            b.number = U256::from(env.number);
            b.timestamp = U256::from(env.timestamp);
            b.coinbase = to_address(&env.coinbase);
            b.gas_limit = U256::from(gas_limit);
            // eth_call charges no fee → zero basefee so a zero gas price is admissible.
            b.basefee = U256::ZERO;
            b.difficulty = U256::ZERO;
            b.prevrandao = Some(B256::ZERO);
        })
        // Honour the F002 withdraw intercept so a call targeting it simulates faithfully.
        .append_handler_register(crate::withdraw::register_f002_withdraw)
        .build();
    evm.context.evm.env.tx = txenv;

    let outcome = evm.transact().map_err(|e| EvmExecError::InvalidTx(format!("{e:?}")))?;
    Ok(match outcome.result {
        ExecutionResult::Success { output, gas_used, .. } => {
            EthCallOutcome { success: true, output: output.into_data().to_vec(), gas_used }
        }
        ExecutionResult::Revert { output, gas_used } => EthCallOutcome { success: false, output: output.to_vec(), gas_used },
        ExecutionResult::Halt { gas_used, .. } => EthCallOutcome { success: false, output: Vec::new(), gas_used },
    })
}

/// `eth_estimateGas`: binary-search the minimal gas limit that lets the call
/// succeed. `Err` if the call reverts even at the gas cap.
pub fn estimate_gas(snapshot: &EvmStateSnapshot, env: &EthCallEnv, call: &EthCall) -> Result<u64, EvmExecError> {
    let cap = effective_gas(0, env.gas_limit);
    let at_cap = simulate_call(snapshot, env, &EthCall { gas_limit: cap, ..call.clone() })?;
    if !at_cap.success {
        return Err(EvmExecError::InvalidTx("execution reverted at the gas cap (cannot estimate gas)".to_string()));
    }
    // Invariant: `lo` fails (below intrinsic), `hi` succeeds; converge to min `hi`.
    let mut lo = 20_999u64;
    let mut hi = cap;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        // A hard execution/setup fault (DB miss, invalid env) is NOT "needs more
        // gas" — propagate it rather than silently returning an inflated estimate
        // (audit H-03). Only a genuine revert/OOG (`Ok(false)`) bumps `lo`.
        let ok = simulate_call(snapshot, env, &EthCall { gas_limit: mid, ..call.clone() })?.success;
        if ok {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}
