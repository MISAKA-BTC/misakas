//! **`misaka model add <model>`** — ADR-0122 §8.2: a model's life walked from this build's catalog
//! to LIVE — `REGISTERED → CERTIFIED (block lane) → CERTIFIED (prompt lane, optional) → LIVE` — and
//! resumed from the chain's state, never from a journal: every step is done when the class table or
//! the chain's certified families say so.
//!
//! Everything `palw-certify` and `palw submit-object` did by hand runs here as a library call:
//! * **Registration** is built and signed with the chain's LIVE terms (`getPalwRegistrationTerms`).
//!   The genesis terms `palw extension submit` used are refused as soon as the base class has
//!   retargeted — every epoch.
//! * **Certification** looks first: a lane whose kernels a chain-certified family already covers is
//!   bound directly, and a family is drilled and filed only when none does. Filing a family the
//!   chain already holds is refused `FamilyAlreadyCertified`, and a binding no chain family covers
//!   is dropped `NoCertifiedFamilyCovers` — both with the fee gone and only the node's log saying
//!   why.
//! * **The drill's chunks** are written and submitted in index order, and the flow waits for the
//!   group to apply before it binds.
//!
//! Nothing is spent without a yes. A registration reserves exposure on the registrant's bond; every
//! filing pays carrier fees (and the certification rents where the chain charges them).

use crate::operator::finding::{Finding, Severity, paint};
use crate::operator::profile::Profile;
use crate::operator::snapshot::{self, NodeRead};
use crate::operator::tty::{Flow, Halt, Step};
use crate::operator::{catalog, host, status};
use crate::{CliResult, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_e2e_adjudicability::PalwE2eFamilyV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwCertificationEvidenceV1, PalwCertifiedLaneV1, PalwConsensusObjectV2, PalwRegistrationTermsV2,
};
use kaspa_rpc_core::api::rpc::RpcApi;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// What `model add` takes.
#[derive(Clone, Debug, Default)]
pub(crate) struct ModelAddArgs {
    /// A catalog model id (or a unique part of one); `None` lists the catalog.
    pub(crate) model: Option<String>,
    pub(crate) artifact: Option<String>,
    /// Also certify the free-prompt lane.
    pub(crate) prompt_lane: bool,
    /// The slowest fleet seat's measured replay cost, for the seat-window bound.
    pub(crate) seat_ms_per_position: Option<u64>,
    pub(crate) yes: bool,
    pub(crate) no_wait: bool,
}

/// A certified family as the chain holds it.
#[derive(Clone, Debug)]
pub(crate) struct ChainFamily {
    pub(crate) lane: PalwCertifiedLaneV1,
    pub(crate) digest: Hash64,
    pub(crate) family: PalwE2eFamilyV1,
}

/// **The live terms**, decoded: what a registration must take from the chain, with the chain's
/// certified families of both lanes.
pub(crate) fn decode_terms(
    r: &kaspa_rpc_core::GetPalwRegistrationTermsResponse,
) -> Result<(PalwRegistrationTermsV2, Vec<ChainFamily>), String> {
    let hash = |s: &str| s.parse::<Hash64>().map_err(|_| format!("'{s}' is not a 128-hex id"));
    let mut families = Vec::new();
    for f in &r.families {
        let lane = match f.lane.as_str() {
            "attempt" => PalwCertifiedLaneV1::Attempt,
            "free_prompt" => PalwCertifiedLaneV1::FreePrompt,
            other => return Err(format!("unknown lane '{other}'")),
        };
        let mut bytes = vec![0u8; f.family_hex.len() / 2];
        faster_hex::hex_decode(f.family_hex.as_bytes(), &mut bytes).map_err(|e| format!("family {}: {e}", f.digest))?;
        let family: PalwE2eFamilyV1 = borsh::from_slice(&bytes).map_err(|e| format!("family {}: {e}", f.digest))?;
        families.push(ChainFamily { lane, digest: hash(&f.digest)?, family });
    }
    let terms = PalwRegistrationTermsV2 {
        min_grantable_share_permille: r.min_grantable_share_permille,
        slash_value_per_pwu: r.slash_value_per_pwu,
        initial_target: r.initial_target.parse().map_err(|_| format!("initial target '{}' is not a u128", r.initial_target))?,
        registered_class_ids: r.registered_class_ids.iter().map(|c| hash(c)).collect::<Result<_, _>>()?,
        registered_artifact_roots: r.registered_artifact_roots.iter().map(|c| hash(c)).collect::<Result<_, _>>()?,
        chain_certified_families: families
            .iter()
            .filter(|f| f.lane == PalwCertifiedLaneV1::Attempt)
            .map(|f| f.family.clone())
            .collect(),
    };
    Ok((terms, families))
}

/// The chain family whose kernels contain every kernel `profile` reaches on `lane` — the
/// transition's own test (`reachable ⊆ family.kernel_ids`, one family, never a union).
pub(crate) fn covering_chain_family<'a>(
    families: &'a [ChainFamily],
    lane: PalwCertifiedLaneV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Option<&'a ChainFamily> {
    let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(profile);
    families.iter().find(|f| f.lane == lane && reachable.is_subset(&f.family.kernel_ids))
}

/// **A signed `ClassRegistered`** — built twice by the SDK (the object, then the registrant's
/// signature over its whole preimage), exactly as the panel builds one. `extension submit` and
/// `model add` share it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn signed_class_registration(
    params: &kaspa_consensus_core::config::params::Params,
    bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    sdk: &misaka_palw_sdk::PalwClassSdk,
    candidate: &misaka_palw_sdk::PalwRegistrationCandidateV1,
    terms: &PalwRegistrationTermsV2,
    shape: &kaspa_consensus_core::palw_class_admission_v2::PalwAdmissionShapeV1,
    bond_key: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2,
    key: &kaspa_pq_validator_core::ValidatorKey,
) -> Result<PalwConsensusObjectV2, String> {
    let build = |signature: Vec<u8>| sdk.build_post_genesis_registration(bundle, candidate, terms, 0, bond_key, signature, shape);
    let unsigned = build(Vec::new())?;
    let PalwConsensusObjectV2::ClassRegistered {
        class_id,
        activation_daa,
        artifact_root,
        slash_value_per_pwu,
        initial_target,
        pwu_rule,
        share_permille,
        ..
    } = &unsigned
    else {
        return Err("the SDK did not build a registration".into());
    };
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    let canonical = candidate.entry.canonical_context();
    let message = kaspa_consensus_core::palw_state_v2::palw_class_registration_message_v2(
        network_domain,
        *class_id,
        *share_permille,
        *activation_daa,
        &bond_key,
        *artifact_root,
        *slash_value_per_pwu,
        *initial_target,
        pwu_rule,
        &canonical,
    );
    let signature = key
        .sign_with_context(message.as_byte_slice(), kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_V2_MLDSA87_CONTEXT);
    build(signature.to_vec())
}

/// `model add <model>`: which catalog row it names — the exact id, else a unique part of one.
pub(crate) fn resolve_catalog<'a>(
    entries: &'a [misaka_palw_sdk::PalwClassEntryV1],
    selector: &str,
) -> Result<&'a misaka_palw_sdk::PalwClassEntryV1, String> {
    if let Some(e) = entries.iter().find(|e| e.model_id == selector) {
        return Ok(e);
    }
    let lower = selector.to_ascii_lowercase();
    let exact: Vec<_> = entries.iter().filter(|e| e.model_id.to_ascii_lowercase() == lower).collect();
    if let [one] = exact.as_slice() {
        return Ok(one);
    }
    let hits: Vec<_> = entries.iter().filter(|e| e.model_id.to_ascii_lowercase().contains(&lower)).collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("this build's catalog has no model matching '{selector}'")),
        many => Err(format!(
            "'{selector}' matches {} models: {}",
            many.len(),
            many.iter().map(|e| e.model_id).collect::<Vec<_>>().join(", ")
        )),
    }
}

fn lane_name(lane: PalwCertifiedLaneV1) -> &'static str {
    match lane {
        PalwCertifiedLaneV1::Attempt => "block lane",
        PalwCertifiedLaneV1::FreePrompt => "prompt lane",
    }
}

/// The class row, fresh.
async fn class_row(node: &NodeRead, class_id: &str) -> Result<Option<kaspa_rpc_core::RpcPalwClassRow>, Halt> {
    let table = node.client().get_palw_classes().await.map_err(|e| {
        Halt::Blocked(
            Finding::error("E-NODE-CLASSES", exit::COMPONENT_DOWN, "The class table could not be read").current(e.to_string()),
        )
    })?;
    Ok(table.classes.into_iter().find(|c| c.class_id == class_id))
}

async fn families_now(node: &NodeRead) -> Result<Vec<ChainFamily>, Halt> {
    let r = node.client().get_palw_registration_terms().await.map_err(|e| {
        Halt::Blocked(
            Finding::error("E-NODE-TERMS", exit::COMPONENT_DOWN, "The registration terms could not be read").current(e.to_string()),
        )
    })?;
    decode_terms(&r).map(|(_, f)| f).map_err(|e| Halt::Blocked(Finding::error("E-NODE-TERMS", exit::COMPONENT_DOWN, e)))
}

/// Everything one `model add` run holds.
struct Walk<'a> {
    ctx: &'a crate::node::Ctx,
    profile: &'a Profile,
    args: &'a ModelAddArgs,
    node: NodeRead,
    params: kaspa_consensus_core::config::params::Params,
    workdir: PathBuf,
}

impl Walk<'_> {
    fn submit_ctx(&self) -> crate::node::Ctx {
        crate::node::Ctx {
            output: crate::OutputFormat::Human,
            network: self.profile.network.clone(),
            rpc: Some(self.node.url.trim_start_matches("ws://").to_string()),
            node_grpc: self.ctx.node_grpc.clone(),
            evm_rpc: self.ctx.evm_rpc.clone(),
            timeout_secs: self.ctx.timeout_secs,
            quiet: true,
        }
    }

    fn key_source(&self) -> Result<crate::keys::KeySource, Halt> {
        let path = self.profile.key_path.as_ref().ok_or_else(|| {
            Halt::Blocked(
                Finding::error("E-IDENT-NO-KEY", exit::IDENTITY, "No key to sign with")
                    .reason("a registration is signed by the registrant bond's key, and every filing's carrier is funded by it")
                    .fix("--key-file <seed>, or set [mining] key in ~/.misaka/mining.toml (misaka mining setup)"),
            )
        })?;
        Ok(crate::keys::KeySource { key_file: Some(path.display().to_string()), key_stdin: false })
    }

    /// Write objects in order and submit them as one chained run (`palw submit-object`'s path:
    /// one funding output, every later carrier funded by the previous one's change).
    async fn submit(&self, flow: &Flow, name: &str, objects: &[PalwConsensusObjectV2]) -> Step {
        if flow.ui.json {
            return Err(Halt::Declined(format!(
                "{name}: filing pays fees — run `misaka model add` without --output json to be asked, or file {} yourself",
                self.workdir.display()
            )));
        }
        std::fs::create_dir_all(&self.workdir)
            .map_err(|e| Halt::Blocked(Finding::error("E-HOST-WORKDIR", exit::HOST, format!("{}: {e}", self.workdir.display()))))?;
        let mut paths = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            let file = if objects.len() == 1 {
                self.workdir.join(format!("{name}.obj"))
            } else {
                self.workdir.join(format!("{name}.obj.chunk{i}"))
            };
            let bytes =
                borsh::to_vec(object).map_err(|e| Halt::Blocked(Finding::error("E-OBJECT-ENCODE", exit::GENERIC, e.to_string())))?;
            std::fs::write(&file, bytes)
                .map_err(|e| Halt::Blocked(Finding::error("E-HOST-WORKDIR", exit::HOST, format!("{}: {e}", file.display()))))?;
            paths.push(file);
        }
        let ks = self.key_source()?;
        crate::palw_fp::submit_objects(&self.submit_ctx(), &ks, &paths, true).await.map_err(|e| {
            Halt::Blocked(
                Finding::error("E-OBJECT-REFUSED", exit::FUNDS, format!("The {name} carrier(s) were refused"))
                    .current(e.msg)
                    .fix("the key's largest spendable output funds every carrier of one filing: consolidate, then re-run"),
            )
        })
    }

    /// Poll until `done` says so, or the deadline; Ctrl-C stops.
    async fn wait_for<F, Fut>(&self, flow: &Flow, what: &str, minutes: u64, mut done: F) -> Step
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<bool, Halt>>,
    {
        if self.args.no_wait {
            return Err(Halt::Waiting(what.to_string(), exit::NOT_READY));
        }
        let deadline = Instant::now() + Duration::from_secs(minutes * 60);
        let mut beat = Instant::now();
        while Instant::now() < deadline {
            if done().await? {
                return Ok(());
            }
            if beat.elapsed() > Duration::from_secs(60) {
                flow.ui.sub(&paint::dim(&format!("still waiting for {what} (Ctrl-C stops; running it again resumes)")));
                beat = Instant::now();
            }
            flow.pause(5).await?;
        }
        Err(Halt::Waiting(what.to_string(), exit::NOT_READY))
    }
}

/// `misaka model add [<model>]`.
pub(crate) async fn run(ctx: &crate::node::Ctx, profile: Profile, args: ModelAddArgs) -> CliResult {
    let mut flow = Flow::new(ctx.output, args.yes);
    let mut doc = serde_json::Map::new();
    let result = walk(ctx, &profile, &args, &mut flow, &mut doc).await;
    if args.model.is_none() && result.is_ok() {
        // The catalog listing: nothing was added, so there is no "done" line to print.
        return Ok(());
    }
    let resume = match &args.model {
        Some(m) => format!("misaka model add {m}"),
        None => "misaka model add <model>".to_string(),
    };
    flow.finish(result, "misaka.model.add.v1", "LIVE — weight-bearing from the next epoch", &resume, doc)
}

async fn walk(
    ctx: &crate::node::Ctx,
    profile: &Profile,
    args: &ModelAddArgs,
    flow: &mut Flow,
    doc: &mut serde_json::Map<String, serde_json::Value>,
) -> Step {
    let net = profile
        .network
        .parse::<kaspa_consensus_core::network::NetworkId>()
        .map_err(|e| Halt::Blocked(Finding::error("E-CONFIG-NETWORK", exit::CONFIG, format!("'{}': {e}", profile.network))))?;
    let params = kaspa_consensus_core::config::params::Params::from(net);
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        return Err(Halt::Blocked(Finding::error(
            "E-SETUP-NO-PALW",
            exit::CONFIG,
            format!("{net} has no PALW classes in this build"),
        )));
    };
    flow.ui.say(&paint::bold(&format!("MISAKA model add · {net}")));
    flow.ui.say(&paint::dim("  Each step reads what the chain already holds, so running this again resumes where it stopped."));
    flow.ui.say("");
    flow.ui.mark(Severity::Info, "catalog", "building this build's class table (it drills each family once, a few seconds)…");
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), net.to_string().into_bytes());
    let ledger = sdk.ledger();

    // No model named: the catalog, and which of it the chain already holds.
    let Some(selector) = args.model.clone() else {
        let node = snapshot::connect_to(&profile.network, profile.rpc.as_deref(), Duration::from_secs(5)).await.ok();
        let held: Vec<String> = match &node {
            Some(n) if n.ops_0122 => {
                n.client().get_palw_classes().await.map(|t| t.classes.into_iter().map(|c| c.class_id).collect()).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        let width = ledger.iter().map(|e| e.model_id.len()).max().unwrap_or(5) + 2;
        flow.ui.say(&paint::dim(&format!("  {:<width$}{:<18}{:<12}{}", "MODEL", "LINEAGE", "CLASS", "ON CHAIN")));
        for e in &ledger {
            let id = e.class_id().to_string();
            flow.ui.say(&format!(
                "  {:<width$}{:<18}{:<12}{}",
                e.model_id,
                e.lineage_id,
                format!("{}…", &id[..8]),
                if node.is_none() {
                    "?"
                } else if held.contains(&id) {
                    "yes"
                } else {
                    "no"
                }
            ));
        }
        flow.ui.say(&paint::dim(
            "  misaka model add <model>   registers one and certifies its lanes; misaka model status <class> reads one",
        ));
        return Ok(());
    };
    let entry = resolve_catalog(&ledger, &selector)
        .map_err(|why| {
            Halt::Blocked(Finding::error("E-MODEL-UNKNOWN", exit::MODEL, why).fix("misaka model add   (lists this build's catalog)"))
        })?
        .clone();
    let class_id = entry.class_id();
    let class_hex = class_id.to_string();
    doc.insert("model_id".into(), entry.model_id.into());
    doc.insert("class_id".into(), class_hex.clone().into());
    flow.row(
        Severity::Ok,
        "model",
        format!(
            "{} · {} · class {}… · {}",
            entry.model_id,
            entry.lineage_id,
            &class_hex[..16],
            if entry.needs_artifact_file { "runs from an artifact file" } else { "derived — no file" }
        ),
    );

    // The node, and the terms it reads from its tip.
    let node = snapshot::connect_to(&profile.network, profile.rpc.as_deref(), Duration::from_secs(ctx.timeout_secs.clamp(2, 15)))
        .await
        .map_err(|(url, e)| {
            Halt::Blocked(
                Finding::error("E-NODE-RPC-UNREACHABLE", exit::COMPONENT_DOWN, "The node's RPC does not answer")
                    .current(format!("{url}: {e}")),
            )
        })?;
    if !node.ops_0122 {
        return Err(Halt::Blocked(
            Finding::error("E-NODE-TOO-OLD", exit::COMPONENT_DOWN, "The node predates the reads model add needs")
                .fix("run a node built from this tree"),
        ));
    }
    let terms_resp = node.client().get_palw_registration_terms().await.map_err(|e| {
        Halt::Blocked(
            Finding::error("E-NODE-TOO-OLD", exit::COMPONENT_DOWN, "The node does not serve the registration terms")
                .current(format!("getPalwRegistrationTerms: {e}"))
                .fix("run a node built from this tree"),
        )
    })?;
    if !terms_resp.available {
        return Err(Halt::Blocked(Finding::error(
            "E-NODE-TERMS",
            exit::COMPONENT_DOWN,
            "The node has no PALW state to read terms from yet",
        )));
    }
    let (terms, families) =
        decode_terms(&terms_resp).map_err(|e| Halt::Blocked(Finding::error("E-NODE-TERMS", exit::COMPONENT_DOWN, e)))?;
    let walk = Walk {
        ctx,
        profile,
        args,
        workdir: dirs::home_dir().unwrap_or_default().join(".misaka").join(&profile.network).join("model-add").join(&class_hex[..16]),
        params: params.clone(),
        node,
    };

    // REGISTERED.
    let row = match class_row(&walk.node, &class_hex).await? {
        Some(row) => {
            flow.row(Severity::Ok, "registered", format!("{} · registered at DAA {}", row.status, status::group(row.registered_daa)));
            row
        }
        None => {
            register(&walk, flow, &bundle, &sdk, &entry, &terms).await?;
            class_row(&walk.node, &class_hex)
                .await?
                .ok_or_else(|| Halt::Waiting("the registration to appear in the class table".into(), exit::NOT_READY))?
        }
    };
    let stage = crate::operator::market::class_stage(&row.status, row.share_permille);
    match stage {
        crate::operator::market::ClassStage::Registered { activation_daa, .. } => {
            return Err(Halt::Waiting(
                format!("activation at DAA {} (the flip is a clock)", status::group(activation_daa)),
                exit::NOT_READY,
            ));
        }
        crate::operator::market::ClassStage::Frozen { since_daa } => {
            return Err(Halt::Blocked(
                Finding::error("E-MODEL-FROZEN", exit::MODEL, "The class is frozen")
                    .current(format!("frozen since DAA {} — contradicted; no certification changes that", status::group(since_daa))),
            ));
        }
        _ => {}
    }

    // CERTIFIED (block lane): weight-bearing.
    if row.share_permille.is_some_and(|s| s > 0) {
        flow.row(Severity::Ok, "block lane", format!("weight-bearing at {} ‰", row.share_permille.unwrap_or(0)));
    } else {
        certify(&walk, flow, &entry, &families, PalwCertifiedLaneV1::Attempt).await?;
    }
    // CERTIFIED (prompt lane), when asked.
    let row = class_row(&walk.node, &class_hex).await?.unwrap_or(row);
    if row.fp_certified {
        flow.row(Severity::Ok, "prompt lane", "certified: free-prompt claims enter state");
    } else if args.prompt_lane {
        let families = families_now(&walk.node).await?;
        certify(&walk, flow, &entry, &families, PalwCertifiedLaneV1::FreePrompt).await?;
    } else {
        flow.row(Severity::Skip, "prompt lane", "not certified — optional: misaka model add … --prompt-lane");
    }
    doc.insert("next".into(), format!("misaka model market open {class_hex}").into());
    flow.ui.sub(&paint::dim(&format!(
        "next: misaka model market open {}…  ·  misaka mining setup --model {}…",
        &class_hex[..16],
        &class_hex[..16]
    )));
    Ok(())
}

async fn register(
    walk: &Walk<'_>,
    flow: &mut Flow,
    bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    sdk: &misaka_palw_sdk::PalwClassSdk,
    entry: &misaka_palw_sdk::PalwClassEntryV1,
    terms: &PalwRegistrationTermsV2,
) -> Step {
    let class_hex = entry.class_id().to_string();
    // The root: the artifact's own, computed from the file — the chain pins it, and a class whose
    // root is someone else's weights is a mispairing the admission gate refuses.
    let artifact_root = if entry.needs_artifact_file {
        let named = walk.args.artifact.clone().map(|a| PathBuf::from(crate::operator::procs::expand_home(&a)));
        let candidates: Vec<PathBuf> = match named {
            Some(p) => vec![p],
            None => crate::operator::wizard::artifacts_here(&walk.profile.network, Some(&walk.profile.appdir)),
        };
        let mut found = None;
        for path in &candidates {
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            flow.ui.mark(
                Severity::Info,
                "artifact",
                &format!("reading {} ({}) for its root…", host::tilde(path), host::human_bytes(size)),
            );
            let loaded = match sdk.load_artifact(path) {
                Ok(l) => l,
                Err(e) => {
                    flow.ui.sub(&paint::yellow(&format!("! {e}")));
                    continue;
                }
            };
            if let Some((_, Ok(root))) = sdk.pairings(&loaded).into_iter().find(|(e, _)| e.model_id == entry.model_id) {
                flow.row(Severity::Ok, "artifact", format!("{} · root {}…", host::tilde(path), &root.to_string()[..16]));
                found = Some(root);
                break;
            }
            flow.ui.sub(&paint::yellow(&format!("! {} does not pair with {}", host::tilde(path), entry.model_id)));
        }
        found.ok_or_else(|| {
            Halt::Blocked(
                Finding::error(
                    "E-MODEL-ARTIFACT-MISSING",
                    exit::MODEL,
                    format!("{} needs its artifact file to register", entry.model_id),
                )
                .reason("the registration pins the artifact's root, computed from the file")
                .fix("misaka model add <model> --artifact <file>"),
            )
        })?
    } else {
        return Err(Halt::Blocked(
            Finding::error("E-MODEL-DERIVED-UNREGISTERED", exit::MODEL, "A derived row this chain does not hold")
                .reason("derived rows register at genesis; this build cannot root one without a file")
                .current(entry.model_id.to_string()),
        ));
    };
    if terms.registered_artifact_roots.contains(&artifact_root) {
        return Err(Halt::Blocked(
            Finding::error("E-MODEL-WEIGHTS-REGISTERED", exit::MODEL, "These weights are already registered as another class")
                .reason("a second class over the same root is the mispairing the admission gate exists to refuse")
                .current(format!("root {artifact_root}")),
        ));
    }
    // The registrant: this key's bond, active.
    let bond = walk.profile.bond.clone().ok_or_else(|| {
        Halt::Blocked(
            Finding::error("E-IDENT-NO-BOND", exit::IDENTITY, "No bond to register the class under")
                .reason("a class registration is signed by, and reserves exposure on, the registrant's bond")
                .fix("misaka mining setup (registers one), or --bond <txid>:<index>"),
        )
    })?;
    let ks = walk.key_source()?;
    let key = ks.load_key().map_err(|e| {
        Halt::Blocked(catalog::key_unreadable(&walk.profile.key_path.clone().unwrap_or_default().display().to_string(), &e.msg))
    })?;
    let ours = faster_hex::hex_string(key.public_key());
    let claims = walk.node.client().get_palw_claims(bond.clone(), "seat".into(), false, 1).await.map_err(|e| {
        Halt::Blocked(Finding::error("E-SETUP-BOND-UNREAD", exit::COMPONENT_DOWN, "The bond could not be read").current(e.to_string()))
    })?;
    if !claims.bond_known || !claims.bond_pubkey.eq_ignore_ascii_case(&ours) || claims.bond_retiring_since_daa.is_some() {
        return Err(Halt::Blocked(
            Finding::error("E-IDENT-BOND-NOT-REGISTRANT", exit::IDENTITY, "The bond cannot register a class")
                .current(format!(
                    "{bond}: {}",
                    if !claims.bond_known {
                        "the registry holds no bond there"
                    } else if claims.bond_retiring_since_daa.is_some() {
                        "it is retiring"
                    } else {
                        "it was registered by another key"
                    }
                ))
                .required("an Active bond registered to this key"),
        ));
    }
    let bond_op =
        crate::bond::parse_outpoint(&bond).map_err(|e| Halt::Blocked(Finding::error("E-SETUP-BOND", exit::IDENTITY, e.msg)))?;
    let shape = kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(
        &walk.params,
        bundle,
        &entry.profile,
        walk.node.daa(),
    )
    .map_err(|e| {
        Halt::Blocked(Finding::error("E-MODEL-SHAPE", exit::MODEL, "The chain would not admit this class's shape").current(e))
    })?;
    let candidate = misaka_palw_sdk::PalwRegistrationCandidateV1 { entry: entry.clone(), artifact_root };
    let object = signed_class_registration(
        &walk.params,
        bundle,
        sdk,
        &candidate,
        terms,
        &shape,
        kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(bond_op),
        &key,
    )
    .map_err(|e| {
        Halt::Blocked(Finding::error("E-MODEL-REGISTRATION", exit::MODEL, "The registration could not be built").current(e))
    })?;
    let share = match &object {
        PalwConsensusObjectV2::ClassRegistered { share_permille, .. } => *share_permille,
        _ => 0,
    };
    flow.ui.say("");
    flow.ui.say(&paint::bold("  Register this model's class:"));
    flow.ui.sub(&format!("class     {class_hex}"));
    flow.ui.sub(&format!("root      {artifact_root}"));
    flow.ui.sub(&format!("bond      {bond} — signs it, and holds its registration exposure"));
    flow.ui.sub(&if share > 0 {
        format!("share     {share} ‰ — a certified family covers its kernels, so it carries weight from registration")
    } else {
        "share     0 ‰ — weightless until its block lane is certified (the next step)".to_string()
    });
    flow.ask("Register it?", false, "the class was not registered").await?;
    walk.submit(flow, "class-registered", std::slice::from_ref(&object)).await?;
    let node = &walk.node;
    walk.wait_for(flow, "the registration to be mined", 20, || async { Ok(class_row(node, &class_hex).await?.is_some()) }).await?;
    flow.row(Severity::Ok, "registered", format!("class {}… · share {share} ‰", &class_hex[..16]));
    Ok(())
}

async fn certify(
    walk: &Walk<'_>,
    flow: &mut Flow,
    entry: &misaka_palw_sdk::PalwClassEntryV1,
    families: &[ChainFamily],
    lane: PalwCertifiedLaneV1,
) -> Step {
    let profile = &entry.profile;
    let class_id = entry.class_id();
    let class_hex = class_id.to_string();
    let name = lane_name(lane);
    // A family on the chain already covers it, or one has to be filed first.
    if let Some(f) = covering_chain_family(families, lane, profile) {
        flow.row(Severity::Ok, "family", format!("{name}: covered by chain family {}…", &f.digest.to_string()[..16]));
    } else {
        let family = misaka_palw_base0::e2e_drill::covering_rc_family_v1(profile, lane).ok_or_else(|| {
            Halt::Blocked(
                Finding::error("E-MODEL-NO-FAMILY", exit::MODEL, format!("No family this build drills covers its {name}"))
                    .reason(
                        "a certificate is about kernels the court implements; a new architecture needs a build whose court serves it",
                    )
                    .docs("docs/palw-certify-a-new-model.md"),
            )
        })?;
        flow.ui.mark(
            Severity::Info,
            "family",
            &format!("{name}: drilling the {} family (its built-in fixture, seconds)…", family.name()),
        );
        let evidence = match lane {
            PalwCertifiedLaneV1::Attempt => misaka_palw_base0::e2e_drill::rc_attempt_evidence_v1(family)
                .map(PalwCertificationEvidenceV1::Attempt)
                .map_err(|e| format!("{e:?}")),
            PalwCertifiedLaneV1::FreePrompt => misaka_palw_base0::e2e_drill::rc_free_prompt_evidence_v1(family)
                .map(PalwCertificationEvidenceV1::FreePrompt)
                .map_err(|e| format!("{e:?}")),
        }
        .map_err(|e| Halt::Blocked(Finding::error("E-MODEL-DRILL", exit::MODEL, "The drill failed").current(e)))?;
        // Graded here first, as palw-certify does: a drill the chain would refuse never leaves.
        let graded = evidence.grade().map_err(|e| {
            Halt::Blocked(
                Finding::error("E-MODEL-DRILL", exit::MODEL, "This build's court refuses its own drill").current(format!("{e:?}")),
            )
        })?;
        let digest = graded.digest();
        let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(evidence) };
        let pieces = match kaspa_consensus_core::palw_state_v2::palw_object_chunks_v1(&object) {
            Ok(None) => vec![object],
            Ok(Some(chunks)) => chunks,
            Err(e) => {
                return Err(Halt::Blocked(
                    Finding::error("E-MODEL-DRILL", exit::MODEL, "The drill cannot be chunked").current(format!("{e:?}")),
                ));
            }
        };
        flow.ui.say("");
        flow.ui.say(&paint::bold(&format!("  File the {} family for the {name}:", family.name())));
        flow.ui.sub(&format!("digest    {}…", &digest.to_string()[..16]));
        flow.ui.sub(&format!("carriers  {} — about one a block, sent in index order; the group applies with its last", pieces.len()));
        flow.ui.sub("pays      each carrier's fee, and the certification rents where this chain charges them");
        flow.ui.sub(&paint::dim("once filed it certifies every class its kernels cover — anyone's, not only this one"));
        flow.ask("File it?", false, "the family was not filed").await?;
        walk.submit(flow, &format!("family-{}", if lane == PalwCertifiedLaneV1::Attempt { "attempt" } else { "fp" }), &pieces).await?;
        let node = &walk.node;
        walk.wait_for(flow, &format!("the {} family's group to apply", family.name()), 40, || async {
            Ok(families_now(node).await?.iter().any(|f| f.lane == lane && f.digest == digest))
        })
        .await?;
        flow.row(Severity::Ok, "family", format!("{name}: {} family certified on chain", family.name()));
    }
    // The binding, behind the seat-window bound palw-certify applies.
    let (n_max, ms, _) = misaka_palw_base0::e2e_drill::seat_width_bound_v1(profile, walk.args.seat_ms_per_position);
    if profile.n_ctx as u64 > n_max {
        return Err(Halt::Blocked(
            Finding::error("E-MODEL-SEAT-WINDOW", exit::MODEL, "No seat can recompute this row inside the receipt window")
                .current(format!("n_ctx {} against n_max {n_max} at {ms} ms per position", profile.n_ctx))
                .fix("re-measure with --seat-ms-per-position from the slowest fleet host, or register a narrower row")
                .docs("docs/palw-certify-a-new-model.md"),
        ));
    }
    let binding = PalwConsensusObjectV2::ClassLaneCertified { class_id, lane, profile: Box::new(profile.clone()) };
    flow.ask(&format!("Bind this class's {name} to it (one carrier)?"), true, "the lane was not bound").await?;
    walk.submit(
        flow,
        &format!("class-{}", if lane == PalwCertifiedLaneV1::Attempt { "attempt" } else { "fp" }),
        std::slice::from_ref(&binding),
    )
    .await?;
    let node = &walk.node;
    walk.wait_for(flow, &format!("the {name} binding to apply"), 20, || async {
        let row = class_row(node, &class_hex).await?;
        Ok(match lane {
            PalwCertifiedLaneV1::Attempt => row.is_some_and(|r| r.share_permille.is_some_and(|s| s > 0)),
            PalwCertifiedLaneV1::FreePrompt => row.is_some_and(|r| r.fp_certified),
        })
    })
    .await?;
    flow.row(Severity::Ok, name, "certified");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn family(lane: PalwCertifiedLaneV1, kernels: &[u64]) -> ChainFamily {
        let family = PalwE2eFamilyV1 {
            family_id: Hash64::from_u64_word(1),
            drilled_class_id: Hash64::from_u64_word(2),
            kernel_ids: kernels.iter().map(|k| Hash64::from_u64_word(*k)).collect(),
            covering: Default::default(),
        };
        ChainFamily { lane, digest: family.digest(), family }
    }

    /// The live terms decode to the type the SDK builds from, with each family on its own lane —
    /// and a family's bytes round-trip, so its kernel set is the chain's, not a summary of it.
    #[test]
    fn the_live_terms_decode_to_what_the_builder_takes() {
        let attempt = family(PalwCertifiedLaneV1::Attempt, &[1, 2, 3]);
        let fp = family(PalwCertifiedLaneV1::FreePrompt, &[1, 2]);
        let row = |f: &ChainFamily, lane: &str| kaspa_rpc_core::RpcPalwCertifiedFamily {
            lane: lane.into(),
            digest: f.digest.to_string(),
            certified_daa: 9,
            family_hex: faster_hex::hex_string(&borsh::to_vec(&f.family).unwrap()),
        };
        let r = kaspa_rpc_core::GetPalwRegistrationTermsResponse {
            available: true,
            tip_daa: 100,
            base_class_id: Hash64::from_u64_word(7).to_string(),
            min_grantable_share_permille: 50,
            slash_value_per_pwu: 3,
            initial_target: "340282366920938463463374607431768211455".into(),
            registered_class_ids: vec![Hash64::from_u64_word(7).to_string()],
            registered_artifact_roots: vec![Hash64::from_u64_word(8).to_string()],
            families: vec![row(&attempt, "attempt"), row(&fp, "free_prompt")],
        };
        let (terms, families) = decode_terms(&r).unwrap();
        assert_eq!(terms.initial_target, u128::MAX);
        assert_eq!(terms.chain_certified_families, vec![attempt.family.clone()], "only the attempt lane prices a registration");
        assert_eq!(families.len(), 2);
        assert_eq!(families[1].lane, PalwCertifiedLaneV1::FreePrompt);
        assert_eq!(families[1].family.digest(), fp.digest);
        let mut bad = r.clone();
        bad.families[0].lane = "sideways".into();
        assert!(decode_terms(&bad).is_err());
    }

    /// Coverage is the transition's test: ONE family whose kernels contain every kernel the class
    /// reaches, on the same lane — never a union of two, never the other lane's.
    #[test]
    fn a_lane_is_covered_by_one_family_of_that_lane() {
        let profile = misaka_palw_base0::e2e_drill::catalog_profiles_v1(
            &kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .unwrap(),
        )
        .into_iter()
        .next()
        .expect("a catalog row")
        .1;
        let reachable: Vec<Hash64> =
            kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&profile).into_iter().collect();
        assert!(!reachable.is_empty());
        let all = |lane| ChainFamily {
            lane,
            digest: Hash64::from_u64_word(3),
            family: PalwE2eFamilyV1 {
                family_id: Hash64::default(),
                drilled_class_id: Hash64::default(),
                kernel_ids: reachable.iter().copied().collect(),
                covering: Default::default(),
            },
        };
        let half = ChainFamily {
            lane: PalwCertifiedLaneV1::Attempt,
            digest: Hash64::from_u64_word(4),
            family: PalwE2eFamilyV1 {
                kernel_ids: reachable.iter().take(1).copied().collect(),
                ..all(PalwCertifiedLaneV1::Attempt).family
            },
        };
        assert!(
            covering_chain_family(std::slice::from_ref(&half), PalwCertifiedLaneV1::Attempt, &profile).is_none()
                || reachable.len() == 1
        );
        let fp_only = [all(PalwCertifiedLaneV1::FreePrompt)];
        assert!(
            covering_chain_family(&fp_only, PalwCertifiedLaneV1::Attempt, &profile).is_none(),
            "the other lane's family does not count"
        );
        let both = [half, all(PalwCertifiedLaneV1::Attempt)];
        assert_eq!(
            covering_chain_family(&both, PalwCertifiedLaneV1::Attempt, &profile).map(|f| f.digest),
            Some(Hash64::from_u64_word(3))
        );
    }
}
