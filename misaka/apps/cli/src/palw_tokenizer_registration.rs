//! Read-only RFC-0051 quote and accepted registration progress.
use crate::{CliError, CliResult, exit, node::Ctx};
use clap::Subcommand;
use kaspa_consensus_core::palw_tokenizer_registration_v1::{
    TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1, TokenizerRegistrationOpenBodyV1,
};
use kaspa_rpc_core::{GetPalwTokenizerRegistrationQuoteRequest, GetPalwTokenizerRegistrationStatusRequest, api::rpc::RpcApi};
use std::{io::Read, path::PathBuf};

#[derive(Subcommand, Debug)]
pub(crate) enum TokenizerRegistrationCmd {
    /// Quote a canonical unsigned Open body. Does not sign, pay or reserve slots.
    Quote {
        /// Binary Borsh TokenizerRegistrationOpenBodyV1, without a carrier or signature.
        #[arg(long)]
        open_body: PathBuf,
    },
    /// Read an accepted Open's immutable slots and received Parts.
    Status {
        /// Full 128-hex Open ID returned by the quote.
        open_id: String,
    },
}

fn input_error(message: impl Into<String>) -> CliError {
    CliError::new(exit::GENERIC, message)
}

fn open_body_hex(path: &std::path::Path) -> Result<String, CliError> {
    let mut bytes = Vec::with_capacity(TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1 + 1);
    std::fs::File::open(path)
        .map_err(|e| input_error(e.to_string()))?
        .take((TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1 + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| input_error(e.to_string()))?;
    if bytes.len() != TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1 {
        return Err(input_error(format!("Open body must be exactly {TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1} binary bytes")));
    }
    let body: TokenizerRegistrationOpenBodyV1 = borsh::from_slice(&bytes).map_err(|e| input_error(e.to_string()))?;
    body.check_v1().map_err(input_error)?;
    Ok(faster_hex::hex_string(&bytes))
}

fn print_observation<T: serde::Serialize>(reply: &T, observation: &str) -> CliResult {
    let mut value = serde_json::to_value(reply).map_err(|e| input_error(e.to_string()))?;
    value.as_object_mut().expect("RPC response is an object").remove("observationJson");
    value["observation"] = if observation.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(observation).map_err(|e| input_error(format!("malformed tokenizer observation: {e}")))?
    };
    println!("{}", serde_json::to_string_pretty(&value).map_err(|e| input_error(e.to_string()))?);
    Ok(())
}

pub(crate) async fn run(ctx: &Ctx, command: TokenizerRegistrationCmd) -> CliResult {
    match command {
        TokenizerRegistrationCmd::Quote { open_body } => {
            let request = GetPalwTokenizerRegistrationQuoteRequest { open_body_hex: open_body_hex(&open_body)? };
            let reader = crate::palw_derived::connect(ctx).await?;
            let result = reader.client.get_palw_tokenizer_registration_quote(request).await;
            let _ = reader.client.disconnect().await;
            let reply = result.map_err(|e| CliError::new(exit::CONNECTION, format!("tokenizer quote RPC: {e}")))?;
            print_observation(&reply, &reply.observation_json)?;
            if !reply.available || !reply.quoted {
                return Err(input_error(if reply.refusal.is_empty() {
                    "tokenizer registration is not available".into()
                } else {
                    reply.refusal
                }));
            }
        }
        TokenizerRegistrationCmd::Status { open_id } => {
            if open_id.len() != 128 || !open_id.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(input_error("Open ID must contain exactly 128 hex digits"));
            }
            let reader = crate::palw_derived::connect(ctx).await?;
            let result =
                reader.client.get_palw_tokenizer_registration_status(GetPalwTokenizerRegistrationStatusRequest { open_id }).await;
            let _ = reader.client.disconnect().await;
            let reply = result.map_err(|e| CliError::new(exit::CONNECTION, format!("tokenizer status RPC: {e}")))?;
            print_observation(&reply, &reply.observation_json)?;
            if !reply.available {
                return Err(input_error("tokenizer registration is not available"));
            }
        }
    }
    Ok(())
}
