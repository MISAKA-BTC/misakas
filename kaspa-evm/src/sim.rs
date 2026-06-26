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
    /// PREA P0-1: whether the F003 verify precompile is active at the simulated
    /// head (`head_daa_score >= evm_f003_mldsa_verify_activation_daa_score`). The
    /// RPC layer sets it so `eth_call`/`eth_estimateGas` register the SAME handler
    /// set the executor uses (parity). `false` (inert) ⇒ no F003 in simulation,
    /// matching the executor below the fence.
    pub f003_active: bool,
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
        // Register the MISAKA precompiles through the SAME shared seam the executor
        // uses (parity): F002 always, F003 iff active at this head.
        .append_handler_register_box({
            let f003_active = env.f003_active;
            Box::new(move |h| crate::precompiles::register_all_misaka_precompiles(h, f003_active))
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::evm::{
        EVM_CHAIN_ID, F003_PREA_OP_MLDSA87_CONTEXT, F003_PREA_ROOT_MLDSA87_CONTEXT, F003_VERSION_PREA_ROOT,
        MISAKA_MLDSA_VERIFY_PRECOMPILE,
    };
    use kaspa_hashes::{blake2b_512_address_payload, blake2b_512_keyed};
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;

    /// PREA P0-1: a CALL to F003 with a valid version-0x02 input through the REAL
    /// simulation seam returns the 32-byte ABI `true` WHEN the fence is active, and
    /// behaves as a call to an empty account (success, empty output) when inert.
    /// This exercises the actual handler registration + gas charge + ABI encoding +
    /// the fence gate — the same `register_all_misaka_precompiles` the executor uses
    /// (executor↔simulation parity).
    #[test]
    fn f003_call_through_simulation_active_vs_inert() {
        // The op preimage the smart account's executeRoot would pass (canonical op
        // bytes); F003 hashes it with the OP context and verifies the sig over that.
        let preimage = b"misaka-pq-account/executeRoot|to=0x..|value=0|epoch=0|nonce=0".to_vec();
        let kp = mldsa::generate_key_pair([0x91u8; 32]);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let digest = blake2b_512_keyed(F003_PREA_OP_MLDSA87_CONTEXT, &preimage);
        let sig = mldsa::sign(&kp.signing_key, digest.as_byte_slice(), F003_PREA_ROOT_MLDSA87_CONTEXT, [0x42u8; 32])
            .expect("sign")
            .as_ref()
            .to_vec();
        let payload = blake2b_512_address_payload(&pubkey).as_bytes().to_vec();

        let mut input = vec![F003_VERSION_PREA_ROOT];
        input.extend_from_slice(&payload);
        input.extend_from_slice(&pubkey);
        input.extend_from_slice(&sig);
        input.extend_from_slice(&preimage);

        let call = EthCall { to: Some(MISAKA_MLDSA_VERIFY_PRECOMPILE), data: input, ..Default::default() };
        let snapshot = EvmStateSnapshot::default();

        // ACTIVE: the handler is registered → 32-byte ABI true.
        let env_on = EthCallEnv { chain_id: EVM_CHAIN_ID, gas_limit: 30_000_000, f003_active: true, ..Default::default() };
        let out = simulate_call(&snapshot, &env_on, &call).expect("sim");
        assert!(out.success, "F003 call succeeds when active");
        assert_eq!(out.output.len(), 32, "32-byte ABI bool");
        assert_eq!(out.output[31], 1, "valid signature ⇒ ABI true");
        assert!(out.output[..31].iter().all(|&b| b == 0), "high 31 bytes are zero");

        // INERT: no handler → a call to an empty account (success, EMPTY output).
        let env_off = EthCallEnv { f003_active: false, ..env_on.clone() };
        let out_inert = simulate_call(&snapshot, &env_off, &call).expect("sim");
        assert!(out_inert.success);
        assert!(out_inert.output.is_empty(), "inert ⇒ F003 is an empty account, no ABI bool");

        // ACTIVE but a tampered op preimage (last byte) ⇒ the digest changes, the sig
        // no longer verifies ⇒ ABI false (still a successful call). This is the
        // signature↔operation binding the smart account relies on.
        let mut bad = call.clone();
        let n = bad.data.len();
        bad.data[n - 1] ^= 0x01;
        let out_bad = simulate_call(&snapshot, &env_on, &bad).expect("sim");
        assert!(out_bad.success);
        assert_eq!(out_bad.output.len(), 32);
        assert_eq!(out_bad.output[31], 0, "invalid signature ⇒ ABI false");
    }
}
