//! **`misaka palw panel-da …` (ADR-0206, lane panelda-ux): the PanelDa disclosure duty, from the user's side.**
//!
//! A PanelDa text claim accepted past `palw_panel_da_disclosure_v1` must have its prompt ids published by `a + 1,040` DAA or it
//! defaults (claim void, deposit burned; never a conviction). The duty holder — the producer, or the poster who holds the ids — files:
//!
//! * `palw panel-da status <CLAIM>` — op 249 `getPalwPanelDaDisclosure`: deadline, remaining DAA, disclosed or not, the deposit
//!   (locked / returned / burned) and the void reason. Read-only.
//! * `palw panel-da disclose --claim <C> --ids-file <F> --key-file <K>` — builds the tag-233 object (no signature: the claim's hash
//!   fixes the ids), checks it against the node's row (count, hash, deadline) and carries it on a lifecycle transaction funded by the
//!   key. A dry-run preview unless `--submit`.
//!
//! Nothing here prints a prompt id.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_panel_da_disclosure_v1::palw_panel_da_disclosure_object_v1;
use kaspa_rpc_core::GetPalwPanelDaDisclosureRequest;
use kaspa_rpc_core::api::rpc::RpcApi;
use serde_json::Value;

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

pub(crate) const PALW_PANEL_DA_CLI_SCHEMA: &str = "misaka.palw.panel-da.v1";

#[derive(clap::Subcommand, Debug)]
pub(crate) enum PanelDaCmd {
    /// **A PanelDa claim's disclosure duty** (`getPalwPanelDaDisclosure`, op 249): deadline, remaining DAA, disclosed, deposit
    /// (locked / returned / burned), void reason. Read-only.
    Status {
        /// 128-hex claim id.
        #[arg(value_name = "CLAIM_ID")]
        claim: String,
        #[arg(long)]
        json: bool,
    },
    /// **Disclose a PanelDa claim's prompt ids** (tag 233) before `a + 1,040`. Dry-run unless `--submit`.
    Disclose {
        #[command(flatten)]
        key: crate::KeyArgs,
        /// 128-hex claim id.
        #[arg(long)]
        claim: String,
        /// The prompt token ids: a file of decimal ids separated by whitespace or commas (a JSON array also parses).
        #[arg(long)]
        ids_file: std::path::PathBuf,
        /// Where to keep the built object (default: `<claim>.panel-da.borsh` in the current directory).
        #[arg(long)]
        out: Option<std::path::PathBuf>,
        /// Build and file it even if the node cannot confirm the claim's row (offline drill).
        #[arg(long)]
        skip_node_check: bool,
        /// Actually broadcast (otherwise a dry-run preview). `--yes` is the same flag.
        #[arg(long, alias = "yes")]
        submit: bool,
    },
}

/// Parse a file's prompt ids: decimal u32s separated by whitespace, commas or the brackets of a JSON array.
pub(crate) fn parse_ids(text: &str) -> Result<Vec<u32>, CliError> {
    let ids: Result<Vec<u32>, _> =
        text.split(|c: char| c.is_whitespace() || c == ',' || c == '[' || c == ']').filter(|t| !t.is_empty()).map(str::parse).collect();
    let ids = ids.map_err(|_| CliError::new(exit::GENERIC, "--ids-file: every id must be a decimal u32".to_string()))?;
    if ids.is_empty() {
        return Err(CliError::new(exit::GENERIC, "--ids-file holds no ids".to_string()));
    }
    Ok(ids)
}

fn parse_claim(s: &str) -> Result<Hash64, CliError> {
    s.trim().parse::<Hash64>().map_err(|_| CliError::new(exit::GENERIC, format!("claim '{s}' is not 128 hex digits")))
}

/// The local checks against the node's document: governed, not yet disclosed, inside the deadline, same count and hash.
pub(crate) fn check_against_view(doc: &Value, ids: &[u32]) -> Result<(), CliError> {
    let refuse = |why: String| Err(CliError::new(exit::UNSAFE_REFUSED, why));
    if doc["governed"] != Value::Bool(true) || doc["deadlineDaa"].is_null() {
        return refuse("the claim has no PanelDa disclosure row (not governed, retired, or already defaulted)".into());
    }
    if doc["disclosed"] == Value::Bool(true) {
        return refuse("already disclosed — a second disclosure is refused on chain".into());
    }
    let (now, deadline) = (doc["nowDaa"].as_u64().unwrap_or(u64::MAX), doc["deadlineDaa"].as_u64().unwrap_or(0));
    if now > deadline {
        return refuse(format!("past the deadline (DAA {deadline}, now {now}): the chain refuses a late disclosure"));
    }
    if doc["promptTokens"].as_u64() != Some(ids.len() as u64) {
        return refuse(format!("the file holds {} ids; the claim committed {}", ids.len(), doc["promptTokens"]));
    }
    let hash = doc["promptTokenIdsHash"].as_str().unwrap_or_default().parse::<Hash64>().ok();
    let binds = hash.is_some_and(|h| {
        use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1 as F, prompt_token_ids_match_v1};
        prompt_token_ids_match_v1(F::Flat, ids, &h) || prompt_token_ids_match_v1(F::MerkleV1, ids, &h)
    });
    if !binds {
        return refuse("these ids do not hash to the claim's prompt_token_ids_hash — the chain would refuse them".into());
    }
    Ok(())
}

pub(crate) fn render_status(doc: &Value) -> String {
    let t = |v: &Value| match v {
        Value::Null => "-".to_string(),
        Value::String(s) => s.clone(),
        o => o.to_string(),
    };
    format!(
        "claim        {}\ngoverned     {}\naccepted     DAA {}\ndeadline     DAA {} (remaining {})\ndisclosed    {} (at DAA {})\n\
         deposit      {} ({} sompi)\nphase        {}\nvoid reason  {}\n",
        t(&doc["claim"]),
        t(&doc["governed"]),
        t(&doc["acceptedDaa"]),
        t(&doc["deadlineDaa"]),
        t(&doc["remainingDaa"]),
        t(&doc["disclosed"]),
        t(&doc["disclosedDaa"]),
        t(&doc["deposit"]),
        t(&doc["depositSompi"]),
        t(&doc["claimPhase"]),
        t(&doc["voidReason"]),
    )
}

async fn read_view(ctx: &Ctx, claim: Hash64) -> Result<Value, CliError> {
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_panel_da_disclosure(GetPalwPanelDaDisclosureRequest { claim: claim.to_string() }).await;
    let _ = reader.client.disconnect().await;
    let r = answer.map_err(|e| {
        CliError::new(exit::GENERIC, format!("getPalwPanelDaDisclosure (op 249): {e} — a node built before op 249 drops the connection"))
    })?;
    if !r.available {
        return Err(CliError::new(exit::GENERIC, "getPalwPanelDaDisclosure: the node holds no PALW ConsensusV2 state".to_string()));
    }
    if !r.found {
        return Err(CliError::new(exit::GENERIC, format!("getPalwPanelDaDisclosure: the chain holds no claim {claim}")));
    }
    let doc: Value = serde_json::from_str(&r.view_json)
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwPanelDaDisclosure: bad document: {e}")))?;
    match doc["version"].as_u64() {
        Some(1) => Ok(doc),
        other => Err(CliError::new(exit::GENERIC, format!("getPalwPanelDaDisclosure: document version {other:?} is newer than this CLI"))),
    }
}

pub(crate) async fn run(ctx: &Ctx, cmd: PanelDaCmd) -> CliResult {
    match cmd {
        PanelDaCmd::Status { claim, json } => {
            let mut doc = read_view(ctx, parse_claim(&claim)?).await?;
            if json || ctx.output == OutputFormat::Json {
                doc["schema"] = PALW_PANEL_DA_CLI_SCHEMA.into();
                println!("{}", serde_json::to_string_pretty(&doc).expect("a JSON value serializes"));
            } else {
                print!("{}", render_status(&doc));
            }
            Ok(())
        }
        PanelDaCmd::Disclose { key, claim, ids_file, out, skip_node_check, submit } => {
            let claim = parse_claim(&claim)?;
            let text = std::fs::read_to_string(&ids_file)
                .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", ids_file.display())))?;
            let ids = parse_ids(&text)?;
            if !skip_node_check {
                check_against_view(&read_view(ctx, claim).await?, &ids)?;
            }
            let object = palw_panel_da_disclosure_object_v1(claim, ids);
            kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
                .map_err(|why| CliError::new(exit::GENERIC, format!("the object cannot ride a carrier: {why}")))?;
            let bytes = borsh::to_vec(&object).map_err(|e| CliError::new(exit::GENERIC, format!("cannot serialize: {e}")))?;
            let out = out.unwrap_or_else(|| std::path::PathBuf::from(format!("{claim}.panel-da.borsh")));
            std::fs::write(&out, &bytes).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", out.display())))?;
            println!("PanelDaPromptDisclosedV1 (tag 233): claim {claim}, {} bytes -> {}", bytes.len(), out.display());
            crate::palw_fp::submit_objects_v2_capped(ctx, &key.source(), std::slice::from_ref(&out), submit, false, None)
                .await
                .map(|_| ())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(clap::Parser, Debug)]
    struct T {
        #[command(subcommand)]
        cmd: PanelDaCmd,
    }

    #[test]
    fn the_subcommands_parse() {
        let c = "ab".repeat(64);
        assert!(matches!(T::try_parse_from(["t", "status", &c, "--json"]).unwrap().cmd, PanelDaCmd::Status { json: true, .. }));
        let d = T::try_parse_from(["t", "disclose", "--claim", &c, "--ids-file", "ids.txt", "--key-file", "k"]).unwrap();
        assert!(matches!(d.cmd, PanelDaCmd::Disclose { submit: false, .. }), "dry-run by default");
        let d = T::try_parse_from(["t", "disclose", "--claim", &c, "--ids-file", "ids.txt", "--key-file", "k", "--submit"]).unwrap();
        assert!(matches!(d.cmd, PanelDaCmd::Disclose { submit: true, .. }));
        assert!(T::try_parse_from(["t", "disclose", "--claim", &c]).is_err(), "--ids-file is required");
    }

    #[test]
    fn ids_parse_from_plain_csv_and_json() {
        assert_eq!(parse_ids("1 2\n3").unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_ids("[4,5, 6]").unwrap(), vec![4, 5, 6]);
        assert!(parse_ids("").is_err());
        assert!(parse_ids("1 x").is_err());
    }

    #[test]
    fn the_node_check_refuses_what_the_chain_would_refuse() {
        let ids = vec![7u32, 8, 9];
        let hash = kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&ids);
        let doc = |now: u64, disclosed: bool| {
            serde_json::json!({ "version": 1, "governed": true, "deadlineDaa": 1_140, "nowDaa": now, "disclosed": disclosed,
                "promptTokens": 3, "promptTokenIdsHash": hash.to_string() })
        };
        check_against_view(&doc(500, false), &ids).expect("binds, in time");
        assert!(check_against_view(&doc(1_141, false), &ids).is_err(), "late");
        assert!(check_against_view(&doc(500, true), &ids).is_err(), "already disclosed");
        assert!(check_against_view(&doc(500, false), &[7, 8, 10]).is_err(), "other ids");
        assert!(check_against_view(&doc(500, false), &[7, 8]).is_err(), "other count");
        let built = borsh::to_vec(&palw_panel_da_disclosure_object_v1(Hash64::default(), ids)).unwrap();
        assert_eq!(built[0], 233, "tag 233, as the fold decodes it");
    }
}
