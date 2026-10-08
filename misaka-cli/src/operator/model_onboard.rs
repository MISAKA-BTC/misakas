//! **`misaka model onboard …`** (G14 onboarding P0): the signed registration envelope (tag 108) with detached signing, and the
//! chain's conformance record read back and re-verified from public material.
//!
//! ```text
//! envelope-export   registration object + bond + validity window  ── no key, no node ──►  request.json (UNSIGNED)
//!                   (the fork digest is THIS build's `fork_id_v1(params, valid_from).fired`, never a node's answer)
//! envelope-sign     request.json, re-derived and checked against THIS build's network and fork digest  ── the key ──►  envelope.borsh
//!                   (file it: `misaka palw submit-object --object envelope.borsh`)
//! status            op 231: the lifecycle state, the attempt, the beacon, the posted evidence, the gate
//! verify            ops 231 + 212 (+ the artifact): the fresh verifier rebuilds the verdict and says whether it agrees with the chain
//! ```
//!
//! The seed is read only by `envelope-sign`, from `--key-file` / `--key-stdin` (never an argument, never the environment), and only
//! through the SDK's `EnvelopeSigner`; it is never printed or stored.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwConformanceEvidenceRequest, GetPalwKernelFinalsRequest};
use misaka_palw_sdk::onboarding_chain::{
    EnvelopeSigner, PublicConformanceReadsV1, SignedRegistrationRequestV1, fresh_verify_from_reads_v1, verify_signed_registration_v1,
};
use std::path::Path;
use std::str::FromStr;

/// The chain this CLI signs for: this build's own compiled parameters for `--network` (never a node's or a file's answer).
fn params_of(ctx: &Ctx) -> Result<kaspa_consensus_core::config::params::Params, CliError> {
    let net = kaspa_consensus_core::network::NetworkId::from_str(&ctx.network)
        .map_err(|e| CliError::new(exit::CONFIG, format!("'{}' is not a network id: {e}", ctx.network)))?;
    Ok(crate::wallet::chain_params(ctx, net)?.0)
}

fn parse_bond(text: &str) -> Result<PalwBondKeyV2, CliError> {
    let (txid, index) = text.split_once(':').ok_or_else(|| CliError::new(exit::CONFIG, format!("bond {text}: not <txid>:<index>")))?;
    Ok(PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        txid.parse().map_err(|_| CliError::new(exit::CONFIG, format!("bond {text}: not a transaction id")))?,
        index.parse().map_err(|_| CliError::new(exit::CONFIG, format!("bond {text}: not an index")))?,
    )))
}

fn refusal(e: misaka_palw_sdk::runtime_pack::commit::Refusal) -> CliError {
    CliError::new(exit::GENERIC, e.to_string())
}

/// **Step 1 (no key, no node):** wrap a registration object (Borsh, as `misaka palw tir-registration` writes it) in an UNSIGNED
/// tag-108 request for `bond`, valid in `[valid_from_daa, valid_until_daa]` while the chain's fork digest is the one this build
/// computes at `valid_from_daa` (F-C4R3-01(b)).
pub(crate) fn envelope_export(
    ctx: &Ctx,
    registration: &Path,
    bond: &str,
    valid_from_daa: u64,
    valid_until_daa: u64,
    out: &Path,
) -> CliResult {
    let bytes = std::fs::read(registration).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", registration.display())))?;
    let object: PalwConsensusObjectV2 = borsh::from_slice(&bytes)
        .map_err(|e| CliError::new(exit::GENERIC, format!("{}: not a consensus object: {e}", registration.display())))?;
    let params = params_of(ctx)?;
    let request = SignedRegistrationRequestV1::for_network(object, parse_bond(bond)?, valid_from_daa, valid_until_daa, &params)
        .map_err(refusal)?;
    let text = serde_json::to_string_pretty(&request.to_json()).expect("json serializes");
    std::fs::write(out, text).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", out.display())))?;
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({"schema": "misaka.palw.onboard.envelope-export.v1", "out": out.display().to_string(), "signed": false})
        ),
        _ => {
            println!("wrote {} (UNSIGNED tag-108 request, valid in DAA [{valid_from_daa}, {valid_until_daa}])", out.display());
            println!(
                "sign it where the key is: misaka model onboard envelope-sign --request {} --key-file <0600 file> --out <envelope>",
                out.display()
            );
        }
    }
    Ok(())
}

struct KeySigner(kaspa_pq_validator_core::ValidatorKey);

impl EnvelopeSigner for KeySigner {
    fn public_key(&self) -> Vec<u8> {
        self.0.public_key().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign_with_context(message, context).to_vec())
    }
}

/// **Step 2 (the key, no node):** re-read the request, refuse it unless it is for THIS build's network and fork digest at its
/// `valid_from_daa` and (when given) for `expect_bond`, re-derive the message from its fields, sign, verify, and write the envelope
/// object (Borsh) for `misaka palw submit-object`.
pub(crate) fn envelope_sign(
    ctx: &Ctx,
    request: &Path,
    key: &crate::keys::KeySource,
    expect_bond: Option<&str>,
    out: &Path,
) -> CliResult {
    let text = std::fs::read_to_string(request).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", request.display())))?;
    let req = SignedRegistrationRequestV1::from_json(&text).map_err(refusal)?;
    let params = params_of(ctx)?;
    if let Err(e) = req.check_for_network(&params) {
        return Err(CliError::new(exit::NETWORK_MISMATCH, format!("{e} — for {} in this build: nothing was signed", ctx.network)));
    }
    if let Some(expect) = expect_bond
        && parse_bond(expect)? != req.signer
    {
        return Err(CliError::new(exit::GENERIC, "the request names another signer bond than --expect-bond: nothing was signed"));
    }
    let signer = KeySigner(key.load_key()?);
    let envelope = req.sign(&signer).map_err(refusal)?;
    verify_signed_registration_v1(&envelope, req.network_domain, &signer.public_key()).map_err(refusal)?;
    let bytes = borsh::to_vec(&envelope).expect("an object serializes");
    std::fs::write(out, &bytes).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", out.display())))?;
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({"schema": "misaka.palw.onboard.envelope-sign.v1", "out": out.display().to_string(), "bytes": bytes.len(),
                "valid_until_daa": req.valid_until_daa, "message": req.message().to_string()})
        ),
        _ => {
            println!(
                "wrote {} ({} bytes): SignedRegistrationV1 (tag 108), valid through DAA {}",
                out.display(),
                bytes.len(),
                req.valid_until_daa
            );
            println!("file it with: misaka palw submit-object --object {} --yes", out.display());
        }
    }
    Ok(())
}

fn unhex(s: &str) -> Result<Vec<u8>, CliError> {
    if s.len() % 2 != 0 {
        return Err(CliError::new(exit::GENERIC, "the node served odd-length hex"));
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| CliError::new(exit::GENERIC, "the node served non-hex")))
        .collect()
}

/// **`status`**: op 231 as the chain states it.
pub(crate) async fn status(ctx: &Ctx, class: &str) -> CliResult {
    let nv = crate::wallet::connect(ctx).await?;
    let r = nv
        .client
        .get_palw_conformance_evidence(GetPalwConformanceEvidenceRequest { class_id: class.to_string() })
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwConformanceEvidence: {e}")))?;
    if ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&r).expect("json serializes"));
        return Ok(());
    }
    if !r.found {
        println!("class {class}: not found (available: {})", r.available);
        return Ok(());
    }
    println!("class            {}", r.class_id);
    println!(
        "lifecycle        {}  (last failure: {}; last end: {})",
        r.lifecycle_state,
        or_dash(&r.last_failure),
        or_dash(&r.attempt_end)
    );
    println!("attempts         {} of {}", r.attempts, r.attempt_limit);
    println!("commitment       {} at DAA {} (epoch {})", or_dash(&r.statement_root), r.committed_daa, r.challenge_epoch);
    println!(
        "beacon           {} {}/{} lock {} output {}",
        or_dash(&r.beacon_state),
        r.beacon_have,
        r.beacon_need,
        r.lock_position,
        or_dash(&r.beacon_output)
    );
    if r.evidence_posted {
        println!("evidence         {} posted at DAA {}, refutable until DAA {}", r.evidence_id, r.evidence_daa, r.window_end_daa);
    }
    println!("gate             {} {} — {}", r.gate, r.gate_code, r.gate_reason);
    println!("verify it yourself: misaka model onboard verify --class {class} [--artifact <PALWTIR1 file>]");
    Ok(())
}

fn or_dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

/// **`verify`**: the fresh verifier over ops 231 and 212 (and the public artifact, when given).
pub(crate) async fn verify(ctx: &Ctx, class: &str, artifact: Option<&Path>) -> CliResult {
    let nv = crate::wallet::connect(ctx).await?;
    let r = nv
        .client
        .get_palw_conformance_evidence(GetPalwConformanceEvidenceRequest { class_id: class.to_string() })
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwConformanceEvidence: {e}")))?;
    if !r.found || r.attempt_row.is_empty() {
        return Err(CliError::new(exit::NOT_READY, format!("class {class}: no conformance record on this node")));
    }
    let finals = nv
        .client
        .get_palw_kernel_finals(GetPalwKernelFinalsRequest { limit: 0 })
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwKernelFinals: {e}")))?;
    let mut events = Vec::new();
    for f in &finals.finals {
        if !f.work_final_event.is_empty() {
            events.push(
                // An attributed event (the event and its producer): the type is the reads' (inferred).
                borsh::from_slice(&unhex(&f.work_final_event)?)
                    .map_err(|e| CliError::new(exit::GENERIC, format!("a Final's beacon event does not decode: {e}")))?,
            );
        }
    }
    let reads = PublicConformanceReadsV1 {
        attempt_row: unhex(&r.attempt_row)?,
        evidence_row: if r.evidence_row.is_empty() { None } else { Some(unhex(&r.evidence_row)?) },
        events,
        tip_daa: r.tip_daa,
        program: unhex(&r.program)?,
    };
    let report = fresh_verify_from_reads_v1(&reads, artifact).map_err(refusal)?;
    let v = &report.verdict;
    let posted = match &v.posted {
        None => "none".to_string(),
        Some(Ok(())) => "bound, rebuilt exactly, a pass".to_string(),
        Some(Err(why)) => why.clone(),
    };
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw.onboard.verify.v1",
                "class": class,
                "beacon": v.beacon,
                "seed": v.seed.map(|s| misaka_palw_sdk::runtime_pack::commit::hex(&s)),
                "posted": posted,
                "vectors_selected": v.vectors_selected,
                "leaves_selected": v.leaves_selected,
                "leaves_rechecked": v.leaves_rechecked,
                "leaf_faults": v.leaf_faults,
                "chain_state": r.lifecycle_state,
                "agrees": report.agrees,
                "why": report.why,
                "note": "vector logits/commits digests are not re-executed here (the runtime pack's verify-conformance does); their tokens are refutable through a Final kernel claim",
            })
        );
    } else {
        println!("beacon           {}", v.beacon);
        println!("posted evidence  {posted}");
        println!(
            "selection        {} vector(s), {} leaf(s); re-read {} leaf(s) from the artifact, {} contradict it",
            v.vectors_selected,
            v.leaves_selected,
            v.leaves_rechecked,
            v.leaf_faults.len()
        );
        println!("chain            {} — {}", r.lifecycle_state, if report.agrees { "AGREES" } else { "DISAGREES" });
        println!("                 {}", report.why);
        for check in &v.leaf_faults {
            println!("refutable        LeafDecode at check {check}: file a tag-109 Refute with the leaf's opening");
        }
    }
    if report.agrees { Ok(()) } else { Err(CliError::new(exit::GENERIC, "the fresh verifier disagrees with the chain")) }
}
