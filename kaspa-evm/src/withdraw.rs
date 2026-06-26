//! F002 `MISAKA_WITHDRAW` — the EVM → UTXO exit (design v0.4 §9.3).
//!
//! Implemented as a **call-frame interception** (a revm handler register that
//! wraps `handler.execution.call`), NOT a standard precompile: the stateless
//! precompile ABI receives only `(input, gas_limit, ctx)` and cannot see
//! `msg.sender` / `msg.value`, both of which the withdraw needs. The intercept
//! sees the full `CallInputs`.
//!
//! Semantics (`withdraw` = a payable call to `0x…F002`):
//! - calldata = `[version: u16 BE][script bytes]` — the destination UTXO
//!   `ScriptPublicKey`. It MUST be the PQ-standard ML-DSA P2PKH class (the
//!   UTXO lane is PQ-only; any other class would be consensus-rejected when
//!   materialized).
//! - `msg.value` = the withdrawn wei. MUST be > 0 and an exact multiple of
//!   [`EVM_NATIVE_SCALE`], and fit `u64` sompi.
//! - On success: `msg.value` moves caller → F002 via the journal (revert-safe
//!   for nested frames), and a withdraw LOG is journaled on F002. Charged
//!   [`F002_WITHDRAW_GAS`].
//! - Any user-input fault ⇒ the call REVERTS (class 4 — the carrying tx gets a
//!   status-0 receipt; the block stays valid, §6.2).
//!
//! The executor (post-tx) scans each receipt's COMMITTED logs for the withdraw
//! topic: reverted frames lose their journal entries (transfer + log) at once,
//! so the surviving logs are exactly the effective withdrawals — no
//! side-channel state, no double counting. For each one it debits F002 (the
//! wei leaves the EVM lane) and emits a [`WithdrawOp`] that consensus
//! materializes as a synthetic UTXO output in the accepting block's diff.

use kaspa_consensus_core::evm::{EVM_NATIVE_SCALE, F002_WITHDRAW_GAS, MAX_WITHDRAW_SCRIPT_BYTES, MISAKA_WITHDRAW_PRECOMPILE};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_txscript::script_class::ScriptClass;
use revm::handler::register::EvmHandler;
use revm::interpreter::{CallOutcome, Gas, InstructionResult, InterpreterResult};
use revm::primitives::{Address, B256, Bytes, Log, LogData, U256};
use revm::{Database, FrameOrResult, FrameResult};
use std::sync::OnceLock;

/// The F002 address as a revm `Address`.
pub fn f002_address() -> Address {
    Address::from(MISAKA_WITHDRAW_PRECOMPILE.as_bytes())
}

/// The withdraw event topic: `keccak256("MisakaWithdraw(address,uint256,bytes)")`.
/// Frozen at activation (it is part of the committed receipts/logs).
pub fn withdraw_topic() -> B256 {
    static TOPIC: OnceLock<B256> = OnceLock::new();
    *TOPIC.get_or_init(|| revm::primitives::keccak256(b"MisakaWithdraw(address,uint256,bytes)"))
}

/// A withdraw recovered from one committed F002 log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedWithdraw {
    pub from: [u8; 20],
    pub amount_wei: u128,
    pub script_public_key: ScriptPublicKey,
}

/// Log data layout (frozen): `caller(20) ‖ amount_wei(32 BE) ‖ spk_version(2 BE) ‖ script(rest)`.
fn encode_withdraw_log_data(caller: Address, amount_wei: U256, spk_version: u16, script: &[u8]) -> Bytes {
    let mut data = Vec::with_capacity(20 + 32 + 2 + script.len());
    data.extend_from_slice(caller.as_slice());
    data.extend_from_slice(&amount_wei.to_be_bytes::<32>());
    data.extend_from_slice(&spk_version.to_be_bytes());
    data.extend_from_slice(script);
    Bytes::from(data)
}

/// Decode one committed F002 withdraw log (`None` = not a withdraw log /
/// malformed — impossible for logs our own intercept emitted).
pub fn decode_withdraw_log(log: &revm::primitives::Log) -> Option<DecodedWithdraw> {
    if log.address != f002_address() || log.data.topics() != [withdraw_topic()] {
        return None;
    }
    let data = log.data.data.as_ref();
    if data.len() < 20 + 32 + 2 {
        return None;
    }
    let mut from = [0u8; 20];
    from.copy_from_slice(&data[..20]);
    let amount = U256::from_be_slice(&data[20..52]);
    let amount_wei = u128::try_from(amount).ok()?;
    let version = u16::from_be_bytes([data[52], data[53]]);
    let script = data[54..].to_vec();
    let script_public_key = ScriptPublicKey::from_vec(version, script);
    // Audit F2 (defense in depth): re-assert the withdraw invariants even though the F002 intercept
    // already validate_withdraw'd before emitting — so a malformed log can NEVER feed the bridge if
    // a future executor/spec change regresses the emission gate. In-spec logs always pass, so this
    // never changes behavior on valid data (commitments are byte-identical).
    withdraw_invariants_hold(amount_wei, &script_public_key).ok()?;
    Some(DecodedWithdraw { from, amount_wei, script_public_key })
}

/// The withdraw payout invariants, shared by the emission-time gate
/// ([`validate_withdraw`]) and the decode-time re-check ([`decode_withdraw_log`]):
/// the amount is a positive exact sompi multiple that fits `u64`, and the
/// destination script is within the byte cap AND is the PQ-standard ML-DSA P2PKH
/// class (so the materialized synthetic UTXO is spendable). Audit F2 — making both
/// the producer and the verifier re-assert these means a malformed log can never
/// feed the bridge even if a future executor/spec change regresses the emission gate.
fn withdraw_invariants_hold(amount_wei: u128, spk: &ScriptPublicKey) -> Result<(), &'static str> {
    if amount_wei == 0 {
        return Err("withdraw amount is zero");
    }
    if amount_wei % EVM_NATIVE_SCALE as u128 != 0 {
        return Err("withdraw amount is not an exact sompi multiple");
    }
    if amount_wei / EVM_NATIVE_SCALE as u128 > u64::MAX as u128 {
        return Err("withdraw amount exceeds u64 sompi");
    }
    if spk.script().len() > MAX_WITHDRAW_SCRIPT_BYTES {
        return Err("destination script exceeds the byte cap");
    }
    if !ScriptClass::from_script(spk).is_pq_standard() {
        return Err("destination script is not the PQ-standard ML-DSA P2PKH class");
    }
    Ok(())
}

/// Validate the withdraw user inputs. `Err` = user fault ⇒ the call reverts.
fn validate_withdraw(input: &[u8], value: U256) -> Result<(u16, Vec<u8>), &'static str> {
    let Ok(wei) = u128::try_from(value) else {
        return Err("withdraw amount exceeds u128");
    };
    if input.len() < 2 {
        return Err("calldata shorter than the spk version prefix");
    }
    let version = u16::from_be_bytes([input[0], input[1]]);
    let script = &input[2..];
    let spk = ScriptPublicKey::from_vec(version, script.to_vec());
    withdraw_invariants_hold(wei, &spk)?;
    Ok((version, script.to_vec()))
}

/// Wrap `handler.execution.call` so calls targeting F002 run the withdraw
/// instead of loading (empty) code. Everything else delegates to the previous
/// handle. Registered on every block-executor `Evm` instance.
pub fn register_f002_withdraw<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>) {
    let prev = handler.execution.call.clone();
    handler.execution.call = std::sync::Arc::new(move |ctx, inputs| {
        let f002 = f002_address();
        if inputs.target_address != f002 || inputs.bytecode_address != f002 {
            return prev(ctx, inputs);
        }
        // Charge the fixed cost first; an under-gassed call fails outright.
        let mut gas = Gas::new(inputs.gas_limit);
        if !gas.record_cost(F002_WITHDRAW_GAS) {
            return Ok(FrameOrResult::Result(FrameResult::Call(CallOutcome::new(
                InterpreterResult { result: InstructionResult::PrecompileOOG, output: Bytes::new(), gas },
                inputs.return_memory_offset.clone(),
            ))));
        }
        let revert = |gas: Gas, memory: std::ops::Range<usize>| {
            Ok(FrameOrResult::Result(FrameResult::Call(CallOutcome::new(
                InterpreterResult { result: InstructionResult::Revert, output: Bytes::new(), gas },
                memory,
            ))))
        };
        // A static frame cannot withdraw (state change); delegate/callcode
        // never match (their target_address is the caller's own address).
        let Some(value) = inputs.value.transfer() else {
            return revert(gas, inputs.return_memory_offset.clone());
        };
        if inputs.is_static {
            return revert(gas, inputs.return_memory_offset.clone());
        }
        let (version, script) = match validate_withdraw(&inputs.input, value) {
            Ok(v) => v,
            Err(_) => return revert(gas, inputs.return_memory_offset.clone()),
        };
        // Journal the value move caller → F002 and the withdraw log. Both are
        // part of the CURRENT tx journal: if an outer frame reverts later they
        // unwind together, so committed logs == effective withdrawals.
        let inner = &mut ctx.evm.inner;
        match inner.journaled_state.transfer(&inputs.caller, &f002, value, &mut inner.db) {
            Ok(None) => {}
            // Insufficient caller balance (or another transfer fault) ⇒ revert.
            Ok(Some(_)) => return revert(gas, inputs.return_memory_offset.clone()),
            Err(e) => return Err(e),
        }
        inner.journaled_state.log(Log {
            address: f002,
            data: LogData::new_unchecked(vec![withdraw_topic()], encode_withdraw_log_data(inputs.caller, value, version, &script)),
        });
        Ok(FrameOrResult::Result(FrameResult::Call(CallOutcome::new(
            InterpreterResult { result: InstructionResult::Return, output: Bytes::new(), gas },
            inputs.return_memory_offset.clone(),
        ))))
    });
}
