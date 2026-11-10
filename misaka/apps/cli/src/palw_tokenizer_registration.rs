//! RFC-0051 reads and locally signed payload preparation. Preparation does not
//! fund, submit or reserve service slots.
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
    /// Validate the pinned raw tokenizer and sign a payload bundle using a live Open quote.
    Prepare {
        #[arg(long)]
        open_body: PathBuf,
        #[arg(long)]
        raw_tokenizer: PathBuf,
        /// Binary Borsh EOG opening for the table root named by the Open.
        #[arg(long)]
        eog_opening: PathBuf,
        #[arg(long)]
        key_file: Option<String>,
        #[arg(long)]
        key_stdin: bool,
        /// New directory; an existing directory is never overwritten.
        #[arg(long)]
        output_dir: PathBuf,
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
        TokenizerRegistrationCmd::Prepare { open_body, raw_tokenizer, eog_opening, key_file, key_stdin, output_dir } => {
            prepare(ctx, &open_body, &raw_tokenizer, &eog_opening, crate::keys::KeySource { key_file, key_stdin }, &output_dir)
                .await?;
        }
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

fn read_bounded(path: &std::path::Path, maximum: u64) -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(maximum + 1).read_to_end(&mut bytes))
        .map_err(|e| input_error(format!("{}: {e}", path.display())))?;
    if bytes.len() as u64 > maximum {
        return Err(input_error(format!("{} exceeds {maximum} bytes", path.display())));
    }
    Ok(bytes)
}

fn decode_fixed_hex<T: borsh::BorshDeserialize>(value: &serde_json::Value, name: &str) -> Result<T, CliError> {
    let hex = value.as_str().ok_or_else(|| input_error(format!("quote omitted {name}")))?;
    if hex.len() > 4096 || hex.len() % 2 != 0 {
        return Err(input_error(format!("invalid {name} length")));
    }
    let mut bytes = vec![0; hex.len() / 2];
    faster_hex::hex_decode(hex.as_bytes(), &mut bytes).map_err(|_| input_error(format!("invalid {name} hex")))?;
    borsh::from_slice(&bytes).map_err(|_| input_error(format!("invalid {name} encoding")))
}

async fn prepare(
    ctx: &Ctx,
    body_path: &std::path::Path,
    raw_path: &std::path::Path,
    eog_path: &std::path::Path,
    key_source: crate::keys::KeySource,
    output: &std::path::Path,
) -> CliResult {
    use kaspa_consensus_core::palw_tokenizer_registration_v1::{
        TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1, TokenizerRegistrationPolicyV1, TokenizerRegistrationQuoteV1,
    };
    use misaka_palw_sdk::tokenizer_registration::{TokenizerRegistrationPlanInputV1, prepare_tokenizer_registration_v1};
    let body_hex = open_body_hex(body_path)?;
    let open: TokenizerRegistrationOpenBodyV1 = decode_fixed_hex(&serde_json::json!(body_hex), "Open body")?;
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader
        .client
        .get_palw_tokenizer_registration_quote(GetPalwTokenizerRegistrationQuoteRequest { open_body_hex: body_hex })
        .await;
    let _ = reader.client.disconnect().await;
    let reply = answer.map_err(|e| CliError::connection(format!("tokenizer quote RPC: {e}")))?;
    if !reply.available || !reply.quoted || !reply.refusal.is_empty() {
        return Err(input_error(format!("Open cannot be quoted: {}", reply.refusal)));
    }
    if reply.observation_json.len() > kaspa_rpc_core::TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1 {
        return Err(input_error("tokenizer quote exceeds its observation bound"));
    }
    let view: serde_json::Value = serde_json::from_str(&reply.observation_json).map_err(|e| input_error(e.to_string()))?;
    let policy: TokenizerRegistrationPolicyV1 = decode_fixed_hex(&view["policyHex"], "policy")?;
    let quote: TokenizerRegistrationQuoteV1 = decode_fixed_hex(&view["quote"]["quoteHex"], "quote")?;
    let id = open.id_v1().map_err(input_error)?;
    if view["schema"] != 1
        || view["openId"] != id.to_string()
        || view["signingDigest"] != id.to_string()
        || view["policyId"] != open.policy.to_string()
        || policy.id_v1().map_err(input_error)? != open.policy
        || view["signingContextHex"] != faster_hex::hex_string(TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1)
        || view["slotsReserved"] != false
        || view["fundingVerified"] != false
    {
        return Err(input_error("the quote substituted the Open, policy or signing scope"));
    }
    let number = |field: &str| view[field].as_u64().ok_or_else(|| input_error(format!("quote omitted {field}")));
    let proposed_accepting_daa = number("prospectiveDaa")?;
    let declared_key_events = number("declaredKeyEvents")?;
    if policy.quote_v1(&open, proposed_accepting_daa, declared_key_events).map_err(input_error)? != quote {
        return Err(input_error("the node quote differs from the pinned native fee calculation"));
    }
    let raw = read_bounded(raw_path, policy.max_raw_bytes)?;
    // EOG IDs plus one Merkle opening; the native bounded decoder below is
    // reused from Finish so a forged vector prefix cannot allocate first.
    let eog_bytes = read_bounded(eog_path, 4 * u64::from(policy.max_eog_ids) + 4096)?;
    let mut finish_bytes = borsh::to_vec(&kaspa_consensus_core::palw_tokenizer_registration_v1::TokenizerRegistrationFinishBodyV1 {
        version: open.version,
        network: open.network,
        policy: open.policy,
        open_id: id,
        opener: open.opener,
        eog_digest: open.eog_digest,
        finish_until_daa: open.finish_until_daa,
    })
    .map_err(|e| input_error(e.to_string()))?;
    finish_bytes.resize(finish_bytes.len() + kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN, 0);
    finish_bytes.extend_from_slice(&eog_bytes);
    let finish: kaspa_consensus_core::palw_tokenizer_registration_v1::TokenizerRegistrationFinishV1 =
        borsh::from_slice(&finish_bytes).map_err(|e| input_error(format!("invalid bounded EOG opening: {e}")))?;
    finish.check_open_v1(&open, &policy).map_err(input_error)?;
    let bond = format!("{}:{}", open.opener.0.transaction_id, open.opener.0.index);
    let (nv, key, _) = crate::palw_user::bond_signer_v1(ctx, &key_source, &bond).await?;
    let _ = nv.client.disconnect().await;
    let prepared = prepare_tokenizer_registration_v1(
        TokenizerRegistrationPlanInputV1 {
            open,
            policy: &policy,
            raw: &raw,
            eog: finish.eog,
            opener_public_key: key.public_key(),
            proposed_accepting_daa,
            declared_key_events,
        },
        &key,
    )
    .map_err(input_error)?;
    std::fs::create_dir(output).map_err(|e| input_error(format!("cannot create {}: {e}", output.display())))?;
    let mut created = Vec::new();
    let mut write = |name: &str, bytes: &[u8]| -> CliResult {
        use std::io::Write;
        let path = output.join(name);
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| input_error(e.to_string()))?;
        created.push(path);
        file.write_all(bytes).map_err(|e| input_error(e.to_string()))?;
        Ok(())
    };
    let written = (|| -> CliResult {
        write("open.tkr1", &prepared.open_payload)?;
        for (index, bytes) in prepared.indexed_part_payloads.iter().enumerate() {
            write(&format!("part-{index:04}.tkr1"), bytes)?;
        }
        write("finish.tkr1", &prepared.finish_payload)?;
        write("quote.json", reply.observation_json.as_bytes())?;
        Ok(())
    })();
    if let Err(error) = written {
        // Remove only this invocation's files. Never recursively remove a
        // directory that could have acquired unrelated files in the meantime.
        for path in created {
            let _ = std::fs::remove_file(path);
        }
        let _ = std::fs::remove_dir(output);
        return Err(input_error(format!(
            "bundle write failed at {}: {}; any residual directory is incomplete",
            output.display(),
            error.msg
        )));
    }
    println!(
        "{}",
        serde_json::json!({"openId": id.to_string(), "directory": output, "parts": prepared.indexed_part_payloads.len(),
        "registrationChargeSompi": quote.charge, "submitted": false, "slotsReserved": false})
    );
    Ok(())
}
