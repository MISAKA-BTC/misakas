//! **RFC-0009 A0, detached signing — `misaka model sign` and `misaka model submit`** (step 1, the builder, is
//! `model add --export-bundle`, in [`crate::operator::model_add`]).
//!
//! * **`sign`** runs where the key is, and needs no node. It reads the exported bundle, **re-derives everything it signs** from the
//!   bundle's own object bytes and from this build's network parameters ([`misaka_palw_remote::bundle::check_bundle_v1`]), compares
//!   the class, the root and the owner with what the user EXPECTS, shows every cost by its payer, asks, and only then signs: the bond
//!   key signs the registration message, the payer key (the same key, or another one) signs the carrier's funding input. A bundle
//!   whose recipient, fee, class root or owner was altered is refused by name.
//! * **`submit`** needs no key. It verifies the signed file from its bytes alone, tells a resend (the same signed bytes — idempotent)
//!   from a duplicate registration (another carrier for the same class — a second fee for a registration the chain would refuse),
//!   relays through several nodes, and follows the registry with a quorum. An ACK is not inclusion; an accepted registration is not
//!   `ACTIVE_REWARDABLE`; every state read from a node is `UNVERIFIED_REMOTE_STATE`.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_rpc_core::api::rpc::RpcApi;
use misaka_palw_remote::bundle::{
    BundleStageV1, DetachedSigner, ExpectationsV1, RegistrationBundleV1, SentRecordV1, SignedRegistrationV1, SignerPolicyV1,
    SubmissionKindV1, SubmitPolicyV1, UNVERIFIED_REMOTE_STATE, carrier_sign_v1, check_bundle_v1, classify_submission_v1,
    onboarding_view_v1, owner_sign_v1, unsigned_object_of, verify_signed_registration_v1,
};
use misaka_palw_remote::register::{DuplicateVerdictV1, RegistrationStateV1, RegistryRowV1, exact_duplicate_v1};

use crate::operator::finding::{Finding, Severity, paint};
use crate::operator::model_remote as remote;
use crate::operator::snapshot;
use crate::operator::tty::{Flow, Halt, Step};
use crate::{CliResult, exit};
use misaka_palw_remote::verify::{ModeLabelV1, signing_gate_v1};

/// **The class a detached signature is made in.** Offline, the only verified fact is the owner bond's key proven against `--pin` (checked by
/// `check_bundle_v1`): from the user's own node that pin is a trusted checkpoint and the decision point for an append-only fact
/// (`VERIFIED_REMOTE`); typed in from elsewhere it anchors the header only (`HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED`); without it every chain
/// fact in the bundle is the builder's nodes' word (`UNVERIFIED_REMOTE`).
pub(crate) fn sign_mode_v1(pinned_proof_checked: bool, pin_from_own_node: bool) -> ModeLabelV1 {
    use misaka_palw_remote::verify::{L2StatusV1, mode_label_v1};
    let l2 = if pin_from_own_node {
        L2StatusV1::EstablishedAtTrustedCheckpoint
    } else {
        L2StatusV1::Unverified("a pin from elsewhere anchors the header, not the fork choice")
    };
    mode_label_v1(false, pinned_proof_checked, pinned_proof_checked, &l2)
}

/// `--expect-class`, `--expect-root`, `--expect-owner`.
#[derive(Clone, Debug, Default)]
pub(crate) struct ExpectArgs {
    pub(crate) class: Option<String>,
    pub(crate) root: Option<String>,
    pub(crate) owner: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct SignArgs {
    pub(crate) bundle: PathBuf,
    pub(crate) key_file: Option<String>,
    pub(crate) key_stdin: bool,
    pub(crate) payer_key_file: Option<String>,
    pub(crate) owner_only: bool,
    pub(crate) carrier_only: bool,
    pub(crate) expect: ExpectArgs,
    pub(crate) max_fee_sompi: Option<u64>,
    pub(crate) pin: Option<String>,
    /// The pin came from the user's own full node.
    pub(crate) pin_from_own_node: bool,
    /// `--accept-unverified-state <LABEL>`.
    pub(crate) accept_unverified_state: Option<String>,
    pub(crate) out: Option<PathBuf>,
    pub(crate) yes: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SubmitArgs {
    pub(crate) signed: PathBuf,
    pub(crate) relay: Vec<String>,
    pub(crate) relay_min: Option<usize>,
    pub(crate) expect: ExpectArgs,
    pub(crate) max_fee_sompi: Option<u64>,
    pub(crate) allow_duplicate: bool,
    pub(crate) no_wait: bool,
    pub(crate) pin: Option<String>,
    pub(crate) yes: bool,
}

/// The CLI's seed-holding key behind the library's signer trait. The library never sees a seed.
pub(crate) struct KeySigner(pub(crate) kaspa_pq_validator_core::ValidatorKey);

impl DetachedSigner for KeySigner {
    fn public_key(&self) -> Vec<u8> {
        self.0.public_key().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        if context.len() > 255 {
            return Err("an ML-DSA context is at most 255 bytes".into());
        }
        Ok(self.0.sign_with_context(message, context).to_vec())
    }
}

fn halt(code: &'static str, exit_code: i32, what: impl Into<String>, why: impl Into<String>) -> Halt {
    Halt::Blocked(Finding::error(code, exit_code, what.into()).current(why.into()))
}

fn refused(e: impl std::fmt::Display) -> Halt {
    Halt::Blocked(
        Finding::error("E-BUNDLE-REFUSED", exit::NOT_READY, "Nothing was signed: the bundle does not hold")
            .current(e.to_string())
            .reason("the signer derives what it signs itself; the builder is not trusted")
            .fix("export the bundle again from a node you trust, or correct --expect-*"),
    )
}

fn parse_hash(label: &str, text: &str) -> Result<Hash64, Halt> {
    text.trim()
        .parse::<Hash64>()
        .map_err(|_| halt("E-EXPECT", exit::GENERIC, format!("{label} is not a 128-hex hash"), text.to_string()))
}

fn net_of(ctx: &crate::node::Ctx) -> Result<NetworkId, Halt> {
    NetworkId::from_str(&ctx.network)
        .map_err(|e| halt("E-CONFIG-NETWORK", exit::CONFIG, format!("'{}' is not a network id", ctx.network), e.to_string()))
}

struct Chain {
    net: NetworkId,
    exposure: u64,
    domain: Hash64,
    ruleset: String,
}

fn chain_of(ctx: &crate::node::Ctx) -> Result<Chain, Halt> {
    let net = net_of(ctx)?;
    let (params, _salt) = crate::wallet::chain_params(ctx, net)
        .map_err(|e| halt("E-CONFIG-DRILL-SALT", exit::CONFIG, "The drill salt is refused", e.msg))?;
    let PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        return Err(halt("E-SETUP-NO-PALW", exit::CONFIG, format!("{net} has no PALW classes in this build"), ""));
    };
    Ok(Chain {
        net,
        exposure: bundle.state.registration_exposure_sompi(),
        domain: kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            net.to_string().as_bytes(),
            Some(params.genesis.hash),
        ),
        ruleset: params.consensus_params_id().to_string(),
    })
}

/// What the user expects. With flags: exactly those. Without: the file's own values, **flagged**, and never with `--yes`.
fn expectations(e: &ExpectArgs, file: (Hash64, Hash64, &str), yes: bool, flow: &Flow) -> Result<(ExpectationsV1, bool), Halt> {
    let given = e.class.is_some() && e.root.is_some() && e.owner.is_some();
    if yes && !given {
        return Err(Halt::Blocked(
            Finding::error("E-EXPECT-REQUIRED", exit::GENERIC, "--yes needs --expect-class, --expect-root and --expect-owner")
                .reason("a yes that names nothing would sign whatever the builder put in the file")
                .fix("pass the class, the artifact root and the owner bond you mean to register"),
        ));
    }
    let class_id = match &e.class {
        Some(c) => parse_hash("--expect-class", c)?,
        None => file.0,
    };
    let artifact_root = match &e.root {
        Some(r) => parse_hash("--expect-root", r)?,
        None => file.1,
    };
    let owner_bond = e.owner.clone().unwrap_or_else(|| file.2.to_string());
    if !given {
        flow.ui.say(&paint::yellow(
            "  ! no --expect-class/--expect-root/--expect-owner: the class, root and owner below are the FILE's own — you are confirming them by reading them",
        ));
    }
    Ok((ExpectationsV1 { class_id, artifact_root, owner_bond }, !given))
}

fn default_out(bundle: &Path, suffix: &str) -> PathBuf {
    let stem = bundle.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "bundle".into());
    let stem = stem.strip_suffix(".owner-signed").unwrap_or(&stem).to_string();
    bundle.with_file_name(format!("{stem}.{suffix}.json"))
}

fn write_new(path: &Path, text: &str) -> Result<(), Halt> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| {
        Halt::Blocked(
            Finding::error("E-WRITE", exit::HOST, format!("{}: {e}", path.display()))
                .fix("--out <a file that does not exist>: a signed file is never overwritten"),
        )
    })?;
    f.write_all(text.as_bytes()).map_err(|e| halt("E-WRITE", exit::HOST, format!("{}: {e}", path.display()), ""))
}

/// `misaka model sign`.
pub(crate) async fn sign(ctx: &crate::node::Ctx, args: SignArgs) -> CliResult {
    let mut flow = Flow::new(ctx.output, args.yes);
    let mut doc = serde_json::Map::new();
    let result = sign_flow(ctx, &args, &mut flow, &mut doc).await;
    let done = match (doc.get("signed").and_then(|v| v.as_str()), doc.get("owner_signed").and_then(|v| v.as_str())) {
        (Some(path), _) => format!(
            "SIGNED — {path}. Nothing was sent and the class is NOT registered: `misaka model submit {path} --relay <node>,<node>`"
        ),
        (None, Some(path)) => format!(
            "OWNER-SIGNED — {path}. The payer signs the carrier next: `misaka model sign {path} --carrier-only --key-file <payer seed>`"
        ),
        _ => "nothing was signed".to_string(),
    };
    flow.finish(result, "misaka.model.sign.v1", &done, "misaka model sign <bundle>", doc)
}

async fn sign_flow(
    ctx: &crate::node::Ctx,
    args: &SignArgs,
    flow: &mut Flow,
    doc: &mut serde_json::Map<String, serde_json::Value>,
) -> Step {
    let chain = chain_of(ctx)?;
    let text = std::fs::read_to_string(&args.bundle)
        .map_err(|e| halt("E-READ", exit::HOST, format!("{}: {e}", args.bundle.display()), ""))?;
    let b = RegistrationBundleV1::from_json(&text).map_err(refused)?;
    flow.ui.say(&paint::bold(&format!("MISAKA model sign · {}", chain.net)));
    flow.ui.say(&paint::dim(
        "  Re-derived here, not trusted from the bundle: network domain, ruleset, class/root/owner against what YOU expect,",
    ));
    flow.ui.say(&paint::dim(
        "  the message the bond signs, the carrier's recipient and fee. The builder's chain facts are UNVERIFIED_REMOTE_STATE.",
    ));
    let first = b.stage;
    if first == BundleStageV1::OwnerSigned && args.owner_only {
        return Err(halt("E-STAGE", exit::GENERIC, "The bundle is already owner-signed", "--owner-only has nothing to do"));
    }
    if first == BundleStageV1::Unsigned && args.carrier_only {
        return Err(halt("E-STAGE", exit::GENERIC, "The owner has not signed yet", "--carrier-only needs an owner-signed bundle"));
    }
    let (expect, from_file) = expectations(&args.expect, (b.class_id, b.artifact_root, &b.owner_bond), args.yes, flow)?;
    let now_daa = match ctx.rpc.as_deref() {
        Some(rpc) => {
            let node = snapshot::connect_to(&ctx.network, Some(rpc), Duration::from_secs(ctx.timeout_secs.clamp(2, 15)))
                .await
                .map_err(|(u, e)| {
                    halt("E-NODE-RPC-UNREACHABLE", exit::COMPONENT_DOWN, "The clock node does not answer", format!("{u}: {e}"))
                })?;
            Some(node.daa())
        }
        None => None,
    };
    let pin = args.pin.as_deref().map(crate::operator::remote_proof::parse_pin).transpose()?;
    let policy = SignerPolicyV1 {
        network: chain.net.to_string(),
        network_domain: chain.domain,
        ruleset_id: chain.ruleset.clone(),
        registration_exposure_sompi: chain.exposure,
        expect,
        max_wallet_sompi: args.max_fee_sompi.unwrap_or(b.quote.wallet_total_sompi),
        now_daa,
        pin,
    };
    let checked = check_bundle_v1(&b, &policy, first).map_err(refused)?;
    flow.ui.say("");
    flow.ui.say(&paint::bold("  What you are about to sign:"));
    for line in checked.review_lines(&b) {
        flow.ui.sub(&line);
    }
    flow.ui.sub(&format!(
        "payer      {} pays the carrier (input {}:{}, {} sompi)",
        b.payer.address, b.payer.funding.txid, b.payer.funding.index, b.payer.funding.amount
    ));
    flow.ui.sub(&format!(
        "expiry     DAA {} {}",
        checked.expiry_daa,
        match now_daa {
            Some(now) => format!("(the node at --rpc is at {now}: {UNVERIFIED_REMOTE_STATE})"),
            None => "— NOT checked offline (no --rpc); `submit` checks it, and consensus does not: a leaked signed file stays valid until its funding input is spent".to_string(),
        }
    ));
    if from_file {
        flow.ui.sub(&paint::yellow("expect     taken from the file itself"));
    }
    // **The security class this signature is made in** (RFC-0009, 2026-10-08), printed, and the gate: below VERIFIED_REMOTE nothing is
    // signed without --accept-unverified-state naming the class. A pinned proof of the owner key was checked by `check_bundle_v1` above.
    let label = sign_mode_v1(pin.is_some(), args.pin_from_own_node);
    flow.row(
        if label >= ModeLabelV1::VerifiedRemote { Severity::Ok } else { Severity::Warning },
        "mode",
        format!("{} — {}", label.as_str(), label.claim()),
    );
    doc.insert("mode".into(), label.as_str().into());
    let accepted = match args.accept_unverified_state.as_deref() {
        Some(t) => Some(ModeLabelV1::parse(t).ok_or_else(|| {
            halt(
                "E-MODE-LABEL",
                exit::GENERIC,
                "--accept-unverified-state names no class",
                format!("{t:?}: HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED or UNVERIFIED_REMOTE"),
            )
        })?),
        None => None,
    };
    signing_gate_v1(label, accepted).map_err(|g| {
        Halt::Blocked(
            Finding::error(
                "E-MODE-UNVERIFIED",
                exit::NOT_READY,
                "Nothing was signed: the state behind this registration is not verified",
            )
            .current(g.to_string())
            .reason("not running a full node is fine; trusting what a node says is not — the class is named so the choice is yours")
            .fix(format!("--pin <a block from your own node> --pin-from-own-node, or --accept-unverified-state {}", label.as_str())),
        )
    })?;
    flow.ask("Sign this registration?", false, "nothing was signed").await?;

    let load = |file: Option<&String>, stdin: bool| -> Result<KeySigner, Halt> {
        let ks = crate::keys::KeySource { key_file: file.cloned(), key_stdin: stdin };
        ks.load_key().map(KeySigner).map_err(|e| halt("E-IDENT-NO-KEY", exit::IDENTITY, "The key could not be read", e.msg))
    };
    let owner_or_payer = load(args.key_file.as_ref(), args.key_stdin)?;
    let mut working = b.clone();
    if first == BundleStageV1::Unsigned {
        working = owner_sign_v1(&b, &policy, &owner_or_payer).map_err(refused)?;
        let path = args.out.clone().filter(|_| args.owner_only).unwrap_or_else(|| default_out(&args.bundle, "owner-signed"));
        if args.owner_only {
            write_new(&path, &working.to_json())?;
            flow.row(Severity::Ok, "owner signed", format!("{} — the carrier is still unsigned", path.display()));
            doc.insert("owner_signed".into(), path.display().to_string().into());
            return Ok(());
        }
    }
    let payer_signer;
    let payer: &KeySigner = match args.payer_key_file.as_ref() {
        Some(file) => {
            payer_signer = load(Some(file), false)?;
            &payer_signer
        }
        None => &owner_or_payer,
    };
    let signed = carrier_sign_v1(&working, &policy, payer).map_err(refused)?;
    let path = args.out.clone().unwrap_or_else(|| default_out(&args.bundle, "signed"));
    write_new(&path, &signed.to_json())?;
    flow.row(
        Severity::Ok,
        "signed",
        format!("tx {}… · object {}…", &signed.tx_id.to_string()[..16], &signed.object_id.to_string()[..16]),
    );
    flow.ui.sub(&format!("class      {}", signed.class_id));
    flow.ui.sub(&format!("written    {}", path.display()));
    doc.insert("signed".into(), path.display().to_string().into());
    doc.insert("tx_id".into(), signed.tx_id.to_string().into());
    doc.insert("class_id".into(), signed.class_id.to_string().into());
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------------
// submit
// ---------------------------------------------------------------------------------------------------------------------------

fn journal_path(network: &str, class_hex: &str) -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".misaka").join(network).join("model-add").join(&class_hex[..16]).join("sent.jsonl")
}

fn read_sent(path: &Path) -> Vec<SentRecordV1> {
    std::fs::read_to_string(path)
        .map(|t| t.lines().filter_map(|l| serde_json::from_str::<SentRecordV1>(l).ok()).collect())
        .unwrap_or_default()
}

fn append_sent(path: &Path, rec: &SentRecordV1) {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{}", serde_json::to_string(rec).unwrap_or_default());
    }
}

/// The registry verdict across the relay nodes that answered: a node that says the exact class is registered wins, then a conflict.
fn registry_verdict(class_id: Hash64, root: Hash64, per_node: &[Vec<RegistryRowV1>]) -> DuplicateVerdictV1 {
    let mut verdict = DuplicateVerdictV1::New;
    for rows in per_node {
        match exact_duplicate_v1(class_id, root, rows) {
            v @ DuplicateVerdictV1::Reuse { .. } => return v,
            v @ DuplicateVerdictV1::Conflict(_) => verdict = v,
            DuplicateVerdictV1::New => {}
        }
    }
    verdict
}

pub(crate) fn print_onboarding(flow: &Flow, native: Option<&str>) {
    flow.ui.say("");
    flow.ui.say(&paint::bold("  Where this registration is (shared onboarding lifecycle):"));
    for l in onboarding_view_v1(native) {
        flow.ui.sub(&format!("{:<31} {:<24} {}", l.code, l.status, l.note));
    }
}

/// `misaka model submit`.
pub(crate) async fn submit(ctx: &crate::node::Ctx, args: SubmitArgs) -> CliResult {
    let mut flow = Flow::new(ctx.output, args.yes);
    let mut doc = serde_json::Map::new();
    let result = submit_flow(ctx, &args, &mut flow, &mut doc).await;
    let done = match doc.get("state").and_then(|v| v.as_str()) {
        Some("already-registered") => "ALREADY REGISTERED — nothing was sent; the registry's state is shown above (accepted is not Active)".to_string(),
        Some("registration-accepted") => "REGISTRATION ACCEPTED by a quorum of nodes (UNVERIFIED_REMOTE_STATE) — REGISTERED_DORMANT is not Active, and not a model line".to_string(),
        Some("relayed") => "RELAYED — an ACK is not inclusion; `misaka model registration <class>` follows it".to_string(),
        _ => "done".to_string(),
    };
    flow.finish(result, "misaka.model.submit.v1", &done, "misaka model submit <signed>", doc)
}

async fn submit_flow(
    ctx: &crate::node::Ctx,
    args: &SubmitArgs,
    flow: &mut Flow,
    doc: &mut serde_json::Map<String, serde_json::Value>,
) -> Step {
    let chain = chain_of(ctx)?;
    let text = std::fs::read_to_string(&args.signed)
        .map_err(|e| halt("E-READ", exit::HOST, format!("{}: {e}", args.signed.display()), ""))?;
    let s = SignedRegistrationV1::from_json(&text).map_err(refused)?;
    flow.ui.say(&paint::bold(&format!("MISAKA model submit · {}", chain.net)));
    // Relaying signs nothing (the bytes are already signed); the class of what the relays REPORT is printed all the same.
    let mode = ModeLabelV1::UnverifiedRemote;
    flow.row(
        Severity::Warning,
        "mode",
        format!(
            "{} — {} (a --pin proof of the registration is shown separately once the relays report it)",
            mode.as_str(),
            mode.claim()
        ),
    );
    let (expect, from_file) = expectations(&args.expect, (s.class_id, s.artifact_root, &s.owner_bond), args.yes, flow)?;
    if s.ruleset_id != chain.ruleset {
        return Err(halt(
            "E-RULESET",
            exit::NETWORK_MISMATCH,
            "The signed file was made under another ruleset than this build's",
            format!("file {} · this build {}", s.ruleset_id, chain.ruleset),
        ));
    }
    let timeout = Duration::from_secs(ctx.timeout_secs.clamp(2, 15));
    // The relays, as readers: their tips are the clock and their class tables the registry (all UNVERIFIED_REMOTE_STATE).
    let mut nodes = Vec::new();
    for url in &args.relay {
        match snapshot::connect_to(&ctx.network, Some(url), timeout).await {
            Ok(n) if n.ops_0122 => nodes.push(n),
            Ok(n) => flow.row(Severity::Warning, "relay", format!("{} predates the registration reads — not counted", n.url)),
            Err((u, e)) => flow.row(Severity::Warning, "relay", format!("{u}: {e} — not counted")),
        }
    }
    if nodes.is_empty() {
        return Err(halt("E-RELAY", exit::COMPONENT_DOWN, "No relay node answers", args.relay.join(", ")));
    }
    let now = nodes.iter().map(|n| n.daa()).min().unwrap_or(0);
    let policy = SubmitPolicyV1 {
        network: chain.net.to_string(),
        network_domain: chain.domain,
        expect,
        max_fee_sompi: args.max_fee_sompi.unwrap_or(s.fee_sompi),
        now_daa: Some(now),
    };
    let pin = args.pin.as_deref().map(crate::operator::remote_proof::parse_pin).transpose()?;
    let verified = verify_signed_registration_v1(&s, &policy).map_err(refused)?;
    flow.row(
        Severity::Ok,
        "signed bytes",
        format!(
            "tx {}… verifies: owner signature, funding signature, one change output back to the payer, fee {} sompi",
            &s.tx_id.to_string()[..16],
            verified.fee_sompi
        ),
    );
    flow.ui.sub(&format!("class      {}", s.class_id));
    flow.ui.sub(&format!("root       {}", s.artifact_root));
    flow.ui.sub(&format!("owner      {}", s.owner_bond));
    flow.ui.sub(&format!(
        "expiry     DAA {} (the relays are at {now}: {UNVERIFIED_REMOTE_STATE}; consensus does not enforce it)",
        s.expiry_daa
    ));
    if from_file {
        flow.ui.sub(&paint::yellow("expect     taken from the file itself"));
    }

    // Resend or duplicate?
    let mut tables = Vec::new();
    for n in &nodes {
        let rows = n
            .client()
            .get_palw_classes()
            .await
            .map(|t| {
                t.classes
                    .into_iter()
                    .map(|c| RegistryRowV1 {
                        class_id: c.class_id.parse().unwrap_or_default(),
                        artifact_root: c.artifact_root.parse().unwrap_or_default(),
                        registrant_bond: None,
                        lifecycle: c.status,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        tables.push(rows);
    }
    let verdict = registry_verdict(s.class_id, s.artifact_root, &tables);
    let class_hex = s.class_id.to_string();
    let journal = journal_path(&chain.net.to_string(), &class_hex);
    let sent = read_sent(&journal);
    let kind = classify_submission_v1(s.tx_id, s.class_id, s.artifact_root, &sent, &verdict);
    match &kind {
        SubmissionKindV1::AlreadyRegistered { lifecycle } => {
            flow.row(
                Severity::Ok,
                "registry",
                format!(
                    "class {}… is already registered ({lifecycle}, {UNVERIFIED_REMOTE_STATE}) — nothing is sent",
                    &class_hex[..16]
                ),
            );
            print_onboarding(flow, Some(lifecycle));
            doc.insert("state".into(), "already-registered".into());
            return Ok(());
        }
        SubmissionKindV1::Conflict(why) => {
            return Err(halt(
                "E-MODEL-CONFLICT",
                exit::MODEL,
                "The registry holds this class or these weights under something else",
                why.clone(),
            ));
        }
        SubmissionKindV1::DuplicateRegistration { earlier_tx } if !args.allow_duplicate => {
            return Err(Halt::Blocked(
                Finding::error("E-MODEL-DUPLICATE-CARRIER", exit::MODEL, "ANOTHER carrier for this class was already sent from here")
                    .current(format!("earlier tx {earlier_tx}; this one is {}", s.tx_id))
                    .reason("if the first folds, the chain refuses a second registration of the class (DuplicateClass) and the second fee is gone")
                    .fix("resend the FIRST file (the same bytes are idempotent), or --allow-duplicate once you know the first is lost"),
            ));
        }
        SubmissionKindV1::DuplicateRegistration { .. } => flow.row(
            Severity::Warning,
            "duplicate",
            "another carrier for this class was sent before — sent anyway (--allow-duplicate)",
        ),
        SubmissionKindV1::Resend => {
            flow.row(Severity::Info, "resend", "the same signed bytes were sent before: idempotent, no new fee, no new registration")
        }
        SubmissionKindV1::First => flow.row(Severity::Info, "first send", "no earlier send of this class from here"),
    }
    flow.ask(&format!("Relay these signed bytes through {} node(s)?", nodes.len()), false, "nothing was sent").await?;

    let urls: Vec<String> = nodes.iter().map(|n| n.url.trim_start_matches("ws://").to_string()).collect();
    let min = args.relay_min.unwrap_or(urls.len() / 2 + 1).clamp(1, urls.len());
    let report = remote::relay(&chain.net.to_string(), &urls, &verified.tx, min, timeout)
        .await
        .map_err(|e| halt("E-RELAY", exit::COMPONENT_DOWN, "Too few nodes took the signed carrier", e))?;
    for (node, outcome) in &report.per_node {
        flow.ui.sub(&format!("relay      {node}: {outcome:?}"));
    }
    for liar in report.tampered() {
        flow.row(Severity::Warning, "relay", format!("{liar} answered with another transaction id — it did not relay these bytes"));
    }
    append_sent(
        &journal,
        &SentRecordV1 { class_id: s.class_id, artifact_root: s.artifact_root, tx_id: s.tx_id, object_id: s.object_id },
    );
    flow.row(
        Severity::Ok,
        "relayed",
        format!("tx {}… via {} of {} node(s) — an ACK is not inclusion", &s.tx_id.to_string()[..16], report.successes, urls.len()),
    );
    doc.insert("state".into(), "relayed".into());
    doc.insert("tx_id".into(), s.tx_id.to_string().into());
    if args.no_wait {
        print_onboarding(flow, None);
        return Ok(());
    }
    let unsigned = unsigned_object_of(&verified.object).map_err(refused)?;
    let mut tracker = remote::tracker(&verified.tx, &unsigned, &s.owner_bond, min);
    tracker.relayed();
    let object_id = s.object_id.to_string();
    let txid = s.tx_id.to_string();
    let deadline = std::time::Instant::now() + Duration::from_secs(20 * 60);
    let mut last = String::new();
    while std::time::Instant::now() < deadline {
        let round = remote::observe(&chain.net.to_string(), &urls, &class_hex, &object_id, &txid, timeout).await;
        let state = tracker.observe(&round).clone();
        if state.name() != last {
            flow.ui.sub(&format!(
                "state      {} ({UNVERIFIED_REMOTE_STATE}: {min} node(s) agree; no header or state proof)",
                state.name()
            ));
            last = state.name().to_string();
        }
        match state {
            RegistrationStateV1::RegistrationAccepted { .. } => {
                let native = round.iter().find_map(|o| o.row.as_ref().map(|r| r.lifecycle.clone()));
                flow.row(Severity::Ok, "registered", format!("class {}… accepted by a quorum of {min} node(s)", &class_hex[..16]));
                // With a pin, also PROVE it: the class table's proof against the block the user pinned.
                if let Some(pin) = pin {
                    let bond = crate::bond::parse_outpoint(&s.owner_bond).map(kaspa_consensus_core::palw_state_v2::PalwBondKeyV2);
                    match (bond, crate::operator::remote_proof::fetch_state_proof(&nodes[0], pin, "classes").await) {
                        (Ok(owner), Ok((header, classes))) => {
                            if let Err(e) = crate::operator::remote_proof::print_registration_standing(
                                flow,
                                &header,
                                pin,
                                &classes,
                                &s.class_id,
                                s.artifact_root,
                                &owner,
                            ) {
                                flow.row(Severity::Warning, "proof", format!("does not hold against your pin: {e}"));
                            }
                        }
                        (_, Err(e)) => flow.row(
                            Severity::Warning,
                            "proof",
                            format!("unavailable ({e}); the registry row stays {UNVERIFIED_REMOTE_STATE}"),
                        ),
                        (Err(e), _) => flow.row(Severity::Warning, "proof", format!("the owner bond does not parse: {}", e.msg)),
                    }
                }
                print_onboarding(flow, native.as_deref());
                doc.insert("state".into(), "registration-accepted".into());
                return Ok(());
            }
            RegistrationStateV1::Refused { code } => {
                return Err(halt(
                    "E-MODEL-REGISTRATION-DROPPED",
                    exit::MODEL,
                    "The registration was mined and the chain refused it",
                    format!("{code} — the carrier's fee is spent and no class row was written"),
                ));
            }
            RegistrationStateV1::Misattributed(why) => {
                return Err(halt(
                    "E-MODEL-MISATTRIBUTED",
                    exit::MODEL,
                    "The registry holds this class under another bond or root",
                    why,
                ));
            }
            // Only the SAME signed bytes are re-sent; nothing that costs a new fee happens on its own.
            RegistrationStateV1::Reorged => {
                let _ = remote::relay(&chain.net.to_string(), &urls, &verified.tx, 1, timeout).await;
            }
            _ => {}
        }
        flow.pause(5).await?;
    }
    print_onboarding(flow, None);
    Err(Halt::Waiting("a quorum of nodes to report the registration's fate".into(), exit::NOT_READY))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_base0_profile::rc_job_context;
    use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B_A16, QWEN25_A16_CANONICAL, qwen25_a16_profile_v1};
    use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwClassAdmissionCarriageV2, PalwConsensusObjectV2, PalwPwuRuleV2};
    use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
    use kaspa_pq_validator_core::ValidatorKey;
    use misaka_palw_remote::bundle as detached;

    fn h(n: u8) -> Hash64 {
        Hash64::from_bytes([n; 64])
    }

    fn object(bond: TransactionOutpoint) -> PalwConsensusObjectV2 {
        let profile = qwen25_a16_profile_v1(QWEN25_1_5B_A16).expect("the A16 geometry projects");
        let canonical = rc_job_context(&profile, QWEN25_A16_CANONICAL.0, QWEN25_A16_CANONICAL.1);
        PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            artifact_root: h(0x77),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1_000 },
            initial_target: 1u128 << 100,
            share_permille: 10,
            activation_daa: 0,
            admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
                profile,
                canonical,
                registrant_bond: PalwBondKeyV2(bond),
                signature: Vec::new(),
            })),
        }
    }

    fn params() -> kaspa_consensus_core::config::params::Params {
        kaspa_consensus_core::config::params::Params::from(NetworkId::with_suffix(
            kaspa_consensus_core::network::NetworkType::Testnet,
            12,
        ))
    }

    /// The key-free pricing the exporter uses is the shipped, key-signing builder's price, to the sompi.
    #[test]
    fn the_key_free_price_is_the_shipped_builders_price_and_the_detached_carrier_has_its_id() {
        let params = params();
        let owner = ValidatorKey::from_seed([1; 32]);
        let outpoint = TransactionOutpoint::new(h(0x66), 0);
        let spk = kaspa_txscript::pay_to_address_script(&owner.funding_address(params.prefix()));
        let entry = UtxoEntry::new(50_000_000, spk, 100, false);
        let probe = {
            let mut o = object(TransactionOutpoint::new(h(0x33), 1));
            if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut o {
                c.signature = vec![0u8; remote::MLDSA87_SIGNATURE_LEN];
            }
            o
        };
        let (mass, fee) = remote::price_carrier(&params, &probe, outpoint, &entry).expect("prices");
        // The shipped path: sign, measure, rebuild.
        let calc = kaspa_consensus_core::mass::MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );
        let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
        let shipped_probe = owner.build_palw_lifecycle_tx(&probe, outpoint, &entry, floor).unwrap();
        let shipped_mass = calc.calc_non_contextual_masses(&shipped_probe).compute_mass;
        assert_eq!(mass, shipped_mass, "a placeholder script of the real length weighs what the real one does");
        let shipped_fee =
            kaspa_pq_validator_core::relay_fee_for_compute_mass(shipped_mass).max(floor).max(crate::palw_fp::carrier_rent_v1(&probe));
        assert_eq!(fee, shipped_fee);
        // And the detached path's carrier is the shipped builder's transaction: same id (the id excludes the signature script).
        let signed_obj = {
            let mut o = probe.clone();
            if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut o {
                c.signature = vec![7u8; remote::MLDSA87_SIGNATURE_LEN];
            }
            o
        };
        let shipped = owner.build_palw_lifecycle_tx(&signed_obj, outpoint, &entry, fee).unwrap();
        let detached_tx =
            detached::carrier_body_v1(&signed_obj, outpoint, entry.amount, fee, &entry.script_public_key, vec![]).unwrap();
        assert_eq!(misaka_palw_remote::relay::tx_id_of_bytes(&shipped), misaka_palw_remote::relay::tx_id_of_bytes(&detached_tx));
        assert_eq!(shipped.payload, detached_tx.payload);
        assert_eq!(shipped.outputs, detached_tx.outputs);
        assert_eq!(shipped.inputs[0].sequence, detached_tx.inputs[0].sequence);
        assert_eq!(shipped.inputs[0].sig_op_count, detached_tx.inputs[0].sig_op_count);
    }

    struct Fixture {
        params: kaspa_consensus_core::config::params::Params,
        owner_seed: [u8; 32],
        payer_seed: [u8; 32],
        bundle: RegistrationBundleV1,
        policy: SignerPolicyV1,
        fee: u64,
    }

    fn fixture() -> Fixture {
        let params = params();
        let (owner_seed, payer_seed) = ([1u8; 32], [2u8; 32]);
        let owner = ValidatorKey::from_seed(owner_seed);
        let payer = ValidatorKey::from_seed(payer_seed);
        let bond = TransactionOutpoint::new(h(0x33), 1);
        let bond_text = format!("{}:{}", bond.transaction_id, bond.index);
        let unsigned = object(bond);
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            params.net.to_string().as_bytes(),
            Some(params.genesis.hash),
        );
        let payer_spk = kaspa_txscript::pay_to_address_script(&payer.funding_address(params.prefix()));
        let funding = UtxoEntry::new(50_000_000, payer_spk.clone(), 100, false);
        let outpoint = TransactionOutpoint::new(h(0x66), 0);
        let mut probe = unsigned.clone();
        if let PalwConsensusObjectV2::ClassRegistered { admission: Some(c), .. } = &mut probe {
            c.signature = vec![0u8; remote::MLDSA87_SIGNATURE_LEN];
        }
        let (mass, fee) = remote::price_carrier(&params, &probe, outpoint, &funding).unwrap();
        let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } = &unsigned else { unreachable!() };
        let PalwConsensusMode::ConsensusV2(pb) = params.palw_consensus_mode.clone() else { panic!("testnet-12 is ConsensusV2") };
        let facts = misaka_palw_remote::register::RegistrationFactsV1 {
            network_domain: domain,
            terms_digest: h(0x44),
            tip_hash: h(0x55),
            tip_daa: 1_000,
            class_id: *class_id,
            artifact_root: *artifact_root,
            object_digest: remote::object_digest(&unsigned),
            exposure_sompi: pb.state.registration_exposure_sompi(),
            burn_sompi: kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1,
            bond: misaka_palw_remote::register::BondFactsV1 {
                outpoint: bond_text.clone(),
                known: true,
                key_matches: true,
                retiring: false,
                collateral_sompi: u64::MAX / 4,
                backing_sompi: 0,
                live_locked_sompi: 0,
            },
            carrier_mass: mass,
            carrier_fee_sompi: fee,
            wallet_spendable_sompi: funding.amount,
            filings: vec![],
        };
        let quote = misaka_palw_remote::register::quote_registration_v1(facts, 600, fee).unwrap();
        let bundle = detached::build_bundle_v1(detached::BundleInputsV1 {
            network: params.net.to_string(),
            network_domain: domain,
            ruleset_id: params.consensus_params_id().to_string(),
            unsigned_object: unsigned.clone(),
            owner_pubkey: owner.public_key().to_vec(),
            owner_proof: None,
            payer_address: payer.funding_address(params.prefix()).to_string(),
            payer_spk,
            funding_outpoint: outpoint,
            funding_entry: funding,
            quote,
            sources: vec![],
        })
        .unwrap();
        let policy = SignerPolicyV1 {
            network: params.net.to_string(),
            network_domain: domain,
            ruleset_id: params.consensus_params_id().to_string(),
            registration_exposure_sompi: pb.state.registration_exposure_sompi(),
            expect: ExpectationsV1 { class_id: *class_id, artifact_root: *artifact_root, owner_bond: bond_text },
            max_wallet_sompi: fee,
            now_daa: Some(1_001),
            pin: None,
        };
        Fixture { params, owner_seed, payer_seed, bundle, policy, fee }
    }

    /// The CLI's real key type through the detached steps: bond key and a DIFFERENT payer key, then verified from the bytes.
    #[test]
    fn the_validator_key_signs_both_detached_steps_with_two_different_keys() {
        let fx = fixture();
        let owner_signer = KeySigner(ValidatorKey::from_seed(fx.owner_seed));
        let payer_signer = KeySigner(ValidatorKey::from_seed(fx.payer_seed));
        let owner_signed = owner_sign_v1(&fx.bundle, &fx.policy, &owner_signer).expect("the bond key signs");
        // The payer key is not the bond key: it cannot sign the OWNER step.
        assert!(owner_sign_v1(&fx.bundle, &fx.policy, &payer_signer).is_err());
        let signed = carrier_sign_v1(&owner_signed, &fx.policy, &payer_signer).expect("the payer key signs the carrier");
        let verified = verify_signed_registration_v1(
            &signed,
            &SubmitPolicyV1 {
                network: fx.policy.network.clone(),
                network_domain: fx.policy.network_domain,
                expect: fx.policy.expect.clone(),
                max_fee_sompi: fx.fee,
                now_daa: Some(1_001),
            },
        )
        .expect("verifies from the bytes alone");
        // The carrier decodes exactly as the node's fold reads it: a lifecycle carrier registering this class, with this object.
        assert_eq!(
            kaspa_consensus_core::palw_model_registration_v1::palw_registration_carrier_class_v1(&verified.tx),
            Some((fx.policy.expect.class_id, fx.policy.expect.artifact_root))
        );
        assert_eq!(
            kaspa_consensus_core::palw_model_registration_v1::palw_registration_carrier_object_v1(&verified.tx),
            Some(verified.object)
        );
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("misaka-model-sign-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ctx() -> crate::node::Ctx {
        crate::node::Ctx {
            output: crate::OutputFormat::Human,
            network: "testnet-12".into(),
            rpc: None,
            node_grpc: None,
            evm_rpc: String::new(),
            timeout_secs: 5,
            quiet: true,
            palw_drill_genesis_salt: None,
        }
    }

    fn seed_file(dir: &Path, name: &str, seed: [u8; 32]) -> String {
        let path = dir.join(name).display().to_string();
        kaspa_pq_validator_core::write_validator_seed(&path, &seed).expect("the seed file is written");
        path
    }

    fn sign_args(dir: &Path, fx: &Fixture, bundle: &RegistrationBundleV1) -> SignArgs {
        let path = dir.join("bundle.json");
        std::fs::write(&path, bundle.to_json()).unwrap();
        SignArgs {
            bundle: path,
            key_file: Some(seed_file(dir, "owner.seed", fx.owner_seed)),
            key_stdin: false,
            payer_key_file: Some(seed_file(dir, "payer.seed", fx.payer_seed)),
            owner_only: false,
            carrier_only: false,
            expect: ExpectArgs {
                class: Some(fx.policy.expect.class_id.to_string()),
                root: Some(fx.policy.expect.artifact_root.to_string()),
                owner: Some(fx.policy.expect.owner_bond.clone()),
            },
            max_fee_sompi: Some(fx.fee),
            pin: None,
            pin_from_own_node: false,
            // No pin in these fixtures: the chain facts are the builder's nodes' word, accepted explicitly.
            accept_unverified_state: Some("UNVERIFIED_REMOTE".into()),
            out: None,
            yes: true,
        }
    }

    /// `misaka model sign` end to end on a fixture bundle: files in, a signed file out that verifies from its bytes.
    #[tokio::test]
    async fn model_sign_writes_a_signed_file_that_verifies_and_never_overwrites_it() {
        let fx = fixture();
        let dir = scratch("ok");
        let args = sign_args(&dir, &fx, &fx.bundle);
        assert!(sign(&ctx(), args.clone()).await.is_ok());
        let signed_path = dir.join("bundle.signed.json");
        let signed = SignedRegistrationV1::from_json(&std::fs::read_to_string(&signed_path).unwrap()).unwrap();
        verify_signed_registration_v1(
            &signed,
            &SubmitPolicyV1 {
                network: fx.policy.network.clone(),
                network_domain: fx.policy.network_domain,
                expect: fx.policy.expect.clone(),
                max_fee_sompi: fx.fee,
                now_daa: None,
            },
        )
        .unwrap();
        // A signed file is never overwritten: a second run fails loudly instead of replacing it.
        assert!(sign(&ctx(), args).await.is_err());
        // The owner-only step writes its own file and leaves the carrier unsigned.
        let dir2 = scratch("owner-only");
        let mut args = sign_args(&dir2, &fx, &fx.bundle);
        args.owner_only = true;
        assert!(sign(&ctx(), args).await.is_ok());
        let owner_signed =
            RegistrationBundleV1::from_json(&std::fs::read_to_string(dir2.join("bundle.owner-signed.json")).unwrap()).unwrap();
        assert_eq!(owner_signed.stage, BundleStageV1::OwnerSigned);
        assert!(!dir2.join("bundle.signed.json").exists());
    }

    /// **The class the signature is made in** (RFC-0009, 2026-10-08): without a pinned proof the bundle's chain facts are the builder's
    /// nodes' word, and `model sign` signs nothing until the user names that class; a pin from elsewhere is header-verified only; a pin
    /// from the user's own node is VERIFIED_REMOTE. A wrong label name is refused too.
    #[tokio::test]
    async fn model_sign_refuses_to_sign_unverified_state_unless_the_class_is_named() {
        assert_eq!(sign_mode_v1(false, false), ModeLabelV1::UnverifiedRemote);
        assert_eq!(sign_mode_v1(true, false), ModeLabelV1::HeaderVerifiedForkChoiceUnverified);
        assert_eq!(sign_mode_v1(true, true), ModeLabelV1::VerifiedRemote);
        let fx = fixture();
        for accept in [None, Some("HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED".to_string()), Some("FULLY_TRUSTED".to_string())] {
            let dir = scratch("mode");
            let mut args = sign_args(&dir, &fx, &fx.bundle);
            args.accept_unverified_state = accept.clone();
            assert!(sign(&ctx(), args).await.is_err(), "{accept:?} does not accept UNVERIFIED_REMOTE");
            assert!(!dir.join("bundle.signed.json").exists() && !dir.join("bundle.owner-signed.json").exists(), "nothing signed");
        }
    }

    /// A bundle altered by the builder is refused by `model sign`, and no file is written.
    #[tokio::test]
    async fn model_sign_refuses_a_tampered_bundle_and_a_yes_that_names_nothing() {
        let fx = fixture();
        // The change recipient swapped.
        let dir = scratch("recipient");
        let mut evil = fx.bundle.clone();
        evil.carrier.change_spk = detached::spk_text_v1(&kaspa_txscript::pay_to_address_script(
            &ValidatorKey::from_seed([9; 32]).funding_address(fx.params.prefix()),
        ));
        assert!(sign(&ctx(), sign_args(&dir, &fx, &evil)).await.is_err());
        assert!(!dir.join("bundle.signed.json").exists());
        // The fee inflated past what the signer allowed.
        let dir = scratch("fee");
        let mut args = sign_args(&dir, &fx, &fx.bundle);
        args.max_fee_sompi = Some(fx.fee - 1);
        assert!(sign(&ctx(), args).await.is_err());
        assert!(!dir.join("bundle.signed.json").exists());
        // Another root than the one the user expects.
        let dir = scratch("root");
        let mut args = sign_args(&dir, &fx, &fx.bundle);
        args.expect.root = Some(h(0x99).to_string());
        assert!(sign(&ctx(), args).await.is_err());
        assert!(!dir.join("bundle.signed.json").exists());
        // Another owner than the one the user expects.
        let dir = scratch("owner");
        let mut args = sign_args(&dir, &fx, &fx.bundle);
        args.expect.owner = Some(format!("{}:{}", h(0xAB), 0));
        assert!(sign(&ctx(), args).await.is_err());
        assert!(!dir.join("bundle.signed.json").exists());
        // `--yes` with no expectations would sign whatever the builder wrote.
        let dir = scratch("yes");
        let mut args = sign_args(&dir, &fx, &fx.bundle);
        args.expect = ExpectArgs::default();
        assert!(sign(&ctx(), args).await.is_err());
        assert!(!dir.join("bundle.signed.json").exists());
        // The wrong key for the owner step.
        let dir = scratch("key");
        let mut args = sign_args(&dir, &fx, &fx.bundle);
        args.key_file = Some(seed_file(&dir, "other.seed", [7; 32]));
        assert!(sign(&ctx(), args).await.is_err());
        assert!(!dir.join("bundle.signed.json").exists());
    }
}
