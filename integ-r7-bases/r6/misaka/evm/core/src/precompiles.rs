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
//! - **The model market** (ADR-0089: `0x…F010` registry, `0x…F011` AMM, `0x…F012`
//!   position, `0x…F013` writer, and every line's facade) is registered ONLY when
//!   the caller hands over a [`MarketHandlers`] whose fences say `evm_active`
//!   (`Params::palw_model_evm`, Decision 9). Below that fence — or with no view at
//!   all — nothing is registered and the four addresses and the facades are empty
//!   accounts: the F003 idiom.

use crate::model_market::MarketHandlers;
use revm::Database;
use revm::handler::register::EvmHandler;

/// Register every MISAKA precompile/intercept on `handler`. F002 unconditionally;
/// F003 iff `f003_active`; the model market iff `market` is given AND its fences say
/// `evm_active`. Order: F002, F003, market (each wraps `execution.call`, so a call to a
/// market door is matched by the market's wrapper, a call to F003 falls through it to
/// F003's, a call to F002 falls through both, and any other call falls through all
/// three to the default).
pub fn register_all_misaka_precompiles<EXT, DB: Database>(
    handler: &mut EvmHandler<'_, EXT, DB>,
    f003_active: bool,
    market: Option<MarketHandlers>,
) {
    crate::withdraw::register_f002_withdraw(handler);
    if f003_active {
        crate::mldsa_verify::register_f003_mldsa_verify(handler);
    }
    if let Some(market) = market
        && market.fences().evm_active
    {
        crate::model_market::register_market_handlers(handler, market);
    }
}
