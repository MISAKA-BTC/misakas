//! Single registration seam for all MISAKA EVM precompiles / call-frame
//! intercepts (PREA design v1.1 §9.5, §23.1 #4). Both the block executor
//! ([`crate::executor::execute_block_evm`]) and the read-only simulator
//! ([`crate::sim::simulate_call`] / `estimate_gas`) register handlers through
//! THIS function so they can never diverge — an `eth_call` / `eth_estimateGas`
//! result is always computed with the exact handler set consensus uses.
//!
//! - **F002** (`MISAKA_WITHDRAW`) is always registered (live since the EVM lane).
//! - **F003** (`MLDSA87_VERIFY`) is registered ONLY when its activation fence is
//!   reached (`f003_active`). Below the fence it is absent, so a call to
//!   `0x…F003` behaves as a call to an empty account — byte-identical execution,
//!   genesis/state-root unchanged. `f003_active` is derived identically on both
//!   sides (`daa_score >= evm_f003_mldsa_verify_activation_daa_score`), keeping
//!   executor↔simulation parity.

use revm::handler::register::EvmHandler;
use revm::Database;

/// Register every MISAKA precompile/intercept on `handler`. F002 unconditionally;
/// F003 iff `f003_active`. Order: F002 then F003 (each wraps `execution.call`, so
/// a call to F003 is matched by F003's wrapper, a call to F002 falls through F003
/// to F002, and any other call falls through both to the default).
pub fn register_all_misaka_precompiles<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>, f003_active: bool) {
    crate::withdraw::register_f002_withdraw(handler);
    if f003_active {
        crate::mldsa_verify::register_f003_mldsa_verify(handler);
    }
}
