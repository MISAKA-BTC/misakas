//! §11 `debug_traceTransaction` replay engine — Geth `callTracer` (design §11.1/§11.4).
//!
//! Re-executes one ACCEPTED EVM tx with a revm call-frame [`Inspector`] against the
//! EXACT pre-state the consensus executor left right before it, then reconciles the
//! replay's gas / status / logs against the committed [`EvmReceipt`]. A mismatch
//! returns no trace (`replay mismatch`, design §11.3 step 7).
//!
//! Parity is structural, not by re-implementation: the pre-target state is rebuilt
//! by running the pre-target acceptance candidates through [`execute_block_evm`]
//! **unchanged** — the consensus-critical executor is never forked. The pre-target
//! candidates are a prefix of the block's acceptance list, and BOTH gas-pool modes
//! reproduce a prefix identically (v1 strict prefix-take by construction; v2's
//! sequential pool is order-independent of later txs), so the rebuilt state is the
//! state the target actually executed against. The target itself then runs once
//! through `transact()` (no commit — a throwaway replay) with the inspector
//! attached; an inspector is observation-only, so its gas/logs/status equal the
//! committed execution's.

use crate::env::derive_env;
use crate::executor::{make_receipt, to_revm_address};
use crate::snapshot::seed_cachedb;
use crate::tx::decode_tx_to_env;
use crate::{execute_block_evm, AcceptedTxCandidate, EvmBlockInput, EvmExecError, EVM_SPEC_ID};
use kaspa_consensus_core::evm::{
    EvmCandidateOutcome, EvmExecutionHeader, EvmExecutionPayload, EvmReceipt, EvmStateSnapshot, EvmTraceReplayBodyV1,
};
use crate::sim::EthCallEnv;
use revm::interpreter::{CallInputs, CallOutcome, CallScheme, CreateInputs, CreateOutcome, InstructionResult};
use revm::primitives::{Address, Bytes, B256, U256};
use revm::{inspector_handle_register, Database, DatabaseRef, Evm, EvmContext, Inspector};

/// The kind of an EVM call frame, rendered as Geth's `callTracer` `type` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceCallKind {
    Call,
    CallCode,
    DelegateCall,
    StaticCall,
    Create,
    Create2,
}

impl TraceCallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TraceCallKind::Call => "CALL",
            TraceCallKind::CallCode => "CALLCODE",
            TraceCallKind::DelegateCall => "DELEGATECALL",
            TraceCallKind::StaticCall => "STATICCALL",
            TraceCallKind::Create => "CREATE",
            TraceCallKind::Create2 => "CREATE2",
        }
    }

    fn from_scheme(scheme: CallScheme) -> Self {
        match scheme {
            // SHANGHAI has no EOF EXT*CALL; map them defensively to their classic kind.
            CallScheme::Call | CallScheme::ExtCall => TraceCallKind::Call,
            CallScheme::CallCode => TraceCallKind::CallCode,
            CallScheme::DelegateCall | CallScheme::ExtDelegateCall => TraceCallKind::DelegateCall,
            CallScheme::StaticCall | CallScheme::ExtStaticCall => TraceCallKind::StaticCall,
        }
    }
}

/// One node of the `callTracer` call tree (design §11.4). Values are revm
/// primitives; the eth-rpc adapter renders them as `0x…` hex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallFrame {
    pub kind: TraceCallKind,
    pub from: Address,
    /// `None` for a CREATE until the address is known (`create_end`).
    pub to: Option<Address>,
    pub value: U256,
    /// Gas provided to the frame (the top-level frame is overwritten with the tx
    /// gas limit; sub-frames carry the gas forwarded by the caller).
    pub gas: u64,
    pub gas_used: u64,
    pub input: Bytes,
    pub output: Bytes,
    /// `Some` for a failed frame (revert / halt). Geth `error` string.
    pub error: Option<String>,
    /// Decoded `Error(string)` / `Panic(uint256)` revert reason, when present.
    pub revert_reason: Option<String>,
    pub calls: Vec<CallFrame>,
}

/// §11.5 resource caps for a single trace (the hard timeout + dedicated pool are
/// enforced by the RPC layer; these bound the in-engine work).
#[derive(Clone, Copy, Debug)]
pub struct TraceLimits {
    pub max_steps: u64,
    pub max_frames: usize,
    pub max_output_bytes: usize,
}

impl Default for TraceLimits {
    fn default() -> Self {
        // Design §11.5 defaults.
        Self { max_steps: 5_000_000, max_frames: 100_000, max_output_bytes: 32 * 1024 * 1024 }
    }
}

/// Failure modes of a trace replay.
#[derive(Debug)]
pub enum TraceError {
    /// The tx hash / receipt index does not resolve to an ACCEPTED candidate in
    /// this block's replay body (skipped/absent — §11.6 directs those to
    /// `misaka_traceEvmCandidate`).
    TargetNotAccepted,
    /// The pre-target replay or the target execution failed (store/exec error).
    Exec(EvmExecError),
    /// The replay's gas / status / logs disagree with the committed receipt
    /// (design §11.3 step 7) — no trace is returned.
    ReplayMismatch(String),
    /// A §11.5 resource cap was hit.
    ResourceExceeded(String),
    /// An internal precondition was violated (e.g. a malformed replay body).
    Internal(String),
}

impl std::fmt::Display for TraceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceError::TargetNotAccepted => write!(f, "transaction was not accepted (no receipt to trace)"),
            TraceError::Exec(e) => write!(f, "trace replay execution error: {e}"),
            TraceError::ReplayMismatch(m) => write!(f, "replay mismatch: {m}"),
            TraceError::ResourceExceeded(m) => write!(f, "trace resource limit exceeded: {m}"),
            TraceError::Internal(m) => write!(f, "trace internal error: {m}"),
        }
    }
}

/// A per-account state view for the `prestateTracer` diff (the balance/nonce/code
/// plus the storage slots relevant to the diff).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountStateView {
    pub balance: U256,
    pub nonce: u64,
    /// Empty when the account has no code.
    pub code: Bytes,
    /// `(slot, value)` for the slots included in the diff.
    pub storage: Vec<(U256, U256)>,
}

/// One account's `prestateTracer` (diffMode) entry. `pre = None` ⇒ the account did
/// not exist before (created by the tx); `post = None` ⇒ it was self-destructed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrestateAccount {
    pub address: Address,
    pub pre: Option<AccountStateView>,
    pub post: Option<AccountStateView>,
}

/// One opcode step of the Geth default (struct/opcode) logger. Memory and storage
/// are intentionally NOT captured (design §11.5 — off by default; the heavy fields).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructLog {
    pub pc: u64,
    pub op: u8,
    pub op_name: &'static str,
    /// Gas remaining BEFORE the op executed.
    pub gas: u64,
    /// Gas the op consumed (`gas_before − gas_after`).
    pub gas_cost: u64,
    /// 1-based call depth.
    pub depth: u32,
    /// Stack (bottom→top) as big-endian 32-byte words.
    pub stack: Vec<[u8; 32]>,
    /// Set when this op produced a non-OK result (revert / halt).
    pub error: Option<String>,
}

/// The result of a successful trace. A single replay yields the callTracer call
/// tree, the prestateTracer diff, and (when requested) the opcode struct logs, so
/// the RPC layer can serve any tracer from one execution.
#[derive(Clone, Debug)]
pub struct TracedTx {
    pub frame: CallFrame,
    pub gas_used: u64,
    pub succeeded: bool,
    pub output: Bytes,
    /// `prestateTracer` diffMode: every account the tx changed, pre vs post.
    pub prestate: Vec<PrestateAccount>,
    /// Geth default struct/opcode logs — `Some` only when capture was requested
    /// (it is expensive: one entry per executed opcode).
    pub struct_logs: Option<Vec<StructLog>>,
}

/// Resolve the acceptance-candidate index whose recorded outcome is
/// `Accepted{receipt_index}`. `None` ⇒ no accepted candidate has that index.
pub fn candidate_index_for_receipt(body: &EvmTraceReplayBodyV1, receipt_index: u32) -> Option<usize> {
    body.txs.iter().position(|t| matches!(t.outcome, EvmCandidateOutcome::Accepted { receipt_index: ri } if ri == receipt_index))
}

/// Trace the accepted tx at `receipt_index` of the accepting block described by
/// `body`, against `parent_snapshot` (the selected parent's committed post-state)
/// and `parent_header` (its committed EVM header, `None` for the first EVM block).
/// `expected_receipt` is the committed receipt the replay is reconciled against.
/// The three activation scores MUST be the network params' values so the replay
/// runs the exact gas-pool / fence regime the block did.
#[allow(clippy::too_many_arguments)]
pub fn trace_accepted_tx(
    parent_snapshot: &EvmStateSnapshot,
    parent_header: Option<&EvmExecutionHeader>,
    body: &EvmTraceReplayBodyV1,
    receipt_index: u32,
    expected_receipt: &EvmReceipt,
    gas_pool_v2_activation_daa_score: u64,
    f002_withdraw_cap_activation_daa_score: u64,
    f003_mldsa_verify_activation_daa_score: u64,
    capture_struct_logs: bool,
    limits: TraceLimits,
) -> Result<TracedTx, TraceError> {
    let target_idx = candidate_index_for_receipt(body, receipt_index).ok_or(TraceError::TargetNotAccepted)?;

    // Reconstruct the executor inputs from the replay body. `transactions` is
    // data-only (unused by the executor) so it is left empty; `evm_coinbase` is
    // the accepting block's declared coinbase (COINBASE opcode + deposit tips).
    let payload = EvmExecutionPayload {
        system_ops: body.system_ops.clone(),
        transactions: Vec::new(),
        evm_coinbase: body.env.coinbase,
        extra_data: Vec::new(),
    };
    let all_candidates: Vec<AcceptedTxCandidate> =
        body.txs.iter().map(|t| AcceptedTxCandidate { raw: t.raw.clone(), payload_coinbase: t.payload_coinbase }).collect();
    let selected_parent_hash = body.selected_parent.as_bytes();

    // 1. Rebuild the EXACT pre-target state by running the pre-target candidate
    //    PREFIX through the consensus executor unchanged (system ops + accepted
    //    prefix txs, with all their tip-reroute / F002-burn accounting).
    let prefix = &all_candidates[..target_idx];
    let input_prefix = EvmBlockInput {
        parent: parent_header,
        header_timestamp_ms: body.env.header_timestamp_ms,
        selected_parent_hash,
        blue_work_be: body.env.blue_work_be.clone(),
        daa_score: body.env.daa_score,
        payload: &payload,
        accepted_txs: prefix,
        gas_pool_v2_activation_daa_score,
        f002_withdraw_cap_activation_daa_score,
        f003_mldsa_verify_activation_daa_score,
        // §12 Phase-7: the typed-receipt-root fence affects ONLY the committed
        // receipts_root, which the trace neither emits nor reconciles (the DiD check
        // above compares candidate OUTCOMES only). So replay with the v1 root (inert)
        // — the call tree + pre/post state are identical either way.
        typed_receipt_root_activation_daa_score: u64::MAX,
    };
    let seed = seed_cachedb(parent_snapshot).map_err(TraceError::Exec)?;
    let (prefix_result, mut pre_state) = execute_block_evm(seed, &input_prefix).map_err(TraceError::Exec)?;

    // Defense-in-depth (fail closed): the re-derived prefix MUST reproduce the
    // recorded per-candidate outcomes exactly. If it does not — a wrong activation
    // fence regime, an executor change, or store skew — the rebuilt pre-state does
    // NOT match the state the target actually executed against, so refuse to trace
    // rather than risk a call tree computed against the wrong world. (The prefix is
    // [0..target_idx]; the executor's decisions for those candidates depend only on
    // earlier candidates, so truncation does not change them.)
    for (i, replayed) in prefix_result.candidate_outcomes.iter().enumerate() {
        if *replayed != body.txs[i].outcome {
            return Err(TraceError::ReplayMismatch(format!(
                "prefix candidate {i} replayed outcome {:?} != recorded {:?} (activation-fence or executor skew)",
                replayed, body.txs[i].outcome
            )));
        }
    }

    // 2. Derive the env exactly as production (EIP-1559 base fee from the parent
    //    header, keyed-BLAKE2b prevrandao). The F003 fence is selected by daa_score.
    let f003_active = body.env.daa_score >= f003_mldsa_verify_activation_daa_score;
    let derived = derive_env(
        parent_header,
        body.env.header_timestamp_ms,
        &selected_parent_hash,
        &body.env.blue_work_be,
        body.env.daa_score,
        to_revm_address(&body.env.coinbase),
    );

    // 3. Decode the target tx and run it once with the call-frame inspector.
    let txenv = decode_tx_to_env(&body.txs[target_idx].raw).map_err(|e| TraceError::Exec(EvmExecError::TxDecode(e)))?;
    let target_gas_limit = txenv.gas_limit;
    let basefee = derived.base_fee_per_gas;
    let mut tracer = CallTracer::new(limits, capture_struct_logs);
    let result = {
        let mut evm = Evm::builder()
            .with_db(&mut pre_state)
            .with_external_context(&mut tracer)
            .with_spec_id(EVM_SPEC_ID)
            .modify_cfg_env(|c| c.chain_id = derived.chain_id)
            .modify_block_env(|b| {
                b.number = U256::from(derived.evm_number);
                b.timestamp = U256::from(derived.evm_timestamp_sec);
                b.coinbase = derived.coinbase;
                b.gas_limit = U256::from(derived.gas_limit);
                b.basefee = U256::from(basefee);
                b.difficulty = U256::ZERO;
                b.prevrandao = Some(derived.prev_randao);
            })
            // Same precompile seam as the executor + the eth_call simulator (parity),
            // composed with the inspector handler (inspector_handle_register does not
            // override handlers — observation only).
            .append_handler_register_box(Box::new(move |h| crate::precompiles::register_all_misaka_precompiles(h, f003_active)))
            .append_handler_register(inspector_handle_register)
            .build();
        evm.context.evm.env.tx = txenv;
        evm.transact().map_err(|e| TraceError::Exec(EvmExecError::InvalidTx(format!("{e:?}"))))?
    };

    // 4. Resource caps (§11.5).
    if let Some(reason) = tracer.exceeded.take() {
        return Err(TraceError::ResourceExceeded(reason));
    }

    // 5. Reconcile the replay against the committed receipt: status, per-tx gas,
    //    and the full log list must match, else the trace is suppressed (§11.3).
    let replay_receipt = make_receipt(&result.result, expected_receipt.cumulative_gas_used);
    if replay_receipt.succeeded != expected_receipt.succeeded {
        return Err(TraceError::ReplayMismatch(format!(
            "status {} != committed {}",
            replay_receipt.succeeded, expected_receipt.succeeded
        )));
    }
    if replay_receipt.gas_used != expected_receipt.gas_used {
        return Err(TraceError::ReplayMismatch(format!(
            "gasUsed {} != committed {}",
            replay_receipt.gas_used, expected_receipt.gas_used
        )));
    }
    if replay_receipt.logs != expected_receipt.logs {
        return Err(TraceError::ReplayMismatch(format!(
            "logs differ ({} replayed vs {} committed)",
            replay_receipt.logs.len(),
            expected_receipt.logs.len()
        )));
    }

    // 6. Finalize the root frame. Geth reports the TX-level gas at the root (the
    //    inspector only sees the post-intrinsic call gas), so override it.
    let mut frame = tracer.root.take().ok_or_else(|| TraceError::Internal("inspector produced no root frame".into()))?;
    frame.gas = target_gas_limit;
    frame.gas_used = result.result.gas_used();

    let output = result.result.output().cloned().unwrap_or_default();
    if output.len() > tracer_output_cap(&limits) {
        return Err(TraceError::ResourceExceeded(format!("output {} bytes exceeds cap", output.len())));
    }

    // prestateTracer (diffMode): every account the tx changed, pre vs post, computed
    // from the pre-target state (pre) and revm's post-execution state set (post). The
    // same single replay above feeds it — no extra execution.
    let prestate = compute_prestate(&pre_state, &result.state);
    let struct_logs = if capture_struct_logs { Some(std::mem::take(&mut tracer.struct_logs)) } else { None };

    Ok(TracedTx { frame, gas_used: result.result.gas_used(), succeeded: result.result.is_success(), output, prestate, struct_logs })
}

/// Compute the `prestateTracer` diffMode entries from the replay's pre-target state
/// and revm's post-execution `EvmState` (the touched-account set). An account is
/// included iff its balance/nonce/code changed, it gained/lost storage values, it
/// was created, or it self-destructed. Storage entries are the slots whose value
/// actually changed (`original_value != present_value`).
fn compute_prestate(pre_state: &revm::db::CacheDB<revm::db::EmptyDB>, post: &revm::primitives::EvmState) -> Vec<PrestateAccount> {
    let mut out = Vec::new();
    for (addr, acc) in post.iter() {
        let changed_slots: Vec<(U256, U256, U256)> = acc
            .storage
            .iter()
            .filter(|(_, s)| s.original_value != s.present_value)
            .map(|(k, s)| (*k, s.original_value, s.present_value))
            .collect();
        let pre_info = pre_state.basic_ref(*addr).ok().flatten();
        let created = acc.is_created() || pre_info.is_none();
        let selfdestructed = acc.is_selfdestructed();
        let info_changed = match &pre_info {
            Some(p) => p.balance != acc.info.balance || p.nonce != acc.info.nonce || p.code_hash != acc.info.code_hash,
            None => !acc.info.is_empty(),
        };
        if !info_changed && changed_slots.is_empty() && !created && !selfdestructed {
            continue; // touched but unchanged — omit from a diff
        }
        let pre = if created {
            None
        } else {
            let p = pre_info.as_ref();
            Some(AccountStateView {
                balance: p.map(|i| i.balance).unwrap_or_default(),
                nonce: p.map(|i| i.nonce).unwrap_or_default(),
                code: p.and_then(|i| i.code.clone()).map(|c| c.original_bytes()).unwrap_or_default(),
                storage: changed_slots.iter().map(|(k, orig, _)| (*k, *orig)).collect(),
            })
        };
        let post_view = if selfdestructed {
            None
        } else {
            Some(AccountStateView {
                balance: acc.info.balance,
                nonce: acc.info.nonce,
                code: acc.info.code.clone().map(|c| c.original_bytes()).unwrap_or_default(),
                storage: changed_slots.iter().map(|(k, _, pres)| (*k, *pres)).collect(),
            })
        };
        out.push(PrestateAccount { address: *addr, pre, post: post_view });
    }
    // Deterministic order (HashMap iteration is unordered) for stable output.
    out.sort_by(|a, b| a.address.cmp(&b.address));
    out
}

fn tracer_output_cap(limits: &TraceLimits) -> usize {
    limits.max_output_bytes
}

/// The diagnosis of a candidate (typically skipped / not-yet-accepted) tx traced
/// against a given head state (§11.6 `misaka_traceEvmCandidate`).
#[derive(Clone, Debug)]
pub struct CandidateTrace {
    /// `false` ⇒ the tx failed pre-execution validation (nonce / funds / gas /
    /// basefee) and never entered the EVM — the §6.1 class-2 family. `reason` holds
    /// the validation error and `frame` is `None`.
    pub executed: bool,
    /// Meaningful only when `executed`: did the top-level call succeed (vs revert/halt).
    pub succeeded: bool,
    pub gas_used: u64,
    pub output: Bytes,
    /// The pre-validation error (not executed) or the decoded revert reason (executed
    /// + reverted) — the human-readable "why this candidate did not yield a receipt".
    pub reason: Option<String>,
    /// The call tree, when the tx executed.
    pub frame: Option<CallFrame>,
}

/// Trace a RAW signed EIP-2718 tx against `snapshot` (a head/committed state) with
/// the call-frame inspector — the §11.6 candidate diagnosis for a tx that has no
/// receipt (skipped class 2/3/5, or still pending). Fee-free env (basefee 0, like
/// `eth_call`), but the tx's OWN nonce/gas/value are kept, so a class-2 nonce/funds
/// failure surfaces as a pre-validation error (`executed = false`). A tx that runs
/// returns its call tree + status; a successful run means the tx is currently
/// executable (it was likely skipped for block packing — class 5 — or is pending).
pub fn trace_candidate_tx(
    snapshot: &EvmStateSnapshot,
    env: &EthCallEnv,
    raw: &[u8],
    limits: TraceLimits,
) -> Result<CandidateTrace, TraceError> {
    let mut db = seed_cachedb(snapshot).map_err(TraceError::Exec)?;
    let txenv = decode_tx_to_env(raw).map_err(|e| TraceError::Exec(EvmExecError::TxDecode(e)))?;
    let target_gas_limit = txenv.gas_limit;
    let block_gas = if env.gas_limit == 0 { 30_000_000 } else { env.gas_limit };
    let f003_active = env.f003_active;
    let mut tracer = CallTracer::new(limits, false);
    let exec = {
        let mut evm = Evm::builder()
            .with_db(&mut db)
            .with_external_context(&mut tracer)
            .with_spec_id(EVM_SPEC_ID)
            .modify_cfg_env(|c| c.chain_id = env.chain_id)
            .modify_block_env(|b| {
                b.number = U256::from(env.number);
                b.timestamp = U256::from(env.timestamp);
                b.coinbase = to_revm_address(&env.coinbase);
                b.gas_limit = U256::from(block_gas);
                // Candidate diagnosis is fee-free (matches the eth_call simulator), so
                // a real signed gas price is admissible against a zero basefee.
                b.basefee = U256::ZERO;
                b.difficulty = U256::ZERO;
                b.prevrandao = Some(B256::ZERO);
            })
            .append_handler_register_box(Box::new(move |h| crate::precompiles::register_all_misaka_precompiles(h, f003_active)))
            .append_handler_register(inspector_handle_register)
            .build();
        evm.context.evm.env.tx = txenv;
        evm.transact()
    };
    if let Some(reason) = tracer.exceeded.take() {
        return Err(TraceError::ResourceExceeded(reason));
    }
    match exec {
        Ok(result) => {
            let mut frame = tracer.root.take().ok_or_else(|| TraceError::Internal("inspector produced no root frame".into()))?;
            frame.gas = target_gas_limit;
            frame.gas_used = result.result.gas_used();
            let output = result.result.output().cloned().unwrap_or_default();
            let reason = if result.result.is_success() { None } else { decode_revert_reason(&output) };
            Ok(CandidateTrace {
                executed: true,
                succeeded: result.result.is_success(),
                gas_used: result.result.gas_used(),
                output,
                reason,
                frame: Some(frame),
            })
        }
        // Pre-execution validation failure (nonce / funds / gas / basefee) — the
        // class-2 family. Report it as the skip reason; there is no call tree.
        Err(e) => Ok(CandidateTrace {
            executed: false,
            succeeded: false,
            gas_used: 0,
            output: Bytes::new(),
            reason: Some(format!("{e:?}")),
            frame: None,
        }),
    }
}

// ---------------------------------------------------------------------------
// The call-frame inspector.
// ---------------------------------------------------------------------------

struct CallTracer {
    root: Option<CallFrame>,
    stack: Vec<CallFrame>,
    steps: u64,
    frame_count: usize,
    limits: TraceLimits,
    /// Set when a cap is breached; checked after `transact()` (gas already bounds
    /// total work, so there is no need to abort mid-execution).
    exceeded: Option<String>,
    /// When true, the `step`/`step_end` hooks also accumulate `struct_logs` (the
    /// Geth default opcode logger). Off for callTracer/prestate to avoid the cost.
    capture_struct_logs: bool,
    struct_logs: Vec<StructLog>,
}

impl CallTracer {
    fn new(limits: TraceLimits, capture_struct_logs: bool) -> Self {
        Self {
            root: None,
            stack: Vec::new(),
            steps: 0,
            frame_count: 0,
            limits,
            exceeded: None,
            capture_struct_logs,
            struct_logs: Vec::new(),
        }
    }

    fn push_frame(&mut self, frame: CallFrame) {
        self.frame_count += 1;
        if self.frame_count > self.limits.max_frames && self.exceeded.is_none() {
            self.exceeded = Some(format!("max call frames {} exceeded", self.limits.max_frames));
        }
        self.stack.push(frame);
    }

    fn finish_frame(&mut self, gas_used: u64, output: Bytes, ir: InstructionResult) {
        if let Some(mut frame) = self.stack.pop() {
            frame.gas_used = gas_used;
            frame.output = output.clone();
            if !ir.is_ok() {
                frame.error = Some(instruction_error_string(ir));
                if ir.is_revert() {
                    frame.revert_reason = decode_revert_reason(&output);
                }
            }
            match self.stack.last_mut() {
                Some(parent) => parent.calls.push(frame),
                None => self.root = Some(frame),
            }
        }
    }
}

impl<DB: Database> Inspector<DB> for CallTracer {
    fn step(&mut self, interp: &mut revm::interpreter::Interpreter, _context: &mut EvmContext<DB>) {
        self.steps += 1;
        if self.steps > self.limits.max_steps && self.exceeded.is_none() {
            self.exceeded = Some(format!("max steps {} exceeded", self.limits.max_steps));
        }
        if self.capture_struct_logs && self.exceeded.is_none() {
            let op = interp.current_opcode();
            let depth = self.stack.len() as u32; // open call frames = current call depth
            let pc = interp.program_counter() as u64;
            let gas = interp.gas.remaining();
            let stack: Vec<[u8; 32]> = interp.stack.data().iter().map(|u| u.to_be_bytes::<32>()).collect();
            self.struct_logs.push(StructLog {
                pc,
                op,
                op_name: revm::interpreter::OpCode::new(op).map(|o| o.as_str()).unwrap_or("INVALID"),
                gas,
                gas_cost: 0, // patched in step_end (gas_before − gas_after)
                depth,
                stack,
                error: None,
            });
        }
    }

    fn step_end(&mut self, interp: &mut revm::interpreter::Interpreter, _context: &mut EvmContext<DB>) {
        if self.capture_struct_logs {
            let gas_after = interp.gas.remaining();
            let ir = interp.instruction_result;
            if let Some(last) = self.struct_logs.last_mut() {
                last.gas_cost = last.gas.saturating_sub(gas_after);
                if !ir.is_ok() {
                    last.error = Some(instruction_error_string(ir));
                }
            }
        }
    }

    fn call(&mut self, _context: &mut EvmContext<DB>, inputs: &mut CallInputs) -> Option<CallOutcome> {
        self.push_frame(CallFrame {
            kind: TraceCallKind::from_scheme(inputs.scheme),
            from: inputs.caller,
            // Geth callTracer reports `to` as the CODE address. For DELEGATECALL /
            // CALLCODE that is `bytecode_address` (the implementation/library), not
            // `target_address` (the storage/proxy context); for CALL / STATICCALL the
            // two are equal, so this is unconditionally correct.
            to: Some(inputs.bytecode_address),
            value: inputs.value.get(),
            gas: inputs.gas_limit,
            gas_used: 0,
            input: inputs.input.clone(),
            output: Bytes::new(),
            error: None,
            revert_reason: None,
            calls: Vec::new(),
        });
        None
    }

    fn call_end(&mut self, _context: &mut EvmContext<DB>, _inputs: &CallInputs, outcome: CallOutcome) -> CallOutcome {
        self.finish_frame(outcome.result.gas.spent(), outcome.result.output.clone(), outcome.result.result);
        outcome
    }

    fn create(&mut self, _context: &mut EvmContext<DB>, inputs: &mut CreateInputs) -> Option<CreateOutcome> {
        let kind = match inputs.scheme {
            revm::primitives::CreateScheme::Create => TraceCallKind::Create,
            revm::primitives::CreateScheme::Create2 { .. } => TraceCallKind::Create2,
        };
        self.push_frame(CallFrame {
            kind,
            from: inputs.caller,
            to: None,
            value: inputs.value,
            gas: inputs.gas_limit,
            gas_used: 0,
            input: inputs.init_code.clone(),
            output: Bytes::new(),
            error: None,
            revert_reason: None,
            calls: Vec::new(),
        });
        None
    }

    fn create_end(&mut self, _context: &mut EvmContext<DB>, _inputs: &CreateInputs, outcome: CreateOutcome) -> CreateOutcome {
        if let Some(top) = self.stack.last_mut() {
            top.to = outcome.address;
        }
        self.finish_frame(outcome.result.gas.spent(), outcome.result.output.clone(), outcome.result.result);
        outcome
    }
}

/// A short Geth-style error string for a non-OK instruction result.
fn instruction_error_string(ir: InstructionResult) -> String {
    use InstructionResult::*;
    match ir {
        Revert => "execution reverted".to_string(),
        OutOfGas | MemoryOOG | MemoryLimitOOG | PrecompileOOG | InvalidOperandOOG => "out of gas".to_string(),
        OpcodeNotFound | InvalidFEOpcode => "invalid opcode".to_string(),
        StackOverflow => "stack overflow".to_string(),
        StackUnderflow => "stack underflow".to_string(),
        InvalidJump => "invalid jump destination".to_string(),
        CallTooDeep => "max call depth exceeded".to_string(),
        OutOfFunds => "insufficient balance for transfer".to_string(),
        CreateCollision => "contract address collision".to_string(),
        CreateContractSizeLimit | CreateContractStartingWithEF => "max code size exceeded".to_string(),
        StateChangeDuringStaticCall => "write protection".to_string(),
        other => format!("{other:?}"),
    }
}

/// Decode a Solidity revert payload: `Error(string)` (selector `0x08c379a0`) or
/// `Panic(uint256)` (selector `0x4e487b71`). Returns `None` for a bare revert.
fn decode_revert_reason(output: &[u8]) -> Option<String> {
    const ERROR_STRING: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];
    const PANIC_UINT: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71];
    if output.len() < 4 {
        return None;
    }
    let selector = &output[0..4];
    if selector == ERROR_STRING {
        // [4 selector][32 offset][32 len][len bytes]
        if output.len() < 4 + 32 + 32 {
            return None;
        }
        let len_word = &output[36..68];
        // Length fits a usize on any realistic payload; reject absurd lengths.
        let len = u64::from_be_bytes(len_word[24..32].try_into().ok()?) as usize;
        if len_word[..24].iter().any(|&b| b != 0) {
            return None;
        }
        let start: usize = 68;
        let end = start.checked_add(len)?;
        if end > output.len() {
            return None;
        }
        Some(String::from_utf8_lossy(&output[start..end]).into_owned())
    } else if selector == PANIC_UINT {
        if output.len() < 4 + 32 {
            return None;
        }
        let word = &output[4..4 + 32];
        // Reject a non-standard panic code (any high byte set) rather than silently
        // truncating to the low byte — parity with the Error(string) validation above.
        if word[..31].iter().any(|&b| b != 0) {
            return None;
        }
        Some(format!("panic: 0x{:02x}", word[31]))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_reason_decodes_error_string() {
        // abi.encodeWithSignature("Error(string)", "boom")
        let mut out = vec![0x08, 0xc3, 0x79, 0xa0];
        out.extend_from_slice(&{
            let mut w = [0u8; 32];
            w[31] = 0x20;
            w
        }); // offset = 32
        out.extend_from_slice(&{
            let mut w = [0u8; 32];
            w[31] = 4;
            w
        }); // len = 4
        let mut data = [0u8; 32];
        data[..4].copy_from_slice(b"boom");
        out.extend_from_slice(&data);
        assert_eq!(decode_revert_reason(&out).as_deref(), Some("boom"));
    }

    #[test]
    fn revert_reason_decodes_panic() {
        let mut out = vec![0x4e, 0x48, 0x7b, 0x71];
        let mut w = [0u8; 32];
        w[31] = 0x11; // arithmetic overflow panic code
        out.extend_from_slice(&w);
        assert_eq!(decode_revert_reason(&out).as_deref(), Some("panic: 0x11"));
    }

    #[test]
    fn revert_reason_none_for_bare_revert() {
        assert_eq!(decode_revert_reason(&[]), None);
        assert_eq!(decode_revert_reason(&[0xde, 0xad, 0xbe, 0xef]), None);
    }

    #[test]
    fn call_kind_strings() {
        assert_eq!(TraceCallKind::Call.as_str(), "CALL");
        assert_eq!(TraceCallKind::DelegateCall.as_str(), "DELEGATECALL");
        assert_eq!(TraceCallKind::Create2.as_str(), "CREATE2");
    }

    // -- end-to-end replay tests ------------------------------------------------

    use crate::snapshot::{seed_cachedb, snapshot_from_cachedb};
    use crate::tx::tx_hash;
    use crate::{execute_block_evm, AcceptedTxCandidate, EvmBlockInput};
    use kaspa_consensus_core::evm::{EvmAddress, EvmExecutionPayload, EvmReplayEnv, EvmReplayTx, EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE};
    use kaspa_hashes::Hash64;
    use revm::db::{CacheDB, EmptyDB};
    use revm::primitives::{keccak256, AccountInfo, Bytecode, B256, KECCAK_EMPTY};

    const COINBASE: [u8; 20] = [0xC0; 20];
    const SELECTED_PARENT: [u8; 64] = [0xAA; 64];

    /// Sign a 1559 tx with the fixed default key → (sender, raw EIP-2718 bytes).
    fn signed_tx(nonce: u64, to: revm::primitives::TxKind, value: u128, gas_limit: u64, input: Bytes) -> (Address, Vec<u8>) {
        signed_tx_key(0x42, nonce, to, value, gas_limit, input)
    }

    /// Sign a 1559 tx with an explicit key byte (distinct keys ⇒ distinct senders).
    fn signed_tx_key(key: u8, nonce: u64, to: revm::primitives::TxKind, value: u128, gas_limit: u64, input: Bytes) -> (Address, Vec<u8>) {
        use alloy_consensus::{SignableTransaction, TxEip1559, TxEnvelope};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        let signer = PrivateKeySigner::from_bytes(&B256::from([key; 32])).unwrap();
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID,
            nonce,
            gas_limit,
            max_fee_per_gas: EVM_INITIAL_BASE_FEE as u128 * 4,
            max_priority_fee_per_gas: 0,
            to,
            value: U256::from(value),
            access_list: Default::default(),
            input,
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        (signer.address(), TxEnvelope::from(tx.into_signed(sig)).encoded_2718())
    }

    /// Build the committed execution + a matching replay body for a single-tx block.
    fn run_and_body(parent_snapshot: &EvmStateSnapshot, raw: Vec<u8>) -> (EvmReceipt, EvmTraceReplayBodyV1) {
        let candidates = vec![AcceptedTxCandidate { raw: raw.clone(), payload_coinbase: EvmAddress::from_bytes(COINBASE) }];
        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes(COINBASE), ..Default::default() };
        let input = EvmBlockInput {
            parent: None,
            header_timestamp_ms: 1_000,
            selected_parent_hash: SELECTED_PARENT,
            blue_work_be: Vec::new(),
            daa_score: 0,
            payload: &payload,
            accepted_txs: &candidates,
            gas_pool_v2_activation_daa_score: u64::MAX,
            f002_withdraw_cap_activation_daa_score: u64::MAX,
            f003_mldsa_verify_activation_daa_score: u64::MAX,
            typed_receipt_root_activation_daa_score: u64::MAX,
        };
        let (result, _state) = execute_block_evm(seed_cachedb(parent_snapshot).unwrap(), &input).unwrap();
        assert_eq!(result.candidate_outcomes[0], EvmCandidateOutcome::Accepted { receipt_index: 0 }, "the tx must be accepted");
        let receipt = result.receipts[0].clone();
        let body = EvmTraceReplayBodyV1 {
            selected_parent: Hash64::from_bytes(SELECTED_PARENT),
            env: EvmReplayEnv {
                header_timestamp_ms: 1_000,
                blue_work_be: Vec::new(),
                daa_score: 0,
                coinbase: EvmAddress::from_bytes(COINBASE),
            },
            system_ops: Vec::new(),
            txs: vec![EvmReplayTx {
                tx_hash: tx_hash(&raw),
                raw,
                payload_coinbase: EvmAddress::from_bytes(COINBASE),
                originating_payload_block: Hash64::from_bytes([0x55; 64]),
                outcome: EvmCandidateOutcome::Accepted { receipt_index: 0 },
            }],
        };
        (receipt, body)
    }

    fn fund(addr: Address) -> EvmStateSnapshot {
        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            addr,
            AccountInfo { balance: U256::from(10u128).pow(U256::from(22)), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
        );
        snapshot_from_cachedb(&db)
    }

    /// Value transfer: a single CALL frame, reconciliation passes, and the trace
    /// round-trips the from/to/value/gas of the committed tx.
    #[test]
    fn trace_value_transfer_reconciles() {
        let recipient = Address::from([0x11u8; 20]);
        let (sender, raw) = signed_tx(0, revm::primitives::TxKind::Call(recipient), 1_000, 21_000, Bytes::new());
        let snap = fund(sender);
        let (receipt, body) = run_and_body(&snap, raw);

        let traced =
            trace_accepted_tx(&snap, None, &body, 0, &receipt, u64::MAX, u64::MAX, u64::MAX, false, TraceLimits::default()).unwrap();
        assert_eq!(traced.frame.kind, TraceCallKind::Call);
        assert_eq!(traced.frame.from, sender);
        assert_eq!(traced.frame.to, Some(recipient));
        assert_eq!(traced.frame.value, U256::from(1_000));
        assert_eq!(traced.frame.gas, 21_000, "root frame gas = tx gas limit");
        assert!(traced.succeeded);
        assert_eq!(traced.gas_used, receipt.gas_used);
        assert_eq!(traced.gas_used, 21_000);
        assert!(traced.frame.calls.is_empty(), "a plain transfer has no sub-calls");
        assert!(traced.frame.error.is_none());
    }

    /// A tampered committed receipt is rejected as a replay mismatch (design §11.3
    /// step 7) — the trace is suppressed rather than returning a wrong result.
    #[test]
    fn trace_rejects_receipt_mismatch() {
        let recipient = Address::from([0x22u8; 20]);
        let (sender, raw) = signed_tx(0, revm::primitives::TxKind::Call(recipient), 5, 21_000, Bytes::new());
        let snap = fund(sender);
        let (mut receipt, body) = run_and_body(&snap, raw);
        receipt.gas_used += 1; // diverge from the replay
        let err = trace_accepted_tx(&snap, None, &body, 0, &receipt, u64::MAX, u64::MAX, u64::MAX, false, TraceLimits::default());
        assert!(matches!(err, Err(TraceError::ReplayMismatch(_))), "got {err:?}");
    }

    /// A receipt index with no accepted candidate ⇒ `TargetNotAccepted` (§11.6).
    #[test]
    fn trace_unknown_receipt_index_is_not_accepted() {
        let recipient = Address::from([0x33u8; 20]);
        let (sender, raw) = signed_tx(0, revm::primitives::TxKind::Call(recipient), 5, 21_000, Bytes::new());
        let snap = fund(sender);
        let (receipt, body) = run_and_body(&snap, raw);
        let err = trace_accepted_tx(&snap, None, &body, 7, &receipt, u64::MAX, u64::MAX, u64::MAX, false, TraceLimits::default());
        assert!(matches!(err, Err(TraceError::TargetNotAccepted)), "got {err:?}");
    }

    /// Calling a contract that reverts captures the CALL frame's error + input, and
    /// reconciliation still passes (the committed receipt is also a failure).
    #[test]
    fn trace_contract_revert_captures_error() {
        // Runtime code `PUSH1 0 PUSH1 0 REVERT` = bare revert (no reason string).
        let code = Bytes::from(vec![0x60, 0x00, 0x60, 0x00, 0xfd]);
        let contract = Address::from([0xDEu8; 20]);
        let (sender, raw) = signed_tx(0, revm::primitives::TxKind::Call(contract), 0, 100_000, Bytes::from(vec![0x01, 0x02, 0x03]));

        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            sender,
            AccountInfo { balance: U256::from(10u128).pow(U256::from(22)), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
        );
        db.insert_account_info(
            contract,
            AccountInfo { balance: U256::ZERO, nonce: 1, code_hash: keccak256(&code), code: Some(Bytecode::new_raw(code.clone())) },
        );
        let snap = snapshot_from_cachedb(&db);
        let (receipt, body) = run_and_body(&snap, raw);
        assert!(!receipt.succeeded, "the call reverted");

        let traced =
            trace_accepted_tx(&snap, None, &body, 0, &receipt, u64::MAX, u64::MAX, u64::MAX, false, TraceLimits::default()).unwrap();
        assert_eq!(traced.frame.kind, TraceCallKind::Call);
        assert_eq!(traced.frame.to, Some(contract));
        assert_eq!(traced.frame.input, Bytes::from(vec![0x01, 0x02, 0x03]));
        assert!(!traced.succeeded);
        assert_eq!(traced.frame.error.as_deref(), Some("execution reverted"));
        assert!(traced.frame.revert_reason.is_none(), "bare revert has no reason string");
    }

    /// A tiny step budget trips the §11.5 resource cap. A plain value transfer
    /// executes ZERO opcodes (the `step` hook never fires), so the cap is exercised
    /// against a contract call (PUSH/PUSH/REVERT = real interpreter steps).
    #[test]
    fn trace_respects_step_cap() {
        let code = Bytes::from(vec![0x60, 0x00, 0x60, 0x00, 0xfd]); // PUSH1 0 PUSH1 0 REVERT
        let contract = Address::from([0xEEu8; 20]);
        let (sender, raw) = signed_tx(0, revm::primitives::TxKind::Call(contract), 0, 100_000, Bytes::new());
        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            sender,
            AccountInfo { balance: U256::from(10u128).pow(U256::from(22)), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
        );
        db.insert_account_info(
            contract,
            AccountInfo { balance: U256::ZERO, nonce: 1, code_hash: keccak256(&code), code: Some(Bytecode::new_raw(code)) },
        );
        let snap = snapshot_from_cachedb(&db);
        let (receipt, body) = run_and_body(&snap, raw);
        let limits = TraceLimits { max_steps: 1, max_frames: 100, max_output_bytes: 1024 };
        let err = trace_accepted_tx(&snap, None, &body, 0, &receipt, u64::MAX, u64::MAX, u64::MAX, false, limits);
        assert!(matches!(err, Err(TraceError::ResourceExceeded(_))), "got {err:?}");
    }

    /// The activation-fence MUST match what the block executed with. Construct a
    /// block whose v1 (strict prefix-take) and v2 (sequential pool) accept-sets
    /// diverge: candidate B declares ~0.6× the block gas budget, so B's cumulative
    /// DECLARED gas exceeds the budget (v1 over-caps + skips B) but B fits the
    /// ACTUAL-gas pool (v2 accepts B). The chain ran v2 (B accepted), so tracing the
    /// later tx C must reproduce the v2 prefix. With the correct (v2) fence the trace
    /// succeeds; with the wrong (inert/v1) fence the defense-in-depth prefix-outcome
    /// check fails CLOSED (ReplayMismatch) instead of tracing C against a world
    /// missing B's effects. This is the regression guard for the hardcoded-fence bug.
    #[test]
    fn trace_fence_mismatch_fails_closed_on_v1_v2_divergence() {
        use kaspa_consensus_core::evm::EVM_GAS_LIMIT;
        let big = EVM_GAS_LIMIT * 6 / 10; // ~0.6× budget: A alone fits, A+B over-declares
        let recipient = Address::from([0x77u8; 20]);
        // Three distinct senders (keys 1/2/3), each a plain transfer.
        let (sa, ra) = signed_tx_key(1, 0, revm::primitives::TxKind::Call(recipient), 1, big, Bytes::new());
        let (sb, rb) = signed_tx_key(2, 0, revm::primitives::TxKind::Call(recipient), 1, big, Bytes::new());
        let (sc, rc) = signed_tx_key(3, 0, revm::primitives::TxKind::Call(recipient), 1, 21_000, Bytes::new());

        let mut db = CacheDB::new(EmptyDB::default());
        for s in [sa, sb, sc] {
            db.insert_account_info(
                s,
                AccountInfo { balance: U256::from(10u128).pow(U256::from(24)), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
            );
        }
        let snap = snapshot_from_cachedb(&db);

        let candidates = vec![
            AcceptedTxCandidate { raw: ra.clone(), payload_coinbase: EvmAddress::from_bytes(COINBASE) },
            AcceptedTxCandidate { raw: rb.clone(), payload_coinbase: EvmAddress::from_bytes(COINBASE) },
            AcceptedTxCandidate { raw: rc.clone(), payload_coinbase: EvmAddress::from_bytes(COINBASE) },
        ];
        let payload = EvmExecutionPayload { evm_coinbase: EvmAddress::from_bytes(COINBASE), ..Default::default() };
        // COMMIT under v2 (fence 0, daa_score 100): all three accepted → C is receipt 2.
        let input = EvmBlockInput {
            parent: None,
            header_timestamp_ms: 1_000,
            selected_parent_hash: SELECTED_PARENT,
            blue_work_be: Vec::new(),
            daa_score: 100,
            payload: &payload,
            accepted_txs: &candidates,
            gas_pool_v2_activation_daa_score: 0,
            f002_withdraw_cap_activation_daa_score: u64::MAX,
            f003_mldsa_verify_activation_daa_score: u64::MAX,
            typed_receipt_root_activation_daa_score: u64::MAX,
        };
        let (result, _) = execute_block_evm(seed_cachedb(&snap).unwrap(), &input).unwrap();
        assert_eq!(result.candidate_outcomes[0], EvmCandidateOutcome::Accepted { receipt_index: 0 });
        assert_eq!(result.candidate_outcomes[1], EvmCandidateOutcome::Accepted { receipt_index: 1 }, "v2 accepts B");
        assert_eq!(result.candidate_outcomes[2], EvmCandidateOutcome::Accepted { receipt_index: 2 });
        let receipt_c = result.receipts[2].clone();

        let body = EvmTraceReplayBodyV1 {
            selected_parent: Hash64::from_bytes(SELECTED_PARENT),
            env: EvmReplayEnv { header_timestamp_ms: 1_000, blue_work_be: Vec::new(), daa_score: 100, coinbase: EvmAddress::from_bytes(COINBASE) },
            system_ops: Vec::new(),
            txs: vec![ra, rb, rc]
                .into_iter()
                .enumerate()
                .map(|(i, raw)| EvmReplayTx {
                    tx_hash: tx_hash(&raw),
                    raw,
                    payload_coinbase: EvmAddress::from_bytes(COINBASE),
                    originating_payload_block: Hash64::from_bytes([0x55; 64]),
                    outcome: result.candidate_outcomes[i],
                })
                .collect(),
        };

        // Correct fence (v2): the prefix [A,B] reproduces, C traces successfully.
        let ok = trace_accepted_tx(&snap, None, &body, 2, &receipt_c, 0, u64::MAX, u64::MAX, false, TraceLimits::default()).unwrap();
        assert_eq!(ok.frame.from, sc);
        assert_eq!(ok.frame.to, Some(recipient));

        // Wrong fence (inert/v1): B is over-capped in the rebuilt prefix, diverging
        // from the recorded v2 outcome ⇒ fail closed, never a wrong trace.
        let err = trace_accepted_tx(&snap, None, &body, 2, &receipt_c, u64::MAX, u64::MAX, u64::MAX, false, TraceLimits::default());
        assert!(matches!(err, Err(TraceError::ReplayMismatch(_))), "wrong fence must fail closed, got {err:?}");
    }

    /// prestateTracer (diffMode): a tx that SSTOREs slot 0 = 0x2a yields a storage
    /// diff (pre 0 → post 0x2a) on the contract and a nonce bump on the sender.
    #[test]
    fn trace_prestate_captures_storage_diff() {
        // Runtime code `PUSH1 0x2a PUSH1 0x00 SSTORE STOP` writes slot 0 = 0x2a.
        let code = Bytes::from(vec![0x60, 0x2a, 0x60, 0x00, 0x55, 0x00]);
        let contract = Address::from([0xDDu8; 20]);
        let (sender, raw) = signed_tx(0, revm::primitives::TxKind::Call(contract), 0, 100_000, Bytes::new());
        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            sender,
            AccountInfo { balance: U256::from(10u128).pow(U256::from(22)), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
        );
        db.insert_account_info(
            contract,
            AccountInfo { balance: U256::ZERO, nonce: 1, code_hash: keccak256(&code), code: Some(Bytecode::new_raw(code)) },
        );
        let snap = snapshot_from_cachedb(&db);
        let (receipt, body) = run_and_body(&snap, raw);
        assert!(receipt.succeeded);

        let traced =
            trace_accepted_tx(&snap, None, &body, 0, &receipt, u64::MAX, u64::MAX, u64::MAX, false, TraceLimits::default()).unwrap();

        let c = traced.prestate.iter().find(|a| a.address == contract).expect("contract in prestate diff");
        assert_eq!(c.pre.as_ref().unwrap().storage, vec![(U256::ZERO, U256::ZERO)], "slot 0 pre = 0");
        assert_eq!(c.post.as_ref().unwrap().storage, vec![(U256::ZERO, U256::from(0x2a))], "slot 0 post = 0x2a");

        let s = traced.prestate.iter().find(|a| a.address == sender).expect("sender in prestate diff");
        assert_eq!(s.pre.as_ref().unwrap().nonce, 0);
        assert_eq!(s.post.as_ref().unwrap().nonce, 1, "sender nonce bumped");
    }

    fn head_env() -> EthCallEnv {
        EthCallEnv {
            chain_id: EVM_CHAIN_ID,
            number: 1,
            timestamp: 1_000,
            coinbase: EvmAddress::from_bytes(COINBASE),
            gas_limit: 30_000_000,
            f003_active: false,
        }
    }

    /// §11.6 candidate diagnosis: an executable tx runs (with a call frame); a tx
    /// whose nonce is ahead of the account fails pre-validation (`executed = false`,
    /// a reason, no frame) — exactly the class-2 skip family.
    #[test]
    fn trace_candidate_executable_and_nonce_fail() {
        let recipient = Address::from([0x88u8; 20]);
        let (sender, raw_ok) = signed_tx(0, revm::primitives::TxKind::Call(recipient), 5, 21_000, Bytes::new());
        let snap = fund(sender);
        let env = head_env();

        let ct = trace_candidate_tx(&snap, &env, &raw_ok, TraceLimits::default()).unwrap();
        assert!(ct.executed && ct.succeeded, "nonce-0 tx is executable at head");
        assert_eq!(ct.frame.as_ref().unwrap().from, sender);

        // Same sender (fixed key), nonce 5 ≠ account nonce 0 ⇒ pre-validation failure.
        let (_s, raw_bad) = signed_tx(5, revm::primitives::TxKind::Call(recipient), 5, 21_000, Bytes::new());
        let ct2 = trace_candidate_tx(&snap, &env, &raw_bad, TraceLimits::default()).unwrap();
        assert!(!ct2.executed, "nonce-too-high fails pre-validation");
        assert!(ct2.reason.is_some() && ct2.frame.is_none());
    }

    /// A candidate that executes but reverts: `executed = true`, `succeeded = false`,
    /// with a call frame (the revert is an execution outcome, not a skip).
    #[test]
    fn trace_candidate_revert_executes() {
        let code = Bytes::from(vec![0x60, 0x00, 0x60, 0x00, 0xfd]); // PUSH1 0 PUSH1 0 REVERT
        let contract = Address::from([0xBEu8; 20]);
        let (sender, raw) = signed_tx(0, revm::primitives::TxKind::Call(contract), 0, 100_000, Bytes::new());
        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            sender,
            AccountInfo { balance: U256::from(10u128).pow(U256::from(22)), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
        );
        db.insert_account_info(
            contract,
            AccountInfo { balance: U256::ZERO, nonce: 1, code_hash: keccak256(&code), code: Some(Bytecode::new_raw(code)) },
        );
        let snap = snapshot_from_cachedb(&db);
        let ct = trace_candidate_tx(&snap, &head_env(), &raw, TraceLimits::default()).unwrap();
        assert!(ct.executed && !ct.succeeded, "reverts but executed");
        assert_eq!(ct.frame.as_ref().unwrap().error.as_deref(), Some("execution reverted"));
    }

    /// Struct/opcode logger: capture=true yields one entry per executed opcode
    /// (PUSH1/SSTORE/STOP…) with pc/op/gas/depth; capture=false yields no logs.
    #[test]
    fn trace_struct_logs_capture_opcodes() {
        let code = Bytes::from(vec![0x60, 0x2a, 0x60, 0x00, 0x55, 0x00]); // PUSH1 0x2a PUSH1 0x00 SSTORE STOP
        let contract = Address::from([0xC5u8; 20]);
        let (sender, raw) = signed_tx(0, revm::primitives::TxKind::Call(contract), 0, 100_000, Bytes::new());
        let mut db = CacheDB::new(EmptyDB::default());
        db.insert_account_info(
            sender,
            AccountInfo { balance: U256::from(10u128).pow(U256::from(22)), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
        );
        db.insert_account_info(
            contract,
            AccountInfo { balance: U256::ZERO, nonce: 1, code_hash: keccak256(&code), code: Some(Bytecode::new_raw(code)) },
        );
        let snap = snapshot_from_cachedb(&db);
        let (receipt, body) = run_and_body(&snap, raw);

        let traced =
            trace_accepted_tx(&snap, None, &body, 0, &receipt, u64::MAX, u64::MAX, u64::MAX, true, TraceLimits::default()).unwrap();
        let logs = traced.struct_logs.expect("struct logs captured when requested");
        let ops: Vec<&str> = logs.iter().map(|l| l.op_name).collect();
        assert!(ops.contains(&"SSTORE"), "expected SSTORE in {ops:?}");
        assert!(ops.contains(&"PUSH1"), "expected PUSH1 in {ops:?}");
        assert!(logs.iter().all(|l| l.depth >= 1), "depth is 1-based");
        assert!(logs.iter().any(|l| l.gas_cost > 0), "at least one op has a gas cost");

        // callTracer/default replay (capture=false) carries no struct logs.
        let plain =
            trace_accepted_tx(&snap, None, &body, 0, &receipt, u64::MAX, u64::MAX, u64::MAX, false, TraceLimits::default()).unwrap();
        assert!(plain.struct_logs.is_none());
    }
}
