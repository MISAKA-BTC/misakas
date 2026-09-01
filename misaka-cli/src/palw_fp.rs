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
pub async fn submit(ctx: &Ctx, path: &Path, yes: bool, material_out: Option<&Path>, capture: Option<&Path>) -> Result<(), CliError> {
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

    // **Everything that can fail about the material is done BEFORE the broadcast.**
    //
    // It used to run entirely after, on the reasoning that a material for a claim the chain never
    // saw would make a panel replay a ghost. That reasoning is right about the FINAL file and
    // wrong about the work: a decode error or an unwritable directory then surfaced only once the
    // claim was already on chain, and re-running could not repair it — the second run hits
    // `submit_transaction` first, which refuses an already-accepted transaction and returns
    // before the material block. The claim would sit there, certifiable by nobody, with its
    // producer's bond carrying the exposure.
    //
    // So: encode and stage to a `.partial` now (the producer's own retention discipline —
    // `retain_execution` writes then renames), broadcast, and rename only after acceptance. A
    // reader never sees a half-written obligation, and a failure that can be caught early is.
    let staged = match material_out {
        Some(dir) => {
            let payload: kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 = borsh::from_slice(&tx.payload)
                .map_err(|e| {
                    CliError::new(exit::GENERIC, format!("this payload does not decode, so no material can be written: {e}"))
                })?;
            let claim = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&payload.commitment);
            // With the capture, the answer travels beside the question (`FPC1`, ADR-0073
            // Decision 1a); without it, the question alone (`FPM1`) and a seat's only verifier is
            // a re-run. Checked before the broadcast like everything else here: a capture that
            // does not decode as the family's tuple is refused now, not discovered by a seat.
            let bytes = match capture {
                Some(capture_path) => {
                    let capture_bytes = std::fs::read(capture_path)
                        .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", capture_path.display())))?;
                    // The family tuple is the backend's to decode (a seat verifies it against the
                    // claim's roots); what is refused here is the one mistake a hand can make —
                    // pointing this at a payload instead of at the worker's capture.
                    let looks_like_a_payload = capture_bytes.is_empty()
                        || capture_bytes.starts_with(&kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_MATERIAL_V1_MAGIC)
                        || capture_bytes.starts_with(&kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_CAPTURE_V1_MAGIC);
                    if looks_like_a_payload {
                        return Err(CliError::new(
                            exit::GENERIC,
                            format!("{} is not a family capture (expected the worker's material.bin)", capture_path.display()),
                        ));
                    }
                    kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_encode_v1(
                        &payload.commitment.job,
                        &payload.prompt_token_ids,
                        &capture_bytes,
                    )
                }
                None => kaspa_consensus_core::palw_freeprompt_v3::palw_fp_material_encode_v1(
                    &payload.commitment.job,
                    &payload.prompt_token_ids,
                ),
            };
            std::fs::create_dir_all(dir).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", dir.display())))?;
            let file = dir.join(format!("{claim}.material"));
            let partial = file.with_extension("material.partial");
            std::fs::write(&partial, &bytes).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", partial.display())))?;
            Some((partial, file))
        }
        None => None,
    };

    let nv = connect(ctx).await?;
    let submitted = nv.client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|e| {
        // The staged file is not a claim's material if the claim never reached the chain.
        if let Some((partial, _)) = &staged {
            let _ = std::fs::remove_file(partial);
        }
        CliError::new(exit::GENERIC, format!("submit {txid}: {e}"))
    })?;

    // Accepted: the obligation is real, so the file takes its real name.
    let mut material_note: Option<String> = None;
    if let Some((partial, file)) = staged {
        std::fs::rename(&partial, &file).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", file.display())))?;
        material_note = Some(file.display().to_string());
    }

    match ctx.output {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({ "submitted": true, "txid": submitted.to_string(), "material": material_note }))
        }
        _ => {
            println!("submitted {submitted}");
            if let Some(file) = &material_note {
                println!("  DA material written: {file}");
            }
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
