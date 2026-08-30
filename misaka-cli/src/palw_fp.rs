//! **FP-R4: putting a free-prompt commitment on the chain.**
//!
//! `misaka-palw-fp-rail` builds and signs the transaction and stops there, and its reason for
//! stopping — "no network accepts subnetwork `0x4a` yet" — is out of date: `tx_validation_in_
//! isolation` validates that subnetwork, the algo-7 arm exists in `calculate_l1_tag`, and
//! testnet-11 runs the `ConsensusV2` bundle. What was actually missing is this: a submitter with a
//! node connection.
//!
//! It lives in the CLI rather than in the rail because the CLI already holds the RPC client, the
//! endpoint registry and the network identity, and a second submission path would be a second
//! place for "which node, which network, which encoding" to be answered differently. The rail
//! stays a builder; this is the mouth.
//!
//! # Dry-run first, like every other side-effecting command here
//!
//! Without `--yes` it decodes the transaction, prints what would be sent, and exits. That is not
//! politeness: a free-prompt commitment spends a real UTXO and starts a claim whose exposure the
//! executor's bond carries until it certifies, so the preview is the last cheap place to notice
//! that the file is the wrong one.

use crate::node::Ctx;
use crate::wallet::connect;
use crate::{CliError, OutputFormat, exit};
use kaspa_consensus_core::tx::Transaction;
use kaspa_rpc_core::{RpcTransaction, api::rpc::RpcApi};
use std::path::Path;

/// Submit the rail's `*.commitment-tx.borsh`.
pub async fn submit(ctx: &Ctx, path: &Path, yes: bool) -> Result<(), CliError> {
    let bytes = std::fs::read(path).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", path.display())))?;
    // Borsh, because that is what the rail wrote. A file that does not decode is named as such
    // rather than passed to a node that would refuse it less clearly.
    let tx: Transaction = borsh::from_slice(&bytes)
        .map_err(|e| CliError::new(exit::GENERIC, format!("{} is not a borsh transaction: {e}", path.display())))?;

    let subnetwork = tx.subnetwork_id.clone();
    let expected = kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
    if subnetwork != expected {
        // Refused rather than submitted and left to the node: this command exists for one kind of
        // transaction, and submitting another under its name would put a spend on chain that the
        // operator asked for by accident.
        return Err(CliError::new(
            exit::GENERIC,
            format!("{} carries subnetwork {subnetwork} — this command submits free-prompt commitments (0x4a) only", path.display()),
        ));
    }

    let txid = tx.id();
    let inputs = tx.inputs.len();
    let outputs = tx.outputs.len();
    let payload = tx.payload.len();

    if !yes {
        match ctx.output {
            OutputFormat::Json => println!(
                "{}",
                serde_json::json!({
                    "dry_run": true, "txid": txid.to_string(), "subnetwork": subnetwork.to_string(),
                    "inputs": inputs, "outputs": outputs, "payload_bytes": payload,
                })
            ),
            _ => {
                println!("free-prompt commitment {txid}");
                println!("  subnetwork {subnetwork}, {inputs} input(s), {outputs} output(s), {payload}-byte payload");
                println!("  dry run — nothing was sent. Re-run with --yes to submit.");
            }
        }
        return Ok(());
    }

    let nv = connect(ctx).await?;
    let submitted = nv
        .client
        .submit_transaction(RpcTransaction::from(&tx), false)
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("submit {txid}: {e}")))?;

    match ctx.output {
        OutputFormat::Json => println!("{}", serde_json::json!({ "submitted": true, "txid": submitted.to_string() })),
        _ => {
            println!("submitted {submitted}");
            // Said here because the next question is always "so am I mining now", and the answer
            // is no — not yet, and not because anything is wrong.
            println!(
                "  the claim now has to certify — bind, receipt, challenge, court — before any quantum of it\n  \
                 can be spent into a receipt block. That is chain time, not seconds."
            );
        }
    }
    Ok(())
}
