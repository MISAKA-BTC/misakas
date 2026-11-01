//! Opt-in retained-artifact registration, one model per invocation. No model
//! execution, new conversion, shortened context, policy relaxation or credit.
//! All three cases use the SAME private isolated court2 Params; the two original
//! narrow programs still dispatch through v10. Run only under the parent grant.
use super::*;
use futures_util::FutureExt;
use kaspa_consensus_core::{mass::MassCalculator, tx::PopulatedTransaction};
use kaspa_mining::{
    AttestationMempoolPolicy, MAXIMUM_STANDARD_TRANSACTION_MASS, MiningCounters,
    manager::MiningManager,
    mempool::{
        config::PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE,
        tx::{Orphan, Priority, RbfPolicy},
    },
};
use misaka_palw_sdk::{
    lineages::tir::TirLineageV1,
    runtime_pack::build::sha256_file,
    tir_registration::{build_tir_registration_v1, tir_registration_message_v1},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    panic::{AssertUnwindSafe, catch_unwind},
    path::Path,
    sync::Arc,
    time::Instant,
};

const PINS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../testing/positive-court-a1/inputs/hf3-native-registration-pins-20261102.json"
));
const PINS_SHA256: &str = "b9b30c7909ff0dd8545e6e74eadb1c7edb20a4eb1d453802cffd4491dbe776fc";
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Failure {
    stage: &'static str,
    detail: String,
}
struct Report {
    slug: &'static str,
    started: Instant,
    stages: Vec<Value>,
    facts: serde_json::Map<String, Value>,
    current: &'static str,
}
impl Report {
    fn new(slug: &'static str) -> Self {
        Self { slug, started: Instant::now(), stages: vec![], facts: serde_json::Map::new(), current: "not_started" }
    }
    fn stage<T>(&mut self, name: &'static str, run: impl FnOnce() -> Result<T, String>) -> Result<T, Failure> {
        self.current = name;
        eprintln!("A1_HF3_STAGE {}", json!({"model":self.slug,"stage":name,"status":"started"}));
        let at = Instant::now();
        let result = catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|panic| Err(panic_message(panic)));
        self.finish_stage(name, at, result)
    }
    fn finish_stage<T>(&mut self, name: &'static str, at: Instant, result: Result<T, String>) -> Result<T, Failure> {
        let mut event = json!({"model":self.slug,"stage":name,"seconds":at.elapsed().as_secs_f64(),"status":if result.is_ok(){"passed"}else{"refused"}});
        if let Err(detail) = &result {
            event["detail"] = json!(detail);
        }
        eprintln!("A1_HF3_STAGE {event}");
        self.stages.push(event);
        result.map_err(|detail| Failure { stage: name, detail })
    }
}
fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "non-string panic".into())
}
fn require(condition: bool, detail: &str) -> Result<(), String> {
    if condition { Ok(()) } else { Err(detail.into()) }
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key].as_str().ok_or_else(|| format!("missing string {key}"))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("noncanonical hex metadata".into());
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string())).collect()
}
fn read_fixture(pin: &Value) -> Result<Value, String> {
    let path = Path::new(text(pin, "path")?);
    let cap = (4 * MAX_PROGRAM_BYTES) as u64;
    require(std::fs::metadata(path).map_err(|e| e.to_string())?.len() <= cap, "fixture exceeds bounded metadata cap")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path).map_err(|e| e.to_string())?.take(cap + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    require(bytes.len() as u64 <= cap, "fixture grew past metadata cap")?;
    require(digest(&bytes) == text(pin, "sha256")?, "fixture SHA256 differs")?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn check_file(pin: &Value) -> Result<(u64, String), String> {
    let path = Path::new(text(pin, "path")?);
    let before = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let (bytes, observed) = sha256_file(path)?;
    let after = std::fs::metadata(path).map_err(|e| e.to_string())?;
    require(observed == text(pin, "sha256")?, "whole-file SHA256 differs")?;
    require(
        before.len() == bytes && after.len() == bytes && before.modified().ok() == after.modified().ok(),
        "input changed while streaming SHA256",
    )?;
    if let Some(expected) = pin["bytes"].as_u64() {
        require(bytes == expected, "artifact byte count differs")?;
    }
    Ok((bytes, observed))
}
fn carrier_with_fee(env: &Env, object: &Obj, funding: &(TransactionOutpoint, UtxoEntry), fee: u64) -> Result<Transaction, String> {
    require(funding.1.amount > fee, "actual fee float cannot fund carrier and positive change")?;
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
        .map_err(|e| e.to_string())?;
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(funding.0, vec![], 0, 1)],
        vec![TransactionOutput::new(funding.1.amount - fee, card_payout_spk(CARD))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, funding.1.clone(), CARD, env.config.params.storage_mass_parameter);
    Ok(tx)
}
fn relay_fee(compute_mass: u64) -> Result<u64, String> {
    let rate = PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE;
    let fee = compute_mass.checked_mul(rate).ok_or("relay fee arithmetic overflow")? / 1000;
    Ok(if fee == 0 { rate } else { fee }.min(kaspa_consensus_core::constants::MAX_SOMPI))
}
fn mining_manager(env: &Env) -> MiningManager {
    MiningManager::new_with_extended_config(
        env.config.params.target_time_per_block(),
        false,
        env.config.params.max_block_mass,
        env.config.ram_scale,
        env.config.block_template_cache_lifetime,
        Arc::new(MiningCounters::default()),
        true,
        env.config.params.palw_model_market.is_some(),
        env.config.params.palw_activation_pool_fence().is_some(),
        None,
        AttestationMempoolPolicy::disabled(),
        kaspa_mining::mempool::config::palw_h1_carrier_priority_for(&env.config.params),
    )
}
async fn heartbeat(report: &mut Report, name: &'static str, env: &mut Env, txs: Vec<Transaction>) -> Result<Block, Failure> {
    report.current = name;
    eprintln!("A1_HF3_STAGE {}", json!({"model":report.slug,"stage":name,"status":"started"}));
    let at = Instant::now();
    let result = AssertUnwindSafe(env.chain.heartbeat(env.config.params.target_time_per_block(), txs))
        .catch_unwind()
        .await
        .map_err(panic_message);
    report.finish_stage(name, at, result)
}

async fn execute(report: &mut Report) -> Result<(), Failure> {
    let slug = report.slug;
    let pin = report.stage("cohort_pins", || {
        require(digest(PINS) == PINS_SHA256, "cohort input pins SHA256 differs")?;
        let all: Value = serde_json::from_slice(PINS).map_err(|e| e.to_string())?;
        all["models"]
            .as_array()
            .ok_or("cohort has no models")?
            .iter()
            .find(|m| m["slug"] == slug)
            .cloned()
            .ok_or_else(|| "model slug is not pinned".into())
    })?;
    report.facts.insert("input_pin".into(), pin.clone());
    report.facts.insert(
        "native_processor".into(),
        json!({"status":"NOT_RUN","reason":"SDK unsigned construction/full gate has not returned"}),
    );
    report.facts.insert("native_acceptance".into(), json!({"status":"NOT_RUN","reason":"no carrier block processed yet"}));
    let fixture = report.stage("fixture_identity", || read_fixture(&pin["fixture"]))?;
    let inventory = report.stage("prior_inventory_file_identity", || check_file(&pin["inventory"]))?;
    report.facts.insert("inventory_sha256".into(), json!(inventory.1));
    // Actual mapped-container inventory traversal is independent of the second
    // complete streaming SHA read below. Neither re-executes any model token.
    let entry =
        report.stage("retained_sdk_entry_and_inventory", || TirLineageV1::open_entry(Path::new(text(&pin["artifact"], "path")?)))?;
    let artifact = report.stage("whole_retained_artifact_sha256", || check_file(&pin["artifact"]))?;
    report.facts.insert("artifact_bytes_stream_hashed".into(), json!(artifact.0));
    report.stage("original_class_program_layout_profile", || {
        require(entry.class.program == unhex(text(&fixture, "program_borsh_hex")?)?, "actual program differs from pinned fixture")?;
        let layout = borsh::to_vec(&entry.class.layout).map_err(|e| e.to_string())?;
        require(layout == unhex(text(&fixture, "layout_borsh_hex")?)?, "actual layout differs from pinned fixture")?;
        require(digest(&entry.class.program) == text(&pin, "program_sha256")?, "actual program SHA256 differs")?;
        require(digest(&layout) == text(&pin, "layout_sha256")?, "actual layout SHA256 differs")?;
        require(
            entry.class_id().to_string() == text(&pin, "class_id")? && entry.class_id().to_string() == text(&fixture, "class_id_hex")?,
            "derived class ID differs",
        )?;
        require(
            entry.artifact_root.to_string() == text(&pin, "artifact_root")?
                && entry.artifact_root.to_string() == text(&fixture, "artifact_root_hex")?,
            "actual inventory root differs",
        )?;
        require(
            entry.class.tokenizer_id.to_string() == text(&pin, "tokenizer_id")?
                && entry.class.tokenizer_id.to_string() == text(&fixture, "tokenizer_id_hex")?,
            "original tokenizer identity differs",
        )?;
        require(
            fixture["artifact_bytes"].as_u64() == Some(artifact.0) && fixture["artifact_sha256"].as_str() == Some(artifact.1.as_str()),
            "fixture artifact identity differs",
        )?;
        let context = pin["original_max_context"].as_u64().ok_or("missing original context")?;
        require(
            u64::from(entry.class.layout.max_context) == context && fixture["max_context"].as_u64() == Some(context),
            "original full context differs",
        )?;
        require(
            entry.class.layout.checkpoint_interval == 65536 && entry.class.layout.h_tile == 64,
            "original checkpoint/history layout differs",
        )?;
        let program = entry.class.decode_program().map_err(|e| e.to_string())?;
        let output = &program.blocks[program.schedule.post as usize].nodes[program.logits as usize];
        match slug {
            "pythia-70m" => require(
                program.wide_logits_q24() && output.out.dtype == DType::I64 && context == 2048,
                "Pythia original wide final I64/profile/context differs",
            ),
            _ => require(
                program.logits_scheme_id == kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes()
                    && output.out.dtype == DType::I32
                    && context == 8192,
                "Smol original narrow profile/context differs",
            ),
        }
    })?;
    report.facts.insert(
        "identity_before".into(),
        json!({"class_id":entry.class_id().to_string(),
        "artifact_root":entry.artifact_root.to_string(),"tokenizer_id":entry.class.tokenizer_id.to_string(),
        "program_sha256":digest(&entry.class.program),"layout_sha256":digest(&borsh::to_vec(&entry.class.layout).unwrap()),
        "max_context":entry.class.layout.max_context,"checkpoint_interval":entry.class.layout.checkpoint_interval,
        "h_tile":entry.class.layout.h_tile}),
    );
    let (config, bundle, premine, floats) = report.stage("private_same_court2_params", || Ok(isolated_origin_config()))?;
    report.facts.insert(
        "chain_identity".into(),
        json!({"genesis":config.params.genesis.hash.to_string(),
        "params_id":config.params.consensus_params_id().to_string(),"schedule_id":config.params.consensus_schedule_id().to_string(),
        "court_version":config.params.palw_tir_v1.unwrap().court_version,"isolated_record_exemption":true}),
    );
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let (_db_lifetime, db) =
        report.stage("registration_database", || Ok(create_temp_db!(ConnBuilder::default().with_files_limit(10))))?;
    let mut env = report.stage("native_genesis_active_bond", || {
        let (sender, _rx) = async_channel::unbounded();
        let consensus = TestConsensus::with_db(db.clone(), &config, sender);
        let chain = t12_genesis_chain_on(consensus, &config, &bundle, &premine, &floats);
        let env = Env { chain, config, bundle, premine, floats };
        let (_, state) = env.chain.tip_state();
        require(
            matches!(state.bond(&env.chain.bonds[CARD]).ok_or("genesis registrant bond missing")?.status, PalwBondStatusV2::Active),
            "genesis registrant bond is not Active",
        )?;
        Ok(env)
    })?;
    heartbeat(report, "warm_block", &mut env, vec![]).await?;
    let (_, before) = env.chain.tip_state();
    let bond = env.chain.bonds[CARD];
    let collateral = before.bond(&bond).unwrap().collateral;
    let terms = report.stage("chain_registration_terms", || {
        env.chain.ctx.consensus.palw_v2_registration_terms().ok_or_else(|| "chain registration terms unavailable".into())
    })?;
    let daa = env.chain.daa_of(env.chain.sink());
    report.facts.insert(
        "registration_inputs".into(),
        json!({"active_genesis_bond":format!("{bond:?}"),
        "bond_registry_pubkey_sha256":digest(TestConsensus::palw_v2_registry_keypair(CARD as u64).verification_key.as_ref().as_ref()),
        "bond_collateral_sompi":collateral,"prior_registration_exposure":before.registration_exposure(&bond),
        "chain_target":terms.initial_target.to_string(),"slash_value_per_pwu":terms.slash_value_per_pwu,
        "served_min_grantable_share_permille":terms.min_grantable_share_permille,"sdk_protocol_share_permille":0,
        "selected_daa":daa,"activation_daa":daa+30,"registered_classes_count":terms.registered_class_ids.len(),
        "chain_certified_families_count":terms.chain_certified_families.len()}),
    );
    let mut object = report.stage("sdk_unsigned_build_and_selected_full_gate", || {
        build_tir_registration_v1(&env.config.params, &env.bundle, &entry, &terms, daa + 30, bond, vec![], daa)
    })?;
    // Only the excluded signature field is filled; the expensive SDK build/full
    // gate is not repeated and no signed class/canonical/price field is changed.
    report.stage("actual_registrant_mldsa_signature", || {
        let message = tir_registration_message_v1(env.domain(), &object).ok_or("SDK did not return tag61 signing message")?;
        let sig = libcrux_ml_dsa::ml_dsa_87::sign(
            &TestConsensus::palw_v2_registry_keypair(CARD as u64).signing_key,
            message.as_byte_slice(),
            PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
            [0x61; 32],
        )
        .map_err(|e| format!("{e:?}"))?;
        let Obj::ClassRegisteredTirV1 { admission, .. } = &mut object else { return Err("not a tag61 registration".into()) };
        admission.signature = sig.as_ref().to_vec();
        Ok(())
    })?;
    report.stage("signed_original_payload_identity", || {
        let Obj::ClassRegisteredTirV1 {
            class_id, artifact_root, share_permille, initial_target, slash_value_per_pwu, admission, ..
        } = &object
        else {
            return Err("SDK did not return tag61".into());
        };
        require(
            admission.class == *entry.class && *class_id == entry.class_id() && *artifact_root == entry.artifact_root,
            "signed original class/program/layout/tokenizer/root changed",
        )?;
        require(
            *share_permille == 0 && *initial_target == terms.initial_target && *slash_value_per_pwu == terms.slash_value_per_pwu,
            "signed chain terms or SDK protocol share changed",
        )?;
        require(
            admission.canonical == entry.canonical_context_v2(MAX_PROGRAM_BYTES)?,
            "signed original full canonical context changed",
        )
    })?;
    report.facts.insert(
        "signed_object".into(),
        json!({"borsh_sha256":digest(&borsh::to_vec(&object).unwrap()),
        "borsh_bytes":borsh::object_length(&object).unwrap(),
        "sdk_message_digest":tir_registration_message_v1(env.domain(),&object).unwrap().to_string(),
        "network_domain":env.domain().to_string()}),
    );
    let native_result = report.stage("actual_native_processor_validation", || {
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: env.chain.sink(),
            daa_score: daa,
            blue_score: before.last_point().map_or(1, |p| p.blue_score + 1),
            subsidy: 0,
        };
        env.chain.vp().palw_v2_validate_objects(&before, &env.bundle.state, &point, std::slice::from_ref(&object))
    });
    report.facts.insert(
        "native_processor".into(),
        match &native_result {
            Ok(()) => json!({"status":"VALIDATED","method":"palw_v2_validate_objects","selected_daa":daa,
            "scope":"actual native validator on current chain state; additional full-gate traversal, before carrier inclusion"}),
            Err(f) => {
                json!({"status":"REFUSED","method":"palw_v2_validate_objects","selected_daa":daa,"raw_error_code_and_text":f.detail})
            }
        },
    );
    native_result?;
    let class_id = entry.class_id();
    let expected_record = report.stage("expected_full_schema1_metadata", || {
        if entry.class.decode_program().map_err(|e| e.to_string())?.wide_logits_q24() {
            kaspa_consensus_core::palw_tir_class_record_v2::ProfiledTirClassRecordV2::build(
                &entry.class,
                entry.artifact_root,
                kaspa_consensus_core::palw_tir_class_record_v2::TirClassRecordProfileLimitsV2 {
                    max_program_bytes: MAX_PROGRAM_BYTES,
                    max_layout_entries: entry.class.layout.commit_tiles.len() + entry.class.layout.state_tiles.len(),
                },
            )
            .map(|p| p.into_storage_row())
            .map_err(|e| format!("{e:?}"))
        } else {
            kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(&entry.class, &entry.artifact_root)
                .map(|p| p.0)
                .map_err(|e| e.to_string())
        }
    })?;
    let funding = env.floats[CARD].clone();
    let carrier = report.stage("actual_funded_carrier_and_production_fee", || {
        let first = carrier_with_fee(&env, &object, &funding, FEE)?;
        let calculator = MassCalculator::new_with_consensus_params(&env.config.params);
        let fee = relay_fee(calculator.calc_non_contextual_masses(&first).compute_mass)?.max(FEE);
        let tx = carrier_with_fee(&env, &object, &funding, fee)?;
        let non = calculator.calc_non_contextual_masses(&tx);
        let storage = calculator
            .calc_contextual_masses(&PopulatedTransaction::new(&tx, vec![funding.1.clone()]))
            .ok_or("contextual mass unavailable")?;
        require(tx.mass() == storage.storage_mass, "signed storage-mass commitment differs")?;
        require(fee >= relay_fee(non.compute_mass)?, "final signed transaction underpays production relay fee")?;
        // Record all axes, then let the real standard-policy insertion report
        // its original error code/text if any axis exceeds the unchanged cap.
        Ok((tx, fee, non, storage))
    })?;
    let (carrier, fee, non, storage) = carrier;
    report.facts.insert(
        "carrier".into(),
        json!({"txid":carrier.id().to_string(),"payload_bytes":carrier.payload.len(),
        "compute_mass":non.compute_mass,"transient_mass":non.transient_mass,"storage_mass":storage.storage_mass,
        "existing_standard_mass_limit":MAXIMUM_STANDARD_TRANSACTION_MASS,
        "fee_sompi":fee,"production_relay_rate_sompi_per_1000_mass":PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE,
        "actual_input_sompi":funding.1.amount,"actual_change_sompi":carrier.outputs[0].value}),
    );
    let manager = report.stage("production_mining_manager_construction", || Ok(mining_manager(&env)))?;
    report.stage("production_mining_manager_standard_insertion", || {
        let consensus = env.chain.ctx.consensus.consensus_clone();
        let insertion = manager
            .validate_and_insert_transaction(
                consensus.as_ref(),
                carrier.clone(),
                Priority::High,
                Orphan::Forbidden,
                RbfPolicy::Forbidden,
            )
            .map_err(|e| e.to_string())?;
        require(insertion.accepted.iter().any(|tx| tx.id() == carrier.id()), "manager returned no accepted carrier")
    })?;
    let carrying = heartbeat(report, "standard_template_and_carrying_block", &mut env, vec![carrier.clone()]).await?;
    report.stage("actual_template_inclusion", || {
        require(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "Standard node template did not include carrier")
    })?;
    let folding = heartbeat(report, "accepting_block_and_native_fold", &mut env, vec![]).await?;
    let native_result = report.stage("accepted_utxo_and_native_full_row_origin", || {
        let accepted = env.chain.vp().acceptance_data_store.get(folding.header.hash).map_err(|e| e.to_string())?;
        require(
            accepted.iter().any(|b| {
                b.block_hash == carrying.header.hash && b.accepted_transactions.iter().any(|tx| tx.transaction_id == carrier.id())
            }),
            "actual carrier transaction not accepted",
        )?;
        let (_, state) = env.chain.tip_state();
        let row = match state.class(&class_id) {
            Some(row)=>row,
            None=>{
                let point=kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {block:folding.header.hash,
                    daa_score:env.chain.daa_of(folding.header.hash),blue_score:before.last_point().map_or(1,|p|p.blue_score+1),subsidy:0};
                let diagnostic=env.chain.vp().palw_v2_validate_objects(&before,&env.bundle.state,&point,std::slice::from_ref(&object));
                let fold_diagnostic=diagnostic.as_ref().ok().map(|_| env.chain.vp().palw_v2_fold_accepted_for_tests(
                    &before,&env.bundle.state,&point,std::slice::from_ref(&object)).map(|_|()).map_err(|e|e.to_string()));
                return Err(format!("accepted UTXO carrier produced no class; actual skip code unavailable; diagnostic recheck validator={diagnostic:?}, rehearsal={fold_diagnostic:?}"));
            }
        };
        require(state.tir_class_v1(&class_id) == Some(&expected_record), "full original schema1 row not written")?;
        require(
            row.artifact_root == entry.artifact_root && row.registrant_bond == Some(bond),
            "native class root/registrant mismatch",
        )?;
        require(
            matches!(row.status, PalwClassStatusV2::Registered { pending_share_permille: 0, .. }),
            "unexpected native class registration status",
        )?;
        require(
            state.class_target(&class_id).is_some_and(|t| t.target == terms.initial_target),
            "native class target differs from original chain terms",
        )?;
        require(
            state.bond(&bond).unwrap().collateral
                == collateral - kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1,
            "actual bond burn differs",
        )?;
        require(state.registration_exposure(&bond) > before.registration_exposure(&bond), "registration exposure not reserved")?;
        require(
            env.chain.ctx.consensus.get_virtual_utxo_entry(funding.0).is_none()
                && env.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier.id(), 0)).is_some(),
            "actual funding/change UTXO state differs",
        )?;
        let rows = env.chain.ctx.consensus.palw_v2_class_table();
        require(
            kaspa_consensus_core::palw_model_registration_v1::palw_registration_row_written_by_v1(&carrier, row.registered_daa, &rows)
                .is_some_and(|r| r.class_id == class_id),
            "registration status does not join actual carrier",
        )?;
        let origin = kaspa_consensus_core::palw_class_registration_origin_v1::palw_class_registration_origin_row_v1(
            state.kernel_route(),
            &class_id,
        )
        .ok_or("native aux141 registration origin missing")?;
        let Obj::ClassRegisteredTirV1 { admission, .. } = &object else { return Err("not tag61".into()) };
        require(
            origin.class_id == class_id
                && origin.block == folding.header.hash
                && origin.daa_score == row.registered_daa
                && origin.door == kaspa_consensus_core::palw_class_registration_origin_v1::PalwClassRegistrationDoorV1::Tir
                && origin.admission_digest
                    == Some(
                        kaspa_consensus_core::palw_class_registration_origin_v1::palw_class_admission_digest_v1(&**admission)
                            .map_err(|e| e.to_string())?,
                    ),
            "native origin block/DAA/door/signature-body identity differs",
        )?;
        require(admission.class==*entry.class,"signed original class changed after native acceptance")?;
        Ok(json!({"class_id":class_id.to_string(),"folding_block":folding.header.hash.to_string(),
            "registered_daa":row.registered_daa,"state_root":state.state_root().to_string(),"origin_aux141":true,
            "original_max_context":admission.class.layout.max_context,"claim_credit":false,
            "identity_after":{"program_sha256":digest(&expected_record.program),
                "layout_sha256":digest(&borsh::to_vec(&admission.class.layout).map_err(|e|e.to_string())?),
                "tokenizer_id":expected_record.tokenizer_id.to_string(),"class_id":state.tir_class_v1(&class_id).unwrap().facts.class_id.to_string(),
                "artifact_root":row.artifact_root.to_string(),"max_context":admission.class.layout.max_context}}))
    });
    report.facts.insert(
        "native_acceptance".into(),
        match &native_result {
            Ok(_) => {
                json!({"status":"FOLDED","folding_block":folding.header.hash.to_string(),"accepted_carrier":carrier.id().to_string()})
            }
            Err(f) => json!({"status":"REJECTED_OR_MISSING","actual_skip_code":"UNAVAILABLE","raw_diagnostic":f.detail}),
        },
    );
    let native_facts = native_result?;
    report.facts.insert("native_registration".into(), native_facts);
    let replay = report.stage("second_node_genesis", || Ok(t12_genesis_chain(&env.config, &env.bundle, &env.premine, &env.floats)))?;
    let blocks = report.stage("accepted_block_chain_inventory", || Ok(chain_blocks(&env.chain, env.chain.sink())))?;
    for block in &blocks {
        let at = Instant::now();
        let result = AssertUnwindSafe(arrive(&replay, block.clone(), "HF retained registration replay"))
            .catch_unwind()
            .await
            .map_err(panic_message);
        report.finish_stage("second_node_replay_block", at, result)?;
    }
    report.stage("second_node_replay_identity", || {
        require(replay.tip_state().1.state_root() == env.chain.tip_state().1.state_root(), "replayed native root differs")?;
        require(replay.tip_state().1.tir_class_v1(&class_id) == Some(&expected_record), "replayed full schema1 row differs")?;
        for block in &blocks {
            require(
                root_at(&replay, block.header.hash) == root_at(&env.chain, block.header.hash),
                "per-block persisted delta root differs",
            )?;
        }
        Ok(())
    })?;
    let (sink, original_state) = env.chain.tip_state();
    let expected_root = original_state.state_root();
    let expected_class = original_state
        .class(&class_id)
        .cloned()
        .ok_or_else(|| Failure { stage: "same_database_restore", detail: "original class missing before close".into() })?;
    let expected_origin = kaspa_consensus_core::palw_class_registration_origin_v1::palw_class_registration_origin_row_v1(
        original_state.kernel_route(),
        &class_id,
    );
    let nonce = env.chain.nonce_for_reopen();
    let time = env.chain.ctx.simulated_time;
    let bundle = env.bundle.clone();
    let mut resumed = env.config.clone();
    resumed.process_genesis = false;
    drop(env);
    report.stage("same_database_restore", || {
        let (sender, _rx2) = async_channel::unbounded();
        let second = TestConsensus::with_db(db.clone(), &resumed, sender);
        let reopened = t12_reopened_chain(second, &resumed, &bundle, time, nonce);
        let (tip, state) = reopened.tip_state();
        require(tip == sink && state.state_root() == expected_root, "same-DB native tip/root differs")?;
        require(
            state.class(&class_id) == Some(&expected_class) && state.tir_class_v1(&class_id) == Some(&expected_record),
            "same-DB original class/full schema1 row differs",
        )?;
        require(
            kaspa_consensus_core::palw_class_registration_origin_v1::palw_class_registration_origin_row_v1(
                state.kernel_route(),
                &class_id,
            ) == expected_origin,
            "same-DB origin aux141 differs",
        )?;
        require(
            reopened.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier.id(), 0)).is_some(),
            "same-DB actual carrier change UTXO missing",
        )?;
        for block in &blocks {
            require(
                root_at(&reopened, block.header.hash) == root_at(&replay, block.header.hash),
                "same-DB persisted delta root differs",
            )?;
        }
        Ok(())
    })?;
    Ok(())
}

async fn run(slug: &'static str) {
    let _serial = SERIAL.lock().await;
    let mut report = Report::new(slug);
    let result = AssertUnwindSafe(execute(&mut report))
        .catch_unwind()
        .await
        .unwrap_or_else(|panic| Err(Failure { stage: report.current, detail: panic_message(panic) }));
    let failure = result.as_ref().err().map(|f| json!({"stage":f.stage,"detail":f.detail}));
    let output = json!({"schema":"misaka.a1.hf3.native-registration-result.v1","model":slug,
        "result":if result.is_ok(){"native_registration_checked"}else{"refused"},"failure":failure,
        "seconds":report.started.elapsed().as_secs_f64(),"facts":report.facts,"stages":report.stages,
        "scope":"actual retained artifact identity, SDK build, actual bond/carrier signatures, production MiningManager standard insertion, Standard template, native registration fold, second-node replay and same-DB restore",
        "funding_scope":"genesis Active registrant bond and its existing backed allocation, plus a new real fee-float UTXO spend; SDK protocol share0; no dummy pure-gate registration",
        "not_performed":["new model inference/full original-context run","new conversion","claim certification or credit","public activation"]});
    eprintln!("A1_HF3_RESULT {output}");
    assert!(result.is_ok(), "HF3 model {slug} refused: {}", output["failure"]);
}

#[tokio::test]
#[ignore = "requires parent execution grant; retained model read, one model invocation at a time"]
async fn retained_smollm2_360m_native_registration() {
    run("smollm2-360m").await;
}
#[tokio::test]
#[ignore = "requires parent execution grant; retained model read, one model invocation at a time"]
async fn retained_smollm2_135m_native_registration() {
    run("smollm2-135m").await;
}
#[tokio::test]
#[ignore = "requires parent execution grant; retained model read, one model invocation at a time"]
async fn retained_pythia_70m_native_registration() {
    run("pythia-70m").await;
}
