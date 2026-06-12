//! EVM block-environment derivation (design §4). Every input is ancestor-derived
//! — the `selected_parent`'s committed `EvmExecutionHeader` plus this block's L1
//! header — so the env (and therefore the committed EVM result) is a pure
//! function of the block. The current L1/EVM block hashes are intentionally NOT
//! inputs (they already commit to the EVM result — design §4.2 circularity rule).

use kaspa_consensus_core::evm::{
    EvmExecutionHeader, EVM_BASE_FEE_MAX_CHANGE_DENOMINATOR, EVM_CHAIN_ID, EVM_ELASTICITY_MULTIPLIER, EVM_GAS_LIMIT,
    EVM_INITIAL_BASE_FEE, MISAKA_EVM_PREVRANDAO_CONTEXT,
};
use kaspa_hashes::blake2b_256_keyed;
use revm::primitives::{Address, B256};

/// The deterministic EVM block environment for one block (design §4.1).
#[derive(Clone, Debug)]
pub struct EvmDerivedEnv {
    pub evm_number: u64,
    pub evm_timestamp_sec: u64,
    pub base_fee_per_gas: u128,
    pub prev_randao: B256,
    pub coinbase: Address,
    pub gas_limit: u64,
    pub chain_id: u64,
}

/// Derive the env for a block from its `selected_parent`'s EVM header (`None`
/// for the first EVM block, whose parent is the EVM genesis at number 0) and
/// this block's L1 header context.
pub fn derive_env(
    parent: Option<&EvmExecutionHeader>,
    header_timestamp_ms: u64,
    selected_parent_hash: &[u8; 64],
    blue_work_be: &[u8],
    daa_score: u64,
    coinbase: Address,
) -> EvmDerivedEnv {
    let (parent_number, parent_ts, base_fee_per_gas) = match parent {
        Some(p) => (p.evm_number, p.evm_timestamp_sec, next_base_fee(evmu256_to_u128(p.base_fee_per_gas), p.gas_used)),
        None => (0, 0, EVM_INITIAL_BASE_FEE as u128),
    };

    // Non-decreasing EVM logical time (design v0.4 §5.3, D6 — replaced the
    // v0.2 strict-monotone parent+1 clamp): consecutive EVM blocks may share a
    // timestamp, which keeps logical time bounded by the header's deviation
    // tolerance at ANY chain-block rate (kills the BPS≤1 coupling, audit
    // K-3/AH-3). Uniswap v2 (`timeElapsed > 0`) and v3 (same-ts early return)
    // oracles are equal-timestamp-safe; contracts must sequence by
    // block.number, not block.timestamp.
    let header_sec = header_timestamp_ms / 1000;
    let evm_timestamp_sec = header_sec.max(parent_ts);

    // prevrandao = keyed-BLAKE2b-256(domain, selected_parent_hash ‖ blue_work ‖ daa_score)
    // (design §4.3). FROZEN byte order. Grindable, not secure randomness.
    let mut preimage = Vec::with_capacity(64 + blue_work_be.len() + 8);
    preimage.extend_from_slice(selected_parent_hash);
    preimage.extend_from_slice(blue_work_be);
    preimage.extend_from_slice(&daa_score.to_le_bytes());
    let prev_randao = B256::from(blake2b_256_keyed(MISAKA_EVM_PREVRANDAO_CONTEXT, &preimage));

    EvmDerivedEnv {
        evm_number: parent_number + 1,
        evm_timestamp_sec,
        base_fee_per_gas,
        prev_randao,
        coinbase,
        gas_limit: EVM_GAS_LIMIT,
        chain_id: EVM_CHAIN_ID,
    }
}

/// EIP-1559 base-fee update from the parent block (design §5.3, P2 fixed-limit
/// form). Integer math, deterministic.
pub fn next_base_fee(parent_base_fee: u128, parent_gas_used: u64) -> u128 {
    let gas_target = (EVM_GAS_LIMIT / EVM_ELASTICITY_MULTIPLIER) as u128;
    let denom = EVM_BASE_FEE_MAX_CHANGE_DENOMINATOR as u128;
    let used = parent_gas_used as u128;
    if used == gas_target {
        parent_base_fee
    } else if used > gas_target {
        // Increase, by at least 1 wei.
        let delta = (parent_base_fee.saturating_mul(used - gas_target) / gas_target / denom).max(1);
        parent_base_fee.saturating_add(delta)
    } else {
        // Decrease.
        let delta = parent_base_fee.saturating_mul(gas_target - used) / gas_target / denom;
        parent_base_fee.saturating_sub(delta)
    }
}

/// The committed base fee is a `U256` field but is computed and carried internally
/// as `u128` (audit R2-#5). This is DETERMINISTIC — every node runs the identical
/// `u128` saturating update, so it can never cause a consensus split — and the
/// `u128` ceiling is economically UNREACHABLE: the seed is 1 gwei (1e9) and the
/// EIP-1559 step is capped at +1/8 (+12.5%) per block, so reaching `u128::MAX`
/// (~3.4e38) would take ~580 *consecutive maximally-full* blocks (see
/// `base_fee_stays_within_u128_over_a_long_full_run`). Long before that the base
/// fee would exceed the entire native supply (bounded by bridged deposits, far
/// below u128), so no tx could pay it → blocks empty → the fee decreases. The
/// parent value below therefore always fits u128 (we only ever produce u128
/// fees); the saturating fallback is pure defense, never reached on a valid chain.
fn evmu256_to_u128(v: kaspa_consensus_core::evm::EvmU256) -> u128 {
    v.try_to_u128().unwrap_or(u128::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent(ts: u64) -> EvmExecutionHeader {
        EvmExecutionHeader { evm_number: 1, evm_timestamp_sec: ts, ..Default::default() }
    }

    /// v0.4 §5.3 (D6) / invariant I8: the EVM logical clock is NON-decreasing —
    /// equal timestamps across consecutive EVM blocks are allowed, and the
    /// clock never runs ahead of `max(header time, parent time)`.
    #[test]
    fn timestamp_clamp_is_non_decreasing_not_strict() {
        let cb = Address::ZERO;
        // Header behind the parent clock ⇒ clamped UP to the parent (not parent+1).
        let e = derive_env(Some(&parent(100)), 98_000, &[0u8; 64], &[], 0, cb);
        assert_eq!(e.evm_timestamp_sec, 100);
        // Header equal to the parent clock ⇒ EQUAL is allowed (the v0.2
        // strict-monotone clamp would have forced 101 here).
        let e = derive_env(Some(&parent(100)), 100_000, &[0u8; 64], &[], 0, cb);
        assert_eq!(e.evm_timestamp_sec, 100);
        // Header ahead ⇒ wall clock wins.
        let e = derive_env(Some(&parent(100)), 102_000, &[0u8; 64], &[], 0, cb);
        assert_eq!(e.evm_timestamp_sec, 102);
        // First EVM block: parent clock is 0.
        let e = derive_env(None, 5_000, &[0u8; 64], &[], 0, cb);
        assert_eq!(e.evm_timestamp_sec, 5);
    }

    /// audit R2-#5: the base fee is carried as u128. Drive the EIP-1559 update
    /// through 500 consecutive MAXIMALLY-FULL blocks (each forces the maximum
    /// +12.5% step) and assert it stays comfortably below u128::MAX — i.e. the
    /// ceiling is unreachable within any realistic horizon, and the update never
    /// overflows/saturates on a valid chain. (At the +1/8 cap it takes ~580 such
    /// blocks even to approach u128::MAX, by which point the fee would dwarf the
    /// whole native supply.) Also a pure determinism check: same inputs → same fee.
    #[test]
    fn base_fee_stays_within_u128_over_a_long_full_run() {
        let full_gas = EVM_GAS_LIMIT; // every block 100% full ⇒ max increase
        let mut fee = EVM_INITIAL_BASE_FEE as u128;
        for _ in 0..500 {
            let next = next_base_fee(fee, full_gas);
            assert!(next > fee, "a full block must raise the base fee");
            assert!(next < u128::MAX / 2, "base fee must stay far below the u128 ceiling");
            // determinism: recomputing the same step yields the same value.
            assert_eq!(next, next_base_fee(fee, full_gas));
            fee = next;
        }
        // An empty block lowers it; an at-target block holds it.
        assert!(next_base_fee(fee, 0) < fee);
        let target = EVM_GAS_LIMIT / kaspa_consensus_core::evm::EVM_ELASTICITY_MULTIPLIER;
        assert_eq!(next_base_fee(fee, target), fee);
    }
}
