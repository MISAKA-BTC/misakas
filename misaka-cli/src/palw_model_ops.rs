//! `misaka model inspect|preflight|registration|readiness|certify` — and the tracking face of `model add`.

use crate::node::Ctx;
use crate::operator::profile::Profile;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1;
use kaspa_consensus_core::palw_model_fit_v1::{palw_fit_regime_for_v1, palw_model_fit_v2};
use kaspa_consensus_core::palw_model_registration_v1::{
    PalwModelRegistrationCodeV1, palw_model_reject_from_submit_text_v1, palw_registration_object_id_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{
    GetPalwModelCertificationRequest, GetPalwModelPreflightRequest, GetPalwModelReadinessRequest,
    GetPalwModelRegistrationStatusRequest, GetPalwModelRequest, RpcPalwModelRegistration,
};
use std::path::{Path, PathBuf};

pub(crate) fn print_pipeline(reg: &RpcPalwModelRegistration) {
    let tick = |ok: bool| if ok { "✅" } else { "❌" };
    println!("constructed {}", tick(reg.constructed));
    println!(
        "submitted  {}{}",
        tick(reg.submitted),
        if reg.transaction_id.is_empty() { String::new() } else { format!("  tx {}", &reg.transaction_id[..reg.transaction_id.len().min(16)]) }
    );
    let accepted_note = if !reg.accepted && !reg.reject_code.is_empty() {
        format!("  {}", reg.reject_code)
    } else {
        String::new()
    };
    println!("accepted   {}{accepted_note}", tick(reg.accepted));
    let included_note = if reg.included {
        format!("  DAA {}", reg.included_daa)
    } else if !reg.reject_code.is_empty() && reg.reject_code == PalwModelRegistrationCodeV1::RegistrationNotIncluded.code() {
        format!("  {}", reg.reject_code)
    } else {
        String::new()
    };
    println!("included   {}{included_note}", tick(reg.included));
    println!(
        "folded     {}{}",
        tick(reg.folded),
        if reg.registry_state.is_empty() { String::new() } else { format!("  {}", reg.registry_state) }
    );
}

fn connect(ctx: &Ctx) -> impl std::future::Future<Output = Result<crate::wallet::NodeView, CliError>> + '_ {
    crate::wallet::connect(ctx)
}

fn tick(ok: bool) -> &'static str {
    if ok { "✅" } else { "❌" }
}

pub(crate) async fn inspect(ctx: &Ctx, profile: Profile, artifact: PathBuf) -> CliResult {
    let net = profile.network.parse::<kaspa_consensus_core::network::NetworkId>().map_err(|e| CliError::new(exit::CONFIG, e.to_string()))?;
    let params = kaspa_consensus_core::config::params::Params::from(net);
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        return Err(CliError::new(exit::CONFIG, format!("{net} has no PALW classes")));
    };
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), net.to_string().into_bytes());
    let loaded = sdk.load_artifact(&artifact).map_err(|e| CliError::new(exit::MODEL, e))?;
    if ctx.output == OutputFormat::Json {
        let pairings: Vec<_> = sdk
            .pairings(&loaded)
            .into_iter()
            .map(|(entry, paired)| {
                serde_json::json!({
                    "model_id": entry.model_id,
                    "class_id": entry.class_id().to_string(),
                    "n_ctx": entry.profile.n_ctx,
                    "paired": paired.as_ref().ok().map(|r| r.to_string()),
                    "error": paired.err(),
                })
            })
            .collect();
        println!("{}", serde_json::json!({ "schema": "misaka.model.inspect.v1", "summary": loaded.summary, "lineage": loaded.lineage_id, "pairings": pairings }));
        return Ok(());
    }
    println!("{}", loaded.summary);
    println!("lineage          {}", loaded.lineage_id);
    for (entry, paired) in sdk.pairings(&loaded) {
        match paired {
            Ok(root) => {
                let canonical = entry.canonical_context();
                let shape = palw_admission_shape_at_v1(&params, &bundle, &entry.profile, 0).ok();
                let fit = shape.map(|s| {
                    palw_model_fit_v2(&entry.profile, &bundle, s.court, params.palw_prompt_ids_form_at(0), palw_fit_regime_for_v1(s.held, &entry.profile))
                });
                let court = fit.as_ref().and_then(|f| f.rows.iter().find(|r| r.wall == kaspa_consensus_core::palw_model_fit_v1::PalwFitWallV1::CourtWindow));
                let geom = fit.as_ref().and_then(|f| f.rows.iter().find(|r| r.wall == kaspa_consensus_core::palw_model_fit_v1::PalwFitWallV1::GeometryCeiling));
                let admission = shape.as_ref().and_then(|s| sdk.preflight_admission(&bundle, &entry, root, s).ok());
                println!("class_id         {}", entry.class_id());
                println!("model_id         {}  (ctx {})", entry.model_id, entry.profile.n_ctx);
                println!("source root      {}", entry.lineage_id);
                println!("artifact root    {root}");
                println!("tokenizer root   {}", canonical.tokenizer_id);
                println!("graph profile    {}", entry.profile.shape_profile_id());
                println!("ctx              {}", entry.profile.n_ctx);
                println!(
                    "CanonicalWork    prefill {} / decode {} / max_context {}",
                    canonical.declared_prefill_tokens, canonical.exact_decode_tokens, canonical.max_context_tokens
                );
                if let Some(row) = court {
                    println!("verification window  {} {} / {} {}", row.verdict_label(), row.need, row.have, row.unit);
                }
                if let Some(f) = &fit {
                    println!(
                        "working set      kv {} B + recurrent {} B + ids {} B",
                        f.seat.kv_cache_bytes, f.seat.recurrent_state_bytes, f.seat.prompt_ids_bytes
                    );
                }
                if let Some(row) = geom {
                    println!("FitsGlobalWindow {}", if row.verdict == kaspa_consensus_core::palw_model_fit_v1::PalwFitVerdictV1::Admitted { "✅" } else { "❌ GLOBAL_WINDOW_EXCEEDED" });
                }
                println!(
                    "admission result {}",
                    if admission.is_some() { "ADMISSION_OK".to_string() } else { "see palw-class inspect for the refusal".into() }
                );
                println!();
            }
            Err(why) => println!("  no  {} — {why}", entry.model_id),
        }
    }
    Ok(())
}

trait FitRowLabel {
    fn verdict_label(&self) -> &'static str;
}
impl FitRowLabel for kaspa_consensus_core::palw_model_fit_v1::PalwFitRowV1 {
    fn verdict_label(&self) -> &'static str {
        match self.verdict {
            kaspa_consensus_core::palw_model_fit_v1::PalwFitVerdictV1::Admitted => "Admitted",
            kaspa_consensus_core::palw_model_fit_v1::PalwFitVerdictV1::Refused => "Refused",
            kaspa_consensus_core::palw_model_fit_v1::PalwFitVerdictV1::Unpriced => "Unpriced",
        }
    }
}

pub(crate) async fn preflight(ctx: &Ctx, profile: Profile, artifact: PathBuf) -> CliResult {
    let object = local_registration_object(ctx, &profile, &artifact).await?;
    let bytes = borsh::to_vec(&object).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?;
    let nv = connect(ctx).await?;
    let resp = nv
        .client
        .get_palw_model_preflight(GetPalwModelPreflightRequest { object_hex: faster_hex::hex_string(&bytes), class_id: String::new() })
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelPreflight: {e}")))?;
    if ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&resp).expect("serializable"));
        return if resp.admissible { Ok(()) } else { Err(CliError::new(exit::MODEL, resp.reject_code)) };
    }
    println!("class_id           {}", resp.class_id);
    println!("artifact_root      {}", resp.artifact_root);
    println!("ctx / layers       {} / {}", resp.n_ctx, resp.layer_count);
    println!("admissible         {}  {}", tick(resp.admissible), resp.processor_verdict);
    if !resp.reject_code.is_empty() {
        println!("reject_code        {}", resp.reject_code);
    }
    for check in &resp.checks {
        println!("  {}  {}  {}", tick(check.ok), check.code, check.message);
    }
    if resp.admissible { Ok(()) } else { Err(CliError::new(exit::MODEL, resp.reject_code)) }
}

async fn local_registration_object(ctx: &Ctx, profile: &Profile, artifact: &Path) -> Result<PalwConsensusObjectV2, CliError> {
    let net = profile.network.parse::<kaspa_consensus_core::network::NetworkId>().map_err(|e| CliError::new(exit::CONFIG, e.to_string()))?;
    let params = kaspa_consensus_core::config::params::Params::from(net);
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        return Err(CliError::new(exit::CONFIG, format!("{net} has no PALW classes")));
    };
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), net.to_string().into_bytes());
    let loaded = sdk.load_artifact(artifact).map_err(|e| CliError::new(exit::MODEL, e))?;
    let pairings: Vec<_> = sdk.pairings(&loaded).into_iter().filter_map(|(e, p)| p.ok().map(|root| (e, root))).collect();
    let (entry, root) = pairings.into_iter().next().ok_or_else(|| CliError::new(exit::MODEL, "this artifact pairs with no class this build knows"))?;
    let nv = connect(ctx).await?;
    let terms_resp = nv.client.get_palw_registration_terms().await.map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwRegistrationTerms: {e}")))?;
    let (terms, _) = crate::operator::model_add::decode_terms(&terms_resp).map_err(|e| CliError::new(exit::GENERIC, e))?;
    let daa = nv.virtual_daa;
    let shape = palw_admission_shape_at_v1(&params, &bundle, &entry.profile, daa).map_err(|e| CliError::new(exit::MODEL, e))?;
    let dummy = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::Hash64::default(), 0));
    let candidate = misaka_palw_sdk::PalwRegistrationCandidateV1 { entry, artifact_root: root };
    sdk.build_post_genesis_registration(&bundle, &candidate, &terms, 0, dummy, Vec::new(), &shape)
        .map_err(|e| CliError::new(exit::MODEL, e))
}

/// **What `misaka model registration <id>` asks the node**, from the id, the object file it may
/// name, and the journal `model add` wrote when it submitted (one row per registration naming its
/// class, its object and its carrier together).
///
/// Two defects lived here (testnet-12, 2026-09-23). A 128-hex id was copied into all three slots
/// and the journal could only fill an EMPTY slot, so `registration <txid>` asked the node about a
/// class whose id was the carrier's — no row has that id — even when the journal named the real
/// class. And the journal matched by `text.contains(slot)`, which an empty slot satisfies for every
/// row, so `registration QWEN36` borrowed whichever registration's carrier the directory listed
/// first. A row now matches only by equality with one of its own ids, and a matching row supplies
/// all three.
pub(crate) fn registration_status_request(
    id: &str,
    object: Option<&[u8]>,
    journal: &[serde_json::Value],
) -> GetPalwModelRegistrationStatusRequest {
    let mut req = GetPalwModelRegistrationStatusRequest::default();
    if let Some(bytes) = object
        && let Ok(object) = borsh::from_slice::<PalwConsensusObjectV2>(bytes)
    {
        req.object_id = palw_registration_object_id_v1(bytes).to_string();
        if let PalwConsensusObjectV2::ClassRegistered { class_id, .. } = object {
            req.class_id = class_id.to_string();
        }
    }
    let field = |row: &serde_json::Value, name: &str| row.get(name).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let wanted = [id.trim().to_string(), req.class_id.clone(), req.object_id.clone()];
    let journaled = journal.iter().find(|row| {
        ["class_id", "object_id", "transaction_id"].iter().any(|name| {
            let have = field(row, name);
            !have.is_empty() && wanted.iter().any(|want| !want.is_empty() && have.eq_ignore_ascii_case(want))
        })
    });
    if let Some(row) = journaled {
        if req.class_id.is_empty() {
            req.class_id = field(row, "class_id");
        }
        if req.object_id.is_empty() {
            req.object_id = field(row, "object_id");
        }
        req.transaction_id = field(row, "transaction_id");
        return req;
    }
    if req.class_id.is_empty() && req.object_id.is_empty() {
        let bare = id.trim();
        if bare.len() >= 64 && bare.chars().all(|c| c.is_ascii_hexdigit()) {
            // A class id, an object id or a carrier id, and nothing here says which: asked as all
            // three, the node answers by its own record — a class id names its row, a carrier id
            // the row it wrote.
            req.class_id = bare.to_string();
            req.object_id = bare.to_string();
            req.transaction_id = bare.to_string();
        } else {
            req.class_id = bare.to_string();
        }
    }
    req
}

pub(crate) async fn registration(ctx: &Ctx, profile: Profile, id: String) -> CliResult {
    let nv = connect(ctx).await?;
    let object = if Path::new(&id).is_file() { std::fs::read(&id).ok() } else { None };
    let journal_dir = dirs::home_dir().unwrap_or_default().join(".misaka").join(&profile.network).join("model-add");
    let journal: Vec<serde_json::Value> = std::fs::read_dir(&journal_dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|ent| std::fs::read_to_string(ent.path().join("registration.json")).ok())
                .filter_map(|text| serde_json::from_str(&text).ok())
                .collect()
        })
        .unwrap_or_default();
    let req = registration_status_request(&id, object.as_deref(), &journal);
    let resp = nv
        .client
        .get_palw_model_registration_status(req)
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelRegistrationStatus: {e}")))?;
    if ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&resp).expect("serializable"));
        return Ok(());
    }
    println!("class_id     {}", resp.registration.class_id);
    println!("object_id    {}", resp.registration.object_id);
    println!("tx           {}", resp.registration.transaction_id);
    println!("state        {}", resp.registration.submission_state);
    if !resp.registration.processor_verdict.is_empty() {
        println!("verdict      {}", resp.registration.processor_verdict);
    }
    print_pipeline(&resp.registration);
    Ok(())
}

pub(crate) async fn status(ctx: &Ctx, profile: Profile, class_id: String) -> CliResult {
    let nv = connect(ctx).await?;
    let resp = nv
        .client
        .get_palw_model(GetPalwModelRequest { class_id: class_id.clone() })
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModel: {e}")))?;
    if !resp.available {
        return crate::operator::market::model_status(ctx, profile, &class_id).await;
    }
    if ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&resp).expect("serializable"));
        return Ok(());
    }
    if !resp.found {
        return Err(CliError::new(exit::MODEL, format!("class {class_id} is not on the class table — misaka model registration {class_id}")));
    }
    println!("class_id              {}", resp.class_id);
    println!("model                 {}  ctx {}", resp.model_name, resp.n_ctx);
    println!("class status          {}", resp.class_status);
    println!("registry              {}", resp.registry_state);
    println!("readySeats            {} / {}", resp.ready_seats, resp.required_ready_seats);
    println!("inflight              {}", resp.inflight_claims);
    println!("admission permille    {}", resp.admission_permille);
    println!("share                 {} ‰", resp.share_permille);
    println!("certified family      {}", if resp.certified_family.is_empty() { "none".into() } else { resp.certified_family });
    println!("fence                 {}", if resp.fence_active { "armed" } else { "closed / inactive" });
    if !resp.reason.is_empty() {
        println!("reason                {}", resp.reason);
    }
    Ok(())
}

pub(crate) async fn readiness(ctx: &Ctx, _profile: Profile, class_id: String) -> CliResult {
    let nv = connect(ctx).await?;
    let resp = nv
        .client
        .get_palw_model_readiness(GetPalwModelReadinessRequest { class_id })
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelReadiness: {e}")))?;
    if ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&resp).expect("serializable"));
        return Ok(());
    }
    println!("class_id         {}", resp.class_id);
    println!("registry         {}", resp.registry_state);
    println!("readySeats       {} / {}", resp.ready_seats, resp.required_ready_seats);
    if resp.ready_seats < resp.required_ready_seats {
        println!("code             {}", PalwModelRegistrationCodeV1::ReadySeatsInsufficient.code());
    }
    println!("{:<18}{:<10}{:<12}{:<12}{}", "SEAT", "READY", "PROVED", "EXPIRES", "REASON");
    for s in &resp.seats {
        println!(
            "{:<18}{:<10}{:<12}{:<12}{}",
            format!("{}:{}", &s.bond_txid[..s.bond_txid.len().min(8)], s.bond_index),
            if s.ready { "yes" } else { "no" },
            s.proved_daa,
            s.expires_daa,
            if s.ready { format!("collateral {}", s.collateral_sompi) } else { s.not_ready_reason.clone() }
        );
    }
    Ok(())
}

pub(crate) async fn certify(ctx: &Ctx, profile: Profile, class_id: String, yes: bool) -> CliResult {
    let nv = connect(ctx).await?;
    let cert = nv
        .client
        .get_palw_model_certification(GetPalwModelCertificationRequest { class_id: class_id.clone() })
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelCertification: {e}")))?;
    if cert.end_to_end_certified {
        if ctx.output == OutputFormat::Json {
            println!("{}", serde_json::json!({"ok": true, "already_certified": true, "class_id": class_id, "families": cert.families}));
        } else {
            println!("already covered by a chain family — this is not activation or a fence");
            for f in &cert.families {
                println!("  {}  {}  {}", f.lane, &f.digest[..f.digest.len().min(16)], if f.covers { "covers" } else { "" });
            }
        }
        return Ok(());
    }
    let net = profile.network.parse::<kaspa_consensus_core::network::NetworkId>().map_err(|e| CliError::new(exit::CONFIG, e.to_string()))?;
    let params = kaspa_consensus_core::config::params::Params::from(net);
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        return Err(CliError::new(exit::CONFIG, format!("{net} has no PALW classes")));
    };
    let sdk = misaka_palw_sdk::PalwClassSdk::builtin_v1(bundle.court, params.palw_prompt_ids_form_v1(), net.to_string().into_bytes());
    let class = palw_parse_or_err(&class_id)?;
    let entry = sdk.ledger().into_iter().find(|e| e.class_id() == class).ok_or_else(|| {
        CliError::new(exit::MODEL, "this class is not in this build's catalog — file a FamilyCertified with palw-certify")
    })?;
    let family = misaka_palw_base0::e2e_drill::covering_rc_family_v1(&entry.profile, kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt)
        .ok_or_else(|| CliError::new(exit::MODEL, "no family this build drills covers the class"))?;
    let evidence = misaka_palw_base0::e2e_drill::rc_attempt_evidence_v1(family).map_err(|e| CliError::new(exit::MODEL, format!("{e:?}")))?;
    let object = PalwConsensusObjectV2::FamilyCertified { evidence: Box::new(kaspa_consensus_core::palw_state_v2::PalwCertificationEvidenceV1::Attempt(evidence)) };
    let workdir = dirs::home_dir().unwrap_or_default().join(".misaka").join(&profile.network).join("model-certify");
    std::fs::create_dir_all(&workdir).map_err(|e| CliError::new(exit::HOST, e.to_string()))?;
    let path = workdir.join("family-certified.obj");
    std::fs::write(&path, borsh::to_vec(&object).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?)
        .map_err(|e| CliError::new(exit::HOST, e.to_string()))?;
    if !yes {
        println!("wrote {} — this files a FamilyCertified candidate; it does not arm a fence or activate the class", path.display());
        println!("re-run with --yes to submit");
        return Ok(());
    }
    let ks = crate::keys::KeySource {
        key_file: profile.key_path.as_ref().map(|p| p.display().to_string()),
        key_stdin: false,
    };
    crate::palw_fp::submit_objects(ctx, &ks, &[path], true).await?;
    Ok(())
}

fn palw_parse_or_err(raw: &str) -> Result<kaspa_consensus_core::Hash64, CliError> {
    kaspa_consensus_core::palw_panel_view_v1::palw_parse_class_alias_v1(raw).map_err(|e| CliError::new(exit::GENERIC, e))
}

pub(crate) fn write_registration_journal(workdir: &Path, class_id: &str, object_id: &str, txid: &str) {
    let _ = std::fs::create_dir_all(workdir);
    let _ = std::fs::write(
        workdir.join("registration.json"),
        serde_json::json!({"class_id": class_id, "object_id": object_id, "transaction_id": txid}).to_string(),
    );
}

pub(crate) async fn track_after_submit(
    client: &impl RpcApi,
    class_id: &str,
    object_id: &str,
    txid: &str,
    reject_text: Option<&str>,
) -> RpcPalwModelRegistration {
    let mut reg = client
        .get_palw_model_registration_status(GetPalwModelRegistrationStatusRequest {
            class_id: class_id.to_string(),
            object_id: object_id.to_string(),
            transaction_id: txid.to_string(),
        })
        .await
        .map(|r| r.registration)
        .unwrap_or_default();
    if reg.class_id.is_empty() {
        reg.class_id = class_id.to_string();
    }
    if reg.object_id.is_empty() {
        reg.object_id = object_id.to_string();
    }
    if !txid.is_empty() {
        reg.transaction_id = txid.to_string();
        reg.submitted = true;
        reg.constructed = true;
    }
    if let Some(text) = reject_text {
        if let Some(code) = palw_model_reject_from_submit_text_v1(text) {
            reg.reject_code = code.code().to_string();
            reg.processor_verdict = code.code().to_string();
            reg.accepted = false;
            reg.mempool_accepted = false;
        }
    }
    reg
}

#[cfg(test)]
mod registration_request_tests {
    use super::registration_status_request;

    fn journal_row(class: &str, object: &str, tx: &str) -> serde_json::Value {
        serde_json::json!({ "class_id": class, "object_id": object, "transaction_id": tx })
    }

    /// **`misaka model registration <txid>` asks about the class the carrier registered**
    /// (testnet-12, 2026-09-23): the carrier id went into the class slot and the journal, which
    /// names the class, was never allowed to replace it — so the node looked for a class whose id
    /// was the carrier's, found none, and read an included registration as not included.
    #[test]
    fn a_carrier_id_asks_about_the_class_it_registered() {
        let (class, object, tx) = ("c1".repeat(64), "0b".repeat(64), "27".repeat(64));
        let journal = vec![journal_row(&"aa".repeat(64), &"bb".repeat(64), &"cc".repeat(64)), journal_row(&class, &object, &tx)];
        let req = registration_status_request(&tx, None, &journal);
        assert_eq!(req.class_id, class, "the class the carrier registered, not the carrier id");
        assert_eq!(req.object_id, object);
        assert_eq!(req.transaction_id, tx);
        let by_class = registration_status_request(&class, None, &journal);
        assert_eq!((by_class.object_id, by_class.transaction_id), (object, tx), "a class id finds its carrier");
    }

    /// An empty slot matches no journal row: `text.contains("")` matched every one.
    #[test]
    fn an_alias_borrows_no_other_registrations_carrier() {
        let journal = vec![journal_row(&"aa".repeat(64), &"bb".repeat(64), &"cc".repeat(64))];
        let req = registration_status_request("QWEN36", None, &journal);
        assert_eq!(req.class_id, "QWEN36");
        assert!(req.object_id.is_empty() && req.transaction_id.is_empty(), "{req:?}");
    }

    /// With no journal row, a 128-hex id is asked as all three — the node resolves a carrier id to
    /// the row it wrote.
    #[test]
    fn an_unjournaled_id_is_asked_in_every_slot() {
        let id = "27".repeat(64);
        let req = registration_status_request(&id, None, &[]);
        assert_eq!(
            (req.class_id.as_str(), req.object_id.as_str(), req.transaction_id.as_str()),
            (id.as_str(), id.as_str(), id.as_str())
        );
    }
}
