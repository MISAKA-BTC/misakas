//! **The UTXO side of the EVM bridge** (EVM lane §7.2 / §9.2): lock MSK for an EVM address, then
//! claim the lock on a mining node, which credits the address when an accepting chain block
//! executes the claim.
//!
//! Moved here from the `kaspa-pq-validator` sidecar, which was the only shipped tool that carried it.
//! The builders are unchanged (`kaspa-pq-validator-core`), and so are the guards: the lane must be
//! active (a lock that can never be claimed can only be refunded after its timeout), and the EVM
//! address is checked — a typo credits a stranger and there is no refund once the claim executes.

use crate::keys::KeySource;
use crate::node::Ctx;
use crate::wallet::{Funding, connect, page_all, sompi_to_msk};
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_rpc_core::RpcTransaction;
use kaspa_rpc_core::api::rpc::RpcApi;
use serde_json::json;

/// The most inputs one deposit spends (each ML-DSA-87 input is about 7 KB).
const MAX_DEPOSIT_INPUTS: usize = 20;

/// EIP-55 mixed-case checksum of a 20-byte address: `0x` and 40 case-encoded hex characters.
pub(crate) fn eip55_checksum(addr: &[u8; 20]) -> String {
    use sha3::{Digest, Keccak256};
    let lower: String = addr.iter().map(|b| format!("{b:02x}")).collect();
    let hash = Keccak256::digest(lower.as_bytes());
    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, c) in lower.chars().enumerate() {
        // Uppercase a hex letter iff the matching Keccak-256 nibble is at least 8.
        if c.is_ascii_alphabetic() && ((hash[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0x0f) >= 8 {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// What a deposit destination is, once parsed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EvmDestination {
    pub bytes: [u8; 20],
    pub checksummed: String,
    /// The operator typed a checksummed (mixed-case) address, so a typo would have been caught.
    pub checked: bool,
    /// A system or precompile address — almost certainly a mistake, since no account holds a
    /// balance there.
    pub system: bool,
}

/// Parse a deposit destination: 40 hex characters (optional `0x`), a valid EIP-55 checksum where
/// one is given, never the zero address.
pub(crate) fn parse_evm_destination(input: &str) -> Result<EvmDestination, String> {
    let hex = input.strip_prefix("0x").or_else(|| input.strip_prefix("0X")).unwrap_or(input);
    if hex.len() != 40 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("--evm-address must be 40 hex characters (20 bytes), got '{input}'"));
    }
    let mut bytes = [0u8; 20];
    faster_hex::hex_decode(hex.to_ascii_lowercase().as_bytes(), &mut bytes).map_err(|e| format!("malformed --evm-address: {e}"))?;
    let checksummed = eip55_checksum(&bytes);
    let checked = hex.bytes().any(|b| b.is_ascii_uppercase()) && hex.bytes().any(|b| b.is_ascii_lowercase());
    if checked && checksummed != format!("0x{hex}") {
        return Err(format!(
            "--evm-address fails its EIP-55 checksum — likely a typo. You entered 0x{hex}; those bytes check as {checksummed}. \
             A wrong address cannot be recovered after the claim."
        ));
    }
    if bytes == [0u8; 20] {
        return Err("--evm-address is the zero address — refusing: the credit could never be spent".to_string());
    }
    let tail = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let system = bytes[..16].iter().all(|b| *b == 0) && (matches!(tail, 0xF001..=0xF003) || (1..=9).contains(&tail));
    Ok(EvmDestination { bytes, checksummed, checked, system })
}

/// `misaka evm deposit-lock`: lock `amount_sompi` for `evm_address`, refundable to this key after
/// `timeout_daa_delta` DAA. A dry run unless `yes`.
#[allow(clippy::too_many_arguments)]
pub async fn deposit_lock(
    ctx: &Ctx,
    ks: &KeySource,
    evm_address: &str,
    amount_sompi: u64,
    claim_tip_sompi: u64,
    timeout_daa_delta: u64,
    fee: Option<u64>,
    yes: bool,
) -> CliResult {
    let generic = |m: String| CliError::new(exit::GENERIC, m);
    if amount_sompi == 0 {
        return Err(generic("--amount must be > 0 (sompi)".to_string()));
    }
    if claim_tip_sompi > amount_sompi {
        return Err(generic("--claim-tip cannot exceed --amount: the tip is paid out of the deposit".to_string()));
    }
    let destination = parse_evm_destination(evm_address).map_err(generic)?;
    let nv = connect(ctx).await?;
    if !nv.params.is_evm_active(nv.virtual_daa) {
        return Err(generic(format!(
            "the EVM lane is not active on {} at DAA {} — a deposit-lock here could only be refunded after its timeout, never \
             claimed. Refusing to create it.",
            ctx.network, nv.virtual_daa
        )));
    }
    let key = ks.load_key()?;
    let prefix = nv.params.prefix();
    let from = key.funding_address(prefix);
    let mass = MassCalculator::new(
        nv.params.mass_per_tx_byte,
        nv.params.mass_per_script_pub_key_byte,
        nv.params.mass_per_sig_op,
        nv.params.storage_mass_parameter,
    );
    let mut mature: Vec<Funding> = page_all(&nv, &from).await?.into_iter().filter(|u| u.selectable()).collect();
    mature.sort_by(|a, b| b.amount.cmp(&a.amount));
    let mut selected: Vec<&Funding> = Vec::new();
    let mut sum = 0u64;
    let mut fee_sompi = fee.unwrap_or_else(|| key.estimate_deposit_lock_fee_for_inputs(&mass, prefix, 1));
    for u in mature.iter().take(MAX_DEPOSIT_INPUTS) {
        selected.push(u);
        sum = sum.saturating_add(u.amount);
        if fee.is_none() {
            fee_sompi = key.estimate_deposit_lock_fee_for_inputs(&mass, prefix, selected.len());
        }
        if sum >= amount_sompi.saturating_add(fee_sompi) {
            break;
        }
    }
    let needed = amount_sompi.checked_add(fee_sompi).ok_or_else(|| generic("amount + fee overflows".to_string()))?;
    if selected.is_empty() || sum < needed {
        return Err(generic(format!(
            "insufficient mature funds at {from}: have {} MSK across {} UTXO(s) (cap {MAX_DEPOSIT_INPUTS}), need {} MSK (amount + fee {fee_sompi} sompi)",
            sompi_to_msk(sum),
            selected.len(),
            sompi_to_msk(needed)
        )));
    }
    let timeout_daa_score = nv.virtual_daa.saturating_add(timeout_daa_delta);
    let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = selected.iter().map(|u| (u.outpoint, u.entry.clone())).collect();
    let tx = key
        .build_funded_deposit_lock_tx_multi(amount_sompi, destination.bytes, timeout_daa_score, claim_tip_sompi, &fundings, fee_sompi)
        .map_err(|e| generic(format!("build deposit-lock: {e}")))?;
    let txid = if yes {
        Some(
            nv.client
                .submit_transaction(RpcTransaction::from(&tx), false)
                .await
                .map_err(|e| CliError::new(exit::TX_REJECTED, format!("submit deposit-lock: {e}")))?
                .to_string(),
        )
    } else {
        None
    };
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            json!({ "ok": true, "dryRun": txid.is_none(), "from": from.to_string(), "evmAddress": destination.checksummed,
                    "checksumChecked": destination.checked, "systemAddress": destination.system,
                    "amountSompi": amount_sompi, "claimTipSompi": claim_tip_sompi, "feeSompi": fee_sompi,
                    "refundTimeoutDaa": timeout_daa_score, "inputs": fundings.len(),
                    "lockOutpoint": txid.as_ref().map(|t| format!("{t}:0")) })
        ),
        OutputFormat::Human => {
            println!("From        : {from}");
            println!("EVM address : {}", destination.checksummed);
            if !destination.checked {
                println!(
                    "              (typed without an EIP-55 checksum, so a typo could not be detected — prefer pasting the checksummed form above)"
                );
            }
            if destination.system {
                println!("              WARNING: a system or precompile address — almost certainly a mistake");
            }
            println!("Amount      : {} MSK   Claim tip: {claim_tip_sompi} sompi   Fee: {fee_sompi} sompi", sompi_to_msk(amount_sompi));
            println!("Refund after: DAA {timeout_daa_score} (to this key)");
            match &txid {
                Some(t) => {
                    println!("Lock        : {t}:0");
                    println!("Next        : misaka evm claim --outpoint {t}:0   (against a MINING node, once the lock is accepted)");
                }
                None => println!("Mode        : dry-run (no submit; pass --yes to broadcast)"),
            }
        }
    }
    let _ = nv.client.disconnect().await;
    Ok(())
}

/// `misaka evm claim`: queue the claim of a deposit lock on the node (which must mine for the claim
/// to be included).
pub async fn claim(ctx: &Ctx, outpoint: &str) -> CliResult {
    let (txid, index) = outpoint
        .rsplit_once(':')
        .and_then(|(t, i)| i.parse::<u32>().ok().map(|i| (t.to_string(), i)))
        .ok_or_else(|| CliError::new(exit::GENERIC, format!("--outpoint must be 'txid_hex:index', got '{outpoint}'")))?;
    let nv = connect(ctx).await?;
    let response = nv
        .client
        .submit_evm_deposit_claim(txid.clone(), index)
        .await
        .map_err(|e| CliError::new(exit::TX_REJECTED, format!("submitEvmDepositClaim: {e}")))?;
    match ctx.output {
        OutputFormat::Json => {
            println!("{}", json!({ "ok": true, "outpoint": format!("{txid}:{index}"), "response": format!("{response:?}") }))
        }
        OutputFormat::Human => println!("Claim queued for {txid}:{index}: {response:?}"),
    }
    let _ = nv.client.disconnect().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eip55_matches_the_reference_vectors() {
        // EIP-55's own test vectors.
        for expected in [
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        ] {
            let parsed = parse_evm_destination(expected).expect("a reference address parses");
            assert_eq!(parsed.checksummed, expected);
            assert!(parsed.checked && !parsed.system);
        }
    }

    #[test]
    fn a_typo_the_zero_address_and_a_short_address_are_refused() {
        assert!(parse_evm_destination("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAeD").is_err(), "one flipped case is a failed checksum");
        assert!(parse_evm_destination("0x0000000000000000000000000000000000000000").is_err());
        assert!(parse_evm_destination("0x5aaeb6053f3e94c9b9a09f33669435e7ef1beae").is_err(), "39 characters");
        let lower = parse_evm_destination("5aaeb6053f3e94c9b9a09f33669435e7ef1beaed").expect("single-case parses");
        assert!(!lower.checked, "and says it could not be checked");
        assert!(parse_evm_destination("0x000000000000000000000000000000000000f001").unwrap().system);
        assert!(parse_evm_destination("0x0000000000000000000000000000000000000001").unwrap().system);
    }
}
