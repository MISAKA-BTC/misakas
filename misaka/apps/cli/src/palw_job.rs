//! **`misaka palw job …` / `misaka palw classes` (lane J1): post a kernel job as text and read its result back as text.**
//!
//! * `palw classes` — op 227 view `kernel-classes`: class id, artifact root, tier, attested, token bound, positions.
//! * `palw job post --class <C> --prompt <TEXT> --max-new <N> --fee <SOMPI> --tokenizer <FILE> --bond <B> --key-file <K> [--yes]` —
//!   tokenises with the user's tokenizer, builds and signs `KernelRouteObjectV1::PostJob` (the escrow is opened by the ledger at
//!   posting), and files it through `palw submit-object`'s path. Without `--yes` a dry run: the quote, the job id, the escrow.
//!   `--fee` is the most of the poster bond's free collateral the user lets the job take (escrow + check fees + burned job fee); the
//!   amounts themselves are the route policy's, not chosen by the poster.
//! * `palw job status <JOB>` — op 227 view `kernel-job`.
//! * `palw job result <JOB> [--tokenizer <FILE>]` — op 227 view `kernel-result`, rendered to text with the tokenizer; `provisional`
//!   until the claim is SecurityCertified, `certified` after.
//!
//! A kernel class commits to no tokenizer, so the tokenizer file is the user's; its digest is printed, never checked against the
//! chain (there is nothing to check it against) and never reviewed on chain.

use kaspa_consensus_core::Hash64;
use misaka_palw_kernel::route::KernelRouteObjectV1 as KernelObj;
use misaka_palw_sdk::kernel_job_v1::{kernel_job_from_text_v1, kernel_result_text_v1, kernel_tokenizer_v1};
use serde_json::Value;

use crate::node::Ctx;
use crate::palw_user::{emit, parse_hash, parse_view, text};
use crate::{CliError, CliResult, exit};

/// The typed refusal `job post` gives below the kernel route's fence.
pub(crate) const KERNEL_ROUTE_NOT_ARMED: &str = "KERNEL_ROUTE_NOT_ARMED";

#[derive(clap::Subcommand, Debug)]
pub(crate) enum JobCmd {
    /// **Post a kernel job from text.** Dry run unless `--yes`.
    Post {
        #[command(flatten)]
        key: crate::KeyArgs,
        /// The poster's bond outpoint `<txid>:<index>` (its free collateral funds the escrow).
        #[arg(long)]
        bond: String,
        /// 128-hex kernel class id (`misaka palw classes`).
        #[arg(long)]
        class: String,
        /// The prompt text.
        #[arg(long)]
        prompt: String,
        /// Exactly this many generated tokens.
        #[arg(long)]
        max_new: u32,
        /// The most sompi of the bond's free collateral this job may take (escrow + check fees + job fee); refused above it.
        #[arg(long)]
        fee: u64,
        /// The class's `tokenizer.json` (a kernel class commits to none: the file is yours).
        #[arg(long)]
        tokenizer: std::path::PathBuf,
        /// 128-hex requester nonce (derived from the bond and the DAA when omitted).
        #[arg(long)]
        nonce: Option<String>,
        /// Sign and submit (otherwise a dry run).
        #[arg(long)]
        yes: bool,
    },
    /// **A job's status**: pending / claimed / final / security-certified / settled / defaulted, and its escrow.
    Status {
        #[arg(value_name = "JOB_ID")]
        job: String,
        #[arg(long)]
        json: bool,
    },
    /// **A job's result as text** (token ids without `--tokenizer`): provisional before SecurityCertified, certified after.
    Result {
        #[arg(value_name = "JOB_ID")]
        job: String,
        #[arg(long)]
        tokenizer: Option<std::path::PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

async fn view(ctx: &Ctx, kind: &str, key: String, what: &str) -> Result<Value, CliError> {
    let (available, found, doc) = crate::palw_status::status_view(ctx, kind, key).await?;
    parse_view("getPalwFenceSchedule", available, found, &doc, what)
}

fn read_file(p: &std::path::Path) -> Result<Vec<u8>, CliError> {
    std::fs::read(p).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", p.display())))
}

pub(crate) async fn classes(ctx: &Ctx, json: bool) -> CliResult {
    let doc = view(ctx, "kernel-classes", String::new(), "kernel route").await?;
    emit(ctx, json, doc, |d| {
        let mut out = format!("{} kernel classes (ledger DAA {})\n", text(&d["classCount"]), text(&d["ledgerDaa"]));
        for c in d["classes"].as_array().into_iter().flatten() {
            out += &format!(
                "  {}  tier {}  attested {}  tokens<{}  positions {}\n",
                text(&c["classId"]),
                text(&c["tier"]),
                text(&c["attested"]),
                text(&c["tokenBound"]),
                text(&c["maxPositions"])
            );
        }
        out += &format!(
            "  job quote: escrow {} + job fee {} sompi (route policy)\n",
            text(&d["jobQuote"]["rewardEscrowSompi"]),
            text(&d["jobQuote"]["jobFeeBurnedSompi"])
        );
        out
    });
    Ok(())
}

pub(crate) async fn run(ctx: &Ctx, cmd: JobCmd) -> CliResult {
    match cmd {
        JobCmd::Post { key, bond, class, prompt, max_new, fee, tokenizer, nonce, yes } => {
            post(ctx, key, bond, class, prompt, max_new, fee, tokenizer, nonce, yes).await
        }
        JobCmd::Status { job, json } => {
            let job = parse_hash("JOB_ID", &job)?;
            let doc = view(ctx, "kernel-job", job.to_string(), "such kernel job").await?;
            emit(ctx, json, doc, |d| {
                let mut out = format!("job {}  class {}\n  status {}\n", text(&d["jobId"]), text(&d["classId"]), text(&d["status"]));
                if d["claim"].is_object() {
                    let c = &d["claim"];
                    out += &format!(
                        "  claim {}  state {}  SecurityCertified {}  held reward {}\n",
                        text(&c["claimId"]),
                        text(&c["state"]),
                        text(&c["securityCertified"]),
                        text(&c["heldRewardSompi"])
                    );
                }
                let e = &d["escrow"];
                out += &format!("  escrow {}  {}\n", text(&e["state"]), e.to_string());
                out
            });
            Ok(())
        }
        JobCmd::Result { job, tokenizer, json } => {
            let job = parse_hash("JOB_ID", &job)?;
            let mut doc = view(ctx, "kernel-result", job.to_string(), "such kernel job").await?;
            if let (Some(path), Some(tokens)) = (tokenizer, doc["tokens"].as_array()) {
                let ids: Vec<u32> = tokens.iter().filter_map(|t| t.as_u64()).map(|t| t as u32).collect();
                let t = kernel_tokenizer_v1(&read_file(&path)?, None).map_err(|e| CliError::new(exit::GENERIC, e))?;
                let rendered = kernel_result_text_v1(&t.tokenizer, &ids).map_err(|e| CliError::new(exit::GENERIC, e))?;
                doc["text"] = rendered.into();
                doc["tokenizerDigest"] = t.digest.to_string().into();
            }
            emit(ctx, json, doc, |d| {
                let standing = text(&d["standing"]);
                let label = match standing.as_str() {
                    "certified" => "CERTIFIED (SecurityCertified)",
                    "provisional" => "PROVISIONAL (not SecurityCertified yet)",
                    "void" => "VOID (the claim failed)",
                    _ => "no claim yet",
                };
                let mut out = format!("job {}  {label}\n", text(&d["jobId"]));
                match d.get("text").and_then(|t| t.as_str()) {
                    Some(t) => out += &format!("{t}\n"),
                    None => out += &format!("  tokens {}\n", d["tokens"]),
                }
                out
            });
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn post(
    ctx: &Ctx,
    key: crate::KeyArgs,
    bond: String,
    class: String,
    prompt: String,
    max_new: u32,
    fee: u64,
    tokenizer: std::path::PathBuf,
    nonce: Option<String>,
    yes: bool,
) -> CliResult {
    let class = parse_hash("--class", &class)?;
    let nonce = nonce.map(|n| parse_hash("--nonce", &n)).transpose()?;
    let tok = kernel_tokenizer_v1(&read_file(&tokenizer)?, None).map_err(|e| CliError::new(exit::GENERIC, e))?;
    let (nv, signer, poster) = crate::palw_user::bond_signer_v1(ctx, &key.source(), &bond).await?;
    let armed = nv.params.palw_probabilistic_constraints_v1.is_some_and(|a| a.is_active(nv.virtual_daa));
    if !armed {
        return Err(CliError::new(
            exit::UNSAFE_REFUSED,
            format!(
                "{KERNEL_ROUTE_NOT_ARMED}: palw_probabilistic_constraints_v1 (the kernel route) is not armed on {} at DAA {} — \
                 the chain drops a PostJob below it",
                nv.params.net, nv.virtual_daa
            ),
        ));
    }
    let classes = view(ctx, "kernel-classes", String::new(), "kernel route").await?;
    let row = classes["classes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["classId"] == class.to_string())
        .cloned()
        .ok_or_else(|| CliError::new(exit::GENERIC, format!("the kernel route lists no class {class} (`misaka palw classes`)")))?;
    let bound = row["tokenBound"].as_u64().unwrap_or(0) as u32;
    let positions = row["maxPositions"].as_u64().unwrap_or(0) as u32;
    let nonce = nonce.unwrap_or_else(|| {
        let mut seed = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&poster).to_vec();
        seed.extend_from_slice(&nv.virtual_daa.to_le_bytes());
        seed.extend_from_slice(prompt.as_bytes());
        Hash64::from_bytes(misaka_palw_kernel::hash::id(b"misaka-palw/cli/job-post/nonce/v1", &seed))
    });
    let job = kernel_job_from_text_v1(&tok.tokenizer, &prompt, class, bound, positions, max_new, nonce)
        .map_err(|e| CliError::new(exit::GENERIC, e))?;
    let verifier_pay = nv.params.palw_verifier_pay_v1.filter(|a| a.is_active(nv.virtual_daa)).map(|a| a.daa_score());
    let quote = crate::palw_user::kernel_job_quote_v1(verifier_pay);
    let job_id = Hash64::from_bytes(job.id());
    println!(
        "PostJob: job {job_id}, class {class} (tier {}), {} prompt tokens, {max_new} new tokens",
        text(&row["tier"]),
        job.prompt.len()
    );
    println!("tokenizer {} digest {} (the class commits to no tokenizer: used as given)", tokenizer.display(), tok.digest);
    println!("{}", quote.render());
    if quote.free_collateral_needed() > fee {
        return Err(CliError::new(
            exit::UNSAFE_REFUSED,
            format!("refused: the job needs {} sompi, above --fee {fee}", quote.free_collateral_needed()),
        ));
    }
    let object =
        crate::palw_user::kernel_route_object_v1(crate::bond::network_domain(&nv), poster, &KernelObj::PostJob { job }, &signer);
    if !yes {
        println!("dry run: nothing signed into a file or submitted (add --yes)");
        return Ok(());
    }
    let path = std::env::temp_dir().join(format!("misaka-job-{}.borsh", &job_id.to_string()[..16]));
    crate::palw_user::write_object(&path, &object)?;
    crate::palw_fp::submit_objects_v2_capped(ctx, &key.source(), &[path], true, false, None).await?;
    println!("posted: misaka palw job status {job_id}");
    Ok(())
}
